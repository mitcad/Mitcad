// SPDX-License-Identifier: MIT
//! The C ABI Windows Explorer's thumbnail handler calls (mitcad#1695): a
//! design saved with its preview, as the application's Save writes it,
//! gives its pixels at the size asked for; a design saved without one (as
//! before MitCad kept them) gives no picture and no error; what is no
//! design is an error, and nothing panics or leaks across the boundary.

use mitcad_model::file::{DesignPreview, FileFormat, SaveStamp};
use mitcad_model::test_support::block;
use mitcad_model::testing::{remove_temp, unique_temp_dir};
use mitcad_thumbnailer::capi::{
    MITCAD_THUMBNAIL_ERROR, MITCAD_THUMBNAIL_NONE, MITCAD_THUMBNAIL_PICTURE, MitcadThumbnail,
    mitcad_thumbnail_free, mitcad_thumbnail_pixels,
};

/// A 3D view's picture: red above, blue below, opaque.
fn view(width: u32, height: u32) -> Vec<u8> {
    let mut pixels = Vec::with_capacity((width * height * 4) as usize);
    for y in 0..height {
        let colour = if y < height / 2 {
            [200, 0, 0, 255]
        } else {
            [0, 0, 200, 255]
        };
        for _ in 0..width {
            pixels.extend_from_slice(&colour);
        }
    }
    mitcad_preview::png::encode(width, height, 4, &pixels)
}

/// What the C ABI gives for `bytes` at `size`: its result and a copy of
/// the thumbnail, whose pixels are then given back.
fn call(bytes: &[u8], size: u32) -> (i32, u32, u32, Vec<u8>) {
    let mut out = MitcadThumbnail::EMPTY;
    // SAFETY: the bytes and the out value are valid for the call.
    let result = unsafe { mitcad_thumbnail_pixels(bytes.as_ptr(), bytes.len(), size, &mut out) };
    let pixels = if out.pixels.is_null() {
        Vec::new()
    } else {
        // SAFETY: the pixels the call gave, `length` of them.
        unsafe { std::slice::from_raw_parts(out.pixels, out.length) }.to_vec()
    };
    let (width, height) = (out.width, out.height);
    // SAFETY: the thumbnail the call filled in.
    unsafe { mitcad_thumbnail_free(&mut out) };
    assert!(out.pixels.is_null() && out.length == 0);
    (result, width, height, pixels)
}

fn pixel(pixels: &[u8], width: u32, x: u32, y: u32) -> [u8; 4] {
    let at = ((y * width + x) * 4) as usize;
    pixels[at..at + 4].try_into().unwrap()
}

#[test]
fn a_saved_design_gives_its_pixels_at_the_size_asked_for() {
    let dir = unique_temp_dir("thumbnailer-capi-saved");
    let design = dir.join("block.mitcad");
    let b = block();
    b.doc
        .save_file_previewed(
            &design,
            FileFormat::Single,
            &SaveStamp::now(),
            Some(DesignPreview::from_png(&view(256, 192)).unwrap()),
        )
        .unwrap();
    let bytes = std::fs::read(&design).unwrap();
    // Explorer's sizes: scaled down, the aspect ratio kept, never up.
    for (size, expected) in [(96, (96, 72)), (256, (256, 192)), (1024, (256, 192))] {
        let (result, width, height, pixels) = call(&bytes, size);
        assert_eq!(result, MITCAD_THUMBNAIL_PICTURE, "size {size}");
        assert_eq!((width, height), expected, "size {size}");
        assert_eq!(pixels.len(), (width * height * 4) as usize);
        // Row by row from the top, RGBA.
        assert_eq!(pixel(&pixels, width, 0, 0), [200, 0, 0, 255]);
        assert_eq!(
            pixel(&pixels, width, width - 1, height - 1),
            [0, 0, 200, 255]
        );
    }
    remove_temp(&dir);
}

#[test]
fn a_design_saved_without_a_preview_gives_no_picture_and_no_error() {
    let dir = unique_temp_dir("thumbnailer-capi-old");
    let design = dir.join("old.mitcad");
    block().doc.save_file(&design, FileFormat::Single).unwrap();
    let (result, width, height, pixels) = call(&std::fs::read(&design).unwrap(), 96);
    assert_eq!(result, MITCAD_THUMBNAIL_NONE);
    assert_eq!((width, height), (0, 0));
    assert!(pixels.is_empty());
    remove_temp(&dir);
}

#[test]
fn what_is_no_design_is_an_error() {
    let old = br#"{"format": "mitcad", "version": 2, "features": []}"#;
    let cases: [(&[u8], u32); 5] = [
        (b"{\"format\": \"other\"}", 96),
        (b"not json", 96),
        (b"", 96),
        (b"\xff\xfe not UTF-8", 96),
        (old, 0),
    ];
    for (bytes, size) in cases {
        let (result, _, _, pixels) = call(bytes, size);
        assert_eq!(result, MITCAD_THUMBNAIL_ERROR, "{bytes:?} at {size}");
        assert!(pixels.is_empty());
    }
    // Null pointers: an error, never a crash.
    let mut out = MitcadThumbnail::EMPTY;
    // SAFETY: null text with a length is refused before it is read; a
    // null out is refused; freeing null or an empty thumbnail does nothing.
    unsafe {
        assert_eq!(
            mitcad_thumbnail_pixels(std::ptr::null(), 4, 96, &mut out),
            MITCAD_THUMBNAIL_ERROR
        );
        assert_eq!(
            mitcad_thumbnail_pixels(old.as_ptr(), old.len(), 96, std::ptr::null_mut()),
            MITCAD_THUMBNAIL_ERROR
        );
        mitcad_thumbnail_free(std::ptr::null_mut());
        mitcad_thumbnail_free(&mut out);
    }
    assert!(out.pixels.is_null());
}
