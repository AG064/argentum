use std::path::PathBuf;

use argentum_cli::onboarding::{
    GuidanceLevel, OnboardingSnapshot, OnboardingStep, SetupAction, SetupState,
};
use argentum_cli::{CommandHost, HostConfig};
use argentum_domain::{AppCommand, OnboardingCredentialState, ProviderKind, ProviderProfile};

#[test]
fn setup_state_requires_guidance_then_follows_the_declared_order() {
    let mut state = SetupState::new(OnboardingSnapshot::initial());

    assert_eq!(state.step(), OnboardingStep::Guidance);
    assert!(state
        .apply(SetupAction::SelectProvider {
            provider_id: "llama-cpp".into(),
        })
        .is_err());

    state
        .apply(SetupAction::SelectGuidance {
            level: GuidanceLevel::Guided,
        })
        .expect("guidance selection");
    assert_eq!(state.step(), OnboardingStep::Provider);

    state
        .apply(SetupAction::SelectProvider {
            provider_id: "llama-cpp".into(),
        })
        .expect("provider selection");
    assert_eq!(state.step(), OnboardingStep::Model);

    state
        .apply(SetupAction::SelectModel {
            provider_id: "llama-cpp".into(),
            model: "local-model".into(),
        })
        .expect("model selection");
    assert_eq!(state.step(), OnboardingStep::Workspace);

    state
        .apply(SetupAction::SelectWorkspace {
            workspace: PathBuf::from("A:\\workspace"),
        })
        .expect("workspace selection");
    assert_eq!(state.step(), OnboardingStep::Permission);

    state
        .apply(SetupAction::SelectPermission {
            profile_id: "confirm-before-changes".into(),
        })
        .expect("permission selection");
    assert_eq!(state.step(), OnboardingStep::Review);
    assert!(!state.snapshot().complete);

    state
        .apply(SetupAction::ConfirmReview)
        .expect("review confirmation");
    assert_eq!(state.step(), OnboardingStep::Complete);
    assert!(state.snapshot().complete);
}

#[test]
fn setup_state_can_restart_without_retaining_previous_selections() {
    let mut state = SetupState::new(OnboardingSnapshot::initial());
    state
        .apply(SetupAction::SelectGuidance {
            level: GuidanceLevel::Expert,
        })
        .expect("guidance selection");
    state.apply(SetupAction::Begin).expect("restart setup");

    assert_eq!(state.step(), OnboardingStep::Guidance);
    assert_eq!(state.snapshot().guidance, None);
    assert_eq!(state.snapshot().provider_id, None);
    assert!(!state.snapshot().complete);
}

#[tokio::test]
async fn onboarding_commands_persist_completion_and_reload_after_restart() {
    let workspace = tempfile::tempdir().expect("workspace");
    let database = tempfile::tempdir().expect("database directory");
    let database_path = database.path().join("argentum.db");
    let mut config = HostConfig::in_memory(workspace.path());
    config.database = Some(database_path);

    let host = CommandHost::start(config.clone()).expect("host");
    assert_eq!(
        host.onboarding_snapshot().expect("initial onboarding").step,
        OnboardingStep::Guidance
    );
    let client = host.client();

    client
        .dispatch(AppCommand::SaveProviderProfile {
            profile: ProviderProfile {
                id: "setup-fixture".into(),
                label: "Setup fixture".into(),
                kind: ProviderKind::OpenAiCompatible,
                endpoint: "http://127.0.0.1:9/v1/".into(),
                model: "local-model".into(),
                selected: false,
            },
        })
        .await
        .unwrap();

    client
        .dispatch(AppCommand::SetOnboardingGuidance {
            guidance: GuidanceLevel::Expert,
        })
        .await
        .expect("guidance");
    client
        .dispatch(AppCommand::SetOnboardingProvider {
            provider_id: "setup-fixture".into(),
        })
        .await
        .expect("provider");
    client
        .dispatch(AppCommand::SetOnboardingModel {
            provider_id: "setup-fixture".into(),
            model: "local-model".into(),
        })
        .await
        .expect("model");
    client
        .dispatch(AppCommand::SetOnboardingWorkspace {
            workspace: workspace.path().to_path_buf(),
        })
        .await
        .expect("workspace");
    client
        .dispatch(AppCommand::SetOnboardingPermission {
            profile_id: "confirm-before-changes".into(),
        })
        .await
        .expect("permission");
    assert!(client
        .dispatch(AppCommand::CompleteOnboarding)
        .await
        .unwrap_err()
        .to_string()
        .contains("context_budget_invalid"));
    assert!(!host.onboarding_snapshot().unwrap().complete);
    host.declare_model_budget("setup-fixture", [204800, 196608, 4096, 4096, 1024])
        .unwrap();
    client
        .dispatch(AppCommand::CompleteOnboarding)
        .await
        .expect("complete onboarding");

    let completed = host.onboarding_snapshot().expect("completed onboarding");
    assert_eq!(completed.step, OnboardingStep::Complete);
    assert!(completed.complete);
    let serialized = serde_json::to_string(&completed).expect("serialize onboarding");
    assert!(serialized.contains("credential_state"));
    assert!(!serialized.contains("api_key"));
    assert!(!serialized.contains("secret"));

    drop(host);
    let reopened = CommandHost::start(config).expect("reopened host");
    let restored = reopened.onboarding_snapshot().expect("restored onboarding");
    assert_eq!(restored.step, OnboardingStep::Complete);
    assert!(restored.complete);
    assert!(reopened.selected_provider_coding_issue().unwrap().is_none());
    reopened
        .client()
        .dispatch(AppCommand::SelectProviderModel {
            provider_id: "setup-fixture".into(),
            model: "another-model".into(),
        })
        .await
        .unwrap();
    let changed = reopened.onboarding_snapshot().unwrap();
    assert!(!changed.complete);
    assert_eq!(changed.step, OnboardingStep::Review);
    assert_eq!(changed.model.as_deref(), Some("another-model"));
    assert!(reopened
        .selected_provider_coding_issue()
        .unwrap()
        .unwrap()
        .contains("provider budget"));
}

#[tokio::test]
async fn partial_onboarding_resumes_at_the_next_step_after_restart() {
    let workspace = tempfile::tempdir().expect("workspace");
    let database = tempfile::tempdir().expect("database directory");
    let database_path = database.path().join("argentum.db");
    let mut config = HostConfig::in_memory(workspace.path());
    config.database = Some(database_path);

    let host = CommandHost::start(config.clone()).expect("host");
    let client = host.client();
    client
        .dispatch(AppCommand::SetOnboardingGuidance {
            guidance: GuidanceLevel::Guided,
        })
        .await
        .expect("guidance");
    client
        .dispatch(AppCommand::SetOnboardingProvider {
            provider_id: "llama-cpp".into(),
        })
        .await
        .expect("provider");
    drop(host);

    let reopened = CommandHost::start(config).expect("reopened host");
    let restored = reopened.onboarding_snapshot().expect("restored onboarding");
    assert_eq!(restored.step, OnboardingStep::Model);
    assert_eq!(restored.guidance, Some(GuidanceLevel::Guided));
    assert_eq!(restored.provider_id.as_deref(), Some("llama-cpp"));
    assert_eq!(restored.model, None);
    assert!(!restored.complete);
}

#[tokio::test]
async fn hosted_provider_without_a_credential_cannot_complete_onboarding() {
    let workspace = tempfile::tempdir().expect("workspace");
    let host = CommandHost::start(HostConfig::in_memory(workspace.path())).expect("host");
    let client = host.client();
    client
        .dispatch(AppCommand::SaveProviderProfile {
            profile: ProviderProfile {
                id: "openai".into(),
                label: "OpenAI".into(),
                kind: ProviderKind::OpenAiCompatible,
                endpoint: "https://api.openai.com/v1/".into(),
                model: "gpt-4o-mini".into(),
                selected: true,
            },
        })
        .await
        .expect("provider profile");
    client
        .dispatch(AppCommand::SetOnboardingGuidance {
            guidance: GuidanceLevel::Guided,
        })
        .await
        .expect("guidance");
    client
        .dispatch(AppCommand::SetOnboardingProvider {
            provider_id: "openai".into(),
        })
        .await
        .expect("provider");
    client
        .dispatch(AppCommand::SetOnboardingModel {
            provider_id: "openai".into(),
            model: "gpt-4o-mini".into(),
        })
        .await
        .expect("model");
    client
        .dispatch(AppCommand::SetOnboardingWorkspace {
            workspace: workspace.path().to_path_buf(),
        })
        .await
        .expect("workspace");
    client
        .dispatch(AppCommand::SetOnboardingPermission {
            profile_id: "read-only".into(),
        })
        .await
        .expect("permission");

    let error = client
        .dispatch(AppCommand::CompleteOnboarding)
        .await
        .expect_err("missing credential must block completion");
    assert!(error.to_string().contains("requires a credential"));
    let snapshot = host.onboarding_snapshot().expect("onboarding snapshot");
    assert_eq!(
        snapshot.credential_state,
        OnboardingCredentialState::Missing
    );
    assert_eq!(snapshot.step, OnboardingStep::Review);
    assert!(!snapshot.complete);
}

#[tokio::test]
async fn onboarding_rejects_invalid_workspace_and_non_selectable_permission() {
    let workspace = tempfile::tempdir().expect("workspace");
    let host = CommandHost::start(HostConfig::in_memory(workspace.path())).expect("host");
    let client = host.client();
    for command in [
        AppCommand::SetOnboardingGuidance {
            guidance: GuidanceLevel::Expert,
        },
        AppCommand::SetOnboardingProvider {
            provider_id: "llama-cpp".into(),
        },
        AppCommand::SetOnboardingModel {
            provider_id: "llama-cpp".into(),
            model: "local-model".into(),
        },
    ] {
        client
            .dispatch(command)
            .await
            .expect("onboarding selection");
    }

    let invalid_workspace = workspace.path().join("missing");
    assert!(client
        .dispatch(AppCommand::SetOnboardingWorkspace {
            workspace: invalid_workspace,
        })
        .await
        .is_err());
    assert_eq!(
        host.onboarding_snapshot().expect("workspace step").step,
        OnboardingStep::Workspace
    );

    client
        .dispatch(AppCommand::SetOnboardingWorkspace {
            workspace: workspace.path().to_path_buf(),
        })
        .await
        .expect("workspace");
    assert!(client
        .dispatch(AppCommand::SetOnboardingPermission {
            profile_id: "custom".into(),
        })
        .await
        .is_err());
    assert_eq!(
        host.onboarding_snapshot().expect("permission step").step,
        OnboardingStep::Permission
    );
}
