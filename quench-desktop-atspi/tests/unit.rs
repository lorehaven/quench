// The custom harness does not set `cfg(test)`, so clippy no longer treats these files as test code.
#![allow(
    clippy::option_if_let_else,
    clippy::unwrap_used,
    clippy::too_many_lines,
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap
)]
#![cfg(target_os = "linux")]

use std::os::unix::net::UnixStream;
use std::sync::Arc;
use std::time::Duration;

use quench_desktop_atspi::conn::{authenticate_client, authenticate_server, connect};
use quench_desktop_atspi::value::split_signature;
use quench_desktop_atspi::{Connection, Kind, Message, Service, Shared, Value};
use quench_ui_native::{AccessAction, AccessNode, Role};

fn roundtrip(v: &Value) -> Value {
    let mut buf = Vec::new();
    v.encode(&mut buf);
    let mut at = 0;
    let back = Value::decode(&v.signature(), &buf, &mut at).unwrap();
    assert_eq!(at, buf.len(), "all of it was read: {v:?}");
    back
}

#[quench_test::test]
fn every_type_survives_the_wire() {
    let values = [
        Value::Byte(7),
        Value::Bool(true),
        Value::I16(-2),
        Value::U16(65000),
        Value::I32(-100_000),
        Value::U32(4_000_000_000),
        Value::I64(-5_000_000_000),
        Value::U64(u64::MAX),
        Value::F64(1.5),
        Value::str("héllo"),
        Value::path("/a/b"),
        Value::Signature("a{sv}".into()),
        Value::strings(&["a", "bb", ""]),
        Value::Array("y".into(), vec![Value::Byte(1), Value::Byte(2)]),
        Value::Array("(so)".into(), Vec::new()),
        Value::reference(":1.5", "/x"),
        Value::Struct(vec![Value::Byte(1), Value::I64(2), Value::str("z")]),
        Value::Variant(Box::new(Value::Variant(Box::new(Value::I32(3))))),
        Value::Array(
            "{sv}".into(),
            vec![
                Value::DictEntry(
                    Box::new(Value::str("k")),
                    Box::new(Value::Variant(Box::new(Value::str("v")))),
                ),
                Value::DictEntry(
                    Box::new(Value::str("n")),
                    Box::new(Value::Variant(Box::new(Value::U32(9)))),
                ),
            ],
        ),
        Value::Array(
            "a{ss}".into(),
            vec![Value::Array(
                "{ss}".into(),
                vec![Value::DictEntry(
                    Box::new(Value::str("a")),
                    Box::new(Value::str("b")),
                )],
            )],
        ),
    ];
    for v in &values {
        assert_eq!(&roundtrip(v), v);
    }
    // Alignment: a byte, then an int is padded to 4, a 64-bit to 8.
    let mut buf = Vec::new();
    Value::Byte(1).encode(&mut buf);
    Value::U32(2).encode(&mut buf);
    Value::U64(3).encode(&mut buf);
    assert_eq!(buf.len(), 16);
    assert_eq!(&buf[..8], &[1, 0, 0, 0, 2, 0, 0, 0]);
    // A string is its length, the bytes and a NUL.
    let mut buf = Vec::new();
    Value::str("ab").encode(&mut buf);
    assert_eq!(buf, [2, 0, 0, 0, b'a', b'b', 0]);
    assert_eq!(
        Value::Struct(vec![Value::Byte(1), Value::str("s")]).signature(),
        "(ys)"
    );
    assert_eq!(
        split_signature("a{sv}i(ss)ay").unwrap(),
        ["a{sv}", "i", "(ss)", "ay"]
    );
    assert!(split_signature("(i").is_err() && split_signature("!").is_err());
}

#[quench_test::test]
fn hostile_values_are_errors_not_panics() {
    let decode = |sig: &str, bytes: &[u8]| Value::decode(sig, bytes, &mut 0);
    assert!(decode("u", &[1, 2]).is_err());
    assert!(decode("s", &[255, 255, 255, 255, 0]).is_err());
    assert!(
        decode("s", &[1, 0, 0, 0, b'a', 1]).is_err(),
        "no terminator"
    );
    assert!(
        decode("s", &[2, 0, 0, 0, 0xff, 0xfe, 0]).is_err(),
        "not UTF-8"
    );
    assert!(decode("g", &[5, b'a', 0]).is_err());
    assert!(
        decode("ai", &[255, 255, 255, 127]).is_err(),
        "an array longer than the data"
    );
    assert!(decode("ai", &[3, 0, 0, 0, 1, 0, 0]).is_err());
    assert!(
        decode("v", &[2, b'i', b'i', 0]).is_err(),
        "a variant holds one value"
    );
    assert!(decode("{i}", &[0; 16]).is_err());
    assert!(decode("z", &[0; 8]).is_err() && decode("", &[]).is_err());
    let deep = format!("{}y{}", "(".repeat(40), ")".repeat(40));
    assert!(decode(&deep, &[0; 64]).is_err());
    assert!(
        Message::from_bytes(&[b'B', 1, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]).is_err(),
        "big endian is refused"
    );
    assert!(
        Message::from_bytes(&[b'l', 9, 0, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0]).is_err(),
        "unknown kind"
    );
    assert!(
        Message::from_bytes(&[b'l', 1, 0, 1, 255, 255, 255, 255, 1, 0, 0, 0, 0, 0, 0, 0]).is_err(),
        "body too long"
    );
    assert!(Message::from_bytes(&[b'l', 1, 0]).is_err());
}

#[quench_test::test]
fn messages_carry_their_fields_and_body() {
    let mut call = Message::call(
        Some("org.x"),
        "/p",
        "org.x.I",
        "M",
        vec![
            Value::str("a"),
            Value::I32(5),
            Value::reference(":1.1", "/q"),
        ],
    );
    call.serial = 42;
    call.sender = Some(":1.9".into());
    let back = Message::from_bytes(&call.encode()).unwrap();
    assert_eq!(back, call);
    let reply = Message::reply(&back, vec![Value::Bool(true)]);
    assert_eq!(
        (reply.kind, reply.reply_serial, reply.destination.as_deref()),
        (Kind::MethodReturn, Some(42), Some(":1.9"))
    );
    let error = Message::error(&back, "org.x.Error", "bad");
    assert_eq!(
        Message::from_bytes(&{
            let mut e = error;
            e.serial = 1;
            e.encode()
        })
        .unwrap()
        .body,
        [Value::str("bad")]
    );
    let signal = Message::signal("/s", "org.x.I", "Changed", vec![]);
    assert_eq!(signal.kind, Kind::Signal);
    let mut bytes = signal.encode();
    assert_eq!(bytes[2] & 1, 1, "a signal expects no reply");
    bytes.truncate(bytes.len() - 1);
    assert!(
        Message::from_bytes(&bytes).is_err() || bytes.len() == 16 + 8,
        "a truncated message is not read"
    );
}

#[quench_test::test]
fn the_peers_authenticate_with_external() {
    let (mut a, mut b) = UnixStream::pair().unwrap();
    let server = std::thread::spawn(move || {
        authenticate_server(&mut b).unwrap();
        b
    });
    authenticate_client(&mut a).unwrap();
    let _ = server.join().unwrap();
    // A client that asks for something else is rejected.
    let (mut a, mut b) = UnixStream::pair().unwrap();
    let server = std::thread::spawn(move || authenticate_server(&mut b).is_err());
    std::io::Write::write_all(&mut a, b"\0AUTH ANONYMOUS\r\n").unwrap();
    assert!(server.join().unwrap());
    assert!(connect("tcp:host=x").is_err());
    assert!(connect("unix:path=/nonexistent/quench-bus").is_err());
    assert!(connect("unix:foo=bar").is_err());
}

fn node(
    id: usize,
    role: Role,
    name: &str,
    bounds: (f32, f32, f32, f32),
    children: Vec<AccessNode>,
) -> AccessNode {
    AccessNode {
        id,
        role,
        level: 0,
        name: name.into(),
        description: String::new(),
        value: None,
        bounds,
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
        children,
    }
}

fn page() -> AccessNode {
    let mut button = node(
        5,
        Role::Button,
        "Save",
        (10.0, 20.0, 80.0, 30.0),
        Vec::new(),
    );
    button.actions = vec![
        AccessAction::Click,
        AccessAction::Focus,
        AccessAction::ScrollIntoView,
    ];
    button.focusable = true;
    let mut check = node(
        6,
        Role::Checkbox,
        "Agree",
        (10.0, 60.0, 20.0, 20.0),
        Vec::new(),
    );
    check.checked = Some(true);
    let mut field = node(
        7,
        Role::TextBox,
        "Email",
        (10.0, 90.0, 200.0, 30.0),
        Vec::new(),
    );
    field.value = Some("ann@example.org".into());
    field.focusable = true;
    field.focused = true;
    field.caret = Some((3, 3));
    field.required = true;
    let mut heading = node(
        4,
        Role::Heading,
        "Welcome",
        (10.0, 0.0, 200.0, 20.0),
        Vec::new(),
    );
    heading.level = 2;
    let mut tab = node(8, Role::Tab, "First", (0.0, 130.0, 50.0, 20.0), Vec::new());
    tab.selected = Some(true);
    let mut off = node(9, Role::Button, "Off", (0.0, 160.0, 50.0, 20.0), Vec::new());
    off.disabled = true;
    node(
        0,
        Role::Document,
        "",
        (0.0, 0.0, 300.0, 600.0),
        vec![heading, button, check, field, tab, off],
    )
}

/// A client talking to a service over a socket pair, as a screen reader talks to an application.
struct Client {
    connection: Connection,
}

impl Client {
    fn call(
        &mut self,
        path: &str,
        interface: &str,
        member: &str,
        body: Vec<Value>,
    ) -> Result<Vec<Value>, String> {
        self.connection
            .call(Message::call(Some(":app"), path, interface, member, body))
            .map(|m| m.body)
            .map_err(|e| e.to_string())
    }

    fn get(&mut self, path: &str, interface: &str, property: &str) -> Value {
        let Value::Variant(v) = self
            .call(
                path,
                "org.freedesktop.DBus.Properties",
                "Get",
                vec![Value::str(interface), Value::str(property)],
            )
            .unwrap()
            .remove(0)
        else {
            panic!("a variant")
        };
        *v
    }
}

fn start() -> (Client, Service) {
    let (a, mut b) = UnixStream::pair().unwrap();
    let server_side = std::thread::spawn(move || {
        authenticate_server(&mut b).unwrap();
        b
    });
    let mut a = a;
    authenticate_client(&mut a).unwrap();
    let b = server_side.join().unwrap();
    let shared = Shared::new("quench test");
    shared.set_name(":app");
    let service = Service::serve(Connection::new(b, ":app").unwrap(), Arc::clone(&shared));
    let connection = Connection::new(a, ":client").unwrap();
    connection
        .set_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    (Client { connection }, service)
}

const A: &str = "org.a11y.atspi.Accessible";

#[quench_test::test]
fn the_application_window_and_page_form_a_tree() {
    let (mut c, service) = start();
    service.publish(page());
    let root = "/org/a11y/atspi/accessible/root";
    assert_eq!(c.get(root, A, "Name"), Value::str("quench test"));
    assert_eq!(
        c.call(root, A, "GetRole", vec![]).unwrap(),
        [Value::U32(75)]
    );
    assert_eq!(
        c.get(root, "org.a11y.atspi.Application", "ToolkitName"),
        Value::str("quench")
    );
    assert_eq!(c.get(root, A, "ChildCount"), Value::I32(1));
    let Value::Array(_, kids) = c.call(root, A, "GetChildren", vec![]).unwrap().remove(0) else {
        panic!()
    };
    assert_eq!(
        kids,
        [Value::reference(
            ":app",
            "/org/a11y/atspi/accessible/window"
        )]
    );
    let window = "/org/a11y/atspi/accessible/window";
    assert_eq!(
        c.call(window, A, "GetRole", vec![]).unwrap(),
        [Value::U32(23)]
    );
    assert_eq!(
        c.call(window, A, "GetRoleName", vec![]).unwrap(),
        [Value::str("frame")]
    );
    assert_eq!(c.get(window, A, "Parent"), Value::reference(":app", root));
    let Value::Array(_, kids) = c.call(window, A, "GetChildren", vec![]).unwrap().remove(0) else {
        panic!()
    };
    assert_eq!(
        kids,
        [Value::reference(":app", "/org/a11y/atspi/accessible/0")]
    );
    let document = "/org/a11y/atspi/accessible/0";
    assert_eq!(
        c.call(document, A, "GetRole", vec![]).unwrap(),
        [Value::U32(95)]
    );
    assert_eq!(c.get(document, A, "ChildCount"), Value::I32(6));
    assert_eq!(
        c.get(document, A, "Parent"),
        Value::reference(":app", window)
    );
    let child = c
        .call(document, A, "GetChildAtIndex", vec![Value::I32(1)])
        .unwrap();
    assert_eq!(
        child,
        [Value::reference(":app", "/org/a11y/atspi/accessible/5")]
    );
    assert_eq!(
        c.call(document, A, "GetChildAtIndex", vec![Value::I32(99)])
            .unwrap(),
        [Value::reference("", "/org/a11y/atspi/null")]
    );
    assert_eq!(
        c.call(
            "/org/a11y/atspi/accessible/5",
            A,
            "GetIndexInParent",
            vec![]
        )
        .unwrap(),
        [Value::I32(1)]
    );
    assert_eq!(
        c.call(root, A, "GetApplication", vec![]).unwrap(),
        [Value::reference(":app", root)]
    );
}

fn states(c: &mut Client, id: usize) -> u64 {
    let Value::Array(_, words) = c
        .call(
            &format!("/org/a11y/atspi/accessible/{id}"),
            A,
            "GetState",
            vec![],
        )
        .unwrap()
        .remove(0)
    else {
        panic!()
    };
    let (Value::U32(lo), Value::U32(hi)) = (&words[0], &words[1]) else {
        panic!()
    };
    u64::from(*lo) | (u64::from(*hi) << 32)
}

#[quench_test::test]
fn nodes_report_role_name_state_and_attributes() {
    let (mut c, service) = start();
    service.publish(page());
    let path = |id: usize| format!("/org/a11y/atspi/accessible/{id}");
    assert_eq!(c.get(&path(5), A, "Name"), Value::str("Save"));
    assert_eq!(
        c.call(&path(5), A, "GetRole", vec![]).unwrap(),
        [Value::U32(43)]
    );
    assert_eq!(
        c.call(&path(5), A, "GetRoleName", vec![]).unwrap(),
        [Value::str("push button")]
    );
    assert_eq!(
        c.call(&path(4), A, "GetRole", vec![]).unwrap(),
        [Value::U32(83)]
    );
    assert_eq!(
        c.call(&path(6), A, "GetRole", vec![]).unwrap(),
        [Value::U32(7)]
    );
    assert_eq!(
        c.call(&path(7), A, "GetRole", vec![]).unwrap(),
        [Value::U32(79)]
    );
    assert_eq!(
        c.call(&path(8), A, "GetRole", vec![]).unwrap(),
        [Value::U32(37)]
    );
    let bit = |s: u32| 1u64 << s;
    let button = states(&mut c, 5);
    assert!(
        button & bit(8) != 0 && button & bit(11) != 0 && button & bit(12) == 0,
        "enabled, focusable, not focused"
    );
    let check = states(&mut c, 6);
    assert!(
        check & bit(41) != 0 && check & bit(4) != 0,
        "checkable and checked"
    );
    let field = states(&mut c, 7);
    assert!(
        field & bit(12) != 0 && field & bit(7) != 0 && field & bit(33) != 0 && field & bit(26) != 0,
        "focused, editable, required, single line"
    );
    let tab = states(&mut c, 8);
    assert!(
        tab & bit(22) != 0 && tab & bit(23) != 0,
        "selectable and selected"
    );
    let off = states(&mut c, 9);
    assert!(
        off & bit(8) == 0 && off & bit(24) == 0,
        "a disabled button is not enabled or sensitive"
    );
    let Value::Array(_, attrs) = c
        .call(&path(4), A, "GetAttributes", vec![])
        .unwrap()
        .remove(0)
    else {
        panic!()
    };
    assert!(attrs.iter().any(|a| matches!(a, Value::DictEntry(k, v) if **k == Value::str("level") && **v == Value::str("2"))));
    let Value::Array(_, interfaces) = c
        .call(&path(5), A, "GetInterfaces", vec![])
        .unwrap()
        .remove(0)
    else {
        panic!()
    };
    let names: Vec<_> = interfaces.iter().filter_map(Value::as_str).collect();
    assert!(
        names.contains(&"org.a11y.atspi.Action") && names.contains(&"org.a11y.atspi.Component"),
        "{names:?}"
    );
    let Value::Array(_, interfaces) = c
        .call(&path(7), A, "GetInterfaces", vec![])
        .unwrap()
        .remove(0)
    else {
        panic!()
    };
    assert!(
        interfaces
            .iter()
            .any(|i| i.as_str() == Some("org.a11y.atspi.Text"))
    );
    // All properties at once.
    let Value::Array(_, all) = c
        .call(
            &path(5),
            "org.freedesktop.DBus.Properties",
            "GetAll",
            vec![Value::str(A)],
        )
        .unwrap()
        .remove(0)
    else {
        panic!()
    };
    assert_eq!(all.len(), 6);
    assert_eq!(
        c.call(&path(5), "org.freedesktop.DBus.Peer", "Ping", vec![])
            .unwrap(),
        []
    );
}

#[quench_test::test]
fn components_report_where_nodes_are_and_find_what_is_at_a_point() {
    let (mut c, service) = start();
    service.publish(page());
    let path = "/org/a11y/atspi/accessible/5";
    let comp = "org.a11y.atspi.Component";
    assert_eq!(
        c.call(path, comp, "GetExtents", vec![Value::U32(0)])
            .unwrap(),
        [Value::Struct(vec![
            Value::I32(10),
            Value::I32(20),
            Value::I32(80),
            Value::I32(30)
        ])]
    );
    assert_eq!(
        c.call(path, comp, "GetPosition", vec![Value::U32(0)])
            .unwrap(),
        [Value::I32(10), Value::I32(20)]
    );
    assert_eq!(
        c.call(path, comp, "GetSize", vec![]).unwrap(),
        [Value::I32(80), Value::I32(30)]
    );
    assert_eq!(
        c.call(
            path,
            comp,
            "Contains",
            vec![Value::I32(15), Value::I32(25), Value::U32(0)]
        )
        .unwrap(),
        [Value::Bool(true)]
    );
    assert_eq!(
        c.call(
            path,
            comp,
            "Contains",
            vec![Value::I32(0), Value::I32(0), Value::U32(0)]
        )
        .unwrap(),
        [Value::Bool(false)]
    );
    let at = |c: &mut Client, x: i32, y: i32| {
        c.call(
            "/org/a11y/atspi/accessible/window",
            comp,
            "GetAccessibleAtPoint",
            vec![Value::I32(x), Value::I32(y), Value::U32(0)],
        )
        .unwrap()
    };
    assert_eq!(
        at(&mut c, 15, 25),
        [Value::reference(":app", "/org/a11y/atspi/accessible/5")]
    );
    assert_eq!(
        at(&mut c, 290, 590),
        [Value::reference("", "/org/a11y/atspi/null")]
    );
    assert_eq!(
        c.call(path, comp, "GetLayer", vec![]).unwrap(),
        [Value::U32(3)]
    );
}

#[quench_test::test]
fn actions_asked_for_are_queued_for_the_program() {
    let (mut c, service) = start();
    service.publish(page());
    let path = "/org/a11y/atspi/accessible/5";
    let action = "org.a11y.atspi.Action";
    assert_eq!(c.get(path, action, "NActions"), Value::I32(2));
    assert_eq!(
        c.call(path, action, "GetName", vec![Value::I32(0)])
            .unwrap(),
        [Value::str("click")]
    );
    assert_eq!(
        c.call(path, action, "GetName", vec![Value::I32(1)])
            .unwrap(),
        [Value::str("focus")]
    );
    assert_eq!(
        c.call(path, action, "GetName", vec![Value::I32(7)])
            .unwrap(),
        [Value::str("")]
    );
    assert_eq!(
        c.call(path, action, "DoAction", vec![Value::I32(0)])
            .unwrap(),
        [Value::Bool(true)]
    );
    assert_eq!(
        c.call(path, action, "DoAction", vec![Value::I32(9)])
            .unwrap(),
        [Value::Bool(false)]
    );
    assert_eq!(
        c.call(path, "org.a11y.atspi.Component", "GrabFocus", vec![])
            .unwrap(),
        [Value::Bool(true)]
    );
    assert_eq!(
        c.call(
            path,
            "org.a11y.atspi.Component",
            "ScrollTo",
            vec![Value::U32(0)]
        )
        .unwrap(),
        [Value::Bool(true)]
    );
    assert!(service.take_asked());
    // The program takes them in order.
    assert_eq!(
        service.take_actions(),
        [
            (5, AccessAction::Click),
            (5, AccessAction::Focus),
            (5, AccessAction::ScrollIntoView)
        ]
    );
    assert!(service.take_actions().is_empty());
    let Value::Array(_, list) = c
        .call(path, action, "GetActions", vec![])
        .unwrap()
        .remove(0)
    else {
        panic!()
    };
    assert_eq!(list.len(), 2);
}

#[quench_test::test]
fn text_fields_answer_the_text_interface() {
    let (mut c, service) = start();
    service.publish(page());
    let path = "/org/a11y/atspi/accessible/7";
    let text = "org.a11y.atspi.Text";
    assert_eq!(c.get(path, text, "CharacterCount"), Value::I32(15));
    assert_eq!(c.get(path, text, "CaretOffset"), Value::I32(3));
    assert_eq!(
        c.call(path, text, "GetText", vec![Value::I32(0), Value::I32(3)])
            .unwrap(),
        [Value::str("ann")]
    );
    assert_eq!(
        c.call(path, text, "GetText", vec![Value::I32(4), Value::I32(-1)])
            .unwrap(),
        [Value::str("example.org")]
    );
    assert_eq!(
        c.call(path, text, "GetCharacterCount", vec![]).unwrap(),
        [Value::I32(15)]
    );
    assert_eq!(
        c.call(path, text, "GetNSelections", vec![]).unwrap(),
        [Value::I32(0)]
    );
    assert_eq!(
        c.call(path, text, "GetSelection", vec![Value::I32(0)])
            .unwrap(),
        [Value::I32(3), Value::I32(3)]
    );
    assert_eq!(
        c.call(path, text, "GetCaretOffset", vec![]).unwrap(),
        [Value::I32(3)]
    );
    // Static text reads as its name.
    assert_eq!(
        c.call(
            "/org/a11y/atspi/accessible/4",
            text,
            "GetText",
            vec![Value::I32(0), Value::I32(-1)]
        )
        .unwrap(),
        [Value::str("Welcome")]
    );
}

#[quench_test::test]
fn what_is_not_there_is_an_error_not_silence() {
    let (mut c, service) = start();
    service.publish(page());
    let err = c
        .call("/org/a11y/atspi/accessible/99", A, "GetRole", vec![])
        .unwrap_err();
    assert!(err.contains("UnknownObject"), "{err}");
    let err = c.call("/elsewhere", A, "GetRole", vec![]).unwrap_err();
    assert!(err.contains("UnknownObject"), "{err}");
    let err = c
        .call("/org/a11y/atspi/accessible/5", A, "Frobnicate", vec![])
        .unwrap_err();
    assert!(err.contains("UnknownMethod"), "{err}");
    let err = c
        .call(
            "/org/a11y/atspi/accessible/5",
            "org.freedesktop.DBus.Properties",
            "Get",
            vec![Value::str(A), Value::str("Nope")],
        )
        .unwrap_err();
    assert!(err.contains("UnknownProperty"), "{err}");
    // Before anything is published there is a window and no page.
    let (mut c, _service) = start();
    let Value::Array(_, kids) = c
        .call(
            "/org/a11y/atspi/accessible/window",
            A,
            "GetChildren",
            vec![],
        )
        .unwrap()
        .remove(0)
    else {
        panic!()
    };
    assert!(kids.is_empty());
}

#[quench_test::test]
fn focus_changes_are_announced_as_signals() {
    let (mut c, service) = start();
    let mut first = page();
    first.children[3].focused = false;
    service.publish(first);
    // Focus moves to the field: a signal says so.
    service.publish(page());
    let signal = loop {
        let m = c.connection.recv().unwrap();
        if m.kind == Kind::Signal {
            break m;
        }
    };
    assert_eq!(signal.member.as_deref(), Some("StateChanged"));
    assert_eq!(
        signal.interface.as_deref(),
        Some("org.a11y.atspi.Event.Object")
    );
    assert_eq!(signal.path.as_deref(), Some("/org/a11y/atspi/accessible/7"));
    assert_eq!(
        &signal.body[..3],
        &[Value::str("focused"), Value::I32(1), Value::I32(0)]
    );
    // And away again.
    let mut away = page();
    away.children[3].focused = false;
    service.publish(away);
    let lost = loop {
        let m = c.connection.recv().unwrap();
        if m.kind == Kind::Signal {
            break m;
        }
    };
    assert_eq!(lost.body[1], Value::I32(0));
    // The same focus again is not announced twice.
    service.publish(page());
    let _ = c.connection.recv().unwrap();
    assert!(format!("{service:?}").contains("Service"));
}

use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::UnixListener;

/// A bus that says hello and answers what a test tells it to: runs on its own thread until the client leaves.
fn fake_bus(
    path: &std::path::Path,
    unique: &'static str,
    answer: impl Fn(&Message) -> Option<Vec<Value>> + Send + 'static,
) -> std::thread::JoinHandle<()> {
    let listener = UnixListener::bind(path).unwrap();
    std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        authenticate_server(&mut stream).unwrap();
        let mut conn = Connection::new(stream, "bus").unwrap();
        while let Ok(call) = conn.recv() {
            let reply = if call.member.as_deref() == Some("Hello") {
                Message::reply(&call, vec![Value::str(unique)])
            } else {
                match answer(&call) {
                    Some(body) => Message::reply(&call, body),
                    None => Message::error(
                        &call,
                        "org.freedesktop.DBus.Error.ServiceUnknown",
                        "nobody home",
                    ),
                }
            };
            let mut reply = reply;
            conn.send(reply.clone()).unwrap();
            reply.serial = 0;
        }
    })
}

fn temp(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("quench-atspi-{}-{name}", std::process::id()))
}

#[quench_test::test]
fn a_bus_connection_says_hello_and_reports_errors_from_calls() {
    let path = temp("hello");
    let _ = std::fs::remove_file(&path);
    let bus = fake_bus(&path, ":1.77", |call| {
        (call.member.as_deref() == Some("Echo")).then(|| call.body.clone())
    });
    let mut c = Connection::bus(&format!("unix:path={}", path.display())).unwrap();
    assert_eq!(c.name, ":1.77");
    c.set_timeout(Some(Duration::from_secs(5))).unwrap();
    let reply = c
        .call(Message::call(
            Some("x"),
            "/",
            "x.I",
            "Echo",
            vec![Value::I32(5)],
        ))
        .unwrap();
    assert_eq!(reply.body, [Value::I32(5)]);
    let err = c
        .call(Message::call(Some("x"), "/", "x.I", "Nope", vec![]))
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("ServiceUnknown") && err.contains("nobody home"),
        "{err}"
    );
    assert!(format!("{c:?}").contains(":1.77") && format!("{:?}", c.sender()).contains("Sender"));
    drop(c);
    bus.join().unwrap();
    let _ = std::fs::remove_file(&path);
}

#[quench_test::test]
fn addresses_with_escapes_and_abstract_sockets_connect() {
    let path = temp("a b");
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).unwrap();
    let escaped = path.display().to_string().replace(' ', "%20");
    assert!(connect(&format!("unix:guid=abc,path={escaped}")).is_ok());
    assert!(
        connect(&format!("tcp:host=x;unix:path={escaped}")).is_ok(),
        "the second alternative is usable"
    );
    drop(listener);
    let _ = std::fs::remove_file(&path);
    let name = format!("quench-atspi-test-{}", std::process::id());
    let listener = UnixListener::bind_addr(
        &std::os::unix::net::SocketAddr::from_abstract_name(name.as_bytes()).unwrap(),
    )
    .unwrap();
    assert!(connect(&format!("unix:abstract={name}")).is_ok());
    drop(listener);
}

#[quench_test::test]
fn the_service_registers_through_the_session_bus_and_the_accessibility_bus() {
    let (session_path, a11y_path) = (temp("session"), temp("a11y"));
    for p in [&session_path, &a11y_path] {
        let _ = std::fs::remove_file(p);
    }
    let address = format!("unix:path={}", a11y_path.display());
    let session = fake_bus(&session_path, ":1.1", move |call| {
        (call.member.as_deref() == Some("GetAddress")).then(|| vec![Value::Str(address.clone())])
    });
    let a11y = fake_bus(&a11y_path, ":1.2", |call| {
        (call.member.as_deref() == Some("Embed"))
            .then(|| vec![Value::reference(":1.3", "/org/a11y/atspi/accessible/root")])
    });
    let session_connection =
        Connection::bus(&format!("unix:path={}", session_path.display())).unwrap();
    let service = Service::start_on(session_connection, "registered").unwrap();
    service.publish(page());
    assert!(!service.take_asked());
    assert!(format!("{:?}", service.shared()).contains(":1.2"));
    drop(service);
    // The fake buses end when their client goes; the service's thread holds the connection, so do not wait.
    let _ = (session, a11y);
    for p in [&session_path, &a11y_path] {
        let _ = std::fs::remove_file(p);
    }
}

#[quench_test::test]
fn every_role_has_a_role_number_and_a_name_and_states_are_complete() {
    let roles = [
        (Role::Document, 95, "document web"),
        (Role::Heading, 83, "heading"),
        (Role::Paragraph, 73, "paragraph"),
        (Role::Text, 116, "static"),
        (Role::Button, 43, "push button"),
        (Role::Link, 88, "link"),
        (Role::Checkbox, 7, "check box"),
        (Role::Radio, 44, "radio button"),
        (Role::Switch, 62, "toggle button"),
        (Role::TextBox, 79, "entry"),
        (Role::List, 31, "list"),
        (Role::ListItem, 32, "list item"),
        (Role::Tab, 37, "page tab"),
        (Role::TabList, 38, "page tab list"),
        (Role::TabPanel, 39, "panel"),
        (Role::Dialog, 16, "dialog"),
        (Role::Navigation, 110, "landmark"),
        (Role::Status, 54, "status bar"),
        (Role::Alert, 2, "alert"),
        (Role::Image, 27, "image"),
        (Role::Form, 87, "form"),
        (Role::Group, 85, "section"),
    ];
    let kids: Vec<AccessNode> = roles
        .iter()
        .enumerate()
        .map(|(i, (r, _, _))| node(i + 1, *r, "n", (0.0, 0.0, 1.0, 1.0), Vec::new()))
        .collect();
    let mut expanded = node(100, Role::Button, "More", (0.0, 0.0, 1.0, 1.0), Vec::new());
    expanded.expanded = Some(true);
    expanded.selected = Some(true);
    let mut readonly = node(101, Role::TextBox, "Ro", (0.0, 0.0, 1.0, 1.0), Vec::new());
    readonly.read_only = true;
    let mut all = kids;
    all.push(expanded);
    all.push(readonly);
    let (mut c, service) = start();
    service.publish(node(0, Role::Document, "", (0.0, 0.0, 10.0, 10.0), all));
    for (i, (_, number, name)) in roles.iter().enumerate() {
        let path = format!("/org/a11y/atspi/accessible/{}", i + 1);
        assert_eq!(
            c.call(&path, A, "GetRole", vec![]).unwrap(),
            [Value::U32(*number)],
            "{name}"
        );
        assert_eq!(
            c.call(&path, A, "GetRoleName", vec![]).unwrap(),
            [Value::str(name)]
        );
    }
    let bit = |s: u32| 1u64 << s;
    let more = states(&mut c, 100);
    assert!(
        more & bit(9) != 0 && more & bit(10) != 0 && more & bit(20) != 0,
        "expandable, expanded, pressed: {more:b}"
    );
    assert_eq!(
        c.call("/org/a11y/atspi/accessible/100", A, "GetRole", vec![])
            .unwrap(),
        [Value::U32(62)],
        "a toggle button"
    );
    assert!(
        states(&mut c, 101) & bit(43) != 0 && states(&mut c, 101) & bit(7) == 0,
        "read only, not editable"
    );
    let window = states(&mut c, usize::MAX);
    let _ = window;
}

quench_test::main!();
