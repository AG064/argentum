//! Owner-driven settings. Labels are display data; actions are typed values.
use super::*;
use argentum_domain::{HarnessCapabilityKind, ProviderKind, VerificationRunnerId};
use argentum_providers::PreparedLocalRuntime;
use argentum_runtime::RuntimeError;
use argentum_security::SecretValue;
use unicode_segmentation::UnicodeSegmentation;
use zeroize::Zeroize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditKind {
    Model(String),
    Endpoint(String),
    Credential(String),
    Budget(String),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    ChatGptLogin,
    ChatGptLogout,
    Open(menu::View),
    Provider(String),
    Model(String, String),
    Refresh(String),
    Edit(EditKind),
    SubmitInput,
    Context(String, u32),
    Prepare(String),
    Activate,
    Unload,
    AddPreset(String),
    Confirm(Box<Action>),
    Command(AppCommand),
    Tool(String),
    Capability(String),
    Skill(String),
    Verifier(VerificationRunnerId),
    UseSkill(String),
    ClearSkill,
    ContextDetails,
    Fork {
        source_session_id: argentum_domain::SessionId,
        title: String,
        expected_last_message_id: argentum_domain::MessageId,
    },
}
struct Entry {
    label: String,
    action: Action,
}
struct Editor {
    kind: EditKind,
    value: String,
    cursor: usize,
    secret: credential_input::SecretInput,
}
pub(super) struct State {
    entries: Vec<Entry>,
    selected: usize,
    offset: usize,
    notice: String,
    parent: Option<menu::View>,
    editor: Option<Editor>,
    pending_secret: Option<SecretValue>,
    prepared: Option<PreparedLocalRuntime>,
    confirm: Option<Action>,
    ticket: RunId,
    cancel: tokio_util::sync::CancellationToken,
    max_offset: std::cell::Cell<usize>,
    approval: Option<argentum_domain::ApprovalId>,
}
impl Drop for State {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
pub(super) struct Completion {
    ticket: RunId,
    scope: Option<SubmissionTarget>,
    action: Action,
    result: Result<Effect, String>,
}
enum Effect {
    Done,
    Prepared(Box<PreparedLocalRuntime>),
    Forked(Box<argentum_domain::SessionForkSnapshot>),
}
fn state(t: &TuiState) -> Option<&State> {
    t.overlay.as_ref()?.settings.as_ref()
}
fn state_mut(t: &mut TuiState) -> Option<&mut State> {
    t.overlay.as_mut()?.settings.as_mut()
}
fn selected(local: &LocalCommandHandle, local_text: bool) -> Result<ProviderProfile, HostError> {
    local
        .provider_profiles()?
        .into_iter()
        .find(|p| {
            if local_text {
                p.id == "llama-cpp"
            } else {
                p.selected
            }
        })
        .ok_or(RuntimeError::NoProvider.into())
}
pub(super) fn supports(view: menu::View) -> bool {
    matches!(
        view,
        menu::View::Model
            | menu::View::Providers
            | menu::View::Setup
            | menu::View::Context
            | menu::View::Sessions
            | menu::View::Fork
            | menu::View::Tools
            | menu::View::Verification
            | menu::View::Skills
    )
}
fn entry(entries: &mut Vec<Entry>, label: impl Into<String>, action: Action) {
    entries.push(Entry {
        label: label.into(),
        action,
    });
}
fn install(
    t: &mut TuiState,
    view: menu::View,
    entries: Vec<Entry>,
    notice: String,
    parent: Option<menu::View>,
) {
    let mut overlay = menu::Overlay::new(view);
    overlay.settings = Some(State {
        entries,
        selected: 0,
        offset: 0,
        notice,
        parent,
        editor: None,
        pending_secret: None,
        prepared: None,
        confirm: None,
        ticket: RunId::new_v4(),
        cancel: tokio_util::sync::CancellationToken::new(),
        max_offset: std::cell::Cell::new(0),
        approval: None,
    });
    t.overlay = Some(overlay);
}
pub(super) fn open(
    t: &mut TuiState,
    local: &LocalCommandHandle,
    view: menu::View,
) -> Result<(), HostError> {
    let mut entries = Vec::new();
    let mut notice = String::new();
    let profile = selected(local, t.local_text_mode)?;
    match view {
        menu::View::Providers => {
            notice="Enter selects a provider. Its model and required configuration follow. No inference is started by selection.".into();
            for p in local.provider_profiles()?.into_iter().take(128) {
                entry(
                    &mut entries,
                    format!(
                        "{} [{}]{}",
                        p.label,
                        p.id,
                        if p.selected { " [selected]" } else { "" }
                    ),
                    Action::Provider(p.id),
                );
            }
            for p in argentum_providers::provider_presets()
                .iter()
                .filter(|p| matches!(p.id, "minimax" | "deepseek" | "openai" | "chatgpt"))
            {
                if !local
                    .provider_profiles()?
                    .iter()
                    .any(|saved| saved.id == p.id)
                {
                    entry(
                        &mut entries,
                        format!("Add {}", p.label),
                        Action::AddPreset(p.id.into()),
                    );
                }
            }
        }
        menu::View::Model => {
            notice=format!("Provider: {}. Choose a model or enter its exact ID. Refresh asks the configured server for its catalog, not an answer.",profile.label);
            for model in local.model_choices(&profile)? {
                entry(
                    &mut entries,
                    format!(
                        "{model}{}",
                        if model == profile.model {
                            " [selected]"
                        } else {
                            ""
                        }
                    ),
                    Action::Model(profile.id.clone(), model),
                );
            }
            entry(
                &mut entries,
                "Refresh available models",
                Action::Refresh(profile.id.clone()),
            );
            entry(
                &mut entries,
                "Enter model ID",
                Action::Edit(EditKind::Model(profile.id.clone())),
            );
            if profile.kind == ProviderKind::LocalLlamaCpp {
                entry(
                    &mut entries,
                    "Review and activate selected local model",
                    Action::Prepare(profile.model.clone()),
                );
                entry(&mut entries, "Unload owned local model", Action::Unload);
            }
        }
        menu::View::Setup => {
            notice = format!(
                "Current: {} / {}. Choose a setting. Draft stays in the composer.",
                profile.label, profile.model
            );
            for (label, v) in [
                ("Provider", menu::View::Providers),
                ("Model", menu::View::Model),
                ("Context capacity", menu::View::Context),
                ("Permissions", menu::View::Permissions),
                ("Sessions", menu::View::Sessions),
                ("Tools", menu::View::Tools),
                ("Verification", menu::View::Verification),
            ] {
                entry(&mut entries, label, Action::Open(v));
            }
            if argentum_domain::is_supported_hosted_credential_provider_id(&profile.id) {
                entry(
                    &mut entries,
                    "Enter or replace API key (masked)",
                    Action::Edit(EditKind::Credential(profile.id.clone())),
                );
            }
            if profile.id == "chatgpt" {
                entry(
                    &mut entries,
                    "Sign in to ChatGPT in your browser",
                    Action::Confirm(Box::new(Action::ChatGptLogin)),
                );
                if local.provider_credential_configured("chatgpt") {
                    entry(
                        &mut entries,
                        "Sign out of selected ChatGPT account",
                        Action::Confirm(Box::new(Action::ChatGptLogout)),
                    );
                }
            }
            if profile.kind != ProviderKind::LocalLlamaCpp && profile.id != "chatgpt" {
                entry(
                    &mut entries,
                    "Edit provider endpoint (never put an API key here)",
                    Action::Edit(EditKind::Endpoint(profile.id.clone())),
                );
            }
        }
        menu::View::Context => {
            let budget = local.profile_model_budget(&profile).ok().flatten();
            notice=format!("Provider: {}. Configured context: {}. Choose a window; a local size change requires activation before chatting. External servers must actually support the chosen size.",profile.label,budget.as_ref().map_or_else(||"not set".into(),|b|b.usable_context_tokens.to_string()));
            let ceiling = argentum_providers::bundled_context_window(&profile);
            let mut sizes: Vec<u32> = argentum_providers::SIDECAR_CONTEXT_VARIANTS
                .into_iter()
                .filter(|n| ceiling.map_or(true, |c| u64::from(*n) <= c))
                .collect();
            if profile.kind != ProviderKind::LocalLlamaCpp {
                if let Some(c) = ceiling.and_then(|c| u32::try_from(c).ok()) {
                    if !sizes.contains(&c) {
                        sizes.push(c);
                    }
                }
            }
            if profile.kind != ProviderKind::LocalLlamaCpp {
                sizes.clear();
                let limits = budget.as_ref().map_or_else(
                    || "not set or stale".into(),
                    |b| {
                        format!(
                            "{},{},{},{},{}",
                            b.usable_context_tokens,
                            b.input_limit_tokens.unwrap_or(b.usable_context_tokens),
                            b.maximum_output_tokens,
                            b.requested_output_tokens,
                            b.uncertainty_tokens
                        )
                    },
                );
                notice = format!("Provider: {}. Limits: {limits}. Declare TOTAL,INPUT,MAX_OUTPUT,REQUESTED_OUTPUT,UNCERTAINTY explicitly. Values are owner supplied, not provider verified. A model or endpoint change requires a matching declaration.", profile.label);
                entry(
                    &mut entries,
                    "Edit all five budget limits",
                    Action::Edit(EditKind::Budget(profile.id.clone())),
                );
            }
            for size in sizes {
                entry(
                    &mut entries,
                    format!(
                        "{}K ({} tokens){}",
                        size / 1024,
                        size,
                        if budget
                            .as_ref()
                            .is_some_and(|b| b.usable_context_tokens == u64::from(size))
                        {
                            " [selected]"
                        } else {
                            ""
                        }
                    ),
                    Action::Context(profile.id.clone(), size),
                );
            }
            entry(
                &mut entries,
                "Inspect selected request context",
                Action::ContextDetails,
            );
        }
        menu::View::Sessions => {
            notice="Enter resumes the selected session. Each session keeps its draft; retained work does not resume automatically.".into();
            let workspace = local.workspace_snapshot()?;
            for session in workspace.sessions.into_iter().take(128) {
                entry(
                    &mut entries,
                    format!(
                        "{} [{}]{}",
                        session.title,
                        session.id,
                        if Some(session.id) == workspace.active_session_id {
                            " [selected]"
                        } else {
                            ""
                        }
                    ),
                    Action::Command(AppCommand::SelectSession {
                        session_id: session.id,
                    }),
                );
            }
            entry(
                &mut entries,
                "New session",
                Action::Command(AppCommand::NewSession),
            );
            if workspace.active_session_id.is_some() {
                entry(
                    &mut entries,
                    "Fork current conversation...",
                    Action::Open(menu::View::Fork),
                );
            }
        }
        menu::View::Fork => {
            let workspace = local.workspace_snapshot()?;
            let source_session_id = workspace
                .active_session_id
                .ok_or(RuntimeError::SubmissionTargetChanged)?;
            let source = workspace
                .sessions
                .iter()
                .find(|session| session.id == source_session_id)
                .ok_or(RuntimeError::SubmissionTargetChanged)?;
            let title = format!("Fork of {}", source.title);
            let preview = local.session_fork_preview(source_session_id, &title)?;
            notice = format!(
                "Fork conversation preview\nSource: {} ({})\nCutoff message: {}\nDestination: {}\nWorkspace: {}\nOmitted older sources: {}\n{}\nCreating does not select the child or submit its draft. Escape cancels without changes.",
                preview.source_session_id,
                preview.source_title,
                preview.cutoff.last_message_id,
                preview.destination_title,
                preview.workspace.display(),
                preview.omitted_older_sources,
                preview.warning,
            );
            entry(
                &mut entries,
                "Create fork",
                Action::Fork {
                    source_session_id,
                    title,
                    expected_last_message_id: preview.cutoff.last_message_id,
                },
            );
        }
        menu::View::Tools => {
            let h = local.harness_snapshot()?;
            notice="Choose a tool or agent capability to inspect or change its enabled state. Changes still obey the workspace policy.".into();
            for tool in local.tool_descriptors().into_iter().take(128) {
                let enabled = h
                    .capabilities
                    .iter()
                    .any(|c| c.id == tool.capability_id && c.enabled);
                entry(
                    &mut entries,
                    format!(
                        "{} [{}]",
                        tool.id,
                        if enabled { "enabled" } else { "disabled" }
                    ),
                    Action::Tool(tool.id),
                );
            }
            for capability in h
                .capabilities
                .iter()
                .filter(|capability| {
                    capability.kind == HarnessCapabilityKind::Agent && capability.configurable
                })
                .take(128)
            {
                entry(
                    &mut entries,
                    format!(
                        "{} [agent, {}]",
                        capability.label,
                        if capability.enabled {
                            "enabled"
                        } else {
                            "disabled"
                        }
                    ),
                    Action::Capability(capability.id.clone()),
                );
            }
        }
        menu::View::Skills => {
            notice="Choose a skill to inspect, then explicitly attach it to the next message. Inspection alone does not activate instructions.".into();
            entry(
                &mut entries,
                "No skill for the next message",
                Action::ClearSkill,
            );
            for skill in local.skill_catalog()?.entries.into_iter().take(128) {
                if let Some(name) = skill.name {
                    entry(
                        &mut entries,
                        format!(
                            "{} [{}]",
                            name,
                            if skill.valid { "valid" } else { "unavailable" }
                        ),
                        Action::Skill(name),
                    );
                }
            }
        }
        menu::View::Verification => {
            notice =
                "Choose a fixed verifier to inspect and run. This does not submit a model prompt."
                    .into();
            for runner in [
                VerificationRunnerId::CargoFmtCheck,
                VerificationRunnerId::CargoCheckLocked,
                VerificationRunnerId::CargoTestLocked,
                VerificationRunnerId::CargoClippyStrict,
            ] {
                entry(&mut entries, runner.id(), Action::Verifier(runner));
            }
            entry(
                &mut entries,
                "Review recorded verification and changes",
                Action::Open(menu::View::Changes),
            );
        }
        _ => {}
    }
    if entries.is_empty() {
        notice.push_str(" No entries are available.");
    }
    install(t, view, entries, notice, None);
    Ok(())
}
pub(super) async fn refresh_preflight(
    t: &mut TuiState,
    local: &LocalCommandHandle,
) -> Result<(), HostError> {
    let p = selected(local, t.local_text_mode)?;
    let mut issue = if t.local_text_mode {
        None
    } else {
        local.selected_provider_coding_issue()?
    };
    if !t.local_text_mode {
        if argentum_domain::is_supported_hosted_credential_provider_id(&p.id)
            && !local.provider_credential_configured(&p.id)
        {
            issue = Some("API key is missing; open /setup".into());
        } else if p.kind == ProviderKind::LocalLlamaCpp && !local.local_model_ready(&p.model).await
        {
            issue = Some("Selected local model needs activation; open /models".into());
        }
    } else if !local.local_model_ready(&p.model).await {
        issue = Some("Selected local model needs activation; open /models".into());
    }
    t.set_coding_preflight_issue(issue);
    Ok(())
}
fn busy(t: &TuiState) -> bool {
    t.settings_busy
        || t.access_pending
        || t.active_run_id.is_some()
        || t.manual_compaction.is_some()
        || t.pending_approval.is_some()
        || t.session_transition
        || t.submission_target()
            .is_some_and(|target| t.followups.has_in_flight(&target))
}
fn mutating(action: &Action) -> bool {
    matches!(
        action,
        Action::Provider(_)
            | Action::ChatGptLogin
            | Action::ChatGptLogout
            | Action::Model(..)
            | Action::Context(..)
            | Action::Prepare(_)
            | Action::Activate
            | Action::Unload
            | Action::AddPreset(_)
            | Action::Command(_)
            | Action::SubmitInput
    )
}
pub(super) fn approval(t: &mut TuiState, request: &ApprovalRequest) {
    let detail=format!("Approval required\nAction: {}\nTarget: {}\nCapabilities: {:?}\nReason: {}\nApprove once permits only this exact request; Escape rejects it.",request.action,request.target,request.capabilities,request.reason);
    install(
        t,
        menu::View::OwnerConfirm,
        vec![
            Entry {
                label: "Reject".into(),
                action: Action::Command(AppCommand::RejectTool {
                    approval_id: request.id,
                }),
            },
            Entry {
                label: "Approve once".into(),
                action: Action::Command(AppCommand::ApproveTool {
                    approval_id: request.id,
                    scope: ApprovalScope::Once,
                }),
            },
        ],
        detail,
        None,
    );
    state_mut(t).unwrap().approval = Some(request.id);
}
pub(super) fn dispatch(
    t: &mut TuiState,
    local: &LocalCommandHandle,
    tasks: &mut JoinSet<CommandCompletion>,
    action: Action,
) {
    if let Action::Open(view) = action {
        let parent = t.overlay.as_ref().map(|o| o.view);
        if let Err(e) = menu::open_view(t, local, view) {
            t.status = e.to_string();
        }
        if let Some(s) = state_mut(t) {
            s.parent = parent;
        }
        if view == menu::View::Model {
            if let Ok(p) = selected(local, t.local_text_mode) {
                if p.kind != ProviderKind::LocalLlamaCpp {
                    dispatch(t, local, tasks, Action::Refresh(p.id));
                }
            }
        }
        return;
    }
    if let Action::Command(
        command @ (AppCommand::ApproveTool { .. } | AppCommand::RejectTool { .. }),
    ) = &action
    {
        t.overlay = None;
        dispatch_tui_action(t, local, tasks, TuiAction::Dispatch(command.clone()));
        return;
    }
    if t.settings_busy || tasks.len() >= 8 {
        if let Some(s) = state_mut(t) {
            s.notice = "Wait for the current settings operation, or Escape to cancel it.".into();
        }
        return;
    }
    if let Action::Command(command @ (AppCommand::SelectSession { .. } | AppCommand::NewSession)) =
        &action
    {
        if t.local_text_mode {
            if let Some(s) = state_mut(t) {
                s.notice =
                    "Text-only history is memory-only. Use normal chat for saved sessions.".into();
            }
            return;
        }
        let text = match command {
            AppCommand::SelectSession { session_id } => format!("/resume {session_id}"),
            _ => "/new".into(),
        };
        t.overlay = None;
        let next = t.handle_slash_command(&text);
        dispatch_tui_action(t, local, tasks, next);
        return;
    }
    if busy(t) && mutating(&action) {
        if let Some(s) = state_mut(t) {
            s.notice =
                "Finish or cancel active work before changing settings. Your draft is retained."
                    .into();
        }
        return;
    }
    if busy(t) && matches!(action, Action::Fork { .. }) {
        if let Some(s) = state_mut(t) {
            s.notice =
                "Settle active, queued, approval, restore, or compaction work before forking."
                    .into();
        }
        return;
    }
    if t.local_text_mode
        && matches!(
            &action,
            Action::Provider(_)
                | Action::AddPreset(_)
                | Action::Command(_)
                | Action::Verifier(_)
                | Action::UseSkill(_)
        )
    {
        if let Some(s) = state_mut(t) {
            s.notice="This session is explicitly local text-only. Use normal chat for coding providers, tools and skills; no tools were enabled.".into();
        }
        return;
    }
    match &action {
        Action::ClearSkill => {
            t.next_skill = None;
            t.status = "No skill selected for the next message.".into();
            t.overlay = None;
            return;
        }
        Action::UseSkill(name) => {
            match local.select_skill(name) {
                Ok(skill) => {
                    t.next_skill = Some(skill);
                    t.status = format!("Skill {name} selected for the next message only.");
                    t.overlay = None;
                }
                Err(e) => t.status = e.to_string(),
            }
            return;
        }
        Action::ContextDetails => {
            if let Some(id) = t.active_session_id {
                match local.context_preview(id) {
                    Ok(c) => install(
                        t,
                        menu::View::Context,
                        vec![],
                        serde_json::to_string_pretty(&c).unwrap_or_default(),
                        Some(menu::View::Context),
                    ),
                    Err(e) => t.status = e.to_string(),
                }
            }
            return;
        }
        Action::Edit(kind) => {
            if t.diagnostic_active {
                t.status = "Stop diagnostic capture before entering settings.".into();
                return;
            }
            let value =
                selected(local, t.local_text_mode)
                    .ok()
                    .map_or(String::new(), |p| match kind {
                        EditKind::Model(_) => p.model,
                        EditKind::Endpoint(_) => p.endpoint,
                        EditKind::Credential(_) => String::new(),
                        EditKind::Budget(_) => local
                            .profile_model_budget(&p)
                            .ok()
                            .flatten()
                            .map_or_else(String::new, |b| {
                                format!(
                                    "{},{},{},{},{}",
                                    b.usable_context_tokens,
                                    b.input_limit_tokens.unwrap_or(b.usable_context_tokens),
                                    b.maximum_output_tokens,
                                    b.requested_output_tokens,
                                    b.uncertainty_tokens
                                )
                            }),
                    });
            if let Some(s) = state_mut(t) {
                let cursor = value.len();
                s.editor = Some(Editor {
                    kind: kind.clone(),
                    value,
                    cursor,
                    secret: credential_input::SecretInput::default(),
                });
                s.offset = 0;
                s.notice="Enter saves. Escape cancels. Ctrl+U clears. Clipboard/terminal paste supported. API keys belong only in the masked key field.".into();
            }
            return;
        }
        Action::Confirm(next) => {
            let detail=format!("Workspace: {}\nAction: {next:?}\nHold F8 to apply this action. Escape cancels. Pasted key sequences cannot approve it.",t.workspace.display());
            if let Some(s) = state_mut(t) {
                s.confirm = Some(*next.clone());
                s.notice = detail;
                s.offset = 0;
            }
            return;
        }
        Action::Tool(id) => {
            if let Some(tool) = local.tool_descriptors().into_iter().find(|d| d.id == *id) {
                let h = local.harness_snapshot().ok();
                let enabled = h.as_ref().is_some_and(|h| {
                    h.capabilities
                        .iter()
                        .any(|c| c.id == tool.capability_id && c.enabled)
                });
                let command = AppCommand::SetHarnessCapabilityEnabled {
                    capability_id: tool.capability_id.clone(),
                    enabled: !enabled,
                };
                let entries = vec![
                    Entry {
                        label: if enabled {
                            "Disable tool"
                        } else {
                            "Enable tool"
                        }
                        .into(),
                        action: Action::Confirm(Box::new(Action::Command(command))),
                    },
                    Entry {
                        label: "Permissions".into(),
                        action: Action::Open(menu::View::Permissions),
                    },
                ];
                install(
                    t,
                    menu::View::Tools,
                    entries,
                    format!("{tool:?}"),
                    Some(menu::View::Tools),
                );
            }
            return;
        }
        Action::Capability(id) => {
            if let Some(capability) = local.harness_snapshot().ok().and_then(|snapshot| {
                snapshot
                    .capabilities
                    .into_iter()
                    .find(|capability| capability.id == *id)
            }) {
                let command = AppCommand::SetHarnessCapabilityEnabled {
                    capability_id: capability.id.clone(),
                    enabled: !capability.enabled,
                };
                let entries = vec![Entry {
                    label: if capability.enabled {
                        "Disable agent capability"
                    } else {
                        "Enable agent capability"
                    }
                    .into(),
                    action: Action::Confirm(Box::new(Action::Command(command))),
                }];
                install(
                    t,
                    menu::View::Tools,
                    entries,
                    format!(
                        "{} [{}]\n{}",
                        capability.label, capability.id, capability.detail
                    ),
                    Some(menu::View::Tools),
                );
            }
            return;
        }
        Action::Skill(name) => {
            match local.skill_catalog() {
                Ok(c) => {
                    if let Some(s) = c
                        .entries
                        .into_iter()
                        .find(|s| s.name.as_deref() == Some(name))
                    {
                        let choices = if s.valid {
                            vec![Entry {
                                label: "Use for next message only".into(),
                                action: Action::UseSkill(name.clone()),
                            }]
                        } else {
                            vec![]
                        };
                        install(
                            t,
                            menu::View::Skills,
                            choices,
                            format!("{s:?}\nNo instructions activated until explicitly selected."),
                            Some(menu::View::Skills),
                        );
                    }
                }
                Err(e) => t.status = e.to_string(),
            }
            return;
        }
        Action::Verifier(runner) => {
            let request = argentum_domain::ToolRequest {
                call_id: RunId::new_v4(),
                run_id: RunId::new_v4(),
                input: argentum_domain::ToolInput::Verify {
                    runners: vec![runner.clone()],
                },
            };
            install(t,menu::View::Verification,vec![Entry{label:format!("Run {} in this workspace",runner.id()),action:Action::Confirm(Box::new(Action::Command(AppCommand::RequestTool{request})))}],format!("Fixed runner: {}. Uses existing process, permission and revision checks; no model inference.",runner.id()),Some(menu::View::Verification));
            return;
        }
        _ => {}
    }
    let Some(s) = state_mut(t) else { return };
    let prepared = if action == Action::Activate {
        s.prepared.take()
    } else {
        None
    };
    let secret = if action == Action::SubmitInput {
        s.pending_secret.take()
    } else {
        None
    };
    let edit = s.editor.as_ref().map(|e| (e.kind.clone(), e.value.clone()));
    let ticket = s.ticket;
    let cancel = s.cancel.clone();
    let is_mutating = mutating(&action);
    s.notice = "Working on the selected setting...".into();
    let scope = t.submission_target();
    t.settings_busy = true;
    t.status = "Applying selected setting...".into();
    if is_mutating {
        t.followups
            .pause_all("Settings changed; use /queue continue before retained work resumes");
    }
    let local = local.clone();
    tasks.spawn(async move {
        let result = async {
            if cancel.is_cancelled() {
                return Err(HostError::Provider(
                    argentum_providers::ProviderError::RequestCancelled,
                ));
            }
            if is_mutating && local.access_review("observe")?.review.busy {
                return Err(HostError::Runtime(RuntimeError::ExecutionPolicyBusy));
            }
            match &action {
                Action::ChatGptLogin => {local.chatgpt_auth()?.sign_in_with_cancellation(false,cancel.clone()).await?;},
                Action::ChatGptLogout => {local.chatgpt_auth()?.sign_out().await?;},
                Action::Provider(id) => {
                    let p = local
                        .provider_profiles()?
                        .into_iter()
                        .find(|p| p.id == *id)
                        .ok_or(RuntimeError::NoProvider)?;
                    if p.kind != ProviderKind::LocalLlamaCpp {
                        local.unload_local_model().await?;
                    }
                    local
                        .dispatch(AppCommand::SelectProviderProfile {
                            provider_id: id.clone(),
                        })
                        .await?;
                    local.seed_selected_model_budget()?;
                }
                Action::Model(id, model) => {
                    local
                        .dispatch(AppCommand::SelectProviderModel {
                            provider_id: id.clone(),
                            model: model.clone(),
                        })
                        .await?;
                    local.seed_selected_model_budget()?;
                }
                Action::Refresh(id) => {
                    local
                        .refresh_model_choices(id.clone(), cancel.clone())
                        .await?
                }
                Action::Context(id, tokens) => {
                    let p = local
                        .provider_profiles()?
                        .into_iter()
                        .find(|p| p.id == *id)
                        .ok_or(RuntimeError::NoProvider)?;
                    if p.kind == ProviderKind::LocalLlamaCpp {
                        local.configure_local_context(id, *tokens)?;
                    } else {
                        return Err(RuntimeError::ContextBudget("use the five-field hosted budget editor").into());
                    }
                }
                Action::Prepare(model) => {
                    return Ok(Effect::Prepared(Box::new(
                        local.prepare_local_model(model.clone()).await?,
                    )))
                }
                Action::Activate => {
                    local
                        .activate_local_model(
                            prepared.ok_or(RuntimeError::LocalAuthorityUnavailable)?,
                            cancel.clone(),
                        )
                        .await?
                }
                Action::Unload => local.unload_local_model().await?,
                Action::AddPreset(id) => {
                    local
                        .dispatch(AppCommand::AddProviderPreset {
                            preset_id: id.clone(),
                        })
                        .await?;
                    local
                        .dispatch(AppCommand::SelectProviderProfile {
                            provider_id: id.clone(),
                        })
                        .await?;
                    local.seed_selected_model_budget()?;
                }
                Action::Command(command) => local.dispatch(command.clone()).await?,
                Action::Fork {
                    source_session_id,
                    title,
                    expected_last_message_id,
                } => {
                    return Ok(Effect::Forked(Box::new(local.fork_session(
                        *source_session_id,
                        title,
                        Some(*expected_last_message_id),
                    )?)))
                }
                Action::SubmitInput => {
                    if let Some((kind, value)) = edit {
                        match kind {
                            EditKind::Credential(id) => local.set_provider_credential(
                                &id,
                                secret.ok_or(HostError::ProviderCredentialStorageUnavailable)?,
                            )?,
                            EditKind::Budget(id) => {
                                let limits = crate::parse_model_budget_limits(&value)
                                    .map_err(|_| RuntimeError::ContextBudget("enter TOTAL,INPUT,MAX_OUTPUT,REQUESTED_OUTPUT,UNCERTAINTY as unsigned integers"))?;
                                local.declare_model_budget(&id, limits)?;
                            }
                            EditKind::Model(id) => {
                                local
                                    .dispatch(AppCommand::SelectProviderModel {
                                        provider_id: id,
                                        model: value.trim().into(),
                                    })
                                    .await?;
                                local.seed_selected_model_budget()?;
                            }
                            EditKind::Endpoint(id) => {
                                let mut p = local
                                    .provider_profiles()?
                                    .into_iter()
                                    .find(|p| p.id == id)
                                    .ok_or(RuntimeError::NoProvider)?;
                                p.endpoint = value.trim().into();
                                local
                                    .dispatch(AppCommand::SaveProviderProfile { profile: p })
                                    .await?;
                                local.seed_selected_model_budget()?;
                            }
                        }
                    }
                }
                _ => {}
            }
            Ok::<Effect, HostError>(Effect::Done)
        }
        .await
        .map_err(|e| e.to_string());
        CommandCompletion {
            settings: Some(Completion {
                ticket,
                scope,
                action,
                result,
            }),
            review: None,
            kind: CommandKind::Other,
            result: Ok(()),
            submission: None,
            compaction_detail: None,
        }
    });
}
pub(super) async fn finish(
    t: &mut TuiState,
    local: &LocalCommandHandle,
    completion: Completion,
) -> Option<Action> {
    t.settings_busy = false;
    if let Ok(profiles) = local.provider_profiles() {
        t.apply_provider_profiles(&profiles);
    }
    let _ = refresh_preflight(t, local).await;
    if completion.scope != t.submission_target()
        || !state(t).is_some_and(|s| s.ticket == completion.ticket)
    {
        return None;
    }
    match completion.result {
        Err(e) => {
            state_mut(t).unwrap().notice = format!(
                "Operation failed: {e}. Review the current saved settings before retrying."
            );
            t.status = "Settings operation failed; details in panel.".into();
        }
        Ok(Effect::Prepared(plan)) => {
            let detail=format!("LOCAL MODEL LAUNCH REVIEW\n{:#?}\nHold F8 to start this exact plan. Escape cancels. No model was started by this review.",plan.plan());
            let s = state_mut(t).unwrap();
            s.prepared = Some(*plan);
            s.confirm = Some(Action::Activate);
            s.notice = detail;
            s.editor = None;
        }
        Ok(Effect::Forked(fork)) => {
            let s = state_mut(t).unwrap();
            s.cancel = tokio_util::sync::CancellationToken::new();
            s.notice = format!(
                "Created conversation fork {}. Selection and drafts were not changed. Select it from /sessions or run /resume {}.",
                fork.child_session_id, fork.child_session_id
            );
            t.status = "Conversation fork created; current session remains selected.".into();
        }
        Ok(Effect::Done) => {
            state_mut(t).unwrap().cancel = tokio_util::sync::CancellationToken::new();
            let view = t.overlay.as_ref().unwrap().view;
            let parent = state(t).unwrap().parent;
            let next = if matches!(
                completion.action,
                Action::Provider(_) | Action::AddPreset(_)
            ) {
                menu::View::Model
            } else {
                view
            };
            if let Err(e) = open(t, local, next) {
                t.status = e.to_string();
                return None;
            }
            let s = state_mut(t).unwrap();
            s.parent = parent;
            s.notice = format!("Saved/refreshed. {}", s.notice);
            t.status = "Settings updated. Draft preserved.".into();
            if let Ok(p) = selected(local, t.local_text_mode) {
                if p.id == "chatgpt" && !local.provider_credential_configured("chatgpt") {
                    let _ = open(t, local, menu::View::Setup);
                    if let Some(s) = state_mut(t) {
                        s.notice="ChatGPT needs browser sign-in. Choose Sign in; no API key is required and no model request has been sent.".into();
                    }
                } else if argentum_domain::is_supported_hosted_credential_provider_id(&p.id)
                    && !local.provider_credential_configured(&p.id)
                {
                    dispatch(
                        t,
                        local,
                        &mut JoinSet::new(),
                        Action::Edit(EditKind::Credential(p.id)),
                    );
                } else if local.profile_model_budget(&p).ok().flatten().is_none() {
                    let _ = open(t, local, menu::View::Context);
                } else if p.kind == ProviderKind::LocalLlamaCpp
                    && !local.local_model_ready(&p.model).await
                    && matches!(
                        completion.action,
                        Action::Provider(_)
                            | Action::Model(..)
                            | Action::Context(..)
                            | Action::SubmitInput
                    )
                {
                    return Some(Action::Prepare(p.model));
                } else if p.kind != ProviderKind::LocalLlamaCpp
                    && matches!(
                        completion.action,
                        Action::Provider(_) | Action::AddPreset(_)
                    )
                {
                    return Some(Action::Refresh(p.id));
                }
            }
        }
    }
    None
}
pub(super) fn rows(t: &TuiState, width: usize, height: usize) -> Option<Vec<String>> {
    let s = state(t)?;
    let view = t.overlay.as_ref()?.view;
    let mut source = vec![format!(
        "{view:?} | settings own navigation | Esc back/close"
    )];
    source.push(
        if s.confirm.is_some() {
            "Hold F8 applies the reviewed action; Esc cancels."
        } else if s.editor.is_some() {
            "Input field owns keyboard. Enter saves, Esc cancels, Ctrl+U clears."
        } else {
            "Arrows choose; Enter opens/applies; PgUp/PgDn scroll."
        }
        .into(),
    );
    if let Some(editor) = &s.editor {
        source.push(match &editor.kind {
            EditKind::Credential(_) => editor.secret.mask(),
            _ => format!("Value: {}", editor.value),
        });
    }
    if s.editor.is_none() && s.confirm.is_none() {
        for (i, e) in s.entries.iter().enumerate().skip(s.selected) {
            source.push(format!(
                "{} {}",
                if i == s.selected { ">" } else { " " },
                e.label
            ));
        }
    }
    source.extend(s.notice.lines().map(str::to_owned));
    let all: Vec<_> = source.iter().flat_map(|s| screen::wrap(s, width)).collect();
    let max = all.len().saturating_sub(height.max(1));
    s.max_offset.set(max);
    let offset = editor_position(t, width).map_or(s.offset.min(max), |(_, y)| {
        y.saturating_add(1).saturating_sub(height).min(max)
    });
    let mut rows: Vec<_> = all.into_iter().skip(offset).take(height).collect();
    rows.resize(height, String::new());
    Some(rows)
}
fn editor_position(t: &TuiState, width: usize) -> Option<(usize, usize)> {
    let s = state(t)?;
    let e = s.editor.as_ref()?;
    let view = t.overlay.as_ref()?.view;
    let header = screen::wrap(
        &format!("{view:?} | settings own navigation | Esc back/close"),
        width,
    )
    .len()
        + screen::wrap(
            "Input field owns keyboard. Enter saves, Esc cancels, Ctrl+U clears.",
            width,
        )
        .len();
    let (value, byte) = if matches!(e.kind, EditKind::Credential(_)) {
        let mask = e.secret.mask();
        let byte = mask.len();
        (mask, byte)
    } else {
        (format!("Value: {}", e.value), 7 + e.cursor)
    };
    let (_, cursor) = screen::layout(&value, width, Some(byte));
    Some((cursor.0, header + cursor.1))
}
pub(super) fn cursor(t: &TuiState, width: usize, height: usize) -> Option<(u16, u16)> {
    let (x, y) = editor_position(t, width)?;
    let offset = y.saturating_add(1).saturating_sub(height);
    Some((
        x.min(width.saturating_sub(1)) as u16,
        y.saturating_sub(offset).min(height.saturating_sub(1)) as u16,
    ))
}
pub(super) fn input(t: &mut TuiState, input: InputEvent) -> TuiAction {
    // Approval navigation must not swallow ordinary follow-up typing. The
    // request remains pending; only an explicit approval response settles it.
    if state(t).is_some_and(|s| s.approval.is_some())
        && matches!(
            input,
            InputEvent::Text(_)
                | InputEvent::Paste(_)
                | InputEvent::PasteClipboard
                | InputEvent::Backspace
                | InputEvent::Delete
                | InputEvent::Left
                | InputEvent::Right
                | InputEvent::Home
                | InputEvent::End
                | InputEvent::NewLine
                | InputEvent::ClearInput
        )
    {
        t.overlay = None;
        return t.apply_input(input);
    }
    if let InputEvent::Resize(w, _) = input {
        t.terminal_width = w.max(1);
        return TuiAction::Redraw;
    }
    if input == InputEvent::Eof {
        t.begin_shutdown();
        return TuiAction::Exit;
    }
    if input == InputEvent::CtrlC
        && state(t)
            .is_some_and(|s| s.editor.is_none() && s.confirm.is_none() && s.approval.is_none())
    {
        t.overlay = None;
        if t.active_run_id.is_some() || t.manual_compaction.is_some() {
            return t.handle_ctrl_c();
        }
        return TuiAction::Redraw;
    }
    if input == InputEvent::Escape || input == InputEvent::CtrlC {
        if let Some(id) = state(t).and_then(|s| s.approval) {
            t.overlay = None;
            return TuiAction::Ui(menu::Request::Settings(Action::Command(
                AppCommand::RejectTool { approval_id: id },
            )));
        }
        let s = state_mut(t).unwrap();
        if s.confirm.take().is_some() {
            s.prepared = None;
            s.notice = "Action cancelled; no approval granted.".into();
            return TuiAction::Redraw;
        }
        if s.editor.take().is_some() {
            s.pending_secret = None;
            s.notice = "Input cancelled; saved settings kept.".into();
            return TuiAction::Redraw;
        }
        if let Some(parent) = s.parent {
            return TuiAction::Ui(menu::Request::Open(parent));
        }
        t.overlay = None;
        return TuiAction::Redraw;
    }
    if t.settings_busy {
        return TuiAction::Redraw;
    }
    let s = state_mut(t).unwrap();
    if let Some(action) = s.confirm.clone() {
        if input == InputEvent::ConfirmAccess(true) {
            s.confirm = None;
            return TuiAction::Ui(menu::Request::Settings(action));
        }
        let offset = s.offset.min(s.max_offset.get());
        if let InputEvent::Scroll(n) = input {
            s.offset = (offset as i64 + i64::from(n)).clamp(0, s.max_offset.get() as i64) as usize;
        } else if input == InputEvent::HistoryNext {
            s.offset = (offset + 1).min(s.max_offset.get());
        } else if input == InputEvent::HistoryPrevious {
            s.offset = offset.saturating_sub(1);
        }
        return TuiAction::Redraw;
    }
    if let Some(editor) = &mut s.editor {
        if matches!(editor.kind, EditKind::Credential(_)) {
            let event = match input {
                InputEvent::Text(text) | InputEvent::Paste(text) => Some(Event::Paste(text)),
                InputEvent::PasteClipboard => match credential_input::read_clipboard() {
                    Ok(text) => Some(Event::Paste(text.to_string())),
                    Err(_) => {
                        s.notice =
                            "Clipboard unavailable; use terminal paste or type the key.".into();
                        None
                    }
                },
                InputEvent::ClearInput => Some(Event::Key(KeyEvent::new(
                    KeyCode::Char('u'),
                    KeyModifiers::CONTROL,
                ))),
                InputEvent::Backspace => Some(Event::Key(KeyEvent::new(
                    KeyCode::Backspace,
                    KeyModifiers::NONE,
                ))),
                InputEvent::Enter => Some(Event::Key(KeyEvent::new(
                    KeyCode::Enter,
                    KeyModifiers::NONE,
                ))),
                _ => None,
            };
            if let Some(result) = event.and_then(|e| editor.secret.event(e)) {
                match result {
                    Ok(secret) => {
                        s.pending_secret = Some(secret);
                        return TuiAction::Ui(menu::Request::Settings(Action::SubmitInput));
                    }
                    Err(_) => s.notice = "Key not saved: enter one nonempty bounded line.".into(),
                }
            }
        } else {
            match input {
                InputEvent::Text(mut text) | InputEvent::Paste(mut text) => {
                    let clean = text.trim_end_matches(['\r', '\n']);
                    if !clean.chars().any(char::is_control)
                        && editor.value.len() + clean.len() <= 2048
                    {
                        editor.value.insert_str(editor.cursor, clean);
                        editor.cursor += clean.len();
                    } else {
                        s.notice = "Input must be one line, at most 2048 bytes.".into();
                    }
                    text.zeroize();
                }
                InputEvent::PasteClipboard => {
                    if let Ok(text) = credential_input::read_clipboard() {
                        let value = text.trim();
                        if !value.chars().any(char::is_control)
                            && editor.value.len() + value.len() <= 2048
                        {
                            editor.value.insert_str(editor.cursor, value);
                            editor.cursor += value.len();
                        }
                    }
                }
                InputEvent::ClearInput => {
                    editor.value.clear();
                    editor.cursor = 0;
                }
                InputEvent::Home => editor.cursor = 0,
                InputEvent::End => editor.cursor = editor.value.len(),
                InputEvent::Left | InputEvent::Backspace => {
                    let previous = editor.value[..editor.cursor]
                        .grapheme_indices(true)
                        .next_back()
                        .map_or(0, |(i, _)| i);
                    if input == InputEvent::Backspace {
                        editor.value.drain(previous..editor.cursor);
                    }
                    editor.cursor = previous;
                }
                InputEvent::Right | InputEvent::Delete => {
                    let next = editor.value[editor.cursor..]
                        .grapheme_indices(true)
                        .nth(1)
                        .map_or(editor.value.len(), |(i, _)| editor.cursor + i);
                    if input == InputEvent::Delete {
                        editor.value.drain(editor.cursor..next);
                    } else {
                        editor.cursor = next;
                    }
                }
                InputEvent::Enter => {
                    return TuiAction::Ui(menu::Request::Settings(Action::SubmitInput))
                }
                _ => {}
            }
        }
        return TuiAction::Redraw;
    }
    match input {
        InputEvent::HistoryPrevious => {
            s.selected = s.selected.saturating_sub(1);
            s.offset = 0;
        }
        InputEvent::HistoryNext => {
            s.selected = (s.selected + 1).min(s.entries.len().saturating_sub(1));
            s.offset = 0;
        }
        InputEvent::Scroll(n) => {
            s.offset = (s.offset.min(s.max_offset.get()) as i64 + i64::from(n))
                .clamp(0, s.max_offset.get() as i64) as usize
        }
        InputEvent::Enter => {
            if let Some(e) = s.entries.get(s.selected) {
                return TuiAction::Ui(menu::Request::Settings(e.action.clone()));
            }
        }
        _ => {}
    }
    TuiAction::Redraw
}

#[cfg(test)]
mod tests {
    use super::*;
    use argentum_domain::{ConversationMessage, ConversationMessageStatus};
    use argentum_platform::{PlatformError, SecretStore};
    use argentum_providers::ProviderRegistry;
    use argentum_runtime::RuntimeService;
    use argentum_security::{ApprovalPolicy, CapabilityBroker};
    use argentum_store::Store;
    use argentum_tools::ToolRegistry;
    use argentum_workspaces::WorkspaceManager;
    #[derive(Default)]
    struct Vault(std::sync::Mutex<std::collections::BTreeMap<String, SecretValue>>);
    impl SecretStore for Vault {
        fn get(&self, key: &str) -> Result<Option<SecretValue>, PlatformError> {
            Ok(self.0.lock().unwrap().get(key).cloned())
        }
        fn set(&self, key: &str, value: SecretValue) -> Result<(), PlatformError> {
            self.0.lock().unwrap().insert(key.into(), value);
            Ok(())
        }
        fn delete(&self, key: &str) -> Result<(), PlatformError> {
            self.0.lock().unwrap().remove(key);
            Ok(())
        }
    }
    async fn settle(
        t: &mut TuiState,
        local: &LocalCommandHandle,
        tasks: &mut JoinSet<CommandCompletion>,
    ) {
        while let Some(result) = tasks.join_next().await {
            let completion = result.unwrap();
            if let Some(c) = completion.settings {
                if let Some(next) = finish(t, local, c).await {
                    dispatch(t, local, tasks, next);
                }
            } else {
                assert!(completion.result.is_ok());
            }
        }
    }

    #[tokio::test]
    async fn fork_palette_previews_and_creates_without_selecting_or_submitting_the_draft() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let broker = CapabilityBroker::new(root.path(), ApprovalPolicy::default()).unwrap();
        let manager = WorkspaceManager::new(broker);
        let runtime = RuntimeService::new(
            store.clone(),
            ProviderRegistry::default(),
            ToolRegistry::with_builtins(manager.clone()),
            manager,
        )
        .unwrap();
        let project_id = runtime.project_id();
        let source_session_id = runtime.session_id();
        let run_id = RunId::new_v4();
        for (role, text) in [
            (ConversationRole::User, "fork source"),
            (ConversationRole::Assistant, "settled answer"),
        ] {
            let assistant = role == ConversationRole::Assistant;
            store
                .append_conversation_message(&ConversationMessage {
                    id: argentum_domain::MessageId::new_v4(),
                    project_id,
                    session_id: source_session_id,
                    run_id,
                    role,
                    text: text.into(),
                    reasoning: String::new(),
                    usage: None,
                    profile_id: if assistant {
                        "fixture".into()
                    } else {
                        String::new()
                    },
                    model: if assistant {
                        "fixture".into()
                    } else {
                        String::new()
                    },
                    status: ConversationMessageStatus::Complete,
                    created_at: argentum_domain::now(),
                })
                .unwrap();
        }
        let mut host = CommandHost::from_runtime(runtime);
        let local = host.local_handle();
        let mut t = TuiState::new(root.path().into(), true);
        t.apply_event(AppEvent::WorkspaceStateLoaded(
            host.workspace_snapshot().unwrap(),
        ));
        t.apply_input(InputEvent::Text("draft stays here".into()));
        let before_target = t.submission_target();
        let before_count = host.workspace_snapshot().unwrap().sessions.len();

        open(&mut t, &local, menu::View::Fork).unwrap();
        let panel = rows(&t, 140, 30).unwrap().join("\n");
        assert!(panel.contains(argentum_domain::SESSION_FORK_WARNING));
        assert!(panel.contains("Create fork"));
        let action = state(&t).unwrap().entries[0].action.clone();
        assert!(matches!(action, Action::Fork { .. }));
        let mut tasks = JoinSet::new();
        dispatch(&mut t, &local, &mut tasks, action);
        settle(&mut t, &local, &mut tasks).await;

        let after = host.workspace_snapshot().unwrap();
        assert_eq!(after.sessions.len(), before_count + 1);
        assert_eq!(after.active_session_id, Some(source_session_id));
        assert_eq!(t.submission_target(), before_target);
        assert_eq!(t.composer, "draft stays here");
        assert!(state(&t)
            .unwrap()
            .notice
            .contains("Selection and drafts were not changed"));
        host.shutdown().await.unwrap();
    }
    #[tokio::test]
    async fn reliability_local_text_settings_do_not_change_coding_provider_and_keep_unready_draft()
    {
        let root = tempfile::tempdir().unwrap();
        let mut host = CommandHost::start_composed(crate::HostConfig::in_memory(root.path()))
            .await
            .unwrap();
        let local = host.local_handle();
        local
            .dispatch(AppCommand::AddProviderPreset {
                preset_id: "minimax".into(),
            })
            .await
            .unwrap();
        local
            .dispatch(AppCommand::SelectProviderProfile {
                provider_id: "minimax".into(),
            })
            .await
            .unwrap();
        let mut t = TuiState::new(root.path().into(), true);
        t.local_text_mode = true;
        t.apply_event(AppEvent::WorkspaceStateLoaded(
            host.workspace_snapshot().unwrap(),
        ));
        t.apply_input(InputEvent::Text("keep this".into()));
        open(&mut t, &local, menu::View::Model).unwrap();
        assert!(rows(&t, 120, 20)
            .unwrap()
            .join("\n")
            .contains("Provider: llama.cpp"));
        let mut tasks = JoinSet::new();
        dispatch(
            &mut t,
            &local,
            &mut tasks,
            Action::Provider("llama-cpp".into()),
        );
        assert!(tasks.is_empty());
        dispatch(
            &mut t,
            &local,
            &mut tasks,
            Action::Verifier(VerificationRunnerId::CargoFmtCheck),
        );
        assert!(tasks.is_empty());
        assert!(local
            .provider_profiles()
            .unwrap()
            .iter()
            .any(|p| p.id == "minimax" && p.selected));
        refresh_preflight(&mut t, &local).await.unwrap();
        t.overlay = None;
        assert_eq!(t.apply_input(InputEvent::Enter), TuiAction::Redraw);
        assert_eq!(t.composer, "keep this");
        host.shutdown().await.unwrap();
    }
    #[tokio::test]
    async fn reliability_skill_selection_is_visible_one_shot_and_settings_cancel_preserves_draft() {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join(".agents/skills/fixture");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("SKILL.md"),"---\nname: fixture\ndescription: Synthetic test selection\n---\nUse this synthetic test marker.\n").unwrap();
        let mut host = CommandHost::start_composed(crate::HostConfig::in_memory(root.path()))
            .await
            .unwrap();
        let local = host.local_handle();
        let mut t = TuiState::new(root.path().into(), true);
        t.apply_event(AppEvent::WorkspaceStateLoaded(
            host.workspace_snapshot().unwrap(),
        ));
        t.apply_input(InputEvent::Text("unsent".into()));
        open(&mut t, &local, menu::View::Skills).unwrap();
        let mut tasks = JoinSet::new();
        dispatch(
            &mut t,
            &local,
            &mut tasks,
            Action::UseSkill("fixture".into()),
        );
        assert!(screen::frame(&t, 80, 24)
            .rows
            .join("\n")
            .contains("Next message skill: fixture"));
        let TuiAction::Submit(submission) = t.apply_input(InputEvent::Enter) else {
            panic!("skill submission was not accepted")
        };
        assert_eq!(submission.skill.unwrap().name, "fixture");
        assert!(t.next_skill.is_none());
        t.apply_input(InputEvent::Text("retained".into()));
        open(&mut t, &local, menu::View::Setup).unwrap();
        let cancel = state(&t).unwrap().cancel.clone();
        t.apply_input(InputEvent::CtrlC);
        assert!(cancel.is_cancelled());
        assert_eq!(t.composer, "retained");
        assert!(t.overlay.is_none());
        host.shutdown().await.unwrap();
    }
    #[tokio::test]
    async fn reliability_masked_setup_key_save_cancel_and_input_focus_preserve_draft() {
        let root = tempfile::tempdir().unwrap();
        let vault = Arc::new(Vault::default());
        let config = crate::HostConfig::in_memory(root.path()).with_secret_store(vault.clone());
        let mut host = CommandHost::start_composed(config).await.unwrap();
        let local = host.local_handle();
        local
            .dispatch(AppCommand::AddProviderPreset {
                preset_id: "minimax".into(),
            })
            .await
            .unwrap();
        local
            .dispatch(AppCommand::SelectProviderProfile {
                provider_id: "minimax".into(),
            })
            .await
            .unwrap();
        local.seed_selected_model_budget().unwrap();
        let mut t = TuiState::new(root.path().into(), true);
        t.apply_event(AppEvent::WorkspaceStateLoaded(
            host.workspace_snapshot().unwrap(),
        ));
        t.apply_input(InputEvent::Text("untouched draft".into()));
        let mut tasks = JoinSet::new();
        open(&mut t, &local, menu::View::Setup).unwrap();
        dispatch(
            &mut t,
            &local,
            &mut tasks,
            Action::Edit(EditKind::Credential("minimax".into())),
        );
        let secret = "synthetic-ui-secret-not-a-real-key";
        t.apply_input(InputEvent::Paste(secret.into()));
        let frame = screen::frame(&t, 80, 24);
        assert!(frame.rows.join("\n").contains("****"));
        assert!(!frame.rows.join("\n").contains(secret));
        assert!(frame.cursor.1 < 20);
        let action = t.apply_input(InputEvent::Enter);
        dispatch_tui_action(&mut t, &local, &mut tasks, action);
        settle(&mut t, &local, &mut tasks).await;
        assert!(local.provider_credential_configured("minimax"));
        assert_eq!(vault.0.lock().unwrap().len(), 1);
        assert_eq!(t.composer, "untouched draft");
        assert!(!screen::frame(&t, 80, 24).rows.join("\n").contains(secret));
        dispatch(
            &mut t,
            &local,
            &mut tasks,
            Action::Edit(EditKind::Credential("minimax".into())),
        );
        t.apply_input(InputEvent::Text("cancelled-value".into()));
        t.apply_input(InputEvent::Escape);
        assert_eq!(
            vault.0.lock().unwrap().values().next().unwrap().expose(),
            secret
        );
        assert_eq!(t.composer, "untouched draft");
        host.shutdown().await.unwrap();
    }
    #[tokio::test]
    async fn reliability_tool_toggle_requires_confirmation_and_rejects_pasted_confirmation() {
        let root = tempfile::tempdir().unwrap();
        let mut host = CommandHost::start_composed(crate::HostConfig::in_memory(root.path()))
            .await
            .unwrap();
        let local = host.local_handle();
        let mut t = TuiState::new(root.path().into(), true);
        t.apply_event(AppEvent::WorkspaceStateLoaded(
            host.workspace_snapshot().unwrap(),
        ));
        let mut tasks = JoinSet::new();
        open(&mut t, &local, menu::View::Tools).unwrap();
        dispatch(
            &mut t,
            &local,
            &mut tasks,
            Action::Tool("write_text".into()),
        );
        let action = t.apply_input(InputEvent::Enter);
        dispatch_tui_action(&mut t, &local, &mut tasks, action);
        assert!(state(&t).unwrap().confirm.is_some());
        assert_eq!(
            t.apply_input(InputEvent::ConfirmAccess(false)),
            TuiAction::Redraw
        );
        assert!(host.execution_policy().unwrap().capability_enabled["tool.write-text"]);
        let action = t.apply_input(InputEvent::ConfirmAccess(true));
        dispatch_tui_action(&mut t, &local, &mut tasks, action);
        settle(&mut t, &local, &mut tasks).await;
        assert!(!host.execution_policy().unwrap().capability_enabled["tool.write-text"]);
        host.shutdown().await.unwrap();
    }

    #[tokio::test]
    async fn readonly_delegation_is_visible_and_requires_physical_owner_confirmation() {
        let root = tempfile::tempdir().unwrap();
        let mut host = CommandHost::start_composed(crate::HostConfig::in_memory(root.path()))
            .await
            .unwrap();
        let local = host.local_handle();
        let mut t = TuiState::new(root.path().into(), true);
        t.apply_event(AppEvent::WorkspaceStateLoaded(
            host.workspace_snapshot().unwrap(),
        ));
        let mut tasks = JoinSet::new();

        open(&mut t, &local, menu::View::Tools).unwrap();
        assert!(state(&t).unwrap().entries.iter().any(|entry| {
            entry.label == "Read-only delegated investigation [agent, disabled]"
                && matches!(
                    &entry.action,
                    Action::Capability(id) if id == "model.delegate-readonly"
                )
        }));
        dispatch(
            &mut t,
            &local,
            &mut tasks,
            Action::Capability("model.delegate-readonly".into()),
        );
        let action = t.apply_input(InputEvent::Enter);
        dispatch_tui_action(&mut t, &local, &mut tasks, action);
        assert!(state(&t).unwrap().confirm.is_some());
        assert_eq!(
            t.apply_input(InputEvent::ConfirmAccess(false)),
            TuiAction::Redraw
        );
        assert!(!host.execution_policy().unwrap().capability_enabled["model.delegate-readonly"]);

        let action = t.apply_input(InputEvent::ConfirmAccess(true));
        dispatch_tui_action(&mut t, &local, &mut tasks, action);
        settle(&mut t, &local, &mut tasks).await;
        assert!(host.execution_policy().unwrap().capability_enabled["model.delegate-readonly"]);
        host.shutdown().await.unwrap();
    }
}
