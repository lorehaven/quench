//! Typed UI events, independent of the platform that produced them.

use std::rc::Rc;

/// Which event a listener is for.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum EventKind {
    Click,
    DoubleClick,
    Input,
    Change,
    KeyDown,
    KeyUp,
    Submit,
    Focus,
    Blur,
    PointerDown,
    PointerUp,
    PointerMove,
}

impl EventKind {
    /// The DOM event name a web backend listens for.
    #[must_use]
    pub const fn dom_name(self) -> &'static str {
        match self {
            Self::Click => "click",
            Self::DoubleClick => "dblclick",
            Self::Input => "input",
            Self::Change => "change",
            Self::KeyDown => "keydown",
            Self::KeyUp => "keyup",
            Self::Submit => "submit",
            Self::Focus => "focus",
            Self::Blur => "blur",
            Self::PointerDown => "pointerdown",
            Self::PointerUp => "pointerup",
            Self::PointerMove => "pointermove",
        }
    }
}

/// A key press.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)] // the four modifier keys
pub struct KeyEvent {
    /// The DOM `key` value, such as `"a"`, `"Enter"` or `"ArrowDown"`.
    pub key: String,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
}

impl KeyEvent {
    #[must_use]
    pub fn new(key: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            ..Self::default()
        }
    }
}

/// A pointer (mouse, touch or pen) position in CSS pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PointerEvent {
    pub x: f64,
    pub y: f64,
    pub pointer_id: i32,
}

/// What a listener receives.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Click,
    DoubleClick,
    /// The current value of the control after the edit.
    Input(String),
    Change(String),
    KeyDown(KeyEvent),
    KeyUp(KeyEvent),
    Submit,
    Focus,
    Blur,
    PointerDown(PointerEvent),
    PointerUp(PointerEvent),
    PointerMove(PointerEvent),
}

impl Event {
    /// The kind of listener this event is delivered to.
    #[must_use]
    pub const fn kind(&self) -> EventKind {
        match self {
            Self::Click => EventKind::Click,
            Self::DoubleClick => EventKind::DoubleClick,
            Self::Input(_) => EventKind::Input,
            Self::Change(_) => EventKind::Change,
            Self::KeyDown(_) => EventKind::KeyDown,
            Self::KeyUp(_) => EventKind::KeyUp,
            Self::Submit => EventKind::Submit,
            Self::Focus => EventKind::Focus,
            Self::Blur => EventKind::Blur,
            Self::PointerDown(_) => EventKind::PointerDown,
            Self::PointerUp(_) => EventKind::PointerUp,
            Self::PointerMove(_) => EventKind::PointerMove,
        }
    }
}

/// An event listener stored in the view tree.
pub type Handler = Rc<dyn Fn(&Event)>;
