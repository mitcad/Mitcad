// SPDX-License-Identifier: MIT
// A rim rounded with a ball larger than the roundings of the corners it runs
// round (mitcad#1977), with the port's patch 1760.
//
// A block whose sides are drafted inwards by 2.5 degrees, its four slanted
// corner edges rounded with 3, as a cast part's: its top's rim is a chain of
// four lines and four arcs where the top meets the corners' roundings, whose
// axes lean with the draft. The rim rounded with 6 (twice the corners'):
// round each corner the ball's centre at 6 from the corner's rounding,
// towards its axis, lies beyond that axis, so no ball of 6 touches the
// rounding from inside the block. The rolling ball's equations (patch
// 0075) then hold only for a ball that holds the rounding's section, whose
// sections fold back, and OCCT's walk failed on the four corners (one
// faulty contour). With patch 1760 a blend whose support curves along the
// guide more tightly than the ball, towards its centre, is walked with its
// sections in the guide's normal plane, as OCCT 8.0.1 walks every one; a
// ball of the support's own radius or smaller is the rolling ball as
// before. Without the draft the corners are analytic (ChFiKPart) and never
// walked.
//
// - OCCT's fillet alone rounds the rim with 4, 6 and 8, done and valid,
//   taking more away with each (without the patch 6 failed and 4 was
//   invalid).
// - In steps of 0.01 across the corners' radius (2.985 to 3.015; true
//   balls below it, sections in the guide's normal plane above it), done and
//   valid (without the patch 3.005 and 3.015 were invalid), and the step
//   over the corners' radius changes the material taken away as the steps
//   beside it do, within 30 %: the two ways meet there.

#include <cmath>
#include <cstdio>
#include <string>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepOffsetAPI_DraftAngle.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>

#include "bundled_test.hpp"

namespace {

int failures = 0;

void check(bool condition, const std::string& what) {
  if (!condition) {
    std::fprintf(stderr, "check failed: %s\n", what.c_str());
    ++failures;
  }
}

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

// The edges of `shape` whose middle lies at height z (within 1e-6) and
// whose faces are both of the given kinds: `slanted` for the corner edges
// (the drafted sides', between z 0 and the top), else the top's rim.
std::vector<TopoDS_Edge> edges_at(const TopoDS_Shape& shape, double z, bool slanted) {
  std::vector<TopoDS_Edge> edges;
  for (TopExp_Explorer ex(shape, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(ex.Current());
    TopoDS_Vertex a, b;
    TopExp::Vertices(edge, a, b);
    const double za = BRep_Tool::Pnt(a).Z();
    const double zb = BRep_Tool::Pnt(b).Z();
    const bool here = slanted ? std::abs(za - zb) > 1.0 : std::abs(za - z) < 1e-6 && std::abs(zb - z) < 1e-6;
    bool seen = false;
    for (const TopoDS_Edge& e : edges) {
      seen = seen || e.IsSame(edge);
    }
    if (here && !seen) {
      edges.push_back(edge);
    }
  }
  return edges;
}

// The block, 100 x 60 x 30, its sides drafted inwards by 2.5 degrees from
// its foot, its corner edges rounded with `corner`.
TopoDS_Shape drafted_block(double corner) {
  const TopoDS_Shape box = BRepPrimAPI_MakeBox(gp_Pnt(-50, -30, 0), 100, 60, 30).Shape();
  BRepOffsetAPI_DraftAngle draft(box);
  const gp_Pln neutral(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1));
  const double angle = 2.5 * M_PI / 180.0;
  for (TopExp_Explorer ex(box, TopAbs_FACE); ex.More(); ex.Next()) {
    const TopoDS_Face& face = TopoDS::Face(ex.Current());
    const gp_Dir normal = BRepAdaptor_Surface(face).Plane().Axis().Direction();
    if (std::abs(normal.Z()) < 1e-9) {
      draft.Add(face, gp_Dir(0, 0, 1), angle, neutral);
    }
  }
  draft.Build();
  check(draft.IsDone(), "the block's sides drafted");
  BRepFilletAPI_MakeFillet corners(draft.Shape());
  const std::vector<TopoDS_Edge> slanted = edges_at(draft.Shape(), 0, true);
  check(slanted.size() == 4, "four corner edges");
  for (const TopoDS_Edge& edge : slanted) {
    corners.Add(corner, edge);
  }
  corners.Build();
  check(corners.IsDone(), "the corners rounded");
  return corners.Shape();
}

// The top's rim of `block` rounded with `radius` by OCCT alone; the volume
// taken away, or a negative number where it failed.
double rim_rounded(const TopoDS_Shape& block, double radius) {
  BRepFilletAPI_MakeFillet rim(block);
  const std::vector<TopoDS_Edge> edges = edges_at(block, 30, false);
  check(edges.size() == 8, "the rim: four lines and four arcs");
  for (const TopoDS_Edge& edge : edges) {
    rim.Add(radius, edge);
  }
  try {
    rim.Build();
  } catch (const Standard_Failure& failure) {
    std::printf("radius %g: %s\n", radius, failure.what());
    return -1.0;
  }
  if (!rim.IsDone()) {
    std::printf("radius %g: not done, %d faulty contours\n", radius, rim.NbFaultyContours());
    return -1.0;
  }
  const bool valid = BRepCheck_Analyzer(rim.Shape()).IsValid();
  const double taken = volume_of(block) - volume_of(rim.Shape());
  std::printf("radius %g: valid %d, %.6f taken away\n", radius, valid ? 1 : 0, taken);
  check(valid, "the rim rounded with " + std::to_string(radius) + " is valid");
  return valid ? taken : -1.0;
}

}  // namespace

MITCAD_BUNDLED_TEST(fillet_ball_beyond_support) {
  try {
    const TopoDS_Shape block = drafted_block(3.0);
    double before = 0.0;
    for (const double radius : {4.0, 6.0, 8.0}) {
      const double taken = rim_rounded(block, radius);
      check(taken > before, "the rim rounded with " + std::to_string(radius) + ", more taken away");
      before = taken;
    }
    // Across the corners' radius, in steps of 0.01: the step over it (true
    // balls below, sections in the guide's normal plane above) changes the
    // material taken away as the steps beside it do, within 30 % (measured:
    // 3.45, 4.28 and 3.51 mm^3; just above the corners' radius OCCT's walk
    // without the patch left the result invalid).
    std::vector<double> taken;
    for (const double radius : {2.985, 2.995, 3.005, 3.015}) {
      taken.push_back(rim_rounded(block, radius));
    }
    check(taken[0] > 0.0 && taken[1] > taken[0] && taken[2] > taken[1] && taken[3] > taken[2],
          "the rim rounded on both sides of the corners' radius, more taken away");
    const double beside = (taken[1] - taken[0] + taken[3] - taken[2]) / 2.0;
    check(std::abs(taken[2] - taken[1] - beside) < 0.3 * beside, "the two ways meet at the corners' radius");
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "OCCT: %s\n", failure.what());
    return 1;
  }
  if (failures != 0) {
    std::fprintf(stderr, "%d checks failed\n", failures);
    return 1;
  }
  std::printf("ok\n");
  return 0;
}
