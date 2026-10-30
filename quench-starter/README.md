# Quench Starter

Server bootstrap crate shared by Forge's HTTP services, on [Quench Http](../quench-http/README.md) — wraps TLS/plain HTTP setup, base-path scoping, health/readiness state, request logging and correlation IDs, database bootstrap via `quench-db`, and a small UI/routing layer built on `quench-web` behind one `serve()` entry point.

See [docs/crates/quench-starter/index.md](../docs/crates/quench-starter/index.md) for full documentation.
