// SPDX-License-Identifier: MIT
// A rounding whose contact runs off a narrow face onto an earlier blend of
// the same radius (mitcad#1215), with OCCT's fillet alone, on the cases
// `fillet-roll-over-blend` and `fillet-roll-over-blend-2` of
// MITCAD_GEOMETRY_CASES: `body.brep`, a reported design's body before the
// rounding (two versions of the design), and `after.brep`, the body the
// design stores after it. A ring's top, 5 wide between an inner cylinder of
// radius 88.5 and an outer rim, has its outer rim rounded with 2.5 already
// (its contact on the top at radius 90.978), with fins at eight mouths whose
// crests meet the top there, some with sliver edges of a few micrometres;
// the inner rim, a closed chain of 17 edges, is rounded with 2.5. The ball's
// contact on the top would lie at radius 91.0, past the top face, on the
// earlier blend, which curves away from the ball as fast as its own sphere:
// the walk failed at once. With the port's patch 1230 the ball rolls on the
// top's far edges all round, the top face vanishing, as in the stored body.
// OCCT's booleans cannot build those tangent blends, so the cases are the
// reported bodies.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <Bnd_Box.hxx>
#include <NCollection_Map.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>

#include "check.hpp"
#include "mitcad/geometry/query.hpp"
#include "mitcad/geometry/shape.hpp"

namespace test {
namespace {

double volume_of(const TopoDS_Shape& shape) {
  return mitcad::geometry::mass_properties(mitcad::geometry::Shape(shape)).volume;
}

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

int count(const TopoDS_Shape& shape, TopAbs_ShapeEnum type) {
  int n = 0;
  for (TopExp_Explorer it(shape, type); it.More(); it.Next()) {
    ++n;
  }
  return n;
}

double top_of(const TopoDS_Shape& shape) {
  Bnd_Box box;
  BRepBndLib::Add(shape, box);
  double x0 = 0., y0 = 0., z0 = 0., x1 = 0., y1 = 0., z1 = 0.;
  box.Get(x0, y0, z0, x1, y1, z1);
  return z1;
}

double bottom_of(const TopoDS_Shape& shape) {
  Bnd_Box box;
  BRepBndLib::Add(shape, box);
  double x0 = 0., y0 = 0., z0 = 0., x1 = 0., y1 = 0., z1 = 0.;
  box.Get(x0, y0, z0, x1, y1, z1);
  return z0;
}

// The faces of a shape reaching above a height (`whole`: wholly above it),
// as a compound.
TopoDS_Compound faces_above(const TopoDS_Shape& shape, double z, bool whole) {
  TopoDS_Compound out;
  BRep_Builder    builder;
  builder.MakeCompound(out);
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    if ((whole ? bottom_of(f.Current()) : top_of(f.Current())) >= z) {
      builder.Add(out, f.Current());
    }
  }
  return out;
}

// The farthest of points inside the faces of `from` wholly above z from the
// faces of `to` reaching above 3 lower.
double farthest(const TopoDS_Shape& from, const TopoDS_Shape& to, double z) {
  const TopoDS_Compound target = faces_above(to, z - 3., false);
  double                worst  = 0.;
  for (TopExp_Explorer f(faces_above(from, z, true), TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face         face = TopoDS::Face(f.Current());
    const BRepAdaptor_Surface s(face);
    for (int i = 0; i <= 4; ++i) {
      for (int j = 0; j <= 4; ++j) {
        const double u = s.FirstUParameter() + (s.LastUParameter() - s.FirstUParameter()) * (0.05 + 0.9 * i / 4.);
        const double v = s.FirstVParameter() + (s.LastVParameter() - s.FirstVParameter()) * (0.05 + 0.9 * j / 4.);
        if (BRepClass_FaceClassifier(face, gp_Pnt2d(u, v), 1e-7).State() != TopAbs_IN) {
          continue;
        }
        BRepExtrema_DistShapeShape d(BRepBuilderAPI_MakeVertex(s.Value(u, v)).Vertex(), target);
        if (d.IsDone()) {
          worst = std::max(worst, d.Value());
        }
      }
    }
  }
  return worst;
}

void round_inner_rim(const char* name, double expected_volume, int expected_faces, std::size_t rim_edges) {
  const std::string cases = geometry_cases();
  if (cases.empty()) {
    std::printf("%s: skipped (MITCAD_GEOMETRY_CASES is not set)\n", name);
    return;
  }
  const TopoDS_Shape body  = read(cases + "/" + name + "/body.brep");
  const TopoDS_Shape after = read(cases + "/" + name + "/after.brep");
  CHECK(!body.IsNull() && !after.IsNull());
  if (body.IsNull() || after.IsNull()) {
    return;
  }
  const double before = volume_of(body);
  // The inner rim: the circles of radius 88.5 about the axis, the highest.
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> seen;
  std::vector<TopoDS_Edge>                               circles;
  double                                                 top = -1e100;
  for (TopExp_Explorer e(body, TopAbs_EDGE); e.More(); e.Next()) {
    if (!seen.Add(e.Current())) {
      continue;
    }
    const BRepAdaptor_Curve curve(TopoDS::Edge(e.Current()));
    if (curve.GetType() == GeomAbs_Circle && std::abs(curve.Circle().Radius() - 88.5) < 1e-6 &&
        std::hypot(curve.Circle().Location().X(), curve.Circle().Location().Y()) < 1e-6) {
      circles.push_back(TopoDS::Edge(e.Current()));
      top = std::max(top, curve.Circle().Location().Z());
    }
  }
  std::vector<TopoDS_Edge> rim;
  for (const TopoDS_Edge& edge : circles) {
    if (std::abs(BRepAdaptor_Curve(edge).Circle().Location().Z() - top) < 1e-3) {
      rim.push_back(edge);
    }
  }
  CHECK(rim.size() == rim_edges);
  BRepFilletAPI_MakeFillet fillet(body);
  for (const TopoDS_Edge& edge : rim) {
    fillet.Add(2.5, edge);
  }
  fillet.Build();
  CHECK(fillet.IsDone());
  if (!fillet.IsDone()) {
    std::printf("%s: the fillet failed\n", name);
    return;
  }
  const TopoDS_Shape result = fillet.Shape();
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(count(result, TopAbs_SOLID) == 1);
  CHECK(count(result, TopAbs_FACE) == expected_faces);
  const double volume = volume_of(result);
  std::printf("%s: the body %.6f, rounded %.6f (%d faces), the file's %.6f\n", name, before, volume,
              count(result, TopAbs_FACE), volume_of(after));
  CHECK_NEAR_TOL(volume, expected_volume, 1e-6);
  CHECK(std::abs(volume_of(body) - before) <= 1e-9 * before);
  // Against the file's body round the rim: its faces and the result's on
  // each other within 2 micrometres (OCCT's walked blends against the
  // file's).
  const double there = farthest(result, after, top - 3.);
  const double back  = farthest(after, result, top - 3.);
  std::printf("%s: against the file's body %.6f mm, back %.6f mm\n", name, there, back);
  CHECK(there <= 0.002);
  CHECK(back <= 0.002);
  // The rim's edges generate the blend faces.
  int generated = 0;
  for (const TopoDS_Edge& edge : rim) {
    generated += fillet.Generated(edge).Extent();
  }
  CHECK(generated > 0);
}

void test_rounding_rolls_on_the_far_edges() {
  round_inner_rim("fillet-roll-over-blend", 324916.183179, 343, 17);
}

void test_rounding_rolls_on_the_far_edges_other_version() {
  round_inner_rim("fillet-roll-over-blend-2", 362484.952973, 358, 17);
}

// The same bodies with their faces on one surface joined, as the import's
// replay builds them since mitcad#1261 (`fillet-roll-over-seam` and
// `fillet-roll-over-seam-2`): the top one face, the inner cylinder one face
// closed round its axis with its seam under a crest at one mouth, the rim
// one closed edge. The ball's turn over the sliver at that mouth carries
// its contact on the cylinder across the seam (port patch 1370,
// mitcad#1370).
void test_rounding_turns_past_the_seam() {
  round_inner_rim("fillet-roll-over-seam", 324917.409071, 244, 1);
}

void test_rounding_turns_past_the_seam_other_version() {
  round_inner_rim("fillet-roll-over-seam-2", 362485.012459, 261, 1);
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_rounding_rolls_on_the_far_edges", test::test_rounding_rolls_on_the_far_edges);
  test::guarded("test_rounding_rolls_on_the_far_edges_other_version", test::test_rounding_rolls_on_the_far_edges_other_version);
  test::guarded("test_rounding_turns_past_the_seam", test::test_rounding_turns_past_the_seam);
  test::guarded("test_rounding_turns_past_the_seam_other_version",
                test::test_rounding_turns_past_the_seam_other_version);
  if (test::failures == 0) {
    std::puts("test_fillet_roll_over_blend: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
