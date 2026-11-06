// SPDX-License-Identifier: MIT
// A fin's top rounded up to its end face, and the fin's end rounded along
// its sides up to the top (mitcad#1345), with OCCT's fillet alone, on cases
// of MITCAD_GEOMETRY_CASES from a reported design's fin, 1 thick, whose top
// is chamfered and whose end face, an extrusion of a profile's spline
// stretch, goes on from the top's arc tangentially and falls away from it.
//
// - The case `fin-top-chamfer-rounding` (mitcad#1341): `body.brep`, the fin
//   with its top chamfered, rounded with 0.2 along the top's edges in
//   `edges.txt`. Each side's rounding ends at the vertex where the top's arc,
//   its chamfer and the end face meet; the end face takes the top on
//   tangentially there. The stored body after the rounding, `after.brep`,
//   runs each side's torus on past that vertex, cut by the end face up to
//   where it meets the chamfer's edge with the end face: within the box of
//   `fin-end.txt` (of the case below) the result lies on the stored body
//   within 0.002 mm. Without the port's patch 1310 one side's torus ended at
//   the vertex and a corner closed it, 0.013 off the stored body.
// - The case `fin-end-rounding`: `body.brep`, the fin's body as MitCad
//   builds it before the end's rounding (its end face the extrusion of the
//   spline's stretch alone), rounded with 1 along the end face's edges in
//   `edges.txt`: the rounding builds, closed, and the stored body after it,
//   `after.brep`, lies on the result within 0.01 mm round the end. Each
//   side's rounding runs on past the vertex at the top of its chain with
//   its contact on the end face's surface past the top, beyond the stretch
//   the face is an extrusion of; without the port's patch 1311 the walk
//   there fell off that surface and the two sides' roundings were taken
//   for intersecting.
//
// OCCT's own solids cannot model the fin's spline profile, so the cases are
// the reported bodies.

#include <algorithm>
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <string>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
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
    int index = -1;
    if (std::sscanf(line.c_str(), "%d", &index) == 1 && index >= 0) {
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
// dressup calls it (chains stopped at turns, carried across notches); the
// box of those edges in `region`. A null shape where it fails.
TopoDS_Shape round_edges(const TopoDS_Shape& body, const std::vector<int>& list, double radius, Bnd_Box& region) {
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
    BRepBndLib::Add(edge, region);
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

void test_top_rounding_runs_past_the_fin_end() {
  const std::string cases = geometry_cases();
  if (cases.empty()) {
    std::printf("test_top_rounding_runs_past_the_fin_end: skipped (MITCAD_GEOMETRY_CASES is not set)\n");
    return;
  }
  const std::string  dir   = cases + "/fin-top-chamfer-rounding/";
  const TopoDS_Shape body  = read(dir + "body.brep");
  const TopoDS_Shape after = read(dir + "after.brep");
  const auto         list  = read_indices(dir + "edges.txt");
  const Bnd_Box      end   = read_box(cases + "/fin-end-rounding/fin-end.txt");
  CHECK(!body.IsNull() && !after.IsNull() && list.size() == 12 && !end.IsVoid());
  if (body.IsNull() || after.IsNull() || list.size() != 12 || end.IsVoid()) {
    return;
  }
  Bnd_Box            region;
  const TopoDS_Shape result = round_edges(body, list, 0.2, region);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(closed(result));
  // The result on the stored body about the fin's end, and back.
  const double out  = farthest(result, after, end);
  const double back = farthest(after, result, end);
  std::printf("about the fin's end: the result on the stored body within %.6f mm, the stored body on it within "
              "%.6f mm\n",
              out,
              back);
  CHECK(out <= 0.002);
  CHECK(back <= 0.002);
}

void test_end_rounding_up_to_the_top() {
  const std::string cases = geometry_cases();
  if (cases.empty()) {
    std::printf("test_end_rounding_up_to_the_top: skipped (MITCAD_GEOMETRY_CASES is not set)\n");
    return;
  }
  const std::string  dir   = cases + "/fin-end-rounding/";
  const TopoDS_Shape body  = read(dir + "body.brep");
  const TopoDS_Shape after = read(dir + "after.brep");
  const auto         list  = read_indices(dir + "edges.txt");
  CHECK(!body.IsNull() && !after.IsNull() && list.size() == 4);
  if (body.IsNull() || after.IsNull() || list.size() != 4) {
    return;
  }
  Bnd_Box            region;
  const TopoDS_Shape result = round_edges(body, list, 1., region);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(closed(result));
  // The stored body on the result round the end (the rounded edges' box,
  // grown by the radius).
  region.Enlarge(1.);
  const double back = farthest(after, result, region);
  std::printf("the stored body on the result within %.6f mm round the end\n", back);
  CHECK(back <= 0.01);
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_top_rounding_runs_past_the_fin_end", test::test_top_rounding_runs_past_the_fin_end);
  test::guarded("test_end_rounding_up_to_the_top", test::test_end_rounding_up_to_the_top);
  if (test::failures == 0) {
    std::puts("test_fillet_fin_end: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
