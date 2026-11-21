// SPDX-License-Identifier: MIT
// Port patch 0821: both edges of a lens rounded, a face of two edges, one
// concave and one convex, narrower than the roundings: OCCT rolled each
// rounding on the other's edge and the concave one found no start, so the
// fillet failed. The patch joins the two roundings in one S-shaped section
// (DeferLensRoundings, JoinLensPair), the lens consumed: the concave blend
// rolls on the lens's convex edge, the convex blend's ball touches the key
// and the concave ball from outside, and the shaft cuts the convex blend
// towards the lens's ends.
//
// A shaft of radius 25 (z from 0 to 60) with a key along it: a cylinder of
// radius 5.1854 about (0, 20.3525) from z = 21.432 to 41.432, standing out
// of the shaft by up to 0.538. The key's end at z = 21.432 is a lens
// between the key's convex arc and the shaft's concave arc (x within
// +-2.5454). Both of the lens's edges rounded with 2, wider than the lens:
// built, valid, no face left on the lens's plane, the convex blend's
// contact on the key ending on the key's sides at z = 21.584, the material
// added on both sides of x = 0 alike. The boundary: the key's far end (a
// lens at z = 41.432) is not rounded; rounding only the lens's convex edge
// with 0.2, narrower than the lens in its middle, is built as before.

#include <cmath>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>

#include "port_test.hpp"

namespace {

constexpr double kLens = 21.432; // the lens's plane

TopoDS_Shape key_on_a_shaft() {
  const TopoDS_Shape shaft =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1), gp_Dir(1, 0, 0)), 25.0, 60.0).Shape();
  const TopoDS_Shape key =
    BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 20.3525, kLens), gp_Dir(0, 0, 1), gp_Dir(0, -1, 0)), 5.1854, 20.0)
      .Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(shaft, key).Shape(), true, true, true);
  unify.Build();
  return unify.Shape();
}

// The lens's edges (on its plane, near the key).
std::vector<TopoDS_Edge> lens_edges(const TopoDS_Shape& body) {
  return port::edges_where(body, [](const gp_Pnt& p) {
    return std::abs(p.Z() - kLens) < 1e-6 && std::abs(p.X()) < 3.0 && p.Y() > 20.0;
  });
}

bool has_vertex_near(const TopoDS_Shape& shape, const gp_Pnt& p, double tol) {
  for (TopExp_Explorer v(shape, TopAbs_VERTEX); v.More(); v.Next()) {
    if (BRep_Tool::Pnt(TopoDS::Vertex(v.Current())).Distance(p) < tol) {
      return true;
    }
  }
  return false;
}

// Whether a face of the shape lies on the lens's plane.
bool has_lens_face(const TopoDS_Shape& shape) {
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    const BRepAdaptor_Surface s(TopoDS::Face(f.Current()));
    if (s.GetType() == GeomAbs_Plane && std::abs(s.Plane().Location().Z() - kLens) < 1e-6 &&
        std::abs(std::abs(s.Plane().Axis().Direction().Z()) - 1.0) < 1e-9) {
      return true;
    }
  }
  return false;
}

double volume_in(const TopoDS_Shape& shape, const gp_Pnt& a, const gp_Pnt& b) {
  return port::volume_of(BRepAlgoAPI_Common(shape, BRepPrimAPI_MakeBox(a, b).Shape()).Shape());
}

// Both edges rounded with 2: one S-shaped section, the lens consumed.
void test_lens_rounded_on_both_edges_is_consumed() {
  const TopoDS_Shape body = key_on_a_shaft();
  CHECK(port::valid(body));
  const std::vector<TopoDS_Edge> lens = lens_edges(body);
  CHECK(lens.size() == 2);
  const TopoDS_Shape result = port::rounded(body, lens, 2.0);
  CHECK(port::valid(result));
  if (!port::valid(result)) {
    return;
  }
  CHECK(!has_lens_face(result));
  // The convex blend's contact on the key ends on the key's sides.
  CHECK(has_vertex_near(result, gp_Pnt(2.5454, 24.8701, 21.584), 1e-3));
  CHECK(has_vertex_near(result, gp_Pnt(-2.5454, 24.8701, 21.584), 1e-3));
  // Material added on both sides of x = 0 alike (within 0.01: the walks
  // are not exactly symmetric).
  const gp_Pnt lo(-10, 15, 15), hi(10, 30, 30), mid_lo(0, 15, 15), mid_hi(0, 30, 30);
  const double left  = volume_in(result, lo, mid_hi) - volume_in(body, lo, mid_hi);
  const double right = volume_in(result, mid_lo, hi) - volume_in(body, mid_lo, hi);
  CHECK(left > 0.0);
  CHECK(std::abs(left - right) < 0.01);
}

// The boundary: only the lens's convex edge rounded with 0.2, narrower than
// the lens in its middle; no lens pair: built and valid, the lens keeps a
// face on its plane.
void test_one_narrow_rounding_of_the_lens_is_as_before() {
  const TopoDS_Shape             body = key_on_a_shaft();
  const std::vector<TopoDS_Edge> lens = lens_edges(body);
  std::vector<TopoDS_Edge>       convex;
  for (const TopoDS_Edge& edge : lens) {
    const BRepAdaptor_Curve curve(edge);
    const gp_Pnt            m = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
    if (m.Y() > 25.0) {
      convex.push_back(edge);
    }
  }
  CHECK(convex.size() == 1);
  const TopoDS_Shape result = port::rounded(body, convex, 0.2);
  CHECK(port::valid(result));
  if (port::valid(result)) {
    CHECK(has_lens_face(result));
  }
}

} // namespace

MITCAD_PORT_TEST("0821", test_lens_rounded_on_both_edges_is_consumed);
MITCAD_PORT_TEST("0821", test_one_narrow_rounding_of_the_lens_is_as_before);
