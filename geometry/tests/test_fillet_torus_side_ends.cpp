// SPDX-License-Identifier: MIT
// A rounding along a wall's top and bottom edges whose chains end past a
// concave corner of the wall, with OCCT's fillet alone (mitcad#1425, the
// port's patches 1580 and 1581).
//
// The case `fillet-torus-side-ends` of MITCAD_GEOMETRY_CASES: `body.brep`, a
// reported design's stored body before the rounding, and `after.brep`, the
// stored body after it. A bar's front wall meets a boss through a concave
// rounding of 3 (a torus) at one end and, at the other, turns through a
// concave corner rounded with 3 (a cylinder about the vertical) into the
// side of a tip whose top and bottom edges are rounded with 4; that
// rounding goes on round the corner up to the wall as a corner patch
// tangent to the wall. The bar's top and bottom slope by 2.2 degrees to
// the tip's, meeting them along a crease. Each chain rounded with 5 runs
// along the torus's edge, the wall's edge and the corner patch's edge up to
// the crease, where it ends.
//
// At that end the ball's contact on the wall has gone on round the concave
// corner (tangent to the wall), not onto the corner patch, and its contact
// on the top has left the top across the crease before the vertex. The
// stored body runs the blend on past the spine's end over both faces'
// surfaces, the corner's cylinder and the top's plane, and cuts it with
// the faces it runs into: the tip's top beyond the crease, the corner
// patch (a sliver of it is left), the tip's rounding of 4 and the tip's
// side. Before, the walk took the tip's side for the contact past the
// corner and the corner's walk round the vertex failed.
//
// Checks: the result is valid and closed, has the stored body's faces, lies
// on the stored body and the stored body on it within 0.01 mm about each
// end and along each chain, and changes the volume as the stored body
// does within 1e-3 of it.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
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
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_Map.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Pnt.hxx>

#include "check.hpp"

namespace test {
namespace {

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

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

int face_count(const TopoDS_Shape& shape) {
  int n = 0;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    ++n;
  }
  return n;
}

bool closed(const TopoDS_Shape& shape) {
  NCollection_IndexedDataMap<TopoDS_Shape, int, TopTools_ShapeMapHasher> uses;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    for (TopExp_Explorer e(f.Current(), TopAbs_EDGE); e.More(); e.Next()) {
      const TopAbs_Orientation o = e.Current().Orientation();
      if (BRep_Tool::Degenerated(TopoDS::Edge(e.Current())) || o == TopAbs_INTERNAL || o == TopAbs_EXTERNAL) {
        continue;
      }
      const int i = uses.FindIndex(e.Current());
      if (i == 0) {
        uses.Add(e.Current(), 1);
      } else {
        ++uses(i);
      }
    }
  }
  for (int i = 1; i <= uses.Extent(); ++i) {
    if (uses(i) != 2) {
      return false;
    }
  }
  return true;
}

// The edge of the shape between two vertices at the points (within 1e-3).
TopoDS_Edge edge_between(const TopoDS_Shape& shape, const gp_Pnt& a, const gp_Pnt& b) {
  for (TopExp_Explorer e(shape, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
    TopoDS_Vertex      v1, v2;
    TopExp::Vertices(edge, v1, v2);
    if (v1.IsNull() || v2.IsNull()) {
      continue;
    }
    const gp_Pnt p1 = BRep_Tool::Pnt(v1), p2 = BRep_Tool::Pnt(v2);
    if ((p1.Distance(a) < 1e-3 && p2.Distance(b) < 1e-3) || (p1.Distance(b) < 1e-3 && p2.Distance(a) < 1e-3)) {
      return edge;
    }
  }
  return TopoDS_Edge();
}

TopoDS_Shape round_edges(const TopoDS_Shape& shape, const std::vector<TopoDS_Edge>& edges, double radius) {
  BRepFilletAPI_MakeFillet                               fillet(shape);
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> selected;
  for (const TopoDS_Edge& e : edges) {
    selected.Add(e);
  }
  fillet.StopChainsAtTurns(selected);
  for (const TopoDS_Edge& e : edges) {
    if (fillet.Contour(e) == 0) {
      fillet.Add(radius, e);
    }
  }
  fillet.CarryChainsAcrossNotches();
  try {
    fillet.Build();
  } catch (const Standard_Failure& failure) {
    std::printf("the rounding threw: %s\n", failure.what());
    return TopoDS_Shape();
  }
  if (!fillet.IsDone()) {
    std::printf("the rounding failed: %d faulty contours, %d faulty vertices\n",
                fillet.NbFaultyContours(),
                fillet.NbFaultyVertices());
    return TopoDS_Shape();
  }
  return fillet.Shape();
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
    for (int i = 0; i <= 10; ++i) {
      for (int j = 0; j <= 10; ++j) {
        const double u = u0 + (u1 - u0) * (0.02 + 0.96 * i / 10.);
        const double v = v0 + (v1 - v0) * (0.02 + 0.96 * j / 10.);
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

void test_reported_body() {
  const std::string cases = geometry_cases();
  if (cases.empty()) {
    std::printf("test_reported_body: skipped (MITCAD_GEOMETRY_CASES is not set)\n");
    return;
  }
  const std::string  dir   = cases + "/fillet-torus-side-ends/";
  const TopoDS_Shape shape = read(dir + "body.brep");
  const TopoDS_Shape after = read(dir + "after.brep");
  CHECK(!shape.IsNull() && !after.IsNull());
  if (shape.IsNull() || after.IsNull()) {
    return;
  }
  // Each chain: the torus's edge, the wall's edge and the corner patch's
  // edge with the bar's top (z about 21) and bottom (z about -6), as the
  // design selects them.
  std::vector<TopoDS_Edge> edges;
  for (const double side : {1., -1.}) {
    const auto at = [side](double x, double y, double z) { return gp_Pnt(x, y, side > 0. ? z : 15. - z); };
    edges.push_back(edge_between(shape, at(31.8229, -100.6507, 19.9906), at(22.5845, -97.9075, 20.3463)));
    edges.push_back(edge_between(shape, at(22.5845, -97.9075, 20.3463), at(6.6483, -98.3525, 20.9656)));
    edges.push_back(edge_between(shape, at(6.6483, -98.3525, 20.9656), at(5.7662, -98.4536, 21.0)));
  }
  for (const TopoDS_Edge& e : edges) {
    CHECK(!e.IsNull());
    if (e.IsNull()) {
      return;
    }
  }
  const TopoDS_Shape result = round_edges(shape, edges, 5.);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(closed(result));
  std::printf("faces %d, the stored body's %d\n", face_count(result), face_count(after));
  CHECK(face_count(result) == face_count(after));
  for (const double side : {1., -1.}) {
    const double z0 = side > 0. ? 14.5 : -7.5, z1 = side > 0. ? 22.5 : 0.5;
    // About the end past the corner.
    Bnd_Box end;
    end.Update(-1., -104., z0, 9., -92., z1);
    const double out  = farthest(result, after, end);
    const double back = farthest(after, result, end);
    std::printf("about the %s end: the result on the stored body within %.6f mm, the stored body on it within "
                "%.6f mm\n",
                side > 0. ? "top" : "bottom",
                out,
                back);
    CHECK(out <= 0.01);
    CHECK(back <= 0.01);
    // Along the whole chain.
    Bnd_Box chain;
    chain.Update(-1., -104., z0, 33., -92., z1);
    const double out2  = farthest(result, after, chain);
    const double back2 = farthest(after, result, chain);
    std::printf("along the %s chain: %.6f mm and %.6f mm\n", side > 0. ? "top" : "bottom", out2, back2);
    CHECK(out2 <= 0.01);
    CHECK(back2 <= 0.01);
  }
  const double change = volume_of(result) - volume_of(shape);
  const double file   = volume_of(after) - volume_of(shape);
  std::printf("the change of volume %.6f mm3, the stored body's %.6f\n", change, file);
  CHECK(std::abs(change - file) <= 1e-3 * std::abs(file));
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_reported_body", test::test_reported_body);
  if (test::failures == 0) {
    std::puts("test_fillet_torus_side_ends: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
