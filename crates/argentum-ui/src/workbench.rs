//! Projects module, theme, and panel state into the shell, and connects the
//! callbacks that only the workbench uses. This renders domain state and dispatches
//! commands. Provider, Git, and network work all happens behind the command host.

use std::cell::RefCell;
use std::rc::Rc;

use argentum_domain::{
    AppCommand, BrowserBlockKind, BrowserPage, BrowserPageState, GitDiffSnapshot,
    GitStatusSnapshot, GitViewState, HarnessSnapshot, HarnessSurfaceState, ProviderUsageRow,
    SurfaceId, UiChangeSource, UiColor, UiDensity, UiPanelSide, UiPatch, UiPreferences,
    UI_FONT_PERCENT_RANGE, UI_PRESETS, UI_RADIUS_PERCENT_RANGE,
};
use slint::{ComponentHandle, Model, ModelRc, SharedString, VecModel};

use crate::markdown::make_block;
use crate::{
    App, DiffLineView, GitFileView, MainWindow, Mods, PaletteItemView, ProviderUsageView,
    RichBlockView, Theme, ThemePresetView,
};

const MAX_DIFF_LINES: usize = 4000;

/// Folder names are usually lowercase, but the shell uses this as a proper name.
/// A name that already has its own capitals, such as `iOS`, is left alone.
pub fn display_project_name(name: &str) -> String {
    if name.chars().any(char::is_uppercase) {
        return name.to_owned();
    }
    let mut characters = name.chars();
    match characters.next() {
        Some(first) => first.to_uppercase().chain(characters).collect(),
        None => String::new(),
    }
}

/// Windows canonicalization adds an extended-length prefix nobody needs to read.
pub fn display_path(path: &std::path::Path) -> String {
    let raw = path.display().to_string();
    if let Some(rest) = raw.strip_prefix("\\\\?\\UNC\\") {
        format!("\\\\{rest}")
    } else if let Some(rest) = raw.strip_prefix("\\\\?\\") {
        rest.to_owned()
    } else {
        raw
    }
}

fn to_color(color: UiColor) -> slint::Color {
    slint::Color::from_argb_u8(color.a, color.r, color.g, color.b)
}

pub(crate) fn surface_icon(surface: SurfaceId) -> &'static str {
    match surface {
        SurfaceId::Conversation => "message-circle",
        SurfaceId::Plan => "plan",
        SurfaceId::Changes => "diff",
        SurfaceId::Files => "folder",
        SurfaceId::Terminal => "terminal",
        SurfaceId::Preview => "maximize",
        SurfaceId::Activity => "activity",
        SurfaceId::Trajectory => "workflow",
        SurfaceId::Approvals => "approval",
        SurfaceId::Sessions => "sessions",
        SurfaceId::GitDiff => "trajectory",
        SurfaceId::Browser => "globe",
        SurfaceId::Usage => "database",
        SurfaceId::ContextMeter => "scan-text",
        SurfaceId::Reasoning => "sparkles",
        SurfaceId::Goal => "map",
        SurfaceId::StatusBar => "info",
        SurfaceId::CommandPalette => "command",
        SurfaceId::Requests => "model",
    }
}

pub(crate) fn surface_group(surface: SurfaceId) -> &'static str {
    match surface {
        SurfaceId::Conversation | SurfaceId::Approvals => "core",
        _ if surface.is_inspector() => "panel",
        _ => "shell",
    }
}

/// Apply appearance to the live window. Called on load and after every change, so
/// a change from the agent takes effect at once.
pub fn apply_preferences(window: &MainWindow, preferences: &UiPreferences) {
    let theme = window.global::<Theme>();
    for (token, color) in preferences.resolved_colors() {
        let value = to_color(color);
        match token {
            "canvas" => theme.set_canvas(value),
            "sidebar" => theme.set_sidebar(value),
            "surface" => theme.set_surface(value),
            "raised" => theme.set_raised(value),
            "overlay" => theme.set_overlay(value),
            "code-bg" => theme.set_code_bg(value),
            "hover" => theme.set_hover(value),
            "pressed" => theme.set_pressed(value),
            "border" => theme.set_border(value),
            "border-strong" => theme.set_border_strong(value),
            "text" => theme.set_text(value),
            "text-2" => theme.set_text_2(value),
            "text-3" => theme.set_text_3(value),
            "silver" => theme.set_silver(value),
            "accent" => {
                theme.set_accent(value);
                theme.set_accent_soft(value.with_alpha(0.14));
            }
            "accent-hover" => theme.set_accent_hover(value),
            "accent-text" => theme.set_accent_text(value),
            "accent-ink" => theme.set_accent_ink(value),
            "ok" => theme.set_ok(value),
            "warn" => theme.set_warn(value),
            "danger" => theme.set_danger(value),
            "diff-add-text" => theme.set_diff_add_text(value),
            "diff-add-bg" => theme.set_diff_add_bg(value),
            "diff-del-text" => theme.set_diff_del_text(value),
            "diff-del-bg" => theme.set_diff_del_bg(value),
            _ => {}
        }
    }
    theme.set_font_scale(f32::from(preferences.font_percent) / 100.0);
    theme.set_radius_scale(f32::from(preferences.radius_percent) / 100.0);
    theme.set_density(preferences.density.scale());
    theme.set_font_ui(preferences.font_ui.clone().into());
    theme.set_font_mono(preferences.font_mono.clone().into());
    theme.set_reduced_motion(preferences.reduced_motion);
    theme.set_sidebar_width(f32::from(preferences.sidebar_width));
    theme.set_panel_width(f32::from(preferences.panel_width));
    theme.set_chat_width(f32::from(preferences.chat_width));
    theme.set_panel_left(preferences.panel_side == UiPanelSide::Left);
    theme.set_user_bubbles(preferences.user_bubbles);
    window.set_reduced_motion(preferences.reduced_motion);

    window.set_theme_preset_id(preferences.preset.clone().into());
    window.set_ui_density(preferences.density.id().into());
    window.set_ui_font_percent(i32::from(preferences.font_percent));
    window.set_ui_radius_percent(i32::from(preferences.radius_percent));
    window.set_ui_error(SharedString::default());
    window.set_theme_presets(theme_preset_model(&preferences.preset));
}

fn theme_preset_model(selected: &str) -> ModelRc<ThemePresetView> {
    let rows = UI_PRESETS
        .iter()
        .map(|preset| {
            let preview = UiPreferences {
                preset: preset.id.to_owned(),
                ..UiPreferences::default()
            }
            .resolved_colors();
            let color = |token: &str| {
                preview
                    .get(token)
                    .copied()
                    .map_or(slint::Color::default(), to_color)
            };
            ThemePresetView {
                id: preset.id.into(),
                label: preset.label.into(),
                selected: preset.id == selected,
                canvas: color("canvas"),
                surface: color("surface"),
                accent: color("accent"),
                text: color("text"),
            }
        })
        .collect::<Vec<_>>();
    ModelRc::new(VecModel::from(rows))
}

pub fn apply_harness_modules(window: &MainWindow, snapshot: &HarnessSnapshot) {
    let on = |surface: SurfaceId| {
        snapshot
            .surfaces
            .iter()
            .any(|state: &HarnessSurfaceState| state.id == surface && state.visible)
    };
    let mods = window.global::<Mods>();
    mods.set_sessions(on(SurfaceId::Sessions));
    mods.set_context_meter(on(SurfaceId::ContextMeter));
    mods.set_reasoning(on(SurfaceId::Reasoning));
    mods.set_status_bar(on(SurfaceId::StatusBar));
    mods.set_palette(on(SurfaceId::CommandPalette));
    mods.set_goal(on(SurfaceId::Goal));
    mods.set_usage(on(SurfaceId::Usage));
    mods.set_any_panel(
        snapshot
            .surfaces
            .iter()
            .any(|state| state.id.is_inspector() && state.visible),
    );
}

pub fn parse_diff(text: &str) -> (Vec<DiffLineView>, bool) {
    let mut lines = Vec::new();
    let (mut old_line, mut new_line) = (0u32, 0u32);
    let mut truncated = false;
    for raw in text.lines() {
        if lines.len() >= MAX_DIFF_LINES {
            truncated = true;
            break;
        }
        let line = |kind: &str, old_no: String, new_no: String, text: &str| DiffLineView {
            kind: kind.into(),
            old_no: old_no.into(),
            new_no: new_no.into(),
            text: text.into(),
        };
        if let Some(rest) = raw.strip_prefix("@@") {
            // @@ -a,b +c,d @@ optional heading
            let mut parts = rest.split_whitespace();
            let start = |part: Option<&str>| {
                part.and_then(|value| value[1..].split(',').next()?.parse::<u32>().ok())
                    .unwrap_or(0)
            };
            old_line = start(parts.next());
            new_line = start(parts.next());
            lines.push(line("hunk", String::new(), String::new(), raw));
        } else if raw.starts_with("diff --git")
            || raw.starts_with("index ")
            || raw.starts_with("--- ")
            || raw.starts_with("+++ ")
        {
            // The panel already names the file, so Git's own header adds nothing.
        } else if raw.starts_with("new file")
            || raw.starts_with("deleted file")
            || raw.starts_with("old mode")
            || raw.starts_with("new mode")
            || raw.starts_with("similarity")
            || raw.starts_with("rename ")
            || raw.starts_with("Binary files")
        {
            lines.push(line("meta", String::new(), String::new(), raw));
        } else if let Some(rest) = raw.strip_prefix('+') {
            lines.push(line("add", String::new(), new_line.to_string(), rest));
            new_line += 1;
        } else if let Some(rest) = raw.strip_prefix('-') {
            lines.push(line("del", old_line.to_string(), String::new(), rest));
            old_line += 1;
        } else if raw.starts_with('\\') {
            lines.push(line("meta", String::new(), String::new(), raw));
        } else {
            let rest = raw.strip_prefix(' ').unwrap_or(raw);
            lines.push(line(
                "ctx",
                old_line.to_string(),
                new_line.to_string(),
                rest,
            ));
            old_line += 1;
            new_line += 1;
        }
    }
    (lines, truncated)
}

pub fn apply_git_status(window: &MainWindow, snapshot: &GitStatusSnapshot) {
    window.set_git_state(
        match snapshot.state {
            GitViewState::Ready => "ready",
            GitViewState::Blocked => "blocked",
            GitViewState::NotRepository => "not-repo",
            GitViewState::Failed => "error",
        }
        .into(),
    );
    window.set_git_message(snapshot.message.clone().into());
    window.set_git_branch(snapshot.branch.clone().into());
    let selected = window.get_git_selected().to_string();
    let still_listed = snapshot.files.iter().any(|file| file.path == selected);
    if !still_listed {
        window.set_git_selected(SharedString::default());
        window.set_git_diff_lines(ModelRc::default());
    }
    let rows = snapshot
        .files
        .iter()
        .map(|file| GitFileView {
            path: file.path.clone().into(),
            status: file.status.clone().into(),
            selected: file.path == selected,
        })
        .collect::<Vec<_>>();
    window.set_git_files(ModelRc::new(VecModel::from(rows)));
}

pub fn apply_git_diff(window: &MainWindow, snapshot: &GitDiffSnapshot) {
    if window.get_git_selected().as_str() != snapshot.path {
        return;
    }
    if snapshot.state != GitViewState::Ready {
        window.set_git_diff_lines(ModelRc::new(VecModel::from(vec![DiffLineView {
            kind: "meta".into(),
            old_no: SharedString::default(),
            new_no: SharedString::default(),
            text: snapshot.message.clone().into(),
        }])));
        window.set_git_diff_truncated(false);
        return;
    }
    let (lines, truncated) = parse_diff(&snapshot.diff);
    window.set_git_diff_truncated(truncated || snapshot.truncated);
    window.set_git_diff_lines(ModelRc::new(VecModel::from(lines)));
}

fn browser_block_view(block: &argentum_domain::BrowserBlock) -> RichBlockView {
    let kind = match block.kind {
        BrowserBlockKind::Heading1 => "h1",
        BrowserBlockKind::Heading2 => "h2",
        BrowserBlockKind::Heading3 => "h3",
        BrowserBlockKind::Paragraph => "p",
        BrowserBlockKind::ListItem => "li",
        BrowserBlockKind::Quote => "quote",
        BrowserBlockKind::Code => "code",
        BrowserBlockKind::Rule => "hr",
    };
    let marker = if kind == "li" { "\u{2022}" } else { "" };
    let markup = has_inline_markup(&block.markdown).then_some(block.markdown.as_str());
    make_block(kind, block.text.clone(), markup, "", 0, marker)
}

/// True when the text has formatting beyond escaped literals.
fn has_inline_markup(markdown: &str) -> bool {
    let mut escaped = false;
    for character in markdown.chars() {
        if escaped {
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if matches!(character, '*' | '`' | '[') {
            return true;
        }
    }
    false
}

pub fn apply_browser_page(window: &MainWindow, page: &BrowserPage) {
    window.set_browser_url(page.url.clone().into());
    window.set_browser_title(page.title.clone().into());
    window.set_browser_message(page.message.clone().into());
    window.set_browser_truncated(page.truncated);
    window.set_browser_state(
        match page.state {
            BrowserPageState::Ready => "ready",
            BrowserPageState::Blocked => "blocked",
            BrowserPageState::Failed => "error",
        }
        .into(),
    );
    let rows = page
        .blocks
        .iter()
        .map(browser_block_view)
        .collect::<Vec<_>>();
    window.set_browser_blocks(ModelRc::new(VecModel::from(rows)));
}

/// Release notes are untrusted text, so they go through the Markdown block model.
pub fn set_update_notes(window: &MainWindow, notes: &str) {
    window.set_update_notes(crate::markdown::model(notes));
}

fn compact_count(value: u64) -> String {
    match value {
        0..=999 => value.to_string(),
        1_000..=999_999 => format!("{:.1}k", value as f64 / 1_000.0),
        _ => format!("{:.2}M", value as f64 / 1_000_000.0),
    }
}

pub fn apply_provider_usage(window: &MainWindow, rows: &[ProviderUsageRow]) {
    let profiles = window.get_provider_profiles();
    let label_for = |id: &str| {
        (0..profiles.row_count())
            .filter_map(|index| profiles.row_data(index))
            .find(|profile| profile.id.as_str() == id)
    };
    let total = rows.iter().map(|row| row.total_tokens).sum::<u64>();
    let selected = window.get_provider_id();
    let views = rows
        .iter()
        .map(|row| {
            let profile = label_for(&row.profile_id);
            ProviderUsageView {
                id: row.profile_id.clone().into(),
                label: profile.as_ref().map_or_else(
                    || row.profile_id.clone().into(),
                    |profile| profile.label.clone(),
                ),
                model: profile
                    .as_ref()
                    .map_or_else(SharedString::default, |profile| profile.model.clone()),
                turns: format!(
                    "{} {}",
                    row.turns,
                    if row.turns == 1 { "turn" } else { "turns" }
                )
                .into(),
                input: compact_count(row.input_tokens).into(),
                output: compact_count(row.output_tokens).into(),
                total: compact_count(row.total_tokens).into(),
                share: if total == 0 {
                    0.0
                } else {
                    row.total_tokens as f32 / total as f32
                },
                selected: selected.as_str() == row.profile_id,
            }
        })
        .collect::<Vec<_>>();
    window.set_usage_total(if rows.is_empty() {
        SharedString::default()
    } else {
        format!("{} tokens reported", compact_count(total)).into()
    });
    window.set_usage_state("ready".into());
    window.set_usage_rows(ModelRc::new(VecModel::from(views)));
}

pub fn apply_ui_change(
    window: &MainWindow,
    preferences: &UiPreferences,
    source: UiChangeSource,
    summary: &str,
    undo: Option<&UiPatch>,
) {
    apply_preferences(window, preferences);
    window
        .global::<crate::App>()
        .set_thinking_mode(preferences.thinking.id().into());
    if source == UiChangeSource::Agent {
        window.set_ui_last_agent_change(summary.into());
        window.set_toast_text(format!("Agent changed: {summary}").into());
        window.set_toast_undo_token(
            undo.and_then(|patch| serde_json::to_string(patch).ok())
                .unwrap_or_default()
                .into(),
        );
    }
}

#[derive(Default)]
struct BrowserHistory {
    entries: Vec<String>,
    position: usize,
}

impl BrowserHistory {
    fn visit(&mut self, url: String) {
        if self.entries.get(self.position) == Some(&url) {
            return;
        }
        if !self.entries.is_empty() {
            self.entries.truncate(self.position + 1);
        }
        self.entries.push(url);
        self.position = self.entries.len() - 1;
    }

    fn can_back(&self) -> bool {
        self.position > 0
    }

    fn can_forward(&self) -> bool {
        self.position + 1 < self.entries.len()
    }

    fn current(&self) -> Option<&String> {
        self.entries.get(self.position)
    }
}

fn palette_items(window: &MainWindow, query: &str) -> Vec<PaletteItemView> {
    let needle = query.trim().to_ascii_lowercase();
    let mut items = Vec::new();
    let mut add = |id: String, label: String, detail: String, icon: &str| {
        let matches = needle.is_empty()
            || label.to_ascii_lowercase().contains(&needle)
            || detail.to_ascii_lowercase().contains(&needle);
        if matches {
            items.push(PaletteItemView {
                id: id.into(),
                label: label.into(),
                detail: detail.into(),
                icon: icon.into(),
            });
        }
    };
    add(
        "cmd:new".into(),
        "New chat".into(),
        "Ctrl+N".into(),
        "new-task",
    );
    add(
        "cmd:settings".into(),
        "Open settings".into(),
        "Ctrl+,".into(),
        "settings",
    );
    add(
        "cmd:modules".into(),
        "Choose modules".into(),
        "Turn parts of the interface on or off".into(),
        "boxes",
    );
    add(
        "cmd:updates".into(),
        "Check for updates".into(),
        "Open the Updates page".into(),
        "refresh",
    );
    add(
        "cmd:appearance".into(),
        "Change appearance".into(),
        "Theme, density, and text size".into(),
        "sparkles",
    );
    let surfaces = window.get_harness_surfaces();
    for surface in (0..surfaces.row_count()).filter_map(|index| surfaces.row_data(index)) {
        if !surface.configurable {
            continue;
        }
        let verb = if surface.visible {
            "Turn off"
        } else {
            "Turn on"
        };
        add(
            format!("module:{}:{}", surface.id, !surface.visible),
            format!("{verb} {}", surface.label),
            "Module".into(),
            &surface.icon,
        );
    }
    for preset in UI_PRESETS {
        add(
            format!("theme:{}", preset.id),
            format!("Theme: {}", preset.label),
            preset.detail.into(),
            "sparkles",
        );
    }
    let sessions = window.get_session_items();
    for session in (0..sessions.row_count())
        .filter_map(|index| sessions.row_data(index))
        .take(40)
    {
        add(
            format!("session:{}", session.id),
            session.title.to_string(),
            "Session".into(),
            "sessions",
        );
    }
    items.truncate(60);
    items
}

fn clamp_step(current: i32, delta: i32, (min, max): (u16, u16)) -> u16 {
    (current + delta).clamp(i32::from(min), i32::from(max)) as u16
}

/// Connect the callbacks only the workbench uses.
pub fn connect<F>(window: &MainWindow, dispatch: F)
where
    F: Fn(AppCommand) + Clone + 'static,
{
    let app = window.global::<App>();
    window.set_theme_presets(theme_preset_model(argentum_domain::DEFAULT_UI_PRESET));

    let patch_dispatch = dispatch.clone();
    let send_patch = move |patch: UiPatch| {
        patch_dispatch(AppCommand::UpdateUiPreferences {
            patch: Box::new(patch),
        });
    };

    // First run.
    {
        let dispatch = dispatch.clone();
        let weak = window.as_weak();
        app.on_first_run_choose(move |preset| {
            let Some(window) = weak.upgrade() else { return };
            let Some(kind) = crate::provider_kind_from_id(preset.kind.as_str()) else {
                window.set_first_run_error("This provider type is not available.".into());
                return;
            };
            window.set_first_run_error(SharedString::default());
            window.set_first_run_stage(if preset.needs_key { "key" } else { "modules" }.into());
            dispatch(AppCommand::SaveProviderProfile {
                profile: argentum_domain::ProviderProfile {
                    id: preset.id.to_string(),
                    label: preset.label.to_string(),
                    kind,
                    endpoint: preset.endpoint.to_string(),
                    model: preset.model.to_string(),
                    selected: true,
                },
            });
            dispatch(AppCommand::ListHarnessState);
        });
    }
    {
        let send_patch = send_patch.clone();
        let weak = window.as_weak();
        app.on_first_run_finish(move || {
            if let Some(window) = weak.upgrade() {
                window.set_first_run_open(false);
                window
                    .global::<App>()
                    .set_composer_focus_tick(window.global::<App>().get_composer_focus_tick() + 1);
            }
            send_patch(UiPatch {
                first_run_complete: Some(true),
                ..UiPatch::default()
            });
        });
    }

    // Panel.
    {
        let dispatch = dispatch.clone();
        let weak = window.as_weak();
        app.on_set_panel_visible(move |visible| {
            let Some(window) = weak.upgrade() else { return };
            if !visible {
                dispatch(AppCommand::SetInspectorSurface { surface: None });
                return;
            }
            let active = window.get_active_inspector();
            let modes = window.get_inspector_modes();
            let target = if (0..modes.row_count())
                .filter_map(|index| modes.row_data(index))
                .any(|mode| mode.id == active)
            {
                Some(active)
            } else {
                modes.row_data(0).map(|mode| mode.id)
            };
            if let Some(surface) = target.and_then(|id| SurfaceId::from_id(id.as_str())) {
                dispatch(AppCommand::SetInspectorSurface {
                    surface: Some(surface),
                });
            }
        });
    }
    {
        let send_patch = send_patch.clone();
        app.on_persist_layout(move |sidebar, panel| {
            send_patch(UiPatch {
                sidebar_width: Some(sidebar.round() as u16),
                panel_width: Some(panel.round() as u16),
                ..UiPatch::default()
            });
        });
    }

    // Git.
    {
        let dispatch = dispatch.clone();
        let weak = window.as_weak();
        app.on_git_refresh(move || {
            if let Some(window) = weak.upgrade() {
                window.set_git_state("loading".into());
            }
            dispatch(AppCommand::RefreshGitStatus);
        });
    }
    {
        let dispatch = dispatch.clone();
        let weak = window.as_weak();
        app.on_git_select(move |path| {
            if let Some(window) = weak.upgrade() {
                window.set_git_selected(path.clone());
                window.set_git_diff_lines(ModelRc::default());
                let files = window.get_git_files();
                let rows = (0..files.row_count())
                    .filter_map(|index| files.row_data(index))
                    .map(|mut file| {
                        file.selected = file.path == path;
                        file
                    })
                    .collect::<Vec<_>>();
                window.set_git_files(ModelRc::new(VecModel::from(rows)));
            }
            dispatch(AppCommand::LoadGitFileDiff {
                path: path.to_string(),
            });
        });
    }

    // Browser.
    let history = Rc::new(RefCell::new(BrowserHistory::default()));
    let navigate = {
        let dispatch = dispatch.clone();
        let weak = window.as_weak();
        let history = history.clone();
        Rc::new(move |url: String, record: bool| {
            let Some(window) = weak.upgrade() else { return };
            let url = url.trim().to_owned();
            if url.is_empty() {
                return;
            }
            let mut history = history.borrow_mut();
            if record {
                history.visit(url.clone());
            }
            window.set_browser_can_back(history.can_back());
            window.set_browser_can_forward(history.can_forward());
            window.set_browser_state("loading".into());
            window.set_browser_url(url.clone().into());
            dispatch(AppCommand::BrowserNavigate { url });
        })
    };
    {
        let navigate = navigate.clone();
        app.on_browser_go(move |url| navigate(url.to_string(), true));
    }
    {
        let navigate = navigate.clone();
        let history = history.clone();
        app.on_browser_back(move || {
            let target = {
                let mut history = history.borrow_mut();
                if !history.can_back() {
                    return;
                }
                history.position -= 1;
                history.current().cloned()
            };
            if let Some(url) = target {
                navigate(url, false);
            }
        });
    }
    {
        let navigate = navigate.clone();
        let history = history.clone();
        app.on_browser_forward(move || {
            let target = {
                let mut history = history.borrow_mut();
                if !history.can_forward() {
                    return;
                }
                history.position += 1;
                history.current().cloned()
            };
            if let Some(url) = target {
                navigate(url, false);
            }
        });
    }
    {
        let navigate = navigate.clone();
        let history = history.clone();
        app.on_browser_reload(move || {
            let target = history.borrow().current().cloned();
            if let Some(url) = target {
                navigate(url, false);
            }
        });
    }
    {
        // Links in chat come from a model. They only ever open in the reader, on a
        // direct click, and never hand off to another program.
        let navigate = navigate.clone();
        let dispatch = dispatch.clone();
        let weak = window.as_weak();
        app.on_open_link(move |url| {
            let Some(window) = weak.upgrade() else { return };
            let modes = window.get_inspector_modes();
            let browser_on = (0..modes.row_count())
                .filter_map(|index| modes.row_data(index))
                .any(|mode| mode.id == SurfaceId::Browser.id());
            if !browser_on {
                window.set_toast_undo_token(SharedString::default());
                window.set_toast_text("Turn on the Browser module to open links here.".into());
                return;
            }
            dispatch(AppCommand::SetInspectorSurface {
                surface: Some(SurfaceId::Browser),
            });
            navigate(url.to_string(), true);
        });
    }

    // Usage.
    {
        let dispatch = dispatch.clone();
        let weak = window.as_weak();
        app.on_usage_refresh(move || {
            if let Some(window) = weak.upgrade() {
                window.set_usage_state("loading".into());
            }
            dispatch(AppCommand::LoadProviderUsage);
        });
    }

    // Appearance.
    {
        let send_patch = send_patch.clone();
        app.on_ui_set_preset(move |preset| {
            send_patch(UiPatch {
                preset: Some(preset.to_string()),
                ..UiPatch::default()
            });
        });
    }
    {
        let send_patch = send_patch.clone();
        app.on_ui_set_density(move |density| {
            let density = match density.as_str() {
                "compact" => UiDensity::Compact,
                "spacious" => UiDensity::Spacious,
                _ => UiDensity::Comfortable,
            };
            send_patch(UiPatch {
                density: Some(density),
                ..UiPatch::default()
            });
        });
    }
    {
        let send_patch = send_patch.clone();
        let weak = window.as_weak();
        app.on_ui_step_font(move |delta| {
            let Some(window) = weak.upgrade() else { return };
            send_patch(UiPatch {
                font_percent: Some(clamp_step(
                    window.get_ui_font_percent(),
                    delta,
                    UI_FONT_PERCENT_RANGE,
                )),
                ..UiPatch::default()
            });
        });
    }
    {
        let send_patch = send_patch.clone();
        let weak = window.as_weak();
        app.on_ui_step_radius(move |delta| {
            let Some(window) = weak.upgrade() else { return };
            send_patch(UiPatch {
                radius_percent: Some(clamp_step(
                    window.get_ui_radius_percent(),
                    delta,
                    UI_RADIUS_PERCENT_RANGE,
                )),
                ..UiPatch::default()
            });
        });
    }
    {
        let send_patch = send_patch.clone();
        app.on_ui_set_panel_side(move |left| {
            send_patch(UiPatch {
                panel_side: Some(if left {
                    UiPanelSide::Left
                } else {
                    UiPanelSide::Right
                }),
                ..UiPatch::default()
            });
        });
    }
    {
        let send_patch = send_patch.clone();
        app.on_ui_set_user_bubbles(move |on| {
            send_patch(UiPatch {
                user_bubbles: Some(on),
                ..UiPatch::default()
            });
        });
    }
    {
        let send_patch = send_patch.clone();
        app.on_ui_reset(move || {
            send_patch(UiPatch {
                reset: true,
                ..UiPatch::default()
            });
        });
    }
    {
        let send_patch = send_patch.clone();
        let weak = window.as_weak();
        app.on_toast_undo(move || {
            let Some(window) = weak.upgrade() else { return };
            let token = window.get_toast_undo_token();
            window.set_toast_undo_token(SharedString::default());
            window.set_toast_text(SharedString::default());
            if let Ok(patch) = serde_json::from_str::<UiPatch>(token.as_str()) {
                send_patch(patch);
            }
        });
    }

    // Command palette.
    {
        let weak = window.as_weak();
        app.on_palette_query(move |query| {
            if let Some(window) = weak.upgrade() {
                let items = palette_items(&window, query.as_str());
                window.set_palette_items(ModelRc::new(VecModel::from(items)));
            }
        });
    }
    {
        let dispatch = dispatch.clone();
        let send_patch = send_patch.clone();
        let weak = window.as_weak();
        app.on_palette_run(move |id| {
            let Some(window) = weak.upgrade() else { return };
            let app = window.global::<App>();
            match id.split_once(':') {
                Some(("cmd", "new")) => dispatch(AppCommand::NewSession),
                Some(("cmd", "settings")) => app.invoke_open_settings(),
                Some(("cmd", "modules")) => {
                    window.set_settings_section("modules".into());
                    app.invoke_open_settings();
                }
                Some(("cmd", "updates")) => {
                    window.set_settings_section("updates".into());
                    app.invoke_open_settings();
                }
                Some(("cmd", "appearance")) => {
                    window.set_settings_section("appearance".into());
                    app.invoke_open_settings();
                }
                Some(("module", rest)) => {
                    if let Some((module, on)) = rest.rsplit_once(':') {
                        if let Some(surface) = SurfaceId::from_id(module) {
                            dispatch(AppCommand::SetSurfaceVisibility {
                                surface,
                                visible: on == "true",
                            });
                        }
                    }
                }
                Some(("theme", preset)) => send_patch(UiPatch {
                    preset: Some(preset.to_owned()),
                    ..UiPatch::default()
                }),
                Some(("session", session)) => app.invoke_select_session(session.into()),
                _ => {}
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diff_lines_carry_their_numbers() {
        let (lines, truncated) = parse_diff(
            "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -3,2 +3,2 @@ fn a\n keep\n-old\n+new\n",
        );
        assert!(!truncated);
        let shape = lines
            .iter()
            .map(|line| {
                (
                    line.kind.to_string(),
                    line.old_no.to_string(),
                    line.new_no.to_string(),
                )
            })
            .collect::<Vec<_>>();
        assert_eq!(shape[0], ("hunk".to_owned(), String::new(), String::new()));
        assert_eq!(shape[1], ("ctx".to_owned(), "3".to_owned(), "3".to_owned()));
        assert_eq!(shape[2], ("del".to_owned(), "4".to_owned(), String::new()));
        assert_eq!(shape[3], ("add".to_owned(), String::new(), "4".to_owned()));
    }

    #[test]
    fn diff_line_count_is_bounded() {
        let source = "+x\n".repeat(MAX_DIFF_LINES + 5);
        let (lines, truncated) = parse_diff(&source);
        assert_eq!(lines.len(), MAX_DIFF_LINES);
        assert!(truncated);
    }

    #[test]
    fn malformed_hunks_do_not_panic() {
        let _ = parse_diff("@@\n@@ x\n@@ -a +b @@\n");
    }

    #[test]
    fn project_names_read_as_proper_names() {
        assert_eq!(display_project_name("argentum"), "Argentum");
        assert_eq!(display_project_name("Argentum"), "Argentum");
        assert_eq!(display_project_name("iOS"), "iOS");
        assert_eq!(display_project_name("my-app"), "My-app");
        assert_eq!(display_project_name(""), "");
    }

    #[test]
    fn counts_read_compactly() {
        assert_eq!(compact_count(950), "950");
        assert_eq!(compact_count(12_345), "12.3k");
        assert_eq!(compact_count(2_500_000), "2.50M");
    }

    #[test]
    fn browser_history_truncates_forward_entries() {
        let mut history = BrowserHistory::default();
        history.visit("a".into());
        history.visit("b".into());
        history.position -= 1;
        assert!(history.can_forward());
        history.visit("c".into());
        assert_eq!(history.entries, ["a", "c"]);
        assert!(!history.can_forward());
        assert!(history.can_back());
    }

    #[test]
    fn escaped_text_is_not_treated_as_markup() {
        assert!(!has_inline_markup(r"a \*b\* \[c\]\(d\)"));
        assert!(has_inline_markup("a **b**"));
        assert!(has_inline_markup("[a](https://x.test)"));
    }

    #[test]
    fn every_icon_the_shell_names_exists() {
        let icons = include_str!("../../../ui/components/icon.slint");
        let sources = [
            include_str!("../../../ui/workbench/chat.slint"),
            include_str!("../../../ui/workbench/kit.slint"),
            include_str!("../../../ui/workbench/panels.slint"),
            include_str!("../../../ui/workbench/provider.slint"),
            include_str!("../../../ui/workbench/settings.slint"),
            include_str!("../../../ui/workbench/shell.slint"),
        ];
        for source in sources {
            for line in source.lines() {
                for prefix in ["icon: ", "name: "] {
                    let mut rest = line;
                    while let Some(at) = rest.find(prefix) {
                        rest = &rest[at + prefix.len()..];
                        // Up to the end of the value, which may be conditional.
                        let value = rest.split([';', ',']).next().unwrap_or_default();
                        let parts = value.split('"').collect::<Vec<_>>();
                        for pair in parts.chunks(2) {
                            let [before, name] = pair else { continue };
                            // A string after a comparison is a condition, not an icon.
                            let before = before.trim_end();
                            if before.ends_with("==") || before.ends_with("!=") {
                                continue;
                            }
                            assert!(
                                icons.contains(&format!("\"{name}\"")),
                                "icon '{name}' is not in the icon set: {line}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn every_surface_has_an_icon_the_icon_set_knows() {
        let source = include_str!("../../../ui/components/icon.slint");
        for surface in SurfaceId::ALL {
            let icon = surface_icon(surface);
            assert!(
                source.contains(&format!("\"{icon}\"")),
                "no icon named {icon}"
            );
        }
    }
}
