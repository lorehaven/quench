# Gap report: format and algorithm crates

Crates: quench-proto, quench-crypto, quench-archive, quench-html, quench-style, quench-image, quench-pdf, quench-qr, quench-svg. Written against the source of 2026-10-30. Example snippets in the new pages were compiled and run in a scratch crate (outside the repository) with the exceptions marked *Unverified:* in the pages.

## 1. Stale or wrong statements found

### In the code's own documentation and manifests

* **quench-image `src/lib.rs:1-3` and `Cargo.toml:5`** say the crate decodes PNG only and that "JPEG and GIF are not read" (`decode` doc: "`Unsupported` for a format that is not PNG"). The code decodes PNG, JPEG (baseline, extended, progressive Huffman), GIF (first frame) and WebP (lossless, lossy, alpha, first animation frame): `lib.rs:63-84`. The old flat page was right; the crate docs are stale.
* **quench-image `src/lib.rs:76-85`**: the fall-through that would name `"WebP"` is unreachable (WebP is handled by the branch above it).
* **quench-qr old page**: `to_text()` was described as "`#` and space" per module; the code writes `"##"` and two spaces (`src/lib.rs` `to_text`).
* **quench-svg `src/lib.rs:7-14`** lists "`viewBox` with `preserveAspectRatio` (meet, centred)"; the code never reads `preserveAspectRatio` (always meet and centre, `render()` near line 1040). Also `visibility: hidden` is honoured only as an attribute, not inside `style` (`lib.rs:862-866`), and a nested `<svg>`'s own `viewBox` is ignored (only `x`, `y` apply).
* **quench-pdf old page** advertised `Document::{pages, get_object, resolve, stream_data}` "for callers that need more than text". Their parameter and return types (`Page`, `Object`, `Dict`) live in private modules and are not re-exported (`src/lib.rs:40` exports `Document` only), so callers cannot name them; only `Document::load(..)?.pages().len()` is practically usable.
* **quench-crypto old page** said Argon2id is the only thing implemented from scratch; the crate also implements BLAKE2b, HChaCha20/XChaCha20 framing, X25519, the age format and bech32, HOTP/TOTP and JWT itself (on top of `ring` primitives).
* **quench-proto brief**: the task description mentions "protobuf/proto wire rules". `quench-proto` is the HTTP protocol layer; it contains no protobuf code.
* **quench-proto `src/uri.rs:103`**: the doc comment of `Uri::scheme` says "`http` or `https`"; `Uri::parse` accepts any scheme matching `[A-Za-z][A-Za-z0-9+.-]*`.
* **quench-html old page** claims about `pulldown-cmark`/`scraper` comparison runs, **quench-archive old page** claims about zlib/zstd/GNU tar/Info-ZIP cross-checks and compression-size numbers, **quench-pdf old page** list of producers compared with `pdftotext`, **quench-image old page** claims about libwebp bit-exactness and `scripts/webp-fixtures.py`, **quench-qr old page** `zbarimg` claim, **quench-style old page** "98 % line coverage": none of these can be reproduced from this repository's tests. They are marked *Unverified* in the new pages.
* **quench-proto old page**: "with nothing enabled the crate pulls in 15 third-party crates; the defaults pull in 25" was not re-counted.

## 2. In the code but undocumented (now covered)

* quench-crypto: `hkdf`, `x25519`, `age`, `Hasher` (Clone), `hash::sha1_legacy`, `argon2::hash_raw_keyed`, `rsa::public_key_from_components`, `jwt::verify`/`decode_header`, `totp::Algorithm::name`; the `Validation::new` default of `validate_aud = true` (which rejects every token unless `aud` is configured or validation is turned off).
* quench-proto: `Extensions`, `io::{Io, Upgraded}`, `header::{AsHeaderName, IntoHeaderName, is_token_byte}`, `h1::reader::Reader`, `h1::decode::{encode_chunk, LAST_CHUNK}`, `h1::parse::{find_head_end, write_*_head, has_token, wants_keep_alive}`, `ws::{switch, client, client_handshake_headers, accept_key, generate_key}`, `Cookie` auto-adds `Secure` when `SameSite=None` and `secure` is unset (`cookie.rs` Display).
* quench-html: the whole `entities` module (`ENTITIES`, `LEGACY`, `named`, `numeric`, `decode`, `reference`) and `decode_entities`; `Node::{inner_html, prev_element, next_element, descendants, ancestors, text_nodes}`.
* quench-style: `theme::ThemeSpec`, `theme::palette::{Color, Palette}` (mix, luminance, contrast, `color("mix:...")`), `tokens::{Token, Length, Number, Millis}` and every constant, `components::{TEXT_PAIRS, EDGE_PAIRS, DANGER_TEXT_TOKEN}`, `ftl::{parse, Message, FtlError}`, `assets::create_asset_files*` ignoring `default_theme` and `resources_prefix`.
* quench-archive: `checksum::{Crc32, adler32, xxh64}`, `deflate::inflate` (consumed-byte count), `zip::EntryInfo`, `zip::Method`.
* quench-qr: the `dump` example (`cargo run -p quench-qr --example dump -- <L|M|Q|H> <text>`).
* quench-image: exact limits (16 384 / 64 Mi pixels), exact `Unsupported` names.

## 3. Open questions and suspected bugs

Security and robustness (no source was changed):

* `quench-crypto/src/argon2.rs:442` `verify_password` takes `m`, `t`, `p` from the stored string without any upper bound; `m` up to `u32::MAX` KiB (about 4 TiB) means a hostile hash string can request huge allocations or near-endless CPU. Safe only for hashes the service wrote itself.
* `quench-crypto/src/jwt.rs:295,473-475`: `Validation::new` sets `validate_aud: true` with `aud: None`, so `decode` returns `InvalidAudience` for every token until the caller sets `aud` or clears `validate_aud`. Probably intended to mirror `jsonwebtoken`; worth confirming it is not a trap for new users.
* quench-crypto: keys, seeds and TOTP secrets are not zeroized on drop; `x25519::x25519` does not reject low-order points (only `age::decrypt` checks the all-zero output); TOTP has no replay protection.
* `quench-html/src/dom.rs:816`: every raw-text start tag (`script`, `style`, `textarea`, `title`, `iframe`, ...) lower-cases the entire remaining input, so parse time is quadratic in input size times the number of such tags (20 000 `<script>` tags took about 4 s in a debug build). Implicit-end-tag scans (`in_scope`) walk the open-element stack, so deep nesting is quadratic too (5 000 nested `<div>` about 0.5 s; 200 000 did not finish in the time available).
* `quench-html/src/dom.rs:280` `write_html` (`Node::html`/`inner_html`) is recursive: a very deep tree can overflow the stack. There is no depth or size limit in the parser.
* `quench-html/src/entities.rs:2159` `numeric()` does not apply the HTML windows-1252 remapping for `&#128;`..`&#159;` (HTML5 does).
* quench-html has no sanitizer; the Markdown renderer passes raw HTML and `javascript:` URLs through. Documented as a caveat; consider a `sanitize` module if untrusted Markdown is rendered anywhere.
* `quench-image/src/lib.rs` (PNG path): the zlib Adler-32 trailer is not verified (the deflate stream is decoded and the last four bytes ignored); CRC-32 of every chunk is verified.
* `quench-archive/src/gzip.rs:73`: the optional header CRC-16 (`FHCRC`) is skipped, not verified.
* `quench-archive/src/zip.rs:292`: a non-UTF-8 entry name without the UTF-8 flag is decoded lossily, not as CP437 as the zip spec says.
* `quench-archive/src/zip.rs:178` `Archive::read` panics when `index >= entries().len()` (documented in the source).
* quench-archive: no path sanitising in `tar::Reader` or `zip::Archive` (by design; callers must validate). zstd decoder ignores the window size (memory is bounded by `limit`).
* `quench-qr/src/lib.rs` `QrCode::capacity(version, level)` is `pub const fn` but unchecked: version 0 returns a meaningless 6 and versions above 40 panic (index out of bounds in `tables.rs`).
* `quench-pdf`: `Document` holds `RefCell`/`Rc` so it is neither `Send` nor `Sync`; `Page`/`Object`/`Dict` are unnameable (see section 1). Encryption is detected only from a trailer `/Encrypt`, after a full load.
* `quench-proto`: the server driver has no cap on request body size and no limit on connection count (left to `quench-http`); the client has no timeout or size limit for response bodies. `ws::WebSocket::recv` returns `Message::Pong` to the caller but swallows pings (asymmetric; documented).
* `quench-proto/src/h1/server.rs`: a response with both a handler-set `Content-Length` and a body of a different length makes the driver fail with `Error::BodyLength` (for bodies over 16 KiB the head is already on the wire by then, and the connection is dropped); callers should not set `Content-Length` by hand.

Unclear intent:

* quench-style `assets::create_asset_files(default_theme, resources_prefix)` ignores both parameters (`src/assets.rs`); the underscore-prefixed names in `create_asset_files_with_options` suggest leftovers from the move out of `quench-web`.
* quench-style `body { user-select: none }` and `* { color: ... }` in the shared stylesheet apply globally (`styles/common/root.rs`); confirm this is wanted for non-shell pages.
* quench-svg: `switch` draws all children (no `systemLanguage` evaluation); `defs`/`symbol` content is only reachable through `use`.
* quench-image: GIF `background colour` and frame disposal are ignored; only the first frame is returned.

## 4. Counts

Public items documented per crate (counted as lines of `pub fn|struct|enum|trait|const|static|type|mod|use` in `src/`, so private-module items and constants such as the 62 status codes and 71 header names are included; for `quench-proto` and `quench-style` the number is dominated by constants):

| Crate | `pub` item lines |
|---|---|
| quench-proto | 407 |
| quench-crypto | 126 |
| quench-archive | 99 |
| quench-style | 108 |
| quench-html | 72 |
| quench-pdf | 45 |
| quench-qr | 20 |
| quench-image | 13 |
| quench-svg | 6 |

## 5. Links

* `docs/README.md`: links of the nine crates now point at `./crates/quench-<crate>/index.md`; its tree listing (`├── quench-archive.md` etc.) is generated text and was left.
* The repository root `README.md` (outside `docs/`, not a crate README) still links `./docs/quench-<crate>.md` for all nine crates; those targets no longer exist. It was not changed under the "documentation only inside docs/" rule.
