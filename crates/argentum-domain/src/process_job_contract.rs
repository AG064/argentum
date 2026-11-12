use serde::{Deserialize, Serialize};

pub const MAX_PROCESS_JOB_ID_BYTES: usize = 64;
pub const MAX_POLL_CURSOR: u64 = 16_777_216;
pub const POLL_WAIT_MS: u64 = 30_000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PollSelection {
    pub job_id: String,
    pub stdout_cursor: u64,
    pub stderr_cursor: u64,
    pub wait_ms: u64,
}
