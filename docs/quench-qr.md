# Quench QR

`quench-qr` (crate `quench_qr`) is a QR code encoder (ISO/IEC 18004, Model 2) with SVG and text output. It has no dependencies at all and replaces the `qrcode` crate.

```rust
use quench_qr::{EcLevel, QrCode};

let code = QrCode::new("otpauth://totp/Forge:alice?secret=...")?;   // level M, smallest version that fits
let svg = code.to_svg(200);                                        // standalone SVG, at least 200 px
let high = QrCode::with_level(b"data", EcLevel::H)?;
```

## API

- `QrCode::new(data)` / `with_level(data, level)` — encodes bytes (byte mode) at the smallest version 1-40 that fits, `Error::TooLong` when even version 40 cannot hold them. `EcLevel` is `L`, `M`, `Q` or `H`.
- `size()` (modules per side), `version()`, `level()`, `mask()`, `is_dark(x, y)`, `QrCode::capacity(version, level)` (bytes that fit).
- `to_text()` — one line per row, `#` and space, for tests and terminals.
- `to_svg(min_pixels)` — a standalone SVG document: four-module quiet zone, an integer scale so modules stay crisp, dark modules drawn as one path of runs. Safe to embed as a `data:image/svg+xml;base64,` URI.

## Verification

- Golden symbol for `Hello, World` (version 1, level M) equals `qrencode`'s.
- Every version and level is built at full capacity and checked for the structural invariants (function patterns, format and version information, matching data length).
- Against `qrencode -8` the modules are identical whenever both pick the same mask; mask choice differs in a minority of cases (penalty ties), both being valid symbols.
- `zbarimg` decodes all random samples and one maximum-size payload for each of the 160 version/level pairs.
