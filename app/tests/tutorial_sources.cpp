// SPDX-License-Identifier: MIT
// The in-app tutorials' sources (mitcad#2012, tutorials/TutorialSources):
// the public repository by default, sources added once per repository and
// read back, an installed tutorial's record of where it came from, the
// versions a tutorial's oldest MitCad is compared by; a tutorial's progress
// and the Start page's list of tutorials (mitcad#2346).
#include "UnitTests.hpp"
#include "tutorials/TutorialSources.hpp"

#include <cstdio>

#include <QCoreApplication>
#include <QDir>
#include <QFile>
#include <QJsonArray>
#include <QJsonObject>
#include <QSettings>
#include <QTemporaryDir>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "tutorial_sources.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

void settingsTests() {
  QSettings().remove(QStringLiteral("tutorials/sources"));
  TutorialSettings settings = TutorialSettings::load();
  CHECK(settings.sources == QStringList{QString::fromLatin1(kPublicTutorials)});
  CHECK(settings.add(QStringLiteral("/courses/own")));
  // Another form of a listed repository's URL is the same source.
  CHECK(!settings.add(QStringLiteral("https://www.github.com/mitcad/tutorials")));
  CHECK(!settings.add(QStringLiteral("  ")));
  settings.save();
  const TutorialSettings loaded = TutorialSettings::load();
  CHECK(loaded.sources == (QStringList{QString::fromLatin1(kPublicTutorials), QStringLiteral("/courses/own")}));
  // All removed stays empty: no default comes back.
  TutorialSettings none = loaded;
  CHECK(none.remove(QString::fromLatin1(kPublicTutorials)));
  CHECK(none.remove(QStringLiteral("/courses/own")));
  CHECK(!none.remove(QStringLiteral("/courses/own")));
  none.save();
  CHECK(TutorialSettings::load().sources.isEmpty());
}

void installedTests() {
  QTemporaryDir root;
  CHECK(root.isValid());
  CHECK(!installedTutorial(root.path(), QStringLiteral("sketch-extrude")));
  CHECK(QDir(root.path()).mkpath(QStringLiteral("sketch-extrude")));
  QFile mark(QDir(root.path()).filePath(QStringLiteral("sketch-extrude/.mitcad-tutorial-source.json")));
  CHECK(mark.open(QIODevice::WriteOnly));
  mark.write(R"({"url": "/courses/own", "rev": "0123abcd", "version": "1.2"})");
  mark.close();
  const std::optional<InstalledTutorial> installed = installedTutorial(root.path(), QStringLiteral("sketch-extrude"));
  CHECK(installed && installed->url == QStringLiteral("/courses/own") && installed->rev == QStringLiteral("0123abcd") &&
        installed->version == QStringLiteral("1.2"));
}

void versionTests() {
  CHECK(compareVersions(QStringLiteral("0.9"), QStringLiteral("0.10.1")) < 0);
  CHECK(compareVersions(QStringLiteral("0.68.0"), QStringLiteral("0.68")) == 0);
  CHECK(compareVersions(QStringLiteral("0.68.0-rc1"), QStringLiteral("0.68.0")) == 0);
  CHECK(compareVersions(QStringLiteral("1.0"), QStringLiteral("0.99.9")) > 0);
}

// A tutorial's progress (mitcad#2346): none at first, a step of how many
// read back, done, forgotten; the bridge's set_progress arguments taken or
// refused; the log's words.
void progressTests() {
  const QString id = QStringLiteral("sketch-extrude");
  forgetTutorialProgress(id);
  CHECK(loadTutorialProgress(id).state == TutorialProgress::State::NotStarted);
  CHECK(loadTutorialProgress(id).words() == QStringLiteral("not started"));
  TutorialProgress progress;
  QString error;
  CHECK(tutorialProgressFromCall(QJsonObject{{QStringLiteral("step"), 3}, {QStringLiteral("steps"), 8}}, &progress,
                                 &error));
  CHECK(progress.state == TutorialProgress::State::InProgress && progress.step == 3 && progress.steps == 8);
  saveTutorialProgress(id, progress);
  CHECK(loadTutorialProgress(id) == progress);
  CHECK(loadTutorialProgress(id).words() == QStringLiteral("in progress (step 3 of 8)"));
  CHECK(loadTutorialProgress(id).toJson() == (QJsonObject{{QStringLiteral("state"), QStringLiteral("in_progress")},
                                                          {QStringLiteral("step"), 3},
                                                          {QStringLiteral("steps"), 8}}));
  // Another tutorial's progress is its own.
  CHECK(loadTutorialProgress(QStringLiteral("other")).state == TutorialProgress::State::NotStarted);
  // A step without the count.
  CHECK(tutorialProgressFromCall(QJsonObject{{QStringLiteral("step"), 2}}, &progress, &error));
  CHECK(progress.words() == QStringLiteral("in progress (step 2)"));
  CHECK(tutorialProgressFromCall(QJsonObject{{QStringLiteral("done"), true}}, &progress, &error));
  saveTutorialProgress(id, progress);
  CHECK(loadTutorialProgress(id).state == TutorialProgress::State::Done);
  CHECK(loadTutorialProgress(id).words() == QStringLiteral("done"));
  forgetTutorialProgress(id);
  CHECK(loadTutorialProgress(id).state == TutorialProgress::State::NotStarted);
  // Wrong arguments: no step, step 0, past the count, not whole.
  for (const QJsonObject& wrong :
       {QJsonObject{}, QJsonObject{{QStringLiteral("step"), 0}},
        QJsonObject{{QStringLiteral("step"), 9}, {QStringLiteral("steps"), 8}},
        QJsonObject{{QStringLiteral("step"), 2.5}}, QJsonObject{{QStringLiteral("step"), QStringLiteral("3")}},
        QJsonObject{{QStringLiteral("done"), false}}}) {
    error.clear();
    CHECK(!tutorialProgressFromCall(wrong, &progress, &error));
    CHECK(!error.isEmpty());
  }
}

// The Start page's list (mitcad#2346): the sources in the settings' order,
// each source's tutorials as the model ordered them, one tutorial per id;
// the languages by their base; the UI's language first when a tutorial is
// in it, else all.
void listingTests() {
  const auto tutorial = [](const char* id, const char* language) {
    return QJsonObject{{QStringLiteral("id"), QLatin1String(id)}, {QStringLiteral("language"), QLatin1String(language)}};
  };
  const QJsonArray sources{
      QJsonObject{{QStringLiteral("url"), QStringLiteral("/courses/own")},
                  {QStringLiteral("name"), QStringLiteral("Own")},
                  {QStringLiteral("rev"), QStringLiteral("0123")},
                  {QStringLiteral("tutorials"), QJsonArray{tutorial("own-first", "fi"), tutorial("shared", "fi")}}},
      QJsonObject{{QStringLiteral("url"), QStringLiteral("https://github.com/mitcad/tutorials.git")},
                  {QStringLiteral("name"), QStringLiteral("MitCad tutorials")},
                  {QStringLiteral("rev"), QStringLiteral("4567")},
                  {QStringLiteral("tutorials"),
                   QJsonArray{tutorial("part-one", "en"), tutorial("part-two", "pt-BR"), tutorial("shared", "en")}}}};
  const QList<ListedTutorial> listed = listedTutorials(
      sources, {QStringLiteral("https://www.github.com/mitcad/tutorials"), QStringLiteral("/courses/own")});
  QStringList ids;
  for (const ListedTutorial& entry : listed) {
    ids << entry.id();
  }
  CHECK(ids == (QStringList{QStringLiteral("part-one"), QStringLiteral("part-two"), QStringLiteral("shared"),
                            QStringLiteral("own-first")}));
  CHECK(listed.at(0).source == QStringLiteral("MitCad tutorials") && listed.at(0).rev == QStringLiteral("4567"));
  CHECK(listed.at(3).url == QStringLiteral("/courses/own"));
  const QStringList languages = tutorialLanguages(listed);
  CHECK(languages == (QStringList{QStringLiteral("en"), QStringLiteral("fi"), QStringLiteral("pt")}));
  CHECK(defaultTutorialLanguage(languages, QStringLiteral("fi")) == QStringLiteral("fi"));
  CHECK(defaultTutorialLanguage(languages, QStringLiteral("pt-BR")) == QStringLiteral("pt"));
  CHECK(defaultTutorialLanguage(languages, QStringLiteral("sv")).isEmpty());
  CHECK(tutorialInLanguage(QStringLiteral("pt-BR"), QStringLiteral("pt")));
  CHECK(tutorialInLanguage(QStringLiteral("en"), QString()));
  CHECK(!tutorialInLanguage(QStringLiteral("en"), QStringLiteral("fi")));
}

} // namespace

int tutorialSourcesTests() {
  failures = 0;
  // The settings in a folder of the test's own, never the user's.
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QSettings::Format format = QSettings::defaultFormat();
  const QString organization = QCoreApplication::organizationName();
  const QString application = QCoreApplication::applicationName();
  QCoreApplication::setOrganizationName(QStringLiteral("MitcadTest"));
  QCoreApplication::setApplicationName(QStringLiteral("tutorial_sources"));
  QSettings::setDefaultFormat(QSettings::IniFormat);
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, directory.path());
  settingsTests();
  installedTests();
  versionTests();
  progressTests();
  listingTests();
  QSettings::setDefaultFormat(format);
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  return failures;
}
MITCAD_UNIT_TEST(tutorialSourcesTests);
