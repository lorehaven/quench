//! AT-SPI accessibility for the native renderer on Linux, with the D-Bus protocol written here.
//!
//! Screen readers such as Orca talk to applications over a second D-Bus, the accessibility bus. [`Service::start`]
//! connects to it, registers the application with the registry and answers the questions a screen reader asks
//! (what is on the page, what is focused, where is it, what can be done to it) from the
//! [`AccessNode`](quench_ui_native::AccessNode) tree the renderer produces. Call [`Service::publish`] with a new
//! tree whenever the page changes, and carry out what [`Service::take_actions`] returns.

// Sizes and coordinates on the wire are 32-bit integers by definition.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_possible_wrap,
    clippy::cast_sign_loss,
    clippy::too_many_lines,
    clippy::many_single_char_names,
    clippy::format_collect,
    clippy::significant_drop_tightening,
    clippy::assigning_clones,
    clippy::option_option,
    clippy::unused_self,
    clippy::match_same_arms,
    clippy::option_if_let_else,
    clippy::use_self,
    clippy::needless_pass_by_value,
    clippy::cast_precision_loss,
    clippy::items_after_statements
)]
#![cfg(target_os = "linux")]

pub mod conn;
pub mod message;
pub mod service;
pub mod value;

pub use conn::{Connection, Sender};
pub use message::{Kind, Message};
pub use service::{ROOT_PATH, Service, Shared};
pub use value::Value;
