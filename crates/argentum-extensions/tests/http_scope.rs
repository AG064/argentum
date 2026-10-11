use argentum_extensions::{
    connection::McpSession,
    http::{ReviewedHttp, ScopedCredential},
    MAX_MESSAGE_BYTES,
};
use argentum_security::SecretValue;
use serde_json::{json, Value};
use std::{
    collections::BTreeSet,
    io::{Read, Write},
    net::{SocketAddr, TcpListener, TcpStream},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    time::Duration,
};
use tokio_util::sync::CancellationToken;

struct Server {
    address: SocketAddr,
    stopped: Arc<AtomicBool>,
    observations: Arc<Mutex<Vec<(String, bool)>>>,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Server {
    fn start(mode: &'static str, redirect: Option<SocketAddr>) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = listener.local_addr().unwrap();
        let stopped = Arc::new(AtomicBool::new(false));
        let observations = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::clone(&stopped);
        let records = Arc::clone(&observations);
        let thread = std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                let (mut socket, _) = match listener.accept() {
                    Ok(value) => value,
                    Err(_) => {
                        std::thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                };
                socket.set_nonblocking(false).unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(2)))
                    .unwrap();
                let mut bytes = Vec::new();
                let mut buffer = [0; 4096];
                let split;
                loop {
                    let Ok(count) = socket.read(&mut buffer) else {
                        break;
                    };
                    if count == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&buffer[..count]);
                    if bytes
                        .windows(4)
                        .position(|part| part == b"\r\n\r\n")
                        .is_some()
                    {
                        break;
                    }
                }
                let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") else {
                    continue;
                };
                split = end + 4;
                let header = String::from_utf8_lossy(&bytes[..end]);
                let authorized = header.lines().any(|line| {
                    line.eq_ignore_ascii_case("authorization: Bearer synthetic-test-value")
                });
                let length = header
                    .lines()
                    .filter_map(|line| line.split_once(':'))
                    .find(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                    .and_then(|(_, value)| value.trim().parse::<usize>().ok())
                    .unwrap_or(0);
                if length > MAX_MESSAGE_BYTES {
                    continue;
                }
                while bytes.len() < split + length {
                    let Ok(count) = socket.read(&mut buffer) else {
                        break;
                    };
                    if count == 0 {
                        break;
                    }
                    bytes.extend_from_slice(&buffer[..count]);
                }
                let request: Value = serde_json::from_slice(&bytes[split..]).unwrap_or(Value::Null);
                let method = request["method"].as_str().unwrap_or("DELETE").to_owned();
                records.lock().unwrap().push((method.clone(), authorized));
                let id = request.get("id").cloned().unwrap_or(Value::Null);
                let (status, content_type, body, extra) = if mode == "redirect" {
                    (
                        307,
                        "text/plain",
                        b"redirect".to_vec(),
                        format!("Location: http://{}/outside\r\n", redirect.unwrap()),
                    )
                } else if mode == "unauthorized" || (mode == "auth" && !authorized) {
                    (401, "text/plain", b"scope required".to_vec(), String::new())
                } else if mode == "oversize" {
                    (
                        200,
                        "application/json",
                        vec![b'x'; MAX_MESSAGE_BYTES + 1],
                        String::new(),
                    )
                } else if mode == "malformed" {
                    (200, "application/json", b"not json".to_vec(), String::new())
                } else if mode == "expired" {
                    (404, "text/plain", b"expired".to_vec(), String::new())
                } else if id.is_null() {
                    (202, "application/json", Vec::new(), String::new())
                } else {
                    let result = match method.as_str() {
                        "server/discover" => {
                            serde_json::to_value(rmcp::model::DiscoverResult::new(
                                vec![rmcp::model::ProtocolVersion::V_2026_07_28],
                                serde_json::from_value(json!({"tools":{}})).unwrap(),
                            ))
                            .unwrap()
                        }
                        "tools/list" => {
                            json!({"tools":[{"name":"echo","inputSchema":{"type":"object","properties":{"message":{"type":"string"}}}}]})
                        }
                        "tools/call" => {
                            json!({"content":[{"type":"text","text":request["params"]["arguments"]["message"]}]})
                        }
                        _ => json!({}),
                    };
                    let body =
                        serde_json::to_vec(&json!({"jsonrpc":"2.0","id":id,"result":result}))
                            .unwrap();
                    if mode == "sse" {
                        (
                            200,
                            "text/event-stream",
                            format!(
                                "event: message\ndata: {}\n\n",
                                String::from_utf8(body).unwrap()
                            )
                            .into_bytes(),
                            String::new(),
                        )
                    } else {
                        (200, "application/json", body, String::new())
                    }
                };
                let response = format!("HTTP/1.1 {status} Control\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n", body.len());
                let _ = socket.write_all(response.as_bytes());
                let _ = socket.write_all(&body);
            }
        });
        Self {
            address,
            stopped,
            observations,
            thread: Some(thread),
        }
    }
    fn endpoint(&self) -> String {
        format!("http://{}/mcp", self.address)
    }
    fn client(&self, authenticated: bool) -> ReviewedHttp {
        let credential = authenticated.then(|| ScopedCredential {
            origin: format!("http://{}", self.address),
            account: "synthetic-account".into(),
            secret: Arc::new(SecretValue::new("synthetic-test-value")),
        });
        ReviewedHttp::new(&self.endpoint(), &[self.address], true, credential).unwrap()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        self.stopped.store(true, Ordering::Relaxed);
        let _ = TcpStream::connect(self.address);
        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}

#[tokio::test]
async fn actual_streamable_json_sse_and_scoped_authentication() {
    for mode in ["json", "sse", "auth"] {
        let server = Server::start(mode, None);
        let client = server.client(mode == "auth");
        let starts = Arc::clone(&client.request_starts);
        let mut session = McpSession::connect_http(
            client,
            BTreeSet::from(["echo".into()]),
            CancellationToken::new(),
        )
        .await
        .unwrap();
        let catalog = session.discover().await.unwrap_or_else(|error| {
            panic!(
                "mode {mode}: {error}; non-secret RPC observations: {:?}",
                server.observations.lock().unwrap()
            )
        });
        let handle = catalog[0].handle.clone();
        assert_eq!(
            session
                .invoke(&handle, json!({"message":"http-current-control"}))
                .await
                .unwrap()["content"][0]["text"],
            "http-current-control"
        );
        session.retire().await.unwrap();
        assert!(starts.load(Ordering::Relaxed) >= 4);
        if mode == "auth" {
            assert!(server
                .observations
                .lock()
                .unwrap()
                .iter()
                .all(|(_, authorized)| *authorized));
        }
    }
}

#[tokio::test]
async fn redirects_auth_malformed_oversize_and_expiry_cannot_produce_success() {
    let outside = Server::start("json", None);
    for mode in [
        "redirect",
        "unauthorized",
        "malformed",
        "oversize",
        "expired",
    ] {
        let server = Server::start(mode, Some(outside.address));
        assert!(
            McpSession::connect_http(
                server.client(true),
                BTreeSet::new(),
                CancellationToken::new()
            )
            .await
            .is_err(),
            "{mode}"
        );
    }
    assert!(outside.observations.lock().unwrap().is_empty());
}
