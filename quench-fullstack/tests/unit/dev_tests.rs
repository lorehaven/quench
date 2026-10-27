//! Development live reload: off unless asked for, and then a script on every page plus the stream.

use super::support::*;
use quench_fullstack::{Page, Ui, UiApp};
use quench_http::prelude::Endpoint;
use quench_ui::prelude::*;

fn app(ui: Ui) -> UiApp {
    UiApp::new(ui).page("/", |_| Page::new("Home", p().text("hi")))
}

#[quench_test::test]
async fn pages_have_no_reload_script_unless_asked() {
    let r = out(app(Ui::new(big())).call(request("/", &[]).await).await).await;
    assert!(!r.text().contains("/_quench/dev"));
    let r = out(app(Ui::new(big()))
        .call(request("/_quench/dev.js", &[]).await)
        .await)
    .await;
    assert_eq!(r.status, 404);
}

#[quench_test::test]
async fn dev_mode_adds_the_script_last_and_serves_it() {
    let app = app(Ui::new(big()).dev_reload());
    let r = out(app.call(request("/", &[]).await).await).await;
    let html = r.text();
    assert!(html.ends_with("<script src=\"/_quench/dev.js\"></script></body></html>"));
    assert!(
        r.header("content-security-policy")
            .unwrap()
            .contains("script-src 'self'"),
        "the policy already allows a script from this origin"
    );
    let js = out(app.call(request("/_quench/dev.js", &[]).await).await).await;
    assert_eq!(js.status, 200);
    assert_eq!(js.header("cache-control").as_deref(), Some("no-store"));
    assert!(js.text().contains("EventSource('/_quench/dev')"));
    assert!(js.text().contains("location.reload()"));
}

#[quench_test::test]
async fn the_dev_stream_stays_open_as_server_sent_events() {
    let app = app(Ui::new(big()).dev_reload());
    let inner = app
        .call(request("/_quench/dev", &[]).await)
        .await
        .into_inner();
    assert_eq!(inner.status.as_u16(), 200);
    let kind = inner.headers.get("content-type").unwrap().to_str().unwrap();
    assert!(kind.starts_with("text/event-stream"), "{kind}");
}
