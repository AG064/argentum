//! First-party optional browser adapter registration and local status.
//!
//! Setup writes owner-reviewed local configuration and an immutable portable
//! identity package. It never starts Node, a browser, a preview or a provider
//! request. Exact activation remains a separate interactive owner decision.
use crate::local_readiness::{
    browser_cache_report, file_detail, file_hash, file_report, module_detail, module_report,
    read_bounded, stream_hash, BrowserCacheReport, FileHash, FileReport, ModuleReport,
};
use argentum_domain::{
    extensions::{ExtensionDefinition, ExtensionTransport, ExtensionTrust},
    Sha256Digest,
};
use argentum_execution::{ProcessRunner, ProcessSpec};
use argentum_extensions::configuration::{admit, AdmittedDefinition};
use argentum_extensions::connection::McpSession;
use argentum_extensions::package::{self, MCP_SCHEMA, PLUGIN_SCHEMA};
use argentum_platform::AppPaths;
use serde::Serialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio_util::sync::CancellationToken;

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
    pub json: bool,
    pub handshake: bool,
    pub install: bool,
    pub confirm_network: bool,
}

pub(crate) async fn run(subcommand: &str, request: &BrowserRequest<'_>) -> Result<(), Error> {
    match subcommand {
        "setup" => setup(request).await,
        "status" => status(request.local),
        "check" => check(request).await,
        "prepare" => prepare(request).await,
        _ => Err(
            "extensions browser requires setup, status, check, or prepare; use extensions help"
                .into(),
        ),
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

#[derive(Debug, Serialize)]
struct BrowserCheckReport {
    schema_version: u32,
    namespace: &'static str,
    configured: bool,
    handshake_requested: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<crate::json_contract::JsonErrorBody>,
    node: NodeReport,
    files: FilesReport,
    configuration: Value,
    dependencies: DependenciesReport,
    definition: DefinitionReport,
    checks: Vec<CheckReport>,
    ready: bool,
    handshake: HandshakeReport,
    qualification: &'static str,
}

#[derive(Debug, Default, Serialize)]
struct NodeReport {
    path: Option<String>,
    exists: bool,
    sha256: Option<String>,
    version: Option<String>,
    version_status: &'static str,
    error: Option<String>,
}

#[derive(Debug, Default, Serialize)]
struct FilesReport {
    adapter: FileReport,
    adapter_package: FileReport,
    package_lock: FileReport,
    config: FileReport,
    manifest: FileReport,
}

#[derive(Debug, Default, Serialize)]
struct DependenciesReport {
    playwright: ModuleReport,
    playwright_core: ModuleReport,
    browser_cache: BrowserCacheReport,
}

#[derive(Debug, Default, Serialize)]
struct DefinitionReport {
    configured: bool,
    active: bool,
    revision: Option<String>,
    package_sha256: Option<String>,
    package_root: Option<String>,
    allowed_tools: Vec<String>,
    readiness: Option<Value>,
}

#[derive(Debug, Serialize)]
struct CheckReport {
    id: &'static str,
    required: bool,
    ok: bool,
    detail: String,
    next_action: Option<String>,
}

#[derive(Debug, Default, Serialize)]
struct HandshakeReport {
    requested: bool,
    attempted: bool,
    ok: Option<bool>,
    tools: Vec<String>,
    error: Option<String>,
    duration_ms: Option<u128>,
}

const CHECK_QUALIFICATION: &str = "Dependency readiness is a local filesystem observation. The default check starts no Node process, adapter, browser, preview, provider request or network connection. --handshake starts the reviewed adapter only, lists its MCP tools and shuts it down; it does not open a page, navigate or start a preview. A saved activation is not a running process.";

async fn build_check_report(request: &BrowserRequest<'_>) -> Result<BrowserCheckReport, Error> {
    let records = request.local.extension_records()?;
    let readiness_rows = request.local.extension_readiness()?;
    let record = records
        .into_iter()
        .find(|record| record.definition.namespace == NAMESPACE);

    let mut checks = Vec::new();
    let mut report = BrowserCheckReport {
        schema_version: 1,
        namespace: NAMESPACE,
        configured: record.is_some(),
        handshake_requested: request.handshake,
        error: None,
        node: NodeReport::default(),
        files: FilesReport::default(),
        configuration: json!({"state": "unavailable"}),
        dependencies: DependenciesReport::default(),
        definition: DefinitionReport::default(),
        checks: Vec::new(),
        ready: false,
        handshake: HandshakeReport {
            requested: request.handshake,
            ..HandshakeReport::default()
        },
        qualification: CHECK_QUALIFICATION,
    };

    let Some(record) = record else {
        checks.push(check_row(
            "definition",
            true,
            false,
            "browser-control.stdio is not installed for this workspace".into(),
            Some(browser_setup_action(request)),
        ));
        if request.handshake {
            checks.push(check_row(
                "handshake",
                true,
                false,
                "skipped because the definition is not installed; no Node process started".into(),
                Some(browser_setup_action(request)),
            ));
        } else {
            checks.push(check_row(
                "handshake",
                false,
                true,
                "not requested; no Node process started".into(),
                None,
            ));
        }
        report.node.version_status = "not_configured";
        report.checks = checks;
        report.ready = false;
        return Ok(report);
    };

    let definition = &record.definition;
    report.definition = DefinitionReport {
        configured: true,
        active: record.active,
        revision: Some(record.revision.to_string()),
        package_sha256: Some(definition.package_sha256.0.clone()),
        package_root: Some(definition.package_root.clone()),
        allowed_tools: definition.allowed_tools.clone(),
        readiness: readiness_rows
            .iter()
            .find(|row| row.get("namespace").and_then(Value::as_str) == Some(NAMESPACE))
            .cloned(),
    };

    let ExtensionTransport::Stdio {
        program,
        args,
        environment,
        ..
    } = &definition.transport
    else {
        checks.push(check_row(
            "definition",
            true,
            false,
            "browser-control.stdio is not a stdio transport".into(),
            Some(browser_setup_action(request)),
        ));
        if request.handshake {
            checks.push(check_row(
                "handshake",
                true,
                false,
                "skipped because the definition is not stdio; no Node process started".into(),
                Some(browser_setup_action(request)),
            ));
        } else {
            checks.push(check_row(
                "handshake",
                false,
                true,
                "not requested; no Node process started".into(),
                None,
            ));
        }
        report.node.version_status = "not_configured";
        report.checks = checks;
        report.ready = false;
        return Ok(report);
    };

    let node_path = PathBuf::from(program);
    let adapter_path = args.first().map(PathBuf::from);
    let config_path = args.get(1).map(PathBuf::from);
    let package_root = PathBuf::from(&definition.package_root);

    report.node = NodeReport {
        path: Some(node_path.display().to_string()),
        exists: node_path.is_file(),
        sha256: Some(definition_pinned_program(definition)),
        version: None,
        version_status: if request.handshake {
            "pending_handshake"
        } else {
            "not_run_without_handshake"
        },
        error: None,
    };

    let adapter_dir = adapter_path
        .as_ref()
        .and_then(|path| path.parent())
        .map(Path::to_path_buf);
    report.files.adapter = adapter_path
        .as_deref()
        .map(|path| file_report(path, MAX_ADAPTER_BYTES))
        .unwrap_or_default();
    report.files.adapter_package = adapter_dir
        .as_ref()
        .map(|directory| file_report(&directory.join("package.json"), MAX_ADAPTER_BYTES))
        .unwrap_or_default();
    report.files.package_lock = adapter_dir
        .as_ref()
        .map(|directory| file_report(&directory.join("package-lock.json"), MAX_ADAPTER_BYTES))
        .unwrap_or_default();
    report.files.config = config_path
        .as_deref()
        .map(|path| file_report(path, MAX_CONFIG_BYTES))
        .unwrap_or_default();
    report.files.manifest = file_report(&package_root.join("plugin.json"), MAX_ADAPTER_BYTES);
    report.configuration = config_path
        .as_deref()
        .map(config_summary)
        .unwrap_or_else(|| json!({"state": "missing_configuration_argument"}));

    report.dependencies.playwright = module_report(
        &adapter_dir
            .as_ref()
            .map(|directory| directory.join("node_modules").join("playwright"))
            .unwrap_or_default(),
    );
    report.dependencies.playwright_core = module_report(
        &adapter_dir
            .as_ref()
            .map(|directory| directory.join("node_modules").join("playwright-core"))
            .unwrap_or_default(),
    );
    report.dependencies.browser_cache = browser_cache_report(environment);

    checks.push(check_row(
        "definition",
        true,
        true,
        "browser-control.stdio is installed".into(),
        None,
    ));
    checks.push(check_row(
        "adapter",
        true,
        report.files.adapter.exists && report.files.adapter.error.is_none(),
        file_detail(&report.files.adapter),
        Some(format!(
            "Restore {}, then re-run: {}",
            report.files.adapter.path,
            browser_setup_action(request)
        )),
    ));
    checks.push(check_row(
        "adapter_package",
        true,
        report.files.adapter_package.exists && report.files.adapter_package.error.is_none(),
        file_detail(&report.files.adapter_package),
        Some(format!(
            "Restore {}, then re-run: {}",
            report.files.adapter_package.path,
            browser_setup_action(request)
        )),
    ));
    checks.push(check_row(
        "package_lock",
        false,
        report.files.package_lock.exists,
        file_detail(&report.files.package_lock),
        Some(format!(
            "Run: cd {} && npm install",
            adapter_dir
                .as_ref()
                .map(|path| path.display().to_string())
                .unwrap_or_else(|| "<adapter directory>".into())
        )),
    ));
    let config_state = report
        .configuration
        .get("state")
        .and_then(Value::as_str)
        .unwrap_or("unavailable");
    let config_ok =
        report.files.config.exists && report.files.config.error.is_none() && config_state == "read";
    checks.push(check_row(
        "config",
        true,
        config_ok,
        if config_ok {
            file_detail(&report.files.config)
        } else {
            format!("{} ({config_state})", file_detail(&report.files.config))
        },
        Some(browser_setup_action(request)),
    ));
    checks.push(check_row(
        "manifest",
        true,
        report.files.manifest.exists && report.files.manifest.error.is_none(),
        file_detail(&report.files.manifest),
        Some(
            "Reinstall the reviewed adapter package, then re-run extensions browser setup.".into(),
        ),
    ));
    checks.push(check_row(
        "node",
        true,
        report.node.exists,
        report
            .node
            .path
            .clone()
            .unwrap_or_else(|| "Node path is missing from the definition".into()),
        Some("Install Node or re-run extensions browser setup --node <absolute-node>.".into()),
    ));
    checks.push(check_row(
        "playwright",
        true,
        report.dependencies.playwright.exists,
        module_detail(&report.dependencies.playwright),
        Some(adapter_install_action(&adapter_dir)),
    ));
    checks.push(check_row(
        "playwright_core",
        true,
        report.dependencies.playwright_core.exists,
        module_detail(&report.dependencies.playwright_core),
        Some(adapter_install_action(&adapter_dir)),
    ));

    let cache = &report.dependencies.browser_cache;
    checks.push(check_row(
        "browser_cache",
        cache.configured,
        !cache.configured || (cache.exists && !cache.chromium.is_empty()),
        if cache.configured {
            format!(
                "{}; {} Chromium executable(s)",
                cache.path.as_deref().unwrap_or("cache path missing"),
                cache.chromium.len()
            )
        } else {
            "not configured; Playwright default cache was not inspected".into()
        },
        Some(
            "Run: npx playwright install chromium with ARGENTUM_PLAYWRIGHT_BROWSERS_PATH set to the configured cache."
                .into(),
        ),
    ));

    let definition_ready = report
        .definition
        .readiness
        .as_ref()
        .and_then(|row| row.get("definition_ready"))
        .and_then(Value::as_bool)
        .unwrap_or(false);
    checks.push(check_row(
        "definition_ready",
        false,
        definition_ready,
        report
            .definition
            .readiness
            .as_ref()
            .and_then(|row| row.get("issue"))
            .and_then(Value::as_str)
            .unwrap_or("readiness row unavailable")
            .to_owned(),
        Some(format!(
            "Review and activate in the local owner console: argentum extensions activate {NAMESPACE} --workspace <workspace> --database <database>"
        )),
    ));
    checks.push(check_row(
        "activation",
        false,
        record.active,
        if record.active {
            "saved activation is present; it is not a running process".into()
        } else {
            "saved activation is absent; the adapter can still be tested with --handshake".into()
        },
        Some(format!(
            "Run: argentum extensions activate {NAMESPACE} --workspace <workspace> --database <database>"
        )),
    ));

    let required_ready = checks
        .iter()
        .filter(|check| check.required)
        .all(|check| check.ok);
    if request.handshake {
        if !required_ready {
            checks.push(check_row(
                "handshake",
                true,
                false,
                "skipped because a required dependency check failed; no Node process was started"
                    .into(),
                Some(
                    "Resolve the failed required checks, then re-run extensions browser check --handshake."
                        .into(),
                ),
            ));
            report.handshake.attempted = false;
            report.handshake.ok = Some(false);
            report.handshake.error =
                Some("required dependency checks failed before handshake".into());
        } else {
            report.node.version = None;
            report.node.version_status = "probed_with_handshake";
            match probe_node_version(&node_path, Duration::from_secs(5)) {
                Ok(version) => {
                    report.node.version = Some(version);
                    match run_handshake(definition, request.workspace).await {
                        Ok((tools, duration_ms)) => {
                            report.handshake.attempted = true;
                            report.handshake.ok = Some(true);
                            report.handshake.tools = tools;
                            report.handshake.duration_ms = Some(duration_ms);
                            checks.push(check_row(
                                "handshake",
                                true,
                                true,
                                "adapter initialized, listed tools and retired".into(),
                                None,
                            ));
                        }
                        Err(error) => {
                            report.handshake.attempted = true;
                            report.handshake.ok = Some(false);
                            report.handshake.error = Some(error.clone());
                            checks.push(check_row(
                                "handshake",
                                true,
                                false,
                                error,
                                Some(
                                    "Inspect the adapter stderr and the pinned dependencies, then re-run extensions browser check --handshake."
                                        .into(),
                                ),
                            ));
                        }
                    }
                }
                Err(error) => {
                    report.node.version_status = "probe_failed";
                    report.node.error = Some(error.clone());
                    report.handshake.attempted = false;
                    report.handshake.ok = Some(false);
                    report.handshake.error = Some(format!("node --version failed: {error}"));
                    checks.push(check_row(
                        "handshake",
                        true,
                        false,
                        format!("node --version failed: {error}"),
                        Some("Fix the Node executable, then re-run --handshake.".into()),
                    ));
                }
            }
        }
    } else {
        checks.push(check_row(
            "handshake",
            false,
            true,
            "not requested; no adapter process started".into(),
            None,
        ));
    }

    report.checks = checks;
    report.ready = report
        .checks
        .iter()
        .filter(|check| check.required)
        .all(|check| check.ok);
    Ok(report)
}

async fn check(request: &BrowserRequest<'_>) -> Result<(), Error> {
    let mut report = build_check_report(request).await?;
    if !report.ready {
        report.error = Some(browser_readiness_error(&report));
    }
    print_check(&report, request.json)?;
    if !report.ready {
        if request.json {
            return Err(crate::json_contract::JsonEmitted.into());
        }
        return Err("browser readiness check failed".into());
    }
    Ok(())
}

fn browser_readiness_error(report: &BrowserCheckReport) -> crate::json_contract::JsonErrorBody {
    if !report.configured {
        crate::json_contract::JsonErrorBody::new(
            "browser_definition_missing",
            "browser-control.stdio is not installed for this workspace",
        )
    } else {
        crate::json_contract::JsonErrorBody::new(
            "browser_dependencies_missing",
            "browser dependencies are not ready",
        )
    }
}

#[derive(Debug, Serialize)]
struct PrepareReport {
    schema_version: u32,
    command: &'static str,
    ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<crate::json_contract::JsonErrorBody>,
    install: InstallReport,
    readiness: BrowserCheckReport,
    dependency_hashes: DependencyHashes,
}

#[derive(Debug, Default, Serialize)]
struct InstallReport {
    requested: bool,
    confirmed_network: bool,
    attempted: bool,
    ok: Option<bool>,
    commands: Vec<InstallCommand>,
    steps: Vec<InstallStep>,
    error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct InstallCommand {
    program: String,
    args: Vec<String>,
    cwd: String,
    environment: BTreeMap<String, String>,
}

#[derive(Debug, Serialize)]
struct InstallStep {
    name: &'static str,
    program: String,
    args: Vec<String>,
    cwd: String,
    state: String,
    exit_code: Option<i32>,
    duration_ms: u64,
    stdout: String,
    stderr: String,
    stdout_truncated: bool,
    stderr_truncated: bool,
    error: Option<String>,
}

#[derive(Debug, Default, Serialize)]
struct DependencyHashes {
    package_lock: Option<FileHash>,
    playwright_package: Option<FileHash>,
    playwright_core_package: Option<FileHash>,
    chromium: Vec<FileHash>,
}

struct InstallPlan {
    node: PathBuf,
    npm_cli: PathBuf,
    npx_cli: PathBuf,
    adapter_dir: PathBuf,
    cache: PathBuf,
    commands: Vec<InstallCommand>,
}

const INSTALL_TIMEOUT_SECONDS: u64 = 600;
const INSTALL_OUTPUT_LIMIT: usize = 256 * 1024;
const INSTALL_OUTPUT_TAIL_CHARS: usize = 8192;
async fn prepare(request: &BrowserRequest<'_>) -> Result<(), Error> {
    let mut initial = build_check_report(request).await?;
    if !initial.ready {
        initial.error = Some(browser_readiness_error(&initial));
    }
    let plan = install_plan(&initial);
    let mut install = InstallReport {
        requested: request.install,
        confirmed_network: request.confirm_network,
        ..InstallReport::default()
    };

    if !request.install {
        install.commands = plan
            .as_ref()
            .map(|plan| plan.commands.clone())
            .unwrap_or_else(|_| fallback_commands(&initial, request));
        let error = initial.error.clone();
        let report = PrepareReport {
            schema_version: 1,
            command: "browser.prepare",
            ok: initial.ready,
            error,
            install,
            readiness: initial,
            dependency_hashes: DependencyHashes::default(),
        };
        print_prepare(&report, request.json)?;
        if !report.ok {
            if request.json {
                return Err(crate::json_contract::JsonEmitted.into());
            }
            return Err(
                "browser dependencies are not ready; run the printed install commands".into(),
            );
        }
        return Ok(());
    }

    let plan = match plan {
        Ok(plan) => plan,
        Err(error) => {
            install.error = Some(error.clone());
            let report = PrepareReport {
                schema_version: 1,
                command: "browser.prepare",
                ok: false,
                error: initial.error.clone(),
                install,
                readiness: initial,
                dependency_hashes: DependencyHashes::default(),
            };
            print_prepare(&report, request.json)?;
            if request.json {
                return Err(crate::json_contract::JsonEmitted.into());
            }
            return Err(error.into());
        }
    };
    install.commands = plan.commands.clone();
    install.attempted = true;

    let cancel = CancellationToken::new();
    let signal = cancel.clone();
    let listener = tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            signal.cancel();
        }
    });
    let npm_args = vec![
        "ci".to_owned(),
        "--ignore-scripts".to_owned(),
        "--no-audit".to_owned(),
        "--no-fund".to_owned(),
    ];
    let npx_args = vec![
        "--no-install".to_owned(),
        "playwright".to_owned(),
        "install".to_owned(),
        "chromium".to_owned(),
    ];
    install.steps.push(
        run_install_step(
            "npm-ci",
            &plan.node,
            &plan.npm_cli,
            &npm_args,
            &plan.adapter_dir,
            &plan.cache,
            &cancel,
        )
        .await,
    );
    let npm_ok = install
        .steps
        .last()
        .is_some_and(|step| step.exit_code == Some(0) && step.error.is_none());
    if npm_ok {
        install.steps.push(
            run_install_step(
                "playwright-chromium",
                &plan.node,
                &plan.npx_cli,
                &npx_args,
                &plan.adapter_dir,
                &plan.cache,
                &cancel,
            )
            .await,
        );
    }
    listener.abort();
    let _ = listener.await;

    let install_steps_ok = install.steps.len() == 2
        && install
            .steps
            .iter()
            .all(|step| step.exit_code == Some(0) && step.error.is_none());
    install.ok = Some(install_steps_ok);
    let mut readiness = build_check_report(request).await?;
    if !readiness.ready {
        readiness.error = Some(browser_readiness_error(&readiness));
    }
    let dependency_hashes = dependency_hashes(&readiness);
    let hashes_ok = dependency_hashes.package_lock.is_some()
        && dependency_hashes.playwright_package.is_some()
        && !dependency_hashes.chromium.is_empty();
    let report_ok = install_steps_ok && readiness.ready && hashes_ok;
    if !report_ok && install.error.is_none() {
        install.error = Some(if !install_steps_ok {
            "dependency install command failed".into()
        } else if !readiness.ready {
            "readiness still has required failures after install".into()
        } else {
            "dependency hashes are incomplete after install".into()
        });
    }
    let error = if report_ok {
        None
    } else {
        Some(crate::json_contract::JsonErrorBody::new(
            "browser_prepare_failed",
            install
                .error
                .clone()
                .unwrap_or_else(|| "browser dependency preparation failed".into()),
        ))
    };
    let report = PrepareReport {
        schema_version: 1,
        command: "browser.prepare",
        ok: report_ok,
        error,
        install,
        readiness,
        dependency_hashes,
    };
    print_prepare(&report, request.json)?;
    if !report.ok {
        if request.json {
            return Err(crate::json_contract::JsonEmitted.into());
        }
        return Err("browser dependency preparation failed".into());
    }
    Ok(())
}

fn install_plan(report: &BrowserCheckReport) -> Result<InstallPlan, String> {
    if !report.definition.configured {
        return Err(
            "browser-control.stdio is not installed; run extensions browser setup first".into(),
        );
    }
    let adapter = PathBuf::from(&report.files.adapter.path);
    if !adapter.is_file() {
        return Err(format!("adapter is missing: {}", adapter.display()));
    }
    let adapter_dir = adapter
        .parent()
        .ok_or("adapter has no parent directory")?
        .to_path_buf();
    if !report.files.adapter_package.exists || !report.files.package_lock.exists {
        return Err("adapter package.json and package-lock.json are required".into());
    }
    let node = PathBuf::from(
        report
            .node
            .path
            .as_deref()
            .ok_or("definition has no Node executable path")?,
    );
    if !node.is_file() {
        return Err(format!("Node executable is missing: {}", node.display()));
    }
    let node_dir = node
        .parent()
        .ok_or("Node executable has no parent directory")?;
    let npm_cli = node_dir
        .join("node_modules")
        .join("npm")
        .join("bin")
        .join("npm-cli.js");
    let npx_cli = node_dir
        .join("node_modules")
        .join("npm")
        .join("bin")
        .join("npx-cli.js");
    if !npm_cli.is_file() || !npx_cli.is_file() {
        return Err(format!(
            "npm and npx CLI scripts are missing under {}",
            node_dir.display()
        ));
    }
    let cache = report
        .dependencies
        .browser_cache
        .path
        .as_deref()
        .filter(|_| report.dependencies.browser_cache.configured)
        .map(PathBuf::from)
        .ok_or(
            "configure the browser cache with extensions browser setup --browser-cache <absolute-dir>",
        )?;
    if !cache.is_dir() {
        return Err(format!(
            "configured browser cache is missing: {}",
            cache.display()
        ));
    }
    let commands = vec![
        InstallCommand {
            program: "npm".into(),
            args: vec![
                "ci".into(),
                "--ignore-scripts".into(),
                "--no-audit".into(),
                "--no-fund".into(),
            ],
            cwd: adapter_dir.display().to_string(),
            environment: BTreeMap::new(),
        },
        InstallCommand {
            program: "npx".into(),
            args: vec![
                "--no-install".into(),
                "playwright".into(),
                "install".into(),
                "chromium".into(),
            ],
            cwd: adapter_dir.display().to_string(),
            environment: BTreeMap::from([(
                "PLAYWRIGHT_BROWSERS_PATH".to_owned(),
                cache.display().to_string(),
            )]),
        },
    ];
    Ok(InstallPlan {
        node,
        npm_cli,
        npx_cli,
        adapter_dir,
        cache,
        commands,
    })
}

fn fallback_commands(
    report: &BrowserCheckReport,
    request: &BrowserRequest<'_>,
) -> Vec<InstallCommand> {
    let cwd = PathBuf::from(&report.files.adapter.path)
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| request.workspace.join("extensions").join("browser"))
        .display()
        .to_string();
    let cache = report
        .dependencies
        .browser_cache
        .path
        .clone()
        .unwrap_or_else(|| "<configured browser cache>".into());
    vec![
        InstallCommand {
            program: "npm".into(),
            args: vec![
                "ci".into(),
                "--ignore-scripts".into(),
                "--no-audit".into(),
                "--no-fund".into(),
            ],
            cwd: cwd.clone(),
            environment: BTreeMap::new(),
        },
        InstallCommand {
            program: "npx".into(),
            args: vec![
                "--no-install".into(),
                "playwright".into(),
                "install".into(),
                "chromium".into(),
            ],
            cwd,
            environment: BTreeMap::from([("PLAYWRIGHT_BROWSERS_PATH".to_owned(), cache)]),
        },
    ]
}

async fn run_install_step(
    name: &'static str,
    node: &Path,
    cli: &Path,
    args: &[String],
    cwd: &Path,
    cache: &Path,
    cancel: &CancellationToken,
) -> InstallStep {
    let mut argv = vec![cli.to_string_lossy().into_owned()];
    argv.extend(args.iter().cloned());
    let environment = BTreeMap::from([(
        "PLAYWRIGHT_BROWSERS_PATH".to_owned(),
        cache.to_string_lossy().into_owned(),
    )]);
    let spec = ProcessSpec::argv(node.to_string_lossy().into_owned(), argv.clone(), cwd)
        .and_then(|spec| spec.with_timeout(Duration::from_secs(INSTALL_TIMEOUT_SECONDS)))
        .and_then(|spec| spec.with_output_limit(INSTALL_OUTPUT_LIMIT))
        .and_then(|spec| spec.with_environment(environment));
    let spec = match spec {
        Ok(spec) => spec,
        Err(error) => {
            return InstallStep {
                name,
                program: node.display().to_string(),
                args: argv,
                cwd: cwd.display().to_string(),
                state: "not_started".into(),
                exit_code: None,
                duration_ms: 0,
                stdout: String::new(),
                stderr: String::new(),
                stdout_truncated: false,
                stderr_truncated: false,
                error: Some(error.to_string()),
            }
        }
    };
    let result = ProcessRunner.run(spec, cancel.clone()).await;
    match result {
        Ok(result) => {
            let state = match result.state {
                argentum_execution::ProcessState::Exited => "exited",
                argentum_execution::ProcessState::TimedOut => "timed_out",
                argentum_execution::ProcessState::Cancelled => "cancelled",
            };
            let error = if result.state != argentum_execution::ProcessState::Exited {
                Some(format!("install command {}", state.replace('_', " ")))
            } else if result.exit_code != Some(0) {
                Some(format!("install command exited {:?}", result.exit_code))
            } else {
                None
            };
            InstallStep {
                name,
                program: node.display().to_string(),
                args: argv,
                cwd: cwd.display().to_string(),
                state: state.into(),
                exit_code: result.exit_code,
                duration_ms: result.duration_ms,
                stdout: output_tail(&result.stdout),
                stderr: output_tail(&result.stderr),
                stdout_truncated: result.stdout_truncated
                    || result.stdout.chars().count() > INSTALL_OUTPUT_TAIL_CHARS,
                stderr_truncated: result.stderr_truncated
                    || result.stderr.chars().count() > INSTALL_OUTPUT_TAIL_CHARS,
                error,
            }
        }
        Err(error) => InstallStep {
            name,
            program: node.display().to_string(),
            args: argv,
            cwd: cwd.display().to_string(),
            state: "failed_to_start".into(),
            exit_code: None,
            duration_ms: 0,
            stdout: String::new(),
            stderr: String::new(),
            stdout_truncated: false,
            stderr_truncated: false,
            error: Some(error.to_string()),
        },
    }
}

fn output_tail(value: &str) -> String {
    let count = value.chars().count();
    if count <= INSTALL_OUTPUT_TAIL_CHARS {
        return value.to_owned();
    }
    value
        .chars()
        .skip(count - INSTALL_OUTPUT_TAIL_CHARS)
        .collect()
}

fn dependency_hashes(report: &BrowserCheckReport) -> DependencyHashes {
    let package_lock = report
        .files
        .package_lock
        .exists
        .then(|| FileHash {
            path: report.files.package_lock.path.clone(),
            bytes: report.files.package_lock.bytes.unwrap_or_default(),
            sha256: report.files.package_lock.sha256.clone().unwrap_or_default(),
        })
        .filter(|hash| !hash.sha256.is_empty());
    DependencyHashes {
        package_lock,
        playwright_package: file_hash(
            &PathBuf::from(&report.dependencies.playwright.path).join("package.json"),
        ),
        playwright_core_package: file_hash(
            &PathBuf::from(&report.dependencies.playwright_core.path).join("package.json"),
        ),
        chromium: report
            .dependencies
            .browser_cache
            .chromium
            .iter()
            .filter_map(|path| stream_hash(Path::new(path)))
            .collect(),
    }
}

fn print_prepare(report: &PrepareReport, json_output: bool) -> Result<(), Error> {
    if json_output {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }
    println!("Browser dependency prepare: {}", report.readiness.namespace);
    println!(
        "Install: requested={} confirmed_network={} attempted={} ok={}",
        report.install.requested,
        report.install.confirmed_network,
        report.install.attempted,
        report
            .install
            .ok
            .map_or("not run".into(), |ok| ok.to_string())
    );
    if let Some(error) = &report.install.error {
        println!("Install error: {error}");
    }
    println!("Install commands:");
    for command in &report.install.commands {
        println!("  cd {}", command.cwd);
        for (name, value) in &command.environment {
            println!("  set {name}={value}");
        }
        println!("  {} {}", command.program, command.args.join(" "));
    }
    for step in &report.install.steps {
        println!(
            "  {}: state={} exit={:?} duration={}ms",
            step.name, step.state, step.exit_code, step.duration_ms
        );
        if let Some(error) = &step.error {
            println!("    error: {error}");
        }
        if !step.stdout.is_empty() {
            println!("    stdout: {}", step.stdout);
        }
        if !step.stderr.is_empty() {
            println!("    stderr: {}", step.stderr);
        }
    }
    if report.install.requested {
        println!("Dependency hashes:");
        print_hash("package-lock", &report.dependency_hashes.package_lock);
        print_hash(
            "playwright package",
            &report.dependency_hashes.playwright_package,
        );
        print_hash(
            "playwright-core package",
            &report.dependency_hashes.playwright_core_package,
        );
        for chromium in &report.dependency_hashes.chromium {
            println!(
                "  Chromium {}: {} bytes sha256 {}",
                chromium.path, chromium.bytes, chromium.sha256
            );
        }
    }
    print_check(&report.readiness, false)?;
    println!("Prepare ready: {}", if report.ok { "yes" } else { "no" });
    Ok(())
}

fn print_hash(label: &str, hash: &Option<FileHash>) {
    match hash {
        Some(hash) => println!(
            "  {label}: {} ({} bytes, sha256 {})",
            hash.path, hash.bytes, hash.sha256
        ),
        None => println!("  {label}: unavailable"),
    }
}

fn print_check(report: &BrowserCheckReport, json_output: bool) -> Result<(), Error> {
    if json_output {
        println!("{}", serde_json::to_string_pretty(report)?);
        return Ok(());
    }
    println!("Browser control check: {}", report.namespace);
    println!(
        "Definition: configured={} active={} readiness={}",
        report.definition.configured,
        report.definition.active,
        report
            .definition
            .readiness
            .as_ref()
            .and_then(|row| row.get("definition_ready"))
            .and_then(Value::as_bool)
            .map_or("unknown".into(), |ready| ready.to_string())
    );
    println!(
        "Node: {} [present={} version={} {}]",
        report.node.path.as_deref().unwrap_or("missing"),
        report.node.exists,
        report.node.version.as_deref().unwrap_or("not probed"),
        report.node.version_status
    );
    for (label, file) in [
        ("Adapter", &report.files.adapter),
        ("Package", &report.files.adapter_package),
        ("Lock", &report.files.package_lock),
        ("Config", &report.files.config),
        ("Manifest", &report.files.manifest),
    ] {
        println!("{label}: {} [{}]", file.path, file_detail(file));
    }
    println!(
        "Playwright: {} [{}]",
        report.dependencies.playwright.path,
        module_detail(&report.dependencies.playwright)
    );
    println!(
        "Playwright core: {} [{}]",
        report.dependencies.playwright_core.path,
        module_detail(&report.dependencies.playwright_core)
    );
    println!(
        "Browser cache: configured={} {}",
        report.dependencies.browser_cache.configured,
        report
            .dependencies
            .browser_cache
            .path
            .as_deref()
            .unwrap_or("not configured")
    );
    for chromium in &report.dependencies.browser_cache.chromium {
        println!("  Chromium: {chromium}");
    }
    if report.handshake.requested {
        println!(
            "Handshake: attempted={} ok={} tools={} {}",
            report.handshake.attempted,
            report
                .handshake
                .ok
                .map_or("not run".into(), |ok| ok.to_string()),
            report.handshake.tools.len(),
            report.handshake.error.as_deref().unwrap_or("")
        );
        for tool in &report.handshake.tools {
            println!("  {tool}");
        }
    } else {
        println!("Handshake: not requested; no adapter process started");
    }
    println!("Ready: {}", if report.ready { "yes" } else { "no" });
    for check in &report.checks {
        println!(
            "  {} [{}] {}: {}",
            if check.ok { "ok" } else { "FAIL" },
            if check.required {
                "required"
            } else {
                "advisory"
            },
            check.id,
            check.detail
        );
        if !check.ok {
            if let Some(next) = &check.next_action {
                println!("    next: {next}");
            }
        }
    }
    println!("{}", report.qualification);
    Ok(())
}

fn check_row(
    id: &'static str,
    required: bool,
    ok: bool,
    detail: String,
    next_action: Option<String>,
) -> CheckReport {
    CheckReport {
        id,
        required,
        ok,
        detail,
        next_action,
    }
}

fn probe_node_version(program: &Path, timeout: Duration) -> Result<String, String> {
    let mut child = Command::new(program)
        .arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| error.to_string())?;
    let deadline = Instant::now() + timeout;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut output = String::new();
                if let Some(stdout) = child.stdout.take() {
                    let _ = stdout.take(4096).read_to_string(&mut output);
                }
                if !status.success() {
                    return Err("node --version exited nonzero".into());
                }
                let version = output.lines().next().unwrap_or_default().trim().to_owned();
                if version.is_empty() || version.len() > 64 || version.chars().any(char::is_control)
                {
                    return Err("node --version returned invalid output".into());
                }
                return Ok(version);
            }
            Ok(None) if Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("node --version exceeded 5 seconds".into());
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(10)),
            Err(error) => return Err(error.to_string()),
        }
    }
}

async fn run_handshake(
    definition: &ExtensionDefinition,
    workspace: &Path,
) -> Result<(Vec<String>, u128), String> {
    let started = Instant::now();
    let admitted = admit(definition, workspace).map_err(|error| error.to_string())?;
    let AdmittedDefinition {
        program_spec,
        file_leases,
        ..
    } = admitted;
    let spec = program_spec.ok_or("definition has no stdio program")?;
    let limits =
        argentum_extensions::default_process_limits().map_err(|error| error.to_string())?;
    let allowed = definition
        .allowed_tools
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let cancel = CancellationToken::new();
    let mut session = McpSession::connect_stdio(spec, limits, allowed, cancel, move || {
        file_leases
            .iter()
            .all(|lease| lease.verify_identity().is_ok())
    })
    .await
    .map_err(|error| format!("adapter initialization failed: {error}"))?;
    let discovered = tokio::time::timeout(Duration::from_secs(10), session.discover()).await;
    let (result, shutdown) = match discovered {
        Ok(Ok(tools)) => (
            Ok(tools),
            session.retire().await.map_err(|error| error.to_string()),
        ),
        Ok(Err(error)) => (
            Err(format!("tool discovery failed: {error}")),
            session.retire().await.map_err(|error| error.to_string()),
        ),
        Err(_) => (
            Err("tool discovery exceeded 10 seconds".to_owned()),
            session.retire().await.map_err(|error| error.to_string()),
        ),
    };
    if let Err(error) = shutdown {
        return Err(match result {
            Ok(_) => format!("adapter shutdown failed: {error}"),
            Err(result_error) => format!("{result_error}; adapter shutdown failed: {error}"),
        });
    }
    Ok((
        result?
            .into_iter()
            .map(|tool| tool.handle.name)
            .collect::<Vec<_>>(),
        started.elapsed().as_millis(),
    ))
}

fn definition_pinned_program(definition: &ExtensionDefinition) -> String {
    match &definition.transport {
        ExtensionTransport::Stdio { program_sha256, .. } => program_sha256.0.clone(),
        _ => String::new(),
    }
}

fn browser_setup_action(request: &BrowserRequest<'_>) -> String {
    format!(
        "argentum extensions browser setup --source extensions/browser --node <reviewed-node> --origin <loopback-origin> --workspace {} --database {}",
        request.workspace.display(),
        request.database.display()
    )
}

fn adapter_install_action(adapter_dir: &Option<PathBuf>) -> String {
    format!(
        "Run: cd {} && npm install",
        adapter_dir
            .as_ref()
            .map(|path| path.display().to_string())
            .unwrap_or_else(|| "<adapter directory>".into())
    )
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

    #[test]
    fn dependency_reports_are_bounded_and_do_not_spawn_node() {
        let directory = tempfile::tempdir().expect("directory");
        let module = directory.path().join("node_modules").join("playwright");
        std::fs::create_dir_all(&module).expect("module");
        std::fs::write(module.join("package.json"), br#"{"version":"1.60.0"}"#)
            .expect("package.json");
        let report = module_report(&module);
        assert!(report.exists);
        assert_eq!(report.version.as_deref(), Some("1.60.0"));

        let missing = module_report(
            &directory
                .path()
                .join("node_modules")
                .join("playwright-core"),
        );
        assert!(!missing.exists);
        assert_eq!(missing.error.as_deref(), Some("missing"));

        let file = file_report(&directory.path().join("adapter.mjs"), MAX_ADAPTER_BYTES);
        assert!(!file.exists);
        assert!(file.sha256.is_none());

        assert!(probe_node_version(
            &directory.path().join("missing-node"),
            Duration::from_millis(50)
        )
        .is_err());
    }

    #[test]
    fn browser_cache_reports_a_chromium_executable() {
        let directory = tempfile::tempdir().expect("directory");
        let cache = directory.path().join("ms-playwright");
        let chrome = cache
            .join("chromium-1234")
            .join("chrome-win")
            .join("chrome.exe");
        std::fs::create_dir_all(chrome.parent().expect("chrome parent")).expect("cache");
        std::fs::write(&chrome, b"stub").expect("chrome");
        let mut environment = BTreeMap::new();
        environment.insert(
            "ARGENTUM_PLAYWRIGHT_BROWSERS_PATH".to_owned(),
            cache.display().to_string(),
        );
        let report = browser_cache_report(&environment);
        assert!(report.configured);
        assert!(report.exists);
        assert_eq!(report.chromium.len(), 1);
        assert_eq!(
            Path::new(&report.chromium[0]),
            chrome.as_path(),
            "{}",
            report.chromium[0]
        );
    }

    #[test]
    fn prepare_hash_helpers_are_bounded() {
        let directory = tempfile::tempdir().expect("directory");
        let file = directory.path().join("file.bin");
        std::fs::write(&file, b"prepare").expect("file");
        let small = file_hash(&file).expect("file hash");
        let streamed = stream_hash(&file).expect("stream hash");
        assert_eq!(small.bytes, 7);
        assert_eq!(small.sha256, streamed.sha256);
        assert_eq!(streamed.bytes, 7);

        let long = "x".repeat(INSTALL_OUTPUT_TAIL_CHARS + 5);
        assert_eq!(
            output_tail(&long).chars().count(),
            INSTALL_OUTPUT_TAIL_CHARS
        );
        assert_eq!(output_tail("short"), "short");
    }
}
