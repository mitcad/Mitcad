// SPDX-License-Identifier: MIT
// HEIF images through a libheif loaded at run time (view/HeifLibrary.hpp).
#include "view/HeifLibrary.hpp"

#include <cstddef>
#include <cstdint>
#include <cstring>
#include <map>
#include <memory>

#include <QImageReader>
#include <QLibrary>
#include <QMutex>
#include <QMutexLocker>

namespace mitcad {
namespace {

// libheif's C interface as far as MitCad calls it (libheif/heif.h, whose
// enumerations are ints and whose objects are opaque): the values below
// are part of its stable ABI since libheif 1.x.
struct HeifError {
  int code;
  int subcode;
  const char* message;
};
constexpr int kOk = 0;
constexpr int kUnsupportedFeature = 4;   // heif_error_Unsupported_feature
constexpr int kPluginLoadingError = 11;  // heif_error_Plugin_loading_error
constexpr int kUnsupportedCodec = 3001;  // heif_suberror_Unsupported_codec
constexpr int kCompressionHevc = 1;      // heif_compression_HEVC
constexpr int kColorspaceRgb = 1;        // heif_colorspace_RGB
constexpr int kChromaRgb = 10;           // heif_chroma_interleaved_RGB
constexpr int kChromaRgba = 11;          // heif_chroma_interleaved_RGBA
constexpr int kChannelInterleaved = 10;  // heif_channel_interleaved

using Init = HeifError (*)(void*);
using ContextAlloc = void* (*)();
using ContextFree = void (*)(void*);
using ReadFromMemory = HeifError (*)(void*, const void*, std::size_t, const void*);
using PrimaryHandle = HeifError (*)(void*, void**);
using HandleRelease = void (*)(const void*);
using HandleInt = int (*)(const void*);
using Decode = HeifError (*)(const void*, void**, int, int, const void*);
using ImageRelease = void (*)(const void*);
using ImageInt = int (*)(const void*, int);
using Plane = const std::uint8_t* (*)(const void*, int, int*);
using HaveDecoder = int (*)(int);

// The functions of one loaded libheif, or (not `loaded`) why it is not
// there: no such library, or one without a function needed.
struct Library {
  bool loaded = false;
  QString error;
  ContextAlloc contextAlloc = nullptr;
  ContextFree contextFree = nullptr;
  ReadFromMemory readFromMemory = nullptr;
  PrimaryHandle primaryHandle = nullptr;
  HandleRelease handleRelease = nullptr;
  HandleInt handleWidth = nullptr;
  HandleInt handleHeight = nullptr;
  HandleInt handleHasAlpha = nullptr;
  Decode decode = nullptr;
  ImageRelease imageRelease = nullptr;
  ImageInt imageWidth = nullptr;
  ImageInt imageHeight = nullptr;
  ImageInt imageBits = nullptr;
  Plane plane = nullptr;
  // Optional: whether a decoder for a format is installed (libheif 1.x).
  HaveDecoder haveDecoder = nullptr;
};

template <typename Function>
bool resolve(QLibrary& library, const char* name, Function& function, QString& missing) {
  function = reinterpret_cast<Function>(library.resolve(name));
  if (function == nullptr && missing.isEmpty()) {
    missing = QString::fromLatin1(name);
  }
  return function != nullptr;
}

// Loaded once per name and kept: a library unloaded while its plugins are
// loaded is not safe, and the next HEIF file needs it again. One that is
// not there is looked for again with the next file (the user may have
// installed it meanwhile).
Library libraryNamed(const QString& name) {
  static QMutex mutex;
  static std::map<QString, Library> libraries;
  QMutexLocker lock(&mutex);
  const auto known = libraries.find(name);
  if (known != libraries.end()) {
    return known->second;
  }
  Library library;
  // QLibrary never unloads in its destructor: the handle stays open.
  QLibrary loader(name);
  if (!loader.load()) {
    library.error = loader.errorString();
    return library;
  }
  QString missing;
  resolve(loader, "heif_context_alloc", library.contextAlloc, missing);
  resolve(loader, "heif_context_free", library.contextFree, missing);
  resolve(loader, "heif_context_read_from_memory_without_copy", library.readFromMemory, missing);
  resolve(loader, "heif_context_get_primary_image_handle", library.primaryHandle, missing);
  resolve(loader, "heif_image_handle_release", library.handleRelease, missing);
  resolve(loader, "heif_image_handle_get_width", library.handleWidth, missing);
  resolve(loader, "heif_image_handle_get_height", library.handleHeight, missing);
  resolve(loader, "heif_image_handle_has_alpha_channel", library.handleHasAlpha, missing);
  resolve(loader, "heif_decode_image", library.decode, missing);
  resolve(loader, "heif_image_release", library.imageRelease, missing);
  resolve(loader, "heif_image_get_width", library.imageWidth, missing);
  resolve(loader, "heif_image_get_height", library.imageHeight, missing);
  resolve(loader, "heif_image_get_bits_per_pixel_range", library.imageBits, missing);
  resolve(loader, "heif_image_get_plane_readonly", library.plane, missing);
  if (!missing.isEmpty()) {
    library.error = QStringLiteral("%1 has no %2").arg(name, missing);
    return library;
  }
  // libheif 1.13 and later load their decoder plugins here; older ones
  // have no heif_init and their decoders built in.
  Init init = nullptr;
  QString optional;
  if (resolve(loader, "heif_init", init, optional)) {
    const HeifError started = init(nullptr);
    if (started.code != kOk) {
      library.error = QStringLiteral("heif_init failed: %1")
                          .arg(started.message != nullptr ? QString::fromUtf8(started.message) : QString());
      return library;
    }
  }
  resolve(loader, "heif_have_decoder_for_format", library.haveDecoder, optional);
  library.loaded = true;
  libraries.emplace(name, library);
  return library;
}

QString text(const HeifError& result) {
  return result.message != nullptr ? QString::fromUtf8(result.message) : QStringLiteral("unknown error");
}

// No decoder for the file's codec (libheif without its HEVC plugin), or a
// plugin that does not load: the user has something to install. Older
// libheif (1.17) reports the missing codec with other codes, so a library
// that says it has no HEVC decoder counts too.
bool noDecoder(const HeifError& result, const Library& heif) {
  return (result.code == kUnsupportedFeature && result.subcode == kUnsupportedCodec) ||
         result.code == kPluginLoadingError ||
         (heif.haveDecoder != nullptr && heif.haveDecoder(kCompressionHevc) == 0);
}

} // namespace

HeifDecoded decodeHeifWithLibrary(const QString& name, const QByteArray& bytes) {
  const Library heif = libraryNamed(name);
  if (!heif.loaded) {
    return HeifDecoded::missing(heif.error);
  }
  const std::unique_ptr<void, ContextFree> context(heif.contextAlloc(), heif.contextFree);
  if (!context) {
    return HeifDecoded::failed(QStringLiteral("libheif has no memory"));
  }
  HeifError result =
      heif.readFromMemory(context.get(), bytes.constData(), static_cast<std::size_t>(bytes.size()), nullptr);
  if (result.code != kOk) {
    return noDecoder(result, heif) ? HeifDecoded::missing(text(result))
                             : HeifDecoded::failed(QStringLiteral("the HEIF file cannot be read: %1").arg(text(result)));
  }
  void* handle = nullptr;
  result = heif.primaryHandle(context.get(), &handle);
  const std::unique_ptr<const void, HandleRelease> primary(handle, heif.handleRelease);
  if (result.code != kOk || !primary) {
    return HeifDecoded::failed(QStringLiteral("the HEIF file has no primary image: %1").arg(text(result)));
  }
  const int width = heif.handleWidth(handle);
  const int height = heif.handleHeight(handle);
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
  const bool alpha = heif.handleHasAlpha(handle) != 0;
  const int channels = alpha ? 4 : 3;
  void* decoded = nullptr;
  result = heif.decode(handle, &decoded, kColorspaceRgb, alpha ? kChromaRgba : kChromaRgb, nullptr);
  const std::unique_ptr<const void, ImageRelease> picture(decoded, heif.imageRelease);
  if (result.code != kOk || !picture) {
    return noDecoder(result, heif)
               ? HeifDecoded::missing(text(result))
               : HeifDecoded::failed(QStringLiteral("the HEIF image cannot be decoded: %1").arg(text(result)));
  }
  const int bits = heif.imageBits(decoded, kChannelInterleaved);
  if (bits != 8) {
    return HeifDecoded::failed(QStringLiteral("the HEIF image was decoded with %1 bits per channel, not 8").arg(bits));
  }
  int stride = 0;
  const std::uint8_t* plane = heif.plane(decoded, kChannelInterleaved, &stride);
  const int columns = heif.imageWidth(decoded, kChannelInterleaved);
  const int rows = heif.imageHeight(decoded, kChannelInterleaved);
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

} // namespace mitcad
