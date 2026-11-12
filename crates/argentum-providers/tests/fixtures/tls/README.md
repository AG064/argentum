# Public synthetic TLS certificates

These DER certificates are test-only, valid from 2020 to 2050. The localhost
leaf names only DNS `localhost`, not IP `127.0.0.1`. The root is never installed
in an operating-system or production trust store.

Both certificates use the already published ring RSA documentation fixture
`../chatgpt/public-ring-test-key.der`. That existing intentionally public test
key is the server key. No account credential or new private key is included.
The certificates were generated with .NET CertificateRequest, SHA-256/PKCS#1,
root CA/keyCertSign and leaf serverAuth/digitalSignature extensions. Tests
explicitly trust this root only in an in-memory test client, never by disabling
certificate or hostname verification.
