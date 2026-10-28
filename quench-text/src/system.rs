//! Fonts found on the machine, for the characters the embedded font does not have.
//!
//! The directories are indexed once, lazily, by the code points each file's `cmap` covers (read from the file's
//! table directory and its `cmap` table alone, not the whole font); a font is loaded in full only when it is
//! the one that has a character that is asked for.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::font::{cmap_ranges, u16_at, u32_at};

/// The directories fonts are installed in, on Linux and BSD.
fn default_directories() -> Vec<PathBuf> {
    let mut dirs = vec![
        PathBuf::from("/usr/share/fonts"),
        PathBuf::from("/usr/local/share/fonts"),
        // Android keeps its fonts here (symbols, scripts); the app's own `HOME` is not where they are.
        PathBuf::from("/system/fonts"),
        PathBuf::from("/product/fonts"),
    ];
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home);
        dirs.push(home.join(".local/share/fonts"));
        dirs.push(home.join(".fonts"));
    }
    dirs
}

#[derive(Debug)]
struct Entry {
    path: PathBuf,
    ranges: Vec<(u32, u32)>,
}

/// An index of the fonts in some directories.
#[derive(Debug)]
pub struct SystemFonts {
    directories: Vec<PathBuf>,
    index: Mutex<Option<Vec<Entry>>>,
}

impl Default for SystemFonts {
    fn default() -> Self {
        Self::in_directories(default_directories())
    }
}

/// How deep directories are followed, and how many files are indexed.
const MAX_DEPTH: usize = 6;
const MAX_FILES: usize = 4096;

fn collect(dir: &Path, depth: usize, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(std::fs::DirEntry::file_name);
    for entry in entries {
        if into.len() >= MAX_FILES {
            return;
        }
        let path = entry.path();
        if path.is_dir() {
            if depth < MAX_DEPTH {
                collect(&path, depth + 1, into);
            }
        } else if path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("ttf"))
            // Colour emoji are bitmaps, which cannot be drawn from outlines: they would answer for a character
            // (a gear, say) and draw nothing, where a plain symbol font would draw it.
            && !path
                .file_name()
                .is_some_and(|n| n.to_string_lossy().to_ascii_lowercase().contains("emoji"))
        {
            into.push(path);
        }
    }
}

/// The code points a font file covers, from its `cmap` alone.
fn ranges_of(path: &Path) -> Option<Vec<(u32, u32)>> {
    let mut file = File::open(path).ok()?;
    let mut header = [0u8; 12];
    file.read_exact(&mut header).ok()?;
    if !matches!(&header[..4], [0, 1, 0, 0] | b"true") {
        return None;
    }
    let count = usize::from(u16_at(&header, 4)?).min(64);
    let mut directory = vec![0u8; count * 16];
    file.read_exact(&mut directory).ok()?;
    let (offset, length) = (0..count).find_map(|i| {
        let record = &directory[i * 16..i * 16 + 16];
        (&record[..4] == b"cmap").then(|| (u32_at(record, 8), u32_at(record, 12)))
    })?;
    let (offset, length) = (u64::from(offset?), usize::try_from(length?).ok()?);
    if length > 4 << 20 {
        return None;
    }
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut table = vec![0u8; length];
    file.read_exact(&mut table).ok()?;
    Some(cmap_ranges(&table, 0))
}

impl SystemFonts {
    /// The fonts in `directories`, searched in this order.
    #[must_use]
    pub const fn in_directories(directories: Vec<PathBuf>) -> Self {
        Self {
            directories,
            index: Mutex::new(None),
        }
    }

    /// The files whose `cmap` covers `c`, in the order of the directories. A file may still lack the glyph (a
    /// range can have gaps): the caller checks it once the font is loaded.
    #[must_use]
    pub fn candidates(&self, c: char) -> Vec<PathBuf> {
        let mut guard = self
            .index
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let index = guard.get_or_insert_with(|| {
            let mut files = Vec::new();
            for dir in &self.directories {
                collect(dir, 0, &mut files);
            }
            files
                .into_iter()
                .filter_map(|path| {
                    Some(Entry {
                        ranges: ranges_of(&path)?,
                        path,
                    })
                })
                .collect()
        });
        let code = c as u32;
        let found = index
            .iter()
            .filter(|e| {
                e.ranges
                    .iter()
                    .any(|&(first, last)| (first..=last).contains(&code))
            })
            .map(|e| e.path.clone())
            .collect();
        drop(guard);
        found
    }

    /// How many font files were indexed (0 before the first search).
    #[must_use]
    pub fn indexed(&self) -> usize {
        self.index
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .map_or(0, Vec::len)
    }
}
