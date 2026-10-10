use std::collections::{BTreeMap, VecDeque};

use argentum_domain::{AppEvent, SteeringEntryId, SteeringLifecycle, SteeringStatus};
use sha2::{Digest, Sha256};

use super::*;

pub const MAX_STEERING_ENTRIES: usize = 8;
pub const MAX_STEERING_ENTRY_BYTES: usize = 8 * 1024;
pub const MAX_STEERING_BYTES: usize = 32 * 1024;

#[derive(Clone, PartialEq, Eq)]
pub struct SteeringSubmission {
    pub id: SteeringEntryId,
    pub run_id: RunId,
    pub target: SubmissionTarget,
    pub text: String,
}

impl std::fmt::Debug for SteeringSubmission {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SteeringSubmission")
            .field("id", &self.id)
            .field("run_id", &self.run_id)
            .field("target", &self.target)
            .field("text_bytes", &self.text.len())
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SteeringReceipt {
    pub id: SteeringEntryId,
    pub run_id: RunId,
    pub position: u64,
    pub lifecycle: SteeringLifecycle,
}

#[derive(Debug, Clone)]
pub(super) struct SteeringEntry {
    pub id: SteeringEntryId,
    pub position: u64,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolGate {
    Admission,
    OrdinaryExecuting,
    UnsupportedBusy,
}

struct SeenEntry {
    digest: [u8; 32],
    receipt: SteeringReceipt,
}

struct SteeringRun {
    run_id: RunId,
    target: SubmissionTarget,
    next_position: u64,
    pending: VecDeque<SteeringEntry>,
    in_flight: Vec<SteeringEntry>,
    seen: BTreeMap<SteeringEntryId, SeenEntry>,
    tool_gate: Option<ToolGate>,
    settling: bool,
}

impl SteeringRun {
    fn outstanding(&self) -> (usize, usize) {
        self.pending
            .iter()
            .chain(&self.in_flight)
            .fold((0, 0), |(count, bytes), entry| {
                (count + 1, bytes + entry.text.len())
            })
    }
}

#[derive(Default)]
pub(super) struct SteeringControl {
    runs: BTreeMap<RunId, SteeringRun>,
}

fn text_digest(text: &str) -> [u8; 32] {
    Sha256::digest(text.as_bytes()).into()
}

fn status(
    run: &SteeringRun,
    entry: &SteeringEntry,
    lifecycle: SteeringLifecycle,
    reason: Option<&str>,
) -> SteeringStatus {
    SteeringStatus {
        version: 1,
        entry_id: entry.id,
        project_id: run.target.project_id,
        session_id: run.target.session_id,
        run_id: run.run_id,
        position: entry.position,
        text_bytes: entry.text.len() as u32,
        lifecycle,
        reason: reason.map(str::to_owned),
    }
}

impl RuntimeService {
    pub(super) fn register_steering_run(
        &self,
        target: &SubmissionTarget,
        run_id: RunId,
    ) -> Result<(), RuntimeError> {
        let mut control = self
            .steering
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?;
        if control.runs.contains_key(&run_id) {
            return Err(RuntimeError::SteeringTargetChanged);
        }
        control.runs.insert(
            run_id,
            SteeringRun {
                run_id,
                target: target.clone(),
                next_position: 1,
                pending: VecDeque::new(),
                in_flight: Vec::new(),
                seen: BTreeMap::new(),
                tool_gate: None,
                settling: false,
            },
        );
        Ok(())
    }

    pub fn steer_active_run(
        &self,
        submission: SteeringSubmission,
    ) -> Result<SteeringReceipt, RuntimeError> {
        if submission.id.is_nil() || submission.run_id.is_nil() {
            return Err(RuntimeError::SteeringTargetChanged);
        }
        if submission.text.trim().is_empty() {
            return Err(RuntimeError::EmptySteering);
        }
        if submission.text.len() > MAX_STEERING_ENTRY_BYTES {
            return Err(RuntimeError::SteeringEntryTooLarge);
        }
        if self.shutdown_cancellation.is_cancelled() {
            return Err(RuntimeError::ShuttingDown);
        }

        let target_key = submission
            .target
            .workspace
            .to_string_lossy()
            .replace('\\', "/");
        #[cfg(target_os = "windows")]
        let target_key = target_key.to_ascii_lowercase();
        if submission.target.project_id != self.project_id || target_key != self.workspace_key {
            return Err(RuntimeError::SteeringTargetChanged);
        }

        let mut control = self
            .steering
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?;
        let run = control
            .runs
            .get_mut(&submission.run_id)
            .ok_or(RuntimeError::NoActiveSteeringRun)?;
        if run.target != submission.target || run.settling {
            return Err(RuntimeError::SteeringTargetChanged);
        }
        if let Some(seen) = run.seen.get(&submission.id) {
            if seen.digest == text_digest(&submission.text) {
                return Ok(seen.receipt.clone());
            }
            return Err(RuntimeError::SteeringTargetChanged);
        }
        if matches!(run.tool_gate, Some(ToolGate::Admission)) {
            return Err(RuntimeError::SteeringBusy(
                "tool admission is settling; retry after its state is visible",
            ));
        }
        if matches!(run.tool_gate, Some(ToolGate::UnsupportedBusy)) {
            return Err(RuntimeError::SteeringBusy(
                "an approval or asynchronous process job is active",
            ));
        }
        if self
            .goal_runs
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?
            .contains_key(&submission.run_id)
        {
            return Err(RuntimeError::SteeringBusy(
                "goal supervision is active for this run",
            ));
        }
        if self
            .pending_approvals
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?
            .values()
            .any(|pending| pending.request.run_id == submission.run_id)
            || self
                .pending_restores
                .lock()
                .map_err(|_| RuntimeError::StateLockPoisoned)?
                .values()
                .any(|pending| pending.source_run_id == submission.run_id)
        {
            return Err(RuntimeError::SteeringBusy(
                "an owner approval or restore is pending",
            ));
        }
        if self
            .tools
            .process_jobs()
            .has_live_run(submission.run_id)
            .map_err(|_| RuntimeError::SteeringState)?
        {
            return Err(RuntimeError::SteeringBusy(
                "an asynchronous process job is active",
            ));
        }

        let (count, bytes) = run.outstanding();
        if count >= MAX_STEERING_ENTRIES {
            return Err(RuntimeError::SteeringCapacity(
                "eight steering entries are already outstanding",
            ));
        }
        if bytes + submission.text.len() > MAX_STEERING_BYTES {
            return Err(RuntimeError::SteeringCapacity(
                "outstanding steering text would exceed 32 KiB",
            ));
        }
        let entry = SteeringEntry {
            id: submission.id,
            position: run.next_position,
            text: submission.text,
        };
        run.next_position = run.next_position.saturating_add(1);
        let receipt = SteeringReceipt {
            id: entry.id,
            run_id: submission.run_id,
            position: entry.position,
            lifecycle: SteeringLifecycle::Pending,
        };
        run.seen.insert(
            entry.id,
            SeenEntry {
                digest: text_digest(&entry.text),
                receipt: receipt.clone(),
            },
        );
        run.pending.push_back(entry.clone());
        let event = status(run, &entry, SteeringLifecycle::Pending, None);
        drop(control);
        if let Err(error) = self.emit_for_run(AppEvent::SteeringChanged(event), submission.run_id) {
            let mut control = self
                .steering
                .lock()
                .map_err(|_| RuntimeError::StateLockPoisoned)?;
            if let Some(run) = control.runs.get_mut(&submission.run_id) {
                run.pending.retain(|pending| pending.id != submission.id);
                run.seen.remove(&submission.id);
            }
            return Err(error);
        }
        Ok(receipt)
    }

    pub(super) fn begin_steering_request(
        &self,
        run_id: RunId,
    ) -> Result<Vec<SteeringEntry>, RuntimeError> {
        let mut control = self
            .steering
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?;
        let run = control
            .runs
            .get_mut(&run_id)
            .ok_or(RuntimeError::NoActiveSteeringRun)?;
        if !run.in_flight.is_empty() {
            return Err(RuntimeError::SteeringState);
        }
        run.in_flight.extend(run.pending.drain(..));
        Ok(run.in_flight.clone())
    }

    pub(super) fn commit_steering_request(
        &self,
        run_id: RunId,
        entries: &[SteeringEntry],
    ) -> Result<(), RuntimeError> {
        if entries.is_empty() {
            return Ok(());
        }
        let mut control = self
            .steering
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?;
        let run = control
            .runs
            .get_mut(&run_id)
            .ok_or(RuntimeError::NoActiveSteeringRun)?;
        if run
            .in_flight
            .iter()
            .map(|entry| entry.id)
            .collect::<Vec<_>>()
            != entries.iter().map(|entry| entry.id).collect::<Vec<_>>()
        {
            return Err(RuntimeError::SteeringState);
        }
        let events = entries
            .iter()
            .map(|entry| status(run, entry, SteeringLifecycle::Included, None))
            .collect::<Vec<_>>();
        run.in_flight.clear();
        for entry in entries {
            if let Some(seen) = run.seen.get_mut(&entry.id) {
                seen.receipt.lifecycle = SteeringLifecycle::Included;
            }
        }
        drop(control);
        for event in events {
            self.emit_for_run(AppEvent::SteeringChanged(event), run_id)?;
        }
        Ok(())
    }

    pub(super) fn steering_pending(&self, run_id: RunId) -> Result<bool, RuntimeError> {
        let control = self
            .steering
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?;
        Ok(control
            .runs
            .get(&run_id)
            .is_some_and(|run| !run.pending.is_empty()))
    }

    pub(super) fn begin_steering_tool_admission(
        &self,
        run_id: RunId,
    ) -> Result<bool, RuntimeError> {
        let mut control = self
            .steering
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?;
        let run = control
            .runs
            .get_mut(&run_id)
            .ok_or(RuntimeError::NoActiveSteeringRun)?;
        if !run.pending.is_empty() {
            return Ok(false);
        }
        if run.tool_gate.is_some() {
            return Err(RuntimeError::SteeringState);
        }
        run.tool_gate = Some(ToolGate::Admission);
        Ok(true)
    }

    pub(super) fn steering_tool_execution_started(
        &self,
        run_id: RunId,
        unsupported_busy: bool,
    ) -> Result<(), RuntimeError> {
        let mut control = self
            .steering
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?;
        let run = control
            .runs
            .get_mut(&run_id)
            .ok_or(RuntimeError::NoActiveSteeringRun)?;
        match (run.tool_gate, unsupported_busy) {
            (Some(ToolGate::Admission), false) => {
                run.tool_gate = Some(ToolGate::OrdinaryExecuting);
                Ok(())
            }
            (Some(ToolGate::Admission | ToolGate::OrdinaryExecuting), true) => {
                run.tool_gate = Some(ToolGate::UnsupportedBusy);
                Ok(())
            }
            (Some(ToolGate::OrdinaryExecuting), false)
            | (Some(ToolGate::UnsupportedBusy), false)
            | (Some(ToolGate::UnsupportedBusy), true) => Ok(()),
            _ => Err(RuntimeError::SteeringState),
        }
    }

    pub(super) fn finish_steering_tool(&self, run_id: RunId) -> Result<(), RuntimeError> {
        let mut control = self
            .steering
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?;
        let run = control
            .runs
            .get_mut(&run_id)
            .ok_or(RuntimeError::NoActiveSteeringRun)?;
        run.tool_gate = None;
        Ok(())
    }

    pub(super) fn claim_steering_terminal(&self, run_id: RunId) -> Result<bool, RuntimeError> {
        let mut control = self
            .steering
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?;
        let run = control
            .runs
            .get_mut(&run_id)
            .ok_or(RuntimeError::NoActiveSteeringRun)?;
        if !run.pending.is_empty() || !run.in_flight.is_empty() {
            return Ok(false);
        }
        run.settling = true;
        Ok(true)
    }

    pub(super) fn persist_steering_request(
        &self,
        context: &ModelRunContext,
        entries: &[SteeringEntry],
        next_context_ordinal: &mut u32,
    ) -> Result<(), RuntimeError> {
        if entries.is_empty() {
            return Ok(());
        }
        let entry_count =
            u32::try_from(entries.len()).map_err(|_| RuntimeError::ContextCapacity)?;
        let exceeds_group_limit = match next_context_ordinal.checked_add(entry_count) {
            Some(end) => end > argentum_domain::working_context::MAX_RUN_GROUPS,
            None => true,
        };
        if exceeds_group_limit {
            return Err(RuntimeError::ContextCapacity);
        }
        for entry in entries {
            self.record_context_group(&argentum_domain::working_context::ContextGroup {
                version: argentum_domain::working_context::PROJECTION_VERSION,
                project_id: self.project_id,
                session_id: context.session_id,
                task_id: context.task_id,
                run_id: context.run_id,
                ordinal: *next_context_ordinal,
                body: argentum_domain::working_context::GroupBody::Steering {
                    entry_id: entry.id,
                    position: entry.position,
                    text: entry.text.clone(),
                },
            })?;
            *next_context_ordinal = next_context_ordinal.saturating_add(1);
        }
        self.commit_steering_request(context.run_id, entries)
    }

    pub(super) fn settle_steering(
        &self,
        run_id: RunId,
        reason: &'static str,
    ) -> Result<(), RuntimeError> {
        let mut control = self
            .steering
            .lock()
            .map_err(|_| RuntimeError::StateLockPoisoned)?;
        let Some(mut run) = control.runs.remove(&run_id) else {
            return Ok(());
        };
        let entries = run
            .pending
            .drain(..)
            .chain(run.in_flight.drain(..))
            .collect::<Vec<_>>();
        let events = entries
            .iter()
            .map(|entry| status(&run, entry, SteeringLifecycle::NotApplied, Some(reason)))
            .collect::<Vec<_>>();
        drop(control);
        for event in events {
            self.emit_for_run(AppEvent::SteeringChanged(event), run_id)?;
        }
        Ok(())
    }
}

pub(super) fn request_message(entry: &SteeringEntry) -> String {
    format!(
        "Steering entry {} at position {} was accepted for this run. Apply it at this safe request boundary.\n\n{}",
        entry.id, entry.position, entry.text
    )
}

pub(super) fn not_executed_result() -> String {
    serde_json::json!({
        "result": "not_executed",
        "reason": "accepted_steering_requires_fresh_model_decision"
    })
    .to_string()
}
