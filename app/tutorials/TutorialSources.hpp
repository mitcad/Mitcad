// SPDX-License-Identifier: MIT
#pragma once

// Where in-app tutorials come from and where they are installed (mitcad#2012,
// docs/tutorials.md "Sources"): the settings tutorials/sources (git
// repositories by URL or folder, as library sources are; the public
// repository by default), the folder installed tutorials are written to
// (MITCAD_TUTORIALS_DIR, else the user's local data folder's "tutorials"),
// what an installed tutorial records of where it came from, and the check of
// a tutorial's oldest MitCad. Plain Qt Core (app/tests/tutorial_sources.cpp);
// the fetches go through the libraries' (files/Libraries.hpp).

#include <optional>

#include <QString>
#include <QStringList>

namespace mitcad {

// The public repository of tutorials, the default source.
inline constexpr char kPublicTutorials[] = "https://github.com/mitcad/tutorials.git";

struct TutorialSettings {
  QStringList sources; // in the order the user added them

  static TutorialSettings load();
  void save() const;
  // Adds a source unless it or another form of its repository's URL is
  // listed (repositoryKey); true when added.
  bool add(const QString& url);
  // Removes the source `url`; true when it was listed.
  bool remove(const QString& url);
};

// The folder installed tutorials are written to, one folder each by id.
QString tutorialsDirectory();

// What an installed tutorial's folder records (.mitcad-tutorial-source.json):
// its source, the source's version it was installed at, its own version.
struct InstalledTutorial {
  QString id;
  QString folder;
  QString url;
  QString rev;
  QString version;
};
std::optional<InstalledTutorial> installedTutorial(const QString& root, const QString& id);

// Dotted version numbers compared ("0.9" < "0.10.1"): <0, 0 or >0. A part
// that is no number counts as 0.
int compareVersions(const QString& a, const QString& b);

} // namespace mitcad
