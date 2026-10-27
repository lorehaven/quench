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
    /// The browser took the pointer over (to scroll, say): the gesture is off.
    PointerCancel,
    /// An input method (an IME, a handwriting pad) started composing text.
    CompositionStart,
    CompositionEnd,
    Scroll,
    /// A CSS transition finished.
    TransitionEnd,
    /// The element's size changed (also reported once when it is first laid out).
    Resize,
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
            Self::PointerCancel => "pointercancel",
            Self::CompositionStart => "compositionstart",
            Self::CompositionEnd => "compositionend",
            Self::Scroll => "scroll",
            Self::TransitionEnd => "transitionend",
            Self::Resize => "resize",
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

/// How far an element has scrolled, in CSS pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ScrollEvent {
    pub top: f64,
    pub left: f64,
}

/// An element's size in CSS pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

thread_local! {
    static PREVENT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Asks the host to stop the browser's default action for the event being handled.
///
/// Call it from inside a listener, for example to keep Home and End from scrolling the page when
/// they move a widget's selection. It has an effect only while the host is still waiting on the
/// listener, which is the case for events from the user, not for ones replayed after an import.
pub fn prevent_default() {
    PREVENT.with(|p| p.set(true));
}

/// For hosts: whether a listener asked for the default action to be stopped, and forgets the
/// request. Call it once before delivering an event (to clear) and once after (to read).
#[must_use]
pub fn take_prevent_request() -> bool {
    PREVENT.with(std::cell::Cell::take)
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
    PointerCancel(PointerEvent),
    /// The control's value when composing started, and when the composed text was committed.
    CompositionStart(String),
    CompositionEnd(String),
    Scroll(ScrollEvent),
    TransitionEnd,
    Resize(Size),
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
            Self::PointerCancel(_) => EventKind::PointerCancel,
            Self::CompositionStart(_) => EventKind::CompositionStart,
            Self::CompositionEnd(_) => EventKind::CompositionEnd,
            Self::Scroll(_) => EventKind::Scroll,
            Self::TransitionEnd => EventKind::TransitionEnd,
            Self::Resize(_) => EventKind::Resize,
        }
    }
}

/// An event listener stored in the view tree.
pub type Handler = Rc<dyn Fn(&Event)>;
