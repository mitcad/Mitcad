// SPDX-License-Identifier: MIT
// A plate's side outlines rounded along an earlier rounding of its top edge
// that vanishes into the side (mitcad#1346), with OCCT's fillet alone, on the
// case `plate-side-vanishing-rounding` of MITCAD_GEOMETRY_CASES from a
// reported design.
//
// `body.brep` is the plate as MitCad builds it before the rounding: its top
// edge rounded earlier, that rounding coming tangent to the plate's sides
// at a vertex on each side, where it vanishes into the side. The edges of
// `edges.txt`, the side outlines at x = -7 and x = 7, are rounded with 0.3;
// on each side the chain runs along the earlier rounding up to that vertex,
// where the walk ends with both contacts at the vertex, on the side's edge.
// The stored body after the rounding, `after.brep`, ends each side's
// rounding in a point there. Within the box of `ends.txt` (about both
// vertices) the result lies on the stored body within 0.01 mm both ways,
// and the result is valid. Without the port's patch 1320 the contact on the
// earlier rounding ended at the walk's other side, the end's degenerated
// edge crossed the rounding's face, and the faces' wires crossed (an
// invalid result on both sides).
//
// The vertex where a rounding comes tangent to the side is that design's
// own, so the case is the reported body.

#include <algorithm>
#include <cstdio>
#include <sstream>
#include <cstdlib>
#include <fstream>
#include <string>
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

// The indices of a list (the first number of each line).
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

// A box from a line of six numbers (its corners), void when there is none.
Bnd_Box read_box(const std::string& path) {
  Bnd_Box       box;
  std::ifstream in(path);
  double        x0 = 0., y0 = 0., z0 = 0., x1 = 0., y1 = 0., z1 = 0.;
  if (in >> x0 >> y0 >> z0 >> x1 >> y1 >> z1) {
    box.Update(x0, y0, z0, x1, y1, z1);
  }
  return box;
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

// The faces of a shape whose box meets the region.
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

// The farthest of points inside the faces of `from` within the region from
// the faces of `to` about it.
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

// The fillet of the listed edges of a body with one radius, as MitCad's
// dressup calls it (chains stopped at turns, carried across notches). A null
// shape where it fails.
TopoDS_Shape round_edges(const TopoDS_Shape& body, const std::vector<int>& list, double radius) {
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
      fillet.Add(radius, edge);
    }
  }
  fillet.CarryChainsAcrossNotches();
  fillet.Build();
  if (!fillet.IsDone()) {
    std::printf("the rounding failed: %d faulty contours\n", fillet.NbFaultyContours());
    return TopoDS_Shape();
  }
  return fillet.Shape();
}

void test_side_rounding_vanishes_with_the_earlier_rounding() {
  const std::string cases = geometry_cases();
  if (cases.empty()) {
    std::printf("test_side_rounding_vanishes_with_the_earlier_rounding: skipped (MITCAD_GEOMETRY_CASES is not set)\n");
    return;
  }
  const std::string  dir   = cases + "/plate-side-vanishing-rounding/";
  const TopoDS_Shape body  = read(dir + "body.brep");
  const TopoDS_Shape after = read(dir + "after.brep");
  const auto         list  = read_indices(dir + "edges.txt");
  const Bnd_Box      ends  = read_box(dir + "ends.txt");
  CHECK(!body.IsNull() && !after.IsNull() && list.size() == 22 && !ends.IsVoid());
  if (body.IsNull() || after.IsNull() || list.size() != 22 || ends.IsVoid()) {
    return;
  }
  const TopoDS_Shape result = round_edges(body, list, 0.3);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(closed(result));
  CHECK(BRepCheck_Analyzer(result).IsValid());
  // The result on the stored body about the two vanishing ends, and back.
  const double out  = farthest(result, after, ends);
  const double back = farthest(after, result, ends);
  std::printf("about the vanishing ends: the result on the stored body within %.6f mm, the stored body on it "
              "within %.6f mm\n",
              out,
              back);
  CHECK(out <= 0.01);
  CHECK(back <= 0.01);
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_side_rounding_vanishes_with_the_earlier_rounding",
                test::test_side_rounding_vanishes_with_the_earlier_rounding);
  if (test::failures == 0) {
    std::puts("test_fillet_vanishing_side: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
