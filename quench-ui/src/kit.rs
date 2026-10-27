//! Components for touch-first apps.
//!
//! App bar, bottom navigation, sheets and dialogs, snackbar, list tiles, switch, checkbox, text
//! field, floating action button, chips, tabs, pull to refresh, swipe actions, and date and time
//! fields.
//!
//! They are plain functions returning elements with `q-*` classes; `quench-style` (see its
//! `components` module) styles them with the theme's colours. Each carries the ARIA roles and
//! keyboard handling its pattern needs, so an app gets a working keyboard and screen reader
//! experience without further work.

use crate::anim::presence;
use crate::event::{Event, EventKind, prevent_default};
use crate::form::Field;
use crate::gesture::{Axis, Config, Gesture, on_gesture};
use crate::reactive::{Signal, batch, create_signal, untrack};
use crate::view::{
    Element, IntoView, View, button, div, h1, header, input, label, li, nav, p, span, ul,
};
use std::rc::Rc;

type Handler0 = Rc<dyn Fn()>;

/// The bar across the top of a screen: a title, an optional leading control (a back or menu
/// button) and trailing actions.
#[must_use]
pub fn app_bar(title: &str, leading: Option<View>, actions: Vec<View>) -> Element {
    header()
        .class("q-appbar")
        .children(leading)
        .child(h1().class("q-appbar-title").text(title))
        .child(div().class("q-appbar-actions").children(actions))
}

/// A square button with a symbol and an accessible name. `glyph` is decorative text (an emoji or
/// a symbol character); `label` is what a screen reader says.
#[must_use]
pub fn icon_button(label: &str, glyph: &str, on_click: impl Fn() + 'static) -> Element {
    button()
        .attr("type", "button")
        .attr("aria-label", label)
        .class("q-iconbtn")
        .child(span().attr("aria-hidden", "true").text(glyph))
        .on_click(on_click)
}

/// One destination of a [`bottom_nav`].
#[derive(Clone, Debug)]
pub struct NavItem {
    pub label: String,
    pub glyph: String,
}

impl NavItem {
    #[must_use]
    pub fn new(label: &str, glyph: &str) -> Self {
        Self {
            label: label.into(),
            glyph: glyph.into(),
        }
    }
}

/// Navigation between a few top-level destinations, along the bottom of the screen. `selected`
/// is the index of the current one; choosing another sets it.
#[must_use]
pub fn bottom_nav(label: &str, items: Vec<NavItem>, selected: &Signal<usize>) -> Element {
    let buttons = items.into_iter().enumerate().map(|(i, item)| {
        let (current, choose) = (selected.clone(), selected.clone());
        button()
            .attr("type", "button")
            .class("q-bottomnav-item")
            .attr_opt("aria-current", move || {
                (current.get() == i).then(|| "page".to_string())
            })
            .child(
                span()
                    .class("q-bottomnav-glyph")
                    .attr("aria-hidden", "true")
                    .text(item.glyph),
            )
            .child(span().class("q-bottomnav-label").text(item.label))
            .on_click(move || choose.set(i))
            .into_view()
    });
    nav()
        .class("q-bottomnav")
        .attr("aria-label", label)
        .children(buttons)
}

/// What a modal does when it is to close: on Escape, and on a tap on the scrim.
fn modal(open: &Signal<bool>, title: &str, kind: &'static str, content: View) -> View {
    let (scrim, keys) = (open.clone(), open.clone());
    div()
        .class("q-modal")
        .child(div().class("q-scrim").on_click(move || scrim.set(false)))
        .child(
            div()
                .class(kind)
                .attr("role", "dialog")
                .attr("aria-modal", "true")
                .attr("aria-label", title)
                .attr("tabindex", "-1")
                .trap_focus()
                .on_keydown(move |k| {
                    if k.key == "Escape" {
                        keys.set(false);
                    }
                })
                .child(h1().class("q-modal-title").text(title))
                .child(content),
        )
        .into_view()
}

/// A panel that slides up from the bottom, over the page, while `open` is true.
///
/// It traps focus, closes on Escape and on a tap outside, and sets `open` to false when it does.
/// Pair it with `Router::on_back` so the hardware back button closes it too.
#[must_use]
pub fn sheet(open: &Signal<bool>, title: &str, content: impl Fn() -> View + 'static) -> View {
    let (visible, closer, title) = (open.clone(), open.clone(), title.to_string());
    presence(
        move || visible.get(),
        250,
        move || modal(&closer, &title, "q-sheet", content()),
    )
}

/// As [`sheet`], centred, with `actions` (buttons) along the bottom.
#[must_use]
pub fn dialog(
    open: &Signal<bool>,
    title: &str,
    content: impl Fn() -> View + 'static,
    actions: impl Fn() -> Vec<View> + 'static,
) -> View {
    let (visible, closer, title) = (open.clone(), open.clone(), title.to_string());
    presence(
        move || visible.get(),
        200,
        move || {
            let body = div()
                .child(content())
                .child(div().class("q-dialog-actions").children(actions()))
                .into_view();
            modal(&closer, &title, "q-dialog", body)
        },
    )
}

/// A brief message at the bottom of the screen, announced politely by screen readers.
///
/// It shows while `message` holds text; `action` adds a button (label, handler) such as "Undo".
/// Clearing `message` after a few seconds is the app's job ([`toast`] does it).
#[must_use]
pub fn snackbar(message: &Signal<Option<String>>, action: Option<(&str, Handler0)>) -> View {
    let message = message.clone();
    let action = action.map(|(label, handler)| (label.to_string(), handler));
    View::lazy(move || snackbar_view(&message, action))
}

fn snackbar_view(message: &Signal<Option<String>>, action: Option<(String, Handler0)>) -> View {
    // The text stays while the snackbar fades out, after `message` is already empty.
    let last = create_signal(String::new());
    let (visible, remember, text) = (message.clone(), message.clone(), last.clone());
    let hide = message.clone();
    crate::reactive::create_effect(move || {
        if let Some(m) = remember.get() {
            last.set(m);
        }
    });
    presence(
        move || visible.get().is_some(),
        200,
        move || {
            let text = text.clone();
            let mut bar = div()
                .class("q-snackbar")
                .attr("role", "status")
                .attr("aria-live", "polite")
                .child(span().class("q-snackbar-text").text_fn(move || text.get()));
            if let Some((label, handler)) = action.clone() {
                let hide = hide.clone();
                bar = bar.child(
                    button()
                        .attr("type", "button")
                        .class("q-snackbar-action")
                        .text(label)
                        .on_click(move || {
                            handler();
                            hide.set(None);
                        }),
                );
            }
            bar.into_view()
        },
    )
}

/// Shows `text` in a [`snackbar`] and clears it after `ms` milliseconds (unless a newer message
/// has replaced it by then).
pub fn toast(message: &Signal<Option<String>>, text: &str, ms: u32) {
    message.set(Some(text.to_string()));
    let (message, text) = (message.clone(), text.to_string());
    crate::platform::after(ms, move || {
        if message.get_untracked().as_deref() == Some(text.as_str()) {
            message.set(None);
        }
    });
}

/// A list of tiles.
#[must_use]
pub fn list(tiles: Vec<View>) -> Element {
    ul().class("q-list").children(tiles)
}

/// One row of a [`list`]: a title, optionally a second line, a leading and a trailing view. With
/// `on_tap` the whole row is a button.
#[must_use]
pub fn list_tile(
    title: &str,
    subtitle: Option<&str>,
    leading: Option<View>,
    trailing: Option<View>,
    on_tap: Option<Handler0>,
) -> View {
    li().child(tile(title, subtitle, leading, trailing, on_tap))
        .into_view()
}

/// The row of a [`list_tile`] without its `<li>`, for putting inside another (a [`swipe_action`]).
#[must_use]
pub fn tile(
    title: &str,
    subtitle: Option<&str>,
    leading: Option<View>,
    trailing: Option<View>,
    on_tap: Option<Handler0>,
) -> Element {
    let text = div()
        .class("q-tile-text")
        .child(span().class("q-tile-title").text(title))
        .child(subtitle.map(|s| span().class("q-tile-subtitle").text(s)));
    let row = |tag: Element| {
        tag.class("q-tile")
            .child(leading.map(|v| div().class("q-tile-leading").child(v)))
            .child(text)
            .child(trailing.map(|v| div().class("q-tile-trailing").child(v)))
    };
    match on_tap {
        Some(tap) => row(button().attr("type", "button")).on_click(move || tap()),
        None => row(div()),
    }
}

/// An on/off switch with a label. A button with `role="switch"`: Space and Enter toggle it.
#[must_use]
pub fn switch(label: &str, on: &Signal<bool>) -> Element {
    let (state, toggle) = (on.clone(), on.clone());
    button()
        .attr("type", "button")
        .attr("role", "switch")
        .class("q-switch")
        .attr_fn("aria-checked", move || state.get().to_string())
        .child(
            span()
                .class("q-switch-track")
                .attr("aria-hidden", "true")
                .child(span().class("q-switch-thumb")),
        )
        .child(span().class("q-switch-label").text(label))
        .on_click(move || toggle.update(|v| *v = !*v))
}

/// A checkbox with a label: the browser's own `<input type="checkbox">`, so keyboard and screen
/// reader support is the platform's.
#[must_use]
pub fn checkbox(text: &str, checked: &Signal<bool>) -> Element {
    let (state, toggle) = (checked.clone(), checked.clone());
    label()
        .class("q-checkbox")
        .child(
            input()
                .attr("type", "checkbox")
                .attr_bool("checked", move || state.get())
                .on_click(move || toggle.update(|v| *v = !*v)),
        )
        .child(span().class("q-checkbox-label").text(text))
}

/// A labelled text input bound to `field`, with its hint and its error. `kind` is the input type
/// (`text`, `email`, `password`, `search`, `tel`, ...).
#[must_use]
pub fn text_field(text: &str, field: &Field, kind: &str, hint: Option<&str>) -> Element {
    let hint_id = format!("{}-hint", field.name());
    let mut control = field.input().attr("type", kind).class("q-textfield-input");
    if let Some(h) = hint {
        control = control.attr("placeholder", h);
    }
    div()
        .class("q-textfield")
        .child(
            label()
                .attr("for", field.name())
                .class("q-textfield-label")
                .text(text),
        )
        .child(control)
        .child(hint.map(|h| p().attr("id", hint_id).class("q-textfield-hint").text(h)))
        .child(field.error_view().class("q-textfield-error"))
}

/// A date field: the platform's date picker (`<input type="date">`), holding `YYYY-MM-DD`.
#[must_use]
pub fn date_field(text: &str, field: &Field) -> Element {
    text_field(text, field, "date", None)
}

/// A time field: the platform's time picker, holding `HH:MM`.
#[must_use]
pub fn time_field(text: &str, field: &Field) -> Element {
    text_field(text, field, "time", None)
}

/// The floating button for a screen's main action.
#[must_use]
pub fn fab(label: &str, glyph: &str, on_click: impl Fn() + 'static) -> Element {
    icon_button(label, glyph, on_click).class("q-fab")
}

/// A toggle chip (a filter): pressed or not, with `aria-pressed`.
#[must_use]
pub fn chip(text: &str, pressed: &Signal<bool>) -> Element {
    let (state, toggle) = (pressed.clone(), pressed.clone());
    button()
        .attr("type", "button")
        .class("q-chip")
        .class_if("q-chip-on", {
            let state = state.clone();
            move || state.get()
        })
        .attr_fn("aria-pressed", move || state.get().to_string())
        .text(text)
        .on_click(move || toggle.update(|v| *v = !*v))
}

/// Tabs over panels. Arrow keys, Home and End move between the tabs (the way a `tablist` is
/// expected to); `panel(i)` builds the content of tab `i`.
#[must_use]
pub fn tabs(
    id: &str,
    labels: &[&str],
    selected: &Signal<usize>,
    panel: impl Fn(usize) -> View + 'static,
) -> Element {
    let count = labels.len();
    let by_key = create_signal(false);
    let tab_buttons = labels.iter().enumerate().map(|(i, text)| {
        let (current, tab_index, pick, keys, focused) = (
            selected.clone(),
            selected.clone(),
            selected.clone(),
            (selected.clone(), by_key.clone()),
            (selected.clone(), by_key.clone()),
        );
        button()
            .attr("type", "button")
            .attr("role", "tab")
            .attr("id", format!("{id}-tab-{i}"))
            .attr("aria-controls", format!("{id}-panel"))
            .class("q-tab")
            .attr_fn("aria-selected", move || (current.get() == i).to_string())
            .attr_fn("tabindex", move || {
                if tab_index.get() == i { "0" } else { "-1" }.to_string()
            })
            .class_if("q-tab-on", {
                let on = selected.clone();
                move || on.get() == i
            })
            .focus_if(move || focused.1.get() && focused.0.get() == i)
            .text(*text)
            .on_click(move || {
                keys.1.set(false);
                pick.set(i);
            })
            .on_keydown({
                let (selected, by_key) = (selected.clone(), by_key.clone());
                move |k| {
                    let at = selected.get_untracked();
                    let to = match k.key.as_str() {
                        "ArrowRight" => (at + 1) % count,
                        "ArrowLeft" => (at + count - 1) % count,
                        "Home" => 0,
                        "End" => count - 1,
                        _ => return,
                    };
                    prevent_default(); // Home and End would scroll the page
                    batch(|| {
                        by_key.set(true);
                        selected.set(to);
                    });
                }
            })
            .into_view()
    });
    let shown = selected.clone();
    let (labelled, id_panel) = (selected.clone(), format!("{id}-panel"));
    div()
        .class("q-tabs")
        .child(
            div()
                .class("q-tablist")
                .attr("role", "tablist")
                .children(tab_buttons),
        )
        .child(
            div()
                .class("q-tabpanel")
                .attr("role", "tabpanel")
                .attr("id", id_panel)
                .attr("tabindex", "0")
                .attr_fn("aria-labelledby", {
                    let id = id.to_string();
                    move || format!("{id}-tab-{}", labelled.get())
                })
                .child(View::dynamic(move || {
                    let i = shown.get();
                    untrack(|| panel(i))
                })),
        )
}

/// How far a pull must go to refresh, in pixels.
pub const PULL_THRESHOLD: f64 = 64.0;

/// Pull down at the top of `content` to refresh.
///
/// `on_refresh` is called when the pull is released past [`PULL_THRESHOLD`]; it should set
/// `refreshing` back to false when done. A progress indicator (role `progressbar`) shows while
/// pulling and refreshing.
///
/// Touch only: with a mouse, use a button. Whether the browser lets the app have the downward
/// pull at the top of a scroll area while scrolling the rest is a matter of `touch-action`, set
/// by the `q-ptr-top` class; check it on the devices you ship to.
#[must_use]
pub fn pull_to_refresh(
    refreshing: &Signal<bool>,
    on_refresh: impl Fn() + 'static,
    content: View,
) -> Element {
    let pull = create_signal(0.0_f64);
    let at_top = create_signal(true);
    let (scroll_top, pulling, done, shown) = (
        at_top.clone(),
        pull.clone(),
        refreshing.clone(),
        refreshing.clone(),
    );
    let (offset, indicator) = (pull.clone(), pull);
    let class_top = at_top.clone();
    let element = div()
        .class("q-ptr")
        .class_if("q-ptr-top", move || class_top.get())
        .css("--q-pull", move || format!("{}px", offset.get()))
        .on(EventKind::Scroll, move |e| {
            if let Event::Scroll(s) = e {
                scroll_top.set(s.top <= 0.0);
            }
        })
        .child(
            div()
                .class("q-ptr-indicator")
                .attr("role", "progressbar")
                .attr("aria-label", "Refreshing")
                .attr_fn("aria-hidden", move || {
                    (!shown.get() && indicator.get() <= 0.0).to_string()
                })
                .class_if("q-ptr-busy", {
                    let busy = refreshing.clone();
                    move || busy.get()
                }),
        )
        .child(div().class("q-ptr-content").child(content));
    on_gesture(
        element,
        Config {
            axis: Axis::Vertical,
            ..Config::default()
        },
        move |g| match g {
            Gesture::Drag { dy, .. } if at_top.get_untracked() && !done.get_untracked() => {
                pulling.set(dy.clamp(0.0, PULL_THRESHOLD * 1.5));
            }
            Gesture::DragEnd { .. } => {
                let released = pulling.get_untracked();
                pulling.set(0.0);
                if released >= PULL_THRESHOLD && !done.get_untracked() {
                    done.set(true);
                    on_refresh();
                }
            }
            Gesture::Cancel => pulling.set(0.0),
            _ => {}
        },
    )
}

/// How far a row slides to show its action, in pixels.
pub const SWIPE_REVEAL: f64 = 88.0;

/// A row that slides left to show an action button (delete, archive). The button is always in
/// the page, so the keyboard and screen readers reach it; the swipe is the touch shortcut.
#[must_use]
pub fn swipe_action(content: View, action: &str, on_action: impl Fn() + 'static) -> Element {
    let offset = create_signal(0.0_f64);
    let (shift, reset, drag) = (offset.clone(), offset.clone(), offset);
    let front = div()
        .class("q-swipe-front")
        .css("transform", move || {
            format!("translateX({}px)", shift.get())
        })
        .child(content);
    let back = div().class("q-swipe-back").child(
        button()
            .attr("type", "button")
            .class("q-swipe-action")
            .text(action)
            .on_click(move || {
                reset.set(0.0);
                on_action();
            }),
    );
    on_gesture(
        div().class("q-swipe").child(back).child(front),
        Config {
            axis: Axis::Horizontal,
            ..Config::default()
        },
        move |g| match g {
            Gesture::Drag { dx, .. } => {
                let open = if drag.get_untracked() <= -SWIPE_REVEAL + 1.0 {
                    -SWIPE_REVEAL
                } else {
                    0.0
                };
                drag.set((open + dx).clamp(-SWIPE_REVEAL, 0.0));
            }
            Gesture::DragEnd { .. } => {
                let now = drag.get_untracked();
                drag.set(if now < -SWIPE_REVEAL / 2.0 {
                    -SWIPE_REVEAL
                } else {
                    0.0
                });
            }
            Gesture::Cancel => drag.set(0.0),
            _ => {}
        },
    )
}
