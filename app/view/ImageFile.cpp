// SPDX-License-Identifier: MIT
// Image references' files decoded (view/ImageFile.hpp).
#include "view/ImageFile.hpp"

#include <QCoreApplication>
#include <QtGlobal>

#include "view/HeifDecoder.hpp"

namespace mitcad {

bool isHeifFile(const QByteArray& bytes) {
  if (bytes.size() < 16 || bytes.mid(4, 4) != QByteArrayLiteral("ftyp")) {
    return false;
  }
  const auto byte = [&bytes](int at) { return static_cast<quint32>(static_cast<unsigned char>(bytes.at(at))); };
  const quint32 size = (byte(0) << 24) | (byte(1) << 16) | (byte(2) << 8) | byte(3);
  const qsizetype end = qMin(bytes.size(), static_cast<qsizetype>(size));
  static const char* const kBrands[] = {"mif1", "mif2", "msf1", "miaf", "heic", "heix",
                                        "heim", "heis", "hevc", "hevx", "avif", "avis"};
  // The major brand, the minor version (skipped), the compatible brands.
  for (qsizetype at = 8; at + 4 <= end; at += 4) {
    if (at == 12) {
      continue;
    }
    const QByteArray brand = bytes.mid(at, 4);
    for (const char* known : kBrands) {
      if (brand == known) {
        return true;
      }
    }
  }
  return false;
}

bool heifDecoderBuilt() {
#if defined(MITCAD_HEIF)
  return true;
#else
  return false;
#endif
}

QString imageFileFilter() {
  return QCoreApplication::translate("ImageFile",
                                     "Images (*.png *.jpg *.jpeg *.bmp *.webp *.heic *.heif);;All files (*)");
}

QImage decodeImageFile(const QByteArray& bytes, QString* error, bool* noDecoder) {
  QString why;
  QImage image;
  bool missing = false;
  if (isHeifFile(bytes)) {
    // The operating system's decoder (or the build's), looked for now.
    const HeifDecoded decoded = decodeHeif(bytes);
    image = decoded.image;
    missing = decoded.result == HeifDecoded::Result::NoDecoder;
    why = missing ? QStringLiteral("no HEIF decoder here (%1)").arg(decoded.error) : decoded.error;
  } else {
    image = QImage::fromData(bytes);
    if (image.isNull()) {
      why = QStringLiteral("Qt cannot read the image");
    }
  }
  if (error != nullptr) {
    *error = image.isNull() ? why : QString();
  }
  if (noDecoder != nullptr) {
    *noDecoder = image.isNull() && missing;
  }
  return image;
}

} // namespace mitcad
