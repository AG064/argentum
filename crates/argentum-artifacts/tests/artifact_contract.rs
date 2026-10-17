#![cfg(windows)]
//! Ephemeral production-controller checks, no personal account or inference.
use argentum_artifacts::{
    artifact::{Artifacts, Configuration, Input},
    sha256,
};
use serde_json::{json, Value};
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

fn configuration() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let ws = root.path().join("workspace");
    std::fs::create_dir(&ws).unwrap();
    for (name, body) in [
        ("a.txt", "Synthetic source A\n"),
        ("b.txt", "Synthetic source B\n"),
        (
            "handover.md",
            "# Handover\n\nRésumé from Pärnu.\n\n| Fact | Value |\n|---|---|\n| Owner | Õun |\n",
        ),
    ] {
        std::fs::write(ws.join(name), body).unwrap();
    }
    let worker = PathBuf::from(env!("CARGO_BIN_EXE_argentum-pdf-worker"));
    let config = Configuration {
        schema_version: 1,
        workspace: ws.clone(),
        inputs: vec![
            Input {
                path: "a.txt".into(),
                sha256: sha256(b"Synthetic source A\n"),
            },
            Input {
                path: "b.txt".into(),
                sha256: sha256(b"Synthetic source B\n"),
            },
        ],
        markdown: "handover.md".into(),
        output_directory: "out".into(),
        media_directory: root.path().join("media"),
        worker_directory: root.path().join("workers"),
        worker: worker.clone(),
        worker_sha256: sha256(&std::fs::read(worker).unwrap()),
        pdfium: PathBuf::from(
            std::env::var_os("ARGENTUM_TEST_PDFIUM_DLL").expect("pinned DLL required"),
        ),
        font: PathBuf::from(
            std::env::var_os("ARGENTUM_TEST_ARTIFACT_FONT").expect("pinned font required"),
        ),
    };
    for path in [&config.media_directory, &config.worker_directory] {
        argentum_execution::ensure_owner_only_directory(path).unwrap();
    }
    let path = root.path().join("artifact.json");
    std::fs::write(&path, serde_json::to_vec(&config).unwrap()).unwrap();
    (root, ws, path)
}

#[tokio::test]
#[ignore = "requires pinned private PDFium/font assets; explicit offline qualification"]
async fn current_render_images_verification_and_invalidation() {
    let (_root, ws, config) = configuration();
    let app = Artifacts::load(&config).unwrap();
    let opened = app.open().unwrap();
    let sha = opened["markdown_sha256"].as_str().unwrap();
    assert!(app
        .render(&"0".repeat(64), None, &CancellationToken::new())
        .await
        .is_err());
    assert!(!ws.join("out").exists());
    let result = app
        .render(sha, None, &CancellationToken::new())
        .await
        .unwrap();
    let id = serde_json::from_value(result["artifact_id"].clone()).unwrap();
    assert!(
        argentum_artifacts::verify::verify(&config, &CancellationToken::new())
            .await
            .is_err(),
        "missing preparation cannot pass"
    );
    for tile in result["tiles"].as_array().unwrap() {
        let image = app
            .page(
                id,
                tile["page"].as_u64().unwrap() as u32,
                tile["tile"].as_u64().unwrap() as u32,
            )
            .unwrap();
        assert_eq!(image["sha256"], tile["sha256"]);
    }
    let verified = argentum_artifacts::verify::verify(&config, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(verified["raw_pdf_parse_and_render"], "passed");
    assert_eq!(verified["fresh_pixels_match"], "passed");
    assert!(verified["not_established"]
        .as_array()
        .unwrap()
        .contains(&json!("model visual comprehension")));
    assert!(
        app.render(sha, None, &CancellationToken::new())
            .await
            .is_err(),
        "stale selection cannot be replaced"
    );
    std::fs::write(ws.join("handover.md"), "# Corrected draft\n").unwrap();
    assert!(app.require_current(id).is_err());
    assert!(app.page(id, 1, 1).is_err());
    assert!(
        argentum_artifacts::verify::verify(&config, &CancellationToken::new())
            .await
            .is_err()
    );
    assert_eq!(
        std::fs::read(ws.join("a.txt")).unwrap(),
        b"Synthetic source A\n"
    );
    assert_eq!(
        std::fs::read(ws.join("b.txt")).unwrap(),
        b"Synthetic source B\n"
    );
}

#[tokio::test]
#[ignore = "requires pinned private PDFium/font assets; explicit offline qualification"]
async fn changed_sources_and_forged_selection_cannot_redirect_images() {
    let (_root, ws, config) = configuration();
    let app = Artifacts::load(&config).unwrap();
    let sha = sha256(&std::fs::read(ws.join("handover.md")).unwrap());
    app.render(&sha, None, &CancellationToken::new())
        .await
        .unwrap();
    let current = ws.join("out/current.json");
    let mut record: Value = serde_json::from_slice(&std::fs::read(&current).unwrap()).unwrap();
    record["tiles"][0]["artifact"] = json!("../../outside.png");
    std::fs::write(&current, serde_json::to_vec(&record).unwrap()).unwrap();
    assert!(app.current().is_err());
    std::fs::write(ws.join("a.txt"), "changed approved source").unwrap();
    assert!(app.open().is_err());
}

#[test]
fn path_contract_rejects_aliases_and_traversal() {
    use argentum_artifacts::artifact::relative;
    for path in [
        "../secret",
        "/workspace/file",
        "C:/file",
        "a\\b",
        ".git/index",
        "target/file",
        "a/./b",
        "a//b",
        "a./b",
        "",
    ] {
        assert!(relative(path).is_err(), "{path}");
    }
    assert_eq!(
        relative("deliverables/handover.md").unwrap(),
        PathBuf::from("deliverables/handover.md")
    );
}

#[tokio::test]
#[ignore = "requires pinned private assets; explicit offline qualification"]
async fn cancellation_before_admission_creates_no_worker_or_selection() {
    let (_root, ws, config) = configuration();
    let app = Artifacts::load(&config).unwrap();
    let cancel = CancellationToken::new();
    cancel.cancel();
    let sha = sha256(&std::fs::read(ws.join("handover.md")).unwrap());
    assert!(app.render(&sha, None, &cancel).await.is_err());
    assert!(!ws.join("out").exists());
    assert_eq!(
        std::fs::read_dir(&app.configuration.worker_directory)
            .unwrap()
            .count(),
        0
    );
}

#[test]
#[ignore = "requires pinned private assets; explicit offline qualification"]
fn junction_output_is_rejected_without_touching_its_target() {
    use std::os::windows::process::CommandExt;
    let (root, ws, config) = configuration();
    let target = root.path().join("outside");
    std::fs::create_dir(&target).unwrap();
    std::fs::write(target.join("sentinel"), b"UNCHANGED").unwrap();
    let link = ws.join("out");
    let command = format!(r#"mklink /J "{}" "{}""#, link.display(), target.display());
    let result = std::process::Command::new("cmd.exe")
        .args(["/D", "/C"])
        .raw_arg(command)
        .output()
        .unwrap();
    assert!(result.status.success());
    let app = Artifacts::load(&config).unwrap();
    assert!(app.current().is_err());
    assert_eq!(
        std::fs::read(target.join("sentinel")).unwrap(),
        b"UNCHANGED"
    );
    assert_eq!(std::fs::read_dir(&target).unwrap().count(), 1);
    // Remove only the junction itself. Do not recurse into its target.
    std::fs::remove_dir(link).unwrap();
    assert!(target.join("sentinel").is_file());
}

#[tokio::test]
#[ignore = "requires pinned private assets; explicit offline qualification"]
async fn selection_lock_denies_a_competing_generation_before_worker_dispatch() {
    use std::os::windows::fs::OpenOptionsExt;
    let (_root, ws, config) = configuration();
    let app = Artifacts::load(&config).unwrap();
    argentum_execution::ensure_owner_only_directory(ws.join("out")).unwrap();
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .share_mode(0)
        .open(ws.join("out/selection.lock"))
        .unwrap();
    let sha = sha256(&std::fs::read(ws.join("handover.md")).unwrap());
    let error = app
        .render(&sha, None, &CancellationToken::new())
        .await
        .err()
        .unwrap();
    assert!(error.to_string().contains("owns selection"));
    assert_eq!(
        std::fs::read_dir(&app.configuration.worker_directory)
            .unwrap()
            .count(),
        0
    );
    assert!(app.current().unwrap().is_none());
    drop(lock);
}

#[tokio::test]
#[ignore = "requires pinned private assets; explicit offline qualification"]
async fn long_sha_table_wraps_and_repeats_headers_without_losing_text() {
    let (_root, ws, config) = configuration();
    let mut source =
        "# Hash ledger\n\nRésumé from Pärnu.\n\n| Entry | SHA-256 |\n|---|---|\n".to_owned();
    for i in 0..24 {
        source.push_str(&format!(
            "| case-{i:02} | {} |\n",
            "0123456789abcdef".repeat(4)
        ));
    }
    std::fs::write(ws.join("handover.md"), &source).unwrap();
    let app = Artifacts::load(&config).unwrap();
    let result = app
        .render(&sha256(source.as_bytes()), None, &CancellationToken::new())
        .await
        .unwrap();
    assert!(result["page_count"].as_u64().unwrap() > 1);
    let id = serde_json::from_value(result["artifact_id"].clone()).unwrap();
    for tile in result["tiles"].as_array().unwrap() {
        app.page(
            id,
            tile["page"].as_u64().unwrap() as u32,
            tile["tile"].as_u64().unwrap() as u32,
        )
        .unwrap();
    }
    let verified = argentum_artifacts::verify::verify(&config, &CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(verified["text_token_inventory"], "passed");
}
