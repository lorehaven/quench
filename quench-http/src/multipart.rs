//! `multipart/form-data` support - the quench-http counterpart to
//! `actix-multipart`. Field-by-field, like axum's `Multipart` extractor,
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

use crate::error::{ExtractError, HttpError};
use crate::extract::{FromRequest, resolve_body_limit};
use crate::request::Request;
use async_trait::async_trait;
use bytes::Bytes;
use http_body_util::BodyDataStream;

/// A `multipart/form-data` request body, read one field at a time.
/// Extracting this consumes the request body (same rule as
/// [`crate::extract::Json`]/[`crate::extract::Bytes`] - only one
/// body-consuming extractor per handler).
///
/// The whole-stream size cap is the request's [`BodyLimit`] (falling back
/// to [`DEFAULT_BODY_LIMIT`]), same knob `Json`/`Bytes` use - a service
/// that accepts large uploads raises `BodyLimit` once and every
/// body-consuming extractor, multipart included, respects it.
pub struct Multipart {
    inner: multer::Multipart<'static>,
}

impl Multipart {
    /// Reads the next field, or `None` once the body is exhausted.
    pub async fn next_field(&mut self) -> Result<Option<Field>, HttpError> {
        self.inner
            .next_field()
            .await
            .map(|field| field.map(|inner| Field { inner }))
            .map_err(|e| ExtractError::InvalidMultipart(e.to_string()).into())
    }
}

/// One field of a [`Multipart`] body.
pub struct Field {
    inner: multer::Field<'static>,
}

impl Field {
    /// The field's name (the `name` in its `Content-Disposition` header) -
    /// what a form's `<input name="...">` set it to.
    pub fn name(&self) -> Option<&str> {
        self.inner.name()
    }

    /// The original filename, present when this field came from a file
    /// input rather than a plain text one.
    pub fn file_name(&self) -> Option<&str> {
        self.inner.file_name()
    }

    /// The field's own `Content-Type`, if the client sent one.
    pub fn content_type(&self) -> Option<&str> {
        self.inner.content_type().map(|m| m.as_ref())
    }

    /// Buffers this field's whole value as bytes.
    pub async fn bytes(self) -> Result<Bytes, HttpError> {
        self.inner
            .bytes()
            .await
            .map_err(|e| ExtractError::InvalidMultipart(e.to_string()).into())
    }

    /// Buffers this field's whole value as UTF-8 text - for a plain
    /// (non-file) form field.
    pub async fn text(self) -> Result<String, HttpError> {
        self.inner
            .text()
            .await
            .map_err(|e| ExtractError::InvalidMultipart(e.to_string()).into())
    }
}

#[async_trait]
impl FromRequest for Multipart {
    async fn from_request(req: &mut Request) -> Result<Self, HttpError> {
        let content_type = req.header("content-type").unwrap_or("").to_string();
        let boundary = multer::parse_boundary(&content_type).map_err(|_| {
            ExtractError::InvalidMultipart("missing or invalid multipart boundary".to_string())
        })?;

        let limit = resolve_body_limit(req);
        let constraints = multer::Constraints::new()
            .size_limit(multer::SizeLimit::new().whole_stream(limit as u64));

        let stream = BodyDataStream::new(req.take_body());
        let inner = multer::Multipart::with_constraints(stream, boundary, constraints);
        Ok(Multipart { inner })
    }
}
