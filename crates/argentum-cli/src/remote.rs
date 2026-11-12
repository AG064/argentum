// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Remote sessions: the JSONL command protocol carried over a secure channel
//! between two paired devices.
//!
//! The channel carries a byte stream in sealed chunks, so the protocol is the
//! same as `argentum serve` on stdin and stdout, with the same request limits.
//! A remote device has no approval responder: anything that needs approval is
//! refused on the serving device, and its workspace policy stays authoritative.

use std::io::{Read, Write};

use argentum_devices::channel::{ChannelReceiver, ChannelSender};
use tokio::io::{AsyncReadExt, AsyncWriteExt, DuplexStream};
use tokio::sync::mpsc;

use crate::server::serve_jsonl;
use crate::CommandHost;

/// The largest chunk sent in one sealed frame.
const CHUNK_BYTES: usize = 64 * 1024;
const QUEUE_CHUNKS: usize = 16;

/// Connects a split channel to an in-memory stream. Bytes from the other
/// device can be read from the returned stream, and bytes written to it are
/// sent to the other device. Reading ends when the other device hangs up.
///
/// The receiving thread stays blocked until the connection closes, so the
/// caller shuts the connection down when it is done.
pub fn bridge_channel<R, W>(
    mut receiver: ChannelReceiver<R>,
    mut sender: ChannelSender<W>,
) -> DuplexStream
where
    R: Read + Send + 'static,
    W: Write + Send + 'static,
{
    let (local, bridge) = tokio::io::duplex(CHUNK_BYTES);
    let (mut bridge_read, mut bridge_write) = tokio::io::split(bridge);

    let (inbound, mut inbound_queue) = mpsc::channel::<Vec<u8>>(QUEUE_CHUNKS);
    std::thread::spawn(move || {
        while let Ok(chunk) = receiver.receive() {
            if inbound.blocking_send(chunk).is_err() {
                break;
            }
        }
    });
    tokio::spawn(async move {
        while let Some(chunk) = inbound_queue.recv().await {
            if bridge_write.write_all(&chunk).await.is_err() {
                return;
            }
        }
        let _ = bridge_write.shutdown().await;
    });

    let (outbound, mut outbound_queue) = mpsc::channel::<Vec<u8>>(QUEUE_CHUNKS);
    tokio::spawn(async move {
        let mut buffer = vec![0_u8; CHUNK_BYTES];
        loop {
            match bridge_read.read(&mut buffer).await {
                Ok(0) | Err(_) => break,
                Ok(read) => {
                    if outbound.send(buffer[..read].to_vec()).await.is_err() {
                        break;
                    }
                }
            }
        }
    });
    std::thread::spawn(move || {
        while let Some(chunk) = outbound_queue.blocking_recv() {
            if sender.send(&chunk).is_err() {
                break;
            }
        }
    });
    local
}

/// Serves the command protocol to one paired device until it hangs up.
pub async fn serve_jsonl_over_channel<R, W>(
    host: CommandHost,
    receiver: ChannelReceiver<R>,
    sender: ChannelSender<W>,
) -> std::io::Result<()>
where
    R: Read + Send + 'static,
    W: Write + Send + 'static,
{
    let (reader, writer) = tokio::io::split(bridge_channel(receiver, sender));
    serve_jsonl(host, tokio::io::BufReader::new(reader), writer).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use argentum_devices::channel::{ChannelError, SecureChannel};
    use argentum_devices::DeviceIdentity;
    use argentum_platform::{PlatformError, SecretStore};
    use argentum_security::SecretValue;
    use std::collections::HashMap;
    use std::net::{Shutdown, TcpListener, TcpStream};
    use std::sync::Mutex;
    use tokio::io::AsyncBufReadExt;

    #[derive(Default)]
    struct MemoryStore(Mutex<HashMap<String, String>>);

    impl SecretStore for MemoryStore {
        fn get(&self, key: &str) -> Result<Option<SecretValue>, PlatformError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .get(key)
                .cloned()
                .map(SecretValue::new))
        }
        fn set(&self, key: &str, value: SecretValue) -> Result<(), PlatformError> {
            self.0
                .lock()
                .unwrap()
                .insert(key.to_owned(), value.expose().to_owned());
            Ok(())
        }
        fn delete(&self, key: &str) -> Result<(), PlatformError> {
            self.0.lock().unwrap().remove(key);
            Ok(())
        }
    }

    fn identity() -> DeviceIdentity {
        DeviceIdentity::load_or_create(&MemoryStore::default()).unwrap()
    }

    /// A real command host answers a request sent from a paired device, and
    /// the server ends cleanly when that device hangs up.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_paired_device_drives_a_real_command_host() {
        use crate::protocol::RequestEnvelope;
        use argentum_domain::AppCommand;

        let (pc, phone) = (identity(), identity());
        let (pc_key, phone_key) = (pc.public_key(), phone.public_key());
        let workspace = tempfile::tempdir().unwrap();
        let host = CommandHost::start_composed(crate::HostConfig::in_memory(workspace.path()))
            .await
            .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        let serving = tokio::spawn(async move {
            let (stream, _) = tokio::task::spawn_blocking(move || listener.accept())
                .await
                .unwrap()
                .unwrap();
            let writer = stream.try_clone().unwrap();
            let closer = stream.try_clone().unwrap();
            let channel = tokio::task::spawn_blocking(move || {
                SecureChannel::accept(stream, &phone, |key| *key == pc_key)
            })
            .await
            .unwrap()
            .unwrap();
            let (sender, receiver) = channel.split(writer);
            let result = serve_jsonl_over_channel(host, receiver, sender).await;
            closer.shutdown(Shutdown::Both).ok();
            result
        });

        let request = format!(
            "{}\n",
            serde_json::to_string(&RequestEnvelope::command(
                "remote-list",
                AppCommand::ListSessions {
                    query: String::new(),
                    filter: Default::default(),
                    limit: 20,
                    cursor: None,
                },
            ))
            .unwrap()
        );
        let lines = tokio::task::spawn_blocking(move || {
            let stream = TcpStream::connect(address).unwrap();
            stream
                .set_read_timeout(Some(std::time::Duration::from_secs(20)))
                .unwrap();
            let writer = stream.try_clone().unwrap();
            let channel = SecureChannel::connect(stream, &pc, |key| *key == phone_key).unwrap();
            let (mut sender, mut receiver) = channel.split(writer.try_clone().unwrap());
            sender.send(request.as_bytes()).unwrap();
            let mut received = String::new();
            while !received
                .lines()
                .any(|line| line.contains("SessionListLoaded"))
            {
                match receiver.receive() {
                    Ok(chunk) => received.push_str(&String::from_utf8(chunk).unwrap()),
                    Err(error) => panic!("{error}; received so far: {received}"),
                }
            }
            writer.shutdown(Shutdown::Write).unwrap();
            while let Ok(chunk) = receiver.receive() {
                received.push_str(&String::from_utf8(chunk).unwrap());
            }
            received
        })
        .await
        .unwrap();

        let parsed = lines
            .lines()
            .map(|line| serde_json::from_str::<serde_json::Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(parsed[0]["type"], "ready", "{lines}");
        assert!(
            parsed
                .iter()
                .any(|line| line["request_id"] == "remote-list"
                    && line["type"] == "command_completed")
        );
        assert!(parsed
            .iter()
            .any(|line| line["event"]["SessionListLoaded"]["rows"]
                .as_array()
                .is_some_and(|rows| rows.len() == 1)));
        tokio::time::timeout(std::time::Duration::from_secs(10), serving)
            .await
            .unwrap()
            .unwrap()
            .unwrap();
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn lines_cross_the_bridge_in_both_directions_until_hang_up() {
        let (pc, phone) = (identity(), identity());
        let (pc_key, phone_key) = (pc.public_key(), phone.public_key());
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();

        // The serving device echoes each line it receives, upper-cased.
        let serving = tokio::spawn(async move {
            let (stream, _) = tokio::task::spawn_blocking(move || listener.accept())
                .await
                .unwrap()
                .unwrap();
            let writer = stream.try_clone().unwrap();
            let channel = tokio::task::spawn_blocking(move || {
                SecureChannel::accept(stream, &phone, |key| *key == pc_key)
            })
            .await
            .unwrap()
            .unwrap();
            let (sender, receiver) = channel.split(writer.try_clone().unwrap());
            let (reader, mut writer_half) = tokio::io::split(bridge_channel(receiver, sender));
            let mut lines = tokio::io::BufReader::new(reader).lines();
            let mut count = 0;
            while let Some(line) = lines.next_line().await.unwrap() {
                writer_half
                    .write_all(format!("{}\n", line.to_uppercase()).as_bytes())
                    .await
                    .unwrap();
                count += 1;
            }
            drop(writer_half);
            writer.shutdown(Shutdown::Both).ok();
            count
        });

        let client = tokio::task::spawn_blocking(move || {
            let stream = TcpStream::connect(address).unwrap();
            let writer = stream.try_clone().unwrap();
            let channel = SecureChannel::connect(stream, &pc, |key| *key == phone_key).unwrap();
            let (mut sender, mut receiver) = channel.split(writer.try_clone().unwrap());
            // A message split across frames still arrives as one line.
            sender.send(b"{\"hel").unwrap();
            sender.send(b"lo\":1}\n{\"second\":2}\n").unwrap();
            let mut received = Vec::new();
            while received.iter().filter(|byte| **byte == b'\n').count() < 2 {
                received.extend(receiver.receive().unwrap());
            }
            writer.shutdown(Shutdown::Write).unwrap();
            let closed = loop {
                match receiver.receive() {
                    Ok(chunk) => received.extend(chunk),
                    Err(error) => break error,
                }
            };
            (String::from_utf8(received).unwrap(), closed)
        });

        let (received, closed) = client.await.unwrap();
        assert_eq!(received, "{\"HELLO\":1}\n{\"SECOND\":2}\n");
        assert!(matches!(closed, ChannelError::Closed));
        assert_eq!(serving.await.unwrap(), 2);
    }
}
