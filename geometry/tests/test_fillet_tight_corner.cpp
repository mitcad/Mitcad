// SPDX-License-Identifier: MIT
// A block's bottom outline rounded with a radius larger than the rounding
// of its vertical corners, built with OCCT's fillet alone. No ball rolls
// round such a corner: its centre would have to turn back about the
// corner's axis. OCCT walked the chain over the corner anyway and failed
// storing the walk (an approximation that runs backwards). The rounding
// now stops where the axes of the two walls' roundings cross, at the ball
// that touches the bottom and both walls; a vertex blend spans from that
// ball's centre to the corner's arc, and where the second wall leans
// towards or away from the bottom, the corner's cylinder turns about that
// wall's rounding until it stands square to the bottom (a torus). A stored
// design whose bottom outline is rounded with 0.5 round corners rounded
// with 0.3 is the case behind the patch 0970 of the OCCT port
// (third_party/vcpkg-ports, mitcad#343).

#include <cmath>
#include <cstdio>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepTools.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedMap.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>
#include <gp_Torus.hxx>

#include "check.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

constexpr double kR = 0.5;     // the bottom's rounding
constexpr double kCorner = 0.3; // the vertical corners' rounding
constexpr double kHalf = 7.0;  // the block's half width (x)
constexpr double kDepth = 10.0; // its depth (y)
constexpr double kHeight = 5.0; // its height (z)

// The volume, integrated to a relative error of 1e-9.
double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-9, true);
  return props.Mass();
}

// The block x in [-7, 7] on the bottom z = 0, its front wall through the
// bottom's edge y = 0 leaning by `lean` degrees (towards +y as it rises:
// the bottom's front edge acute), the back y = 10; its front corners
// rounded with kCorner (and the back ones too where `loop`).
TopoDS_Shape block(double lean, bool loop) {
  const double t = std::tan(lean * kPi / 180.0);
  BRepBuilderAPI_MakePolygon poly;
  poly.Add(gp_Pnt(-kHalf, 0, 0));
  poly.Add(gp_Pnt(-kHalf, kDepth, 0));
  poly.Add(gp_Pnt(-kHalf, kDepth, kHeight));
  poly.Add(gp_Pnt(-kHalf, kHeight * t, kHeight));
  poly.Close();
  const TopoDS_Face profile = BRepBuilderAPI_MakeFace(poly.Wire(), true).Face();
  const TopoDS_Shape prism = BRepPrimAPI_MakePrism(profile, gp_Vec(2 * kHalf, 0, 0)).Shape();
  BRepFilletAPI_MakeFillet corners(prism);
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(prism, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    BRepAdaptor_Curve e(TopoDS::Edge(edges(i)));
    const gp_Pnt a = e.Value(e.FirstParameter()), b = e.Value(e.LastParameter());
    const bool rising = std::abs(a.Z() - b.Z()) > 1.0 && std::abs(std::abs(a.X()) - kHalf) < 1e-9 &&
                        std::abs(a.X() - b.X()) < 1e-9;
    const bool front = std::abs((a.Z() < b.Z() ? a : b).Y()) < 1e-9; // (its foot on the front edge)
    if (rising && (front || loop)) {
      corners.Add(kCorner, TopoDS::Edge(edges(i)));
    }
  }
  corners.Build();
  return corners.IsDone() ? corners.Shape() : TopoDS_Shape();
}

// The ball touching the bottom and the walls through the corner whose side
// wall is x = sx * kHalf and whose other wall has the outward normal `no`
// through the point `po`: the centre C, kR from all three.
gp_Pnt ball_centre(double sx, const gp_Dir& no, const gp_Pnt& po) {
  // x = sx (kHalf - kR), z = kR, (P - po).no = -kR: solve for y.
  const double x = sx * (kHalf - kR), z = kR;
  const double y = (-kR + no.X() * (po.X() - x) + no.Y() * po.Y() + no.Z() * (po.Z() - z)) / no.Y();
  return gp_Pnt(x, y, z);
}

struct Corner {
  gp_Pnt C;  // the ball's centre
  gp_Pnt O;  // the vertex blend's arc's centre, in the plane z = kR
  gp_Dir np; // the side wall's outward normal
};

// The distance from C along d (in the plane z = kR) to the arc of radius
// kCorner about O: the far crossing.
double reach(const Corner& c, const gp_Vec& d) {
  const gp_Vec w(c.O, c.C);
  const double b = w.Dot(d), q = w.SquareMagnitude() - kCorner * kCorner;
  return -b + std::sqrt(std::max(0.0, b * b - q));
}

// The vertex blend: C + sin t A(u) + cos t kR (0, 0, -1), A(u) on the arc:
// each point's plan distance s from C and its height h below C satisfy
// (s / reach)^2 + (h / kR)^2 = 1.
double off_vertex_blend(const Corner& c, const gp_Pnt& p) {
  const double h = c.C.Z() - p.Z();
  gp_Vec w(c.C, p);
  w.SetZ(0.0);
  const double s = w.Magnitude();
  if (s < 1e-9) {
    return std::abs(h - kR);
  }
  const double rho = reach(c, w / s);
  return std::abs(std::sqrt((s / rho) * (s / rho) + (h / kR) * (h / kR)) - 1.0) * kR;
}

struct Result {
  TopoDS_Shape shape;
  int tori = 0;
  int blends = 0; // B-spline faces: the vertex blends
  double worst = 0.0; // the vertex blends' largest distance off their surface
  double volume = 0.0;
};

// The bottom's edges rounded with kR (the back one only where `back`).
Result round_bottom(const TopoDS_Shape& body, const std::vector<Corner>& corners, bool back) {
  Result r;
  BRepFilletAPI_MakeFillet fillet(body);
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(body, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    BRepAdaptor_Curve e(TopoDS::Edge(edges(i)));
    const gp_Pnt a = e.Value(e.FirstParameter()), b = e.Value(e.LastParameter());
    const bool atBack = std::abs(a.Y() - kDepth) < 1e-9 && std::abs(b.Y() - kDepth) < 1e-9;
    if (std::abs(a.Z()) < 1e-9 && std::abs(b.Z()) < 1e-9 && (back || !atBack) &&
        fillet.Contour(TopoDS::Edge(edges(i))) == 0) {
      fillet.Add(kR, TopoDS::Edge(edges(i)));
    }
  }
  try {
    OCC_CATCH_SIGNALS
    fillet.Build();
  } catch (const Standard_Failure& e) {
    std::printf("fillet threw: %s\n", e.what());
  }
  if (!fillet.IsDone()) {
    return r;
  }
  r.shape = fillet.Shape();
  r.volume = volume_of(r.shape);
  for (TopExp_Explorer ex(r.shape, TopAbs_FACE); ex.More(); ex.Next()) {
    const TopoDS_Face& f = TopoDS::Face(ex.Current());
    BRepAdaptor_Surface s(f);
    if (s.GetType() == GeomAbs_Torus) {
      const gp_Torus t = s.Torus();
      CHECK(std::abs(t.MajorRadius() - (kR - kCorner)) < 1e-9);
      CHECK(std::abs(t.MinorRadius() - kCorner) < 1e-9);
      // About the front rounding's axis (along x through the ball's
      // centre), centred where the corner's cylinder meets the front wall.
      CHECK(std::abs(std::abs(t.Axis().Direction().X()) - 1) < 1e-9);
      bool centred = false;
      for (const Corner& c : corners) {
        const gp_Pnt o(c.C.X() + (kR - kCorner) * c.np.X(), c.C.Y(), c.C.Z());
        centred = centred || t.Location().Distance(o) < 1e-9;
      }
      CHECK(centred);
      ++r.tori;
    } else if (s.GetType() == GeomAbs_BSplineSurface) {
      ++r.blends;
      double u0 = 0, u1 = 0, v0 = 0, v1 = 0;
      BRepTools::UVBounds(f, u0, u1, v0, v1);
      for (int iu = 0; iu <= 8; ++iu) {
        for (int iv = 0; iv <= 8; ++iv) {
          const gp_Pnt p = s.Value(u0 + (u1 - u0) * iu / 8, v0 + (v1 - v0) * iv / 8);
          double best = 1e9;
          for (const Corner& c : corners) {
            if (p.Distance(c.C) < 2 * kR) {
              best = std::min(best, off_vertex_blend(c, p));
            }
          }
          r.worst = std::max(r.worst, best);
        }
      }
    }
  }
  return r;
}

// The front corners of the block leaning by `lean` degrees.
std::vector<Corner> front_corners(double lean) {
  const double a = lean * kPi / 180.0;
  const gp_Dir no(0, -std::cos(a), std::sin(a)); // the front wall's outward normal
  std::vector<Corner> out;
  for (const double sx : {-1.0, 1.0}) {
    Corner c;
    c.C = ball_centre(sx, no, gp_Pnt(0, 0, 0));
    c.np = gp_Dir(sx, 0, 0);
    // O: rho along np and rho towards the corner along the side (-y).
    const double rho = kR - kCorner;
    c.O = gp_Pnt(c.C.X() + rho * sx, c.C.Y() - rho, kR);
    out.push_back(c);
  }
  return out;
}

// The material the rounding takes from a block with square walls: the
// rounding's section (1 - pi/4) R^2 along the edges up to the sections
// through the corners' balls, and at each corner the two roundings' pieces
// up to their ellipses (rho R^2 / 6 each) and the vertex blend over the
// plan between the ball's centre and the corner's arc ((R / 3) times that
// plan's area rho rk + pi rk^2 / 4).
double square_loss(double straight, int corners) {
  const double rho = kR - kCorner;
  const double corner = 2 * rho * kR * kR / 6 + (kR / 3) * (rho * kCorner + kPi * kCorner * kCorner / 4);
  return (1 - kPi / 4) * kR * kR * straight + corners * corner;
}

void check_open(double lean) {
  const TopoDS_Shape body = block(lean, false);
  CHECK(!body.IsNull());
  if (body.IsNull()) {
    return;
  }
  const Result r = round_bottom(body, front_corners(lean), false);
  std::printf("front wall leaning by %g degrees:", lean);
  CHECK(!r.shape.IsNull());
  if (r.shape.IsNull()) {
    std::printf(" not done\n");
    return;
  }
  CHECK(BRepCheck_Analyzer(r.shape).IsValid());
  CHECK(r.tori == (lean == 0.0 ? 0 : 2));
  CHECK(r.blends == 2);
  CHECK(r.worst < 1e-6);
  const double loss = volume_of(body) - r.volume;
  std::printf(" %d tori, %d vertex blends off by %.2g, material taken %.7f", r.tori, r.blends, r.worst, loss);
  if (lean == 0.0) {
    // The side roundings from the balls' sections to the back (cut square
    // by it), the front one between the two balls' sections.
    const double straight = 2 * (kDepth - kR) + 2 * (kHalf - kR);
    const double want = square_loss(straight, 2);
    std::printf(", %.7f wanted", want);
    CHECK(std::abs(loss - want) < 1e-6);
  }
  std::printf("\n");
}

void check_loop() {
  const TopoDS_Shape body = block(0.0, true);
  CHECK(!body.IsNull());
  if (body.IsNull()) {
    return;
  }
  std::vector<Corner> corners = front_corners(0.0);
  for (const double sx : {-1.0, 1.0}) {
    Corner c;
    c.C = gp_Pnt(sx * (kHalf - kR), kDepth - kR, kR);
    c.np = gp_Dir(sx, 0, 0);
    const double rho = kR - kCorner;
    c.O = gp_Pnt(c.C.X() + rho * sx, c.C.Y() + rho, kR);
    corners.push_back(c);
  }
  const Result r = round_bottom(body, corners, true);
  std::printf("the whole outline:");
  CHECK(!r.shape.IsNull());
  if (r.shape.IsNull()) {
    std::printf(" not done\n");
    return;
  }
  CHECK(BRepCheck_Analyzer(r.shape).IsValid());
  CHECK(r.tori == 0);
  CHECK(r.blends == 4);
  CHECK(r.worst < 1e-6);
  const double loss = volume_of(body) - r.volume;
  const double want = square_loss(2 * (kDepth - 2 * kR) + 2 * (2 * kHalf - 2 * kR), 4);
  std::printf(" %d vertex blends off by %.2g, material taken %.7f, %.7f wanted\n", r.blends, r.worst, loss, want);
  CHECK(std::abs(loss - want) < 1e-6);
}

void test_bottom_rounded_past_corners_tighter_than_the_ball() {
  for (const double lean : {0.0, 15.0}) {
    check_open(lean);
  }
  check_loop();
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(fillet_tight_corner) {
  test::guarded("test_bottom_rounded_past_corners_tighter_than_the_ball",
                test::test_bottom_rounded_past_corners_tighter_than_the_ball);
  if (test::failures == 0) {
    std::puts("test_fillet_tight_corner: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
