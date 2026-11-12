//! Bounded display pages, separate from model-context selection.
use super::*;

pub const HISTORY_PAGE_MESSAGES: usize = 32;
pub const HISTORY_PAGE_BYTES: usize = 128 * 1024;
pub const HISTORY_MESSAGE_CHARS: usize = 32 * 1024;

#[derive(Debug, Clone)]
pub struct HistoryPage {
    pub project_id: ProjectId,
    pub session_id: SessionId,
    pub messages: Vec<HistoryMessage>,
    pub has_older: bool,
    pub has_newer: bool,
}

#[derive(Debug, Clone)]
pub struct HistoryMessage {
    pub id: argentum_domain::MessageId,
    pub run_id: RunId,
    pub role: ConversationRole,
    pub status: ConversationMessageStatus,
    pub text: String,
    pub truncated: bool,
}

impl Store {
    pub fn history_page(
        &self,
        project: ProjectId,
        session: SessionId,
        cursor: Option<argentum_domain::MessageId>,
        newer: bool,
    ) -> Result<HistoryPage, StoreError> {
        let connection = self
            .connection
            .lock()
            .map_err(|_| StoreError::LockPoisoned)?;
        if !connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM sessions WHERE id=?1 AND project_id=?2)",
            params![session.to_string(), project.to_string()],
            |r| r.get::<_, bool>(0),
        )? {
            return Err(StoreError::InvalidRecord(
                "history session is outside this project".into(),
            ));
        }
        let boundary: Option<(i64, i64)> = if let Some(id) = cursor {
            Some(connection.query_row("SELECT created_at, rowid FROM conversation_messages WHERE id=?1 AND project_id=?2 AND session_id=?3", params![id.to_string(), project.to_string(), session.to_string()], |r| Ok((r.get(0)?, r.get(1)?)))?)
        } else {
            None
        };
        let (time, row) = boundary.unwrap_or((i64::MAX, i64::MAX));
        let newer = newer && boundary.is_some();
        let comparator = if newer { ">" } else { "<" };
        let order = if newer { "ASC" } else { "DESC" };
        let sql = format!("SELECT id,run_id,role,status,substr(text,1,?5),length(text)>?5,created_at,rowid FROM conversation_messages WHERE project_id=?1 AND session_id=?2 AND (created_at,rowid) {comparator} (?3,?4) ORDER BY created_at {order},rowid {order} LIMIT ?6");
        let mut query = connection.prepare(&sql)?;
        let mut rows = query.query(params![
            project.to_string(),
            session.to_string(),
            time,
            row,
            HISTORY_MESSAGE_CHARS as i64,
            HISTORY_PAGE_MESSAGES as i64
        ])?;
        let mut messages = Vec::new();
        let mut positions = Vec::new();
        let mut bytes = 0usize;
        while let Some(row) = rows.next()? {
            let text: String = row.get(4)?;
            if bytes + text.len() > HISTORY_PAGE_BYTES {
                break;
            }
            bytes += text.len();
            messages.push(HistoryMessage {
                id: row.get::<_, String>(0)?.parse().map_err(|_| {
                    StoreError::InvalidRecord("history message ID is invalid".into())
                })?,
                run_id: row
                    .get::<_, String>(1)?
                    .parse()
                    .map_err(|_| StoreError::InvalidRecord("history run ID is invalid".into()))?,
                role: parse_conversation_role(&row.get::<_, String>(2)?)?,
                status: parse_conversation_status(&row.get::<_, String>(3)?)?,
                text,
                truncated: row.get(5)?,
            });
            positions.push((row.get::<_, i64>(6)?, row.get::<_, i64>(7)?));
        }
        if !newer {
            messages.reverse();
            positions.reverse();
        }
        let has = |position: Option<&(i64, i64)>, comparison: &str| -> Result<bool, StoreError> {
            let Some((time, row)) = position else {
                return Ok(false);
            };
            Ok(connection.query_row(&format!("SELECT EXISTS(SELECT 1 FROM conversation_messages WHERE project_id=?1 AND session_id=?2 AND (created_at,rowid) {comparison} (?3,?4))"), params![project.to_string(),session.to_string(),time,row], |r| r.get(0))?)
        };
        Ok(HistoryPage {
            project_id: project,
            session_id: session,
            has_older: has(positions.first(), "<")?,
            has_newer: has(positions.last(), ">")?,
            messages,
        })
    }
}
