//! Optional local CLI commands. No install/configuration starts an extension.
use argentum_cli::{CommandHost, HostConfig};
use argentum_domain::extensions::ExtensionDefinition;
use argentum_extensions::package;
use argentum_platform::AppPaths;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    io::{self, IsTerminal, Read, Write},
    path::PathBuf,
};

type Error = Box<dyn std::error::Error>;

pub async fn run(values: Vec<String>) -> Result<(), Error> {
    if values.is_empty() || values[0] == "help" || values[0] == "--help" {
        println!("extensions inspect --source DIR\nextensions install --source DIR --sha256 DIGEST --source-id ID --revision REV --workspace DIR [--database FILE]\nextensions configure --file DEFINITION.json --workspace DIR [--database FILE]\nextensions list|status --workspace DIR [--database FILE]\nextensions activate|disable|remove NAMESPACE --workspace DIR [--database FILE]\nextensions key|prepare-media NAMESPACE --workspace DIR [--database FILE]\nConfiguration updates remove activation. Activation requires an interactive owner review; model/JSONL input cannot authorize it. The optional first-party local browser workflow is documented in docs/EXTENSIONS_BROWSER.md.");
        return Ok(());
    }
    let action = &values[0];
    let mut flags = BTreeMap::new();
    let mut subject = None;
    let mut index = 1;
    while index < values.len() {
        let value = &values[index];
        if value.starts_with("--") {
            if ![
                "--source",
                "--sha256",
                "--source-id",
                "--revision",
                "--workspace",
                "--database",
                "--file",
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
            "list" | "status" => {
                println!("{}", serde_json::to_string_pretty(&local.extension_records()?)?);
                if action == "status" { println!("{}",serde_json::to_string_pretty(&local.extension_readiness()?)?); }
                println!("Saved activation is not a connected server or evidence of successful execution.");
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
    let shutdown = host.shutdown().await;
    match (result, shutdown) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) => Err(error),
        (Ok(()), Err(error)) => Err(error.into()),
        (Err(error), Err(shutdown)) => Err(io::Error::other(format!(
            "extension command failed: {error}; cleanup failed: {shutdown}"
        ))
        .into()),
    }
}
