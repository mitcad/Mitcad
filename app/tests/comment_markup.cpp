// SPDX-License-Identifier: MIT
// Capture and Annotate's shapes (mitcad#2011): what a drag of each tool
// draws (an arc through its middle, a parallelogram's fourth corner), the
// shape a press takes, its handles moved, the shapes drawn into the
// picture Attach stores, the log's summary, and undo and redo.
#include "UnitTests.hpp"
#include "view/CommentMarkup.hpp"

#include <cmath>
#include <cstdio>

#include <QColor>
#include <QImage>
#include <QPainterPath>
#include <QSet>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "comment_markup.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

bool near(double a, double b, double tolerance = 1e-6) { return std::abs(a - b) <= tolerance; }

bool near(const QPointF& a, const QPointF& b, double tolerance = 1e-6) {
  return near(a.x(), b.x(), tolerance) && near(a.y(), b.y(), tolerance);
}

const QColor kRed = markupInks().value(0);

void toolTests() {
  // Every tool once, each with a key and a label; six inks, three widths.
  const QVector<MarkupTool> tools = markupTools();
  CHECK(tools.size() == 9);
  CHECK(tools.first() == MarkupTool::Select);
  QSet<QString> keys;
  for (MarkupTool tool : tools) {
    keys.insert(markupToolKey(tool));
    CHECK(!markupToolLabel(tool).isEmpty());
  }
  CHECK(keys.size() == 9);
  CHECK(markupToolKey(MarkupTool::Parallelogram) == QStringLiteral("parallelogram"));
  CHECK(markupDragTool(MarkupTool::Line) && markupDragTool(MarkupTool::Arc) && markupDragTool(MarkupTool::Arrow));
  CHECK(!markupDragTool(MarkupTool::Polyline) && !markupDragTool(MarkupTool::Freehand) &&
        !markupDragTool(MarkupTool::Text) && !markupDragTool(MarkupTool::Select));
  const QVector<QColor> inks = markupInks();
  CHECK(inks.size() == 6);
  QSet<QRgb> distinct;
  for (int i = 0; i < inks.size(); ++i) {
    distinct.insert(inks.at(i).rgb());
    CHECK(!markupInkName(i).isEmpty());
  }
  CHECK(distinct.size() == 6);
  CHECK(markupWidths().size() == 3);
}

void dragTests() {
  // A line and an arrow: their two ends.
  const MarkupShape line = markupFromDrag(MarkupTool::Line, QPointF(1, 2), QPointF(30, 2), kRed, 4.0);
  CHECK(line.kind == MarkupTool::Line);
  CHECK(line.points == (QVector<QPointF>{QPointF(1, 2), QPointF(30, 2)}));
  CHECK(line.color == kRed && line.width == 4.0);
  // An arc: a half circle over the drag, bulging to its left (up for a
  // drag to the right, y down).
  const MarkupShape arc = markupFromDrag(MarkupTool::Arc, QPointF(0, 0), QPointF(10, 0), kRed, 2.0);
  CHECK(arc.points.size() == 3);
  CHECK(near(arc.points.at(1), QPointF(5, -5)));
  const QRectF arcBox = markupPath(arc).boundingRect();
  CHECK(near(arcBox.left(), 0.0, 1e-2) && near(arcBox.right(), 10.0, 1e-2));
  CHECK(near(arcBox.top(), -5.0, 1e-2) && near(arcBox.bottom(), 0.0, 1e-2));
  // Its middle point moved: the arc passes through it.
  const MarkupShape flatter = markupWithHandle(arc, 1, QPointF(5, -2));
  CHECK(near(markupPath(flatter).boundingRect().top(), -2.0, 1e-2));
  // On one line: the straight line.
  const MarkupShape straight = markupWithHandle(arc, 1, QPointF(5, 0));
  CHECK(near(markupPath(straight).boundingRect().height(), 0.0, 1e-6));
  // A rectangle: its corners in either order.
  const MarkupShape box = markupFromDrag(MarkupTool::Rectangle, QPointF(30, 20), QPointF(10, 5), kRed, 2.0);
  CHECK(markupPath(box).boundingRect() == QRectF(10, 5, 20, 15));
  // A parallelogram in the drag's box, leaning by a quarter of its width.
  const MarkupShape leaning = markupFromDrag(MarkupTool::Parallelogram, QPointF(0, 0), QPointF(40, 20), kRed, 2.0);
  CHECK(leaning.points ==
        (QVector<QPointF>{QPointF(10, 0), QPointF(40, 0), QPointF(30, 20)}));
  CHECK(near(parallelogramFourth(leaning), QPointF(0, 20)));
  CHECK(markupPath(leaning).boundingRect() == QRectF(0, 0, 40, 20));
  // A corner dragged: the fourth follows.
  const MarkupShape edited = markupWithHandle(leaning, 2, QPointF(36, 24));
  CHECK(near(parallelogramFourth(edited), QPointF(6, 24)));
  // An arrow: its head reaches its end.
  const MarkupShape arrow = markupFromDrag(MarkupTool::Arrow, QPointF(0, 10), QPointF(50, 10), kRed, 4.0);
  CHECK(markupBounds(arrow).contains(QPointF(50, 10)));
  CHECK(markupPath(arrow).boundingRect().right() < 50.0);
}

void pickTests() {
  QVector<MarkupShape> shapes;
  shapes << markupFromDrag(MarkupTool::Line, QPointF(0, 10), QPointF(100, 10), kRed, 4.0);
  shapes << markupFromDrag(MarkupTool::Rectangle, QPointF(40, 0), QPointF(60, 20), kRed, 2.0);
  // On a line, or within the tolerance; nothing far from both.
  CHECK(markupShapeAt(shapes, QPointF(20, 10), 3.0) == 0);
  CHECK(markupShapeAt(shapes, QPointF(20, 14.5), 3.0) == 0);
  CHECK(markupShapeAt(shapes, QPointF(20, 30), 3.0) == -1);
  // Where both are, the one on top.
  CHECK(markupShapeAt(shapes, QPointF(40, 10), 3.0) == 1);
  // A rectangle is taken on its outline.
  CHECK(markupShapeAt(shapes, QPointF(60, 5), 3.0) == 1);
  // Handles: a line's ends; a freehand path has none.
  CHECK(markupHandleAt(shapes.at(0), QPointF(98, 11), 4.0) == 1);
  CHECK(markupHandleAt(shapes.at(0), QPointF(50, 10), 4.0) == -1);
  MarkupShape freehand;
  freehand.kind = MarkupTool::Freehand;
  freehand.points = {QPointF(0, 0), QPointF(5, 5), QPointF(10, 0)};
  freehand.color = kRed;
  CHECK(markupHandles(freehand).isEmpty());
  CHECK(markupWithHandle(freehand, 1, QPointF(50, 50)) == freehand);
  // Moved whole.
  const MarkupShape moved = markupMoved(freehand, QPointF(3, -1));
  CHECK(moved.points == (QVector<QPointF>{QPointF(3, -1), QPointF(8, 4), QPointF(13, -1)}));
  // A text: taken on its words.
  MarkupShape words;
  words.kind = MarkupTool::Text;
  words.points = {QPointF(10, 40)};
  words.text = QStringLiteral("M6");
  words.color = kRed;
  const QRectF wordsBox = markupBounds(words);
  CHECK(wordsBox.width() > 0.0 && wordsBox.height() > 0.0);
  shapes << words;
  CHECK(markupShapeAt(shapes, wordsBox.center(), 1.0) == 2);
}

void flattenTests() {
  QImage picture(40, 40, QImage::Format_ARGB32);
  picture.fill(markupInks().value(5)); // white
  picture.setDevicePixelRatio(2.0);
  QVector<MarkupShape> shapes{markupFromDrag(MarkupTool::Line, QPointF(5, 20), QPointF(35, 20), kRed, 4.0)};
  const QImage flat = flattenMarkup(picture, shapes);
  CHECK(flat.size() == picture.size());
  CHECK(flat.devicePixelRatio() == 2.0);
  // In the picture's pixels, whatever its pixel ratio: red on the line,
  // white away from it; the picture itself untouched.
  const QColor onLine = flat.pixelColor(20, 20);
  CHECK(onLine.red() > 200 && onLine.green() < 80 && onLine.blue() < 80);
  CHECK(flat.pixelColor(20, 5) == picture.pixelColor(20, 5));
  CHECK(flat.pixelColor(20, 30) == picture.pixelColor(20, 30));
  CHECK(picture.pixelColor(20, 20) == markupInks().value(5));
  // Nothing drawn: the same picture.
  const QImage plain = flattenMarkup(picture, {});
  CHECK(plain.pixelColor(20, 20) == picture.pixelColor(20, 20));
}

void historyTests() {
  CHECK(markupSummary({}) == QStringLiteral("none"));
  QVector<MarkupShape> shapes;
  MarkupHistory history;
  CHECK(!history.canUndo() && !history.canRedo());
  shapes << markupFromDrag(MarkupTool::Line, QPointF(0, 0), QPointF(10, 0), kRed, 4.0);
  history.push(shapes);
  shapes << markupFromDrag(MarkupTool::Arc, QPointF(0, 0), QPointF(10, 0), kRed, 4.0);
  history.push(shapes);
  shapes << markupFromDrag(MarkupTool::Rectangle, QPointF(0, 0), QPointF(10, 10), kRed, 4.0);
  history.push(shapes);
  CHECK(markupSummary(history.shapes()) == QStringLiteral("line, arc, rectangle"));
  // The same shapes again are no step.
  history.push(shapes);
  CHECK(history.undo());
  CHECK(markupSummary(history.shapes()) == QStringLiteral("line, arc"));
  CHECK(history.canRedo());
  CHECK(history.redo());
  CHECK(markupSummary(history.shapes()) == QStringLiteral("line, arc, rectangle"));
  CHECK(!history.redo());
  // A change after an undo drops what was undone.
  CHECK(history.undo() && history.undo());
  CHECK(markupSummary(history.shapes()) == QStringLiteral("line"));
  QVector<MarkupShape> other{history.shapes().first(),
                             markupFromDrag(MarkupTool::Arrow, QPointF(0, 0), QPointF(10, 10), kRed, 4.0)};
  history.push(other);
  CHECK(!history.canRedo());
  CHECK(markupSummary(history.shapes()) == QStringLiteral("line, arrow"));
  CHECK(history.undo() && history.undo());
  CHECK(history.shapes().isEmpty() && !history.canUndo());
}

} // namespace

int commentMarkupTests() {
  toolTests();
  dragTests();
  pickTests();
  flattenTests();
  historyTests();
  return failures;
}
MITCAD_UNIT_TEST(commentMarkupTests);
