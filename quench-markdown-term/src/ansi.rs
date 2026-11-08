//! Writing rendered lines: with SGR styles and OSC 8 hyperlinks for a terminal, or as plain text for pipes.

use crate::model::{CalloutKind, Line, Rendered, Role, Style};
use std::fmt::Write as _;

/// The 16 basic terminal colours, which every terminal maps to its own palette (so they suit its background).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Colour {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
}

impl Colour {
    /// The SGR code setting it as the foreground colour.
    #[must_use]
    pub const fn foreground(self) -> u8 {
        match self {
            Self::Black => 30,
            Self::Red => 31,
            Self::Green => 32,
            Self::Yellow => 33,
            Self::Blue => 34,
            Self::Magenta => 35,
            Self::Cyan => 36,
            Self::White => 37,
            Self::BrightBlack => 90,
            Self::BrightRed => 91,
            Self::BrightGreen => 92,
            Self::BrightYellow => 93,
            Self::BrightBlue => 94,
            Self::BrightMagenta => 95,
            Self::BrightCyan => 96,
            Self::BrightWhite => 97,
        }
    }
}

/// The colours of the [`Role`]s; `None` leaves the terminal's own foreground.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Theme {
    /// Levels 1 to 6.
    pub headings: [Option<Colour>; 6],
    pub link: Option<Colour>,
    pub image: Option<Colour>,
    pub code: Option<Colour>,
    pub quote: Option<Colour>,
    pub rule: Option<Colour>,
    pub marker: Option<Colour>,
    pub math: Option<Colour>,
    pub html: Option<Colour>,
    pub footnote: Option<Colour>,
    pub note: Option<Colour>,
    pub tip: Option<Colour>,
    pub important: Option<Colour>,
    pub warning: Option<Colour>,
    pub highlight: Option<Colour>,
    pub new: Option<Colour>,
}

impl Default for Theme {
    /// Colours that read on dark and light backgrounds: no white, black or yellow text, grey only for
    /// decoration.
    fn default() -> Self {
        Self {
            headings: [
                Some(Colour::Magenta),
                Some(Colour::Cyan),
                Some(Colour::Green),
                None,
                None,
                None,
            ],
            link: Some(Colour::Blue),
            image: Some(Colour::Magenta),
            code: Some(Colour::Green),
            quote: Some(Colour::BrightBlack),
            rule: Some(Colour::BrightBlack),
            marker: Some(Colour::Cyan),
            math: Some(Colour::Magenta),
            html: None,
            footnote: Some(Colour::Cyan),
            note: Some(Colour::Blue),
            tip: Some(Colour::Green),
            important: Some(Colour::Magenta),
            warning: Some(Colour::Red),
            highlight: Some(Colour::Cyan),
            new: Some(Colour::Green),
        }
    }
}

impl Theme {
    /// The colour of `role`.
    #[must_use]
    pub fn colour(&self, role: Role) -> Option<Colour> {
        match role {
            Role::None => None,
            Role::Heading(level) => self.headings[usize::from(level.clamp(1, 6)) - 1],
            Role::Link => self.link,
            Role::Image => self.image,
            Role::Code => self.code,
            Role::Quote => self.quote,
            Role::Rule => self.rule,
            Role::Marker => self.marker,
            Role::Math => self.math,
            Role::Html => self.html,
            Role::Footnote => self.footnote,
            Role::Callout(kind) => match kind {
                CalloutKind::Note => self.note,
                CalloutKind::Tip => self.tip,
                CalloutKind::Important => self.important,
                CalloutKind::Warning => self.warning,
                CalloutKind::Highlight => self.highlight,
                CalloutKind::New => self.new,
            },
        }
    }
}

/// How [`to_ansi`] writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnsiOptions {
    pub theme: Theme,
    /// Links with a URL as OSC 8 hyperlinks. Render with
    /// [`Options::numbered_links`](crate::Options::numbered_links) off when this is on, so the `[n]` marks
    /// are not shown as well.
    pub hyperlinks: bool,
    /// Theme colours; without, only bold, italic, underline, strikethrough and dim.
    pub colour: bool,
}

impl Default for AnsiOptions {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            hyperlinks: false,
            colour: true,
        }
    }
}

fn sgr(style: Style, options: &AnsiOptions) -> String {
    let mut codes: Vec<u8> = Vec::new();
    for (on, code) in [
        (style.bold, 1),
        (style.dim, 2),
        (style.italic, 3),
        (style.underline, 4),
        (style.strikethrough, 9),
    ] {
        if on {
            codes.push(code);
        }
    }
    if options.colour
        && let Some(colour) = options.theme.colour(style.role)
    {
        codes.push(colour.foreground());
    }
    codes
        .iter()
        .map(u8::to_string)
        .collect::<Vec<_>>()
        .join(";")
}

/// A URL safe inside an OSC 8 sequence: no control characters.
fn osc_url(url: &str) -> String {
    url.chars().filter(|c| !c.is_control()).collect()
}

/// `rendered` for a terminal: SGR styles reset at each line's end, and OSC 8 hyperlinks when asked.
#[must_use]
pub fn to_ansi(rendered: &Rendered, options: &AnsiOptions) -> String {
    let mut out = String::new();
    for line in &rendered.lines {
        let mut current = String::new();
        let mut open: Option<usize> = None;
        for span in &line.spans {
            let link = span.style.link.filter(|&i| {
                options.hyperlinks && rendered.links.get(i).is_some_and(crate::LinkTarget::is_url)
            });
            if link != open {
                if open.is_some() {
                    out.push_str("\x1b]8;;\x1b\\");
                }
                if let Some(index) = link {
                    let _ = write!(
                        out,
                        "\x1b]8;;{}\x1b\\",
                        osc_url(&rendered.links[index].destination)
                    );
                }
                open = link;
            }
            let codes = sgr(span.style, options);
            if codes != current {
                if !current.is_empty() {
                    out.push_str("\x1b[0m");
                }
                if !codes.is_empty() {
                    let _ = write!(out, "\x1b[{codes}m");
                }
                current = codes;
            }
            out.push_str(&span.text);
        }
        if open.is_some() {
            out.push_str("\x1b]8;;\x1b\\");
        }
        if !current.is_empty() {
            out.push_str("\x1b[0m");
        }
        out.push('\n');
    }
    out
}

/// `rendered` as plain text, one line per line: what `--plain` and pipes get.
#[must_use]
pub fn to_plain(rendered: &Rendered) -> String {
    let mut out = String::new();
    for line in &rendered.lines {
        out.push_str(&Line::text(line));
        out.push('\n');
    }
    out
}
