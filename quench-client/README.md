# Quench Client

Forge's shared HTTP client abstraction: thin `reqwest`-based wrappers that add authentication (Basic, Bearer, OAuth2 `client_credentials`) and consistent JSON error handling, so services calling each other over HTTP don't each reimplement auth headers and response parsing.

See [docs/quench-client.md](../docs/quench-client.md) for full documentation.
