// SPDX-License-Identifier: MIT
#pragma once

#include <QHash>
#include <QPair>
#include <QString>
#include <QVector>
#include <QWidget>

#include "FavoriteParameterRows.hpp"

class QEvent;
class QGridLayout;
class QLabel;
class QLineEdit;
class QSlider;
class QVBoxLayout;

namespace mitcad {

class DocumentHost;

// The Favorite Parameters card (mitcad#2203, GitHub mitcad/Mitcad#547;
// app/COMMANDS.md "Favorite Parameters"): the starred parameters' values
// changed without opening the Parameters window. The content of a left
// panel of its own (MainWindow::addFavoriteParametersCard and
// dockFavoriteParametersPanel, browser/MainWindowFavoriteParameters.cpp),
// shown by MANAGE › Parameters › Favorite Parameters.
//
// One row per favorite in the Parameters window's Favorites order (its
// sort): the name as plain text, the value field with the expression as
// the window shows it; a formula's value in a small grey line under it
// ("= 60 mm"); a parameter with a slider (a minimum and a maximum of its
// own, mitcad#665) the slider on a row of its own under them, the card's
// width. Enter or leaving the field sends what was typed as one
// `set_parameter` (one undo step; a bare number gets the parameter's
// unit), Esc restores the field; a refusal keeps the text as typed with a
// red frame and the model's reason under it. While the slider's handle is
// dragged the field and a tooltip at the handle show the value there; the
// released handle or a key sends it (one undo step per release). Without
// favorites it says how to star them and offers Open Parameters….
//
// Logged for UI tests: "Favorite parameters card: width = 1200 mm (600 mm
// … 2400 mm, step 10 mm); depth = width / 20 (= 60 mm); count = 4" or
// "Favorite parameters card: none" when it changes (a refused row ends with
// " [typed 2600, refused: <why>]"); the places "Favorite
// parameters <name> at x,y" (the field), "Favorite parameters <name>
// slider at x,y", "Favorite parameters <name> slider handle at x,y" and
// "Favorite parameters open at x,y" (Open Parameters… of the empty card)
// in the main window's coordinates; "Favorite parameters <name> at the
// handle: 1300 mm" while a handle is dragged; "Favorite parameter <name>
// set: <expression> = <value>", "Favorite parameter <name> refused:
// <typed>: <why>", "Favorite parameter <name> restored: <expression>"
// (Esc).
class FavoriteParametersCard : public QWidget {
  Q_OBJECT

public:
  explicit FavoriteParametersCard(DocumentHost& host, QWidget* parent = nullptr);

  // Reads the parameters again (after any change of the design). Only
  // while the card is shown; a hidden card reads them when it shows.
  void refresh();
  void logLayout();
  // The rows' height as laid out (the panel's content height).
  int contentHeight() const;

signals:
  // Open Parameters… (the empty card) or the header's button.
  void openParametersRequested();
  // The rows changed (the panel follows their height).
  void contentChanged();

protected:
  void showEvent(QShowEvent* event) override;
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  struct Row {
    FavoriteRow data;
    QWidget* block = nullptr;
    QLabel* name = nullptr;
    QLineEdit* field = nullptr;
    QLabel* value = nullptr; // a formula's value
    QLabel* error = nullptr; // a refusal's reason
    QSlider* slider = nullptr;
  };

  void rebuild();
  void showValues();
  Row* row(const QString& name);
  void fieldEdited(const QString& name);
  void restore(const QString& name);
  void handleMoved(const QString& name, int position);
  void sliderSet(const QString& name, int position);
  void send(const QString& name, const QString& expression, const QString& typed);
  void showHandleTip(const Row& row);
  QPoint handleCenter(const QSlider* slider) const;

  DocumentHost& m_host;
  QVBoxLayout* m_layout = nullptr;
  QWidget* m_empty = nullptr;
  QWidget* m_openButton = nullptr;
  QVector<Row> m_rows;
  // Values the model refused, by name: the text as typed and why.
  QHash<QString, QPair<QString, QString>> m_refused;
  QString m_structure; // what the rows were built for
  QString m_loggedState;
  QString m_logged;
  QString m_loggedHandle;
  bool m_stale = true; // the model changed while the card was hidden
};

} // namespace mitcad
