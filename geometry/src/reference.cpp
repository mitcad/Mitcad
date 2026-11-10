// SPDX-License-Identifier: MIT
#include "mitcad/geometry/reference.hpp"

#include <algorithm>
#include <cmath>
#include <limits>
#include <optional>
#include <stdexcept>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepGProp.hxx>
#include <BRepTools.hxx>
#include <GProp_GProps.hxx>
#include <Precision.hxx>
#include <TopoDS.hxx>
#include <gp_Cone.hxx>
#include <gp_Cylinder.hxx>

#include "face_select.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

std::vector<TopoDS_Face> faces_named(const Shape& shape, const std::string& face) {
  std::vector<TopoDS_Face> faces;
  for (const int i : shape.find_faces(face)) {
    faces.push_back(shape.face(i));
  }
  if (faces.empty()) {
    throw std::invalid_argument(no_face_error(face));
  }
  return faces;
}

// The face's normal at the middle of its parameter range, out of the material.
gp_Dir outward_normal(const TopoDS_Face& face, gp_Pnt& point) {
  const BRepAdaptor_Surface surface(face);
  double u0 = 0, u1 = 0, v0 = 0, v1 = 0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  gp_Vec du;
  gp_Vec dv;
  surface.D1((u0 + u1) / 2.0, (v0 + v1) / 2.0, point, du, dv);
  gp_Dir normal(du.Crossed(dv));
  if (face.Orientation() == TopAbs_REVERSED) {
    normal.Reverse();
  }
  return normal;
}

} // namespace

gp_Pln face_plane(const Shape& shape, const std::string& face) {
  return detail::run("face plane", [&] {
    const std::vector<TopoDS_Face> faces = faces_named(shape, face);
    // A plane, or a flat spline face (a sheet body's wall, mitcad#1554).
    const std::optional<gp_Pln> found = detail::reference_plane(faces.front());
    if (!found) {
      throw std::invalid_argument(face_error(face, "the face is not planar", "face " + face + " is not planar"));
    }
    const gp_Dir normal = found->Axis().Direction();
    GProp_GProps props;
    for (const TopoDS_Face& piece : faces) {
      GProp_GProps own;
      BRepGProp::SurfaceProperties(piece, own);
      props.Add(own);
    }
    // The centre of mass of the pieces, put on the plane.
    const gp_Pln& plane = *found;
    const gp_Pnt centre = props.CentreOfMass();
    const double height = gp_Vec(plane.Location(), centre).Dot(gp_Vec(plane.Axis().Direction()));
    return gp_Pln(centre.Translated(gp_Vec(plane.Axis().Direction()) * -height), normal);
  });
}

CylinderFace face_cylinder(const Shape& shape, const std::string& face) {
  return detail::run("face cylinder", [&] {
    const std::vector<TopoDS_Face> faces = faces_named(shape, face);
    const BRepAdaptor_Surface surface(faces.front());
    if (surface.GetType() != GeomAbs_Cylinder) {
      throw std::invalid_argument(face_error(face, "the face is not cylindrical", "face " + face + " is not cylindrical"));
    }
    const gp_Cylinder cylinder = surface.Cylinder();
    const gp_Ax1 axis = cylinder.Axis();
    double low = std::numeric_limits<double>::infinity();
    double high = -std::numeric_limits<double>::infinity();
    for (const TopoDS_Face& piece : faces) {
      const BRepAdaptor_Surface own(piece);
      if (own.GetType() != GeomAbs_Cylinder ||
          !own.Cylinder().Axis().IsCoaxial(axis, 1.0e-9, Precision::Confusion()) ||
          std::abs(own.Cylinder().Radius() - cylinder.Radius()) > Precision::Confusion()) {
        throw std::invalid_argument(
            face_error(face, "the pieces of the face are not one cylinder", "the pieces of face " + face + " are not one cylinder"));
      }
      double u0 = 0, u1 = 0, v0 = 0, v1 = 0;
      BRepTools::UVBounds(piece, u0, u1, v0, v1);
      // Points at both ends, measured along the axis.
      for (const double v : {v0, v1}) {
        const gp_Pnt p = own.Value(u0, v);
        const double t = gp_Vec(axis.Location(), p).Dot(gp_Vec(axis.Direction()));
        low = std::min(low, t);
        high = std::max(high, t);
      }
    }
    CylinderFace result;
    result.axis = gp_Ax1(axis.Location().Translated(gp_Vec(axis.Direction()) * low), axis.Direction());
    result.radius = cylinder.Radius();
    result.length = high - low;
    gp_Pnt point;
    const gp_Dir normal = outward_normal(faces.front(), point);
    const gp_Vec radial(gp_Pnt(axis.Location().XYZ() +
                               axis.Direction().XYZ() *
                                   gp_Vec(axis.Location(), point).Dot(gp_Vec(axis.Direction()))),
                        point);
    result.internal = radial.Dot(gp_Vec(normal)) < 0.0;
    return result;
  });
}

ConeFace face_cone(const Shape& shape, const std::string& face) {
  return detail::run("face cone", [&] {
    const std::vector<TopoDS_Face> faces = faces_named(shape, face);
    const BRepAdaptor_Surface surface(faces.front());
    if (surface.GetType() != GeomAbs_Cone) {
      throw std::invalid_argument(face_error(face, "the face is not conical", "face " + face + " is not conical"));
    }
    const gp_Cone cone = surface.Cone();
    for (const TopoDS_Face& piece : faces) {
      const BRepAdaptor_Surface own(piece);
      if (own.GetType() != GeomAbs_Cone || own.Cone().Apex().Distance(cone.Apex()) > Precision::Confusion() ||
          !own.Cone().Axis().IsParallel(cone.Axis(), 1.0e-9)) {
        throw std::invalid_argument(
            face_error(face, "the pieces of the face are not one cone", "the pieces of face " + face + " are not one cone"));
      }
    }
    ConeFace result;
    result.apex = cone.Apex();
    // OCCT's semi-angle is negative when the radius shrinks along the axis.
    result.axis = cone.SemiAngle() < 0.0 ? cone.Axis().Direction().Reversed() : cone.Axis().Direction();
    result.half_angle = std::abs(cone.SemiAngle());
    gp_Pnt point;
    const gp_Dir normal = outward_normal(faces.front(), point);
    const gp_Vec along(result.axis);
    const gp_Vec from(result.apex, point);
    const gp_Vec radial = from - along * from.Dot(along);
    result.internal = radial.Dot(gp_Vec(normal)) < 0.0;
    return result;
  });
}

} // namespace mitcad::geometry
