//! Checks for, downloads, and installs Argentum releases.
//!
//! Every step runs from an explicit click in Settings. Nothing here runs in the
//! background, and no tool or model can reach it. A download only counts once it
//! matches the SHA-256 published beside it, and the swap restores the previous
//! executable if any step fails.
//!
//! The checksum proves the file arrived intact. Its authenticity rests on TLS to
//! GitHub and on control of the release account, because releases are not signed
//! yet. See `docs/UI_MODULES.md`.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::Deserialize;
use sha2::{Digest, Sha256};
use url::Url;

pub const CURRENT_VERSION: &str = env!("CARGO_PKG_VERSION");
const RELEASES_URL: &str = "https://api.github.com/repos/AG064/argentum/releases/latest";
const EXECUTABLE_ASSET: &str = "Argentum.exe";
const CHECKSUM_ASSET: &str = "SHA256SUMS.txt";
const MAX_EXECUTABLE_BYTES: u64 = 512 * 1024 * 1024;
const MAX_CHECKSUM_BYTES: usize = 64 * 1024;
const MAX_METADATA_BYTES: usize = 2 * 1024 * 1024;
const MAX_NOTES_CHARS: usize = 16 * 1024;

/// A plain message, already worded for the person reading it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateError(pub String);

impl std::fmt::Display for UpdateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl std::error::Error for UpdateError {}

fn fail<T>(message: impl Into<String>) -> Result<T, UpdateError> {
    Err(UpdateError(message.into()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub page: String,
    pub executable_url: Option<String>,
    pub checksum_url: Option<String>,
}

#[derive(Deserialize)]
struct WireRelease {
    tag_name: String,
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    html_url: String,
    #[serde(default)]
    assets: Vec<WireAsset>,
}

#[derive(Deserialize)]
struct WireAsset {
    name: String,
    browser_download_url: String,
}

/// `(major, minor, patch, is_release)`, so a pre-release sorts below its release.
fn version_key(version: &str) -> Option<(u64, u64, u64, bool)> {
    let trimmed = version.trim().trim_start_matches(['v', 'V']);
    let (core, pre) = match trimmed.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (trimmed, None),
    };
    let core = core.split('+').next()?;
    let mut parts = core.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next().map_or(Some(0), |part| part.parse().ok())?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch, pre.is_none()))
}

pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (version_key(candidate), version_key(current)) {
        (Some(candidate), Some(current)) => candidate > current,
        _ => false,
    }
}

/// Assets may only come from GitHub over https.
fn trusted_asset_url(raw: &str) -> Option<String> {
    let url = Url::parse(raw).ok()?;
    (url.scheme() == "https" && url.host_str() == Some("github.com")).then(|| url.to_string())
}

pub fn parse_release(payload: &[u8]) -> Result<Release, UpdateError> {
    let wire: WireRelease = serde_json::from_slice(payload)
        .map_err(|_| UpdateError("The release information could not be read.".into()))?;
    if version_key(&wire.tag_name).is_none() {
        return fail("The latest release does not have a readable version.");
    }
    let asset = |name: &str| {
        wire.assets
            .iter()
            .find(|asset| asset.name == name)
            .and_then(|asset| trusted_asset_url(&asset.browser_download_url))
    };
    Ok(Release {
        version: wire.tag_name.trim_start_matches(['v', 'V']).to_owned(),
        notes: wire
            .body
            .as_deref()
            .unwrap_or_default()
            .chars()
            .take(MAX_NOTES_CHARS)
            .collect(),
        page: wire.html_url.clone(),
        executable_url: asset(EXECUTABLE_ASSET),
        checksum_url: asset(CHECKSUM_ASSET),
    })
}

fn client() -> Result<reqwest::Client, UpdateError> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::limited(5))
        .connect_timeout(Duration::from_secs(15))
        .user_agent(concat!("Argentum/", env!("CARGO_PKG_VERSION")))
        .https_only(true)
        .build()
        .map_err(|_| UpdateError("The update request could not be prepared.".into()))
}

async fn fetch_bounded(
    client: &reqwest::Client,
    url: &str,
    limit: usize,
) -> Result<Vec<u8>, UpdateError> {
    let mut response = client
        .get(url)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json, */*")
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .map_err(|_| UpdateError("Argentum could not reach GitHub.".into()))?;
    if !response.status().is_success() {
        return fail(format!(
            "GitHub answered with status {}.",
            response.status().as_u16()
        ));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| UpdateError("The download stopped early.".into()))?
    {
        if body.len() + chunk.len() > limit {
            return fail("The response was larger than expected.");
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

pub async fn check_latest() -> Result<Release, UpdateError> {
    let client = client()?;
    parse_release(&fetch_bounded(&client, RELEASES_URL, MAX_METADATA_BYTES).await?)
}

/// Find one file's digest in `sha256sum`-style text.
pub fn checksum_for(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let digest = parts.next()?;
        let name = parts.next()?.trim_start_matches('*');
        (name == file && digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit()))
            .then(|| digest.to_ascii_lowercase())
    })
}

/// Download into `directory` and return the file's path once its checksum matches.
/// On a mismatch or any failure the partial file is removed.
pub async fn download_verified(
    release: &Release,
    directory: &Path,
    mut progress: impl FnMut(u64, Option<u64>),
) -> Result<PathBuf, UpdateError> {
    let (Some(executable_url), Some(checksum_url)) = (
        release.executable_url.as_deref(),
        release.checksum_url.as_deref(),
    ) else {
        return fail("This release has no Windows update package.");
    };
    download_with(
        &client()?,
        &release.version,
        executable_url,
        checksum_url,
        directory,
        &mut progress,
    )
    .await
}

async fn download_with(
    client: &reqwest::Client,
    version: &str,
    executable_url: &str,
    checksum_url: &str,
    directory: &Path,
    progress: &mut impl FnMut(u64, Option<u64>),
) -> Result<PathBuf, UpdateError> {
    let sums = fetch_bounded(client, checksum_url, MAX_CHECKSUM_BYTES).await?;
    let expected = checksum_for(&String::from_utf8_lossy(&sums), EXECUTABLE_ASSET)
        .ok_or_else(|| UpdateError("This release does not list a checksum for the app.".into()))?;

    fs::create_dir_all(directory)
        .map_err(|_| UpdateError("The update folder could not be created.".into()))?;
    let partial = directory.join(format!("Argentum-{version}.partial"));
    let finished = directory.join(format!("Argentum-{version}.exe"));
    let result = async {
        let mut file = fs::File::create(&partial)
            .map_err(|_| UpdateError("The update file could not be created.".into()))?;
        let mut response = client
            .get(executable_url)
            .send()
            .await
            .map_err(|_| UpdateError("The download could not start.".into()))?;
        if !response.status().is_success() {
            return fail(format!(
                "The download answered with status {}.",
                response.status().as_u16()
            ));
        }
        let total = response.content_length();
        if total.is_some_and(|total| total > MAX_EXECUTABLE_BYTES) {
            return fail("The update is larger than Argentum accepts.");
        }
        let mut hasher = Sha256::new();
        let mut received = 0u64;
        while let Some(chunk) = response
            .chunk()
            .await
            .map_err(|_| UpdateError("The download stopped early.".into()))?
        {
            received += chunk.len() as u64;
            if received > MAX_EXECUTABLE_BYTES {
                return fail("The update is larger than Argentum accepts.");
            }
            hasher.update(&chunk);
            file.write_all(&chunk).map_err(|_| {
                UpdateError("The update could not be saved. Check free disk space.".into())
            })?;
            progress(received, total);
        }
        file.sync_all()
            .map_err(|_| UpdateError("The update could not be saved.".into()))?;
        drop(file);
        let actual = hex(&hasher.finalize());
        if actual != expected {
            return fail(
                "The download does not match its published checksum, so it was discarded.",
            );
        }
        fs::rename(&partial, &finished)
            .map_err(|_| UpdateError("The update could not be saved.".into()))?;
        Ok(finished.clone())
    }
    .await;
    if result.is_err() {
        let _ = fs::remove_file(&partial);
    }
    result
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Whether this process may replace its own executable.
pub fn install_blocker(executable: &Path) -> Option<&'static str> {
    if cfg!(debug_assertions) {
        return Some("This is a development build, so it does not replace itself.");
    }
    if !cfg!(windows) {
        return Some("Installing from the app is only available on Windows so far.");
    }
    let in_build_tree = executable
        .components()
        .any(|part| part.as_os_str().eq_ignore_ascii_case("target"));
    in_build_tree.then_some("This copy runs from a build folder, so it does not replace itself.")
}

/// Put `verified` where `executable` is. The running file can be renamed on Windows
/// but not overwritten, so it moves aside first and comes back if anything fails.
pub fn swap_executable(executable: &Path, verified: &Path) -> Result<(), UpdateError> {
    let staged = executable.with_extension("update-new");
    let previous = executable.with_extension("update-old");
    let _ = fs::remove_file(&previous);
    fs::copy(verified, &staged).map_err(|_| {
        UpdateError(
            "Argentum cannot write to its own folder, so the update was not installed.".into(),
        )
    })?;
    if fs::rename(executable, &previous).is_err() {
        let _ = fs::remove_file(&staged);
        return fail("The running app could not be moved aside, so nothing changed.");
    }
    if fs::rename(&staged, executable).is_err() {
        let _ = fs::rename(&previous, executable);
        let _ = fs::remove_file(&staged);
        return fail("The new version could not be put in place, so the old one was restored.");
    }
    Ok(())
}

/// Undo [`swap_executable`] when the new version could not be started.
pub fn restore_previous(executable: &Path) -> Result<(), UpdateError> {
    let previous = executable.with_extension("update-old");
    let _ = fs::remove_file(executable);
    fs::rename(&previous, executable)
        .map_err(|_| UpdateError("The previous version could not be restored.".into()))
}

/// Remove what the last update left behind. Called at startup, when the previous
/// executable is no longer running.
pub fn clean_after_update(executable: &Path) {
    let _ = fs::remove_file(executable.with_extension("update-old"));
    let _ = fs::remove_file(executable.with_extension("update-new"));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_numerically() {
        assert!(is_newer("0.1.1", "0.1.0"));
        assert!(is_newer("v0.10.0", "0.9.9"));
        assert!(is_newer("1.0.0", "1.0.0-rc.1"));
        assert!(!is_newer("0.0.9", "0.1.0"));
        assert!(!is_newer("0.1.0", "0.1.0"));
        assert!(!is_newer("1.0.0-rc.1", "1.0.0"));
        assert!(!is_newer("nonsense", "0.1.0"));
        assert!(!is_newer("1.2.3.4", "0.1.0"));
    }

    fn release_json(executable_url: &str) -> Vec<u8> {
        serde_json::json!({
            "tag_name": "v0.2.0",
            "body": "Notes",
            "html_url": "https://github.com/AG064/argentum/releases/tag/v0.2.0",
            "assets": [
                { "name": "Argentum.exe", "browser_download_url": executable_url },
                { "name": "SHA256SUMS.txt", "browser_download_url": "https://github.com/AG064/argentum/releases/download/v0.2.0/SHA256SUMS.txt" },
                { "name": "other.zip", "browser_download_url": "https://github.com/x" }
            ]
        })
        .to_string()
        .into_bytes()
    }

    #[test]
    fn release_parsing_finds_the_package() {
        let release = parse_release(&release_json(
            "https://github.com/AG064/argentum/releases/download/v0.2.0/Argentum.exe",
        ))
        .expect("release");
        assert_eq!(release.version, "0.2.0");
        assert!(release.executable_url.is_some());
        assert!(release.checksum_url.is_some());
    }

    #[test]
    fn assets_from_anywhere_else_are_ignored() {
        for url in [
            "http://github.com/AG064/argentum/Argentum.exe",
            "https://evil.example/Argentum.exe",
            "https://github.com.evil.example/Argentum.exe",
            "not a url",
        ] {
            let release = parse_release(&release_json(url)).expect("release");
            assert_eq!(release.executable_url, None, "{url}");
        }
    }

    #[test]
    fn malformed_metadata_is_an_error() {
        assert!(parse_release(b"{").is_err());
        assert!(parse_release(br#"{"tag_name":"latest"}"#).is_err());
    }

    #[test]
    fn checksums_must_be_well_formed_and_exact() {
        let digest = "a".repeat(64);
        let sums = format!(
            "{digest}  Argentum.exe\n{}  Other.exe\nshort  Bad.exe\n",
            "b".repeat(64)
        );
        assert_eq!(checksum_for(&sums, "Argentum.exe"), Some(digest.clone()));
        assert_eq!(
            checksum_for(&format!("{digest} *Argentum.exe"), "Argentum.exe"),
            Some(digest)
        );
        assert_eq!(checksum_for(&sums, "Bad.exe"), None);
        assert_eq!(checksum_for(&sums, "Argentum"), None);
    }

    #[tokio::test]
    async fn a_release_without_the_package_downloads_nothing() {
        let release = Release {
            version: "0.2.0".into(),
            notes: String::new(),
            page: String::new(),
            executable_url: None,
            checksum_url: None,
        };
        let directory =
            std::env::temp_dir().join(format!("argentum-update-{}", std::process::id()));
        let result = download_verified(&release, &directory, |_, _| {}).await;
        assert!(result.is_err());
        assert!(!directory.exists());
    }

    /// Answers each request by path, over a real socket, until the test ends.
    fn serve(routes: Vec<(&'static str, Vec<u8>)>) -> String {
        use std::io::Read;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
        let address = listener.local_addr().expect("address");
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { return };
                let mut request = [0u8; 2048];
                let read = stream.read(&mut request).unwrap_or(0);
                let head = String::from_utf8_lossy(&request[..read]).into_owned();
                let path = head.split_whitespace().nth(1).unwrap_or("/").to_owned();
                let body = routes
                    .iter()
                    .find(|(route, _)| *route == path)
                    .map(|(_, body)| body.clone());
                let (status, body) = match body {
                    Some(body) => ("200 OK", body),
                    None => ("404 Not Found", Vec::new()),
                };
                let _ = write!(
                    stream,
                    "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                );
                let _ = stream.write_all(&body);
            }
        });
        format!("http://{address}")
    }

    async fn download(base: &str, directory: &Path) -> Result<PathBuf, UpdateError> {
        download_with(
            &reqwest::Client::new(),
            "9.9.9",
            &format!("{base}/Argentum.exe"),
            &format!("{base}/SHA256SUMS.txt"),
            directory,
            &mut |_, _| {},
        )
        .await
    }

    #[tokio::test]
    async fn a_matching_download_is_kept() {
        let payload = b"new program".to_vec();
        let sums = format!("{}  Argentum.exe\n", hex(&Sha256::digest(&payload)));
        let base = serve(vec![
            ("/Argentum.exe", payload.clone()),
            ("/SHA256SUMS.txt", sums.into_bytes()),
        ]);
        let directory = scratch("match");
        let path = download(&base, &directory).await.expect("download");
        assert_eq!(fs::read(&path).expect("read"), payload);
        fs::remove_dir_all(&directory).expect("cleanup");
    }

    #[tokio::test]
    async fn a_tampered_download_is_discarded() {
        let sums = format!(
            "{}  Argentum.exe\n",
            hex(&Sha256::digest(b"the real program"))
        );
        let base = serve(vec![
            ("/Argentum.exe", b"something else".to_vec()),
            ("/SHA256SUMS.txt", sums.into_bytes()),
        ]);
        let directory = scratch("tampered");
        let error = download(&base, &directory).await.expect_err("mismatch");
        assert!(error.0.contains("checksum"), "{}", error.0);
        assert_eq!(fs::read_dir(&directory).expect("list").count(), 0);
        fs::remove_dir_all(&directory).expect("cleanup");
    }

    #[tokio::test]
    async fn a_release_with_no_listed_checksum_downloads_nothing() {
        let base = serve(vec![
            ("/Argentum.exe", b"program".to_vec()),
            ("/SHA256SUMS.txt", b"nothing useful here".to_vec()),
        ]);
        let directory = scratch("unlisted");
        assert!(download(&base, &directory).await.is_err());
        assert_eq!(fs::read_dir(&directory).expect("list").count(), 0);
        fs::remove_dir_all(&directory).expect("cleanup");
    }

    fn scratch(name: &str) -> PathBuf {
        let directory =
            std::env::temp_dir().join(format!("argentum-update-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).expect("scratch");
        directory
    }

    #[test]
    fn swap_replaces_the_executable_and_can_be_undone() {
        let directory = scratch("swap");
        let executable = directory.join("Argentum.exe");
        let verified = directory.join("verified.bin");
        fs::write(&executable, "old").expect("old");
        fs::write(&verified, "new").expect("new");

        swap_executable(&executable, &verified).expect("swap");
        assert_eq!(fs::read_to_string(&executable).expect("read"), "new");
        assert_eq!(
            fs::read_to_string(executable.with_extension("update-old")).expect("read"),
            "old"
        );

        restore_previous(&executable).expect("restore");
        assert_eq!(fs::read_to_string(&executable).expect("read"), "old");
        assert!(!executable.with_extension("update-old").exists());
        fs::remove_dir_all(&directory).expect("cleanup");
    }

    #[test]
    fn a_failed_swap_leaves_the_original_in_place() {
        let directory = scratch("fail");
        let executable = directory.join("Argentum.exe");
        fs::write(&executable, "old").expect("old");

        let missing = directory.join("does-not-exist");
        assert!(swap_executable(&executable, &missing).is_err());
        assert_eq!(fs::read_to_string(&executable).expect("read"), "old");
        assert!(!executable.with_extension("update-new").exists());
        fs::remove_dir_all(&directory).expect("cleanup");
    }

    #[test]
    fn cleanup_removes_only_update_leftovers() {
        let directory = scratch("clean");
        let executable = directory.join("Argentum.exe");
        fs::write(&executable, "current").expect("current");
        fs::write(executable.with_extension("update-old"), "x").expect("old");
        clean_after_update(&executable);
        assert!(executable.exists());
        assert!(!executable.with_extension("update-old").exists());
        fs::remove_dir_all(&directory).expect("cleanup");
    }

    #[test]
    fn build_folders_never_replace_themselves() {
        assert!(install_blocker(Path::new(r"C:\repo\target\release\argentum.exe")).is_some());
    }
}
