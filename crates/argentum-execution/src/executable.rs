use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

pub(crate) fn resolve_fixed_executable(
    program: &str,
    untrusted_roots: &[&Path],
) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    let entries = std::env::split_paths(&path).collect::<Vec<_>>();
    let current = std::env::current_dir()
        .ok()
        .and_then(|directory| std::fs::canonicalize(directory).ok());
    resolve_fixed_executable_from_paths(program, &entries, untrusted_roots, current.as_deref())
}

pub(crate) fn resolve_fixed_executable_from_paths(
    program: &str,
    entries: &[PathBuf],
    untrusted_roots: &[&Path],
    current: Option<&Path>,
) -> Option<PathBuf> {
    let names = executable_names(program)?;
    let roots = untrusted_roots
        .iter()
        .filter_map(|root| std::fs::canonicalize(root).ok())
        .collect::<Vec<_>>();
    let current = current.and_then(|path| std::fs::canonicalize(path).ok());

    for entry in entries {
        if !entry.is_absolute() {
            continue;
        }
        let Ok(entry) = std::fs::canonicalize(entry) else {
            continue;
        };
        if is_untrusted(&entry, &roots, current.as_deref()) {
            continue;
        }
        for name in &names {
            let launch_path = entry.join(name);
            let Ok(candidate) = std::fs::canonicalize(&launch_path) else {
                continue;
            };
            if candidate.is_file()
                && is_executable(&candidate)
                && !is_untrusted(&candidate, &roots, current.as_deref())
            {
                // Keep the validated absolute alias as the launch path. Windows
                // rustup proxies select cargo or rustc from argv[0]; replacing
                // cargo.exe with its canonical rustup.exe target changes the
                // command and makes fixed Cargo runners unusable.
                return Some(launch_path);
            }
        }
    }
    None
}

fn executable_names(program: &str) -> Option<Vec<OsString>> {
    let path = Path::new(program);
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
    {
        return None;
    }
    #[cfg(windows)]
    {
        if path.extension().is_some() {
            Some(vec![OsString::from(program)])
        } else {
            Some(vec![
                OsString::from(format!("{program}.exe")),
                OsString::from(format!("{program}.com")),
            ])
        }
    }
    #[cfg(not(windows))]
    {
        Some(vec![OsString::from(program)])
    }
}

fn is_untrusted(path: &Path, roots: &[PathBuf], current: Option<&Path>) -> bool {
    roots.iter().any(|root| path_is_within(path, root))
        || current.is_some_and(|current| path_is_within(path, current))
}

#[cfg(windows)]
fn path_is_within(path: &Path, root: &Path) -> bool {
    let path = path.to_string_lossy().replace('/', "\\").to_lowercase();
    let root = root
        .to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase();
    path == root || path.starts_with(&format!("{root}\\"))
}

#[cfg(not(windows))]
fn path_is_within(path: &Path, root: &Path) -> bool {
    path == root || path.starts_with(root)
}

#[cfg(windows)]
fn is_executable(_path: &Path) -> bool {
    true
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.metadata()
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(any(windows, unix)))]
fn is_executable(_path: &Path) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn resolver_preserves_the_fixed_cargo_launch_alias() {
        let resolved = resolve_fixed_executable("cargo", &[]).expect("Cargo executable");
        let name = resolved
            .file_name()
            .and_then(|value| value.to_str())
            .expect("Cargo executable name")
            .to_ascii_lowercase();
        #[cfg(windows)]
        assert!(matches!(name.as_str(), "cargo.exe" | "cargo.com"));
        #[cfg(not(windows))]
        assert_eq!(name, "cargo");
    }

    #[test]
    fn resolver_skips_relative_workspace_and_launch_entries() {
        let root = tempfile::tempdir().expect("resolution root");
        let workspace = root.path().join("workspace");
        let launch = root.path().join("launch");
        let trusted = root.path().join("trusted");
        for directory in [&workspace, &launch, &trusted] {
            fs::create_dir_all(directory).expect("candidate directory");
        }
        #[cfg(windows)]
        let executable_name = "fixed-tool.exe";
        #[cfg(not(windows))]
        let executable_name = "fixed-tool";
        for directory in [&workspace, &launch, &trusted] {
            let candidate = directory.join(executable_name);
            fs::write(&candidate, "fixture").expect("candidate file");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut permissions = fs::metadata(&candidate).expect("metadata").permissions();
                permissions.set_mode(0o700);
                fs::set_permissions(&candidate, permissions).expect("executable permission");
            }
        }

        let resolved = resolve_fixed_executable_from_paths(
            "fixed-tool",
            &[
                PathBuf::from("."),
                workspace.clone(),
                launch.clone(),
                trusted.clone(),
            ],
            &[&workspace],
            Some(&launch),
        )
        .expect("trusted executable");

        assert_eq!(
            resolved,
            fs::canonicalize(trusted.join(executable_name)).expect("trusted canonical path")
        );
    }
}
