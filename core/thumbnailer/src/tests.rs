// SPDX-License-Identifier: MIT
//! The thumbnails of design files' previews (mitcad#1539): the PNG reader
//! on every colour type, the scaling, and what a design without a preview
//! or a damaged one gives.

use mitcad_model_base::design_preview::DesignPreview;

use super::*;
use crate::png_read::{ReadError, Rgba};

/// A picture with something in every channel and row filter.
fn pixels(width: u32, height: u32, channels: usize) -> Vec<u8> {
    let mut out = Vec::new();
    for y in 0..height {
        for x in 0..width {
            let all = [
                (x * 7 % 256) as u8,
                (y * 5 % 256) as u8,
                ((x * y) % 256) as u8,
                (255 - (x + y) % 128) as u8,
            ];
            out.extend_from_slice(&all[..channels]);
        }
    }
    out
}

/// A design file's text with `png` as its preview.
fn design_with(png: &[u8]) -> String {
    let preview = DesignPreview::from_png(png).unwrap();
    serde_json::json!({"format": "mitcad", "version": 2, "parameters": [], "features": [],
                       "preview": preview})
    .to_string()
}

/// A PNG file made by hand: `rows` (each a filter type and its bytes),
/// IHDR's bit depth, colour type and interlace, and more chunks before the
/// pixel data. CRCs are left zero: the reader does not check them.
fn png_file(
    width: u32,
    height: u32,
    header: [u8; 3],
    chunks: &[(&[u8; 4], &[u8])],
    rows: &[u8],
) -> Vec<u8> {
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut add = |kind: &[u8; 4], body: &[u8]| {
        out.extend_from_slice(&(body.len() as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(body);
        out.extend_from_slice(&[0; 4]);
    };
    let mut ihdr = width.to_be_bytes().to_vec();
    ihdr.extend_from_slice(&height.to_be_bytes());
    ihdr.extend_from_slice(&[header[0], header[1], 0, 0, header[2]]);
    add(b"IHDR", &ihdr);
    for (kind, body) in chunks {
        add(kind, body);
    }
    add(
        b"IDAT",
        &miniz_oxide::deflate::compress_to_vec_zlib(rows, 6),
    );
    add(b"IEND", &[]);
    out
}

#[test]
fn the_reader_reads_what_the_writer_wrote() {
    for channels in [3, 4] {
        let picture = pixels(37, 23, channels);
        let png = mitcad_preview::png::encode(37, 23, channels, &picture);
        let read = png_read::read(&png).unwrap();
        assert_eq!((read.width, read.height), (37, 23));
        let expected: Vec<u8> = picture
            .chunks_exact(channels)
            .flat_map(|p| [p[0], p[1], p[2], if channels == 4 { p[3] } else { 255 }])
            .collect();
        assert_eq!(read.pixels, expected, "{channels} channels");
    }
}

#[test]
fn grey_and_palette_pictures_read_as_rgba() {
    // Grey, then grey with alpha: two pixels a row, filter "up" below.
    let grey = png_file(2, 2, [8, 0, 0], &[], &[0, 10, 20, 2, 1, 1]);
    assert_eq!(
        png_read::read(&grey).unwrap().pixels,
        [
            10, 10, 10, 255, 20, 20, 20, 255, 11, 11, 11, 255, 21, 21, 21, 255
        ]
    );
    let grey_alpha = png_file(1, 1, [8, 4, 0], &[], &[0, 50, 128]);
    assert_eq!(
        png_read::read(&grey_alpha).unwrap().pixels,
        [50, 50, 50, 128]
    );
    // A palette of two colours, the second half transparent.
    let palette: &[u8] = &[255, 0, 0, 0, 0, 255];
    let alpha: &[u8] = &[255, 128];
    let indexed = png_file(
        2,
        1,
        [8, 3, 0],
        &[(b"PLTE", palette), (b"tRNS", alpha)],
        &[0, 1, 0],
    );
    assert_eq!(
        png_read::read(&indexed).unwrap().pixels,
        [0, 0, 255, 128, 255, 0, 0, 255]
    );
    // An index outside the palette: damaged.
    let outside = png_file(1, 1, [8, 3, 0], &[(b"PLTE", palette)], &[0, 7]);
    assert!(matches!(
        png_read::read(&outside),
        Err(ReadError::Broken(_))
    ));
}

#[test]
fn other_pngs_are_unsupported_and_damaged_ones_broken() {
    let sixteen = png_file(1, 1, [16, 2, 0], &[], &[0, 0, 0, 0, 0, 0, 0]);
    assert!(matches!(
        png_read::read(&sixteen),
        Err(ReadError::Unsupported)
    ));
    let interlaced = png_file(1, 1, [8, 2, 1], &[], &[0, 1, 2, 3]);
    assert!(matches!(
        png_read::read(&interlaced),
        Err(ReadError::Unsupported)
    ));
    // Too few rows, an unknown filter, no PNG, cut short.
    let short = png_file(2, 2, [8, 0, 0], &[], &[0, 1, 2]);
    assert!(matches!(png_read::read(&short), Err(ReadError::Broken(_))));
    let filter = png_file(1, 1, [8, 0, 0], &[], &[9, 1]);
    assert!(matches!(png_read::read(&filter), Err(ReadError::Broken(_))));
    assert!(matches!(
        png_read::read(b"GIF89a"),
        Err(ReadError::Broken(_))
    ));
    let good = mitcad_preview::png::encode(4, 4, 3, &pixels(4, 4, 3));
    assert!(matches!(
        png_read::read(&good[..good.len() / 2]),
        Err(ReadError::Broken(_))
    ));
}

#[test]
fn scaling_averages_the_pixels_by_their_alpha() {
    // Opaque red beside transparent blue: red, half covered.
    let image = Rgba {
        width: 2,
        height: 1,
        pixels: vec![255, 0, 0, 255, 0, 0, 255, 0],
    };
    let one = scale::fit(&image, 1);
    assert_eq!((one.width, one.height), (1, 1));
    assert_eq!(one.pixels, [255, 0, 0, 128]);
    // Fitting already: as it is.
    assert_eq!(scale::fit(&image, 2), image);
    // A wide picture keeps its aspect ratio and at least one row.
    let wide = Rgba {
        width: 300,
        height: 2,
        pixels: vec![200; 300 * 2 * 4],
    };
    let small = scale::fit(&wide, 100);
    assert_eq!((small.width, small.height), (100, 1));
    assert!(small.pixels.iter().all(|&p| p == 200));
}

#[test]
fn a_thumbnail_is_the_preview_scaled_to_fit() {
    let preview = mitcad_preview::png::encode(256, 192, 4, &pixels(256, 192, 4));
    let text = design_with(&preview);
    // As large as the preview or larger: the preview's own bytes.
    for size in [256, 512, 1024] {
        assert_eq!(thumbnail(&text, size).unwrap().unwrap(), preview);
    }
    // Smaller: scaled, the aspect ratio kept.
    for (size, expected) in [(128, (128, 96)), (100, (100, 75)), (1, (1, 1))] {
        let small = thumbnail(&text, size).unwrap().unwrap();
        assert_eq!(png_size(&small), Some(expected), "size {size}");
        assert!(png_read::read(&small).is_ok());
    }
    // A preview this reader does not decode goes as it is.
    let sixteen = png_file(300, 1, [16, 2, 0], &[], &[]);
    let text = design_with(&sixteen);
    assert_eq!(thumbnail(&text, 128).unwrap().unwrap(), sixteen);
    assert!(thumbnail(&text, 0).is_err());
}

#[test]
fn pixels_are_the_preview_decoded_and_scaled_to_fit() {
    let picture = pixels(256, 192, 4);
    let text = design_with(&mitcad_preview::png::encode(256, 192, 4, &picture));
    let full = thumbnail_pixels(&text, 256).unwrap().unwrap();
    assert_eq!((full.width, full.height), (256, 192));
    assert_eq!(full.rgba, picture);
    assert_eq!(thumbnail_pixels(&text, 1024).unwrap().unwrap(), full);
    let small = thumbnail_pixels(&text, 96).unwrap().unwrap();
    assert_eq!((small.width, small.height), (96, 72));
    assert_eq!(small.rgba.len(), 96 * 72 * 4);
    // No preview: none; a size of 0 and a PNG this reader does not decode
    // (which a file manager drawing pixels cannot take as it is): errors.
    let old = r#"{"format": "mitcad", "version": 2, "parameters": [], "features": []}"#;
    assert_eq!(thumbnail_pixels(old, 96), Ok(None));
    assert!(thumbnail_pixels(&text, 0).is_err());
    let sixteen = design_with(&png_file(300, 1, [16, 2, 0], &[], &[]));
    assert!(
        thumbnail_pixels(&sixteen, 96)
            .unwrap_err()
            .contains("does not decode")
    );
}

#[test]
fn no_preview_is_none_and_damage_an_error() {
    let old = r#"{"format": "mitcad", "version": 2, "parameters": [], "features": []}"#;
    assert_eq!(thumbnail(old, 128), Ok(None));
    assert!(thumbnail("{}", 128).is_err());
    assert!(thumbnail("not json", 128).is_err());
    // A preview whose pixel data does not inflate: an error, when it has
    // to be scaled; the bytes as they are otherwise.
    let mut damaged = png_file(300, 2, [8, 2, 0], &[], &[0; 10]);
    let idat = damaged.windows(4).position(|w| w == b"IDAT").unwrap();
    damaged[idat + 4] ^= 0xff;
    let text = design_with(&damaged);
    assert!(
        thumbnail(&text, 128)
            .unwrap_err()
            .contains("cannot be read")
    );
}
