// SPDX-License-Identifier: MIT
// A drawing's sheet (mitcad#707) on Qt's offscreen platform: a wheel zoom
// or a pan kept when the model's next answer brings a sheet of the same
// size, the whole sheet fitted for another size (mitcad#751); the sheet
// itself painted from its picture while the pointer only moves over it,
// painted again for a zoom (mitcad#759); a section's line's and a break's
// clicks snapped to points, a section's second to 45 degrees, Shift as
// clicked (mitcad#1362).
#include "drawings/DrawingSheetView.hpp"

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
#include <QPixmap>
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
  return failures;
}
