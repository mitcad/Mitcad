// SPDX-License-Identifier: MIT
// Port patch 2291: a tight corner at a cylinder's rim (1590) is obtuse by
// the other wall's normal where the corner's ball touches it (e0), not
// where the wall meets the face U (mitcad#2291, GitHub mitcad/Mitcad#592).
//
// The body, as the reporter's: a block 30 long (x), 10 high (y) and 20
// deep (z), its profile in z = 0 the bottom from the origin to x = 30, the
// right side, the top y = 10 back to x = 10, and a concave quarter circle
// of radius rw = 10 about (0, 10) down to the origin (tangent to the
// bottom there, square to the top at (10, 10)): the cut-out's axis lies in
// the top's plane. The end face z = 20's edge with the cut-out is rounded
// with rk = 2 (a torus K about the cut-out's axis, its tube circle of
// radius 12 at z = 18; the chain runs on along the bottom's edge), then
// the end face's edge with the top with R = 3: the chain runs on round K's
// arc on the top (radius 2, tighter than R) and along the top's edge with
// the cut-out.
//
// The ball C touching the top, the end face and the cut-out: centre
// (sqrt(13^2 - 3^2), 7, 17); the cut-out's normal there, e0, turns 3 / 13
// towards the top's normal, while at the cut-out's line on the top it is
// square to it. Taken for square, the corner was the acute one: the
// vertex blend's arc in the plane y = 7 and a torus turning it to K's
// meridian at e0, back over the part of K that is kept (0.03 mm off it),
// and the cut-out's rounding carried past its contact with the cut-out (a
// face of 0.0063 mm2): the surface folded at the corner. Obtuse, the
// corner's own ball (rk) rolls on between the cut-out and the end face's
// rounding, then between the two roundings: every point of the corner's
// faces rk from a centre R - rk from the end face's rounding's axis and
// either rk inside the cut-out (rw + rk from its axis) or R - rk from the
// cut-out's rounding's axis.
//
// Checks that fail without the patch: the result valid, free of
// self-interference, no face under 0.01 mm2, its corner faces on the
// surface the corner's ball sweeps within 5e-4 mm and on the ball by the
// centre along their normals within 2e-3 mm (the faces are fitted within
// 1e-4; their normals' error times rk). The boundary: both edges rounded with rk
// (no tight corner, the ball stays put round K's arc): valid and clean
// with and without the patch.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <vector>

#include <BOPAlgo_ArgumentAnalyzer.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepTools.hxx>
#include <Bnd_Box.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Torus.hxx>
#include <gp_Vec.hxx>

#include "port_test.hpp"

namespace {

constexpr double kRw = 10.0;   // the cut-out's radius; its axis x = 0, y = kH
constexpr double kH = 10.0;    // the block's height: the top y = kH
constexpr double kLong = 30.0; // its length along x
constexpr double kDeep = 20.0; // its depth along z: the end face z = kDeep
constexpr double kRk = 2.0;    // the end face's edge with the cut-out
constexpr double kR = 3.0;     // the end face's edge with the top

TopoDS_Shape block() {
  const double s = std::sqrt(0.5);
  const gp_Pnt a(0, 0, 0);
  const gp_Pnt b(kLong, 0, 0);
  const gp_Pnt c(kLong, kH, 0);
  const gp_Pnt d(kRw, kH, 0);
  const gp_Pnt m(kRw * s, kH - kRw * s, 0); // the arc's middle
  BRepBuilderAPI_MakeWire wire;
  wire.Add(BRepBuilderAPI_MakeEdge(a, b).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(b, c).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(c, d).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(GC_MakeArcOfCircle(d, m, a).Value()).Edge());
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(wire.Wire(), true).Face();
  return BRepPrimAPI_MakePrism(face, gp_Vec(0, 0, kDeep)).Shape();
}

// The block's end face's edge with the cut-out rounded with kRk, then its
// edge with the top with r; null where either fails.
TopoDS_Shape rounded_twice(double r) {
  const TopoDS_Shape body = block();
  const std::vector<TopoDS_Edge> arc = port::edges_where(body, [](const gp_Pnt& p) {
    return std::abs(p.Z() - kDeep) < 1e-7 && std::abs(std::hypot(p.X(), p.Y() - kH) - kRw) < 1e-7;
  });
  CHECK(arc.size() == 1);
  if (arc.size() != 1) {
    return TopoDS_Shape();
  }
  const TopoDS_Shape first = port::rounded(body, arc, kRk);
  CHECK(port::valid(first));
  if (first.IsNull()) {
    return first;
  }
  const std::vector<TopoDS_Edge> top = port::edges_where(first, [](const gp_Pnt& p) {
    return std::abs(p.Z() - kDeep) < 1e-7 && std::abs(p.Y() - kH) < 1e-7 && p.X() > kRw + kRk + 1.0;
  });
  CHECK(top.size() == 1);
  if (top.size() != 1) {
    return TopoDS_Shape();
  }
  return port::rounded(first, top, r);
}

// The shape's faces intersect neither themselves nor each other.
bool free_of_self_interference(const TopoDS_Shape& shape) {
  BOPAlgo_ArgumentAnalyzer analyzer;
  analyzer.SetShape1(shape);
  analyzer.SelfInterMode() = true;
  analyzer.Perform();
  return !analyzer.HasFaulty();
}

double smallest_face(const TopoDS_Shape& shape) {
  double smallest = -1.0;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    const double a = port::area_of(f.Current());
    smallest = smallest < 0.0 ? a : std::min(smallest, a);
  }
  return smallest;
}

// How far point c (a ball's centre) is from the obtuse corner's ball: R -
// rk from the end face's rounding's axis (along x at y = kH - kR, z = kDeep
// - kR) and either R - rk from the cut-out's rounding's axis (along z at
// x = sqrt((rw + R)^2 - R^2), y = kH - kR) or rk inside the cut-out.
double off_corner_ball(const gp_Pnt& c) {
  const double xo = std::sqrt((kRw + kR) * (kRw + kR) - kR * kR);
  const double dp = std::hypot(c.Y() - (kH - kR), c.Z() - (kDeep - kR)) - (kR - kRk);
  const double dO = std::hypot(c.X() - xo, c.Y() - (kH - kR)) - (kR - kRk);
  const double dK = std::hypot(c.X(), c.Y() - kH) - (kRw + kRk);
  return std::abs(dp) + std::min(std::abs(dO), std::abs(dK));
}

// The centres of the corner's ball, sampled along one of their two curves:
// R - rk from the end face's rounding's axis, at the angle t about it from
// the top's inward normal (t = 0: where the two roundings' contacts on the
// top meet) towards the end face (t = pi / 2: K's own section, where the
// end face's rounding's contact on it reaches K), and either rk inside the
// cut-out (`strip`) or R - rk from the cut-out's rounding's axis, on the
// cut-out's side of it (the triangle). Each curve smooth and a little past
// the corner's range both ways: a face on a curve's balls lies on the outer
// side of their envelope, at rk from the nearest centre of that curve. (The
// other curve's balls past the junction of the strip and the triangle reach
// into its face: each face is measured against one curve.)
std::vector<gp_Pnt> corner_ball_centres(bool strip) {
  const double xo = std::sqrt((kRw + kR) * (kRw + kR) - kR * kR);
  const double rho = kR - kRk;
  const int steps = 6000;
  std::vector<gp_Pnt> centres;
  for (int i = 0; i <= steps; ++i) {
    const double t = -0.3 + (test::kPi / 2 + 0.6) * i / steps;
    const double y = kH - kR + rho * std::cos(t);
    const double z = kDeep - kR + rho * std::sin(t);
    const double x = strip ? std::sqrt((kRw + kRk) * (kRw + kRk) - (y - kH) * (y - kH)) : xo - rho * std::sin(t);
    centres.emplace_back(x, y, z);
  }
  return centres;
}

// How far point p (on a corner face) is from the surface the balls centred
// on `centres` sweep: its distance from the nearest centre, less rk. Unlike
// the centre found along the face's normal (off_corner_ball), it does not
// take the fitted surface's normal, whose error the radius multiplies.
double off_corner_surface(const gp_Pnt& p, const std::vector<gp_Pnt>& centres) {
  double nearest = -1.0;
  for (const gp_Pnt& c : centres) {
    const double d = p.Distance(c);
    nearest = nearest < 0.0 ? d : std::min(nearest, d);
  }
  return std::abs(nearest - kRk);
}

// The corner's faces (in the box about the corner, neither planes nor
// cylinders nor K's torus: not the walls, the roundings along the edges or
// K): how many, their points' largest distance off the corner's ball (the
// centre found along the face's normal) and off the surface it sweeps.
struct Corner {
  int faces = 0;
  double worst = 0.0;
  double worst_surface = 0.0;
};

Corner corner_faces(const TopoDS_Shape& shape) {
  Corner out;
  const std::vector<gp_Pnt> strip = corner_ball_centres(true);
  const std::vector<gp_Pnt> triangle = corner_ball_centres(false);
  for (TopExp_Explorer ex(shape, TopAbs_FACE); ex.More(); ex.Next()) {
    const TopoDS_Face& f = TopoDS::Face(ex.Current());
    const BRepAdaptor_Surface s(f);
    if (s.GetType() == GeomAbs_Plane || s.GetType() == GeomAbs_Cylinder) {
      continue;
    }
    // (K itself, or a piece of it.)
    if (s.GetType() == GeomAbs_Torus && std::abs(s.Torus().MajorRadius() - (kRw + kRk)) < 1e-7 &&
        std::abs(s.Torus().MinorRadius() - kRk) < 1e-7) {
      continue;
    }
    Bnd_Box box;
    BRepBndLib::Add(f, box);
    double x0 = 0, y0 = 0, z0 = 0, x1 = 0, y1 = 0, z1 = 0;
    box.Get(x0, y0, z0, x1, y1, z1);
    if (x0 < kRw - 1.5 || x1 > kRw + kR + 3.0 || y0 < kH - kR - 1.5 || z0 < kDeep - kR - 2.0) {
      continue;
    }
    ++out.faces;
    // (Off the strip's balls and off the triangle's: the face is one of
    // them, the nearer.)
    double off_strip = 0.0;
    double off_triangle = 0.0;
    double u0 = 0, u1 = 0, v0 = 0, v1 = 0;
    BRepTools::UVBounds(f, u0, u1, v0, v1);
    for (int iu = 0; iu <= 8; ++iu) {
      for (int iv = 0; iv <= 8; ++iv) {
        const double u = u0 + (u1 - u0) * (0.02 + 0.96 * iu / 8);
        const double v = v0 + (v1 - v0) * (0.02 + 0.96 * iv / 8);
        if (BRepClass_FaceClassifier(f, gp_Pnt2d(u, v), 1e-7).State() != TopAbs_IN) {
          continue;
        }
        gp_Pnt p;
        gp_Vec du, dv;
        s.D1(u, v, p, du, dv);
        off_strip = std::max(off_strip, off_corner_surface(p, strip));
        off_triangle = std::max(off_triangle, off_corner_surface(p, triangle));
        gp_Vec n = du.Crossed(dv);
        if (n.Magnitude() < 1e-12) {
          continue;
        }
        n.Normalize();
        if (f.Orientation() == TopAbs_REVERSED) {
          n.Reverse();
        }
        // (The ball's centre: rk into the material.)
        out.worst = std::max(out.worst, off_corner_ball(p.Translated(-kRk * n)));
      }
    }
    out.worst_surface = std::max(out.worst_surface, std::min(off_strip, off_triangle));
  }
  return out;
}

// The top's edge rounded with 3 past the cut-out's rim rounded with 2: the
// corner's own ball closes it.
void test_top_rounded_past_a_cut_out_rim_whose_axis_lies_in_the_top() {
  const TopoDS_Shape first = rounded_twice(kRk); // (the boundary below)
  const TopoDS_Shape result = rounded_twice(kR);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(port::valid(result));
  CHECK(free_of_self_interference(result));
  const double smallest = smallest_face(result);
  const Corner corner = corner_faces(result);
  std::printf("top past the rim: %d faces, the smallest %.6f mm2, %d corner faces off the corner's ball by %.2g "
              "(centres along the normal) and off its surface by %.2g, volume %.6f mm3\n",
              port::count_of(result, TopAbs_FACE), smallest, corner.faces, corner.worst, corner.worst_surface,
              port::volume_of(result));
  CHECK(smallest > 0.01);
  CHECK(corner.faces >= 1);
  // The corner's strip and triangle are B-splines fitted through nine
  // points of each of the ball's sections (1590's tcFit,
  // GeomAPI_PointsToBSplineSurface within max(tolapp3d, 1e-6), 1e-4 for
  // OCCT's fillet). Off the ball's surface: within five times that, for
  // the fit between its points. Off the ball by the centre along the
  // fitted normal: the normal's error times rk, which 1590's tube (rk 1)
  // keeps within 1e-3 (5e-4 measured): here rk 2, within 2e-3 (1.3e-3
  // measured: about the same angle of the normals). Before this patch the
  // acute corner's torus lay up to 0.03 off K's ball, by its geometry.
  CHECK(corner.worst_surface < 5e-4);
  CHECK(corner.worst < 1e-3 * kRk);
  // Less material than with the smaller rounding of the top.
  if (!first.IsNull()) {
    CHECK(port::volume_of(result) < port::volume_of(first));
  }
}

// The boundary: the top's edge rounded with the rim's own radius, the ball
// staying put round K's arc (no tight corner): clean.
void test_top_rounded_with_the_rims_radius_stays_clean() {
  const TopoDS_Shape result = rounded_twice(kRk);
  CHECK(!result.IsNull());
  if (result.IsNull()) {
    return;
  }
  CHECK(port::valid(result));
  CHECK(free_of_self_interference(result));
  const double smallest = smallest_face(result);
  std::printf("top with the rim's radius: %d faces, the smallest %.6f mm2, volume %.6f mm3\n",
              port::count_of(result, TopAbs_FACE), smallest, port::volume_of(result));
  CHECK(smallest > 0.01);
}

} // namespace

MITCAD_PORT_TEST("2291", test_top_rounded_past_a_cut_out_rim_whose_axis_lies_in_the_top);
MITCAD_PORT_TEST("2291", test_top_rounded_with_the_rims_radius_stays_clean);
