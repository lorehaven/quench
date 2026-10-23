//! A QR code encoder (ISO/IEC 18004): any bytes, versions 1 to 40, the four error-correction levels, the
//! eight masks scored by the standard's penalty rules, and SVG output. No dependencies.
//!
//! ```
//! let code = quench_qr::QrCode::new("otpauth://totp/Forge:ann?secret=ABC").unwrap();
//! assert!(code.size() >= 21);
//! let svg = code.to_svg(200);
//! assert!(svg.starts_with("<?xml"));
//! ```
//!
//! Data is always encoded in byte mode; the other modes only save space, never change what a scanner reads.

mod galois;
mod layout;
mod tables;

use std::fmt::{self, Write as _};

pub use tables::EcLevel;

/// Why data could not be encoded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// More bytes than version 40 holds at the chosen level (`max` bytes).
    TooLong { len: usize, max: usize },
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLong { len, max } => write!(f, "{len} bytes do not fit a QR code at this error-correction level (at most {max})"),
        }
    }
}

impl std::error::Error for Error {}

/// An encoded QR code: a square grid of dark and light modules.
#[derive(Clone, PartialEq, Eq)]
pub struct QrCode {
    version: u8,
    level: EcLevel,
    mask: u8,
    size: usize,
    modules: Vec<bool>,
}

impl fmt::Debug for QrCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("QrCode")
            .field("version", &self.version)
            .field("level", &self.level)
            .field("mask", &self.mask)
            .finish_non_exhaustive()
    }
}

/// Light modules around the symbol that scanners need (the standard asks for 4).
const QUIET_ZONE: usize = 4;

impl QrCode {
    /// Encodes `data` at error-correction level M (about 15% of the symbol can be damaged), in the smallest
    /// version that holds it.
    ///
    /// # Errors
    ///
    /// [`Error::TooLong`] when even version 40 is too small.
    pub fn new(data: impl AsRef<[u8]>) -> Result<Self, Error> {
        Self::with_level(data, EcLevel::M)
    }

    /// Like [`new`](Self::new) at a chosen level.
    ///
    /// # Errors
    ///
    /// [`Error::TooLong`] when even version 40 is too small.
    pub fn with_level(data: impl AsRef<[u8]>, level: EcLevel) -> Result<Self, Error> {
        let data = data.as_ref();
        let version = (1..=40u8).find(|v| layout::fits(*v, level, data.len())).ok_or_else(|| Error::TooLong {
            len: data.len(),
            max: layout::capacity(40, level),
        })?;
        Ok(layout::build(data, version, level))
    }

    /// How many bytes version `version` (1 to 40) holds at `level`.
    #[must_use]
    pub const fn capacity(version: u8, level: EcLevel) -> usize {
        layout::capacity(version, level)
    }

    /// Modules per side (`4 * version + 17`).
    #[must_use]
    pub const fn size(&self) -> usize {
        self.size
    }

    /// The version, 1 to 40.
    #[must_use]
    pub const fn version(&self) -> u8 {
        self.version
    }

    #[must_use]
    pub const fn level(&self) -> EcLevel {
        self.level
    }

    /// The mask pattern (0 to 7) the encoder chose.
    #[must_use]
    pub const fn mask(&self) -> u8 {
        self.mask
    }

    /// Whether the module at column `x`, row `y` is dark; `false` outside the symbol.
    #[must_use]
    pub fn is_dark(&self, x: usize, y: usize) -> bool {
        x < self.size && y < self.size && self.modules[y * self.size + x]
    }

    /// A text drawing, two characters per module, for terminals and tests (`##` dark, two spaces light).
    #[must_use]
    pub fn to_text(&self) -> String {
        let mut out = String::with_capacity((self.size * 2 + 1) * self.size);
        for y in 0..self.size {
            for x in 0..self.size {
                out.push_str(if self.is_dark(x, y) { "##" } else { "  " });
            }
            out.push('\n');
        }
        out
    }

    /// The symbol as a standalone SVG document at least `min_pixels` wide and tall, with the quiet zone
    /// and a white background, scaled by a whole number of pixels per module so edges stay sharp.
    #[must_use]
    pub fn to_svg(&self, min_pixels: u32) -> String {
        let total = self.size + 2 * QUIET_ZONE;
        let total_u32 = u32::try_from(total).unwrap_or(u32::MAX);
        let scale = min_pixels.div_ceil(total_u32).max(1);
        let pixels = total_u32.saturating_mul(scale);
        let mut path = String::new();
        for y in 0..self.size {
            let mut x = 0;
            while x < self.size {
                if !self.is_dark(x, y) {
                    x += 1;
                    continue;
                }
                let start = x;
                while self.is_dark(x, y) {
                    x += 1;
                }
                let _ = write!(path, "M{} {}h{}v1h-{}z", start + QUIET_ZONE, y + QUIET_ZONE, x - start, x - start);
            }
        }
        format!(
            "<?xml version=\"1.0\" standalone=\"yes\"?>\n<svg xmlns=\"http://www.w3.org/2000/svg\" version=\"1.1\" width=\"{pixels}\" height=\"{pixels}\" viewBox=\"0 0 {total} {total}\" shape-rendering=\"crispEdges\"><rect width=\"{total}\" height=\"{total}\" fill=\"#fff\"/><path d=\"{path}\" fill=\"#000\"/></svg>\n"
        )
    }
}
