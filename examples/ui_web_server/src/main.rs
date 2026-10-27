//! Serves the `ui_web_app` example. Build the app first:
//!
//! ```text
//! cargo build --target wasm32-unknown-unknown --profile wasm -p quench-example-ui-web-app
//! cargo run -p quench-example-ui-web-server            # then open http://127.0.0.1:3000
//! ```
//!
//! `PORT` and `WASM` (the path of the module) override the defaults. Every response carries a
//! strict Content-Security-Policy, which the page satisfies: no inline script, no inline handler.

use std::sync::Arc;

use quench_core::async_trait;
use quench_http::prelude::http::StatusCode;
use quench_http::prelude::*;

const INDEX: &str = r#"<!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>quench-ui</title>
<link rel="stylesheet" href="/style.css">
</head>
<body>
<div id="app"></div>
<script type="module" src="/quench.js?wasm=/app.wasm"></script>
</body>
</html>
"#;

const STYLE: &str = "body{font-family:sans-serif;margin:2rem auto;max-width:36rem;padding:0 1rem}\
.card{border:1px solid #ccc;border-radius:.5rem;padding:1rem;margin:1rem 0}\
li.done span{text-decoration:line-through;color:#888}li{margin:.25rem 0}button{margin-left:.5rem}";

const TODOS: &str = r#"[{"id":1,"title":"read the contract","done":true},{"id":2,"title":"build the wasm","done":false},{"id":3,"title":"open the page","done":false}]"#;

const CSP: &str = "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; connect-src 'self'";

struct Site {
    wasm: String,
}

#[async_trait]
impl Endpoint for Site {
    async fn call(&self, req: Request) -> Response {
        let (content_type, body): (&str, Vec<u8>) = match req.uri().path() {
            "/" => ("text/html; charset=utf-8", INDEX.as_bytes().to_vec()),
            "/style.css" => ("text/css", STYLE.as_bytes().to_vec()),
            "/quench.js" => (
                "text/javascript",
                quench_ui_web::QUENCH_JS.as_bytes().to_vec(),
            ),
            "/favicon.ico" => return Response::new(StatusCode::NO_CONTENT),
            "/api/todos" => ("application/json", TODOS.as_bytes().to_vec()),
            "/app.wasm" => match std::fs::read(&self.wasm) {
                Ok(bytes) => ("application/wasm", bytes),
                Err(e) => {
                    let message = format!("cannot read {}: {e} - build the app first", self.wasm);
                    return Response::text(StatusCode::NOT_FOUND, message);
                }
            },
            _ => return Response::not_found(),
        };
        Response::from_bytes(StatusCode::OK, body.into())
            .header("content-type", content_type)
            .header("content-security-policy", CSP)
            .header("cache-control", "no-store")
    }
}

#[tokio::main]
async fn main() {
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let wasm = std::env::var("WASM").unwrap_or_else(|_| {
        "target/wasm32-unknown-unknown/wasm/quench_example_ui_web_app.wasm".to_string()
    });
    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let addr = format!("127.0.0.1:{port}").parse().unwrap();
    println!("http://{addr}  (wasm: {wasm})");
    serve_http(Arc::new(Site { wasm }), container, addr)
        .await
        .unwrap();
}
