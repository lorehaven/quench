//! A routed app: nested layouts, a parameterised page, a guarded page and a not-found page, all
//! under `/app`. The server renders any of its URLs and the browser carries on from there.

use quench_ui::prelude::*;
use quench_ui::router::{Guard, RouteContext, Router, Routes, layout, link};
use quench_ui::virtual_list::virtual_list;

const USERS: u32 = 60;

fn page(heading: &str, body: &str) -> View {
    div()
        .child(h2().text(heading))
        .child(p().text(body))
        .into_view()
}

fn users() -> View {
    let items = (1..=USERS).map(|id| {
        li().child(link(&format!("/app/users/{id}")).text(format!("User {id}")))
            .into_view()
    });
    div()
        .child(h2().text("Users"))
        .child(ul().attr("id", "user-list").children(items))
        .into_view()
}

fn user(ctx: &RouteContext) -> View {
    let ctx = ctx.clone();
    let visits = create_signal(0);
    let bump = visits.clone();
    div()
        .child(
            h2().attr("id", "user-name")
                .text_fn(move || format!("User {}", ctx.param("id").unwrap_or_default())),
        )
        .child(
            p().attr("id", "visits")
                .text_fn(move || format!("Clicks here: {}", visits.get())),
        )
        .child(
            button()
                .attr("id", "bump")
                .text("Click")
                .on_click(move || bump.update(|n| *n += 1)),
        )
        .child(link("/app/users").attr("id", "all-users").text("All users"))
        .into_view()
}

fn big() -> View {
    let items = create_signal((1..=10_000_u32).collect::<Vec<_>>());
    div()
        .child(h2().text("10,000 rows"))
        .child(
            virtual_list(
                &items,
                32.0,
                |n| u64::from(*n),
                |n| span().text(format!("Row {n}")).into_view(),
            )
            .attr("id", "big-list")
            .class("vlist-demo"),
        )
        .into_view()
}

fn shell(ctx: &RouteContext, inner: View) -> View {
    let ctx = ctx.clone();
    div()
        .attr("id", "shell")
        .child(
            nav()
                .child(link("/app").text("Home"))
                .child(link("/app/users").text("Users"))
                .child(link("/app/big").text("Big"))
                .child(link("/app/settings/profile").text("Settings"))
                .child(link("/app/admin").text("Admin")),
        )
        .child(main().attr("id", "view").child(inner))
        .child(
            p().attr("id", "where")
                .text_fn(move || ctx.location().url()),
        )
        .into_view()
}

fn login(signed_in: &Signal<bool>) -> impl Fn(&RouteContext) -> View + use<> {
    let signed_in = signed_in.clone();
    move |ctx| {
        let (signed_in, router) = (signed_in.clone(), ctx.router().clone());
        let next = ctx
            .location()
            .query_get("next")
            .unwrap_or_else(|| "/app".into());
        div()
            .child(h2().text("Sign in"))
            .child(
                button()
                    .attr("id", "sign-in")
                    .text("Sign in")
                    .on_click(move || {
                        signed_in.set(true);
                        router.replace(&next);
                    }),
            )
            .into_view()
    }
}

/// The whole app, for a router that starts wherever the page was loaded.
#[must_use]
pub fn site(router: &Router) -> View {
    let signed_in = create_signal(false);
    let guard = signed_in.clone();
    Routes::new()
        .layout(
            layout("/app", shell)
                .route("", |_| page("Welcome", "A routed app."))
                .route("big", |_| big())
                .route("users", |_| users())
                .route("users/:id", user)
                .route("login", login(&signed_in))
                .guarded_route(
                    "admin",
                    move |visit| {
                        if guard.get() {
                            Guard::Allow
                        } else {
                            Guard::Redirect(format!("/app/login?next={}", visit.location.path))
                        }
                    },
                    |_| page("Admin", "Only for the signed in."),
                )
                .layout(
                    layout("settings", |_, inner| {
                        div()
                            .child(h2().text("Settings"))
                            .child(link("/app/settings/profile").text("Profile"))
                            .child(link("/app/settings/about").text("About"))
                            .child(inner)
                            .into_view()
                    })
                    .route("profile", |_| page("Profile", "Your profile."))
                    .route("about", |_| page("About", "About this app.")),
                ),
        )
        .not_found(|_| page("Not found", "There is nothing here."))
        .view(router)
}
