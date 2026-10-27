//! Turns the numbers `quench.js` sends into a typed [`Event`].

use quench_ui::{Event, EventKind, KeyEvent, PointerEvent};

/// The listener kinds in the order of the contract's event table.
const KINDS: [EventKind; 12] = [
    EventKind::Click,
    EventKind::DoubleClick,
    EventKind::Input,
    EventKind::Change,
    EventKind::KeyDown,
    EventKind::KeyUp,
    EventKind::Submit,
    EventKind::Focus,
    EventKind::Blur,
    EventKind::PointerDown,
    EventKind::PointerUp,
    EventKind::PointerMove,
];

/// The number `quench.js` uses for `kind`.
#[must_use]
pub fn kind_code(kind: EventKind) -> u32 {
    KINDS
        .iter()
        .position(|k| *k == kind)
        .and_then(|i| u32::try_from(i).ok())
        .expect("every kind is in the table")
}

/// The event a listener of `kind` receives, or `None` for a number outside the table.
///
/// `payload` is the text (value or key), `(a, b, c)` a pointer's x, y and id, and `flags` the
/// modifier bits: 1 ctrl, 2 shift, 4 alt, 8 meta.
#[must_use]
pub fn decode(kind: u32, payload: String, a: f64, b: f64, c: i32, flags: u32) -> Option<Event> {
    let key = |payload: String| KeyEvent {
        key: payload,
        ctrl: flags & 1 != 0,
        shift: flags & 2 != 0,
        alt: flags & 4 != 0,
        meta: flags & 8 != 0,
    };
    let pointer = PointerEvent {
        x: a,
        y: b,
        pointer_id: c,
    };
    Some(match KINDS.get(usize::try_from(kind).ok()?)? {
        EventKind::Click => Event::Click,
        EventKind::DoubleClick => Event::DoubleClick,
        EventKind::Input => Event::Input(payload),
        EventKind::Change => Event::Change(payload),
        EventKind::KeyDown => Event::KeyDown(key(payload)),
        EventKind::KeyUp => Event::KeyUp(key(payload)),
        EventKind::Submit => Event::Submit,
        EventKind::Focus => Event::Focus,
        EventKind::Blur => Event::Blur,
        EventKind::PointerDown => Event::PointerDown(pointer),
        EventKind::PointerUp => Event::PointerUp(pointer),
        EventKind::PointerMove => Event::PointerMove(pointer),
    })
}
