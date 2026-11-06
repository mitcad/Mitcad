// SPDX-License-Identifier: MIT
// A plate's side edge rounded with 0.3 down to a sloped face, past a groove
// of radius 13.5 cut beside the slot's wall (mitcad#1395), with OCCT's
// fillet alone, on the case `fillet-end-on-reversed-faces` of
// MITCAD_GEOMETRY_CASES: two pieces of a reported design's body about the
// rounding's end, cut from the same box, the same faces within 1e-6 mm but
// parametrised otherwise.
//
// - `stored.brep`, the stored body's piece, with `stored-edges.txt`, the
//   index (TopExp order) of the rounded edge: the plate's side x = 7 and
//   the slot's wall y = -11.6155, from the sloped face at z = 20 up. The
//   rounding's end runs over the sloped face, the groove's cylinder (the
//   rounding's contact on the wall lies in the groove there) and the
//   groove's top z = 22.
// - `replay.brep`, MitCad's piece, with `replay-edges.txt`: its cylinders
//   lie on left-handed axes, its planes face the other way (reversed faces),
//   and the sloped face's edge with the groove (an ellipse) runs past the
//   end of its curve's first period. Without the port's patch 1460 the end
//   evaluated that edge's p-curve on the cylinder far outside its range and
//   rebuilt the cylinder with a wire running 195 mm down its axis: the result
//   was invalid and 0.5 mm3 short.
//
// Both rounded give the same closed, valid result.

#include <algorithm>
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <sstream>
#include <string>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_IndexedMap.hxx>
#include <NCollection_List.hxx>
#include <NCollection_Map.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>

#include "check.hpp"

namespace test {
namespace {

using ShapeIndex = NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher>;
using EdgeFaces  = NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;

// The rounding's radius.
constexpr double kRadius = 0.3;

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

double volume(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

// The farthest of points inside the faces of `from` from the faces of `to`.
double farthest(const TopoDS_Shape& from, const TopoDS_Shape& to) {
  double worst = 0.;
  for (TopExp_Explorer f(from, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face face = TopoDS::Face(f.Current());
    double            u0 = 0., u1 = 0., v0 = 0., v1 = 0.;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    const BRepAdaptor_Surface s(face);
    for (int i = 0; i <= 6; ++i) {
      for (int j = 0; j <= 6; ++j) {
        const double u = u0 + (u1 - u0) * (0.02 + 0.96 * i / 6.);
        const double v = v0 + (v1 - v0) * (0.02 + 0.96 * j / 6.);
        if (BRepClass_FaceClassifier(face, gp_Pnt2d(u, v), 1e-7).State() != TopAbs_IN) {
          continue;
        }
        BRepExtrema_DistShapeShape d(BRepBuilderAPI_MakeVertex(s.Value(u, v)).Vertex(), to);
        if (d.IsDone()) {
          worst = std::max(worst, d.Value());
        }
      }
    }
  }
  return worst;
}

// The listed edges rounded with 0.3 as MitCad's dressup calls the fillet:
// chains stopped at turns, carried across notches. Null where the fillet
// fails.
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

// A piece of the case rounded; null where the case is missing or the
// rounding fails.
TopoDS_Shape rounded(const char* name, const char* body_file, const char* list_file) {
  const std::string cases = geometry_cases();
  if (cases.empty()) {
    std::printf("%s: skipped (MITCAD_GEOMETRY_CASES is not set)\n", name);
    return TopoDS_Shape();
  }
  const std::string      dir  = cases + "/fillet-end-on-reversed-faces/";
  const TopoDS_Shape     body = read(dir + body_file);
  const std::vector<int> list = read_indices(dir + list_file);
  CHECK(!body.IsNull() && list.size() == 1);
  if (body.IsNull() || list.size() != 1) {
    return TopoDS_Shape();
  }
  const TopoDS_Shape result = round_edges(body, list);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return result;
  }
  const bool valid = BRepCheck_Analyzer(result).IsValid();
  std::printf("%s: %d faces, volume %.6f, valid %d\n", name, face_count(result), volume(result), valid ? 1 : 0);
  CHECK(closed(result));
  CHECK(valid);
  return result;
}

void test_pieces_alike() {
  const TopoDS_Shape stored = rounded("the stored piece", "stored.brep", "stored-edges.txt");
  const TopoDS_Shape replay = rounded("MitCad's piece", "replay.brep", "replay-edges.txt");
  if (stored.IsNull() || replay.IsNull()) {
    return;
  }
  CHECK(face_count(replay) == face_count(stored));
  // The pieces lie on each other within 1e-6 mm; so do the results.
  CHECK(std::abs(volume(replay) - volume(stored)) <= 1e-4);
  const double out  = farthest(replay, stored);
  const double back = farthest(stored, replay);
  std::printf("MitCad's result on the stored one within %.7f mm, the stored one on it within %.7f mm\n", out, back);
  CHECK(out <= 1e-5);
  CHECK(back <= 1e-5);
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_pieces_alike", test::test_pieces_alike);
  if (test::failures == 0) {
    std::puts("test_fillet_end_reversed_faces: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
