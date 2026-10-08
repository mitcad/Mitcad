// SPDX-License-Identifier: MIT
#include "ModelShapes.hpp"

#include <algorithm>
#include <cmath>
#include <map>

#include <QJsonArray>
#include <QSet>

#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRep_Builder.hxx>
#include <Geom_BSplineCurve.hxx>
#include <NCollection_Array1.hxx>
#include <TopoDS_Compound.hxx>
#include <Standard_Failure.hxx>
#include <gp_Circ.hxx>
#include <gp_Elips.hxx>
#include <gp_Pln.hxx>

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

} // namespace

gp_Pnt pointOf(const QJsonValue& xyz) {
  const QJsonArray a = xyz.toArray();
  return gp_Pnt(a.at(0).toDouble(), a.at(1).toDouble(), a.at(2).toDouble());
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

} // namespace mitcad
