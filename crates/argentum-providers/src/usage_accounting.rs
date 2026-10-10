use super::*;
use argentum_domain::goal_accounting::ObservedUsage;

#[derive(Deserialize)]
struct UsageReport {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: u64,
    prompt_tokens_details: Option<PromptTokenDetails>,
    completion_tokens_details: Option<CompletionTokenDetails>,
}

#[derive(Default)]
pub(super) struct StreamUsage {
    latest: Option<ObservedUsage>,
    final_report: bool,
}

impl StreamUsage {
    pub(super) fn accept(
        &mut self,
        raw: serde_json::Value,
        final_report: bool,
    ) -> Result<Option<ObservedUsage>, ProviderError> {
        let report: UsageReport = serde_json::from_value(raw)
            .map_err(|_| invalid_tool_response("malformed provider usage"))?;
        let usage = ObservedUsage {
            input_tokens: report.prompt_tokens,
            output_tokens: report.completion_tokens,
            total_tokens: report.total_tokens,
            cached_input_tokens: report
                .prompt_tokens_details
                .and_then(|details| details.cached_tokens),
            reasoning_tokens: report
                .completion_tokens_details
                .and_then(|details| details.reasoning_tokens),
        };
        usage.validate().map_err(invalid_tool_response)?;
        if let Some(previous) = &self.latest {
            let decreasing = usage.total_tokens < previous.total_tokens
                || matches!((previous.input_tokens, usage.input_tokens), (Some(a), Some(b)) if b < a)
                || matches!((previous.output_tokens, usage.output_tokens), (Some(a), Some(b)) if b < a);
            if decreasing
                || (self.final_report && &usage != previous)
                || (usage.total_tokens == previous.total_tokens && &usage != previous)
            {
                return Err(invalid_tool_response("conflicting provider usage"));
            }
            if self.final_report && &usage == previous {
                return Ok(None);
            }
        }
        self.latest = Some(usage.clone());
        self.final_report = final_report;
        Ok(final_report.then_some(usage))
    }

    pub(super) fn complete_components(
        &self,
        context_window_tokens: Option<u64>,
    ) -> Option<ModelUsage> {
        if !self.final_report {
            return None;
        }
        let usage = self.latest.as_ref()?;
        Some(ModelUsage {
            input_tokens: usage.input_tokens?,
            output_tokens: usage.output_tokens?,
            total_tokens: usage.total_tokens,
            cached_input_tokens: usage.cached_input_tokens,
            reasoning_tokens: usage.reasoning_tokens,
            context_window_tokens,
        })
    }
}
