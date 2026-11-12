//! Production prepared-request streaming over credential-free synthetic TLS.
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use tokio_rustls::{rustls, TlsAcceptor};
use tokio_util::sync::CancellationToken;

use super::{ModelProvider, OpenAiCompatibleProvider, ProviderError, ProviderEvent};
use crate::tests::{event_stream_response, prepared_fixture, test_provider};

const ROOT: &[u8] = include_bytes!("../tests/fixtures/tls/public-root.der");
const LEAF: &[u8] = include_bytes!("../tests/fixtures/tls/public-localhost.der");
const PUBLIC_KEY: &[u8] = include_bytes!("../tests/fixtures/chatgpt/public-ring-test-key.der");

struct LocalTls {
    port: u16,
    task: Option<tokio::task::JoinHandle<Option<String>>>,
}

impl LocalTls {
    async fn start(stall_after_delta: bool) -> Self {
        let config = rustls::ServerConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .expect("protocols")
        .with_no_client_auth()
        .with_single_cert(
            vec![rustls::pki_types::CertificateDer::from(LEAF.to_vec())],
            rustls::pki_types::PrivateKeyDer::try_from(PUBLIC_KEY.to_vec())
                .expect("public fixture key"),
        )
        .expect("synthetic server certificate");
        let acceptor = TlsAcceptor::from(Arc::new(config));
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("loopback listener");
        let port = listener.local_addr().expect("local address").port();
        let task = tokio::spawn(async move {
            let (tcp, _) = tokio::time::timeout(Duration::from_secs(5), listener.accept())
                .await
                .ok()?
                .ok()?;
            let mut stream = tokio::time::timeout(Duration::from_secs(5), acceptor.accept(tcp))
                .await
                .ok()?
                .ok()?;
            let mut request = Vec::new();
            loop {
                let mut buffer = [0_u8; 1024];
                let count = tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buffer))
                    .await
                    .ok()?
                    .ok()?;
                if count == 0 || request.len() + count > 16 * 1024 {
                    return None;
                }
                request.extend_from_slice(&buffer[..count]);
                if let Some(split) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                    let headers = String::from_utf8_lossy(&request[..split]);
                    let length = headers
                        .lines()
                        .find_map(|line| {
                            line.to_ascii_lowercase()
                                .strip_prefix("content-length:")?
                                .trim()
                                .parse::<usize>()
                                .ok()
                        })
                        .unwrap_or(0);
                    if request.len() >= split + 4 + length {
                        break;
                    }
                }
            }
            let request = String::from_utf8(request).ok()?;
            let body = "data: {\"choices\":[{\"delta\":{\"content\":\"tls-ok\"},\"finish_reason\":\"stop\"}]}\n\ndata: [DONE]\n\n";
            if stall_after_delta {
                let headers = "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nConnection: close\r\n\r\n";
                stream.write_all(headers.as_bytes()).await.ok()?;
                stream
                    .write_all(b"data: {\"choices\":[{\"delta\":{\"content\":\"first\"}}]}\n\n")
                    .await
                    .ok()?;
                stream.flush().await.ok()?;
                let mut buffer = [0_u8; 1];
                let _ =
                    tokio::time::timeout(Duration::from_secs(5), stream.read(&mut buffer)).await;
            } else {
                stream
                    .write_all(event_stream_response(body).as_bytes())
                    .await
                    .ok()?;
                stream.flush().await.ok()?;
            }
            Some(request)
        });
        Self {
            port,
            task: Some(task),
        }
    }

    async fn finish(mut self) -> Option<String> {
        tokio::time::timeout(
            Duration::from_secs(6),
            self.task.take().expect("owned task"),
        )
        .await
        .expect("bounded server completion")
        .expect("server task")
    }
}

impl Drop for LocalTls {
    fn drop(&mut self) {
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

fn provider(endpoint: &str, trust_fixture: bool) -> OpenAiCompatibleProvider {
    let mut provider = test_provider(
        endpoint,
        Some(argentum_security::SecretValue::new("synthetic-tls-canary")),
    );
    let mut client = reqwest::Client::builder()
        .no_proxy()
        .https_only(true)
        .connect_timeout(Duration::from_secs(3))
        .timeout(Duration::from_secs(5));
    if trust_fixture {
        client = client
            .add_root_certificate(reqwest::Certificate::from_der(ROOT).expect("public test root"));
    }
    provider.client = client.build().expect("test-only trust configuration");
    provider
}

#[tokio::test]
async fn tls_prepared_stream_accepts_trusted_hostname_and_routes_synthetic_credential() {
    let server = LocalTls::start(false).await;
    let endpoint = format!("https://localhost:{}/v1/", server.port);
    let provider = provider(&endpoint, true);
    let (profile, budget, request) = prepared_fixture(&endpoint);
    let prepared = provider
        .prepare_request(&profile, &budget, request)
        .expect("prepare");
    let validation = provider
        .validate_prepared_request(&prepared)
        .await
        .expect("validate");
    let (sender, mut receiver) = mpsc::channel(8);
    provider
        .stream_prepared_validated_with_cancellation(
            &prepared,
            &validation,
            sender,
            CancellationToken::new(),
        )
        .await
        .expect("trusted TLS production stream");
    assert_eq!(
        receiver.recv().await,
        Some(ProviderEvent::Delta("tls-ok".into()))
    );
    assert_eq!(receiver.recv().await, Some(ProviderEvent::Completed));
    assert!(receiver.recv().await.is_none());
    let wire = server.finish().await.expect("authenticated request");
    assert!(wire.starts_with("POST /v1/chat/completions HTTP/1.1"));
    assert!(wire
        .to_ascii_lowercase()
        .contains("authorization: bearer synthetic-tls-canary"));
}

#[tokio::test]
async fn tls_prepared_stream_rejects_untrusted_root_and_wrong_hostname_before_http() {
    for (host, trust_fixture) in [("localhost", false), ("127.0.0.1", true)] {
        let server = LocalTls::start(false).await;
        let endpoint = format!("https://{host}:{}/v1/", server.port);
        let provider = provider(&endpoint, trust_fixture);
        let (profile, budget, request) = prepared_fixture(&endpoint);
        let prepared = provider
            .prepare_request(&profile, &budget, request)
            .expect("prepare");
        let validation = provider
            .validate_prepared_request(&prepared)
            .await
            .expect("validate");
        let (sender, mut receiver) = mpsc::channel(8);
        assert!(matches!(
            provider
                .stream_prepared_validated_with_cancellation(
                    &prepared,
                    &validation,
                    sender,
                    CancellationToken::new()
                )
                .await,
            Err(ProviderError::PreDispatchConnection)
        ));
        assert!(receiver.recv().await.is_none());
        assert!(
            server.finish().await.is_none(),
            "HTTP reached an unauthenticated peer"
        );
    }
}

#[tokio::test]
async fn tls_prepared_stream_cancellation_stops_after_first_delta() {
    let server = LocalTls::start(true).await;
    let endpoint = format!("https://localhost:{}/v1/", server.port);
    let provider = provider(&endpoint, true);
    let (profile, budget, request) = prepared_fixture(&endpoint);
    let prepared = provider
        .prepare_request(&profile, &budget, request)
        .expect("prepare");
    let validation = provider
        .validate_prepared_request(&prepared)
        .await
        .expect("validate");
    let (sender, mut receiver) = mpsc::channel(8);
    let cancellation = CancellationToken::new();
    let run = provider.stream_prepared_validated_with_cancellation(
        &prepared,
        &validation,
        sender,
        cancellation.clone(),
    );
    tokio::pin!(run);
    tokio::select! {
        event = receiver.recv() => assert_eq!(event, Some(ProviderEvent::Delta("first".into()))),
        result = &mut run => panic!("stream ended before cancellation: {result:?}"),
    }
    cancellation.cancel();
    assert!(matches!(run.await, Err(ProviderError::RequestCancelled)));
    assert!(receiver.recv().await.is_none());
    assert!(server.finish().await.is_some());
}
