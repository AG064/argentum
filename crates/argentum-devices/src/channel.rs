//! An encrypted, authenticated connection between two paired devices.
//!
//! Handshake, as bounded JSON lines:
//! 1. The connecting device sends its identity key and a fresh X25519 key.
//! 2. The other replies with its own two keys and its signature over the
//!    transcript of all four keys.
//! 3. The connecting device checks that the other is paired and its signature
//!    is valid, then sends its own signature, which the other checks the same way.
//!
//! Both derive one ChaCha20-Poly1305 key per direction with HKDF-SHA256 from
//! the X25519 secret, salted with the transcript hash. Each frame is a 4-byte
//! length and a sealed message whose nonce is a per-direction counter, so a
//! reordered, replayed or altered frame fails to open.
//!
//! A secure connection authenticates the device. It grants nothing: what a
//! paired device may ask for stays with each device's workspace policy.

use std::io::{BufRead, BufReader, Read, Write};

use ring::aead::{Aad, LessSafeKey, Nonce, UnboundKey, CHACHA20_POLY1305};
use ring::agreement::{self, EphemeralPrivateKey, UnparsedPublicKey, X25519};
use ring::hkdf;
use ring::rand::SystemRandom;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{decode_hex, encode_hex, fingerprint, verify, DeviceIdentity};

const LABEL: &[u8] = b"argentum-channel-v1";
const MAX_HANDSHAKE_LINE: usize = 512;
/// The largest message either side sends or accepts.
pub const MAX_MESSAGE_BYTES: usize = 1024 * 1024;

#[derive(Debug, thiserror::Error)]
pub enum ChannelError {
    #[error("the connection failed")]
    Io(#[from] std::io::Error),
    #[error("the other device sent something that is not part of the protocol")]
    Malformed,
    #[error("the other device is not paired with this one")]
    NotPaired,
    #[error("the other device could not prove its identity")]
    BadSignature,
    #[error("a message failed its integrity check")]
    Integrity,
    #[error("a message is larger than allowed")]
    TooLarge,
    #[error("a secure connection could not be set up")]
    Crypto,
    #[error("the other device closed the connection")]
    Closed,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Hello {
    identity: String,
    ephemeral: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    signature: Option<String>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Proof {
    signature: String,
}

pub struct SecureChannel<S: Read + Write> {
    stream: BufReader<S>,
    peer: [u8; 32],
    sealing: LessSafeKey,
    opening: LessSafeKey,
    sent: u64,
    received: u64,
}

impl<S: Read + Write> std::fmt::Debug for SecureChannel<S> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SecureChannel")
            .field("peer", &fingerprint(&self.peer))
            .finish_non_exhaustive()
    }
}

impl<S: Read + Write> SecureChannel<S> {
    /// Opens a channel as the connecting device.
    pub fn connect(
        stream: S,
        identity: &DeviceIdentity,
        is_paired: impl Fn(&[u8; 32]) -> bool,
    ) -> Result<Self, ChannelError> {
        let mut stream = BufReader::new(stream);
        let (secret, ephemeral) = ephemeral()?;
        send_line(
            stream.get_mut(),
            &Hello {
                identity: encode_hex(&identity.public_key()),
                ephemeral: encode_hex(&ephemeral),
                signature: None,
            },
        )?;
        let reply: Hello = receive_line(&mut stream)?;
        let peer = key(&reply.identity)?;
        let peer_ephemeral = key(&reply.ephemeral)?;
        if !is_paired(&peer) {
            return Err(ChannelError::NotPaired);
        }
        let transcript = transcript(&identity.public_key(), &ephemeral, &peer, &peer_ephemeral);
        let signature = signature(reply.signature.as_deref().ok_or(ChannelError::Malformed)?)?;
        if !verify(&peer, &transcript, &signature) {
            return Err(ChannelError::BadSignature);
        }
        send_line(
            stream.get_mut(),
            &Proof {
                signature: encode_hex(&identity.sign(&transcript)),
            },
        )?;
        let (sealing, opening) = session_keys(secret, &peer_ephemeral, &transcript, true)?;
        Ok(Self::ready(stream, peer, sealing, opening))
    }

    /// Accepts a channel from a connecting device.
    pub fn accept(
        stream: S,
        identity: &DeviceIdentity,
        is_paired: impl Fn(&[u8; 32]) -> bool,
    ) -> Result<Self, ChannelError> {
        let mut stream = BufReader::new(stream);
        let hello: Hello = receive_line(&mut stream)?;
        if hello.signature.is_some() {
            return Err(ChannelError::Malformed);
        }
        let peer = key(&hello.identity)?;
        let peer_ephemeral = key(&hello.ephemeral)?;
        // An unpaired device learns nothing beyond the refusal.
        if !is_paired(&peer) {
            return Err(ChannelError::NotPaired);
        }
        let (secret, ephemeral) = ephemeral()?;
        let transcript = transcript(&peer, &peer_ephemeral, &identity.public_key(), &ephemeral);
        send_line(
            stream.get_mut(),
            &Hello {
                identity: encode_hex(&identity.public_key()),
                ephemeral: encode_hex(&ephemeral),
                signature: Some(encode_hex(&identity.sign(&transcript))),
            },
        )?;
        let proof: Proof = receive_line(&mut stream)?;
        if !verify(&peer, &transcript, &signature(&proof.signature)?) {
            return Err(ChannelError::BadSignature);
        }
        let (sealing, opening) = session_keys(secret, &peer_ephemeral, &transcript, false)?;
        Ok(Self::ready(stream, peer, sealing, opening))
    }

    fn ready(
        stream: BufReader<S>,
        peer: [u8; 32],
        sealing: LessSafeKey,
        opening: LessSafeKey,
    ) -> Self {
        Self {
            stream,
            peer,
            sealing,
            opening,
            sent: 0,
            received: 0,
        }
    }

    /// The other device's identity key.
    pub fn peer(&self) -> [u8; 32] {
        self.peer
    }

    pub fn send(&mut self, message: &[u8]) -> Result<(), ChannelError> {
        seal_frame(
            self.stream.get_mut(),
            &self.sealing,
            &mut self.sent,
            message,
        )
    }

    pub fn receive(&mut self) -> Result<Vec<u8>, ChannelError> {
        open_frame(&mut self.stream, &self.opening, &mut self.received)
    }

    /// Splits the channel so one thread can send while another receives.
    /// `writer` must write to the same connection, such as a cloned socket.
    pub fn split<W: Write>(self, writer: W) -> (ChannelSender<W>, ChannelReceiver<S>) {
        (
            ChannelSender {
                writer,
                sealing: self.sealing,
                sent: self.sent,
            },
            ChannelReceiver {
                reader: self.stream,
                opening: self.opening,
                received: self.received,
            },
        )
    }
}

/// The sending half of a split channel.
pub struct ChannelSender<W: Write> {
    writer: W,
    sealing: LessSafeKey,
    sent: u64,
}

impl<W: Write> ChannelSender<W> {
    pub fn send(&mut self, message: &[u8]) -> Result<(), ChannelError> {
        seal_frame(&mut self.writer, &self.sealing, &mut self.sent, message)
    }
}

/// The receiving half of a split channel.
pub struct ChannelReceiver<R: Read> {
    reader: BufReader<R>,
    opening: LessSafeKey,
    received: u64,
}

impl<R: Read> ChannelReceiver<R> {
    /// Returns [`ChannelError::Closed`] once the other device has hung up.
    pub fn receive(&mut self) -> Result<Vec<u8>, ChannelError> {
        open_frame(&mut self.reader, &self.opening, &mut self.received)
    }
}

fn seal_frame<W: Write>(
    writer: &mut W,
    sealing: &LessSafeKey,
    sent: &mut u64,
    message: &[u8],
) -> Result<(), ChannelError> {
    if message.len() > MAX_MESSAGE_BYTES {
        return Err(ChannelError::TooLarge);
    }
    let nonce = counter_nonce(*sent)?;
    *sent += 1;
    let mut sealed = message.to_vec();
    sealing
        .seal_in_place_append_tag(nonce, Aad::empty(), &mut sealed)
        .map_err(|_| ChannelError::Crypto)?;
    let length = u32::try_from(sealed.len()).map_err(|_| ChannelError::TooLarge)?;
    writer.write_all(&length.to_be_bytes())?;
    writer.write_all(&sealed)?;
    writer.flush()?;
    Ok(())
}

fn open_frame<R: Read>(
    reader: &mut R,
    opening: &LessSafeKey,
    received: &mut u64,
) -> Result<Vec<u8>, ChannelError> {
    let mut length = [0_u8; 4];
    match reader.read_exact(&mut length) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => {
            return Err(ChannelError::Closed)
        }
        Err(error) => return Err(error.into()),
    }
    let length = u32::from_be_bytes(length) as usize;
    if length > MAX_MESSAGE_BYTES + CHACHA20_POLY1305.tag_len() {
        return Err(ChannelError::TooLarge);
    }
    let mut sealed = vec![0_u8; length];
    reader.read_exact(&mut sealed)?;
    let nonce = counter_nonce(*received)?;
    *received += 1;
    let opened = opening
        .open_in_place(nonce, Aad::empty(), &mut sealed)
        .map_err(|_| ChannelError::Integrity)?;
    Ok(opened.to_vec())
}

fn ephemeral() -> Result<(EphemeralPrivateKey, [u8; 32]), ChannelError> {
    let secret = EphemeralPrivateKey::generate(&X25519, &SystemRandom::new())
        .map_err(|_| ChannelError::Crypto)?;
    let public = secret
        .compute_public_key()
        .map_err(|_| ChannelError::Crypto)?;
    let mut bytes = [0_u8; 32];
    bytes.copy_from_slice(public.as_ref());
    Ok((secret, bytes))
}

fn transcript(
    initiator: &[u8; 32],
    initiator_ephemeral: &[u8; 32],
    responder: &[u8; 32],
    responder_ephemeral: &[u8; 32],
) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(LABEL.len() + 128);
    bytes.extend_from_slice(LABEL);
    for part in [
        initiator,
        initiator_ephemeral,
        responder,
        responder_ephemeral,
    ] {
        bytes.extend_from_slice(part);
    }
    bytes
}

/// One key for each direction, so the two sides never seal with the same key.
fn session_keys(
    secret: EphemeralPrivateKey,
    peer_ephemeral: &[u8; 32],
    transcript: &[u8],
    initiator: bool,
) -> Result<(LessSafeKey, LessSafeKey), ChannelError> {
    let salt = Sha256::digest(transcript);
    agreement::agree_ephemeral(
        secret,
        &UnparsedPublicKey::new(&X25519, peer_ephemeral),
        |shared| {
            let prk = hkdf::Salt::new(hkdf::HKDF_SHA256, &salt).extract(shared);
            let derive = |info: &[u8]| -> Result<LessSafeKey, ChannelError> {
                let info = [info];
                let okm = prk
                    .expand(&info, &CHACHA20_POLY1305)
                    .map_err(|_| ChannelError::Crypto)?;
                Ok(LessSafeKey::new(UnboundKey::from(okm)))
            };
            let to_responder = derive(b"initiator to responder")?;
            let to_initiator = derive(b"responder to initiator")?;
            Ok(if initiator {
                (to_responder, to_initiator)
            } else {
                (to_initiator, to_responder)
            })
        },
    )
    .map_err(|_| ChannelError::Crypto)?
}

fn counter_nonce(counter: u64) -> Result<Nonce, ChannelError> {
    let mut nonce = [0_u8; 12];
    nonce[4..].copy_from_slice(&counter.to_be_bytes());
    Ok(Nonce::assume_unique_for_key(nonce))
}

fn key(text: &str) -> Result<[u8; 32], ChannelError> {
    decode_hex(text)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or(ChannelError::Malformed)
}

fn signature(text: &str) -> Result<[u8; 64], ChannelError> {
    decode_hex(text)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or(ChannelError::Malformed)
}

fn send_line<W: Write, T: Serialize>(writer: &mut W, message: &T) -> Result<(), ChannelError> {
    let mut line = serde_json::to_vec(message).map_err(|_| ChannelError::Malformed)?;
    line.push(b'\n');
    writer.write_all(&line)?;
    writer.flush()?;
    Ok(())
}

fn receive_line<R: BufRead, T: for<'de> Deserialize<'de>>(
    reader: &mut R,
) -> Result<T, ChannelError> {
    let mut line = Vec::new();
    reader
        .take(MAX_HANDSHAKE_LINE as u64 + 1)
        .read_until(b'\n', &mut line)?;
    if line.last() != Some(&b'\n') || line.len() > MAX_HANDSHAKE_LINE {
        return Err(ChannelError::Malformed);
    }
    serde_json::from_slice(&line).map_err(|_| ChannelError::Malformed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::MemoryStore;
    use std::net::{TcpListener, TcpStream};
    use std::time::Duration;

    fn identity() -> DeviceIdentity {
        DeviceIdentity::load_or_create(&MemoryStore::default()).unwrap()
    }

    fn pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (server, _) = listener.accept().unwrap();
        for stream in [&client, &server] {
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
        }
        (client, server)
    }

    #[test]
    fn paired_devices_exchange_messages_both_ways() {
        let (pc, phone) = (identity(), identity());
        let (pc_key, phone_key) = (pc.public_key(), phone.public_key());
        let (client, server) = pair();
        let phone_side = std::thread::spawn(move || {
            let mut channel = SecureChannel::accept(server, &phone, |key| *key == pc_key).unwrap();
            let request = channel.receive().unwrap();
            channel
                .send(&[request.as_slice(), b" received"].concat())
                .unwrap();
            channel.peer()
        });
        let mut channel = SecureChannel::connect(client, &pc, |key| *key == phone_key).unwrap();
        channel.send(b"hello phone").unwrap();
        assert_eq!(channel.receive().unwrap(), b"hello phone received");
        assert_eq!(channel.peer(), phone_key);
        assert_eq!(phone_side.join().unwrap(), pc_key);
    }

    #[test]
    fn an_unpaired_device_is_refused_on_both_sides() {
        let (pc, phone) = (identity(), identity());
        let (client, server) = pair();
        let phone_side = std::thread::spawn(move || {
            SecureChannel::accept(server, &phone, |_| false).map(|_| ())
        });
        let result = SecureChannel::connect(client, &pc, |_| true);
        assert!(matches!(
            phone_side.join().unwrap(),
            Err(ChannelError::NotPaired)
        ));
        assert!(result.is_err());

        let (pc, phone) = (identity(), identity());
        let (client, server) = pair();
        std::thread::spawn(move || {
            let _ = SecureChannel::accept(server, &phone, |_| true);
        });
        assert!(matches!(
            SecureChannel::connect(client, &pc, |_| false),
            Err(ChannelError::NotPaired)
        ));
    }

    #[test]
    fn an_altered_frame_fails_its_integrity_check() {
        let (pc, phone) = (identity(), identity());
        let (pc_key, phone_key) = (pc.public_key(), phone.public_key());
        let (client, server) = pair();
        // A relay that flips one byte of the first encrypted frame.
        let relay_listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let relay_address = relay_listener.local_addr().unwrap();
        let phone_side = std::thread::spawn(move || {
            let mut channel = SecureChannel::accept(server, &phone, |key| *key == pc_key).unwrap();
            channel.receive()
        });
        let relay = std::thread::spawn(move || {
            let (mut from_pc, _) = relay_listener.accept().unwrap();
            let mut to_phone = client;
            let mut to_phone_reader = to_phone.try_clone().unwrap();
            let mut from_pc_writer = from_pc.try_clone().unwrap();
            let back = std::thread::spawn(move || {
                let _ = std::io::copy(&mut to_phone_reader, &mut from_pc_writer);
            });
            let mut handshake = [0_u8; 1];
            let mut newlines = 0;
            // Pass the two handshake lines from the PC through unchanged.
            while newlines < 2 {
                from_pc.read_exact(&mut handshake).unwrap();
                to_phone.write_all(&handshake).unwrap();
                if handshake[0] == b'\n' {
                    newlines += 1;
                }
            }
            let mut frame = [0_u8; 64];
            let read = from_pc.read(&mut frame).unwrap();
            frame[read - 1] ^= 0x01;
            to_phone.write_all(&frame[..read]).unwrap();
            drop(back);
        });
        let mut channel =
            SecureChannel::connect(TcpStream::connect(relay_address).unwrap(), &pc, |key| {
                *key == phone_key
            })
            .unwrap();
        channel.send(b"intact?").unwrap();
        assert!(matches!(
            phone_side.join().unwrap(),
            Err(ChannelError::Integrity)
        ));
        relay.join().unwrap();
    }

    #[test]
    fn oversized_messages_are_refused_before_sending() {
        let (pc, phone) = (identity(), identity());
        let (pc_key, phone_key) = (pc.public_key(), phone.public_key());
        let (client, server) = pair();
        let phone_side = std::thread::spawn(move || {
            SecureChannel::accept(server, &phone, |key| *key == pc_key).map(|_| ())
        });
        let mut channel = SecureChannel::connect(client, &pc, |key| *key == phone_key).unwrap();
        phone_side.join().unwrap().unwrap();
        assert!(matches!(
            channel.send(&vec![0_u8; MAX_MESSAGE_BYTES + 1]),
            Err(ChannelError::TooLarge)
        ));
    }

    #[test]
    fn split_halves_carry_messages_and_report_a_hang_up() {
        let (pc, phone) = (identity(), identity());
        let (pc_key, phone_key) = (pc.public_key(), phone.public_key());
        let (client, server) = pair();
        let phone_side = std::thread::spawn(move || {
            let writer = server.try_clone().unwrap();
            let channel = SecureChannel::accept(server, &phone, |key| *key == pc_key).unwrap();
            let (mut sender, mut receiver) = channel.split(writer);
            sender.send(b"first").unwrap();
            assert_eq!(receiver.receive().unwrap(), b"reply");
            sender.send(b"second").unwrap();
        });
        let writer = client.try_clone().unwrap();
        let channel = SecureChannel::connect(client, &pc, |key| *key == phone_key).unwrap();
        let (mut sender, mut receiver) = channel.split(writer);
        assert_eq!(receiver.receive().unwrap(), b"first");
        sender.send(b"reply").unwrap();
        assert_eq!(receiver.receive().unwrap(), b"second");
        phone_side.join().unwrap();
        assert!(matches!(receiver.receive(), Err(ChannelError::Closed)));
    }
}
