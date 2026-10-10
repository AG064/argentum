//! Owner-side review and restore preparation. Never a model tool.
use super::*;
use argentum_store::{ReviewCursor, ReviewRecord, ReviewRuns};

pub const DIFF_INPUT_BYTES: usize = 64 * 1024;
pub const DIFF_OUTPUT_BYTES: usize = 64 * 1024;
const RESTORE_PREVIEW_DIFF_BYTES: usize = 24 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewRequest {
    Runs {
        before: Option<ReviewCursor>,
    },
    Run(RunId),
    Diagnostics(RunId),
    Diff {
        run: RunId,
        change: argentum_domain::FileChangeId,
    },
    RestorePreview {
        run: RunId,
        change: argentum_domain::FileChangeId,
    },
    Verification(RunId),
    Operation {
        run: RunId,
        operation: argentum_domain::ChangeOperationId,
    },
}
#[derive(Debug, Clone)]
pub enum ReviewBody {
    Runs(ReviewRuns),
    Run {
        id: RunId,
        summary: String,
        records: Vec<ReviewRecord>,
    },
    Change {
        text: String,
        sensitive_path: Option<String>,
        run: RunId,
        change: argentum_domain::FileChangeId,
        restore_available: bool,
    },
    Restore {
        text: String,
        sensitive_path: Option<String>,
        run: RunId,
        change: argentum_domain::FileChangeId,
        approval_binding: Option<Sha256Digest>,
        available: bool,
    },
    Detail {
        text: String,
        sensitive_path: Option<String>,
    },
}
#[derive(Debug, Clone)]
pub struct ReviewPage {
    pub project: argentum_domain::ProjectId,
    pub session: SessionId,
    pub policy: String,
    pub body: ReviewBody,
}

impl RuntimeService {
    /// Metadata-only historical pagination, separate from ordinary review UI bodies.
    pub fn historical_change_page(
        &self,
        run: RunId,
        limit: usize,
        after: Option<argentum_domain::ChangePageCursor>,
    ) -> Result<argentum_domain::ChangePage, RuntimeError> {
        let mut page = self.store.change_page(
            self.project_id(),
            self.session_id(),
            run,
            limit,
            after.as_ref(),
        )?;
        for item in &mut page.items {
            if self.review_body_allowed(&item.path)? {
                item.path = self.review_display_path(&item.path);
            } else {
                item.path = "[excluded by current read policy]".into();
            }
        }
        Ok(page)
    }

    pub fn review_policy_stamp(&self) -> Result<String, RuntimeError> {
        Ok(argentum_security::execution_policy_digest(&self.load_execution_policy()?).0)
    }
    pub fn review_body_allowed(&self, path: &str) -> Result<bool, RuntimeError> {
        Ok(self.context_path_allowed(path, &self.load_execution_policy()?))
    }
    pub fn review_page(
        &self,
        project: argentum_domain::ProjectId,
        session: SessionId,
        request: ReviewRequest,
    ) -> Result<ReviewPage, RuntimeError> {
        if project != self.project_id || session != self.session_id() {
            return Err(RuntimeError::SubmissionTargetChanged);
        }
        let policy = self.review_policy_stamp()?;
        let body = match request {
            ReviewRequest::Runs { before } => {
                ReviewBody::Runs(self.store.review_runs(project, session, before.as_ref())?)
            }
            ReviewRequest::Run(run) => {
                let mut recorded = self.store.review_run(project, session, run)?;
                let applied = recorded
                    .records
                    .iter()
                    .filter(|r| r.file_change_id.is_some())
                    .count();
                let mut summary=format!("Run: {run}\nRuntime state: {}\nRecorded applied change entries: {applied}{}\nOther journal entries: {}\nOnly this run's records; unrelated working-tree edits are not included.\n",
                    recorded.final_state.map(|s|format!("{s:?}")).or_else(||recorded.lifecycle.map(|s|format!("{s:?}"))).unwrap_or_else(||"Unknown: state record unavailable".into()),
                    if recorded.records_complete{""}else{" (partial count; first 128 journal entries)"},recorded.records.len()-applied);
                summary.push_str(&verification_summary(&recorded));
                for item in &mut recorded.records {
                    item.path = self.review_display_path(&item.path);
                }
                ReviewBody::Run {
                    id: run,
                    summary,
                    records: recorded.records,
                }
            }
            ReviewRequest::Diagnostics(run) => {
                let report = self
                    .store
                    .run_diagnostics(project, Some(session), run)?
                    .ok_or(RuntimeError::ChangeNotFound)?;
                ReviewBody::Detail {
                    text: argentum_store::format_run_diagnostics_human(&report),
                    sensitive_path: None,
                }
            }
            ReviewRequest::Diff { run, change } => {
                let record = self.review_change_record(project, session, run, change)?;
                let diff = self.recorded_change_diff(change)?;
                let (restore_available, restore_status) =
                    self.change_restore_review_status(&record, &diff)?;
                let text=format!("Run: {run}\nChange: {change}\nChange set: {} | revision: {}\nPath: {}\nRecorded kind: {:?}; recorded applied, not a claim about current files.\n{}\nOriginal change diff, stored preimage -> stored postimage:\n{}\nRestore this change: {}",record.change_set_id,record.revision,self.review_display_path(&record.relative_path),record.kind,
                    if !diff.available{format!("Diff unavailable: {}",diff.unavailable_reason)}else if diff.truncated{"Partial diff: output bound reached.".into()}else{"Stored preimage -> stored postimage. Current workspace checked separately for restoration eligibility.".into()},diff.text,restore_status);
                ReviewBody::Change {
                    text,
                    sensitive_path: diff.available.then_some(record.relative_path),
                    run,
                    change,
                    restore_available,
                }
            }
            ReviewRequest::RestorePreview { run, change } => {
                let record = self.review_change_record(project, session, run, change)?;
                let original =
                    self.recorded_change_diff_with_limit(change, RESTORE_PREVIEW_DIFF_BYTES)?;
                let (available, status) = self.change_restore_review_status(&record, &original)?;
                let sensitive_path = original.available.then_some(record.relative_path.clone());
                let (reverse, approval_binding) = if available {
                    let before = self.snapshot_for_diff(record.before.as_ref())?;
                    let after = self.snapshot_for_diff(record.after.as_ref())?;
                    let prepared = PreparedChange {
                        relative_path: record.relative_path.clone(),
                        kind: argentum_domain::FileChangeKind::Modified,
                        before: after.clone(),
                        after: before.clone(),
                        additions: record.removals,
                        removals: record.additions,
                        line_count_availability: record.line_count_availability,
                    };
                    let approval_binding = restore_binding(
                        "restore_file",
                        &WriteApprovalBinding {
                            relative_path: record.relative_path.clone(),
                            before: after,
                            after: before,
                            origin: argentum_domain::FileChangeOrigin::Restore,
                            restored_from: Some(change),
                            additions: record.removals,
                            removals: record.additions,
                            line_count_availability: record.line_count_availability,
                        },
                    );
                    (
                        load_unified_diff(&prepared, RESTORE_PREVIEW_DIFF_BYTES)?,
                        Some(approval_binding),
                    )
                } else {
                    (
                        argentum_domain::UnifiedDiffSnapshot {
                            file_change_id: change,
                            relative_path: self.review_display_path(&record.relative_path),
                            text: String::new(),
                            line_count: 0,
                            byte_count: 0,
                            truncated: false,
                            available: false,
                            unavailable_reason: status.clone(),
                        },
                        None,
                    )
                };
                let text = format!(
                    "Restore review\nRun: {run}\nChange: {change}\nChange set: {} | revision: {}\nPath: {}\nRecorded kind: {:?}\nCurrent state matches recorded postimage: {}\n\nOriginal recorded change, preimage -> postimage{}:\n{}\n\nProposed restoration, current postimage -> recorded preimage{}:\n{}\n\n{}",
                    record.change_set_id,
                    record.revision,
                    self.review_display_path(&record.relative_path),
                    record.kind,
                    if available { "yes" } else { "no or unavailable" },
                    if original.truncated { " (partial preview)" } else { "" },
                    original.text,
                    if reverse.truncated { " (partial preview)" } else { "" },
                    reverse.text,
                    if available {
                        "Select Confirm exact restoration... to create the exact owner approval. Escape cancels without changing files."
                    } else {
                        status.as_str()
                    }
                );
                ReviewBody::Restore {
                    text,
                    sensitive_path,
                    run,
                    change,
                    approval_binding,
                    available,
                }
            }
            ReviewRequest::Verification(run) => {
                let recorded = self.store.review_run(project, session, run)?;
                let mut text = format!(
                    "Run: {run}\n{}\nRecorded result; current workspace not checked.\n",
                    verification_summary(&recorded)
                );
                for v in recorded.verification {
                    text.push_str(&format!("\nReceipt: {}\nRunner: {} | origin: {}\nRecorded state: {:?} | exit: {:?} | duration: {} ms\nDefinition: {} | registry version: {}\nRevision before: {}\nRevision after: {}\nOutput: retained counts/hashes only; raw output is not available here.\n",v.id,v.runner.id(),if v.runner.project_id().is_some(){"project-defined"}else{"fixed"},v.state,v.exit_code,v.duration_ms,v.definition_sha256.as_ref().map(|s|s.0.as_str()).unwrap_or("unavailable"),v.registry_version,v.revision_before.0,v.revision_after.0));
                }
                ReviewBody::Detail {
                    text,
                    sensitive_path: None,
                }
            }
            ReviewRequest::Operation { run, operation } => {
                let recorded = self.store.review_run(project, session, run)?;
                let record = recorded
                    .records
                    .iter()
                    .find(|record| record.operation_id == Some(operation))
                    .ok_or(RuntimeError::ChangeNotFound)?;
                ReviewBody::Detail{text:format!("Run: {run}\nTransaction: {operation}\nPath: {}\nRecorded state: {}\nNo committed change diff is available for this journal entry. Prepared is a proposal, Applied is not yet finalized, NotApplied includes rolled-back or unexecuted work, and Conflicted requires existing recovery inspection. No recovery action is performed here.",self.review_display_path(&record.path),record.state),sensitive_path:None}
            }
        };
        if self.review_policy_stamp()? != policy
            || self.project_id != project
            || self.session_id() != session
        {
            return Err(RuntimeError::SubmissionTargetChanged);
        }
        Ok(ReviewPage {
            project,
            session,
            policy,
            body,
        })
    }

    fn review_display_path(&self, path: &str) -> String {
        if Path::new(path).is_absolute() {
            return "[outside relative-workspace review; body unavailable]".into();
        }
        let mut display: String = path.chars().take(512).collect();
        if display.len() < path.len() {
            display.push_str(" [path preview abbreviated]");
        }
        display
    }

    fn review_change_record(
        &self,
        project: argentum_domain::ProjectId,
        session: SessionId,
        run: RunId,
        change: argentum_domain::FileChangeId,
    ) -> Result<argentum_domain::FileChangeRecord, RuntimeError> {
        let record = self
            .store
            .file_change(project, change)?
            .ok_or(RuntimeError::ChangeNotFound)?;
        if record.run_id != run
            || record.session_id != session
            || record.project_id != project
            || record.id != change
        {
            return Err(RuntimeError::SubmissionTargetChanged);
        }
        Ok(record)
    }

    fn change_restore_review_status(
        &self,
        record: &argentum_domain::FileChangeRecord,
        original: &argentum_domain::UnifiedDiffSnapshot,
    ) -> Result<(bool, String), RuntimeError> {
        if record.kind != argentum_domain::FileChangeKind::Modified {
            return Ok((
                false,
                "unavailable in this panel: creation and deletion restoration remain on the existing expert command path.".into(),
            ));
        }
        if !original.available {
            return Ok((
                false,
                format!("unavailable: {}", original.unavailable_reason),
            ));
        }
        if self.ensure_restore_host_idle(record.run_id).is_err() {
            return Ok((
                false,
                "unavailable while admitted work, an approval, or another restoration is active."
                    .into(),
            ));
        }
        if self
            .workspace
            .requires_approval(argentum_domain::Capability::WriteFiles)
            .is_err()
        {
            return Ok((
                false,
                "unavailable under the current workspace write policy.".into(),
            ));
        }
        if self
            .ensure_path_not_in_recovery(&record.relative_path)
            .is_err()
        {
            return Ok((
                false,
                "unavailable while this path has unresolved change recovery.".into(),
            ));
        }
        let current = self.current_snapshot(&record.relative_path)?;
        if !snapshot_refs_match(current.as_ref(), record.after.as_ref()) {
            return Ok((
                false,
                "conflict: current bytes or absence do not match the recorded postimage; later work was preserved.".into(),
            ));
        }
        Ok((
            true,
            "available; exact current postimage matches and owner review is required.".into(),
        ))
    }

    pub(super) fn recorded_change_diff(
        &self,
        change: argentum_domain::FileChangeId,
    ) -> Result<argentum_domain::UnifiedDiffSnapshot, RuntimeError> {
        self.recorded_change_diff_with_limit(change, DIFF_OUTPUT_BYTES)
    }

    fn recorded_change_diff_with_limit(
        &self,
        change: argentum_domain::FileChangeId,
        output_limit: usize,
    ) -> Result<argentum_domain::UnifiedDiffSnapshot, RuntimeError> {
        let record = self
            .store
            .file_change(self.project_id, change)?
            .ok_or(RuntimeError::ChangeNotFound)?;
        if record.project_id != self.project_id || record.id != change {
            return Err(RuntimeError::ChangeNotFound);
        }
        let unavailable = |reason: &str| argentum_domain::UnifiedDiffSnapshot {
            file_change_id: change,
            relative_path: self.review_display_path(&record.relative_path),
            text: String::new(),
            line_count: 0,
            byte_count: 0,
            truncated: false,
            available: false,
            unavailable_reason: reason.into(),
        };
        let policy = self.review_policy_stamp()?;
        if !self.review_body_allowed(&record.relative_path)? {
            return Ok(unavailable(
                "Current read/content policy does not permit this body.",
            ));
        }
        let before = self.snapshot_for_diff(record.before.as_ref())?;
        let after = self.snapshot_for_diff(record.after.as_ref())?;
        for snapshot in before.iter().chain(after.iter()) {
            let Some(bytes) = snapshot.bytes.as_deref() else {
                return Ok(unavailable(
                    "Snapshot missing, pruned, or over the 64 KiB per-image limit.",
                ));
            };
            let Ok(text) = std::str::from_utf8(bytes) else {
                return Ok(unavailable("Binary diff unavailable."));
            };
            if text.contains('\0') {
                return Ok(unavailable("Binary diff unavailable."));
            }
            if working_context::sensitive_body(text) {
                return Ok(unavailable(
                    "Historical body is filtered by the shared sensitive-content rules.",
                ));
            }
            if text.lines().count() > 2048 || text.lines().any(|line| line.len() > 4096) {
                return Ok(unavailable("Diff line/line-length limit exceeded."));
            }
        }
        let prepared = PreparedChange {
            relative_path: record.relative_path.clone(),
            kind: record.kind,
            before,
            after,
            additions: record.additions,
            removals: record.removals,
            line_count_availability: record.line_count_availability,
        };
        let mut diff = load_unified_diff(&prepared, output_limit)?;
        diff.file_change_id = change;
        if self.review_policy_stamp()? != policy
            || !self.review_body_allowed(&record.relative_path)?
        {
            return Ok(unavailable(
                "Read/content policy changed during inspection.",
            ));
        }
        Ok(diff)
    }
}

fn verification_summary(recorded: &argentum_store::ReviewRun) -> String {
    if !recorded.verification_available {
        return "Historical verification: unavailable (record could not be read safely).".into();
    }
    if recorded.verification.is_empty() {
        return "Historical verification: no recorded receipts; current bytes are not verified."
            .into();
    }
    let states = recorded
        .verification
        .iter()
        .map(|v| format!("{}={:?}", v.runner.id(), v.state))
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "Historical verification: {states}{}\nReceipt freshness for current workspace bytes is not established. Runtime Complete is not verification Passed.",
        if recorded.verification_complete {
            ""
        } else {
            " (partial; first 16 receipts)"
        }
    )
}
