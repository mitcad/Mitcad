// SPDX-License-Identifier: MIT
// Editing a 3D sketch without the view (mitcad#1526): the sketch3d
// commands the palette and the SKETCH tab's constraints send for a
// selection (Coincident, Point on Curve, Tangent at the shared end, Equal,
// Fix and Unfix of what is not yet so, Align with the sense the line runs,
// Detach; Parallel, Perpendicular, Angle, Symmetric with its plane, On
// Plane, Curvature, mitcad#1661; Length, Delta Z, Radius, a driving Spline
// Length, Angle, Tangent Magnitude) and the reasons
// when the selection takes none; what a delete takes with a point; the
// drag goals of a curve and of a tangent handle; the handles shown at a
// free or tangent end only; a cubic's samples; the palette's words.
#include "sketch/Sketch3dEdit.hpp"

#include <cmath>
#include <cstdio>

#include <QJsonDocument>
#include <QRegularExpression>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "sketch3d_edit.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad::sketch;

bool same(const Xyz& a, const Xyz& b, double tolerance = 1e-9) {
  return std::abs(a[0] - b[0]) <= tolerance && std::abs(a[1] - b[1]) <= tolerance &&
         std::abs(a[2] - b[2]) <= tolerance;
}

Xyz xyz(const QJsonValue& value) {
  const QJsonArray a = value.toArray();
  return a.size() >= 3 ? Xyz{a.at(0).toDouble(), a.at(1).toDouble(), a.at(2).toDouble()} : Xyz{};
}

// A route A-B-C along +X and +Y with an arc from C, a fit-point spline
// from the arc's end with a free start, a reference curve with a point on
// it; A fixed, B-C held by a length.
const char* const kSketch = R"({
  "uid": "F2", "name": "3DSketch1", "type": "spatial_sketch", "support": "xy",
  "frame": {"origin": [0, 0, 0], "x_axis": [1, 0, 0], "y_axis": [0, 1, 0], "normal": [0, 0, 1]},
  "points": [
    {"id": "p1", "at": [0, 0, 0], "determined": true},
    {"id": "p2", "at": [40, 0, 0]},
    {"id": "p3", "at": [40, 30, 0]},
    {"id": "p7", "at": [30, 30, 0]},
    {"id": "p9", "at": [30, 40, 0]},
    {"id": "p11", "at": [0, 40, 20]},
    {"id": "p13", "at": [60, 0, 5]}
  ],
  "curves": [
    {"id": "c4", "type": "line", "start": "p1", "end": "p2", "geometry": {"start": [0, 0, 0], "end": [40, 0, 0]}},
    {"id": "c6", "type": "line", "start": "p3", "end": "p2", "geometry": {"start": [40, 30, 0], "end": [40, 0, 0]}},
    {"id": "c8", "type": "arc", "center": "p7", "start": "p3", "end": "p9", "normal": [0, 0, 1],
     "geometry": {"normal": [0, 0, 1], "radius": 10, "sweep": 1.5707963267948966, "length": 15.7}},
    {"id": "c10", "type": "fitted_spline", "points": ["p9", "p11"], "ends": ["free", "natural"],
     "geometry": {"degree": 3, "knots": [0, 0, 0, 0, 1, 1, 1, 1], "weights": [],
                  "control_points": [[30, 40, 0], [20, 40, 0], [10, 40, 10], [0, 40, 20]],
                  "end_tangents": [[-30, 0, 0], [-30, 0, 30]]}},
    {"id": "c12", "type": "reference", "source": {"body": "F1.b0", "edge": "E{F1:side0|F1:side1}"},
     "geometry": {"kind": "line", "start": [60, 0, 0], "end": [60, 0, 20]}}
  ],
  "constraints": [
    {"id": "k1", "type": "fix", "point": "p1", "at": [0, 0, 0]},
    {"id": "k3", "type": "point_on", "point": "p13", "curve": "c12"}
  ],
  "dimensions": [
    {"id": "k5", "type": "length", "line": "c6", "value": "d1", "driven": false, "expression": "30 mm",
     "measured": 30},
    {"id": "k6", "type": "offset", "a": "p1", "b": "p11", "axis": "z", "driven": true, "measured": 20}
  ],
  "references": [
    {"id": "c12", "kind": "curve", "source": {"body": "F1.b0", "edge": "E{F1:side0|F1:side1}"}, "body": "F1.b0",
     "mode": "include", "users": ["k3"], "status": "failed", "message": "the source edge was not found",
     "detail": "edge E{F1:side0|F1:side1} was not found"}
  ],
  "evaluated": true,
  "status": {"dof": 9, "fully_constrained": false, "redundant": []}
})";

Sketch3dModel model() {
  return Sketch3dModel::read(QJsonDocument::fromJson(QByteArray(kSketch)).object());
}

EditSelection selectionOf(const QStringList& points, const QStringList& curves) {
  EditSelection s;
  s.points = points;
  s.curves = curves;
  return s;
}

QJsonObject first(const EditCommand& edit) { return edit.commands.isEmpty() ? QJsonObject() : edit.commands.first().toObject(); }

QJsonObject constraintOf(const EditCommand& edit) { return first(edit).value(QStringLiteral("constraint")).toObject(); }

QJsonObject dimensionOf(const EditCommand& edit) { return first(edit).value(QStringLiteral("dimension")).toObject(); }

void relationTests() {
  const Sketch3dModel m = model();
  // Coincident: two points; a point and a curve is Point on Curve.
  EditCommand edit = relationCommand(m, QStringLiteral("F2"), Relation3::Coincident, selectionOf({"p9", "p13"}, {}));
  CHECK(edit.valid());
  CHECK(first(edit).value(QStringLiteral("cmd")).toString() == QStringLiteral("sketch3d.add_constraint"));
  CHECK(first(edit).value(QStringLiteral("sketch")).toString() == QStringLiteral("F2"));
  CHECK(constraintOf(edit).value(QStringLiteral("type")).toString() == QStringLiteral("coincident"));
  edit = relationCommand(m, QStringLiteral("F2"), Relation3::Coincident, selectionOf({"p13"}, {"c4"}));
  CHECK(constraintOf(edit).value(QStringLiteral("type")).toString() == QStringLiteral("point_on"));
  CHECK(constraintOf(edit).value(QStringLiteral("curve")).toString() == QStringLiteral("c4"));
  edit = relationCommand(m, QStringLiteral("F2"), Relation3::Coincident, selectionOf({}, {"c4", "c6"}));
  CHECK(!edit.valid() && !edit.problem.isEmpty());
  // A point of the curve is on it already.
  CHECK(!relationCommand(m, QStringLiteral("F2"), Relation3::PointOnCurve, selectionOf({"p1"}, {"c4"})).valid());
  // Midpoint of a line; not of a spline.
  edit = relationCommand(m, QStringLiteral("F2"), Relation3::Midpoint, selectionOf({"p13"}, {"c4"}));
  CHECK(constraintOf(edit).value(QStringLiteral("type")).toString() == QStringLiteral("midpoint"));
  CHECK(!relationCommand(m, QStringLiteral("F2"), Relation3::Midpoint, selectionOf({"p13"}, {"c10"})).valid());

  // Tangent at the shared end: the line c6 and the arc c8 at p3; two
  // lines are refused; curves without a shared end need the point.
  edit = relationCommand(m, QStringLiteral("F2"), Relation3::Tangent, selectionOf({}, {"c6", "c8"}));
  CHECK(edit.valid());
  CHECK(constraintOf(edit).value(QStringLiteral("at")).toString() == QStringLiteral("p3"));
  CHECK(!relationCommand(m, QStringLiteral("F2"), Relation3::Tangent, selectionOf({}, {"c4", "c6"})).valid());
  edit = relationCommand(m, QStringLiteral("F2"), Relation3::Tangent, selectionOf({}, {"c4", "c10"}));
  CHECK(!edit.valid() && edit.problem.contains(QStringLiteral("share no end")));
  edit = relationCommand(m, QStringLiteral("F2"), Relation3::Tangent, selectionOf({"p9"}, {"c8", "c10"}));
  CHECK(constraintOf(edit).value(QStringLiteral("at")).toString() == QStringLiteral("p9"));

  // Equal: two lines or two rounds.
  edit = relationCommand(m, QStringLiteral("F2"), Relation3::Equal, selectionOf({}, {"c4", "c6"}));
  CHECK(constraintOf(edit).value(QStringLiteral("type")).toString() == QStringLiteral("equal"));
  CHECK(!relationCommand(m, QStringLiteral("F2"), Relation3::Equal, selectionOf({}, {"c4", "c8"})).valid());

  // Fix only what is not fixed, Unfix only what is; the SKETCH tab's Fix
  // toggles.
  edit = relationCommand(m, QStringLiteral("F2"), Relation3::Fix, selectionOf({"p1", "p2"}, {}));
  CHECK(first(edit).value(QStringLiteral("cmd")).toString() == QStringLiteral("sketch3d.fix"));
  CHECK(first(edit).value(QStringLiteral("points")).toArray() == QJsonArray{QStringLiteral("p2")});
  CHECK(first(edit).value(QStringLiteral("fixed")).toBool());
  edit = relationCommand(m, QStringLiteral("F2"), Relation3::Unfix, selectionOf({"p1", "p2"}, {}));
  CHECK(first(edit).value(QStringLiteral("points")).toArray() == QJsonArray{QStringLiteral("p1")});
  CHECK(!first(edit).value(QStringLiteral("fixed")).toBool());
  CHECK(fixToggle(m, selectionOf({"p1"}, {})) == Relation3::Unfix);
  CHECK(fixToggle(m, selectionOf({"p1"}, {"c4"})) == Relation3::Fix); // c4's p2 is free
  CHECK(!relationCommand(m, QStringLiteral("F2"), Relation3::Unfix, selectionOf({"p2"}, {})).valid());

  // Align: a line in the sense it runs, an arc's normal; points take none.
  edit = relationCommand(m, QStringLiteral("F2"), Relation3::AlignY, selectionOf({}, {"c6", "c8"}));
  CHECK(edit.commands.size() == 2); // the line and the arc's normal, one step
  edit = relationCommand(m, QStringLiteral("F2"), Relation3::AlignY, selectionOf({}, {"c6"}));
  CHECK(constraintOf(edit).value(QStringLiteral("type")).toString() == QStringLiteral("align"));
  CHECK(constraintOf(edit).value(QStringLiteral("axis")).toString() == QStringLiteral("y"));
  CHECK(constraintOf(edit).value(QStringLiteral("sense")).toString() == QStringLiteral("negative"));
  edit = relationCommand(m, QStringLiteral("F2"), Relation3::AlignZ, selectionOf({}, {"c8"}));
  CHECK(constraintOf(edit).value(QStringLiteral("type")).toString() == QStringLiteral("normal_align"));
  CHECK(constraintOf(edit).value(QStringLiteral("sense")).toString() == QStringLiteral("positive"));
  CHECK(!relationCommand(m, QStringLiteral("F2"), Relation3::AlignX, selectionOf({"p1"}, {})).valid());

  // On Sketch Plane and Release from Plane; Release Plane needs a
  // plane-bound arc.
  edit = relationCommand(m, QStringLiteral("F2"), Relation3::OnSketchPlane, selectionOf({"p11"}, {}));
  CHECK(constraintOf(edit).value(QStringLiteral("type")).toString() == QStringLiteral("on_sketch_plane"));
  CHECK(!relationCommand(m, QStringLiteral("F2"), Relation3::ReleaseFromPlane, selectionOf({"p11"}, {})).valid());
  CHECK(!relationCommand(m, QStringLiteral("F2"), Relation3::ReleasePlane, selectionOf({}, {"c8"})).valid());

  // Detach: a point on a reference curve, or the reference curve itself.
  edit = relationCommand(m, QStringLiteral("F2"), Relation3::Detach, selectionOf({}, {"c12"}));
  CHECK(first(edit).value(QStringLiteral("cmd")).toString() == QStringLiteral("sketch3d.detach"));
  CHECK(first(edit).value(QStringLiteral("points")).toArray() == QJsonArray{QStringLiteral("p13")});
  CHECK(!relationCommand(m, QStringLiteral("F2"), Relation3::Detach, selectionOf({"p2"}, {})).valid());
  // 3D relations (mitcad#1661): directions of lines and arcs' normals.
  edit = relationCommand(m, QStringLiteral("F2"), Relation3::Parallel, selectionOf({}, {"c4", "c6"}));
  CHECK(constraintOf(edit).value(QStringLiteral("type")).toString() == QStringLiteral("parallel"));
  edit = relationCommand(m, QStringLiteral("F2"), Relation3::Perpendicular, selectionOf({}, {"c4", "c8"}));
  CHECK(constraintOf(edit).value(QStringLiteral("b")).toString() == QStringLiteral("c8"));
  CHECK(!relationCommand(m, QStringLiteral("F2"), Relation3::Parallel, selectionOf({}, {"c4", "c10"})).valid());
  edit = relationCommand(m, QStringLiteral("F2"), Relation3::Angle, selectionOf({}, {"c4", "c6"}));
  CHECK(dimensionOf(edit).value(QStringLiteral("type")).toString() == QStringLiteral("angle"));
  // Symmetric: two points; the plane picked after the button.
  CHECK(takesPlane(Relation3::Symmetric) && takesPlane(Relation3::OnPlane) && !takesPlane(Relation3::Parallel));
  EditSelection mirrored = selectionOf({"p1", "p11"}, {});
  QJsonObject yz;
  yz.insert(QStringLiteral("frame"), QStringLiteral("x"));
  mirrored.plane = yz;
  edit = relationCommand(m, QStringLiteral("F2"), Relation3::Symmetric, mirrored);
  CHECK(constraintOf(edit).value(QStringLiteral("plane")).toObject().value(QStringLiteral("frame")).toString() ==
        QStringLiteral("x"));
  CHECK(!relationCommand(m, QStringLiteral("F2"), Relation3::Symmetric, selectionOf({"p1"}, {})).valid());
  edit = relationCommand(m, QStringLiteral("F2"), Relation3::OnPlane, selectionOf({}, {"c4"}));
  CHECK(constraintOf(edit).value(QStringLiteral("points")).toArray().size() == 2);
  // Curvature at the arc's end where the spline starts.
  edit = relationCommand(m, QStringLiteral("F2"), Relation3::Curvature, selectionOf({}, {"c8", "c10"}));
  CHECK(constraintOf(edit).value(QStringLiteral("at")).toString() == QStringLiteral("p9"));
  CHECK(!relationCommand(m, QStringLiteral("F2"), Relation3::Curvature, selectionOf({}, {"c4", "c6"})).valid());
  // Concentric needs two arcs or circles.
  CHECK(!relationCommand(m, QStringLiteral("F2"), Relation3::Concentric, selectionOf({}, {"c4", "c8"})).valid());
  CHECK(detachPoints(m, QStringLiteral("c12")) == QStringList{QStringLiteral("p13")});
}

void dimensionTests() {
  const Sketch3dModel m = model();
  EditCommand edit = dimensionCommand(m, QStringLiteral("F2"), Dimension3::Length, selectionOf({}, {"c4"}));
  CHECK(first(edit).value(QStringLiteral("cmd")).toString() == QStringLiteral("sketch3d.add_dimension"));
  CHECK(dimensionOf(edit).value(QStringLiteral("type")).toString() == QStringLiteral("length"));
  CHECK(dimensionOf(edit).value(QStringLiteral("line")).toString() == QStringLiteral("c4"));
  CHECK(!first(edit).contains(QStringLiteral("value"))); // what it measures now
  CHECK(!dimensionCommand(m, QStringLiteral("F2"), Dimension3::Length, selectionOf({}, {"c8"})).valid());
  // Delta Z of two points, or of a line's ends.
  edit = dimensionCommand(m, QStringLiteral("F2"), Dimension3::DeltaZ, selectionOf({"p1", "p11"}, {}));
  CHECK(dimensionOf(edit).value(QStringLiteral("type")).toString() == QStringLiteral("offset"));
  CHECK(dimensionOf(edit).value(QStringLiteral("axis")).toString() == QStringLiteral("z"));
  CHECK(dimensionOf(edit).value(QStringLiteral("a")).toString() == QStringLiteral("p1"));
  CHECK(dimensionOf(edit).value(QStringLiteral("b")).toString() == QStringLiteral("p11"));
  edit = dimensionCommand(m, QStringLiteral("F2"), Dimension3::DeltaX, selectionOf({}, {"c6"}));
  CHECK(dimensionOf(edit).value(QStringLiteral("a")).toString() == QStringLiteral("p3"));
  edit = dimensionCommand(m, QStringLiteral("F2"), Dimension3::Radius, selectionOf({}, {"c8"}));
  CHECK(dimensionOf(edit).value(QStringLiteral("curve")).toString() == QStringLiteral("c8"));
  CHECK(dimensionCommand(m, QStringLiteral("F2"), Dimension3::ArcLength, selectionOf({}, {"c8"})).valid());
  CHECK(!dimensionCommand(m, QStringLiteral("F2"), Dimension3::ArcLength, selectionOf({}, {"c4"})).valid());
  // A spline's length drives at its length now (mitcad#1661).
  edit = dimensionCommand(m, QStringLiteral("F2"), Dimension3::SplineLength, selectionOf({}, {"c10"}));
  CHECK(edit.valid() && !first(edit).value(QStringLiteral("driven")).toBool());
  // An angle between two lines, or a line and an arc's normal.
  edit = dimensionCommand(m, QStringLiteral("F2"), Dimension3::Angle, selectionOf({}, {"c4", "c8"}));
  CHECK(dimensionOf(edit).value(QStringLiteral("type")).toString() == QStringLiteral("angle"));
  CHECK(!dimensionCommand(m, QStringLiteral("F2"), Dimension3::Angle, selectionOf({}, {"c4", "c10"})).valid());
  // A tangent magnitude at the spline's free start.
  edit = dimensionCommand(m, QStringLiteral("F2"), Dimension3::TangentMagnitude, selectionOf({"p9"}, {"c10"}));
  CHECK(dimensionOf(edit).value(QStringLiteral("end")).toString() == QStringLiteral("start"));
  CHECK(!dimensionCommand(m, QStringLiteral("F2"), Dimension3::TangentMagnitude, selectionOf({"p1"}, {"c10"})).valid());
  // The SKETCH tab's Sketch Dimension: by what is selected.
  edit = autoDimensionCommand(m, QStringLiteral("F2"), selectionOf({}, {"c8"}));
  CHECK(dimensionOf(edit).value(QStringLiteral("type")).toString() == QStringLiteral("radius"));
  edit = autoDimensionCommand(m, QStringLiteral("F2"), selectionOf({"p1", "p2"}, {}));
  CHECK(!edit.valid() && edit.problem.contains(QStringLiteral("Delta")));
}

void deleteAndDragTests() {
  const Sketch3dModel m = model();
  // Deleting B takes both lines that use it, not the arc.
  const QStringList dependents = dependentsOf(m, {QStringLiteral("p2")});
  CHECK(dependents.contains(QStringLiteral("c4")));
  CHECK(dependents.contains(QStringLiteral("c6")));
  CHECK(!dependents.contains(QStringLiteral("c8")));
  // Listed with its curves, nothing more goes (their own relations go with
  // them in the model); A's Fix is named with A.
  CHECK(dependentsOf(m, {QStringLiteral("p2"), QStringLiteral("c4"), QStringLiteral("c6")}).isEmpty());
  CHECK(dependentsOf(m, {QStringLiteral("p1")}).contains(QStringLiteral("k1")));
  CHECK(dependentsOf(m, {QStringLiteral("c4")}).isEmpty());
  // A curve drags its points; a reference curve does not drag.
  const QStringList arc = dragPoints(m, QStringLiteral("c8"));
  CHECK(arc.contains(QStringLiteral("p7")) && arc.contains(QStringLiteral("p3")) && arc.contains(QStringLiteral("p9")));
  CHECK(dragPoints(m, QStringLiteral("c12")).isEmpty());
  const QJsonObject drag =
      dragCommand(QStringLiteral("F2"), {QStringLiteral("p1"), QStringLiteral("p2")}, {{0, 0, 5}, {40, 0, 5}}, true);
  CHECK(drag.value(QStringLiteral("cmd")).toString() == QStringLiteral("sketch3d.drag"));
  CHECK(drag.value(QStringLiteral("preview")).toBool());
  CHECK(drag.value(QStringLiteral("points")).toArray().size() == 2);
  CHECK(same(xyz(drag.value(QStringLiteral("points")).toArray().at(1).toObject().value(QStringLiteral("to"))),
             {40, 0, 5}));

  // Tangent handles: the free start only (the natural end has no
  // tangency); its tip a third of C' ahead; a dragged tip back to C'.
  const std::vector<TangentHandle> handles = tangentHandles(m);
  CHECK(handles.size() == 1);
  if (handles.size() == 1) {
    CHECK(handles[0].curve == QStringLiteral("c10"));
    CHECK(handles[0].point == QStringLiteral("p9"));
    CHECK(handles[0].end == 0);
    CHECK(same(handles[0].tip, {20, 40, 0}));
  }
  const QJsonObject handle =
      handleDragCommand(QStringLiteral("F2"), QStringLiteral("c10"), 0, {30, 40, 0}, {30, 40, 10}, false);
  CHECK(!handle.contains(QStringLiteral("preview")));
  const QJsonObject pull = handle.value(QStringLiteral("tangents")).toArray().at(0).toObject();
  CHECK(pull.value(QStringLiteral("end")).toString() == QStringLiteral("start"));
  CHECK(same(xyz(pull.value(QStringLiteral("to"))), {0, 0, 30}));
  CHECK(same(tangentOfTip({1, 2, 3}, handleTip({1, 2, 3}, {6, -3, 9})), {6, -3, 9}));
}

void sampleAndTextTests() {
  // A cubic Bezier at its middle; a straight degree-1 spline.
  const std::vector<Xyz> cubic =
      bsplinePoints(3, {0, 0, 0, 0, 1, 1, 1, 1}, {}, {{0, 0, 0}, {1, 2, 0}, {3, 2, 0}, {4, 0, 0}}, 2);
  CHECK(cubic.size() == 3);
  if (cubic.size() == 3) {
    CHECK(same(cubic[0], {0, 0, 0}));
    CHECK(same(cubic[1], {2, 1.5, 0}));
    CHECK(same(cubic[2], {4, 0, 0}));
  }
  const std::vector<Xyz> straight = bsplinePoints(1, {0, 0, 1, 1}, {}, {{0, 0, 0}, {10, 0, 0}}, 4);
  CHECK(straight.size() == 5 && same(straight[1], {2.5, 0, 0}));
  CHECK(bsplinePoints(3, {0, 1}, {}, {{0, 0, 0}}, 4).empty());

  const Sketch3dModel m = model();
  const auto pointAt = [&m](const QString& id) -> std::optional<Xyz> {
    const Sketch3dPoint* p = m.point(id);
    return p != nullptr ? std::optional<Xyz>(p->at) : std::nullopt;
  };
  const Sketch3dCurve* line = m.curve(QStringLiteral("c4"));
  const Sketch3dCurve* arc = m.curve(QStringLiteral("c8"));
  CHECK(line != nullptr && curveSamples(*line, QJsonObject(), pointAt).size() == 2);
  if (arc != nullptr) {
    const std::vector<Xyz> samples =
        curveSamples(*arc, arc->json.value(QStringLiteral("geometry")).toObject(), pointAt);
    CHECK(samples.size() == 49);
    CHECK(!samples.empty() && same(samples.front(), {40, 30, 0}, 1e-9) && same(samples.back(), {30, 40, 0}, 1e-6));
  }

  // The palette's words.
  const Sketch3dItem* length = m.item(QStringLiteral("k5"));
  const Sketch3dItem* offset = m.item(QStringLiteral("k6"));
  CHECK(length != nullptr && offset != nullptr);
  if (length != nullptr && offset != nullptr) {
    const QString text = itemText(m, *length, QStringLiteral("mm"), 1.0);
    CHECK(text.startsWith(QStringLiteral("k5 Length c6: 30.00 mm")));
    CHECK(itemValueText(*length, 1.0) == QStringLiteral("30 mm"));
    CHECK(itemParameter(*length) == QStringLiteral("d1"));
    const QString delta = itemText(m, *offset, QStringLiteral("mm"), 1.0);
    CHECK(delta.contains(QStringLiteral("Delta Z p1 to p11: 20.00 mm")));
    CHECK(delta.contains(QStringLiteral("driven")));
    CHECK(itemValueText(*offset, 1.0) == QStringLiteral("20.00"));
    CHECK(itemParameter(*offset).isEmpty());
  }
  const Sketch3dReference* reference = m.reference(QStringLiteral("c12"));
  CHECK(reference != nullptr);
  if (reference != nullptr) {
    const QString text = referenceText(*reference);
    CHECK(text.contains(QStringLiteral("edge E{F1:side0|F1:side1} of F1.b0")));
    CHECK(text.contains(QStringLiteral("failed: edge E{F1:side0|F1:side1} was not found")));
    CHECK(text.contains(QStringLiteral("used by k3")));
  }
  // The same as the palette shows them, in words (mitcad#1652): kinds and
  // running numbers, the source by its feature's name, the reason in plain
  // words; no ids, topological names or coordinates.
  SourceNames names;
  names.feature = [](const QString& uid) { return uid == QStringLiteral("F1") ? QStringLiteral("Box1") : QString(); };
  if (length != nullptr && offset != nullptr) {
    CHECK(itemWords(m, *length, QStringLiteral("mm"), 1.0, names) == QStringLiteral("Length 1 on Line 2: 30.00 mm"));
    CHECK(itemWords(m, *offset, QStringLiteral("mm"), 1.0, names) ==
          QStringLiteral("Offset 1, Delta Z from Point 1 to Point 6: 20.00 mm; driven"));
  }
  if (reference != nullptr) {
    CHECK(referenceWords(m, *reference, names) ==
          QStringLiteral("Reference curve 1 (an edge of Box1); included in 3D; failed: the source edge was not found; "
                         "used by Point on Curve 1"));
    CHECK(referenceWords(m, *reference).startsWith(QStringLiteral("Reference curve 1 (an edge of a body); ")));
  }
  static const QRegularExpression ids(
      QStringLiteral(R"(\b[pckFO][0-9]+\b|[EV]\{|\bF[0-9]+:|\bb[0-9]+\b|-?[0-9]+\.[0-9]+, -?[0-9]+\.[0-9]+)"));
  for (const Sketch3dItem& item : m.items) {
    const QString words = itemWords(m, item, QStringLiteral("mm"), 1.0);
    CHECK(!words.isEmpty() && !ids.match(words).hasMatch());
  }
  for (const Sketch3dReference& record : m.references) {
    const QString words = referenceWords(m, record);
    CHECK(!words.isEmpty() && !ids.match(words).hasMatch());
  }
  CHECK(relationName(Relation3::ReleaseFromPlane) == QStringLiteral("Release from Plane"));
  CHECK(dimensionKey(Dimension3::DeltaZ) == QStringLiteral("delta_z"));
  CHECK(allRelations().size() == 22 && allDimensions().size() == 10);
}

} // namespace

int sketch3dEditTests() {
  failures = 0;
  relationTests();
  dimensionTests();
  deleteAndDragTests();
  sampleAndTextTests();
  return failures;
}
