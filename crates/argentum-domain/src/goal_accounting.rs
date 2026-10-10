//! Client-side estimated admission and observed usage. Not a billing ceiling.
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use uuid::Uuid;

pub const ACCOUNTING_VERSION: u16 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalBinding {
    pub goal_id: Uuid,
    pub revision: Uuid,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunLimits {
    pub token_limit: Option<u64>,
    pub request_limit: Option<u32>,
    pub tool_limit: Option<u32>,
    pub elapsed_seconds: Option<u64>,
}

impl RunLimits {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.token_limit.is_some_and(|n| n == 0 || n > 10_000_000)
            || self.request_limit.is_some_and(|n| n == 0 || n > 100_000)
            || self.tool_limit.is_some_and(|n| n == 0 || n > 100_000)
            || self.elapsed_seconds.is_some_and(|n| n == 0 || n > 604_800)
        {
            return Err("invalid finite run limits");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Counters {
    pub observed_tokens: u64,
    #[serde(default)]
    pub owner_accepted_tokens: u64,
    #[serde(default)]
    pub later_usage_excess_tokens: u64,
    pub outstanding_tokens: u64,
    pub unresolved_tokens: u64,
    pub request_attempts: u32,
    pub tool_admissions: u32,
    pub iterations: u32,
    pub elapsed_milliseconds: u64,
    pub overrun_tokens: u64,
}

impl Counters {
    /// Admission includes nonrefundable policy charges, not just provider usage.
    pub fn effective_tokens(&self) -> u64 {
        self.observed_tokens
            .saturating_add(self.owner_accepted_tokens)
            .saturating_add(self.later_usage_excess_tokens)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalAccounting {
    pub version: u16,
    /// Required in v2. Older writers discard it; such a record must fail closed.
    #[serde(default)]
    pub disposition_format: Option<u16>,
    pub revision: Uuid,
    pub started_at: OffsetDateTime,
    pub incomplete_history: bool,
    pub counters: Counters,
    pub request_limit: Option<u32>,
    pub iteration_limit: Option<u32>,
    pub per_run: RunLimits,
    /// Explicit local-owner amendment, never inferred from absent limits.
    #[serde(default)]
    pub owner_removed_aggregate_limits: bool,
    pub stop_reason: Option<String>,
}

impl GoalAccounting {
    pub fn new(incomplete_history: bool) -> Self {
        Self {
            version: ACCOUNTING_VERSION,
            disposition_format: Some(1),
            revision: Uuid::new_v4(),
            started_at: crate::now(),
            incomplete_history,
            counters: Counters::default(),
            request_limit: None,
            iteration_limit: None,
            per_run: RunLimits::default(),
            owner_removed_aggregate_limits: false,
            stop_reason: incomplete_history.then(|| "legacy_usage_history_unavailable".into()),
        }
    }
}

/// Components remain unavailable when only a trustworthy total was reported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObservedUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: u64,
    pub cached_input_tokens: Option<u64>,
    pub reasoning_tokens: Option<u64>,
}

impl ObservedUsage {
    pub fn quality_label(&self) -> &'static str {
        if self.total_tokens == 0 {
            "reported_zero"
        } else if self.input_tokens.is_none() || self.output_tokens.is_none() {
            "reported_total_components_unavailable"
        } else {
            "reported_components_and_total"
        }
    }
    pub fn validate(&self) -> Result<(), &'static str> {
        if let (Some(input), Some(output)) = (self.input_tokens, self.output_tokens) {
            if input.checked_add(output) != Some(self.total_tokens) {
                return Err("inconsistent usage total");
            }
        }
        if self.input_tokens.is_some_and(|n| n > self.total_tokens)
            || self.output_tokens.is_some_and(|n| n > self.total_tokens)
            || self
                .cached_input_tokens
                .is_some_and(|n| self.input_tokens.map_or(true, |bound| n > bound))
            || self
                .reasoning_tokens
                .is_some_and(|n| self.output_tokens.map_or(true, |bound| n > bound))
        {
            return Err("inconsistent usage components");
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChargeState {
    Reserved,
    Dispatched,
    Settled,
    Unresolved,
    NotDispatched,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestCharge {
    pub request_id: Uuid,
    pub run_id: Uuid,
    pub binding: GoalBinding,
    pub limit_revision: Uuid,
    pub goal_token_limit: Option<u64>,
    pub run_token_limit: Option<u64>,
    pub profile_id: String,
    pub model_budget_revision: Uuid,
    pub purpose: String,
    pub method: String,
    pub reserved_tokens: u64,
    pub state: ChargeState,
    #[serde(default)]
    pub usage_quality: String,
    pub observed: Option<ObservedUsage>,
    pub overrun_tokens: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disposition: Option<AccountingDisposition>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountingRecoveryBinding {
    pub store_id: Uuid,
    pub physical_store_sha256: crate::Sha256Digest,
    pub project_id: Uuid,
    pub workspace_key: String,
    pub session_id: Uuid,
    pub goal_id: Uuid,
    pub request_id: Uuid,
    pub run_id: Uuid,
    pub owner_generation: u64,
    pub owner_id: Uuid,
    pub accounting_revision: Uuid,
    pub accounting_snapshot_sha256: crate::Sha256Digest,
    pub reserved_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum AccountingDecision {
    OwnerAcceptReservation,
    AuthoritativeUsage { event_id: u64, usage: ObservedUsage },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountingResolution {
    pub command_id: String,
    pub binding: AccountingRecoveryBinding,
    pub decision: AccountingDecision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountingDisposition {
    pub resolution: AccountingResolution,
    pub recorded_at: OffsetDateTime,
    pub resolving_owner_generation: u64,
    pub client_charge_tokens: u64,
    pub later_observed_usage: Option<ObservedUsage>,
    #[serde(default)]
    pub conflicting_later_usage: Vec<ObservedUsage>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountingRecoveryInspection {
    pub binding: AccountingRecoveryBinding,
    pub charge: RequestCharge,
    pub run: RunAccounting,
    pub goal_counters: Counters,
    pub ownership: crate::recovery::RunRecoveryRecord,
    pub recoverable: bool,
    pub blocker: Option<String>,
    pub provider_usage_complete: bool,
    pub mutation_performed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RemainingAllowance {
    pub estimated_tokens: Option<u64>,
    pub reason: String,
    pub method: String,
    pub provider_billing_ceiling: bool,
    pub dollar_cost_enforcement: bool,
    pub autonomous_continuation: bool,
    pub automatic_verified_completion: bool,
    pub effective_client_charge_tokens: u64,
    pub owner_accepted_nonrefundable_tokens: u64,
    pub provider_usage_coverage_complete: bool,
}

impl GoalAccounting {
    pub fn supported_format(&self) -> bool {
        (self.version == ACCOUNTING_VERSION && self.disposition_format == Some(1))
            || (self.version == 1
                && self.disposition_format.is_none()
                && self.counters.owner_accepted_tokens == 0
                && self.counters.later_usage_excess_tokens == 0)
    }

    pub fn remaining(&self, budget: Option<u64>) -> RemainingAllowance {
        let reason = if !self.supported_format() {
            "unsupported_accounting_version"
        } else if self.stop_reason.as_deref() == Some("goal_accounting_unresolved") {
            "goal_accounting_unresolved"
        } else if self.incomplete_history {
            "legacy_usage_history_unavailable"
        } else if self.counters.outstanding_tokens > 0 || self.counters.unresolved_tokens > 0 {
            "outstanding_or_unresolved_usage"
        } else if self.counters.overrun_tokens > 0 {
            "observed_overrun_blocks_goal_work"
        } else if budget.is_none() {
            "no_cumulative_token_limit_configured"
        } else {
            "estimated_admission_allowance_not_billing_guarantee"
        };
        RemainingAllowance {
            estimated_tokens: (reason == "estimated_admission_allowance_not_billing_guarantee")
                .then(|| {
                    budget
                        .unwrap_or(0)
                        .saturating_sub(self.counters.effective_tokens())
                }),
            reason: reason.into(),
            method:
                "estimated_utf8_wire_bytes_v1_not_tokenizer_upper_bound_with_observed_settlement"
                    .into(),
            provider_billing_ceiling: false,
            dollar_cost_enforcement: false,
            autonomous_continuation: false,
            automatic_verified_completion: false,
            effective_client_charge_tokens: self.counters.effective_tokens(),
            owner_accepted_nonrefundable_tokens: self.counters.owner_accepted_tokens,
            provider_usage_coverage_complete: !self.incomplete_history
                && self.counters.owner_accepted_tokens == 0
                && self.counters.outstanding_tokens == 0
                && self.counters.unresolved_tokens == 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunAccounting {
    pub run_id: Uuid,
    pub project_id: Uuid,
    pub session_id: Uuid,
    pub binding: GoalBinding,
    pub limits: RunLimits,
    pub counters: Counters,
    pub admitted_at: OffsetDateTime,
    pub finished_at: Option<OffsetDateTime>,
    pub stop_reason: Option<String>,
    pub maintenance: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalInspection {
    pub recent_runs: Vec<RunAccounting>,
    pub requests: Vec<RequestCharge>,
    pub runs_truncated: bool,
    pub requests_truncated: bool,
}
