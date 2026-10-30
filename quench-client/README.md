# Quench Client

Forge's shared HTTP client abstraction: thin wrappers over its own small hyper/rustls client (`quench_client::transport`) that add authentication (Basic, Bearer, OAuth2 `client_credentials`) and consistent JSON error handling, so services calling each other over HTTP don't each reimplement auth headers and response parsing.

See [docs/crates/quench-client/index.md](../docs/crates/quench-client/index.md) for full documentation.
