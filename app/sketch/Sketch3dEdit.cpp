// SPDX-License-Identifier: MIT
#include "Sketch3dEdit.hpp"

#include <algorithm>
#include <array>
#include <cmath>

#include <QCoreApplication>
#include <QJsonValue>
#include <QSet>

#include "framework/UserText.hpp"

namespace mitcad::sketch {
namespace {

// The file's texts in the context Sketch3d, where lupdate records them too.
struct Text {
  Q_DECLARE_TR_FUNCTIONS(Sketch3d)
};

QJsonObject base(const QString& sketch, const char* cmd) {
  return {{QStringLiteral("cmd"), QString::fromLatin1(cmd)}, {QStringLiteral("sketch"), sketch}};
}

QJsonObject constraintCommand(const QString& sketch, const QJsonObject& constraint) {
  QJsonObject cmd = base(sketch, "sketch3d.add_constraint");
  cmd.insert(QStringLiteral("constraint"), constraint);
  return cmd;
}

QJsonObject dimensionOf(const QString& sketch, const QJsonObject& dimension, bool driven = false) {
  QJsonObject cmd = base(sketch, "sketch3d.add_dimension");
  cmd.insert(QStringLiteral("dimension"), dimension);
  if (driven) {
    cmd.insert(QStringLiteral("driven"), true);
  }
  return cmd;
}

QJsonArray xyzJson(const Xyz& at) { return {at[0], at[1], at[2]}; }

EditCommand refused(const QString& why) {
  EditCommand out;
  out.problem = why;
  return out;
}

EditCommand single(const QJsonObject& command) {
  EditCommand out;
  out.commands.append(command);
  return out;
}

bool isLine(const Sketch3dCurve* c) { return c != nullptr && c->type == QStringLiteral("line"); }
bool isRound(const Sketch3dCurve* c) {
  return c != nullptr && (c->type == QStringLiteral("arc") || c->type == QStringLiteral("circle"));
}
bool isSpline(const Sketch3dCurve* c) {
  return c != nullptr && (c->type == QStringLiteral("spline") || c->type == QStringLiteral("fitted_spline"));
}
bool isReference(const Sketch3dCurve* c) { return c != nullptr && c->type == QStringLiteral("reference"); }
// The plane a Symmetric or On Plane relation was given, or the sketch
// plane standing for it until the plane is picked (mitcad#1661: the
// command is not sent before the pick).
QJsonValue planeOrPlaceholder(const EditSelection& selection) {
  if (selection.plane.isObject()) {
    return selection.plane;
  }
  QJsonObject frame;
  frame.insert(QStringLiteral("frame"), QStringLiteral("z"));
  return frame;
}

// A direction of a relation (mitcad#1661): a line, or an arc's or circle's
// normal.
bool isDirection(const Sketch3dCurve* c) { return isLine(c) || isRound(c); }

// The selection's points that the model has, then the points of the
// selected curves that are its own (not reference curves), once each.
QStringList pointsOf(const Sketch3dModel& model, const EditSelection& selection, bool withCurves) {
  QStringList out;
  for (const QString& id : selection.points) {
    if (model.point(id) != nullptr && !out.contains(id)) {
      out << id;
    }
  }
  if (withCurves) {
    for (const QString& id : selection.curves) {
      const Sketch3dCurve* curve = model.curve(id);
      if (curve == nullptr || isReference(curve)) {
        continue;
      }
      for (const QString& p : curve->points) {
        if (model.point(p) != nullptr && !out.contains(p)) {
          out << p;
        }
      }
    }
  }
  return out;
}

// The curves of the selection that the model has.
std::vector<const Sketch3dCurve*> curvesOf(const Sketch3dModel& model, const EditSelection& selection) {
  std::vector<const Sketch3dCurve*> out;
  for (const QString& id : selection.curves) {
    if (const Sketch3dCurve* curve = model.curve(id)) {
      out.push_back(curve);
    }
  }
  return out;
}

// A curve's ends where routes join it: a line's, an arc's, an open
// spline's first and last point.
QStringList endsOf(const Sketch3dCurve& curve) {
  const QJsonObject& json = curve.json;
  if (curve.type == QStringLiteral("line") || curve.type == QStringLiteral("arc") ||
      curve.type == QStringLiteral("elliptical_arc")) {
    return {json.value(QStringLiteral("start")).toString(), json.value(QStringLiteral("end")).toString()};
  }
  const QString list = curve.type == QStringLiteral("spline")          ? QStringLiteral("control")
                       : curve.type == QStringLiteral("fitted_spline") ? QStringLiteral("points")
                                                                       : QString();
  if (list.isEmpty() || json.value(QStringLiteral("closed")).toBool()) {
    return {};
  }
  const QJsonArray points = json.value(list).toArray();
  if (points.isEmpty()) {
    return {};
  }
  return {points.first().toString(), points.last().toString()};
}

bool coincident(const Sketch3dModel& model, const QString& a, const QString& b) {
  if (a == b) {
    return true;
  }
  return std::any_of(model.items.begin(), model.items.end(), [&](const Sketch3dItem& item) {
    return !item.dimension && item.type == QStringLiteral("coincident") && item.entities.contains(a) &&
           item.entities.contains(b);
  });
}

QString axisName(int axis) { return QString(QChar(QLatin1Char("xyz"[axis]))); }

// An angle dimension (mitcad#1661): measured in radians, shown in
// degrees.
bool isAngle(const Sketch3dItem& item) { return item.dimension && item.type == QStringLiteral("angle"); }

QString degrees(double radians) { return QString::number(radians * 180.0 / 3.14159265358979323846, 'f', 2); }

QString number(double value, double millimetres) {
  const double scale = millimetres > 0.0 ? millimetres : 1.0;
  const double v = value / scale;
  return QString::number(std::abs(v) < 5e-7 ? 0.0 : v, 'f', 2);
}

} // namespace

std::vector<Relation3> allRelations() {
  return {Relation3::Coincident, Relation3::PointOnCurve, Relation3::Midpoint,    Relation3::Tangent,
          Relation3::Equal,      Relation3::Fix,          Relation3::Unfix,       Relation3::AlignX,
          Relation3::AlignY,     Relation3::AlignZ,       Relation3::OnSketchPlane, Relation3::ReleaseFromPlane,
          Relation3::ReleasePlane, Relation3::Detach,      Relation3::Parallel,    Relation3::Perpendicular,
          Relation3::Angle,        Relation3::Symmetric,   Relation3::Concentric,  Relation3::SameCenter,
          Relation3::Curvature,    Relation3::OnPlane};
}

std::vector<Dimension3> allDimensions() {
  return {Dimension3::Length,       Dimension3::DeltaX,   Dimension3::DeltaY,    Dimension3::DeltaZ,
          Dimension3::Radius,       Dimension3::Diameter, Dimension3::ArcLength, Dimension3::SplineLength,
          Dimension3::Angle,        Dimension3::TangentMagnitude};
}

bool takesPlane(Relation3 relation) { return relation == Relation3::Symmetric || relation == Relation3::OnPlane; }

QString relationName(Relation3 relation) {
  switch (relation) {
  case Relation3::Coincident:
    return Text::tr("Coincident");
  case Relation3::PointOnCurve:
    return Text::tr("Point on Curve");
  case Relation3::Midpoint:
    return Text::tr("Midpoint");
  case Relation3::Tangent:
    return Text::tr("Tangent");
  case Relation3::Equal:
    return Text::tr("Equal");
  case Relation3::Fix:
    return Text::tr("Fix");
  case Relation3::Unfix:
    return Text::tr("Unfix");
  case Relation3::AlignX:
    return Text::tr("Align X");
  case Relation3::AlignY:
    return Text::tr("Align Y");
  case Relation3::AlignZ:
    return Text::tr("Align Z");
  case Relation3::OnSketchPlane:
    return Text::tr("On Sketch Plane");
  case Relation3::ReleaseFromPlane:
    return Text::tr("Release from Plane");
  case Relation3::ReleasePlane:
    return Text::tr("Release Plane");
  case Relation3::Detach:
    return Text::tr("Detach");
  case Relation3::Parallel:
    return Text::tr("Parallel");
  case Relation3::Perpendicular:
    return Text::tr("Perpendicular");
  case Relation3::Angle:
    return Text::tr("Angle");
  case Relation3::Symmetric:
    return Text::tr("Symmetric");
  case Relation3::Concentric:
    return Text::tr("Concentric");
  case Relation3::SameCenter:
    return Text::tr("Same Center");
  case Relation3::Curvature:
    return Text::tr("Curvature");
  case Relation3::OnPlane:
    return Text::tr("On Plane");
  }
  return QString();
}

QString relationKey(Relation3 relation) {
  switch (relation) {
  case Relation3::Coincident:
    return QStringLiteral("coincident");
  case Relation3::PointOnCurve:
    return QStringLiteral("point_on");
  case Relation3::Midpoint:
    return QStringLiteral("midpoint");
  case Relation3::Tangent:
    return QStringLiteral("tangent");
  case Relation3::Equal:
    return QStringLiteral("equal");
  case Relation3::Fix:
    return QStringLiteral("fix");
  case Relation3::Unfix:
    return QStringLiteral("unfix");
  case Relation3::AlignX:
    return QStringLiteral("align_x");
  case Relation3::AlignY:
    return QStringLiteral("align_y");
  case Relation3::AlignZ:
    return QStringLiteral("align_z");
  case Relation3::OnSketchPlane:
    return QStringLiteral("on_sketch_plane");
  case Relation3::ReleaseFromPlane:
    return QStringLiteral("release_from_plane");
  case Relation3::ReleasePlane:
    return QStringLiteral("release_plane");
  case Relation3::Detach:
    return QStringLiteral("detach");
  case Relation3::Parallel:
    return QStringLiteral("parallel");
  case Relation3::Perpendicular:
    return QStringLiteral("perpendicular");
  case Relation3::Angle:
    return QStringLiteral("angle_relation");
  case Relation3::Symmetric:
    return QStringLiteral("symmetric");
  case Relation3::Concentric:
    return QStringLiteral("concentric");
  case Relation3::SameCenter:
    return QStringLiteral("same_center");
  case Relation3::Curvature:
    return QStringLiteral("curvature");
  case Relation3::OnPlane:
    return QStringLiteral("on_plane");
  }
  return QString();
}

QString dimensionName(Dimension3 dimension) {
  switch (dimension) {
  case Dimension3::Length:
    return Text::tr("Length");
  case Dimension3::DeltaX:
    return Text::tr("Delta X");
  case Dimension3::DeltaY:
    return Text::tr("Delta Y");
  case Dimension3::DeltaZ:
    return Text::tr("Delta Z");
  case Dimension3::Radius:
    return Text::tr("Radius");
  case Dimension3::Diameter:
    return Text::tr("Diameter");
  case Dimension3::ArcLength:
    return Text::tr("Arc Length");
  case Dimension3::SplineLength:
    return Text::tr("Spline Length");
  case Dimension3::Angle:
    return Text::tr("Angle");
  case Dimension3::TangentMagnitude:
    return Text::tr("Tangent Magnitude");
  }
  return QString();
}

QString dimensionKey(Dimension3 dimension) {
  switch (dimension) {
  case Dimension3::Length:
    return QStringLiteral("length");
  case Dimension3::DeltaX:
    return QStringLiteral("delta_x");
  case Dimension3::DeltaY:
    return QStringLiteral("delta_y");
  case Dimension3::DeltaZ:
    return QStringLiteral("delta_z");
  case Dimension3::Radius:
    return QStringLiteral("radius");
  case Dimension3::Diameter:
    return QStringLiteral("diameter");
  case Dimension3::ArcLength:
    return QStringLiteral("arc_length");
  case Dimension3::SplineLength:
    return QStringLiteral("spline_length");
  case Dimension3::Angle:
    return QStringLiteral("angle");
  case Dimension3::TangentMagnitude:
    return QStringLiteral("tangent_magnitude");
  }
  return QString();
}

bool isFixed(const Sketch3dModel& model, const QString& point) {
  return std::any_of(model.items.begin(), model.items.end(), [&point](const Sketch3dItem& item) {
    return !item.dimension && item.type == QStringLiteral("fix") && item.entities.contains(point);
  });
}

Relation3 fixToggle(const Sketch3dModel& model, const EditSelection& selection) {
  const QStringList points = pointsOf(model, selection, true);
  const bool all = !points.isEmpty() &&
                   std::all_of(points.begin(), points.end(), [&model](const QString& p) { return isFixed(model, p); });
  return all ? Relation3::Unfix : Relation3::Fix;
}

EditCommand relationCommand(const Sketch3dModel& model, const QString& sketch, Relation3 relation,
                            const EditSelection& selection) {
  const QStringList points = pointsOf(model, selection, false);
  const std::vector<const Sketch3dCurve*> curves = curvesOf(model, selection);
  switch (relation) {
  case Relation3::Coincident:
    if (points.size() == 2 && curves.empty()) {
      if (coincident(model, points[0], points[1])) {
        return refused(Text::tr("%1 and %2 are coincident already")
                           .arg(entityName(model, points[0]), entityName(model, points[1])));
      }
      return single(constraintCommand(sketch, {{QStringLiteral("type"), QStringLiteral("coincident")},
                                               {QStringLiteral("a"), points[0]},
                                               {QStringLiteral("b"), points[1]}}));
    }
    if (points.size() == 1 && curves.size() == 1) {
      return relationCommand(model, sketch, Relation3::PointOnCurve, selection);
    }
    return refused(Text::tr("Select two points, or a point and a curve."));
  case Relation3::PointOnCurve:
  case Relation3::Midpoint: {
    const bool middle = relation == Relation3::Midpoint;
    if (points.size() != 1 || curves.size() != 1) {
      return refused(middle ? Text::tr("Select a point and a line, an arc or a reference curve.")
                            : Text::tr("Select a point and a curve."));
    }
    const Sketch3dCurve* curve = curves.front();
    if (curve->points.contains(points[0])) {
      return refused(Text::tr("%1 is a point of %2 already")
                         .arg(entityName(model, points[0]), entityName(model, curve->id)));
    }
    if (middle && !isLine(curve) && curve->type != QStringLiteral("arc") && !isReference(curve)) {
      return refused(Text::tr("A midpoint is of a line, an arc or a reference curve."));
    }
    return single(constraintCommand(
        sketch, {{QStringLiteral("type"), middle ? QStringLiteral("midpoint") : QStringLiteral("point_on")},
                 {QStringLiteral("point"), points[0]},
                 {QStringLiteral("curve"), curve->id}}));
  }
  case Relation3::Tangent: {
    if (curves.size() != 2 || points.size() > 1) {
      return refused(Text::tr("Select two curves that share an end (and the point, where they share several)."));
    }
    for (const Sketch3dCurve* curve : curves) {
      if (!(isLine(curve) || isRound(curve) || isSpline(curve))) {
        return refused(Text::tr("Tangency is between lines, arcs, circles and splines."));
      }
    }
    if (isLine(curves[0]) && isLine(curves[1])) {
      return refused(Text::tr("Two lines are not tangent: one of them is an arc, a circle or a spline."));
    }
    QString at = points.isEmpty() ? QString() : points.front();
    if (at.isEmpty()) {
      for (const QString& a : endsOf(*curves[0])) {
        for (const QString& b : endsOf(*curves[1])) {
          if (at.isEmpty() && !a.isEmpty() && coincident(model, a, b)) {
            at = a;
          }
        }
      }
    }
    if (at.isEmpty()) {
      return refused(Text::tr("%1 and %2 share no end: select the point where they touch too.")
                         .arg(entityName(model, curves[0]->id), entityName(model, curves[1]->id)));
    }
    return single(constraintCommand(sketch, {{QStringLiteral("type"), QStringLiteral("tangent")},
                                             {QStringLiteral("a"), curves[0]->id},
                                             {QStringLiteral("b"), curves[1]->id},
                                             {QStringLiteral("at"), at}}));
  }
  case Relation3::Equal: {
    if (curves.size() != 2 || !points.isEmpty() ||
        !((isLine(curves[0]) && isLine(curves[1])) || (isRound(curves[0]) && isRound(curves[1])))) {
      return refused(Text::tr("Select two lines (equal lengths) or two arcs or circles (equal radii)."));
    }
    return single(constraintCommand(sketch, {{QStringLiteral("type"), QStringLiteral("equal")},
                                             {QStringLiteral("a"), curves[0]->id},
                                             {QStringLiteral("b"), curves[1]->id}}));
  }
  case Relation3::Fix:
  case Relation3::Unfix: {
    const bool fix = relation == Relation3::Fix;
    QStringList chosen;
    for (const QString& p : pointsOf(model, selection, true)) {
      if (isFixed(model, p) != fix) {
        chosen << p;
      }
    }
    if (chosen.isEmpty()) {
      if (pointsOf(model, selection, true).isEmpty()) {
        return refused(Text::tr("Select points or curves of the sketch."));
      }
      return refused(fix ? Text::tr("The selected points are fixed already.")
                         : Text::tr("None of the selected points is fixed."));
    }
    QJsonObject cmd = base(sketch, "sketch3d.fix");
    cmd.insert(QStringLiteral("points"), QJsonArray::fromStringList(chosen));
    cmd.insert(QStringLiteral("fixed"), fix);
    return single(cmd);
  }
  case Relation3::AlignX:
  case Relation3::AlignY:
  case Relation3::AlignZ: {
    const int axis = relation == Relation3::AlignX ? 0 : relation == Relation3::AlignY ? 1 : 2;
    EditCommand out;
    for (const Sketch3dCurve* curve : curves) {
      const QJsonObject geometry = curve->json.value(QStringLiteral("geometry")).toObject();
      double along = 0.0;
      QJsonObject constraint;
      if (isLine(curve)) {
        const Sketch3dPoint* a = model.point(curve->json.value(QStringLiteral("start")).toString());
        const Sketch3dPoint* b = model.point(curve->json.value(QStringLiteral("end")).toString());
        if (a == nullptr || b == nullptr) {
          continue;
        }
        along = b->at[static_cast<std::size_t>(axis)] - a->at[static_cast<std::size_t>(axis)];
        constraint = {{QStringLiteral("type"), QStringLiteral("align")}, {QStringLiteral("line"), curve->id}};
      } else if (isRound(curve) && !curve->planeBound) {
        const QJsonArray n = geometry.value(QStringLiteral("normal")).toArray();
        along = n.size() >= 3 ? n.at(axis).toDouble() : 0.0;
        constraint = {{QStringLiteral("type"), QStringLiteral("normal_align")}, {QStringLiteral("curve"), curve->id}};
      } else {
        continue;
      }
      constraint.insert(QStringLiteral("axis"), axisName(axis));
      constraint.insert(QStringLiteral("sense"), along < 0.0 ? QStringLiteral("negative") : QStringLiteral("positive"));
      out.commands.append(constraintCommand(sketch, constraint));
    }
    if (out.commands.isEmpty()) {
      return refused(Text::tr("Select lines, or arcs and circles whose normal is free."));
    }
    return out;
  }
  case Relation3::OnSketchPlane:
  case Relation3::ReleaseFromPlane: {
    const bool keep = relation == Relation3::OnSketchPlane;
    QStringList chosen;
    for (const QString& p : pointsOf(model, selection, true)) {
      if (model.point(p)->onSketchPlane != keep) {
        chosen << p;
      }
    }
    if (chosen.isEmpty()) {
      return refused(keep ? Text::tr("Select points that On Sketch Plane does not keep yet.")
                          : Text::tr("Select points that On Sketch Plane keeps."));
    }
    if (keep) {
      return single(constraintCommand(sketch, {{QStringLiteral("type"), QStringLiteral("on_sketch_plane")},
                                               {QStringLiteral("points"), QJsonArray::fromStringList(chosen)}}));
    }
    QJsonObject cmd = base(sketch, "sketch3d.release_from_plane");
    cmd.insert(QStringLiteral("points"), QJsonArray::fromStringList(chosen));
    return single(cmd);
  }
  case Relation3::ReleasePlane: {
    QStringList chosen;
    for (const Sketch3dCurve* curve : curves) {
      if (isRound(curve) && curve->planeBound) {
        chosen << curve->id;
      }
    }
    if (chosen.isEmpty()) {
      return refused(Text::tr("Select arcs or circles kept in the sketch plane."));
    }
    QJsonObject cmd = base(sketch, "sketch3d.release_plane");
    cmd.insert(QStringLiteral("curves"), QJsonArray::fromStringList(chosen));
    return single(cmd);
  }
  case Relation3::Detach: {
    QStringList chosen;
    for (const QString& p : points) {
      if (model.referenceOf(p) != nullptr && !chosen.contains(p)) {
        chosen << p;
      }
    }
    for (const Sketch3dCurve* curve : curves) {
      if (isReference(curve)) {
        for (const QString& p : detachPoints(model, curve->id)) {
          if (!chosen.contains(p)) {
            chosen << p;
          }
        }
      }
    }
    if (chosen.isEmpty()) {
      return refused(Text::tr("Select points attached to a source, or a reference curve with points on it."));
    }
    QJsonObject cmd = base(sketch, "sketch3d.detach");
    cmd.insert(QStringLiteral("points"), QJsonArray::fromStringList(chosen));
    return single(cmd);
  }
  case Relation3::Parallel:
  case Relation3::Perpendicular:
  case Relation3::Angle: {
    if (curves.size() != 2 || !points.isEmpty() || !isDirection(curves[0]) || !isDirection(curves[1])) {
      return refused(Text::tr("Select two lines, or a line and an arc or circle (its normal)."));
    }
    if (curves[0]->planeBound && curves[1]->planeBound && isRound(curves[0]) && isRound(curves[1])) {
      return refused(Text::tr("Both normals are kept by the sketch's planes: release one plane first."));
    }
    if (relation == Relation3::Angle) {
      // A driving angle at the angle they have now: it holds them there
      // and its value can be typed.
      return single(dimensionOf(sketch, {{QStringLiteral("type"), QStringLiteral("angle")},
                                         {QStringLiteral("a"), curves[0]->id},
                                         {QStringLiteral("b"), curves[1]->id}}));
    }
    return single(constraintCommand(sketch, {{QStringLiteral("type"), relation == Relation3::Parallel
                                                                         ? QStringLiteral("parallel")
                                                                         : QStringLiteral("perpendicular")},
                                             {QStringLiteral("a"), curves[0]->id},
                                             {QStringLiteral("b"), curves[1]->id}}));
  }
  case Relation3::Symmetric: {
    if (points.size() != 2 || !curves.empty()) {
      return refused(Text::tr("Select two points, then pick the plane they are mirrored across."));
    }
    // The plane is picked after the button; until then the frame's XY
    // stands for it (the command is not sent before the pick).
    const QJsonValue plane = planeOrPlaceholder(selection);
    return single(constraintCommand(sketch, {{QStringLiteral("type"), QStringLiteral("symmetric")},
                                             {QStringLiteral("a"), points[0]},
                                             {QStringLiteral("b"), points[1]},
                                             {QStringLiteral("plane"), plane}}));
  }
  case Relation3::OnPlane: {
    const QStringList chosen = pointsOf(model, selection, true);
    if (chosen.isEmpty()) {
      return refused(Text::tr("Select points or curves, then pick the plane they are kept on."));
    }
    const QJsonValue plane = planeOrPlaceholder(selection);
    return single(constraintCommand(sketch, {{QStringLiteral("type"), QStringLiteral("on_plane")},
                                             {QStringLiteral("points"), QJsonArray::fromStringList(chosen)},
                                             {QStringLiteral("plane"), plane}}));
  }
  case Relation3::Concentric:
  case Relation3::SameCenter: {
    if (curves.size() != 2 || !points.isEmpty() || !isRound(curves[0]) || !isRound(curves[1])) {
      return refused(Text::tr("Select two arcs or circles."));
    }
    QJsonObject constraint{{QStringLiteral("type"), QStringLiteral("concentric")},
                           {QStringLiteral("a"), curves[0]->id},
                           {QStringLiteral("b"), curves[1]->id}};
    if (relation == Relation3::SameCenter) {
      constraint.insert(QStringLiteral("normal"), false);
    }
    return single(constraintCommand(sketch, constraint));
  }
  case Relation3::Curvature: {
    if (curves.size() != 2 || points.size() > 1) {
      return refused(Text::tr("Select a spline and a line, an arc or a spline that joins it at an end."));
    }
    if (!(isSpline(curves[0]) || isSpline(curves[1]))) {
      return refused(Text::tr("Curvature continuity is where a spline ends: select a spline too."));
    }
    for (const Sketch3dCurve* curve : curves) {
      if (!(isLine(curve) || isRound(curve) || isSpline(curve))) {
        return refused(Text::tr("Curvature continuity joins a spline to a line, an arc or a spline."));
      }
    }
    QString at = points.isEmpty() ? QString() : points.front();
    if (at.isEmpty()) {
      for (const QString& a : endsOf(*curves[0])) {
        for (const QString& b : endsOf(*curves[1])) {
          if (at.isEmpty() && !a.isEmpty() && coincident(model, a, b)) {
            at = a;
          }
        }
      }
    }
    if (at.isEmpty()) {
      return refused(Text::tr("%1 and %2 share no end: select the point where they join too.")
                         .arg(entityName(model, curves[0]->id), entityName(model, curves[1]->id)));
    }
    return single(constraintCommand(sketch, {{QStringLiteral("type"), QStringLiteral("curvature")},
                                             {QStringLiteral("a"), curves[0]->id},
                                             {QStringLiteral("b"), curves[1]->id},
                                             {QStringLiteral("at"), at}}));
  }
  }
  return refused(QString());
}

EditCommand dimensionCommand(const Sketch3dModel& model, const QString& sketch, Dimension3 dimension,
                             const EditSelection& selection) {
  const QStringList points = pointsOf(model, selection, false);
  const std::vector<const Sketch3dCurve*> curves = curvesOf(model, selection);
  const Sketch3dCurve* curve = curves.size() == 1 && points.isEmpty() ? curves.front() : nullptr;
  switch (dimension) {
  case Dimension3::Length:
    if (!isLine(curve)) {
      return refused(Text::tr("Select one line: its true length."));
    }
    return single(dimensionOf(sketch, {{QStringLiteral("type"), QStringLiteral("length")},
                                       {QStringLiteral("line"), curve->id}}));
  case Dimension3::DeltaX:
  case Dimension3::DeltaY:
  case Dimension3::DeltaZ: {
    const int axis = dimension == Dimension3::DeltaX ? 0 : dimension == Dimension3::DeltaY ? 1 : 2;
    QString a;
    QString b;
    if (points.size() == 2 && curves.empty()) {
      a = points[0];
      b = points[1];
    } else if (isLine(curve)) {
      a = curve->json.value(QStringLiteral("start")).toString();
      b = curve->json.value(QStringLiteral("end")).toString();
    } else {
      return refused(Text::tr("Select two points, or a line: the signed offset from the first to the second."));
    }
    return single(dimensionOf(sketch, {{QStringLiteral("type"), QStringLiteral("offset")},
                                       {QStringLiteral("a"), a},
                                       {QStringLiteral("b"), b},
                                       {QStringLiteral("axis"), axisName(axis)}}));
  }
  case Dimension3::Radius:
  case Dimension3::Diameter:
    if (!isRound(curve)) {
      return refused(Text::tr("Select one arc or circle."));
    }
    return single(dimensionOf(sketch, {{QStringLiteral("type"), dimension == Dimension3::Radius
                                                                    ? QStringLiteral("radius")
                                                                    : QStringLiteral("diameter")},
                                       {QStringLiteral("curve"), curve->id}}));
  case Dimension3::ArcLength:
    if (curve == nullptr || curve->type != QStringLiteral("arc")) {
      return refused(Text::tr("Select one arc."));
    }
    return single(dimensionOf(sketch, {{QStringLiteral("type"), QStringLiteral("arc_length")},
                                       {QStringLiteral("curve"), curve->id}}));
  case Dimension3::SplineLength:
    if (!isSpline(curve)) {
      return refused(Text::tr("Select one spline."));
    }
    // Driving at its length now (mitcad#1661); Driven makes it measure.
    return single(dimensionOf(sketch, {{QStringLiteral("type"), QStringLiteral("spline_length")},
                                       {QStringLiteral("curve"), curve->id}}));
  case Dimension3::Angle:
    if (curves.size() != 2 || !points.isEmpty() || !isDirection(curves[0]) || !isDirection(curves[1])) {
      return refused(Text::tr("Select two lines, or a line and an arc or circle (its normal)."));
    }
    return single(dimensionOf(sketch, {{QStringLiteral("type"), QStringLiteral("angle")},
                                       {QStringLiteral("a"), curves[0]->id},
                                       {QStringLiteral("b"), curves[1]->id}}));
  case Dimension3::TangentMagnitude: {
    const Sketch3dCurve* spline = curves.size() == 1 && isSpline(curves.front()) ? curves.front() : nullptr;
    if (spline == nullptr || points.size() != 1) {
      return refused(Text::tr("Select a spline and its start or end point."));
    }
    const QStringList ends = endsOf(*spline);
    if (ends.size() != 2 || !ends.contains(points[0])) {
      return refused(Text::tr("%1 is not the start or end of %2.")
                         .arg(entityName(model, points[0]), entityName(model, spline->id)));
    }
    return single(dimensionOf(sketch, {{QStringLiteral("type"), QStringLiteral("tangent_magnitude")},
                                       {QStringLiteral("curve"), spline->id},
                                       {QStringLiteral("end"), points[0] == ends[0] ? QStringLiteral("start")
                                                                                    : QStringLiteral("end")}}));
  }
  }
  return refused(QString());
}

EditCommand autoDimensionCommand(const Sketch3dModel& model, const QString& sketch, const EditSelection& selection) {
  const QStringList points = pointsOf(model, selection, false);
  const std::vector<const Sketch3dCurve*> curves = curvesOf(model, selection);
  if (points.size() == 2 && curves.empty()) {
    return refused(Text::tr("Two points are dimensioned along an axis: use Delta X, Delta Y or Delta Z in the 3D "
                            "Sketch Palette."));
  }
  if (curves.size() == 1 && points.isEmpty()) {
    const Sketch3dCurve* curve = curves.front();
    if (isLine(curve)) {
      return dimensionCommand(model, sketch, Dimension3::Length, selection);
    }
    if (curve->type == QStringLiteral("arc")) {
      return dimensionCommand(model, sketch, Dimension3::Radius, selection);
    }
    if (curve->type == QStringLiteral("circle")) {
      return dimensionCommand(model, sketch, Dimension3::Diameter, selection);
    }
    if (isSpline(curve)) {
      return dimensionCommand(model, sketch, Dimension3::SplineLength, selection);
    }
  }
  return refused(Text::tr("Select a line, an arc, a circle or a spline to dimension."));
}

QStringList dependentsOf(const Sketch3dModel& model, const QStringList& items) {
  QSet<QString> listed(items.begin(), items.end());
  QSet<QString> points;
  for (const QString& id : items) {
    if (model.point(id) != nullptr) {
      points.insert(id);
    }
  }
  QStringList out;
  if (points.isEmpty()) {
    return out;
  }
  const auto uses = [&points](const QStringList& ids) {
    return std::any_of(ids.begin(), ids.end(), [&points](const QString& id) { return points.contains(id); });
  };
  for (const Sketch3dCurve& curve : model.curves) {
    if (!listed.contains(curve.id) && uses(curve.points)) {
      out << curve.id;
    }
  }
  for (const Sketch3dItem& item : model.items) {
    if (!listed.contains(item.id) && uses(item.entities)) {
      out << item.id;
    }
  }
  return out;
}

QStringList dragPoints(const Sketch3dModel& model, const QString& curveId) {
  const Sketch3dCurve* curve = model.curve(curveId);
  if (curve == nullptr || isReference(curve)) {
    return {};
  }
  QStringList out;
  for (const QString& p : curve->points) {
    if (model.point(p) != nullptr) {
      out << p;
    }
  }
  return out;
}

QJsonObject dragCommand(const QString& sketch, const QStringList& points, const std::vector<Xyz>& targets,
                        bool preview) {
  QJsonObject cmd = base(sketch, "sketch3d.drag");
  QJsonArray goals;
  for (int i = 0; i < points.size() && static_cast<std::size_t>(i) < targets.size(); ++i) {
    goals.append(QJsonObject{{QStringLiteral("point"), points[i]},
                             {QStringLiteral("to"), xyzJson(targets[static_cast<std::size_t>(i)])}});
  }
  cmd.insert(QStringLiteral("points"), goals);
  if (preview) {
    cmd.insert(QStringLiteral("preview"), true);
  }
  return cmd;
}

QJsonObject handleDragCommand(const QString& sketch, const QString& curve, int end, const Xyz& endPoint,
                              const Xyz& tip, bool preview) {
  QJsonObject cmd = base(sketch, "sketch3d.drag");
  cmd.insert(QStringLiteral("tangents"),
             QJsonArray{QJsonObject{{QStringLiteral("curve"), curve},
                                    {QStringLiteral("end"), end == 0 ? QStringLiteral("start") : QStringLiteral("end")},
                                    {QStringLiteral("to"), xyzJson(tangentOfTip(endPoint, tip))}}});
  if (preview) {
    cmd.insert(QStringLiteral("preview"), true);
  }
  return cmd;
}

Xyz handleTip(const Xyz& at, const Xyz& tangent) { return add(at, scaled(tangent, 1.0 / 3.0)); }

Xyz tangentOfTip(const Xyz& at, const Xyz& tip) { return scaled(sub(tip, at), 3.0); }

std::vector<TangentHandle> tangentHandles(const Sketch3dModel& model) {
  std::vector<TangentHandle> out;
  for (const Sketch3dCurve& curve : model.curves) {
    if (curve.type != QStringLiteral("fitted_spline") || curve.json.value(QStringLiteral("closed")).toBool()) {
      continue;
    }
    const QJsonArray fit = curve.json.value(QStringLiteral("points")).toArray();
    const QJsonArray tangents =
        curve.json.value(QStringLiteral("geometry")).toObject().value(QStringLiteral("end_tangents")).toArray();
    const QJsonArray ends = curve.json.value(QStringLiteral("ends")).toArray();
    if (fit.size() < 2 || tangents.size() != 2) {
      continue;
    }
    for (const int end : {0, 1}) {
      const QString pointId = end == 0 ? fit.first().toString() : fit.last().toString();
      const bool freeEnd = ends.size() == 2 && ends.at(end).toString() == QStringLiteral("free");
      const bool tangent = std::any_of(model.items.begin(), model.items.end(), [&](const Sketch3dItem& item) {
        const QString at = item.json.value(QStringLiteral("at")).toString();
        return !item.dimension && item.type == QStringLiteral("tangent") && item.entities.contains(curve.id) &&
               coincident(model, at, pointId);
      });
      const Sketch3dPoint* point = model.point(pointId);
      const QJsonArray t = tangents.at(end).toArray();
      if ((!freeEnd && !tangent) || point == nullptr || t.size() < 3) {
        continue;
      }
      TangentHandle handle;
      handle.curve = curve.id;
      handle.point = pointId;
      handle.end = end;
      handle.at = point->at;
      handle.tip = handleTip(point->at, {t.at(0).toDouble(), t.at(1).toDouble(), t.at(2).toDouble()});
      out.push_back(handle);
    }
  }
  return out;
}

std::vector<Xyz> bsplinePoints(int degree, const std::vector<double>& knots, const std::vector<double>& weights,
                               const std::vector<Xyz>& control, int count) {
  const int n = static_cast<int>(control.size());
  if (degree < 1 || n < degree + 1 || static_cast<int>(knots.size()) != n + degree + 1 || count < 1) {
    return {};
  }
  const bool rational = static_cast<int>(weights.size()) == n;
  const double lo = knots[static_cast<std::size_t>(degree)];
  const double hi = knots[static_cast<std::size_t>(n)];
  if (!(hi > lo)) {
    return {};
  }
  const auto knot = [&knots](int i) { return knots[static_cast<std::size_t>(i)]; };
  std::vector<Xyz> out;
  out.reserve(static_cast<std::size_t>(count) + 1);
  std::vector<std::array<double, 4>> d(static_cast<std::size_t>(degree) + 1);
  for (int i = 0; i <= count; ++i) {
    const double t = lo + (hi - lo) * i / count;
    int k = degree;
    while (k < n - 1 && t >= knot(k + 1)) {
      ++k;
    }
    for (int j = 0; j <= degree; ++j) {
      const std::size_t index = static_cast<std::size_t>(k - degree + j);
      const double w = rational ? weights[index] : 1.0;
      const Xyz& p = control[index];
      d[static_cast<std::size_t>(j)] = {p[0] * w, p[1] * w, p[2] * w, w};
    }
    for (int r = 1; r <= degree; ++r) {
      for (int j = degree; j >= r; --j) {
        const int index = k - degree + j;
        const double span = knot(index + degree + 1 - r) - knot(index);
        const double alpha = span > 0.0 ? (t - knot(index)) / span : 0.0;
        auto& to = d[static_cast<std::size_t>(j)];
        const auto& from = d[static_cast<std::size_t>(j) - 1];
        for (std::size_t c = 0; c < 4; ++c) {
          to[c] = (1.0 - alpha) * from[c] + alpha * to[c];
        }
      }
    }
    const auto& h = d[static_cast<std::size_t>(degree)];
    if (std::abs(h[3]) < 1e-300) {
      return {};
    }
    out.push_back({h[0] / h[3], h[1] / h[3], h[2] / h[3]});
  }
  return out;
}

QString itemText(const Sketch3dModel& model, const Sketch3dItem& item, const QString& unit, double millimetres) {
  // The agents' and the log's: English, with the ids (mitcad#1652).
  QString kind = itemKindEnglish(item.type, item.dimension);
  QString on = item.entities.join(QStringLiteral(", "));
  if (item.dimension && item.type == QStringLiteral("offset")) {
    const QString axis = item.json.value(QStringLiteral("axis")).toString().toUpper();
    kind = QStringLiteral("Delta %1").arg(axis);
    on = QStringLiteral("%1 to %2").arg(item.json.value(QStringLiteral("a")).toString(),
                                        item.json.value(QStringLiteral("b")).toString());
  }
  QString text = on.isEmpty() ? QStringLiteral("%1 %2").arg(item.id, kind)
                              : QStringLiteral("%1 %2 %3").arg(item.id, kind, on);
  QStringList state;
  if (item.dimension) {
    const QJsonValue measured = item.json.value(QStringLiteral("measured"));
    if (measured.isDouble() && isAngle(item)) {
      text += QStringLiteral(": %1 deg").arg(degrees(measured.toDouble()));
    } else if (measured.isDouble()) {
      text += QStringLiteral(": %1 %2").arg(number(measured.toDouble(), millimetres), unit);
    }
    if (item.driven) {
      state << QStringLiteral("driven");
    }
  }
  if (item.inPlane) {
    state << QStringLiteral("In plane");
  }
  if (item.redundant) {
    state << QStringLiteral("redundant");
  }
  if (model.failureItems.contains(item.id)) {
    state << QStringLiteral("involved in the failure");
  }
  return state.isEmpty() ? text : QStringLiteral("%1; %2").arg(text, state.join(QStringLiteral("; ")));
}

QString itemWords(const Sketch3dModel& model, const Sketch3dItem& item, const QString& unit, double millimetres,
                  const SourceNames& names) {
  // Its kind and number, what it is on by their kinds and numbers
  // (mitcad#1652): no ids.
  const QString name = entityName(model, item.id);
  QString text;
  if (item.dimension && item.type == QStringLiteral("offset")) {
    const QString axis = item.json.value(QStringLiteral("axis")).toString().toUpper();
    text = Text::tr("%1, Delta %2 from %3 to %4")
               .arg(name, axis, entityName(model, item.json.value(QStringLiteral("a")).toString()),
                    entityName(model, item.json.value(QStringLiteral("b")).toString()));
  } else {
    const QString on = entityNames(model, item.entities);
    text = on.isEmpty() ? name : Text::tr("%1 on %2").arg(name, on);
  }
  QStringList state;
  if (item.dimension) {
    const QJsonValue measured = item.json.value(QStringLiteral("measured"));
    if (measured.isDouble() && isAngle(item)) {
      text = Text::tr("%1: %2°").arg(text, degrees(measured.toDouble()));
    } else if (measured.isDouble()) {
      text = Text::tr("%1: %2 %3").arg(text, number(measured.toDouble(), millimetres), unit);
    }
    if (item.driven) {
      state << Text::tr("driven");
    }
  }
  // An attachment's or On Face's source.
  if (const Sketch3dReference* record = model.reference(item.id)) {
    state << Text::tr("linked to %1").arg(sourceText(record->source, names));
  }
  if (item.inPlane) {
    state << Text::tr("In plane");
  }
  if (item.redundant) {
    state << Text::tr("redundant");
  }
  if (model.failureItems.contains(item.id)) {
    state << Text::tr("involved in the failure");
  }
  return state.isEmpty() ? text : Text::tr("%1; %2").arg(text, state.join(QStringLiteral("; ")));
}

QString itemValueText(const Sketch3dItem& item, double millimetres) {
  if (!item.dimension) {
    return QString();
  }
  if (item.driven) {
    const QJsonValue measured = item.json.value(QStringLiteral("measured"));
    if (measured.isDouble() && isAngle(item)) {
      return QStringLiteral("%1 deg").arg(degrees(measured.toDouble()));
    }
    return measured.isDouble() ? number(measured.toDouble(), millimetres) : QString();
  }
  const QString expression = item.json.value(QStringLiteral("expression")).toString();
  return expression.isEmpty() ? item.json.value(QStringLiteral("value")).toString() : expression;
}

QString itemParameter(const Sketch3dItem& item) {
  return item.dimension && !item.driven ? item.json.value(QStringLiteral("value")).toString() : QString();
}

QString referenceText(const Sketch3dReference& reference) {
  // The agents' and the log's: English, with the ids (mitcad#1652).
  QStringList parts;
  parts << QStringLiteral("%1: %2").arg(reference.id, sourceDetail(reference.source));
  if (!reference.body.isEmpty()) {
    parts << QStringLiteral("body %1").arg(reference.body);
  }
  if (!reference.occurrence.isEmpty()) {
    parts << QStringLiteral("through occurrence %1").arg(reference.occurrence);
  }
  if (reference.kind == QStringLiteral("face")) {
    parts << QStringLiteral("its point slides on the face"); // mitcad#1549
  } else {
    parts << (reference.mode == QStringLiteral("project") ? QStringLiteral("projected onto the sketch plane")
                                                          : QStringLiteral("included in 3D"));
  }
  if (reference.status == QStringLiteral("failed")) {
    parts << QStringLiteral("failed: %1").arg(reference.detail);
  } else if (reference.status == QStringLiteral("unknown")) {
    parts << QStringLiteral("not evaluated");
  }
  if (!reference.users.isEmpty()) {
    parts << QStringLiteral("used by %1").arg(reference.users.join(QStringLiteral(", ")));
  }
  return parts.join(QStringLiteral("; "));
}

QString referenceWords(const Sketch3dModel& model, const Sketch3dReference& reference, const SourceNames& names) {
  // What it is and its source in words (mitcad#1652): "Reference curve 1
  // (an edge of Extrude2)", "Attachment 1 (the origin)"; the source's
  // component is in its words, its body and occurrence ids are the agents'.
  QStringList parts;
  const QString name = entityName(model, reference.id);
  const QString source = sourceText(reference.source, names);
  parts << (name.isEmpty() ? source : Text::tr("%1 (%2)").arg(name, source));
  if (reference.kind == QStringLiteral("face")) {
    parts << Text::tr("its point slides on the face"); // mitcad#1549
  } else {
    parts << (reference.mode == QStringLiteral("project") ? Text::tr("projected onto the sketch plane")
                                                          : Text::tr("included in 3D"));
  }
  if (reference.status == QStringLiteral("failed")) {
    parts << Text::tr("failed: %1").arg(userText(reference.message));
  } else if (reference.status == QStringLiteral("unknown")) {
    parts << Text::tr("not evaluated");
  }
  const QString users = entityNames(model, reference.users);
  if (!users.isEmpty()) {
    parts << Text::tr("used by %1").arg(users);
  }
  return parts.join(QStringLiteral("; "));
}

QStringList detachPoints(const Sketch3dModel& model, const QString& record) {
  const Sketch3dReference* reference = model.reference(record);
  QStringList out;
  if (reference == nullptr) {
    return out;
  }
  for (const QString& user : reference->users) {
    if (model.point(user) != nullptr) {
      if (!out.contains(user)) {
        out << user;
      }
      continue;
    }
    // A relation with the reference curve: its point.
    if (const Sketch3dItem* relation = model.item(user)) {
      for (const QString& id : relation->entities) {
        if (model.point(id) != nullptr && !out.contains(id)) {
          out << id;
        }
      }
    }
  }
  // The attachment's point when the record lists no users.
  if (out.isEmpty() && reference->kind == QStringLiteral("point")) {
    if (const Sketch3dItem* attachment = model.item(record)) {
      for (const QString& id : attachment->entities) {
        if (model.point(id) != nullptr) {
          out << id;
        }
      }
    }
  }
  return out;
}

} // namespace mitcad::sketch
