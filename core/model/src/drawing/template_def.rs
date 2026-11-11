// SPDX-License-Identifier: MIT
//! Sheet templates and revisions as a design keeps them (mitcad#707 phase
//! 4): a template's frame, title block, fields and revision table, a
//! drawing's revisions, with their checks and the names a template's fields
//! fill in. A sheet keeps a complete copy of its template. The templates'
//! libraries, the built-in ones and their drawing on a sheet are the
//! drawing crate's (`mitcad-model-drawing`, its `template`), above the
//! model (mitcad#777).
//!
//! A title block may hold images (a company's logo, mitcad#1625): PNG,
//! JPEG or SVG files kept in the template as base64, so that a sheet's
//! copy of its template carries them.

use serde::{Deserialize, Serialize};

use super::{Orientation, P2, Projection, SheetSize};
use crate::appearance::ImageData;

/// The `format` of a template's file.
pub const TEMPLATE_FORMAT: &str = "mitcad-drawing-template";
/// A revision table's rows, mm.
#[doc(hidden)]
pub const REVISION_ROW: f64 = 5.0;

fn default_margins() -> [f64; 4] {
    [20.0, 10.0, 10.0, 10.0]
}

fn default_rows() -> usize {
    4
}

/// A sheet template: its frame, title block and revision table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SheetTemplate {
    /// [`TEMPLATE_FORMAT`] in a library's file; left out in a sheet.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    /// Unique in its library: its file's name without `.json`.
    pub id: String,
    pub name: String,
    /// Where it came from: `builtin`, `user` or `project` (given when it
    /// is read, left out of a library's file).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub library: Option<String>,
    /// The sheet size a drawing made with it starts with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<SheetSize>,
    /// The projection method a drawing made with it starts with (ISO: first
    /// angle, ANSI: third angle; mitcad#725).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projection: Option<Projection>,
    /// The frame's distances from the sheet's edges: left, bottom, right,
    /// top, mm (ISO 5457: 20 on the left for filing, 10 elsewhere).
    #[serde(default = "default_margins")]
    pub margins: [f64; 4],
    pub title_block: BlockLayout,
    /// A revision table above the title block, drawn once the drawing has
    /// a revision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub revision_table: Option<RevisionLayout>,
}

/// A title block in the frame's lower right corner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockLayout {
    /// mm.
    pub width: f64,
    pub height: f64,
    /// Thin lines in it, `[x0, y0, x1, y1]` in mm from its lower left
    /// corner; its outline is drawn wide.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub lines: Vec<[f64; 4]>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<TemplateField>,
    /// Where the projection symbol starts (its left end, on its axis), mm
    /// from the block's lower left corner; no symbol when left out.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub projection_symbol: Option<P2>,
    /// Images in it, such as a logo (mitcad#1625), drawn under its lines
    /// and texts.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<BlockImage>,
}

/// The largest image a title block holds (decoded), 4 MB.
pub const MAX_BLOCK_IMAGE: usize = 4 << 20;

/// An image of a title block (mitcad#1625).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BlockImage {
    /// Its box's lower left corner, mm from the block's lower left corner.
    pub at: P2,
    /// Its box's width and height, mm: the image is drawn whole in its
    /// middle, its proportions kept ([`fitted_box`]).
    pub size: P2,
    /// The file's bytes, base64: a PNG, JPEG or SVG image.
    pub data: ImageData,
}

/// What an image of a title block is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    Png,
    Jpeg,
    Svg,
}

impl ImageKind {
    /// `png`, `jpeg`, `svg`.
    pub fn name(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpeg",
            Self::Svg => "svg",
        }
    }

    /// Its media type, as a data URL names it.
    pub fn media_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Svg => "image/svg+xml",
        }
    }
}

/// What a file's bytes are, if they are a PNG, JPEG or SVG image: by the
/// PNG's and the JPEG's signatures, an SVG's root element.
pub fn image_kind(bytes: &[u8]) -> Option<ImageKind> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some(ImageKind::Png);
    }
    if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        return Some(ImageKind::Jpeg);
    }
    svg_root(bytes).map(|_| ImageKind::Svg)
}

/// The start tag of an SVG file's root element (`<svg …>`), within its
/// first 4 kB after an XML declaration, comments and a doctype.
fn svg_root(bytes: &[u8]) -> Option<&str> {
    let head = &bytes[..bytes.len().min(4096)];
    let text = match std::str::from_utf8(head) {
        Ok(text) => text,
        // Cut inside a character: the valid part.
        Err(e) => std::str::from_utf8(&head[..e.valid_up_to()]).ok()?,
    };
    let start = text.find("<svg")?;
    let after = text[start + 4..].chars().next()?;
    if !(after.is_whitespace() || after == '>' || after == '/') {
        return None;
    }
    let end = text[start..].find('>')?;
    Some(&text[start..start + end + 1])
}

/// The value of an attribute in a start tag.
fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let mut rest = tag;
    while let Some(at) = rest.find(name) {
        let before = rest[..at].chars().last();
        let after = rest[at + name.len()..].trim_start();
        rest = &rest[at + name.len()..];
        if !before.is_some_and(char::is_whitespace) {
            continue;
        }
        let Some(value) = after.strip_prefix('=') else {
            continue;
        };
        let value = value.trim_start();
        let quote = value.chars().next()?;
        if quote != '"' && quote != '\'' {
            continue;
        }
        let inner = &value[1..];
        return inner.find(quote).map(|end| &inner[..end]);
    }
    None
}

/// A length of an SVG's `width` or `height` without its unit (`120`,
/// `30mm`), not a percentage.
fn svg_length(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.ends_with('%') {
        return None;
    }
    let number: String = text
        .chars()
        .take_while(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | '+' | 'e' | 'E'))
        .collect();
    number
        .parse()
        .ok()
        .filter(|v: &f64| v.is_finite() && *v > 0.0)
}

/// An image's width per height: a PNG's and a JPEG's pixels, an SVG's
/// `viewBox`, else its `width` and `height`; none when it does not say.
pub fn image_aspect(bytes: &[u8], kind: ImageKind) -> Option<f64> {
    let (w, h) = match kind {
        ImageKind::Png | ImageKind::Jpeg => {
            let header = crate::image_header::read(bytes).ok()?;
            (f64::from(header.width), f64::from(header.height))
        }
        ImageKind::Svg => {
            let root = svg_root(bytes)?;
            let from_box = attribute(root, "viewBox").and_then(|b| {
                let v: Vec<f64> = b
                    .split(|c: char| c.is_whitespace() || c == ',')
                    .filter(|p| !p.is_empty())
                    .filter_map(|p| p.parse().ok())
                    .collect();
                (v.len() == 4).then(|| (v[2], v[3]))
            });
            match from_box {
                Some(size) => size,
                None => (
                    svg_length(attribute(root, "width")?)?,
                    svg_length(attribute(root, "height")?)?,
                ),
            }
        }
    };
    (w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0).then(|| w / h)
}

/// Where an image of this width per height goes in a box (its lower left
/// corner and size): whole, its proportions kept, in the box's middle; the
/// whole box when its proportions are not known.
pub fn fitted_box(at: P2, size: P2, aspect: Option<f64>) -> (P2, P2) {
    let Some(aspect) = aspect.filter(|a| a.is_finite() && *a > 0.0) else {
        return (at, size);
    };
    let (w, h) = if size[0] / size[1] > aspect {
        (size[1] * aspect, size[1])
    } else {
        (size[0], size[0] / aspect)
    };
    (
        [at[0] + (size[0] - w) / 2.0, at[1] + (size[1] - h) / 2.0],
        [w, h],
    )
}

impl BlockImage {
    /// The file's bytes and what they are, or why they are no image a
    /// title block holds.
    pub fn decoded(&self) -> Result<(Vec<u8>, ImageKind), String> {
        let bytes = crate::base64::decode(self.data.base64())
            .map_err(|e| format!("an image of its title block is no base64 data: {e}"))?;
        if bytes.len() > MAX_BLOCK_IMAGE {
            return Err(format!(
                "an image of its title block is {} kB, more than the {} kB a title block \
                 holds per image",
                bytes.len() >> 10,
                MAX_BLOCK_IMAGE >> 10
            ));
        }
        let kind = image_kind(&bytes)
            .ok_or_else(|| "an image of its title block is no PNG, JPEG or SVG image".to_owned())?;
        Ok((bytes, kind))
    }
}

/// A cell of a title block.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateField {
    /// Written small in its upper left corner (`TITLE`).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label: String,
    /// Its value: text with names in braces filled in from the design.
    pub text: String,
    /// Its lower left corner, mm from the block's lower left corner.
    pub at: P2,
    /// Its width and height, mm.
    pub size: P2,
    /// The value's height of capitals, mm (3.5 when left out); a value too
    /// wide for the cell is made smaller, then cut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
}

/// A revision table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevisionLayout {
    /// How many of the latest revisions it shows.
    #[serde(default = "default_rows")]
    pub rows: usize,
}

/// A row of a drawing's revision table.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Revision {
    /// `A`, `B`, …
    pub letter: String,
    pub description: String,
    /// YYYY-MM-DD.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub date: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
}

impl Revision {
    pub fn check(&self) -> Result<(), String> {
        if self.letter.trim().is_empty() || self.letter.chars().any(char::is_whitespace) {
            return Err("a revision is named by a letter".to_owned());
        }
        if self.description.trim().is_empty() {
            return Err(format!("revision {} needs a description", self.letter));
        }
        Ok(())
    }
}

impl SheetTemplate {
    /// Why it cannot be used, if it cannot.
    pub fn check(&self) -> Result<(), String> {
        let at = format!("template {}", self.id);
        if let Some(format) = &self.format
            && format != TEMPLATE_FORMAT
        {
            return Err(format!(
                "{at}: its format is {TEMPLATE_FORMAT}, not {format}"
            ));
        }
        if self.id.is_empty()
            || !self
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        {
            return Err(format!(
                "{at}: a template's id is letters, digits, '-' and '_'"
            ));
        }
        if self.name.trim().is_empty() {
            return Err(format!("{at}: a template needs a name"));
        }
        if !self.margins.iter().all(|m| m.is_finite() && *m >= 0.0) {
            return Err(format!("{at}: its margins are not negative"));
        }
        let block = &self.title_block;
        if !(block.width.is_finite() && block.width > 0.0)
            || !(block.height.is_finite() && block.height > 0.0)
        {
            return Err(format!("{at}: its title block has a size"));
        }
        if !block.lines.iter().flatten().all(|v| v.is_finite())
            || !block
                .projection_symbol
                .iter()
                .flatten()
                .all(|v| v.is_finite())
        {
            return Err(format!("{at}: its lines and symbol are points"));
        }
        for image in &block.images {
            let finite = image.at.iter().chain(&image.size).all(|v| v.is_finite());
            if !finite || image.size[0] <= 0.0 || image.size[1] <= 0.0 {
                return Err(format!("{at}: an image of its title block has no box"));
            }
            image.decoded().map_err(|e| format!("{at}: {e}"))?;
        }
        for field in &block.fields {
            let finite = field.at.iter().chain(&field.size).all(|v| v.is_finite());
            if !finite || field.size[0] <= 0.0 || field.size[1] <= 0.0 {
                return Err(format!("{at}: the field {} has no cell", field.text));
            }
            if let Some(h) = field.height
                && !(h.is_finite() && h > 0.0)
            {
                return Err(format!("{at}: the field {} has no height", field.text));
            }
        }
        if let Some(table) = &self.revision_table
            && !(1..=50).contains(&table.rows)
        {
            return Err(format!("{at}: a revision table shows 1 to 50 rows"));
        }
        // Its own sheet size, as a drawing made with it starts.
        if let Some(size) = self.size {
            let (width, height) = size.oriented(Orientation::Landscape);
            self.fits(width, height)?;
        }
        Ok(())
    }

    /// Why it does not fit a sheet of this size, if it does not
    /// (mitcad#722: large margins on A4 gave a frame of negative width):
    /// its margins leave a frame, and its title block with the revision
    /// table's room fits in the frame.
    pub fn fits(&self, width: f64, height: f64) -> Result<(), String> {
        let at = format!("template {}", self.id);
        let (min, max) = self.frame(width, height);
        let (w, h) = (max[0] - min[0], max[1] - min[1]);
        if !(w > 0.0 && h > 0.0) {
            return Err(format!(
                "{at}: its margins leave no frame on a {width} × {height} mm sheet"
            ));
        }
        let block = &self.title_block;
        let high = block.height + self.revision_room();
        if block.width > w + 1e-6 || high > h + 1e-6 {
            return Err(format!(
                "{at}: its title block and revision table ({} x {high} mm) do not fit in its \
                 frame ({w} x {h} mm) on a {width} x {height} mm sheet",
                block.width
            ));
        }
        Ok(())
    }

    /// The frame's corners on a sheet of this size.
    pub fn frame(&self, width: f64, height: f64) -> (P2, P2) {
        let [left, bottom, right, top] = self.margins;
        ([left, bottom], [width - right, height - top])
    }

    /// The title block's box on a sheet of this size: in the frame's lower
    /// right corner, no wider than the frame.
    pub fn title_box(&self, width: f64, height: f64) -> (P2, P2) {
        let (min, max) = self.frame(width, height);
        let w = self.title_block.width.min(max[0] - min[0]);
        (
            [max[0] - w, min[1]],
            [max[0], min[1] + self.title_block.height],
        )
    }

    /// The height its revision table takes above the title block when it
    /// shows all its rows and its header, mm; 0 without one. Views and the
    /// hole table keep clear of it from the start, so that the first
    /// revision overlaps nothing (mitcad#722).
    pub fn revision_room(&self) -> f64 {
        self.revision_table
            .as_ref()
            .map_or(0.0, |t| REVISION_ROW * (t.rows.max(1) + 1) as f64)
    }

    /// The names its fields fill in, each once, in order.
    pub fn keys(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for field in &self.title_block.fields {
            for key in keys(&field.text) {
                if !out.contains(&key) {
                    out.push(key);
                }
            }
        }
        out
    }

    /// What a drawing's sheet keeps of it: without its file's format.
    pub fn for_sheet(mut self) -> Self {
        self.format = None;
        self
    }

    /// Its choice in commands and lists: a built-in one's id (`iso`), else
    /// `user:<id>` or `project:<id>`.
    pub fn choice(&self) -> String {
        match self.library.as_deref() {
            None | Some("builtin") => self.id.clone(),
            Some(library) => format!("{library}:{}", self.id),
        }
    }
}

/// A text with the names in its braces filled in by `value`; a brace that
/// is not closed stays as it is.
pub fn fill(text: &str, value: &mut dyn FnMut(&str) -> String) -> String {
    let mut out = String::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close) => {
                out.push_str(&value(after[..close].trim()));
                rest = &after[close + 1..];
            }
            None => {
                out.push_str(&rest[open..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out
}

/// The names in a text's braces.
pub fn keys(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    fill(text, &mut |key| {
        out.push(key.to_owned());
        String::new()
    });
    out
}
