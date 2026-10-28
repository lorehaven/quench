//! The server-rendered document, its embedded state, nonce and policy.

use super::support::*;
use quench_fullstack::{
    Page, STATE_ID, Ui, UiApp, content_security_policy, escape_json, new_nonce, render_document,
};
use quench_http::prelude::Endpoint;
use quench_http::prelude::http::StatusCode;
use quench_ui::prelude::*;
use std::path::{Path, PathBuf};

const NONCE: &str = "00112233445566778899aabbccddeeff";

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

/// Compares with the stored golden file, or rewrites it when `QUENCH_UPDATE_GOLDEN` is set.
fn assert_golden(name: &str, actual: &str) {
    let path = fixture(name);
    if std::env::var_os("QUENCH_UPDATE_GOLDEN").is_some() {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("missing {}: {e}", path.display()));
    assert_eq!(
        expected, actual,
        "{name} differs (regenerate with QUENCH_UPDATE_GOLDEN=1 if intended)"
    );
}

fn ui() -> Ui {
    Ui::new(big())
}

/// An app with fixed bytes, so its hashed paths (and the golden page) do not move when `quench.js`
/// is edited.
fn fixed_ui() -> Ui {
    Ui::with_script("export async function start() {}\n".repeat(20), big())
}

fn app_view() -> View {
    let count = create_signal(3);
    let shown = count.clone();
    div()
        .class("card")
        .child(h1().text("Hello & <welcome>"))
        .child(
            p().attr("id", "n")
                .text_fn(move || format!("count: {}", shown.get())),
        )
        .child(ul().child(View::keyed(
            || vec!["a", "b"],
            |s| s.to_string(),
            |s| li().text(s).into_view(),
        )))
        .child(button().text("+").on_click(|| ()))
        .into_view()
}

#[quench_test::test]
async fn the_page_has_the_expected_markup() {
    let page = Page::new("A <title> & more", app_view())
        .lang("pl")
        .stylesheet("/_quench/site.css")
        .state(r#"{"count":3}"#);
    let (script, wasm) = {
        let ui = fixed_ui();
        let (s, w) = ui.paths();
        (s.to_string(), w.to_string())
    };
    let html = render_document(page, NONCE, &script, &wasm);
    assert_golden("page.html", &html);
}

#[quench_test::test]
async fn the_document_is_served_with_a_policy_that_names_its_nonce() {
    let ui = ui();
    let r = out(ui.render_with_nonce(Page::new("t", app_view()), NONCE)).await;
    assert_eq!(r.status, 200);
    assert_eq!(
        r.header("content-type").as_deref(),
        Some("text/html; charset=utf-8")
    );
    assert_eq!(r.header("cache-control").as_deref(), Some("no-store"));
    assert_eq!(
        r.header("x-content-type-options").as_deref(),
        Some("nosniff")
    );
    let csp = r.header("content-security-policy").unwrap();
    assert_eq!(csp, content_security_policy(NONCE));
    assert!(csp.contains(&format!("'nonce-{NONCE}'")));
    assert!(csp.contains("default-src 'self'") && csp.contains("'wasm-unsafe-eval'"));
    assert!(
        !csp.contains("unsafe-inline") && !csp.contains("unsafe-eval'")
            || csp.contains("'wasm-unsafe-eval'")
    );
    assert!(!csp.contains("'unsafe-inline'"));
    assert!(r.text().contains(&format!("nonce=\"{NONCE}\"")));
}

#[quench_test::test]
async fn no_inline_handlers_and_every_script_carries_the_nonce() {
    let ui = ui();
    let html = out(ui.render_with_nonce(Page::new("t", app_view()).state("{}"), NONCE))
        .await
        .text();
    assert!(
        !html.contains(" onclick") && !html.contains(" on"),
        "no inline event handlers: {html}"
    );
    let scripts: Vec<&str> = html
        .match_indices("<script")
        .map(|(i, _)| &html[i..i + html[i..].find('>').unwrap()])
        .collect();
    assert_eq!(scripts.len(), 2, "the state block and the module");
    assert!(
        scripts
            .iter()
            .all(|s| s.contains(&format!("nonce=\"{NONCE}\""))),
        "{scripts:?}"
    );
    assert!(html.contains("<script type=\"module\""));
    assert!(
        !html.contains("<style") && !html.contains(" style="),
        "the policy allows no inline styles"
    );
}

#[quench_test::test]
async fn every_render_gets_a_fresh_unguessable_nonce() {
    let ui = ui();
    let a = out(ui.render(Page::new("t", "x"))).await;
    let b = out(ui.render(Page::new("t", "x"))).await;
    let nonce = |r: &Out| {
        let csp = r.header("content-security-policy").unwrap();
        let start = csp.find("'nonce-").unwrap() + 7;
        csp[start..csp[start..].find('\'').unwrap() + start].to_string()
    };
    assert_ne!(nonce(&a), nonce(&b));
    assert_eq!(nonce(&a).len(), 32, "128 bits as hex");
    assert!(nonce(&a).bytes().all(|b| b.is_ascii_hexdigit()));
    assert_ne!(new_nonce(), new_nonce());
}

#[quench_test::test]
async fn embedded_state_cannot_end_its_script_element() {
    for hostile in [
        r#"{"x":"</script><script>alert(1)</script>"}"#,
        r#"{"x":"<!-- <script>"}"#,
        "{\"x\":\"a\u{2028}b\u{2029}c\"}",
        r#"{"x":"&amp; <b>"}"#,
    ] {
        let escaped = escape_json(hostile);
        assert!(
            !escaped.contains('<') && !escaped.contains('>') && !escaped.contains('&'),
            "{escaped}"
        );
        assert!(!escaped.contains('\u{2028}') && !escaped.contains('\u{2029}'));
        let ui = ui();
        let html = out(ui.render_with_nonce(Page::new("t", "x").state(hostile), NONCE))
            .await
            .text();
        let open = html.find(STATE_ID).unwrap();
        let body_start = html[open..].find('>').unwrap() + open + 1;
        let body_end = html[body_start..].find("</script>").unwrap() + body_start;
        assert_eq!(
            &html[body_start..body_end],
            escaped,
            "the first </script> is the real one"
        );
        assert_eq!(
            html.matches("</script>").count(),
            2,
            "only the two the page itself writes"
        );
    }
    // Escapes mean the same thing to a JSON reader: < is "<".
    let escaped_lt = format!("{{\"a\":\"{}u003c\"}}", '\\');
    assert_eq!(escape_json(r#"{"a":"<"}"#), escaped_lt);
    assert_eq!(escape_json("plain 1 2 3"), "plain 1 2 3");
}

#[quench_test::test]
async fn title_language_and_stylesheets_are_escaped() {
    let ui = ui();
    let page = Page::new("</title><script>alert(1)</script>", "x")
        .lang("en\" onload=\"alert(1)")
        .stylesheet("/a.css\"><script>alert(2)</script>");
    let html = out(ui.render_with_nonce(page, NONCE)).await.text();
    assert!(html.contains("<title>&lt;/title&gt;&lt;script&gt;alert(1)&lt;/script&gt;</title>"));
    assert!(html.contains("lang=\"en&quot; onload=&quot;alert(1)\""));
    assert!(html.contains("href=\"/a.css&quot;&gt;&lt;script&gt;alert(2)&lt;/script&gt;\""));
    assert!(!html.contains("<script>alert"));
}

#[quench_test::test]
async fn the_status_can_be_chosen_and_there_is_no_state_block_without_state() {
    let ui = ui();
    let r = out(ui.render_with_nonce(Page::new("gone", "x").status(StatusCode::GONE), NONCE)).await;
    assert_eq!(r.status, 410);
    assert!(!r.text().contains(STATE_ID));
    assert!(format!("{:?}", Page::new("t", "x")).contains("Page"));
}

#[quench_test::test]
async fn the_app_endpoint_serves_pages_and_assets_and_404s_the_rest() {
    let mut ui = ui();
    let sheet = ui.add_asset("site.css", b"body{margin:0}".to_vec(), "text/css");
    let (script, wasm) = {
        let (s, w) = ui.paths();
        (s.to_string(), w.to_string())
    };
    let app = UiApp::new(ui).page("/", move |_req| {
        Page::new("home", app_view())
            .stylesheet(&sheet)
            .state("{\"a\":1}")
    });
    assert!(format!("{app:?}").contains("UiApp"));
    assert!(app.ui().assets().get(&wasm).is_some());

    let page = out(app.call(request("/", &[]).await).await).await;
    assert_eq!(page.status, 200);
    let html = page.text();
    assert!(
        html.contains("<h1>Hello &amp; &lt;welcome&gt;</h1>"),
        "the content is in the page itself"
    );
    assert!(html.contains(&format!("src=\"{script}?wasm={wasm}\"")));

    let asset = out(app
        .call(request(&wasm, &[("accept-encoding", "gzip")]).await)
        .await)
    .await;
    assert_eq!(asset.status, 200);
    assert_eq!(asset.header("content-encoding").as_deref(), Some("gzip"));
    let js = out(app.call(request(&script, &[]).await).await).await;
    assert!(js.text().contains("export async function start"));
    let not_modified = out(app
        .call(request(&wasm, &[("if-none-match", "*")]).await)
        .await)
    .await;
    assert_eq!(not_modified.status, 304);

    for missing in ["/nope", "/_quench/old.0000000000000000.wasm", "/index.html"] {
        assert_eq!(
            out(app.call(request(missing, &[]).await).await)
                .await
                .status,
            404,
            "{missing}"
        );
    }
}

#[quench_test::test]
async fn a_stylesheet_can_be_limited_to_a_media_query() {
    let page = Page::new("t", "x")
        .stylesheet("/a.css")
        .stylesheet_for("/dark.css", "(prefers-color-scheme: dark)\" onload=\"x");
    let html = render_document(page, NONCE, "/s.js", "/a.wasm");
    assert!(html.contains("<link rel=\"stylesheet\" href=\"/a.css\">"));
    assert!(
        html.contains(
            "href=\"/dark.css\" media=\"(prefers-color-scheme: dark)&quot; onload=&quot;x\">"
        ),
        "{html}"
    );
}

#[quench_test::test]
fn export_writes_the_page_and_every_asset_at_its_public_path() {
    let mut ui = Ui::new(big());
    let css = ui.add_asset("site.css", "body{margin:0}", "text/css");
    let dir = std::env::temp_dir().join(format!("quench-export-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let page = Page::new("Bundle", p().text("hi"))
        .stylesheet(&css)
        .state("{\"a\":1}");
    ui.export(&dir, page).unwrap();
    let html = std::fs::read_to_string(dir.join("index.html")).unwrap();
    assert!(html.contains("<p>hi</p>") && html.contains(&css), "{html}");
    assert!(html.contains("nonce=\"bundle\""), "{html}");
    let (script, wasm) = ui.paths();
    for path in [script, wasm, css.as_str()] {
        assert!(dir.join(path.trim_start_matches('/')).is_file(), "{path}");
        assert!(html.contains(path) || path == wasm || path == css, "{path}");
    }
    assert_eq!(
        std::fs::read(dir.join(wasm.trim_start_matches('/'))).unwrap(),
        big()
    );
    let _ = std::fs::remove_dir_all(&dir);
}
