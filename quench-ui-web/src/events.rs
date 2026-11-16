//! Turns the numbers `quench.js` sends into a typed [`Event`].

use quench_serde::{Value, json};
use quench_ui::{
    BeforeInputEvent, BlockPoint, EditSelectionEvent, Event, EventKind, KeyEvent, PasteEvent,
    PointerEvent, ScrollEvent, SelectionEvent, Size, WheelEvent,
};

/// The listener kinds in the order of the contract's event table.
const KINDS: [EventKind; 26] = [
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
    EventKind::CompositionStart,
    EventKind::CompositionEnd,
    EventKind::Scroll,
    EventKind::TransitionEnd,
    EventKind::Resize,
    EventKind::PointerCancel,
    EventKind::Wheel,
    EventKind::Copy,
    EventKind::Cut,
    EventKind::Paste,
    EventKind::ContextMenu,
    EventKind::Selection,
    EventKind::BeforeInput,
    EventKind::EditSelection,
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
/// modifier bits: 1 ctrl, 2 shift, 4 alt, 8 meta. A wheel's `payload` is `x,y,width,height,dx,dy`
/// (CSS pixels, `x` and `y` relative to the element).
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
        EventKind::PointerCancel => Event::PointerCancel(pointer),
        EventKind::CompositionStart => Event::CompositionStart(payload),
        EventKind::CompositionEnd => Event::CompositionEnd(payload),
        EventKind::Scroll => Event::Scroll(ScrollEvent { top: a, left: b }),
        EventKind::TransitionEnd => Event::TransitionEnd,
        EventKind::Resize => Event::Resize(Size {
            width: a,
            height: b,
        }),
        EventKind::ContextMenu => Event::ContextMenu(pointer),
        // JSON: [startBlock, startOffset, endBlock, endOffset, inputType, data]
        EventKind::BeforeInput => {
            let v = json::from_str::<Value>(&payload).unwrap_or(Value::Null);
            let n = |i: usize| {
                v.get(i)
                    .and_then(Value::as_i64)
                    .and_then(|x| usize::try_from(x).ok())
                    .unwrap_or(0)
            };
            let s = |i: usize| {
                v.get(i)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            };
            Event::BeforeInput(BeforeInputEvent {
                input_type: s(4),
                data: s(5),
                start: BlockPoint {
                    block: n(0),
                    offset: n(1),
                },
                end: BlockPoint {
                    block: n(2),
                    offset: n(3),
                },
            })
        }
        // JSON: [anchorBlock, anchorOffset, focusBlock, focusOffset]
        EventKind::EditSelection => {
            let v = json::from_str::<Value>(&payload).unwrap_or(Value::Null);
            let n = |i: usize| {
                v.get(i)
                    .and_then(Value::as_i64)
                    .and_then(|x| usize::try_from(x).ok())
                    .unwrap_or(0)
            };
            Event::EditSelection(EditSelectionEvent {
                anchor: BlockPoint {
                    block: n(0),
                    offset: n(1),
                },
                focus: BlockPoint {
                    block: n(2),
                    offset: n(3),
                },
            })
        }
        EventKind::Selection => Event::Selection(SelectionEvent {
            start: a.max(0.0) as usize,
            end: b.max(0.0) as usize,
        }),
        EventKind::Copy => Event::Copy,
        EventKind::Cut => Event::Cut,
        // The payload is JSON: {"text": ..., "html": ...}; anything else is an empty paste.
        EventKind::Paste => {
            let value = json::from_str::<Value>(&payload).unwrap_or(Value::Null);
            let field = |name: &str| {
                value
                    .get(name)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string()
            };
            Event::Paste(PasteEvent {
                text: field("text"),
                html: field("html"),
            })
        }
        EventKind::Wheel => {
            let n: Vec<f64> = payload.split(',').filter_map(|v| v.parse().ok()).collect();
            let at = |i: usize| n.get(i).copied().unwrap_or(0.0);
            Event::Wheel(WheelEvent {
                x: at(0),
                y: at(1),
                width: at(2),
                height: at(3),
                dx: at(4),
                dy: at(5),
                ctrl: flags & 1 != 0,
                shift: flags & 2 != 0,
                alt: flags & 4 != 0,
                meta: flags & 8 != 0,
            })
        }
    })
}
