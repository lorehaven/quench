//! Reading a named cookie back out of a quench-http [`Request`]'s `Cookie`
//! header.
//!
//! quench-http has no cookie support at all, on purpose - it's a `quench-auth`-specific need,
//! not a core HTTP concern, so the `cookie` crate dependency and this
//! parsing live here instead of in `quench-http` itself.

use quench_http::prelude::Request;

/// The value of the first cookie named `name` in the request's `Cookie`
/// header, if any. Malformed pairs in the header are skipped rather than
/// failing the whole lookup - a cookie jar's usual tolerance.
pub fn cookie_value(req: &Request, name: &str) -> Option<String> {
    let header = req.header("cookie")?;
    // Split on the `;` separator RFC 6265 specifies and parse each pair
    // individually, so one malformed pair doesn't poison the rest.
    header
        .split(';')
        .filter_map(|pair| cookie::Cookie::parse(pair.trim()).ok())
        .find(|c| c.name() == name)
        .map(|c| c.value().to_string())
}
