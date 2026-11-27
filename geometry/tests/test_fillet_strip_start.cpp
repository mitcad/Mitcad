// SPDX-License-Identifier: MIT
// A rounding along an edge of a strip the ball cannot touch, built with
// OCCT's fillet alone: a ring's outer cylinder cut by a notch, one of the
// notch's corners left with a strip of an earlier rounding of a smaller
// radius (5 µm wide, bending more sharply than the ball) between the notch
// and the cylinder, as in a stored design's endcap. The rounding of the
// strip's edge with the notch is the ball touching the notch and the
// cylinder, the strip consumed, as at the corner without the strip: the
// case behind the patch 0880 of the OCCT port (third_party/vcpkg-ports,
// mitcad#343).

#include <cmath>
#include <cstdio>
#include <utility>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Tool.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GC_MakeSegment.hxx>
#include <GProp_GProps.hxx>
#include <GeomAbs_SurfaceType.hxx>
#include <NCollection_IndexedMap.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

// The volume, integrated to a relative error of 1e-9.
double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

double area_of(const TopoDS_Shape& face) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(face, props);
  return props.Mass();
}

struct Circle2 {
  double x;
  double y;
  double r;
};

// The point of circle a's intersections with circle b nearer (x, y).
gp_Pnt meet(const Circle2& a, const Circle2& b, double x, double y) {
  const double dx = b.x - a.x, dy = b.y - a.y;
  const double d = std::hypot(dx, dy);
  const double along = (d * d + a.r * a.r - b.r * b.r) / (2.0 * d);
  const double half = std::sqrt(std::max(0.0, a.r * a.r - along * along));
  const double mx = a.x + dx / d * along, my = a.y + dy / d * along;
  const gp_Pnt p(mx - dy / d * half, my + dx / d * half, 0.0);
  const gp_Pnt q(mx + dy / d * half, my - dx / d * half, 0.0);
  const gp_Pnt at(x, y, 0.0);
  return p.Distance(at) <= q.Distance(at) ? p : q;
}

// The point of circle c at the angle halfway between those of p and q
// (the shorter way).
gp_Pnt halfway(const Circle2& c, const gp_Pnt& p, const gp_Pnt& q) {
  const double a = std::atan2(p.Y() - c.y, p.X() - c.x);
  double b = std::atan2(q.Y() - c.y, q.X() - c.x);
  if (b - a > kPi) {
    b -= 2.0 * kPi;
  } else if (a - b > kPi) {
    b += 2.0 * kPi;
  }
  const double m = 0.5 * (a + b);
  return gp_Pnt(c.x + c.r * std::cos(m), c.y + c.r * std::sin(m), 0.0);
}

TopoDS_Edge arc(const gp_Pnt& a, const gp_Pnt& m, const gp_Pnt& b) {
  return BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(a, m, b).Value()).Edge();
}

// The vertical edge of the prism through (x, y).
TopoDS_Edge vertical_edge_at(const TopoDS_Shape& body, const gp_Pnt& at) {
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(body, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    const TopoDS_Edge& e = TopoDS::Edge(edges(i));
    const gp_Pnt a = BRep_Tool::Pnt(TopExp::FirstVertex(e));
    const gp_Pnt b = BRep_Tool::Pnt(TopExp::LastVertex(e));
    if (std::abs(a.Z() - b.Z()) > 1.0 && std::hypot(a.X() - at.X(), a.Y() - at.Y()) < 1e-7) {
      return e;
    }
  }
  return TopoDS_Edge();
}

// The faces of the result on cylinders of radius r.
int cylinders_of_radius(const TopoDS_Shape& shape, double r) {
  int n = 0;
  for (TopExp_Explorer ex(shape, TopAbs_FACE); ex.More(); ex.Next()) {
    BRepAdaptor_Surface s(TopoDS::Face(ex.Current()));
    n += s.GetType() == GeomAbs_Cylinder && std::abs(s.Cylinder().Radius() - r) < 1e-9 ? 1 : 0;
  }
  return n;
}

// The areas of the rounding's faces: those neither planar nor on one of
// the input's cylinders (radii 0.4, 1.5 and 15.4).
std::vector<double> blend_face_areas(const TopoDS_Shape& result) {
  std::vector<double> out;
  for (TopExp_Explorer ex(result, TopAbs_FACE); ex.More(); ex.Next()) {
    BRepAdaptor_Surface s(TopoDS::Face(ex.Current()));
    if (s.GetType() == GeomAbs_Plane) {
      continue;
    }
    if (s.GetType() == GeomAbs_Cylinder) {
      const double r = s.Cylinder().Radius();
      if (std::abs(r - 0.4) < 1e-9 || std::abs(r - 1.5) < 1e-9 || std::abs(r - 15.4) < 1e-9) {
        continue;
      }
    }
    out.push_back(area_of(ex.Current()));
  }
  return out;
}

double rounding_removes(const TopoDS_Shape& body, const std::vector<TopoDS_Edge>& edges, double r,
                        TopoDS_Shape* result) {
  BRepFilletAPI_MakeFillet fillet(body);
  for (const TopoDS_Edge& e : edges) {
    fillet.Add(r, e);
  }
  try {
    OCC_CATCH_SIGNALS
    fillet.Build();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "fillet strip start test: %s\n", failure.what());
  }
  CHECK(fillet.IsDone());
  if (!fillet.IsDone()) {
    return -1.0;
  }
  CHECK(BRepCheck_Analyzer(fillet.Shape()).IsValid());
  if (result != nullptr) {
    *result = fillet.Shape();
  }
  return volume_of(body) - volume_of(fillet.Shape());
}

void test_rounding_along_a_strip_the_ball_cannot_touch() {
  const Circle2 ring{0.0, 0.0, 15.4};
  const Circle2 notch{15.0, 0.0, 1.5};
  const Circle2 strip{15.004, 1.7, 0.4};
  // The notch's corners: the lower one sharp, the upper one with the strip
  // (0.005 wide) between the notch and the ring.
  const gp_Pnt lower = meet(ring, notch, 15.33, -1.46);
  const gp_Pnt a = meet(strip, notch, 15.327, 1.464);
  const gp_Pnt b = meet(strip, ring, 15.330, 1.468);
  CHECK(a.Distance(b) > 1e-3 && a.Distance(b) < 1e-2);
  const gp_Pnt bottom(0.0, -15.4, 0.0), top(0.0, 15.4, 0.0);
  BRepBuilderAPI_MakeWire wire;
  wire.Add(arc(bottom, halfway(ring, bottom, lower), lower));
  wire.Add(arc(lower, gp_Pnt(13.5, 0.0, 0.0), a));
  wire.Add(arc(a, halfway(strip, a, b), b));
  wire.Add(arc(b, halfway(ring, b, top), top));
  wire.Add(BRepBuilderAPI_MakeEdge(GC_MakeSegment(top, bottom).Value()).Edge());
  CHECK(wire.IsDone());
  const TopoDS_Face profile = BRepBuilderAPI_MakeFace(wire.Wire(), true).Face();
  const TopoDS_Shape body = BRepPrimAPI_MakePrism(profile, gp_Vec(0.0, 0.0, 15.0)).Shape();
  CHECK(BRepCheck_Analyzer(body).IsValid());
  CHECK(cylinders_of_radius(body, 0.4) == 1);
  const TopoDS_Edge sharp = vertical_edge_at(body, lower);
  const TopoDS_Edge beside = vertical_edge_at(body, a);
  CHECK(!sharp.IsNull() && !beside.IsNull());
  if (sharp.IsNull() || beside.IsNull()) {
    return;
  }
  const double r = 1.0;
  // The sharp corner: the reference.
  TopoDS_Shape plain;
  const double removed_sharp = rounding_removes(body, {sharp}, r, &plain);
  CHECK(removed_sharp > 0.0);
  // The strip's edge with the notch: the ball touches the notch and the
  // ring, not the strip, which it consumes.
  TopoDS_Shape stripped;
  const double removed_strip = rounding_removes(body, {beside}, r, &stripped);
  if (removed_strip < 0.0) {
    return;
  }
  CHECK(cylinders_of_radius(stripped, 0.4) == 0);
  // The material the strip's corner lacks is a sliver 5 µm wide (8e-6 mm²
  // of the section).
  CHECK_NEAR_TOL(removed_strip, removed_sharp, 1e-4);
  CHECK(removed_strip < removed_sharp);
  const std::vector<double> sharp_new = blend_face_areas(plain);
  const std::vector<double> strip_new = blend_face_areas(stripped);
  CHECK(sharp_new.size() == 1 && strip_new.size() == 1);
  if (sharp_new.size() == 1 && strip_new.size() == 1) {
    // The two corners' roundings are mirror images.
    CHECK_NEAR_TOL(strip_new[0], sharp_new[0], 1e-6);
  }
  // Both corners at once, as the stored design rounds them.
  const double removed_both = rounding_removes(body, {sharp, beside}, r, nullptr);
  CHECK_NEAR_TOL(removed_both, removed_sharp + removed_strip, 1e-6);
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(fillet_strip_start) {
  test::guarded("test_rounding_along_a_strip_the_ball_cannot_touch",
                test::test_rounding_along_a_strip_the_ball_cannot_touch);
  if (test::failures == 0) {
    std::puts("test_fillet_strip_start: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
