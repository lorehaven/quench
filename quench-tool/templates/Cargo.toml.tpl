[workspace]
members = [{% if fullstack %}"shared", {% endif %}"core", "app", "server"]
resolver = "2"

[workspace.package]
version = "0.1.0"
edition = "2024"

[workspace.dependencies]
{{ deps }}tokio = { version = "1.54", features = ["rt-multi-thread", "macros", "net", "io-util", "signal", "sync", "time"] }
{{ name }}-core = { path = "core" }
{{ name }}-app = { path = "app" }
{% if fullstack %}{{ name }}-shared = { path = "shared" }
{% endif %}
# The browser build, as `quench build --release` makes it: small and slow to compile.
[profile.wasm]
inherits = "release"
opt-level = "z"
lto = true
codegen-units = 1
panic = "abort"
strip = true

# The browser build while you work: quick to compile.
[profile.wasm-dev]
inherits = "dev"
