//! Serves this app to paired devices while the owner allows it.
//!
//! Each connection must open a secure channel from a paired device. It then
//! becomes a second frontend for this app's runtime, limited to the commands
//! in `argentum_cli::remote_app::allowed_remotely`. Turning the setting off
//! stops accepting and closes every open connection.

use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use argentum_cli::remote::bridge_channel;
use argentum_cli::remote_app::serve_remote_client;
use argentum_cli::InProcessClient;
use argentum_devices::channel::SecureChannel;
use argentum_devices::DeviceIdentity;

/// The default port for remote sessions between paired devices.
pub const SESSION_PORT: u16 = 47_911;
const HANDSHAKE_WAIT: Duration = Duration::from_secs(15);

/// What a connection needs from the running app.
#[derive(Clone)]
pub struct RemoteHost {
    pub client: InProcessClient,
    pub workspace: PathBuf,
    pub runtime: tokio::runtime::Handle,
}

#[derive(Default)]
struct ServerState {
    stop: Option<Arc<AtomicBool>>,
    port: Option<u16>,
    connections: Vec<TcpStream>,
}

/// The on and off switch. Cloning shares the same server.
#[derive(Clone, Default)]
pub struct RemoteServer {
    state: Arc<Mutex<ServerState>>,
}

impl RemoteServer {
    /// Starts accepting paired devices, or keeps the running server.
    pub fn start(
        &self,
        host: RemoteHost,
        identity: DeviceIdentity,
        paired: Vec<[u8; 32]>,
    ) -> Result<u16, String> {
        self.start_on(SESSION_PORT, host, identity, paired)
    }

    /// Like [`Self::start`] on a chosen port; 0 picks a free one. Returns the
    /// port in use.
    fn start_on(
        &self,
        port: u16,
        host: RemoteHost,
        identity: DeviceIdentity,
        paired: Vec<[u8; 32]>,
    ) -> Result<u16, String> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| "remote control is unavailable")?;
        if let Some(port) = state.port.filter(|_| state.stop.is_some()) {
            return Ok(port);
        }
        let listener = TcpListener::bind(("0.0.0.0", port)).map_err(|error| error.to_string())?;
        let port = listener
            .local_addr()
            .map_err(|error| error.to_string())?
            .port();
        state.port = Some(port);
        listener
            .set_nonblocking(true)
            .map_err(|error| error.to_string())?;
        let stop = Arc::new(AtomicBool::new(false));
        state.stop = Some(Arc::clone(&stop));
        let shared = Arc::clone(&self.state);
        let identity = Arc::new(identity);
        std::thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        if let Ok(copy) = stream.try_clone() {
                            if let Ok(mut state) = shared.lock() {
                                state.connections.push(copy);
                            }
                        }
                        let (host, identity, paired) =
                            (host.clone(), Arc::clone(&identity), paired.clone());
                        std::thread::spawn(move || serve_one(stream, &host, &identity, &paired));
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(Duration::from_millis(200));
                    }
                    Err(_) => break,
                }
            }
        });
        Ok(port)
    }

    /// Stops accepting and closes every open connection.
    pub fn stop(&self) {
        if let Ok(mut state) = self.state.lock() {
            if let Some(stop) = state.stop.take() {
                stop.store(true, Ordering::SeqCst);
            }
            for connection in state.connections.drain(..) {
                let _ = connection.shutdown(Shutdown::Both);
            }
        }
    }
}

fn serve_one(stream: TcpStream, host: &RemoteHost, identity: &DeviceIdentity, paired: &[[u8; 32]]) {
    if stream.set_nonblocking(false).is_err()
        || stream.set_read_timeout(Some(HANDSHAKE_WAIT)).is_err()
    {
        return;
    }
    let Ok(writer) = stream.try_clone() else {
        return;
    };
    let Ok(reader) = stream.try_clone() else {
        return;
    };
    // An unpaired device is refused here and learns nothing else.
    let Ok(channel) = SecureChannel::accept(reader, identity, |key| paired.contains(key)) else {
        let _ = stream.shutdown(Shutdown::Both);
        return;
    };
    if stream.set_read_timeout(None).is_err() {
        return;
    }
    let (sender, receiver) = channel.split(writer);
    let host = host.clone();
    let _ = host.runtime.clone().block_on(async move {
        let (read, write) = tokio::io::split(bridge_channel(receiver, sender));
        serve_remote_client(
            host.client,
            host.workspace,
            tokio::io::BufReader::new(read),
            write,
        )
        .await
    });
    let _ = stream.shutdown(Shutdown::Both);
}

/// A stored public key, 64 hex digits, as bytes.
pub fn decode_key(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 || !hex.is_ascii() {
        return None;
    }
    (0..64)
        .step_by(2)
        .map(|index| u8::from_str_radix(&hex[index..index + 2], 16).ok())
        .collect::<Option<Vec<u8>>>()?
        .try_into()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use argentum_cli::protocol::RequestEnvelope;
    use argentum_cli::{CommandHost, HostConfig};
    use argentum_domain::AppCommand;
    use argentum_platform::{PlatformError, SecretStore};
    use argentum_security::SecretValue;
    use std::collections::HashMap;

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

    #[test]
    fn stored_keys_decode_only_when_well_formed() {
        assert_eq!(decode_key(&"0f".repeat(32)), Some([0x0f; 32]));
        assert_eq!(decode_key("0f"), None);
        assert_eq!(decode_key(&"zz".repeat(32)), None);
    }

    /// A paired phone drives the app, an unpaired one is refused, and turning
    /// the setting off closes the connection.
    #[test]
    fn paired_devices_drive_the_app_until_it_is_turned_off() {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();
        let workspace = tempfile::tempdir().unwrap();
        let host = runtime
            .block_on(CommandHost::start_composed(HostConfig::in_memory(
                workspace.path(),
            )))
            .unwrap();
        let (app, phone, stranger) = (identity(), identity(), identity());
        let phone_key = phone.public_key();
        let app_key = app.public_key();
        let server = RemoteServer::default();
        let port = server
            .start_on(
                0,
                RemoteHost {
                    client: host.client(),
                    workspace: workspace.path().to_path_buf(),
                    runtime: runtime.handle().clone(),
                },
                app,
                vec![phone_key],
            )
            .unwrap();

        let connect = |who: &DeviceIdentity| {
            let stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(20)))
                .unwrap();
            let writer = stream.try_clone().unwrap();
            SecureChannel::connect(stream.try_clone().unwrap(), who, |key| *key == app_key)
                .map(|channel| (stream, channel.split(writer)))
        };
        assert!(connect(&stranger).is_err());

        let (_stream, (mut sender, mut receiver)) = connect(&phone).unwrap();
        let request = serde_json::to_string(&RequestEnvelope::command(
            "list",
            AppCommand::ListSessions {
                query: String::new(),
                filter: Default::default(),
                limit: 20,
                cursor: None,
            },
        ))
        .unwrap();
        sender.send(format!("{request}\n").as_bytes()).unwrap();
        let mut received = String::new();
        while !received.contains("SessionListLoaded") {
            received.push_str(&String::from_utf8(receiver.receive().unwrap()).unwrap());
        }
        assert!(received.starts_with("{\"protocol_version\""));
        assert!(received.contains("\"type\":\"ready\""));

        server.stop();
        let closed = loop {
            match receiver.receive() {
                Ok(_) => continue,
                Err(error) => break error,
            }
        };
        assert!(
            !matches!(closed, argentum_devices::channel::ChannelError::Io(ref e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut)
        );
        drop(host);
    }
}
