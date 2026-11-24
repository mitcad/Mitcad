// SPDX-License-Identifier: MIT
#pragma once

// The Start page's tutorials (mitcad#2346, docs/tutorials.md "Sources";
// app/COMMANDS.md "In-app tutorials"): shown in the Start page's centre in
// place of the designs' browser by the page's Tutorials button and by Help ›
// Tutorials…, and the choice of the tutorial to start. Its header is a
// breadcrumb whose first part (Recent designs) leads back to the designs,
// a language choice (only when the tutorials are in several languages; the
// UI's language first when a tutorial is in it) and Sources, a menu of
// Add Source… and Update. Below it one card per tutorial of the configured
// sources (tutorials/sources, TutorialSources.hpp), in the recommended
// order: its picture (when it gives one), title, one line of what it
// teaches, its length and level (when it gives them), its progress (Not
// started, In progress (step 3 of 8), Done) and a line of its version,
// licence and source; one action, Start (Continue while in progress, Start
// Again once done), that installs the tutorial when needed and starts it
// (MainWindow::openListedTutorial: in an empty design of its own); a card's
// context menu has Start Over while it is in progress. The sources are
// fetched the first time the list shows one never fetched, and on Update;
// a picture is read from the fetched source (`tutorial_picture`) into
// <tutorials folder>/.pictures, once per version.
//
// Logged for the UI tests: "Tutorials: listed <id> (<title>) from <url>:
// <progress words>" (TutorialProgress::words) for each card, "Tutorials:
// language <code|all>", "Tutorials: source added: <url>", "Tutorials:
// chosen <id> (start|continue|start again|start over)", "Tutorials: none
// listed (<why>)"; places "Start page tutorial <id> at x,y" and "Start page
// tutorials back at x,y" (debug). The Start button of a card is
// tutorialStart_<id>.

#include <functional>

#include <QJsonObject>
#include <QList>
#include <QPointer>
#include <QString>
#include <QStringList>
#include <QWidget>

#include "TutorialSources.hpp"

class QComboBox;
class QGridLayout;
class QLabel;
class QToolButton;

namespace mitcad {

class TutorialChoice : public QWidget {
  Q_OBJECT

public:
  struct Host {
    // The tutorial chosen: installed when needed and started.
    std::function<void(const QString& url, const QJsonObject& tutorial)> start;
    // Back to the recent designs.
    std::function<void()> back;
  };

  explicit TutorialChoice(Host host, QWidget* parent = nullptr);
  ~TutorialChoice() override;

  // A tutorial's progress changed (the bridge's set_progress, in any
  // window): every list shown shows it.
  static void progressChanged(const QString& id);

protected:
  void showEvent(QShowEvent* event) override;
  void resizeEvent(QResizeEvent* event) override;

private:
  // The fetched sources' tutorials into cards, the progress of each read
  // again.
  void refresh();
  // Fetches `urls` (all the sources for Update) with the libraries' fetch.
  void fetch(const QStringList& urls);
  void addSource();
  void chooseLanguage(const QString& language);
  void startTutorial(const ListedTutorial& tutorial, bool over);
  // The cards' columns for the width.
  void layoutCards();
  void logPlaces();
  // A tutorial's picture as a file of this computer; empty when it has none
  // or it could not be read.
  QString pictureFile(const ListedTutorial& tutorial) const;

  Host m_host;
  QToolButton* m_back = nullptr;
  QComboBox* m_language = nullptr;
  QToolButton* m_sources = nullptr;
  QWidget* m_cards = nullptr;
  QGridLayout* m_grid = nullptr;
  QLabel* m_note = nullptr;
  QList<QPointer<QWidget>> m_cardWidgets;
  QList<ListedTutorial> m_listed;
  QString m_chosenLanguage; // "" all
  bool m_languageChosen = false;
  bool m_fetchedAtShow = false;
  int m_columns = 0;
  QStringList m_loggedPlaces;
  bool m_placesQueued = false;
};

} // namespace mitcad
