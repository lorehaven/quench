//! The calls into `GTK` 3, `webkit2gtk` 4.1 and `GLib`, and nothing else: every `unsafe` of the crate is in this
//! file. The rest of the crate sees the safe types at the bottom ([`Gtk`], [`Window`], [`WebView`]).
//!
//! The contract the safe types keep:
//!
//! - **One thread.** GTK may only be used from the thread that initialised it. The types are `!Send` and
//!   `!Sync` (they hold a `PhantomData<*const ()>`), and [`Gtk::init`] hands out the only [`Gtk`] there
//!   can be, so a type of this file cannot reach another thread.
//! - **Ownership.** A widget pointer is owned by GTK. A [`WebView`] lives inside its [`Window`] (the window
//!   owns it from `gtk_container_add`), and both are kept alive by the application's reference until the
//!   window is destroyed, after which the safe types refuse to use the pointers (`alive` is false).
//! - **Callbacks.** A Rust closure given to GTK is boxed, its pointer is the callback's user data, and GTK
//!   frees it through a destroy notify when the signal handler or the scheme is gone. A closure never
//!   unwinds into C: it runs under `catch_unwind` and a panic is reported on standard error.
//! - **Strings.** Text for C is copied into a `CString` that lives for the call; text from C is copied into
//!   a `String` (and freed with `g_free` where the C side hands over ownership) before the callback returns.
#![allow(unsafe_code)] // the whole point of this file
#![allow(clippy::missing_safety_doc)] // the raw declarations are private; the safe wrappers state the contract

use std::cell::{Cell, RefCell};
use std::ffi::{CStr, CString, c_char, c_int, c_long, c_uint, c_ulong, c_void};
use std::marker::PhantomData;
use std::rc::Rc;

type Pointer = *mut c_void;
/// Receives the files a chooser picked (`None`: it was cancelled).
type Chosen = Box<dyn FnOnce(Option<Vec<String>>)>;
/// Receives the value of a script as JSON, or the error's message.
type Evaluated = Box<dyn FnOnce(Result<String, String>)>;

/// `GLib`'s error: a domain, a code, and a message.
#[repr(C)]
struct GError {
    domain: u32,
    code: c_int,
    message: *mut c_char,
}

/// `GLib`'s singly linked list node.
#[repr(C)]
struct GSList {
    data: Pointer,
    next: *mut Self,
}

/// Answers a request for an asset (`None` is a 404).
type Assets = Box<dyn Fn(&str) -> Option<Answer>>;
/// Receives a message from the page and the URI the page is at.
type OnMessage = Box<dyn FnMut(&str, &str)>;
/// Decides whether the view may go to a URI (and whether it would be a new window).
type Decide = Box<dyn FnMut(&str, bool) -> bool>;
type Destroy = unsafe extern "C" fn(Pointer);

#[link(name = "glib-2.0")]
#[link(name = "gobject-2.0")]
#[link(name = "gio-2.0")]
#[link(name = "gtk-3")]
#[link(name = "gdk-3")]
#[link(name = "webkit2gtk-4.1")]
#[link(name = "javascriptcoregtk-4.1")]
unsafe extern "C" {
    fn g_free(memory: Pointer);
    fn g_malloc(size: c_ulong) -> Pointer;
    fn g_object_unref(object: Pointer);
    fn g_signal_connect_data(
        instance: Pointer,
        signal: *const c_char,
        handler: Pointer,
        data: Pointer,
        destroy: Option<Destroy>,
        flags: c_int,
    ) -> c_ulong;
    fn g_memory_input_stream_new_from_data(
        data: *const c_void,
        len: c_long,
        destroy: Option<Destroy>,
    ) -> Pointer;
    fn g_idle_add_full(
        priority: c_int,
        function: unsafe extern "C" fn(Pointer) -> c_int,
        data: Pointer,
        destroy: Option<Destroy>,
    ) -> c_uint;

    fn gtk_init_check(argc: *mut c_int, argv: *mut Pointer) -> c_int;
    fn gtk_main();
    fn gtk_main_quit();
    fn gtk_window_new(kind: c_int) -> Pointer;
    fn gtk_window_set_title(window: Pointer, title: *const c_char);
    fn gtk_window_set_default_size(window: Pointer, width: c_int, height: c_int);
    fn gtk_container_add(container: Pointer, widget: Pointer);
    fn gtk_widget_show_all(widget: Pointer);
    fn gtk_widget_destroy(widget: Pointer);
    fn gtk_file_chooser_native_new(
        title: *const c_char,
        parent: Pointer,
        action: c_int,
        accept: *const c_char,
        cancel: *const c_char,
    ) -> Pointer;
    fn gtk_file_chooser_set_select_multiple(chooser: Pointer, multiple: c_int);
    fn gtk_file_chooser_get_uris(chooser: Pointer) -> *mut GSList;
    fn gtk_native_dialog_show(dialog: Pointer);
    fn g_slist_free(list: *mut GSList);

    fn webkit_user_content_manager_new() -> Pointer;
    fn webkit_user_content_manager_register_script_message_handler(
        manager: Pointer,
        name: *const c_char,
    ) -> c_int;
    fn webkit_web_view_new_with_user_content_manager(manager: Pointer) -> Pointer;
    fn webkit_web_view_get_context(view: Pointer) -> Pointer;
    fn webkit_web_view_get_user_content_manager(view: Pointer) -> Pointer;
    fn webkit_web_view_get_settings(view: Pointer) -> Pointer;
    fn webkit_web_view_load_uri(view: Pointer, uri: *const c_char);
    fn webkit_web_view_get_uri(view: Pointer) -> *const c_char;
    fn webkit_policy_decision_ignore(decision: Pointer);
    fn webkit_navigation_policy_decision_get_navigation_action(decision: Pointer) -> Pointer;
    fn webkit_navigation_action_get_request(action: Pointer) -> Pointer;
    fn webkit_uri_request_get_uri(request: Pointer) -> *const c_char;
    fn webkit_web_view_evaluate_javascript(
        view: Pointer,
        script: *const c_char,
        length: isize,
        world: *const c_char,
        source_uri: *const c_char,
        cancellable: Pointer,
        callback: Pointer,
        data: Pointer,
    );
    fn webkit_settings_set_user_agent_with_application_details(
        settings: Pointer,
        name: *const c_char,
        version: *const c_char,
    );
    fn webkit_settings_set_enable_developer_extras(settings: Pointer, enabled: c_int);
    fn webkit_web_context_register_uri_scheme(
        context: Pointer,
        scheme: *const c_char,
        callback: Pointer,
        data: Pointer,
        destroy: Option<Destroy>,
    );
    fn webkit_web_context_get_security_manager(context: Pointer) -> Pointer;
    fn webkit_security_manager_register_uri_scheme_as_secure(
        manager: Pointer,
        scheme: *const c_char,
    );
    fn webkit_security_manager_register_uri_scheme_as_cors_enabled(
        manager: Pointer,
        scheme: *const c_char,
    );
    fn webkit_uri_scheme_request_get_uri(request: Pointer) -> *const c_char;
    fn webkit_uri_scheme_response_new(stream: Pointer, length: i64) -> Pointer;
    fn webkit_uri_scheme_response_set_status(
        response: Pointer,
        status: c_uint,
        reason: *const c_char,
    );
    fn webkit_uri_scheme_response_set_content_type(response: Pointer, content_type: *const c_char);
    fn webkit_uri_scheme_request_finish_with_response(request: Pointer, response: Pointer);
    fn webkit_javascript_result_get_js_value(result: Pointer) -> Pointer;
    fn webkit_web_view_evaluate_javascript_finish(
        view: Pointer,
        result: Pointer,
        error: *mut *mut GError,
    ) -> Pointer;
    fn jsc_value_is_undefined(value: Pointer) -> c_int;
    fn jsc_value_to_json(value: Pointer, indent: c_uint) -> *mut c_char;
    fn g_error_free(error: *mut GError);
    fn jsc_value_to_string(value: Pointer) -> *mut c_char;
}

/// A C string as text; empty for null.
///
/// # Safety
/// `text` is null or points to a NUL-terminated string that stays valid for the call.
unsafe fn cstr_or_empty(text: *const c_char) -> String {
    if text.is_null() {
        String::new()
    } else {
        // SAFETY: by the contract above.
        unsafe { CStr::from_ptr(text) }
            .to_string_lossy()
            .into_owned()
    }
}

/// Makes a `CString`, dropping any interior NUL (which C could not carry anyway).
fn c_string(text: &str) -> CString {
    CString::new(text.replace('\0', "")).unwrap_or_default()
}

/// `catch_unwind` around a callback body: a panic must not unwind into C.
fn guarded(what: &str, body: impl FnOnce()) {
    if std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)).is_err() {
        eprintln!("quench-desktop: {what} panicked");
    }
}

/// A boxed value as callback data, and the destroy notify that drops it.
fn boxed<T>(value: T) -> (Pointer, Destroy) {
    unsafe extern "C" fn drop_box<T>(data: Pointer) {
        // SAFETY: `data` came from `Box::into_raw` of a `Box<T>` in `boxed`, and GTK calls the notify once.
        drop(unsafe { Box::from_raw(data.cast::<T>()) });
    }
    (Box::into_raw(Box::new(value)).cast(), drop_box::<T>)
}

/// GTK, initialised on this thread. There is at most one, and it cannot leave the thread.
#[derive(Debug)]
pub struct Gtk {
    _not_send: PhantomData<*const ()>,
}

impl Gtk {
    /// Initialises GTK; `None` when there is no display to open.
    #[must_use]
    pub fn init() -> Option<Self> {
        thread_local!(static DONE: Cell<bool> = const { Cell::new(false) });
        if DONE.with(Cell::get) {
            return None;
        }
        // SAFETY: null argc/argv are allowed; GTK then reads nothing from the command line.
        let ok = unsafe { gtk_init_check(std::ptr::null_mut(), std::ptr::null_mut()) } != 0;
        if ok {
            DONE.with(|done| done.set(true));
        }
        ok.then_some(Self {
            _not_send: PhantomData,
        })
    }

    /// Runs the main loop until [`quit`](Self::quit) is called.
    pub fn run(&self) {
        // SAFETY: GTK is initialised on this thread (`self` proves it).
        unsafe { gtk_main() };
    }

    /// Ends the main loop.
    pub fn quit(&self) {
        // SAFETY: as for `run`.
        unsafe { gtk_main_quit() };
    }
}

/// What the window's "destroy" handler needs, and what the window shares with it.
struct WindowState {
    alive: Cell<bool>,
    /// The `alive` flags of the widgets inside, cleared when the window goes (GTK destroys them with it).
    inside: RefCell<Vec<Rc<Cell<bool>>>>,
    then: RefCell<Option<Box<dyn FnOnce()>>>,
}

/// A way to run a job on the main loop from any thread (plugins finish on threads of their own).
#[derive(Debug, Clone, Copy, Default)]
pub struct Remote;

impl Remote {
    /// Queues `job` for the main loop. It runs there, on the thread that called [`Gtk::run`]; if the loop
    /// has ended it never runs.
    pub fn post(self, job: impl FnOnce() + Send + 'static) {
        unsafe extern "C" fn run_job(data: Pointer) -> c_int {
            // SAFETY: `data` is the `Box<Option<Box<dyn FnOnce() + Send>>>` made in `post`, alive until its notify.
            let job = unsafe { &mut *data.cast::<Option<Box<dyn FnOnce() + Send>>>() };
            if let Some(job) = job.take() {
                guarded("a posted job", job);
            }
            0 // G_SOURCE_REMOVE
        }
        let job: Option<Box<dyn FnOnce() + Send>> = Some(Box::new(job));
        let (data, destroy) = boxed(job);
        // SAFETY: `g_idle_add_full` may be called from any thread; `run_job` and the notify match `data`.
        unsafe { g_idle_add_full(200, run_job, data, Some(destroy)) };
    }
}

/// A top-level window.
#[derive(Debug)]
pub struct Window {
    raw: Pointer,
    state: Rc<WindowState>,
    _not_send: PhantomData<*const ()>,
}

impl std::fmt::Debug for WindowState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WindowState")
            .field("alive", &self.alive.get())
            .finish_non_exhaustive()
    }
}

impl Window {
    #[must_use]
    pub fn new(_gtk: &Gtk, title: &str, width: i32, height: i32) -> Self {
        unsafe extern "C" fn closed(_widget: Pointer, data: Pointer) {
            // SAFETY: `data` is the `Box<Rc<WindowState>>` made below, alive until its notify.
            let state = unsafe { &*data.cast::<Rc<WindowState>>() };
            state.alive.set(false);
            for flag in state.inside.borrow().iter() {
                flag.set(false);
            }
            let then = state.then.borrow_mut().take();
            if let Some(then) = then {
                guarded("the close handler", then);
            }
        }
        let state = Rc::new(WindowState {
            alive: Cell::new(true),
            inside: RefCell::new(Vec::new()),
            then: RefCell::new(None),
        });
        let (data, destroy) = boxed(Rc::clone(&state));
        // SAFETY: GTK is initialised (`_gtk`); the title is a valid C string for the call; the handler's
        // signature is that of "destroy" and the notify matches `data`.
        let raw = unsafe {
            let window = gtk_window_new(0);
            gtk_window_set_title(window, c_string(title).as_ptr());
            gtk_window_set_default_size(window, width, height);
            g_signal_connect_data(
                window,
                c"destroy".as_ptr(),
                closed as unsafe extern "C" fn(Pointer, Pointer) as Pointer,
                data,
                Some(destroy),
                0,
            );
            window
        };
        Self {
            raw,
            state,
            _not_send: PhantomData,
        }
    }

    /// Puts the web view in the window.
    pub fn add(&self, view: &WebView) {
        if self.state.alive.get() && view.alive.get() {
            self.state.inside.borrow_mut().push(Rc::clone(&view.alive));
            // SAFETY: both widgets are alive.
            unsafe { gtk_container_add(self.raw, view.raw) };
        }
    }

    pub fn show(&self) {
        if self.state.alive.get() {
            // SAFETY: the window is alive.
            unsafe { gtk_widget_show_all(self.raw) };
        }
    }

    /// Calls `then` once, when the window is closed (by the user or by [`close`](Self::close)).
    pub fn on_closed(&self, then: impl FnOnce() + 'static) {
        *self.state.then.borrow_mut() = Some(Box::new(then));
    }

    /// Closes the window; the `on_closed` handler runs.
    pub fn close(&self) {
        if self.state.alive.get() {
            // SAFETY: the window is alive; its "destroy" handler marks it dead.
            unsafe { gtk_widget_destroy(self.raw) };
        }
    }
}

impl Window {
    /// Shows the system's file chooser over this window: for a folder, or for one file or several. `then` is
    /// called with the chosen `file://` URIs, or `None` if the user cancelled.
    pub fn choose(
        &self,
        folder: bool,
        multiple: bool,
        then: impl FnOnce(Option<Vec<String>>) + 'static,
    ) {
        unsafe extern "C" fn response(dialog: Pointer, id: c_int, data: Pointer) {
            // SAFETY: `data` is the `Box<Option<Box<dyn FnOnce(..)>>>` made below; the signal fires once.
            let then = unsafe { &mut *data.cast::<Option<Chosen>>() };
            let picked = if id == -3 {
                // SAFETY: GTK_RESPONSE_ACCEPT: the chooser is live, and the list it returns is ours; each
                // node's `data` is a C string we copy, then the list (and its strings) are freed.
                let mut uris = Vec::new();
                unsafe {
                    let list = gtk_file_chooser_get_uris(dialog);
                    let mut node = list;
                    while !node.is_null() {
                        uris.push(cstr_or_empty((*node).data.cast::<c_char>()));
                        g_free((*node).data);
                        node = (*node).next;
                    }
                    g_slist_free(list);
                }
                Some(uris)
            } else {
                None
            };
            if let Some(then) = then.take() {
                guarded("the file chooser", move || then(picked));
            }
            // SAFETY: the dialog is ours (we hold the reference `gtk_file_chooser_native_new` returned).
            unsafe { g_object_unref(dialog) };
        }
        if !self.state.alive.get() {
            then(None);
            return;
        }
        let then: Option<Chosen> = Some(Box::new(then));
        let (data, destroy) = boxed(then);
        let title = c_string(if folder {
            "Choose a folder"
        } else {
            "Choose a file"
        });
        // SAFETY: the window is alive; the strings are valid for the call; the handler's signature is that of
        // "response" and the notify matches `data`. The dialog stays alive until `response` unrefs it.
        unsafe {
            let dialog = gtk_file_chooser_native_new(
                title.as_ptr(),
                self.raw,
                if folder { 2 } else { 0 },
                c"Choose".as_ptr(),
                c"Cancel".as_ptr(),
            );
            gtk_file_chooser_set_select_multiple(dialog, c_int::from(multiple && !folder));
            g_signal_connect_data(
                dialog,
                c"response".as_ptr(),
                response as unsafe extern "C" fn(Pointer, c_int, Pointer) as Pointer,
                data,
                Some(destroy),
                0,
            );
            gtk_native_dialog_show(dialog);
        }
    }
}

/// What an asset request is answered with.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Answer {
    pub status: u16,
    pub content_type: String,
    pub body: Vec<u8>,
}

impl Answer {
    /// A `200` with this body.
    #[must_use]
    pub fn ok(content_type: &str, body: Vec<u8>) -> Self {
        Self {
            status: 200,
            content_type: content_type.to_string(),
            body,
        }
    }

    /// A `404` with a short text.
    #[must_use]
    pub fn not_found() -> Self {
        Self {
            status: 404,
            content_type: "text/plain; charset=utf-8".to_string(),
            body: b"not found".to_vec(),
        }
    }
}

/// The reason phrase of the statuses an asset server uses.
const fn reason(status: u16) -> &'static str {
    match status {
        200 => "OK",
        400 => "Bad Request",
        404 => "Not Found",
        405 => "Method Not Allowed",
        _ => "Status",
    }
}

/// A web view with a script message handler named `quench` and a URI scheme that serves assets.
#[derive(Debug)]
pub struct WebView {
    raw: Pointer,
    alive: Rc<Cell<bool>>,
    _not_send: PhantomData<*const ()>,
}

impl WebView {
    /// A web view whose user agent ends in `application/version`, with developer tools when `debug`.
    #[must_use]
    pub fn new(_gtk: &Gtk, application: &str, version: &str, debug: bool) -> Self {
        // SAFETY: GTK is initialised; every string is a valid C string for its call.
        let raw = unsafe {
            let manager = webkit_user_content_manager_new();
            let view = webkit_web_view_new_with_user_content_manager(manager);
            let settings = webkit_web_view_get_settings(view);
            webkit_settings_set_user_agent_with_application_details(
                settings,
                c_string(application).as_ptr(),
                c_string(version).as_ptr(),
            );
            webkit_settings_set_enable_developer_extras(settings, c_int::from(debug));
            view
        };
        Self {
            raw,
            alive: Rc::new(Cell::new(true)),
            _not_send: PhantomData,
        }
    }

    /// Serves `scheme:` URIs from `assets` (called with the request's URI; `None` is a 404). The scheme
    /// is registered secure and CORS-enabled, so the page is a secure context that can `fetch` from itself.
    pub fn serve(&self, scheme: &str, assets: impl Fn(&str) -> Option<Answer> + 'static) {
        unsafe extern "C" fn request(request: Pointer, data: Pointer) {
            // SAFETY: `data` is the `Box<Box<dyn Fn>>` made in `serve`, alive until its notify.
            let assets = unsafe { &*data.cast::<Assets>() };
            guarded("an asset request", || {
                // SAFETY: GTK passes a live request; the URI is a C string owned by it.
                let uri = unsafe { CStr::from_ptr(webkit_uri_scheme_request_get_uri(request)) }
                    .to_string_lossy()
                    .into_owned();
                let answer = assets(&uri).unwrap_or_else(Answer::not_found);
                // SAFETY: `memory` is a fresh `g_malloc` block of the body's length, filled before the
                // stream takes it; the stream frees it with `g_free`. `finish` takes its own reference.
                unsafe {
                    let len = answer.body.len();
                    let size = i64::try_from(len).unwrap_or(i64::MAX);
                    let memory = g_malloc(len.max(1) as c_ulong).cast::<u8>();
                    std::ptr::copy_nonoverlapping(answer.body.as_ptr(), memory, len);
                    let stream = g_memory_input_stream_new_from_data(
                        memory.cast(),
                        size as c_long,
                        Some(g_free),
                    );
                    let response = webkit_uri_scheme_response_new(stream, size);
                    webkit_uri_scheme_response_set_status(
                        response,
                        c_uint::from(answer.status),
                        c_string(reason(answer.status)).as_ptr(),
                    );
                    webkit_uri_scheme_response_set_content_type(
                        response,
                        c_string(&answer.content_type).as_ptr(),
                    );
                    webkit_uri_scheme_request_finish_with_response(request, response);
                    g_object_unref(response);
                    g_object_unref(stream);
                }
            });
        }
        let assets: Assets = Box::new(assets);
        let (data, destroy) = boxed(assets);
        let scheme = c_string(scheme);
        // SAFETY: the view is alive; `request` and the notify match `data`; the scheme is valid for the calls.
        unsafe {
            let context = webkit_web_view_get_context(self.raw);
            webkit_web_context_register_uri_scheme(
                context,
                scheme.as_ptr(),
                request as unsafe extern "C" fn(Pointer, Pointer) as Pointer,
                data,
                Some(destroy),
            );
            let security = webkit_web_context_get_security_manager(context);
            webkit_security_manager_register_uri_scheme_as_secure(security, scheme.as_ptr());
            webkit_security_manager_register_uri_scheme_as_cors_enabled(security, scheme.as_ptr());
        }
    }

    /// Calls `handler` with each message the page posts to `window.webkit.messageHandlers.quench`
    /// (a string), and the URI the page is at when it posts.
    pub fn on_message(&self, handler: impl FnMut(&str, &str) + 'static) {
        unsafe extern "C" fn message(manager: Pointer, value: Pointer, data: Pointer) {
            let _ = manager;
            // SAFETY: `data` is the `Box<MessageData>` made below, alive until its notify.
            let state = unsafe { &mut *data.cast::<MessageData>() };
            guarded("a page message", || {
                // SAFETY: `value` is the signal's live WebKitJavascriptResult; the string it returns is ours to free.
                let text = unsafe {
                    let raw = jsc_value_to_string(webkit_javascript_result_get_js_value(value));
                    if raw.is_null() {
                        return;
                    }
                    let text = CStr::from_ptr(raw).to_string_lossy().into_owned();
                    g_free(raw.cast());
                    text
                };
                // SAFETY: the view is alive while it can deliver a message.
                let uri = unsafe {
                    let uri = webkit_web_view_get_uri(state.view);
                    if uri.is_null() {
                        String::new()
                    } else {
                        CStr::from_ptr(uri).to_string_lossy().into_owned()
                    }
                };
                (state.handler)(&text, &uri);
            });
        }
        struct MessageData {
            view: Pointer,
            handler: OnMessage,
        }
        let (data, destroy) = boxed(MessageData {
            view: self.raw,
            handler: Box::new(handler),
        });
        // SAFETY: the manager belongs to the view; connect before registering, as webkit advises; the
        // handler's signature is that of "script-message-received".
        unsafe {
            let manager = webkit_web_view_get_user_content_manager(self.raw);
            g_signal_connect_data(
                manager,
                c"script-message-received::quench".as_ptr(),
                message as unsafe extern "C" fn(Pointer, Pointer, Pointer) as Pointer,
                data,
                Some(destroy),
                0,
            );
            webkit_user_content_manager_register_script_message_handler(
                manager,
                c"quench".as_ptr(),
            );
        }
    }

    /// The URI the page is at.
    #[must_use]
    pub fn uri(&self) -> String {
        if !self.alive.get() {
            return String::new();
        }
        // SAFETY: the view is alive; the string is owned by it and copied before the call returns.
        unsafe { cstr_or_empty(webkit_web_view_get_uri(self.raw)) }
    }

    /// Asks `decide` about every navigation (a link, a redirect, `window.open`): it gets the target URI and
    /// whether it would open in a new window, and answers whether to let the view go there. A navigation it
    /// refuses does not happen, and a new window is never opened (there is only the one view): a page's
    /// `window.open` and `target=_blank` reach `decide` with `true`, and the answer does not matter.
    pub fn on_navigate(&self, decide: impl FnMut(&str, bool) -> bool + 'static) {
        /// What both signals share.
        type Shared = Rc<RefCell<Decide>>;
        unsafe extern "C" fn policy(
            _view: Pointer,
            decision: Pointer,
            kind: c_int,
            data: Pointer,
        ) -> c_int {
            // Only navigations and new windows are ours; a response is left to the default.
            if kind != 0 && kind != 1 {
                return 0;
            }
            // SAFETY: `data` is the `Box<Shared>` made below, alive until its notify.
            let decide = unsafe { &*data.cast::<Shared>() };
            let mut handled = 0;
            guarded("a navigation decision", || {
                // SAFETY: for these two kinds `decision` is a navigation policy decision, and the action,
                // request and URI it leads to are owned by it and copied here.
                let uri = unsafe {
                    let action = webkit_navigation_policy_decision_get_navigation_action(decision);
                    let request = webkit_navigation_action_get_request(action);
                    cstr_or_empty(webkit_uri_request_get_uri(request))
                };
                if !(decide.borrow_mut())(&uri, kind == 1) {
                    // SAFETY: the decision is live for the signal.
                    unsafe { webkit_policy_decision_ignore(decision) };
                    handled = 1;
                }
            });
            handled
        }
        unsafe extern "C" fn create(_view: Pointer, action: Pointer, data: Pointer) -> Pointer {
            // SAFETY: `data` is the `Box<Shared>` made below, alive until its notify.
            let decide = unsafe { &*data.cast::<Shared>() };
            guarded("a new window request", || {
                // SAFETY: the action is live for the signal; its request and URI are copied here.
                let uri = unsafe {
                    cstr_or_empty(webkit_uri_request_get_uri(
                        webkit_navigation_action_get_request(action),
                    ))
                };
                // Whatever it answers, no new view is made: the caller hands a web link to the system.
                let _ = (decide.borrow_mut())(&uri, true);
            });
            std::ptr::null_mut() // no new view
        }
        let shared: Shared = Rc::new(RefCell::new(Box::new(decide)));
        let (policy_data, policy_destroy) = boxed(Rc::clone(&shared));
        let (create_data, create_destroy) = boxed(shared);
        // SAFETY: the view is alive; the handlers' signatures are those of "decide-policy" and "create".
        unsafe {
            g_signal_connect_data(
                self.raw,
                c"decide-policy".as_ptr(),
                policy as unsafe extern "C" fn(Pointer, Pointer, c_int, Pointer) -> c_int
                    as Pointer,
                policy_data,
                Some(policy_destroy),
                0,
            );
            g_signal_connect_data(
                self.raw,
                c"create".as_ptr(),
                create as unsafe extern "C" fn(Pointer, Pointer, Pointer) -> Pointer as Pointer,
                create_data,
                Some(create_destroy),
                0,
            );
        }
    }

    pub fn load(&self, uri: &str) {
        if self.alive.get() {
            // SAFETY: the view is alive.
            unsafe { webkit_web_view_load_uri(self.raw, c_string(uri).as_ptr()) };
        }
    }

    /// Runs `script` in the page and calls `then` with its value as JSON (`undefined` is `null`), or with the
    /// error's message. A promise is not awaited: its value is `{}`.
    pub fn eval_json(&self, script: &str, then: impl FnOnce(Result<String, String>) + 'static) {
        unsafe extern "C" fn finished(view: Pointer, result: Pointer, data: Pointer) {
            // SAFETY: `data` is the `Box<Option<Box<dyn FnOnce(..)>>>` leaked below; GIO calls this exactly once,
            // so the box is taken back (and freed) here.
            let mut then = unsafe { Box::from_raw(data.cast::<Option<Evaluated>>()) };
            let answer = {
                let mut error: *mut GError = std::ptr::null_mut();
                // SAFETY: `view` and `result` are what GIO hands the callback; on failure `error` is set
                // and ours to free; a value returned is ours to unref, and its JSON to free.
                unsafe {
                    let value =
                        webkit_web_view_evaluate_javascript_finish(view, result, &raw mut error);
                    if value.is_null() {
                        let message = if error.is_null() {
                            "the script failed".to_string()
                        } else {
                            let message = cstr_or_empty((*error).message);
                            g_error_free(error);
                            message
                        };
                        Err(message)
                    } else if jsc_value_is_undefined(value) != 0 {
                        g_object_unref(value);
                        Ok("null".to_string())
                    } else {
                        let json = jsc_value_to_json(value, 0);
                        let text = cstr_or_empty(json);
                        g_free(json.cast());
                        g_object_unref(value);
                        Ok(text)
                    }
                }
            };
            if let Some(then) = then.take() {
                guarded("a script result", move || then(answer));
            }
        }
        if !self.alive.get() {
            then(Err("the window is closed".to_string()));
            return;
        }
        let then: Option<Evaluated> = Some(Box::new(then));
        let data: Pointer = Box::into_raw(Box::new(then)).cast();
        let script = c_string(script);
        // SAFETY: the view is alive; the script is a valid C string for the call (WebKit copies it); the
        // callback matches GAsyncReadyCallback, and it takes `data` back when the call finishes.
        unsafe {
            webkit_web_view_evaluate_javascript(
                self.raw,
                script.as_ptr(),
                -1,
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null_mut(),
                finished as unsafe extern "C" fn(Pointer, Pointer, Pointer) as Pointer,
                data,
            );
        }
    }

    /// Runs `script` in the page. The result is not read.
    pub fn eval(&self, script: &str) {
        if self.alive.get() {
            let script = c_string(script);
            // SAFETY: the view is alive; the script is a valid C string, and null for the rest is allowed.
            unsafe {
                webkit_web_view_evaluate_javascript(
                    self.raw,
                    script.as_ptr(),
                    -1,
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                );
            }
        }
    }
}
