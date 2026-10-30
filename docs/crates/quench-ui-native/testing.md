---
title: Testing
parent: quench-ui-native
nav_order: 9
---

## The unit suite

`quench-ui-native/tests/unit.rs` (2030 lines, 41 tests) is a custom harness (`[[test]] name = "unit"`, `harness = false`, `autotests = false`) using `#[quench_test::test]`. Run:

```bash
cargo test -p quench-ui-native --test unit
```

Last run for this documentation: `41 passed; 0 failed`. The doc-test of `channel` is `ignore`d.

Helpers in the file show the pattern for testing code built on this crate:

* `page(css, view, w, h)`: new renderer, `add_stylesheet`, `set_viewport`, `mount` (the `Mount` is `std::mem::forget`ed), `render` into a `Canvas`; returns `(renderer, canvas, boxes)`. Assertions read pixels (`canvas.pixel(x, y)`) or boxes.
* `live(css, view, w, h)`: same, but returns the `Mount` so listeners stay alive, and draws once so the layout exists for events.
* `FLAT`: `body { margin: 0 } button, input, textarea { display: block; box-sizing: border-box; width: 100px; height: 30px; margin: 0; padding: 0; border: 0; background: white }`, a stylesheet that makes controls plain rectangles at predictable positions.
* `flat(&r.accessibility_tree())`: flatten the tree to find nodes by name.

Areas covered (by test name): mounting/styling/painting; UA sheet defaults; cascade order; attributes and classes in selectors; removal; page background; input value/placeholder/check marks; media and safe area; DOM walking; form controls; state and sibling selectors; list/link styling; click rules; bubbling and `prevent_default`; hover/active; checkbox/radio toggles by click, label and Space; Tab and Enter; focus/blur events; typing and editing; caret placement; Enter submit and textarea newline; wheel scrolling limits; focus trap; IME composition, commit and deletion; accessibility roles, names, states, bounds; bounds under scrolling and fixed boxes; scale; images (data URLs, registered bytes, loader); timers, channels and the motion preference; `touch-action`; cancelled touches and released drags; background images; SVG at density; sliders, progress and `set_field_value`; transitions; keyframe animations; submit buttons; long-press selection and handles; layout reuse; caret of an empty field.

Example (from the suite) of event testing:

```rust
let (r, _m) = live(FLAT, view, 200, 100);
r.pointer_down(10.0, 10.0);
r.pointer_up(10.0, 10.0);
assert_eq!(clicks.get(), 1);
r.pointer_down(10.0, 10.0);
r.pointer_up(10.0, 40.0);                 // released on another button
assert_eq!(clicks.get(), 1);              // not a click
```

Animation tests use `advance_animations(Duration)` instead of sleeping; timer tests use real sleeps of a few tens of milliseconds.

## The Chromium gate

`scripts/native-gate.mjs [width] [height] [out-dir]` compares the native renderer with headless Chromium on the component gallery. `examples/native_gallery` (package `quench-example-native-gallery`) mounts the gallery into a `NativeRenderer` with the same stylesheets the web page loads and writes `page.html`, `layout.json` and `native.png`; the script loads `page.html` in its own headless Chromium (`CHROMIUM` env var, default `chromium`, remote debugging on a random port 9800-10199, device scale 1, no desktop capture), reads `getBoundingClientRect()` of every element, and compares boxes and pixels. It fails below 90 % of boxes within 3 px or above 8 % of pixels differing by more than 32 levels (`GATE_GEOMETRY`, `GATE_PIXELS` override). It writes `chromium.png` and `diff.png` and prints the worst boxes (`GATE_LIST`, `GATE_ORDER=1`, `GATE_DUMP=k1,k2`).

*Unverified (not re-run for this documentation; figures from the previous page):* on the controls tab at 390 x 844, 55 of 57 boxes within 1 px (96.5 %) and 1.3 % of pixels differing; at 1280 x 800, 0.5 % of pixels. The differing boxes were fallback-font symbols.

`scripts/native-measure.mjs` also exists (not described in the previous docs; *Unverified:* its purpose).

## Testing a host

A host test needs no window: call the input methods, `tick`, `take_dirty`, `render` into a `Canvas`, and read `accessibility_tree()`. `quench-desktop-native` and `quench-android-surface` do exactly that around their own surfaces.
