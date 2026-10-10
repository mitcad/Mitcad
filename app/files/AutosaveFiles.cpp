// SPDX-License-Identifier: MIT
#include "Autosave.hpp"

#include <QDir>
#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QSaveFile>
#include <QStringList>
#include <QUuid>

namespace mitcad {
namespace {

bool validId(const QString& id) {
  const QUuid uuid = QUuid::fromString(id);
  return !uuid.isNull() && uuid.toString(QUuid::WithoutBraces) == id;
}

bool writeAtomically(const QString& path, const QByteArray& data, QString& error) {
  QSaveFile file(path);
  if (!file.open(QIODevice::WriteOnly) || file.write(data) != data.size() || !file.commit()) {
    error = QStringLiteral("%1: %2").arg(QDir::toNativeSeparators(path), file.errorString());
    return false;
  }
  return true;
}

// Remove only owned snapshot/metadata names, including QSaveFile suffixes.
// A filename from metadata is never passed to cleanup.
void removeFiles(const QString& directory, const QString& session) {
  if (!validId(session)) {
    return;
  }
  const QDir dir(directory);
  for (const QString& name : dir.entryList({session + QLatin1String(".*")}, QDir::Files | QDir::Hidden)) {
    const QStringList parts = name.split(QLatin1Char('.'));
    const bool metadata = parts.size() >= 2 && parts[1] == QLatin1String("json");
    const bool legacy = parts.size() >= 2 && parts[1] == QLatin1String("mitcad");
    const bool snapshot = parts.size() >= 3 && validId(parts[1]) && parts[2] == QLatin1String("mitcad");
    if (metadata || legacy || snapshot) {
      QFile::remove(dir.filePath(name));
    }
  }
}

} // namespace

QString fileDigest(QByteArrayView data) {
  quint64 hash = 0xcbf29ce484222325ULL;
  for (const char byte : data) {
    hash ^= static_cast<unsigned char>(byte);
    hash *= 0x100000001b3ULL;
  }
  return QStringLiteral("%1").arg(hash, 16, 16, QLatin1Char('0'));
}

bool validAutosaveProject(const QString& session, const QString& project, int version) {
  if (!validId(session)) {
    return false;
  }
  if (version == 1) {
    return project == session + QLatin1String(".mitcad");
  }
  const QStringList parts = project.split(QLatin1Char('.'));
  return version == 2 && parts.size() == 3 && parts[0] == session && validId(parts[1]) &&
         parts[2] == QLatin1String("mitcad");
}

bool publishAutosave(const QString& directory, const QString& session, const QByteArray& data,
                     QJsonObject about, QString& error, const AutosaveWriter& writer) {
  if (!validId(session)) {
    error = QStringLiteral("invalid autosave session");
    return false;
  }
  const QDir dir(directory);
  const QString project = session + QLatin1Char('.') + QUuid::createUuid().toString(QUuid::WithoutBraces) +
                          QLatin1String(".mitcad");
  const AutosaveWriter write = writer ? writer : AutosaveWriter(writeAtomically);
  if (!write(dir.filePath(project), data, error)) {
    return false;
  }
  about.insert(QStringLiteral("format"), QStringLiteral("mitcad-autosave"));
  about.insert(QStringLiteral("session"), session);
  about.insert(QStringLiteral("version"), 2);
  about.insert(QStringLiteral("project"), project);
  about.insert(QStringLiteral("size"), static_cast<qint64>(data.size()));
  about.insert(QStringLiteral("digest"), fileDigest(data));
  if (!write(dir.filePath(session + QLatin1String(".json")), QJsonDocument(about).toJson(), error)) {
    // Also safe after interruption here: the old metadata still names the
    // old immutable snapshot. A later successful publication cleans orphans.
    return false;
  }
  // Publication succeeded; remove obsolete snapshots without removing the
  // metadata pointer we have just committed.
  for (const QString& name : dir.entryList({session + QLatin1String("*.mitcad*")}, QDir::Files | QDir::Hidden)) {
    const QStringList parts = name.split(QLatin1Char('.'));
    const bool owned = (parts.size() >= 2 && parts[0] == session && parts[1] == QLatin1String("mitcad")) ||
                       (parts.size() >= 3 && parts[0] == session && validId(parts[1]) &&
                        parts[2] == QLatin1String("mitcad"));
    if (owned && name != project) {
      QFile::remove(dir.filePath(name));
    }
  }
  return true;
}

void removeSessionFiles(const QString& directory, const QString& session) {
  if (!validId(session)) {
    return;
  }
  QFile::remove(QDir(directory).filePath(session + QLatin1String(".json")));
  removeFiles(directory, session);
}

} // namespace mitcad
