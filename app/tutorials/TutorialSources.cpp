// SPDX-License-Identifier: MIT
// Where in-app tutorials come from and where they are installed
// (mitcad#2012): see TutorialSources.hpp.
#include "TutorialSources.hpp"

#include <algorithm>
#include <utility>

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
// tutorials/progress/<id> (mitcad#2346).
constexpr char kProgressKey[] = "tutorials/progress/";

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

QJsonObject TutorialProgress::toJson() const {
  static const char* const states[] = {"not_started", "in_progress", "done"};
  QJsonObject json{{QStringLiteral("state"), QLatin1String(states[static_cast<int>(state)])}};
  if (step > 0) {
    json.insert(QStringLiteral("step"), step);
  }
  if (steps > 0) {
    json.insert(QStringLiteral("steps"), steps);
  }
  return json;
}

QString TutorialProgress::words() const {
  switch (state) {
  case State::NotStarted:
    return QStringLiteral("not started");
  case State::Done:
    return QStringLiteral("done");
  case State::InProgress:
    break;
  }
  if (step > 0 && steps > 0) {
    return QStringLiteral("in progress (step %1 of %2)").arg(step).arg(steps);
  }
  if (step > 0) {
    return QStringLiteral("in progress (step %1)").arg(step);
  }
  return QStringLiteral("in progress");
}

TutorialProgress loadTutorialProgress(const QString& id) {
  const QJsonObject json =
      QJsonDocument::fromJson(QSettings().value(QLatin1String(kProgressKey) + id).toString().toUtf8()).object();
  TutorialProgress progress;
  const QString state = json.value(QStringLiteral("state")).toString();
  if (state == QLatin1String("done")) {
    progress.state = TutorialProgress::State::Done;
  } else if (state == QLatin1String("in_progress")) {
    progress.state = TutorialProgress::State::InProgress;
    progress.step = std::max(0, json.value(QStringLiteral("step")).toInt());
    progress.steps = std::max(0, json.value(QStringLiteral("steps")).toInt());
  }
  return progress;
}

void saveTutorialProgress(const QString& id, const TutorialProgress& progress) {
  if (progress.state == TutorialProgress::State::NotStarted) {
    forgetTutorialProgress(id);
    return;
  }
  QSettings().setValue(QLatin1String(kProgressKey) + id,
                       QString::fromUtf8(QJsonDocument(progress.toJson()).toJson(QJsonDocument::Compact)));
}

void forgetTutorialProgress(const QString& id) { QSettings().remove(QLatin1String(kProgressKey) + id); }

bool tutorialProgressFromCall(const QJsonObject& args, TutorialProgress* progress, QString* error) {
  if (args.value(QStringLiteral("done")).toBool()) {
    *progress = TutorialProgress();
    progress->state = TutorialProgress::State::Done;
    return true;
  }
  const QJsonValue step = args.value(QStringLiteral("step"));
  const QJsonValue steps = args.value(QStringLiteral("steps"));
  // Whole numbers only: 2.5 is no step.
  const auto whole = [](const QJsonValue& value) {
    return value.isDouble() && value.toDouble() == static_cast<double>(value.toInt()) ? value.toInt() : -1;
  };
  const int at = whole(step);
  const int of = steps.isUndefined() || steps.isNull() ? 0 : whole(steps);
  if (at < 1 || of < 0 || (of > 0 && at > of) || of > 10000) {
    *error = QStringLiteral("set_progress takes {\"step\": n, \"steps\": m} with 1 <= n <= m, or {\"done\": true}");
    return false;
  }
  *progress = TutorialProgress();
  progress->state = TutorialProgress::State::InProgress;
  progress->step = at;
  progress->steps = of;
  return true;
}

QString ListedTutorial::id() const { return tutorial.value(QStringLiteral("id")).toString(); }

QString ListedTutorial::language() const { return tutorial.value(QStringLiteral("language")).toString(); }

QList<ListedTutorial> listedTutorials(const QJsonArray& sources, const QStringList& configured) {
  QStringList keys;
  for (const QString& url : configured) {
    keys << repositoryKey(url);
  }
  QList<QJsonObject> ordered;
  for (const QJsonValue& value : sources) {
    ordered << value.toObject();
  }
  // The sources in the settings' order; a stable sort keeps the model's
  // order among those that are not there.
  const auto rank = [&keys](const QJsonObject& source) {
    const qsizetype at = keys.indexOf(repositoryKey(source.value(QStringLiteral("url")).toString()));
    return at < 0 ? keys.size() : at;
  };
  std::stable_sort(ordered.begin(), ordered.end(),
                   [&rank](const QJsonObject& a, const QJsonObject& b) { return rank(a) < rank(b); });
  QList<ListedTutorial> listed;
  QStringList ids;
  for (const QJsonObject& source : std::as_const(ordered)) {
    const QString url = source.value(QStringLiteral("url")).toString();
    const QString name = source.value(QStringLiteral("name")).toString();
    for (const QJsonValue& entry : source.value(QStringLiteral("tutorials")).toArray()) {
      ListedTutorial tutorial;
      tutorial.url = url;
      tutorial.source = name.isEmpty() ? url : name;
      tutorial.rev = source.value(QStringLiteral("rev")).toString();
      tutorial.tutorial = entry.toObject();
      // One tutorial per id: it is installed into a folder by its id.
      if (tutorial.id().isEmpty() || ids.contains(tutorial.id())) {
        continue;
      }
      ids << tutorial.id();
      listed << tutorial;
    }
  }
  return listed;
}

QStringList tutorialLanguages(const QList<ListedTutorial>& tutorials) {
  QStringList languages;
  for (const ListedTutorial& tutorial : tutorials) {
    const QString language = tutorial.language().section(QLatin1Char('-'), 0, 0).toLower();
    if (!language.isEmpty() && !languages.contains(language)) {
      languages << language;
    }
  }
  languages.sort();
  return languages;
}

bool tutorialInLanguage(const QString& tutorialLanguage, const QString& language) {
  if (language.isEmpty()) {
    return true;
  }
  return tutorialLanguage.compare(language, Qt::CaseInsensitive) == 0 ||
         tutorialLanguage.section(QLatin1Char('-'), 0, 0).compare(language, Qt::CaseInsensitive) == 0;
}

QString defaultTutorialLanguage(const QStringList& languages, const QString& interface) {
  const QString base = interface.section(QLatin1Char('-'), 0, 0).section(QLatin1Char('_'), 0, 0);
  if (base.isEmpty()) {
    return QString();
  }
  const bool any = std::any_of(languages.cbegin(), languages.cend(),
                               [&base](const QString& language) { return tutorialInLanguage(language, base); });
  return any ? base.toLower() : QString();
}

} // namespace mitcad
