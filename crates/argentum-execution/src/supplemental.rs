//! Optional bounded Rust-library checks, always isolated on Windows.
use crate::ProcessJobOwner;
use argentum_domain::{supplemental::*, Sha256Digest};
use argentum_workspaces::WorkspaceManager;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use thiserror::Error;
use tokio_util::sync::CancellationToken;

pub const CHECK_TOOLCHAIN_BYTES: u64 = 1024 * 1024 * 1024;
pub const CHECK_SCRATCH_BYTES: u64 = 256 * 1024 * 1024;
pub const CHECK_SNAPSHOT_BYTES: u64 = 16 * 1024 * 1024;
const CHECK_TARGET: &str = "argentum_public_checks";
const RAW_OUTPUT_BYTES: usize = 64 * 1024;
const MODEL_DIAGNOSTIC_BYTES: usize = 4 * 1024;

pub fn rust_check_configuration_digest(root: &Path) -> Result<Sha256Digest, RustCheckError> {
    #[cfg(windows)]
    {
        windows::configuration_supported(root)?;
        Ok(digest(
            windows::selected_toolchain()?.to_string_lossy().as_bytes(),
        ))
    }
    #[cfg(not(windows))]
    {
        let _ = root;
        Err(RustCheckError::Unavailable("Windows LPAC is required"))
    }
}

#[derive(Debug, Error)]
pub enum RustCheckError {
    #[error("supplemental Rust checking is unavailable: {0}")]
    Unavailable(&'static str),
    #[error("supplemental Rust check input is invalid: {0}")]
    Input(&'static str),
    #[error("supplemental Rust check resources are unavailable")]
    Resource,
    #[error(transparent)]
    Workspace(#[from] argentum_workspaces::WorkspaceError),
    #[error(transparent)]
    Process(#[from] crate::ProcessError),
    #[error("supplemental Rust check owned I/O failed")]
    Io(#[from] std::io::Error),
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use argentum_security::{ApprovalPolicy, CapabilityBroker};
    #[tokio::test]
    async fn actual_source_compiles_and_assertion_failure_is_returned() {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("src")).unwrap();
        std::fs::write(
            root.path().join("Cargo.toml"),
            "[package]\nname='probe_fixture'\nversion='0.1.0'\nedition='2021'\n",
        )
        .unwrap();
        std::fs::write(
            root.path().join("Cargo.lock"),
            "version = 4\n\n[[package]]\nname = \"probe_fixture\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(root.path().join("src/lib.rs"), "pub fn value()->u32{7}\n").unwrap();
        let workspace = WorkspaceManager::new(
            CapabilityBroker::new(root.path(), ApprovalPolicy::default()).unwrap(),
        );
        let owner = ProcessJobOwner {
            host_generation: uuid::Uuid::new_v4(),
            project_id: uuid::Uuid::new_v4(),
            session_id: uuid::Uuid::new_v4(),
            run_id: uuid::Uuid::new_v4(),
            workspace_key: root
                .path()
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
                .to_ascii_lowercase(),
        };
        let source = RustCheckSource::new(
            "#[test] fn public_value(){assert_eq!(probe_fixture::value(),8);}".into(),
        )
        .unwrap();
        let before = std::fs::read(root.path().join("src/lib.rs")).unwrap();
        let output = run_rust_check(
            workspace,
            owner,
            RustCheckRequest {
                package: "probe_fixture".into(),
                profile: RustCheckProfile::Debug,
                source,
                replaces: None,
            },
            CancellationToken::new(),
            Duration::from_secs(60),
        )
        .await
        .unwrap();
        assert_eq!(
            output.receipt.state,
            SupplementalCheckState::Failed,
            "{:?}: {}",
            output.receipt,
            output.stderr
        );
        assert!(output.stdout.contains("public_value"));
        assert_eq!(
            std::fs::read(root.path().join("src/lib.rs")).unwrap(),
            before
        );
    }

    fn fixture() -> (tempfile::TempDir, WorkspaceManager, ProcessJobOwner) {
        let root = tempfile::tempdir().unwrap();
        std::fs::create_dir(root.path().join("src")).unwrap();
        std::fs::write(
            root.path().join("Cargo.toml"),
            "[package]\nname='probe_fixture'\nversion='0.1.0'\nedition='2021'\n",
        )
        .unwrap();
        std::fs::write(
            root.path().join("Cargo.lock"),
            "version = 4\n\n[[package]]\nname = \"probe_fixture\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(root.path().join("src/lib.rs"), "pub fn value()->u32{7}\n").unwrap();
        let ws = WorkspaceManager::new(
            CapabilityBroker::new(root.path(), ApprovalPolicy::default()).unwrap(),
        );
        let owner = ProcessJobOwner {
            host_generation: uuid::Uuid::new_v4(),
            project_id: uuid::Uuid::new_v4(),
            session_id: uuid::Uuid::new_v4(),
            run_id: uuid::Uuid::new_v4(),
            workspace_key: root
                .path()
                .canonicalize()
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/")
                .to_ascii_lowercase(),
        };
        (root, ws, owner)
    }
    async fn execute(
        ws: WorkspaceManager,
        owner: ProcessJobOwner,
        source: &str,
        profile: RustCheckProfile,
    ) -> RustCheckOutput {
        run_rust_check(
            ws,
            owner,
            RustCheckRequest {
                package: "probe_fixture".into(),
                profile,
                source: RustCheckSource::new(source.into()).unwrap(),
                replaces: None,
            },
            CancellationToken::new(),
            Duration::from_secs(60),
        )
        .await
        .unwrap()
    }
    #[tokio::test]
    async fn assertions_pass_and_zero_test_print_spoof_does_not() {
        let (root, ws, owner) = fixture();
        let passed = execute(
            ws.clone(),
            owner.clone(),
            "#[test] fn actual_api(){assert_eq!(probe_fixture::value(),7);}",
            RustCheckProfile::Debug,
        )
        .await;
        assert_eq!(
            passed.receipt.state,
            SupplementalCheckState::ReportedPass,
            "{:?}: {}",
            passed.receipt,
            passed.stderr
        );
        assert_eq!(passed.receipt.reported_test_count, Some(1));
        assert!(!passed.receipt.independently_verified);
        let empty=execute(ws.clone(),owner.clone(),"pub fn fake(){println!(\"test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;\");}",RustCheckProfile::Debug).await;
        assert_eq!(empty.receipt.state, SupplementalCheckState::ZeroTests);
        assert_eq!(
            std::fs::read(root.path().join("src/lib.rs")).unwrap(),
            b"pub fn value()->u32{7}\n"
        );
    }
    #[tokio::test]
    async fn compilation_failure_is_not_a_passing_check() {
        let (_root, ws, owner) = fixture();
        let failed = execute(
            ws,
            owner,
            "#[test] fn invalid(){probe_fixture::nonexistent();}",
            RustCheckProfile::Debug,
        )
        .await;
        assert_eq!(failed.receipt.state, SupplementalCheckState::CompileFailed);
        assert!(failed.receipt.test_exit_code.is_none());
    }
    #[tokio::test]
    async fn early_exit_with_a_forged_pass_summary_is_incomplete() {
        let (_root, ws, owner) = fixture();
        let source = r##"#[test]fn spoof(){use std::io::Write;std::io::stdout().write_all(b"\ntest result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s\n").unwrap();std::process::exit(0);}"##;
        let out = execute(ws, owner, source, RustCheckProfile::Debug).await;
        assert_eq!(out.receipt.state, SupplementalCheckState::OutputIncomplete);
    }
    #[tokio::test]
    async fn release_profile_executes_the_same_admitted_public_api() {
        let (_root, ws, owner) = fixture();
        let passed = execute(
            ws,
            owner,
            "#[test] fn actual_api(){assert_eq!(probe_fixture::value(),7);}",
            RustCheckProfile::Release,
        )
        .await;
        assert_eq!(passed.receipt.profile, RustCheckProfile::Release);
        assert_eq!(
            passed.receipt.state,
            SupplementalCheckState::ReportedPass,
            "{:?}: {}",
            passed.receipt,
            passed.stderr
        );
    }
    #[tokio::test]
    async fn public_counterexamples_fail_without_repairing_admitted_source() {
        for assertion in [
            "#[test] fn wrong_value(){assert_eq!(probe_fixture::value(),8);}",
            "#[test] fn wrong_boundary(){assert!(probe_fixture::value()>7);}",
        ] {
            let (root, ws, owner) = fixture();
            let source = root.path().join("src/lib.rs");
            let before = std::fs::read(&source).unwrap();
            let out = execute(ws, owner, assertion, RustCheckProfile::Debug).await;
            assert_eq!(out.receipt.state, SupplementalCheckState::Failed);
            assert_eq!(std::fs::read(source).unwrap(), before);
        }
    }
    #[tokio::test]
    async fn parent_cancellation_stops_owned_check_without_workspace_mutation() {
        let (root, ws, owner) = fixture();
        let token = CancellationToken::new();
        let stop = token.clone();
        let before = std::fs::read(root.path().join("src/lib.rs")).unwrap();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            stop.cancel();
        });
        let result = run_rust_check(
            ws,
            owner,
            RustCheckRequest {
                package: "probe_fixture".into(),
                profile: RustCheckProfile::Debug,
                source: RustCheckSource::new(
                    "#[test] fn forever(){loop{std::hint::spin_loop();}}".into(),
                )
                .unwrap(),
                replaces: None,
            },
            token.clone(),
            Duration::from_secs(60),
        )
        .await;
        assert!(token.is_cancelled());
        assert!(
            result.is_err()
                || result.is_ok_and(|out| out.receipt.state == SupplementalCheckState::Cancelled)
        );
        assert_eq!(
            std::fs::read(root.path().join("src/lib.rs")).unwrap(),
            before
        );
    }
    #[tokio::test]
    async fn preparation_deadline_and_foreign_owner_fail_closed() {
        let (_root, ws, mut owner) = fixture();
        let manager = crate::ProcessJobManager::new();
        let request = || RustCheckRequest {
            package: "probe_fixture".into(),
            profile: RustCheckProfile::Debug,
            source: RustCheckSource::new(
                "#[test] fn small(){assert_eq!(probe_fixture::value(),7);}".into(),
            )
            .unwrap(),
            replaces: None,
        };
        assert!(matches!(
            manager
                .run_rust_check(
                    owner.clone(),
                    ws.clone(),
                    request(),
                    CancellationToken::new(),
                    Duration::from_secs(60)
                )
                .await,
            Err(crate::ProcessJobError::NotFound)
        ));
        owner.host_generation = manager.host_generation();
        assert!(manager
            .run_rust_check(
                owner,
                ws,
                request(),
                CancellationToken::new(),
                Duration::from_millis(1)
            )
            .await
            .is_err());
    }
    #[tokio::test]
    async fn source_change_makes_a_passing_snapshot_historical() {
        let (root, ws, owner) = fixture();
        let source = root.path().join("src/lib.rs");
        let changed = source.clone();
        let change = Box::new(move || {
            std::fs::write(&changed, "pub fn value()->u32{9}\n").unwrap();
        });
        let out = AFTER_SNAPSHOT_ADMISSION
            .scope(
                change,
                execute(
                    ws,
                    owner,
                    "#[test]fn snapshot_value(){assert_eq!(probe_fixture::value(),7);}",
                    RustCheckProfile::Debug,
                ),
            )
            .await;
        assert_eq!(out.receipt.state, SupplementalCheckState::Stale);
        assert!(!out.receipt.source_fresh);
        assert_eq!(std::fs::read(source).unwrap(), b"pub fn value()->u32{9}\n");
    }
    #[test]
    fn inherited_configuration_and_size_limits_fail_closed() {
        for name in [
            "RUSTFLAGS",
            "CARGO_ENCODED_RUSTFLAGS",
            "CARGO_PROFILE_DEV_OVERFLOW_CHECKS",
            "RUSTC_WORKSPACE_WRAPPER",
            "CARGO_BUILD_TARGET",
            "CARGO_TARGET_X86_64_PC_WINDOWS_MSVC_RUSTFLAGS",
        ] {
            assert!(
                windows::environment_supported([(name.into(), "synthetic override".into())])
                    .is_err()
            );
        }
        assert!(windows::environment_supported([(
            "CARGO_TARGET_DIR".into(),
            "owned-build".into()
        )])
        .is_ok());
        assert!(RustCheckSource::new("x".repeat(RUST_CHECK_MAX_SOURCE_BYTES + 1)).is_err());
        assert!(RustCheckSource::new("nul\0input".into()).is_err());
        let root = tempfile::tempdir().unwrap();
        let child = root.path().join("child");
        std::fs::create_dir(&child).unwrap();
        std::fs::create_dir(root.path().join(".cargo")).unwrap();
        std::fs::write(
            root.path().join(".cargo/config.toml"),
            "[build]\ntarget='aarch64-pc-windows-msvc'\n",
        )
        .unwrap();
        assert!(windows::configuration_supported(&child).is_err());
    }
    #[test]
    fn mutable_scratch_inventory_tolerates_disappearing_children_not_missing_roots() {
        let root = tempfile::tempdir().unwrap();
        let volatile = root.path().join("compiler-tmp");
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let worker_stop = stop.clone();
        let writer = std::thread::spawn(move || {
            while !worker_stop.load(std::sync::atomic::Ordering::Relaxed) {
                std::fs::write(&volatile, b"owned compiler temporary").unwrap();
                let _ = std::fs::remove_file(&volatile);
            }
        });
        let observations = (0..2000)
            .map(|_| windows::tree_bytes(root.path()))
            .collect::<Vec<_>>();
        stop.store(true, std::sync::atomic::Ordering::Relaxed);
        writer.join().unwrap();
        assert!(
            observations.iter().all(Result::is_ok),
            "scratch observer failures: {:?}",
            observations
                .iter()
                .filter_map(|result| result.as_ref().err())
                .take(4)
                .collect::<Vec<_>>()
        );
        assert!(windows::tree_bytes(&root.path().join("missing-root")).is_err());
    }
    #[tokio::test]
    async fn truncated_direct_output_is_not_a_passing_check() {
        let (_root, ws, owner) = fixture();
        let out=execute(ws,owner,"#[test]fn output(){use std::io::Write;std::io::stdout().write_all(&vec![b'x';100000]).unwrap();assert_eq!(probe_fixture::value(),7);}",RustCheckProfile::Debug).await;
        assert_eq!(out.receipt.state, SupplementalCheckState::OutputIncomplete);
        assert!(out.receipt.output_truncated);
        assert!(out.stdout_prefix_truncated);
        assert!(out.stdout.len() <= MODEL_DIAGNOSTIC_BYTES);
        assert!(out.receipt.cleanup_complete);
    }
    #[tokio::test]
    async fn infinite_test_times_out_and_owned_work_settles() {
        let (_root, ws, owner) = fixture();
        let started = Instant::now();
        let out = execute(
            ws,
            owner,
            "#[test]fn never_finishes(){loop{std::hint::spin_loop();}}",
            RustCheckProfile::Debug,
        )
        .await;
        assert_eq!(out.receipt.state, SupplementalCheckState::TimedOut);
        assert!(out.receipt.cleanup_complete);
        assert!(started.elapsed() < Duration::from_secs(70));
    }
    #[tokio::test]
    async fn actual_compiler_and_test_cannot_access_private_inputs() {
        let (root, ws, owner) = fixture();
        std::fs::create_dir(root.path().join(".git")).unwrap();
        let index = root.path().join(".git/index");
        std::fs::write(&index, b"SYNTHETIC_PROTECTED_INDEX").unwrap();
        let outside = tempfile::tempdir().unwrap();
        let marker = outside.path().join("owner-store-marker");
        std::fs::write(&marker, b"SYNTHETIC_SECRET_NOT_A_CREDENTIAL").unwrap();
        let test=format!("#[test]fn private_access(){{assert!(std::fs::read({:?}).is_err());assert!(std::fs::write({:?},b\"bad\").is_err());assert!(std::env::var(\"DEEPSEEK_API_KEY\").is_err());assert!(std::env::var(\"MINIMAX_API_KEY\").is_err());assert!(std::fs::write(\"src/lib.rs\",b\"bad\").is_err());assert!(std::fs::hard_link({:?},\"private-link\").is_err());assert!(std::fs::write({:?},b\"bad\").is_err());assert_eq!(probe_fixture::value(),7);}}",marker,marker,marker,index);
        let out = execute(ws.clone(), owner.clone(), &test, RustCheckProfile::Debug).await;
        assert_eq!(
            out.receipt.state,
            SupplementalCheckState::ReportedPass,
            "{}",
            out.stderr
        );
        let compile_input=format!("const PRIVATE:&[u8]=include_bytes!({:?});#[test]fn compile_read(){{assert!(!PRIVATE.is_empty());}}",marker);
        let out = execute(ws, owner, &compile_input, RustCheckProfile::Debug).await;
        assert_eq!(out.receipt.state, SupplementalCheckState::CompileFailed);
        assert_eq!(
            std::fs::read(marker).unwrap(),
            b"SYNTHETIC_SECRET_NOT_A_CREDENTIAL"
        );
        assert_eq!(
            std::fs::read(root.path().join("src/lib.rs")).unwrap(),
            b"pub fn value()->u32{7}\n"
        );
        assert_eq!(std::fs::read(index).unwrap(), b"SYNTHETIC_PROTECTED_INDEX");
    }
    #[tokio::test]
    async fn cargo_redirect_added_after_admission_makes_the_result_historical() {
        let (root, ws, owner) = fixture();
        let config = root.path().join(".cargo");
        let change = Box::new(move || {
            std::fs::create_dir(&config).unwrap();
            std::fs::write(
                config.join("config.toml"),
                "[build]\ntarget='aarch64-pc-windows-msvc'\n",
            )
            .unwrap();
        });
        let out = AFTER_SNAPSHOT_ADMISSION
            .scope(
                change,
                execute(
                    ws,
                    owner,
                    "#[test]fn admitted_value(){assert_eq!(probe_fixture::value(),7);}",
                    RustCheckProfile::Debug,
                ),
            )
            .await;
        assert_eq!(out.receipt.state, SupplementalCheckState::Stale);
        assert!(!out.receipt.source_fresh);
    }
    #[tokio::test]
    async fn synthetic_correct_reference_passes_through_the_same_boundary() {
        for profile in [RustCheckProfile::Debug, RustCheckProfile::Release] {
            let (root, ws, owner) = fixture();
            let before = std::fs::read(root.path().join("src/lib.rs")).unwrap();
            let out = execute(
                ws,
                owner,
                "#[test] fn public_boundary(){assert_eq!(probe_fixture::value(),7);}",
                profile,
            )
            .await;
            assert_eq!(
                out.receipt.state,
                SupplementalCheckState::ReportedPass,
                "{}",
                out.stderr
            );
            assert_eq!(
                std::fs::read(root.path().join("src/lib.rs")).unwrap(),
                before
            );
        }
    }
}
pub struct RustCheckRequest {
    pub package: String,
    pub profile: RustCheckProfile,
    pub source: RustCheckSource,
    pub replaces: Option<uuid::Uuid>,
}
pub struct RustCheckOutput {
    pub receipt: SupplementalCheckReceipt,
    pub stdout: String,
    pub stderr: String,
    pub stdout_prefix_truncated: bool,
    pub stderr_prefix_truncated: bool,
}
impl std::fmt::Debug for RustCheckOutput {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RustCheckOutput")
            .field("receipt", &self.receipt)
            .finish()
    }
}
fn digest(bytes: &[u8]) -> Sha256Digest {
    Sha256Digest(format!("{:x}", Sha256::digest(bytes)))
}

pub async fn run_rust_check(
    workspace: WorkspaceManager,
    owner: ProcessJobOwner,
    request: RustCheckRequest,
    cancellation: CancellationToken,
    maximum: Duration,
) -> Result<RustCheckOutput, RustCheckError> {
    #[cfg(not(windows))]
    {
        let _ = (workspace, owner, request, cancellation, maximum);
        Err(RustCheckError::Unavailable(
            "requires the qualified Windows LPAC execution boundary",
        ))
    }
    #[cfg(windows)]
    {
        windows::run(workspace, owner, request, cancellation, maximum).await
    }
}

// Test-only, task-local synchronization for concurrent-editor controls. It is
// absent from release builds and cannot be selected by a check request.
#[cfg(test)]
tokio::task_local! {
    static AFTER_SNAPSHOT_ADMISSION: Box<dyn Fn() + Send + Sync>;
}

#[cfg(windows)]
mod windows {
    use super::*;
    use crate::probe_sandbox::ProbeSandbox;
    use crate::{ManagedProcess, ManagedProcessLimits, ProcessSpec};
    use std::fs::OpenOptions;
    use std::io::{Read, Write};
    use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
    use std::sync::{Arc, Mutex};
    use windows_sys::Win32::Storage::FileSystem::{FILE_ATTRIBUTE_REPARSE_POINT, FILE_SHARE_READ};

    #[derive(Serialize)]
    struct FileIdentity {
        path: String,
        bytes: u64,
        sha256: Sha256Digest,
    }
    struct Area {
        root: PathBuf,
        keep: PathBuf,
    }
    impl Area {
        fn new() -> Result<Self, RustCheckError> {
            let root = std::env::temp_dir().join(format!(
                "argentum-rust-check-{}",
                uuid::Uuid::new_v4().simple()
            ));
            std::fs::create_dir(&root)?;
            crate::ensure_owner_only_directory(&root)?;
            // Preserve the validated physical path in every descendant launch
            // binding and environment value, including Windows short temp aliases.
            let root = root.canonicalize()?;
            let keep = root.join("evidence");
            std::fs::create_dir(&keep)?;
            Ok(Self { root, keep })
        }
        fn clean_generated(&self) -> Result<(), RustCheckError> {
            for name in ["tools", "build", "cargo-home", "temp"] {
                let target = self.root.join(name);
                if target.parent() != Some(self.root.as_path()) {
                    return Err(RustCheckError::Resource);
                }
                if target.exists() {
                    std::fs::remove_dir_all(target)?;
                }
            }
            // Retained source/test/metadata stops being accessible to a retired package identity.
            let mut paths = vec![self.keep.clone()];
            while let Some(path) = paths.pop() {
                let metadata = std::fs::symlink_metadata(&path)?;
                if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                    return Err(RustCheckError::Resource);
                }
                if metadata.is_dir() {
                    crate::ensure_owner_only_directory(&path)?;
                    for entry in std::fs::read_dir(path)? {
                        paths.push(entry?.path());
                    }
                } else {
                    crate::ensure_owner_only_file(path)?;
                }
            }
            crate::ensure_owner_only_directory(&self.root)?;
            Ok(())
        }
    }
    impl Drop for Area {
        fn drop(&mut self) {
            // Only generated subtrees of this create-new random area, never the project/source.
            // Evidence (bounded inputs, hashes and receipt) is retained owner-only.
            let _ = self.clean_generated();
        }
    }
    fn bounded_file(path: &Path, maximum: u64) -> Result<Vec<u8>, RustCheckError> {
        let metadata = std::fs::symlink_metadata(path)?;
        if !metadata.is_file()
            || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
            || metadata.len() > maximum
        {
            return Err(RustCheckError::Unavailable(
                "input is not a bounded regular file",
            ));
        }
        let mut file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(path)?;
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(maximum + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > maximum {
            return Err(RustCheckError::Resource);
        }
        Ok(bytes)
    }
    fn create(path: &Path, bytes: &[u8]) -> Result<(), RustCheckError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        file.write_all(bytes)?;
        Ok(())
    }
    pub(super) fn tree_bytes(root: &Path) -> Result<u64, RustCheckError> {
        let metadata = std::fs::symlink_metadata(root)?;
        if !metadata.is_dir() || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
            return Err(RustCheckError::Unavailable(
                "owned scratch root is not a regular directory",
            ));
        }
        let mut stack = vec![root.to_path_buf()];
        let mut count = 1;
        let mut bytes = 0u64;
        while let Some(path) = stack.pop() {
            let entries = match std::fs::read_dir(&path) {
                Ok(entries) => entries,
                Err(error) if path != root && error.kind() == std::io::ErrorKind::NotFound => {
                    continue
                }
                Err(error) => return Err(error.into()),
            };
            for entry in entries {
                let entry = entry?;
                count += 1;
                if count > 4096 {
                    return Err(RustCheckError::Resource);
                }
                // Windows DirEntry metadata is captured by directory enumeration without
                // reopening a compiler file that may already be delete-pending (Win32 error 5).
                // It does not follow links. This is sampled accounting, not an atomic disk quota.
                let metadata = entry.metadata()?;
                if metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
                    return Err(RustCheckError::Unavailable(
                        "owned scratch inventory contains a reparse point",
                    ));
                }
                if metadata.is_file() {
                    bytes = bytes
                        .checked_add(metadata.len())
                        .ok_or(RustCheckError::Resource)?;
                } else if metadata.is_dir() {
                    stack.push(entry.path());
                } else {
                    return Err(RustCheckError::Resource);
                }
            }
        }
        Ok(bytes)
    }
    fn parse(bytes: &[u8]) -> Result<toml::Value, RustCheckError> {
        let text = std::str::from_utf8(bytes)
            .map_err(|_| RustCheckError::Input("manifest is not UTF-8"))?;
        toml::from_str::<toml::Table>(text)
            .map(toml::Value::Table)
            .map_err(|_| RustCheckError::Input("invalid Cargo manifest"))
    }
    fn manifest(
        workspace: &WorkspaceManager,
        request: &RustCheckRequest,
    ) -> Result<(Vec<u8>, String), RustCheckError> {
        configuration_supported(workspace.root())?;
        let capture = workspace.capture_file(Path::new("Cargo.toml"), 128 * 1024)?;
        let bytes = capture
            .bytes
            .ok_or(RustCheckError::Input("Cargo.toml is unavailable"))?;
        let value = parse(&bytes)?;
        if value
            .get("package")
            .and_then(|x| x.get("name"))
            .and_then(toml::Value::as_str)
            != Some(request.package.as_str())
        {
            return Err(RustCheckError::Input(
                "select the exact root Cargo package name",
            ));
        }
        for key in [
            "dependencies",
            "dev-dependencies",
            "build-dependencies",
            "target",
            "patch",
            "replace",
            "profile",
        ] {
            if value
                .get(key)
                .is_some_and(|x| x.as_table().is_none_or(|t| !t.is_empty()))
            {
                return Err(RustCheckError::Unavailable("initial library slice does not admit dependencies, target overrides or custom profiles"));
            }
        }
        if value
            .get("workspace")
            .is_some_and(|x| x.as_table().is_none_or(|t| !t.is_empty()))
            || value["package"].get("workspace").is_some()
        {
            return Err(RustCheckError::Unavailable(
                "initial library slice requires a standalone package",
            ));
        }
        if value["package"]
            .get("build")
            .is_some_and(|x| x.as_bool() != Some(false))
            || workspace.root().join("build.rs").exists()
        {
            return Err(RustCheckError::Unavailable(
                "build scripts are not admitted in the initial library slice",
            ));
        }
        if workspace.root().join(".cargo").exists()
            || workspace.root().join("rust-toolchain").exists()
            || workspace.root().join("rust-toolchain.toml").exists()
        {
            return Err(RustCheckError::Unavailable(
                "project toolchain and Cargo redirects require a separately supported definition",
            ));
        }
        let lib = value.get("lib");
        if lib.is_some_and(|x| x.get("proc-macro").and_then(toml::Value::as_bool) == Some(true)) {
            return Err(RustCheckError::Unavailable(
                "procedural macros are not admitted",
            ));
        }
        let path = lib
            .and_then(|x| x.get("path"))
            .and_then(toml::Value::as_str)
            .unwrap_or("src/lib.rs");
        if !path.starts_with("src/")
            || path.contains("..")
            || !path.ends_with(".rs")
            || !workspace.root().join(path).is_file()
        {
            return Err(RustCheckError::Unavailable(
                "requires a contained Rust library entry point under src",
            ));
        }
        if value
            .get("test")
            .and_then(toml::Value::as_array)
            .is_some_and(|a| {
                a.iter()
                    .any(|x| x.get("name").and_then(toml::Value::as_str) == Some(CHECK_TARGET))
            })
        {
            return Err(RustCheckError::Input(
                "reserved supplemental test target conflicts",
            ));
        }
        let name = lib
            .and_then(|x| x.get("name"))
            .and_then(toml::Value::as_str)
            .map(str::to_owned)
            .unwrap_or_else(|| request.package.replace('-', "_"));
        Ok((bytes, name))
    }
    pub(super) fn environment_supported(
        values: impl IntoIterator<Item = (String, String)>,
    ) -> Result<(), RustCheckError> {
        for (name, value) in values {
            let name = name.to_ascii_uppercase();
            if !value.is_empty()
                && (matches!(
                    name.as_str(),
                    "RUSTFLAGS"
                        | "CARGO_ENCODED_RUSTFLAGS"
                        | "RUSTDOCFLAGS"
                        | "CARGO_ENCODED_RUSTDOCFLAGS"
                        | "RUSTC_WRAPPER"
                        | "RUSTC_WORKSPACE_WRAPPER"
                        | "RUSTC"
                        | "RUSTDOC"
                        | "RUSTC_BOOTSTRAP"
                ) || name.starts_with("CARGO_BUILD_")
                    || name.starts_with("CARGO_PROFILE_")
                    || name.starts_with("CARGO_TARGET_"))
                && name != "CARGO_TARGET_DIR"
            {
                return Err(RustCheckError::Unavailable(
                    "ambient build overrides require a separately supported definition",
                ));
            }
        }
        Ok(())
    }
    pub(super) fn configuration_supported(root: &Path) -> Result<(), RustCheckError> {
        environment_supported(std::env::vars_os().filter_map(|(name, value)| {
            name.into_string()
                .ok()
                .map(|name| (name, value.to_string_lossy().into_owned()))
        }))?;
        for ancestor in root.ancestors().skip(1) {
            if ancestor.join(".cargo/config").exists()
                || ancestor.join(".cargo/config.toml").exists()
            {
                return Err(RustCheckError::Unavailable(
                    "inherited Cargo configuration is not admitted",
                ));
            }
            let manifest = ancestor.join("Cargo.toml");
            if manifest.exists()
                && parse(&bounded_file(&manifest, 128 * 1024)?)?
                    .get("workspace")
                    .is_some()
            {
                return Err(RustCheckError::Unavailable(
                    "inherited Cargo workspace configuration is not admitted",
                ));
            }
        }
        let cargo_home = std::env::var_os("CARGO_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join(".cargo")));
        if cargo_home.is_some_and(|p| p.join("config").exists() || p.join("config.toml").exists()) {
            return Err(RustCheckError::Unavailable(
                "owner Cargo configuration requires a separately supported definition",
            ));
        }
        Ok(())
    }
    fn snapshot(
        workspace: &WorkspaceManager,
        project: &Path,
    ) -> Result<Vec<FileIdentity>, RustCheckError> {
        let mut files = workspace.list_files()?;
        files.sort();
        let mut identities = Vec::new();
        let mut total = 0u64;
        for absolute in files {
            let relative = absolute
                .strip_prefix(workspace.root())
                .map_err(|_| RustCheckError::Input("source scope changed"))?;
            let display = relative.to_string_lossy().replace('\\', "/");
            let selected = display == "Cargo.toml"
                || display == "Cargo.lock"
                || ((display.starts_with("src/") || display.starts_with("tests/"))
                    && display.ends_with(".rs"));
            if !selected {
                continue;
            }
            if identities.len() >= 2048 {
                return Err(RustCheckError::Resource);
            }
            let capture = workspace.capture_file(relative, 1024 * 1024)?;
            let bytes = capture.bytes.ok_or(RustCheckError::Unavailable(
                "required source exceeds the admitted copy bound",
            ))?;
            total += bytes.len() as u64;
            if total > CHECK_SNAPSHOT_BYTES {
                return Err(RustCheckError::Resource);
            }
            create(&project.join(relative), &bytes)?;
            identities.push(FileIdentity {
                path: display,
                bytes: bytes.len() as u64,
                sha256: digest(&bytes),
            });
        }
        Ok(identities)
    }
    pub(super) fn selected_toolchain() -> Result<PathBuf, RustCheckError> {
        let rustup = std::env::var_os("RUSTUP_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("USERPROFILE").map(|p| PathBuf::from(p).join(".rustup")))
            .ok_or(RustCheckError::Unavailable(
                "Rust toolchain root unavailable",
            ))?;
        let selected = if let Ok(value) = std::env::var("RUSTUP_TOOLCHAIN") {
            value
        } else {
            let settings = parse(&bounded_file(&rustup.join("settings.toml"), 64 * 1024)?)?;
            settings
                .get("default_toolchain")
                .and_then(toml::Value::as_str)
                .ok_or(RustCheckError::Unavailable(
                    "configured Rust toolchain unavailable",
                ))?
                .to_owned()
        };
        if selected.contains(['/', '\\']) || !selected.ends_with("x86_64-pc-windows-msvc") {
            return Err(RustCheckError::Unavailable(
                "requires installed x86_64 Windows MSVC Rust toolchain",
            ));
        }
        let rust = rustup.join("toolchains").join(selected);
        Ok(rust.canonicalize()?)
    }
    fn toolchain(
        root: &Path,
        workspace: &Path,
        cancel: &CancellationToken,
        deadline: Instant,
    ) -> Result<(PathBuf, PathBuf, BTreeMap<String, String>, Sha256Digest), RustCheckError> {
        let rust = selected_toolchain()?;
        for program in ["cargo", "rustc", "rustdoc"] {
            crate::executable::resolve_fixed_executable_from_paths(
                program,
                &[rust.join("bin")],
                &[workspace, root],
                std::env::current_dir().ok().as_deref(),
            )
            .ok_or(RustCheckError::Unavailable(
                "configured compiler executable is not a trusted outside-workspace input",
            ))?;
        }
        let tools = root.join("tools");
        let mut sources = Vec::new();
        for entry in std::fs::read_dir(rust.join("bin")).map_err(|_| {
            RustCheckError::Unavailable("installed Rust compiler directory unavailable")
        })? {
            let path = entry?.path();
            let name = path
                .file_name()
                .and_then(|x| x.to_str())
                .unwrap_or("")
                .to_owned();
            if matches!(name.as_str(), "cargo.exe" | "rustc.exe" | "rustdoc.exe")
                || path.extension().is_some_and(|x| x == "dll")
            {
                let destination = PathBuf::from("bin").join(name);
                sources.push((path, destination));
            }
        }
        let rust_lib = rust.join("lib/rustlib/x86_64-pc-windows-msvc/lib");
        for entry in std::fs::read_dir(&rust_lib).map_err(|_| {
            RustCheckError::Unavailable("installed Rust target libraries unavailable")
        })? {
            let path = entry?.path();
            if path.is_file()
                && path
                    .extension()
                    .is_some_and(|x| x == "rlib" || x == "rmeta" || x == "lib" || x == "dll")
            {
                let destination = PathBuf::from("lib/rustlib/x86_64-pc-windows-msvc/lib")
                    .join(path.file_name().unwrap());
                sources.push((path, destination));
            }
        }
        let linker = find_msvc_tools::find_tool("x86_64-pc-windows-msvc", "link.exe").ok_or(
            RustCheckError::Unavailable("MSVC linker configuration unavailable"),
        )?;
        let linker_dir = linker.path().parent().ok_or(RustCheckError::Resource)?;
        crate::executable::resolve_fixed_executable_from_paths(
            "link",
            &[linker_dir.to_path_buf()],
            &[workspace, root],
            std::env::current_dir().ok().as_deref(),
        )
        .ok_or(RustCheckError::Unavailable(
            "configured linker is not a trusted outside-workspace input",
        ))?;
        for entry in std::fs::read_dir(linker_dir)? {
            let path = entry?.path();
            if path.file_name().is_some_and(|x| x == "link.exe")
                || path.extension().is_some_and(|x| x == "dll")
            {
                let destination = PathBuf::from("linker").join(path.file_name().unwrap());
                sources.push((path, destination));
            }
        }
        let libraries = linker
            .env()
            .into_iter()
            .find(|(name, _)| name.to_string_lossy().eq_ignore_ascii_case("LIB"))
            .map(|(_, value)| value)
            .ok_or(RustCheckError::Unavailable(
                "MSVC library configuration unavailable",
            ))?;
        let mut lib_paths = Vec::new();
        for (index, path) in std::env::split_paths(libraries)
            .filter(|p| !p.as_os_str().is_empty())
            .enumerate()
        {
            if index >= 8 {
                return Err(RustCheckError::Resource);
            }
            let destination = PathBuf::from(format!("native-lib/{index}"));
            lib_paths.push(tools.join(&destination));
            for entry in std::fs::read_dir(path).map_err(|_| {
                RustCheckError::Unavailable("configured MSVC library directory unavailable")
            })? {
                let path = entry?.path();
                let name = path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .to_ascii_lowercase();
                // The initial std-only slice does not need the SDK's unrelated static libraries.
                // An unsupported native link dependency fails, never falls back to ambient LIB.
                if matches!(
                    name.as_str(),
                    "kernel32.lib"
                        | "advapi32.lib"
                        | "bcrypt.lib"
                        | "ws2_32.lib"
                        | "userenv.lib"
                        | "ntdll.lib"
                        | "dbghelp.lib"
                        | "ole32.lib"
                        | "oleaut32.lib"
                        | "user32.lib"
                        | "shell32.lib"
                        | "uuid.lib"
                        | "rpcrt4.lib"
                        | "synchronization.lib"
                        | "winmm.lib"
                        | "crypt32.lib"
                        | "iphlpapi.lib"
                        | "psapi.lib"
                        | "msvcrt.lib"
                        | "msvcrtd.lib"
                        | "vcruntime.lib"
                        | "vcruntimed.lib"
                        | "libvcruntime.lib"
                        | "libvcruntimed.lib"
                        | "ucrt.lib"
                        | "ucrtd.lib"
                        | "oldnames.lib"
                        | "legacy_stdio_definitions.lib"
                ) {
                    let filename = path.file_name().unwrap().to_owned();
                    sources.push((path, destination.join(filename)));
                }
            }
        }
        sources.sort();
        if sources.len() > 2048 {
            return Err(RustCheckError::Resource);
        }
        let mut identities = Vec::new();
        let mut total = 0;
        for (source, destination) in sources {
            let canonical = source.canonicalize()?;
            if !canonical.is_absolute()
                || canonical.starts_with(workspace)
                || canonical.starts_with(root)
            {
                return Err(RustCheckError::Unavailable(
                    "compiler or library input is inside untrusted workspace storage",
                ));
            }
            let metadata = std::fs::symlink_metadata(&source)?;
            if !metadata.is_file()
                || metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
                || metadata.len() > 256 * 1024 * 1024
            {
                return Err(RustCheckError::Resource);
            }
            let target = tools.join(&destination);
            std::fs::create_dir_all(target.parent().ok_or(RustCheckError::Resource)?)?;
            let mut input = OpenOptions::new()
                .read(true)
                .share_mode(FILE_SHARE_READ)
                .open(&source)?;
            let mut output = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(target)?;
            let mut hash = Sha256::new();
            let mut bytes = 0u64;
            let mut buffer = [0u8; 64 * 1024];
            loop {
                if cancel.is_cancelled() {
                    return Err(RustCheckError::Unavailable(
                        "check cancelled during admission",
                    ));
                }
                if Instant::now() >= deadline {
                    return Err(RustCheckError::Unavailable(
                        "check deadline expired during admission",
                    ));
                }
                let count = input.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                bytes += count as u64;
                total += count as u64;
                if total > CHECK_TOOLCHAIN_BYTES || bytes > 256 * 1024 * 1024 {
                    return Err(RustCheckError::Resource);
                }
                hash.update(&buffer[..count]);
                output.write_all(&buffer[..count])?;
            }
            identities.push(FileIdentity {
                path: destination.to_string_lossy().replace('\\', "/"),
                bytes,
                sha256: Sha256Digest(format!("{:x}", hash.finalize())),
            });
        }
        let hash = digest(&serde_json::to_vec(&identities).map_err(|_| RustCheckError::Resource)?);
        create(
            &root.join("evidence/toolchain-manifest.json"),
            &serde_json::to_vec(&identities).map_err(|_| RustCheckError::Resource)?,
        )?;
        let temp = root.join("temp");
        let build = root.join("build");
        let home = root.join("cargo-home");
        for p in [&temp, &build, &home] {
            std::fs::create_dir(p)?;
        }
        let flags = [
            "-C".to_owned(),
            format!("linker={}", tools.join("linker/link.exe").display()),
            "--remap-path-prefix".into(),
            format!("{}=/argentum-check", root.display()),
        ]
        .join("\u{1f}");
        let env = BTreeMap::from([
            (
                "RUSTC".into(),
                tools.join("bin/rustc.exe").to_string_lossy().into_owned(),
            ),
            (
                "RUSTDOC".into(),
                tools.join("bin/rustdoc.exe").to_string_lossy().into_owned(),
            ),
            ("CARGO_HOME".into(), home.to_string_lossy().into_owned()),
            (
                "CARGO_TARGET_DIR".into(),
                build.to_string_lossy().into_owned(),
            ),
            ("CARGO_ENCODED_RUSTFLAGS".into(), flags),
            ("CARGO_INCREMENTAL".into(), "0".into()),
            ("CARGO_TERM_COLOR".into(), "never".into()),
            (
                "LIB".into(),
                std::env::join_paths(lib_paths)
                    .map_err(|_| RustCheckError::Resource)?
                    .to_string_lossy()
                    .into_owned(),
            ),
            (
                "PATH".into(),
                tools.join("bin").to_string_lossy().into_owned(),
            ),
            ("TEMP".into(), temp.to_string_lossy().into_owned()),
            ("TMP".into(), temp.to_string_lossy().into_owned()),
            ("USERPROFILE".into(), temp.to_string_lossy().into_owned()),
            ("APPDATA".into(), temp.to_string_lossy().into_owned()),
        ]);
        Ok((
            tools.join("bin/cargo.exe"),
            tools.join("bin/rustc.exe"),
            env,
            hash,
        ))
    }

    struct Step {
        exit: crate::ManagedProcessExit,
        stdout: String,
        stderr: String,
    }
    struct StepContext<'a> {
        cwd: &'a Path,
        env: &'a BTreeMap<String, String>,
        boundary: &'a Arc<ProbeSandbox>,
        cancel: &'a CancellationToken,
        deadline: Instant,
        storage: PathBuf,
    }
    async fn step(
        program: &Path,
        args: Vec<String>,
        context: &StepContext<'_>,
        peak: &mut u64,
    ) -> Result<Step, RustCheckError> {
        let StepContext {
            cwd,
            env,
            boundary,
            cancel,
            deadline,
            storage,
        } = context;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(RustCheckError::Unavailable("parent check deadline expired"));
        }
        if cancel.is_cancelled() {
            return Err(RustCheckError::Unavailable("check cancelled before spawn"));
        }
        let spec = ProcessSpec::argv(program.to_string_lossy(), args, cwd)?
            .with_timeout(remaining)?
            .with_output_limit(RAW_OUTPUT_BYTES)?
            .with_probe_boundary(Arc::clone(boundary), (*env).clone());
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<crate::ProcessOutputChunk>();
        let buffers = Arc::new(Mutex::new([String::new(), String::new()]));
        let copies = Arc::clone(&buffers);
        let capture = tokio::spawn(async move {
            while let Some(chunk) = rx.recv().await {
                let index = if chunk.stream == crate::ProcessStream::Stdout {
                    0
                } else {
                    1
                };
                if let Ok(mut b) = copies.lock() {
                    let room = RAW_OUTPUT_BYTES.saturating_sub(b[index].len());
                    let mut end = room.min(chunk.text.len());
                    while !chunk.text.is_char_boundary(end) {
                        end -= 1;
                    }
                    b[index].push_str(&chunk.text[..end]);
                }
            }
        });
        let process = match ManagedProcess::spawn_with_output(
            spec,
            ManagedProcessLimits::for_fixed_rust_verification()?,
            tx,
        )
        .await
        {
            Ok(process) => process,
            Err(error) => {
                capture.await.map_err(|_| RustCheckError::Resource)?;
                return Err(error.into());
            }
        };
        let guard = cancel.child_token();
        let mut wait = Box::pin(process.wait_with_cancellation(guard.clone(), remaining));
        let mut exceeded = false;
        let mut observation_failure = None;
        let exit = loop {
            tokio::select! {value=&mut wait=>break value,_ = tokio::time::sleep(Duration::from_millis(50))=>{
                let total=(||{let root=storage.parent().ok_or(RustCheckError::Resource)?;let mut bytes=0u64;for name in ["build","temp","cargo-home"]{bytes=bytes.checked_add(tree_bytes(&root.join(name))?).ok_or(RustCheckError::Resource)?;}bytes=bytes.checked_add(tree_bytes(boundary.profile())?).ok_or(RustCheckError::Resource)?;Ok::<_,RustCheckError>(bytes)})();
                match total {Ok(bytes)=>{*peak=(*peak).max(bytes);if bytes>CHECK_SCRATCH_BYTES{exceeded=true;guard.cancel();}},Err(error)=>{observation_failure=Some(error);exceeded=true;guard.cancel();}}
            }}
        };
        capture.await.map_err(|_| RustCheckError::Resource)?;
        let exit = exit?;
        if exceeded {
            return Err(observation_failure.unwrap_or(RustCheckError::Resource));
        }
        let mut values = buffers.lock().map_err(|_| RustCheckError::Resource)?;
        Ok(Step {
            exit,
            stdout: std::mem::take(&mut values[0]),
            stderr: std::mem::take(&mut values[1]),
        })
    }
    pub(super) async fn run(
        workspace: WorkspaceManager,
        owner: ProcessJobOwner,
        request: RustCheckRequest,
        cancel: CancellationToken,
        maximum: Duration,
    ) -> Result<RustCheckOutput, RustCheckError> {
        let start = Instant::now();
        let deadline = start + maximum.min(Duration::from_secs(RUST_CHECK_MAX_SECONDS));
        if cancel.is_cancelled() {
            return Err(RustCheckError::Unavailable(
                "check cancelled before admission",
            ));
        }
        let (original_manifest, library) = manifest(&workspace, &request)?;
        let configuration = rust_check_configuration_digest(workspace.root())?;
        let before = workspace.revision_digest(crate::default_revision_limits())?;
        let area = Area::new()?;
        let project = area.keep.join("project");
        std::fs::create_dir(&project)?;
        let files = snapshot(&workspace, &project)?;
        let snapshot_sha =
            digest(&serde_json::to_vec(&files).map_err(|_| RustCheckError::Resource)?);
        let current = workspace.revision_digest(crate::default_revision_limits())?;
        if current != before {
            return Err(RustCheckError::Unavailable(
                "source changed during snapshot admission",
            ));
        }
        if bounded_file(&project.join("Cargo.toml"), 128 * 1024)? != original_manifest {
            return Err(RustCheckError::Unavailable("admitted manifest changed"));
        }
        let lockfile = bounded_file(&project.join("Cargo.lock"), 1024 * 1024)?;
        create(&area.keep.join("original-Cargo.toml"), &original_manifest)?;
        let derived = format!(
            "{}\n[[test]]\nname = \"{CHECK_TARGET}\"\npath = \".argentum_public_checks.rs\"\n",
            std::str::from_utf8(&original_manifest)
                .map_err(|_| RustCheckError::Input("manifest UTF-8"))?
        );
        std::fs::write(project.join("Cargo.toml"), derived.as_bytes())?;
        create(
            &project.join(".argentum_public_checks.rs"),
            request.source.as_str().as_bytes(),
        )?;
        create(
            &area.keep.join("snapshot-manifest.json"),
            &serde_json::to_vec(&files).map_err(|_| RustCheckError::Resource)?,
        )?;
        let (cargo, _rustc, env, toolchain) =
            toolchain(&area.root, workspace.root(), &cancel, deadline)?;
        let boundary = ProbeSandbox::create(&area.root)?;
        std::fs::create_dir_all(boundary.profile().join("Temp"))?;
        boundary.grant_tree(boundary.profile(), true)?;
        for p in [
            area.root.join("build"),
            area.root.join("cargo-home"),
            area.root.join("temp"),
        ] {
            boundary.grant_tree(&p, true)?;
        }
        let definition=digest(b"argentum.rust-check.v1.lpac.ro-source.registry-read.no-network.default-features.8-processes.95-cpu.4GiB.60sec.32KiB-input.64KiB-output.1GiB-toolchain.256MiB-build");
        let mut peak = 0;
        let mut args = vec![
            "test".into(),
            "--no-run".into(),
            "--locked".into(),
            "--offline".into(),
            "-j".into(),
            "1".into(),
            "--package".into(),
            request.package.clone(),
            "--test".into(),
            CHECK_TARGET.into(),
            "--message-format=json".into(),
        ];
        if request.profile == RustCheckProfile::Release {
            args.push("--release".into());
        }
        let step_context = StepContext {
            cwd: &project,
            env: &env,
            boundary: &boundary,
            cancel: &cancel,
            deadline,
            storage: area.root.join("build"),
        };
        #[cfg(test)]
        let _ = AFTER_SNAPSHOT_ADMISSION.try_with(|change| change());
        let compile = step(&cargo, args, &step_context, &mut peak).await?;
        let mut receipt = SupplementalCheckReceipt {
            schema_version: 1,
            id: uuid::Uuid::new_v4(),
            project_id: owner.project_id,
            session_id: owner.session_id,
            run_id: owner.run_id,
            owner_binding_sha256: digest(format!("{:?}", owner).as_bytes()),
            definition_sha256: definition,
            package: request.package,
            library_target: library,
            target: "x86_64-pc-windows-msvc".into(),
            profile: request.profile,
            source_revision: before.digest.clone(),
            revision_after: before.digest.clone(),
            source_fresh: true,
            snapshot_sha256: snapshot_sha,
            snapshot_file_count: files.len() as u64,
            snapshot_bytes: files.iter().map(|x| x.bytes).sum(),
            manifest_sha256: digest(&original_manifest),
            derived_manifest_sha256: digest(derived.as_bytes()),
            lockfile_sha256: digest(&lockfile),
            test_sha256: digest(request.source.as_str().as_bytes()),
            toolchain_sha256: toolchain,
            toolchain_selection_sha256: configuration.clone(),
            test_executable_sha256: None,
            state: SupplementalCheckState::CompileFailed,
            compile_exit_code: compile.exit.exit_code,
            test_exit_code: None,
            reported_test_count: None,
            duration_ms: 0,
            stdout_bytes: compile.exit.stdout_bytes,
            stderr_bytes: compile.exit.stderr_bytes,
            stdout_sha256: compile.exit.stdout_sha256.clone(),
            stderr_sha256: compile.exit.stderr_sha256.clone(),
            output_complete: !compile.exit.stdout_truncated && !compile.exit.stderr_truncated,
            output_truncated: compile.exit.stdout_truncated || compile.exit.stderr_truncated,
            boundary: "windows.lpac.registry-read.no-network.owned-copies.v1".into(),
            reason_code: None,
            storage_peak_bytes: peak,
            cleanup_complete: false,
            replacements: request.replaces,
            assertions_authored_by: "model".into(),
            independently_verified: false,
        };
        let mut stdout = String::new();
        let mut stderr = compile.stderr.clone();
        for line in compile.stdout.lines() {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
                if let Some(rendered) = value
                    .get("message")
                    .and_then(|x| x.get("rendered"))
                    .and_then(|x| x.as_str())
                {
                    stderr.push_str(rendered);
                }
            }
        }
        if compile.exit.timed_out {
            receipt.state = SupplementalCheckState::TimedOut;
        } else if cancel.is_cancelled() {
            receipt.state = SupplementalCheckState::Cancelled;
        } else if compile.exit.exit_code == Some(0) && receipt.output_complete {
            let executables = compile
                .stdout
                .lines()
                .filter_map(|line| serde_json::from_str::<serde_json::Value>(line).ok())
                .filter(|x| {
                    x["reason"] == "compiler-artifact" && x["target"]["name"] == CHECK_TARGET
                })
                .filter_map(|x| {
                    x.get("executable")
                        .and_then(|x| x.as_str())
                        .map(PathBuf::from)
                })
                .collect::<Vec<_>>();
            if executables.len() != 1 {
                return Err(RustCheckError::Unavailable(
                    "compiled test artifact identity unavailable",
                ));
            }
            let executable = executables[0].canonicalize()?;
            if !executable.starts_with(area.root.join("build").canonicalize()?) {
                return Err(RustCheckError::Unavailable(
                    "test artifact escapes the owned build area",
                ));
            }
            let binary = bounded_file(&executable, CHECK_SCRATCH_BYTES)?;
            receipt.test_executable_sha256 = Some(digest(&binary));
            // After compilation, executable input has no write access to its source/build/temp/profile.
            for p in [
                area.root.join("build"),
                area.root.join("cargo-home"),
                area.root.join("temp"),
                boundary.profile().to_path_buf(),
            ] {
                boundary.grant_tree(&p, false)?;
            }
            let list = step(
                &executable,
                vec!["--list".into(), "--format=terse".into()],
                &step_context,
                &mut peak,
            )
            .await?;
            let names = list
                .stdout
                .lines()
                .filter_map(|x| x.strip_suffix(": test"))
                .collect::<Vec<_>>();
            let count = names.len();
            receipt.test_exit_code = list.exit.exit_code;
            receipt.reported_test_count = Some(count as u32);
            receipt.stdout_bytes = list.exit.stdout_bytes;
            receipt.stderr_bytes = list.exit.stderr_bytes;
            receipt.stdout_sha256 = list.exit.stdout_sha256.clone();
            receipt.stderr_sha256 = list.exit.stderr_sha256.clone();
            receipt.output_truncated = list.exit.stdout_truncated || list.exit.stderr_truncated;
            receipt.output_complete = !receipt.output_truncated;
            stdout = list.stdout.clone();
            stderr.push_str(&list.stderr);
            receipt.state = if list.exit.timed_out {
                SupplementalCheckState::TimedOut
            } else if cancel.is_cancelled() {
                SupplementalCheckState::Cancelled
            } else if !receipt.output_complete {
                SupplementalCheckState::OutputIncomplete
            } else if list.exit.exit_code != Some(0) {
                receipt.reason_code = Some("test_inventory_failed".into());
                SupplementalCheckState::Failed
            } else if count == 0 {
                SupplementalCheckState::ZeroTests
            } else {
                SupplementalCheckState::OutputIncomplete
            };
            if count > 0
                && count <= 128
                && list.exit.exit_code == Some(0)
                && !list.exit.stdout_truncated
                && !list.exit.stderr_truncated
            {
                let executed = step(
                    &executable,
                    vec!["--test-threads=1".into()],
                    &step_context,
                    &mut peak,
                )
                .await?;
                receipt.test_exit_code = executed.exit.exit_code;
                receipt.reported_test_count = Some(count as u32);
                receipt.stdout_bytes = executed.exit.stdout_bytes;
                receipt.stderr_bytes = executed.exit.stderr_bytes;
                receipt.stdout_sha256 = executed.exit.stdout_sha256;
                receipt.stderr_sha256 = executed.exit.stderr_sha256;
                receipt.output_truncated =
                    executed.exit.stdout_truncated || executed.exit.stderr_truncated;
                receipt.output_complete = !receipt.output_truncated;
                let expected=format!("test result: ok. {count} passed; 0 failed; 0 ignored; 0 measured; 0 filtered out;");
                let names_finished = names.iter().all(|name| {
                    executed
                        .stdout
                        .lines()
                        .any(|line| line.trim() == format!("test {name} ... ok"))
                });
                receipt.state = if executed.exit.timed_out {
                    SupplementalCheckState::TimedOut
                } else if cancel.is_cancelled() {
                    SupplementalCheckState::Cancelled
                } else if !receipt.output_complete {
                    SupplementalCheckState::OutputIncomplete
                } else if executed.exit.exit_code != Some(0) {
                    SupplementalCheckState::Failed
                } else if names_finished
                    && executed.stdout.lines().any(|x| x.starts_with(&expected))
                {
                    SupplementalCheckState::ReportedPass
                } else {
                    SupplementalCheckState::OutputIncomplete
                };
                stdout = executed.stdout;
                stderr.push_str(&executed.stderr);
            }
        } else if !receipt.output_complete {
            receipt.state = SupplementalCheckState::OutputIncomplete;
        }
        let after = workspace.revision_digest(crate::default_revision_limits())?;
        receipt.revision_after = after.digest;
        receipt.source_fresh = receipt.source_revision == receipt.revision_after
            && rust_check_configuration_digest(workspace.root())
                .is_ok_and(|value| value == configuration);
        if !receipt.source_fresh {
            receipt.state = SupplementalCheckState::Stale;
            receipt.reason_code = Some("source_or_configuration_changed".into());
        }
        receipt.duration_ms = start.elapsed().as_millis().min(u64::MAX as u128) as u64;
        receipt.storage_peak_bytes = peak;
        receipt.cleanup_complete = boundary.retire().is_ok() && area.clean_generated().is_ok();
        if !receipt.cleanup_complete {
            receipt.state = SupplementalCheckState::Unavailable;
            receipt.reason_code = Some("owned_cleanup_unsettled".into());
        }
        create(
            &area.keep.join("receipt.json"),
            &serde_json::to_vec(&receipt).map_err(|_| RustCheckError::Resource)?,
        )?;
        fn prefix(value: String) -> String {
            let mut end = MODEL_DIAGNOSTIC_BYTES.min(value.len());
            while !value.is_char_boundary(end) {
                end -= 1;
            }
            value[..end].to_owned()
        }
        let stdout_prefix_truncated = stdout.len() > MODEL_DIAGNOSTIC_BYTES;
        let stderr_prefix_truncated = stderr.len() > MODEL_DIAGNOSTIC_BYTES;
        Ok(RustCheckOutput {
            receipt,
            stdout: prefix(stdout),
            stderr: prefix(stderr),
            stdout_prefix_truncated,
            stderr_prefix_truncated,
        })
    }
}
