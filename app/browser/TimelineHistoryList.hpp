// SPDX-License-Identifier: MIT
#pragma once

#include <vector>

#include <QCoreApplication>
#include <QPoint>
#include <QString>
#include <QStringList>
#include <QTreeWidget>

class QStyledItemDelegate;

namespace mitcad {

class TimelineWidget;

// The timeline as a vertical list (mitcad#2427, GitHub mitcad/Mitcad#581;
// Preferences › Display › Timeline: History list), inside TimelineWidget
// under its buttons. A row per timeline item in timeline order, the first
// at the top: its icon and full name, painted as the Browser paints its
// rows (browser/BrowserRows.hpp: the selection's band, the owner's mark,
// the component's colour bar, the fx badge, the light bulb). A failed or
// warned feature has the level's colour and badge, a rolled-back or
// suppressed one the hidden rows' grey (a suppressed one struck through),
// a match of the search a tint of the search's colour. A timeline group is
// a folder row with its name and a fold arrow, its features under it; it
// is folded as the strip folds it (one state, TimelineWidget's). A run of
// features the timeline's filter hides is one grey row. Sketches,
// construction features and features that made or last changed bodies at
// the marker have a light bulb: the sketch's or the feature's own, or the
// bodies' (shown while any of them is).
//
// The timeline marker is a bar between the rows with a handle at the
// left, dragged up and down to the boundaries the strip allows. Everything
// else the strip's cells do the rows do, through the strip's own code:
// click, Ctrl+click and Shift+click select (and so select in the view and
// the Browser), double-click edits, the context menu, F2 renames (in the
// row), Delete deletes, Ctrl+F searches, a drag moves a feature (a red line
// and the model's reason where it cannot go), hover previews and the
// tooltip is the cell's. TimelineWidget owns all of it; this class only
// shows its state and passes the gestures on.
//
// For UI tests TimelineWidget::logLayout logs, in the window's
// coordinates, "History row <name> at x,y" for each row in sight,
// "History group <name> at x,y" (and "History group <name> arrow at x,y")
// for a group's row, "History eye <name> at x,y" for a light bulb and
// "History marker at x,y" for the marker's handle; the list logs
// "History list: Sketch1, Extrude1, [Holes: Hole1, Hole2], ...; marker
// after Extrude1" when its rows change (a folded group "[Holes folded]", a
// run "(2 hidden)"), "History eye <name>: shown|hidden" when a light
// bulb's state changes, "History selected: <names>|nothing" and "History
// drag ended: the timeline changed" when the items change while a feature
// is pressed or dragged (the release then moves nothing).
class TimelineHistoryList : public QTreeWidget {
  Q_DECLARE_TR_FUNCTIONS(mitcad::TimelineHistoryList)

public:
  explicit TimelineHistoryList(TimelineWidget& owner);

  // The rows again from the strip's items (after each timeline rebuild).
  void rebuild();
  // The selection, the owners' marks and the search's matches again.
  void updateMarks();
  // A feature's row in sight (its group opened by the strip first).
  void scrollToFeature(const QString& uid);
  // The last marked row in sight when none is.
  void showMarked();
  // Renames a feature in its row; false when no editor is open to cancel.
  void startRename(const QString& uid);
  bool cancelRename();
  // The height its rows need, all of them unfolded as they are.
  int contentHeight() const;
  // The places TimelineWidget::logLayout logs (window coordinates).
  QStringList layoutLines() const;

protected:
  void paintEvent(QPaintEvent* event) override;
  void mousePressEvent(QMouseEvent* event) override;
  void mouseMoveEvent(QMouseEvent* event) override;
  void mouseReleaseEvent(QMouseEvent* event) override;
  void mouseDoubleClickEvent(QMouseEvent* event) override;
  void contextMenuEvent(QContextMenuEvent* event) override;
  void keyPressEvent(QKeyEvent* event) override;
  void leaveEvent(QEvent* event) override;
  bool viewportEvent(QEvent* event) override;
  bool event(QEvent* event) override;
  void drawRow(QPainter* painter, const QStyleOptionViewItem& option, const QModelIndex& index) const override;

private:
  enum class Kind { Feature = 1, Group, Run };
  Kind kindOf(const QTreeWidgetItem* row) const;
  int firstOf(const QTreeWidgetItem* row) const;
  int lastOf(const QTreeWidgetItem* row) const;
  QString uidOf(const QTreeWidgetItem* row) const;
  // The row's light bulb and fx badge, in the viewport.
  QRect eyeRect(QTreeWidgetItem* row) const;
  QRect fxRect(QTreeWidgetItem* row) const;
  // Where boundary g (before item g; g = the count: after the last) is
  // drawn: the top of the row that stands for item g, else the bottom of
  // the last row shown.
  int boundaryY(int g) const;
  QTreeWidgetItem* lastShownRow() const;
  // The boundary the marker or a dragged feature takes at y, of those the
  // strip allows; -1 when there is none.
  int nearestBoundary(int y) const;
  // Whether a point is on the marker's bar or its handle.
  bool onMarker(const QPoint& at) const;
  QRect markerHandle() const;
  // A group's fold arrow clicked or its row double-clicked: folded or
  // opened through the strip (after the event: the rows are made again).
  void toggleGroup(QTreeWidgetItem* row);
  void hoverRow(QTreeWidgetItem* row);
  void logRows();
  void endGestures();
  // A feature's press or drag ended without a move (the marker's drag goes
  // on).
  void endItemGesture();
  // The items' uids in timeline order, as the press and drop indices name
  // them.
  QStringList itemOrder() const;

  TimelineWidget& m_owner;
  QStyledItemDelegate* m_delegate = nullptr;
  bool m_building = false;
  // The row whose top edge shows boundary g, by g (nullptr: after the last
  // row); the feature rows by item.
  std::vector<QTreeWidgetItem*> m_topRow;
  std::vector<QTreeWidgetItem*> m_featureRows;
  QTreeWidgetItem* m_hover = nullptr;
  // Dragging the marker.
  bool m_dragMarker = false;
  int m_dragPosition = 0;
  // Pressing and dragging a feature's row.
  int m_press = -1;
  QPoint m_pressAt;
  bool m_shift = false;
  bool m_toggle = false;
  bool m_dragItem = false;
  int m_dropIndex = -1;
  bool m_dropAllowed = true;
  // The items as they were at the press: when a rebuild changes them (an
  // agent's command, an undo) the indices name other items, and the
  // gesture ends (mitcad#2527).
  QStringList m_pressOrder;
  // The rename editor's feature.
  QString m_renaming;
  QString m_loggedRows;
  QString m_loggedSelection;
  QStringList m_loggedEyes;
};

} // namespace mitcad
