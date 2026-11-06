// SPDX-License-Identifier: MIT
// A fin's top chamfered and rounded across slight creases of its profile
// (mitcad#1341), with OCCT's chamfer and fillet alone, on the case
// `fin-top-chamfer-rounding` of MITCAD_GEOMETRY_CASES: a reported design's
// fin, 1 thick, whose top runs as arcs and spline stretches meeting at
// creases of one or two degrees.
//
// - `before-chamfer.brep`, the design's stored body before the top's
//   chamfer, and `chamfer-edges.txt`, the indices of the top's two chains
//   (TopExp order; the second number marks the chain on the other side):
//   chamfered by 0.263 at 45 degrees, each chamfer face is one that OCCT's
//   history gives a contour edge, also those of the parts that the narrow
//   end face's cut makes (the port's patch 1302).
// - `body.brep`, that body chamfered, and `edges.txt`, the indices of the
//   top's edges the design rounds: rounded with 0.2 the fillet builds, its
//   result closed (every edge on two faces), and the design's stored body
//   after the rounding, `after.brep`, lies on the result within 0.01 mm
//   round the fin's top. Without the port's patch 1300 the walk after the
//   arc's torus rolled on the top's crease, convex though OCCT took it for
//   concave, and failed; without 1301 a walk stopped a few micrometres
//   short of the next torus and its piece left the result open.
//
// OCCT's own solids cannot model the fin's spline profile with its
// creases, so the case is the reported body.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
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

// 45 degrees.
constexpr double kQuarterTurnHalf = 0.78539816339744831;

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

// The lines of a list: an index and, optionally, a mark.
std::vector<std::pair<int, int>> read_indices(const std::string& path) {
  std::vector<std::pair<int, int>> out;
  std::ifstream                    in(path);
  std::string                      line;
  while (std::getline(in, line)) {
    int index = -1, mark = 0;
    if (std::sscanf(line.c_str(), "%d %d", &index, &mark) >= 1 && index >= 0) {
      out.emplace_back(index, mark);
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

// The faces of a shape whose box meets the box about the fin's top.
TopoDS_Compound top_faces(const TopoDS_Shape& shape, const Bnd_Box& region) {
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

// The farthest of points inside the faces of `from` about the fin's top
// from the faces of `to` there.
double farthest(const TopoDS_Shape& from, const TopoDS_Shape& to, const Bnd_Box& region) {
  Bnd_Box wider = region;
  wider.Enlarge(1.);
  const TopoDS_Compound target = top_faces(to, wider);
  double                worst  = 0.;
  for (TopExp_Explorer f(top_faces(from, region), TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face face = TopoDS::Face(f.Current());
    double            u0 = 0., u1 = 0., v0 = 0., v1 = 0.;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    const BRepAdaptor_Surface s(face);
    for (int i = 0; i <= 6; ++i) {
      for (int j = 0; j <= 6; ++j) {
        const double u = u0 + (u1 - u0) * (0.05 + 0.9 * i / 6.);
        const double v = v0 + (v1 - v0) * (0.05 + 0.9 * j / 6.);
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

void test_chamfer_faces_are_generated_by_their_edges() {
  const std::string cases = geometry_cases();
  if (cases.empty()) {
    std::printf("test_chamfer_faces_are_generated_by_their_edges: skipped (MITCAD_GEOMETRY_CASES is not set)\n");
    return;
  }
  const std::string  dir  = cases + "/fin-top-chamfer-rounding/";
  const TopoDS_Shape body = read(dir + "before-chamfer.brep");
  const auto         list = read_indices(dir + "chamfer-edges.txt");
  CHECK(!body.IsNull() && list.size() == 8);
  if (body.IsNull() || list.size() != 8) {
    return;
  }
  ShapeIndex edges;
  TopExp::MapShapes(body, TopAbs_EDGE, edges);
  EdgeFaces edge_faces;
  TopExp::MapShapesAndAncestors(body, TopAbs_EDGE, TopAbs_FACE, edge_faces);
  BRepFilletAPI_MakeChamfer                              chamfer(body);
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> selected;
  for (const auto& item : list) {
    selected.Add(edges(item.first + 1));
  }
  chamfer.StopChainsAtTurns(selected);
  for (const auto& item : list) {
    const TopoDS_Edge& edge = TopoDS::Edge(edges(item.first + 1));
    if (chamfer.Contour(edge) == 0) {
      // (At 45 degrees either face measures the same chamfer.)
      chamfer.AddDA(0.263, kQuarterTurnHalf, edge, TopoDS::Face(edge_faces.FindFromKey(edge).First()));
    }
  }
  chamfer.Build();
  CHECK(chamfer.IsDone());
  if (!chamfer.IsDone()) {
    return;
  }
  ShapeIndex made;
  for (int ic = 1; ic <= chamfer.NbContours(); ++ic) {
    for (int ie = 1; ie <= chamfer.NbEdges(ic); ++ie) {
      for (const TopoDS_Shape& image : chamfer.Generated(chamfer.Edge(ic, ie))) {
        if (image.ShapeType() == TopAbs_FACE) {
          made.Add(image);
        }
      }
    }
  }
  // The chamfer's faces: those of the result that are no faces of the body
  // and no modified ones.
  ShapeIndex old;
  TopExp::MapShapes(body, TopAbs_FACE, old);
  int new_faces = 0, missing = 0;
  ShapeIndex faces;
  TopExp::MapShapes(chamfer.Shape(), TopAbs_FACE, faces);
  for (int f = 1; f <= faces.Extent(); ++f) {
    if (old.Contains(faces(f))) {
      continue;
    }
    bool modified = false;
    for (int k = 1; k <= old.Extent() && !modified; ++k) {
      for (const TopoDS_Shape& image : chamfer.Modified(old(k))) {
        modified = modified || image.IsSame(faces(f));
      }
    }
    if (modified) {
      continue;
    }
    ++new_faces;
    if (!made.Contains(faces(f))) {
      ++missing;
    }
  }
  std::printf("chamfer: %d new faces, %d not generated by a contour edge\n", new_faces, missing);
  CHECK(new_faces == 10);
  CHECK(missing == 0);
}

void test_rounding_across_the_creases() {
  const std::string cases = geometry_cases();
  if (cases.empty()) {
    std::printf("test_rounding_across_the_creases: skipped (MITCAD_GEOMETRY_CASES is not set)\n");
    return;
  }
  const std::string  dir   = cases + "/fin-top-chamfer-rounding/";
  const TopoDS_Shape body  = read(dir + "body.brep");
  const TopoDS_Shape after = read(dir + "after.brep");
  const auto         list  = read_indices(dir + "edges.txt");
  CHECK(!body.IsNull() && !after.IsNull() && list.size() == 12);
  if (body.IsNull() || after.IsNull() || list.size() != 12) {
    return;
  }
  ShapeIndex edges;
  TopExp::MapShapes(body, TopAbs_EDGE, edges);
  // As MitCad's dressup calls the fillet: chains stopped at turns, carried
  // across notches.
  BRepFilletAPI_MakeFillet                               fillet(body);
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> selected;
  for (const auto& item : list) {
    selected.Add(edges(item.first + 1));
  }
  fillet.StopChainsAtTurns(selected);
  Bnd_Box region;
  for (const auto& item : list) {
    const TopoDS_Edge& edge = TopoDS::Edge(edges(item.first + 1));
    BRepBndLib::Add(edge, region);
    if (fillet.Contour(edge) == 0) {
      fillet.Add(0.2, edge);
    }
  }
  fillet.CarryChainsAcrossNotches();
  fillet.Build();
  CHECK(fillet.IsDone());
  if (!fillet.IsDone()) {
    std::printf("the rounding failed: %d faulty contours\n", fillet.NbFaultyContours());
    return;
  }
  const TopoDS_Shape result = fillet.Shape();
  CHECK(closed(result));
  // The stored body on the result round the fin's top (the rounded edges'
  // box, above the root's roundings).
  double x0 = 0., y0 = 0., z0 = 0., x1 = 0., y1 = 0., z1 = 0.;
  region.Get(x0, y0, z0, x1, y1, z1);
  Bnd_Box top;
  top.Update(x0, y0, std::max(z0, z1 - 20.), x1, y1, z1);
  const double back = farthest(after, result, top);
  std::printf("the stored body on the result within %.6f mm\n", back);
  CHECK(back <= 0.01);
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_chamfer_faces_are_generated_by_their_edges", test::test_chamfer_faces_are_generated_by_their_edges);
  test::guarded("test_rounding_across_the_creases", test::test_rounding_across_the_creases);
  if (test::failures == 0) {
    std::puts("test_fillet_fin_top_creases: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
