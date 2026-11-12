// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Commands behind the shell's optional modules, and the agent's view of the UI.
//!
//! The agent reads the interface with `ui_inspect` and changes it with `ui_update`.
//! Both go through the tool boundary like any other tool. `ui_update` waits for one
//! explicit approval, and the patch it applies is the same validated [`UiPatch`] the
//! settings page sends. Presentation cannot grant authority: a module being on does
//! not enable the capability behind it, and the approval prompt can be neither
//! hidden nor restyled.

use std::collections::BTreeMap;

use argentum_domain::{
    AppEvent, BrowserPage, BrowserPageState, GitDiffSnapshot, GitStatusSnapshot, GitViewState,
    SurfaceId, UiChangeSource, UiPanelSide, UiPatch, UiPreferences, UI_CHAT_WIDTH_RANGE,
    UI_COLOR_TOKENS, UI_FONT_PERCENT_RANGE, UI_PANEL_WIDTH_RANGE, UI_PRESETS,
    UI_RADIUS_PERCENT_RANGE, UI_SIDEBAR_WIDTH_RANGE,
};

use super::{RuntimeError, RuntimeService};

const GIT_CAPABILITY: &str = "git.read";
const BROWSER_CAPABILITY: &str = "browser.reader";
const GIT_BLOCKED_MESSAGE: &str = "Git runs as an external process, and the current execution profile does not allow it. Turn on Git inspection under Permissions.";
const BROWSER_BLOCKED_MESSAGE: &str =
    "Opening a page needs network access, and the current execution profile does not allow it. Turn on the page reader under Permissions.";

/// JSON schema for `ui_update`. Enumerations come from the same tables validation
/// uses, so the model is told exactly what will be accepted.
pub(super) fn ui_update_schema() -> serde_json::Value {
    let presets = UI_PRESETS
        .iter()
        .map(|preset| preset.id)
        .collect::<Vec<_>>();
    serde_json::json!({
        "type": "object",
        "properties": {
            "preset": { "type": "string", "enum": presets },
            "colors": {
                "type": "object",
                "description": "Token overrides as #RRGGBB or #RRGGBBAA, or null to remove one. See ui_inspect for tokens.",
                "additionalProperties": { "type": ["string", "null"] }
            },
            "font_percent": { "type": "integer", "minimum": UI_FONT_PERCENT_RANGE.0, "maximum": UI_FONT_PERCENT_RANGE.1 },
            "radius_percent": { "type": "integer", "minimum": UI_RADIUS_PERCENT_RANGE.0, "maximum": UI_RADIUS_PERCENT_RANGE.1 },
            "density": { "type": "string", "enum": ["compact", "comfortable", "spacious"] },
            "font_ui": { "type": "string", "maxLength": 64 },
            "font_mono": { "type": "string", "maxLength": 64 },
            "sidebar_width": { "type": "integer", "minimum": UI_SIDEBAR_WIDTH_RANGE.0, "maximum": UI_SIDEBAR_WIDTH_RANGE.1 },
            "panel_width": { "type": "integer", "minimum": UI_PANEL_WIDTH_RANGE.0, "maximum": UI_PANEL_WIDTH_RANGE.1 },
            "chat_width": { "type": "integer", "minimum": UI_CHAT_WIDTH_RANGE.0, "maximum": UI_CHAT_WIDTH_RANGE.1 },
            "panel_side": { "type": "string", "enum": ["left", "right"] },
            "user_bubbles": { "type": "boolean" },
            "reduced_motion": { "type": "boolean" },
            "modules": {
                "type": "object",
                "description": "Module IDs from ui_inspect mapped to on or off.",
                "additionalProperties": { "type": "boolean" }
            },
            "module_profile": { "type": "string", "enum": ["focused", "standard", "review", "trace", "full"] },
            "reset": { "type": "boolean", "description": "Return appearance to defaults first." }
        },
        "additionalProperties": false
    })
}

fn module_region(surface: SurfaceId) -> &'static str {
    match surface {
        SurfaceId::Conversation => "main",
        SurfaceId::Approvals => "overlay",
        SurfaceId::Sessions => "sidebar",
        SurfaceId::ContextMeter => "composer",
        SurfaceId::Reasoning => "message",
        SurfaceId::Goal => "above-composer",
        SurfaceId::CommandPalette => "overlay",
        SurfaceId::StatusBar => "bottom",
        _ if surface.is_inspector() => "side-panel",
        _ => "none",
    }
}

fn hex(color: argentum_domain::UiColor) -> String {
    if color.a == 255 {
        format!("#{:02X}{:02X}{:02X}", color.r, color.g, color.b)
    } else {
        format!(
            "#{:02X}{:02X}{:02X}{:02X}",
            color.r, color.g, color.b, color.a
        )
    }
}

fn inverse_patch(previous: &UiPreferences, next: &UiPreferences) -> UiPatch {
    let mut colors: BTreeMap<String, Option<String>> = previous
        .colors
        .iter()
        .map(|(token, value)| (token.clone(), Some(value.clone())))
        .collect();
    for token in next.colors.keys() {
        colors.entry(token.clone()).or_insert(None);
    }
    UiPatch {
        preset: (previous.preset != next.preset).then(|| previous.preset.clone()),
        colors,
        font_percent: Some(previous.font_percent),
        radius_percent: Some(previous.radius_percent),
        density: Some(previous.density),
        font_ui: Some(previous.font_ui.clone()),
        font_mono: Some(previous.font_mono.clone()),
        sidebar_width: Some(previous.sidebar_width),
        panel_width: Some(previous.panel_width),
        chat_width: Some(previous.chat_width),
        panel_side: Some(previous.panel_side),
        user_bubbles: Some(previous.user_bubbles),
        reduced_motion: Some(previous.reduced_motion),
        ..UiPatch::default()
    }
}

impl RuntimeService {
    pub(super) fn publish_ui_preferences(&self) -> Result<(), RuntimeError> {
        self.publish_transient(AppEvent::UiPreferencesLoaded {
            preferences: Box::new(self.store.load_ui_preferences()?),
            source: UiChangeSource::User,
            summary: String::new(),
            undo: None,
        });
        Ok(())
    }

    fn resolve_patch_modules(
        &self,
        patch: &UiPatch,
    ) -> Result<Option<argentum_domain::LayoutProfile>, RuntimeError> {
        if patch.modules.is_empty() && patch.module_profile.is_none() {
            return Ok(None);
        }
        let mut layout = self.load_harness_layout()?;
        if let Some(profile) = &patch.module_profile {
            layout = self.with_harness(|harness| harness.apply_profile(&layout, profile))?;
        }
        for (module, enabled) in &patch.modules {
            let surface = SurfaceId::from_id(module)
                .ok_or_else(|| RuntimeError::UiPreferences(format!("unknown module '{module}'")))?;
            layout = self.with_harness(|harness| {
                harness.set_surface_visibility(&layout, surface, *enabled)
            })?;
        }
        Ok(Some(layout))
    }

    /// Checks everything without saving, so a bad patch is rejected before anyone
    /// is asked to approve it.
    pub(super) fn validate_agent_ui_patch(&self, patch: &UiPatch) -> Result<(), RuntimeError> {
        if patch.first_run_complete.is_some() {
            return Err(RuntimeError::UiPreferences(
                "first-run state is not something the agent can change".to_owned(),
            ));
        }
        if patch.describe() == "no change" {
            return Err(RuntimeError::UiPreferences(
                "the patch does not change anything".to_owned(),
            ));
        }
        self.store
            .load_ui_preferences()?
            .with_patch(patch)
            .map_err(|error| RuntimeError::UiPreferences(error.to_string()))?;
        self.resolve_patch_modules(patch)?;
        Ok(())
    }

    /// Applies a patch from any source. Modules and appearance are resolved first,
    /// so a failure in either leaves both unchanged.
    pub(super) fn apply_ui_patch(
        &self,
        patch: &UiPatch,
        source: UiChangeSource,
    ) -> Result<(), RuntimeError> {
        let previous = self.store.load_ui_preferences()?;
        let next = previous
            .with_patch(patch)
            .map_err(|error| RuntimeError::UiPreferences(error.to_string()))?;
        let previous_layout = self.load_harness_layout()?;
        let next_layout = self.resolve_patch_modules(patch)?;

        let mut undo = inverse_patch(&previous, &next);
        if let Some(layout) = &next_layout {
            for surface in SurfaceId::ALL {
                let before = previous_layout
                    .visible
                    .get(&surface)
                    .copied()
                    .unwrap_or(false);
                let after = layout.visible.get(&surface).copied().unwrap_or(false);
                if before != after {
                    undo.modules.insert(surface.id().to_owned(), before);
                }
            }
        }

        if next != previous {
            self.store.save_ui_preferences(&next)?;
        }
        if let Some(layout) = next_layout {
            self.save_and_publish_harness_layout(layout)?;
        }
        self.publish_transient(AppEvent::UiPreferencesLoaded {
            preferences: Box::new(next),
            source,
            summary: patch.describe(),
            undo: Some(Box::new(undo)),
        });
        Ok(())
    }

    /// What the agent sees when it inspects the interface. Presentation facts only:
    /// no conversation text, paths, provider endpoints, or credentials.
    pub(super) fn ui_manifest(&self) -> Result<serde_json::Value, RuntimeError> {
        let preferences = self.store.load_ui_preferences()?;
        let layout = self.load_harness_layout()?;
        let snapshot = self.harness_snapshot()?;
        let colors = preferences.resolved_colors();
        let modules = snapshot
            .surfaces
            .iter()
            .map(|surface| {
                serde_json::json!({
                    "id": surface.id.id(),
                    "label": surface.label,
                    "region": module_region(surface.id),
                    "on": surface.visible,
                    "changeable": surface.configurable,
                    "available": surface.availability == argentum_domain::HarnessAvailability::Available,
                    "detail": if surface.unavailable_reason.is_empty() { &surface.detail } else { &surface.unavailable_reason },
                })
            })
            .collect::<Vec<_>>();
        let tokens = UI_COLOR_TOKENS
            .iter()
            .map(|token| {
                serde_json::json!({
                    "id": token.id,
                    "role": token.role,
                    "value": colors.get(token.id).copied().map(hex),
                    "overridden": preferences.colors.contains_key(token.id),
                })
            })
            .collect::<Vec<_>>();
        let presets = UI_PRESETS
            .iter()
            .map(|preset| serde_json::json!({ "id": preset.id, "label": preset.label, "detail": preset.detail }))
            .collect::<Vec<_>>();
        let profiles = snapshot
            .profiles
            .iter()
            .filter(|profile| profile.selectable)
            .map(|profile| serde_json::json!({ "id": profile.id, "label": profile.label, "detail": profile.detail }))
            .collect::<Vec<_>>();
        Ok(serde_json::json!({
            "about": "Argentum's native interface. The core is the conversation, composer, provider chooser, and approval prompt. Everything else is an optional module. Use ui_update to change anything marked changeable. Each change asks the user to approve it.",
            "layout": {
                "regions": {
                    "sidebar": "Left column, present when the sessions module is on.",
                    "main": "Conversation with the composer below it. Empty sessions center the composer.",
                    "side-panel": "Resizable tabbed column. Holds every module with this region.",
                    "bottom": "Status bar, when on.",
                    "overlay": "Settings, command palette, provider menu, and approval prompt.",
                },
                "panel_side": match preferences.panel_side {
                    UiPanelSide::Left => "left",
                    UiPanelSide::Right => "right",
                },
                "panel_open": !layout.inspector_collapsed,
                "active_panel": layout.active_inspector.map(SurfaceId::id),
                "sidebar_width": preferences.sidebar_width,
                "panel_width": preferences.panel_width,
                "chat_width": preferences.chat_width,
            },
            "modules": modules,
            "module_profile": snapshot.selected_profile_id,
            "module_profiles": profiles,
            "theme": {
                "preset": preferences.preset,
                "presets": presets,
                "density": preferences.density.id(),
                "font_percent": preferences.font_percent,
                "radius_percent": preferences.radius_percent,
                "font_ui": preferences.font_ui,
                "font_mono": preferences.font_mono,
                "user_bubbles": preferences.user_bubbles,
                "reduced_motion": preferences.reduced_motion,
                "tokens": tokens,
            },
            "rules": [
                "Colors must stay legible. Text below 4.5:1 contrast on its background is rejected, and nothing changes.",
                "The approval prompt uses fixed colors and cannot be hidden, moved, or restyled.",
                "Modules change what is shown. They never grant permissions, and this tool cannot change permissions.",
                "Interface text cannot be changed.",
            ],
        }))
    }

    pub(super) fn publish_provider_usage(&self) -> Result<(), RuntimeError> {
        self.publish_transient(AppEvent::ProviderUsageLoaded {
            rows: self.store.provider_usage(self.project_id)?,
            daily: self.store.usage_by_day(self.project_id, 30)?,
            projects: self.store.projects_overview(self.project_id)?,
        });
        Ok(())
    }

    /// A module being on grants nothing. Its own capability in the execution policy
    /// is what lets the action run.
    fn module_capability_enabled(&self, capability_id: &str) -> Result<bool, RuntimeError> {
        let policy = self.load_execution_policy()?;
        Ok(self.with_harness(|harness| harness.capability_is_enabled(&policy, capability_id)))
    }

    /// Runs only from a direct user action in the Git module.
    pub(super) async fn refresh_git_status(&self) -> Result<(), RuntimeError> {
        if !self.module_capability_enabled(GIT_CAPABILITY)? {
            self.publish_transient(AppEvent::GitStatusLoaded(GitStatusSnapshot {
                state: GitViewState::Blocked,
                message: GIT_BLOCKED_MESSAGE.to_owned(),
                branch: String::new(),
                files: Vec::new(),
                truncated: false,
            }));
            return Ok(());
        }
        let root = self.workspace_root().to_path_buf();
        let snapshot = tokio::task::spawn_blocking(move || argentum_gitview::status(&root))
            .await
            .unwrap_or_else(|_| GitStatusSnapshot {
                state: GitViewState::Failed,
                message: "Git status did not finish.".to_owned(),
                branch: String::new(),
                files: Vec::new(),
                truncated: false,
            });
        if let Ok(mut known) = self.git_known_paths.lock() {
            *known = snapshot
                .files
                .iter()
                .map(|file| (file.path.clone(), file.status.clone()))
                .collect();
        }
        self.publish_transient(AppEvent::GitStatusLoaded(snapshot));
        Ok(())
    }

    /// `path` must come from the last status, so nothing outside what Git already
    /// listed can reach the command line.
    pub(super) async fn load_git_file_diff(&self, path: String) -> Result<(), RuntimeError> {
        let failed = |state, message: &str, path: &str| GitDiffSnapshot {
            state,
            message: message.to_owned(),
            path: path.to_owned(),
            diff: String::new(),
            truncated: false,
        };
        if !self.module_capability_enabled(GIT_CAPABILITY)? {
            self.publish_transient(AppEvent::GitDiffLoaded(failed(
                GitViewState::Blocked,
                GIT_BLOCKED_MESSAGE,
                &path,
            )));
            return Ok(());
        }
        let status = self
            .git_known_paths
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?
            .get(&path)
            .cloned();
        let Some(status) = status else {
            self.publish_transient(AppEvent::GitDiffLoaded(failed(
                GitViewState::Failed,
                "That file is no longer in the status list. Refresh and try again.",
                &path,
            )));
            return Ok(());
        };
        let root = self.workspace_root().to_path_buf();
        let requested = path.clone();
        let snapshot = tokio::task::spawn_blocking(move || {
            argentum_gitview::file_diff(&root, &requested, status == "?")
        })
        .await
        .unwrap_or_else(|_| failed(GitViewState::Failed, "Git diff did not finish.", &path));
        self.publish_transient(AppEvent::GitDiffLoaded(snapshot));
        Ok(())
    }

    /// Runs only from a direct user action in the Browser module. Page content stays
    /// in a transient event and is never stored or sent to a model.
    pub(super) async fn browser_navigate(&self, url: String) -> Result<(), RuntimeError> {
        if !self.module_capability_enabled(BROWSER_CAPABILITY)? {
            self.publish_transient(AppEvent::BrowserPageLoaded(BrowserPage {
                state: BrowserPageState::Blocked,
                message: BROWSER_BLOCKED_MESSAGE.to_owned(),
                url,
                title: String::new(),
                blocks: Vec::new(),
                truncated: false,
            }));
            return Ok(());
        }
        let page = argentum_browser::fetch(&url).await;
        self.publish_transient(AppEvent::BrowserPageLoaded(page));
        Ok(())
    }
}
