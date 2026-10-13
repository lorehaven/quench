use async_trait::async_trait;
use quench_http::prelude::http::StatusCode;
use quench_http::prelude::*;
use quench_starter::http::routers::ui::serve_assets;
use quench_web::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

/// Serves the pre-rendered pages from a path -> body map, and anything under
/// `/assets` from `dist/assets`.
struct Site {
    pages: HashMap<&'static str, (&'static str, String)>,
}

#[async_trait]
impl Endpoint for Site {
    async fn call(&self, req: Request) -> Response {
        let path = req.uri().path();
        if let Some(asset) = path.strip_prefix("/assets/") {
            return serve_assets(asset, "dist/assets").await;
        }
        match self.pages.get(path) {
            Some((content_type, body)) => {
                Response::text(StatusCode::OK, body.clone()).header("content-type", *content_type)
            }
            None => Response::not_found(),
        }
    }
}

#[tokio::main]
async fn main() {
    let app_shell = AppShellBuilder::new()
        .default_theme(Theme::BootstrapDark)
        .supported_themes(vec![Theme::BootstrapDark, Theme::BootstrapLight])
        .build();

    let index = app_shell.page(
        content()
            .class("main-content")
            .child(h1().attr("id", "root").text("Hello.")),
    );

    let about = app_shell.page(
        div()
            .class("page")
            .child(h2().text("About Us"))
            .child(p().text("We are a company that builds amazing SPAs"))
            .child(p().text("Our mission is to make the web better")),
    );

    let contact = app_shell.page(
        div()
            .class("page")
            .child(h2().text("Contact Us"))
            .child(p().text("Email: contact@myapp.com"))
            .child(p().text("Phone: +1 (123) 456-7890")),
    );

    let html = "text/html; charset=utf-8";
    let site = Site {
        pages: HashMap::from([
            ("/", (html, index)),
            ("/about", (html, about)),
            ("/contact", (html, contact)),
            (
                "/api/data",
                (html, r#"{"message": "Hello from API"}"#.to_string()),
            ),
        ]),
    };

    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let addr = "127.0.0.1:3000".parse().unwrap();

    println!("Server running on http://{addr}");

    serve_http(Arc::new(site), container, addr).await.unwrap();
}
