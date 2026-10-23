Test-only TLS material for the `quench-proto` TLS tests (CA and a `localhost` / `127.0.0.1` leaf, ECDSA P-256, valid for 100 years).
The CA private key was discarded after signing; regenerate the set with `openssl` if the leaf ever needs changing.
Never use these keys for anything but tests.
