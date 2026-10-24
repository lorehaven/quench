Test-only TLS material for the `quench-proto` TLS tests (CA and a `localhost` / `127.0.0.1` leaf, ECDSA P-256, valid for 100 years).
The CA private key was discarded after signing; regenerate the set with `openssl` if the leaf ever needs changing.
Never use these keys for anything but tests.

The `client-ca.pem` / `client.pem` / `client.key` set is a separate CA and a client certificate (`CN=quench-test-client`, `clientAuth`) for the mutual-TLS tests; its CA key was discarded as well.
