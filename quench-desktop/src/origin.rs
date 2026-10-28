//! Which pages are the app's own, and which links leave it.

use crate::bundle::{HOST, SCHEME};

/// `scheme://host[:port]` of a URI; empty when it has none.
#[must_use]
pub fn origin_of(uri: &str) -> String {
    let Some((scheme, rest)) = uri.split_once("://") else {
        return String::new();
    };
    let authority = rest.split(['/', '?', '#']).next().unwrap_or("");
    format!("{scheme}://{authority}")
}

/// Whether a page at `uri` is the app's own: the bundle's origin, or the dev server's.
#[must_use]
pub fn is_app_origin(uri: &str, dev_url: Option<&str>) -> bool {
    let origin = origin_of(uri);
    origin == format!("{SCHEME}://{HOST}") || dev_url.is_some_and(|dev| origin == origin_of(dev))
}

/// Whether a link to `uri` is one the system's browser or mail client should open.
#[must_use]
pub fn opens_outside(uri: &str) -> bool {
    ["https://", "http://", "mailto:"]
        .iter()
        .any(|prefix| uri.starts_with(prefix))
}
