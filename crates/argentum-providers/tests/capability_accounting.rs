use std::sync::Arc;

use argentum_domain::{context_budget::ModelBudget, ProviderKind, ProviderProfile};
use argentum_providers::{
    AndroidLlamaCppBackend, ModelRequest, ModelToolDefinition, ProviderError, ProviderRegistry,
};
use uuid::Uuid;

fn profile() -> ProviderProfile {
    ProviderProfile {
        id: "llama-cpp".into(),
        label: "contained fixture".into(),
        kind: ProviderKind::LocalLlamaCpp,
        endpoint: "local://llama-cpp/".into(),
        model: "Qwen3.5-0.8B-Q4_0.gguf".into(),
        selected: true,
    }
}

fn budget(profile: &ProviderProfile) -> ModelBudget {
    ModelBudget {
        revision: Uuid::new_v4(),
        profile_id: profile.id.clone(),
        provider_kind: profile.kind,
        endpoint: profile.endpoint.clone(),
        model: profile.model.clone(),
        usable_context_tokens: 4_096,
        input_limit_tokens: Some(3_900),
        maximum_output_tokens: 512,
        requested_output_tokens: 64,
        uncertainty_tokens: 256,
    }
}

#[test]
fn final_contained_representation_accounts_for_embedded_catalog_and_exact_output_control() {
    let directory = tempfile::tempdir().unwrap();
    let (_, _, generation) = argentum_security::host_authority_channel().unwrap();
    let module = argentum_android_inference::AndroidInferenceModule::stopped(
        argentum_android_inference::InferenceConfig::new(
            directory.path(),
            argentum_android_inference::InferenceBounds::default(),
        )
        .unwrap(),
        generation,
    )
    .unwrap();
    let registry = ProviderRegistry::default().with_local_backend(Arc::new(
        AndroidLlamaCppBackend::new(Arc::new(tokio::sync::Mutex::new(module))),
    ));
    let profile = profile();
    let budget = budget(&profile);
    let request = ModelRequest {
        require_tool_call: false,
        continuation_scope: None,
        output_limit: 64,
        model: profile.model.clone(),
        system: Some("Host guidance with escaped text: \"x\" and multibyte Eesti".into()),
        history: Vec::new(),
        prompt: "Read file".into(),
        tools: vec![ModelToolDefinition {
            name: "read_text".into(),
            description: "Read text".into(),
            parameters: serde_json::json!({"type":"object","properties":{"path":{"type":"string"}}}),
        }],
        tool_exchanges: Vec::new(),
    };
    let accounting = registry
        .account_request(&profile, &budget, &request)
        .unwrap();
    assert_eq!(accounting.output_reserve, 64);
    assert_eq!(accounting.maximum_output, 512);
    assert_eq!(accounting.bundled_model_context_limit, Some(4_096));
    assert!(accounting.serialized_catalog_bytes > 0);
    assert!(accounting
        .message_components
        .contains_key("estimated_final_native_role_tokens_excluding_catalog"));
    assert!(accounting.method.contains("final_native_messages"));
}

#[test]
fn unsupported_backend_and_impossible_owner_limits_fail_before_dispatch() {
    let profile = profile();
    let request = ModelRequest {
        require_tool_call: false,
        continuation_scope: None,
        output_limit: 64,
        model: profile.model.clone(),
        system: Some("host".into()),
        history: Vec::new(),
        prompt: "task".into(),
        tools: Vec::new(),
        tool_exchanges: Vec::new(),
    };
    let unconfigured = ProviderRegistry::default();
    assert!(matches!(
        unconfigured.account_request(&profile, &budget(&profile), &request),
        Err(ProviderError::ContextAccountingUnsupported(_))
    ));

    let directory = tempfile::tempdir().unwrap();
    let (_, _, generation) = argentum_security::host_authority_channel().unwrap();
    let module = argentum_android_inference::AndroidInferenceModule::stopped(
        argentum_android_inference::InferenceConfig::new(
            directory.path(),
            argentum_android_inference::InferenceBounds::default(),
        )
        .unwrap(),
        generation,
    )
    .unwrap();
    let registry = ProviderRegistry::default().with_local_backend(Arc::new(
        AndroidLlamaCppBackend::new(Arc::new(tokio::sync::Mutex::new(module))),
    ));
    let mut impossible = budget(&profile);
    impossible.usable_context_tokens = 4_097;
    assert!(matches!(
        registry.account_request(&profile, &impossible, &request),
        Err(ProviderError::ContextBudgetInvalid(_))
    ));
}
