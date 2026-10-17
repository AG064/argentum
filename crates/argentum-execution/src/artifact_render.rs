//! Fixed PDF worker in a private LPAC byte-copy. Not a generic process tool.
use std::{
    io,
    path::{Path, PathBuf},
};
use tokio_util::sync::CancellationToken;

pub const PDFIUM_DLL_SHA256: &str =
    "ae913f407e8d3f5cdbf08a9869564d647b13e132e01eb376c0a5a33b7a227a45";
pub const ARTIFACT_FONT_SHA256: &str =
    "bfb7bb691513f12e734dc346c03a03f784912432d7e3fa8e56efcf906fe86b3d";

enum WorkerInput<'a> {
    Pdf(&'a [u8]),
    Markdown { source: &'a str, font: &'a Path },
}

#[derive(Debug)]
pub struct PdfWorkerRun {
    pub directory: PathBuf,
    pub exit_code: Option<i32>,
    pub timed_out: bool,
    pub duration_ms: u64,
    pub boundary: &'static str,
    pub diagnostic: String,
}

pub async fn run_isolated_pdf_worker(
    worker: &Path,
    worker_sha256: &str,
    pdfium: &Path,
    pdf: &[u8],
    private_root: &Path,
    cancel: &CancellationToken,
) -> io::Result<PdfWorkerRun> {
    #[cfg(windows)]
    {
        windows::run(
            worker,
            worker_sha256,
            pdfium,
            WorkerInput::Pdf(pdf),
            private_root,
            cancel,
        )
        .await
    }
    #[cfg(not(windows))]
    {
        let _ = (worker, worker_sha256, pdfium, pdf, private_root, cancel);
        Err(io::Error::other("PDF LPAC worker requires Windows"))
    }
}

pub async fn run_isolated_markdown_worker(
    worker: &Path,
    worker_sha256: &str,
    pdfium: &Path,
    font: &Path,
    markdown: &str,
    private_root: &Path,
    cancel: &CancellationToken,
) -> io::Result<PdfWorkerRun> {
    #[cfg(windows)]
    {
        windows::run(
            worker,
            worker_sha256,
            pdfium,
            WorkerInput::Markdown {
                source: markdown,
                font,
            },
            private_root,
            cancel,
        )
        .await
    }
    #[cfg(not(windows))]
    {
        let _ = (
            worker,
            worker_sha256,
            pdfium,
            font,
            markdown,
            private_root,
            cancel,
        );
        Err(io::Error::other("Markdown PDF worker requires Windows"))
    }
}

/// Defense in depth: the worker refuses a direct launch in the owner's token.
pub fn current_process_is_lpac() -> bool {
    #[cfg(windows)]
    {
        windows::is_lpac()
    }
    #[cfg(not(windows))]
    {
        false
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use crate::{
        probe_sandbox::ProbeSandbox, ManagedProcess, ManagedProcessLimits, ProcessSpec,
        VerifiedFileLease,
    };
    use sha2::{Digest, Sha256};
    use std::{
        collections::BTreeMap,
        io::{Read, Write},
        os::windows::fs::MetadataExt,
        sync::Arc,
        time::{Duration, Instant},
    };

    pub(super) fn is_lpac() -> bool {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            Security::{GetTokenInformation, TokenIsAppContainer, TOKEN_DUPLICATE, TOKEN_QUERY},
            System::Threading::{GetCurrentProcess, OpenProcessToken},
        };
        let mut token = std::ptr::null_mut();
        // SAFETY: query only the current process token; close the unique handle.
        unsafe {
            if OpenProcessToken(
                GetCurrentProcess(),
                TOKEN_QUERY | TOKEN_DUPLICATE,
                &mut token,
            ) == 0
            {
                return false;
            }
            let mut app = 0u32;
            let mut count = 0;
            let ok = GetTokenInformation(
                token,
                TokenIsAppContainer,
                (&mut app as *mut u32).cast(),
                4,
                &mut count,
            ) != 0
                && crate::probe_sandbox::verify_no_all_app_packages(token).is_ok();
            CloseHandle(token);
            ok && app == 1
        }
    }

    fn copy_pinned(from: &Path, to: &Path, max: u64, expected: &str) -> io::Result<()> {
        let mut lease = VerifiedFileLease::open(from, max)?;
        if lease.sha256() != expected {
            return Err(io::Error::other("PDF worker dependency identity changed"));
        }
        let mut bytes = Vec::new();
        std::fs::File::open(lease.path())?
            .take(max + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > max || format!("{:x}", Sha256::digest(&bytes)) != expected {
            return Err(io::Error::other("PDF dependency changed while copying"));
        }
        lease.verify_identity()?;
        lease.verify_unchanged()?;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(to)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        Ok(())
    }

    fn tree_bytes(root: &Path) -> io::Result<u64> {
        let mut pending = vec![root.to_path_buf()];
        let mut total = 0u64;
        let mut count = 0;
        while let Some(path) = pending.pop() {
            count += 1;
            if count > 256 {
                return Err(io::Error::other("PDF worker file count exceeded"));
            }
            let meta = std::fs::symlink_metadata(&path)?;
            if meta.file_attributes() & 0x400 != 0 {
                return Err(io::Error::other(
                    "PDF worker output contains a reparse point",
                ));
            }
            if meta.is_dir() {
                for entry in std::fs::read_dir(path)? {
                    pending.push(entry?.path());
                }
            } else {
                total = total
                    .checked_add(meta.len())
                    .ok_or_else(|| io::Error::other("PDF worker storage overflow"))?;
            }
        }
        Ok(total)
    }

    pub(super) async fn run(
        worker: &Path,
        worker_sha256: &str,
        pdfium: &Path,
        input: WorkerInput<'_>,
        root: &Path,
        cancel: &CancellationToken,
    ) -> io::Result<PdfWorkerRun> {
        if cancel.is_cancelled() {
            return Err(io::Error::other("PDF render cancelled before admission"));
        }
        let valid_input = match &input {
            WorkerInput::Pdf(pdf) => pdf.len() <= 10 * 1024 * 1024 && pdf.starts_with(b"%PDF-"),
            WorkerInput::Markdown { source, .. } => {
                !source.is_empty() && source.len() <= 64 * 1024 && !source.contains('\0')
            }
        };
        if !root.is_absolute() || !valid_input {
            return Err(io::Error::other("invalid bounded PDF render input"));
        }
        crate::ensure_owner_only_directory(root)?;
        let root = root.canonicalize()?;
        let owned = root.join(format!("pdf-{}", uuid::Uuid::new_v4()));
        crate::ensure_owner_only_directory(&owned)?;
        copy_pinned(
            worker,
            &owned.join("argentum-pdf-worker.exe"),
            64 * 1024 * 1024,
            worker_sha256,
        )?;
        copy_pinned(
            pdfium,
            &owned.join("pdfium.dll"),
            16 * 1024 * 1024,
            PDFIUM_DLL_SHA256,
        )?;
        let (name, bytes) = match input {
            WorkerInput::Pdf(pdf) => ("input.pdf", pdf),
            WorkerInput::Markdown { source, font } => {
                copy_pinned(
                    font,
                    &owned.join("NotoSans.ttf"),
                    4 * 1024 * 1024,
                    ARTIFACT_FONT_SHA256,
                )?;
                ("input.md", source.as_bytes())
            }
        };
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(owned.join(name))?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        let output = owned.join("output");
        crate::ensure_owner_only_directory(&output)?;
        let sandbox = ProbeSandbox::create(&owned)?;
        sandbox.grant_tree(&output, true)?;
        let profile = sandbox.profile().to_path_buf();
        let env = BTreeMap::from([
            ("TEMP".into(), profile.to_string_lossy().into_owned()),
            ("TMP".into(), profile.to_string_lossy().into_owned()),
        ]);
        let outcome = async {
            let spec = ProcessSpec::argv(owned.join("argentum-pdf-worker.exe").to_string_lossy(),Vec::<String>::new(),&owned)
                .map_err(io::Error::other)?.with_timeout(Duration::from_secs(30)).map_err(io::Error::other)?
                .with_output_limit(4096).map_err(io::Error::other)?.with_probe_boundary(Arc::clone(&sandbox),env);
            let (tx,mut rx) = tokio::sync::mpsc::unbounded_channel::<crate::ProcessOutputChunk>();
            let capture = tokio::spawn(async move {
                let mut result = String::new();
                while let Some(chunk) = rx.recv().await {
                    if chunk.stream == crate::ProcessStream::Stderr {
                        let mut end = 4096usize.saturating_sub(result.len()).min(chunk.text.len());
                        while !chunk.text.is_char_boundary(end) { end -= 1; }
                        result.push_str(&chunk.text[..end]);
                    }
                }
                result
            });
            let process = match ManagedProcess::spawn_with_output(spec,ManagedProcessLimits::new(1,50,4096,512*1024*1024,512*1024*1024)
                .map_err(io::Error::other)?,tx).await {
                Ok(process) => process,
                Err(error) => { let _ = capture.await; return Err(io::Error::other(error)); }
            };
            let guard = cancel.child_token();
            let start = Instant::now();
            let mut wait = Box::pin(process.wait_with_cancellation(guard.clone(),Duration::from_secs(30)));
            let mut storage_failed = false;
            let exit = loop {
                tokio::select! { result = &mut wait => break result.map_err(io::Error::other)?,
                    _ = tokio::time::sleep(Duration::from_millis(50)) => {
                        if tree_bytes(&output).and_then(|a| tree_bytes(&profile).map(|b|a.saturating_add(b)))
                            .map_or(true,|n| n > 24*1024*1024) { storage_failed = true; guard.cancel(); }
                    }
                }
            };
            let diagnostic = capture.await.map_err(io::Error::other)?;
            if storage_failed { return Err(io::Error::other("PDF worker exceeded its output boundary")); }
            if cancel.is_cancelled() { return Err(io::Error::other("PDF worker cancelled and drained")); }
            if exit.stdout_truncated || exit.stderr_truncated { return Err(io::Error::other("PDF worker exceeded diagnostic limits")); }
            Ok(PdfWorkerRun {directory:owned,exit_code:exit.exit_code,timed_out:exit.timed_out,
                duration_ms:start.elapsed().as_millis() as u64,boundary:"windows.lpac.no-network.fixed-pdf-worker.v1",diagnostic})
        }.await;
        sandbox.retire()?;
        outcome
    }
}
