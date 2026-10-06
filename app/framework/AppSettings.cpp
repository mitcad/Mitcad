// SPDX-License-Identifier: MIT
#include "AppSettings.hpp"

#include <algorithm>
#include <optional>

#include <QByteArray>
#include <QDir>
#include <QSettings>
#include <QStandardPaths>
#include <QtGlobal>

namespace mitcad {
namespace {

// Not in a group "general": Qt's INI files keep their top-level keys in a
// section [General], and would write that group as [%General].
const QString kAutosave = QStringLiteral("autosave/enabled");
const QString kAutosaveMinutes = QStringLiteral("autosave/minutes");

} // namespace

GeneralSettings GeneralSettings::load() {
  const QSettings settings;
  GeneralSettings general;
  general.autosave = settings.value(kAutosave, general.autosave).toBool();
  general.autosaveMinutes = std::clamp(settings.value(kAutosaveMinutes, general.autosaveMinutes).toInt(),
                                       kMinAutosaveMinutes, kMaxAutosaveMinutes);
  return general;
}

void GeneralSettings::save() const {
  QSettings settings;
  settings.setValue(kAutosave, autosave);
  settings.setValue(kAutosaveMinutes, autosaveMinutes);
}

bool VersionSettings::complete() const {
  return !name.trimmed().isEmpty() && !email.trimmed().isEmpty();
}

QString VersionSettings::author() const {
  return QStringLiteral("%1 <%2>").arg(name.trimmed(), email.trimmed());
}

VersionSettings VersionSettings::load() {
  const QSettings settings;
  VersionSettings versions;
  versions.useGit = settings.value(QStringLiteral("versions/useGit"), versions.useGit).toBool();
  versions.name = settings.value(QStringLiteral("versions/name")).toString();
  versions.email = settings.value(QStringLiteral("versions/email")).toString();
  versions.confirmed = settings.value(QStringLiteral("versions/confirmed"), false).toBool();
  return versions;
}

void VersionSettings::save() const {
  QSettings settings;
  settings.setValue(QStringLiteral("versions/useGit"), useGit);
  settings.setValue(QStringLiteral("versions/name"), name.trimmed());
  settings.setValue(QStringLiteral("versions/email"), email.trimmed());
  settings.setValue(QStringLiteral("versions/confirmed"), confirmed);
}

RemoteSettings RemoteSettings::load() {
  const QSettings settings;
  RemoteSettings remote;
  remote.git = settings.value(QStringLiteral("remote/git")).toString().trimmed();
  remote.checkMinutes = std::clamp(settings.value(QStringLiteral("remote/checkMinutes"), remote.checkMinutes).toInt(),
                                   0, kMaxCheckMinutes);
  remote.autoPush = settings.value(QStringLiteral("remote/autoPush"), remote.autoPush).toBool();
  return remote;
}

void RemoteSettings::save() const {
  QSettings settings;
  settings.setValue(QStringLiteral("remote/git"), git.trimmed());
  settings.setValue(QStringLiteral("remote/checkMinutes"), checkMinutes);
  settings.setValue(QStringLiteral("remote/autoPush"), autoPush);
}

void RemoteSettings::apply() const {
  // MITCAD_GIT as the environment had it before Mitcad set it.
  static const std::optional<QByteArray> original =
      qEnvironmentVariableIsSet("MITCAD_GIT") ? std::optional<QByteArray>(qgetenv("MITCAD_GIT")) : std::nullopt;
  if (!git.trimmed().isEmpty()) {
    qputenv("MITCAD_GIT", QDir::toNativeSeparators(git.trimmed()).toUtf8());
  } else if (original) {
    qputenv("MITCAD_GIT", *original);
  } else {
    qunsetenv("MITCAD_GIT");
  }
}

QString projectsDirectory() {
  const QString given = qEnvironmentVariable("MITCAD_PROJECTS_DIR");
  if (!given.isEmpty()) {
    return QDir::cleanPath(QDir(given).absolutePath());
  }
  const QString documents = QStandardPaths::writableLocation(QStandardPaths::DocumentsLocation);
  return documents.isEmpty() ? QString() : QDir(documents).filePath(QStringLiteral("Mitcad"));
}

QString recoveryDirectory() {
  const QString given = qEnvironmentVariable("MITCAD_AUTOSAVE_DIR");
  if (!given.isEmpty()) {
    return QDir::cleanPath(QDir(given).absolutePath());
  }
  const QString data = QStandardPaths::writableLocation(QStandardPaths::AppLocalDataLocation);
  // Never relative to the working directory, which may be a project's.
  return data.isEmpty() ? QString() : QDir(data).filePath(QStringLiteral("autosave"));
}

} // namespace mitcad
