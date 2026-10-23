//! Reading a named cookie back out of a quench-http [`Request`]'s `Cookie`
//! header.
//!
//! Cookie parsing itself lives in `quench_http::http::cookie`; this is the
//! thin lookup on a [`Request`].

use quench_http::prelude::Request;

/// The value of the first cookie named `name` in the request's `Cookie`
/// header, if any. Malformed pairs in the header are skipped rather than
/// failing the whole lookup - a cookie jar's usual tolerance.
pub fn cookie_value(req: &Request, name: &str) -> Option<String> {
    quench_http::http::cookie::cookie_value(req.header("cookie")?, name).map(str::to_string)
}
