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
// With Keep, two sections of one closed edge each (a circle to a spline
// outline) are still OCCT's through-sections loft; its side, the same
// clamped surface closing on itself, was not offset outward Sharp (Shell
// Outside and Both). That face is now put on its surface made periodic
// where it closes smoothly (geometry/src/loft_skin.cpp,
// periodic_closed_sides; mitcad#2271, mitcad#2308): the loft is the
// kernel's own through-sections loft within 1e-6 mm, with the same faces
// and names. The merged circle to spline outline is C2 all round, but its
// side was kept only C1 where it closes and where a ring was cut to start,
// so a symmetric Thicken's half offset was C0 along those lines, which the
// kernel's offset refuses; the side now keeps its smoothness there
// (geometry/src/loft_skin.cpp, periodic_where_closed and smooth_where_c2).
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
// back on). The merged side, and the kept one through sections of one
// edge each, is one face on a periodic surface. The owner's surface lofts
// and the circle to spline outline as a surface, Keep and Merge, thicken
// to either side and symmetric into valid single solids.
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
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepExtrema_SupportType.hxx>
#include <BRepGProp.hxx>
#include <BRepOffset_Error.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <BRepOffsetAPI_MakeThickSolid.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <GeomAbs_Shape.hxx>
#include <GeomAbs_SurfaceType.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_OffsetSurface.hxx>
#include <Geom_Surface.hxx>
#include <NCollection_List.hxx>
#include <ShapeUpgrade_ShapeDivideClosed.hxx>
#include <ShapeUpgrade_ShapeDivideContinuity.hxx>
#include <Standard_Failure.hxx>
#include <Standard_Type.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Iterator.hxx>
#include <TopoDS_Shell.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt2d.hxx>

#include "check.hpp"
#include "mitcad/geometry/faceops.hpp"
#include "mitcad/geometry/loft.hpp"
#include "mitcad/geometry/naming.hpp"
#include "mitcad/geometry/profile.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/surfaces.hpp"
#include "bundled_test.hpp"

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

// Merged, or kept through sections of one edge each: the side is one face,
// on a periodic surface continuous in its first derivative (mitcad#2271,
// mitcad#2308).
void check_round_side(const char* what, const g::Shape& body) {
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

// The sections are one closed edge each (a circle, an ellipse, a closed
// spline).
bool one_edge_each(const g::LoftSpec& spec) {
  for (const g::LoftSection& section : spec.sections) {
    if (section.region.loops.size() != 1 || section.region.loops.front().segments.size() != 1) {
      return false;
    }
  }
  return true;
}

// The faces of the shape that carry the name.
int faces_called(const g::Shape& shape, const std::string& name) {
  int n = 0;
  for (int i = 0; i < shape.face_count(); ++i) {
    const g::NameList& names = shape.face_names(i);
    n += std::find(names.begin(), names.end(), name) != names.end() ? 1 : 0;
  }
  return n;
}

// How far the points of `from` at a grid over its parameter box (those
// not outside it) lie from the face `to` at most.
double farthest(const TopoDS_Face& from, const TopoDS_Face& to) {
  double u0 = 0.0;
  double u1 = 0.0;
  double v0 = 0.0;
  double v1 = 0.0;
  BRepTools::UVBounds(from, u0, u1, v0, v1);
  const BRepAdaptor_Surface surface(from);
  double largest = 0.0;
  for (int a = 0; a <= 8; ++a) {
    for (int b = 0; b <= 4; ++b) {
      const gp_Pnt2d uv(u0 + (u1 - u0) * a / 8, v0 + (v1 - v0) * b / 4);
      if (BRepClass_FaceClassifier(from, uv, 1.0e-7).State() == TopAbs_OUT) {
        continue;
      }
      BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(surface.Value(uv.X(), uv.Y())).Vertex(), to);
      CHECK(distance.IsDone());
      if (distance.IsDone()) {
        largest = std::max(largest, distance.Value());
      }
    }
  }
  return largest;
}

// Kept through sections of one edge each (mitcad#2308): the loft is the
// kernel's through-sections loft of the two outlines as it was, its side
// only put on its surface made periodic: the kernel's own loft's side and
// this one lie on each other within 1e-6 mm, the volumes agree, and the
// faces keep their names (the side after the first section's edge, the
// ends after the regions).
void check_as_through_sections(const std::string& what, const g::LoftSpec& spec, const g::Shape& body) {
  BRepOffsetAPI_ThruSections kernel(true, spec.ruled, 1.0e-6);
  kernel.CheckCompatibility(true);
  kernel.SetMutableInput(false);
  for (const g::LoftSection& section : spec.sections) {
    kernel.AddWire(BRepTools::OuterWire(g::make_profile(section.frame, section.region).face));
  }
  kernel.Build();
  CHECK(kernel.IsDone());
  if (!kernel.IsDone()) {
    return;
  }
  const TopoDS_Shape& theirs = kernel.Shape();
  CHECK(body.face_count() == count(theirs, TopAbs_FACE));
  const double ours = std::abs(volume_of(body.occt()));
  const double kernels = std::abs(volume_of(theirs));
  std::printf("%s: volume %.6f, the kernel's through-sections loft %.6f\n", what.c_str(), ours, kernels);
  CHECK(near(ours, kernels, 1e-6));
  std::optional<TopoDS_Face> their_side;
  for (TopExp_Explorer it(theirs, TopAbs_FACE); it.More(); it.Next()) {
    if (BRepAdaptor_Surface(TopoDS::Face(it.Current())).GetType() != GeomAbs_Plane) {
      CHECK(!their_side);
      their_side = TopoDS::Face(it.Current());
    }
  }
  const std::vector<TopoDS_Face> sides = side_faces(body);
  CHECK(their_side.has_value() && sides.size() == 1);
  if (their_side && sides.size() == 1) {
    const double off = std::max(farthest(*their_side, sides.front()), farthest(sides.front(), *their_side));
    std::printf("%s: the side lies %.2e mm from the kernel's\n", what.c_str(), off);
    CHECK(off <= 1.0e-6);
  }
  const std::string edge = spec.sections.front().region.loops.front().segments.front().name;
  CHECK(faces_called(body, g::face_name("F3", "side", edge)) == 1);
  CHECK(faces_called(body, g::face_name("F3", "start", spec.sections.front().region.name)) == 1);
  CHECK(faces_called(body, g::face_name("F3", "end", spec.sections.back().region.name)) == 1);
}

// The loft with Tangent Edges Keep or Merge: a valid single solid (the
// merged side, and a kept one through sections of one edge each, one face
// on a periodic surface), shelled each way, Sharp and Rounded, with the
// first section's end removed.
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
  if (merge || one_edge_each(spec)) {
    check_round_side(name.c_str(), *body);
  }
  if (!merge && one_edge_each(spec)) {
    check_as_through_sections(name, spec, *body);
  }
  const std::string removed = g::face_name("F3", "start", spec.sections.front().region.name);
  for (const Direction direction : {Direction::Inside, Direction::Outside, Direction::Both}) {
    for (const bool rounded : {false, true}) {
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

const char* continuity_words(GeomAbs_Shape shape) {
  switch (shape) {
  case GeomAbs_C0:
    return "C0";
  case GeomAbs_G1:
    return "G1";
  case GeomAbs_C1:
    return "C1";
  case GeomAbs_G2:
    return "G2";
  case GeomAbs_C2:
    return "C2";
  case GeomAbs_C3:
    return "C3";
  case GeomAbs_CN:
    return "CN";
  }
  return "?";
}

const char* offset_error_words(BRepOffset_Error error) {
  switch (error) {
  case BRepOffset_NoError:
    return "no error";
  case BRepOffset_UnknownError:
    return "unknown error";
  case BRepOffset_BadNormalsOnGeometry:
    return "bad normals on the geometry";
  case BRepOffset_C0Geometry:
    return "a C0 surface";
  case BRepOffset_NullOffset:
    return "null offset";
  case BRepOffset_NotConnectedShell:
    return "not a connected shell";
  case BRepOffset_CannotTrimEdges:
    return "cannot trim edges";
  case BRepOffset_CannotFuseVertices:
    return "cannot fuse vertices";
  case BRepOffset_CannotExtentEdge:
    return "cannot extend an edge";
  case BRepOffset_UserBreak:
    return "user break";
  case BRepOffset_MixedConnectivity:
    return "mixed connectivity";
  }
  return "?";
}

// Each face of a sheet as the kernel's offset takes it: its surface (an
// offset one with its basis), the continuity it reports, whether it is
// periodic and whether the face closes on itself (a seam edge on it).
void describe_offset_faces(const std::string& what, const TopoDS_Shape& shape) {
  int index = 0;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next(), ++index) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face);
    if (surface.IsNull()) {
      std::printf("%s: face %d without a surface\n", what.c_str(), index);
      continue;
    }
    const occ::handle<Geom_OffsetSurface> offset = occ::down_cast<Geom_OffsetSurface>(surface);
    const occ::handle<Geom_Surface> basis = offset.IsNull() ? surface : offset->BasisSurface();
    bool seam = false;
    for (TopExp_Explorer e(face, TopAbs_EDGE); e.More() && !seam; e.Next()) {
      seam = BRep_Tool::IsClosed(TopoDS::Edge(e.Current()), face);
    }
    double u0 = 0.0;
    double u1 = 0.0;
    double v0 = 0.0;
    double v1 = 0.0;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    std::printf("%s: face %d, %s%s, %s (basis %s%s), %s, u %.6f to %.6f, v %.6f to %.6f\n", what.c_str(), index,
                offset.IsNull() ? "" : "offset of ", basis->DynamicType()->Name(),
                continuity_words(surface->Continuity()), continuity_words(basis->Continuity()),
                basis->IsUPeriodic() ? ", U periodic" : "", seam ? "closed on itself" : "not closed", u0, u1, v0,
                v1);
  }
}

// The kernel's offset of (a copy of) the sheet as thicken asks it, in
// Skin mode with sharp joins: the result, or none with the kernel's error
// printed.
std::optional<TopoDS_Shape> traced_offset(const std::string& what, const TopoDS_Shape& sheet, double distance,
                                          bool thickening) {
  TopoDS_Shape input = BRepBuilderAPI_Copy(sheet).Shape();
  if (input.ShapeType() == TopAbs_COMPOUND) {
    TopoDS_Iterator it(input);
    if (it.More()) {
      const TopoDS_Shape only = it.Value();
      it.Next();
      if (!it.More()) {
        input = only;
      }
    }
  }
  try {
    BRepOffset_MakeOffset maker;
    maker.Initialize(input, distance, 1.0e-6, BRepOffset_Skin, false, false, GeomAbs_Intersection, thickening);
    maker.MakeOffsetShape();
    std::printf("%s: the kernel's offset by %g%s: %s\n", what.c_str(), distance, thickening ? ", thickening" : "",
                maker.IsDone() ? "done" : offset_error_words(maker.Error()));
    if (maker.IsDone()) {
      return maker.Shape();
    }
  } catch (const Standard_Failure& failure) {
    std::printf("%s: the kernel's offset by %g raised: %s\n", what.c_str(), distance, failure.what());
  }
  return std::nullopt;
}

// Where a symmetric thicken of the sheet stops (mitcad#2308), as
// geometry/src/surfaces.cpp's thicken goes: how smooth the sheet's
// periodic surfaces are where they close (whether their knot there comes
// out once more within 1e-7 to 1e-4 mm), the half offset each way, its
// faces, and the whole offset of it with thickening as it is, divided
// across its seams and split at its creases (split_at_creases), each with
// the kernel's own error.
void trace_symmetric(const std::string& what, const g::Shape& sheet, double thickness) {
  for (int i = 0; i < sheet.face_count(); ++i) {
    const occ::handle<Geom_BSplineSurface> spline =
        occ::down_cast<Geom_BSplineSurface>(BRep_Tool::Surface(sheet.face(i)));
    if (spline.IsNull() || !spline->IsUPeriodic()) {
      continue;
    }
    const int closing = spline->FirstUKnotIndex();
    std::printf("%s: face %d, U degree %d, %s, its knot where it closes %d times\n", what.c_str(), i,
                spline->UDegree(), continuity_words(spline->Continuity()), spline->UMultiplicity(closing));
    for (const double tolerance : {1.0e-7, 1.0e-6, 1.0e-5, 1.0e-4}) {
      const occ::handle<Geom_BSplineSurface> copy = occ::down_cast<Geom_BSplineSurface>(spline->Copy());
      const int fewer = copy->UMultiplicity(closing) - 1;
      const bool removed = fewer >= 0 && copy->RemoveUKnot(closing, fewer, tolerance);
      std::printf("%s: face %d, its knot where it closes once less within %g mm: %s\n", what.c_str(), i,
                  tolerance, removed ? "yes" : "no");
    }
    // Every knot along U at which the surface is less than C2 (more than
    // degree - 2 times): where, how often, and whether it comes out once
    // more; and the V knots so (mitcad#2308: the closure C2, the surface
    // was still C1).
    const int degree = spline->UDegree();
    for (int k = spline->FirstUKnotIndex(); k <= spline->LastUKnotIndex(); ++k) {
      const int times = spline->UMultiplicity(k);
      if (times <= degree - 2) {
        continue;
      }
      std::string removable;
      for (const double tolerance : {1.0e-7, 1.0e-6, 1.0e-5, 1.0e-4}) {
        const occ::handle<Geom_BSplineSurface> copy = occ::down_cast<Geom_BSplineSurface>(spline->Copy());
        const bool removed = copy->RemoveUKnot(k, times - 1, tolerance);
        removable += std::string(removable.empty() ? "" : ", ") + (removed ? "yes" : "no");
      }
      std::printf("%s: face %d, U knot %d at %.9f of %.9f to %.9f, %d times; once less within 1e-7, 1e-6, "
                  "1e-5, 1e-4 mm: %s\n",
                  what.c_str(), i, k, spline->UKnot(k), spline->UKnot(spline->FirstUKnotIndex()),
                  spline->UKnot(spline->LastUKnotIndex()), times, removable.c_str());
    }
    for (int k = spline->FirstVKnotIndex() + 1; k < spline->LastVKnotIndex(); ++k) {
      if (spline->VMultiplicity(k) > spline->VDegree() - 2) {
        std::printf("%s: face %d, V knot %d at %.9f, %d times at V degree %d\n", what.c_str(), i, k,
                    spline->VKnot(k), spline->VMultiplicity(k), spline->VDegree());
      }
    }
    // The same surface with every such U knot once less where that moves
    // it by no more than 1e-7 mm, as a face of its own: its continuity, and
    // whether the kernel offsets it by half the thickness and that again
    // with thickening (would smoothing those knots do?).
    const occ::handle<Geom_BSplineSurface> smoothed = occ::down_cast<Geom_BSplineSurface>(spline->Copy());
    for (int k = smoothed->FirstUKnotIndex(); k <= smoothed->LastUKnotIndex(); ++k) {
      const int times = smoothed->UMultiplicity(k);
      if (times > degree - 2) {
        smoothed->RemoveUKnot(k, times - 1, 1.0e-7);
      }
    }
    std::printf("%s: face %d smoothed within 1e-7 mm: %s\n", what.c_str(), i,
                continuity_words(smoothed->Continuity()));
    const BRepBuilderAPI_MakeFace made(smoothed, 1.0e-7);
    if (made.IsDone()) {
      BRep_Builder builder;
      TopoDS_Shell shell;
      builder.MakeShell(shell);
      builder.Add(shell, made.Face());
      const std::string at = what + ", smoothed";
      const std::optional<TopoDS_Shape> back = traced_offset(at + ", half offset", shell, -thickness / 2, false);
      if (back) {
        describe_offset_faces(at + ", half offset", *back);
        traced_offset(at + ", half offset", *back, thickness, true);
      }
    }
  }
  describe_offset_faces(what + ", the sheet", sheet.occt());
  const double half = thickness / 2;
  for (const double d : {-half, half}) {
    const std::string at = what + ", half offset " + (d < 0 ? "against" : "along") + " the normals";
    const std::optional<TopoDS_Shape> back = traced_offset(at, sheet.occt(), d, false);
    if (!back) {
      continue;
    }
    describe_offset_faces(at, *back);
    traced_offset(at + ", as it is", *back, thickness, true);
    traced_offset(at + ", as it is", *back, -thickness, true);
    try {
      ShapeUpgrade_ShapeDivideClosed divide(*back);
      divide.SetNbSplitPoints(1);
      const bool divided = divide.Perform();
      std::printf("%s: divided across its seams: %s\n", at.c_str(), divided ? "yes" : "no");
      if (divided && !divide.Result().IsNull()) {
        describe_offset_faces(at + ", divided", divide.Result());
        traced_offset(at + ", divided", divide.Result(), thickness, true);
        traced_offset(at + ", divided", divide.Result(), -thickness, true);
      }
      ShapeUpgrade_ShapeDivideContinuity creases(*back);
      creases.SetSurfaceCriterion(GeomAbs_C1);
      const bool split = creases.Perform();
      std::printf("%s: split at its creases: %s\n", at.c_str(), split ? "yes" : "no");
      if (split && !creases.Result().IsNull()) {
        describe_offset_faces(at + ", split", creases.Result());
        traced_offset(at + ", split", creases.Result(), thickness, true);
      }
    } catch (const Standard_Failure& failure) {
      std::printf("%s: dividing raised: %s\n", at.c_str(), failure.what());
    }
  }
}

// The loft as a surface, Keep or Merge, thickened 1 mm along its normals,
// against them and symmetric. The volume between a surface of area A and
// its offset by t is t A + t^2 Int H + t^3 / 3 Int K: the two sides sum to
// 2 t A + 2/3 t^3 Int K, symmetric is t A + t^3 / 12 Int K (the t^3 terms
// a few mm^3 against some 10^4 to 10^5 here).
void check_thickened_surface(const char* what, g::LoftSpec spec, bool merge) {
  spec.solid = false;
  spec.merge_tangent = merge;
  const std::string name = std::string(what) + (merge ? ", Merge" : ", Keep");
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
  if (one_edge_each(spec)) {
    check_round_side(name.c_str(), *sheet);
  }
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
  if (!(both > 0.0)) {
    // Where the symmetric thicken stops, step by step (mitcad#2308).
    trace_symmetric(name + ", 1 mm symmetric", *sheet, 1.0);
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

// One ctest: a loft shelled every way with Keep or Merge, or the loft
// as a surface thickened (`thickened`).
struct Part {
  const char* name;
  const char* what;
  g::LoftSpec (*loft)();
  bool merge;
  bool thickened;

  void run() const {
    guarded(name, [this] {
      if (thickened) {
        check_thickened_surface(what, loft(), merge);
      } else {
        check_loft(what, loft(), merge);
      }
    });
  }
};

const Part kParts[] = {
    {"circle_slot_r30_keep", "R30 to 160 x 40", circle_slot_r30, false, false},
    {"circle_slot_r30_merge", "R30 to 160 x 40", circle_slot_r30, true, false},
    {"circle_slot_r50_keep", "R50 to 188 x 42", circle_slot_r50, false, false},
    {"circle_slot_r50_merge", "R50 to 188 x 42", circle_slot_r50, true, false},
    {"circle_slot_owner_keep", "the owner's R111 to 520 x 80", owner_loft, false, false},
    {"circle_slot_owner_merge", "the owner's R111 to 520 x 80", owner_loft, true, false},
    {"circle_slot_long_keep", "R40 to 400 x 20", circle_slot_long, false, false},
    {"circle_slot_long_merge", "R40 to 400 x 20", circle_slot_long, true, false},
    {"ellipse_rounded_rectangle_keep", "ellipse to rounded rectangle", ellipse_rounded_rectangle, false, false},
    {"ellipse_rounded_rectangle_merge", "ellipse to rounded rectangle", ellipse_rounded_rectangle, true, false},
    {"circle_spline_keep", "circle to spline outline", circle_spline, false, false},
    {"circle_spline_merge", "circle to spline outline", circle_spline, true, false},
    {"thicken_owner_keep", "the owner's surface loft", owner_loft, false, true},
    {"thicken_owner_merge", "the owner's surface loft", owner_loft, true, true},
    {"thicken_circle_spline_keep", "the circle to spline outline surface loft", circle_spline, false, true},
    {"thicken_circle_spline_merge", "the circle to spline outline surface loft", circle_spline, true, true},
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

MITCAD_BUNDLED_TEST_ARGS(loft_smooth_sections_shell) {
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
