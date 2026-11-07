// SPDX-License-Identifier: MIT
// HEIF images on macOS (mitcad#1304, platform/PlatformHeif.hpp): ImageIO,
// which every macOS has for HEIF and HEIC (and the other kinds of the HEIF
// family the system reads). ImageIO turns and mirrors the image as the
// file says (the thumbnail made with the transform, at the image's full
// size), so the image comes upright.
#include "../PlatformHeif.hpp"

#include <algorithm>
#include <cstdint>

#import <CoreFoundation/CoreFoundation.h>
#import <CoreGraphics/CoreGraphics.h>
#import <Foundation/Foundation.h>
#import <ImageIO/ImageIO.h>

#include <QImage>
#include <QImageReader>
#include <QString>

namespace mitcad {
namespace {

// A Core Foundation object, released when it goes (ARC leaves them alone).
template <typename T>
class CfOwned {
public:
  explicit CfOwned(T object) : m_object(object) {}
  ~CfOwned() {
    if (m_object != nullptr) {
      CFRelease(m_object);
    }
  }
  CfOwned(const CfOwned&) = delete;
  CfOwned& operator=(const CfOwned&) = delete;
  T get() const { return m_object; }
  explicit operator bool() const { return m_object != nullptr; }

private:
  T m_object;
};

long long integerOf(CFDictionaryRef properties, CFStringRef key) {
  const CFTypeRef value = CFDictionaryGetValue(properties, key);
  long long number = 0;
  if (value == nullptr || CFGetTypeID(value) != CFNumberGetTypeID() ||
      !CFNumberGetValue(static_cast<CFNumberRef>(value), kCFNumberLongLongType, &number)) {
    return 0;
  }
  return number;
}

} // namespace

HeifDecoded platformDecodeHeif(const QByteArray& bytes) {
  @autoreleasepool {
    const CfOwned<CFDataRef> data(CFDataCreate(kCFAllocatorDefault, reinterpret_cast<const UInt8*>(bytes.constData()),
                                               static_cast<CFIndex>(bytes.size())));
    if (!data) {
      return HeifDecoded::failed(QStringLiteral("there is no memory for the file"));
    }
    const CfOwned<CGImageSourceRef> source(CGImageSourceCreateWithData(data.get(), nullptr));
    if (!source || CGImageSourceGetCount(source.get()) == 0) {
      return HeifDecoded::failed(QStringLiteral("macOS cannot read the file"));
    }
    const CfOwned<CFDictionaryRef> properties(CGImageSourceCopyPropertiesAtIndex(source.get(), 0, nullptr));
    const long long width = properties ? integerOf(properties.get(), kCGImagePropertyPixelWidth) : 0;
    const long long height = properties ? integerOf(properties.get(), kCGImagePropertyPixelHeight) : 0;
    if (width <= 0 || height <= 0) {
      return HeifDecoded::failed(QStringLiteral("macOS reads no image in the file"));
    }
    // The limit Qt's own readers keep to (QImageReader::allocationLimit, MB).
    const int limit = QImageReader::allocationLimit();
    if (limit > 0 && width * height * 4 > (static_cast<long long>(limit) << 20)) {
      return HeifDecoded::failed(QStringLiteral("the HEIF image of %1 x %2 pixels needs more than the %3 MB an image may take")
                                     .arg(width)
                                     .arg(height)
                                     .arg(limit));
    }
    // The whole image (never the file's own small thumbnail) with the
    // file's rotation and mirroring applied.
    NSDictionary* options = @{
      (__bridge NSString*)kCGImageSourceCreateThumbnailFromImageAlways : @YES,
      (__bridge NSString*)kCGImageSourceCreateThumbnailWithTransform : @YES,
      (__bridge NSString*)kCGImageSourceShouldCacheImmediately : @YES,
      (__bridge NSString*)kCGImageSourceThumbnailMaxPixelSize : @(std::max(width, height)),
    };
    const CfOwned<CGImageRef> picture(
        CGImageSourceCreateThumbnailAtIndex(source.get(), 0, (__bridge CFDictionaryRef)options));
    if (!picture) {
      return HeifDecoded::failed(QStringLiteral("macOS cannot decode the image"));
    }
    const int columns = static_cast<int>(CGImageGetWidth(picture.get()));
    const int rows = static_cast<int>(CGImageGetHeight(picture.get()));
    QImage image(columns, rows, QImage::Format_RGBA8888_Premultiplied);
    if (image.isNull()) {
      return HeifDecoded::failed(
          QStringLiteral("there is no memory for the image's %1 x %2 pixels").arg(columns).arg(rows));
    }
    image.fill(Qt::transparent);
    // Drawn into the image's own pixels, in sRGB: the first row of the
    // bitmap is the image's top.
    const CfOwned<CGColorSpaceRef> space(CGColorSpaceCreateWithName(kCGColorSpaceSRGB));
    const CGBitmapInfo info = static_cast<CGBitmapInfo>(static_cast<std::uint32_t>(kCGImageAlphaPremultipliedLast) |
                                                        static_cast<std::uint32_t>(kCGBitmapByteOrder32Big));
    const CfOwned<CGContextRef> context(CGBitmapContextCreate(image.bits(), static_cast<size_t>(columns),
                                                              static_cast<size_t>(rows), 8,
                                                              static_cast<size_t>(image.bytesPerLine()), space.get(),
                                                              info));
    if (!context) {
      return HeifDecoded::failed(QStringLiteral("macOS cannot draw the image"));
    }
    CGContextDrawImage(context.get(), CGRectMake(0, 0, columns, rows), picture.get());
    HeifDecoded out;
    out.result = HeifDecoded::Result::Decoded;
    out.image = image;
    return out;
  }
}

} // namespace mitcad
