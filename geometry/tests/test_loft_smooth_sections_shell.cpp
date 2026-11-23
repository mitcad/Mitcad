// SPDX-License-Identifier: MIT
// Shell and Thicken of lofts through two smooth closed sections, Tangent
// Edges Keep and Merge alike (mitcad#2271), through the geometry facade as
// the model builds them.
//
// A loft whose sections were merged into one smooth closed edge each
// (mitcad#2162) was OCCT's through-sections loft: one side face on a
// clamped B-spline surface that only closes on itself. The kernel's offset
// does not take such a surface (mitcad#2141 found the same for its eased
// faces): the owner's loft from a circle of radius 111 to a slot
// 520 x 80 mm 100 mm away, shelled 2 mm Inside with the circle's end
// removed, failed with Merge and worked with Keep. Two merged sections are
// now skinned as they are, the side's surface periodic where it closes
// smoothly (geometry/src/loft_skin.cpp, periodic_where_closed), and the
// joined curves are continuous in their second derivative along each
// former edge, only in their first across the former joins
// (geometry/src/loft_merge.cpp, joined_curve). A symmetric Thicken offsets
// the merged face twice: its first offset is only C0 along the former
// joins inside the one face, and is split there for the second
// (geometry/src/surfaces.cpp, split_at_creases).
//
// Not one model but a set: circles to slots of several proportions (the
// owner's among them), an ellipse to a rounded rectangle, and a circle to a
// closed spline outline; each with Keep and Merge (one ctest each), shelled
// Inside, Outside and Both, Sharp and Rounded, the first section's end
// removed. Each shell is a valid single solid of positive volume without a
// note, every face of it (but the rim where the end was) lies at a wall's
// thickness from the loft (or on it) within 1e-6 mm, the rounded joins of
// a Rounded wall within the kernel's 1e-4 mm (see kJoin), and for the
// inside wall the kernel's own offset closes (not the cavity Shell falls
// back on). The merged side is one face on a periodic surface. The owner's
// surface lofts thicken to either side and symmetric into valid single
// solids.
//
// Where a shell fails or its volume is not positive, the test prints the
// faces of what the kernel's offset built (their surfaces, orientations
// and areas, the volume as Shell measures it and to 1e-9), so that a
// failing run shows where the wall goes wrong (mitcad#2271).

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <optional>
#include <set>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepExtrema_SupportType.hxx>
#include <BRepGProp.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <GeomAbs_SurfaceType.hxx>
#include <Geom_Surface.hxx>
#include <NCollection_List.hxx>
#include <ShapeUpgrade_ShapeDivideContinuity.hxx>
#include <Standard_Failure.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt2d.hxx>

#include "check.hpp"
#include "mitcad/geometry/faceops.hpp"
#include "mitcad/geometry/loft.hpp"
#include "mitcad/geometry/naming.hpp"
#include "mitcad/geometry/profile.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/surfaces.hpp"

namespace test {
namespace {

namespace g = mitcad::geometry;

constexpr double kWall = 1.0e-6; // mm

// The rounded joins of a Rounded wall are pipes about the loft's edges
// that the kernel's offset approximates within 1e-4 mm (BRepOffset_Offset
// builds them with GeomFill_Pipe at its default tolerance of 1e-4): a point
// of the shell whose nearest point on the loft lies on an edge or a vertex
// of it (not inside a face) lies at a wall's thickness within this.
constexpr double kJoin = 1.0e-4; // mm

// The volume, measured on a copy whose faces are split where their
// surfaces are less than C2 continuous. On the merged side, one face
// through pieces of very different curvature joined C1, and above all on
// its offset, the kernel's integration is far off whole (a 2 mm wall of
// some 85 900 mm3 measured -10 460, or 161 619 to 1e-9; the owner's
// surface thickened 1 mm, 0.1 % over its area to 1e-9 with Keep); split
// there it agrees with a fine triangulation's (mitcad#2271).
double volume_of(const TopoDS_Shape& shape) {
  ShapeUpgrade_ShapeDivideContinuity divide(BRepBuilderAPI_Copy(shape).Shape());
  divide.SetBoundaryCriterion(GeomAbs_C2);
  divide.SetPCurveCriterion(GeomAbs_C0);
  divide.SetSurfaceCriterion(GeomAbs_C2);
  // Perform says whether it split anything; unsplit, the result is the
  // copy as it was.
  divide.Perform();
  const TopoDS_Shape split = divide.Result();
  GProp_GProps props;
  BRepGProp::VolumeProperties(split.IsNull() ? shape : split, props);
  return props.Mass();
}

// The volume to 1e-9 and the integration's own estimate of its relative
// error.
std::pair<double, double> volume_and_error(const TopoDS_Shape& shape) {
  GProp_GProps props;
  const double error = BRepGProp::VolumeProperties(shape, props, 1.0e-9);
  return {props.Mass(), error};
}

// The volume as Shell's check first measures it (the default integration
// over the whole faces).
double plain_volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

// The volume to 1e-9 about the middle of the shape's box rather than the
// origin: the faces' integrals then cancel less, so a difference from
// the volume to 1e-9 about the origin shows how far that integration is off.
double volume_about_middle(const TopoDS_Shape& shape) {
  Bnd_Box box;
  BRepBndLib::Add(shape, box);
  if (box.IsVoid()) {
    return 0.0;
  }
  double x0 = 0.0;
  double y0 = 0.0;
  double z0 = 0.0;
  double x1 = 0.0;
  double y1 = 0.0;
  double z1 = 0.0;
  box.Get(x0, y0, z0, x1, y1, z1);
  GProp_GProps props(gp_Pnt((x0 + x1) / 2, (y0 + y1) / 2, (z0 + z1) / 2));
  BRepGProp::VolumeProperties(shape, props, 1.0e-9);
  return props.Mass();
}

const char* surface_kind(GeomAbs_SurfaceType type) {
  switch (type) {
  case GeomAbs_Plane:
    return "plane";
  case GeomAbs_Cylinder:
    return "cylinder";
  case GeomAbs_Cone:
    return "cone";
  case GeomAbs_Sphere:
    return "sphere";
  case GeomAbs_Torus:
    return "torus";
  case GeomAbs_BezierSurface:
    return "Bezier";
  case GeomAbs_BSplineSurface:
    return "B-spline";
  case GeomAbs_SurfaceOfRevolution:
    return "revolution";
  case GeomAbs_SurfaceOfExtrusion:
    return "extrusion";
  case GeomAbs_OffsetSurface:
    return "offset";
  case GeomAbs_OtherSurface:
    return "other";
  }
  return "?";
}

// Each face of the shape as it lies in it: its surface (periodic or not),
// its orientation, its parameter box and its area to 1e-9; then the
// volume both ways (mitcad#2271: the diagnosis of a failing shell).
void describe(const std::string& what, const TopoDS_Shape& shape) {
  int index = 0;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next(), ++index) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    const BRepAdaptor_Surface surface(face);
    double u0 = 0.0;
    double u1 = 0.0;
    double v0 = 0.0;
    double v1 = 0.0;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    GProp_GProps props;
    const double error = BRepGProp::SurfaceProperties(face, props, 1.0e-9);
    std::printf("%s: face %d, %s%s%s surface, %s, u %.6f to %.6f, v %.6f to %.6f, area %.6f (error %.1e), "
                "valid %d\n",
                what.c_str(), index, surface_kind(surface.GetType()), surface.IsUPeriodic() ? ", U periodic" : "",
                surface.IsVPeriodic() ? ", V periodic" : "",
                face.Orientation() == TopAbs_REVERSED ? "reversed" : "forward", u0, u1, v0, v1, props.Mass(),
                error, BRepCheck_Analyzer(face).IsValid() ? 1 : 0);
  }
  const auto [volume, error] = volume_and_error(shape);
  std::printf("%s: volume %.6f to 1e-9 (error %.1e), %.6f about its middle, %.6f as Shell measures it\n",
              what.c_str(), volume, error, volume_about_middle(shape), plain_volume_of(shape));
}

double area_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(shape, props, 1.0e-9);
  return props.Mass();
}

int count(const TopoDS_Shape& shape, TopAbs_ShapeEnum type) {
  int n = 0;
  for (TopExp_Explorer it(shape, type); it.More(); it.Next()) {
    ++n;
  }
  return n;
}

g::Segment line(const std::string& name, double x0, double y0, double x1, double y1) {
  g::Segment s;
  s.name = name;
  s.kind = g::SegmentKind::Line;
  s.start = gp_Pnt2d(x0, y0);
  s.end = gp_Pnt2d(x1, y1);
  return s;
}

g::Segment arc(const std::string& name, double cx, double cy, double radius, double from) {
  g::Segment s;
  s.name = name;
  s.kind = g::SegmentKind::Arc;
  s.center = gp_Pnt2d(cx, cy);
  s.radius = radius;
  s.start_angle = from;
  s.end_angle = from + kPi / 2;
  return s;
}

g::Region region_of(const std::string& name, const std::vector<g::Segment>& segments) {
  g::Region region;
  region.name = name;
  region.loops.push_back(g::Loop{segments});
  return region;
}

// A circle of `radius` about the sketch origin.
g::Region circle(double radius) {
  g::Segment s;
  s.name = "c1";
  s.kind = g::SegmentKind::Circle;
  s.center = gp_Pnt2d(0, 0);
  s.radius = radius;
  return region_of("r{c1}", {s});
}

// A centre-point slot along the sketch's x axis about its origin, its arcs'
// centres `half` either side of it, of radius `radius`, counter-clockwise
// from its lower straight side.
g::Region slot(double half, double radius) {
  g::Segment right = arc("c3", half, 0, radius, -kPi / 2);
  right.end_angle = kPi / 2;
  g::Segment left = arc("c5", -half, 0, radius, kPi / 2);
  left.end_angle = 3 * kPi / 2;
  return region_of("r{c2,c3,c4,c5}", {line("c2", -half, -radius, half, -radius), right,
                                      line("c4", half, radius, -half, radius), left});
}

// An ellipse of half-axes `a` along x and `b` about the sketch origin.
g::Region ellipse(double a, double b) {
  g::Segment s;
  s.name = "c1";
  s.kind = g::SegmentKind::Ellipse;
  s.center = gp_Pnt2d(0, 0);
  s.radius = a;
  s.minor_radius = b;
  return region_of("r{c1}", {s});
}

// A rectangle `w` x `h` about the sketch origin with its corners rounded
// to `r`, counter-clockwise from its lower side: lines and quarter arcs
// meeting tangentially.
g::Region rounded_rectangle(double w, double h, double r) {
  const double x = w / 2;
  const double y = h / 2;
  return region_of("r{c2,c3,c4,c5,c6,c7,c8,c9}",
                   {line("c2", -x + r, -y, x - r, -y), arc("c3", x - r, -y + r, r, -kPi / 2),
                    line("c4", x, -y + r, x, y - r), arc("c5", x - r, y - r, r, 0.0),
                    line("c6", x - r, y, -x + r, y), arc("c7", -x + r, y - r, r, kPi / 2),
                    line("c8", -x, y - r, -x, -y + r), arc("c9", -x + r, -y + r, r, kPi)});
}

// A closed periodic cubic spline about the sketch origin through ten
// poles at radii that wave round it: a smooth outline of no particular
// symmetry, some 30 mm across from its middle.
g::Region spline_outline() {
  g::Segment s;
  s.name = "c1";
  s.kind = g::SegmentKind::BSpline;
  s.degree = 3;
  s.periodic = true;
  for (int i = 0; i < 10; ++i) {
    const double t = i * kPi / 5.0;
    const double rho = 30.0 * (1.0 + 0.2 * std::cos(2 * t) + 0.12 * std::sin(3 * t));
    s.poles.emplace_back(rho * std::cos(t), rho * std::sin(t));
  }
  for (int i = 0; i <= 10; ++i) {
    s.knots.push_back(i);
    s.multiplicities.push_back(1);
  }
  return region_of("r{c1}", {s});
}

g::LoftSection section(const g::Frame& frame, const g::Region& region) {
  g::LoftSection s;
  s.frame = frame;
  s.region = region;
  return s;
}

// A loft of the two regions, on XY and `height` above it.
g::LoftSpec loft_xy(const g::Region& bottom, const g::Region& top, double height) {
  g::Frame lower;
  g::Frame upper;
  upper.origin = gp_Pnt(0, 0, height);
  g::LoftSpec spec;
  spec.feature = "F3";
  spec.sections = {section(lower, bottom), section(upper, top)};
  return spec;
}

// A loft of the two regions as the owner sketched them: on XZ, and on a
// plane `distance` along +Y.
g::LoftSpec loft_xz(const g::Region& first, const g::Region& second, double distance) {
  g::Frame xz;
  xz.x_axis = gp_Dir(1, 0, 0);
  xz.y_axis = gp_Dir(0, 0, 1);
  g::Frame plane = xz;
  plane.origin = gp_Pnt(0, distance, 0);
  g::LoftSpec spec;
  spec.feature = "F3";
  spec.sections = {section(xz, first), section(plane, second)};
  return spec;
}

// The owner's loft (mitcad#2271): a circle of radius 110.9655561796864 on
// XZ, a slot 520 x 80 (centres 220 either side, radius 40) 100 along Y.
g::LoftSpec owner_loft() { return loft_xz(circle(110.9655561796864), slot(220, 40), 100); }

// The side faces of the loft (named <feature>:side(...)).
std::vector<TopoDS_Face> side_faces(const g::Shape& body) {
  std::vector<TopoDS_Face> faces;
  for (int i = 0; i < body.face_count(); ++i) {
    for (const std::string& name : body.face_names(i)) {
      if (name.rfind("F3:side(", 0) == 0) {
        faces.push_back(body.face(i));
        break;
      }
    }
  }
  return faces;
}

// Merged: the side is one face, on a periodic surface continuous in its
// first derivative (mitcad#2271).
void check_merged_side(const char* what, const g::Shape& body) {
  const std::vector<TopoDS_Face> sides = side_faces(body);
  CHECK(sides.size() == 1);
  if (sides.size() != 1) {
    std::printf("%s: %zu side faces\n", what, sides.size());
    return;
  }
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(sides.front());
  CHECK(!surface.IsNull());
  if (surface.IsNull()) {
    return;
  }
  const bool periodic = surface->IsUPeriodic() || surface->IsVPeriodic();
  // How it lies in the solid (its surface's normals out of it or into it).
  bool reversed = false;
  for (TopExp_Explorer it(body.occt(), TopAbs_FACE); it.More(); it.Next()) {
    if (it.Current().IsSame(sides.front())) {
      reversed = it.Current().Orientation() == TopAbs_REVERSED;
    }
  }
  std::printf("%s: one side face, its surface %s, %s in the solid\n", what, periodic ? "periodic" : "not periodic",
              reversed ? "reversed" : "forward");
  CHECK(periodic);
  CHECK(surface->Continuity() != GeomAbs_C0);
}

// The kernel's own offset of (a copy of) the body by `offset` with the
// face `removed` open, as Shell first asks it (before the cavity); none
// when it is not done.
std::optional<TopoDS_Shape> kernel_offset(const g::Shape& body, const std::string& removed, double offset,
                                          bool rounded) {
  const std::vector<int> faces = body.find_faces(removed);
  if (faces.size() != 1) {
    return std::nullopt;
  }
  try {
    BRepBuilderAPI_Copy copy(body.occt());
    const NCollection_List<TopoDS_Shape>& images = copy.Modified(body.face(faces.front()));
    if (images.IsEmpty()) {
      return std::nullopt;
    }
    NCollection_List<TopoDS_Shape> closing;
    closing.Append(images.First());
    BRepOffsetAPI_MakeThickSolid maker;
    maker.MakeThickSolidByJoin(copy.Shape(), closing, offset, 1.0e-6, BRepOffset_Skin, false, false,
                               rounded ? GeomAbs_Arc : GeomAbs_Intersection, false);
    if (!maker.IsDone()) {
      return std::nullopt;
    }
    return maker.Shape();
  } catch (const Standard_Failure& failure) {
    std::printf("the kernel's offset raised: %s\n", failure.what());
    return std::nullopt;
  }
}

// The kernel's own offset closes into a valid shape.
bool kernel_closes(const g::Shape& body, const std::string& removed, double offset, bool rounded) {
  const std::optional<TopoDS_Shape> wall = kernel_offset(body, removed, offset, rounded);
  return wall && BRepCheck_Analyzer(*wall).IsValid();
}

// What the kernel's own offset builds of the body's wall at `offset`,
// face by face (a failing shell's diagnosis).
void describe_kernel_offset(const std::string& what, const g::Shape& body, const std::string& removed, double offset,
                            bool rounded) {
  const std::optional<TopoDS_Shape> wall = kernel_offset(body, removed, offset, rounded);
  if (!wall) {
    std::printf("%s: the kernel's offset at %g is not done\n", what.c_str(), offset);
    return;
  }
  std::printf("%s: the kernel's offset at %g, valid %d, %d solids\n", what.c_str(), offset,
              BRepCheck_Analyzer(*wall).IsValid() ? 1 : 0, count(*wall, TopAbs_SOLID));
  describe(what + ", the kernel's offset", *wall);
}

// Every face of the shell but those in the plane of the removed end (the
// rim) lies at one of the distances `allowed` from the loft's faces, at up
// to five points of it (its middle and four about it, those inside the
// face) within kWall, or within kJoin where the nearest point of the loft
// is on an edge or a vertex of it (a rounded join's pipe about that edge);
// each of `needed` is the distance of some face.
void check_walls(const char* what, const g::Shape& body, const gp_Pln& removed_plane, const g::Shape& shell,
                 const std::vector<double>& allowed, const std::vector<double>& needed) {
  BRep_Builder builder;
  TopoDS_Compound loft_faces;
  builder.MakeCompound(loft_faces);
  for (int i = 0; i < body.face_count(); ++i) {
    builder.Add(loft_faces, body.face(i));
  }
  std::set<std::size_t> found;
  int measured = 0;
  for (int i = 0; i < shell.face_count(); ++i) {
    const TopoDS_Face& face = shell.face(i);
    double u0 = 0.0;
    double u1 = 0.0;
    double v0 = 0.0;
    double v1 = 0.0;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    const BRepAdaptor_Surface surface(face);
    if (surface.GetType() == GeomAbs_Plane) {
      const gp_Pln plane = surface.Plane();
      if (plane.Axis().Direction().IsParallel(removed_plane.Axis().Direction(), 1.0e-9) &&
          removed_plane.Distance(plane.Location()) <= kWall) {
        continue; // the rim
      }
    }
    for (const auto& [su, sv] : {std::pair{0.5, 0.5}, {0.3, 0.3}, {0.7, 0.3}, {0.3, 0.7}, {0.7, 0.7}}) {
      const gp_Pnt2d uv(u0 + (u1 - u0) * su, v0 + (v1 - v0) * sv);
      if (BRepClass_FaceClassifier(face, uv, 1.0e-7).State() != TopAbs_IN) {
        continue;
      }
      BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(surface.Value(uv.X(), uv.Y())).Vertex(),
                                          loft_faces);
      CHECK(distance.IsDone());
      if (!distance.IsDone()) {
        continue;
      }
      ++measured;
      bool at_edge = false;
      for (int s = 1; s <= distance.NbSolution(); ++s) {
        at_edge = at_edge || distance.SupportTypeShape2(s) != BRepExtrema_IsInFace;
      }
      const double tolerance = at_edge ? kJoin : kWall;
      bool matched = false;
      for (std::size_t k = 0; k < allowed.size(); ++k) {
        if (std::abs(distance.Value() - allowed[k]) <= tolerance) {
          matched = true;
          found.insert(k);
        }
      }
      if (!matched) {
        std::printf("%s: a face of the shell lies %.9f from the loft (nearest %s)\n", what, distance.Value(),
                    at_edge ? "an edge of it" : "inside a face of it");
        CHECK(false);
      }
    }
  }
  CHECK(measured > 0);
  for (const double d : needed) {
    const auto at = std::find(allowed.begin(), allowed.end(), d);
    const bool seen = at != allowed.end() && found.count(static_cast<std::size_t>(at - allowed.begin())) > 0;
    if (!seen) {
      std::printf("%s: no face of the shell lies %g from the loft\n", what, d);
    }
    CHECK(seen);
  }
}

enum class Direction { Inside, Outside, Both };

const char* words(Direction direction) {
  switch (direction) {
  case Direction::Inside:
    return "Inside";
  case Direction::Outside:
    return "Outside";
  case Direction::Both:
    return "Both";
  }
  return "";
}

// The body shelled with the face `removed` open: 2 mm Inside, 2 mm
// Outside, or Both (2 mm in, 1 mm out), Sharp or Rounded; a valid single
// solid of positive volume without a note whose walls lie at the
// thicknesses, the body left as it was.
void check_shell(const std::string& what, const g::ShapePtr& body, const std::string& removed, Direction direction,
                 bool rounded) {
  const std::string name = what + ", " + words(direction) + (rounded ? ", Rounded" : ", Sharp");
  const double before = volume_of(body->occt());
  const std::vector<int> removed_faces = body->find_faces(removed);
  CHECK(removed_faces.size() == 1);
  if (removed_faces.size() != 1) {
    return;
  }
  const BRepAdaptor_Surface end(body->face(removed_faces.front()));
  CHECK(end.GetType() == GeomAbs_Plane);
  if (end.GetType() != GeomAbs_Plane) {
    return;
  }
  const double inside = direction == Direction::Outside ? 0.0 : 2.0;
  const double outside = direction == Direction::Inside ? 0.0 : direction == Direction::Outside ? 2.0 : 1.0;
  g::ShellSpec spec;
  spec.faces = {removed};
  spec.inside = inside;
  spec.outside = outside;
  spec.rounded = rounded;
  g::ShapePtr result;
  try {
    result = g::shell("F4", *body, spec);
  } catch (const std::exception& error) {
    std::printf("%s: %s\n", name.c_str(), error.what());
  }
  CHECK(result != nullptr);
  if (!result) {
    // What the kernel's offset built of each wall (mitcad#2271).
    if (inside > 0.0) {
      describe_kernel_offset(name, *body, removed, -inside, rounded);
    }
    if (outside > 0.0) {
      describe_kernel_offset(name, *body, removed, outside, rounded);
    }
    return;
  }
  CHECK(BRepCheck_Analyzer(result->occt()).IsValid());
  CHECK(count(result->occt(), TopAbs_SOLID) == 1);
  CHECK(result->notes().empty());
  CHECK(near(volume_of(body->occt()), before, 1e-9));
  if (inside > 0.0) {
    const bool closes = kernel_closes(*body, removed, -inside, rounded);
    if (!closes) {
      std::printf("%s: the kernel's offset of the inside wall does not close\n", name.c_str());
    }
    CHECK(closes);
  }
  const double volume = volume_of(result->occt());
  std::printf("%s: %d faces, volume %.6f of the loft's %.6f\n", name.c_str(), result->face_count(), volume, before);
  // A wall has a volume: an inside-out solid or a face that does not bound
  // what it should shows as none (mitcad#2271).
  CHECK(volume > 0.0);
  if (!(volume > 0.0)) {
    describe(name, result->occt());
  }
  switch (direction) {
  case Direction::Inside:
    check_walls(name.c_str(), *body, end.Plane(), *result, {0.0, inside}, {inside});
    break;
  case Direction::Outside:
    check_walls(name.c_str(), *body, end.Plane(), *result, {0.0, outside}, {outside});
    break;
  case Direction::Both:
    check_walls(name.c_str(), *body, end.Plane(), *result, {inside, outside}, {inside, outside});
    break;
  }
}

// The loft with Tangent Edges Keep or Merge: a valid single solid (the
// merged side one face on a periodic surface), shelled each way, Sharp and
// Rounded, with the first section's end removed.
void check_loft(const char* what, g::LoftSpec spec, bool merge) {
  spec.merge_tangent = merge;
  const std::string name = std::string(what) + (merge ? ", Merge" : ", Keep");
  g::ShapePtr body;
  try {
    body = g::loft(spec);
  } catch (const std::exception& error) {
    std::printf("%s: %s\n", name.c_str(), error.what());
  }
  CHECK(body != nullptr);
  if (!body) {
    return;
  }
  CHECK(BRepCheck_Analyzer(body->occt()).IsValid());
  CHECK(count(body->occt(), TopAbs_SOLID) == 1);
  if (merge) {
    check_merged_side(name.c_str(), *body);
  }
  const std::string removed = g::face_name("F3", "start", spec.sections.front().region.name);
  for (const Direction direction : {Direction::Inside, Direction::Outside, Direction::Both}) {
    for (const bool rounded : {false, true}) {
      // With Keep, a circle to a spline outline (one edge each) is OCCT's
      // through-sections loft, whose clamped closed side the kernel does
      // not offset outward Sharp (mitcad#2308): skipped until that is done.
      if (!merge && spec.sections.back().region.loops.front().segments.front().kind == g::SegmentKind::BSpline &&
          direction != Direction::Inside && !rounded) {
        std::printf("%s, %s, Sharp: skipped, the outward Sharp offset of a Keep one-edge loft is mitcad#2308\n",
                    name.c_str(), words(direction));
        continue;
      }
      check_shell(name, body, removed, direction, rounded);
    }
  }
}

// The surface loft thickened by `thickness` (to the other side when
// negative, half each side with `symmetric`): a valid single solid.
// Returns its volume, or -1.
double check_thicken(const std::string& what, const g::ShapePtr& sheet, double thickness, bool symmetric) {
  g::ShapePtr thick;
  try {
    thick = g::thicken("F9", *sheet, thickness, symmetric);
  } catch (const std::exception& error) {
    std::printf("%s: %s\n", what.c_str(), error.what());
  }
  CHECK(thick != nullptr);
  if (!thick) {
    return -1.0;
  }
  CHECK(BRepCheck_Analyzer(thick->occt()).IsValid());
  CHECK(count(thick->occt(), TopAbs_SOLID) == 1);
  // Beside it the kernel's measure of the whole solid to 1e-9, with its
  // own error and about the solid's middle (the far side's faces are
  // offset surfaces, which the integration takes without their knots).
  const double volume = volume_of(thick->occt());
  const auto [whole, error] = volume_and_error(thick->occt());
  std::printf("%s: volume %.6f (whole %.6f, error %.1e, %.6f about its middle), %d faces\n", what.c_str(), volume,
              whole, error, volume_about_middle(thick->occt()), thick->face_count());
  return volume;
}

// The owner's loft as a surface, Keep or Merge, thickened 1 mm along its
// normals, against them and symmetric. The volume between a surface of
// area A and its offset by t is t A + t^2 Int H + t^3 / 3 Int K: the two
// sides sum to 2 t A + 2/3 t^3 Int K, symmetric is t A + t^3 / 12 Int K
// (the t^3 terms a few mm^3 against some 10^5 here).
void check_thickened_owner_surface(bool merge) {
  g::LoftSpec spec = owner_loft();
  spec.solid = false;
  spec.merge_tangent = merge;
  const std::string name = std::string("the owner's surface loft, ") + (merge ? "Merge" : "Keep");
  g::ShapePtr sheet;
  try {
    sheet = g::loft(spec);
  } catch (const std::exception& error) {
    std::printf("%s: %s\n", name.c_str(), error.what());
  }
  CHECK(sheet != nullptr);
  if (!sheet) {
    return;
  }
  CHECK(count(sheet->occt(), TopAbs_SOLID) == 0);
  const double area = area_of(sheet->occt());
  const double front = check_thicken(name + ", 1 mm along the normals", sheet, 1.0, false);
  const double back = check_thicken(name + ", 1 mm against the normals", sheet, -1.0, false);
  const double both = check_thicken(name + ", 1 mm symmetric", sheet, 1.0, true);
  CHECK(front > 0.0 && back > 0.0 && both > 0.0);
  CHECK(near(area_of(sheet->occt()), area, 1e-9));
  if (front > 0.0 && back > 0.0) {
    CHECK_NEAR_TOL(front + back, 2.0 * area, 1e-3);
  }
  if (both > 0.0) {
    CHECK_NEAR_TOL(both, area, 1e-3);
  }
  if (!near(front + back, 2.0 * area, 1e-3) || !near(both, area, 1e-3)) {
    // The sheet's faces, to tell a measurement that is off from a wall
    // that is (mitcad#2271).
    describe(name + ", the sheet", sheet->occt());
  }
}

// The lofts: circles to slots of several proportions, an ellipse to a
// rounded rectangle, a circle to a spline outline.
g::LoftSpec circle_slot_r30() { return loft_xy(circle(30), slot(60, 20), 50); }
g::LoftSpec circle_slot_r50() { return loft_xz(circle(50), slot(73, 21), 100); }
// The long slot 150 mm above its circle: at 60 mm its sides met its end at
// some 21 degrees, where a 2 mm inside wall moves the inner end face's
// outline 11 mm in from the slot's tip, past the 10 mm of its end arcs, so
// that the outline crosses itself (BRepCheck_SelfIntersectingWire) and
// part of that face would have to vanish, which the kernel's offset cannot
// do; that is not what this case is about (mitcad#2271). At 150 mm the
// angle is 43 degrees and the outline moves 5 mm.
g::LoftSpec circle_slot_long() { return loft_xy(circle(40), slot(190, 10), 150); }
g::LoftSpec ellipse_rounded_rectangle() { return loft_xy(ellipse(60, 30), rounded_rectangle(140, 80, 15), 70); }
g::LoftSpec circle_spline() { return loft_xy(circle(25), spline_outline(), 60); }

// One ctest: a loft shelled every way with Keep or Merge, or (no loft)
// the owner's surface loft thickened.
struct Part {
  const char* name;
  const char* what;
  g::LoftSpec (*loft)();
  bool merge;

  void run() const {
    guarded(name, [this] {
      if (loft != nullptr) {
        check_loft(what, loft(), merge);
      } else {
        check_thickened_owner_surface(merge);
      }
    });
  }
};

const Part kParts[] = {
    {"circle_slot_r30_keep", "R30 to 160 x 40", circle_slot_r30, false},
    {"circle_slot_r30_merge", "R30 to 160 x 40", circle_slot_r30, true},
    {"circle_slot_r50_keep", "R50 to 188 x 42", circle_slot_r50, false},
    {"circle_slot_r50_merge", "R50 to 188 x 42", circle_slot_r50, true},
    {"circle_slot_owner_keep", "the owner's R111 to 520 x 80", owner_loft, false},
    {"circle_slot_owner_merge", "the owner's R111 to 520 x 80", owner_loft, true},
    {"circle_slot_long_keep", "R40 to 400 x 20", circle_slot_long, false},
    {"circle_slot_long_merge", "R40 to 400 x 20", circle_slot_long, true},
    {"ellipse_rounded_rectangle_keep", "ellipse to rounded rectangle", ellipse_rounded_rectangle, false},
    {"ellipse_rounded_rectangle_merge", "ellipse to rounded rectangle", ellipse_rounded_rectangle, true},
    {"circle_spline_keep", "circle to spline outline", circle_spline, false},
    {"circle_spline_merge", "circle to spline outline", circle_spline, true},
    {"thicken_owner_keep", "the owner's surface loft thickened", nullptr, false},
    {"thicken_owner_merge", "the owner's surface loft thickened", nullptr, true},
};

const Part* find_part(const std::string& name) {
  for (const Part& part : kParts) {
    if (name == part.name) {
      return &part;
    }
  }
  return nullptr;
}

// The ctests' parts (loft_smooth_sections_shell_parts in
// geometry/CMakeLists.txt) are this test's parts, each once, so that no
// case goes unrun.
int check_parts(const std::vector<std::string>& names) {
  int wrong = 0;
  std::set<std::string> seen;
  for (const std::string& name : names) {
    if (find_part(name) == nullptr) {
      std::fprintf(stderr, "loft_smooth_sections_shell_parts names %s, which is no part of this test\n",
                   name.c_str());
      ++wrong;
    } else if (!seen.insert(name).second) {
      std::fprintf(stderr, "loft_smooth_sections_shell_parts names %s twice\n", name.c_str());
      ++wrong;
    }
  }
  for (const Part& part : kParts) {
    if (seen.count(part.name) == 0) {
      std::fprintf(stderr,
                   "the part %s is not in loft_smooth_sections_shell_parts (geometry/CMakeLists.txt), so no "
                   "ctest runs it\n",
                   part.name);
      ++wrong;
    }
  }
  if (wrong == 0) {
    std::printf("test_loft_smooth_sections_shell: the %zu parts are the ctests' parts\n", seen.size());
  }
  return wrong == 0 ? 0 : 1;
}

} // namespace
} // namespace test

int main(int argc, char** argv) {
  const std::vector<std::string> args(argv + 1, argv + argc);
  if (!args.empty() && args[0] == "--check-parts") {
    return test::check_parts(std::vector<std::string>(args.begin() + 1, args.end()));
  }
  if (args.size() > 1 || (args.size() == 1 && test::find_part(args[0]) == nullptr)) {
    std::fprintf(stderr, "usage: test_loft_smooth_sections_shell [PART | --check-parts PART...]; the parts:");
    for (const test::Part& part : test::kParts) {
      std::fprintf(stderr, " %s", part.name);
    }
    std::fprintf(stderr, "\n");
    return 2;
  }
  for (const test::Part& part : test::kParts) {
    if (args.empty() || args[0] == part.name) {
      part.run();
    }
  }
  if (test::failures == 0) {
    std::puts("test_loft_smooth_sections_shell: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
