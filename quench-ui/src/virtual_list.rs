//! A list that builds only the rows in view, for data sets too long to put in the page whole.
//!
//! Rows have one fixed height. The scroll element holds a spacer as tall as the whole list, and
//! the rows that are visible (plus a few more each side) sit inside a window moved to where they
//! belong. Scrolling to another place builds the rows that came into view and drops the rest;
//! scrolling a little keeps the rows it keeps, so a flick through 10,000 rows costs about as much as
//! a flick through 10.
//!
//! The list needs a height from your stylesheet (`.q-vlist` scrolls, and a bounded height is what
//! makes it scroll): a fixed height, or a flex or grid cell.

use crate::event::Event;
use crate::reactive::{Signal, create_memo, create_signal};
use crate::view::{Element, IntoView, Key, View, div};

/// The viewport height assumed until the browser reports the real one.
const ASSUMED_VIEWPORT: f64 = 640.0;

/// Rows built beyond each edge of the viewport, so a fast scroll shows content, not gaps.
pub const OVERSCAN: usize = 5;

/// A scrolling list of `render(item)` rows, `item_height` pixels tall each, for `items`. A row is
/// matched across changes by `key(&item)`, as in [`View::keyed`].
#[must_use]
pub fn virtual_list<T, K>(
    items: &Signal<Vec<T>>,
    item_height: f64,
    key: impl Fn(&T) -> K + 'static,
    render: impl Fn(T) -> View + 'static,
) -> Element
where
    T: Clone + 'static,
    K: Into<Key>,
{
    let scroll = create_signal(0.0_f64);
    let viewport = create_signal(ASSUMED_VIEWPORT);

    // The rows to build: only changes when the first or last row in view does.
    let range = {
        let (items, scroll, viewport) = (items.clone(), scroll.clone(), viewport.clone());
        create_memo(move || {
            let len = items.with(Vec::len);
            let (top, height) = (scroll.get().max(0.0), viewport.get());
            let first = rows_before(top, item_height).saturating_sub(OVERSCAN);
            let last = rows_before(top + height, item_height) + 1 + OVERSCAN;
            (first.min(len), last.min(len))
        })
    };

    let space = {
        let items = items.clone();
        // A row's height is a custom property on the window, which `.q-vlist-row` reads.
        div().class("q-vlist-space").css("height", move || {
            format!("{}px", rows_px(items.with(Vec::len), item_height))
        })
    };
    let offset = range.clone();
    let rows = {
        let (items, range) = (items.clone(), range);
        View::keyed(
            move || {
                let (first, last) = range.get();
                items.with(|all| all[first..last].to_vec())
            },
            key,
            move |item| {
                div()
                    .class("q-vlist-row")
                    .attr("role", "listitem")
                    .child(render(item))
                    .into_view()
            },
        )
    };
    let window = div()
        .class("q-vlist-window")
        .attr("role", "list")
        .css("--q-row-h", move || format!("{item_height}px"))
        .css("transform", move || {
            format!("translateY({}px)", rows_px(offset.get().0, item_height))
        })
        .child(rows);

    div()
        .class("q-vlist")
        .on(crate::EventKind::Scroll, move |e| {
            if let Event::Scroll(s) = e {
                scroll.set(s.top);
            }
        })
        .on(crate::EventKind::Resize, move |e| {
            if let Event::Resize(size) = e {
                viewport.set(size.height);
            }
        })
        .child(space.child(window))
}

/// The height of `rows` rows.
#[allow(clippy::cast_precision_loss)] // lists are far shorter than 2^52 rows
fn rows_px(rows: usize, item_height: f64) -> f64 {
    rows as f64 * item_height
}

/// How many whole rows lie above `y`.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a row index, never negative
fn rows_before(y: f64, item_height: f64) -> usize {
    // A non-finite or negative height cannot place rows: show from the top.
    if item_height.is_finite() && item_height > 0.0 {
        (y / item_height).floor().max(0.0) as usize
    } else {
        0
    }
}
