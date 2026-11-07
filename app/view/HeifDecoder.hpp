// SPDX-License-Identifier: MIT
// HEIF/HEIC images through a decoder that is already there (mitcad#1304):
// MitCad ships no HEVC decoder. macOS decodes them with ImageIO
// (platform/macos/HeifImageIO.mm), Windows with WIC and the system's HEIF
// decoder, there when Microsoft's HEIF Image Extensions and HEVC Video
// Extensions are installed (platform/windows/HeifWic.cpp), Linux with the
// distribution's libheif loaded at run time when it is installed
// (platform/unix/HeifUnix.cpp, view/HeifLibrary.hpp). A build with
// MITCAD_HEIF (off by default, releases included; mitcad#850) decodes with
// the libheif and libde265 it links instead.
//
// Nothing is looked for before a HEIF file is decoded: no check at start,
// no log line, no notice. Only a HEIF file that no decoder can read gets a
// message, which names what to install (heifDecoderMissingMessage).
#pragma once

#include <functional>

#include <QByteArray>
#include <QImage>
#include <QString>

namespace mitcad {

// What a decoder made of a HEIF file.
struct HeifDecoded {
  enum class Result {
    Decoded,   // image holds the pixels
    NoDecoder, // nothing here decodes the file: heifDecoderMissingMessage()
    Failed,    // a decoder read the file and refused it: error says why
  };
  Result result = Result::NoDecoder;
  QImage image;
  // The EXIF orientation (1 to 8) still to be applied to image: 1 when the
  // decoder applied the file's rotation and mirroring itself.
  int orientation = 1;
  // Why it failed, or (NoDecoder) what the decoder said, for the log.
  QString error;

  static HeifDecoded failed(const QString& why) { return {Result::Failed, {}, 1, why}; }
  static HeifDecoded missing(const QString& why = QString()) { return {Result::NoDecoder, {}, 1, why}; }
};

using HeifDecoderFunction = std::function<HeifDecoded(const QByteArray&)>;

// The file's primary image upright (its orientation applied), with an alpha
// channel only when a pixel is not opaque. The decoder: one set for a test
// (setHeifDecoderForTests), else in a build with MITCAD_HEIF the libheif it
// links, else the platform's (platform/PlatformHeif.hpp).
// MITCAD_TEST_HEIF_DECODER=none makes it a NoDecoder for the UI tests (read
// when a HEIF file is decoded, not at start).
HeifDecoded decodeHeif(const QByteArray& bytes);

// Tests: decodeHeif uses `decoder` instead of the build's and the platform's
// until it is set again; an empty function restores them.
void setHeifDecoderForTests(HeifDecoderFunction decoder);

// What a HEIF file with no decoder here needs, for the user: Windows the
// two extensions from the Microsoft Store, Linux the distribution's libheif
// with its HEVC plugin.
QString heifDecoderMissingMessage();

// An image turned and mirrored as an EXIF orientation (1 to 8) says, so
// that it reads upright; 1 and other values leave it as it is.
QImage orientedImage(const QImage& image, int orientation);

// What Insert › Image Reference embeds for a HEIF file (mitcad#1304): the
// image upright as a PNG file (lossless), so that the design opens on any
// machine, with or without a HEIF decoder. The PNG states 96 pixels to the
// inch, as the model takes a HEIF file that states none.
struct HeifPng {
  HeifDecoded::Result result = HeifDecoded::Result::NoDecoder;
  QByteArray png;
  int width = 0;
  int height = 0;
  QString error;
};
HeifPng heifAsPng(const QByteArray& bytes);

} // namespace mitcad
