// SPDX-License-Identifier: MIT
// The thumbnails' C ABI (mitcad#1695): the preview a MitCad design file
// carries, as pixels at the size a file manager asks for. Implemented in
// Rust (core/thumbnailer/src/capi.rs) and linked as the static library
// mitcad_thumbnailer_capi; Windows Explorer's thumbnail handler
// (app/thumbnails) calls it, so that it has no reader of its own.
#ifndef MITCAD_THUMBNAILER_H
#define MITCAD_THUMBNAILER_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

// mitcad_thumbnail_pixels' results.
// The thumbnail was made: the MitcadThumbnail holds it.
#define MITCAD_THUMBNAIL_PICTURE 0
// The design has no preview (saved before MitCad kept them): no picture and
// no error; the file manager shows the file type's icon.
#define MITCAD_THUMBNAIL_NONE 1
// No MitCad design (or not UTF-8), a preview that cannot be read, a size of
// 0 or a null out.
#define MITCAD_THUMBNAIL_ERROR 2

// A thumbnail's pixels: RGBA, 8 bits a channel, not premultiplied, row by
// row from the top; length is width * height * 4. Owned by the library:
// given back with mitcad_thumbnail_free.
typedef struct MitcadThumbnail {
  uint32_t width;
  uint32_t height;
  uint8_t* pixels;
  size_t length;
} MitcadThumbnail;

// The thumbnail of the design file whose bytes (UTF-8 text) are text,
// length of them, scaled down to fit size pixels a side, never up. *out is
// empty (zeros, null pixels) unless the result is MITCAD_THUMBNAIL_PICTURE.
int32_t mitcad_thumbnail_pixels(const uint8_t* text, size_t length, uint32_t size, MitcadThumbnail* out);

// Gives back a thumbnail's pixels and empties *thumbnail; null and an empty
// thumbnail are left as they are.
void mitcad_thumbnail_free(MitcadThumbnail* thumbnail);

#ifdef __cplusplus
}
#endif

#endif
