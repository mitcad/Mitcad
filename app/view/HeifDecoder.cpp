// SPDX-License-Identifier: MIT
// HEIF images through the decoder that is there (view/HeifDecoder.hpp).
#include "view/HeifDecoder.hpp"

#include <cstddef>
#include <cstdint>
#include <cstring>
#include <memory>
#include <utility>

#include <QBuffer>
#include <QCoreApplication>
#include <QImageReader>
#include <QImageWriter>
#include <QTransform>
#include <QtGlobal>

#include "platform/PlatformHeif.hpp"

#if defined(MITCAD_HEIF)
#include <libheif/heif.h>
#endif

namespace mitcad {
namespace {

HeifDecoderFunction& testDecoder() {
  static HeifDecoderFunction decoder;
  return decoder;
}

#if defined(MITCAD_HEIF)

QString heifText(const heif_error& result) {
  return result.message != nullptr ? QString::fromUtf8(result.message) : QStringLiteral("unknown error");
}

// The libheif and libde265 this build links (MITCAD_HEIF, mitcad#850): the
// primary image as it is shown, libheif applying the file's irot, imir and
// clap and joining a grid's tiles.
HeifDecoded decodeBundled(const QByteArray& bytes) {
  // Once per process (libheif counts its users; the process' end frees it).
  static const heif_error started = heif_init(nullptr);
  if (started.code != heif_error_Ok) {
    return HeifDecoded::failed(QStringLiteral("the HEIF decoder did not start: %1").arg(heifText(started)));
  }
  const std::unique_ptr<heif_context, void (*)(heif_context*)> context(heif_context_alloc(), heif_context_free);
  if (!context) {
    return HeifDecoded::failed(QStringLiteral("the HEIF decoder has no memory"));
  }
  heif_error result = heif_context_read_from_memory_without_copy(context.get(), bytes.constData(),
                                                                 static_cast<std::size_t>(bytes.size()), nullptr);
  if (result.code != heif_error_Ok) {
    return HeifDecoded::failed(QStringLiteral("the HEIF file cannot be read: %1").arg(heifText(result)));
  }
  heif_image_handle* handle = nullptr;
  result = heif_context_get_primary_image_handle(context.get(), &handle);
  const std::unique_ptr<heif_image_handle, void (*)(const heif_image_handle*)> primary(handle,
                                                                                       heif_image_handle_release);
  if (result.code != heif_error_Ok || !primary) {
    return HeifDecoded::failed(QStringLiteral("the HEIF file has no primary image: %1").arg(heifText(result)));
  }
  const int width = heif_image_handle_get_width(handle);
  const int height = heif_image_handle_get_height(handle);
  if (width <= 0 || height <= 0) {
    return HeifDecoded::failed(QStringLiteral("the HEIF image has no pixels"));
  }
  // The limit Qt's own readers keep to (QImageReader::allocationLimit, MB).
  const int limit = QImageReader::allocationLimit();
  if (limit > 0 && static_cast<qint64>(width) * height * 4 > (static_cast<qint64>(limit) << 20)) {
    return HeifDecoded::failed(QStringLiteral("the HEIF image of %1 x %2 pixels needs more than the %3 MB an image may take")
                                   .arg(width)
                                   .arg(height)
                                   .arg(limit));
  }
  const bool alpha = heif_image_handle_has_alpha_channel(handle) != 0;
  const int channels = alpha ? 4 : 3;
  heif_image* decoded = nullptr;
  result = heif_decode_image(handle, &decoded, heif_colorspace_RGB,
                             alpha ? heif_chroma_interleaved_RGBA : heif_chroma_interleaved_RGB, nullptr);
  const std::unique_ptr<heif_image, void (*)(const heif_image*)> picture(decoded, heif_image_release);
  if (result.code != heif_error_Ok || !picture) {
    return HeifDecoded::failed(QStringLiteral("the HEIF image cannot be decoded: %1").arg(heifText(result)));
  }
  const int bits = heif_image_get_bits_per_pixel_range(decoded, heif_channel_interleaved);
  if (bits != 8) {
    return HeifDecoded::failed(QStringLiteral("the HEIF image was decoded with %1 bits per channel, not 8").arg(bits));
  }
  int stride = 0;
  const std::uint8_t* plane = heif_image_get_plane_readonly(decoded, heif_channel_interleaved, &stride);
  const int columns = heif_image_get_width(decoded, heif_channel_interleaved);
  const int rows = heif_image_get_height(decoded, heif_channel_interleaved);
  const std::size_t row = static_cast<std::size_t>(columns) * static_cast<std::size_t>(channels);
  if (plane == nullptr || columns <= 0 || rows <= 0 || stride < 0 || static_cast<std::size_t>(stride) < row) {
    return HeifDecoded::failed(QStringLiteral("the HEIF image was decoded without its pixels"));
  }
  QImage image(columns, rows, alpha ? QImage::Format_RGBA8888 : QImage::Format_RGB888);
  if (image.isNull()) {
    return HeifDecoded::failed(
        QStringLiteral("there is no memory for the image's %1 x %2 pixels").arg(columns).arg(rows));
  }
  for (int y = 0; y < rows; ++y) {
    std::memcpy(image.scanLine(y), plane + static_cast<std::ptrdiff_t>(y) * stride, row);
  }
  HeifDecoded out;
  out.result = HeifDecoded::Result::Decoded;
  out.image = image;
  return out;
}

#endif

// Whether every pixel is opaque (an image with an alpha channel that has no
// use for it: a decoder that always gives four channels).
bool opaque(const QImage& image) {
  const QImage argb = image.convertToFormat(QImage::Format_ARGB32);
  for (int y = 0; y < argb.height(); ++y) {
    const QRgb* line = reinterpret_cast<const QRgb*>(argb.constScanLine(y));
    for (int x = 0; x < argb.width(); ++x) {
      if (qAlpha(line[x]) != 255) {
        return false;
      }
    }
  }
  return true;
}

} // namespace

QImage orientedImage(const QImage& image, int orientation) {
  // The EXIF orientations as the matrix that makes them upright: 2 mirrored
  // left to right, 3 turned half, 4 mirrored top to bottom, 5 mirrored along
  // the diagonal, 6 to be turned a quarter clockwise, 7 mirrored along the
  // other diagonal, 8 to be turned a quarter anticlockwise.
  QTransform transform;
  switch (orientation) {
  case 2:
    transform = QTransform(-1, 0, 0, 1, 0, 0);
    break;
  case 3:
    transform = QTransform(-1, 0, 0, -1, 0, 0);
    break;
  case 4:
    transform = QTransform(1, 0, 0, -1, 0, 0);
    break;
  case 5:
    transform = QTransform(0, 1, 1, 0, 0, 0);
    break;
  case 6:
    transform = QTransform(0, 1, -1, 0, 0, 0);
    break;
  case 7:
    transform = QTransform(0, -1, -1, 0, 0, 0);
    break;
  case 8:
    transform = QTransform(0, -1, 1, 0, 0, 0);
    break;
  default:
    return image;
  }
  return image.transformed(transform);
}

void setHeifDecoderForTests(HeifDecoderFunction decoder) { testDecoder() = std::move(decoder); }

HeifDecoded decodeHeif(const QByteArray& bytes) {
  HeifDecoded decoded;
  if (testDecoder()) {
    decoded = testDecoder()(bytes);
  } else if (qEnvironmentVariable("MITCAD_TEST_HEIF_DECODER") == QLatin1String("none")) {
    decoded = HeifDecoded::missing(QStringLiteral("MITCAD_TEST_HEIF_DECODER=none"));
  } else {
#if defined(MITCAD_HEIF)
    decoded = decodeBundled(bytes);
#else
    decoded = platformDecodeHeif(bytes);
#endif
  }
  if (decoded.result != HeifDecoded::Result::Decoded) {
    decoded.image = QImage();
    return decoded;
  }
  if (decoded.image.isNull()) {
    return HeifDecoded::failed(QStringLiteral("the HEIF image was decoded without its pixels"));
  }
  QImage upright = orientedImage(decoded.image, decoded.orientation);
  if (upright.hasAlphaChannel() && opaque(upright)) {
    upright = upright.convertToFormat(QImage::Format_RGB888);
  }
  decoded.image = upright;
  decoded.orientation = 1;
  return decoded;
}

QString heifDecoderMissingMessage() {
#if defined(Q_OS_WIN)
  return QCoreApplication::translate(
      "HeifDecoder",
      "Reading HEIF and HEIC images on Windows needs HEIF Image Extensions and HEVC Video Extensions from the "
      "Microsoft Store. Install both, then choose the image again.");
#elif defined(Q_OS_MACOS)
  return QCoreApplication::translate("HeifDecoder", "macOS cannot read this HEIF image.");
#else
  return QCoreApplication::translate(
      "HeifDecoder",
      "Reading HEIF and HEIC images needs your distribution's libheif with its HEVC plugin (e.g. libheif1 + "
      "libheif-plugin-libde265). Install them, then choose the image again.");
#endif
}

HeifPng heifAsPng(const QByteArray& bytes) {
  HeifPng png;
  HeifDecoded decoded = decodeHeif(bytes);
  png.result = decoded.result;
  png.error = decoded.error;
  if (decoded.result != HeifDecoded::Result::Decoded) {
    return png;
  }
  // 96 pixels to the inch (3780 per metre), as the model takes a HEIF file
  // that states none; Insert › Image Reference gives the model that pixel
  // size as well.
  decoded.image.setDotsPerMeterX(3780);
  decoded.image.setDotsPerMeterY(3780);
  QBuffer buffer(&png.png);
  if (!buffer.open(QIODevice::WriteOnly)) {
    png.result = HeifDecoded::Result::Failed;
    png.error = QStringLiteral("there is no memory for the PNG file");
    return png;
  }
  QImageWriter writer(&buffer, "png");
  if (!writer.write(decoded.image)) {
    png.result = HeifDecoded::Result::Failed;
    png.error = QStringLiteral("the image cannot be written as PNG: %1").arg(writer.errorString());
    png.png.clear();
    return png;
  }
  png.width = decoded.image.width();
  png.height = decoded.image.height();
  return png;
}

} // namespace mitcad
