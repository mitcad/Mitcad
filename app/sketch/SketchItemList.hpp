// SPDX-License-Identifier: MIT
#pragma once

#include <QIcon>
#include <QListWidget>
#include <QSize>
#include <QString>

#include "../framework/NoticeLevel.hpp"

class QKeyEvent;
class QMouseEvent;

namespace mitcad::sketch {

class SketchController;
struct ConstraintData;
struct CurveData;
struct DimensionData;
struct PointData;

// Sketch mode's list in the timeline's place (mitcad#1492): the edited
// sketch's points, curves, constraints and dimensions in the order they
// were made (the `sketch` query's `order`), one cell each with the item's
// icon and what it is (a curve's kind, marked when it is construction, a
// centerline or projected; a constraint's kind; a dimension's value as
// the view shows it), left to right and wrapping onto the next line. The
// cells are not a history: no marker, nothing rolled back or hidden.
//
// A click selects the item in the view in place of the selection (Ctrl or
// Shift adds or takes it out), and the view's selection marks the cells of
// what it holds; Delete deletes the selection as the sketch's Delete does;
// a double-click on a dimension opens its value editor in the view. A
// constraint or dimension in conflict carries the error badge, one that
// follows from the others (redundant) and a curve or point a warning of
// the sketch is about the warning badge, an inactive one the geometry does
// not meet the information badge, as the Sketch Palette tells them; an
// inactive constraint or dimension is shown dimmed.
//
// For UI tests it logs "Sketch items: p1 point, c3 line, k5 horizontal,
// k9 length 40.00" when the items change, "Sketch item c3 at x,y" for each
// cell in sight when the places change, and "Sketch item rows selected:
// c3" (or none) when the marks change.
class SketchItemList : public QListWidget {
  Q_OBJECT

public:
  explicit SketchItemList(SketchController& controller, QWidget* parent = nullptr);

  // No taller than a line of cells: the timeline beside it in the panel
  // decides the height.
  QSize sizeHint() const override;
  QSize minimumSizeHint() const override;

  // The rows again from the sketch (its `changed`), and their marks from
  // the view's selection and conflicts.
  void rebuild();
  void showSelection();
  // Logs where the cells in sight are, when that changed.
  void logPlaces();

protected:
  void mousePressEvent(QMouseEvent* event) override;
  void mouseReleaseEvent(QMouseEvent* event) override;
  void mouseMoveEvent(QMouseEvent* event) override;
  void mouseDoubleClickEvent(QMouseEvent* event) override;
  void keyPressEvent(QKeyEvent* event) override;
  void changeEvent(QEvent* event) override;

private:
  // What a row shows and logs.
  struct Row {
    QString id;
    QString icon;
    QString text;
    QString tooltip;
    QString logged; // "c3 line construction"
    bool dimmed = false;
  };
  Row pointRow(const PointData& point) const;
  Row curveRow(const CurveData& curve) const;
  Row constraintRow(const ConstraintData& constraint) const;
  Row dimensionRow(const DimensionData& dimension) const;
  // The level a row is marked with: an error for a conflict, a warning
  // for a redundant item or one a warning is about, information for an
  // unmet inactive one; a hint for none. `why` gets the words for its
  // tooltip.
  NoticeLevel markOf(const QString& id, QString& why) const;
  // The item's icon with the level's badge over its corner.
  QIcon rowIcon(const QString& name, NoticeLevel level) const;
  // The ids in the order the rows show them: the model's order, else the
  // points and curves, then the constraints and dimensions.
  QStringList orderedIds() const;
  // Logs a place once the list has laid out its cells.
  void schedulePlaces();

  SketchController& m_c;
  bool m_marking = false; // the rows' selection set from the view's
  // What the cells show (a line per cell), the conflicts their badges are
  // of, and whether they were made: a reload that changes neither keeps
  // them.
  bool m_built = false;
  QStringList m_shown;
  QStringList m_shownConflicts;
  QString m_loggedItems;
  QString m_loggedPlaces;
  QString m_loggedSelection;
};

} // namespace mitcad::sketch
