//! `multipart/form-data` request bodies (RFC 7578), for file uploads and mixed forms.

use quench_core::bytes::Bytes;
use quench_core::{hex, random};

/// One field of a [`Form`]: bytes with an optional file name and content type.
#[derive(Debug, Clone)]
pub struct Part {
    bytes: Bytes,
    file_name: Option<String>,
    mime: Option<String>,
}

impl Part {
    /// A field holding `bytes` (sent as `application/octet-stream` unless a type is set).
    #[must_use]
    pub fn bytes(bytes: impl Into<Bytes>) -> Self {
        Self {
            bytes: bytes.into(),
            file_name: None,
            mime: None,
        }
    }

    /// A text field.
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            bytes: Bytes::from(text.into().into_bytes()),
            file_name: None,
            mime: None,
        }
    }

    /// Makes the field a file with this name.
    #[must_use]
    pub fn file_name(mut self, name: impl Into<String>) -> Self {
        self.file_name = Some(name.into());
        self
    }

    /// The field's `Content-Type`, e.g. `image/png`.
    #[must_use]
    pub fn mime_str(mut self, mime: impl Into<String>) -> Self {
        self.mime = Some(mime.into());
        self
    }
}

/// A multipart body under construction.
#[derive(Debug, Clone)]
pub struct Form {
    boundary: String,
    parts: Vec<(String, Part)>,
}

impl Default for Form {
    fn default() -> Self {
        Self::new()
    }
}

impl Form {
    /// An empty form with a random boundary.
    #[must_use]
    pub fn new() -> Self {
        let tag: [u8; 12] = random::bytes();
        Self {
            boundary: format!("quench-boundary-{}", hex::encode(tag)),
            parts: Vec::new(),
        }
    }

    /// Adds a text field.
    #[must_use]
    pub fn text(self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.part(name, Part::text(value))
    }

    /// Adds a field of any kind.
    #[must_use]
    pub fn part(mut self, name: impl Into<String>, part: Part) -> Self {
        self.parts.push((name.into(), part));
        self
    }

    /// The boundary that separates the parts.
    #[must_use]
    pub fn boundary(&self) -> &str {
        &self.boundary
    }

    /// The `Content-Type` header value that goes with [`into_body`](Self::into_body).
    #[must_use]
    pub fn content_type(&self) -> String {
        format!("multipart/form-data; boundary={}", self.boundary)
    }

    /// The encoded body.
    #[must_use]
    pub fn into_body(self) -> Bytes {
        let mut out = Vec::new();
        for (name, part) in &self.parts {
            out.extend_from_slice(format!("--{}\r\n", self.boundary).as_bytes());
            let file_name = part.file_name.as_ref().map_or_else(String::new, |file| {
                format!("; filename=\"{}\"", escape(file))
            });
            let disposition = format!(
                "Content-Disposition: form-data; name=\"{}\"{file_name}",
                escape(name)
            );
            out.extend_from_slice(disposition.as_bytes());
            out.extend_from_slice(b"\r\n");
            let mime = part
                .mime
                .as_deref()
                .or_else(|| part.file_name.as_ref().map(|_| "application/octet-stream"));
            if let Some(mime) = mime {
                out.extend_from_slice(format!("Content-Type: {mime}\r\n").as_bytes());
            }
            out.extend_from_slice(b"\r\n");
            out.extend_from_slice(&part.bytes);
            out.extend_from_slice(b"\r\n");
        }
        out.extend_from_slice(format!("--{}--\r\n", self.boundary).as_bytes());
        Bytes::from(out)
    }
}

/// Quotes, CR and LF inside a header parameter are percent-escaped, as browsers do.
fn escape(value: &str) -> String {
    value
        .replace('"', "%22")
        .replace('\r', "%0D")
        .replace('\n', "%0A")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_text_and_file_parts() {
        let form = Form::new()
            .text("title", "hello")
            .part(
                "upload",
                Part::bytes(&b"\x00\x01data"[..])
                    .file_name("a \"b\".bin")
                    .mime_str("image/png"),
            )
            .part("plain", Part::bytes(&b"x"[..]).file_name("p.txt"));
        let boundary = form.boundary().to_string();
        assert!(form.content_type().ends_with(&boundary));
        let body = form.into_body();
        let text = String::from_utf8_lossy(&body).into_owned();

        assert!(text.starts_with(&format!("--{boundary}\r\n")));
        assert!(text.ends_with(&format!("--{boundary}--\r\n")));
        assert!(text.contains("Content-Disposition: form-data; name=\"title\"\r\n\r\nhello\r\n"));
        assert!(text.contains(
            "name=\"upload\"; filename=\"a %22b%22.bin\"\r\nContent-Type: image/png\r\n\r\n"
        ));
        assert!(text.contains("filename=\"p.txt\"\r\nContent-Type: application/octet-stream\r\n"));
        assert!(body.windows(6).any(|w| w == b"\x00\x01data"));
    }

    #[test]
    fn boundaries_differ_and_names_are_escaped() {
        assert_ne!(Form::new().boundary(), Form::default().boundary());
        let body = Form::new().text("a\r\nb", "v").into_body();
        let text = String::from_utf8_lossy(&body);
        assert!(text.contains("name=\"a%0D%0Ab\""));
        assert!(text.contains("\r\n\r\nv\r\n"));
        let empty = Form::new();
        let boundary = empty.boundary().to_string();
        assert_eq!(
            empty.into_body().as_ref(),
            format!("--{boundary}--\r\n").as_bytes()
        );
    }
}
