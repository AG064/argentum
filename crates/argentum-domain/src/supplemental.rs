//! Model-authored checks are executable inputs and separate from project-suite evidence.
use crate::{ProjectId, RunId, SessionId, Sha256Digest};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub const RUST_CHECK_MAX_SOURCE_BYTES: usize = 32 * 1024;
pub const RUST_CHECK_MAX_BATCHES: usize = 6;
pub const RUST_CHECK_MAX_SECONDS: u64 = 60;

#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct RustCheckSource(String);
impl RustCheckSource {
    pub fn new(value: String) -> Result<Self, &'static str> {
        if value.is_empty() || value.len() > RUST_CHECK_MAX_SOURCE_BYTES || value.contains('\0') {
            return Err("Rust check source must contain 1 through 32768 UTF-8 bytes without NUL");
        }
        Ok(Self(value))
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}
impl std::fmt::Debug for RustCheckSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RustCheckSource")
            .field("bytes", &self.0.len())
            .finish()
    }
}
impl<'de> Deserialize<'de> for RustCheckSource {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::new(String::deserialize(d)?).map_err(serde::de::Error::custom)
    }
}
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RustCheckProfile {
    #[default]
    Debug,
    Release,
}
impl RustCheckProfile {
    pub fn label(self) -> &'static str {
        match self {
            Self::Debug => "debug",
            Self::Release => "release",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupplementalCheckState {
    ReportedPass,
    Failed,
    CompileFailed,
    ZeroTests,
    OutputIncomplete,
    Stale,
    Cancelled,
    TimedOut,
    Unavailable,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupplementalCheckReceipt {
    pub schema_version: u16,
    pub id: Uuid,
    pub project_id: ProjectId,
    pub session_id: SessionId,
    pub run_id: RunId,
    pub owner_binding_sha256: Sha256Digest,
    pub definition_sha256: Sha256Digest,
    pub package: String,
    pub library_target: String,
    pub target: String,
    pub profile: RustCheckProfile,
    pub source_revision: Sha256Digest,
    pub revision_after: Sha256Digest,
    pub source_fresh: bool,
    pub snapshot_sha256: Sha256Digest,
    pub snapshot_file_count: u64,
    pub snapshot_bytes: u64,
    pub manifest_sha256: Sha256Digest,
    pub derived_manifest_sha256: Sha256Digest,
    pub lockfile_sha256: Sha256Digest,
    pub test_sha256: Sha256Digest,
    pub toolchain_sha256: Sha256Digest,
    pub toolchain_selection_sha256: Sha256Digest,
    pub test_executable_sha256: Option<Sha256Digest>,
    pub state: SupplementalCheckState,
    pub compile_exit_code: Option<i32>,
    pub test_exit_code: Option<i32>,
    pub reported_test_count: Option<u32>,
    pub duration_ms: u64,
    pub stdout_bytes: u64,
    pub stderr_bytes: u64,
    pub stdout_sha256: Sha256Digest,
    pub stderr_sha256: Sha256Digest,
    pub output_complete: bool,
    pub output_truncated: bool,
    pub boundary: String,
    pub reason_code: Option<String>,
    pub storage_peak_bytes: u64,
    pub cleanup_complete: bool,
    pub replacements: Option<Uuid>,
    pub assertions_authored_by: String,
    pub independently_verified: bool,
}
