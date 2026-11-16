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
    /// The person copied (ctrl+C, a menu) with focus inside the element. A listener gives the host what to put on the
    /// clipboard with [`set_clipboard`]; without that the browser copies the selection as usual.
    Copy,
    /// As `Copy`, and the content is being removed.
    Cut,
    /// The person pasted: the event carries what was on the clipboard. A listener that used it should call
    /// [`prevent_default`] so the browser does not also insert it.
    Paste,
    /// The wheel (or a trackpad's two-finger scroll or pinch, which arrives as a wheel with the
    /// ctrl key down). The web backend registers it as non-passive, so a listener may call
    /// [`prevent_default`] to stop the page from scrolling or zooming.
    Wheel,
    /// The secondary button (or a long press): the browser's own menu is replaced. The event carries where the pointer is.
    ContextMenu,
    /// The caret or the selection of a text field moved: the event carries where, in UTF-16 units (see [`utf16_to_byte`]).
    Selection,
    /// Something is about to change in an editable element (`contenteditable`): a character typed, a deletion, a paste, a
    /// formatting command. The browser's own change is cancelled (except while an input method is composing), so the app
    /// changes its model and draws the result.
    BeforeInput,
    /// The selection moved inside an editable element, as positions in its blocks.
    EditSelection,
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
            Self::Wheel => "wheel",
            Self::Copy => "copy",
            Self::Cut => "cut",
            Self::Paste => "paste",
            Self::ContextMenu => "contextmenu",
            Self::Selection => "selection",
            Self::BeforeInput => "beforeinput",
            Self::EditSelection => "editselection",
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

/// Where the selection of a text field is: UTF-16 offsets into its value, `start <= end`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SelectionEvent {
    pub start: usize,
    pub end: usize,
}

/// A place in an editable element: which block (its n-th child) and how many UTF-16 units of the block's text come before it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct BlockPoint {
    pub block: usize,
    pub offset: usize,
}

/// A change an editable element is about to make (see [`EventKind::BeforeInput`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BeforeInputEvent {
    /// The browser's name for it: `insertText`, `insertParagraph`, `deleteContentBackward`, `insertFromPaste`,
    /// `historyUndo`, `formatBold`...; and `compositionEnd` when an input method finished (see `data`).
    pub input_type: String,
    /// What is inserted: the typed text, the pasted plain text; for `compositionEnd` the whole text of the block the input
    /// method worked in (the browser has already changed it, so the app takes it as it is).
    pub data: String,
    /// What the change acts on: the browser's target range, which for a deletion is what is deleted.
    pub start: BlockPoint,
    pub end: BlockPoint,
}

/// The selection inside an editable element: where it started and where it ends (the caret).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EditSelectionEvent {
    pub anchor: BlockPoint,
    pub focus: BlockPoint,
}

/// The byte offset in `text` of a UTF-16 offset (the units the browser counts in); the end of the text when it is past it.
#[must_use]
pub fn utf16_to_byte(text: &str, units: usize) -> usize {
    let mut seen = 0;
    for (i, c) in text.char_indices() {
        if seen >= units {
            return i;
        }
        seen += c.len_utf16();
    }
    text.len()
}

/// The UTF-16 offset of a byte offset in `text` (a byte inside a character counts as its start).
#[must_use]
pub fn byte_to_utf16(text: &str, byte: usize) -> usize {
    text.char_indices().take_while(|(i, _)| *i < byte).map(|(_, c)| c.len_utf16()).sum()
}

/// One tick of the wheel over an element. Positions are in CSS pixels, relative to the element's
/// top-left corner, with the element's size so a listener can zoom about the pointer.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
#[allow(clippy::struct_excessive_bools)] // the four modifier keys
pub struct WheelEvent {
    pub x: f64,
    pub y: f64,
    /// The element's width and height.
    pub width: f64,
    pub height: f64,
    /// How far the wheel turned, in CSS pixels (lines and pages are converted by the browser
    /// layer): positive `dy` is away from the user (scroll down, zoom out).
    pub dx: f64,
    pub dy: f64,
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    pub meta: bool,
}

/// What was on the clipboard when the person pasted.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PasteEvent {
    /// The plain text (`text/plain`); empty when there was none.
    pub text: String,
    /// The HTML (`text/html`), as a spreadsheet or a web page puts it; empty when there was none.
    pub html: String,
}

/// What a `copy` or `cut` listener wants on the clipboard.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClipboardData {
    pub text: String,
    /// Optional rich form (`text/html`); empty for none.
    pub html: String,
}

thread_local! {
    static CLIPBOARD: std::cell::RefCell<Option<ClipboardData>> = const { std::cell::RefCell::new(None) };
}

/// Puts text (and optionally HTML) on the clipboard for the `copy` or `cut` event being handled.
///
/// It has an effect only while the host is still waiting on the listener, as with [`prevent_default`]; call it from
/// inside the listener.
pub fn set_clipboard(text: &str, html: &str) {
    CLIPBOARD.with(|c| {
        *c.borrow_mut() = Some(ClipboardData {
            text: text.to_string(),
            html: html.to_string(),
        });
    });
}

/// For hosts: what a listener asked to put on the clipboard, and forgets it. Call it once before delivering a `copy` or
/// `cut` event (to clear) and once after (to read).
#[must_use]
pub fn take_clipboard_request() -> Option<ClipboardData> {
    CLIPBOARD.with(|c| c.borrow_mut().take())
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
    Wheel(WheelEvent),
    Copy,
    Cut,
    Paste(PasteEvent),
    /// The secondary button was pressed at this point (viewport coordinates); the browser's menu does not open.
    ContextMenu(PointerEvent),
    /// The selection of an input or textarea, as UTF-16 offsets into its value (`start == end` is a caret).
    Selection(SelectionEvent),
    BeforeInput(BeforeInputEvent),
    EditSelection(EditSelectionEvent),
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
            Self::Wheel(_) => EventKind::Wheel,
            Self::Copy => EventKind::Copy,
            Self::Cut => EventKind::Cut,
            Self::Paste(_) => EventKind::Paste,
            Self::ContextMenu(_) => EventKind::ContextMenu,
            Self::Selection(_) => EventKind::Selection,
            Self::BeforeInput(_) => EventKind::BeforeInput,
            Self::EditSelection(_) => EventKind::EditSelection,
        }
    }
}

/// An event listener stored in the view tree.
pub type Handler = Rc<dyn Fn(&Event)>;
