// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Runs the pairing exchange over a byte stream, such as a TCP connection on
//! the local network. Each message is one JSON line of bounded length.
//!
//! The stream is not trusted. Every message is checked, and the exchange ends
//! with the comparison code for the owner to confirm on both devices; nothing is
//! saved here.

use std::io::{BufRead, BufReader, Read, Write};

use serde::{Deserialize, Serialize};

use crate::pairing::{PairedDevice, PairingError, PairingHello, PairingTranscript};
use crate::{DeviceError, DeviceIdentity};

/// Longer lines are refused before parsing.
const MAX_LINE_BYTES: usize = 512;

#[derive(Debug, thiserror::Error)]
pub enum ExchangeError {
    #[error("the connection failed during pairing")]
    Io(#[from] std::io::Error),
    #[error("the other device sent a message that is not part of pairing")]
    Malformed,
    #[error(transparent)]
    Pairing(#[from] PairingError),
    #[error(transparent)]
    Device(#[from] DeviceError),
}

/// What the owner sees before confirming: the code both screens show, and the
/// device that would be paired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairingOutcome {
    pub comparison_code: String,
    pub peer: PairedDevice,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct HelloMessage {
    public_key: String,
    nonce: String,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SignatureMessage {
    signature: String,
}

/// Runs the exchange. The device that connected is the initiator.
pub fn exchange<S: Read + Write>(
    stream: S,
    identity: &DeviceIdentity,
    initiator: bool,
) -> Result<PairingOutcome, ExchangeError> {
    let mut reader = BufReader::new(stream);
    let mine = PairingHello::new(identity)?;
    let theirs = if initiator {
        send(reader.get_mut(), &hello_message(&mine))?;
        hello_from(&receive::<_, HelloMessage>(&mut reader)?)?
    } else {
        let theirs = hello_from(&receive::<_, HelloMessage>(&mut reader)?)?;
        send(reader.get_mut(), &hello_message(&mine))?;
        theirs
    };
    let transcript = if initiator {
        PairingTranscript::new(mine, theirs)?
    } else {
        PairingTranscript::new(theirs, mine)?
    };

    send(
        reader.get_mut(),
        &SignatureMessage {
            signature: crate::encode_hex(&transcript.sign(identity)),
        },
    )?;
    let signature = receive::<_, SignatureMessage>(&mut reader)?;
    let signature: [u8; 64] = crate::decode_hex(&signature.signature)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or(ExchangeError::Malformed)?;
    transcript.verify_peer(&theirs.public_key, &signature)?;

    Ok(PairingOutcome {
        comparison_code: transcript.comparison_code(),
        peer: transcript.peer(&identity.public_key()),
    })
}

fn hello_message(hello: &PairingHello) -> HelloMessage {
    HelloMessage {
        public_key: crate::encode_hex(&hello.public_key),
        nonce: crate::encode_hex(&hello.nonce),
    }
}

fn hello_from(message: &HelloMessage) -> Result<PairingHello, ExchangeError> {
    let key = |text: &str| -> Option<[u8; 32]> {
        crate::decode_hex(text).and_then(|bytes| bytes.try_into().ok())
    };
    Ok(PairingHello {
        public_key: key(&message.public_key).ok_or(ExchangeError::Malformed)?,
        nonce: key(&message.nonce).ok_or(ExchangeError::Malformed)?,
    })
}

fn send<W: Write, T: Serialize>(writer: &mut W, message: &T) -> Result<(), ExchangeError> {
    let mut line = serde_json::to_vec(message).map_err(|_| ExchangeError::Malformed)?;
    line.push(b'\n');
    writer.write_all(&line)?;
    writer.flush()?;
    Ok(())
}

fn receive<R: BufRead, T: for<'de> Deserialize<'de>>(reader: &mut R) -> Result<T, ExchangeError> {
    let mut line = Vec::new();
    // Read at most one byte past the limit, so an endless line cannot grow.
    reader
        .take(MAX_LINE_BYTES as u64 + 1)
        .read_until(b'\n', &mut line)?;
    if line.last() != Some(&b'\n') || line.len() > MAX_LINE_BYTES {
        return Err(ExchangeError::Malformed);
    }
    serde_json::from_slice(&line).map_err(|_| ExchangeError::Malformed)
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

    #[test]
    fn two_devices_pair_over_a_local_connection() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let phone = std::thread::spawn(move || {
            let phone = identity();
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            (
                phone.fingerprint(),
                exchange(stream, &phone, false).unwrap(),
            )
        });
        let pc = identity();
        let stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let on_pc = exchange(stream, &pc, true).unwrap();
        let (phone_fingerprint, on_phone) = phone.join().unwrap();

        assert_eq!(on_pc.comparison_code, on_phone.comparison_code);
        assert_eq!(on_pc.peer.fingerprint, phone_fingerprint);
        assert_eq!(on_phone.peer.fingerprint, pc.fingerprint());
    }

    #[test]
    fn garbage_and_oversized_lines_are_refused() {
        let pc = identity();
        for incoming in [
            b"not json\n".to_vec(),
            b"{\"public_key\":\"00\",\"nonce\":\"00\"}\n".to_vec(),
            [vec![b'{'; 600], vec![b'\n']].concat(),
            b"{\"public_key\":\"00\"".to_vec(),
        ] {
            let stream = FakeStream {
                incoming: std::io::Cursor::new(incoming),
                outgoing: Vec::new(),
            };
            assert!(matches!(
                exchange(stream, &pc, true),
                Err(ExchangeError::Malformed | ExchangeError::Io(_))
            ));
        }
    }

    #[test]
    fn a_signature_from_another_key_fails() {
        // The responder answers with a valid hello but signs with a different key.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let liar = std::thread::spawn(move || {
            let (claimed, actual) = (identity(), identity());
            let (stream, _) = listener.accept().unwrap();
            let mut reader = BufReader::new(stream);
            let theirs = hello_from(&receive::<_, HelloMessage>(&mut reader).unwrap()).unwrap();
            let mine = PairingHello::new(&claimed).unwrap();
            send(reader.get_mut(), &hello_message(&mine)).unwrap();
            let transcript = PairingTranscript::new(theirs, mine).unwrap();
            let _ = receive::<_, SignatureMessage>(&mut reader);
            send(
                reader.get_mut(),
                &SignatureMessage {
                    signature: crate::encode_hex(&transcript.sign(&actual)),
                },
            )
            .unwrap();
        });
        let stream = TcpStream::connect(address).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        assert!(matches!(
            exchange(stream, &identity(), true),
            Err(ExchangeError::Pairing(PairingError::BadSignature))
        ));
        liar.join().unwrap();
    }

    struct FakeStream {
        incoming: std::io::Cursor<Vec<u8>>,
        outgoing: Vec<u8>,
    }

    impl Read for FakeStream {
        fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
            self.incoming.read(buffer)
        }
    }

    impl Write for FakeStream {
        fn write(&mut self, buffer: &[u8]) -> std::io::Result<usize> {
            self.outgoing.write(buffer)
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
}
