//! The protocol layer shared by quench's HTTP server and client: message
//! types, an HTTP/1.1 codec and connection drivers, cookies and WebSocket framing.
//!
//! Only HTTP/1.1 is implemented; TLS is terminated at the ingress or by the
//! thin [`tls`] glue over rustls. The drivers are generic over [`io::Io`], so
//! another protocol version can be added next to [`h1`] without touching the
//! callers.

pub mod body;
pub mod cookie;
pub mod date;
pub mod error;
pub mod extensions;
pub mod h1;
pub mod header;
pub mod io;
pub mod message;
pub mod method;
#[cfg(feature = "multipart")]
pub mod multipart;
#[cfg(feature = "sse")]
pub mod sse;
pub mod status;
pub mod uri;
pub mod version;
#[cfg(feature = "ws")]
pub mod ws;

#[cfg(feature = "tls")]
pub use quench_tls as tls;

pub use body::{Body, BoxError};
pub use error::Error;
pub use extensions::Extensions;
pub use header::{HeaderMap, HeaderName, HeaderValue};
pub use message::{Request, Response};
pub use method::Method;
pub use status::StatusCode;
pub use uri::Uri;
pub use version::Version;

#[cfg(test)]
quench_test::main!();
