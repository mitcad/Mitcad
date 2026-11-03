// SPDX-License-Identifier: MIT
// A rounding over the mouths of slots whose rounded ends touch the top face
// along a crest line (mitcad#1204), with OCCT's fillet alone, on the case
// `fillet-slot-mouths` of MITCAD_GEOMETRY_CASES: `body.brep`, a reported
// design's body before the rounding, and `after.brep`, the body the design
// stores after it. A rim of eight arcs (a circle of radius 93.5 between the
// top plane and eight pieces of the outer cylinder, 2.17 high above a
// rounding of 5) is rounded with 2.5; at each vertex between the arcs a fin
// 1.5 thick stands out of the wall, its top a cylinder of radius 0.75 whose
// crest touches the top plane along a line with no edge there. OCCT's
// corners failed at all eight vertices ("Failed to find edge"); with the
// port's patch 1210 the fillet builds them as a construction of its own:
// the body rounded with slabs through the mouths cut out, the slabs put
// back with the blends prolonged across them and the fins' tops grown up
// to the blends. OCCT's booleans cannot build the tangent crests, so the
// case is the reported body.

#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <gp_Ax2.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <NCollection_Map.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>

#include "check.hpp"
#include "mitcad/geometry/query.hpp"
#include "mitcad/geometry/shape.hpp"

namespace test {
namespace {

// The rounded body's volume (MitCad's integrals). The file's body comes out
// 16 mm3 less by these integrals, but by the material within the same
// wedges of the rim only about 0.04 mm3 round a mouth and 0.19 mm3 over the
// arc beside it less: OCCT's walked blends lie up to 2 micrometres outside
// the file's along the arcs.
constexpr double kExpectedVolume = 325667.054553;

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

void test_rounding_over_slot_mouths() {
  const std::string cases = geometry_cases();
  if (cases.empty()) {
    std::printf("test_rounding_over_slot_mouths: skipped (MITCAD_GEOMETRY_CASES is not set)\n");
    return;
  }
  const TopoDS_Shape body  = read(cases + "/fillet-slot-mouths/body.brep");
  const TopoDS_Shape after = read(cases + "/fillet-slot-mouths/after.brep");
  CHECK(!body.IsNull() && !after.IsNull());
  if (body.IsNull() || after.IsNull()) {
    return;
  }
  const double before = volume_of(body);
  // The rim's eight arcs.
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> seen;
  std::vector<TopoDS_Edge>                               rim;
  for (TopExp_Explorer e(body, TopAbs_EDGE); e.More(); e.Next()) {
    if (!seen.Add(e.Current())) {
      continue;
    }
    const BRepAdaptor_Curve curve(TopoDS::Edge(e.Current()));
    if (curve.GetType() == GeomAbs_Circle && std::abs(curve.Circle().Radius() - 93.5) < 1e-6 &&
        std::abs(curve.Circle().Location().Z() - 307.46349) < 1e-3) {
      rim.push_back(TopoDS::Edge(e.Current()));
    }
  }
  CHECK(rim.size() == 8);
  BRepFilletAPI_MakeFillet fillet(body);
  for (const TopoDS_Edge& edge : rim) {
    fillet.Add(2.5, edge);
  }
  fillet.Build();
  CHECK(fillet.IsDone());
  if (!fillet.IsDone()) {
    std::printf("test_rounding_over_slot_mouths: the fillet failed at %d vertices\n", fillet.NbFaultyVertices());
    return;
  }
  const TopoDS_Shape result = fillet.Shape();
  CHECK(BRepCheck_Analyzer(result).IsValid());
  CHECK(count(result, TopAbs_SOLID) == 1);
  const double volume = volume_of(result);
  std::printf("the body %.6f, rounded %.6f, the file's %.6f\n", before, volume, volume_of(after));
  CHECK_NEAR_TOL(volume, kExpectedVolume, 1e-6);
  CHECK(std::abs(volume_of(body) - before) <= 1e-9 * before);
  // Against the file's body, the material of each within the same wedges
  // of the rim: one round the mouth on the x axis (6 degrees) and one over
  // the arc beside it, short of the next mouth (39 degrees).
  const auto wedge = [&](const TopoDS_Shape& shape, double from, double to) {
    const gp_Ax2 axes(gp_Pnt(0, 0, 295), gp_Dir(0, 0, 1), gp_Dir(std::cos(from * kPi / 180), std::sin(from * kPi / 180), 0));
    const TopoDS_Shape cake = BRepPrimAPI_MakeCylinder(axes, 120, 15, (to - from) * kPi / 180).Shape();
    return volume_of(BRepAlgoAPI_Common(shape, cake).Shape());
  };
  const double mouth = wedge(result, -3, 3) - wedge(after, -3, 3);
  const double arc   = wedge(result, 3, 42) - wedge(after, 3, 42);
  std::printf("against the file's body: round the mouth %.6f, over the arc %.6f\n", mouth, arc);
  CHECK(std::abs(mouth) <= 0.05);
  CHECK(std::abs(arc) <= 0.3);
  // The history: each arc generates the blend faces on it, every face of
  // the result is the body's, an image of one or generated by an arc.
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> accounted;
  for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
    accounted.Add(f.Current());
    for (const TopoDS_Shape& image : fillet.Modified(f.Current())) {
      accounted.Add(image);
    }
  }
  for (const TopoDS_Edge& edge : rim) {
    const NCollection_List<TopoDS_Shape>& generated = fillet.Generated(edge);
    CHECK(!generated.IsEmpty());
    for (const TopoDS_Shape& face : generated) {
      accounted.Add(face);
    }
  }
  int unaccounted = 0;
  for (TopExp_Explorer f(result, TopAbs_FACE); f.More(); f.Next()) {
    unaccounted += accounted.Contains(f.Current()) ? 0 : 1;
  }
  CHECK(unaccounted == 0);
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_rounding_over_slot_mouths", test::test_rounding_over_slot_mouths);
  if (test::failures == 0) {
    std::puts("test_fillet_slot_mouths: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
