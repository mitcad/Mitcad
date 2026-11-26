// SPDX-License-Identifier: MIT
// Port patch 2365: an analytic blend whose contact passes over a hole of
// its face is cut by the hole's wall also where the wall's seam runs up
// from its rim under the blend (mitcad#2365, GitHub mitcad/Mitcad#635).
//
// The body, made as the reporter's: a block 36 x 40 x 20 (x 0..36,
// y -20..20, z -20..0) whose bottom is chamfered along x = 36 (a plane from
// (6, y, -20) to (36, y, -15)), with two counterbores from below at
// (16, -11.5) and (16, 11.5): 8.5 through, 15 wide and 8 deep from z = -20,
// both walls' seams towards +y (at y = -4 and y = 19). Each counterbore's
// rim on the chamfer comes 1 mm from an end face, y = -20 and y = 20. The
// body is its own mirror image in y = 0 but for the seams.
//
// The edges between the chamfer and the end faces rounded with 2: the
// contact on the chamfer, 2 from the edge, passes over the counterbore,
// and the blend runs on over it, cut by the counterbore's wall (patch
// 0055). At y = 20 the wall's seam runs up from the rim under the blend;
// the piece was refused there (a vertex of the face beyond under the
// blend), and the gap was walked from the end face over to the wall: 1.22
// mm3 more material than at y = -20, two more faces.
//
// Checks that fail without the patch: each edge alone, and both together,
// give valid solids whose two ends are each other's mirror images: each
// edge takes the same volume, 28.0273 mm3 (within 1e-3 mm3; 26.81 at
// y = 20 without the patch), the two the sum, as many faces at each end,
// and every point of a grid around the counterbore's rim at y = 20 is
// inside the solid where its mirror image at y = -20 is. (Rounded together
// with the bottom's edges at the end faces, the kernel's fillet alone gave
// both ends alike already; MitCad's fillet of the reported design did not:
// cli.fillet_counterbore_seam_reported.)

#include <cmath>
#include <cstdio>
#include <vector>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <gp_Ax2.hxx>
#include <gp_Vec.hxx>

#include "port_test.hpp"

namespace {

constexpr double kRadius = 2.0;
constexpr double kTaken  = 28.0273; // what each end's rounding takes (mm3)

TopoDS_Shape cut(const TopoDS_Shape& a, const TopoDS_Shape& b) {
  BRepAlgoAPI_Cut cutter(a, b);
  return cutter.IsDone() && !cutter.HasErrors() ? cutter.Shape() : TopoDS_Shape();
}

TopoDS_Shape body() {
  TopoDS_Shape shape = BRepPrimAPI_MakeBox(gp_Pnt(0, -20, -20), gp_Pnt(36, 20, 0)).Shape();
  // The chamfer: the wedge under the plane z = -20 + (x - 6) / 6, beyond
  // the block's ends along y.
  BRepBuilderAPI_MakePolygon triangle(gp_Pnt(0, -21, -21), gp_Pnt(37, -21, -20.0 + 31.0 / 6.0), gp_Pnt(37, -21, -21),
                                      true);
  const TopoDS_Face wedge_face = BRepBuilderAPI_MakeFace(triangle.Wire(), true).Face();
  shape = cut(shape, BRepPrimAPI_MakePrism(wedge_face, gp_Vec(0, 42, 0)).Shape());
  for (const double yc : {-11.5, 11.5}) {
    if (shape.IsNull()) {
      return shape;
    }
    // (The cylinders' seams at their x direction, +y.)
    const gp_Ax2 axis(gp_Pnt(16, yc, -25), gp_Dir(0, 0, 1), gp_Dir(0, 1, 0));
    shape = cut(shape, BRepPrimAPI_MakeCylinder(axis, 7.5, 13.0).Shape());
    if (!shape.IsNull()) {
      shape = cut(shape, BRepPrimAPI_MakeCylinder(axis, 4.25, 30.0).Shape());
    }
  }
  return shape;
}

// The edge whose middle lies at p.
TopoDS_Edge edge_at(const TopoDS_Shape& shape, const gp_Pnt& p) {
  const std::vector<TopoDS_Edge> edges = port::edges_where(shape, [&](const gp_Pnt& m) { return m.Distance(p) < 1e-6; });
  return edges.size() == 1 ? edges.front() : TopoDS_Edge();
}

TopoDS_Edge chamfer_edge(const TopoDS_Shape& shape, double y) { return edge_at(shape, gp_Pnt(21, y, -17.5)); }

TopoDS_Shape first_solid(const TopoDS_Shape& shape) {
  TopExp_Explorer solid(shape, TopAbs_SOLID);
  return solid.More() ? solid.Current() : TopoDS_Shape();
}

// The points of a grid around the rim of the counterbore at the end y > 0
// (0.5 apart, off round numbers) whose state in the solid `a` differs from
// that of their mirror images in y = 0 in the solid `b` (those on either
// boundary aside).
int differing_points(const TopoDS_Shape& in_a, const TopoDS_Shape& in_b) {
  const TopoDS_Shape solid_a = first_solid(in_a), solid_b = first_solid(in_b);
  if (solid_a.IsNull() || solid_b.IsNull()) {
    return -1;
  }
  BRepClass3d_SolidClassifier classifier_a(solid_a), classifier_b(solid_b);
  const auto state = [](BRepClass3d_SolidClassifier& classifier, const gp_Pnt& p) {
    classifier.Perform(p, 1e-3);
    return classifier.State();
  };
  int differ = 0;
  for (int i = 0; i <= 28; ++i) {
    for (int j = 0; j <= 6; ++j) {
      for (int k = 0; k <= 10; ++k) {
        const gp_Pnt p(9.0 + 0.5 * i + 0.0137, 17.0 + 0.5 * j + 0.0113, -20.0 + 0.5 * k + 0.0071);
        const gp_Pnt q(p.X(), -p.Y(), p.Z());
        const TopAbs_State a = state(classifier_a, p), b = state(classifier_b, q);
        if (a != TopAbs_ON && b != TopAbs_ON && a != b) {
          if (differ < 8) {
            std::printf("    (%.3f, %.3f, %.3f) %s, its mirror image %s\n", p.X(), p.Y(), p.Z(),
                        a == TopAbs_IN ? "in" : "out", b == TopAbs_IN ? "in" : "out");
          }
          ++differ;
        }
      }
    }
  }
  return differ;
}

void test_each_end_edge_alone_as_its_mirror_image() {
  const TopoDS_Shape b = body();
  CHECK(port::valid(b));
  const TopoDS_Edge plus = chamfer_edge(b, 20.0), minus = chamfer_edge(b, -20.0);
  CHECK(!plus.IsNull() && !minus.IsNull());
  if (plus.IsNull() || minus.IsNull()) {
    return;
  }
  const TopoDS_Shape at_plus  = port::rounded(b, {plus}, kRadius);
  const TopoDS_Shape at_minus = port::rounded(b, {minus}, kRadius);
  CHECK(port::valid(at_plus));
  CHECK(port::valid(at_minus));
  if (!port::valid(at_plus) || !port::valid(at_minus)) {
    return;
  }
  const double v_plus = port::volume_of(at_plus), v_minus = port::volume_of(at_minus);
  std::printf("  y = 20 alone: %.6f (%d faces), y = -20 alone: %.6f (%d faces)\n", v_plus,
              port::count_of(at_plus, TopAbs_FACE), v_minus, port::count_of(at_minus, TopAbs_FACE));
  // (Each takes the corner's wedge less what the counterbore already took
  // there: 28.0273 mm3.)
  const double before = port::volume_of(b);
  CHECK(std::abs((before - v_plus) - kTaken) <= 1e-3);
  CHECK(std::abs((before - v_minus) - kTaken) <= 1e-3);
  CHECK(port::count_of(at_plus, TopAbs_FACE) == port::count_of(at_minus, TopAbs_FACE));
  CHECK(differing_points(at_plus, at_minus) == 0);
}

void test_both_end_edges_as_mirror_images() {
  const TopoDS_Shape b = body();
  CHECK(port::valid(b));
  const TopoDS_Edge plus = chamfer_edge(b, 20.0), minus = chamfer_edge(b, -20.0);
  CHECK(!plus.IsNull() && !minus.IsNull());
  if (plus.IsNull() || minus.IsNull()) {
    return;
  }
  const TopoDS_Shape both = port::rounded(b, {plus, minus}, kRadius);
  CHECK(port::valid(both));
  if (!port::valid(both)) {
    return;
  }
  const double v_both = port::volume_of(both);
  std::printf("  both: %.6f (%d faces)\n", v_both, port::count_of(both, TopAbs_FACE));
  CHECK(std::abs((port::volume_of(b) - v_both) - 2.0 * kTaken) <= 2e-3);
  CHECK(differing_points(both, both) == 0);
}

} // namespace

MITCAD_PORT_TEST("2365", test_each_end_edge_alone_as_its_mirror_image);
MITCAD_PORT_TEST("2365", test_both_end_edges_as_mirror_images);
