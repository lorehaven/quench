//! Quench mail: build an email and hand it to an SMTP server.
//!
//! ```no_run
//! use quench_mail::{Mailer, Message, Security};
//!
//! # async fn demo() -> Result<(), Box<dyn std::error::Error>> {
//! let mailer = Mailer::builder("mail.example.com")
//!     .security(Security::Tls)
//!     .credentials("noreply@example.com", "app-password")
//!     .build()?;
//!
//! let message = Message::builder()
//!     .from("Forge <noreply@example.com>")
//!     .to("alice@example.org")
//!     .subject("Confirm your address")
//!     .text("Follow this link to confirm your address: https://example.com/verify?token=abc")
//!     .build()?;
//!
//! mailer.send(&message).await?;
//! # Ok(())
//! # }
//! ```
//!
//! Scope: composing a message (text and/or HTML, non-ASCII subjects and names,
//! extra headers) and submitting it to an authenticated SMTP server over TLS or
//! STARTTLS. Not in scope: attachments, receiving mail, connection pooling.

mod address;
mod error;
mod message;
pub mod mime;
mod smtp;

pub use address::{Address, Mailbox};
pub use error::{BuildError, Error, Reply};
pub use message::{Envelope, Message, MessageBuilder};
pub use smtp::{Mailer, MailerBuilder, Response, Security};

pub use smtp::dot_stuff;
