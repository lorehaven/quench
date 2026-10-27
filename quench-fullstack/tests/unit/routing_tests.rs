//! Deep links: pages under a prefix, rendered by the router at the request's URL, with redirects
//! and a 404 for what no route matches.

use super::support::*;
use quench_fullstack::{Page, Ui, UiApp};
use quench_http::prelude::{Endpoint, Request};
use quench_ui::prelude::*;
use quench_ui::router::{Guard, Routes, layout, link};

fn path_and_query(req: &Request) -> String {
    req.uri().path_and_query().to_string()
}

fn app() -> UiApp {
    UiApp::new(Ui::new(big())).pages_under("/app/", |req| {
        Page::routed("App", &path_and_query(req), |router| {
            Routes::new()
                .layout(
                    layout("/app", |_, inner| {
                        div()
                            .child(nav().child(link("/app/users")))
                            .child(inner)
                            .into_view()
                    })
                    .route("", |_| p().text("index").into_view())
                    .route("users/:id", |ctx| {
                        p().text(format!("user {}", ctx.param("id").unwrap()))
                            .into_view()
                    })
                    .route("users", |_| p().text("all users").into_view()),
                )
                .guarded_route(
                    "/secret",
                    |_| Guard::Redirect("/app/users?from=secret".into()),
                    |_| p().into_view(),
                )
                .not_found(|_| p().text("no such page").into_view())
                .view(router)
        })
    })
}

#[quench_test::test]
async fn a_deep_link_is_rendered_at_its_own_url() {
    let app = app();
    let r = out(app.call(request("/app/users/7", &[]).await).await).await;
    assert_eq!(r.status, 200);
    assert!(r.text().contains("<p>user 7</p>"), "{}", r.text());
    assert!(r.text().contains("<a href=\"/app/users\">"));
    let index = out(app.call(request("/app", &[]).await).await).await;
    assert!(index.text().contains("<p>index</p>"));
    let index = out(app.call(request("/app/", &[]).await).await).await;
    assert!(index.text().contains("<p>index</p>"));
}

#[quench_test::test]
async fn a_url_no_route_matches_is_a_404_with_the_not_found_page() {
    let r = out(app().call(request("/app/zzz", &[]).await).await).await;
    assert_eq!(r.status, 404);
    assert!(r.text().contains("no such page"));
}

#[quench_test::test]
async fn a_guard_redirect_is_a_302() {
    let r = out(app().call(request("/secret", &[]).await).await).await;
    assert_eq!(r.status, 404, "outside the prefix nothing serves it");
    let app = UiApp::new(Ui::new(big())).pages_under("/secret", |req| {
        Page::routed("S", &path_and_query(req), |router| {
            Routes::new()
                .guarded_route(
                    "/secret",
                    |_| Guard::Redirect("/login?next=%2Fsecret".into()),
                    |_| p().into_view(),
                )
                .route("/login", |_| p().text("login").into_view())
                .view(router)
        })
    });
    let r = out(app.call(request("/secret", &[]).await).await).await;
    assert_eq!(r.status, 302);
    assert_eq!(
        r.header("location").as_deref(),
        Some("/login?next=%2Fsecret")
    );
    assert_eq!(r.header("cache-control").as_deref(), Some("no-store"));
}

#[quench_test::test]
async fn a_page_can_just_redirect() {
    let app = UiApp::new(Ui::new(big())).page("/old", |_| Page::redirect("/new"));
    let r = out(app.call(request("/old", &[]).await).await).await;
    assert_eq!(
        (r.status, r.header("location").as_deref()),
        (302, Some("/new"))
    );
}

#[quench_test::test]
async fn the_prefix_matches_whole_segments_only_and_exact_pages_win() {
    let app = UiApp::new(Ui::new(big()))
        .pages_under("/app", |_| Page::new("under", "under"))
        .page("/app/special", |_| Page::new("exact", "exact"));
    for (path, shown) in [
        ("/app", "under"),
        ("/app/x/y", "under"),
        ("/app/special", "exact"),
    ] {
        let r = out(app.call(request(path, &[]).await).await).await;
        assert!(
            r.text().contains(&format!("<div id=\"app\">{shown}")),
            "{path}: {}",
            r.text()
        );
    }
    assert_eq!(
        out(app.call(request("/apple", &[]).await).await)
            .await
            .status,
        404
    );
}

#[quench_test::test]
async fn a_missing_favicon_is_a_quiet_204_unless_the_app_serves_one() {
    let r = out(app().call(request("/favicon.ico", &[]).await).await).await;
    assert_eq!(r.status, 204);
    assert!(r.header("cache-control").unwrap().contains("max-age"));
    let own = UiApp::new(Ui::new(big())).page("/favicon.ico", |_| Page::new("icon", p()));
    let r = out(own.call(request("/favicon.ico", &[]).await).await).await;
    assert_eq!(r.status, 200, "a page registered at the path wins");
}
