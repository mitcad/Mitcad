// SPDX-License-Identifier: MIT
#pragma once

#include <vector>

#include <QPoint>
#include <QRect>
#include <QString>
#include <QStringList>
#include <QWidget>

namespace mitcad {

// The list a press held still on the 3D view opens beside the pointer
// (mitcad#1703, app/COMMANDS.md "Selection"): one row for each item under
// the pointer, nearest first, in words ("Body1: Face"). A child of the
// view, painted in the chrome's roles (Raised, Line, Text, HoverFill); it
// keeps clear of the cards floating over the view where it can, as the
// cards are above the view and its children.
//
// The pointer over a row says so (rowHovered, -1 when it leaves the rows),
// a click on a row chooses it with the keys held then (rowChosen); Esc
// anywhere in the application or a press outside the list closes it
// (dismissed; the press goes on to where it was made). The wheel scrolls
// a list longer than the view. It never takes the keyboard focus, so a
// command panel's field keeps it.
class PickList : public QWidget {
  Q_OBJECT

public:
  explicit PickList(QWidget* view);
  ~PickList() override;

  // Shows the rows beside `pointer` (the view's widget pixels): below and
  // to the right of it, else on another side, inside `area` and clear of
  // `covered` where one side is.
  void open(const QStringList& rows, const QPoint& pointer, const QRect& area, const std::vector<QRect>& covered);
  // Hides it (no signal).
  void dismiss();
  bool isOpen() const { return isVisible(); }
  int rowCount() const { return static_cast<int>(m_rows.size()); }
  // A row's rectangle in this widget, or an empty one for a row scrolled
  // out of sight.
  QRect rowRect(int row) const;
  int hoveredRow() const { return m_hovered; }

signals:
  void rowHovered(int row);
  void rowChosen(int row, Qt::KeyboardModifiers modifiers);
  void dismissed(const QString& why);

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;
  void paintEvent(QPaintEvent* event) override;
  void mouseMoveEvent(QMouseEvent* event) override;
  void mousePressEvent(QMouseEvent* event) override;
  void mouseReleaseEvent(QMouseEvent* event) override;
  void leaveEvent(QEvent* event) override;
  void wheelEvent(QWheelEvent* event) override;

private:
  int rowAt(const QPoint& position) const;
  int rowHeight() const;
  void setHovered(int row);
  void closeFor(const QString& why);

  QStringList m_rows;
  int m_first = 0;      // the first row in sight (scrolled)
  int m_shown = 0;      // how many rows fit
  int m_hovered = -1;
  int m_pressed = -1;   // the row a press on the list began on
  bool m_filtering = false;
};

} // namespace mitcad
