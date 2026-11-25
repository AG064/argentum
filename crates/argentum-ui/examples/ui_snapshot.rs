// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

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
use slint::{ComponentHandle, Model, ModelRc, PhysicalSize, SharedString, VecModel};

struct Headless(Rc<MinimalSoftwareWindow>);

impl Platform for Headless {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, slint::PlatformError> {
        Ok(self.0.clone())
    }
}

const REPLY: &str = "I found the problem. The session store opens a new transaction per row, so a large import holds the write lock for **minutes**.\n\n## What I changed\n\n1. Batched the inserts in `import_sessions`.\n2. Added a `busy_timeout` so readers wait instead of failing.\n3. Kept the public API the same.\n\n```rust\nlet tx = connection.transaction()?;\nfor row in rows {\n    insert_session(&tx, &row)?;\n}\ntx.commit()?;\n```\n\n> The existing tests pass, and the import went from 94 s to 2.1 s on the sample file.\n\n| Step | Before | After |\n| --- | --- | --- |\n| Import | 94 s | 2.1 s |\n| Lookup | 3 ms | 3 ms |\n\nCompared with the other options:\n\n| Option | Where it runs | Needs approval | Notes |\n| :--- | :---: | ---: | --- |\n| `read_file` | Workspace | No | Reads only files inside the resolved workspace and never follows links out of it. |\n| `write_file` | Workspace | Yes | Shows the diff before anything is written to disk. |\n| `shell` | Your machine | Yes | Each command is approved once, for the session, or never. |\n\nSee the [SQLite docs](https://sqlite.org/lang_transaction.html) for why this matters.";

const DIFF: &str = "diff --git a/src/store.rs b/src/store.rs\n--- a/src/store.rs\n+++ b/src/store.rs\n@@ -41,7 +41,9 @@ pub fn import_sessions(rows: &[Row]) -> Result<()> {\n     let connection = open()?;\n-    for row in rows {\n-        insert_session(&connection, row)?;\n+    let tx = connection.transaction()?;\n+    for row in rows {\n+        insert_session(&tx, row)?;\n     }\n+    tx.commit()?;\n     Ok(())\n }\n";

/// A reply whose table has a column of sentences, as models often write.
const LONG_NOTES_TABLE: &str = "| Language | Year | Typing | Notes |
| --- | --- | --- | --- |
| Python | 1991 | Dynamic | Designed around readability, so it uses indentation instead of braces. Its standard library and package ecosystem are the real reason it dominates data work, though the global interpreter lock limits threads. |
| Rust | 2015 | Static | Memory safety without a garbage collector, checked at compile time. The borrow checker takes time to learn, and compile times are long, but whole classes of bugs disappear. |
| Go | 2009 | Static | Small language with fast builds and simple concurrency through goroutines. Generics arrived late, in 1.18, and error handling stays verbose. |";

fn conversation(ui: &UiHandle) {
    conversation_with(ui, REPLY);
}

fn conversation_with(ui: &UiHandle, reply: &str) {
    conversation_full(
        ui,
        reply,
        "The trace shows one fsync per row, which points at autocommit.",
    );
}

fn conversation_full(ui: &UiHandle, reply: &str, reasoning: &str) {
    conversation_with_tools(ui, reply, reasoning, &[]);
}

/// A finished reply whose tool calls ran with these ids, tools and lines.
fn conversation_with_tools(
    ui: &UiHandle,
    reply: &str,
    reasoning: &str,
    tools: &[(uuid::Uuid, &str, &str)],
) {
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
        text: reply.into(),
        reasoning: reasoning.into(),
        usage: Some(ModelUsage {
            input_tokens: 41_200,
            output_tokens: 610,
            total_tokens: 41_810,
            context_window_tokens: Some(200_000),
            reasoning_tokens: None,
            cached_input_tokens: None,
            turn_total: None,
        }),
        profile_id: "lm-studio".into(),
        model: "qwen3-coder-next".into(),
        created_at: id + time::Duration::seconds(125),
        ..user.clone()
    };
    let traces = tools
        .iter()
        .map(|(id, tool, line)| argentum_domain::ToolTrace {
            id: *id,
            run_id: assistant.run_id,
            tool_id: (*tool).into(),
            summary: (*line).into(),
            result: argentum_domain::ToolResultState::Succeeded,
            duration_ms: None,
        })
        .collect::<Vec<_>>();
    let window = ui.window();
    window.set_conversation_messages(argentum_ui::conversation_message_model_with_traces(
        &[user, assistant],
        &traces,
    ));
    window.set_session_title("Slow session import".into());
    window.set_usage_reported(true);
    window.set_usage_total_tokens(41_810);
    window.global::<App>().set_usage_total_label("41.8k".into());
    window
        .global::<App>()
        .set_context_window_label("200k".into());
    window
        .global::<App>()
        .set_session_tokens_label("83.6k".into());
    window.set_context_window_reported(true);
    window.set_context_window_tokens(200_000);
}

fn base(ui: &UiHandle) {
    let window = ui.window();
    window.set_update_current("0.1.0".into());
    window.set_project_name(workbench::display_project_name("argentum").into());
    window.set_workspace_label("C:\\Projects\\argentum".into());
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
            context_window_label: "200k".into(),
            context_window_reported: true,
            selected: true,
            thinking_levels: Default::default(),
        },
        argentum_ui::ProviderModelView {
            id: "gemma-4-large".into(),
            label: "gemma-4-large".into(),
            context_window_tokens: 0,
            context_window_label: Default::default(),
            context_window_reported: false,
            selected: false,
            thinking_levels: Default::default(),
        },
    ])));
    window.set_session_items(ModelRc::new(VecModel::from(
        [
            ("Slow session import", true, "working", 3),
            ("Add retry to the provider probe", false, "waiting", 1),
            ("Write release notes for 0.1, with every change since the first preview and the known limits", false, "unread", 12),
            ("Why does clippy flag this?", false, "", 0),
        ]
        .map(|(title, active, status, files)| argentum_ui::SessionItemView {
            id: title.into(),
            title: title.into(),
            detail: "".into(),
            active,
            archived: false,
            pinned: false,
            status: status.into(),
            files_changed: files,
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

fn goal(
    ui: &UiHandle,
    lifecycle: argentum_domain::GoalLifecycle,
    tokens_used: u64,
    token_budget: Option<u64>,
) {
    let now = argentum_domain::now();
    let session_id = uuid::Uuid::new_v4();
    ui.window()
        .set_active_session_id(session_id.to_string().into());
    ui.apply_event(&argentum_domain::AppEvent::GoalSnapshotLoaded {
        session_id,
        goal: Some(Box::new(argentum_domain::Goal {
            id: uuid::Uuid::new_v4(),
            project_id: uuid::Uuid::new_v4(),
            session_id,
            objective: "Make large session imports finish in under five seconds".into(),
            lifecycle,
            token_budget,
            tool_budget: None,
            time_budget_seconds: None,
            tokens_used,
            tools_used: 4,
            iteration: 3,
            next_action: "Run the next task".into(),
            verification_history: Vec::new(),
            created_at: now,
            updated_at: now,
            accounting: Some(argentum_domain::goal_accounting::GoalAccounting::new(false)),
            definition_revision: uuid::Uuid::new_v4(),
            acceptance: None,
            owner_attestation: None,
            completion: None,
            completion_report: None,
        })),
    });
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
                compaction_tokens: 0,
                month_tokens: 41_800,
                monthly_limit: Some(50_000),
            },
            ProviderUsageRow {
                profile_id: "deepseek".into(),
                turns: 12,
                input_tokens: 210_000,
                output_tokens: 18_400,
                total_tokens: 228_400,
                compaction_tokens: 0,
                month_tokens: 2_100,
                monthly_limit: None,
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

/// The model marketplace in Settings > Providers, with fixture search results.
fn marketplace(ui: &UiHandle, state: &str) {
    let app = ui.window().global::<App>();
    app.set_addon_supported(true);
    app.set_addon_state("installed".into());
    app.set_addon_detail("Installed: llama.cpp runtime with Qwen3.5 4B Q4_K_M.".into());
    app.set_market_supported(true);
    app.set_market_query("qwen coder".into());
    app.set_market_hardware("This computer: 34 GB of system memory, Example GPU with 8.6 GB of GPU memory. Read locally and never sent. Fit marks use the launcher's own memory rules at a 8192-token context, with an estimated KV cache; llama.cpp measures again at launch.".into());
    app.set_market_state(state.into());
    app.set_market_detail(
        "3 GGUF repositories, most downloaded first. Open one to see its files.".into(),
    );
    app.set_market_results(ModelRc::new(VecModel::from(
        [
            ("unsloth/Qwen3-Coder-30B-A3B-Instruct-GGUF", "412.3k downloads, 1.1k likes, apache-2.0 license"),
            ("bartowski/Qwen2.5-Coder-7B-Instruct-GGUF", "98.4k downloads, 310 likes, apache-2.0 license"),
            ("tester/Gated-Coder-GGUF", "1.2k downloads, 12 likes, license not stated, gated: needs a token and accepted terms"),
        ]
        .iter()
        .map(|(id, detail)| argentum_ui::MarketRepoView {
            id: (*id).into(),
            detail: (*detail).into(),
        })
        .collect::<Vec<_>>(),
    )));
    app.set_market_repo_title("bartowski/Qwen2.5-Coder-7B-Instruct-GGUF".into());
    app.set_market_repo_detail(
        "apache-2.0 license, context length 32768 tokens, 7.6B parameters, qwen2 architecture, commit 0123456789ab".into(),
    );
    app.set_market_files(ModelRc::new(VecModel::from(
        [
            ("Q4_K_M, recommended for this computer", "4.68 GB, Qwen2.5-Coder-7B-Instruct-Q4_K_M.gguf"),
            ("Q8_0", "8.10 GB, Qwen2.5-Coder-7B-Instruct-Q8_0.gguf"),
            ("F16, 2 parts", "15.24 GB, Qwen2.5-Coder-7B-Instruct-F16-00001-of-00002.gguf, Qwen2.5-Coder-7B-Instruct-F16-00002-of-00002.gguf"),
        ]
        .iter()
        .enumerate()
        .map(|(index, (label, detail))| argentum_ui::MarketFileView {
            index: index as i32,
            label: (*label).into(),
            detail: (*detail).into(),
            fit: ["Fits fully on GPU", "Fits with offload to system memory", "Will not fit"][index].into(),
            fit_tone: ["ok", "secondary", "danger"][index].into(),
        })
        .collect::<Vec<_>>(),
    )));
    if state != "results" {
        app.set_market_detail("".into());
    }
    app.set_market_can_download(true);
    app.set_market_confirm_text(
        "Q4_K_M from huggingface.co/bartowski/Qwen2.5-Coder-7B-Instruct-GGUF at commit 0123456789ab.
Qwen2.5-Coder-7B-Instruct-Q4_K_M.gguf: 4.68 GB, checked against the SHA-256 Hugging Face publishes (65b8fcd92af6b4fe...).
License: apache-2.0. Read it on the model page before you use the model.
Needs 4.68 GB on disk, 92.10 GB is free.
The files go to the model folder in your Argentum data folder. A file that fails its check is deleted. Nothing runs until you start the local model, which asks for approval."
            .into(),
    );
}

/// The desktop first start before any project exists.
fn first_run_project(ui: &UiHandle) {
    ui.window().set_first_run_open(true);
    ui.window().set_first_run_stage("project".into());
    let app = ui.window().global::<App>();
    app.set_workspace_picker_available(true);
    app.set_default_workspace_path("C:\\Users\\you\\Documents\\Argentum".into());
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
        s("00-first-run-project", D, |ui| {
            first_run_project(ui);
        }),
        s("00-first-run-project-refused", D, |ui| {
            first_run_project(ui);
            let app = ui.window().global::<App>();
            app.set_workspace_draft("D:\\no such folder".into());
            app.set_workspace_notice("That folder cannot be opened.".into());
        }),
        s("00-first-run-project-narrow", (420, 820), |ui| {
            first_run_project(ui);
            ui.window()
                .global::<App>()
                .set_workspace_draft("C:\\Users\\you\\projects\\shop".into());
        }),
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
        s("47-full-activity-off", (1440, 900), |ui| {
            modules(ui, "full");
            conversation(ui);
            open_panel(ui, SurfaceId::Activity);
            ui.window().set_activity_module_enabled(false);
        }),
        s("48-running-queue", D, |ui| {
            modules(ui, "standard");
            conversation(ui);
            ui.window().set_running(true);
            ui.window().set_run_state("Running".into());
            ui.window().set_draft("Also update the changelog".into());
            ui.window()
                .global::<App>()
                .set_queued_prompts(ModelRc::new(VecModel::from(vec![
                    SharedString::from("Run the tests again"),
                    SharedString::from("Summarize what changed"),
                ])));
            ui.window()
                .global::<App>()
                .set_steer_notice("Your note was added to the run.".into());
        }),
        s("49-phone-running", (360, 760), |ui| {
            modules(ui, "standard");
            conversation(ui);
            ui.window().set_running(true);
            ui.window().set_run_state("Running".into());
        }),
        s("50-phone-everything-panel", (360, 760), |ui| {
            ui.window().set_touch_device(true);
            modules(ui, "full");
            conversation(ui);
            open_panel(ui, SurfaceId::Changes);
        }),
        s("51-phone-long-notes-table", (360, 760), |ui| {
            ui.window().set_touch_device(true);
            modules(ui, "standard");
            conversation_with(ui, LONG_NOTES_TABLE);
        }),
        s("52-phone-sent-messages", (360, 760), |ui| {
            ui.window().set_touch_device(true);
            modules(ui, "standard");
            conversation(ui);
            ui.window().set_running(true);
            ui.window().set_run_state("Running".into());
            ui.window()
                .set_user_prompt("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into());
            ui.window()
                .global::<App>()
                .set_live_steers(ModelRc::new(VecModel::from(vec![
                    argentum_ui::SteerNoteView {
                        run_id: "".into(),
                        entry_id: "".into(),
                        text: "Keep it short".into(),
                        note: "Steering. Added to the run.".into(),
                    },
                ])));
        }),
        s("53-desktop-sent-messages", D, |ui| {
            modules(ui, "standard");
            conversation(ui);
            ui.window().set_running(true);
            ui.window().set_run_state("Running".into());
            ui.window().set_user_prompt(
                "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
            );
        }),
        s("54-phone-reply-steps", (360, 760), |ui| {
            ui.window().set_touch_device(true);
            modules(ui, "standard");
            use argentum_domain::reply_steps::{step_start, thought_mark, tool_mark};
            let first = "I will check the import loop first.";
            let reply =
                format!("{first}The loop opens one transaction per row. Batching them fixes it.");
            let ids = [
                uuid::Uuid::new_v4(),
                uuid::Uuid::new_v4(),
                uuid::Uuid::new_v4(),
            ];
            let reasoning = format!(
                "The user reports slow imports. Look at the store first.{}{}{}{}Profile one import.{}{}{}The trace shows one fsync per row.{}",
                thought_mark(6_000),
                tool_mark(first.len(), &ids[0].to_string()),
                tool_mark(first.len(), &ids[1].to_string()),
                step_start(first.len()),
                thought_mark(3_000),
                tool_mark(first.len(), &ids[2].to_string()),
                step_start(first.len()),
                thought_mark(9_000),
            );
            conversation_with_tools(
                ui,
                &reply,
                &reasoning,
                &[
                    (ids[0], "read_text", "Read `store/import.rs`"),
                    (ids[1], "read_text", "Read `store/session.rs`"),
                    (
                        ids[2],
                        "execute_shell",
                        "Ran `argentum import big.json --trace`",
                    ),
                ],
            );
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
        s("10b-settings-marketplace-results", D, |ui| {
            modules(ui, "standard");
            marketplace(ui, "results");
            ui.window().set_settings_open(true);
        }),
        s("10c-settings-marketplace-repo", D, |ui| {
            modules(ui, "standard");
            marketplace(ui, "repo");
            ui.window().set_settings_open(true);
        }),
        s("10d-settings-marketplace-confirm", D, |ui| {
            modules(ui, "standard");
            marketplace(ui, "confirm");
            ui.window().set_settings_open(true);
        }),
        s("11-settings-modules", D, |ui| {
            modules(ui, "standard");
            ui.window().set_settings_open(true);
            ui.window().set_settings_section("modules".into());
        }),
        s("11b-settings-modules-tall", (1280, 1500), |ui| {
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
        s("61-settings-import", (1440, 900), |ui| {
            use argentum_domain::imports::{ImportItemView, ImportOrigin};
            modules(ui, "full");
            let item =
                |id: &str, kind: &str, summary: &str, destination: Option<&str>| ImportItemView {
                    id: id.into(),
                    kind: kind.into(),
                    name: id.rsplit(':').next().unwrap_or(id).into(),
                    origin: ImportOrigin::ClaudeCodeProject,
                    summary: summary.into(),
                    source_path: ".claude".into(),
                    destination: destination.map(Into::into),
                    dropped: if kind == "skill" {
                        vec!["the allowed-tools field".into()]
                    } else {
                        vec![]
                    },
                    conflict: None,
                    importable: kind != "instructions",
                };
            ui.apply_event(&argentum_domain::AppEvent::ImportScanSnapshot {
                items: vec![
                    item(
                        "instructions:claude_code_project:CLAUDE.md",
                        "instructions",
                        "Instructions CLAUDE.md: loaded with AGENTS.md, nothing to import",
                        None,
                    ),
                    item(
                        "mcp:claude_code_project:github",
                        "mcp_server",
                        "MCP server github: npx -y @modelcontextprotocol/server-github",
                        None,
                    ),
                    item(
                        "skill:claude_code_project:release-review",
                        "skill",
                        "Skill release-review: Check a release before tagging",
                        Some(".agents/skills/release-review/SKILL.md"),
                    ),
                ],
                problems: vec![],
                results: vec![],
            });
            ui.window().set_settings_open(true);
            ui.window().set_settings_section("tools".into());
        }),
        s("62-settings-memory", (1440, 1700), |ui| {
            use argentum_domain::memory::{MemoryAuthor, MemoryNote};
            modules(ui, "full");
            let note = |id: i64, text: &str, author: MemoryAuthor| MemoryNote {
                id,
                text: text.into(),
                author,
                created_at: 0,
                updated_at: 0,
            };
            ui.apply_event(&argentum_domain::AppEvent::MemorySnapshot {
                notes: vec![
                    note(
                        2,
                        "Run cargo test -p argentum-store before a commit.",
                        MemoryAuthor::Agent,
                    ),
                    note(
                        1,
                        "Plain language in docs, no em dashes.",
                        MemoryAuthor::Owner,
                    ),
                ],
                notice: None,
                refused: false,
            });
            ui.window().set_settings_open(true);
            ui.window().set_settings_section("workspace".into());
        }),
        s("32-goal-panel", (1440, 900), |ui| {
            modules(ui, "full");
            conversation(ui);
            ui.window().set_goal_id("goal-1".into());
            open_panel(ui, SurfaceId::Goal);
        }),
        s("34-goal-card-empty", D, |ui| {
            modules(ui, "full");
            conversation(ui);
        }),
        s("35-goal-card-active", D, |ui| {
            modules(ui, "full");
            conversation(ui);
            goal(
                ui,
                argentum_domain::GoalLifecycle::Active,
                12_400,
                Some(50_000),
            );
        }),
        s("36-goal-card-paused", D, |ui| {
            modules(ui, "full");
            conversation(ui);
            goal(ui, argentum_domain::GoalLifecycle::Paused, 12_400, None);
        }),
        s("37-goal-card-phone", (360, 760), |ui| {
            modules(ui, "full");
            conversation(ui);
            goal(
                ui,
                argentum_domain::GoalLifecycle::Active,
                12_400,
                Some(50_000),
            );
        }),
        s("38-goal-card-phone-empty", (360, 760), |ui| {
            modules(ui, "full");
            conversation(ui);
        }),
        s("39-goal-card-report", (360, 760), |ui| {
            modules(ui, "full");
            conversation(ui);
            goal(ui, argentum_domain::GoalLifecycle::Active, 12_400, None);
            ui.window().set_goal_report_pending(true);
            ui.window().set_goal_report_status("complete".into());
            ui.window().set_goal_report_summary(
                "Batched the inserts and added a busy timeout. The sample import now takes 2.1 s."
                    .into(),
            );
        }),
        s("39b-goal-card-needs-input", (360, 760), |ui| {
            modules(ui, "full");
            conversation(ui);
            goal(ui, argentum_domain::GoalLifecycle::Active, 12_400, None);
            ui.window().set_goal_report_pending(true);
            ui.window().set_goal_report_status("needs_input".into());
            ui.window().set_goal_report_summary(
                "The import has two schemas. Should the old one keep working?".into(),
            );
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
            ui.window().set_touch_device(true);
            modules(ui, "standard");
            conversation(ui);
        }),
        s("20-phone-360-empty", (360, 720), |ui| {
            ui.window().set_touch_device(true);
            modules(ui, "standard")
        }),
        s("40-phone-360-table", (360, 760), |ui| {
            ui.window().set_touch_device(true);
            modules(ui, "standard");
            conversation(ui);
        }),
        s("41-phone-360-palette", (360, 760), |ui| {
            ui.window().set_touch_device(true);
            modules(ui, "standard");
            conversation(ui);
            ui.window().set_command_palette_open(true);
            ui.window().global::<App>().invoke_palette_query("".into());
        }),
        s("42-phone-360-settings-verification", (360, 760), |ui| {
            ui.window().set_touch_device(true);
            modules(ui, "standard");
            ui.window().set_settings_open(true);
            ui.window().set_settings_section("verification".into());
        }),
        s("43-phone-360-settings-modules", (360, 760), |ui| {
            ui.window().set_touch_device(true);
            modules(ui, "standard");
            ui.window().set_settings_open(true);
            ui.window().set_settings_section("modules".into());
        }),
        s("44-phone-360-approval-scopes", (360, 760), |ui| {
            ui.window().set_touch_device(true);
            modules(ui, "standard");
            conversation(ui);
            ui.window().set_approval_open(true);
            ui.window().set_approval_action("Run a command".into());
            ui.window()
                .set_approval_target("cargo test --workspace".into());
            ui.window().set_approval_scope("once".into());
            ui.window()
                .set_approval_scope_choices(ModelRc::new(VecModel::from(vec![
                    argentum_ui::ApprovalScopeChoiceView {
                        id: "once".into(),
                        label: "Once".into(),
                    },
                    argentum_ui::ApprovalScopeChoiceView {
                        id: "session".into(),
                        label: "This session".into(),
                    },
                ])));
        }),
        s("45-context-meter-amber", D, |ui| {
            modules(ui, "standard");
            conversation(ui);
            ui.window().set_usage_total_tokens(150_000);
            ui.window()
                .global::<App>()
                .set_usage_total_label("150k".into());
        }),
        s("46-context-meter-red", D, |ui| {
            modules(ui, "standard");
            conversation(ui);
            ui.window().set_usage_total_tokens(190_000);
            ui.window()
                .global::<App>()
                .set_usage_total_label("190k".into());
        }),
        s("21-phone-360-settings", (360, 720), |ui| {
            ui.window().set_touch_device(true);
            modules(ui, "standard");
            ui.window().set_settings_open(true);
            ui.window().set_settings_section("appearance".into());
        }),
        s("22-phone-430-drawer", (430, 880), |ui| {
            ui.window().set_touch_device(true);
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
        s("55-files-panel", D, |ui| {
            modules(ui, "full");
            conversation(ui);
            open_panel(ui, SurfaceId::Files);
            files(ui);
        }),
        s("56-question-card", D, |ui| {
            modules(ui, "standard");
            conversation(ui);
            question(ui);
        }),
        s("57-phone-question-card", (360, 760), |ui| {
            ui.window().set_touch_device(true);
            modules(ui, "standard");
            conversation(ui);
            question(ui);
        }),
    ]
}

fn files(ui: &UiHandle) {
    let entry = |path: &str, directory: bool, size_bytes: u64| argentum_domain::WorkspaceEntry {
        path: path.into(),
        name: path.rsplit('/').next().unwrap_or(path).into(),
        directory,
        size_bytes,
    };
    ui.apply_event(&argentum_domain::AppEvent::WorkspaceDirectoryLoaded {
        path: "src".into(),
        entries: vec![
            entry("src/store", true, 0),
            entry("src/ui", true, 0),
            entry("src/import.rs", false, 4_212),
            entry("src/lib.rs", false, 812),
            entry("src/main.rs", false, 1_390),
        ],
        truncated: false,
    });
    let app = ui.window().global::<App>();
    app.set_change_files(ModelRc::new(VecModel::from(vec![
        argentum_ui::FileChangeView {
            id: "change".into(),
            path: "src/import.rs".into(),
            kind: "Modified".into(),
            revision: 1,
            selected: false,
            restore_available: true,
            restore_reason: SharedString::default(),
        },
    ])));
    app.set_files_preview_path("src/import.rs".into());
    ui.apply_event(&argentum_domain::AppEvent::WorkspaceFilePreviewLoaded {
        path: "src/import.rs".into(),
        text: Some(
            "pub fn import_sessions(rows: &[Row]) -> Result<()> {\n    let connection = open()?;\n    let tx = connection.transaction()?;\n    for row in rows {\n        insert_session(&tx, row)?;\n    }\n    tx.commit()?;\n    Ok(())\n}\n"
                .into(),
        ),
        reason: String::new(),
    });
}

fn question(ui: &UiHandle) {
    let app = ui.window().global::<App>();
    app.set_question_text(
        "Should the import keep sessions that fail to parse, or skip them and list them at the end?"
            .into(),
    );
    app.set_question_choices(ModelRc::new(VecModel::from(vec![
        SharedString::from("Keep them"),
        SharedString::from("Skip and list them"),
    ])));
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
        // The marketplace sits at the bottom of Settings > Providers, so these
        // scenes scroll the page with the mouse wheel before capturing.
        if scene.name.contains("marketplace") {
            let (width, height) = (scene.size.0 as f32, scene.size.1 as f32);
            for _ in 0..12 {
                ui.window().window().dispatch_event(
                    slint::platform::WindowEvent::PointerScrolled {
                        position: slint::LogicalPosition::new(width * 0.6, height * 0.5),
                        delta_x: 0.0,
                        delta_y: -400.0,
                    },
                );
                slint::platform::update_timers_and_animations();
                let _ = ui.window().window().take_snapshot();
            }
        }
        let image = ui.window().window().take_snapshot().expect("snapshot");
        let path = Path::new(&out).join(format!("{}.png", scene.name));
        save(&image, &path);
        println!("{}", path.display());
        ui.hide().expect("hide");
    }
}
