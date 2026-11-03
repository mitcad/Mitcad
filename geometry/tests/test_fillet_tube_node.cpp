// SPDX-License-Identifier: MIT
// Roundings of the valleys where tubes meet, run on through the nodes where
// three of them meet a facet narrower than the radius (mitcad#1205).
//
// A frame of tubes of radius 19.5 (bores of 17.5) along four edges of
// tetrahedra from one point O, cut by a side plane: at each of two nodes
// three tubes meet, and a rod of radius 2.5 along the node's axis, capped
// square to it just past where the three tubes' surfaces meet, leaves a
// facet of about 5 mm² between them. The valleys where the tubes meet run
// from the facets' vertices to the side plane (one from node to node).
// Rounded with 3, wider than the facet: OCCT's corners at the facet's
// vertices fail. MitCad builds each valley's rounding, run on through the
// node, and joins them to the body: where they overlap at the node each
// keeps what lies outside the others, so the three meet at a point of the
// node's axis and the facet is buried (node_dressup.hpp). The stored
// design behind it has two such nodes.

#include <cmath>
#include <cstdio>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRep_Tool.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/import.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

constexpr double kTube = 19.5;  // the tubes' radius
constexpr double kBore = 17.5;  // their bores'
constexpr double kRod = 2.5;    // the rods' along the nodes' axes
constexpr double kCap = 35.17;  // the rods' caps from O along the axes
constexpr double kSide = 4.0;   // the side plane from O along its normal
constexpr double kR = 3.0;      // the rounding's radius
constexpr double kPi = 3.14159265358979323846;

const gp_Dir kC(-1.0, 0.0, 0.0);
const gp_Dir kB(-0.5, std::sqrt(3.0) / 2.0, 0.0);
const gp_Dir kA(-0.5, std::sqrt(3.0) / 6.0, -std::sqrt(2.0 / 3.0));
const gp_Dir kA2(-0.5, std::sqrt(3.0) / 6.0, std::sqrt(2.0 / 3.0)); // kA mirrored in z = 0
const gp_Dir kSideNormal(-std::sqrt(3.0) / 2.0, 0.5, 0.0);           // between kB and kC

// A node's axis: halfway between its three tubes.
gp_Dir axis_of(const gp_Dir& a, const gp_Dir& b, const gp_Dir& c) {
  return gp_Dir(gp_Vec(a) + gp_Vec(b) + gp_Vec(c));
}

// A cylinder along `d` from O, its seam on the side away from the other
// tubes (off the valleys).
TopoDS_Shape cylinder(const gp_Dir& d, double radius, double from, double to) {
  gp_Vec away(0.0, 0.0, 0.0);
  for (const gp_Dir& other : {kA, kA2, kB, kC}) {
    if (!other.IsEqual(d, 1e-9)) {
      away -= gp_Vec(other);
    }
  }
  away -= gp_Vec(d) * away.Dot(gp_Vec(d));
  return BRepPrimAPI_MakeCylinder(gp_Ax2(gp::Origin().Translated(gp_Vec(d) * from), d, gp_Dir(away)), radius,
                                  to - from)
      .Shape();
}

TopoDS_Shape body() {
  TopoDS_Shape outer = cylinder(kC, kTube, -25.0, 70.0);
  TopoDS_Shape bores = cylinder(kC, kBore, -26.0, 71.0);
  for (const gp_Dir& d : {kB, kA, kA2}) {
    outer = BRepAlgoAPI_Fuse(outer, cylinder(d, kTube, -25.0, 70.0)).Shape();
    bores = BRepAlgoAPI_Fuse(bores, cylinder(d, kBore, -26.0, 71.0)).Shape();
  }
  TopoDS_Shape frame = BRepAlgoAPI_Cut(outer, bores).Shape();
  frame = BRepAlgoAPI_Fuse(frame, cylinder(axis_of(kA, kB, kC), kRod, 0.0, kCap)).Shape();
  frame = BRepAlgoAPI_Fuse(frame, cylinder(axis_of(kA2, kB, kC), kRod, 0.0, kCap)).Shape();
  // The side: the half towards the tubes beyond a plane across them.
  const gp_Ax2 side(gp::Origin().Translated(gp_Vec(kSideNormal) * kSide), kSideNormal);
  const gp_Pnt corner = side.Location().Translated(gp_Vec(side.XDirection()) * -200.0 +
                                                   gp_Vec(side.YDirection()) * -200.0);
  const TopoDS_Shape half =
      BRepPrimAPI_MakeBox(gp_Ax2(corner, kSideNormal, side.XDirection()), 400.0, 400.0, 200.0).Shape();
  return BRepAlgoAPI_Common(frame, half).Shape();
}

// The centre of the ball rolling in the valley of the tubes along `a` and
// `b`: the ellipse in the plane through their bisector and their normal at
// R + r from both axes.
struct Valley {
  gp_Dir u, n;
  double along_u, along_n;
  gp_Pnt at(double t) const {
    return gp::Origin().Translated(gp_Vec(u) * (along_u * std::cos(t)) + gp_Vec(n) * (along_n * std::sin(t)));
  }
  // The distance of a point from the ellipse (by sampling, then refining).
  double distance(const gp_Pnt& p) const {
    double best = 1e300;
    double at_best = 0.0;
    const int n_samples = 3600;
    for (int i = 0; i < n_samples; ++i) {
      const double t = 2.0 * kPi * i / n_samples;
      const double d = p.Distance(at(t));
      if (d < best) {
        best = d;
        at_best = t;
      }
    }
    double lo = at_best - 2.0 * kPi / n_samples;
    double hi = at_best + 2.0 * kPi / n_samples;
    for (int k = 0; k < 100; ++k) {
      const double m1 = lo + (hi - lo) / 3.0;
      const double m2 = hi - (hi - lo) / 3.0;
      if (p.Distance(at(m1)) < p.Distance(at(m2))) {
        hi = m2;
      } else {
        lo = m1;
      }
    }
    return p.Distance(at(0.5 * (lo + hi)));
  }
};

Valley valley_of(const gp_Dir& a, const gp_Dir& b) {
  Valley v;
  v.u = gp_Dir(gp_Vec(a) + gp_Vec(b));
  v.n = gp_Dir(gp_Vec(a).Crossed(gp_Vec(b)));
  const double half = std::acos(v.u.Dot(a)); // half the angle between them
  v.along_u = (kTube + kR) / std::sin(half);
  v.along_n = kTube + kR;
  return v;
}

// Where the roundings of a node's valleys meet its axis `s`: the point of
// the axis a radius from the ball's centre in the valley of `a` and `b`.
gp_Pnt meeting(const gp_Dir& s, const gp_Dir& a, const gp_Dir& b) {
  const Valley v = valley_of(a, b);
  double lo = kCap - 2.0;
  double hi = kCap + 2.0;
  for (int k = 0; k < 80; ++k) {
    const double m = 0.5 * (lo + hi);
    if (v.distance(gp::Origin().Translated(gp_Vec(s) * m)) > kR) {
      lo = m;
    } else {
      hi = m;
    }
  }
  return gp::Origin().Translated(gp_Vec(s) * (0.5 * (lo + hi)));
}

bool is_tube(const TopoDS_Face& face) {
  const BRepAdaptor_Surface s(face, false);
  return s.GetType() == GeomAbs_Cylinder && std::abs(s.Cylinder().Radius() - kTube) < 1e-9;
}

void test_valleys_rounded_through_the_nodes() {
  const TopoDS_Shape input = body();
  CHECK(BRepCheck_Analyzer(input).IsValid());
  const ShapePtr named = import_body(input, "F1", 1);
  // The valleys: edges between two tubes with an end on a facet.
  const std::vector<gp_Dir> axes = {axis_of(kA, kB, kC), axis_of(kA2, kB, kC)};
  const auto on_a_facet = [&](const gp_Pnt& p) {
    for (const gp_Dir& s : axes) {
      if (std::abs(gp_Vec(gp::Origin(), p).Dot(gp_Vec(s)) - kCap) < 1e-6) {
        return true;
      }
    }
    return false;
  };
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces_of;
  TopExp::MapShapesAndAncestors(named->occt(), TopAbs_EDGE, TopAbs_FACE, faces_of);
  std::vector<std::string> names;
  for (int i = 0; i < named->edge_count(); ++i) {
    const TopoDS_Edge& edge = named->edge(i);
    const NCollection_List<TopoDS_Shape>& faces = faces_of.FindFromKey(edge);
    bool tubes = faces.Extent() == 2 && !faces.First().IsSame(faces.Last());
    for (const TopoDS_Shape& f : faces) {
      tubes = tubes && is_tube(TopoDS::Face(f));
    }
    TopoDS_Vertex v0;
    TopoDS_Vertex v1;
    TopExp::Vertices(edge, v0, v1);
    if (tubes && (on_a_facet(BRep_Tool::Pnt(v0)) || on_a_facet(BRep_Tool::Pnt(v1)))) {
      names.push_back(named->edge_name(i));
    }
  }
  // (The booleans split the valley from node to node at its middle, where
  // it crosses the plane of its tubes' axes: two edges, one rounding.)
  CHECK(names.size() == 6);
  if (names.size() != 6) {
    return;
  }
  const ShapePtr rounded = fillet("F2", *named, names, kR);
  CHECK(BRepCheck_Analyzer(rounded->occt()).IsValid());
  // Each valley's rounding: one face, on the pipe of radius r about its
  // ball's centre (OCCT's rounding of the two tubes, approximated).
  const std::vector<Valley> valleys = {valley_of(kA, kB), valley_of(kA, kC), valley_of(kB, kC),
                                       valley_of(kA2, kB), valley_of(kA2, kC)};
  double worst = 0.0;
  for (const std::string& edge : names) {
    int count = 0;
    for (int f = 0; f < rounded->face_count(); ++f) {
      bool mine = false;
      for (const std::string& name : rounded->face_names(f)) {
        mine = mine || name == "F2:fillet(" + edge + ")";
      }
      if (!mine) {
        continue;
      }
      ++count;
      const BRepAdaptor_Surface s(rounded->face(f));
      double best_valley = 1e300;
      for (const Valley& v : valleys) {
        double off = 0.0;
        for (int i = 1; i < 6; ++i) {
          for (int j = 1; j < 6; ++j) {
            const gp_Pnt p = s.Value(s.FirstUParameter() + (s.LastUParameter() - s.FirstUParameter()) * i / 6.0,
                                     s.FirstVParameter() + (s.LastVParameter() - s.FirstVParameter()) * j / 6.0);
            off = std::max(off, std::abs(v.distance(p) - kR));
          }
        }
        best_valley = std::min(best_valley, off);
      }
      worst = std::max(worst, best_valley);
    }
    CHECK(count == 1);
  }
  std::printf("roundings off their pipes by at most %.3g mm\n", worst);
  CHECK(worst < 1e-5);
  // The facets are gone: no plane square to a node's axis is left.
  for (int f = 0; f < rounded->face_count(); ++f) {
    const BRepAdaptor_Surface s(rounded->face(f), false);
    if (s.GetType() != GeomAbs_Plane) {
      continue;
    }
    for (const gp_Dir& axis : axes) {
      CHECK(!s.Plane().Axis().Direction().IsParallel(axis, 1e-6));
    }
  }
  // At each node the three roundings meet at a point of its axis.
  for (const auto& [s, a] : {std::pair<gp_Dir, gp_Dir>{axes[0], kA}, std::pair<gp_Dir, gp_Dir>{axes[1], kA2}}) {
    const gp_Pnt expected = meeting(s, a, kB);
    double nearest = 1e300;
    for (TopExp_Explorer v(rounded->occt(), TopAbs_VERTEX); v.More(); v.Next()) {
      nearest = std::min(nearest, BRep_Tool::Pnt(TopoDS::Vertex(v.Current())).Distance(expected));
    }
    std::printf("node vertex %.3g mm from the meeting point on the axis\n", nearest);
    CHECK(nearest < 1e-5);
  }
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_valleys_rounded_through_the_nodes", test::test_valleys_rounded_through_the_nodes);
  if (test::failures == 0) {
    std::puts("test_fillet_tube_node: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
