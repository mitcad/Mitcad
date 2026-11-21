// SPDX-License-Identifier: MIT
// The in-app tutorials' sources (mitcad#2012, tutorials/TutorialSources):
// the public repository by default, sources added once per repository and
// read back, an installed tutorial's record of where it came from, and the
// versions a tutorial's oldest MitCad is compared by.
#include "UnitTests.hpp"
#include "tutorials/TutorialSources.hpp"

#include <cstdio>

#include <QCoreApplication>
#include <QDir>
#include <QFile>
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
  QSettings::setDefaultFormat(format);
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  return failures;
}
MITCAD_UNIT_TEST(tutorialSourcesTests);
