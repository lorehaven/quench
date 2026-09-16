# Quench Starter

Server bootstrap crate shared by Forge's HTTP services, for both the Actix and [Quench Http](../quench-http/README.md) stacks — wraps TLS/plain HTTP setup, base-path scoping, health/readiness state, request logging and correlation IDs, database bootstrap via `quench-db`, and a small UI/routing layer built on `quench-web` behind one `serve()` entry point.

See [docs/quench-starter.md](../docs/quench-starter.md) for full documentation.
