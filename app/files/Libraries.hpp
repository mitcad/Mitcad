// SPDX-License-Identifier: MIT
#pragma once

// Component libraries in the application (mitcad#64, mitcad#63): the
// settings (the sources the user chose: libraries and community indexes by
// URL or folder, each on or off), where fetched libraries are kept, and
// the library commands (core/model/src/api/commands.md, "Component
// libraries"). Local commands (list, show, search, preview, diff) run at
// once; fetches run on a thread of their own with progress and Cancel, and
// only when the user asks.

#include <QJsonObject>
#include <QList>
#include <QString>
#include <QStringList>

class QWidget;

namespace mitcad {

struct LibrarySource {
  QString url; // a git URL or a folder
  bool enabled = true;
};

// The settings libraries/sources: the sources in the order the user added
// them; the defaults before the user changed anything.
struct LibrarySettings {
  QList<LibrarySource> sources;

  static LibrarySettings load();
  void save() const;
  // The enabled sources' URLs.
  QStringList enabledUrls() const;
  // Adds a source (enabled) unless it is listed; true when added.
  bool add(const QString& url);
};

// The default sources: Mitcad's fastener library and the community index.
QList<LibrarySource> defaultLibrarySources();

// Where fetched libraries are kept: MITCAD_LIBRARIES_DIR, else
// "libraries" in the application's local data folder.
QString librariesDirectory();

// Tells the model where libraries are (once, at start).
void configureLibraries();

// Runs a local library command; a failure is {"error": {"message"}}.
QJsonObject libraryCommand(const QJsonObject& command);

// Fetches the URLs one after another with a progress dialog (Cancel ends
// git); the answers in order (a failed one has "error"). Logs each.
QList<QJsonObject> fetchLibraries(QWidget* parent, const QStringList& urls);

// A licence's note for users ("Credit the authors ..."), empty when the
// licence is not one of the accepted ones.
QString licenseNote(const QString& license);

// The licences a community index accepts.
QStringList acceptedLicenses();

} // namespace mitcad
