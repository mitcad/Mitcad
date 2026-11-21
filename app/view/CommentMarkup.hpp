// SPDX-License-Identifier: MIT
#pragma once

// The markup of Capture and Annotate (mitcad#2011, app/COMMANDS.md
// "Comments"): clean geometric shapes drawn over a capture of the view
// before it is attached to a comment or a reply. Each shape stays editable
// (moved, its points dragged, deleted) until Attach, which draws them into
// one flat picture. Plain Qt Gui, without widgets: the app's unit tests
// take this file alone.
//
// Points are in the picture's pixels; a line's width too, so that the
// picture looks the same at any zoom of the editor.

#include <QColor>
#include <QImage>
#include <QPointF>
#include <QRectF>
#include <QString>
#include <QStringList>
#include <QVector>

class QPainter;
class QPainterPath;

namespace mitcad {

// The editor's tools: Select picks, moves and edits shapes; the others
// draw one kind each.
enum class MarkupTool { Select, Freehand, Line, Polyline, Arc, Rectangle, Parallelogram, Arrow, Text };

// Every tool in the toolbar's order.
QVector<MarkupTool> markupTools();
// "select", "freehand", "line", …: the tool's or a shape's name in logs.
QString markupToolKey(MarkupTool tool);
// "Select", "Freehand", "Line", …: its button's text.
QString markupToolLabel(MarkupTool tool);
// Whether the tool draws its shape with one drag (Line, Arc, Rectangle,
// Parallelogram, Arrow); Freehand follows the drag, Polyline takes clicks,
// Text a click and its words.
bool markupDragTool(MarkupTool tool);

// The inks offered, in their order, the first the default: red, yellow,
// green, blue, black and white.
QVector<QColor> markupInks();
// "Red", "Yellow", …: an ink's name for its button.
QString markupInkName(int index);
// The line widths offered, in the picture's pixels (Thin, Medium, Thick).
QVector<double> markupWidths();

// One shape. Its points by kind:
//   Freehand    the path drawn, every point
//   Line, Arrow the start and the end (the arrow's head)
//   Polyline    its corners in order
//   Arc         the start, a point it passes through and the end
//   Rectangle   two opposite corners
//   Parallelogram  three corners in order; the fourth is the first plus
//               the third minus the second
//   Text        the top left of its words
struct MarkupShape {
  MarkupTool kind = MarkupTool::Line;
  QVector<QPointF> points;
  QColor color;
  double width = 4.0;
  QString text;

  bool operator==(const MarkupShape& other) const;
};

// The shape a drag from `from` to `to` draws with a drag tool: a line or
// an arrow between them, a rectangle with them as corners, a half circle
// over them (bulging to the left of the drag) or a parallelogram in their
// box leaning right by a quarter of its width.
MarkupShape markupFromDrag(MarkupTool tool, const QPointF& from, const QPointF& to, const QColor& color,
                           double width);

// The parallelogram's fourth corner.
QPointF parallelogramFourth(const MarkupShape& shape);

// The size of a text shape's letters for a line width (pixels).
double markupTextSize(double width);

// The shape's outline as a path (a text's box for a text).
QPainterPath markupPath(const MarkupShape& shape);
// Where the shape lies, its line's width included.
QRectF markupBounds(const MarkupShape& shape);
// Draws the shape: its line round-capped and round-joined in its color and
// width, an arrow's head filled, a text in its color.
void paintMarkupShape(QPainter& painter, const MarkupShape& shape);

// The points of a shape that can be dragged on their own: all of its
// points but a freehand path's (moved only as a whole).
QVector<QPointF> markupHandles(const MarkupShape& shape);
// The index of the topmost shape at `at`: on its line, or within
// `tolerance` of it, or on a text's words; -1 when none.
int markupShapeAt(const QVector<MarkupShape>& shapes, const QPointF& at, double tolerance);
// The index of the handle of `shape` within `tolerance` of `at`; -1 when
// none.
int markupHandleAt(const MarkupShape& shape, const QPointF& at, double tolerance);
// The shape moved by `delta`, or one of its handles moved to `to`.
MarkupShape markupMoved(const MarkupShape& shape, const QPointF& delta);
MarkupShape markupWithHandle(const MarkupShape& shape, int handle, const QPointF& to);

// The picture with the shapes drawn into it, in their order: the one
// flattened picture Attach stores.
QImage flattenMarkup(const QImage& picture, const QVector<MarkupShape>& shapes);

// "line, arc, rectangle" (the shapes' kinds in their order), "none".
QString markupSummary(const QVector<MarkupShape>& shapes);

// The editor's undo and redo: the shapes after each change.
class MarkupHistory {
public:
  explicit MarkupHistory(QVector<MarkupShape> start = {});
  const QVector<MarkupShape>& shapes() const { return m_states.at(m_at); }
  // A change: the shapes as they are now; what was undone is gone.
  void push(const QVector<MarkupShape>& shapes);
  bool canUndo() const { return m_at > 0; }
  bool canRedo() const { return m_at + 1 < m_states.size(); }
  bool undo();
  bool redo();

private:
  QVector<QVector<MarkupShape>> m_states;
  qsizetype m_at = 0;
};

} // namespace mitcad
