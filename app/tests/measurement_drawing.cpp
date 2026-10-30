// SPDX-License-Identifier: MIT
// Measurement objects drawn as dimensions (mitcad#649): the `measurements`
// query's entries to what is drawn (hidden and broken ones left out), the
// values in words in the document's unit, a length's, a radius', a
// diameter's and an angle's layout through a projection and at a larger
// picture's scale, and the projection of a final render's camera. The look
// of a technical drawing (mitcad#894): texts readable from any side (ISO
// 129-1), no card behind a value, closed 30-degree arrowheads sized by the
// text's height, the model's text height with its pixel floor, Auto's
// colour of the background tone. The reference look (mitcad#902): no band
// or box under a selected or hovered one, lines on the pixel grid, the
// text between its floor and ceiling, arrowheads and centre marks in the
// plane, extension lines from the measured point, frames two text heights
// high, the bundled lettering, a picture's own background tone, hole
// callouts and a frame under one.
#include "view/MeasurementDrawing.hpp"

#include <array>
#include <cmath>
#include <cstdio>
#include <initializer_list>
#include <optional>
#include <utility>
#include <vector>

#include <QCoreApplication>
#include <QDir>
#include <QFileInfo>
#include <QImage>
#include <QJsonArray>
#include <QJsonObject>
#include <QPainter>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "measurement_drawing.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

bool near(double a, double b, double tolerance = 1e-6) { return std::abs(a - b) <= tolerance; }

QJsonArray xyz(double x, double y, double z) { return QJsonArray{x, y, z}; }

// A list of one segment. MSVC takes QJsonArray{QJsonArray{p, q}} for a copy of
// the inner array (one element in braces), not a list holding it.
QJsonArray oneSegment(const QJsonArray& p, const QJsonArray& q) {
  QJsonArray list;
  list.append(QJsonArray{p, q});
  return list;
}

// Looking down Z: x to the right, y up, 10 pixels per mm, the origin at
// (100, 100).
std::optional<QPointF> top(const gp_Pnt& p) { return QPointF(100.0 + 10.0 * p.X(), 100.0 - 10.0 * p.Y()); }

double lengthOf(const QPointF& p) { return std::hypot(p.x(), p.y()); }

// Whether a pixel of the picture is about `colour` (antialiased edges blend
// into what is under them).
bool paintedIn(const QImage& image, const QColor& colour, int tolerance = 40) {
  for (int y = 0; y < image.height(); ++y) {
    for (int x = 0; x < image.width(); ++x) {
      const QColor at = image.pixelColor(x, y);
      if (at.alpha() > 200 && std::abs(at.red() - colour.red()) <= tolerance &&
          std::abs(at.green() - colour.green()) <= tolerance && std::abs(at.blue() - colour.blue()) <= tolerance) {
        return true;
      }
    }
  }
  return false;
}

bool nearColour(const QColor& a, const QColor& b, int tolerance = 40) {
  return std::abs(a.red() - b.red()) <= tolerance && std::abs(a.green() - b.green()) <= tolerance &&
         std::abs(a.blue() - b.blue()) <= tolerance;
}

// A laid out text's axes on the picture: along it and down it, a picture
// pixel per text pixel face on.
struct TextAxes {
  QPointF along;
  QPointF down;
  // Its frame (x right, y down) turned over on the picture: mirrored.
  bool mirrored = false;
};

TextAxes textAxes(const mitcad::MeasurementShape& shape) {
  const QPointF at = shape.textTransform.map(QPointF(0.0, 0.0));
  TextAxes axes;
  axes.along = shape.textTransform.map(QPointF(1.0, 0.0)) - at;
  axes.down = shape.textTransform.map(QPointF(0.0, 1.0)) - at;
  axes.mirrored = axes.along.x() * axes.down.y() - axes.along.y() * axes.down.x() <= 0.0;
  return axes;
}

} // namespace

int measurementDrawingTests() {
  using namespace mitcad;
  const auto words = [](const QString& kind, double value) {
    return measurementValueText(kind, value, QStringLiteral("mm"), 1.0);
  };

  // Values in words.
  CHECK(measurementValueText(QStringLiteral("distance"), 40.0, QStringLiteral("mm"), 1.0) == QStringLiteral("40 mm"));
  CHECK(measurementValueText(QStringLiteral("offset"), 12.5, QStringLiteral("cm"), lengthUnitMillimetres(QStringLiteral("cm"))) ==
        QStringLiteral("1.25 cm"));
  CHECK(measurementValueText(QStringLiteral("radius"), 25.4, QStringLiteral("in"), lengthUnitMillimetres(QStringLiteral("in"))) ==
        QStringLiteral("1 in"));
  CHECK(measurementValueText(QStringLiteral("angle"), std::acos(-1.0) / 2.0, QStringLiteral("mm"), 1.0) ==
        QStringLiteral("90°"));
  CHECK(near(lengthUnitMillimetres(QStringLiteral("ft")), 304.8));
  CHECK(near(lengthUnitMillimetres(QStringLiteral("mm")), 1.0));

  // The query's entries: hidden and broken ones are not drawn.
  const QJsonArray entries{
      QJsonObject{{QStringLiteral("name"), QStringLiteral("Distance1")},
                  {QStringLiteral("kind"), QStringLiteral("distance")},
                  {QStringLiteral("color"), QStringLiteral("#d9480f")},
                  {QStringLiteral("visible"), true},
                  {QStringLiteral("value"), 4.0},
                  {QStringLiteral("points"), QJsonArray{xyz(0, 0, 0), xyz(4, 0, 0)}}},
      QJsonObject{{QStringLiteral("name"), QStringLiteral("Hidden")},
                  {QStringLiteral("kind"), QStringLiteral("distance")},
                  {QStringLiteral("color"), QStringLiteral("#1971c2")},
                  {QStringLiteral("visible"), false},
                  {QStringLiteral("value"), 4.0},
                  {QStringLiteral("points"), QJsonArray{xyz(0, 0, 0), xyz(4, 0, 0)}}},
      QJsonObject{{QStringLiteral("name"), QStringLiteral("Broken")},
                  {QStringLiteral("kind"), QStringLiteral("offset")},
                  {QStringLiteral("color"), QStringLiteral("#2f9e44")},
                  {QStringLiteral("visible"), true},
                  {QStringLiteral("error"), QStringLiteral("a pick is not there: body F2.b0")}},
      QJsonObject{{QStringLiteral("name"), QStringLiteral("Diameter1")},
                  {QStringLiteral("kind"), QStringLiteral("diameter")},
                  {QStringLiteral("color"), QStringLiteral("#7048e8")},
                  {QStringLiteral("value"), 10.0},
                  {QStringLiteral("points"), QJsonArray{xyz(-5, 0, 0), xyz(5, 0, 0)}},
                  {QStringLiteral("center"), xyz(0, 0, 0)},
                  {QStringLiteral("axis"), xyz(0, 0, 1)},
                  {QStringLiteral("radius"), 5.0}},
      QJsonObject{{QStringLiteral("name"), QStringLiteral("Angle1")},
                  {QStringLiteral("kind"), QStringLiteral("angle")},
                  {QStringLiteral("color"), QStringLiteral("#c2255c")},
                  {QStringLiteral("value"), std::acos(-1.0) / 2.0},
                  {QStringLiteral("arc"), QJsonObject{{QStringLiteral("center"), xyz(0, 0, 0)},
                                                       {QStringLiteral("from"), xyz(1, 0, 0)},
                                                       {QStringLiteral("to"), xyz(0, 1, 0)}}}}};
  const std::vector<MeasurementDisplay> shown = measurementDisplays(entries, words);
  CHECK(shown.size() == 3);
  if (shown.size() != 3) {
    return failures;
  }
  CHECK(shown[0].label == QStringLiteral("Distance1 = 4 mm"));
  CHECK(shown[0].color == QColor(0xd9, 0x48, 0x0f));
  CHECK(shown[0].points.size() == 2);
  CHECK(shown[1].name == QStringLiteral("Diameter1") && shown[1].circle && near(shown[1].radius, 5.0));
  CHECK(shown[2].label == QStringLiteral("Angle1 = 90°") && shown[2].arc);

  // A length: extension lines from both ends, the dimension line above
  // them with an arrow at each end, the label above its middle.
  const MeasurementShape length = layoutMeasurement(shown[0], top);
  CHECK(length.valid);
  CHECK(length.lines.size() == 3);
  CHECK(length.arrows.size() == 2);
  if (length.lines.size() == 3) {
    const QLineF line = length.lines[2];
    CHECK(near(line.length(), 40.0));
    CHECK(near(line.p1().y(), line.p2().y()) && line.p1().y() < 100.0);
    CHECK(length.labelAt.y() < line.p1().y());
    CHECK(near(length.labelAt.x(), 120.0));
  }
  // Twice as large: everything twice as far.
  const auto twice = [](const gp_Pnt& p) -> std::optional<QPointF> { return *top(p) * 2.0; };
  const MeasurementShape larger = layoutMeasurement(shown[0], twice, 2.0);
  CHECK(larger.valid && larger.lines.size() == 3);
  if (larger.lines.size() == 3 && length.lines.size() == 3) {
    CHECK(near(larger.lines[2].p1().y(), 2.0 * length.lines[2].p1().y()));
  }

  // A diameter across the circle where it shows longest, arrows out.
  const MeasurementShape diameter = layoutMeasurement(shown[1], top);
  CHECK(diameter.valid);
  CHECK(diameter.arrows.size() == 2);
  bool across = false;
  for (const QLineF& line : diameter.lines) {
    across = across || near(line.length(), 100.0, 1e-6);
  }
  CHECK(across);
  // A radius from the centre to the rim.
  MeasurementDisplay radius = shown[1];
  radius.kind = QStringLiteral("radius");
  const MeasurementShape radial = layoutMeasurement(radius, top);
  CHECK(radial.valid && radial.arrows.size() == 1);
  if (radial.arrows.size() == 1) {
    CHECK(near(lengthOf(radial.arrows[0].first - QPointF(100.0, 100.0)), 50.0, 1e-6));
  }

  // An angle: an arc of five text heights (45 pixels at the floor of 9)
  // about the corner from one ray to the other, the rays drawn past it,
  // arrows at its ends.
  const MeasurementShape angle = layoutMeasurement(shown[2], top);
  CHECK(angle.valid);
  CHECK(angle.arcs.size() == 1);
  CHECK(angle.arrows.size() == 2);
  CHECK(angle.lines.size() == 2);
  if (angle.arcs.size() == 1) {
    for (const QPointF& at : angle.arcs[0]) {
      CHECK(near(lengthOf(at - QPointF(100.0, 100.0)), 45.0, 1e-6));
    }
    CHECK(near(angle.arcs[0].first().x(), 145.0) && near(angle.arcs[0].last().y(), 55.0));
  }
  // Points that cannot be drawn: nothing.
  const MeasurementShape none = layoutMeasurement(shown[0], [](const gp_Pnt&) { return std::optional<QPointF>(); });
  CHECK(!none.valid);

  // Painting them does not fail, and draws in their colours.
  QImage image(240, 200, QImage::Format_ARGB32);
  image.fill(Qt::white);
  {
    QPainter painter(&image);
    paintMeasurements(painter, {length, diameter, angle}, QColor(250, 250, 252), measurementFont(QFont()));
  }
  CHECK(paintedIn(image, QColor(0xd9, 0x48, 0x0f)));

  // A final render's camera: from the front, 100 mm high (half_height 50)
  // in a 200 x 100 image, a pixel per mm.
  const QJsonObject front{{QStringLiteral("projection"), QStringLiteral("orthographic")},
                          {QStringLiteral("eye"), xyz(0, -100, 0)},
                          {QStringLiteral("target"), xyz(0, 0, 0)},
                          {QStringLiteral("up"), xyz(0, 0, 1)},
                          {QStringLiteral("half_height"), 50.0},
                          {QStringLiteral("size"), QJsonArray{200, 100}}};
  const MeasurementProjection camera = renderViewProjection(front);
  const std::optional<QPointF> middle = camera(gp_Pnt(0, 0, 0));
  CHECK(middle && near(middle->x(), 100.0) && near(middle->y(), 50.0));
  const std::optional<QPointF> high = camera(gp_Pnt(50, 30, 50));
  CHECK(high && near(high->x(), 150.0) && near(high->y(), 0.0));
  // Seen in perspective, a point behind the camera cannot be drawn, and one
  // twice as far away is half as far from the middle.
  QJsonObject perspective = front;
  perspective.insert(QStringLiteral("projection"), QStringLiteral("perspective"));
  const MeasurementProjection lens = renderViewProjection(perspective);
  CHECK(!lens(gp_Pnt(0, -200, 0)));
  const std::optional<QPointF> far = lens(gp_Pnt(0, 100, 50));
  CHECK(far && near(far->y(), 25.0));

  // Annotations in their planes (mitcad#702): a length of 40 along X in
  // the plane square to Z, its dimension line 10 off along Y, the text at
  // its middle running along X; the model's label.
  const QJsonObject planar{
      {QStringLiteral("name"), QStringLiteral("Distance2")},
      {QStringLiteral("kind"), QStringLiteral("distance")},
      {QStringLiteral("color"), QStringLiteral("#2f9e44")},
      {QStringLiteral("group"), QStringLiteral("Shop")},
      {QStringLiteral("value"), 40.0},
      {QStringLiteral("points"), QJsonArray{xyz(0, 0, 0), xyz(40, 0, 0)}},
      {QStringLiteral("label"), QJsonObject{{QStringLiteral("text"), QStringLiteral("40 +0.2/-0.1")},
                                            {QStringLiteral("main"), QStringLiteral("40")},
                                            {QStringLiteral("upper"), QStringLiteral("+0.2")},
                                            {QStringLiteral("lower"), QStringLiteral("-0.1")},
                                            {QStringLiteral("brackets"), false}}},
      {QStringLiteral("drawing"),
       QJsonObject{
           {QStringLiteral("plane"), QJsonObject{{QStringLiteral("origin"), xyz(0, 0, 0)},
                                                 {QStringLiteral("x"), xyz(1, 0, 0)},
                                                 {QStringLiteral("y"), xyz(0, 1, 0)},
                                                 {QStringLiteral("normal"), xyz(0, 0, 1)}}},
           {QStringLiteral("line_offset"), 10.0},
           {QStringLiteral("text_offset"), QJsonArray{0.0, 0.0}},
           {QStringLiteral("extension_lines"),
            QJsonArray{QJsonArray{xyz(0, 0, 0), xyz(0, 10, 0)}, QJsonArray{xyz(40, 0, 0), xyz(40, 10, 0)}}},
           {QStringLiteral("dimension_lines"), oneSegment(xyz(0, 10, 0), xyz(40, 10, 0))},
           {QStringLiteral("arcs"), QJsonArray{}},
           {QStringLiteral("arrows"),
            QJsonArray{QJsonObject{{QStringLiteral("tip"), xyz(0, 10, 0)}, {QStringLiteral("direction"), xyz(-1, 0, 0)}},
                       QJsonObject{{QStringLiteral("tip"), xyz(40, 10, 0)}, {QStringLiteral("direction"), xyz(1, 0, 0)}}}},
           {QStringLiteral("text"), QJsonObject{{QStringLiteral("at"), xyz(20, 10, 0)},
                                                {QStringLiteral("direction"), xyz(1, 0, 0)},
                                                {QStringLiteral("up"), xyz(0, 1, 0)}}},
           // 1.2 mm: 12 pixels at 10 pixels a millimetre (mitcad#894).
           {QStringLiteral("text_height"), 1.2}}}};
  const QJsonObject note{
      {QStringLiteral("name"), QStringLiteral("Note1")},
      {QStringLiteral("kind"), QStringLiteral("note")},
      {QStringLiteral("color"), QStringLiteral("#7048e8")},
      {QStringLiteral("text"), QStringLiteral("Deburr\nall edges")},
      {QStringLiteral("label"), QJsonObject{{QStringLiteral("text"), QStringLiteral("Deburr / all edges")},
                                            {QStringLiteral("lines"), QJsonArray{QStringLiteral("Deburr"),
                                                                                 QStringLiteral("all edges")}}}},
      {QStringLiteral("drawing"),
       QJsonObject{
           {QStringLiteral("plane"), QJsonObject{{QStringLiteral("origin"), xyz(0, 0, 0)},
                                                 {QStringLiteral("x"), xyz(1, 0, 0)},
                                                 {QStringLiteral("y"), xyz(0, 1, 0)},
                                                 {QStringLiteral("normal"), xyz(0, 0, 1)}}},
           {QStringLiteral("line_offset"), 0.0},
           {QStringLiteral("text_offset"), QJsonArray{10.0, 10.0}},
           {QStringLiteral("dimension_lines"), oneSegment(xyz(10, 10, 0), xyz(0, 0, 0))},
           {QStringLiteral("arrows"),
            QJsonArray{QJsonObject{{QStringLiteral("tip"), xyz(0, 0, 0)}, {QStringLiteral("direction"), xyz(-1, -1, 0)}}}},
           {QStringLiteral("text"), QJsonObject{{QStringLiteral("at"), xyz(10, 10, 0)},
                                                {QStringLiteral("direction"), xyz(1, 0, 0)},
                                                {QStringLiteral("up"), xyz(0, 1, 0)}}}}}};
  const std::vector<MeasurementDisplay> drawn = measurementDisplays(QJsonArray{planar, note}, words);
  CHECK(drawn.size() == 2);
  if (drawn.size() != 2) {
    return failures;
  }
  CHECK(drawn[0].planar && drawn[0].label == QStringLiteral("40 +0.2/-0.1") && drawn[0].group == QStringLiteral("Shop"));
  CHECK(drawn[0].upper == QStringLiteral("+0.2") && drawn[0].lower == QStringLiteral("-0.1"));
  CHECK(drawn[0].extensionLines.size() == 2 && drawn[0].dimensionLines.size() == 1 && drawn[0].arrows.size() == 2);
  CHECK(near(drawn[0].lineOffset, 10.0));
  CHECK(near(drawn[0].textHeight, 1.2) && !drawn[0].autoColor);
  CHECK(drawn[1].planar && drawn[1].lines.size() == 2);

  // Face on: drawn whole, the text along X, upright, a pixel per pixel.
  const MeasurementShape face = layoutMeasurement(drawn[0], top);
  CHECK(face.valid && face.planar && near(face.opacity, 1.0));
  CHECK(face.lines.size() == 3 && face.arrows.size() == 2 && face.grips.size() == 1);
  const QPointF origin = face.textTransform.map(QPointF(0.0, 0.0));
  CHECK(near(origin.x(), 300.0, 1e-6) && near(origin.y(), 0.0, 1e-6));
  const QPointF along = face.textTransform.map(QPointF(1.0, 0.0)) - origin;
  const QPointF down = face.textTransform.map(QPointF(0.0, 1.0)) - origin;
  CHECK(near(along.x(), 1.0, 1e-6) && near(along.y(), 0.0, 1e-6));
  CHECK(near(down.x(), 0.0, 1e-6) && near(down.y(), 1.0, 1e-6));
  // The text above the line takes clicks; the line away from it too.
  CHECK(measurementHit(face, face.labelAt) == 1);
  CHECK(face.labelAt.y() < 0.0);
  CHECK(measurementHit(face, QPointF(130.0, 0.0)) == 2);
  CHECK(measurementHit(face, QPointF(130.0, 60.0)) == 0);
  // A drag in pixels into millimetres along the plane's axes.
  const std::optional<std::array<double, 2>> dragged = measurementDragMillimetres(face, QPointF(30.0, -20.0));
  CHECK(dragged && near((*dragged)[0], 3.0, 1e-6) && near((*dragged)[1], 2.0, 1e-6));
  const MeasurementShape movedText = draggedMeasurement(face, false, QPointF(30.0, 0.0));
  CHECK(near(movedText.labelAt.x(), face.labelAt.x() + 30.0, 1e-6));
  const MeasurementShape movedLine = draggedMeasurement(face, true, QPointF(30.0, -20.0));
  CHECK(near(movedLine.grips.front().first().y(), face.grips.front().first().y() - 20.0, 1e-6));
  CHECK(near(movedLine.grips.front().first().x(), face.grips.front().first().x(), 1e-6));
  // Seen obliquely, the plane's axes are not at right angles on the
  // picture: the line's preview shifts as the drag is committed (its part
  // along y of the drag split into the axes), not by the drag's
  // perpendicular projection on y (mitcad#759).
  const auto oblique = [](const gp_Pnt& p) -> std::optional<QPointF> {
    return QPointF(100.0 + 10.0 * p.X() + 6.0 * p.Y(), 100.0 - 4.0 * p.Y() - 10.0 * p.Z());
  };
  const MeasurementShape skewed = layoutMeasurement(drawn[0], oblique);
  CHECK(skewed.valid && skewed.planar && !skewed.grips.empty());
  if (skewed.valid && !skewed.grips.empty()) {
    for (const QPointF& drag : {QPointF(0.0, -10.0), QPointF(10.0, 0.0), QPointF(7.0, -3.0)}) {
      const std::optional<std::array<double, 2>> committed = measurementDragMillimetres(skewed, drag);
      CHECK(committed.has_value());
      if (!committed) {
        continue;
      }
      const QPointF expected = skewed.yPerMillimetre * (*committed)[1];
      const MeasurementShape shifted = draggedMeasurement(skewed, true, drag);
      const QPointF moved = shifted.grips.front().first() - skewed.grips.front().first();
      CHECK(near(moved.x(), expected.x(), 1e-6) && near(moved.y(), expected.y(), 1e-6));
      CHECK(near(shifted.labelAt.x() - skewed.labelAt.x(), expected.x(), 1e-6) &&
            near(shifted.labelAt.y() - skewed.labelAt.y(), expected.y(), 1e-6));
    }
    // A drag along the line (its x axis) commits no shift, and shows none.
    const MeasurementShape alongLine = draggedMeasurement(skewed, true, skewed.xPerMillimetre * 2.0);
    CHECK(near(alongLine.grips.front().first().x(), skewed.grips.front().first().x(), 1e-6) &&
          near(alongLine.grips.front().first().y(), skewed.grips.front().first().y(), 1e-6));
  }

  // Seen from below (the picture mirrored) and from behind (right to
  // left): the text still reads left to right, upright.
  const auto below = [](const gp_Pnt& p) -> std::optional<QPointF> {
    return QPointF(100.0 + 10.0 * p.X(), 100.0 + 10.0 * p.Y());
  };
  const auto behind = [](const gp_Pnt& p) -> std::optional<QPointF> {
    return QPointF(100.0 - 10.0 * p.X(), 100.0 - 10.0 * p.Y());
  };
  for (const MeasurementProjection& seen : {MeasurementProjection(below), MeasurementProjection(behind)}) {
    const MeasurementShape turned = layoutMeasurement(drawn[0], seen);
    const QPointF at = turned.textTransform.map(QPointF(0.0, 0.0));
    const QPointF right = turned.textTransform.map(QPointF(1.0, 0.0)) - at;
    const QPointF up = turned.textTransform.map(QPointF(0.0, -1.0)) - at;
    CHECK(turned.valid && right.x() > 0.9 && up.y() < -0.9);
  }
  // Edge on (looking along Y at a plane square to Z): faded away, no
  // clicks.
  const auto side = [](const gp_Pnt& p) -> std::optional<QPointF> {
    return QPointF(100.0 + 10.0 * p.X(), 100.0 - 10.0 * p.Z());
  };
  const MeasurementShape edgeOn = layoutMeasurement(drawn[0], side);
  CHECK(edgeOn.valid && near(edgeOn.opacity, 0.0));
  CHECK(measurementHit(edgeOn, edgeOn.labelAt) == 0);

  // A note: its leader and arrow, its lines starting at the leader's end.
  const MeasurementShape noted = layoutMeasurement(drawn[1], top);
  CHECK(noted.valid && noted.note && noted.noteLines.size() == 2);
  CHECK(noted.lines.size() == 1 && noted.arrows.size() == 1 && noted.grips.empty());
  CHECK(!noted.rightAligned);
  CHECK(measurementHit(noted, noted.labelAt) == 1);

  // Painting them, highlighted too: in the selection's colour, not its own.
  QImage planarImage(420, 240, QImage::Format_ARGB32);
  planarImage.fill(Qt::white);
  {
    MeasurementShape highlighted = face;
    highlighted.highlighted = true;
    QPainter painter(&planarImage);
    paintMeasurements(painter, {highlighted, noted, edgeOn}, QColor(250, 250, 252), measurementFont(QFont()));
  }
  CHECK(paintedIn(planarImage, measurementSelectionColor()));
  CHECK(!paintedIn(planarImage, QColor(0x2f, 0x9e, 0x44), 20));

  // A datum label and feature control frames (mitcad#702 phase 2): on the
  // note's leader, a letter in its square at the leader's end and a filled
  // triangle on the feature; a frame's cells in a row; a frame under the
  // dimension, centred below its line and not dragged.
  QJsonObject datum = note;
  datum.insert(QStringLiteral("name"), QStringLiteral("DatumA"));
  datum.insert(QStringLiteral("kind"), QStringLiteral("datum"));
  datum.insert(QStringLiteral("label"),
               QJsonObject{{QStringLiteral("text"), QStringLiteral("A")}, {QStringLiteral("datum"), QStringLiteral("A")}});
  QJsonObject datumDrawing = note.value(QStringLiteral("drawing")).toObject();
  datumDrawing.insert(QStringLiteral("terminator"), QStringLiteral("datum"));
  datum.insert(QStringLiteral("drawing"), datumDrawing);
  const QJsonObject frameLabel{
      {QStringLiteral("text"), QStringLiteral("⟂ | Ø0.05 Ⓜ | A")},
      {QStringLiteral("frame"), QJsonArray{QStringLiteral("⟂"), QStringLiteral("Ø0.05 Ⓜ"), QStringLiteral("A")}},
      {QStringLiteral("tolerance"), QStringLiteral("Ø0.05 Ⓜ")}};
  QJsonObject frame = note;
  frame.insert(QStringLiteral("name"), QStringLiteral("Frame1"));
  frame.insert(QStringLiteral("kind"), QStringLiteral("frame"));
  frame.insert(QStringLiteral("label"), frameLabel);
  frame.insert(QStringLiteral("frame"), QJsonObject{{QStringLiteral("characteristic"), QStringLiteral("perpendicularity")},
                                                  {QStringLiteral("value"), 0.05},
                                                  {QStringLiteral("diameter"), true},
                                                  {QStringLiteral("modifier"), QStringLiteral("M")},
                                                  {QStringLiteral("datums"), QJsonArray{QStringLiteral("A")}}});
  QJsonObject under = frame;
  under.insert(QStringLiteral("name"), QStringLiteral("Frame2"));
  under.insert(QStringLiteral("drawing"),
               QJsonObject{{QStringLiteral("plane"), QJsonObject{{QStringLiteral("origin"), xyz(20, 10, 0)},
                                                                 {QStringLiteral("x"), xyz(1, 0, 0)},
                                                                 {QStringLiteral("y"), xyz(0, 1, 0)},
                                                                 {QStringLiteral("normal"), xyz(0, 0, 1)}}},
                           {QStringLiteral("line_offset"), 0.0},
                           {QStringLiteral("text_offset"), QJsonArray{0.0, 0.0}},
                           {QStringLiteral("text"), QJsonObject{{QStringLiteral("at"), xyz(20, 10, 0)},
                                                                {QStringLiteral("direction"), xyz(1, 0, 0)},
                                                                {QStringLiteral("up"), xyz(0, 1, 0)}}},
                           {QStringLiteral("attached"), QStringLiteral("Distance2")}});
  const std::vector<MeasurementDisplay> leaders = measurementDisplays(QJsonArray{datum, frame, under}, words);
  CHECK(leaders.size() == 3);
  if (leaders.size() != 3) {
    return failures;
  }
  CHECK(leaders[0].datum == QStringLiteral("A") && leaders[0].datumTerminator);
  CHECK(leaders[1].frame.size() == 3 && leaders[2].attached == QStringLiteral("Distance2"));
  const MeasurementShape datumShape = layoutMeasurement(leaders[0], top);
  CHECK(datumShape.valid && datumShape.leader && datumShape.datumTerminator && datumShape.datumLetter == QStringLiteral("A"));
  CHECK(datumShape.noteLines.isEmpty() && datumShape.arrows.size() == 1);
  CHECK(measurementHit(datumShape, datumShape.labelAt) == 1);
  const QFont font = measurementFont(QFont());
  // A square box beside the leader's end.
  const QRectF letterBox = measurementLabelRect(datumShape, font);
  CHECK(near(letterBox.width(), letterBox.height(), 1e-6) && near(letterBox.left(), 0.0, 1e-6));
  const MeasurementShape frameShape = layoutMeasurement(leaders[1], top);
  CHECK(frameShape.valid && frameShape.leader && frameShape.frameCells.size() == 3 && !frameShape.datumTerminator);
  // Its symbol and Ⓜ drawn in lines (fonts may lack them).
  CHECK(frameShape.characteristic == QStringLiteral("perpendicularity") && frameShape.modifier == QStringLiteral("M"));
  const QRectF cells = measurementLabelRect(frameShape, font);
  CHECK(cells.width() > 3.0 * cells.height() - 1e-6 && near(cells.center().y(), 0.0, 1e-6));
  const MeasurementShape underShape = layoutMeasurement(leaders[2], top);
  CHECK(underShape.valid && underShape.attached && underShape.arrows.empty() && underShape.lines.empty());
  const QRectF underRect = measurementLabelRect(underShape, font);
  CHECK(underRect.top() > 0.0 && near(underRect.center().x(), 0.0, 1e-6));
  CHECK(!measurementDragMillimetres(underShape, QPointF(5.0, 5.0)));
  const MeasurementShape notMoved = draggedMeasurement(underShape, false, QPointF(5.0, 5.0));
  CHECK(near(notMoved.labelAt.x(), underShape.labelAt.x(), 1e-9) && near(notMoved.labelAt.y(), underShape.labelAt.y(), 1e-9));
  // Painted: the datum's triangle filled in its colour.
  QImage leaderImage(420, 240, QImage::Format_ARGB32);
  leaderImage.fill(Qt::white);
  {
    QPainter painter(&leaderImage);
    paintMeasurements(painter, {datumShape, frameShape, underShape}, QColor(250, 250, 252), font);
  }
  const std::optional<QPointF> foot = top(gp_Pnt(0, 0, 0));
  CHECK(foot.has_value());
  if (foot) {
    // Beside the leader's end, inside the triangle's base, where an
    // arrowhead's point leaves the halo: the leader comes down to the left.
    const QPoint inside = foot->toPoint() + QPoint(2, 1);
    CHECK(nearColour(leaderImage.pixelColor(inside), QColor(0x70, 0x48, 0xe8)));
  }

  // mitcad#894: the model's text height in the plane, 12 pixels here; a
  // tiny one stays at the floor of 9 pixels on screen.
  CHECK(near(face.textPixels, 12.0, 1e-6));
  MeasurementDisplay tiny = drawn[0];
  tiny.textHeight = 0.1;
  CHECK(near(layoutMeasurement(tiny, top).textPixels, 9.0, 1e-6));
  CHECK(near(layoutMeasurement(tiny, twice, 2.0).textPixels, 18.0, 1e-6));

  // Text direction (ISO 129-1): read from the bottom or from the right,
  // never mirrored, from the front, from below and from behind:
  // horizontal texts left to right, vertical ones and those half a degree
  // off vertical (either way, pointing up or down the picture) bottom to
  // top.
  const double halfDegree = 0.5 * std::acos(-1.0) / 180.0;
  const std::vector<std::pair<gp_Vec, bool>> directions = {
      {gp_Vec(1.0, 0.0, 0.0), false},
      {gp_Vec(-1.0, 0.0, 0.0), false},
      {gp_Vec(0.0, 1.0, 0.0), true},
      {gp_Vec(0.0, -1.0, 0.0), true},
      {gp_Vec(std::sin(halfDegree), std::cos(halfDegree), 0.0), true},
      {gp_Vec(-std::sin(halfDegree), std::cos(halfDegree), 0.0), true},
      {gp_Vec(std::sin(halfDegree), -std::cos(halfDegree), 0.0), true},
      {gp_Vec(-std::sin(halfDegree), -std::cos(halfDegree), 0.0), true}};
  const std::vector<MeasurementProjection> sides = {MeasurementProjection(top), MeasurementProjection(below),
                                                    MeasurementProjection(behind)};
  for (const MeasurementProjection& seen : sides) {
    for (const auto& [direction, upright] : directions) {
      MeasurementDisplay turned = drawn[0];
      turned.textDirection = direction;
      turned.textUp = gp_Vec(0.0, 0.0, 1.0).Crossed(direction);
      const MeasurementShape shape = layoutMeasurement(turned, seen);
      CHECK(shape.valid && near(shape.opacity, 1.0));
      const TextAxes axes = textAxes(shape);
      CHECK(!axes.mirrored);
      if (upright) {
        CHECK(axes.along.y() < -0.99); // bottom to top
      } else {
        CHECK(axes.along.x() > 0.99 && axes.down.y() > 0.99); // left to right, upright
      }
    }
  }

  // No filled box behind a dimension's value: seen lower on the picture,
  // painted on nothing, most of the text's place stays clear, its corner
  // (where a card was) too.
  const auto lowered = [](const gp_Pnt& p) -> std::optional<QPointF> {
    return QPointF(100.0 + 10.0 * p.X(), 220.0 - 10.0 * p.Y());
  };
  const MeasurementShape valueShape = layoutMeasurement(drawn[0], lowered);
  QImage bare(520, 260, QImage::Format_ARGB32_Premultiplied);
  bare.fill(Qt::transparent);
  {
    QPainter painter(&bare);
    paintMeasurements(painter, {valueShape}, QColor(250, 250, 252), measurementFont(QFont()));
  }
  const QRect area = valueShape.textArea.boundingRect().toAlignedRect().intersected(bare.rect());
  CHECK(area.width() > 20 && area.height() > 8);
  int covered = 0;
  int pixels = 0;
  for (int y = area.top(); y <= area.bottom(); ++y) {
    for (int x = area.left(); x <= area.right(); ++x) {
      ++pixels;
      covered += qAlpha(bare.pixel(x, y)) > 0 ? 1 : 0;
    }
  }
  CHECK(pixels > 0 && covered < pixels * 6 / 10);
  CHECK(qAlpha(bare.pixel(area.topLeft() + QPoint(1, 1))) == 0);
  // The value itself is painted, in its colour.
  CHECK(paintedIn(bare, QColor(0x2f, 0x9e, 0x44)));

  // Closed arrowheads of 30 degrees, a text height long (mitcad#902); at least 5
  // pixels.
  const QPolygonF head = measurementArrowHead(QPointF(0.0, 0.0), QPointF(1.0, 0.0), 20.0);
  CHECK(head.size() == 3);
  if (head.size() == 3) {
    const QPointF one = head[1] - head[0];
    const QPointF other = head[2] - head[0];
    const double cosine = (one.x() * other.x() + one.y() * other.y()) / (lengthOf(one) * lengthOf(other));
    CHECK(near(std::acos(cosine) * 180.0 / std::acos(-1.0), 30.0, 1e-6));
    CHECK(near(lengthOf((head[1] + head[2]) / 2.0 - head[0]), 20.0, 1e-9));
    CHECK(near(lengthOf((head[1] + head[2]) / 2.0 - head[0]) / 20.0, 1.0, 1e-9));
  }
  const QPolygonF least = measurementArrowHead(QPointF(0.0, 0.0), QPointF(0.0, 1.0), 2.0);
  CHECK(least.size() == 3 && near(lengthOf((least[1] + least[2]) / 2.0 - least[0]), 5.0, 1e-9));
  // A laid out dimension's arrowheads: its text's height on the picture.
  if (face.arrows.size() == 2) {
    const QPolygonF drawnHead = measurementArrowHead(face.arrows[0].first, face.arrows[0].second, face.textPixels);
    CHECK(near(lengthOf((drawnHead[1] + drawnHead[2]) / 2.0 - drawnHead[0]), 1.0 * 12.0, 1e-6));
  }

  // Auto (mitcad#894): no colour of its own, drawn in the neutral colour of
  // the background tone: near-black on a light one, light grey on a dark
  // one.
  QJsonObject automatic = planar;
  automatic.insert(QStringLiteral("color"), QStringLiteral("auto"));
  const std::vector<MeasurementDisplay> autos = measurementDisplays(QJsonArray{automatic}, words);
  CHECK(autos.size() == 1 && autos[0].autoColor && !autos[0].color.isValid());
  const QColor light(250, 250, 252);
  const QColor dark(38, 41, 46);
  CHECK(measurementAutoColor(light).lightnessF() < 0.2);
  // A mid grey on dark (mitcad#902): clearly darker than the old chalk
  // #dee2e6, still light enough to read on the dark background.
  CHECK(measurementAutoColor(dark).lightnessF() > 0.55 && measurementAutoColor(dark).lightnessF() < 0.78);
  CHECK(measurementAutoColor(dark).lightnessF() < QColor(0xde, 0xe2, 0xe6).lightnessF() - 0.1);
  if (autos.size() == 1) {
    const MeasurementShape autoShape = layoutMeasurement(autos[0], lowered);
    CHECK(autoShape.autoColor);
    QImage onLight(520, 260, QImage::Format_ARGB32);
    onLight.fill(light);
    QImage onDark(520, 260, QImage::Format_ARGB32);
    onDark.fill(dark);
    {
      QPainter painter(&onLight);
      paintMeasurements(painter, {autoShape}, light, measurementFont(QFont()));
    }
    {
      QPainter painter(&onDark);
      paintMeasurements(painter, {autoShape}, dark, measurementFont(QFont()));
    }
    CHECK(paintedIn(onLight, measurementAutoColor(light)));
    CHECK(paintedIn(onDark, measurementAutoColor(dark)));
    CHECK(!paintedIn(onLight, QColor(0x2f, 0x9e, 0x44), 20));
  }

  // mitcad#902, the reference look.
  // The bundled technical lettering is found beside the test (fonts/).
  // (A build without the font file, third_party/fonts/iso-lettering, takes
  // the UI's font.)
  if (QFileInfo::exists(QDir(QCoreApplication::applicationDirPath()).filePath(QStringLiteral("fonts/lettering.ttf")))) {
    CHECK(!measurementFontFamily().isEmpty());
    CHECK(measurementTextFont(QFont(), face).families().contains(measurementFontFamily()));
  } else {
    CHECK(measurementFontFamily().isEmpty());
  }

  // No band and no filled box behind a selected or hovered annotation: the
  // same thin lines (at most 1.5 pixels) and text, in the selection's or the
  // hover's colour.
  for (const bool selected : {true, false}) {
    MeasurementShape lit = valueShape;
    lit.highlighted = selected;
    lit.hovered = !selected;
    const QColor colour = selected ? measurementSelectionColor() : measurementHoverColor();
    QImage picture(520, 260, QImage::Format_ARGB32_Premultiplied);
    picture.fill(Qt::transparent);
    {
      QPainter painter(&picture);
      paintMeasurements(painter, {lit}, QColor(250, 250, 252), measurementFont(QFont()));
    }
    CHECK(paintedIn(picture, colour));
    CHECK(!paintedIn(picture, QColor(0x2f, 0x9e, 0x44), 20));
    // The text's place mostly clear (no box), its corner too.
    const QRect place = lit.textArea.boundingRect().toAlignedRect().intersected(picture.rect());
    int inked = 0;
    int all = 0;
    for (int y = place.top(); y <= place.bottom(); ++y) {
      for (int x = place.left(); x <= place.right(); ++x) {
        ++all;
        inked += qAlpha(picture.pixel(x, y)) > 0 ? 1 : 0;
      }
    }
    CHECK(all > 0 && inked < all * 6 / 10);
    CHECK(qAlpha(picture.pixel(place.topLeft() + QPoint(1, 1))) == 0);
    // Across the dimension line (y 120 here, x 130 clear of the text and
    // the arrows), at most two pixels painted: no band of four.
    int crossing = 0;
    for (int y = 110; y <= 130; ++y) {
      crossing += qAlpha(picture.pixel(130, y)) > 40 ? 1 : 0;
    }
    CHECK(crossing >= 1 && crossing <= 2);
  }

  // Crisp lines: every line of a laid out annotation on whole-pixel
  // centres (a pixel wide), on pixel corners at twice the scale (two wide).
  CHECK(near(measurementLineWidth(), 1.0) && near(measurementLineWidth(2.0), 2.0));
  const auto onCentres = [](const QPointF& p) {
    return near(p.x() - std::floor(p.x()), 0.5, 1e-9) && near(p.y() - std::floor(p.y()), 0.5, 1e-9);
  };
  CHECK(!face.lines.empty() && !skewed.lines.empty());
  for (const MeasurementShape* laid : {&face, &skewed, &noted}) {
    for (const QLineF& line : laid->lines) {
      CHECK(onCentres(line.p1()) && onCentres(line.p2()));
    }
  }
  const QPointF evenSnapped = measurementSnapped(QPointF(10.3, 7.8), 2.0);
  CHECK(near(evenSnapped.x(), 10.0) && near(evenSnapped.y(), 8.0));
  const QPointF oddSnapped = measurementSnapped(QPointF(10.3, 7.8), 1.0);
  CHECK(near(oddSnapped.x(), 10.5) && near(oddSnapped.y(), 7.5));

  // The text's height between its floor and its ceiling on screen: 5 mm
  // at 10 pixels a millimetre (zoomed in) stays at the ceiling, 0.1 mm
  // (zoomed out) at the floor, both times a picture's scale.
  CHECK(near(measurementTextFloor(), 9.0) && near(measurementTextCeiling(), 16.0));
  MeasurementDisplay tall = drawn[0];
  tall.textHeight = 5.0;
  CHECK(near(layoutMeasurement(tall, top).textPixels, measurementTextCeiling(), 1e-9));
  CHECK(near(layoutMeasurement(tall, twice, 2.0).textPixels, measurementTextCeiling(2.0), 1e-9));
  CHECK(near(layoutMeasurement(tiny, top).textPixels, measurementTextFloor(), 1e-9));
  CHECK(near(face.textPixels, 12.0, 1e-9)); // between them: the model's

  // Nearly equal projection scales must not lose their small difference
  // in the singular-value calculation (a face-on plane's repeated value).
  const auto almostTop = [](const gp_Pnt& p) -> std::optional<QPointF> {
    return QPointF(100.0 + 10.0 * (1.0 + 1e-8) * p.X(), 100.0 - 10.0 * p.Y());
  };
  CHECK(near(layoutMeasurement(tall, almostTop).textPixels, measurementTextCeiling() * (1.0 + 1e-8), 1e-9));
  CHECK(near(layoutMeasurement(tiny, almostTop).textPixels, measurementTextFloor() * (1.0 + 1e-8), 1e-9));
  CHECK(near(layoutMeasurement(drawn[0], almostTop).textPixels, 12.0 * (1.0 + 1e-8), 1e-9));

  // Arrowheads in the annotation's plane, as its text: seen with the
  // plane's y squashed to 0.7 (the plane turned away about X), an
  // arrowhead along X keeps its length but its width is squashed with the
  // plane, where one facing the screen would keep its full width.
  const auto squashed = [](const gp_Pnt& p) -> std::optional<QPointF> {
    return QPointF(100.0 + 10.0 * p.X(), 220.0 - 7.0 * p.Y());
  };
  MeasurementDisplay marked = drawn[0];
  marked.centerMark = gp_Pnt(20.0, 10.0, 0.0);
  const MeasurementShape turned = layoutMeasurement(marked, squashed);
  CHECK(turned.valid && turned.heads.size() == 2 && turned.arrows.size() == 2);
  if (turned.heads.size() == 2 && turned.heads[0].size() == 3) {
    const QPolygonF& planeHead = turned.heads[0];
    const double headLength = lengthOf((planeHead[1] + planeHead[2]) / 2.0 - planeHead[0]);
    const double headWidth = lengthOf(planeHead[1] - planeHead[2]);
    const QPolygonF facing = measurementArrowHead(turned.arrows[0].first, turned.arrows[0].second, turned.textPixels);
    const double facingWidth = lengthOf(facing[1] - facing[2]);
    CHECK(near(headLength, 1.0 * turned.textPixels, 1e-6));
    CHECK(near(headWidth, 0.7 * facingWidth, 1e-6));
    // Its tip on the dimension line's end.
    CHECK(near(planeHead[0].x(), turned.arrows[0].first.x(), 1e-9) &&
          near(planeHead[0].y(), turned.arrows[0].first.y(), 1e-9));
  }
  // The centre mark's arms in the plane: the one along y squashed too.
  CHECK(turned.lines.size() >= 5);
  if (turned.lines.size() >= 5) {
    const QLineF alongX = turned.lines[turned.lines.size() - 2];
    const QLineF alongY = turned.lines[turned.lines.size() - 1];
    CHECK(alongY.length() < 0.85 * alongX.length());
  }

  // Extension lines start a quarter text height off the measured point the
  // model gives (its picked edge, vertex or face outline), not anywhere
  // else, and run half a text height past the dimension line.
  if (face.lines.size() == 3) {
    const QLineF first = face.lines[0];
    // The model's (0, 0, 0) is (100, 100) on the picture, the line goes up.
    CHECK(near(first.p1().x(), 100.5, 1e-9));
    CHECK(std::abs(first.p1().y() - (100.0 - 0.5 * face.textPixels)) <= 0.5 + 1e-9);
    CHECK(std::abs(first.p2().y() - (0.0 - 0.8 * face.textPixels)) <= 0.5 + 1e-9);
  }

  // A frame's cells two text heights high, a datum label's square too.
  CHECK(near(cells.height(), 2.0 * frameShape.textPixels, 0.5 + 1e-9));
  CHECK(near(letterBox.height(), 2.0 * datumShape.textPixels, 0.5 + 1e-9));

  // A leader on a face ends in a dot in its plane; a hole callout's lines
  // start at its leader's end on the side the model says, and a frame under
  // it stands below its lines on that side.
  QJsonObject dotted = note;
  QJsonObject dottedDrawing = note.value(QStringLiteral("drawing")).toObject();
  dottedDrawing.insert(QStringLiteral("terminator"), QStringLiteral("dot"));
  dotted.insert(QStringLiteral("drawing"), dottedDrawing);
  QJsonObject callout = note;
  callout.insert(QStringLiteral("name"), QStringLiteral("HoleCallout1"));
  callout.insert(QStringLiteral("kind"), QStringLiteral("hole_callout"));
  callout.remove(QStringLiteral("text"));
  callout.insert(QStringLiteral("label"),
                 QJsonObject{{QStringLiteral("text"), QStringLiteral("4x Ø6.6 THRU / ⌴ Ø11 ↧ 6.8")},
                             {QStringLiteral("lines"), QJsonArray{QStringLiteral("4x Ø6.6 THRU"),
                                                                  QStringLiteral("⌴ Ø11 ↧ 6.8")}}});
  QJsonObject calloutDrawing = note.value(QStringLiteral("drawing")).toObject();
  calloutDrawing.insert(QStringLiteral("align"), QStringLiteral("end"));
  callout.insert(QStringLiteral("drawing"), calloutDrawing);
  QJsonObject position = frame;
  position.insert(QStringLiteral("name"), QStringLiteral("Position"));
  QJsonObject positionDrawing = under.value(QStringLiteral("drawing")).toObject();
  positionDrawing.insert(QStringLiteral("attached"), QStringLiteral("HoleCallout1"));
  positionDrawing.insert(QStringLiteral("under_lines"), 2);
  positionDrawing.insert(QStringLiteral("align"), QStringLiteral("end"));
  position.insert(QStringLiteral("drawing"), positionDrawing);
  const std::vector<MeasurementDisplay> pmi = measurementDisplays(QJsonArray{dotted, callout, position}, words);
  CHECK(pmi.size() == 3);
  if (pmi.size() == 3) {
    CHECK(pmi[0].dotTerminator && !pmi[0].datumTerminator);
    const MeasurementShape dot = layoutMeasurement(pmi[0], top);
    CHECK(dot.dotTerminator && dot.heads.size() == 1 && dot.heads[0].size() >= 12);
    CHECK(pmi[1].align == QStringLiteral("end") && pmi[1].lines.size() == 2);
    const MeasurementShape holeShape = layoutMeasurement(pmi[1], top);
    CHECK(holeShape.valid && holeShape.note && holeShape.leader && holeShape.noteLines.size() == 2);
    CHECK(holeShape.rightAligned && !holeShape.dotTerminator && holeShape.heads.size() == 1);
    CHECK(measurementHit(holeShape, holeShape.labelAt) == 1);
    const QRectF holeText = measurementLabelRect(holeShape, font);
    CHECK(holeText.right() < 0.0); // ends at the leader's end
    CHECK(pmi[2].underLines == 2 && pmi[2].attached == QStringLiteral("HoleCallout1"));
    const MeasurementShape positionShape = layoutMeasurement(pmi[2], top);
    const QRectF positionCells = measurementLabelRect(positionShape, font);
    CHECK(positionShape.attached && positionShape.rightAligned && positionShape.underLines == 2);
    // Below the callout's two lines (1.6 text heights apart), on its side.
    CHECK(positionCells.top() >= 1.6 * positionShape.textPixels - 1e-6);
    // The callout's first line level with the landing (mitcad#902), its
    // lines and the frame under them flush left: each knows the other's
    // words.
    CHECK(near(holeText.top(), -(0.8 + 0.3) * holeShape.textPixels, 1e-6));
    CHECK(pmi[1].framesUnder.size() == 1 && pmi[2].overLines == pmi[1].lines);
    CHECK(near(positionCells.left(), holeText.left() + 0.3 * holeShape.textPixels, 0.5));
    QImage pmiImage(420, 240, QImage::Format_ARGB32);
    pmiImage.fill(Qt::white);
    {
      QPainter painter(&pmiImage);
      paintMeasurements(painter, {dot, holeShape, positionShape}, QColor(250, 250, 252), font);
    }
    CHECK(paintedIn(pmiImage, QColor(0x70, 0x48, 0xe8)));
  }

  // Render Image and snapshots in the tone of their own background: a
  // light picture's, a dark one's; none of a transparent one.
  QImage lightPicture(64, 48, QImage::Format_ARGB32);
  lightPicture.fill(QColor(245, 245, 245));
  QImage darkPicture(64, 48, QImage::Format_ARGB32);
  darkPicture.fill(QColor(30, 32, 36));
  QImage clearPicture(64, 48, QImage::Format_ARGB32);
  clearPicture.fill(Qt::transparent);
  CHECK(measurementPictureTone(lightPicture).lightnessF() > 0.9);
  CHECK(measurementPictureTone(darkPicture).lightnessF() < 0.2);
  CHECK(!measurementPictureTone(clearPicture).isValid());
  CHECK(measurementAutoColor(measurementPictureTone(lightPicture)).lightnessF() < 0.2);

  // Auto's ink chosen against what lies behind it, single plain lines
  // (mitcad#902): near-black over the light half of a picture, a light grey
  // over the dark half; a line across both piece by piece (readable over
  // either), a text over both in one ink, that of what lies behind most of
  // it.
  QImage halves(200, 40, QImage::Format_RGB32);
  halves.fill(QColor(30, 32, 36));
  for (int y = 0; y < 40; ++y) {
    for (int x = 100; x < 200; ++x) {
      halves.setPixelColor(x, y, QColor(210, 213, 218));
    }
  }
  const MeasurementBackdrop backdrop{halves, 1.0};
  const QColor darkBehind = measurementAutoInk(&backdrop, QPointF(50.0, 20.0), QColor(30, 32, 36));
  const QColor lightBehind = measurementAutoInk(&backdrop, QPointF(150.0, 20.0), QColor(30, 32, 36));
  CHECK(darkBehind.lightnessF() > 0.55 && lightBehind.lightnessF() < 0.2);
  CHECK(measurementAutoInk(nullptr, QPointF(150.0, 20.0), QColor(30, 32, 36)) == measurementAutoColor(QColor(30, 32, 36)));
  MeasurementShape twoTone = autos.empty() ? MeasurementShape() : layoutMeasurement(autos[0], top);
  twoTone.lines = {QLineF(QPointF(10.5, 20.5), QPointF(190.5, 20.5))};
  twoTone.arrows.clear();
  twoTone.heads.clear();
  twoTone.planar = false;
  twoTone.label.clear();
  twoTone.valid = true;
  twoTone.opacity = 1.0;
  QImage inked = halves;
  {
    QPainter painter(&inked);
    paintMeasurements(painter, {twoTone}, QColor(30, 32, 36), measurementFont(QFont()), 1.0, &backdrop);
  }
  CHECK(nearColour(inked.pixelColor(50, 20), darkBehind, 20));
  CHECK(nearColour(inked.pixelColor(150, 20), lightBehind, 20));
  MeasurementShape mostlyDark = twoTone;
  mostlyDark.textArea = QPolygonF(QRectF(10.0, 5.0, 130.0, 10.0));
  CHECK(measurementAutoInk(&backdrop, mostlyDark, QColor(30, 32, 36)) == darkBehind);
  MeasurementShape mostlyLight = twoTone;
  mostlyLight.textArea = QPolygonF(QRectF(60.0, 5.0, 130.0, 10.0));
  CHECK(measurementAutoInk(&backdrop, mostlyLight, QColor(30, 32, 36)) == lightBehind);
  // No halo: the pixels beside the line are what lay behind.
  CHECK(nearColour(inked.pixelColor(150, 18), QColor(210, 213, 218), 8));
  CHECK(nearColour(inked.pixelColor(50, 18), QColor(30, 32, 36), 8));
  return failures;
}
