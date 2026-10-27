//! Routing: URLs and patterns, nested layouts that stay mounted, guards and redirects, history.

use quench_ui::prelude::*;
use quench_ui::router::{
    Guard, History, Location, MemoryHistory, Params, Router, Routes, layout, link, percent_decode,
    use_router,
};
use std::cell::Cell;
use std::rc::Rc;

fn page(text: &'static str) -> impl Fn(&quench_ui::router::RouteContext) -> View {
    move |_| p().text(text).into_view()
}

#[quench_test::test]
fn locations_parse_and_print() {
    let l = Location::parse("/a/b?x=1&y=two%20words+more#top");
    assert_eq!((l.path.as_str(), l.hash.as_str()), ("/a/b", "top"));
    assert_eq!(l.query_get("y").as_deref(), Some("two words more"));
    assert_eq!(l.query_get("z"), None);
    assert_eq!(l.query_pairs().len(), 2);
    assert_eq!(l.url(), "/a/b?x=1&y=two%20words+more#top");
    assert_eq!(Location::parse("").path, "/");
    assert_eq!(Location::parse("/p").url(), "/p");
    assert_eq!(
        Location::parse("/q?flag").query_get("flag").as_deref(),
        Some("")
    );
    assert_eq!(percent_decode("a%20b%zz%4"), "a b%zz%4");
    assert_eq!(percent_decode("%C5%82"), "\u{142}");
    assert_eq!(Params::default().get("x"), None);
}

struct App {
    r: Rc<TestRenderer>,
    router: Router,
    history: Rc<MemoryHistory>,
    _mount: Mount<TestRenderer>,
}

impl App {
    /// The markup without the empty comments that mark dynamic regions.
    fn html(&self) -> String {
        self.r.html().replace("<!---->", "")
    }
}

fn app(start: &str, routes: impl FnOnce() -> Routes + 'static) -> App {
    let r = Rc::new(TestRenderer::new());
    let history = Rc::new(MemoryHistory::new(start));
    let h: Rc<dyn History> = history.clone();
    let router = Router::new(h);
    let mount = {
        let (router, routes) = (router.clone(), Cell::new(Some(routes)));
        mount(
            &r,
            &r.root(),
            View::dynamic(move || routes.take().expect("built once")().view(&router)),
        )
    };
    App {
        r,
        router,
        history,
        _mount: mount,
    }
}

#[quench_test::test]
fn the_first_matching_route_renders_and_navigation_swaps_it() {
    let a = app("/", || {
        Routes::new()
            .route("/", page("home"))
            .route("/about", page("about"))
            .route("/files/*path", |ctx| {
                let ctx = ctx.clone();
                p().text_fn(move || ctx.param("path").unwrap_or_default())
                    .into_view()
            })
    });
    assert_eq!(a.html(), "<p>home</p>");
    a.router.navigate("/about");
    assert_eq!(a.html(), "<p>about</p>");
    a.router.navigate("/files/a%20b/c.txt");
    assert_eq!(a.html(), "<p>a b/c.txt</p>");
    assert_eq!(a.router.location().path, "/files/a%20b/c.txt");
    // an unknown URL with no not_found shows a plain message
    a.router.navigate("/nope");
    assert_eq!(a.r.text(), "Not found");
}

#[quench_test::test]
fn params_follow_navigation_without_rebuilding_the_page() {
    let built = Rc::new(Cell::new(0));
    let b = built.clone();
    let a = app("/users/1", move || {
        Routes::new().route("/users/:id", move |ctx| {
            b.set(b.get() + 1);
            let ctx = ctx.clone();
            p().text_fn(move || format!("user {}", ctx.param("id").unwrap()))
                .into_view()
        })
    });
    assert_eq!(a.r.text(), "user 1");
    let node = a.r.find_by_tag("p")[0];
    a.router.navigate("/users/2");
    assert_eq!(a.r.text(), "user 2");
    assert_eq!(a.r.find_by_tag("p")[0], node, "same node, patched in place");
    assert_eq!(built.get(), 1);
}

#[quench_test::test]
fn layouts_stay_mounted_while_their_children_change() {
    let shell_built = Rc::new(Cell::new(0));
    let sb = shell_built.clone();
    let a = app("/app/users", move || {
        Routes::new()
            .layout(
                layout("/app", move |ctx, inner| {
                    sb.set(sb.get() + 1);
                    let ctx = ctx.clone();
                    div()
                        .child(
                            nav()
                                .child(link("/app/users"))
                                .child(link("/app/settings/about")),
                        )
                        .child(main().child(inner))
                        .child(span().text_fn(move || ctx.location().path))
                        .into_view()
                })
                .route("", page("index"))
                .route("users", page("users"))
                .layout(
                    layout("settings", |_, inner| section().child(inner).into_view())
                        .route("profile", page("profile"))
                        .route("about", page("about")),
                ),
            )
            .not_found(page("lost"))
    });
    let html = a.html();
    assert!(html.contains("<main><p>users</p></main>"), "{html}");
    let nav_node = a.r.find_by_tag("nav")[0];
    for (url, shown) in [
        ("/app", "<main><p>index</p></main>"),
        ("/app/", "<main><p>index</p></main>"),
        (
            "/app/settings/profile",
            "<main><section><p>profile</p></section></main>",
        ),
        (
            "/app/settings/about",
            "<main><section><p>about</p></section></main>",
        ),
        ("/app/users", "<main><p>users</p></main>"),
    ] {
        a.router.navigate(url);
        assert!(a.html().contains(shown), "{url}: {}", a.html());
    }
    assert_eq!(a.r.find_by_tag("nav")[0], nav_node);
    assert_eq!(shell_built.get(), 1);
    a.router.navigate("/app/settings/zzz");
    assert_eq!(a.r.text(), "lost");
    a.router.navigate("/other");
    assert_eq!(a.r.text(), "lost");
    a.router.navigate("/app/users");
    assert_eq!(
        shell_built.get(),
        2,
        "the layout was left and entered again"
    );
}

#[quench_test::test]
fn links_navigate_and_mark_the_current_page() {
    let a = app("/", || {
        Routes::new()
            .route("/", |_| {
                div()
                    .child(link("/a").text("a"))
                    .child(link("https://example.com/").text("out"))
                    .child(link("//cdn.example/x").text("proto"))
                    .into_view()
            })
            .route("/a", |_| link("/").text("home").into_view())
    });
    let links = a.r.find_by_tag("a");
    assert_eq!(a.r.attr(links[0], "href").as_deref(), Some("/a"));
    assert_eq!(a.r.attr(links[1], "aria-current"), None);
    a.r.fire(links[1], &Event::Click); // an external link has no listener
    assert_eq!(a.router.location().path, "/");
    a.r.fire(links[0], &Event::Click);
    assert_eq!(a.router.location().path, "/a");
    let home = a.r.find_by_tag("a")[0];
    assert_eq!(a.r.attr(home, "aria-current"), None);
    a.router.navigate("/a");
    assert_eq!(a.history.entries().len(), 2, "the same URL again replaces");
}

#[quench_test::test]
fn the_current_link_is_marked() {
    let a = app("/b", || {
        Routes::new().route("/b", |_| {
            div()
                .child(link("/b").text("b"))
                .child(link("/c").text("c"))
                .into_view()
        })
    });
    let links = a.r.find_by_tag("a");
    assert_eq!(a.r.attr(links[0], "aria-current").as_deref(), Some("page"));
    assert_eq!(a.r.attr(links[1], "aria-current"), None);
}

#[quench_test::test]
fn guards_redirect_and_deny() {
    let signed_in = create_signal(false);
    let s = signed_in.clone();
    let a = app("/admin?tab=2", move || {
        let (s1, s2) = (s.clone(), s.clone());
        Routes::new()
            .route("/login", page("login"))
            .guarded_route(
                "/admin",
                move |_| {
                    if s1.get() {
                        Guard::Allow
                    } else {
                        Guard::Redirect("/login".into())
                    }
                },
                page("admin"),
            )
            .layout(
                layout("/staff", |_, inner| div().child(inner).into_view())
                    .guard(move |_| if s2.get() { Guard::Allow } else { Guard::Deny })
                    .route("desk", page("desk")),
            )
            .forbidden(page("no"))
    });
    assert_eq!(a.r.text(), "login");
    assert_eq!(
        a.router.location().url(),
        "/login",
        "the redirect replaced the entry"
    );
    assert_eq!(a.history.entries().len(), 1);
    a.router.navigate("/staff/desk");
    assert_eq!(a.r.text(), "no");
    // a guard that reads a signal re-decides when it changes
    signed_in.set(true);
    assert_eq!(a.r.text(), "desk");
    a.router.navigate("/admin");
    assert_eq!(a.r.text(), "admin");
    signed_in.set(false);
    assert_eq!(a.r.text(), "login");
}

#[quench_test::test]
fn a_redirect_loop_ends_in_not_found() {
    let a = app("/a", || {
        Routes::new()
            .guarded_route("/a", |_| Guard::Redirect("/b".into()), page("a"))
            .guarded_route("/b", |_| Guard::Redirect("/a".into()), page("b"))
            .route("/ok", page("ok"))
    });
    assert_eq!(a.r.text(), "Not found");
    a.router.navigate("/ok");
    assert_eq!(a.r.text(), "ok");
}

#[quench_test::test]
fn a_redirect_to_nowhere_still_lands_on_not_found() {
    let a = app("/old", || {
        Routes::new()
            .guarded_route("/old", |_| Guard::Redirect("/gone".into()), page("old"))
            .not_found(page("404"))
    });
    assert_eq!(a.r.text(), "404");
    assert_eq!(a.router.location().path, "/gone");
}

#[quench_test::test]
fn history_back_and_forward_update_the_view() {
    let a = app("/", || {
        Routes::new()
            .route("/", page("one"))
            .route("/two", page("two"))
    });
    a.router.navigate("/two");
    assert_eq!(a.r.text(), "two");
    assert!(a.router.back());
    assert_eq!(a.r.text(), "one");
    assert!(!a.router.back());
    assert!(a.router.forward());
    assert_eq!(a.r.text(), "two");
    assert!(!a.router.forward());
    // navigating from the middle drops the forward entries
    assert!(a.router.back());
    a.router.navigate("/two?x=1");
    assert_eq!(a.history.entries().len(), 2);
    assert!(!a.router.forward());
    assert!(format!("{:?}", a.router).contains("/two?x=1"));
}

#[quench_test::test]
fn the_back_button_goes_to_handlers_first() {
    let a = app("/", || {
        Routes::new()
            .route("/", page("one"))
            .route("/two", page("two"))
    });
    let sheet_open = Rc::new(Cell::new(true));
    let (scope_router, open) = (a.router.clone(), sheet_open.clone());
    let (_, scope) = create_root(move || {
        scope_router.on_back(move || open.replace(false));
    });
    a.router.navigate("/two");
    assert!(a.router.handle_back(), "the sheet took it");
    assert_eq!(a.r.text(), "two");
    assert!(!sheet_open.get());
    scope.dispose();
    assert!(a.router.handle_back(), "no handler left: history goes back");
    assert_eq!(a.r.text(), "one");
    assert!(!a.router.handle_back(), "nothing left: the host leaves");
}

#[quench_test::test]
fn use_router_finds_the_router_under_the_view() {
    let found = Rc::new(Cell::new(false));
    let f = found.clone();
    let _a = app("/", move || {
        Routes::new().route("/", move |ctx| {
            f.set(use_router().location() == ctx.router().location());
            p().into_view()
        })
    });
    assert!(found.get());
}

#[quench_test::test]
fn click_interception_is_a_renderer_hint() {
    let r = Rc::new(TestRenderer::new());
    let m = mount(
        &r,
        &r.root(),
        a().attr("href", "/x").intercept_click().on_click(|| {}),
    );
    m.unmount();
}
