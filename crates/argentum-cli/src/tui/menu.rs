use super::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Help,
    Commands,
    Status,
    Model,
    Providers,
    Permissions,
    Tools,
    Verification,
    Skills,
    Context,
    Sessions,
    Fork,
    Setup,
    AccessReview,
    Changes,
    OwnerConfirm,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Request {
    Open(View),
    ReviewAccess(String),
    CommitAccess,
    Review,
    Settings(super::settings::Action),
}
pub(super) struct Command {
    pub name: &'static str,
    description: &'static str,
    group: &'static str,
    pub view: Option<View>,
    local: bool,
}
macro_rules! cmd {
    ($n:literal,$d:literal,$g:literal,$v:expr,$l:expr) => {
        Command {
            name: $n,
            description: $d,
            group: $g,
            view: $v,
            local: $l,
        }
    };
}
pub(super) const COMMANDS: &[Command] = &[
    cmd!(
        "/help",
        "F1/Ctrl+P: commands. PgUp/PgDn/wheel: history. Ctrl+End: latest. F2: access. Arrows/Home/End: composer; Shift+Enter: newline.",
        "conversation",
        Some(View::Help),
        true
    ),
    cmd!(
        "/sessions",
        "List retained sessions; /resume ID switches",
        "conversation",
        Some(View::Sessions),
        false
    ),
    cmd!(
        "/new",
        "New conversation; draft kept per session",
        "conversation",
        None,
        false
    ),
    cmd!(
        "/fork",
        "Fork the current conversation at its latest settled boundary",
        "conversation",
        Some(View::Fork),
        false
    ),
    cmd!(
        "/resume",
        "Select session by ID",
        "conversation",
        Some(View::Sessions),
        false
    ),
    cmd!(
        "/queue",
        "Inspect; continue/discard explicitly",
        "conversation",
        None,
        false
    ),
    cmd!(
        "/stop",
        "Cancel active work and drain it",
        "conversation",
        None,
        true
    ),
    cmd!(
        "/steer",
        "Apply the current draft to the active run at its next safe request boundary",
        "conversation",
        None,
        false
    ),
    cmd!(
        "/retry",
        "Submit the previous prompt as a new task",
        "conversation",
        None,
        false
    ),
    cmd!("/quit", "Exit chat", "conversation", None, true),
    cmd!(
        "/model",
        "Choose a model, refresh its catalog or activate local inference",
        "model/provider",
        Some(View::Model),
        true
    ),
    cmd!(
        "/provider",
        "Choose and configure saved providers",
        "model/provider",
        Some(View::Providers),
        true
    ),
    cmd!(
        "/setup",
        "Configure provider, model, key, context and access",
        "model/provider",
        Some(View::Setup),
        true
    ),
    cmd!(
        "/permissions",
        "Project access; Advanced includes Full Access",
        "access",
        Some(View::Permissions),
        true
    ),
    cmd!("/models", "Choose a model", "model/provider", Some(View::Model), true),
    cmd!("/providers", "Choose a provider", "model/provider", Some(View::Providers), true),
    cmd!(
        "/access",
        "Alias of /permissions",
        "access",
        Some(View::Permissions),
        true
    ),
    cmd!(
        "/tools",
        "Registration, backend, policy, approval rules",
        "tools/verification",
        Some(View::Tools),
        true
    ),
    cmd!(
        "/verification",
        "Fixed verifiers and exact owner authorization",
        "tools/verification",
        Some(View::Verification),
        true
    ),
    cmd!("/changes", "Recorded changes, owner restore review and verification", "tools/verification", Some(View::Changes), true),
    cmd!(
        "/skills",
        "Inspect local skill metadata",
        "tools/verification",
        Some(View::Skills),
        false
    ),
    cmd!(
        "/context",
        "Selected context, separate from scrollback",
        "diagnostics",
        Some(View::Context),
        true
    ),
    cmd!(
        "/compact",
        "Request bounded compaction",
        "diagnostics",
        None,
        false
    ),
    cmd!(
        "/status",
        "Authoritative runtime facts; no inference",
        "diagnostics",
        Some(View::Status),
        true
    ),
    cmd!(
        "/about",
        "Application, model, mode and access",
        "diagnostics",
        Some(View::Status),
        true
    ),
    cmd!(
        "/goal-run",
        "Submit explicit goal ID and task",
        "conversation",
        None,
        false
    ),
];

pub(super) struct Overlay {
    pub(super) settings: Option<super::settings::State>,
    pub(super) review: Option<super::review::State>,
    pub view: View,
    selected: usize,
    offset: usize,
    query: String,
    from_slash: bool,
    advanced: bool,
    text: String,
    profiles: Vec<argentum_domain::HarnessExecutionProfileSummary>,
}
impl Overlay {
    pub(super) fn new(view: View) -> Self {
        Self {
            settings: None,
            review: None,
            view,
            selected: 0,
            offset: 0,
            query: String::new(),
            from_slash: false,
            advanced: false,
            text: String::new(),
            profiles: Vec::new(),
        }
    }
}
impl TuiState {
    fn candidates(&self) -> Vec<&'static Command> {
        let query = self
            .overlay
            .as_ref()
            .map(|o| {
                o.query
                    .trim_start_matches('/')
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
            })
            .unwrap_or("");
        COMMANDS
            .iter()
            .filter(|c| c.name.trim_start_matches('/').starts_with(query))
            .collect()
    }
    pub(super) fn command_unavailable(&self, c: &Command) -> Option<&str> {
        if self.local_text_mode && !c.local {
            Some("unavailable in memory-only local text chat")
        } else if matches!(c.name, "/compact" | "/goal-run")
            && self.coding_preflight_issue.is_some()
        {
            self.coding_preflight_issue.as_deref()
        } else if matches!(c.name, "/new" | "/compact") && self.active_run_id.is_some() {
            Some("active work must settle first; use /stop")
        } else if c.name == "/steer" && self.active_run_id.is_none() {
            Some("No active ordinary run is eligible; draft is preserved")
        } else {
            None
        }
    }
    pub(super) fn open_commands(&mut self, from_slash: bool) {
        let mut o = Overlay::new(View::Commands);
        o.from_slash = from_slash;
        if from_slash {
            o.query = "/".into();
        }
        self.overlay = Some(o);
    }
    fn profiles(&self) -> Vec<&argentum_domain::HarnessExecutionProfileSummary> {
        self.overlay
            .as_ref()
            .map(|o| {
                o.profiles
                    .iter()
                    .filter(|p| {
                        p.selectable
                            && (o.advanced
                                || matches!(p.id.as_str(), "observe" | "guided" | "verification"))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
    pub(super) fn overlay_rows(&self, width: usize, height: usize) -> Option<Vec<String>> {
        if self.overlay.as_ref().is_some_and(|o| o.settings.is_some()) {
            return super::settings::rows(self, width, height);
        }
        if self
            .overlay
            .as_ref()
            .is_some_and(|o| o.view == View::Changes)
        {
            return super::review::rows(self, width, height);
        }
        let o = self.overlay.as_ref()?;
        let mut source = vec![format!("{:?} | Esc closes; draft preserved", o.view)];
        match o.view {
            View::Commands | View::Help => {
                source.push(format!(
                    "Filter: {} | arrows select, Enter opens, Tab completes",
                    o.query
                ));
                let list = self.candidates();
                if list.is_empty() {
                    source.push("Unknown command. No model task is submitted.".into());
                }
                let visible = height.saturating_sub(4).max(1);
                for (i, c) in list
                    .iter()
                    .enumerate()
                    .skip(o.selected.saturating_sub(visible - 1))
                    .take(visible)
                {
                    source.push(
                        screen::wrap(
                            &format!(
                                "{} {:18} [{}]",
                                if i == o.selected { ">" } else { " " },
                                c.name,
                                c.group,
                            ),
                            width,
                        )
                        .remove(0),
                    );
                }
                if let Some(c) = list.get(o.selected) {
                    source.push(format!(
                        "{}{}",
                        c.description,
                        self.command_unavailable(c)
                            .map_or(String::new(), |r| format!(" - {r}"))
                    ));
                }
            }
            View::Permissions => {
                source.push(format!(
                    "Current: {} | Tab: {} | Enter: review",
                    self.permission_mode,
                    if o.advanced { "Common" } else { "Advanced" }
                ));
                let profiles = self.profiles();
                let visible = height.saturating_sub(5).max(1);
                for (i, p) in profiles
                    .iter()
                    .enumerate()
                    .skip(o.selected.saturating_sub(visible - 1))
                    .take(visible)
                {
                    source.push(
                        screen::wrap(
                            &format!(
                                "{} {}{}",
                                if i == o.selected { ">" } else { " " },
                                p.label,
                                if p.selected { " [current]" } else { "" },
                            ),
                            width,
                        )
                        .remove(0),
                    );
                }
                if let Some(p) = profiles.get(o.selected) {
                    source.push(p.detail.clone());
                }
                source.push("Project scope. Selecting access does not resume work or add missing model tools.".into());
            }
            _ => source.extend(o.text.lines().map(str::to_owned)),
        }
        let mut rows: Vec<_> = source
            .into_iter()
            .flat_map(|s| screen::wrap(&s, width))
            .skip(o.offset)
            .take(height)
            .collect();
        rows.resize(height, String::new());
        Some(rows)
    }
    pub(super) fn overlay_input(&mut self, input: InputEvent) -> TuiAction {
        if self.overlay.as_ref().is_some_and(|o| o.settings.is_some()) {
            return super::settings::input(self, input);
        }
        if self
            .overlay
            .as_ref()
            .is_some_and(|o| o.view == View::Changes)
        {
            return super::review::input(self, input);
        }
        if let InputEvent::Resize(width, _) = input {
            self.terminal_width = width.max(1);
            return TuiAction::Redraw;
        }
        if input == InputEvent::Escape {
            self.overlay = None;
            self.access_review = None;
            return TuiAction::Redraw;
        }
        if input == InputEvent::CtrlC {
            self.overlay = None;
            self.access_review = None;
            return self.handle_ctrl_c();
        }
        if input == InputEvent::Eof {
            return TuiAction::Exit;
        }
        match self.overlay.as_ref().unwrap().view {
            View::Commands | View::Help => {
                if let InputEvent::Text(text) = &input {
                    let o = self.overlay.as_mut().unwrap();
                    if o.query.len() + text.len() <= 256 {
                        o.query.push_str(text);
                        o.selected = 0;
                    }
                } else if input == InputEvent::Backspace {
                    let o = self.overlay.as_mut().unwrap();
                    o.query.pop();
                    o.selected = 0;
                } else if input == InputEvent::HistoryPrevious {
                    let o = self.overlay.as_mut().unwrap();
                    o.selected = o.selected.saturating_sub(1);
                } else if input == InputEvent::HistoryNext {
                    let n = self.candidates().len();
                    let o = self.overlay.as_mut().unwrap();
                    o.selected = (o.selected + 1).min(n.saturating_sub(1));
                } else if input == InputEvent::Complete {
                    if let Some(c) = self
                        .candidates()
                        .get(self.overlay.as_ref().unwrap().selected)
                    {
                        self.overlay.as_mut().unwrap().query = c.name.into();
                    }
                } else if input == InputEvent::Enter {
                    let o = self.overlay.as_ref().unwrap();
                    if o.from_slash && o.query.contains(char::is_whitespace) {
                        let command = o.query.clone();
                        self.overlay = None;
                        if command
                            .split_whitespace()
                            .next()
                            .is_some_and(|name| name.eq_ignore_ascii_case("/steer"))
                        {
                            self.composer = command.clone();
                            self.composer_cursor = self.composer.len();
                        } else {
                            self.clear_composer();
                        }
                        return self.handle_slash_command(&command);
                    }
                    if let Some(c) = self.candidates().get(o.selected).copied() {
                        if let Some(reason) = self.command_unavailable(c) {
                            self.status = reason.into();
                            return TuiAction::Redraw;
                        }
                        let consumed = o.from_slash;
                        self.overlay = None;
                        if c.name == "/steer" && !consumed {
                            return self.submit_steering(self.composer.clone());
                        }
                        if consumed {
                            self.clear_composer();
                        }
                        return c.view.map_or_else(
                            || self.handle_slash_command(c.name),
                            |v| TuiAction::Ui(Request::Open(v)),
                        );
                    }
                }
            }
            View::Permissions => {
                if input == InputEvent::Complete {
                    let o = self.overlay.as_mut().unwrap();
                    o.advanced = !o.advanced;
                    o.selected = 0;
                } else if input == InputEvent::HistoryPrevious {
                    let o = self.overlay.as_mut().unwrap();
                    o.selected = o.selected.saturating_sub(1);
                } else if input == InputEvent::HistoryNext {
                    let n = self.profiles().len();
                    let o = self.overlay.as_mut().unwrap();
                    o.selected = (o.selected + 1).min(n.saturating_sub(1));
                } else if input == InputEvent::Enter {
                    if let Some(p) = self.profiles().get(self.overlay.as_ref().unwrap().selected) {
                        return TuiAction::Ui(Request::ReviewAccess(p.id.clone()));
                    }
                }
            }
            View::AccessReview => {
                if let InputEvent::Scroll(n) = input {
                    let o = self.overlay.as_mut().unwrap();
                    o.offset = (o.offset as i32 + n).clamp(0, 4096) as usize;
                } else if input == InputEvent::HistoryPrevious {
                    let o = self.overlay.as_mut().unwrap();
                    o.offset = o.offset.saturating_sub(1);
                } else if input == InputEvent::HistoryNext {
                    let o = self.overlay.as_mut().unwrap();
                    o.offset = (o.offset + 1).min(4096);
                }
                if let InputEvent::ConfirmAccess(physical) = input {
                    if physical {
                        return TuiAction::Ui(Request::CommitAccess);
                    }
                    self.status="Hold F8 on the physical keyboard to confirm. Pasted/PTY text is not authority.".into();
                }
            }
            _ => {
                let o = self.overlay.as_mut().unwrap();
                match input {
                    InputEvent::HistoryPrevious => o.offset = o.offset.saturating_sub(1),
                    InputEvent::HistoryNext => o.offset = (o.offset + 1).min(4096),
                    InputEvent::Scroll(n) => {
                        o.offset = (o.offset as i32 + n).clamp(0, 4096) as usize
                    }
                    _ => {}
                }
            }
        }
        TuiAction::Redraw
    }
    pub(super) fn info_overlay(&mut self, view: View, text: String) {
        let mut o = Overlay::new(view);
        o.text = text.chars().take(16 * 1024).collect();
        self.overlay = Some(o);
    }
}

pub(super) fn open_view(
    state: &mut TuiState,
    local: &LocalCommandHandle,
    view: View,
) -> Result<(), HostError> {
    if super::settings::supports(view) {
        return super::settings::open(state, local, view);
    }
    if view == View::Changes {
        super::review::begin(state);
        return super::review::load_sync(state, local);
    }
    if matches!(view, View::Commands | View::Help) {
        state.open_commands(false);
        return Ok(());
    }
    let facts = local.runtime_facts(state.local_text_mode)?;
    state.permission_mode = local
        .harness_snapshot()?
        .execution_profiles
        .into_iter()
        .find(|p| p.selected)
        .map_or_else(|| "custom".into(), |p| p.label);
    match view{
        View::Permissions=>{let mut o=Overlay::new(view);o.profiles=local.harness_snapshot()?.execution_profiles;state.overlay=Some(o);}
        View::Tools=>{
            let h=local.harness_snapshot()?;let backend=facts["coding_available"].as_bool().unwrap_or(false);
            let mut rows=vec!["Host inspection is not a model tool. Exact actions are checked again at execution.".into()];
            for t in local.tool_descriptors(){let p=h.capabilities.iter().find(|c|c.id==t.capability_id);let exposed=facts["model_tools"].as_array().is_some_and(|tools|tools.iter().any(|name|name.as_str()==Some(t.id.as_str())));rows.push(format!("{}: registered; coding_backend={}; policy_enabled={}; in_model_catalog={}; descriptor_approval={}; availability={:?}",t.id,backend,p.is_some_and(|c|c.enabled),exposed,t.requires_approval,p.map(|c|c.availability)));}
            rows.push("Descriptor approval is refined by the profile, exact grants and runtime checks. Processes outside Full Access need approval; project verifiers need their exact binding.".into());state.info_overlay(view,rows.join("\n"));
        }
        View::Providers=>state.info_overlay(view,local.provider_profiles()?.iter().map(|p|format!("{} [{}] {:?}: {}{}",p.id,p.label,p.kind,p.model,if p.selected{" (selected)"}else{""})).collect::<Vec<_>>().join("\n")),
        View::Skills=>{let catalog=local.skill_catalog()?;state.info_overlay(view,format!("Catalog complete: {}\n{}",catalog.complete,catalog.entries.iter().map(|s|format!("{} [{}]: {}",s.name.as_deref().unwrap_or("unnamed"),if s.valid{"available"}else{"invalid"},s.description.as_deref().or(s.error.as_deref()).unwrap_or(""))).collect::<Vec<_>>().join("\n")));},
        View::Sessions=>{let s=local.workspace_snapshot()?;state.info_overlay(view,s.sessions.iter().map(|s|format!("{} {}",s.id,s.title)).collect::<Vec<_>>().join("\n")+"\nUse /resume SESSION_ID. Drafts are kept per session.");}
        View::Context if !state.local_text_mode=>{if let Some(id)=state.active_session_id{state.info_overlay(view,serde_json::to_string_pretty(&local.context_preview(id)?).unwrap_or_default());}}
        View::Verification=>state.info_overlay(view,local.verification_summary()?),
        View::Setup=>state.info_overlay(view,"Change access here with F2. For provider setup, finish work, /quit, then run argentum setup for this workspace. Access and inspection do not require onboarding.".into()),
        _=>state.info_overlay(view,serde_json::to_string_pretty(&facts).unwrap_or_default()),
    }
    Ok(())
}
pub(super) fn review_access(
    state: &mut TuiState,
    local: &LocalCommandHandle,
    id: &str,
) -> Result<(), HostError> {
    let mut review = local.access_review(id)?;
    review.review.busy |= state.active_run_id.is_some()
        || state.pending_approval.is_some()
        || state.session_transition;
    let mut text = format!(
        "Project-scoped access\nCurrent: {} {:?} {:?}\nProposed: {} {:?} {:?}\n",
        review.review.current.profile_id,
        review.review.current.sandbox_scope,
        review.review.current.approval_mode,
        review.review.proposed.profile_id,
        review.review.proposed.sandbox_scope,
        review.review.proposed.approval_mode
    );
    for (label, cap) in [
        ("Reads", "tool.read-text"),
        ("Edits", "tool.edit-text"),
        ("Writes", "tool.write-text"),
        ("Processes", "execution.argv"),
        ("Shell", "execution.shell"),
        ("Fixed verification", "verification.runner"),
    ] {
        text.push_str(&format!(
            "{label}: {} -> {}\n",
            review
                .review
                .current
                .capability_enabled
                .get(cap)
                .copied()
                .unwrap_or(false),
            review
                .review
                .proposed
                .capability_enabled
                .get(cap)
                .copied()
                .unwrap_or(false)
        ));
    }
    text.push_str("Processes remain approval-gated outside Full Access. Project-defined verifiers still require exact owner authorization. Full Access is not OS elevation or arbitrary filesystem/network isolation. Provider transport is separate from agent network tools.\n");
    if state.local_text_mode {
        text.push_str("This model stays text-only with no tools under every project profile.\n");
    }
    if review.review.busy {
        text.push_str("Active work/approval blocks changes. Ctrl+C cancels and drains; reopen access afterward. Nothing is auto-approved or queued.\n");
    } else {
        text.push_str(
            "Hold F8 on the physical keyboard to commit. Esc cancels. Follow-ups remain paused.\n",
        );
    }
    state.access_review = Some(review);
    state.info_overlay(View::AccessReview, text);
    Ok(())
}
