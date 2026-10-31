// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <utility>

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
//   - the actions on one row: Keep as Dimension (the measurement shown
//     kept in the design, mitcad#649) and Annotate ▾, a menu of Note…,
//     Datum Label…, Frame…, Hole Callout and Annotations… (mitcad#702,
//     mitcad#902);
//   - the history, folded when the panel opens: its header "History (n)"
//     unfolds it, its ⋯ menu has Copy All and Clear; one line a row,
//     newest first, in a list of limited height that scrolls. A click on a
//     row shows it again; its context menu has Show, Keep as Dimension and
//     Copy. A row whose geometry is gone is grey and its tooltip says so.
//
// Logged for the UI tests: the rows whenever they change ("Measure
// history: 3 rows", "Measure history row 3: <picked>: <values>"), the
// places of the controls and the rows with the panel's ("Measure Keep as
// Dimension at x,y", "Measure history row 3 at x,y"), whether a control's
// text is clipped ("Measure controls: none clipped"), the clicks and the
// menus' entries ("Context menu: Copy All | Clear").
class MeasureHistoryView : public QWidget {
public:
  struct Actions {
    std::function<void(int number)> show; // a row clicked, or its Show
    std::function<void()> clear;          // Clear chosen
    // Measurement objects (mitcad#649): Keep as Dimension clicked (the
    // measurement the panel shows), or chosen in a row's menu.
    std::function<void()> keep;
    std::function<void(int number)> keepRow;
    // Annotations (mitcad#702): Note… chosen (a leader note on the one
    // pick the panel shows), Annotations… chosen (their window).
    std::function<void()> note;
    std::function<void()> annotations;
    // Datum Label… and Frame… chosen (phase 2): a datum label or a feature
    // control frame on a leader to that pick.
    std::function<void()> datum;
    std::function<void()> frame;
    // Hole Callout chosen (mitcad#902): a hole callout on that pick.
    std::function<void()> callout;
  };

  explicit MeasureHistoryView(const MeasureHistory& history, QWidget* parent = nullptr);
  void setActions(Actions actions) { m_actions = std::move(actions); }
  // Whether the measurement the panel shows can be kept (picks of the
  // model's, with a result): Keep as Dimension is enabled. Disabled with a
  // reason (sketch geometry picked, mitcad#869), its tooltip gives the
  // reason instead of what it does. Logs "Measure Keep as Dimension
  // enabled" or "… disabled" (": <reason>" with one) when it changes, and
  // the places again.
  void setKeepable(bool keepable, const QString& why = QString());
  // Whether the panel shows one pick of the model's, which a note can
  // point at: Note… is enabled; and whether it is a feature (not a vertex
  // or a datum point) a datum label or a frame can mark: Datum Label…,
  // Frame… and Hole Callout are enabled. Logs the Annotate menu's entries
  // that can be chosen when they change ("Measure Annotate entries:
  // Note…, Annotations…").
  void setNoteable(bool noteable, bool feature);
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
  void annotateMenu();
  void historyMenu();
  void rowMenu(QListWidgetItem* item, const QPoint& global);
  void logAnnotateEntries();
  // The list as high as its rows up to a limit. Each row at least a line
  // of text high: a list not shown yet (Windows session 0) gave smaller
  // size hints, and only one row was in sight.
  void fitHeight();

  const MeasureHistory& m_history;
  Actions m_actions;
  QPushButton* m_keep = nullptr;     // Keep as Dimension (mitcad#649)
  QString m_keepTip;                 // its own tooltip, when it has no reason
  QPushButton* m_annotate = nullptr; // Annotate ▾
  QMenu* m_annotateMenu = nullptr;
  QAction* m_note = nullptr;    // a note on the pick shown (mitcad#702)
  QAction* m_datum = nullptr;   // a datum label on it
  QAction* m_frame = nullptr;   // a feature control frame on it
  QAction* m_callout = nullptr; // a hole callout on it (mitcad#902)
  QAction* m_annotations = nullptr; // the Annotations window
  QToolButton* m_toggle = nullptr; // "History (n)": folds the history
  QToolButton* m_more = nullptr;   // ⋯: Copy All, Clear
  QListWidget* m_list = nullptr;
  bool m_open = false; // the history unfolded
  QString m_entries;   // the Annotate entries logged last
  QStringList m_logged; // the places logged last
};

} // namespace mitcad
