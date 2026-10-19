//! Bounded, metadata-only diagnostics for one retained run.
use super::*;
use argentum_domain::{
    context_summary::CompactionStatus, goal_accounting::ChargeState, AppEvent, ProviderKind,
    TaskLifecycle, ToolResultState, VerificationRunnerId, VerificationState,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const RUN_DIAGNOSTICS_SCHEMA_VERSION: u16 = 1;
pub const RUN_DIAGNOSTICS_MAX_EVENT_ROWS: usize = 256;
pub const RUN_DIAGNOSTICS_MAX_EVENT_PAYLOAD_BYTES: usize = 16 * 1024;
pub const RUN_DIAGNOSTICS_MAX_SOURCE_BYTES: usize = 256 * 1024;
pub const RUN_DIAGNOSTICS_MAX_CHANGES: usize = 128;
pub const RUN_DIAGNOSTICS_MAX_VERIFICATIONS: usize = 32;
pub const RUN_DIAGNOSTICS_MAX_JSON_BYTES: usize = 256 * 1024;
pub const RUN_DIAGNOSTICS_MAX_SOURCE_KEYS: usize = 3;
pub const RUN_DIAGNOSTICS_MAX_CACHED_PAGES: usize = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiagnosticQuality {
    Observed,
    DerivedComplete,
    Partial,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticValue<T> {
    pub quality: DiagnosticQuality,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

impl<T> DiagnosticValue<T> {
    fn observed(value: T) -> Self {
        Self {
            quality: DiagnosticQuality::Observed,
            value: Some(value),
            reason: None,
        }
    }
    fn derived(value: T, complete: bool) -> Self {
        Self {
            quality: if complete {
                DiagnosticQuality::DerivedComplete
            } else {
                DiagnosticQuality::Partial
            },
            value: Some(value),
            reason: (!complete).then(|| "bounded_source_window".into()),
        }
    }
    fn unavailable(reason: &'static str) -> Self {
        Self {
            quality: DiagnosticQuality::Unavailable,
            value: None,
            reason: Some(reason.into()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticGenerator {
    pub product: String,
    pub package_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunOutcomeDiagnostics {
    pub stored_state: DiagnosticValue<String>,
    pub final_state: DiagnosticValue<bool>,
    pub task_accepted: DiagnosticValue<bool>,
    pub goal_completed: DiagnosticValue<bool>,
    pub run_error_recorded: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderDiagnostics {
    pub historical: bool,
    pub backend_class: DiagnosticValue<String>,
    pub mode: DiagnosticValue<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RequestAttemptDiagnostics {
    pub request_id: Uuid,
    pub purpose: String,
    pub attempt_number: u8,
    pub maximum_attempts: u8,
    pub physical_attempts: u16,
    pub retry_schedules: u16,
    pub planned_wait_ms: u64,
    pub last_outcome: String,
    pub last_retry_class: DiagnosticValue<String>,
    pub last_http_status: DiagnosticValue<u16>,
    pub stop_reason: DiagnosticValue<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelRequestDiagnostics {
    pub logical_rounds: DiagnosticValue<u32>,
    pub physical_attempts: DiagnosticValue<u32>,
    pub retry_schedules: DiagnosticValue<u32>,
    pub planned_wait_ms: DiagnosticValue<u64>,
    pub attempts: Vec<RequestAttemptDiagnostics>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageDiagnostics {
    pub sequence: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub total_tokens: u64,
    pub reasoning_tokens: DiagnosticValue<u64>,
    pub cached_input_tokens: DiagnosticValue<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccountingDiagnostics {
    pub request_id: Uuid,
    pub state: String,
    pub reserved_tokens: u64,
    pub observed_total_tokens: DiagnosticValue<u64>,
    pub overrun_tokens: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ToolDiagnostics {
    pub call_id: Uuid,
    pub tool: String,
    pub started: DiagnosticValue<bool>,
    pub outcome: DiagnosticValue<String>,
    pub duration_ms: DiagnosticValue<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalDiagnostics {
    pub approval_id: Uuid,
    pub tool: String,
    pub outcome: DiagnosticValue<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeDiagnostics {
    pub change_id: Uuid,
    pub change_set_id: Uuid,
    pub revision: u64,
    pub kind: String,
    pub origin: String,
    pub restored_from: DiagnosticValue<Uuid>,
    pub additions: u32,
    pub removals: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChangeOperationDiagnostics {
    pub operation_id: Uuid,
    pub kind: String,
    pub origin: String,
    pub state: String,
    pub file_change_id: DiagnosticValue<Uuid>,
    pub restored_from: DiagnosticValue<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerificationDiagnostics {
    pub verification_id: Uuid,
    pub runner_class: String,
    pub state: String,
    pub revision_before: String,
    pub revision_after: String,
    pub duration_ms: u64,
    pub exit_code: DiagnosticValue<i32>,
    pub stdout_bytes: u64,
    pub stderr_bytes: u64,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextDiagnostics {
    pub request_id: Uuid,
    pub purpose: String,
    pub accounted_input: u64,
    pub output_reserve: u32,
    pub usable_context: u64,
    pub maximum_output: u32,
    pub fits: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactionDiagnostics {
    pub status: String,
    pub source_reference_count: u32,
    pub checkpoint_retained: bool,
    pub observed_usage_total: DiagnosticValue<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SteeringDiagnostics {
    pub entry_id: Uuid,
    pub position: u64,
    pub text_bytes: u32,
    pub lifecycle: String,
    pub reason: DiagnosticValue<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvenanceDiagnostics {
    pub instruction_sources: DiagnosticValue<u32>,
    pub selected_skill: DiagnosticValue<bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticCoverage {
    pub complete: bool,
    pub event_rows_decoded: u32,
    pub event_rows_omitted: u32,
    pub oversized_event_rows: u32,
    pub invalid_event_rows: u32,
    pub source_bytes_decoded: u64,
    pub change_rows_returned: u32,
    pub change_rows_omitted: u32,
    pub operation_rows_returned: u32,
    pub operation_rows_omitted: u32,
    pub verification_rows_returned: u32,
    pub verification_rows_omitted: u32,
    pub limits: DiagnosticLimits,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiagnosticLimits {
    pub source_keys: u32,
    pub cached_pages: u32,
    pub event_rows: u32,
    pub event_payload_bytes: u32,
    pub source_bytes: u32,
    pub change_rows: u32,
    pub verification_rows: u32,
    pub json_bytes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RunDiagnosticsReport {
    pub schema_version: u16,
    pub metadata_only: bool,
    pub snapshot: bool,
    pub generator: DiagnosticGenerator,
    pub historical_executable: DiagnosticValue<String>,
    pub project_id: ProjectId,
    pub session_id: DiagnosticValue<SessionId>,
    pub run_id: RunId,
    pub outcome: RunOutcomeDiagnostics,
    pub provider: ProviderDiagnostics,
    pub model_requests: ModelRequestDiagnostics,
    pub elapsed_ms: DiagnosticValue<u64>,
    pub usage_records: DiagnosticValue<u32>,
    pub usage: Vec<UsageDiagnostics>,
    pub accounting: Vec<AccountingDiagnostics>,
    pub tools: Vec<ToolDiagnostics>,
    pub approvals: Vec<ApprovalDiagnostics>,
    pub changes: Vec<ChangeDiagnostics>,
    pub change_operations: Vec<ChangeOperationDiagnostics>,
    pub verification: Vec<VerificationDiagnostics>,
    pub context: Vec<ContextDiagnostics>,
    pub compaction: Vec<CompactionDiagnostics>,
    pub steering: Vec<SteeringDiagnostics>,
    pub provenance: ProvenanceDiagnostics,
    pub coverage: DiagnosticCoverage,
}

#[derive(Default)]
struct AttemptAccumulator {
    purpose: String,
    attempt_number: u8,
    maximum_attempts: u8,
    physical_attempts: u16,
    retry_schedules: u16,
    planned_wait_ms: u64,
    last_outcome: String,
    last_retry_class: Option<String>,
    last_status: Option<u16>,
    stop_reason: Option<String>,
}

#[derive(Default)]
struct ToolAccumulator {
    tool: String,
    started: bool,
    outcome: Option<String>,
    duration_ms: Option<u64>,
}

fn safe_purpose(value: &str) -> String {
    match value {
        "main_generation" | "compaction_tools_disabled" => value.into(),
        _ => "other".into(),
    }
}

fn safe_retry_class(value: &str) -> Option<String> {
    matches!(value, "pre_dispatch_connection" | "transient_http_status").then(|| value.into())
}

fn safe_stop_reason(value: &str) -> Option<String> {
    matches!(
        value,
        "completed"
            | "non_retryable_error"
            | "retry_exhausted_or_wait_bound"
            | "wait_bound"
            | "cancelled_during_backoff"
    )
    .then(|| value.into())
}

fn safe_steering_reason(value: &str) -> Option<String> {
    matches!(
        value,
        "run_completed_before_inclusion"
            | "run_cancelled_before_inclusion"
            | "run_failed_before_inclusion"
    )
    .then(|| value.into())
}

fn safe_tool(value: &str) -> String {
    match value {
        "list_directory"
        | "file_metadata"
        | "read_text"
        | "read_binary_metadata"
        | "search_text"
        | "search_filenames"
        | "create_directory"
        | "apply_patch"
        | "write_text"
        | "edit_text"
        | "move_file"
        | "copy_file"
        | "delete_file"
        | "execute_argv"
        | "execute_shell"
        | "git_read"
        | "git_write"
        | "verify"
        | "start_verification"
        | "poll_process_job"
        | "cancel_process_job"
        | "restore_file"
        | "recovery_restore" => value.into(),
        _ => "other".into(),
    }
}

fn safe_operation_kind(value: &str) -> String {
    match value {
        "added" | "modified" | "deleted" => value.into(),
        _ => "unavailable".into(),
    }
}

fn safe_operation_origin(value: &str) -> String {
    match value {
        "tool_call" | "restore" => value.into(),
        _ => "unavailable".into(),
    }
}

fn safe_operation_state(value: &str) -> String {
    match value {
        "prepared" | "applied" | "finalized" | "not_applied" | "conflicted" => value.into(),
        _ => "unavailable".into(),
    }
}

fn safe_digest(value: String) -> String {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        value.to_ascii_lowercase()
    } else {
        "unavailable".into()
    }
}

fn lifecycle_name(value: TaskLifecycle) -> String {
    format!("{value:?}").to_ascii_lowercase()
}

fn provider_class(value: ProviderKind) -> (&'static str, &'static str) {
    match value {
        ProviderKind::LocalLlamaCpp => ("llama_cpp", "local"),
        ProviderKind::LocalLmStudio => ("openai_compatible", "local_server"),
        ProviderKind::OpenAiCompatible => ("openai_compatible", "hosted_or_server"),
        ProviderKind::Anthropic => ("anthropic", "hosted"),
        ProviderKind::Unknown => ("unknown", "unknown"),
    }
}

fn verification_state(value: &VerificationState) -> String {
    format!("{value:?}").to_ascii_lowercase()
}

fn runner_class(value: &VerificationRunnerId) -> String {
    if value.project_id().is_some() {
        "project_defined".into()
    } else {
        "fixed".into()
    }
}

fn parse_id(value: String, label: &str) -> Result<Uuid, StoreError> {
    value
        .parse()
        .map_err(|_| StoreError::InvalidRecord(format!("invalid {label}")))
}

impl Store {
    pub fn run_diagnostics(
        &self,
        project_id: ProjectId,
        expected_session_id: Option<SessionId>,
        run_id: RunId,
    ) -> Result<Option<RunDiagnosticsReport>, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let expected_session = expected_session_id.map(|value| value.to_string());
        let exists: bool = connection.query_row(
            "SELECT EXISTS(
               SELECT 1 FROM event_log WHERE project_id=?1 AND run_id=?2 AND (?3 IS NULL OR session_id=?3)
               UNION ALL SELECT 1 FROM file_changes WHERE project_id=?1 AND run_id=?2 AND (?3 IS NULL OR session_id=?3)
               UNION ALL SELECT 1 FROM change_operations WHERE project_id=?1 AND run_id=?2 AND (?3 IS NULL OR session_id=?3)
               UNION ALL SELECT 1 FROM verification_evidence WHERE project_id=?1 AND run_id=?2 AND (?3 IS NULL OR session_id=?3)
             )",
            params![project_id.to_string(), run_id.to_string(), expected_session],
            |row| row.get(0),
        )?;
        if !exists {
            return Ok(None);
        }

        let session_text: Option<String> = connection.query_row(
            "SELECT session_id FROM (
               SELECT session_id, 1 AS rank FROM event_log WHERE project_id=?1 AND run_id=?2 AND length(CAST(session_id AS BLOB))=36
               UNION ALL SELECT session_id, 2 FROM file_changes WHERE project_id=?1 AND run_id=?2 AND length(CAST(session_id AS BLOB))=36
               UNION ALL SELECT session_id, 3 FROM change_operations WHERE project_id=?1 AND run_id=?2 AND length(CAST(session_id AS BLOB))=36
               UNION ALL SELECT session_id, 4 FROM verification_evidence WHERE project_id=?1 AND run_id=?2 AND length(CAST(session_id AS BLOB))=36
             ) ORDER BY rank LIMIT 1",
            params![project_id.to_string(), run_id.to_string()],
            |row| row.get(0),
        ).optional()?;
        let session_id = session_text
            .map(|value| parse_id(value, "diagnostic session id"))
            .transpose()?;
        if expected_session_id.is_some() && session_id != expected_session_id {
            return Err(StoreError::InvalidRecord(
                "diagnostic session scope mismatch".into(),
            ));
        }

        let event_total: usize = connection.query_row(
            "SELECT COUNT(*) FROM event_log
             WHERE project_id=?1 AND run_id=?2 AND (?3 IS NULL OR session_id=?3)
               AND event_type IN ('task_accepted','run_status_changed','model_usage_updated',
                 'context_request_accounted','model_request_attempt','goal_request_accounting',
                 'tool_started','tool_finished','approval_requested','approval_resolved',
                 'run_error','context_compaction_completed','steering_changed')",
            params![project_id.to_string(), run_id.to_string(), expected_session],
            |row| row.get(0),
        )?;
        let mut statement = connection.prepare(
            "SELECT id, created_at, event_type,
                    CASE WHEN event_type IN ('task_accepted','run_error') THEN 0 ELSE length(CAST(payload AS BLOB)) END,
                    CASE WHEN event_type IN ('task_accepted','run_error') THEN NULL
                         WHEN length(CAST(payload AS BLOB))<=?4 THEN payload ELSE NULL END
             FROM event_log
             WHERE project_id=?1 AND run_id=?2 AND (?3 IS NULL OR session_id=?3)
               AND event_type IN ('task_accepted','run_status_changed','model_usage_updated',
                 'context_request_accounted','model_request_attempt','goal_request_accounting',
                 'tool_started','tool_finished','approval_requested','approval_resolved',
                 'run_error','context_compaction_completed','steering_changed')
             ORDER BY id DESC LIMIT ?5",
        )?;
        let rows = statement.query_map(
            params![
                project_id.to_string(),
                run_id.to_string(),
                expected_session,
                RUN_DIAGNOSTICS_MAX_EVENT_PAYLOAD_BYTES as i64,
                (RUN_DIAGNOSTICS_MAX_EVENT_ROWS + 1) as i64
            ],
            |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, i64>(3)?,
                    row.get::<_, Option<String>>(4)?,
                ))
            },
        )?;
        let mut raw_rows = Vec::new();
        for row in rows {
            raw_rows.push(row?);
        }
        let row_limited = event_total > RUN_DIAGNOSTICS_MAX_EVENT_ROWS;
        raw_rows.truncate(RUN_DIAGNOSTICS_MAX_EVENT_ROWS);
        raw_rows.reverse();

        let mut decoded = 0_u32;
        let mut omitted = event_total
            .saturating_sub(RUN_DIAGNOSTICS_MAX_EVENT_ROWS)
            .min(u32::MAX as usize) as u32;
        let mut oversized = 0_u32;
        let mut invalid = 0_u32;
        let mut source_bytes = 0_usize;
        let mut lifecycle = None;
        let mut accepted = false;
        let mut run_error_recorded = false;
        let mut event_first = None;
        let mut event_last = None;
        let mut provider_kinds = BTreeSet::new();
        let mut attempts: BTreeMap<Uuid, AttemptAccumulator> = BTreeMap::new();
        let mut logical_rounds = 0_u32;
        let mut usage = Vec::new();
        let mut accounting = Vec::new();
        let mut tools: BTreeMap<Uuid, ToolAccumulator> = BTreeMap::new();
        let mut approvals: BTreeMap<Uuid, ApprovalDiagnostics> = BTreeMap::new();
        let mut context = Vec::new();
        let mut compaction = Vec::new();
        let mut steering = Vec::new();

        for (sequence, created_at, event_type, payload_bytes, payload) in raw_rows {
            event_first.get_or_insert(created_at);
            event_last = Some(created_at);
            if event_type == "task_accepted" {
                accepted = true;
                continue;
            }
            if event_type == "run_error" {
                run_error_recorded = true;
                continue;
            }
            let Ok(payload_bytes) = usize::try_from(payload_bytes) else {
                invalid += 1;
                continue;
            };
            let Some(payload) = payload else {
                oversized += 1;
                omitted += 1;
                continue;
            };
            if source_bytes.saturating_add(payload_bytes) > RUN_DIAGNOSTICS_MAX_SOURCE_BYTES {
                omitted += 1;
                continue;
            }
            source_bytes += payload_bytes;
            let Ok(event) = serde_json::from_str::<AppEvent>(&payload) else {
                invalid += 1;
                continue;
            };
            decoded += 1;
            match event {
                AppEvent::RunStatusChanged {
                    run_id: stored,
                    lifecycle: value,
                } if stored == run_id => lifecycle = Some(value),
                AppEvent::ContextRequestAccounted {
                    run_id: stored,
                    request_id,
                    purpose,
                    accounting: value,
                    ..
                } if stored == run_id => {
                    provider_kinds.insert(format!("{:?}", value.provider_kind));
                    context.push(ContextDiagnostics {
                        request_id,
                        purpose: safe_purpose(&purpose),
                        accounted_input: value.accounted_input,
                        output_reserve: value.output_reserve,
                        usable_context: value.usable_context,
                        maximum_output: value.maximum_output,
                        fits: value.fits,
                    });
                }
                AppEvent::ModelRequestAttempt {
                    run_id: stored,
                    request_id,
                    purpose,
                    outcome,
                    error_class,
                    status,
                    planned_delay_ms,
                    stop_reason,
                    attempt,
                    max_attempts,
                    ..
                } if stored == run_id => {
                    let item = attempts.entry(request_id).or_default();
                    item.purpose = safe_purpose(&purpose);
                    item.attempt_number = attempt;
                    item.maximum_attempts = max_attempts;
                    item.last_outcome = match outcome.as_str() {
                        "started" | "stopped" | "retry_scheduled" => outcome,
                        _ => "other".into(),
                    };
                    if item.last_outcome == "started" {
                        item.physical_attempts = item.physical_attempts.saturating_add(1);
                        if attempt == 1 {
                            logical_rounds = logical_rounds.saturating_add(1);
                        }
                    }
                    if item.last_outcome == "retry_scheduled" {
                        item.retry_schedules = item.retry_schedules.saturating_add(1);
                    }
                    item.planned_wait_ms = item
                        .planned_wait_ms
                        .saturating_add(planned_delay_ms.unwrap_or(0));
                    if let Some(value) = error_class.as_deref().and_then(safe_retry_class) {
                        item.last_retry_class = Some(value);
                    }
                    if status.is_some() {
                        item.last_status = status;
                    }
                    if let Some(value) = stop_reason.as_deref().and_then(safe_stop_reason) {
                        item.stop_reason = Some(value);
                    }
                }
                AppEvent::ModelUsageUpdated {
                    run_id: stored,
                    usage: value,
                    ..
                } if stored == run_id => usage.push(UsageDiagnostics {
                    sequence: sequence as u64,
                    input_tokens: value.input_tokens,
                    output_tokens: value.output_tokens,
                    total_tokens: value.total_tokens,
                    reasoning_tokens: value
                        .reasoning_tokens
                        .map(DiagnosticValue::observed)
                        .unwrap_or_else(|| {
                            DiagnosticValue::unavailable("provider_component_not_reported")
                        }),
                    cached_input_tokens: value
                        .cached_input_tokens
                        .map(DiagnosticValue::observed)
                        .unwrap_or_else(|| {
                            DiagnosticValue::unavailable("provider_component_not_reported")
                        }),
                }),
                AppEvent::GoalRequestAccounting {
                    run_id: stored,
                    charge,
                    ..
                } if stored == run_id => accounting.push(AccountingDiagnostics {
                    request_id: charge.request_id,
                    state: match charge.state {
                        ChargeState::Reserved => "reserved",
                        ChargeState::Dispatched => "dispatched",
                        ChargeState::Settled => "settled",
                        ChargeState::Unresolved => "unresolved",
                        ChargeState::NotDispatched => "not_dispatched",
                    }
                    .into(),
                    reserved_tokens: charge.reserved_tokens,
                    observed_total_tokens: charge
                        .observed
                        .as_ref()
                        .map(|value| DiagnosticValue::observed(value.total_tokens))
                        .unwrap_or_else(|| {
                            DiagnosticValue::unavailable("observed_usage_not_retained")
                        }),
                    overrun_tokens: charge.overrun_tokens,
                }),
                AppEvent::ToolStarted(trace) if trace.run_id == run_id => {
                    let item = tools.entry(trace.id).or_default();
                    item.tool = safe_tool(&trace.tool_id);
                    item.started = true;
                }
                AppEvent::ToolFinished(trace) if trace.run_id == run_id => {
                    let item = tools.entry(trace.id).or_default();
                    item.tool = safe_tool(&trace.tool_id);
                    item.outcome = Some(
                        match trace.result {
                            ToolResultState::Running => "running",
                            ToolResultState::Succeeded => "succeeded",
                            ToolResultState::Failed => "failed",
                            ToolResultState::Cancelled => "cancelled",
                            ToolResultState::NotExecuted => "not_executed",
                        }
                        .into(),
                    );
                    item.duration_ms = trace.duration_ms;
                }
                AppEvent::ApprovalRequested(request) if request.run_id == run_id => {
                    approvals.insert(
                        request.id,
                        ApprovalDiagnostics {
                            approval_id: request.id,
                            tool: safe_tool(&request.tool_id),
                            outcome: DiagnosticValue::observed("pending".into()),
                        },
                    );
                }
                AppEvent::ApprovalResolved {
                    approval_id,
                    approved,
                } => {
                    if let Some(item) = approvals.get_mut(&approval_id) {
                        item.outcome = DiagnosticValue::observed(if approved {
                            "approved".into()
                        } else {
                            "denied".into()
                        });
                    }
                }
                AppEvent::ContextCompactionCompleted { result, .. } => {
                    compaction.push(CompactionDiagnostics {
                        status: match result.status {
                            CompactionStatus::Success => "success",
                            CompactionStatus::NoOp => "no_op",
                            CompactionStatus::Failed => "failed",
                            CompactionStatus::Deactivated => "deactivated",
                        }
                        .into(),
                        source_reference_count: result
                            .source_references
                            .len()
                            .min(u32::MAX as usize)
                            as u32,
                        checkpoint_retained: result.checkpoint_id.is_some(),
                        observed_usage_total: result
                            .observed_usage
                            .as_ref()
                            .map(|value| DiagnosticValue::observed(value.total_tokens))
                            .unwrap_or_else(|| {
                                DiagnosticValue::unavailable("summarizer_usage_not_reported")
                            }),
                    })
                }
                AppEvent::SteeringChanged(status) if status.run_id == run_id => {
                    steering.push(SteeringDiagnostics {
                        entry_id: status.entry_id,
                        position: status.position,
                        text_bytes: status.text_bytes,
                        lifecycle: match status.lifecycle {
                            argentum_domain::SteeringLifecycle::Pending => "pending",
                            argentum_domain::SteeringLifecycle::Included => "included",
                            argentum_domain::SteeringLifecycle::NotApplied => "not_applied",
                        }
                        .into(),
                        reason: status
                            .reason
                            .as_deref()
                            .and_then(safe_steering_reason)
                            .map(DiagnosticValue::observed)
                            .unwrap_or_else(|| {
                                DiagnosticValue::unavailable("no_safe_reason_retained")
                            }),
                    });
                }
                _ => {}
            }
        }
        drop(statement);

        let events_complete = !row_limited && omitted == 0 && oversized == 0 && invalid == 0;
        let model_attempts: Vec<_> = attempts
            .into_iter()
            .map(|(request_id, item)| RequestAttemptDiagnostics {
                request_id,
                purpose: item.purpose,
                attempt_number: item.attempt_number,
                maximum_attempts: item.maximum_attempts,
                physical_attempts: item.physical_attempts,
                retry_schedules: item.retry_schedules,
                planned_wait_ms: item.planned_wait_ms,
                last_outcome: item.last_outcome,
                last_retry_class: item
                    .last_retry_class
                    .map(DiagnosticValue::observed)
                    .unwrap_or_else(|| DiagnosticValue::unavailable("no_retry_class_retained")),
                last_http_status: item
                    .last_status
                    .map(DiagnosticValue::observed)
                    .unwrap_or_else(|| DiagnosticValue::unavailable("no_http_status_retained")),
                stop_reason: item
                    .stop_reason
                    .map(DiagnosticValue::observed)
                    .unwrap_or_else(|| {
                        DiagnosticValue::unavailable("no_safe_stop_reason_retained")
                    }),
            })
            .collect();
        let physical_attempts = model_attempts
            .iter()
            .map(|item| u32::from(item.physical_attempts))
            .sum();
        let retry_schedules = model_attempts
            .iter()
            .map(|item| u32::from(item.retry_schedules))
            .sum();
        let planned_wait_ms = model_attempts.iter().map(|item| item.planned_wait_ms).sum();
        let attempt_metadata_available = !model_attempts.is_empty();
        let tool_items = tools
            .into_iter()
            .map(|(call_id, item)| ToolDiagnostics {
                call_id,
                tool: if item.tool.is_empty() {
                    "other".into()
                } else {
                    item.tool
                },
                started: if item.started {
                    DiagnosticValue::observed(true)
                } else {
                    DiagnosticValue::unavailable("start_event_not_retained")
                },
                outcome: item
                    .outcome
                    .map(DiagnosticValue::observed)
                    .unwrap_or_else(|| {
                        DiagnosticValue::unavailable("completion_event_not_retained")
                    }),
                duration_ms: item
                    .duration_ms
                    .map(DiagnosticValue::observed)
                    .unwrap_or_else(|| DiagnosticValue::unavailable("duration_not_retained")),
            })
            .collect();

        let change_total: usize = connection.query_row(
            "SELECT COUNT(*) FROM file_changes WHERE project_id=?1 AND run_id=?2 AND (?3 IS NULL OR session_id=?3)",
            params![project_id.to_string(), run_id.to_string(), expected_session],
            |row| row.get(0),
        )?;
        let mut change_statement = connection.prepare(
            "SELECT CASE WHEN length(CAST(payload AS BLOB))<=?4 THEN payload ELSE NULL END
             FROM file_changes WHERE project_id=?1 AND run_id=?2 AND (?3 IS NULL OR session_id=?3)
             ORDER BY revision,id LIMIT ?5",
        )?;
        let change_rows = change_statement.query_map(
            params![
                project_id.to_string(),
                run_id.to_string(),
                expected_session,
                RUN_DIAGNOSTICS_MAX_EVENT_PAYLOAD_BYTES as i64,
                (RUN_DIAGNOSTICS_MAX_CHANGES + 1) as i64
            ],
            |row| row.get::<_, Option<String>>(0),
        )?;
        let mut changes = Vec::new();
        for row in change_rows {
            let Some(payload) = row? else {
                continue;
            };
            if source_bytes.saturating_add(payload.len()) > RUN_DIAGNOSTICS_MAX_SOURCE_BYTES {
                continue;
            }
            source_bytes += payload.len();
            let Ok(record) = serde_json::from_str::<FileChangeRecord>(&payload) else {
                invalid += 1;
                continue;
            };
            if record.project_id != project_id || record.run_id != run_id {
                invalid += 1;
                continue;
            }
            changes.push(ChangeDiagnostics {
                change_id: record.id,
                change_set_id: record.change_set_id,
                revision: record.revision,
                kind: format!("{:?}", record.kind).to_ascii_lowercase(),
                origin: format!("{:?}", record.origin).to_ascii_lowercase(),
                restored_from: record
                    .restored_from
                    .map(DiagnosticValue::observed)
                    .unwrap_or_else(|| DiagnosticValue::unavailable("not_a_linked_restore")),
                additions: record.additions,
                removals: record.removals,
            });
        }
        let changes_limited = change_total > RUN_DIAGNOSTICS_MAX_CHANGES;
        changes.truncate(RUN_DIAGNOSTICS_MAX_CHANGES);
        drop(change_statement);

        let operation_total: usize = connection.query_row(
            "SELECT COUNT(*) FROM change_operations WHERE project_id=?1 AND run_id=?2 AND (?3 IS NULL OR session_id=?3)",
            params![project_id.to_string(), run_id.to_string(), expected_session],
            |row| row.get(0),
        )?;
        let mut operation_statement = connection.prepare(
            "SELECT
               CASE WHEN length(CAST(id AS BLOB))=36 THEN id ELSE '' END,
               CASE WHEN length(CAST(change_kind AS BLOB))<=32 THEN change_kind ELSE 'other' END,
               CASE WHEN length(CAST(origin AS BLOB))<=32 THEN origin ELSE 'other' END,
               CASE WHEN length(CAST(state AS BLOB))<=32 THEN state ELSE 'other' END,
               CASE WHEN file_change_id IS NULL THEN NULL WHEN length(CAST(file_change_id AS BLOB))=36 THEN file_change_id ELSE '' END,
               CASE WHEN restored_from IS NULL THEN NULL WHEN length(CAST(restored_from AS BLOB))=36 THEN restored_from ELSE '' END
             FROM change_operations
             WHERE project_id=?1 AND run_id=?2 AND (?3 IS NULL OR session_id=?3)
             ORDER BY created_at,id LIMIT ?4",
        )?;
        let operation_rows = operation_statement.query_map(
            params![
                project_id.to_string(),
                run_id.to_string(),
                expected_session,
                (RUN_DIAGNOSTICS_MAX_CHANGES + 1) as i64
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, Option<String>>(5)?,
                ))
            },
        )?;
        let mut operations = Vec::new();
        for row in operation_rows {
            let (id, kind, origin, state, file, restored) = row?;
            operations.push(ChangeOperationDiagnostics {
                operation_id: parse_id(id, "change operation id")?,
                kind: safe_operation_kind(&kind),
                origin: safe_operation_origin(&origin),
                state: safe_operation_state(&state),
                file_change_id: file
                    .map(|value| parse_id(value, "operation file change id"))
                    .transpose()?
                    .map(DiagnosticValue::observed)
                    .unwrap_or_else(|| DiagnosticValue::unavailable("operation_not_finalized")),
                restored_from: restored
                    .map(|value| parse_id(value, "restore source id"))
                    .transpose()?
                    .map(DiagnosticValue::observed)
                    .unwrap_or_else(|| DiagnosticValue::unavailable("not_a_restore_operation")),
            });
        }
        let operations_limited = operation_total > RUN_DIAGNOSTICS_MAX_CHANGES;
        operations.truncate(RUN_DIAGNOSTICS_MAX_CHANGES);
        drop(operation_statement);

        let verification_total: usize = connection.query_row(
            "SELECT COUNT(*) FROM verification_evidence WHERE project_id=?1 AND run_id=?2 AND (?3 IS NULL OR session_id=?3)",
            params![project_id.to_string(), run_id.to_string(), expected_session],
            |row| row.get(0),
        )?;
        let mut verification_statement = connection.prepare(
            "SELECT CASE WHEN length(CAST(payload AS BLOB))<=?4 THEN payload ELSE NULL END
             FROM verification_evidence WHERE project_id=?1 AND run_id=?2 AND (?3 IS NULL OR session_id=?3)
             ORDER BY recorded_at,id LIMIT ?5")?;
        let verification_rows = verification_statement.query_map(
            params![
                project_id.to_string(),
                run_id.to_string(),
                expected_session,
                RUN_DIAGNOSTICS_MAX_EVENT_PAYLOAD_BYTES as i64,
                (RUN_DIAGNOSTICS_MAX_VERIFICATIONS + 1) as i64
            ],
            |row| row.get::<_, Option<String>>(0),
        )?;
        let mut verification = Vec::new();
        for row in verification_rows {
            let Some(payload) = row? else {
                continue;
            };
            if source_bytes.saturating_add(payload.len()) > RUN_DIAGNOSTICS_MAX_SOURCE_BYTES {
                continue;
            }
            source_bytes += payload.len();
            let Ok(value) = serde_json::from_str::<VerificationEvidence>(&payload) else {
                invalid += 1;
                continue;
            };
            if value.run_id != run_id {
                invalid += 1;
                continue;
            }
            verification.push(VerificationDiagnostics {
                verification_id: value.id,
                runner_class: runner_class(&value.runner),
                state: verification_state(&value.state),
                revision_before: safe_digest(value.revision_before.0),
                revision_after: safe_digest(value.revision_after.0),
                duration_ms: value.duration_ms,
                exit_code: value
                    .exit_code
                    .map(DiagnosticValue::observed)
                    .unwrap_or_else(|| DiagnosticValue::unavailable("exit_code_not_retained")),
                stdout_bytes: value.stdout_bytes,
                stderr_bytes: value.stderr_bytes,
                stdout_truncated: value.stdout_truncated,
                stderr_truncated: value.stderr_truncated,
            });
        }
        let verification_limited = verification_total > RUN_DIAGNOSTICS_MAX_VERIFICATIONS;
        verification.truncate(RUN_DIAGNOSTICS_MAX_VERIFICATIONS);

        let provider = if provider_kinds.len() == 1 {
            let name = provider_kinds
                .iter()
                .next()
                .map(String::as_str)
                .unwrap_or("Unknown");
            let kind = match name {
                "LocalLlamaCpp" => ProviderKind::LocalLlamaCpp,
                "LocalLmStudio" => ProviderKind::LocalLmStudio,
                "OpenAiCompatible" => ProviderKind::OpenAiCompatible,
                "Anthropic" => ProviderKind::Anthropic,
                _ => ProviderKind::Unknown,
            };
            let (class, mode) = provider_class(kind);
            ProviderDiagnostics {
                historical: true,
                backend_class: DiagnosticValue::observed(class.into()),
                mode: DiagnosticValue::observed(mode.into()),
            }
        } else if provider_kinds.is_empty() {
            ProviderDiagnostics {
                historical: true,
                backend_class: DiagnosticValue::unavailable("provider_class_not_retained"),
                mode: DiagnosticValue::unavailable("provider_mode_not_retained"),
            }
        } else {
            ProviderDiagnostics {
                historical: true,
                backend_class: DiagnosticValue::unavailable("multiple_provider_classes_retained"),
                mode: DiagnosticValue::unavailable("multiple_provider_modes_retained"),
            }
        };
        let complete = events_complete
            && !changes_limited
            && changes.len() == change_total
            && !operations_limited
            && operations.len() == operation_total
            && !verification_limited
            && verification.len() == verification_total;
        let elapsed_ms = match (event_first, event_last) {
            (Some(first), Some(last)) => {
                DiagnosticValue::derived(last.saturating_sub(first) as u64 * 1000, complete)
            }
            _ => DiagnosticValue::unavailable("event_timestamps_not_retained"),
        };
        let stored_state = lifecycle
            .map(lifecycle_name)
            .map(DiagnosticValue::observed)
            .unwrap_or_else(|| DiagnosticValue::unavailable("run_state_not_retained"));
        let session_id = session_id
            .map(DiagnosticValue::observed)
            .unwrap_or_else(|| DiagnosticValue::unavailable("session_identity_not_retained"));
        let change_rows_returned = changes.len().min(u32::MAX as usize) as u32;
        let change_rows_omitted = change_total
            .saturating_sub(changes.len())
            .min(u32::MAX as usize) as u32;
        let operation_rows_returned = operations.len().min(u32::MAX as usize) as u32;
        let operation_rows_omitted = operation_total
            .saturating_sub(operations.len())
            .min(u32::MAX as usize) as u32;
        let verification_rows_returned = verification.len().min(u32::MAX as usize) as u32;
        let verification_rows_omitted = verification_total
            .saturating_sub(verification.len())
            .min(u32::MAX as usize) as u32;
        let report = RunDiagnosticsReport {
            schema_version: RUN_DIAGNOSTICS_SCHEMA_VERSION,
            metadata_only: true,
            snapshot: true,
            generator: DiagnosticGenerator {
                product: "argentum".into(),
                package_version: env!("CARGO_PKG_VERSION").into(),
            },
            historical_executable: DiagnosticValue::unavailable(
                "historical_executable_identity_not_retained",
            ),
            project_id,
            session_id,
            run_id,
            outcome: RunOutcomeDiagnostics {
                stored_state,
                final_state: lifecycle
                    .map(|value| DiagnosticValue::observed(value.is_terminal()))
                    .unwrap_or_else(|| DiagnosticValue::unavailable("run_state_not_retained")),
                task_accepted: if accepted {
                    DiagnosticValue::observed(true)
                } else {
                    DiagnosticValue::unavailable("task_acceptance_not_retained")
                },
                goal_completed: DiagnosticValue::unavailable(
                    "goal_completion_is_not_a_run_outcome",
                ),
                run_error_recorded,
            },
            provider,
            model_requests: ModelRequestDiagnostics {
                logical_rounds: if attempt_metadata_available {
                    DiagnosticValue::derived(logical_rounds, events_complete)
                } else {
                    DiagnosticValue::unavailable("model_attempt_metadata_not_retained")
                },
                physical_attempts: if attempt_metadata_available {
                    DiagnosticValue::derived(physical_attempts, events_complete)
                } else {
                    DiagnosticValue::unavailable("model_attempt_metadata_not_retained")
                },
                retry_schedules: if attempt_metadata_available {
                    DiagnosticValue::derived(retry_schedules, events_complete)
                } else {
                    DiagnosticValue::unavailable("model_attempt_metadata_not_retained")
                },
                planned_wait_ms: if attempt_metadata_available {
                    DiagnosticValue::derived(planned_wait_ms, events_complete)
                } else {
                    DiagnosticValue::unavailable("model_attempt_metadata_not_retained")
                },
                attempts: model_attempts,
            },
            elapsed_ms,
            usage_records: if usage.is_empty() {
                DiagnosticValue::unavailable("provider_usage_not_reported")
            } else {
                DiagnosticValue::derived(usage.len().min(u32::MAX as usize) as u32, events_complete)
            },
            usage,
            accounting,
            tools: tool_items,
            approvals: approvals.into_values().collect(),
            changes,
            change_operations: operations,
            verification,
            context,
            compaction,
            steering,
            provenance: ProvenanceDiagnostics {
                instruction_sources: DiagnosticValue::unavailable(
                    "safe_instruction_provenance_not_retained",
                ),
                selected_skill: DiagnosticValue::unavailable("safe_skill_provenance_not_retained"),
            },
            coverage: DiagnosticCoverage {
                complete,
                event_rows_decoded: decoded,
                event_rows_omitted: omitted,
                oversized_event_rows: oversized,
                invalid_event_rows: invalid,
                source_bytes_decoded: source_bytes as u64,
                change_rows_returned,
                change_rows_omitted,
                operation_rows_returned,
                operation_rows_omitted,
                verification_rows_returned,
                verification_rows_omitted,
                limits: DiagnosticLimits {
                    source_keys: RUN_DIAGNOSTICS_MAX_SOURCE_KEYS as u32,
                    cached_pages: RUN_DIAGNOSTICS_MAX_CACHED_PAGES as u32,
                    event_rows: RUN_DIAGNOSTICS_MAX_EVENT_ROWS as u32,
                    event_payload_bytes: RUN_DIAGNOSTICS_MAX_EVENT_PAYLOAD_BYTES as u32,
                    source_bytes: RUN_DIAGNOSTICS_MAX_SOURCE_BYTES as u32,
                    change_rows: RUN_DIAGNOSTICS_MAX_CHANGES as u32,
                    verification_rows: RUN_DIAGNOSTICS_MAX_VERIFICATIONS as u32,
                    json_bytes: RUN_DIAGNOSTICS_MAX_JSON_BYTES as u32,
                },
            },
        };
        if serde_json::to_vec(&report)?.len() > RUN_DIAGNOSTICS_MAX_JSON_BYTES {
            return Err(StoreError::InvalidRecord(
                "run diagnostics output exceeds the JSON size limit".into(),
            ));
        }
        Ok(Some(report))
    }
}

fn diagnostic_text(item: &DiagnosticValue<String>) -> &str {
    item.value.as_deref().unwrap_or("unavailable")
}

pub fn format_run_diagnostics_human(report: &RunDiagnosticsReport) -> String {
    let mut text = format!(
        "Run diagnostics snapshot\nRun: {}\nStored outcome: {} ({:?})\nTask accepted: {}\nGoal completion: unavailable; run outcome is kept separate.\nProvider: {} | mode: {}\nLogical model rounds: {} | physical attempts: {} | retries: {} | planned wait: {} ms\nElapsed: {} ms\nUsage records: {} | accounting records: {}\nTools: {} | approvals: {}\nChanges: {} | change operations: {} | verification receipts: {}\nContext accounting records: {} | compaction records: {} | steering records: {}\nCoverage: {} | decoded events: {} | omitted: {} | invalid: {}\nMetadata only: prompt, answer, reasoning, bodies, paths, labels, URLs, credentials, and raw errors are excluded.",
        report.run_id,
        diagnostic_text(&report.outcome.stored_state),
        report.outcome.stored_state.quality,
        report.outcome.task_accepted.value.map_or("unavailable", |v| if v { "yes" } else { "no" }),
        diagnostic_text(&report.provider.backend_class),
        diagnostic_text(&report.provider.mode),
        report.model_requests.logical_rounds.value.map_or_else(|| "unavailable".into(), |v| v.to_string()),
        report.model_requests.physical_attempts.value.map_or_else(|| "unavailable".into(), |v| v.to_string()),
        report.model_requests.retry_schedules.value.map_or_else(|| "unavailable".into(), |v| v.to_string()),
        report.model_requests.planned_wait_ms.value.map_or_else(|| "unavailable".into(), |v| v.to_string()),
        report.elapsed_ms.value.map_or_else(|| "unavailable".into(), |v| v.to_string()),
        report.usage_records.value.map_or_else(|| "unavailable".into(), |v| v.to_string()), report.accounting.len(), report.tools.len(), report.approvals.len(),
        report.changes.len(), report.change_operations.len(), report.verification.len(),
        report.context.len(), report.compaction.len(), report.steering.len(),
        if report.coverage.complete { "complete" } else { "partial" },
        report.coverage.event_rows_decoded, report.coverage.event_rows_omitted,
        report.coverage.invalid_event_rows,
    );
    if report.changes.is_empty() && report.coverage.change_rows_omitted == 0 {
        text.push_str("\nNo completed file-changing operation was recorded in the complete change-reference coverage.");
    } else if report.changes.is_empty() {
        text.push_str("\nCompleted file-changing operations are unknown because change-reference coverage is partial.");
    }
    for tool in &report.tools {
        text.push_str(&format!(
            "\nTool: {} | call: {} | outcome: {}",
            tool.tool,
            tool.call_id,
            diagnostic_text(&tool.outcome)
        ));
    }
    for steering in &report.steering {
        text.push_str(&format!(
            "\nSteering: {} | position: {} | bytes: {} | lifecycle: {}",
            steering.entry_id, steering.position, steering.text_bytes, steering.lifecycle
        ));
    }
    for change in &report.changes {
        text.push_str(&format!(
            "\nChange: {} | revision: {} | kind: {} | origin: {}",
            change.change_id, change.revision, change.kind, change.origin
        ));
        if let Some(source) = change.restored_from.value {
            text.push_str(&format!(" | restores: {source}"));
        }
    }
    for verification in &report.verification {
        text.push_str(&format!(
            "\nVerification: {} | runner: {} | state: {} | revisions: {} -> {}",
            verification.verification_id,
            verification.runner_class,
            verification.state,
            verification.revision_before,
            verification.revision_after
        ));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;
    use argentum_domain::{
        context_budget::RequestAccounting, ApprovalRequest, FileChangeKind, FileChangeOrigin,
        FileSnapshotRef, ModelUsage, SteeringLifecycle, SteeringStatus, Task, ToolTrace,
    };

    fn scope(store: &Store) -> (tempfile::TempDir, WorkspaceResolution, EventScope) {
        let root = tempfile::tempdir().unwrap();
        let resolution = store.resolve_workspace(root.path()).unwrap();
        let event_scope = EventScope {
            workspace_key: resolution.workspace_key.clone(),
            project_id: resolution.snapshot.project.id,
            session_id: resolution.snapshot.active_session_id,
            run_id: None,
        };
        (root, resolution, event_scope)
    }

    fn append(store: &Store, base: &EventScope, run: RunId, event: AppEvent) {
        let mut scoped = base.clone();
        scoped.run_id = Some(run);
        store.append_event_scoped(&scoped, &event).unwrap();
    }

    fn snapshot(bytes: &[u8]) -> FileSnapshotRef {
        FileSnapshotRef {
            digest: digest_bytes(bytes),
            size_bytes: bytes.len() as u64,
            retained: true,
        }
    }

    #[test]
    fn f21a_report_preserves_typed_failure_retry_changes_and_excludes_canaries() {
        let store = Store::open_in_memory().unwrap();
        let (_root, resolution, event_scope) = scope(&store);
        let project = resolution.snapshot.project.id;
        let session = resolution.snapshot.active_session_id.unwrap();
        let run = Uuid::new_v4();
        let request = Uuid::new_v4();
        let retry_request = Uuid::new_v4();
        let canaries = [
            "PROMPT_CANARY_f21a",
            "ANSWER_CANARY_f21a",
            "PRIVATE_PATH_CANARY_f21a",
            "PROFILE_LABEL_CANARY_f21a",
            "SKILL_BODY_CANARY_f21a",
            "TOOL_OUTPUT_CANARY_f21a",
            "sk-live-CREDENTIAL_CANARY_f21a",
            "RAW_ERROR_CANARY_f21a",
        ];
        append(
            &store,
            &event_scope,
            run,
            AppEvent::TaskAccepted(Task {
                id: run,
                session_id: session,
                prompt: canaries[0].into(),
                lifecycle: TaskLifecycle::Running,
                created_at: now(),
            }),
        );
        append(
            &store,
            &event_scope,
            run,
            AppEvent::AssistantDelta {
                run_id: run,
                text: canaries[1].into(),
            },
        );
        let accounting = RequestAccounting {
            method: "estimated_utf8_wire_bytes_v1_not_tokenizer_upper_bound".into(),
            capacity_source: "fixture".into(),
            budget_revision: Uuid::new_v4(),
            profile_id: canaries[3].into(),
            model: "PRIVATE_MODEL_FILENAME_CANARY.gguf".into(),
            provider_endpoint: format!("https://{}/v1", canaries[6]),
            provider_kind: ProviderKind::LocalLlamaCpp,
            accounted_input: 100,
            serialized_message_bytes: 70,
            message_components: BTreeMap::new(),
            serialized_catalog_bytes: 20,
            serialized_envelope_bytes: 10,
            output_reserve: 50,
            uncertainty_allowance: 5,
            usable_context: 4096,
            separate_input_limit: None,
            maximum_output: 1024,
            bundled_model_context_limit: Some(4096),
            profile_catalog_context_limit: None,
            profile_catalog_source: None,
            fits: true,
        };
        append(
            &store,
            &event_scope,
            run,
            AppEvent::ContextRequestAccounted {
                session_id: session,
                run_id: run,
                request_id: request,
                purpose: "main_generation".into(),
                accounting,
            },
        );
        for event in [
            AppEvent::ModelRequestAttempt {
                session_id: session,
                run_id: run,
                request_id: request,
                purpose: "main_generation".into(),
                attempt: 1,
                max_attempts: 3,
                outcome: "started".into(),
                error_class: None,
                status: None,
                planned_delay_ms: None,
                stop_reason: None,
            },
            AppEvent::ModelRequestAttempt {
                session_id: session,
                run_id: run,
                request_id: request,
                purpose: "main_generation".into(),
                attempt: 1,
                max_attempts: 3,
                outcome: "retry_scheduled".into(),
                error_class: Some("transient_http_status".into()),
                status: Some(503),
                planned_delay_ms: Some(700),
                stop_reason: None,
            },
            AppEvent::ModelRequestAttempt {
                session_id: session,
                run_id: run,
                request_id: retry_request,
                purpose: "main_generation".into(),
                attempt: 2,
                max_attempts: 3,
                outcome: "started".into(),
                error_class: None,
                status: None,
                planned_delay_ms: None,
                stop_reason: None,
            },
            AppEvent::ModelRequestAttempt {
                session_id: session,
                run_id: run,
                request_id: retry_request,
                purpose: "main_generation".into(),
                attempt: 2,
                max_attempts: 3,
                outcome: "stopped".into(),
                error_class: None,
                status: None,
                planned_delay_ms: None,
                stop_reason: Some("completed".into()),
            },
        ] {
            append(&store, &event_scope, run, event);
        }
        append(
            &store,
            &event_scope,
            run,
            AppEvent::ModelUsageUpdated {
                session_id: session,
                run_id: run,
                profile_id: canaries[3].into(),
                model: "PRIVATE_MODEL_FILENAME_CANARY.gguf".into(),
                usage: ModelUsage {
                    input_tokens: 80,
                    output_tokens: 20,
                    total_tokens: 100,
                    reasoning_tokens: None,
                    cached_input_tokens: Some(10),
                    context_window_tokens: Some(4096),
                },
            },
        );
        let call = Uuid::new_v4();
        append(
            &store,
            &event_scope,
            run,
            AppEvent::ToolStarted(ToolTrace {
                id: call,
                run_id: run,
                tool_id: "write_text".into(),
                summary: canaries[4].into(),
                result: ToolResultState::Running,
                duration_ms: None,
            }),
        );
        append(
            &store,
            &event_scope,
            run,
            AppEvent::ToolFinished(ToolTrace {
                id: call,
                run_id: run,
                tool_id: "write_text".into(),
                summary: canaries[5].into(),
                result: ToolResultState::Failed,
                duration_ms: Some(9),
            }),
        );
        let approval = Uuid::new_v4();
        append(
            &store,
            &event_scope,
            run,
            AppEvent::ApprovalRequested(ApprovalRequest {
                id: approval,
                run_id: run,
                tool_id: "write_text".into(),
                action: "write".into(),
                target: canaries[2].into(),
                binding: Sha256Digest("a".repeat(64)),
                reason: canaries[6].into(),
                capabilities: Vec::new(),
                expires_at: None,
            }),
        );
        append(
            &store,
            &event_scope,
            run,
            AppEvent::ApprovalResolved {
                approval_id: approval,
                approved: false,
            },
        );

        let before = b"before";
        let after = b"after";
        let original = PreparedChangeOperation {
            operation_id: Uuid::new_v4(),
            workspace_key: resolution.workspace_key.clone(),
            project_id: project,
            session_id: session,
            run_id: run,
            relative_path: format!("{}.txt", canaries[2]),
            kind: FileChangeKind::Modified,
            origin: FileChangeOrigin::ToolCall,
            before: Some(snapshot(before)),
            after: Some(snapshot(after)),
            before_bytes: Some(before.to_vec()),
            after_bytes: Some(after.to_vec()),
            additions: 1,
            removals: 1,
            line_count_availability: argentum_domain::LineCountAvailability::Unknown,
            restored_from: None,
        };
        store.prepare_change_operation(&original).unwrap();
        store
            .mark_change_operation_applied(original.operation_id)
            .unwrap();
        let original_record = store
            .finalize_change_operation(original.operation_id)
            .unwrap();
        let restore = PreparedChangeOperation {
            operation_id: Uuid::new_v4(),
            workspace_key: resolution.workspace_key.clone(),
            project_id: project,
            session_id: session,
            run_id: run,
            relative_path: format!("{}.txt", canaries[2]),
            kind: FileChangeKind::Modified,
            origin: FileChangeOrigin::Restore,
            before: Some(snapshot(after)),
            after: Some(snapshot(before)),
            before_bytes: Some(after.to_vec()),
            after_bytes: Some(before.to_vec()),
            additions: 1,
            removals: 1,
            line_count_availability: argentum_domain::LineCountAvailability::Unknown,
            restored_from: Some(original_record.id),
        };
        store.prepare_change_operation(&restore).unwrap();
        store
            .mark_change_operation_applied(restore.operation_id)
            .unwrap();
        let (restore_record, _) = store
            .finalize_restore_change_operation(restore.operation_id, original_record.id)
            .unwrap();

        store
            .save_verification_evidence(
                project,
                session,
                &VerificationEvidence {
                    id: Uuid::new_v4(),
                    run_id: run,
                    runner: VerificationRunnerId::CargoTestLocked,
                    registry_version: 1,
                    program: "cargo".into(),
                    args: vec!["test".into(), "--workspace".into(), "--locked".into()],
                    state: VerificationState::Failed,
                    started_at: now(),
                    duration_ms: 11,
                    exit_code: Some(1),
                    revision_before: Sha256Digest("b".repeat(64)),
                    revision_after: Sha256Digest("c".repeat(64)),
                    stdout_sha256: Sha256Digest("d".repeat(64)),
                    stderr_sha256: Sha256Digest("e".repeat(64)),
                    stdout_bytes: 12,
                    stderr_bytes: 34,
                    stdout_truncated: false,
                    stderr_truncated: true,
                    workspace_key: String::new(),
                    definition_sha256: None,
                    input_scope: None,
                    input_before: None,
                    input_after: None,
                    generated_outputs: None,
                },
            )
            .unwrap();
        append(
            &store,
            &event_scope,
            run,
            AppEvent::VerificationCompleted {
                run_id: run,
                passed: true,
                summary: "misleading success prose".into(),
            },
        );
        append(
            &store,
            &event_scope,
            run,
            AppEvent::RunError {
                session_id: session,
                run_id: run,
                message: canaries[7].into(),
                recoverable: false,
            },
        );
        append(
            &store,
            &event_scope,
            run,
            AppEvent::RunStatusChanged {
                run_id: run,
                lifecycle: TaskLifecycle::Failed,
            },
        );

        let database_changes = || {
            store
                .connection
                .lock()
                .unwrap()
                .query_row("SELECT total_changes()", [], |row| row.get::<_, i64>(0))
                .unwrap()
        };
        let baseline = database_changes();
        let first = store
            .run_diagnostics(project, Some(session), run)
            .unwrap()
            .unwrap();
        let second = store
            .run_diagnostics(project, Some(session), run)
            .unwrap()
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(database_changes(), baseline);
        assert_eq!(first.outcome.stored_state.value.as_deref(), Some("failed"));
        assert_eq!(
            first.outcome.goal_completed.quality,
            DiagnosticQuality::Unavailable
        );
        assert_eq!(
            first.provider.backend_class.value.as_deref(),
            Some("llama_cpp")
        );
        assert_eq!(first.model_requests.logical_rounds.value, Some(1));
        assert_eq!(first.model_requests.physical_attempts.value, Some(2));
        assert_eq!(first.model_requests.retry_schedules.value, Some(1));
        assert_eq!(first.model_requests.planned_wait_ms.value, Some(700));
        assert_eq!(first.usage.len(), 1);
        assert_eq!(first.verification[0].state, "failed");
        assert!(first
            .changes
            .iter()
            .any(|item| item.change_id == original_record.id));
        assert!(first.changes.iter().any(|item| {
            item.change_id == restore_record.id
                && item.restored_from.value == Some(original_record.id)
        }));
        assert_eq!(first.approvals[0].outcome.value.as_deref(), Some("denied"));
        let json = serde_json::to_string(&first).unwrap();
        assert!(json.len() <= RUN_DIAGNOSTICS_MAX_JSON_BYTES);
        for canary in canaries {
            assert!(!json.contains(canary), "leaked {canary}");
        }
        assert!(store
            .run_diagnostics(Uuid::new_v4(), None, run)
            .unwrap()
            .is_none());
        assert!(store
            .run_diagnostics(project, Some(Uuid::new_v4()), run)
            .unwrap()
            .is_none());
    }

    #[test]
    fn f21a_source_limits_are_applied_before_decode_and_json_remains_complete() {
        let store = Store::open_in_memory().unwrap();
        let (_root, resolution, event_scope) = scope(&store);
        let project = resolution.snapshot.project.id;
        let session = resolution.snapshot.active_session_id.unwrap();
        let run = Uuid::new_v4();
        append(
            &store,
            &event_scope,
            run,
            AppEvent::TaskAccepted(Task {
                id: run,
                session_id: session,
                prompt: "X".repeat(RUN_DIAGNOSTICS_MAX_EVENT_PAYLOAD_BYTES + 32),
                lifecycle: TaskLifecycle::Running,
                created_at: now(),
            }),
        );
        for _ in 0..=RUN_DIAGNOSTICS_MAX_EVENT_ROWS {
            append(
                &store,
                &event_scope,
                run,
                AppEvent::RunStatusChanged {
                    run_id: run,
                    lifecycle: TaskLifecycle::Running,
                },
            );
        }
        let report = store
            .run_diagnostics(project, Some(session), run)
            .unwrap()
            .unwrap();
        assert!(!report.coverage.complete);
        assert!(report.coverage.event_rows_omitted > 0);
        assert_eq!(
            report.coverage.limits.json_bytes as usize,
            RUN_DIAGNOSTICS_MAX_JSON_BYTES
        );
        let bytes = serde_json::to_vec(&report).unwrap();
        assert!(bytes.len() <= RUN_DIAGNOSTICS_MAX_JSON_BYTES);
        serde_json::from_slice::<RunDiagnosticsReport>(&bytes).unwrap();
    }

    #[test]
    fn f21a_read_only_run_reports_no_change_without_inventing_goal_completion() {
        let store = Store::open_in_memory().unwrap();
        let (_root, resolution, event_scope) = scope(&store);
        let project = resolution.snapshot.project.id;
        let session = resolution.snapshot.active_session_id.unwrap();
        let run = Uuid::new_v4();
        let call = Uuid::new_v4();
        append(
            &store,
            &event_scope,
            run,
            AppEvent::ToolStarted(ToolTrace {
                id: call,
                run_id: run,
                tool_id: "read_text".into(),
                summary: "PRIVATE_READ_PATH_CANARY".into(),
                result: ToolResultState::Running,
                duration_ms: None,
            }),
        );
        append(
            &store,
            &event_scope,
            run,
            AppEvent::ToolFinished(ToolTrace {
                id: call,
                run_id: run,
                tool_id: "read_text".into(),
                summary: "PRIVATE_READ_OUTPUT_CANARY".into(),
                result: ToolResultState::Succeeded,
                duration_ms: Some(2),
            }),
        );
        append(
            &store,
            &event_scope,
            run,
            AppEvent::RunStatusChanged {
                run_id: run,
                lifecycle: TaskLifecycle::Complete,
            },
        );
        let report = store
            .run_diagnostics(project, Some(session), run)
            .unwrap()
            .unwrap();
        assert!(report.coverage.complete);
        assert!(report.changes.is_empty());
        assert_eq!(report.tools[0].outcome.value.as_deref(), Some("succeeded"));
        assert_eq!(report.outcome.final_state.value, Some(true));
        assert_eq!(
            report.outcome.goal_completed.quality,
            DiagnosticQuality::Unavailable
        );
        assert_eq!(
            report.model_requests.logical_rounds.quality,
            DiagnosticQuality::Unavailable
        );
        let text = format_run_diagnostics_human(&report);
        assert!(text.contains("No completed file-changing operation was recorded"));
        assert!(!text.contains("PRIVATE_READ_PATH_CANARY"));
        assert!(!text.contains("PRIVATE_READ_OUTPUT_CANARY"));
    }

    #[test]
    fn steering_diagnostics_retain_lifecycle_metadata_without_a_text_body() {
        let store = Store::open_in_memory().unwrap();
        let (_root, resolution, event_scope) = scope(&store);
        let project = resolution.snapshot.project.id;
        let session = resolution.snapshot.active_session_id.unwrap();
        let run = Uuid::new_v4();
        let entry = Uuid::new_v4();
        for lifecycle in [SteeringLifecycle::Pending, SteeringLifecycle::Included] {
            append(
                &store,
                &event_scope,
                run,
                AppEvent::SteeringChanged(SteeringStatus {
                    version: 1,
                    entry_id: entry,
                    project_id: project,
                    session_id: session,
                    run_id: run,
                    position: 1,
                    text_bytes: 37,
                    lifecycle,
                    reason: None,
                }),
            );
        }
        append(
            &store,
            &event_scope,
            run,
            AppEvent::SteeringChanged(SteeringStatus {
                version: 1,
                entry_id: entry,
                project_id: project,
                session_id: session,
                run_id: run,
                position: 1,
                text_bytes: 37,
                lifecycle: SteeringLifecycle::NotApplied,
                reason: Some("PRIVATE_STEERING_BODY_CANARY".into()),
            }),
        );
        let report = store
            .run_diagnostics(project, Some(session), run)
            .unwrap()
            .unwrap();
        assert_eq!(report.steering.len(), 3);
        assert_eq!(report.steering[0].lifecycle, "pending");
        assert_eq!(report.steering[1].lifecycle, "included");
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("PRIVATE_STEERING_BODY_CANARY"));
    }
}
