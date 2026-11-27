// SPDX-License-Identifier: MIT
// A drawing's sheet (mitcad#707) on Qt's offscreen platform: a wheel zoom
// or a pan kept when the model's next answer brings a sheet of the same
// size, the whole sheet fitted for another size (mitcad#751); the sheet
// itself painted from its picture while the pointer only moves over it,
// painted again for a zoom (mitcad#759); a section's line's and a break's
// clicks snapped to points, a section's second to 45 degrees, Shift as
// clicked (mitcad#1362); the hover's and the picks' marks as wide as the
// line they mark at two zooms (mitcad#1618); hidden-edge dashes painted
// where the paper has them at every zoom, the one-pixel minimum too
// (mitcad#2477).
#include "UnitTests.hpp"
#include "drawings/DrawingSheetView.hpp"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <initializer_list>
#include <optional>

#include <QApplication>
#include <QColor>
#include <QImage>
#include <QJsonArray>
#include <QJsonObject>
#include <QMouseEvent>
#include <QPainter>
#include <QPen>
#include <QPixmap>
#include <QPoint>
#include <QPointF>
#include <QVector>
#include <QWheelEvent>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "drawing_sheet_view.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

bool same(const QPointF& a, const QPointF& b) { return std::abs(a.x() - b.x()) < 1e-9 && std::abs(a.y() - b.y()) < 1e-9; }

QJsonArray xy(double x, double y) { return QJsonArray{x, y}; }

// A sheet as the `drawing` query gives it: a view's box with a thick line,
// a filled square and a text in its group.
QJsonObject sheetOf(double width, double height) {
  QJsonArray line;
  line.append(xy(20.0, 20.0));
  line.append(xy(120.0, 20.0));
  QJsonArray square;
  square.append(xy(40.0, 30.0));
  square.append(xy(70.0, 30.0));
  square.append(xy(70.0, 50.0));
  square.append(xy(40.0, 50.0));
  QJsonArray items;
  items.append(QJsonObject{{QStringLiteral("line"), line}, {QStringLiteral("pen"), QStringLiteral("thick")}});
  items.append(QJsonObject{{QStringLiteral("fill"), square}});
  items.append(QJsonObject{{QStringLiteral("text"), QStringLiteral("A")},
                           {QStringLiteral("at"), xy(30.0, 30.0)},
                           {QStringLiteral("height"), 3.5},
                           {QStringLiteral("angle"), 0.0},
                           {QStringLiteral("anchor"), QStringLiteral("start")}});
  QJsonArray groups;
  groups.append(QJsonObject{{QStringLiteral("id"), QStringLiteral("V1")}, {QStringLiteral("items"), items}});
  QJsonArray box;
  box.append(xy(20.0, 10.0));
  box.append(xy(120.0, 60.0));
  QJsonArray views;
  views.append(QJsonObject{{QStringLiteral("id"), 1}, {QStringLiteral("type"), QStringLiteral("base")}, {QStringLiteral("box"), box}});
  return {{QStringLiteral("width"), width},
          {QStringLiteral("height"), height},
          {QStringLiteral("views"), views},
          {QStringLiteral("annotations"), QJsonArray()},
          {QStringLiteral("groups"), groups}};
}

void wheel(QWidget& widget, const QPointF& at, int steps) {
  QWheelEvent event(at, widget.mapToGlobal(at), QPoint(), QPoint(0, 120 * steps), Qt::NoButton, Qt::NoModifier,
                    Qt::NoScrollPhase, false);
  QApplication::sendEvent(&widget, &event);
}

void mouse(QWidget& widget, QEvent::Type type, const QPointF& at, Qt::MouseButton button, Qt::MouseButtons buttons,
           Qt::KeyboardModifiers modifiers = Qt::NoModifier) {
  QMouseEvent event(type, at, widget.mapToGlobal(at), button, buttons, modifiers);
  QApplication::sendEvent(&widget, &event);
}

bool closeTo(const QPointF& a, const QPointF& b) { return std::abs(a.x() - b.x()) < 1e-6 && std::abs(a.y() - b.y()) < 1e-6; }

// The pens' table as the `drawing` query gives it (core/model-drawing's
// Pen table): the hidden edges' narrow dashed line, a narrow chain line
// and a wide continuous one.
QJsonObject penTable() {
  return {{QStringLiteral("hidden"),
           QJsonObject{{QStringLiteral("width"), 0.18}, {QStringLiteral("dashes"), QJsonArray{3.0, 1.0}}}},
          {QStringLiteral("chain"),
           QJsonObject{{QStringLiteral("width"), 0.18}, {QStringLiteral("dashes"), QJsonArray{12.0, 1.5, 0.5, 1.5}}}},
          {QStringLiteral("thick"), QJsonObject{{QStringLiteral("width"), 0.35}}}};
}

// The stretches of a line from 0 to `length` (sheet mm) a pen paints with
// its round caps: each dash of its pattern (in its widths, from its
// offset) whose middle is on the line, and half its width more at each
// end.
QVector<QPointF> paintedDashes(const QPen& pen, double length) {
  QVector<QPointF> out;
  const double width = pen.widthF();
  const QVector<qreal> pattern = pen.dashPattern();
  double period = 0.0;
  for (const qreal d : pattern) {
    period += d * width;
  }
  if (period <= 0.0) {
    return out;
  }
  const double offset = pen.dashOffset() * width;
  for (double from = -std::ceil(offset / period + 1.0) * period - offset; from < length + period; from += period) {
    double at = from;
    for (int i = 0; i + 1 < pattern.size(); i += 2) {
      const double end = at + pattern[i] * width;
      const double middle = (at + end) / 2.0;
      if (middle >= 0.0 && middle < length) {
        out.append(QPointF(at - width / 2.0, end + width / 2.0));
      }
      at = end + pattern[i + 1] * width;
    }
  }
  return out;
}

bool sameDashes(const QVector<QPointF>& a, const QVector<QPointF>& b, double tolerance) {
  if (a.size() != b.size()) {
    return false;
  }
  for (int i = 0; i < a.size(); ++i) {
    if (std::abs(a[i].x() - b[i].x()) > tolerance || std::abs(a[i].y() - b[i].y()) > tolerance) {
      return false;
    }
  }
  return true;
}

// The runs of ink along row `y` of an image: each run's first and last
// pixel.
QVector<QPoint> inkRuns(const QImage& image, int y) {
  QVector<QPoint> runs;
  int start = -1;
  for (int x = 0; x < image.width(); ++x) {
    const bool ink = qGray(image.pixel(x, y)) < 160;
    if (ink && start < 0) {
      start = x;
    } else if (!ink && start >= 0) {
      runs.append(QPoint(start, x - 1));
      start = -1;
    }
  }
  if (start >= 0) {
    runs.append(QPoint(start, image.width() - 1));
  }
  return runs;
}

// Hidden-edge dashes on the screen (mitcad#2477, GitHub #695): the pen
// widened to a pixel kept its pattern in widths of the pen's own width, so
// that its dashes grew with it, the farther out the sheet was zoomed the
// longer (at 2 px/mm a 3 mm dash 8.3 mm long). Now each painted dash, its
// caps included, covers on the sheet what it covers on paper, at every
// zoom.
void sheetPenTests() {
  using mitcad::drawings::DrawingSheetView;
  const QJsonObject pens = penTable();
  // Zoomed in: the pen of the table, its pattern in its widths.
  const QPen own = DrawingSheetView::sheetPen(pens, QStringLiteral("hidden"), 50.0);
  CHECK(std::abs(own.widthF() - 0.18) < 1e-12);
  CHECK(own.style() == Qt::CustomDashLine);
  CHECK(own.capStyle() == Qt::RoundCap);
  CHECK(own.dashPattern().size() == 2);
  if (own.dashPattern().size() == 2) {
    CHECK(std::abs(own.dashPattern()[0] * 0.18 - 3.0) < 1e-9);
    CHECK(std::abs(own.dashPattern()[1] * 0.18 - 1.0) < 1e-9);
  }
  // On paper: 3 mm dashes 4 mm apart, 0.09 mm of cap at each end.
  const QVector<QPointF> paper = paintedDashes(own, 40.0);
  CHECK(paper.size() == 10);
  if (!paper.isEmpty()) {
    CHECK(std::abs(paper.first().x() + 0.09) < 1e-9 && std::abs(paper.first().y() - 3.09) < 1e-9);
  }
  // Out to where the one-pixel minimum applies, and farther: wider, the
  // same stretches painted.
  for (const double zoom : {5.0, 3.0, 2.0, 1.0, 0.5}) {
    const QPen pen = DrawingSheetView::sheetPen(pens, QStringLiteral("hidden"), zoom);
    CHECK(std::abs(pen.widthF() - std::max(0.18, 1.0 / zoom)) < 1e-12);
    CHECK(pen.capStyle() == Qt::RoundCap);
    if (!sameDashes(paintedDashes(pen, 40.0), paper, 1e-9)) {
      std::fprintf(stderr, "drawing_sheet_view.cpp: hidden dashes at %g px/mm not those on paper\n", zoom);
      ++failures;
    }
  }
  // A chain line at 0.5 px/mm (2 mm wide): its long dashes kept, its
  // 0.5 mm dot, shorter than the widening, a dot at the same middle; the
  // period (15.5 mm) kept.
  const QPen chainOwn = DrawingSheetView::sheetPen(pens, QStringLiteral("chain"), 50.0);
  const QPen chain = DrawingSheetView::sheetPen(pens, QStringLiteral("chain"), 0.5);
  const QVector<QPointF> chainPaper = paintedDashes(chainOwn, 60.0);
  const QVector<QPointF> chainScreen = paintedDashes(chain, 60.0);
  CHECK(chainPaper.size() == chainScreen.size());
  for (int i = 0; i < std::min(chainPaper.size(), chainScreen.size()); ++i) {
    const QPointF a = chainPaper[i];
    const QPointF b = chainScreen[i];
    if (a.y() - a.x() > 2.0) { // a long dash: the same stretch
      CHECK(std::abs(a.x() - b.x()) < 1e-6 && std::abs(a.y() - b.y()) < 1e-6);
    } else { // the dot: the same middle, the pen's width across
      CHECK(std::abs((a.x() + a.y()) - (b.x() + b.y())) < 1e-6);
      CHECK(std::abs((b.y() - b.x()) - 2.0) < 1e-3);
    }
  }
  // A continuous pen: only wider.
  const QPen thick = DrawingSheetView::sheetPen(pens, QStringLiteral("thick"), 1.0);
  CHECK(thick.style() == Qt::SolidLine && std::abs(thick.widthF() - 1.0) < 1e-12);

  // Painted: a hidden edge 39.5 mm long at 3 and 2 px/mm (where the pixel
  // minimum applies) shows its 10 dashes, each's middle within 1.5 px of
  // where it is on paper (1.5 mm + 4 mm steps from the line's start). It
  // ends in the middle of a gap: Qt's one-pixel lines paint the pixel that
  // holds their end point, and a line exactly 10 periods long (40 mm) had
  // the next dash start inside that pixel (0.08 mm past the end at 3 px/mm,
  // 0.16 mm at 2), painted as an 11th dash one pixel long.
  for (const double zoom : {3.0, 2.0}) {
    const QPen pen = DrawingSheetView::sheetPen(pens, QStringLiteral("hidden"), zoom);
    QImage image(static_cast<int>(std::ceil(50.0 * zoom)), 20, QImage::Format_RGB32);
    image.fill(Qt::white);
    {
      QPainter painter(&image);
      painter.translate(5.0 * zoom, 10.5);
      painter.scale(zoom, -zoom);
      painter.setPen(pen);
      painter.drawLine(QPointF(0.0, 0.0), QPointF(39.5, 0.0));
    }
    // The row the line is painted on: the one with the most ink.
    int row = 0;
    int most = -1;
    for (int y = 0; y < image.height(); ++y) {
      int ink = 0;
      for (int x = 0; x < image.width(); ++x) {
        ink += qGray(image.pixel(x, y)) < 160 ? 1 : 0;
      }
      if (ink > most) {
        most = ink;
        row = y;
      }
    }
    const QVector<QPoint> runs = inkRuns(image, row);
    CHECK(runs.size() == 10);
    if (runs.size() != 10) {
      std::fprintf(stderr, "drawing_sheet_view.cpp: %d dashes painted at %g px/mm, 10 expected\n",
                   static_cast<int>(runs.size()), zoom);
      // Where they are (pixels; the line from 5 mm to 44.5 mm), and the pen.
      for (const QPoint& run : runs) {
        std::fprintf(stderr, "drawing_sheet_view.cpp:   run %d..%d px (%.3f..%.3f mm from the line's start)\n",
                     run.x(), run.y(), run.x() / zoom - 5.0, (run.y() + 1) / zoom - 5.0);
      }
      QString dashes;
      for (const qreal d : pen.dashPattern()) {
        dashes += QStringLiteral(" %1").arg(d * pen.widthF(), 0, 'f', 4);
      }
      std::fprintf(stderr, "drawing_sheet_view.cpp:   pen %.4f mm, dashes (mm)%s, offset %.4f mm, row %d\n",
                   pen.widthF(), qPrintable(dashes), pen.dashOffset() * pen.widthF(), row);
      continue;
    }
    for (int i = 0; i < 10; ++i) {
      const double middle = (runs[i].x() + runs[i].y() + 1) / 2.0;
      const double expected = (5.0 + 1.5 + 4.0 * i) * zoom;
      CHECK(std::abs(middle - expected) <= 1.5);
    }
  }
}

} // namespace

int drawingSheetViewTests() {
  using mitcad::drawings::DrawingSheetView;
  DrawingSheetView sheet;
  sheet.resize(800, 600);
  sheet.show();
  sheet.setSheet(sheetOf(297.0, 210.0));
  const QPointF probe(100.0, 100.0);
  const QPointF fitted = sheet.toWidget(probe);

  // A wheel zoom, then the model's next answer of the same size: the zoom
  // stays (mitcad#751).
  wheel(sheet, QPointF(400.0, 300.0), 2);
  const QPointF zoomed = sheet.toWidget(probe);
  CHECK(!same(zoomed, fitted));
  sheet.setSheet(sheetOf(297.0, 210.0));
  CHECK(same(sheet.toWidget(probe), zoomed));
  // A pan with the middle button, kept too.
  mouse(sheet, QEvent::MouseButtonPress, QPointF(300.0, 300.0), Qt::MiddleButton, Qt::MiddleButton);
  mouse(sheet, QEvent::MouseMove, QPointF(340.0, 320.0), Qt::NoButton, Qt::MiddleButton);
  mouse(sheet, QEvent::MouseButtonRelease, QPointF(340.0, 320.0), Qt::MiddleButton, Qt::NoButton);
  const QPointF panned = sheet.toWidget(probe);
  CHECK(same(panned, zoomed + QPointF(40.0, 20.0)));
  sheet.setSheet(sheetOf(297.0, 210.0));
  CHECK(same(sheet.toWidget(probe), panned));
  // Another size: the whole sheet again.
  sheet.setSheet(sheetOf(420.0, 297.0));
  const QPointF larger = sheet.toWidget(probe);
  CHECK(!same(larger, panned));
  sheet.fit();
  CHECK(same(sheet.toWidget(probe), larger));

  // The sheet painted once into its picture; moves of the pointer over it
  // (the view under it changing) paint over the picture without painting
  // the sheet again (mitcad#759).
  const QPixmap first = sheet.grab();
  CHECK(!first.isNull());
  const int painted = sheet.sheetPaints();
  CHECK(painted >= 1);
  // The filled square is on the picture, the paper white beside it.
  const QImage firstImage = first.toImage();
  CHECK(qGray(firstImage.pixel(sheet.toWidget(QPointF(55.0, 40.0)).toPoint())) < 64);
  CHECK(qGray(firstImage.pixel(sheet.toWidget(QPointF(200.0, 150.0)).toPoint())) > 192);
  for (const QPointF& at : {QPointF(10.0, 10.0), sheet.toWidget(QPointF(90.0, 35.0)), QPointF(15.0, 20.0)}) {
    mouse(sheet, QEvent::MouseMove, at, Qt::NoButton, Qt::NoButton);
    const QPixmap hovered = sheet.grab();
    CHECK(!hovered.isNull());
  }
  CHECK(sheet.sheetPaints() == painted);
  // A zoom paints it again, once.
  wheel(sheet, QPointF(400.0, 300.0), -1);
  const QPixmap after = sheet.grab();
  CHECK(!after.isNull());
  CHECK(sheet.sheetPaints() == painted + 1);

  // Corner dots (mitcad#1338): a filled dot at each corner given, on the
  // screen only, gone when they are cleared; the corner under the pointer
  // a filled marker.
  sheet.fit();
  const QPointF corner(200.0, 150.0); // on the paper, away from the items
  const QPoint pixel = sheet.toWidget(corner).toPoint();
  const auto grey = [&sheet, pixel] { return qGray(sheet.grab().toImage().pixel(pixel)); };
  CHECK(grey() > 192);
  sheet.setCornerDots({corner}, 3.0);
  CHECK(sheet.cornerDotCount() == 1);
  CHECK(grey() < 192);
  sheet.setCornerDots({}, 3.0);
  CHECK(sheet.cornerDotCount() == 0);
  CHECK(grey() > 192);
  QJsonArray hovered;
  hovered.append(QJsonObject{{QStringLiteral("point"), xy(corner.x(), corner.y())}});
  sheet.setHoverMarks(hovered);
  CHECK(grey() < 192);
  sheet.setHoverMarks(QJsonArray());
  CHECK(grey() > 192);

  // The marks of a line hovered and picked (mitcad#1618): as wide as the
  // line is drawn at the zoom (its pen's width, here 0.7 mm)
  // and 2 pixels more on each side, fitted and zoomed in; without a pen,
  // the fixed widths.
  {
    QJsonObject withPens = sheetOf(420.0, 297.0);
    withPens.insert(QStringLiteral("pens"),
                    QJsonObject{{QStringLiteral("thick"), QJsonObject{{QStringLiteral("width"), 0.7}}}});
    sheet.setSheet(withPens);
    sheet.fit();
    QJsonArray edge;
    edge.append(xy(20.0, 20.0));
    edge.append(xy(120.0, 20.0));
    const QJsonObject marked{{QStringLiteral("line"), edge}, {QStringLiteral("pen"), QStringLiteral("thick")}};
    const QJsonObject bare{{QStringLiteral("line"), edge}};
    const auto expected = [&sheet] {
      return std::max(0.7 * sheet.zoom(), 1.0) + 2.0 * DrawingSheetView::kMarkMargin;
    };
    const double fittedZoom = sheet.zoom();
    CHECK(std::abs(sheet.markWidth(marked, false) - std::max(expected(), DrawingSheetView::kHoverMarkWidth)) < 1e-9);
    CHECK(std::abs(sheet.markWidth(marked, true) - std::max(expected(), DrawingSheetView::kPickMarkWidth)) < 1e-9);
    CHECK(sheet.markWidth(bare, false) == DrawingSheetView::kHoverMarkWidth);
    CHECK(sheet.markWidth(bare, true) == DrawingSheetView::kPickMarkWidth);
    // Zoomed in about the edge's middle: wider with the line.
    const QPointF middle(70.0, 20.0);
    wheel(sheet, sheet.toWidget(middle), 16);
    CHECK(sheet.zoom() > 4.0 * fittedZoom);
    const double drawn = 0.7 * sheet.zoom();
    CHECK(drawn > 8.0);
    CHECK(std::abs(sheet.markWidth(marked, false) - (drawn + 4.0)) < 1e-9);
    CHECK(std::abs(sheet.markWidth(marked, true) - (drawn + 4.0)) < 1e-9);
    CHECK(sheet.markWidth(bare, true) == DrawingSheetView::kPickMarkWidth);
    // On the screen, the pixel whose centre lies midway between the drawn
    // line's edge and the mark's (1 pixel from each): the paper without
    // the mark, the hover's blue or the pick's orange with it; the line
    // itself still dark under the mark.
    const QPointF centre = sheet.toWidget(middle);
    const QPoint beside(static_cast<int>(std::floor(centre.x())),
                        static_cast<int>(std::floor(centre.y() + drawn / 2.0 + 1.0)));
    const QPoint onLine(static_cast<int>(std::floor(centre.x())), static_cast<int>(std::floor(centre.y())));
    const auto colourAt = [&sheet](const QPoint& at) { return QColor(sheet.grab().toImage().pixel(at)); };
    CHECK(qGray(colourAt(beside).rgb()) > 192);
    CHECK(qGray(colourAt(onLine).rgb()) < 64);
    QJsonArray markedItems;
    markedItems.append(marked);
    sheet.setHoverMarks(markedItems);
    const QColor hoveredBeside = colourAt(beside);
    CHECK(hoveredBeside.blue() > hoveredBeside.red() + 40);
    CHECK(qGray(colourAt(onLine).rgb()) < 128);
    sheet.setHoverMarks(QJsonArray());
    sheet.setPickMarks(markedItems);
    const QColor pickedBeside = colourAt(beside);
    CHECK(pickedBeside.red() > pickedBeside.blue() + 40);
    CHECK(qGray(colourAt(onLine).rgb()) < 128);
    sheet.setPickMarks(QJsonArray());
    CHECK(qGray(colourAt(beside).rgb()) > 192);
    sheet.fit();
  }

  // A section's line and a break's sides (mitcad#1362): each click snaps
  // to the point the model gives (here a hole's centre at (50, 35) and a
  // corner at (90, 37)), asked on the view of the first click; a section's
  // second near none of them turns onto a multiple of 45 degrees from its
  // first; Shift: as clicked.
  using mitcad::drawings::SheetPick;
  using mitcad::drawings::ToolPicks;
  QVector<ToolPicks> got;
  QObject::connect(&sheet, &DrawingSheetView::picked, &sheet, [&got](const ToolPicks& p) { got.append(p); });
  QVector<int> askedOn;
  sheet.setPointSnap([&askedOn](int on, const QPointF& at) -> std::optional<QPointF> {
    askedOn.append(on);
    for (const QPointF& p : {QPointF(50.0, 35.0), QPointF(90.0, 37.0)}) {
      if (std::hypot(at.x() - p.x(), at.y() - p.y()) < 1.0) {
        return p;
      }
    }
    return std::nullopt;
  });
  const auto click = [&sheet](const QPointF& at, Qt::KeyboardModifiers modifiers = Qt::NoModifier) {
    const QPointF w = sheet.toWidget(at);
    mouse(sheet, QEvent::MouseButtonPress, w, Qt::LeftButton, Qt::LeftButton, modifiers);
    mouse(sheet, QEvent::MouseButtonRelease, w, Qt::LeftButton, Qt::NoButton, modifiers);
  };
  const QPointF nearCentre(50.4, 35.3);
  const QPointF placed(200.0, 150.0);
  sheet.setPick(SheetPick::Line, true);
  // Through the centre, its second click off the view, 1.9° off upright:
  // upright, as long as it was.
  click(nearCentre);
  click(QPointF(51.0, 65.0));
  CHECK(sheet.isPlacing());
  click(placed);
  CHECK(got.size() == 1);
  if (got.size() == 1) {
    const ToolPicks& line = got[0];
    CHECK(line.view == 1 && line.points.size() == 3);
    if (line.points.size() == 3) {
      CHECK(closeTo(line.points[0], QPointF(50.0, 35.0)));
      CHECK(closeTo(line.points[1], QPointF(50.0, 35.0 + std::hypot(1.0, 30.0))));
      CHECK(closeTo(line.points[2], placed));
    }
  }
  CHECK(askedOn.size() == 2 && askedOn.value(0) == 1 && askedOn.value(1) == 1);
  // Near 45°: 45°.
  click(nearCentre);
  click(QPointF(70.0, 56.0));
  click(placed);
  CHECK(got.size() == 2);
  if (got.size() == 2 && got[1].points.size() == 3) {
    const double r = std::hypot(20.0, 21.0) / std::sqrt(2.0);
    CHECK(closeTo(got[1].points[1], QPointF(50.0 + r, 35.0 + r)));
  }
  // The corner 3.3° off level: its point wins over the angle.
  click(nearCentre);
  click(QPointF(90.3, 37.3));
  click(placed);
  CHECK(got.size() == 3);
  if (got.size() == 3 && got[2].points.size() == 3) {
    CHECK(closeTo(got[2].points[1], QPointF(90.0, 37.0)));
  }
  // Shift held: where it is clicked, nothing asked.
  const qsizetype asked = askedOn.size();
  click(nearCentre, Qt::ShiftModifier);
  click(QPointF(51.0, 55.0), Qt::ShiftModifier);
  click(placed);
  CHECK(askedOn.size() == asked);
  CHECK(got.size() == 4);
  if (got.size() == 4 && got[3].points.size() == 3) {
    CHECK(closeTo(got[3].points[0], nearCentre));
    CHECK(closeTo(got[3].points[1], QPointF(51.0, 55.0)));
  }
  // A break: its sides snap to points, the second never turns.
  sheet.setPick(SheetPick::Line, false);
  click(nearCentre);
  click(QPointF(51.0, 55.0));
  CHECK(got.size() == 5);
  if (got.size() == 5 && got[4].points.size() == 2) {
    CHECK(closeTo(got[4].points[0], QPointF(50.0, 35.0)));
    CHECK(closeTo(got[4].points[1], QPointF(51.0, 55.0)));
  }
  sheet.setPick(SheetPick::None, false);

  sheetPenTests();
  return failures;
}
MITCAD_UNIT_TEST(drawingSheetViewTests);
