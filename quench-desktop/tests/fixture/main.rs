//! A window for the tests: serves one page from the `quench:` scheme, answers the page's messages and
//! prints what happened. Exits 77 when there is no display.
use quench_desktop::{Answer, Gtk, WebView, Window};
use std::rc::Rc;

const PAGE: &str = r"<!doctype html><title>fixture</title><script>
const host = window.webkit.messageHandlers.quench;
window.__reply = (text) => host.postMessage('got:' + text);
(async () => {
  host.postMessage('ping');
  const asset = await (await fetch('quench://app/data.txt')).text();
  host.postMessage('asset:' + asset + ':' + location.origin + ':' + isSecureContext + ':' + navigator.userAgent.includes('QuenchHost/1'));
})();
</script>";

/// A page that speaks the desktop bridge's contract by hand (it is not `quench.js`).
const APP_PAGE: &str = r"<!doctype html><title>app</title><body><script>
window.answers = [];
window.__quenchBridge = (text) => window.answers.push(JSON.parse(text));
window.call = (plugin, method, args) => webkit.messageHandlers.quench.postMessage(
  JSON.stringify({ id: window.answers.length + 1, plugin, method, args }));
</script>";

/// Ends the process the way a program ends (so its coverage is written), when a test is done with it.
struct Quit;

impl quench_desktop::Plugin for Quit {
    fn name(&self) -> &'static str {
        "quit"
    }

    fn call(&self, _method: &str, _args: &quench_serde::Value, call: quench_desktop::Call) {
        call.resolve(&quench_serde::Value::Null);
        std::process::exit(0);
    }
}

/// `FIXTURE=app`: the real host (`quench_desktop::run`) with the echo plugin and the control socket.
fn app() {
    let bundle = quench_desktop::Bundle::new().with("index.html", APP_PAGE.as_bytes());
    let app = quench_desktop::App::new("Fixture app", bundle)
        .with_echo()
        .plugin(Quit)
        .standard_plugins("dev.lorehaven.fixture");
    if let Err(error) = quench_desktop::run(app) {
        eprintln!("{error}");
        std::process::exit(if error == quench_desktop::RunError::NoDisplay {
            77
        } else {
            1
        });
    }
}

fn main() {
    if std::env::var("FIXTURE").as_deref() == Ok("app") {
        return app();
    }
    let Some(gtk) = Gtk::init() else {
        eprintln!("no display");
        std::process::exit(77);
    };
    let gtk = Rc::new(gtk);
    let window = Rc::new(Window::new(&gtk, "fixture", 480, 320));
    let view = Rc::new(WebView::new(&gtk, "QuenchHost", "1", false));
    view.serve("quench", |uri| match uri {
        "quench://app/" => Some(Answer::ok("text/html", PAGE.as_bytes().to_vec())),
        "quench://app/data.txt" => Some(Answer::ok("text/plain", b"from the scheme".to_vec())),
        _ => None,
    });
    {
        let (window, view) = (Rc::clone(&window), Rc::clone(&view));
        view.on_message({
            let view = Rc::clone(&view);
            move |text, uri| {
                println!("message {text:?} at {uri}");
                if text == "ping" {
                    view.eval("window.__reply('pong')");
                } else if text.starts_with("asset:") {
                    window.close();
                }
            }
        });
    }
    {
        let gtk = Rc::clone(&gtk);
        window.on_closed(move || gtk.quit());
    }
    window.add(&view);
    view.load("quench://app/");
    window.show();
    gtk.run();
    println!("closed");
}
