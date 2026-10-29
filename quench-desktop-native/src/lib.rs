//! Runs a [`quench_ui_native::NativeRenderer`] in a Wayland window.
//!
//! ```no_run
//! use std::rc::Rc;
//! use quench_ui_native::NativeRenderer;
//!
//! let renderer = Rc::new(NativeRenderer::new());
//! // renderer.add_stylesheet(...); mount(&renderer, &renderer.root(), view);
//! quench_desktop_native::run("My app", "dev.example.app", (800, 600), &renderer)?;
//! # Ok::<(), std::io::Error>(())
//! ```
//!
//! The loop draws a frame whenever the renderer says something changed, hands it the window's events (pointer,
//! wheel, keys, text, size), blinks the caret of a focused field, and repeats a held key as the compositor's
//! repeat settings say (the compositor leaves that to the program). [`Host`] is the same loop in steps, for a
//! program that has its own to run.

// Pixel sizes and wheel values are small numbers.
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_possible_wrap
)]
#![cfg(target_os = "linux")]

use std::io;
use std::rc::Rc;
use std::time::{Duration, Instant};

use quench_desktop_atspi::Service;
use quench_desktop_wayland::{Key, Modifiers, Window, WindowEvent};
use quench_ui::KeyEvent;
use quench_ui_native::{ImeState, NativeRenderer};

/// The Linux input code of the primary mouse button.
const BUTTON_LEFT: u32 = 0x110;
/// How the wheel's value (about 10 per notch) becomes pixels.
const WHEEL_SCALE: f32 = 4.0;
/// How long the caret is on and off.
const BLINK: Duration = Duration::from_millis(530);

/// A key being held down, for repeating.
struct Held {
    key: Key,
    modifiers: Modifiers,
    next: Instant,
}

/// Turns window events into calls on a renderer. It owns no window, so it can be driven by any source of
/// [`WindowEvent`]s (and by a test).
pub struct Controller {
    renderer: Rc<NativeRenderer>,
    held: Option<Held>,
    caret_on: bool,
    blink_at: Instant,
    size: (usize, usize),
    pointer: (f32, f32),
    closing: bool,
    /// What the input method was last told.
    ime: Option<ImeState>,
    /// Key repeat: characters per second and the delay before it starts, in milliseconds.
    repeat: (i32, i32),
}

/// A window and the renderer it shows.
pub struct Host {
    window: Window,
    control: Controller,
    /// What a screen reader talks to, when the accessibility bus is there.
    access: Option<Service>,
    /// Until when to poll quickly: a screen reader asked something lately and expects an answer soon.
    hurry_until: Instant,
}

/// The DOM's view of a key press.
#[must_use]
pub fn key_event(key: &Key, m: Modifiers) -> KeyEvent {
    KeyEvent {
        key: key.name.clone(),
        ctrl: m.ctrl,
        shift: m.shift,
        alt: m.alt,
        meta: m.meta,
    }
}

impl Controller {
    /// A controller for `renderer`, whose window is `size` pixels.
    #[must_use]
    pub fn new(renderer: &Rc<NativeRenderer>, size: (usize, usize)) -> Self {
        renderer.set_viewport(size.0 as f32, size.1 as f32);
        Self {
            renderer: Rc::clone(renderer),
            held: None,
            caret_on: true,
            blink_at: Instant::now() + BLINK,
            size,
            pointer: (0.0, 0.0),
            closing: false,
            ime: None,
            repeat: (25, 600),
        }
    }

    /// Whether the user asked for the window to close.
    #[must_use]
    pub const fn closing(&self) -> bool {
        self.closing
    }

    /// The size the window was last told to be.
    #[must_use]
    pub const fn size(&self) -> (usize, usize) {
        self.size
    }

    /// The text field state the input method should be told about now, if it differs from what it was told:
    /// `Some(None)` means no field has focus any more, `Some(Some(state))` a field or its text or caret changed.
    pub fn ime_change(&mut self) -> Option<Option<ImeState>> {
        let now = self.renderer.ime_state();
        if now == self.ime {
            return None;
        }
        self.ime.clone_from(&now);
        Some(now)
    }

    /// Whether the input method was enabled by the last [`ime_change`](Self::ime_change) (so a change is an
    /// update, not an enable).
    #[must_use]
    pub const fn ime_enabled(&self) -> bool {
        self.ime.is_some()
    }

    /// Sets the key repeat the compositor configured.
    pub const fn set_repeat(&mut self, repeat: (i32, i32)) {
        self.repeat = repeat;
    }

    /// When the next timer (caret blink, key repeat) is due.
    #[must_use]
    pub fn next_timer(&self) -> Instant {
        let blink = self.blink_at;
        self.held.as_ref().map_or(blink, |h| h.next.min(blink))
    }

    /// Runs the timers that are due: blinks the caret and repeats a held key.
    pub fn timers(&mut self, now: Instant) {
        if now >= self.blink_at {
            self.caret_on = !self.caret_on;
            if self.renderer.text_field_focused() {
                self.renderer.set_caret_visible(self.caret_on);
            }
            self.blink_at = now + BLINK;
        }
        if let Some(held) = &mut self.held
            && now >= held.next
        {
            held.next = now + Duration::from_secs_f32(1.0 / self.repeat.0.max(1) as f32);
            let (key, modifiers) = (held.key.clone(), held.modifiers);
            self.type_key(&key, modifiers);
        }
    }

    fn type_key(&self, key: &Key, m: Modifiers) {
        self.renderer.key_down(&key_event(key, m));
        if let Some(text) = &key.text
            && !m.ctrl
            && !m.alt
            && !m.meta
        {
            self.renderer.text(text);
        }
    }

    /// Does what `event` asks of the renderer.
    pub fn handle(&mut self, event: WindowEvent) {
        if quench_desktop_wayland::trace_enabled() && !matches!(event, WindowEvent::Frame) {
            let thin = matches!(event, WindowEvent::PointerMoved { .. })
                && !quench_desktop_wayland::trace_motion();
            if !thin {
                eprintln!("quench-host: {event:?}");
            }
        }
        match event {
            WindowEvent::CloseRequested => self.closing = true,
            WindowEvent::Resized { width, height } => {
                self.size = (width, height);
                self.renderer.set_viewport(width as f32, height as f32);
            }
            WindowEvent::PointerEntered { x, y } | WindowEvent::PointerMoved { x, y } => {
                self.pointer = (x as f32, y as f32);
                self.renderer.pointer_move(self.pointer.0, self.pointer.1);
            }
            WindowEvent::PointerLeft => self.renderer.pointer_move(-1.0, -1.0),
            WindowEvent::PointerButton {
                x,
                y,
                button,
                pressed,
            } if button == BUTTON_LEFT => {
                let (x, y) = (x as f32, y as f32);
                if pressed {
                    self.renderer.pointer_down(x, y);
                } else {
                    self.renderer.pointer_up(x, y);
                }
            }
            WindowEvent::Scroll { axis, value } => {
                let pixels = value as f32 * WHEEL_SCALE;
                let (dx, dy) = if axis == 0 {
                    (0.0, pixels)
                } else {
                    (pixels, 0.0)
                };
                self.renderer.wheel(self.pointer.0, self.pointer.1, dx, dy);
            }
            WindowEvent::Key {
                pressed,
                key,
                modifiers,
                ..
            } => {
                if key.modifier {
                    return;
                }
                if pressed {
                    self.type_key(&key, modifiers);
                    let repeats = key.text.is_some()
                        || matches!(
                            key.name.as_str(),
                            "Backspace"
                                | "Delete"
                                | "ArrowLeft"
                                | "ArrowRight"
                                | "ArrowUp"
                                | "ArrowDown"
                        );
                    let delay = Duration::from_millis(self.repeat.1.max(0) as u64);
                    self.held = repeats.then(|| Held {
                        key,
                        modifiers,
                        next: Instant::now() + delay,
                    });
                } else {
                    if self.held.as_ref().is_some_and(|h| h.key == key) {
                        self.held = None;
                    }
                    self.renderer.key_up(&key_event(&key, modifiers));
                }
            }
            WindowEvent::Ime(change) => {
                let preedit = change.preedit.as_ref().map(|(t, c, _)| (t.as_str(), *c));
                self.renderer.ime_apply(
                    change.delete_before,
                    change.delete_after,
                    change.commit.as_deref(),
                    preedit,
                );
            }
            _ => {}
        }
    }
}

impl Host {
    /// Hands an event to the controller, with the system clipboard on both sides of it: read before a paste,
    /// written after a copy or cut.
    fn deliver(&mut self, event: WindowEvent) -> io::Result<()> {
        if let WindowEvent::Key {
            pressed: true,
            key,
            modifiers,
            ..
        } = &event
            && modifiers.ctrl
            && key.name.eq_ignore_ascii_case("v")
            && let Some(text) = self.window.clipboard()?
        {
            self.control.renderer.set_clipboard(&text);
        }
        self.control.handle(event);
        if let Some(text) = self.control.renderer.take_clipboard_out() {
            self.window.set_clipboard(&text)?;
        }
        Ok(())
    }

    /// Opens the window, sizes the renderer's viewport to it and draws the first frame.
    ///
    /// # Errors
    /// No Wayland compositor, or one that lacks what a window needs.
    pub fn open(
        title: &str,
        app_id: &str,
        size: (usize, usize),
        renderer: &Rc<NativeRenderer>,
    ) -> io::Result<Self> {
        let window = Window::open(title, app_id, size.0, size.1)?;
        // Without an accessibility bus (no desktop session, no screen reader support) the window runs without.
        let access = Service::start(title).ok();
        let mut host = Self {
            window,
            control: Controller::new(renderer, size),
            access,
            hurry_until: Instant::now(),
        };
        host.draw()?;
        host.publish_access();
        Ok(host)
    }

    /// Whether the user asked for the window to close.
    #[must_use]
    pub const fn closing(&self) -> bool {
        self.control.closing()
    }

    /// Whether a screen reader is attached (the accessibility bus was there and answered).
    #[must_use]
    pub const fn has_accessibility(&self) -> bool {
        self.access.is_some()
    }

    /// Shows the accessibility service the page as it is now.
    fn publish_access(&self) {
        if let Some(access) = &self.access {
            access.publish(self.control.renderer.accessibility_tree());
        }
    }

    /// Carries out what assistive technology asked for.
    fn serve_access(&mut self) {
        let Some(access) = &self.access else { return };
        if access.take_asked() {
            self.hurry_until = Instant::now() + Duration::from_secs(3);
        }
        for (id, action) in access.take_actions() {
            self.control.renderer.perform(id, action);
        }
        for (id, text) in access.take_edits() {
            self.control.renderer.set_field_value(id, &text);
        }
    }

    /// Tells the input method which field has focus and where its caret is, when that changed.
    fn sync_ime(&mut self) -> io::Result<()> {
        let enabled = self.control.ime_enabled();
        match self.control.ime_change() {
            None => Ok(()),
            Some(None) => self.window.ime_disable(),
            Some(Some(state)) => {
                let rect = (
                    state.rect.0 as i32,
                    state.rect.1 as i32,
                    state.rect.2 as i32,
                    state.rect.3 as i32,
                );
                let (cursor, anchor) = (state.cursor as i32, state.anchor as i32);
                if enabled {
                    self.window.ime_update(&state.text, cursor, anchor, rect)
                } else {
                    self.window.ime_enable(&state.text, cursor, anchor, rect)
                }
            }
        }
    }

    /// Draws a frame if anything changed; says whether it did.
    ///
    /// # Errors
    /// The connection's.
    pub fn draw(&mut self) -> io::Result<bool> {
        if !self.control.renderer.take_dirty() {
            return Ok(false);
        }
        {
            let mut canvas = self.window.begin_frame()?;
            self.control.renderer.render(&mut canvas);
        }
        self.window.present()?;
        Ok(true)
    }

    /// Waits up to `timeout` for something to happen and does what it asks, then draws if the renderer changed.
    ///
    /// # Errors
    /// The connection's.
    pub fn step(&mut self, timeout: Duration) -> io::Result<()> {
        let wake = self.control.renderer.next_wake().unwrap_or(timeout);
        // A screen reader asked something lately: look often, so its answers are quick.
        let hurry = if Instant::now() < self.hurry_until {
            Duration::from_millis(25)
        } else {
            timeout
        };
        let wait = timeout.min(wake).min(hurry).min(
            self.control
                .next_timer()
                .saturating_duration_since(Instant::now()),
        );
        if let Some(event) = self.window.next_event(Some(wait))? {
            self.control.set_repeat(self.window.key_repeat());
            self.deliver(event)?;
            // Whatever else arrived with it.
            while let Some(event) = self.window.next_event(Some(Duration::from_millis(1)))? {
                self.deliver(event)?;
            }
        }
        self.control.timers(Instant::now());
        self.control.renderer.tick();
        self.serve_access();
        if self.draw()? {
            self.publish_access();
        }
        self.sync_ime()
    }
}

/// Opens a window and runs until it is closed.
///
/// # Errors
/// As [`Host::open`] and [`Host::step`].
pub fn run(
    title: &str,
    app_id: &str,
    size: (usize, usize),
    renderer: &Rc<NativeRenderer>,
) -> io::Result<()> {
    let mut host = Host::open(title, app_id, size, renderer)?;
    while !host.closing() {
        host.step(Duration::from_secs(1))?;
    }
    Ok(())
}

impl std::fmt::Debug for Host {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Host")
            .field("size", &self.control.size())
            .field("closing", &self.closing())
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Controller {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Controller")
            .field("size", &self.size)
            .field("closing", &self.closing)
            .finish_non_exhaustive()
    }
}
