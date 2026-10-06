// SPDX-License-Identifier: MIT
#include "ResultCache.hpp"

#include <algorithm>
#include <cstddef>
#include <cstdint>
#include <string>

#include <QByteArray>
#include <QDir>
#include <QSettings>
#include <QStandardPaths>
#include <QThreadPool>
#include <QtLogging>

#include "Json.hpp"
#include "mitcad/geometry/persist.hpp"
#include "mitcad_bridge/lib.h"

namespace mitcad {
namespace {

const QString kDisk = QStringLiteral("cache/disk");
const QString kDiskMegabytes = QStringLiteral("cache/diskMegabytes");
const QString kMemoryMegabytes = QStringLiteral("cache/memoryMegabytes");

constexpr double kDefaultMinMs = 100.0;
// When the system does not tell its memory.
constexpr qint64 kUnknownMemoryMegabytes = 8 * 1024;

} // namespace

qint64 CacheSettings::physicalMegabytes() {
  const std::size_t bytes = geometry::physical_memory();
  return bytes > 0 ? static_cast<qint64>(bytes / (1024 * 1024)) : kUnknownMemoryMegabytes;
}

qint64 CacheSettings::defaultMemoryMegabytes() {
  return std::max<qint64>(kMinMemoryMegabytes, physicalMegabytes() / 4);
}

CacheSettings CacheSettings::load() {
  const QSettings settings;
  CacheSettings cache;
  cache.disk = settings.value(kDisk, cache.disk).toBool();
  cache.diskMegabytes = std::clamp<qint64>(settings.value(kDiskMegabytes, cache.diskMegabytes).toLongLong(),
                                           qint64(kMinDiskGigabytes) * 1024, qint64(kMaxDiskGigabytes) * 1024);
  cache.memoryMegabytes =
      std::clamp<qint64>(settings.value(kMemoryMegabytes, cache.memoryMegabytes).toLongLong(), kMinMemoryMegabytes,
                         std::max<qint64>(kMinMemoryMegabytes, physicalMegabytes()));
  return cache;
}

void CacheSettings::save() const {
  QSettings settings;
  settings.setValue(kDisk, disk);
  settings.setValue(kDiskMegabytes, diskMegabytes);
  settings.setValue(kMemoryMegabytes, memoryMegabytes);
}

QString resultStoreLocation() {
  const QString given = qEnvironmentVariable("MITCAD_RESULT_STORE");
  if (given == QLatin1String("off")) {
    return QString();
  }
  if (!given.isEmpty()) {
    return QDir::cleanPath(QDir(given).absolutePath());
  }
  const QString cache = QStandardPaths::writableLocation(QStandardPaths::CacheLocation);
  // Never relative to the working directory, which may be a project's.
  return cache.isEmpty() ? QString() : QDir(cache).filePath(QStringLiteral("results"));
}

QString resultStoreDirectory() {
  return CacheSettings::load().disk ? resultStoreLocation() : QString();
}

QString resultStoreBuildId() {
  // The program's file does not change while it runs.
  static const QString id = QString::fromStdString(geometry::kernel_build_id());
  return id;
}

double resultStoreMinMs() {
  bool ok = false;
  const double given = qEnvironmentVariable("MITCAD_RESULT_STORE_MIN_MS").toDouble(&ok);
  return ok && given >= 0.0 ? given : kDefaultMinMs;
}

void collectResultStoreGarbage() {
  const QString dir = resultStoreDirectory();
  if (dir.isEmpty()) {
    return;
  }
  const QByteArray path = dir.toUtf8();
  const auto budget = static_cast<std::uint64_t>(CacheSettings::load().diskMegabytes);
  QThreadPool::globalInstance()->start([path, budget] {
    const QJsonObject report = parseObject(result_store_gc(rustStr(path), budget));
    const qint64 removed = report.value(QStringLiteral("removed")).toInteger();
    if (removed > 0) {
      qInfo().noquote() << QStringLiteral("Result store: removed %1 old results (%2 MB), %3 MB kept")
                               .arg(removed)
                               .arg(report.value(QStringLiteral("removed_bytes")).toDouble() / 1048576.0, 0, 'f', 1)
                               .arg(report.value(QStringLiteral("bytes")).toDouble() / 1048576.0, 0, 'f', 1);
    }
  });
}

} // namespace mitcad
