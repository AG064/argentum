//! Settings > Devices: pairing this install with another on the local network.
//!
//! The exchange runs on its own thread. Both devices show the same six digits
//! and the owner confirms on each; only that confirmation saves the other
//! device. Being paired grants nothing by itself.

use std::net::{TcpListener, TcpStream, ToSocketAddrs, UdpSocket};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use argentum_devices::exchange::{exchange, PairingOutcome};
use argentum_devices::DeviceIdentity;
use argentum_platform::SecretStore;
use argentum_store::{PairedDeviceRecord, Store};
use argentum_ui::{App, DeviceView, MainWindow, UiHandle};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};

pub const PAIRING_PORT: u16 = 47_910;
const PAIRING_WAIT: Duration = Duration::from_secs(120);

#[derive(Clone)]
pub struct DevicesContext {
    pub database: Option<PathBuf>,
    pub secrets: Arc<dyn SecretStore>,
    /// Present when paired devices may be offered control of this app.
    pub remote: Option<crate::remote_control::RemoteHost>,
}

pub fn connect(ui: &UiHandle, context: DevicesContext) {
    let app = ui.window().global::<App>();
    let pending: Arc<Mutex<Option<PairingOutcome>>> = Arc::default();

    {
        let context = context.clone();
        let weak = ui.weak_window();
        app.on_device_refresh(move || {
            if let Some(window) = weak.upgrade() {
                refresh(&window, &context);
            }
        });
    }
    {
        let context = context.clone();
        let weak = ui.weak_window();
        let pending = pending.clone();
        app.on_device_listen(move || {
            let address = local_address().map_or_else(
                || format!("port {PAIRING_PORT}"),
                |ip| format!("{ip}:{PAIRING_PORT}"),
            );
            set_state(
                &weak,
                "waiting",
                "",
                &format!("Waiting up to two minutes. On the other device, connect to {address}."),
            );
            let (context, weak, pending) = (context.clone(), weak.clone(), pending.clone());
            std::thread::spawn(move || {
                let result = accept_one().and_then(|stream| run(stream, &context, false));
                finish(&weak, &pending, result);
            });
        });
    }
    {
        let context = context.clone();
        let weak = ui.weak_window();
        let pending = pending.clone();
        app.on_device_connect(move |address| {
            set_state(
                &weak,
                "connecting",
                "",
                &format!("Connecting to {address}..."),
            );
            let (context, weak, pending) = (context.clone(), weak.clone(), pending.clone());
            let address = address.trim().to_owned();
            std::thread::spawn(move || {
                let result = connect_to(&address).and_then(|stream| run(stream, &context, true));
                finish(&weak, &pending, result);
            });
        });
    }
    {
        let context = context.clone();
        let weak = ui.weak_window();
        let pending = pending.clone();
        app.on_device_confirm(move |matched, name| {
            let Some(window) = weak.upgrade() else { return };
            let outcome = pending.lock().ok().and_then(|mut slot| slot.take());
            let app = window.global::<App>();
            app.set_device_pair_code(SharedString::default());
            app.set_device_pair_detail(SharedString::default());
            app.set_device_pair_state("idle".into());
            let (Some(outcome), true) = (outcome, matched) else {
                return;
            };
            let name = if name.trim().is_empty() {
                "Paired device".to_owned()
            } else {
                name.trim().to_owned()
            };
            let saved = open_store(&context).and_then(|store| {
                store
                    .save_paired_device(&PairedDeviceRecord {
                        public_key: hex(&outcome.peer.public_key),
                        fingerprint: outcome.peer.fingerprint.clone(),
                        name,
                        paired_at: argentum_domain::now().unix_timestamp(),
                    })
                    .map_err(|error| error.to_string())
            });
            if let Err(error) = saved {
                app.set_device_pair_state("error".into());
                app.set_device_pair_detail(error.into());
            }
            refresh(&window, &context);
        });
    }
    if let Some(remote) = context.remote.clone() {
        app.set_device_remote_available(true);
        let context = context.clone();
        let weak = ui.weak_window();
        let server = crate::remote_control::RemoteServer::default();
        app.on_device_set_remote(move |on| {
            let Some(window) = weak.upgrade() else { return };
            let app = window.global::<App>();
            if !on {
                server.stop();
                app.set_device_remote_on(false);
                app.set_device_remote_detail(SharedString::default());
                return;
            }
            let keys = open_store(&context)
                .and_then(|store| store.paired_devices().map_err(|error| error.to_string()))
                .map(|devices| {
                    devices
                        .iter()
                        .filter_map(|device| crate::remote_control::decode_key(&device.public_key))
                        .collect::<Vec<_>>()
                });
            let identity = DeviceIdentity::load_or_create(context.secrets.as_ref())
                .map_err(|error| error.to_string());
            match (keys, identity) {
                (Ok(keys), Ok(identity)) if !keys.is_empty() => {
                    match server.start(remote.clone(), identity, keys) {
                        Ok(port) => {
                            let address = local_address().map_or_else(
                                || format!("port {port}"),
                                |ip| format!("{ip}:{port}"),
                            );
                            app.set_device_remote_on(true);
                            app.set_device_remote_detail(
                                format!("On. Paired devices can connect to {address}.").into(),
                            );
                        }
                        Err(error) => {
                            app.set_device_remote_on(false);
                            app.set_device_remote_detail(
                                format!("Could not start: {error}").into(),
                            );
                        }
                    }
                }
                (Ok(_), Ok(_)) => {
                    app.set_device_remote_on(false);
                    app.set_device_remote_detail("Pair a device first.".into());
                }
                (Err(error), _) | (_, Err(error)) => {
                    app.set_device_remote_on(false);
                    app.set_device_remote_detail(format!("Could not start: {error}").into());
                }
            }
        });
    }
    {
        let weak = ui.weak_window();
        app.on_device_forget(move |fingerprint| {
            let Some(window) = weak.upgrade() else { return };
            if let Ok(store) = open_store(&context) {
                let _ = store.forget_paired_device(fingerprint.as_str());
            }
            refresh(&window, &context);
        });
    }
}

fn refresh(window: &MainWindow, context: &DevicesContext) {
    let app = window.global::<App>();
    // Opening the page writes nothing; the key is made when pairing starts.
    match DeviceIdentity::load(context.secrets.as_ref()) {
        Ok(Some(identity)) => app.set_device_fingerprint(identity.fingerprint().into()),
        Ok(None) => app.set_device_fingerprint("Created when you first pair".into()),
        Err(error) => app.set_device_fingerprint(format!("Unavailable: {error}").into()),
    }
    let devices = open_store(context)
        .and_then(|store| store.paired_devices().map_err(|error| error.to_string()))
        .unwrap_or_default()
        .into_iter()
        .map(|device| DeviceView {
            name: device.name.into(),
            fingerprint: device.fingerprint.into(),
        })
        .collect::<Vec<_>>();
    app.set_devices(ModelRc::new(VecModel::from(devices)));
}

fn open_store(context: &DevicesContext) -> Result<Store, String> {
    match &context.database {
        Some(path) => Store::open(path).map_err(|error| error.to_string()),
        None => Err("This session keeps no database.".into()),
    }
}

fn run(
    stream: TcpStream,
    context: &DevicesContext,
    initiator: bool,
) -> Result<PairingOutcome, String> {
    stream
        .set_read_timeout(Some(PAIRING_WAIT))
        .and_then(|()| stream.set_write_timeout(Some(Duration::from_secs(15))))
        .map_err(|error| error.to_string())?;
    let identity = DeviceIdentity::load_or_create(context.secrets.as_ref())
        .map_err(|error| error.to_string())?;
    exchange(&stream, &identity, initiator).map_err(|error| error.to_string())
}

fn accept_one() -> Result<TcpStream, String> {
    let listener =
        TcpListener::bind(("0.0.0.0", PAIRING_PORT)).map_err(|error| error.to_string())?;
    listener
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    let deadline = Instant::now() + PAIRING_WAIT;
    loop {
        match listener.accept() {
            Ok((stream, _)) => {
                stream
                    .set_nonblocking(false)
                    .map_err(|error| error.to_string())?;
                return Ok(stream);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if Instant::now() >= deadline {
                    return Err("No device connected within two minutes.".into());
                }
                std::thread::sleep(Duration::from_millis(200));
            }
            Err(error) => return Err(error.to_string()),
        }
    }
}

fn connect_to(address: &str) -> Result<TcpStream, String> {
    let address = if address.contains(':') {
        address.to_owned()
    } else {
        format!("{address}:{PAIRING_PORT}")
    };
    let resolved = address
        .to_socket_addrs()
        .map_err(|_| format!("{address} is not an address."))?
        .next()
        .ok_or_else(|| format!("{address} is not an address."))?;
    TcpStream::connect_timeout(&resolved, Duration::from_secs(15))
        .map_err(|error| error.to_string())
}

/// The address other devices on this network would use. Connecting a UDP
/// socket sends nothing; it only picks the outgoing interface.
fn local_address() -> Option<std::net::IpAddr> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("192.0.2.1:9").ok()?;
    Some(socket.local_addr().ok()?.ip())
}

fn finish(
    weak: &slint::Weak<MainWindow>,
    pending: &Arc<Mutex<Option<PairingOutcome>>>,
    result: Result<PairingOutcome, String>,
) {
    match result {
        Ok(outcome) => {
            let code = outcome.comparison_code.clone();
            let detail = format!("Other device: {}", outcome.peer.fingerprint);
            if let Ok(mut slot) = pending.lock() {
                *slot = Some(outcome);
            }
            set_state(weak, "code", &code, &detail);
        }
        Err(error) => set_state(
            weak,
            "error",
            "",
            &format!("Pairing did not finish: {error}"),
        ),
    }
}

fn set_state(weak: &slint::Weak<MainWindow>, state: &str, code: &str, detail: &str) {
    let (weak, state, code, detail) = (
        weak.clone(),
        state.to_owned(),
        code.to_owned(),
        detail.to_owned(),
    );
    let _ = slint::invoke_from_event_loop(move || {
        if let Some(window) = weak.upgrade() {
            let app = window.global::<App>();
            app.set_device_pair_state(state.into());
            app.set_device_pair_code(code.into());
            app.set_device_pair_detail(detail.into());
        }
    });
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
