//! Reading a named cookie back out of a quench-http [`Request`]'s `Cookie`
//! header. actix-web has `HttpRequest::cookie(name)` built in (it parses
//! and caches the header the first time anything asks); quench-http has no
//! cookie support at all, on purpose - it's a `quench-auth`-specific need,
//! not a core HTTP concern, so the `cookie` crate dependency and this
//! parsing live here instead of in `quench-http` itself.

use quench_http::prelude::Request;

/// The value of the first cookie named `name` in the request's `Cookie`
/// header, if any. Malformed pairs in the header are skipped rather than
/// failing the whole lookup - the same tolerance actix-web's cookie jar has.
pub fn cookie_value(req: &Request, name: &str) -> Option<String> {
    let header = req.header("cookie")?;
    // `cookie` 0.16 (pinned to match actix-web's own re-export - see the
    // workspace `Cargo.toml`) has no `split_parse` for a whole `Cookie:`
    // header; split on the `; ` separator RFC 6265 specifies and parse each
    // pair individually instead.
    header
        .split(';')
        .filter_map(|pair| cookie::Cookie::parse(pair.trim()).ok())
        .find(|c| c.name() == name)
        .map(|c| c.value().to_string())
}
