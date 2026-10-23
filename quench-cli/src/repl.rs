use crate::editor::{LineEditor, Outcome};

/// What a REPL should do after handling one line, and what to prompt with
/// next - carried here instead of a separate prompt closure, so a
/// self-borrowing handler and a self-borrowing prompt never coexist.
#[derive(Debug)]
pub enum ReplControl {
    Continue(String),
    Exit,
}

/// Reads lines until Ctrl-C, Ctrl-D, or `on_line` asks to stop.
///
/// Hands each non-empty one to `on_line` with history already recorded.
/// Error reporting stays with `on_line` - each REPL keeps its own styling for that.
///
/// # Errors
///
/// An I/O error from the terminal.
pub fn run(
    initial_prompt: impl Into<String>,
    on_line: impl FnMut(&str) -> ReplControl,
) -> std::io::Result<()> {
    run_with(&mut LineEditor::new(), initial_prompt, on_line)
}

/// Like [`run`], with a caller-owned editor (to keep or preload its history).
///
/// # Errors
///
/// An I/O error from the terminal.
pub fn run_with(
    editor: &mut LineEditor,
    initial_prompt: impl Into<String>,
    mut on_line: impl FnMut(&str) -> ReplControl,
) -> std::io::Result<()> {
    let mut prompt = initial_prompt.into();

    // Ctrl-C and Ctrl-D end the loop: leaving without finishing anything is a valid answer.
    while let Outcome::Line(line) = editor.readline(&prompt)? {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        editor.add_history_entry(trimmed);

        match on_line(trimmed) {
            ReplControl::Continue(next_prompt) => prompt = next_prompt,
            ReplControl::Exit => break,
        }
    }

    Ok(())
}
