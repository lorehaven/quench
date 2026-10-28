//! Turning a string into positioned glyphs, lines and pixels.
//!
//! One glyph per character, positioned by its advance and the font's pair kerning; no ligatures, no combining
//! marks (precomposed text, NFC, is what is laid out correctly), no right-to-left text. Lines break after spaces
//! and hyphens and, for a word wider than the line, between characters.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use quench_raster::{Canvas, Color};

use crate::font::Font;
use crate::system::SystemFonts;

/// The embedded regular face, parsed once.
///
/// # Panics
/// Never for the file that is embedded (a test parses it).
#[must_use]
pub fn noto_sans() -> Arc<Font> {
    static FONT: OnceLock<Arc<Font>> = OnceLock::new();
    Arc::clone(FONT.get_or_init(|| {
        Arc::new(
            Font::parse(&include_bytes!("../fonts/NotoSans-Regular.ttf")[..])
                .expect("the embedded font is valid"),
        )
    }))
}

/// The embedded bold face, parsed once.
///
/// # Panics
/// Never for the file that is embedded (a test parses it).
#[must_use]
pub fn noto_sans_bold() -> Arc<Font> {
    static FONT: OnceLock<Arc<Font>> = OnceLock::new();
    Arc::clone(FONT.get_or_init(|| {
        Arc::new(
            Font::parse(&include_bytes!("../fonts/NotoSans-Bold.ttf")[..])
                .expect("the embedded font is valid"),
        )
    }))
}

/// A glyph placed on a line.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Glyph {
    /// Which face of the [`FontSet`] it is from.
    pub face: usize,
    /// The glyph id in that face.
    pub id: u16,
    /// The pen position from the start of the line, in pixels.
    pub x: f32,
    /// How far the pen moves after it, kerning included, in pixels.
    pub advance: f32,
    /// The character it came from, and where that starts in the text.
    pub character: char,
    pub byte: usize,
}

/// One line of a [`Layout`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Line {
    pub glyphs: Vec<Glyph>,
    /// The width of what is drawn: trailing spaces hang past the end and do not count.
    pub width: f32,
    /// The bytes of the text this line holds, trailing spaces and the line break included.
    pub bytes: std::ops::Range<usize>,
}

/// Text laid out at one size.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub lines: Vec<Line>,
    /// Pixels per em.
    pub size: f32,
    /// The distance between baselines.
    pub line_height: f32,
    /// Pixels from the top of a line to its baseline.
    pub ascent: f32,
    /// The width of the widest line.
    pub width: f32,
    /// All the lines.
    pub height: f32,
}

/// A primary face, fallbacks, and optionally the machine's fonts for what none of them has.
pub struct FontSet {
    faces: Mutex<Vec<Arc<Font>>>,
    /// The face index that has each character (`None`: no face has it), so a miss is searched once.
    resolved: Mutex<HashMap<char, Option<usize>>>,
    system: Option<SystemFonts>,
}

impl std::fmt::Debug for FontSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FontSet")
            .field("faces", &self.faces.lock().map_or(0, |f| f.len()))
            .finish_non_exhaustive()
    }
}

impl FontSet {
    /// A set whose first face is `primary`.
    #[must_use]
    pub fn new(primary: Arc<Font>) -> Self {
        Self {
            faces: Mutex::new(vec![primary]),
            resolved: Mutex::new(HashMap::new()),
            system: None,
        }
    }

    /// The embedded Noto Sans.
    #[must_use]
    pub fn noto_sans() -> Self {
        Self::new(noto_sans())
    }

    /// The embedded Noto Sans Bold.
    #[must_use]
    pub fn noto_sans_bold() -> Self {
        Self::new(noto_sans_bold())
    }

    /// Adds a face that is tried after the ones already there.
    #[must_use]
    pub fn with_fallback(self, face: Arc<Font>) -> Self {
        self.faces
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .push(face);
        self
    }

    /// Uses the fonts installed on this machine for characters no face has.
    #[must_use]
    pub fn with_system_fonts(mut self) -> Self {
        self.system = Some(SystemFonts::default());
        self
    }

    /// As [`with_system_fonts`](Self::with_system_fonts), for fonts in these directories.
    #[must_use]
    pub fn with_system_directories(mut self, directories: Vec<PathBuf>) -> Self {
        self.system = Some(SystemFonts::in_directories(directories));
        self
    }

    /// Face `index` (0 is the primary).
    ///
    /// # Panics
    /// When there is no such face: indexes come from a [`Layout`] made by this set.
    #[must_use]
    pub fn face(&self, index: usize) -> Arc<Font> {
        Arc::clone(&self.faces.lock().unwrap_or_else(PoisonError::into_inner)[index])
    }

    /// How many faces there are, the ones loaded from the system included.
    #[must_use]
    pub fn face_count(&self) -> usize {
        self.faces
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    /// The face and glyph for `c`: the first face that has it, the system's after those; `None` when nothing does.
    fn resolve(&self, c: char) -> Option<(usize, u16)> {
        let known = self
            .resolved
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&c)
            .copied();
        if let Some(cached) = known {
            return cached.and_then(|i| self.face(i).glyph(c).map(|g| (i, g)));
        }
        let found = self.search(c);
        self.resolved
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(c, found.map(|f| f.0));
        found
    }

    fn search(&self, c: char) -> Option<(usize, u16)> {
        let count = self.face_count();
        for i in 0..count {
            if let Some(g) = self.face(i).glyph(c) {
                return Some((i, g));
            }
        }
        for path in self.system.as_ref()?.candidates(c) {
            let Ok(bytes) = std::fs::read(&path) else {
                continue;
            };
            let Ok(font) = Font::parse(bytes) else {
                continue;
            };
            if let Some(g) = font.glyph(c) {
                let mut faces = self.faces.lock().unwrap_or_else(PoisonError::into_inner);
                faces.push(Arc::new(font));
                return Some((faces.len() - 1, g));
            }
        }
        None
    }

    /// Positions `text` at `size` pixels per em, wrapping to `max_width` when given.
    #[must_use]
    pub fn layout(&self, text: &str, size: f32, max_width: Option<f32>) -> Layout {
        let primary = self.face(0);
        let scale = size / f32::from(primary.units_per_em());
        let ascent = f32::from(primary.ascent()) * scale;
        let line_height = (f32::from(primary.ascent()) - f32::from(primary.descent())
            + f32::from(primary.line_gap()))
            * scale;
        let mut lines = Vec::new();
        let mut offset = 0;
        for paragraph in text.split_inclusive('\n') {
            let content = paragraph.trim_end_matches(['\n', '\r']);
            let glyphs = self.shape(content, size, offset);
            lines.extend(break_lines(
                &glyphs,
                max_width,
                offset..offset + paragraph.len(),
            ));
            offset += paragraph.len();
        }
        if text.is_empty() || text.ends_with('\n') {
            // An empty text, or a text that ends with a line break, still has a line to put a caret on.
            lines.push(Line {
                bytes: text.len()..text.len(),
                ..Line::default()
            });
        }
        let width = lines.iter().map(|l| l.width).fold(0.0, f32::max);
        let height = line_height * lines.len() as f32;
        Layout {
            lines,
            size,
            line_height,
            ascent,
            width,
            height,
        }
    }

    /// The width of `text` on one line.
    #[must_use]
    pub fn measure(&self, text: &str, size: f32) -> f32 {
        let line = text.split('\n').next().unwrap_or("");
        self.layout(line, size, None).width
    }

    /// One glyph per character, advanced and kerned; `base` is the byte offset of `text` in the whole string.
    fn shape(&self, text: &str, size: f32, base: usize) -> Vec<Glyph> {
        let mut glyphs: Vec<Glyph> = Vec::new();
        for (byte, c) in text.char_indices() {
            if c == '\u{200B}' || (c.is_control() && c != '\t') {
                continue;
            }
            let c_shown = if c == '\t' { ' ' } else { c };
            let (face, id) = self.resolve(c_shown).unwrap_or((0, 0));
            let font = self.face(face);
            let scale = size / f32::from(font.units_per_em());
            let mut advance = f32::from(font.advance(id)) * scale;
            if c == '\t' {
                advance *= 4.0;
            }
            if let Some(previous) = glyphs.last_mut()
                && previous.face == face
            {
                previous.advance += f32::from(font.kerning(previous.id, id)) * scale;
            }
            glyphs.push(Glyph {
                face,
                id,
                x: 0.0,
                advance,
                character: c,
                byte: base + byte,
            });
        }
        glyphs
    }

    /// Draws `layout` with its top left corner at `(x, y)`.
    pub fn draw<S: AsRef<[u8]> + AsMut<[u8]>>(
        &self,
        canvas: &mut Canvas<S>,
        layout: &Layout,
        x: f32,
        y: f32,
        color: Color,
    ) {
        for (n, line) in layout.lines.iter().enumerate() {
            let baseline = y + layout.ascent + layout.line_height * n as f32;
            for glyph in &line.glyphs {
                if glyph.character.is_whitespace() {
                    continue;
                }
                let font = self.face(glyph.face);
                let scale = layout.size / f32::from(font.units_per_em());
                if let Some(outline) = font.outline(glyph.id) {
                    canvas.fill_path(
                        &outline.to_path(scale, x + glyph.x, baseline),
                        0.0,
                        0.0,
                        color,
                    );
                }
            }
        }
    }
}

/// A character after which a line may end.
const fn breaks_after(c: char) -> bool {
    matches!(c, ' ' | '\t' | '-' | '\u{2010}' | '\u{2013}' | '\u{2014}')
}

/// Cuts a paragraph's glyphs into lines no wider than `max_width` (when given), and puts each glyph at its x.
fn break_lines(
    glyphs: &[Glyph],
    max_width: Option<f32>,
    bytes: std::ops::Range<usize>,
) -> Vec<Line> {
    let mut lines: Vec<Line> = Vec::new();
    let mut start = 0;
    while start < glyphs.len() || lines.is_empty() {
        // Where this line ends: as many glyphs as fit, cut after the last break opportunity if the line is full.
        let mut end = start;
        let mut x = 0.0;
        let mut last_break = None;
        while end < glyphs.len() {
            let g = &glyphs[end];
            let hangs = g.character == ' ' || g.character == '\t';
            if let Some(max) = max_width
                && !hangs
                && end > start
                && x + g.advance > max + 0.001
            {
                end = last_break.filter(|b| *b > start).unwrap_or(end);
                break;
            }
            x += g.advance;
            end += 1;
            if breaks_after(g.character) {
                last_break = Some(end);
            }
        }
        // Spaces that end a line do not count toward its width.
        let mut line_glyphs: Vec<Glyph> = glyphs[start..end].to_vec();
        let mut pen = 0.0;
        for g in &mut line_glyphs {
            g.x = pen;
            pen += g.advance;
        }
        let shown = line_glyphs
            .iter()
            .rposition(|g| g.character != ' ' && g.character != '\t')
            .map_or(0, |i| i + 1);
        let width = line_glyphs
            .get(shown.wrapping_sub(1))
            .map_or(0.0, |g| g.x + g.advance);
        let line_bytes = glyphs.get(start).map_or(bytes.start, |g| g.byte)
            ..glyphs.get(end).map_or(bytes.end, |g| g.byte);
        lines.push(Line {
            glyphs: line_glyphs,
            width,
            bytes: line_bytes,
        });
        if end == start {
            break; // an empty paragraph is one empty line
        }
        start = end;
    }
    lines
}
