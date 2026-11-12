// SPDX-License-Identifier: AGPL-3.0-only
// Copyright (C) 2026 AG064

//! Just enough DER reading to pull named fields out of an X.509 certificate.
//!
//! Signature and chain checks are done by `ring` and `rustls-webpki`. This reader
//! only finds the public key and the extensions that name who a certificate was
//! issued to. It accepts definite lengths only, as DER requires.

pub(crate) const SEQUENCE: u8 = 0x30;
pub(crate) const OCTET_STRING: u8 = 0x04;
pub(crate) const BIT_STRING: u8 = 0x03;
pub(crate) const OID: u8 = 0x06;
pub(crate) const BOOLEAN: u8 = 0x01;
pub(crate) const UTF8_STRING: u8 = 0x0c;
pub(crate) const CONTEXT_0: u8 = 0xa0;
pub(crate) const CONTEXT_3: u8 = 0xa3;
pub(crate) const URI_NAME: u8 = 0x86;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Malformed;

/// A cursor over a run of DER elements.
#[derive(Clone, Copy)]
pub(crate) struct Reader<'a> {
    rest: &'a [u8],
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { rest: bytes }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.rest.is_empty()
    }

    /// The next element's tag and contents.
    pub(crate) fn next(&mut self) -> Result<(u8, &'a [u8]), Malformed> {
        let (&tag, after_tag) = self.rest.split_first().ok_or(Malformed)?;
        // Multi-byte tags never appear in the fields read here.
        if tag & 0x1f == 0x1f {
            return Err(Malformed);
        }
        let (&first, mut after_length) = after_tag.split_first().ok_or(Malformed)?;
        let length = if first < 0x80 {
            usize::from(first)
        } else {
            let count = usize::from(first & 0x7f);
            if count == 0 || count > 4 || after_length.len() < count {
                return Err(Malformed);
            }
            let mut length = 0usize;
            for &byte in &after_length[..count] {
                length = (length << 8) | usize::from(byte);
            }
            if length < 0x80 || (count > 1 && after_length[0] == 0) {
                return Err(Malformed);
            }
            after_length = &after_length[count..];
            length
        };
        if after_length.len() < length {
            return Err(Malformed);
        }
        let (contents, rest) = after_length.split_at(length);
        self.rest = rest;
        Ok((tag, contents))
    }

    /// The next element's contents, which must carry `tag`.
    pub(crate) fn expect(&mut self, tag: u8) -> Result<&'a [u8], Malformed> {
        match self.next()? {
            (found, contents) if found == tag => Ok(contents),
            _ => Err(Malformed),
        }
    }

    /// The next element's contents if it carries `tag`, without consuming otherwise.
    pub(crate) fn optional(&mut self, tag: u8) -> Result<Option<&'a [u8]>, Malformed> {
        if self.rest.first() != Some(&tag) {
            return Ok(None);
        }
        self.expect(tag).map(Some)
    }

    pub(crate) fn skip(&mut self) -> Result<(), Malformed> {
        self.next().map(|_| ())
    }
}

/// The parts of a certificate the attestation check reads.
pub(crate) struct CertificateFields<'a> {
    /// The full `SubjectPublicKeyInfo` element, tag and length included.
    pub(crate) spki: &'a [u8],
    /// `(oid, critical, value)` for every extension.
    pub(crate) extensions: Vec<(&'a [u8], bool, &'a [u8])>,
}

pub(crate) fn certificate_fields(der: &[u8]) -> Result<CertificateFields<'_>, Malformed> {
    let mut outer = Reader::new(der);
    let certificate = outer.expect(SEQUENCE)?;
    if !outer.is_empty() {
        return Err(Malformed);
    }
    let mut certificate = Reader::new(certificate);
    let tbs = certificate.expect(SEQUENCE)?;
    let mut tbs = Reader::new(tbs);
    tbs.optional(CONTEXT_0)?; // version
    tbs.skip()?; // serial number
    tbs.skip()?; // signature algorithm
    tbs.skip()?; // issuer
    tbs.skip()?; // validity
    tbs.skip()?; // subject
    let before_spki = tbs.rest;
    tbs.expect(SEQUENCE)?;
    let spki = &before_spki[..before_spki.len() - tbs.rest.len()];
    let mut extensions = Vec::new();
    while !tbs.is_empty() {
        let (tag, contents) = tbs.next()?;
        if tag != CONTEXT_3 {
            continue; // issuer or subject unique identifiers
        }
        let mut wrapper = Reader::new(contents);
        let list = wrapper.expect(SEQUENCE)?;
        let mut list = Reader::new(list);
        while !list.is_empty() {
            let mut extension = Reader::new(list.expect(SEQUENCE)?);
            let oid = extension.expect(OID)?;
            let critical = match extension.optional(BOOLEAN)? {
                Some([0xff]) => true,
                Some([0x00]) => false,
                Some(_) => return Err(Malformed),
                None => false,
            };
            let value = extension.expect(OCTET_STRING)?;
            if !extension.is_empty() {
                return Err(Malformed);
            }
            extensions.push((oid, critical, value));
        }
    }
    Ok(CertificateFields { spki, extensions })
}

/// The raw public key bytes and the curve OID from a `SubjectPublicKeyInfo`.
pub(crate) fn ec_public_key(spki: &[u8]) -> Result<(&[u8], &[u8]), Malformed> {
    let mut outer = Reader::new(spki);
    let mut info = Reader::new(outer.expect(SEQUENCE)?);
    let mut algorithm = Reader::new(info.expect(SEQUENCE)?);
    let kind = algorithm.expect(OID)?;
    // id-ecPublicKey, 1.2.840.10045.2.1
    if kind != [0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01] {
        return Err(Malformed);
    }
    let curve = algorithm.expect(OID)?;
    let key = info.expect(BIT_STRING)?;
    match key.split_first() {
        Some((0, point)) => Ok((curve, point)),
        _ => Err(Malformed),
    }
}

/// Every URI in a `SubjectAltName` extension value.
pub(crate) fn uri_names(value: &[u8]) -> Result<Vec<&[u8]>, Malformed> {
    let mut outer = Reader::new(value);
    let mut names = Reader::new(outer.expect(SEQUENCE)?);
    let mut found = Vec::new();
    while !names.is_empty() {
        let (tag, contents) = names.next()?;
        if tag == URI_NAME {
            found.push(contents);
        }
    }
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_must_be_minimal_and_present() {
        assert!(Reader::new(&[0x04, 0x81, 0x05, 1, 2, 3, 4, 5])
            .next()
            .is_err());
        assert!(Reader::new(&[0x04, 0x03, 1, 2]).next().is_err());
        assert!(Reader::new(&[0x04, 0x80]).next().is_err());
        assert_eq!(
            Reader::new(&[0x04, 0x02, 1, 2]).next(),
            Ok((0x04, &[1u8, 2][..]))
        );
    }
}
