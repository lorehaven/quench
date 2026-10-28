//! The accessibility tree: what a screen reader or another assistive technology is told about the page.
//!
//! [`NativeRenderer::accessibility_tree`] reads the DOM and the last layout and returns a tree of
//! [`AccessNode`]s with a role, an accessible name, states, the actions that can be taken and the rectangle on
//! the screen. Elements that mean nothing to a person using the page (a `div` that only groups) are left out and
//! their children are lifted into the parent. Both platform bridges read this one tree: AT-SPI on Linux and the
//! Android accessibility node provider.

use quench_css::Display;
use quench_layout::LayoutBox;

use crate::dom::{Dom, Kind, NativeRenderer};
use crate::input::{field_value, is_text_field};

/// What a node is, in the terms assistive technology shares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    /// The whole page.
    Document,
    Heading,
    Paragraph,
    /// A piece of text with no other role.
    Text,
    Button,
    Link,
    Checkbox,
    Radio,
    Switch,
    TextBox,
    List,
    ListItem,
    Tab,
    TabList,
    TabPanel,
    Dialog,
    Navigation,
    Status,
    Alert,
    Image,
    Form,
    Group,
}

/// An action a user of assistive technology can ask for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccessAction {
    /// Press, check or follow.
    Click,
    Focus,
    /// Scroll the node into view.
    ScrollIntoView,
}

/// A node of the accessibility tree.
#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::struct_excessive_bools)] // the states assistive technology asks about
pub struct AccessNode {
    /// The DOM node it stands for (`NodeId::index`); an action on it is delivered to that element.
    pub id: usize,
    pub role: Role,
    /// `<h1>` to `<h6>`: the level; 0 otherwise.
    pub level: u8,
    /// The accessible name: what is read out for it.
    pub name: String,
    /// More than the name: a hint, an error message.
    pub description: String,
    /// The text of a field, or the position of a slider.
    pub value: Option<String>,
    /// The rectangle on the screen: `(x, y, width, height)`.
    pub bounds: (f32, f32, f32, f32),
    pub focusable: bool,
    pub focused: bool,
    pub disabled: bool,
    /// `Some` for checkboxes, radio buttons and switches.
    pub checked: Option<bool>,
    /// `Some` for toggle buttons (`aria-pressed`) and tabs (`aria-selected`).
    pub selected: Option<bool>,
    /// `Some` for what opens and closes (`aria-expanded`).
    pub expanded: Option<bool>,
    pub required: bool,
    pub read_only: bool,
    /// For a text field: the caret and the other end of the selection, in characters.
    pub caret: Option<(usize, usize)>,
    pub actions: Vec<AccessAction>,
    pub children: Vec<Self>,
}

impl AccessNode {
    /// The node with this DOM id, searching depth first.
    #[must_use]
    pub fn find(&self, id: usize) -> Option<&Self> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|c| c.find(id))
    }

    /// How many nodes there are in the tree.
    #[must_use]
    pub fn count(&self) -> usize {
        1 + self.children.iter().map(Self::count).sum::<usize>()
    }
}

impl NativeRenderer {
    /// The accessibility tree of the page as it is now (laid out for the current viewport).
    #[must_use]
    pub fn accessibility_tree(&self) -> AccessNode {
        if self.dom.borrow().layout.is_none() || self.dom.borrow().ui.dirty {
            let _ = self.layout();
        }
        let dom = self.dom.borrow();
        let boxes = dom.layout.clone().expect("laid out");
        let rects = screen_rects(&dom, &boxes);
        let mut kids = Vec::new();
        collect(&dom, &rects, 1, &mut kids);
        let title = dom.attr(1, "aria-label").unwrap_or("").to_string();
        AccessNode {
            id: 0,
            role: Role::Document,
            level: 0,
            name: title,
            description: String::new(),
            value: None,
            bounds: (0.0, 0.0, dom.viewport.0, dom.viewport.1),
            focusable: false,
            focused: false,
            disabled: false,
            checked: None,
            selected: None,
            expanded: None,
            required: false,
            read_only: false,
            caret: None,
            actions: Vec::new(),
            children: kids,
        }
    }

    /// Does what an assistive technology asked of the node `id`. Returns whether there was anything to do.
    pub fn perform(&self, id: usize, action: AccessAction) -> bool {
        let center = {
            let dom = self.dom.borrow();
            let Some(boxes) = dom.layout.clone() else {
                return false;
            };
            if dom.tag(id).is_none() {
                return false;
            }
            let rects = screen_rects(&dom, &boxes);
            rects.get(&id).copied()
        };
        match action {
            AccessAction::Focus => {
                self.focus_node(id, true);
                true
            }
            AccessAction::Click => {
                let Some((x, y, w, h)) = center else {
                    return false;
                };
                self.pointer_down(x + w / 2.0, y + h / 2.0);
                self.pointer_up(x + w / 2.0, y + h / 2.0);
                true
            }
            AccessAction::ScrollIntoView => {
                let Some((_, y, _, h)) = center else {
                    return false;
                };
                let viewport = self.dom.borrow().viewport.1;
                if y < 0.0 {
                    self.wheel(0.0, 0.0, 0.0, y);
                } else if y + h > viewport {
                    self.wheel(0.0, 0.0, 0.0, y + h - viewport);
                }
                true
            }
        }
    }
}

/// Where each box is on the screen (scrolling and `translate()` applied, fixed boxes where they are fixed).
fn screen_rects(
    dom: &Dom,
    root: &LayoutBox,
) -> std::collections::HashMap<usize, (f32, f32, f32, f32)> {
    fn walk(
        dom: &Dom,
        b: &LayoutBox,
        x: f32,
        y: f32,
        scrolled: (f32, f32),
        out: &mut std::collections::HashMap<usize, (f32, f32, f32, f32)>,
    ) {
        let t = &b.style.transform.translate;
        let (x, y) = (x + t.0.resolve(b.width), y + t.1.resolve(b.height));
        if b.key != usize::MAX {
            out.insert(b.key, (x, y, b.width, b.height));
        }
        let clips =
            b.style.overflow != (quench_css::Overflow::Visible, quench_css::Overflow::Visible);
        let (sx, sy) = if clips || b.key == 0 {
            dom.ui.scroll.get(&b.key).copied().unwrap_or((0.0, 0.0))
        } else {
            (0.0, 0.0)
        };
        let scrolled = (scrolled.0 + sx, scrolled.1 + sy);
        for c in &b.children {
            let fixed = c.style.position == quench_css::Position::Fixed;
            let (fx, fy) = if fixed { scrolled } else { (0.0, 0.0) };
            walk(dom, c, x - sx + c.x + fx, y - sy + c.y + fy, scrolled, out);
        }
    }
    let mut out = std::collections::HashMap::new();
    walk(dom, root, root.x, root.y, (0.0, 0.0), &mut out);
    out
}

fn attr<'a>(dom: &'a Dom, id: usize, name: &str) -> Option<&'a str> {
    dom.attr(id, name)
}

fn flag(dom: &Dom, id: usize, name: &str) -> Option<bool> {
    attr(dom, id, name).map(|v| v != "false")
}

fn hidden(dom: &Dom, id: usize) -> bool {
    attr(dom, id, "aria-hidden") == Some("true")
        || attr(dom, id, "hidden").is_some()
        || dom
            .layout
            .as_ref()
            .is_some_and(|l| l.find(id).is_none() && dom.tag(id).is_some() && !inline_like(dom, id))
        || dom
            .layout
            .as_ref()
            .and_then(|l| l.find(id))
            .is_some_and(|b| b.style.display == Display::None || !b.style.visible)
}

/// Inline elements have no box of their own in the layout; they are not hidden for that.
fn inline_like(dom: &Dom, id: usize) -> bool {
    matches!(
        dom.tag(id),
        Some(
            "span"
                | "a"
                | "b"
                | "i"
                | "em"
                | "strong"
                | "small"
                | "code"
                | "label"
                | "br"
                | "abbr"
                | "mark"
                | "u"
                | "s"
                | "sub"
                | "sup"
        )
    )
}

fn role_of(dom: &Dom, id: usize) -> Option<(Role, u8)> {
    let tag = dom.tag(id)?;
    if let Some(role) = attr(dom, id, "role") {
        let r = match role {
            "button" => Role::Button,
            "link" => Role::Link,
            "checkbox" => Role::Checkbox,
            "radio" => Role::Radio,
            "switch" => Role::Switch,
            "textbox" | "searchbox" => Role::TextBox,
            "tab" => Role::Tab,
            "tablist" => Role::TabList,
            "tabpanel" => Role::TabPanel,
            "dialog" | "alertdialog" => Role::Dialog,
            "navigation" => Role::Navigation,
            "status" => Role::Status,
            "alert" => Role::Alert,
            "list" => Role::List,
            "listitem" => Role::ListItem,
            "img" | "image" => Role::Image,
            "heading" => {
                return Some((
                    Role::Heading,
                    attr(dom, id, "aria-level")
                        .and_then(|l| l.parse().ok())
                        .unwrap_or(2),
                ));
            }
            "presentation" | "none" => return None,
            _ => Role::Group,
        };
        return Some((r, 0));
    }
    Some(match tag {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => (Role::Heading, tag.as_bytes()[1] - b'0'),
        "p" => (Role::Paragraph, 0),
        "button" => (Role::Button, 0),
        "a" if attr(dom, id, "href").is_some() => (Role::Link, 0),
        "input" => match attr(dom, id, "type").unwrap_or("text") {
            "checkbox" => (Role::Checkbox, 0),
            "radio" => (Role::Radio, 0),
            "button" | "submit" | "reset" => (Role::Button, 0),
            "hidden" => return None,
            _ => (Role::TextBox, 0),
        },
        "textarea" | "select" => (Role::TextBox, 0),
        "ul" | "ol" => (Role::List, 0),
        "li" => (Role::ListItem, 0),
        "nav" => (Role::Navigation, 0),
        "form" => (Role::Form, 0),
        "img" => (Role::Image, 0),
        "dialog" => (Role::Dialog, 0),
        _ => return None,
    })
}

/// The text under `id`, as a person would read it (hidden parts left out).
fn text_content(dom: &Dom, id: usize) -> String {
    let mut parts: Vec<String> = Vec::new();
    fn walk(dom: &Dom, id: usize, parts: &mut Vec<String>) {
        for c in &dom.nodes[id].children {
            match &dom.nodes[*c].kind {
                Kind::Text(t) => {
                    let t = t.split_whitespace().collect::<Vec<_>>().join(" ");
                    if !t.is_empty() {
                        parts.push(t);
                    }
                }
                Kind::Element { .. } if !hidden(dom, *c) && dom.tag(*c) != Some("input") => {
                    walk(dom, *c, parts);
                }
                _ => {}
            }
        }
    }
    walk(dom, id, &mut parts);
    parts.join(" ")
}

/// The accessible name: `aria-label`, `aria-labelledby`, an associated label, the content, the `title`, the
/// placeholder.
fn name_of(dom: &Dom, id: usize, role: Role) -> String {
    if let Some(label) = attr(dom, id, "aria-label").filter(|l| !l.is_empty()) {
        return label.to_string();
    }
    if let Some(ids) = attr(dom, id, "aria-labelledby") {
        let text: Vec<String> = ids
            .split_whitespace()
            .filter_map(|target| (0..dom.nodes.len()).find(|n| attr(dom, *n, "id") == Some(target)))
            .map(|n| text_content(dom, n))
            .filter(|t| !t.is_empty())
            .collect();
        if !text.is_empty() {
            return text.join(" ");
        }
    }
    if (matches!(role, Role::TextBox | Role::Checkbox | Role::Radio)
        || dom.tag(id) == Some("input"))
        && let Some(label) = label_for(dom, id)
    {
        return label;
    }
    let from_content = matches!(
        role,
        Role::Button
            | Role::Link
            | Role::Heading
            | Role::Tab
            | Role::Switch
            | Role::ListItem
            | Role::Checkbox
            | Role::Radio
    );
    if from_content {
        let text = text_content(dom, id);
        if !text.is_empty() {
            return text;
        }
    }
    if let Some(title) = attr(dom, id, "title").filter(|t| !t.is_empty()) {
        return title.to_string();
    }
    if let Some(alt) = attr(dom, id, "alt").filter(|t| !t.is_empty()) {
        return alt.to_string();
    }
    if let Some(p) = attr(dom, id, "placeholder").filter(|p| !p.is_empty()) {
        return p.to_string();
    }
    if dom.tag(id) == Some("input")
        && matches!(attr(dom, id, "type"), Some("button" | "submit" | "reset"))
    {
        return attr(dom, id, "value").unwrap_or("").to_string();
    }
    String::new()
}

/// The text of the `<label>` of a control: one that wraps it, or one whose `for` names it.
fn label_for(dom: &Dom, id: usize) -> Option<String> {
    let mut at = dom.nodes[id].parent;
    while let Some(n) = at {
        if dom.tag(n) == Some("label") {
            let text = text_content(dom, n);
            return (!text.is_empty()).then_some(text);
        }
        at = dom.nodes[n].parent;
    }
    let own = attr(dom, id, "id")?;
    (0..dom.nodes.len())
        .find(|n| dom.tag(*n) == Some("label") && attr(dom, *n, "for") == Some(own))
        .map(|n| text_content(dom, n))
        .filter(|t| !t.is_empty())
}

fn describe(dom: &Dom, id: usize) -> String {
    let Some(ids) = attr(dom, id, "aria-describedby") else {
        return attr(dom, id, "aria-description").unwrap_or("").to_string();
    };
    ids.split_whitespace()
        .filter_map(|target| (0..dom.nodes.len()).find(|n| attr(dom, *n, "id") == Some(target)))
        .map(|n| text_content(dom, n))
        .filter(|t| !t.is_empty())
        .collect::<Vec<_>>()
        .join(" ")
}

fn collect(
    dom: &Dom,
    rects: &std::collections::HashMap<usize, (f32, f32, f32, f32)>,
    id: usize,
    out: &mut Vec<AccessNode>,
) {
    match &dom.nodes[id].kind {
        Kind::Anchor => {}
        Kind::Text(t) => {
            // Text of a parent that does not give its content as its name (a paragraph, a group) is a node of its own.
            let text = t.split_whitespace().collect::<Vec<_>>().join(" ");
            let parent_names_itself = dom.nodes[id]
                .parent
                .is_some_and(|p| role_of(dom, p).is_some_and(|(r, _)| names_from_content(r)));
            if !text.is_empty() && !parent_names_itself {
                let parent = dom.nodes[id].parent.unwrap_or(1);
                let bounds = rects.get(&parent).copied().unwrap_or((0.0, 0.0, 0.0, 0.0));
                out.push(AccessNode {
                    name: text,
                    bounds,
                    ..leaf(id, Role::Text)
                });
            }
        }
        Kind::Element { .. } => {
            if hidden(dom, id) {
                return;
            }
            let Some((role, level)) = role_of(dom, id) else {
                for c in &dom.nodes[id].children {
                    collect(dom, rects, *c, out);
                }
                return;
            };
            let field = is_text_field(dom, id);
            let mut node = leaf(id, role);
            node.level = level;
            node.name = name_of(dom, id, role);
            node.description = describe(dom, id);
            node.bounds = rects.get(&id).copied().unwrap_or((0.0, 0.0, 0.0, 0.0));
            node.disabled = attr(dom, id, "disabled").is_some()
                || attr(dom, id, "aria-disabled") == Some("true");
            node.focusable = !node.disabled && crate::input::focusable(dom, id);
            node.focused = dom.ui.focused == Some(id);
            node.required = attr(dom, id, "required").is_some()
                || attr(dom, id, "aria-required") == Some("true");
            node.read_only = attr(dom, id, "readonly").is_some();
            node.checked = match role {
                Role::Checkbox | Role::Radio => Some(
                    attr(dom, id, "checked").is_some()
                        || attr(dom, id, "aria-checked") == Some("true"),
                ),
                Role::Switch => Some(attr(dom, id, "aria-checked") == Some("true")),
                _ => None,
            };
            node.selected =
                flag(dom, id, "aria-pressed").or_else(|| flag(dom, id, "aria-selected"));
            node.expanded = flag(dom, id, "aria-expanded");
            if field {
                node.value = Some(field_value(dom, id));
                if node.focused {
                    node.caret = Some((dom.ui.caret, dom.ui.anchor));
                }
            }
            if !node.disabled {
                if matches!(
                    role,
                    Role::Button
                        | Role::Link
                        | Role::Checkbox
                        | Role::Radio
                        | Role::Switch
                        | Role::Tab
                ) {
                    node.actions.push(AccessAction::Click);
                }
                if node.focusable {
                    node.actions.push(AccessAction::Focus);
                }
            }
            node.actions.push(AccessAction::ScrollIntoView);
            // What names itself from its content has no children of its own (a button's text is its name).
            if names_from_content(role) {
                for c in &dom.nodes[id].children {
                    if matches!(dom.nodes[*c].kind, Kind::Element { .. }) {
                        collect_interactive(dom, rects, *c, &mut node.children);
                    }
                }
            } else {
                for c in &dom.nodes[id].children {
                    collect(dom, rects, *c, &mut node.children);
                }
            }
            out.push(node);
        }
    }
}

/// Of the content of something that names itself from it, only what is itself interactive stays a node.
fn collect_interactive(
    dom: &Dom,
    rects: &std::collections::HashMap<usize, (f32, f32, f32, f32)>,
    id: usize,
    out: &mut Vec<AccessNode>,
) {
    if hidden(dom, id) {
        return;
    }
    match role_of(dom, id) {
        Some((r, _))
            if matches!(
                r,
                Role::Button
                    | Role::Link
                    | Role::Checkbox
                    | Role::Radio
                    | Role::Switch
                    | Role::TextBox
            ) =>
        {
            collect(dom, rects, id, out);
        }
        _ => {
            for c in &dom.nodes[id].children {
                if matches!(dom.nodes[*c].kind, Kind::Element { .. }) {
                    collect_interactive(dom, rects, *c, out);
                }
            }
        }
    }
}

const fn names_from_content(role: Role) -> bool {
    matches!(
        role,
        Role::Button
            | Role::Link
            | Role::Heading
            | Role::Tab
            | Role::Switch
            | Role::Checkbox
            | Role::Radio
    )
}

const fn leaf(id: usize, role: Role) -> AccessNode {
    AccessNode {
        id,
        role,
        level: 0,
        name: String::new(),
        description: String::new(),
        value: None,
        bounds: (0.0, 0.0, 0.0, 0.0),
        focusable: false,
        focused: false,
        disabled: false,
        checked: None,
        selected: None,
        expanded: None,
        required: false,
        read_only: false,
        caret: None,
        actions: Vec::new(),
        children: Vec::new(),
    }
}
