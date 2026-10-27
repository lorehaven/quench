//! The HTML document around a server-rendered view: head, root, embedded state, the script that
//! starts the app, and the Content-Security-Policy that goes with them.

use std::fmt::Write as _;

use quench_http::prelude::Response;
use quench_http::prelude::http::StatusCode;
use quench_ui::html::escape_attr;
use quench_ui::{IntoView, View, render_to_string};

/// The id of the element the app is mounted into, and that the server renders its view inside.
pub const ROOT_ID: &str = "app";

/// The id of the script element that carries the app's initial state.
pub const STATE_ID: &str = "__q_state";

/// What to send for one page.
pub struct Page {
    pub(crate) title: String,
    pub(crate) lang: String,
    pub(crate) stylesheets: Vec<String>,
    pub(crate) view: View,
    pub(crate) state: Option<String>,
    pub(crate) status: StatusCode,
}

impl std::fmt::Debug for Page {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Page")
            .field("title", &self.title)
            .field("status", &self.status)
            .finish_non_exhaustive()
    }
}

impl Page {
    /// A page that renders `view` on the server and starts the same app in the browser.
    #[must_use]
    pub fn new(title: &str, view: impl IntoView) -> Self {
        Self {
            title: title.to_string(),
            lang: "en".to_string(),
            stylesheets: Vec::new(),
            view: view.into_view(),
            state: None,
            status: StatusCode::OK,
        }
    }

    /// The document language (the `lang` attribute), `en` by default.
    #[must_use]
    pub fn lang(mut self, lang: &str) -> Self {
        self.lang = lang.to_string();
        self
    }

    /// Links a stylesheet from this origin. The policy allows no inline styles.
    #[must_use]
    pub fn stylesheet(mut self, href: &str) -> Self {
        self.stylesheets.push(href.to_string());
        self
    }

    /// JSON the app reads at startup with `quench_ui_web::initial_state()`, so it does not have
    /// to fetch what the server already knows. Pass valid JSON: it is escaped so it cannot end
    /// its `<script>` element, but not validated.
    #[must_use]
    pub fn state(mut self, json: impl Into<String>) -> Self {
        self.state = Some(json.into());
        self
    }

    #[must_use]
    pub const fn status(mut self, status: StatusCode) -> Self {
        self.status = status;
        self
    }
}

/// Escapes JSON for the inside of a `<script>` element.
///
/// `<`, `>` and `&` (so `</script>` and `<!--` cannot appear) and the two line separators
/// JavaScript treats as newlines each become a `\u` escape. Those only exist inside JSON strings,
/// so valid JSON stays valid and means the same.
#[must_use]
pub fn escape_json(json: &str) -> String {
    let mut out = String::with_capacity(json.len());
    for c in json.chars() {
        match c {
            '<' => out.push_str("\\u003c"),
            '>' => out.push_str("\\u003e"),
            '&' => out.push_str("\\u0026"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            _ => out.push(c),
        }
    }
    out
}

/// The Content-Security-Policy for a page whose scripts carry `nonce`.
#[must_use]
pub fn content_security_policy(nonce: &str) -> String {
    format!(
        "default-src 'self'; script-src 'self' 'nonce-{nonce}' 'wasm-unsafe-eval'; style-src 'self'; \
         connect-src 'self'; img-src 'self' data:; object-src 'none'; base-uri 'none'; \
         form-action 'self'; frame-ancestors 'none'"
    )
}

/// A fresh nonce: 128 random bits as hex.
#[must_use]
pub fn new_nonce() -> String {
    quench_core::hex::encode(quench_core::random::bytes::<16>())
}

/// Builds the HTML of a page. `script` and `wasm` are the hashed public paths.
#[must_use]
pub fn render_document(page: Page, nonce: &str, script: &str, wasm: &str) -> String {
    let Page {
        title,
        lang,
        stylesheets,
        view,
        state,
        ..
    } = page;
    let mut html = String::from("<!doctype html>");
    let _ = write!(html, "<html lang=\"{}\">", escape_attr(&lang));
    html.push_str("<head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">");
    let _ = write!(
        html,
        "<title>{}</title>",
        quench_ui::html::escape_text(&title)
    );
    for href in &stylesheets {
        let _ = write!(
            html,
            "<link rel=\"stylesheet\" href=\"{}\">",
            escape_attr(href)
        );
    }
    html.push_str("</head><body>");
    let _ = write!(
        html,
        "<div id=\"{ROOT_ID}\">{}</div>",
        render_to_string(view)
    );
    if let Some(json) = state {
        let _ = write!(
            html,
            "<script type=\"application/json\" id=\"{STATE_ID}\" nonce=\"{nonce}\">{}</script>",
            escape_json(&json)
        );
    }
    let _ = write!(
        html,
        "<script type=\"module\" nonce=\"{nonce}\" src=\"{}?wasm={}\"></script>",
        escape_attr(script),
        escape_attr(wasm)
    );
    html.push_str("</body></html>");
    html
}

/// The response for a rendered document: not cacheable (it carries a nonce), with the policy.
#[must_use]
pub fn document_response(page_status: StatusCode, html: String, nonce: &str) -> Response {
    Response::html(page_status, html)
        .header("cache-control", "no-store")
        .header("content-security-policy", content_security_policy(nonce))
        .header("x-content-type-options", "nosniff")
        .header("referrer-policy", "same-origin")
}
