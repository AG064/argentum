use argentum_domain::{ProviderKind, ProviderProfile};
use sha2::{Digest, Sha256};

use crate::{local_backend::LocalBackendIdentity, ProviderError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilitySupport {
    Supported,
    Unsupported,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCapabilities {
    pub text_generation: CapabilitySupport,
    pub structured_tools: CapabilitySupport,
    pub streaming: CapabilitySupport,
    pub cancellation: CapabilitySupport,
    pub context_accounting: CapabilitySupport,
    pub output_limits: CapabilitySupport,
    pub usage: CapabilitySupport,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCapabilityIdentity {
    pub profile_id: String,
    pub endpoint: String,
    pub model: String,
    pub adapter_representation: &'static str,
    pub local_backend_id: Option<&'static str>,
    pub local_backend_generation: Option<u64>,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProviderCapabilityResolution {
    pub identity: ProviderCapabilityIdentity,
    pub capabilities: ProviderCapabilities,
    pub runtime_evidence: CapabilitySupport,
    pub detail: &'static str,
}

impl ProviderCapabilityResolution {
    pub fn supports_coding(&self) -> bool {
        [
            self.capabilities.text_generation,
            self.capabilities.structured_tools,
            self.capabilities.streaming,
            self.capabilities.cancellation,
            self.capabilities.context_accounting,
        ]
        .into_iter()
        .all(|support| support == CapabilitySupport::Supported)
            && (self.capabilities.output_limits == CapabilitySupport::Supported
                || (self.identity.adapter_representation
                    == "openai.responses.chatgpt-plan.tools.v1"
                    && self.capabilities.output_limits == CapabilitySupport::Unsupported))
    }

    pub fn require_coding(&self) -> Result<(), ProviderError> {
        if self.supports_coding() {
            Ok(())
        } else {
            Err(ProviderError::CodingCapabilityUnavailable {
                reason: self.detail,
            })
        }
    }
}

pub(crate) fn resolve_provider_capabilities(
    profile: &ProviderProfile,
    local_backend: LocalBackendIdentity,
) -> ProviderCapabilityResolution {
    let (adapter, backend_id, backend_generation, capabilities, evidence, detail) =
        match profile.kind {
            ProviderKind::OpenAiCompatible if profile.id == "chatgpt" && profile.endpoint == "https://api.openai.com/v1/" => (
                "openai.responses.chatgpt-plan.tools.v1",
                None, None,
                ProviderCapabilities {
                    text_generation: CapabilitySupport::Supported,
                    structured_tools: CapabilitySupport::Supported,
                    streaming: CapabilitySupport::Supported,
                    cancellation: CapabilitySupport::Supported,
                    context_accounting: CapabilitySupport::Supported,
                    // No provider output cap. The exact route's canonical budget
                    // must explicitly opt in with requested output zero.
                    output_limits: CapabilitySupport::Unsupported,
                    usage: CapabilitySupport::Supported,
                },
                CapabilitySupport::Unknown,
                "native ChatGPT Responses adapter; uncapped provider output with client resource guards",
            ),
            ProviderKind::OpenAiCompatible => (
                "openai.chat-completions.tools.v1",
                None,
                None,
                ProviderCapabilities {
                    text_generation: CapabilitySupport::Supported,
                    structured_tools: CapabilitySupport::Supported,
                    streaming: CapabilitySupport::Supported,
                    cancellation: CapabilitySupport::Supported,
                    context_accounting: CapabilitySupport::Supported,
                    output_limits: CapabilitySupport::Supported,
                    usage: CapabilitySupport::Supported,
                },
                CapabilitySupport::Unknown,
                "compatible chat-completions coding adapter is implemented",
            ),
            ProviderKind::LocalLlamaCpp
                if profile.endpoint == "local://llama-cpp/"
                    && local_backend.backend_id == Some("llama.cpp.android")
                    && profile.model == "Qwen3.5-0.8B-Q4_0.gguf" =>
            {
                (
                    "qwen3.5.xml-tools.android.v1",
                    local_backend.backend_id,
                    local_backend.generation,
                    ProviderCapabilities {
                        text_generation: CapabilitySupport::Supported,
                        structured_tools: CapabilitySupport::Supported,
                        streaming: CapabilitySupport::Supported,
                        cancellation: CapabilitySupport::Supported,
                        context_accounting: CapabilitySupport::Supported,
                        output_limits: CapabilitySupport::Supported,
                        usage: CapabilitySupport::Unknown,
                    },
                    CapabilitySupport::Unknown,
                    "contained Qwen3.5 Android coding adapter is implemented; a matching loaded model is validated at dispatch",
                )
            }
            ProviderKind::LocalLlamaCpp if profile.endpoint == "local://llama-cpp/" && local_backend.backend_id == Some("llama.cpp.sidecar") => (
                "llama.cpp.server.chat-completions.tools.v1",
                local_backend.backend_id,
                local_backend.generation,
                ProviderCapabilities {
                    text_generation: CapabilitySupport::Supported,
                    structured_tools: CapabilitySupport::Supported,
                    streaming: CapabilitySupport::Supported,
                    cancellation: CapabilitySupport::Supported,
                    context_accounting: CapabilitySupport::Supported,
                    output_limits: CapabilitySupport::Supported,
                    usage: CapabilitySupport::Unknown,
                },
                CapabilitySupport::Unknown,
                "contained llama.cpp tool adapter is implemented; the loaded template and exact tokenizer are validated before dispatch",
            ),
            ProviderKind::LocalLlamaCpp => (
                "local.llama-cpp.unresolved",
                local_backend.backend_id,
                local_backend.generation,
                ProviderCapabilities {
                    text_generation: CapabilitySupport::Unknown,
                    structured_tools: CapabilitySupport::Unsupported,
                    streaming: CapabilitySupport::Unknown,
                    cancellation: CapabilitySupport::Unknown,
                    context_accounting: CapabilitySupport::Unsupported,
                    output_limits: CapabilitySupport::Unknown,
                    usage: CapabilitySupport::Unknown,
                },
                CapabilitySupport::Unknown,
                "no supported contained coding representation matches this backend, endpoint, and model",
            ),
            ProviderKind::Anthropic => (
                "anthropic.messages.text.v1",
                None,
                None,
                ProviderCapabilities {
                    text_generation: CapabilitySupport::Supported,
                    structured_tools: CapabilitySupport::Unsupported,
                    streaming: CapabilitySupport::Supported,
                    cancellation: CapabilitySupport::Supported,
                    context_accounting: CapabilitySupport::Supported,
                    output_limits: CapabilitySupport::Supported,
                    usage: CapabilitySupport::Supported,
                },
                CapabilitySupport::Unknown,
                "the Anthropic adapter does not implement the shared structured-tool contract",
            ),
            ProviderKind::LocalLmStudio | ProviderKind::Unknown => (
                "provider.text-only-or-unknown",
                None,
                None,
                ProviderCapabilities {
                    text_generation: CapabilitySupport::Unknown,
                    structured_tools: CapabilitySupport::Unsupported,
                    streaming: CapabilitySupport::Unknown,
                    cancellation: CapabilitySupport::Unknown,
                    context_accounting: CapabilitySupport::Unknown,
                    output_limits: CapabilitySupport::Unknown,
                    usage: CapabilitySupport::Unknown,
                },
                CapabilitySupport::Unknown,
                "this configured adapter has no implemented shared coding-tool contract",
            ),
        };
    let fingerprint = capability_fingerprint(profile, adapter, backend_id, backend_generation);
    ProviderCapabilityResolution {
        identity: ProviderCapabilityIdentity {
            profile_id: profile.id.clone(),
            endpoint: profile.endpoint.clone(),
            model: profile.model.clone(),
            adapter_representation: adapter,
            local_backend_id: backend_id,
            local_backend_generation: backend_generation,
            fingerprint,
        },
        capabilities,
        runtime_evidence: evidence,
        detail,
    }
}

fn capability_fingerprint(
    profile: &ProviderProfile,
    adapter: &str,
    backend_id: Option<&str>,
    backend_generation: Option<u64>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"argentum.provider-capability.v1\0");
    for field in [
        profile.id.as_str(),
        &format!("{:?}", profile.kind),
        profile.endpoint.as_str(),
        profile.model.as_str(),
        adapter,
        backend_id.unwrap_or(""),
    ] {
        hasher.update((field.len() as u64).to_le_bytes());
        hasher.update(field.as_bytes());
    }
    hasher.update(backend_generation.unwrap_or_default().to_le_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn local_profile(model: &str) -> ProviderProfile {
        ProviderProfile {
            id: "llama-cpp".into(),
            label: "llama.cpp".into(),
            kind: ProviderKind::LocalLlamaCpp,
            endpoint: "local://llama-cpp/".into(),
            model: model.into(),
            selected: true,
        }
    }

    #[test]
    fn only_the_exact_contained_configuration_resolves_coding_support() {
        let android = resolve_provider_capabilities(
            &local_profile("Qwen3.5-0.8B-Q4_0.gguf"),
            LocalBackendIdentity {
                backend_id: Some("llama.cpp.android"),
                generation: Some(7),
            },
        );
        assert!(android.supports_coding());
        assert_eq!(android.identity.local_backend_generation, Some(7));

        let different_model = resolve_provider_capabilities(
            &local_profile("loaded-but-unproven.gguf"),
            LocalBackendIdentity {
                backend_id: Some("llama.cpp.android"),
                generation: Some(7),
            },
        );
        assert!(!different_model.supports_coding());
        assert_eq!(
            different_model.capabilities.structured_tools,
            CapabilitySupport::Unsupported
        );

        let sidecar = resolve_provider_capabilities(
            &local_profile("Qwen3.5-0.8B-Q4_0.gguf"),
            LocalBackendIdentity {
                backend_id: Some("llama.cpp.sidecar"),
                generation: Some(8),
            },
        );
        assert!(sidecar.supports_coding());
        assert_eq!(sidecar.runtime_evidence, CapabilitySupport::Unknown);
        assert_ne!(android.identity.fingerprint, sidecar.identity.fingerprint);
    }

    #[test]
    fn compatible_http_support_does_not_claim_runtime_validation() {
        let profile = ProviderProfile {
            id: "fixture".into(),
            label: "fixture".into(),
            kind: ProviderKind::OpenAiCompatible,
            endpoint: "http://127.0.0.1:12345/v1/".into(),
            model: "fixture-model".into(),
            selected: true,
        };
        let resolution = resolve_provider_capabilities(&profile, LocalBackendIdentity::default());
        assert!(resolution.supports_coding());
        assert_eq!(resolution.runtime_evidence, CapabilitySupport::Unknown);
        assert_eq!(resolution.identity.local_backend_id, None);
    }
}
