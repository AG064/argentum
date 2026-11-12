//! Pinned llama.cpp chat/tool protocol. Metadata and exact token counts use the
//! already-approved, authenticated child. Nothing here loads a model or grants tools.
use super::*;
use argentum_domain::context_budget::{ModelBudget, RequestAccounting};
use sha2::{Digest, Sha256};

pub const SIDECAR_CONTEXT_TOKENS: u32 = 8192;
pub const SIDECAR_CONTEXT_VARIANTS: [u32; 6] = [8192, 16384, 32768, 65536, 131072, 262144];

/// Select the smallest supported runtime window covering an owner budget.
/// Smaller legacy fixed-output budgets still use the existing 8K launch.
pub fn sidecar_context_for_limit(tokens: u64) -> Result<u32, ProviderError> {
    SIDECAR_CONTEXT_VARIANTS
        .into_iter()
        .find(|size| tokens > 0 && u64::from(*size) >= tokens)
        .ok_or(ProviderError::ContextBudgetInvalid(
            "local context must fit a supported 8K, 16K, 32K, 64K, 128K or 256K window",
        ))
}

pub fn parse_sidecar_context(value: &str) -> Result<u32, ProviderError> {
    let normalized = value.trim().to_ascii_lowercase();
    let tokens = if let Some(k) = normalized.strip_suffix('k') {
        k.parse::<u32>().ok().and_then(|n| n.checked_mul(1024))
    } else {
        normalized.parse::<u32>().ok()
    };
    tokens
        .filter(|n| SIDECAR_CONTEXT_VARIANTS.contains(n))
        .ok_or(ProviderError::ContextBudgetInvalid(
            "choose local context 8K, 16K, 32K, 64K, 128K or 256K",
        ))
}

pub(super) fn account_request(
    profile: &ProviderProfile,
    budget: &ModelBudget,
    request: &ModelRequest,
) -> Result<RequestAccounting, ProviderError> {
    budget
        .validate(profile)
        .map_err(ProviderError::ContextBudgetInvalid)?;
    let configured_context = sidecar_context_for_limit(budget.usable_context_tokens)?;
    if profile.endpoint != "local://llama-cpp/"
        || request.model != profile.model
        || request.output_limit != budget.requested_output_tokens
        || (request.output_limit == 0
            && budget.usable_context_tokens != u64::from(configured_context))
    {
        return Err(ProviderError::ContextBudgetInvalid(
            "sidecar context/model/output binding mismatch",
        ));
    }
    let body = build_chat_request_inner(request.clone(), false, false, true)?;
    let messages = serde_json::to_vec(&body.messages)
        .map_err(|_| ProviderError::InvalidRequest("message serialization failed"))?
        .len() as u64;
    let tools = serde_json::to_vec(&body.tools)
        .map_err(|_| ProviderError::InvalidRequest("tool serialization failed"))?
        .len() as u64;
    let total = serde_json::to_vec(&body)
        .map_err(|_| ProviderError::InvalidRequest("request serialization failed"))?
        .len() as u64;
    let mut result = RequestAccounting::from_wire_bytes(
        budget,
        messages,
        tools,
        total.saturating_sub(messages + tools),
    )
    .map_err(ProviderError::InvalidRequest)?;
    result.capacity_source =
        "configured_contained_sidecar; exact loaded-tokenizer validation required before dispatch"
            .into();
    result.bundled_model_context_limit = Some(u64::from(configured_context));
    Ok(result)
}

impl OpenAiCompatibleProvider {
    async fn sidecar_json(
        &self,
        url: Url,
        body: Option<&ChatCompletionRequest>,
    ) -> Result<serde_json::Value, ProviderError> {
        // This client has proxies and redirects disabled. Its address and bearer
        // credential came from the approved child, never from model text.
        let key = self
            .api_key
            .as_ref()
            .ok_or(ProviderError::LocalRuntimeLeaseStale)?;
        let req = match body {
            Some(body) => self.probe_client.post(url).json(body),
            None => self.probe_client.get(url),
        }
        .bearer_auth(key.expose())
        .timeout(Duration::from_secs(10));
        let response = req
            .send()
            .await
            .map_err(|_| ProviderError::LocalRuntimeUnavailable {
                reason: "contained tokenizer/metadata IPC failed",
            })?;
        if !response.status().is_success() {
            return Err(ProviderError::LocalRuntimeUnavailable {
                reason: "contained tokenizer/metadata endpoint rejected the request",
            });
        }
        let bytes = self.read_probe_body(response).await?;
        serde_json::from_slice(&bytes).map_err(|_| ProviderError::LocalRuntimeUnavailable {
            reason: "contained tokenizer/metadata response is invalid",
        })
    }

    pub(super) async fn validate_sidecar_request(
        &self,
        request: &ModelRequest,
    ) -> Result<ProviderRequestValidation, ProviderError> {
        let body = build_chat_request_inner(request.clone(), false, false, true)?;
        let props = self
            .sidecar_json(self.endpoint.join("../props")?, None)
            .await?;
        let context = props["default_generation_settings"]["n_ctx"]
            .as_u64()
            .filter(|n| *n == u64::from(self.sidecar_context_tokens))
            .ok_or(ProviderError::LocalRuntimeUnavailable {
                reason: "loaded context does not match the approved sidecar configuration",
            })?;
        let template = props["chat_template"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or(ProviderError::LocalCapabilityUnsupported {
                capability: "a loaded chat template",
            })?;
        if (!request.tools.is_empty()
            || !request.tool_exchanges.is_empty()
            || request.history.iter().any(|m| m.tool_exchange.is_some()))
            && (props["chat_template_caps"]["supports_tools"] != true
                || props["chat_template_caps"]["supports_tool_calls"] != true)
        {
            return Err(ProviderError::LocalCapabilityUnsupported {
                capability: "tool definitions and results in this loaded model's chat template",
            });
        }
        // Same chat-completions body and server template path as generation.
        // This endpoint counts the final prompt without running inference.
        let counted = self
            .sidecar_json(
                self.endpoint.join("chat/completions/input_tokens")?,
                Some(&body),
            )
            .await?;
        let tokens = counted["input_tokens"].as_u64().filter(|n| *n > 0).ok_or(
            ProviderError::LocalRuntimeUnavailable {
                reason: "contained tokenizer omitted its token count",
            },
        )?;
        let mut hash = Sha256::new();
        hash.update(b"argentum.sidecar.chat-tools.v1\0");
        hash.update(template.as_bytes());
        hash.update(
            serde_json::to_vec(&body)
                .map_err(|_| ProviderError::InvalidRequest("request fingerprint failed"))?,
        );
        Ok(ProviderRequestValidation {
            exact_input_tokens: Some(tokens),
            context_tokens: Some(context),
            output_limit: request.output_limit,
            representation_fingerprint: Some(format!("{:x}", hash.finalize())),
            runtime_fingerprint: None,
        })
    }
}

#[cfg(test)]
mod context_variant_tests {
    use super::*;
    #[test]
    fn context_variants_parse_and_account_every_power_of_two() {
        let profile = ProviderProfile {
            id: "llama-cpp".into(),
            label: "local".into(),
            kind: ProviderKind::LocalLlamaCpp,
            endpoint: "local://llama-cpp/".into(),
            model: "fixture.gguf".into(),
            selected: true,
        };
        for size in SIDECAR_CONTEXT_VARIANTS {
            assert_eq!(
                parse_sidecar_context(&format!("{}K", size / 1024)).unwrap(),
                size
            );
            assert_eq!(parse_sidecar_context(&size.to_string()).unwrap(), size);
            let budget = ModelBudget {
                revision: uuid::Uuid::new_v4(),
                profile_id: profile.id.clone(),
                provider_kind: profile.kind,
                endpoint: profile.endpoint.clone(),
                model: profile.model.clone(),
                usable_context_tokens: u64::from(size),
                input_limit_tokens: Some(u64::from(size)),
                maximum_output_tokens: size - 1,
                requested_output_tokens: 0,
                uncertainty_tokens: 128,
            };
            let request = ModelRequest {
                require_tool_call: false,
                continuation_scope: None,
                model: profile.model.clone(),
                output_limit: 0,
                system: None,
                history: Vec::new(),
                prompt: "fixture".into(),
                tools: Vec::new(),
                tool_exchanges: Vec::new(),
            };
            assert_eq!(
                account_request(&profile, &budget, &request)
                    .unwrap()
                    .bundled_model_context_limit,
                Some(u64::from(size))
            );
        }
        for value in [
            "0",
            "4k",
            "12k",
            "512k",
            "8193",
            "99999999999999k",
            "-1",
            "32k --shell",
        ] {
            assert!(parse_sidecar_context(value).is_err(), "{value}");
        }
        assert_eq!(sidecar_context_for_limit(4096).unwrap(), 8192);
        assert!(sidecar_context_for_limit(262145).is_err());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn uncapped_mode_cannot_underreserve_a_smaller_declared_context() {
        let profile = ProviderProfile {
            id: "llama-cpp".into(),
            label: "fixture".into(),
            kind: ProviderKind::LocalLlamaCpp,
            endpoint: "local://llama-cpp/".into(),
            model: "fixture.gguf".into(),
            selected: true,
        };
        let mut budget = ModelBudget {
            revision: argentum_domain::RunId::new_v4(),
            profile_id: profile.id.clone(),
            provider_kind: profile.kind,
            endpoint: profile.endpoint.clone(),
            model: profile.model.clone(),
            usable_context_tokens: 8192,
            input_limit_tokens: Some(8192),
            maximum_output_tokens: 8191,
            requested_output_tokens: 0,
            uncertainty_tokens: 128,
        };
        let request = ModelRequest {
            require_tool_call: false,
            continuation_scope: None,
            model: profile.model.clone(),
            output_limit: 0,
            prompt: "hello".into(),
            system: None,
            history: Vec::new(),
            tools: Vec::new(),
            tool_exchanges: Vec::new(),
        };
        assert!(account_request(&profile, &budget, &request).is_ok());
        budget.usable_context_tokens = 4096;
        budget.input_limit_tokens = Some(4096);
        budget.maximum_output_tokens = 4095;
        assert!(account_request(&profile, &budget, &request).is_err());
        let mut hosted = profile;
        hosted.kind = ProviderKind::OpenAiCompatible;
        budget.provider_kind = hosted.kind;
        assert!(budget.validate(&hosted).is_err());
    }
}
