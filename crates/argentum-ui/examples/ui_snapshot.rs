//! Renders every shell state to a PNG with the software renderer, with no window
//! and no host. This is a visual QA tool. The fixture data below exists only here
//! and never reaches the product.
//!
//! ```text
//! cargo run -p argentum-ui --example ui_snapshot -- OUT_DIR [SCENE_FILTER]
//! ```

use std::fs::File;
use std::io::BufWriter;
use std::path::Path;
use std::rc::Rc;

use argentum_domain::{
    BrowserBlock, BrowserBlockKind, BrowserPage, BrowserPageState, GitDiffSnapshot, GitFileStatus,
    GitStatusSnapshot, GitViewState, HarnessCapabilityKind, HarnessCapabilityState,
    HarnessExecutionPolicy, HarnessReadiness, ProviderUsageRow, SurfaceId, UiPreferences,
};
use argentum_harness::{HarnessFacts, HarnessRegistry};
use argentum_ui::{workbench, App, UiHandle};
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::{Platform, WindowAdapter};
use slint::{ComponentHandle, Model, ModelRc, PhysicalSize, VecModel};

struct Headless(Rc<MinimalSoftwareWindow>);

impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

const REPLY: &str = "I found the problem. The session store opens a new transaction per row, so a large import holds the write lock for **minutes**.\n\n## What I changed\n\n1. Batched the inserts in `import_sessions`.\n2. Added a `busy_timeout` so readers wait instead of failing.\n3. Kept the public API the same.\n\n```rust\nlet tx = connection.transaction()?;\nfor row in rows {\n    insert_session(&tx, &row)?;\n}\ntx.commit()?;\n```\n\n> The existing tests pass, and the import went from 94 s to 2.1 s on the sample file.\n\n| Step | Before | After |\n| --- | --- | --- |\n| Import | 94 s | 2.1 s |\n| Lookup | 3 ms | 3 ms |\n\nSee the [SQLite docs](https://sqlite.org/lang_transaction.html) for why this matters.";

const DIFF: &str = "diff --git a/src/store.rs b/src/store.rs\n--- a/src/store.rs\n+++ b/src/store.rs\n@@ -41,7 +41,9 @@ pub fn import_sessions(rows: &[Row]) -> Result<()> {\n     let connection = open()?;\n-    for row in rows {\n-        insert_session(&connection, row)?;\n+    let tx = connection.transaction()?;\n+    for row in rows {\n+        insert_session(&tx, row)?;\n     }\n+    tx.commit()?;\n     Ok(())\n }\n";

fn conversation(ui: &UiHandle) {
    use argentum_domain::{
        ConversationMessage, ConversationMessageStatus, ConversationRole, ModelUsage,
    };
    let id = argentum_domain::now();
    let user = ConversationMessage {
        id: uuid::Uuid::new_v4(),
        project_id: uuid::Uuid::new_v4(),
        session_id: uuid::Uuid::new_v4(),
        run_id: uuid::Uuid::new_v4(),
        role: ConversationRole::User,
        status: ConversationMessageStatus::Complete,
        created_at: id,
        text: "Imports of large session files take over a minute. Can you find out why and fix it?"
            .into(),
        reasoning: String::new(),
        usage: None,
        profile_id: String::new(),
        model: String::new(),
    };
    let assistant = ConversationMessage {
        role: ConversationRole::Assistant,
        text: REPLY.into(),
        reasoning: "The trace shows one fsync per row, which points at autocommit.".into(),
        usage: Some(ModelUsage {
            input_tokens: 41_200,
            output_tokens: 610,
            total_tokens: 41_810,
            context_window_tokens: Some(200_000),
            reasoning_tokens: None,
            cached_input_tokens: None,
        }),
        profile_id: "lm-studio".into(),
        model: "qwen3-coder-next".into(),
        ..user.clone()
    };
    let window = ui.window();
    window.set_conversation_messages(argentum_ui::conversation_message_model_for_tools(&[
        user, assistant,
    ]));
    window.set_session_title("Slow session import".into());
    window.set_usage_reported(true);
    window.set_usage_total_tokens(41_810);
    window.set_context_window_reported(true);
    window.set_context_window_tokens(200_000);
}

fn base(ui: &UiHandle) {
    let window = ui.window();
    window.set_update_current("0.1.0".into());
    window.set_project_name(workbench::display_project_name("argentum").into());
    window.set_workspace_label("A:\\ag064\\argentum".into());
    window.set_provider_id("lm-studio".into());
    window.set_provider_label("LM Studio".into());
    window.set_provider_kind("lm-studio".into());
    window.set_provider_kind_label("Local".into());
    window.set_provider_endpoint("http://127.0.0.1:1234/v1/".into());
    window.set_provider_model("qwen3-coder-next".into());
    window.set_provider_draft_label("LM Studio".into());
    window.set_provider_draft_endpoint("http://127.0.0.1:1234/v1/".into());
    window.set_provider_draft_model("qwen3-coder-next".into());
    window.set_execution_profile_label("Confirm Before Changes".into());
    window.set_provider_profiles(ModelRc::new(VecModel::from(vec![
        argentum_ui::ProviderProfileView {
            id: "lm-studio".into(),
            label: "LM Studio".into(),
            kind: "lm-studio".into(),
            kind_label: "Local".into(),
            endpoint: "http://127.0.0.1:1234/v1/".into(),
            model: "qwen3-coder-next".into(),
            selected: true,
            credential_configured: false,
            credential_supported: false,
        },
        argentum_ui::ProviderProfileView {
            id: "deepseek".into(),
            label: "DeepSeek".into(),
            kind: "openai-compatible".into(),
            kind_label: "OpenAI-compatible".into(),
            endpoint: "https://api.deepseek.com/v1/".into(),
            model: "deepseek-v4-flash".into(),
            selected: false,
            credential_configured: true,
            credential_supported: true,
        },
    ])));
    window.set_provider_models(ModelRc::new(VecModel::from(vec![
        argentum_ui::ProviderModelView {
            id: "qwen3-coder-next".into(),
            label: "qwen3-coder-next".into(),
            context_window_tokens: 200_000,
            context_window_reported: true,
            selected: true,
        },
        argentum_ui::ProviderModelView {
            id: "gemma-4-large".into(),
            label: "gemma-4-large".into(),
            context_window_tokens: 0,
            context_window_reported: false,
            selected: false,
        },
    ])));
    window.set_session_items(ModelRc::new(VecModel::from(
        [
            ("Slow session import", true),
            ("Add retry to the provider probe", false),
            ("Write release notes for 0.1", false),
            ("Why does clippy flag this?", false),
        ]
        .map(|(title, active)| argentum_ui::SessionItemView {
            id: title.into(),
            title: title.into(),
            detail: "".into(),
            active,
            archived: false,
            pinned: false,
        })
        .to_vec(),
    )));
    workbench::apply_preferences(
        window,
        &UiPreferences {
            first_run_complete: true,
            ..UiPreferences::default()
        },
    );
}

fn modules(ui: &UiHandle, profile: &str) {
    let registry = HarnessRegistry::built_in().expect("registry");
    let layout = registry
        .apply_profile(&argentum_domain::LayoutProfile::default(), profile)
        .expect("profile");
    let snapshot = registry.snapshot(
        &layout,
        &HarnessExecutionPolicy::default(),
        &HarnessFacts::default(),
    );
    ui.apply_event(&argentum_domain::AppEvent::HarnessSnapshotLoaded(snapshot));
    ui.window().set_active_inspector("".into());
    ui.window().set_inspector_collapsed(true);
}

fn open_panel(ui: &UiHandle, surface: SurfaceId) {
    ui.window().set_active_inspector(surface.id().into());
    ui.window().set_inspector_collapsed(false);
}

fn git(ui: &UiHandle) {
    ui.apply_event(&argentum_domain::AppEvent::GitStatusLoaded(
        GitStatusSnapshot {
            state: GitViewState::Ready,
            message: String::new(),
            branch: "fix/import-batching".into(),
            files: [
                ("src/store.rs", "M"),
                ("src/import.rs", "M"),
                ("tests/import.rs", "A"),
                ("notes.txt", "?"),
            ]
            .map(|(path, status)| GitFileStatus {
                path: path.into(),
                status: status.into(),
            })
            .to_vec(),
            truncated: false,
        },
    ));
    ui.window().set_git_selected("src/store.rs".into());
    let files = ui.window().get_git_files();
    let rows = (0..files.row_count())
        .filter_map(|index| files.row_data(index))
        .map(|mut file| {
            file.selected = file.path == "src/store.rs";
            file
        })
        .collect::<Vec<_>>();
    ui.window()
        .set_git_files(ModelRc::new(VecModel::from(rows)));
    ui.apply_event(&argentum_domain::AppEvent::GitDiffLoaded(GitDiffSnapshot {
        state: GitViewState::Ready,
        message: String::new(),
        path: "src/store.rs".into(),
        diff: DIFF.into(),
        truncated: false,
    }));
}

fn browser(ui: &UiHandle) {
    let block = |kind, text: &str, markdown: &str| BrowserBlock {
        kind,
        text: text.into(),
        markdown: markdown.into(),
    };
    ui.apply_event(&argentum_domain::AppEvent::BrowserPageLoaded(BrowserPage {
        state: BrowserPageState::Ready,
        message: String::new(),
        url: "https://sqlite.org/lang_transaction.html".into(),
        title: "Transaction".into(),
        blocks: vec![
            block(BrowserBlockKind::Heading2, "Transactions", "Transactions"),
            block(BrowserBlockKind::Paragraph, "No reads or writes occur except within a transaction. Any command that accesses the database will automatically start a transaction if one is not already in effect.", "No reads or writes occur except within a transaction\\. Any command that accesses the database will automatically start a transaction if one is not already in effect\\."),
            block(BrowserBlockKind::ListItem, "DEFERRED means no lock until first use", "**DEFERRED** means no lock until first use"),
            block(BrowserBlockKind::ListItem, "IMMEDIATE starts a write transaction", "**IMMEDIATE** starts a write transaction"),
            block(BrowserBlockKind::Code, "BEGIN IMMEDIATE;\nINSERT INTO t VALUES (1);\nCOMMIT;", ""),
        ],
        truncated: false,
    }));
}

fn usage(ui: &UiHandle) {
    ui.apply_event(&argentum_domain::AppEvent::ProviderUsageLoaded {
        daily: Vec::new(),
        projects: Vec::new(),
        rows: vec![
            ProviderUsageRow {
                profile_id: "lm-studio".into(),
                turns: 84,
                input_tokens: 1_240_000,
                output_tokens: 96_000,
                total_tokens: 1_336_000,
            },
            ProviderUsageRow {
                profile_id: "deepseek".into(),
                turns: 12,
                input_tokens: 210_000,
                output_tokens: 18_400,
                total_tokens: 228_400,
            },
        ],
    });
}

fn capabilities(ui: &UiHandle) {
    // Keeps the import honest if the shape changes.
    let _ = (
        HarnessCapabilityKind::Tool,
        HarnessReadiness::Ready,
        std::mem::size_of::<HarnessCapabilityState>(),
    );
    let _ = ui;
}

fn provider_catalog(ui: &UiHandle) {
    let preset = |id: &str, label: &str, group: &str, local: bool, supported: bool| {
        argentum_ui::ProviderPresetView {
            id: id.into(),
            label: label.into(),
            kind: "openai-compatible".into(),
            kind_label: "OpenAI compatible".into(),
            endpoint: format!("https://{id}.example.test/v1/").into(),
            default_model: "fixture-model".into(),
            credential: if local {
                "No credential required".into()
            } else {
                "Saved credential required".into()
            },
            api_key_env: "".into(),
            docs_url: "https://example.test/docs".into(),
            group: group.into(),
            group_label: group.into(),
            local,
            supported,
            tested: id == "deepseek",
            notes: if supported {
                "".into()
            } else {
                "This route has no adapter yet.".into()
            },
            detail: if local {
                "Runs on this computer".into()
            } else {
                "API key".into()
            },
            model: "fixture-model".into(),
            needs_key: !local,
            featured: matches!(
                id,
                "minimax" | "deepseek" | "openai" | "llama-cpp" | "custom"
            ),
            icon: if local { "hard-drive" } else { "provider" }.into(),
        }
    };
    let hosted = vec![
        preset("minimax", "MiniMax", "hosted", false, true),
        preset("deepseek", "DeepSeek", "hosted", false, true),
        preset("openai", "OpenAI", "hosted", false, true),
    ];
    let aggregator = vec![preset(
        "openrouter",
        "OpenRouter",
        "aggregator",
        false,
        true,
    )];
    let local = vec![preset("llama-cpp", "Local llama.cpp", "local", true, true)];
    let custom = vec![preset("custom", "Custom server", "custom", true, true)];
    let unsupported = vec![preset(
        "vendor",
        "Vendor route",
        "unsupported",
        false,
        false,
    )];
    let window = ui.window();
    let mut all = hosted.clone();
    all.extend(aggregator.clone());
    all.extend(local.clone());
    all.extend(custom.clone());
    window.set_provider_preset_detail(hosted[1].clone());
    window.set_provider_presets(ModelRc::new(VecModel::from(all)));
    window.set_provider_presets_hosted(ModelRc::new(VecModel::from(hosted)));
    window.set_provider_presets_aggregator(ModelRc::new(VecModel::from(aggregator)));
    window.set_provider_presets_local(ModelRc::new(VecModel::from(local)));
    window.set_provider_presets_custom(ModelRc::new(VecModel::from(custom)));
    window.set_provider_presets_unsupported(ModelRc::new(VecModel::from(unsupported)));
}

fn record_lines(lines: &[(&str, &str)]) -> ModelRc<argentum_ui::RecordLineView> {
    ModelRc::new(VecModel::from(
        lines
            .iter()
            .map(|(label, detail)| argentum_ui::RecordLineView {
                label: (*label).into(),
                detail: (*detail).into(),
            })
            .collect::<Vec<_>>(),
    ))
}

struct Scene {
    name: &'static str,
    size: (u32, u32),
    setup: fn(&UiHandle),
}

fn scenes() -> Vec<Scene> {
    fn s(name: &'static str, size: (u32, u32), setup: fn(&UiHandle)) -> Scene {
        Scene { name, size, setup }
    }
    const D: (u32, u32) = (1280, 820);
    vec![
        s("01-first-run-provider", D, |ui| {
            provider_catalog(ui);
            modules(ui, "focused");
            ui.window().set_first_run_open(true);
        }),
        s("02-first-run-modules", D, |ui| {
            modules(ui, "focused");
            ui.window().set_first_run_open(true);
            ui.window().set_first_run_stage("modules".into());
        }),
        s("03-minimal-empty", D, |ui| modules(ui, "focused")),
        s("04-minimal-chat", D, |ui| {
            modules(ui, "focused");
            conversation(ui);
        }),
        s("05-standard-chat", D, |ui| {
            modules(ui, "standard");
            conversation(ui);
        }),
        s("06-full-git", (1440, 900), |ui| {
            modules(ui, "full");
            conversation(ui);
            open_panel(ui, SurfaceId::GitDiff);
            git(ui);
        }),
        s("07-full-browser", (1440, 900), |ui| {
            modules(ui, "full");
            conversation(ui);
            open_panel(ui, SurfaceId::Browser);
            browser(ui);
        }),
        s("08-full-usage", (1440, 900), |ui| {
            modules(ui, "full");
            conversation(ui);
            open_panel(ui, SurfaceId::Usage);
            usage(ui);
        }),
        s("09-git-blocked", D, |ui| {
            modules(ui, "review");
            conversation(ui);
            open_panel(ui, SurfaceId::GitDiff);
            ui.window().set_git_state("blocked".into());
            ui.window().set_git_message("Git runs as an external process, and this project does not allow that yet. You can allow it under Permissions.".into());
        }),
        s("10-settings-providers", D, |ui| {
            modules(ui, "standard");
            ui.window().set_settings_open(true);
        }),
        s("11-settings-modules", D, |ui| {
            modules(ui, "standard");
            ui.window().set_settings_open(true);
            ui.window().set_settings_section("modules".into());
        }),
        s("12-settings-appearance", D, |ui| {
            modules(ui, "standard");
            ui.window().set_settings_open(true);
            ui.window().set_settings_section("appearance".into());
            ui.window()
                .set_ui_last_agent_change("theme to ember, text size to 110%".into());
        }),
        s("13-settings-permissions", D, |ui| {
            modules(ui, "standard");
            capabilities(ui);
            ui.window().set_settings_open(true);
            ui.window().set_settings_section("permissions".into());
        }),
        s("25-settings-updates", D, |ui| {
            modules(ui, "standard");
            ui.window().set_settings_open(true);
            ui.window().set_settings_section("updates".into());
            ui.window().set_update_current("0.1.0".into());
            ui.window().set_update_state("available".into());
            ui.window().set_update_version("0.2.0".into());
            workbench::set_update_notes(
                ui.window(),
                "## New\n\n- A faster session import\n- Fixes to the Git panel",
            );
        }),
        s("26-first-run-key", D, |ui| {
            provider_catalog(ui);
            ui.window().set_first_run_open(true);
            ui.window().set_first_run_stage("key".into());
            ui.window().set_provider_label("DeepSeek".into());
            ui.window().set_provider_credential_supported(true);
        }),
        s("27-settings-catalog", (1280, 1500), |ui| {
            modules(ui, "standard");
            provider_catalog(ui);
            ui.window().set_provider_credential_supported(true);
            ui.window().set_provider_credential_configured(true);
            ui.window().set_settings_open(true);
            ui.window().set_settings_section("providers".into());
        }),
        s("28-settings-verification", D, |ui| {
            modules(ui, "standard");
            ui.window()
                .set_verification_runners(ModelRc::new(VecModel::from(vec![
                    argentum_ui::VerificationRunnerView {
                        id: "cargo-test".into(),
                        origin: "Workspace".into(),
                        state: "Not run".into(),
                        detail: "Runs the workspace tests with no network.".into(),
                        available: true,
                        approval_required: true,
                        approved: false,
                        definition: "cargo test --workspace --locked".into(),
                    },
                ])));
            ui.window().set_settings_open(true);
            ui.window().set_settings_section("verification".into());
        }),
        s("29-settings-capabilities", D, |ui| {
            modules(ui, "standard");
            ui.window()
                .set_browser_summary("Installed, not activated".into());
            ui.window().set_browser_extensions_built(true);
            ui.window().set_browser_definition_ready(true);
            ui.window().set_browser_configured(true);
            ui.window()
                .set_browser_readiness_detail("Adapter handshake passed with 8 tools.".into());
            ui.window().set_desktop_summary("Not built".into());
            ui.window().set_settings_open(true);
            ui.window().set_settings_section("capabilities".into());
        }),
        s("30-settings-diagnostics", D, |ui| {
            modules(ui, "standard");
            ui.window()
                .set_accounting_summary("3 runs, 12 requests".into());
            ui.window().set_accounting_runs(record_lines(&[
                ("Run 1", "4,120 tokens, completed"),
                ("Run 2", "9,880 tokens, completed"),
            ]));
            ui.window()
                .set_context_summary("18% of the context window".into());
            ui.window().set_settings_open(true);
            ui.window().set_settings_section("diagnostics".into());
        }),
        s("31-settings-tools", D, |ui| {
            modules(ui, "standard");
            ui.window()
                .set_tool_catalog(ModelRc::new(VecModel::from(vec![
                    argentum_ui::ToolCatalogView {
                        id: "ui_update".into(),
                        capability: "tool.ui-update".into(),
                        title: "Change interface".into(),
                        summary:
                            "Change modules, layout, or theme. Each change waits for one approval."
                                .into(),
                        risk: "Mutation".into(),
                        requires_approval: true,
                    },
                ])));
            ui.window().set_settings_open(true);
            ui.window().set_settings_section("tools".into());
        }),
        s("32-goal-panel", (1440, 900), |ui| {
            modules(ui, "full");
            conversation(ui);
            ui.window().set_goal_id("goal-1".into());
            open_panel(ui, SurfaceId::Goal);
        }),
        s("33-session-actions", D, |ui| {
            modules(ui, "standard");
            conversation(ui);
            ui.window().set_session_actions_open(true);
            ui.window()
                .set_session_actions_title("Why does clippy flag this?".into());
            ui.window()
                .set_session_rename_draft("Why does clippy flag this?".into());
        }),
        s("14-approval", D, |ui| {
            modules(ui, "standard");
            conversation(ui);
            ui.window().set_approval_open(true);
            ui.window()
                .set_approval_action("Change the interface".into());
            ui.window()
                .set_approval_target("theme to ember, text size to 110%, turn on browser".into());
            ui.window().set_approval_reason("The agent asked to change how Argentum looks. This grants no permissions, and you can undo it afterwards.".into());
        }),
        s("15-provider-menu", D, |ui| {
            modules(ui, "standard");
            conversation(ui);
            ui.window().set_provider_menu_open(true);
        }),
        s("16-palette", D, |ui| {
            modules(ui, "full");
            conversation(ui);
            ui.window().set_command_palette_open(true);
            ui.window().global::<App>().invoke_palette_query("".into());
        }),
        s("17-toast", D, |ui| {
            modules(ui, "standard");
            conversation(ui);
            ui.window()
                .set_toast_text("Agent changed: theme to ember, text size to 110%".into());
            ui.window().set_toast_undo_token("{}".into());
        }),
        s("18-tablet-panel", (820, 1000), |ui| {
            modules(ui, "full");
            conversation(ui);
            open_panel(ui, SurfaceId::GitDiff);
            git(ui);
        }),
        s("19-phone-430", (430, 880), |ui| {
            modules(ui, "standard");
            conversation(ui);
        }),
        s("20-phone-360-empty", (360, 720), |ui| {
            modules(ui, "standard")
        }),
        s("21-phone-360-settings", (360, 720), |ui| {
            modules(ui, "standard");
            ui.window().set_settings_open(true);
            ui.window().set_settings_section("appearance".into());
        }),
        s("22-phone-430-drawer", (430, 880), |ui| {
            modules(ui, "standard");
            conversation(ui);
            ui.window().set_drawer_open(true);
        }),
        s("23-theme-silver", D, |ui| {
            modules(ui, "standard");
            conversation(ui);
            workbench::apply_preferences(
                ui.window(),
                &UiPreferences {
                    preset: "silver".into(),
                    first_run_complete: true,
                    ..UiPreferences::default()
                },
            );
        }),
        s("24-theme-ember-spacious", D, |ui| {
            modules(ui, "standard");
            conversation(ui);
            workbench::apply_preferences(
                ui.window(),
                &UiPreferences {
                    preset: "ember".into(),
                    density: argentum_domain::UiDensity::Spacious,
                    font_percent: 115,
                    first_run_complete: true,
                    ..UiPreferences::default()
                },
            );
        }),
    ]
}

fn save(buffer: &slint::SharedPixelBuffer<slint::Rgba8Pixel>, path: &Path) {
    let file = File::create(path).expect("create png");
    let mut encoder = png::Encoder::new(BufWriter::new(file), buffer.width(), buffer.height());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer
        .write_image_data(buffer.as_bytes())
        .expect("png data");
}

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().expect("usage: ui_snapshot OUT_DIR [FILTER]");
    let filter = args.next().unwrap_or_default();
    std::fs::create_dir_all(&out).expect("output directory");

    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(Headless(window.clone()))).expect("platform");

    for scene in scenes() {
        if !filter.is_empty() && !scene.name.contains(&filter) {
            continue;
        }
        let ui = UiHandle::new().expect("window");
        argentum_ui::connect_commands(ui.window(), |_| {});
        base(&ui);
        (scene.setup)(&ui);
        ui.window()
            .window()
            .set_size(PhysicalSize::new(scene.size.0, scene.size.1));
        ui.show().expect("show");
        // Let bindings settle, layout resolve, and timers fire before capturing.
        for _ in 0..4 {
            slint::platform::update_timers_and_animations();
            let _ = ui.window().window().take_snapshot();
        }
        let image = ui.window().window().take_snapshot().expect("snapshot");
        let path = Path::new(&out).join(format!("{}.png", scene.name));
        save(&image, &path);
        println!("{}", path.display());
        ui.hide().expect("hide");
    }
}
