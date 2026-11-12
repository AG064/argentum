# Public synthetic key

`public-ring-test-key.der` is the published ring 0.17.14 RSA documentation
test fixture, copied from `ring/src/rsa/signature_rsa_example_private_key.der`.
It is not an account key, credential, or production signing key. It is compiled
only into unit tests, never into the application authentication path.

Upstream: https://github.com/briansmith/ring/tree/0.17.14/src/rsa
Upstream is distributed under ISC, MIT, and OpenSSL licenses. This fixture is
retained solely for signed synthetic JWT positive and negative controls.
