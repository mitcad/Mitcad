// SPDX-License-Identifier: MIT
#include "SectionPreview.hpp"

#include <algorithm>
#include <cmath>

#include <QJsonArray>

#include <BOPTools_AlgoTools3D.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepGProp.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <GeomAbs_SurfaceType.hxx>
#include <Geom_Surface.hxx>
#include <IntTools_Context.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad {
namespace {

// A JSON array of three numbers as a point; none for another value.
bool xyzOf(const QJsonValue& value, gp_XYZ& xyz) {
  const QJsonArray array = value.toArray();
  if (array.size() != 3) {
    return false;
  }
  xyz.SetCoord(array.at(0).toDouble(), array.at(1).toDouble(), array.at(2).toDouble());
  return true;
}

// The faces of `shape` (in its coordinates) that lie in the plane through
// `origin` with the normal (design coordinates), the shape placed by
// `placement`.
std::vector<TopoDS_Face> facesInPlane(const TopoDS_Shape& shape, const TopLoc_Location& placement,
                                      const gp_Pnt& origin, const gp_Dir& normal) {
  std::vector<TopoDS_Face> found;
  for (TopExp_Explorer faces(shape, TopAbs_FACE); faces.More(); faces.Next()) {
    const TopoDS_Face& face = TopoDS::Face(faces.Current());
    TopLoc_Location location;
    if (BRep_Tool::Surface(face, location).IsNull()) {
      continue; // a mesh body's triangles; a clipped one's caps are planar faces
    }
    const BRepAdaptor_Surface surface(face);
    if (surface.GetType() != GeomAbs_Plane) {
      continue;
    }
    const gp_Pln pln = surface.Plane();
    const gp_Pnt at = pln.Location().Transformed(placement.Transformation());
    gp_Dir axis = pln.Axis().Direction();
    axis.Transform(placement.Transformation());
    if (std::abs(axis.Dot(normal)) > 1.0 - 1e-9 && std::abs(gp_Vec(origin, at).Dot(gp_Vec(normal))) < 1e-6) {
      found.push_back(face);
    }
  }
  return found;
}

} // namespace

QString previewBodyKey(const std::string& uid, const std::string& occurrence) {
  return QString::fromStdString(uid) + QLatin1Char('@') + QString::fromStdString(occurrence);
}

QJsonObject previewClipTarget(const QString& uid, bool preview, const TopLoc_Location& placement) {
  const gp_Trsf trsf = placement.Transformation();
  QJsonArray linear;
  QJsonArray translation;
  for (int row = 1; row <= 3; ++row) {
    QJsonArray values;
    for (int column = 1; column <= 3; ++column) {
      values.append(trsf.Value(row, column));
    }
    linear.append(values);
    translation.append(trsf.Value(row, 4));
  }
  QJsonObject target{{QStringLiteral("body"), uid},
                     {QStringLiteral("transform"),
                      QJsonObject{{QStringLiteral("linear"), linear}, {QStringLiteral("translation"), translation}}}};
  if (preview) {
    target.insert(QStringLiteral("preview"), true);
  }
  return target;
}

SectionedShape sectionedShape(const std::shared_ptr<geometry::Shape>& drawn,
                              const std::shared_ptr<geometry::Shape>& uncut,
                              const std::shared_ptr<geometry::Shape>& left) {
  SectionedShape shown;
  shown.shape = drawn;
  if (!left || (uncut && left->occt().IsSame(uncut->occt()))) {
    return shown; // failed, or whole
  }
  shown.shape = left;
  shown.uncut = uncut;
  shown.sectioned = true;
  return shown;
}

bool cutPlaneOf(const QJsonValue& plane, gp_Pnt& origin, gp_Dir& normal) {
  if (!plane.isObject()) {
    return false;
  }
  gp_XYZ at;
  gp_XYZ towards;
  if (!xyzOf(plane.toObject().value(QStringLiteral("origin")), at) ||
      !xyzOf(plane.toObject().value(QStringLiteral("normal")), towards) || towards.Modulus() < 1e-12) {
    return false;
  }
  origin = gp_Pnt(at);
  normal = gp_Dir(towards);
  return true;
}

std::vector<TopoDS_Face> sectionCapFaces(const TopoDS_Shape& cut, const TopoDS_Shape& uncut,
                                         const TopLoc_Location& placement, const gp_Pnt& origin,
                                         const gp_Dir& normal) {
  std::vector<TopoDS_Face> caps;
  if (cut.IsNull()) {
    return caps;
  }
  const std::vector<TopoDS_Face> own =
      uncut.IsNull() ? std::vector<TopoDS_Face>() : facesInPlane(uncut, placement, origin, normal);
  const occ::handle<IntTools_Context> context = new IntTools_Context();
  for (const TopoDS_Face& face : facesInPlane(cut, placement, origin, normal)) {
    gp_Pnt inside;
    gp_Pnt2d uv;
    const bool original = !own.empty() && BOPTools_AlgoTools3D::PointInFace(face, inside, uv, context) == 0 &&
                          std::any_of(own.begin(), own.end(), [&](const TopoDS_Face& mine) {
                            return context->IsValidPointForFace(inside, mine, 1e-6);
                          });
    if (!original) {
      caps.push_back(face);
    }
  }
  return caps;
}

double facesArea(const std::vector<TopoDS_Face>& faces) {
  double area = 0.0;
  for (const TopoDS_Face& face : faces) {
    GProp_GProps properties;
    BRepGProp::SurfaceProperties(face, properties);
    area += properties.Mass();
  }
  return area;
}

} // namespace mitcad
