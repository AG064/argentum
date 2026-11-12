//! Offline check of a GitHub artifact attestation, which is a Sigstore bundle.
//!
//! A publisher's release workflow signs an in-toto statement that lists each file
//! it built with its SHA-256. The signing certificate is issued by Sigstore's
//! Fulcio CA to that exact workflow, and Sigstore's Rekor log countersigns the
//! moment it was used. This module checks all of that against trust material
//! pinned in the source, so no network is needed:
//!
//! 1. The certificate chains to the pinned Fulcio root, for code signing, at the
//!    time Rekor recorded.
//! 2. The certificate names the expected workflow and token issuer.
//! 3. The statement's signature verifies with the certificate's key.
//! 4. Rekor's signed entry timestamp verifies with the pinned Rekor key, and the
//!    log entry is for this statement, this signature, and this certificate.
//! 5. The statement lists the expected file name with the expected SHA-256.
//!
//! The Merkle inclusion proof is not checked. The signed entry timestamp is
//! Rekor's promise of inclusion, which is what the time check needs.

use base64::engine::general_purpose::STANDARD;
use base64::Engine as _;
use ring::signature::{self, UnparsedPublicKey};
use rustls_pki_types::{CertificateDer, UnixTime};
use serde::Deserialize;
use sha2::{Digest, Sha256};

use crate::der;
use crate::UpdateError;

const FULCIO_ROOT: &[u8] = include_bytes!("../pins/fulcio-root.der");
const FULCIO_INTERMEDIATE: &[u8] = include_bytes!("../pins/fulcio-intermediate.der");
const REKOR_KEY: &[u8] = include_bytes!("../pins/rekor.spki.der");

const BUNDLE_MEDIA_TYPE: &str = "application/vnd.dev.sigstore.bundle.v0.3+json";
const IN_TOTO_PAYLOAD: &str = "application/vnd.in-toto+json";
const IN_TOTO_STATEMENT: &str = "https://in-toto.io/Statement/v1";

const OID_SUBJECT_ALT_NAME: &[u8] = &[0x55, 0x1d, 0x11];
/// 1.3.6.1.4.1.57264.1.1, the token issuer as raw text.
const OID_FULCIO_ISSUER_V1: &[u8] = &[0x2b, 0x06, 0x01, 0x04, 0x01, 0x83, 0xbf, 0x30, 0x01, 0x01];
/// 1.3.6.1.4.1.57264.1.8, the token issuer as a DER UTF8String.
const OID_FULCIO_ISSUER_V2: &[u8] = &[0x2b, 0x06, 0x01, 0x04, 0x01, 0x83, 0xbf, 0x30, 0x01, 0x08];
/// 1.3.6.1.5.5.7.3.3, id-kp-codeSigning.
const OID_CODE_SIGNING: &[u8] = &[0x2b, 0x06, 0x01, 0x05, 0x05, 0x07, 0x03, 0x03];
const OID_P256: &[u8] = &[0x2a, 0x86, 0x48, 0xce, 0x3d, 0x03, 0x01, 0x07];
const OID_P384: &[u8] = &[0x2b, 0x81, 0x04, 0x00, 0x22];

/// Who must have signed: the workflow named in the certificate and the issuer of
/// the token it presented to Fulcio.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PublisherIdentity {
    pub workflow: &'static str,
    pub issuer: &'static str,
}

fn rejected<T>(reason: &str) -> Result<T, UpdateError> {
    Err(UpdateError(format!(
        "The publisher's signature could not be confirmed: {reason}."
    )))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Bundle {
    media_type: String,
    verification_material: Material,
    dsse_envelope: Envelope,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Material {
    certificate: RawBytes,
    tlog_entries: Vec<LogEntry>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawBytes {
    raw_bytes: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogEntry {
    log_index: String,
    log_id: LogId,
    kind_version: KindVersion,
    integrated_time: String,
    inclusion_promise: Promise,
    canonicalized_body: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LogId {
    key_id: String,
}

#[derive(Deserialize)]
struct KindVersion {
    kind: String,
    version: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Promise {
    signed_entry_timestamp: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope {
    payload: String,
    payload_type: String,
    signatures: Vec<EnvelopeSignature>,
}

#[derive(Deserialize)]
struct EnvelopeSignature {
    sig: String,
}

#[derive(Deserialize)]
struct LoggedDsse {
    kind: String,
    spec: LoggedSpec,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LoggedSpec {
    payload_hash: LoggedHash,
    signatures: Vec<LoggedSignature>,
}

#[derive(Deserialize)]
struct LoggedHash {
    algorithm: String,
    value: String,
}

#[derive(Deserialize)]
struct LoggedSignature {
    signature: String,
    verifier: String,
}

#[derive(Deserialize)]
struct Statement {
    #[serde(rename = "_type")]
    kind: String,
    subject: Vec<StatementSubject>,
}

#[derive(Deserialize)]
struct StatementSubject {
    name: String,
    digest: std::collections::BTreeMap<String, String>,
}

fn decode(text: &str, what: &str) -> Result<Vec<u8>, UpdateError> {
    STANDARD
        .decode(text)
        .or_else(|_| rejected(&format!("the {what} is not valid base64")))
}

/// Confirm that `bundle` is the publisher's signed statement that a file named
/// `file_name` has SHA-256 `sha256`. Returns the Unix time it was signed.
pub fn verify_attestation(
    bundle: &[u8],
    identity: PublisherIdentity,
    file_name: &str,
    sha256: &str,
) -> Result<u64, UpdateError> {
    let bundle: Bundle =
        serde_json::from_slice(bundle).or_else(|_| rejected("the signature file is unreadable"))?;
    if bundle.media_type != BUNDLE_MEDIA_TYPE {
        return rejected("the signature file has an unknown format");
    }
    let [entry] = bundle.verification_material.tlog_entries.as_slice() else {
        return rejected("the signature file must carry exactly one log entry");
    };
    let [envelope_signature] = bundle.dsse_envelope.signatures.as_slice() else {
        return rejected("the statement must carry exactly one signature");
    };
    let leaf = decode(
        &bundle.verification_material.certificate.raw_bytes,
        "certificate",
    )?;
    let payload = decode(&bundle.dsse_envelope.payload, "statement")?;
    let statement_signature = decode(&envelope_signature.sig, "statement signature")?;
    let signed_at: u64 = entry
        .integrated_time
        .parse()
        .or_else(|_| rejected("the log time is unreadable"))?;

    // 4a. Rekor vouches for the log entry and its time.
    verify_log_promise(entry, signed_at)?;

    // 1. The certificate is Fulcio's, for code signing, valid when it was logged.
    let leaf_der = CertificateDer::from(leaf.as_slice());
    let end_entity = webpki::EndEntityCert::try_from(&leaf_der)
        .or_else(|_| rejected("the certificate is unreadable"))?;
    let root = CertificateDer::from(FULCIO_ROOT);
    let anchor = webpki::anchor_from_trusted_cert(&root)
        .or_else(|_| rejected("the pinned root is unreadable"))?;
    let intermediates = [CertificateDer::from(FULCIO_INTERMEDIATE)];
    end_entity
        .verify_for_usage(
            &[
                webpki::ring::ECDSA_P256_SHA256,
                webpki::ring::ECDSA_P256_SHA384,
                webpki::ring::ECDSA_P384_SHA256,
                webpki::ring::ECDSA_P384_SHA384,
            ],
            &[anchor],
            &intermediates,
            UnixTime::since_unix_epoch(std::time::Duration::from_secs(signed_at)),
            webpki::KeyUsage::required(OID_CODE_SIGNING),
            None,
            None,
        )
        .or_else(|_| rejected("the certificate was not issued by Sigstore when it was used"))?;

    // 2. The certificate names the expected workflow and token issuer.
    let fields =
        der::certificate_fields(&leaf).or_else(|_| rejected("the certificate is unreadable"))?;
    check_identity(&fields, identity)?;

    // 3. The statement was signed with the certificate's key.
    let (curve, point) =
        der::ec_public_key(fields.spki).or_else(|_| rejected("the certificate key is unusable"))?;
    let algorithm: &dyn signature::VerificationAlgorithm = match curve {
        OID_P256 => &signature::ECDSA_P256_SHA256_ASN1,
        OID_P384 => &signature::ECDSA_P384_SHA384_ASN1,
        _ => return rejected("the certificate key uses an unsupported curve"),
    };
    let signed = pre_authentication_encoding(&bundle.dsse_envelope.payload_type, &payload);
    UnparsedPublicKey::new(algorithm, point)
        .verify(&signed, &statement_signature)
        .or_else(|_| rejected("the statement signature does not match"))?;

    // 4b. The log entry is for this statement, signature, and certificate.
    check_log_body(entry, &payload, &statement_signature, &leaf)?;

    // 5. The statement lists the file with the pinned digest.
    if bundle.dsse_envelope.payload_type != IN_TOTO_PAYLOAD {
        return rejected("the statement has an unknown type");
    }
    let statement: Statement =
        serde_json::from_slice(&payload).or_else(|_| rejected("the statement is unreadable"))?;
    if statement.kind != IN_TOTO_STATEMENT {
        return rejected("the statement has an unknown type");
    }
    let listed = statement.subject.iter().any(|subject| {
        subject.name == file_name
            && subject
                .digest
                .get("sha256")
                .is_some_and(|digest| digest.eq_ignore_ascii_case(sha256))
    });
    if !listed {
        return rejected("the signed statement does not list this file with this checksum");
    }
    Ok(signed_at)
}

fn check_identity(
    fields: &der::CertificateFields<'_>,
    identity: PublisherIdentity,
) -> Result<(), UpdateError> {
    let mut workflow_matches = false;
    let mut issuer = None;
    for (oid, _critical, value) in &fields.extensions {
        match *oid {
            OID_SUBJECT_ALT_NAME => {
                let names = der::uri_names(value)
                    .or_else(|_| rejected("the certificate names are unreadable"))?;
                // Fulcio issues exactly one identity per certificate.
                workflow_matches = names.len() == 1 && names[0] == identity.workflow.as_bytes();
            }
            OID_FULCIO_ISSUER_V2 => {
                let text = der::Reader::new(value)
                    .expect(der::UTF8_STRING)
                    .or_else(|_| rejected("the certificate issuer is unreadable"))?;
                issuer = Some(text.to_vec());
            }
            OID_FULCIO_ISSUER_V1 if issuer.is_none() => issuer = Some(value.to_vec()),
            _ => {}
        }
    }
    if !workflow_matches {
        return rejected("the certificate was issued to a different workflow");
    }
    if issuer.as_deref() != Some(identity.issuer.as_bytes()) {
        return rejected("the certificate was issued for a different token issuer");
    }
    Ok(())
}

/// DSSE's pre-authentication encoding, the exact bytes that get signed.
fn pre_authentication_encoding(payload_type: &str, payload: &[u8]) -> Vec<u8> {
    let mut out = format!(
        "DSSEv1 {} {} {} ",
        payload_type.len(),
        payload_type,
        payload.len()
    )
    .into_bytes();
    out.extend_from_slice(payload);
    out
}

fn verify_log_promise(entry: &LogEntry, signed_at: u64) -> Result<(), UpdateError> {
    let key_id = decode(&entry.log_id.key_id, "log ID")?;
    if key_id != Sha256::digest(REKOR_KEY).as_slice() {
        return rejected("the entry is from an unknown transparency log");
    }
    let log_index: u64 = entry
        .log_index
        .parse()
        .or_else(|_| rejected("the log index is unreadable"))?;
    if !entry
        .canonicalized_body
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'+' | b'/' | b'='))
    {
        return rejected("the log entry is not valid base64");
    }
    // Rekor signs this canonical JSON: sorted keys, no spaces.
    let signed = format!(
        r#"{{"body":"{}","integratedTime":{},"logID":"{}","logIndex":{}}}"#,
        entry.canonicalized_body,
        signed_at,
        crate::hex(&key_id),
        log_index
    );
    let promise = decode(
        &entry.inclusion_promise.signed_entry_timestamp,
        "log promise",
    )?;
    let (curve, point) =
        der::ec_public_key(REKOR_KEY).or_else(|_| rejected("the pinned log key is unusable"))?;
    if curve != OID_P256 {
        return rejected("the pinned log key is unusable");
    }
    UnparsedPublicKey::new(&signature::ECDSA_P256_SHA256_ASN1, point)
        .verify(signed.as_bytes(), &promise)
        .or_else(|_| rejected("the transparency log did not sign this entry"))
}

fn check_log_body(
    entry: &LogEntry,
    payload: &[u8],
    statement_signature: &[u8],
    leaf: &[u8],
) -> Result<(), UpdateError> {
    if entry.kind_version.kind != "dsse" || entry.kind_version.version != "0.0.1" {
        return rejected("the log entry has an unknown kind");
    }
    let body = decode(&entry.canonicalized_body, "log entry")?;
    let body: LoggedDsse =
        serde_json::from_slice(&body).or_else(|_| rejected("the log entry is unreadable"))?;
    if body.kind != "dsse"
        || body.spec.payload_hash.algorithm != "sha256"
        || body.spec.payload_hash.value != crate::hex(&Sha256::digest(payload))
    {
        return rejected("the log entry is for a different statement");
    }
    let [logged] = body.spec.signatures.as_slice() else {
        return rejected("the log entry must carry exactly one signature");
    };
    if decode(&logged.signature, "logged signature")? != statement_signature {
        return rejected("the log entry is for a different signature");
    }
    let pem = decode(&logged.verifier, "logged certificate")?;
    if pem_certificate(&pem)? != leaf {
        return rejected("the log entry is for a different certificate");
    }
    Ok(())
}

fn pem_certificate(pem: &[u8]) -> Result<Vec<u8>, UpdateError> {
    let text =
        std::str::from_utf8(pem).or_else(|_| rejected("the logged certificate is unreadable"))?;
    let body = text
        .trim()
        .strip_prefix("-----BEGIN CERTIFICATE-----")
        .and_then(|rest| rest.strip_suffix("-----END CERTIFICATE-----"));
    let Some(body) = body else {
        return rejected("the logged certificate is unreadable");
    };
    let joined: String = body.split_whitespace().collect();
    decode(&joined, "logged certificate")
}

#[cfg(test)]
mod tests {
    use super::*;

    const BUNDLE: &[u8] = include_bytes!("../pins/llama-b10621-bin-win-vulkan-x64.sigstore.json");
    const FILE: &str = "llama-b10621-bin-win-vulkan-x64.zip";
    const SHA: &str = "2672d85bf87c8280d94dee01eb6a86280046878f70a07d786a93637fa9081163";
    const IDENTITY: PublisherIdentity = PublisherIdentity {
        workflow:
            "https://github.com/ggml-org/llama.cpp/.github/workflows/release.yml@refs/heads/master",
        issuer: "https://token.actions.githubusercontent.com",
    };

    fn edited(edit: impl FnOnce(&mut serde_json::Value)) -> Vec<u8> {
        let mut value: serde_json::Value = serde_json::from_slice(BUNDLE).expect("bundle");
        edit(&mut value);
        serde_json::to_vec(&value).expect("json")
    }

    fn error_of(bundle: &[u8]) -> String {
        verify_attestation(bundle, IDENTITY, FILE, SHA)
            .expect_err("must be rejected")
            .0
    }

    #[test]
    fn the_pinned_llama_cpp_attestation_verifies() {
        let signed_at = verify_attestation(BUNDLE, IDENTITY, FILE, SHA).expect("verified");
        assert_eq!(signed_at, 1_787_653_019);
    }

    #[test]
    fn a_different_checksum_is_rejected() {
        let other = "0".repeat(64);
        let error = verify_attestation(BUNDLE, IDENTITY, FILE, &other).expect_err("rejected");
        assert!(error.0.contains("does not list this file"), "{}", error.0);
        let error = verify_attestation(BUNDLE, IDENTITY, "llama-other.zip", SHA).expect_err("x");
        assert!(error.0.contains("does not list this file"), "{}", error.0);
    }

    #[test]
    fn a_different_publisher_is_rejected() {
        let wrong_workflow = PublisherIdentity {
            workflow: "https://github.com/someone/llama.cpp/.github/workflows/release.yml@refs/heads/master",
            ..IDENTITY
        };
        let error = verify_attestation(BUNDLE, wrong_workflow, FILE, SHA).expect_err("rejected");
        assert!(error.0.contains("different workflow"), "{}", error.0);
        let wrong_issuer = PublisherIdentity {
            issuer: "https://accounts.example.com",
            ..IDENTITY
        };
        let error = verify_attestation(BUNDLE, wrong_issuer, FILE, SHA).expect_err("rejected");
        assert!(error.0.contains("different token issuer"), "{}", error.0);
    }

    #[test]
    fn an_edited_statement_is_rejected() {
        let tampered = edited(|value| {
            let payload = value["dsseEnvelope"]["payload"].as_str().expect("payload");
            let mut statement = STANDARD.decode(payload).expect("decode");
            let at = statement.len() / 2;
            statement[at] ^= 0x01;
            value["dsseEnvelope"]["payload"] = STANDARD.encode(statement).into();
        });
        assert!(error_of(&tampered).contains("statement"));
    }

    #[test]
    fn a_moved_log_time_is_rejected() {
        let tampered = edited(|value| {
            value["verificationMaterial"]["tlogEntries"][0]["integratedTime"] = "1787653020".into();
        });
        assert!(error_of(&tampered).contains("did not sign this entry"));
    }

    #[test]
    fn a_swapped_certificate_is_rejected() {
        let tampered = edited(|value| {
            value["verificationMaterial"]["certificate"]["rawBytes"] =
                STANDARD.encode(FULCIO_INTERMEDIATE).into();
        });
        assert!(error_of(&tampered).contains("certificate"));
    }

    #[test]
    fn unreadable_input_is_rejected() {
        assert!(error_of(b"{").contains("unreadable"));
        let tampered = edited(|value| value["mediaType"] = "something/else".into());
        assert!(error_of(&tampered).contains("unknown format"));
    }
}
