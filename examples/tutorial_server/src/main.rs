//! Serves the tutorial app. Build the browser half first, then run the server:
//!
//! ```text
//! cargo build --target wasm32-unknown-unknown --profile wasm -p quench-example-tutorial-app
//! cargo run -p quench-example-tutorial-server        # then open http://127.0.0.1:3000
//! ```
//!
//! `PORT` and `WASM` (the path of the module) override the defaults.

use std::sync::Arc;

use quench_example_tutorial_app::{pantry_view, store};
use quench_fullstack::{Page, Ui, UiApp};
use quench_http::prelude::*;

const STYLE: &str = "body{font-family:sans-serif;margin:2rem auto;max-width:32rem;padding:0 1rem}\
.card{border:1px solid #ccc;border-radius:.5rem;padding:1rem}\
li.bought span{text-decoration:line-through;color:#888}li{margin:.25rem 0}button{margin-left:.5rem}\
.error{color:#b00020;min-height:1.2em}";

#[tokio::main]
async fn main() {
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());
    let wasm = std::env::var("WASM").unwrap_or_else(|_| {
        "target/wasm32-unknown-unknown/wasm/quench_example_tutorial_app.wasm".to_string()
    });
    let bytes = std::fs::read(&wasm)
        .unwrap_or_else(|e| panic!("cannot read {wasm}: {e} - build the app first"));

    let mut ui = Ui::new(bytes);
    let css = ui.add_asset("site.css", STYLE, "text/css");
    let app = UiApp::new(ui).page("/", move |_req| {
        let items = store::all();
        Page::new("Pantry", pantry_view(items.clone()))
            .stylesheet(&css)
            .state(quench_serde::json::to_string(&items))
    });

    let container = Arc::new(ContainerBuilder::new().build().await.unwrap());
    let addr = format!("127.0.0.1:{port}").parse().unwrap();
    println!("http://{addr}");
    serve_http(Arc::new(app), container, addr).await.unwrap();
}
