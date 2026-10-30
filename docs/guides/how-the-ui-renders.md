---
title: How the UI renders
parent: Guides
nav_order: 11
---

One view tree, two ways to show it. `quench-ui` builds a tree of nodes through the `Renderer` trait and patches single nodes when signals change. A renderer decides what a node is: in the browser it is a DOM element driven from WebAssembly; natively it is an entry in an in-memory DOM that is styled, laid out and painted into pixels by the crates below.

## The common part: views to renderer calls

1. Your component functions return a `View` (`quench_ui::view`). Reactive parts are closures (`text_fn`, `attr_fn`, `class_if`, `View::dynamic`, `View::keyed`).
2. `mount(&renderer, &parent, view)` walks the view in document order and calls `Renderer` methods: create an element, text or anchor, set text and attributes, insert, listen, and so on. Each reactive closure becomes an effect owned by the mount's scope; when a signal it read changes, only that effect re-runs and issues one call (for example one `set_text`). Safety rules (escaping, dropping `on*` attributes, blocking `javascript:` URLs) are applied here, so every renderer gets them. See [quench-ui render API](../crates/quench-ui/api-render.md).
3. Events flow the other way: the renderer calls the stored `Handler` with an `Event`; handlers run batched.

`TestRenderer` and `render_to_string` are renderers too (an in-memory tree you can inspect, and an HTML string for servers).

## Native path: CSS to pixels

`quench-ui-native` provides `NativeRenderer`, a `Renderer` that keeps a DOM in memory and draws it. For a frame (`NativeRenderer::render(canvas)`):

```text
Renderer calls            +--------------------+
(from mount/effects)  --> |  in-memory DOM     |   tags, attributes, text, listeners, focus/hover state
                          +---------+----------+
                                    | cascade
                                    v
                          quench-css: user-agent sheet (ua.css) + page sheets
                          selectors match through the `Element` trait over the DOM,
                          computing a `ComputedStyle` per element (inheritance,
                          custom properties, calc, media queries, specificity)
                                    |
                                    v   tree of `quench_layout::Node` (style, key, text/children/replaced)
                          quench-layout: `layout(&root, viewport, &fonts)`
                          block, inline, flex, grid, positioning, margin collapsing;
                          text measured with quench-text (Noto Sans embedded)
                                    |
                                    v   `LayoutBox` tree (border boxes, fragments, overflow)
                          quench-paint: `paint(canvas, &boxes, &fonts, scroll, decorations)`
                          shadows, backgrounds, borders, clipping, text, outlines,
                          opacity layers, transforms, scroll bars; decorations for
                          checkmarks, images, carets supplied by the renderer
                                    |
                                    v   calls on `Canvas`
                          quench-raster: premultiplied BGRA pixels
                          rects, rounded rects (signed distance), shadows (Gaussian),
                          paths (exact area coverage), images, layers
```

Details that matter:

* **Style.** `NativeRenderer::cascade()` builds a `Cascade` from the user-agent stylesheet and the sheets added to the DOM, once, and reuses it until sheets change. The root style has `font-size` 16 px times the zoom. Pseudo-class state (`:hover`, `:active`, `:focus`, `:focus-visible`, `:disabled`, `:checked`) is answered from the DOM and the input state. See [quench-css supported](../crates/quench-css/supported.md).
* **Layout.** `NativeRenderer::layout()` rebuilds the node tree from the DOM and runs `layout`; the result is kept, because pointer events are matched against it. See [quench-layout supported](../crates/quench-layout/supported.md).
* **Frame caching.** A frame reuses the last layout when only the paint can differ (a scroll, a caret blink): not stale, no restyle, no running animation. Otherwise the whole document is restyled and laid out again; there is no incremental layout.
* **Paint.** The page background is the first non-transparent background among the root and its first child; the canvas is cleared with it. Images referenced by `img` and `background-image` are loaded through the host's loader (or `data:` URLs) and passed to the painter as `Decoration`s, as are checkbox/radio marks and the progress bars. See [quench-paint painting](../crates/quench-paint/painting.md).
* **Raster.** The canvas bytes are the frame, in the layout Wayland and Android surfaces expect. See [quench-raster internals](../crates/quench-raster/internals.md).
* **Text.** The same `Fonts` (four Noto Sans weights, optional system fonts) measures in layout and draws in paint, so line breaks agree. See [quench-text internals](../crates/quench-text/internals.md).
* **Input and accessibility.** Pointer and keyboard events are hit-tested against the kept layout and turned back into `Event`s for the handlers; an accessibility tree is derived from the DOM. These are covered in the [quench-ui-native](../crates/quench-ui-native/index.md) pages.

## Browser path: WebAssembly and `quench.js`

1. The app compiles to `wasm32-unknown-unknown` with `quench_ui_web::app!(my_app)`, which defines the exported `q_main`. A page loads `quench.js` (its source is `quench_ui_web::QUENCH_JS`, for a server to serve), instantiates the module and calls `q_main`.
2. `quench_ui_web::run` installs the panic hook and the web platform (timers, clock, reduced-motion), then: if the root element already has content, `hydrate_root` mounts the view **over the server-rendered markup** (each created node claims the next existing node; mismatches are replaced and counted); otherwise `mount_root` builds the DOM from scratch. The mount is leaked: it lives as long as the page.
3. `DomRenderer` implements `Renderer` by calling imports that `quench.js` provides (create/insert/set attribute/listen/...). Nodes are handles into a table on the JS side.
4. Events go the other way: `quench.js` encodes the DOM event and calls the exported `q_dispatch_event`, which decodes it (`decode_event`, `kind_code`) and runs the handler. Other exports: `q_fetch_done`, `q_sse_event`, `q_timer_fire`, `q_tick`, `q_popstate`, `q_set_location`, `q_set_env`, `q_set_state`, `q_alloc`, `q_dealloc`. `fetch`, `EventSource`, history and timers are exposed to Rust through `Fetch`, `EventSource`, `BrowserHistory` and `WebPlatform`, with futures run by a small executor (`spawn`, `sleep`).
5. Styling is the browser's: your stylesheet applies to real DOM nodes. Under a strict Content-Security-Policy there is no inline `style` attribute: `Element::css` goes through the style object.
6. Off `wasm32` the JS side is replaced by a mock host (`quench_ui_web::mock`), so everything but the thin wasm imports runs under `cargo test`.

The byte-level contract of the bridge is in the [quench-ui-web](../crates/quench-ui-web/index.md) pages.

## Server rendering

`quench_ui::render_to_string(view)` mounts into a `TestRenderer` and returns the HTML (regions appear as `<!---->` anchors). A server sends it with the wasm and `quench.js`; the browser path above hydrates it.

## Choosing

| | Browser | Native |
|---|---|---|
| Style engine | the browser's CSS | `quench-css` subset |
| Layout | the browser | `quench-layout` (block, inline, flex, grid, positioning) |
| Text | the browser | `quench-text` (Noto Sans, no shaping beyond kerning) |
| Output | DOM | a pixel canvas handed to a window system |
| Same app code | yes | yes |

*Unverified:* the numbered browser steps summarise `quench-ui-web/src/lib.rs` and `ffi.rs` names; the exact import list of `quench.js` is documented by the quench-ui-web pages.
