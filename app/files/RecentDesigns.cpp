// SPDX-License-Identifier: MIT
#include "RecentDesigns.hpp"

#include <utility>

#include <QCoreApplication>
#include <QCryptographicHash>
#include <QDir>
#include <QFileInfo>
#include <QLocale>
#include <QSettings>
#include <QStandardPaths>
#include <QVariantMap>

namespace mitcad::recent {
namespace {

QString absolute(const QString& path) { return QFileInfo(path).absoluteFilePath(); }

QVariantMap openedTimes() { return QSettings().value(openedKey()).toMap(); }

QStringList pinnedPaths() { return QSettings().value(pinnedKey()).toStringList(); }

} // namespace

QString recentFilesKey() { return QStringLiteral("recentFiles"); }
QString openedKey() { return QStringLiteral("start/recentOpened"); }
QString pinnedKey() { return QStringLiteral("start/pinnedDesigns"); }
QString showAtLaunchKey() { return QStringLiteral("start/showAtLaunch"); }

void noteOpened(const QString& path, const QDateTime& when) {
  QSettings settings;
  QStringList files = settings.value(recentFilesKey()).toStringList();
  const QString file = absolute(path);
  files.removeAll(file);
  files.prepend(file);
  while (files.size() > kRecentCount) {
    files.removeLast();
  }
  settings.setValue(recentFilesKey(), files);
  // The times of the files in the list and of the pinned ones only.
  const QStringList pinned = pinnedPaths();
  QVariantMap times = openedTimes();
  times.insert(file, when.toString(Qt::ISODate));
  for (auto it = times.begin(); it != times.end();) {
    if (!files.contains(it.key()) && !pinned.contains(it.key())) {
      it = times.erase(it);
    } else {
      ++it;
    }
  }
  settings.setValue(openedKey(), times);
}

QList<RecentDesign> load() {
  const QSettings settings;
  const QStringList files = settings.value(recentFilesKey()).toStringList();
  const QStringList pinned = settings.value(pinnedKey()).toStringList();
  const QVariantMap times = settings.value(openedKey()).toMap();
  QStringList order = files;
  for (const QString& path : pinned) {
    if (!order.contains(path)) {
      order.append(path);
    }
  }
  QList<RecentDesign> designs;
  for (const QString& path : std::as_const(order)) {
    if (path.isEmpty()) {
      continue;
    }
    const QFileInfo info(path);
    RecentDesign design;
    design.path = path;
    design.name = info.fileName();
    design.type = typeText(path);
    design.folder = folderText(path);
    design.opened = QDateTime::fromString(times.value(path).toString(), Qt::ISODate);
    // An earlier list without times: the file's own.
    if (!design.opened.isValid() && info.exists()) {
      design.opened = info.lastModified();
    }
    design.pinned = pinned.contains(path);
    design.missing = !info.isFile();
    const QString preview = designPreviewPath(path);
    if (!design.missing && !preview.isEmpty() && QFileInfo(preview).isFile()) {
      design.preview = preview;
    }
    designs.append(design);
  }
  return designs;
}

void setPinned(const QString& path, bool pinned) {
  QSettings settings;
  QStringList paths = settings.value(pinnedKey()).toStringList();
  const QString file = absolute(path);
  paths.removeAll(file);
  if (pinned) {
    paths.append(file);
  }
  settings.setValue(pinnedKey(), paths);
}

void remove(const QString& path) {
  QSettings settings;
  const QString file = absolute(path);
  QStringList files = settings.value(recentFilesKey()).toStringList();
  files.removeAll(file);
  settings.setValue(recentFilesKey(), files);
  QStringList pinned = settings.value(pinnedKey()).toStringList();
  pinned.removeAll(file);
  settings.setValue(pinnedKey(), pinned);
  QVariantMap times = settings.value(openedKey()).toMap();
  times.remove(file);
  settings.setValue(openedKey(), times);
}

void clearRecent() {
  QSettings settings;
  settings.remove(recentFilesKey());
  // The pinned designs keep their times.
  const QStringList pinned = settings.value(pinnedKey()).toStringList();
  QVariantMap times = settings.value(openedKey()).toMap();
  for (auto it = times.begin(); it != times.end();) {
    if (!pinned.contains(it.key())) {
      it = times.erase(it);
    } else {
      ++it;
    }
  }
  settings.setValue(openedKey(), times);
}

QList<RecentDesign> filter(const QList<RecentDesign>& designs, bool pinnedOnly, const QString& search) {
  const QString words = search.trimmed();
  QList<RecentDesign> shown;
  for (const RecentDesign& design : designs) {
    if (pinnedOnly && !design.pinned) {
      continue;
    }
    if (!words.isEmpty() && !design.name.contains(words, Qt::CaseInsensitive) &&
        !design.folder.contains(words, Qt::CaseInsensitive)) {
      continue;
    }
    shown.append(design);
  }
  return shown;
}

QString openedText(const QDateTime& opened, const QDateTime& now) {
  if (!opened.isValid()) {
    return QString();
  }
  const qint64 days = opened.date().daysTo(now.date());
  if (days <= 0) {
    return QCoreApplication::translate("RecentDesigns", "Today");
  }
  if (days == 1) {
    return QCoreApplication::translate("RecentDesigns", "Yesterday");
  }
  if (days < 7) {
    return QCoreApplication::translate("RecentDesigns", "%1 days ago").arg(days);
  }
  return QLocale().toString(opened.date(), QLocale::ShortFormat);
}

QString folderText(const QString& path) {
  const QStringList parts = QDir::fromNativeSeparators(QFileInfo(path).absolutePath()).split(QLatin1Char('/'), Qt::SkipEmptyParts);
  if (parts.isEmpty()) {
    return QDir::toNativeSeparators(QFileInfo(path).absolutePath());
  }
  if (parts.size() == 1) {
    return parts.constFirst();
  }
  return parts.at(parts.size() - 2) + QStringLiteral(" / ") + parts.constLast();
}

QString typeText(const QString& path) {
  const QString suffix = QFileInfo(path).suffix();
  return suffix.isEmpty() ? QStringLiteral("FILE") : suffix.toUpper();
}

QString designPreviewPath(const QString& path) {
  QString folder = qEnvironmentVariable("MITCAD_THUMBNAIL_DIR"); // tests
  if (folder.isEmpty()) {
    const QString data = QStandardPaths::writableLocation(QStandardPaths::AppLocalDataLocation);
    if (!data.isEmpty()) {
      folder = QDir(data).filePath(QStringLiteral("thumbnails"));
    }
  }
  if (folder.isEmpty() || path.isEmpty()) {
    return {};
  }
  // By the absolute path: a design moved elsewhere gets a preview of its
  // own once it is saved there.
  const QString key = QDir::fromNativeSeparators(absolute(path));
  const QByteArray hash = QCryptographicHash::hash(key.toUtf8(), QCryptographicHash::Sha1).toHex();
  return QDir(folder).filePath(QStringLiteral("recent/%1.png").arg(QString::fromLatin1(hash)));
}

bool showAtLaunch() { return QSettings().value(showAtLaunchKey(), true).toBool(); }

void setShowAtLaunch(bool shown) { QSettings().setValue(showAtLaunchKey(), shown); }

} // namespace mitcad::recent
