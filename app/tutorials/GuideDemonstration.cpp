// SPDX-License-Identifier: MIT
// An input demonstration of a guide step (mitcad#2366): see
// GuideDemonstration.hpp.
#include "GuideDemonstration.hpp"

#include <algorithm>
#include <cmath>

#include <QHash>
#include <QJsonArray>
#include <QPainter>
#include <QStringList>
#include <QSvgRenderer>
#include <QUrl>

namespace mitcad {
namespace {

constexpr int kMaxFrames = 64;
constexpr qsizetype kMaxImageBytes = 2 * 1024 * 1024;
constexpr double kMaxSide = 1024;
constexpr int kMaxFrameMs = 20000;
constexpr int kMaxRepeat = 100;
constexpr int kMaxPauseMs = 60000;
constexpr int kMaxLabel = 200;
constexpr double kMaxOffset = 4000;

// A pair of numbers ([x, y]) within [low, high]; false when it is not one.
bool pair(const QJsonValue& value, double low, double high, QPointF* out) {
  const QJsonArray array = value.toArray();
  if (!value.isArray() || array.size() != 2 || !array.at(0).isDouble() || !array.at(1).isDouble()) {
    return false;
  }
  const double x = array.at(0).toDouble();
  const double y = array.at(1).toDouble();
  if (!std::isfinite(x) || !std::isfinite(y) || x < low || x > high || y < low || y > high) {
    return false;
  }
  *out = QPointF(x, y);
  return true;
}

// A whole number within [low, high]; false when it is not one.
bool whole(const QJsonValue& value, int low, int high, int* out) {
  if (!value.isDouble()) {
    return false;
  }
  const double number = value.toDouble();
  if (!std::isfinite(number) || number != std::floor(number) || number < low || number > high) {
    return false;
  }
  *out = static_cast<int>(number);
  return true;
}

bool unknownKey(const QJsonObject& json, const QStringList& known, const QString& what, QString* error) {
  for (auto it = json.begin(); it != json.end(); ++it) {
    if (!known.contains(it.key())) {
      *error = QStringLiteral("%1 has no field \"%2\"").arg(what, it.key());
      return true;
    }
  }
  return false;
}

bool looksLikeSvg(const QByteArray& bytes) {
  return bytes.left(1024).contains("<svg");
}

} // namespace

bool guideDataUrl(const QString& address, QByteArray* bytes, QString* mimeType, QString* error) {
  if (!address.startsWith(QLatin1String("data:"), Qt::CaseInsensitive)) {
    *error = QStringLiteral("not a data: address");
    return false;
  }
  const qsizetype comma = address.indexOf(QLatin1Char(','));
  if (comma < 0) {
    *error = QStringLiteral("a data: address without a comma");
    return false;
  }
  const QStringList header = address.mid(5, comma - 5).split(QLatin1Char(';'));
  *mimeType = header.value(0).trimmed().toLower();
  const bool base64 =
      header.size() > 1 && header.last().trimmed().compare(QLatin1String("base64"), Qt::CaseInsensitive) == 0;
  const QByteArray data = address.mid(comma + 1).toUtf8();
  if (base64) {
    const auto decoded = QByteArray::fromBase64Encoding(data, QByteArray::AbortOnBase64DecodingErrors);
    if (!decoded) {
      *error = QStringLiteral("a data: address that is not base64");
      return false;
    }
    *bytes = decoded.decoded;
  } else {
    *bytes = QByteArray::fromPercentEncoding(data);
  }
  return true;
}

QImage guideDemonstrationImage(const QByteArray& bytes, bool svg, const QSizeF& size, qreal pixelRatio,
                               QSizeF* drawnSize, QString* error) {
  const qreal ratio = pixelRatio > 0 ? pixelRatio : 1.0;
  QSizeF logical = size;
  QImage image;
  if (svg || looksLikeSvg(bytes)) {
    QSvgRenderer renderer(bytes);
    if (!renderer.isValid()) {
      *error = QStringLiteral("the SVG picture could not be read");
      return QImage();
    }
    if (logical.isEmpty()) {
      logical = QSizeF(renderer.defaultSize());
      if (logical.isEmpty()) {
        logical = renderer.viewBoxF().size();
      }
      if (logical.isEmpty()) {
        logical = QSizeF(32, 32);
      }
    }
  } else {
    image = QImage::fromData(bytes);
    if (image.isNull()) {
      *error = QStringLiteral("the picture could not be read (SVG, PNG or JPEG)");
      return QImage();
    }
    if (logical.isEmpty()) {
      logical = QSizeF(image.size());
    }
  }
  // At most kMaxSide logical pixels a side, in proportion.
  const double scale = std::min(1.0, kMaxSide / std::max(logical.width(), logical.height()));
  logical *= scale;
  const QSize pixels = (logical * ratio).toSize().expandedTo(QSize(1, 1));
  if (image.isNull()) {
    QSvgRenderer renderer(bytes);
    image = QImage(pixels, QImage::Format_ARGB32_Premultiplied);
    image.fill(Qt::transparent);
    QPainter painter(&image);
    painter.setRenderHint(QPainter::Antialiasing, true);
    renderer.render(&painter);
    painter.end();
  } else {
    image = image.convertToFormat(QImage::Format_ARGB32_Premultiplied)
                .scaled(pixels, Qt::IgnoreAspectRatio, Qt::SmoothTransformation);
  }
  image.setDevicePixelRatio(ratio);
  *drawnSize = logical;
  return image;
}

GuideDemonstration GuideDemonstration::parse(const QJsonObject& json, const Loader& load, qreal pixelRatio,
                                             QString* error) {
  GuideDemonstration result;
  const auto refuse = [&result, error](const QString& reason) {
    *error = reason;
    result.frames.clear();
    return result;
  };
  if (unknownKey(json, {QStringLiteral("frames"), QStringLiteral("repeat"), QStringLiteral("pause"),
                        QStringLiteral("label")},
                 QStringLiteral("A demonstration"), error)) {
    return refuse(*error);
  }
  if (json.contains(QStringLiteral("repeat")) &&
      !whole(json.value(QStringLiteral("repeat")), 0, kMaxRepeat, &result.repeat)) {
    return refuse(QStringLiteral("\"repeat\" is a whole number from 0 (until stopped) to %1").arg(kMaxRepeat));
  }
  if (json.contains(QStringLiteral("pause")) &&
      !whole(json.value(QStringLiteral("pause")), 0, kMaxPauseMs, &result.pauseMs)) {
    return refuse(QStringLiteral("\"pause\" is a whole number of ms from 0 to %1").arg(kMaxPauseMs));
  }
  if (json.contains(QStringLiteral("label"))) {
    if (!json.value(QStringLiteral("label")).isString()) {
      return refuse(QStringLiteral("\"label\" is a text"));
    }
    result.label = json.value(QStringLiteral("label")).toString().simplified();
    if (result.label.size() > kMaxLabel) {
      return refuse(QStringLiteral("\"label\" is at most %1 characters").arg(kMaxLabel));
    }
  }
  const QJsonValue framesValue = json.value(QStringLiteral("frames"));
  const QJsonArray frames = framesValue.toArray();
  if (!framesValue.isArray() || frames.isEmpty() || frames.size() > kMaxFrames) {
    return refuse(QStringLiteral("\"frames\" is a list of 1 to %1 frames").arg(kMaxFrames));
  }
  // Each picture read once per size.
  QHash<QString, std::pair<QImage, QSizeF>> pictures;
  for (qsizetype i = 0; i < frames.size(); ++i) {
    const QString what = QStringLiteral("Frame %1").arg(i + 1);
    if (!frames.at(i).isObject()) {
      return refuse(QStringLiteral("%1 is not an object").arg(what));
    }
    const QJsonObject object = frames.at(i).toObject();
    if (unknownKey(object, {QStringLiteral("image"), QStringLiteral("size"), QStringLiteral("at"),
                            QStringLiteral("offset"), QStringLiteral("anchor"), QStringLiteral("ms"),
                            QStringLiteral("move")},
                   what, error)) {
      return refuse(*error);
    }
    GuideDemonstrationFrame frame;
    QSizeF wanted;
    if (object.contains(QStringLiteral("size"))) {
      QPointF size;
      if (!pair(object.value(QStringLiteral("size")), 1, kMaxSide, &size)) {
        return refuse(QStringLiteral("%1: \"size\" is [width, height], 1 to %2 pixels").arg(what).arg(kMaxSide));
      }
      wanted = QSizeF(size.x(), size.y());
    }
    if (object.contains(QStringLiteral("at")) && !pair(object.value(QStringLiteral("at")), -2, 3, &frame.at)) {
      return refuse(QStringLiteral("%1: \"at\" is [x, y], parts of the target's width and height").arg(what));
    }
    if (object.contains(QStringLiteral("offset")) &&
        !pair(object.value(QStringLiteral("offset")), -kMaxOffset, kMaxOffset, &frame.offset)) {
      return refuse(QStringLiteral("%1: \"offset\" is [x, y] in pixels").arg(what));
    }
    if (object.contains(QStringLiteral("anchor")) &&
        !pair(object.value(QStringLiteral("anchor")), 0, 1, &frame.anchor)) {
      return refuse(QStringLiteral("%1: \"anchor\" is [x, y], 0 to 1 of the picture's size").arg(what));
    }
    if (object.contains(QStringLiteral("ms")) &&
        !whole(object.value(QStringLiteral("ms")), 1, kMaxFrameMs, &frame.ms)) {
      return refuse(QStringLiteral("%1: \"ms\" is a whole number from 1 to %2").arg(what).arg(kMaxFrameMs));
    }
    if (object.contains(QStringLiteral("move"))) {
      if (!object.value(QStringLiteral("move")).isBool()) {
        return refuse(QStringLiteral("%1: \"move\" is true or false").arg(what));
      }
      frame.move = object.value(QStringLiteral("move")).toBool();
    }
    const QString source = object.value(QStringLiteral("image")).toString();
    if (source.isEmpty()) {
      return refuse(QStringLiteral("%1 has no \"image\"").arg(what));
    }
    const QString key = QStringLiteral("%1x%2 %3").arg(wanted.width()).arg(wanted.height()).arg(source);
    auto known = pictures.constFind(key);
    if (known == pictures.constEnd()) {
      QByteArray bytes;
      QString reason;
      bool svg = false;
      if (source.startsWith(QLatin1String("data:"), Qt::CaseInsensitive)) {
        QString mime;
        if (!guideDataUrl(source, &bytes, &mime, &reason)) {
          return refuse(QStringLiteral("%1: %2").arg(what, reason));
        }
        svg = mime == QLatin1String("image/svg+xml");
      } else {
        if (!load || !load(source, &bytes, &reason)) {
          return refuse(QStringLiteral("%1: %2").arg(what, reason.isEmpty() ? QStringLiteral("no picture") : reason));
        }
        svg = source.endsWith(QLatin1String(".svg"), Qt::CaseInsensitive);
      }
      if (bytes.size() > kMaxImageBytes) {
        return refuse(QStringLiteral("%1: the picture is larger than 2 MB").arg(what));
      }
      QSizeF drawn;
      const QImage image = guideDemonstrationImage(bytes, svg, wanted, pixelRatio, &drawn, &reason);
      if (image.isNull()) {
        return refuse(QStringLiteral("%1: %2").arg(what, reason));
      }
      known = pictures.insert(key, {image, drawn});
    }
    frame.image = known->first;
    frame.size = known->second;
    result.frames.push_back(frame);
  }
  error->clear();
  return result;
}

int GuideDemonstration::cycleMs() const {
  int total = 0;
  for (const GuideDemonstrationFrame& frame : frames) {
    total += frame.ms;
  }
  return total;
}

QPointF GuideDemonstration::framePoint(int index, const QRectF& target) const {
  const GuideDemonstrationFrame& frame = frames.at(index);
  return target.topLeft() + QPointF(frame.at.x() * target.width(), frame.at.y() * target.height()) + frame.offset;
}

QRectF GuideDemonstration::frameRect(int index, const QPointF& point) const {
  const GuideDemonstrationFrame& frame = frames.at(index);
  return QRectF(point - QPointF(frame.anchor.x() * frame.size.width(), frame.anchor.y() * frame.size.height()),
                frame.size);
}

GuideDemonstration::Pose GuideDemonstration::poseAt(qint64 elapsed, const QRectF& target) const {
  Pose pose;
  const int cycle = cycleMs();
  if (frames.isEmpty() || cycle <= 0) {
    pose.ended = true;
    return pose;
  }
  elapsed = std::max<qint64>(elapsed, 0);
  const qint64 period = static_cast<qint64>(cycle) + pauseMs;
  // The pause after the last play is not waited for.
  if (repeat > 0 && elapsed >= static_cast<qint64>(repeat) * period - pauseMs) {
    pose.ended = true;
    return pose;
  }
  const qint64 t = elapsed % period;
  if (t >= cycle) {
    return pose; // between two plays
  }
  qint64 start = 0;
  for (int i = 0; i < frames.size(); ++i) {
    const GuideDemonstrationFrame& frame = frames.at(i);
    if (t < start + frame.ms) {
      pose.frame = i;
      pose.point = framePoint(i, target);
      if (frame.move && i > 0) {
        const QPointF from = framePoint(i - 1, target);
        const double f = static_cast<double>(t - start) / frame.ms;
        const double eased = f * f * (3 - 2 * f);
        pose.point = from + (pose.point - from) * eased;
      }
      return pose;
    }
    start += frame.ms;
  }
  return pose;
}

} // namespace mitcad
