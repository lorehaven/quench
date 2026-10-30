---
title: Usage
parent: quench-qr
nav_order: 1
---

## 1. Encode text to a standalone SVG

```rust
use quench_qr::QrCode;

let code = QrCode::new("https://example.com/")?;   // level M, smallest version that fits
let svg = code.to_svg(300);                          // at least 300 x 300 pixels
std::fs::write("code.svg", svg)?;
# Ok::<(), Box<dyn std::error::Error>>(())
```

For "Hello, World" (version 1, 21 modules): `to_svg(200)` yields `width="203" height="203" viewBox="0 0 29 29"` (21 + 2 x 4 quiet zone = 29 modules; 200 / 29 rounds up to 7 pixels per module). This is asserted in `tests/qr.rs`.

Embed in a page without a file:

```rust
use quench_qr::QrCode;
let svg = QrCode::new("hi")?.to_svg(160);
// <img src="data:image/svg+xml;utf8,..."> or base64 via quench_core::base64
# Ok::<(), quench_qr::Error>(())
```

## 2. Choose an error-correction level

```rust
use quench_qr::{EcLevel, QrCode};

let sturdy = QrCode::with_level(b"data", EcLevel::H)?; // ~30 % can be lost, bigger symbol
assert_eq!(sturdy.level(), EcLevel::H);
# Ok::<(), quench_qr::Error>(())
```

Higher levels need a larger version for the same data: at version 1 the byte capacity is 17 (L), 14 (M), 11 (Q), 7 (H).

## 3. Check the size before encoding

```rust
use quench_qr::{EcLevel, QrCode};

let max = QrCode::capacity(40, EcLevel::M);        // 2331
match QrCode::new(vec![0u8; max + 1]) {
    Err(quench_qr::Error::TooLong { len, max }) => assert_eq!((len, max), (2332, 2331)),
    other => panic!("{other:?}"),
}
```

## 4. Draw in a terminal or test

```rust
let code = quench_qr::QrCode::new("x")?;
print!("{}", code.to_text()); // "##" per dark module, two spaces per light one, one line per row
# Ok::<(), quench_qr::Error>(())
```

There is no quiet zone in the text output. The crate's example prints it for any text and level:

```bash
cargo run -p quench-qr --example dump -- M "Hello, World"
```

stderr: `version 1 mask 3`; stdout: the 21 text rows. Exit code 2 and the error text when the data is too long.

## 5. Read modules yourself

```rust
let code = quench_qr::QrCode::new("x")?;
let dark = (0..code.size()).flat_map(|y| (0..code.size()).map(move |x| (x, y)))
    .filter(|&(x, y)| code.is_dark(x, y)).count();
assert!(dark > 0);
assert!(!code.is_dark(code.size(), 0)); // outside the symbol: false
# Ok::<(), quench_qr::Error>(())
```

Use this to rasterize with your own renderer (add the 4-module quiet zone yourself).

## 6. TOTP enrolment

```rust
use quench_crypto::totp::{generate_secret, Totp};
use quench_qr::QrCode;

let totp = Totp::new(generate_secret())?;
let svg = QrCode::new(totp.url("Forge", "ann"))?.to_svg(240);
# Ok::<(), Box<dyn std::error::Error>>(())
```

Pitfall: the SVG contains the secret; do not log it.
