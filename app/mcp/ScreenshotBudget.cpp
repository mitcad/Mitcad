// SPDX-License-Identifier: MIT
#include "mcp/ScreenshotBudget.hpp"

#include <algorithm>
#include <cmath>

#include <QBuffer>
#include <QIODevice>

namespace mitcad::mcp {

QByteArray encodeImage(const QImage& image, bool jpeg, int quality) {
  QByteArray bytes;
  QBuffer buffer(&bytes);
  buffer.open(QIODevice::WriteOnly);
  const bool saved =
      jpeg ? image.convertToFormat(QImage::Format_RGB32).save(&buffer, "JPEG", quality) : image.save(&buffer, "PNG");
  return saved ? bytes : QByteArray();
}

FittedImage fitImage(const QImage& image, int longest, qsizetype budget, const ImageEncoder& encode) {
  FittedImage fitted;
  int limit = std::max(1, longest);
  for (int attempt = 0; attempt < kScreenshotAttempts; ++attempt) {
    QImage scaled = image;
    if (scaled.width() > limit || scaled.height() > limit) {
      // From the original each time, so that the steps add no blur.
      scaled = image.scaled(limit, limit, Qt::KeepAspectRatio, Qt::SmoothTransformation);
    }
    fitted.bytes = encode(scaled);
    fitted.image = scaled;
    fitted.downscaled = attempt > 0;
    ++fitted.attempts;
    if (fitted.bytes.isEmpty()) {
      return fitted;
    }
    if (fitted.bytes.size() <= budget) {
      fitted.fits = true;
      return fitted;
    }
    const int side = std::max(scaled.width(), scaled.height());
    if (side <= kScreenshotMinSide) {
      break;
    }
    // The bytes go about with the area: aim a little under the budget,
    // between a quarter and four fifths of the side at a time.
    const double ratio =
        std::clamp(std::sqrt(static_cast<double>(budget) / static_cast<double>(fitted.bytes.size())) * 0.9, 0.25, 0.8);
    limit = std::max(kScreenshotMinSide, static_cast<int>(side * ratio));
  }
  return fitted;
}

} // namespace mitcad::mcp
