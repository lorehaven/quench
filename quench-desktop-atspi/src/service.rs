//! The AT-SPI side: the objects a screen reader reads, answered from a snapshot of the accessibility tree.
//!
//! An application shows up on the accessibility bus as an `Application` object (path `.../accessible/root`)
//! with one `Frame` (the window) whose child is the page, `Document`, with the nodes of the tree under it. Each
//! node answers the `Accessible` interface (name, role, state, children, parent), `Component` (where it is),
//! `Action` (what can be done) and, for text, `Text`. Actions asked for are queued for the program to carry out
//! on its own thread.

use std::collections::{HashMap, VecDeque};
use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use quench_ui_native::{AccessAction, AccessNode, Role};

use crate::conn::{Connection, Sender};
use crate::message::{Kind, Message};
use crate::value::Value;

pub const ROOT_PATH: &str = "/org/a11y/atspi/accessible/root";
const WINDOW_PATH: &str = "/org/a11y/atspi/accessible/window";
const PREFIX: &str = "/org/a11y/atspi/accessible/";
const NULL_PATH: &str = "/org/a11y/atspi/null";

const ACCESSIBLE: &str = "org.a11y.atspi.Accessible";
const APPLICATION: &str = "org.a11y.atspi.Application";
const COMPONENT: &str = "org.a11y.atspi.Component";
const ACTION: &str = "org.a11y.atspi.Action";
const TEXT: &str = "org.a11y.atspi.Text";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";

/// AT-SPI role numbers (`AtspiRole`).
const fn atspi_role(node: &AccessNode) -> u32 {
    match node.role {
        Role::Document => 95,
        Role::Heading => 83,
        Role::Paragraph => 73,
        Role::Text => 116,
        Role::Button if node.selected.is_some() => 62,
        Role::Button => 43,
        Role::Link => 88,
        Role::Checkbox => 7,
        Role::Radio => 44,
        Role::Switch => 62,
        Role::TextBox => 79,
        Role::List => 31,
        Role::ListItem => 32,
        Role::Tab => 37,
        Role::TabList => 38,
        Role::TabPanel => 39,
        Role::Dialog => 16,
        Role::Navigation => 110,
        Role::Status => 54,
        Role::Alert => 2,
        Role::Image => 27,
        Role::Form => 87,
        Role::Group => 85,
    }
}

const fn role_name(role: u32) -> &'static str {
    match role {
        95 => "document web",
        83 => "heading",
        73 => "paragraph",
        116 => "static",
        43 => "push button",
        62 => "toggle button",
        88 => "link",
        7 => "check box",
        44 => "radio button",
        79 => "entry",
        31 => "list",
        32 => "list item",
        37 => "page tab",
        38 => "page tab list",
        39 => "panel",
        16 => "dialog",
        110 => "landmark",
        54 => "status bar",
        2 => "alert",
        27 => "image",
        87 => "form",
        85 => "section",
        23 => "frame",
        75 => "application",
        _ => "unknown",
    }
}

/// `AtspiStateType` bits.
mod state {
    pub const ACTIVE: u32 = 1;
    pub const CHECKED: u32 = 4;
    pub const EDITABLE: u32 = 7;
    pub const ENABLED: u32 = 8;
    pub const EXPANDABLE: u32 = 9;
    pub const EXPANDED: u32 = 10;
    pub const FOCUSABLE: u32 = 11;
    pub const FOCUSED: u32 = 12;
    pub const PRESSED: u32 = 20;
    pub const SELECTABLE: u32 = 22;
    pub const SELECTED: u32 = 23;
    pub const SENSITIVE: u32 = 24;
    pub const SHOWING: u32 = 25;
    pub const SINGLE_LINE: u32 = 26;
    pub const VISIBLE: u32 = 30;
    pub const REQUIRED: u32 = 33;
    pub const CHECKABLE: u32 = 41;
    pub const READ_ONLY: u32 = 43;
}

/// What the service knows about the page, replaced whenever the page changes.
#[derive(Default)]
struct Snapshot {
    tree: Option<AccessNode>,
    /// id -> (parent id, index in parent); the document is id 0 and its parent is the window.
    parents: HashMap<usize, (usize, usize)>,
}

fn index(node: &AccessNode, parents: &mut HashMap<usize, (usize, usize)>) {
    for (i, c) in node.children.iter().enumerate() {
        parents.insert(c.id, (node.id, i));
        index(c, parents);
    }
}

enum Object<'a> {
    Application,
    Window,
    Node(&'a AccessNode),
}

/// The state shared between the thread that answers the bus and the program.
pub struct Shared {
    snapshot: Mutex<Snapshot>,
    actions: Mutex<VecDeque<(usize, AccessAction)>>,
    asked: AtomicBool,
    name: Mutex<String>,
    title: Mutex<String>,
    /// The registry's root, the parent of the application object.
    registry_root: Mutex<(String, String)>,
}

fn path_of(id: usize) -> String {
    format!("{PREFIX}{id}")
}

fn parse_path(path: &str) -> Option<Option<usize>> {
    match path {
        ROOT_PATH => Some(None),
        WINDOW_PATH => Some(Some(usize::MAX)),
        other => other.strip_prefix(PREFIX)?.parse().ok().map(Some),
    }
}

impl Shared {
    #[must_use]
    pub fn new(app_name: &str) -> Arc<Self> {
        Arc::new(Self {
            snapshot: Mutex::new(Snapshot::default()),
            actions: Mutex::new(VecDeque::new()),
            asked: AtomicBool::new(false),
            name: Mutex::new(String::new()),
            title: Mutex::new(app_name.to_string()),
            registry_root: Mutex::new((String::new(), NULL_PATH.to_string())),
        })
    }

    pub fn set_name(&self, name: &str) {
        *self
            .name
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = name.to_string();
    }

    pub fn set_registry_root(&self, name: &str, path: &str) {
        *self
            .registry_root
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) =
            (name.to_string(), path.to_string());
    }

    /// Replaces the snapshot; returns the id that had focus before and the one that has it now.
    pub fn publish(&self, tree: AccessNode) -> (Option<usize>, Option<usize>) {
        fn focused(n: &AccessNode) -> Option<usize> {
            if n.focused {
                return Some(n.id);
            }
            n.children.iter().find_map(focused)
        }
        let mut snapshot = self
            .snapshot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let before = snapshot.tree.as_ref().and_then(focused);
        let now = focused(&tree);
        let mut parents = HashMap::new();
        index(&tree, &mut parents);
        snapshot.parents = parents;
        snapshot.tree = Some(tree);
        (before, now)
    }

    /// The actions asked for since the last call.
    pub fn take_actions(&self) -> Vec<(usize, AccessAction)> {
        self.actions
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .drain(..)
            .collect()
    }

    /// Whether anything has asked a question since the last call (a screen reader is attached and active).
    pub fn take_asked(&self) -> bool {
        self.asked.swap(false, Ordering::Relaxed)
    }

    fn me(&self) -> String {
        self.name
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    fn reference(&self, path: &str) -> Value {
        Value::reference(&self.me(), path)
    }

    fn null(&self) -> Value {
        Value::reference("", NULL_PATH)
    }

    /// Answers a method call (`None`: not a call for this object at all).
    pub fn handle(&self, call: &Message) -> Option<Message> {
        if call.kind != Kind::MethodCall {
            return None;
        }
        self.asked.store(true, Ordering::Relaxed);
        let (path, interface, member) = (
            call.path.as_deref()?,
            call.interface.as_deref().unwrap_or(""),
            call.member.as_deref()?,
        );
        if interface == "org.freedesktop.DBus.Peer" {
            return Some(match member {
                "Ping" => Message::reply(call, Vec::new()),
                _ => Message::reply(call, vec![Value::str("0123456789abcdef0123456789abcdef")]),
            });
        }
        let Some(target) = parse_path(path) else {
            return Some(Message::error(
                call,
                "org.freedesktop.DBus.Error.UnknownObject",
                path,
            ));
        };
        let snapshot = self
            .snapshot
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let object = match target {
            None => Object::Application,
            Some(usize::MAX) => Object::Window,
            Some(id) => match snapshot.tree.as_ref().and_then(|t| t.find(id)) {
                Some(n) => Object::Node(n),
                None => {
                    return Some(Message::error(
                        call,
                        "org.freedesktop.DBus.Error.UnknownObject",
                        path,
                    ));
                }
            },
        };
        let reply = self.dispatch(&snapshot, &object, interface, member, call);
        Some(reply.unwrap_or_else(|| {
            Message::error(
                call,
                "org.freedesktop.DBus.Error.UnknownMethod",
                &format!("{interface}.{member}"),
            )
        }))
    }

    #[allow(
        clippy::too_many_lines,
        reason = "one arm per method of each interface"
    )]
    fn dispatch(
        &self,
        snapshot: &Snapshot,
        object: &Object<'_>,
        interface: &str,
        member: &str,
        call: &Message,
    ) -> Option<Message> {
        let arg = |i: usize| call.body.get(i).and_then(Value::as_i64);
        let ok = |body: Vec<Value>| Some(Message::reply(call, body));
        let children: Vec<Value> = match object {
            Object::Application => vec![self.reference(WINDOW_PATH)],
            Object::Window => snapshot
                .tree
                .iter()
                .map(|t| self.reference(&path_of(t.id)))
                .collect(),
            Object::Node(n) => n
                .children
                .iter()
                .map(|c| self.reference(&path_of(c.id)))
                .collect(),
        };
        let (role, name, description) = match object {
            Object::Application => (
                75,
                self.title
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .clone(),
                String::new(),
            ),
            Object::Window => (
                23,
                self.title
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .clone(),
                String::new(),
            ),
            Object::Node(n) => (atspi_role(n), n.name.clone(), n.description.clone()),
        };
        let parent = || match object {
            Object::Application => {
                let root = self
                    .registry_root
                    .lock()
                    .unwrap_or_else(std::sync::PoisonError::into_inner)
                    .clone();
                Value::reference(&root.0, &root.1)
            }
            Object::Window => self.reference(ROOT_PATH),
            Object::Node(n) => match snapshot.parents.get(&n.id) {
                Some((p, _)) => self.reference(&path_of(*p)),
                None => self.reference(WINDOW_PATH),
            },
        };
        let bounds = match object {
            Object::Node(n) => n.bounds,
            Object::Window => snapshot
                .tree
                .as_ref()
                .map_or((0.0, 0.0, 0.0, 0.0), |t| t.bounds),
            Object::Application => (0.0, 0.0, 0.0, 0.0),
        };
        let states = || -> Value {
            let mut bits: u64 = 0;
            bits |= 1u64 << state::ENABLED;
            bits |= 1u64 << state::SENSITIVE;
            bits |= 1u64 << state::SHOWING;
            bits |= 1u64 << state::VISIBLE;
            match object {
                Object::Window => bits |= 1u64 << state::ACTIVE,
                Object::Application => {}
                Object::Node(n) => {
                    if n.disabled {
                        bits &= !((1u64 << state::ENABLED) | (1u64 << state::SENSITIVE));
                    }
                    if n.focusable {
                        bits |= 1u64 << state::FOCUSABLE;
                    }
                    if n.focused {
                        bits |= 1u64 << state::FOCUSED;
                    }
                    if let Some(c) = n.checked {
                        bits |= 1u64 << state::CHECKABLE;
                        if c {
                            bits |= 1u64 << state::CHECKED;
                        }
                    }
                    if let Some(p) = n.selected {
                        if matches!(n.role, Role::Tab) {
                            bits |= 1u64 << state::SELECTABLE;
                            if p {
                                bits |= 1u64 << state::SELECTED;
                            }
                        } else if p {
                            bits |= 1u64 << state::PRESSED;
                        }
                    }
                    if let Some(e) = n.expanded {
                        bits |= 1u64 << state::EXPANDABLE;
                        if e {
                            bits |= 1u64 << state::EXPANDED;
                        }
                    }
                    if n.role == Role::TextBox {
                        bits |= 1u64 << state::SINGLE_LINE;
                        if n.read_only {
                            bits |= 1u64 << state::READ_ONLY;
                        } else {
                            bits |= 1u64 << state::EDITABLE;
                        }
                    }
                    if n.required {
                        bits |= 1u64 << state::REQUIRED;
                    }
                }
            }
            Value::Array(
                "u".into(),
                vec![Value::U32(bits as u32), Value::U32((bits >> 32) as u32)],
            )
        };
        let interfaces = || {
            let mut list = vec![ACCESSIBLE];
            match object {
                Object::Application => list.push(APPLICATION),
                Object::Window => list.push(COMPONENT),
                Object::Node(n) => {
                    list.push(COMPONENT);
                    if n.actions
                        .iter()
                        .any(|a| !matches!(a, AccessAction::ScrollIntoView))
                    {
                        list.push(ACTION);
                    }
                    if matches!(n.role, Role::TextBox | Role::Text) {
                        list.push(TEXT);
                    }
                }
            }
            list
        };
        let text = || match object {
            Object::Node(n) if n.role == Role::TextBox => n.value.clone().unwrap_or_default(),
            Object::Node(n) => n.name.clone(),
            _ => String::new(),
        };
        let actions: Vec<(&str, AccessAction)> = match object {
            Object::Node(n) => n
                .actions
                .iter()
                .filter_map(|a| match a {
                    AccessAction::Click => Some(("click", *a)),
                    AccessAction::Focus => Some(("focus", *a)),
                    AccessAction::ScrollIntoView => None,
                })
                .collect(),
            _ => Vec::new(),
        };
        let get = |interface: &str, prop: &str| -> Option<Value> {
            Some(match (interface, prop) {
                (ACCESSIBLE, "Name") => Value::Str(name.clone()),
                (ACCESSIBLE, "Description") => Value::Str(description.clone()),
                (ACCESSIBLE, "Parent") => parent(),
                (ACCESSIBLE, "ChildCount") => Value::I32(children.len() as i32),
                (ACCESSIBLE, "Locale") => Value::str("en_US"),
                (ACCESSIBLE, "AccessibleId") => Value::str(""),
                (APPLICATION, "ToolkitName") => Value::str("quench"),
                (APPLICATION, "Version") => Value::str("0.1"),
                (APPLICATION, "Id") => Value::I32(0),
                (ACTION, "NActions") => Value::I32(actions.len() as i32),
                (TEXT, "CharacterCount") => Value::I32(text().chars().count() as i32),
                (TEXT, "CaretOffset") => Value::I32(match object {
                    Object::Node(n) => n.caret.map_or(0, |c| c.0 as i32),
                    _ => 0,
                }),
                _ => return None,
            })
        };
        let extents = || {
            let (x, y, w, h) = bounds;
            Value::Struct(vec![
                Value::I32(x as i32),
                Value::I32(y as i32),
                Value::I32(w as i32),
                Value::I32(h as i32),
            ])
        };
        match (interface, member) {
            (PROPERTIES, "Get") => {
                let (i, p) = (call.body.first()?.as_str()?, call.body.get(1)?.as_str()?);
                match get(i, p) {
                    Some(v) => ok(vec![Value::Variant(Box::new(v))]),
                    None => Some(Message::error(
                        call,
                        "org.freedesktop.DBus.Error.UnknownProperty",
                        p,
                    )),
                }
            }
            (PROPERTIES, "GetAll") => {
                let i = call.body.first()?.as_str()?;
                let names: &[&str] = match i {
                    ACCESSIBLE => &[
                        "Name",
                        "Description",
                        "Parent",
                        "ChildCount",
                        "Locale",
                        "AccessibleId",
                    ],
                    APPLICATION => &["ToolkitName", "Version", "Id"],
                    ACTION => &["NActions"],
                    TEXT => &["CharacterCount", "CaretOffset"],
                    _ => &[],
                };
                let entries = names
                    .iter()
                    .filter_map(|p| {
                        get(i, p).map(|v| {
                            Value::DictEntry(
                                Box::new(Value::str(p)),
                                Box::new(Value::Variant(Box::new(v))),
                            )
                        })
                    })
                    .collect();
                ok(vec![Value::Array("{sv}".into(), entries)])
            }
            (ACCESSIBLE, "GetChildAtIndex") => {
                let i = arg(0)?;
                ok(vec![
                    usize::try_from(i)
                        .ok()
                        .and_then(|i| children.get(i).cloned())
                        .unwrap_or_else(|| self.null()),
                ])
            }
            (ACCESSIBLE, "GetChildren") => ok(vec![Value::Array("(so)".into(), children)]),
            (ACCESSIBLE, "GetIndexInParent") => ok(vec![Value::I32(match object {
                Object::Node(n) => snapshot.parents.get(&n.id).map_or(0, |p| p.1 as i32),
                _ => 0,
            })]),
            (ACCESSIBLE, "GetRelationSet") => ok(vec![Value::Array("(ua(so))".into(), Vec::new())]),
            (ACCESSIBLE, "GetRole") => ok(vec![Value::U32(role)]),
            (ACCESSIBLE, "GetRoleName" | "GetLocalizedRoleName") => {
                ok(vec![Value::str(role_name(role))])
            }
            (ACCESSIBLE, "GetState") => ok(vec![states()]),
            (ACCESSIBLE, "GetAttributes") => {
                let mut attrs = vec![Value::DictEntry(
                    Box::new(Value::str("toolkit")),
                    Box::new(Value::str("quench")),
                )];
                if let Object::Node(n) = object
                    && n.level > 0
                {
                    attrs.push(Value::DictEntry(
                        Box::new(Value::str("level")),
                        Box::new(Value::str(&n.level.to_string())),
                    ));
                }
                ok(vec![Value::Array("{ss}".into(), attrs)])
            }
            (ACCESSIBLE, "GetApplication") => ok(vec![self.reference(ROOT_PATH)]),
            (ACCESSIBLE, "GetInterfaces") => ok(vec![Value::strings(&interfaces())]),
            (APPLICATION, "GetLocale") => ok(vec![Value::str("en_US")]),
            (APPLICATION, "GetApplicationBusAddress") => ok(vec![Value::str("")]),
            (COMPONENT, "Contains") => {
                let (px, py) = (arg(0)? as f32, arg(1)? as f32);
                ok(vec![Value::Bool(
                    px >= bounds.0
                        && px < bounds.0 + bounds.2
                        && py >= bounds.1
                        && py < bounds.1 + bounds.3,
                )])
            }
            (COMPONENT, "GetAccessibleAtPoint") => {
                let (px, py) = (arg(0)? as f32, arg(1)? as f32);
                fn deepest(n: &AccessNode, x: f32, y: f32) -> Option<usize> {
                    let (bx, by, bw, bh) = n.bounds;
                    let inside = x >= bx && x < bx + bw && y >= by && y < by + bh;
                    n.children
                        .iter()
                        .rev()
                        .find_map(|c| deepest(c, x, y))
                        .or_else(|| (inside && n.id != 0).then_some(n.id))
                }
                let found = snapshot.tree.as_ref().and_then(|t| deepest(t, px, py));
                ok(vec![found.map_or_else(
                    || self.null(),
                    |id| self.reference(&path_of(id)),
                )])
            }
            (COMPONENT, "GetExtents") => ok(vec![extents()]),
            (COMPONENT, "GetPosition") => ok(vec![
                Value::I32(bounds.0 as i32),
                Value::I32(bounds.1 as i32),
            ]),
            (COMPONENT, "GetSize") => ok(vec![
                Value::I32(bounds.2 as i32),
                Value::I32(bounds.3 as i32),
            ]),
            (COMPONENT, "GetLayer") => ok(vec![Value::U32(3)]),
            (COMPONENT, "GetMDIZOrder") => ok(vec![Value::I16(0)]),
            (COMPONENT, "GetAlpha") => ok(vec![Value::F64(1.0)]),
            (COMPONENT, "GrabFocus") => {
                if let Object::Node(n) = object {
                    self.actions
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .push_back((n.id, AccessAction::Focus));
                    ok(vec![Value::Bool(n.focusable)])
                } else {
                    ok(vec![Value::Bool(false)])
                }
            }
            (COMPONENT, "ScrollTo") => {
                if let Object::Node(n) = object {
                    self.actions
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .push_back((n.id, AccessAction::ScrollIntoView));
                }
                ok(vec![Value::Bool(true)])
            }
            (ACTION, "GetNActions") => ok(vec![Value::I32(actions.len() as i32)]),
            (ACTION, "GetName" | "GetLocalizedName") => ok(vec![Value::str(
                usize::try_from(arg(0)?)
                    .ok()
                    .and_then(|i| actions.get(i))
                    .map_or("", |a| a.0),
            )]),
            (ACTION, "GetDescription" | "GetKeyBinding") => ok(vec![Value::str("")]),
            (ACTION, "GetActions") => ok(vec![Value::Array(
                "(sss)".into(),
                actions
                    .iter()
                    .map(|a| Value::Struct(vec![Value::str(a.0), Value::str(""), Value::str("")]))
                    .collect(),
            )]),
            (ACTION, "DoAction") => {
                let chosen = usize::try_from(arg(0)?).ok().and_then(|i| actions.get(i));
                if let (Some((_, action)), Object::Node(n)) = (chosen, object) {
                    self.actions
                        .lock()
                        .unwrap_or_else(std::sync::PoisonError::into_inner)
                        .push_back((n.id, *action));
                    ok(vec![Value::Bool(true)])
                } else {
                    ok(vec![Value::Bool(false)])
                }
            }
            (TEXT, "GetText") => {
                let (start, end) = (usize::try_from(arg(0)?).unwrap_or(0), arg(1)?);
                let all = text();
                let end = usize::try_from(end)
                    .ok()
                    .filter(|e| *e > 0)
                    .unwrap_or_else(|| all.chars().count());
                ok(vec![Value::Str(
                    all.chars()
                        .skip(start)
                        .take(end.saturating_sub(start))
                        .collect(),
                )])
            }
            (TEXT, "GetCharacterCount") => ok(vec![Value::I32(text().chars().count() as i32)]),
            (TEXT, "GetCaretOffset") => ok(vec![get(TEXT, "CaretOffset")?]),
            (TEXT, "GetNSelections") => ok(vec![Value::I32(match object {
                Object::Node(n) if n.caret.is_some_and(|c| c.0 != c.1) => 1,
                _ => 0,
            })]),
            (TEXT, "GetSelection") => {
                let (a, b) = match object {
                    Object::Node(n) => n.caret.unwrap_or((0, 0)),
                    _ => (0, 0),
                };
                ok(vec![
                    Value::I32(a.min(b) as i32),
                    Value::I32(a.max(b) as i32),
                ])
            }
            _ => None,
        }
    }
}

/// A running service: the thread that answers the accessibility bus, and what the program shares with it.
pub struct Service {
    shared: Arc<Shared>,
    sender: Sender,
    focused: Mutex<Option<usize>>,
}

impl Service {
    /// Connects to the accessibility bus, registers the application and starts answering.
    ///
    /// # Errors
    /// No session bus, no accessibility bus (the accessibility service is not running), or a bus that
    /// answers wrongly.
    pub fn start(app_name: &str) -> io::Result<Self> {
        let session = session_bus()?;
        Self::start_on(session, app_name)
    }

    /// Like [`start`](Self::start), with the session bus connection to ask for the accessibility bus on.
    ///
    /// # Errors
    /// As [`start`](Self::start).
    pub fn start_on(mut session: Connection, app_name: &str) -> io::Result<Self> {
        // Nothing may wait for ever on a bus that does not answer.
        session.set_timeout(Some(std::time::Duration::from_secs(5)))?;
        let reply = session.call(Message::call(
            Some("org.a11y.Bus"),
            "/org/a11y/bus",
            "org.a11y.Bus",
            "GetAddress",
            Vec::new(),
        ))?;
        let address = reply
            .body
            .first()
            .and_then(Value::as_str)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "no address"))?
            .to_string();
        let mut bus = Connection::bus(&address)?;
        bus.set_timeout(Some(std::time::Duration::from_secs(5)))?;
        let shared = Shared::new(app_name);
        shared.set_name(&bus.name);
        let registered = bus.call(Message::call(
            Some("org.a11y.atspi.Registry"),
            ROOT_PATH,
            "org.a11y.atspi.Socket",
            "Embed",
            vec![Value::reference(&bus.name, ROOT_PATH)],
        ))?;
        if let Some(Value::Struct(parts)) = registered.body.first()
            && let [a, b] = parts.as_slice()
            && let (Some(a), Some(b)) = (a.as_str(), b.as_str())
        {
            shared.set_registry_root(a, b);
        }
        // Answering calls waits as long as it takes.
        bus.set_timeout(None)?;
        Ok(Self::serve(bus, shared))
    }

    /// Answers calls on `connection` (a bus, or a peer in a test) from a thread of its own.
    #[must_use]
    pub fn serve(mut connection: Connection, shared: Arc<Shared>) -> Self {
        let sender = connection.sender();
        let (answers, state) = (connection.sender(), Arc::clone(&shared));
        std::thread::Builder::new()
            .name("atspi".into())
            .spawn(move || {
                while let Ok(message) = connection.recv() {
                    if let Some(mut reply) = state.handle(&message) {
                        if message.sender.is_some() || reply.destination.is_none() {
                            reply.destination.clone_from(&message.sender);
                        }
                        if answers.send(&mut reply).is_err() {
                            break;
                        }
                    }
                }
            })
            .expect("a thread");
        Self {
            shared,
            sender,
            focused: Mutex::new(None),
        }
    }

    /// The state shared with the answering thread.
    #[must_use]
    pub const fn shared(&self) -> &Arc<Shared> {
        &self.shared
    }

    /// Shows the service the page as it is now, and tells assistive technology when focus moved.
    pub fn publish(&self, tree: AccessNode) {
        let (_, now) = self.shared.publish(tree);
        let mut focused = self
            .focused
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if *focused == now {
            return;
        }
        let emit = |id: usize, gained: bool| {
            let mut signal = Message::signal(
                &path_of(id),
                "org.a11y.atspi.Event.Object",
                "StateChanged",
                vec![
                    Value::str("focused"),
                    Value::I32(i32::from(gained)),
                    Value::I32(0),
                    Value::Variant(Box::new(Value::I32(0))),
                    Value::Array("{sv}".into(), Vec::new()),
                ],
            );
            let _ = self.sender.send(&mut signal);
        };
        if let Some(old) = *focused {
            emit(old, false);
        }
        if let Some(new) = now {
            emit(new, true);
        }
        *focused = now;
    }

    /// The actions assistive technology asked for since the last call.
    #[must_use]
    pub fn take_actions(&self) -> Vec<(usize, AccessAction)> {
        self.shared.take_actions()
    }

    /// Whether assistive technology has asked anything since the last call.
    #[must_use]
    pub fn take_asked(&self) -> bool {
        self.shared.take_asked()
    }
}

/// The session bus: the address in the environment, or the user's standard socket when that is not there.
fn session_bus() -> io::Result<Connection> {
    let mut last = io::Error::new(io::ErrorKind::NotFound, "no session bus");
    let standard = std::fs::metadata("/proc/self").ok().map(|m| {
        use std::os::unix::fs::MetadataExt;
        format!("unix:path=/run/user/{}/bus", m.uid())
    });
    for address in std::env::var("DBUS_SESSION_BUS_ADDRESS")
        .ok()
        .into_iter()
        .chain(standard)
    {
        match Connection::bus(&address) {
            Ok(c) => return Ok(c),
            Err(e) => last = e,
        }
    }
    Err(last)
}

impl std::fmt::Debug for Shared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Shared")
            .field("name", &self.me())
            .finish_non_exhaustive()
    }
}

impl std::fmt::Debug for Service {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Service")
            .field("shared", &self.shared)
            .finish_non_exhaustive()
    }
}
