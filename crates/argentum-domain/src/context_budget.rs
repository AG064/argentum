//! Trusted-owner declarations, never permissions or provider-verified capacity.
use serde::{Deserialize, Serialize};

use crate::{ProviderKind, ProviderProfile};

/// All values apply to one exact saved configuration. Replacing a declaration
/// creates a new revision even when its numerical limits happen to be unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModelBudget {
    pub revision: uuid::Uuid,
    pub profile_id: String,
    pub provider_kind: ProviderKind,
    pub endpoint: String,
    pub model: String,
    pub usable_context_tokens: u64,
    pub input_limit_tokens: Option<u64>,
    pub maximum_output_tokens: u32,
    pub requested_output_tokens: u32,
    pub uncertainty_tokens: u64,
}

impl ModelBudget {
    pub const SOURCE: &'static str = "trusted_owner_supplied_not_provider_verified";

    pub fn validate(&self, profile: &ProviderProfile) -> Result<(), &'static str> {
        if self.revision.is_nil()
            || self.profile_id != profile.id
            || self.provider_kind != profile.kind
            || self.endpoint != profile.endpoint
            || self.model != profile.model
        {
            return Err(
                "budget binding is stale; declare a budget for the current profile configuration",
            );
        }
        let chatgpt_uncapped = profile.id == "chatgpt"
            && profile.kind == ProviderKind::OpenAiCompatible
            && profile.endpoint == "https://api.openai.com/v1/";
        // Resource bounds are client configuration limits, not model capacities.
        if self.usable_context_tokens == 0
            || self.usable_context_tokens > 100_000_000
            || self.maximum_output_tokens == 0
            || (self.requested_output_tokens == 0
                && !chatgpt_uncapped
                && (profile.kind != ProviderKind::LocalLlamaCpp
                    || u64::from(self.maximum_output_tokens) + 1 != self.usable_context_tokens))
            || (chatgpt_uncapped && self.requested_output_tokens != 0)
            || (chatgpt_uncapped
                && u64::from(self.maximum_output_tokens)
                    .checked_add(self.uncertainty_tokens)
                    .map_or(true, |reserved| reserved >= self.usable_context_tokens))
            || self.requested_output_tokens > self.maximum_output_tokens
            || u64::from(self.maximum_output_tokens) >= self.usable_context_tokens
            || self.uncertainty_tokens < 128
            || self
                .input_limit_tokens
                .is_some_and(|limit| limit == 0 || limit > self.usable_context_tokens)
            || u64::from(self.requested_output_tokens)
                .checked_add(self.uncertainty_tokens)
                .map_or(true, |reserved| reserved >= self.usable_context_tokens)
        {
            return Err(
                "invalid context/input/output/uncertainty relationship; no allowance was reduced",
            );
        }
        Ok(())
    }
}

/// Estimated accounting, not observed usage or a universal tokenizer bound.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestAccounting {
    pub method: String,
    pub capacity_source: String,
    pub budget_revision: uuid::Uuid,
    pub profile_id: String,
    pub model: String,
    pub provider_endpoint: String,
    pub provider_kind: ProviderKind,
    pub accounted_input: u64,
    pub serialized_message_bytes: u64,
    pub message_components: std::collections::BTreeMap<String, u64>,
    pub serialized_catalog_bytes: u64,
    pub serialized_envelope_bytes: u64,
    pub output_reserve: u32,
    pub uncertainty_allowance: u64,
    pub usable_context: u64,
    pub separate_input_limit: Option<u64>,
    pub maximum_output: u32,
    pub bundled_model_context_limit: Option<u64>,
    pub profile_catalog_context_limit: Option<u64>,
    pub profile_catalog_source: Option<crate::ProviderCatalogSource>,
    pub fits: bool,
}

impl RequestAccounting {
    /// One estimated token per serialized UTF-8 wire byte, including escaping,
    /// with an additional owner allowance for unknown tokenization/protocol
    /// overhead. Deliberately conservative for common text, but NOT a proof of
    /// an upper bound for arbitrary tokenizers or vendor-side transformations.
    pub fn from_wire_bytes(
        budget: &ModelBudget,
        message_bytes: u64,
        catalog_bytes: u64,
        envelope_bytes: u64,
    ) -> Result<Self, &'static str> {
        let input = message_bytes
            .checked_add(catalog_bytes)
            .and_then(|n| n.checked_add(envelope_bytes))
            .ok_or("context accounting overflow")?;
        let uncertain_input = input
            .checked_add(budget.uncertainty_tokens)
            .ok_or("context accounting overflow")?;
        // Uncapped plan output still reserves the declared maximum for admission.
        let output_reserve =
            if budget.profile_id == "chatgpt" && budget.requested_output_tokens == 0 {
                budget.maximum_output_tokens
            } else {
                budget.requested_output_tokens
            };
        let total = uncertain_input
            .checked_add(u64::from(output_reserve))
            .ok_or("context accounting overflow")?;
        Ok(Self {
            method: "estimated_utf8_wire_bytes_v1_not_tokenizer_upper_bound".into(),
            capacity_source: ModelBudget::SOURCE.into(),
            budget_revision: budget.revision,
            profile_id: budget.profile_id.clone(),
            model: budget.model.clone(),
            provider_endpoint: budget.endpoint.clone(),
            provider_kind: budget.provider_kind,
            accounted_input: input,
            serialized_message_bytes: message_bytes,
            message_components: std::collections::BTreeMap::new(),
            serialized_catalog_bytes: catalog_bytes,
            serialized_envelope_bytes: envelope_bytes,
            output_reserve,
            uncertainty_allowance: budget.uncertainty_tokens,
            usable_context: budget.usable_context_tokens,
            separate_input_limit: budget.input_limit_tokens,
            maximum_output: budget.maximum_output_tokens,
            bundled_model_context_limit: None,
            profile_catalog_context_limit: None,
            profile_catalog_source: None,
            fits: total <= budget.usable_context_tokens
                && budget
                    .input_limit_tokens
                    .map_or(true, |limit| uncertain_input <= limit),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture() -> (ProviderProfile, ModelBudget) {
        let profile = ProviderProfile {
            id: "fixture".into(),
            label: "Synthetic fixture".into(),
            kind: ProviderKind::OpenAiCompatible,
            endpoint: "http://127.0.0.1:12345/v1/".into(),
            model: "synthetic".into(),
            selected: true,
        };
        let budget = ModelBudget {
            revision: uuid::Uuid::new_v4(),
            profile_id: profile.id.clone(),
            provider_kind: profile.kind,
            endpoint: profile.endpoint.clone(),
            model: profile.model.clone(),
            usable_context_tokens: 1000,
            input_limit_tokens: Some(900),
            maximum_output_tokens: 200,
            requested_output_tokens: 100,
            uncertainty_tokens: 128,
        };
        (profile, budget)
    }

    #[test]
    fn chatgpt_uncapped_output_is_exact_route_bound_and_reserves_maximum() {
        let (mut profile, mut budget) = fixture();
        profile.id = "chatgpt".into();
        profile.endpoint = "https://api.openai.com/v1/".into();
        budget.profile_id = profile.id.clone();
        budget.endpoint = profile.endpoint.clone();
        budget.requested_output_tokens = 0;
        assert!(budget.validate(&profile).is_ok());
        let accounting = RequestAccounting::from_wire_bytes(&budget, 673, 0, 0).unwrap();
        assert_eq!(accounting.output_reserve, 200);
        assert!(!accounting.fits);
        budget.requested_output_tokens = 100;
        assert!(budget.validate(&profile).is_err());
        budget.requested_output_tokens = 0;
        profile.id = "deepseek".into();
        budget.profile_id = profile.id.clone();
        assert!(budget.validate(&profile).is_err());
        profile.id = "chatgpt".into();
        budget.profile_id = profile.id.clone();
        profile.endpoint = "https://other.invalid/v1/".into();
        budget.endpoint = profile.endpoint.clone();
        assert!(budget.validate(&profile).is_err());
    }

    #[test]
    fn exact_boundary_and_one_byte_overflow_keep_requested_output() {
        let (profile, budget) = fixture();
        budget.validate(&profile).unwrap();
        let exact = RequestAccounting::from_wire_bytes(&budget, 700, 60, 12).unwrap();
        assert!(exact.fits);
        assert_eq!(exact.output_reserve, 100);
        assert!(
            !RequestAccounting::from_wire_bytes(&budget, 701, 60, 12)
                .unwrap()
                .fits
        );
        assert!(RequestAccounting::from_wire_bytes(&budget, u64::MAX, 1, 0).is_err());
    }

    #[test]
    fn invalid_and_stale_declarations_fail_without_adjustment() {
        let (mut profile, mut budget) = fixture();
        profile.model = "other".into();
        assert!(budget.validate(&profile).is_err());
        profile.model = budget.model.clone();
        budget.requested_output_tokens = 201;
        assert!(budget.validate(&profile).is_err());
        assert_eq!(budget.requested_output_tokens, 201);
        budget.requested_output_tokens = 100;
        budget.input_limit_tokens = Some(0);
        assert!(budget.validate(&profile).is_err());
        budget.input_limit_tokens = None;
        budget.uncertainty_tokens = u64::MAX;
        assert!(budget.validate(&profile).is_err());
    }
}
