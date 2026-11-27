// SPDX-License-Identifier: MIT
// Pieces of a face a fillet takes away, left between two of its own blend
// faces, closed by a patch at each place (mitcad#1858).
//
// - The patch itself (detail::patch_remnants), on a rounding of a block's
//   three edges at one corner with its corner face given as the piece: the
//   three roundings are cut back a setback from the corner and the corner
//   with their near parts replaced by one patch tangent to what is left
//   and to the block's faces: one valid closed solid, the patch named as
//   the corner at the block's vertex, the roundings and the block's faces
//   keeping their names, the volume within 1 % of the rounding's. And the
//   same on a block whose edge along x at the corner is split 3 from it
//   (two boxes joined at x = 17): that edge's rounding is one face over
//   both pieces of the edge, named after both, and is cut back past the
//   short piece's end, on along the long one.
// - Two bodies like the places a reported nut design's roundings of 5 left
//   such pieces (3 of 0.42 mm2 and 6 of 0.21 mm2, each between two of the
//   rounding's own faces), rounded through the facade: a slot cut into a
//   block's top edge rounded with 20, its inner corners rounded with 8,
//   the slot's rim on that rounding rounded with 5 along its whole chain
//   (a chain over rounded corners along an earlier rounding's face); and a
//   block's corner rounded with 10 and cut off by a slanted face, the
//   face's rim rounded with 5 (a chain passing from a rounding onto the
//   faces beside it). Each: one valid closed solid, no warning, no piece of
//   an input face under the rounding's reach squared between two of the
//   rounding's faces, built within 20 s. A line tells whether the geometry
//   kernel's own rounding of the chain leaves such pieces there (the case
//   the closure is for) or none.

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdio>
#include <initializer_list>
#include <memory>
#include <set>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeHalfSpace.hxx>
#include <BRep_Tool.hxx>
#include <GC_MakePlane.hxx>
#include <GProp_GProps.hxx>
#include <Geom_Plane.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Solid.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>

#include "../src/remnant_patch.hpp"
#include "check.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;
using Ancestors = NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;

// The body with its faces named N1, N2, ... in TopExp order.
ShapePtr named(const TopoDS_Shape& body) {
  std::vector<Shape::NamedFace> faces;
  int i = 0;
  for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
    faces.push_back({f.Current(), {"N" + std::to_string(++i)}});
  }
  return std::make_shared<Shape>(body, faces);
}

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

double area_of(const TopoDS_Shape& face) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(face, props);
  return props.Mass();
}

bool closed_valid_solid(const TopoDS_Shape& shape) {
  int solids = 0;
  for (TopExp_Explorer solid(shape, TopAbs_SOLID); solid.More(); solid.Next()) {
    ++solids;
    for (TopExp_Explorer shell(solid.Current(), TopAbs_SHELL); shell.More(); shell.Next()) {
      if (!BRep_Tool::IsClosed(shell.Current())) {
        return false;
      }
    }
  }
  return solids == 1 && BRepCheck_Analyzer(shape).IsValid();
}

bool starts_with(const std::string& name, const std::string& prefix) {
  return name.compare(0, prefix.size(), prefix) == 0;
}

bool any_starts_with(const NameList& names, const std::string& prefix) {
  return std::any_of(names.begin(), names.end(), [&](const std::string& n) { return starts_with(n, prefix); });
}

// How many faces hold the name.
int faces_holding(const Shape& shape, const std::string& name) {
  int count = 0;
  for (int f = 0; f < shape.face_count(); ++f) {
    const NameList& names = shape.face_names(f);
    count += std::find(names.begin(), names.end(), name) != names.end() ? 1 : 0;
  }
  return count;
}

int faces_starting(const Shape& shape, const std::string& prefix) {
  int count = 0;
  for (int f = 0; f < shape.face_count(); ++f) {
    count += any_starts_with(shape.face_names(f), prefix) ? 1 : 0;
  }
  return count;
}

// The faces beside each face of the shape (TopExp order of faces).
std::vector<std::set<int>> neighbours(const TopoDS_Shape& shape, const std::vector<TopoDS_Shape>& faces) {
  Ancestors faces_of_edge;
  TopExp::MapShapesAndAncestors(shape, TopAbs_EDGE, TopAbs_FACE, faces_of_edge);
  const auto index = [&](const TopoDS_Shape& face) {
    for (std::size_t i = 0; i < faces.size(); ++i) {
      if (faces[i].IsSame(face)) {
        return static_cast<int>(i);
      }
    }
    return -1;
  };
  std::vector<std::set<int>> out(faces.size());
  for (std::size_t f = 0; f < faces.size(); ++f) {
    for (TopExp_Explorer ex(faces[f], TopAbs_EDGE); ex.More(); ex.Next()) {
      const int i = faces_of_edge.FindIndex(ex.Current());
      if (i == 0) {
        continue;
      }
      for (const TopoDS_Shape& other : faces_of_edge.FindFromIndex(i)) {
        const int g = index(other);
        if (g >= 0 && g != static_cast<int>(f)) {
          out[f].insert(g);
        }
      }
    }
  }
  return out;
}

std::vector<TopoDS_Shape> faces_of(const TopoDS_Shape& shape) {
  std::vector<TopoDS_Shape> faces;
  for (TopExp_Explorer ex(shape, TopAbs_FACE); ex.More(); ex.Next()) {
    if (std::none_of(faces.begin(), faces.end(), [&](const TopoDS_Shape& f) { return f.IsSame(ex.Current()); })) {
      faces.push_back(ex.Current());
    }
  }
  return faces;
}

// Pieces of the result's input faces (no name of the dressup's, a piece's
// "#k") under `limit` mm2 beside two or more of the dressup's faces
// (`prefix`), printed.
int remnants(const Shape& shape, const std::string& prefix, double limit) {
  std::vector<TopoDS_Shape> faces;
  for (int f = 0; f < shape.face_count(); ++f) {
    faces.push_back(shape.face(f));
  }
  const std::vector<std::set<int>> beside = neighbours(shape.occt(), faces);
  int count = 0;
  for (int f = 0; f < shape.face_count(); ++f) {
    const NameList& names = shape.face_names(f);
    if (names.empty() || any_starts_with(names, prefix) || names.front().find('#') == std::string::npos) {
      continue;
    }
    const double area = area_of(shape.face(f));
    if (area >= limit) {
      continue;
    }
    int made = 0;
    for (const int g : beside[static_cast<std::size_t>(f)]) {
      made += any_starts_with(shape.face_names(g), prefix) ? 1 : 0;
    }
    if (made >= 2) {
      std::fprintf(stderr, "  a piece of %.4g mm2 between %d faces of the rounding: %s\n", area, made,
                   names.front().c_str());
      ++count;
    }
  }
  return count;
}

// How many faces of the geometry kernel's own rounding of the edges (OCCT's
// fillet, without the facade) are faces of the body's under `limit` mm2
// beside two or more faces the rounding made: the pieces the closure is
// for. Negative where the kernel does not build it.
int kernel_pieces(const Shape& body, const std::vector<int>& edges, double radius, double limit) {
  try {
    OCC_CATCH_SIGNALS
    BRepFilletAPI_MakeFillet maker(body.occt());
    for (const int e : edges) {
      maker.Add(radius, body.edge(e));
    }
    maker.Build();
    if (!maker.IsDone()) {
      return -1;
    }
    const TopoDS_Shape rounded = maker.Shape();
    const std::vector<TopoDS_Shape> faces = faces_of(rounded);
    std::vector<char> made(faces.size(), 0);
    for (const int e : edges) {
      for (const TopoDS_Shape& generated : maker.Generated(body.edge(e))) {
        for (std::size_t f = 0; f < faces.size(); ++f) {
          made[f] = (made[f] != 0 || faces[f].IsSame(generated)) ? 1 : 0;
        }
      }
    }
    const std::vector<std::set<int>> beside = neighbours(rounded, faces);
    int count = 0;
    for (std::size_t f = 0; f < faces.size(); ++f) {
      if (made[f] != 0 || area_of(faces[f]) >= limit) {
        continue;
      }
      int around = 0;
      for (const int g : beside[f]) {
        around += made[static_cast<std::size_t>(g)] != 0 ? 1 : 0;
      }
      count += around >= 2 ? 1 : 0;
    }
    return count;
  } catch (const Standard_Failure&) {
    return -1;
  }
}

// --- The patch itself ---------------------------------------------------------

// The block (20 x 20 x 20 from the origin, its faces named) rounded with 4
// along the edges at its corner (20, 20, 20); the corner face given as the
// piece. `rounding_faces` of the result are the roundings'.
void patch_at_the_corner_of(const ShapePtr& block, int rounding_faces) {
  std::vector<std::string> edges;
  for (int e = 0; e < block->edge_count(); ++e) {
    TopoDS_Vertex a;
    TopoDS_Vertex b;
    TopExp::Vertices(block->edge(e), a, b);
    for (const TopoDS_Vertex& v : {a, b}) {
      if (BRep_Tool::Pnt(v).Distance(gp_Pnt(20, 20, 20)) < 1e-9) {
        edges.push_back(block->edge_name(e));
      }
    }
  }
  CHECK(edges.size() == 3u);
  const ShapePtr rounded = fillet("K", *block, edges, 4.0);
  // (The roundings run on along the edges' tangent chains.)
  CHECK(rounded != nullptr);
  if (!rounded) {
    return;
  }
  std::vector<int> pieces;
  std::vector<char> made(static_cast<std::size_t>(rounded->face_count()), 0);
  for (int f = 0; f < rounded->face_count(); ++f) {
    const NameList& names = rounded->face_names(f);
    made[static_cast<std::size_t>(f)] = any_starts_with(names, "K:") ? 1 : 0;
    if (any_starts_with(names, "K:corner(")) {
      pieces.push_back(f);
    }
  }
  CHECK(pieces.size() == 1u);
  if (pieces.size() != 1u) {
    return;
  }
  const ShapePtr patched = detail::patch_remnants(*rounded, *block, pieces, made, "K", "fillet", 4.0);
  CHECK(patched != nullptr);
  if (!patched) {
    return;
  }
  CHECK(closed_valid_solid(patched->occt()));
  CHECK(faces_starting(*patched, "K:corner(") == 1);
  // (The patch named after the block's corner: the input's vertex there.)
  for (int f = 0; f < patched->face_count(); ++f) {
    const NameList& names = patched->face_names(f);
    if (!any_starts_with(names, "K:corner(")) {
      continue;
    }
    const std::string& name = names.front();
    const std::string vertex = name.substr(9, name.size() - 10);
    std::vector<int> found;
    try {
      found = block->find_vertices(vertex);
    } catch (const std::exception&) {
      found.clear();
    }
    CHECK(found.size() == 1u);
    if (found.size() == 1u) {
      CHECK(BRep_Tool::Pnt(TopoDS::Vertex(block->vertex(found.front()))).Distance(gp_Pnt(20, 20, 20)) < 1e-9);
    }
  }
  CHECK(faces_starting(*patched, "K:fillet(") == rounding_faces);
  for (int f = 0; f < block->face_count(); ++f) {
    CHECK(faces_holding(*patched, block->face_names(f).front()) >= 1);
  }
  // The patch replaces the corner's sphere and the roundings' near parts.
  for (int f = 0; f < patched->face_count(); ++f) {
    if (any_starts_with(patched->face_names(f), "K:corner(")) {
      CHECK(BRepAdaptor_Surface(patched->face(f)).GetType() != GeomAbs_Sphere);
    }
  }
  CHECK_NEAR_TOL(volume_of(patched->occt()), volume_of(rounded->occt()), 1e-2);
}

void patch_at_a_corner() {
  patch_at_the_corner_of(named(BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), 20, 20, 20).Shape()), 3);
}

// The block as two boxes joined at x = 17: its faces across that plane and
// its edges along x in two pieces each, the one at the corner 3 long. The
// cut back of that edge's rounding, the corner's size and half the reach
// past the corner (over 7), runs past the short piece's end, on along the
// long piece (cut_along).
void patch_at_a_corner_past_an_edge_end() {
  const TopoDS_Shape joined = BRepAlgoAPI_Fuse(BRepPrimAPI_MakeBox(gp_Pnt(0, 0, 0), 17, 20, 20).Shape(),
                                               BRepPrimAPI_MakeBox(gp_Pnt(17, 0, 0), 3, 20, 20).Shape())
                                .Shape();
  int      along_x = 0;
  ShapeMap all_edges;
  TopExp::MapShapes(joined, TopAbs_EDGE, all_edges);
  for (int e = 1; e <= all_edges.Extent(); ++e) {
    TopoDS_Vertex a;
    TopoDS_Vertex b;
    TopExp::Vertices(TopoDS::Edge(all_edges(e)), a, b);
    const gp_Pnt p = BRep_Tool::Pnt(a), q = BRep_Tool::Pnt(b);
    along_x += std::abs(p.Y() - 20) < 1e-9 && std::abs(q.Y() - 20) < 1e-9 && std::abs(p.Z() - 20) < 1e-9 &&
                       std::abs(q.Z() - 20) < 1e-9
                   ? 1
                   : 0;
  }
  CHECK(along_x == 2);
  patch_at_the_corner_of(named(joined), 3);
}

// --- Bodies like the reported design's -------------------------------------

// The edges of the body whose points at a quarter, half and three quarters
// all satisfy `on`.
template <class On>
std::vector<int> edges_where(const Shape& body, On&& on) {
  std::vector<int> out;
  for (int e = 0; e < body.edge_count(); ++e) {
    if (BRep_Tool::Degenerated(body.edge(e))) {
      continue;
    }
    const BRepAdaptor_Curve curve(body.edge(e));
    bool all = true;
    for (int k = 1; k <= 3 && all; ++k) {
      all = on(curve.Value(curve.FirstParameter() + (curve.LastParameter() - curve.FirstParameter()) * k / 4.0));
    }
    if (all) {
      out.push_back(e);
    }
  }
  return out;
}

// The chain rounded with `radius` through the facade, and the checks.
void round_chain(const char* what, const Shape& body, const std::vector<int>& chain, double radius) {
  std::fprintf(stderr, "%s: %zu edges, rounded with %g\n", what, chain.size(), radius);
  CHECK(!chain.empty());
  if (chain.empty()) {
    return;
  }
  const double limit = radius * radius;
  const int left = kernel_pieces(body, chain, radius, limit);
  if (left < 0) {
    std::fprintf(stderr, "  the kernel's own rounding does not build\n");
  } else {
    std::fprintf(stderr, "  the kernel's own rounding leaves %d pieces between its faces\n", left);
  }
  std::vector<std::string> edges;
  for (const int e : chain) {
    edges.push_back(body.edge_name(e));
  }
  const auto start = std::chrono::steady_clock::now();
  ShapePtr result;
  try {
    result = fillet("R", body, edges, radius);
  } catch (const std::exception& error) {
    std::fprintf(stderr, "  failed: %s\n", error.what());
  }
  const double seconds = std::chrono::duration<double>(std::chrono::steady_clock::now() - start).count();
  std::fprintf(stderr, "  built in %.2f s\n", seconds);
  CHECK(result != nullptr);
  if (!result) {
    return;
  }
  for (const std::string& note : result->notes()) {
    std::fprintf(stderr, "  note: %s\n", note.c_str());
  }
  CHECK(closed_valid_solid(result->occt()));
  CHECK(result->notes().empty());
  CHECK(remnants(*result, "R:", limit) == 0);
  CHECK(faces_starting(*result, "R:fillet(") >= static_cast<int>(chain.size()));
  CHECK(seconds < 20.0);
}

// A block 80 x 80 x 40, its top edge along x at y = 40 rounded with 20 (a
// quarter cylinder about x at y = z = 20), a slot 20 wide cut into that
// edge down to z = 10 from y = 25 on, its two inner corners rounded with
// 8; the slot's rim on the rounding (walls, corners and back wall against
// the quarter cylinder) rounded with 5.
void slot_in_a_rounded_edge() {
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(-40, -40, 0), 80, 80, 40).Shape();
  BRepFilletAPI_MakeFillet edge(block);
  for (TopExp_Explorer ex(block, TopAbs_EDGE); ex.More(); ex.Next()) {
    const BRepAdaptor_Curve curve(TopoDS::Edge(ex.Current()));
    const gp_Pnt m = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
    if (std::abs(m.Y() - 40) < 1e-9 && std::abs(m.Z() - 40) < 1e-9) {
      edge.Add(20.0, TopoDS::Edge(ex.Current()));
    }
  }
  edge.Build();
  CHECK(edge.IsDone());
  if (!edge.IsDone()) {
    return;
  }
  const TopoDS_Shape tool = BRepPrimAPI_MakeBox(gp_Pnt(-10, 25, 10), 20, 30, 40).Shape();
  BRepFilletAPI_MakeFillet corners(tool);
  for (TopExp_Explorer ex(tool, TopAbs_EDGE); ex.More(); ex.Next()) {
    const BRepAdaptor_Curve curve(TopoDS::Edge(ex.Current()));
    const gp_Pnt m = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
    if (std::abs(m.Y() - 25) < 1e-9 && std::abs(std::abs(m.X()) - 10) < 1e-9 && std::abs(m.Z() - 30) < 1e-9) {
      corners.Add(8.0, TopoDS::Edge(ex.Current()));
    }
  }
  corners.Build();
  CHECK(corners.IsDone());
  if (!corners.IsDone()) {
    return;
  }
  const ShapePtr body = named(BRepAlgoAPI_Cut(edge.Shape(), corners.Shape()).Shape());
  // The rim: on the quarter cylinder, within the slot.
  const std::vector<int> chain = edges_where(*body, [](const gp_Pnt& p) {
    return std::abs(std::hypot(p.Y() - 20, p.Z() - 20) - 20) < 1e-6 && p.Y() > 25 - 1e-6 && p.Z() > 20 + 1e-6 &&
           std::abs(p.X()) < 10 + 1e-6;
  });
  round_chain("a slot in a rounded edge", *body, chain, 5.0);
}

// A block 60 x 60 x 30, its vertical edge at x = y = 30 rounded with 10,
// the top corner there cut off by the plane through (30, 10, 30), (10, 30,
// 30) and (30, 30, 14); the cut face's rim on the sides and the rounding
// rounded with 5.
void corner_cut_across_a_rounding() {
  const TopoDS_Shape block = BRepPrimAPI_MakeBox(gp_Pnt(-30, -30, 0), 60, 60, 30).Shape();
  BRepFilletAPI_MakeFillet edge(block);
  for (TopExp_Explorer ex(block, TopAbs_EDGE); ex.More(); ex.Next()) {
    const BRepAdaptor_Curve curve(TopoDS::Edge(ex.Current()));
    const gp_Pnt m = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
    if (std::abs(m.X() - 30) < 1e-9 && std::abs(m.Y() - 30) < 1e-9) {
      edge.Add(10.0, TopoDS::Edge(ex.Current()));
    }
  }
  edge.Build();
  CHECK(edge.IsDone());
  if (!edge.IsDone()) {
    return;
  }
  const gp_Pnt a(30, 10, 30);
  const gp_Pnt b(10, 30, 30);
  const gp_Pnt c(30, 30, 14);
  const GC_MakePlane plane(a, b, c);
  CHECK(plane.IsDone());
  if (!plane.IsDone()) {
    return;
  }
  const gp_Pln pln = plane.Value()->Pln();
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(pln, -200, 200, -200, 200).Face();
  // The side of the plane away from the block's middle.
  const TopoDS_Solid away = BRepPrimAPI_MakeHalfSpace(face, gp_Pnt(40, 40, 40)).Solid();
  const ShapePtr body = named(BRepAlgoAPI_Cut(edge.Shape(), away).Shape());
  // The rim: on the plane, below the top.
  const std::vector<int> chain = edges_where(*body, [&](const gp_Pnt& p) {
    return pln.Distance(p) < 1e-6 && p.Z() < 30 - 1e-6;
  });
  round_chain("a corner cut across a rounding", *body, chain, 5.0);
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(dressup_blend_remnants) {
  test::guarded("patch at a corner", test::patch_at_a_corner);
  test::guarded("patch at a corner past an edge end", test::patch_at_a_corner_past_an_edge_end);
  test::guarded("slot in a rounded edge", test::slot_in_a_rounded_edge);
  test::guarded("corner cut across a rounding", test::corner_cut_across_a_rounding);
  if (test::failures == 0) {
    std::puts("test_dressup_blend_remnants: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
