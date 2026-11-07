// SPDX-License-Identifier: MIT
// A hub's foot rounded between holes that touch the hub, past the earlier
// roundings of the holes' rims, tighter than the ball (mitcad#1413), with
// OCCT's fillet alone, on the case `hub-foot-tight-rim-ends` of
// MITCAD_GEOMETRY_CASES: a reported design's hub (a cylinder of radius
// 22.75) standing on a flange (the plane z = 22), with holes of radius 1.5
// whose walls touch the hub. Each hole's rim is rounded with 0.5: a torus
// round the hole and, between the torus and the hub, on each side of the
// vertex where the hole touches the hub, a face along the hub's foot (its
// edge with the hub an arc of the foot's circle) tangent to the torus and
// bending towards a ball of 1 more sharply than the ball.
//
// - `body.bin`, the stored body before the rounding (OCCT's binary format,
//   as the import keeps it: written as text, the hub's foot reads back as
//   one tangent chain through the vertices where the holes touch the hub),
//   with `edges.txt`, the indices (TopExp order) of the nine edges of the
//   hub's foot rounded with 1 (three tangent chains between the holes), and
//   `after.bin`, the stored body after the rounding. Near each hole the walk's contact on the
//   flange reaches the rim's torus and then the face along the foot, which
//   no ball of 1 can roll on; the stored body rolls the ball on over the
//   torus's own surface, between it and the hub, cuts the blend by that face
//   from the crossing, and ends it in a point on the foot's edge where the
//   cut meets the contact on the hub, 0.29 short of the vertex where the
//   hole touches the hub; the edge stays sharp from there to the vertex.
//   Four such ends, two at each of the two smaller holes. Without the
//   port's patch 1530 the walk reframed onto that face and failed; without
//   1540 (mitcad#1418) each end was two faces, the piece walked up to the
//   crossing and the piece cut after it, where the stored body has one.
//
// OCCT's own fillet cannot make the earlier roundings of a hole's rim that
// touches the hub (on a hub and flange built with OCCT's solids the rim's
// rounding throws in the topological build), so the case is the reported
// body.

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
#include <BRepGProp.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <BinTools.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
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
constexpr double kRadius = 1.0;

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
  BinTools::Read(shape, path.c_str());
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

// The volume, integrated adaptively (the default integration of the
// rounding's B-spline faces is off by hundredths of a cubic millimetre).
double volume(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-7);
  return props.Mass();
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

// The distance from a point to the nearest vertex of a shape.
double nearest_vertex(const TopoDS_Shape& shape, const gp_Pnt& p) {
  double best = 1e100;
  for (TopExp_Explorer v(shape, TopAbs_VERTEX); v.More(); v.Next()) {
    best = std::min(best, BRep_Tool::Pnt(TopoDS::Vertex(v.Current())).Distance(p));
  }
  return best;
}

double area(const TopoDS_Shape& face) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(face, props, 1e-7);
  return props.Mass();
}

int face_count(const TopoDS_Shape& shape) {
  ShapeIndex faces;
  TopExp::MapShapes(shape, TopAbs_FACE, faces);
  return faces.Extent();
}

// The rounding's face at one of its ends: of the faces through the point
// that are no cylinders (the hub), the largest (the other is the remainder
// of the rim's face along the foot, about 0.25 mm2). Its area, or 0.
double end_face_area(const TopoDS_Shape& shape, const gp_Pnt& p) {
  const TopoDS_Shape point = BRepBuilderAPI_MakeVertex(p).Vertex();
  double             best  = 0.;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    if (BRepAdaptor_Surface(TopoDS::Face(f.Current())).GetType() == GeomAbs_Cylinder) {
      continue;
    }
    BRepExtrema_DistShapeShape d(point, f.Current());
    if (d.IsDone() && d.Value() <= 1e-3) {
      best = std::max(best, area(f.Current()));
    }
  }
  return best;
}

// The edges in `list` rounded with 1 as MitCad's dressup calls the fillet:
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

// The two smaller holes: about each, a box round its two ends (the rim's
// torus and faces, the rounding's last stretches and the hub's foot there).
struct Hole {
  const char* name;
  double      box[6];
};
constexpr Hole kHoles[] = {
    {"the hole at x = 3", {0.9, 22.2, 21.4, 5.0, 24.1, 23.1}},
    {"the hole at x = -3", {-5.0, 22.2, 21.4, -0.9, 24.1, 23.1}}};

// The points where the stored rounding ends, on the hub's foot short of
// the vertices where the holes touch the hub.
constexpr double kEnds[][3] = {{3.10129, 22.53762, 22.0},
                               {2.52712, 22.60921, 22.0},
                               {-2.52712, 22.60921, 22.0},
                               {-3.10129, 22.53762, 22.0}};

void test_ends_cut_by_the_rims() {
  const std::string cases = geometry_cases();
  if (cases.empty()) {
    std::printf("test_ends_cut_by_the_rims: skipped (MITCAD_GEOMETRY_CASES is not set)\n");
    return;
  }
  const std::string      dir   = cases + "/hub-foot-tight-rim-ends/";
  const TopoDS_Shape     body  = read(dir + "body.bin");
  const TopoDS_Shape     after = read(dir + "after.bin");
  const std::vector<int> list  = read_indices(dir + "edges.txt");
  CHECK(!body.IsNull() && !after.IsNull() && list.size() == 9);
  if (body.IsNull() || after.IsNull() || list.size() != 9) {
    return;
  }
  const TopoDS_Shape result = round_edges(body, list);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(closed(result));
  CHECK(BRepCheck_Analyzer(result).IsValid());
  const double before = volume(body), ours = volume(result) - before, stored = volume(after) - before;
  std::printf("the rounding adds %.6f mm3, the stored body %.6f mm3\n", ours, stored);
  CHECK(std::abs(ours - stored) <= 0.005 * stored);
  for (const Hole& h : kHoles) {
    Bnd_Box box;
    box.Update(h.box[0], h.box[1], h.box[2], h.box[3], h.box[4], h.box[5]);
    const double out = farthest(result, after, box), back = farthest(after, result, box);
    std::printf("%s: the result on the stored body within %.6f mm, the stored body on it within %.6f mm\n", h.name,
                out, back);
    CHECK(out <= 0.01);
    CHECK(back <= 0.01);
  }
  for (const auto& e : kEnds) {
    const gp_Pnt p(e[0], e[1], e[2]);
    const double d = nearest_vertex(result, p);
    std::printf("the end at (%.5f %.5f %.5f): a vertex of the result within %.6f mm\n", e[0], e[1], e[2], d);
    CHECK(d <= 0.01);
    // One face from the section where the contact on the flange reaches the
    // rim's torus to the end, as the stored body's (mitcad#1418): before, the
    // piece walked up to the crossing onto the rim's face along the foot
    // and the piece cut by that face were two faces, the one at the end
    // about 0.05 mm2.
    const double a = end_face_area(result, p), s = end_face_area(after, p);
    std::printf("  its face %.6f mm2, the stored body's %.6f mm2\n", a, s);
    CHECK(s > 0. && std::abs(a - s) <= 0.01 * s);
  }
  const int faces = face_count(result), stored_faces = face_count(after);
  std::printf("the result has %d faces, the stored body %d\n", faces, stored_faces);
  CHECK(faces == stored_faces);
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_ends_cut_by_the_rims", test::test_ends_cut_by_the_rims);
  if (test::failures == 0) {
    std::puts("test_fillet_tight_tangent_end: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
