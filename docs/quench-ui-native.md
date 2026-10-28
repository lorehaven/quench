# Quench UI Native

`quench-ui-native` (crate `quench_ui_native`) is a [`quench_ui::Renderer`](./quench-ui.md) that draws itself. Views are built into an in-memory DOM (as with `TestRenderer`), and [`NativeRenderer::render`] runs the pipeline on it: [`quench-css`](./quench-css.md) styles every element, [`quench-layout`](./quench-layout.md) places the boxes, [`quench-paint`](./quench-paint.md) draws them into a canvas. No WebView, no browser. It is the last piece of the [native renderer](../plans/NATIVE_RENDERER.md) of slice 2, and since slice 3 it also takes input.

```rust
use std::rc::Rc;
use quench_ui::prelude::*;
use quench_ui_native::NativeRenderer;

let r = Rc::new(NativeRenderer::new());
r.add_stylesheet(&quench_style::components::stylesheet());   // after the browser's own
r.set_viewport(390.0, 844.0);
let _mounted = mount(&r, &r.root(), my_view());              // root() is the <body>
let mut canvas = quench_raster::Canvas::new(390, 844);
let boxes = r.render(&mut canvas);                           // style, layout, paint; the layout comes back
```

The host decides what the page cannot know: `set_viewport` (the window), `set_media` (`prefers-color-scheme`, `hover`, `pointer`, `prefers-reduced-motion`; the size follows the viewport) and `set_safe_area` (what `env(safe-area-inset-*)` is). Changing any of them restyles. `layout()` does the first two stages alone; `kind`, `attrs`, `styles`, `children` and `text_of` walk the document (the gate uses them to write it out as HTML).

## The browser's sheet

`UA_STYLESHEET` is the browser's own style (modelled on Chromium's), written as CSS and read by the same engine: `html`/`body` (8 px margin), block elements, headings with their sizes and margins, paragraphs, lists, links, `b`/`i`, form controls (buttons with `box-sizing: border-box`, text fields with their border and padding, 13 px checkboxes and radio buttons with a 2 px radius or a circle and their checked colour), `[hidden]` and `<head>` stuff as `display: none`. The page's sheets come after it, then `style` attributes and what `set_style` sets (parsed as declarations, winning over every rule).

## Elements that are not plain boxes

- **Buttons and text fields centre their content** between top and bottom, as a browser's inner box does: when the page leaves them `inline-block` or `block`, they are laid out as a column flex container with `justify-content: center`, their children wrapped in one anonymous block so inline content still flows together.
- **`<input>`**: shows its `value` attribute, else its `placeholder` in grey; passwords show bullets; `date` and `time` show their value as `mm/dd/yyyy` and `hh:mm AM` or the empty pattern in the text colour. An empty field is still one line tall. Checkboxes and radio buttons are empty boxes with a `checked` attribute drawing the mark through a `Decoration`. `type=hidden` is not shown. `<textarea>` shows its value or placeholder in at least `rows` lines. `<br>` is a line break.
- **The page background** is the root's, or else the body's, and fills the whole canvas.
- Selectors see the DOM through `Element`: tags, `id`, `class`, every attribute (so `aria-checked`, `disabled`, `checked` work), parent and previous sibling. `:hover`, `:focus` and `:active` are false until slice 3 gives the renderer a pointer.

## Interaction

The host calls `pointer_move`, `pointer_down`, `pointer_up`, `pointer_cancel`, `wheel`, `key_down` (returns whether a listener took the key), `key_up` and `text`; the renderer finds the target in the last layout (text fragments and inline boxes count, so a click on words lands on their `<span>`; fixed boxes ignore the scrolling of what they are in), delivers the events through the DOM with bubbling (`Click`, `PointerDown/Up/Move`, `KeyDown/Up`, `Input`, `Focus`/`Blur` without), and does what a browser does unless a listener called `prevent_default`: a click is a press and release on the same thing (on their common ancestor); checkboxes and radio buttons toggle (a radio group is exclusive); a click on a `<label>` clicks its control; focus follows the mouse and **Tab** (document order, disabled and hidden skipped, trapped inside `trap_focus`); Enter and Space press a button; arrow keys, Page Up/Down, Home and End scroll; the wheel scrolls the nearest container that can move, then the document (`scroll_of(0)`), clamped, with a `Scroll` event; a thin bar shows where in a scroll container (or the page) you are. `:hover`, `:active`, `:focus` and `:focus-visible` (keyboard focus, or any focus in a text field) are real. `take_dirty()` tells the host when to redraw.

**Text fields** keep their value in the `value` attribute (so a view's `attr_fn("value")` and the field agree): typing, Backspace and Delete (by word with Ctrl), the arrows (Home, End, Shift for a selection, Ctrl for words), Ctrl+A, and an internal clipboard for Ctrl+C, X and V; a click puts the caret where it landed; Enter submits the enclosing form (a textarea takes the line break); the caret and selection are drawn; `Input` events carry the new value.

**Input methods**: `ime_state()` (the field's text, caret, anchor and caret rectangle) and `ime_apply(delete_before, delete_after, commit, preedit)`: the composition shows in the field underlined and is not in the value until committed (`CompositionStart` and `CompositionEnd` events). **Scale**: `set_scale(density)` makes a CSS pixel that many device pixels (absolute lengths, the root font size, `thin`/`medium` borders), `@media` and `env()` stay right.

## Images, timers and threads

**Images**: `<img src>` is drawn from `data:` URLs (base64 or percent-escaped), bytes registered with `add_image(src, bytes)` or an image loader (`set_image_loader`) the host sets; [`quench-image`](./quench-image.md) decodes PNG, JPEG and GIF and [`quench-svg`](./quench-svg.md) draws SVG (made at the page's density). A box with only a width or a height keeps the image's proportions; with neither, the natural size (CSS pixels times the scale); a missing or undecodable image shows its `alt` text. Failures are remembered, not retried. **`background-image: url(...)`** (and the `background` shorthand) is drawn over the padding box, after the colour and before the border, sized by `background-size` (`auto`, `cover`, `contain`), placed by `background-position` and tiled unless `no-repeat`; gradients are not drawn, and a background is not clipped to rounded corners. [`quench-image`](./quench-image.md) also decodes WebP. Not built: animation of images.

**Timers and threads**: `NativeRenderer::new()` installs a platform for `quench-ui` (a clock, one-shot timers, the reduced-motion preference from the media), so `set_timeout`-style code, toasts and delays work. The host calls `tick()` each time it wakes and waits at most `next_wake()`. `channel::<T>(handler)` returns an `mpsc::Sender<T>` that any thread can send to; the handler runs on the renderer's thread during `tick()` (a worker thread's answers become signal updates this way).

**Selection by touch**: `select_word_at(x, y)` (a long press) focuses the text field there and selects the word under the point; `selection_handles()` says where the two ends of the selection are on the screen and `move_selection_end(end, x)` moves one (a handle being dragged), so a host can draw handles and let a finger drag them. A click on a submit button (`<button>` with no other type, `<input type=submit>`) submits its form.

**Clipboard**: Ctrl+C and X record the text (`take_clipboard_out()` returns it once, for the host to put on the system clipboard); Ctrl+V pastes what `set_clipboard(text)` last set (the host reads the system clipboard into it first).

**Transitions**: when an element's `opacity`, `color`, `background-color` or `border-color` changes and its `transition` names the property (or `all`), the old value moves to the new one over the duration, after the delay, along the named curve; a change half way starts from what is shown; the first style an element gets is not transitioned; `prefers-reduced-motion` makes changes immediate. The renderer remembers each element's last style while styles are computed, `next_wake()` asks for a frame every 16 ms while any transition runs and `tick()` reports it, so a host that ticks as above animates with no more code. `advance_animations(by)` moves the clock they use (for tests, and for a still at a chosen moment). **Keyframe animations** (`animation: name 1s linear infinite`) play from the `@keyframes` of the cascade: each of opacity, colours (`color`, `background-color`, `border-color`) and `transform`/`rotate` is interpolated between the keyframes that set it (from the element's own value at an end the keyframes leave out), with the timing function between each pair, the iteration count, direction (`alternate` and `reverse`) and fill mode (`backwards`, `forwards`, `both`); reduced motion shows the plain style. Transitions and animations of sizes are not played.

**Sliders and progress**: `<input type=range>` (with `min`, `max`, `step`, default step 1) moves with the arrow keys, Page Up and Down, Home and End and follows a press or a drag, firing `Input`; `<progress>` and `<meter>` show how full they are. Both are drawn from the UA sheet with bars.

**Layout** also does **CSS grid**: see [Quench Layout](./quench-layout.md#grid).

## Accessibility

`accessibility_tree()` returns the page as `AccessNode`s: role (heading with its level, button, link, checkbox, radio, switch, text box, list, tab and tab list, dialog, navigation, status, alert, image, form...), the accessible name (`aria-label`, `aria-labelledby`, a wrapping or `for` label, the content, `title`, `alt`, the placeholder), the description (`aria-describedby`), the value and caret of a field, the states (focusable, focused, disabled, checked, selected or pressed, expanded, required, read-only), the actions (click, focus, scroll into view), the range of a slider or progress bar (`Role::Slider`, `Role::ProgressBar`, `range: (min, max, now)`; `now` is `None` while indeterminate), `set_field_value(id, text)` for an assistive technology that sets a field's text and the rectangle **on the screen** (scrolling applied, fixed boxes where they are). `aria-hidden`, `hidden`, `display: none` and `visibility: hidden` are left out; a `div` that only groups is flattened. `perform(id, action)` carries out what an assistive technology asks: focus, a click at the node's centre, scrolling it into view. Two bridges read this tree: [AT-SPI on Linux](./quench-desktop-atspi.md) and the [Android accessibility node provider](./quench-android-surface.md).

## The gate

`scripts/native-gate.mjs [width] [height] [out-dir]` is the check of slice 2. `examples/native_gallery` mounts the [component gallery](./quench-ui-web.md) into a `NativeRenderer` with the same stylesheets the web page loads (the dark and light themes under `@media (prefers-color-scheme)`, tokens, mobile, components, and the page's own), and writes `page.html` (the same DOM, every element keyed), `layout.json` (every box) and `native.png`. The script loads `page.html` in **headless Chromium** (its own throwaway page, dark scheme, device scale 1, driven over the DevTools protocol; nothing of the desktop is captured), reads `getBoundingClientRect()` of every element, and compares: the native box against Chromium's for every element that is a box (inline elements have none of ours), and the two screenshots pixel by pixel. It writes `chromium.png` and a `diff.png` with the differing pixels in red, prints the worst boxes (`GATE_LIST`, `GATE_ORDER=1` for document order, `GATE_DUMP=k1,k2` for chosen keys), and fails below 90 % of boxes within 3 px or above 8 % of pixels differing by more than 32 levels (`GATE_GEOMETRY`, `GATE_PIXELS`).

Measured on the gallery's controls tab: at 390 x 844, **55 of 57 boxes within 1 px (96.5 %)** and **1.3 % of pixels** differing; at 1280 x 800 the same boxes and **0.5 %** of pixels. The two boxes that differ are the glyphs of the bottom navigation (a symbol from a fallback font: the machine's fonts are chosen by coverage, Chromium's by fontconfig's preference, so a symbol can come out of a different face); the pixels that differ are those symbols, the menu glyph, the date and time pickers' icons (not drawn), and sub-pixel text. A full frame of the gallery takes about 130 ms unoptimised (debug) on this machine; nothing is cached between frames yet.

## Next steps: other platforms

The native renderer runs on Linux (Wayland) and Android. Each of these is a host port, not a renderer change: the renderer takes pointer, key, text and scale calls, draws into a BGRA canvas and exposes an accessibility tree, so a host has to supply a window, input, text input (IME), a clipboard, an accessibility bridge and a loop that calls `tick()`.

- **X11** (Linux without Wayland): a client on the X protocol with `MIT-SHM` frames, XKB keys, XIM or IBus for input methods, selections for the clipboard; the AT-SPI service is shared with the Wayland host.
- **Windows**: a Win32 window (`CreateWindowEx`, `StretchDIBits` or Direct2D), `WM_*` input, TSF for input methods, the Win32 clipboard, UI Automation for accessibility; the system calls are `unsafe` and would live in one allow-listed file as `sys.rs` does for Wayland.
- **macOS**: an `NSWindow` with a layer-backed view, `NSTextInputClient` for input methods, `NSPasteboard`, NSAccessibility; reached through the Objective-C runtime by hand, because no third-party crates are allowed.
- **Multi-window**: `Host` owns one window; several need a renderer per window, a shared event loop and a way for views to open windows.
