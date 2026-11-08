// SPDX-License-Identifier: MIT
// A gable roof's slab rounded on all its edges (mitcad#1482), reconstructed
// from a reported design: a section of 7 points on the yz plane, extruded
// 25.2 along x. The slab's slopes rise at about 60 degrees, 2.6 apart
// measured vertically (1.3 square to them), from eaves cut level at
// z = 17.8: each eave is a level face 1.506 wide (y from -14.906 to -13.4,
// and from 35.2 to 36.706), between the acute edge at the tip (60 degrees)
// and the obtuse one where the underside begins (120 degrees). The
// underside ends in a level ceiling below the ridge.
//
// All edges rounded:
// - with 0.5 the roundings fit on every face (on an eave 1.73 r and 0.58 r
//   from its edges, 1.15 of its 1.506; on the end faces r from each slope's
//   edge, 1.0 of 1.3): a valid closed solid;
// - with 0.9 and 1.0 the roundings of each eave's two edges together reach
//   farther across it than it is wide (2.08 and 2.31 of 1.506), as those of
//   the slopes' edges across the end faces do. The design built with 0.9
//   and failed with 1.0 ("the fillet would leave the body open", and on
//   another build OCCT's "cannot join the roundings" at a vertex). With
//   the port's patches 1640 to 1659 (mitcad#1494, #1499, #1500, #1530, #1532, #1533:
//   rides on the end faces, the tip's rounding rolling on the eave's obtuse
//   edge, the corners at the eaves' ends) each builds a valid closed solid,
//   at 0.7, 0.8, 0.862, 0.865 and 1.1 too, its volume checked against the
//   roundings' union.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <exception>
#include <string>
#include <vector>

#include <BRepCheck_Analyzer.hxx>
#include <BRepGProp.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <gp_Vec2d.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt2d.hxx>

#include "check.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/extrude.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

// The section's points (y, z), counter-clockwise: the slopes rise
// 2.6 / 1.506 (59.9 degrees) from the eaves' tips, the underside runs
// parallel to them 2.6 lower, and the ceiling is level 3 below where the
// underside's lines would meet.
std::vector<gp_Pnt2d> section() {
  const double rise = 2.6 / 1.506;
  const double ridge = 17.8 + (10.9 + 14.906) * rise;
  const double ceiling = ridge - 2.6 - 3.0;
  const double ceiling_left = -13.4 + (ceiling - 17.8) / rise;
  return {gp_Pnt2d(-14.906, 17.8),
          gp_Pnt2d(-13.4, 17.8),
          gp_Pnt2d(ceiling_left, ceiling),
          gp_Pnt2d(21.8 - ceiling_left, ceiling),
          gp_Pnt2d(35.2, 17.8),
          gp_Pnt2d(36.706, 17.8),
          gp_Pnt2d(10.9, ridge)};
}

// The section on the yz plane extruded 25.2 along x, its lines c8 to c14.
ShapePtr slab() {
  const std::vector<gp_Pnt2d> points = section();
  Region region;
  Loop loop;
  std::string name = "r{";
  for (std::size_t i = 0; i < points.size(); ++i) {
    Segment segment;
    segment.name = "c" + std::to_string(8 + static_cast<int>(i));
    segment.start = points[i];
    segment.end = points[(i + 1) % points.size()];
    loop.segments.push_back(segment);
    name += (i == 0 ? "" : ",") + segment.name;
  }
  region.loops.push_back(loop);
  region.name = name + "}";
  ExtrudeSpec spec;
  spec.feature = "F6";
  spec.frame.x_axis = gp_Dir(0.0, 1.0, 0.0);
  spec.frame.y_axis = gp_Dir(0.0, 0.0, 1.0);
  spec.direction = gp_Dir(1.0, 0.0, 0.0);
  spec.regions = {region};
  spec.start = 0.0;
  spec.end = 25.2;
  return extrude(spec);
}

// Whether the shape is one solid whose every edge lies on two faces.
bool one_closed_solid(const TopoDS_Shape& shape) {
  int solids = 0;
  for (TopExp_Explorer solid(shape, TopAbs_SOLID); solid.More(); solid.Next()) {
    ++solids;
  }
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces;
  TopExp::MapShapesAndAncestors(shape, TopAbs_EDGE, TopAbs_FACE, faces);
  for (int i = 1; i <= faces.Extent(); ++i) {
    if (!BRep_Tool::Degenerated(TopoDS::Edge(faces.FindKey(i))) && faces(i).Extent() != 2) {
      return false;
    }
  }
  return solids == 1;
}

// Every edge of the slab rounded with the radius: the result, or null with
// the error in `error`.
ShapePtr all_edges_rounded(const Shape& body, double radius, std::string& error) {
  std::vector<std::string> edges;
  for (int e = 0; e < body.edge_count(); ++e) {
    if (!body.edge_name(e).empty()) {
      edges.push_back(body.edge_name(e));
    }
  }
  CHECK(static_cast<int>(edges.size()) == body.edge_count());
  try {
    return fillet("F7", body, edges, radius);
  } catch (const std::exception& failure) {
    error = failure.what();
  }
  return nullptr;
}

// ------------------------------------------------- the roundings' union
//
// The slab's volume rounded on all its edges as the union of the
// roundings' removal sets (and the concave ceiling corners' fills), in the
// section (y, z):
// - each edge along x takes the triangle of its vertex and its two contacts
//   less its ball's segment over the chord, along the whole 25.2; at an
//   eave, from r = 0.65 on, the obtuse edge's rounding rides (0016, 1658):
//   its ball passes through the tip's contact line on the eave (where that
//   line is within 0.02 r of the obtuse edge, the ride's walk fails and the
//   rounding is dropped, 1658: the ride takes less than 0.003 a side
//   there); from r = 0.87 on the tip's ball touches the top slope and
//   passes through the obtuse edge, which takes nothing (1650);
// - each concave ceiling corner adds the triangle less the segment outside
//   the section, along 25.2;
// - each end face's edge takes, at a point of the section outside those,
//   the depth in x of a 90 degree edge's rounding, r - sqrt(r^2 - (r - d)^2)
//   at the distance d from its side; the underside's end edge, from r = 0.65
//   on, rides on the top slope's line (its ball through it): x6 - sqrt(r^2 -
//   (r - w)^2); the points take the largest depth, integrated over strips
//   along each side (each point in the strip of its nearest side, a grid of
//   0.01 along by 0.002 across, within 0.015 of a finer one), at both ends.
// The corners at the 14 vertices of the ends are no such union: they fill
// between the roundings (a ball's or a plate's corner at the convex ones, a
// mixed corner at the ceiling's), which the check's tolerance allows.

struct Region {
  gp_Pnt2d v, a, b;   // the vertex and the arc's ends (the contacts)
  gp_Pnt2d c;         // the ball's centre
  double r = 0.;
};

double cross2(const gp_Pnt2d& o, const gp_Pnt2d& a, const gp_Pnt2d& b) {
  return (a.X() - o.X()) * (b.Y() - o.Y()) - (a.Y() - o.Y()) * (b.X() - o.X());
}

// The triangle of the vertex and the two contacts less the ball's segment.
double region_area(const Region& g) {
  const double triangle = 0.5 * std::abs(cross2(g.v, g.a, g.b));
  const double chord = g.a.Distance(g.b);
  const double angle = 2.0 * std::asin(std::min(1.0, chord / (2.0 * g.r)));
  return triangle - 0.5 * g.r * g.r * (angle - std::sin(angle));
}

bool in_region(const Region& g, const gp_Pnt2d& p) {
  const double s1 = cross2(g.v, g.a, p), s2 = cross2(g.a, g.b, p), s3 = cross2(g.b, g.v, p);
  const bool in = (s1 >= 0 && s2 >= 0 && s3 >= 0) || (s1 <= 0 && s2 <= 0 && s3 <= 0);
  return in && p.SquareDistance(g.c) > g.r * g.r;
}

// A ball of radius r touching the line through q along unit direction d (on
// the side of unit normal n) and passing through point l: the one whose
// contact lies farther along d. Its centre and its contact.
void ball_through(const gp_Pnt2d& q, const gp_Vec2d& d, const gp_Vec2d& n, const gp_Pnt2d& l, double r,
                  gp_Pnt2d& centre, gp_Pnt2d& contact) {
  const gp_Vec2d ql(q, l);
  const double along = ql.Dot(d), across = ql.Dot(n);
  const double s = along + std::sqrt(std::max(0.0, r * r - (r - across) * (r - across)));
  contact = q.Translated(d * s);
  centre = contact.Translated(n * r);
}

// The ball of an edge along x at vertex v between the sides to p (before)
// and to q (after), touching both: its region (inside or, at a concave
// vertex, outside the section).
Region corner_region(const gp_Pnt2d& p, const gp_Pnt2d& v, const gp_Pnt2d& q, double r) {
  const gp_Vec2d d1 = gp_Vec2d(v, p).Normalized(), d2 = gp_Vec2d(v, q).Normalized();
  const double half = 0.5 * std::acos(std::max(-1.0, std::min(1.0, d1.Dot(d2))));
  const double t = r / std::tan(half);
  Region g;
  g.v = v;
  g.r = r;
  g.a = v.Translated(d1 * t);
  g.b = v.Translated(d2 * t);
  g.c = v.Translated((d1 + d2).Normalized() * (r / std::sin(half)));
  return g;
}

bool in_polygon(const std::vector<gp_Pnt2d>& s, const gp_Pnt2d& p) {
  bool in = false;
  for (std::size_t i = 0; i < s.size(); ++i) {
    const gp_Pnt2d& a = s[i];
    const gp_Pnt2d& b = s[(i + 1) % s.size()];
    if ((a.Y() > p.Y()) != (b.Y() > p.Y()) &&
        p.X() < a.X() + (p.Y() - a.Y()) * (b.X() - a.X()) / (b.Y() - a.Y())) {
      in = !in;
    }
  }
  return in;
}

double polygon_area(const std::vector<gp_Pnt2d>& s) {
  double a = 0.0;
  for (std::size_t i = 0; i < s.size(); ++i) {
    a += s[i].X() * s[(i + 1) % s.size()].Y() - s[(i + 1) % s.size()].X() * s[i].Y();
  }
  return 0.5 * std::abs(a);
}

double union_volume(double r) {
  const std::vector<gp_Pnt2d> s = section(); // 0 tip, 1 obtuse, 2, 3 ceiling, 4 obtuse, 5 tip, 6 ridge
  const int n = static_cast<int>(s.size());
  const double length = 25.2;
  // The thickness across the slopes and the eaves' thresholds.
  const gp_Vec2d up = gp_Vec2d(s[0], s[6]).Normalized();
  const double thick = std::abs(gp_Vec2d(s[0], s[1]).Crossed(up));
  const double tip_reach = r / std::tan(0.5 * std::acos(gp_Vec2d(s[0], s[1]).Normalized().Dot(up)));
  const double width = s[0].Distance(s[1]);
  const double obtuse_reach =
    r / std::tan(0.5 * std::acos(gp_Vec2d(s[1], s[0]).Normalized().Dot(gp_Vec2d(s[1], s[2]).Normalized())));
  const bool overlap = tip_reach + obtuse_reach > width, rolls = tip_reach > width;
  std::vector<Region> removed, added;
  for (int i = 0; i < n; ++i) {
    const gp_Pnt2d& p = s[(i + n - 1) % n];
    const gp_Pnt2d& v = s[i];
    const gp_Pnt2d& q = s[(i + 1) % n];
    const Region g = corner_region(p, v, q, r);
    if (i == 2 || i == 3) {
      added.push_back(g);
      continue;
    }
    const bool left = i <= 1;
    const bool tip = i == 0 || i == 5;
    if ((i <= 1 || i == 4 || i == 5) && overlap) {
      const gp_Pnt2d tv = s[left ? 0 : 5], ov = s[left ? 1 : 4];
      const gp_Pnt2d slope = s[6], under = s[left ? 2 : 3];
      if (rolls) {
        if (!tip) {
          continue; // ridden on all along: nothing (1650)
        }
        // The tip's ball touches the top slope and passes through the obtuse edge.
        const gp_Vec2d d = gp_Vec2d(tv, slope).Normalized();
        gp_Vec2d nn(-d.Y(), d.X());
        if (gp_Vec2d(tv, ov).Dot(nn) < 0) {
          nn.Reverse();
        }
        Region t;
        t.v = tv;
        t.r = r;
        t.b = ov;
        ball_through(tv, d, nn, ov, r, t.c, t.a);
        removed.push_back(t);
        continue;
      }
      // The rider: the obtuse edge, whose contact lies nearer its edge (1658).
      if (tip) {
        removed.push_back(g);
        continue;
      }
      // (Its ball through the tip's contact on the eave.)
      const gp_Pnt2d line = left ? corner_region(s[6], s[0], s[1], r).b : corner_region(s[4], s[5], s[6], r).a;
      const gp_Pnt2d far = under;
      const gp_Vec2d d = gp_Vec2d(v, far).Normalized();
      gp_Vec2d nn(-d.Y(), d.X());
      if (gp_Vec2d(v, line).Dot(nn) < 0) {
        nn.Reverse();
      }
      Region t;
      t.v = v;
      t.r = r;
      t.b = line;
      ball_through(v, d, nn, line, r, t.c, t.a);
      removed.push_back(t);
      continue;
    }
    removed.push_back(g);
  }
  double removed_area = 0.0, added_area = 0.0;
  for (const Region& g : removed) {
    removed_area += region_area(g);
  }
  for (const Region& g : added) {
    added_area += region_area(g);
  }
  // The end faces' edges: side i from s[i] to s[i+1].
  const bool rides = 2.0 * r > thick;
  const double x_rider = rides ? std::sqrt(r * r - (2.0 * r - thick) * (2.0 * r - thick)) : 0.0;
  const auto depth = [&](int i, const gp_Pnt2d& p) {
    const gp_Pnt2d& a = s[i];
    const gp_Pnt2d& b = s[(i + 1) % n];
    const gp_Vec2d d = gp_Vec2d(a, b).Normalized();
    const double dist = gp_Vec2d(a, p).Crossed(d) * -1.0; // into the section (counter-clockwise)
    const double along = gp_Vec2d(a, p).Dot(d);
    const double len = a.Distance(b);
    // (Cut where a side meets a concave vertex: the ceiling's corners.)
    const bool concave_start = i == 2 || i == 3, concave_end = i == 1 || i == 2;
    if (dist < 0.0 || dist >= r || (concave_start && along < 0.0) || (concave_end && along > len)) {
      return 0.0;
    }
    if (rides && (i == 1 || i == 3)) { // an underside's end edge rides on the slope's line
      const double h = r - dist;
      return std::max(0.0, x_rider - std::sqrt(r * r - h * h));
    }
    const double h = r - dist;
    return r - std::sqrt(r * r - h * h);
  };
  double ends = 0.0;
  const double ds = 0.01, dd = 0.002;
  for (int i = 0; i < n; ++i) {
    const gp_Pnt2d& a = s[i];
    const gp_Pnt2d& b = s[(i + 1) % n];
    const gp_Vec2d d = gp_Vec2d(a, b).Normalized();
    const gp_Vec2d in(-d.Y(), d.X()); // counter-clockwise: the section's inside
    const double len = a.Distance(b);
    for (double t = -r + ds / 2; t < len + r; t += ds) {
      for (double h = dd / 2; h < r; h += dd) {
        const gp_Pnt2d pt = a.Translated(d * t + in * h);
        if (!in_polygon(s, pt)) {
          continue;
        }
        // (Only the points nearest to this side.)
        double best = 1.e100;
        int nearest = -1;
        for (int j = 0; j < n; ++j) {
          const gp_Pnt2d& aj = s[j];
          const gp_Pnt2d& bj = s[(j + 1) % n];
          const gp_Vec2d dj(aj, bj);
          const double u = std::max(0.0, std::min(1.0, gp_Vec2d(aj, pt).Dot(dj) / dj.SquareMagnitude()));
          const double dist = pt.Distance(aj.Translated(dj * u));
          if (dist < best) {
            best = dist;
            nearest = j;
          }
        }
        if (nearest != i) {
          continue;
        }
        bool taken = false;
        for (const Region& g : removed) {
          taken = taken || in_region(g, pt);
        }
        if (taken) {
          continue;
        }
        double x = 0.0;
        for (int j = 0; j < n; ++j) {
          x = std::max(x, depth(j, pt));
        }
        ends += x * ds * dd;
      }
    }
  }
  return polygon_area(s) * length - length * removed_area + length * added_area - 2.0 * ends;
}

void test_slab() {
  const ShapePtr body = slab();
  // 7 sides and the two ends; 21 edges.
  CHECK(body->face_count() == 9);
  CHECK(body->edge_count() == 21);
  for (const double radius : {0.5, 0.7, 0.8, 0.862, 0.865, 0.9, 1.0, 1.1}) {
    std::string error;
    const ShapePtr rounded = all_edges_rounded(*body, radius, error);
    CHECK(rounded != nullptr);
    if (!rounded) {
      std::fprintf(stderr, "  radius %.3f: %s\n", radius, error.c_str());
      continue;
    }
    std::printf("  radius %.3f: %d faces\n", radius, rounded->face_count());
    CHECK(BRepCheck_Analyzer(rounded->occt()).IsValid());
    CHECK(one_closed_solid(rounded->occt()));
    GProp_GProps props;
    BRepGProp::VolumeProperties(rounded->occt(), props, 1e-9, true);
    const double model = union_volume(radius);
    std::printf("    volume %.6f, the roundings' union %.6f, difference %.6f\n", props.Mass(), model,
                props.Mass() - model);
    // The corners take a little more than the union (a ball's corner at a
    // convex vertex of three 90 degree edges about 0.06 r^3 more; 14
    // vertices), never less beyond the integration's error (0.03).
    CHECK(props.Mass() - model <= 0.03);
    CHECK(props.Mass() - model >= -1.5 * radius * radius * radius);
  }
}

} // namespace
} // namespace test

int main() {
  test::guarded("roof slab", test::test_slab);
  if (test::failures == 0) {
    std::puts("test_fillet_roof_slab: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
