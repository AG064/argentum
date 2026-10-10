use argentum_domain::context_summary::{SummaryCheckpoint, MAX_CHECKPOINT_BYTES};
use argentum_domain::{ProjectId, SessionId};
use rusqlite::{params, OptionalExtension};

use super::{Store, StoreError};

/// Process-local optimistic snapshot. Mutations invalidate it, including
/// unrelated sessions and external connections. Only exact successful writes
/// made by local accounting and recovery-ownership transactions are excluded:
/// they change no context source, instruction, policy, or owner configuration.
/// Trigger writes and failed transactions are not excluded. Never an authority
/// token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextCommitToken {
    changes: u64,
    external_version: u64,
}

pub struct SummarySnapshot {
    pub token: ContextCommitToken,
    pub checkpoint: Option<SummaryCheckpoint>,
}

fn token(
    store: &Store,
    connection: &rusqlite::Connection,
) -> Result<ContextCommitToken, StoreError> {
    Ok(ContextCommitToken {
        changes: connection
            .total_changes()
            .checked_sub(
                store
                    .context_neutral_changes
                    .load(std::sync::atomic::Ordering::Relaxed),
            )
            .ok_or_else(|| StoreError::InvalidRecord("context change counter underflow".into()))?,
        external_version: connection.query_row("PRAGMA data_version", [], |row| row.get(0))?,
    })
}

fn check_scope(
    connection: &rusqlite::Connection,
    project: ProjectId,
    session: SessionId,
) -> Result<(), StoreError> {
    let exists: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM sessions WHERE id=?1 AND project_id=?2)",
        params![session.to_string(), project.to_string()],
        |row| row.get(0),
    )?;
    if !exists {
        return Err(StoreError::InvalidRecord(
            "summary session scope mismatch".into(),
        ));
    }
    Ok(())
}

impl Store {
    pub fn summary_snapshot(
        &self,
        project: ProjectId,
        session: SessionId,
    ) -> Result<SummarySnapshot, StoreError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let tx = connection.transaction()?;
        check_scope(&tx, project, session)?;
        let before = token(self, &tx)?;
        let conflicting_scope: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM context_checkpoints WHERE session_id=?1 AND project_id<>?2)",
            params![session.to_string(), project.to_string()], |row| row.get(0))?;
        if conflicting_scope {
            return Err(StoreError::InvalidRecord(
                "summary row scope mismatch; deactivate it locally".into(),
            ));
        }
        let payload: Option<(Option<String>, bool)> = tx
            .query_row(
                "SELECT CASE WHEN length(CAST(payload AS BLOB)) <= ?3 THEN payload ELSE NULL END,
                payload IS NULL OR length(CAST(payload AS BLOB)) <= ?3
             FROM context_checkpoints WHERE project_id=?1 AND session_id=?2",
                params![
                    project.to_string(),
                    session.to_string(),
                    MAX_CHECKPOINT_BYTES as i64
                ],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;
        let checkpoint = match payload {
            None | Some((None, true)) => None,
            Some((Some(payload), true)) => {
                let checkpoint: SummaryCheckpoint =
                    serde_json::from_str(&payload).map_err(|_| {
                        StoreError::InvalidRecord(
                            "malformed summary checkpoint; deactivate it locally".into(),
                        )
                    })?;
                checkpoint
                    .validate()
                    .map_err(|reason| StoreError::InvalidRecord(reason.into()))?;
                if checkpoint.project_id != project || checkpoint.session_id != session {
                    return Err(StoreError::InvalidRecord(
                        "summary payload scope mismatch".into(),
                    ));
                }
                Some(checkpoint)
            }
            _ => {
                return Err(StoreError::InvalidRecord(
                    "summary checkpoint exceeds bound".into(),
                ))
            }
        };
        tx.commit()?;
        Ok(SummarySnapshot {
            token: before,
            checkpoint,
        })
    }

    /// The candidate exists only in memory until all validation has completed.
    /// One transaction commits the complete body and coverage. There is no cursor
    /// pointing at an unfinished summary and no restart-time model work.
    pub fn activate_summary(
        &self,
        expected: ContextCommitToken,
        candidate: &SummaryCheckpoint,
    ) -> Result<(), StoreError> {
        self.activate_summary_if(expected, candidate, || true)
    }

    pub fn activate_summary_if(
        &self,
        expected: ContextCommitToken,
        candidate: &SummaryCheckpoint,
        still_active: impl Fn() -> bool,
    ) -> Result<(), StoreError> {
        candidate
            .validate()
            .map_err(|reason| StoreError::InvalidRecord(reason.into()))?;
        let payload = serde_json::to_string(candidate)?;
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let tx = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        check_scope(&tx, candidate.project_id, candidate.session_id)?;
        if token(self, &tx)? != expected || !still_active() {
            return Err(StoreError::InvalidRecord(
                "summary snapshot changed; prior checkpoint preserved".into(),
            ));
        }
        let activated = tx.execute("INSERT INTO context_checkpoints(session_id, project_id, payload) VALUES(?1, ?2, ?3)
            ON CONFLICT(session_id) DO UPDATE SET payload=excluded.payload WHERE project_id=excluded.project_id",
            params![candidate.session_id.to_string(), candidate.project_id.to_string(), payload])?;
        if activated != 1 {
            return Err(StoreError::InvalidRecord(
                "summary activation scope mismatch".into(),
            ));
        }
        if !still_active() {
            return Err(StoreError::InvalidRecord(
                "summary cancelled before commit".into(),
            ));
        }
        tx.commit()?;
        Ok(())
    }

    /// Owner recovery also works for malformed stored payloads. Originals and
    /// receipts are untouched. Absence of a summary does not promise capacity.
    pub fn deactivate_summary(
        &self,
        project: ProjectId,
        session: SessionId,
    ) -> Result<bool, StoreError> {
        self.deactivate_summary_if(project, session, || true)
    }

    pub fn deactivate_summary_if(
        &self,
        project: ProjectId,
        session: SessionId,
        still_active: impl Fn() -> bool,
    ) -> Result<bool, StoreError> {
        let mut connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let tx = connection.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        check_scope(&tx, project, session)?;
        if !still_active() {
            return Err(StoreError::InvalidRecord(
                "summary deactivation cancelled".into(),
            ));
        }
        let changed = tx.execute(
            "UPDATE context_checkpoints SET payload=NULL, project_id=?1
            WHERE session_id=?2 AND (payload IS NOT NULL OR project_id<>?1)",
            params![project.to_string(), session.to_string()],
        )? == 1;
        if !still_active() {
            return Err(StoreError::InvalidRecord(
                "summary deactivation cancelled".into(),
            ));
        }
        tx.commit()?;
        Ok(changed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use argentum_domain::context_summary::{SourceCoverage, SummaryNarrative};
    use uuid::Uuid;

    fn candidate(project: ProjectId, session: SessionId) -> SummaryCheckpoint {
        let identity = format!("{}:0:group", Uuid::new_v4());
        SummaryCheckpoint {
            version: 1,
            id: Uuid::new_v4(),
            project_id: project,
            session_id: session,
            budget_revision: Uuid::new_v4(),
            input_binding_sha256: "a".repeat(64),
            sources: vec![SourceCoverage {
                identity: identity.clone(),
                projection_sha256: "b".repeat(64),
            }],
            narrative: SummaryNarrative {
                objective: "Historical test objective".into(),
                decisions_constraints: vec![],
                observed_changes_verification: vec![],
                unresolved_work: vec![],
                source_references: vec![identity],
            },
            evidence_anchors: vec![],
        }
    }

    #[test]
    fn summary_diagnostic_is_neutral_but_its_trigger_writes_are_not() {
        use argentum_domain::{context_summary::SummaryTransportObservation, AppEvent};
        let root = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let resolved = store.resolve_workspace(root.path()).unwrap();
        let project = resolved.snapshot.project.id;
        let session = resolved.snapshot.active_session_id.unwrap();
        let run = Uuid::new_v4();
        let scope = super::super::EventScope {
            workspace_key: resolved.workspace_key,
            project_id: project,
            session_id: Some(session),
            run_id: Some(run),
        };
        let event = AppEvent::SummaryTransportObserved {
            session_id: session,
            run_id: run,
            request_id: Uuid::new_v4(),
            observation: SummaryTransportObservation {
                envelope_ms: 120000,
                execution_budget_ms: 115000,
                elapsed_ms: 12000,
                first_event_ms: Some(12000),
                transport_content_bytes: Some(100),
                retained_content_bytes: Some(100),
                terminal_event_seen: true,
                valid_usage_observed: true,
                usage_invalid: false,
                primary_stop: "completed".into(),
                cancellation_source: "none".into(),
                worker_result: "completed".into(),
                drain_result: "settled".into(),
                worker_settled: true,
            },
        };
        let before = store.summary_snapshot(project, session).unwrap().token;
        store
            .append_context_neutral_event_scoped_record(&scope, &event)
            .unwrap();
        assert_eq!(
            before,
            store.summary_snapshot(project, session).unwrap().token
        );
        store.connection.lock().unwrap().execute_batch("CREATE TABLE summary_context_trigger(value INTEGER); CREATE TRIGGER summary_diagnostic_context_change AFTER INSERT ON event_log WHEN NEW.event_type='summary_transport_observed' BEGIN INSERT INTO summary_context_trigger VALUES(1); END;").unwrap();
        let before = store.summary_snapshot(project, session).unwrap().token;
        store
            .append_context_neutral_event_scoped_record(&scope, &event)
            .unwrap();
        assert_ne!(
            before,
            store.summary_snapshot(project, session).unwrap().token
        );
        assert!(store
            .activate_summary(before, &candidate(project, session))
            .is_err());
    }

    #[test]
    fn atomic_checkpoint_restart_conflict_failure_and_deactivation() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("state.db");
        let store = Store::open(&path).unwrap();
        let state = store.resolve_workspace(root.path()).unwrap().snapshot;
        let project = state.project.id;
        let session = state.active_session_id.unwrap();
        let first = candidate(project, session);
        let snapshot = store.summary_snapshot(project, session).unwrap();
        store.activate_summary(snapshot.token, &first).unwrap();
        let second = candidate(project, session);
        assert!(store.activate_summary(snapshot.token, &second).is_err());
        assert_eq!(
            store
                .summary_snapshot(project, session)
                .unwrap()
                .checkpoint
                .unwrap(),
            first
        );
        let before_cancel = store.summary_snapshot(project, session).unwrap();
        let cancellation_checks = std::cell::Cell::new(0);
        assert!(store
            .activate_summary_if(before_cancel.token, &second, || {
                cancellation_checks.set(cancellation_checks.get() + 1);
                cancellation_checks.get() == 1
            })
            .is_err());
        assert_eq!(
            store
                .summary_snapshot(project, session)
                .unwrap()
                .checkpoint
                .unwrap(),
            first
        );
        let external = rusqlite::Connection::open(&path).unwrap();
        let stale = store.summary_snapshot(project, session).unwrap();
        external
            .execute_batch("CREATE TABLE fixture_change(value INTEGER)")
            .unwrap();
        assert!(store.activate_summary(stale.token, &second).is_err());
        external.execute_batch("CREATE TRIGGER fail_summary BEFORE UPDATE ON context_checkpoints BEGIN SELECT RAISE(ROLLBACK,'owned failure'); END").unwrap();
        let valid = store.summary_snapshot(project, session).unwrap();
        assert!(store.activate_summary(valid.token, &second).is_err());
        drop(store);
        let reopened = Store::open(&path).unwrap();
        assert_eq!(
            reopened
                .summary_snapshot(project, session)
                .unwrap()
                .checkpoint
                .unwrap(),
            first
        );
        external.execute_batch("DROP TRIGGER fail_summary").unwrap();
        assert!(reopened.deactivate_summary(project, session).unwrap());
        assert!(reopened
            .summary_snapshot(project, session)
            .unwrap()
            .checkpoint
            .is_none());
    }

    #[test]
    fn malformed_stored_checkpoint_is_explicit_and_owner_can_deactivate_it() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let state = store.resolve_workspace(root.path()).unwrap().snapshot;
        let project = state.project.id;
        let session = state.active_session_id.unwrap();
        store
            .connection
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO context_checkpoints(session_id,project_id,payload) VALUES(?1,?2,'{}')",
                params![session.to_string(), project.to_string()],
            )
            .unwrap();
        assert!(store.summary_snapshot(project, session).is_err());
        assert!(store.deactivate_summary(project, session).unwrap());
        assert!(store
            .summary_snapshot(project, session)
            .unwrap()
            .checkpoint
            .is_none());
    }

    #[test]
    fn duplicate_and_excessive_original_coverage_cannot_replace_a_checkpoint() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let state = store.resolve_workspace(root.path()).unwrap().snapshot;
        let project = state.project.id;
        let session = state.active_session_id.unwrap();
        let first = candidate(project, session);
        store
            .activate_summary(
                store.summary_snapshot(project, session).unwrap().token,
                &first,
            )
            .unwrap();
        for duplicate in [false, true] {
            let mut invalid = candidate(project, session);
            invalid.sources = (0..if duplicate { 2 } else { 97 })
                .map(|index| SourceCoverage {
                    identity: if duplicate {
                        first.sources[0].identity.clone()
                    } else {
                        format!("{}:0:group", Uuid::from_u128(index + 1))
                    },
                    projection_sha256: "c".repeat(64),
                })
                .collect();
            invalid.narrative.source_references = invalid
                .sources
                .iter()
                .map(|source| source.identity.clone())
                .collect();
            assert!(store
                .activate_summary(
                    store.summary_snapshot(project, session).unwrap().token,
                    &invalid
                )
                .is_err());
            assert_eq!(
                store
                    .summary_snapshot(project, session)
                    .unwrap()
                    .checkpoint
                    .unwrap(),
                first
            );
        }
    }
}
