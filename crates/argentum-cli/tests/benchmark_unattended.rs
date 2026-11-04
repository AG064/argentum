//! The benchmark switch must do nothing in an ordinary build, and in a benchmark build it
//! must need the container's marker. Everything stays in memory, because a saved database
//! is Windows-only today. Run the second half on Linux with:
//! `cargo test -p argentum-cli --features benchmark-unattended --test benchmark_unattended`

use std::path::Path;
use std::process::{Command, Output};

const MARKER: &str = "ARGENTUM_BENCHMARK_DISPOSABLE";

fn base_command(marker: bool) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_argentum"));
    command.env("RUST_LOG", "off").env_remove(MARKER);
    if marker {
        command.env(MARKER, "1");
    }
    command
}

fn invoke(root: &Path, args: &[&str], marker: bool) -> Output {
    base_command(marker)
        .args(args)
        .args(["--isolated", "--workspace"])
        .arg(root.join("workspace"))
        .arg("--database")
        .arg(root.join("state.db"))
        .args(["--json", "--no-color"])
        .output()
        .unwrap()
}

fn output_of(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

fn fresh_root() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir(root.path().join("workspace")).unwrap();
    root
}

#[cfg(not(feature = "benchmark-unattended"))]
#[test]
fn an_ordinary_build_ignores_the_marker() {
    let root = fresh_root();
    let out = invoke(root.path(), &["permissions", "select", "full-access"], true);
    assert!(!out.status.success(), "{}", output_of(&out));
    // Other platforms refuse earlier, because the saved database is Windows-only.
    #[cfg(windows)]
    assert!(
        output_of(&out).contains("local authority is required for this command"),
        "{}",
        output_of(&out)
    );

    let version = base_command(true).arg("--version").output().unwrap();
    assert!(!output_of(&version).contains("benchmark"));

    let run = base_command(true)
        .args(["benchmark-run", "--model", "any", "--prompt", "any"])
        .output()
        .unwrap();
    assert!(!run.status.success());
}

#[cfg(feature = "benchmark-unattended")]
mod benchmark_build {
    use super::*;
    use std::io::{Read, Write};
    use std::net::TcpListener;

    #[test]
    fn it_still_refuses_without_the_marker() {
        let root = fresh_root();
        let out = invoke(
            root.path(),
            &["permissions", "select", "full-access"],
            false,
        );
        assert!(!out.status.success(), "{}", output_of(&out));

        let run = base_command(false)
            .args(["benchmark-run", "--model", "any", "--prompt", "any"])
            .output()
            .unwrap();
        assert!(!run.status.success());
        assert!(output_of(&run).contains(MARKER), "{}", output_of(&run));
    }

    #[test]
    fn it_says_what_it_is() {
        let out = base_command(false).arg("--version").output().unwrap();
        assert!(output_of(&out).contains("benchmark build"));
    }

    fn read_request(stream: &mut std::net::TcpStream) {
        let mut seen = Vec::new();
        let mut chunk = [0; 8192];
        loop {
            let read = stream.read(&mut chunk).unwrap();
            seen.extend_from_slice(&chunk[..read]);
            let text = String::from_utf8_lossy(&seen);
            if let Some(end) = text.find("\r\n\r\n") {
                let length = text[..end]
                    .lines()
                    .find_map(|line| {
                        let (name, value) = line.split_once(':')?;
                        name.eq_ignore_ascii_case("content-length")
                            .then(|| value.trim().parse::<usize>().unwrap())
                    })
                    .unwrap_or(0);
                if seen.len() >= end + 4 + length {
                    return;
                }
            }
            assert!(read > 0, "request ended early");
        }
    }

    /// The point of the build: with no owner, the agent can run the project's own
    /// tests, the same as the other agents it is compared with.
    #[test]
    fn it_runs_a_task_with_a_shell_command_and_no_approval() {
        let root = fresh_root();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}/v1/", listener.local_addr().unwrap());
        let server = std::thread::spawn(move || {
            let replies = [
                serde_json::json!({"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call-1","type":"function","function":{"name":"execute_shell","arguments":"{\"command\":\"echo ran > proof.txt\",\"timeout_ms\":10000}"}}]},"finish_reason":"tool_calls"}]}),
                serde_json::json!({"choices":[{"delta":{"content":"done"},"finish_reason":"stop"}]}),
            ];
            for reply in replies {
                let (mut stream, _) = listener.accept().unwrap();
                read_request(&mut stream);
                stream
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n")
                    .unwrap();
                stream
                    .write_all(format!("data: {reply}\n\ndata: [DONE]\n\n").as_bytes())
                    .unwrap();
            }
        });

        let run = base_command(true)
            .arg("benchmark-run")
            .arg("--workspace")
            .arg(root.path().join("workspace"))
            .args(["--provider", "fixture", "--model", "fixture-model"])
            .args(["--endpoint", endpoint.as_str()])
            .args(["--limits", "204800,196608,4096,4096,1024"])
            .args(["--prompt", "make the proof file"])
            .output()
            .unwrap();
        assert!(run.status.success(), "{}", output_of(&run));
        server.join().unwrap();
        // Windows shells may write UTF-16, so compare only the letters.
        let proof = std::fs::read(root.path().join("workspace/proof.txt")).unwrap();
        let letters = proof
            .into_iter()
            .filter(u8::is_ascii_alphabetic)
            .collect::<Vec<_>>();
        assert_eq!(letters, b"ran");
    }
}
