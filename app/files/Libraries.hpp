// SPDX-License-Identifier: MIT
#pragma once

// Component libraries in the application (mitcad#64, mitcad#63): the
// settings (the sources the user chose: libraries and community indexes by
// URL or folder, each on or off), where fetched libraries are kept, and
// the library commands (core/model-api/src/commands.md, "Component
// libraries"). Local commands (list, show, search, preview, diff) run at
// once; fetches run on a thread of their own with progress and Cancel, and
// when the user asks, or at start-up for the sources with Auto-update on
// (mitcad#576, LibraryAutoUpdate.hpp).

#include <QJsonArray>
#include <QJsonObject>
#include <QList>
#include <QString>
#include <QStringList>

class QWidget;

namespace mitcad {

struct LibrarySource {
  QString url; // a git URL or a folder
  bool enabled = true;
  // Fetched in the background when Mitcad starts (mitcad#576); only while
  // the source is enabled. Off by default; settings without the key are off.
  bool autoUpdate = false;
};

// The settings libraries/sources: the sources in the order the user added
// them; the defaults before the user changed anything.
struct LibrarySettings {
  QList<LibrarySource> sources;

  static LibrarySettings load();
  void save() const;
  // The enabled sources' URLs.
  QStringList enabledUrls() const;
  // The URLs of the sources that are enabled and have Auto-update on, in
  // order: the ones fetched at start-up.
  QStringList autoUpdateUrls() const;
  // Adds a source (enabled, Auto-update off) unless it is listed; true when
  // added.
  bool add(const QString& url);
};

// The default sources: Mitcad's parts library and the community index.
QList<LibrarySource> defaultLibrarySources();

// A source's URL compared as written, without a trailing slash, the
// separators of a folder as '/'.
QString normalLibraryUrl(const QString& url);

// The libraries of the community indexes a library browser lists from a
// `library_search` answer (mitcad#949): Insert from Library those not
// fetched, the Community Library all. The answer leaves out a library an
// index offers whose repository an enabled source has (`sources`) or whose
// id a fetched library replaces (`left_out`); an entry whose URL is an
// enabled source's while its id is not fetched is left out here too.
// `leftOut` gets one text per library left out ("<id> left out: same
// repository as <source>").
QJsonArray browserLibraries(const QJsonObject& answer, const QStringList& sources, bool community,
                            QStringList* leftOut = nullptr);
// Logs "Library index: <text>" for each text, once in a session.
void logLeftOutLibraries(const QStringList& texts);

// Where fetched libraries are kept: MITCAD_LIBRARIES_DIR, else
// "libraries" in the application's local data folder.
QString librariesDirectory();

// Tells the model where libraries are (once, at start).
void configureLibraries();

// Runs a local library command; a failure is {"error": {"message"}}.
QJsonObject libraryCommand(const QJsonObject& command);

// Fetches the URLs one after another with a progress dialog (Cancel ends
// git); the answers in order (a failed one has "error"). Logs each. A
// background fetch at start-up that is still running is ended first.
QList<QJsonObject> fetchLibraries(QWidget* parent, const QStringList& urls);

// Logs a fetch's answer: "Library fetched: ..." or "Library fetch failed:
// ...".
void logLibraryFetch(const QString& url, const QJsonObject& answer);

// A licence's note for users ("Credit the authors ..."), empty when the
// licence is not one of the accepted ones.
QString licenseNote(const QString& license);

// The licences a community index accepts.
QStringList acceptedLicenses();

} // namespace mitcad
