//! A message: who it is from and for, what it says, and its wire form.

use crate::address::{Address, Mailbox};
use crate::error::BuildError;
use crate::mime;
use chrono::{DateTime, Utc};

/// Headers `quench-mail` writes itself; a caller cannot set them as custom ones.
const RESERVED: [&str; 12] = [
    "from",
    "to",
    "cc",
    "bcc",
    "reply-to",
    "sender",
    "subject",
    "date",
    "message-id",
    "mime-version",
    "content-type",
    "content-transfer-encoding",
];

/// The SMTP envelope: what the server is told, independent of the headers.
/// A `Bcc` recipient is here but appears in no header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Envelope {
    pub from: Address,
    pub to: Vec<Address>,
}

#[derive(Debug, Clone)]
pub struct Message {
    from: Mailbox,
    reply_to: Vec<Mailbox>,
    to: Vec<Mailbox>,
    cc: Vec<Mailbox>,
    bcc: Vec<Mailbox>,
    subject: String,
    text: Option<String>,
    html: Option<String>,
    headers: Vec<(String, String)>,
    date: DateTime<Utc>,
    message_id: String,
    envelope_from: Option<Address>,
}

impl Message {
    pub fn builder() -> MessageBuilder {
        MessageBuilder::default()
    }

    /// `<id@domain>`, as written in the `Message-ID` header.
    pub fn message_id(&self) -> &str {
        &self.message_id
    }

    /// Sender is the `From` address unless [`MessageBuilder::envelope_from`]
    /// said otherwise; recipients are To, Cc and Bcc without duplicates.
    pub fn envelope(&self) -> Envelope {
        let mut to: Vec<Address> = Vec::new();
        for mailbox in self.to.iter().chain(&self.cc).chain(&self.bcc) {
            let known = to
                .iter()
                .any(|a| a.as_str().eq_ignore_ascii_case(mailbox.address.as_str()));
            if !known {
                to.push(mailbox.address.clone());
            }
        }
        Envelope {
            from: self
                .envelope_from
                .clone()
                .unwrap_or_else(|| self.from.address.clone()),
            to,
        }
    }

    /// The message as it goes on the wire: CRLF line endings, 7-bit clean.
    pub fn formatted(&self) -> Vec<u8> {
        let mut out = String::new();
        let mut header = |name: &str, value: &str| {
            out.push_str(&mime::fold_header(name, value));
            out.push_str("\r\n");
        };

        header("Date", &self.date.to_rfc2822());
        header("From", &mailbox_header(std::slice::from_ref(&self.from)));
        if !self.reply_to.is_empty() {
            header("Reply-To", &mailbox_header(&self.reply_to));
        }
        if !self.to.is_empty() {
            header("To", &mailbox_header(&self.to));
        }
        if !self.cc.is_empty() {
            header("Cc", &mailbox_header(&self.cc));
        }
        if !self.subject.is_empty() {
            header("Subject", &mime::unstructured(&self.subject));
        }
        header("Message-ID", &self.message_id);
        header("MIME-Version", "1.0");
        for (name, value) in &self.headers {
            header(name, &mime::unstructured(value));
        }

        match (&self.text, &self.html) {
            (Some(text), None) => {
                out.push_str(&part_headers("text/plain", text));
                out.push_str("\r\n");
                out.push_str(&part_body(text));
            }
            (None, Some(html)) => {
                out.push_str(&part_headers("text/html", html));
                out.push_str("\r\n");
                out.push_str(&part_body(html));
            }
            (Some(text), Some(html)) => {
                let boundary = format!("quench-{}", uuid::Uuid::new_v4().simple());
                out.push_str(&format!(
                    "Content-Type: multipart/alternative;\r\n boundary=\"{boundary}\"\r\n\r\n"
                ));
                out.push_str("This is a multi-part message in MIME format.\r\n");
                for (kind, body) in [("text/plain", text), ("text/html", html)] {
                    out.push_str(&format!("--{boundary}\r\n"));
                    out.push_str(&part_headers(kind, body));
                    out.push_str("\r\n");
                    out.push_str(&part_body(body));
                }
                out.push_str(&format!("--{boundary}--\r\n"));
            }
            // `build` refuses a message with no body.
            (None, None) => out.push_str("\r\n"),
        }
        out.into_bytes()
    }
}

fn mailbox_header(mailboxes: &[Mailbox]) -> String {
    mailboxes
        .iter()
        .map(|m| match &m.name {
            Some(name) => format!("{} <{}>", mime::phrase(name), m.address),
            None => m.address.to_string(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn part_headers(content_type: &str, body: &str) -> String {
    let encoding = if mime::is_7bit(&mime::normalize_crlf(body)) {
        "7bit"
    } else {
        "quoted-printable"
    };
    format!(
        "Content-Type: {content_type}; charset=utf-8\r\nContent-Transfer-Encoding: {encoding}\r\n"
    )
}

/// The body, encoded, and ending in exactly one CRLF.
fn part_body(body: &str) -> String {
    let normalized = mime::normalize_crlf(body);
    let mut encoded = if mime::is_7bit(&normalized) {
        normalized
    } else {
        mime::quoted_printable(&normalized)
    };
    if !encoded.ends_with("\r\n") {
        encoded.push_str("\r\n");
    }
    encoded
}

#[derive(Default)]
pub struct MessageBuilder {
    from: Option<Mailbox>,
    reply_to: Vec<Mailbox>,
    to: Vec<Mailbox>,
    cc: Vec<Mailbox>,
    bcc: Vec<Mailbox>,
    subject: String,
    text: Option<String>,
    html: Option<String>,
    headers: Vec<(String, String)>,
    date: Option<DateTime<Utc>>,
    message_id: Option<String>,
    envelope_from: Option<Address>,
    error: Option<BuildError>,
}

impl MessageBuilder {
    fn fail(&mut self, error: BuildError) {
        self.error.get_or_insert(error);
    }

    fn mailbox(&mut self, raw: &str) -> Option<Mailbox> {
        match raw.parse() {
            Ok(mailbox) => Some(mailbox),
            Err(err) => {
                self.fail(err);
                None
            }
        }
    }

    /// `"Name <user@host>"` or a bare address. A parse error is reported by
    /// [`build`](Self::build), so calls can be chained.
    pub fn from(mut self, mailbox: &str) -> Self {
        self.from = self.mailbox(mailbox);
        self
    }

    /// [`from`](Self::from) for a mailbox that is already built, e.g. from
    /// configuration - a display name is taken as-is, never re-parsed.
    pub fn from_mailbox(mut self, mailbox: Mailbox) -> Self {
        self.from = Some(mailbox);
        self
    }

    /// [`to`](Self::to) for an already-built mailbox.
    pub fn to_mailbox(mut self, mailbox: Mailbox) -> Self {
        self.to.push(mailbox);
        self
    }

    pub fn to(mut self, mailbox: &str) -> Self {
        if let Some(m) = self.mailbox(mailbox) {
            self.to.push(m);
        }
        self
    }

    pub fn cc(mut self, mailbox: &str) -> Self {
        if let Some(m) = self.mailbox(mailbox) {
            self.cc.push(m);
        }
        self
    }

    /// Delivered to, but named in no header.
    pub fn bcc(mut self, mailbox: &str) -> Self {
        if let Some(m) = self.mailbox(mailbox) {
            self.bcc.push(m);
        }
        self
    }

    pub fn reply_to(mut self, mailbox: &str) -> Self {
        if let Some(m) = self.mailbox(mailbox) {
            self.reply_to.push(m);
        }
        self
    }

    pub fn subject(mut self, subject: &str) -> Self {
        if subject.chars().any(|c| c == '\r' || c == '\n' || c == '\0') {
            self.fail(BuildError::HeaderInjection("subject"));
        } else {
            self.subject = subject.to_string();
        }
        self
    }

    pub fn text(mut self, body: &str) -> Self {
        self.text = Some(body.to_string());
        self
    }

    /// With [`text`](Self::text) as well, the message is `multipart/alternative`.
    pub fn html(mut self, body: &str) -> Self {
        self.html = Some(body.to_string());
        self
    }

    /// An extra header, e.g. `Auto-Submitted: auto-generated`. Headers the
    /// library writes itself (From, Subject, Content-Type, ...) are refused.
    pub fn header(mut self, name: &str, value: &str) -> Self {
        let valid_name = !name.is_empty() && name.chars().all(|c| c.is_ascii_graphic() && c != ':');
        if !valid_name {
            self.fail(BuildError::InvalidHeaderName(name.to_string()));
        } else if RESERVED.contains(&name.to_ascii_lowercase().as_str()) {
            self.fail(BuildError::ReservedHeader(name.to_string()));
        } else if value.chars().any(|c| c == '\r' || c == '\n' || c == '\0') {
            self.fail(BuildError::HeaderInjection("header value"));
        } else {
            self.headers.push((name.to_string(), value.to_string()));
        }
        self
    }

    /// Overrides the `Date` header (defaults to now) - mainly for tests.
    pub fn date(mut self, date: DateTime<Utc>) -> Self {
        self.date = Some(date);
        self
    }

    /// The SMTP `MAIL FROM` when it should differ from the `From` header,
    /// e.g. a bounce address.
    pub fn envelope_from(mut self, address: &str) -> Self {
        match address.parse() {
            Ok(address) => self.envelope_from = Some(address),
            Err(err) => self.fail(err),
        }
        self
    }

    pub fn build(self) -> Result<Message, BuildError> {
        if let Some(error) = self.error {
            return Err(error);
        }
        let from = self.from.ok_or(BuildError::MissingFrom)?;
        if self.to.is_empty() && self.cc.is_empty() && self.bcc.is_empty() {
            return Err(BuildError::NoRecipients);
        }
        if self.text.is_none() && self.html.is_none() {
            return Err(BuildError::MissingBody);
        }
        let message_id = self.message_id.unwrap_or_else(|| {
            format!(
                "<{}@{}>",
                uuid::Uuid::new_v4().simple(),
                from.address.domain()
            )
        });
        Ok(Message {
            from,
            reply_to: self.reply_to,
            to: self.to,
            cc: self.cc,
            bcc: self.bcc,
            subject: self.subject,
            text: self.text,
            html: self.html,
            headers: self.headers,
            date: self.date.unwrap_or_else(Utc::now),
            message_id,
            envelope_from: self.envelope_from,
        })
    }
}
