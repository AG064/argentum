//! Bounded host facts for subsequent model requests. No raw error bodies escape.
use super::*;

#[derive(Debug, Clone, serde::Serialize)]
pub struct RunOutcome {
    pub run_id: RunId,
    pub state: FinalRunState,
    pub reason: Option<&'static str>,
    pub recorded_file_changes: u64,
}

impl Store {
    pub fn recent_run_outcomes(
        &self,
        project: ProjectId,
        session: SessionId,
        exclude: Option<RunId>,
    ) -> Result<Vec<RunOutcome>, StoreError> {
        let db = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        let mut query=db.prepare("WITH latest AS (
            SELECT run_id,MAX(id) AS seq FROM event_log WHERE project_id=?1 AND session_id=?2 AND event_type='run_status_changed' AND (?3 IS NULL OR run_id<>?3) GROUP BY run_id
        ) SELECT substr(e.run_id,1,37),e.payload FROM latest l JOIN event_log e ON e.id=l.seq WHERE CASE WHEN length(CAST(e.payload AS BLOB))<=4096 AND json_valid(e.payload) THEN json_extract(e.payload,'$.RunStatusChanged.lifecycle') IN ('Complete','Failed','Cancelled','Interrupted') ELSE 0 END ORDER BY e.id DESC LIMIT 4")?;
        let rows = query.query_map(
            params![
                project.to_string(),
                session.to_string(),
                exclude.map(|id| id.to_string())
            ],
            |r| Ok((r.get::<_, String>(0)?, r.get::<_, Option<String>>(1)?)),
        )?;
        let mut outcomes = Vec::new();
        for row in rows {
            let (id, payload) = row?;
            let Ok(run) = Uuid::parse_str(&id) else {
                continue;
            };
            let Some(event) = payload.and_then(|p| serde_json::from_str::<AppEvent>(&p).ok())
            else {
                continue;
            };
            let AppEvent::RunStatusChanged { run_id, lifecycle } = event else {
                continue;
            };
            if run_id != run {
                continue;
            }
            let Some(state) = FinalRunState::from_lifecycle(lifecycle) else {
                continue;
            };
            let error:Option<String>=db.query_row("SELECT CASE WHEN length(CAST(payload AS BLOB))<=8192 THEN payload ELSE NULL END FROM event_log WHERE project_id=?1 AND session_id=?2 AND run_id=?3 AND event_type='run_error' ORDER BY id DESC LIMIT 1",params![project.to_string(),session.to_string(),run.to_string()],|r|r.get(0)).optional()?.flatten();
            let reason = error
                .and_then(|e| serde_json::from_str::<AppEvent>(&e).ok())
                .and_then(|e| match e {
                    AppEvent::RunError {
                        run_id, message, ..
                    } if run_id == run => Some(if message.contains("repeated_tool_rejection") {
                        "repeated_tool_rejection"
                    } else if message.contains("tool loop exceeded") {
                        "tool_loop_limit"
                    } else if message.contains("context_compaction_failed") {
                        "context_compaction_failed"
                    } else if message.contains("instruction_loading_failed") {
                        "instruction_loading_failed"
                    } else if message.contains("response truncated") {
                        "provider_response_truncated"
                    } else if message.contains("(os error 2)") || message.contains("(os error 3)") {
                        "file_not_found"
                    } else if message.contains("(os error 5)")
                        || message.contains("Permission denied")
                    {
                        "access_denied"
                    } else {
                        "runtime_failure"
                    }),
                    _ => None,
                });
            let recorded_file_changes=db.query_row("SELECT COUNT(*) FROM file_changes WHERE project_id=?1 AND session_id=?2 AND run_id=?3",params![project.to_string(),session.to_string(),run.to_string()],|r|r.get(0))?;
            outcomes.push(RunOutcome {
                run_id: run,
                state,
                reason,
                recorded_file_changes,
            });
        }
        Ok(outcomes)
    }
}
