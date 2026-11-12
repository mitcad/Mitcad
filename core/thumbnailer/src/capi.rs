// SPDX-License-Identifier: MIT
//! The thumbnails' C ABI (mitcad#1695): what Windows Explorer's thumbnail
//! handler (`app/thumbnails`, a C++ COM DLL) calls to read a design file's
//! preview, so that it has no reader of its own. The static library
//! `mitcad_thumbnailer_capi` (`core/thumbnailer-capi`) carries these
//! functions; their C declarations are in
//! `core/thumbnailer-capi/include/mitcad_thumbnailer.h`.
//!
//! Nothing here panics across the boundary: a panic is caught and reported
//! as [`MITCAD_THUMBNAIL_ERROR`].

use std::panic::{self, AssertUnwindSafe};

/// The thumbnail was made: `pixels` holds it.
pub const MITCAD_THUMBNAIL_PICTURE: i32 = 0;
/// The design has no preview (saved before MitCad kept them): no picture,
/// and no error; the file manager shows the file type's icon.
pub const MITCAD_THUMBNAIL_NONE: i32 = 1;
/// No MitCad design (or not UTF-8), a preview that cannot be read, a size
/// of 0 or a null `out`.
pub const MITCAD_THUMBNAIL_ERROR: i32 = 2;

/// A thumbnail's pixels: RGBA, 8 bits a channel, not premultiplied, row by
/// row from the top; `length` is `width * height * 4`. Owned by Rust:
/// given back with [`mitcad_thumbnail_free`].
#[repr(C)]
#[derive(Debug)]
pub struct MitcadThumbnail {
    pub width: u32,
    pub height: u32,
    pub pixels: *mut u8,
    pub length: usize,
}

impl MitcadThumbnail {
    /// No picture.
    pub const EMPTY: Self = Self {
        width: 0,
        height: 0,
        pixels: std::ptr::null_mut(),
        length: 0,
    };
}

/// The thumbnail of the design file whose bytes (UTF-8 text) are `text`,
/// `length` of them, scaled down to fit `size` pixels a side
/// ([`crate::thumbnail_pixels`]). Returns [`MITCAD_THUMBNAIL_PICTURE`] with
/// the pixels in `*out`, [`MITCAD_THUMBNAIL_NONE`] for a design without a
/// preview, or [`MITCAD_THUMBNAIL_ERROR`]; `*out` is
/// [`MitcadThumbnail::EMPTY`] unless a picture was made.
///
/// # Safety
///
/// `text` points to `length` readable bytes (or is anything when `length`
/// is 0), and `out` is null or points to a writable [`MitcadThumbnail`].
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mitcad_thumbnail_pixels(
    text: *const u8,
    length: usize,
    size: u32,
    out: *mut MitcadThumbnail,
) -> i32 {
    if out.is_null() {
        return MITCAD_THUMBNAIL_ERROR;
    }
    // SAFETY: the caller passes a writable MitcadThumbnail.
    unsafe { out.write(MitcadThumbnail::EMPTY) };
    if text.is_null() && length != 0 {
        return MITCAD_THUMBNAIL_ERROR;
    }
    let bytes: &[u8] = if length == 0 {
        &[]
    } else {
        // SAFETY: the caller passes `length` readable bytes at `text`.
        unsafe { std::slice::from_raw_parts(text, length) }
    };
    let made = panic::catch_unwind(AssertUnwindSafe(|| -> Result<_, &'static str> {
        let text = std::str::from_utf8(bytes).map_err(|_| "the file is not UTF-8 text")?;
        crate::thumbnail_pixels(text, size).map_err(|_| "no thumbnail")
    }));
    match made {
        Ok(Ok(Some(picture))) => {
            let pixels = picture.rgba.into_boxed_slice();
            let length = pixels.len();
            let thumbnail = MitcadThumbnail {
                width: picture.width,
                height: picture.height,
                pixels: Box::into_raw(pixels).cast::<u8>(),
                length,
            };
            // SAFETY: as above.
            unsafe { out.write(thumbnail) };
            MITCAD_THUMBNAIL_PICTURE
        }
        Ok(Ok(None)) => MITCAD_THUMBNAIL_NONE,
        Ok(Err(_)) | Err(_) => MITCAD_THUMBNAIL_ERROR,
    }
}

/// Gives back the pixels of a thumbnail [`mitcad_thumbnail_pixels`] made,
/// and leaves `*thumbnail` [`MitcadThumbnail::EMPTY`]. Null and an empty
/// thumbnail are left as they are.
///
/// # Safety
///
/// `thumbnail` is null or points to a [`MitcadThumbnail`] that
/// [`mitcad_thumbnail_pixels`] filled in (or an empty one), whose pixels
/// have not been given back yet.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn mitcad_thumbnail_free(thumbnail: *mut MitcadThumbnail) {
    if thumbnail.is_null() {
        return;
    }
    // SAFETY: the caller passes a thumbnail this module filled in.
    let thumbnail = unsafe { &mut *thumbnail };
    if !thumbnail.pixels.is_null() {
        let pixels = std::ptr::slice_from_raw_parts_mut(thumbnail.pixels, thumbnail.length);
        // SAFETY: the pixels are the boxed slice mitcad_thumbnail_pixels
        // gave away, with its length.
        drop(unsafe { Box::from_raw(pixels) });
    }
    *thumbnail = MitcadThumbnail::EMPTY;
}
