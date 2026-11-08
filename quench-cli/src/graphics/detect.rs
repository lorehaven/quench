//! Which graphics the terminal shows, and the cell size and colours to draw for it.
//!
//! The queries are sent all at once: the kitty graphics query, `CSI 16 t` (cell size in pixels),
//! `CSI 14 t` (text area in pixels), `OSC 10 ?` and `OSC 11 ?` (foreground and background), and last the
//! primary device attributes `CSI c`. Every terminal answers that one, and answers in order, so its reply
//! marks the end: [`probe`] reads until it arrives or the timeout passes, then parses what came. Terminals
//! that do not know a query ignore it.
//!
//! [`probe`] works on any reader and writer with a clock passed in, so it is tested with canned replies;
//! [`detect`] (feature `tui`, Unix) runs it on the controlling terminal in raw mode, after [`plan`] has
//! decided from the environment whether to ask at all:
//!
//! * nothing is sent when standard input or output is not a terminal, or `TERM` is `dumb` or unset;
//! * inside tmux or screen nothing is ever sent (passing the sequences through the multiplexer is not
//!   supported, and the multiplexer would answer for itself): no graphics are reported, unless forced;
//! * `QUENCH_GRAPHICS=none|kitty|sixel` forces the answer, for terminals that report wrongly and for tests.
//!   `none` sends nothing; `kitty` and `sixel` still ask for the sizes and colours, but the protocol is the
//!   forced one whatever the terminal says. Inside a multiplexer a forced protocol is reported without
//!   asking, so with no cell size, text area or colours: the caller picks its own.
//!
//! A reply that comes after the timeout is not read here and ends up in the program's input.

use super::Rgb;
use super::kitty;
use super::reply::{Replies, parse_replies};
use std::io::{self, Read, Write};
use std::time::Duration;

/// How long [`detect`] waits for the terminal by default.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_millis(200);

/// Everything [`probe`] sends, the device attributes query (the sentinel) last.
pub const QUERIES: &str = concat!(
    "\x1b_Gi=31,s=1,v=1,a=q,t=d,f=24;AAAA\x1b\\",
    "\x1b[16t",
    "\x1b[14t",
    "\x1b]10;?\x1b\\",
    "\x1b]11;?\x1b\\",
    "\x1b[c",
);

/// More than the replies ever take: past this, reading stops (a terminal echoing garbage).
const MAX_REPLY: usize = 64 * 1024;

/// A read that comes back empty in less than this came back at once (an input at its end, or one that
/// does not block), not after the terminal's read timeout of a tenth of a second.
const IMMEDIATE: Duration = Duration::from_millis(50);

/// How long [`probe`] waits after a read that came back empty at once, so it does not spin.
const BACKOFF: Duration = Duration::from_millis(10);

/// A graphics protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    Kitty,
    Sixel,
}

/// What the terminal can show, and what was measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capabilities {
    /// The kitty graphics protocol.
    pub kitty: bool,
    /// Sixel graphics.
    pub sixel: bool,
    /// One cell, `(width, height)` in pixels.
    pub cell_size_px: Option<(u32, u32)>,
    /// The text area, `(width, height)` in pixels.
    pub text_area_px: Option<(u32, u32)>,
    /// The default text colour.
    pub foreground: Option<Rgb>,
    /// The default background colour.
    pub background: Option<Rgb>,
}

impl Capabilities {
    /// No graphics, nothing measured.
    pub const NONE: Self = Self {
        kitty: false,
        sixel: false,
        cell_size_px: None,
        text_area_px: None,
        foreground: None,
        background: None,
    };

    /// From the terminal's answers. `size` (`(columns, rows)`, from [`crate::term::size`]) gives the
    /// cell size as text area over the grid when the terminal did not report the cell size itself.
    #[must_use]
    pub fn from_replies(replies: &Replies, size: Option<(u16, u16)>) -> Self {
        let derived = || {
            let (width, height) = replies.text_area_px?;
            let (columns, rows) = size?;
            let cell = (width / u32::from(columns), height / u32::from(rows));
            (cell.0 > 0 && cell.1 > 0).then_some(cell)
        };
        Self {
            kitty: replies.kitty,
            sixel: replies.sixel(),
            cell_size_px: replies.cell_size_px.or_else(|| {
                size.filter(|&(c, r)| c > 0 && r > 0)
                    .and_then(|_| derived())
            }),
            text_area_px: replies.text_area_px,
            foreground: replies.foreground,
            background: replies.background,
        }
    }

    /// The protocol to draw with: kitty when there is a choice.
    #[must_use]
    pub const fn protocol(&self) -> Option<Protocol> {
        if self.kitty {
            Some(Protocol::Kitty)
        } else if self.sixel {
            Some(Protocol::Sixel)
        } else {
            None
        }
    }
}

/// The value of `QUENCH_GRAPHICS`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Forced {
    /// No graphics; nothing is sent to the terminal.
    None,
    Kitty,
    Sixel,
}

impl Forced {
    /// `none`, `kitty` or `sixel` (any case, surrounding space ignored); `None` for anything else, which
    /// is then ignored.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "none" => Some(Self::None),
            "kitty" => Some(Self::Kitty),
            "sixel" => Some(Self::Sixel),
            _ => None,
        }
    }
}

/// What detection looks at besides the terminal's answers.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Environment {
    /// `TERM`.
    pub term: Option<String>,
    /// `QUENCH_GRAPHICS`.
    pub graphics: Option<String>,
    /// Inside tmux or screen (`TMUX` or `STY` set, or `TERM` starting with `tmux` or `screen`).
    pub multiplexer: bool,
    /// Standard input and output are both terminals.
    pub terminal: bool,
}

impl Environment {
    /// This process's environment and standard descriptors.
    #[must_use]
    pub fn from_process() -> Self {
        let var = |name: &str| std::env::var(name).ok();
        let term = var("TERM");
        let multiplexer = var("TMUX").is_some_and(|v| !v.is_empty())
            || var("STY").is_some_and(|v| !v.is_empty())
            || term
                .as_deref()
                .is_some_and(|t| t.starts_with("tmux") || t.starts_with("screen"));
        Self {
            term,
            graphics: var("QUENCH_GRAPHICS"),
            multiplexer,
            terminal: is_terminal(),
        }
    }
}

#[cfg(all(unix, feature = "tui"))]
fn is_terminal() -> bool {
    crate::term::stdin_is_tty() && crate::term::stdout_is_tty()
}

#[cfg(not(all(unix, feature = "tui")))]
const fn is_terminal() -> bool {
    false
}

/// What to do, decided from the [`Environment`] alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Plan {
    /// Send the queries and read the answers.
    pub probe: bool,
    /// The protocol `QUENCH_GRAPHICS` forces.
    pub forced: Option<Forced>,
}

/// Whether to ask the terminal, and what is forced (see the module documentation for the rules).
#[must_use]
pub fn plan(environment: &Environment) -> Plan {
    let forced = environment.graphics.as_deref().and_then(Forced::parse);
    let dumb = environment
        .term
        .as_deref()
        .is_none_or(|term| term.is_empty() || term == "dumb");
    let probe =
        environment.terminal && !dumb && !environment.multiplexer && forced != Some(Forced::None);
    Plan { probe, forced }
}

/// The capabilities from the plan, the answers (`None` when nothing was asked or reading failed) and
/// the terminal's `(columns, rows)`.
#[must_use]
pub fn resolve(plan: Plan, replies: Option<&Replies>, size: Option<(u16, u16)>) -> Capabilities {
    let mut capabilities =
        replies.map_or(Capabilities::NONE, |r| Capabilities::from_replies(r, size));
    match plan.forced {
        Some(Forced::None) => capabilities = Capabilities::NONE,
        Some(Forced::Kitty) => (capabilities.kitty, capabilities.sixel) = (true, false),
        Some(Forced::Sixel) => (capabilities.kitty, capabilities.sixel) = (false, true),
        None => {}
    }
    capabilities
}

/// Sends [`QUERIES`] to `output` and reads `input` until the device attributes reply arrives or
/// `elapsed()` reaches `timeout`, then parses what came.
///
/// A read that returns no bytes means nothing has come yet (a terminal in raw mode with a read timeout
/// returns that after the timeout), not the end, so `input` should not block for long: `elapsed` is checked
/// between reads. Interrupted, would-block and timed-out reads are retried; `UnexpectedEof` stops reading.
/// A read that returns no bytes (or would block) at once, by `elapsed` less than 50 ms after it started,
/// is followed by a 10 ms sleep, so an input at its real end (which returns no bytes at once, forever)
/// costs about one read per 10 ms until the timeout instead of a busy loop.
///
/// # Errors
///
/// When writing the queries fails, or reading fails otherwise.
pub fn probe(
    output: &mut dyn Write,
    input: &mut dyn Read,
    timeout: Duration,
    elapsed: &mut dyn FnMut() -> Duration,
) -> io::Result<Replies> {
    output.write_all(QUERIES.as_bytes())?;
    output.flush()?;
    let mut received = Vec::new();
    let mut buf = [0u8; 1024];
    let mut now = elapsed();
    while now < timeout && received.len() < MAX_REPLY {
        let started = now;
        let read = input.read(&mut buf);
        now = elapsed();
        let at_once = now.saturating_sub(started) < IMMEDIATE;
        match read {
            Ok(0) => {
                if at_once {
                    std::thread::sleep(BACKOFF);
                    now = elapsed();
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                if at_once {
                    std::thread::sleep(BACKOFF);
                    now = elapsed();
                }
            }
            Ok(n) => {
                received.extend_from_slice(&buf[..n]);
                if parse_replies(&received).device_attributes.is_some() {
                    break;
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::Interrupted | io::ErrorKind::TimedOut
                ) => {}
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => break,
            Err(error) => return Err(error),
        }
    }
    Ok(parse_replies(&received))
}

/// The terminal's capabilities, asked of the controlling terminal (standard input and output).
///
/// The terminal is put in raw mode for the exchange and back afterwards; the wait is at most about
/// `timeout` (plus up to a tenth of a second, the read granularity). Without a terminal, with `TERM=dumb`,
/// or inside tmux or screen, nothing is sent and no capabilities are reported, unless `QUENCH_GRAPHICS`
/// forces a protocol (reported then without cell size or colours). Never fails: what cannot be found out
/// is reported as absent.
///
/// Standard output and standard error are flushed first, and the queries go through the locked
/// [`std::io::stdout`], so text printed before (even with `print!` and no newline) reaches the terminal
/// before the queries, and nothing printed from another thread lands between them.
#[cfg(all(unix, feature = "tui"))]
#[must_use]
pub fn detect(timeout: Duration) -> Capabilities {
    detect_in(&Environment::from_process(), timeout)
}

/// [`detect`] with the environment given.
#[cfg(all(unix, feature = "tui"))]
#[must_use]
pub fn detect_in(environment: &Environment, timeout: Duration) -> Capabilities {
    let plan = plan(environment);
    if !plan.probe {
        return resolve(plan, None, None);
    }
    let replies = probe_terminal(timeout).ok();
    let size = replies
        .as_ref()
        .filter(|r| r.cell_size_px.is_none() && r.text_area_px.is_some())
        .and_then(|_| crate::term::size().ok());
    resolve(plan, replies.as_ref(), size)
}

#[cfg(all(unix, feature = "tui"))]
fn probe_terminal(timeout: Duration) -> io::Result<Replies> {
    use crate::tty::{RawMode, open_fd};
    let not_a_terminal = || io::Error::other("not a terminal");
    // Through Rust's own buffer, after what is already in it: a separate handle on fd 1 would overtake
    // unflushed `print!` output. Flushed before raw mode, so that text keeps its usual line endings.
    let mut output = io::stdout().lock();
    output.flush()?;
    io::stderr().flush()?;
    // Reads give up after a tenth of a second, so the loop sees the clock without a reader thread.
    let raw = RawMode::enable_with_read_timeout(0, 1)?.ok_or_else(not_a_terminal)?;
    let mut input = open_fd(0).ok_or_else(not_a_terminal)?;
    let start = std::time::Instant::now();
    let replies = probe(&mut output, &mut input, timeout, &mut || start.elapsed());
    drop(raw);
    replies
}

// The kitty query is spelled out in `QUERIES` so the whole string stays a constant.
const _: () = {
    let (a, b) = (kitty::QUERY.as_bytes(), QUERIES.as_bytes());
    let mut i = 0;
    while i < a.len() {
        assert!(a[i] == b[i]);
        i += 1;
    }
};
