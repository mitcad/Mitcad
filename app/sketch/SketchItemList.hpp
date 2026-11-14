// SPDX-License-Identifier: MIT
#pragma once

#include <QFrame>
#include <QHash>
#include <QIcon>
#include <QListWidget>
#include <QPointer>
#include <QSize>
#include <QString>
#include <QStringList>
#include <QTimer>

#include <vector>

#include "../framework/NoticeLevel.hpp"

class QEvent;

class QKeyEvent;
class QLabel;
class QMouseEvent;
class QResizeEvent;

namespace mitcad::sketch {

class SketchController;
struct ConstraintData;
struct CurveData;
struct DimensionData;
struct PointData;

// Sketch mode's list in the timeline's place: the edited sketch's work
// steps (mitcad#1750, the `sketch` query's `steps`), one cell per use of
// a tool (Project, Line, Rectangle, Sketch Fillet, a constraint, a
// dimension, Paste, an inserted drawing, …) in the order they were made,
// left to right and wrapping onto the next line. A cell shows the tool's
// icon only, at the timeline's icon size, with a count badge when the step
// made several items. An item no recorded step holds (a design saved
// before steps were recorded, an import) is a step of its own. The cells
// are not a history: no marker, nothing rolled back or hidden (as
// mitcad#1492's list of items before it).
//
// The pointer over a cell opens its hover list over the window, above
// every floating card (mitcad#1763: the message band's card lay over its
// lower rows): the step's name (the cell's tooltip) and below it the items
// the step made, one row
// each with its icon and its kind in words ("Line", "Point",
// "Coincident"). A row under the pointer highlights its item in the view;
// a click on a row selects it (Ctrl or Shift adds or takes it out). A
// click on a cell selects all its items in place of the selection (Ctrl or
// Shift adds them, or takes them out when all are in it), and the view's
// selection marks the cells of what it holds; Delete deletes the
// selection as the sketch's Delete does; a double-click on a dimension's
// step opens its value editor in the view. A constraint or dimension in
// conflict carries the error badge, one that follows from the others
// (redundant) and a curve or point a warning of the sketch is about the
// warning badge, an inactive one the geometry does not meet the
// information badge, as the Sketch Palette tells them; the worst of a
// step's items is its cell's badge, and the hover list shows each on its
// item's row. A selected cell is drawn as the timeline draws one (a tint
// of the accent colour and its ring) with its icon in its own colours, a
// step whose items are all inactive with its icon in the disabled colours,
// and an inactive item's row so too (mitcad#1763). Texts follow
// docs/ui-conventions.md: no ids, names or coordinates.
//
// For UI tests it logs "Sketch steps: s1 project x8, s2 rectangle x12
// [error], p5 point" when the steps change (a recorded step with its id,
// kind and item count; an item alone with its id and kind), "Sketch step
// s2 at x,y" and "Sketch step s2 icon at x,y wxh" for each cell in sight,
// after "Sketch steps bar at x,y wxh" (the card or docked panel it is in,
// mitcad#1760), when the places change, "Sketch steps selected: s2" (or none) when the marks change,
// "Sketch step s2 drawn: selected, icon normal" (or "dimmed, icon
// disabled", "selected, dimmed, icon disabled") when a cell is painted
// otherwise than before and is not a plain one (mitcad#1763; in a window
// that is never painted, Windows session 0, when its state changes,
// mitcad#1787), "Sketch step s1 clicked",
// and for the hover list "Sketch step s1 hover list: Project: c2 line,
// p3 point", "Sketch step item c2 at x,y" for its rows, "Sketch step item
// c2 hovered" (or none), "Sketch step item c2 clicked" and "Sketch step
// hover list closed".
class SketchItemList : public QListWidget {
  Q_OBJECT

public:
  explicit SketchItemList(SketchController& controller, QWidget* parent = nullptr);
  ~SketchItemList() override;

  // A row of cells high; a taller place keeps its rows in its middle.
  QSize sizeHint() const override;
  QSize minimumSizeHint() const override;

  // The cells again from the sketch (its `changed`), and their marks from
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
  void resizeEvent(QResizeEvent* event) override;
  bool viewportEvent(QEvent* event) override;
  // The hover list's own events (its frame and its rows).
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  // An item as its row in the hover list shows and logs it.
  struct ItemRow {
    QString id;
    QString icon;
    QString text;
    QString logged; // "c3 line construction"
    NoticeLevel level = NoticeLevel::Hint;
    QString why; // the level's words
    bool dimmed = false;
  };
  // A cell: a step with its items.
  struct Cell {
    QString key;  // the step's id ("s2"), or its item's for one alone
    QString kind; // the step's kind; empty for an item alone
    QString name; // what the tooltip names it
    QString icon;
    std::vector<ItemRow> items;
    NoticeLevel level = NoticeLevel::Hint;
    QString why;
    bool dimmed = false;
    QString logged; // "s2 rectangle x12"
  };
  ItemRow pointRow(const PointData& point) const;
  ItemRow curveRow(const CurveData& curve) const;
  ItemRow constraintRow(const ConstraintData& constraint) const;
  ItemRow dimensionRow(const DimensionData& dimension) const;
  // The row of an id of the sketch; false for one it lacks.
  bool itemRow(const QString& id, ItemRow& row) const;
  // The tool's name and icon for a step of `kind` made of `items`.
  void describeStep(const QString& kind, Cell& cell) const;
  // The level a row is marked with: an error for a conflict, a warning
  // for a redundant item or one a warning is about, information for an
  // unmet inactive one; a hint for none. `why` gets the words for it.
  NoticeLevel markOf(const QString& id, QString& why) const;
  // An icon with the level's badge over its top right corner and, above
  // one, the count of items over its bottom right corner; `dimmed`: the
  // icon in its disabled colours, the badge and the count not (an inactive
  // item, a step of inactive ones).
  QIcon badgedIcon(const QString& name, NoticeLevel level, int count, const QSize& size, bool dimmed) const;
  // Logs how the delegate painted the cell of `key`, when that changed.
  void logLook(const QString& key, const QString& look);
  // In a window Qt never exposes, which never paints (Windows session 0,
  // mitcad#1787): logs each cell's look from its state as the delegate
  // would draw it. Nothing in an exposed window, where the delegate logs.
  void logUnpaintedLooks();
  // The steps as the model lists them; one per item for a model that
  // lists none.
  std::vector<Cell> cells() const;
  const Cell* cellOf(const QString& key) const;
  // Logs a place once the list has laid out its cells.
  void schedulePlaces();

  // The hover list.
  void showHoverList(const QString& key, const QRect& cellRect);
  void fillHoverList(const Cell& cell);
  void placeHoverList(const QRect& cellRect);
  void hideHoverList();
  void hideHoverListLater();
  void hoverRow(const QString& id);
  void logRowPlaces();

  SketchController& m_c;
  bool m_marking = false; // the cells' selection set from the view's
  // What the cells show (a line per cell), the conflicts their badges are
  // of, and whether they were made: a reload that changes neither keeps
  // them.
  bool m_built = false;
  std::vector<Cell> m_cells;
  QStringList m_shown;
  QStringList m_shownConflicts;
  QString m_loggedSteps;
  QString m_loggedPlaces;
  QString m_loggedSelection;
  QHash<QString, QString> m_loggedLooks; // logLook's, by the cell's key

  QPointer<QFrame> m_hover;
  QLabel* m_hoverTitle = nullptr;
  QListWidget* m_hoverRows = nullptr;
  QString m_hoverKey;   // the step it shows
  QRect m_hoverCell;    // where its cell is, in the list's viewport
  QString m_hoveredRow; // the item its row under the pointer highlights
  QString m_loggedRowPlaces;
  bool m_inCell = false;
  bool m_inHoverList = false;
  QTimer m_hideTimer;
};

} // namespace mitcad::sketch
