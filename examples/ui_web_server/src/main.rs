//! Serves the `ui_web_app` example, rendering the page on the server. Build the app first:
//!
//! ```text
//! cargo build --target wasm32-unknown-unknown --profile wasm -p quench-example-ui-web-app
//! cargo run -p quench-example-ui-web-server            # then open http://127.0.0.1:3000
//! ```
//!
//! `PORT` and `WASM` (the path of the module) override the defaults. The page, `quench.js` and the
//! module are served by `quench-fullstack` with content-hashed names and a strict
//! Content-Security-Policy; `view-source:` shows the page already rendered.
//!
//! Hooks for the browser test (this is an example: do not copy them into a service):
//! `/?corrupt=<kind>` damages the server's HTML (`tag`, `text`, `drop`, `extra`, `attr`, `empty`),
//! and `/__delay?ms=N` makes the module arrive N milliseconds late, so a test can look at the page
//! before the browser has hydrated it.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use quench_core::async_trait;
use quench_example_ui_web_app::{AppState, ServerTodo, demo};
use quench_fullstack::{Page, Ui, UiApp};
use quench_http::prelude::http::StatusCode;
use quench_http::prelude::*;

const STYLE: &str = "body{font-family:sans-serif;margin:2rem auto;max-width:36rem;padding:0 1rem}\
.card{border:1px solid #ccc;border-radius:.5rem;padding:1rem;margin:1rem 0}\
li.done span{text-decoration:line-through;color:#888}li{margin:.25rem 0}button{margin-left:.5rem}";

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
    wasm_path: String,
    delay_ms: AtomicU64,
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
            "/__delay" => {
                let ms = query
                    .strip_prefix("ms=")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(0);
                self.delay_ms.store(ms, Ordering::Relaxed);
                return Response::text(StatusCode::OK, format!("delay {ms}ms"));
            }
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
    let style = ui.add_asset("site.css", STYLE, "text/css");
    let wasm_path = ui.paths().1.to_string();
    let app = UiApp::new(ui).page("/", move |_req| {
        let state = AppState {
            server_todos: todos(),
        };
        Page::new("quench-ui", demo(Some(state.clone())))
            .stylesheet(&style)
            .state(state.to_json())
    });
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let addr = format!("127.0.0.1:{port}").parse().unwrap();
    println!("http://{addr}  (wasm: {wasm_path})");
    let site = Site {
        app,
        wasm_path,
        delay_ms: AtomicU64::new(0),
    };
    serve_http(Arc::new(site), container, addr).await.unwrap();
}
