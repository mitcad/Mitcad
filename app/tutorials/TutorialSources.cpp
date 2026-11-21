// SPDX-License-Identifier: MIT
// Where in-app tutorials come from and where they are installed
// (mitcad#2012): see TutorialSources.hpp.
#include "TutorialSources.hpp"

#include <algorithm>

#include <QDir>
#include <QFile>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QSettings>
#include <QStandardPaths>

#include "../files/Libraries.hpp"

namespace mitcad {
namespace {

constexpr char kSourcesKey[] = "tutorials/sources";

} // namespace

TutorialSettings TutorialSettings::load() {
  QSettings settings;
  TutorialSettings loaded;
  if (!settings.contains(QLatin1String(kSourcesKey))) {
    loaded.sources = {QString::fromLatin1(kPublicTutorials)};
    return loaded;
  }
  const QJsonArray list =
      QJsonDocument::fromJson(settings.value(QLatin1String(kSourcesKey)).toString().toUtf8()).array();
  for (const QJsonValue& value : list) {
    const QString url = value.toString().trimmed();
    if (!url.isEmpty() && !loaded.sources.contains(url)) {
      loaded.sources << url;
    }
  }
  return loaded;
}

void TutorialSettings::save() const {
  QSettings().setValue(QLatin1String(kSourcesKey),
                       QString::fromUtf8(QJsonDocument(QJsonArray::fromStringList(sources)).toJson(QJsonDocument::Compact)));
}

bool TutorialSettings::add(const QString& url) {
  const QString given = url.trimmed();
  if (given.isEmpty()) {
    return false;
  }
  const QString key = repositoryKey(given);
  if (std::any_of(sources.begin(), sources.end(), [&key](const QString& s) { return repositoryKey(s) == key; })) {
    return false;
  }
  sources << given;
  return true;
}

bool TutorialSettings::remove(const QString& url) { return sources.removeAll(url) > 0; }

QString tutorialsDirectory() {
  const QString given = qEnvironmentVariable("MITCAD_TUTORIALS_DIR");
  return given.isEmpty() ? QDir(QStandardPaths::writableLocation(QStandardPaths::AppLocalDataLocation))
                               .filePath(QStringLiteral("tutorials"))
                         : QDir::cleanPath(given);
}

std::optional<InstalledTutorial> installedTutorial(const QString& root, const QString& id) {
  const QString folder = QDir(root).filePath(id);
  QFile mark(QDir(folder).filePath(QStringLiteral(".mitcad-tutorial-source.json")));
  if (!mark.open(QIODevice::ReadOnly)) {
    return std::nullopt;
  }
  const QJsonObject source = QJsonDocument::fromJson(mark.readAll()).object();
  InstalledTutorial installed;
  installed.id = id;
  installed.folder = folder;
  installed.url = source.value(QStringLiteral("url")).toString();
  installed.rev = source.value(QStringLiteral("rev")).toString();
  installed.version = source.value(QStringLiteral("version")).toString();
  if (installed.rev.isEmpty()) {
    return std::nullopt;
  }
  return installed;
}

int compareVersions(const QString& a, const QString& b) {
  const QStringList left = a.trimmed().split(QLatin1Char('.'));
  const QStringList right = b.trimmed().split(QLatin1Char('.'));
  const qsizetype count = std::max(left.size(), right.size());
  for (qsizetype i = 0; i < count; ++i) {
    // "0.68.0-rc1": the number before the suffix.
    const auto number = [](const QStringList& parts, qsizetype at) {
      if (at >= parts.size()) {
        return 0;
      }
      QString part = parts.at(at);
      const qsizetype dash = part.indexOf(QLatin1Char('-'));
      if (dash >= 0) {
        part.truncate(dash);
      }
      return part.toInt();
    };
    const int l = number(left, i);
    const int r = number(right, i);
    if (l != r) {
      return l < r ? -1 : 1;
    }
  }
  return 0;
}

} // namespace mitcad
