// SPDX-License-Identifier: MIT
#include "mitcad/geometry/primitive.hpp"

#include <stdexcept>

#include <BRepAdaptor_Surface.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCone.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <BRepPrimAPI_MakeTorus.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Ax2.hxx>

#include "face_select.hpp"
#include "history.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

// The role of a face by its outward normal in the frame: bottom and top
// along the normal, the box's sides counter-clockwise from -y.
std::string role(const TopoDS_Face& face, const Frame& frame) {
  const BRepAdaptor_Surface surface(face);
  if (surface.GetType() != GeomAbs_Plane) {
    return "side0";
  }
  const gp_Dir normal = detail::planar_normal(face, surface.Plane());
  const double z = normal.Dot(frame.normal());
  const double x = normal.Dot(frame.x_axis);
  const double y = normal.Dot(frame.y_axis);
  if (z < -0.5) {
    return "bottom";
  }
  if (z > 0.5) {
    return "top";
  }
  if (y < -0.5) {
    return "side0";
  }
  if (x > 0.5) {
    return "side1";
  }
  if (y > 0.5) {
    return "side2";
  }
  return "side3";
}

} // namespace

ShapePtr primitive(const PrimitiveSpec& spec) {
  if (spec.kind == PrimitiveKind::Cone) {
    // A radius may be zero, the apex there (mitcad#2239).
    detail::require_finite("the radius", spec.a);
    detail::require_finite("the top radius", spec.b);
    if (spec.a < 0.0 || spec.b < 0.0) {
      throw std::invalid_argument("a cone's radius must not be negative");
    }
    if (spec.a <= 0.0 && spec.b <= 0.0) {
      throw std::invalid_argument("a cone needs a radius greater than zero at its base or its top");
    }
    if (spec.a == spec.b) {
      throw std::invalid_argument("a cone of equal radii is a cylinder");
    }
  } else {
    detail::require_positive("the size", spec.a);
  }
  if (spec.kind == PrimitiveKind::Box || spec.kind == PrimitiveKind::Torus) {
    detail::require_positive("the size", spec.b);
  }
  if (spec.kind == PrimitiveKind::Box || spec.kind == PrimitiveKind::Cylinder || spec.kind == PrimitiveKind::Cone) {
    detail::require_positive("the height", spec.c);
  }
  if (spec.kind == PrimitiveKind::Torus && !(spec.b < spec.a)) {
    throw std::invalid_argument("the torus section is too large for its ring");
  }
  return detail::run("primitive", [&] {
    const gp_Ax2 axes(spec.frame.origin, spec.frame.normal(), spec.frame.x_axis);
    TopoDS_Shape solid;
    switch (spec.kind) {
    case PrimitiveKind::Box:
      solid = BRepPrimAPI_MakeBox(axes, spec.a, spec.b, spec.c).Shape();
      break;
    case PrimitiveKind::Cylinder:
      solid = BRepPrimAPI_MakeCylinder(axes, spec.a, spec.c).Shape();
      break;
    case PrimitiveKind::Sphere:
      solid = BRepPrimAPI_MakeSphere(axes, spec.a).Shape();
      break;
    case PrimitiveKind::Torus:
      solid = BRepPrimAPI_MakeTorus(axes, spec.a, spec.b).Shape();
      break;
    case PrimitiveKind::Cone:
      // The base disc (none for an apex) is `bottom`, the top disc `top`
      // and the conical face `side0`, by role() as a cylinder's.
      solid = BRepPrimAPI_MakeCone(axes, spec.a, spec.b, spec.c).Shape();
      break;
    }
    detail::FaceNamer namer(solid);
    for (TopExp_Explorer it(solid, TopAbs_FACE); it.More(); it.Next()) {
      const TopoDS_Face& face = TopoDS::Face(it.Current());
      namer.add(face, face_name(spec.feature, role(face, spec.frame)));
    }
    namer.finish();
    detail::require_valid(namer.result(), "the primitive");
    return namer.shape();
  });
}

} // namespace mitcad::geometry
