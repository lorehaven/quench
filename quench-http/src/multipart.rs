//! `multipart/form-data` support. Field-by-field
//! rather than a `#[derive(MultipartForm)]` struct: a handler calls
//! `next_field()` in a loop and matches on `field.name()`.
//!
//! ```ignore
//! async fn upload(mut form: Multipart) -> Result<Response, HttpError> {
//!     let mut file_bytes = None;
//!     while let Some(field) = form.next_field().await? {
//!         match field.name() {
//!             Some("file") => file_bytes = Some(field.bytes().await?),
//!             _ => {}
//!         }
//!     }
//!     // ...
//! }
//! ```

use crate::body::InboundBody;
use crate::error::{ExtractError, HttpError};
use crate::extract::{FromRequest, resolve_body_limit};
use crate::request::Request;
use quench_core::async_trait;
use quench_core::bytes::Bytes;
use quench_proto::multipart::{Limits, parse_boundary};

type Inner = quench_proto::multipart::Multipart<InboundBody>;

fn invalid(err: impl std::fmt::Display) -> HttpError {
    ExtractError::InvalidMultipart(err.to_string()).into()
}

/// A `multipart/form-data` request body, read one field at a time.
///
/// Extracting this consumes the request body (same rule as
/// [`crate::extract::Json`]/[`crate::extract::Bytes`] - only one
/// body-consuming extractor per handler).
///
/// The whole-stream size cap is the request's [`BodyLimit`] (falling back
/// to [`DEFAULT_BODY_LIMIT`]), same knob `Json`/`Bytes` use - a service
/// that accepts large uploads raises `BodyLimit` once and every
/// body-consuming extractor, multipart included, respects it.
#[derive(Debug)]
pub struct Multipart {
    inner: Inner,
}

impl Multipart {
    /// Reads the next field, or `None` once the body is exhausted. A field
    /// that was not read to its end is skipped.
    pub async fn next_field(&mut self) -> Result<Option<Field<'_>>, HttpError> {
        self.inner
            .next_field()
            .await
            .map(|field| field.map(|inner| Field { inner }))
            .map_err(invalid)
    }
}

/// One field of a [`Multipart`] body.
#[derive(Debug)]
pub struct Field<'a> {
    inner: quench_proto::multipart::Field<'a, InboundBody>,
}

impl Field<'_> {
    /// The field's name (the `name` in its `Content-Disposition` header) -
    /// what a form's `<input name="...">` set it to.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.inner.name()
    }

    /// The original filename, present when this field came from a file
    /// input rather than a plain text one.
    #[must_use]
    pub fn file_name(&self) -> Option<&str> {
        self.inner.file_name()
    }

    /// The field's own `Content-Type`, if the client sent one.
    #[must_use]
    pub fn content_type(&self) -> Option<&str> {
        self.inner.content_type()
    }

    /// The next piece of this field's value, `None` at its end - for
    /// piping a large upload somewhere without buffering it.
    pub async fn chunk(&mut self) -> Result<Option<Bytes>, HttpError> {
        self.inner.chunk().await.map_err(invalid)
    }

    /// Buffers this field's whole value as bytes.
    pub async fn bytes(self) -> Result<Bytes, HttpError> {
        self.inner.bytes().await.map_err(invalid)
    }

    /// Buffers this field's whole value as UTF-8 text - for a plain
    /// (non-file) form field.
    pub async fn text(self) -> Result<String, HttpError> {
        self.inner.text().await.map_err(invalid)
    }
}

#[async_trait]
impl FromRequest for Multipart {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let content_type = req.header("content-type").unwrap_or("").to_string();
        let boundary = parse_boundary(&content_type).map_err(|_| {
            ExtractError::InvalidMultipart("missing or invalid multipart boundary".to_string())
        })?;
        let limits = Limits {
            whole_stream: Some(resolve_body_limit(req) as u64),
            ..Limits::default()
        };
        Ok(Self {
            inner: quench_proto::multipart::Multipart::new(req.take_body(), &boundary, limits),
        })
    }
}
