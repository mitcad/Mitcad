// SPDX-License-Identifier: MIT
// The Quick Look thumbnail of a MitCad design (mitcad#1696); see the header.
#include "QuickLookThumbnail.hpp"

#include <mitcad_thumbnailer.h>

#include <algorithm>
#include <cmath>
#include <fstream>
#include <ios>

namespace mitcad::quicklook {

namespace {

// The largest side asked for: well above any icon, below what overflows.
constexpr CGFloat kLargestRequest = 16384;

// RGBA rows from the top, not premultiplied, as the C ABI gives them, as an
// sRGB image that owns a copy of them. Null when Core Graphics refuses.
CGImageRef imageOf(const MitcadThumbnail& pixels) {
  CFDataRef data = CFDataCreate(nullptr, pixels.pixels, static_cast<CFIndex>(pixels.length));
  if (!data) {
    return nullptr;
  }
  CGDataProviderRef provider = CGDataProviderCreateWithCFData(data);
  CFRelease(data);
  if (!provider) {
    return nullptr;
  }
  CGColorSpaceRef space = CGColorSpaceCreateWithName(kCGColorSpaceSRGB);
  CGImageRef image = nullptr;
  if (space) {
    image = CGImageCreate(pixels.width, pixels.height, 8, 32, static_cast<std::size_t>(pixels.width) * 4, space,
                          static_cast<CGBitmapInfo>(kCGImageAlphaLast), provider, nullptr, true,
                          kCGRenderingIntentDefault);
    CGColorSpaceRelease(space);
  }
  CGDataProviderRelease(provider);
  return image;
}

}  // namespace

std::uint32_t requestPixels(CGSize maximumSize, CGFloat scale) {
  const CGFloat side = std::max(maximumSize.width, maximumSize.height) * (scale > 0 ? scale : 1);
  // Not `side < 1`: NaN is refused too.
  if (!(side >= 1)) {
    return 0;
  }
  return static_cast<std::uint32_t>(std::ceil(std::min(side, kLargestRequest)));
}

Thumbnail thumbnailOfBytes(const std::string& bytes, CGSize maximumSize, CGFloat scale) {
  const std::uint32_t pixels = requestPixels(maximumSize, scale);
  if (pixels == 0 || !(maximumSize.width > 0) || !(maximumSize.height > 0)) {
    return Thumbnail(ThumbnailResult::Error);
  }
  MitcadThumbnail made{};
  const std::int32_t result =
      mitcad_thumbnail_pixels(reinterpret_cast<const std::uint8_t*>(bytes.data()), bytes.size(), pixels, &made);
  if (result == MITCAD_THUMBNAIL_NONE) {
    return Thumbnail(ThumbnailResult::None);
  }
  if (result != MITCAD_THUMBNAIL_PICTURE || made.width == 0 || made.height == 0) {
    mitcad_thumbnail_free(&made);
    return Thumbnail(ThumbnailResult::Error);
  }
  CGImageRef image = imageOf(made);
  const CGFloat width = made.width;
  const CGFloat height = made.height;
  mitcad_thumbnail_free(&made);
  if (!image) {
    return Thumbnail(ThumbnailResult::Error);
  }
  // The preview's aspect ratio in the largest box maximumSize allows.
  const CGFloat fit = std::min(maximumSize.width / width, maximumSize.height / height);
  const CGSize context = CGSizeMake(std::max<CGFloat>(1, std::round(width * fit)),
                                    std::max<CGFloat>(1, std::round(height * fit)));
  return Thumbnail(image, context);
}

Thumbnail thumbnailOfFile(const std::string& path, CGSize maximumSize, CGFloat scale) {
  std::ifstream in(path, std::ios::binary | std::ios::ate);
  if (!in) {
    return Thumbnail(ThumbnailResult::Error);
  }
  const std::streamoff size = in.tellg();
  if (size < 0 || static_cast<std::uint64_t>(size) > kMaximumFileBytes) {
    return Thumbnail(ThumbnailResult::Error);
  }
  std::string bytes(static_cast<std::size_t>(size), '\0');
  in.seekg(0);
  if (size > 0 && !in.read(bytes.data(), size)) {
    return Thumbnail(ThumbnailResult::Error);
  }
  return thumbnailOfBytes(bytes, maximumSize, scale);
}

}  // namespace mitcad::quicklook
