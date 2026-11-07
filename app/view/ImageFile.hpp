// SPDX-License-Identifier: MIT
// The pixels of an image reference's file (mitcad#782, mitcad#850,
// mitcad#1304): PNG, JPEG, BMP and WebP through Qt's image readers; HEIF
// (.heic, .heif) through the decoder that is there (view/HeifDecoder.hpp:
// the operating system's, or in a build with MITCAD_HEIF the libheif and
// libde265 it links). A HEIF image comes as it is shown: its rotation,
// mirroring and crop applied, which is the size
// core/model-base/src/image_header/heif.rs reads; 8 bits per channel (a
// 10-bit image reduced), with an alpha channel when it has one.
#pragma once

#include <QByteArray>
#include <QImage>
#include <QString>

namespace mitcad {

// Whether the bytes start as a HEIF file does: a `ftyp` box naming a brand
// of the HEIF family (the brands image_header/heif.rs takes).
bool isHeifFile(const QByteArray& bytes);

// Whether this build links its own HEIF decoder (MITCAD_HEIF); without it
// the operating system's decodes them.
bool heifDecoderBuilt();

// The file dialog's filter of Insert › Image Reference: every format, HEIF
// included on every platform.
QString imageFileFilter();

// The image of a file's bytes, or a null image and why in *error;
// *noDecoder says whether it is a HEIF file that no decoder here reads.
QImage decodeImageFile(const QByteArray& bytes, QString* error = nullptr, bool* noDecoder = nullptr);

} // namespace mitcad
