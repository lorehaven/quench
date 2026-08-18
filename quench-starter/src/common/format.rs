//! Plain string formatting with no `actix`/`Element` dependency, so it's
//! reachable from non-UI code too (a tool executor, a CLI).

/// A byte count as a short, human-scaled label (`"512 B"`, `"7 KB"`, `"3.4 MB"`).
pub fn human_bytes(bytes: i64) -> String {
    const KB: f64 = 1024.0;
    const MB: f64 = KB * 1024.0;
    let b = bytes as f64;
    if b >= MB {
        format!("{:.1} MB", b / MB)
    } else if b >= KB {
        format!("{:.0} KB", b / KB)
    } else {
        format!("{bytes} B")
    }
}
