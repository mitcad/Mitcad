// SPDX-License-Identifier: MIT
// Section curves of sketches (mitcad#389): where a plane cuts a shape's
// faces, a piece per face, B-spline pieces that meet end to end joined.

#include <cmath>
#include <cstdint>
#include <set>
#include <string>
#include <utility>
#include <vector>

#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Builder.hxx>
#include <GeomAPI_PointsToBSpline.hxx>
#include <Geom_BSplineCurve.hxx>
#include <NCollection_Array1.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>
#include <gp_Trsf.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Shell.hxx>

#include "../src/history.hpp"
#include "check.hpp"
#include "mitcad/geometry/import.hpp"
#include "mitcad/geometry/persist.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad/geometry/sketch_ref.hpp"
#include "mitcad/geometry/surfaces.hpp"

namespace test {
namespace {

using mitcad::geometry::ModelCurveKind;
using mitcad::geometry::SectionCurve;
using mitcad::geometry::Shape;

// A B-spline through points of the plane z = 0.
occ::handle<Geom_BSplineCurve> spline(const std::vector<gp_Pnt>& points) {
  NCollection_Array1<gp_Pnt> array(1, static_cast<int>(points.size()));
  for (std::size_t i = 0; i < points.size(); ++i) {
    array(static_cast<int>(i) + 1) = points[i];
  }
  return GeomAPI_PointsToBSpline(array, 3, 3).Curve();
}

const gp_Pnt& first_pole(const SectionCurve& c) { return c.curve.poles.front(); }

const gp_Pnt& last_pole(const SectionCurve& c) { return c.curve.poles.back(); }

void test_a_box_cut_through_its_middle() {
  // The plane y = 5 through a 10 mm cube: a line on each of the four faces
  // it crosses, none on the two faces parallel to it.
  const Shape box(BRepPrimAPI_MakeBox(10.0, 10.0, 10.0).Shape());
  const std::vector<SectionCurve> curves =
      mitcad::geometry::section_curves(box, "", gp_Pnt(0.0, 5.0, 0.0), gp_Dir(0.0, 1.0, 0.0));
  CHECK(curves.size() == 4);
  double length = 0.0;
  for (const SectionCurve& c : curves) {
    CHECK(c.curve.kind == ModelCurveKind::Line);
    CHECK(near(c.curve.start.Y(), 5.0) && near(c.curve.end.Y(), 5.0));
    length += c.curve.start.Distance(c.curve.end);
  }
  CHECK_NEAR_TOL(length, 40.0, 1e-9);
  // A plane that misses it cuts nothing.
  CHECK(mitcad::geometry::section_curves(box, "", gp_Pnt(0.0, 20.0, 0.0), gp_Dir(0.0, 1.0, 0.0)).empty());
}

void test_spline_pieces_end_to_end_are_one_curve() {
  // A prism on a profile of two splines end to end and a line closing them:
  // cut across, the splines' faces give pieces that meet where only they
  // end, one curve; the line's face gives a line.
  const gp_Pnt a(0.0, 0.0, 0.0);
  const gp_Pnt b(10.0, 0.0, 0.0);
  const gp_Pnt c(20.0, 0.0, 0.0);
  const occ::handle<Geom_BSplineCurve> s1 = spline({a, gp_Pnt(3.0, 4.0, 0.0), gp_Pnt(7.0, 5.0, 0.0), b});
  const occ::handle<Geom_BSplineCurve> s2 = spline({b, gp_Pnt(13.0, -3.0, 0.0), gp_Pnt(17.0, 6.0, 0.0), c});
  BRepBuilderAPI_MakeWire wire;
  wire.Add(BRepBuilderAPI_MakeEdge(s1).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(s2).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(c, a).Edge());
  const TopoDS_Face profile = BRepBuilderAPI_MakeFace(wire.Wire(), true).Face();
  const Shape prism(BRepPrimAPI_MakePrism(profile, gp_Vec(0.0, 0.0, 8.0)).Shape());
  const std::vector<SectionCurve> curves =
      mitcad::geometry::section_curves(prism, "", gp_Pnt(0.0, 0.0, 3.0), gp_Dir(0.0, 0.0, 1.0));
  CHECK(curves.size() == 2);
  int splines = 0;
  for (const SectionCurve& curve : curves) {
    if (curve.curve.kind != ModelCurveKind::BSpline) {
      CHECK(curve.curve.kind == ModelCurveKind::Line);
      continue;
    }
    ++splines;
    // From one end of the profile's splines to the other, at z = 3.
    const gp_Pnt p0 = first_pole(curve);
    const gp_Pnt p1 = last_pole(curve);
    CHECK((p0.Distance(gp_Pnt(0, 0, 3)) < 1e-6 && p1.Distance(gp_Pnt(20, 0, 3)) < 1e-6) ||
          (p1.Distance(gp_Pnt(0, 0, 3)) < 1e-6 && p0.Distance(gp_Pnt(20, 0, 3)) < 1e-6));
    for (const gp_Pnt& pole : curve.curve.poles) {
      CHECK(std::abs(pole.Z() - 3.0) < 1e-6);
    }
  }
  CHECK(splines == 1);
}

// Intersect's crossing points (mitcad#736): a plane through an edge gives
// the point where it passes, an edge in the plane its ends, a plane that
// misses it nothing.
void test_edges_crossing_points() {
  const ShapePtr block = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 20);
  // Line 0 runs along y = 0 and line 1 along x = 60: their side faces
  // meet in the vertical edge at (60, 0).
  const std::string corner = edge(side("F2", 1, 0), side("F2", 1, 1));
  const std::vector<gp_Pnt> across =
      mitcad::geometry::section_points(*block, corner, gp_Pnt(0.0, 0.0, 5.0), gp_Dir(0.0, 0.0, 1.0));
  CHECK(across.size() == 1);
  if (across.size() == 1) {
    CHECK(across[0].Distance(gp_Pnt(60.0, 0.0, 5.0)) < 1e-6);
  }
  // The top front edge lies in the plane y = 0: its ends.
  const std::string top_front = edge(end_cap("F2", 1), side("F2", 1, 0));
  const std::vector<gp_Pnt> lying =
      mitcad::geometry::section_points(*block, top_front, gp_Pnt(0.0, 0.0, 0.0), gp_Dir(0.0, 1.0, 0.0));
  CHECK(lying.size() == 2);
  for (const gp_Pnt& p : lying) {
    CHECK(p.Distance(gp_Pnt(0.0, 0.0, 20.0)) < 1e-6 || p.Distance(gp_Pnt(60.0, 0.0, 20.0)) < 1e-6);
  }
  // Above the block the plane misses the edge.
  CHECK(mitcad::geometry::section_points(*block, corner, gp_Pnt(0.0, 0.0, 30.0), gp_Dir(0.0, 0.0, 1.0)).empty());
}

void test_named_face_boundaries() {
  using namespace mitcad::geometry;
  const std::string cap = end_cap("F2", 1);
  const ShapePtr block = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 20);
  const auto boundary = face_boundary(*block, cap);
  CHECK(boundary.size() == 4);
  std::set<std::string> names;
  double length = 0.0;
  for (const NamedBoundaryCurve& c : boundary) {
    CHECK(c.curve.kind == ModelCurveKind::Line);
    CHECK(block->find_edges(c.name).size() == 1);
    CHECK_NEAR(c.curve.start.Z(), 20.0);
    CHECK_NEAR(c.curve.end.Z(), 20.0);
    length += c.curve.start.Distance(c.curve.end);
    CHECK(names.insert(c.name).second);
  }
  CHECK_NEAR(length, 200.0);
  for (int i = 0; i < 4; ++i) {
    CHECK(names.count(edge(cap, side("F2", 1, i))) == 1);
  }
  CHECK(face_boundary(*block, "F99:missing").empty());

  // A body location is already part of its curves, with no occurrence
  // transform invented here. Its edge names remain the same.
  Affine placement;
  placement.translation = {100.0, -15.0, 7.0};
  const ShapePtr moved = move_shape(*block, placement);
  const auto placed = face_boundary(*moved, cap);
  CHECK(placed.size() == boundary.size());
  for (std::size_t i = 0; i < placed.size() && i < boundary.size(); ++i) {
    CHECK(placed[i].name == boundary[i].name);
    CHECK(placed[i].curve.start.Distance(boundary[i].curve.start.Translated(gp_Vec(100, -15, 7))) < 1e-9);
    CHECK(placed[i].curve.end.Distance(boundary[i].curve.end.Translated(gp_Vec(100, -15, 7))) < 1e-9);
  }

  // The unsuffixed name resolves both pieces of a split cap. Their shared
  // edge is internal to the union: only the six outer pieces bound it.
  const Tool cutter = Tool::of_plane(gp_Pln(gp_Pnt(20, 0, 0), gp_Dir(1, 0, 0)));
  const ShapePtr split = split_faces("F7", *block, {cap}, cutter);
  const auto pieces = face_boundary(*split, cap);
  CHECK(pieces.size() == 6);
  names.clear();
  for (const NamedBoundaryCurve& c : pieces) {
    CHECK(names.insert(c.name).second);
    CHECK(split->find_edges(c.name).size() == 1);
  }
  for (const int f : split->find_faces(cap)) {
    const auto piece = face_boundary(*split, split->face_names(f).front());
    CHECK(piece.size() == 4);
    int internal = 0;
    for (const NamedBoundaryCurve& c : piece) internal += names.count(c.name) == 0;
    CHECK(internal == 1);
  }
}

void test_named_boundaries_of_a_surface_body() {
  using namespace mitcad::geometry;
  const std::string cap = end_cap("F2", 1);
  const ShapePtr block = extrude("F2", {rectangle(1, 0, 0, 60, 40)}, 0, 20);
  CHECK(body_kind(block->occt()) == BodyKind::Solid);
  const auto bottom = block->find_faces(start_cap("F2", 1));
  CHECK(bottom.size() == 1);
  if (bottom.size() != 1) return;

  // Make an open shell from the solid's actual faces, omitting its bottom.
  // Keep their construction names and shared edge handles unchanged.
  // The selected top has canonical two-face edges; the open bottom does not.
  BRep_Builder builder;
  TopoDS_Shell shell;
  builder.MakeShell(shell);
  std::vector<Shape::NamedFace> faces;
  for (int i = 0; i < block->face_count(); ++i) {
    if (i == bottom.front()) continue;
    builder.Add(shell, block->face(i));
    faces.push_back({block->face(i), block->face_names(i)});
  }
  const Shape surface(shell, faces);
  CHECK(surface.occt().ShapeType() == TopAbs_SHELL);
  CHECK(body_kind(surface.occt()) == BodyKind::Sheet);
  CHECK(surface.face_count() == 5);
  CHECK_NEAR(mass_properties(surface).area, 6400.0);
  CHECK(!surface.has_open_edge_names());
  int unnamed = 0;
  for (int i = 0; i < surface.edge_count(); ++i) unnamed += surface.edge_name(i).empty();
  CHECK(unnamed == 4);

  const auto expected = face_boundary(*block, cap);
  const auto boundary = face_boundary(surface, cap);
  CHECK(boundary.size() == 4 && boundary.size() == expected.size());
  double length = 0;
  for (std::size_t i = 0; i < boundary.size() && i < expected.size(); ++i) {
    const auto& curve = boundary[i];
    CHECK(curve.name == expected[i].name);
    CHECK(curve.name.rfind("E{", 0) == 0);
    CHECK(curve.curve.kind == ModelCurveKind::Line);
    CHECK(curve.curve.start.Distance(expected[i].curve.start) < 1e-9);
    CHECK(curve.curve.end.Distance(expected[i].curve.end) < 1e-9);
    CHECK_NEAR(curve.curve.start.Z(), 20.0);
    CHECK_NEAR(curve.curve.end.Z(), 20.0);
    length += curve.curve.start.Distance(curve.curve.end);
    const auto edges = surface.find_edges(curve.name);
    const auto original = block->find_edges(curve.name);
    CHECK(edges.size() == 1 && original.size() == 1);
    if (edges.size() != 1 || original.size() != 1) continue;
    CHECK(surface.edge(edges.front()).IsSame(block->edge(original.front())));
    CHECK(surface.edge_name(edges.front()) == curve.name);
    const auto adjacent = surface.edge_faces(edges.front());
    CHECK(adjacent[0] >= 0 && adjacent[1] >= 0 && adjacent[0] != adjacent[1]);
    CHECK(surface.open_edge_names(edges.front()).empty());
  }
  CHECK_NEAR(length, 200.0);
}

void test_named_circular_face_boundaries() {
  using namespace mitcad::geometry;
  const std::string cap = "F2:end(r{c1})";
  std::set<std::string> names;
  for (const double radius : {20.0, 30.0}) {
    Region ring = circle(1, 4, 7, radius);
    ring.loops.push_back(circle(2, 4, 7, 5).loops.front());
    const ShapePtr body = extrude("F2", {ring}, 0, 12);
    const auto boundary = face_boundary(*body, cap);
    CHECK(boundary.size() == 2);
    std::set<std::string> current;
    for (const NamedBoundaryCurve& c : boundary) {
      CHECK(current.insert(c.name).second);
      CHECK(body->find_edges(c.name).size() == 1);
      CHECK(c.curve.kind == ModelCurveKind::Conic && c.curve.closed);
      CHECK(c.curve.center.Distance(gp_Pnt(4, 7, 12)) < 1e-9);
      const double expected = c.name == edge(cap, "F2:side(c1)") ? radius : 5.0;
      CHECK(c.name == edge(cap, "F2:side(c1)") || c.name == edge(cap, "F2:side(c2)"));
      CHECK_NEAR(c.curve.major, expected);
      CHECK_NEAR(c.curve.minor, expected);
    }
    if (!names.empty()) {
      CHECK(current == names);
    }
    names = std::move(current);
    CHECK(throws_with([&] { face_boundary(*body, "F2:side(c1)"); }, "is not planar"));
  }

  // An open surface's one-sided edges have no canonical EdgeName. Refuse
  // explicitly rather than returning unstable index-based references.
  const ShapePtr block = extrude("F3", {rectangle(3, 0, 0, 10, 10)}, 0, 5);
  const auto faces = block->find_faces(end_cap("F3", 3));
  CHECK(faces.size() == 1);
  if (faces.size() == 1) {
    const TopoDS_Face face = block->face(faces.front());
    const Shape open(face, {{face, {"F3:surface"}}});
    CHECK(throws_with([&] { face_boundary(open, "F3:surface"); }, "unnamed edge"));
  }
}

std::set<std::string> open_names(const Shape& shape, const std::string& face) {
  std::set<std::string> names;
  for (const auto& edge : mitcad::geometry::face_boundary(shape, face)) {
    CHECK(names.insert(edge.name).second);
  }
  return names;
}

void test_source_profile_open_boundaries() {
  using namespace mitcad::geometry;
  const std::string face = face_name("F12", "patch", rectangle_region_name(1));
  std::set<std::string> expected;
  for (int i = 0; i < 4; ++i) expected.insert(make_open_edge_name(face, rectangle_segment(1, i)));
  ShapePtr original;
  for (const double width : {10.0, 14.0}) {
    const auto patches = patch_regions("F12", Frame(), {rectangle(1, 0, 0, width, 10)});
    CHECK(patches.size() == 1);
    if (patches.size() != 1) return;
    const ShapePtr patch = patches.front();
    CHECK(open_names(*patch, face) == expected);
    double perimeter = 0;
    for (const auto& edge : face_boundary(*patch, face)) {
      CHECK(edge.curve.kind == ModelCurveKind::Line);
      perimeter += edge.curve.start.Distance(edge.curve.end);
    }
    CHECK_NEAR(perimeter, 2 * (width + 10));
    // Open references do not turn a one-sided edge into a two-face name.
    for (int i = 0; i < patch->edge_count(); ++i) CHECK(patch->edge_name(i).empty());
    const ShapePtr restored = deserialize_shape(serialize_shape(*patch));
    CHECK(open_names(*restored, face) == expected);
    const ShapePtr copied = independent_copy(*patch);
    CHECK(open_names(*copied, face) == expected);
    original = patch;
  }
  CHECK(bool(original));
  if (!original) return;
  Affine placement;
  placement.translation = {30, -15, 7};
  const ShapePtr moved = move_shape(*original, placement);
  CHECK(open_names(*moved, face) == expected);
  for (const auto& edge : face_boundary(*moved, face)) {
    CHECK_NEAR(edge.curve.start.Z(), 7);
    CHECK_NEAR(edge.curve.end.Z(), 7);
  }
  const std::string prefix = "F20:inst0";
  const ShapePtr renamed = transform_shape(*original, placement, prefix);
  std::set<std::string> renamedExpected;
  for (const auto& name : expected) renamedExpected.insert(rename_open_edge_name(name, prefix));
  CHECK(open_names(*renamed, prefix + '(' + face + ')') == renamedExpected);
  const ShapePtr renamedOnly = transform_shape(*original, Affine(), prefix);
  CHECK(open_names(*renamedOnly, prefix + '(' + face + ')') == renamedExpected);
  const std::string nestedFace = "F12:patch(r{b{E{F1:bottom|F1:top}}})";
  CHECK(rename_open_edge_name(make_open_edge_name(nestedFace, "c1[c2,c4]"), prefix) ==
        make_open_edge_name(prefix + '(' + nestedFace + ')', "c1[c2,c4]"));
  // An instance wraps the owner, rather than replacing feature ids inside
  // the exact original source segment. Nested edge/face pipes stay intact.
  const std::string nestedSegment = "b{E{F2:end(r{c2})|F2:side(c2)}}[c1,c3]";
  CHECK(rename_open_edge_name(make_open_edge_name(nestedFace, nestedSegment), prefix) ==
        make_open_edge_name(prefix + '(' + nestedFace + ')', nestedSegment));
  const std::string recursiveSegment = "b{O{F2:patch(r{c2})|c2}}[b{E{F3:bottom|F3:top}},c4]";
  CHECK(rename_open_edge_name(make_open_edge_name(nestedFace, recursiveSegment), prefix) ==
        make_open_edge_name(prefix + '(' + nestedFace + ')', recursiveSegment));
  CHECK(open_names(*original, face) == expected);

  // A copied algorithm input uses exact OCCT Modified/Generated history,
  // not proximity between the source and output curves.
  const detail::InputCopy input(original->occt());
  gp_Trsf translation;
  translation.SetTranslation(gp_Vec(0, 0, 4));
  BRepBuilderAPI_Transform operation(input.shape(), translation, true);
  detail::FaceNamer namer(operation.Shape());
  namer.carry(operation, *original, input);
  namer.finish();
  CHECK(open_names(*namer.shape(), face) == expected);

  // Holes retain their own exact source segment, beside the outer edges.
  Region ring = rectangle(1, 0, 0, 10, 10);
  ring.loops.push_back(circle(5, 5, 5, 2).loops.front());
  const ShapePtr holed = patch_regions("F12", Frame(), {ring}).front();
  auto withHole = expected;
  withHole.insert(make_open_edge_name(face, "c5"));
  CHECK(open_names(*holed, face) == withHole);
}

void test_ambiguous_open_boundary_history_is_refused() {
  using namespace mitcad::geometry;
  const std::string face = face_name("F12", "patch", rectangle_region_name(1));
  const ShapePtr patch = patch_regions("F12", Frame(), {rectangle(1, 0, 0, 10, 10)}).front();
  const ShapePtr merged = independent_copy(*patch);
  auto metadata = merged->named_open_edges();
  CHECK(!metadata.empty());
  if (metadata.empty()) return;
  metadata.front().names.push_back(make_open_edge_name("F13:patch(r{c9})", "c9"));
  merged->set_open_edge_names(metadata);
  CHECK(throws_with([&] { face_boundary(*merged, face); }, "ambiguous profile boundary"));

  // One source can have several topological images in different face
  // pieces. Selecting only one piece must not hide that ambiguity.
  Affine placement;
  placement.translation = {20, 0, 0};
  const ShapePtr other = move_shape(*patch, placement);
  BRep_Builder builder;
  TopoDS_Compound compound;
  builder.MakeCompound(compound);
  builder.Add(compound, patch->occt());
  builder.Add(compound, other->occt());
  Shape split(compound, {{patch->face(0), {face + "#0"}}, {other->face(0), {face + "#1"}}});
  metadata = patch->named_open_edges();
  const auto second = other->named_open_edges();
  metadata.insert(metadata.end(), second.begin(), second.end());
  split.set_open_edge_names(metadata);
  CHECK(throws_with([&] { face_boundary(split, face + "#0"); }, "ambiguous profile boundary"));
}

void test_old_stored_shapes_need_no_open_provenance() {
  using namespace mitcad::geometry;
  const ShapePtr block = extrude("F2", {rectangle(1, 0, 0, 10, 10)}, 0, 5);
  std::string bytes = serialize_shape(*block);
  CHECK(!deserialize_shape(bytes)->has_open_edge_names());
  // Format 3 has the same fields except for format 4's per-edge lists.
  // Produce those legacy bytes from an ordinary solid with empty lists.
  std::size_t at = 8;
  const auto read = [&bytes, &at](int count) {
    std::uint64_t value = 0;
    for (int i = 0; i < count; ++i) {
      value |= std::uint64_t(static_cast<unsigned char>(bytes.at(at++))) << (8 * i);
    }
    return value;
  };
  const std::size_t brep = static_cast<std::size_t>(read(8));
  at += brep;
  const auto faces = read(4);
  const auto edges = read(4);
  (void)read(4); // vertices
  for (std::uint64_t f = 0; f < faces; ++f) {
    const auto names = read(4);
    for (std::uint64_t n = 0; n < names; ++n) {
      const auto length = static_cast<std::size_t>(read(4));
      at += length;
    }
  }
  for (std::uint64_t e = 0; e < edges; ++e) {
    at += 8; // adjacent faces
    const auto length = static_cast<std::size_t>(read(4));
    at += length;
    const std::size_t metadata = at;
    CHECK(read(4) == 0);
    bytes.erase(metadata, 4);
    at = metadata;
  }
  bytes[4] = 3;
  const ShapePtr legacy = deserialize_shape(bytes);
  CHECK(!legacy->has_open_edge_names());
  CHECK_NEAR(volume(*legacy), 500);
  CHECK(open_names(*legacy, end_cap("F2", 1)) == open_names(*block, end_cap("F2", 1)));
}

} // namespace

void section_curve_tests() {
  test_a_box_cut_through_its_middle();
  test_spline_pieces_end_to_end_are_one_curve();
  test_edges_crossing_points();
  test_named_face_boundaries();
  test_named_boundaries_of_a_surface_body();
  test_named_circular_face_boundaries();
  test_source_profile_open_boundaries();
  test_ambiguous_open_boundary_history_is_refused();
  test_old_stored_shapes_need_no_open_provenance();
}

} // namespace test
