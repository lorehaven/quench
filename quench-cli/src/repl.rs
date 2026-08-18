use rustyline::DefaultEditor;
use rustyline::error::ReadlineError;

/// What a REPL should do after handling one line, and what to prompt with
/// next - carried here instead of a separate prompt closure, so a
/// self-borrowing handler and a self-borrowing prompt never coexist.
pub enum ReplControl {
    Continue(String),
    Exit,
}

/// Reads lines until Ctrl-C, Ctrl-D, or `on_line` asks to stop, handing each
/// non-empty one to `on_line` with history already recorded. Error reporting
/// stays with `on_line` - each REPL keeps its own styling for that.
pub fn run(
    initial_prompt: impl Into<String>,
    mut on_line: impl FnMut(&str) -> ReplControl,
) -> Result<(), ReadlineError> {
    let mut editor = DefaultEditor::new()?;
    let mut prompt = initial_prompt.into();

    loop {
        let line = match editor.readline(&prompt) {
            Ok(line) => line,
            // Ctrl-C and Ctrl-D. Leaving without finishing anything is a
            // valid answer.
            Err(ReadlineError::Interrupted | ReadlineError::Eof) => break,
            Err(err) => return Err(err),
        };

        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let _ = editor.add_history_entry(trimmed);

        match on_line(trimmed) {
            ReplControl::Continue(next_prompt) => prompt = next_prompt,
            ReplControl::Exit => break,
        }
    }

    Ok(())
}
