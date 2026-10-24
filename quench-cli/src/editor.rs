//! A small single-line editor with history, for REPL prompts.
//!
//! [`edit_line`] works on any byte stream in "raw" form (no local echo, no line
//! buffering), so the key handling is testable without a terminal;
//! [`LineEditor::readline`] puts the real terminal in raw mode around it and
//! falls back to a plain `read_line` when stdin is not a terminal.
//!
//! Supported keys: printable text (UTF-8), Enter, Backspace, Delete, the arrow
//! keys, Home/End, Ctrl-A/E/B/F/K/U/W/L/P/N/T, Alt-B/F, Ctrl-C (interrupt) and
//! Ctrl-D (end of input on an empty line, delete otherwise). The line is a
//! single row: no wrapping-aware cursor math, no completion, no reverse search.

use std::io::{self, BufRead, Read, Write};

/// How a read ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// A line, without its newline.
    Line(String),
    /// Ctrl-C.
    Interrupted,
    /// Ctrl-D on an empty line, or the end of the input.
    Eof,
}

/// A line editor that remembers what was entered.
#[derive(Debug, Default)]
pub struct LineEditor {
    history: Vec<String>,
}

impl LineEditor {
    #[must_use]
    pub const fn new() -> Self {
        Self {
            history: Vec::new(),
        }
    }

    /// Adds a line to the history, unless it is empty or repeats the last entry.
    /// Returns whether it was added.
    pub fn add_history_entry(&mut self, line: impl Into<String>) -> bool {
        let line = line.into();
        if line.trim().is_empty() || self.history.last() == Some(&line) {
            return false;
        }
        self.history.push(line);
        true
    }

    #[must_use]
    pub fn history(&self) -> &[String] {
        &self.history
    }

    /// Reads one line from the terminal, editing in place. When stdin is not a
    /// terminal (a pipe, a file, a test run) the prompt is printed and a plain
    /// line is read instead.
    ///
    /// # Errors
    ///
    /// Any I/O error from the terminal or stdin.
    pub fn readline(&mut self, prompt: &str) -> io::Result<Outcome> {
        let stdin = io::stdin();
        let mut stdout = io::stdout();
        #[cfg(unix)]
        if let Some(_raw) = crate::tty::RawMode::enable(0)? {
            let mut input = stdin.lock();
            return edit_line(&mut self.history, prompt, &mut input, &mut stdout);
        }
        stdout.write_all(prompt.as_bytes())?;
        stdout.flush()?;
        let mut line = String::new();
        if stdin.lock().read_line(&mut line)? == 0 {
            return Ok(Outcome::Eof);
        }
        Ok(Outcome::Line(
            line.trim_end_matches(['\n', '\r']).to_string(),
        ))
    }
}

/// Width of `text` on screen, ignoring ANSI escape sequences (colored prompts).
#[must_use]
pub fn visible_width(text: &str) -> usize {
    let mut width = 0;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // CSI: ESC [ ... final byte in 0x40..=0x7e.
            if chars.next() == Some('[') {
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
        } else if !c.is_control() {
            width += 1;
        }
    }
    width
}

/// A decoded key press.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Key {
    Char(char),
    Enter,
    Backspace,
    Delete,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    WordLeft,
    WordRight,
    Ctrl(char),
    Unknown,
}

/// Bytes from the input, with one byte of pushback for sequences cut short.
struct Input<R> {
    inner: R,
    pushed: Option<u8>,
}

fn read_byte<R: Read>(input: &mut Input<R>) -> io::Result<Option<u8>> {
    if let Some(byte) = input.pushed.take() {
        return Ok(Some(byte));
    }
    let mut byte = [0u8; 1];
    loop {
        match input.inner.read(&mut byte) {
            Ok(0) => return Ok(None),
            Ok(_) => return Ok(Some(byte[0])),
            Err(err) if err.kind() == io::ErrorKind::Interrupted => {}
            Err(err) => return Err(err),
        }
    }
}

fn read_key<R: Read>(input: &mut Input<R>) -> io::Result<Option<Key>> {
    let Some(first) = read_byte(input)? else {
        return Ok(None);
    };
    Ok(Some(match first {
        b'\r' | b'\n' => Key::Enter,
        0x7f | 0x08 => Key::Backspace,
        0x1b => read_escape(input)?,
        0x01..=0x1a => Key::Ctrl((b'a' + first - 1) as char),
        0x20..=0x7e => Key::Char(char::from(first)),
        0x80..=0xff => read_utf8(input, first)?,
        _ => Key::Unknown,
    }))
}

fn read_utf8<R: Read>(input: &mut Input<R>, first: u8) -> io::Result<Key> {
    let extra = match first {
        0xc0..=0xdf => 1,
        0xe0..=0xef => 2,
        0xf0..=0xf7 => 3,
        _ => return Ok(Key::Unknown),
    };
    let mut bytes = vec![first];
    for _ in 0..extra {
        match read_byte(input)? {
            Some(b) if b & 0xc0 == 0x80 => bytes.push(b),
            // Not a continuation byte: the sequence was cut short, and this byte starts the next key.
            Some(b) => {
                input.pushed = Some(b);
                return Ok(Key::Unknown);
            }
            None => return Ok(Key::Unknown),
        }
    }
    Ok(std::str::from_utf8(&bytes)
        .ok()
        .and_then(|s| s.chars().next())
        .map_or(Key::Unknown, Key::Char))
}

fn read_escape<R: Read>(input: &mut Input<R>) -> io::Result<Key> {
    let Some(next) = read_byte(input)? else {
        return Ok(Key::Unknown);
    };
    match next {
        b'[' => read_csi(input),
        b'O' => Ok(match read_byte(input)? {
            Some(b'A') => Key::Up,
            Some(b'B') => Key::Down,
            Some(b'C') => Key::Right,
            Some(b'D') => Key::Left,
            Some(b'H') => Key::Home,
            Some(b'F') => Key::End,
            _ => Key::Unknown,
        }),
        b'b' => Ok(Key::WordLeft),
        b'f' => Ok(Key::WordRight),
        _ => Ok(Key::Unknown),
    }
}

fn read_csi<R: Read>(input: &mut Input<R>) -> io::Result<Key> {
    let mut params = String::new();
    loop {
        let Some(byte) = read_byte(input)? else {
            return Ok(Key::Unknown);
        };
        if (b'@'..=b'~').contains(&byte) {
            return Ok(match (params.as_str(), byte) {
                (_, b'A') => Key::Up,
                (_, b'B') => Key::Down,
                ("1;5" | "1;3", b'C') => Key::WordRight,
                ("1;5" | "1;3", b'D') => Key::WordLeft,
                (_, b'C') => Key::Right,
                (_, b'D') => Key::Left,
                (_, b'H') | ("1" | "7", b'~') => Key::Home,
                (_, b'F') | ("4" | "8", b'~') => Key::End,
                ("3", b'~') => Key::Delete,
                _ => Key::Unknown,
            });
        }
        params.push(char::from(byte));
    }
}

/// The text being edited and where the cursor is.
struct Line {
    chars: Vec<char>,
    cursor: usize,
}

impl Line {
    fn insert(&mut self, c: char) {
        self.chars.insert(self.cursor, c);
        self.cursor += 1;
    }

    fn backspace(&mut self) {
        if self.cursor > 0 {
            self.cursor -= 1;
            self.chars.remove(self.cursor);
        }
    }

    fn delete(&mut self) {
        if self.cursor < self.chars.len() {
            self.chars.remove(self.cursor);
        }
    }

    fn word_start(&self) -> usize {
        let mut i = self.cursor;
        while i > 0 && self.chars[i - 1].is_whitespace() {
            i -= 1;
        }
        while i > 0 && !self.chars[i - 1].is_whitespace() {
            i -= 1;
        }
        i
    }

    fn word_end(&self) -> usize {
        let mut i = self.cursor;
        while i < self.chars.len() && self.chars[i].is_whitespace() {
            i += 1;
        }
        while i < self.chars.len() && !self.chars[i].is_whitespace() {
            i += 1;
        }
        i
    }

    fn set(&mut self, text: &str) {
        self.chars = text.chars().collect();
        self.cursor = self.chars.len();
    }

    fn text(&self) -> String {
        self.chars.iter().collect()
    }
}

fn redraw<W: Write>(output: &mut W, prompt: &str, line: &Line) -> io::Result<()> {
    let text = line.text();
    let column = visible_width(prompt)
        + line.chars[..line.cursor]
            .iter()
            .filter(|c| !c.is_control())
            .count();
    write!(output, "\r\x1b[K{prompt}{text}\r")?;
    if column > 0 {
        write!(output, "\x1b[{column}C")?;
    }
    output.flush()
}

/// Reads one line from a raw byte stream, echoing and redrawing on `output`.
///
/// `history` is browsed with Up/Down but not modified; the caller records the
/// result with [`LineEditor::add_history_entry`].
///
/// # Errors
///
/// Any I/O error from `input` or `output`.
pub fn edit_line<R: Read, W: Write>(
    history: &mut [String],
    prompt: &str,
    input: &mut R,
    output: &mut W,
) -> io::Result<Outcome> {
    let mut input = Input {
        inner: input,
        pushed: None,
    };
    let input = &mut input;
    let mut line = Line {
        chars: Vec::new(),
        cursor: 0,
    };
    // Position in `history` while browsing (`history.len()` is the line being typed) and that line.
    let mut browsing = history.len();
    let mut draft = String::new();
    redraw(output, prompt, &line)?;

    loop {
        let Some(key) = read_key(input)? else {
            output.write_all(b"\r\n")?;
            return Ok(if line.chars.is_empty() {
                Outcome::Eof
            } else {
                Outcome::Line(line.text())
            });
        };
        match key {
            Key::Enter => {
                output.write_all(b"\r\n")?;
                output.flush()?;
                return Ok(Outcome::Line(line.text()));
            }
            Key::Ctrl('c') => {
                output.write_all(b"^C\r\n")?;
                return Ok(Outcome::Interrupted);
            }
            Key::Ctrl('d') if line.chars.is_empty() => {
                output.write_all(b"\r\n")?;
                return Ok(Outcome::Eof);
            }
            Key::Ctrl('d') | Key::Delete => line.delete(),
            Key::Char(c) => line.insert(c),
            Key::Backspace => line.backspace(),
            Key::Left | Key::Ctrl('b') => line.cursor = line.cursor.saturating_sub(1),
            Key::Right | Key::Ctrl('f') => line.cursor = (line.cursor + 1).min(line.chars.len()),
            Key::Home | Key::Ctrl('a') => line.cursor = 0,
            Key::End | Key::Ctrl('e') => line.cursor = line.chars.len(),
            Key::WordLeft => line.cursor = line.word_start(),
            Key::WordRight => line.cursor = line.word_end(),
            Key::Ctrl('k') => line.chars.truncate(line.cursor),
            Key::Ctrl('u') => {
                line.chars.drain(..line.cursor);
                line.cursor = 0;
            }
            Key::Ctrl('w') => {
                let start = line.word_start();
                line.chars.drain(start..line.cursor);
                line.cursor = start;
            }
            Key::Ctrl('t') if line.cursor > 0 && line.chars.len() > 1 => {
                if line.cursor == line.chars.len() {
                    line.cursor -= 1;
                }
                line.chars.swap(line.cursor - 1, line.cursor);
                line.cursor += 1;
            }
            Key::Ctrl('l') => output.write_all(b"\x1b[H\x1b[2J")?,
            Key::Up | Key::Ctrl('p') if browsing > 0 => {
                if browsing == history.len() {
                    draft = line.text();
                }
                browsing -= 1;
                line.set(&history[browsing]);
            }
            Key::Down | Key::Ctrl('n') if browsing < history.len() => {
                browsing += 1;
                if browsing == history.len() {
                    let text = std::mem::take(&mut draft);
                    line.set(&text);
                } else {
                    line.set(&history[browsing]);
                }
            }
            _ => {}
        }
        redraw(output, prompt, &line)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(history: &[&str], keys: &[u8]) -> (Outcome, String) {
        let mut history: Vec<String> = history.iter().map(|s| (*s).to_string()).collect();
        let mut output = Vec::new();
        let outcome = edit_line(
            &mut history,
            "> ",
            &mut keys.to_vec().as_slice(),
            &mut output,
        )
        .unwrap();
        (outcome, String::from_utf8_lossy(&output).into_owned())
    }

    fn line(history: &[&str], keys: &[u8]) -> String {
        match run(history, keys).0 {
            Outcome::Line(text) => text,
            other => panic!("expected a line, got {other:?}"),
        }
    }

    #[quench_test::test]
    fn typing_and_enter() {
        assert_eq!(line(&[], b"hello\r"), "hello");
        assert_eq!(line(&[], b"hello\n"), "hello");
        assert_eq!(line(&[], b"\r"), "");
        let (_, screen) = run(&[], b"hi\r");
        assert!(
            screen.contains("> hi") && screen.ends_with("\r\n"),
            "{screen:?}"
        );
    }

    #[quench_test::test]
    fn utf8_text_round_trips() {
        assert_eq!(
            line(&[], "héllo wörld ✓ 🦀\r".as_bytes()),
            "héllo wörld ✓ 🦀"
        );
        // A truncated multi-byte sequence is dropped, not turned into garbage.
        assert_eq!(line(&[], b"a\xc3b\r"), "ab");
        assert_eq!(line(&[], b"a\xffb\r"), "ab");
    }

    #[quench_test::test]
    fn backspace_and_delete() {
        assert_eq!(line(&[], b"abc\x7f\r"), "ab");
        assert_eq!(line(&[], b"abc\x08\x08\r"), "a");
        assert_eq!(
            line(&[], b"\x7f\x7fx\r"),
            "x",
            "backspace on an empty line does nothing"
        );
        assert_eq!(line(&[], b"abc\x1b[D\x1b[3~\r"), "ab");
        assert_eq!(
            line(&[], b"abc\x1b[3~\r"),
            "abc",
            "delete at the end does nothing"
        );
        assert_eq!(
            line(&[], b"abc\x01\x04\r"),
            "bc",
            "ctrl-d inside a line deletes"
        );
    }

    #[quench_test::test]
    fn cursor_movement() {
        assert_eq!(line(&[], b"abc\x1b[D\x1b[Dx\r"), "axbc");
        assert_eq!(line(&[], b"abc\x1b[D\x1b[Cx\r"), "abcx");
        assert_eq!(
            line(&[], b"abc\x1b[C\x1b[Cx\r"),
            "abcx",
            "right at the end stays"
        );
        assert_eq!(
            line(&[], b"\x1b[D\x1b[Dx\r"),
            "x",
            "left at the start stays"
        );
        assert_eq!(line(&[], b"abc\x01x\r"), "xabc");
        assert_eq!(line(&[], b"abc\x01\x05x\r"), "abcx");
        assert_eq!(line(&[], b"abc\x02\x02x\x06y\r"), "axbyc");
        assert_eq!(line(&[], b"abc\x1b[Hx\x1b[Fy\r"), "xabcy");
        assert_eq!(line(&[], b"abc\x1b[1~x\x1b[4~y\r"), "xabcy");
        assert_eq!(line(&[], b"abc\x1bOHx\x1bOFy\r"), "xabcy");
        assert_eq!(line(&[], b"abc\x1bODx\x1bOCy\r"), "abxcy");
    }

    #[quench_test::test]
    fn word_wise_editing() {
        assert_eq!(line(&[], b"one two three\x17\r"), "one two ");
        assert_eq!(line(&[], b"one two   \x17\r"), "one ");
        assert_eq!(line(&[], b"\x17x\r"), "x");
        assert_eq!(line(&[], b"one two\x1bb\x1bbX\r"), "Xone two");
        assert_eq!(line(&[], b"one two\x01\x1bfX\r"), "oneX two");
        assert_eq!(line(&[], b"one two\x01\x1b[1;5CX\r"), "oneX two");
        assert_eq!(line(&[], b"one two\x1b[1;5DX\r"), "one Xtwo");
        assert_eq!(line(&[], b"one two\x01\x1bf\x1bf\x1bfX\r"), "one twoX");
    }

    #[quench_test::test]
    fn kill_commands() {
        assert_eq!(line(&[], b"hello world\x01\x1b[C\x1b[C\x0b\r"), "he");
        assert_eq!(
            line(&[], b"hello world\x1b[D\x1b[D\x1b[D\x1b[D\x1b[D\x15\r"),
            "world"
        );
        assert_eq!(
            line(&[], b"abc\x14\r"),
            "acb",
            "transpose at the end swaps the last two"
        );
        assert_eq!(
            line(&[], b"abc\x01\x14\r"),
            "abc",
            "transpose at the start does nothing"
        );
        assert_eq!(line(&[], b"a\x14\r"), "a");
        assert_eq!(line(&[], b"abc\x1b[D\x14\r"), "acb");
    }

    #[quench_test::test]
    fn history_navigation() {
        let history = ["first", "second", "third"];
        assert_eq!(line(&history, b"\x1b[A\r"), "third");
        assert_eq!(line(&history, b"\x1b[A\x1b[A\r"), "second");
        assert_eq!(
            line(&history, b"\x1b[A\x1b[A\x1b[A\x1b[A\x1b[A\r"),
            "first",
            "up stops at the oldest"
        );
        assert_eq!(
            line(&history, b"\x10\x10\x0e\r"),
            "third",
            "ctrl-p / ctrl-n"
        );
        assert_eq!(
            line(&history, b"\x1b[A\x1b[B\r"),
            "",
            "down past the newest restores the empty draft"
        );
        assert_eq!(
            line(&history, b"draft\x1b[A\x1b[B\r"),
            "draft",
            "the line being typed is kept"
        );
        assert_eq!(
            line(&history, b"\x1b[B\r"),
            "",
            "down with nothing newer does nothing"
        );
        assert_eq!(line(&history, b"\x1bOA\x1bOA\x1bOB\r"), "third");
        assert_eq!(line(&[], b"\x1b[A\x1b[Bx\r"), "x");
        assert_eq!(
            line(&history, b"\x1b[Ax\r"),
            "thirdx",
            "editing a recalled line"
        );
    }

    #[quench_test::test]
    fn interrupt_and_end_of_input() {
        assert_eq!(run(&[], b"abc\x03").0, Outcome::Interrupted);
        assert_eq!(run(&[], b"\x04").0, Outcome::Eof);
        assert_eq!(
            run(&[], b"").0,
            Outcome::Eof,
            "the stream ending on an empty line"
        );
        assert_eq!(
            run(&[], b"partial").0,
            Outcome::Line("partial".into()),
            "the stream ending mid-line yields it"
        );
        assert!(run(&[], b"x\x03").1.contains("^C"));
    }

    #[quench_test::test]
    fn unknown_keys_are_ignored() {
        assert_eq!(
            line(&[], b"a\x1b[Zb\x1b[99~c\x1bxd\x1b[1;2Ae\x1b[5~f\r"),
            "abcdef"
        );
        assert_eq!(
            line(&[], b"a\x00b\x1b\x1b[\r"),
            "ab[",
            "a doubled escape is one unknown key, the bracket is text"
        );
        assert_eq!(line(&[], b"a\x1b"), "a", "an escape at the end of input");
        assert_eq!(line(&[], b"a\x1b[1;5"), "a");
        assert_eq!(line(&[], b"a\x1bO"), "a");
    }

    #[quench_test::test]
    fn clear_screen_redraws_the_line() {
        let (outcome, screen) = run(&[], b"ab\x0c\r");
        assert_eq!(outcome, Outcome::Line("ab".into()));
        assert!(screen.contains("\x1b[H\x1b[2J"), "{screen:?}");
    }

    #[quench_test::test]
    fn redraw_positions_the_cursor_after_the_visible_prompt() {
        let mut history = Vec::new();
        let mut output = Vec::new();
        edit_line(
            &mut history,
            "\x1b[1;32mapp\x1b[0m> ",
            &mut b"xy\r".as_slice(),
            &mut output,
        )
        .unwrap();
        let screen = String::from_utf8(output).unwrap();
        // "app> " is five columns wide; the colour codes do not count.
        assert!(screen.contains("\x1b[7C"), "{screen:?}");
        assert!(
            screen.contains("\x1b[1;32mapp"),
            "the prompt is printed with its colours"
        );
    }

    #[quench_test::test]
    fn visible_width_skips_escape_sequences() {
        assert_eq!(visible_width("plain> "), 7);
        assert_eq!(visible_width("\x1b[1;32mgreen\x1b[0m"), 5);
        assert_eq!(visible_width(""), 0);
        assert_eq!(visible_width("héllo"), 5);
        assert_eq!(visible_width("a\x1b"), 1);
        assert_eq!(
            visible_width("a\x1bXb"),
            2,
            "a lone escape swallows the next character"
        );
    }

    #[quench_test::test]
    fn history_rules() {
        let mut editor = LineEditor::new();
        assert!(editor.add_history_entry("one"));
        assert!(
            !editor.add_history_entry("one"),
            "a repeat of the last entry is skipped"
        );
        assert!(!editor.add_history_entry("   "), "blank lines are skipped");
        assert!(editor.add_history_entry(String::from("two")));
        assert!(editor.add_history_entry("one"));
        assert_eq!(editor.history(), ["one", "two", "one"]);
        assert!(format!("{editor:?}").contains("LineEditor"));
    }

    struct Failing;

    impl Read for Failing {
        fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::other("broken terminal"))
        }
    }

    struct Interrupting(u8);

    impl Read for Interrupting {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.0 += 1;
            if self.0 == 1 {
                return Err(io::ErrorKind::Interrupted.into());
            }
            buf[0] = b'\r';
            Ok(1)
        }
    }

    #[quench_test::test]
    fn io_errors_propagate() {
        let mut output = Vec::new();
        let err = edit_line(&mut [], "> ", &mut Failing, &mut output).unwrap_err();
        assert_eq!(err.to_string(), "broken terminal");
        assert_eq!(
            edit_line(&mut [], "> ", &mut Interrupting(0), &mut output).unwrap(),
            Outcome::Line(String::new())
        );
    }
}
