// SPDX-License-Identifier: MIT
#pragma once

#include <QLayout>
#include <QPointer>
#include <QRect>
#include <QSize>

#include <vector>

class QLineEdit;
class QWidget;

namespace mitcad {

// The line of a command panel's list of values (mitcad#2421, GitHub
// mitcad/Mitcad#671: Fillet's Mid Radii with Set, Position and Radius; its
// Control Radii): the row's name, each value's label and field, and the
// remove button.
//   - One line while every field gets its readable width there
//     (readableWidth), the fields sharing what is left.
//   - Stacked where they do not: the name and the remove button on a line
//     of their own, then each value on a line under it, the labels in a
//     column and the fields beside them as wide as the rest of the line.
//   - Under where even that leaves a field less than its readable width (a
//     larger Text size in the narrowest panel): each field under its label,
//     across the whole width.
// A field's note (its error or its value's hint, under the line edit in
// the field) is as wide as the field and as high as its text needs, in
// every arrangement: the layout lays the fields out at their height for
// their width. The labels, the name and the remove button stand on the
// center line of their line edits (docs/ui-conventions.md, Alignment).
class ListLineLayout : public QLayout {
public:
  enum class Arrangement { OneLine, Stacked, Under };

  explicit ListLineLayout(QWidget* parent = nullptr);
  ~ListLineLayout() override;
  ListLineLayout(const ListLineLayout&) = delete;
  ListLineLayout& operator=(const ListLineLayout&) = delete;

  // The row's name at the line's start, the remove button at its end.
  void setLead(QWidget* widget);
  void setTrail(QWidget* widget);
  // A value: its label (none: the field alone), its field (the line edit
  // with its note under it) and the line edit the readable width and the
  // label's center line are of (none: the field's own minimum width and
  // top line).
  void addCell(QWidget* label, QWidget* field, QLineEdit* edit = nullptr);

  // The arrangement at a width of the layout's contents, and the one it
  // was laid out in last (OneLine before that).
  Arrangement arrangementAt(int width) const;
  Arrangement arrangement() const { return m_arranged; }
  // "one line", "stacked", "under": as the panel logs it.
  static const char* arrangementName(Arrangement arrangement);

  // The width at which a value field shows a short value whole: seven
  // digits beside its trailing buttons (the step buttons, fx, a shown
  // mark), in its font, text margins and frame; never less than its
  // minimum.
  static int readableWidth(const QLineEdit* edit);

  // The width the one line needs: every item at its size hint, each field
  // at its readable width.
  int oneLineWidth() const;

  void addItem(QLayoutItem* item) override;
  QLayoutItem* itemAt(int index) const override;
  QLayoutItem* takeAt(int index) override;
  int count() const override;
  QSize sizeHint() const override;
  QSize minimumSize() const override;
  bool hasHeightForWidth() const override;
  int heightForWidth(int width) const override;
  Qt::Orientations expandingDirections() const override;
  void setGeometry(const QRect& rect) override;
  void invalidate() override;

private:
  struct Cell {
    QLayoutItem* label = nullptr;
    QLayoutItem* field = nullptr;
    QPointer<QLineEdit> edit;
  };

  // The items in itemAt's order: the lead, each cell's label and field,
  // the trail.
  std::vector<QLayoutItem*> items() const;
  int fieldNeed(const Cell& cell) const;
  int labelColumn() const;
  // Lays the items out in rect (only measures without apply); returns the
  // height used.
  int arrange(const QRect& rect, bool apply) const;
  int arrangeOneLine(const QRect& rect, bool apply) const;
  int arrangeStacked(const QRect& rect, bool under, bool apply) const;
  // The lead and the trail on a line of their own at y; returns its height
  // (0 with neither shown).
  int placeEnds(const QRect& rect, int y, bool apply) const;

  QLayoutItem* m_lead = nullptr;
  QLayoutItem* m_trail = nullptr;
  std::vector<Cell> m_cells;
  Arrangement m_arranged = Arrangement::OneLine;
  mutable int m_cachedWidth = -1;
  mutable int m_cachedHeight = -1;
};

} // namespace mitcad
