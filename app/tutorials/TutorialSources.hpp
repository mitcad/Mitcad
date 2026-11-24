// SPDX-License-Identifier: MIT
#pragma once

// Where in-app tutorials come from and where they are installed (mitcad#2012,
// docs/tutorials.md "Sources"): the settings tutorials/sources (git
// repositories by URL or folder, as library sources are; the public
// repository by default), the folder installed tutorials are written to
// (MITCAD_TUTORIALS_DIR, else the user's local data folder's "tutorials"),
// what an installed tutorial records of where it came from, and the check of
// a tutorial's oldest MitCad; a tutorial's progress and the Start page's
// list of tutorials (mitcad#2346). Plain Qt Core
// (app/tests/tutorial_sources.cpp); the fetches go through the libraries'
// (files/Libraries.hpp).

#include <optional>

#include <QJsonArray>
#include <QJsonObject>
#include <QList>
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

// A tutorial's progress as its pages report it (mitcad#2346, the bridge's
// set_progress, docs/tutorials.md): not started, in progress at a step of
// how many (when the page says), or done. Kept per tutorial id in the
// settings (tutorials/progress/<id>), so that the Start page's card shows it
// and the tutorial can be continued where it was left.
struct TutorialProgress {
  enum class State { NotStarted, InProgress, Done };
  State state = State::NotStarted;
  int step = 0;  // the step the user is at, from 1; 0 when the page gave none
  int steps = 0; // how many steps there are; 0 when the page gave none

  // {"state": "not_started|in_progress|done", "step", "steps"}: the
  // bridge's state().progress.
  QJsonObject toJson() const;
  // The log's words: "not started", "in progress (step 3 of 8)", "in
  // progress", "done".
  QString words() const;
  bool operator==(const TutorialProgress& other) const {
    return state == other.state && step == other.step && steps == other.steps;
  }
};
TutorialProgress loadTutorialProgress(const QString& id);
void saveTutorialProgress(const QString& id, const TutorialProgress& progress);
// Not started again (Start Over, Start Again).
void forgetTutorialProgress(const QString& id);
// set_progress's arguments: {"step": n, "steps": m} (1 <= n <= m; "steps"
// may be left out) or {"done": true}. False with the reason when they are
// none of these.
bool tutorialProgressFromCall(const QJsonObject& args, TutorialProgress* progress, QString* error);

// A tutorial as the Start page lists it (tutorials/TutorialChoice.hpp): its
// source's URL and name, the source's version (tutorial_list's rev) and its
// manifest.
struct ListedTutorial {
  QString url;
  QString source;
  QString rev;
  QJsonObject tutorial;
  QString id() const;
  QString language() const;
};
// tutorial_list's `sources`, their tutorials in the recommended order: the
// sources in the order of `configured` (the settings' sources; one not
// there last), each source's tutorials as the model sorted them (by their
// "order", then by title). A tutorial id listed by an earlier source is
// left out.
QList<ListedTutorial> listedTutorials(const QJsonArray& sources, const QStringList& configured);
// The tutorials' languages by their base ("pt" for "pt-BR"), in lower case,
// each once, sorted.
QStringList tutorialLanguages(const QList<ListedTutorial>& tutorials);
// The language the list shows first: the base of `interface` (the UI's)
// when one of `languages` is it, else "" (all of them).
QString defaultTutorialLanguage(const QStringList& languages, const QString& interface);
// Whether `language` ("" for any) is the tutorial's language or its base.
bool tutorialInLanguage(const QString& tutorialLanguage, const QString& language);

} // namespace mitcad
