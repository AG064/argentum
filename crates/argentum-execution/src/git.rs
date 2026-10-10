use std::collections::BTreeMap;
use std::path::{Component, Path};

use argentum_domain::GitOperation;
use argentum_security::SecurityError;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

use crate::executable::resolve_fixed_executable;
use crate::{ProcessError, ProcessResult, ProcessRunner, ProcessSpec};

const MAX_GIT_PATHS: usize = 128;
const MAX_GIT_PATH_BYTES: usize = 4096;
const MAX_GIT_REF_BYTES: usize = 256;
const MAX_COMMIT_MESSAGE_BYTES: usize = 4096;
const MAX_LOG_COUNT: u16 = 200;

#[derive(Debug, Error)]
pub enum GitError {
    #[error(transparent)]
    Security(#[from] SecurityError),
    #[error(transparent)]
    Process(#[from] ProcessError),
    #[error("Git branch or revision is invalid")]
    InvalidReference,
    #[error("Git path is invalid or outside the operation boundary")]
    InvalidPath,
    #[error("Git path count exceeds the operation limit")]
    PathLimit,
    #[error("Git log count must be from 1 through 200")]
    InvalidLogCount,
    #[error("Git commit message is empty, invalid, or exceeds its bound")]
    InvalidCommitMessage,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct GitExecutor {
    runner: ProcessRunner,
}

impl GitExecutor {
    pub fn spec_for(
        operation: &GitOperation,
        repository: impl AsRef<Path>,
    ) -> Result<ProcessSpec, GitError> {
        let repository = repository.as_ref();
        let executable = resolve_fixed_executable("git", &[repository])
            .ok_or_else(|| ProcessError::FixedExecutableUnavailable("git".into()))?;
        let mut args = vec!["-c".into(), "core.fsmonitor=false".into()];
        match operation {
            GitOperation::Status { short } => {
                args.push("status".into());
                if *short {
                    args.push("--short".into());
                }
                args.push("--branch".into());
                args.push("--untracked-files=all".into());
            }
            GitOperation::Diff { cached, path } => {
                args.extend([
                    "diff".into(),
                    "--no-ext-diff".into(),
                    "--no-textconv".into(),
                    "--binary".into(),
                ]);
                if *cached {
                    args.push("--cached".into());
                }
                if let Some(path) = path {
                    validate_git_path(path)?;
                    args.extend(["--".into(), path.clone()]);
                }
            }
            GitOperation::Log { max_count } => {
                if *max_count == 0 || *max_count > MAX_LOG_COUNT {
                    return Err(GitError::InvalidLogCount);
                }
                args.extend([
                    "log".into(),
                    "--no-decorate".into(),
                    "--date=iso-strict".into(),
                    "--oneline".into(),
                    "-n".into(),
                    max_count.to_string(),
                ]);
            }
            GitOperation::Show { revision, path } => {
                validate_git_reference(revision)?;
                args.extend([
                    "show".into(),
                    "--no-ext-diff".into(),
                    "--no-textconv".into(),
                    "--format=fuller".into(),
                    revision.clone(),
                ]);
                if let Some(path) = path {
                    validate_git_path(path)?;
                    args.extend(["--".into(), path.clone()]);
                }
            }
            GitOperation::BranchList => {
                args.extend(["branch".into(), "--list".into(), "--no-color".into()]);
            }
            GitOperation::BranchCreate { name, start_point } => {
                validate_branch_name(name)?;
                args.extend(["branch".into(), name.clone()]);
                if let Some(start_point) = start_point {
                    validate_git_reference(start_point)?;
                    args.push(start_point.clone());
                }
            }
            GitOperation::BranchSwitch { name } => {
                validate_branch_name(name)?;
                args.extend(["switch".into(), name.clone()]);
            }
            GitOperation::WorktreeList => {
                args.extend(["worktree".into(), "list".into(), "--porcelain".into()]);
            }
            GitOperation::WorktreeCreate {
                path,
                branch,
                create_branch,
            } => {
                validate_worktree_path(path)?;
                validate_branch_name(branch)?;
                args.extend(["worktree".into(), "add".into()]);
                if *create_branch {
                    args.extend(["-b".into(), branch.clone(), path.clone()]);
                } else {
                    args.extend([path.clone(), branch.clone()]);
                }
            }
            GitOperation::Add { paths } => {
                validate_git_paths(paths)?;
                args.extend(["add".into(), "--".into()]);
                args.extend(paths.iter().cloned());
            }
            GitOperation::Commit { message } => {
                if message.trim().is_empty()
                    || message.len() > MAX_COMMIT_MESSAGE_BYTES
                    || message.chars().any(char::is_control)
                {
                    return Err(GitError::InvalidCommitMessage);
                }
                argentum_security::validate_process_argument(message)?;
                args.extend(["commit".into(), "-m".into(), message.clone()]);
            }
            GitOperation::Restore { path, source } => {
                validate_git_path(path)?;
                args.push("restore".into());
                if let Some(source) = source {
                    validate_git_reference(source)?;
                    args.extend(["--source".into(), source.clone()]);
                }
                args.extend(["--".into(), path.clone()]);
            }
        }
        let environment = BTreeMap::from([
            ("GIT_TERMINAL_PROMPT".into(), "0".into()),
            ("GIT_PAGER".into(), "cat".into()),
            ("GIT_CONFIG_NOSYSTEM".into(), "1".into()),
        ]);
        Ok(
            ProcessSpec::argv(executable.to_string_lossy().into_owned(), args, repository)?
                .with_environment(environment)?,
        )
    }

    pub async fn execute(
        &self,
        operation: &GitOperation,
        repository: impl AsRef<Path>,
        cancellation: CancellationToken,
    ) -> Result<ProcessResult, GitError> {
        let spec = Self::spec_for(operation, repository)?;
        self.runner
            .run(spec, cancellation)
            .await
            .map_err(Into::into)
    }
}

fn validate_git_paths(paths: &[String]) -> Result<(), GitError> {
    if paths.is_empty() || paths.len() > MAX_GIT_PATHS {
        return Err(GitError::PathLimit);
    }
    for path in paths {
        validate_git_path(path)?;
    }
    Ok(())
}

fn validate_git_path(path: &str) -> Result<(), GitError> {
    let path_value = Path::new(path);
    if path.is_empty()
        || path.len() > MAX_GIT_PATH_BYTES
        || path.chars().any(char::is_control)
        || path_value.is_absolute()
        || path_value.components().any(|component| {
            matches!(
                component,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(GitError::InvalidPath);
    }
    Ok(())
}

fn validate_worktree_path(path: &str) -> Result<(), GitError> {
    if path.is_empty()
        || path.len() > MAX_GIT_PATH_BYTES
        || path.chars().any(char::is_control)
        || Path::new(path)
            .components()
            .any(|component| matches!(component, Component::ParentDir))
    {
        Err(GitError::InvalidPath)
    } else {
        Ok(())
    }
}

fn validate_git_reference(reference: &str) -> Result<(), GitError> {
    let valid = !reference.is_empty()
        && reference.len() <= MAX_GIT_REF_BYTES
        && !reference.starts_with('-')
        && reference.bytes().all(|byte| {
            byte.is_ascii_alphanumeric()
                || matches!(
                    byte,
                    b'.' | b'_' | b'/' | b'-' | b'~' | b'^' | b'@' | b'{' | b'}'
                )
        });
    if valid {
        Ok(())
    } else {
        Err(GitError::InvalidReference)
    }
}

fn validate_branch_name(name: &str) -> Result<(), GitError> {
    validate_git_reference(name)?;
    if name.contains("..")
        || name.contains("//")
        || name.contains("@{")
        || name.ends_with('/')
        || name.ends_with('.')
        || name.ends_with(".lock")
    {
        Err(GitError::InvalidReference)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::process::Command;

    use argentum_domain::{GitOperation, ToolRisk};
    use tokio_util::sync::CancellationToken;

    use super::*;

    fn initialized_repository() -> tempfile::TempDir {
        let repository = tempfile::tempdir().expect("repository");
        for arguments in [
            vec!["init", "--quiet"],
            vec!["config", "user.name", "Argentum Test"],
            vec!["config", "user.email", "argentum-test@example.invalid"],
        ] {
            let status = Command::new("git")
                .args(arguments)
                .current_dir(repository.path())
                .status()
                .expect("Git setup");
            assert!(status.success());
        }
        repository
    }

    #[test]
    fn git_spec_maps_every_read_operation_to_fixed_arguments() {
        let repository = initialized_repository();
        let cases = [
            (
                GitOperation::Status { short: true },
                vec!["status", "--short", "--branch", "--untracked-files=all"],
            ),
            (
                GitOperation::Diff {
                    cached: true,
                    path: Some("src/lib.rs".into()),
                },
                vec![
                    "diff",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--binary",
                    "--cached",
                    "--",
                    "src/lib.rs",
                ],
            ),
            (
                GitOperation::Log { max_count: 12 },
                vec![
                    "log",
                    "--no-decorate",
                    "--date=iso-strict",
                    "--oneline",
                    "-n",
                    "12",
                ],
            ),
            (
                GitOperation::Show {
                    revision: "HEAD~1".into(),
                    path: Some("Cargo.toml".into()),
                },
                vec![
                    "show",
                    "--no-ext-diff",
                    "--no-textconv",
                    "--format=fuller",
                    "HEAD~1",
                    "--",
                    "Cargo.toml",
                ],
            ),
            (
                GitOperation::BranchList,
                vec!["branch", "--list", "--no-color"],
            ),
            (
                GitOperation::WorktreeList,
                vec!["worktree", "list", "--porcelain"],
            ),
        ];

        for (operation, expected) in cases {
            let spec = GitExecutor::spec_for(&operation, repository.path()).expect("Git spec");
            assert!(Path::new(spec.program()).is_absolute());
            assert!(Path::new(spec.program()).is_file());
            assert!(!Path::new(spec.program()).starts_with(repository.path()));
            assert_eq!(&spec.args()[..2], ["-c", "core.fsmonitor=false"]);
            assert_eq!(&spec.args()[2..], expected, "{operation:?}");
            assert_eq!(operation.risk(), ToolRisk::GitRead);
            assert_eq!(
                spec.environment().get("GIT_TERMINAL_PROMPT"),
                Some(&"0".into())
            );
        }
    }

    #[test]
    fn git_spec_maps_mutations_without_generic_or_publication_arguments() {
        let repository = initialized_repository();
        let worktree = repository.path().with_extension("worktree");
        let worktree_text = worktree.to_string_lossy().into_owned();
        let cases = [
            (
                GitOperation::BranchCreate {
                    name: "feature/safe".into(),
                    start_point: Some("HEAD".into()),
                },
                vec!["branch", "feature/safe", "HEAD"],
            ),
            (
                GitOperation::BranchSwitch {
                    name: "feature/safe".into(),
                },
                vec!["switch", "feature/safe"],
            ),
            (
                GitOperation::WorktreeCreate {
                    path: worktree_text.clone(),
                    branch: "feature/worktree".into(),
                    create_branch: true,
                },
                vec![
                    "worktree",
                    "add",
                    "-b",
                    "feature/worktree",
                    worktree_text.as_str(),
                ],
            ),
            (
                GitOperation::Add {
                    paths: vec!["one.txt".into(), "two.txt".into()],
                },
                vec!["add", "--", "one.txt", "two.txt"],
            ),
            (
                GitOperation::Commit {
                    message: "test commit".into(),
                },
                vec!["commit", "-m", "test commit"],
            ),
            (
                GitOperation::Restore {
                    path: "one.txt".into(),
                    source: Some("HEAD".into()),
                },
                vec!["restore", "--source", "HEAD", "--", "one.txt"],
            ),
        ];
        for (operation, expected) in cases {
            let spec = GitExecutor::spec_for(&operation, repository.path()).expect("Git spec");
            assert_eq!(&spec.args()[..2], ["-c", "core.fsmonitor=false"]);
            assert_eq!(&spec.args()[2..], expected, "{operation:?}");
            assert_eq!(operation.risk(), ToolRisk::GitMutation);
        }
    }

    #[test]
    fn git_spec_rejects_unbounded_or_option_like_user_values() {
        let repository = initialized_repository();
        for operation in [
            GitOperation::BranchSwitch {
                name: "--help".into(),
            },
            GitOperation::Log { max_count: 0 },
            GitOperation::Add {
                paths: vec!["../outside".into()],
            },
            GitOperation::Commit { message: "".into() },
        ] {
            assert!(GitExecutor::spec_for(&operation, repository.path()).is_err());
        }
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn git_run_executes_the_allowed_lifecycle_in_an_isolated_repository() {
        let repository = initialized_repository();
        fs::write(repository.path().join("one.txt"), "one\n").expect("fixture");
        let executor = GitExecutor::default();

        let status = executor
            .execute(
                &GitOperation::Status { short: true },
                repository.path(),
                CancellationToken::new(),
            )
            .await
            .expect("status");
        assert!(status.stdout.contains("one.txt"));
        assert_eq!(status.exit_code, Some(0));

        executor
            .execute(
                &GitOperation::Add {
                    paths: vec!["one.txt".into()],
                },
                repository.path(),
                CancellationToken::new(),
            )
            .await
            .expect("add");
        executor
            .execute(
                &GitOperation::Commit {
                    message: "initial".into(),
                },
                repository.path(),
                CancellationToken::new(),
            )
            .await
            .expect("commit");
        let log = executor
            .execute(
                &GitOperation::Log { max_count: 1 },
                repository.path(),
                CancellationToken::new(),
            )
            .await
            .expect("log");
        assert!(log.stdout.contains("initial"));

        fs::write(repository.path().join("one.txt"), "changed\n").expect("change");
        executor
            .execute(
                &GitOperation::Restore {
                    path: "one.txt".into(),
                    source: Some("HEAD".into()),
                },
                repository.path(),
                CancellationToken::new(),
            )
            .await
            .expect("restore");
        assert_eq!(
            fs::read_to_string(repository.path().join("one.txt")).expect("restored read"),
            "one\n"
        );
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn fixed_git_status_does_not_execute_a_repository_fsmonitor() {
        let repository = initialized_repository();
        let helper = repository.path().join("fsmonitor.cmd");
        let marker = repository.path().join("fsmonitor-marker.txt");
        fs::write(
            &helper,
            format!("@echo off\r\n> \"{}\" echo invoked\r\n", marker.display()),
        )
        .expect("fsmonitor helper");
        let helper_config = helper.to_string_lossy().replace('\\', "/");
        assert!(Command::new("git")
            .args(["config", "core.fsmonitor", helper_config.as_str()])
            .current_dir(repository.path())
            .status()
            .expect("configure fsmonitor")
            .success());

        GitExecutor::default()
            .execute(
                &GitOperation::Status { short: true },
                repository.path(),
                CancellationToken::new(),
            )
            .await
            .expect("fixed status");

        assert!(!marker.exists(), "fixed status executed core.fsmonitor");
    }
}
