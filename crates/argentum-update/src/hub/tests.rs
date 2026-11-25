// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Hub client tests against a local fake server. No test reaches the network.

use super::*;
use std::io::{Read, Write};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
enum Route {
    Json(Vec<u8>),
    /// A file. With `ranges`, a `Range: bytes=N-` request gets a 206 answer.
    File {
        body: Vec<u8>,
        ranges: bool,
    },
    /// Announces the whole file but closes after this many bytes.
    CutAfter(Vec<u8>, usize),
    Redirect(String),
    Status(u16),
}

/// A fake Hub. Every request head is kept in `log`.
struct Server {
    base: String,
    log: Arc<Mutex<Vec<String>>>,
}

fn serve(routes: Vec<(&str, Route)>) -> Server {
    let routes: Vec<(String, Route)> = routes
        .into_iter()
        .map(|(path, route)| (path.to_owned(), route))
        .collect();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind");
    let base = format!("http://{}", listener.local_addr().expect("address"));
    let log = Arc::new(Mutex::new(Vec::new()));
    let server_log = log.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { return };
            let mut head = Vec::new();
            let mut byte = [0_u8; 1];
            while !head.ends_with(b"\r\n\r\n") && stream.read(&mut byte).unwrap_or(0) == 1 {
                head.push(byte[0]);
            }
            let head = String::from_utf8_lossy(&head).into_owned();
            server_log.lock().expect("log").push(head.clone());
            let target = head.split_whitespace().nth(1).unwrap_or("/").to_owned();
            let path = target.split('?').next().unwrap_or("/").to_owned();
            let range_start = head.lines().find_map(|line| {
                let lower = line.to_ascii_lowercase();
                lower
                    .strip_prefix("range: bytes=")
                    .and_then(|rest| rest.trim_end_matches('-').trim().parse::<usize>().ok())
            });
            let route = routes
                .iter()
                .find(|(route, _)| *route == path)
                .map(|(_, route)| route.clone());
            let _ = match route {
                Some(Route::Json(body)) => write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .and_then(|()| stream.write_all(&body)),
                Some(Route::File { body, ranges }) => match range_start.filter(|_| ranges) {
                    Some(start) if start < body.len() => write!(
                        stream,
                        "HTTP/1.1 206 Partial Content\r\nContent-Range: bytes {start}-{}/{}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len() - 1,
                        body.len(),
                        body.len() - start
                    )
                    .and_then(|()| stream.write_all(&body[start..])),
                    _ => write!(
                        stream,
                        "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                        body.len()
                    )
                    .and_then(|()| stream.write_all(&body)),
                },
                Some(Route::CutAfter(body, cut)) => write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    body.len()
                )
                .and_then(|()| stream.write_all(&body[..cut])),
                Some(Route::Redirect(location)) => write!(
                    stream,
                    "HTTP/1.1 302 Found\r\nLocation: {location}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                ),
                Some(Route::Status(code)) => write!(
                    stream,
                    "HTTP/1.1 {code} Status\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                ),
                None => stream.write_all(
                    b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                ),
            };
        }
    });
    Server { base, log }
}

impl Server {
    fn heads(&self) -> Vec<String> {
        self.log.lock().expect("log").clone()
    }
}

const COMMIT: &str = "0123456789abcdef0123456789abcdef01234567";

fn sha(bytes: &[u8]) -> String {
    crate::hex(&Sha256::digest(bytes))
}

fn repo_json(files: &[(&str, &[u8])]) -> Vec<u8> {
    let siblings: Vec<serde_json::Value> = files
        .iter()
        .map(|(name, body)| {
            serde_json::json!({
                "rfilename": name,
                "size": body.len(),
                "lfs": { "sha256": sha(body), "size": body.len(), "pointerSize": 134 }
            })
        })
        .collect();
    serde_json::to_vec(&serde_json::json!({
        "id": "tester/Tiny-GGUF",
        "sha": COMMIT,
        "gated": false,
        "tags": ["gguf", "license:mit"],
        "cardData": { "license": "apache-2.0" },
        "gguf": { "total": 1_500_000_000_u64, "architecture": "qwen3", "context_length": 32768 },
        "siblings": siblings
    }))
    .expect("json")
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("argentum-hub-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    dir
}

fn run<T>(future: impl std::future::Future<Output = T>) -> T {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime")
        .block_on(future)
}

#[test]
fn search_asks_for_gguf_repositories_without_a_token() {
    let body = serde_json::to_vec(&serde_json::json!([
        { "id": "a/B-GGUF", "downloads": 10, "likes": 2, "tags": ["license:apache-2.0"], "gated": false },
        { "id": "c/D-GGUF", "downloads": 5, "likes": 1, "tags": [], "gated": "manual" },
        { "id": "../escape", "downloads": 1, "likes": 0 }
    ]))
    .expect("json");
    let server = serve(vec![("/api/models", Route::Json(body))]);
    let hub = Hub::local(&server.base, None);
    let results = run(hub.search("qwen coder")).expect("search");
    assert_eq!(results.len(), 2, "an invalid repository name is dropped");
    assert_eq!(results[0].id, "a/B-GGUF");
    assert_eq!(results[0].license.as_deref(), Some("apache-2.0"));
    assert!(!results[0].gated);
    assert!(results[1].gated);
    let head = &server.heads()[0];
    assert!(head.contains("search=qwen+coder"), "{head}");
    assert!(head.contains("filter=gguf"), "{head}");
    assert!(
        !head.to_ascii_lowercase().contains("authorization"),
        "{head}"
    );
}

#[test]
fn a_repository_lists_its_quantizations_and_projector() {
    let payload = serde_json::to_vec(&serde_json::json!({
        "id": "tester/Tiny-GGUF",
        "sha": COMMIT,
        "gated": "auto",
        "tags": ["license:mit"],
        "cardData": { "license": "apache-2.0" },
        "gguf": { "total": 7_000_000_000_u64, "architecture": "llama", "context_length": 131072 },
        "siblings": [
            { "rfilename": "README.md", "size": 10 },
            { "rfilename": "Tiny-Q4_K_M.gguf", "size": 400, "lfs": { "sha256": "a".repeat(64), "size": 400 } },
            { "rfilename": "Tiny-UD-Q4_K_XL.gguf", "size": 420, "lfs": { "sha256": "b".repeat(64), "size": 420 } },
            { "rfilename": "BF16/Tiny-BF16-00002-of-00002.gguf", "size": 500, "lfs": { "sha256": "c".repeat(64), "size": 500 } },
            { "rfilename": "BF16/Tiny-BF16-00001-of-00002.gguf", "size": 600, "lfs": { "sha256": "d".repeat(64), "size": 600 } },
            { "rfilename": "Q8_0/Tiny-Q8_0-00001-of-00002.gguf", "size": 300, "lfs": { "sha256": "e".repeat(64), "size": 300 } },
            { "rfilename": "mmproj-Tiny-F16.gguf", "size": 50, "lfs": { "sha256": "f".repeat(64), "size": 50 } },
            { "rfilename": "NoHash-Q2_K.gguf", "size": 100 },
            { "rfilename": "../evil-Q4_0.gguf", "size": 1, "lfs": { "sha256": "1".repeat(64), "size": 1 } }
        ]
    }))
    .expect("json");
    let repo = parse_repo(&payload).expect("repo");
    assert_eq!(
        repo.license.as_deref(),
        Some("apache-2.0"),
        "the model card wins over tags"
    );
    assert!(repo.gated);
    assert_eq!(repo.context_length, Some(131_072));
    assert_eq!(repo.parameters, Some(7_000_000_000));
    let labels: Vec<String> = repo.variants.iter().map(Variant::label).collect();
    assert_eq!(
        labels,
        ["Q4_K_M", "Q4_K_XL", "BF16, 2 parts", "F16 vision projector"],
        "{labels:?}"
    );
    let bf16 = &repo.variants[2];
    assert_eq!(bf16.model_file(), "Tiny-BF16-00001-of-00002.gguf");
    assert_eq!(bf16.size_bytes(), 1100);
    assert_eq!(repo.variants[3].kind, VariantKind::Projector);
    // The half of a split set, the file with no hash and the escaping path are skipped.
    assert_eq!(repo.skipped_files.len(), 3, "{:?}", repo.skipped_files);
}

#[test]
fn names_and_quantizations_are_read_strictly() {
    assert!(valid_repo_id("unsloth/Qwen3.5-4B-GGUF"));
    for bad in [
        "",
        "a",
        "a/b/c",
        "../b",
        "a/..",
        "a/b c",
        "a/.hidden",
        "a\\b",
    ] {
        assert!(!valid_repo_id(bad), "{bad}");
    }
    assert_eq!(
        local_name("sub/Model-Q4_0.gguf").as_deref(),
        Some("Model-Q4_0.gguf")
    );
    for bad in [
        "../x.gguf",
        "/x.gguf",
        "a\\x.gguf",
        "x.bin",
        "a//x.gguf",
        "x y.gguf",
        ".x.gguf",
    ] {
        assert_eq!(local_name(bad), None, "{bad}");
    }
    assert_eq!(quant_of("Qwen3-8B-Q4_K_M.gguf").as_deref(), Some("Q4_K_M"));
    assert_eq!(quant_of("model.IQ4_XS.gguf").as_deref(), Some("IQ4_XS"));
    assert_eq!(
        quant_of("m-UD-Q4_K_XL-00001-of-00002.gguf").as_deref(),
        Some("Q4_K_XL")
    );
    assert_eq!(quant_of("gemma-bf16.gguf").as_deref(), Some("BF16"));
    assert_eq!(quant_of("NOQ4_K_MX.gguf"), None);
    assert!(parse_repo(br#"{"id":"a/b","sha":"short","siblings":[]}"#).is_err());
    assert_eq!(
        format!("{:?}", HubToken::new("hf_secret").expect("token")),
        "HubToken(hidden)"
    );
    assert_eq!(HubToken::new("  "), None);
}

fn tiny_model() -> Vec<u8> {
    (0..200_000_u32).map(|value| (value % 251) as u8).collect()
}

#[test]
fn a_download_is_checked_and_placed_in_the_model_folder() {
    let body = tiny_model();
    let file_path = format!("/tester/Tiny-GGUF/resolve/{COMMIT}/Tiny-Q4_K_M.gguf");
    let server = serve(vec![(
        file_path.as_str(),
        Route::File {
            body: body.clone(),
            ranges: true,
        },
    )]);
    let repo = parse_repo(&repo_json(&[("Tiny-Q4_K_M.gguf", &body)])).expect("repo");
    let models = temp_dir("whole");
    let hub = Hub::local(&server.base, None);
    let plan = plan_download(&repo, &repo.variants[0], &models).expect("plan");
    assert_eq!(plan.remaining_bytes, body.len() as u64);
    let mut last = None;
    let path = run(hub.download(&repo, &repo.variants[0], &models, |progress| {
        last = Some(progress)
    }))
    .expect("download");
    assert_eq!(path, models.join("Tiny-Q4_K_M.gguf"));
    assert_eq!(fs::read(&path).expect("model"), body);
    assert_eq!(
        last.map(|progress| progress.received),
        Some(body.len() as u64)
    );
    assert!(!models.join("Tiny-Q4_K_M.gguf.download").exists());
    assert!(!models.join("Tiny-Q4_K_M.gguf.download.json").exists());

    // A second download finds the file, checks it, and fetches nothing.
    let requests = server.heads().len();
    run(hub.download(&repo, &repo.variants[0], &models, |_| {})).expect("again");
    assert_eq!(server.heads().len(), requests);
    let _ = fs::remove_dir_all(&models);
}

#[test]
fn an_interrupted_download_resumes_with_a_range_request() {
    let body = tiny_model();
    let file_path = format!("/tester/Tiny-GGUF/resolve/{COMMIT}/Tiny-Q4_K_M.gguf");
    let cut = serve(vec![(
        file_path.as_str(),
        Route::CutAfter(body.clone(), 70_000),
    )]);
    let repo = parse_repo(&repo_json(&[("Tiny-Q4_K_M.gguf", &body)])).expect("repo");
    let models = temp_dir("resume");
    let error =
        run(Hub::local(&cut.base, None).download(&repo, &repo.variants[0], &models, |_| {}))
            .expect_err("cut short");
    assert!(error.0.contains("resume"), "{error}");
    let partial = models.join("Tiny-Q4_K_M.gguf.download");
    let kept = fs::metadata(&partial).expect("partial kept").len();
    assert!(kept > 0 && kept < body.len() as u64);
    let plan = plan_download(&repo, &repo.variants[0], &models).expect("plan");
    assert_eq!(plan.resumed_bytes, kept);
    assert_eq!(plan.remaining_bytes, body.len() as u64 - kept);

    let full = serve(vec![(
        file_path.as_str(),
        Route::File {
            body: body.clone(),
            ranges: true,
        },
    )]);
    run(Hub::local(&full.base, None).download(&repo, &repo.variants[0], &models, |_| {}))
        .expect("resumed");
    assert_eq!(
        fs::read(models.join("Tiny-Q4_K_M.gguf")).expect("model"),
        body
    );
    let head = &full.heads()[0];
    assert!(
        head.to_ascii_lowercase()
            .contains(&format!("range: bytes={kept}-")),
        "{head}"
    );
    let _ = fs::remove_dir_all(&models);
}

#[test]
fn a_server_that_ignores_ranges_restarts_the_file() {
    let body = tiny_model();
    let file_path = format!("/tester/Tiny-GGUF/resolve/{COMMIT}/Tiny-Q4_K_M.gguf");
    let repo = parse_repo(&repo_json(&[("Tiny-Q4_K_M.gguf", &body)])).expect("repo");
    let models = temp_dir("norange");
    fs::create_dir_all(&models).expect("models");
    let target = models.join("Tiny-Q4_K_M.gguf");
    let (partial, record) = partial_paths(&target);
    fs::write(&partial, &body[..5000]).expect("partial");
    fs::write(
        &record,
        serde_json::to_vec(&PartialRecord::for_file(&repo, &repo.variants[0].files[0]))
            .expect("record"),
    )
    .expect("record");
    let server = serve(vec![(
        file_path.as_str(),
        Route::File {
            body: body.clone(),
            ranges: false,
        },
    )]);
    run(Hub::local(&server.base, None).download(&repo, &repo.variants[0], &models, |_| {}))
        .expect("restarted");
    assert_eq!(fs::read(&target).expect("model"), body);
    let _ = fs::remove_dir_all(&models);
}

#[test]
fn a_partial_file_from_another_commit_is_not_resumed() {
    let body = tiny_model();
    let repo = parse_repo(&repo_json(&[("Tiny-Q4_K_M.gguf", &body)])).expect("repo");
    let models = temp_dir("stale");
    fs::create_dir_all(&models).expect("models");
    let (partial, record) = partial_paths(&models.join("Tiny-Q4_K_M.gguf"));
    fs::write(&partial, &body[..5000]).expect("partial");
    let mut stale = PartialRecord::for_file(&repo, &repo.variants[0].files[0]);
    stale.commit = "f".repeat(40);
    fs::write(&record, serde_json::to_vec(&stale).expect("record")).expect("record");
    let plan = plan_download(&repo, &repo.variants[0], &models).expect("plan");
    assert_eq!(plan.resumed_bytes, 0);
    discard_partial(&repo.variants[0], &models);
    assert!(!partial.exists() && !record.exists());
    let _ = fs::remove_dir_all(&models);
}

#[test]
fn a_file_with_the_wrong_checksum_is_deleted() {
    let body = tiny_model();
    let mut tampered = body.clone();
    tampered[1234] ^= 0xff;
    let file_path = format!("/tester/Tiny-GGUF/resolve/{COMMIT}/Tiny-Q4_K_M.gguf");
    let server = serve(vec![(
        file_path.as_str(),
        Route::File {
            body: tampered,
            ranges: true,
        },
    )]);
    let repo = parse_repo(&repo_json(&[("Tiny-Q4_K_M.gguf", &body)])).expect("repo");
    let models = temp_dir("tampered");
    let error =
        run(Hub::local(&server.base, None).download(&repo, &repo.variants[0], &models, |_| {}))
            .expect_err("tampered");
    assert!(error.0.contains("SHA-256"), "{error}");
    assert!(!models.join("Tiny-Q4_K_M.gguf").exists());
    assert!(!models.join("Tiny-Q4_K_M.gguf.download").exists());
    let _ = fs::remove_dir_all(&models);
}

#[test]
fn the_token_goes_to_the_hub_and_not_to_the_storage_host() {
    let body = tiny_model();
    let storage = serve(vec![(
        "/blob",
        Route::File {
            body: body.clone(),
            ranges: true,
        },
    )]);
    let file_path = format!("/tester/Tiny-GGUF/resolve/{COMMIT}/Tiny-Q4_K_M.gguf");
    let hub_server = serve(vec![(
        file_path.as_str(),
        Route::Redirect(format!("{}/blob", storage.base)),
    )]);
    let repo = parse_repo(&repo_json(&[("Tiny-Q4_K_M.gguf", &body)])).expect("repo");
    let models = temp_dir("token");
    let token = HubToken::new("hf_test_token_value").expect("token");
    run(Hub::local(&hub_server.base, Some(token)).download(
        &repo,
        &repo.variants[0],
        &models,
        |_| {},
    ))
    .expect("download");
    let hub_head = hub_server.heads()[0].to_ascii_lowercase();
    assert!(
        hub_head.contains("authorization: bearer hf_test_token_value"),
        "{hub_head}"
    );
    let storage_head = storage.heads()[0].to_ascii_lowercase();
    assert!(!storage_head.contains("authorization"), "{storage_head}");
    let _ = fs::remove_dir_all(&models);
}

#[test]
fn a_gated_model_without_access_says_what_to_do() {
    let body = tiny_model();
    let file_path = format!("/tester/Tiny-GGUF/resolve/{COMMIT}/Tiny-Q4_K_M.gguf");
    let server = serve(vec![(file_path.as_str(), Route::Status(401))]);
    let mut repo = parse_repo(&repo_json(&[("Tiny-Q4_K_M.gguf", &body)])).expect("repo");
    repo.gated = true;
    let models = temp_dir("gated");
    let error =
        run(Hub::local(&server.base, None).download(&repo, &repo.variants[0], &models, |_| {}))
            .expect_err("gated");
    assert!(
        error.0.contains("gated") && error.0.contains("token"),
        "{error}"
    );
    let _ = fs::remove_dir_all(&models);
}

#[test]
fn the_disk_space_check_names_both_sizes() {
    let plan = DownloadPlan {
        present: Vec::new(),
        remaining_bytes: 5_000_000_000,
        resumed_bytes: 0,
        free_bytes: Some(3_000_000_000),
    };
    let problem = plan.space_problem().expect("too little space");
    assert!(
        problem.contains("5.00 GB") && problem.contains("3.00 GB"),
        "{problem}"
    );
    let roomy = DownloadPlan {
        free_bytes: Some(50_000_000_000),
        ..plan.clone()
    };
    assert_eq!(roomy.space_problem(), None);
    let unknown = DownloadPlan {
        free_bytes: None,
        ..plan
    };
    assert_eq!(unknown.space_problem(), None);
    assert!(free_space(&std::env::temp_dir()).is_some_and(|free| free > 0));
}

#[test]
fn repository_lookups_use_the_api_path() {
    let server = serve(vec![(
        "/api/models/tester/Tiny-GGUF",
        Route::Json(repo_json(&[("Tiny-Q4_K_M.gguf", b"x")])),
    )]);
    let repo = run(Hub::local(&server.base, None).repo("tester/Tiny-GGUF")).expect("repo");
    assert_eq!(repo.commit, COMMIT);
    assert!(server.heads()[0].contains("blobs=true"));
    assert!(run(Hub::local(&server.base, None).repo("../x")).is_err());
}
