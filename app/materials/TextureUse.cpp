// SPDX-License-Identifier: MIT
#include "TextureUse.hpp"

#include <cmath>

#include <QCryptographicHash>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QImage>
#include <QImageReader>
#include <QObject>

namespace mitcad {
namespace {

double toLinear(double c) { return c <= 0.04045 ? c / 12.92 : std::pow((c + 0.055) / 1.055, 2.4); }
double toSrgb(double c) { return c <= 0.0031308 ? c * 12.92 : 1.055 * std::pow(c, 1.0 / 2.4) - 0.055; }

} // namespace

QColor averageImageColor(const QString& file) {
  QImageReader reader(file);
  reader.setAutoTransform(true);
  const QSize full = reader.size();
  if (full.isValid() && !full.isEmpty()) {
    reader.setScaledSize(full.scaled(64, 64, Qt::KeepAspectRatio).expandedTo(QSize(1, 1)));
  }
  const QImage image = reader.read().convertToFormat(QImage::Format_RGB32);
  if (image.isNull()) {
    return QColor();
  }
  // Averaged in linear light, as the surface mixes it.
  double sum[3] = {0.0, 0.0, 0.0};
  for (int y = 0; y < image.height(); ++y) {
    const QRgb* row = reinterpret_cast<const QRgb*>(image.constScanLine(y));
    for (int x = 0; x < image.width(); ++x) {
      sum[0] += toLinear(qRed(row[x]) / 255.0);
      sum[1] += toLinear(qGreen(row[x]) / 255.0);
      sum[2] += toLinear(qBlue(row[x]) / 255.0);
    }
  }
  const double n = double(image.width()) * double(image.height());
  return QColor::fromRgbF(float(toSrgb(sum[0] / n)), float(toSrgb(sum[1] / n)), float(toSrgb(sum[2] / n)));
}

QSet<QString> embeddedImageDigests(const QJsonArray& appearances) {
  QSet<QString> digests;
  const auto add = [&digests](const QJsonObject& image) {
    const QString digest = image.value(QStringLiteral("image_sha256")).toString();
    if (image.value(QStringLiteral("embedded")).toBool() && !digest.isEmpty()) {
      digests.insert(digest);
    }
  };
  for (const QJsonValue& value : appearances) {
    const QJsonObject texture = value.toObject().value(QStringLiteral("texture")).toObject();
    add(texture);
    for (const QString& map : textureSetMapNames()) {
      add(texture.value(map).toObject());
    }
  }
  return digests;
}

TextureUse textureUseCommand(const QString& sourceId, const TextureAsset& asset, const TextureFiles& files,
                             const QString& name, const QSet<QString>& embedded, qint64 maxBytes) {
  TextureUse use;
  const QString base = files.maps.value(QStringLiteral("base_color"));
  if (base.isEmpty()) {
    use.error = QObject::tr("%1 has no base color image.").arg(asset.name);
    return use;
  }
  QJsonObject texture;
  for (const QString& map : textureSetMapNames()) {
    const QString path = files.maps.value(map);
    if (path.isEmpty()) {
      continue;
    }
    QFile file(path);
    if (!file.open(QIODevice::ReadOnly)) {
      use.error = QObject::tr("The image %1 cannot be read.").arg(QDir::toNativeSeparators(path));
      return use;
    }
    if (file.size() > maxBytes) {
      use.error = QObject::tr("The image %1 is larger than %2: choose a smaller resolution.")
                      .arg(QDir::toNativeSeparators(path), textureBytesText(maxBytes));
      return use;
    }
    const QByteArray bytes = file.readAll();
    if (!bytes.startsWith("\x89PNG") && !bytes.startsWith("\xFF\xD8\xFF")) {
      use.error = QObject::tr("The image %1 is no PNG or JPEG file.").arg(QDir::toNativeSeparators(path));
      return use;
    }
    const QString digest =
        QString::fromLatin1(QCryptographicHash::hash(bytes, QCryptographicHash::Sha256).toHex());
    QJsonObject image{{QStringLiteral("path"), QFileInfo(path).fileName()}};
    if (embedded.contains(digest)) {
      image.insert(QStringLiteral("embedded"), true);
      image.insert(QStringLiteral("image_sha256"), digest);
      ++use.shared;
    } else {
      image.insert(QStringLiteral("data"), QString::fromLatin1(bytes.toBase64()));
      ++use.sent;
    }
    if (map == QLatin1String("base_color")) {
      for (auto it = image.constBegin(); it != image.constEnd(); ++it) {
        texture.insert(it.key(), it.value());
      }
    } else {
      texture.insert(map, image);
    }
  }
  QSizeF size = asset.size;
  if (!size.isValid() || size.isEmpty()) {
    const QSize pixels = QImageReader(base).size();
    size = QSizeF(100.0, pixels.isValid() && pixels.width() > 0 ? 100.0 * pixels.height() / pixels.width() : 100.0);
  }
  texture.insert(QStringLiteral("size"), QJsonArray{size.width(), size.height()});
  QJsonObject source{{QStringLiteral("id"), sourceId}, {QStringLiteral("asset"), asset.recorded().trimmed()}};
  if (!files.resolution.isEmpty()) {
    source.insert(QStringLiteral("resolution"), files.resolution);
  }
  if (!asset.license.isEmpty()) {
    source.insert(QStringLiteral("license"), asset.license);
  }
  if (asset.url.startsWith(QLatin1String("https://")) || asset.url.startsWith(QLatin1String("http://"))) {
    source.insert(QStringLiteral("url"), asset.url);
  }
  texture.insert(QStringLiteral("source"), source);
  use.command = {{QStringLiteral("cmd"), QStringLiteral("create_appearance")},
                 {QStringLiteral("name"), name},
                 {QStringLiteral("texture"), texture}};
  const QColor average = averageImageColor(base);
  if (average.isValid()) {
    use.command.insert(QStringLiteral("base_color"),
                       QJsonArray{average.redF(), average.greenF(), average.blueF()});
  }
  return use;
}

} // namespace mitcad
