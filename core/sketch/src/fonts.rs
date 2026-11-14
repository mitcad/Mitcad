// SPDX-License-Identifier: MIT
//! Font files for sketch text (mitcad#473): a `.ttf`, `.otf`, `.ttc`,
//! `.woff` or `.woff2` file read here (`wuff` decodes the web fonts), its
//! text shaped with `harfrust` (joined forms, marks, ligatures, kerning) in
//! runs of one direction found by the Unicode bidirectional algorithm
//! (`unicode-bidi`), and its glyph outlines, names and metrics read with
//! `skrifa`.
//!
//! The result has the form of the kernel's [`Kernel::font_glyphs`]: one
//! [`Glyph`] per character of the text. A cluster of characters that
//! shapes into glyphs together (a ligature, a letter with its marks) is the
//! glyph of its first character, holding all of the cluster's outlines
//! placed from the cluster's left edge; the cluster's other characters have
//! none and no advance. Layout (`text.rs`) places the characters of a line
//! in their visual order, so a right-to-left run reads from right to left.
//!
//! [`Kernel::font_glyphs`]: crate::kernel::Kernel::font_glyphs

use std::path::Path;

use harfrust::{Buffer, Direction, ShapeOptions, ShaperFont};
use skrifa::attribute::Style;
use skrifa::instance::{LocationRef, Size};
use skrifa::outline::{DrawSettings, OutlinePen};
use skrifa::raw::FileRef;
use skrifa::string::StringId;
use skrifa::{FontRef, GlyphId, MetadataProvider};
use unicode_bidi::BidiInfo;

use crate::kernel::{FontGlyphs, Glyph};

/// The slant of an italic made from an upright face (x moves by this
/// times y), about 11 degrees.
const SLANT: f64 = 0.2;

/// The bytes of a font (an OpenType font or collection: WOFF and WOFF2 are
/// decoded) and the face used.
#[derive(Debug, Clone)]
pub struct FontData {
    bytes: Vec<u8>,
    index: u32,
}

/// A text shaped in a font: its glyphs and the ids they come from.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ShapedText {
    /// One per character, as [`FontGlyphs::glyphs`]: the ids of the glyphs
    /// of the cluster the character starts, in visual order (empty for a
    /// character inside a cluster and for line breaks).
    pub ids: Vec<Vec<u32>>,
    /// The outlines and metrics.
    pub glyphs: FontGlyphs,
}

impl FontData {
    /// Reads a font file. `index` is the face in a collection.
    pub fn read(path: &Path, index: u32) -> Result<Self, String> {
        let bytes = std::fs::read(path)
            .map_err(|e| format!("the font file {} cannot be read: {e}", path.display()))?;
        Self::from_bytes(bytes, index).map_err(|e| format!("the font file {}: {e}", path.display()))
    }

    /// A font from a file's bytes (WOFF and WOFF2 decoded).
    pub fn from_bytes(bytes: Vec<u8>, index: u32) -> Result<Self, String> {
        let bytes = match bytes.get(..4) {
            Some(b"wOFF") => wuff::decompress_woff1(&bytes)
                .map_err(|e| format!("the WOFF data cannot be decoded: {e:?}"))?,
            Some(b"wOF2") => wuff::decompress_woff2(&bytes)
                .map_err(|e| format!("the WOFF2 data cannot be decoded: {e:?}"))?,
            _ => bytes,
        };
        let count = match FileRef::new(&bytes) {
            Ok(FileRef::Font(_)) => 1,
            Ok(FileRef::Collection(c)) => c.len(),
            Err(e) => return Err(format!("it is not a font that can be read ({e})")),
        };
        if index >= count {
            // One whole message for one and for several: no "(s)" (mitcad#1759).
            return Err(if count == 1 {
                format!("it has one font, not a font number {}", index + 1)
            } else {
                format!("it has {count} fonts, not a font number {}", index + 1)
            });
        }
        FontRef::from_index(&bytes, index)
            .map_err(|e| format!("it is not a font that can be read ({e})"))?;
        Ok(Self { bytes, index })
    }

    fn font(&self) -> FontRef<'_> {
        FontRef::from_index(&self.bytes, self.index).expect("checked when read")
    }

    /// A name of the font's name table, in English where it has several.
    fn name(&self, id: StringId) -> Option<String> {
        self.font()
            .localized_strings(id)
            .english_or_first()
            .map(|s| s.to_string().trim().to_owned())
            .filter(|s| !s.is_empty())
    }

    /// The font's family name.
    pub fn family(&self) -> String {
        self.name(StringId::TYPOGRAPHIC_FAMILY_NAME)
            .or_else(|| self.name(StringId::FAMILY_NAME))
            .unwrap_or_default()
    }

    /// What the font says of its copyright and licence (its name table's
    /// copyright, licence description and licence address), one per line.
    pub fn licence(&self) -> String {
        [
            StringId::COPYRIGHT_NOTICE,
            StringId::LICENSE_DESCRIPTION,
            StringId::LICENSE_URL,
        ]
        .into_iter()
        .filter_map(|id| self.name(id))
        .collect::<Vec<_>>()
        .join("\n")
    }

    /// Whether the face is an italic or oblique one.
    pub fn is_italic(&self) -> bool {
        !matches!(self.font().attributes().style, Style::Normal)
    }

    /// The text shaped in the font (see the module's notes), in em units.
    /// `italic` slants an upright face.
    pub fn shape(&self, text: &str, italic: bool) -> ShapedText {
        let font = self.font();
        let metrics = font.metrics(Size::unscaled(), LocationRef::default());
        let em = f64::from(metrics.units_per_em.max(1));
        let outlines = font.outline_glyphs();
        let slant = if italic && !self.is_italic() {
            SLANT
        } else {
            0.0
        };
        let chars: Vec<(usize, char)> = text.char_indices().collect();
        let mut result = ShapedText {
            ids: vec![Vec::new(); chars.len()],
            glyphs: FontGlyphs {
                family: self.family(),
                fallback: false,
                ascender: f64::from(metrics.ascent) / em,
                descender: f64::from(metrics.descent) / em,
                cap_height: 0.0,
                line_spacing: f64::from(metrics.ascent - metrics.descent + metrics.leading) / em,
                glyphs: vec![Glyph::default(); chars.len()],
            },
        };
        // The capitals' height: the top of an A, as the kernel's fonts.
        result.glyphs.cap_height = font
            .charmap()
            .map('A')
            .and_then(|id| {
                font.glyph_metrics(Size::unscaled(), LocationRef::default())
                    .bounds(id)
            })
            .map_or(0.0, |b| f64::from(b.y_max) / em);
        let Some(shaper_font) = harfrust::Font::new(self.bytes.clone(), self.index) else {
            return result;
        };
        let shaper = ShaperFont::new(&shaper_font);
        // The character at each byte offset.
        let char_at = |byte: usize| chars.partition_point(|(b, _)| *b < byte);
        // Each line (paragraph) on its own, in runs of one direction.
        let mut start = 0;
        for line in text.split('\n') {
            let bidi = BidiInfo::new(line, None);
            let mut run = 0;
            while run < line.len() {
                let level = bidi.levels[run];
                let mut end = run;
                while end < line.len() && bidi.levels[end] == level {
                    end += 1;
                }
                let mut buffer = Buffer::new();
                buffer.push_str(&line[run..end]);
                buffer.guess_segment_properties();
                buffer.set_direction(if level.is_rtl() {
                    Direction::RightToLeft
                } else {
                    Direction::LeftToRight
                });
                if harfrust::shape(&shaper, &mut buffer, ShapeOptions::new()).is_err() {
                    run = end;
                    continue;
                }
                let infos = buffer.glyph_infos();
                let positions = buffer.glyph_positions();
                // The glyphs come in visual order; a cluster's glyphs are
                // next to each other.
                let mut i = 0;
                while i < infos.len() {
                    let cluster = infos[i].cluster;
                    let mut j = i;
                    while j < infos.len() && infos[j].cluster == cluster {
                        j += 1;
                    }
                    let k = char_at(start + run + cluster as usize);
                    let mut pen = 0.0;
                    let mut glyph = Glyph::default();
                    let mut ids = Vec::new();
                    for (info, position) in infos[i..j].iter().zip(&positions[i..j]) {
                        ids.push(info.glyph_id);
                        let dx = (pen + f64::from(position.x_offset)) / em;
                        let dy = f64::from(position.y_offset) / em;
                        let mut outline = Outline::new(em, [dx, dy], slant);
                        if let Some(drawn) = outlines.get(GlyphId::new(info.glyph_id)) {
                            let settings =
                                DrawSettings::unhinted(Size::unscaled(), LocationRef::default());
                            if drawn.draw(settings, &mut outline).is_ok() {
                                glyph.contours.extend(outline.finish());
                            }
                        }
                        pen += f64::from(position.x_advance);
                    }
                    glyph.advance = pen / em;
                    if let Some(slot) = result.glyphs.glyphs.get_mut(k) {
                        *slot = glyph;
                        result.ids[k] = ids;
                    }
                    i = j;
                }
                run = end;
            }
            start += line.len() + 1;
        }
        result
    }
}

/// Whether a text has characters that a font's glyphs one by one do not
/// set (mitcad#473): right-to-left scripts, scripts whose letters join or
/// reorder, combining marks and the bidirectional controls. An installed
/// font's text with them is shaped by the model from the font's file
/// ([`crate::kernel::Kernel::font_source`]); other text keeps the kernel's
/// glyphs.
pub fn needs_shaping(text: &str) -> bool {
    text.chars().any(|c| {
        matches!(
            u32::from(c),
            0x0300..=0x036F
                | 0x0483..=0x0489
                | 0x0590..=0x0FFF
                | 0x1000..=0x109F
                | 0x1780..=0x18AF
                | 0x1A00..=0x1CFF
                | 0x200C..=0x200F
                | 0x202A..=0x202E
                | 0x2066..=0x2069
                | 0xA800..=0xABFF
                | 0xFB1D..=0xFDFF
                | 0xFE70..=0xFEFF
                | 0x10800..=0x11FFF
                | 0x1E900..=0x1E95F
        )
    })
}

/// A glyph's outline as closed contours of Bézier pieces in ems, moved by
/// `offset` and slanted.
struct Outline {
    em: f64,
    offset: [f64; 2],
    slant: f64,
    contours: Vec<Vec<Vec<[f64; 2]>>>,
    start: [f64; 2],
    pen: [f64; 2],
}

impl Outline {
    fn new(em: f64, offset: [f64; 2], slant: f64) -> Self {
        Self {
            em,
            offset,
            slant,
            contours: Vec::new(),
            start: [0.0; 2],
            pen: [0.0; 2],
        }
    }

    fn point(&self, x: f32, y: f32) -> [f64; 2] {
        let (x, y) = (
            f64::from(x) / self.em + self.offset[0],
            f64::from(y) / self.em + self.offset[1],
        );
        [x + self.slant * y, y]
    }

    fn piece(&mut self, points: Vec<[f64; 2]>) {
        let end = points[points.len() - 1];
        let empty = points[1..]
            .iter()
            .all(|p| (p[0] - points[0][0]).hypot(p[1] - points[0][1]) <= 1e-12);
        self.pen = end;
        if !empty && let Some(contour) = self.contours.last_mut() {
            contour.push(points);
        }
    }

    /// The closed contours; open or degenerate ones are left out.
    fn finish(mut self) -> Vec<Vec<Vec<[f64; 2]>>> {
        self.close();
        self.contours.retain(|c| c.len() >= 2);
        self.contours
    }
}

impl OutlinePen for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.close();
        let p = self.point(x, y);
        self.contours.push(Vec::new());
        self.start = p;
        self.pen = p;
    }

    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.point(x, y);
        self.piece(vec![self.pen, p]);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (c, p) = (self.point(x1, y1), self.point(x, y));
        self.piece(vec![self.pen, c, p]);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (c1, c2, p) = (self.point(x1, y1), self.point(x2, y2), self.point(x, y));
        self.piece(vec![self.pen, c1, c2, p]);
    }

    /// Closes the contour being built with a line back to its start, and
    /// makes its last piece end exactly there.
    fn close(&mut self) {
        let start = self.start;
        let Some(contour) = self.contours.last_mut() else {
            return;
        };
        let Some(last) = contour.last().map(|p| p[p.len() - 1]) else {
            return;
        };
        if (last[0] - start[0]).hypot(last[1] - start[1]) > 1e-9 {
            contour.push(vec![last, start]);
        } else if let Some(piece) = contour.last_mut() {
            let n = piece.len();
            piece[n - 1] = start;
        }
        self.pen = start;
    }
}
