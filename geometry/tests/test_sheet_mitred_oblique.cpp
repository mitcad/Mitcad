// SPDX-License-Identifier: MIT
// Mitred elbows and eccentric cones developed into sheet panels
// (mitcad#2491): the gores of a 90-degree elbow, their oblique cuts kept
// at the neutral surface (the known stretch-out), and an eccentric cone
// frustum developed by its generators (generator lengths and the lengths
// of both edges at the neutral surface, its area), also when its side is
// two faces developed together as one panel (mitcad#2559).
#include <algorithm>
#include <cmath>
#include <cstdio>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeHalfSpace.hxx>
#include <BRep_Tool.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <Geom2dAdaptor_Curve.hxx>
#include <Geom2d_BSplineCurve.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <GeomLProp_SLProps.hxx>
#include <NCollection_Array1.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Pln.hxx>
#include <gp_Trsf.hxx>

#include "check.hpp"
#include "mitcad/geometry/input_check.hpp"
#include "mitcad/geometry/sheet_develop.hpp"
#include "bundled_test.hpp"

namespace {
using namespace mitcad::geometry;

const double kTan = std::tan(test::kPi / 8); // the cut at 22.5 degrees
const double kBend = 60;                      // the elbow's centre-line radius
const double kNeutral = 19.5;                 // the tube 19 to 20, K 0.5

// `shape` less the half-space beyond the plane through `at` normal to
// `normal` (the side it points to).
TopoDS_Shape cut_beyond(const TopoDS_Shape& shape, const gp_Pnt& at, const gp_Dir& normal) {
  const TopoDS_Face plane = BRepBuilderAPI_MakeFace(gp_Pln(at, normal)).Face();
  const TopoDS_Shape half = BRepPrimAPI_MakeHalfSpace(plane, at.Translated(gp_Vec(normal) * 10)).Solid();
  BRepAlgoAPI_Cut cut(shape, half);
  CHECK(cut.IsDone());
  return cut.Shape();
}

// A tube 19 to 20 about Z from z0 to z1, its seam at +Y.
TopoDS_Shape tube(double z0, double z1) {
  const gp_Ax2 axes(gp_Pnt(0, 0, z0), gp_Dir(0, 0, 1), gp_Dir(0, 1, 0));
  const TopoDS_Shape outer = BRepPrimAPI_MakeCylinder(axes, 20, z1 - z0).Shape();
  const TopoDS_Shape inner = BRepPrimAPI_MakeCylinder(axes, 19, z1 - z0).Shape();
  BRepAlgoAPI_Cut cut(outer, inner);
  CHECK(cut.IsDone());
  return cut.Shape();
}

// The shape with its face that is no plane named `name` (every piece of
// it, as a boolean may leave a cut tube's face in pieces).
Shape named(const TopoDS_Shape& shape, const std::string& name, double radius = 0) {
  std::vector<Shape::NamedFace> faces;
  for (TopExp_Explorer e(shape, TopAbs_FACE); e.More(); e.Next()) {
    const BRepAdaptor_Surface surface(TopoDS::Face(e.Current()));
    if (surface.GetType() == GeomAbs_Plane) continue;
    if (radius > 0 && (surface.GetType() != GeomAbs_Cylinder || std::abs(surface.Cylinder().Radius() - radius) > 1e-9)) {
      continue;
    }
    faces.push_back({e.Current(), {name}});
  }
  CHECK(!faces.empty());
  return Shape(shape, faces);
}

occ::handle<Geom2d_BSplineCurve> spline_of(const Segment& s) {
  NCollection_Array1<gp_Pnt2d> poles(1, static_cast<int>(s.poles.size()));
  for (std::size_t i = 0; i < s.poles.size(); ++i) poles(static_cast<int>(i) + 1) = s.poles[i];
  NCollection_Array1<double> knots(1, static_cast<int>(s.knots.size()));
  NCollection_Array1<int> mults(1, static_cast<int>(s.knots.size()));
  for (std::size_t i = 0; i < s.knots.size(); ++i) {
    knots(static_cast<int>(i) + 1) = s.knots[i];
    mults(static_cast<int>(i) + 1) = s.multiplicities[i];
  }
  if (s.weights.empty()) return new Geom2d_BSplineCurve(poles, knots, mults, s.degree, s.periodic);
  NCollection_Array1<double> weights(1, static_cast<int>(s.weights.size()));
  for (std::size_t i = 0; i < s.weights.size(); ++i) weights(static_cast<int>(i) + 1) = s.weights[i];
  return new Geom2d_BSplineCurve(poles, weights, knots, mults, s.degree, s.periodic);
}

// Where the developed outline crosses the line u = `u` (a cylinder's chart:
// along the circumference, v along the axis).
std::vector<double> crossings(const std::vector<BoundedCurve2>& edges, double u) {
  std::vector<double> result;
  for (const auto& edge : edges) {
    const Segment& s = edge.geometry;
    if (s.kind == SegmentKind::Line) {
      const double du = s.end.X() - s.start.X();
      if (std::abs(du) <= 1e-9) continue;
      const double f = (u - s.start.X()) / du;
      if (f >= 0 && f <= 1) result.push_back(s.start.Y() + f * (s.end.Y() - s.start.Y()));
      continue;
    }
    CHECK(s.kind == SegmentKind::BSpline);
    if (s.kind != SegmentKind::BSpline) continue;
    const auto curve = spline_of(s);
    const double a = std::min(edge.range.first, edge.range.last), b = std::max(edge.range.first, edge.range.last);
    double lo = a, flo = curve->Value(a).X() - u;
    for (int i = 1; i <= 64; ++i) {
      const double hi = a + (b - a) * i / 64, fhi = curve->Value(hi).X() - u;
      if ((flo <= 0 && fhi >= 0) || (flo >= 0 && fhi <= 0)) {
        double x0 = lo, x1 = hi, f0 = flo;
        for (int k = 0; k < 80; ++k) {
          const double m = (x0 + x1) / 2, fm = curve->Value(m).X() - u;
          if ((f0 <= 0) == (fm <= 0)) {
            x0 = m;
            f0 = fm;
          } else {
            x1 = m;
          }
        }
        result.push_back(curve->Value((x0 + x1) / 2).Y());
        break;
      }
      lo = hi;
      flo = fhi;
    }
  }
  return result;
}

// A gore developed at the neutral radius: a closed panel 2 pi 19.5 round,
// its seam (at +Y, where x = 0) two lines `seam` long; at the developed
// length s from the seam the generator is `seam + cuts 19.5 tan a |sin(s /
// 19.5)|` long (cuts: how many ends are mitred), longer one way round (the
// heel) and shorter the other (the throat), and the mitred end `low_cut`
// (the lower one) offset from the seam's end by 19.5 tan a |sin(s / 19.5)|.
void check_gore(const Shape& gore, double seam, int cuts, bool low_cut, const char* what) {
  const SheetFaceDevelopment developed = sheet_face_development(gore, {"G:outer"}, 0.5, 1);
  CHECK(!developed.cone && !developed.oblique);
  CHECK(developed.closed);
  CHECK(!developed.inner_is_front);
  CHECK_NEAR(developed.neutral_radius, kNeutral);
  CHECK(developed.faces.size() == 1);
  if (developed.faces.size() != 1) return;
  const auto& edges = developed.faces.front();
  // The seam: two lines along the generator.
  std::vector<double> seams;
  for (const auto& edge : edges) {
    const Segment& s = edge.geometry;
    if (s.kind == SegmentKind::Line && std::abs(s.end.X() - s.start.X()) <= 1e-9) {
      seams.push_back(s.start.X());
      CHECK(std::abs(std::abs(s.end.Y() - s.start.Y()) - seam) <= 1e-6);
    }
  }
  CHECK(seams.size() == 2);
  if (seams.size() != 2) return;
  const double from = std::min(seams[0], seams[1]);
  CHECK(std::abs(std::abs(seams[1] - seams[0]) - 2 * test::kPi * kNeutral) <= 1e-6);
  const double amplitude = kNeutral * kTan;
  double longest = 0, shortest = 1e300;
  double low_start = 0;
  {
    const auto at = crossings(edges, from + 1e-7);
    CHECK(at.size() == 2);
    if (!at.empty()) low_start = *std::min_element(at.begin(), at.end());
  }
  int wrong = 0;
  for (int k = 1; k < 64; ++k) {
    const double s = 2 * test::kPi * kNeutral * k / 64;
    const auto at = crossings(edges, from + s);
    if (at.size() != 2) {
      ++wrong;
      continue;
    }
    const double low = std::min(at[0], at[1]), high = std::max(at[0], at[1]);
    const double length = high - low;
    const double off = amplitude * std::abs(std::sin(s / kNeutral));
    if (std::abs(std::abs(length - seam) - cuts * off) > 1e-6) ++wrong;
    if (low_cut && std::abs(std::abs(low - low_start) - off) > 1e-6) ++wrong;
    if (!low_cut && std::abs(low - low_start) > 1e-6) ++wrong;
    longest = std::max(longest, length);
    shortest = std::min(shortest, length);
  }
  if (wrong != 0) std::fprintf(stderr, "%s: %d generators off the stretch-out\n", what, wrong);
  CHECK(wrong == 0);
  // The heel and the throat (a quarter turn from the seam either way).
  CHECK(std::abs(longest - (seam + cuts * amplitude)) <= 1e-6);
  CHECK(std::abs(shortest - (seam - cuts * amplitude)) <= 1e-6);
}

// A 90-degree elbow of three gores (two joints of 45 degrees, each cut at
// 22.5): the middle gore mitred at both ends (Lm = 2 60 tan a at its
// centre line), the end gores at one (Le = 60 tan a), each in its place in
// the elbow (an end gore turned 45 degrees about the bend's axis).
void mitred_elbow() {
  const double lm = 2 * kBend * kTan, le = kBend * kTan;
  // Middle: from z = -x tan a to z = Lm + x tan a.
  TopoDS_Shape middle = tube(-30, 90);
  middle = cut_beyond(middle, gp_Pnt(0, 0, 0), gp_Dir(-kTan, 0, -1));
  middle = cut_beyond(middle, gp_Pnt(0, 0, lm), gp_Dir(-kTan, 0, 1));
  check_gore(named(middle, "G:outer", 20), lm, 2, true, "middle gore");
  // An end gore: square at z = 0, mitred at z = Le + x tan a.
  TopoDS_Shape end = tube(0, 60);
  end = cut_beyond(end, gp_Pnt(0, 0, le), gp_Dir(-kTan, 0, 1));
  check_gore(named(end, "G:outer", 20), le, 1, false, "end gore");
  // The other: mitred at z = -x tan a, square at Le; in the elbow, turned
  // about the bend's axis (along Y through (-60, 0, Le)) onto the middle
  // gore's upper end.
  TopoDS_Shape other = tube(-30, le);
  other = cut_beyond(other, gp_Pnt(0, 0, 0), gp_Dir(-kTan, 0, -1));
  gp_Trsf place;
  place.SetRotation(gp_Ax1(gp_Pnt(-kBend, 0, le), gp_Dir(0, 1, 0)), -test::kPi / 4);
  gp_Trsf lift;
  lift.SetTranslation(gp_Vec(0, 0, lm - le - le));
  other = BRepBuilderAPI_Transform(other, place.Multiplied(lift), true).Shape();
  check_gore(named(other, "G:outer", 20), le, 1, true, "placed end gore");
}

// An eccentric frustum: the ruled solid from the circle of radius 60 about
// the origin on XY to the one of radius 30 about (20, 0, 60), the cone of
// the lines from (40, 0, 120). Its side developed at the surface 0.5 inside
// it (t 1, K 0.5, the side the outer surface): the seam two lines along
// the generator from (60, 0, 0) to (50, 0, 60); both edges as long as the
// neutral surface's curves in the end planes (each point where the
// generator's neutral line meets the plane, the normal read from the
// face); the developed area that of the neutral surface between the planes.
const gp_Pnt kApex(40, 0, 120);

// The eccentric frustum's ruled loft; with `split`, each circle is two
// arcs meeting at (0, +-r) about its centre (a quarter turn either side of
// +X), so its side is two faces meeting along the generators there
// (mitcad#2559).
TopoDS_Shape frustum_loft(bool split) {
  BRepOffsetAPI_ThruSections loft(true, true);
  for (const auto& [centre, radius] : {std::pair{gp_Pnt(0, 0, 0), 60.0}, std::pair{gp_Pnt(20, 0, 60), 30.0}}) {
    const gp_Circ circle(gp_Ax2(centre, gp_Dir(0, 0, 1), gp_Dir(1, 0, 0)), radius);
    BRepBuilderAPI_MakeWire wire;
    if (split) {
      wire.Add(BRepBuilderAPI_MakeEdge(circle, -test::kPi / 2, test::kPi / 2).Edge());
      wire.Add(BRepBuilderAPI_MakeEdge(circle, test::kPi / 2, 3 * test::kPi / 2).Edge());
    } else {
      wire.Add(BRepBuilderAPI_MakeEdge(circle).Edge());
    }
    CHECK(wire.IsDone());
    loft.AddWire(wire.Wire());
  }
  loft.Build();
  CHECK(loft.IsDone());
  return loft.IsDone() ? loft.Shape() : TopoDS_Shape();
}

// The lengths of the neutral surface's curves in the end planes z = 0 and
// z = 60 (each point where the generator's neutral line meets the plane,
// 0.5 inside the side `side` along its normals, read from its surface).
std::pair<double, double> neutral_edges(const TopoDS_Face& side) {
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(side);
  const gp_Pnt& apex = kApex;
  const double d = -0.5;
  const auto neutral_on = [&](double theta, double z) {
    const gp_Pnt b(60 * std::cos(theta), 60 * std::sin(theta), 0);
    const gp_Vec g = gp_Vec(apex, b).Normalized();
    GeomAPI_ProjectPointOnSurf project(b, surface);
    double u = 0, v = 0;
    project.LowerDistanceParameters(u, v);
    GeomLProp_SLProps props(surface, u, v, 1, 1e-9);
    gp_Vec n(props.Normal());
    if (n.Dot(gp_Vec(std::cos(theta), std::sin(theta), 0)) < 0) n.Reverse();
    const gp_Pnt start = b.Translated(d * n);
    return start.Translated(g * ((z - start.Z()) / g.Z()));
  };
  double bottom = 0, top = 0;
  constexpr int kSteps = 20000;
  for (int i = 0; i < kSteps; ++i) {
    const double a = 2 * test::kPi * i / kSteps, b = 2 * test::kPi * (i + 1) / kSteps;
    bottom += neutral_on(a, 0).Distance(neutral_on(b, 0));
    top += neutral_on(a, 60).Distance(neutral_on(b, 60));
  }
  return {bottom, top};
}

// What the developments of an eccentric frustum's faces measure: the
// lengths of their bottom and top edges, their generators' lengths and
// their area.
struct Measured {
  double bottom = 0, top = 0, area = 0;
  std::vector<double> generators;
};

// One face's development added to `out`: each edge from the start of its
// range to its end, with its ends and its part of Green's area (the
// integral of (x dy - y dx) / 2), the area summed round the loop.
void measure(const std::vector<BoundedCurve2>& edges, Measured& out) {
  struct Piece {
    gp_Pnt2d first, last;
    double area = 0;
    bool used = false;
  };
  std::vector<Piece> pieces;
  for (const auto& edge : edges) {
    const Segment& s = edge.geometry;
    Piece piece;
    if (s.kind == SegmentKind::Line) {
      out.generators.push_back(s.start.Distance(s.end));
      const auto at = [&](double t) { return gp_Pnt2d(s.start.XY() + t * (s.end.XY() - s.start.XY())); };
      piece.first = at(edge.range.first);
      piece.last = at(edge.range.last);
      piece.area = (piece.first.X() * piece.last.Y() - piece.last.X() * piece.first.Y()) / 2;
      pieces.push_back(piece);
      continue;
    }
    CHECK(s.kind == SegmentKind::BSpline);
    if (s.kind != SegmentKind::BSpline) continue;
    const auto curve = spline_of(s);
    const double a = edge.range.first, b = edge.range.last;
    const double length = GCPnts_AbscissaPoint::Length(Geom2dAdaptor_Curve(curve), std::min(a, b), std::max(a, b));
    const gp_Pnt2d middle = curve->Value((a + b) / 2);
    // The bottom edge lies farther from the apex (about 125 against 63).
    (std::hypot(middle.X(), middle.Y()) > 94 ? out.bottom : out.top) += length;
    constexpr int kParts = 4000;
    for (int i = 0; i < kParts; ++i) {
      const gp_Pnt2d p = curve->Value(a + (b - a) * i / kParts), q = curve->Value(a + (b - a) * (i + 1) / kParts);
      piece.area += (p.X() * q.Y() - q.X() * p.Y()) / 2;
    }
    piece.first = curve->Value(a);
    piece.last = curve->Value(b);
    pieces.push_back(piece);
  }
  // Round the loop: each next piece forwards or backwards from where the
  // last one ended.
  double area = 0;
  if (!pieces.empty()) {
    pieces.front().used = true;
    area = pieces.front().area;
    gp_Pnt2d at = pieces.front().last;
    for (std::size_t k = 1; k < pieces.size(); ++k) {
      bool found = false;
      for (auto& piece : pieces) {
        if (piece.used) continue;
        if (piece.first.Distance(at) <= 1e-6) {
          area += piece.area;
          at = piece.last;
        } else if (piece.last.Distance(at) <= 1e-6) {
          area -= piece.area;
          at = piece.first;
        } else {
          continue;
        }
        piece.used = true;
        found = true;
        break;
      }
      CHECK(found);
      if (!found) break;
    }
    CHECK(at.Distance(pieces.front().first) <= 1e-6);
  }
  out.area += std::abs(area);
}

// The side of a loft (its one face that is no plane).
TopoDS_Face side_of(const TopoDS_Shape& loft) {
  TopoDS_Face side;
  for (TopExp_Explorer e(loft, TopAbs_FACE); e.More(); e.Next()) {
    if (BRepAdaptor_Surface(TopoDS::Face(e.Current())).GetType() != GeomAbs_Plane) side = TopoDS::Face(e.Current());
  }
  return side;
}

// The developed lengths and area against the neutral surface's: both
// edges as long as its curves in the end planes, the area the integral of
// (rho |g'| - d (n . g'') / |g'|) over the generators between the planes,
// round the cone.
void check_frustum(const Measured& measured, const std::pair<double, double>& neutral, const char* what) {
  std::printf("%s: edges %.6f / %.6f developed, %.6f / %.6f at the neutral surface, area %.4f\n", what,
              measured.bottom, measured.top, neutral.first, neutral.second, measured.area);
  CHECK(std::abs(measured.bottom - neutral.first) <= 1e-5);
  CHECK(std::abs(measured.top - neutral.second) <= 1e-5);
  CHECK(std::abs(measured.area - 19060.048223272675) <= 1e-2);
}

void eccentric_frustum() {
  const TopoDS_Shape loft = frustum_loft(false);
  if (loft.IsNull()) return;
  const Shape frustum = named(loft, "E:side");
  const SheetFaceDevelopment developed = sheet_face_development(frustum, {"E:side"}, 0.5, 1);
  CHECK(developed.oblique);
  CHECK(!developed.cone);
  CHECK(developed.closed);
  CHECK(!developed.inner_is_front);
  CHECK(developed.origin.Distance(kApex) < 1e-6);
  CHECK(developed.faces.size() == 1);
  if (developed.faces.size() != 1) return;
  Measured measured;
  measure(developed.faces.front(), measured);
  // The seam: two lines along the generator from (60, 0, 0) to (50, 0, 60).
  CHECK(measured.generators.size() == 2);
  for (const double length : measured.generators) CHECK(std::abs(length - std::sqrt(10.0 * 10 + 60 * 60)) <= 1e-6);
  check_frustum(measured, neutral_edges(side_of(loft)), "eccentric frustum");
}

// The same frustum whose side is two faces of their own names, meeting
// along the generators from (0, +-60, 0) to (20, +-30, 60), 70 long
// (mitcad#2559): developed together as one panel of the cone, each face
// its own loop, joined along one of those generators and opened along the
// other; the edges and the area as the one face's.
void eccentric_frustum_two_faces() {
  const TopoDS_Shape loft = frustum_loft(true);
  if (loft.IsNull()) return;
  std::vector<Shape::NamedFace> faces;
  for (TopExp_Explorer e(loft, TopAbs_FACE); e.More(); e.Next()) {
    if (BRepAdaptor_Surface(TopoDS::Face(e.Current())).GetType() == GeomAbs_Plane) continue;
    faces.push_back({e.Current(), {faces.empty() ? "E:one" : "E:two"}});
  }
  CHECK(faces.size() == 2);
  if (faces.size() != 2) return;
  const Shape frustum(loft, faces);
  const SheetFaceDevelopment developed = sheet_face_development(frustum, {"E:one", "E:two"}, 0.5, 1);
  CHECK(developed.oblique);
  CHECK(!developed.cone);
  CHECK(developed.closed);
  CHECK(!developed.inner_is_front);
  CHECK(developed.origin.Distance(kApex) < 1e-6);
  CHECK(developed.faces.size() == 2);
  Measured measured;
  for (const auto& face : developed.faces) measure(face, measured);
  CHECK(measured.generators.size() == 4);
  for (const double length : measured.generators) CHECK(std::abs(length - 70) <= 1e-6);
  const TopoDS_Shape whole = frustum_loft(false);
  if (whole.IsNull()) return;
  check_frustum(measured, neutral_edges(side_of(whole)), "eccentric frustum of two faces");
}

// A ruled surface whose generators do not meet at one point is no cone:
// the upper circle starts a quarter turn on, and the loft keeps the
// sections as they are (no alignment of their starts, which would make it
// a cone of revolution).
void twisted_refused() {
  BRepOffsetAPI_ThruSections loft(true, true);
  loft.CheckCompatibility(false);
  loft.AddWire(BRepBuilderAPI_MakeWire(
                   BRepBuilderAPI_MakeEdge(gp_Circ(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1), gp_Dir(1, 0, 0)), 60)))
                   .Wire());
  loft.AddWire(BRepBuilderAPI_MakeWire(
                   BRepBuilderAPI_MakeEdge(gp_Circ(gp_Ax2(gp_Pnt(0, 0, 60), gp_Dir(0, 0, 1), gp_Dir(0, 1, 0)), 30)))
                   .Wire());
  loft.Build();
  CHECK(loft.IsDone());
  if (!loft.IsDone()) return;
  bool refused = false;
  try {
    (void)sheet_face_development(named(loft.Shape(), "T:side"), {"T:side"}, 0.5, 1);
  } catch (const std::exception&) {
    refused = true;
  }
  CHECK(refused);
}
} // namespace

MITCAD_BUNDLED_TEST(sheet_mitred_oblique) {
  enable_input_checks(true);
  test::guarded("mitred elbow", mitred_elbow);
  test::guarded("eccentric frustum", eccentric_frustum);
  test::guarded("eccentric frustum of two faces", eccentric_frustum_two_faces);
  test::guarded("twisted ruled surface refused", twisted_refused);
  CHECK(input_changes().empty());
  if (test::failures == 0) std::puts("test_sheet_mitred_oblique: all checks passed");
  return test::failures == 0 ? 0 : 1;
}
