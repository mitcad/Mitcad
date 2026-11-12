// SPDX-License-Identifier: MIT
//! The preview a design file carries (mitcad#1539): the 3D view as the
//! application's Save left it, a PNG of at most [`MAX_SIDE`] pixels a
//! side (the application writes 256), in the file's top-level `preview`,
//! so that a file manager's thumbnailer (`mitcad-thumbnailer`,
//! `core/thumbnailer`) shows the design without loading it.
//!
//! ```json
//! "preview": { "format": "png", "width": 256, "height": 192,
//!   "data": "iVBORw0KGgoAAAANSUhEUgAAAQAAAADA…" }
//! ```
//!
//! `data` is the PNG file in standard base64 (RFC 4648, padded, no line
//! breaks); `width` and `height` are its size in pixels, for readers of the
//! file: the PNG's own header is what counts. Fields a later MitCad adds
//! to the object are ignored. A design whose file was saved before MitCad
//! kept a preview, or that only a program without a 3D view saved (the
//! command line tool, an import), has none.
//!
//! Like the design's statistics the preview is no part of the definition:
//! the model keeps it beside the document's state (not undone, not in a
//! design comparison) and writes it with the file.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{base64, image_header};

/// The design file's top-level field.
pub const FIELD: &str = "preview";

/// The largest width or height a preview may have, in pixels.
pub const MAX_SIDE: u32 = 1024;

/// The largest PNG file a preview may be, in bytes.
pub const MAX_BYTES: usize = 4 * 1024 * 1024;

/// A design's preview: a PNG file with its size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DesignPreview {
    png: Arc<[u8]>,
    width: u32,
    height: u32,
}

impl DesignPreview {
    /// The preview of the PNG file `png`, or why it cannot be one (no PNG,
    /// no pixels, larger than [`MAX_SIDE`] or [`MAX_BYTES`]).
    pub fn from_png(png: &[u8]) -> Result<Self, String> {
        if png.len() > MAX_BYTES {
            return Err(format!(
                "the preview is {} bytes, more than {MAX_BYTES}",
                png.len()
            ));
        }
        if !png.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err("the preview is no PNG image".to_owned());
        }
        let header = image_header::read(png)?;
        if header.width > MAX_SIDE || header.height > MAX_SIDE {
            return Err(format!(
                "the preview is {} x {} pixels, more than {MAX_SIDE} a side",
                header.width, header.height
            ));
        }
        Ok(Self {
            png: png.into(),
            width: header.width,
            height: header.height,
        })
    }

    /// The PNG file.
    pub fn png(&self) -> &[u8] {
        &self.png
    }

    /// Its width in pixels.
    pub fn width(&self) -> u32 {
        self.width
    }

    /// Its height in pixels.
    pub fn height(&self) -> u32 {
        self.height
    }

    /// The preview of the design file's `preview` value.
    pub fn from_value(value: Value) -> Result<Self, String> {
        #[derive(Deserialize)]
        struct PreviewIn {
            format: String,
            data: String,
        }
        let preview: PreviewIn = serde_json::from_value(value).map_err(|e| e.to_string())?;
        if preview.format != "png" {
            return Err(format!(
                "the preview's format `{}` is not read",
                preview.format
            ));
        }
        let png = base64::decode(&preview.data)?;
        Self::from_png(&png)
    }
}

impl Serialize for DesignPreview {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        #[derive(Serialize)]
        struct PreviewOut {
            format: &'static str,
            width: u32,
            height: u32,
            data: String,
        }
        PreviewOut {
            format: "png",
            width: self.width,
            height: self.height,
            data: base64::encode(&self.png),
        }
        .serialize(serializer)
    }
}

/// The preview of the design file whose text is `text`, read without the
/// rest of the design (its other fields are skipped, not read): None for
/// a design without one. An error for text that is no MitCad design file
/// and for a preview that cannot be read.
pub fn read(text: &str) -> Result<Option<DesignPreview>, String> {
    #[derive(Deserialize)]
    struct Head {
        format: Option<Value>,
        #[serde(default)]
        preview: Option<Value>,
    }
    // Some Windows editors start UTF-8 text with a byte order mark.
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let head: Head = serde_json::from_str(text).map_err(|e| format!("not valid JSON: {e}"))?;
    if head.format.as_ref().and_then(Value::as_str) != Some("mitcad") {
        return Err("not a MitCad design file".to_owned());
    }
    head.preview.map(DesignPreview::from_value).transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PNG file's signature and IHDR chunk: enough for its header.
    fn png_head(width: u32, height: u32) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.extend_from_slice(&13u32.to_be_bytes());
        bytes.extend_from_slice(b"IHDR");
        bytes.extend_from_slice(&width.to_be_bytes());
        bytes.extend_from_slice(&height.to_be_bytes());
        bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
        bytes.extend_from_slice(&[0, 0, 0, 0]); // the CRC is not checked
        bytes
    }

    #[test]
    fn a_preview_is_written_and_read_back() {
        let png = png_head(256, 192);
        let preview = DesignPreview::from_png(&png).unwrap();
        assert_eq!((preview.width(), preview.height()), (256, 192));
        let value = serde_json::to_value(&preview).unwrap();
        assert_eq!(value["format"], "png");
        assert_eq!(value["width"], 256);
        assert_eq!(value["height"], 192);
        let text = serde_json::json!({"format": "mitcad", "version": 2, "features": [],
                                      "preview": value})
        .to_string();
        let read = read(&text).unwrap().unwrap();
        assert_eq!(read.png(), png.as_slice());
        assert_eq!(read, preview);
    }

    #[test]
    fn a_design_without_one_has_none() {
        let text = r#"{"format": "mitcad", "version": 2, "parameters": [], "features": []}"#;
        assert_eq!(read(text), Ok(None));
        assert_eq!(read(r#"{"format": "mitcad", "preview": null}"#), Ok(None));
    }

    #[test]
    fn other_files_and_broken_previews_are_errors() {
        assert!(read("not json").is_err());
        assert!(read(r#"{"format": "other", "preview": {}}"#).is_err());
        let broken = r#"{"format": "mitcad", "preview": {"format": "png", "data": "!!!!"}}"#;
        assert!(read(broken).is_err());
        let jpeg = r#"{"format": "mitcad", "preview": {"format": "jpeg", "data": ""}}"#;
        assert!(read(jpeg).unwrap_err().contains("jpeg"));
        assert!(DesignPreview::from_png(b"GIF89a").is_err());
        assert!(DesignPreview::from_png(&png_head(4096, 16)).is_err());
        assert!(DesignPreview::from_png(&png_head(0, 16)).is_err());
    }

    #[test]
    fn fields_a_later_mitcad_adds_are_ignored() {
        let png = png_head(8, 8);
        let text = serde_json::json!({"format": "mitcad", "preview": {
            "format": "png", "data": base64::encode(&png), "camera": "iso"}})
        .to_string();
        assert_eq!(read(&text).unwrap().unwrap().png(), png.as_slice());
    }
}
