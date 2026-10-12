//! Length-bounded stdio framing around the maintained MCP service machinery.
use std::io;
use std::sync::{
    atomic::{AtomicU64, AtomicU8, Ordering},
    Arc,
};
use std::time::Duration;

use rmcp::service::{RxJsonRpcMessage, TxJsonRpcMessage};
use rmcp::transport::Transport;
use rmcp::RoleClient;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::Mutex;

use crate::MAX_MESSAGE_BYTES;
use argentum_domain::extensions::ExtensionTransportObservation;

#[derive(Default)]
pub struct WireAccounting {
    pub requests: AtomicU64,
    pub sent_bytes: AtomicU64,
    pub received_bytes: AtomicU64,
    /// 0 none, 1 oversize frame, 2 malformed frame, 3 disconnect mid-frame.
    pub fault: AtomicU8,
    pub mcp_requests: AtomicU64,
    pub mcp_notifications: AtomicU64,
    pub mcp_responses: AtomicU64,
    pub tool_calls: AtomicU64,
    pub repeated_ids: AtomicU64,
    pub ids: std::sync::Mutex<std::collections::BTreeSet<[u8; 32]>>,
    pub incomplete: AtomicU8,
}

impl WireAccounting {
    /// Called at the transport send boundary. No body or RPC identifier retained.
    pub fn observe_send(&self, body: &[u8]) {
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(body) else {
            self.incomplete.store(1, Ordering::Relaxed);
            return;
        };
        if value.get("method").is_some() {
            if let Some(id) = value.get("id") {
                self.mcp_requests.fetch_add(1, Ordering::Relaxed);
                use sha2::Digest;
                let digest: [u8; 32] = sha2::Sha256::digest(id.to_string().as_bytes()).into();
                if let Ok(mut ids) = self.ids.lock() {
                    if ids.contains(&digest) {
                        self.repeated_ids.fetch_add(1, Ordering::Relaxed);
                    } else if ids.len() < 1024 {
                        ids.insert(digest);
                    } else {
                        self.incomplete.store(1, Ordering::Relaxed);
                    }
                } else {
                    self.incomplete.store(1, Ordering::Relaxed);
                }
                if value.get("method").and_then(serde_json::Value::as_str) == Some("tools/call") {
                    self.tool_calls.fetch_add(1, Ordering::Relaxed);
                }
            } else {
                self.mcp_notifications.fetch_add(1, Ordering::Relaxed);
            }
        } else {
            self.mcp_responses.fetch_add(1, Ordering::Relaxed);
        }
    }

    pub fn snapshot(&self) -> ExtensionTransportObservation {
        ExtensionTransportObservation {
            mcp_request_starts: self.mcp_requests.load(Ordering::Relaxed),
            mcp_notification_starts: self.mcp_notifications.load(Ordering::Relaxed),
            mcp_response_starts: self.mcp_responses.load(Ordering::Relaxed),
            tool_call_starts: self.tool_calls.load(Ordering::Relaxed),
            completed_messages: self.requests.load(Ordering::Relaxed),
            repeated_request_ids: self.repeated_ids.load(Ordering::Relaxed),
            sent_bytes: self.sent_bytes.load(Ordering::Relaxed),
            received_bytes: self.received_bytes.load(Ordering::Relaxed),
            http_request_starts: None,
            coverage_incomplete: self.incomplete.load(Ordering::Relaxed) != 0
                || self.fault.load(Ordering::Relaxed) != 0
                || self
                    .mcp_requests
                    .load(Ordering::Relaxed)
                    .saturating_add(self.mcp_notifications.load(Ordering::Relaxed))
                    .saturating_add(self.mcp_responses.load(Ordering::Relaxed))
                    != self.requests.load(Ordering::Relaxed),
        }
    }
}

pub struct BoundedStdio {
    reader: BufReader<Box<dyn AsyncRead + Send + Unpin>>,
    writer: Arc<Mutex<Option<Box<dyn AsyncWrite + Send + Unpin>>>>,
    pub accounting: Arc<WireAccounting>,
}

impl BoundedStdio {
    pub fn new<R, W>(read: R, write: W) -> Self
    where
        R: AsyncRead + Send + Unpin + 'static,
        W: AsyncWrite + Send + Unpin + 'static,
    {
        Self {
            reader: BufReader::new(Box::new(read)),
            writer: Arc::new(Mutex::new(Some(Box::new(write)))),
            accounting: Arc::default(),
        }
    }

    async fn frame(&mut self) -> Option<Vec<u8>> {
        let mut frame = Vec::new();
        loop {
            let chunk = self.reader.fill_buf().await.ok()?;
            if chunk.is_empty() {
                if !frame.is_empty() {
                    self.accounting.fault.store(3, Ordering::Relaxed);
                }
                return None;
            }
            let end = chunk.iter().position(|b| *b == b'\n').map(|p| p + 1);
            let count = end.unwrap_or(chunk.len());
            if count > MAX_MESSAGE_BYTES.saturating_sub(frame.len()) {
                self.accounting.fault.store(1, Ordering::Relaxed);
                return None;
            }
            frame.extend_from_slice(&chunk[..count]);
            self.reader.consume(count);
            self.accounting
                .received_bytes
                .fetch_add(count as u64, Ordering::Relaxed);
            if end.is_some() {
                return Some(frame);
            }
        }
    }
}

impl Transport<RoleClient> for BoundedStdio {
    type Error = io::Error;
    fn send(
        &mut self,
        item: TxJsonRpcMessage<RoleClient>,
    ) -> impl std::future::Future<Output = io::Result<()>> + Send + 'static {
        let writer = Arc::clone(&self.writer);
        let accounting = Arc::clone(&self.accounting);
        let body = serde_json::to_vec(&item);
        async move {
            let mut body = body.map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "MCP request cannot serialize")
            })?;
            if body.len() >= MAX_MESSAGE_BYTES {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "MCP request exceeds byte limit",
                ));
            }
            body.push(b'\n');
            tokio::time::timeout(Duration::from_secs(5), async {
                let mut writer = writer.lock().await;
                let writer = writer.as_mut().ok_or_else(|| {
                    io::Error::new(io::ErrorKind::NotConnected, "MCP connection closed")
                })?;
                accounting.observe_send(&body);
                writer.write_all(&body).await?;
                writer.flush().await
            })
            .await
            .map_err(|_| {
                accounting.incomplete.store(1, Ordering::Relaxed);
                io::Error::new(io::ErrorKind::TimedOut, "MCP write deadline exceeded")
            })??;
            accounting.requests.fetch_add(1, Ordering::Relaxed);
            accounting
                .sent_bytes
                .fetch_add(body.len() as u64, Ordering::Relaxed);
            Ok(())
        }
    }
    async fn receive(&mut self) -> Option<RxJsonRpcMessage<RoleClient>> {
        let frame = self.frame().await?;
        match serde_json::from_slice(&frame) {
            Ok(value) => Some(value),
            Err(_) => {
                self.accounting.fault.store(2, Ordering::Relaxed);
                None
            }
        }
    }
    async fn close(&mut self) -> io::Result<()> {
        if let Some(mut writer) = self.writer.lock().await.take() {
            writer.shutdown().await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn send_boundary_separates_requests_notifications_repeats_and_closed_writer() {
        let mut wire = BoundedStdio::new(tokio::io::empty(), tokio::io::sink());
        let request = serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"control","arguments":{"value":"PRIVATE_FIELD_CANARY"}}});
        let notify = serde_json::json!({"jsonrpc":"2.0","method":"notifications/initialized"});
        for message in [request.clone(), notify, request.clone()] {
            let message: TxJsonRpcMessage<RoleClient> = serde_json::from_value(message).unwrap();
            wire.send(message).await.unwrap();
        }
        let observed = wire.accounting.snapshot();
        assert_eq!(observed.mcp_request_starts, 2);
        assert_eq!(observed.mcp_notification_starts, 1);
        assert_eq!(observed.tool_call_starts, 2);
        assert_eq!(observed.completed_messages, 3);
        assert_eq!(observed.repeated_request_ids, 1);
        assert!(!observed.coverage_incomplete);
        assert!(!serde_json::to_string(&observed)
            .unwrap()
            .contains("PRIVATE_FIELD_CANARY"));
        wire.close().await.unwrap();
        assert!(wire
            .send(serde_json::from_value(request).unwrap())
            .await
            .is_err());
        assert_eq!(
            wire.accounting.snapshot(),
            observed,
            "closed writer cannot claim a physical send"
        );
    }

    #[test]
    fn interrupted_send_and_bounded_identity_tracking_report_incomplete_coverage() {
        let counters = WireAccounting::default();
        for id in 0..1026 {
            counters.observe_send(
                serde_json::json!({"jsonrpc":"2.0","id":id,"method":"tools/list"})
                    .to_string()
                    .as_bytes(),
            );
        }
        assert_eq!(counters.ids.lock().unwrap().len(), 1024);
        assert!(counters.snapshot().coverage_incomplete);
        assert_eq!(counters.snapshot().completed_messages, 0);
        assert_eq!(counters.snapshot().tool_call_starts, 0);
    }
    async fn incoming(bytes: Vec<u8>) -> BoundedStdio {
        let (mut source, read) = tokio::io::duplex(MAX_MESSAGE_BYTES * 2);
        source.write_all(&bytes).await.unwrap();
        source.shutdown().await.unwrap();
        BoundedStdio::new(read, tokio::io::sink())
    }
    #[tokio::test]
    async fn unterminated_oversize_and_malformed_frames_are_distinct() {
        for (bytes, fault) in [
            (vec![b'x'; MAX_MESSAGE_BYTES + 1], 1),
            (b"not-json\n".to_vec(), 2),
            (b"{\"jsonrpc\":\"2.0\"}".to_vec(), 3),
        ] {
            let mut wire = incoming(bytes).await;
            assert!(wire.receive().await.is_none());
            assert_eq!(wire.accounting.fault.load(Ordering::Relaxed), fault);
        }
    }
    #[tokio::test]
    async fn current_jsonrpc_response_round_trips() {
        let mut wire =
            incoming(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"result\":{\"tools\":[]}}\n".to_vec()).await;
        assert!(wire.receive().await.is_some());
        assert_eq!(wire.accounting.fault.load(Ordering::Relaxed), 0);
        assert!(wire.receive().await.is_none());
    }
}
