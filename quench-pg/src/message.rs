//! The PostgreSQL v3 wire protocol: encoding the messages a client sends and
//! decoding the ones the server answers with.

use crate::error::{Error, Result, ServerError};

/// Protocol version 3.0.
const PROTOCOL_VERSION: u32 = 196_608;
/// The magic number of an `SSLRequest`.
const SSL_REQUEST_CODE: u32 = 80_877_103;

/// A message under construction. Tagged messages are `tag, length, body`; the
/// untagged startup-phase ones are `length, body`. The length counts itself and the body.
struct Builder {
    buf: Vec<u8>,
    tagged: bool,
}

impl Builder {
    fn tagged(tag: u8) -> Self {
        let mut buf = Vec::with_capacity(64);
        buf.push(tag);
        buf.extend_from_slice(&[0; 4]);
        Self { buf, tagged: true }
    }

    fn untagged() -> Self {
        Self {
            buf: vec![0; 4],
            tagged: false,
        }
    }

    fn u8(&mut self, value: u8) -> &mut Self {
        self.buf.push(value);
        self
    }

    fn i16(&mut self, value: i16) -> &mut Self {
        self.buf.extend_from_slice(&value.to_be_bytes());
        self
    }

    fn u32(&mut self, value: u32) -> &mut Self {
        self.buf.extend_from_slice(&value.to_be_bytes());
        self
    }

    fn i32(&mut self, value: i32) -> &mut Self {
        self.buf.extend_from_slice(&value.to_be_bytes());
        self
    }

    fn cstr(&mut self, text: &str) -> &mut Self {
        self.buf.extend_from_slice(text.as_bytes());
        self.buf.push(0);
        self
    }

    fn bytes(&mut self, data: &[u8]) -> &mut Self {
        self.buf.extend_from_slice(data);
        self
    }

    fn finish(mut self) -> Vec<u8> {
        let start = usize::from(self.tagged);
        let len = u32::try_from(self.buf.len() - start).unwrap_or(u32::MAX);
        self.buf[start..start + 4].copy_from_slice(&len.to_be_bytes());
        self.buf
    }
}

/// `StartupMessage`: protocol version and `name=value` parameters (`user` is required).
#[must_use]
pub fn startup(params: &[(&str, &str)]) -> Vec<u8> {
    let mut b = Builder::untagged();
    b.u32(PROTOCOL_VERSION);
    for (name, value) in params {
        b.cstr(name).cstr(value);
    }
    b.u8(0);
    b.finish()
}

/// `SSLRequest`: asks whether the server will speak TLS (it answers one byte, `S` or `N`).
#[must_use]
pub fn ssl_request() -> Vec<u8> {
    let mut b = Builder::untagged();
    b.u32(SSL_REQUEST_CODE);
    b.finish()
}

/// `PasswordMessage` for cleartext authentication.
#[must_use]
pub fn password(password: &str) -> Vec<u8> {
    let mut b = Builder::tagged(b'p');
    b.cstr(password);
    b.finish()
}

/// `SASLInitialResponse`: the chosen mechanism and the client-first message.
#[must_use]
pub fn sasl_initial_response(mechanism: &str, data: &[u8]) -> Vec<u8> {
    let mut b = Builder::tagged(b'p');
    b.cstr(mechanism)
        .i32(i32::try_from(data.len()).unwrap_or(i32::MAX))
        .bytes(data);
    b.finish()
}

/// `SASLResponse`: a later client message of the exchange.
#[must_use]
pub fn sasl_response(data: &[u8]) -> Vec<u8> {
    let mut b = Builder::tagged(b'p');
    b.bytes(data);
    b.finish()
}

/// `Query`: the simple query protocol, possibly several statements.
#[must_use]
pub fn query(sql: &str) -> Vec<u8> {
    let mut b = Builder::tagged(b'Q');
    b.cstr(sql);
    b.finish()
}

/// `Parse`: prepares `sql` as statement `name` (empty for the unnamed one).
/// `param_types` are type OIDs; 0 lets the server infer.
#[must_use]
pub fn parse(name: &str, sql: &str, param_types: &[u32]) -> Vec<u8> {
    let mut b = Builder::tagged(b'P');
    b.cstr(name)
        .cstr(sql)
        .i16(i16::try_from(param_types.len()).unwrap_or(i16::MAX));
    for oid in param_types {
        b.u32(*oid);
    }
    b.finish()
}

/// `Bind`: binds text-format parameters (`None` is NULL) to a statement as `portal`,
/// asking for every result column in text format.
#[must_use]
pub fn bind(portal: &str, statement: &str, params: &[Option<String>]) -> Vec<u8> {
    let mut b = Builder::tagged(b'B');
    b.cstr(portal).cstr(statement);
    // One parameter-format code (0 = text) applies to all parameters; likewise one result-format code.
    b.i16(1).i16(0);
    b.i16(i16::try_from(params.len()).unwrap_or(i16::MAX));
    for param in params {
        match param {
            None => {
                b.i32(-1);
            }
            Some(text) => {
                b.i32(i32::try_from(text.len()).unwrap_or(i32::MAX))
                    .bytes(text.as_bytes());
            }
        }
    }
    b.i16(1).i16(0);
    b.finish()
}

/// `Describe` a statement (`b'S'`) or portal (`b'P'`).
#[must_use]
pub fn describe(kind: u8, name: &str) -> Vec<u8> {
    let mut b = Builder::tagged(b'D');
    b.u8(kind).cstr(name);
    b.finish()
}

/// `Execute` a portal; `max_rows` of 0 means all.
#[must_use]
pub fn execute(portal: &str, max_rows: u32) -> Vec<u8> {
    let mut b = Builder::tagged(b'E');
    b.cstr(portal).u32(max_rows);
    b.finish()
}

/// `Close` a statement (`b'S'`) or portal (`b'P'`).
#[must_use]
pub fn close(kind: u8, name: &str) -> Vec<u8> {
    let mut b = Builder::tagged(b'C');
    b.u8(kind).cstr(name);
    b.finish()
}

/// `Sync`: ends an extended-query cycle; the server answers `ReadyForQuery`.
#[must_use]
pub fn sync() -> Vec<u8> {
    Builder::tagged(b'S').finish()
}

/// `Terminate`: the polite goodbye.
#[must_use]
pub fn terminate() -> Vec<u8> {
    Builder::tagged(b'X').finish()
}

/// One column of a `RowDescription`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDescription {
    pub name: String,
    pub table_oid: u32,
    pub column_id: i16,
    pub type_oid: u32,
    pub type_size: i16,
    pub type_modifier: i32,
    /// 0 text, 1 binary.
    pub format: i16,
}

/// What the server can send.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Backend {
    AuthenticationOk,
    AuthenticationCleartextPassword,
    AuthenticationMd5Password([u8; 4]),
    AuthenticationSasl(Vec<String>),
    AuthenticationSaslContinue(Vec<u8>),
    AuthenticationSaslFinal(Vec<u8>),
    /// An authentication request of a kind this client does not know (Kerberos, GSS, SSPI, ...).
    AuthenticationOther(u32),
    ParameterStatus {
        name: String,
        value: String,
    },
    BackendKeyData {
        process_id: u32,
        secret_key: u32,
    },
    /// The transaction status: `I` idle, `T` in a transaction, `E` in a failed transaction.
    ReadyForQuery(u8),
    RowDescription(Vec<FieldDescription>),
    DataRow(Vec<Option<Vec<u8>>>),
    CommandComplete(String),
    EmptyQueryResponse,
    ParseComplete,
    BindComplete,
    CloseComplete,
    NoData,
    PortalSuspended,
    ParameterDescription(Vec<u32>),
    ErrorResponse(ServerError),
    NoticeResponse(ServerError),
    NotificationResponse {
        process_id: u32,
        channel: String,
        payload: String,
    },
    /// A message type this client does not interpret.
    Unknown(u8),
}

/// A cursor over a message body.
struct Reader<'a> {
    data: &'a [u8],
}

impl<'a> Reader<'a> {
    const fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        if self.data.len() < n {
            return Err(Error::Protocol("message shorter than its contents".into()));
        }
        let (head, rest) = self.data.split_at(n);
        self.data = rest;
        Ok(head)
    }

    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }

    fn i16(&mut self) -> Result<i16> {
        let b = self.take(2)?;
        Ok(i16::from_be_bytes([b[0], b[1]]))
    }

    fn u32(&mut self) -> Result<u32> {
        let b = self.take(4)?;
        Ok(u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn i32(&mut self) -> Result<i32> {
        let b = self.take(4)?;
        Ok(i32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    }

    fn cstr(&mut self) -> Result<String> {
        let end = self
            .data
            .iter()
            .position(|&b| b == 0)
            .ok_or_else(|| Error::Protocol("unterminated string in message".into()))?;
        let text = String::from_utf8_lossy(&self.data[..end]).into_owned();
        self.data = &self.data[end + 1..];
        Ok(text)
    }

    const fn rest(&self) -> &'a [u8] {
        self.data
    }
}

/// Decodes one backend message from its type byte and body.
///
/// # Errors
///
/// [`Error::Protocol`] when the body is shorter than its fields claim.
pub fn decode(tag: u8, body: &[u8]) -> Result<Backend> {
    let mut r = Reader::new(body);
    Ok(match tag {
        b'R' => match r.u32()? {
            0 => Backend::AuthenticationOk,
            3 => Backend::AuthenticationCleartextPassword,
            5 => {
                let salt = r.take(4)?;
                Backend::AuthenticationMd5Password([salt[0], salt[1], salt[2], salt[3]])
            }
            10 => {
                let mut mechanisms = Vec::new();
                loop {
                    let name = r.cstr()?;
                    if name.is_empty() {
                        break;
                    }
                    mechanisms.push(name);
                }
                Backend::AuthenticationSasl(mechanisms)
            }
            11 => Backend::AuthenticationSaslContinue(r.rest().to_vec()),
            12 => Backend::AuthenticationSaslFinal(r.rest().to_vec()),
            other => Backend::AuthenticationOther(other),
        },
        b'S' => Backend::ParameterStatus {
            name: r.cstr()?,
            value: r.cstr()?,
        },
        b'K' => Backend::BackendKeyData {
            process_id: r.u32()?,
            secret_key: r.u32()?,
        },
        b'Z' => Backend::ReadyForQuery(r.u8()?),
        b'T' => {
            let count = r.i16()?;
            let mut fields = Vec::with_capacity(usize::try_from(count).unwrap_or(0));
            for _ in 0..count {
                fields.push(FieldDescription {
                    name: r.cstr()?,
                    table_oid: r.u32()?,
                    column_id: r.i16()?,
                    type_oid: r.u32()?,
                    type_size: r.i16()?,
                    type_modifier: r.i32()?,
                    format: r.i16()?,
                });
            }
            Backend::RowDescription(fields)
        }
        b'D' => {
            let count = r.i16()?;
            let mut values = Vec::with_capacity(usize::try_from(count).unwrap_or(0));
            for _ in 0..count {
                let len = r.i32()?;
                values.push(match usize::try_from(len) {
                    Ok(len) => Some(r.take(len)?.to_vec()),
                    Err(_) if len == -1 => None,
                    Err(_) => return Err(Error::Protocol(format!("negative column length {len}"))),
                });
            }
            Backend::DataRow(values)
        }
        b'C' => Backend::CommandComplete(r.cstr()?),
        b'I' => Backend::EmptyQueryResponse,
        b'1' => Backend::ParseComplete,
        b'2' => Backend::BindComplete,
        b'3' => Backend::CloseComplete,
        b'n' => Backend::NoData,
        b's' => Backend::PortalSuspended,
        b't' => {
            let count = r.i16()?;
            let mut oids = Vec::with_capacity(usize::try_from(count).unwrap_or(0));
            for _ in 0..count {
                oids.push(r.u32()?);
            }
            Backend::ParameterDescription(oids)
        }
        b'E' => Backend::ErrorResponse(ServerError::parse(body)),
        b'N' => Backend::NoticeResponse(ServerError::parse(body)),
        b'A' => Backend::NotificationResponse {
            process_id: r.u32()?,
            channel: r.cstr()?,
            payload: r.cstr()?,
        },
        other => Backend::Unknown(other),
    })
}

/// The number of rows a `CommandComplete` tag reports (`INSERT 0 3`, `UPDATE 2`, `SELECT 5`, ...); 0 if it has none.
#[must_use]
pub fn rows_affected(tag: &str) -> u64 {
    tag.rsplit(' ')
        .next()
        .and_then(|n| n.parse().ok())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(build: impl FnOnce(&mut Vec<u8>)) -> Vec<u8> {
        let mut out = Vec::new();
        build(&mut out);
        out
    }

    fn cstr(out: &mut Vec<u8>, text: &str) {
        out.extend_from_slice(text.as_bytes());
        out.push(0);
    }

    #[quench_test::test]
    fn startup_and_ssl_request_wire_forms() {
        let ssl = ssl_request();
        assert_eq!(ssl, [0, 0, 0, 8, 4, 210, 22, 47]);
        let startup = startup(&[("user", "bob"), ("database", "db")]);
        assert_eq!(&startup[4..8], &[0, 3, 0, 0], "protocol 3.0");
        assert_eq!(&startup[8..], b"user\0bob\0database\0db\0\0");
        assert_eq!(
            u32::from_be_bytes([startup[0], startup[1], startup[2], startup[3]]) as usize,
            startup.len()
        );
    }

    #[quench_test::test]
    fn simple_frontend_messages() {
        assert_eq!(query("SELECT 1"), b"Q\0\0\0\x0dSELECT 1\0");
        assert_eq!(sync(), [b'S', 0, 0, 0, 4]);
        assert_eq!(terminate(), [b'X', 0, 0, 0, 4]);
        assert_eq!(password("pw"), b"p\0\0\0\x07pw\0");
        assert_eq!(describe(b'P', ""), b"D\0\0\0\x06P\0");
        assert_eq!(close(b'S', "s1"), b"C\0\0\0\x08Ss1\0");
        assert_eq!(execute("", 0), b"E\0\0\0\x09\0\0\0\0\0");
    }

    #[quench_test::test]
    fn parse_and_bind_encode_parameters() {
        let parse_msg = parse("", "SELECT $1", &[0]);
        assert_eq!(&parse_msg[..1], b"P");
        assert!(parse_msg.windows(9).any(|w| w == b"SELECT $1"));
        let bind_msg = bind("", "", &[Some("hi".into()), None]);
        assert_eq!(bind_msg[0], b'B');
        // portal "\0", statement "\0", 1 format code (0), 2 params: len 2 "hi", len -1; 1 result format (0)
        assert_eq!(
            &bind_msg[5..],
            b"\0\0\0\x01\0\0\0\x02\0\0\0\x02hi\xff\xff\xff\xff\0\x01\0\0"
        );
        assert_eq!(
            u32::from_be_bytes([bind_msg[1], bind_msg[2], bind_msg[3], bind_msg[4]]) as usize,
            bind_msg.len() - 1
        );
    }

    #[quench_test::test]
    fn sasl_messages() {
        let initial = sasl_initial_response("SCRAM-SHA-256", b"n,,n=,r=abc");
        assert_eq!(initial[0], b'p');
        assert_eq!(&initial[5..19], b"SCRAM-SHA-256\0");
        assert_eq!(&initial[19..23], &[0, 0, 0, 11]);
        assert_eq!(&initial[23..], b"n,,n=,r=abc");
        assert_eq!(sasl_response(b"c=biws"), b"p\0\0\0\x0ac=biws");
    }

    #[quench_test::test]
    fn decodes_authentication_requests() {
        assert_eq!(
            decode(b'R', &[0, 0, 0, 0]).unwrap(),
            Backend::AuthenticationOk
        );
        assert_eq!(
            decode(b'R', &[0, 0, 0, 3]).unwrap(),
            Backend::AuthenticationCleartextPassword
        );
        assert_eq!(
            decode(b'R', &[0, 0, 0, 5, 1, 2, 3, 4]).unwrap(),
            Backend::AuthenticationMd5Password([1, 2, 3, 4])
        );
        let sasl = body(|b| {
            b.extend_from_slice(&[0, 0, 0, 10]);
            cstr(b, "SCRAM-SHA-256-PLUS");
            cstr(b, "SCRAM-SHA-256");
            b.push(0);
        });
        assert_eq!(
            decode(b'R', &sasl).unwrap(),
            Backend::AuthenticationSasl(vec!["SCRAM-SHA-256-PLUS".into(), "SCRAM-SHA-256".into()])
        );
        assert_eq!(
            decode(b'R', &[0, 0, 0, 11, b'r', b'=']).unwrap(),
            Backend::AuthenticationSaslContinue(b"r=".to_vec())
        );
        assert_eq!(
            decode(b'R', &[0, 0, 0, 12, b'v', b'=']).unwrap(),
            Backend::AuthenticationSaslFinal(b"v=".to_vec())
        );
        assert_eq!(
            decode(b'R', &[0, 0, 0, 7]).unwrap(),
            Backend::AuthenticationOther(7)
        );
        assert!(decode(b'R', &[0, 0, 0, 5, 1]).is_err());
    }

    #[quench_test::test]
    fn decodes_session_messages() {
        let status = body(|b| {
            cstr(b, "server_version");
            cstr(b, "18.0");
        });
        assert_eq!(
            decode(b'S', &status).unwrap(),
            Backend::ParameterStatus {
                name: "server_version".into(),
                value: "18.0".into()
            }
        );
        assert_eq!(
            decode(b'K', &[0, 0, 0, 7, 0, 0, 1, 0]).unwrap(),
            Backend::BackendKeyData {
                process_id: 7,
                secret_key: 256
            }
        );
        assert_eq!(decode(b'Z', b"I").unwrap(), Backend::ReadyForQuery(b'I'));
        assert_eq!(
            decode(b'Z', b"").map_err(|e| e.to_string()).unwrap_err(),
            "protocol error: message shorter than its contents"
        );
        let notification = body(|b| {
            b.extend_from_slice(&[0, 0, 0, 9]);
            cstr(b, "chan");
            cstr(b, "payload");
        });
        assert_eq!(
            decode(b'A', &notification).unwrap(),
            Backend::NotificationResponse {
                process_id: 9,
                channel: "chan".into(),
                payload: "payload".into()
            }
        );
    }

    #[quench_test::test]
    fn decodes_result_messages() {
        let description = body(|b| {
            b.extend_from_slice(&[0, 1]);
            cstr(b, "id");
            b.extend_from_slice(&[
                0, 0, 0x40, 0, 0, 1, 0, 0, 0, 23, 0, 4, 0xff, 0xff, 0xff, 0xff, 0, 0,
            ]);
        });
        let Backend::RowDescription(fields) = decode(b'T', &description).unwrap() else {
            panic!("row description")
        };
        assert_eq!(fields.len(), 1);
        assert_eq!(
            (
                fields[0].name.as_str(),
                fields[0].table_oid,
                fields[0].column_id,
                fields[0].type_oid,
                fields[0].type_size,
                fields[0].type_modifier,
                fields[0].format
            ),
            ("id", 16384, 1, 23, 4, -1, 0)
        );

        let row = body(|b| {
            b.extend_from_slice(&[
                0, 3, 0, 0, 0, 2, b'4', b'2', 0xff, 0xff, 0xff, 0xff, 0, 0, 0, 0,
            ]);
        });
        assert_eq!(
            decode(b'D', &row).unwrap(),
            Backend::DataRow(vec![Some(b"42".to_vec()), None, Some(Vec::new())])
        );
        assert!(
            decode(b'D', &[0, 1, 0xff, 0xff, 0xff, 0xfe]).is_err(),
            "-2 is not a valid length"
        );
        assert!(
            decode(b'D', &[0, 1, 0, 0, 0, 9, 1]).is_err(),
            "a value running past the message"
        );

        assert_eq!(
            decode(b'C', b"SELECT 1\0").unwrap(),
            Backend::CommandComplete("SELECT 1".into())
        );
        assert!(decode(b'C', b"SELECT 1").is_err(), "an unterminated tag");
        for (tag, expected) in [
            (b'I', Backend::EmptyQueryResponse),
            (b'1', Backend::ParseComplete),
            (b'2', Backend::BindComplete),
            (b'3', Backend::CloseComplete),
            (b'n', Backend::NoData),
            (b's', Backend::PortalSuspended),
            (b'?', Backend::Unknown(b'?')),
        ] {
            assert_eq!(decode(tag, b"").unwrap(), expected);
        }
        assert_eq!(
            decode(b't', &[0, 2, 0, 0, 0, 23, 0, 0, 0, 25]).unwrap(),
            Backend::ParameterDescription(vec![23, 25])
        );
    }

    #[quench_test::test]
    fn decodes_errors_and_notices() {
        let fields = b"SERROR\0C42P01\0Mrelation \"x\" does not exist\0\0";
        let Backend::ErrorResponse(err) = decode(b'E', fields).unwrap() else {
            panic!("error")
        };
        assert_eq!(
            (err.code.as_str(), err.severity.as_str()),
            ("42P01", "ERROR")
        );
        let Backend::NoticeResponse(notice) = decode(b'N', b"SNOTICE\0Mhello\0\0").unwrap() else {
            panic!("notice")
        };
        assert_eq!(notice.message, "hello");
    }

    #[quench_test::test]
    fn command_tags_report_row_counts() {
        assert_eq!(rows_affected("INSERT 0 3"), 3);
        assert_eq!(rows_affected("UPDATE 12"), 12);
        assert_eq!(rows_affected("SELECT 5"), 5);
        assert_eq!(rows_affected("CREATE TABLE"), 0);
        assert_eq!(rows_affected(""), 0);
    }
}
