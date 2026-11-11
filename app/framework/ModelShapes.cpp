// SPDX-License-Identifier: MIT
#include "ModelShapes.hpp"

#include <algorithm>
#include <cmath>
#include <map>

#include <QHash>
#include <QJsonArray>
#include <QSet>
#include <QStringList>

#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRep_Builder.hxx>
#include <ElCLib.hxx>
#include <Geom_BSplineCurve.hxx>
#include <NCollection_Array1.hxx>
#include <TopoDS_Compound.hxx>
#include <Standard_Failure.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Elips.hxx>
#include <gp_Pln.hxx>
#include <gp_XYZ.hxx>

#include "Json.hpp"

namespace mitcad {
namespace {

constexpr double kTiny = 1e-9;
constexpr double kTwoPi = 6.283185307179586;

QString text(const QJsonObject& object, const char* key) {
  return object.value(QLatin1String(key)).toString();
}

double number(const QJsonObject& object, const char* key) {
  return object.value(QLatin1String(key)).toDouble();
}

gp_Pnt2d point2(const QJsonValue& xy) {
  const QJsonArray a = xy.toArray();
  return gp_Pnt2d(a.at(0).toDouble(), a.at(1).toDouble());
}

// A curve of the sketch in model space; null when it cannot be built.
TopoDS_Shape curveShape(const QString& type, const QJsonObject& g, const geometry::Frame& frame) {
  const gp_Dir normal = frame.normal();
  const auto axes = [&](const gp_Pnt2d& center, double rotation) {
    const gp_Dir x(frame.x_axis.XYZ() * std::cos(rotation) +
                   frame.y_axis.XYZ() * std::sin(rotation));
    return gp_Ax2(frame.point(center), normal, x);
  };
  // Angles run counter-clockwise from start to end.
  const auto span = [](double start, double end) {
    while (end <= start) {
      end += kTwoPi;
    }
    return end;
  };
  if (type == QStringLiteral("line")) {
    const gp_Pnt a = frame.point(point2(g.value(QStringLiteral("start"))));
    const gp_Pnt b = frame.point(point2(g.value(QStringLiteral("end"))));
    return a.Distance(b) > kTiny ? TopoDS_Shape(BRepBuilderAPI_MakeEdge(a, b).Edge())
                                 : TopoDS_Shape();
  }
  if (type == QStringLiteral("circle") || type == QStringLiteral("arc")) {
    const double radius = number(g, "radius");
    if (radius <= kTiny) {
      return TopoDS_Shape();
    }
    const gp_Circ circle(axes(point2(g.value(QStringLiteral("center"))), 0.0), radius);
    if (type == QStringLiteral("circle")) {
      return BRepBuilderAPI_MakeEdge(circle).Edge();
    }
    const double start = number(g, "start_angle");
    return BRepBuilderAPI_MakeEdge(circle, start, span(start, number(g, "end_angle"))).Edge();
  }
  if (type == QStringLiteral("ellipse") || type == QStringLiteral("elliptical_arc")) {
    const double major = number(g, "major_radius");
    const double minor = number(g, "minor_radius");
    if (minor <= kTiny || major < minor) {
      return TopoDS_Shape();
    }
    const gp_Elips ellipse(axes(point2(g.value(QStringLiteral("center"))), number(g, "rotation")),
                           major, minor);
    if (type == QStringLiteral("ellipse")) {
      return BRepBuilderAPI_MakeEdge(ellipse).Edge();
    }
    const double start = number(g, "start_angle");
    return BRepBuilderAPI_MakeEdge(ellipse, start, span(start, number(g, "end_angle"))).Edge();
  }
  if (g.contains(QStringLiteral("control"))) {
    // A NURBS: knots as a full vector (clamped uniform when left out).
    const QJsonArray control = g.value(QStringLiteral("control")).toArray();
    const int degree = g.value(QStringLiteral("degree")).toInt(3);
    const int count = static_cast<int>(control.size());
    if (degree < 1 || count < degree + 1) {
      return TopoDS_Shape();
    }
    NCollection_Array1<gp_Pnt> poles(1, count);
    NCollection_Array1<double> weights(1, count);
    const QJsonArray weightValues = g.value(QStringLiteral("weights")).toArray();
    for (int i = 0; i < count; ++i) {
      poles.SetValue(i + 1, frame.point(point2(control.at(i))));
      weights.SetValue(i + 1, weightValues.size() == count ? weightValues.at(i).toDouble() : 1.0);
    }
    std::vector<double> flat;
    for (const QJsonValue& k : g.value(QStringLiteral("knots")).toArray()) {
      flat.push_back(k.toDouble());
    }
    if (static_cast<int>(flat.size()) != count + degree + 1) {
      flat.assign(static_cast<std::size_t>(degree + 1), 0.0);
      const int inner = count - degree - 1;
      for (int i = 1; i <= inner; ++i) {
        flat.push_back(static_cast<double>(i) / (inner + 1));
      }
      flat.insert(flat.end(), static_cast<std::size_t>(degree + 1), 1.0);
    }
    std::vector<double> knots;
    std::vector<int> multiplicities;
    for (const double k : flat) {
      if (!knots.empty() && std::abs(k - knots.back()) < kTiny) {
        ++multiplicities.back();
      } else {
        knots.push_back(k);
        multiplicities.push_back(1);
      }
    }
    NCollection_Array1<double> knotArray(1, static_cast<int>(knots.size()));
    NCollection_Array1<int> multArray(1, static_cast<int>(knots.size()));
    for (std::size_t i = 0; i < knots.size(); ++i) {
      knotArray.SetValue(static_cast<int>(i) + 1, knots[i]);
      multArray.SetValue(static_cast<int>(i) + 1, multiplicities[i]);
    }
    const occ::handle<Geom_BSplineCurve> curve =
        new Geom_BSplineCurve(poles, weights, knotArray, multArray, degree);
    return BRepBuilderAPI_MakeEdge(curve).Edge();
  }
  return TopoDS_Shape();
}

// A 3D sketch's frame in its component's coordinates (mitcad#1524): local
// x and y along its support's axes, z along its normal.
struct SpatialFrame {
  gp_XYZ origin;
  gp_XYZ x;
  gp_XYZ y;
  gp_XYZ n;
  gp_Pnt point(const gp_XYZ& local) const {
    return gp_Pnt(origin + x * local.X() + y * local.Y() + n * local.Z());
  }
  gp_XYZ vector(const gp_XYZ& local) const { return x * local.X() + y * local.Y() + n * local.Z(); }
};

std::optional<gp_XYZ> xyzFrom(const QJsonValue& value) {
  const QJsonArray a = value.toArray();
  if (a.size() < 3) {
    return std::nullopt;
  }
  return gp_XYZ(a.at(0).toDouble(), a.at(1).toDouble(), a.at(2).toDouble());
}

bool spatialFrame(const QJsonObject& sketch, SpatialFrame& frame) {
  const QJsonObject f = sketch.value(QStringLiteral("frame")).toObject();
  const auto origin = xyzFrom(f.value(QStringLiteral("origin")));
  const auto x = xyzFrom(f.value(QStringLiteral("x_axis")));
  const auto y = xyzFrom(f.value(QStringLiteral("y_axis")));
  const auto n = xyzFrom(f.value(QStringLiteral("normal")));
  if (!origin || !x || !y || !n) {
    return false;
  }
  frame = SpatialFrame{*origin, *x, *y, *n};
  return true;
}

TopoDS_Shape polylineThrough(const std::vector<gp_Pnt>& points) {
  if (points.size() < 2) {
    return TopoDS_Shape();
  }
  BRepBuilderAPI_MakePolygon polygon;
  for (const gp_Pnt& point : points) {
    polygon.Add(point);
  }
  return polygon.IsDone() ? TopoDS_Shape(polygon.Wire()) : TopoDS_Shape();
}

// A B-spline by local poles, weights (empty: 1) and its full knot vector
// (clamped uniform when it does not fit); null when it cannot be built.
TopoDS_Shape spatialSpline(const SpatialFrame& frame, const QJsonArray& control, const QJsonArray& weightValues,
                           const QJsonArray& knotValues, int degree) {
  const int count = static_cast<int>(control.size());
  if (degree < 1 || count < degree + 1) {
    return TopoDS_Shape();
  }
  NCollection_Array1<gp_Pnt> poles(1, count);
  NCollection_Array1<double> weights(1, count);
  for (int i = 0; i < count; ++i) {
    const auto local = xyzFrom(control.at(i));
    if (!local) {
      return TopoDS_Shape();
    }
    poles.SetValue(i + 1, frame.point(*local));
    weights.SetValue(i + 1, weightValues.size() == count ? weightValues.at(i).toDouble() : 1.0);
  }
  std::vector<double> flat;
  for (const QJsonValue& k : knotValues) {
    flat.push_back(k.toDouble());
  }
  if (static_cast<int>(flat.size()) != count + degree + 1) {
    flat.assign(static_cast<std::size_t>(degree + 1), 0.0);
    const int inner = count - degree - 1;
    for (int i = 1; i <= inner; ++i) {
      flat.push_back(static_cast<double>(i) / (inner + 1));
    }
    flat.insert(flat.end(), static_cast<std::size_t>(degree + 1), 1.0);
  }
  std::vector<double> knots;
  std::vector<int> multiplicities;
  for (const double k : flat) {
    if (!knots.empty() && std::abs(k - knots.back()) < kTiny) {
      ++multiplicities.back();
    } else {
      knots.push_back(k);
      multiplicities.push_back(1);
    }
  }
  NCollection_Array1<double> knotArray(1, static_cast<int>(knots.size()));
  NCollection_Array1<int> multArray(1, static_cast<int>(knots.size()));
  for (std::size_t i = 0; i < knots.size(); ++i) {
    knotArray.SetValue(static_cast<int>(i) + 1, knots[i]);
    multArray.SetValue(static_cast<int>(i) + 1, multiplicities[i]);
  }
  const occ::handle<Geom_BSplineCurve> curve = new Geom_BSplineCurve(poles, weights, knotArray, multArray, degree);
  return BRepBuilderAPI_MakeEdge(curve).Edge();
}

// An ellipse about `normal` with the radius `rx` along `x` and `ry` across
// it: gp_Elips wants the larger as its major radius, so a taller one turns
// a quarter (its parameters then run a quarter behind: `shift`).
gp_Elips spatialEllipse(const gp_Pnt& center, const gp_Dir& normal, const gp_Dir& x, double rx, double ry,
                        double& shift) {
  if (rx >= ry) {
    shift = 0.0;
    return gp_Elips(gp_Ax2(center, normal, x), rx, ry);
  }
  shift = -kTwoPi / 4.0;
  return gp_Elips(gp_Ax2(center, normal, normal.Crossed(x)), ry, rx);
}

// A reference curve's geometry (the `sketch3d` query's `geometry` of a
// `reference` curve) in the component's coordinates.
TopoDS_Shape spatialReference(const SpatialFrame& frame, const QJsonObject& g) {
  const QString kind = text(g, "kind");
  if (kind == QStringLiteral("line")) {
    const auto start = xyzFrom(g.value(QStringLiteral("start")));
    const auto end = xyzFrom(g.value(QStringLiteral("end")));
    if (!start || !end || (*end - *start).Modulus() <= kTiny) {
      return TopoDS_Shape();
    }
    return BRepBuilderAPI_MakeEdge(frame.point(*start), frame.point(*end)).Edge();
  }
  if (kind == QStringLiteral("conic")) {
    const auto center = xyzFrom(g.value(QStringLiteral("center")));
    const auto normal = xyzFrom(g.value(QStringLiteral("normal")));
    const auto x = xyzFrom(g.value(QStringLiteral("x_axis")));
    const double major = number(g, "major");
    const double minor = number(g, "minor");
    if (!center || !normal || !x || major <= kTiny || minor <= kTiny) {
      return TopoDS_Shape();
    }
    const gp_Pnt c = frame.point(*center);
    const gp_Dir n(frame.vector(*normal));
    const gp_Dir along(frame.vector(*x));
    const bool closed = g.value(QStringLiteral("closed")).toBool();
    const double start = number(g, "start");
    const double end = number(g, "end");
    if (std::abs(major - minor) <= kTiny) {
      const gp_Circ circle(gp_Ax2(c, n, along), major);
      return closed ? BRepBuilderAPI_MakeEdge(circle).Edge() : BRepBuilderAPI_MakeEdge(circle, start, end).Edge();
    }
    double shift = 0.0;
    const gp_Elips ellipse = spatialEllipse(c, n, along, major, minor, shift);
    return closed ? BRepBuilderAPI_MakeEdge(ellipse).Edge()
                  : BRepBuilderAPI_MakeEdge(ellipse, start + shift, end + shift).Edge();
  }
  if (kind == QStringLiteral("bspline")) {
    return spatialSpline(frame, g.value(QStringLiteral("poles")).toArray(),
                         g.value(QStringLiteral("weights")).toArray(), g.value(QStringLiteral("knots")).toArray(),
                         g.value(QStringLiteral("degree")).toInt(3));
  }
  if (kind == QStringLiteral("helix")) {
    // Drawn through points: 64 a turn.
    const auto center = xyzFrom(g.value(QStringLiteral("center")));
    const auto axis = xyzFrom(g.value(QStringLiteral("axis")));
    const auto x = xyzFrom(g.value(QStringLiteral("x_axis")));
    const double radius = number(g, "radius");
    const double pitch = number(g, "pitch");
    const double start = number(g, "start");
    const double end = number(g, "end");
    if (!center || !axis || !x || radius <= kTiny || !(end > start)) {
      return TopoDS_Shape();
    }
    const gp_XYZ y = axis->Crossed(*x);
    const int steps = std::clamp(static_cast<int>(std::ceil((end - start) / kTwoPi * 64.0)), 8, 4096);
    std::vector<gp_Pnt> points;
    for (int i = 0; i <= steps; ++i) {
      const double t = start + (end - start) * i / steps;
      const gp_XYZ local =
          *center + *x * (radius * std::cos(t)) + y * (radius * std::sin(t)) + *axis * (pitch * t / kTwoPi);
      points.push_back(frame.point(local));
    }
    return polylineThrough(points);
  }
  return TopoDS_Shape();
}

} // namespace

gp_Pnt pointOf(const QJsonValue& xyz) {
  const QJsonArray a = xyz.toArray();
  return gp_Pnt(a.at(0).toDouble(), a.at(1).toDouble(), a.at(2).toDouble());
}

std::optional<gp_Pnt> pointFrom(const QJsonValue& xyz) {
  const QJsonArray a = xyz.toArray();
  if (a.size() < 3) {
    return std::nullopt;
  }
  return gp_Pnt(a.at(0).toDouble(), a.at(1).toDouble(), a.at(2).toDouble());
}

TopoDS_Shape polylineShape(const QJsonArray& points) {
  if (points.size() < 2) {
    return TopoDS_Shape();
  }
  BRepBuilderAPI_MakePolygon polygon;
  for (const QJsonValue& point : points) {
    polygon.Add(pointOf(point));
  }
  return polygon.IsDone() ? TopoDS_Shape(polygon.Wire()) : TopoDS_Shape();
}

gp_Vec vectorOf(const QJsonValue& xyz) {
  const QJsonArray a = xyz.toArray();
  return gp_Vec(a.at(0).toDouble(), a.at(1).toDouble(), a.at(2).toDouble());
}

gp_Trsf trsfOf(const QJsonArray& rows) {
  const auto at = [&rows](int r, int c) { return rows[r].toArray()[c].toDouble(); };
  gp_Trsf trsf;
  if (rows.size() >= 3) {
    trsf.SetValues(at(0, 0), at(0, 1), at(0, 2), at(0, 3), at(1, 0), at(1, 1), at(1, 2), at(1, 3),
                   at(2, 0), at(2, 1), at(2, 2), at(2, 3));
  }
  return trsf;
}

bool isIdentity(const QJsonArray& rows) {
  for (int r = 0; r < std::min(3, static_cast<int>(rows.size())); ++r) {
    for (int c = 0; c < 4; ++c) {
      if (std::abs(rows[r].toArray()[c].toDouble() - (r == c ? 1.0 : 0.0)) > 1e-12) {
        return false;
      }
    }
  }
  return true;
}

QJsonObject placedDatum(const QJsonObject& datum, const gp_Trsf& placement) {
  QJsonObject placed = datum;
  const auto array = [](const gp_XYZ& xyz) {
    QJsonArray out;
    out.append(xyz.X());
    out.append(xyz.Y());
    out.append(xyz.Z());
    return out;
  };
  for (const char* key : {"origin", "point"}) {
    const QJsonValue value = datum.value(QLatin1String(key));
    if (const auto p = pointFrom(value)) {
      placed.insert(QLatin1String(key), array(p->Transformed(placement).XYZ()));
    }
  }
  for (const char* key : {"normal", "x_axis", "y_axis", "direction"}) {
    const QJsonValue value = datum.value(QLatin1String(key));
    if (value.toArray().size() >= 3) {
      placed.insert(QLatin1String(key), array(vectorOf(value).Transformed(placement).XYZ()));
    }
  }
  return placed;
}

QString datumText(const QJsonObject& datum) {
  const auto xyz = [&datum](const char* key) {
    const QJsonArray v = datum.value(QLatin1String(key)).toArray();
    QStringList parts;
    for (const QJsonValue& c : v) {
      parts << QString::number(std::abs(c.toDouble()) < 1e-9 ? 0.0 : c.toDouble(), 'g', 6);
    }
    return QStringLiteral("(%1)").arg(parts.join(QStringLiteral(", ")));
  };
  const QString kind = text(datum, "type");
  if (kind == QStringLiteral("plane")) {
    return QStringLiteral("origin %1, normal %2").arg(xyz("origin"), xyz("normal"));
  }
  if (kind == QStringLiteral("axis")) {
    return QStringLiteral("origin %1, direction %2").arg(xyz("origin"), xyz("direction"));
  }
  return QStringLiteral("point %1").arg(xyz("point"));
}

QJsonObject fixedPlaneDatum(const QJsonObject& plane) {
  const auto origin = pointFrom(plane.value(QStringLiteral("origin")));
  const auto normalAt = pointFrom(plane.value(QStringLiteral("normal")));
  if (!origin || !normalAt) {
    return QJsonObject();
  }
  gp_Vec normal(normalAt->XYZ());
  if (normal.Magnitude() < 1e-12) {
    return QJsonObject();
  }
  normal.Normalize();
  // As core/model-base's DatumPlane::from_normal and default_x_axis.
  gp_Vec x(0.0, 0.0, 0.0);
  if (const auto hint = pointFrom(plane.value(QStringLiteral("x_axis")))) {
    const gp_Vec along(hint->XYZ());
    x = along - normal * normal.Dot(along);
  }
  if (x.Magnitude() < 1e-12) {
    const gp_Vec axis = std::abs(normal.X()) < 1.0 - 1e-3 ? gp_Vec(1.0, 0.0, 0.0) : gp_Vec(0.0, 1.0, 0.0);
    x = axis - normal * normal.Dot(axis);
  }
  if (x.Magnitude() < 1e-12) {
    return QJsonObject();
  }
  x.Normalize();
  const gp_Vec y = normal.Crossed(x);
  const auto array = [](const gp_XYZ& xyz) { return QJsonArray{xyz.X(), xyz.Y(), xyz.Z()}; };
  return {{QStringLiteral("type"), QStringLiteral("plane")},
          {QStringLiteral("origin"), array(origin->XYZ())},
          {QStringLiteral("normal"), array(normal.XYZ())},
          {QStringLiteral("x_axis"), array(x.XYZ())},
          {QStringLiteral("y_axis"), array(y.XYZ())}};
}

TopoDS_Shape datumShape(const QJsonObject& datum, double size) {
  try {
    const QString type = text(datum, "type");
    if (type == QStringLiteral("plane")) {
      const gp_Ax3 frame(pointOf(datum.value(QStringLiteral("origin"))),
                         gp_Dir(vectorOf(datum.value(QStringLiteral("normal")))),
                         gp_Dir(vectorOf(datum.value(QStringLiteral("x_axis")))));
      const double half = size / 2.0;
      return BRepBuilderAPI_MakeFace(gp_Pln(frame), -half, half, -half, half).Face();
    }
    if (type == QStringLiteral("axis")) {
      const gp_Pnt origin = pointOf(datum.value(QStringLiteral("origin")));
      const gp_Vec direction =
          gp_Vec(gp_Dir(vectorOf(datum.value(QStringLiteral("direction"))))) * (size / 2.0);
      return BRepBuilderAPI_MakeEdge(origin.Translated(-direction), origin.Translated(direction))
          .Edge();
    }
    if (type == QStringLiteral("point")) {
      return BRepBuilderAPI_MakeVertex(pointOf(datum.value(QStringLiteral("point")))).Vertex();
    }
  } catch (const Standard_Failure&) {
    // A degenerate datum (zero vectors) is not drawn.
  }
  return TopoDS_Shape();
}

bool sketchFrame(const QJsonObject& sketch, geometry::Frame& frame) {
  const QJsonObject f = sketch.value(QStringLiteral("frame")).toObject();
  if (f.isEmpty()) {
    return false;
  }
  try {
    frame.origin = pointOf(f.value(QStringLiteral("origin")));
    frame.x_axis = gp_Dir(vectorOf(f.value(QStringLiteral("x_axis"))));
    frame.y_axis = gp_Dir(vectorOf(f.value(QStringLiteral("y_axis"))));
  } catch (const Standard_Failure&) {
    return false;
  }
  return true;
}

gp_Ax3 toAx3(const geometry::Frame& frame) {
  return gp_Ax3(frame.origin, frame.normal(), frame.x_axis);
}

std::vector<SketchEntityDisplay> sketchEntities(const QJsonObject& sketch) {
  std::vector<SketchEntityDisplay> result;
  geometry::Frame frame;
  if (!sketchFrame(sketch, frame)) {
    return result;
  }
  const QString uid = text(sketch, "uid");
  // The frame the entities are placed with, for their signatures.
  const QString placed = QString::fromUtf8(compactJson(sketch.value(QStringLiteral("frame")).toObject()));
  // The ends of the curves: of a derived offset's spline (P4) only they are
  // shown, its other control points are the offset's, not handles to drag.
  QSet<QString> ends;
  for (const QJsonValue& value : sketch.value(QStringLiteral("entities")).toArray()) {
    const QJsonObject entity = value.toObject();
    for (const char* field : {"start", "end", "center"}) {
      ends << entity.value(QLatin1String(field)).toString();
    }
    for (const char* field : {"control", "points"}) {
      const QJsonArray ids = entity.value(QLatin1String(field)).toArray();
      if (!ids.isEmpty()) {
        ends << ids.first().toString() << ids.last().toString();
      }
    }
  }
  for (const QJsonValue& value : sketch.value(QStringLiteral("entities")).toArray()) {
    const QJsonObject entity = value.toObject();
    // The line that stands for a sketch axis and its far end (mitcad#500):
    // sketch mode draws the axis across the view instead.
    if (entity.contains(QStringLiteral("axis"))) {
      continue;
    }
    SketchEntityDisplay display;
    display.sketch = uid;
    display.id = text(entity, "id");
    display.signature = QString::fromUtf8(compactJson(entity)) + placed;
    display.construction = entity.value(QStringLiteral("construction")).toBool();
    display.centerline = entity.value(QStringLiteral("centerline")).toBool();
    display.reference = entity.value(QStringLiteral("reference")).toBool();
    display.constrained = entity.value(QStringLiteral("fully_constrained")).toBool();
    const QString type = text(entity, "type");
    try {
      if (type == QStringLiteral("point")) {
        if (!entity.contains(QStringLiteral("at")) ||
            (entity.value(QStringLiteral("derived")).toBool() && !ends.contains(display.id))) {
          continue;
        }
        display.geometry = QStringLiteral("point");
        display.shape =
            BRepBuilderAPI_MakeVertex(frame.point(point2(entity.value(QStringLiteral("at")))))
                .Vertex();
      } else {
        const QJsonObject geometry = entity.value(QStringLiteral("geometry")).toObject();
        if (geometry.isEmpty()) {
          continue;
        }
        display.geometry = type;
        display.shape = curveShape(type, geometry, frame);
      }
    } catch (const Standard_Failure&) {
      continue;
    }
    if (!display.shape.IsNull()) {
      result.push_back(display);
    }
  }
  // Texts as their glyph outlines (P3), one shape per text: a click picks
  // the text.
  for (const QJsonValue& value : sketch.value(QStringLiteral("texts")).toArray()) {
    const QJsonObject t = value.toObject();
    const QJsonArray outline = t.value(QStringLiteral("outline")).toArray();
    if (outline.isEmpty()) {
      continue;
    }
    SketchEntityDisplay display;
    display.sketch = uid;
    display.id = text(t, "id");
    display.geometry = QStringLiteral("text");
    display.signature = QString::fromUtf8(compactJson(t)) + placed;
    TopoDS_Compound compound;
    BRep_Builder builder;
    builder.MakeCompound(compound);
    bool any = false;
    for (const QJsonValue& contour : outline) {
      for (const QJsonValue& piece : contour.toObject().value(QStringLiteral("curves")).toArray()) {
        const QJsonObject g = piece.toObject();
        try {
          const TopoDS_Shape edge =
              curveShape(g.contains(QStringLiteral("control")) ? QStringLiteral("spline") : QStringLiteral("line"),
                         g, frame);
          if (!edge.IsNull()) {
            builder.Add(compound, edge);
            any = true;
          }
        } catch (const Standard_Failure&) {
        }
      }
    }
    if (any) {
      display.shape = compound;
      result.push_back(display);
    }
  }
  return result;
}

std::vector<SketchEntityDisplay> spatialSketchEntities(const QJsonObject& sketch) {
  std::vector<SketchEntityDisplay> result;
  SpatialFrame frame;
  if (!spatialFrame(sketch, frame)) {
    return result;
  }
  const QString uid = text(sketch, "uid");
  const QString placed = QString::fromUtf8(compactJson(sketch.value(QStringLiteral("frame")).toObject()));
  // The points where the last solve left them, in local coordinates.
  QHash<QString, gp_XYZ> local;
  for (const QJsonValue& value : sketch.value(QStringLiteral("points")).toArray()) {
    const QJsonObject point = value.toObject();
    if (const auto at = xyzFrom(point.value(QStringLiteral("at")))) {
      local.insert(text(point, "id"), *at);
    }
  }
  const auto pointAt = [&local, &frame](const QJsonValue& id) -> std::optional<gp_Pnt> {
    const auto found = local.constFind(id.toString());
    if (found == local.cend()) {
      return std::nullopt;
    }
    return frame.point(*found);
  };
  for (const QJsonValue& value : sketch.value(QStringLiteral("points")).toArray()) {
    const QJsonObject point = value.toObject();
    const auto at = pointAt(point.value(QStringLiteral("id")));
    if (!at) {
      continue;
    }
    SketchEntityDisplay display;
    display.sketch = uid;
    display.id = text(point, "id");
    display.geometry = QStringLiteral("point");
    display.constrained = point.value(QStringLiteral("determined")).toBool();
    display.signature = QString::fromUtf8(compactJson(point)) + placed;
    display.shape = BRepBuilderAPI_MakeVertex(*at).Vertex();
    result.push_back(display);
  }
  const gp_Dir sketchNormal(frame.n);
  for (const QJsonValue& value : sketch.value(QStringLiteral("curves")).toArray()) {
    const QJsonObject curve = value.toObject();
    const QString type = text(curve, "type");
    const QJsonObject g = curve.value(QStringLiteral("geometry")).toObject();
    SketchEntityDisplay display;
    display.sketch = uid;
    display.id = text(curve, "id");
    display.geometry = type;
    display.construction = curve.value(QStringLiteral("construction")).toBool();
    display.reference = type == QStringLiteral("reference");
    display.constrained = curve.value(QStringLiteral("determined")).toBool();
    display.signature = QString::fromUtf8(compactJson(curve)) + placed;
    try {
      if (type == QStringLiteral("line")) {
        const auto start = xyzFrom(g.value(QStringLiteral("start")));
        const auto end = xyzFrom(g.value(QStringLiteral("end")));
        if (start && end && (*end - *start).Modulus() > kTiny) {
          display.shape = BRepBuilderAPI_MakeEdge(frame.point(*start), frame.point(*end)).Edge();
        }
      } else if (type == QStringLiteral("arc") || type == QStringLiteral("circle")) {
        const auto center = pointAt(curve.value(QStringLiteral("center")));
        const auto normal = xyzFrom(g.value(QStringLiteral("normal")));
        const double radius = number(g, "radius");
        if (center && normal && radius > kTiny) {
          const gp_Dir n(frame.vector(*normal));
          if (type == QStringLiteral("circle")) {
            display.shape = BRepBuilderAPI_MakeEdge(gp_Circ(gp_Ax2(*center, n), radius)).Edge();
          } else if (const auto start = pointAt(curve.value(QStringLiteral("start")))) {
            // Counter-clockwise about its normal from its start point.
            const double sweep = number(g, "sweep");
            const gp_Vec toStart(*center, *start);
            if (toStart.Magnitude() > kTiny && sweep > kTiny) {
              const gp_Circ circle(gp_Ax2(*center, n, gp_Dir(toStart)), radius);
              display.shape = BRepBuilderAPI_MakeEdge(circle, 0.0, sweep).Edge();
            }
          }
        }
      } else if (type == QStringLiteral("spline") || type == QStringLiteral("fitted_spline")) {
        display.shape = spatialSpline(frame, g.value(QStringLiteral("control_points")).toArray(),
                                      g.value(QStringLiteral("weights")).toArray(),
                                      g.value(QStringLiteral("knots")).toArray(),
                                      g.value(QStringLiteral("degree")).toInt(3));
      } else if (type == QStringLiteral("ellipse") || type == QStringLiteral("elliptical_arc")) {
        // Plane-bound: in the plane through its centre square to its
        // frame axis (`normal`, the sketch plane's Z by default,
        // mitcad#1550), its major radius to its major point, its solved
        // minor radius.
        const auto center = pointAt(curve.value(QStringLiteral("center")));
        const auto major = pointAt(curve.value(QStringLiteral("major")));
        const double minor = g.contains(QStringLiteral("minor")) ? number(g, "minor") : number(curve, "minor_radius");
        const QString axis = text(curve, "normal");
        const gp_Dir planeNormal =
            axis == QStringLiteral("x")   ? gp_Dir(frame.vector(gp_XYZ(1.0, 0.0, 0.0)))
            : axis == QStringLiteral("y") ? gp_Dir(frame.vector(gp_XYZ(0.0, 1.0, 0.0)))
                                          : sketchNormal;
        const gp_Vec toMajor = center && major ? gp_Vec(*center, *major) : gp_Vec();
        if (center && major && toMajor.Magnitude() > kTiny && minor > kTiny) {
          double shift = 0.0;
          const gp_Elips ellipse =
              spatialEllipse(*center, planeNormal, gp_Dir(toMajor), toMajor.Magnitude(), minor, shift);
          if (type == QStringLiteral("ellipse")) {
            display.shape = BRepBuilderAPI_MakeEdge(ellipse).Edge();
          } else {
            const auto start = pointAt(curve.value(QStringLiteral("start")));
            const auto end = pointAt(curve.value(QStringLiteral("end")));
            if (start && end) {
              const double from = ElCLib::Parameter(ellipse, *start);
              double to = ElCLib::Parameter(ellipse, *end);
              while (to <= from) {
                to += kTwoPi;
              }
              display.shape = BRepBuilderAPI_MakeEdge(ellipse, from, to).Edge();
            }
          }
        }
      } else if (display.reference) {
        display.shape = spatialReference(frame, g);
      }
    } catch (const Standard_Failure&) {
      continue;
    }
    if (!display.shape.IsNull()) {
      result.push_back(display);
    }
  }
  // Texts (mitcad#1550): their outlines in their planes, as the query lays
  // them out (local polylines).
  for (const QJsonValue& value : sketch.value(QStringLiteral("texts")).toArray()) {
    const QJsonObject t = value.toObject();
    BRep_Builder builder;
    TopoDS_Compound outline;
    builder.MakeCompound(outline);
    bool any = false;
    for (const QJsonValue& contour : t.value(QStringLiteral("outline")).toArray()) {
      std::vector<gp_Pnt> points;
      for (const QJsonValue& p : contour.toArray()) {
        if (const auto at = xyzFrom(p)) {
          points.push_back(frame.point(*at));
        }
      }
      const TopoDS_Shape wire = polylineThrough(points);
      if (!wire.IsNull()) {
        builder.Add(outline, wire);
        any = true;
      }
    }
    if (!any) {
      continue;
    }
    SketchEntityDisplay display;
    display.sketch = uid;
    display.id = text(t, "id");
    display.geometry = QStringLiteral("text");
    display.signature = QString::fromUtf8(compactJson(t)) + placed;
    display.shape = outline;
    result.push_back(display);
  }
  return result;
}

} // namespace mitcad
