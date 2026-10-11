//! Optional, explicitly reviewed extension components. Construction is inert.
//! Installation is not execution approval. External code is never host policy.
pub mod configuration;
pub mod connection;
pub mod http;
pub mod lifecycle;
pub mod package;
pub mod wire;

pub const SDK_VERSION: &str = "rmcp/3.5.1";
pub const MODERN_PROTOCOL: &str = "2026-07-28";
pub const LEGACY_PROTOCOL: &str = "2025-11-25";
pub const MAX_MESSAGE_BYTES: usize = 256 * 1024;
pub const MAX_RESULT_BYTES: usize = 64 * 1024;
pub const MAX_CATALOG_ENTRIES: usize = 128;
pub fn open_image_lease(
    path: &std::path::Path,
) -> Result<argentum_execution::VerifiedFileLease, std::io::Error> {
    argentum_execution::VerifiedFileLease::open(path, 128 * 1024)
}

pub fn default_process_limits(
) -> Result<argentum_execution::ManagedProcessLimits, argentum_execution::ProcessError> {
    // Chromium is an explicitly reviewed multi-process optional component.
    argentum_execution::ManagedProcessLimits::new(
        16,
        95,
        1024 * 1024,
        2 * 1024 * 1024 * 1024,
        4 * 1024 * 1024 * 1024,
    )
}
