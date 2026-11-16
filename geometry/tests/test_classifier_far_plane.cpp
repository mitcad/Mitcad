// SPDX-License-Identifier: MIT
// OCCT's solid classifier on a point in the plane of a far face of the
// shell, with the port's patch 1285 (mitcad#1280). The classifier takes
// segments from the point to points of the faces, one face after another;
// a face whose surface holds the point gives none, and a face seen almost
// edge-on only a weak one, which such a face later in the shell discards.
// Without the patch the faces ran out and the classifier rejected the point,
// which it gives as outside: the checker (BRepCheck_Solid) took a point of
// one cavity for lying outside the solid of another whose bottom face lies
// in its plane, and the face-merged union of a machine's 257 bodies was
// thrown away for the unmerged one. With the patch such a point is
// classified along fixed lines towards points of the shape's box.
//
// The block here has two cavities: a thin pointed one along x, its bottom
// face (last in its shell) in the plane z = 0, and a small box beyond its
// point whose bottom (first in its shell, so that the checker's point of it
// lies on the bottom's edges) lies in that plane too. Every other face of
// the pointed cavity is seen from there less than 5 degrees off its plane.
//
// A flat shell (every face in one plane, volume 0) has every fixed line of a
// point in its plane run in that plane; such a line through an edge is passed
// over (mitcad#1881), so that a cut by the flat shell leaves its object as it
// is instead of emptying it.

#include <cmath>
#include <cstdio>
#include <vector>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepCheck_Solid.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepGProp.hxx>
#include <BRepLib.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <TopoDS_Vertex.hxx>
#include <GProp_GProps.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>
#include <gp_XYZ.hxx>

#include "check.hpp"

namespace {

using Polygon = std::vector<gp_Pnt>;

// The mean of the polygon's corners.
gp_XYZ centre(const Polygon& corners) {
  gp_XYZ sum(0, 0, 0);
  for (const gp_Pnt& corner : corners) {
    sum += corner.XYZ();
  }
  return sum / static_cast<double>(corners.size());
}

// The mean of the face's vertices (each one met twice, by its two edges).
gp_XYZ centre(const TopoDS_Face& face) {
  gp_XYZ sum(0, 0, 0);
  int n = 0;
  for (TopExp_Explorer it(face, TopAbs_VERTEX); it.More(); it.Next()) {
    sum += BRep_Tool::Pnt(TopoDS::Vertex(it.Current())).XYZ();
    ++n;
  }
  return n > 0 ? sum / static_cast<double>(n) : sum;
}

TopoDS_Face planar_face(const Polygon& corners) {
  BRepBuilderAPI_MakePolygon wire;
  for (const gp_Pnt& corner : corners) {
    wire.Add(corner);
  }
  wire.Close();
  return BRepBuilderAPI_MakeFace(wire.Wire(), true).Face();
}

// The closed shell of the polygons, sewn and oriented outwards, its faces
// in the order of the polygons (the classifier and the checker go through
// them in that order).
TopoDS_Shell shell_in_order(const std::vector<Polygon>& polygons) {
  BRepBuilderAPI_Sewing sewing;
  for (const Polygon& corners : polygons) {
    sewing.Add(planar_face(corners));
  }
  sewing.Perform();
  TopoDS_Solid solid = BRepBuilderAPI_MakeSolid(TopoDS::Shell(sewing.SewedShape())).Solid();
  BRepLib::OrientClosedSolid(solid);
  BRep_Builder builder;
  TopoDS_Shell shell;
  builder.MakeShell(shell);
  for (const Polygon& corners : polygons) {
    const gp_XYZ wanted = centre(corners);
    for (TopExp_Explorer it(solid, TopAbs_FACE); it.More(); it.Next()) {
      if ((centre(TopoDS::Face(it.Current())) - wanted).Modulus() < 1e-9) {
        builder.Add(shell, it.Current());
        break;
      }
    }
  }
  shell.Closed(true);
  return shell;
}

// The pointed cavity: from its point A at the origin along x to B at
// x = 100, 2 mm wide and 2 mm high at x = 50; its bottom, the quadrilateral
// A M1 B M2 in z = 0, last.
TopoDS_Shell pointed_cavity() {
  const gp_Pnt a(0, 0, 0);
  const gp_Pnt b(100, 0, 0);
  const gp_Pnt m1(50, -1, 0);
  const gp_Pnt m2(50, 1, 0);
  const gp_Pnt m3(50, 1, 2);
  const gp_Pnt m4(50, -1, 2);
  return shell_in_order({
      {a, m2, m3},
      {a, m3, m4},
      {a, m4, m1},
      {b, m3, m2},
      {b, m4, m3},
      {b, m1, m4},
      {a, m1, b, m2},
  });
}

// The box-shaped cavity beyond the point: x from -42 to -38, y from -2 to
// 2, z from 0 to 4; its bottom first.
TopoDS_Shell box_cavity() {
  const double x0 = -42, x1 = -38, y0 = -2, y1 = 2, z0 = 0, z1 = 4;
  const gp_Pnt p000(x0, y0, z0), p100(x1, y0, z0), p110(x1, y1, z0), p010(x0, y1, z0);
  const gp_Pnt p001(x0, y0, z1), p101(x1, y0, z1), p111(x1, y1, z1), p011(x0, y1, z1);
  return shell_in_order({
      {p000, p010, p110, p100},
      {p001, p101, p111, p011},
      {p000, p100, p101, p001},
      {p100, p110, p111, p101},
      {p110, p010, p011, p111},
      {p010, p000, p001, p011},
  });
}

int face_count(const TopoDS_Shape& shape) {
  int n = 0;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    ++n;
  }
  return n;
}

// The face of the shell at the index (from 0) in its order.
TopoDS_Face face_at(const TopoDS_Shape& shape, int index) {
  int n = 0;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next(), ++n) {
    if (n == index) {
      return TopoDS::Face(it.Current());
    }
  }
  return TopoDS_Face();
}

// Whether all the face's vertices lie in z = 0.
bool in_the_plane(const TopoDS_Face& face) {
  bool all = !face.IsNull();
  for (TopExp_Explorer it(face, TopAbs_VERTEX); it.More(); it.Next()) {
    all = all && std::abs(BRep_Tool::Pnt(TopoDS::Vertex(it.Current())).Z()) < 1e-12;
  }
  return all;
}

void test_point_in_the_plane_of_a_far_face() {
  const TopoDS_Shell pointed = pointed_cavity();
  CHECK(face_count(pointed) == 7);
  CHECK(BRep_Tool::IsClosed(pointed));
  // The premise: the bottom is the last face.
  CHECK(in_the_plane(face_at(pointed, 6)));
  // The pointed cavity's solid as the checker makes it: its shell as a
  // cavity, everything outside it.
  BRep_Builder builder;
  TopoDS_Solid outside;
  builder.MakeSolid(outside);
  builder.Add(outside, pointed.Reversed());
  BRepClass3d_SolidClassifier classifier(outside);
  // Points in z = 0 beyond the point: in the solid (outside the cavity).
  // Without the patch the classifier rejected them.
  for (const gp_Pnt& point : {gp_Pnt(-40, 0, 0), gp_Pnt(-39, 1.5, 0), gp_Pnt(-41, -1, 0), gp_Pnt(-38.5, 0.5, 0)}) {
    classifier.Perform(point, 1e-7);
    CHECK(classifier.State() == TopAbs_IN);
    CHECK(!classifier.Rejected());
    if (classifier.State() != TopAbs_IN) {
      std::fprintf(stderr, "  (%g, %g, %g): state %d\n", point.X(), point.Y(), point.Z(),
                   static_cast<int>(classifier.State()));
    }
  }
  // Points inside the cavity: outside the solid, as before.
  for (const gp_Pnt& point : {gp_Pnt(50, 0, 0.5), gp_Pnt(30, 0.1, 0.3), gp_Pnt(70, -0.2, 0.4)}) {
    classifier.Perform(point, 1e-7);
    CHECK(classifier.State() == TopAbs_OUT);
  }
}

void test_block_with_the_two_cavities() {
  const TopoDS_Shell pointed = pointed_cavity();
  const TopoDS_Shell box = box_cavity();
  CHECK(face_count(box) == 6);
  // The premise: the box's bottom is its first face, so that the checker's
  // point of it (on its first edge) lies in z = 0.
  CHECK(in_the_plane(face_at(box, 0)));
  BRep_Builder builder;
  TopoDS_Solid block;
  builder.MakeSolid(block);
  builder.Add(block, BRepPrimAPI_MakeBox(gp_Pnt(-70, -20, -10), gp_Pnt(120, 20, 20)).Shell());
  builder.Add(block, pointed.Reversed());
  builder.Add(block, box.Reversed());
  // The checker accepts it: without the patch it found the box's point
  // outside the pointed cavity's solid (BRepCheck_SubshapeNotInShape).
  const occ::handle<BRepCheck_Solid> check = new BRepCheck_Solid(block);
  for (const BRepCheck_Status status : check->Status()) {
    CHECK(status == BRepCheck_NoError);
    if (status != BRepCheck_NoError) {
      std::fprintf(stderr, "  solid status %d\n", static_cast<int>(status));
    }
  }
  CHECK(BRepCheck_Analyzer(block).IsValid());
  // A point of the material in z = 0 between the cavities is in the block;
  // points in the cavities are not.
  BRepClass3d_SolidClassifier classifier(block);
  classifier.Perform(gp_Pnt(-30, 0.5, 0), 1e-7);
  CHECK(classifier.State() == TopAbs_IN);
  classifier.Perform(gp_Pnt(-40, 0, 1), 1e-7);
  CHECK(classifier.State() == TopAbs_OUT);
  classifier.Perform(gp_Pnt(50, 0, 0.5), 1e-7);
  CHECK(classifier.State() == TopAbs_OUT);
}

// A flat shell: a rectangle in the plane x = 0 swept along z, within its own
// plane, so that every face of the solid lies in x = 0 and its volume is 0
// (the tool of OCCT's TKBO GTests BCutSimpleTest1 K1, K5, K7 and L1).
TopoDS_Shape flat_prism() {
  const TopoDS_Face face =
      planar_face({gp_Pnt(0, 75, 40), gp_Pnt(0, 175, 40), gp_Pnt(0, 175, 115), gp_Pnt(0, 75, 115)});
  return BRepPrimAPI_MakePrism(face, gp_Vec(0, 0, 30)).Shape();
}

double volume(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

void test_point_in_the_plane_of_a_flat_shell() {
  // mitcad#1881: a point in the plane of a flat shell gets no segment from
  // any face (each surface holds it), so it is classified along the fixed
  // lines, and the shape's box being flat, every one of them runs in that
  // plane. A line through an edge there was decided by the transition across
  // the edge's faces, which counted points outside the shell inside it; a
  // fixed line through an edge is now passed over, and with no line meeting
  // a face cleanly the point is rejected as before the patch (OUT).
  const TopoDS_Shape flat = flat_prism();
  CHECK(std::abs(volume(flat)) < 1e-6);
  BRepClass3d_SolidClassifier classifier(flat);
  for (const gp_Pnt& point : {gp_Pnt(0, 200, 22.7144), gp_Pnt(0, 190, 20), gp_Pnt(0, 20, 10),
                              gp_Pnt(0, 120, 10)}) {
    classifier.Perform(point, 1e-7);
    CHECK(classifier.State() == TopAbs_OUT);
    if (classifier.State() != TopAbs_OUT) {
      std::fprintf(stderr, "  (%g, %g, %g): state %d\n", point.X(), point.Y(), point.Z(),
                   static_cast<int>(classifier.State()));
    }
  }
}

void test_cut_by_a_flat_shell() {
  // A block with a pocket in its top, whose face x = 0 the flat shell
  // touches along the block's top edge: the cut leaves the block as it is
  // (with the patch's earlier form it came out empty: the block was
  // classified inside the tool).
  // Both prisms as the GTests make them: a rectangle in a plane z = const,
  // swept down.
  const TopoDS_Shape plate = BRepPrimAPI_MakePrism(
      planar_face({gp_Pnt(0, 0, 40), gp_Pnt(150, 0, 40), gp_Pnt(150, 200, 40), gp_Pnt(0, 200, 40)}),
      gp_Vec(0, 0, -40)).Shape();
  const TopoDS_Shape pocket = BRepPrimAPI_MakePrism(
      planar_face(
          {gp_Pnt(25, 25, 50), gp_Pnt(25, 125, 50), gp_Pnt(100, 125, 50), gp_Pnt(100, 25, 50)}),
      gp_Vec(0, 0, -30)).Shape();
  const TopoDS_Shape block = BRepAlgoAPI_Cut(plate, pocket).Shape();
  const double before = 150.0 * 200.0 * 40.0 - 75.0 * 100.0 * 20.0;
  CHECK_NEAR(volume(block), before);
  BRepAlgoAPI_Cut cut(block, flat_prism());
  CHECK(cut.IsDone());
  CHECK_NEAR(volume(cut.Shape()), before);
}

} // namespace

int main() {
  test::guarded("point_in_the_plane_of_a_far_face", test_point_in_the_plane_of_a_far_face);
  test::guarded("block_with_the_two_cavities", test_block_with_the_two_cavities);
  test::guarded("point_in_the_plane_of_a_flat_shell", test_point_in_the_plane_of_a_flat_shell);
  test::guarded("cut_by_a_flat_shell", test_cut_by_a_flat_shell);
  if (test::failures > 0) {
    std::fprintf(stderr, "%d check(s) failed\n", test::failures);
    return 1;
  }
  std::printf("classifier on a point in the plane of a far face: ok\n");
  return 0;
}
