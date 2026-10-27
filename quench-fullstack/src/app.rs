//! A `quench-http` endpoint that serves server-rendered quench-ui pages and their assets.

use std::sync::Arc;

use quench_core::async_trait;
use quench_http::prelude::{Endpoint, FromRequest, Request, Response};

use crate::assets::{Assets, PREFIX};
use crate::document::{Page, document_response, new_nonce, render_document};
use crate::user::User;

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
type UserPageFn = Box<dyn Fn(&Request, Option<User>) -> Page + Send + Sync>;

enum PageKind {
    Plain(PageFn),
    ForUser(UserPageFn),
}

/// Pages at exact paths, the assets, and every `#[server]` function, as one endpoint.
pub struct UiApp {
    ui: Arc<Ui>,
    pages: Vec<(String, PageKind)>,
    functions: Vec<(&'static str, Arc<dyn Endpoint>)>,
}

impl std::fmt::Debug for UiApp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("UiApp")
            .field(
                "pages",
                &self.pages.iter().map(|(p, _)| p).collect::<Vec<_>>(),
            )
            .field(
                "functions",
                &self.functions.iter().map(|(p, _)| p).collect::<Vec<_>>(),
            )
            .finish_non_exhaustive()
    }
}

impl UiApp {
    /// Collects the `#[server]` functions linked into the binary.
    ///
    /// # Panics
    /// If two of them have the same path.
    #[must_use]
    pub fn new(ui: Ui) -> Self {
        Self {
            ui: Arc::new(ui),
            pages: Vec::new(),
            functions: crate::server::functions(),
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
        self.pages
            .push((path.to_string(), PageKind::Plain(Box::new(render))));
        self
    }

    /// As [`UiApp::page`], for a page that depends on who is asking: `render` is given the
    /// signed-in [`User`], or `None` when there is no valid session. The session is checked the
    /// way a `#[context] user: User` parameter is, from the request's cookie or bearer token, so
    /// the server can render the signed-in view (or the sign-in form) in the first response.
    #[must_use]
    pub fn page_for_user(
        mut self,
        path: &str,
        render: impl Fn(&Request, Option<User>) -> Page + Send + Sync + 'static,
    ) -> Self {
        self.pages
            .push((path.to_string(), PageKind::ForUser(Box::new(render))));
        self
    }

    #[must_use]
    pub fn ui(&self) -> &Ui {
        &self.ui
    }
}

#[async_trait]
impl Endpoint for UiApp {
    async fn call(&self, mut req: Request) -> Response {
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
        if let Some((_, endpoint)) = self.functions.iter().find(|(p, _)| *p == path) {
            return endpoint.call(req).await;
        }
        let Some((_, kind)) = self.pages.iter().find(|(p, _)| p == path) else {
            return Response::not_found();
        };
        // Who is asking is settled first: a page holds `Rc`s, so it is built and rendered with no
        // `.await` after it exists.
        let user = match kind {
            PageKind::ForUser(_) => User::from_request(&mut req).await.ok(),
            PageKind::Plain(_) => None,
        };
        let page = match kind {
            PageKind::Plain(render) => render(&req),
            PageKind::ForUser(render) => render(&req, user),
        };
        self.ui.render(page)
    }
}
