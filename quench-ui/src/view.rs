//! The view tree and the builders that make it.
//!
//! A [`View`] describes UI; nothing exists until [`crate::mount`] builds it into a renderer.
//! Static parts are built once. The reactive parts (text, attributes, classes, [`View::dynamic`],
//! [`View::keyed`]) are closures that mounting wraps in effects, so a signal change patches just
//! the node that read it.

use std::fmt;
use std::rc::Rc;

use crate::event::{Event, EventKind, Handler, KeyEvent};
use crate::html::is_valid_tag;
use crate::renderer::Prevent;

/// What identifies an item in a keyed list.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    Int(i64),
    Str(String),
}

macro_rules! key_from_int {
    ($($t:ty),*) => {$(
        impl From<$t> for Key {
            fn from(v: $t) -> Self {
                Self::Int(i64::from(v))
            }
        }
    )*};
}
key_from_int!(i8, i16, i32, i64, u8, u16, u32);

impl From<u64> for Key {
    fn from(v: u64) -> Self {
        i64::try_from(v).map_or_else(|_| Self::Str(v.to_string()), Self::Int)
    }
}

impl From<usize> for Key {
    fn from(v: usize) -> Self {
        Self::from(v as u64)
    }
}

impl From<String> for Key {
    fn from(v: String) -> Self {
        Self::Str(v)
    }
}

impl From<&str> for Key {
    fn from(v: &str) -> Self {
        Self::Str(v.to_string())
    }
}

/// One keyed item: its key and how to build its view, called only if the key is new.
pub(crate) type KeyedItem = (Key, Box<dyn FnOnce() -> View>);

/// A description of UI.
pub enum View {
    /// Nothing.
    Empty,
    Text(String),
    /// Text that follows a closure.
    DynText(Rc<dyn Fn() -> String>),
    Element(Box<Element>),
    Fragment(Vec<Self>),
    /// A region whose content is rebuilt whenever what the closure read changes.
    Dynamic(Rc<dyn Fn() -> Self>),
    /// A list reconciled by key: items that keep their key keep their nodes and state.
    Keyed(Rc<dyn Fn() -> Vec<KeyedItem>>),
}

impl fmt::Debug for View {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("Empty"),
            Self::Text(t) => f.debug_tuple("Text").field(t).finish(),
            Self::DynText(_) => f.write_str("DynText(..)"),
            Self::Element(e) => e.fmt(f),
            Self::Fragment(v) => f.debug_tuple("Fragment").field(v).finish(),
            Self::Dynamic(_) => f.write_str("Dynamic(..)"),
            Self::Keyed(_) => f.write_str("Keyed(..)"),
        }
    }
}

impl View {
    /// Static text.
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self::Text(text.into())
    }

    /// Text recomputed whenever the signals `f` reads change.
    #[must_use]
    pub fn text_fn(f: impl Fn() -> String + 'static) -> Self {
        Self::DynText(Rc::new(f))
    }

    /// A region rebuilt whenever the signals `f` reads change.
    #[must_use]
    pub fn dynamic(f: impl Fn() -> Self + 'static) -> Self {
        Self::Dynamic(Rc::new(f))
    }

    /// A view built when it is mounted rather than now, so that what it creates (signals that
    /// need cleanup, effects, timers) belongs to the mount's scope. A component that is called
    /// before `mount` and sets up effects should return its view through this.
    #[must_use]
    pub fn lazy(build: impl FnOnce() -> Self + 'static) -> Self {
        let pending = std::cell::Cell::new(Some(build));
        Self::dynamic(move || {
            pending
                .take()
                .map_or(Self::Empty, |build| crate::reactive::untrack(build))
        })
    }

    /// `view` while `when` is true, nothing otherwise.
    #[must_use]
    pub fn show(when: impl Fn() -> bool + 'static, view: impl Fn() -> Self + 'static) -> Self {
        Self::dynamic(move || if when() { view() } else { Self::Empty })
    }

    /// A list of `render(item)` views, one per item `items()` returns, matched across updates by
    /// `key(&item)`. An item whose key is still present is not rebuilt, even if the item differs:
    /// carry changing data in signals.
    ///
    /// # Panics
    /// When `items()` returns two items with the same key.
    #[must_use]
    pub fn keyed<T, K>(
        items: impl Fn() -> Vec<T> + 'static,
        key: impl Fn(&T) -> K + 'static,
        render: impl Fn(T) -> Self + 'static,
    ) -> Self
    where
        T: 'static,
        K: Into<Key>,
    {
        let render = Rc::new(render);
        Self::Keyed(Rc::new(move || {
            items()
                .into_iter()
                .map(|item| {
                    let key = key(&item).into();
                    let render = Rc::clone(&render);
                    let make: Box<dyn FnOnce() -> Self> = Box::new(move || render(item));
                    (key, make)
                })
                .collect()
        }))
    }
}

/// Anything that can become a [`View`].
pub trait IntoView {
    fn into_view(self) -> View;
}

impl IntoView for View {
    fn into_view(self) -> View {
        self
    }
}

impl IntoView for Element {
    fn into_view(self) -> View {
        View::Element(Box::new(self))
    }
}

impl IntoView for String {
    fn into_view(self) -> View {
        View::Text(self)
    }
}

impl IntoView for &str {
    fn into_view(self) -> View {
        View::Text(self.to_string())
    }
}

macro_rules! into_view_display {
    ($($t:ty),*) => {$(
        impl IntoView for $t {
            fn into_view(self) -> View {
                View::Text(self.to_string())
            }
        }
    )*};
}
into_view_display!(
    i8, i16, i32, i64, isize, u8, u16, u32, u64, usize, f32, f64, bool, char
);

impl<V: IntoView> IntoView for Vec<V> {
    fn into_view(self) -> View {
        View::Fragment(self.into_iter().map(IntoView::into_view).collect())
    }
}

impl<V: IntoView> IntoView for Option<V> {
    fn into_view(self) -> View {
        self.map_or(View::Empty, IntoView::into_view)
    }
}

impl IntoView for () {
    fn into_view(self) -> View {
        View::Empty
    }
}

/// Joins views into one fragment.
#[must_use]
pub fn fragment(children: impl IntoIterator<Item = View>) -> View {
    View::Fragment(children.into_iter().collect())
}

/// An attribute value.
pub(crate) enum Attr {
    Static(String),
    /// `None` removes the attribute.
    Dynamic(Rc<dyn Fn() -> Option<String>>),
}

/// A class name, always on or following a condition.
pub(crate) struct Class {
    pub(crate) name: String,
    pub(crate) when: Option<Rc<dyn Fn() -> bool>>,
}

/// An element and everything attached to it.
pub struct Element {
    pub(crate) tag: String,
    pub(crate) attrs: Vec<(String, Attr)>,
    pub(crate) classes: Vec<Class>,
    pub(crate) styles: Vec<(String, String)>,
    pub(crate) css: Vec<(String, Rc<dyn Fn() -> String>)>,
    pub(crate) children: Vec<View>,
    pub(crate) handlers: Vec<(EventKind, Handler)>,
    pub(crate) prevent: Vec<(EventKind, Prevent)>,
    pub(crate) focus: Focus,
    pub(crate) focus_if: Vec<Rc<dyn Fn() -> bool>>,
}

/// What an element does with keyboard focus when it is built.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub(crate) enum Focus {
    #[default]
    Leave,
    Take,
    Trap,
}

impl fmt::Debug for Element {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Element")
            .field("tag", &self.tag)
            .field("attrs", &self.attrs.len())
            .field("classes", &self.classes.len())
            .field("children", &self.children)
            .finish_non_exhaustive()
    }
}

/// Starts an element.
///
/// # Panics
/// If `tag` is not a plain tag name (letters, digits and `-`, starting with a letter).
#[must_use]
pub fn element(tag: &str) -> Element {
    assert!(is_valid_tag(tag), "invalid tag name: {tag:?}");
    Element {
        tag: tag.to_ascii_lowercase(),
        attrs: Vec::new(),
        classes: Vec::new(),
        styles: Vec::new(),
        css: Vec::new(),
        children: Vec::new(),
        handlers: Vec::new(),
        prevent: Vec::new(),
        focus: Focus::Leave,
        focus_if: Vec::new(),
    }
}

impl Element {
    /// Sets an attribute. A later call with the same name replaces an earlier one. Names that
    /// start with `on` and names with unusual characters are dropped when mounted, and URL
    /// attributes holding `javascript:` are blocked.
    #[must_use]
    pub fn attr(mut self, name: &str, value: impl Into<String>) -> Self {
        self.set_attr(name, Attr::Static(value.into()));
        self
    }

    /// An attribute that follows a closure.
    #[must_use]
    pub fn attr_fn(mut self, name: &str, value: impl Fn() -> String + 'static) -> Self {
        self.set_attr(name, Attr::Dynamic(Rc::new(move || Some(value()))));
        self
    }

    /// An attribute that follows a closure and is absent while it returns `None`.
    #[must_use]
    pub fn attr_opt(mut self, name: &str, value: impl Fn() -> Option<String> + 'static) -> Self {
        self.set_attr(name, Attr::Dynamic(Rc::new(value)));
        self
    }

    /// A boolean attribute, present (empty) while the closure returns true.
    #[must_use]
    pub fn attr_bool(self, name: &str, value: impl Fn() -> bool + 'static) -> Self {
        self.attr_opt(name, move || value().then(String::new))
    }

    fn set_attr(&mut self, name: &str, value: Attr) {
        let name = name.to_ascii_lowercase();
        if let Some(slot) = self.attrs.iter_mut().find(|(n, _)| *n == name) {
            slot.1 = value;
        } else {
            self.attrs.push((name, value));
        }
    }

    /// Adds a class.
    #[must_use]
    pub fn class(mut self, name: impl Into<String>) -> Self {
        self.classes.push(Class {
            name: name.into(),
            when: None,
        });
        self
    }

    /// Adds a class that is on while `when` returns true.
    #[must_use]
    pub fn class_if(mut self, name: impl Into<String>, when: impl Fn() -> bool + 'static) -> Self {
        self.classes.push(Class {
            name: name.into(),
            when: Some(Rc::new(when)),
        });
        self
    }

    /// Sets an inline style property.
    #[must_use]
    pub fn style(mut self, property: &str, value: impl Into<String>) -> Self {
        let value = value.into();
        if let Some(slot) = self.styles.iter_mut().find(|(p, _)| p == property) {
            slot.1 = value;
        } else {
            self.styles.push((property.to_string(), value));
        }
        self
    }

    /// Sets a CSS property from a function of signals, through the style object. Unlike
    /// [`Element::style`] it is not an inline `style` attribute, so a policy that forbids inline
    /// styles (the one `quench-fullstack` sends) allows it. It is applied in the browser only: a
    /// server render leaves the property out, so use it for what depends on layout.
    #[must_use]
    pub fn css(mut self, property: &str, value: impl Fn() -> String + 'static) -> Self {
        self.css.push((property.to_string(), Rc::new(value)));
        self
    }

    /// Appends a child.
    #[must_use]
    pub fn child(mut self, child: impl IntoView) -> Self {
        self.children.push(child.into_view());
        self
    }

    /// Appends several children.
    #[must_use]
    pub fn children(mut self, children: impl IntoIterator<Item = View>) -> Self {
        self.children.extend(children);
        self
    }

    /// Appends static text.
    #[must_use]
    pub fn text(self, text: impl Into<String>) -> Self {
        self.child(View::Text(text.into()))
    }

    /// Appends text that follows a closure.
    #[must_use]
    pub fn text_fn(self, f: impl Fn() -> String + 'static) -> Self {
        self.child(View::text_fn(f))
    }

    /// Listens for `kind`.
    #[must_use]
    pub fn on(mut self, kind: EventKind, handler: impl Fn(&Event) + 'static) -> Self {
        self.handlers.push((kind, Rc::new(handler)));
        self
    }

    /// Stops the browser's default action for `kind` on this element (a form's own submit is always
    /// stopped; this is for the rest, such as a `keydown` you handle yourself).
    #[must_use]
    pub fn prevent_default(mut self, kind: EventKind) -> Self {
        self.prevent.push((kind, Prevent::Always));
        self
    }

    /// Makes this element's `click` listener a navigation: it runs for a plain left click, whose
    /// default (following the link) is stopped, and not for ctrl-, shift- or middle-clicks, which
    /// the browser then handles itself (open in a new tab).
    #[must_use]
    pub fn intercept_click(mut self) -> Self {
        self.prevent.push((EventKind::Click, Prevent::PlainClick));
        self
    }

    /// Takes keyboard focus each time `when` becomes true (and when it is built, if it is true
    /// then). For moving focus to a part of a widget the keyboard just selected.
    #[must_use]
    pub fn focus_if(mut self, when: impl Fn() -> bool + 'static) -> Self {
        self.focus_if.push(Rc::new(when));
        self
    }

    /// Takes keyboard focus when it is built.
    #[must_use]
    pub const fn focus_on_mount(mut self) -> Self {
        self.focus = Focus::Take;
        self
    }

    /// Takes keyboard focus when it is built and keeps it inside while it exists: Tab and Shift+Tab
    /// cycle through its contents. For modal dialogs and sheets.
    #[must_use]
    pub const fn trap_focus(mut self) -> Self {
        self.focus = Focus::Trap;
        self
    }

    #[must_use]
    pub fn on_click(self, handler: impl Fn() + 'static) -> Self {
        self.on(EventKind::Click, move |_| handler())
    }

    /// Listens for `input`, passing the control's new value.
    #[must_use]
    pub fn on_input(self, handler: impl Fn(&str) + 'static) -> Self {
        self.on(EventKind::Input, move |e| {
            if let Event::Input(v) = e {
                handler(v);
            }
        })
    }

    /// Listens for `change`, passing the control's new value.
    #[must_use]
    pub fn on_change(self, handler: impl Fn(&str) + 'static) -> Self {
        self.on(EventKind::Change, move |e| {
            if let Event::Change(v) = e {
                handler(v);
            }
        })
    }

    #[must_use]
    pub fn on_submit(self, handler: impl Fn() + 'static) -> Self {
        self.on(EventKind::Submit, move |_| handler())
    }

    #[must_use]
    pub fn on_keydown(self, handler: impl Fn(&KeyEvent) + 'static) -> Self {
        self.on(EventKind::KeyDown, move |e| {
            if let Event::KeyDown(k) = e {
                handler(k);
            }
        })
    }
}

macro_rules! tags {
    ($($(#[$m:meta])* $name:ident => $tag:literal),* $(,)?) => {$(
        $(#[$m])*
        #[must_use]
        pub fn $name() -> Element {
            element($tag)
        }
    )*};
}

tags! {
    a => "a",
    button => "button",
    div => "div",
    footer => "footer",
    form => "form",
    h1 => "h1",
    h2 => "h2",
    h3 => "h3",
    header => "header",
    i => "i",
    img => "img",
    input => "input",
    label => "label",
    li => "li",
    main => "main",
    nav => "nav",
    option => "option",
    p => "p",
    pre => "pre",
    section => "section",
    select => "select",
    span => "span",
    strong => "strong",
    textarea => "textarea",
    ul => "ul",
}
