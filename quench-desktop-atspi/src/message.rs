//! D-Bus messages: the header, its fields, and the body.

use std::io::{self, Read, Write};

use crate::value::{Value, pad, split_signature};

/// What kind of message it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    MethodCall,
    MethodReturn,
    Error,
    Signal,
}

impl Kind {
    const fn code(self) -> u8 {
        match self {
            Self::MethodCall => 1,
            Self::MethodReturn => 2,
            Self::Error => 3,
            Self::Signal => 4,
        }
    }

    const fn of(code: u8) -> Option<Self> {
        Some(match code {
            1 => Self::MethodCall,
            2 => Self::MethodReturn,
            3 => Self::Error,
            4 => Self::Signal,
            _ => return None,
        })
    }
}

/// A message, as sent or received. Fields that a kind does not use are `None`.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub kind: Kind,
    pub serial: u32,
    pub path: Option<String>,
    pub interface: Option<String>,
    pub member: Option<String>,
    pub error_name: Option<String>,
    pub reply_serial: Option<u32>,
    pub destination: Option<String>,
    pub sender: Option<String>,
    pub body: Vec<Value>,
}

const NO_REPLY_EXPECTED: u8 = 1;

impl Message {
    const fn empty(kind: Kind) -> Self {
        Self {
            kind,
            serial: 0,
            path: None,
            interface: None,
            member: None,
            error_name: None,
            reply_serial: None,
            destination: None,
            sender: None,
            body: Vec::new(),
        }
    }

    /// A method call.
    #[must_use]
    pub fn call(
        destination: Option<&str>,
        path: &str,
        interface: &str,
        member: &str,
        body: Vec<Value>,
    ) -> Self {
        Self {
            destination: destination.map(String::from),
            path: Some(path.into()),
            interface: Some(interface.into()),
            member: Some(member.into()),
            body,
            ..Self::empty(Kind::MethodCall)
        }
    }

    /// The reply to `call`.
    #[must_use]
    pub fn reply(call: &Self, body: Vec<Value>) -> Self {
        Self {
            reply_serial: Some(call.serial),
            destination: call.sender.clone(),
            body,
            ..Self::empty(Kind::MethodReturn)
        }
    }

    /// The error answer to `call`.
    #[must_use]
    pub fn error(call: &Self, name: &str, text: &str) -> Self {
        Self {
            reply_serial: Some(call.serial),
            destination: call.sender.clone(),
            error_name: Some(name.into()),
            body: vec![Value::str(text)],
            ..Self::empty(Kind::Error)
        }
    }

    /// A signal.
    #[must_use]
    pub fn signal(path: &str, interface: &str, member: &str, body: Vec<Value>) -> Self {
        Self {
            path: Some(path.into()),
            interface: Some(interface.into()),
            member: Some(member.into()),
            body,
            ..Self::empty(Kind::Signal)
        }
    }

    /// The wire form, little-endian.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut body = Vec::new();
        for v in &self.body {
            v.encode(&mut body);
        }
        let signature: String = self.body.iter().map(Value::signature).collect();
        let mut out = vec![
            b'l',
            self.kind.code(),
            if self.kind == Kind::Signal {
                NO_REPLY_EXPECTED
            } else {
                0
            },
            1,
        ];
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&self.serial.to_le_bytes());
        // The header fields: an array of (code, variant).
        let field = |code: u8, value: Value| {
            Value::Struct(vec![Value::Byte(code), Value::Variant(Box::new(value))])
        };
        let mut fields = Vec::new();
        if let Some(p) = &self.path {
            fields.push(field(1, Value::path(p)));
        }
        if let Some(i) = &self.interface {
            fields.push(field(2, Value::str(i)));
        }
        if let Some(m) = &self.member {
            fields.push(field(3, Value::str(m)));
        }
        if let Some(e) = &self.error_name {
            fields.push(field(4, Value::str(e)));
        }
        if let Some(r) = self.reply_serial {
            fields.push(field(5, Value::U32(r)));
        }
        if let Some(d) = &self.destination {
            fields.push(field(6, Value::str(d)));
        }
        if let Some(s) = &self.sender {
            fields.push(field(7, Value::str(s)));
        }
        if !signature.is_empty() {
            fields.push(field(8, Value::Signature(signature)));
        }
        Value::Array("(yv)".into(), fields).encode(&mut out);
        pad(&mut out, 8);
        out.extend_from_slice(&body);
        out
    }

    /// Reads one message from `stream`.
    ///
    /// # Errors
    /// The stream's, or a message that is not well formed.
    pub fn read(stream: &mut impl Read) -> io::Result<Self> {
        let mut head = [0u8; 16];
        stream.read_exact(&mut head)?;
        if head[0] != b'l' {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "only little-endian messages are supported",
            ));
        }
        let body_length = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
        let serial = u32::from_le_bytes([head[8], head[9], head[10], head[11]]);
        let fields_length = u32::from_le_bytes([head[12], head[13], head[14], head[15]]) as usize;
        if fields_length > 1 << 16 || body_length > 1 << 24 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "message too long",
            ));
        }
        let padded = fields_length.next_multiple_of(8);
        let mut rest = vec![0u8; padded + body_length];
        stream.read_exact(&mut rest)?;
        let mut whole = head.to_vec();
        whole.extend_from_slice(&rest);
        Self::decode(
            &whole,
            Kind::of(head[1]).ok_or_else(|| {
                io::Error::new(io::ErrorKind::InvalidData, "unknown message type")
            })?,
            serial,
            12,
            body_length,
        )
    }

    fn decode(
        whole: &[u8],
        kind: Kind,
        serial: u32,
        fields_at: usize,
        body_length: usize,
    ) -> io::Result<Self> {
        let mut at = fields_at;
        let fields = Value::decode("a(yv)", whole, &mut at)?;
        let Value::Array(_, items) = fields else {
            unreachable!()
        };
        let mut m = Self::empty(kind);
        m.serial = serial;
        let mut signature = String::new();
        for item in items {
            let Value::Struct(parts) = item else { continue };
            let [Value::Byte(code), Value::Variant(v)] = parts.as_slice() else {
                continue;
            };
            match (code, v.as_ref()) {
                (1, Value::Path(p)) => m.path = Some(p.clone()),
                (2, Value::Str(s)) => m.interface = Some(s.clone()),
                (3, Value::Str(s)) => m.member = Some(s.clone()),
                (4, Value::Str(s)) => m.error_name = Some(s.clone()),
                (5, Value::U32(n)) => m.reply_serial = Some(*n),
                (6, Value::Str(s)) => m.destination = Some(s.clone()),
                (7, Value::Str(s)) => m.sender = Some(s.clone()),
                (8, Value::Signature(s)) => signature = s.clone(),
                _ => {}
            }
        }
        let mut at = at.next_multiple_of(8);
        if whole.len() != at + body_length {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "the body is not as long as the header says",
            ));
        }
        for t in split_signature(&signature)? {
            m.body.push(Value::decode(&t, whole, &mut at)?);
        }
        Ok(m)
    }

    /// Writes the message to `stream`.
    ///
    /// # Errors
    /// The stream's.
    pub fn write(&self, stream: &mut impl Write) -> io::Result<()> {
        stream.write_all(&self.encode())
    }

    /// The message as received from the bytes of `encode`, for tests.
    ///
    /// # Errors
    /// As [`read`](Self::read).
    pub fn from_bytes(bytes: &[u8]) -> io::Result<Self> {
        Self::read(&mut &bytes[..])
    }
}
