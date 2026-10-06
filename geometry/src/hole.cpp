// SPDX-License-Identifier: MIT
#include "mitcad/geometry/hole.hpp"

#include <algorithm>
#include <cmath>
#include <limits>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GeomAPI_IntCS.hxx>
#include <Geom_Line.hxx>
#include <IntCurvesFace_ShapeIntersector.hxx>
#include <Precision.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Lin.hxx>

#include "history.hpp"
#include "sweep.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

constexpr double kPi = 3.14159265358979323846;
constexpr double kTwoPi = 2.0 * kPi;
// Hits closer than this to the start are the start face itself.
constexpr double kNear = 1.0e-6;

// Where the axis meets the target, measured from the hole's start.
double depth_to(const Target& target, const gp_Ax1& ray) {
  const gp_Lin line(ray);
  bool planar = target.kind == Target::Kind::Plane;
  if (target.kind == Target::Kind::Face && target.extend) {
    planar = BRepAdaptor_Surface(detail::target_faces(target).front()).GetType() == GeomAbs_Plane;
  }
  if (planar) {
    const gp_Pln plane = detail::target_plane(target, ray.Direction());
    const double slope = plane.Axis().Direction().Dot(ray.Direction());
    if (slope < 1.0e-9) {
      throw std::invalid_argument("the hole runs parallel to the target plane");
    }
    const double t = gp_Vec(ray.Location(), plane.Location()).Dot(gp_Vec(plane.Axis().Direction())) / slope;
    if (t <= kNear) {
      throw std::invalid_argument("the target lies behind the start of the hole");
    }
    return t;
  }
  std::vector<double> hits;
  if (target.kind == Target::Kind::Face && target.extend) {
    const occ::handle<Geom_Line> geometry = new Geom_Line(line);
    GeomAPI_IntCS intersection(geometry, BRep_Tool::Surface(detail::target_faces(target).front()));
    if (intersection.IsDone()) {
      for (int i = 1; i <= intersection.NbPoints(); ++i) {
        double u = 0, v = 0, w = 0;
        intersection.Parameters(i, u, v, w);
        hits.push_back(w);
      }
    }
  } else {
    TopoDS_Shape shape = target.body ? target.body->occt() : TopoDS_Shape();
    if (target.kind == Target::Kind::Face) {
      BRep_Builder builder;
      TopoDS_Compound faces;
      builder.MakeCompound(faces);
      for (const TopoDS_Face& face : detail::target_faces(target)) {
        builder.Add(faces, face);
      }
      shape = faces;
    }
    IntCurvesFace_ShapeIntersector intersection;
    intersection.Load(shape, Precision::Confusion());
    intersection.Perform(line, -1.0e100, 1.0e100);
    if (intersection.IsDone()) {
      for (int i = 1; i <= intersection.NbPnt(); ++i) {
        hits.push_back(intersection.WParameter(i));
      }
    }
  }
  double best = target.through ? -std::numeric_limits<double>::infinity()
                               : std::numeric_limits<double>::infinity();
  for (const double w : hits) {
    if (w > kNear) {
      best = target.through ? std::max(best, w) : std::min(best, w);
    }
  }
  if (!std::isfinite(best)) {
    throw std::invalid_argument("the hole does not reach its target");
  }
  return best + target.offset;
}

// How far the hole must run to pass every body.
double depth_through(const std::vector<ShapePtr>& bodies, const gp_Ax1& ray) {
  if (bodies.empty()) {
    throw std::invalid_argument("there is no body to drill through");
  }
  double farthest = -std::numeric_limits<double>::infinity();
  for (const ShapePtr& body : bodies) {
    const Bnd_Box box = detail::bounds_of(body->occt());
    if (box.IsVoid()) {
      continue;
    }
    double x0 = 0, y0 = 0, z0 = 0, x1 = 0, y1 = 0, z1 = 0;
    box.Get(x0, y0, z0, x1, y1, z1);
    for (int i = 0; i < 8; ++i) {
      const gp_Pnt corner(i & 1 ? x1 : x0, i & 2 ? y1 : y0, i & 4 ? z1 : z0);
      farthest = std::max(farthest, gp_Vec(ray.Location(), corner).Dot(gp_Vec(ray.Direction())));
    }
  }
  if (!(farthest > kNear)) {
    throw std::invalid_argument("the hole points away from every body");
  }
  return farthest + 1.0;
}

struct Corner {
  double r;
  double z;
  // The part the edge from this corner to the next sweeps; empty on the axis.
  const char* part;
};

// The half section of a hole in its axial plane, from the start centre
// round to the axis at the bottom.
std::vector<Corner> section(const HoleSpec& spec, double depth, bool drill_point) {
  const double r = spec.diameter / 2.0;
  if (!(std::abs(spec.taper) < kPi / 2.0)) {
    throw std::invalid_argument("the hole's taper must be less than 90 degrees either way");
  }
  // The wall's radius at a depth (a tapered wall leans in as it goes
  // deeper), and the depth where a cone from radius `from` at depth `at`,
  // narrowing by `slope` per depth, meets it.
  const double lean = std::tan(spec.taper);
  const auto wall = [&](double z) { return r - z * lean; };
  const auto meets = [&](double from, double at, double slope, const char* what) {
    if (!(slope > lean)) {
      throw std::invalid_argument(std::string("the ") + what + " cone does not meet the hole's wall");
    }
    return (from - r + at * slope) / (slope - lean);
  };
  const auto angle_of = [](double angle, const char* what) {
    if (!(angle > 0.0 && angle < kPi)) {
      throw std::invalid_argument(std::string("the ") + what + " angle must be between 0 and 180 degrees");
    }
    return std::tan(angle / 2.0);
  };
  std::vector<Corner> corners{{0.0, 0.0, "top"}};
  switch (spec.type) {
  case HoleType::Simple:
    corners.push_back({r, 0.0, "wall"});
    break;
  case HoleType::Counterbore:
    if (!(spec.counterbore_diameter > spec.diameter)) {
      throw std::invalid_argument("the counterbore must be wider than the hole");
    }
    detail::require_positive("counterbore depth", spec.counterbore_depth);
    if (!(spec.counterbore_depth < depth)) {
      throw std::invalid_argument("the counterbore must be shallower than the hole");
    }
    if (!(spec.counterbore_diameter / 2.0 > wall(spec.counterbore_depth))) {
      throw std::invalid_argument("the counterbore must be wider than the hole at its floor");
    }
    corners.push_back({spec.counterbore_diameter / 2.0, 0.0, "cbore_wall"});
    corners.push_back({spec.counterbore_diameter / 2.0, spec.counterbore_depth, "cbore_floor"});
    corners.push_back({wall(spec.counterbore_depth), spec.counterbore_depth, "wall"});
    break;
  case HoleType::Countersink: {
    if (!(spec.countersink_diameter > spec.diameter)) {
      throw std::invalid_argument("the countersink must be wider than the hole");
    }
    const double cone =
        meets(spec.countersink_diameter / 2.0, 0.0, angle_of(spec.countersink_angle, "countersink"),
              "countersink");
    if (!(cone < depth)) {
      throw std::invalid_argument("the countersink must be shallower than the hole");
    }
    corners.push_back({spec.countersink_diameter / 2.0, 0.0, "csink"});
    corners.push_back({wall(cone), cone, "wall"});
    break;
  }
  case HoleType::Counterdrill: {
    if (!(spec.counterbore_diameter > spec.diameter)) {
      throw std::invalid_argument("the counterdrill must be wider than the hole");
    }
    detail::require_positive("counterdrill depth", spec.counterbore_depth);
    const double cone = meets(spec.counterbore_diameter / 2.0, spec.counterbore_depth,
                              angle_of(spec.countersink_angle, "counterdrill"), "counterdrill");
    if (!(cone < depth) || !(cone > spec.counterbore_depth)) {
      throw std::invalid_argument("the counterdrill must be shallower than the hole");
    }
    corners.push_back({spec.counterbore_diameter / 2.0, 0.0, "cbore_wall"});
    corners.push_back({spec.counterbore_diameter / 2.0, spec.counterbore_depth, "cdrill"});
    corners.push_back({wall(cone), cone, "wall"});
    break;
  }
  }
  const double bottom = wall(depth);
  if (!(bottom > Precision::Confusion())) {
    throw std::invalid_argument("the taper closes the hole before its depth");
  }
  corners.push_back({bottom, depth, "tip"});
  if (drill_point && !spec.flat) {
    if (!(spec.tip_angle > 0.0 && spec.tip_angle < kPi)) {
      throw std::invalid_argument("the drill point angle must be between 0 and 180 degrees");
    }
    corners.push_back({0.0, depth + bottom / std::tan(spec.tip_angle / 2.0), ""});
  } else {
    corners.push_back({0.0, depth, ""});
  }
  return corners;
}

ShapePtr one_hole(const HoleSpec& spec, int index, const gp_Ax1& at, double depth, bool drill_point) {
  const gp_Dir along = at.Direction();
  const gp_Dir across = std::abs(along.X()) < 0.9 ? along.Crossed(gp_Dir(1, 0, 0)) : along.Crossed(gp_Dir(0, 1, 0));
  const auto point = [&](const Corner& c) {
    return at.Location().Translated(gp_Vec(across) * c.r + gp_Vec(along) * c.z);
  };
  const std::vector<Corner> corners = section(spec, depth, drill_point);
  BRepBuilderAPI_MakeWire wire;
  std::vector<std::pair<TopoDS_Edge, std::string>> named;
  for (std::size_t i = 0; i < corners.size(); ++i) {
    const Corner& from = corners[i];
    const Corner& to = corners[(i + 1) % corners.size()];
    BRepBuilderAPI_MakeEdge edge(point(from), point(to));
    if (!edge.IsDone()) {
      throw std::runtime_error("the hole's section could not be built");
    }
    wire.Add(edge.Edge());
    if (*from.part != '\0') {
      named.emplace_back(wire.Edge(),
                         face_name(spec.feature, "hole" + std::to_string(index) + "." + from.part));
    }
  }
  BRepBuilderAPI_MakeFace face(wire.Wire(), true);
  if (!face.IsDone()) {
    throw std::runtime_error("the hole's section could not be built");
  }
  BRepPrimAPI_MakeRevol revolution(face.Face(), gp_Ax1(at.Location(), along), kTwoPi);
  revolution.Build();
  if (!revolution.IsDone()) {
    throw std::runtime_error("the hole could not be built");
  }
  detail::FaceNamer namer(revolution.Shape());
  for (const auto& [edge, name] : named) {
    namer.generated(revolution, edge, name);
  }
  return detail::with_swept_names(*namer.shape(), named, gp_Ax1(at.Location(), along), kTwoPi);
}

} // namespace

ShapePtr hole_tool(const HoleSpec& spec) {
  if (spec.positions.empty()) {
    throw std::invalid_argument("the hole has no position");
  }
  detail::require_positive("hole diameter", spec.diameter);
  if (spec.extent == HoleExtent::Distance) {
    detail::require_positive("hole depth", spec.depth);
  }
  if (spec.extent == HoleExtent::Target && !spec.target) {
    throw std::invalid_argument("the hole has no target");
  }
  return detail::run("hole", [&] {
    BRep_Builder builder;
    TopoDS_Compound compound;
    builder.MakeCompound(compound);
    std::vector<Shape::NamedFace> names;
    for (std::size_t i = 0; i < spec.positions.size(); ++i) {
      const gp_Ax1& at = spec.positions[i];
      double depth = spec.depth;
      bool drill_point = true;
      if (spec.extent == HoleExtent::ThroughAll) {
        depth = depth_through(spec.bodies, at);
        drill_point = false;
      } else if (spec.extent == HoleExtent::Target) {
        depth = depth_to(*spec.target, at);
      }
      const ShapePtr hole = one_hole(spec, static_cast<int>(i), at, depth, drill_point);
      builder.Add(compound, hole->occt());
      for (int f = 0; f < hole->face_count(); ++f) {
        names.push_back({hole->face(f), hole->face_names(f)});
      }
    }
    return std::make_shared<Shape>(compound, names);
  });
}

} // namespace mitcad::geometry
