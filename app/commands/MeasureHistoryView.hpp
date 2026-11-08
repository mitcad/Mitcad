// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <utility>

#include <QCoreApplication>
#include <QStringList>
#include <QWidget>

class QAction;
class QListWidget;
class QListWidgetItem;
class QMenu;
class QPushButton;
class QToolButton;

namespace mitcad {

class MeasureHistory;

// The Measure panel's section under its result (mitcad#648, mitcad#1010;
// app/COMMANDS.md "Measure's panel" and "Measure's history"):
//
//   - the history, folded when the panel opens: its header "History (n)"
//     unfolds it, its ⋯ menu has Copy All and Clear; one line a row,
//     newest first, in a list of limited height that scrolls. A click on a
//     row shows it again; its context menu has Show and Copy. A row whose
//     geometry is gone is grey and its tooltip says so.
//
// What kept a measurement as an annotation (Keep as Dimension, Annotate ▾)
// is the Make tab's Annotate group since mitcad#1049: Measure measures.
//
// Logged for the UI tests: the rows whenever they change ("Measure
// history: 3 rows", "Measure history row 3: <picked>: <values>"), the
// places of the controls and the rows with the panel's ("Measure history
// at x,y", "Measure history row 3 at x,y"), whether a control's
// text is clipped ("Measure controls: none clipped"), the clicks and the
// menus' entries ("Context menu: Copy All | Clear").
class MeasureHistoryView : public QWidget {
  Q_DECLARE_TR_FUNCTIONS(mitcad::MeasureHistoryView)

public:
  struct Actions {
    std::function<void(int number)> show; // a row clicked, or its Show
    std::function<void()> clear;          // Clear chosen
  };

  explicit MeasureHistoryView(const MeasureHistory& history, QWidget* parent = nullptr);
  void setActions(Actions actions) { m_actions = std::move(actions); }
  // The rows again from the history; logs them, and their places once laid
  // out.
  void refresh();
  // The controls' and the rows' places in the main window's coordinates
  // ("Measure history row 3 at x,y", "… out of sight" folded or scrolled
  // away), and whether a control's text is clipped.
  QStringList places() const;

private:
  void copyRow(int number);
  void copyAll();
  void logPlaces();
  void setOpen(bool open);
  void historyMenu();
  void rowMenu(QListWidgetItem* item, const QPoint& global);
  // The list as high as its rows up to a limit. Each row at least a line
  // of text high: a list not shown yet (Windows session 0) gave smaller
  // size hints, and only one row was in sight.
  void fitHeight();

  const MeasureHistory& m_history;
  Actions m_actions;
  QToolButton* m_toggle = nullptr; // "History (n)": folds the history
  QToolButton* m_more = nullptr;   // ⋯: Copy All, Clear
  QListWidget* m_list = nullptr;
  bool m_open = false; // the history unfolded
  QStringList m_logged; // the places logged last
};

} // namespace mitcad
