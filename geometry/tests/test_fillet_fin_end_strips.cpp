// SPDX-License-Identifier: MIT
// A fin's side strips rounded against its root's chamfer with a ball wider
// than the strips (mitcad#1388), with OCCT's fillet alone, on the case
// `fin-end-strips` of MITCAD_GEOMETRY_CASES: a reported design's fin, 1
// thick, whose sides keep strips about 0.05 high between the arcs' chamfer
// (cones) and the root's chamfer, each strip narrowing to a point at its
// ends under a transition piece of the arc's chamfer (or the fin's end
// face), rounded with 0.86.
//
// - `body.brep`, the design's body before the rounding as MitCad's import
//   makes it (its root's chamfer a spline where the stored body has a cone,
//   0.003 mm off it; its top chamfer's spline pieces joined), with
//   `edges.txt`, the indices (TopExp order) of the eleven edges the stored
//   body rounds: two strips beside the fin's middle arc, the strip beside
//   its end arc on one side, and the rim round the fin's end. The ball
//   rolls on each strip's edge with the arc's chamfer, past that edge's
//   ends, and the transition pieces cut it (the stored body's tori); along
//   the end face it rolls on the strip's edges with that face. The result
//   is closed and valid, has the stored body's faces (`after.brep`), and
//   about the strips and the rim the result and the stored body lie on
//   each other within 0.01 mm. Without the port's patch 1420 the strip
//   beside the end arc failed, its walk starting beside the strip's point
//   or rolling on along the transition piece's edge into it.
// - `near-wall.txt`, the two edges of that strip alone, in both orders
//   (the spine's direction): each builds, as the stored body's faces there.
// - `state.brep`, the stored body before the rounding, with
//   `state-edges.txt`: the same rounding has the stored body's faces there
//   too (with or without the patch).
//
// OCCT's own solids cannot model the fin's profile with its chamfer's
// transition pieces, so the case is the reported body.

#include <algorithm>
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <sstream>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_IndexedMap.hxx>
#include <NCollection_List.hxx>
#include <NCollection_Map.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>

#include "check.hpp"

namespace test {
namespace {

using ShapeIndex = NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher>;
using EdgeFaces  = NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;

// The rounding's radius.
constexpr double kRadius = 0.86;

std::string geometry_cases() {
#ifdef _MSC_VER
  char*       value  = nullptr;
  std::size_t length = 0;
  std::string out;
  if (_dupenv_s(&value, &length, "MITCAD_GEOMETRY_CASES") == 0 && value != nullptr) {
    out = value;
    std::free(value);
  }
  return out;
#else
  const char* value = std::getenv("MITCAD_GEOMETRY_CASES");
  return value != nullptr ? std::string(value) : std::string();
#endif
}

TopoDS_Shape read(const std::string& path) {
  TopoDS_Shape shape;
  BRep_Builder builder;
  BRepTools::Read(shape, path.c_str(), builder);
  return shape;
}

// The indices of a list, one a line.
std::vector<int> read_indices(const std::string& path) {
  std::vector<int> out;
  std::ifstream    in(path);
  std::string      line;
  while (std::getline(in, line)) {
    std::istringstream fields(line);
    int                index = -1;
    if (fields >> index && index >= 0) {
      out.push_back(index);
    }
  }
  return out;
}

// Whether every edge of the shape lies on two faces (a closed shell).
bool closed(const TopoDS_Shape& shape) {
  EdgeFaces map;
  TopExp::MapShapesAndAncestors(shape, TopAbs_EDGE, TopAbs_FACE, map);
  for (int i = 1; i <= map.Extent(); ++i) {
    if (!BRep_Tool::Degenerated(TopoDS::Edge(map.FindKey(i))) && map(i).Extent() != 2) {
      return false;
    }
  }
  return true;
}

int face_count(const TopoDS_Shape& shape) {
  ShapeIndex faces;
  TopExp::MapShapes(shape, TopAbs_FACE, faces);
  return faces.Extent();
}

// The faces of a shape whose box meets `region`.
TopoDS_Compound faces_in(const TopoDS_Shape& shape, const Bnd_Box& region) {
  TopoDS_Compound out;
  BRep_Builder    builder;
  builder.MakeCompound(out);
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    Bnd_Box box;
    BRepBndLib::Add(f.Current(), box);
    if (!box.IsOut(region)) {
      builder.Add(out, f.Current());
    }
  }
  return out;
}

// The farthest of points inside the faces of `from` in `region` from the
// faces of `to` there.
double farthest(const TopoDS_Shape& from, const TopoDS_Shape& to, const Bnd_Box& region) {
  Bnd_Box wider = region;
  wider.Enlarge(1.);
  const TopoDS_Compound target = faces_in(to, wider);
  double                worst  = 0.;
  for (TopExp_Explorer f(faces_in(from, region), TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face face = TopoDS::Face(f.Current());
    double            u0 = 0., u1 = 0., v0 = 0., v1 = 0.;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    const BRepAdaptor_Surface s(face);
    for (int i = 0; i <= 8; ++i) {
      for (int j = 0; j <= 8; ++j) {
        const double u = u0 + (u1 - u0) * (0.02 + 0.96 * i / 8.);
        const double v = v0 + (v1 - v0) * (0.02 + 0.96 * j / 8.);
        if (BRepClass_FaceClassifier(face, gp_Pnt2d(u, v), 1e-7).State() != TopAbs_IN) {
          continue;
        }
        const gp_Pnt p = s.Value(u, v);
        if (region.IsOut(p)) {
          continue;
        }
        BRepExtrema_DistShapeShape d(BRepBuilderAPI_MakeVertex(p).Vertex(), target);
        if (d.IsDone()) {
          worst = std::max(worst, d.Value());
        }
      }
    }
  }
  return worst;
}

// The edges in `list` rounded with 0.86 as MitCad's dressup calls the
// fillet: chains stopped at turns, carried across notches. Null where the
// fillet fails.
TopoDS_Shape round_edges(const TopoDS_Shape& body, const std::vector<int>& list) {
  ShapeIndex edges;
  TopExp::MapShapes(body, TopAbs_EDGE, edges);
  BRepFilletAPI_MakeFillet                               fillet(body);
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> selected;
  for (const int index : list) {
    selected.Add(edges(index + 1));
  }
  fillet.StopChainsAtTurns(selected);
  for (const int index : list) {
    const TopoDS_Edge& edge = TopoDS::Edge(edges(index + 1));
    if (fillet.Contour(edge) == 0) {
      fillet.Add(kRadius, edge);
    }
  }
  fillet.CarryChainsAcrossNotches();
  fillet.Build();
  if (!fillet.IsDone()) {
    std::printf("the rounding failed: %d faulty contours, %d faulty vertices\n", fillet.NbFaultyContours(),
                fillet.NbFaultyVertices());
    return TopoDS_Shape();
  }
  return fillet.Shape();
}

// About the strips beside the middle arc, the strip beside the end arc
// (the side at x = -0.5) and the rim round the fin's end.
struct Region {
  const char* name;
  double      box[6];
};
constexpr Region kStrips[] = {{"the middle arc's strips", {-1., -27., 178., 1., -16., 195.}},
                              {"the end arc's strip", {-1.5, -11., 197., 0., 10., 203.}},
                              {"the rim", {-3., 11., 196., 3., 15., 201.}}};

// The result and the stored body on each other about `regions`.
void check_on_stored(const TopoDS_Shape& result, const TopoDS_Shape& after, const Region* regions, int count) {
  for (int k = 0; k < count; ++k) {
    const Region& r = regions[k];
    Bnd_Box       box;
    box.Update(r.box[0], r.box[1], r.box[2], r.box[3], r.box[4], r.box[5]);
    const double out  = farthest(result, after, box);
    const double back = farthest(after, result, box);
    std::printf("%s: the result on the stored body within %.6f mm, the stored body on it within %.6f mm\n", r.name,
                out, back);
    CHECK(out <= 0.01);
    CHECK(back <= 0.01);
  }
}

bool load(const char* name, TopoDS_Shape& body, TopoDS_Shape& after, std::vector<int>& list, const char* body_file,
          const char* list_file, std::size_t count) {
  const std::string cases = geometry_cases();
  if (cases.empty()) {
    std::printf("%s: skipped (MITCAD_GEOMETRY_CASES is not set)\n", name);
    return false;
  }
  const std::string dir = cases + "/fin-end-strips/";
  body                  = read(dir + body_file);
  after                 = read(dir + "after.brep");
  list                  = read_indices(dir + list_file);
  CHECK(!body.IsNull() && !after.IsNull() && list.size() == count);
  return !body.IsNull() && !after.IsNull() && list.size() == count;
}

void test_strips_rolled_on_past_their_edges() {
  TopoDS_Shape     body, after;
  std::vector<int> list;
  if (!load("test_strips_rolled_on_past_their_edges", body, after, list, "body.brep", "edges.txt", 11)) {
    return;
  }
  const TopoDS_Shape result = round_edges(body, list);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(closed(result));
  CHECK(BRepCheck_Analyzer(result).IsValid());
  const int faces = face_count(result), stored = face_count(after);
  std::printf("the rounding: %d faces, the stored body %d\n", faces, stored);
  CHECK(faces == stored);
  check_on_stored(result, after, kStrips, 3);
}

void test_end_arc_strip_either_way() {
  TopoDS_Shape     body, after;
  std::vector<int> list;
  if (!load("test_end_arc_strip_either_way", body, after, list, "body.brep", "near-wall.txt", 2)) {
    return;
  }
  for (int order = 0; order < 2; ++order) {
    const TopoDS_Shape result = round_edges(body, list);
    CHECK(!result.IsNull());
    if (!result.IsNull()) {
      CHECK(closed(result));
      CHECK(BRepCheck_Analyzer(result).IsValid());
      std::printf("the end arc's strip alone, edges in %s order: %d faces\n", order == 0 ? "one" : "the other",
                  face_count(result));
      check_on_stored(result, after, kStrips + 1, 1);
    }
    std::reverse(list.begin(), list.end());
  }
}

void test_stored_state() {
  TopoDS_Shape     body, after;
  std::vector<int> list;
  if (!load("test_stored_state", body, after, list, "state.brep", "state-edges.txt", 11)) {
    return;
  }
  const TopoDS_Shape result = round_edges(body, list);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(closed(result));
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(face_count(result) == face_count(after));
  check_on_stored(result, after, kStrips, 3);
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_strips_rolled_on_past_their_edges", test::test_strips_rolled_on_past_their_edges);
  test::guarded("test_end_arc_strip_either_way", test::test_end_arc_strip_either_way);
  test::guarded("test_stored_state", test::test_stored_state);
  if (test::failures == 0) {
    std::puts("test_fillet_fin_end_strips: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
