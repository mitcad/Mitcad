// SPDX-License-Identifier: MIT
#include "mitcad/geometry/tool.hpp"

#include <algorithm>
#include <cmath>
#include <set>
#include <stdexcept>
#include <utility>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GeomAbs_SurfaceType.hxx>
#include <Geom_Surface.hxx>
#include <Precision.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "face_select.hpp"

namespace mitcad::geometry {

Tool Tool::of_plane(const gp_Pln& plane) {
  Tool tool;
  tool.kind = Kind::Plane;
  tool.plane = plane;
  return tool;
}

Tool Tool::of_face(ShapePtr shape, std::string face) {
  Tool tool;
  tool.kind = Kind::Face;
  tool.shape = std::move(shape);
  tool.face = std::move(face);
  return tool;
}

Tool Tool::of_body(ShapePtr shape) {
  Tool tool;
  tool.kind = Kind::Body;
  tool.shape = std::move(shape);
  return tool;
}

namespace detail {
namespace {

constexpr double kTwoPi = 6.28318530717958647692;

// The size and corners of a box.
double box_size(const Bnd_Box& box) {
  if (box.IsVoid()) {
    return 100.0;
  }
  return std::max(1.0, box.CornerMin().Distance(box.CornerMax()));
}

std::vector<gp_Pnt> box_corners(const Bnd_Box& box) {
  std::vector<gp_Pnt> corners;
  if (box.IsVoid()) {
    return corners;
  }
  const gp_Pnt low = box.CornerMin();
  const gp_Pnt high = box.CornerMax();
  for (int i = 0; i < 8; ++i) {
    corners.emplace_back((i & 1) ? high.X() : low.X(), (i & 2) ? high.Y() : low.Y(),
                         (i & 4) ? high.Z() : low.Z());
  }
  return corners;
}

// The face's surface grown until it passes the box, where the surface type
// allows; other faces stay as they are.
TopoDS_Face extended_face(const TopoDS_Face& face, const Bnd_Box& around) {
  if (const auto plane = face_plane(face)) {
    return plane_face(*plane, around);
  }
  const BRepAdaptor_Surface adaptor(face, false);
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face);
  double v_low = 0.0;
  double v_high = 0.0;
  switch (adaptor.GetType()) {
  case GeomAbs_Cylinder:
  case GeomAbs_Cone: {
    const gp_Ax1 axis = adaptor.GetType() == GeomAbs_Cylinder ? adaptor.Cylinder().Axis()
                                                                : adaptor.Cone().Axis();
    const double scale =
        adaptor.GetType() == GeomAbs_Cone ? 1.0 / std::cos(adaptor.Cone().SemiAngle()) : 1.0;
    v_low = adaptor.FirstVParameter();
    v_high = adaptor.LastVParameter();
    for (const gp_Pnt& corner : box_corners(around)) {
      const double along = gp_Vec(axis.Location(), corner).Dot(gp_Vec(axis.Direction())) * scale;
      v_low = std::min(v_low, along);
      v_high = std::max(v_high, along);
    }
    const double margin = box_size(around) * 0.1;
    BRepBuilderAPI_MakeFace maker(surface, 0.0, kTwoPi, v_low - margin, v_high + margin,
                                  Precision::Confusion());
    if (maker.IsDone()) {
      return maker.Face();
    }
    break;
  }
  case GeomAbs_Sphere: {
    BRepBuilderAPI_MakeFace maker(surface, 0.0, kTwoPi, -kTwoPi / 4, kTwoPi / 4,
                                  Precision::Confusion());
    if (maker.IsDone()) {
      return maker.Face();
    }
    break;
  }
  default:
    break;
  }
  return face;
}

// The curve's id in a segment name: "c3[c2,c4]#1" -> "c3".
std::string curve_of(const std::string& segment) {
  return segment.substr(0, segment.find_first_of("[#"));
}

} // namespace

gp_Pln tool_plane(const Tool& tool, const char* what) {
  switch (tool.kind) {
  case Tool::Kind::Plane:
    return tool.plane;
  case Tool::Kind::Face: {
    if (!tool.shape) {
      throw std::invalid_argument(std::string(what) + " has no body");
    }
    const std::vector<int> faces = resolve_faces(*tool.shape, {tool.face}, what);
    const auto plane = face_plane(tool.shape->face(faces.front()));
    if (!plane) {
      throw std::invalid_argument(std::string("unsupported: ") + what + " " + tool.face +
                                  " is not planar");
    }
    return *plane;
  }
  case Tool::Kind::Body:
  case Tool::Kind::Curves:
    break;
  }
  throw std::invalid_argument(std::string(what) + " must be a plane or a planar face");
}

Cutter make_cutter(const Tool& tool, const Bnd_Box& around, bool extend) {
  Cutter cutter;
  BRep_Builder builder;
  TopoDS_Compound compound;
  builder.MakeCompound(compound);
  const auto add = [&](const TopoDS_Shape& face, std::string key) {
    builder.Add(compound, face);
    cutter.faces.push_back(face);
    cutter.keys.push_back(std::move(key));
  };
  switch (tool.kind) {
  case Tool::Kind::Plane:
    add(plane_face(tool.plane, around), {});
    cutter.normal = tool.plane.Axis().Direction();
    break;
  case Tool::Kind::Face: {
    if (!tool.shape) {
      throw std::invalid_argument("the splitting face has no body");
    }
    for (int f : resolve_faces(*tool.shape, {tool.face}, "splitting faces")) {
      const TopoDS_Face& face = tool.shape->face(f);
      if (!cutter.normal) {
        if (const auto plane = face_plane(face)) {
          cutter.normal = plane->Axis().Direction();
        }
      }
      add(extend ? extended_face(face, around) : face, primary_name(*tool.shape, f));
    }
    break;
  }
  case Tool::Kind::Body: {
    if (!tool.shape) {
      throw std::invalid_argument("the splitting body has no shape");
    }
    for (int f = 0; f < tool.shape->face_count(); ++f) {
      cutter.faces.push_back(tool.shape->face(f));
      cutter.keys.push_back(primary_name(*tool.shape, f));
    }
    cutter.shape = tool.shape->occt();
    return cutter;
  }
  case Tool::Kind::Curves: {
    const std::set<std::string> wanted(tool.curves.begin(), tool.curves.end());
    std::set<std::string> done;
    const double length = box_size(around) * 2.0;
    gp_Trsf back;
    back.SetTranslation(gp_Vec(tool.direction) * -length);
    for (const Region& region : tool.regions) {
      for (const auto& [edge, segment] : make_profile(tool.frame, region).edges) {
        if ((!wanted.empty() && wanted.count(curve_of(segment)) == 0) || !done.insert(segment).second) {
          continue;
        }
        const TopoDS_Shape start = BRepBuilderAPI_Transform(edge, back, true).Shape();
        BRepPrimAPI_MakePrism prism(start, gp_Vec(tool.direction) * (2.0 * length));
        if (!prism.IsDone()) {
          throw std::runtime_error("the curve " + segment + " could not be swept");
        }
        for (TopExp_Explorer it(prism.Shape(), TopAbs_FACE); it.More(); it.Next()) {
          add(it.Current(), segment);
        }
      }
    }
    if (cutter.faces.empty()) {
      throw std::invalid_argument("the sketch has none of the splitting curves");
    }
    break;
  }
  }
  cutter.shape = compound;
  return cutter;
}

} // namespace detail
} // namespace mitcad::geometry
