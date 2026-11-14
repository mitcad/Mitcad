// SPDX-License-Identifier: MIT
// The Quick Look thumbnail of a MitCad design (mitcad#1696): what the
// Finder's thumbnail extension MitCadThumbnails.appex
// (QuickLookThumbnailProvider.mm) draws for a .mitcad file. It reads the
// preview the file carries through the thumbnailer's C ABI
// (mitcad_thumbnail_pixels, core/thumbnailer/src/capi.rs), so that it has
// no reader of its own, and makes a CGImage of it. Plain C++ on Core
// Graphics, so that a test calls it without Quick Look
// (app/tests/quicklook_thumbnail_macos.mm).
#pragma once

#include <CoreGraphics/CoreGraphics.h>

#include <cstdint>
#include <string>

namespace mitcad::quicklook {

// What became of a thumbnail.
enum class ThumbnailResult {
  // The picture is in image.
  Picture,
  // The design has no preview (saved before MitCad kept them): no picture
  // and no error; Finder shows the file type's icon.
  None,
  // No MitCad design, a preview that cannot be read, a file that cannot be
  // read or is larger than kMaximumFileBytes, or a request of no size.
  Error,
};

// The largest design file read for its thumbnail (as Explorer's handler).
constexpr std::uint64_t kMaximumFileBytes = 512ull * 1024 * 1024;

// A thumbnail: the preview's pixels as an sRGB image and the size, in
// points, of the context Quick Look draws it in. Owns its image.
class Thumbnail {
public:
  Thumbnail() = default;
  explicit Thumbnail(ThumbnailResult result) : m_result(result) {}
  Thumbnail(CGImageRef image, CGSize contextSize)
      : m_result(ThumbnailResult::Picture), m_image(image), m_contextSize(contextSize) {}
  ~Thumbnail() { reset(); }
  Thumbnail(const Thumbnail&) = delete;
  Thumbnail& operator=(const Thumbnail&) = delete;
  Thumbnail(Thumbnail&& other) noexcept { *this = static_cast<Thumbnail&&>(other); }
  Thumbnail& operator=(Thumbnail&& other) noexcept {
    if (this != &other) {
      reset();
      m_result = other.m_result;
      m_image = other.m_image;
      m_contextSize = other.m_contextSize;
      other.m_result = ThumbnailResult::Error;
      other.m_image = nullptr;
      other.m_contextSize = CGSizeZero;
    }
    return *this;
  }

  ThumbnailResult result() const { return m_result; }
  // The picture (null unless result() is Picture), owned by the thumbnail.
  CGImageRef image() const { return m_image; }
  // The image's aspect ratio, fitted into the request's maximum size.
  CGSize contextSize() const { return m_contextSize; }

private:
  void reset() {
    if (m_image) {
      CGImageRelease(m_image);
      m_image = nullptr;
    }
  }

  ThumbnailResult m_result = ThumbnailResult::Error;
  CGImageRef m_image = nullptr;
  CGSize m_contextSize = CGSizeZero;
};

// The pixels a side Quick Look's request asks for: the larger side of
// maximumSize (points) at scale pixels a point, rounded up; 0 for no size.
std::uint32_t requestPixels(CGSize maximumSize, CGFloat scale);

// The thumbnail of a design file's bytes for a request of at most
// maximumSize points at scale pixels a point. The image is the preview
// scaled down to fit requestPixels() a side, never up (the C ABI); the
// context size is its aspect ratio fitted into maximumSize, so that Finder
// fills the icon's square with it at every icon size.
Thumbnail thumbnailOfBytes(const std::string& bytes, CGSize maximumSize, CGFloat scale);

// The same for the file at path (read whole, kMaximumFileBytes at most).
Thumbnail thumbnailOfFile(const std::string& path, CGSize maximumSize, CGFloat scale);

}  // namespace mitcad::quicklook
