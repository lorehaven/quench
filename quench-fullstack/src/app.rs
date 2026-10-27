//! A `quench-http` endpoint that serves server-rendered quench-ui pages and their assets.

use std::sync::Arc;

use quench_core::async_trait;
use quench_http::prelude::{Endpoint, Request, Response};

use crate::assets::{Assets, PREFIX};
use crate::document::{Page, document_response, new_nonce, render_document};

/// The assets of one app and how pages that start it are written.
#[derive(Clone, Debug)]
pub struct Ui {
    assets: Assets,
    script: String,
    wasm: String,
}

impl Ui {
    /// An app whose compiled module is `wasm` (the bytes of the `.wasm` file). `quench.js` is
    /// added from `quench-ui-web`.
    #[must_use]
    pub fn new(wasm: impl Into<Vec<u8>>) -> Self {
        Self::with_script(quench_ui_web::QUENCH_JS, wasm)
    }

    /// As [`Ui::new`] with another loader script than the one in `quench-ui-web`.
    #[must_use]
    pub fn with_script(script: impl Into<Vec<u8>>, wasm: impl Into<Vec<u8>>) -> Self {
        let mut assets = Assets::new();
        let script = assets
            .add("quench.js", script, "text/javascript; charset=utf-8")
            .to_string();
        let wasm = assets.add("app.wasm", wasm, "application/wasm").to_string();
        Self {
            assets,
            script,
            wasm,
        }
    }

    /// Serves another file under the hashed prefix, such as a stylesheet, and returns its path.
    pub fn add_asset(
        &mut self,
        name: &str,
        body: impl Into<Vec<u8>>,
        content_type: &str,
    ) -> String {
        self.assets.add(name, body, content_type).to_string()
    }

    #[must_use]
    pub const fn assets(&self) -> &Assets {
        &self.assets
    }

    /// The hashed paths of `quench.js` and of the app's module.
    #[must_use]
    pub fn paths(&self) -> (&str, &str) {
        (&self.script, &self.wasm)
    }

    /// Renders `page` with a fresh nonce.
    #[must_use]
    pub fn render(&self, page: Page) -> Response {
        self.render_with_nonce(page, &new_nonce())
    }

    /// Renders `page` with a chosen nonce. For tests; use [`Ui::render`] otherwise, since a nonce
    /// that repeats protects nothing.
    #[must_use]
    pub fn render_with_nonce(&self, page: Page, nonce: &str) -> Response {
        let status = page.status;
        let html = render_document(page, nonce, &self.script, &self.wasm);
        document_response(status, html, nonce)
    }
}

type PageFn = Box<dyn Fn(&Request) -> Page + Send + Sync>;

/// Pages at exact paths, plus the assets, as one endpoint.
pub struct UiApp {
    ui: Arc<Ui>,
    pages: Vec<(String, PageFn)>,
}

impl std::fmt::Debug for UiApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UiApp")
            .field(
                "pages",
                &self.pages.iter().map(|(p, _)| p).collect::<Vec<_>>(),
            )
            .finish_non_exhaustive()
    }
}

impl UiApp {
    #[must_use]
    pub fn new(ui: Ui) -> Self {
        Self {
            ui: Arc::new(ui),
            pages: Vec::new(),
        }
    }

    /// Serves `render(request)` at `path`. The page is rendered on the server and the same code
    /// starts in the browser.
    #[must_use]
    pub fn page(
        mut self,
        path: &str,
        render: impl Fn(&Request) -> Page + Send + Sync + 'static,
    ) -> Self {
        self.pages.push((path.to_string(), Box::new(render)));
        self
    }

    #[must_use]
    pub fn ui(&self) -> &Ui {
        &self.ui
    }
}

#[async_trait]
impl Endpoint for UiApp {
    async fn call(&self, req: Request) -> Response {
        let path = req.uri().path();
        if path.starts_with(PREFIX) {
            return self
                .ui
                .assets()
                .serve(
                    path,
                    req.header("accept-encoding"),
                    req.header("if-none-match"),
                )
                .unwrap_or_else(Response::not_found);
        }
        // A page holds `Rc`s, so it must be built and rendered without an `.await` in between.
        self.pages
            .iter()
            .find(|(p, _)| p == path)
            .map_or_else(Response::not_found, |(_, render)| {
                self.ui.render(render(&req))
            })
    }
}
