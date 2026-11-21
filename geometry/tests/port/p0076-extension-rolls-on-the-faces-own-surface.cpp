// SPDX-License-Identifier: MIT
// Port patch 0076: past the end of a spine, where a contact leaves its face
// and no face follows, the walked blend rolls on the face's own surface,
// unbounded, when that surface is a cylinder, cone, sphere or torus
// (ChFi3d_BuildPlane). Before, it rolled on the plane tangent to the face
// where the contact crossed the face's edge: the extension's balls touched
// that plane, not the face's surface, and where the face at the end cuts
// the blend within that extension, the cut ran along the wrong balls.
//
// The body: a plate (top z = 0) and the cap x + y + k z >= 5 of the sphere
// of radius 5 about the origin standing on it, its foot (the arc of the
// equator from (5, 0, 0) to (0, 5, 0)) rounded with 2. The foot is no line
// or circle on a plane and a cylinder or cone, so the rounding is walked
// (ChFi3d_KParticular). The true rounding is the torus round the z axis
// whose ball (radius 2) has its centre on the circle of radius
// sqrt(7^2 - 2^2) = 6.708 at height 2: every point of it lies 2 from that
// circle. The cap's flat face, the plane x + y + k z = 5, is the face at
// both ends of the spine (mirror images in the plane x = y); the sections
// are the planes through the z axis, the section at angle t from the end at
// (5, 0, 0) (negative past it) has its sphere contact at height 1.43, 4.79
// from the axis, and its plate contact 6.708 from the axis.
//
// With k = 0.5 (the flat face leaning over the plate) the sphere contact
// leaves the sphere across the cap's edge at t = -5.77 degrees, past the
// spine's end, while the plate contact reaches the flat face only at
// t = -13.19 degrees: there the walk goes on past the end (the spine is
// extended to be cut by the face at the end), StartSol finds no face
// tangent to the sphere across that edge, which ends at the spine's end
// vertex (no obstacle), and ChFi3d_BuildPlane takes the rest. Every point of
// the blend on the flat face (the cut, from the plate contact at -13.19 up
// to the sphere's edge at -5.77) belongs to a section of that extension.
// With the patch the ball rolls on the sphere itself: the cut lies 2 from
// the centre's circle. Without it the ball rolls on the plane tangent to
// the sphere at -5.77: its centre runs along the tangent of the centre's
// circle and is off it by up to 0.057 at -13.19 (0.014 halfway), so the
// cut's points miss 2 by about 0.01. The check: the result is valid with
// its tolerances raised to the tests' baseline of 1e-6 (with the patch the
// cap's edge on the trimmed sphere keeps the tolerance 1e-7 its curve on
// that face is off by 1.17e-7) and the blend's faces, its cuts on the
// flat face above all, lie within 2e-4 of 2 from the centre's circle.
//
// The boundary: k = 1.5, the flat face leaning further. The plate contact
// reaches it first (t = -13.19), the sphere contact only at t = -20.06: the
// cut is made of sections whose sphere contact is still on the sphere
// face, walked before any extension rolls past its edge; the same check
// holds with and without the patch.

#include <algorithm>
#include <cmath>
#include <cstdio>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <BRepTools.hxx>
#include <ShapeFix_ShapeTolerance.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>

#include "port_test.hpp"

namespace {

constexpr double kSphere = 5.0; // the sphere's radius
constexpr double kRound  = 2.0; // the rounding's

// The radius of the circle the ball's centre runs on (at height kRound).
double centre_circle() {
  return std::sqrt((kSphere + kRound) * (kSphere + kRound) - kRound * kRound);
}

// |the distance of p from the ball centre's circle - the ball's radius|.
double miss(const gp_Pnt& p) {
  const double radial = std::hypot(p.X(), p.Y()) - centre_circle();
  return std::abs(std::hypot(radial, p.Z() - kRound) - kRound);
}

// The cap's flat face: the plane x + y + lean z = kSphere.
struct Cut {
  gp_Dir normal;
  gp_Pnt point;
  bool   holds(const gp_Pnt& p) const { return std::abs(gp_Vec(point, p).Dot(gp_Vec(normal))) < 1e-5; }
};

Cut cut(double lean) { return {gp_Dir(1.0, 1.0, lean), gp_Pnt(0.5 * kSphere, 0.5 * kSphere, 0.0)}; }

// The plate with the cap x + y + lean z >= kSphere of the sphere standing on
// it; the sphere's seam turned away from the cap (towards (-1, -1, 0)).
TopoDS_Shape capped_plate(double lean) {
  const TopoDS_Shape plate  = BRepPrimAPI_MakeBox(gp_Pnt(-20, -20, -8), gp_Pnt(20, 20, 0)).Shape();
  const TopoDS_Shape sphere = BRepPrimAPI_MakeSphere(gp_Ax2(gp::Origin(), gp::DZ(), gp_Dir(-1, -1, 0)), kSphere).Shape();
  const Cut          c      = cut(lean);
  const gp_Ax2       frame(c.point, c.normal, gp_Dir(1, -1, 0));
  const gp_Pnt       corner = c.point.Translated(gp_Vec(frame.XDirection()) * -30.0 + gp_Vec(frame.YDirection()) * -30.0);
  const TopoDS_Shape side   = BRepPrimAPI_MakeBox(gp_Ax2(corner, c.normal, frame.XDirection()), 60, 60, 30).Shape();
  const TopoDS_Shape cap    = BRepAlgoAPI_Common(sphere, side).Shape();
  ShapeUpgrade_UnifySameDomain unify(BRepAlgoAPI_Fuse(plate, cap).Shape());
  unify.Build();
  return unify.Shape();
}

// Whether the shape is valid with its tolerances raised to 1e-6 where they
// are smaller (on a copy): deviations up to the tests' baseline of 1e-6
// are no fault.
bool valid_within_baseline(const TopoDS_Shape& shape) {
  if (shape.IsNull()) {
    return false;
  }
  const TopoDS_Shape copy = BRepBuilderAPI_Copy(shape).Shape();
  ShapeFix_ShapeTolerance().LimitTolerance(copy, 1e-6);
  return port::valid(copy);
}

// The faces of the rounding: neither planes nor spheres.
bool is_blend(const TopoDS_Face& face) {
  const GeomAbs_SurfaceType type = BRepAdaptor_Surface(face).GetType();
  return type != GeomAbs_Plane && type != GeomAbs_Sphere;
}

// The largest miss over the blend's faces (sampled inside them) and,
// separately, over their edges on the flat face `c` (`cuts` counts them).
void largest_misses(const TopoDS_Shape& rounded, const Cut& c, double& on_faces, double& on_cuts, int& cuts) {
  on_faces = 0.0;
  on_cuts  = 0.0;
  cuts     = 0;
  for (TopExp_Explorer f(rounded, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face& face = TopoDS::Face(f.Current());
    if (!is_blend(face)) {
      continue;
    }
    const BRepAdaptor_Surface s(face);
    double                    u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    for (int i = 0; i <= 80; ++i) {
      for (int j = 0; j <= 10; ++j) {
        const gp_Pnt2d uv(u0 + (u1 - u0) * i / 80.0, v0 + (v1 - v0) * j / 10.0);
        if (BRepClass_FaceClassifier(face, uv, 1e-7).State() != TopAbs_OUT) {
          on_faces = std::max(on_faces, miss(s.Value(uv.X(), uv.Y())));
        }
      }
    }
    for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
      const BRepAdaptor_Curve curve(TopoDS::Edge(e.Current()));
      const double            a = curve.FirstParameter(), b = curve.LastParameter();
      if (!c.holds(curve.Value(0.5 * (a + b)))) {
        continue;
      }
      ++cuts;
      for (int k = 0; k <= 40; ++k) {
        on_cuts = std::max(on_cuts, miss(curve.Value(a + (b - a) * k / 40.0)));
      }
    }
  }
}

// The cap's foot rounded with 2; the blend checked against the ball's
// centre circle, `what` naming the case.
void check_rounded_foot(double lean, const char* what) {
  const TopoDS_Shape             body = capped_plate(lean);
  const double                   foot = kSphere / std::sqrt(2.0);
  const std::vector<TopoDS_Edge> feet = port::edges_where(body, [foot](const gp_Pnt& m) {
    return m.Distance(gp_Pnt(foot, foot, 0.0)) < 1e-4;
  });
  CHECK(feet.size() == 1);
  if (feet.size() != 1) {
    return;
  }
  const TopoDS_Shape rounded = port::rounded(body, feet, kRound);
  CHECK(valid_within_baseline(rounded));
  if (!valid_within_baseline(rounded)) {
    return;
  }
  double on_faces = 0.0, on_cuts = 0.0;
  int    cuts     = 0;
  largest_misses(rounded, cut(lean), on_faces, on_cuts, cuts);
  std::printf("%s: %d cuts on the flat face, missing the rolling ball by %.3g (faces %.3g)\n", what, cuts, on_cuts,
              on_faces);
  CHECK(cuts >= 2);
  CHECK(on_cuts < 2e-4);
  CHECK(on_faces < 2e-4);
}

// The flat face leaning by 0.5: the cuts at both ends are made of the
// extension's sections, whose ball rolls past the sphere's edge.
void test_cut_of_the_extension_past_the_sphere_is_a_rolling_ball() {
  check_rounded_foot(0.5, "cut past the sphere's edge");
}

// The boundary: leaning by 1.5, the cuts are made of sections whose sphere
// contact is still on the sphere face.
void test_cut_before_the_sphere_edge_is_a_rolling_ball() { check_rounded_foot(1.5, "cut before the sphere's edge"); }

} // namespace

MITCAD_PORT_TEST("0076", test_cut_of_the_extension_past_the_sphere_is_a_rolling_ball);
MITCAD_PORT_TEST("0076", test_cut_before_the_sphere_edge_is_a_rolling_ball);
