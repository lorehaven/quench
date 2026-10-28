//! The Linux desktop host: a GTK window around webkit2gtk.
//!
//! The parts that need no GTK (the bundle and its routing, the bridge, the plugins, the rules about origins and
//! links) are always built. The window itself (`ffi`, `host`, the control socket) needs the `window` feature and,
//! to link, `gtk-3` and `webkit2gtk-4.1`: a build without them (a CI image, `cargo package`) leaves it out. The
//! generated `desktop/` crate turns it on.
#![cfg(target_os = "linux")]

mod answer;
mod bridge;
mod bundle;
#[cfg(feature = "window")]
pub mod control;
#[cfg(feature = "window")]
mod ffi;
#[cfg(feature = "window")]
mod host;
mod origin;
pub mod plugins;

pub use answer::Answer;
pub use bridge::{Bridge, Call, Deliver, Emitter, MAX_MESSAGE, Plugin, string_arg};
pub use bundle::{Bundle, HOST, SCHEME, content_type};
#[cfg(feature = "window")]
pub use ffi::{Gtk, Remote, WebView, Window};
#[cfg(feature = "window")]
pub use host::{App, RunError, run};
pub use origin::{is_app_origin, opens_outside, origin_of};
