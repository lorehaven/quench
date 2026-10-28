use std::io::Write;

/// `text` wrapped in the SGR `codes` (e.g. `"1;96"`), or untouched when the
/// `NO_COLOR` convention (<https://no-color.org>) asks for plain output.
fn paint(codes: &str, text: impl std::fmt::Display) -> String {
    if quench_core::env::var_os("NO_COLOR").is_some_and(|v| !v.is_empty()) {
        text.to_string()
    } else {
        format!("\x1b[{codes}m{text}\x1b[0m")
    }
}

#[derive(Debug)]
pub enum Tone {
    Info,
    Success,
    Warn,
    Error,
}

pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";
pub const DIM: &str = "\x1b[2m";
pub const CYAN: &str = "\x1b[36m";
pub const BLUE: &str = "\x1b[34m";
pub const GREEN: &str = "\x1b[32m";
pub const YELLOW: &str = "\x1b[33m";
pub const WHITE: &str = "\x1b[37m";

pub const SEP: &str = "────────────────────────────────────────────────────────";
pub const SEP_THIN: &str = "┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄┄";

pub fn print_box_banner(title: &str, subtitle: &str) {
    let border = "┌─────────────────────────────────────────────┐";
    let bottom = "└─────────────────────────────────────────────┘";

    println!("{}", paint("36", border));
    println!("{}", paint("1;97", format!("│ {title:<43} │")));
    println!("{}", paint("90", format!("│ {subtitle:<43} │")));
    println!("{}", paint("36", bottom));
}

#[allow(clippy::needless_pass_by_value)] // public API takes `Tone` by value; making it `Copy` would be a semver break
pub fn print_status(tone: Tone, label: &str, message: &str) {
    let styled_label = match tone {
        Tone::Info => paint("1;96", label),
        Tone::Success => paint("1;92", label),
        Tone::Warn => paint("1;93", label),
        Tone::Error => paint("1;91", label),
    };

    println!("{} {}", styled_label, paint("97", message));
}

pub fn print_component_preview(name: &str, description: &str) {
    let head = paint("1;96", format!("[{name}]"));
    println!("{} {}", head, paint("97", description));
}

#[must_use]
pub fn repl_prompt(app: &str, context: &str) -> String {
    format!("{}({})> ", paint("1;96", app), paint("90", context))
}

pub fn print_line(message: impl AsRef<str>) {
    println!("{}", message.as_ref());
}

pub fn print_error_line(message: impl AsRef<str>) {
    eprintln!("{}", message.as_ref());
}

pub fn print_inline(message: impl AsRef<str>) {
    print!("{}", message.as_ref());
    let _ = std::io::stdout().flush();
}
