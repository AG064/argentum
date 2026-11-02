//! Shared local-only readiness helpers for doctor and the browser commands.
//!
//! These helpers inspect bounded regular files and module manifests. They do
//! not execute Node, npm, npx, a browser, or any network request.

use std::collections::BTreeMap;
use std::io::{Error as IoError, Read};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::Value;
use sha2::{Digest, Sha256};

#[cfg(feature = "extensions")]
pub(crate) const CHROMIUM_HASH_LIMIT_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Debug, Default, Serialize)]
pub(crate) struct FileReport {
    pub path: String,
    pub exists: bool,
    pub bytes: Option<u64>,
    pub sha256: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct ModuleReport {
    pub path: String,
    pub exists: bool,
    pub version: Option<String>,
    pub error: Option<String>,
}

#[derive(Debug, Default, Serialize)]
pub(crate) struct BrowserCacheReport {
    pub configured: bool,
    pub path: Option<String>,
    pub exists: bool,
    pub chromium: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[cfg(feature = "extensions")]
pub(crate) struct FileHash {
    pub path: String,
    pub bytes: u64,
    pub sha256: String,
}

pub(crate) fn read_bounded(
    path: &Path,
    limit: u64,
    label: &'static str,
) -> Result<Vec<u8>, IoError> {
    let metadata = std::fs::symlink_metadata(path)?;
    if !metadata.file_type().is_file() || metadata.len() > limit {
        return Err(IoError::other(format!(
            "{label} must be a bounded regular file"
        )));
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(IoError::other(format!("{label} exceeds its byte limit")));
    }
    Ok(bytes)
}

pub(crate) fn file_report(path: &Path, limit: u64) -> FileReport {
    let mut report = FileReport {
        path: path.display().to_string(),
        ..FileReport::default()
    };
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_file() => {
            report.exists = true;
            match read_bounded(path, limit, "browser dependency file") {
                Ok(bytes) => {
                    report.bytes = Some(bytes.len() as u64);
                    report.sha256 = Some(format!("{:x}", Sha256::digest(&bytes)));
                }
                Err(error) => report.error = Some(error.to_string()),
            }
        }
        Ok(_) => {
            report.exists = true;
            report.error = Some("not a regular file".into());
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => report.error = Some(error.to_string()),
    }
    report
}

pub(crate) fn module_report(directory: &Path) -> ModuleReport {
    let package = directory.join("package.json");
    let file = file_report(&package, 64 * 1024);
    let mut report = ModuleReport {
        path: directory.display().to_string(),
        exists: file.exists,
        ..ModuleReport::default()
    };
    if !file.exists {
        report.error = Some("missing".into());
        return report;
    }
    match read_bounded(&package, 64 * 1024, "module package.json") {
        Ok(bytes) => match serde_json::from_slice::<Value>(&bytes) {
            Ok(value) => {
                report.version = value
                    .get("version")
                    .and_then(Value::as_str)
                    .filter(|value| !value.is_empty() && value.len() <= 64)
                    .map(str::to_owned);
            }
            Err(_) => report.error = Some("package.json is invalid".into()),
        },
        Err(error) => report.error = Some(error.to_string()),
    }
    report
}

pub(crate) fn browser_cache_report(environment: &BTreeMap<String, String>) -> BrowserCacheReport {
    let Some(path) = environment
        .get("ARGENTUM_PLAYWRIGHT_BROWSERS_PATH")
        .map(|value| value.trim())
        .filter(|value| !value.is_empty())
    else {
        return BrowserCacheReport::default();
    };
    let cache = PathBuf::from(path);
    let mut report = BrowserCacheReport {
        configured: true,
        path: Some(cache.display().to_string()),
        exists: cache.is_dir(),
        ..BrowserCacheReport::default()
    };
    report.chromium = chromium_executables(&cache);
    report
}

pub(crate) fn chromium_executables(cache: &Path) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(cache) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten().take(256) {
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if !name.starts_with("chromium") {
            continue;
        }
        for relative in [
            "chrome-win/chrome.exe",
            "chrome-win64/chrome.exe",
            "chrome-linux/chrome",
            "chrome-mac/Chromium.app/Contents/MacOS/Chromium",
        ] {
            let candidate = entry.path().join(relative);
            if candidate.is_file() {
                found.push(candidate.display().to_string());
            }
        }
    }
    found.sort();
    found
}

#[cfg(feature = "extensions")]
pub(crate) fn file_detail(file: &FileReport) -> String {
    if let Some(error) = &file.error {
        return error.clone();
    }
    if !file.exists {
        return "missing".into();
    }
    format!(
        "{} bytes sha256 {}",
        file.bytes.unwrap_or_default(),
        file.sha256.as_deref().unwrap_or("unavailable")
    )
}

#[cfg(feature = "extensions")]
pub(crate) fn module_detail(module: &ModuleReport) -> String {
    if !module.exists {
        return module.error.clone().unwrap_or_else(|| "missing".into());
    }
    format!("version {}", module.version.as_deref().unwrap_or("unknown"))
}

#[cfg(feature = "extensions")]
pub(crate) fn file_hash(path: &Path) -> Option<FileHash> {
    let report = file_report(path, 64 * 1024);
    if !report.exists || report.sha256.is_none() {
        return None;
    }
    Some(FileHash {
        path: report.path,
        bytes: report.bytes.unwrap_or_default(),
        sha256: report.sha256.unwrap_or_default(),
    })
}

#[cfg(feature = "extensions")]
pub(crate) fn stream_hash(path: &Path) -> Option<FileHash> {
    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; 1024 * 1024];
    let mut total = 0_u64;
    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        total = total.saturating_add(read as u64);
        if total > CHROMIUM_HASH_LIMIT_BYTES {
            return None;
        }
        hasher.update(&buffer[..read]);
    }
    Some(FileHash {
        path: path.display().to_string(),
        bytes: total,
        sha256: format!("{:x}", hasher.finalize()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_and_module_reports_cover_present_and_missing_files() {
        let directory = tempfile::tempdir().expect("directory");
        let file = directory.path().join("adapter.mjs");
        std::fs::write(&file, b"adapter").expect("adapter");
        let report = file_report(&file, 1024);
        assert!(report.exists);
        assert_eq!(report.bytes, Some(7));
        assert!(report.sha256.is_some());
        let missing = file_report(&directory.path().join("missing.mjs"), 1024);
        assert!(!missing.exists);

        let module = directory.path().join("node_modules").join("playwright");
        std::fs::create_dir_all(&module).expect("module");
        std::fs::write(module.join("package.json"), br#"{"version":"1.64.0"}"#).expect("package");
        let report = module_report(&module);
        assert!(report.exists);
        assert_eq!(report.version.as_deref(), Some("1.64.0"));
        let missing = module_report(
            &directory
                .path()
                .join("node_modules")
                .join("playwright-core"),
        );
        assert!(!missing.exists);
    }

    #[test]
    fn browser_cache_reports_a_chromium_executable() {
        let directory = tempfile::tempdir().expect("directory");
        let cache = directory.path().join("ms-playwright");
        let chrome = cache
            .join("chromium-1234")
            .join("chrome-win")
            .join("chrome.exe");
        std::fs::create_dir_all(chrome.parent().expect("chrome parent")).expect("cache");
        std::fs::write(&chrome, b"stub").expect("chrome");
        let mut environment = BTreeMap::new();
        environment.insert(
            "ARGENTUM_PLAYWRIGHT_BROWSERS_PATH".to_owned(),
            cache.display().to_string(),
        );
        let report = browser_cache_report(&environment);
        assert!(report.configured);
        assert!(report.exists);
        assert_eq!(report.chromium.len(), 1);
        assert_eq!(Path::new(&report.chromium[0]), chrome.as_path());
    }
}
