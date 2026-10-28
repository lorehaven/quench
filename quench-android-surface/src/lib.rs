//! The program side of an Android app drawn by the native renderer.
//!
//! The Kotlin host owns a `SurfaceView`, the input method and the accessibility services. It starts this program
//! (a separate process, as Quench's other native code is: no JNI) and talks to it over its standard input and output
//! ([`proto`]): touches, keys and text go in; frames, keyboard requests, the focused field and the accessibility
//! tree come out. [`run`] is the whole program: mount your views into a [`NativeRenderer`] and call it.
//!
//! ```no_run
//! use std::rc::Rc;
//! use quench_ui_native::NativeRenderer;
//!
//! let renderer = Rc::new(NativeRenderer::new());
//! // renderer.add_stylesheet(...); mount(&renderer, &renderer.root(), view);
//! quench_android_surface::run(&renderer)?;
//! # Ok::<(), std::io::Error>(())
//! ```

// Pixel sizes and coordinates are small numbers; the wire is 32-bit.
#![allow(
    clippy::suboptimal_flops,
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

pub mod bridge;
pub mod proto;

pub use bridge::{Bridge, CallError};
/// The JSON value of a call's arguments and answer, and the map an object of arguments is made with.
pub use quench_serde::{Map, Value};

use std::io::{self, Read, Write};
use std::rc::Rc;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use quench_raster::Canvas;
use quench_ui::KeyEvent;
use quench_ui_native::{ColorScheme, ImeState, MediaEnv, NativeRenderer, Pointer};

use proto::{Event, Out, action_of, encode_tree, read_event, write_out};

/// How far a finger may move before a touch is a scroll and no longer a tap, in CSS pixels.
const SLOP: f32 = 8.0;
/// How long the caret is on and off.
const BLINK: Duration = Duration::from_millis(530);

/// How long the movement that sets a fling's speed is looked at, how fast a finger must go (device pixels a second)
/// for the content to keep moving, how quickly it slows down (per second) and when it is slow enough to stop.
const FLING_WINDOW: Duration = Duration::from_millis(100);
const FLING_MIN_SPEED: f32 = 150.0;
const FLING_FRICTION: f32 = 3.5;
const FLING_STOP_SPEED: f32 = 25.0;
/// How long a finger must stay before it selects a word, and how far from a handle's centre it takes hold of it
/// (in CSS pixels).
const LONG_PRESS: Duration = Duration::from_millis(500);
const HANDLE_REACH: f32 = 28.0;
/// The size of a handle's knob, in CSS pixels.
const HANDLE_KNOB: f32 = 9.0;

/// How often a fling moves the content.
const FLING_STEP: Duration = Duration::from_millis(16);

/// A finger on the screen.
#[derive(Debug, Clone)]
struct Touch {
    start: (f32, f32),
    last: (f32, f32),
    scrolling: bool,
    /// When the finger came down, and whether it has been held long enough to select a word.
    started: Instant,
    long: bool,
    /// A selection handle the finger took hold of: `false` for the start of the selection, `true` for its end.
    handle: Option<bool>,
    /// Where the finger was and when, over the last moments (to know how fast it left).
    samples: Vec<(Instant, f32, f32)>,
}

/// Content that keeps moving after the finger has left it.
#[derive(Debug, Clone, Copy)]
struct Fling {
    at: (f32, f32),
    /// The finger's velocity, device pixels a second.
    velocity: (f32, f32),
    last: Instant,
}

/// The state of the program: the renderer, the window it draws into and what it last told the host.
#[allow(clippy::struct_excessive_bools)] // independent flags of the program's state
pub struct Surface<W: Write> {
    renderer: Rc<NativeRenderer>,
    out: W,
    size: Option<(u32, u32)>,
    density: f32,
    touch: Option<Touch>,
    fling: Option<Fling>,
    dark: bool,
    visible: bool,
    ime: Option<ImeState>,
    tree: Vec<u8>,
    caret_on: bool,
    blink_at: Instant,
    exit: bool,
    bridge: Bridge,
}

impl<W: Write> std::fmt::Debug for Surface<W> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Surface")
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

/// The UTF-16 length of `text[..byte]`.
fn utf16_len(text: &str, byte: usize) -> i32 {
    text[..byte.min(text.len())].encode_utf16().count() as i32
}

/// The byte offset in `text` that is `units` UTF-16 units after (or, negative, before) `from`.
fn move_utf16(text: &str, from: usize, units: i32) -> usize {
    let mut at = from.min(text.len());
    if units >= 0 {
        let mut left = units;
        for c in text[at..].chars() {
            if left <= 0 {
                break;
            }
            left -= c.len_utf16() as i32;
            at += c.len_utf8();
        }
    } else {
        let mut left = -units;
        for c in text[..at].chars().rev() {
            if left <= 0 {
                break;
            }
            left -= c.len_utf16() as i32;
            at -= c.len_utf8();
        }
    }
    at
}

impl<W: Write> Surface<W> {
    /// A program that draws `renderer` and writes to `out`.
    pub fn new(renderer: &Rc<NativeRenderer>, out: W) -> Self {
        Self::with_bridge(renderer, out, Bridge::new())
    }

    /// A program whose calls to the host's plugins go through `bridge`.
    pub fn with_bridge(renderer: &Rc<NativeRenderer>, out: W, bridge: Bridge) -> Self {
        Self {
            renderer: Rc::clone(renderer),
            out,
            size: None,
            density: 1.0,
            touch: None,
            fling: None,
            dark: true,
            visible: true,
            ime: None,
            tree: Vec::new(),
            caret_on: true,
            blink_at: Instant::now() + BLINK,
            exit: false,
            bridge,
        }
    }

    /// Whether the program is finished (the page asked to exit, or the host went away).
    #[must_use]
    pub const fn finished(&self) -> bool {
        self.exit
    }

    fn media(&self) {
        // A phone has a touch screen: no hover, a coarse pointer.
        self.renderer.set_media(MediaEnv {
            color_scheme: if self.dark {
                ColorScheme::Dark
            } else {
                ColorScheme::Light
            },
            hover: false,
            pointer: Pointer::Coarse,
            ..MediaEnv::default()
        });
    }

    /// Does what an event from the host asks.
    pub fn handle(&mut self, event: Event) {
        match event {
            Event::Resize {
                width,
                height,
                density,
                insets,
            } => {
                eprintln!("surface: {width}x{height} at density {density}, insets {insets:?}");
                self.size = Some((width, height));
                self.density = density.max(0.5);
                self.renderer.set_scale(self.density);
                self.renderer.set_viewport(width as f32, height as f32);
                self.renderer.set_safe_area(insets);
                self.media();
            }
            Event::Touch { action, x, y } => self.touch(action, x, y),
            Event::Key { down, mods, name } => {
                let key = KeyEvent {
                    key: name.clone(),
                    shift: mods & 1 != 0,
                    ctrl: mods & 2 != 0,
                    alt: mods & 4 != 0,
                    meta: mods & 8 != 0,
                };
                if down {
                    self.renderer.key_down(&key);
                    // A single character with no shortcut modifier is text.
                    if name.chars().count() == 1 && !key.ctrl && !key.alt && !key.meta {
                        self.renderer.text(&name);
                    }
                } else {
                    self.renderer.key_up(&key);
                }
            }
            Event::Text(text) => self.renderer.ime_apply(0, 0, Some(&text), None),
            Event::Compose { text, cursor } => {
                let byte = if cursor < 0 {
                    -1
                } else {
                    move_utf16(&text, 0, cursor) as i32
                };
                self.renderer.ime_apply(0, 0, None, Some((&text, byte)));
            }
            Event::Delete { before, after } => {
                let (text, caret) = self
                    .renderer
                    .ime_state()
                    .map(|s| (s.text, s.cursor))
                    .unwrap_or_default();
                let from = move_utf16(&text, caret, -(before as i32));
                let to = move_utf16(&text, caret, after as i32);
                self.renderer
                    .ime_apply((caret - from) as u32, (to - caret) as u32, None, None);
            }
            Event::Access { id, action } => {
                if let Some(action) = action_of(action) {
                    self.renderer.perform(id as usize, action);
                }
            }
            Event::Scheme { dark } => {
                self.dark = dark;
                self.media();
            }
            Event::Back => {
                // A page that uses Escape for something (closing a dialog) takes it and stops the default.
                let used = self.renderer.key_down(&KeyEvent::new("Escape"));
                let _ = write_out(
                    &mut self.out,
                    &if used { Out::BackHandled } else { Out::Exit },
                );
            }
            Event::Visible(v) => self.visible = v,
            Event::Clipboard(text) => self.renderer.set_clipboard(&text),
            Event::Reply(envelope) => self.bridge.deliver(&envelope),
        }
    }

    /// A touch becomes a tap or a scroll.
    fn touch(&mut self, action: u8, x: f32, y: f32) {
        match action {
            0 => {
                // A finger on moving content stops it.
                self.fling = None;
                // A finger on a selection handle drags that end of the selection instead of pressing the page.
                let handle = self.handle_at(x, y);
                self.touch = Some(Touch {
                    start: (x, y),
                    last: (x, y),
                    scrolling: false,
                    started: Instant::now(),
                    long: false,
                    handle,
                    samples: vec![(Instant::now(), x, y)],
                });
                if handle.is_none() {
                    self.renderer.pointer_down(x, y);
                }
            }
            1 => {
                let Some(mut t) = self.touch.take() else {
                    return;
                };
                if let Some(end) = t.handle {
                    self.renderer.move_selection_end(end, x);
                    t.last = (x, y);
                    self.touch = Some(t);
                    return;
                }
                let slop = SLOP * self.density;
                if !t.scrolling && !t.long && (x - t.start.0).hypot(y - t.start.1) > slop {
                    // Moved too far to be a tap: the finger scrolls what is under it, and nothing is clicked.
                    t.scrolling = true;
                    self.renderer.pointer_cancel();
                }
                if t.scrolling {
                    self.renderer.wheel(x, y, t.last.0 - x, t.last.1 - y);
                }
                t.last = (x, y);
                let now = Instant::now();
                t.samples.push((now, x, y));
                t.samples
                    .retain(|(at, ..)| now.duration_since(*at) <= FLING_WINDOW);
                self.touch = Some(t);
            }
            2 => {
                if let Some(t) = self.touch.take() {
                    if t.scrolling {
                        self.start_fling(&t, x, y);
                    } else if t.long || t.handle.is_some() {
                        // The press was a selection, not a click.
                        self.renderer.pointer_cancel();
                    } else {
                        self.renderer.pointer_up(x, y);
                    }
                }
                // A finger leaves no hover behind.
                self.renderer.pointer_move(-1.0, -1.0);
            }
            _ => {
                if self.touch.take().is_some() {
                    self.renderer.pointer_cancel();
                }
                self.renderer.pointer_move(-1.0, -1.0);
            }
        }
    }

    /// Content keeps moving at the speed the finger had when it left it, if that was fast enough.
    fn start_fling(&mut self, touch: &Touch, x: f32, y: f32) {
        let now = Instant::now();
        let Some(&(from, fx, fy)) = touch
            .samples
            .iter()
            .find(|(at, ..)| now.duration_since(*at) <= FLING_WINDOW)
        else {
            return;
        };
        let seconds = now.duration_since(from).as_secs_f32();
        if seconds < 0.005 {
            return;
        }
        let velocity = ((x - fx) / seconds, (y - fy) / seconds);
        if velocity.0.hypot(velocity.1) >= FLING_MIN_SPEED {
            self.fling = Some(Fling {
                at: (x, y),
                velocity,
                last: now,
            });
        }
    }

    /// The next timer: the caret blinks while a text field has focus, and a fling moves the content.
    #[must_use]
    pub fn next_timer(&self) -> Instant {
        let mut next = self
            .fling
            .map_or(self.blink_at, |f| self.blink_at.min(f.last + FLING_STEP));
        if let Some(t) = &self.touch
            && !t.long
            && !t.scrolling
            && t.handle.is_none()
        {
            next = next.min(t.started + LONG_PRESS);
        }
        next
    }

    /// Draws a handle at each end of the selection: a stem along the caret and a knob below it.
    fn draw_handles(&self, canvas: &mut Canvas) {
        let Some(handles) = self.renderer.selection_handles() else {
            return;
        };
        let accent = quench_raster::Color::rgb(0, 117, 255);
        let (radius, stem) = (HANDLE_KNOB * self.density, 2.0 * self.density);
        for (x, top, height) in handles {
            canvas.fill_rect(x - stem / 2.0, top, stem, height + radius, accent);
            canvas.fill_rounded_rect(
                x - radius,
                top + height,
                radius * 2.0,
                radius * 2.0,
                radius,
                accent,
            );
        }
    }

    /// The knob of a selection handle near `(x, y)`: `false` for the start of the selection, `true` for the end.
    fn handle_at(&self, x: f32, y: f32) -> Option<bool> {
        let handles = self.renderer.selection_handles()?;
        let reach = HANDLE_REACH * self.density;
        let knob = |h: (f32, f32, f32)| (h.0, h.1 + h.2 + HANDLE_KNOB * self.density);
        let distance = |h| {
            let (kx, ky) = knob(h);
            (x - kx).hypot(y - ky)
        };
        let (a, b) = (distance(handles[0]), distance(handles[1]));
        (a.min(b) <= reach).then_some(b < a)
    }

    /// Runs the timers that are due.
    pub fn timers(&mut self, now: Instant) {
        // A finger that has stayed where it came down selects the word there.
        if let Some(t) = &mut self.touch
            && !t.long
            && !t.scrolling
            && t.handle.is_none()
            && now >= t.started + LONG_PRESS
        {
            t.long = true;
            let (x, y) = t.start;
            self.renderer.pointer_cancel();
            self.renderer.select_word_at(x, y);
        }
        if let Some(mut fling) = self.fling
            && now >= fling.last + FLING_STEP
        {
            let seconds = now.duration_since(fling.last).as_secs_f32().min(0.05);
            // The content follows the finger's direction: the page moves by what the wheel is told, reversed.
            self.renderer.wheel(
                fling.at.0,
                fling.at.1,
                -fling.velocity.0 * seconds,
                -fling.velocity.1 * seconds,
            );
            let slow = (-FLING_FRICTION * seconds).exp();
            fling.velocity = (fling.velocity.0 * slow, fling.velocity.1 * slow);
            fling.last = now;
            self.fling =
                (fling.velocity.0.hypot(fling.velocity.1) >= FLING_STOP_SPEED).then_some(fling);
        }
        if now >= self.blink_at {
            self.caret_on = !self.caret_on;
            // Only a text field has a caret; redrawing for a focused button every blink would keep the CPU busy.
            if self.renderer.text_field_focused() {
                self.renderer.set_caret_visible(self.caret_on);
            }
            self.blink_at = now + BLINK;
        }
    }

    /// Sends what changed since the last time: a frame, the soft keyboard, the field the input method types into
    /// and the accessibility tree.
    ///
    /// # Errors
    /// The output's: the host has gone away.
    pub fn flush(&mut self) -> io::Result<()> {
        for call in self.bridge.take_outbox() {
            write_out(&mut self.out, &Out::Call(&call))?;
        }
        if let Some(text) = self.renderer.take_clipboard_out() {
            write_out(&mut self.out, &Out::Clipboard(&text))?;
        }
        let Some((w, h)) = self.size else {
            return Ok(());
        };
        if !self.visible || !self.renderer.take_dirty() {
            return Ok(());
        }
        let mut canvas = Canvas::new(w as usize, h as usize);
        self.renderer.render(&mut canvas);
        self.draw_handles(&mut canvas);
        // The frame goes as RGBA, the byte order of an Android bitmap; the canvas is BGRA.
        let mut rgba = canvas.bytes().to_vec();
        for px in rgba.as_chunks_mut::<4>().0 {
            px.swap(0, 2);
        }
        write_out(
            &mut self.out,
            &Out::Frame {
                width: w,
                height: h,
                rgba: &rgba,
            },
        )?;
        let state = self.renderer.ime_state();
        if state != self.ime {
            match &state {
                Some(s) => {
                    if self.ime.is_none() {
                        write_out(&mut self.out, &Out::Keyboard(true))?;
                    }
                    write_out(
                        &mut self.out,
                        &Out::Ime {
                            text: &s.text,
                            cursor: utf16_len(&s.text, s.cursor),
                            anchor: utf16_len(&s.text, s.anchor),
                            rect: [s.rect.0, s.rect.1, s.rect.2, s.rect.3],
                        },
                    )?;
                }
                None => write_out(&mut self.out, &Out::Keyboard(false))?,
            }
            self.ime = state;
        }
        let tree = encode_tree(&self.renderer.accessibility_tree());
        if tree != self.tree {
            write_out(&mut self.out, &Out::Tree(&tree))?;
            self.tree = tree;
        }
        Ok(())
    }
}

enum Message {
    Event(Event),
    Closed,
}

/// Runs the program over the process's standard input and output until the host closes them.
///
/// # Errors
/// The output's.
pub fn run(renderer: &Rc<NativeRenderer>) -> io::Result<()> {
    serve(renderer, io::stdin(), io::stdout())
}

/// [`run`] for a program that calls the host's plugins through `bridge`.
///
/// # Errors
/// The output's.
pub fn run_with(renderer: &Rc<NativeRenderer>, bridge: &Bridge) -> io::Result<()> {
    serve_with(renderer, io::stdin(), io::stdout(), bridge)
}

/// Runs the program over any pair of streams (what [`run`] does with standard input and output).
///
/// # Errors
/// The output's.
pub fn serve(
    renderer: &Rc<NativeRenderer>,
    input: impl Read + Send + 'static,
    output: impl Write,
) -> io::Result<()> {
    serve_with(renderer, input, output, &Bridge::new())
}

/// [`serve`] for a program that calls the host's plugins through `bridge`.
///
/// # Errors
/// The output's.
pub fn serve_with(
    renderer: &Rc<NativeRenderer>,
    mut input: impl Read + Send + 'static,
    output: impl Write,
    bridge: &Bridge,
) -> io::Result<()> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("surface-input".into())
        .spawn(move || {
            loop {
                if let Ok(Some(event)) = read_event(&mut input) {
                    if tx.send(Message::Event(event)).is_err() {
                        return;
                    }
                } else {
                    let _ = tx.send(Message::Closed);
                    return;
                }
            }
        })?;
    let mut surface = Surface::with_bridge(renderer, output, bridge.clone());
    while !surface.finished() {
        let mut wait = surface
            .next_timer()
            .saturating_duration_since(Instant::now());
        if let Some(wake) = surface.renderer.next_wake() {
            wait = wait.min(wake);
        }
        match rx.recv_timeout(wait) {
            Ok(Message::Event(event)) => {
                surface.handle(event);
                // Whatever else has arrived is one change.
                while let Ok(Message::Event(event)) = rx.try_recv() {
                    surface.handle(event);
                }
            }
            Ok(Message::Closed) | Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {}
        }
        surface.timers(Instant::now());
        surface.renderer.tick();
        surface.flush()?;
    }
    Ok(())
}
