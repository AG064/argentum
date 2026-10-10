//! Strict OIDC identity validation. Decoded claims alone are not identity evidence.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde::Deserialize;

use crate::chatgpt_auth::{AuthError, ISSUER};

#[derive(Deserialize)]
struct Header {
    alg: String,
    kid: String,
    crit: Option<serde_json::Value>,
}
#[derive(Deserialize)]
struct Claims {
    iss: String,
    aud: serde_json::Value,
    sub: String,
    exp: u64,
    iat: u64,
    nonce: Option<String>,
}

/// Only the verified subject is returned. Tokens/claims must never become events.
pub(crate) fn verify(
    token: &str,
    jwks: &serde_json::Value,
    client: &str,
    nonce: Option<&str>,
    expected_subject: Option<&str>,
    now: u64,
) -> Result<String, AuthError> {
    if token.len() > 32 * 1024 {
        return Err(AuthError::Identity);
    }
    let parts: Vec<_> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(AuthError::Identity);
    }
    let decode = |part: &str| {
        URL_SAFE_NO_PAD
            .decode(part)
            .map_err(|_| AuthError::Identity)
    };
    let header: Header =
        serde_json::from_slice(&decode(parts[0])?).map_err(|_| AuthError::Identity)?;
    if header.alg != "RS256" || header.kid.is_empty() || header.crit.is_some() {
        return Err(AuthError::Identity);
    }
    let keys = jwks
        .get("keys")
        .and_then(|v| v.as_array())
        .ok_or(AuthError::Identity)?;
    if keys.len() > 64 {
        return Err(AuthError::Identity);
    }
    let matching: Vec<_> = keys
        .iter()
        .filter(|v| v.get("kid").and_then(|k| k.as_str()) == Some(&header.kid))
        .collect();
    if matching.len() != 1 {
        return Err(AuthError::Identity);
    }
    let key = matching[0];
    if key["kty"] != "RSA"
        || key.get("use").is_some_and(|v| v != "sig")
        || key.get("alg").is_some_and(|v| v != "RS256")
    {
        return Err(AuthError::Identity);
    }
    let n = decode(key["n"].as_str().ok_or(AuthError::Identity)?)?;
    let e = decode(key["e"].as_str().ok_or(AuthError::Identity)?)?;
    let signature = decode(parts[2])?;
    ring::signature::RsaPublicKeyComponents { n: &n, e: &e }
        .verify(
            &ring::signature::RSA_PKCS1_2048_8192_SHA256,
            format!("{}.{}", parts[0], parts[1]).as_bytes(),
            &signature,
        )
        .map_err(|_| AuthError::Identity)?;
    let claims: Claims =
        serde_json::from_slice(&decode(parts[1])?).map_err(|_| AuthError::Identity)?;
    let audience = match &claims.aud {
        serde_json::Value::String(aud) => aud == client,
        serde_json::Value::Array(aud) => aud.len() == 1 && aud[0].as_str() == Some(client),
        _ => false,
    };
    if claims.iss != ISSUER
        || !audience
        || claims.exp <= now.saturating_sub(5)
        || claims.iat > now.saturating_add(5)
        || claims.exp <= claims.iat
        || claims.sub.is_empty()
        || claims.sub.len() > 1024
        || nonce.is_some_and(|n| claims.nonce.as_deref() != Some(n))
        || expected_subject.is_some_and(|subject| claims.sub != subject)
    {
        return Err(AuthError::Identity);
    }
    Ok(claims.sub)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::KeyPair;
    use serde_json::json;
    // Public ring 0.17.14 documentation fixture, not a user credential.
    const TEST_KEY: &[u8] = include_bytes!("../tests/fixtures/chatgpt/public-ring-test-key.der");
    fn tlv<'a>(bytes: &mut &'a [u8], tag: u8) -> &'a [u8] {
        assert_eq!(bytes[0], tag);
        let first = bytes[1];
        let (length, offset) = if first < 128 {
            (usize::from(first), 2)
        } else {
            let n = usize::from(first & 127);
            let len = bytes[2..2 + n]
                .iter()
                .fold(0usize, |a, b| (a << 8) | usize::from(*b));
            (len, 2 + n)
        };
        let result = &bytes[offset..offset + length];
        *bytes = &bytes[offset + length..];
        result
    }
    fn signed(header: serde_json::Value, claims: serde_json::Value) -> (String, serde_json::Value) {
        let key = ring::signature::RsaKeyPair::from_der(TEST_KEY)
            .or_else(|_| ring::signature::RsaKeyPair::from_pkcs8(TEST_KEY))
            .unwrap();
        let mut public = key.public_key().as_ref();
        let mut sequence = tlv(&mut public, 0x30);
        let n = tlv(&mut sequence, 2);
        let n = n.strip_prefix(&[0]).unwrap_or(n);
        let e = tlv(&mut sequence, 2);
        let jwks = json!({"keys":[{"kid":"test","kty":"RSA","alg":"RS256","use":"sig","n":URL_SAFE_NO_PAD.encode(n),"e":URL_SAFE_NO_PAD.encode(e)}]});
        let input = format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&header).unwrap()),
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap())
        );
        let mut signature = vec![0; key.public().modulus_len()];
        key.sign(
            &ring::signature::RSA_PKCS1_SHA256,
            &ring::rand::SystemRandom::new(),
            input.as_bytes(),
            &mut signature,
        )
        .unwrap();
        (
            format!("{input}.{}", URL_SAFE_NO_PAD.encode(signature)),
            jwks,
        )
    }
    #[test]
    fn signed_identity_requires_signature_issuer_audience_time_nonce_and_subject() {
        let claims = json!({"iss":ISSUER,"aud":"oaiapp_test","sub":"synthetic_subject","iat":95,"exp":200,"nonce":"nonce"});
        let (token, jwks) = signed(json!({"alg":"RS256","kid":"test"}), claims.clone());
        assert_eq!(
            verify(&token, &jwks, "oaiapp_test", Some("nonce"), None, 100).unwrap(),
            "synthetic_subject"
        );
        assert!(verify(&token, &jwks, "oaiapp_other", Some("nonce"), None, 100).is_err());
        assert!(verify(&token, &jwks, "oaiapp_test", Some("wrong"), None, 100).is_err());
        assert!(verify(
            &token,
            &jwks,
            "oaiapp_test",
            None,
            Some("other_subject"),
            100
        )
        .is_err());
        for (field, value) in [
            ("iss", json!("https://evil.invalid")),
            ("aud", json!(["oaiapp_test", "oaiapp_other"])),
            ("exp", json!(80)),
            ("iat", json!(120)),
        ] {
            let mut bad = claims.clone();
            bad[field] = value;
            let (token, jwks) = signed(json!({"alg":"RS256","kid":"test"}), bad);
            assert!(verify(&token, &jwks, "oaiapp_test", Some("nonce"), None, 100).is_err());
        }
        for header in [
            json!({"alg":"none","kid":"test"}),
            json!({"alg":"RS256","kid":"unknown"}),
        ] {
            let (token, jwks) = signed(header, claims.clone());
            assert!(verify(&token, &jwks, "oaiapp_test", None, None, 100).is_err());
        }
        let (token, mut jwks) = signed(json!({"alg":"RS256","kid":"test"}), claims);
        let mut bad = token.clone().into_bytes();
        let last = bad.len() - 5;
        bad[last] = if bad[last] == b'A' { b'B' } else { b'A' };
        assert!(verify(
            std::str::from_utf8(&bad).unwrap(),
            &jwks,
            "oaiapp_test",
            None,
            None,
            100
        )
        .is_err());
        let duplicate = jwks["keys"][0].clone();
        jwks["keys"].as_array_mut().unwrap().push(duplicate);
        assert!(verify(&token, &jwks, "oaiapp_test", None, None, 100).is_err());
    }
}
