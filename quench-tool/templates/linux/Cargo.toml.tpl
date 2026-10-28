[package]
name = "{{ name }}-desktop"
{% if standalone %}version = "0.1.0"
edition = "2024"
{% else %}version.workspace = true
edition.workspace = true
{% endif %}build = "build.rs"

# The Linux desktop host: the app's bundle (`dist/www`, made by `quench build --bundle`) in a window.
# `build.rs` embeds the bundle, so the binary runs offline.
[[bin]]
name = "{{ name }}-desktop"
path = "src/main.rs"

[dependencies]
{% if standalone %}{{ quench_desktop }}
{% else %}quench-desktop = { workspace = true }
{% endif %}{% if standalone %}
# Not part of any other workspace.
[workspace]
{% endif %}
