//! Serves the `ui_web_app` example, rendering pages on the server. Build the app first:
//!
//! ```text
//! cargo build --target wasm32-unknown-unknown --profile wasm -p quench-example-ui-web-app
//! cargo run -p quench-example-ui-web-server            # then open http://127.0.0.1:3000
//! ```
//!
//! `PORT` and `WASM` (the path of the module) override the defaults. `QUENCH_EXPORT=<dir>` writes
//! the gallery as a static bundle for the Android host and exits. Pages, `quench.js` and the
//! module are served by `quench-fullstack` with content-hashed names and a strict
//! Content-Security-Policy; `view-source:` shows a page already rendered.
//!
//! - `/` is the demo (counter, todos, a list the server embedded).
//! - `/notes` is the notes app: sign in as `ola` / `pw` or `ewa` / `pw2`. Its calls are `#[server]`
//!   functions, and `/api/v1/auth/*` is a gatehouse-compatible stub that issues real signed
//!   tokens. `/api/notes/stream` pushes a server-sent event when your notes change.
//!
//! Hooks for the browser test (this is an example: do not copy them into a service):
//! `/?corrupt=<kind>` damages the server's HTML (`tag`, `text`, `drop`, `extra`, `attr`, `empty`),
//! and `/__delay?ms=N` makes the module arrive N milliseconds late, so a test can look at the page
//! before the browser has hydrated it.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use quench_auth::domain::jwt::JwtConfig;
use quench_core::async_trait;
use quench_core::stream;
use quench_example_ui_web_app::notes::{self, NotesState, store};
use quench_example_ui_web_app::{AppState, PageState, ServerTodo, demo, gallery, site};
use quench_fullstack::testing::StubGatehouse;
use quench_fullstack::{Page, Ui, UiApp, User};
use quench_http::prelude::http::StatusCode;
use quench_http::prelude::*;
use quench_proto::sse::Event;
use quench_style::theme::Theme;

const GALLERY_STYLE: &str = "body{--q-fab-offset:72px;margin:0;font-family:system-ui,sans-serif;background:var(--bs-gray-950);color:var(--bs-gray-100);padding-bottom:72px}\
#gallery{max-width:40rem;margin:0 auto}#gallery>*:not(.q-appbar):not(.q-bottomnav){padding-left:var(--q-space-4);padding-right:var(--q-space-4)}\
.q-bottomnav{position:fixed;left:0;right:0;bottom:0}button:not([class]){min-height:44px;margin:.25rem;font:inherit;color:var(--bs-gray-100);background:var(--bs-gray-800);border:2px solid var(--bs-gray-400);border-radius:.5rem;padding:0 1rem}\
button:focus-visible{outline:3px solid var(--bs-gray-100);outline-offset:2px}\
.gallery-pad{height:10rem;display:flex;align-items:center;justify-content:center;border:2px dashed var(--bs-gray-400);border-radius:.5rem;user-select:none}\
.gallery-card{padding:1rem;margin-top:1rem;background:var(--bs-gray-900);border-radius:.5rem}\
.gallery-rows{height:18rem;border:1px solid var(--bs-gray-700)}#pull{max-height:14rem;border:1px solid var(--bs-gray-700)}";

const STYLE: &str = "body{font-family:sans-serif;margin:2rem auto;max-width:36rem;padding:0 1rem}\
.card{border:1px solid #ccc;border-radius:.5rem;padding:1rem;margin:1rem 0}\
li.done span{text-decoration:line-through;color:#888}li{margin:.25rem 0}button{margin-left:.5rem}\
.error{color:#b00020;min-height:1.2em}\
.vlist-demo{height:20rem;border:1px solid #ccc}";

fn todos() -> Vec<ServerTodo> {
    [
        ("read the contract", true),
        ("build the wasm", false),
        ("open the page", false),
    ]
    .into_iter()
    .zip(1..)
    .map(|((title, done), id)| ServerTodo {
        id,
        title: title.to_string(),
        done,
    })
    .collect()
}

/// Damages the server's HTML in one way, to see what hydration does with it.
fn corrupt(kind: &str, html: String) -> String {
    match kind {
        "tag" => html.replacen("<h2>Counter</h2>", "<h3>Counter</h3>", 1),
        "text" => html.replacen("0 left", "9 left", 1),
        "drop" => html.replacen("<button id=\"up\">+</button>", "", 1),
        "extra" => html.replacen("<h2>Todos</h2>", "<em>extra</em><h2>Todos</h2>", 1),
        "attr" => html.replacen("what needs doing?", "WRONG", 1),
        "empty" => {
            let start = html
                .find("<div id=\"app\">")
                .map_or(0, |i| i + "<div id=\"app\">".len());
            let end = html.find("</div><script").unwrap_or(start);
            format!("{}{}", &html[..start], &html[end..])
        }
        _ => html,
    }
}

struct Site {
    app: UiApp,
    auth: StubGatehouse,
    wasm_path: String,
    delay_ms: AtomicU64,
}

impl Site {
    /// Server-sent events: one `note` event each time one of the signed-in user's notes changes.
    async fn stream(&self, mut req: Request) -> Response {
        let Ok(user) = User::from_request(&mut req).await else {
            return Response::new(StatusCode::UNAUTHORIZED);
        };
        let prefix = format!("{} ", user.sub);
        let events = stream::unfold(store::subscribe(), move |mut changes| {
            let prefix = prefix.clone();
            async move {
                loop {
                    match changes.recv().await {
                        Ok(message) if message.starts_with(&prefix) => {
                            return Some((Event::new().event("note").data(message), changes));
                        }
                        Ok(_) | Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => {}
                        Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
                    }
                }
            }
        });
        Response::sse(events).header("cache-control", "no-store")
    }
}

#[async_trait]
impl Endpoint for Site {
    async fn call(&self, req: Request) -> Response {
        let path = req.uri().path().to_string();
        let query = req.uri().query().unwrap_or("").to_string();
        match path.as_str() {
            "/favicon.ico" => return Response::new(StatusCode::NO_CONTENT),
            "/api/todos" => {
                let list = quench_serde::json::to_string(&todos());
                return Response::from_bytes(StatusCode::OK, list.into_bytes().into())
                    .header("content-type", "application/json")
                    .header("cache-control", "no-store");
            }
            "/api/notes/stream" => return self.stream(req).await,
            "/__delay" => {
                let ms = query
                    .strip_prefix("ms=")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                self.delay_ms.store(ms, Ordering::Relaxed);
                return Response::text(StatusCode::OK, format!("delay {ms}ms"));
            }
            _ if path.starts_with("/api/v1/auth/") => return self.auth.call(req).await,
            _ => {}
        }
        if path == self.wasm_path {
            let ms = self.delay_ms.load(Ordering::Relaxed);
            if ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(ms)).await;
            }
        }
        let corruption = query.strip_prefix("corrupt=").map(str::to_string);
        let response = self.app.call(req).await;
        let Some(kind) = corruption else {
            return response;
        };
        let inner = response.into_inner();
        let body = inner
            .body
            .collect_limited(1 << 22)
            .await
            .unwrap_or_default();
        let html = corrupt(&kind, String::from_utf8_lossy(&body).into_owned());
        let mut out = Response::html(StatusCode::OK, html);
        for name in ["cache-control", "content-security-policy"] {
            if let Some(v) = inner.headers.get(name).and_then(|v| v.to_str().ok()) {
                out = out.header(name, v);
            }
        }
        out
    }
}

#[tokio::main]
async fn main() {
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let wasm = std::env::var("WASM").unwrap_or_else(|_| {
        "target/wasm32-unknown-unknown/wasm/quench_example_ui_web_app.wasm".to_string()
    });
    let bytes = std::fs::read(&wasm)
        .unwrap_or_else(|e| panic!("cannot read {wasm}: {e} - build the app first"));
    let mut ui = Ui::new(bytes);
    let sheet = format!("{STYLE}\n{}", quench_style::components::stylesheet());
    let style = ui.add_asset("site.css", sheet, "text/css");
    let gallery_css = format!(
        "{}\n{}\n{}\n{GALLERY_STYLE}",
        quench_style::tokens::stylesheet(),
        quench_style::mobile::stylesheet(),
        quench_style::components::stylesheet(),
    );
    let gallery_style = ui.add_asset("gallery.css", gallery_css, "text/css");
    let dark = ui.add_asset(
        "theme-dark.css",
        Theme::theme(Theme::DefaultDark),
        "text/css",
    );
    let light = ui.add_asset(
        "theme-light.css",
        Theme::theme(Theme::DefaultLight),
        "text/css",
    );
    let wasm_path = ui.paths().1.to_string();
    let (demo_style, notes_style, site_style) = (style.clone(), style.clone(), style);

    let gallery_page = move || {
        Page::new("Gallery", gallery::gallery())
            .stylesheet_for(&dark, "(prefers-color-scheme: dark)")
            .stylesheet_for(&light, "(prefers-color-scheme: light)")
            .stylesheet(&gallery_style)
            .state(PageState::Gallery.to_json())
    };
    let export = gallery_page.clone();
    let app = UiApp::new(ui)
        .page("/", move |_req| {
            let state = AppState {
                server_todos: todos(),
            };
            Page::new("quench-ui", demo(Some(state.clone())))
                .stylesheet(&demo_style)
                .state(PageState::Demo(state).to_json())
        })
        .page("/gallery", move |_req| gallery_page())
        .pages_under("/app", move |req| {
            Page::routed("Routed app", req.uri().path_and_query(), site::site)
                .stylesheet(&site_style)
                .state(PageState::Site.to_json())
        })
        .page_for_user("/notes", move |_req, user| {
            let state = NotesState {
                notes: user
                    .as_ref()
                    .map(|u| store::notes_of(&u.sub))
                    .unwrap_or_default(),
                user: user.map(|u| u.sub),
            };
            Page::new("Notes", notes::notes_view(state.clone()))
                .stylesheet(&notes_style)
                .state(PageState::Notes(state).to_json())
        });

    // `QUENCH_EXPORT=<dir>` writes the gallery as a static bundle (what the Android host serves)
    // instead of starting the server.
    if let Some(dir) = std::env::var_os("QUENCH_EXPORT") {
        app.ui()
            .export(std::path::Path::new(&dir), export())
            .expect("cannot write the bundle");
        println!("bundle written to {}", std::path::Path::new(&dir).display());
        return;
    }

    // The stub issues the tokens; the service verifies them with the same configuration, as a
    // relying party would verify gatehouse's.
    let auth = StubGatehouse::new(&[("ola", "pw"), ("ewa", "pw2")]);
    let mut jwt: JwtConfig = auth.jwt();
    jwt.auth_enabled = true;
    let container = Arc::new(ContainerBuilder::new().provide(jwt).build().await.unwrap());
    let addr = format!("127.0.0.1:{port}").parse().unwrap();
    println!("http://{addr}  (wasm: {wasm_path})");
    let site = Site {
        app,
        auth,
        wasm_path,
        delay_ms: AtomicU64::new(0),
    };
    serve_http(Arc::new(site), container, addr).await.unwrap();
}
