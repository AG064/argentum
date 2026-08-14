use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use argentum_domain::{
    now, ActiveRunState, AppCommand, AppEvent, ApprovalRequest, ApprovalScope, ChangeSet,
    ConversationMessage, ConversationMessageStatus, ConversationRole, ConversationSnapshot,
    LayoutProfile, ModelUsage as DomainModelUsage, PlanStep, ProviderKind,
    ProviderModel as DomainProviderModel, ProviderProfile, ProviderStatus, Run, RunId, SessionId,
    Task, TaskLifecycle, ToolResultState, ToolTrace, WorkspaceSnapshot,
};
use argentum_providers::{
    normalize_provider_profile, ModelMessage, ModelMessageRole, ModelRequest, ModelToolCall,
    ModelToolDefinition, ModelToolExchange, ModelToolResult, ProviderError, ProviderEvent,
    ProviderRegistry,
};
use argentum_security::{ApprovalGrant, SecurityError};
use argentum_store::{EventScope, Store, StoreError};
use argentum_tools::{ToolContext, ToolError, ToolRegistry, ToolRequest, ToolResult};
use argentum_workspaces::WorkspaceManager;
use serde::Deserialize;
use thiserror::Error;
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_util::sync::CancellationToken;
use tracing::{error, warn};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error(transparent)]
    Store(#[from] StoreError),
    #[error(transparent)]
    Provider(#[from] ProviderError),
    #[error(transparent)]
    Security(#[from] SecurityError),
    #[error(transparent)]
    Tool(#[from] ToolError),
    #[error("no provider is configured")]
    NoProvider,
    #[error("task prompt must not be empty")]
    EmptyPrompt,
    #[error("workspace has no active session")]
    MissingActiveSession,
    #[error("{0}")]
    ProviderProbeFailed(String),
    #[error("{0}")]
    ProviderCatalogFailed(String),
    #[error("provider ID must use 1 to 64 ASCII letters, digits, dots, hyphens, or underscores")]
    InvalidProviderId,
    #[error("provider model must use 1 to 256 printable characters")]
    InvalidProviderModel,
    #[error("this session already has an active run")]
    SessionRunActive,
    #[error("runtime state lock was poisoned")]
    StateLockPoisoned,
    #[error("the model tool loop exceeded its safety limit")]
    ToolLoopLimit,
    #[error("the pending tool continuation closed before completion")]
    ToolContinuationClosed,
}

const MAX_MODEL_ROUNDS: usize = 8;
const MAX_MODEL_TOOL_CALLS_PER_ROUND: usize = 8;
const MAX_PROMPT_BYTES: usize = 128 * 1024;
const MAX_HISTORY_MESSAGES: usize = 128;
const MAX_HISTORY_BYTES: usize = 512 * 1024;
const MAX_VISIBLE_ASSISTANT_BYTES: usize = 512 * 1024;
const MAX_VISIBLE_REASONING_BYTES: usize = 512 * 1024;
const MAX_MODEL_TOOL_CALL_ID_BYTES: usize = 256;
const MAX_MODEL_TOOL_NAME_BYTES: usize = 64;
const MAX_MODEL_TOOL_ARGUMENT_BYTES: usize = 128 * 1024;
const MAX_MODEL_TOOL_PATH_BYTES: usize = 4 * 1024;
const MAX_MODEL_WRITE_BYTES: usize = 64 * 1024;
const MAX_MODEL_TOOL_RESULT_BYTES: usize = 64 * 1024;
const MAX_MODEL_CONTEXT_CACHE_ENTRIES: usize = 16_384;
const MAX_CACHED_PROVIDER_CATALOGS: usize = 256;

#[derive(Clone)]
pub struct RuntimeService {
    store: Store,
    providers: ProviderRegistry,
    tools: ToolRegistry,
    workspace: WorkspaceManager,
    workspace_key: String,
    project_id: Uuid,
    session_id: Arc<Mutex<SessionId>>,
    events: broadcast::Sender<AppEvent>,
    cancellation_tokens: Arc<Mutex<BTreeMap<RunId, CancellationToken>>>,
    run_sessions: Arc<Mutex<BTreeMap<RunId, SessionId>>>,
    active_session_runs: Arc<Mutex<BTreeMap<SessionId, RunId>>>,
    run_lifecycles: Arc<Mutex<BTreeMap<RunId, TaskLifecycle>>>,
    model_context_windows: Arc<Mutex<BTreeMap<(String, String), u64>>>,
    provider_model_catalogs: Arc<Mutex<BTreeMap<String, Vec<DomainProviderModel>>>>,
    pending_approvals: Arc<Mutex<BTreeMap<argentum_domain::ApprovalId, PendingTool>>>,
}

struct PendingTool {
    request: ToolRequest,
    approval: ApprovalRequest,
    continuation: PendingToolContinuation,
}

enum PendingToolContinuation {
    Manual,
    Model {
        provider_call_id: String,
        sender: oneshot::Sender<ModelToolStep>,
    },
}

enum ModelToolStep {
    Ready(ModelToolResult),
    Cancelled,
}

enum ModelRound {
    Completed {
        text: String,
        reasoning: String,
        tool_calls: Vec<ModelToolCall>,
    },
    Cancelled,
}

struct ModelRunContext {
    session_id: SessionId,
    run_id: RunId,
    profile_id: String,
    model: String,
    prompt: String,
    history: Vec<ModelMessage>,
    tools: Vec<ModelToolDefinition>,
    cancellation: CancellationToken,
}

#[derive(Default)]
struct ModelStreamState {
    assistant_text: String,
    assistant_reasoning: String,
    reasoning_bytes: usize,
    latest_usage: Option<DomainModelUsage>,
}

#[derive(Default)]
struct ModelRoundState {
    completed: bool,
    text: String,
    reasoning: String,
    tool_calls: Vec<ModelToolCall>,
    usage_seen: bool,
}

#[derive(Debug, Default)]
struct RunOutput {
    text: String,
    reasoning: String,
    usage: Option<DomainModelUsage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunCompletion {
    Complete,
    Cancelled,
}

impl std::fmt::Debug for RuntimeService {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("RuntimeService")
            .field("project_id", &self.project_id)
            .field("session_id", &self.session_id)
            .finish_non_exhaustive()
    }
}

impl RuntimeService {
    pub fn new(
        store: Store,
        providers: ProviderRegistry,
        tools: ToolRegistry,
        workspace: WorkspaceManager,
    ) -> Result<Self, RuntimeError> {
        Self::new_with_default_provider_profile(
            store,
            providers,
            tools,
            workspace,
            ProviderProfile::default_lm_studio(),
        )
    }

    pub fn new_with_default_provider_profile(
        store: Store,
        providers: ProviderRegistry,
        tools: ToolRegistry,
        workspace: WorkspaceManager,
        default_provider: ProviderProfile,
    ) -> Result<Self, RuntimeError> {
        let default_provider = normalize_provider_profile(default_provider)?;
        let resolution =
            store.resolve_workspace_with_default_provider(workspace.root(), &default_provider)?;
        let session_id = resolution
            .snapshot
            .active_session_id
            .ok_or(RuntimeError::MissingActiveSession)?;
        let (events, _) = broadcast::channel(256);
        Ok(Self {
            store,
            providers,
            tools,
            workspace,
            workspace_key: resolution.workspace_key,
            project_id: resolution.snapshot.project.id,
            session_id: Arc::new(Mutex::new(session_id)),
            events,
            cancellation_tokens: Arc::new(Mutex::new(BTreeMap::new())),
            run_sessions: Arc::new(Mutex::new(BTreeMap::new())),
            active_session_runs: Arc::new(Mutex::new(BTreeMap::new())),
            run_lifecycles: Arc::new(Mutex::new(BTreeMap::new())),
            model_context_windows: Arc::new(Mutex::new(BTreeMap::new())),
            provider_model_catalogs: Arc::new(Mutex::new(BTreeMap::new())),
            pending_approvals: Arc::new(Mutex::new(BTreeMap::new())),
        })
    }

    pub fn subscribe(&self) -> broadcast::Receiver<AppEvent> {
        self.events.subscribe()
    }

    pub fn provider_statuses(&self) -> Vec<argentum_domain::ProviderStatus> {
        self.provider_profiles()
            .unwrap_or_default()
            .iter()
            .filter_map(|profile| self.providers.status_for_profile(profile).ok())
            .collect()
    }

    pub fn publish_provider_statuses(&self) {
        for status in self.provider_statuses() {
            self.publish_transient(AppEvent::ProviderStatus(status));
        }
    }

    pub fn provider_profiles(&self) -> Result<Vec<ProviderProfile>, RuntimeError> {
        Ok(self.store.provider_profiles(self.project_id)?)
    }

    pub fn publish_provider_profiles(&self) -> Result<Vec<ProviderProfile>, RuntimeError> {
        let profiles = self.provider_profiles()?;
        self.publish_transient(AppEvent::ProviderProfilesSnapshot {
            profiles: profiles.clone(),
        });
        Ok(profiles)
    }

    pub fn publish_workspace_state(&self) -> Result<WorkspaceSnapshot, RuntimeError> {
        let snapshot = self.workspace_snapshot()?;
        self.publish_transient(AppEvent::WorkspaceStateLoaded(snapshot.clone()));
        if let Some(session_id) = snapshot.active_session_id {
            self.publish_conversation_snapshot(session_id)?;
        }
        self.publish_active_runs_snapshot()?;
        Ok(snapshot)
    }

    fn publish_active_runs_snapshot(&self) -> Result<(), RuntimeError> {
        let active_runs = self
            .active_session_runs
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?;
        let lifecycles = self
            .run_lifecycles
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?;
        let runs = active_runs
            .iter()
            .filter_map(|(session_id, run_id)| {
                lifecycles
                    .get(run_id)
                    .copied()
                    .filter(|lifecycle| !lifecycle.is_terminal())
                    .map(|lifecycle| ActiveRunState {
                        session_id: *session_id,
                        run_id: *run_id,
                        lifecycle,
                    })
            })
            .collect();
        drop(lifecycles);
        drop(active_runs);
        self.publish_transient(AppEvent::ActiveRunsSnapshot { runs });
        Ok(())
    }

    pub fn workspace_snapshot(&self) -> Result<WorkspaceSnapshot, RuntimeError> {
        Ok(self.store.workspace_snapshot(self.project_id)?)
    }

    pub fn conversation_snapshot(
        &self,
        session_id: SessionId,
    ) -> Result<ConversationSnapshot, RuntimeError> {
        Ok(self
            .store
            .conversation_snapshot(self.project_id, session_id)?)
    }

    fn publish_conversation_snapshot(
        &self,
        session_id: SessionId,
    ) -> Result<ConversationSnapshot, RuntimeError> {
        let snapshot = self.conversation_snapshot(session_id)?;
        self.publish_transient(AppEvent::ConversationSnapshotLoaded(snapshot.clone()));
        Ok(snapshot)
    }

    pub fn publish_layout(&self) -> Result<(), RuntimeError> {
        let profile = self.store.load_layout("default")?.unwrap_or_default();
        self.publish_transient(AppEvent::LayoutChanged(profile));
        Ok(())
    }

    pub async fn dispatch(&self, command: AppCommand) -> Result<(), RuntimeError> {
        match command {
            AppCommand::NewSession => {
                let session = self.store.create_session_with_event(
                    &self.workspace_key,
                    self.project_id,
                    "New session",
                )?;
                if let Ok(mut current) = self.session_id.lock() {
                    *current = session.id;
                }
                let _ = self.events.send(AppEvent::SessionCreated(session));
                self.publish_workspace_state()?;
                Ok(())
            }
            AppCommand::SelectSession { session_id } => {
                let snapshot = self.store.select_session(self.project_id, session_id)?;
                if let Ok(mut current) = self.session_id.lock() {
                    *current = session_id;
                }
                self.publish_transient(AppEvent::WorkspaceStateLoaded(snapshot));
                self.publish_conversation_snapshot(session_id)?;
                Ok(())
            }
            AppCommand::ProbeProvider { provider_id } => self.probe_provider(provider_id).await,
            AppCommand::ListProviderProfiles => {
                self.publish_provider_profiles()?;
                Ok(())
            }
            AppCommand::SaveProviderProfile { profile } => self.save_provider_profile(profile),
            AppCommand::SelectProviderProfile { provider_id } => {
                self.select_provider_profile(provider_id)
            }
            AppCommand::ListProviderModels { provider_id } => {
                self.list_provider_models(provider_id).await
            }
            AppCommand::SelectProviderModel { provider_id, model } => {
                self.select_provider_model(provider_id, model)
            }
            AppCommand::SubmitTask { prompt } => self.submit_task(prompt).await,
            AppCommand::RequestTool { request } => self.request_tool(request).await,
            AppCommand::CancelRun { run_id } => self.cancel(run_id),
            AppCommand::ApproveTool { approval_id, scope } => {
                self.approve_tool(approval_id, scope).await
            }
            AppCommand::RejectTool { approval_id } => self.reject_tool(approval_id),
            AppCommand::ToggleSurface { surface } => {
                let mut profile = self.store.load_layout("default")?.unwrap_or_default();
                let is_visible = profile.visible.entry(surface).or_insert(false);
                *is_visible = !*is_visible;
                self.store.save_layout("default", &profile)?;
                self.publish_transient(AppEvent::LayoutChanged(profile));
                Ok(())
            }
            AppCommand::SetLayout { profile } => {
                self.store.save_layout("default", &profile)?;
                self.publish_transient(AppEvent::LayoutChanged(profile));
                Ok(())
            }
            AppCommand::ResetLayout => {
                let profile = LayoutProfile::default();
                self.store.save_layout("default", &profile)?;
                self.publish_transient(AppEvent::LayoutChanged(profile));
                Ok(())
            }
        }
    }

    fn save_provider_profile(&self, profile: ProviderProfile) -> Result<(), RuntimeError> {
        let result = (|| {
            let profile = normalize_provider_profile(profile)?;
            let profiles = self
                .store
                .save_provider_profile(self.project_id, &profile)?;
            self.clear_profile_model_contexts(&profile.id)?;
            self.clear_profile_model_catalog(&profile.id)?;
            self.publish_transient(AppEvent::ProviderProfilesSnapshot { profiles });
            Ok(())
        })();
        self.publish_profile_command_error(&result);
        result
    }

    fn select_provider_profile(&self, provider_id: String) -> Result<(), RuntimeError> {
        let contains_control = provider_id.chars().any(char::is_control);
        let provider_id = provider_id.trim();
        let result = if contains_control || !valid_provider_id(provider_id) {
            Err(RuntimeError::InvalidProviderId)
        } else {
            self.store
                .select_provider_profile(self.project_id, provider_id)
                .map(|profiles| {
                    self.publish_transient(AppEvent::ProviderProfilesSnapshot { profiles });
                })
                .map_err(RuntimeError::from)
        };
        self.publish_profile_command_error(&result);
        result
    }

    async fn list_provider_models(&self, provider_id: String) -> Result<(), RuntimeError> {
        let contains_control = provider_id.chars().any(char::is_control);
        let provider_id = provider_id.trim();
        if contains_control || !valid_provider_id(provider_id) {
            let error_value = RuntimeError::InvalidProviderId;
            self.publish_profile_command_error(&Err(error_value));
            return Err(RuntimeError::InvalidProviderId);
        }
        let profile = self
            .store
            .provider_profiles(self.project_id)?
            .into_iter()
            .find(|profile| profile.id == provider_id);
        let Some(profile) = profile else {
            let detail = "Provider profile is not available in this workspace.".to_owned();
            self.publish_transient(AppEvent::Error {
                message: detail.clone(),
                recoverable: true,
            });
            return Err(RuntimeError::ProviderCatalogFailed(detail));
        };
        match self.providers.list_models_for_profile(&profile).await {
            Ok(models) => {
                let models = models
                    .into_iter()
                    .map(|model| DomainProviderModel {
                        id: model.id,
                        label: model.label,
                        context_window_tokens: model.context_window_tokens,
                    })
                    .collect::<Vec<_>>();
                self.replace_profile_model_contexts(&profile.id, &models)?;
                self.replace_profile_model_catalog(&profile.id, &models)?;
                self.publish_transient(AppEvent::ProviderModelsSnapshot {
                    provider_id: profile.id,
                    models,
                    selected_model: profile.model,
                });
                Ok(())
            }
            Err(error_value) => {
                let detail = provider_catalog_failure_detail(&profile.label, &error_value);
                self.publish_transient(AppEvent::Error {
                    message: detail.clone(),
                    recoverable: true,
                });
                Err(RuntimeError::ProviderCatalogFailed(detail))
            }
        }
    }

    fn select_provider_model(
        &self,
        provider_id: String,
        model: String,
    ) -> Result<(), RuntimeError> {
        let provider_contains_control = provider_id.chars().any(char::is_control);
        let provider_id = provider_id.trim();
        let model_contains_control = model.chars().any(char::is_control);
        let model = model.trim();
        let result = if provider_contains_control || !valid_provider_id(provider_id) {
            Err(RuntimeError::InvalidProviderId)
        } else if model_contains_control || !valid_provider_model(model) {
            Err(RuntimeError::InvalidProviderModel)
        } else {
            (|| {
                if self
                    .cached_profile_model_catalog(provider_id)?
                    .is_some_and(|models| !models.iter().any(|entry| entry.id == model))
                {
                    return Err(RuntimeError::InvalidProviderModel);
                }
                let profiles =
                    self.store
                        .select_provider_model(self.project_id, provider_id, model)?;
                self.publish_transient(AppEvent::ProviderProfilesSnapshot { profiles });
                if let Some(models) = self.cached_profile_model_catalog(provider_id)? {
                    self.publish_transient(AppEvent::ProviderModelsSnapshot {
                        provider_id: provider_id.to_owned(),
                        models,
                        selected_model: model.to_owned(),
                    });
                }
                Ok(())
            })()
        };
        self.publish_profile_command_error(&result);
        result
    }

    fn clear_profile_model_contexts(&self, profile_id: &str) -> Result<(), RuntimeError> {
        let mut cache = self
            .model_context_windows
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?;
        cache.retain(|(cached_profile_id, _), _| cached_profile_id != profile_id);
        Ok(())
    }

    fn clear_profile_model_catalog(&self, profile_id: &str) -> Result<(), RuntimeError> {
        self.provider_model_catalogs
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?
            .remove(profile_id);
        Ok(())
    }

    fn replace_profile_model_catalog(
        &self,
        profile_id: &str,
        models: &[DomainProviderModel],
    ) -> Result<(), RuntimeError> {
        let mut catalogs = self
            .provider_model_catalogs
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?;
        if !catalogs.contains_key(profile_id) && catalogs.len() >= MAX_CACHED_PROVIDER_CATALOGS {
            catalogs.pop_first();
        }
        catalogs.insert(profile_id.to_owned(), models.to_vec());
        Ok(())
    }

    fn cached_profile_model_catalog(
        &self,
        profile_id: &str,
    ) -> Result<Option<Vec<DomainProviderModel>>, RuntimeError> {
        self.provider_model_catalogs
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)
            .map(|catalogs| catalogs.get(profile_id).cloned())
    }

    fn replace_profile_model_contexts(
        &self,
        profile_id: &str,
        models: &[DomainProviderModel],
    ) -> Result<(), RuntimeError> {
        let entries = models
            .iter()
            .filter_map(|model| {
                model
                    .context_window_tokens
                    .map(|tokens| ((profile_id.to_owned(), model.id.clone()), tokens))
            })
            .take(MAX_MODEL_CONTEXT_CACHE_ENTRIES)
            .collect::<Vec<_>>();
        let mut cache = self
            .model_context_windows
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?;
        cache.retain(|(cached_profile_id, _), _| cached_profile_id != profile_id);
        while cache.len().saturating_add(entries.len()) > MAX_MODEL_CONTEXT_CACHE_ENTRIES {
            if cache.pop_first().is_none() {
                break;
            }
        }
        cache.extend(entries);
        Ok(())
    }

    fn model_context_window(
        &self,
        profile_id: &str,
        model: &str,
    ) -> Result<Option<u64>, RuntimeError> {
        self.model_context_windows
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)
            .map(|cache| {
                cache
                    .get(&(profile_id.to_owned(), model.to_owned()))
                    .copied()
            })
    }

    fn publish_profile_command_error(&self, result: &Result<(), RuntimeError>) {
        if let Err(error_value) = result {
            self.publish_transient(AppEvent::Error {
                message: error_value.to_string(),
                recoverable: true,
            });
        }
    }

    async fn probe_provider(&self, provider_id: String) -> Result<(), RuntimeError> {
        let contains_control = provider_id.chars().any(char::is_control);
        let provider_id = provider_id.trim();
        if contains_control || !valid_provider_id(provider_id) {
            let error_value = RuntimeError::InvalidProviderId;
            self.publish_transient(AppEvent::Error {
                message: error_value.to_string(),
                recoverable: true,
            });
            return Err(error_value);
        }
        let display_id = display_provider_id(provider_id);
        let profile = self
            .store
            .provider_profiles(self.project_id)?
            .into_iter()
            .find(|profile| profile.id == provider_id);
        let baseline = match &profile {
            Some(profile) => self.providers.status_for_profile(profile)?,
            None => self
                .providers
                .status(provider_id)
                .unwrap_or_else(|| unavailable_provider_status(provider_id, &display_id)),
        };
        let probe_result = match &profile {
            Some(profile) => self.providers.probe_profile(profile).await,
            None => self.providers.probe(provider_id).await,
        };

        match probe_result {
            Ok(status) => {
                self.publish_transient(AppEvent::ProviderStatus(status));
                Ok(())
            }
            Err(error_value) => {
                let detail = provider_probe_failure_detail(&baseline.label, &error_value);
                let mut status = baseline;
                status.connected = false;
                status.detail = detail.clone();
                self.publish_transient(AppEvent::ProviderStatus(status));
                self.publish_transient(AppEvent::Error {
                    message: detail.clone(),
                    recoverable: true,
                });
                Err(RuntimeError::ProviderProbeFailed(detail))
            }
        }
    }

    async fn submit_task(&self, prompt: String) -> Result<(), RuntimeError> {
        let prompt = prompt.trim().to_owned();
        if prompt.is_empty() {
            self.emit(AppEvent::Error {
                message: "Enter a task before selecting Start.".into(),
                recoverable: true,
            })?;
            return Err(RuntimeError::EmptyPrompt);
        }
        if prompt.len() > MAX_PROMPT_BYTES {
            self.publish_transient(AppEvent::Error {
                message: "Task input exceeded the safety limit.".into(),
                recoverable: true,
            });
            return Err(ProviderError::InvalidResponse(
                "task input exceeded the safety limit".into(),
            )
            .into());
        }

        let session_id = self.session_id();
        let run_id = Uuid::new_v4();
        if let Err(error_value) = self.acquire_session_run(session_id, run_id) {
            self.publish_transient(AppEvent::Error {
                message: error_value.to_string(),
                recoverable: true,
            });
            return Err(error_value);
        }
        let result = self.submit_task_for_run(session_id, run_id, prompt).await;
        self.clear_run_state(run_id, session_id);
        result
    }

    async fn submit_task_for_run(
        &self,
        session_id: SessionId,
        run_id: RunId,
        prompt: String,
    ) -> Result<(), RuntimeError> {
        let history = self
            .store
            .conversation_snapshot(self.project_id, session_id)?;
        let task = Task {
            id: Uuid::new_v4(),
            session_id,
            prompt: prompt.clone(),
            lifecycle: TaskLifecycle::Queued,
            created_at: now(),
        };
        let task_id = task.id;
        if let Some(snapshot) = self.store.title_default_session_from_prompt(
            self.project_id,
            task.session_id,
            &prompt,
        )? {
            self.publish_transient(AppEvent::WorkspaceStateLoaded(snapshot));
        }
        self.cancellation_tokens
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?
            .insert(run_id, CancellationToken::new());
        self.run_sessions
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?
            .insert(run_id, task.session_id);
        self.store
            .append_conversation_message(&ConversationMessage {
                id: Uuid::new_v4(),
                project_id: self.project_id,
                session_id,
                run_id,
                role: ConversationRole::User,
                text: prompt.clone(),
                reasoning: String::new(),
                usage: None,
                profile_id: String::new(),
                model: String::new(),
                status: ConversationMessageStatus::Complete,
                created_at: now(),
            })?;
        self.emit(AppEvent::TaskAccepted(task))?;
        self.emit(AppEvent::RunStatusChanged {
            run_id,
            lifecycle: TaskLifecycle::Planning,
        })?;

        let steps = vec![
            PlanStep {
                id: "understand".into(),
                title: "Prepare".into(),
                detail: "Create the run context for this workspace".into(),
                complete: true,
                blocked: false,
            },
            PlanStep {
                id: "execute".into(),
                title: "Request".into(),
                detail: "Stream a response from the selected model".into(),
                complete: false,
                blocked: false,
            },
            PlanStep {
                id: "verify".into(),
                title: "Finish".into(),
                detail: "Record the response and final run state".into(),
                complete: false,
                blocked: false,
            },
        ];
        self.emit(AppEvent::PlanUpdated {
            run_id,
            steps: steps.clone(),
        })?;
        let run = Run {
            id: run_id,
            task_id,
            lifecycle: TaskLifecycle::Planning,
            steps,
            started_at: None,
            finished_at: None,
        };
        let _ = run;
        self.run_task(run_id, session_id, prompt, history).await
    }

    async fn run_task(
        &self,
        run_id: RunId,
        session_id: SessionId,
        prompt: String,
        history: ConversationSnapshot,
    ) -> Result<(), RuntimeError> {
        let profile = self.store.selected_provider_profile(self.project_id)?;
        let provider = profile
            .as_ref()
            .ok_or(RuntimeError::NoProvider)
            .and_then(|profile| {
                self.providers
                    .provider_for_profile(profile)
                    .map_err(RuntimeError::from)
            });
        let provider = match provider {
            Ok(provider) => provider,
            Err(error_value) => {
                self.publish_conversation_snapshot(session_id)?;
                self.fail_run(run_id, &error_value)?;
                return Err(error_value);
            }
        };
        let profile = profile.ok_or(RuntimeError::NoProvider)?;
        let profile_id = profile.id.clone();
        let model = profile.model.clone();

        let cancellation = self
            .cancellation_tokens
            .lock()
            .ok()
            .and_then(|tokens| tokens.get(&run_id).cloned())
            .unwrap_or_default();

        self.emit(AppEvent::RunStatusChanged {
            run_id,
            lifecycle: TaskLifecycle::Running,
        })?;
        let history = bounded_model_history(
            history
                .messages
                .into_iter()
                .filter(|message| message.status == ConversationMessageStatus::Complete)
                .map(|message| ModelMessage {
                    role: match message.role {
                        ConversationRole::User => ModelMessageRole::User,
                        ConversationRole::Assistant => ModelMessageRole::Assistant,
                    },
                    content: message.text,
                })
                .collect::<Vec<_>>(),
        );
        let tools = if matches!(
            profile.kind,
            ProviderKind::OpenAiCompatible | ProviderKind::LocalLmStudio
        ) {
            self.model_tool_definitions()
        } else {
            Vec::new()
        };
        let model_run = ModelRunContext {
            session_id,
            run_id,
            profile_id: profile_id.clone(),
            model: model.clone(),
            prompt,
            history,
            tools,
            cancellation,
        };
        let (result, output) = self.run_model_loop(provider, model_run).await;
        match result {
            Ok(RunCompletion::Complete) => {
                self.persist_assistant_message(
                    session_id,
                    run_id,
                    output,
                    &profile_id,
                    &model,
                    ConversationMessageStatus::Complete,
                )?;
                self.publish_conversation_snapshot(session_id)?;
                self.finish_run(run_id)
            }
            Ok(RunCompletion::Cancelled) => {
                self.persist_assistant_message(
                    session_id,
                    run_id,
                    output,
                    &profile_id,
                    &model,
                    ConversationMessageStatus::Interrupted,
                )?;
                self.publish_conversation_snapshot(session_id)?;
                self.emit(AppEvent::RunStatusChanged {
                    run_id,
                    lifecycle: TaskLifecycle::Cancelled,
                })
            }
            Err(error_value) => {
                self.persist_assistant_message(
                    session_id,
                    run_id,
                    output,
                    &profile_id,
                    &model,
                    ConversationMessageStatus::Failed,
                )?;
                self.publish_conversation_snapshot(session_id)?;
                self.fail_run(run_id, &error_value)?;
                Err(error_value)
            }
        }
    }

    async fn run_model_loop(
        &self,
        provider: Arc<dyn argentum_providers::ModelProvider>,
        context: ModelRunContext,
    ) -> (Result<RunCompletion, RuntimeError>, RunOutput) {
        let mut stream = ModelStreamState::default();
        let mut exchanges = Vec::new();
        let result = async {
            for round_index in 0..MAX_MODEL_ROUNDS {
                if context.cancellation.is_cancelled() {
                    return Ok(RunCompletion::Cancelled);
                }
                stream.latest_usage = None;
                let system = if context.tools.is_empty() {
                    "You are Argentum, a precise agent harness. Be concise, state assumptions, and never claim a change you did not make."
                } else {
                    "You are Argentum, a precise agent harness. Be concise, state assumptions, and never claim a change you did not make. You may use only the listed workspace tools. Tool paths must be relative to the workspace. File writes always require explicit user approval."
                };
                let request = ModelRequest {
                    model: context.model.clone(),
                    system: Some(system.into()),
                    history: context.history.clone(),
                    prompt: context.prompt.clone(),
                    tools: context.tools.clone(),
                    tool_exchanges: exchanges.clone(),
                };
                let round = self
                    .stream_model_round(provider.as_ref(), &context, request, &mut stream)
                    .await?;
                let (round_text, round_reasoning, tool_calls) = match round {
                    ModelRound::Completed {
                        text,
                        reasoning,
                        tool_calls,
                    } => (text, reasoning, tool_calls),
                    ModelRound::Cancelled => return Ok(RunCompletion::Cancelled),
                };
                if tool_calls.is_empty() {
                    return Ok(RunCompletion::Complete);
                }
                if round_index + 1 >= MAX_MODEL_ROUNDS {
                    return Err(RuntimeError::ToolLoopLimit);
                }

                let mut results = Vec::with_capacity(tool_calls.len());
                let mut call_ids = std::collections::BTreeSet::new();
                for call in &tool_calls {
                    validate_runtime_model_tool_call(call)?;
                    if !call_ids.insert(call.id.as_str()) {
                        return Err(ProviderError::InvalidResponse(
                            "provider returned duplicate tool-call identifiers".into(),
                        )
                        .into());
                    }
                    match self
                        .execute_model_tool_call(
                            context.run_id,
                            call.clone(),
                            &context.cancellation,
                        )
                        .await?
                    {
                        ModelToolStep::Ready(result) => results.push(result),
                        ModelToolStep::Cancelled => return Ok(RunCompletion::Cancelled),
                    }
                }
                exchanges.push(ModelToolExchange {
                    assistant_content: round_text,
                    assistant_reasoning: round_reasoning,
                    calls: tool_calls,
                    results,
                });
            }
            Err(RuntimeError::ToolLoopLimit)
        }
        .await;
        (
            result,
            RunOutput {
                text: stream.assistant_text,
                reasoning: stream.assistant_reasoning,
                usage: stream.latest_usage,
            },
        )
    }

    async fn stream_model_round(
        &self,
        provider: &dyn argentum_providers::ModelProvider,
        context: &ModelRunContext,
        request: ModelRequest,
        stream: &mut ModelStreamState,
    ) -> Result<ModelRound, RuntimeError> {
        let (sender, mut receiver) = mpsc::channel(32);
        let provider_future = provider.stream(request, sender);
        tokio::pin!(provider_future);
        let mut receiver_open = true;
        let mut round = ModelRoundState::default();

        loop {
            tokio::select! {
                _ = context.cancellation.cancelled() => return Ok(ModelRound::Cancelled),
                provider_result = &mut provider_future => {
                    while let Ok(event) = receiver.try_recv() {
                        self.apply_provider_event(context, event, &mut round, stream)?;
                    }
                    return match provider_result {
                        Ok(()) if round.completed => Ok(ModelRound::Completed {
                            text: round.text,
                            reasoning: round.reasoning,
                            tool_calls: round.tool_calls,
                        }),
                        Ok(()) => Err(ProviderError::InvalidResponse(
                            "provider stream ended before completion".into(),
                        ).into()),
                        Err(error_value) => Err(error_value.into()),
                    };
                }
                event = receiver.recv(), if receiver_open => {
                    match event {
                        Some(event) => self.apply_provider_event(context, event, &mut round, stream)?,
                        None => receiver_open = false,
                    }
                }
            }
        }
    }

    fn apply_provider_event(
        &self,
        context: &ModelRunContext,
        event: ProviderEvent,
        round: &mut ModelRoundState,
        stream: &mut ModelStreamState,
    ) -> Result<(), RuntimeError> {
        match event {
            ProviderEvent::Delta(delta) if !round.completed => {
                append_visible_text(&mut stream.assistant_text, &delta)?;
                round.text.push_str(&delta);
                self.publish_transient(AppEvent::AssistantDelta {
                    run_id: context.run_id,
                    text: delta,
                });
            }
            ProviderEvent::ReasoningDelta(delta) if !round.completed => {
                append_bounded_fragment(
                    &mut stream.assistant_reasoning,
                    &mut stream.reasoning_bytes,
                    &delta,
                    MAX_VISIBLE_REASONING_BYTES,
                    "provider reasoning exceeded the safety limit",
                )?;
                round.reasoning.push_str(&delta);
                self.publish_transient(AppEvent::AssistantReasoningDelta {
                    run_id: context.run_id,
                    text: delta,
                });
            }
            ProviderEvent::Usage(usage) if !round.completed => {
                if round.usage_seen {
                    return Err(ProviderError::InvalidResponse(
                        "provider emitted duplicate usage for one response".into(),
                    )
                    .into());
                }
                round.usage_seen = true;
                let usage = DomainModelUsage {
                    input_tokens: usage.input_tokens,
                    output_tokens: usage.output_tokens,
                    total_tokens: usage.total_tokens,
                    reasoning_tokens: usage.reasoning_tokens,
                    cached_input_tokens: usage.cached_input_tokens,
                    context_window_tokens: usage
                        .context_window_tokens
                        .or(self.model_context_window(&context.profile_id, &context.model)?),
                };
                stream.latest_usage = Some(usage.clone());
                self.publish_transient(AppEvent::ModelUsageUpdated {
                    session_id: context.session_id,
                    run_id: context.run_id,
                    profile_id: context.profile_id.clone(),
                    model: context.model.clone(),
                    usage,
                });
            }
            ProviderEvent::ToolCall(call) if !round.completed => {
                if round.tool_calls.len() >= MAX_MODEL_TOOL_CALLS_PER_ROUND {
                    return Err(ProviderError::InvalidResponse(
                        "provider returned too many tool calls".into(),
                    )
                    .into());
                }
                round.tool_calls.push(call);
            }
            ProviderEvent::Completed if !round.completed => round.completed = true,
            ProviderEvent::Delta(_)
            | ProviderEvent::ReasoningDelta(_)
            | ProviderEvent::ToolCall(_)
            | ProviderEvent::Usage(_)
            | ProviderEvent::Completed => {
                return Err(ProviderError::InvalidResponse(
                    "provider emitted events after completion".into(),
                )
                .into());
            }
        }
        Ok(())
    }

    fn model_tool_definitions(&self) -> Vec<ModelToolDefinition> {
        self.tools
            .descriptors()
            .into_iter()
            .filter_map(|descriptor| match descriptor.id.as_str() {
                "read_text"
                    if descriptor.capabilities.as_slice() == [argentum_domain::Capability::ReadFiles] =>
                {
                    Some(ModelToolDefinition {
                        name: "read_text".into(),
                        description: "Read a bounded UTF-8 text file inside the active workspace."
                            .into(),
                        parameters: serde_json::json!({
                            "type": "object",
                            "properties": {
                                "path": { "type": "string", "maxLength": MAX_MODEL_TOOL_PATH_BYTES }
                            },
                            "required": ["path"],
                            "additionalProperties": false
                        }),
                    })
                }
                "write_text"
                    if descriptor.requires_approval
                        && descriptor.capabilities.as_slice()
                            == [argentum_domain::Capability::WriteFiles] =>
                {
                    Some(ModelToolDefinition {
                        name: "write_text".into(),
                        description: "Write bounded UTF-8 text to a file inside the active workspace after user approval."
                            .into(),
                        parameters: serde_json::json!({
                            "type": "object",
                            "properties": {
                                "path": { "type": "string", "maxLength": MAX_MODEL_TOOL_PATH_BYTES },
                                "content": { "type": "string", "maxLength": MAX_MODEL_WRITE_BYTES }
                            },
                            "required": ["path", "content"],
                            "additionalProperties": false
                        }),
                    })
                }
                _ => None,
            })
            .collect()
    }

    async fn execute_model_tool_call(
        &self,
        run_id: RunId,
        call: ModelToolCall,
        cancellation: &CancellationToken,
    ) -> Result<ModelToolStep, RuntimeError> {
        let input = match parse_model_tool_input(&call) {
            Ok(input) => input,
            Err(content) => {
                return Ok(ModelToolStep::Ready(ModelToolResult {
                    call_id: call.id,
                    content,
                }));
            }
        };
        let request = ToolRequest {
            call_id: Uuid::new_v4(),
            run_id,
            input,
        };
        let descriptor = self.tool_descriptor(&request)?;
        if !self.tool_needs_approval(&descriptor)? {
            return self
                .execute_model_tool_request(request, call.id, ApprovalGrant::default())
                .await;
        }

        let approval = self.approval_request(&request, descriptor);
        let approval_id = approval.id;
        let (sender, receiver) = oneshot::channel();
        self.pending_approvals
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?
            .insert(
                approval_id,
                PendingTool {
                    request,
                    approval: approval.clone(),
                    continuation: PendingToolContinuation::Model {
                        provider_call_id: call.id,
                        sender,
                    },
                },
            );
        self.emit(AppEvent::ApprovalRequested(approval))?;
        self.emit(AppEvent::RunStatusChanged {
            run_id,
            lifecycle: TaskLifecycle::WaitingForApproval,
        })?;

        tokio::select! {
            _ = cancellation.cancelled() => Ok(ModelToolStep::Cancelled),
            result = receiver => {
                let result = result.map_err(|_| RuntimeError::ToolContinuationClosed)?;
                if matches!(result, ModelToolStep::Ready(_)) {
                    self.emit(AppEvent::RunStatusChanged {
                        run_id,
                        lifecycle: TaskLifecycle::Running,
                    })?;
                }
                Ok(result)
            }
        }
    }

    fn finish_run(&self, run_id: RunId) -> Result<(), RuntimeError> {
        self.emit(AppEvent::ChangeSetReady(ChangeSet {
            run_id,
            files_changed: 0,
            additions: 0,
            removals: 0,
            verification_ready: false,
        }))?;
        self.emit(AppEvent::RunStatusChanged {
            run_id,
            lifecycle: TaskLifecycle::Complete,
        })
    }

    fn fail_run(&self, run_id: RunId, error_value: &RuntimeError) -> Result<(), RuntimeError> {
        error!(run_id = %run_id, error = %error_value, "agent run failed");
        self.emit(AppEvent::RunStatusChanged {
            run_id,
            lifecycle: TaskLifecycle::Failed,
        })?;
        self.emit_run_error(run_id, error_value.to_string(), true)
    }

    fn persist_assistant_message(
        &self,
        session_id: SessionId,
        run_id: RunId,
        output: RunOutput,
        profile_id: &str,
        model: &str,
        status: ConversationMessageStatus,
    ) -> Result<(), RuntimeError> {
        if output.text.is_empty() && output.reasoning.is_empty() && output.usage.is_none() {
            return Ok(());
        }
        self.store
            .append_conversation_message(&ConversationMessage {
                id: Uuid::new_v4(),
                project_id: self.project_id,
                session_id,
                run_id,
                role: ConversationRole::Assistant,
                text: output.text,
                reasoning: output.reasoning,
                usage: output.usage,
                profile_id: profile_id.to_owned(),
                model: model.to_owned(),
                status,
                created_at: now(),
            })?;
        Ok(())
    }

    fn acquire_session_run(
        &self,
        session_id: SessionId,
        run_id: RunId,
    ) -> Result<(), RuntimeError> {
        let mut active_runs = self
            .active_session_runs
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?;
        if active_runs.contains_key(&session_id) {
            return Err(RuntimeError::SessionRunActive);
        }
        let mut lifecycles = self
            .run_lifecycles
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?;
        active_runs.insert(session_id, run_id);
        lifecycles.insert(run_id, TaskLifecycle::Queued);
        Ok(())
    }

    fn cancel(&self, run_id: RunId) -> Result<(), RuntimeError> {
        let token = self
            .cancellation_tokens
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?
            .get(&run_id)
            .cloned();
        let pending = self.take_pending_for_run(run_id)?;
        let mut first_error = None;
        for (approval_id, pending) in pending {
            let PendingTool {
                request,
                approval,
                continuation,
            } = pending;
            for event in [
                AppEvent::ApprovalResolved {
                    approval_id,
                    approved: false,
                },
                AppEvent::ToolFinished(ToolTrace {
                    id: request.call_id,
                    run_id,
                    tool_id: approval.tool_id,
                    summary: approval.target,
                    result: ToolResultState::Cancelled,
                    duration_ms: Some(0),
                }),
            ] {
                if let Err(error_value) = self.emit_for_run(event, run_id) {
                    if first_error.is_none() {
                        first_error = Some(error_value);
                    }
                }
            }
            if let PendingToolContinuation::Model { sender, .. } = continuation {
                let _ = sender.send(ModelToolStep::Cancelled);
            }
        }
        if let Some(token) = token {
            token.cancel();
        } else {
            warn!(run_id = %run_id, "cancellation requested for an inactive run");
        }
        first_error.map_or(Ok(()), Err)
    }

    fn clear_run_state(&self, run_id: RunId, session_id: SessionId) {
        if let Ok(mut tokens) = self.cancellation_tokens.lock() {
            tokens.remove(&run_id);
        }
        if let Ok(mut sessions) = self.run_sessions.lock() {
            sessions.remove(&run_id);
        }
        if let Ok(mut active_runs) = self.active_session_runs.lock() {
            if active_runs.get(&session_id) == Some(&run_id) {
                active_runs.remove(&session_id);
            }
        }
        if let Ok(mut lifecycles) = self.run_lifecycles.lock() {
            lifecycles.remove(&run_id);
        }
        if let Ok(pending) = self.take_pending_for_run(run_id) {
            for (_, pending) in pending {
                if let PendingToolContinuation::Model { sender, .. } = pending.continuation {
                    let _ = sender.send(ModelToolStep::Cancelled);
                }
            }
        }
    }

    fn emit(&self, event: AppEvent) -> Result<(), RuntimeError> {
        if let AppEvent::RunStatusChanged { run_id, lifecycle } = &event {
            let mut lifecycles = self
                .run_lifecycles
                .lock()
                .map_err(|_| RuntimeError::StateLockPoisoned)?;
            if lifecycle.is_terminal() {
                lifecycles.remove(run_id);
            } else {
                lifecycles.insert(*run_id, *lifecycle);
            }
        }
        let run_id = event_run_id(&event);
        let session_id = match &event {
            AppEvent::SessionCreated(session) => Some(session.id),
            AppEvent::TaskAccepted(task) => Some(task.session_id),
            _ => run_id
                .and_then(|run_id| self.session_for_run(run_id))
                .or_else(|| Some(self.session_id())),
        };
        self.emit_scoped(event, session_id, run_id)
    }

    fn emit_for_run(&self, event: AppEvent, run_id: RunId) -> Result<(), RuntimeError> {
        let session_id = self
            .session_for_run(run_id)
            .or_else(|| Some(self.session_id()));
        self.emit_scoped(event, session_id, Some(run_id))
    }

    fn emit_run_error(
        &self,
        run_id: RunId,
        message: String,
        recoverable: bool,
    ) -> Result<(), RuntimeError> {
        let session_id = self
            .session_for_run(run_id)
            .unwrap_or_else(|| self.session_id());
        self.emit_scoped(
            AppEvent::RunError {
                session_id,
                run_id,
                message,
                recoverable,
            },
            Some(session_id),
            Some(run_id),
        )
    }

    fn emit_scoped(
        &self,
        event: AppEvent,
        session_id: Option<SessionId>,
        run_id: Option<RunId>,
    ) -> Result<(), RuntimeError> {
        self.store.append_event_scoped(
            &EventScope {
                workspace_key: self.workspace_key.clone(),
                project_id: self.project_id,
                session_id,
                run_id,
            },
            &event,
        )?;
        let _ = self.events.send(event);
        Ok(())
    }

    fn publish_transient(&self, event: AppEvent) {
        let _ = self.events.send(event);
    }

    fn session_for_run(&self, run_id: RunId) -> Option<SessionId> {
        self.run_sessions
            .lock()
            .ok()
            .and_then(|sessions| sessions.get(&run_id).copied())
    }

    pub fn tool_count(&self) -> usize {
        self.tools.descriptors().len()
    }

    pub async fn request_tool(&self, request: ToolRequest) -> Result<(), RuntimeError> {
        if let Ok(mut sessions) = self.run_sessions.lock() {
            sessions
                .entry(request.run_id)
                .or_insert_with(|| self.session_id());
        }
        let descriptor = self.tool_descriptor(&request)?;
        if self.tool_needs_approval(&descriptor)? {
            let approval = self.approval_request(&request, descriptor);
            self.pending_approvals
                .lock()
                .map_err(|_| RuntimeError::StateLockPoisoned)?
                .insert(
                    approval.id,
                    PendingTool {
                        request,
                        approval: approval.clone(),
                        continuation: PendingToolContinuation::Manual,
                    },
                );
            self.emit(AppEvent::ApprovalRequested(approval))?;
            return Ok(());
        }
        self.execute_tool(request, ApprovalGrant::default()).await
    }

    fn tool_descriptor(
        &self,
        request: &ToolRequest,
    ) -> Result<argentum_domain::ToolDescriptor, RuntimeError> {
        self.tools
            .descriptors()
            .into_iter()
            .find(|descriptor| match &request.input {
                argentum_tools::ToolInput::ReadText { .. } => descriptor.id == "read_text",
                argentum_tools::ToolInput::WriteText { .. } => descriptor.id == "write_text",
            })
            .ok_or_else(|| ToolError::NotRegistered("requested tool".into()).into())
    }

    fn tool_needs_approval(
        &self,
        descriptor: &argentum_domain::ToolDescriptor,
    ) -> Result<bool, RuntimeError> {
        let policy_requires_approval = descriptor
            .capabilities
            .iter()
            .try_fold(false, |requires_approval, capability| {
                Ok::<_, argentum_workspaces::WorkspaceError>(
                    requires_approval || self.workspace.requires_approval(*capability)?,
                )
            })
            .map_err(ToolError::from)?;
        Ok(descriptor.requires_approval || policy_requires_approval)
    }

    fn approval_request(
        &self,
        request: &ToolRequest,
        descriptor: argentum_domain::ToolDescriptor,
    ) -> ApprovalRequest {
        let (action, target, reason) = match &request.input {
            argentum_tools::ToolInput::ReadText { path } => (
                "Read file".to_owned(),
                path.clone(),
                "Argentum requested permission to read this file.".to_owned(),
            ),
            argentum_tools::ToolInput::WriteText { path, .. } => (
                "Write file".to_owned(),
                path.clone(),
                "Argentum requested permission to write to this file.".to_owned(),
            ),
        };
        ApprovalRequest {
            id: Uuid::new_v4(),
            run_id: request.run_id,
            tool_id: descriptor.id,
            action,
            target,
            reason,
            capabilities: descriptor.capabilities,
            expires_at: None,
        }
    }

    async fn approve_tool(
        &self,
        approval_id: argentum_domain::ApprovalId,
        _scope: ApprovalScope,
    ) -> Result<(), RuntimeError> {
        let Some(pending) = self.take_pending(approval_id) else {
            self.emit(AppEvent::Error {
                message: "No approval is currently pending".into(),
                recoverable: true,
            })?;
            return Ok(());
        };
        let PendingTool {
            request,
            approval,
            continuation,
        } = pending;
        self.emit_for_run(
            AppEvent::ApprovalResolved {
                approval_id,
                approved: true,
            },
            request.run_id,
        )?;
        let grant = ApprovalGrant::for_capabilities(approval.capabilities);
        match continuation {
            PendingToolContinuation::Manual => self.execute_tool(request, grant).await,
            PendingToolContinuation::Model {
                provider_call_id,
                sender,
            } => {
                let fallback_call_id = provider_call_id.clone();
                let result = self
                    .execute_model_tool_request(request, provider_call_id, grant)
                    .await;
                match result {
                    Ok(step) => {
                        let _ = sender.send(step);
                        Ok(())
                    }
                    Err(error_value) => {
                        let _ = sender.send(ModelToolStep::Ready(ModelToolResult {
                            call_id: fallback_call_id,
                            content: "Tool execution could not be completed safely.".into(),
                        }));
                        Err(error_value)
                    }
                }
            }
        }
    }

    fn reject_tool(&self, approval_id: argentum_domain::ApprovalId) -> Result<(), RuntimeError> {
        let Some(pending) = self.take_pending(approval_id) else {
            self.emit(AppEvent::Error {
                message: "No approval is currently pending".into(),
                recoverable: true,
            })?;
            return Ok(());
        };
        let PendingTool {
            request,
            approval,
            continuation,
        } = pending;
        self.emit_for_run(
            AppEvent::ApprovalResolved {
                approval_id,
                approved: false,
            },
            request.run_id,
        )?;
        self.emit(AppEvent::ToolFinished(ToolTrace {
            id: request.call_id,
            run_id: request.run_id,
            tool_id: approval.tool_id,
            summary: approval.target,
            result: ToolResultState::Cancelled,
            duration_ms: Some(0),
        }))?;
        match continuation {
            PendingToolContinuation::Manual => self.emit(AppEvent::RunStatusChanged {
                run_id: request.run_id,
                lifecycle: TaskLifecycle::Cancelled,
            }),
            PendingToolContinuation::Model {
                provider_call_id,
                sender,
            } => {
                let _ = sender.send(ModelToolStep::Ready(ModelToolResult {
                    call_id: provider_call_id,
                    content: "Tool request was rejected by the user.".into(),
                }));
                Ok(())
            }
        }
    }

    async fn execute_tool(
        &self,
        request: ToolRequest,
        approval: ApprovalGrant,
    ) -> Result<(), RuntimeError> {
        let _ = self.execute_tool_with_trace(request, approval).await?;
        Ok(())
    }

    async fn execute_model_tool_request(
        &self,
        request: ToolRequest,
        provider_call_id: String,
        approval: ApprovalGrant,
    ) -> Result<ModelToolStep, RuntimeError> {
        let content = match self.execute_tool_with_trace(request, approval).await? {
            Ok(result) => bounded_tool_result(result),
            Err(_) => "Tool execution failed inside the workspace boundary.".into(),
        };
        Ok(ModelToolStep::Ready(ModelToolResult {
            call_id: provider_call_id,
            content,
        }))
    }

    async fn execute_tool_with_trace(
        &self,
        request: ToolRequest,
        approval: ApprovalGrant,
    ) -> Result<Result<ToolResult, ToolError>, RuntimeError> {
        let tool_id = match &request.input {
            argentum_tools::ToolInput::ReadText { .. } => "read_text",
            argentum_tools::ToolInput::WriteText { .. } => "write_text",
        };
        let target = match &request.input {
            argentum_tools::ToolInput::ReadText { path }
            | argentum_tools::ToolInput::WriteText { path, .. } => path.clone(),
        };
        let trace = ToolTrace {
            id: request.call_id,
            run_id: request.run_id,
            tool_id: tool_id.into(),
            summary: target.clone(),
            result: ToolResultState::Running,
            duration_ms: None,
        };
        self.emit(AppEvent::ToolStarted(trace))?;
        let started = std::time::Instant::now();
        let result = self
            .tools
            .execute(
                request.clone(),
                ToolContext {
                    workspace: self.workspace.clone(),
                    approval,
                },
            )
            .await;
        let duration_ms = started.elapsed().as_millis().min(u64::MAX as u128) as u64;
        match &result {
            Ok(_) => self.emit(AppEvent::ToolFinished(ToolTrace {
                id: request.call_id,
                run_id: request.run_id,
                tool_id: tool_id.into(),
                summary: target.clone(),
                result: ToolResultState::Succeeded,
                duration_ms: Some(duration_ms),
            }))?,
            Err(_) => {
                self.emit(AppEvent::ToolFinished(ToolTrace {
                    id: request.call_id,
                    run_id: request.run_id,
                    tool_id: tool_id.into(),
                    summary: target,
                    result: ToolResultState::Failed,
                    duration_ms: Some(duration_ms),
                }))?;
                self.emit_run_error(
                    request.run_id,
                    "Tool execution failed inside the workspace boundary.".into(),
                    true,
                )?;
            }
        }
        Ok(result)
    }

    fn take_pending(&self, approval_id: argentum_domain::ApprovalId) -> Option<PendingTool> {
        self.pending_approvals
            .lock()
            .ok()
            .and_then(|mut pending| pending.remove(&approval_id))
    }

    fn take_pending_for_run(
        &self,
        run_id: RunId,
    ) -> Result<Vec<(argentum_domain::ApprovalId, PendingTool)>, RuntimeError> {
        let mut pending = self
            .pending_approvals
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?;
        let approval_ids = pending
            .iter()
            .filter_map(|(approval_id, pending)| {
                (pending.approval.run_id == run_id).then_some(*approval_id)
            })
            .collect::<Vec<_>>();
        Ok(approval_ids
            .into_iter()
            .filter_map(|approval_id| {
                pending
                    .remove(&approval_id)
                    .map(|request| (approval_id, request))
            })
            .collect())
    }

    pub fn project_id(&self) -> Uuid {
        self.project_id
    }

    pub fn session_id(&self) -> SessionId {
        self.session_id
            .lock()
            .map(|value| *value)
            .unwrap_or_default()
    }

    pub fn workspace_root(&self) -> &std::path::Path {
        self.workspace.root()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadTextArguments {
    path: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WriteTextArguments {
    path: String,
    content: String,
}

fn validate_runtime_model_tool_call(call: &ModelToolCall) -> Result<(), RuntimeError> {
    let valid_id = !call.id.is_empty()
        && call.id.len() <= MAX_MODEL_TOOL_CALL_ID_BYTES
        && !call.id.chars().any(char::is_control);
    let valid_name = !call.name.is_empty()
        && call.name.len() <= MAX_MODEL_TOOL_NAME_BYTES
        && call
            .name
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || matches!(character, '_' | '-'));
    if !valid_id || !valid_name || call.arguments.len() > MAX_MODEL_TOOL_ARGUMENT_BYTES {
        return Err(ProviderError::InvalidResponse("invalid model tool call".into()).into());
    }
    Ok(())
}

fn parse_model_tool_input(call: &ModelToolCall) -> Result<argentum_tools::ToolInput, String> {
    match call.name.as_str() {
        "read_text" => {
            let arguments = serde_json::from_str::<ReadTextArguments>(&call.arguments)
                .map_err(|_| "Tool call arguments were invalid.".to_owned())?;
            validate_model_tool_path(&arguments.path)?;
            Ok(argentum_tools::ToolInput::ReadText {
                path: arguments.path,
            })
        }
        "write_text" => {
            let arguments = serde_json::from_str::<WriteTextArguments>(&call.arguments)
                .map_err(|_| "Tool call arguments were invalid.".to_owned())?;
            validate_model_tool_path(&arguments.path)?;
            if arguments.content.len() > MAX_MODEL_WRITE_BYTES {
                return Err("Tool call content exceeded the safety limit.".into());
            }
            Ok(argentum_tools::ToolInput::WriteText {
                path: arguments.path,
                content: arguments.content,
            })
        }
        _ => Err("Tool is not available in this workspace.".into()),
    }
}

fn validate_model_tool_path(path: &str) -> Result<(), String> {
    use std::path::Component;

    if path.is_empty()
        || path.len() > MAX_MODEL_TOOL_PATH_BYTES
        || path.chars().any(char::is_control)
        || std::path::Path::new(path).components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err("Tool path must remain inside the active workspace.".into());
    }
    Ok(())
}

fn bounded_tool_result(result: ToolResult) -> String {
    truncate_utf8_bytes(result.output, MAX_MODEL_TOOL_RESULT_BYTES)
}

fn truncate_utf8_bytes(mut value: String, limit: usize) -> String {
    if value.len() <= limit {
        return value;
    }
    const MARKER: &str = "\n[tool output truncated]";
    let target = limit.saturating_sub(MARKER.len());
    let mut end = target.min(value.len());
    while end > 0 && !value.is_char_boundary(end) {
        end -= 1;
    }
    value.truncate(end);
    value.push_str(MARKER);
    value
}

fn bounded_model_history(history: Vec<ModelMessage>) -> Vec<ModelMessage> {
    let mut retained = Vec::new();
    let mut total_bytes = 0_usize;
    for message in history.into_iter().rev().take(MAX_HISTORY_MESSAGES) {
        if total_bytes.saturating_add(message.content.len()) > MAX_HISTORY_BYTES {
            break;
        }
        total_bytes += message.content.len();
        retained.push(message);
    }
    retained.reverse();
    retained
}

fn append_visible_text(target: &mut String, delta: &str) -> Result<(), RuntimeError> {
    if target.len().saturating_add(delta.len()) > MAX_VISIBLE_ASSISTANT_BYTES {
        return Err(ProviderError::InvalidResponse(
            "provider output exceeded the safety limit".into(),
        )
        .into());
    }
    target.push_str(delta);
    Ok(())
}

fn append_bounded_fragment(
    target: &mut String,
    total_bytes: &mut usize,
    fragment: &str,
    limit: usize,
    reason: &'static str,
) -> Result<(), RuntimeError> {
    if total_bytes.saturating_add(fragment.len()) > limit {
        return Err(ProviderError::InvalidResponse(reason.into()).into());
    }
    *total_bytes += fragment.len();
    target.push_str(fragment);
    Ok(())
}

fn valid_provider_id(provider_id: &str) -> bool {
    !provider_id.is_empty()
        && provider_id.len() <= 64
        && provider_id.chars().all(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
}

fn valid_provider_model(model: &str) -> bool {
    !model.is_empty() && model.chars().count() <= 256 && !model.chars().any(char::is_control)
}

fn display_provider_id(provider_id: &str) -> String {
    let provider_id = provider_id
        .chars()
        .filter(|character| {
            character.is_ascii_alphanumeric() || matches!(character, '-' | '_' | '.')
        })
        .take(64)
        .collect::<String>();
    if provider_id.is_empty() {
        "requested provider".into()
    } else {
        provider_id
    }
}

fn display_provider_label(label: &str) -> String {
    let label = label
        .trim()
        .chars()
        .filter(|character| !character.is_control())
        .take(64)
        .collect::<String>();
    if label.is_empty() {
        "Provider".into()
    } else {
        label
    }
}

fn unavailable_provider_status(provider_id: &str, display_id: &str) -> ProviderStatus {
    let (kind, label) = match provider_id {
        "lm-studio" => (ProviderKind::LocalLmStudio, "LM Studio".to_owned()),
        "anthropic" | "anthropic-compatible" => (ProviderKind::Anthropic, "Anthropic".to_owned()),
        "openai-compatible" => (
            ProviderKind::OpenAiCompatible,
            "OpenAI compatible".to_owned(),
        ),
        _ => (ProviderKind::Unknown, display_id.to_owned()),
    };
    ProviderStatus {
        profile_id: display_id.to_owned(),
        kind,
        label,
        endpoint: String::new(),
        connected: false,
        detail: String::new(),
    }
}

fn provider_probe_failure_detail(label: &str, error_value: &ProviderError) -> String {
    let label = display_provider_label(label);
    match error_value {
        ProviderError::ProviderNotConfigured { .. } => {
            format!("{label} is not configured. Check the provider ID and settings.")
        }
        ProviderError::CredentialsRequired { .. } => format!(
            "{label} requires credentials before it can be used or tested."
        ),
        ProviderError::UnsupportedProviderKind { .. } => {
            format!("{label} uses a provider kind that is not available in this build.")
        }
        ProviderError::InvalidProfile { .. } => {
            format!("{label} has an invalid saved profile. Check its provider settings.")
        }
        ProviderError::ProbeUnsupported { .. } => format!(
            "{label} does not provide a safe non-billable connectivity probe."
        ),
        ProviderError::ProbeHttpStatus { status: 401 | 403 } => {
            format!("{label} rejected the probe. Check its credentials and endpoint.")
        }
        ProviderError::ProbeHttpStatus { status: 404 } => format!(
            "{label} has no models endpoint at the configured URL. Check the endpoint."
        ),
        ProviderError::ProbeHttpStatus { status } => format!(
            "{label} returned HTTP {status} from its models endpoint. Check the service."
        ),
        ProviderError::ProbeTimeout { timeout_ms } => format!(
            "{label} did not respond within {timeout_ms} ms. Start the service or check the endpoint."
        ),
        ProviderError::ProbeResponseTooLarge { .. } => format!(
            "{label} returned an oversized models response. Check the configured endpoint."
        ),
        ProviderError::UnsafeProbeEndpoint { .. } => format!(
            "{label} has an endpoint that cannot be probed safely. Check the provider URL."
        ),
        ProviderError::UnsafeHostedEndpoint { .. } => format!(
            "{label} must use its approved HTTPS API endpoint before credentials can be sent."
        ),
        ProviderError::InvalidResponse(_) => format!(
            "{label} returned an invalid models response. Check OpenAI API compatibility."
        ),
        ProviderError::Endpoint(_) => {
            format!("{label} has an invalid endpoint URL. Check provider settings.")
        }
        ProviderError::Http(_) => format!(
            "{label} could not be reached. Start the service or check the endpoint."
        ),
        ProviderError::Api(_)
        | ProviderError::ApiHttpStatus { .. }
        | ProviderError::StreamSetup
        | ProviderError::Stream
        | ProviderError::ConsumerClosed => {
            format!("{label} probe failed. Check provider settings and try again.")
        }
    }
}

fn provider_catalog_failure_detail(label: &str, error_value: &ProviderError) -> String {
    let label = display_provider_label(label);
    match error_value {
        ProviderError::CredentialsRequired { .. } => {
            format!("{label} requires credentials before its models can be listed.")
        }
        ProviderError::ProviderNotConfigured { .. } => {
            format!("{label} is not configured. Check its provider settings.")
        }
        ProviderError::UnsupportedProviderKind { .. } | ProviderError::ProbeUnsupported { .. } => {
            format!("{label} does not support model listing in this build.")
        }
        ProviderError::InvalidProfile { .. } | ProviderError::Endpoint(_) => {
            format!("{label} has invalid provider settings.")
        }
        ProviderError::ProbeHttpStatus { status: 401 | 403 }
        | ProviderError::ApiHttpStatus { status: 401 | 403 } => {
            format!("{label} rejected the model catalog request. Check its credentials.")
        }
        ProviderError::ProbeHttpStatus { status: 404 }
        | ProviderError::ApiHttpStatus { status: 404 } => {
            format!("{label} has no models endpoint at the configured URL.")
        }
        ProviderError::ProbeHttpStatus { status } | ProviderError::ApiHttpStatus { status } => {
            format!("{label} returned HTTP {status} while listing models.")
        }
        ProviderError::ProbeTimeout { timeout_ms } => {
            format!("{label} did not return its models within {timeout_ms} ms.")
        }
        ProviderError::ProbeResponseTooLarge { .. }
        | ProviderError::InvalidResponse(_)
        | ProviderError::Api(_) => format!("{label} returned an invalid models catalog."),
        ProviderError::UnsafeProbeEndpoint { .. } | ProviderError::UnsafeHostedEndpoint { .. } => {
            format!("{label} has an endpoint that cannot be used safely.")
        }
        ProviderError::Http(_)
        | ProviderError::StreamSetup
        | ProviderError::Stream
        | ProviderError::ConsumerClosed => {
            format!("{label} could not return its model catalog. Check its settings.")
        }
    }
}

fn event_run_id(event: &AppEvent) -> Option<RunId> {
    match event {
        AppEvent::PlanUpdated { run_id, .. }
        | AppEvent::RunStatusChanged { run_id, .. }
        | AppEvent::AssistantDelta { run_id, .. }
        | AppEvent::AssistantReasoningDelta { run_id, .. }
        | AppEvent::ModelUsageUpdated { run_id, .. }
        | AppEvent::RunError { run_id, .. }
        | AppEvent::VerificationCompleted { run_id, .. } => Some(*run_id),
        AppEvent::ToolStarted(trace) | AppEvent::ToolFinished(trace) => Some(trace.run_id),
        AppEvent::ApprovalRequested(approval) => Some(approval.run_id),
        AppEvent::ChangeSetReady(change_set) => Some(change_set.run_id),
        AppEvent::WorkspaceStateLoaded(_)
        | AppEvent::ConversationSnapshotLoaded(_)
        | AppEvent::ActiveRunsSnapshot { .. }
        | AppEvent::ProjectCreated(_)
        | AppEvent::SessionCreated(_)
        | AppEvent::TaskAccepted(_)
        | AppEvent::ApprovalResolved { .. }
        | AppEvent::ProviderStatus(_)
        | AppEvent::ProviderProfilesSnapshot { .. }
        | AppEvent::ProviderModelsSnapshot { .. }
        | AppEvent::LayoutChanged(_)
        | AppEvent::Error { .. } => None,
    }
}

#[cfg(test)]
mod tests {
    use argentum_domain::{
        AppCommand, AppEvent, Capability, LayoutProfile, SurfaceId, TaskLifecycle, ToolResultState,
    };
    use argentum_providers::{
        ModelProvider, ModelRequest, ModelUsage as ProviderUsage, ProviderEvent,
        ProviderModel as ProviderCatalogModel,
    };
    use argentum_security::{ApprovalPolicy, CapabilityBroker};
    use argentum_tools::{ToolInput, ToolRegistry, ToolRequest};
    use argentum_workspaces::WorkspaceManager;
    use async_trait::async_trait;
    use tokio::sync::mpsc;

    use super::*;

    #[tokio::test]
    async fn publish_layout_restores_saved_surface_visibility() {
        let workspace_dir = tempfile::tempdir().expect("workspace");
        let broker =
            CapabilityBroker::new(workspace_dir.path(), ApprovalPolicy::default()).expect("broker");
        let workspace = WorkspaceManager::new(broker);
        let tools = ToolRegistry::with_builtins(workspace.clone());
        let runtime = RuntimeService::new(
            Store::open_in_memory().expect("store"),
            ProviderRegistry::default(),
            tools,
            workspace,
        )
        .expect("runtime");
        let mut profile = LayoutProfile::default();
        profile.visible.insert(SurfaceId::Changes, true);
        runtime
            .store
            .save_layout("default", &profile)
            .expect("saved layout");
        let mut events = runtime.subscribe();

        runtime.publish_layout().expect("published layout");

        match events.recv().await.expect("layout event") {
            AppEvent::LayoutChanged(restored) => assert_eq!(restored, profile),
            event => panic!("unexpected event: {event:?}"),
        }
    }

    #[tokio::test]
    async fn initial_workspace_snapshot_is_published_without_persisting_it() {
        let workspace_dir = tempfile::tempdir().expect("workspace");
        let broker =
            CapabilityBroker::new(workspace_dir.path(), ApprovalPolicy::default()).expect("broker");
        let workspace = WorkspaceManager::new(broker);
        let tools = ToolRegistry::with_builtins(workspace.clone());
        let runtime = RuntimeService::new(
            Store::open_in_memory().expect("store"),
            ProviderRegistry::default(),
            tools,
            workspace,
        )
        .expect("runtime");
        let mut events = runtime.subscribe();

        let snapshot = runtime
            .publish_workspace_state()
            .expect("workspace snapshot");

        assert_eq!(snapshot.project.id, runtime.project_id());
        assert_eq!(snapshot.active_session_id, Some(runtime.session_id()));
        assert!(matches!(
            events.recv().await.expect("snapshot event"),
            AppEvent::WorkspaceStateLoaded(published) if published == snapshot
        ));
        assert!(matches!(
            events.recv().await.expect("conversation event"),
            AppEvent::ConversationSnapshotLoaded(conversation)
                if conversation.project_id == runtime.project_id()
                    && conversation.session_id == runtime.session_id()
                    && conversation.messages.is_empty()
        ));
        assert!(matches!(
            events.recv().await.expect("active runs event"),
            AppEvent::ActiveRunsSnapshot { runs } if runs.is_empty()
        ));
        assert!(runtime.store.events().expect("stored events").is_empty());
    }

    #[tokio::test]
    async fn initial_conversation_snapshot_restores_messages_after_restart() {
        let directory = tempfile::tempdir().expect("directory");
        let workspace_root = directory.path().join("workspace");
        std::fs::create_dir(&workspace_root).expect("workspace");
        let database = directory.path().join("argentum.db");
        let store = Store::open(&database).expect("store");
        let broker =
            CapabilityBroker::new(&workspace_root, ApprovalPolicy::default()).expect("broker");
        let workspace = WorkspaceManager::new(broker);
        let tools = ToolRegistry::with_builtins(workspace.clone());
        let mut providers = ProviderRegistry::default();
        providers.register(MultiDeltaProvider);
        let runtime = RuntimeService::new(store, providers, tools, workspace).expect("runtime");
        configure_selected_provider(&runtime, "multi-delta").await;
        runtime
            .dispatch(AppCommand::SubmitTask {
                prompt: "restore this turn".into(),
            })
            .await
            .expect("completed task");
        let session_id = runtime.session_id();
        drop(runtime);

        let reopened_store = Store::open(&database).expect("reopened store");
        let broker = CapabilityBroker::new(&workspace_root, ApprovalPolicy::default())
            .expect("reopened broker");
        let workspace = WorkspaceManager::new(broker);
        let tools = ToolRegistry::with_builtins(workspace.clone());
        let reopened = RuntimeService::new(
            reopened_store,
            ProviderRegistry::default(),
            tools,
            workspace,
        )
        .expect("reopened runtime");
        let mut events = reopened.subscribe();

        reopened
            .publish_workspace_state()
            .expect("workspace snapshot");
        assert!(matches!(
            events.recv().await.expect("workspace event"),
            AppEvent::WorkspaceStateLoaded(_)
        ));
        assert!(matches!(
            events.recv().await.expect("conversation event"),
            AppEvent::ConversationSnapshotLoaded(snapshot)
                if snapshot.session_id == session_id
                    && snapshot.messages.len() == 2
                    && snapshot.messages[0].text == "restore this turn"
                    && snapshot.messages[1].text == "three deltas"
        ));
    }

    #[tokio::test]
    async fn new_session_is_stored_before_it_is_broadcast() {
        let workspace_dir = tempfile::tempdir().expect("workspace");
        let broker =
            CapabilityBroker::new(workspace_dir.path(), ApprovalPolicy::default()).expect("broker");
        let workspace = WorkspaceManager::new(broker);
        let tools = ToolRegistry::with_builtins(workspace.clone());
        let runtime = RuntimeService::new(
            Store::open_in_memory().expect("store"),
            ProviderRegistry::default(),
            tools,
            workspace,
        )
        .expect("runtime");
        let mut events = runtime.subscribe();

        runtime
            .dispatch(AppCommand::NewSession)
            .await
            .expect("new session");
        let created = match events.recv().await.expect("session event") {
            AppEvent::SessionCreated(session) => session,
            event => panic!("unexpected event: {event:?}"),
        };
        let snapshot = runtime
            .store
            .workspace_snapshot(runtime.project_id())
            .expect("stored snapshot");

        assert_eq!(snapshot.active_session_id, Some(created.id));
        assert!(snapshot
            .sessions
            .iter()
            .any(|session| session.id == created.id));
        assert!(runtime.store.events().expect("stored events").iter().any(
            |event| matches!(event, AppEvent::SessionCreated(session) if session.id == created.id)
        ));
    }

    #[tokio::test]
    async fn select_session_updates_runtime_and_publishes_a_snapshot() {
        let workspace_dir = tempfile::tempdir().expect("workspace");
        let store = Store::open_in_memory().expect("store");
        let broker =
            CapabilityBroker::new(workspace_dir.path(), ApprovalPolicy::default()).expect("broker");
        let workspace = WorkspaceManager::new(broker);
        let tools = ToolRegistry::with_builtins(workspace.clone());
        let runtime =
            RuntimeService::new(store.clone(), ProviderRegistry::default(), tools, workspace)
                .expect("runtime");
        let original = runtime.session_id();
        let other = store
            .create_session(runtime.project_id(), "Other session")
            .expect("other session");
        store
            .select_session(runtime.project_id(), original)
            .expect("reset active session");
        let mut events = runtime.subscribe();

        runtime
            .dispatch(AppCommand::SelectSession {
                session_id: other.id,
            })
            .await
            .expect("select session");

        assert_eq!(runtime.session_id(), other.id);
        assert!(matches!(
            events.recv().await.expect("snapshot event"),
            AppEvent::WorkspaceStateLoaded(snapshot)
                if snapshot.active_session_id == Some(other.id)
        ));
        assert!(matches!(
            events.recv().await.expect("conversation event"),
            AppEvent::ConversationSnapshotLoaded(snapshot)
                if snapshot.session_id == other.id && snapshot.messages.is_empty()
        ));
    }

    #[tokio::test]
    async fn provider_probe_success_publishes_connected_redacted_status() {
        let mut providers = ProviderRegistry::default();
        providers.register(HealthyProbeProvider);
        let (runtime, mut events) = runtime_with_providers(providers);

        runtime
            .dispatch(AppCommand::ProbeProvider {
                provider_id: "healthy".into(),
            })
            .await
            .expect("provider probe");
        let emitted = drain_events(&mut events);

        assert!(matches!(
            emitted.as_slice(),
            [AppEvent::ProviderStatus(status)]
                if status.connected
                    && status.detail == "Reachable; configured model: test"
                    && status.endpoint == "https://example.test/v1/"
        ));
    }

    #[tokio::test]
    async fn provider_probe_failure_publishes_safe_status_and_error_then_fails() {
        let mut providers = ProviderRegistry::default();
        providers.register(LeakyFailingProbeProvider);
        let (runtime, mut events) = runtime_with_providers(providers);

        let result = runtime
            .dispatch(AppCommand::ProbeProvider {
                provider_id: "leaky-failure".into(),
            })
            .await;
        let emitted = drain_events(&mut events);
        let rendered = format!("{result:?} {emitted:?}");

        assert!(matches!(
            &result,
            Err(RuntimeError::ProviderProbeFailed(detail))
                if detail.contains("probe failed")
        ));
        assert!(matches!(
            emitted.as_slice(),
            [AppEvent::ProviderStatus(status), AppEvent::Error { message, recoverable: true }]
                if !status.connected
                    && status.endpoint == "https://example.test/v1/"
                    && status.detail == *message
        ));
        assert!(!rendered.contains("secret-query"));
        assert!(!rendered.contains("secret response body"));
        assert!(!rendered.contains("password"));
    }

    #[tokio::test]
    async fn unknown_provider_still_publishes_disconnected_status_and_error() {
        let (runtime, mut events) = runtime_with_providers(ProviderRegistry::default());

        let result = runtime
            .dispatch(AppCommand::ProbeProvider {
                provider_id: "missing-provider".into(),
            })
            .await;
        let emitted = drain_events(&mut events);

        assert!(matches!(
            result,
            Err(RuntimeError::ProviderProbeFailed(detail))
                if detail.contains("is not configured")
        ));
        assert!(matches!(
            emitted.as_slice(),
            [AppEvent::ProviderStatus(status), AppEvent::Error { .. }]
                if status.kind == ProviderKind::Unknown
                    && status.label == "missing-provider"
                    && !status.connected
        ));
    }

    #[tokio::test]
    async fn provider_probe_events_do_not_grow_the_durable_event_log() {
        let mut providers = ProviderRegistry::default();
        providers.register(HealthyProbeProvider);
        let (runtime, mut events) = runtime_with_providers(providers);
        let before = runtime.store.events().expect("events before probe");

        runtime
            .dispatch(AppCommand::ProbeProvider {
                provider_id: "healthy".into(),
            })
            .await
            .expect("provider probe");

        assert!(matches!(
            events.try_recv().expect("transient provider status"),
            AppEvent::ProviderStatus(_)
        ));
        assert_eq!(runtime.store.events().expect("events after probe"), before);
    }

    #[tokio::test]
    async fn invalid_provider_ids_fail_before_status_publication() {
        let (runtime, mut events) = runtime_with_providers(ProviderRegistry::default());

        for provider_id in ["", "bad\nprovider"] {
            let result = runtime
                .dispatch(AppCommand::ProbeProvider {
                    provider_id: provider_id.into(),
                })
                .await;
            assert!(matches!(result, Err(RuntimeError::InvalidProviderId)));
        }
        let emitted = drain_events(&mut events);

        assert_eq!(emitted.len(), 2);
        assert!(emitted
            .iter()
            .all(|event| matches!(event, AppEvent::Error { .. })));
        assert!(!emitted
            .iter()
            .any(|event| matches!(event, AppEvent::ProviderStatus(_))));
    }

    #[tokio::test]
    async fn selected_provider_profile_routes_task_streaming() {
        let mut providers = ProviderRegistry::default();
        providers.register(RoutingProvider {
            id: "route-left",
            response: "left",
        });
        providers.register(RoutingProvider {
            id: "route-right",
            response: "right",
        });
        let (runtime, mut events) = runtime_with_providers(providers);
        for (id, label, selected) in [
            ("route-left", "Route left", true),
            ("route-right", "Route right", false),
        ] {
            runtime
                .dispatch(AppCommand::SaveProviderProfile {
                    profile: ProviderProfile {
                        id: id.into(),
                        label: label.into(),
                        kind: ProviderKind::OpenAiCompatible,
                        endpoint: format!("https://{id}.example.test/v1/"),
                        model: "test-model".into(),
                        selected,
                    },
                })
                .await
                .expect("saved route profile");
        }
        runtime
            .dispatch(AppCommand::SelectProviderProfile {
                provider_id: "route-right".into(),
            })
            .await
            .expect("selected route");
        let _ = drain_events(&mut events);

        runtime
            .dispatch(AppCommand::SubmitTask {
                prompt: "route this".into(),
            })
            .await
            .expect("routed task");
        let emitted = drain_events(&mut events);

        assert!(emitted.iter().any(|event| matches!(
            event,
            AppEvent::AssistantDelta { text, .. } if text == "right"
        )));
        assert!(!emitted.iter().any(|event| matches!(
            event,
            AppEvent::AssistantDelta { text, .. } if text == "left"
        )));
    }

    #[tokio::test]
    async fn provider_profile_commands_publish_transient_snapshots() {
        let (runtime, mut events) = runtime_with_providers(ProviderRegistry::default());
        let before = runtime
            .store
            .events()
            .expect("events before profile command");

        runtime
            .dispatch(AppCommand::ListProviderProfiles)
            .await
            .expect("profile list");

        assert!(matches!(
            events.try_recv().expect("profile snapshot"),
            AppEvent::ProviderProfilesSnapshot { profiles }
                if profiles.len() == 1 && profiles[0].selected
        ));
        assert_eq!(
            runtime
                .store
                .events()
                .expect("events after profile command"),
            before
        );
    }

    #[tokio::test]
    async fn provider_model_catalog_is_scoped_to_the_exact_saved_profile() {
        let mut providers = ProviderRegistry::default();
        providers.register(CatalogProvider);
        let (runtime, mut events) = runtime_with_providers(providers);
        runtime
            .dispatch(AppCommand::SaveProviderProfile {
                profile: ProviderProfile {
                    id: "catalog".into(),
                    label: "Catalog".into(),
                    kind: ProviderKind::OpenAiCompatible,
                    endpoint: "https://catalog.example.test/v1/".into(),
                    model: "model-b".into(),
                    selected: false,
                },
            })
            .await
            .expect("saved catalog profile");
        let _ = drain_events(&mut events);

        runtime
            .dispatch(AppCommand::ListProviderModels {
                provider_id: "catalog".into(),
            })
            .await
            .expect("model catalog");

        assert!(matches!(
            events.recv().await.expect("catalog event"),
            AppEvent::ProviderModelsSnapshot {
                provider_id,
                models,
                selected_model,
            } if provider_id == "catalog"
                && selected_model == "model-b"
                && models.iter().any(|model| {
                    model.id == "model-a" && model.context_window_tokens == Some(32_768)
                })
        ));

        runtime
            .dispatch(AppCommand::SelectProviderModel {
                provider_id: "catalog".into(),
                model: "model-a".into(),
            })
            .await
            .expect("select catalog model");
        assert!(matches!(
            events.recv().await.expect("profile event"),
            AppEvent::ProviderProfilesSnapshot { profiles }
                if profiles.iter().any(|profile| {
                    profile.id == "catalog" && profile.model == "model-a" && !profile.selected
                })
        ));
        assert!(matches!(
            events.recv().await.expect("updated catalog event"),
            AppEvent::ProviderModelsSnapshot {
                provider_id,
                selected_model,
                ..
            } if provider_id == "catalog" && selected_model == "model-a"
        ));

        runtime
            .dispatch(AppCommand::SelectProviderProfile {
                provider_id: "catalog".into(),
            })
            .await
            .expect("select catalog provider");
        let _ = drain_events(&mut events);
        runtime
            .dispatch(AppCommand::SubmitTask {
                prompt: "report usage".into(),
            })
            .await
            .expect("catalog model run");
        let emitted = drain_events(&mut events);
        assert!(emitted.iter().any(|event| matches!(
            event,
            AppEvent::ModelUsageUpdated {
                session_id,
                profile_id,
                model,
                usage,
                ..
            } if *session_id == runtime.session_id()
                && profile_id == "catalog"
                && model == "model-a"
                && usage.context_window_tokens == Some(32_768)
        )));
        assert!(emitted.iter().any(|event| matches!(
            event,
            AppEvent::AssistantReasoningDelta { text, .. } if text == "bounded reasoning"
        )));
        let conversation = runtime
            .conversation_snapshot(runtime.session_id())
            .expect("persisted conversation");
        let assistant = conversation.messages.last().expect("assistant message");
        assert_eq!(assistant.profile_id, "catalog");
        assert_eq!(assistant.model, "model-a");
        assert_eq!(assistant.reasoning, "bounded reasoning");
        assert_eq!(
            assistant
                .usage
                .as_ref()
                .and_then(|usage| usage.context_window_tokens),
            Some(32_768)
        );

        runtime
            .dispatch(AppCommand::SaveProviderProfile {
                profile: ProviderProfile {
                    id: "catalog".into(),
                    label: "Catalog changed".into(),
                    kind: ProviderKind::OpenAiCompatible,
                    endpoint: "https://catalog-changed.example.test/v1/".into(),
                    model: "model-a".into(),
                    selected: true,
                },
            })
            .await
            .expect("updated profile");
        assert_eq!(
            runtime
                .model_context_window("catalog", "model-a")
                .expect("context cache"),
            None
        );
        assert!(runtime
            .cached_profile_model_catalog("catalog")
            .expect("catalog cache")
            .is_none());
    }

    #[tokio::test]
    async fn provider_model_selection_does_not_implicitly_switch_profiles_and_routes_exact_model() {
        let left_models = Arc::new(Mutex::new(Vec::new()));
        let right_models = Arc::new(Mutex::new(Vec::new()));
        let mut providers = ProviderRegistry::default();
        providers.register(ModelCapturingProvider {
            id: "model-left",
            seen_models: left_models.clone(),
        });
        providers.register(ModelCapturingProvider {
            id: "model-right",
            seen_models: right_models.clone(),
        });
        let (runtime, mut events) = runtime_with_providers(providers);
        for (id, selected) in [("model-left", true), ("model-right", false)] {
            runtime
                .dispatch(AppCommand::SaveProviderProfile {
                    profile: ProviderProfile {
                        id: id.into(),
                        label: id.into(),
                        kind: ProviderKind::OpenAiCompatible,
                        endpoint: format!("https://{id}.example.test/v1/"),
                        model: "old-model".into(),
                        selected,
                    },
                })
                .await
                .expect("saved profile");
        }
        let _ = drain_events(&mut events);

        runtime
            .dispatch(AppCommand::SelectProviderModel {
                provider_id: "model-right".into(),
                model: "right-model".into(),
            })
            .await
            .expect("selected model");
        let profiles = match events.recv().await.expect("profile event") {
            AppEvent::ProviderProfilesSnapshot { profiles } => profiles,
            event => panic!("unexpected event: {event:?}"),
        };
        assert!(profiles
            .iter()
            .any(|profile| profile.id == "model-left" && profile.selected));
        assert!(profiles.iter().any(|profile| {
            profile.id == "model-right" && !profile.selected && profile.model == "right-model"
        }));

        runtime
            .dispatch(AppCommand::SelectProviderProfile {
                provider_id: "model-right".into(),
            })
            .await
            .expect("selected provider");
        let _ = drain_events(&mut events);
        runtime
            .dispatch(AppCommand::SubmitTask {
                prompt: "use exact model".into(),
            })
            .await
            .expect("model request");

        assert!(left_models.lock().expect("left models").is_empty());
        assert_eq!(
            right_models.lock().expect("right models").as_slice(),
            ["right-model"]
        );
    }

    #[tokio::test]
    async fn model_catalog_and_selection_reject_invalid_or_foreign_profiles_safely() {
        let (runtime, mut events) = runtime_with_providers(ProviderRegistry::default());

        assert!(matches!(
            runtime
                .dispatch(AppCommand::ListProviderModels {
                    provider_id: "missing".into(),
                })
                .await,
            Err(RuntimeError::ProviderCatalogFailed(_))
        ));
        assert!(matches!(
            runtime
                .dispatch(AppCommand::SelectProviderModel {
                    provider_id: "lm-studio".into(),
                    model: "bad\nmodel".into(),
                })
                .await,
            Err(RuntimeError::InvalidProviderModel)
        ));
        let rendered = format!("{:?}", drain_events(&mut events));
        assert!(!rendered.contains("bad\nmodel"));
    }

    #[tokio::test]
    async fn duplicate_usage_in_one_provider_round_is_rejected() {
        let mut providers = ProviderRegistry::default();
        providers.register(DuplicateUsageProvider);
        let (runtime, mut events) = runtime_with_providers(providers);
        configure_selected_provider(&runtime, "duplicate-usage").await;
        let _ = drain_events(&mut events);

        let result = runtime
            .dispatch(AppCommand::SubmitTask {
                prompt: "duplicate usage".into(),
            })
            .await;

        assert!(matches!(
            result,
            Err(RuntimeError::Provider(ProviderError::InvalidResponse(message)))
                if message.contains("duplicate usage")
        ));
        assert_eq!(
            drain_events(&mut events)
                .iter()
                .filter(|event| matches!(event, AppEvent::ModelUsageUpdated { .. }))
                .count(),
            1
        );
    }

    #[tokio::test]
    async fn final_round_without_usage_does_not_persist_an_earlier_rounds_usage() {
        let workspace_dir = tempfile::tempdir().expect("workspace");
        std::fs::write(workspace_dir.path().join("notes.txt"), "workspace evidence")
            .expect("fixture");
        let mut providers = ProviderRegistry::default();
        providers.register(FirstRoundOnlyUsageProvider);
        let broker =
            CapabilityBroker::new(workspace_dir.path(), ApprovalPolicy::default()).expect("broker");
        let workspace = WorkspaceManager::new(broker);
        let tools = ToolRegistry::with_builtins(workspace.clone());
        let runtime = RuntimeService::new(
            Store::open_in_memory().expect("store"),
            providers,
            tools,
            workspace,
        )
        .expect("runtime");
        configure_selected_provider(&runtime, "first-round-usage").await;
        let mut events = runtime.subscribe();

        runtime
            .dispatch(AppCommand::SubmitTask {
                prompt: "read then answer".into(),
            })
            .await
            .expect("model tool run");

        let emitted = drain_events(&mut events);
        assert_eq!(
            emitted
                .iter()
                .filter(|event| matches!(event, AppEvent::ModelUsageUpdated { .. }))
                .count(),
            1
        );
        let snapshot = runtime
            .conversation_snapshot(runtime.session_id())
            .expect("conversation");
        let assistant = snapshot.messages.last().expect("assistant message");
        assert_eq!(assistant.text, "final answer");
        assert_eq!(assistant.profile_id, "first-round-usage");
        assert!(assistant.usage.is_none());
    }

    #[tokio::test]
    async fn approval_grants_a_single_write_tool_call() {
        let workspace_dir = tempfile::tempdir().expect("workspace");
        let broker =
            CapabilityBroker::new(workspace_dir.path(), ApprovalPolicy::default()).expect("broker");
        let workspace = WorkspaceManager::new(broker);
        let tools = ToolRegistry::with_builtins(workspace.clone());
        let runtime = RuntimeService::new(
            Store::open_in_memory().expect("store"),
            ProviderRegistry::default(),
            tools,
            workspace,
        )
        .expect("runtime");
        let mut events = runtime.subscribe();
        let run_id = Uuid::new_v4();
        let call_id = Uuid::new_v4();

        runtime
            .request_tool(ToolRequest {
                call_id,
                run_id,
                input: ToolInput::WriteText {
                    path: "approved.txt".into(),
                    content: "approved".into(),
                },
            })
            .await
            .expect("approval request");

        let approval_id = match events.recv().await.expect("approval event") {
            AppEvent::ApprovalRequested(request) => request.id,
            event => panic!("unexpected event: {event:?}"),
        };
        runtime
            .dispatch(AppCommand::ApproveTool {
                approval_id,
                scope: ApprovalScope::Once,
            })
            .await
            .expect("approved tool");

        let mut succeeded = false;
        while let Ok(event) = events.try_recv() {
            if let AppEvent::ToolFinished(trace) = event {
                succeeded = trace.result == ToolResultState::Succeeded;
                break;
            }
        }
        assert!(succeeded, "approved tool did not finish successfully");
        assert_eq!(
            std::fs::read_to_string(runtime.workspace_root().join("approved.txt"))
                .expect("written file"),
            "approved"
        );
        assert_eq!(runtime.tool_count(), 2);
        assert!(!TaskLifecycle::Running.is_terminal());
        assert_eq!(Capability::WriteFiles.label(), "Write files");
    }

    #[tokio::test]
    async fn selected_provider_without_credentials_is_both_an_event_and_a_command_failure() {
        let (runtime, mut events) = runtime_with_providers(ProviderRegistry::default());
        runtime
            .dispatch(AppCommand::SaveProviderProfile {
                profile: ProviderProfile {
                    id: "anthropic-test".into(),
                    label: "Anthropic test".into(),
                    kind: ProviderKind::Anthropic,
                    endpoint: "https://api.anthropic.com/v1/messages".into(),
                    model: "claude-test".into(),
                    selected: true,
                },
            })
            .await
            .expect("saved selected profile");
        let _ = drain_events(&mut events);

        let result = runtime
            .dispatch(AppCommand::SubmitTask {
                prompt: "inspect".into(),
            })
            .await;
        let emitted = drain_events(&mut events);

        assert!(matches!(
            result,
            Err(RuntimeError::Provider(
                ProviderError::CredentialsRequired { .. }
            ))
        ));
        assert!(emitted.iter().any(|event| matches!(
            event,
            AppEvent::RunStatusChanged {
                lifecycle: TaskLifecycle::Failed,
                ..
            }
        )));
        assert!(emitted.iter().any(|event| matches!(
            event,
            AppEvent::RunError { session_id, message, .. }
                if *session_id == runtime.session_id()
                    && message.contains("credentials are required")
        )));
        assert!(!emitted.iter().any(|event| matches!(
            event,
            AppEvent::RunStatusChanged {
                lifecycle: TaskLifecycle::Complete,
                ..
            }
        )));
    }

    #[tokio::test]
    async fn provider_stream_failure_is_both_an_event_and_a_command_failure() {
        let mut providers = ProviderRegistry::default();
        providers.register(FailingProvider);
        let (runtime, mut events) = runtime_with_providers(providers);
        runtime
            .dispatch(AppCommand::SaveProviderProfile {
                profile: ProviderProfile {
                    id: "failing".into(),
                    label: "Failing".into(),
                    kind: ProviderKind::OpenAiCompatible,
                    endpoint: "https://example.test/v1/".into(),
                    model: "test-model".into(),
                    selected: true,
                },
            })
            .await
            .expect("saved selected profile");
        let _ = drain_events(&mut events);

        let result = runtime
            .dispatch(AppCommand::SubmitTask {
                prompt: "inspect".into(),
            })
            .await;
        let emitted = drain_events(&mut events);

        assert!(matches!(
            result,
            Err(RuntimeError::Provider(ProviderError::Api(message)))
                if message == "test provider failure"
        ));
        assert!(emitted.iter().any(|event| matches!(
            event,
            AppEvent::RunStatusChanged {
                lifecycle: TaskLifecycle::Failed,
                ..
            }
        )));
        assert!(emitted.iter().any(|event| matches!(
            event,
            AppEvent::RunError { session_id, message, .. }
                if *session_id == runtime.session_id()
                    && message.contains("test provider failure")
        )));
    }

    #[tokio::test]
    async fn completed_run_persists_one_aggregate_without_durable_deltas() {
        let mut providers = ProviderRegistry::default();
        providers.register(MultiDeltaProvider);
        let (runtime, mut events) = runtime_with_providers(providers);
        configure_selected_provider(&runtime, "multi-delta").await;
        let _ = drain_events(&mut events);

        runtime
            .dispatch(AppCommand::SubmitTask {
                prompt: "aggregate this".into(),
            })
            .await
            .expect("completed task");

        let durable_events = runtime.store.events().expect("durable events");
        assert!(!durable_events
            .iter()
            .any(|event| matches!(event, AppEvent::AssistantDelta { .. })));
        let snapshot = runtime
            .conversation_snapshot(runtime.session_id())
            .expect("conversation");
        assert_eq!(snapshot.messages.len(), 2);
        assert_eq!(snapshot.messages[0].role, ConversationRole::User);
        assert_eq!(snapshot.messages[1].role, ConversationRole::Assistant);
        assert_eq!(snapshot.messages[1].text, "three deltas");
        assert_eq!(
            snapshot.messages[1].status,
            ConversationMessageStatus::Complete
        );
    }

    #[tokio::test]
    async fn failed_partial_output_is_restored_with_failed_status() {
        let mut providers = ProviderRegistry::default();
        providers.register(PartialFailingProvider);
        let (runtime, mut events) = runtime_with_providers(providers);
        configure_selected_provider(&runtime, "partial-failure").await;
        let _ = drain_events(&mut events);

        let result = runtime
            .dispatch(AppCommand::SubmitTask {
                prompt: "fail after output".into(),
            })
            .await;

        assert!(matches!(
            result,
            Err(RuntimeError::Provider(ProviderError::Api(message)))
                if message == "failure after partial output"
        ));
        let snapshot = runtime
            .conversation_snapshot(runtime.session_id())
            .expect("conversation");
        assert_eq!(snapshot.messages.len(), 2);
        assert_eq!(snapshot.messages[1].text, "partial response");
        assert_eq!(
            snapshot.messages[1].status,
            ConversationMessageStatus::Failed
        );
        let emitted = drain_events(&mut events);
        let snapshot_position = emitted
            .iter()
            .position(|event| matches!(event, AppEvent::ConversationSnapshotLoaded(_)))
            .expect("conversation snapshot event");
        let failed_position = emitted
            .iter()
            .position(|event| {
                matches!(
                    event,
                    AppEvent::RunStatusChanged {
                        lifecycle: TaskLifecycle::Failed,
                        ..
                    }
                )
            })
            .expect("failed event");
        assert!(snapshot_position < failed_position);
    }

    #[tokio::test]
    async fn cancellation_persists_partial_output_and_releases_session_guard() {
        let mut providers = ProviderRegistry::default();
        providers.register(BlockingProvider);
        let (runtime, mut events) = runtime_with_providers(providers);
        configure_selected_provider(&runtime, "blocking").await;
        let _ = drain_events(&mut events);
        let running_runtime = runtime.clone();
        let first = tokio::spawn(async move {
            running_runtime
                .dispatch(AppCommand::SubmitTask {
                    prompt: "wait for cancellation".into(),
                })
                .await
        });
        let run_id = loop {
            if let AppEvent::AssistantDelta { run_id, .. } =
                tokio::time::timeout(std::time::Duration::from_secs(2), events.recv())
                    .await
                    .expect("run event timeout")
                    .expect("run event")
            {
                break run_id;
            }
        };

        runtime
            .dispatch(AppCommand::CancelRun { run_id })
            .await
            .expect("cancel request");
        first.await.expect("task join").expect("cancelled task");

        let snapshot = runtime
            .conversation_snapshot(runtime.session_id())
            .expect("conversation");
        assert_eq!(snapshot.messages.len(), 2);
        assert_eq!(snapshot.messages[1].text, "partial before wait");
        assert_eq!(
            snapshot.messages[1].status,
            ConversationMessageStatus::Interrupted
        );
        assert!(runtime
            .active_session_runs
            .lock()
            .expect("active runs")
            .is_empty());
        assert!(runtime
            .cancellation_tokens
            .lock()
            .expect("cancellation tokens")
            .is_empty());
        assert!(runtime
            .run_sessions
            .lock()
            .expect("run sessions")
            .is_empty());
    }

    #[tokio::test]
    async fn second_submit_to_active_session_is_rejected() {
        let mut providers = ProviderRegistry::default();
        providers.register(BlockingProvider);
        let (runtime, mut events) = runtime_with_providers(providers);
        configure_selected_provider(&runtime, "blocking").await;
        let _ = drain_events(&mut events);
        let running_runtime = runtime.clone();
        let first = tokio::spawn(async move {
            running_runtime
                .dispatch(AppCommand::SubmitTask {
                    prompt: "first".into(),
                })
                .await
        });
        let run_id = loop {
            if let AppEvent::AssistantDelta { run_id, .. } =
                tokio::time::timeout(std::time::Duration::from_secs(2), events.recv())
                    .await
                    .expect("run event timeout")
                    .expect("run event")
            {
                break run_id;
            }
        };

        let second = runtime
            .dispatch(AppCommand::SubmitTask {
                prompt: "second".into(),
            })
            .await;
        assert!(matches!(second, Err(RuntimeError::SessionRunActive)));
        runtime
            .dispatch(AppCommand::CancelRun { run_id })
            .await
            .expect("cancel request");
        first.await.expect("task join").expect("cancelled task");
        let snapshot = runtime
            .conversation_snapshot(runtime.session_id())
            .expect("conversation");
        assert_eq!(snapshot.messages.len(), 2);
        assert!(!snapshot
            .messages
            .iter()
            .any(|message| message.text == "second"));
    }

    #[tokio::test]
    async fn same_session_second_turn_receives_history_and_publishes_full_snapshot() {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let mut providers = ProviderRegistry::default();
        providers.register(RecordingProvider {
            requests: Arc::clone(&requests),
        });
        let (runtime, mut events) = runtime_with_providers(providers);
        configure_selected_provider(&runtime, "recording").await;
        let _ = drain_events(&mut events);

        runtime
            .dispatch(AppCommand::SubmitTask {
                prompt: "first question".into(),
            })
            .await
            .expect("first turn");
        let _ = drain_events(&mut events);
        runtime
            .dispatch(AppCommand::SubmitTask {
                prompt: "second question".into(),
            })
            .await
            .expect("second turn");
        let emitted = drain_events(&mut events);
        let latest_snapshot = emitted
            .iter()
            .rev()
            .filter_map(|event| match event {
                AppEvent::ConversationSnapshotLoaded(snapshot) => Some(snapshot),
                _ => None,
            })
            .next()
            .expect("conversation snapshot");
        assert_eq!(latest_snapshot.messages.len(), 4);
        assert_eq!(latest_snapshot.messages[0].text, "first question");
        assert_eq!(latest_snapshot.messages[1].text, "recorded answer");
        assert_eq!(latest_snapshot.messages[2].text, "second question");
        assert_eq!(latest_snapshot.messages[3].text, "recorded answer");

        let requests = requests.lock().expect("requests");
        assert_eq!(requests.len(), 2);
        assert!(requests[0].history.is_empty());
        assert_eq!(requests[1].history.len(), 2);
        assert_eq!(requests[1].history[0].role, ModelMessageRole::User);
        assert_eq!(requests[1].history[0].content, "first question");
        assert_eq!(requests[1].history[1].role, ModelMessageRole::Assistant);
        assert_eq!(requests[1].history[1].content, "recorded answer");
        assert_eq!(requests[1].prompt, "second question");
    }

    #[tokio::test]
    async fn model_read_tool_result_returns_to_provider_and_finishes_once() {
        let workspace_dir = tempfile::tempdir().expect("workspace");
        std::fs::write(workspace_dir.path().join("notes.txt"), "workspace evidence")
            .expect("fixture");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let mut providers = ProviderRegistry::default();
        providers.register(ModelToolProvider {
            requests: Arc::clone(&requests),
            tool_name: "read_text",
            arguments: r#"{"path":"notes.txt"}"#,
            final_text: "Read complete",
        });
        let broker =
            CapabilityBroker::new(workspace_dir.path(), ApprovalPolicy::default()).expect("broker");
        let workspace = WorkspaceManager::new(broker);
        let tools = ToolRegistry::with_builtins(workspace.clone());
        let runtime = RuntimeService::new(
            Store::open_in_memory().expect("store"),
            providers,
            tools,
            workspace,
        )
        .expect("runtime");
        configure_selected_provider(&runtime, "model-tool").await;
        let mut events = runtime.subscribe();

        runtime
            .dispatch(AppCommand::SubmitTask {
                prompt: "Read the notes".into(),
            })
            .await
            .expect("model tool run");

        let requests = requests.lock().expect("requests");
        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests[0]
                .tools
                .iter()
                .map(|tool| tool.name.as_str())
                .collect::<Vec<_>>(),
            vec!["read_text", "write_text"]
        );
        assert_eq!(requests[1].tool_exchanges.len(), 1);
        assert_eq!(
            requests[1].tool_exchanges[0].results[0].content,
            "workspace evidence"
        );
        drop(requests);
        let emitted = drain_events(&mut events);
        assert_eq!(
            emitted
                .iter()
                .filter(|event| matches!(event, AppEvent::ToolStarted(_)))
                .count(),
            1
        );
        let snapshot = runtime
            .conversation_snapshot(runtime.session_id())
            .expect("conversation");
        assert_eq!(
            snapshot.messages.last().expect("answer").text,
            "Read complete"
        );
    }

    #[tokio::test]
    async fn model_tool_paths_cannot_escape_the_workspace() {
        let workspace_dir = tempfile::tempdir().expect("workspace");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let mut providers = ProviderRegistry::default();
        providers.register(ModelToolProvider {
            requests: Arc::clone(&requests),
            tool_name: "read_text",
            arguments: r#"{"path":"../outside.txt"}"#,
            final_text: "Handled safely",
        });
        let broker =
            CapabilityBroker::new(workspace_dir.path(), ApprovalPolicy::default()).expect("broker");
        let workspace = WorkspaceManager::new(broker);
        let tools = ToolRegistry::with_builtins(workspace.clone());
        let runtime = RuntimeService::new(
            Store::open_in_memory().expect("store"),
            providers,
            tools,
            workspace,
        )
        .expect("runtime");
        configure_selected_provider(&runtime, "model-tool").await;
        let mut events = runtime.subscribe();

        runtime
            .dispatch(AppCommand::SubmitTask {
                prompt: "Try an unsafe path".into(),
            })
            .await
            .expect("bounded model response");

        let requests = requests.lock().expect("requests");
        assert_eq!(requests.len(), 2);
        assert_eq!(
            requests[1].tool_exchanges[0].results[0].content,
            "Tool path must remain inside the active workspace."
        );
        drop(requests);
        assert!(!drain_events(&mut events)
            .iter()
            .any(|event| matches!(event, AppEvent::ToolStarted(_))));
    }

    #[tokio::test]
    async fn model_write_waits_for_approval_resumes_once_and_returns_result() {
        let workspace_dir = tempfile::tempdir().expect("workspace");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let mut providers = ProviderRegistry::default();
        providers.register(ModelToolProvider {
            requests: Arc::clone(&requests),
            tool_name: "write_text",
            arguments: r#"{"path":"approved.txt","content":"approved once"}"#,
            final_text: "Write complete",
        });
        let broker =
            CapabilityBroker::new(workspace_dir.path(), ApprovalPolicy::default()).expect("broker");
        let workspace = WorkspaceManager::new(broker);
        let tools = ToolRegistry::with_builtins(workspace.clone());
        let runtime = RuntimeService::new(
            Store::open_in_memory().expect("store"),
            providers,
            tools,
            workspace,
        )
        .expect("runtime");
        configure_selected_provider(&runtime, "model-tool").await;
        let mut events = runtime.subscribe();
        let running_runtime = runtime.clone();
        let run = tokio::spawn(async move {
            running_runtime
                .dispatch(AppCommand::SubmitTask {
                    prompt: "Write the file".into(),
                })
                .await
        });
        let approval_id = loop {
            if let AppEvent::ApprovalRequested(approval) =
                tokio::time::timeout(std::time::Duration::from_secs(2), events.recv())
                    .await
                    .expect("approval timeout")
                    .expect("approval event")
            {
                break approval.id;
            }
        };

        runtime
            .publish_workspace_state()
            .expect("resync during approval");
        let resync_events = drain_events(&mut events);
        assert!(resync_events.iter().any(|event| matches!(
            event,
            AppEvent::ActiveRunsSnapshot { runs }
                if matches!(runs.as_slice(), [ActiveRunState {
                    session_id,
                    lifecycle: TaskLifecycle::WaitingForApproval,
                    ..
                }] if *session_id == runtime.session_id())
        )));

        runtime
            .dispatch(AppCommand::ApproveTool {
                approval_id,
                scope: ApprovalScope::Once,
            })
            .await
            .expect("approve model write");
        run.await.expect("run join").expect("completed write run");
        runtime
            .dispatch(AppCommand::ApproveTool {
                approval_id,
                scope: ApprovalScope::Once,
            })
            .await
            .expect("duplicate approval is harmless");

        assert_eq!(
            std::fs::read_to_string(workspace_dir.path().join("approved.txt"))
                .expect("written file"),
            "approved once"
        );
        let requests = requests.lock().expect("requests");
        assert_eq!(requests.len(), 2);
        assert!(requests[1].tool_exchanges[0].results[0]
            .content
            .contains("bytes written"));
        drop(requests);
        let emitted = drain_events(&mut events);
        assert_eq!(
            emitted
                .iter()
                .filter(|event| matches!(event, AppEvent::ToolStarted(_)))
                .count(),
            1
        );
        assert!(runtime
            .pending_approvals
            .lock()
            .expect("approvals")
            .is_empty());
        assert!(runtime
            .run_lifecycles
            .lock()
            .expect("lifecycles")
            .is_empty());
    }

    #[tokio::test]
    async fn cancelling_a_model_write_approval_cleans_up_without_writing() {
        let workspace_dir = tempfile::tempdir().expect("workspace");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let mut providers = ProviderRegistry::default();
        providers.register(ModelToolProvider {
            requests,
            tool_name: "write_text",
            arguments: r#"{"path":"cancelled.txt","content":"must not exist"}"#,
            final_text: "unexpected",
        });
        let broker =
            CapabilityBroker::new(workspace_dir.path(), ApprovalPolicy::default()).expect("broker");
        let workspace = WorkspaceManager::new(broker);
        let tools = ToolRegistry::with_builtins(workspace.clone());
        let runtime = RuntimeService::new(
            Store::open_in_memory().expect("store"),
            providers,
            tools,
            workspace,
        )
        .expect("runtime");
        configure_selected_provider(&runtime, "model-tool").await;
        let mut events = runtime.subscribe();
        let running_runtime = runtime.clone();
        let run = tokio::spawn(async move {
            running_runtime
                .dispatch(AppCommand::SubmitTask {
                    prompt: "Write then cancel".into(),
                })
                .await
        });
        let run_id = loop {
            if let AppEvent::ApprovalRequested(approval) =
                tokio::time::timeout(std::time::Duration::from_secs(2), events.recv())
                    .await
                    .expect("approval timeout")
                    .expect("approval event")
            {
                break approval.run_id;
            }
        };

        runtime
            .dispatch(AppCommand::CancelRun { run_id })
            .await
            .expect("cancel run");
        run.await.expect("run join").expect("cancelled run");

        assert!(!workspace_dir.path().join("cancelled.txt").exists());
        assert!(runtime
            .pending_approvals
            .lock()
            .expect("approvals")
            .is_empty());
        assert!(runtime
            .active_session_runs
            .lock()
            .expect("active runs")
            .is_empty());
        assert!(runtime
            .run_lifecycles
            .lock()
            .expect("lifecycles")
            .is_empty());
    }

    #[test]
    fn history_and_visible_output_limits_keep_recent_complete_data() {
        let history = (0..(MAX_HISTORY_MESSAGES + 4))
            .map(|index| ModelMessage {
                role: ModelMessageRole::User,
                content: format!("message-{index}"),
            })
            .collect::<Vec<_>>();
        let bounded = bounded_model_history(history);
        assert_eq!(bounded.len(), MAX_HISTORY_MESSAGES);
        assert_eq!(bounded.last().expect("latest").content, "message-131");

        let oversized_recent = bounded_model_history(vec![ModelMessage {
            role: ModelMessageRole::Assistant,
            content: "x".repeat(MAX_HISTORY_BYTES + 1),
        }]);
        assert!(oversized_recent.is_empty());

        let mut output = "a".repeat(MAX_VISIBLE_ASSISTANT_BYTES);
        assert!(append_visible_text(&mut output, "b").is_err());
        assert_eq!(output.len(), MAX_VISIBLE_ASSISTANT_BYTES);
    }

    #[tokio::test]
    async fn oversized_prompt_fails_before_persistence_or_provider_use() {
        let (runtime, mut events) = runtime_with_providers(ProviderRegistry::default());
        let result = runtime
            .dispatch(AppCommand::SubmitTask {
                prompt: "p".repeat(MAX_PROMPT_BYTES + 1),
            })
            .await;

        assert!(matches!(
            result,
            Err(RuntimeError::Provider(ProviderError::InvalidResponse(message)))
                if message == "task input exceeded the safety limit"
        ));
        assert!(runtime
            .conversation_snapshot(runtime.session_id())
            .expect("conversation")
            .messages
            .is_empty());
        assert!(drain_events(&mut events).iter().any(|event| matches!(
            event,
            AppEvent::Error { message, .. }
                if message == "Task input exceeded the safety limit."
        )));
    }

    #[tokio::test]
    async fn oversized_visible_output_fails_instead_of_completing() {
        let mut providers = ProviderRegistry::default();
        providers.register(OversizedOutputProvider);
        let (runtime, mut events) = runtime_with_providers(providers);
        configure_selected_provider(&runtime, "oversized-output").await;
        let _ = drain_events(&mut events);

        let result = runtime
            .dispatch(AppCommand::SubmitTask {
                prompt: "Bound the output".into(),
            })
            .await;

        assert!(matches!(
            result,
            Err(RuntimeError::Provider(ProviderError::InvalidResponse(message)))
                if message == "provider output exceeded the safety limit"
        ));
        let emitted = drain_events(&mut events);
        assert!(emitted.iter().any(|event| matches!(
            event,
            AppEvent::RunStatusChanged {
                lifecycle: TaskLifecycle::Failed,
                ..
            }
        )));
        assert!(!emitted.iter().any(|event| matches!(
            event,
            AppEvent::RunStatusChanged {
                lifecycle: TaskLifecycle::Complete,
                ..
            }
        )));
    }

    #[tokio::test]
    async fn model_tool_loop_stops_at_the_round_limit_and_fails_the_run() {
        let workspace_dir = tempfile::tempdir().expect("workspace");
        std::fs::write(workspace_dir.path().join("loop.txt"), "again").expect("fixture");
        let requests = Arc::new(Mutex::new(Vec::new()));
        let mut providers = ProviderRegistry::default();
        providers.register(LoopingToolProvider {
            requests: Arc::clone(&requests),
        });
        let broker =
            CapabilityBroker::new(workspace_dir.path(), ApprovalPolicy::default()).expect("broker");
        let workspace = WorkspaceManager::new(broker);
        let tools = ToolRegistry::with_builtins(workspace.clone());
        let runtime = RuntimeService::new(
            Store::open_in_memory().expect("store"),
            providers,
            tools,
            workspace,
        )
        .expect("runtime");
        configure_selected_provider(&runtime, "looping-tool").await;
        let mut events = runtime.subscribe();

        let result = runtime
            .dispatch(AppCommand::SubmitTask {
                prompt: "Loop".into(),
            })
            .await;

        assert!(matches!(result, Err(RuntimeError::ToolLoopLimit)));
        assert_eq!(requests.lock().expect("requests").len(), MAX_MODEL_ROUNDS);
        let emitted = drain_events(&mut events);
        assert!(emitted.iter().any(|event| matches!(
            event,
            AppEvent::RunError { session_id, .. }
                if *session_id == runtime.session_id()
        )));
        assert!(!emitted.iter().any(|event| matches!(
            event,
            AppEvent::RunStatusChanged {
                lifecycle: TaskLifecycle::Complete,
                ..
            }
        )));
        assert!(runtime
            .run_lifecycles
            .lock()
            .expect("lifecycles")
            .is_empty());
    }

    fn runtime_with_providers(
        providers: ProviderRegistry,
    ) -> (RuntimeService, broadcast::Receiver<AppEvent>) {
        let workspace_dir = tempfile::tempdir().expect("workspace");
        let broker =
            CapabilityBroker::new(workspace_dir.path(), ApprovalPolicy::default()).expect("broker");
        let workspace = WorkspaceManager::new(broker);
        let tools = ToolRegistry::with_builtins(workspace.clone());
        let runtime = RuntimeService::new(
            Store::open_in_memory().expect("store"),
            providers,
            tools,
            workspace,
        )
        .expect("runtime");
        let events = runtime.subscribe();
        (runtime, events)
    }

    fn drain_events(events: &mut broadcast::Receiver<AppEvent>) -> Vec<AppEvent> {
        let mut emitted = Vec::new();
        while let Ok(event) = events.try_recv() {
            emitted.push(event);
        }
        emitted
    }

    async fn configure_selected_provider(runtime: &RuntimeService, id: &str) {
        runtime
            .dispatch(AppCommand::SaveProviderProfile {
                profile: ProviderProfile {
                    id: id.into(),
                    label: id.into(),
                    kind: ProviderKind::OpenAiCompatible,
                    endpoint: format!("https://{id}.example.test/v1/"),
                    model: "test-model".into(),
                    selected: true,
                },
            })
            .await
            .expect("selected provider profile");
    }

    #[derive(Debug)]
    struct FailingProvider;

    #[async_trait]
    impl ModelProvider for FailingProvider {
        fn id(&self) -> &'static str {
            "failing"
        }

        fn status(&self) -> argentum_domain::ProviderStatus {
            argentum_domain::ProviderStatus {
                profile_id: self.id().into(),
                kind: argentum_domain::ProviderKind::OpenAiCompatible,
                label: "Failing".into(),
                endpoint: "local".into(),
                connected: false,
                detail: "test".into(),
            }
        }

        async fn stream(
            &self,
            _request: ModelRequest,
            _sender: mpsc::Sender<ProviderEvent>,
        ) -> Result<(), ProviderError> {
            Err(ProviderError::Api("test provider failure".into()))
        }
    }

    #[derive(Debug)]
    struct HealthyProbeProvider;

    #[async_trait]
    impl ModelProvider for HealthyProbeProvider {
        fn id(&self) -> &'static str {
            "healthy"
        }

        fn status(&self) -> argentum_domain::ProviderStatus {
            argentum_domain::ProviderStatus {
                profile_id: self.id().into(),
                kind: ProviderKind::OpenAiCompatible,
                label: "Healthy".into(),
                endpoint: "https://example.test/v1/".into(),
                connected: false,
                detail: "Not tested".into(),
            }
        }

        async fn probe(&self) -> Result<argentum_domain::ProviderStatus, ProviderError> {
            Ok(argentum_domain::ProviderStatus {
                profile_id: self.id().into(),
                kind: ProviderKind::OpenAiCompatible,
                label: "Healthy".into(),
                endpoint: "https://user:password@example.test/v1/?api_key=secret-query#fragment"
                    .into(),
                connected: true,
                detail: "Reachable; configured model: test".into(),
            })
        }

        async fn stream(
            &self,
            _request: ModelRequest,
            _sender: mpsc::Sender<ProviderEvent>,
        ) -> Result<(), ProviderError> {
            Ok(())
        }
    }

    #[derive(Debug)]
    struct DuplicateUsageProvider;

    #[async_trait]
    impl ModelProvider for DuplicateUsageProvider {
        fn id(&self) -> &'static str {
            "duplicate-usage"
        }

        fn status(&self) -> ProviderStatus {
            test_provider_status(self.id())
        }

        async fn stream(
            &self,
            _request: ModelRequest,
            sender: mpsc::Sender<ProviderEvent>,
        ) -> Result<(), ProviderError> {
            let usage = ProviderUsage {
                input_tokens: 1,
                output_tokens: 1,
                total_tokens: 2,
                cached_input_tokens: None,
                reasoning_tokens: None,
                context_window_tokens: Some(1_024),
            };
            sender
                .send(ProviderEvent::Usage(usage.clone()))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
            sender
                .send(ProviderEvent::Usage(usage))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
            sender
                .send(ProviderEvent::Completed)
                .await
                .map_err(|_| ProviderError::ConsumerClosed)
        }
    }

    #[derive(Debug)]
    struct FirstRoundOnlyUsageProvider;

    #[async_trait]
    impl ModelProvider for FirstRoundOnlyUsageProvider {
        fn id(&self) -> &'static str {
            "first-round-usage"
        }

        fn status(&self) -> ProviderStatus {
            test_provider_status(self.id())
        }

        async fn stream(
            &self,
            request: ModelRequest,
            sender: mpsc::Sender<ProviderEvent>,
        ) -> Result<(), ProviderError> {
            if request.tool_exchanges.is_empty() {
                sender
                    .send(ProviderEvent::Usage(ProviderUsage {
                        input_tokens: 10,
                        output_tokens: 2,
                        total_tokens: 12,
                        cached_input_tokens: None,
                        reasoning_tokens: None,
                        context_window_tokens: Some(1_024),
                    }))
                    .await
                    .map_err(|_| ProviderError::ConsumerClosed)?;
                sender
                    .send(ProviderEvent::ToolCall(ModelToolCall {
                        id: "usage-read".into(),
                        name: "read_text".into(),
                        arguments: r#"{"path":"notes.txt"}"#.into(),
                    }))
                    .await
                    .map_err(|_| ProviderError::ConsumerClosed)?;
            } else {
                sender
                    .send(ProviderEvent::Delta("final answer".into()))
                    .await
                    .map_err(|_| ProviderError::ConsumerClosed)?;
            }
            sender
                .send(ProviderEvent::Completed)
                .await
                .map_err(|_| ProviderError::ConsumerClosed)
        }
    }

    #[derive(Debug)]
    struct LeakyFailingProbeProvider;

    #[async_trait]
    impl ModelProvider for LeakyFailingProbeProvider {
        fn id(&self) -> &'static str {
            "leaky-failure"
        }

        fn status(&self) -> argentum_domain::ProviderStatus {
            argentum_domain::ProviderStatus {
                profile_id: self.id().into(),
                kind: ProviderKind::OpenAiCompatible,
                label: "Failing provider".into(),
                endpoint: "https://user:password@example.test/v1/?api_key=secret-query#fragment"
                    .into(),
                connected: false,
                detail: "Not tested".into(),
            }
        }

        async fn probe(&self) -> Result<argentum_domain::ProviderStatus, ProviderError> {
            Err(ProviderError::Api("secret response body".into()))
        }

        async fn stream(
            &self,
            _request: ModelRequest,
            _sender: mpsc::Sender<ProviderEvent>,
        ) -> Result<(), ProviderError> {
            Ok(())
        }
    }

    #[derive(Debug)]
    struct RoutingProvider {
        id: &'static str,
        response: &'static str,
    }

    #[async_trait]
    impl ModelProvider for RoutingProvider {
        fn id(&self) -> &'static str {
            self.id
        }

        fn status(&self) -> argentum_domain::ProviderStatus {
            argentum_domain::ProviderStatus {
                profile_id: self.id.into(),
                kind: ProviderKind::OpenAiCompatible,
                label: self.id.into(),
                endpoint: format!("https://{}.example.test/v1/", self.id),
                connected: true,
                detail: "test provider".into(),
            }
        }

        async fn stream(
            &self,
            _request: ModelRequest,
            sender: mpsc::Sender<ProviderEvent>,
        ) -> Result<(), ProviderError> {
            sender
                .send(ProviderEvent::Delta(self.response.into()))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
            sender
                .send(ProviderEvent::Completed)
                .await
                .map_err(|_| ProviderError::ConsumerClosed)
        }
    }

    #[derive(Debug)]
    struct CatalogProvider;

    #[async_trait]
    impl ModelProvider for CatalogProvider {
        fn id(&self) -> &'static str {
            "catalog"
        }

        fn status(&self) -> ProviderStatus {
            test_provider_status(self.id())
        }

        async fn list_models(&self) -> Result<Vec<ProviderCatalogModel>, ProviderError> {
            Ok(vec![
                ProviderCatalogModel {
                    id: "model-a".into(),
                    label: "Model A".into(),
                    context_window_tokens: Some(32_768),
                },
                ProviderCatalogModel {
                    id: "model-b".into(),
                    label: "Model B".into(),
                    context_window_tokens: None,
                },
            ])
        }

        async fn stream(
            &self,
            _request: ModelRequest,
            sender: mpsc::Sender<ProviderEvent>,
        ) -> Result<(), ProviderError> {
            sender
                .send(ProviderEvent::ReasoningDelta("bounded reasoning".into()))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
            sender
                .send(ProviderEvent::Usage(ProviderUsage {
                    input_tokens: 12,
                    output_tokens: 4,
                    total_tokens: 16,
                    cached_input_tokens: None,
                    reasoning_tokens: Some(2),
                    context_window_tokens: None,
                }))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
            sender
                .send(ProviderEvent::Delta("catalog response".into()))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
            sender
                .send(ProviderEvent::Completed)
                .await
                .map_err(|_| ProviderError::ConsumerClosed)
        }
    }

    #[derive(Debug)]
    struct ModelCapturingProvider {
        id: &'static str,
        seen_models: Arc<Mutex<Vec<String>>>,
    }

    #[async_trait]
    impl ModelProvider for ModelCapturingProvider {
        fn id(&self) -> &'static str {
            self.id
        }

        fn status(&self) -> ProviderStatus {
            test_provider_status(self.id())
        }

        async fn stream(
            &self,
            request: ModelRequest,
            sender: mpsc::Sender<ProviderEvent>,
        ) -> Result<(), ProviderError> {
            self.seen_models
                .lock()
                .map_err(|_| ProviderError::InvalidResponse("test model lock failed".into()))?
                .push(request.model);
            sender
                .send(ProviderEvent::Delta("captured".into()))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
            sender
                .send(ProviderEvent::Completed)
                .await
                .map_err(|_| ProviderError::ConsumerClosed)
        }
    }

    #[derive(Debug)]
    struct MultiDeltaProvider;

    #[async_trait]
    impl ModelProvider for MultiDeltaProvider {
        fn id(&self) -> &'static str {
            "multi-delta"
        }

        fn status(&self) -> argentum_domain::ProviderStatus {
            test_provider_status(self.id())
        }

        async fn stream(
            &self,
            _request: ModelRequest,
            sender: mpsc::Sender<ProviderEvent>,
        ) -> Result<(), ProviderError> {
            for delta in ["three", " ", "deltas"] {
                sender
                    .send(ProviderEvent::Delta(delta.into()))
                    .await
                    .map_err(|_| ProviderError::ConsumerClosed)?;
            }
            sender
                .send(ProviderEvent::Completed)
                .await
                .map_err(|_| ProviderError::ConsumerClosed)
        }
    }

    #[derive(Debug)]
    struct PartialFailingProvider;

    #[async_trait]
    impl ModelProvider for PartialFailingProvider {
        fn id(&self) -> &'static str {
            "partial-failure"
        }

        fn status(&self) -> argentum_domain::ProviderStatus {
            test_provider_status(self.id())
        }

        async fn stream(
            &self,
            _request: ModelRequest,
            sender: mpsc::Sender<ProviderEvent>,
        ) -> Result<(), ProviderError> {
            sender
                .send(ProviderEvent::Delta("partial response".into()))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
            Err(ProviderError::Api("failure after partial output".into()))
        }
    }

    #[derive(Debug)]
    struct BlockingProvider;

    #[async_trait]
    impl ModelProvider for BlockingProvider {
        fn id(&self) -> &'static str {
            "blocking"
        }

        fn status(&self) -> argentum_domain::ProviderStatus {
            test_provider_status(self.id())
        }

        async fn stream(
            &self,
            _request: ModelRequest,
            sender: mpsc::Sender<ProviderEvent>,
        ) -> Result<(), ProviderError> {
            sender
                .send(ProviderEvent::Delta("partial before wait".into()))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
            std::future::pending::<()>().await;
            Ok(())
        }
    }

    #[derive(Debug)]
    struct RecordingProvider {
        requests: Arc<Mutex<Vec<ModelRequest>>>,
    }

    #[async_trait]
    impl ModelProvider for RecordingProvider {
        fn id(&self) -> &'static str {
            "recording"
        }

        fn status(&self) -> argentum_domain::ProviderStatus {
            test_provider_status(self.id())
        }

        async fn stream(
            &self,
            request: ModelRequest,
            sender: mpsc::Sender<ProviderEvent>,
        ) -> Result<(), ProviderError> {
            self.requests.lock().expect("requests").push(request);
            sender
                .send(ProviderEvent::Delta("recorded answer".into()))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
            sender
                .send(ProviderEvent::Completed)
                .await
                .map_err(|_| ProviderError::ConsumerClosed)
        }
    }

    #[derive(Debug)]
    struct ModelToolProvider {
        requests: Arc<Mutex<Vec<ModelRequest>>>,
        tool_name: &'static str,
        arguments: &'static str,
        final_text: &'static str,
    }

    #[async_trait]
    impl ModelProvider for ModelToolProvider {
        fn id(&self) -> &'static str {
            "model-tool"
        }

        fn status(&self) -> argentum_domain::ProviderStatus {
            test_provider_status(self.id())
        }

        async fn stream(
            &self,
            request: ModelRequest,
            sender: mpsc::Sender<ProviderEvent>,
        ) -> Result<(), ProviderError> {
            let has_result = !request.tool_exchanges.is_empty();
            self.requests.lock().expect("requests").push(request);
            if has_result {
                sender
                    .send(ProviderEvent::Delta(self.final_text.into()))
                    .await
                    .map_err(|_| ProviderError::ConsumerClosed)?;
            } else {
                sender
                    .send(ProviderEvent::ToolCall(ModelToolCall {
                        id: "provider-call-1".into(),
                        name: self.tool_name.into(),
                        arguments: self.arguments.into(),
                    }))
                    .await
                    .map_err(|_| ProviderError::ConsumerClosed)?;
            }
            sender
                .send(ProviderEvent::Completed)
                .await
                .map_err(|_| ProviderError::ConsumerClosed)
        }
    }

    #[derive(Debug)]
    struct LoopingToolProvider {
        requests: Arc<Mutex<Vec<ModelRequest>>>,
    }

    #[async_trait]
    impl ModelProvider for LoopingToolProvider {
        fn id(&self) -> &'static str {
            "looping-tool"
        }

        fn status(&self) -> argentum_domain::ProviderStatus {
            test_provider_status(self.id())
        }

        async fn stream(
            &self,
            request: ModelRequest,
            sender: mpsc::Sender<ProviderEvent>,
        ) -> Result<(), ProviderError> {
            let call_number = request.tool_exchanges.len();
            self.requests.lock().expect("requests").push(request);
            sender
                .send(ProviderEvent::ToolCall(ModelToolCall {
                    id: format!("loop-call-{call_number}"),
                    name: "read_text".into(),
                    arguments: r#"{"path":"loop.txt"}"#.into(),
                }))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
            sender
                .send(ProviderEvent::Completed)
                .await
                .map_err(|_| ProviderError::ConsumerClosed)
        }
    }

    #[derive(Debug)]
    struct OversizedOutputProvider;

    #[async_trait]
    impl ModelProvider for OversizedOutputProvider {
        fn id(&self) -> &'static str {
            "oversized-output"
        }

        fn status(&self) -> argentum_domain::ProviderStatus {
            test_provider_status(self.id())
        }

        async fn stream(
            &self,
            _request: ModelRequest,
            sender: mpsc::Sender<ProviderEvent>,
        ) -> Result<(), ProviderError> {
            sender
                .send(ProviderEvent::Delta(
                    "o".repeat(MAX_VISIBLE_ASSISTANT_BYTES + 1),
                ))
                .await
                .map_err(|_| ProviderError::ConsumerClosed)?;
            sender
                .send(ProviderEvent::Completed)
                .await
                .map_err(|_| ProviderError::ConsumerClosed)
        }
    }

    fn test_provider_status(id: &str) -> argentum_domain::ProviderStatus {
        argentum_domain::ProviderStatus {
            profile_id: id.into(),
            kind: ProviderKind::OpenAiCompatible,
            label: id.into(),
            endpoint: format!("https://{id}.example.test/v1/"),
            connected: true,
            detail: "test provider".into(),
        }
    }
}
