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
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]

pub mod proto;

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

/// A finger on the screen.
#[derive(Debug, Clone, Copy)]
struct Touch {
    start: (f32, f32),
    last: (f32, f32),
    scrolling: bool,
}

/// The state of the program: the renderer, the window it draws into and what it last told the host.
#[allow(clippy::struct_excessive_bools)] // independent flags of the program's state
pub struct Surface<W: Write> {
    renderer: Rc<NativeRenderer>,
    out: W,
    size: Option<(u32, u32)>,
    density: f32,
    touch: Option<Touch>,
    dark: bool,
    visible: bool,
    ime: Option<ImeState>,
    tree: Vec<u8>,
    caret_on: bool,
    blink_at: Instant,
    exit: bool,
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
        Self {
            renderer: Rc::clone(renderer),
            out,
            size: None,
            density: 1.0,
            touch: None,
            dark: true,
            visible: true,
            ime: None,
            tree: Vec::new(),
            caret_on: true,
            blink_at: Instant::now() + BLINK,
            exit: false,
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
        }
    }

    /// A touch becomes a tap or a scroll.
    fn touch(&mut self, action: u8, x: f32, y: f32) {
        match action {
            0 => {
                self.touch = Some(Touch {
                    start: (x, y),
                    last: (x, y),
                    scrolling: false,
                });
                self.renderer.pointer_down(x, y);
            }
            1 => {
                let Some(mut t) = self.touch else { return };
                let slop = SLOP * self.density;
                if !t.scrolling && (x - t.start.0).hypot(y - t.start.1) > slop {
                    // Moved too far to be a tap: the finger scrolls what is under it, and nothing is clicked.
                    t.scrolling = true;
                    self.renderer.pointer_cancel();
                }
                if t.scrolling {
                    self.renderer.wheel(x, y, t.last.0 - x, t.last.1 - y);
                }
                t.last = (x, y);
                self.touch = Some(t);
            }
            2 => {
                if let Some(t) = self.touch.take()
                    && !t.scrolling
                {
                    self.renderer.pointer_up(x, y);
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

    /// The next timer: the caret blinks while a text field has focus.
    #[must_use]
    pub const fn next_timer(&self) -> Instant {
        self.blink_at
    }

    /// Runs the timers that are due.
    pub fn timers(&mut self, now: Instant) {
        if now >= self.blink_at {
            self.caret_on = !self.caret_on;
            if self.renderer.focused().is_some() {
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
        let Some((w, h)) = self.size else {
            return Ok(());
        };
        if !self.visible || !self.renderer.take_dirty() {
            return Ok(());
        }
        let mut canvas = Canvas::new(w as usize, h as usize);
        self.renderer.render(&mut canvas);
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

/// Runs the program over any pair of streams (what [`run`] does with standard input and output).
///
/// # Errors
/// The output's.
pub fn serve(
    renderer: &Rc<NativeRenderer>,
    mut input: impl Read + Send + 'static,
    output: impl Write,
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
    let mut surface = Surface::new(renderer, output);
    while !surface.finished() {
        let wait = surface
            .next_timer()
            .saturating_duration_since(Instant::now());
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
        surface.flush()?;
    }
    Ok(())
}
