//! What an asset request is answered with.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
}

impl Answer {
    /// A `200` with this body.
    #[must_use]
    pub fn ok(content_type: &str, body: Vec<u8>) -> Self {
        Self {
            status: 200,
            content_type: content_type.to_string(),
            body,
        }
    }

    /// A `404` with a short text.
    #[must_use]
    pub fn not_found() -> Self {
        Self {
            status: 404,
            content_type: "text/plain; charset=utf-8".to_string(),
            body: b"not found".to_vec(),
        }
    }
}
