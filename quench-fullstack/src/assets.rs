//! Content-hashed static assets: the path of an asset changes when its bytes do, so it can be cached
//! forever, and a deploy can never leave a page pointing at the old bytes under the new name.

use quench_http::prelude::Response;
use quench_http::prelude::http::StatusCode;

/// Where hashed assets are served from.
pub const PREFIX: &str = "/_quench/";

/// One year, the longest a cache is asked to keep something.
const IMMUTABLE: &str = "public, max-age=31536000, immutable";

/// Compressing something this small saves less than the header costs.
const MIN_GZIP: usize = 256;

/// An asset: its bytes, its public path, and a gzip copy when that is smaller.
#[derive(Clone, Debug)]
pub struct Asset {
    path: String,
    etag: String,
    content_type: String,
    body: Vec<u8>,
    gzip: Option<Vec<u8>>,
}

impl Asset {
    /// The public path, such as `/_quench/app.3f9a1c0b7d2e4a65.wasm`.
    #[must_use]
    pub fn path(&self) -> &str {
        &self.path
    }

    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.body
    }

    #[must_use]
    pub fn content_type(&self) -> &str {
        &self.content_type
    }
}

/// A set of assets served under [`PREFIX`].
#[derive(Clone, Debug, Default)]
pub struct Assets {
    items: Vec<Asset>,
}

/// The first 16 hex digits of the SHA-256 of `bytes`.
#[must_use]
pub fn content_hash(bytes: &[u8]) -> String {
    quench_core::hex::encode(&quench_crypto::hash::sha256(bytes)[..8])
}

fn compressible(content_type: &str) -> bool {
    let base = content_type.split(';').next().unwrap_or("").trim();
    base.starts_with("text/")
        || matches!(
            base,
            "application/javascript"
                | "application/json"
                | "application/wasm"
                | "image/svg+xml"
                | "application/xml"
        )
}

impl Assets {
    #[must_use]
    pub const fn new() -> Self {
        Self { items: Vec::new() }
    }

    /// Adds an asset called `name` (`app.wasm`) and returns its hashed public path
    /// (`/_quench/app.<hash>.wasm`). Adding the same name again replaces it.
    pub fn add(&mut self, name: &str, body: impl Into<Vec<u8>>, content_type: &str) -> &str {
        let body = body.into();
        let hash = content_hash(&body);
        let path = match name.rsplit_once('.') {
            Some((stem, ext)) => format!("{PREFIX}{stem}.{hash}.{ext}"),
            None => format!("{PREFIX}{name}.{hash}"),
        };
        let gzip = (compressible(content_type) && body.len() >= MIN_GZIP)
            .then(|| quench_archive::gzip::compress(&body))
            .filter(|gz| gz.len() < body.len());
        let asset = Asset {
            etag: format!("\"{hash}\""),
            path,
            content_type: content_type.to_string(),
            body,
            gzip,
        };
        self.items.retain(|a| !is_same_name(&a.path, name));
        self.items.push(asset);
        &self.items.last().expect("just pushed").path
    }

    /// The hashed path of the asset added as `name`.
    #[must_use]
    pub fn path(&self, name: &str) -> Option<&str> {
        self.items
            .iter()
            .find(|a| is_same_name(&a.path, name))
            .map(|a| a.path.as_str())
    }

    /// Every asset, in the order they were added.
    pub fn iter(&self) -> impl Iterator<Item = &Asset> {
        self.items.iter()
    }

    /// Looks an asset up by its public path.
    #[must_use]
    pub fn get(&self, path: &str) -> Option<&Asset> {
        self.items.iter().find(|a| a.path == path)
    }

    /// Answers a request for something under [`PREFIX`], or `None` for any other path. A path under
    /// the prefix that is not an asset (an old hash, say) is a 404, never a fallback to a page.
    #[must_use]
    pub fn serve(
        &self,
        path: &str,
        accept_encoding: Option<&str>,
        if_none_match: Option<&str>,
    ) -> Option<Response> {
        if !path.starts_with(PREFIX) {
            return None;
        }
        let Some(asset) = self.get(path) else {
            return Some(Response::new(StatusCode::NOT_FOUND).header("cache-control", "no-store"));
        };
        if if_none_match.is_some_and(|v| matches_etag(v, &asset.etag)) {
            return Some(
                Response::new(StatusCode::NOT_MODIFIED)
                    .header("cache-control", IMMUTABLE)
                    .header("etag", &asset.etag)
                    .header("vary", "accept-encoding"),
            );
        }
        let (body, encoded) = match (&asset.gzip, accept_encoding.is_some_and(accepts_gzip)) {
            (Some(gz), true) => (gz.clone(), true),
            _ => (asset.body.clone(), false),
        };
        let response = Response::from_bytes(StatusCode::OK, body.into())
            .header("content-type", &asset.content_type)
            .header("cache-control", IMMUTABLE)
            .header("etag", &asset.etag)
            .header("vary", "accept-encoding")
            .header("x-content-type-options", "nosniff");
        Some(if encoded {
            response.header("content-encoding", "gzip")
        } else {
            response
        })
    }
}

/// Whether the hashed `path` was added as `name`.
fn is_same_name(path: &str, name: &str) -> bool {
    let file = path.strip_prefix(PREFIX).unwrap_or(path);
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name, ""));
    let Some(rest) = file.strip_prefix(stem).and_then(|r| r.strip_prefix('.')) else {
        return false;
    };
    let (hash, extension) = rest.split_once('.').unwrap_or((rest, ""));
    hash.len() == 16 && hash.bytes().all(|b| b.is_ascii_hexdigit()) && extension == ext
}

/// Whether an `If-None-Match` value names this etag (or is `*`). Weak validators match too.
fn matches_etag(header: &str, etag: &str) -> bool {
    header
        .split(',')
        .map(str::trim)
        .any(|candidate| candidate == "*" || candidate.trim_start_matches("W/") == etag)
}

/// Whether an `Accept-Encoding` value allows gzip (and does not set its quality to 0).
fn accepts_gzip(header: &str) -> bool {
    header.split(',').any(|part| {
        let mut pieces = part.split(';');
        let coding = pieces.next().unwrap_or("").trim();
        if !coding.eq_ignore_ascii_case("gzip") && coding != "*" {
            return false;
        }
        pieces
            .find_map(|p| p.trim().strip_prefix("q="))
            .is_none_or(|q| q.trim().parse::<f32>().is_ok_and(|q| q > 0.0))
    })
}
