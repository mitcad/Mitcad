// SPDX-License-Identifier: MIT
#include "mitcad/geometry/flex_route.hpp"

// How a route is made (mitcad#1713):
//
// 1. Each node gets its fixed point: a node on a face its point lifted off
//    the face by half the section's height, a node in space its point (as
//    far off a body it lies nearer to, unless it is a port's).
// 2. Between each two nodes a first line: along the surfaces (flex_mesh's
//    shortest path, lifted off them, round a convex edge by an arc and into
//    a concave one up to where both faces are half a height away, straight
//    across a gap the item spans rather than round the inside of it), or
//    through the air round the bodies.
// 3. The whole line as points at even steps, moved until they rest (relax):
//    each bend no tighter than allowed (a point and its two neighbours
//    moved apart), no point nearer to a body than half the height (than
//    the section's reach towards it where a twist turns it, a node too),
//    nor the line between two points,
//    the points of a surface stretch drawn back to the surface where a bend
//    lifted them, those in the air drawn tight. The nodes stay. Coarse to
//    fine: first at wider steps, then the bends and the bodies alone at
//    the steps.
// 4. A B-spline through the points that matter, lifted where it comes
//    nearer to a body than half the height (kept_off), the section's frames
//    along it, and the checks: the bends, the twist, and the section
//    against the bodies (any reach into one above a thousandth of a mm).

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <exception>
#include <memory>
#include <optional>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepGProp.hxx>
#include <BRepOffsetAPI_MakeOffset.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <BRep_Tool.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <GProp_GProps.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <GeomAdaptor_Curve.hxx>
#include <Geom_BSplineCurve.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_HArray1.hxx>
#include <OSD_Parallel.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax2.hxx>
#include <gp_Lin.hxx>
#include <gp_Pln.hxx>
#include <gp_Vec.hxx>
#include <gp_XYZ.hxx>

#include "flex_mesh.hpp"
#include "mitcad/geometry/path_sweep.hpp"
#include "spine.hpp"
#include "util.hpp"

namespace mitcad::geometry::routing {

namespace {

using detail::el;
using detail::FlexMesh;
using detail::FlexNearest;
using detail::FlexSurfacePoint;
using detail::isize;

constexpr double kPi = 3.14159265358979323846;
// A bend radius that allows no bend.
constexpr double kStraight = 1.0e9;
// A bend this much tighter than its limit is a problem; the pass that
// bends the route within its limits stops near them, not on them.
constexpr double kBendSlack = 1.15;
// The pass aims at radii this much wider than the limits: the spline
// through its points, and the frames along the spline, bend a little
// tighter here and there than the points did (a flat item's twist, where
// its edgewise limit is ten times its flat one, by more than the slack;
// at 1.1, standing on its edge at a half turn on a cylinder, 1.16 times
// tighter than its edgewise limit, mitcad#1713).
constexpr double kAim = 1.2;

gp_XYZ unit_of(const gp_XYZ& v) {
  const double length = v.Modulus();
  return length > 1.0e-300 ? v / length : gp_XYZ(0.0, 0.0, 0.0);
}

// A unit vector square to the unit vector t.
gp_XYZ any_square(const gp_XYZ& t) {
  return t.Modulus() > 0.5 ? detail::any_normal(gp_Dir(t)).XYZ() : gp_XYZ(1.0, 0.0, 0.0);
}

// v made square to the unit vector t, a unit vector; any vector square to
// t when v runs along it.
gp_XYZ square_to(const gp_XYZ& v, const gp_XYZ& t) {
  const gp_XYZ across = v - t * v.Dot(t);
  return across.Modulus() > 1.0e-9 ? unit_of(across) : any_square(t);
}

// v turned about the unit axis by the angle (right hand).
gp_XYZ turned(const gp_XYZ& v, const gp_XYZ& axis, double angle) {
  const double c = std::cos(angle);
  const double s = std::sin(angle);
  return v * c + axis.Crossed(v) * s + axis * (axis.Dot(v) * (1.0 - c));
}

// The angle from a to b about the axis.
double angle_about(const gp_XYZ& a, const gp_XYZ& b, const gp_XYZ& axis) {
  return std::atan2(a.Crossed(b).Dot(axis), a.Dot(b));
}

// How far the middle of three points of a circle of the radius lies off
// the middle of the chord between the outer two, half the chord long.
double sag_limit(double radius, double half_chord) {
  if (half_chord >= radius) {
    return radius;
  }
  return half_chord * half_chord / (radius + std::sqrt(radius * radius - half_chord * half_chord));
}

// What of the value lies beyond the limit either way.
double beyond(double value, double limit) {
  return value > limit ? value - limit : value < -limit ? value + limit : 0.0;
}

double smooth_step(double x) { return x <= 0.0 ? 0.0 : x >= 1.0 ? 1.0 : x * x * (3.0 - 2.0 * x); }

std::vector<double> lengths_along(const std::vector<gp_XYZ>& line) {
  std::vector<double> at(line.size(), 0.0);
  for (int i = 1; i < isize(line); ++i) {
    el(at, i) = el(at, i - 1) + (el(line, i) - el(line, i - 1)).Modulus();
  }
  return at;
}

// The line as points at even steps along it, `intervals` steps.
std::vector<gp_XYZ> resampled(const std::vector<gp_XYZ>& line, int intervals) {
  const std::vector<double> at = lengths_along(line);
  const double total = at.back();
  std::vector<gp_XYZ> out;
  int segment = 0;
  for (int i = 0; i <= intervals; ++i) {
    const double want = total * i / intervals;
    while (segment + 2 < isize(line) && el(at, segment + 1) < want) {
      ++segment;
    }
    const double run = el(at, segment + 1) - el(at, segment);
    const double f = run > 1.0e-15 ? std::clamp((want - el(at, segment)) / run, 0.0, 1.0) : 0.0;
    out.push_back(el(line, segment) + (el(line, segment + 1) - el(line, segment)) * f);
  }
  out.front() = line.front();
  out.back() = line.back();
  return out;
}

// A path along the surfaces lifted `half` off them (and the deflection
// more off curved faces, whose triangles lie inside them): over a convex
// edge by an arc about it, into a concave one up to the point that far
// from both faces.
std::vector<gp_XYZ> lifted(const std::vector<FlexSurfacePoint>& path, double half, double deflection) {
  std::vector<gp_XYZ> out;
  for (int j = 0; j < isize(path); ++j) {
    const FlexSurfacePoint& s = el(path, j);
    const double off = half + (s.curved ? deflection : 0.0);
    const double cosine = std::clamp(s.normal_in.Dot(s.normal_out), -1.0, 1.0);
    const gp_XYZ axis = unit_of(s.normal_in.Crossed(s.normal_out));
    if (cosine > 0.94 || j == 0 || axis.Modulus() < 0.5) {
      // (Within 20 degrees: one surface.)
      out.push_back(s.point + unit_of(s.normal_in + s.normal_out + s.normal_in * 1.0e-12) * off);
      continue;
    }
    const gp_XYZ arriving = s.point - el(path, j - 1).point;
    if (s.normal_out.Dot(arriving) > 0.0) {
      const double angle = std::acos(cosine);
      const int steps = std::max(1, static_cast<int>(std::ceil(angle / (kPi / 12.0))));
      for (int k = 0; k <= steps; ++k) {
        out.push_back(s.point + turned(s.normal_in, axis, angle * k / steps) * off);
      }
    } else if (1.0 + cosine > 0.2) {
      out.push_back(s.point + (s.normal_in + s.normal_out) * (off / (1.0 + cosine)));
    } else {
      out.push_back(s.point + s.normal_in * off);
      out.push_back(s.point + s.normal_out * off);
    }
  }
  return out;
}

// The lifted path along the surfaces without the loops it makes round the
// inside of a gap the item spans (mitcad#1713: a wire 5 mm across from the
// top of a 20 x 20 profile to a node on the far wall of its T-slot's mouth,
// 5.2 mm wide: the shortest way along the surfaces runs down the near wall,
// round the slot's cavity and up the far wall, while the wire crosses the
// mouth; lifted off the walls the path passed within 0.2 mm of itself, and
// the pull back to the surfaces held the route down in the cavity, a dip
// below the node and, in the pointer's quicker route, a knot bent to
// 1.4 mm with a bend radius of 4). Where two points of the path lie no
// farther apart than `across` and than half the way between them along it,
// and the line between them keeps `need` off every body, the line takes the
// place of the way between them. The same holds measured across the
// stretch, square to the line from its first point to its last: the
// shortest way along the surfaces goes down into the slot at once and
// along the cavity, so that the points on the near and the far wall of
// the mouth are far apart along the stretch, though only the mouth apart
// across it. A path over a convex edge or into a concave corner is not
// cut: the line there passes nearer to the body, or is not that much
// shorter than the way along the faces.
std::vector<gp_XYZ> across_gaps(const std::vector<gp_XYZ>& line, const FlexMesh& mesh, double need, double across) {
  const int count = isize(line);
  if (count < 3 || mesh.empty()) {
    return line;
  }
  const std::vector<double> at = lengths_along(line);
  // (The way and the chords across the stretch: without their parts along it.)
  const gp_XYZ ends = line.back() - line.front();
  const gp_XYZ way = ends.Modulus() > 1.0e-9 ? unit_of(ends) : gp_XYZ(0, 0, 0);
  const auto square = [&way](const gp_XYZ& v) { return (v - way * v.Dot(way)).Modulus(); };
  std::vector<double> at_across(count, 0.0);
  for (int k = 1; k < count; ++k) {
    el(at_across, k) = el(at_across, k - 1) + square(el(line, k) - el(line, k - 1));
  }
  std::vector<gp_XYZ> out{line.front()};
  int i = 0;
  while (i + 1 < count) {
    int to = i + 1;
    // (The farthest point it can go straight to.)
    for (int j = count - 1; j > i + 1; --j) {
      const gp_XYZ between = el(line, j) - el(line, i);
      const double chord = between.Modulus();
      const bool cut_short = chord <= across && chord <= 0.5 * (el(at, j) - el(at, i));
      // (A way across that is a detour at all: not a stretch straight along
      // a face, which has none.)
      const double chord_across = square(between);
      const double way_across = el(at_across, j) - el(at_across, i);
      const bool cut_across = way_across > need && chord_across <= across && chord_across <= 0.5 * way_across;
      if (!cut_short && !cut_across) {
        continue;
      }
      if (mesh.segment_clear(el(line, i), el(line, j), need)) {
        to = j;
        break;
      }
    }
    out.push_back(el(line, to));
    i = to;
  }
  return out;
}

// A node's turn of the section: where along the route, by how much, over
// what length.
struct Twist {
  double at = 0.0;
  double angle = 0.0;
  double length = 1.0;
};

double twist_at(const std::vector<Twist>& twists, double at) {
  double sum = 0.0;
  for (const Twist& twist : twists) {
    sum += twist.angle * smooth_step((at - twist.at) / twist.length + 0.5);
  }
  return sum;
}

// The section's `down` at each point of a line with unit tangents: at the
// anchored points their `anchor` (the way to the surface, a port's
// orientation) made square to the tangent; between two anchored points
// carried from the first without twisting about the line, and the turn
// that is left at the second spread evenly over the stretch; before the
// first and after the last carried without twisting. Without any, from
// straight down (-Z) at the start. Then turned by the twists.
std::vector<gp_XYZ> frames_along(const std::vector<gp_XYZ>& points, const std::vector<gp_XYZ>& tangents,
                                 const std::vector<double>& at, const std::vector<char>& anchored,
                                 const std::vector<gp_XYZ>& anchor, const std::vector<Twist>& twists) {
  const int count = isize(points);
  std::vector<gp_XYZ> down(points.size());
  std::vector<int> anchors;
  for (int i = 0; i < count; ++i) {
    if (el(anchored, i) != 0) {
      el(down, i) = square_to(el(anchor, i), el(tangents, i));
      anchors.push_back(i);
    }
  }
  // The frame of point i carried to its neighbour j (double reflection).
  const auto carried = [&](int i, int j) {
    const gp_XYZ v1 = el(points, j) - el(points, i);
    const double c1 = v1.SquareModulus();
    if (c1 < 1.0e-24) {
      return square_to(el(down, i), el(tangents, j));
    }
    const gp_XYZ r = el(down, i) - v1 * (2.0 / c1 * v1.Dot(el(down, i)));
    const gp_XYZ t = el(tangents, i) - v1 * (2.0 / c1 * v1.Dot(el(tangents, i)));
    const gp_XYZ v2 = el(tangents, j) - t;
    const double c2 = v2.SquareModulus();
    return square_to(c2 < 1.0e-24 ? r : r - v2 * (2.0 / c2 * v2.Dot(r)), el(tangents, j));
  };
  if (anchors.empty()) {
    el(down, 0) = square_to(gp_XYZ(0.0, 0.0, -1.0), el(tangents, 0));
    for (int i = 1; i < count; ++i) {
      el(down, i) = carried(i - 1, i);
    }
  } else {
    for (int i = anchors.front() - 1; i >= 0; --i) {
      el(down, i) = carried(i + 1, i);
    }
    for (int i = anchors.back() + 1; i < count; ++i) {
      el(down, i) = carried(i - 1, i);
    }
    for (int k = 0; k + 1 < isize(anchors); ++k) {
      const int first = el(anchors, k);
      const int last = el(anchors, k + 1);
      if (last == first + 1) {
        continue;
      }
      for (int i = first + 1; i < last; ++i) {
        el(down, i) = carried(i - 1, i);
      }
      const double left = angle_about(carried(last - 1, last), el(down, last), el(tangents, last));
      const double run = el(at, last) - el(at, first);
      for (int i = first + 1; i < last; ++i) {
        const double share = run > 1.0e-12 ? (el(at, i) - el(at, first)) / run : 0.0;
        el(down, i) = turned(el(down, i), el(tangents, i), left * share);
      }
    }
  }
  if (!twists.empty()) {
    for (int i = 0; i < count; ++i) {
      const double angle = twist_at(twists, el(at, i));
      if (angle != 0.0) {
        el(down, i) = turned(el(down, i), el(tangents, i), angle);
      }
    }
  }
  return down;
}

// What a route is made with.
struct Making {
  const FlexMesh* mesh = nullptr;
  bool round = true;
  double half = 0.0;       // half the section's height
  double width = 0.0;
  double height = 0.0;
  double deflection = 0.0; // of the mesh
  // The radii the route is bent within (a floor of the section's own size
  // where there is no limit).
  double radius = 0.0;
  double flat_radius = 0.0;
  double edge_radius = 0.0;
  int iterations = 0;
  // Ports: the direction and the orientation at the ends, and how long the
  // stretch from a port is that is not held off the bodies.
  std::optional<gp_XYZ> first_direction;
  std::optional<gp_XYZ> last_direction;
  std::optional<gp_XYZ> first_down;
  std::optional<gp_XYZ> last_down;
  double lead = 0.0;
  std::vector<double> node_twist;
  double twist_length_per_angle = 0.0;
};

// The route as points at about even steps: which are held (the nodes, the
// point beside a port), which lie in a port's first stretch, which belong
// to a stretch along the surfaces, and each node's point.
struct Band {
  std::vector<gp_XYZ> p;
  std::vector<char> held;
  std::vector<char> exempt;
  std::vector<char> surface;
  std::vector<int> node;
};

Band band_of(const std::vector<std::vector<gp_XYZ>>& spans, const std::vector<char>& span_surface, double spacing,
             const Making& making) {
  Band band;
  for (int k = 0; k < isize(spans); ++k) {
    const std::vector<gp_XYZ>& line = el(spans, k);
    const double length = lengths_along(line).back();
    const int intervals = std::clamp(static_cast<int>(std::ceil(length / spacing)), 4, 20000);
    const std::vector<gp_XYZ> points = resampled(line, intervals);
    if (k == 0) {
      band.node.push_back(0);
      band.p.push_back(points.front());
      band.surface.push_back(el(span_surface, k));
    }
    for (int i = 1; i < isize(points); ++i) {
      band.p.push_back(el(points, i));
      band.surface.push_back(el(span_surface, k));
    }
    band.node.push_back(isize(band.p) - 1);
  }
  const int count = isize(band.p);
  band.held.assign(band.p.size(), 0);
  band.exempt.assign(band.p.size(), 0);
  for (const int i : band.node) {
    el(band.held, i) = 1;
  }
  // A port's direction: the point beside it held on it.
  const std::vector<double> at = lengths_along(band.p);
  if (making.first_direction) {
    el(band.p, 1) = el(band.p, 0) + *making.first_direction * (el(band.p, 1) - el(band.p, 0)).Modulus();
    el(band.held, 1) = 1;
    for (int i = 0; i < count && el(at, i) < making.lead; ++i) {
      el(band.exempt, i) = 1;
    }
  }
  if (making.last_direction) {
    el(band.p, count - 2) =
        el(band.p, count - 1) - *making.last_direction * (el(band.p, count - 1) - el(band.p, count - 2)).Modulus();
    el(band.held, count - 2) = 1;
    for (int i = count - 1; i >= 0 && at.back() - el(at, i) < making.lead; --i) {
      el(band.exempt, i) = 1;
    }
  }
  return band;
}

std::vector<gp_XYZ> tangents_of(const std::vector<gp_XYZ>& p) {
  const int count = isize(p);
  std::vector<gp_XYZ> t(p.size());
  for (int i = 0; i < count; ++i) {
    el(t, i) = unit_of(el(p, std::min(i + 1, count - 1)) - el(p, std::max(i - 1, 0)));
  }
  return t;
}

// The turns the nodes give the section. Each is spread over the length it
// needs, about its node, moved along the route and cut to it where the
// route is too short: the whole turn is made within the route (and is
// then steeper than the limit, a problem of the route).
std::vector<Twist> twists_of(const Making& making, const std::vector<double>& node_at, double total) {
  std::vector<Twist> twists;
  for (int k = 0; k < isize(node_at) && k < isize(making.node_twist); ++k) {
    const double angle = el(making.node_twist, k);
    if (angle != 0.0) {
      Twist twist;
      const double needed = std::max(std::abs(angle) * making.twist_length_per_angle, 1.0e-6);
      twist.length = std::max(std::min(needed, total), 1.0e-6);
      const double half = twist.length / 2.0;
      twist.at = std::clamp(el(node_at, k), half, std::max(total - half, half));
      twist.angle = angle;
      twists.push_back(twist);
    }
  }
  return twists;
}

// How near two stretches of the item may come to each other: its diameter,
// or the smaller of a section's two sizes (it may lie on itself either
// way).
double self_thickness(const Making& making) {
  return making.round ? making.width : std::min(making.width, making.height);
}

// Where two segments are nearest to each other: how far along each (0 to
// 1), the two points and their distance.
struct SegmentGap {
  double s = 0.0;
  double t = 0.0;
  gp_XYZ on_first;
  gp_XYZ on_second;
  double distance = 0.0;
};

SegmentGap segment_gap(const gp_XYZ& p1, const gp_XYZ& q1, const gp_XYZ& p2, const gp_XYZ& q2) {
  const gp_XYZ d1 = q1 - p1;
  const gp_XYZ d2 = q2 - p2;
  const gp_XYZ r = p1 - p2;
  const double a = d1.Dot(d1);
  const double e = d2.Dot(d2);
  const double f = d2.Dot(r);
  SegmentGap gap;
  if (a <= 1.0e-24 && e <= 1.0e-24) {
    // (Two points.)
  } else if (a <= 1.0e-24) {
    gap.t = std::clamp(f / e, 0.0, 1.0);
  } else {
    const double c = d1.Dot(r);
    if (e <= 1.0e-24) {
      gap.s = std::clamp(-c / a, 0.0, 1.0);
    } else {
      const double b = d1.Dot(d2);
      const double denominator = a * e - b * b;
      gap.s = denominator > 1.0e-24 ? std::clamp((b * f - c * e) / denominator, 0.0, 1.0) : 0.0;
      gap.t = (b * gap.s + f) / e;
      if (gap.t < 0.0) {
        gap.t = 0.0;
        gap.s = std::clamp(-c / a, 0.0, 1.0);
      } else if (gap.t > 1.0) {
        gap.t = 1.0;
        gap.s = std::clamp((b - c) / a, 0.0, 1.0);
      }
    }
  }
  gap.on_first = p1 + d1 * gap.s;
  gap.on_second = p2 + d2 * gap.t;
  gap.distance = (gap.on_first - gap.on_second).Modulus();
  return gap;
}

// How far the section reaches towards the surface it lies on when the
// nodes' twists have turned it by the angle: half its height lying flat,
// half its width standing on its edge, and between them its half diagonal
// along the surface's normal.
double rest_of(const Making& making, double angle) {
  if (making.round) {
    return making.half;
  }
  return making.half * std::abs(std::cos(angle)) + making.width / 2.0 * std::abs(std::sin(angle));
}

// How far off a surface along it the section is still turned towards it:
// half its height, or for a section that is not round what a turn can
// lift it beyond that (its half diagonal less half its height).
double lift_slack(const Making& making) {
  const double half_diagonal = making.round ? making.half : std::hypot(making.width, making.height) / 2.0;
  return std::max(0.5 * making.half, half_diagonal - making.half);
}

// The rest of each point of a line (rest_of by the twists there).
std::vector<double> rests_along(const Making& making, const std::vector<gp_XYZ>& p, const std::vector<int>& nodes) {
  std::vector<double> rest(p.size(), making.half);
  if (making.round) {
    return rest;
  }
  const std::vector<double> at = lengths_along(p);
  std::vector<double> node_at;
  for (const int i : nodes) {
    node_at.push_back(el(at, i));
  }
  const std::vector<Twist> twists = twists_of(making, node_at, at.back());
  if (twists.empty()) {
    return rest;
  }
  std::vector<double> turned_rest(p.size(), making.half);
  for (int i = 0; i < isize(p); ++i) {
    el(turned_rest, i) = rest_of(making, twist_at(twists, el(at, i)));
  }
  // The most of it along as much of the route as the turn takes, at its
  // steepest, to turn the section from standing on its edge to its
  // diagonal: standing on its edge it reaches less far (half its width) than
  // turned a little either side of that (half its diagonal), and a route
  // kept off by just that dipped at the node of a half turn, bent over its
  // edge there (mitcad#1713). (Within the section's height either way the
  // route rose so steeply before the turn that it bent over its edge
  // there.)
  const double window = std::atan2(making.height, making.width) * making.twist_length_per_angle / 1.5;
  int from = 0;
  int to = 0;
  for (int i = 0; i < isize(p); ++i) {
    while (el(at, i) - el(at, from) > window) {
      ++from;
    }
    while (to + 1 < isize(p) && el(at, to + 1) - el(at, i) <= window) {
      ++to;
    }
    double most = making.half;
    for (int k = from; k <= to; ++k) {
      most = std::max(most, el(turned_rest, k));
    }
    el(rest, i) = most;
  }
  return rest;
}

// Moves the band's points until they rest: see the top of the file. Whether
// the bends rested (false where they stopped settling).
bool relax(Band& band, const Making& making, bool drawn, std::int64_t& work) {
  const FlexMesh& mesh = *making.mesh;
  const int count = isize(band.p);
  std::vector<gp_XYZ>& p = band.p;
  // The way from the nearest body to each point, its distance, and the
  // distance to keep: the section's reach towards the surface (more than
  // half its height where a twist turns it; a node is lifted then too).
  std::vector<gp_XYZ> away(p.size());
  std::vector<double> gap(p.size(), 0.0);
  std::vector<double> rest = rests_along(making, p, band.node);
  std::vector<double> need(p.size(), making.half);
  std::vector<char> known(p.size(), 0);
  std::vector<char> seen(p.size(), 0);
  std::vector<char> active(p.size(), 1);
  std::vector<double> moved(p.size(), 0.0);
  std::vector<gp_XYZ> down;
  const double thick = self_thickness(making);
  std::vector<std::pair<int, int>> beside;

  const auto off_bodies = [&](int i, bool push) {
    const FlexNearest hit = mesh.nearest(el(p, i));
    el(seen, i) = 1;
    if (hit.triangle < 0) {
      el(known, i) = 0;
      return;
    }
    const detail::FlexTriangle& triangle = mesh.triangle(hit.triangle);
    // Off a planar face by half the height. The triangles of a curved face
    // lie up to the deflection inside it, and their planes meet at angles
    // the face does not have: a point between half the height and twice
    // the deflection more off them is left where it is (the path lifted
    // off the face's own points runs there, as smooth as the face), a
    // nearer one is put in the middle of that. Where a twist lifts the
    // section off the face the band is the deflection higher: that far off
    // the triangles a turned flat cable's corner reached into the face itself
    // (mitcad#1713). (Kept at the near end of the band always, the points
    // followed the triangles' facets, and the cable standing on its edge at
    // a half turn was bent over its edge by them.)
    const double slack = triangle.curved ? making.deflection : 0.0;
    gp_XYZ way = el(p, i) - hit.point;
    double distance = way.Modulus();
    const bool inside = hit.inside && distance > 1.0e-6;
    way = (inside || distance < 1.0e-9) ? triangle.normal : way / distance;
    if (inside) {
      distance = 0.0;
    }
    const double keep = el(rest, i);
    const bool lifted_node = keep > making.half + 1.0e-9;
    const double raise = lifted_node ? slack : 0.0;
    if (push && (el(band.held, i) == 0 || lifted_node) && el(band.exempt, i) == 0 && distance < keep + raise) {
      const gp_XYZ placed = hit.point + way * (keep + slack + raise);
      el(moved, i) += (placed - el(p, i)).Modulus();
      el(p, i) = placed;
      distance = keep + slack + raise;
    }
    // (Over a curved face the normal that turns evenly, not the triangle's.)
    el(away, i) = triangle.curved && !inside ? mesh.smooth_normal(hit.triangle, hit.point) : way;
    el(gap, i) = distance;
    el(need, i) = keep + 2.0 * slack;
    el(known, i) = 1;
  };

  // The stretch from point i to the next off the bodies as its points are
  // (mitcad#1713): round a convex edge two points either side of it, each
  // half the height off it, are joined by a line that passes nearer (a wire
  // of 1 mm with points 0.65 mm apart, 0.40 mm off the edge, and the curve
  // through them 0.085 mm into the block). Its point nearest to what is
  // nearest to its middle is found, and both ends are moved out by what it
  // lacks (the one that can move, by as much more as it is farther along).
  const auto off_bodies_between = [&](int i) {
    // (Not a stretch from a port's first one, which leaves from inside the
    // pipe.)
    if (el(band.exempt, i) != 0 || el(band.exempt, i + 1) != 0) {
      return;
    }
    const bool first_moves = el(band.held, i) == 0;
    const bool second_moves = el(band.held, i + 1) == 0;
    if (!first_moves && !second_moves) {
      return;
    }
    const gp_XYZ a = el(p, i);
    const gp_XYZ along = el(p, i + 1) - a;
    const double run = along.SquareModulus();
    if (run < 1.0e-24) {
      return;
    }
    // (No point of it is nearer than an end's distance less half its
    // length: far enough off, it is not looked at. A flat cable's pointer
    // preview took a minute when every stretch was.)
    if (el(known, i) != 0 && el(known, i + 1) != 0 &&
        std::min(el(gap, i), el(gap, i + 1)) - 0.5 * std::sqrt(run) >
            std::max(el(rest, i), el(rest, i + 1)) + 2.0 * making.deflection) {
      return;
    }
    const FlexNearest middle = mesh.nearest(a + along * 0.5);
    if (middle.triangle < 0) {
      return;
    }
    const double t = std::clamp((middle.point - a).Dot(along) / run, 0.0, 1.0);
    const gp_XYZ q = a + along * t;
    const FlexNearest hit = mesh.nearest(q);
    if (hit.triangle < 0) {
      return;
    }
    const detail::FlexTriangle& triangle = mesh.triangle(hit.triangle);
    const double keep = std::min(el(rest, i), el(rest, i + 1));
    gp_XYZ way = q - hit.point;
    double distance = way.Modulus();
    const bool inside = hit.inside && distance > 1.0e-6;
    way = (inside || distance < 1.0e-9) ? triangle.normal : way / distance;
    if (inside) {
      distance = -distance;
    }
    // (Off a curved face's triangles into the middle of what is left alone,
    // as off_bodies puts a point.)
    const double lack = keep - distance;
    if (lack <= 1.0e-9) {
      return;
    }
    const double by = lack + (triangle.curved ? making.deflection : 0.0);
    const auto shift = [&](int k, double by) {
      el(p, k) = el(p, k) + way * by;
      el(moved, k) += by;
    };
    // (Not where the nearest place is at a held end, nearly: off_bodies
    // keeps that end off, or it is a node that lies on the body itself, and
    // the other end, moved each pass by four times what is missing there,
    // ran away, 40 mm a pass, mitcad#1713.)
    const double share = first_moves && second_moves ? 1.0 : first_moves ? 1.0 - t : t;
    if (share < 0.1) {
      return;
    }
    if (first_moves && second_moves) {
      shift(i, by);
      shift(i + 1, by);
    } else {
      shift(first_moves ? i : i + 1, by / std::max(share, 0.25));
    }
  };

  const auto frames_now = [&] {
    rest = rests_along(making, p, band.node);
    const std::vector<double> at = lengths_along(p);
    std::vector<char> anchored(p.size(), 0);
    std::vector<gp_XYZ> anchor(p.size());
    for (int i = 0; i < count; ++i) {
      if (el(band.surface, i) != 0 && el(known, i) != 0 && el(gap, i) <= el(need, i) + lift_slack(making)) {
        el(anchored, i) = 1;
        el(anchor, i) = el(away, i).Reversed();
      }
    }
    if (making.first_down) {
      el(anchored, 0) = 1;
      el(anchor, 0) = *making.first_down;
    }
    if (making.last_down) {
      el(anchored, count - 1) = 1;
      el(anchor, count - 1) = *making.last_down;
    }
    std::vector<double> node_at;
    for (const int i : band.node) {
      node_at.push_back(el(at, i));
    }
    return frames_along(p, tangents_of(p), at, anchored, anchor, twists_of(making, node_at, at.back()));
  };

  // One bend made no tighter than allowed: the point and its neighbours
  // moved apart by what its middle lies too far off their chord. Returns
  // that excess.
  const auto bend = [&](int i) {
    const gp_XYZ chord = el(p, i + 1) - el(p, i - 1);
    const double half_chord = chord.Modulus() / 2.0;
    if (half_chord < 1.0e-12) {
      return 0.0;
    }
    const gp_XYZ off = el(p, i) - (el(p, i - 1) + el(p, i + 1)) * 0.5;
    gp_XYZ excess(0.0, 0.0, 0.0);
    if (making.round) {
      const double limit = sag_limit(kAim * making.radius, half_chord);
      const double size = off.Modulus();
      if (size <= limit) {
        return 0.0;
      }
      excess = off * (1.0 - limit / size);
    } else {
      const gp_XYZ t = chord / (2.0 * half_chord);
      const gp_XYZ flat = square_to(el(down, i), t);
      const gp_XYZ edge = t.Crossed(flat);
      const double over_flat = beyond(off.Dot(flat), sag_limit(kAim * making.flat_radius, half_chord));
      const double over_edge = beyond(off.Dot(edge), sag_limit(kAim * making.edge_radius, half_chord));
      if (over_flat == 0.0 && over_edge == 0.0) {
        return 0.0;
      }
      excess = flat * over_flat + edge * over_edge;
    }
    // (The nearest points that meet the limit: the middle one moved twice
    // as far as its neighbours. Other shares do not settle.)
    const double wb = el(band.held, i) != 0 ? 0.0 : 1.0;
    const double wa = el(band.held, i - 1) != 0 ? 0.0 : 0.5;
    const double wc = el(band.held, i + 1) != 0 ? 0.0 : 0.5;
    const double sum = wb + (wa + wc) / 2.0;
    if (sum <= 0.0) {
      return 0.0;
    }
    const double k = 1.0 / sum;
    const double size = excess.Modulus();
    el(p, i) = el(p, i) - excess * (k * wb);
    el(p, i - 1) = el(p, i - 1) + excess * (k * wa);
    el(p, i + 1) = el(p, i + 1) + excess * (k * wc);
    el(moved, i) += size * k * wb;
    el(moved, i - 1) += size * k * wa;
    el(moved, i + 1) += size * k * wc;
    return size;
  };

  // Off its own run: the pairs of segments far apart along the item that
  // lie near each other (looked for every eighth pass), each pushed apart
  // to the item's thickness.
  const auto off_own_run = [&](int pass) {
    if (pass % 8 == 0) {
      beside.clear();
      const double pitch = lengths_along(p).back() / std::max(count - 1, 1);
      const int skip = static_cast<int>(std::ceil(3.0 * thick / std::max(pitch, 1.0e-9))) + 2;
      for (int i = 0; i + 1 < count; ++i) {
        for (int j = i + skip; j + 1 < count; ++j) {
          if ((el(p, i) - el(p, j)).Modulus() < 2.0 * thick + 4.0 * pitch) {
            beside.emplace_back(i, j);
          }
        }
      }
    }
    double deepest = 0.0;
    for (const std::pair<int, int>& two : beside) {
      const int i = two.first;
      const int j = two.second;
      const SegmentGap apart = segment_gap(el(p, i), el(p, i + 1), el(p, j), el(p, j + 1));
      if (apart.distance >= thick) {
        continue;
      }
      gp_XYZ way = apart.on_first - apart.on_second;
      if (apart.distance < 1.0e-9) {
        // (They cross: apart along what is square to both.)
        way = (el(p, i + 1) - el(p, i)).Crossed(el(p, j + 1) - el(p, j));
      }
      if (way.Modulus() < 1.0e-12) {
        continue;
      }
      deepest = std::max(deepest, thick - apart.distance);
      const gp_XYZ push = unit_of(way) * ((thick - apart.distance) / 2.0);
      const auto shift = [&](int k, double weight, double sign) {
        if (el(band.held, k) == 0 && weight > 0.0) {
          el(p, k) = el(p, k) + push * (sign * weight);
          el(moved, k) += push.Modulus() * weight;
        }
      };
      shift(i, 1.0 - apart.s, 1.0);
      shift(i + 1, apart.s, 1.0);
      shift(j, 1.0 - apart.t, -1.0);
      shift(j + 1, apart.t, -1.0);
    }
    return deepest;
  };

  for (int pass = 0; drawn && pass < making.iterations; ++pass) {
    throw_if_cancelled();
    work += count;
    std::fill(moved.begin(), moved.end(), 0.0);
    for (int i = 0; i < count; ++i) {
      if (el(active, i) != 0 || el(seen, i) == 0) {
        off_bodies(i, true);
      }
    }
    for (int i = 0; i + 1 < count; ++i) {
      if (el(active, i) != 0 || el(active, i + 1) != 0) {
        off_bodies_between(i);
      }
    }
    if (!making.round && pass % 8 == 0) {
      down = frames_now();
    }
    // Drawn tight in the air; along a surface drawn straight in it and
    // back to it.
    for (int i = 1; i + 1 < count; ++i) {
      if (el(band.held, i) != 0 || el(active, i) == 0) {
        continue;
      }
      const gp_XYZ pull = (el(p, i - 1) + el(p, i + 1)) * 0.5 - el(p, i);
      gp_XYZ step = pull * 0.2;
      if (el(band.surface, i) != 0 && el(known, i) != 0) {
        const gp_XYZ& normal = el(away, i);
        step = (pull - normal * pull.Dot(normal)) * 0.2;
        const double lift = el(gap, i) - el(need, i);
        if (lift > 0.0) {
          step = step - normal * (0.1 * lift);
        }
      }
      el(p, i) = el(p, i) + step;
      el(moved, i) += step.Modulus();
    }
    for (int i = 1; i + 1 < count; ++i) {
      if (el(active, i - 1) != 0 || el(active, i) != 0 || el(active, i + 1) != 0) {
        bend(i);
      }
    }
    for (int i = count - 2; i >= 1; --i) {
      if (el(active, i - 1) != 0 || el(active, i) != 0 || el(active, i + 1) != 0) {
        bend(i);
      }
    }
    off_own_run(pass);
    // The points that moved, and their neighbours, go on.
    bool any = false;
    std::vector<char> next(p.size(), 0);
    for (int i = 0; i < count; ++i) {
      for (int j = std::max(i - 2, 0); j <= std::min(i + 2, count - 1); ++j) {
        if (el(moved, j) > 1.0e-7) {
          el(next, i) = 1;
          any = true;
          break;
        }
      }
    }
    active.swap(next);
    if (!any) {
      break;
    }
  }
  // Then the bends alone against the bodies (and the item's own run): the
  // pull to the surfaces and the tension above hold a route down on the
  // faces, where it has to
  // leave them to bend no tighter than allowed (over an edge it lifts off
  // both faces well before the edge). The bends' sweeps carry a bend along
  // the route a point per sweep, so they run until the bends rest.
  // They stop too where they no longer settle: limits that cannot all be
  // met (a flat cable between a face and a point beside it, a turn on its
  // edge it may not make) kept them moving to the last sweep, 21 s for a
  // pointer preview (mitcad#1713); the checks then show where.
  std::fill(active.begin(), active.end(), 1);
  const double settled = 1.0e-4 * std::max(making.half, 1.0e-3);
  constexpr int kSettling = 100;
  double least = kStraight;
  double least_before = kStraight;
  bool rested = false;
  for (int pass = 0; pass < 8 * making.iterations; ++pass) {
    if (pass % 64 == 0) {
      throw_if_cancelled();
    }
    work += count;
    std::fill(moved.begin(), moved.end(), 0.0);
    for (int i = 0; i < count; ++i) {
      if (el(active, i) != 0) {
        off_bodies(i, true);
      }
    }
    for (int i = 0; i + 1 < count; ++i) {
      if (el(active, i) != 0 || el(active, i + 1) != 0) {
        off_bodies_between(i);
      }
    }
    if (!making.round && pass % 16 == 0) {
      down = frames_now();
    }
    double worst = 0.0;
    for (int i = 1; i + 1 < count; ++i) {
      if (el(active, i - 1) != 0 || el(active, i) != 0 || el(active, i + 1) != 0) {
        worst = std::max(worst, bend(i));
      }
    }
    for (int i = count - 2; i >= 1; --i) {
      if (el(active, i - 1) != 0 || el(active, i) != 0 || el(active, i + 1) != 0) {
        worst = std::max(worst, bend(i));
      }
    }
    worst = std::max(worst, off_own_run(pass));
    if (worst < settled) {
      rested = true;
      break;
    }
    least = std::min(least, worst);
    if ((pass + 1) % kSettling == 0) {
      // (Settling at the rate of the last window, from the window before:
      // how many passes until they rest; more than twice the passes left,
      // they will not.)
      bool settling = least < 0.99 * least_before;
      if (settling && least_before < kStraight && least > settled) {
        const double windows = std::log(settled / least) / std::log(least / least_before);
        settling = windows * kSettling <= 2.0 * (8 * making.iterations - (pass + 1));
      }
      if (!settling) {
        break;
      }
      least_before = least;
      least = kStraight;
    }
    std::vector<char> next(p.size(), 0);
    for (int i = 0; i < count; ++i) {
      for (int j = std::max(i - 2, 0); j <= std::min(i + 2, count - 1); ++j) {
        if (el(moved, j) > 1.0e-7) {
          el(next, i) = 1;
          break;
        }
      }
    }
    active.swap(next);
  }
  for (int i = 0; i < count; ++i) {
    off_bodies(i, true);
  }
  for (int i = 0; i + 1 < count; ++i) {
    off_bodies_between(i);
  }
  return rested;
}

// Whether the bend at point i of the line is one (above a quarter of a
// degree).
std::vector<double> turns_of(const std::vector<gp_XYZ>& p) {
  std::vector<double> turn(p.size(), 0.0);
  for (int i = 1; i + 1 < isize(p); ++i) {
    const gp_XYZ a = el(p, i) - el(p, i - 1);
    const gp_XYZ b = el(p, i + 1) - el(p, i);
    el(turn, i) = std::atan2(a.Crossed(b).Modulus(), a.Dot(b));
  }
  return turn;
}

ModelCurve model_curve_of(const occ::handle<Geom_BSplineCurve>& spline) {
  ModelCurve out;
  out.kind = ModelCurveKind::BSpline;
  out.degree = spline->Degree();
  for (int i = 1; i <= spline->NbPoles(); ++i) {
    out.poles.push_back(spline->Pole(i));
  }
  const auto& knots = spline->KnotSequence();
  for (int i = knots.Lower(); i <= knots.Upper(); ++i) {
    out.knots.push_back(knots(i));
  }
  return out;
}

// The B-spline through the points at the parameters (rising), with the end
// tangents when given; a polyline where the interpolation fails.
occ::handle<Geom_BSplineCurve> spline_through(const std::vector<gp_XYZ>& points, const std::vector<double>& parameters,
                                              const std::optional<gp_XYZ>& first_tangent,
                                              const std::optional<gp_XYZ>& last_tangent) {
  const int count = isize(points);
  try {
    occ::handle<NCollection_HArray1<gp_Pnt>> through = new NCollection_HArray1<gp_Pnt>(1, count);
    occ::handle<NCollection_HArray1<double>> at = new NCollection_HArray1<double>(1, count);
    for (int i = 0; i < count; ++i) {
      through->SetValue(i + 1, gp_Pnt(el(points, i)));
      at->SetValue(i + 1, el(parameters, i));
    }
    GeomAPI_Interpolate interpolate(through, at, false, 1.0e-9);
    if (first_tangent && last_tangent) {
      // (Unit tangents, as the parameters are lengths along the points: the
      // speed of the curve is about one. Scaled by OCCT from the points
      // round an end, an end's tangent came out 17 times longer and the
      // spline swung back on itself before a node.)
      interpolate.Load(gp_Vec(unit_of(*first_tangent)), gp_Vec(unit_of(*last_tangent)), false);
    }
    interpolate.Perform();
    if (interpolate.IsDone()) {
      return interpolate.Curve();
    }
  } catch (const Standard_Failure&) {
    // (The polyline below.)
  }
  NCollection_Array1<gp_Pnt> poles(1, count);
  NCollection_Array1<double> knots(1, count);
  NCollection_Array1<int> multiplicities(1, count);
  for (int i = 0; i < count; ++i) {
    poles.SetValue(i + 1, gp_Pnt(el(points, i)));
    knots.SetValue(i + 1, el(parameters, i));
    multiplicities.SetValue(i + 1, i == 0 || i == count - 1 ? 2 : 1);
  }
  return new Geom_BSplineCurve(poles, knots, multiplicities, 1);
}

// The band's curve: a B-spline through its nodes, the points of its bends
// (every one of a tight bend, fewer of a gentle one) and, along its
// straight stretches, points at steps that double away from the bends, so
// that no long span of the spline lies beside a bend. With each node's
// parameter, and the points, their parameters and the end tangents it is
// made with.
struct Fitted {
  occ::handle<Geom_BSplineCurve> curve;
  std::vector<double> node_parameter;
  std::vector<gp_XYZ> points;
  std::vector<double> parameters;
  std::optional<gp_XYZ> first_tangent;
  std::optional<gp_XYZ> last_tangent;
};

Fitted fitted(const Band& band, const Making& making) {
  const std::vector<gp_XYZ>& p = band.p;
  const int count = isize(p);
  const std::vector<double> turn = turns_of(p);
  constexpr double kBent = 0.004;
  std::vector<char> keep(p.size(), 0);
  keep.front() = 1;
  keep.back() = 1;
  for (int i = 0; i < count; ++i) {
    if (el(band.held, i) != 0) {
      el(keep, i) = 1;
    }
  }
  // (Along a long gentle bend a point at least every half of the tightest
  // radius allowed: a spline through points farther apart swung beside a
  // twist's lift further than an edgewise bend allows.)
  double tightest = making.radius;
  if (!making.round && making.edge_radius < kStraight) {
    tightest = std::min(tightest, making.edge_radius);
  }
  const double farthest = 0.5 * tightest;
  double gathered = 0.0;
  double since = 0.0;
  for (int i = 1; i + 1 < count; ++i) {
    const double step = (el(p, i) - el(p, i - 1)).Modulus();
    since += step;
    if (el(turn, i) <= kBent) {
      gathered = 0.0;
      continue;
    }
    gathered += el(turn, i);
    // (Every point of a bend near the tightest allowed: with one of them
    // left out the spline bent the rest of it into the next point, tighter
    // than allowed where a hose bent through a node.)
    const bool tight = el(turn, i) >= 0.5 * step / tightest;
    if (el(turn, i - 1) <= kBent || el(turn, i + 1) <= kBent || gathered >= 0.14 || since >= farthest || tight) {
      el(keep, i) = 1;
      gathered = 0.0;
      since = 0.0;
    }
  }
  // Along the straight stretches.
  int before = 0;
  for (int i = 1; i < count; ++i) {
    if (el(keep, i) == 0) {
      continue;
    }
    bool straight = i - before > 1;
    for (int j = before + 1; j < i && straight; ++j) {
      straight = el(turn, j) <= kBent;
    }
    if (straight) {
      const int middle = (before + i) / 2;
      int offset = 1;
      int step = 2;
      while (before + offset < middle) {
        el(keep, before + offset) = 1;
        el(keep, i - offset) = 1;
        offset += step;
        step = std::min(step * 2, 16);
      }
      el(keep, middle) = 1;
    }
    before = i;
  }
  std::vector<gp_XYZ> points;
  std::vector<double> parameters;
  std::vector<double> parameter_of(p.size(), 0.0);
  for (int i = 0; i < count; ++i) {
    if (el(keep, i) != 0) {
      const double step = points.empty() ? 0.0 : (el(p, i) - points.back()).Modulus();
      if (points.empty() || step > 1.0e-6) {
        parameters.push_back(points.empty() ? 0.0 : parameters.back() + step);
        points.push_back(el(p, i));
      } else if (i == count - 1) {
        points.back() = el(p, i);
      }
    }
    el(parameter_of, i) = parameters.back();
  }
  if (points.size() < 2) {
    throw std::invalid_argument("the nodes of the route are in one place");
  }
  if (points.size() == 2) {
    points.insert(points.begin() + 1, (points.front() + points.back()) * 0.5);
    parameters.insert(parameters.begin() + 1, parameters.back() * 0.5);
  }
  std::optional<gp_XYZ> first_tangent;
  std::optional<gp_XYZ> last_tangent;
  if (making.first_direction || making.last_direction) {
    first_tangent = making.first_direction ? *making.first_direction : unit_of(el(points, 1) - points.front());
    last_tangent =
        making.last_direction ? *making.last_direction : unit_of(points.back() - el(points, isize(points) - 2));
  }
  Fitted out;
  out.curve = spline_through(points, parameters, first_tangent, last_tangent);
  for (const int i : band.node) {
    out.node_parameter.push_back(el(parameter_of, i));
  }
  out.points = std::move(points);
  out.parameters = std::move(parameters);
  out.first_tangent = first_tangent;
  out.last_tangent = last_tangent;
  return out;
}

// The curve kept off the bodies (mitcad#1713). The band's points are half
// the height off them (and the deflection more off a curved face's
// triangles), but the curve through them cuts inside where they turn
// sharply: over an edge a wire of 1 mm with a bend radius of 4 passed
// 0.415 mm from the edge, 0.085 mm into the block, between the two points
// either side of it, and a hose rose off a face 0.008 mm into it where the
// curve rang before its bend. Where the curve comes nearer than that, the
// point of it nearest to the body is lifted by what is missing and the
// curve made through it too, until it keeps off everywhere. (A point added
// between two of the curve's halves the span the curve rings over there;
// the band's points pushed out instead, with their neighbours, made the
// route bend tighter, and rise off a face right after its node.)
Fitted kept_off(const Band& band, const Making& making) {
  Fitted fit = fitted(band, making);
  const FlexMesh& mesh = *making.mesh;
  if (mesh.empty()) {
    return fit;
  }
  const double tolerance = 1.0e-4;
  // How much nearer than half the height to the bodies a point is.
  const auto missing_at = [&](const gp_XYZ& point, FlexNearest& hit) {
    hit = mesh.nearest(point);
    if (hit.triangle < 0) {
      return 0.0;
    }
    const bool inside = hit.inside && hit.distance > 1.0e-9;
    return making.half - (inside ? -hit.distance : hit.distance);
  };
  // The nodes that are themselves that near (a node in space on a body's
  // surface, where a click on an edge puts it): the curve cannot be lifted
  // where it comes from such a node or goes to it. (It was lifted there
  // again and again, by points added ever closer together, until the
  // spline rang: 977 mm for a route of 296, mitcad#1713.)
  std::vector<double> sunk;
  for (const double u : fit.node_parameter) {
    FlexNearest hit;
    if (missing_at(fit.curve->Value(u).XYZ(), hit) > tolerance) {
      sunk.push_back(u);
    }
  }
  for (int round = 0; round < 16; ++round) {
    throw_if_cancelled();
    const double first = fit.curve->FirstParameter();
    const double last = fit.curve->LastParameter();
    const int samples = std::clamp(8 * isize(fit.points), 256, 20000);
    const double step = (last - first) / samples;
    // The worst place of each run of samples that come too near: its
    // parameter and the point it is lifted to; none of a run that reaches a
    // sunk node.
    std::vector<std::pair<double, gp_XYZ>> lifts;
    double worst = 0.0;
    double worst_at = 0.0;
    double run_from = 0.0;
    double run_to = 0.0;
    gp_XYZ worst_point;
    const auto close_run = [&] {
      const bool at_node = std::any_of(sunk.begin(), sunk.end(), [&](double u) {
        return u >= run_from - 1.5 * step && u <= run_to + 1.5 * step;
      });
      if (worst > 0.0 && !at_node) {
        lifts.emplace_back(worst_at, worst_point);
      }
      worst = 0.0;
    };
    for (int s = 0; s <= samples; ++s) {
      const double u = first + (last - first) * s / samples;
      // (Not in a port's first stretch, which leaves from inside the pipe.)
      if ((making.first_direction && u - first < making.lead) || (making.last_direction && last - u < making.lead)) {
        close_run();
        continue;
      }
      const gp_XYZ point = fit.curve->Value(u).XYZ();
      // (Half the height off the triangles, as the band's points are: what
      // a twist lifts a flat section by besides is the check's, against the
      // section's outline, and so is the body itself where a curved face
      // lies outside its triangles.)
      FlexNearest hit;
      const double missing = missing_at(point, hit);
      if (hit.triangle < 0 || missing <= tolerance) {
        close_run();
        continue;
      }
      const detail::FlexTriangle& triangle = mesh.triangle(hit.triangle);
      const bool inside = hit.inside && hit.distance > 1.0e-9;
      if (worst <= 0.0) {
        run_from = u;
      }
      run_to = u;
      if (missing > worst) {
        const gp_XYZ towards = point - hit.point;
        const gp_XYZ way = inside || towards.Modulus() < 1.0e-9 ? triangle.normal : unit_of(towards);
        worst = missing;
        worst_at = u;
        worst_point = point + way * (missing + tolerance);
      }
    }
    close_run();
    if (lifts.empty()) {
      break;
    }
    // Each into the curve's points by its parameter: in place of one of
    // them that is nearly there (not a node's), else between two.
    const double close_by = 1.0e-3 * (last - first) / std::max(isize(fit.points), 1);
    for (const auto& lift : lifts) {
      const auto at = std::lower_bound(fit.parameters.begin(), fit.parameters.end(), lift.first);
      const int index = static_cast<int>(at - fit.parameters.begin());
      bool replaced = false;
      for (const int k : {index - 1, index}) {
        if (k > 0 && k + 1 < isize(fit.points) && std::abs(el(fit.parameters, k) - lift.first) < close_by &&
            std::find(fit.node_parameter.begin(), fit.node_parameter.end(), el(fit.parameters, k)) ==
                fit.node_parameter.end()) {
          el(fit.points, k) = lift.second;
          replaced = true;
          break;
        }
      }
      if (!replaced && index > 0 && index < isize(fit.points) &&
          lift.first - el(fit.parameters, index - 1) > 1.0e-9 && el(fit.parameters, index) - lift.first > 1.0e-9) {
        fit.points.insert(fit.points.begin() + index, lift.second);
        fit.parameters.insert(fit.parameters.begin() + index, lift.first);
      }
    }
    fit.curve = spline_through(fit.points, fit.parameters, fit.first_tangent, fit.last_tangent);
  }
  return fit;
}

// The points of the section's outline to check against the bodies, in
// section coordinates (across, away from the surface), drawn in by `inset`.
std::vector<std::array<double, 2>> outline_points(const Making& making, double inset) {
  std::vector<std::array<double, 2>> out;
  if (making.round) {
    const double r = std::max(making.width / 2.0 - inset, 0.0);
    for (int k = 0; k < 12; ++k) {
      out.push_back({r * std::cos(kPi * k / 6.0), r * std::sin(kPi * k / 6.0)});
    }
    return out;
  }
  const double hw = std::max(making.width / 2.0 - inset, 0.0);
  const double hh = std::max(making.height / 2.0 - inset, 0.0);
  const int steps = std::clamp(static_cast<int>(std::ceil(making.width / making.height)), 2, 8);
  for (int k = 0; k <= steps; ++k) {
    const double u = hw * (2.0 * k / steps - 1.0);
    out.push_back({u, -hh});
    out.push_back({u, hh});
  }
  out.push_back({-hw, 0.0});
  out.push_back({hw, 0.0});
  return out;
}

// Adds a problem with a place for each run of points whose `badness` is
// above zero, the place's point where it is worst.
void add_problem(FlexRoute& route, FlexProblemKind kind, const std::vector<double>& badness,
                 const std::vector<int>* bodies = nullptr) {
  FlexProblem problem;
  problem.kind = kind;
  const int count = isize(badness);
  for (int i = 0; i < count; ++i) {
    if (el(badness, i) <= 0.0) {
      continue;
    }
    int worst = i;
    int last = i;
    while (last + 1 < count && el(badness, last + 1) > 0.0) {
      ++last;
      if (el(badness, last) > el(badness, worst)) {
        worst = last;
      }
    }
    FlexProblem::Place place;
    place.from = el(route.samples, i).at;
    place.to = el(route.samples, last).at;
    place.point = el(route.samples, worst).point;
    place.body = bodies != nullptr ? el(*bodies, worst) : -1;
    problem.places.push_back(place);
    i = last;
  }
  if (!problem.places.empty()) {
    route.problems.push_back(std::move(problem));
  }
}

// The times of a route's phases, on one line of standard error when it is
// made (MITCAD_FLEX_TRACE).
class Phases {
public:
  void mark(const char* what) {
    if (!detail::flex_tracing()) {
      return;
    }
    const auto now = std::chrono::steady_clock::now();
    char text[64];
    std::snprintf(text, sizeof text, " %s %.3f", what, std::chrono::duration<double>(now - m_last).count());
    m_line += text;
    m_last = now;
  }
  ~Phases() {
    if (detail::flex_tracing()) {
      std::fprintf(stderr, "flex-trace: route phases:%s\n", m_line.c_str());
    }
  }
  Phases() = default;
  Phases(const Phases&) = delete;
  Phases& operator=(const Phases&) = delete;

private:
  std::chrono::steady_clock::time_point m_last = std::chrono::steady_clock::now();
  std::string m_line;
};

} // namespace

struct FlexRouter::Impl {
  Impl(const std::vector<TopoDS_Shape>& obstacles, const FlexRouterOptions& given)
      : options(given), mesh(obstacles, given.deflection, given.angle) {}

  FlexRoute route(const std::vector<FlexNode>& nodes, const FlexSection& section, const FlexLimits& limits,
                  bool quick) const;

  FlexRouterOptions options;
  FlexMesh mesh;
};

FlexRouter::FlexRouter(const std::vector<TopoDS_Shape>& obstacles, const FlexRouterOptions& options) {
  detail::require_positive("the mesh deflection", options.deflection);
  detail::require_positive("the mesh angle", options.angle);
  m_impl = detail::run("flex router", [&] { return std::make_unique<Impl>(obstacles, options); });
}

FlexRouter::~FlexRouter() = default;
FlexRouter::FlexRouter(FlexRouter&&) noexcept = default;
FlexRouter& FlexRouter::operator=(FlexRouter&&) noexcept = default;

FlexRoute FlexRouter::route(const std::vector<FlexNode>& nodes, const FlexSection& section,
                            const FlexLimits& limits, bool quick) const {
  if (nodes.size() < 2) {
    throw std::invalid_argument("a route takes two nodes or more");
  }
  detail::require_positive("the section's width", section.width);
  if (!section.round()) {
    detail::require_positive("the section's height", section.height);
  }
  detail::require_finite("the bend radius", limits.bend_radius);
  detail::require_finite("the bend radius over the edge", limits.edge_bend_radius);
  detail::require_finite("the twist limit", limits.max_twist);
  return detail::run("flex route", [&] { return m_impl->route(nodes, section, limits, quick); });
}

FlexRoute FlexRouter::Impl::route(const std::vector<FlexNode>& nodes, const FlexSection& section,
                                  const FlexLimits& limits, bool quick) const {
  const int node_count = isize(nodes);
  Phases phases;
  Making making;
  making.mesh = &mesh;
  making.round = section.round();
  making.half = section.half_height();
  making.width = section.width;
  making.height = section.round() ? section.width : section.height;
  making.deflection = mesh.deflection();
  // (A quick route: a quarter of the passes, mitcad#1713.)
  making.iterations = std::max(quick ? options.relax_iterations / 4 : options.relax_iterations, 1);
  making.radius = limits.bend_radius > 0.0 ? limits.bend_radius : making.half;
  making.flat_radius = !limits.flat_bend ? kStraight : making.radius;
  making.edge_radius =
      !limits.edge_bend ? kStraight : limits.edge_bend_radius > 0.0 ? limits.edge_bend_radius : making.width / 2.0;
  making.lead = 1.25 * making.half + making.deflection + 1.0e-3;
  if (nodes.front().direction) {
    making.first_direction = nodes.front().direction->XYZ();
    if (nodes.front().down) {
      making.first_down = nodes.front().down->XYZ();
    }
  }
  if (nodes.back().direction) {
    making.last_direction = nodes.back().direction->XYZ();
    if (nodes.back().down) {
      making.last_down = nodes.back().down->XYZ();
    }
  }
  if (!making.round) {
    for (const FlexNode& node : nodes) {
      making.node_twist.push_back(node.twist);
    }
    // A turn's length: at the twist limit where it is steepest; without a
    // limit four times the section's size for half a turn.
    making.twist_length_per_angle =
        limits.max_twist > 0.0 ? 1.5 / limits.max_twist : 4.0 * std::max(making.width, making.height) / kPi;
  }

  // 1. The nodes' points.
  struct Held {
    bool surface = false;
    int triangle = -1;
    gp_XYZ on;    // the point on the triangles
    gp_XYZ point; // the route's point
  };
  std::vector<Held> held(nodes.size());
  for (int k = 0; k < node_count; ++k) {
    const FlexNode& node = el(nodes, k);
    Held& h = el(held, k);
    h.point = node.point.XYZ();
    if (node.body < 0 || node.body >= mesh.body_count()) {
      // A node in space nearer to a body than half the height (a node on an
      // edge, which comes as its point on the edge) lifted off it to that,
      // as a node on a face is, unless it is a port's. (Left on the surface
      // of a wire, its route dived to it and could not be swept,
      // mitcad#1713.)
      for (int round = 0; round < 3 && !node.direction; ++round) {
        const FlexNearest hit = mesh.nearest(h.point);
        if (hit.triangle < 0) {
          break;
        }
        const detail::FlexTriangle& triangle = mesh.triangle(hit.triangle);
        const double off = making.half + (triangle.curved ? making.deflection : 0.0);
        const bool inside = hit.inside && hit.distance > 1.0e-9;
        if (!inside && hit.distance >= off - 1.0e-9) {
          break;
        }
        const gp_XYZ way = !inside && hit.distance > 1.0e-6 ? (h.point - hit.point) / hit.distance
                           : triangle.curved             ? mesh.smooth_normal(hit.triangle, hit.point)
                                                         : triangle.normal;
        h.point = hit.point + way * off;
      }
      continue;
    }
    const int face = mesh.face_index(node.body, node.face);
    FlexNearest hit = mesh.nearest_on(h.point, node.body, face);
    if (hit.triangle < 0 && face >= 0) {
      hit = mesh.nearest_on(h.point, node.body, -1);
    }
    if (hit.triangle < 0) {
      continue;
    }
    const detail::FlexTriangle& triangle = mesh.triangle(hit.triangle);
    h.surface = true;
    h.triangle = hit.triangle;
    h.on = hit.point;
    h.point = h.point + triangle.normal * (making.half + (triangle.curved ? making.deflection : 0.0));
  }

  // 2. The first line of each stretch.
  std::vector<std::vector<gp_XYZ>> spans;
  std::vector<char> span_surface;
  std::vector<char> span_blocked;
  double total = 0.0;
  std::optional<gp_XYZ> heading;
  const double turn_cost = making.round ? 0.0 : !limits.edge_bend ? 1.0e6 : std::max(limits.edge_bend_radius, 0.0);
  for (int k = 0; k + 1 < node_count; ++k) {
    const Held& from = el(held, k);
    const Held& to = el(held, k + 1);
    std::vector<gp_XYZ> line;
    bool surface = false;
    bool blocked = false;
    const bool leaves_port = k == 0 && making.first_direction.has_value();
    const bool reaches_port = k + 2 == node_count && making.last_direction.has_value();
    if (from.surface && to.surface && !leaves_port && !reaches_port) {
      // (An item that bends badly over its edge: the way that turns less
      // from where the stretch before ends, even when it is longer.)
      const std::vector<FlexSurfacePoint> path = mesh.surface_path(
          from.on, from.triangle, to.on, to.triangle, heading ? &*heading : nullptr, nullptr, turn_cost);
      if (path.size() >= 2) {
        line = lifted(path, making.half, making.deflection);
        surface = true;
      }
    }
    if (!surface) {
      // Through the air, from beyond a port's first stretch.
      gp_XYZ start = from.point;
      gp_XYZ end = to.point;
      if (leaves_port) {
        start = start + *making.first_direction * making.lead;
      }
      if (reaches_port) {
        end = end - *making.last_direction * making.lead;
      }
      bool clean = true;
      line = mesh.free_path(start, end, making.half, clean);
      blocked = !clean;
    }
    line.insert(line.begin(), from.point);
    line.push_back(to.point);
    // (Without points at one place.)
    std::vector<gp_XYZ> apart{line.front()};
    for (const gp_XYZ& point : line) {
      if ((point - apart.back()).Modulus() > 1.0e-9) {
        apart.push_back(point);
      }
    }
    if (apart.size() < 2) {
      apart.push_back(line.back());
    }
    // (Along the surfaces: straight across a gap the item spans, a few of its
    // sizes wide at most, not round the inside of it.)
    if (surface) {
      apart = across_gaps(apart, mesh, making.half, 4.0 * std::max(making.width, making.height));
    }
    heading = unit_of(apart.back() - el(apart, isize(apart) - 2));
    total += lengths_along(apart).back();
    spans.push_back(std::move(apart));
    span_surface.push_back(static_cast<char>(surface));
    span_blocked.push_back(static_cast<char>(blocked));
  }
  if (total < 1.0e-6) {
    throw std::invalid_argument("the nodes of the route are in one place");
  }
  phases.mark("first-lines");

  // 3. The points, moved until they rest; in two runs, at even steps again
  // for the second.
  double reference = 3.0 * std::max(making.half, making.width / 2.0);
  if (making.round) {
    if (limits.bend_radius > 0.0) {
      reference = limits.bend_radius;
    }
  } else {
    double least = kStraight;
    if (limits.flat_bend && limits.bend_radius > 0.0) {
      least = std::min(least, limits.bend_radius);
    }
    if (limits.edge_bend && limits.edge_bend_radius > 0.0) {
      least = std::min(least, limits.edge_bend_radius);
    }
    if (least < kStraight) {
      reference = least;
    }
  }
  // A sixth of the tightest bend's radius, and no more than twice the
  // section's height, so that no body of the item's own size lies unseen
  // between two points.
  const double spacing = std::max({std::min(reference / 6.0, 4.0 * making.half), total / 1200.0, 1.0e-3});
  // Coarse to fine: the moves that bend a route within its limits carry a
  // bend along it a point per sweep, so over many points they rest only
  // after very many sweeps (a wire's turn from a port was still tighter
  // than allowed after thousands). First at four and at twice the steps
  // (no longer than two thirds of the radius, where three points still
  // tell a bend), then at the steps, twice. The pull to the surfaces and
  // the tension only in the first run: in the others they undid the bends
  // the run before had made. (Not at the steps again where the bends did
  // not rest at them: limits that cannot all be met do not rest the second
  // time either, and the sweeps of a pointer preview took seconds.)
  std::vector<double> levels{spacing, spacing};
  while (levels.size() < 4 && 2.0 * levels.front() <= reference / 1.5) {
    levels.insert(levels.begin(), 2.0 * levels.front());
  }
  // A quick route (the pointer's, mitcad#1713): the coarser steps only. The
  // runs at the finest steps, with twice the points or more, were most of
  // a route's time onto a swept wire (1.2 of 1.8 s on the build VM).
  if (quick) {
    while (levels.size() > 1 && levels.back() <= 1.5 * spacing) {
      levels.pop_back();
    }
  }
  Band band = band_of(spans, span_surface, levels.front(), making);
  bool rested = true;
  std::int64_t work = 0;
  for (int run = 0; run < isize(levels); ++run) {
    if (run > 0 && !rested && el(levels, run) == el(levels, run - 1)) {
      break;
    }
    if (run > 0) {
      std::vector<std::vector<gp_XYZ>> again;
      for (int k = 0; k + 1 < isize(band.node); ++k) {
        again.emplace_back(band.p.begin() + el(band.node, k), band.p.begin() + el(band.node, k + 1) + 1);
      }
      band = band_of(again, span_surface, el(levels, run), making);
    }
    if (!mesh.empty() || making.first_direction || making.last_direction || node_count > 2) {
      rested = relax(band, making, run == 0, work);
    }
    phases.mark(run == 0 ? "relax" : "relax-again");
  }

  // 4. The curve and the points along it.
  const Fitted fit = kept_off(band, making);
  phases.mark("curve");
  const occ::handle<Geom_BSplineCurve>& curve = fit.curve;
  FlexRoute made;
  made.relax_work = work;
  made.curve = model_curve_of(curve);
  const double first_parameter = curve->FirstParameter();
  const double last_parameter = curve->LastParameter();
  // Points at even steps of the parameter between each two nodes, one at
  // each node.
  const int wanted = std::clamp(2 * isize(band.p), 32, 3000);
  std::vector<double> parameters;
  std::vector<int> node_sample;
  for (int k = 0; k + 1 < isize(fit.node_parameter); ++k) {
    const double from = el(fit.node_parameter, k);
    const double to = el(fit.node_parameter, k + 1);
    const double share = (to - from) / std::max(last_parameter - first_parameter, 1.0e-12);
    const int steps = std::max(2, static_cast<int>(std::ceil(wanted * share)));
    if (k == 0) {
      node_sample.push_back(0);
      parameters.push_back(from);
    }
    for (int i = 1; i <= steps; ++i) {
      parameters.push_back(from + (to - from) * i / steps);
    }
    node_sample.push_back(isize(parameters) - 1);
  }
  const int count = isize(parameters);
  std::vector<gp_XYZ> points(static_cast<std::size_t>(count));
  std::vector<gp_XYZ> tangents(static_cast<std::size_t>(count));
  std::vector<gp_XYZ> bends(static_cast<std::size_t>(count)); // the curvature, towards its centre
  for (int i = 0; i < count; ++i) {
    const double u = el(parameters, i);
    gp_Pnt place;
    gp_Vec d1;
    gp_Vec d2;
    curve->D2(u, place, d1, d2);
    el(points, i) = place.XYZ();
    const double speed = d1.Magnitude();
    if (speed > 1.0e-12) {
      el(tangents, i) = d1.XYZ() / speed;
      el(bends, i) = (d2.XYZ() - el(tangents, i) * d2.XYZ().Dot(el(tangents, i))) / (speed * speed);
    } else if (i > 0) {
      el(tangents, i) = el(tangents, i - 1);
    } else {
      el(tangents, i) = unit_of(band.p.back() - band.p.front());
    }
  }
  std::vector<double> at = lengths_along(points);
  double length = at.back();
  try {
    length = GCPnts_AbscissaPoint::Length(GeomAdaptor_Curve(curve));
  } catch (const Standard_Failure&) {
    // (The length along the points.)
  }
  const double scale = at.back() > 0.0 ? length / at.back() : 1.0;
  for (double& value : at) {
    value *= scale;
  }
  made.length = length;
  for (const int i : node_sample) {
    made.node_at.push_back(el(at, i));
  }

  // The frames: on a surface stretch `down` points to the surface where
  // the item lies on it.
  const double touch = std::max({2.0 * making.deflection, 0.02 * std::min(making.width, making.height), 1.0e-3});
  std::vector<char> on_surface(static_cast<std::size_t>(count), 0);
  std::vector<char> exempt(static_cast<std::size_t>(count), 0);
  std::vector<double> gap(static_cast<std::size_t>(count), detail::kFlexFar);
  std::vector<char> anchored(static_cast<std::size_t>(count), 0);
  std::vector<gp_XYZ> anchor(static_cast<std::size_t>(count));
  std::vector<int> lies_body(static_cast<std::size_t>(count), -1);
  std::vector<int> lies_face(static_cast<std::size_t>(count), -1);
  const std::vector<Twist> twists = twists_of(making, made.node_at, length);
  int span = 0;
  for (int i = 0; i < count; ++i) {
    while (span + 2 < isize(made.node_at) && el(at, i) > el(made.node_at, span + 1)) {
      ++span;
    }
    const bool from_port = (making.first_direction && el(at, i) < making.lead) ||
                           (making.last_direction && length - el(at, i) < making.lead);
    el(exempt, i) = static_cast<char>(from_port);
    const FlexNearest hit = mesh.nearest(el(points, i));
    if (hit.triangle < 0) {
      continue;
    }
    const detail::FlexTriangle& triangle = mesh.triangle(hit.triangle);
    const bool inside = hit.inside && hit.distance > 1.0e-6;
    el(gap, i) = inside ? 0.0 : hit.distance;
    const double need =
        rest_of(making, twist_at(twists, el(at, i))) + (triangle.curved ? 2.0 * making.deflection : 0.0);
    // (Turned towards the surface also where the item is lifted a little
    // off it, by a turn nearby or over an edge.)
    if (el(span_surface, span) != 0 && !inside && el(exempt, i) == 0 &&
        hit.distance <= need + std::max(touch, lift_slack(making))) {
      if (hit.distance <= need + touch) {
        el(on_surface, i) = 1;
        el(lies_body, i) = triangle.body;
        el(lies_face, i) = triangle.face;
      }
      el(anchored, i) = 1;
      // (Over a curved face against the normal that turns evenly over its
      // triangles; else towards the nearest point, which turns round an
      // edge the item bends over.)
      el(anchor, i) = triangle.curved            ? mesh.smooth_normal(hit.triangle, hit.point).Reversed()
                      : hit.distance > 1.0e-9 ? (hit.point - el(points, i)) / hit.distance
                                              : triangle.normal.Reversed();
    }
  }
  if (making.first_down) {
    el(anchored, 0) = 1;
    el(anchor, 0) = *making.first_down;
  }
  if (making.last_down) {
    el(anchored, count - 1) = 1;
    el(anchor, count - 1) = *making.last_down;
  }
  const std::vector<gp_XYZ> down = frames_along(points, tangents, at, anchored, anchor, twists);
  made.samples.resize(static_cast<std::size_t>(count));
  for (int i = 0; i < count; ++i) {
    FlexSample& sample = el(made.samples, i);
    sample.point = gp_Pnt(el(points, i));
    sample.tangent = el(tangents, i).Modulus() > 0.5 ? gp_Dir(el(tangents, i)) : gp_Dir(1.0, 0.0, 0.0);
    sample.down = gp_Dir(el(down, i));
    sample.at = el(at, i);
    sample.on_surface = el(on_surface, i) != 0;
    sample.body = el(lies_body, i);
    sample.face = el(lies_face, i);
  }
  for (int k = 0; k < isize(node_sample); ++k) {
    el(made.samples, el(node_sample, k)).node = k;
  }
  phases.mark("frames");

  // The checks. No way round the bodies:
  {
    std::vector<double> bad(static_cast<std::size_t>(count), 0.0);
    span = 0;
    for (int i = 0; i < count; ++i) {
      while (span + 2 < isize(made.node_at) && el(at, i) > el(made.node_at, span + 1)) {
        ++span;
      }
      el(bad, i) = el(span_blocked, span) != 0 ? 1.0 : 0.0;
    }
    add_problem(made, FlexProblemKind::NoCleanRoute, bad);
  }
  // The bends: by how much each is tighter than allowed, by the circle
  // through each point and its neighbours two points away (as the bends
  // were made: the spline's own curvature at one point, where it swings
  // through a node, is no bend of the item). The two points at each end,
  // which have no neighbour two points away on one side, are measured as
  // the nearest point that has, in its frame: neither the spline's own
  // curvature at its end nor a lopsided circle there (two steps on one
  // side, one on the other) is a bend of the item, nor the end's own frame,
  // set to the surface rather than carried along; where the route curls
  // onto a drum at its end either leans its flat bend a degree or two over
  // the edge, which a flat cable that may not bend over its edge at all (no
  // tighter than a hundred times its width) was warned of over the last
  // 0.4 mm on one platform's rounding and not on the others' (0.65.0, arm64).
  // (Two points either side where there are; on a route of three or four
  // points, one.)
  const int stencil = std::min(2, (count - 1) / 2);
  const auto measured = [&](int i) { return stencil == 0 ? i : std::clamp(i, stencil, count - 1 - stencil); };
  {
    for (int i = 0; i < count; ++i) {
      if (stencil == 0 || measured(i) != i) {
        el(bends, i) = gp_XYZ(0.0, 0.0, 0.0);
        continue;
      }
      const gp_XYZ a = el(points, i - stencil) - el(points, i);
      const gp_XYZ c = el(points, i + stencil) - el(points, i);
      const gp_XYZ normal = a.Crossed(c);
      const double area = normal.SquareModulus();
      if (area < 1.0e-24) {
        el(bends, i) = gp_XYZ(0.0, 0.0, 0.0);
        continue;
      }
      // (The circle's centre from the point; the curvature is towards it.)
      const gp_XYZ centre = (c * a.SquareModulus() - a * c.SquareModulus()).Crossed(normal) / (2.0 * area);
      const double radius2 = centre.SquareModulus();
      el(bends, i) = radius2 > 1.0e-24 ? centre / radius2 : gp_XYZ(0.0, 0.0, 0.0);
    }
    // (Over the flat side, and of a section that is not round over its
    // edge, each its own problem: a flat cable's warning says which way it
    // bends too tightly, mitcad#1713.)
    std::vector<double> bad(static_cast<std::size_t>(count), 0.0);
    std::vector<double> bad_edge(static_cast<std::size_t>(count), 0.0);
    // (Not allowed to bend a way: no tighter than a hundred times the
    // section's size.)
    const double hardly = 100.0 * std::max(making.width, making.height);
    for (int i = 0; i < count; ++i) {
      const int m = measured(i);
      double over = 0.0;
      double over_edge = 0.0;
      if (making.round) {
        if (limits.bend_radius > 0.0) {
          over = el(bends, m).Modulus() * limits.bend_radius;
        }
      } else {
        const gp_XYZ across = el(tangents, m).Crossed(el(down, m));
        const double flat = std::abs(el(bends, m).Dot(el(down, m)));
        const double edge = std::abs(el(bends, m).Dot(across));
        if (!limits.flat_bend) {
          over = flat * hardly;
        } else if (limits.bend_radius > 0.0) {
          over = flat * limits.bend_radius;
        }
        if (!limits.edge_bend) {
          over_edge = edge * hardly;
        } else if (limits.edge_bend_radius > 0.0) {
          over_edge = edge * limits.edge_bend_radius;
        }
      }
      el(bad, i) = over > kBendSlack ? over : 0.0;
      el(bad_edge, i) = over_edge > kBendSlack ? over_edge : 0.0;
    }
    add_problem(made, FlexProblemKind::BendTooTight, bad);
    add_problem(made, FlexProblemKind::EdgeBendTooTight, bad_edge);
  }
  // The twist: the turn of `down` about the route between two points,
  // beyond what the route's own run turns it.
  if (!making.round && limits.max_twist > 0.0) {
    std::vector<double> bad(static_cast<std::size_t>(count), 0.0);
    for (int i = 0; i + 1 < count; ++i) {
      const double step = el(at, i + 1) - el(at, i);
      if (step < 1.0e-12) {
        continue;
      }
      // (The frame of i carried to i + 1: frames_along's, of the two
      // points alone.)
      std::vector<char> pair_anchored{1, 0};
      const std::vector<gp_XYZ> onward = frames_along({el(points, i), el(points, i + 1)},
                                                    {el(tangents, i), el(tangents, i + 1)}, {0.0, step},
                                                    pair_anchored, {el(down, i), el(down, i)}, {});
      const double rate = std::abs(angle_about(onward.back(), el(down, i + 1), el(tangents, i + 1))) / step;
      if (rate > 1.1 * limits.max_twist) {
        el(bad, i) = rate / limits.max_twist;
        el(bad, i + 1) = std::max(el(bad, i + 1), rate / limits.max_twist);
      }
    }
    add_problem(made, FlexProblemKind::TwistOverLimit, bad);
  }
  phases.mark("bends");
  // The section against the bodies: points of its outline, drawn in by a
  // thousandth of a millimetre (any reach into a body above that is one,
  // mitcad#1713: a wire 0.085 mm into a block was not, while the outline
  // was drawn in by what counts as touching, 0.2 mm), and of a round
  // section the point of its outline towards the nearest body. Against a
  // planar face the triangles are the face; near a curved one the body
  // itself decides.
  std::vector<double> bad(static_cast<std::size_t>(count), 0.0);
  std::vector<int> bad_body(static_cast<std::size_t>(count), -1);
  if (!mesh.empty()) {
    constexpr double kReachesIn = 1.0e-3;
    const std::vector<std::array<double, 2>> outline = outline_points(making, kReachesIn);
    const double reach = making.round ? making.width / 2.0 : std::hypot(making.width, making.height) / 2.0;
    using Classifiers = std::vector<std::unique_ptr<BRepClass3d_SolidClassifier>>;
    const auto in_body = [&](Classifiers& classifiers, int body, const gp_XYZ& point, bool by_mesh) {
      try {
        std::unique_ptr<BRepClass3d_SolidClassifier>& classifier = el(classifiers, body);
        if (!classifier) {
          classifier = std::make_unique<BRepClass3d_SolidClassifier>(mesh.obstacle(body));
        }
        classifier->Perform(gp_Pnt(point), 1.0e-7);
        return classifier->State() == TopAbs_IN;
      } catch (const Standard_Failure&) {
        return by_mesh;
      }
    };
    const auto check = [&](int i, Classifiers& classifiers) {
      // (The gap is to the triangles: a curved face lies up to the
      // deflection nearer.)
      if (el(exempt, i) != 0 || el(gap, i) >= reach + making.deflection + kReachesIn) {
        return;
      }
      const gp_XYZ across = el(tangents, i).Crossed(el(down, i));
      std::vector<gp_XYZ> checked;
      for (const std::array<double, 2>& uv : outline) {
        checked.push_back(el(points, i) + across * uv[0] - el(down, i) * uv[1]);
      }
      if (making.round) {
        const FlexNearest centre = mesh.nearest(el(points, i));
        if (centre.triangle >= 0) {
          const gp_XYZ towards = centre.point - el(points, i);
          const double distance = towards.Modulus();
          const gp_XYZ way = centre.inside || distance < 1.0e-9 ? mesh.triangle(centre.triangle).normal.Reversed()
                                                                : towards / distance;
          checked.push_back(el(points, i) + way * (making.width / 2.0 - kReachesIn));
        }
      }
      for (const gp_XYZ& point : checked) {
        const FlexNearest hit = mesh.nearest(point);
        if (hit.triangle < 0) {
          continue;
        }
        const detail::FlexTriangle& triangle = mesh.triangle(hit.triangle);
        const bool by_mesh = hit.inside && hit.distance > 1.0e-6;
        bool reaches_in = by_mesh;
        // (Within the deflection of a curved face's triangles, either side:
        // farther in, the point is in the body, farther out, not.)
        if (triangle.curved && hit.distance < 1.5 * making.deflection) {
          reaches_in = in_body(classifiers, triangle.body, point, by_mesh);
        }
        if (reaches_in) {
          el(bad, i) = std::max(el(bad, i), 1.0 + (by_mesh ? hit.distance : 0.0));
          el(bad_body, i) = triangle.body;
        }
      }
    };
    // On all cores, every chunk-th sample in one, each with classifiers of
    // its own: most of a flat cable's route along a swept wire was here
    // (0.8 of 1.3 s on the build VM, mitcad#1713). The same result.
    const int chunks = std::clamp(OSD_Parallel::NbLogicalProcessors(), 1, 16);
    std::vector<std::exception_ptr> errors(static_cast<std::size_t>(chunks));
    OSD_Parallel::For(
        0, chunks,
        [&](int chunk) {
          try {
            Classifiers classifiers(static_cast<std::size_t>(mesh.body_count()));
            for (int i = chunk; i < count; i += chunks) {
              check(i, classifiers);
            }
          } catch (...) {
            el(errors, chunk) = std::current_exception();
          }
        },
        chunks < 2 || count < 64);
    for (const std::exception_ptr& error : errors) {
      if (error) {
        std::rethrow_exception(error);
      }
    }
  }
  phases.mark("section");
  // The item against its own run: two stretches more than three of its
  // thicknesses apart along it may touch, not reach into each other.
  {
    const double thick = self_thickness(making);
    for (int i = 0; i + 1 < count; ++i) {
      for (int j = i + 2; j + 1 < count; ++j) {
        if (el(at, j) - el(at, i + 1) < 3.0 * thick) {
          continue;
        }
        const double roughly = (el(points, i) - el(points, j)).Modulus();
        if (roughly > thick + (el(at, i + 1) - el(at, i)) + (el(at, j + 1) - el(at, j))) {
          continue;
        }
        const double distance =
            segment_gap(el(points, i), el(points, i + 1), el(points, j), el(points, j + 1)).distance;
        if (distance < thick - touch) {
          const double depth = 1.0 + thick - distance;
          for (const int k : {i, i + 1, j, j + 1}) {
            if (depth > el(bad, k)) {
              el(bad, k) = depth;
              el(bad_body, k) = -1;
            }
          }
        }
      }
    }
  }
  add_problem(made, FlexProblemKind::PassesThrough, bad, &bad_body);
  phases.mark("own-run");
  return made;
}

namespace {

// A rectangle about the origin, its corners rounded, counter-clockwise from
// its lower side: its four sides named <lines><first_line>... and the
// corner after each <arcs><first_arc>... (a side or a corner without a
// length is left out; the others keep their numbers).
Loop rounded_rectangle(double width, double height, double radius, const std::string& lines, int first_line,
                       const std::string& arcs, int first_arc) {
  const double hw = width / 2.0;
  const double hh = height / 2.0;
  const double r = std::clamp(radius, 0.0, std::min(hw, hh));
  Loop loop;
  int line_name = first_line;
  int arc_name = first_arc;
  const auto line = [&](double x1, double y1, double x2, double y2) {
    const int number = line_name++;
    if (std::hypot(x2 - x1, y2 - y1) < 1.0e-9) {
      return;
    }
    Segment segment;
    segment.kind = SegmentKind::Line;
    segment.start = gp_Pnt2d(x1, y1);
    segment.end = gp_Pnt2d(x2, y2);
    segment.name = lines + std::to_string(number);
    loop.segments.push_back(segment);
  };
  const auto arc = [&](double cx, double cy, double from) {
    const int number = arc_name++;
    if (r < 1.0e-9) {
      return;
    }
    Segment segment;
    segment.kind = SegmentKind::Arc;
    segment.center = gp_Pnt2d(cx, cy);
    segment.radius = r;
    segment.start_angle = from;
    segment.end_angle = from + kPi / 2.0;
    segment.name = arcs + std::to_string(number);
    loop.segments.push_back(segment);
  };
  line(-hw + r, -hh, hw - r, -hh);
  arc(hw - r, -hh + r, -kPi / 2.0);
  line(hw, -hh + r, hw, hh - r);
  arc(hw - r, hh - r, 0.0);
  line(hw - r, hh, -hw + r, hh);
  arc(-hw + r, hh - r, kPi / 2.0);
  line(-hw, hh - r, -hw, -hh + r);
  arc(-hw + r, -hh + r, kPi);
  return loop;
}

// The face of a profile's outline with a wall's hole: the outline's face
// less the face within its offset inwards.
TopoDS_Face walled_profile(const Frame& frame, const Region& region, double wall) {
  const ProfileFace outer = make_profile(frame, region);
  BRepOffsetAPI_MakeOffset offset(outer.face, GeomAbs_Arc);
  offset.Perform(-wall);
  TopoDS_Shape inner;
  if (offset.IsDone() && !offset.Shape().IsNull()) {
    for (TopExp_Explorer w(offset.Shape(), TopAbs_WIRE); w.More() && inner.IsNull(); w.Next()) {
      BRepBuilderAPI_MakeFace bore(TopoDS::Wire(w.Current()), true);
      if (bore.IsDone()) {
        inner = bore.Face();
      }
    }
  }
  if (inner.IsNull()) {
    throw std::invalid_argument("the wall is thicker than the section allows");
  }
  const TopoDS_Shape ring = BRepAlgoAPI_Cut(outer.face, inner).Shape();
  const TopExp_Explorer f(ring, TopAbs_FACE);
  if (!f.More()) {
    throw std::invalid_argument("the wall is thicker than the section allows");
  }
  return TopoDS::Face(f.Current());
}

// A sweep's face name as a Flex item has it: <feature>:start and
// <feature>:end for the caps, and for a side the name its segment was
// given (side0, corner1, inner2; the number alone, of a walled profile's
// edge, as side<number>), pieces keeping what follows.
std::string plain_name(const std::string& name, const std::string& feature) {
  const std::string prefix = feature + ":";
  if (name.compare(0, prefix.size(), prefix) != 0) {
    return name;
  }
  const std::string rest = name.substr(prefix.size());
  const std::size_t open = rest.find('(');
  const std::size_t close = open == std::string::npos ? std::string::npos : rest.find(')', open);
  if (close == std::string::npos) {
    return name;
  }
  const std::string role = rest.substr(0, open);
  const std::string argument = rest.substr(open + 1, close - open - 1);
  const std::string tail = rest.substr(close + 1);
  if (role == "start" || role == "end") {
    return prefix + role + tail;
  }
  if (role != "side") {
    return name;
  }
  const bool given = argument.compare(0, 4, "side") == 0 || argument.compare(0, 6, "corner") == 0 ||
                     argument.compare(0, 5, "inner") == 0;
  return prefix + (given ? argument : "side" + argument) + tail;
}

// The swept solid with its faces named so.
ShapePtr plainly_named(const Shape& shape, const std::string& feature) {
  std::vector<Shape::NamedFace> faces;
  for (int i = 0; i < shape.face_count(); ++i) {
    NameList names;
    for (const std::string& name : shape.face_names(i)) {
      names.push_back(plain_name(name, feature));
    }
    if (!names.empty()) {
      faces.push_back(Shape::NamedFace{shape.face(i), names});
    }
  }
  ShapePtr out = std::make_shared<Shape>(shape.occt(), faces);
  out->copy_notes(shape);
  return out;
}

// The item's solid, or the failure: never a shape without a solid, or
// without volume (a hollow item whose bore was cut out of nothing), or with
// less than `least` (a valid solid along only a part of the route).
ShapePtr whole_solid(ShapePtr shape, double least = 0.0) {
  int solids = 0;
  for (TopExp_Explorer s(shape->occt(), TopAbs_SOLID); s.More(); s.Next()) {
    ++solids;
  }
  GProp_GProps props;
  if (solids > 0) {
    BRepGProp::VolumeProperties(shape->occt(), props);
  }
  if (solids == 0 || !(props.Mass() > 0.0) || props.Mass() < least) {
    throw std::runtime_error("the item's solid could not be built along its route");
  }
  return shape;
}

// The area of a round or rectangular section (its wall's, when hollow); 0
// for an outline, whose area is not worked out here.
double section_area(const FlexSection& section, double wall) {
  const auto rounded = [](double width, double height, double corner) {
    const double r = std::clamp(corner, 0.0, 0.5 * std::min(width, height));
    return width * height - (4.0 - kPi) * r * r;
  };
  if (section.round()) {
    const double r = 0.5 * section.width;
    const double bore = wall > 0.0 ? std::max(r - wall, 0.0) : 0.0;
    return kPi * (r * r - bore * bore);
  }
  if (section.kind != FlexSectionKind::Rectangular) {
    return 0.0;
  }
  double area = rounded(section.width, section.height, section.corner_radius);
  if (wall > 0.0) {
    area -= rounded(std::max(section.width - 2.0 * wall, 0.0), std::max(section.height - 2.0 * wall, 0.0),
                    std::max(section.corner_radius - wall, 0.0));
  }
  return area;
}

// The length of the polyline through the samples' points.
double samples_length(const std::vector<FlexSample>& samples) {
  double length = 0.0;
  for (std::size_t i = 1; i < samples.size(); ++i) {
    length += samples[i].point.Distance(samples[i - 1].point);
  }
  return length;
}

// The route's samples with its bends eased where they are tighter than the
// section can be swept round, or nullopt where none is (mitcad#1713: a
// flat cable 10 mm wide whose route turned over its edge at a node on a
// thin wire, 0.25 mm round, and was not swept: "the profile could not be
// swept along the path"). Each bend at least half the section's extent in
// the plane it bends in (a round section's half diameter): the points
// there, and those within the section's size of them, drawn a little
// towards their neighbours' middle, and the sides' way (`down`) with them,
// until it is. The ends stay; the route's own check has said where it
// bends too tightly, so the solid, not the route, gives way there.
std::optional<std::vector<FlexSample>> eased_bends(const std::vector<FlexSample>& samples, double width,
                                                   double height, bool round) {
  const int count = isize(samples);
  if (count < 3) {
    return std::nullopt;
  }
  std::vector<gp_XYZ> p;
  std::vector<gp_XYZ> d;
  double length = 0.0;
  for (int i = 0; i < count; ++i) {
    p.push_back(el(samples, i).point.XYZ());
    d.push_back(el(samples, i).down.XYZ());
    if (i > 0) {
      length += (el(p, i) - el(p, i - 1)).Modulus();
    }
  }
  const double spacing = length / (count - 1);
  if (!(spacing > 1.0e-9)) {
    return std::nullopt;
  }
  const int reach = static_cast<int>(std::ceil(std::max(width, height) / spacing)) + 1;
  // The radius the section needs in a bend about `axis` at point i.
  const auto needed = [&](int i, const gp_XYZ& axis) {
    if (round) {
      return 0.5 * width;
    }
    const gp_XYZ& t = el(samples, i).tangent.XYZ();
    const gp_XYZ& down = el(samples, i).down.XYZ();
    const gp_XYZ x = t.Crossed(down);
    return 0.5 * (std::abs(axis.Dot(down)) * width + std::abs(axis.Dot(x)) * height);
  };
  bool eased = false;
  constexpr int kPasses = 4000;
  std::vector<char> tight(static_cast<std::size_t>(count), 0);
  for (int pass = 0; pass < kPasses; ++pass) {
    if (pass % 64 == 0) {
      throw_if_cancelled();
    }
    std::fill(tight.begin(), tight.end(), 0);
    bool found = false;
    for (int i = 1; i + 1 < count; ++i) {
      const gp_XYZ a = el(p, i) - el(p, i - 1);
      const gp_XYZ b = el(p, i + 1) - el(p, i);
      const gp_XYZ cross = a.Crossed(b);
      const double twice = cross.Modulus();
      if (twice < 1.0e-12) {
        continue;
      }
      const double radius = a.Modulus() * b.Modulus() * (el(p, i + 1) - el(p, i - 1)).Modulus() / (2.0 * twice);
      if (radius < 0.98 * needed(i, cross / twice)) {
        found = true;
        for (int k = std::max(1, i - reach); k <= std::min(count - 2, i + reach); ++k) {
          el(tight, k) = 1;
        }
      }
    }
    if (!found) {
      break;
    }
    eased = true;
    std::vector<gp_XYZ> p2 = p;
    std::vector<gp_XYZ> d2 = d;
    for (int i = 1; i + 1 < count; ++i) {
      if (el(tight, i) != 0) {
        el(p2, i) = el(p, i) + ((el(p, i - 1) + el(p, i + 1)) * 0.5 - el(p, i)) * 0.5;
        el(d2, i) = el(d, i) + ((el(d, i - 1) + el(d, i + 1)) * 0.5 - el(d, i)) * 0.5;
      }
    }
    p = std::move(p2);
    d = std::move(d2);
  }
  if (!eased) {
    return std::nullopt;
  }
  std::vector<FlexSample> out = samples;
  double at = 0.0;
  for (int i = 0; i < count; ++i) {
    FlexSample& sample = el(out, i);
    sample.point = gp_Pnt(el(p, i));
    if (i > 0) {
      at += (el(p, i) - el(p, i - 1)).Modulus();
    }
    sample.at = at;
    if (i > 0 && i + 1 < count) {
      const gp_XYZ along = el(p, i + 1) - el(p, i - 1);
      if (along.Modulus() > 1.0e-12) {
        sample.tangent = gp_Dir(along);
      }
    } else {
      // An end keeps its way unless that turns back against the eased
      // route there (a route that ran past its last node on a thin wire
      // and came back, mitcad#1713): the path through the points would
      // start or end with that turn, which the section cannot follow.
      const gp_XYZ along = i == 0 ? el(p, 1) - el(p, 0) : el(p, i) - el(p, i - 1);
      if (along.Modulus() > 1.0e-12 && sample.tangent.XYZ().Dot(along) <= 0.0) {
        sample.tangent = gp_Dir(along);
      }
    }
    const gp_XYZ t = sample.tangent.XYZ();
    gp_XYZ down = el(d, i) - t * el(d, i).Dot(t);
    if (down.Modulus() < 1.0e-9 && i > 0) {
      const gp_XYZ before = el(out, i - 1).down.XYZ();
      down = before - t * before.Dot(t);
    }
    if (down.Modulus() > 1.0e-9) {
      sample.down = gp_Dir(down);
    }
  }
  return out;
}

// The route's samples without the places where it turns back on itself, or
// nullopt where it does not (mitcad#1713: a node added at the route's own
// start, beside a first node on a profile's end edge; the route ran 0.15 mm
// out and back to the same point, no section can be swept round that, and
// the item was lost with "the profile could not be swept along the path").
// A point where the route turns by a right angle or more from one sample to
// the next is dropped, and a point at the one before it, until none is
// left. The ends stay; a way (tangent) that runs against the route left is
// turned to it, and the sides' way (`down`) with it.
std::optional<std::vector<FlexSample>> without_reversals(const std::vector<FlexSample>& samples) {
  constexpr double kSame = 1.0e-7;
  std::vector<FlexSample> out;
  bool dropped = false;
  for (const FlexSample& sample : samples) {
    if (!out.empty() && (sample.point.XYZ() - out.back().point.XYZ()).Modulus() < kSame) {
      continue; // (a point twice is no turn: the sweeps skip it themselves)
    }
    out.push_back(sample);
    // Turned back at the point before: that point dropped, and this one too
    // where it then lies at the one before it; again with the points left.
    while (out.size() >= 3) {
      const int n = isize(out);
      const gp_XYZ a = el(out, n - 2).point.XYZ() - el(out, n - 3).point.XYZ();
      const gp_XYZ b = el(out, n - 1).point.XYZ() - el(out, n - 2).point.XYZ();
      if (a.Dot(b) > 0.0) {
        break;
      }
      out.erase(out.end() - 2);
      dropped = true;
      if ((out.back().point.XYZ() - el(out, isize(out) - 2).point.XYZ()).Modulus() < kSame) {
        out.pop_back();
      }
    }
  }
  if (!dropped || out.size() < 2) {
    return std::nullopt;
  }
  const int count = isize(out);
  double at = 0.0;
  for (int i = 0; i < count; ++i) {
    FlexSample& sample = el(out, i);
    if (i > 0) {
      at += (sample.point.XYZ() - el(out, i - 1).point.XYZ()).Modulus();
    }
    sample.at = at;
    const gp_XYZ along = i == 0           ? el(out, 1).point.XYZ() - sample.point.XYZ()
                         : i + 1 == count ? sample.point.XYZ() - el(out, i - 1).point.XYZ()
                                          : el(out, i + 1).point.XYZ() - el(out, i - 1).point.XYZ();
    if (along.Modulus() > 1.0e-12 && sample.tangent.XYZ().Dot(along) <= 0.0) {
      sample.tangent = gp_Dir(along);
      const gp_XYZ t = sample.tangent.XYZ();
      const gp_XYZ down = sample.down.XYZ() - t * sample.down.XYZ().Dot(t);
      sample.down = down.Modulus() > 1.0e-9 ? gp_Dir(down) : detail::any_normal(sample.tangent);
    }
  }
  return out;
}

// The last way to show an item whose solid could not be swept along its
// route, which is never lost for it (mitcad#1713): straight pieces of its
// section between points of the route about half the section's size apart
// (a round one's joined by balls), as one compound, with a note at the
// route's tightest place. Its sides are <feature>:side0, its ends
// <feature>:start and <feature>:end; a wall is not cut out of it.
ShapePtr rough_solid(const std::string& feature, const std::vector<FlexSample>& samples, const FlexSection& section) {
  const int count = isize(samples);
  if (count < 2) {
    throw std::invalid_argument("the route is empty");
  }
  const bool round = section.round();
  const double width = section.width;
  const double height = round ? section.width : section.height;
  double length = 0.0;
  for (int i = 1; i < count; ++i) {
    length += el(samples, i).point.Distance(el(samples, i - 1).point);
  }
  constexpr int kMostPieces = 400;
  const double step = std::max(0.5 * std::max(width, height), length / kMostPieces);
  std::vector<int> kept{0};
  for (int i = 1; i + 1 < count; ++i) {
    if (el(samples, i).point.Distance(el(samples, kept.back()).point) >= step) {
      kept.push_back(i);
    }
  }
  if (el(samples, count - 1).point.Distance(el(samples, kept.back()).point) > 1.0e-6) {
    kept.push_back(count - 1);
  } else if (kept.size() > 1) {
    kept.back() = count - 1;
  }
  if (kept.size() < 2) {
    throw std::runtime_error("the item's solid could not be built along its route");
  }

  const std::string side = feature + ":side0";
  const std::string start = feature + ":start";
  const std::string end = feature + ":end";
  std::vector<TopoDS_Shape> pieces;
  std::vector<Shape::NamedFace> faces;
  const auto add_piece = [&](const TopoDS_Shape& piece, const gp_Pnt* start_at, const gp_Pnt* end_at) {
    pieces.push_back(piece);
    for (TopExp_Explorer f(piece, TopAbs_FACE); f.More(); f.Next()) {
      std::string name = side;
      if (start_at != nullptr || end_at != nullptr) {
        GProp_GProps props;
        BRepGProp::SurfaceProperties(f.Current(), props);
        const gp_Pnt centre = props.CentreOfMass();
        if (start_at != nullptr && centre.Distance(*start_at) < 1.0e-6) {
          name = start;
        } else if (end_at != nullptr && centre.Distance(*end_at) < 1.0e-6) {
          name = end;
        }
      }
      faces.push_back(Shape::NamedFace{TopoDS::Face(f.Current()), {name}});
    }
  };
  detail::run("flex item in pieces", [&] {
    for (int k = 0; k + 1 < isize(kept); ++k) {
      const FlexSample& a = el(samples, el(kept, k));
      const FlexSample& b = el(samples, el(kept, k + 1));
      const gp_Vec run(a.point, b.point);
      const double piece_length = run.Magnitude();
      if (!(piece_length > 1.0e-9)) {
        continue;
      }
      const gp_Dir dir(run);
      const bool first = k == 0;
      const bool last = k + 2 == isize(kept);
      if (round) {
        add_piece(BRepPrimAPI_MakeCylinder(gp_Ax2(a.point, dir), 0.5 * width, piece_length).Shape(),
                  first ? &a.point : nullptr, last ? &b.point : nullptr);
        if (!last) {
          add_piece(BRepPrimAPI_MakeSphere(b.point, 0.5 * width).Shape(), nullptr, nullptr);
        }
      } else {
        // u across (tangent x down), v against `down`, as the sweep has them.
        const gp_XYZ t = dir.XYZ();
        gp_XYZ down = a.down.XYZ() - t * a.down.XYZ().Dot(t);
        const gp_Dir d = down.Modulus() > 1.0e-9 ? gp_Dir(down) : detail::any_normal(dir);
        const gp_Dir x(t.Crossed(d.XYZ()));
        const gp_Pnt origin(a.point.XYZ() - x.XYZ() * (0.5 * width) + d.XYZ() * (0.5 * height));
        add_piece(BRepPrimAPI_MakeBox(gp_Ax2(origin, dir, x), width, height, piece_length).Shape(),
                  first ? &a.point : nullptr, last ? &b.point : nullptr);
      }
    }
  });
  if (pieces.empty()) {
    throw std::runtime_error("the item's solid could not be built along its route");
  }
  ShapePtr shape = std::make_shared<Shape>(detail::compound_of(pieces), faces);
  // Where: the route's tightest place (by the circle through each point
  // and its neighbours), else its start.
  gp_Pnt where = el(samples, 0).point;
  double tightest = kStraight;
  for (int i = 1; i + 1 < count; ++i) {
    const gp_XYZ p = el(samples, i - 1).point.XYZ();
    const gp_XYZ q = el(samples, i).point.XYZ();
    const gp_XYZ r = el(samples, i + 1).point.XYZ();
    const double twice = (q - p).Crossed(r - q).Modulus();
    const double radius = twice > 1.0e-12 ? (q - p).Modulus() * (r - q).Modulus() * (r - p).Modulus() / (2.0 * twice)
                                          : ((q - p).Dot(r - q) < 0.0 ? 0.0 : kStraight);
    if (radius < tightest) {
      tightest = radius;
      where = el(samples, i).point;
    }
  }
  shape->add_note("the item is shown in straight pieces here: its solid could not be made smooth along the route",
                  {Shape::NotePlace::at(where.X(), where.Y(), where.Z())});
  return whole_solid(shape);
}

} // namespace

ShapePtr build_solid(const std::string& feature, const FlexRoute& route, const FlexSection& section) {
  return build_solid(feature, Path{PathCurve{"route", route.curve}}, route.samples, section);
}

ShapePtr build_solid(const std::string& feature, const Path& path, const std::vector<FlexSample>& samples,
                     const FlexSection& section) {
  detail::require_positive("the section's width", section.width);
  detail::require_finite("the wall", section.wall);
  if (samples.size() < 2) {
    throw std::invalid_argument("the route is empty");
  }
  // A route that turns back on itself (two nodes at one place on it): its
  // solid only along the route without the points where it does
  // (without_reversals). Along the route as it is, pipe() and the sweep can
  // both give a valid solid that is wrong (a 60 mm hose of 2.6 % of its
  // volume, mitcad#1713), so neither is tried there.
  const std::optional<std::vector<FlexSample>> cleaned = without_reversals(samples);
  // Each way below gives a solid of at least half the section's area along
  // its route, or is a failure (pipe() and the sweep can give a valid
  // solid along only a part of it).
  const double swept_area = section_area(section, std::max(section.wall, 0.0));
  const auto least_volume = [&](const std::vector<FlexSample>& along) {
    return 0.5 * swept_area * samples_length(along);
  };
  // A round section along a route in one plane as pipe() builds it. Along
  // any other route pipe()'s own frame (OCCT's corrected Frenet frame) can
  // take minutes on a route's spline: then as the other sections, along
  // the route's frames.
  const bool piped =
      !cleaned && section.round() &&
      detail::run("flex route", [&] { return detail::Spine(detail::path_wire(path)).plane_normal().has_value(); });
  if (piped) {
    PipeSpec spec;
    spec.feature = feature;
    spec.path = path;
    spec.size = section.width;
    spec.thickness = std::max(section.wall, 0.0);
    try {
      return whole_solid(pipe(spec), least_volume(samples));
    } catch (const Cancelled&) {
      throw;
    } catch (const std::exception&) {
      // (Then as the other sections, below, with their ways round a
      // failure: an item is never lost for its solid, mitcad#1713.)
    }
  }
  if (section.round() && section.wall > 0.0) {
    if (section.wall >= section.width / 2.0) {
      throw std::invalid_argument("the pipe's wall must be thinner than " + std::to_string(section.width / 2.0));
    }
  }
  if (!section.round()) {
    detail::require_positive("the section's height", section.height);
  }
  const double wall = std::max(section.wall, 0.0);
  if (section.kind == FlexSectionKind::Rectangular && wall >= std::min(section.width, section.height) / 2.0 &&
      wall > 0.0) {
    throw std::invalid_argument("the wall is thicker than the section allows");
  }
  // A route that runs back along itself for more than a blip (more than
  // twice the section's size or 2 % of its length lost without its turns,
  // mitcad#1713: each point of the way back takes away one of the way out,
  // and a locked hanging item kept a stub of its solid): in straight pieces
  // along the whole route, never along a part of it.
  const double extent = std::max(section.width, section.round() ? section.width : section.height);
  const double route_length = samples_length(samples);
  if (cleaned && route_length - samples_length(*cleaned) > std::max(2.0 * extent, 0.02 * route_length)) {
    return rough_solid(feature, samples, section);
  }

  // The section at the route's start: u across, v against `down`.
  const FlexSample& first = samples.front();
  SweepSpec spec;
  spec.feature = feature;
  spec.frame.origin = first.point;
  spec.frame.x_axis = first.tangent.Crossed(first.down);
  spec.frame.y_axis = first.down.Reversed();
  spec.path = path;
  if (section.round()) {
    // (Its side side0 and its bore inner0, as pipe() names them.)
    Region region = circle_region(0.0, 0.0, section.width / 2.0);
    region.name = "r1";
    region.loops.front().segments.front().name = "side0";
    if (wall > 0.0) {
      Region bore = circle_region(0.0, 0.0, section.width / 2.0 - wall);
      bore.loops.front().segments.front().name = "inner0";
      region.loops.push_back(bore.loops.front());
    }
    spec.regions = {region};
  } else if (section.kind == FlexSectionKind::Rectangular) {
    Region region;
    region.name = "r1";
    region.loops.push_back(
        rounded_rectangle(section.width, section.height, section.corner_radius, "side", 0, "corner", 0));
    if (wall > 0.0) {
      region.loops.push_back(rounded_rectangle(section.width - 2.0 * wall, section.height - 2.0 * wall,
                                               std::max(section.corner_radius - wall, 0.0), "inner", 0, "inner", 4));
    }
    spec.regions = {region};
  } else if (wall > 0.0) {
    const TopoDS_Face ring =
        detail::run("flex section", [&] { return walled_profile(spec.frame, section.profile, wall); });
    const std::string name = face_name(feature, "section");
    spec.profile_body = std::make_shared<Shape>(ring, std::vector<Shape::NamedFace>{{ring, {name}}});
    spec.profile_face = name;
    spec.profile_key = "r1";
  } else {
    Region region = section.profile;
    if (region.name.empty()) {
      region.name = "r1";
    }
    // Its segments by their place: the outer loop's side<i>, the others'
    // inner<i>.
    int inner = 0;
    for (int l = 0; l < isize(region.loops); ++l) {
      std::vector<Segment>& segments = el(region.loops, l).segments;
      for (int k = 0; k < isize(segments); ++k) {
        el(segments, k).name = l == 0 ? "side" + std::to_string(k) : "inner" + std::to_string(inner++);
      }
    }
    spec.regions = {region};
  }

  // (The section's height across; a round section's diameter both ways.)
  const double height = section.round() ? section.width : section.height;
  const auto swept = [&](const std::vector<FlexSample>& route_samples, const Path& route_path) {
    const FlexSample& start = route_samples.front();
    spec.frame.origin = start.point;
    spec.frame.x_axis = start.tangent.Crossed(start.down);
    spec.frame.y_axis = start.down.Reversed();
    spec.path = route_path;
    // The frames as a curve beside the route, towards `down`: the sweep
    // turns the section towards it in each plane square to the route. Nearer
    // to the route than its tightest bend, so that the curve has no cusp.
    const int count = isize(route_samples);
    double tightest = kStraight;
    for (int i = 1; i + 1 < count; ++i) {
      const gp_XYZ a = el(route_samples, i - 1).point.XYZ();
      const gp_XYZ b = el(route_samples, i).point.XYZ();
      const gp_XYZ c = el(route_samples, i + 1).point.XYZ();
      const double twice = (b - a).Crossed(c - b).Modulus();
      if (twice > 1.0e-12) {
        tightest = std::min(tightest, (b - a).Modulus() * (c - b).Modulus() * (c - a).Modulus() / (2.0 * twice));
      }
    }
    const double size = std::max(section.width, height);
    const double beside = std::clamp(0.25 * tightest, 0.05 * std::min(section.width, height), size);
    // (A round section looks the same however it turns: its frames are
    // carried along the route without twisting, not turned towards the faces
    // it passes, whose sudden turns over an edge the sweep follows badly.)
    std::vector<gp_XYZ> downs;
    for (const FlexSample& sample : route_samples) {
      downs.push_back(sample.down.XYZ());
    }
    if (section.round()) {
      std::vector<gp_XYZ> along;
      std::vector<gp_XYZ> tangents;
      std::vector<double> at;
      for (const FlexSample& sample : route_samples) {
        along.push_back(sample.point.XYZ());
        tangents.push_back(sample.tangent.XYZ());
        at.push_back(sample.at);
      }
      std::vector<char> anchored(route_samples.size(), 0);
      anchored.front() = 1;
      downs = frames_along(along, tangents, at, anchored, downs, {});
    }
    // The frames' curve through every `every`-th sample (the last always).
    const auto frames_curve = [&](int every) {
      std::vector<gp_XYZ> points;
      std::vector<double> parameters;
      for (int i = 0; i < count; i += every) {
        if (i + every >= count) {
          i = count - 1;
        }
        const FlexSample& sample = el(route_samples, i);
        const gp_XYZ point = sample.point.XYZ() + el(downs, i) * beside;
        if (!points.empty() && (point - points.back()).Modulus() < 1.0e-7) {
          continue;
        }
        parameters.push_back(points.empty() ? 0.0 : parameters.back() + (point - points.back()).Modulus());
        points.push_back(point);
      }
      if (points.size() < 2) {
        throw std::invalid_argument("the route is empty");
      }
      const occ::handle<Geom_BSplineCurve> frames =
          detail::run("flex frames", [&] { return spline_through(points, parameters, std::nullopt, std::nullopt); });
      return Path{PathCurve{"frames", model_curve_of(frames)}};
    };
    // Through at most about 400 of the samples; where the sweep fails with
    // that curve, through all of them (mitcad#1713: a locked hanging cable
    // of 1.4 m and 2 400 samples, through every sixth "could not be swept
    // along the path", through every one swept in half a second; its every
    // stretch swept with the coarser curve too).
    const int every = std::max(1, count / 400);
    spec.auxiliary = frames_curve(every);
    try {
      return whole_solid(plainly_named(*sweep(spec), feature), least_volume(route_samples));
    } catch (const Cancelled&) {
      throw;
    } catch (const std::exception&) {
      if (every == 1) {
        throw;
      }
    }
    spec.auxiliary = frames_curve(1);
    return whole_solid(plainly_named(*sweep(spec), feature), least_volume(route_samples));
  };
  std::exception_ptr failed; // the first failure, along the route as it is
  if (!cleaned) {
    try {
      return swept(samples, path);
    } catch (const Cancelled&) {
      throw;
    } catch (const std::exception&) {
      failed = std::current_exception();
    }
  }
  {
    const std::string name = path.size() == 1 ? path.front().name : std::string("route");
    // The solid along a spline through the points of the samples.
    const auto through = [&](const std::vector<FlexSample>& route_samples, const char* what) {
      std::vector<gp_XYZ> points;
      std::vector<double> parameters;
      for (const FlexSample& sample : route_samples) {
        if (!points.empty() && (sample.point.XYZ() - points.back()).Modulus() < 1.0e-7) {
          continue;
        }
        parameters.push_back(points.empty() ? 0.0
                                            : parameters.back() + (sample.point.XYZ() - points.back()).Modulus());
        points.push_back(sample.point.XYZ());
      }
      const occ::handle<Geom_BSplineCurve> spine = detail::run(what, [&] {
        return spline_through(points, parameters, route_samples.front().tangent.XYZ(),
                              route_samples.back().tangent.XYZ());
      });
      return swept(route_samples, Path{PathCurve{name, model_curve_of(spine)}});
    };
    // First along the route's own samples: its curve, through fewer points,
    // can swing round a sharp turn at a node between its samples (a flat
    // cable turned over its edge at a node in the air, which it may not:
    // "the profile could not be swept along the path", mitcad#1713, where a
    // spline through its 450 samples sweeps).
    // A route that turns back on itself: without the points where it does.
    const std::vector<FlexSample>& clean = cleaned ? *cleaned : samples;
    try {
      return through(clean, "flex route through its samples");
    } catch (const Cancelled&) {
      throw;
    } catch (const std::exception&) {
      // (Eased, below.)
    }
    // A bend tighter than the section can be swept round: the solid along
    // the route eased there (eased_bends), on a path through its points.
    const std::optional<std::vector<FlexSample>> eased = eased_bends(clean, section.width, height, section.round());
    if (eased) {
      try {
        return through(*eased, "flex route eased");
      } catch (const Cancelled&) {
        throw;
      } catch (const std::exception&) {
        // (In pieces, below.)
      }
    }
    // Last, the item in straight pieces with a note, so that it is not
    // lost (rough_solid); only where even that fails, the first failure.
    try {
      return rough_solid(feature, eased ? *eased : clean, section);
    } catch (const Cancelled&) {
      throw;
    } catch (const std::exception&) {
      if (failed) {
        std::rethrow_exception(failed);
      }
      throw;
    }
  }
}

FlexPort end_port(const FlexRoute& route, bool at_start) {
  if (route.samples.empty()) {
    throw std::invalid_argument("the route is empty");
  }
  const FlexSample& sample = at_start ? route.samples.front() : route.samples.back();
  FlexPort port;
  port.position = sample.point;
  port.direction = at_start ? sample.tangent.Reversed() : sample.tangent;
  port.down = sample.down;
  return port;
}

FlexPort end_port(const Shape& body, const std::string& face) {
  const std::vector<int> found = body.find_faces(face);
  if (found.empty()) {
    throw std::invalid_argument(no_face_error(face));
  }
  return detail::run("flex port", [&] {
    const TopoDS_Face& end = body.face(found.front());
    TopLoc_Location location;
    if (BRep_Tool::Surface(end, location).IsNull() || BRepAdaptor_Surface(end).GetType() != GeomAbs_Plane) {
      throw std::invalid_argument(
          face_error(face, "the end face is not planar", "the end " + face + " is not planar"));
    }
    gp_Dir normal = BRepAdaptor_Surface(end).Plane().Axis().Direction();
    if (end.Orientation() == TopAbs_REVERSED) {
      normal.Reverse();
    }
    GProp_GProps props;
    BRepGProp::SurfaceProperties(end, props);
    FlexPort port;
    port.position = props.CentreOfMass();
    port.direction = normal;
    port.down = detail::any_normal(normal);
    double longest = 0.0;
    for (TopExp_Explorer e(end, TopAbs_EDGE); e.More(); e.Next()) {
      const BRepAdaptor_Curve edge(TopoDS::Edge(e.Current()));
      if (edge.GetType() != GeomAbs_Line) {
        continue;
      }
      const double length = std::abs(edge.LastParameter() - edge.FirstParameter());
      const gp_Lin line = edge.Line();
      const gp_XYZ origin = line.Location().XYZ();
      const gp_XYZ along = line.Direction().XYZ();
      const gp_XYZ centre = port.position.XYZ();
      gp_XYZ foot = origin + along * (centre - origin).Dot(along) - centre;
      foot = foot - normal.XYZ() * foot.Dot(normal.XYZ());
      if (length > longest + 1.0e-9 && foot.Modulus() > 1.0e-9) {
        longest = length;
        port.down = gp_Dir(foot);
      }
    }
    return port;
  });
}

} // namespace mitcad::geometry::routing
