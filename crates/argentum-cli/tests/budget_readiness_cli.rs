use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn executable() -> PathBuf {
    std::env::var_os("ARGENTUM_F20B_CLI_EXE")
        .map(Into::into)
        .unwrap_or_else(|| env!("CARGO_BIN_EXE_argentum").into())
}

fn invoke(root: &Path, args: &[&str]) -> Output {
    Command::new(executable())
        .args(args)
        .args(["--isolated", "--workspace"])
        .arg(root.join("workspace"))
        .arg("--database")
        .arg(root.join("state.db"))
        .args(["--json", "--no-color"])
        .env("RUST_LOG", "off")
        .output()
        .unwrap()
}

fn json(root: &Path, args: &[&str]) -> serde_json::Value {
    let out = invoke(root, args);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}

#[test]
fn budget_readiness_actual_process_restart_wrong_store_and_no_dispatch() {
    let a = tempfile::tempdir().unwrap();
    let b = tempfile::tempdir().unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
    for root in [a.path(), b.path()] {
        std::fs::create_dir(root.join("workspace")).unwrap();
        json(
            root,
            &[
                "provider",
                "save",
                "fixture",
                "--label",
                "Fixture",
                "--kind",
                "openai-compatible",
                "--endpoint",
                &endpoint,
                "--model",
                "fixture-model",
                "--select",
            ],
        );
    }
    let preview = json(a.path(), &["context"]);
    assert!(preview["accounting"].is_null());
    let failed = invoke(a.path(), &["run", "preserve this unsent draft"]);
    assert!(!failed.status.success());
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&failed.stdout),
        String::from_utf8_lossy(&failed.stderr)
    );
    assert!(output.contains("context_budget_invalid"), "{output}");
    assert!(
        !output.contains("TaskAccepted"),
        "draft must not be accepted: {output}"
    );
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );

    let invalid = invoke(
        a.path(),
        &[
            "provider",
            "budget",
            "fixture",
            "--limits",
            "204800,196608,4096,8192,1024",
        ],
    );
    assert!(!invalid.status.success());
    let invalid_run = invoke(a.path(), &["run", "still unsent"]);
    assert!(!invalid_run.status.success());
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    json(
        a.path(),
        &[
            "provider",
            "budget",
            "fixture",
            "--limits",
            "204800,196608,4096,4096,1024",
        ],
    );
    let a_budget = json(a.path(), &["context"])["accounting"].clone();
    assert_eq!(a_budget["usable_context"], 204800);
    assert_eq!(a_budget["separate_input_limit"], 196608);
    assert_eq!(a_budget["maximum_output"], 4096);
    assert_eq!(a_budget["output_reserve"], 4096);
    assert_eq!(a_budget["uncertainty_allowance"], 1024);
    assert!(json(b.path(), &["context"])["accounting"].is_null());
    json(
        b.path(),
        &[
            "provider",
            "budget",
            "fixture",
            "--limits",
            "204800,196608,4096,4096,1024",
        ],
    );
    let b_budget = json(b.path(), &["context"])["accounting"].clone();
    assert_ne!(a_budget["budget_revision"], b_budget["budget_revision"]);
    assert_eq!(
        json(a.path(), &["context"])["accounting"]["budget_revision"],
        a_budget["budget_revision"]
    );

    json(
        a.path(),
        &["provider", "model", "fixture", "--model", "different-model"],
    );
    assert!(json(a.path(), &["context"])["accounting"].is_null());
    let stale = invoke(a.path(), &["run", "different model has no declaration"]);
    assert!(!stale.status.success());
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}
