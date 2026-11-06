// SPDX-License-Identifier: MIT
// A drawing's sheet (mitcad#707) on Qt's offscreen platform: a wheel zoom
// or a pan kept when the model's next answer brings a sheet of the same
// size, the whole sheet fitted for another size (mitcad#751); the sheet
// itself painted from its picture while the pointer only moves over it,
// painted again for a zoom (mitcad#759).
#include "drawings/DrawingSheetView.hpp"

#include <cmath>
#include <cstdio>
#include <initializer_list>

#include <QApplication>
#include <QColor>
#include <QImage>
#include <QJsonArray>
#include <QJsonObject>
#include <QMouseEvent>
#include <QPixmap>
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

void mouse(QWidget& widget, QEvent::Type type, const QPointF& at, Qt::MouseButton button, Qt::MouseButtons buttons) {
  QMouseEvent event(type, at, widget.mapToGlobal(at), button, buttons, Qt::NoModifier);
  QApplication::sendEvent(&widget, &event);
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
  return failures;
}
