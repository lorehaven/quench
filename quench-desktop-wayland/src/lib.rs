//! A Wayland client written against the wire protocol: a window, shared-memory frames to draw into
//! ([`quench_raster::Canvas`] draws straight into them), and pointer input.
//!
//! ```no_run
//! use quench_desktop_wayland::{Window, WindowEvent};
//! use quench_raster::Color;
//!
//! let mut window = Window::open("Hello", "dev.example.hello", 640, 400)?;
//! loop {
//!     let mut canvas = window.begin_frame()?;
//!     canvas.clear(Color::rgb(30, 32, 38));
//!     drop(canvas);
//!     window.present()?;
//!     match window.next_event(None)? {
//!         Some(WindowEvent::CloseRequested) => break,
//!         _ => {}
//!     }
//! }
//! # Ok::<(), std::io::Error>(())
//! ```
#![cfg(target_os = "linux")]
// The protocol's numbers are 32-bit by definition (sizes, ids, serials), and every size that is cast is bounded
// first: a message is at most 65535 bytes and a window side at most 16384 pixels.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss
)]

pub mod client;
pub mod keymap;
pub mod proto;
pub mod sys;
mod window;
pub mod wire;

pub use keymap::{Key, Keymap, Modifiers};
pub use window::{ImeEvent, Window, WindowEvent};
