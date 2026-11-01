//! First-party optional browser adapter registration and local status.
//!
//! Setup writes owner-reviewed local configuration and an immutable portable
//! identity package. It never starts Node, a browser, a preview or a provider
//! request. Exact activation remains a separate interactive owner decision.
use argentum_domain::{
    extensions::{ExtensionDefinition, ExtensionTransport, ExtensionTrust},
    Sha256Digest,
};
use argentum_extensions::package::{self, MCP_SCHEMA, PLUGIN_SCHEMA};
use argentum_platform::AppPaths;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

type Error = Box<dyn std::error::Error>;

pub(crate) const NAMESPACE: &str = "browser-control.stdio";
const PACKAGE_NAME: &str = "browser-control";
const ADAPTER_NAME: &str = "argentum-playwright-adapter";
const SOURCE_ID: &str = "first-party-argentum-browser";
const MAX_CONFIG_BYTES: u64 = 128 * 1024;
const MAX_ADAPTER_BYTES: u64 = 4 * 1024 * 1024;
const MAX_LICENSE_BYTES: u64 = 64 * 1024;
const MAX_REVIEWED_STYLESHEET_BYTES: u64 = 4 * 1024 * 1024;
const MAX_NODE_BYTES: u64 = 256 * 1024 * 1024;
const MAX_INPUT_BYTES: u64 = 256 * 1024 * 1024;
const MAX_ORIGINS: usize = 16;
const MAX_INPUTS: usize = 4096;
const STATUS_QUALIFICATION: &str = "Local definition readiness is not a live browser, connectivity, credential or execution check. Saved activation is not a running process. This command starts no Node process, browser or preview.";
// The adapter declares these eight tool names. Seven are browser operations;
// browser_preview_start starts the fixed owned static preview.
const BROWSER_TOOLS: [&str; 8] = [
    "browser_preview_start",
    "browser_open",
    "browser_observe",
    "browser_viewport",
    "browser_interact",
    "browser_screenshot",
    "browser_diagnostics",
    "browser_close",
];

pub(crate) struct BrowserRequest<'a> {
    pub flags: &'a BTreeMap<String, String>,
    pub origins: &'a [String],
    pub inputs: &'a [String],
    pub workspace: &'a Path,
    pub paths: &'a AppPaths,
    pub database: &'a Path,
    pub local: &'a argentum_cli::LocalCommandHandle,
}

pub(crate) async fn run(subcommand: &str, request: &BrowserRequest<'_>) -> Result<(), Error> {
    match subcommand {
        "setup" => setup(request).await,
        "status" => status(request.local),
        _ => Err("extensions browser requires setup or status; use extensions help".into()),
    }
}

async fn setup(request: &BrowserRequest<'_>) -> Result<(), Error> {
    let flags = request.flags;
    let source = absolute_existing_dir(
        required(flags, "--source")?,
        "browser setup requires --source DIR for extensions/browser",
    )?;
    let node = absolute_existing_file(
        required(flags, "--node")?,
        "browser setup requires --node FILE with the reviewed Node executable",
    )?;
    let origins = loopback_origins(request.origins)?;
    let identity = format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&(
            request.workspace,
            request.database.canonicalize()?
        ))?)
    );
    let cache = request.paths.data_dir.join("extensions").join(identity);
    std::fs::create_dir_all(&cache)?;
    argentum_execution::ensure_owner_only_directory(&cache)?;

    let artifact_directory = match flags.get("--artifact-directory") {
        Some(value) => absolute_path(value, "artifact directory must be an absolute path")?,
        None => cache.join("browser").join("artifacts"),
    };

    let preview = match (
        flags.get("--preview-workspace"),
        flags.get("--preview-port"),
    ) {
        (None, None) => None,
        (Some(workspace_value), Some(port_value)) => {
            let preview_workspace = absolute_existing_dir(
                workspace_value,
                "preview workspace must be an existing directory",
            )?;
            let port: u16 = port_value
                .parse()
                .map_err(|_| "preview port must be an integer from 1024 through 65535")?;
            if !(1024..=65535).contains(&port) {
                return Err("preview port must be an integer from 1024 through 65535".into());
            }
            let expected = format!("http://127.0.0.1:{port}");
            if !origins.iter().any(|origin| origin == &expected) {
                return Err(format!(
                    "preview port {port} requires an explicit --origin {expected}"
                )
                .into());
            }
            Some((preview_workspace, port))
        }
        _ => return Err("--preview-workspace and --preview-port must be provided together".into()),
    };

    let reviewed_stylesheet = match flags.get("--reviewed-stylesheet") {
        None => None,
        Some(relative) => {
            let (preview_workspace, _) = preview
                .as_ref()
                .ok_or("--reviewed-stylesheet requires --preview-workspace and --preview-port")?;
            Some(reviewed_stylesheet(preview_workspace, relative)?)
        }
    };

    let mut config = Map::new();
    config.insert("allowedOrigins".into(), json!(origins));
    config.insert(
        "artifactDirectory".into(),
        json!(artifact_directory.to_string_lossy()),
    );
    if let Some((preview_workspace, port)) = &preview {
        config.insert(
            "previewWorkspace".into(),
            json!(preview_workspace.to_string_lossy()),
        );
        config.insert("previewPort".into(), json!(port));
    }
    if let Some(style) = &reviewed_stylesheet {
        config.insert("reviewedStylesheet".into(), style.clone());
    }
    let config_bytes = serde_json::to_vec_pretty(&Value::Object(config))?;

    let config_path = cache.join("browser").join("browser.json");

    let adapter_path = source.join("adapter.mjs");
    let adapter_bytes = read_bounded(&adapter_path, MAX_ADAPTER_BYTES, "adapter.mjs")?;
    let adapter_digest = Sha256Digest(format!("{:x}", Sha256::digest(&adapter_bytes)));
    let manifest_path = source.join("package.json");
    let manifest_bytes = read_bounded(&manifest_path, MAX_ADAPTER_BYTES, "adapter package.json")?;
    let manifest: Value = serde_json::from_slice(&manifest_bytes)?;
    if manifest.get("name").and_then(Value::as_str) != Some(ADAPTER_NAME) {
        return Err("--source is not the first-party Playwright adapter".into());
    }
    let version = manifest
        .get("version")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty() && value.len() <= 64)
        .ok_or("adapter package.json version is missing or invalid")?;
    if manifest
        .pointer("/dependencies/playwright")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty)
    {
        return Err("adapter package.json must pin a playwright dependency".into());
    }
    let lock_path = source.join("package-lock.json");
    let lock_digest = match std::fs::symlink_metadata(&lock_path) {
        Ok(metadata) if metadata.file_type().is_file() => {
            Some(verified_digest(&lock_path, MAX_ADAPTER_BYTES)?)
        }
        Ok(_) => return Err("package-lock.json must be a regular file".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.into()),
    };
    let node_digest = verified_digest(&node, MAX_NODE_BYTES)?;

    let license_path = match flags.get("--license") {
        Some(value) => {
            absolute_existing_file(value, "--license must be an existing absolute file")?
        }
        None => default_license(&source)?,
    };
    let license_bytes = read_bounded(&license_path, MAX_LICENSE_BYTES, "license file")?;

    let adapter_argument = adapter_path.to_string_lossy().into_owned();
    let config_argument = config_path.to_string_lossy().into_owned();
    let args = vec![adapter_argument.clone(), config_argument.clone()];
    let configuration_digest = Sha256Digest(format!("{:x}", Sha256::digest(&config_bytes)));
    let manifest_digest = Sha256Digest(format!("{:x}", Sha256::digest(&manifest_bytes)));

    let staging = cache.join(format!(
        "pending-browser-{}-{}",
        std::process::id(),
        nanos()
    ));
    std::fs::create_dir(&staging)?;
    let package_result = (|| -> Result<package::PackageInspection, Error> {
        std::fs::write(
            staging.join("plugin.json"),
            serde_json::to_vec_pretty(&json!({
                "$schema": PLUGIN_SCHEMA,
                "name": PACKAGE_NAME,
                "version": version,
                "description": "First-party optional Playwright browser adapter for reviewed loopback local development.",
                "license": "SEE LICENSE IN LICENSE",
            }))?,
        )?;
        std::fs::write(staging.join("LICENSE"), &license_bytes)?;
        std::fs::write(
            staging.join("mcp.json"),
            serde_json::to_vec_pretty(&json!({
                "$schema": MCP_SCHEMA,
                "mcpServers": {
                    "stdio": {
                        "type": "stdio",
                        "command": node.to_string_lossy(),
                        "args": &args,
                    }
                },
            }))?,
        )?;
        std::fs::write(
            staging.join("browser-source.json"),
            serde_json::to_vec_pretty(&json!({
                "schema_version": 1,
                "adapter": {
                    "path": "adapter.mjs",
                    "sha256": &adapter_digest.0,
                    "bytes": adapter_bytes.len(),
                },
                "package_json_sha256": &manifest_digest.0,
                "package_lock_sha256": lock_digest.as_ref().map(|digest| &digest.0),
                "node_sha256": &node_digest.0,
                "configuration_sha256": &configuration_digest.0,
            }))?,
        )?;
        let inspected = package::inspect(&staging)?;
        package::install(&staging, &cache, &inspected.digest)?;
        Ok(inspected)
    })();
    let _ = std::fs::remove_dir_all(&staging);
    let inspected = package_result?;
    let package_root = cache.join(&inspected.digest);
    write_install_receipt(&cache, &inspected.digest, &node_digest, &adapter_digest)?;

    let mut inputs = BTreeMap::new();
    inputs.insert(adapter_argument.clone(), adapter_digest.clone());
    inputs.insert(config_argument.clone(), configuration_digest);
    inputs.insert(
        manifest_path.to_string_lossy().into_owned(),
        manifest_digest,
    );
    if let Some(digest) = lock_digest {
        inputs.insert(lock_path.to_string_lossy().into_owned(), digest);
    }
    for value in request.inputs {
        if inputs.len() >= MAX_INPUTS {
            return Err("too many browser dependency inputs".into());
        }
        let input = absolute_existing_file(value, "--input must be an existing absolute file")?;
        let digest = verified_digest(&input, MAX_INPUT_BYTES)?;
        inputs.insert(input.to_string_lossy().into_owned(), digest);
    }
    let mut environment = BTreeMap::new();
    if let Some(value) = flags.get("--browser-cache") {
        let browser_cache = absolute_existing_dir(
            value,
            "--browser-cache must be an existing absolute directory",
        )?;
        environment.insert(
            "ARGENTUM_PLAYWRIGHT_BROWSERS_PATH".to_owned(),
            browser_cache.to_string_lossy().into_owned(),
        );
    }

    let definition = ExtensionDefinition {
        namespace: NAMESPACE.to_owned(),
        package_root: package_root.to_string_lossy().into_owned(),
        package_sha256: Sha256Digest(inspected.digest.clone()),
        source_id: SOURCE_ID.to_owned(),
        source_revision: source_revision(&inspected.digest),
        transport: ExtensionTransport::Stdio {
            program: node.to_string_lossy().into_owned(),
            program_sha256: node_digest.clone(),
            args,
            cwd: request.workspace.to_string_lossy().into_owned(),
            environment,
            inputs,
        },
        allowed_tools: BROWSER_TOOLS
            .iter()
            .map(|name| (*name).to_owned())
            .collect(),
        resource_prefixes: Vec::new(),
        trust: ExtensionTrust::ReviewedCurrentUserCode,
        image_scope: None,
    };

    // Only touch owner-visible configuration after the reviewed adapter, Node,
    // license and immutable package identity have been validated.
    ensure_private_directory(&artifact_directory)?;
    let browser_directory = config_path
        .parent()
        .ok_or("configuration directory is unavailable")?;
    std::fs::create_dir_all(browser_directory)?;
    argentum_execution::ensure_owner_only_directory(browser_directory)?;
    write_private_file(&config_path, &config_bytes)?;

    let existing = request
        .local
        .extension_records()?
        .into_iter()
        .find(|record| record.definition.namespace == NAMESPACE);
    let unchanged = existing
        .as_ref()
        .is_some_and(|record| record.definition == definition);
    let record = match (unchanged, existing) {
        (true, Some(record)) => record,
        _ => request.local.configure_extension(definition).await?,
    };

    let document = json!({
        "schema_version": 1,
        "namespace": NAMESPACE,
        "changed": !unchanged,
        "saved_active": record.active,
        "definition_revision": record.revision,
        "package_root": &record.definition.package_root,
        "package_sha256": &record.definition.package_sha256.0,
        "source_id": &record.definition.source_id,
        "source_revision": &record.definition.source_revision,
        "node_entry_point": node.to_string_lossy(),
        "node_sha256": &node_digest.0,
        "adapter": adapter_argument,
        "adapter_sha256": &adapter_digest.0,
        "configuration": config_argument,
        "allowed_origins": origins,
        "artifact_directory": artifact_directory.to_string_lossy(),
        "preview": preview.as_ref().map(|(path, port)| json!({
            "workspace": path.to_string_lossy(),
            "port": port,
        })),
        "reviewed_stylesheet": reviewed_stylesheet,
        "allowed_tools": BROWSER_TOOLS,
        "execution_authorized": false,
        "process_started": false,
        "next": format!("review this definition, then run: argentum extensions activate {NAMESPACE} --workspace <workspace> --database <database>"),
    });
    println!("{}", serde_json::to_string_pretty(&document)?);
    if unchanged {
        println!(
            "Browser adapter configuration is unchanged. No Node process, browser or preview started."
        );
    } else {
        println!("Configuration saved inactive. No Node process, browser or preview started. Activation requires the local owner console.");
    }
    Ok(())
}

fn status(local: &argentum_cli::LocalCommandHandle) -> Result<(), Error> {
    let records = local.extension_records()?;
    let readiness = local.extension_readiness()?;
    let Some(record) = records
        .into_iter()
        .find(|record| record.definition.namespace == NAMESPACE)
    else {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "namespace": NAMESPACE,
                "configured": false,
                "execution_authorized": false,
                "process_started": false,
                "qualification": STATUS_QUALIFICATION,
            }))?
        );
        return Ok(());
    };
    let readiness = readiness
        .iter()
        .find(|row| row.get("namespace").and_then(Value::as_str) == Some(NAMESPACE));
    let ExtensionTransport::Stdio {
        program,
        args,
        inputs,
        environment,
        ..
    } = &record.definition.transport
    else {
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "namespace": NAMESPACE,
                "configured": true,
                "transport": "not_stdio",
                "execution_authorized": false,
                "process_started": false,
                "qualification": STATUS_QUALIFICATION,
            }))?
        );
        return Ok(());
    };
    let configuration = args
        .get(1)
        .map(PathBuf::from)
        .map(|path| {
            if path.is_absolute() {
                config_summary(&path)
            } else {
                json!({"state": "not_absolute"})
            }
        })
        .unwrap_or_else(|| json!({"state": "missing_configuration_argument"}));
    let configuration_summary = configuration.clone();
    let document = json!({
        "schema_version": 1,
        "namespace": NAMESPACE,
        "configured": true,
        "saved_active": record.active,
        "definition_revision": record.revision,
        "package_root": &record.definition.package_root,
        "package_sha256": &record.definition.package_sha256.0,
        "source_id": &record.definition.source_id,
        "source_revision": &record.definition.source_revision,
        "node_entry_point": program,
        "adapter": args.first(),
        "configuration": args.get(1),
        "allowed_origins": configuration.get("allowedOrigins"),
        "artifact_directory": configuration.get("artifactDirectory"),
        "preview_workspace": configuration.get("previewWorkspace"),
        "preview_port": configuration.get("previewPort"),
        "reviewed_stylesheet": configuration.get("reviewedStylesheet"),
        "configuration_summary": configuration_summary,
        "browser_cache": environment.get("ARGENTUM_PLAYWRIGHT_BROWSERS_PATH"),
        "dependency_inputs": inputs.len(),
        "allowed_tools": &record.definition.allowed_tools,
        "readiness": readiness,
        "execution_authorized": false,
        "process_started": false,
        "qualification": STATUS_QUALIFICATION,
    });
    println!("{}", serde_json::to_string_pretty(&document)?);
    Ok(())
}

fn required<'a>(flags: &'a BTreeMap<String, String>, name: &str) -> Result<&'a str, Error> {
    flags
        .get(name)
        .map(String::as_str)
        .ok_or_else(|| format!("browser setup requires {name}").into())
}

fn absolute_path(value: &str, message: &'static str) -> Result<PathBuf, Error> {
    let path = std::path::absolute(value).map_err(|_| message)?;
    if !path.is_absolute() {
        return Err(message.into());
    }
    Ok(path)
}

fn absolute_existing_dir(value: &str, message: &'static str) -> Result<PathBuf, Error> {
    let path = absolute_path(value, message)?;
    if !path.is_dir() {
        return Err(message.into());
    }
    Ok(path)
}

fn absolute_existing_file(value: &str, message: &'static str) -> Result<PathBuf, Error> {
    let path = absolute_path(value, message)?;
    if !path.is_file() {
        return Err(message.into());
    }
    Ok(path)
}

fn ensure_private_directory(path: &Path) -> Result<(), Error> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() => {
            let evidence = argentum_execution::owner_only_acl_evidence(path)?;
            if !evidence.protected
                || evidence.inherited_ace_count != 0
                || evidence.non_owner_allow_ace_count != 0
                || !evidence.owner_full_control
            {
                return Err("artifact directory must already be owner-only; prepare a dedicated private directory".into());
            }
            Ok(())
        }
        Ok(_) => Err("artifact directory path is not a directory".into()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            std::fs::create_dir_all(path)?;
            argentum_execution::ensure_owner_only_directory(path)?;
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

fn loopback_origins(values: &[String]) -> Result<Vec<String>, Error> {
    if values.is_empty() {
        return Err("browser setup requires at least one --origin http://127.0.0.1:PORT".into());
    }
    if values.len() > MAX_ORIGINS {
        return Err("at most 16 loopback origins are supported".into());
    }
    let mut unique = BTreeSet::new();
    let mut origins = Vec::new();
    for value in values {
        let origin = loopback_origin(value)?;
        if unique.insert(origin.clone()) {
            origins.push(origin);
        }
    }
    Ok(origins)
}

fn loopback_origin(value: &str) -> Result<String, Error> {
    let origin = argentum_extensions::http::endpoint_origin(value).map_err(|_| {
        "origin must be an exact http://127.0.0.1:PORT origin without a path, query, fragment or credentials"
    })?;
    if origin != value {
        return Err("origin must be an exact http://127.0.0.1:PORT origin without a trailing slash, path, query, fragment or credentials".into());
    }
    let port = value
        .strip_prefix("http://127.0.0.1:")
        .ok_or("origin must use literal 127.0.0.1 over http")?;
    if port.is_empty() || !port.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err("origin must use an explicit decimal loopback port".into());
    }
    let port: u16 = port
        .parse()
        .map_err(|_| "loopback origin port is out of range")?;
    if port == 0 {
        return Err("loopback origin port must be nonzero".into());
    }
    Ok(origin)
}

fn reviewed_stylesheet(preview_workspace: &Path, value: &str) -> Result<Value, Error> {
    let normalized = reviewed_stylesheet_path(value)?;
    let candidate = preview_workspace.join(&normalized);
    let canonical_root = preview_workspace.canonicalize()?;
    let canonical = candidate
        .canonicalize()
        .map_err(|_| "reviewed stylesheet is unavailable")?;
    if !canonical.starts_with(&canonical_root) {
        return Err("reviewed stylesheet must remain inside the preview workspace".into());
    }
    let bytes = read_bounded(
        &candidate,
        MAX_REVIEWED_STYLESHEET_BYTES,
        "reviewed stylesheet",
    )?;
    Ok(json!({
        "path": normalized,
        "sha256": format!("{:x}", Sha256::digest(&bytes)),
        "bytes": bytes.len(),
    }))
}

fn reviewed_stylesheet_path(value: &str) -> Result<String, Error> {
    if value.is_empty()
        || value.len() > 1024
        || value.contains('\\')
        || value.contains(':')
        || value.chars().any(char::is_control)
        || value
            .chars()
            .any(|character| matches!(character, '<' | '>' | '"' | '|' | '?' | '*'))
        || Path::new(value).is_absolute()
    {
        return Err(
            "reviewed stylesheet path must be a normalized preview-root-relative path".into(),
        );
    }
    let parts: Vec<&str> = value.split('/').collect();
    if parts.iter().any(|part| {
        part.is_empty()
            || part.starts_with('.')
            || part.ends_with(' ')
            || part.ends_with('.')
            || device_name(part)
    }) {
        return Err("reviewed stylesheet path must not contain empty, dot, device, or trailing-space components".into());
    }
    let Some((leaf, parents)) = parts.split_last() else {
        return Err("reviewed stylesheet path is empty".into());
    };
    if !is_token_stylesheet(leaf) {
        return Err("reviewed stylesheet leaf must be token.css or tokens.css".into());
    }
    if parents.iter().any(|part| private_component(part)) {
        return Err(
            "reviewed stylesheet parent components must not be credential, secret, or token names"
                .into(),
        );
    }
    Ok(parts.join("/"))
}

fn device_name(part: &str) -> bool {
    let stem = part
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    if matches!(stem.as_str(), "con" | "prn" | "aux" | "nul") {
        return true;
    }
    let prefix: String = stem.chars().take(3).collect();
    if prefix != "com" && prefix != "lpt" {
        return false;
    }
    let rest: Vec<char> = stem.chars().skip(3).collect();
    rest.len() == 1 && matches!(rest[0], '1'..='9' | '\u{b9}' | '\u{b2}' | '\u{b3}')
}

fn is_token_stylesheet(leaf: &str) -> bool {
    let leaf = leaf.to_ascii_lowercase();
    leaf == "token.css" || leaf == "tokens.css"
}

fn private_component(part: &str) -> bool {
    let part = part.to_ascii_lowercase();
    [
        "credential",
        "credentials",
        "secret",
        "secrets",
        "token",
        "tokens",
    ]
    .iter()
    .any(|name| {
        part.strip_prefix(name)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('.'))
    })
}

fn verified_digest(path: &Path, limit: u64) -> Result<Sha256Digest, Error> {
    let lease = argentum_execution::VerifiedFileLease::open(path, limit)?;
    Ok(Sha256Digest(lease.sha256().to_owned()))
}

fn read_bounded(path: &Path, limit: u64, label: &'static str) -> Result<Vec<u8>, Error> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.len() > limit {
        return Err(format!("{label} must be a bounded regular file").into());
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(format!("{label} exceeds its byte limit").into());
    }
    Ok(bytes)
}

fn default_license(source: &Path) -> Result<PathBuf, Error> {
    let local = source.join("LICENSE");
    if local.is_file() {
        return Ok(local);
    }
    if let Some(root) = source.parent().and_then(Path::parent) {
        let root = root.join("LICENSE");
        if root.is_file() {
            return Ok(root);
        }
    }
    Err("browser setup requires --license FILE when no LICENSE is next to the adapter or at the repository root".into())
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), Error> {
    let parent = path
        .parent()
        .ok_or("configuration directory is unavailable")?;
    let pending = parent.join(format!("pending-{}-{}", std::process::id(), nanos()));
    let result = (|| -> Result<(), Error> {
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&pending)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        argentum_execution::ensure_owner_only_file(&pending)?;
        std::fs::rename(&pending, path)?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&pending);
    }
    result
}

fn write_install_receipt(
    cache: &Path,
    package_digest: &str,
    node_digest: &Sha256Digest,
    adapter_digest: &Sha256Digest,
) -> Result<(), Error> {
    let receipt = json!({
        "schema_version": 1,
        "source_id": SOURCE_ID,
        "source_revision": source_revision(package_digest),
        "provenance": "owner_declared_not_upstream_signature_verified",
        "package_sha256": package_digest,
        "node_sha256": &node_digest.0,
        "adapter_sha256": &adapter_digest.0,
        "execution_authorized": false,
    });
    let path = cache.join(format!("{package_digest}.receipt.json"));
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
    {
        Ok(mut file) => {
            file.write_all(&serde_json::to_vec_pretty(&receipt)?)?;
            file.sync_all()?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            let retained: Value = serde_json::from_slice(&std::fs::read(&path)?)?;
            if retained != receipt {
                return Err("existing immutable package has different source provenance; preserve it and inspect the conflict".into());
            }
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

fn source_revision(package_digest: &str) -> String {
    format!("package-sha256:{package_digest}")
}

fn nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default()
}

fn config_summary(path: &Path) -> Value {
    let unavailable = |state: &'static str| json!({ "state": state });
    let metadata = match std::fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(_) => return unavailable("unavailable"),
    };
    if !metadata.file_type().is_file() || metadata.len() > MAX_CONFIG_BYTES {
        return unavailable("not_a_bounded_regular_file");
    }
    let mut bytes = Vec::new();
    if std::fs::File::open(path)
        .and_then(|file| file.take(MAX_CONFIG_BYTES + 1).read_to_end(&mut bytes))
        .is_err()
    {
        return unavailable("unreadable");
    }
    if bytes.len() as u64 > MAX_CONFIG_BYTES {
        return unavailable("not_a_bounded_regular_file");
    }
    let value: Value = match serde_json::from_slice(&bytes) {
        Ok(value) => value,
        Err(_) => return unavailable("invalid_json"),
    };
    let Some(object) = value.as_object() else {
        return unavailable("not_an_object");
    };
    let mut summary = Map::new();
    summary.insert("state".into(), json!("read"));
    if let Some(origins) = object.get("allowedOrigins").and_then(Value::as_array) {
        if origins.len() <= MAX_ORIGINS && origins.iter().all(Value::is_string) {
            summary.insert("allowedOrigins".into(), Value::Array(origins.clone()));
        }
    }
    for key in ["artifactDirectory", "previewWorkspace"] {
        if let Some(value) = object.get(key).and_then(Value::as_str) {
            summary.insert(key.into(), json!(value));
        }
    }
    if let Some(port) = object.get("previewPort").and_then(Value::as_u64) {
        summary.insert("previewPort".into(), json!(port));
    }
    if let Some(style) = object.get("reviewedStylesheet").and_then(Value::as_object) {
        let mut projected = Map::new();
        for key in ["path", "sha256", "bytes"] {
            if let Some(value) = style.get(key) {
                projected.insert(key.into(), value.clone());
            }
        }
        summary.insert("reviewedStylesheet".into(), Value::Object(projected));
    }
    Value::Object(summary)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loopback_origins_are_exact_and_deduplicated() {
        assert_eq!(
            loopback_origins(&[
                "http://127.0.0.1:3000".into(),
                "http://127.0.0.1:3000".into(),
            ])
            .unwrap(),
            vec!["http://127.0.0.1:3000"]
        );
        for rejected in [
            "https://127.0.0.1:3000",
            "http://localhost:3000",
            "http://127.0.0.1",
            "http://127.0.0.1:3000/",
            "http://user@127.0.0.1:3000",
            "http://127.0.0.1:3000/path",
            "http://127.0.0.1:0",
        ] {
            assert!(loopback_origins(&[rejected.into()]).is_err(), "{rejected}");
        }
    }

    #[test]
    fn reviewed_stylesheet_paths_are_exact_token_files() {
        assert_eq!(
            reviewed_stylesheet_path("styles/tokens.css").unwrap(),
            "styles/tokens.css"
        );
        assert_eq!(reviewed_stylesheet_path("token.css").unwrap(), "token.css");
        for rejected in [
            "../tokens.css",
            "/tokens.css",
            "C:/tokens.css",
            "styles\\tokens.css",
            "styles/../tokens.css",
            "styles/token.css.bak",
            "credentials/tokens.css",
            "styles/./tokens.css",
            "styles/tokens.css ",
            "con/tokens.css",
        ] {
            assert!(reviewed_stylesheet_path(rejected).is_err(), "{rejected}");
        }
    }

    #[test]
    fn browser_tool_scope_is_unique_and_matches_the_adapter() {
        let unique: BTreeSet<_> = BROWSER_TOOLS.iter().collect();
        assert_eq!(unique.len(), BROWSER_TOOLS.len());
        assert!(BROWSER_TOOLS.contains(&"browser_preview_start"));
        assert!(BROWSER_TOOLS.contains(&"browser_close"));
    }
}
