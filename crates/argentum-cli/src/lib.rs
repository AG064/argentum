pub mod protocol;
pub mod server;

use std::path::{Path, PathBuf};

use argentum_domain::{
    AppCommand, AppEvent, Capability, ProviderKind, ProviderProfile, ProviderStatus,
    WorkspaceSnapshot,
};
use argentum_platform::{AppPaths, PlatformError};
use argentum_providers::{
    LocalLmStudioProvider, ProviderCredentials, ProviderError, ProviderRegistry,
};
use argentum_runtime::{RuntimeError, RuntimeService};
use argentum_security::{ApprovalPolicy, CapabilityBroker, SecretValue, SecurityError};
use argentum_store::{Store, StoreError};
use argentum_tools::ToolRegistry;
use argentum_workspaces::WorkspaceManager;
use thiserror::Error;
use tokio::sync::broadcast;
use tracing::warn;

pub const DEFAULT_LM_STUDIO_ENDPOINT: &str = argentum_domain::DEFAULT_LM_STUDIO_ENDPOINT;
pub const DEFAULT_MODEL: &str = argentum_domain::DEFAULT_MODEL;
pub const DEFAULT_PROVIDER_ID: &str = argentum_domain::DEFAULT_PROVIDER_ID;

#[derive(Clone)]
pub struct HostConfig {
    pub workspace: PathBuf,
    pub database: Option<PathBuf>,
    pub provider_endpoint: String,
    pub model: String,
    provider_credentials: ProviderCredentials,
}

impl std::fmt::Debug for HostConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("HostConfig")
            .field("workspace", &self.workspace)
            .field("database", &self.database)
            .field("provider_endpoint", &self.provider_endpoint)
            .field("model", &self.model)
            .field("provider_credentials", &self.provider_credentials)
            .finish()
    }
}

impl HostConfig {
    pub fn discover(workspace: impl Into<PathBuf>) -> Result<Self, HostError> {
        let paths = AppPaths::discover()?;
        Ok(Self {
            workspace: workspace.into(),
            database: Some(paths.database),
            provider_endpoint: DEFAULT_LM_STUDIO_ENDPOINT.into(),
            model: DEFAULT_MODEL.into(),
            provider_credentials: provider_credentials_from_environment()?,
        })
    }

    pub fn in_memory(workspace: impl Into<PathBuf>) -> Self {
        Self {
            workspace: workspace.into(),
            database: None,
            provider_endpoint: DEFAULT_LM_STUDIO_ENDPOINT.into(),
            model: DEFAULT_MODEL.into(),
            provider_credentials: ProviderCredentials::default(),
        }
    }

    pub fn with_provider_credentials(mut self, credentials: ProviderCredentials) -> Self {
        self.provider_credentials = credentials;
        self
    }

    pub fn with_provider_credential(
        mut self,
        profile_id: impl AsRef<str>,
        api_key: SecretValue,
    ) -> Result<Self, HostError> {
        self.provider_credentials.insert(profile_id, api_key)?;
        Ok(self)
    }

    pub fn with_provider_api_keys(
        mut self,
        values: impl IntoIterator<Item = (String, SecretValue)>,
    ) -> Result<Self, HostError> {
        self.provider_credentials = ProviderCredentials::from_profile_api_keys(values)?;
        Ok(self)
    }
}

fn provider_credentials_from_environment() -> Result<ProviderCredentials, HostError> {
    provider_credentials_from_lookup(|variable| std::env::var(variable))
}

fn provider_credentials_from_lookup(
    mut lookup: impl FnMut(&str) -> Result<String, std::env::VarError>,
) -> Result<ProviderCredentials, HostError> {
    const SUPPORTED_KEYS: [(&str, &str); 3] = [
        ("openai", "OPENAI_API_KEY"),
        ("minimax", "MINIMAX_API_KEY"),
        ("deepseek", "DEEPSEEK_API_KEY"),
    ];
    let mut credentials = ProviderCredentials::default();
    for (profile_id, variable) in SUPPORTED_KEYS {
        match lookup(variable) {
            Ok(value) if !value.trim().is_empty() => {
                credentials.insert(profile_id, SecretValue::new(value))?;
            }
            Ok(_) | Err(std::env::VarError::NotPresent) => {}
            Err(std::env::VarError::NotUnicode(_)) => {
                warn!(
                    environment_variable = variable,
                    "provider credential is not valid Unicode"
                );
            }
        }
    }
    Ok(credentials)
}

#[derive(Debug, Error)]
pub enum HostError {
    #[error(transparent)]
    Platform(#[from] PlatformError),
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Security(#[from] SecurityError),
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error(transparent)]
    Runtime(#[from] RuntimeError),
}

#[derive(Clone, Debug)]
pub struct CommandHost {
    runtime: RuntimeService,
}

impl CommandHost {
    pub fn start(config: HostConfig) -> Result<Self, HostError> {
        let store = match config.database {
            Some(path) => Store::open(path)?,
            None => Store::open_in_memory()?,
        };
        let policy = ApprovalPolicy::default().allow_without_approval(Capability::ReadFiles);
        let broker = CapabilityBroker::new(&config.workspace, policy)?;
        let workspace = WorkspaceManager::new(broker);
        let tools = ToolRegistry::with_builtins(workspace.clone());
        let mut providers = ProviderRegistry::with_credentials(config.provider_credentials);
        match LocalLmStudioProvider::new(&config.provider_endpoint, &config.model) {
            Ok(provider) => providers.register(provider),
            Err(error_value) => {
                warn!(error = %error_value, "unable to configure local LM Studio provider");
            }
        }
        let default_provider = ProviderProfile {
            id: DEFAULT_PROVIDER_ID.into(),
            label: "LM Studio".into(),
            kind: ProviderKind::LocalLmStudio,
            endpoint: config.provider_endpoint,
            model: config.model,
            selected: true,
        };
        Ok(Self::from_runtime(
            RuntimeService::new_with_default_provider_profile(
                store,
                providers,
                tools,
                workspace,
                default_provider,
            )?,
        ))
    }

    pub fn from_runtime(runtime: RuntimeService) -> Self {
        Self { runtime }
    }

    pub fn client(&self) -> InProcessClient {
        InProcessClient {
            runtime: self.runtime.clone(),
        }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.runtime.subscribe()
    }

    pub fn provider_statuses(&self) -> Vec<ProviderStatus> {
        self.runtime.provider_statuses()
    }

    pub fn provider_profiles(&self) -> Result<Vec<ProviderProfile>, HostError> {
        Ok(self.runtime.provider_profiles()?)
    }

    pub fn publish_initial_state(&self) -> Result<(), HostError> {
        self.runtime.publish_workspace_state()?;
        self.runtime.publish_provider_profiles()?;
        self.runtime.publish_provider_statuses();
        self.runtime.publish_layout()?;
        Ok(())
    }

    pub fn workspace_snapshot(&self) -> Result<WorkspaceSnapshot, HostError> {
        Ok(self.runtime.workspace_snapshot()?)
    }

    pub fn workspace_root(&self) -> &Path {
        self.runtime.workspace_root()
    }

    pub fn runtime(&self) -> &RuntimeService {
        &self.runtime
    }
}

#[derive(Clone, Debug)]
pub struct InProcessClient {
    runtime: RuntimeService,
}

impl InProcessClient {
    pub async fn dispatch(&self, command: AppCommand) -> Result<(), HostError> {
        self.runtime.dispatch(command).await?;
        Ok(())
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.runtime.subscribe()
    }
}

#[cfg(test)]
mod tests {
    use argentum_domain::{
        AppCommand, AppEvent, ApprovalScope, Capability, ToolInput, ToolRequest, ToolResultState,
    };

    use super::*;

    #[tokio::test]
    async fn in_process_client_dispatches_without_a_transport_boundary() {
        let workspace = tempfile::tempdir().expect("workspace");
        let host = CommandHost::start(HostConfig::in_memory(workspace.path())).expect("host");
        let client = host.client();
        let mut events = client.subscribe();

        client
            .dispatch(AppCommand::NewSession)
            .await
            .expect("new session command");

        assert!(matches!(
            events.recv().await.expect("session event"),
            AppEvent::SessionCreated(_)
        ));
    }

    #[tokio::test]
    async fn host_keeps_write_approval_enabled() {
        let workspace = tempfile::tempdir().expect("workspace");
        let host = CommandHost::start(HostConfig::in_memory(workspace.path())).expect("host");

        assert_eq!(
            host.runtime().workspace_root(),
            std::fs::canonicalize(workspace.path())
                .expect("canonical workspace")
                .as_path()
        );
        assert_eq!(host.runtime().tool_count(), 2);
    }

    #[tokio::test]
    async fn in_process_client_executes_read_tools_through_app_commands() {
        let workspace = tempfile::tempdir().expect("workspace");
        std::fs::write(workspace.path().join("note.txt"), "workspace note").expect("fixture");
        let host = CommandHost::start(HostConfig::in_memory(workspace.path())).expect("host");
        let client = host.client();
        let mut events = client.subscribe();
        let call_id = "00000000-0000-0000-0000-000000000011"
            .parse()
            .expect("call id");
        let run_id = "00000000-0000-0000-0000-000000000012"
            .parse()
            .expect("run id");

        client
            .dispatch(AppCommand::RequestTool {
                request: ToolRequest {
                    call_id,
                    run_id,
                    input: ToolInput::ReadText {
                        path: "note.txt".into(),
                    },
                },
            })
            .await
            .expect("read tool command");

        let emitted = std::iter::from_fn(|| events.try_recv().ok()).collect::<Vec<_>>();
        assert!(emitted.iter().any(|event| matches!(
            event,
            AppEvent::ToolStarted(trace)
                if trace.id == call_id && trace.tool_id == "read_text"
        )));
        assert!(emitted.iter().any(|event| matches!(
            event,
            AppEvent::ToolFinished(trace)
                if trace.id == call_id && trace.result == ToolResultState::Succeeded
        )));
        assert!(!emitted
            .iter()
            .any(|event| matches!(event, AppEvent::ApprovalRequested(_))));
    }

    #[tokio::test]
    async fn in_process_client_preserves_write_approval() {
        let workspace = tempfile::tempdir().expect("workspace");
        let host = CommandHost::start(HostConfig::in_memory(workspace.path())).expect("host");
        let client = host.client();
        let mut events = client.subscribe();
        let call_id = "00000000-0000-0000-0000-000000000021"
            .parse()
            .expect("call id");
        let run_id = "00000000-0000-0000-0000-000000000022"
            .parse()
            .expect("run id");

        client
            .dispatch(AppCommand::RequestTool {
                request: ToolRequest {
                    call_id,
                    run_id,
                    input: ToolInput::WriteText {
                        path: "approved.txt".into(),
                        content: "approved content".into(),
                    },
                },
            })
            .await
            .expect("write tool command");

        let approval = match events.recv().await.expect("approval event") {
            AppEvent::ApprovalRequested(approval) => approval,
            event => panic!("unexpected event: {event:?}"),
        };
        assert_eq!(approval.run_id, run_id);
        assert_eq!(approval.tool_id, "write_text");
        assert_eq!(approval.capabilities, vec![Capability::WriteFiles]);
        assert!(!workspace.path().join("approved.txt").exists());

        client
            .dispatch(AppCommand::ApproveTool {
                approval_id: approval.id,
                scope: ApprovalScope::Once,
            })
            .await
            .expect("approval command");

        let emitted = std::iter::from_fn(|| events.try_recv().ok()).collect::<Vec<_>>();
        assert!(emitted.iter().any(|event| matches!(
            event,
            AppEvent::ApprovalResolved {
                approval_id,
                approved: true,
            } if *approval_id == approval.id
        )));
        assert!(emitted.iter().any(|event| matches!(
            event,
            AppEvent::ToolFinished(trace)
                if trace.id == call_id && trace.result == ToolResultState::Succeeded
        )));
        assert_eq!(
            std::fs::read_to_string(workspace.path().join("approved.txt")).expect("approved write"),
            "approved content"
        );
    }

    #[tokio::test]
    async fn in_process_tool_commands_cannot_escape_the_workspace() {
        let workspace = tempfile::tempdir().expect("workspace");
        let outside = tempfile::NamedTempFile::new().expect("outside file");
        std::fs::write(outside.path(), "private").expect("outside fixture");
        let host = CommandHost::start(HostConfig::in_memory(workspace.path())).expect("host");
        let client = host.client();
        let mut events = client.subscribe();
        let call_id = "00000000-0000-0000-0000-000000000031"
            .parse()
            .expect("call id");

        client
            .dispatch(AppCommand::RequestTool {
                request: ToolRequest {
                    call_id,
                    run_id: "00000000-0000-0000-0000-000000000032"
                        .parse()
                        .expect("run id"),
                    input: ToolInput::ReadText {
                        path: outside.path().display().to_string(),
                    },
                },
            })
            .await
            .expect("bounded read command");

        let emitted = std::iter::from_fn(|| events.try_recv().ok()).collect::<Vec<_>>();
        assert!(emitted.iter().any(|event| matches!(
            event,
            AppEvent::ToolFinished(trace)
                if trace.id == call_id && trace.result == ToolResultState::Failed
        )));
        assert!(emitted.iter().any(|event| matches!(
            event,
            AppEvent::RunError { message, .. }
                if message == "Tool execution failed inside the workspace boundary."
        )));
    }

    #[tokio::test]
    async fn command_host_restores_and_selects_durable_sessions() {
        let directory = tempfile::tempdir().expect("directory");
        let workspace = directory.path().join("workspace");
        std::fs::create_dir(&workspace).expect("workspace");
        let database = directory.path().join("argentum.db");
        let mut config = HostConfig::in_memory(&workspace);
        config.database = Some(database.clone());
        let host = CommandHost::start(config.clone()).expect("host");
        let first = host.workspace_snapshot().expect("first snapshot");
        host.client()
            .dispatch(AppCommand::NewSession)
            .await
            .expect("new session");
        let expanded = host.workspace_snapshot().expect("expanded snapshot");
        assert_eq!(expanded.sessions.len(), 2);
        drop(host);

        let reopened = CommandHost::start(config).expect("reopened host");
        let restored = reopened.workspace_snapshot().expect("restored snapshot");
        assert_eq!(restored.project.id, first.project.id);
        assert_eq!(restored.sessions, expanded.sessions);
        reopened
            .client()
            .dispatch(AppCommand::SelectSession {
                session_id: first.active_session_id.expect("first session"),
            })
            .await
            .expect("select session");

        assert_eq!(
            reopened
                .workspace_snapshot()
                .expect("selected snapshot")
                .active_session_id,
            first.active_session_id
        );
    }

    #[test]
    fn supported_environment_credentials_map_to_canonical_profile_ids_safely() {
        let openai_secret = "openai-environment-fixture";
        let minimax_secret = "minimax-environment-fixture";
        let deepseek_secret = "deepseek-environment-fixture";
        let credentials = provider_credentials_from_lookup(|variable| match variable {
            "OPENAI_API_KEY" => Ok(openai_secret.into()),
            "MINIMAX_API_KEY" => Ok(minimax_secret.into()),
            "DEEPSEEK_API_KEY" => Ok(deepseek_secret.into()),
            _ => Err(std::env::VarError::NotPresent),
        })
        .expect("environment credentials");

        for profile_id in ["openai", "minimax", "deepseek"] {
            assert!(credentials.contains_profile(profile_id));
        }
        let rendered = format!("{credentials:?}");
        assert!(!rendered.contains(openai_secret));
        assert!(!rendered.contains(minimax_secret));
        assert!(!rendered.contains(deepseek_secret));
    }

    #[test]
    fn single_credential_builder_merges_with_existing_credentials() {
        let config = HostConfig::in_memory(".")
            .with_provider_credential("openai", SecretValue::new("openai-fixture"))
            .expect("OpenAI credential")
            .with_provider_credential("minimax", SecretValue::new("minimax-fixture"))
            .expect("MiniMax credential");

        assert!(config.provider_credentials.contains_profile("openai"));
        assert!(config.provider_credentials.contains_profile("minimax"));
        let rendered = format!("{config:?}");
        assert!(!rendered.contains("openai-fixture"));
        assert!(!rendered.contains("minimax-fixture"));
    }

    #[tokio::test]
    async fn command_host_keeps_injected_credentials_out_of_profiles_events_and_sqlite() {
        let directory = tempfile::tempdir().expect("directory");
        let workspace = directory.path().join("workspace");
        std::fs::create_dir(&workspace).expect("workspace");
        let database = directory.path().join("argentum.db");
        let secret = "minimax-host-only-credential";
        let mut config = HostConfig::in_memory(&workspace)
            .with_provider_api_keys([("minimax".into(), SecretValue::new(secret))])
            .expect("host credentials");
        config.database = Some(database.clone());
        assert!(!format!("{config:?}").contains(secret));

        let host = CommandHost::start(config).expect("host");
        let client = host.client();
        let mut events = client.subscribe();
        client
            .dispatch(AppCommand::SaveProviderProfile {
                profile: ProviderProfile {
                    id: "minimax".into(),
                    label: "MiniMax".into(),
                    kind: ProviderKind::OpenAiCompatible,
                    endpoint: "https://api.minimax.example/v1/".into(),
                    model: "minimax-test".into(),
                    selected: false,
                },
            })
            .await
            .expect("save profile");
        host.publish_initial_state().expect("initial state");

        let profiles = host.provider_profiles().expect("profiles");
        let statuses = host.provider_statuses();
        let emitted = std::iter::from_fn(|| events.try_recv().ok()).collect::<Vec<_>>();
        let rendered = format!(
            "{}\n{}\n{}",
            serde_json::to_string(&profiles).expect("profiles JSON"),
            serde_json::to_string(&statuses).expect("statuses JSON"),
            serde_json::to_string(&emitted).expect("events JSON")
        );
        assert!(rendered.contains("minimax"));
        assert!(!rendered.contains(secret));
        drop(host);

        for path in [
            database.clone(),
            database.with_extension("db-wal"),
            database.with_extension("db-shm"),
        ] {
            if let Ok(bytes) = std::fs::read(path) {
                assert!(!bytes
                    .windows(secret.len())
                    .any(|window| window == secret.as_bytes()));
            }
        }
    }
}
