// SPDX-License-Identifier: MIT
#include "mitcad/geometry/press_pull.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>
#include <cstdio>
#include <functional>
#include <initializer_list>
#include <limits>
#include <memory>
#include <optional>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepGProp.hxx>
#include <BRepLib.hxx>
#include <BRepOffsetAPI_MakeOffset.hxx>
#include <BRepOffsetAPI_MakePipeShell.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <GeomAbs_JoinType.hxx>
#include <GeomAbs_SurfaceType.hxx>
#include <Geom_Surface.hxx>
#include <NCollection_IndexedMap.hxx>
#include <NCollection_List.hxx>
#include <Standard_Failure.hxx>
#include <TopAbs.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Solid.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Lin.hxx>
#include <gp_Pln.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "face_select.hpp"
#include "history.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/face_extrude.hpp"
#include "mitcad/geometry/face_revolve.hpp"
#include "mitcad/geometry/naming.hpp"
#include "mitcad/geometry/user_error.hpp"
#include "replace.hpp"
#include "sweep.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

constexpr double kPi = 3.14159265358979323846;
constexpr double kTwoPi = 2.0 * kPi;
// Sines of angles below this count as parallel directions.
constexpr double kAngular = 1.0e-6;
// Axes and centres closer than this count as one, mm.
constexpr double kCoaxial = 1.0e-5;
// Points closer than this to a face lie on it, mm.
constexpr double kOnFace = 1.0e-5;
// As an extrusion's tapers: below 90 degrees by a margin (85 degrees
// itself allowed, with rounding).
constexpr double kMaxTaper = 85.0 * kPi / 180.0 + 1.0e-9;

using Edges = std::vector<std::pair<TopoDS_Edge, std::string>>;

std::string text(double value) {
  char buffer[32];
  std::snprintf(buffer, sizeof buffer, "%.6g", value);
  return buffer;
}

bool parallel(const gp_Dir& a, const gp_Dir& b) { return gp_Vec(a).Crossed(gp_Vec(b)).Magnitude() < kAngular; }

// A face beside the pulled one, and what its surface is about.
struct Wall {
  int face = -1;
  GeomAbs_SurfaceType type = GeomAbs_OtherSurface;
  gp_Dir normal;          // a plane's
  gp_Ax1 axis;            // a cylinder's, cone's, torus' or surface of revolution's; an extrusion's direction
  gp_Pnt centre;          // a sphere's
};

Wall wall_of(const Shape& body, int f) {
  Wall wall;
  wall.face = f;
  TopLoc_Location location;
  if (BRep_Tool::Surface(body.face(f), location).IsNull()) {
    return wall; // a mesh body's face: nothing to continue
  }
  const BRepAdaptor_Surface surface(body.face(f));
  wall.type = surface.GetType();
  switch (wall.type) {
  case GeomAbs_Plane:
    wall.normal = surface.Plane().Axis().Direction();
    break;
  case GeomAbs_Cylinder:
    wall.axis = surface.Cylinder().Axis();
    break;
  case GeomAbs_Cone:
    wall.axis = surface.Cone().Axis();
    break;
  case GeomAbs_Torus:
    wall.axis = surface.Torus().Axis();
    break;
  case GeomAbs_Sphere:
    wall.centre = surface.Sphere().Location();
    break;
  case GeomAbs_SurfaceOfRevolution:
    wall.axis = surface.AxeOfRevolution();
    break;
  case GeomAbs_SurfaceOfExtrusion:
    wall.axis = gp_Ax1(gp_Pnt(0.0, 0.0, 0.0), surface.Direction());
    break;
  default:
    break;
  }
  return wall;
}

// The faces beside the face across its edges, each once.
std::vector<Wall> walls_of(const Shape& body, int f) {
  std::vector<int> seen;
  std::vector<Wall> walls;
  for (TopExp_Explorer e(body.face(f), TopAbs_EDGE); e.More(); e.Next()) {
    const int edge = body.edge_index(e.Current());
    if (edge < 0) {
      continue;
    }
    const std::array<int, 2> faces = body.edge_faces(edge);
    for (const int other : faces) {
      if (other >= 0 && other != f && std::find(seen.begin(), seen.end(), other) == seen.end()) {
        seen.push_back(other);
        walls.push_back(wall_of(body, other));
      }
    }
  }
  return walls;
}

// The wall runs along the direction: a plane containing it, a cylinder or
// an extrusion along it.
bool fits_straight(const Wall& wall, const gp_Dir& direction) {
  switch (wall.type) {
  case GeomAbs_Plane:
    return std::abs(wall.normal.Dot(direction)) < kAngular;
  case GeomAbs_Cylinder:
  case GeomAbs_SurfaceOfExtrusion:
    return parallel(wall.axis.Direction(), direction);
  default:
    return false;
  }
}

// The wall is of revolution about the axis: a plane square to it, a
// cylinder, cone, torus or surface of revolution about it, a sphere centred
// on it.
bool fits_turn(const Wall& wall, const gp_Ax1& axis) {
  const gp_Lin line(axis);
  switch (wall.type) {
  case GeomAbs_Plane:
    return parallel(wall.normal, axis.Direction());
  case GeomAbs_Cylinder:
  case GeomAbs_Cone:
  case GeomAbs_Torus:
  case GeomAbs_SurfaceOfRevolution:
    return parallel(wall.axis.Direction(), axis.Direction()) && line.Distance(wall.axis.Location()) < kCoaxial;
  case GeomAbs_Sphere:
    return line.Distance(wall.centre) < kCoaxial;
  default:
    return false;
  }
}

// The direction the walls might run along: a cylinder's or an extrusion's,
// else the line two planes meet in.
std::optional<gp_Dir> straight_candidate(const std::vector<Wall>& walls) {
  for (const Wall& wall : walls) {
    if (wall.type == GeomAbs_Cylinder || wall.type == GeomAbs_SurfaceOfExtrusion) {
      return wall.axis.Direction();
    }
  }
  for (std::size_t i = 0; i < walls.size(); ++i) {
    for (std::size_t j = i + 1; j < walls.size(); ++j) {
      if (walls[i].type != GeomAbs_Plane || walls[j].type != GeomAbs_Plane) {
        continue;
      }
      const gp_Vec across = gp_Vec(walls[i].normal).Crossed(gp_Vec(walls[j].normal));
      if (across.Magnitude() > 1.0e-3) {
        return gp_Dir(across);
      }
    }
  }
  return std::nullopt;
}

// The axis the walls might turn about: the first curved wall's.
std::optional<gp_Ax1> turn_candidate(const std::vector<Wall>& walls) {
  for (const Wall& wall : walls) {
    switch (wall.type) {
    case GeomAbs_Cylinder:
    case GeomAbs_Cone:
    case GeomAbs_Torus:
    case GeomAbs_SurfaceOfRevolution:
      return wall.axis;
    default:
      break;
    }
  }
  return std::nullopt;
}

// The one face a reference finds.
int one_face(const Shape& body, const std::string& face) {
  const std::vector<int> found = body.find_faces(face);
  if (found.empty()) {
    throw std::invalid_argument(no_face_error(face));
  }
  if (found.size() > 1) {
    throw std::invalid_argument(face_error(face, "the face is in " + std::to_string(found.size()) + " pieces; pick one of them",
                                           face + " names " + std::to_string(found.size()) + " faces; pick one of them"));
  }
  return found.front();
}

// A name of the face that finds it alone.
std::string reference_of(const Shape& body, int f) {
  for (const std::string& name : body.face_names(f)) {
    const std::vector<int> found = body.find_faces(name);
    if (found.size() == 1 && found.front() == f) {
      return name;
    }
  }
  throw std::invalid_argument("a face to press pull has no name of its own");
}

// How the walls of the face continue (press_pull.hpp).
FaceRun run_of(const Shape& body, int f, const std::string& name) {
  const TopoDS_Face& face = body.face(f);
  TopLoc_Location location;
  if (BRep_Tool::Surface(face, location).IsNull() || BRepAdaptor_Surface(face).GetType() != GeomAbs_Plane) {
    const std::string why = " is not flat: Straight and Along the walls take a flat face; use Offset";
    throw std::invalid_argument(face_error(name, "the face" + why, "face " + name + why));
  }
  FaceRun run;
  GProp_GProps props;
  BRepGProp::SurfaceProperties(face, props);
  run.centre = props.CentreOfMass();
  run.normal = detail::planar_normal(face, BRepAdaptor_Surface(face).Plane());
  for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
    const BRepAdaptor_Curve curve(TopoDS::Edge(e.Current()));
    for (int i = 0; i <= 32; ++i) {
      const double t = curve.FirstParameter() + (curve.LastParameter() - curve.FirstParameter()) * i / 32.0;
      run.reach = std::max(run.reach, run.centre.Distance(curve.Value(t)));
    }
  }
  const std::vector<Wall> walls = walls_of(body, f);
  const std::optional<gp_Dir> straight = straight_candidate(walls);
  const std::optional<gp_Ax1> turn = turn_candidate(walls);
  const auto all = [&walls](const std::function<bool(const Wall&)>& fits) {
    return !walls.empty() && std::all_of(walls.begin(), walls.end(), fits);
  };
  if (straight && std::abs(straight->Dot(run.normal)) > kAngular &&
      all([&](const Wall& wall) { return fits_straight(wall, *straight); })) {
    run.along = FaceRun::Along::Straight;
    run.direction = straight->Dot(run.normal) > 0.0 ? *straight : straight->Reversed();
    return run;
  }
  if (turn && all([&](const Wall& wall) { return fits_turn(wall, *turn); })) {
    const double radius = gp_Lin(*turn).Distance(run.centre);
    if (radius > kCoaxial) {
      const gp_Vec tangent = gp_Vec(turn->Direction()).Crossed(gp_Vec(turn->Location(), run.centre));
      const double out = tangent.Normalized().Dot(gp_Vec(run.normal));
      if (std::abs(out) > kAngular) {
        run.along = FaceRun::Along::Turn;
        run.axis = out > 0.0 ? *turn : turn->Reversed();
        run.radius = radius;
        return run;
      }
    }
  }
  // The walls that fit neither way stop the run; where each fits one way
  // (and the face lies along the run), all of them.
  std::vector<ErrorPlace> places;
  std::string names;
  const auto add = [&](const Wall& wall) {
    const std::string wall_name = detail::primary_name(body, wall.face);
    places.push_back(face_place(wall_name));
    names += (names.empty() ? "" : ", ") + wall_name;
  };
  for (const Wall& wall : walls) {
    if (!(straight && fits_straight(wall, *straight)) && !(turn && fits_turn(wall, *turn))) {
      add(wall);
    }
  }
  if (places.empty()) {
    for (const Wall& wall : walls) {
      add(wall);
    }
  }
  run.refusal = placed_error("the walls do not continue as a turn or a straight run; use Offset or Straight",
                             "the walls of face " + name + " do not continue as a turn or a straight run (" +
                                 names + "); use Offset or Straight",
                             places);
  return run;
}

// Throws where an inward draft of `inset` at the end of the run closes the
// face: past half its narrowest width across its plane's axes, or where
// the plane offset of its outline that far in leaves nothing.
void check_draft_open(const TopoDS_Face& face, const std::string& key, double inset) {
  const gp_Pln plane = BRepAdaptor_Surface(face).Plane();
  const gp_Vec x(plane.XAxis().Direction());
  const gp_Vec y(plane.YAxis().Direction());
  double x0 = std::numeric_limits<double>::infinity();
  double x1 = -x0;
  double y0 = x0;
  double y1 = -x0;
  for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
    const BRepAdaptor_Curve curve(TopoDS::Edge(e.Current()));
    for (int i = 0; i <= 32; ++i) {
      const double t = curve.FirstParameter() + (curve.LastParameter() - curve.FirstParameter()) * i / 32.0;
      const gp_Vec p(plane.Location(), curve.Value(t));
      x0 = std::min(x0, p.Dot(x));
      x1 = std::max(x1, p.Dot(x));
      y0 = std::min(y0, p.Dot(y));
      y1 = std::max(y1, p.Dot(y));
    }
  }
  bool closes = inset >= 0.5 * std::min(x1 - x0, y1 - y0);
  // An offset either way that is done and empty is the inward one: an
  // outward offset never vanishes.
  for (const double sign : {-1.0, 1.0}) {
    if (closes) {
      break;
    }
    try {
      BRepOffsetAPI_MakeOffset offset(face, GeomAbs_Arc);
      offset.Perform(sign * inset);
      if (offset.IsDone()) {
        const TopoDS_Shape shape = offset.Shape();
        closes = shape.IsNull() || !TopExp_Explorer(shape, TopAbs_EDGE).More();
      }
    } catch (const Standard_Failure&) {
      // undecided: the taper itself tells
    }
  }
  if (closes) {
    throw std::invalid_argument(face_error(
        key, "the taper closes the face to nothing before the end of the run; make the angle or the distance smaller",
        "the taper closes face " + key + " to nothing: its edges move " + text(inset) +
            " mm in by the end of the run"));
  }
}

// The face copied, sharing nothing with the body, and its edges with the
// names of the walls they sweep (as extrude_face names them).
std::pair<TopoDS_Face, Edges> copied_face(const Shape& body, const TopoDS_Face& face, const std::string& feature) {
  const detail::InputCopy copy(face);
  Edges edges;
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> map;
  TopExp::MapShapes(face, TopAbs_EDGE, map);
  for (int i = 1; i <= map.Extent(); ++i) {
    const TopoDS_Edge& edge = TopoDS::Edge(map(i));
    const std::optional<std::string> name = body.name_of_edge(edge);
    edges.emplace_back(TopoDS::Edge(copy.of(edge)), face_name(feature, "side", name ? *name : std::to_string(i - 1)));
  }
  return {TopoDS::Face(copy.shape()), edges};
}

TopoDS_Shape moved(const TopoDS_Shape& shape, const gp_Trsf& trsf) {
  return BRepBuilderAPI_Transform(shape, trsf, true).Shape();
}

gp_Pnt middle_of(const TopoDS_Edge& edge) {
  const BRepAdaptor_Curve curve(edge);
  return curve.Value((curve.FirstParameter() + curve.LastParameter()) / 2.0);
}

gp_Pnt centre_of_wire(const TopoDS_Wire& wire) {
  GProp_GProps props;
  BRepGProp::LinearProperties(wire, props);
  return props.CentreOfMass();
}

bool on_plane(const TopoDS_Face& face, const gp_Pln& plane) {
  const BRepAdaptor_Surface surface(face);
  if (surface.GetType() != GeomAbs_Plane) {
    return false;
  }
  const gp_Pln own = surface.Plane();
  return parallel(own.Axis().Direction(), plane.Axis().Direction()) && plane.Distance(own.Location()) < kOnFace;
}

bool touches(const TopoDS_Face& face, const gp_Pnt& point) {
  BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(point).Vertex(), face);
  return distance.IsDone() && distance.Value() < kOnFace;
}

// A solid of the run named: its cap on `start` and its cap on `end` by
// their names, each other face by the name of the face's edge it starts
// from (the edge's middle lies on it).
ShapePtr named_piece(TopoDS_Shape solid, const Edges& edges, const gp_Pln& start, const gp_Pln& end,
                     const NameList& start_names, const NameList& end_names) {
  if (solid.ShapeType() == TopAbs_SOLID) {
    TopoDS_Solid oriented = TopoDS::Solid(solid);
    BRepLib::OrientClosedSolid(oriented);
    solid = oriented;
  }
  detail::FaceNamer namer(solid);
  for (TopExp_Explorer f(solid, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face& face = TopoDS::Face(f.Current());
    if (on_plane(face, start)) {
      for (const std::string& name : start_names) {
        namer.add(face, name);
      }
      continue;
    }
    if (on_plane(face, end)) {
      for (const std::string& name : end_names) {
        namer.add(face, name);
      }
      continue;
    }
    for (const auto& [edge, name] : edges) {
      if (touches(face, middle_of(edge))) {
        namer.add(face, name);
        break;
      }
    }
  }
  return namer.shape();
}

// A ruled loft between a loop and its image: planes and cones where the
// image is the loop scaled and moved.
TopoDS_Shape ruled_solid(const TopoDS_Wire& from, const TopoDS_Wire& to) {
  BRepOffsetAPI_ThruSections loft(true, true, 1.0e-6);
  loft.AddWire(from);
  loft.AddWire(to);
  loft.CheckCompatibility(false);
  detail::build(loft);
  if (!loft.IsDone()) {
    throw std::runtime_error("the scaled run of the face could not be built");
  }
  return loft.Shape();
}

// A sweep along an arc from a loop to the loop at its end, the sections
// turning with the arc about its axis (a fixed binormal) and changing
// linearly along it.
TopoDS_Shape turned_solid(const TopoDS_Wire& from, const TopoDS_Wire& to, const TopoDS_Wire& spine,
                          const gp_Dir& axis) {
  BRepOffsetAPI_MakePipeShell pipe(spine);
  pipe.SetMode(axis);
  pipe.SetTolerance(1.0e-5, 1.0e-5, 1.0e-3);
  TopoDS_Vertex first;
  TopoDS_Vertex last;
  TopExp::Vertices(spine, first, last);
  pipe.Add(from, first, false, false);
  pipe.Add(to, last, false, false);
  detail::build(pipe);
  if (!pipe.IsDone() || !pipe.MakeSolid()) {
    throw std::runtime_error("the tapered turn of the face could not be built");
  }
  return pipe.Shape();
}

// The new piece of a face from its loops paired with their ends (the
// outline first): each loop's solid by `make`, the outline's less the
// holes'.
ShapePtr piece_of(const std::vector<std::pair<TopoDS_Wire, TopoDS_Wire>>& loops, const Edges& edges,
                  const std::function<TopoDS_Shape(const TopoDS_Wire&, const TopoDS_Wire&)>& make,
                  const gp_Pln& start, const gp_Pln& end, const NameList& start_names, const NameList& end_names) {
  ShapePtr piece;
  for (const auto& [from, to] : loops) {
    const ShapePtr solid = named_piece(make(from, to), edges, start, end, start_names, end_names);
    if (!piece) {
      piece = solid;
      continue;
    }
    const BooleanResult cut = boolean(BooleanOp::Cut, {piece.get()}, *solid);
    if (cut.pieces.size() != 1) {
      throw std::runtime_error("the run of a hole of the face could not be built");
    }
    piece = cut.pieces.front().shape;
  }
  return piece;
}

// The face's loops, the outline first.
std::vector<TopoDS_Wire> loops_of(const TopoDS_Face& face) {
  const TopoDS_Wire outer = BRepTools::OuterWire(face);
  std::vector<TopoDS_Wire> loops{outer};
  for (TopExp_Explorer w(face, TopAbs_WIRE); w.More(); w.Next()) {
    if (!w.Current().IsSame(outer)) {
      loops.push_back(TopoDS::Wire(w.Current()));
    }
  }
  return loops;
}

// The face's loops paired with the loops of `end` (a face of as many loops,
// in the face's plane): outline with outline, each hole with the hole of
// `end` nearest to where `expected` takes it.
std::vector<std::pair<TopoDS_Wire, TopoDS_Wire>> paired(const TopoDS_Face& face, const TopoDS_Face& end,
                                                        const gp_Trsf& expected, const std::string& key) {
  const std::vector<TopoDS_Wire> from = loops_of(face);
  std::vector<TopoDS_Wire> to = loops_of(end);
  if (to.size() != from.size()) {
    throw std::invalid_argument(face_error(
        key, "the taper closes a hole of the face before the end of the run; make the angle or the distance smaller",
        "the taper closes a hole of face " + key + " before the end of the run"));
  }
  std::vector<std::pair<TopoDS_Wire, TopoDS_Wire>> pairs{{from.front(), to.front()}};
  to.erase(to.begin());
  for (std::size_t i = 1; i < from.size(); ++i) {
    const gp_Pnt want = centre_of_wire(from[i]).Transformed(expected);
    std::size_t best = 0;
    for (std::size_t j = 1; j < to.size(); ++j) {
      if (centre_of_wire(to[j]).Distance(want) < centre_of_wire(to[best]).Distance(want)) {
        best = j;
      }
    }
    pairs.emplace_back(from[i], to[best]);
    to.erase(to.begin() + static_cast<std::ptrdiff_t>(best));
  }
  return pairs;
}

// The shape with these names added to its face named `end`.
ShapePtr with_end_names(const Shape& tool, const std::string& end, const NameList& names) {
  std::vector<Shape::NamedFace> faces;
  for (int i = 0; i < tool.face_count(); ++i) {
    NameList list = tool.face_names(i);
    if (std::find(list.begin(), list.end(), end) != list.end()) {
      for (const std::string& name : names) {
        if (std::find(list.begin(), list.end(), name) == list.end()) {
          list.push_back(name);
        }
      }
    }
    faces.push_back({tool.face(i), list});
  }
  return std::make_shared<Shape>(tool.occt(), faces);
}

// Throws where the turn's piece overlaps the body (more than touching it):
// the turn runs into the body itself. The places: the body's faces at the
// overlap, other than the face, and the overlap's middle.
void check_turn_clear(const Shape& body, int f, const Shape& tool, const std::string& key) {
  BRepAlgoAPI_Common common;
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(body.occt());
  NCollection_List<TopoDS_Shape> tools;
  tools.Append(tool.occt());
  common.SetArguments(arguments);
  common.SetTools(tools);
  common.SetNonDestructive(true);
  detail::build(common);
  if (!common.IsDone() || common.HasErrors()) {
    return; // the join says what is wrong
  }
  const double overlap = std::abs(detail::volume_of(common.Shape()));
  const double size = std::abs(detail::volume_of(tool.occt()));
  if (overlap <= 1.0e-6 * std::max(1.0, size)) {
    return;
  }
  std::vector<ErrorPlace> places;
  for (int i = 0; i < body.face_count(); ++i) {
    if (i == f) {
      continue;
    }
    BRepExtrema_DistShapeShape distance(body.face(i), common.Shape());
    if (distance.IsDone() && distance.Value() < kOnFace) {
      places.push_back(face_place(detail::primary_name(body, i)));
    }
  }
  GProp_GProps props;
  BRepGProp::VolumeProperties(common.Shape(), props);
  places.push_back(point_place(props.CentreOfMass()));
  throw std::invalid_argument(placed_error("the turn runs into the body itself; make it shorter",
                                           "the turn of face " + key + " runs into the body itself (" + text(overlap) +
                                               " mm3 overlap)",
                                           places));
}

// The new piece of one face, named, the end keeping the face's names.
ShapePtr tool_of(const Shape& body, int f, const std::string& key, const PressPullSpec& spec) {
  const FaceRun run = run_of(body, f, key);
  const bool join = spec.distance > 0.0;
  double length = std::abs(spec.distance);
  const bool draft = spec.taper == PressPullTaper::Draft && spec.taper_angle != 0.0;
  const bool scale = spec.taper == PressPullTaper::Scale && spec.end_scale != 1.0;
  const TopoDS_Face& face = body.face(f);
  // A straight run along the face's normal ends on the level of another
  // planar face of the body where it comes within detail::kPlanarLevel of
  // it (mitcad#2548, as Move Face's and Offset Face's): flush with the side
  // of the base under a raised block's end, not a fraction of a micrometre
  // past it.
  const bool straight = spec.run == PressPullRun::Straight || run.along == FaceRun::Along::Straight;
  const bool normal_run =
      spec.run == PressPullRun::Straight || run.direction.IsParallel(run.normal, kAngular);
  if (straight && normal_run && !draft && !scale) {
    if (const std::optional<gp_Pln> own = detail::face_plane(face)) {
      const gp_Vec outward(own->Axis().Direction());
      if (const std::optional<gp_Vec> landed = detail::landing_motion(body, {f}, *own, outward * spec.distance)) {
        length = std::abs(landed->Dot(outward));
      }
    }
  }
  const std::string start_name = face_name(spec.feature, "start", key);
  const std::string end_name = face_name(spec.feature, "end", key);
  if (draft && spec.taper_angle < 0.0) {
    check_draft_open(face, key, length * std::tan(-spec.taper_angle));
  }
  const gp_Pln plane = BRepAdaptor_Surface(face).Plane();
  ShapePtr tool;

  if (straight) {
    const std::optional<gp_Dir> along =
        spec.run == PressPullRun::Along ? std::optional<gp_Dir>(run.direction) : std::nullopt;
    if (!scale) {
      // Extrude's own taper path (sweep.cpp `tapered`).
      FaceExtrudeSpec extrusion;
      extrusion.feature = spec.feature;
      extrusion.face = key;
      extrusion.key = key;
      extrusion.reversed = !join;
      extrusion.distance1 = length;
      extrusion.direction = along;
      extrusion.taper1 = draft ? spec.taper_angle : 0.0;
      tool = extrude_face(body, extrusion);
    } else {
      const gp_Vec vector = gp_Vec(along ? *along : run.normal) * (join ? length : -length);
      gp_Trsf scaling;
      scaling.SetScale(run.centre, spec.end_scale);
      gp_Trsf move;
      move.SetTranslation(vector);
      const gp_Trsf to_end = move.Multiplied(scaling);
      const auto [copy, edges] = copied_face(body, face, spec.feature);
      const TopoDS_Face end = TopoDS::Face(moved(copy, to_end));
      tool = piece_of(paired(copy, end, to_end, key), edges, ruled_solid, plane, plane.Translated(vector),
                      {start_name}, {end_name});
    }
  } else {
    if (run.along == FaceRun::Along::None) {
      throw std::invalid_argument(run.refusal);
    }
    const gp_Ax1 axis = join ? run.axis : run.axis.Reversed();
    const double angle = length / run.radius;
    if (angle >= kTwoPi - 1.0e-9) {
      throw std::invalid_argument(face_error(key, "the turn comes round to the face itself; make it shorter",
                                             "the turn of face " + key + " by " + text(angle * 180.0 / kPi) +
                                                 " degrees comes round to the face itself"));
    }
    if (!draft && !scale) {
      FaceRevolveSpec revolution;
      revolution.feature = spec.feature;
      revolution.face = key;
      revolution.key = key;
      revolution.axis = axis;
      revolution.angle1 = angle;
      tool = revolve_face(body, revolution);
    } else {
      gp_Trsf turn;
      turn.SetRotation(axis, angle);
      const auto [copy, edges] = copied_face(body, face, spec.feature);
      // The end's section in the face's plane: scaled about the area
      // centre, or as Extrude's taper leaves the face at the run's length.
      TopoDS_Face section;
      gp_Trsf expected;
      if (scale) {
        expected.SetScale(run.centre, spec.end_scale);
        section = TopoDS::Face(moved(copy, expected));
      } else {
        FaceExtrudeSpec extrusion;
        extrusion.feature = spec.feature;
        extrusion.face = key;
        extrusion.key = key;
        extrusion.distance1 = length;
        extrusion.taper1 = spec.taper_angle;
        const ShapePtr prism = extrude_face(body, extrusion);
        const std::vector<int> ends = prism->find_faces(end_name);
        if (ends.size() != 1) {
          throw std::runtime_error("the drafted end of the turn could not be built");
        }
        gp_Trsf back;
        back.SetTranslation(gp_Vec(run.normal) * -length);
        section = TopoDS::Face(moved(prism->face(ends.front()), back));
      }
      std::vector<std::pair<TopoDS_Wire, TopoDS_Wire>> loops = paired(copy, section, expected, key);
      for (auto& loop : loops) {
        loop.second = TopoDS::Wire(moved(loop.second, turn));
      }
      // The arc the area centre runs along.
      const gp_Pnt foot = axis.Location().Translated(
          gp_Vec(axis.Direction()) * gp_Vec(axis.Location(), run.centre).Dot(gp_Vec(axis.Direction())));
      const gp_Circ circle(gp_Ax2(foot, axis.Direction(), gp_Dir(gp_Vec(foot, run.centre))), run.radius);
      const TopoDS_Wire spine = BRepBuilderAPI_MakeWire(BRepBuilderAPI_MakeEdge(circle, 0.0, angle).Edge()).Wire();
      const auto make = [&spine, &axis](const TopoDS_Wire& from, const TopoDS_Wire& to) {
        return turned_solid(from, to, spine, axis.Direction());
      };
      tool = piece_of(loops, edges, make, plane, plane.Transformed(turn), {start_name}, {end_name});
    }
    if (join) {
      check_turn_clear(body, f, *tool, key);
    }
  }
  return with_end_names(*tool, end_name, body.face_names(f));
}

// One shape of the pieces of a boolean, their names kept.
ShapePtr combined(const std::vector<ShapePtr>& pieces) {
  if (pieces.size() == 1) {
    return pieces.front();
  }
  BRep_Builder builder;
  TopoDS_Compound compound;
  builder.MakeCompound(compound);
  for (const ShapePtr& piece : pieces) {
    builder.Add(compound, piece->occt());
  }
  detail::FaceNamer namer(compound);
  for (const ShapePtr& piece : pieces) {
    for (int i = 0; i < piece->face_count(); ++i) {
      for (const std::string& name : piece->face_names(i)) {
        namer.add(piece->face(i), name);
      }
    }
  }
  return namer.shape();
}

} // namespace

FaceRun face_run(const Shape& body, const std::string& face) {
  detail::require_surfaces(body.occt(), "press pull");
  return detail::run("face run", [&] { return run_of(body, one_face(body, face), face); });
}

ShapePtr press_pull(const Shape& body, const PressPullSpec& spec) {
  detail::require_finite("press pull distance", spec.distance);
  if (spec.distance == 0.0) {
    throw std::invalid_argument("the distance must not be zero");
  }
  if (spec.taper == PressPullTaper::Draft) {
    detail::require_finite("taper angle", spec.taper_angle);
    if (std::abs(spec.taper_angle) > kMaxTaper) {
      throw std::invalid_argument("the taper angle must be between -85 and 85 degrees");
    }
  }
  if (spec.taper == PressPullTaper::Scale && !(spec.end_scale > 0.0 && std::isfinite(spec.end_scale))) {
    throw std::invalid_argument("the end scale must be more than 0 %: the face would close to nothing");
  }
  detail::require_surfaces(body.occt(), "press pull");
  const std::vector<int> chosen = detail::resolve_faces(body, spec.faces, "faces to press pull");
  return detail::run("press pull", [&] {
    ShapePtr result = std::make_shared<Shape>(body);
    for (const int f : chosen) {
      const std::string key = reference_of(body, f);
      const ShapePtr tool = tool_of(body, f, key, spec);
      const bool join = spec.distance > 0.0;
      const BooleanResult made = boolean(join ? BooleanOp::Join : BooleanOp::Cut, {result.get()}, *tool);
      std::vector<ShapePtr> pieces;
      for (const BooleanPiece& piece : made.pieces) {
        pieces.push_back(piece.shape);
      }
      if (!made.touched.empty() && !made.touched.front()) {
        // Untouched: a cut leaves the body as it is; a join (the piece
        // starts on the body, so this is not expected) keeps both.
        if (!join) {
          continue;
        }
        pieces.insert(pieces.begin(), result);
      }
      if (pieces.empty()) {
        throw std::invalid_argument("the cut takes the whole body away");
      }
      result = combined(pieces);
    }
    return detail::finished(*result, "the press pull");
  });
}

} // namespace mitcad::geometry
