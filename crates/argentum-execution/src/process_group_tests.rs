// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Process-group tests. On Android they run on a device or emulator: see
//! docs/ANDROID_HOST.md.
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;

use crate::process::{base_environment, ProcessRunner, ProcessSpec, ProcessState};

fn shell(command: &str, workspace: &Path) -> ProcessSpec {
    ProcessSpec::shell(command.to_owned(), workspace).expect("spec")
}

fn pid_is_gone(pid: &str) -> bool {
    let stat = format!("/proc/{}/stat", pid.trim());
    fs::read_to_string(stat).map_or(true, |text| {
        text.rsplit(") ")
            .next()
            .is_some_and(|rest| rest.starts_with('Z'))
    })
}

async fn wait_until_gone(pid: &str) {
    for _ in 0..100 {
        if pid_is_gone(pid) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("process {} is still running", pid.trim());
}

async fn wait_for_file(path: &Path) {
    for _ in 0..200 {
        if fs::read_to_string(path).is_ok_and(|text| !text.trim().is_empty()) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(25)).await;
    }
}

#[tokio::test]
async fn a_shell_command_runs_in_the_workspace() {
    let workspace = tempfile::tempdir().expect("workspace");
    let result = ProcessRunner
        .run(
            shell("echo hello; pwd; exit 3", workspace.path()),
            CancellationToken::new(),
        )
        .await
        .expect("run");
    assert_eq!(result.state, ProcessState::Exited);
    assert_eq!(result.exit_code, Some(3));
    let canonical = fs::canonicalize(workspace.path()).expect("canonical");
    assert_eq!(
        result.stdout,
        format!("hello\n{}\n", canonical.display()),
        "stderr: {}",
        result.stderr
    );
}

#[tokio::test]
async fn an_argument_vector_finds_system_programs_on_the_path() {
    let workspace = tempfile::tempdir().expect("workspace");
    let spec = ProcessSpec::argv(
        "echo",
        ["one".to_owned(), "two words".to_owned()],
        workspace.path(),
    )
    .expect("spec");
    let result = ProcessRunner
        .run(spec, CancellationToken::new())
        .await
        .expect("run");
    assert_eq!(result.exit_code, Some(0), "stderr: {}", result.stderr);
    assert_eq!(result.stdout, "one two words\n");
}

#[tokio::test]
async fn a_missing_program_fails_to_start() {
    let workspace = tempfile::tempdir().expect("workspace");
    let spec = ProcessSpec::argv(
        "argentum-no-such-program",
        Vec::<String>::new(),
        workspace.path(),
    )
    .expect("spec");
    assert!(matches!(
        ProcessRunner.run(spec, CancellationToken::new()).await,
        Err(crate::ProcessError::Spawn(_))
    ));
}

#[tokio::test]
async fn a_command_that_runs_too_long_is_stopped_with_its_children() {
    let workspace = tempfile::tempdir().expect("workspace");
    let spec = shell("sleep 120 & echo $! > child.pid; wait", workspace.path())
        .with_timeout(Duration::from_millis(500))
        .expect("timeout");
    let started = Instant::now();
    let result = ProcessRunner
        .run(spec, CancellationToken::new())
        .await
        .expect("run");
    assert_eq!(result.state, ProcessState::TimedOut);
    assert_eq!(result.exit_code, None);
    assert!(started.elapsed() < Duration::from_secs(10));
    let child = fs::read_to_string(workspace.path().join("child.pid")).expect("child pid");
    wait_until_gone(&child).await;
}

#[tokio::test]
async fn cancelling_a_command_kills_its_whole_process_group() {
    let workspace = tempfile::tempdir().expect("workspace");
    // The grandchild is a second shell that starts its own sleep, two levels down.
    let spec = shell(
        "sleep 120 & echo $! > child.pid; sh -c 'sleep 120 & echo $! > grandchild.pid; wait' & wait",
        workspace.path(),
    );
    let cancellation = CancellationToken::new();
    let cancel = cancellation.clone();
    let marker = workspace.path().join("grandchild.pid");
    tokio::spawn(async move {
        wait_for_file(&marker).await;
        cancel.cancel();
    });
    let started = Instant::now();
    let result = ProcessRunner.run(spec, cancellation).await.expect("run");
    assert_eq!(result.state, ProcessState::Cancelled);
    assert!(started.elapsed() < Duration::from_secs(10));
    for name in ["child.pid", "grandchild.pid"] {
        let pid = fs::read_to_string(workspace.path().join(name)).expect("pid");
        wait_until_gone(&pid).await;
    }
}

#[tokio::test]
async fn a_finished_command_leaves_no_background_process_behind() {
    let workspace = tempfile::tempdir().expect("workspace");
    let result = ProcessRunner
        .run(
            shell(
                "sleep 120 > /dev/null 2>&1 & echo $! > child.pid",
                workspace.path(),
            ),
            CancellationToken::new(),
        )
        .await
        .expect("run");
    assert_eq!(result.state, ProcessState::Exited);
    let child = fs::read_to_string(workspace.path().join("child.pid")).expect("child pid");
    wait_until_gone(&child).await;
}

#[tokio::test]
async fn output_beyond_the_limit_is_counted_but_not_kept() {
    let workspace = tempfile::tempdir().expect("workspace");
    let spec = shell(
        "i=0; while [ $i -lt 2000 ]; do echo 0123456789abcdef; echo err >&2; i=$((i+1)); done",
        workspace.path(),
    )
    .with_output_limit(1000)
    .expect("limit");
    let result = ProcessRunner
        .run(spec, CancellationToken::new())
        .await
        .expect("run");
    assert_eq!(result.exit_code, Some(0));
    assert!(result.stdout_truncated);
    assert!(result.stderr_truncated);
    assert_eq!(result.stdout_bytes, 2000 * 17);
    assert_eq!(result.stderr_bytes, 2000 * 4);
    assert!(result.stdout.len() <= 1000);
    assert!(result.stderr.len() <= 1000);
    assert!(result.stdout.starts_with("0123456789abcdef\n"));
}

#[tokio::test]
async fn a_command_sees_only_the_minimal_environment() {
    let workspace = tempfile::tempdir().expect("workspace");
    let spec = ProcessSpec::argv("env", Vec::<String>::new(), workspace.path())
        .expect("spec")
        .with_environment(BTreeMap::from([("NO_COLOR".to_owned(), "1".to_owned())]))
        .expect("environment");
    let result = ProcessRunner
        .run(spec, CancellationToken::new())
        .await
        .expect("run");
    assert_eq!(result.exit_code, Some(0), "stderr: {}", result.stderr);
    let names = result
        .stdout
        .lines()
        .filter_map(|line| line.split_once('=').map(|(name, _)| name.to_owned()))
        .collect::<Vec<_>>();
    assert!(names.iter().any(|name| name == "PATH"), "{names:?}");
    assert!(names.iter().any(|name| name == "NO_COLOR"), "{names:?}");
    let expected = base_environment()
        .into_iter()
        .map(|(name, _)| name.to_owned())
        .chain(["NO_COLOR".to_owned()])
        .collect::<Vec<_>>();
    for name in &names {
        assert!(expected.contains(name), "unexpected variable {name}");
    }
    #[cfg(target_os = "android")]
    assert!(result
        .stdout
        .lines()
        .any(|line| line == format!("PATH={}", crate::process::ANDROID_PATH)));
}

#[cfg(target_os = "android")]
#[test]
fn android_shell_is_the_system_shell() {
    let workspace = tempfile::tempdir().expect("workspace");
    let spec = shell("true", workspace.path());
    assert_eq!(spec.program(), "/system/bin/sh");
    assert!(Path::new(spec.program()).exists());
}
