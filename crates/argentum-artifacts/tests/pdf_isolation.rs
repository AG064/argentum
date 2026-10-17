#![cfg(windows)]
//! Real offline renderer controls, never Google or provider calls.
use argentum_artifacts::sha256;
use std::path::PathBuf;
use tokio_util::sync::CancellationToken;

fn pdf(javascript: bool) -> Vec<u8> {
    let catalog = if javascript {
        "<< /Type /Catalog /Pages 2 0 R /Names << /JavaScript << /Names [(control) 6 0 R] >> >> >>"
    } else {
        "<< /Type /Catalog /Pages 2 0 R >>"
    };
    let content = "BT /F1 18 Tf 50 730 Td (Synthetic handover) Tj 0 -30 Td (Renderer control, not live acceptance.) Tj ET";
    let mut objects = vec![catalog.into(),"<< /Type /Pages /Kids [3 0 R] /Count 1 >>".into(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 600 800] /Resources << /Font << /F1 4 0 R >> >> /Contents 5 0 R >>".into(),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".into(),
        format!("<< /Length {} >>\nstream\n{content}\nendstream",content.len())];
    if javascript {
        objects.push("<< /S /JavaScript /JS (app.alert\\(1\\)) >>".into());
    }
    let mut bytes = b"%PDF-1.7\n".to_vec();
    let mut offsets = vec![0];
    for (i, object) in objects.iter().enumerate() {
        offsets.push(bytes.len());
        bytes.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
    }
    let start = bytes.len();
    bytes.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len()).as_bytes());
    for offset in &offsets[1..] {
        bytes.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    bytes.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{start}\n%%EOF\n",
            offsets.len()
        )
        .as_bytes(),
    );
    bytes
}

#[test]
fn worker_refuses_direct_owner_execution() {
    let result = std::process::Command::new(env!("CARGO_BIN_EXE_argentum-pdf-worker"))
        .output()
        .unwrap();
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("isolated LPAC"));
}

#[tokio::test]
#[ignore = "requires private pinned PDFium and Noto Sans assets; run explicitly offline"]
async fn markdown_creation_round_trips_real_unicode_table_and_pages() {
    let dll =
        PathBuf::from(std::env::var_os("ARGENTUM_TEST_PDFIUM_DLL").expect("pinned DLL required"));
    let font = PathBuf::from(
        std::env::var_os("ARGENTUM_TEST_ARTIFACT_FONT").expect("pinned font required"),
    );
    let worker = PathBuf::from(env!("CARGO_BIN_EXE_argentum-pdf-worker"));
    let worker_sha = sha256(&std::fs::read(&worker).unwrap());
    let temp = tempfile::tempdir().unwrap();
    let markdown="# Handover\n\nRésumé from Pärnu for Õun.\n\n## Facts\n\n| Fact | Value |\n|---|---|\n| Budget | 32 requests |\n| Status | Review required |\n\n- Preserve inputs\n- Verify the final PDF\n";
    let made = argentum_execution::run_isolated_markdown_worker(
        &worker,
        &worker_sha,
        &dll,
        &font,
        markdown,
        &temp.path().join("create"),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(made.exit_code, Some(0), "{made:?}");
    let pdf = std::fs::read(made.directory.join("output/result.pdf")).unwrap();
    let rendered = argentum_execution::run_isolated_pdf_worker(
        &worker,
        &worker_sha,
        &dll,
        &pdf,
        &temp.path().join("render"),
        &CancellationToken::new(),
    )
    .await
    .unwrap();
    assert_eq!(rendered.exit_code, Some(0), "{rendered:?}");
    let report: serde_json::Value = serde_json::from_slice(
        &std::fs::read(rendered.directory.join("output/render.json")).unwrap(),
    )
    .unwrap();
    let text = report["pages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["text"].as_str().unwrap())
        .collect::<Vec<_>>()
        .join("\n");
    for expected in [
        "Résumé",
        "Pärnu",
        "Õun",
        "Budget",
        "32 requests",
        "Review required",
        "Preserve inputs",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text:?}");
    }
    assert_eq!(report["pdf_sha256"], sha256(&pdf));
    if let Some(destination) = std::env::var_os("ARGENTUM_TEST_ARTIFACT_RETAIN") {
        let destination = PathBuf::from(destination);
        std::fs::create_dir_all(&destination).unwrap();
        std::fs::copy(
            made.directory.join("output/result.pdf"),
            destination.join("roundtrip.pdf"),
        )
        .unwrap();
        std::fs::copy(
            rendered.directory.join("output/render.json"),
            destination.join("render.json"),
        )
        .unwrap();
        for page in report["pages"].as_array().unwrap() {
            for tile in page["tiles"].as_array().unwrap() {
                let name = tile["artifact"].as_str().unwrap();
                std::fs::copy(
                    rendered.directory.join("output").join(name),
                    destination.join(name),
                )
                .unwrap();
            }
        }
    }
}

#[tokio::test]
#[ignore = "requires the private pinned PDFium DLL; run explicitly offline with ARGENTUM_TEST_PDFIUM_DLL"]
async fn actual_lpac_pdf_render_and_negative_controls() {
    let dll = PathBuf::from(
        std::env::var_os("ARGENTUM_TEST_PDFIUM_DLL").expect("pinned DLL is required"),
    );
    let worker = PathBuf::from(env!("CARGO_BIN_EXE_argentum-pdf-worker"));
    let worker_sha = sha256(&std::fs::read(&worker).unwrap());
    let temp = tempfile::tempdir().unwrap();
    for (name, bytes, pass) in [
        ("valid", pdf(false), true),
        ("malformed", b"%PDF-1.7\nINVALID".to_vec(), false),
        ("javascript", pdf(true), false),
    ] {
        let run = argentum_execution::run_isolated_pdf_worker(
            &worker,
            &worker_sha,
            &dll,
            &bytes,
            &temp.path().join(name),
            &CancellationToken::new(),
        )
        .await
        .unwrap();
        assert_eq!(run.exit_code == Some(0), pass, "{name}: {run:?}");
        assert!(!run.timed_out, "{name}");
        assert_eq!(run.boundary, "windows.lpac.no-network.fixed-pdf-worker.v1");
        if pass {
            let report: serde_json::Value = serde_json::from_slice(
                &std::fs::read(run.directory.join("output/render.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(report["pdf_sha256"], sha256(&bytes));
            assert_eq!(report["page_count"], 1);
            assert!(report["pages"][0]["text"]
                .as_str()
                .unwrap()
                .contains("Synthetic handover"));
            let tiles = report["pages"][0]["tiles"].as_array().unwrap();
            assert!(tiles.len() >= 2);
            for tile in tiles {
                let png = std::fs::read(
                    run.directory
                        .join("output")
                        .join(tile["artifact"].as_str().unwrap()),
                )
                .unwrap();
                assert_eq!(sha256(&png), tile["sha256"]);
                assert!(png.len() <= 128 * 1024);
                assert_eq!(&png[..8], b"\x89PNG\r\n\x1a\n");
            }
        }
    }
}
