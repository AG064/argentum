//! Run-local MiniMax assistant state. This is protocol data, never authority.
//!
//! The supported split format has incremental text fragments. Preserve block
//! identities and optional metadata instead of reconstructing it from display.

use super::{ChatDelta, ModelToolCall, ProviderError};
use argentum_domain::ProviderProfile;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

const MAX_STATE_BYTES: usize = 512 * 1024;
const MAX_DETAIL_BLOCKS: usize = 32;
const MAX_ID_BYTES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContinuationScope {
    pub session_id: Uuid,
    pub run_id: Uuid,
}

#[derive(Clone, PartialEq, Eq, Deserialize, Serialize, Default)]
pub(super) struct ReasoningDetail {
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub index: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
}

impl std::fmt::Debug for ReasoningDetail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReasoningDetail")
            .field("index", &self.index)
            .field("text_bytes", &self.text.as_ref().map(String::len))
            .finish_non_exhaustive()
    }
}

#[derive(Clone, PartialEq, Eq)]
struct RunBinding {
    profile_id: String,
    scope: ContinuationScope,
    request_id: Uuid,
}

/// Bounded completed provider response, intentionally neither serializable nor
/// constructible from model arguments or durable conversation data.
#[derive(Clone, PartialEq, Eq)]
pub struct MinimaxAssistantState {
    origin: String,
    model: String,
    provider_binding: Uuid,
    response_id: Option<String>,
    binding: Option<RunBinding>,
    pub(super) content: Option<String>,
    pub(super) name: Option<String>,
    pub(super) reasoning_content: Option<String>,
    pub(super) reasoning_details: Vec<ReasoningDetail>,
    pub(super) tool_call_indexes: Vec<usize>,
    calls: Vec<ModelToolCall>,
    finish_reason: String,
    retained_bytes: usize,
}

impl std::fmt::Debug for MinimaxAssistantState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MinimaxAssistantState")
            .field("bound", &self.binding.is_some())
            .field("response_id_present", &self.response_id.is_some())
            .field("detail_blocks", &self.reasoning_details.len())
            .field("tool_calls", &self.calls.len())
            .field("retained_bytes", &self.retained_bytes)
            .finish_non_exhaustive()
    }
}

fn invalid() -> ProviderError {
    ProviderError::InvalidResponse("invalid or over-capacity MiniMax continuation state".into())
}

fn normalized_origin(endpoint: &str) -> Result<String, ProviderError> {
    let mut url = url::Url::parse(endpoint)?;
    if url.scheme() != "https"
        || url.host_str() != Some("api.minimax.io")
        || url.port_or_known_default() != Some(443)
        || url.path().trim_end_matches('/') != "/v1"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(ProviderError::PreparedRequestBindingMismatch);
    }
    url.set_path("/v1/");
    Ok(url.to_string())
}

impl MinimaxAssistantState {
    pub fn bind_run(
        mut self,
        profile: &ProviderProfile,
        scope: ContinuationScope,
        request_id: Uuid,
    ) -> Result<Self, ProviderError> {
        if self.binding.is_some()
            || self.origin != normalized_origin(&profile.endpoint)?
            || self.model != profile.model
            || profile.kind != argentum_domain::ProviderKind::OpenAiCompatible
        {
            return Err(ProviderError::PreparedRequestBindingMismatch);
        }
        self.binding = Some(RunBinding {
            profile_id: profile.id.clone(),
            scope,
            request_id,
        });
        Ok(self)
    }

    pub fn retained_bytes(&self) -> usize {
        self.retained_bytes
    }

    pub(super) fn validate(
        &self,
        profile: &ProviderProfile,
        scope: Option<ContinuationScope>,
        calls: &[ModelToolCall],
    ) -> Result<(), ProviderError> {
        self.validate_scope(scope, &profile.model, calls)?;
        let binding = self
            .binding
            .as_ref()
            .ok_or(ProviderError::PreparedRequestBindingMismatch)?;
        if self.origin != normalized_origin(&profile.endpoint)?
            || self.model != profile.model
            || binding.profile_id != profile.id
            || Some(binding.scope) != scope
            || self.calls != calls
            || self.finish_reason != "tool_calls"
        {
            return Err(ProviderError::PreparedRequestBindingMismatch);
        }
        Ok(())
    }

    pub(super) fn validate_scope(
        &self,
        scope: Option<ContinuationScope>,
        model: &str,
        calls: &[ModelToolCall],
    ) -> Result<Uuid, ProviderError> {
        let binding = self
            .binding
            .as_ref()
            .ok_or(ProviderError::PreparedRequestBindingMismatch)?;
        if Some(binding.scope) != scope
            || self.model != model
            || self.calls != calls
            || self.finish_reason != "tool_calls"
        {
            return Err(ProviderError::PreparedRequestBindingMismatch);
        }
        Ok(binding.request_id)
    }

    pub(super) fn validate_provider(&self, provider_binding: Uuid) -> Result<(), ProviderError> {
        if self.provider_binding != provider_binding {
            return Err(ProviderError::PreparedRequestBindingMismatch);
        }
        Ok(())
    }
}

pub(super) struct Accumulator {
    state: MinimaxAssistantState,
    bytes_seen: usize,
}

impl Accumulator {
    pub fn new(origin: &str, model: &str, provider_binding: Uuid) -> Result<Self, ProviderError> {
        Ok(Self {
            state: MinimaxAssistantState {
                origin: normalized_origin(origin)?,
                model: model.into(),
                provider_binding,
                response_id: None,
                binding: None,
                content: None,
                name: None,
                reasoning_content: None,
                reasoning_details: Vec::new(),
                tool_call_indexes: Vec::new(),
                calls: Vec::new(),
                finish_reason: String::new(),
                retained_bytes: 0,
            },
            // Fixed message/field envelope allowance. Charge retained values,
            // not repeated SSE metadata or optional nulls in every frame.
            bytes_seen: 256,
        })
    }

    pub fn push(
        &mut self,
        response_id: Option<&str>,
        delta: &ChatDelta,
        finish_reason: Option<&str>,
    ) -> Result<(), ProviderError> {
        if !self.state.finish_reason.is_empty() {
            return Err(invalid());
        }
        if let Some(id) = response_id {
            bounded_metadata(id)?;
            if self
                .state
                .response_id
                .as_deref()
                .is_some_and(|previous| previous != id)
            {
                return Err(invalid());
            }
            self.state.response_id = Some(id.into());
        }
        if delta
            .role
            .as_deref()
            .is_some_and(|role| role != "assistant")
        {
            return Err(invalid());
        }
        if delta.reasoning.is_some() {
            // This alias is supported by generic adapters, not the documented
            // MiniMax continuation representation. Do not silently discard it.
            return Err(invalid());
        }
        if let Some(name) = &delta.name {
            bounded_metadata(name)?;
            if self.state.name.is_none() {
                self.charge(fragment_bytes(name)? + 16)?;
            }
            merge_metadata(&mut self.state.name, &Some(name.clone()))?;
        }
        for fragment in [&delta.content, &delta.reasoning_content]
            .into_iter()
            .flatten()
        {
            self.charge(fragment_bytes(fragment)?)?;
        }
        append_fragment(&mut self.state.content, delta.content.as_deref());
        append_fragment(
            &mut self.state.reasoning_content,
            delta.reasoning_content.as_deref(),
        );
        for call in &delta.tool_calls {
            if call.index >= super::MAX_TOOL_CALLS_PER_RESPONSE {
                return Err(invalid());
            }
            if !self.state.tool_call_indexes.contains(&call.index) {
                self.state.tool_call_indexes.push(call.index);
                self.state.tool_call_indexes.sort_unstable();
            }
        }
        for (position, detail) in delta.reasoning_details.iter().enumerate() {
            if detail
                .kind
                .as_deref()
                .is_some_and(|kind| kind != "reasoning.text")
            {
                return Err(invalid());
            }
            for value in [&detail.id, &detail.format, &detail.kind]
                .into_iter()
                .flatten()
            {
                bounded_metadata(value)?;
            }
            if detail
                .index
                .is_some_and(|index| index as usize >= MAX_DETAIL_BLOCKS)
            {
                return Err(invalid());
            }
            if detail.index.is_none()
                && detail.id.is_none()
                && self.state.reasoning_details.len() > 1
                && delta.reasoning_details.len() < self.state.reasoning_details.len()
            {
                return Err(invalid()); // no identity for an ambiguous partial block array
            }
            if let Some(id) = &detail.id {
                if self.state.reasoning_details.iter().any(|old| {
                    old.id.as_ref() == Some(id)
                        && old.index.is_some()
                        && detail.index.is_some()
                        && old.index != detail.index
                }) {
                    return Err(invalid());
                }
            }
            // Explicit indexes/IDs correlate incremental fragments. In their
            // absence, array position is the documented block position. Never
            // guess cumulative snapshots by matching text prefixes.
            let slot =
                self.state
                    .reasoning_details
                    .iter()
                    .enumerate()
                    .position(|(slot, old)| {
                        match (
                            detail.index,
                            old.index,
                            detail.id.as_deref(),
                            old.id.as_deref(),
                        ) {
                            (Some(index), Some(old_index), _, _) => index == old_index,
                            (_, _, Some(id), Some(old_id)) => id == old_id,
                            (_, _, Some(_), None) => slot == position,
                            (None, _, None, _) => slot == position,
                            (Some(index), None, None, _) => slot == index as usize,
                        }
                    });
            if let Some(slot) = slot {
                let old = &self.state.reasoning_details[slot];
                let metadata_bytes = [
                    (&old.kind, &detail.kind),
                    (&old.id, &detail.id),
                    (&old.format, &detail.format),
                ]
                .into_iter()
                .filter_map(|(old, new)| old.is_none().then_some(new.as_deref()).flatten())
                .map(|value| fragment_bytes(value).map(|bytes| bytes + 32))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .sum::<usize>();
                self.charge(
                    metadata_bytes
                        + detail
                            .text
                            .as_deref()
                            .map(fragment_bytes)
                            .transpose()?
                            .unwrap_or(0),
                )?;
                let old = &mut self.state.reasoning_details[slot];
                merge_metadata(&mut old.kind, &detail.kind)?;
                merge_metadata(&mut old.id, &detail.id)?;
                merge_metadata(&mut old.format, &detail.format)?;
                if old.index != detail.index && old.index.is_some() && detail.index.is_some() {
                    return Err(invalid());
                }
                old.index = old.index.or(detail.index);
                append_fragment(&mut old.text, detail.text.as_deref());
            } else {
                if self.state.reasoning_details.len() >= MAX_DETAIL_BLOCKS {
                    return Err(invalid());
                }
                self.charge(serde_json::to_vec(detail).map_err(|_| invalid())?.len() + 1)?;
                self.state.reasoning_details.push(detail.clone());
            }
        }
        if let Some(reason) = finish_reason {
            if !self.state.finish_reason.is_empty() {
                return Err(invalid());
            }
            self.state.finish_reason = reason.into();
        }
        Ok(())
    }

    pub fn finish(
        mut self,
        calls: &[ModelToolCall],
    ) -> Result<MinimaxAssistantState, ProviderError> {
        if !matches!(self.state.finish_reason.as_str(), "stop" | "tool_calls")
            || (self.state.finish_reason == "tool_calls") == calls.is_empty()
        {
            return Err(invalid());
        }
        if !self.state.tool_call_indexes.is_empty()
            && self.state.tool_call_indexes.len() != calls.len()
        {
            return Err(invalid());
        }
        self.charge(serde_json::to_vec(calls).map_err(|_| invalid())?.len() + calls.len() * 128)?;
        self.state.calls = calls.to_vec();
        self.state.retained_bytes = self.bytes_seen;
        Ok(self.state)
    }

    fn charge(&mut self, bytes: usize) -> Result<(), ProviderError> {
        self.bytes_seen = self.bytes_seen.checked_add(bytes).ok_or_else(invalid)?;
        if self.bytes_seen > MAX_STATE_BYTES {
            Err(invalid())
        } else {
            Ok(())
        }
    }
}

fn bounded_metadata(value: &str) -> Result<(), ProviderError> {
    if value.is_empty() || value.len() > MAX_ID_BYTES || value.chars().any(char::is_control) {
        Err(invalid())
    } else {
        Ok(())
    }
}

fn merge_metadata(old: &mut Option<String>, new: &Option<String>) -> Result<(), ProviderError> {
    if let Some(new) = new {
        if old.as_ref().is_some_and(|old| old != new) {
            return Err(invalid());
        }
        *old = Some(new.clone());
    }
    Ok(())
}

fn append_fragment(target: &mut Option<String>, fragment: Option<&str>) {
    if let Some(fragment) = fragment {
        target.get_or_insert_with(String::new).push_str(fragment);
    }
}

fn fragment_bytes(value: &str) -> Result<usize, ProviderError> {
    // JSON string escaping is additive across valid UTF-8 fragments. Exclude
    // the two quote bytes so repeated frames cannot charge another field.
    Ok(serde_json::to_vec(value)
        .map_err(|_| invalid())?
        .len()
        .saturating_sub(2))
}
