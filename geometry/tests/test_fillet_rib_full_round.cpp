// SPDX-License-Identifier: MIT
// A rib's full round with OCCT's fillet and chamfer alone (mitcad#1500, port
// patch 1655): a rib w wide, 40 long and 20 high, its four top edges rounded
// or bevelled by w / 2. The end edges' contacts on the top face are cut to
// one point at the ridge by the corners at both their ends (1652); the long
// edges' contacts lie along each other on the ridge, ending at those points
// inside the face. The two contacts are one edge and the top face goes:
// before 1655 it was left as a face of no area between two edges along the
// ridge (and before 1652 the result was invalid, and Mitcad built the
// dressup 0.1 % smaller).
//
// Each result is one valid closed solid with no face of no area and no face
// on top, its volume the exact shape's (within 1e-6 mm3): the top w / 2 at
// height t a rectangle 2 s by (40 - w + 2 s), s = sqrt(r^2 - t^2) for the
// round (pi r^2 / 2 (40 - w) + 8 r^3 / 3 kept of w 40 r) and s = r - t for
// the bevel (r^2 (40 - w) + 4 r^3 / 3 kept). Below w / 2 nothing changes.

#include <cmath>
#include <cstdio>

#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_IndexedMap.hxx>
#include <NCollection_List.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"

namespace test {
namespace {

constexpr double kLength = 40.0; // along y
constexpr double kHeight = 20.0; // along z
constexpr double kPi = 3.14159265358979323846;

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

double area_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(shape, props);
  return props.Mass();
}

// The rib: a rectangle w by 40 extruded 20 up.
TopoDS_Shape rib(double w) {
  BRepBuilderAPI_MakePolygon poly;
  poly.Add(gp_Pnt(0, 0, 0));
  poly.Add(gp_Pnt(w, 0, 0));
  poly.Add(gp_Pnt(w, kLength, 0));
  poly.Add(gp_Pnt(0, kLength, 0));
  poly.Close();
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(poly.Wire(), true);
  return BRepPrimAPI_MakePrism(face, gp_Vec(0, 0, kHeight)).Shape();
}

// Whether both of the edge's vertices are on top.
bool on_top(const TopoDS_Shape& edge) {
  TopoDS_Vertex v1, v2;
  TopExp::Vertices(TopoDS::Edge(edge), v1, v2);
  return BRep_Tool::Pnt(v1).Z() > kHeight - 1e-9 && BRep_Tool::Pnt(v2).Z() > kHeight - 1e-9;
}

// One solid, every edge on two faces, no face of no area, no face up on
// top, valid.
void check_full_round(const TopoDS_Shape& shape) {
  CHECK(BRepCheck_Analyzer(shape).IsValid());
  int solids = 0;
  for (TopExp_Explorer solid(shape, TopAbs_SOLID); solid.More(); solid.Next()) {
    ++solids;
  }
  CHECK(solids == 1);
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces;
  TopExp::MapShapesAndAncestors(shape, TopAbs_EDGE, TopAbs_FACE, faces);
  for (int i = 1; i <= faces.Extent(); ++i) {
    CHECK(BRep_Tool::Degenerated(TopoDS::Edge(faces.FindKey(i))) || faces(i).Extent() == 2);
  }
  for (TopExp_Explorer face(shape, TopAbs_FACE); face.More(); face.Next()) {
    const double area = area_of(face.Current());
    CHECK(area > 1e-6);
    if (area <= 1e-6) {
      std::fprintf(stderr, "a face of area %g\n", area);
    }
    bool top = true;
    for (TopExp_Explorer v(face.Current(), TopAbs_VERTEX); v.More() && top; v.Next()) {
      top = BRep_Tool::Pnt(TopoDS::Vertex(v.Current())).Z() > kHeight - 1e-6;
    }
    CHECK(!top);
  }
}

void check_volume(double got, double expected, const char* what) {
  if (std::abs(got - expected) > 1e-6) {
    std::fprintf(stderr, "check failed: %s: %.9f != %.9f\n", what, got, expected);
    ++failures;
  } else {
    std::printf("  %s: %.9f (expected %.9f)\n", what, got, expected);
  }
}

void test_full_round() {
  for (const double w : {2.0, 1.5, 4.0}) {
    const double r = w / 2.0;
    std::printf(" width %.1f, radius %.2f\n", w, r);
    const TopoDS_Shape body = rib(w);
    try {
      OCC_CATCH_SIGNALS
      BRepFilletAPI_MakeFillet fillet(body);
      for (TopExp_Explorer e(body, TopAbs_EDGE); e.More(); e.Next()) {
        if (on_top(e.Current())) {
          fillet.Add(r, TopoDS::Edge(e.Current()));
        }
      }
      fillet.Build();
      CHECK(fillet.IsDone());
      if (!fillet.IsDone()) {
        continue;
      }
      check_full_round(fillet.Shape());
      const double kept = kPi * r * r / 2.0 * (kLength - w) + 8.0 * r * r * r / 3.0;
      check_volume(volume_of(fillet.Shape()), w * kLength * (kHeight - r) + kept, "rounded");
    } catch (const Standard_Failure& failure) {
      std::fprintf(stderr, "the rounding threw: %s\n", failure.what());
      CHECK(false);
    }
  }
}

void test_full_bevel() {
  for (const double w : {2.0, 4.0}) {
    const double r = w / 2.0;
    std::printf(" width %.1f, distance %.2f\n", w, r);
    const TopoDS_Shape body = rib(w);
    try {
      OCC_CATCH_SIGNALS
      BRepFilletAPI_MakeChamfer chamfer(body);
      for (TopExp_Explorer e(body, TopAbs_EDGE); e.More(); e.Next()) {
        if (on_top(e.Current())) {
          chamfer.Add(r, TopoDS::Edge(e.Current()));
        }
      }
      chamfer.Build();
      CHECK(chamfer.IsDone());
      if (!chamfer.IsDone()) {
        continue;
      }
      check_full_round(chamfer.Shape());
      const double kept = r * r * (kLength - w) + 4.0 * r * r * r / 3.0;
      check_volume(volume_of(chamfer.Shape()), w * kLength * (kHeight - r) + kept, "bevelled");
    } catch (const Standard_Failure& failure) {
      std::fprintf(stderr, "the bevel threw: %s\n", failure.what());
      CHECK(false);
    }
  }
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_full_round", test::test_full_round);
  test::guarded("test_full_bevel", test::test_full_bevel);
  if (test::failures == 0) {
    std::puts("test_fillet_rib_full_round: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
