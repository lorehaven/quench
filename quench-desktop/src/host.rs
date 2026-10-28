//! The window: the app's bundle on its own origin, and the bridge to its plugins.

use crate::bridge::{Bridge, Deliver, Plugin};
use crate::bundle::{Bundle, HOST, SCHEME};
use crate::ffi::{Gtk, Remote, WebView, Window};
use crate::origin::{is_app_origin, opens_outside, origin_of};
use crate::plugins::{
    Chooser, DeviceState, Echo, Files, Notifications, Runner, SecureStorage, Share, System,
};
use quench_serde::Value;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

/// What the host needs to show an app.
pub struct App {
    pub title: String,
    pub width: i32,
    pub height: i32,
    pub bundle: Bundle,
    pub plugins: Vec<Arc<dyn Plugin>>,
    /// Show this dev server instead of the bundle (and allow it as the page's origin). Debug builds only.
    pub dev_url: Option<String>,
    /// Web inspector on (right-click, "Inspect Element").
    pub debug: bool,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("App")
            .field("title", &self.title)
            .field("bundle", &self.bundle.len())
            .field("dev_url", &self.dev_url)
            .finish_non_exhaustive()
    }
}

impl App {
    /// An app that shows `bundle` in a 1000x700 window.
    #[must_use]
    pub fn new(title: &str, bundle: Bundle) -> Self {
        Self {
            title: title.to_string(),
            width: 1000,
            height: 700,
            bundle,
            plugins: Vec::new(),
            dev_url: None,
            debug: false,
        }
    }

    #[must_use]
    pub fn plugin(mut self, plugin: impl Plugin + 'static) -> Self {
        self.plugins.push(Arc::new(plugin));
        self
    }

    /// Adds the plugins every app can use: `device`, `secure_storage`, `notifications`, `share` and `files`.
    /// `id` names the app to the keyring and the notification service (`dev.lorehaven.notes`).
    #[must_use]
    pub fn standard_plugins(self, id: &str) -> Self {
        let run: Arc<dyn Runner> = Arc::new(System);
        let name = self.title.clone();
        self.plugin(DeviceState::default())
            .plugin(SecureStorage::new(id, Arc::clone(&run)))
            .plugin(Notifications::new(&name, Arc::clone(&run)))
            .plugin(Share::new(run))
            .plugin(Files::new(Arc::new(WindowChooser)))
    }

    /// Adds the `echo` plugin, which the gallery's *Native* tab calls. It is for trying the bridge.
    #[must_use]
    pub fn with_echo(self) -> Self {
        self.plugin(Echo)
    }

    #[must_use]
    pub fn dev(mut self, url: &str) -> Self {
        self.dev_url = Some(url.to_string());
        self.debug = true;
        self
    }
}

/// Why the window could not be shown.
#[derive(Debug, PartialEq, Eq)]
pub enum RunError {
    /// GTK found no display to open.
    NoDisplay,
}

impl std::fmt::Display for RunError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoDisplay => write!(f, "there is no display to open a window on"),
        }
    }
}

impl std::error::Error for RunError {}

thread_local! {
    /// The view, for jobs posted from other threads (they run on the main thread).
    static VIEW: RefCell<Option<Rc<WebView>>> = const { RefCell::new(None) };
    /// The window, for dialogs.
    static WINDOW: RefCell<Option<Rc<Window>>> = const { RefCell::new(None) };
}

/// The system's file chooser, shown over the app's window.
#[derive(Debug, Clone, Copy)]
struct WindowChooser;

impl Chooser for WindowChooser {
    fn choose(
        &self,
        folder: bool,
        multiple: bool,
        done: Box<dyn FnOnce(Option<Vec<PathBuf>>) + Send>,
    ) {
        Remote.post(move || {
            let window = WINDOW.with(|w| w.borrow().clone());
            match window {
                Some(window) => window.choose(folder, multiple, move |uris| {
                    done(uris.map(|uris| {
                        uris.iter()
                            .filter_map(|u| crate::plugins::path_of(u))
                            .collect()
                    }));
                }),
                None => done(None),
            }
        });
    }
}

/// Shows `app` and runs until its window is closed.
///
/// # Errors
/// [`RunError::NoDisplay`] without a display.
pub fn run(app: App) -> Result<(), RunError> {
    let gtk = Rc::new(Gtk::init().ok_or(RunError::NoDisplay)?);
    let window = Rc::new(Window::new(&gtk, &app.title, app.width, app.height));
    let view = Rc::new(WebView::new(&gtk, "QuenchHost", "1", app.debug));
    let dev = app.dev_url.clone().filter(|_| app.debug);

    let bundle = app.bundle;
    view.serve(SCHEME, move |uri| Some(bundle.answer(uri)));

    VIEW.with(|v| *v.borrow_mut() = Some(Rc::clone(&view)));
    WINDOW.with(|w| *w.borrow_mut() = Some(Rc::clone(&window)));
    let deliver: Deliver = {
        let dev = dev.clone();
        Arc::new(move |text: String| {
            let dev = dev.clone();
            Remote.post(move || {
                VIEW.with(|v| {
                    // A page that is going away, or is not the app's, is not answered.
                    if let Some(view) = v.borrow().as_ref()
                        && is_app_origin(&view.uri(), dev.as_deref())
                    {
                        let quoted = Value::String(text).to_string();
                        view.eval(&format!("window.__quenchBridge({quoted})"));
                    }
                });
            });
        })
    };
    let bridge = Arc::new(Bridge::new(app.plugins, deliver));
    {
        let (bridge, dev) = (Arc::clone(&bridge), dev.clone());
        view.on_message(move |text, uri| {
            if is_app_origin(uri, dev.as_deref()) {
                bridge.handle(text);
            } else {
                eprintln!("quench-desktop: refused origin {}", origin_of(uri));
            }
        });
    }
    {
        let dev = dev.clone();
        view.on_navigate(move |uri, new_window| {
            if is_app_origin(uri, dev.as_deref()) {
                return !new_window;
            }
            if opens_outside(uri) {
                open_outside(uri);
            }
            false
        });
    }
    {
        let gtk = Rc::clone(&gtk);
        window.on_closed(move || gtk.quit());
    }
    let control = std::env::var_os("QUENCH_DESKTOP_CONTROL").filter(|_| cfg!(debug_assertions));
    if let Some(path) = &control {
        let ask: crate::control::Ask = Arc::new(|script: String| {
            let (tx, rx) = std::sync::mpsc::channel();
            Remote.post(move || {
                let view = VIEW.with(|v| v.borrow().clone());
                match view {
                    Some(view) => view.eval_json(&script, move |result| {
                        let _ = tx.send(result);
                    }),
                    None => {
                        let _ = tx.send(Err("the window is closed".to_string()));
                    }
                }
            });
            rx.recv_timeout(std::time::Duration::from_secs(20))
                .unwrap_or_else(|_| Err("the script did not finish in 20 s".to_string()))
        });
        if let Err(e) = crate::control::listen(std::path::Path::new(path), ask) {
            eprintln!("quench-desktop: no control socket: {e}");
        }
    }
    window.add(&view);
    view.load(&dev.unwrap_or_else(|| format!("{SCHEME}://{HOST}/")));
    window.show();
    gtk.run();
    VIEW.with(|v| *v.borrow_mut() = None);
    WINDOW.with(|w| *w.borrow_mut() = None);
    // The control socket is a file: do not leave it behind.
    if let Some(path) = control {
        let _ = std::fs::remove_file(path);
    }
    Ok(())
}

/// Hands a link to the system (`xdg-open`); the app stays where it is.
fn open_outside(uri: &str) {
    let _ = std::process::Command::new("xdg-open")
        .arg(uri)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}
