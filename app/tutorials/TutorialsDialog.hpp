// SPDX-License-Identifier: MIT
#pragma once

// Help › Tutorials… (mitcad#2012, docs/tutorials.md "Sources"): the
// tutorials of the configured sources (tutorials/sources: the public
// repository by default, the user's own repositories by URL or folder as
// library sources are), fetched through the libraries' machinery (a bare
// clone each, files/Libraries.hpp) the first time the dialog opens and on
// Update, listed with their language, version, licence, source and whether
// they are installed. Open takes the chosen one; MainWindow installs it (or
// its newer version) into the user's data folder and opens its pane.
//
// Logged for the UI tests: "Tutorials: listed <id> (<title>) from <source>,
// installed <version|no>", "Tutorials: source added: <url>".

#include <optional>

#include <QDialog>
#include <QJsonArray>
#include <QJsonObject>
#include <QString>

class QLabel;
class QPushButton;
class QTableWidget;

namespace mitcad {

class TutorialsDialog : public QDialog {
  Q_OBJECT

public:
  // The tutorial chosen: its source and its manifest (tutorial_list's).
  struct Chosen {
    QString url;
    QJsonObject tutorial;
  };

  explicit TutorialsDialog(QWidget* parent);
  std::optional<Chosen> chosen() const { return m_chosen; }

  // For the tests (TestDriver's invoke-widget tutorialsDialog selectTutorial
  // <id>): selects the tutorial's row.
  Q_INVOKABLE void selectTutorial(const QString& id);

protected:
  void showEvent(QShowEvent* event) override;

private:
  // The fetched sources' tutorials into the table.
  void refresh();
  // Fetches `urls` (all the sources for Update) with the libraries' fetch.
  void fetch(const QStringList& urls);
  void addSource();
  void openChosen();
  void updateButtons();

  QTableWidget* m_table = nullptr;
  QLabel* m_note = nullptr;
  QPushButton* m_open = nullptr;
  // Per row: {"url", "tutorial"}.
  QJsonArray m_rows;
  bool m_fetchedAtShow = false;
  std::optional<Chosen> m_chosen;
};

} // namespace mitcad
