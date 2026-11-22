// SPDX-License-Identifier: MIT
// Edges inside one smooth surface (mitcad#1773, smooth_seams.hpp): the
// views leave them out, every other edge stays.
//
// - A box: none of its 12 edges.
// - A cylinder: its seam, not its two circles. A sphere: its seam, not its
//   degenerated poles.
// - A box with a round hole through it: the hole's seam only.
// - A plate whose four upright edges are rounded: none of its 24 edges
//   (the roundings' boundaries meet the sides tangentially, on other
//   surfaces).
// - A teardrop (one closed B-spline with a sharp tip) extruded: none, its
//   seam is the crease at the tip. A closed smooth (periodic) spline and a
//   circle extruded: the seam.
// - Two faces on one plane sharing an edge: that edge; on two planes at
//   the same place: none.
// - Two boxes side by side fused (coplanar faces left on their own
//   planes): none without names; their top faces named as two pieces of
//   one face: the edge between them; named as two faces: none. Named as
//   the pieces of a mirrored copy (`F4:inst1(F1:top#0)`): the edge too.
// - unsplit_face_name drops the piece numbers wherever they stand.
//
// Pieces on different analytic surfaces (mitcad#2148):
// - A circle piped along a line and a tangent arc: its side in two pieces,
//   a cylinder and a torus; the circle between them is drawn, their two
//   seams are not (3 of 5 edges drawn). Along a line, an arc and a line:
//   both circles between the pieces (4 of 7). Along two arcs of one
//   circle: one torus, no line between its pieces (2 of 5). Along two
//   collinear lines: one cylinder (2 drawn).
// - The line-and-arc pipe mirrored, its faces named as a mirror's copy
//   (`F4:inst1(F2:side0#1)`): the same edges as the original.
// - different_analytic_surfaces: planes, cylinders, cones, spheres and
//   tori stored with other reference frames, their axes the other way or
//   their sizes off by noise below the tolerance are one surface; other
//   values or types are different; a B-spline is never judged.
// - smooth_surface_groups: the faces model_render takes as one surface.

#include <cmath>
#include <cstdint>
#include <set>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <GeomConvert.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_ConicalSurface.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Geom_RectangularTrimmedSurface.hxx>
#include <Geom_SphericalSurface.hxx>
#include <Geom_ToroidalSurface.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <gp_Ax3.hxx>
#include <gp_Trsf.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_Plane.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_HArray1.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Pln.hxx>

#include "check.hpp"
#include "mitcad/geometry/smooth_seams.hpp"

namespace test {
namespace {

using mitcad::geometry::ShapeMap;
using mitcad::geometry::smooth_seams;

int edge_count(const TopoDS_Shape& shape) {
  ShapeMap edges;
  TopExp::MapShapes(shape, TopAbs_EDGE, edges);
  return edges.Extent();
}

// A face extruded 10 mm up from a closed wire in the XY plane.
TopoDS_Shape extruded(const TopoDS_Edge& closed) {
  const TopoDS_Wire wire = BRepBuilderAPI_MakeWire(closed).Wire();
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(wire, true).Face();
  return BRepPrimAPI_MakePrism(face, gp_Vec(0.0, 0.0, 10.0)).Shape();
}

void test_box() {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(20.0, 20.0, 10.0).Shape();
  CHECK(edge_count(box) == 12);
  CHECK(smooth_seams(box).Extent() == 0);
}

void test_cylinder_and_sphere() {
  const TopoDS_Shape cylinder = BRepPrimAPI_MakeCylinder(5.0, 10.0).Shape();
  CHECK(edge_count(cylinder) == 3);
  const ShapeMap seams = smooth_seams(cylinder);
  CHECK(seams.Extent() == 1);
  if (seams.Extent() == 1) {
    // The seam runs up the side, not round it.
    const BRepAdaptor_Curve curve(TopoDS::Edge(seams(1)));
    CHECK(curve.GetType() == GeomAbs_Line);
  }
  const TopoDS_Shape sphere = BRepPrimAPI_MakeSphere(5.0).Shape();
  const ShapeMap meridian = smooth_seams(sphere);
  CHECK(meridian.Extent() == 1);
  if (meridian.Extent() == 1) {
    CHECK(!BRep_Tool::Degenerated(TopoDS::Edge(meridian(1))));
  }
}

void test_hole() {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(20.0, 20.0, 10.0).Shape();
  const TopoDS_Shape tool =
      BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(10.0, 10.0, -1.0), gp::DZ()), 3.0, 12.0).Shape();
  const TopoDS_Shape holed = BRepAlgoAPI_Cut(box, tool).Shape();
  CHECK(edge_count(holed) == 15);
  CHECK(smooth_seams(holed).Extent() == 1);
}

void test_filleted_plate() {
  const TopoDS_Shape plate = BRepPrimAPI_MakeBox(60.0, 40.0, 5.0).Shape();
  BRepFilletAPI_MakeFillet fillet(plate);
  ShapeMap edges;
  TopExp::MapShapes(plate, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    const BRepAdaptor_Curve curve(TopoDS::Edge(edges(i)));
    if (curve.GetType() == GeomAbs_Line && curve.Line().Direction().IsParallel(gp::DZ(), 1e-9)) {
      fillet.Add(4.0, TopoDS::Edge(edges(i)));
    }
  }
  fillet.Build();
  CHECK(fillet.IsDone());
  if (!fillet.IsDone()) {
    return;
  }
  const TopoDS_Shape rounded = fillet.Shape();
  CHECK(edge_count(rounded) == 24);
  CHECK(smooth_seams(rounded).Extent() == 0);
}

void test_teardrop_and_smooth_profiles() {
  // A clamped cubic whose first and last poles are one point: closed, with
  // a sharp tip there.
  NCollection_Array1<gp_Pnt> poles(1, 5);
  poles(1) = gp_Pnt(0.0, 0.0, 0.0);
  poles(2) = gp_Pnt(20.0, 15.0, 0.0);
  poles(3) = gp_Pnt(40.0, 0.0, 0.0);
  poles(4) = gp_Pnt(20.0, -15.0, 0.0);
  poles(5) = gp_Pnt(0.0, 0.0, 0.0);
  NCollection_Array1<double> knots(1, 3);
  knots(1) = 0.0;
  knots(2) = 0.5;
  knots(3) = 1.0;
  NCollection_Array1<int> multiplicities(1, 3);
  multiplicities(1) = 4;
  multiplicities(2) = 1;
  multiplicities(3) = 4;
  const occ::handle<Geom_BSplineCurve> tip = new Geom_BSplineCurve(poles, knots, multiplicities, 3);
  const TopoDS_Shape teardrop = extruded(BRepBuilderAPI_MakeEdge(tip).Edge());
  CHECK(edge_count(teardrop) == 3);
  CHECK(smooth_seams(teardrop).Extent() == 0);

  // A periodic cubic through four points: smooth where it closes.
  occ::handle<NCollection_HArray1<gp_Pnt>> points = new NCollection_HArray1<gp_Pnt>(1, 4);
  points->SetValue(1, gp_Pnt(0.0, 0.0, 0.0));
  points->SetValue(2, gp_Pnt(20.0, 12.0, 0.0));
  points->SetValue(3, gp_Pnt(40.0, 0.0, 0.0));
  points->SetValue(4, gp_Pnt(20.0, -12.0, 0.0));
  GeomAPI_Interpolate interpolate(points, true, 1e-9);
  interpolate.Perform();
  CHECK(interpolate.IsDone());
  if (interpolate.IsDone()) {
    const TopoDS_Shape smooth = extruded(BRepBuilderAPI_MakeEdge(interpolate.Curve()).Edge());
    CHECK(edge_count(smooth) == 3);
    CHECK(smooth_seams(smooth).Extent() == 1);
  }

  const TopoDS_Shape round =
      extruded(BRepBuilderAPI_MakeEdge(gp_Circ(gp_Ax2(gp::Origin(), gp::DZ()), 5.0)).Edge());
  CHECK(edge_count(round) == 3);
  CHECK(smooth_seams(round).Extent() == 1);
}

void test_one_plane() {
  // Vertices first, so that the edges share them and the wires take the
  // edges as they are (the shared edge one edge of both faces).
  const auto vertex = [](double x, double y) { return BRepBuilderAPI_MakeVertex(gp_Pnt(x, y, 0.0)).Vertex(); };
  const TopoDS_Vertex a = vertex(0.0, 0.0);
  const TopoDS_Vertex b = vertex(10.0, 0.0);
  const TopoDS_Vertex c = vertex(10.0, 10.0);
  const TopoDS_Vertex d = vertex(0.0, 10.0);
  const TopoDS_Vertex e = vertex(20.0, 0.0);
  const TopoDS_Vertex f = vertex(20.0, 10.0);
  const auto edge = [](const TopoDS_Vertex& from, const TopoDS_Vertex& to) {
    return BRepBuilderAPI_MakeEdge(from, to).Edge();
  };
  const TopoDS_Edge shared = edge(b, c);
  const TopoDS_Wire left = BRepBuilderAPI_MakeWire(edge(a, b), shared, edge(c, d), edge(d, a)).Wire();
  const TopoDS_Wire right =
      BRepBuilderAPI_MakeWire(edge(b, e), edge(e, f), edge(f, c), TopoDS::Edge(shared.Reversed())).Wire();
  const occ::handle<Geom_Plane> plane = new Geom_Plane(gp_Pln(gp::XOY()));
  const auto pair = [&](const occ::handle<Geom_Plane>& second) {
    TopoDS_Compound both;
    BRep_Builder builder;
    builder.MakeCompound(both);
    builder.Add(both, BRepBuilderAPI_MakeFace(plane, left, true).Face());
    builder.Add(both, BRepBuilderAPI_MakeFace(second, right, true).Face());
    return both;
  };
  const ShapeMap one = smooth_seams(pair(plane));
  CHECK(one.Extent() == 1);
  if (one.Extent() == 1) {
    CHECK(one(1).IsSame(shared));
  }
  const occ::handle<Geom_Plane> other = new Geom_Plane(gp_Pln(gp::XOY()));
  CHECK(smooth_seams(pair(other)).Extent() == 0);
}

void test_named_pieces() {
  const TopoDS_Shape first = BRepPrimAPI_MakeBox(gp_Pnt(0.0, 0.0, 0.0), 10.0, 10.0, 10.0).Shape();
  const TopoDS_Shape second = BRepPrimAPI_MakeBox(gp_Pnt(10.0, 0.0, 0.0), 10.0, 10.0, 10.0).Shape();
  const TopoDS_Shape fused = BRepAlgoAPI_Fuse(first, second).Shape();
  CHECK(smooth_seams(fused).Extent() == 0);
  // The two top faces (at z 10).
  std::vector<TopoDS_Shape> tops;
  for (TopExp_Explorer it(fused, TopAbs_FACE); it.More(); it.Next()) {
    const BRepAdaptor_Surface surface(TopoDS::Face(it.Current()));
    const gp_Pnt middle = surface.Value(0.5 * (surface.FirstUParameter() + surface.LastUParameter()),
                                        0.5 * (surface.FirstVParameter() + surface.LastVParameter()));
    if (surface.GetType() == GeomAbs_Plane && std::abs(middle.Z() - 10.0) < 1e-9) {
      tops.push_back(it.Current());
    }
  }
  CHECK(tops.size() == 2);
  if (tops.size() != 2) {
    return;
  }
  const mitcad::geometry::Shape pieces(fused, {{tops[0], {"F1:top#0"}}, {tops[1], {"F1:top#1"}}});
  CHECK(smooth_seams(pieces.occt(), &pieces).Extent() == 1);
  const mitcad::geometry::Shape apart(fused, {{tops[0], {"F1:top#0"}}, {tops[1], {"F2:top#0"}}});
  CHECK(smooth_seams(apart.occt(), &apart).Extent() == 0);
  // The pieces of a mirror's copy (mitcad#2148): pieces too.
  const mitcad::geometry::Shape copied(fused,
                                       {{tops[0], {"F4:inst1(F1:top#0)"}}, {tops[1], {"F4:inst1(F1:top#1)"}}});
  CHECK(smooth_seams(copied.occt(), &copied).Extent() == 1);
}

void test_unsplit_names() {
  using mitcad::geometry::unsplit_face_name;
  CHECK(unsplit_face_name("F5:fillet(E{F2:a|F2:b})#0") == "F5:fillet(E{F2:a|F2:b})");
  CHECK(unsplit_face_name("F3:side0#0#1") == "F3:side0");
  CHECK(unsplit_face_name("F3:side0") == "F3:side0");
  CHECK(unsplit_face_name("F3:side(c1[c4,c2])") == "F3:side(c1[c4,c2])");
  // Wherever the piece numbers stand (mitcad#2148).
  CHECK(unsplit_face_name("F4:inst1(F2:side(c6[c17,c18])#1)") == "F4:inst1(F2:side(c6[c17,c18]))");
  CHECK(unsplit_face_name("F4:inst1(F2:side(c6)#1)#0") == "F4:inst1(F2:side(c6))");
  CHECK(unsplit_face_name("F7:inst2(F4:inst1(F2:top#3)#1)") == "F7:inst2(F4:inst1(F2:top))");
  // A '#' not followed by a piece number stays.
  CHECK(unsplit_face_name("F3:side#a") == "F3:side#a");
  CHECK(unsplit_face_name("F3:side#") == "F3:side#");
}

using mitcad::geometry::Shape;
using mitcad::geometry::ShapePtr;

constexpr double kPi = 3.14159265358979323846;

mitcad::geometry::PathCurve line_curve(const char* name, const gp_Pnt& a, const gp_Pnt& b) {
  mitcad::geometry::PathCurve piece;
  piece.name = name;
  piece.curve.kind = mitcad::geometry::ModelCurveKind::Line;
  piece.curve.start = a;
  piece.curve.end = b;
  return piece;
}

// An arc in the XY plane about `center`, from the angle `first` to `last`
// measured from `x_axis` towards normal x x_axis (normal +Z).
mitcad::geometry::PathCurve arc_curve(const char* name, const gp_Pnt& center, const gp_Dir& x_axis,
                                      double radius, double first, double last) {
  mitcad::geometry::PathCurve piece;
  piece.name = name;
  mitcad::geometry::ModelCurve& m = piece.curve;
  m.kind = mitcad::geometry::ModelCurveKind::Conic;
  m.center = center;
  m.normal = gp::DZ();
  m.x_axis = x_axis;
  m.major = radius;
  m.minor = radius;
  m.first = first;
  m.last = last;
  return piece;
}

// A circle of diameter 20 piped along the path (faces F2:side0#k, F2:start,
// F2:end).
ShapePtr piped(const mitcad::geometry::Path& path) {
  mitcad::geometry::PipeSpec spec;
  spec.feature = "F2";
  spec.path = path;
  spec.size = 20.0;
  return mitcad::geometry::pipe(spec);
}

// Along +X from the origin 25, then a quarter of radius 30 on to (55, 30).
mitcad::geometry::Path line_and_arc() {
  return {line_curve("c1", gp_Pnt(0, 0, 0), gp_Pnt(25, 0, 0)),
          arc_curve("c2", gp_Pnt(25, 30, 0), gp_Dir(0, -1, 0), 30.0, 0.0, 0.5 * kPi)};
}

// The edges drawn: those not left out as smooth seams.
int drawn(const Shape& shape) {
  return edge_count(shape.occt()) - smooth_seams(shape.occt(), &shape).Extent();
}

// The faces whose names, without piece numbers, are `name`.
std::vector<int> pieces_of(const Shape& shape, const std::string& name) {
  std::vector<int> found;
  for (int i = 0; i < shape.face_count(); ++i) {
    for (const std::string& each : shape.face_names(i)) {
      if (mitcad::geometry::unsplit_face_name(each) == name) {
        found.push_back(i);
        break;
      }
    }
  }
  return found;
}

// The edges between faces `a` and `b`.
std::vector<TopoDS_Shape> edges_between(const Shape& shape, int a, int b) {
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces_at;
  TopExp::MapShapesAndUniqueAncestors(shape.occt(), TopAbs_EDGE, TopAbs_FACE, faces_at);
  std::vector<TopoDS_Shape> found;
  for (int i = 1; i <= faces_at.Extent(); ++i) {
    const NCollection_List<TopoDS_Shape>& faces = faces_at(i);
    if (faces.Extent() != 2) {
      continue;
    }
    const int f = shape.face_index(faces.First());
    const int g = shape.face_index(faces.Last());
    if ((f == a && g == b) || (f == b && g == a)) {
      found.push_back(faces_at.FindKey(i));
    }
  }
  return found;
}

GeomAbs_SurfaceType type_of(const Shape& shape, int face) {
  return BRepAdaptor_Surface(shape.face(face), false).GetType();
}

std::size_t groups_of(const Shape& shape) {
  const std::vector<std::uint32_t> groups = mitcad::geometry::smooth_surface_groups(shape);
  CHECK(static_cast<int>(groups.size()) == shape.face_count());
  return std::set<std::uint32_t>(groups.begin(), groups.end()).size();
}

void test_line_and_arc_pipe() {
  const ShapePtr pipe = piped(line_and_arc());
  const std::vector<int> sides = pieces_of(*pipe, "F2:side0");
  CHECK(sides.size() == 2);
  if (sides.size() != 2) {
    return;
  }
  // The reported case is a cylinder and a torus (read from pipe_shell's
  // sweep of a circle along a line and an arc).
  const GeomAbs_SurfaceType first = type_of(*pipe, sides[0]);
  const GeomAbs_SurfaceType second = type_of(*pipe, sides[1]);
  CHECK((first == GeomAbs_Cylinder && second == GeomAbs_Torus) ||
        (first == GeomAbs_Torus && second == GeomAbs_Cylinder));
  CHECK(mitcad::geometry::different_analytic_surfaces(pipe->face(sides[0]), pipe->face(sides[1])));
  // The circle between them: not a smooth seam, though they meet
  // tangentially.
  const ShapeMap seams = smooth_seams(pipe->occt(), pipe.get());
  const std::vector<TopoDS_Shape> between = edges_between(*pipe, sides[0], sides[1]);
  CHECK(between.size() == 1);
  for (const TopoDS_Shape& edge : between) {
    CHECK(!seams.Contains(edge));
    CHECK(mitcad::geometry::smooth_across(TopoDS::Edge(edge), pipe->face(sides[0]), pipe->face(sides[1])));
  }
  // The two seams, along the cylinder and round the torus: left out.
  CHECK(edge_count(pipe->occt()) == 5);
  CHECK(seams.Extent() == 2);
  for (int i = 1; i <= seams.Extent(); ++i) {
    bool closed = false;
    for (const int side : sides) {
      closed = closed || BRep_Tool::IsClosed(TopoDS::Edge(seams(i)), pipe->face(side));
    }
    CHECK(closed);
  }
  CHECK(drawn(*pipe) == 3);
  // model_render's surfaces: the four faces apart.
  CHECK(groups_of(*pipe) == 4);
}

void test_line_arc_line_pipe() {
  mitcad::geometry::Path path = line_and_arc();
  path.push_back(line_curve("c3", gp_Pnt(55, 30, 0), gp_Pnt(55, 60, 0)));
  const ShapePtr pipe = piped(path);
  CHECK(pieces_of(*pipe, "F2:side0").size() == 3);
  // Both circles between the pieces, the three seams left out.
  CHECK(edge_count(pipe->occt()) == 7);
  CHECK(drawn(*pipe) == 4);
  CHECK(groups_of(*pipe) == 5);
}

void test_two_arcs_of_one_circle() {
  // A quarter of radius 30 about (0, 30) from the origin and the next
  // quarter: one torus in two pieces.
  const ShapePtr pipe = piped({arc_curve("c1", gp_Pnt(0, 30, 0), gp_Dir(0, -1, 0), 30.0, 0.0, 0.5 * kPi),
                               arc_curve("c2", gp_Pnt(0, 30, 0), gp_Dir(0, -1, 0), 30.0, 0.5 * kPi, kPi)});
  const std::vector<int> sides = pieces_of(*pipe, "F2:side0");
  CHECK(sides.size() == 2);
  if (sides.size() == 2) {
    CHECK(!mitcad::geometry::different_analytic_surfaces(pipe->face(sides[0]), pipe->face(sides[1])));
  }
  CHECK(edge_count(pipe->occt()) == 5);
  CHECK(drawn(*pipe) == 2);
  // The torus's pieces one surface, each cap its own.
  CHECK(groups_of(*pipe) == 3);
}

void test_two_collinear_lines() {
  const ShapePtr pipe =
      piped({line_curve("c1", gp_Pnt(0, 0, 0), gp_Pnt(20, 0, 0)), line_curve("c2", gp_Pnt(20, 0, 0), gp_Pnt(40, 0, 0))});
  // One cylinder, whether in one piece or two: only the caps' circles.
  CHECK(drawn(*pipe) == 2);
  const std::vector<int> sides = pieces_of(*pipe, "F2:side0");
  if (sides.size() == 2) {
    for (const TopoDS_Shape& edge : edges_between(*pipe, sides[0], sides[1])) {
      CHECK(smooth_seams(pipe->occt(), pipe.get()).Contains(edge));
    }
  }
  CHECK(groups_of(*pipe) == 3);
}

void test_mirrored_pipe() {
  // GitHub issue 516: a mirror's copy and its original show the same
  // edges, the circle between the cylinder and the torus on both.
  const ShapePtr original = piped(line_and_arc());
  gp_Trsf mirror;
  mirror.SetMirror(gp_Ax2(gp_Pnt(-20, 0, 0), gp::DX()));
  BRepBuilderAPI_Transform copy(original->occt(), mirror, true);
  CHECK(copy.IsDone());
  if (!copy.IsDone()) {
    return;
  }
  std::vector<Shape::NamedFace> faces;
  for (int i = 0; i < original->face_count(); ++i) {
    mitcad::geometry::NameList names;
    for (const std::string& name : original->face_names(i)) {
      names.push_back("F4:inst1(" + name + ")");
    }
    faces.push_back({copy.ModifiedShape(original->face(i)), names});
  }
  const Shape mirrored(copy.Shape(), faces);
  CHECK(pieces_of(mirrored, "F4:inst1(F2:side0)").size() == 2);
  CHECK(drawn(mirrored) == 3);
  CHECK(drawn(mirrored) == drawn(*original));
  CHECK(smooth_seams(mirrored.occt(), &mirrored).Extent() == 2);
}

TopoDS_Face face_on(const occ::handle<Geom_Surface>& surface, double u0, double u1, double v0, double v1) {
  return BRepBuilderAPI_MakeFace(surface, u0, u1, v0, v1, 1.0e-7).Face();
}

void test_analytic_surfaces() {
  using mitcad::geometry::different_analytic_surfaces;
  const gp_Pnt origin(0, 0, 0);
  const auto same = [](const TopoDS_Face& a, const TopoDS_Face& b) {
    return !different_analytic_surfaces(a, b) && !different_analytic_surfaces(b, a);
  };
  const auto apart = [](const TopoDS_Face& a, const TopoDS_Face& b) {
    return different_analytic_surfaces(a, b) && different_analytic_surfaces(b, a);
  };
  // Noise below the tolerances: a tenth of them.
  const double noise = 0.1 * mitcad::geometry::kOneSurfaceLength;

  // Spheres: the centre and the radius only.
  const auto sphere = [](const gp_Ax3& frame, double radius) {
    return face_on(new Geom_SphericalSurface(frame, radius), 0.0, kPi, -0.25 * kPi, 0.25 * kPi);
  };
  const TopoDS_Face ball = sphere(gp_Ax3(origin, gp::DZ(), gp::DX()), 10.0);
  CHECK(same(ball, sphere(gp_Ax3(gp_Pnt(0, 0, noise), gp::DX(), gp::DY()), 10.0 + noise)));
  CHECK(apart(ball, sphere(gp_Ax3(origin, gp::DZ(), gp::DX()), 10.5)));
  CHECK(apart(ball, sphere(gp_Ax3(gp_Pnt(0.01, 0, 0), gp::DZ(), gp::DX()), 10.0)));

  // Tori: the axis line, the centre and both radii.
  const auto torus = [](const gp_Ax3& frame, double major, double minor) {
    return face_on(new Geom_ToroidalSurface(frame, major, minor), 0.0, kPi, 0.0, kPi);
  };
  const TopoDS_Face ring = torus(gp_Ax3(origin, gp::DZ(), gp::DX()), 30.0, 10.0);
  CHECK(same(ring, torus(gp_Ax3(gp_Pnt(noise, 0, 0), gp::DZ().Reversed(), gp::DY()), 30.0 + noise, 10.0 - noise)));
  CHECK(apart(ring, torus(gp_Ax3(origin, gp::DZ(), gp::DX()), 30.0, 9.0)));
  CHECK(apart(ring, torus(gp_Ax3(origin, gp::DZ(), gp::DX()), 31.0, 10.0)));
  CHECK(apart(ring, torus(gp_Ax3(gp_Pnt(0, 0, 5), gp::DZ(), gp::DX()), 30.0, 10.0)));
  CHECK(apart(ring, torus(gp_Ax3(origin, gp::DX(), gp::DY()), 30.0, 10.0)));

  // Cylinders: the axis as a line, either way, and the radius.
  const auto cylinder = [](const gp_Ax3& frame, double radius) {
    return face_on(new Geom_CylindricalSurface(frame, radius), 0.0, kPi, 0.0, 10.0);
  };
  const TopoDS_Face rod = cylinder(gp_Ax3(origin, gp::DZ(), gp::DX()), 10.0);
  CHECK(same(rod, cylinder(gp_Ax3(gp_Pnt(0, 0, 50), gp::DZ().Reversed(), gp::DY()), 10.0 + noise)));
  CHECK(apart(rod, cylinder(gp_Ax3(gp_Pnt(1, 0, 0), gp::DZ(), gp::DX()), 10.0)));
  CHECK(apart(rod, cylinder(gp_Ax3(origin, gp::DZ(), gp::DX()), 11.0)));

  // Cones: the apex, the axis line and the half-angle; the same cone
  // through the apex with its axis the other way (the other nappe).
  const double half = kPi / 6.0;
  const auto cone = [](const gp_Ax3& frame, double angle, double radius) {
    return face_on(new Geom_ConicalSurface(frame, angle, radius), 0.0, kPi, 1.0, 5.0);
  };
  const TopoDS_Face funnel = cone(gp_Ax3(gp_Pnt(0, 0, 10), gp::DZ(), gp::DX()), half, 10.0 * std::tan(half));
  CHECK(same(funnel, cone(gp_Ax3(gp_Pnt(0, 0, -20), gp::DZ().Reversed(), gp::DY()), half, 20.0 * std::tan(half))));
  CHECK(apart(funnel, cone(gp_Ax3(gp_Pnt(0, 0, 10), gp::DZ(), gp::DX()), kPi / 5.0, 10.0 * std::tan(kPi / 5.0))));
  CHECK(apart(funnel, cone(gp_Ax3(gp_Pnt(0, 0, 11), gp::DZ(), gp::DX()), half, 10.0 * std::tan(half))));

  // Planes: the plane itself, its normal either way.
  const auto plane = [](const gp_Ax3& frame) { return face_on(new Geom_Plane(frame), 0.0, 10.0, 0.0, 10.0); };
  const TopoDS_Face floor = plane(gp_Ax3(origin, gp::DZ(), gp::DX()));
  CHECK(same(floor, plane(gp_Ax3(gp_Pnt(5, 5, noise), gp::DZ().Reversed(), gp::DY()))));
  CHECK(apart(floor, plane(gp_Ax3(gp_Pnt(0, 0, 0.01), gp::DZ(), gp::DX()))));

  // Types: a cylinder and a torus, a plane and a sphere.
  CHECK(apart(rod, ring));
  CHECK(apart(floor, ball));

  // A B-spline is never judged, not even against the surface it was made
  // from or a plane.
  const occ::handle<Geom_RectangularTrimmedSurface> bounded = new Geom_RectangularTrimmedSurface(
      new Geom_CylindricalSurface(gp_Ax3(origin, gp::DZ(), gp::DX()), 10.0), 0.0, kPi, 0.0, 10.0);
  const occ::handle<Geom_BSplineSurface> spline = GeomConvert::SurfaceToBSplineSurface(bounded);
  const TopoDS_Face free_form = BRepBuilderAPI_MakeFace(spline, 1.0e-7).Face();
  CHECK(same(free_form, rod));
  CHECK(same(free_form, floor));
}

} // namespace
} // namespace test

int main() {
  test::guarded("box", test::test_box);
  test::guarded("cylinder and sphere", test::test_cylinder_and_sphere);
  test::guarded("hole", test::test_hole);
  test::guarded("filleted plate", test::test_filleted_plate);
  test::guarded("teardrop and smooth profiles", test::test_teardrop_and_smooth_profiles);
  test::guarded("one plane", test::test_one_plane);
  test::guarded("named pieces", test::test_named_pieces);
  test::guarded("unsplit names", test::test_unsplit_names);
  test::guarded("line and arc pipe", test::test_line_and_arc_pipe);
  test::guarded("line, arc and line pipe", test::test_line_arc_line_pipe);
  test::guarded("two arcs of one circle", test::test_two_arcs_of_one_circle);
  test::guarded("two collinear lines", test::test_two_collinear_lines);
  test::guarded("mirrored pipe", test::test_mirrored_pipe);
  test::guarded("analytic surfaces", test::test_analytic_surfaces);
  if (test::failures == 0) {
    std::puts("test_smooth_seams: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
