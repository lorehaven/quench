//! A top-level window: shared-memory buffers to draw into, and the pointer and window events back.

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{self, Read as _};
use std::os::fd::{AsFd, OwnedFd};
use std::time::Duration;

use quench_raster::Canvas;

use crate::client::Client;
use crate::keymap::{Key, Keymap, Modifiers};
use crate::proto::{self, DISPLAY, Event, FORMAT_ARGB8888, Interface};
use crate::sys::{Mapping, memfd};
use crate::wire::Args;

/// The largest width or height a window will take: a compositor that suggests more is ignored.
const MAX_SIDE: usize = 16_384;

/// What a window tells the application.
#[derive(Debug, Clone, PartialEq)]
pub enum WindowEvent {
    /// The window has a new size (in pixels); draw a frame of that size. The first one is not sent: the
    /// size at [`Window::open`] is the first.
    Resized {
        width: usize,
        height: usize,
    },
    /// The user (or the desktop) asked for the window to close.
    CloseRequested,
    PointerEntered {
        x: f64,
        y: f64,
    },
    PointerLeft,
    PointerMoved {
        x: f64,
        y: f64,
    },
    /// `button` is the Linux input code (`0x110` left, `0x111` right, `0x112` middle).
    PointerButton {
        x: f64,
        y: f64,
        button: u32,
        pressed: bool,
    },
    /// Scrolling: `axis` 0 is vertical and 1 horizontal; `value` is in surface-local units, positive down/right.
    Scroll {
        axis: u32,
        value: f64,
    },
    /// A key went down or up. `key.modifier` keys change `modifiers` and are reported too (a host usually
    /// ignores them); `code` is the Linux input code.
    Key {
        code: u32,
        pressed: bool,
        key: Key,
        modifiers: Modifiers,
    },
    /// The input method (an IME, an on-screen keyboard) changed the text: apply it in this order.
    Ime(ImeEvent),
    /// The frame requested with [`Window::request_frame`] is the compositor's next one: draw now.
    Frame,
}

/// One change from the input method.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImeEvent {
    /// Characters to delete around the cursor first: before it, and after it, in bytes.
    pub delete_before: u32,
    pub delete_after: u32,
    /// Text to insert for good.
    pub commit: Option<String>,
    /// The composition now: its text and the cursor in it (byte offsets, negative for none). `Some("")` clears it.
    pub preedit: Option<(String, i32, i32)>,
}

struct Slot {
    id: u32,
    /// The compositor holds it (from `present` until it says it is released): do not write it.
    busy: bool,
}

/// Two frames of memory shared with the compositor.
struct Buffers {
    mapping: Mapping,
    pool: u32,
    slots: [Slot; 2],
    width: usize,
    height: usize,
    /// Whole bytes of one frame.
    frame: usize,
}

struct Ids {
    shm: u32,
    wm_base: u32,
    surface: u32,
    xdg_surface: u32,
    seat: Option<u32>,
    pointer: Option<u32>,
    keyboard: Option<u32>,
    text_manager: Option<u32>,
    text_input: Option<u32>,
}

/// A window on the compositor.
pub struct Window {
    client: Client,
    ids: Ids,
    buffers: Buffers,
    /// Which slot the next frame is drawn into.
    back: usize,
    /// The size the compositor suggested, applied when it says the configuration is complete.
    pending: Option<(usize, usize)>,
    events: VecDeque<WindowEvent>,
    pointer: (f64, f64),
    ime: ImeEvent,
    keymap: Keymap,
    modifiers: Modifiers,
    /// Key repeat: characters per second, and the delay before it starts in milliseconds.
    repeat: (i32, i32),
    frames: HashSet<u32>,
}

impl std::fmt::Debug for Window {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Window")
            .field("size", &(self.buffers.width, self.buffers.height))
            .finish_non_exhaustive()
    }
}

fn unsupported(what: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::Unsupported,
        format!("the compositor has no {what}"),
    )
}

impl Window {
    /// Opens a window on the compositor named by the environment.
    ///
    /// # Errors
    /// When there is no compositor, it lacks what a window needs, or it refuses.
    pub fn open(title: &str, app_id: &str, width: usize, height: usize) -> io::Result<Self> {
        Self::open_on(Client::connect()?, title, app_id, width, height)
    }

    /// Opens a window over a connection that is already made.
    ///
    /// # Errors
    /// As [`open`](Self::open).
    #[allow(clippy::too_many_lines)] // one handshake, in the order the protocol needs it
    pub fn open_on(
        mut client: Client,
        title: &str,
        app_id: &str,
        width: usize,
        height: usize,
    ) -> io::Result<Self> {
        let registry = client.new_id(Interface::Registry);
        client.send(
            DISPLAY,
            proto::display::GET_REGISTRY,
            Args::new().object(registry),
            &[],
        )?;
        let mut globals: HashMap<String, (u32, u32)> = HashMap::new();
        for (_, event) in client.roundtrip()? {
            if let Event::Global {
                name,
                interface,
                version,
            } = event
            {
                globals.insert(interface, (name, version));
            }
        }
        let bind = |client: &mut Client,
                    interface: &str,
                    wanted: u32,
                    kind: Interface|
         -> io::Result<Option<u32>> {
            let Some(&(name, version)) = globals.get(interface) else {
                return Ok(None);
            };
            let id = client.new_id(kind);
            let args = Args::new()
                .uint(name)
                .string(interface)
                .uint(version.min(wanted))
                .object(id);
            client.send(registry, proto::registry::BIND, args, &[])?;
            Ok(Some(id))
        };
        let compositor = bind(&mut client, "wl_compositor", 4, Interface::Compositor)?
            .ok_or_else(|| unsupported("wl_compositor"))?;
        let shm =
            bind(&mut client, "wl_shm", 1, Interface::Shm)?.ok_or_else(|| unsupported("wl_shm"))?;
        let wm_base = bind(&mut client, "xdg_wm_base", 1, Interface::XdgWmBase)?
            .ok_or_else(|| unsupported("xdg_wm_base"))?;
        let seat = bind(&mut client, "wl_seat", 5, Interface::Seat)?;
        let text_manager = bind(
            &mut client,
            "zwp_text_input_manager_v3",
            1,
            Interface::TextInputManager,
        )?;

        let surface = client.new_id(Interface::Surface);
        client.send(
            compositor,
            proto::compositor::CREATE_SURFACE,
            Args::new().object(surface),
            &[],
        )?;
        let xdg_surface = client.new_id(Interface::XdgSurface);
        client.send(
            wm_base,
            proto::xdg_wm_base::GET_XDG_SURFACE,
            Args::new().object(xdg_surface).object(surface),
            &[],
        )?;
        let toplevel = client.new_id(Interface::XdgToplevel);
        client.send(
            xdg_surface,
            proto::xdg_surface::GET_TOPLEVEL,
            Args::new().object(toplevel),
            &[],
        )?;
        client.send(
            toplevel,
            proto::xdg_toplevel::SET_TITLE,
            Args::new().string(title),
            &[],
        )?;
        client.send(
            toplevel,
            proto::xdg_toplevel::SET_APP_ID,
            Args::new().string(app_id),
            &[],
        )?;
        // The first commit has no buffer: it asks the compositor for the window's configuration.
        client.send(surface, proto::surface::COMMIT, Args::new(), &[])?;

        // Wait for the first complete configuration, answering what must be answered.
        let mut size = (width, height);
        let serial = loop {
            match client.next_event(Some(Duration::from_secs(5)))? {
                Some((
                    _,
                    Event::ToplevelConfigure {
                        width: w,
                        height: h,
                        ..
                    },
                )) => {
                    if let Some(s) = checked_size(w, h) {
                        size = s;
                    }
                }
                Some((_, Event::XdgConfigure { serial })) => break serial,
                Some((_, Event::XdgPing { serial })) => {
                    client.send(
                        wm_base,
                        proto::xdg_wm_base::PONG,
                        Args::new().uint(serial),
                        &[],
                    )?;
                }
                Some(_) => {}
                None => {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "the compositor never configured the window",
                    ));
                }
            }
        };
        client.send(
            xdg_surface,
            proto::xdg_surface::ACK_CONFIGURE,
            Args::new().uint(serial),
            &[],
        )?;
        let buffers = Buffers::create(&mut client, shm, size.0, size.1)?;
        Ok(Self {
            client,
            ids: Ids {
                shm,
                wm_base,
                surface,
                xdg_surface,
                seat,
                pointer: None,
                keyboard: None,
                text_manager,
                text_input: None,
            },
            buffers,
            back: 0,
            pending: None,
            events: VecDeque::new(),
            ime: ImeEvent::default(),
            keymap: Keymap::us(),
            modifiers: Modifiers::default(),
            repeat: (25, 600),
            pointer: (0.0, 0.0),
            frames: HashSet::new(),
        })
    }

    /// The size of a frame, in pixels.
    #[must_use]
    pub const fn size(&self) -> (usize, usize) {
        (self.buffers.width, self.buffers.height)
    }

    /// The next thing that happened to the window, waiting up to `timeout` (`None`: for ever). `Ok(None)`
    /// when the time ran out.
    ///
    /// # Errors
    /// The connection's: the compositor went away, or reported a protocol error.
    pub fn next_event(&mut self, timeout: Option<Duration>) -> io::Result<Option<WindowEvent>> {
        loop {
            if let Some(event) = self.events.pop_front() {
                return Ok(Some(event));
            }
            let Some((id, event)) = self.client.next_event(timeout)? else {
                return Ok(None);
            };
            self.handle(id, event)?;
        }
    }

    /// The frame to draw into: wait for the compositor to be done with it if it is not, and hand out its
    /// bytes. The previous contents are whatever was drawn two frames ago; clear what you do not cover.
    ///
    /// # Errors
    /// The connection's, or a compositor that never releases a buffer (after 5 s).
    /// Key repeat as the compositor configured it: characters per second and the delay before it starts, in
    /// milliseconds. The compositor does not repeat keys itself: the program does, while a key is held.
    #[must_use]
    pub const fn key_repeat(&self) -> (i32, i32) {
        self.repeat
    }

    /// Whether the compositor offers input methods (text-input-v3).
    #[must_use]
    pub const fn has_ime(&self) -> bool {
        self.ids.text_input.is_some()
    }

    /// Tells the input method that a text field has focus: the text around the caret (at most a few hundred
    /// bytes either side is plenty), where the caret and the other end of the selection are in it (bytes), and
    /// the caret's rectangle in the window (for placing the candidate list). Without an input method this does
    /// nothing.
    ///
    /// # Errors
    /// The connection's.
    pub fn ime_enable(
        &mut self,
        surrounding: &str,
        cursor: i32,
        anchor: i32,
        rect: (i32, i32, i32, i32),
    ) -> io::Result<()> {
        let Some(input) = self.ids.text_input else {
            return Ok(());
        };
        self.client
            .send(input, proto::text_input::ENABLE, Args::new(), &[])?;
        self.ime_state(input, surrounding, cursor, anchor, rect)
    }

    /// Reports a change of the text, the caret or its place to an input method that is enabled.
    ///
    /// # Errors
    /// The connection's.
    pub fn ime_update(
        &mut self,
        surrounding: &str,
        cursor: i32,
        anchor: i32,
        rect: (i32, i32, i32, i32),
    ) -> io::Result<()> {
        let Some(input) = self.ids.text_input else {
            return Ok(());
        };
        self.ime_state(input, surrounding, cursor, anchor, rect)
    }

    fn ime_state(
        &self,
        input: u32,
        surrounding: &str,
        cursor: i32,
        anchor: i32,
        rect: (i32, i32, i32, i32),
    ) -> io::Result<()> {
        // The protocol allows 4000 bytes of surrounding text; keep to a window of it around the cursor.
        let (text, shift) = surrounding_window(surrounding, cursor.max(0) as usize);
        let at = |v: i32| (v - shift as i32).max(0);
        self.client.send(
            input,
            proto::text_input::SET_SURROUNDING_TEXT,
            Args::new().string(text).int(at(cursor)).int(at(anchor)),
            &[],
        )?;
        // Normal text: no hint, purpose "normal".
        self.client.send(
            input,
            proto::text_input::SET_CONTENT_TYPE,
            Args::new().uint(0).uint(0),
            &[],
        )?;
        self.client.send(
            input,
            proto::text_input::SET_CURSOR_RECTANGLE,
            Args::new().int(rect.0).int(rect.1).int(rect.2).int(rect.3),
            &[],
        )?;
        self.client
            .send(input, proto::text_input::COMMIT, Args::new(), &[])
    }

    /// Tells the input method that no text field has focus.
    ///
    /// # Errors
    /// The connection's.
    pub fn ime_disable(&mut self) -> io::Result<()> {
        let Some(input) = self.ids.text_input else {
            return Ok(());
        };
        self.client
            .send(input, proto::text_input::DISABLE, Args::new(), &[])?;
        self.client
            .send(input, proto::text_input::COMMIT, Args::new(), &[])
    }

    pub fn begin_frame(&mut self) -> io::Result<Canvas<&mut [u8]>> {
        while self.buffers.slots[self.back].busy {
            match self.client.next_event(Some(Duration::from_secs(5)))? {
                Some((id, event)) => self.handle(id, event)?,
                None => {
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "the compositor did not release a buffer",
                    ));
                }
            }
        }
        let (width, height, frame) = (self.buffers.width, self.buffers.height, self.buffers.frame);
        let start = self.back * frame;
        let bytes = &mut self.buffers.mapping.as_mut_slice()[start..start + frame];
        Ok(Canvas::over(width, height, bytes).expect("a frame is width * height * 4 bytes"))
    }

    /// Shows the frame that was drawn after [`begin_frame`](Self::begin_frame).
    ///
    /// # Errors
    /// The connection's.
    pub fn present(&mut self) -> io::Result<()> {
        let slot = &mut self.buffers.slots[self.back];
        let (width, height) = (self.buffers.width as i32, self.buffers.height as i32);
        let surface = self.ids.surface;
        self.client.send(
            surface,
            proto::surface::ATTACH,
            Args::new().object(slot.id).int(0).int(0),
            &[],
        )?;
        self.client.send(
            surface,
            proto::surface::DAMAGE_BUFFER,
            Args::new().int(0).int(0).int(width).int(height),
            &[],
        )?;
        self.client
            .send(surface, proto::surface::COMMIT, Args::new(), &[])?;
        slot.busy = true;
        self.back = 1 - self.back;
        Ok(())
    }

    /// Asks to be told when the compositor wants the next frame ([`WindowEvent::Frame`]). Call it before
    /// [`present`](Self::present).
    ///
    /// # Errors
    /// The connection's.
    pub fn request_frame(&mut self) -> io::Result<()> {
        let callback = self.client.new_id(Interface::Callback);
        self.client.send(
            self.ids.surface,
            proto::surface::FRAME,
            Args::new().object(callback),
            &[],
        )?;
        self.frames.insert(callback);
        Ok(())
    }

    // The event is taken apart by value: a reference would make every field a reference to dereference.
    #[allow(clippy::needless_pass_by_value)]
    #[allow(
        clippy::too_many_lines,
        reason = "one arm per event the window reacts to"
    )]
    fn handle(&mut self, id: u32, event: Event) -> io::Result<()> {
        match event {
            Event::XdgPing { serial } => {
                self.client.send(
                    self.ids.wm_base,
                    proto::xdg_wm_base::PONG,
                    Args::new().uint(serial),
                    &[],
                )?;
            }
            Event::ToplevelConfigure { width, height, .. } => {
                if let Some(size) = checked_size(width, height) {
                    self.pending = Some(size);
                }
            }
            Event::XdgConfigure { serial } => {
                self.client.send(
                    self.ids.xdg_surface,
                    proto::xdg_surface::ACK_CONFIGURE,
                    Args::new().uint(serial),
                    &[],
                )?;
                if let Some(size) = self.pending.take()
                    && size != (self.buffers.width, self.buffers.height)
                {
                    self.resize(size)?;
                }
            }
            Event::ToplevelClose => self.events.push_back(WindowEvent::CloseRequested),
            Event::BufferRelease => {
                if let Some(slot) = self.buffers.slots.iter_mut().find(|s| s.id == id) {
                    slot.busy = false;
                }
            }
            Event::SeatCapabilities(capabilities) => {
                if capabilities & proto::seat::POINTER != 0
                    && self.ids.pointer.is_none()
                    && let Some(seat) = self.ids.seat
                {
                    {
                        let pointer = self.client.new_id(Interface::Pointer);
                        self.client.send(
                            seat,
                            proto::seat::GET_POINTER,
                            Args::new().object(pointer),
                            &[],
                        )?;
                        self.ids.pointer = Some(pointer);
                    }
                }
                if capabilities & proto::seat::KEYBOARD != 0
                    && self.ids.keyboard.is_none()
                    && let Some(seat) = self.ids.seat
                {
                    let keyboard = self.client.new_id(Interface::Keyboard);
                    self.client.send(
                        seat,
                        proto::seat::GET_KEYBOARD,
                        Args::new().object(keyboard),
                        &[],
                    )?;
                    self.ids.keyboard = Some(keyboard);
                }
                if let (Some(manager), Some(seat), None) =
                    (self.ids.text_manager, self.ids.seat, self.ids.text_input)
                {
                    let input = self.client.new_id(Interface::TextInput);
                    self.client.send(
                        manager,
                        proto::text_input_manager::GET_TEXT_INPUT,
                        Args::new().object(input).object(seat),
                        &[],
                    )?;
                    self.ids.text_input = Some(input);
                }
            }
            Event::PointerEnter { x, y, .. } => {
                self.pointer = (x, y);
                self.events.push_back(WindowEvent::PointerEntered { x, y });
            }
            Event::PointerLeave => self.events.push_back(WindowEvent::PointerLeft),
            Event::PointerMotion { x, y } => {
                self.pointer = (x, y);
                self.events.push_back(WindowEvent::PointerMoved { x, y });
            }
            Event::PointerButton {
                button, pressed, ..
            } => {
                let (x, y) = self.pointer;
                self.events.push_back(WindowEvent::PointerButton {
                    x,
                    y,
                    button,
                    pressed,
                });
            }
            Event::KeyboardKeymap { size } => {
                if let Some(fd) = self.client.take_fd() {
                    let mut text = String::new();
                    let read = std::io::Read::take(std::fs::File::from(fd), u64::from(size))
                        .read_to_string(&mut text);
                    if read.is_ok()
                        && let Some(map) = Keymap::parse(&text)
                    {
                        self.keymap = map;
                    }
                }
            }
            Event::TextInputPreedit {
                text,
                cursor_begin,
                cursor_end,
            } => self.ime.preedit = Some((text, cursor_begin, cursor_end)),
            Event::TextInputCommit(text) => self.ime.commit = Some(text),
            Event::TextInputDeleteSurrounding { before, after } => {
                self.ime.delete_before = before;
                self.ime.delete_after = after;
            }
            Event::TextInputDone => {
                let change = std::mem::take(&mut self.ime);
                self.events.push_back(WindowEvent::Ime(change));
            }
            Event::KeyboardRepeatInfo { rate, delay } => self.repeat = (rate, delay),
            Event::KeyboardModifiers {
                depressed,
                latched,
                locked,
            } => self.modifiers = Modifiers::from_masks(depressed, latched, locked),
            Event::KeyboardKey { key: code, pressed } => {
                if let Some(key) = self.keymap.key(code, self.modifiers) {
                    self.events.push_back(WindowEvent::Key {
                        code,
                        pressed,
                        key,
                        modifiers: self.modifiers,
                    });
                }
            }
            Event::PointerAxis { axis, value } => {
                self.events.push_back(WindowEvent::Scroll { axis, value });
            }
            Event::CallbackDone if self.frames.remove(&id) => {
                self.events.push_back(WindowEvent::Frame);
            }
            _ => {}
        }
        Ok(())
    }

    fn resize(&mut self, (width, height): (usize, usize)) -> io::Result<()> {
        let fresh = Buffers::create(&mut self.client, self.ids.shm, width, height)?;
        let old = std::mem::replace(&mut self.buffers, fresh);
        for slot in &old.slots {
            self.client
                .send(slot.id, proto::buffer::DESTROY, Args::new(), &[])?;
        }
        self.client
            .send(old.pool, proto::shm_pool::DESTROY, Args::new(), &[])?;
        self.back = 0;
        self.events
            .push_back(WindowEvent::Resized { width, height });
        Ok(())
    }
}

/// A size the compositor suggested, if it is one we will take (0 means "your choice").
fn checked_size(width: i32, height: i32) -> Option<(usize, usize)> {
    let (w, h) = (usize::try_from(width).ok()?, usize::try_from(height).ok()?);
    ((1..=MAX_SIDE).contains(&w) && (1..=MAX_SIDE).contains(&h)).then_some((w, h))
}

impl Buffers {
    /// Memory for two frames of `width` x `height`, shared with the compositor as two `wl_buffer`s of one pool.
    fn create(client: &mut Client, shm: u32, width: usize, height: usize) -> io::Result<Self> {
        let stride = width * 4;
        let frame = stride * height;
        let size = frame * 2;
        let fd: OwnedFd = memfd(size)?;
        let mapping = Mapping::new(fd.as_fd(), size, true)?;
        let pool = client.new_id(Interface::ShmPool);
        client.send(
            shm,
            proto::shm::CREATE_POOL,
            Args::new().object(pool).int(size as i32),
            &[fd.as_fd()],
        )?;
        let mut slots = [Slot { id: 0, busy: false }, Slot { id: 0, busy: false }];
        for (i, slot) in slots.iter_mut().enumerate() {
            slot.id = client.new_id(Interface::Buffer);
            let args = Args::new()
                .object(slot.id)
                .int((i * frame) as i32)
                .int(width as i32)
                .int(height as i32)
                .int(stride as i32)
                .uint(FORMAT_ARGB8888);
            client.send(pool, proto::shm_pool::CREATE_BUFFER, args, &[])?;
        }
        // The descriptor can go: the compositor has its own, and the mapping stays.
        drop(fd);
        // A new memfd is zero-filled by the kernel, and touching it here would commit all its pages at once.
        Ok(Self {
            mapping,
            pool,
            slots,
            width,
            height,
            frame,
        })
    }
}

/// At most 2000 bytes of `text` either side of byte `cursor`, cut at character boundaries, and how many bytes
/// were dropped before it.
fn surrounding_window(text: &str, cursor: usize) -> (&str, usize) {
    let mut start = cursor.saturating_sub(2000).min(text.len());
    while !text.is_char_boundary(start) {
        start += 1;
    }
    let mut end = (cursor + 2000).min(text.len());
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (&text[start..end], start)
}
