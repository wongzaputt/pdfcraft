//! Universal OpenType text shaping engine via `rustybuzz`.
//!
//! Provides text shaping for complex scripts (Thai, Arabic, Hebrew, Indic, Vietnamese)
//! as well as OpenType features such as ligatures (e.g. fi, fl) and contextual forms.
//! Includes dynamic in-memory font source abstractions and font fallback chain partitioning.

use std::fmt;
use std::sync::Arc;
use unicode_segmentation::UnicodeSegmentation;

/// In-memory representation of a dynamic or custom font source.
#[derive(Debug, Clone)]
pub struct CustomFontSource {
    /// PostScript or family name of the font.
    pub name: String,
    /// In-memory font file buffer (OpenType / TrueType).
    pub data: Arc<Vec<u8>>,
    /// Font index within a font collection (.ttc / .otc), usually 0 for single fonts.
    pub index: u32,
}

impl CustomFontSource {
    /// Create a new in-memory custom font source.
    pub fn new(name: impl Into<String>, data: Arc<Vec<u8>>, index: u32) -> Self {
        Self { name: name.into(), data, index }
    }

    /// Whether this font contains OpenType layout tables (GSUB / GPOS).
    pub fn has_opentype_layout(&self) -> bool {
        has_opentype_layout(&self.data)
    }

    /// Check if this font covers all characters in `cluster`.
    pub fn covers_cluster(&self, cluster: &str) -> bool {
        font_covers_cluster(&self.data, cluster)
    }
}

/// Fallback chain prioritizing a primary font followed by ordered fallback alternatives.
#[derive(Debug, Clone)]
pub struct FontFallbackChain<F> {
    /// Primary font to use when glyphs are available.
    pub primary: F,
    /// Ordered fallback fonts to resolve missing glyphs.
    pub fallbacks: Vec<F>,
}

impl<F> FontFallbackChain<F> {
    /// Create a new fallback chain with a primary font and optional fallbacks.
    pub fn new(primary: F, fallbacks: Vec<F>) -> Self {
        Self { primary, fallbacks }
    }
}

/// A partitioned segment of text mapped to a chosen font index or reference.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PartitionedRun<T> {
    /// Contiguous text slice assigned to this font.
    pub text: String,
    /// Identifier or reference to the font that covers this sub-run.
    pub font: T,
}

impl FontFallbackChain<CustomFontSource> {
    /// Partition `text` into contiguous runs mapped to the first font in the chain
    /// that covers each grapheme cluster. Clusters missing in all fonts fall back to the primary font.
    pub fn partition_text<'a>(&'a self, text: &str) -> Vec<PartitionedRun<&'a CustomFontSource>> {
        if text.is_empty() {
            return Vec::new();
        }

        let mut runs: Vec<PartitionedRun<&'a CustomFontSource>> = Vec::new();

        for cluster in text.graphemes(true) {
            // Find first font that covers the cluster
            let chosen = if self.primary.covers_cluster(cluster) {
                &self.primary
            } else {
                self.fallbacks.iter().find(|fb| fb.covers_cluster(cluster)).unwrap_or(&self.primary)
            };

            if let Some(last) = runs.last_mut()
                && std::ptr::eq(last.font, chosen)
            {
                last.text.push_str(cluster);
            } else {
                runs.push(PartitionedRun { text: cluster.to_string(), font: chosen });
            }
        }

        runs
    }
}

/// Helper to check whether `font_data` covers every non-control character in `cluster`.
pub fn font_covers_cluster(font_data: &[u8], cluster: &str) -> bool {
    if let Ok(font) = skrifa::FontRef::new(font_data) {
        use skrifa::MetadataProvider as _;
        let charmap = font.charmap();
        cluster.chars().all(|c| if matches!(c, '\n' | '\r' | '\t' | ' ') { true } else { charmap.map(c).is_some_and(|g| g.to_u32() != 0) })
    } else {
        false
    }
}

/// One shaped glyph with its position and cluster information.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapedGlyph {
    /// Glyph ID inside the font.
    pub glyph_id: u16,
    /// Byte offset cluster index in original source text.
    pub cluster: u32,
    /// Horizontal advance in points (or font size units).
    pub x_advance: f32,
    /// Vertical advance in points (or font size units).
    pub y_advance: f32,
    /// Horizontal offset adjustment in points.
    pub x_offset: f32,
    /// Vertical offset adjustment in points.
    pub y_offset: f32,
}

/// A shaped text run containing positioned glyphs and total horizontal advance.
#[derive(Debug, Clone, PartialEq)]
pub struct ShapedRun {
    /// Sequence of glyphs in visual order.
    pub glyphs: Vec<ShapedGlyph>,
    /// Sum of all glyph x advances in points.
    pub total_advance_width: f32,
}

/// Errors that can occur during text shaping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShapingError {
    /// Font data could not be parsed by the OpenType shaper.
    InvalidFont,
    /// Text is too long or contains invalid control sequences.
    InvalidInput(String),
}

impl fmt::Display for ShapingError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFont => write!(f, "invalid or unparseable font data"),
            Self::InvalidInput(msg) => write!(f, "invalid shaping input: {msg}"),
        }
    }
}

impl std::error::Error for ShapingError {}

/// Shape `text` using `font_data` and target `font_size`.
///
/// If `script` or `direction` are omitted (`None`), they are automatically detected
/// from `text` using Unicode properties and directional heuristics.
pub fn shape_text(
    text: &str,
    font_data: &[u8],
    font_size: f32,
    script: Option<rustybuzz::Script>,
    direction: Option<rustybuzz::Direction>,
) -> Result<ShapedRun, ShapingError> {
    if text.is_empty() {
        return Ok(ShapedRun { glyphs: Vec::new(), total_advance_width: 0.0 });
    }
    if text.len() > 262_144 {
        return Err(ShapingError::InvalidInput("text run exceeds maximum shaping length of 256 KiB".into()));
    }

    let face = rustybuzz::Face::from_slice(font_data, 0).ok_or(ShapingError::InvalidFont)?;
    let upem = face.units_per_em() as f32;
    let scale = if upem > 0.0 { font_size / upem } else { font_size };

    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(text);

    if let Some(d) = direction {
        buffer.set_direction(d);
    } else {
        // Fallback detection using unicode-bidi base direction
        let base_rtl = unicode_bidi::get_base_direction(text) == unicode_bidi::Direction::Rtl;
        buffer.set_direction(if base_rtl { rustybuzz::Direction::RightToLeft } else { rustybuzz::Direction::LeftToRight });
    }

    if let Some(s) = script {
        buffer.set_script(s);
    } else {
        // Automatic script detection using rustybuzz's built-in Unicode script guesser
        buffer.guess_segment_properties();
    }

    let glyph_buffer = rustybuzz::shape(&face, &[], buffer);

    let infos = glyph_buffer.glyph_infos();
    let positions = glyph_buffer.glyph_positions();

    let mut glyphs = Vec::with_capacity(infos.len());
    let mut total_advance = 0.0f32;

    for (info, pos) in infos.iter().zip(positions.iter()) {
        let x_adv = pos.x_advance as f32 * scale;
        let y_adv = pos.y_advance as f32 * scale;
        let x_off = pos.x_offset as f32 * scale;
        let y_off = pos.y_offset as f32 * scale;

        total_advance += x_adv;

        let glyph_id = u16::try_from(info.glyph_id).unwrap_or(0);

        glyphs.push(ShapedGlyph { glyph_id, cluster: info.cluster, x_advance: x_adv, y_advance: y_adv, x_offset: x_off, y_offset: y_off });
    }

    Ok(ShapedRun { glyphs, total_advance_width: total_advance })
}

/// Whether the font has OpenType GSUB or GPOS layout tables.
pub fn has_opentype_layout(font_data: &[u8]) -> bool {
    if let Ok(font) = skrifa::FontRef::new(font_data) {
        use skrifa::raw::TableProvider as _;
        font.gsub().is_ok() || font.gpos().is_ok()
    } else {
        false
    }
}
