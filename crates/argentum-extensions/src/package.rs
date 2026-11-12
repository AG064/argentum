use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const PLUGIN_SCHEMA: &str = "https://agent-plugins.org/schemas/1.0.0/plugin.schema.json";
pub const MCP_SCHEMA: &str = "https://agent-plugins.org/schemas/1.0.0/mcp.schema.json";
const MAX_FILES: usize = 1024;
const MAX_PACKAGE_BYTES: u64 = 20 * 1024 * 1024;
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum PackageError {
    #[error("extension package I/O failed")]
    Io(#[from] std::io::Error),
    #[error("invalid extension manifest JSON")]
    Json(#[from] serde_json::Error),
    #[error("extension package rejected: {0}")]
    Invalid(&'static str),
    #[error("extension integrity changed; inspect and reinstall this revision")]
    Integrity,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    #[serde(rename = "$schema")]
    pub schema: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub license: String,
    #[serde(default)]
    pub author: Option<Value>,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub extensions: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct McpManifest {
    #[serde(rename = "$schema")]
    pub schema: String,
    #[serde(rename = "mcpServers")]
    pub servers: BTreeMap<String, ServerDeclaration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", deny_unknown_fields)]
pub enum ServerDeclaration {
    #[serde(rename = "stdio")]
    Stdio {
        command: String,
        #[serde(default)]
        args: Vec<String>,
        #[serde(default)]
        env: BTreeMap<String, String>,
    },
    #[serde(rename = "streamable-http")]
    Http { url: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageInspection {
    pub manifest: Manifest,
    pub digest: String,
    pub files: BTreeMap<String, String>,
    pub bytes: u64,
    pub skill_paths: Vec<String>,
    pub servers: BTreeMap<String, ServerDeclaration>,
    pub unsupported_host_components: Vec<String>,
}

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && value.as_bytes()[0].is_ascii_alphanumeric()
}

pub fn relative_path(value: &str) -> Result<PathBuf, PackageError> {
    let path = Path::new(value);
    if value.is_empty()
        || value.contains('\\')
        || value.contains(':')
        || path.is_absolute()
        || value.split('/').any(|part| {
            part.is_empty()
                || part.ends_with(['.', ' '])
                || part.chars().any(char::is_control)
                || part.contains(['<', '>', '"', '|', '?', '*'])
                || matches!(
                    part.split('.')
                        .next()
                        .unwrap_or_default()
                        .to_ascii_uppercase()
                        .as_str(),
                    "CON"
                        | "PRN"
                        | "AUX"
                        | "NUL"
                        | "COM1"
                        | "COM2"
                        | "COM3"
                        | "COM4"
                        | "COM5"
                        | "COM6"
                        | "COM7"
                        | "COM8"
                        | "COM9"
                        | "LPT1"
                        | "LPT2"
                        | "LPT3"
                        | "LPT4"
                        | "LPT5"
                        | "LPT6"
                        | "LPT7"
                        | "LPT8"
                        | "LPT9"
                )
        })
        || path
            .components()
            .any(|c| !matches!(c, Component::Normal(_)))
    {
        return Err(PackageError::Invalid(
            "package paths must be relative and contain no traversal, drive, or backslash",
        ));
    }
    Ok(path.to_path_buf())
}

fn regular_metadata(path: &Path) -> Result<fs::Metadata, PackageError> {
    let metadata = fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() {
        return Err(PackageError::Invalid(
            "symlink and reparse components are not supported",
        ));
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(PackageError::Invalid(
                "reparse components are not supported",
            ));
        }
    }
    Ok(metadata)
}

fn inventory(root: &Path) -> Result<(BTreeMap<String, Vec<u8>>, u64), PackageError> {
    let mut pending = vec![root.to_path_buf()];
    let mut files = BTreeMap::new();
    let mut total = 0_u64;
    let mut components = 0;
    while let Some(dir) = pending.pop() {
        if !regular_metadata(&dir)?.is_dir() {
            return Err(PackageError::Invalid("package root must be a directory"));
        }
        for item in fs::read_dir(&dir)? {
            let item = item?;
            components += 1;
            if components > MAX_FILES {
                return Err(PackageError::Invalid("package component limit exceeded"));
            }
            let path = item.path();
            let metadata = regular_metadata(&path)?;
            let component = item.file_name();
            let component = component
                .to_str()
                .ok_or(PackageError::Invalid("package paths must be UTF-8"))?;
            relative_path(component)?;
            if component.starts_with('.') {
                return Err(PackageError::Invalid(
                    "hidden compatibility overlays and stores are not portable components",
                ));
            }
            if metadata.is_dir() {
                pending.push(path);
                continue;
            }
            if !metadata.is_file() || metadata.len() > MAX_FILE_BYTES {
                return Err(PackageError::Invalid(
                    "package files must be bounded regular files",
                ));
            }
            total = total
                .checked_add(metadata.len())
                .ok_or(PackageError::Invalid("package size overflow"))?;
            if total > MAX_PACKAGE_BYTES {
                return Err(PackageError::Invalid("package byte limit exceeded"));
            }
            let name = path
                .strip_prefix(root)
                .map_err(|_| PackageError::Invalid("package path escaped"))?
                .to_str()
                .ok_or(PackageError::Invalid("package paths must be UTF-8"))?
                .replace('\\', "/");
            relative_path(&name)?;
            if name.split('/').any(|part| part.starts_with('.')) {
                return Err(PackageError::Invalid(
                    "hidden files, stores and compatibility overlays are not portable components",
                ));
            }
            use std::io::Read;
            let mut bytes = Vec::new();
            fs::File::open(&path)?
                .take(MAX_FILE_BYTES + 1)
                .read_to_end(&mut bytes)?;
            if bytes.len() as u64 != metadata.len() {
                return Err(PackageError::Integrity);
            }
            files.insert(name, bytes);
        }
    }
    Ok((files, total))
}

pub fn inspect(root: &Path) -> Result<PackageInspection, PackageError> {
    let (contents, bytes) = inventory(root)?;
    inspection(&contents, bytes)
}

fn inspection(
    contents: &BTreeMap<String, Vec<u8>>,
    bytes: u64,
) -> Result<PackageInspection, PackageError> {
    let manifest: Manifest =
        serde_json::from_slice(contents.get("plugin.json").ok_or(PackageError::Invalid(
            "portable root plugin.json is required; .codex-plugin alone is unsupported",
        ))?)?;
    if manifest.schema != PLUGIN_SCHEMA
        || !valid_id(&manifest.name)
        || manifest.version.is_empty()
        || manifest.version.len() > 64
        || manifest.description.len() > 2048
        || manifest.license.is_empty()
        || manifest.license.len() > 128
        || !contents.contains_key("LICENSE")
    {
        return Err(PackageError::Invalid(
            "schema 1.0.0, bounded identity, declared license and LICENSE file are required",
        ));
    }
    let mut unsupported = Vec::new();
    for (host, declarations) in &manifest.extensions {
        let fields = declarations
            .as_object()
            .ok_or(PackageError::Invalid("host extensions must be objects"))?;
        for field in fields.keys() {
            unsupported.push(format!("{host}.{field}"));
        }
    }
    let servers = match contents.get("mcp.json") {
        None => BTreeMap::new(),
        Some(body) => {
            let mcp: McpManifest = serde_json::from_slice(body)?;
            if mcp.schema != MCP_SCHEMA
                || mcp.servers.len() > 16
                || mcp.servers.keys().any(|s| !valid_id(s))
            {
                return Err(PackageError::Invalid(
                    "MCP schema 1.0.0 and at most 16 named servers are supported",
                ));
            }
            for server in mcp.servers.values() {
                match server {
                    ServerDeclaration::Stdio { command, args, env } => {
                        if command.is_empty()
                            || command.len() > 4096
                            || args.len() > 32
                            || args
                                .iter()
                                .any(|a| a.len() > 4096 || a.chars().any(char::is_control))
                            || !env.is_empty()
                        {
                            return Err(PackageError::Invalid("stdio commands need explicit review; package-provided environment is unsupported"));
                        }
                    }
                    ServerDeclaration::Http { url } => {
                        let url = url::Url::parse(url)
                            .map_err(|_| PackageError::Invalid("invalid MCP URL"))?;
                        if !matches!(url.scheme(), "http" | "https")
                            || !url.username().is_empty()
                            || url.password().is_some()
                            || url.fragment().is_some()
                            || url.query().is_some()
                        {
                            return Err(PackageError::Invalid("MCP URL must use HTTP(S) and contain no embedded credentials, query or fragment"));
                        }
                    }
                }
            }
            mcp.servers
        }
    };
    let files: BTreeMap<_, _> = contents
        .iter()
        .map(|(name, body)| (name.clone(), format!("{:x}", Sha256::digest(body))))
        .collect();
    let mut digest = Sha256::new();
    digest.update(b"argentum-portable-package/v1\0");
    for (name, hash) in &files {
        digest.update((name.len() as u64).to_le_bytes());
        digest.update(name.as_bytes());
        digest.update(hash.as_bytes());
    }
    let skill_paths: Vec<String> = files
        .keys()
        .filter(|p| p.starts_with("skills/") && p.ends_with("/SKILL.md"))
        .cloned()
        .collect();
    if skill_paths.len() > 8
        || skill_paths.iter().any(|path| {
            path.len() > 128
                || contents[path].len() > 32 * 1024
                || std::str::from_utf8(&contents[path]).is_err()
        })
    {
        return Err(PackageError::Invalid("supported package profile has at most eight UTF-8 skills, 128-byte relative paths and 32 KiB per body"));
    }
    Ok(PackageInspection {
        manifest,
        digest: format!("{:x}", digest.finalize()),
        files,
        bytes,
        skill_paths,
        servers,
        unsupported_host_components: unsupported,
    })
}

pub fn install(
    source: &Path,
    destination_root: &Path,
    expected_digest: &str,
) -> Result<PackageInspection, PackageError> {
    let (contents, bytes) = inventory(source)?;
    let inspected = inspection(&contents, bytes)?;
    if inspected.digest != expected_digest {
        return Err(PackageError::Integrity);
    }
    fs::create_dir_all(destination_root)?;
    regular_metadata(destination_root)?;
    let destination = destination_root.join(&inspected.digest);
    if destination.exists() {
        if inspect(&destination)?.digest != inspected.digest {
            return Err(PackageError::Integrity);
        }
        return Ok(inspected);
    }
    let staging = destination_root.join(format!("pending-{}", uuid::Uuid::new_v4()));
    fs::create_dir(&staging)?;
    for (name, body) in &contents {
        let path = staging.join(relative_path(name)?);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        use std::io::Write;
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)?;
        file.write_all(body)?;
        file.sync_all()?;
    }
    if inspect(&staging)?.digest != inspected.digest {
        return Err(PackageError::Integrity);
    }
    fs::rename(&staging, &destination)?;
    Ok(inspected)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn package() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("plugin.json"), serde_json::to_vec(&serde_json::json!({"$schema":PLUGIN_SCHEMA,"name":"sample","version":"1.0.0","description":"Synthetic portable package","license":"MIT"})).unwrap()).unwrap();
        fs::write(dir.path().join("LICENSE"), "Synthetic test license").unwrap();
        dir
    }
    #[test]
    fn immutable_install_is_not_execution() {
        let source = package();
        let store = tempfile::tempdir().unwrap();
        let inspected = inspect(source.path()).unwrap();
        let installed = install(source.path(), store.path(), &inspected.digest).unwrap();
        assert_eq!(
            inspect(&store.path().join(&installed.digest))
                .unwrap()
                .digest,
            installed.digest
        );
        fs::write(source.path().join("LICENSE"), "changed").unwrap();
        assert!(matches!(
            install(source.path(), store.path(), &installed.digest),
            Err(PackageError::Integrity)
        ));
        assert!(!store.path().join("approval.json").exists());
    }
    #[test]
    fn traversal_and_drive_paths_are_rejected() {
        for path in [
            "../outside",
            "/outside",
            "C:relative",
            "C:/outside",
            "a\\b",
            "",
            "a//b",
            "a/../b",
            "a/file.",
            "NUL.txt",
            "a/file ",
        ] {
            assert!(relative_path(path).is_err(), "{path}");
        }
        assert!(relative_path("skills/sample/SKILL.md").is_ok());
    }
    #[test]
    fn installed_byte_change_is_visible() {
        let source = package();
        let store = tempfile::tempdir().unwrap();
        let before = inspect(source.path()).unwrap();
        install(source.path(), store.path(), &before.digest).unwrap();
        fs::write(
            store.path().join(&before.digest).join("LICENSE"),
            "tampered",
        )
        .unwrap();
        assert!(matches!(
            install(source.path(), store.path(), &before.digest),
            Err(PackageError::Integrity)
        ));
    }
    #[test]
    fn legacy_layout_and_implicit_environment_fail_closed() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join(".codex-plugin")).unwrap();
        assert!(inspect(dir.path()).is_err());
        let dir = package();
        fs::create_dir(dir.path().join(".codex-plugin")).unwrap();
        assert!(inspect(dir.path()).is_err());
        let dir = package();
        fs::write(dir.path().join("mcp.json"), serde_json::to_vec(&serde_json::json!({"$schema":MCP_SCHEMA,"mcpServers":{"server":{"type":"stdio","command":"node","env":{"PATH":"arbitrary"}}}})).unwrap()).unwrap();
        assert!(inspect(dir.path()).is_err());
    }
    #[test]
    fn host_specific_fields_are_explicit_not_portable_claims() {
        let dir = package();
        let file = dir.path().join("plugin.json");
        let mut value: Value = serde_json::from_slice(&fs::read(&file).unwrap()).unwrap();
        value["extensions"] = serde_json::json!({"com.openai":{"hooks":"./hooks/hooks.json","interface":{"displayName":"Sample"}}});
        fs::write(file, serde_json::to_vec(&value).unwrap()).unwrap();
        assert_eq!(
            inspect(dir.path()).unwrap().unsupported_host_components,
            vec!["com.openai.hooks", "com.openai.interface"]
        );
    }
}
