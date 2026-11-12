// SPDX-License-Identifier: MIT
//! Thumbnails of MitCad designs for file managers (mitcad#1539): the
//! preview a design file carries (`mitcad_model_base::design_preview`, the
//! 3D view as the application's Save left it, 256 px at most a side),
//! read without the rest of the design, and scaled down when the file
//! manager asks for a smaller one. Never scaled up: a file manager that
//! asks for more gets the preview as it is.
//!
//! The program `mitcad-thumbnailer <design> <out.png> [<size>]` (`main.rs`)
//! is what a freedesktop thumbnailer entry runs
//! (`packaging/linux/mitcad.thumbnailer`: `Exec=mitcad-thumbnailer %i %o
//! %s`).
//!
//! Windows Explorer's thumbnail handler (`mitcad-thumbnails.dll`,
//! `app/thumbnails`, mitcad#1695) reads the same previews through the C
//! ABI of [`capi`] ([`thumbnail_pixels`]), linked into it as the static
//! library `mitcad_thumbnailer_capi` (`core/thumbnailer-capi`).

pub mod capi;
mod png_read;
mod scale;

use mitcad_model_base::design_preview;

/// The size a thumbnail is when none is asked for: the preview's own.
pub const DEFAULT_SIZE: u32 = 256;

/// The thumbnail of the design file whose text is `text`: a PNG file of at
/// most `size` pixels a side, the design's preview scaled to fit. None for
/// a design without a preview (one saved before MitCad kept them). An
/// error for text that is no MitCad design, a preview that cannot be read
/// and a `size` of 0.
pub fn thumbnail(text: &str, size: u32) -> Result<Option<Vec<u8>>, String> {
    if size == 0 {
        return Err("the size must be at least 1 pixel".to_owned());
    }
    let Some(preview) = design_preview::read(text)? else {
        return Ok(None);
    };
    if preview.width() <= size && preview.height() <= size {
        return Ok(Some(preview.png().to_vec()));
    }
    match png_read::read(preview.png()) {
        Ok(image) => {
            let small = scale::fit(&image, size);
            Ok(Some(mitcad_preview::png::encode(
                small.width,
                small.height,
                4,
                &small.pixels,
            )))
        }
        // A PNG this reader does not decode (16 bits a channel,
        // interlaced; MitCad writes neither): as it is, which file
        // managers scale themselves.
        Err(png_read::ReadError::Unsupported) => Ok(Some(preview.png().to_vec())),
        Err(png_read::ReadError::Broken(why)) => Err(format!("the preview cannot be read: {why}")),
    }
}

/// A thumbnail's pixels: RGBA, 8 bits a channel, not premultiplied, row by
/// row from the top (`width * height * 4` bytes).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pixels {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// The thumbnail of the design file whose text is `text` as pixels, for a
/// file manager that draws them itself (Windows Explorer's thumbnail
/// handler, mitcad#1695): the design's preview decoded and scaled down to
/// fit `size` pixels a side, never up. None for a design without a
/// preview; an error as for [`thumbnail`], and for a preview this reader
/// does not decode (16 bits a channel, interlaced: MitCad writes neither).
pub fn thumbnail_pixels(text: &str, size: u32) -> Result<Option<Pixels>, String> {
    if size == 0 {
        return Err("the size must be at least 1 pixel".to_owned());
    }
    let Some(preview) = design_preview::read(text)? else {
        return Ok(None);
    };
    let image = png_read::read(preview.png()).map_err(|e| match e {
        png_read::ReadError::Unsupported => {
            "the preview is a PNG this reader does not decode".to_owned()
        }
        png_read::ReadError::Broken(why) => format!("the preview cannot be read: {why}"),
    })?;
    let small = scale::fit(&image, size);
    Ok(Some(Pixels {
        width: small.width,
        height: small.height,
        rgba: small.pixels,
    }))
}

/// The width and height of a PNG file, from its header.
pub fn png_size(png: &[u8]) -> Option<(u32, u32)> {
    let header = mitcad_model_base::image_header::read(png).ok()?;
    (header.format == "png").then_some((header.width, header.height))
}

#[cfg(test)]
mod tests;
