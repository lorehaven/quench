//! The development-mode live reload: a server-sent events stream the page keeps open, and the
//! script that reloads the page once the server is back after a restart.

use std::time::Duration;

use quench_core::stream;
use quench_http::http::sse::Event;
use quench_http::prelude::Response;

/// Where the script and the stream are served.
pub const SCRIPT_PATH: &str = "/_quench/dev.js";
pub const STREAM_PATH: &str = "/_quench/dev";

/// Reloads when the stream drops and the server answers again. The stream carries no messages of
/// its own: a restart is the only thing it reports.
const SCRIPT: &str = "(()=>{\
const wait=()=>fetch('/_quench/dev.js',{cache:'no-store'}).then(r=>{if(r.ok)location.reload();else setTimeout(wait,400)},()=>setTimeout(wait,400));\
const s=new EventSource('/_quench/dev');\
s.onerror=()=>{s.close();wait()};\
})();";

/// Puts the script tag at the end of a rendered document.
pub fn add_script(mut html: String) -> String {
    let tag = format!("<script src=\"{SCRIPT_PATH}\"></script>");
    match html.rfind("</body>") {
        Some(at) => html.insert_str(at, &tag),
        None => html.push_str(&tag),
    }
    html
}

/// The script, or the stream, when `path` asks for one.
pub fn serve(path: &str) -> Option<Response> {
    match path {
        SCRIPT_PATH => Some(
            Response::from_bytes(
                quench_http::prelude::http::StatusCode::OK,
                SCRIPT.as_bytes().to_vec().into(),
            )
            .header("content-type", "text/javascript; charset=utf-8")
            .header("cache-control", "no-store"),
        ),
        STREAM_PATH => {
            // A comment every few seconds keeps proxies from closing an idle stream.
            let beats = stream::unfold((), |()| async {
                tokio::time::sleep(Duration::from_secs(15)).await;
                Some((Event::new().comment("beat"), ()))
            });
            Some(Response::sse(Box::pin(beats)).header("cache-control", "no-store"))
        }
        _ => None,
    }
}
