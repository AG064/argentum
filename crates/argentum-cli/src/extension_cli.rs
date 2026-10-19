//! Optional local CLI commands. No install/configuration starts an extension.
use argentum_cli::{CommandHost, HostConfig};
use argentum_domain::extensions::{ExtensionDefinition, ExtensionRecord};
use argentum_extensions::package;
use argentum_platform::AppPaths;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{self, IsTerminal, Read, Write},
    path::PathBuf,
};

type Error = Box<dyn std::error::Error>;

const ACTIVATION_QUALIFICATION: &str =
    "Saved activation is not a connected server or evidence of successful execution.";
// Store::extension_records and save_extension_record retain at most 32 compact
// 128 KiB records. Reserve the same budget for canonical readiness metadata and
// 4 KiB for the envelope. This bounds formatting, not the underlying inventory:
// an unexpectedly larger result fails as a whole instead of dropping records.
const QUERY_JSON_MAX_BYTES: usize = 2 * 32 * 128 * 1024 + 4096;

#[derive(Serialize)]
struct ExtensionQueryDocument<'a> {
    schema_version: u8,
    command: &'static str,
    records: &'a [ExtensionRecord],
    #[serde(skip_serializing_if = "Option::is_none")]
    readiness: Option<&'a [serde_json::Value]>,
    qualification: QueryQualification,
}

#[derive(Serialize)]
struct QueryQualification {
    activation: &'static str,
    readiness: &'static str,
    host_lifecycle: &'static str,
    observation: &'static str,
}

struct QueryBuffer {
    bytes: Vec<u8>,
    limit: usize,
}

impl Write for QueryBuffer {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        if input.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(io::Error::other("extension query JSON exceeds byte limit"));
        }
        self.bytes
            .try_reserve(input.len())
            .map_err(io::Error::other)?;
        self.bytes.extend_from_slice(input);
        Ok(input.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

fn encode_query_json<T: Serialize>(value: &T, limit: usize) -> Result<Vec<u8>, Error> {
    let mut output = QueryBuffer {
        bytes: Vec::new(),
        limit: limit
            .checked_sub(1)
            .ok_or("extension query JSON byte limit is zero")?,
    };
    serde_json::to_writer(&mut output, value)?;
    output.bytes.try_reserve(1).map_err(io::Error::other)?;
    output.bytes.push(b'\n');
    Ok(output.bytes)
}

fn finish_extension_command(
    result: Result<(), Error>,
    shutdown: Result<(), Error>,
    buffered_json: Option<&[u8]>,
    output: &mut impl Write,
) -> Result<(), Error> {
    match (result, shutdown) {
        (Ok(()), Ok(())) => {
            if let Some(bytes) = buffered_json {
                output.write_all(bytes)?;
                output.flush()?;
            }
            Ok(())
        }
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Err(error), Err(shutdown)) => Err(io::Error::other(format!(
            "extension command failed: {error}; cleanup failed: {shutdown}"
        ))
        .into()),
    }
}

pub async fn run(values: Vec<String>) -> Result<(), Error> {
    if values.is_empty() || values[0] == "help" || values[0] == "--help" {
        if values.iter().any(|value| value == "--json") {
            return Err("--json is accepted only by extensions list and status".into());
        }
        println!("extensions inspect --source DIR\nextensions install --source DIR --sha256 DIGEST --source-id ID --revision REV --workspace DIR [--database FILE]\nextensions configure --file DEFINITION.json --workspace DIR [--database FILE]\nextensions list|status --workspace DIR [--database FILE] [--json]\nextensions receipts [--run-id ID] --workspace DIR [--database FILE]\nextensions activate|disable|remove NAMESPACE --workspace DIR [--database FILE]\nextensions key|prepare-media NAMESPACE --workspace DIR [--database FILE]\nConfiguration updates remove activation. Activation requires an interactive owner review; model/JSONL input cannot authorize it. The optional first-party local browser workflow is documented in docs/EXTENSIONS_BROWSER.md.");
        return Ok(());
    }
    let action = &values[0];
    let mut flags = BTreeMap::new();
    let mut subject = None;
    let mut json_mode = false;
    let mut index = 1;
    while index < values.len() {
        let value = &values[index];
        if value == "--json" {
            if json_mode {
                return Err("duplicate extension --json option".into());
            }
            json_mode = true;
            index += 1;
            continue;
        }
        if value.starts_with("--") {
            if ![
                "--source",
                "--sha256",
                "--source-id",
                "--revision",
                "--workspace",
                "--database",
                "--file",
                "--run-id",
            ]
            .contains(&value.as_str())
            {
                return Err("unknown extension option; use extensions help".into());
            }
            index += 1;
            let argument = values
                .get(index)
                .ok_or("extension option requires a value")?;
            if flags.insert(value.clone(), argument.clone()).is_some() {
                return Err("duplicate extension option".into());
            }
        } else if subject.replace(value.clone()).is_some() {
            return Err("only one namespace is accepted".into());
        }
        index += 1;
    }
    if json_mode && !matches!(action.as_str(), "list" | "status") {
        return Err("--json is accepted only by extensions list and status".into());
    }
    if json_mode && subject.is_some() {
        return Err("extension JSON queries do not accept a namespace".into());
    }
    if flags.contains_key("--run-id") && action != "receipts" {
        return Err("--run-id is accepted only by extensions receipts".into());
    }
    if action == "inspect" {
        let source = PathBuf::from(
            flags
                .get("--source")
                .ok_or("inspect requires --source DIR")?,
        );
        println!(
            "{}",
            serde_json::to_string_pretty(&package::inspect(&source)?)?
        );
        return Ok(());
    }
    let workspace = PathBuf::from(
        flags
            .get("--workspace")
            .ok_or("extensions commands require an explicit --workspace")?,
    )
    .canonicalize()?;
    let paths = AppPaths::discover()?;
    // Do not read credential helpers or contact a model/server during extension setup.
    let mut config = HostConfig::discover_local_only(&workspace)?;
    config = config.with_extension_secret_store(argentum_platform::default_secret_store(
        argentum_platform::SECRET_SERVICE,
    ));
    if let Some(database) = flags.get("--database") {
        config.database = Some(PathBuf::from(database));
    }
    let database = config
        .database
        .clone()
        .ok_or("extension persistence requires a database")?;
    let mut host = CommandHost::start_composed(config).await?;
    host.disconnect_approval_responder();
    let mut buffered_json = None;
    let result: Result<(), Error> = async {
        let local = host.local_handle();
        match action.as_str() {
            "install" => {
                let source = PathBuf::from(flags.get("--source").ok_or("install requires --source")?);
                let digest = flags.get("--sha256").ok_or("install requires explicit --sha256 from inspect")?;
                let source_id = flags.get("--source-id").ok_or("install requires --source-id")?;
                let revision = flags.get("--revision").ok_or("install requires --revision")?;
                if source_id.len() > 2048 || revision.len() > 256 || source_id.is_empty() || revision.is_empty() { return Err("invalid immutable source/revision declaration".into()); }
                let identity = format!("{:x}", Sha256::digest(serde_json::to_vec(&(&workspace, database.canonicalize()?))?));
                let cache = paths.data_dir.join("extensions").join(identity);
                std::fs::create_dir_all(&cache)?;
                argentum_execution::ensure_owner_only_directory(&cache)?;
                let inspected = package::install(&source, &cache, digest)?;
                let receipt = serde_json::json!({"schema_version":1,"source_id":source_id,"source_revision":revision,"provenance":"owner_declared_not_upstream_signature_verified","package_sha256":inspected.digest,"execution_authorized":false});
                let receipt_path = cache.join(format!("{}.receipt.json", inspected.digest));
                match std::fs::OpenOptions::new().write(true).create_new(true).open(&receipt_path) {
                    Ok(mut file) => { file.write_all(&serde_json::to_vec_pretty(&receipt)?)?; file.sync_all()?; }
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                        let retained: serde_json::Value = serde_json::from_slice(&std::fs::read(&receipt_path)?)?;
                        if retained != receipt { return Err("existing immutable package has different source provenance; preserve it and inspect the conflict".into()); }
                    }
                    Err(error) => return Err(error.into()),
                }
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({"receipt":receipt,"package_root":cache.join(&inspected.digest),"next":"configure an exact reviewed definition; installation is not execution approval"}))?);
            }
            "configure" => {
                let file = PathBuf::from(flags.get("--file").ok_or("configure requires --file")?);
                let mut bytes = Vec::new(); std::fs::File::open(file)?.take(128 * 1024 + 1).read_to_end(&mut bytes)?;
                if bytes.len() > 128 * 1024 { return Err("extension definition exceeds byte limit".into()); }
                let definition: ExtensionDefinition = serde_json::from_slice(&bytes)?;
                println!("{}", serde_json::to_string_pretty(&local.configure_extension(definition).await?)?);
                println!("Configuration saved inactive. No server, browser, catalogue or model request started.");
            }
            "prepare-media" => {
                let namespace = subject.as_deref().ok_or("prepare-media requires a namespace")?;
                if namespace.len()>129 || namespace.split('.').count()!=2 || namespace.split('.').any(|part| !package::valid_id(part)) { return Err("invalid namespace".into()); }
                let identity = format!("{:x}", Sha256::digest(serde_json::to_vec(&(&workspace, database.canonicalize()?))?));
                let media = paths.data_dir.join("extensions").join(identity).join("media").join(namespace);
                std::fs::create_dir_all(&media)?;
                argentum_execution::ensure_owner_only_directory(&media)?;
                println!("{}", serde_json::to_string_pretty(&serde_json::json!({"directory":media,"owner_only":true,"execution_authorized":false,"profile_state_exported":false}))?);
            }
            "receipts" => {
                let run = flags.get("--run-id").map(|value| argentum_domain::RunId::parse_str(value))
                    .transpose().map_err(|_| "invalid receipt run ID")?;
                println!("{}", serde_json::to_string_pretty(&local.extension_receipts(run)?)?);
            }
            "list" | "status" => {
                if json_mode {
                    let records = local.extension_records()?;
                    let readiness = if action == "status" { Some(local.extension_readiness()?) } else { None };
                    let document = ExtensionQueryDocument {
                        schema_version: 1,
                        command: if action == "list" { "extensions.list" } else { "extensions.status" },
                        records: &records,
                        readiness: readiness.as_deref(),
                        qualification: QueryQualification {
                            activation: ACTIVATION_QUALIFICATION,
                            readiness: "Local definition readiness is not live connectivity or account entitlement; credential presence is not inspected.",
                            host_lifecycle: "Normal local host discovery, startup bookkeeping and shutdown still occur; this is not an inert database reader or permission ticket.",
                            observation: "Native records and readiness are sequential observations, not an atomic cross-source snapshot.",
                        },
                    };
                    buffered_json = Some(encode_query_json(&document, QUERY_JSON_MAX_BYTES)?);
                } else {
                    println!("{}", serde_json::to_string_pretty(&local.extension_records()?)?);
                    if action == "status" { println!("{}",serde_json::to_string_pretty(&local.extension_readiness()?)?); }
                    println!("{ACTIVATION_QUALIFICATION}");
                }
            }
            "activate" => {
                let namespace = subject.as_deref().ok_or("activate requires a namespace")?;
                let record = local.extension_records()?.into_iter().find(|record| record.definition.namespace == namespace).ok_or("extension namespace is not configured")?;
                println!("{}", serde_json::to_string_pretty(&record.definition)?);
                println!("Review package source/license, exact executable/dependencies, allowed tools and destinations. Local executable servers have current-user filesystem/network access. Job limits are not an OS access sandbox. Installation and tool annotations cannot approve this code.");
                if !io::stdin().is_terminal() || !io::stdout().is_terminal() { return Err("owner approval requires a real interactive local console; no activation recorded".into()); }
                print!("Type ACTIVATE to approve this exact definition: "); io::stdout().flush()?;
                let mut answer = String::new(); io::stdin().read_line(&mut answer)?;
                if answer.trim() != "ACTIVATE" { return Err("extension activation declined; no process started".into()); }
                let record = local.activate_extension(namespace).await?;
                println!("{}", serde_json::to_string_pretty(&record)?);
                println!("Exact owner activation saved. Processes start only when admitted work invokes this connection.");
            }
            "key" => {
                let namespace = subject.as_deref().ok_or("key requires a configured namespace")?;
                if !io::stdin().is_terminal() || !io::stdout().is_terminal() { return Err("secure extension key entry requires the local owner console".into()); }
                let secret = argentum_cli::tui::prompt_provider_secret("extension service")?;
                local.set_extension_secret(namespace, secret).await?;
                println!("Secure service key saved under its exact project/namespace/origin/account binding. Activation is revoked; review and activate this connection again.");
            }
            "disable" | "remove" => {
                local.disable_extension(subject.as_deref().ok_or("namespace required")?, action == "remove").await?;
                println!("Extension admission revoked and owned work drained. Removal drops its registration; immutable installation evidence is retained.");
            }
            _ => return Err("unsupported extension command; use extensions help".into()),
        }
        Ok(())
    }.await;
    let shutdown = host
        .shutdown()
        .await
        .map_err(|error| -> Error { error.into() });
    finish_extension_command(
        result,
        shutdown,
        buffered_json.as_deref(),
        &mut io::stdout().lock(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    fn failure(message: &str) -> Error {
        io::Error::other(message.to_owned()).into()
    }

    #[test]
    fn query_encoding_is_one_complete_document_with_a_counted_newline() {
        let value = serde_json::json!({"name":"Märk \"quoted\"\nC:\\fixture","records":[]});
        let expected = serde_json::to_vec(&value).unwrap();
        let encoded = encode_query_json(&value, expected.len() + 1).unwrap();
        assert_eq!(encoded.len(), expected.len() + 1);
        assert_eq!(encoded.last(), Some(&b'\n'));
        let mut parser = serde_json::Deserializer::from_slice(&encoded);
        assert_eq!(serde_json::Value::deserialize(&mut parser).unwrap(), value);
        parser.end().unwrap();
        assert!(encode_query_json(&value, expected.len()).is_err());
        assert!(encode_query_json(&value, 0).is_err());
    }

    #[test]
    fn actual_query_budget_accepts_its_boundary_and_rejects_overflow() {
        let fits = "x".repeat(QUERY_JSON_MAX_BYTES - 3);
        assert_eq!(
            encode_query_json(&fits, QUERY_JSON_MAX_BYTES)
                .unwrap()
                .len(),
            QUERY_JSON_MAX_BYTES
        );
        let overflow = format!("{fits}x");
        assert!(encode_query_json(&overflow, QUERY_JSON_MAX_BYTES).is_err());
    }

    #[test]
    fn native_serialization_and_shutdown_failures_emit_no_success_document() {
        struct Reject;
        impl Serialize for Reject {
            fn serialize<S: serde::Serializer>(&self, _: S) -> Result<S::Ok, S::Error> {
                Err(serde::ser::Error::custom("synthetic serialization failure"))
            }
        }
        let serialized = encode_query_json(&Reject, QUERY_JSON_MAX_BYTES).unwrap_err();
        let mut output = Vec::new();
        assert!(finish_extension_command(Err(serialized), Ok(()), None, &mut output).is_err());
        assert!(output.is_empty());
        let bytes = b"{\"records\":[]}\n";
        for (operation_failed, shutdown_failed) in [(true, false), (false, true), (true, true)] {
            let result = if operation_failed {
                Err(failure("native operation failure"))
            } else {
                Ok(())
            };
            let shutdown = if shutdown_failed {
                Err(failure("native shutdown failure"))
            } else {
                Ok(())
            };
            let error =
                finish_extension_command(result, shutdown, Some(bytes), &mut output).unwrap_err();
            assert!(output.is_empty());
            if operation_failed {
                assert!(error.to_string().contains("native operation failure"));
            }
            if shutdown_failed {
                assert!(error.to_string().contains("native shutdown failure"));
            }
        }
        finish_extension_command(Ok(()), Ok(()), Some(bytes), &mut output).unwrap();
        assert_eq!(output, bytes);
    }

    #[test]
    fn partial_stdout_delivery_is_an_io_failure_not_success() {
        struct BrokenPipe(Vec<u8>);
        impl Write for BrokenPipe {
            fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
                let count = bytes.len().min(3_usize.saturating_sub(self.0.len()));
                if count == 0 {
                    return Err(io::Error::new(
                        io::ErrorKind::BrokenPipe,
                        "synthetic closed stdout",
                    ));
                }
                self.0.extend_from_slice(&bytes[..count]);
                Ok(count)
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }
        let mut pipe = BrokenPipe(Vec::new());
        let error =
            finish_extension_command(Ok(()), Ok(()), Some(b"{\"records\":[]}\n"), &mut pipe)
                .unwrap_err();
        assert_eq!(
            error.downcast_ref::<io::Error>().unwrap().kind(),
            io::ErrorKind::BrokenPipe
        );
        assert_eq!(pipe.0.len(), 3);
    }
}
