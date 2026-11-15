// SPDX-License-Identifier: MIT
// A sketch on a cylindrical or conical face (mitcad#1768): the face's map
// from the sketch's developed coordinates, the view's pictures on it and
// the plane Create Sketch sends for such a face (SketchSurface.hpp).
#include "SketchSurface.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <exception>
#include <utility>

#include <QJsonArray>
#include <QJsonDocument>

#include <BRepMesh_IncrementalMesh.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Poly_Triangulation.hxx>
#include <Standard_Failure.hxx>
#include <TopAbs_Orientation.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_XYZ.hxx>

#include "../framework/Selection.hpp"

namespace mitcad::sketch {
namespace {

constexpr double kSurfacePi = 3.14159265358979323846;

std::optional<gp_XYZ> xyzOf(const QJsonValue& value) {
  const QJsonArray a = value.toArray();
  if (a.size() < 3) {
    return std::nullopt;
  }
  return gp_XYZ(a.at(0).toDouble(), a.at(1).toDouble(), a.at(2).toDouble());
}

// The axis counted towards +Z, else +Y, else +X (the model's `counted`,
// core/model/src/features/emboss.rs): the sketch's y axis on the face.
gp_XYZ counted(const gp_XYZ& direction) {
  const double z = direction.Z();
  const double y = direction.Y();
  bool flip = false;
  if (std::abs(z) > 1e-9) {
    flip = z < 0.0;
  } else if (std::abs(y) > 1e-9) {
    flip = y < 0.0;
  } else {
    flip = direction.X() < 0.0;
  }
  return flip ? direction.Reversed() : direction;
}

// The reference direction across `n` (the model's datum::default_x_axis):
// the X axis made square to it, else the Y axis.
gp_XYZ defaultXAxis(const gp_XYZ& n) {
  const auto project = [&n](const gp_XYZ& axis) { return axis - n * n.Dot(axis); };
  gp_XYZ across = std::abs(n.Dot(gp_XYZ(1.0, 0.0, 0.0))) < 1.0 - 1e-3 ? project(gp_XYZ(1.0, 0.0, 0.0))
                                                                         : project(gp_XYZ(0.0, 1.0, 0.0));
  const double length = across.Modulus();
  return length > 1e-12 ? across / length : gp_XYZ(1.0, 0.0, 0.0);
}

} // namespace

std::optional<SketchSurface> SketchSurface::fromJson(const QJsonValue& value) {
  if (!value.isObject()) {
    return std::nullopt;
  }
  const QJsonObject surface = value.toObject();
  const QString kindName = surface.value(QStringLiteral("kind")).toString();
  SketchSurface result;
  if (kindName == QStringLiteral("cylinder")) {
    result.m_kind = Kind::Cylinder;
  } else if (kindName == QStringLiteral("cone")) {
    result.m_kind = Kind::Cone;
  } else {
    return std::nullopt; // a plane
  }
  const QJsonObject axisJson = surface.value(QStringLiteral("axis")).toObject();
  const QJsonObject frameJson = surface.value(QStringLiteral("frame")).toObject();
  const std::optional<gp_XYZ> origin = xyzOf(axisJson.value(QStringLiteral("origin")));
  const std::optional<gp_XYZ> direction = xyzOf(axisJson.value(QStringLiteral("direction")));
  const std::optional<gp_XYZ> frameOrigin = xyzOf(frameJson.value(QStringLiteral("origin")));
  const std::optional<gp_XYZ> xAxis = xyzOf(frameJson.value(QStringLiteral("x_axis")));
  const std::optional<gp_XYZ> yAxis = xyzOf(frameJson.value(QStringLiteral("y_axis")));
  if (!origin || !direction || !frameOrigin || !xAxis || !yAxis) {
    return std::nullopt;
  }
  try {
    result.m_origin = gp_Pnt(*origin);
    result.m_axis = gp_Dir(*direction);
    result.m_frame.origin = gp_Pnt(*frameOrigin);
    result.m_frame.x_axis = gp_Dir(*xAxis);
    result.m_frame.y_axis = gp_Dir(*yAxis);
  } catch (const std::exception&) {
    return std::nullopt; // a direction of no length (OCCT's Standard_ConstructionError)
  }
  result.m_radius = surface.value(QStringLiteral("radius")).toDouble();
  result.m_halfAngle = surface.value(QStringLiteral("half_angle")).toDouble();
  result.m_internal = surface.value(QStringLiteral("internal")).toBool();
  if (result.m_kind == Kind::Cone && !(result.m_halfAngle > 0.0 && result.m_halfAngle < kSurfacePi / 2.0)) {
    return std::nullopt;
  }
  if (result.m_kind == Kind::Cylinder && !(result.m_radius > 0.0)) {
    return std::nullopt;
  }
  result.m_signature = QString::fromUtf8(QJsonDocument(surface).toJson(QJsonDocument::Compact));
  result.prepare();
  return result;
}

void SketchSurface::prepare() {
  const gp_XYZ a = m_axis.XYZ();
  const gp_XYZ toOrigin = m_frame.origin.XYZ() - m_origin.XYZ();
  m_centre = m_kind == Kind::Cylinder ? m_origin : gp_Pnt(m_origin.XYZ() + a * toOrigin.Dot(a));
  gp_XYZ n = m_frame.normal().XYZ();
  if ((m_frame.origin.XYZ() - m_centre.XYZ()).Dot(n) < 0.0) {
    n.Reverse();
  }
  gp_XYZ x = n - a * a.Dot(n);
  if (x.Modulus() < 1e-12) {
    x = n;
  }
  m_x = gp_Dir(x);
  m_y = gp_Dir(a.Crossed(m_x.XYZ()));
  if (m_kind == Kind::Cone) {
    const double height = toOrigin.Dot(a);
    m_rho0 = height / std::cos(m_halfAngle);
    m_radius = height * std::tan(m_halfAngle);
  } else {
    m_rho0 = 0.0;
  }
}

void SketchSurface::transform(const gp_Trsf& placement) {
  m_origin.Transform(placement);
  m_axis.Transform(placement);
  m_frame.origin.Transform(placement);
  m_frame.x_axis.Transform(placement);
  m_frame.y_axis.Transform(placement);
  prepare();
}

gp_Pnt SketchSurface::point(const V2& p) const {
  const gp_XYZ a = m_axis.XYZ();
  const gp_XYZ d = m_frame.point(gp_Pnt2d(p.x, p.y)).XYZ() - m_centre.XYZ();
  const double s = d.Dot(m_y.XYZ());
  const double t = d.Dot(a);
  if (m_kind == Kind::Cylinder) {
    const double angle = s / m_radius;
    const gp_XYZ around = m_x.XYZ() * std::cos(angle) + m_y.XYZ() * std::sin(angle);
    return gp_Pnt(m_origin.XYZ() + a * t + around * m_radius);
  }
  const double along = t + m_rho0;
  const double rho = std::hypot(s, along);
  const double theta = std::atan2(s, along) / std::sin(m_halfAngle);
  const gp_XYZ around = m_x.XYZ() * std::cos(theta) + m_y.XYZ() * std::sin(theta);
  const gp_XYZ generator = a * std::cos(m_halfAngle) + around * std::sin(m_halfAngle);
  return gp_Pnt(m_origin.XYZ() + generator * rho);
}

gp_Dir SketchSurface::outward(const V2& p) const {
  const gp_XYZ a = m_axis.XYZ();
  const gp_XYZ d = m_frame.point(gp_Pnt2d(p.x, p.y)).XYZ() - m_centre.XYZ();
  const double s = d.Dot(m_y.XYZ());
  const double t = d.Dot(a);
  gp_XYZ normal;
  if (m_kind == Kind::Cylinder) {
    const double angle = s / m_radius;
    normal = m_x.XYZ() * std::cos(angle) + m_y.XYZ() * std::sin(angle);
  } else {
    const double theta = std::atan2(s, t + m_rho0) / std::sin(m_halfAngle);
    const gp_XYZ around = m_x.XYZ() * std::cos(theta) + m_y.XYZ() * std::sin(theta);
    normal = around * std::cos(m_halfAngle) - a * std::sin(m_halfAngle);
  }
  if (normal.Modulus() < 1e-12) {
    return m_frame.normal();
  }
  return m_internal ? gp_Dir(normal.Reversed()) : gp_Dir(normal);
}

double SketchSurface::step() const { return std::max(0.05, 0.05 * m_radius); }

QString SketchSurface::describe() const {
  QString text = QStringLiteral("%1, radius %2 mm")
                     .arg(m_kind == Kind::Cylinder ? QStringLiteral("cylinder") : QStringLiteral("cone"))
                     .arg(m_radius, 0, 'f', 3);
  if (m_kind == Kind::Cone) {
    text += QStringLiteral(", half angle %1 deg").arg(m_halfAngle * 180.0 / kSurfacePi, 0, 'f', 3);
  }
  if (m_internal) {
    text += QStringLiteral(", inside");
  }
  return text;
}

std::vector<V2> densified(const std::vector<V2>& points, double step, bool closed) {
  std::vector<V2> result;
  if (points.empty()) {
    return result;
  }
  step = std::max(step, 1e-3);
  result.push_back(points.front());
  const std::size_t pieces = closed ? points.size() : points.size() - 1;
  for (std::size_t i = 0; i < pieces; ++i) {
    const V2& from = points[i];
    const V2& to = points[(i + 1) % points.size()];
    const int count = std::clamp(static_cast<int>(std::ceil(norm(to - from) / step)), 1, 2048);
    for (int k = 1; k <= count; ++k) {
      result.push_back(from + (to - from) * (static_cast<double>(k) / count));
    }
  }
  return result;
}

TopoDS_Shape wrappedFace(const TopoDS_Shape& flat, const SketchSurface& surface) {
  if (flat.IsNull()) {
    return TopoDS_Shape();
  }
  const double step = surface.step();
  const geometry::Frame& frame = surface.frame();
  try {
    // The flat face's own triangles first (it is the view's own shape,
    // built for the view alone), then each cut into pieces no longer than
    // the step, laid onto the face point by point.
    BRepMesh_IncrementalMesh(flat, std::min(0.05, step / 4.0), false, 0.2);
    BRep_Builder builder;
    TopoDS_Compound compound;
    builder.MakeCompound(compound);
    bool any = false;
    for (TopExp_Explorer faces(flat, TopAbs_FACE); faces.More(); faces.Next()) {
      const TopoDS_Face& face = TopoDS::Face(faces.Current());
      TopLoc_Location location;
      const occ::handle<Poly_Triangulation> source = BRep_Tool::Triangulation(face, location);
      if (source.IsNull() || source->NbTriangles() == 0) {
        continue;
      }
      const gp_Trsf toModel = location.Transformation();
      const bool reversed = face.Orientation() == TopAbs_REVERSED;
      // One count of pieces per side for every triangle of the face, so that
      // the triangles still meet along their sides.
      double longest = 0.0;
      for (int t = 1; t <= source->NbTriangles(); ++t) {
        int n[3] = {0, 0, 0};
        source->Triangle(t).Get(n[0], n[1], n[2]);
        for (int i = 0; i < 3; ++i) {
          longest = std::max(longest, source->Node(n[i]).Distance(source->Node(n[(i + 1) % 3])));
        }
      }
      int pieces = std::clamp(static_cast<int>(std::ceil(longest / step)), 1, 48);
      // At most about 200000 triangles for a face.
      while (pieces > 1 && static_cast<long long>(source->NbTriangles()) * pieces * pieces > 200000) {
        --pieces;
      }
      const int nodesPer = (pieces + 1) * (pieces + 2) / 2;
      const int trianglesPer = pieces * pieces;
      const occ::handle<Poly_Triangulation> wrapped =
          new Poly_Triangulation(source->NbTriangles() * nodesPer, source->NbTriangles() * trianglesPer, false);
      int node = 0;
      int triangle = 0;
      for (int t = 1; t <= source->NbTriangles(); ++t) {
        int n[3] = {0, 0, 0};
        source->Triangle(t).Get(n[0], n[1], n[2]);
        if (reversed) {
          std::swap(n[1], n[2]);
        }
        const gp_XYZ a = source->Node(n[0]).Transformed(toModel).XYZ();
        const gp_XYZ b = source->Node(n[1]).Transformed(toModel).XYZ();
        const gp_XYZ c = source->Node(n[2]).Transformed(toModel).XYZ();
        // The node of row i (towards b) and column j (towards c).
        const int first = node + 1;
        const auto index = [first, pieces](int i, int j) {
          // Rows before i hold (pieces + 1) + pieces + ... nodes.
          return first + i * (pieces + 1) - i * (i - 1) / 2 + j;
        };
        for (int i = 0; i <= pieces; ++i) {
          for (int j = 0; i + j <= pieces; ++j) {
            const gp_XYZ at =
                a + (b - a) * (static_cast<double>(i) / pieces) + (c - a) * (static_cast<double>(j) / pieces);
            // Its sketch point: where it lies in the carrier plane.
            const gp_XYZ off = at - frame.origin.XYZ();
            const V2 uv{off.Dot(frame.x_axis.XYZ()), off.Dot(frame.y_axis.XYZ())};
            wrapped->SetNode(++node, surface.point(uv));
          }
        }
        for (int i = 0; i < pieces; ++i) {
          for (int j = 0; i + j < pieces; ++j) {
            wrapped->SetTriangle(++triangle, Poly_Triangle(index(i, j), index(i + 1, j), index(i, j + 1)));
            if (i + j + 1 < pieces) {
              wrapped->SetTriangle(++triangle,
                                   Poly_Triangle(index(i + 1, j), index(i + 1, j + 1), index(i, j + 1)));
            }
          }
        }
      }
      TopoDS_Face onFace;
      builder.MakeFace(onFace, wrapped);
      builder.Add(compound, onFace);
      any = true;
    }
    return any ? TopoDS_Shape(compound) : TopoDS_Shape();
  } catch (const std::exception&) {
    return TopoDS_Shape(); // OCCT's Standard_Failure too
  }
}

bool isDevelopedFace(const SelectionItem& item) {
  return item.kind == SelectKind::Face &&
         (item.geometry == QStringLiteral("cylinder") || item.geometry == QStringLiteral("cone"));
}

double developedAngle(const gp_Dir& axis, const gp_Pnt& axisPoint, const gp_Pnt& at) {
  const gp_XYZ up = counted(axis.XYZ());
  const gp_XYZ across = defaultXAxis(up);
  const gp_XYZ d = at.XYZ() - axisPoint.XYZ();
  const gp_XYZ radial = d - up * up.Dot(d);
  if (radial.Modulus() < 1e-9) {
    return 0.0;
  }
  return std::atan2(up.Dot(across.Crossed(radial)), across.Dot(radial));
}

QJsonObject developedPlane(const SelectionItem& item, const std::function<QJsonValue(const QJsonObject&)>& query) {
  double angle = 0.0;
  if (item.at && query) {
    try {
      const QJsonValue answer = query({{QStringLiteral("query"), QStringLiteral("faces")},
                                       {QStringLiteral("body"), item.owner},
                                       {QStringLiteral("geometry"), true},
                                       {QStringLiteral("surface"), item.geometry}});
      const QJsonArray faces =
          answer.isObject() ? answer.toObject().value(QStringLiteral("faces")).toArray() : answer.toArray();
      for (const QJsonValue& value : faces) {
        const QJsonObject face = value.toObject();
        bool named = false;
        for (const QJsonValue& name : face.value(QStringLiteral("names")).toArray()) {
          named = named || name.toString() == item.name;
        }
        const std::optional<gp_XYZ> axis = xyzOf(face.value(QStringLiteral("axis")));
        const std::optional<gp_XYZ> origin = xyzOf(face.value(QStringLiteral("origin")));
        if (!named || !axis || !origin || axis->Modulus() < 1e-12) {
          continue;
        }
        const std::array<double, 3>& at = *item.at;
        angle = developedAngle(gp_Dir(*axis), gp_Pnt(*origin), gp_Pnt(at[0], at[1], at[2]));
        break;
      }
    } catch (const std::exception&) {
      angle = 0.0; // the origin at the reference direction instead (OCCT's failures too)
    }
  }
  return QJsonObject{{QStringLiteral("face"), item.name},
                     {QStringLiteral("body"), item.owner},
                     {QStringLiteral("developed"), QJsonObject{{QStringLiteral("angle"), angle}}}};
}

} // namespace mitcad::sketch
