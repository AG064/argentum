//! Read-only working tree status and diff through the `git` executable.
//!
//! Running Git is an external process, so the runtime only calls this crate after
//! checking the project's external-process authority, and only in response to a
//! direct user action. Every invocation disables the repository-controlled
//! helpers Git can be told to skip: fsmonitor, external diff, and textconv. Filter
//! drivers in a repository's own config can still run during `git diff`, as they
//! would from a terminal. That is why this stays behind the authority key.

use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use argentum_domain::{GitDiffSnapshot, GitFileStatus, GitStatusSnapshot, GitViewState};

const MAX_OUTPUT_BYTES: usize = 2 * 1024 * 1024;
const MAX_FILES: usize = 1000;
const TIMEOUT: Duration = Duration::from_secs(15);

enum GitFailure {
    NotFound,
    TimedOut,
    Spawn,
}

struct GitOutput {
    code: Option<i32>,
    stdout: Vec<u8>,
    stderr: String,
    truncated: bool,
}

fn run_git(workspace: &Path, arguments: &[&str]) -> Result<GitOutput, GitFailure> {
    let mut command = Command::new("git");
    command
        .current_dir(workspace)
        .args([
            "--no-optional-locks",
            "--literal-pathspecs",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "diff.external=",
            "-c",
            "core.quotePath=false",
            "-c",
            "color.ui=false",
        ])
        .args(arguments)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .env_remove("GIT_INDEX_FILE")
        .env_remove("GIT_EXTERNAL_DIFF")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            GitFailure::NotFound
        } else {
            GitFailure::Spawn
        }
    })?;
    let stdout = child.stdout.take().ok_or(GitFailure::Spawn)?;
    let stderr = child.stderr.take().ok_or(GitFailure::Spawn)?;
    let (stdout_sender, stdout_receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let read = stdout
            .take(MAX_OUTPUT_BYTES as u64 + 1)
            .read_to_end(&mut buffer);
        let _ = stdout_sender.send((buffer, read.is_ok()));
    });
    let (stderr_sender, stderr_receiver) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stderr.take(16 * 1024).read_to_end(&mut buffer);
        let _ = stderr_sender.send(buffer);
    });

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() > TIMEOUT => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(GitFailure::TimedOut);
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(15)),
            Err(_) => return Err(GitFailure::Spawn),
        }
    };
    let (mut stdout, read_ok) = stdout_receiver
        .recv_timeout(Duration::from_secs(2))
        .map_err(|_| GitFailure::TimedOut)?;
    let stderr = stderr_receiver
        .recv_timeout(Duration::from_secs(2))
        .unwrap_or_default();
    if !read_ok {
        return Err(GitFailure::Spawn);
    }
    let truncated = stdout.len() > MAX_OUTPUT_BYTES;
    stdout.truncate(MAX_OUTPUT_BYTES);
    Ok(GitOutput {
        code: status.code(),
        stdout,
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        truncated,
    })
}

fn failure_message(failure: &GitFailure) -> String {
    match failure {
        GitFailure::NotFound => "Git was not found on this computer's PATH.".to_owned(),
        GitFailure::TimedOut => "Git did not finish within 15 seconds.".to_owned(),
        GitFailure::Spawn => "Git could not be started.".to_owned(),
    }
}

fn first_line(text: &str) -> String {
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("Git reported an error.")
        .chars()
        .take(240)
        .collect()
}

fn is_not_repository(output: &GitOutput) -> bool {
    output.code == Some(128)
        && output
            .stderr
            .to_ascii_lowercase()
            .contains("not a git repository")
}

/// Parse `git status --porcelain=v1 -z --branch`.
pub fn parse_status(output: &[u8]) -> (String, Vec<GitFileStatus>, bool) {
    let text = String::from_utf8_lossy(output);
    let mut branch = String::new();
    let mut files = Vec::new();
    let mut truncated = false;
    let mut entries = text.split('\0');
    while let Some(entry) = entries.next() {
        if entry.is_empty() {
            continue;
        }
        if let Some(rest) = entry.strip_prefix("## ") {
            branch = rest
                .split("...")
                .next()
                .unwrap_or_default()
                .trim_start_matches("No commits yet on ")
                .to_owned();
            continue;
        }
        if entry.len() < 4 || !entry.is_char_boundary(3) {
            continue;
        }
        let (code, path) = entry.split_at(3);
        let mut letters = code.chars();
        let index = letters.next().unwrap_or(' ');
        let worktree = letters.next().unwrap_or(' ');
        if matches!(index, 'R' | 'C') {
            // The source path of a rename follows as its own entry.
            let _ = entries.next();
        }
        let status = match (index, worktree) {
            ('?', '?') => "?",
            ('U', _) | (_, 'U') | ('A', 'A') | ('D', 'D') => "U",
            ('R', _) | (_, 'R') | ('C', _) => "R",
            ('A', _) => "A",
            ('D', _) | (_, 'D') => "D",
            _ => "M",
        };
        if files.len() >= MAX_FILES {
            truncated = true;
            break;
        }
        files.push(GitFileStatus {
            path: path.to_owned(),
            status: status.to_owned(),
        });
    }
    (branch, files, truncated)
}

pub fn status(workspace: &Path) -> GitStatusSnapshot {
    let failed = |state, message: String| GitStatusSnapshot {
        state,
        message,
        branch: String::new(),
        files: Vec::new(),
        truncated: false,
    };
    let output = match run_git(
        workspace,
        &[
            "status",
            "--porcelain=v1",
            "-z",
            "--branch",
            "--untracked-files=normal",
        ],
    ) {
        Ok(output) => output,
        Err(failure) => return failed(GitViewState::Failed, failure_message(&failure)),
    };
    if is_not_repository(&output) {
        return failed(
            GitViewState::NotRepository,
            "This workspace is not inside a Git repository.".to_owned(),
        );
    }
    if output.code != Some(0) {
        return failed(GitViewState::Failed, first_line(&output.stderr));
    }
    let (branch, files, truncated) = parse_status(&output.stdout);
    GitStatusSnapshot {
        state: GitViewState::Ready,
        message: String::new(),
        branch,
        files,
        truncated: truncated || output.truncated,
    }
}

/// Diff one path from the last status. `untracked` files compare against nothing.
pub fn file_diff(workspace: &Path, path: &str, untracked: bool) -> GitDiffSnapshot {
    let snapshot = |state, message: String, diff: String, truncated| GitDiffSnapshot {
        state,
        message,
        path: path.to_owned(),
        diff,
        truncated,
    };
    let common = ["--no-ext-diff", "--no-textconv", "--no-color"];
    let null_device = if cfg!(windows) { "NUL" } else { "/dev/null" };
    let mut arguments = vec!["diff"];
    arguments.extend(common);
    if untracked {
        arguments.extend(["--no-index", "--", null_device, path]);
    } else {
        arguments.extend(["--no-renames", "HEAD", "--", path]);
    }
    let mut output = match run_git(workspace, &arguments) {
        Ok(output) => output,
        Err(failure) => {
            return snapshot(
                GitViewState::Failed,
                failure_message(&failure),
                String::new(),
                false,
            )
        }
    };
    if !untracked && output.code == Some(128) {
        // A repository with no commits has no HEAD to compare against.
        let mut staged = vec!["diff"];
        staged.extend(common);
        staged.extend(["--no-renames", "--cached", "--", path]);
        output = match run_git(workspace, &staged) {
            Ok(output) => output,
            Err(failure) => {
                return snapshot(
                    GitViewState::Failed,
                    failure_message(&failure),
                    String::new(),
                    false,
                )
            }
        };
    }
    // `--no-index` exits 1 when the files differ, which is the expected case.
    if !matches!(output.code, Some(0) | Some(1)) {
        return snapshot(
            GitViewState::Failed,
            first_line(&output.stderr),
            String::new(),
            false,
        );
    }
    snapshot(
        GitViewState::Ready,
        String::new(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        output.truncated,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_branch_changes_renames_and_untracked() {
        let raw = b"## main...origin/main [ahead 1]\0 M src/a.rs\0A  new.rs\0R  to.rs\0from.rs\0?? scratch.txt\0 D gone.rs\0UU both.rs\0";
        let (branch, files, truncated) = parse_status(raw);
        assert_eq!(branch, "main");
        assert!(!truncated);
        let summary = files
            .iter()
            .map(|file| format!("{} {}", file.status, file.path))
            .collect::<Vec<_>>();
        assert_eq!(
            summary,
            [
                "M src/a.rs",
                "A new.rs",
                "R to.rs",
                "? scratch.txt",
                "D gone.rs",
                "U both.rs"
            ]
        );
    }

    #[test]
    fn parses_unborn_branch() {
        let (branch, files, _) = parse_status(b"## No commits yet on main\0?? a\0");
        assert_eq!(branch, "main");
        assert_eq!(files.len(), 1);
    }

    struct Repo(std::path::PathBuf);

    impl Repo {
        /// `None` when Git is not installed, since there is nothing to exercise then.
        fn new(name: &str) -> Option<Self> {
            let root = std::env::temp_dir()
                .join(format!("argentum-gitview-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).expect("repo directory");
            let repo = Self(root);
            repo.git(&["init", "-q"])?;
            repo.git(&["config", "user.email", "test@example.invalid"])?;
            repo.git(&["config", "user.name", "Test"])?;
            repo.git(&["config", "core.autocrlf", "false"])?;
            Some(repo)
        }

        fn git(&self, arguments: &[&str]) -> Option<()> {
            let status = Command::new("git")
                .current_dir(&self.0)
                .args(arguments)
                .status()
                .ok()?;
            status.success().then_some(())
        }

        fn write(&self, path: &str, text: &str) {
            std::fs::write(self.0.join(path), text).expect("fixture");
        }
    }

    impl Drop for Repo {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn real_repository_status_and_diffs() {
        let Some(repo) = Repo::new("status") else {
            return;
        };
        repo.write("tracked.txt", "one\n");
        repo.git(&["add", "."]).expect("add");
        repo.git(&["commit", "-qm", "first"]).expect("commit");
        repo.write("tracked.txt", "one\ntwo\n");
        repo.write("new.txt", "fresh\n");

        let snapshot = status(&repo.0);
        assert_eq!(snapshot.state, GitViewState::Ready, "{}", snapshot.message);
        assert!(!snapshot.branch.is_empty());
        let listed = snapshot
            .files
            .iter()
            .map(|file| format!("{} {}", file.status, file.path))
            .collect::<Vec<_>>();
        assert_eq!(listed, ["M tracked.txt", "? new.txt"]);

        let tracked = file_diff(&repo.0, "tracked.txt", false);
        assert_eq!(tracked.state, GitViewState::Ready, "{}", tracked.message);
        assert!(tracked.diff.contains("+two"), "{}", tracked.diff);

        let untracked = file_diff(&repo.0, "new.txt", true);
        assert_eq!(
            untracked.state,
            GitViewState::Ready,
            "{}",
            untracked.message
        );
        assert!(untracked.diff.contains("+fresh"), "{}", untracked.diff);
    }

    #[test]
    fn a_path_that_looks_like_an_option_stays_a_path() {
        let Some(repo) = Repo::new("option") else {
            return;
        };
        repo.write("first.txt", "x\n");
        repo.git(&["add", "."]).expect("add");
        repo.git(&["commit", "-qm", "first"]).expect("commit");
        repo.write("--output=escaped.txt", "y\n");

        let diff = file_diff(&repo.0, "--output=escaped.txt", true);
        assert_eq!(diff.state, GitViewState::Ready, "{}", diff.message);
        assert!(diff.diff.contains("+y"), "{}", diff.diff);
        assert!(!repo.0.join("escaped.txt").exists());
    }

    #[test]
    fn a_folder_outside_any_repository_says_so() {
        // Git is not needed to set this up, but the answer comes from it.
        let root =
            std::env::temp_dir().join(format!("argentum-gitview-plain-{}", std::process::id()));
        std::fs::create_dir_all(&root).expect("directory");
        let snapshot = status(&root);
        let _ = std::fs::remove_dir_all(&root);
        // The temp folder could sit inside someone's repository, and Git could be
        // missing. Neither may be reported as a clean working tree of this folder.
        if snapshot.state != GitViewState::Ready {
            assert!(!snapshot.message.is_empty());
            assert!(snapshot.files.is_empty());
        }
    }

    #[test]
    fn caps_the_file_list() {
        let mut raw = Vec::new();
        for index in 0..(MAX_FILES + 5) {
            raw.extend_from_slice(format!(" M f{index}\0").as_bytes());
        }
        let (_, files, truncated) = parse_status(&raw);
        assert_eq!(files.len(), MAX_FILES);
        assert!(truncated);
    }
}
