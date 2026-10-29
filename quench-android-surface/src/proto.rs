//! The messages between the Kotlin host and this program, over the program's standard input and output.
//!
//! Every message is a length (4 bytes), a kind (1 byte) and a payload, all numbers big-endian (what Java's
//! `DataInputStream` reads). Strings are a 2-byte length and UTF-8 (4 bytes for the longer ones).

use std::io::{self, Read, Write};

use quench_ui_native::{AccessAction, AccessNode, Role};

/// The largest message accepted: a frame of 16384 x 4096 pixels is 256 MiB, which is not one.
const MAX_MESSAGE: usize = 1 << 26;

/// What the host tells the program.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// The surface has this size in pixels and this density (device pixels per CSS pixel), with the system bars'
    /// insets in pixels: top, right, bottom, left.
    Resize {
        width: u32,
        height: u32,
        density: f32,
        insets: [f32; 4],
    },
    /// A finger: 0 down, 1 move, 2 up, 3 cancelled.
    Touch { action: u8, x: f32, y: f32 },
    /// A key: `name` is the DOM `key` value; `mods` has 1 for shift, 2 control, 4 alt, 8 meta.
    Key { down: bool, mods: u8, name: String },
    /// Text the input method committed.
    Text(String),
    /// The input method's composition (`cursor` in UTF-16 units, negative for none).
    Compose { text: String, cursor: i32 },
    /// Delete this many UTF-16 units before and after the caret.
    Delete { before: u32, after: u32 },
    /// An assistive technology asked for `action` on the node.
    Access { id: u32, action: u8 },
    /// The system is dark (or light).
    Scheme { dark: bool },
    /// The back gesture or button.
    Back,
    /// The app went to the background (`false`) or came back.
    Visible(bool),
    /// The system clipboard's text (sent when it changes and when the app comes back to the front).
    Clipboard(String),
    /// A plugin's answer to a call, or an event it pushed: a JSON envelope, `{"id","ok"}`, `{"id","err"}` or
    /// `{"event","plugin","data"}`.
    Reply(String),
}

fn short() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "a message shorter than its kind needs",
    )
}

/// A cursor over a message's payload.
struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take(&mut self, n: usize) -> io::Result<&[u8]> {
        if self.0.len() < n {
            return Err(short());
        }
        let (head, tail) = self.0.split_at(n);
        self.0 = tail;
        Ok(head)
    }

    fn u8(&mut self) -> io::Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn u32(&mut self) -> io::Result<u32> {
        Ok(u32::from_be_bytes(
            self.take(4)?.try_into().map_err(|_| short())?,
        ))
    }

    fn i32(&mut self) -> io::Result<i32> {
        Ok(i32::from_be_bytes(
            self.take(4)?.try_into().map_err(|_| short())?,
        ))
    }

    fn f32(&mut self) -> io::Result<f32> {
        Ok(f32::from_be_bytes(
            self.take(4)?.try_into().map_err(|_| short())?,
        ))
    }

    /// A string with a 4-byte length.
    fn long_string(&mut self) -> io::Result<String> {
        let n = self.u32()? as usize;
        String::from_utf8(self.take(n)?.to_vec())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "text that is not UTF-8"))
    }

    fn string(&mut self) -> io::Result<String> {
        let n = usize::from(u16::from_be_bytes(
            self.take(2)?.try_into().map_err(|_| short())?,
        ));
        String::from_utf8(self.take(n)?.to_vec())
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "text that is not UTF-8"))
    }
}

/// Reads the next event; `Ok(None)` when the host closed the pipe.
///
/// # Errors
/// The stream's, or a message that is malformed or too long.
pub fn read_event(input: &mut impl Read) -> io::Result<Option<Event>> {
    let mut length = [0u8; 4];
    match input.read_exact(&mut length) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e),
    }
    let length = u32::from_be_bytes(length) as usize;
    if length == 0 || length > MAX_MESSAGE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "a message of that length is not one",
        ));
    }
    let mut body = vec![0u8; length];
    input.read_exact(&mut body)?;
    let kind = body[0];
    let mut r = Reader(&body[1..]);
    Ok(Some(match kind {
        1 => Event::Resize {
            width: r.u32()?,
            height: r.u32()?,
            density: r.f32()?,
            insets: [r.f32()?, r.f32()?, r.f32()?, r.f32()?],
        },
        2 => Event::Touch {
            action: r.u8()?,
            x: r.f32()?,
            y: r.f32()?,
        },
        3 => Event::Key {
            down: r.u8()? != 0,
            mods: r.u8()?,
            name: r.string()?,
        },
        4 => Event::Text(r.string()?),
        5 => Event::Compose {
            text: r.string()?,
            cursor: r.i32()?,
        },
        6 => Event::Delete {
            before: r.u32()?,
            after: r.u32()?,
        },
        7 => Event::Access {
            id: r.u32()?,
            action: r.u8()?,
        },
        8 => Event::Scheme { dark: r.u8()? != 0 },
        9 => Event::Back,
        10 => Event::Visible(r.u8()? != 0),
        11 => Event::Clipboard(r.long_string()?),
        12 => Event::Reply(r.long_string()?),
        _ => {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("unknown message kind {kind}"),
            ));
        }
    }))
}

/// Encodes an event (what the host does, in Rust: for tests and tools).
#[must_use]
pub fn encode_event(event: &Event) -> Vec<u8> {
    let mut body = Vec::new();
    let put_string = |body: &mut Vec<u8>, s: &str| {
        body.extend_from_slice(&(s.len().min(65535) as u16).to_be_bytes());
        body.extend_from_slice(&s.as_bytes()[..s.len().min(65535)]);
    };
    match event {
        Event::Resize {
            width,
            height,
            density,
            insets,
        } => {
            body.push(1);
            body.extend_from_slice(&width.to_be_bytes());
            body.extend_from_slice(&height.to_be_bytes());
            body.extend_from_slice(&density.to_be_bytes());
            for i in insets {
                body.extend_from_slice(&i.to_be_bytes());
            }
        }
        Event::Touch { action, x, y } => {
            body.push(2);
            body.push(*action);
            body.extend_from_slice(&x.to_be_bytes());
            body.extend_from_slice(&y.to_be_bytes());
        }
        Event::Key { down, mods, name } => {
            body.extend_from_slice(&[3, u8::from(*down), *mods]);
            put_string(&mut body, name);
        }
        Event::Text(t) => {
            body.push(4);
            put_string(&mut body, t);
        }
        Event::Compose { text, cursor } => {
            body.push(5);
            put_string(&mut body, text);
            body.extend_from_slice(&cursor.to_be_bytes());
        }
        Event::Delete { before, after } => {
            body.push(6);
            body.extend_from_slice(&before.to_be_bytes());
            body.extend_from_slice(&after.to_be_bytes());
        }
        Event::Access { id, action } => {
            body.push(7);
            body.extend_from_slice(&id.to_be_bytes());
            body.push(*action);
        }
        Event::Scheme { dark } => body.extend_from_slice(&[8, u8::from(*dark)]),
        Event::Back => body.push(9),
        Event::Visible(v) => body.extend_from_slice(&[10, u8::from(*v)]),
        Event::Clipboard(t) | Event::Reply(t) => {
            body.push(if matches!(event, Event::Clipboard(_)) {
                11
            } else {
                12
            });
            body.extend_from_slice(&(t.len() as u32).to_be_bytes());
            body.extend_from_slice(t.as_bytes());
        }
    }
    let mut out = (body.len() as u32).to_be_bytes().to_vec();
    out.extend(body);
    out
}

/// What the program tells the host.
#[derive(Debug, Clone, PartialEq)]
pub enum Out<'a> {
    /// A frame: RGBA, premultiplied, `width * height * 4` bytes.
    Frame {
        width: u32,
        height: u32,
        rgba: &'a [u8],
    },
    /// Show or hide the soft keyboard.
    Keyboard(bool),
    /// The text field the input method types into: its text, the caret and the other end of the selection (UTF-16
    /// units) and the caret's rectangle in pixels.
    Ime {
        text: &'a str,
        cursor: i32,
        anchor: i32,
        rect: [f32; 4],
    },
    /// The accessibility tree, flattened (see [`encode_tree`]).
    Tree(&'a [u8]),
    /// The back gesture was used by the page.
    BackHandled,
    /// The app is finished.
    Exit,
    /// Text for the system clipboard.
    Clipboard(&'a str),
    /// A call to a plugin of the Kotlin host: a JSON envelope, `{"id","plugin","method","args"}`.
    Call(&'a str),
}

/// Writes a message.
///
/// # Errors
/// The stream's.
pub fn write_out(output: &mut impl Write, message: &Out<'_>) -> io::Result<()> {
    // A frame is megabytes: its pixels go straight to the stream instead of being copied into a message first.
    if let Out::Frame {
        width,
        height,
        rgba,
    } = message
    {
        output.write_all(&((1 + 4 + 4 + rgba.len()) as u32).to_be_bytes())?;
        output.write_all(&[1])?;
        output.write_all(&width.to_be_bytes())?;
        output.write_all(&height.to_be_bytes())?;
        output.write_all(rgba)?;
        return output.flush();
    }
    let mut body: Vec<u8> = Vec::new();
    match message {
        Out::Frame {
            width,
            height,
            rgba,
        } => {
            body.push(1);
            body.extend_from_slice(&width.to_be_bytes());
            body.extend_from_slice(&height.to_be_bytes());
            body.extend_from_slice(rgba);
        }
        Out::Keyboard(show) => body.extend_from_slice(&[2, u8::from(*show)]),
        Out::Ime {
            text,
            cursor,
            anchor,
            rect,
        } => {
            body.push(3);
            body.extend_from_slice(&(text.len() as u32).to_be_bytes());
            body.extend_from_slice(text.as_bytes());
            body.extend_from_slice(&cursor.to_be_bytes());
            body.extend_from_slice(&anchor.to_be_bytes());
            for v in rect {
                body.extend_from_slice(&v.to_be_bytes());
            }
        }
        Out::Tree(bytes) => {
            body.push(4);
            body.extend_from_slice(bytes);
        }
        Out::BackHandled => body.push(5),
        Out::Exit => body.push(6),
        Out::Clipboard(text) | Out::Call(text) => {
            body.push(if matches!(message, Out::Clipboard(_)) {
                7
            } else {
                8
            });
            body.extend_from_slice(&(text.len() as u32).to_be_bytes());
            body.extend_from_slice(text.as_bytes());
        }
    }
    output.write_all(&(body.len() as u32).to_be_bytes())?;
    output.write_all(&body)?;
    output.flush()
}

const fn role_code(role: Role) -> u8 {
    match role {
        Role::Document => 0,
        Role::Heading => 1,
        Role::Paragraph => 2,
        Role::Text => 3,
        Role::Button => 4,
        Role::Link => 5,
        Role::Checkbox => 6,
        Role::Radio => 7,
        Role::Switch => 8,
        Role::TextBox => 9,
        Role::List => 10,
        Role::ListItem => 11,
        Role::Tab => 12,
        Role::TabList => 13,
        Role::TabPanel => 14,
        Role::Dialog => 15,
        Role::Navigation => 16,
        Role::Status => 17,
        Role::Alert => 18,
        Role::Image => 19,
        Role::Form => 20,
        Role::Group => 21,
        Role::Slider => 22,
        Role::ProgressBar => 23,
    }
}

/// The action codes of [`Event::Access`] and of the flags in [`encode_tree`].
#[must_use]
pub const fn action_of(code: u8) -> Option<AccessAction> {
    match code {
        0 => Some(AccessAction::Click),
        1 => Some(AccessAction::Focus),
        2 => Some(AccessAction::ScrollIntoView),
        _ => None,
    }
}

/// The accessibility tree as a flat list in document order.
///
/// A count, then for each node its id, its parent's id
/// (`0xFFFF_FFFF` for the root), a role code, flags (bit 0 focusable, 1 focused, 2 disabled, 3 checkable, 4
/// checked, 5 selectable, 6 selected, 7 expandable, 8 expanded, 9 required, 10 read-only, 11 can click, 12 can
/// focus, 13 can scroll), the heading level, the bounds (`x y w h` as floats), and the name, value and
/// description as strings.
#[must_use]
pub fn encode_tree(root: &AccessNode) -> Vec<u8> {
    fn put(out: &mut Vec<u8>, s: &str) {
        let s = &s.as_bytes()[..s.len().min(4000)];
        let mut end = s.len();
        while std::str::from_utf8(&s[..end]).is_err() {
            end -= 1;
        }
        out.extend_from_slice(&(end as u16).to_be_bytes());
        out.extend_from_slice(&s[..end]);
    }
    fn walk(n: &AccessNode, parent: u32, out: &mut Vec<u8>, count: &mut u32) {
        *count += 1;
        out.extend_from_slice(&(n.id as u32).to_be_bytes());
        out.extend_from_slice(&parent.to_be_bytes());
        out.push(role_code(n.role));
        let mut flags: u16 = 0;
        let mut set = |bit: u16, on: bool| flags |= u16::from(on) << bit;
        set(0, n.focusable);
        set(1, n.focused);
        set(2, n.disabled);
        set(3, n.checked.is_some());
        set(4, n.checked == Some(true));
        set(5, n.selected.is_some());
        set(6, n.selected == Some(true));
        set(7, n.expanded.is_some());
        set(8, n.expanded == Some(true));
        set(9, n.required);
        set(10, n.read_only);
        set(11, n.actions.contains(&AccessAction::Click));
        set(12, n.actions.contains(&AccessAction::Focus));
        set(13, n.actions.contains(&AccessAction::ScrollIntoView));
        out.extend_from_slice(&flags.to_be_bytes());
        out.push(n.level);
        for v in [n.bounds.0, n.bounds.1, n.bounds.2, n.bounds.3] {
            out.extend_from_slice(&v.to_be_bytes());
        }
        put(out, &n.name);
        put(out, n.value.as_deref().unwrap_or(""));
        put(out, &n.description);
        // A slider or progress bar: a flag, then the smallest, the largest and the current value (not a number while
        // a progress bar is indeterminate).
        match n.range {
            Some((min, max, now)) => {
                out.push(1);
                for v in [min as f32, max as f32, now.map_or(f32::NAN, |v| v as f32)] {
                    out.extend_from_slice(&v.to_be_bytes());
                }
            }
            None => out.push(0),
        }
        for c in &n.children {
            walk(c, n.id as u32, out, count);
        }
    }
    let mut body = Vec::new();
    let mut count = 0;
    walk(root, u32::MAX, &mut body, &mut count);
    let mut out = count.to_be_bytes().to_vec();
    out.extend(body);
    out
}
