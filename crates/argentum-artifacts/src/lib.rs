//! Optional local document artifacts. No account connection, automatic process,
//! generic shell, network fetch, or model-authored verification authority.

pub mod artifact;
pub mod layout;
pub mod markdown;
pub mod server;
pub mod verify;

use sha2::{Digest, Sha256};

pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[derive(Debug, thiserror::Error)]
pub enum ArtifactError {
    #[error("artifact contract: {0}")]
    Contract(&'static str),
    #[error("artifact input, output or private worker is unavailable")]
    Io,
    #[error("artifact operation was cancelled and owned work drained")]
    Cancelled,
}
pub type Result<T> = std::result::Result<T, ArtifactError>;
