// SPDX-License-Identifier: MIT
// A fillet or chamfer wider than a face beside its edge takes that face
// away (mitcad#1819), as one table: one row per case.
//
// The owner's order: the dressup is built on the picked edge at its full
// size, measured from that edge, with the narrow face continued along its
// own surface past its far edge; then the face beyond the narrow face is
// grown out (or cut back) along its own surface to meet the dressup's
// boundary, and the narrow face is gone. The reference of each row is
// built that way: for a convex edge the body less the part of R (what the
// dressup of the extended body takes away) on the outer side of the face
// beyond (its surface continued); for a concave edge the body with the
// part of A (what the dressup of the extended body adds) on that face's
// material side. The extended body is the body's section with the narrow
// face continued 5 past its far edge and the face beyond left out, swept
// as the body is. For a planar narrow face along a straight chain the
// reference is integrated exactly in the section (the regions as polygons,
// a rounding's arc in 4000 pieces, the inclined end face by the section's
// first moment); otherwise it is built with the geometry kernel's dressup
// of the extended body and booleans.
//
// The table's columns:
// - the narrow face (1 wide, w): planar between parallel faces (a step: a
//   riser 1 high, the face beyond parallel to the edge's other face),
//   planar between faces that are not parallel (the face beyond a slope
//   rising away from the edge, so that it meets the dressup at every size
//   of the table), a cylinder segment (an arc of radius 5 in the section),
//   a cone segment (the section's narrow face inclined 17.5 degrees: a cone
//   where the chain turns round the rounded rectangle's corners, an
//   inclined plane along straight edges) and a B-spline strip (the step
//   with every face converted to B-splines);
// - the chain: a single straight edge (20 long, ending on the body's ends),
//   a closed tangent chain over rounded corners (a rounded rectangle 60 x
//   40, corners of radius 12), a closed octagon over sharp corners (apothem
//   28) and a straight edge ending on a third face (the end cut at 30
//   degrees). The bodies are frames: the section (the edge's other face 8
//   wide) swept along the chain;
// - convex (the top's rim, the narrow face below it) and concave (a floor's
//   rim, the narrow face a wall rising from it);
// - the dressup: chamfer of equal distance, of two distances (each side the
//   long one), of a distance and an angle (30 degrees, measured on the
//   edge's other face), and a fillet of constant radius;
// - the size s, the dressup's reach along the narrow face: 0.5w, 0.99w,
//   1.0w, 1.01w, 1.5w, 3w, and one past the next face, where the dressup's
//   reach along the edge's other face is 1.25 times its width (it takes
//   that face away too). The rule decides that one: where the faces beyond,
//   grown out (the frame's inner wall too), leave none of the dressup, it
//   fails in words, saying the dressup is wider than the face beside its
//   edge, no names in what a person reads, never that the result is not a
//   closed solid; where some is left (a concave dressup, which only adds
//   material cut by the faces beyond) it is built and checked like the
//   other sizes. A chamfer of a distance and an angle has its angle at the
//   edge's other face.
// Each other row: one valid closed solid, no face under 0.01 mm2, the
// dressup's faces named <feature>:chamfer(…) or <feature>:fillet(…), the
// narrow faces gone from 1.0w on and kept below it, and its volume the
// reference's within 1e-6 relative. Below w the reference is the dressup
// as today (nothing to grow out). A fillet that the geometry kernel's own
// rounding (as patched in the port, mitcad#121) builds closed and valid
// stands as that: the ball rolls on the narrow face's far edge or runs
// onto the face beyond; its volume is then that of the kernel's rounding.
//
// A B-spline strip is built as the step it was converted from, its narrow
// faces continued as the analytic surfaces they are within 1e-4, each
// within its own part of a closed chain (mitcad#1849): its rows round the
// closed chains (a tangent chain chamfered by a distance and an angle, an
// octagon by two distances, both at 3w) are built and checked like the
// others, against the reference built in the owner's order.
//
// The chains part also has an opening's rim over rounded corners in a
// plate, chamfered 0.5 and 1 and rounded 1, and over sharp corners: no
// piece of the old top face left at a corner (mitcad#1821); and a narrow
// face of free form (no analytic surface), which the dressup cannot
// continue: a failure in words that says so where the geometry kernel
// cannot build it either (mitcad#1849).
//
// Not the whole cross product (1400 rows, far too slow): the equal chamfer
// and the fillet at every size on every narrow face (the rule itself),
// every dressup at the exact width, 1.01w and past the next face, and
// pairwise rows, so that every value of every column meets every value of
// every other column in some row (table()).
//
//   test_dressup_consumes_face [--part core|kinds|chains|ring]
// runs one of the table's three parts (or the ring's floor, mitcad#1821), all
// without --part.

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdio>
#include <cstring>
#include <memory>
#include <optional>
#include <string>
#include <vector>

#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_NurbsConvert.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepFilletAPI_MakeFillet2d.hxx>
#include <BRepGProp.hxx>
#include <BRepOffsetAPI_MakePipeShell.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeHalfSpace.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GC_MakeArcOfCircle.hxx>
#include <GProp_GProps.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <NCollection_Array1.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "../src/canonical.hpp"
#include "check.hpp"
#include "mitcad/analysis/properties.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/naming.hpp"
#include "mitcad/geometry/user_error.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

// --- The table's columns ------------------------------------------------

enum class Face { Parallel, Slope, Cylinder, Cone, Spline };
enum class Chain { Single, Tangent, Octagon, Third };
enum class Kind { Equal, TwoLongOnNarrow, TwoLongOnOther, DistanceAngle, Fillet };

const char* face_name(Face face) {
  switch (face) {
  case Face::Parallel:
    return "parallel";
  case Face::Slope:
    return "slope";
  case Face::Cylinder:
    return "cylinder";
  case Face::Cone:
    return "cone";
  case Face::Spline:
    return "spline";
  }
  return "?";
}

const char* chain_name(Chain chain) {
  switch (chain) {
  case Chain::Single:
    return "single";
  case Chain::Tangent:
    return "tangent";
  case Chain::Octagon:
    return "octagon";
  case Chain::Third:
    return "third";
  }
  return "?";
}

const char* kind_name(Kind kind) {
  switch (kind) {
  case Kind::Equal:
    return "chamfer equal";
  case Kind::TwoLongOnNarrow:
    return "chamfer two distances, long on the narrow face";
  case Kind::TwoLongOnOther:
    return "chamfer two distances, long on the other face";
  case Kind::DistanceAngle:
    return "chamfer distance and angle";
  case Kind::Fillet:
    return "fillet";
  }
  return "?";
}

constexpr double kWidth = 1.0;     // the narrow face's width, w
constexpr double kOther = 8.0;     // the edge's other face's width
constexpr double kAngle = kPi / 6; // a distance-and-angle chamfer's angle
constexpr double kTilt = 0.57735026918962576; // tan 30 degrees: the third face
constexpr double kLength = 20.0;   // a straight chain's length

// The dressup's reach along the edge's other face for each unit along the
// narrow face (a fillet: its contact, the same on both), at an edge whose
// faces meet at `angle` inside: a chamfer of a distance and an angle has
// its angle (kAngle) at the other face, so by the law of sines its legs are
// as sin(angle + kAngle) to sin kAngle (cot kAngle at a right angle).
double other_per_narrow(Kind kind, double angle) {
  switch (kind) {
  case Kind::Equal:
  case Kind::Fillet:
    return 1.0;
  case Kind::TwoLongOnNarrow:
    return 0.5;
  case Kind::TwoLongOnOther:
    return 2.0;
  case Kind::DistanceAngle:
    return std::sin(angle + kAngle) / std::sin(kAngle);
  }
  return 1.0;
}

// --- The section ----------------------------------------------------------

// A point of the section: u across the chain (out of the edge's other
// face, the edge at 0), z up.
struct P {
  double u = 0.0;
  double z = 0.0;
};

P operator+(P a, P b) { return {a.u + b.u, a.z + b.z}; }
P operator-(P a, P b) { return {a.u - b.u, a.z - b.z}; }
P operator*(double k, P a) { return {k * a.u, k * a.z}; }

// A closed outline: from each point to the next, an arc through `mid`
// where `arc` is set.
struct Step {
  P to;
  bool arc = false;
  P mid;
};
using Outline = std::vector<Step>; // the last step back to the first's start

// The section of one narrow face, convex or concave.
struct Profile {
  Outline body;
  Outline extended; // the narrow face continued 5 past its far edge, the face beyond left out
  double edge_z = 0.0;
  P corner;          // the edge
  P along;           // the narrow face's direction at the edge, away from it (unit)
  P far;             // the narrow face's far edge
  P beyond;          // the face beyond's direction from the far edge (not vertical)
  bool planar = true; // a planar narrow face: integrated exactly along straight chains
  bool convex = true;
  // The face beyond's line at u: the material lies under it.
  double beyond_at(double u) const { return far.z + (u - far.u) * beyond.z / beyond.u; }
};

// The angle inside the edge between its other face (along -u) and the
// narrow face.
double interior(const Profile& p) { return std::acos(-p.along.u); }

Outline polygon(const std::vector<P>& points) {
  Outline out;
  for (std::size_t i = 1; i <= points.size(); ++i) {
    out.push_back({points[i % points.size()], false, {}});
  }
  return out;
}

Profile profile(Face face, bool convex) {
  Profile p;
  p.convex = convex;
  const double z = convex ? 10.0 : 5.0;
  const double down = convex ? -1.0 : 1.0; // the narrow face's way from the edge
  p.edge_z = z;
  p.corner = {0, z};
  const double reach = 5.0; // how far the extended body continues the narrow face
  switch (face) {
  case Face::Parallel:
  case Face::Spline:
  case Face::Slope:
  case Face::Cone: {
    const double lean = face == Face::Cone ? 0.3 : 0.0;
    p.along = {lean, down * std::sqrt(1.0 - lean * lean)};
    p.far = p.corner + kWidth * p.along;
    if (face == Face::Slope) {
      p.beyond = convex ? P{2, 1} : P{2, -1};
    } else {
      p.beyond = {1, 0};
    }
    const P end = p.corner + reach * p.along;
    if (face == Face::Slope) {
      // The face beyond from the far edge to u = 6, then the outer wall.
      const P out = p.far + 3.0 * p.beyond;
      p.body = polygon({{-kOther, 0}, {out.u, 0}, out, p.far, p.corner, {-kOther, z}});
    } else {
      p.body = polygon({{-kOther, 0}, {10, 0}, {10, p.far.z}, p.far, p.corner, {-kOther, z}});
    }
    if (convex) {
      p.extended = polygon({{-kOther, end.z}, end, p.corner, {-kOther, z}});
    } else {
      p.extended = polygon({{-kOther, 0}, {12, 0}, {12, end.z}, end, p.corner, {-kOther, z}});
    }
    p.planar = true;
    break;
  }
  case Face::Cylinder: {
    // An arc of radius 5 about (-5, z), tangent to the vertical at the
    // edge, 1 long: 0.2 radians.
    const auto on = [&](double angle) { return P{-5 + 5 * std::cos(angle), z + down * 5 * std::sin(angle)}; };
    p.along = {0, down};
    p.far = on(0.2);
    p.beyond = {1, 0};
    p.body = {{{10, 0}, false, {}},     {{10, p.far.z}, false, {}}, {p.far, false, {}},
              {p.corner, true, on(0.1)}, {{-kOther, z}, false, {}},  {{-kOther, 0}, false, {}}};
    const P end = on(1.2); // 5 past the far edge along the circle
    if (convex) {
      p.extended = {{end, false, {}}, {p.corner, true, on(0.6)}, {{-kOther, z}, false, {}},
                    {{-kOther, end.z}, false, {}}};
    } else {
      p.extended = {{{12, 0}, false, {}}, {{12, end.z}, false, {}}, {end, false, {}}, {p.corner, true, on(0.6)},
                    {{-kOther, z}, false, {}}, {{-kOther, 0}, false, {}}};
    }
    p.planar = false;
    break;
  }
  }
  return p;
}

// The first point of an outline (its last step's end).
P start_of(const Outline& outline) { return outline.back().to; }

// --- Areas and first moments of polygons ------------------------------------

struct Moments {
  double area = 0.0;
  double u = 0.0; // the integral of u over the region
};

Moments moments(const std::vector<P>& polygon) {
  Moments m;
  for (std::size_t i = 0; i < polygon.size(); ++i) {
    const P a = polygon[i];
    const P b = polygon[(i + 1) % polygon.size()];
    const double cross = a.u * b.z - b.u * a.z;
    m.area += 0.5 * cross;
    m.u += (a.u + b.u) * cross / 6.0;
  }
  if (m.area < 0) {
    m.area = -m.area;
    m.u = -m.u;
  }
  return m;
}

std::vector<P> points_of(const Outline& outline) {
  std::vector<P> points;
  for (const Step& step : outline) {
    points.push_back(step.to);
  }
  return points;
}

// The polygon clipped to where keep(p) >= 0 (Sutherland-Hodgman, keep
// affine).
template <class Keep>
std::vector<P> clipped(const std::vector<P>& polygon, Keep&& keep) {
  std::vector<P> out;
  for (std::size_t i = 0; i < polygon.size(); ++i) {
    const P a = polygon[i];
    const P b = polygon[(i + 1) % polygon.size()];
    const double ka = keep(a);
    const double kb = keep(b);
    if (ka >= 0) {
      out.push_back(a);
    }
    if ((ka >= 0) != (kb >= 0)) {
      out.push_back(a + (ka / (ka - kb)) * (b - a));
    }
  }
  return out;
}

// The region between the edge and the dressup in the section, as the
// dressup of the narrow face continued gives it: a chamfer's triangle (a
// along the other face, b along the narrow one) or a rounding's corner
// (its contact b along both).
std::vector<P> dressup_region(const Profile& p, Kind kind, double b) {
  const P other{-1, 0};
  if (kind != Kind::Fillet) {
    const double a = b * other_per_narrow(kind, interior(p));
    return {p.corner, p.corner + a * other, p.corner + b * p.along};
  }
  // The interior angle at the edge, the ball's centre on its bisector.
  const double cos_angle = other.u * p.along.u + other.z * p.along.z;
  const double angle = std::acos(cos_angle);
  const double r = b * std::tan(angle / 2);
  P bisector = other + p.along;
  const double length = std::hypot(bisector.u, bisector.z);
  bisector = (1.0 / length) * bisector;
  const P centre = p.corner + (r / std::sin(angle / 2)) * bisector;
  const P on_other = p.corner + b * other;
  const P on_narrow = p.corner + b * p.along;
  const double from = std::atan2(on_other.z - centre.z, on_other.u - centre.u);
  double sweep = std::atan2(on_narrow.z - centre.z, on_narrow.u - centre.u) - from;
  while (sweep > kPi) {
    sweep -= 2 * kPi;
  }
  while (sweep < -kPi) {
    sweep += 2 * kPi;
  }
  std::vector<P> region{p.corner};
  constexpr int kPieces = 4000;
  for (int k = 0; k <= kPieces; ++k) {
    const double t = from + sweep * k / kPieces;
    region.push_back({centre.u + r * std::cos(t), centre.z + r * std::sin(t)});
  }
  return region;
}

// The fillet's radius whose contact on the narrow face is b.
double radius_for(const Profile& p, double b) {
  const double angle = std::acos(-p.along.u);
  return b * std::tan(angle / 2);
}

// Where the dressup's region counts: on the outer side of the face beyond
// (convex: what it takes away there) or on its material side (concave: what
// it adds), within the frame's inner wall (u >= -8: past the edge's other
// face that wall grows out to meet the dressup) and, for a convex edge,
// above the body's bottom. Positive inside.
double counts(const Profile& p, const P& q) {
  const double above = q.z - p.beyond_at(q.u);
  double inside = std::min(p.convex ? above : -above, q.u + kOther);
  if (p.convex) {
    inside = std::min(inside, q.z);
  }
  return inside;
}

// Whether a face of the dressup is left: a point of its boundary in the
// section (the chamfer's line, the rounding's arc) where its region counts.
// Where none is, the faces beyond, grown out, cover all of it: the dressup
// reaches past them too and fails in words.
bool dressup_left(const Profile& p, Kind kind, double b) {
  const std::vector<P> region = dressup_region(p, kind, b);
  std::vector<P> boundary;
  if (kind == Kind::Fillet) {
    boundary.assign(region.begin() + 1, region.end());
  } else {
    for (int k = 0; k <= 400; ++k) {
      boundary.push_back(region[1] + (k / 400.0) * (region[2] - region[1]));
    }
  }
  return std::any_of(boundary.begin(), boundary.end(), [&](const P& q) { return counts(p, q) > 1e-9; });
}

// The exact volume of a row along a straight chain with a planar narrow
// face: the section less (convex) or with (concave) the dressup's region
// where it counts, along 20, or up to the third face (y = 20 + u tan 30).
double exact_volume(const Profile& p, Chain chain, Kind kind, double b) {
  const auto volume = [&](const Moments& m) {
    return chain == Chain::Third ? kLength * m.area + kTilt * m.u : kLength * m.area;
  };
  std::vector<P> region = dressup_region(p, kind, b);
  region = clipped(region, [&](const P& q) {
    const double above = q.z - p.beyond_at(q.u);
    return p.convex ? above : -above;
  });
  region = clipped(region, [&](const P& q) { return q.u + kOther; });
  if (p.convex) {
    region = clipped(region, [&](const P& q) { return q.z; });
  }
  const double body = volume(moments(points_of(p.body)));
  const double part = region.size() < 3 ? 0.0 : volume(moments(region));
  return p.convex ? body - part : body + part;
}

// --- The bodies -------------------------------------------------------------

// The chain's path at the edge's height, its start, and the direction out
// of the edge's other face there.
struct Path {
  TopoDS_Wire wire;
  gp_Pnt start;
  gp_Dir out;
};

Path path(Chain chain, double z) {
  BRepBuilderAPI_MakeWire wire;
  const auto line = [&](double x0, double y0, double x1, double y1) {
    wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(x0, y0, z), gp_Pnt(x1, y1, z)).Edge());
  };
  const auto arc = [&](double cx, double cy, double r, double a0, double a1) {
    const double am = 0.5 * (a0 + a1);
    const Handle(Geom_TrimmedCurve) curve =
        GC_MakeArcOfCircle(gp_Pnt(cx + r * std::cos(a0), cy + r * std::sin(a0), z),
                           gp_Pnt(cx + r * std::cos(am), cy + r * std::sin(am), z),
                           gp_Pnt(cx + r * std::cos(a1), cy + r * std::sin(a1), z))
            .Value();
    wire.Add(BRepBuilderAPI_MakeEdge(curve).Edge());
  };
  Path out;
  out.out = gp_Dir(0, -1, 0);
  if (chain == Chain::Tangent) {
    // A rounded rectangle 60 x 40, its corners of radius 12, from the
    // middle of its lower side round anticlockwise.
    line(0, -20, 18, -20);
    arc(18, -8, 12, -kPi / 2, 0);
    line(30, -8, 30, 8);
    arc(18, 8, 12, 0, kPi / 2);
    line(18, 20, -18, 20);
    arc(-18, 8, 12, kPi / 2, kPi);
    line(-30, 8, -30, -8);
    arc(-18, -8, 12, kPi, 3 * kPi / 2);
    line(-18, -20, 0, -20);
    out.start = gp_Pnt(0, -20, z);
  } else {
    // An octagon of apothem 28, its lower side flat.
    const double r = 28.0 / std::cos(kPi / 8);
    std::vector<std::array<double, 2>> corners;
    for (int k = 0; k < 8; ++k) {
      const double a = -3 * kPi / 8 + k * kPi / 4;
      corners.push_back({r * std::cos(a), r * std::sin(a)});
    }
    line(0, -28, corners[0][0], corners[0][1]);
    for (int k = 0; k < 7; ++k) {
      line(corners[static_cast<std::size_t>(k)][0], corners[static_cast<std::size_t>(k)][1],
           corners[static_cast<std::size_t>(k + 1)][0], corners[static_cast<std::size_t>(k + 1)][1]);
    }
    line(corners[7][0], corners[7][1], 0, -28);
    out.start = gp_Pnt(0, -28, z);
  }
  out.wire = wire.Wire();
  return out;
}

// The outline as a wire through `place`.
template <class Place>
TopoDS_Wire wire_of(const Outline& outline, Place&& place) {
  BRepBuilderAPI_MakeWire wire;
  P from = start_of(outline);
  for (const Step& step : outline) {
    if (step.arc) {
      const Handle(Geom_TrimmedCurve) curve = GC_MakeArcOfCircle(place(from), place(step.mid), place(step.to)).Value();
      wire.Add(BRepBuilderAPI_MakeEdge(curve).Edge());
    } else {
      wire.Add(BRepBuilderAPI_MakeEdge(place(from), place(step.to)).Edge());
    }
    from = step.to;
  }
  return wire.Wire();
}

TopoDS_Shape unified(const TopoDS_Shape& shape) {
  ShapeUpgrade_UnifySameDomain unify(shape, true, true, false);
  unify.Build();
  return unify.Shape();
}

// The outline swept along the chain: along y for the straight ones (the
// third face's end cut at y = 20 + u tan 30 with `cut_end`), along the path
// for the closed ones (mitred at the octagon's corners).
TopoDS_Shape swept(const Outline& outline, Chain chain, double edge_z, bool cut_end = true) {
  if (chain == Chain::Single || chain == Chain::Third) {
    const TopoDS_Wire wire = wire_of(outline, [](const P& q) { return gp_Pnt(q.u, 0, q.z); });
    const TopoDS_Face face = BRepBuilderAPI_MakeFace(wire, true).Face();
    const double length = chain == Chain::Single ? kLength : 30.0;
    const TopoDS_Shape bar = BRepPrimAPI_MakePrism(face, gp_Vec(0, length, 0)).Shape();
    if (chain == Chain::Single || !cut_end) {
      return bar;
    }
    // The half-space y <= 20 + x tan 30.
    const gp_Pln end(gp_Pnt(0, kLength, 0), gp_Dir(-kTilt, 1, 0));
    const TopoDS_Face plane = BRepBuilderAPI_MakeFace(end, -100, 100, -100, 100).Face();
    const TopoDS_Shape keep = BRepPrimAPI_MakeHalfSpace(plane, gp_Pnt(0, 0, 0)).Solid();
    return unified(BRepAlgoAPI_Common(bar, keep).Shape());
  }
  const Path along = path(chain, edge_z);
  const gp_Vec out(along.out);
  const TopoDS_Wire section = wire_of(outline, [&](const P& q) {
    return along.start.Translated(out * q.u + gp_Vec(0, 0, q.z - edge_z));
  });
  BRepOffsetAPI_MakePipeShell pipe(along.wire);
  pipe.SetMode(gp_Dir(0, 0, 1));
  pipe.SetTransitionMode(BRepBuilderAPI_RightCorner);
  pipe.Add(section, false, false);
  pipe.Build();
  pipe.MakeSolid();
  return unified(pipe.Shape());
}

// The body with its faces named N1, N2, ... in TopExp order.
ShapePtr named(const TopoDS_Shape& body) {
  std::vector<Shape::NamedFace> faces;
  int i = 0;
  for (TopExp_Explorer f(body, TopAbs_FACE); f.More(); f.Next()) {
    faces.push_back({f.Current(), {"N" + std::to_string(++i)}});
  }
  return std::make_shared<Shape>(body, faces);
}

double volume_of(const TopoDS_Shape& shape) { return mitcad::analysis::physical_properties(shape).volume; }

double area_of(const TopoDS_Shape& face) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(face, props);
  return props.Mass();
}

double centre_z(const TopoDS_Shape& face) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(face, props);
  return props.CentreOfMass().Z();
}

// The edges of the chain: those at the edge's height on the path (x = 0
// for the straight ones).
std::vector<int> chain_edges(const Shape& body, Chain chain, double edge_z) {
  std::optional<Path> along;
  if (chain == Chain::Tangent || chain == Chain::Octagon) {
    along = path(chain, edge_z);
  }
  std::vector<int> out;
  for (int e = 0; e < body.edge_count(); ++e) {
    if (BRep_Tool::Degenerated(body.edge(e))) {
      continue;
    }
    const BRepAdaptor_Curve curve(body.edge(e));
    bool on = true;
    for (int k = 0; k <= 2 && on; ++k) {
      const gp_Pnt q = curve.Value(curve.FirstParameter() + (curve.LastParameter() - curve.FirstParameter()) * k / 2);
      if (std::abs(q.Z() - edge_z) > 1e-6) {
        on = false;
      } else if (along) {
        BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(q).Vertex(), along->wire);
        on = distance.IsDone() && distance.Value() < 1e-6;
      } else {
        on = std::abs(q.X()) < 1e-6;
      }
    }
    if (on) {
      out.push_back(e);
    }
  }
  return out;
}

// The edge's other face (at the edge's height) and the narrow faces of the
// chain's edges, by name.
struct Faces {
  std::string other;
  std::vector<std::string> narrow;
};

Faces faces_at(const Shape& body, const std::vector<int>& edges, double edge_z) {
  Faces out;
  for (const int e : edges) {
    for (const int f : body.edge_faces(e)) {
      if (f < 0 || body.face_names(f).empty()) {
        continue;
      }
      const std::string& name = body.face_names(f).front();
      if (std::abs(centre_z(body.face(f)) - edge_z) < 1e-6) {
        out.other = name;
      } else if (std::find(out.narrow.begin(), out.narrow.end(), name) == out.narrow.end()) {
        out.narrow.push_back(name);
      }
    }
  }
  return out;
}

std::vector<std::string> names_of(const Shape& body, const std::vector<int>& edges) {
  std::vector<std::string> out;
  for (const int e : edges) {
    out.push_back(body.edge_name(e));
  }
  return out;
}

// The dressup of the row on `body` through the facade.
ShapePtr dressed(const char* feature, const Shape& body, const Profile& p, const std::vector<std::string>& edges,
                 const std::string& other, Kind kind, double b) {
  if (kind == Kind::Fillet) {
    FilletSet set;
    set.edges = edges;
    set.radius = radius_for(p, b);
    return fillet(feature, body, {set});
  }
  ChamferSet set;
  set.edges = edges;
  ChamferSpec& spec = set.spec;
  spec.reference_face = other;
  switch (kind) {
  case Kind::Equal:
    spec.type = ChamferType::EqualDistance;
    spec.distance = b;
    break;
  case Kind::TwoLongOnNarrow:
  case Kind::TwoLongOnOther:
    spec.type = ChamferType::TwoDistances;
    spec.distance = b * other_per_narrow(kind, interior(p)); // on the reference face
    spec.distance2 = b;
    break;
  case Kind::DistanceAngle:
    spec.type = ChamferType::DistanceAngle;
    spec.distance = b * other_per_narrow(kind, interior(p));
    spec.angle = kAngle;
    break;
  case Kind::Fillet:
    break;
  }
  return chamfer(feature, body, {set});
}

bool closed_valid_solid(const TopoDS_Shape& shape) {
  int solids = 0;
  for (TopExp_Explorer solid(shape, TopAbs_SOLID); solid.More(); solid.Next()) {
    ++solids;
    for (TopExp_Explorer shell(solid.Current(), TopAbs_SHELL); shell.More(); shell.Next()) {
      if (!BRep_Tool::IsClosed(shell.Current())) {
        return false;
      }
    }
  }
  return solids == 1 && BRepCheck_Analyzer(shape).IsValid();
}

int faces_starting(const Shape& shape, const std::string& prefix) {
  int count = 0;
  for (int f = 0; f < shape.face_count(); ++f) {
    const NameList& names = shape.face_names(f);
    count += std::any_of(names.begin(), names.end(),
                         [&](const std::string& n) { return n.compare(0, prefix.size(), prefix) == 0; })
                 ? 1
                 : 0;
  }
  return count;
}

// Whether a face of the shape is the face `name` (or a piece of it): a
// face with that name and none of the dressup's (`made`, its prefix). (A
// face the dressup takes away gives its names to the dressup's face there,
// name_consumed_faces.)
bool holds_face(const Shape& shape, const std::string& name, const std::string& made) {
  for (int f = 0; f < shape.face_count(); ++f) {
    const NameList& names = shape.face_names(f);
    const bool of_dressup = std::any_of(names.begin(), names.end(),
                                        [&](const std::string& n) { return n.compare(0, made.size(), made) == 0; });
    if (!of_dressup && std::any_of(names.begin(), names.end(),
                                   [&](const std::string& n) { return name_matches(n, name); })) {
      return true;
    }
  }
  return false;
}

// The reference built in the owner's order with the geometry kernel: the
// dressup of the extended body (the narrow face continued, the face beyond
// left out), its part on the outer side of the face beyond (its surface
// continued) taken from the body (convex), or its part on that face's
// material side added (concave). Negative where it cannot be built.
double built_reference(const Profile& p, Chain chain, Kind kind, double b, const TopoDS_Shape& body) {
  try {
    OCC_CATCH_SIGNALS
    const ShapePtr extended = named(swept(p.extended, chain, p.edge_z));
    const std::vector<int> edges = chain_edges(*extended, chain, p.edge_z);
    const Faces faces = faces_at(*extended, edges, p.edge_z);
    const ShapePtr result = dressed("X", *extended, p, names_of(*extended, edges), faces.other, kind, b);
    // The face beyond's material side, its line continued across the
    // section (u from -9 to 11, z down to -2; for a concave edge from the
    // frame's inner wall, which cuts what is added past the other face),
    // swept as the body is (not cut at the third face).
    const double inner = p.convex ? -9.0 : -kOther;
    const Outline below =
        polygon({{inner, -2}, {11, -2}, {11, p.beyond_at(11)}, {inner, p.beyond_at(inner)}});
    const TopoDS_Shape material = swept(below, chain, p.edge_z, false);
    TopoDS_Shape reference;
    if (p.convex) {
      const TopoDS_Shape taken = BRepAlgoAPI_Cut(extended->occt(), result->occt()).Shape();
      reference = BRepAlgoAPI_Cut(body, BRepAlgoAPI_Cut(taken, material).Shape()).Shape();
    } else {
      const TopoDS_Shape added = BRepAlgoAPI_Cut(result->occt(), extended->occt()).Shape();
      reference = BRepAlgoAPI_Fuse(body, BRepAlgoAPI_Common(added, material).Shape()).Shape();
    }
    return volume_of(reference);
  } catch (const std::exception& error) {
    // (OCCT's failures too.)
    std::fprintf(stderr, "  reference: %s\n", error.what());
  }
  return -1.0;
}

// Diagnostics for a row that fails to build: the geometry kernel's plain
// dressup of the chain on `body` (OCCT's fillet or chamfer, as the facade
// calls it, without the facade's ways round a failure), whether it builds,
// one valid closed solid or not, and its volume, which tells a regression
// of the facade from a case the kernel never built. True where it builds
// one valid closed solid. `what` names the body in the line printed.
bool plain_dressup(const char* what, const Shape& body, const std::vector<int>& edges, const std::string& other,
                   const Profile& p, Kind kind, double b) {
  try {
    OCC_CATCH_SIGNALS
    BRepBuilderAPI_Copy copy(body.occt());
    const std::vector<int> faces = body.find_faces(other);
    const TopoDS_Face face =
        faces.empty() ? TopoDS_Face() : TopoDS::Face(copy.ModifiedShape(body.face(faces.front())));
    TopoDS_Shape built;
    bool done = false;
    if (kind == Kind::Fillet) {
      BRepFilletAPI_MakeFillet maker(copy.Shape());
      for (const int e : edges) {
        maker.Add(radius_for(p, b), TopoDS::Edge(copy.ModifiedShape(body.edge(e))));
      }
      maker.Build();
      done = maker.IsDone();
      if (done) {
        built = maker.Shape();
      }
    } else {
      BRepFilletAPI_MakeChamfer maker(copy.Shape());
      const double a = b * other_per_narrow(kind, interior(p));
      for (const int e : edges) {
        const TopoDS_Edge edge = TopoDS::Edge(copy.ModifiedShape(body.edge(e)));
        if (kind == Kind::Equal) {
          maker.Add(b, edge);
        } else if (kind == Kind::DistanceAngle) {
          maker.AddDA(a, kAngle, edge, face);
        } else {
          maker.Add(a, b, edge, face);
        }
      }
      maker.Build();
      done = maker.IsDone();
      if (done) {
        built = maker.Shape();
      }
    }
    if (!done) {
      std::fprintf(stderr, "  the kernel's plain dressup of %s: fails too\n", what);
      return false;
    }
    const bool sound = closed_valid_solid(built);
    std::fprintf(stderr, "  the kernel's plain dressup of %s: builds, valid closed %d, volume %.9f\n", what,
                 sound ? 1 : 0, volume_of(built));
    return sound;
  } catch (const std::exception& error) {
    std::fprintf(stderr, "  the kernel's plain dressup of %s: throws %s\n", what, error.what());
  }
  return false;
}

// Whether the facade's path past narrow faces can continue a body converted
// to B-splines: whether canonical_dressup (which that path converts the
// body with first, within 1e-7 and, where that leaves a face beside the
// edges a B-spline, up to 1e-4, mitcad#1849) gives every narrow face a
// plane, cylinder, cone, sphere or torus, the surfaces fill_overflow
// continues. Printed for a failing row, at each tolerance.
bool narrow_faces_analytic(const Shape& body, const std::vector<std::string>& narrow) {
  bool any = false;
  for (const double exact : {1e-7, 1e-6, 1e-5, 1e-4}) {
    try {
      OCC_CATCH_SIGNALS
      BRep_Builder builder;
      TopoDS_Compound none;
      builder.MakeCompound(none);
      const ShapePtr analytic = detail::canonical_dressup(Shape(none), body, exact);
      if (!analytic) {
        std::fprintf(stderr, "  the B-spline body within %g: not converted to analytic surfaces\n", exact);
        continue;
      }
      bool all = true;
      for (const std::string& name : narrow) {
        for (const int f : analytic->find_faces(name)) {
          const GeomAbs_SurfaceType type = BRepAdaptor_Surface(analytic->face(f), false).GetType();
          const bool elementary = type == GeomAbs_Plane || type == GeomAbs_Cylinder || type == GeomAbs_Cone ||
                                  type == GeomAbs_Sphere || type == GeomAbs_Torus;
          std::fprintf(stderr, "  the B-spline body within %g: narrow face %s of surface type %d%s\n", exact,
                       name.c_str(), static_cast<int>(type), elementary ? "" : " (not analytic)");
          all = all && elementary;
        }
      }
      any = any || all;
    } catch (const std::exception& error) {
      std::fprintf(stderr, "  the B-spline body's conversion within %g throws %s\n", exact, error.what());
    }
  }
  return any;
}

// Whether the owner's order can be followed with the geometry kernel at
// all: its first step, the kernel's plain dressup of the extended body (the
// narrow face continued, the face beyond left out), builds.
bool extended_dressup(const Profile& p, Chain chain, Kind kind, double b) {
  try {
    OCC_CATCH_SIGNALS
    const ShapePtr extended = named(swept(p.extended, chain, p.edge_z));
    const std::vector<int> edges = chain_edges(*extended, chain, p.edge_z);
    const Faces faces = faces_at(*extended, edges, p.edge_z);
    return plain_dressup("the body with the narrow face continued", *extended, edges, faces.other, p, kind, b);
  } catch (const std::exception& error) {
    std::fprintf(stderr, "  the body with the narrow face continued: %s\n", error.what());
  }
  return false;
}

// The volume of the geometry kernel's own rounding of the chain (as
// patched in the port, mitcad#121), when it is one valid closed solid;
// negative otherwise.
double kernel_rounding(const Shape& body, const std::vector<int>& edges, double r) {
  try {
    OCC_CATCH_SIGNALS
    BRepBuilderAPI_Copy copy(body.occt());
    BRepFilletAPI_MakeFillet maker(copy.Shape());
    for (const int e : edges) {
      maker.Add(r, TopoDS::Edge(copy.ModifiedShape(body.edge(e))));
    }
    maker.Build();
    if (maker.IsDone() && closed_valid_solid(maker.Shape())) {
      return volume_of(maker.Shape());
    }
  } catch (const Standard_Failure&) {
  }
  return -1.0;
}

// --- The rows -----------------------------------------------------------------

struct Body {
  ShapePtr shape;
  std::vector<int> edges;
  std::vector<std::string> edge_names;
  Faces faces;
};

Body body_of(const Profile& p, Face face, Chain chain) {
  TopoDS_Shape shape = swept(p.body, chain, p.edge_z);
  if (face == Face::Spline) {
    shape = BRepBuilderAPI_NurbsConvert(shape, true).Shape();
  }
  Body out;
  out.shape = named(shape);
  out.edges = chain_edges(*out.shape, chain, p.edge_z);
  out.edge_names = names_of(*out.shape, out.edges);
  out.faces = faces_at(*out.shape, out.edges, p.edge_z);
  return out;
}

void run_row(int id, Face face, Chain chain, const Profile& p, const Body& body, Kind kind, double size, bool past) {
  const char* feature = kind == Kind::Fillet ? "R" : "C";
  const std::string made = std::string(feature) + (kind == Kind::Fillet ? ":fillet(" : ":chamfer(");
  std::fprintf(stderr, "row %d: %s, %s, %s, %s, %s%g\n", id, face_name(face), chain_name(chain),
               p.convex ? "convex" : "concave", kind_name(kind), past ? "past the next face: " : "size ", size);
  CHECK(!body.edges.empty() && !body.faces.other.empty() && !body.faces.narrow.empty());
  if (body.edges.empty() || body.faces.other.empty()) {
    return;
  }
  ShapePtr result;
  std::string error;
  try {
    result = dressed(feature, *body.shape, p, body.edge_names, body.faces.other, kind, size);
  } catch (const std::exception& e) {
    error = e.what();
  }
  const bool exact = p.planar && (chain == Chain::Single || chain == Chain::Third);
  // Past the next face the rule decides: where the faces beyond, grown out
  // along their own surfaces (the frame's inner wall past the edge's other
  // face too), leave none of the dressup, it fails in words; where some is
  // left it is built like any other size (a concave dressup only adds
  // material, cut by the faces beyond). Worked out in the section along
  // straight chains with a planar narrow face; elsewhere either outcome is
  // checked as such, without a reference volume.
  const bool expect_built = !past || !exact || dressup_left(p, kind, size);
  // (A fillet past the next face, which keeps the kernel's rounding where
  // that builds and is not under the owner's order, likewise.)
  const bool either = past && (!exact || kind == Kind::Fillet);
  if (!result) {
    const std::string shown = error_message(error);
    std::fprintf(stderr, "  failed: %s\n", shown.c_str());
    const bool in_words =
        shown.find("E{") == std::string::npos && shown.find("not a closed solid") == std::string::npos;
    if (!(past && (!expect_built || either))) {
      // A case the geometry kernel cannot build: below the width, where the
      // dressup is the kernel's own, its plain dressup fails too; from the
      // width on, the owner's order cannot even start (the kernel's plain
      // dressup of the body with the narrow face continued fails). Then a
      // failure in words is the expected outcome; else ours.
      const bool plain = plain_dressup("the body", *body.shape, body.edges, body.faces.other, p, kind, size);
      const bool reachable = size < kWidth ? plain : (plain || extended_dressup(p, chain, kind, size));
      if (!reachable && in_words) {
        std::fprintf(stderr, "  (a case the geometry kernel cannot build: a failure in words)\n");
        return;
      }
      // A body converted to B-splines is built as the body it was converted
      // from, its narrow faces continued as their analytic surfaces, within
      // their own parts of a closed chain (mitcad#1849): no limit of its own.
      // (Printed for a row that still fails.)
      if (face == Face::Spline) {
        (void)narrow_faces_analytic(*body.shape, body.faces.narrow);
      }
      CHECK(false);
      return;
    }
    if (past) {
      CHECK(shown.find("wider than the face beside its edge") != std::string::npos);
      CHECK(in_words);
    }
    return;
  }
  // (A body converted to B-splines integrates to about 1e-5 of its exact
  // volume.)
  const double tolerance = face == Face::Spline ? 1e-5 : 1e-6;
  const double volume = volume_of(result->occt());
  // A fillet keeps the geometry kernel's rolling result where the kernel's
  // plain rounding builds (as patched in the port, mitcad#121; the default
  // recorded on mitcad#1819 while the owner decides): one valid closed
  // solid, no face under 0.01 mm2, a face of the rounding, the volume the
  // kernel's own; the narrow face may stay. Chamfers follow the owner's
  // order.
  if (kind == Kind::Fillet) {
    const double kernel = kernel_rounding(*body.shape, body.edges, radius_for(p, size));
    if (kernel > 0.0) {
      std::fprintf(stderr, "  volume %.9f, the kernel's rolling fillet %.9f (mitcad#121)\n", volume, kernel);
      CHECK(closed_valid_solid(result->occt()));
      CHECK(faces_starting(*result, made) >= 1);
      for (int f = 0; f < result->face_count(); ++f) {
        CHECK(area_of(result->face(f)) > 0.01);
      }
      CHECK_NEAR_TOL(volume, kernel, tolerance);
      return;
    }
  }
  if (!expect_built && !either) {
    std::fprintf(stderr, "  built, volume %.9g: expected to fail in words\n", volume);
    CHECK(false);
    return;
  }
  if (either) {
    CHECK(closed_valid_solid(result->occt()));
    CHECK(faces_starting(*result, made) >= 1);
    for (int f = 0; f < result->face_count(); ++f) {
      CHECK(area_of(result->face(f)) > 0.01);
    }
    std::fprintf(stderr, "  built, volume %.9f (no reference past the other face's width)\n", volume);
    return;
  }
  const double expected =
      exact ? exact_volume(p, chain, kind, size) : built_reference(p, chain, kind, size, body.shape->occt());
  double smallest = 1e300;
  for (int f = 0; f < result->face_count(); ++f) {
    smallest = std::min(smallest, area_of(result->face(f)));
  }
  std::fprintf(stderr, "  volume %.9f, expected %.9f (%s), faces %d, smallest %.3g mm2\n", volume, expected,
               exact ? "exact" : "built", result->face_count(), smallest);
  for (const std::string& note : result->notes()) {
    std::fprintf(stderr, "  note: %s\n", note.c_str());
  }
  CHECK(closed_valid_solid(result->occt()));
  CHECK(smallest > 0.01);
  CHECK(faces_starting(*result, made) >= 1);
  const bool taken = size >= kWidth;
  for (const std::string& narrow : body.faces.narrow) {
    CHECK(holds_face(*result, narrow, made) != taken);
  }
  CHECK(expected > 0.0);
  CHECK_NEAR_TOL(volume, expected, tolerance);
}


// --- The table ----------------------------------------------------------------

// The sizes along the narrow face, as multiples of w; the seventh is past
// the next face (run_row's `past`).
constexpr double kTimes[6] = {0.5, 0.99, 1.0, 1.01, 1.5, 3.0};
constexpr int kSizes = 7;
constexpr int kPast = 6;

struct Row {
  Face face = Face::Parallel;
  Chain chain = Chain::Single;
  bool convex = true;
  Kind kind = Kind::Equal;
  int size = 0; // an index of kTimes, or kPast
};

// The rows of the table's three parts (one ctest each):
// - core: the rule itself, the chamfer of equal distance and the constant
//   fillet at every size on every narrow face, along one straight edge,
//   convex and concave in turn (each of the two kinds sees both);
// - kinds: the other chamfers (two distances each way, distance and
//   angle) at the exact width, 1.01w and past the next face, and the
//   pairwise rows along straight chains;
// - chains: the pairwise rows along the closed chains.
// The pairwise rows complete the table so that every pair of values of any
// two columns (narrow face, chain, convex, dressup, size) is in some row:
// greedily, each the row of the whole cross product that adds the most
// pairs not yet in the table, the cheapest of those (straight chains
// before closed ones, sizes before past). So every value of every column
// meets every value of every other column in some row.
struct Table {
  std::vector<Row> core;
  std::vector<Row> kinds;
  std::vector<Row> chains;
};

Table table() {
  Table out;
  const Face faces[] = {Face::Parallel, Face::Slope, Face::Cylinder, Face::Cone, Face::Spline};
  const Chain chains[] = {Chain::Single, Chain::Tangent, Chain::Octagon, Chain::Third};
  const Kind kinds[] = {Kind::Equal, Kind::TwoLongOnNarrow, Kind::TwoLongOnOther, Kind::DistanceAngle, Kind::Fillet};
  for (const Kind kind : {Kind::Equal, Kind::Fillet}) {
    for (int f = 0; f < 5; ++f) {
      for (int s = 0; s < kSizes; ++s) {
        const bool even = (f + s) % 2 == 0;
        out.core.push_back({faces[f], Chain::Single, kind == Kind::Equal ? even : !even, kind, s});
      }
    }
  }
  int turn = 0;
  for (const Kind kind : {Kind::TwoLongOnNarrow, Kind::TwoLongOnOther, Kind::DistanceAngle}) {
    for (const int s : {2, 3, kPast}) {
      out.kinds.push_back({Face::Parallel, Chain::Single, (turn++ % 2) == 0, kind, s});
    }
  }
  // The pairs: (column i, value a, column j, value b), i < j.
  const int counts[5] = {5, 4, 2, 5, kSizes};
  const auto values = [](const Row& r) {
    return std::array<int, 5>{static_cast<int>(r.face), static_cast<int>(r.chain), r.convex ? 0 : 1,
                              static_cast<int>(r.kind), r.size};
  };
  const auto key = [](int i, int a, int j, int b) { return static_cast<std::size_t>(((i * 5 + j) * 8 + a) * 8 + b); };
  std::vector<char> covered(5 * 5 * 8 * 8, 0);
  const auto cover = [&](const Row& r, bool mark) {
    const std::array<int, 5> v = values(r);
    int added = 0;
    for (std::size_t i = 0; i < 5; ++i) {
      for (std::size_t j = i + 1; j < 5; ++j) {
        char& c = covered[key(static_cast<int>(i), v[i], static_cast<int>(j), v[j])];
        added += c == 0 ? 1 : 0;
        if (mark) {
          c = 1;
        }
      }
    }
    return added;
  };
  for (const Row& r : out.core) {
    cover(r, true);
  }
  for (const Row& r : out.kinds) {
    cover(r, true);
  }
  int missing = 0;
  for (int i = 0; i < 5; ++i) {
    for (int j = i + 1; j < 5; ++j) {
      for (int a = 0; a < counts[i]; ++a) {
        for (int b = 0; b < counts[j]; ++b) {
          missing += covered[key(i, a, j, b)] == 0 ? 1 : 0;
        }
      }
    }
  }
  const auto cost = [](const Row& r) {
    const int chain = r.chain == Chain::Single ? 0 : r.chain == Chain::Third ? 1 : 3;
    return chain + (r.size == kPast ? 2 : 0);
  };
  while (missing > 0) {
    Row best;
    int most = 0;
    int cheapest = 0;
    for (const Face face : faces) {
      for (const Chain chain : chains) {
        for (const bool convex : {true, false}) {
          for (const Kind kind : kinds) {
            for (int s = 0; s < kSizes; ++s) {
              const Row r{face, chain, convex, kind, s};
              const int adds = cover(r, false);
              if (adds > most || (adds == most && adds > 0 && cost(r) < cheapest)) {
                best = r;
                most = adds;
                cheapest = cost(r);
              }
            }
          }
        }
      }
    }
    if (most == 0) {
      break;
    }
    cover(best, true);
    missing -= most;
    (best.chain == Chain::Single || best.chain == Chain::Third ? out.kinds : out.chains).push_back(best);
  }
  return out;
}

// The bodies of the rows, built once each.
struct Bodies {
  std::vector<std::pair<std::array<int, 3>, Body>> built;
  const Body& of(const Profile& p, Face face, Chain chain) {
    const std::array<int, 3> key{static_cast<int>(face), static_cast<int>(chain), p.convex ? 1 : 0};
    for (const auto& entry : built) {
      if (entry.first == key) {
        return entry.second;
      }
    }
    Body body;
    try {
      OCC_CATCH_SIGNALS
      body = body_of(p, face, chain);
    } catch (const std::exception& error) {
      std::fprintf(stderr, "%s %s %s: the body cannot be built: %s\n", face_name(face), chain_name(chain),
                   p.convex ? "convex" : "concave", error.what());
    }
    built.emplace_back(key, body);
    return built.back().second;
  }
};

void run_rows(const char* part, const std::vector<Row>& rows) {
  Bodies bodies;
  // (Built first: a reference into `built` stays valid while no body is
  // added.)
  for (const Row& row : rows) {
    bodies.of(profile(row.face, row.convex), row.face, row.chain);
  }
  int id = 0;
  for (const Row& row : rows) {
    ++id;
    const Profile p = profile(row.face, row.convex);
    const Body& body = bodies.of(p, row.face, row.chain);
    if (!body.shape) {
      CHECK(false);
      continue;
    }
    const bool past = row.size == kPast;
    const double size = past ? 1.25 * kOther / other_per_narrow(row.kind, interior(p))
                             : kTimes[static_cast<std::size_t>(row.size)] * kWidth;
    const std::string name = std::string(part) + " row " + std::to_string(id);
    guarded(name.c_str(), [&] { run_row(id, row.face, row.chain, p, body, row.kind, size, past); });
  }
}

// --- An opening's rim over rounded corners (mitcad#1821) -------------------

// A plate 60 x 60 x 6 with an octagonal opening (apothem 15) through it,
// its corners rounded with `corner` (0: sharp), and the opening's top rim
// chamfered (or rounded) by `size` along its whole chain: no piece of the
// top's old surface is left at any corner (no face under 0.01 mm2), no
// note, one valid closed solid, a face of the dressup along every edge of
// the chain.
void run_opening(double corner, Kind kind, double size) {
  std::fprintf(stderr, "opening, corners %g, %s %g\n", corner, kind_name(kind), size);
  const double r = 15.0 / std::cos(kPi / 8);
  BRepBuilderAPI_MakeWire wire;
  for (int k = 0; k < 8; ++k) {
    const double a0 = kPi / 8 + k * kPi / 4;
    const double a1 = a0 + kPi / 4;
    wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(r * std::cos(a0), r * std::sin(a0), -1),
                                     gp_Pnt(r * std::cos(a1), r * std::sin(a1), -1))
                 .Edge());
  }
  TopoDS_Face octagon = BRepBuilderAPI_MakeFace(wire.Wire(), true).Face();
  if (corner > 0.0) {
    BRepFilletAPI_MakeFillet2d rounding(octagon);
    std::vector<TopoDS_Vertex> corners;
    for (TopExp_Explorer v(octagon, TopAbs_VERTEX); v.More(); v.Next()) {
      const TopoDS_Vertex vertex = TopoDS::Vertex(v.Current());
      if (std::none_of(corners.begin(), corners.end(), [&](const TopoDS_Vertex& c) { return c.IsSame(vertex); })) {
        corners.push_back(vertex);
      }
    }
    for (const TopoDS_Vertex& vertex : corners) {
      rounding.AddFillet(vertex, corner);
    }
    rounding.Build();
    CHECK(rounding.IsDone());
    if (!rounding.IsDone()) {
      return;
    }
    octagon = TopoDS::Face(rounding.Shape());
  }
  const TopoDS_Shape hole = BRepPrimAPI_MakePrism(octagon, gp_Vec(0, 0, 8)).Shape();
  const TopoDS_Shape plate = BRepPrimAPI_MakeBox(gp_Pnt(-30, -30, 0), 60, 60, 6).Shape();
  const ShapePtr body = named(BRepAlgoAPI_Cut(plate, hole).Shape());
  std::vector<std::string> edges;
  for (int e = 0; e < body->edge_count(); ++e) {
    const BRepAdaptor_Curve curve(body->edge(e));
    const gp_Pnt m = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
    if (std::abs(m.Z() - 6) < 1e-6 && std::hypot(m.X(), m.Y()) < 20) {
      edges.push_back(body->edge_name(e));
    }
  }
  CHECK(edges.size() == (corner > 0.0 ? 16u : 8u));
  ShapePtr result;
  try {
    if (kind == Kind::Fillet) {
      FilletSet set;
      set.edges = edges;
      set.radius = size;
      result = fillet("R", *body, {set});
    } else {
      ChamferSet set;
      set.edges = edges;
      set.spec.distance = size;
      result = chamfer("C", *body, {set});
    }
  } catch (const std::exception& error) {
    std::fprintf(stderr, "  failed: %s\n", error.what());
  }
  CHECK(result != nullptr);
  if (!result) {
    return;
  }
  double smallest = 1e300;
  for (int f = 0; f < result->face_count(); ++f) {
    const double area = area_of(result->face(f));
    if (area < 0.01) {
      const NameList& names = result->face_names(f);
      std::fprintf(stderr, "  a face of %.3g mm2: %s\n", area, names.empty() ? "?" : names.front().c_str());
    }
    smallest = std::min(smallest, area);
  }
  for (const std::string& note : result->notes()) {
    std::fprintf(stderr, "  note: %s\n", note.c_str());
  }
  CHECK(closed_valid_solid(result->occt()));
  CHECK(smallest > 0.01);
  CHECK(result->notes().empty());
  const int made = faces_starting(*result, kind == Kind::Fillet ? "R:fillet(" : "C:chamfer(");
  CHECK(made >= static_cast<int>(edges.size()));
}

void run_openings() {
  int id = 0;
  const auto row = [&](double corner, Kind kind, double size) {
    const std::string name = "opening row " + std::to_string(++id);
    guarded(name.c_str(), [&] { run_opening(corner, kind, size); });
  };
  row(3.0, Kind::Equal, 0.5);
  row(3.0, Kind::Equal, 1.0);
  row(3.0, Kind::Fillet, 1.0);
  row(0.0, Kind::Equal, 0.5);
  row(0.0, Kind::Equal, 1.0);
}

// --- A ring's floor chamfered inside a rounded octagon (mitcad#1821) -------

// A reported nut's end, as a pocket: a plate 70 x 70 x 10 with an octagonal
// pocket 5 deep (apothem 26.6, corners rounded `corner`) and a bore of
// radius 26 through its floor, so that the floor is a ring 0.6 wide along
// the octagon's sides and about 2.6 at its corners; the floor's rim along
// the walls (16 concave edges, 8 over sharp corners) chamfered by `size`.
// Where a corner is rounded more tightly than the chamfer reaches, the
// kernel's bevel round the corner's arc folds over: it left a groove down
// to the floor between the two bevels and a sliver of the floor past their
// contact lines' crossing (0.05 mm2 and 0.003 mm2 on the nut). The bevels
// meet at a mitre there instead. One valid closed solid, more material
// than the plate (a chamfer of concave edges only adds), a face of the
// chamfer along each side of the octagon, every face named, no face under
// 0.01 mm2, no piece of the floor under 0.1 mm2 and no note.
void run_ring_floor(double corner, double size) {
  std::fprintf(stderr, "ring floor, corners %g, chamfer %g\n", corner, size);
  const double r = 26.6 / std::cos(kPi / 8);
  BRepBuilderAPI_MakeWire wire;
  for (int k = 0; k < 8; ++k) {
    const double a0 = kPi / 8 + k * kPi / 4;
    const double a1 = a0 + kPi / 4;
    wire.Add(BRepBuilderAPI_MakeEdge(gp_Pnt(r * std::cos(a0), r * std::sin(a0), 5),
                                     gp_Pnt(r * std::cos(a1), r * std::sin(a1), 5))
                 .Edge());
  }
  TopoDS_Face octagon = BRepBuilderAPI_MakeFace(wire.Wire(), true).Face();
  if (corner > 0.0) {
    BRepFilletAPI_MakeFillet2d rounding(octagon);
    std::vector<TopoDS_Vertex> corners;
    for (TopExp_Explorer v(octagon, TopAbs_VERTEX); v.More(); v.Next()) {
      const TopoDS_Vertex vertex = TopoDS::Vertex(v.Current());
      if (std::none_of(corners.begin(), corners.end(), [&](const TopoDS_Vertex& c) { return c.IsSame(vertex); })) {
        corners.push_back(vertex);
      }
    }
    for (const TopoDS_Vertex& vertex : corners) {
      rounding.AddFillet(vertex, corner);
    }
    rounding.Build();
    CHECK(rounding.IsDone());
    if (!rounding.IsDone()) {
      return;
    }
    octagon = TopoDS::Face(rounding.Shape());
  }
  const TopoDS_Shape pocket = BRepPrimAPI_MakePrism(octagon, gp_Vec(0, 0, 6)).Shape();
  const TopoDS_Shape bore = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -1), gp_Dir(0, 0, 1)), 26.0, 12.0).Shape();
  const TopoDS_Shape plate = BRepPrimAPI_MakeBox(gp_Pnt(-35, -35, 0), 70, 70, 10).Shape();
  const ShapePtr body = named(BRepAlgoAPI_Cut(BRepAlgoAPI_Cut(plate, pocket).Shape(), bore).Shape());
  std::string floor;
  for (int f = 0; f < body->face_count(); ++f) {
    if (std::abs(centre_z(body->face(f)) - 5) < 1e-6) {
      floor = body->face_names(f).front();
    }
  }
  CHECK(!floor.empty());
  std::vector<std::string> edges;
  for (int e = 0; e < body->edge_count(); ++e) {
    const BRepAdaptor_Curve curve(body->edge(e));
    const gp_Pnt m = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
    if (std::abs(m.Z() - 5) < 1e-6 && std::hypot(m.X(), m.Y()) > 26.3 && std::hypot(m.X(), m.Y()) < 30) {
      edges.push_back(body->edge_name(e));
    }
  }
  CHECK(edges.size() == (corner > 0.0 ? 16u : 8u));
  ShapePtr result;
  try {
    ChamferSet set;
    set.edges = edges;
    set.spec.distance = size;
    result = chamfer("C", *body, {set});
  } catch (const std::exception& error) {
    std::fprintf(stderr, "  failed: %s\n", error.what());
  }
  CHECK(result != nullptr);
  if (!result) {
    return;
  }
  for (const std::string& note : result->notes()) {
    std::fprintf(stderr, "  note: %s\n", note.c_str());
  }
  double smallest = 1e300;
  for (int f = 0; f < result->face_count(); ++f) {
    const NameList& names = result->face_names(f);
    const double area = area_of(result->face(f));
    smallest = std::min(smallest, area);
    CHECK(!names.empty());
    if (!names.empty() && area < 0.1 && names.front().rfind(floor, 0) == 0) {
      std::fprintf(stderr, "  a piece of the floor of %.3g mm2: %s\n", area, names.front().c_str());
      CHECK(false);
    }
  }
  CHECK(smallest > 0.01);
  CHECK(result->notes().empty());
  // A chamfer of concave edges only adds material.
  CHECK(volume_of(result->occt()) > volume_of(body->occt()));
  CHECK(closed_valid_solid(result->occt()));
  CHECK(faces_starting(*result, "C:chamfer(") >= 8);
}

void run_ring_floors() {
  int id = 0;
  const auto row = [&](double corner, double size) {
    const std::string name = "ring floor row " + std::to_string(++id);
    guarded(name.c_str(), [&] { run_ring_floor(corner, size); });
  };
  row(1.0, 2.55);
  row(1.0, 2.0);
  row(0.0, 2.55);
}

// --- A free-form narrow face (mitcad#1849) ----------------------------------

// The convex step of the table (the edge's other face 8 wide at z = 10, the
// narrow face 1 high) along one straight edge, its narrow face bent into a
// slight S (a cubic of the section, 0.014 off the plane at most, its ends
// at the edge and at the far edge), every face converted to B-splines: the
// narrow face is no plane, cylinder, cone, sphere or torus within 1e-4, so
// the path past narrow faces cannot continue it. Chamfered 3 (3w): where
// the geometry kernel builds it, one valid closed solid with a face of the
// chamfer and no face under 0.01 mm2; else a failure in words that says the
// face is a free-form surface that cannot be continued, no names, never
// that the result is not a closed solid.
void run_freeform() {
  std::fprintf(stderr, "free-form narrow face, single, convex, chamfer equal, size 3\n");
  const auto at = [](double u, double z) { return gp_Pnt(u, 0, z); };
  NCollection_Array1<gp_Pnt> poles(1, 4);
  poles(1) = at(0, 9);
  poles(2) = at(0.05, 9 + 1.0 / 3.0);
  poles(3) = at(-0.05, 9 + 2.0 / 3.0);
  poles(4) = at(0, 10);
  NCollection_Array1<double> knots(1, 2);
  NCollection_Array1<int> mults(1, 2);
  knots(1) = 0.0;
  knots(2) = 1.0;
  mults(1) = 4;
  mults(2) = 4;
  const occ::handle<Geom_BSplineCurve> bent = new Geom_BSplineCurve(poles, knots, mults, 3);
  BRepBuilderAPI_MakeWire wire;
  wire.Add(BRepBuilderAPI_MakeEdge(at(-kOther, 0), at(10, 0)).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(at(10, 0), at(10, 9)).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(at(10, 9), at(0, 9)).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(bent).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(at(0, 10), at(-kOther, 10)).Edge());
  wire.Add(BRepBuilderAPI_MakeEdge(at(-kOther, 10), at(-kOther, 0)).Edge());
  CHECK(wire.IsDone());
  if (!wire.IsDone()) {
    return;
  }
  const TopoDS_Face section = BRepBuilderAPI_MakeFace(wire.Wire(), true).Face();
  const TopoDS_Shape bar = BRepPrimAPI_MakePrism(section, gp_Vec(0, kLength, 0)).Shape();
  const ShapePtr body = named(BRepBuilderAPI_NurbsConvert(bar, true).Shape());
  const std::vector<int> edges = chain_edges(*body, Chain::Single, 10.0);
  CHECK(edges.size() == 1u);
  if (edges.empty()) {
    return;
  }
  ShapePtr result;
  std::string error;
  try {
    ChamferSet set;
    set.edges = names_of(*body, edges);
    set.spec.type = ChamferType::EqualDistance;
    set.spec.distance = 3.0 * kWidth;
    result = chamfer("C", *body, {set});
  } catch (const std::exception& e) {
    error = e.what();
  }
  if (result) {
    double smallest = 1e300;
    for (int f = 0; f < result->face_count(); ++f) {
      smallest = std::min(smallest, area_of(result->face(f)));
    }
    std::fprintf(stderr, "  built by the geometry kernel: volume %.9f, smallest face %.3g mm2\n",
                 volume_of(result->occt()), smallest);
    CHECK(closed_valid_solid(result->occt()));
    CHECK(smallest > 0.01);
    CHECK(faces_starting(*result, "C:chamfer(") >= 1);
    return;
  }
  const std::string shown = error_message(error);
  std::fprintf(stderr, "  failed: %s\n", shown.c_str());
  CHECK(shown.find("wider than the face beside its edge") != std::string::npos);
  CHECK(shown.find("free-form surface") != std::string::npos);
  // (No names: neither an edge's nor a face's, N1, N2, … here.)
  bool names = shown.find("E{") != std::string::npos;
  for (std::size_t i = 0; i + 1 < shown.size(); ++i) {
    names = names || (shown[i] == 'N' && shown[i + 1] >= '0' && shown[i + 1] <= '9');
  }
  CHECK(!names);
  CHECK(shown.find("not a closed solid") == std::string::npos);
}

} // namespace
} // namespace test

int main(int argc, char** argv) {
  std::string part;
  for (int i = 1; i + 1 < argc; ++i) {
    if (std::strcmp(argv[i], "--part") == 0) {
      part = argv[i + 1];
    }
  }
  if (!part.empty() && part != "core" && part != "kinds" && part != "chains" && part != "ring") {
    std::fprintf(stderr, "no part %s\n", part.c_str());
    return 1;
  }
  const test::Table rows = test::table();
  std::fprintf(stderr, "the table: %zu core rows, %zu kinds rows, %zu chains rows, 5 opening rows\n",
               rows.core.size(), rows.kinds.size(), rows.chains.size());
  if (part.empty() || part == "core") {
    test::run_rows("core", rows.core);
  }
  if (part.empty() || part == "kinds") {
    test::run_rows("kinds", rows.kinds);
  }
  if (part.empty() || part == "chains") {
    test::run_rows("chains", rows.chains);
    test::run_openings();
    test::guarded("free-form narrow face", [] { test::run_freeform(); });
  }
  if (part.empty() || part == "ring") {
    test::run_ring_floors();
  }
  if (test::failures == 0) {
    std::puts("test_dressup_consumes_face: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
