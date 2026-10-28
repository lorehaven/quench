//! The parts of the core and `xdg-shell` protocols this client speaks: opcodes, and the decoding of events.

use crate::wire::{Message, Reader, WireError};

/// The object every connection starts with.
pub const DISPLAY: u32 = 1;

/// `wl_shm` pixel format `ARGB8888` (premultiplied alpha, little-endian `0xAARRGGBB`).
pub const FORMAT_ARGB8888: u32 = 0;

pub mod display {
    pub const SYNC: u16 = 0;
    pub const GET_REGISTRY: u16 = 1;
    pub const ERROR: u16 = 0;
    pub const DELETE_ID: u16 = 1;
}

pub mod registry {
    pub const BIND: u16 = 0;
    pub const GLOBAL: u16 = 0;
    pub const GLOBAL_REMOVE: u16 = 1;
}

pub mod callback {
    pub const DONE: u16 = 0;
}

pub mod compositor {
    pub const CREATE_SURFACE: u16 = 0;
}

pub mod shm {
    pub const CREATE_POOL: u16 = 0;
    pub const FORMAT: u16 = 0;
}

pub mod shm_pool {
    pub const CREATE_BUFFER: u16 = 0;
    pub const DESTROY: u16 = 1;
}

pub mod buffer {
    pub const DESTROY: u16 = 0;
    pub const RELEASE: u16 = 0;
}

pub mod surface {
    pub const DESTROY: u16 = 0;
    pub const ATTACH: u16 = 1;
    pub const DAMAGE: u16 = 2;
    pub const FRAME: u16 = 3;
    pub const COMMIT: u16 = 6;
    pub const DAMAGE_BUFFER: u16 = 9;
}

pub mod xdg_wm_base {
    pub const GET_XDG_SURFACE: u16 = 2;
    pub const PONG: u16 = 3;
    pub const PING: u16 = 0;
}

pub mod xdg_surface {
    pub const DESTROY: u16 = 0;
    pub const GET_TOPLEVEL: u16 = 1;
    pub const ACK_CONFIGURE: u16 = 4;
    pub const CONFIGURE: u16 = 0;
}

pub mod xdg_toplevel {
    pub const DESTROY: u16 = 0;
    pub const SET_TITLE: u16 = 2;
    pub const SET_APP_ID: u16 = 3;
    pub const SET_MIN_SIZE: u16 = 8;
    pub const CONFIGURE: u16 = 0;
    pub const CLOSE: u16 = 1;
}

pub mod seat {
    pub const GET_POINTER: u16 = 0;
    pub const GET_KEYBOARD: u16 = 1;
    pub const CAPABILITIES: u16 = 0;
    /// The bit of `capabilities` for a pointer.
    pub const POINTER: u32 = 1;
    /// The bit of `capabilities` for a keyboard.
    pub const KEYBOARD: u32 = 2;
}

pub mod text_input_manager {
    pub const GET_TEXT_INPUT: u16 = 1;
}

pub mod text_input {
    pub const ENABLE: u16 = 1;
    pub const DISABLE: u16 = 2;
    pub const SET_SURROUNDING_TEXT: u16 = 3;
    pub const SET_CONTENT_TYPE: u16 = 5;
    pub const SET_CURSOR_RECTANGLE: u16 = 6;
    pub const COMMIT: u16 = 7;
    pub const ENTER: u16 = 0;
    pub const LEAVE: u16 = 1;
    pub const PREEDIT_STRING: u16 = 2;
    pub const COMMIT_STRING: u16 = 3;
    pub const DELETE_SURROUNDING_TEXT: u16 = 4;
    pub const DONE: u16 = 5;
}

pub mod data_device_manager {
    pub const CREATE_DATA_SOURCE: u16 = 0;
    pub const GET_DATA_DEVICE: u16 = 1;
}

pub mod data_source {
    pub const OFFER: u16 = 0;
    pub const DESTROY: u16 = 1;
    pub const SEND: u16 = 1;
    pub const CANCELLED: u16 = 2;
}

pub mod data_device {
    pub const SET_SELECTION: u16 = 1;
    pub const DATA_OFFER: u16 = 0;
    pub const SELECTION: u16 = 5;
}

pub mod data_offer {
    pub const RECEIVE: u16 = 1;
    pub const DESTROY: u16 = 2;
    pub const OFFER: u16 = 0;
}

pub mod keyboard {
    pub const KEYMAP: u16 = 0;
    pub const ENTER: u16 = 1;
    pub const LEAVE: u16 = 2;
    pub const KEY: u16 = 3;
    pub const MODIFIERS: u16 = 4;
    pub const REPEAT_INFO: u16 = 5;
}

pub mod pointer {
    pub const ENTER: u16 = 0;
    pub const LEAVE: u16 = 1;
    pub const MOTION: u16 = 2;
    pub const BUTTON: u16 = 3;
    pub const AXIS: u16 = 4;
    pub const FRAME: u16 = 5;
}

/// What kind of object an id is, which is what its events' opcodes mean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interface {
    Display,
    Registry,
    Callback,
    Compositor,
    Shm,
    ShmPool,
    Buffer,
    Surface,
    XdgWmBase,
    XdgSurface,
    XdgToplevel,
    Seat,
    Pointer,
    Keyboard,
    TextInputManager,
    TextInput,
    DataDeviceManager,
    DataDevice,
    DataSource,
    DataOffer,
}

/// What the compositor tells us.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// A protocol error: the connection is finished.
    Error {
        object: u32,
        code: u32,
        message: String,
    },
    /// An id can be used again.
    DeleteId(u32),
    Global {
        name: u32,
        interface: String,
        version: u32,
    },
    GlobalRemove(u32),
    /// A `sync` or `frame` callback fired.
    CallbackDone,
    ShmFormat(u32),
    BufferRelease,
    XdgPing {
        serial: u32,
    },
    XdgConfigure {
        serial: u32,
    },
    /// The size the compositor suggests (0 means "your choice") and the state flags.
    ToplevelConfigure {
        width: i32,
        height: i32,
        states: Vec<u32>,
    },
    ToplevelClose,
    SeatCapabilities(u32),
    PointerEnter {
        serial: u32,
        x: f64,
        y: f64,
    },
    PointerLeave,
    PointerMotion {
        x: f64,
        y: f64,
    },
    PointerButton {
        serial: u32,
        button: u32,
        pressed: bool,
    },
    PointerAxis {
        axis: u32,
        value: f64,
    },
    PointerFrame,
    /// The layout: its text arrives in a file descriptor, `size` bytes of it.
    KeyboardKeymap {
        size: u32,
    },
    KeyboardEnter {
        serial: u32,
    },
    KeyboardLeave,
    TextInputEnter,
    TextInputLeave,
    /// The input method's composition in progress: the text (empty when it is cleared) and where the cursor
    /// is in it, as byte offsets (negative: no cursor).
    TextInputPreedit {
        text: String,
        cursor_begin: i32,
        cursor_end: i32,
    },
    TextInputCommit(String),
    TextInputDeleteSurrounding {
        before: u32,
        after: u32,
    },
    /// The events since the last `done` are one change: apply them now.
    TextInputDone,
    KeyboardKey {
        serial: u32,
        /// The Linux input code.
        key: u32,
        pressed: bool,
    },
    /// A new offer of data (the id the compositor gave it): its types follow as `OfferType` events.
    DataOffer(u32),
    OfferType(String),
    /// The clipboard changed: the offer that holds it (`None` when it is empty).
    Selection(Option<u32>),
    /// Somebody wants what this program put on the clipboard: write it to the descriptor.
    SourceSend {
        mime: String,
    },
    SourceCancelled,
    KeyboardModifiers {
        depressed: u32,
        latched: u32,
        locked: u32,
    },
    /// Characters per second, and the delay before the first repeat in milliseconds.
    KeyboardRepeatInfo {
        rate: i32,
        delay: i32,
    },
    /// An event this client does not use.
    Ignored,
}

/// Reads one event of an object of `interface`.
///
/// # Errors
/// When its arguments are not what the protocol says.
#[allow(clippy::too_many_lines, reason = "one arm per event of the protocol")]
pub fn decode(interface: Interface, message: &Message) -> Result<Event, WireError> {
    let mut r = Reader::new(&message.args);
    let event = match (interface, message.opcode) {
        (Interface::Display, display::ERROR) => Event::Error {
            object: r.object()?,
            code: r.uint()?,
            message: r.string()?,
        },
        (Interface::Display, display::DELETE_ID) => Event::DeleteId(r.uint()?),
        (Interface::Registry, registry::GLOBAL) => Event::Global {
            name: r.uint()?,
            interface: r.string()?,
            version: r.uint()?,
        },
        (Interface::Registry, registry::GLOBAL_REMOVE) => Event::GlobalRemove(r.uint()?),
        (Interface::Callback, callback::DONE) => Event::CallbackDone,
        (Interface::Shm, shm::FORMAT) => Event::ShmFormat(r.uint()?),
        (Interface::Buffer, buffer::RELEASE) => Event::BufferRelease,
        (Interface::XdgWmBase, xdg_wm_base::PING) => Event::XdgPing { serial: r.uint()? },
        (Interface::XdgSurface, xdg_surface::CONFIGURE) => {
            Event::XdgConfigure { serial: r.uint()? }
        }
        (Interface::XdgToplevel, xdg_toplevel::CONFIGURE) => {
            let (width, height) = (r.int()?, r.int()?);
            let states = r
                .array()?
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| u32::from_ne_bytes(*c))
                .collect();
            Event::ToplevelConfigure {
                width,
                height,
                states,
            }
        }
        (Interface::XdgToplevel, xdg_toplevel::CLOSE) => Event::ToplevelClose,
        (Interface::Seat, seat::CAPABILITIES) => Event::SeatCapabilities(r.uint()?),
        (Interface::Pointer, pointer::ENTER) => {
            let serial = r.uint()?;
            let _surface = r.object()?;
            Event::PointerEnter {
                serial,
                x: r.fixed()?,
                y: r.fixed()?,
            }
        }
        (Interface::Pointer, pointer::LEAVE) => Event::PointerLeave,
        (Interface::Pointer, pointer::MOTION) => {
            let _time = r.uint()?;
            Event::PointerMotion {
                x: r.fixed()?,
                y: r.fixed()?,
            }
        }
        (Interface::Pointer, pointer::BUTTON) => {
            let serial = r.uint()?;
            let _time = r.uint()?;
            Event::PointerButton {
                serial,
                button: r.uint()?,
                pressed: r.uint()? == 1,
            }
        }
        (Interface::Pointer, pointer::AXIS) => {
            let _time = r.uint()?;
            Event::PointerAxis {
                axis: r.uint()?,
                value: r.fixed()?,
            }
        }
        (Interface::Pointer, pointer::FRAME) => Event::PointerFrame,
        (Interface::Keyboard, keyboard::KEYMAP) => {
            let _format = r.uint()?;
            Event::KeyboardKeymap { size: r.uint()? }
        }
        (Interface::TextInput, text_input::ENTER) => Event::TextInputEnter,
        (Interface::TextInput, text_input::LEAVE) => Event::TextInputLeave,
        (Interface::TextInput, text_input::PREEDIT_STRING) => Event::TextInputPreedit {
            text: r.string()?,
            cursor_begin: r.int()?,
            cursor_end: r.int()?,
        },
        (Interface::TextInput, text_input::COMMIT_STRING) => Event::TextInputCommit(r.string()?),
        (Interface::TextInput, text_input::DELETE_SURROUNDING_TEXT) => {
            Event::TextInputDeleteSurrounding {
                before: r.uint()?,
                after: r.uint()?,
            }
        }
        (Interface::TextInput, text_input::DONE) => Event::TextInputDone,
        (Interface::Keyboard, keyboard::ENTER) => Event::KeyboardEnter { serial: r.uint()? },
        (Interface::DataDevice, data_device::DATA_OFFER) => Event::DataOffer(r.object()?),
        (Interface::DataDevice, data_device::SELECTION) => {
            Event::Selection(Some(r.object()?).filter(|id| *id != 0))
        }
        (Interface::DataOffer, data_offer::OFFER) => Event::OfferType(r.string()?),
        (Interface::DataSource, data_source::SEND) => Event::SourceSend { mime: r.string()? },
        (Interface::DataSource, data_source::CANCELLED) => Event::SourceCancelled,
        (Interface::Keyboard, keyboard::LEAVE) => Event::KeyboardLeave,
        (Interface::Keyboard, keyboard::KEY) => {
            let serial = r.uint()?;
            let _time = r.uint()?;
            Event::KeyboardKey {
                serial,
                key: r.uint()?,
                pressed: r.uint()? == 1,
            }
        }
        (Interface::Keyboard, keyboard::MODIFIERS) => {
            let _serial = r.uint()?;
            Event::KeyboardModifiers {
                depressed: r.uint()?,
                latched: r.uint()?,
                locked: r.uint()?,
            }
        }
        (Interface::Keyboard, keyboard::REPEAT_INFO) => Event::KeyboardRepeatInfo {
            rate: r.int()?,
            delay: r.int()?,
        },
        _ => Event::Ignored,
    };
    Ok(event)
}
