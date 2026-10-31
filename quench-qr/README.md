# quench-qr
A QR code encoder (ISO 18004, byte mode, versions 1-40) with SVG output, no dependencies.

## What it is
`quench-qr` encodes arbitrary bytes into a QR code (ISO/IEC 18004, Model 2): versions 1 to 40, the four error-correction levels, and the eight masks chosen by the standard's penalty rules. Output is text or SVG. It has no dependencies and replaces the `qrcode` crate.

It is an encoder only; there is no decoder. Data is always encoded in byte mode. TOTP provisioning URLs come from `quench-crypto`, and the SVG can be rendered by `quench-svg`.

## Use cases
- Show a TOTP provisioning QR code during two-factor setup.
- Embed a QR code in a page as inline SVG or a data URI.
- Pick an error-correction level for printed or damaged-prone codes.
- Print a code to a terminal in tests or CLIs.

## Example
```rust
use quench_qr::{EcLevel, QrCode};

let code = QrCode::new("https://example.com/")?;   // level M, smallest version that fits
let svg: String = code.to_svg(300);                // at least 300 x 300 pixels
std::fs::write("code.svg", svg)?;

let sturdy = QrCode::with_level(b"data", EcLevel::H)?;
assert_eq!(sturdy.level(), EcLevel::H);
print!("{}", sturdy.to_text());
```

## Documentation
Full documentation: https://lorehaven.github.io/quench/crates/quench-qr/
