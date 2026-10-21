use quench_auth::domain::jwt::JwtConfig;
use quench_http::prelude::{Request, Response};
use quench_web::prelude::*;

pub async fn handle_home<F>(req: &Request, config: &JwtConfig, render_fn: F) -> Response
where
    F: FnOnce() -> Response,
{
    // Check authentication using the utility functions from the parent module
    if !crate::http::routers::ui::is_ui_authenticated(req, config).await {
        return crate::http::routers::ui::ui_login_redirect();
    }
    render_fn()
}

#[must_use]
pub fn service_card(href: &str, title_key: &str, desc_key: &str, extra_class: &str) -> Element {
    a().attr("href", href)
        .class(format!("home-card {extra_class}"))
        .child(
            div()
                .class("home-card-body")
                .child(div().class("home-card-title").attr("data-i18n", title_key))
                .child(div().class("home-card-desc").attr("data-i18n", desc_key)),
        )
        .child(div().class("home-card-arrow").text("→"))
}
