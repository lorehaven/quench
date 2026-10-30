---
title: quench-qr
parent: Crates
nav_order: 36
has_children: true
permalink: /crates/quench-qr/
---

`quench-qr` (crate `quench_qr`) is a QR code encoder for ISO/IEC 18004 (Model 2): arbitrary bytes, versions 1 to 40, the four error-correction levels, the eight masks chosen by the standard's four penalty rules, with text and SVG output. It has **no dependencies** and replaces the `qrcode` crate.

* Version: `0.1.2`. Edition 2024. No features. `[lints] workspace = true` (`unsafe_code = "deny"`; none present).
* Encoder only: there is no decoder. Data is always encoded in **byte mode** (mode indicator `0100`); numeric, alphanumeric, kanji and ECI modes, structured append and micro QR are not implemented. The scanner reads the same text either way; byte mode only costs space.

## Install

```toml
[dependencies]
quench-qr = { version = "0.1", registry = "ennor" }   # in the workspace: { workspace = true }
```

## Dependencies

None (`[dependencies]` is empty). Dev-only: `quench-test` (harness).

## Where to go next

* [Usage](usage.md)
* [API reference](api.md)
* [Internals](internals.md): bit stream, blocks, masks, SVG layout, capacities.
* [Testing](testing.md)
* TOTP provisioning URLs come from `quench_crypto::totp::Totp::url` ([quench-crypto](../quench-crypto/index.md)); the SVG can be rendered by [quench-svg](../quench-svg/index.md).

## Minimal example

```rust
use quench_qr::QrCode;

let code = QrCode::new("otpauth://totp/Forge:ann?secret=ABC")?;
assert!(code.size() >= 21);
let svg: String = code.to_svg(200);
assert!(svg.starts_with("<?xml"));
# Ok::<(), quench_qr::Error>(())
```
