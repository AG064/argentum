use std::io::Write;
use std::process::{Command, Stdio};

fn executable() -> std::path::PathBuf {
    std::env::var_os("ARGENTUM_F20B_CLI_EXE")
        .map(Into::into)
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_argentum").into())
}

#[test]
fn canonical_binary_reports_argentum_identity() {
    let output = Command::new(executable())
        .arg("--version")
        .output()
        .expect("run argentum version");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "argentum 0.1.0"
    );
}

#[test]
fn help_uses_only_the_canonical_command_name() {
    let output = Command::new(executable())
        .arg("--help")
        .output()
        .expect("run argentum help");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(output.status.success());
    assert!(stdout.contains("Usage:"));
    assert!(stdout.contains("argentum"));
    assert!(!stdout.contains("argentum-cli"));
}

#[test]
fn non_tty_chat_uses_plain_host_backed_terminal_fallback() {
    let directory = tempfile::tempdir().expect("directory");
    let workspace = directory.path().join("workspace");
    std::fs::create_dir(&workspace).expect("workspace");
    let database = directory.path().join("state.sqlite3");
    let mut child = Command::new(executable())
        .args([
            "chat",
            "--non-interactive",
            "--isolated",
            "--workspace",
            workspace.to_str().expect("workspace utf-8"),
            "--database",
            database.to_str().expect("database utf-8"),
        ])
        .env("RUST_LOG", "off")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn argentum chat");

    child
        .stdin
        .as_mut()
        .expect("chat stdin")
        .write_all(b"1\nserver\nloopback\nhttp://127.0.0.1:9/v1\nfixture-model\n\n\n\n200000,200000,256,256,1024\ny\n/new\n/quit\n")
        .expect("write chat input");
    drop(child.stdin.take());
    let output = child.wait_with_output().expect("wait for chat");
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("ARGENTUM"));
    assert!(stdout.contains("workspace:"));
    assert!(stdout.contains("session:"));
    assert!(!stdout.contains('\u{1b}'));
    assert!(stdout.contains("minimax"));
    assert!(stdout.contains("deepseek  Add or configure DeepSeek coding"));
    assert!(stdout.contains("local     Start llama.cpp text-only"));
    let connection = rusqlite::Connection::open_with_flags(
        &database,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
    )
    .unwrap();
    assert_eq!(
        connection
            .query_row("SELECT count(*) FROM model_budgets", [], |r| r
                .get::<_, u64>(0))
            .unwrap(),
        1
    );
    assert_eq!(
        connection
            .query_row(
                "SELECT count(*) FROM runtime_host_generations WHERE state = 'active'",
                [],
                |r| r.get::<_, u64>(0)
            )
            .unwrap(),
        0
    );
}

#[test]
fn cancelling_hosted_setup_keeps_profile_and_retires_host() {
    for provider in ["minimax", "deepseek"] {
        let directory = tempfile::tempdir().unwrap();
        let database = directory.path().join(format!("cancel-{provider}.db"));
        let mut child = Command::new(executable())
            .args(["chat", "--non-interactive", "--isolated", "--workspace"])
            .arg(directory.path())
            .arg("--database")
            .arg(&database)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(format!("1\n{provider}\n/cancel\n").as_bytes())
            .unwrap();
        let output = child.wait_with_output().unwrap();
        assert_eq!(output.status.code(), Some(1));
        let text = String::from_utf8_lossy(&output.stdout);
        assert!(text.contains("needs its own API key"));
        assert!(String::from_utf8_lossy(&output.stderr).contains("setup cancelled"));
        let connection = rusqlite::Connection::open_with_flags(
            database,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        assert_eq!(
            connection
                .query_row(
                    "SELECT count(*) FROM runtime_host_generations WHERE state = 'active'",
                    [],
                    |r| r.get::<_, u64>(0)
                )
                .unwrap(),
            0
        );
        assert_eq!(
            connection
                .query_row(
                    "SELECT count(*) FROM provider_profiles WHERE profile_id = ?1",
                    [provider],
                    |r| r.get::<_, u64>(0)
                )
                .unwrap(),
            1
        );
    }
}
