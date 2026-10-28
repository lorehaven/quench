//! The Linux desktop host: a GTK window around webkit2gtk.
#![cfg(target_os = "linux")]

mod bridge;
mod bundle;
pub mod control;
mod ffi;
mod host;
pub mod plugins;

pub use bridge::{Bridge, Call, Deliver, Emitter, MAX_MESSAGE, Plugin, string_arg};
pub use bundle::{Bundle, HOST, SCHEME, content_type};
pub use ffi::{Answer, Gtk, Remote, WebView, Window};
pub use host::{App, RunError, is_app_origin, opens_outside, origin_of, run};
