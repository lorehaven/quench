//! Content-hashed assets: names, hashes, caching headers, validators and compression.

use super::support::*;
use quench_fullstack::{Assets, PREFIX, content_hash};

#[quench_test::test]
fn the_path_carries_a_hash_of_the_bytes() {
    let mut assets = Assets::new();
    let path = assets
        .add("app.wasm", b"first".to_vec(), "application/wasm")
        .to_string();
    assert_eq!(
        path,
        format!("/_quench/app.{}.wasm", content_hash(b"first"))
    );
    assert_eq!(content_hash(b"first").len(), 16);
    assert_eq!(content_hash(b"first"), content_hash(b"first"));
    assert_ne!(content_hash(b"first"), content_hash(b"second"));
    // 16 hex digits of the SHA-256 of "abc", a published vector.
    assert_eq!(content_hash(b"abc"), "ba7816bf8f01cfea");
    assert_eq!(assets.path("app.wasm"), Some(path.as_str()));
    assert_eq!(assets.path("other.wasm"), None);
    assert_eq!(assets.get(&path).unwrap().bytes(), b"first");
    assert_eq!(assets.get(&path).unwrap().path(), path);
}

#[quench_test::test]
fn changing_the_bytes_changes_the_path_and_replaces_the_old_one() {
    let mut assets = Assets::new();
    let old = assets
        .add("app.wasm", b"v1".to_vec(), "application/wasm")
        .to_string();
    let new = assets
        .add("app.wasm", b"v2".to_vec(), "application/wasm")
        .to_string();
    assert_ne!(old, new);
    assert!(assets.get(&old).is_none(), "the old build is gone");
    assert!(assets.get(&new).is_some());
    assert_eq!(assets.path("app.wasm"), Some(new.as_str()));
    // A name that is a prefix of another is not confused with it.
    let other = assets
        .add("app.wasm.map", b"m".to_vec(), "application/json")
        .to_string();
    assert_eq!(assets.path("app.wasm"), Some(new.as_str()));
    assert_eq!(assets.path("app.wasm.map"), Some(other.as_str()));
    let bare = assets
        .add("LICENSE", b"mit".to_vec(), "text/plain")
        .to_string();
    assert!(bare.starts_with("/_quench/LICENSE.") && assets.path("LICENSE").is_some());
}

#[quench_test::test]
async fn an_asset_is_served_to_be_cached_for_a_year() {
    let mut assets = Assets::new();
    let path = assets
        .add("app.wasm", big(), "application/wasm")
        .to_string();
    let r = out(assets.serve(&path, None, None).unwrap()).await;
    assert_eq!(r.status, 200);
    assert_eq!(
        r.header("content-type").as_deref(),
        Some("application/wasm")
    );
    assert_eq!(
        r.header("cache-control").as_deref(),
        Some("public, max-age=31536000, immutable")
    );
    assert_eq!(
        r.header("etag"),
        Some(format!("\"{}\"", content_hash(&big())))
    );
    assert_eq!(r.header("vary").as_deref(), Some("accept-encoding"));
    assert_eq!(
        r.header("x-content-type-options").as_deref(),
        Some("nosniff")
    );
    assert_eq!(r.header("content-encoding"), None);
    assert_eq!(r.body, big());
}

#[quench_test::test]
async fn gzip_is_sent_to_clients_that_accept_it_and_decompresses_to_the_original() {
    let mut assets = Assets::new();
    let path = assets.add("app.js", big(), "text/javascript").to_string();
    for accept in [
        "gzip",
        "gzip, deflate, br",
        "br;q=1, gzip;q=0.5",
        "GZIP",
        "*",
    ] {
        let r = out(assets.serve(&path, Some(accept), None).unwrap()).await;
        assert_eq!(
            r.header("content-encoding").as_deref(),
            Some("gzip"),
            "{accept}"
        );
        assert!(r.body.len() < big().len() / 4, "it really is smaller");
        assert_eq!(
            quench_archive::gzip::decompress(&r.body, 1 << 20).unwrap(),
            big()
        );
    }
    for accept in ["identity", "gzip;q=0", "br", ""] {
        let r = out(assets.serve(&path, Some(accept), None).unwrap()).await;
        assert_eq!(r.header("content-encoding"), None, "{accept}");
        assert_eq!(r.body, big());
    }
}

#[quench_test::test]
async fn small_or_incompressible_assets_are_not_compressed() {
    let mut assets = Assets::new();
    let small = assets
        .add("a.js", b"x=1".to_vec(), "text/javascript")
        .to_string();
    let image = assets.add("logo.png", big(), "image/png").to_string();
    let noise: Vec<u8> = (0..4096_u32)
        .map(|i| (i.wrapping_mul(2654435761) >> 13) as u8)
        .collect();
    let random = assets
        .add("noise.json", noise.clone(), "application/json")
        .to_string();
    for path in [small, image] {
        let r = out(assets.serve(&path, Some("gzip"), None).unwrap()).await;
        assert_eq!(r.header("content-encoding"), None, "{path}");
    }
    let r = out(assets.serve(&random, Some("gzip"), None).unwrap()).await;
    if r.header("content-encoding").is_some() {
        assert!(
            r.body.len() < noise.len(),
            "it is only sent compressed when smaller"
        );
    }
}

#[quench_test::test]
async fn a_matching_validator_gets_a_304_without_a_body() {
    let mut assets = Assets::new();
    let path = assets
        .add("app.wasm", big(), "application/wasm")
        .to_string();
    let etag = format!("\"{}\"", content_hash(&big()));
    for header in [
        etag.clone(),
        format!("W/{etag}"),
        format!("\"x\", {etag}"),
        "*".to_string(),
    ] {
        let r = out(assets.serve(&path, None, Some(&header)).unwrap()).await;
        assert_eq!(r.status, 304, "{header}");
        assert!(r.body.is_empty());
        assert_eq!(
            r.header("cache-control").as_deref(),
            Some("public, max-age=31536000, immutable")
        );
        assert_eq!(r.header("etag"), Some(etag.clone()));
    }
    let r = out(assets.serve(&path, None, Some("\"other\"")).unwrap()).await;
    assert_eq!(r.status, 200);
}

#[quench_test::test]
async fn paths_that_are_not_assets_are_not_served() {
    let mut assets = Assets::new();
    let path = assets
        .add("app.wasm", big(), "application/wasm")
        .to_string();
    assert!(
        assets.serve("/", None, None).is_none(),
        "not under the prefix: not ours"
    );
    assert!(assets.serve("/index.html", None, None).is_none());
    for missing in [
        "/_quench/app.0000000000000000.wasm", // an old build
        "/_quench/app.wasm",
        "/_quench/",
        "/_quench/../secret",
        &format!("{path}/extra"),
        &format!("{path}?x=1"),
    ] {
        let r = out(assets.serve(missing, Some("gzip"), None).unwrap()).await;
        assert_eq!(r.status, 404, "{missing}");
        assert_eq!(r.header("cache-control").as_deref(), Some("no-store"));
    }
    assert_eq!(PREFIX, "/_quench/");
    assert!(format!("{assets:?}").contains("Assets"));
    assert!(Assets::default().get("/_quench/x").is_none());
}
