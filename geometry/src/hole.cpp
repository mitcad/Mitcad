// SPDX-License-Identifier: MIT
#include "mitcad/geometry/hole.hpp"

#include <algorithm>
#include <cmath>
#include <limits>
#include <optional>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GeomAPI_IntCS.hxx>
#include <Geom_Line.hxx>
#include <IntCurvesFace_ShapeIntersector.hxx>
#include <NCollection_List.hxx>
#include <Precision.hxx>
#include <TopAbs_ShapeEnum.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Shape.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Dir.hxx>
#include <gp_Lin.hxx>
#include <gp_Vec.hxx>

#include "face_select.hpp"
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
    // A flat spline face too (a sheet body's wall, mitcad#1554).
    planar = detail::reference_plane(detail::target_faces(target).front()).has_value();
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

// `seam` (a direction at right angles to the axis) is where the revolution
// starts, its round faces' seam; a direction across the axis by default.
ShapePtr one_hole(const HoleSpec& spec, int index, const gp_Ax1& at, double depth, bool drill_point,
                  const std::optional<gp_Dir>& seam = std::nullopt) {
  const gp_Dir along = at.Direction();
  const gp_Dir across = seam                          ? *seam
                        : std::abs(along.X()) < 0.9 ? along.Crossed(gp_Dir(1, 0, 0))
                                                    : along.Crossed(gp_Dir(0, 1, 0));
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

// A press-fit hole (mitcad#1517): its section in the start plane, a
// polygon or a bore with crush ribs, swept along the hole to `depth`, flat
// at the bottom.
ShapePtr press_fit_hole(const HoleSpec& spec, int index, const gp_Ax1& at, double depth) {
  const int count = spec.press_fit_count;
  if (count < 3) {
    throw std::invalid_argument(spec.press_fit == PressFit::Polygon ? "a press fit's polygon needs at least 3 sides"
                                                                    : "a press fit needs at least 3 crush ribs");
  }
  detail::require_positive("press fit inner diameter", spec.press_fit_inner);
  detail::require_finite("press fit angle", spec.press_fit_angle);
  // The turn is measured from the x axis on the start plane (y for a hole
  // along x), counterclockwise about the hole's direction.
  const gp_Dir along = at.Direction();
  const auto onto = [&along](const gp_Vec& axis) { return axis - gp_Vec(along) * axis.Dot(gp_Vec(along)); };
  gp_Vec from = onto(gp_Vec(1, 0, 0));
  if (from.Magnitude() < 0.5) {
    from = onto(gp_Vec(0, 1, 0));
  }
  const gp_Dir ref(from);
  const gp_Dir side = along.Crossed(ref);
  const gp_Pnt origin = at.Location();
  const auto point = [&](double r, double angle) {
    return origin.Translated(gp_Vec(ref) * (r * std::cos(angle)) + gp_Vec(side) * (r * std::sin(angle)));
  };
  const std::string prefix = "hole" + std::to_string(index) + ".";
  BRepBuilderAPI_MakeWire wire;
  std::vector<std::pair<TopoDS_Edge, std::string>> named;
  const auto add = [&](const TopoDS_Edge& edge, const std::string& part) {
    wire.Add(edge);
    if (!wire.IsDone()) {
      throw std::runtime_error("the press fit's section could not be built");
    }
    named.emplace_back(wire.Edge(), face_name(spec.feature, prefix + part));
  };
  const auto line = [](const gp_Pnt& from_point, const gp_Pnt& to_point) {
    BRepBuilderAPI_MakeEdge edge(from_point, to_point);
    if (!edge.IsDone()) {
      throw std::runtime_error("the press fit's section could not be built");
    }
    return edge.Edge();
  };
  const double step = kTwoPi / count;
  if (spec.press_fit == PressFit::Polygon) {
    // The corners lie on the circumscribed circle.
    const double corner = spec.press_fit_inner / 2.0 / std::cos(step / 2.0);
    for (int k = 0; k < count; ++k) {
      const double middle = spec.press_fit_angle + k * step;
      add(line(point(corner, middle - step / 2.0), point(corner, middle + step / 2.0)), "flat" + std::to_string(k));
    }
  } else {
    const double tips = spec.press_fit_inner / 2.0;
    const double bore = spec.press_fit_bore / 2.0;
    if (!(bore > tips)) {
      throw std::invalid_argument("the crush ribs' bore must be wider than their tips");
    }
    // Each rib's base on the bore is twice its height wide.
    const double half = std::asin(std::min(1.0, (bore - tips) / bore));
    if (!(2.0 * half < step)) {
      throw std::invalid_argument("the crush ribs do not fit round the bore");
    }
    const gp_Circ circle(gp_Ax2(origin, along, ref), bore);
    for (int k = 0; k < count; ++k) {
      const double tip = spec.press_fit_angle + k * step;
      const std::string rib = "rib" + std::to_string(k);
      add(line(point(bore, tip - half), point(tips, tip)), rib + ".side0");
      add(line(point(tips, tip), point(bore, tip + half)), rib + ".side1");
      BRepBuilderAPI_MakeEdge arc(circle, point(bore, tip + half), point(bore, tip + step - half));
      if (!arc.IsDone()) {
        throw std::runtime_error("the press fit's section could not be built");
      }
      add(arc.Edge(), "wall" + std::to_string(k));
    }
  }
  BRepBuilderAPI_MakeFace face(wire.Wire(), true);
  if (!face.IsDone()) {
    throw std::runtime_error("the press fit's section could not be built");
  }
  BRepPrimAPI_MakePrism prism(face.Face(), gp_Vec(along) * depth);
  prism.Build();
  if (!prism.IsDone()) {
    throw std::runtime_error("the hole could not be built");
  }
  detail::FaceNamer namer(prism.Shape());
  for (const auto& [edge, name] : named) {
    namer.generated(prism, edge, name);
  }
  namer.add(prism.FirstShape(), face_name(spec.feature, prefix + "top"));
  namer.add(prism.LastShape(), face_name(spec.feature, prefix + "tip"));
  namer.finish();
  return namer.shape();
}

// A print shape's section (mitcad#1515) of the radius `radius` about the
// hole's axis, in its start plane: the circle's lower arc (`<stem>wall`)
// and above it two sides at 45 degrees to `up`, tangent to the circle
// (`<stem>roof0` and `<stem>roof1`, counterclockwise about the hole's
// direction after the arc), meeting at a point the radius times the square
// root of 2 above the axis, or cut short by a flat roof `sag` above the
// circle (`<stem>roof`; 0: none). Swept `length` along the hole: `top` at
// the start, `<stem>roof_end` at the end.
ShapePtr roof_prism(const HoleSpec& spec, int index, const gp_Ax1& at, const gp_Dir& up, const std::string& stem,
                    double radius, double sag, double length) {
  detail::require_positive("hole radius", radius);
  detail::require_positive("hole length", length);
  const gp_Dir along = at.Direction();
  const gp_Dir side = along.Crossed(up);
  const gp_Pnt centre = at.Location();
  // A point `height` along `up` and `aside` along `side` from the axis.
  const auto point = [&](double height, double aside) {
    return centre.Translated(gp_Vec(up) * height + gp_Vec(side) * aside);
  };
  const std::string prefix = "hole" + std::to_string(index) + ".";
  BRepBuilderAPI_MakeWire wire;
  std::vector<std::pair<TopoDS_Edge, std::string>> named;
  const auto add = [&](const TopoDS_Edge& edge, const std::string& part) {
    wire.Add(edge);
    if (!wire.IsDone()) {
      throw std::runtime_error("the hole's print shape could not be built");
    }
    named.emplace_back(wire.Edge(), face_name(spec.feature, prefix + stem + part));
  };
  const auto line = [](const gp_Pnt& from_point, const gp_Pnt& to_point) {
    BRepBuilderAPI_MakeEdge edge(from_point, to_point);
    if (!edge.IsDone()) {
      throw std::runtime_error("the hole's print shape could not be built");
    }
    return edge.Edge();
  };
  // The sides touch the circle 45 degrees either side of `up`.
  const double foot = radius / std::sqrt(2.0);
  const double peak = radius * std::sqrt(2.0);
  const gp_Pnt first = point(foot, foot);
  const gp_Pnt last = point(foot, -foot);
  // The circle's positive sense turns from `up` to `side`: from the first
  // foot round under the axis to the last.
  const gp_Circ circle(gp_Ax2(centre, along, up), radius);
  BRepBuilderAPI_MakeEdge arc(circle, first, last);
  if (!arc.IsDone()) {
    throw std::runtime_error("the hole's print shape could not be built");
  }
  add(arc.Edge(), "wall");
  const double roof = radius + sag;
  if (sag > 0.0 && roof < peak - Precision::Confusion()) {
    // The flat roof meets each side where it is `roof` above the axis.
    const double half = peak - roof;
    add(line(last, point(roof, -half)), "roof0");
    add(line(point(roof, -half), point(roof, half)), "roof");
    add(line(point(roof, half), first), "roof1");
  } else {
    // A teardrop's point (a flat roof above it leaves the point).
    add(line(last, point(peak, 0.0)), "roof0");
    add(line(point(peak, 0.0), first), "roof1");
  }
  BRepBuilderAPI_MakeFace face(wire.Wire(), true);
  if (!face.IsDone()) {
    throw std::runtime_error("the hole's print shape could not be built");
  }
  BRepPrimAPI_MakePrism prism(face.Face(), gp_Vec(along) * length);
  prism.Build();
  if (!prism.IsDone()) {
    throw std::runtime_error("the hole's print shape could not be built");
  }
  detail::FaceNamer namer(prism.Shape());
  for (const auto& [edge, name] : named) {
    namer.generated(prism, edge, name);
  }
  namer.add(prism.FirstShape(), face_name(spec.feature, prefix + "top"));
  namer.add(prism.LastShape(), face_name(spec.feature, prefix + stem + "roof_end"));
  namer.finish();
  return namer.shape();
}

// A hole printed lying (mitcad#1515): the round hole with its seam at the
// top, fused with the roof's prism over its cylindrical part (from the
// start to `depth`) and, for a counterbore or counterdrill, over the wider
// one, each face named as it was in them (the faces they share merged).
ShapePtr print_shaped(const HoleSpec& spec, int index, const gp_Ax1& at, double depth, bool drill_point,
                      const gp_Dir& up) {
  if (std::abs(up.Dot(at.Direction())) > 1.0e-6) {
    throw std::invalid_argument("a print shape's roof must point at right angles to the hole's axis");
  }
  if (!(spec.roof_sag >= 0.0) || !std::isfinite(spec.roof_sag)) {
    throw std::invalid_argument("a print shape's flat roof must lie above the hole");
  }
  const ShapePtr plain = one_hole(spec, index, at, depth, drill_point, up);
  std::vector<ShapePtr> roofs{roof_prism(spec, index, at, up, "", spec.diameter / 2.0, spec.roof_sag, depth)};
  if (spec.type == HoleType::Counterbore || spec.type == HoleType::Counterdrill) {
    roofs.push_back(roof_prism(spec, index, at, up, "cbore_", spec.counterbore_diameter / 2.0, spec.roof_sag,
                               spec.counterbore_depth));
  }
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(plain->occt());
  NCollection_List<TopoDS_Shape> tools;
  for (const ShapePtr& roof : roofs) {
    tools.Append(roof->occt());
  }
  BRepAlgoAPI_Fuse fuse;
  fuse.SetArguments(arguments);
  fuse.SetTools(tools);
  detail::build(fuse);
  if (!fuse.IsDone() || fuse.HasErrors()) {
    throw std::runtime_error("the hole's print shape could not be built");
  }
  // The faces the round hole and the roofs share (the wall's lower arc,
  // the start, a flat bottom) merged into one each; the history includes it.
  fuse.SimplifyResult();
  TopoDS_Shape solid;
  int solids = 0;
  for (TopExp_Explorer it(fuse.Shape(), TopAbs_SOLID); it.More(); it.Next()) {
    solid = it.Current();
    ++solids;
  }
  if (solids != 1) {
    throw std::runtime_error("the hole's print shape came apart");
  }
  detail::FaceNamer namer(solid);
  namer.carry(fuse, *plain);
  for (const ShapePtr& roof : roofs) {
    namer.carry(fuse, *roof);
  }
  namer.finish();
  return namer.shape();
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
  // A print shape (mitcad#1515): a roof per position, round or not.
  if (!spec.roofs.empty()) {
    if (spec.roofs.size() != spec.positions.size()) {
      throw std::invalid_argument("the print shape has no roof direction for every hole");
    }
    if (spec.press_fit != PressFit::None || spec.taper != 0.0) {
      throw std::invalid_argument("a press-fit or tapered hole has no print shape");
    }
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
      const int index = static_cast<int>(i);
      const bool roofed = !spec.roofs.empty() && spec.roofs[i].has_value();
      const ShapePtr hole = spec.press_fit != PressFit::None ? press_fit_hole(spec, index, at, depth)
                            : roofed ? print_shaped(spec, index, at, depth, drill_point, *spec.roofs[i])
                                     : one_hole(spec, index, at, depth, drill_point);
      builder.Add(compound, hole->occt());
      for (int f = 0; f < hole->face_count(); ++f) {
        names.push_back({hole->face(f), hole->face_names(f)});
      }
    }
    return std::make_shared<Shape>(compound, names);
  });
}

} // namespace mitcad::geometry
