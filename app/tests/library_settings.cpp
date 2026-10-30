// SPDX-License-Identifier: MIT
// The library sources' settings (mitcad#576): the defaults, Auto-update kept
// with a source and read back, settings written before the key existed
// (Auto-update off), the sources a start-up fetch takes, added sources, and
// the index's libraries an enabled source has left out (mitcad#949).
#include "files/Libraries.hpp"

#include <cstdio>

#include <QByteArray>
#include <QCoreApplication>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QSettings>
#include <QString>
#include <QStringList>
#include <QTemporaryDir>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "library_settings.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

const QString kKey = QStringLiteral("libraries/sources");

QString text(const char* value) { return QString::fromLatin1(value); }

void defaultsTests() {
  using namespace mitcad;
  // Nothing saved: the default sources, on, and no Auto-update (no test and
  // no new installation fetches without being asked).
  const LibrarySettings settings = LibrarySettings::load();
  CHECK(settings.sources.size() == 2);
  for (const LibrarySource& source : settings.sources) {
    CHECK(source.enabled && !source.autoUpdate);
  }
  CHECK(settings.autoUpdateUrls().isEmpty());
  CHECK(settings.enabledUrls().size() == 2);
}

void roundTripTests() {
  using namespace mitcad;
  LibrarySettings settings;
  settings.sources = {{text("https://example.invalid/a.git"), true, true},
                      {text("https://example.invalid/b.git"), true, false},
                      {text("/folder/c"), false, true},
                      {text("https://example.invalid/d.git"), false, false},
                      {text("ssh://git@example.invalid/e.git"), true, true}};
  settings.save();

  // The key is written for every source.
  const QJsonArray raw = QJsonDocument::fromJson(QSettings().value(kKey).toByteArray()).array();
  CHECK(raw.size() == 5);
  for (const QJsonValue& value : raw) {
    CHECK(value.toObject().contains(QStringLiteral("autoUpdate")));
  }

  const LibrarySettings back = LibrarySettings::load();
  CHECK(back.sources.size() == 5);
  if (back.sources.size() == 5) {
    for (int i = 0; i < 5; ++i) {
      CHECK(back.sources[i].url == settings.sources[i].url);
      CHECK(back.sources[i].enabled == settings.sources[i].enabled);
      CHECK(back.sources[i].autoUpdate == settings.sources[i].autoUpdate);
    }
  }
  // Only the sources that are on and have Auto-update, in order: a source
  // that is off keeps its Auto-update but is not fetched.
  CHECK(back.autoUpdateUrls() ==
        QStringList({text("https://example.invalid/a.git"), text("ssh://git@example.invalid/e.git")}));
  CHECK(back.enabledUrls().size() == 3);

  // A change of one source is kept and the others stay.
  LibrarySettings changed = back;
  changed.sources[1].autoUpdate = true;
  changed.sources[0].autoUpdate = false;
  changed.save();
  const LibrarySettings again = LibrarySettings::load();
  CHECK(again.autoUpdateUrls() ==
        QStringList({text("https://example.invalid/b.git"), text("ssh://git@example.invalid/e.git")}));
  CHECK(again.sources[2].autoUpdate && !again.sources[2].enabled);
}

void oldSettingsTests() {
  using namespace mitcad;
  // Settings from before Auto-update: url and enabled only, and an entry
  // without even enabled (on, as before).
  const QByteArray old =
      "[{\"url\":\"https://example.invalid/a.git\",\"enabled\":true},"
      "{\"url\":\"https://example.invalid/b.git\",\"enabled\":false},"
      "{\"url\":\"  https://example.invalid/c.git \"},"
      "{\"url\":\"\",\"enabled\":true}]";
  QSettings().setValue(kKey, old);
  const LibrarySettings settings = LibrarySettings::load();
  CHECK(settings.sources.size() == 3); // the entry without a URL is dropped
  if (settings.sources.size() == 3) {
    CHECK(settings.sources[0].enabled && !settings.sources[0].autoUpdate);
    CHECK(!settings.sources[1].enabled && !settings.sources[1].autoUpdate);
    CHECK(settings.sources[2].enabled && !settings.sources[2].autoUpdate);
    CHECK(settings.sources[2].url == QStringLiteral("https://example.invalid/c.git"));
  }
  CHECK(settings.autoUpdateUrls().isEmpty());

  // Saved again, the key is there, still off.
  settings.save();
  const LibrarySettings kept = LibrarySettings::load();
  CHECK(kept.sources.size() == 3 && kept.autoUpdateUrls().isEmpty());
  const QJsonArray raw = QJsonDocument::fromJson(QSettings().value(kKey).toByteArray()).array();
  CHECK(!raw.isEmpty() && raw.first().toObject().value(QStringLiteral("autoUpdate")).isBool());
  CHECK(!raw.first().toObject().value(QStringLiteral("autoUpdate")).toBool(true));

  // A value that is not a list gives no sources, not a crash.
  QSettings().setValue(kKey, QByteArray("not json"));
  CHECK(LibrarySettings::load().sources.isEmpty());
}

void addTests() {
  using namespace mitcad;
  LibrarySettings settings;
  CHECK(settings.add(text(" https://example.invalid/new.git ")));
  CHECK(!settings.add(text("https://example.invalid/new.git")));
  CHECK(settings.sources.size() == 1);
  // A new source is on and does not fetch by itself.
  CHECK(settings.sources.first().enabled && !settings.sources.first().autoUpdate);
  CHECK(settings.autoUpdateUrls().isEmpty());
  CHECK(settings.sources.first().url == QStringLiteral("https://example.invalid/new.git"));

  CHECK(normalLibraryUrl(text(" /folder/library// ")) == QStringLiteral("/folder/library"));
  CHECK(normalLibraryUrl(text("https://example.invalid/a.git")) == QStringLiteral("https://example.invalid/a.git"));
}

// The index's libraries a browser lists (mitcad#949): an index library
// whose repository is an enabled source's under another id is left out of
// the part grid (the owner's duplicate Screws group), as are those the
// search answer leaves out; the rest as before.
void indexLibraryTests() {
  using namespace mitcad;
  const QJsonObject answer = QJsonDocument::fromJson(QByteArray(R"json({
    "components": [{"id": "iso4762", "library": "mitcad-parts"}],
    "libraries": [
      {"id": "mitcad-fasteners", "url": "https://github.com/mitcad/parts/", "fetched": false},
      {"id": "mitcad-parts", "url": "https://github.com/mitcad/parts.git", "fetched": true},
      {"id": "brackets", "url": "https://example.invalid/brackets.git", "fetched": false}
    ],
    "left_out": [{"id": "old-nuts", "text": "old-nuts left out: superseded by mitcad-parts (https://github.com/mitcad/parts.git)"}]
  })json")).object();
  const QStringList sources{QStringLiteral("https://github.com/mitcad/parts.git"),
                            QStringLiteral("https://github.com/mitcad/community-index.git")};
  const auto ids = [](const QJsonArray& list) {
    QStringList out;
    for (const QJsonValue& value : list) {
      out << value.toObject().value(QStringLiteral("id")).toString();
    }
    return out;
  };
  QStringList leftOut;
  // Insert from Library: the fetched library's components are listed
  // themselves; the old library under the parts repository is left out.
  CHECK(ids(browserLibraries(answer, sources, false, &leftOut)) == QStringList{QStringLiteral("brackets")});
  CHECK(leftOut.size() == 2);
  CHECK(leftOut.contains(
      QStringLiteral("mitcad-fasteners left out: same repository as https://github.com/mitcad/parts.git")));
  CHECK(leftOut.first().startsWith(QStringLiteral("old-nuts left out: superseded by mitcad-parts")));
  // The Community Library lists the fetched one too, never the old one.
  CHECK(ids(browserLibraries(answer, sources, true)) ==
        (QStringList{QStringLiteral("mitcad-parts"), QStringLiteral("brackets")}));
  // The source disabled: the old library is offered again.
  CHECK(ids(browserLibraries(answer, {sources.last()}, false)) ==
        (QStringList{QStringLiteral("mitcad-fasteners"), QStringLiteral("brackets")}));
}

} // namespace

int librarySettingsTests() {
  // The settings in a folder of the test's own, never the user's.
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QSettings::Format format = QSettings::defaultFormat();
  const QString organization = QCoreApplication::organizationName();
  const QString application = QCoreApplication::applicationName();
  QCoreApplication::setOrganizationName(QStringLiteral("MitcadTest"));
  QCoreApplication::setApplicationName(QStringLiteral("library_settings"));
  QSettings::setDefaultFormat(QSettings::IniFormat);
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, directory.path());
  CHECK(QSettings().fileName().startsWith(directory.path()));
  defaultsTests();
  roundTripTests();
  oldSettingsTests();
  addTests();
  indexLibraryTests();
  QSettings::setDefaultFormat(format);
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  return failures;
}
