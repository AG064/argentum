//! Development-only window for Slint live preview.
//!
//! This binary never opens a workspace, provider, credential store, or network
//! connection. It renders the generated `MainWindow` with local showcase data.
//!
//! Usage: `ui-preview [empty|conversation|approval]`

use slint::{ComponentHandle, ModelRc, VecModel};

use argentum_ui::{
    ActivityItemView, ConversationMessageView, FileChangeView, InspectorModeView, MainWindow,
    PlanStepView, SessionItemView,
};

fn rows<T: Clone + 'static>(items: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(items))
}

fn sessions(active: bool) -> ModelRc<SessionItemView> {
    rows(vec![
        SessionItemView {
            id: "session-release".into(),
            title: "Harden the release gate".into(),
            detail: "2 changes / 3 checks".into(),
            active,
        },
        SessionItemView {
            id: "session-render".into(),
            title: "Trace the rendering path".into(),
            detail: "Yesterday".into(),
            active: false,
        },
        SessionItemView {
            id: "session-deps".into(),
            title: "Review dependency bump".into(),
            detail: "Oct 29".into(),
            active: false,
        },
    ])
}

fn inspector_modes() -> ModelRc<InspectorModeView> {
    rows(vec![
        InspectorModeView {
            id: "changes".into(),
            label: "Changes".into(),
            icon: "changes".into(),
            detail: "Review file changes and diffs".into(),
        },
        InspectorModeView {
            id: "terminal".into(),
            label: "Tools".into(),
            icon: "terminal".into(),
            detail: "Inspect tool activity".into(),
        },
        InspectorModeView {
            id: "activity".into(),
            label: "Activity".into(),
            icon: "activity".into(),
            detail: "Trace the run timeline".into(),
        },
        InspectorModeView {
            id: "trajectory".into(),
            label: "Trajectory".into(),
            icon: "timeline".into(),
            detail: "Durable execution records".into(),
        },
    ])
}

fn conversation() -> ModelRc<ConversationMessageView> {
    rows(vec![
        ConversationMessageView {
            role: "user".into(),
            text: "Make the main window feel like a product, not a control panel. Keep the bridge intact."
                .into(),
            reasoning: "".into(),
            reasoning_available: false,
            provider_label: "".into(),
            model: "".into(),
            usage_reported: false,
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            context_window_reported: false,
            context_window_tokens: 0,
            state: "complete".into(),
        },
        ConversationMessageView {
            role: "assistant".into(),
            text: "I replaced the visual tree with a showcase shell: a full-height navigation rail, an editorial hero for the empty state, a conversation surface with room to breathe, and an inspector that keeps changes, tools, verification, and activity in one place. The public MainWindow contract is unchanged."
                .into(),
            reasoning: "The previous shell was dense. The new composition leads with typography and one signal color."
                .into(),
            reasoning_available: true,
            provider_label: "LM Studio".into(),
            model: "qwen3-coder-30b".into(),
            usage_reported: true,
            input_tokens: 2480,
            output_tokens: 613,
            total_tokens: 3093,
            context_window_reported: true,
            context_window_tokens: 32768,
            state: "complete".into(),
        },
        ConversationMessageView {
            role: "user".into(),
            text: "What did you verify?".into(),
            reasoning: "".into(),
            reasoning_available: false,
            provider_label: "".into(),
            model: "".into(),
            usage_reported: false,
            input_tokens: 0,
            output_tokens: 0,
            total_tokens: 0,
            context_window_reported: false,
            context_window_tokens: 0,
            state: "complete".into(),
        },
        ConversationMessageView {
            role: "assistant".into(),
            text: "The preview binary builds with live-preview enabled, the real app bridge compiles against the same interface, and both empty and populated states were rendered and captured. Live reload was confirmed without restarting the preview process."
                .into(),
            reasoning: "".into(),
            reasoning_available: false,
            provider_label: "LM Studio".into(),
            model: "qwen3-coder-30b".into(),
            usage_reported: true,
            input_tokens: 3912,
            output_tokens: 814,
            total_tokens: 4726,
            context_window_reported: true,
            context_window_tokens: 32768,
            state: "complete".into(),
        },
    ])
}

fn changes() -> ModelRc<FileChangeView> {
    rows(vec![
        FileChangeView {
            id: "change-app".into(),
            path: "ui/app.slint".into(),
            kind: "modified".into(),
            revision: 3,
            selected: true,
            restore_available: false,
            restore_reason: "The file was replaced by an explicit UI rewrite.".into(),
        },
        FileChangeView {
            id: "change-tokens".into(),
            path: "ui/tokens.slint".into(),
            kind: "modified".into(),
            revision: 2,
            selected: false,
            restore_available: false,
            restore_reason: "The file was replaced by an explicit UI rewrite.".into(),
        },
    ])
}

fn activity() -> ModelRc<ActivityItemView> {
    rows(vec![
        ActivityItemView {
            title: "Task submitted".into(),
            detail: "Make the main window feel like a product.".into(),
            state: "complete".into(),
        },
        ActivityItemView {
            title: "Read the interface contract".into(),
            detail: "Preserved every MainWindow in-out property and callback.".into(),
            state: "complete".into(),
        },
        ActivityItemView {
            title: "Wrote the showcase shell".into(),
            detail: "Rail, hero, conversation, inspector, composer.".into(),
            state: "complete".into(),
        },
        ActivityItemView {
            title: "cargo check".into(),
            detail: "The Slint UI compiled without errors.".into(),
            state: "success".into(),
        },
        ActivityItemView {
            title: "Verification".into(),
            detail: "Two changes are ready for review.".into(),
            state: "attention".into(),
        },
    ])
}

fn plan() -> ModelRc<PlanStepView> {
    rows(vec![
        PlanStepView {
            index: 1,
            title: "Preserve the bridge".into(),
            detail: "MainWindow interface copied unchanged.".into(),
            state: "complete".into(),
        },
        PlanStepView {
            index: 2,
            title: "Build the shell".into(),
            detail: "Navigation, hero, conversation, inspector, composer.".into(),
            state: "complete".into(),
        },
        PlanStepView {
            index: 3,
            title: "Compile the real app".into(),
            detail: "cargo build -p argentum-app".into(),
            state: "complete".into(),
        },
        PlanStepView {
            index: 4,
            title: "Review visuals".into(),
            detail: "Capture empty and populated states.".into(),
            state: "complete".into(),
        },
    ])
}

fn configure(window: &MainWindow, mode: &str) {
    window.set_project_name("Argentum".into());
    window.set_workspace_label("workspace/argentum".into());
    window.set_provider_label("LM Studio".into());
    window.set_provider_model("qwen3-coder-30b".into());
    window.set_provider_connected(true);
    window.set_provider_probe_state("connected".into());
    window.set_inspector_modes(inspector_modes());
    window.set_inspector_collapsed(false);
    window.set_active_inspector("changes".into());
    window.set_reduced_motion(false);

    if mode == "empty" {
        window.set_session_title("New session".into());
        window.set_active_session_id("".into());
        window.set_session_items(sessions(false));
        window.set_run_state("Ready".into());
        window.set_running(false);
        window.set_change_summary("No files changed".into());
        window.set_verification_state("pending".into());
        window.set_verification_summary("No automated checks were run".into());
        window.set_activity_summary("No activity yet".into());
        window.set_plan_summary("No stages yet".into());
        return;
    }

    let approval = mode == "approval";
    let inspector = match mode {
        "tools" => "terminal",
        "verification" => "plan",
        "activity" => "activity",
        _ => "changes",
    };
    window.set_active_inspector(inspector.into());
    window.set_session_title("Harden the release gate".into());
    window.set_active_session_id("session-release".into());
    window.set_session_items(sessions(true));
    window.set_conversation_messages(conversation());
    window.set_change_files(changes());
    window.set_changed_files(2);
    window.set_additions(38);
    window.set_removals(7);
    window.set_change_summary("2 files changed / +38 -7".into());
    window.set_selected_file_change_id("change-app".into());
    window.set_selected_change_path("ui/app.slint".into());
    window.set_selected_diff(
        "@@ -0,0 +1,9 @@\n+ export component MainWindow inherits Window {\n+     preferred-width: 1440px;\n+     preferred-height: 900px;\n+\n+     // Navigation rail, editorial hero, conversation, inspector.\n+ }\n".into(),
    );
    window.set_selected_diff_available(true);
    window.set_activity_items(activity());
    window.set_activity_summary("5 events / 2 changes".into());
    window.set_plan_steps(plan());
    window.set_plan_summary("4 stages".into());
    window.set_verification_state("passed".into());
    window.set_verification_summary("3 checks passed / 0 failed".into());

    if approval {
        window.set_run_state("Waiting for approval".into());
        window.set_running(true);
        window.set_active_run_id("run-preview".into());
        window.set_approval_open(true);
        window.set_approval_id("approval-preview".into());
        window.set_approval_action("Write file".into());
        window.set_approval_target("docs/UI-V2.md".into());
        window.set_approval_reason(
            "The run wants to create a release note inside the workspace.".into(),
        );
        window.set_approval_response_pending(false);
    } else {
        window.set_run_state("Ready".into());
        window.set_running(false);
    }
}

fn main() -> Result<(), slint::PlatformError> {
    let mode = std::env::args().nth(1).unwrap_or_else(|| "empty".into());
    let window = MainWindow::new()?;
    configure(&window, &mode);
    window.run()
}
