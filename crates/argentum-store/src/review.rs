//! Bounded metadata projection of the existing journals. No writes or new index.
use super::*;
use serde::{Deserialize, Serialize};

pub const RUN_PAGE: usize = 16;
pub const RECORD_LIMIT: usize = 128;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReviewCursor {
    pub recorded_at: i64,
    pub sequence: i64,
    pub run_id: RunId,
}
#[derive(Debug, Clone)]
pub struct ReviewRuns {
    pub items: Vec<ReviewCursor>,
    pub next: Option<ReviewCursor>,
}
#[derive(Debug, Clone)]
pub struct ReviewRecord {
    pub id: Uuid,
    pub file_change_id: Option<FileChangeId>,
    pub operation_id: Option<ChangeOperationId>,
    pub path: String,
    pub kind: String,
    pub state: String,
}
use argentum_domain::{ChangePage, ChangePageCursor, ChangePageItem};
#[derive(Debug, Clone)]
pub struct ReviewRun {
    pub run_id: RunId,
    pub lifecycle: Option<argentum_domain::TaskLifecycle>,
    pub final_state: Option<FinalRunState>,
    pub records: Vec<ReviewRecord>,
    pub records_complete: bool,
    pub verification: Vec<VerificationEvidence>,
    pub verification_complete: bool,
    pub verification_available: bool,
}

impl Store {
    pub fn review_runs(
        &self,
        project: ProjectId,
        session: SessionId,
        before: Option<&ReviewCursor>,
    ) -> Result<ReviewRuns, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let mut query = connection.prepare(
            "SELECT CASE WHEN length(CAST(run_id AS BLOB))=36 THEN run_id ELSE '' END, stamp, sequence FROM (
              SELECT r.*,COALESCE((SELECT MAX(id) FROM event_log e WHERE e.project_id=?1 AND e.session_id=?2 AND e.run_id=r.run_id),0) AS sequence
              FROM (SELECT run_id, MAX(stamp) AS stamp FROM (
                SELECT run_id, created_at AS stamp FROM file_changes WHERE project_id=?1 AND session_id=?2
                UNION ALL SELECT run_id, created_at FROM change_operations WHERE project_id=?1 AND session_id=?2
                UNION ALL SELECT run_id, recorded_at FROM verification_evidence WHERE project_id=?1 AND session_id=?2
             ) GROUP BY run_id) r) WHERE (?3 IS NULL OR stamp<?3 OR (stamp=?3 AND (sequence<?4 OR (sequence=?4 AND run_id<?5))))
             ORDER BY stamp DESC, sequence DESC, run_id DESC LIMIT ?6")?;
        let rows = query.query_map(
            params![
                project.to_string(),
                session.to_string(),
                before.map(|c| c.recorded_at),
                before.map(|c| c.sequence),
                before.map(|c| c.run_id.to_string()),
                (RUN_PAGE + 1) as i64
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            },
        )?;
        let mut items = Vec::new();
        for row in rows {
            let (id, recorded_at, sequence) = row?;
            items.push(ReviewCursor {
                run_id: parse_uuid(&id, "review run")?,
                recorded_at,
                sequence,
            });
        }
        let more = items.len() > RUN_PAGE;
        items.truncate(RUN_PAGE);
        let next = more.then(|| items.last().cloned()).flatten();
        Ok(ReviewRuns { items, next })
    }

    pub fn review_run(
        &self,
        project: ProjectId,
        session: SessionId,
        run: RunId,
    ) -> Result<ReviewRun, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let exists:bool=connection.query_row("SELECT EXISTS(
            SELECT 1 FROM file_changes WHERE project_id=?1 AND session_id=?2 AND run_id=?3
            UNION ALL SELECT 1 FROM change_operations WHERE project_id=?1 AND session_id=?2 AND run_id=?3
            UNION ALL SELECT 1 FROM verification_evidence WHERE project_id=?1 AND session_id=?2 AND run_id=?3)",params![project.to_string(),session.to_string(),run.to_string()],|row|row.get(0))?;
        if !exists {
            return Err(StoreError::InvalidRecord(
                "review run unavailable in this scope".into(),
            ));
        }
        let lifecycle_json:Option<String>=connection.query_row("SELECT CASE WHEN length(CAST(payload AS BLOB))<=4096 THEN payload ELSE NULL END FROM event_log WHERE project_id=?1 AND session_id=?2 AND run_id=?3 AND event_type='run_status_changed' ORDER BY id DESC LIMIT 1",params![project.to_string(),session.to_string(),run.to_string()],|row|row.get(0)).optional()?.flatten();
        let lifecycle = lifecycle_json
            .and_then(|text| serde_json::from_str::<AppEvent>(&text).ok())
            .and_then(|event| match event {
                AppEvent::RunStatusChanged { run_id, lifecycle } if run_id == run => {
                    Some(lifecycle)
                }
                _ => None,
            });
        // Same typed final-state mapping used by F18A; assistant prose is not read.
        let final_state = lifecycle.and_then(FinalRunState::from_lifecycle);
        let mut query=connection.prepare("SELECT
            CASE WHEN length(CAST(id AS BLOB))=36 THEN id ELSE '' END,
            CASE WHEN file_id IS NULL THEN NULL WHEN length(CAST(file_id AS BLOB))=36 THEN file_id ELSE '' END,
            CASE WHEN operation_id IS NULL THEN NULL WHEN length(CAST(operation_id AS BLOB))=36 THEN operation_id ELSE '' END,
            path,
            CASE kind WHEN 'added' THEN 'created' WHEN 'modified' THEN 'modified' WHEN 'deleted' THEN 'deleted' ELSE 'unavailable' END,
            CASE state WHEN 'recorded applied' THEN 'recorded applied' WHEN 'prepared' THEN 'prepared' WHEN 'applied' THEN 'applied' WHEN 'not_applied' THEN 'not_applied' WHEN 'conflicted' THEN 'conflicted' ELSE 'unavailable' END FROM (
            SELECT f.id AS id,f.id AS file_id,(SELECT o.id FROM change_operations o WHERE o.project_id=f.project_id AND o.file_change_id=f.id LIMIT 1) AS operation_id,
              CASE WHEN length(CAST(f.relative_path AS BLOB))<=4096 THEN f.relative_path ELSE '[path exceeds metadata limit]' END AS path,
              CASE WHEN length(CAST(f.payload AS BLOB))<=16384 AND json_valid(f.payload) THEN json_extract(f.payload,'$.kind') ELSE 'unavailable' END AS kind,
              'recorded applied' AS state, f.created_at AS stamp,f.revision AS sequence
              FROM file_changes f WHERE f.project_id=?1 AND f.session_id=?2 AND f.run_id=?3
            UNION ALL SELECT o.id,NULL,o.id,CASE WHEN length(CAST(o.relative_path AS BLOB))<=4096 THEN o.relative_path ELSE '[path exceeds metadata limit]' END,o.change_kind,o.state,o.created_at,o.rowid
              FROM change_operations o WHERE o.project_id=?1 AND o.session_id=?2 AND o.run_id=?3 AND o.state<>'finalized'
            ) ORDER BY stamp,sequence,id LIMIT ?4")?;
        let rows = query.query_map(
            params![
                project.to_string(),
                session.to_string(),
                run.to_string(),
                (RECORD_LIMIT + 1) as i64
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                ))
            },
        )?;
        let mut records = Vec::new();
        for row in rows {
            let (id, file, operation, path, kind, state) = row?;
            records.push(ReviewRecord {
                id: parse_uuid(&id, "review item")?,
                file_change_id: file
                    .map(|id| parse_uuid(&id, "review change"))
                    .transpose()?,
                operation_id: operation
                    .map(|id| parse_uuid(&id, "review operation"))
                    .transpose()?,
                path,
                kind: kind.unwrap_or_else(|| "unavailable".into()),
                state,
            });
        }
        let records_complete = records.len() <= RECORD_LIMIT;
        records.truncate(RECORD_LIMIT);
        drop(query);
        let receipts = (|| -> Result<Vec<VerificationEvidence>, StoreError> {
            let mut query = connection.prepare("SELECT substr(id,1,37), CASE WHEN length(CAST(payload AS BLOB))<=131072 THEN payload ELSE NULL END FROM verification_evidence WHERE project_id=?1 AND session_id=?2 AND run_id=?3 ORDER BY recorded_at,id LIMIT 17")?;
            let rows = query.query_map(
                params![project.to_string(), session.to_string(), run.to_string()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )?;
            let mut receipts = Vec::new();
            for row in rows {
                let (id, payload) = row?;
                let receipt: VerificationEvidence = serde_json::from_str(&payload)?;
                validate_verification_evidence(&receipt)?;
                if receipt.id.to_string() != id || receipt.run_id != run {
                    return Err(StoreError::InvalidRecord(
                        "review receipt scope mismatch".into(),
                    ));
                }
                receipts.push(receipt);
            }
            Ok(receipts)
        })();
        let (mut verification, verification_available) = match receipts {
            Ok(v) => (v, true),
            Err(_) => (Vec::new(), false),
        };
        let verification_complete = verification_available && verification.len() <= 16;
        verification.truncate(16);
        Ok(ReviewRun {
            run_id: run,
            lifecycle,
            final_state,
            records,
            records_complete,
            verification,
            verification_complete,
            verification_available,
        })
    }

    pub fn change_page(
        &self,
        project: ProjectId,
        session: SessionId,
        run: RunId,
        limit: usize,
        after: Option<&ChangePageCursor>,
    ) -> Result<ChangePage, StoreError> {
        if !(1..=RECORD_LIMIT).contains(&limit) {
            return Err(StoreError::InvalidRecord(
                "change page limit invalid".into(),
            ));
        }
        if let Some(cursor) = after {
            if cursor.project_id != project
                || cursor.session_id != session
                || cursor.run_id != run
                || cursor.stamp < 0
                || cursor.sequence < 0
            {
                return Err(StoreError::InvalidRecord(
                    "change page cursor invalid".into(),
                ));
            }
        }
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let exists: bool = connection.query_row(
            "SELECT EXISTS(
                SELECT 1 FROM file_changes WHERE project_id=?1 AND session_id=?2 AND run_id=?3
                UNION ALL SELECT 1 FROM change_operations WHERE project_id=?1 AND session_id=?2 AND run_id=?3
                UNION ALL SELECT 1 FROM verification_evidence WHERE project_id=?1 AND session_id=?2 AND run_id=?3)",
            params![project.to_string(), session.to_string(), run.to_string()],
            |row| row.get(0),
        )?;

        if !exists {
            let recognized: bool = connection.query_row(
                "SELECT EXISTS(SELECT 1 FROM verification_evidence WHERE project_id=?1 AND session_id=?2 AND run_id=?3)",
                params![project.to_string(), session.to_string(), run.to_string()],
                |row| row.get(0),
            )?;
            if !recognized {
                return Err(StoreError::InvalidRecord(
                    "review run unavailable in this scope".into(),
                ));
            }
            if after.is_some() {
                return Err(StoreError::InvalidRecord(
                    "change page cursor unavailable in this scope".into(),
                ));
            }
            return Ok(ChangePage {
                run_id: run,
                items: Vec::new(),
                next_cursor: None,
                complete: true,
            });
        }
        if let Some(cursor) = after {
            let cursor_matches: bool = connection.query_row(
                "SELECT EXISTS(
                    SELECT 1 FROM file_changes WHERE project_id=?1 AND session_id=?2 AND run_id=?3 AND id=?4 AND created_at=?5 AND revision=?6
                    UNION ALL SELECT 1 FROM change_operations WHERE project_id=?1 AND session_id=?2 AND run_id=?3 AND id=?4 AND created_at=?5 AND rowid=?6 AND state<>'finalized')",
                params![project.to_string(), session.to_string(), run.to_string(), cursor.id.to_string(), cursor.stamp, cursor.sequence],
                |row| row.get(0),
            )?;
            if !cursor_matches {
                return Err(StoreError::InvalidRecord(
                    "change page cursor unavailable in this scope".into(),
                ));
            }
        }
        let mut query = connection.prepare(
            "SELECT
                CASE WHEN length(CAST(id AS BLOB))=36 THEN id ELSE '' END,
                CASE WHEN file_id IS NULL THEN NULL WHEN length(CAST(file_id AS BLOB))=36 THEN file_id ELSE '' END,
                CASE WHEN operation_id IS NULL THEN NULL WHEN length(CAST(operation_id AS BLOB))=36 THEN operation_id ELSE '' END,
                path,
                CASE kind WHEN 'added' THEN 'created' WHEN 'modified' THEN 'modified' WHEN 'deleted' THEN 'deleted' ELSE 'unavailable' END,
                CASE state WHEN 'recorded applied' THEN 'recorded applied' WHEN 'prepared' THEN 'prepared' WHEN 'applied' THEN 'applied' WHEN 'not_applied' THEN 'not_applied' WHEN 'conflicted' THEN 'conflicted' ELSE 'unavailable' END,
                stamp, sequence FROM (
                SELECT f.id AS id,f.id AS file_id,(SELECT o.id FROM change_operations o WHERE o.project_id=f.project_id AND o.file_change_id=f.id LIMIT 1) AS operation_id,
                  CASE WHEN length(CAST(f.relative_path AS BLOB))<=4096 THEN f.relative_path ELSE '[path exceeds metadata limit]' END AS path,
                  CASE WHEN length(CAST(f.payload AS BLOB))<=16384 AND json_valid(f.payload) THEN json_extract(f.payload,'$.kind') ELSE 'unavailable' END AS kind,
                  'recorded applied' AS state, f.created_at AS stamp,f.revision AS sequence
                  FROM file_changes f WHERE f.project_id=?1 AND f.session_id=?2 AND f.run_id=?3
                UNION ALL SELECT o.id,NULL,o.id,CASE WHEN length(CAST(o.relative_path AS BLOB))<=4096 THEN o.relative_path ELSE '[path exceeds metadata limit]' END,o.change_kind,o.state,o.created_at,o.rowid
                  FROM change_operations o WHERE o.project_id=?1 AND o.session_id=?2 AND o.run_id=?3 AND o.state<>'finalized'
                ) WHERE (?4 IS NULL OR stamp>?4 OR (stamp=?4 AND (sequence>?5 OR (sequence=?5 AND id>?6))))
                ORDER BY stamp,sequence,id LIMIT ?7",
        )?;
        let rows = query.query_map(
            params![
                project.to_string(),
                session.to_string(),
                run.to_string(),
                after.map(|c| c.stamp),
                after.map(|c| c.sequence),
                after.map(|c| c.id.to_string()),
                (limit + 1) as i64
            ],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, Option<String>>(1)?,
                    row.get::<_, Option<String>>(2)?,
                    row.get::<_, String>(3)?,
                    row.get::<_, Option<String>>(4)?,
                    row.get::<_, String>(5)?,
                    row.get::<_, i64>(6)?,
                    row.get::<_, i64>(7)?,
                ))
            },
        )?;
        let mut items = Vec::new();
        let mut last: Option<ChangePageCursor> = None;
        for row in rows {
            let (id, file, operation, path, kind, state, stamp, sequence) = row?;
            let item_id = parse_uuid(&id, "change page item")?;
            if items.len() < limit {
                last = Some(ChangePageCursor {
                    project_id: project,
                    session_id: session,
                    run_id: run,
                    stamp,
                    sequence,
                    id: item_id,
                });
            }
            items.push(ChangePageItem {
                id: item_id,
                file_change_id: file
                    .map(|id| parse_uuid(&id, "change page change"))
                    .transpose()?,
                operation_id: operation
                    .map(|id| parse_uuid(&id, "change page operation"))
                    .transpose()?,
                path,
                kind: kind.unwrap_or_else(|| "unavailable".into()),
                state,
            });
        }
        let complete = items.len() <= limit;
        items.truncate(limit);
        let next_cursor = if complete { None } else { last };
        Ok(ChangePage {
            run_id: run,
            items,
            next_cursor,
            complete,
        })
    }

    pub fn change_blob_bounded(
        &self,
        project: ProjectId,
        digest: &Sha256Digest,
        limit: usize,
    ) -> Result<Option<Vec<u8>>, StoreError> {
        if limit > MAX_CHANGE_SNAPSHOT_BYTES {
            return Err(StoreError::InvalidRecord(
                "review blob limit invalid".into(),
            ));
        }
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        Ok(connection.query_row("SELECT bytes FROM change_blobs WHERE project_id=?1 AND digest=?2 AND length(bytes)<=?3 AND size_bytes=length(bytes)",params![project.to_string(),digest.0,limit as i64],|row|row.get(0)).optional()?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn operation(scope: &WorkspaceResolution, run: RunId) -> PreparedChangeOperation {
        let bytes = b"fixture";
        PreparedChangeOperation {
            operation_id: Uuid::new_v4(),
            workspace_key: scope.workspace_key.clone(),
            project_id: scope.snapshot.project.id,
            session_id: scope.snapshot.active_session_id.unwrap(),
            run_id: run,
            relative_path: "file.txt".into(),
            kind: argentum_domain::FileChangeKind::Added,
            origin: argentum_domain::FileChangeOrigin::ToolCall,
            before: None,
            after: Some(FileSnapshotRef {
                digest: digest_bytes(bytes),
                size_bytes: bytes.len() as u64,
                retained: true,
            }),
            before_bytes: None,
            after_bytes: Some(bytes.to_vec()),
            additions: 1,
            removals: 0,
            line_count_availability: argentum_domain::LineCountAvailability::Unknown,
            restored_from: None,
        }
    }
    #[test]
    fn f20d_review_keyset_pages_partial_records_and_journal_states_are_read_only() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let scope = store.resolve_workspace(root.path()).unwrap();
        let project = scope.snapshot.project.id;
        let session = scope.snapshot.active_session_id.unwrap();
        for _ in 0..18 {
            let op = operation(&scope, Uuid::new_v4());
            store.prepare_change_operation(&op).unwrap();
            store
                .mark_change_operation_applied(op.operation_id)
                .unwrap();
            store.finalize_change_operation(op.operation_id).unwrap();
        }
        let run = Uuid::new_v4();
        for i in 0..129 {
            let mut op = operation(&scope, run);
            op.relative_path = format!("entry-{i}.txt");
            store.prepare_change_operation(&op).unwrap();
            match i % 4 {
                1 => store
                    .mark_change_operation_applied(op.operation_id)
                    .unwrap(),
                2 => store
                    .resolve_change_operation(
                        op.operation_id,
                        ChangeRecoveryState::NotApplied,
                        None,
                    )
                    .unwrap(),
                3 => store
                    .resolve_change_operation(
                        op.operation_id,
                        ChangeRecoveryState::Conflicted,
                        None,
                    )
                    .unwrap(),
                _ => {}
            }
        }
        {
            let connection = store.connection.lock().unwrap();
            connection
                .execute(
                    "UPDATE file_changes SET created_at=1000 WHERE run_id=?1",
                    [run.to_string()],
                )
                .unwrap();
            connection
                .execute(
                    "UPDATE change_operations SET created_at=1000 WHERE run_id=?1",
                    [run.to_string()],
                )
                .unwrap();
        }
        let changes = || {
            store
                .connection
                .lock()
                .unwrap()
                .query_row("SELECT total_changes()", [], |r| r.get::<_, i64>(0))
                .unwrap()
        };
        let baseline = changes();
        let first = store.review_runs(project, session, None).unwrap();
        assert_eq!(first.items.len(), 16);
        let second = store
            .review_runs(project, session, first.next.as_ref())
            .unwrap();
        assert_eq!(second.items.len(), 3);
        assert!(second.next.is_none());
        assert!(first
            .items
            .iter()
            .all(|a| second.items.iter().all(|b| a.run_id != b.run_id)));
        let first_page = store.change_page(project, session, run, 64, None).unwrap();
        assert_eq!(first_page.items.len(), 64);
        assert!(!first_page.complete);
        let second_page = store
            .change_page(project, session, run, 64, first_page.next_cursor.as_ref())
            .unwrap();
        assert_eq!(second_page.items.len(), 64);
        assert!(!second_page.complete);
        let third_page = store
            .change_page(project, session, run, 64, second_page.next_cursor.as_ref())
            .unwrap();
        assert_eq!(third_page.items.len(), 1);
        assert!(third_page.complete);
        assert!(third_page.next_cursor.is_none());
        let mut ids = first_page
            .items
            .iter()
            .chain(&second_page.items)
            .chain(&third_page.items)
            .map(|item| item.id)
            .collect::<Vec<_>>();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), 129);
        let valid_cursor = first_page.next_cursor.as_ref().unwrap();
        for bad_scope in [
            ChangePageCursor {
                project_id: Uuid::new_v4(),
                ..valid_cursor.clone()
            },
            ChangePageCursor {
                session_id: Uuid::new_v4(),
                ..valid_cursor.clone()
            },
            ChangePageCursor {
                run_id: Uuid::new_v4(),
                ..valid_cursor.clone()
            },
        ] {
            assert!(store
                .change_page(project, session, run, 64, Some(&bad_scope))
                .is_err());
        }
        let bad_cursor = ChangePageCursor {
            stamp: -1,
            ..valid_cursor.clone()
        };
        assert!(store
            .change_page(project, session, run, 64, Some(&bad_cursor))
            .is_err());
        let bad_sequence = ChangePageCursor {
            sequence: -1,
            ..valid_cursor.clone()
        };
        assert!(store
            .change_page(project, session, run, 64, Some(&bad_sequence))
            .is_err());
        let max_page = store.change_page(project, session, run, 128, None).unwrap();
        assert_eq!(max_page.items.len(), 128);
        assert!(!max_page.complete);
        let foreign_project = Uuid::new_v4();
        assert!(store
            .change_page(
                foreign_project,
                session,
                run,
                64,
                first_page.next_cursor.as_ref()
            )
            .is_err());
        assert!(store
            .change_page(
                project,
                Uuid::new_v4(),
                run,
                64,
                first_page.next_cursor.as_ref()
            )
            .is_err());
        assert!(store.change_page(project, session, run, 0, None).is_err());
        assert!(store.change_page(project, session, run, 129, None).is_err());
        assert!(store
            .change_page(project, session, Uuid::new_v4(), 64, None)
            .is_err());
        let records = store.review_run(project, session, run).unwrap();
        assert_eq!(records.records.len(), 128);
        assert!(!records.records_complete);
        for state in ["prepared", "applied", "not_applied", "conflicted"] {
            assert!(records.records.iter().any(|r| r.state == state));
        }
        assert!(store.review_run(project, Uuid::new_v4(), run).is_err());
        assert!(store
            .review_runs(Uuid::new_v4(), session, None)
            .unwrap()
            .items
            .is_empty());
        assert_eq!(changes(), baseline);
        // Finalized operations are represented by their file-change row only.
        let finalized_run = Uuid::new_v4();
        let finalized = operation(&scope, finalized_run);
        store.prepare_change_operation(&finalized).unwrap();
        store
            .mark_change_operation_applied(finalized.operation_id)
            .unwrap();
        store
            .finalize_change_operation(finalized.operation_id)
            .unwrap();
        let page = store
            .change_page(project, session, finalized_run, 64, None)
            .unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].file_change_id, Some(page.items[0].id));
        assert!(page.items[0].operation_id.is_some());
        assert!(page.complete);
    }
    #[test]
    fn change_page_mixed_journal_ties_are_stable_at_every_page_boundary() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let scope = store.resolve_workspace(root.path()).unwrap();
        let project = scope.snapshot.project.id;
        let session = scope.snapshot.active_session_id.unwrap();
        let run = Uuid::new_v4();
        let mut expected = Vec::new();
        for _ in 0..66 {
            let finalized = operation(&scope, run);
            store.prepare_change_operation(&finalized).unwrap();
            store
                .mark_change_operation_applied(finalized.operation_id)
                .unwrap();
            let file = store
                .finalize_change_operation(finalized.operation_id)
                .unwrap();
            let pending = operation(&scope, run);
            store.prepare_change_operation(&pending).unwrap();
            let db = store.connection.lock().unwrap();
            let sequence: i64 = db
                .query_row(
                    "SELECT rowid FROM change_operations WHERE id=?1",
                    [pending.operation_id.to_string()],
                    |row| row.get(0),
                )
                .unwrap();
            // Force both ordering fields to tie across the two journal sources.
            db.execute(
                "UPDATE file_changes SET created_at=1000,revision=?1 WHERE id=?2",
                params![sequence, file.id.to_string()],
            )
            .unwrap();
            db.execute(
                "UPDATE change_operations SET created_at=1000 WHERE id=?1",
                [pending.operation_id.to_string()],
            )
            .unwrap();
            expected.push((sequence, file.id));
            expected.push((sequence, pending.operation_id));
        }
        expected.sort_by_key(|(sequence, id)| (*sequence, id.to_string()));
        let expected_ids: Vec<_> = expected.iter().map(|(_, id)| *id).collect();
        for limit in [1, 2, 64, 128] {
            let mut cursor = None;
            let mut actual = Vec::new();
            loop {
                let page = store
                    .change_page(project, session, run, limit, cursor.as_ref())
                    .unwrap();
                assert!(page.items.len() <= limit);
                for item in &page.items {
                    assert_eq!(item.kind, "created");
                    if item.file_change_id.is_some() {
                        assert_eq!(item.state, "recorded applied");
                        assert!(item.operation_id.is_some());
                    } else {
                        assert_eq!(item.state, "prepared");
                        assert_eq!(item.operation_id, Some(item.id));
                    }
                }
                actual.extend(page.items.iter().map(|item| item.id));
                assert_eq!(page.complete, actual.len() == expected_ids.len());
                if page.complete {
                    assert!(page.next_cursor.is_none());
                    break;
                }
                let next = page.next_cursor.expect("nonterminal continuation");
                assert_ne!(cursor.as_ref(), Some(&next));
                cursor = Some(next);
                assert!(actual.len() < expected_ids.len());
            }
            assert_eq!(actual, expected_ids);
        }
    }

    #[test]
    fn change_page_empty_recognized_journal_does_not_read_receipt_bodies() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let scope = store.resolve_workspace(root.path()).unwrap();
        let project = scope.snapshot.project.id;
        let session = scope.snapshot.active_session_id.unwrap();
        let run = Uuid::new_v4();
        // Recognition uses identity only, even when the receipt body is invalid.
        store.connection.lock().unwrap().execute(
            "INSERT INTO verification_evidence(id,project_id,session_id,run_id,runner_id,recorded_at,payload) VALUES(?1,?2,?3,?4,'cargo-test-locked',0,?5)",
            params![Uuid::new_v4().to_string(), project.to_string(), session.to_string(), run.to_string(), "x".repeat(131073)],
        ).unwrap();
        for limit in [1, 64, 128] {
            let page = store
                .change_page(project, session, run, limit, None)
                .unwrap();
            assert_eq!(page.run_id, run);
            assert!(page.items.is_empty());
            assert!(page.complete);
            assert!(page.next_cursor.is_none());
        }
        assert!(store
            .change_page(Uuid::new_v4(), session, run, 64, None)
            .is_err());
        assert!(store
            .change_page(project, Uuid::new_v4(), run, 64, None)
            .is_err());
    }

    #[test]
    fn f20d_review_guards_blob_metadata_and_receipt_allocation() {
        let root = tempfile::tempdir().unwrap();
        let store = Store::open_in_memory().unwrap();
        let scope = store.resolve_workspace(root.path()).unwrap();
        let run = Uuid::new_v4();
        let op = operation(&scope, run);
        let project = scope.snapshot.project.id;
        let session = scope.snapshot.active_session_id.unwrap();
        store.prepare_change_operation(&op).unwrap();
        store
            .mark_change_operation_applied(op.operation_id)
            .unwrap();
        let record = store.finalize_change_operation(op.operation_id).unwrap();
        let digest = &record.after.as_ref().unwrap().digest;
        assert!(store
            .change_blob_bounded(project, digest, 2)
            .unwrap()
            .is_none());
        assert_eq!(
            store
                .change_blob_bounded(project, digest, 64)
                .unwrap()
                .unwrap(),
            b"fixture"
        );
        {
            let db = store.connection.lock().unwrap();
            db.execute(
                "UPDATE file_changes SET payload=?1,relative_path=?2 WHERE id=?3",
                params!["x".repeat(16385), "p".repeat(4097), record.id.to_string()],
            )
            .unwrap();
            db.execute(
                "UPDATE change_blobs SET bytes=zeroblob(65537),size_bytes=65537",
                [],
            )
            .unwrap();
            db.execute("INSERT INTO verification_evidence(id,project_id,session_id,run_id,runner_id,recorded_at,payload) VALUES(?1,?2,?3,?4,'cargo-test-locked',0,?5)",params![Uuid::new_v4().to_string(),project.to_string(),session.to_string(),run.to_string(),"x".repeat(131073)]).unwrap();
        }
        assert!(store.file_change(project, record.id).unwrap().is_none());
        assert!(store
            .change_blob_bounded(project, digest, 65536)
            .unwrap()
            .is_none());
        let page = store.change_page(project, session, run, 1, None).unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].path, "[path exceeds metadata limit]");
        assert_eq!(page.items[0].kind, "unavailable");
        assert_eq!(page.items[0].state, "recorded applied");
        assert!(page.complete);
        assert!(page.next_cursor.is_none());
        let review = store.review_run(project, session, run).unwrap();
        assert_eq!(review.records[0].path, "[path exceeds metadata limit]");
        assert_eq!(review.records[0].kind, "unavailable");
        assert!(!review.verification_available);
        assert!(review.verification.is_empty());
    }
}
