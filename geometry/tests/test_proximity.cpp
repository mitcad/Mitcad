// SPDX-License-Identifier: MIT
// Nearness told without measuring the whole distance (mitcad#1785,
// src/proximity.hpp): within_distance answers what BRepExtrema's distance
// compared with the limit answers, and a surface probe what a projection
// of each point on the surface answers.
//
// - Pairs of solids, faces, edges and vertices apart, touching at a
//   vertex, along an edge or over a face without a common vertex, a face
//   inside a solid (its vertices in it: no distance) and a B-spline face
//   above a box, each at limits below, at and above their distance; a
//   B-spline helix (its pieces' boxes, not its whole box, tell which pairs
//   are measured) with boxes in and through it, segments, another helix
//   and a B-spline face.
// - A Proximity made once answers for many shapes in turn, the solid's
//   classifier kept between them.
// - Points on and off a B-spline surface, a cylinder and a plane, within
//   and beyond the tolerance, also points outside the poles' box.
// - Pairs of edges told apart by the halves of their pieces (mitcad#1796):
//   circles, arcs, segments and an ellipse against a helix, apart by a
//   hair, crossing it and touching it, at limits about their distance.
// - PointSide: the state of points of a grid through a box, a tube, a
//   sphere and a cylinder cut by boxes along a helix (on their faces, edges and
//   vertices too) as a solid classifier gives it.
// - BoundaryRays (mitcad#1801): the crossings of short rays and rays
//   through a box, a tube, a sphere, a cone, a torus, a lofted thread (B-spline
//   faces; also moved and turned) and a rod with it, as the shape
//   intersector finds them, the nearest ones too, and most of the thread's
//   faces never asked; a copy of the body crossing the same with the
//   searchers of its faces found again by their bytes.
// - shape_bytes and ContentCache: a copy's face has the face's bytes, another,
//   moved or reversed face not; values taken, found and dropped.

#include <cmath>
#include <cstdio>
#include <string>
#include <vector>

#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRepPrimAPI_MakeCone.hxx>
#include <BRepPrimAPI_MakeTorus.hxx>
#include <Bnd_Box.hxx>
#include <IntCurvesFace_ShapeIntersector.hxx>
#include <gp_Ax1.hxx>
#include <gp_Lin.hxx>
#include <gp_Trsf.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <GeomAPI_PointsToBSpline.hxx>
#include <GeomAPI_PointsToBSplineSurface.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Geom_Plane.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <Precision.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <gp.hxx>
#include <gp_Ax2.hxx>
#include <gp_Ax3.hxx>
#include <gp_Circ.hxx>
#include <gp_Elips.hxx>
#include <gp_Pln.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "../src/content_cache.hpp"
#include "../src/history.hpp"
#include "../src/proximity.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

using mitcad::geometry::detail::BoundaryRays;
using mitcad::geometry::detail::ContentCache;
using mitcad::geometry::detail::InputCopy;
using mitcad::geometry::detail::shape_bytes;
using mitcad::geometry::detail::PointSide;
using mitcad::geometry::detail::Proximity;
using mitcad::geometry::detail::SurfaceProbe;
using mitcad::geometry::detail::within_distance;

constexpr double kPi = 3.14159265358979323846;

double occt_distance(const TopoDS_Shape& a, const TopoDS_Shape& b) {
  BRepExtrema_DistShapeShape d(a, b);
  return d.IsDone() && d.NbSolution() > 0 ? d.Value() : Precision::Infinite();
}

// within_distance agrees with the distance at limits around it, both ways
// round.
void agrees(const char* what, const TopoDS_Shape& a, const TopoDS_Shape& b) {
  const double d = occt_distance(a, b);
  std::vector<double> limits = {0.0, 1.0e-6, 1.0e-3, 0.1, 1.0, 10.0, 100.0};
  if (d > 1.0e-9) {
    for (const double f : {0.5, 0.999, 1.001, 2.0}) {
      limits.push_back(d * f);
    }
  }
  for (const double limit : limits) {
    const bool expected = d <= limit;
    if (within_distance(a, b, limit) != expected || within_distance(b, a, limit) != expected) {
      std::printf("  %s: distance %.9g, limit %.9g: within_distance differs\n", what, d, limit);
      CHECK(false);
    }
  }
}

TopoDS_Shape box_at(double x, double y, double z, double dx, double dy, double dz) {
  return BRepPrimAPI_MakeBox(gp_Pnt(x, y, z), dx, dy, dz).Shape();
}

TopoDS_Face first_face(const TopoDS_Shape& shape) {
  TopExp_Explorer ex(shape, TopAbs_FACE);
  return TopoDS::Face(ex.Current());
}

// A wavy B-spline surface over [0, 10] x [0, 10], at height `z`.
occ::handle<Geom_BSplineSurface> wavy(double z) {
  NCollection_Array2<gp_Pnt> points(1, 6, 1, 6);
  for (int i = 1; i <= 6; ++i) {
    for (int j = 1; j <= 6; ++j) {
      const double x = 2.0 * (i - 1);
      const double y = 2.0 * (j - 1);
      points.SetValue(i, j, gp_Pnt(x, y, z + 0.5 * std::sin(x) * std::cos(y)));
    }
  }
  return GeomAPI_PointsToBSplineSurface(points).Surface();
}

// A helix about the Z axis, `turns` turns of `pitch`, as a B-spline
// through points along it, raised by `z`.
TopoDS_Edge helix_edge(double radius, double pitch, int turns, double z = 0.0) {
  const int count = 24 * turns + 1;
  NCollection_Array1<gp_Pnt> points(1, count);
  for (int i = 0; i < count; ++i) {
    const double t = 2.0 * kPi * i / 24.0;
    points.SetValue(i + 1, gp_Pnt(radius * std::cos(t), radius * std::sin(t), z + pitch * t / (2.0 * kPi)));
  }
  return BRepBuilderAPI_MakeEdge(GeomAPI_PointsToBSpline(points).Curve()).Edge();
}

void test_shapes() {
  const TopoDS_Shape box = box_at(0, 0, 0, 10, 10, 10);
  agrees("boxes apart", box, box_at(12.5, 0, 0, 5, 5, 5));
  agrees("boxes at a vertex", box, box_at(10, 10, 10, 5, 5, 5));
  agrees("boxes along an edge", box, box_at(10, 10, 2, 5, 5, 5));
  agrees("boxes over a face", box, box_at(10, 2, 2, 5, 5, 5));
  agrees("boxes overlapping", box, box_at(5, 5, 5, 10, 10, 10));
  agrees("box and cylinder apart", box, BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(20, 5, 0), gp::DZ()), 3, 10).Shape());
  agrees("box and cylinder touching", box,
         BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(13, 5, 0), gp::DZ()), 3, 10).Shape());
  // A face inside the box: its vertices in the solid, no distance.
  const TopoDS_Face inside = BRepBuilderAPI_MakeFace(gp_Pln(gp_Pnt(0, 0, 5), gp::DZ()), 2, 8, 2, 8).Face();
  CHECK(occt_distance(box, inside) == 0.0);
  agrees("face inside a box", box, inside);
  agrees("face above a box", box, BRepBuilderAPI_MakeFace(gp_Pln(gp_Pnt(0, 0, 11), gp::DZ()), 2, 8, 2, 8).Face());
  agrees("face on a box", box, BRepBuilderAPI_MakeFace(gp_Pln(gp_Pnt(0, 0, 10), gp::DZ()), 2, 8, 2, 8).Face());
  agrees("edge beside a box", box, BRepBuilderAPI_MakeEdge(gp_Pnt(11, -5, 5), gp_Pnt(11, 15, 5)).Edge());
  agrees("vertex above a box", box, BRepBuilderAPI_MakeVertex(gp_Pnt(5, 5, 10.25)).Vertex());
  agrees("vertex in a box", box, BRepBuilderAPI_MakeVertex(gp_Pnt(5, 5, 5)).Vertex());
  // A wavy B-spline face above the box and through it.
  const TopoDS_Face above = BRepBuilderAPI_MakeFace(wavy(11.0), Precision::Confusion()).Face();
  const TopoDS_Face through = BRepBuilderAPI_MakeFace(wavy(10.2), Precision::Confusion()).Face();
  agrees("spline face above a box", box, above);
  agrees("spline face through a box's top", box, through);
  agrees("spline face and a box's face", first_face(box_at(0, 0, 10, 10, 10, 1)), above);
  // A helix of three turns (a B-spline, as a modelled thread's edges):
  // its box holds the whole cylinder, its pieces' boxes a part of a turn.
  const TopoDS_Edge helix = helix_edge(10.0, 3.0, 3);
  agrees("helix and a box in it", helix, box_at(-5, -5, 0, 10, 10, 9));
  agrees("helix and a box through it", helix, box_at(8, -1, 0, 4, 2, 9));
  agrees("helix and a segment along it", helix, BRepBuilderAPI_MakeEdge(gp_Pnt(10.5, 0, -1), gp_Pnt(10.5, 0, 10)).Edge());
  agrees("helix and a segment through it", helix, BRepBuilderAPI_MakeEdge(gp_Pnt(0, 0, 4.5), gp_Pnt(20, 0, 4.5)).Edge());
  agrees("helix and its own copy higher up", helix, helix_edge(10.0, 3.0, 3, 1.5));
  agrees("helix and a spline face", helix, BRepBuilderAPI_MakeFace(wavy(9.0), Precision::Confusion()).Face());
}

// One Proximity of the box against many shapes: the same answers as made
// anew for each (its classifier and the vertices it classified kept).
void test_reused() {
  const TopoDS_Shape box = box_at(0, 0, 0, 10, 10, 10);
  const Proximity kept(box);
  for (int k = 0; k < 12; ++k) {
    const double z = -2.0 + 1.25 * k;
    const TopoDS_Shape other = box_at(3, 3, z, 2, 2, 1);
    for (const double limit : {0.0, 0.5, 1.0}) {
      const bool expected = occt_distance(box, other) <= limit;
      CHECK(within_distance(kept, Proximity(other), limit) == expected);
      CHECK(within_distance(Proximity(other), kept, limit) == expected);
    }
  }
}

// As the cut-outs tested whether points lie on a surface.
bool projected_on(const std::vector<gp_Pnt>& points, const occ::handle<Geom_Surface>& surface, double tolerance) {
  if (points.empty() || surface.IsNull()) {
    return false;
  }
  for (const gp_Pnt& p : points) {
    GeomAPI_ProjectPointOnSurf projection(p, surface);
    if (projection.NbPoints() == 0 || projection.LowerDistance() > tolerance) {
      return false;
    }
  }
  return true;
}

void test_surfaces() {
  const occ::handle<Geom_BSplineSurface> spline = wavy(0.0);
  const occ::handle<Geom_Surface> cylinder = new Geom_CylindricalSurface(gp_Ax3(gp::XOY()), 5.0);
  const occ::handle<Geom_Surface> plane = new Geom_Plane(gp_Pln(gp_Pnt(0, 0, 1), gp::DZ()));
  for (const occ::handle<Geom_Surface>& surface : {occ::handle<Geom_Surface>(spline), cylinder, plane}) {
    SurfaceProbe probe(surface);
    std::vector<std::vector<gp_Pnt>> sets;
    std::vector<gp_Pnt> on;
    for (const double u : {0.1, 0.35, 0.6, 0.9}) {
      for (const double v : {0.2, 0.5, 0.8}) {
        double u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0;
        surface->Bounds(u0, u1, v0, v1);
        const double uu = Precision::IsInfinite(u1 - u0) ? 10.0 * u : u0 + u * (u1 - u0);
        const double vv = Precision::IsInfinite(v1 - v0) ? 10.0 * v : v0 + v * (v1 - v0);
        on.push_back(surface->Value(uu, vv));
      }
    }
    sets.push_back(on);
    for (const double off : {1.0e-7, 1.0e-4, 0.01, 3.0, 50.0}) {
      std::vector<gp_Pnt> moved = on;
      moved[moved.size() / 2].Translate(gp_Vec(off, off, off));
      sets.push_back(moved);
      sets.push_back({gp_Pnt(on.front().X(), on.front().Y(), on.front().Z() + off)});
    }
    sets.push_back({});
    for (const auto& points : sets) {
      for (const double tolerance : {1.0e-6, 1.0e-3, 0.1}) {
        CHECK(probe.holds(points, tolerance) == projected_on(points, surface, tolerance));
      }
    }
  }
}

// Edges whose pieces' boxes come near, told apart or measured: conics
// (taken as their rational B-spline forms) and segments against a helix.
void test_edge_halves() {
  const TopoDS_Edge helix = helix_edge(10.0, 3.0, 3);
  // Circles about the helix's axis at its radius, between its turns and
  // at a height where it passes: crossing it, a hair inside and outside.
  for (const double radius : {10.0, 10.0 - 1.0e-4, 10.0 + 1.0e-4, 10.002, 9.5, 12.0}) {
    for (const double z : {1.5, 4.0, 0.0}) {
      const gp_Circ circle(gp_Ax2(gp_Pnt(0, 0, z), gp::DZ()), radius);
      agrees("circle about the helix", helix, BRepBuilderAPI_MakeEdge(circle).Edge());
      agrees("arc about the helix", helix, BRepBuilderAPI_MakeEdge(circle, 0.3, 2.1).Edge());
    }
  }
  // A circle across the helix (in a plane through its axis), touching it
  // from outside and apart by a hair.
  for (const double gap : {0.0, 1.0e-5, 2.0e-4, 0.01}) {
    const gp_Circ across(gp_Ax2(gp_Pnt(12.0 + gap, 0, 4.5), gp::DY()), 2.0);
    agrees("circle across the helix", helix, BRepBuilderAPI_MakeEdge(across).Edge());
  }
  // An ellipse and a segment beside a turn.
  const gp_Elips ellipse(gp_Ax2(gp_Pnt(0, 0, 2.25), gp::DZ()), 10.5, 9.8);
  agrees("ellipse about the helix", helix, BRepBuilderAPI_MakeEdge(ellipse).Edge());
  agrees("segment a hair off the helix",
         helix, BRepBuilderAPI_MakeEdge(gp_Pnt(10.0001, -1, 0), gp_Pnt(10.0001, 1, 0)).Edge());
  // Two arcs of one circle: they meet at a vertex.
  const gp_Circ circle(gp_Ax2(gp_Pnt(0, 0, 0), gp::DZ()), 5.0);
  agrees("arcs of one circle", BRepBuilderAPI_MakeEdge(circle, 0.0, 1.0).Edge(),
         BRepBuilderAPI_MakeEdge(circle, 1.0, 2.5).Edge());
  agrees("arcs of one circle apart", BRepBuilderAPI_MakeEdge(circle, 0.0, 1.0).Edge(),
         BRepBuilderAPI_MakeEdge(circle, 1.001, 2.5).Edge());
}

// PointSide against a solid classifier: points of a grid through the
// shape, some on its faces, edges and vertices.
void sides_agree(const char* what, const TopoDS_Shape& solid, double from, double to, double step) {
  PointSide sides(solid);
  BRepClass3d_SolidClassifier classifier(solid);
  int differ = 0;
  int points = 0;
  for (double x = from; x <= to + 1.0e-9; x += step) {
    for (double y = from; y <= to + 1.0e-9; y += step) {
      for (double z = from; z <= to + 1.0e-9; z += step) {
        const gp_Pnt p(x, y, z);
        for (const double tolerance : {1.0e-7, 1.0e-3}) {
          classifier.Perform(p, tolerance);
          ++points;
          if (sides.state(p, tolerance) != classifier.State()) {
            ++differ;
            if (differ <= 5) {
              std::printf("  %s: (%g, %g, %g) tolerance %g: PointSide %d, classifier %d\n", what, x, y, z, tolerance,
                          static_cast<int>(sides.state(p, tolerance)), static_cast<int>(classifier.State()));
            }
          }
        }
      }
    }
  }
  CHECK(points > 0);
  CHECK(differ == 0);
}

void test_point_side() {
  sides_agree("box", box_at(0, 0, 0, 10, 10, 10), -1.0, 11.0, 1.0);
  const TopoDS_Shape outer = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(5, 5, 0), gp::DZ()), 5.0, 10.0).Shape();
  const TopoDS_Shape inner = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(5, 5, -1), gp::DZ()), 2.5, 12.0).Shape();
  const TopoDS_Shape tube = BRepAlgoAPI_Cut(outer, inner).Shape();
  sides_agree("tube", tube, -1.0, 11.0, 0.5);
  sides_agree("sphere", BRepPrimAPI_MakeSphere(gp_Pnt(5, 5, 5), 4.0).Shape(), -1.0, 11.0, 1.0);
  // A cylinder with a groove along a helix (a box swept round it would do
  // as well; a ring of boxes at the helix's points keeps it simple).
  TopoDS_Shape grooved = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp::DZ()), 6.0, 10.0).Shape();
  for (int k = 0; k < 12; ++k) {
    const double t = 2.0 * kPi * k / 12.0;
    const gp_Pnt at(6.0 * std::cos(t), 6.0 * std::sin(t), 2.0 + 0.5 * k);
    grooved = BRepAlgoAPI_Cut(grooved, box_at(at.X() - 0.5, at.Y() - 0.5, at.Z() - 0.25, 1.0, 1.0, 0.5)).Shape();
  }
  sides_agree("grooved cylinder", grooved, -7.0, 11.0, 0.75);
}

// A rod with a thread lofted round it (mitcad#1801): a triangle in the
// plane through the axis, eight a turn along a helix of `turns` turns, as
// one solid of B-spline faces and two plane ends.
TopoDS_Shape lofted_thread(double inner, double outer, double pitch, int turns) {
  BRepOffsetAPI_ThruSections loft(true, false);
  const int sections = 8 * turns + 1;
  for (int k = 0; k < sections; ++k) {
    const double a = 2.0 * kPi * k / 8.0;
    const double z = pitch * a / (2.0 * kPi);
    const gp_Dir out(std::cos(a), std::sin(a), 0.0);
    const auto at = [&](double r, double h) { return gp_Pnt(r * out.X(), r * out.Y(), z + h); };
    BRepBuilderAPI_MakePolygon triangle(at(inner, -0.4 * pitch), at(outer, 0.0), at(inner, 0.4 * pitch), true);
    loft.AddWire(triangle.Wire());
  }
  loft.Build();
  return loft.Shape();
}

// A small generator of numbers in [0, 1) (the same on every platform).
struct Numbers {
  unsigned long long state = 12345;
  double next() {
    state = state * 6364136223846793005ULL + 1442695040888963407ULL;
    return static_cast<double>(state >> 11) / 9007199254740992.0;
  }
};

// BoundaryRays against IntCurvesFace_ShapeIntersector on rays through and
// about a shape: Perform the same crossings (W, U, V, state, face) in the
// same order; PerformNearest the same nearest crossing and every crossing
// within the confusion tolerance of it. Returns the share of the faces the short rays asked.
double rays_agree(const char* what, const TopoDS_Shape& shape, int count) {
  IntCurvesFace_ShapeIntersector all;
  all.Load(shape, Precision::Confusion());
  BoundaryRays rays(shape, Precision::Confusion());
  BoundaryRays again(shape, Precision::Confusion()); // (its faces' searchers kept)
  Bnd_Box box;
  BRepBndLib::Add(shape, box);
  box.Enlarge(1.0);
  double x0 = 0.0, y0 = 0.0, z0 = 0.0, x1 = 0.0, y1 = 0.0, z1 = 0.0;
  box.Get(x0, y0, z0, x1, y1, z1);
  int faces = 0;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    ++faces;
  }
  Numbers numbers;
  int differ = 0;
  int crossings = 0;
  long asked = 0;
  for (int n = 0; n < count; ++n) {
    const gp_Pnt from(x0 + (x1 - x0) * numbers.next(), y0 + (y1 - y0) * numbers.next(),
                      z0 + (z1 - z0) * numbers.next());
    const gp_Vec direction(numbers.next() - 0.5, numbers.next() - 0.5, numbers.next() - 0.5);
    if (direction.Magnitude() < 1.0e-3) {
      continue;
    }
    const gp_Lin line(from, gp_Dir(direction));
    // (Short rays as the dressups' checks send, and rays through it all.)
    const double reach = n % 2 == 0 ? 0.5 + 4.0 * numbers.next() : 2.0 * std::sqrt(box.SquareExtent());
    const double close = n % 3 == 0 ? 0.0 : 1.0e-3;
    all.Perform(line, -close, reach);
    BoundaryRays& mine = n % 4 == 0 ? again : rays;
    mine.Perform(line, -close, reach);
    asked += n % 2 == 0 ? mine.asked() : 0;
    bool same = all.IsDone() == mine.IsDone() && (!all.IsDone() || all.NbPnt() == mine.NbPnt());
    for (int i = 1; same && all.IsDone() && i <= all.NbPnt(); ++i) {
      same = all.WParameter(i) == mine.WParameter(i) && all.UParameter(i) == mine.UParameter(i) &&
             all.VParameter(i) == mine.VParameter(i) && all.State(i) == mine.State(i) &&
             all.Face(i).IsEqual(mine.Face(i)) && all.Pnt(i).IsEqual(mine.Pnt(i), 0.0);
    }
    crossings += all.IsDone() ? all.NbPnt() : 0;
    mine.PerformNearest(line, -close, reach);
    if (same && all.IsDone()) {
      if (all.NbPnt() == 0) {
        same = mine.NbPnt() == 0;
      } else {
        int near_all = 0;
        int near_mine = 0;
        for (int i = 1; i <= all.NbPnt(); ++i) {
          near_all += all.WParameter(i) - all.WParameter(1) <= Precision::Confusion() ? 1 : 0;
        }
        for (int i = 1; i <= mine.NbPnt(); ++i) {
          near_mine += mine.WParameter(i) - mine.WParameter(1) <= Precision::Confusion() ? 1 : 0;
        }
        same = mine.NbPnt() > 0 && mine.WParameter(1) == all.WParameter(1) && mine.State(1) == all.State(1) &&
               mine.Face(1).IsEqual(all.Face(1)) && mine.UParameter(1) == all.UParameter(1) &&
               mine.VParameter(1) == all.VParameter(1) && near_all == near_mine;
      }
    }
    if (!same) {
      ++differ;
      if (differ <= 5) {
        std::printf("  %s: ray %d from (%g, %g, %g): %d crossings, BoundaryRays %d\n", what, n, from.X(), from.Y(),
                    from.Z(), all.IsDone() ? all.NbPnt() : -1, mine.IsDone() ? mine.NbPnt() : -1);
      }
    }
  }
  CHECK(crossings > 0);
  CHECK(differ == 0);
  return static_cast<double>(asked) / (0.5 * count * faces);
}

void test_boundary_rays() {
  rays_agree("box", box_at(0, 0, 0, 10, 10, 10), 400);
  const TopoDS_Shape outer = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(5, 5, 0), gp::DZ()), 5.0, 10.0).Shape();
  const TopoDS_Shape inner = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(5, 5, -1), gp::DZ()), 2.5, 12.0).Shape();
  rays_agree("tube", BRepAlgoAPI_Cut(outer, inner).Shape(), 400);
  rays_agree("sphere", BRepPrimAPI_MakeSphere(gp_Pnt(5, 5, 5), 4.0).Shape(), 400);
  rays_agree("cone", BRepPrimAPI_MakeCone(gp_Ax2(gp_Pnt(0, 0, 0), gp::DZ()), 5.0, 1.0, 8.0).Shape(), 400);
  rays_agree("torus", BRepPrimAPI_MakeTorus(gp_Ax2(gp_Pnt(0, 0, 0), gp::DX()), 6.0, 1.5).Shape(), 400);
  const TopoDS_Shape thread = lofted_thread(5.0, 6.5, 2.0, 3);
  CHECK(!thread.IsNull());
  // (The short rays ask few of the faces, which wrap round all the turns:
  // their patches' boxes, halved, tell.)
  const double asked = rays_agree("lofted thread", thread, 600);
  if (!(asked < 0.25)) {
    std::printf("  lofted thread: the short rays asked %.2f of the faces\n", asked);
    CHECK(false);
  }
  // The same, moved and turned (the faces' place in their location).
  gp_Trsf move;
  move.SetRotation(gp_Ax1(gp_Pnt(1, 2, 3), gp_Dir(1, 1, 0)), 0.7);
  rays_agree("lofted thread moved", BRepBuilderAPI_Transform(thread, move, false).Shape(), 300);
  // A rod with the thread on it, a B-spline face beside planes and a
  // cylinder, as a modelled thread's body.
  const TopoDS_Shape rod = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -1), gp::DZ()), 5.2, 8.0).Shape();
  const TopoDS_Shape threaded = BRepAlgoAPI_Fuse(rod, thread).Shape();
  rays_agree("threaded rod", threaded, 600);
  // A copy of it (new shapes, the same geometry), as the next dressup's
  // input: its faces' searchers, kept from the rays above, found again by
  // the faces' bytes, cross as their own would.
  rays_agree("threaded rod copied", InputCopy(threaded).shape(), 600);
}

// The bytes of shapes and the values kept by them (mitcad#1801).
void test_content_cache() {
  const TopoDS_Shape thread = lofted_thread(5.0, 6.5, 2.0, 2);
  const TopoDS_Face face = first_face(thread);
  // A copy's face has the bytes of the face; another face, a moved one or a
  // turned-over one not.
  const std::string bytes = shape_bytes(face);
  CHECK(!bytes.empty());
  CHECK(shape_bytes(first_face(InputCopy(thread).shape())) == bytes);
  TopExp_Explorer faces(thread, TopAbs_FACE);
  faces.Next();
  CHECK(shape_bytes(faces.Current()) != bytes);
  gp_Trsf move;
  move.SetTranslation(gp_Vec(0.0, 0.0, 1.0e-9));
  CHECK(shape_bytes(face.Moved(TopLoc_Location(move))) != bytes);
  CHECK(shape_bytes(face.Reversed()) != bytes);
  CHECK(shape_bytes(TopoDS_Shape()).empty());

  // Taken out while used, found as a copy; the least recently used dropped
  // beyond the entries or the bytes allowed.
  ContentCache<int> kept(3, 100);
  int value = 0;
  CHECK(!kept.find("a", value));
  kept.keep("a", 1);
  kept.keep("b", 2);
  kept.keep("c", 3);
  CHECK(kept.find("a", value) && value == 1);
  kept.keep("d", 4); // ("b" the least recently used)
  CHECK(!kept.find("b", value));
  CHECK(kept.take("c", value) && value == 3);
  CHECK(!kept.find("c", value));
  kept.keep("c", 5);
  CHECK(kept.find("c", value) && value == 5);
  kept.keep("c", 6); // (kept already: not)
  CHECK(kept.find("c", value) && value == 5);
  kept.keep(std::string(60, 'x'), 7);
  kept.keep(std::string(50, 'y'), 8); // (110 bytes of keys: the oldest dropped)
  CHECK(kept.find(std::string(50, 'y'), value) && value == 8);
  CHECK(!kept.find(std::string(60, 'x'), value));
  kept.keep(std::string(101, 'z'), 9); // (longer than allowed: not)
  CHECK(!kept.find(std::string(101, 'z'), value));
  kept.keep("", 10);
  CHECK(!kept.find("", value));
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST(proximity) {
  test::guarded("shapes", test::test_shapes);
  test::guarded("reused", test::test_reused);
  test::guarded("surfaces", test::test_surfaces);
  test::guarded("edge halves", test::test_edge_halves);
  test::guarded("point side", test::test_point_side);
  test::guarded("boundary rays", test::test_boundary_rays);
  test::guarded("content cache", test::test_content_cache);
  if (test::failures == 0) {
    std::puts("test_proximity: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
