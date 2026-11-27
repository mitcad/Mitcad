// SPDX-License-Identifier: MIT
// Nine fillet and chamfer cases of the public B-rep benchmark brep-bench
// (brepbench.org, github.com/bmander/brep-bench), each described there on
// cases/<op>/<name>.md with its exact volume and area, that stock OCCT
// 8.0.1 refuses or throws on (mitcad#2273, GitHub #576): the fillet cases
// l-block-inside-consumed, round-swallowing-a-tiny-face, slab-half-round,
// hole-waist, boss-s-curve and nearly-flat-ridge, and the chamfer cases
// block-ridge, l-block-inside-consumed and octahedron-all-edges. Their
// themes have patches in the OCCT port (third_party/vcpkg-ports): faces a
// dressup consumes, two roundings meeting on one contact line (a full
// round), a convex and a concave rounding of one face meeting, edges near
// tangent, chamfers wider than a face beside them, four planar chamfers
// meeting at a vertex (patch 2306, mitcad#2306).
//
// The bodies are modelled here from the case descriptions (dimensions,
// edges, sizes); the expected volumes and areas are their closed forms. No
// file of the benchmark is used. Each case runs twice:
//   - OCCT's dressup alone (BRepFilletAPI_MakeFillet or
//     BRepFilletAPI_MakeChamfer on the patched build),
//   - MitCad's Fillet or Chamfer (mitcad::geometry::fillet / chamfer,
//     src/dressup.cpp, the path the app takes, with its ways round a
//     kernel failure: the narrow and consumed faces continued, the rings,
//     the 0.1 % smaller retry and so on),
// and prints for each path whether it builds the case, its volume and area
// with their relative errors, its face count and MitCad's infos and notes:
// which path builds a case is in the test's output.
//
// A path passes when it gives one valid (BRepCheck_Analyzer) closed solid,
// right side out, whose volume and area are within relative 1e-6 of the
// exact ones for the planar chamfers and within 1e-4 for the curved blends,
// which OCCT may build as approximated surfaces (the benchmark's own
// grading asks 1e-6 of both; no case states a tolerance of its own).
// MitCad's 0.1 % smaller retry is thus a wrong answer, not a pass.
//
// Known failures. Each path of a case has an expectation, `Expect{}` (it
// passes) or `Expect{"<why, in one line>"}` (a known failure: why it does
// not build the answer, naming the open ticket that covers it where one
// does). A known failure runs and prints its result as "known failure
// (<why>)" and does not fail the ctest. A known failure that
// builds the right answer prints "unexpected pass" and fails the ctest, so
// that the change which fixes it notices and moves the case to `Expect{}`
// in the same change; a case expected to pass that does not fails the ctest
// as any check does. MITCAD_TEST_ONLY=<case> runs one case.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <functional>
#include <memory>
#include <optional>
#include <string>
#include <utility>
#include <vector>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepLib.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_IndexedMap.hxx>
#include <NCollection_List.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/shape.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

namespace mg = mitcad::geometry;

// ---------------------------------------------------------------- expectations

// What a path of a case is expected to give: the right answer (no reason),
// or a known failure with why, in one line (with its open ticket where one
// covers it).
struct Expect {
  const char* known_failure = nullptr;
};

// A case's exact answer: volume and area with the relative tolerance they
// are checked to, and where given (not 0) the volume the dressup adds
// (positive) or removes (negative), checked to 1e-3 of itself, for a case
// whose change is smaller than the tolerance on the volume.
struct Answer {
  double volume = 0.0;
  double area = 0.0;
  double relative = 1e-6;
  double change = 0.0;
};

// What a path made: the shape (null when it failed) with why it failed, and
// MitCad's infos and notes.
struct Outcome {
  TopoDS_Shape shape;
  std::string error;
  std::vector<std::string> infos;
  std::vector<std::string> notes;
};

// ---------------------------------------------------------------- measures

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

double area_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(shape, props, 1e-9);
  return props.Mass();
}

int count_of(const TopoDS_Shape& shape, TopAbs_ShapeEnum type) {
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> all;
  TopExp::MapShapes(shape, type, all);
  return all.Extent();
}

// Every edge but a degenerated one bounds two faces (a seam its face twice).
bool closed(const TopoDS_Shape& shape) {
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces_of;
  TopExp::MapShapesAndAncestors(shape, TopAbs_EDGE, TopAbs_FACE, faces_of);
  for (int i = 1; i <= faces_of.Extent(); ++i) {
    if (!BRep_Tool::Degenerated(TopoDS::Edge(faces_of.FindKey(i))) && faces_of(i).Extent() != 2) {
      return false;
    }
  }
  return true;
}

double relative_error(double value, double exact) { return std::abs(value - exact) / std::max(1.0, std::abs(exact)); }

// Prints what the path made and judges it against the answer and the
// expectation (the file's header).
void judge(const char* name, const char* path, const Outcome& made, const Answer& answer, double body_volume,
           const Expect& expect) {
  bool right = false;
  std::string what;
  if (made.shape.IsNull()) {
    what = "not built: " + (made.error.empty() ? std::string("no result") : made.error);
  } else {
    const bool valid = BRepCheck_Analyzer(made.shape).IsValid();
    const int solids = count_of(made.shape, TopAbs_SOLID);
    const bool shut = closed(made.shape);
    const double volume = volume_of(made.shape);
    const double area = area_of(made.shape);
    const double volume_error = relative_error(volume, answer.volume);
    const double area_error = relative_error(area, answer.area);
    bool change_right = true;
    char buffer[512];
    std::snprintf(buffer, sizeof buffer,
                  "built, %s, %d solid(s)%s, %d faces, volume %.12g (exact %.12g, relative error %.2g), "
                  "area %.12g (exact %.12g, relative error %.2g)",
                  valid ? "valid" : "INVALID", solids, shut ? "" : ", NOT CLOSED", count_of(made.shape, TopAbs_FACE),
                  volume, answer.volume, volume_error, area, answer.area, area_error);
    what = buffer;
    if (answer.change != 0.0) {
      const double change = volume - body_volume;
      change_right = std::abs(change - answer.change) <= 1e-3 * std::abs(answer.change);
      std::snprintf(buffer, sizeof buffer, ", volume changed by %.9g (exact %.9g)", change, answer.change);
      what += buffer;
    }
    for (const std::string& info : made.infos) {
      what += "; info: " + info;
    }
    for (const std::string& note : made.notes) {
      what += "; note: " + note;
    }
    right = valid && solids == 1 && shut && volume > 0.0 && volume_error <= answer.relative &&
            area_error <= answer.relative && change_right;
  }
  if (expect.known_failure == nullptr) {
    std::printf("%s [%s]: %s -> %s\n", name, path, what.c_str(), right ? "pass" : "FAIL");
    if (!right) {
      std::fprintf(stderr, "%s [%s]: check failed: expected to build the exact answer\n", name, path);
      ++failures;
    }
  } else if (right) {
    std::printf("%s [%s]: %s -> unexpected pass\n", name, path, what.c_str());
    std::fprintf(stderr,
                 "%s [%s]: unexpected pass: a known failure (%s) builds the exact answer now; move it to Expect{} "
                 "in the change that fixed it\n",
                 name, path, expect.known_failure);
    ++failures;
  } else {
    std::printf("%s [%s]: %s -> known failure (%s)\n", name, path, what.c_str(), expect.known_failure);
  }
  std::fflush(stdout);
}

// ---------------------------------------------------------------- the two paths

// OCCT's dressup, built; a null shape with the error when it fails.
template <typename Maker>
Outcome built(Maker& maker) {
  Outcome out;
  try {
    OCC_CATCH_SIGNALS
    maker.Build();
    if (maker.IsDone()) {
      out.shape = maker.Shape();
    } else {
      out.error = "the dressup is not done";
    }
  } catch (const Standard_Failure& failure) {
    out.error = std::string("the geometry kernel failed: ") + failure.what();
  }
  return out;
}

// The body's faces named N1, N2, ... in TopExp order.
mg::ShapePtr named(const TopoDS_Shape& body) {
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> faces;
  TopExp::MapShapes(body, TopAbs_FACE, faces);
  std::vector<mg::Shape::NamedFace> named_faces;
  for (int i = 1; i <= faces.Extent(); ++i) {
    named_faces.push_back({faces(i), {"N" + std::to_string(i)}});
  }
  return std::make_shared<mg::Shape>(body, named_faces);
}

// The names of the edges in the named body (empty when one has none).
std::vector<std::string> names_of(const mg::Shape& body, const std::vector<TopoDS_Edge>& edges) {
  std::vector<std::string> names;
  for (const TopoDS_Edge& edge : edges) {
    const std::optional<std::string> name = body.name_of_edge(edge);
    if (!name || name->empty()) {
      return {};
    }
    names.push_back(*name);
  }
  return names;
}

// MitCad's dressup through `dress` (its fillet or chamfer on the named
// body's edges); a null shape with the error when it fails.
Outcome by_mitcad(const std::function<mg::ShapePtr()>& dress) {
  Outcome out;
  try {
    const mg::ShapePtr result = dress();
    if (result) {
      out.shape = result->occt();
      out.infos = result->infos();
      out.notes = result->notes();
    } else {
      out.error = "no result";
    }
  } catch (const Standard_Failure& failure) {
    out.error = std::string("the geometry kernel failed: ") + failure.what();
  } catch (const std::exception& error) {
    out.error = error.what();
  }
  return out;
}

// The edges of a shape whose middle point passes the test.
std::vector<TopoDS_Edge> edges_where(const TopoDS_Shape& shape, const std::function<bool(const gp_Pnt&)>& keep) {
  std::vector<TopoDS_Edge> edges;
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> all;
  TopExp::MapShapes(shape, TopAbs_EDGE, all);
  for (int i = 1; i <= all.Extent(); ++i) {
    const TopoDS_Edge& edge = TopoDS::Edge(all(i));
    double first = 0.0, last = 0.0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last);
    if (!curve.IsNull() && keep(curve->Value(0.5 * (first + last)))) {
      edges.push_back(edge);
    }
  }
  return edges;
}

// The edges rounded with radius r, by OCCT alone and by MitCad's Fillet,
// each judged (the file's header).
void fillet_case(const char* name, const TopoDS_Shape& body, const std::vector<TopoDS_Edge>& edges,
                 std::size_t edge_count, double r, const Answer& answer, const Expect& occt, const Expect& mitcad) {
  CHECK(edges.size() == edge_count);
  if (edges.size() != edge_count) {
    return;
  }
  const double body_volume = volume_of(body);
  BRepFilletAPI_MakeFillet fillet(body);
  for (const TopoDS_Edge& edge : edges) {
    fillet.Add(r, edge);
  }
  judge(name, "OCCT's fillet alone", built(fillet), answer, body_volume, occt);

  const mg::ShapePtr input = named(body);
  mg::FilletSet set;
  set.edges = names_of(*input, edges);
  set.radius = r;
  CHECK(set.edges.size() == edge_count);
  judge(name, "MitCad's Fillet", by_mitcad([&] { return mg::fillet("B", *input, {set}); }), answer, body_volume,
        mitcad);
}

// The edges chamfered with the same distance d on both faces, by OCCT alone
// and by MitCad's Chamfer, each judged (the file's header).
void chamfer_case(const char* name, const TopoDS_Shape& body, const std::vector<TopoDS_Edge>& edges,
                  std::size_t edge_count, double d, const Answer& answer, const Expect& occt, const Expect& mitcad) {
  CHECK(edges.size() == edge_count);
  if (edges.size() != edge_count) {
    return;
  }
  const double body_volume = volume_of(body);
  BRepFilletAPI_MakeChamfer chamfer(body);
  for (const TopoDS_Edge& edge : edges) {
    chamfer.Add(d, edge);
  }
  judge(name, "OCCT's chamfer alone", built(chamfer), answer, body_volume, occt);

  const mg::ShapePtr input = named(body);
  mg::ChamferSet set;
  set.edges = names_of(*input, edges);
  set.spec.type = mg::ChamferType::EqualDistance;
  set.spec.distance = d;
  CHECK(set.edges.size() == edge_count);
  judge(name, "MitCad's Chamfer", by_mitcad([&] { return mg::chamfer("B", *input, {set}); }), answer, body_volume,
        mitcad);
}

// ---------------------------------------------------------------- bodies

bool at(double value, double target) { return std::abs(value - target) < 1e-7; }

// A planar face through the points, in order.
TopoDS_Face polygon(const std::vector<gp_Pnt>& points) {
  BRepBuilderAPI_MakePolygon wire;
  for (const gp_Pnt& p : points) {
    wire.Add(p);
  }
  wire.Close();
  return BRepBuilderAPI_MakeFace(wire.Wire(), true).Face();
}

// The polygon of points (x, y) in the plane z = 0 extruded along +z by h.
TopoDS_Shape prism_up(const std::vector<std::pair<double, double>>& section, double h) {
  std::vector<gp_Pnt> points;
  for (const auto& p : section) {
    points.emplace_back(p.first, p.second, 0.0);
  }
  return BRepPrimAPI_MakePrism(polygon(points), gp_Vec(0, 0, h)).Shape();
}

// The L-block of both l-block-inside-consumed cases: the section through
// (0, 0), (10, 0), (10, 3), (3, 3), (3, 10), (0, 10) in xz extruded along
// +y from 0 to 10. Its one concave edge is x = 3, z = 3.
TopoDS_Shape l_block() {
  std::vector<gp_Pnt> points;
  for (const auto& p : std::vector<std::pair<double, double>>{{0, 0}, {10, 0}, {10, 3}, {3, 3}, {3, 10}, {0, 10}}) {
    points.emplace_back(p.first, 0.0, p.second);
  }
  return BRepPrimAPI_MakePrism(polygon(points), gp_Vec(0, 10, 0)).Shape();
}

std::vector<TopoDS_Edge> l_block_inside_edge(const TopoDS_Shape& body) {
  return edges_where(body, [](const gp_Pnt& m) { return at(m.X(), 3) && at(m.Z(), 3); });
}

// The block from the origin to (10, 4, 6) and the two edges of its top
// along x (y = 0 and y = 4), as slab-half-round and block-ridge have them.
TopoDS_Shape slab() { return BRepPrimAPI_MakeBox(10, 4, 6).Shape(); }

std::vector<TopoDS_Edge> slab_top_long_edges(const TopoDS_Shape& body) {
  return edges_where(body, [](const gp_Pnt& m) { return at(m.Z(), 6) && at(m.X(), 5) && (at(m.Y(), 0) || at(m.Y(), 4)); });
}

// ---------------------------------------------------------------- fillet cases

// fillet/l-block-inside-consumed: the inside edge rounded with 7, the ball
// touching both faces at their far edges, which the round consumes: the
// section the square 10 x 10 less the quarter disc of radius 7 about
// (10, 10).
void test_fillet_l_block_inside_consumed() {
  const TopoDS_Shape body = l_block();
  Answer answer;
  answer.volume = 10.0 * (100.0 - 49.0 * kPi / 4.0);
  answer.area = 460.0 + 21.0 * kPi / 2.0;
  answer.relative = 1e-4;
  fillet_case("fillet/l-block-inside-consumed", body, l_block_inside_edge(body), 1, 7.0, answer, Expect{}, Expect{});
}

// fillet/round-swallowing-a-tiny-face: a block 10 x 10 x 5 whose vertical
// edge at (10, 10) is broken by a flat at 45 degrees 0.0001 * sqrt(2)
// wide; the flat's two edges rounded together with 1. The ball passes clear
// of the flat: the block with that vertical edge rounded with 1, the flat
// gone.
void test_fillet_round_swallowing_a_tiny_face() {
  const TopoDS_Shape body = prism_up({{0, 0}, {10, 0}, {10, 9.9999}, {9.9999, 10}, {0, 10}}, 5.0);
  const std::vector<TopoDS_Edge> edges = edges_where(body, [](const gp_Pnt& m) {
    return at(m.Z(), 2.5) && ((at(m.X(), 10) && at(m.Y(), 9.9999)) || (at(m.X(), 9.9999) && at(m.Y(), 10)));
  });
  const double corner = 1.0 - kPi / 4.0; // the section's loss of a round of radius 1
  Answer answer;
  answer.volume = 5.0 * (100.0 - corner);
  answer.area = 2.0 * (100.0 - corner) + 5.0 * (40.0 - 2.0 + kPi / 2.0);
  answer.relative = 1e-4;
  fillet_case("fillet/round-swallowing-a-tiny-face", body, edges, 2, 1.0, answer, Expect{}, Expect{});
}

// fillet/slab-half-round: the block's two long top edges rounded with 2,
// half the top's width: one half cylinder of radius 2 about y = 2, z = 4,
// the top gone.
void test_fillet_slab_half_round() {
  const TopoDS_Shape body = slab();
  Answer answer;
  answer.volume = 160.0 + 20.0 * kPi;
  answer.area = 152.0 + 24.0 * kPi;
  answer.relative = 1e-4;
  fillet_case("fillet/slab-half-round", body, slab_top_long_edges(body), 2, 2.0, answer, Expect{}, Expect{});
}

// fillet/hole-waist: a plate from (-10, -10, 0) to (10, 10, 2) with a hole
// of radius 3 through it on the z axis, both rims rounded with 1, half the
// plate's thickness: the hole's wall the inner half of the torus about z
// with its centre circle of radius 4 at z = 1, the wall gone.
void test_fillet_hole_waist() {
  const TopoDS_Shape plate = BRepPrimAPI_MakeBox(gp_Pnt(-10, -10, 0), gp_Pnt(10, 10, 2)).Shape();
  const TopoDS_Shape bore = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -1), gp_Dir(0, 0, 1)), 3.0, 4.0).Shape();
  BRepAlgoAPI_Cut cut(plate, bore);
  CHECK(cut.IsDone());
  const TopoDS_Shape body = cut.Shape();
  const std::vector<TopoDS_Edge> rims = edges_where(body, [](const gp_Pnt& m) {
    return std::abs(std::hypot(m.X(), m.Y()) - 3.0) < 1e-6 && (at(m.Z(), 0) || at(m.Z(), 2));
  });
  const double d = (10.0 - 3.0 * kPi) / (12.0 - 3.0 * kPi); // the quarter disc's centroid, r = 1
  Answer answer;
  answer.volume = 800.0 - 18.0 * kPi - 4.0 * kPi * (3.0 + d) * (1.0 - kPi / 4.0);
  answer.area = 960.0 - 36.0 * kPi + 8.0 * kPi * kPi;
  answer.relative = 1e-4;
  fillet_case("fillet/hole-waist", body, rims, 2, 1.0, answer, Expect{}, Expect{});
}

// fillet/boss-s-curve: a plate from (-20, -20, 0) to (20, 20, 4) and a boss
// of radius 5 on the z axis from its top to z = 8, joined; the boss's rim
// (convex) and root (concave) rounded together with 2: the two tori meet
// tangentially on the circle of radius 5 at z = 6, the boss's side gone.
// (The boss's cylinder starts inside the plate, at z = 2, for a fuse
// without coplanar faces.)
void test_fillet_boss_s_curve() {
  const TopoDS_Shape plate = BRepPrimAPI_MakeBox(gp_Pnt(-20, -20, 0), gp_Pnt(20, 20, 4)).Shape();
  const TopoDS_Shape boss = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 2), gp_Dir(0, 0, 1)), 5.0, 6.0).Shape();
  BRepAlgoAPI_Fuse fuse(plate, boss);
  CHECK(fuse.IsDone());
  const TopoDS_Shape body = fuse.Shape();
  const std::vector<TopoDS_Edge> circles = edges_where(body, [](const gp_Pnt& m) {
    return std::abs(std::hypot(m.X(), m.Y()) - 5.0) < 1e-6 && (at(m.Z(), 4) || at(m.Z(), 8));
  });
  Answer answer;
  answer.volume = 6400.0 + 100.0 * kPi + (8.0 * kPi / 3.0) * (10.0 - 3.0 * kPi);
  answer.area = 3840.0 - 40.0 * kPi + 20.0 * kPi * kPi;
  answer.relative = 1e-4;
  fillet_case("fillet/boss-s-curve", body, circles, 2, 2.0, answer, Expect{}, Expect{});
}

// fillet/nearly-flat-ridge: a prism 6 high on the pentagon (0, 0), (8, 0),
// (8, 5), (4, 5 + 4 tan 0.5 deg), (0, 5), its roof two faces meeting at 179
// degrees; the ridge rounded with 180, the ball touching each roof face
// 180 tan 0.5 deg from the ridge. The round removes only 0.043 of 240.8, less
// than the tolerance on the volume: the volume removed is checked too.
void test_fillet_nearly_flat_ridge() {
  const double alpha = 0.5 * kPi / 180.0;
  const double r = 180.0;
  const double ridge = 5.0 + 4.0 * std::tan(alpha);
  const TopoDS_Shape body = prism_up({{0, 0}, {8, 0}, {8, 5}, {4, ridge}, {0, 5}}, 6.0);
  const std::vector<TopoDS_Edge> edges =
    edges_where(body, [&](const gp_Pnt& m) { return at(m.X(), 4) && at(m.Y(), ridge) && at(m.Z(), 3); });
  const double section = 40.0 + 16.0 * std::tan(alpha) - r * r * (std::tan(alpha) - alpha);
  Answer answer;
  answer.volume = 6.0 * section;
  answer.area = 2.0 * section + 6.0 * (18.0 + 8.0 / std::cos(alpha) - 2.0 * r * std::tan(alpha) + 2.0 * r * alpha);
  answer.relative = 1e-4;
  answer.change = -6.0 * r * r * (std::tan(alpha) - alpha);
  fillet_case("fillet/nearly-flat-ridge", body, edges, 1, r, answer, Expect{}, Expect{});
}

// ---------------------------------------------------------------- chamfer cases

// chamfer/block-ridge: the block's two long top edges chamfered with 2 on
// both faces, half the top's width: the chamfers meet at right angles on
// the ridge y = 2, z = 6, the top gone.
void test_chamfer_block_ridge() {
  const TopoDS_Shape body = slab();
  Answer answer;
  answer.volume = 200.0;
  answer.area = 160.0 + 40.0 * std::sqrt(2.0);
  chamfer_case("chamfer/block-ridge", body, slab_top_long_edges(body), 2, 2.0, answer, Expect{}, Expect{});
}

// chamfer/l-block-inside-consumed: the inside edge chamfered with 7 on both
// faces, adding material: the chamfer the plane x + z = 13 through both
// faces' far edges, which it consumes.
void test_chamfer_l_block_inside_consumed() {
  const TopoDS_Shape body = l_block();
  Answer answer;
  answer.volume = 755.0;
  answer.area = 411.0 + 70.0 * std::sqrt(2.0);
  chamfer_case("chamfer/l-block-inside-consumed", body, l_block_inside_edge(body), 1, 7.0, answer, Expect{},
               Expect{});
}

// chamfer/octahedron-all-edges: the octahedron |x| + |y| + |z| <= 10, all
// twelve edges chamfered with sqrt(6) on both faces (the chamfer of the
// edge from (10, 0, 0) to (0, 10, 0) the plane x + y = 8), four chamfers
// meeting at each corner, which a square in the plane |x| = 6 (and so on)
// closes: 26 planar faces. Each face shrinks to a triangle whose corners are
// where the contact lines of two of its chamfers cross, exactly on a corner
// square's edge, so each corner touches the faces only at points.
//
// A known failure on both paths at first (mitcad#2273): the kernel's corner
// of more than three stripes (ChFi3d_Builder::PerformMoreThreeCorner) found
// the four chamfers meeting on the faces at each vertex, but its n-sided
// plate, held G1 to each chamfer's plane along its section, was not done at
// any of the six vertices: no surface is tangent to four planes along a
// square's sides. Port patch 2306 builds such a corner, whose borders are
// sections of planar chamfers in one plane, as that plane; and since
// OrientClosedSolid below reverses the solid around the sewn shell, which
// faces in, the same patch has the dressup take a reversed solid as that
// solid oriented forward (before, the eight triangles came out facing in:
// invalid, the volume 490.67) (mitcad#2306).
void test_chamfer_octahedron_all_edges() {
  BRepBuilderAPI_Sewing sewing;
  for (const double sx : {-1.0, 1.0}) {
    for (const double sy : {-1.0, 1.0}) {
      for (const double sz : {-1.0, 1.0}) {
        sewing.Add(polygon({gp_Pnt(10.0 * sx, 0, 0), gp_Pnt(0, 10.0 * sy, 0), gp_Pnt(0, 0, 10.0 * sz)}));
      }
    }
  }
  sewing.Perform();
  const TopoDS_Shape sewn = sewing.SewedShape();
  CHECK(sewn.ShapeType() == TopAbs_SHELL);
  if (sewn.ShapeType() != TopAbs_SHELL) {
    return;
  }
  BRepBuilderAPI_MakeSolid solid(TopoDS::Shell(sewn));
  TopoDS_Solid body = solid.Solid();
  BRepLib::OrientClosedSolid(body);
  CHECK(std::abs(volume_of(body) - 4000.0 / 3.0) < 1e-6);
  const std::vector<TopoDS_Edge> edges = edges_where(body, [](const gp_Pnt&) { return true; });
  Answer answer;
  answer.volume = 2752.0 / 3.0;
  answer.area = 64.0 * std::sqrt(3.0) + 192.0 * std::sqrt(2.0) + 96.0;
  chamfer_case("chamfer/octahedron-all-edges", body, edges, 12, std::sqrt(6.0), answer, Expect{}, Expect{});
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(fillet_bench_cases) {
  test::guarded("fillet/l-block-inside-consumed", test::test_fillet_l_block_inside_consumed);
  test::guarded("fillet/round-swallowing-a-tiny-face", test::test_fillet_round_swallowing_a_tiny_face);
  test::guarded("fillet/slab-half-round", test::test_fillet_slab_half_round);
  test::guarded("fillet/hole-waist", test::test_fillet_hole_waist);
  test::guarded("fillet/boss-s-curve", test::test_fillet_boss_s_curve);
  test::guarded("fillet/nearly-flat-ridge", test::test_fillet_nearly_flat_ridge);
  test::guarded("chamfer/block-ridge", test::test_chamfer_block_ridge);
  test::guarded("chamfer/l-block-inside-consumed", test::test_chamfer_l_block_inside_consumed);
  test::guarded("chamfer/octahedron-all-edges", test::test_chamfer_octahedron_all_edges);
  if (test::failures == 0) {
    std::puts("test_fillet_bench_cases: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
