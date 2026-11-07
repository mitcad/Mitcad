// SPDX-License-Identifier: MIT
// A rounding whose contact leaves its face across a convex edge into the
// face at its end, short of the end (mitcad#1389), with OCCT's fillet alone,
// on the case `notch-lip-corners` of MITCAD_GEOMETRY_CASES: a reported
// design's bar, 14 wide, whose foot has a notch for a tube (a concave
// cylinder of radius 13.5 about the bar's axis) and, at each side, a narrow
// lip (a plane 0.34 wide) between the notch and the bar's side. Both sides'
// edges are rounded with 0.2 along their tangent chains; each chain ends at
// the vertex where the lip, the rounding of the bar's underside (a cylinder
// of radius 0.5) and the notch meet.
//
// - `body.brep`, the bar before the rounding as MitCad's import makes it,
//   with `edges.txt`, the indices (TopExp order) of the 22 edges rounded,
//   and `after.brep`, the stored body after the rounding. Near the end of
//   each chain the rounding's contact on the lip reaches the lip's edge with
//   the notch (convex, 29.5 degrees) before the end; the stored body rolls
//   the ball on that edge, touching the underside's rounding, until the
//   ball's contact on the edge turns back towards the chain, and the notch
//   cuts the rest of the rolling blend down to where its contact on the
//   underside's rounding reaches the notch: three faces at each corner (the
//   rounding up to the edge, the blend rolling on it up to the turn, the
//   cut piece), the underside untouched. Without the port's patch 1430 the
//   walk ran on over the lip's plane past the edge, its contact on the
//   underside's rounding ran onto the underside, and the notch cut that:
//   two faces at each corner, the underside cut, 0.015 mm from the stored
//   body.
//
// OCCT's own solids could model a notch beside a lip, but the case is the
// reported body, whose faces the import's checks compare.

#include <algorithm>
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <sstream>
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

// The rounding's radius.
constexpr double kRadius = 0.2;

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

// The edges in `list` rounded with 0.2 as MitCad's dressup calls the
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

// The two corners: about each, the faces whose boxes meet a box round the
// corner (the three corner faces, the underside's rounding, the notch and
// the underside), and a wider box for the distances.
struct Corner {
  const char* name;
  double      faces[6];
  double      near[6];
};
constexpr Corner kCorners[] = {
    {"the corner at x = 6.6", {6.4, -11.95, 8.94, 6.8, -11.75, 9.2}, {6.3, -12.1, 8.8, 7.1, -11.6, 9.6}},
    {"the corner at x = -6.6", {-6.8, -11.95, 8.94, -6.4, -11.75, 9.2}, {-7.1, -12.1, 8.8, -6.3, -11.6, 9.6}}};

void test_corners_roll_on_the_lip_edge() {
  const std::string cases = geometry_cases();
  if (cases.empty()) {
    std::printf("test_corners_roll_on_the_lip_edge: skipped (MITCAD_GEOMETRY_CASES is not set)\n");
    return;
  }
  const std::string      dir   = cases + "/notch-lip-corners/";
  const TopoDS_Shape     body  = read(dir + "body.brep");
  const TopoDS_Shape     after = read(dir + "after.brep");
  const std::vector<int> list  = read_indices(dir + "edges.txt");
  CHECK(!body.IsNull() && !after.IsNull() && list.size() == 22);
  if (body.IsNull() || after.IsNull() || list.size() != 22) {
    return;
  }
  const TopoDS_Shape result = round_edges(body, list);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(closed(result));
  CHECK(BRepCheck_Analyzer(result).IsValid());
  std::printf("the rounding: %d faces, the stored body %d\n", face_count(result), face_count(after));
  for (const Corner& c : kCorners) {
    Bnd_Box box, near;
    box.Update(c.faces[0], c.faces[1], c.faces[2], c.faces[3], c.faces[4], c.faces[5]);
    near.Update(c.near[0], c.near[1], c.near[2], c.near[3], c.near[4], c.near[5]);
    const int ours = face_count(faces_in(result, box)), stored = face_count(faces_in(after, box));
    const double out = farthest(result, after, near), back = farthest(after, result, near);
    std::printf("%s: %d faces about it, the stored body %d; the result on the stored body within %.6f mm, the "
                "stored body on it within %.6f mm\n",
                c.name, ours, stored, out, back);
    CHECK(ours == stored);
    CHECK(out <= 0.01);
    CHECK(back <= 0.01);
  }
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_corners_roll_on_the_lip_edge", test::test_corners_roll_on_the_lip_edge);
  if (test::failures == 0) {
    std::puts("test_fillet_end_edge_turn: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
