//! Pointer, keyboard and focus: what turns a window's events into the events views listen for.
//!
//! The host calls [`NativeRenderer::pointer_move`], [`pointer_down`](NativeRenderer::pointer_down),
//! [`pointer_up`](NativeRenderer::pointer_up), [`wheel`](NativeRenderer::wheel),
//! [`key_down`](NativeRenderer::key_down) and [`text`](NativeRenderer::text) with what happened; the renderer
//! finds the target in the last layout, delivers the events (bubbling through the DOM), and does what a browser
//! does when no listener prevents it: focus, click on release, toggling checkboxes, scrolling, Tab, editing.

use quench_layout::{Fragment, LayoutBox};
use quench_paint::Layers;
use quench_ui::event::{ScrollEvent, take_prevent_request};
use quench_ui::{Event, Handler, KeyEvent, PointerEvent};

use crate::dom::{Dom, Kind, NativeRenderer};

/// What an input method needs: the text of the focused field, the caret and the other end of the selection
/// (byte offsets), and the caret's rectangle in the window.
#[derive(Debug, Clone, PartialEq)]
pub struct ImeState {
    pub text: String,
    pub cursor: usize,
    pub anchor: usize,
    pub rect: (f32, f32, f32, f32),
}

/// The pixels one wheel notch or arrow key press scrolls.
const LINE: f32 = 40.0;

impl NativeRenderer {
    /// Lays the document out if it has changed since the last layout (pointer events are matched against it).
    fn ensure_layout(&self) {
        let stale = {
            let dom = self.dom.borrow();
            dom.layout.is_none() || dom.ui.stale
        };
        if stale {
            let _ = self.layout();
        }
    }

    /// Whether anything changed since the last call: the host redraws when it did.
    pub fn take_dirty(&self) -> bool {
        std::mem::take(&mut self.dom.borrow_mut().ui.dirty)
    }

    /// Shows or hides the caret of the focused field (the host blinks it).
    pub fn set_caret_visible(&self, on: bool) {
        let mut dom = self.dom.borrow_mut();
        dom.ui.caret_on = on;
        dom.ui.dirty = true;
    }

    /// The scroll position of a scroll container, by the key of its box.
    #[must_use]
    pub fn scroll_of(&self, key: usize) -> (f32, f32) {
        self.dom
            .borrow()
            .ui
            .scroll
            .get(&key)
            .copied()
            .unwrap_or((0.0, 0.0))
    }

    /// The element focus is on.
    #[must_use]
    pub fn focused(&self) -> Option<crate::NodeId> {
        self.dom.borrow().ui.focused.map(crate::NodeId)
    }

    /// Whether a text field has focus (a focused button has no caret to blink).
    #[must_use]
    pub fn text_field_focused(&self) -> bool {
        let dom = self.dom.borrow();
        dom.ui.focused.is_some_and(|f| is_text_field(&dom, f))
    }

    /// The element under a point of the window (the deepest one a pointer would land on).
    #[must_use]
    pub fn element_at(&self, x: f32, y: f32) -> Option<crate::NodeId> {
        self.ensure_layout();
        let dom = self.dom.borrow();
        hit(&dom, x, y).map(crate::NodeId)
    }

    /// Delivers `event` to `target` and then to its ancestors (when `bubbles`). Returns whether a listener asked
    /// for the default action to be stopped.
    fn dispatch(&self, target: usize, event: &Event, bubbles: bool) -> bool {
        let mut path = vec![target];
        if bubbles {
            let dom = self.dom.borrow();
            let mut at = target;
            while let Some(parent) = dom.nodes[at].parent {
                path.push(parent);
                at = parent;
            }
        }
        let _ = take_prevent_request();
        for id in path {
            let handlers: Vec<Handler> = match &self.dom.borrow().nodes[id].kind {
                Kind::Element { handlers, .. } => handlers
                    .iter()
                    .filter(|(k, _)| *k == event.kind())
                    .map(|(_, h)| h.clone())
                    .collect(),
                _ => Vec::new(),
            };
            for h in handlers {
                h(event);
            }
        }
        self.dom.borrow_mut().ui.dirty = true;
        take_prevent_request()
    }

    /// The pointer moved to `(x, y)`.
    pub fn pointer_move(&self, x: f32, y: f32) {
        self.ensure_layout();
        let target = hit(&self.dom.borrow(), x, y);
        {
            let mut dom = self.dom.borrow_mut();
            if dom.ui.hovered != target {
                dom.ui.hovered = target;
                dom.ui.dirty = true;
            }
        }
        // A slider follows a finger or a button held down on it.
        let dragged = {
            let dom = self.dom.borrow();
            dom.ui.active.filter(|a| is_range(&dom, *a))
        };
        if let Some(f) = dragged {
            self.range_to_x(f, x);
        }
        if let Some(t) = target {
            self.dispatch(t, &Event::PointerMove(pointer(x, y)), true);
        }
    }

    /// The pointer gesture that went down is not a click after all (a touch turned into a scroll, or was
    /// taken over): nothing is clicked when it ends.
    pub fn pointer_cancel(&self) {
        let mut dom = self.dom.borrow_mut();
        dom.ui.pressed = None;
        dom.ui.active = None;
        dom.ui.dirty = true;
    }

    /// The primary button went down at `(x, y)`.
    pub fn pointer_down(&self, x: f32, y: f32) {
        self.ensure_layout();
        self.pointer_move(x, y);
        let target = hit(&self.dom.borrow(), x, y);
        let Some(t) = target else {
            self.blur();
            return;
        };
        {
            let mut dom = self.dom.borrow_mut();
            dom.ui.pressed = Some(t);
            dom.ui.active = Some(t);
            dom.ui.dirty = true;
        }
        let prevented = self.dispatch(t, &Event::PointerDown(pointer(x, y)), true);
        if prevented {
            return;
        }
        let focusable = {
            let dom = self.dom.borrow();
            focusable_ancestor(&dom, t)
        };
        match focusable {
            Some(f) => {
                self.focus_node(f, false);
                if is_text_field(&self.dom.borrow(), f) {
                    self.place_caret(f, x, y);
                } else if is_range(&self.dom.borrow(), f) {
                    self.range_to_x(f, x);
                }
            }
            None => self.blur(),
        }
    }

    /// The primary button went up at `(x, y)`: a click when it is on what it went down on.
    pub fn pointer_up(&self, x: f32, y: f32) {
        self.ensure_layout();
        let target = hit(&self.dom.borrow(), x, y);
        let pressed = {
            let mut dom = self.dom.borrow_mut();
            dom.ui.active = None;
            dom.ui.dirty = true;
            dom.ui.pressed.take()
        };
        let Some(t) = target else { return };
        self.dispatch(t, &Event::PointerUp(pointer(x, y)), true);
        let Some(p) = pressed else { return };
        // A click lands on the nearest element that contains both ends of the gesture.
        let common = {
            let dom = self.dom.borrow();
            common_ancestor(&dom, p, t)
        };
        if let Some(c) = common {
            self.click(c);
        }
    }

    /// A click on `node`: the default actions of controls, then the listeners.
    fn click(&self, node: usize) {
        let (control, label_for) = {
            let dom = self.dom.borrow();
            (control_of(&dom, node), label_target(&dom, node))
        };
        if let Some(c) = control {
            self.toggle(c);
        }
        let prevented = self.dispatch(node, &Event::Click, true);
        if !prevented && let Some(l) = label_for {
            self.focus_node(l, false);
            self.click(l);
        }
        if !prevented && let Some(form) = self.submitted_form(node) {
            self.dispatch(form, &Event::Submit, false);
        }
    }

    /// The form a click on `node` submits: `node` is (inside) a submit button, `<button>` with no other type or
    /// `<input type=submit>`, and has a `<form>` around it.
    fn submitted_form(&self, node: usize) -> Option<usize> {
        let dom = self.dom.borrow();
        let mut at = Some(node);
        let mut submits = false;
        while let Some(n) = at {
            match dom.tag(n) {
                Some("button") if !submits => {
                    submits = matches!(dom.attr(n, "type"), None | Some("submit" | ""));
                    if !submits {
                        return None;
                    }
                }
                Some("input") if !submits => {
                    submits = dom.attr(n, "type") == Some("submit");
                    if !submits {
                        return None;
                    }
                }
                Some("form") => return submits.then_some(n),
                _ => {}
            }
            at = dom.nodes[n].parent;
        }
        None
    }

    /// Checks or unchecks a checkbox or radio button the way a click does.
    fn toggle(&self, id: usize) {
        let mut dom = self.dom.borrow_mut();
        let kind = dom.attr(id, "type").unwrap_or("").to_string();
        let checked = dom.attr(id, "checked").is_some();
        if kind == "radio" {
            let name = dom.attr(id, "name").map(String::from);
            if let Some(name) = name {
                let group: Vec<usize> = (0..dom.nodes.len())
                    .filter(|n| {
                        dom.tag(*n) == Some("input")
                            && dom.attr(*n, "type") == Some("radio")
                            && dom.attr(*n, "name") == Some(name.as_str())
                    })
                    .collect();
                for n in group {
                    set_attr_raw(&mut dom, n, "checked", None);
                }
            }
            set_attr_raw(&mut dom, id, "checked", Some(""));
        } else {
            set_attr_raw(
                &mut dom,
                id,
                "checked",
                if checked { None } else { Some("") },
            );
        }
        dom.ui.dirty = true;
    }

    /// The wheel turned at `(x, y)`: `dy` positive is down, in pixels.
    pub fn wheel(&self, x: f32, y: f32, dx: f32, dy: f32) {
        self.ensure_layout();
        let target = hit(&self.dom.borrow(), x, y);
        if let Some(t) = target {
            self.scroll_from(t, dx, dy);
        }
    }

    /// Scrolls the nearest scroll container at or above `node` that can still move that way.
    fn scroll_from(&self, node: usize, dx: f32, dy: f32) {
        let moved = {
            let mut dom = self.dom.borrow_mut();
            let Some(root) = dom.layout.clone() else {
                return;
            };
            let mut at = Some(node);
            let mut moved = None;
            while let Some(id) = at {
                if let Some(b) = root.find(id) {
                    let (_, _, pw, ph) = b.padding_rect();
                    let (ox, oy) = (b.style.overflow.0, b.style.overflow.1);
                    let can_y = matches!(
                        oy,
                        quench_css::Overflow::Auto | quench_css::Overflow::Scroll
                    );
                    let can_x = matches!(
                        ox,
                        quench_css::Overflow::Auto | quench_css::Overflow::Scroll
                    );
                    let (mut max_y, mut max_x) = (
                        if can_y {
                            (b.overflow.1 - ph).max(0.0)
                        } else {
                            0.0
                        },
                        if can_x {
                            (b.overflow.0 - pw).max(0.0)
                        } else {
                            0.0
                        },
                    );
                    if id == 0 {
                        // The document scrolls in the window.
                        max_y = (b.overflow.1.max(b.height) - dom.viewport.1).max(0.0);
                        max_x = 0.0;
                    }
                    let (cx, cy) = dom.ui.scroll.get(&id).copied().unwrap_or((0.0, 0.0));
                    let (nx, ny) = ((cx + dx).clamp(0.0, max_x), (cy + dy).clamp(0.0, max_y));
                    if (nx, ny) != (cx, cy) {
                        dom.ui.scroll.insert(id, (nx, ny));
                        dom.ui.dirty = true;
                        moved = Some((id, nx, ny));
                        break;
                    }
                }
                at = dom.nodes[id].parent;
            }
            moved
        };
        if let Some((id, x, y)) = moved {
            self.dispatch(
                id,
                &Event::Scroll(ScrollEvent {
                    top: f64::from(y),
                    left: f64::from(x),
                }),
                false,
            );
        }
    }

    /// Moves focus to `id` (`keyboard`: it came from Tab, so `:focus-visible` applies).
    pub(crate) fn focus_node(&self, id: usize, keyboard: bool) {
        let previous = self.dom.borrow().ui.focused;
        if previous == Some(id) {
            return;
        }
        if let Some(p) = previous {
            self.dispatch(p, &Event::Blur, false);
        }
        {
            let mut dom = self.dom.borrow_mut();
            dom.ui.focused = Some(id);
            dom.ui.focus_visible = keyboard || is_text_field(&dom, id);
            let len = field_value(&dom, id).chars().count();
            dom.ui.caret = len;
            dom.ui.anchor = len;
            dom.ui.dirty = true;
        }
        self.dispatch(id, &Event::Focus, false);
    }

    fn blur(&self) {
        let previous = self.dom.borrow_mut().ui.focused.take();
        if let Some(p) = previous {
            self.dispatch(p, &Event::Blur, false);
        }
        self.dom.borrow_mut().ui.dirty = true;
    }

    /// Focus moves to the next (or previous) focusable element, staying inside a trapped node.
    fn tab(&self, backwards: bool) {
        let next = {
            let dom = self.dom.borrow();
            let scope = dom.ui.trap.unwrap_or(0);
            let mut order = Vec::new();
            collect_focusable(&dom, scope, &mut order);
            if order.is_empty() {
                None
            } else {
                let at = dom
                    .ui
                    .focused
                    .and_then(|f| order.iter().position(|n| *n == f));
                Some(match (at, backwards) {
                    (None, false) => order[0],
                    (None, true) => order[order.len() - 1],
                    (Some(i), false) => order[(i + 1) % order.len()],
                    (Some(i), true) => order[(i + order.len() - 1) % order.len()],
                })
            }
        };
        if let Some(n) = next {
            self.focus_node(n, true);
        }
    }

    /// A key went down.
    /// Returns whether a listener took the key (asked for the default action to be stopped).
    pub fn key_down(&self, key: &KeyEvent) -> bool {
        let focused = self.dom.borrow().ui.focused;
        let target = focused.unwrap_or(1);
        if self.dispatch(target, &Event::KeyDown(key.clone()), true) {
            return true;
        }
        let k = key.key.as_str();
        if k == "Tab" {
            self.tab(key.shift);
            return false;
        }
        let Some(f) = focused else {
            self.scroll_key(k, key.ctrl);
            return false;
        };
        let (field, tag, kind) = {
            let dom = self.dom.borrow();
            (
                is_text_field(&dom, f),
                dom.tag(f).unwrap_or("").to_string(),
                dom.attr(f, "type").unwrap_or("").to_string(),
            )
        };
        if field {
            self.edit_key(f, key, &tag);
            return false;
        }
        if kind == "range" && tag == "input" && self.range_key(f, k) {
            return false;
        }
        let activates = match (tag.as_str(), k) {
            ("button" | "a", "Enter" | " ") => true,
            ("input", " ") if matches!(kind.as_str(), "checkbox" | "radio") => true,
            _ => false,
        };
        if activates {
            self.click(f);
        } else {
            self.scroll_key(k, key.ctrl);
        }
        false
    }

    /// A key came up.
    pub fn key_up(&self, key: &KeyEvent) {
        let target = self.dom.borrow().ui.focused.unwrap_or(1);
        self.dispatch(target, &Event::KeyUp(key.clone()), true);
    }

    /// Arrow keys, Page Up and Down, Home and End scroll what is under the pointer, or else the page.
    fn scroll_key(&self, key: &str, _ctrl: bool) {
        let (dx, dy) = match key {
            "ArrowDown" => (0.0, LINE),
            "ArrowUp" => (0.0, -LINE),
            "ArrowRight" => (LINE, 0.0),
            "ArrowLeft" => (-LINE, 0.0),
            "PageDown" | " " => (0.0, self.dom.borrow().viewport.1 * 0.9),
            "PageUp" => (0.0, -self.dom.borrow().viewport.1 * 0.9),
            "Home" => (0.0, -1.0e6),
            "End" => (0.0, 1.0e6),
            _ => return,
        };
        let at = {
            let dom = self.dom.borrow();
            dom.ui.hovered.or(dom.ui.focused).unwrap_or(1)
        };
        self.scroll_from(at, dx, dy);
    }

    fn copied(&self, text: String) {
        let mut dom = self.dom.borrow_mut();
        dom.ui.clipboard = text;
        dom.ui.clipboard_out = true;
    }

    /// Makes `text` what a paste inserts (the host reads the system clipboard into it before a paste key).
    pub fn set_clipboard(&self, text: &str) {
        let mut dom = self.dom.borrow_mut();
        dom.ui.clipboard = text.to_string();
        dom.ui.clipboard_out = false;
    }

    /// What was copied or cut since the last call, for the host to put on the system clipboard.
    pub fn take_clipboard_out(&self) -> Option<String> {
        let mut dom = self.dom.borrow_mut();
        std::mem::take(&mut dom.ui.clipboard_out).then(|| dom.ui.clipboard.clone())
    }

    /// Text typed (or pasted, or committed by an input method) into the focused field.
    pub fn text(&self, text: &str) {
        let Some(f) = self.dom.borrow().ui.focused else {
            return;
        };
        if !is_text_field(&self.dom.borrow(), f) || text.is_empty() {
            return;
        }
        let multiline = self.dom.borrow().tag(f) == Some("textarea");
        let text: String = if multiline {
            text.to_string()
        } else {
            text.chars().filter(|c| !c.is_control()).collect()
        };
        self.replace_selection(f, &text);
    }

    /// Replaces the selection of field `f` with `with` and tells the listeners.
    /// Selects the word under `(x, y)` in a text field (a long press): the field takes focus and the word is the
    /// selection. Returns whether there was a text field there.
    pub fn select_word_at(&self, x: f32, y: f32) -> bool {
        self.ensure_layout();
        let field = {
            let dom = self.dom.borrow();
            hit(&dom, x, y).filter(|n| is_text_field(&dom, *n))
        };
        let Some(f) = field else { return false };
        self.focus_node(f, false);
        self.place_caret(f, x, y);
        let mut dom = self.dom.borrow_mut();
        let chars: Vec<char> = field_value(&dom, f).chars().collect();
        let at = dom.ui.caret.min(chars.len());
        let word = |c: char| c.is_alphanumeric() || c == '_';
        // The word the caret is in or at the end of; else the one character beside it.
        let on = |i: usize| chars.get(i).copied().is_some_and(word);
        let (mut lo, mut hi) = (at, at);
        if on(at) || (at > 0 && on(at - 1)) {
            lo = if on(at) { at } else { at - 1 };
            while lo > 0 && on(lo - 1) {
                lo -= 1;
            }
            hi = lo;
            while on(hi) {
                hi += 1;
            }
        } else if at < chars.len() {
            hi = at + 1;
        }
        dom.ui.anchor = lo;
        dom.ui.caret = hi;
        dom.ui.dirty = true;
        true
    }

    /// Where the ends of the selection in the focused text field are on the screen, for handles to be drawn at:
    /// `(x, top, height)` of the start and of the end. `None` when nothing is selected.
    #[must_use]
    pub fn selection_handles(&self) -> Option<[(f32, f32, f32); 2]> {
        self.ensure_layout();
        let dom = self.dom.borrow();
        let f = dom.ui.focused.filter(|f| is_text_field(&dom, *f))?;
        let (a, b) = (dom.ui.anchor, dom.ui.caret);
        if a == b {
            return None;
        }
        let root = dom.layout.clone()?;
        let at = |i: usize| crate::pipeline::spot_at(&dom, &root, f, i);
        Some([at(a.min(b))?, at(a.max(b))?])
    }

    /// Moves one end of the selection to the character nearest `x` (a handle being dragged): `end` is the later
    /// end of the selection, else the earlier one.
    pub fn move_selection_end(&self, end: bool, x: f32) {
        self.ensure_layout();
        let mut dom = self.dom.borrow_mut();
        let Some(f) = dom.ui.focused.filter(|f| is_text_field(&dom, *f)) else {
            return;
        };
        let Some(root) = dom.layout.clone() else {
            return;
        };
        let value = field_value(&dom, f);
        let index = caret_index(&dom, &root, f, &value, x);
        let (a, b) = (dom.ui.anchor, dom.ui.caret);
        // The end that moves keeps being the one that is dragged, so a handle can cross the other.
        let moving_caret = if end { b >= a } else { b <= a };
        if moving_caret {
            dom.ui.caret = index;
        } else {
            dom.ui.anchor = index;
        }
        dom.ui.dirty = true;
    }

    /// Replaces the whole text of a text field (what an assistive technology asks for): the field takes focus, the
    /// caret goes to the end and listeners see an `Input` event, as if the text had been typed.
    ///
    /// Returns whether `id` is a text field.
    pub fn set_field_value(&self, id: usize, text: &str) -> bool {
        let is_field = {
            let dom = self.dom.borrow();
            is_text_field(&dom, id)
        };
        if !is_field {
            return false;
        }
        self.focus_node(id, true);
        {
            let mut dom = self.dom.borrow_mut();
            let len = field_value(&dom, id).chars().count();
            dom.ui.anchor = 0;
            dom.ui.caret = len;
        }
        self.replace_selection(id, text);
        true
    }

    fn replace_selection(&self, f: usize, with: &str) {
        let value = {
            let mut dom = self.dom.borrow_mut();
            let chars: Vec<char> = field_value(&dom, f).chars().collect();
            let (a, b) = (
                dom.ui.caret.min(dom.ui.anchor).min(chars.len()),
                dom.ui.caret.max(dom.ui.anchor).min(chars.len()),
            );
            let mut out: String = chars[..a].iter().collect();
            out.push_str(with);
            out.extend(&chars[b..]);
            let caret = a + with.chars().count();
            dom.ui.caret = caret;
            dom.ui.anchor = caret;
            set_attr_raw(&mut dom, f, "value", Some(&out));
            dom.ui.dirty = true;
            out
        };
        self.dispatch(f, &Event::Input(value), true);
    }

    #[allow(clippy::too_many_lines, reason = "one arm per editing key")]
    fn edit_key(&self, f: usize, key: &KeyEvent, tag: &str) {
        let (caret, anchor, value) = {
            let dom = self.dom.borrow();
            (dom.ui.caret, dom.ui.anchor, field_value(&dom, f))
        };
        let chars: Vec<char> = value.chars().collect();
        let len = chars.len();
        let (lo, hi) = (caret.min(anchor), caret.max(anchor));
        let select = |this: &Self, to: usize| {
            let mut dom = this.dom.borrow_mut();
            dom.ui.caret = to.min(len);
            if !key.shift {
                dom.ui.anchor = dom.ui.caret;
            }
            dom.ui.dirty = true;
        };
        let word_left = |from: usize| {
            let mut i = from;
            while i > 0 && chars[i - 1].is_whitespace() {
                i -= 1;
            }
            while i > 0 && !chars[i - 1].is_whitespace() {
                i -= 1;
            }
            i
        };
        let word_right = |from: usize| {
            let mut i = from;
            while i < len && chars[i].is_whitespace() {
                i += 1;
            }
            while i < len && !chars[i].is_whitespace() {
                i += 1;
            }
            i
        };
        match key.key.as_str() {
            "ArrowLeft" if lo != hi && !key.shift => select(self, lo),
            "ArrowRight" if lo != hi && !key.shift => select(self, hi),
            "ArrowLeft" => select(
                self,
                if key.ctrl {
                    word_left(caret)
                } else {
                    caret.saturating_sub(1)
                },
            ),
            "ArrowRight" => select(
                self,
                if key.ctrl {
                    word_right(caret)
                } else {
                    caret + 1
                },
            ),
            "Home" => select(self, 0),
            "End" => select(self, len),
            "Backspace" | "Delete" => {
                let back = key.key == "Backspace";
                if lo == hi {
                    let (from, to) = if back {
                        (
                            if key.ctrl {
                                word_left(caret)
                            } else {
                                caret.saturating_sub(1)
                            },
                            caret,
                        )
                    } else {
                        (
                            caret,
                            if key.ctrl {
                                word_right(caret)
                            } else {
                                (caret + 1).min(len)
                            },
                        )
                    };
                    if from == to {
                        return;
                    }
                    let mut dom = self.dom.borrow_mut();
                    dom.ui.caret = from;
                    dom.ui.anchor = to;
                }
                self.replace_selection(f, "");
            }
            "Enter" if tag == "textarea" => self.replace_selection(f, "\n"),
            "Enter" => {
                let form = {
                    let dom = self.dom.borrow();
                    let mut at = dom.nodes[f].parent;
                    while let Some(n) = at {
                        if dom.tag(n) == Some("form") {
                            break;
                        }
                        at = dom.nodes[n].parent;
                    }
                    at.filter(|n| dom.tag(*n) == Some("form"))
                };
                if let Some(form) = form {
                    self.dispatch(form, &Event::Submit, false);
                }
            }
            "a" | "A" if key.ctrl => {
                let mut dom = self.dom.borrow_mut();
                dom.ui.anchor = 0;
                dom.ui.caret = len;
                dom.ui.dirty = true;
            }
            "c" | "C" if key.ctrl => {
                let selected: String = chars[lo..hi].iter().collect();
                self.copied(selected);
            }
            "x" | "X" if key.ctrl => {
                let selected: String = chars[lo..hi].iter().collect();
                self.copied(selected);
                if lo != hi {
                    self.replace_selection(f, "");
                }
            }
            "v" | "V" if key.ctrl => {
                let pasted = self.dom.borrow().ui.clipboard.clone();
                self.text(&pasted);
            }
            _ => {}
        }
    }

    /// What an input method needs to know about the focused text field, or `None` when no text field has focus.
    #[must_use]
    pub fn ime_state(&self) -> Option<ImeState> {
        let dom = self.dom.borrow();
        let f = dom.ui.focused.filter(|f| is_text_field(&dom, *f))?;
        let boxes = dom.layout.as_ref()?;
        let value = field_value(&dom, f);
        let char_to_byte = |i: usize| value.char_indices().nth(i).map_or(value.len(), |(b, _)| b);
        let (x, y, height) =
            crate::pipeline::caret_spot(&dom, boxes, f).unwrap_or((0.0, 0.0, 16.0));
        Some(ImeState {
            cursor: char_to_byte(dom.ui.caret),
            anchor: char_to_byte(dom.ui.anchor),
            text: value,
            rect: (x, y, 1.0, height),
        })
    }

    /// A change from an input method, in this order: delete around the caret (bytes), insert the committed
    /// text, then show the composition (`Some(("", ..))` clears it; `cursor` in bytes, negative for none).
    pub fn ime_apply(
        &self,
        delete_before: u32,
        delete_after: u32,
        commit: Option<&str>,
        preedit: Option<(&str, i32)>,
    ) {
        let Some(f) = self
            .dom
            .borrow()
            .ui
            .focused
            .filter(|f| is_text_field(&self.dom.borrow(), *f))
        else {
            return;
        };
        let composing = self.dom.borrow().ui.preedit.is_some();
        if delete_before > 0 || delete_after > 0 {
            let mut dom = self.dom.borrow_mut();
            let value = field_value(&dom, f);
            let caret = dom.ui.caret.min(value.chars().count());
            let chars: Vec<char> = value.chars().collect();
            let (mut from, mut bytes) = (caret, 0);
            while from > 0 && bytes < delete_before as usize {
                from -= 1;
                bytes += chars[from].len_utf8();
            }
            let (mut to, mut bytes) = (caret, 0);
            while to < chars.len() && bytes < delete_after as usize {
                bytes += chars[to].len_utf8();
                to += 1;
            }
            dom.ui.caret = to;
            dom.ui.anchor = from;
        }
        if let Some(text) = commit.filter(|t| !t.is_empty()) {
            self.dom.borrow_mut().ui.preedit = None;
            self.replace_selection(f, text);
        } else if (delete_before > 0 || delete_after > 0) && {
            let dom = self.dom.borrow();
            dom.ui.caret != dom.ui.anchor
        } {
            self.replace_selection(f, "");
        }
        match preedit {
            Some((text, cursor)) if !text.is_empty() => {
                if !composing {
                    let value = field_value(&self.dom.borrow(), f);
                    self.dispatch(f, &Event::CompositionStart(value), true);
                }
                let cursor_chars = if cursor < 0 {
                    text.chars().count()
                } else {
                    text[..(cursor as usize).min(text.len())].chars().count()
                };
                self.dom.borrow_mut().ui.preedit = Some((text.to_string(), cursor_chars));
            }
            _ => {
                self.dom.borrow_mut().ui.preedit = None;
                if composing {
                    let value = field_value(&self.dom.borrow(), f);
                    self.dispatch(f, &Event::CompositionEnd(value), true);
                }
            }
        }
        self.dom.borrow_mut().ui.dirty = true;
    }

    /// Puts the caret of field `f` where the click at `(x, y)` landed.
    /// Sets a slider to the value under `x` (the box's left edge is the minimum, its right edge the maximum).
    fn range_to_x(&self, f: usize, x: f32) {
        let (min, max, step, left, width) = {
            let dom = self.dom.borrow();
            let Some(root) = dom.layout.clone() else {
                return;
            };
            let (Some(b), Some((left, _))) = (root.find(f), root.position_of(f)) else {
                return;
            };
            let (min, max, _) = crate::access::range_of(&dom, f, crate::access::Role::Slider);
            let step = dom
                .attr(f, "step")
                .and_then(|s| s.parse::<f64>().ok())
                .filter(|s| *s > 0.0);
            (min, max, step, left, b.width)
        };
        if width <= 0.0 {
            return;
        }
        let frac = f64::from(((x - left) / width).clamp(0.0, 1.0));
        self.set_range(f, min + (max - min) * frac, min, max, step.unwrap_or(1.0));
    }

    /// Arrow keys, Page Up and Down, Home and End on a slider. Returns whether the key was used.
    fn range_key(&self, f: usize, k: &str) -> bool {
        let (min, max, now, step) = {
            let dom = self.dom.borrow();
            let (min, max, now) = crate::access::range_of(&dom, f, crate::access::Role::Slider);
            let step = dom
                .attr(f, "step")
                .and_then(|s| s.parse::<f64>().ok())
                .filter(|s| *s > 0.0);
            (min, max, now.unwrap_or(min), step)
        };
        let small = step.unwrap_or(1.0);
        let big = ((max - min) / 10.0).max(small);
        let to = match k {
            "ArrowRight" | "ArrowUp" => now + small,
            "ArrowLeft" | "ArrowDown" => now - small,
            "PageUp" => now + big,
            "PageDown" => now - big,
            "Home" => min,
            "End" => max,
            _ => return false,
        };
        self.set_range(f, to, min, max, step.unwrap_or(1.0));
        true
    }

    /// Stores a slider's new value (kept to its step and range) and tells listeners when it changed.
    fn set_range(&self, f: usize, to: f64, min: f64, max: f64, step: f64) {
        let mut value = to.clamp(min, max);
        if step > 0.0 {
            value = (min + ((value - min) / step).round() * step).clamp(min, max);
        }
        let text = if value.fract() == 0.0 {
            format!("{value:.0}")
        } else {
            format!("{value:.4}")
                .trim_end_matches('0')
                .trim_end_matches('.')
                .to_string()
        };
        {
            let mut dom = self.dom.borrow_mut();
            if dom.attr(f, "value") == Some(text.as_str()) {
                return;
            }
            set_attr_raw(&mut dom, f, "value", Some(&text));
            dom.ui.dirty = true;
        }
        self.dispatch(f, &Event::Input(text), true);
    }

    fn place_caret(&self, f: usize, x: f32, _y: f32) {
        let mut dom = self.dom.borrow_mut();
        let Some(root) = dom.layout.clone() else {
            return;
        };
        let value = field_value(&dom, f);
        let index = caret_index(&dom, &root, f, &value, x);
        dom.ui.caret = index;
        dom.ui.anchor = index;
        dom.ui.dirty = true;
    }
}

fn pointer(x: f32, y: f32) -> PointerEvent {
    PointerEvent {
        x: f64::from(x),
        y: f64::from(y),
        pointer_id: 1,
    }
}

fn set_attr_raw(dom: &mut Dom, id: usize, name: &str, value: Option<&str>) {
    if let Kind::Element { attrs, .. } = &mut dom.nodes[id].kind {
        attrs.retain(|(n, _)| n != name);
        if let Some(v) = value {
            attrs.push((name.to_string(), v.to_string()));
        }
    }
}

/// The text of a field: its `value`, or a textarea's own text.
pub(crate) fn field_value(dom: &Dom, id: usize) -> String {
    if let Some(v) = dom.attr(id, "value") {
        return v.to_string();
    }
    if dom.tag(id) == Some("textarea") {
        return dom.nodes[id]
            .children
            .iter()
            .filter_map(|c| {
                if let Kind::Text(t) = &dom.nodes[*c].kind {
                    Some(t.as_str())
                } else {
                    None
                }
            })
            .collect();
    }
    String::new()
}

/// What is shown in a field: its value, with the composition of an input method where it is being typed.
pub(crate) fn displayed_value(dom: &Dom, id: usize) -> String {
    let value = field_value(dom, id);
    match &dom.ui.preedit {
        Some((text, _)) if dom.ui.focused == Some(id) && !text.is_empty() => {
            let at = dom.ui.caret.min(dom.ui.anchor).min(value.chars().count());
            let mut out: String = value.chars().take(at).collect();
            out.push_str(text);
            out.extend(value.chars().skip(at));
            out
        }
        _ => value,
    }
}

/// Whether `id` takes typed text.
pub(crate) fn is_text_field(dom: &Dom, id: usize) -> bool {
    match dom.tag(id) {
        Some("textarea") => true,
        Some("input") => matches!(
            dom.attr(id, "type").unwrap_or("text"),
            "text" | "email" | "password" | "search" | "tel" | "url" | "number"
        ),
        _ => false,
    }
}

pub(crate) fn is_range(dom: &Dom, id: usize) -> bool {
    dom.tag(id) == Some("input") && dom.attr(id, "type") == Some("range")
}

pub(crate) fn focusable(dom: &Dom, id: usize) -> bool {
    is_focusable(dom, id)
}

fn is_focusable(dom: &Dom, id: usize) -> bool {
    let Some(tag) = dom.tag(id) else { return false };
    if dom.attr(id, "disabled").is_some() {
        return false;
    }
    if let Some(t) = dom.attr(id, "tabindex") {
        return t.parse::<i32>().is_ok_and(|n| n >= 0);
    }
    match tag {
        "button" | "select" | "textarea" => true,
        "input" => dom.attr(id, "type") != Some("hidden"),
        "a" => dom.attr(id, "href").is_some(),
        _ => false,
    }
}

fn focusable_ancestor(dom: &Dom, id: usize) -> Option<usize> {
    let mut at = Some(id);
    while let Some(n) = at {
        if is_focusable(dom, n) {
            return Some(n);
        }
        at = dom.nodes[n].parent;
    }
    None
}

fn collect_focusable(dom: &Dom, id: usize, out: &mut Vec<usize>) {
    if dom.tag(id).is_none() {
        return;
    }
    let visible = dom.layout.as_ref().is_none_or(|l| l.find(id).is_some());
    if !visible {
        return;
    }
    if is_focusable(dom, id) {
        out.push(id);
    }
    for c in &dom.nodes[id].children {
        collect_focusable(dom, *c, out);
    }
}

fn common_ancestor(dom: &Dom, a: usize, b: usize) -> Option<usize> {
    let mut up = vec![a];
    let mut at = a;
    while let Some(p) = dom.nodes[at].parent {
        up.push(p);
        at = p;
    }
    let mut at = Some(b);
    while let Some(n) = at {
        if up.contains(&n) {
            return Some(n);
        }
        at = dom.nodes[n].parent;
    }
    None
}

/// The checkbox or radio button a click on `id` (or on something in it) toggles.
fn control_of(dom: &Dom, id: usize) -> Option<usize> {
    let mut at = Some(id);
    while let Some(n) = at {
        if dom.tag(n) == Some("input")
            && matches!(dom.attr(n, "type"), Some("checkbox" | "radio"))
            && dom.attr(n, "disabled").is_none()
        {
            return Some(n);
        }
        at = dom.nodes[n].parent;
    }
    None
}

/// The control a `<label>` around `id` stands for, when the click was not already on it.
fn label_target(dom: &Dom, id: usize) -> Option<usize> {
    let mut at = Some(id);
    let mut label = None;
    while let Some(n) = at {
        if dom.tag(n) == Some("label") {
            label = Some(n);
            break;
        }
        if is_focusable(dom, n) {
            return None;
        }
        at = dom.nodes[n].parent;
    }
    let label = label?;
    if let Some(target) = dom.attr(label, "for") {
        return (0..dom.nodes.len()).find(|n| dom.attr(*n, "id") == Some(target));
    }
    let mut found = None;
    let mut stack = vec![label];
    while let Some(n) = stack.pop() {
        if n != label && is_focusable(dom, n) {
            found = Some(n);
        }
        stack.extend(dom.nodes[n].children.iter().rev());
    }
    // Document order: the first one wins.
    let mut first = None;
    let mut stack = vec![label];
    while let Some(n) = stack.pop() {
        if n != label && is_focusable(dom, n) && first.is_none() {
            first = Some(n);
        }
        stack.extend(dom.nodes[n].children.iter().rev());
    }
    first.or(found)
}

/// The character index in `value` nearest to window x position `x` in field `f`.
fn caret_index(dom: &Dom, root: &LayoutBox, f: usize, value: &str, x: f32) -> usize {
    let frags = root.text_fragments(f);
    let Some((fx, _, frag)) = frags.first() else {
        return 0;
    };
    let shown = &frag.text;
    let mut best = (f32::MAX, 0);
    let chars: Vec<char> = shown.chars().collect();
    for i in 0..=chars.len() {
        let prefix: String = chars[..i].iter().collect();
        let w = prefix_width(dom, &frag.style, &prefix);
        let d = (fx + w - x).abs();
        if d < best.0 {
            best = (d, i);
        }
    }
    best.1.min(value.chars().count())
}

/// The width of `text` as the start of a longer line (a trailing space counts).
pub(crate) fn prefix_width(dom: &Dom, style: &quench_css::ComputedStyle, text: &str) -> f32 {
    dom.fonts.measure(style, &format!("{text}|")) - dom.fonts.measure(style, "|")
}

/// The deepest element at `(x, y)` in the last layout.
fn hit(dom: &Dom, x: f32, y: f32) -> Option<usize> {
    let root = dom.layout.as_ref()?;
    hit_box(dom, root, root.x, root.y, (0.0, 0.0), x, y)
}

fn hit_box(
    dom: &Dom,
    b: &LayoutBox,
    x: f32,
    y: f32,
    scrolled: (f32, f32),
    px: f32,
    py: f32,
) -> Option<usize> {
    let t = &b.style.transform.translate;
    let (x, y) = (x + t.0.resolve(b.width), y + t.1.resolve(b.height));
    let inside =
        |rx: f32, ry: f32, w: f32, h: f32| px >= rx && px < rx + w && py >= ry && py < ry + h;
    let clips = b.style.overflow != (quench_css::Overflow::Visible, quench_css::Overflow::Visible);
    if clips {
        let (cx, cy, cw, ch) = b.padding_rect();
        if !inside(x + cx, y + cy, cw, ch) {
            return None;
        }
    }
    let (sx, sy) = if clips || b.key == 0 {
        dom.ui.scroll.get(&b.key).copied().unwrap_or((0.0, 0.0))
    } else {
        (0.0, 0.0)
    };
    let (ox, oy) = (x - sx, y - sy);
    let scrolled = (scrolled.0 + sx, scrolled.1 + sy);
    let layers = Layers::of(b);
    let try_children = |list: &[&LayoutBox]| {
        list.iter().rev().find_map(|c| {
            let (tx, ty) = if c.style.position == quench_css::Position::Fixed {
                scrolled
            } else {
                (0.0, 0.0)
            };
            hit_box(dom, c, ox + c.x + tx, oy + c.y + ty, scrolled, px, py)
        })
    };
    if let Some(h) = try_children(&layers.above) {
        return Some(h);
    }
    if let Some(h) = try_children(&layers.inline) {
        return Some(h);
    }
    for f in b.fragments.iter().rev() {
        match f {
            Fragment::Text(t) => {
                let m = dom.fonts.metrics(&t.style);
                if t.key != usize::MAX
                    && inside(
                        ox + t.x,
                        oy + t.baseline - m.ascent,
                        t.width,
                        m.ascent + m.descent,
                    )
                {
                    // A text node belongs to its element; a field's own text (key of the field) is the field.
                    return match dom.nodes.get(t.key) {
                        Some(n) if matches!(n.kind, Kind::Text(_)) => n.parent,
                        _ => Some(t.key),
                    };
                }
            }
            Fragment::Inline(i) => {
                if inside(ox + i.x, oy + i.y, i.width, i.height) {
                    return Some(i.key);
                }
            }
        }
    }
    if let Some(h) = try_children(&layers.blocks) {
        return Some(h);
    }
    if let Some(h) = try_children(&layers.below) {
        return Some(h);
    }
    (b.key != usize::MAX && inside(x, y, b.width, b.height)).then_some(b.key)
}
