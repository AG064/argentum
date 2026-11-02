//! Read-only diagnostic aggregate for `argentum doctor`.
//!
//! The report is built from the persisted store, static feature flags and the
//! fixed verification runner registry. It never starts a runtime, opens a
//! provider connection, reads the OS secret store or writes to the database.

use std::path::{Path, PathBuf};

use argentum_providers::ProviderRegistry;
use argentum_store::{Store, SCHEMA_VERSION};
use argentum_tools::ToolRegistry;
use serde::Serialize;

#[derive(Debug, Serialize)]
pub struct DoctorReport {
    pub schema_version: u32,
    pub healthy: bool,
    pub ready: bool,
    pub workspace: WorkspaceReport,
    pub database: DatabaseReport,
    pub providers: ProviderReport,
    pub harness: HarnessReport,
    pub verification: VerificationReport,
    pub features: FeatureReport,
    pub recovery: RecoveryReport,
    pub checks: Vec<CheckReport>,
}

#[derive(Debug, Serialize)]
pub struct WorkspaceReport {
    pub path: String,
    pub exists: bool,
    pub status: &'static str,
    pub project_id: Option<String>,
    pub workspace_key: Option<String>,
    pub sessions: Option<usize>,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct DatabaseReport {
    pub path: String,
    pub exists: bool,
    pub openable: bool,
    pub schema_version: Option<u16>,
    pub expected_schema_version: u16,
    pub status: &'static str,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ProviderReport {
    pub count: usize,
    pub selected: Option<String>,
    pub readiness: &'static str,
    pub profiles: Vec<ProviderSummary>,
    pub note: &'static str,
}

#[derive(Debug, Serialize)]
pub struct ProviderSummary {
    pub id: String,
    pub label: String,
    pub kind: &'static str,
    pub model: String,
    pub selected: bool,
    pub credential_required: bool,
    pub coding_supported: bool,
}

#[derive(Debug, Serialize)]
pub struct HarnessReport {
    pub status: &'static str,
    pub execution_profile: Option<String>,
    pub approval_mode: Option<String>,
    pub sandbox_scope: Option<String>,
    pub verification_enabled: bool,
    pub module_preferences: usize,
    pub readiness: &'static str,
}

#[derive(Debug, Serialize)]
pub struct VerificationReport {
    pub fixed_runners: Vec<String>,
    pub count: usize,
    pub enabled_by_policy: bool,
}

#[derive(Debug, Serialize)]
pub struct FeatureReport {
    pub extensions: bool,
    pub browser_adapter: bool,
    pub desktop_control: bool,
    pub local_artifacts: bool,
    pub note: &'static str,
}

#[derive(Debug, Serialize)]
pub struct RecoveryReport {
    pub unfinished_change_operations: usize,
    pub attention_required: bool,
}

#[derive(Debug, Serialize)]
pub struct CheckReport {
    pub id: &'static str,
    pub required: bool,
    pub ok: bool,
    pub detail: String,
}

pub fn build(workspace: &Path, database: Option<PathBuf>) -> DoctorReport {
    let workspace_path = workspace.to_path_buf();
    let workspace_exists = workspace_path.exists();
    let mut checks = Vec::new();

    checks.push(CheckReport {
        id: "workspace_exists",
        required: false,
        ok: workspace_exists,
        detail: if workspace_exists {
            "workspace path exists".into()
        } else {
            "workspace path does not exist".into()
        },
    });

    let database_path = database.unwrap_or_else(|| PathBuf::from("unresolved"));
    let database_exists = database_path.is_file();
    checks.push(CheckReport {
        id: "database_exists",
        required: true,
        ok: database_exists,
        detail: if database_exists {
            "database file exists".into()
        } else {
            "database file does not exist; run a normal command to initialize it".into()
        },
    });

    let mut database = DatabaseReport {
        path: database_path.display().to_string(),
        exists: database_exists,
        openable: false,
        schema_version: None,
        expected_schema_version: SCHEMA_VERSION,
        status: "missing",
        error: None,
    };
    let mut workspace_report = WorkspaceReport {
        path: workspace_path.display().to_string(),
        exists: workspace_exists,
        status: "missing",
        project_id: None,
        workspace_key: None,
        sessions: None,
        error: None,
    };
    let mut providers = ProviderReport {
        count: 0,
        selected: None,
        readiness: "unavailable",
        profiles: Vec::new(),
        note: "Credential presence is not inspected. Coding readiness is the static adapter view; local runtime activation is checked at dispatch.",
    };
    let mut harness = HarnessReport {
        status: "unavailable",
        execution_profile: None,
        approval_mode: None,
        sandbox_scope: None,
        verification_enabled: false,
        module_preferences: 0,
        readiness: "unavailable",
    };
    let mut recovery = RecoveryReport {
        unfinished_change_operations: 0,
        attention_required: false,
    };
    let mut verification = VerificationReport {
        fixed_runners: ToolRegistry::default()
            .verification_runners()
            .iter()
            .map(|runner| runner.id())
            .collect(),
        count: 0,
        enabled_by_policy: false,
    };
    verification.count = verification.fixed_runners.len();

    if database_exists {
        match Store::open_existing_read_only(&database_path) {
            Ok(store) => {
                database.openable = true;
                match store.schema_version() {
                    Ok(version) => {
                        database.schema_version = Some(version);
                        if version == SCHEMA_VERSION {
                            database.status = "ok";
                            populate_initialized(
                                &store,
                                &workspace_path,
                                &mut workspace_report,
                                &mut providers,
                                &mut harness,
                                &mut verification,
                                &mut recovery,
                            );
                        } else {
                            database.status = "schema_mismatch";
                        }
                    }
                    Err(error) => {
                        database.status = "unreadable";
                        database.error = Some(error.to_string());
                    }
                }
            }
            Err(error) => {
                database.status = "unopenable";
                database.error = Some(error.to_string());
            }
        }
    }

    checks.push(CheckReport {
        id: "database_openable",
        required: true,
        ok: database.openable,
        detail: if database.openable {
            "database opened read-only".into()
        } else {
            database
                .error
                .clone()
                .unwrap_or_else(|| "database was not opened".into())
        },
    });
    checks.push(CheckReport {
        id: "schema_current",
        required: true,
        ok: database.schema_version == Some(SCHEMA_VERSION),
        detail: match database.schema_version {
            Some(version) => format!("schema {version}; expected {SCHEMA_VERSION}"),
            None => "schema version was not readable".into(),
        },
    });
    checks.push(CheckReport {
        id: "workspace_initialized",
        required: false,
        ok: workspace_report.status == "initialized",
        detail: workspace_report
            .error
            .clone()
            .unwrap_or_else(|| workspace_report.status.into()),
    });
    checks.push(CheckReport {
        id: "provider_ready",
        required: false,
        ok: providers.readiness == "ready",
        detail: providers.readiness.into(),
    });
    checks.push(CheckReport {
        id: "harness_configured",
        required: false,
        ok: harness.status == "persisted",
        detail: if harness.status == "persisted" {
            "execution policy is persisted".into()
        } else {
            "defaults apply until the owner selects a profile".into()
        },
    });
    checks.push(CheckReport {
        id: "recovery_clear",
        required: false,
        ok: recovery.unfinished_change_operations == 0,
        detail: format!(
            "{} unfinished change operation(s)",
            recovery.unfinished_change_operations
        ),
    });

    let healthy = checks
        .iter()
        .filter(|check| check.required)
        .all(|check| check.ok);
    let ready = checks.iter().all(|check| check.ok);

    DoctorReport {
        schema_version: 1,
        healthy,
        ready,
        workspace: workspace_report,
        database,
        providers,
        harness,
        verification,
        features: FeatureReport {
            extensions: cfg!(feature = "extensions"),
            browser_adapter: cfg!(feature = "extensions"),
            desktop_control: cfg!(all(windows, feature = "desktop-control")),
            local_artifacts: cfg!(feature = "artifacts"),
            note: "Feature flags describe this binary. The Node browser adapter and local runtime bundles are not probed.",
        },
        recovery,
        checks,
    }
}

fn populate_initialized(
    store: &Store,
    workspace: &Path,
    workspace_report: &mut WorkspaceReport,
    providers: &mut ProviderReport,
    harness: &mut HarnessReport,
    verification: &mut VerificationReport,
    recovery: &mut RecoveryReport,
) {
    let resolution = match store.resolve_existing_workspace(workspace) {
        Ok(resolution) => resolution,
        Err(error) => {
            workspace_report.status = "not_initialized";
            workspace_report.error = Some(error.to_string());
            return;
        }
    };
    workspace_report.status = "initialized";
    workspace_report.project_id = Some(resolution.snapshot.project.id.to_string());
    workspace_report.workspace_key = Some(resolution.workspace_key);
    workspace_report.sessions = Some(resolution.snapshot.sessions.len());

    let project_id = resolution.snapshot.project.id;
    let profiles = store.provider_profiles(project_id).unwrap_or_default();
    let registry = ProviderRegistry::default();
    providers.count = profiles.len();
    providers.selected = profiles
        .iter()
        .find(|profile| profile.selected)
        .map(|profile| profile.id.clone());
    providers.profiles = profiles
        .iter()
        .map(|profile| {
            let coding = registry
                .resolve_capabilities(profile)
                .map(|resolution| resolution.supports_coding())
                .unwrap_or(false);
            ProviderSummary {
                id: profile.id.clone(),
                label: profile.label.clone(),
                kind: provider_kind_name(profile.kind),
                model: profile.model.clone(),
                selected: profile.selected,
                credential_required: argentum_providers::provider_preset(&profile.id)
                    .is_some_and(|preset| preset.credential_profile_id.is_some()),
                coding_supported: coding,
            }
        })
        .collect();
    providers.readiness = if profiles.is_empty() {
        "no_profiles"
    } else if providers.selected.is_none() {
        "no_selected_profile"
    } else if providers
        .profiles
        .iter()
        .find(|profile| profile.selected)
        .is_some_and(|profile| profile.coding_supported)
    {
        "ready"
    } else {
        "coding_unavailable"
    };

    let policy = store.load_execution_policy(project_id).ok().flatten();
    if let Some(policy) = &policy {
        harness.status = "persisted";
        harness.execution_profile = Some(policy.profile_id.clone());
        harness.approval_mode = enum_json_name(&policy.approval_mode);
        harness.sandbox_scope = enum_json_name(&policy.sandbox_scope);
        harness.verification_enabled = policy
            .capability_enabled
            .get("verification.runner")
            .copied()
            .unwrap_or(false);
    } else {
        harness.status = "defaults";
        harness.readiness = "defaults";
    }
    harness.module_preferences = store
        .load_module_preferences(project_id)
        .ok()
        .flatten()
        .map(|preferences| preferences.len())
        .unwrap_or(0);
    if harness.status == "persisted" {
        harness.readiness = "ready";
    }
    verification.enabled_by_policy = harness.verification_enabled;

    recovery.unfinished_change_operations = store
        .unfinished_change_operations(project_id)
        .map(|operations| operations.len())
        .unwrap_or_default();
    recovery.attention_required = recovery.unfinished_change_operations > 0;
}

pub fn format_text(report: &DoctorReport) -> String {
    let mut output = String::new();
    output.push_str(&format!(
        "Workspace: {} [{}]\n",
        report.workspace.path, report.workspace.status
    ));
    if let Some(project) = &report.workspace.project_id {
        output.push_str(&format!("  project: {project}\n"));
    }
    if let Some(key) = &report.workspace.workspace_key {
        output.push_str(&format!("  identity: {key}\n"));
    }
    if let Some(sessions) = report.workspace.sessions {
        output.push_str(&format!("  sessions: {sessions}\n"));
    }
    if let Some(error) = &report.workspace.error {
        output.push_str(&format!("  detail: {error}\n"));
    }
    output.push_str(&format!(
        "Database: {} [{}; schema {} of {}]\n",
        report.database.path,
        report.database.status,
        report
            .database
            .schema_version
            .map_or_else(|| "unknown".into(), |version| version.to_string()),
        report.database.expected_schema_version
    ));
    if let Some(error) = &report.database.error {
        output.push_str(&format!("  detail: {error}\n"));
    }
    output.push_str(&format!(
        "Providers: {} profile(s); selected {}; readiness {}\n",
        report.providers.count,
        report.providers.selected.as_deref().unwrap_or("none"),
        report.providers.readiness
    ));
    for profile in &report.providers.profiles {
        output.push_str(&format!(
            "  {}  {}  {}  {}  coding={}  credential={}\n",
            profile.id,
            profile.label,
            profile.kind,
            profile.model,
            if profile.coding_supported {
                "yes"
            } else {
                "no"
            },
            if profile.credential_required {
                "required"
            } else {
                "none"
            }
        ));
    }
    output.push_str(&format!(
        "Harness: {}; profile {}; approval {}; scope {}; verification {}\n",
        report.harness.status,
        report
            .harness
            .execution_profile
            .as_deref()
            .unwrap_or("default"),
        report.harness.approval_mode.as_deref().unwrap_or("default"),
        report.harness.sandbox_scope.as_deref().unwrap_or("default"),
        if report.harness.verification_enabled {
            "enabled"
        } else {
            "disabled"
        }
    ));
    output.push_str(&format!(
        "Verification: {} fixed runner(s): {}\n",
        report.verification.count,
        report.verification.fixed_runners.join(", ")
    ));
    output.push_str(&format!(
        "Features: extensions={} browser-adapter={} desktop-control={} local-artifacts={}\n",
        report.features.extensions,
        report.features.browser_adapter,
        report.features.desktop_control,
        report.features.local_artifacts
    ));
    output.push_str(&format!(
        "Recovery: {} unfinished change operation(s)\n",
        report.recovery.unfinished_change_operations
    ));
    output.push_str(&format!(
        "Healthy: {}\nReady: {}\n",
        if report.healthy { "yes" } else { "no" },
        if report.ready { "yes" } else { "no" }
    ));
    for check in &report.checks {
        output.push_str(&format!(
            "  {} {}: {}\n",
            if check.ok { "ok" } else { "FAIL" },
            check.id,
            check.detail
        ));
    }
    output
}

fn enum_json_name<T: Serialize>(value: &T) -> Option<String> {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
}

fn provider_kind_name(kind: argentum_domain::ProviderKind) -> &'static str {
    match kind {
        argentum_domain::ProviderKind::OpenAiCompatible => "openai-compatible",
        argentum_domain::ProviderKind::Anthropic => "anthropic",
        argentum_domain::ProviderKind::LocalLmStudio => "local-lm-studio",
        argentum_domain::ProviderKind::LocalLlamaCpp => "local-llama-cpp",
        argentum_domain::ProviderKind::Unknown => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_database_is_unhealthy_but_reports_the_path() {
        let directory = tempfile::tempdir().expect("directory");
        let database = directory.path().join("missing.db");
        let report = build(directory.path(), Some(database.clone()));
        assert!(!report.healthy);
        assert_eq!(report.database.path, database.display().to_string());
        assert!(!report.database.exists);
        assert_eq!(report.database.status, "missing");
        assert!(report
            .checks
            .iter()
            .any(|check| check.id == "database_exists" && check.required && !check.ok));
    }

    #[test]
    fn current_store_is_healthy_and_workspace_is_reported() {
        let directory = tempfile::tempdir().expect("directory");
        let database = directory.path().join("argentum.db");
        let store = Store::open(&database).expect("store");
        let workspace = store
            .resolve_workspace(directory.path())
            .expect("workspace");
        drop(workspace);
        drop(store);

        let report = build(directory.path(), Some(database));
        assert!(report.healthy, "{:?}", report.checks);
        assert_eq!(report.database.schema_version, Some(SCHEMA_VERSION));
        assert_eq!(report.workspace.status, "initialized");
        assert_eq!(report.workspace.sessions, Some(1));
        assert_eq!(report.verification.count, 4);
        assert_eq!(report.verification.fixed_runners.len(), 4);
    }
}
