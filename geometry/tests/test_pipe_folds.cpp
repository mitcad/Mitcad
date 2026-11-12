// SPDX-License-Identifier: MIT
// Pipes and sweeps that pass through themselves (mitcad#1697): each is one
// valid solid, without faces crossing each other, that holds the union of
// its pieces, built here on their own from OCCT's primitives and fused
// (within 1e-6 relative), and it says it was built in pieces.
//
// - A circle of radius 5 along an arc of radius 3 (three quarters of a
//   turn), alone: the half of the circle on the arc's side of its axis
//   turns round the axis, the other half round the other way, and the two
//   overlap where their turns meet. By Pappus, the half A on the far side
//   holds 3pi/2 M_A, the near half B adds its own first moment M_B times
//   the quarter turn it sweeps outside A (B turned half a turn lies inside
//   A): M_A - M_B = pi r^2 rho, M_B = 2/3 (r^2 - rho^2)^(3/2) -
//   rho (r^2 (pi/2 - asin(rho/r)) - rho sqrt(r^2 - rho^2)).
// - The same circle round a whole circle of radius 3: the far half turned
//   a whole turn holds the near half's, 2pi M_A.
// - The same circle along a 40 mm leg, a quarter of that arc and another
//   40 mm leg: two cylinders and the two halves turned a quarter turn;
//   with a 1 mm wall, the hollow pieces (a wall fills another's bore).
// - A 10 mm square along a leg, a chamfer 2.83 mm long turning 45 degrees
//   at each end (its miters need 4.14 mm) and another leg: the three
//   pieces ending square at the corners and, at each corner, the miter's
//   tip beyond them (a quadrilateral up to where the outer sides meet).
// - A circle of radius 4 along a leg, three quarters of a turn of radius 15
//   and a leg back across the first one: two cylinders and a part of a
//   torus.
// Each as a pipe and as a sweep of a sketch profile. A pipe whose corner
// turns right back on itself is refused in words, with the corner as its
// place.
//
// (mitcad#1708) Cases #1697 left, each as a pipe and as a sweep, solid
// and with a 1 mm wall:
// - A 10 mm square along a polyline in no plane, its leg between a corner
//   in one plane and a corner turning up too short for its miters: the
//   legs ending square at the corners, the section turned at each corner by
//   the least rotation, and the miters' tips beyond them.
// - A 10 mm square along a leg, an arc of radius 6 turning 30 degrees
//   between two 45 degree corners whose miters cut into it, and a leg: the
//   legs and the square turned about the arc's axis, ending square, and
//   the miters' tips.
// - A circle of radius 5 along a leg, a quarter arc of radius 3 rising out
//   of the leg's plane and two legs round a corner: the first leg's rod,
//   the circle's sides turned about the arc's axis, and the two last legs
//   mitered (a round section: the frame it is swept in does not matter).

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <functional>
#include <string>
#include <vector>

#include <BRepAlgoAPI_Check.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_List.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gce_MakeCirc.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>

#include "check.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

ModelCurve line3(const gp_Pnt& a, const gp_Pnt& b) {
  ModelCurve curve;
  curve.kind = ModelCurveKind::Line;
  curve.start = a;
  curve.end = b;
  return curve;
}

// An arc in the XY plane, counter-clockwise from `first` to `last`.
ModelCurve arc3(const gp_Pnt& center, double radius, double first, double last) {
  ModelCurve curve;
  curve.kind = ModelCurveKind::Conic;
  curve.center = center;
  curve.normal = gp_Dir(0, 0, 1);
  curve.x_axis = gp_Dir(1, 0, 0);
  curve.major = curve.minor = radius;
  curve.first = first;
  curve.last = last;
  curve.closed = last - first >= 2 * kPi - 1e-12;
  return curve;
}

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

TopoDS_Shape fused(const std::vector<TopoDS_Shape>& pieces) {
  BRepAlgoAPI_Fuse fuse;
  NCollection_List<TopoDS_Shape> arguments;
  arguments.Append(pieces.front());
  NCollection_List<TopoDS_Shape> tools;
  for (std::size_t i = 1; i < pieces.size(); ++i) {
    tools.Append(pieces[i]);
  }
  fuse.SetArguments(arguments);
  fuse.SetTools(tools);
  fuse.Build();
  CHECK(fuse.IsDone() && !fuse.HasErrors());
  return fuse.Shape();
}

// A cylinder of radius r along the segment from a to b.
TopoDS_Shape rod(const gp_Pnt& a, const gp_Pnt& b, double r) {
  return BRepPrimAPI_MakeCylinder(gp_Ax2(a, gp_Dir(gp_Vec(a, b))), r, a.Distance(b)).Shape();
}

// The disk of radius r about `center` square to `normal`, the part of it
// on either side of the plane through `cut` square to `across`, each turned
// by `angle` about the axis.
std::vector<TopoDS_Shape> halves_turned(const gp_Pnt& center, const gp_Dir& normal, double r, const gp_Pnt& cut,
                                        const gp_Dir& across, const gp_Ax1& axis, double angle) {
  const TopoDS_Edge rim = BRepBuilderAPI_MakeEdge(gce_MakeCirc(center, normal, r).Value()).Edge();
  const TopoDS_Face disk = BRepBuilderAPI_MakeFace(BRepBuilderAPI_MakeWire(rim).Wire(), true).Face();
  std::vector<TopoDS_Shape> out;
  for (const double side : {1.0, -1.0}) {
    // A box of 40 mm on one side of the plane.
    const gp_Dir way = side > 0 ? across : across.Reversed();
    const gp_Ax2 box_frame(cut, way);
    const gp_Pnt corner = cut.Translated(gp_Vec(box_frame.XDirection()) * -20.0)
                              .Translated(gp_Vec(box_frame.YDirection()) * -20.0);
    const TopoDS_Shape box = BRepPrimAPI_MakeBox(gp_Ax2(corner, way, box_frame.XDirection()), 40, 40, 40).Shape();
    const TopoDS_Shape half = BRepAlgoAPI_Common(disk, box).Shape();
    for (TopExp_Explorer f(half, TopAbs_FACE); f.More(); f.Next()) {
      out.push_back(BRepPrimAPI_MakeRevol(f.Current(), axis, angle).Shape());
    }
  }
  return out;
}

// A prism of height 2h (z from -h to h) over the polygon in the XY plane.
TopoDS_Shape slab(const std::vector<gp_Pnt>& corners, double h) {
  BRepBuilderAPI_MakePolygon polygon;
  for (const gp_Pnt& p : corners) {
    polygon.Add(gp_Pnt(p.X(), p.Y(), -h));
  }
  polygon.Close();
  return BRepPrimAPI_MakePrism(BRepBuilderAPI_MakeFace(polygon.Wire(), true).Face(), gp_Vec(0, 0, 2 * h)).Shape();
}

// A leg of a square section of half side h from a to b in the XY plane,
// ending square.
TopoDS_Shape leg(const gp_Pnt& a, const gp_Pnt& b, double h) {
  const gp_Vec d = gp_Vec(a, b).Normalized();
  const gp_Vec n(-d.Y(), d.X(), 0.0);
  return slab({a.Translated(n * h), a.Translated(n * -h), b.Translated(n * -h), b.Translated(n * h)}, h);
}

// The miter's tip beyond two legs of a square section meeting at a corner
// that turns left from `in` to `out`: from the corner to the outer sides'
// ends at it, out to where those sides meet.
TopoDS_Shape miter_tip(const gp_Pnt& corner, const gp_Vec& in, const gp_Vec& out, double h) {
  const gp_Vec a = in.Normalized();
  const gp_Vec b = out.Normalized();
  const gp_Vec ua(a.Y(), -a.X(), 0.0);
  const gp_Vec ub(b.Y(), -b.X(), 0.0);
  const gp_Pnt tip = corner.Translated((ua + ub) * (h / (1.0 + a.Dot(b))));
  return slab({corner, corner.Translated(ua * h), tip, corner.Translated(ub * h)}, h);
}

// One valid solid whose faces do not cross, holding `expected`, that says
// it was built in pieces.
void check_resolved(const ShapePtr& shape, double expected, const char* what) {
  const TopoDS_Shape& s = shape->occt();
  int solids = 0;
  for (TopExp_Explorer e(s, TopAbs_SOLID); e.More(); e.Next()) {
    ++solids;
  }
  const bool valid = BRepCheck_Analyzer(s).IsValid();
  const bool apart = BRepAlgoAPI_Check(s, false, true).IsValid();
  const double volume = volume_of(s);
  std::printf("%s: %d solid(s), valid %d, faces apart %d, volume %.9f, the pieces' union %.9f\n", what, solids,
              valid ? 1 : 0, apart ? 1 : 0, volume, expected);
  CHECK(solids == 1);
  CHECK(valid);
  CHECK(apart);
  CHECK_NEAR_TOL(volume, expected, 1e-6);
  bool said = false;
  for (const std::string& info : shape->infos()) {
    said = said || info.find("built in pieces") != std::string::npos;
  }
  CHECK(said);
}

// The paths.
Path arc_alone() { return {{"c1", arc3(gp_Pnt(0, 0, 0), 3, 0, 1.5 * kPi)}}; }

Path tight_l() {
  return {{"c1", line3(gp_Pnt(-40, 0, 0), gp_Pnt(0, 0, 0))},
          {"c2", arc3(gp_Pnt(0, 3, 0), 3, -kPi / 2, 0)},
          {"c3", line3(gp_Pnt(3, 3, 0), gp_Pnt(3, 43, 0))}};
}

Path chamfer() {
  return {{"c1", line3(gp_Pnt(0, 0, 0), gp_Pnt(38, 0, 0))},
          {"c2", line3(gp_Pnt(38, 0, 0), gp_Pnt(40, 2, 0))},
          {"c3", line3(gp_Pnt(40, 2, 0), gp_Pnt(40, 30, 0))}};
}

Path loop_back() {
  return {{"c1", line3(gp_Pnt(0, 0, 0), gp_Pnt(60, 0, 0))},
          {"c2", arc3(gp_Pnt(60, 15, 0), 15, -kPi / 2, kPi)},
          {"c3", line3(gp_Pnt(45, 15, 0), gp_Pnt(45, -30, 0))}};
}

// The unions of the pieces.
double arc_alone_union() {
  const double r = 5;
  const double rho = 3;
  const double mb = 2.0 / 3.0 * std::pow(r * r - rho * rho, 1.5) -
                    rho * (r * r * (kPi / 2 - std::asin(rho / r)) - rho * std::sqrt(r * r - rho * rho));
  const double ma = kPi * r * r * rho + mb;
  return 1.5 * kPi * ma + 0.5 * kPi * mb;
}

double tight_l_union() {
  std::vector<TopoDS_Shape> pieces{rod(gp_Pnt(-40, 0, 0), gp_Pnt(0, 0, 0), 5), rod(gp_Pnt(3, 3, 0), gp_Pnt(3, 43, 0), 5)};
  for (const TopoDS_Shape& half : halves_turned(gp_Pnt(0, 0, 0), gp_Dir(1, 0, 0), 5, gp_Pnt(0, 3, 0), gp_Dir(0, 1, 0),
                                                gp_Ax1(gp_Pnt(0, 3, 0), gp_Dir(0, 0, 1)), kPi / 2)) {
    pieces.push_back(half);
  }
  return volume_of(fused(pieces));
}

double chamfer_union() {
  const double h = 5;
  const gp_Pnt p0(0, 0, 0);
  const gp_Pnt p1(38, 0, 0);
  const gp_Pnt p2(40, 2, 0);
  const gp_Pnt p3(40, 30, 0);
  return volume_of(fused({leg(p0, p1, h), leg(p1, p2, h), leg(p2, p3, h),
                          miter_tip(p1, gp_Vec(p0, p1), gp_Vec(p1, p2), h),
                          miter_tip(p2, gp_Vec(p1, p2), gp_Vec(p2, p3), h)}));
}

double loop_back_union() {
  // The circle at (60, 0) square to x turned three quarters of a turn
  // about the arc's axis.
  const TopoDS_Edge rim = BRepBuilderAPI_MakeEdge(gce_MakeCirc(gp_Pnt(60, 0, 0), gp_Dir(1, 0, 0), 4).Value()).Edge();
  const TopoDS_Face disk = BRepBuilderAPI_MakeFace(BRepBuilderAPI_MakeWire(rim).Wire(), true).Face();
  const TopoDS_Shape torus = BRepPrimAPI_MakeRevol(disk, gp_Ax1(gp_Pnt(60, 15, 0), gp_Dir(0, 0, 1)), 1.5 * kPi).Shape();
  return volume_of(fused({rod(gp_Pnt(0, 0, 0), gp_Pnt(60, 0, 0), 4), torus, rod(gp_Pnt(45, 15, 0), gp_Pnt(45, -30, 0), 4)}));
}

ShapePtr a_pipe(Path path, PipeSection section, double size) {
  PipeSpec spec;
  spec.feature = "F2";
  spec.path = std::move(path);
  spec.section = section;
  spec.size = size;
  return pipe(spec);
}

// A sweep of a circle (or a square of that half side) about the path's
// start, square to its first direction `along` (x or y).
ShapePtr a_sweep(Path path, bool square, double r, bool along_y) {
  SweepSpec spec;
  spec.feature = "F3";
  const ModelCurve& first = path.front().curve;
  spec.frame.origin = first.kind == ModelCurveKind::Line
                          ? first.start
                          : gp_Pnt(first.center.X() + first.major * std::cos(first.first),
                                   first.center.Y() + first.major * std::sin(first.first), 0.0);
  if (along_y) {
    spec.frame.x_axis = gp_Dir(0, 0, 1);
    spec.frame.y_axis = gp_Dir(1, 0, 0);
  } else {
    spec.frame.x_axis = gp_Dir(0, 1, 0);
    spec.frame.y_axis = gp_Dir(0, 0, 1);
  }
  Region region = square ? rectangle_region(-r, -r, 2 * r, 2 * r) : circle_region(0, 0, r);
  region.name = "r1";
  for (std::size_t i = 0; i < region.loops[0].segments.size(); ++i) {
    region.loops[0].segments[i].name = "c" + std::to_string(10 + i);
  }
  spec.regions = {region};
  spec.path = std::move(path);
  return sweep(spec);
}

void test_tight_arc() {
  check_resolved(a_pipe(arc_alone(), PipeSection::Circular, 10), arc_alone_union(), "pipe, tight arc");
  check_resolved(a_sweep(arc_alone(), false, 5, true), arc_alone_union(), "sweep, tight arc");
}

void test_tight_ring() {
  // A whole circle of radius 3: the far half turned a whole turn holds
  // the near half's turn, 2pi M_A.
  const double r = 5;
  const double rho = 3;
  const double mb = 2.0 / 3.0 * std::pow(r * r - rho * rho, 1.5) -
                    rho * (r * r * (kPi / 2 - std::asin(rho / r)) - rho * std::sqrt(r * r - rho * rho));
  const double ma = kPi * r * r * rho + mb;
  check_resolved(a_pipe({{"c1", arc3(gp_Pnt(0, 0, 0), 3, 0, 2 * kPi)}}, PipeSection::Circular, 10), 2 * kPi * ma,
                 "pipe, tight ring");
}

void test_tight_bend() {
  const double expected = tight_l_union();
  const ShapePtr piped = a_pipe(tight_l(), PipeSection::Circular, 10);
  check_resolved(piped, expected, "pipe, tight bend");
  CHECK(!piped->find_faces("F2:start").empty() && !piped->find_faces("F2:end").empty());
  check_resolved(a_sweep(tight_l(), false, 5, false), expected, "sweep, tight bend");
}

void test_hollow_tight_bend() {
  // A 1 mm wall: the union of the hollow pieces (each cylinder less its
  // bore, each half of the ring turned), so the wall of one piece fills the
  // bore of another where they overlap.
  const TopoDS_Edge outer = BRepBuilderAPI_MakeEdge(gce_MakeCirc(gp_Pnt(0, 0, 0), gp_Dir(1, 0, 0), 5).Value()).Edge();
  const TopoDS_Edge inner = BRepBuilderAPI_MakeEdge(gce_MakeCirc(gp_Pnt(0, 0, 0), gp_Dir(1, 0, 0), 4).Value()).Edge();
  const TopoDS_Face disk = BRepBuilderAPI_MakeFace(BRepBuilderAPI_MakeWire(outer).Wire(), true).Face();
  const TopoDS_Face bore = BRepBuilderAPI_MakeFace(BRepBuilderAPI_MakeWire(inner).Wire(), true).Face();
  const TopoDS_Shape ring = BRepAlgoAPI_Cut(disk, bore).Shape();
  std::vector<TopoDS_Shape> pieces{BRepAlgoAPI_Cut(rod(gp_Pnt(-40, 0, 0), gp_Pnt(0, 0, 0), 5),
                                                   rod(gp_Pnt(-40, 0, 0), gp_Pnt(0, 0, 0), 4))
                                       .Shape(),
                                   BRepAlgoAPI_Cut(rod(gp_Pnt(3, 3, 0), gp_Pnt(3, 43, 0), 5),
                                                   rod(gp_Pnt(3, 3, 0), gp_Pnt(3, 43, 0), 4))
                                       .Shape()};
  const gp_Ax1 axis(gp_Pnt(0, 3, 0), gp_Dir(0, 0, 1));
  for (const gp_Dir& way : {gp_Dir(0, 1, 0), gp_Dir(0, -1, 0)}) {
    const gp_Ax2 box_frame(gp_Pnt(0, 3, 0), way);
    const gp_Pnt corner = gp_Pnt(0, 3, 0)
                              .Translated(gp_Vec(box_frame.XDirection()) * -20.0)
                              .Translated(gp_Vec(box_frame.YDirection()) * -20.0);
    const TopoDS_Shape box = BRepPrimAPI_MakeBox(gp_Ax2(corner, way, box_frame.XDirection()), 40, 40, 40).Shape();
    for (TopExp_Explorer f(BRepAlgoAPI_Common(ring, box).Shape(), TopAbs_FACE); f.More(); f.Next()) {
      pieces.push_back(BRepPrimAPI_MakeRevol(f.Current(), axis, kPi / 2).Shape());
    }
  }
  PipeSpec spec;
  spec.feature = "F2";
  spec.path = tight_l();
  spec.size = 10;
  spec.thickness = 1;
  const ShapePtr tube = pipe(spec);
  check_resolved(tube, volume_of(fused(pieces)), "pipe, hollow tight bend");
  CHECK(!tube->find_faces("F2:inner0").empty());
}

void test_short_chamfer() {
  const double expected = chamfer_union();
  check_resolved(a_pipe(chamfer(), PipeSection::Square, 10), expected, "pipe, short chamfer");
  check_resolved(a_sweep(chamfer(), true, 5, false), expected, "sweep, short chamfer");
}

void test_loop_back() {
  const double expected = loop_back_union();
  check_resolved(a_pipe(loop_back(), PipeSection::Circular, 8), expected, "pipe, loop back");
  check_resolved(a_sweep(loop_back(), false, 4, false), expected, "sweep, loop back");
}

void test_plain_pipe_unchanged() {
  // A pipe that does not pass through itself says nothing of pieces.
  const ShapePtr plain = a_pipe(tight_l(), PipeSection::Circular, 4);
  CHECK(plain->infos().empty());
  CHECK(BRepCheck_Analyzer(plain->occt()).IsValid());
}

void test_turning_back_refused() {
  // A corner that turns 179.6 degrees back: no miter can be built there.
  const Path back{{"c1", line3(gp_Pnt(0, 0, 0), gp_Pnt(40, 0, 0))},
                  {"c2", line3(gp_Pnt(40, 0, 0), gp_Pnt(0, 0.3, 0))}};
  try {
    const ShapePtr shape = a_pipe(back, PipeSection::Square, 10);
    std::printf("turning back: built, valid %d, volume %.6f\n", BRepCheck_Analyzer(shape->occt()).IsValid() ? 1 : 0,
                volume_of(shape->occt()));
    CHECK(false);
  } catch (const std::exception& error) {
    const std::string text = error.what();
    std::printf("turning back: %s\n", text.c_str());
    CHECK(text.find("turns right back at a corner") != std::string::npos);
    CHECK(text.find("\"places\":[{\"kind\":\"point\"") != std::string::npos);
  }
}

// (mitcad#1708) Cases #1697 left: a polyline in no plane with a leg too
// short for its miters, and an arc too short for the miters at its ends. Each as a
// pipe and as a sweep, solid and hollow, against the union of its pieces
// built here on their own.

// A face with the other face's region taken out (a hole), or the face.
TopoDS_Shape holed(const TopoDS_Shape& outer, const TopoDS_Shape& inner, bool hollow) {
  return hollow ? BRepAlgoAPI_Cut(outer, inner).Shape() : outer;
}

// A square of half side h about `c` square to `t`, its sides along x and
// t x x; with a wall of 1 mm, its square bore taken out.
TopoDS_Shape square_section(const gp_Pnt& c, const gp_Dir& t, const gp_Dir& x, double h, bool hollow) {
  const gp_Vec u(x);
  const gp_Vec v = gp_Vec(t).Crossed(u);
  const auto square = [&](double a) {
    BRepBuilderAPI_MakePolygon polygon;
    polygon.Add(c.Translated(u * a + v * a));
    polygon.Add(c.Translated(u * -a + v * a));
    polygon.Add(c.Translated(u * -a + v * -a));
    polygon.Add(c.Translated(u * a + v * -a));
    polygon.Close();
    return BRepBuilderAPI_MakeFace(polygon.Wire(), true).Face();
  };
  return holed(square(h), square(h - 1), hollow);
}

TopoDS_Shape moved(const TopoDS_Shape& shape, const gp_Vec& by) {
  gp_Trsf move;
  move.SetTranslation(by);
  return BRepBuilderAPI_Transform(shape, move, true).Shape();
}

// The region within 500 mm of p on the side of the plane through p
// square to n that n points to.
TopoDS_Shape beyond(const gp_Pnt& p, const gp_Dir& n) {
  const TopoDS_Face plane = BRepBuilderAPI_MakeFace(gp_Pln(p, n), -500, 500, -500, 500).Face();
  return BRepPrimAPI_MakePrism(plane, gp_Vec(n) * 500).Shape();
}

// At a corner from direction a to b, the miter beyond the two legs'
// square ends: the section of the leg before (square to a at the corner)
// carried on past the corner, and the section of the leg after (square to
// b) carried back before it, each up to the plane that halves the corner.
std::vector<TopoDS_Shape> tips(const gp_Pnt& corner, const gp_Dir& a, const gp_Dir& b, const TopoDS_Shape& before,
                               const TopoDS_Shape& after) {
  const gp_Dir half(gp_Vec(a) + gp_Vec(b));
  const double run = 60;
  return {BRepAlgoAPI_Common(BRepPrimAPI_MakePrism(before, gp_Vec(a) * run).Shape(), beyond(corner, half.Reversed()))
              .Shape(),
          BRepAlgoAPI_Common(BRepPrimAPI_MakePrism(moved(after, gp_Vec(b) * -run), gp_Vec(b) * run).Shape(),
                             beyond(corner, half))
              .Shape()};
}

// The frame turned through a corner from a to b by the least rotation.
gp_Dir cornered(const gp_Dir& x, const gp_Dir& a, const gp_Dir& b) {
  gp_Trsf turn;
  turn.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(gp_Vec(a).Crossed(gp_Vec(b)))), a.Angle(b));
  return x.Transformed(turn);
}

ShapePtr pipe_of(Path path, PipeSection section, double size, bool hollow) {
  PipeSpec spec;
  spec.feature = "F2";
  spec.path = std::move(path);
  spec.section = section;
  spec.size = size;
  spec.thickness = hollow ? 1 : 0;
  return pipe(spec);
}

// A sweep of a square of half side r (or a circle of radius r), with a
// 1 mm wall when hollow, about the frame's origin.
ShapePtr sweep_on(Path path, const Frame& frame, bool square, double r, bool hollow) {
  SweepSpec spec;
  spec.feature = "F3";
  spec.frame = frame;
  const auto shape = [&](double a) {
    return square ? rectangle_region(-a, -a, 2 * a, 2 * a) : circle_region(0, 0, a);
  };
  Region region = shape(r);
  if (hollow) {
    region.loops.push_back(shape(r - 1).loops.front());
  }
  region.name = "r1";
  int name = 10;
  for (Loop& loop : region.loops) {
    for (Segment& segment : loop.segments) {
      segment.name = "c" + std::to_string(name++);
    }
  }
  spec.regions = {region};
  spec.path = std::move(path);
  return sweep(spec);
}

// A polyline in no plane: 38 mm along x, a leg 2.83 mm long turning
// 45 degrees in the XY plane (its miters need 4.14 mm) and a leg turning
// up along z; a square of side 10 swept along it, its sides along z and y
// at the start, the sweep's frame turned at each corner by the least
// rotation.
Path skew_polyline() {
  return {{"c1", line3(gp_Pnt(0, 0, 0), gp_Pnt(38, 0, 0))},
          {"c2", line3(gp_Pnt(38, 0, 0), gp_Pnt(40, 2, 0))},
          {"c3", line3(gp_Pnt(40, 2, 0), gp_Pnt(40, 2, 30))}};
}

double skew_union(bool hollow) {
  const std::vector<gp_Pnt> p{gp_Pnt(0, 0, 0), gp_Pnt(38, 0, 0), gp_Pnt(40, 2, 0), gp_Pnt(40, 2, 30)};
  std::vector<gp_Dir> d;
  std::vector<gp_Dir> x{gp_Dir(0, 0, 1)};
  for (std::size_t i = 0; i + 1 < p.size(); ++i) {
    d.emplace_back(gp_Vec(p[i], p[i + 1]));
    if (i > 0) {
      x.push_back(cornered(x.back(), d[i - 1], d[i]));
    }
  }
  std::vector<TopoDS_Shape> pieces;
  for (std::size_t i = 0; i < d.size(); ++i) {
    pieces.push_back(BRepPrimAPI_MakePrism(square_section(p[i], d[i], x[i], 5, hollow), gp_Vec(p[i], p[i + 1])).Shape());
  }
  for (std::size_t i = 1; i < d.size(); ++i) {
    for (const TopoDS_Shape& tip : tips(p[i], d[i - 1], d[i], square_section(p[i], d[i - 1], x[i - 1], 5, hollow),
                                        square_section(p[i], d[i], x[i], 5, hollow))) {
      pieces.push_back(tip);
    }
  }
  return volume_of(fused(pieces));
}

void test_skew_polyline() {
  for (const bool hollow : {false, true}) {
    const double expected = skew_union(hollow);
    check_resolved(pipe_of(skew_polyline(), PipeSection::Square, 10, hollow), expected,
                   hollow ? "pipe, hollow, polyline in no plane" : "pipe, polyline in no plane");
    Frame frame;
    frame.x_axis = gp_Dir(0, 1, 0);
    frame.y_axis = gp_Dir(0, 0, 1);
    check_resolved(sweep_on(skew_polyline(), frame, true, 5, hollow), expected,
                   hollow ? "sweep, hollow, polyline in no plane" : "sweep, polyline in no plane");
  }
}

// A 30 mm leg along x, an arc of radius 6 turning 30 degrees that leaves
// the leg's end 45 degrees to the left, and a leg leaving the arc's end
// another 45 degrees to the left: on the arc's inner side a square of side
// 10 runs 6 x pi/6 x (1 - 5/6) = 0.52 mm, the miters cut 2 x 5 tan 22.5 =
// 4.14 mm into it.
constexpr double kArcRadius = 6;

gp_Pnt arc_centre() {
  return gp_Pnt(30, 0, 0).Translated(gp_Vec(-std::sin(kPi / 4), std::cos(kPi / 4), 0) * kArcRadius);
}

gp_Pnt arc_end() {
  const double angle = -kPi / 4 + kPi / 6;
  return arc_centre().Translated(gp_Vec(std::cos(angle), std::sin(angle), 0) * kArcRadius);
}

Path mitered_arc() {
  const gp_Dir leave(std::cos(2 * kPi / 3), std::sin(2 * kPi / 3), 0);
  return {{"c1", line3(gp_Pnt(0, 0, 0), gp_Pnt(30, 0, 0))},
          {"c2", arc3(arc_centre(), kArcRadius, -kPi / 4, -kPi / 4 + kPi / 6)},
          {"c3", line3(arc_end(), arc_end().Translated(gp_Vec(leave) * 30))}};
}

// The two legs and the arc swept square-ended (the square turned about the
// arc's axis), and each corner's miter beyond them.
double mitered_arc_union(bool hollow) {
  const gp_Dir z(0, 0, 1);
  const gp_Pnt s(30, 0, 0);
  const gp_Pnt e = arc_end();
  const gp_Dir along(1, 0, 0);
  const gp_Dir arc_in(std::cos(kPi / 4), std::sin(kPi / 4), 0);
  const gp_Dir arc_out(std::cos(5 * kPi / 12), std::sin(5 * kPi / 12), 0);
  const gp_Dir leave(std::cos(2 * kPi / 3), std::sin(2 * kPi / 3), 0);
  std::vector<TopoDS_Shape> pieces{
      BRepPrimAPI_MakePrism(square_section(gp_Pnt(0, 0, 0), along, z, 5, hollow), gp_Vec(along) * 30).Shape(),
      BRepPrimAPI_MakeRevol(square_section(s, arc_in, z, 5, hollow), gp_Ax1(arc_centre(), z), kPi / 6).Shape(),
      BRepPrimAPI_MakePrism(square_section(e, leave, z, 5, hollow), gp_Vec(leave) * 30).Shape()};
  for (const TopoDS_Shape& tip :
       tips(s, along, arc_in, square_section(s, along, z, 5, hollow), square_section(s, arc_in, z, 5, hollow))) {
    pieces.push_back(tip);
  }
  for (const TopoDS_Shape& tip :
       tips(e, arc_out, leave, square_section(e, arc_out, z, 5, hollow), square_section(e, leave, z, 5, hollow))) {
    pieces.push_back(tip);
  }
  return volume_of(fused(pieces));
}

void test_mitered_arc() {
  for (const bool hollow : {false, true}) {
    const double expected = mitered_arc_union(hollow);
    check_resolved(pipe_of(mitered_arc(), PipeSection::Square, 10, hollow), expected,
                   hollow ? "pipe, hollow, arc between miters" : "pipe, arc between miters");
    Frame frame;
    frame.x_axis = gp_Dir(0, 1, 0);
    frame.y_axis = gp_Dir(0, 0, 1);
    check_resolved(sweep_on(mitered_arc(), frame, true, 5, hollow), expected,
                   hollow ? "sweep, hollow, arc between miters" : "sweep, arc between miters");
  }
}

// A disk of radius r about `c` square to `t`; with a wall of 1 mm, its
// bore taken out.
TopoDS_Shape disk_section(const gp_Pnt& c, const gp_Dir& t, double r, bool hollow) {
  const auto disk = [&](double a) {
    const TopoDS_Edge rim = BRepBuilderAPI_MakeEdge(gce_MakeCirc(c, t, a).Value()).Edge();
    return BRepBuilderAPI_MakeFace(BRepBuilderAPI_MakeWire(rim).Wire(), true).Face();
  };
  return holed(disk(r), disk(r - 1), hollow);
}

// A 40 mm leg along x, a quarter arc of radius 3 rising in the XZ plane,
// a 40 mm leg up along z and a 40 mm leg along y: a path in no plane with
// a bend that a circle of radius 5 reaches past the centre of, and a
// corner.
Path skew_tight_bend() {
  ModelCurve up;
  up.kind = ModelCurveKind::Conic;
  up.center = gp_Pnt(0, 0, 3);
  up.normal = gp_Dir(0, -1, 0);
  up.x_axis = gp_Dir(1, 0, 0);
  up.major = up.minor = 3;
  up.first = -kPi / 2;
  up.last = 0;
  return {{"c1", line3(gp_Pnt(-40, 0, 0), gp_Pnt(0, 0, 0))},
          {"c2", up},
          {"c3", line3(gp_Pnt(3, 0, 3), gp_Pnt(3, 0, 43))},
          {"c4", line3(gp_Pnt(3, 0, 43), gp_Pnt(3, 40, 43))}};
}

// The first leg's rod, the circle at the arc's start cut along the arc's
// axis with each side turned a quarter turn about it, and the two last
// legs mitered at their corner (not cut there: each leg's rod carried on
// up to the plane that halves the corner, so a hollow pipe's bore runs
// on round it).
double skew_tight_bend_union(bool hollow) {
  const gp_Dir x(1, 0, 0);
  const gp_Dir y(0, 1, 0);
  const gp_Dir z(0, 0, 1);
  const gp_Pnt corner(3, 0, 43);
  const gp_Dir half(gp_Vec(z) + gp_Vec(y));
  std::vector<TopoDS_Shape> pieces{
      BRepPrimAPI_MakePrism(disk_section(gp_Pnt(-40, 0, 0), x, 5, hollow), gp_Vec(x) * 40).Shape(),
      BRepAlgoAPI_Common(BRepPrimAPI_MakePrism(disk_section(gp_Pnt(3, 0, 3), z, 5, hollow), gp_Vec(z) * 100).Shape(),
                         beyond(corner, half.Reversed()))
          .Shape(),
      BRepAlgoAPI_Common(
          BRepPrimAPI_MakePrism(disk_section(corner.Translated(gp_Vec(y) * -60), y, 5, hollow), gp_Vec(y) * 100)
              .Shape(),
          beyond(corner, half))
          .Shape()};
  const TopoDS_Shape section = disk_section(gp_Pnt(0, 0, 0), x, 5, hollow);
  const gp_Ax1 axis(gp_Pnt(0, 0, 3), gp_Dir(0, -1, 0));
  for (const gp_Dir& way : {z, z.Reversed()}) {
    const TopoDS_Shape side = BRepAlgoAPI_Common(section, beyond(gp_Pnt(0, 0, 3), way)).Shape();
    for (TopExp_Explorer f(side, TopAbs_FACE); f.More(); f.Next()) {
      pieces.push_back(BRepPrimAPI_MakeRevol(f.Current(), axis, kPi / 2).Shape());
    }
  }
  return volume_of(fused(pieces));
}

void test_skew_tight_bend() {
  // A round section: the frame the sweep turns it by does not matter.
  for (const bool hollow : {false, true}) {
    const double expected = skew_tight_bend_union(hollow);
    check_resolved(pipe_of(skew_tight_bend(), PipeSection::Circular, 10, hollow), expected,
                   hollow ? "pipe, hollow, tight bend in no plane" : "pipe, tight bend in no plane");
    Frame frame;
    frame.origin = gp_Pnt(-40, 0, 0);
    frame.x_axis = gp_Dir(0, 1, 0);
    frame.y_axis = gp_Dir(0, 0, 1);
    check_resolved(sweep_on(skew_tight_bend(), frame, false, 5, hollow), expected,
                   hollow ? "sweep, hollow, tight bend in no plane" : "sweep, tight bend in no plane");
  }
}

// (mitcad#1709) A tight bend of a curve whose bend changes along it, inside
// a longer path in one plane: a 40 mm leg, the curve and a 40 mm leg, a
// circle of radius 2 swept along it (with a 1 mm wall). The curve's bend is
// tightest inside it, with a radius below 2, so the circle reaches past
// the centre of the bend there and the tube its rim sweeps meets itself.
// The solid holds the points within 2 of the path (each lies in the circle
// at the point of the path nearest to it), less, with the wall, those
// within 1 (the bore's radius is below the tightest bend's, so no wall
// fills it). Its volume within a box round the bend is checked against the
// sampled integral of the height of that set over the box's floor
// (2 sqrt(4 - d^2) less 2 sqrt(1 - d^2) with the wall, d the distance in
// the plane to the path), on a 0.01 mm grid, within 1e-5 relative (the
// grid's own error: 2.3e-6 against a 0.005 mm grid);
// and points sampled round the bend lie in the solid exactly when they lie
// within its reach of the path. The sweep, the same tube whatever its
// profile's seam, matches the pipe's volume within 1e-9.
// - An ellipse of half axes 6 and 3 from -0.8 to 0.8 rad round the end of
//   its long axis, where its bend's radius is 1.5.
// - A cubic spline whose bend is tightest (radius 1.51) off its middle,
//   so not symmetric about it.
// Each as a pipe and as a sweep of a circle whose seam lies elsewhere.

// A curve of a path in one plane as a function of its parameter.
struct PlaneBend {
  Path path;
  std::function<gp_Pnt(double)> curve;
  double first = 0;
  double last = 0;
};

ModelCurve spline3(const std::vector<gp_Pnt>& poles) {
  ModelCurve curve;
  curve.kind = ModelCurveKind::BSpline;
  curve.degree = 3;
  curve.poles = poles;
  curve.knots = {0, 0, 0, 0, 1, 1, 1, 1};
  return curve;
}

PlaneBend with_legs(const ModelCurve& middle, const std::function<gp_Pnt(double)>& curve, double first, double last) {
  const double h = 1e-6;
  const gp_Pnt a = curve(first);
  const gp_Pnt b = curve(last);
  const gp_Vec ta = gp_Vec(curve(first), curve(first + h)).Normalized();
  const gp_Vec tb = gp_Vec(curve(last - h), curve(last)).Normalized();
  PlaneBend bend;
  bend.path = {{"c1", line3(a.Translated(ta * -40), a)}, {"c2", middle}, {"c3", line3(b, b.Translated(tb * 40))}};
  bend.curve = curve;
  bend.first = first;
  bend.last = last;
  return bend;
}

PlaneBend ellipse_bend() {
  ModelCurve e;
  e.kind = ModelCurveKind::Conic;
  e.center = gp_Pnt(0, 0, 0);
  e.normal = gp_Dir(0, 0, 1);
  e.x_axis = gp_Dir(1, 0, 0);
  e.major = 6;
  e.minor = 3;
  e.first = -0.8;
  e.last = 0.8;
  return with_legs(e, [](double t) { return gp_Pnt(6 * std::cos(t), 3 * std::sin(t), 0); }, -0.8, 0.8);
}

const std::vector<gp_Pnt>& spline_poles() {
  static const std::vector<gp_Pnt> poles{gp_Pnt(-4, -3, 0), gp_Pnt(3, -3, 0), gp_Pnt(2.5, -0.5, 0), gp_Pnt(-1, 4, 0)};
  return poles;
}

PlaneBend spline_bend() {
  const std::vector<gp_Pnt>& p = spline_poles();
  return with_legs(spline3(p),
                   [&p](double t) {
                     const double u = 1 - t;
                     return gp_Pnt(u * u * u * p[0].XYZ() + 3 * u * u * t * p[1].XYZ() + 3 * u * t * t * p[2].XYZ() +
                                   t * t * t * p[3].XYZ());
                   },
                   0, 1);
}

// The distance in the plane from (x, y) to the path: its legs, and the
// curve sampled and refined about its nearest sample.
double plane_distance(const PlaneBend& bend, double x, double y) {
  const gp_Pnt p(x, y, 0);
  double best = 1e100;
  for (const PathCurve& leg : {bend.path[0], bend.path[2]}) {
    const gp_Vec d(leg.curve.start, leg.curve.end);
    const double t = std::clamp(gp_Vec(leg.curve.start, p).Dot(d) / d.SquareMagnitude(), 0.0, 1.0);
    best = std::min(best, p.Distance(leg.curve.start.Translated(d * t)));
  }
  const int n = 64;
  int nearest = 0;
  double closest = 1e100;
  for (int k = 0; k <= n; ++k) {
    const double d = p.Distance(bend.curve(bend.first + (bend.last - bend.first) * k / n));
    if (d < closest) {
      closest = d;
      nearest = k;
    }
  }
  double lo = bend.first + (bend.last - bend.first) * std::max(0, nearest - 1) / n;
  double hi = bend.first + (bend.last - bend.first) * std::min(n, nearest + 1) / n;
  for (int i = 0; i < 100; ++i) {
    const double m1 = lo + (hi - lo) / 3;
    const double m2 = hi - (hi - lo) / 3;
    if (p.Distance(bend.curve(m1)) < p.Distance(bend.curve(m2))) {
      hi = m2;
    } else {
      lo = m1;
    }
  }
  return std::min(best, p.Distance(bend.curve((lo + hi) / 2)));
}

// The height of the swept set over (x, y): within 2 of the path, less
// within 1 with the wall.
double swept_height(const PlaneBend& bend, double x, double y, bool hollow) {
  const double d = plane_distance(bend, x, y);
  const double outer = d < 2 ? 2 * std::sqrt(4 - d * d) : 0.0;
  const double bore = hollow && d < 1 ? 2 * std::sqrt(1 - d * d) : 0.0;
  return outer - bore;
}

double precise_volume(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1e-10);
  return props.Mass();
}

void check_plane_bend(const PlaneBend& bend, const gp_Pnt& low, const gp_Pnt& high, const char* name) {
  const double h = 0.01;
  const int points = 200;
  for (const bool hollow : {false, true}) {
    double reference = 0;
    for (double x = low.X() + h / 2; x < high.X(); x += h) {
      for (double y = low.Y() + h / 2; y < high.Y(); y += h) {
        reference += swept_height(bend, x, y, hollow) * h * h;
      }
    }
    const TopoDS_Shape box = BRepPrimAPI_MakeBox(low, high).Shape();
    Frame frame;
    frame.origin = bend.path[0].curve.start;
    const gp_Dir along(gp_Vec(bend.path[0].curve.start, bend.path[0].curve.end));
    {
      // The profile's seam turned away from the pipe's.
      gp_Trsf spin;
      spin.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), along), 1.3);
      frame.x_axis = gp_Dir(0, 0, 1).Transformed(spin);
      frame.y_axis = gp_Dir(gp_Vec(along).Crossed(gp_Vec(frame.x_axis)));
    }
    double piped_window = 0;
    for (const bool piped : {true, false}) {
      const std::string what = std::string(piped ? "pipe, " : "sweep, ") + (hollow ? "hollow, " : "") + name;
      const ShapePtr shape = piped ? pipe_of(bend.path, PipeSection::Circular, 4, hollow)
                                   : sweep_on(bend.path, frame, false, 2, hollow);
      const TopoDS_Shape& s = shape->occt();
      int solids = 0;
      for (TopExp_Explorer e(s, TopAbs_SOLID); e.More(); e.Next()) {
        ++solids;
      }
      const bool valid = BRepCheck_Analyzer(s).IsValid();
      const double window = precise_volume(BRepAlgoAPI_Common(s, box).Shape());
      bool said = false;
      for (const std::string& info : shape->infos()) {
        said = said || info.find("built in pieces") != std::string::npos;
      }
      CHECK(solids == 1);
      CHECK(valid);
      CHECK(said);
      if (!piped) {
        // The sweep is the same tube (it does not depend on where the
        // profile's seam lies): the pipe's checks hold for it.
        std::printf("%s: %d solid(s), valid %d, volume in the box %.9f, the pipe's %.9f\n", what.c_str(), solids,
                    valid ? 1 : 0, window, piped_window);
        CHECK_NEAR_TOL(window, piped_window, 1e-9);
        continue;
      }
      piped_window = window;
      const bool apart = BRepAlgoAPI_Check(s, false, true).IsValid();
      std::printf("%s: %d solid(s), valid %d, faces apart %d, volume in the box %.9f, sampled %.9f (%.2e)\n",
                  what.c_str(), solids, valid ? 1 : 0, apart ? 1 : 0, window, reference,
                  (window - reference) / reference);
      CHECK(apart);
      CHECK_NEAR_TOL(window, reference, 1e-5);
      // Points round the bend: in the solid exactly when within reach.
      unsigned long long seed = 12345;
      const auto next = [&] {
        seed = seed * 6364136223846793005ULL + 1442695040888963407ULL;
        return static_cast<double>(seed >> 11) / static_cast<double>(1ULL << 53);
      };
      int wrong = 0;
      for (int k = 0; k < points; ++k) {
        const gp_Pnt p(low.X() + (high.X() - low.X()) * next(), low.Y() + (high.Y() - low.Y()) * next(),
                       -2.2 + 4.4 * next());
        const double d = plane_distance(bend, p.X(), p.Y());
        const double reach = d * d + p.Z() * p.Z();
        const bool in_sweep = reach <= 4 && (!hollow || reach >= 1);
        BRepClass3d_SolidClassifier classifier(s, p, 1e-7);
        if ((classifier.State() == TopAbs_IN) != in_sweep) {
          BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(p).Vertex(), s);
          if (distance.Value() > 1e-5) {
            ++wrong;
            std::printf("  misplaced (%.6f %.6f %.6f): %s the solid, %.6f from its faces, reach %.9f\n", p.X(), p.Y(),
                        p.Z(), classifier.State() == TopAbs_IN ? "in" : "out of", distance.Value(), std::sqrt(reach));
          }
        }
      }
      std::printf("%s: %d of %d sampled points misplaced\n", what.c_str(), wrong, points);
      CHECK(wrong == 0);
    }
  }
}

void test_ellipse_bend() { check_plane_bend(ellipse_bend(), gp_Pnt(2, -3.5, -3), gp_Pnt(9, 3.5, 3), "tight ellipse"); }

void test_spline_bend() { check_plane_bend(spline_bend(), gp_Pnt(-1, -5, -3), gp_Pnt(6, 3, 3), "tight spline"); }

// (mitcad#1709) A tight bend of a helix inside a longer path: a 40 mm leg,
// a quarter turn of a helix of radius 2.2 rising 6 mm a turn about z (its
// bend's radius 2.2 + c^2 / 2.2 = 2.61, c = 6 / 2pi) and a 40 mm leg, a
// circle of radius 3 (with a 1 mm wall, its bore's radius below the
// bend's) swept along it, as a pipe and as a sweep with the profile's seam
// turned three ways. The circle reaches past the centre of the bend, so
// the tube its rim sweeps meets itself round the inside of the bend. The
// solid holds the points within 3 of the path, less, with the wall, those
// within 2: one valid solid without faces crossing each other, the same
// wherever the seam lies, whose volume matches that set's, sampled along
// the rays from the path (helix_union_volume), within 3e-7 relative (the
// sampling's own error, against twice as many stations and angles:
// 1.2e-4 mm3, 5e-8 of the solid's volume, 9e-8 of the hollow's; the part
// of the tube that lies inside, where it meets itself, is 0.032 mm3, 1.4e-5
// of the volume); and points sampled round the bend lie in the solid
// exactly when within reach of the path.
constexpr double kHelixRadius = 2.2;
constexpr double kHelixPitch = 6;

gp_Pnt helix_point(double t) {
  return gp_Pnt(kHelixRadius * std::cos(t), kHelixRadius * std::sin(t), kHelixPitch * t / (2 * kPi));
}

gp_Vec helix_tangent(double t) {
  return gp_Vec(-kHelixRadius * std::sin(t), kHelixRadius * std::cos(t), kHelixPitch / (2 * kPi)).Normalized();
}

Path helix_bend() {
  ModelCurve helix;
  helix.kind = ModelCurveKind::Helix;
  helix.center = gp_Pnt(0, 0, 0);
  helix.normal = gp_Dir(0, 0, 1);
  helix.x_axis = gp_Dir(1, 0, 0);
  helix.major = kHelixRadius;
  helix.minor = kHelixPitch;
  helix.first = 0;
  helix.last = kPi / 2;
  const gp_Pnt a = helix_point(0);
  const gp_Pnt b = helix_point(kPi / 2);
  return {{"c1", line3(a.Translated(helix_tangent(0) * -40), a)},
          {"c2", helix},
          {"c3", line3(b, b.Translated(helix_tangent(kPi / 2) * 40))}};
}

// The distance from the point to the helix bend's path.
double helix_distance(const gp_Pnt& p) {
  double best = 1e100;
  for (const auto& [a, d] : {std::make_pair(helix_point(0), helix_tangent(0) * -40),
                             std::make_pair(helix_point(kPi / 2), helix_tangent(kPi / 2) * 40)}) {
    const double f = std::clamp(gp_Vec(a, p).Dot(d) / d.SquareMagnitude(), 0.0, 1.0);
    best = std::min(best, p.Distance(a.Translated(d * f)));
  }
  const int n = 48;
  for (int k = 0; k <= n; ++k) {
    const double here = p.Distance(helix_point(kPi / 2 * k / n));
    best = std::min(best, here);
    if ((k > 0 && p.Distance(helix_point(kPi / 2 * (k - 1) / n)) < here) ||
        (k < n && p.Distance(helix_point(kPi / 2 * (k + 1) / n)) < here)) {
      continue;
    }
    // Newton's steps on the square of the distance, within the samples
    // either side.
    const double lo = kPi / 2 * std::max(0, k - 1) / n;
    const double hi = kPi / 2 * std::min(n, k + 1) / n;
    const double c = kHelixPitch / (2 * kPi);
    double t = kPi / 2 * k / n;
    for (int i = 0; i < 30; ++i) {
      const gp_Vec d(helix_point(t), p);
      const gp_Vec d1(-kHelixRadius * std::sin(t), kHelixRadius * std::cos(t), c);
      const gp_Vec d2(-kHelixRadius * std::cos(t), -kHelixRadius * std::sin(t), 0);
      const double slope = -d.Dot(d1);
      const double bend = d1.SquareMagnitude() - d.Dot(d2);
      const double next = std::clamp(bend > 0 ? t - slope / bend : (slope > 0 ? lo : hi), lo, hi);
      const double step = std::abs(next - t);
      t = next;
      if (step < 1e-15) {
        break;
      }
    }
    best = std::min(best, p.Distance(helix_point(t)));
  }
  return best;
}

bool in_helix_sweep(const gp_Pnt& p, bool hollow) {
  const double d = helix_distance(p);
  return d <= 3 && (!hollow || d >= 2);
}

// The volume of the points within r of the helix bend's path. Each point of
// it lies in the disc square to the path at its nearest point of the path,
// and along each ray of that disc from the path the points whose nearest
// point it is run from the path to where another point of the path comes
// as near (a point further on is nearer that one too). So the volume is
// the path's length times the circle's area, less, at each station s and
// angle t (from the bend's normal) where the ray's point at r has another
// point of the path nearer, the part of the ray's sector beyond that
// point: the volume element is (1 - k rho cos t) rho over ds dt drho, k
// the path's curvature. Sampled at the middles of `per_mm` stations a mm
// along the path (from 2 mm before the bend to 2 mm after it, split at the
// joints) and `around` angles, each ray's end found by bisection.
double helix_union_volume(double r, int per_mm, int around) {
  const double c = kHelixPitch / (2 * kPi);
  const double helix_length = std::hypot(kHelixRadius * kPi / 2, c * kPi / 2);
  const double kappa = kHelixRadius / (kHelixRadius * kHelixRadius + c * c);
  double volume = kPi * r * r * (80 + helix_length);
  const double bounds[4] = {38.0, 40.0, 40.0 + helix_length, 42.0 + helix_length};
  for (int part = 0; part < 3; ++part) {
    const int n = static_cast<int>(std::ceil((bounds[part + 1] - bounds[part]) * per_mm));
    const double ds = (bounds[part + 1] - bounds[part]) / n;
    for (int i = 0; i < n; ++i) {
      const double s = bounds[part] + ds * (i + 0.5);
      gp_Pnt p;
      gp_Vec t;
      gp_Vec normal;
      double k = 0;
      if (part == 1) {
        const double at = (s - 40) / helix_length * kPi / 2;
        p = helix_point(at);
        t = helix_tangent(at);
        normal = gp_Vec(-std::cos(at), -std::sin(at), 0);
        k = kappa;
      } else {
        const double at = part == 0 ? 0 : kPi / 2;
        t = helix_tangent(at);
        p = helix_point(at).Translated(t * (part == 0 ? s - 40 : s - 40 - helix_length));
        normal = gp_Vec(0, 0, 1).Crossed(t).Normalized();
      }
      const gp_Vec binormal = t.Crossed(normal);
      for (int j = 0; j < around; ++j) {
        const double angle = 2 * kPi * (j + 0.5) / around;
        const gp_Vec u = normal * std::cos(angle) + binormal * std::sin(angle);
        const auto nearest = [&](double rho) { return helix_distance(p.Translated(u * rho)) >= rho * (1 - 1e-10); };
        if (nearest(r)) {
          continue;
        }
        double lo = 0;
        double hi = r;
        for (int it = 0; it < 40; ++it) {
          const double m = (lo + hi) / 2;
          (nearest(m) ? lo : hi) = m;
        }
        const auto sector = [&](double rho) { return rho * rho / 2 - k * std::cos(angle) * rho * rho * rho / 3; };
        volume -= (sector(r) - sector((lo + hi) / 2)) * ds * 2 * kPi / around;
      }
    }
  }
  return volume;
}

void test_helix_bend() {
  // Points sampled round the bend.
  const gp_Pnt low(-3, -3, -3.5);
  const gp_Pnt high(5.5, 5.5, 5);
  const double outer = helix_union_volume(3, 100, 256);
  const double bore = helix_union_volume(2, 100, 256);
  for (const bool hollow : {false, true}) {
    const double reference = hollow ? outer - bore : outer;
    double piped_volume = 0;
    for (const double seam : {-1.0, 0.7, 2.0, kPi}) {
      const bool piped = seam < 0;
      const std::string what = std::string(piped ? "pipe, " : "sweep, ") + (hollow ? "hollow, " : "") + "tight helix";
      Frame frame;
      frame.origin = helix_point(0).Translated(helix_tangent(0) * -40);
      {
        // The profile's seam turned away from the pipe's.
        gp_Trsf spin;
        spin.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(helix_tangent(0))), piped ? 0.0 : seam);
        frame.x_axis = gp_Dir(1, 0, 0).Transformed(spin);
        frame.y_axis = gp_Dir(helix_tangent(0).Crossed(gp_Vec(frame.x_axis)));
      }
      const ShapePtr shape = piped ? pipe_of(helix_bend(), PipeSection::Circular, 6, hollow)
                                   : sweep_on(helix_bend(), frame, false, 3, hollow);
      const TopoDS_Shape& s = shape->occt();
      int solids = 0;
      for (TopExp_Explorer e(s, TopAbs_SOLID); e.More(); e.Next()) {
        ++solids;
      }
      const bool valid = BRepCheck_Analyzer(s).IsValid();
      bool said = false;
      for (const std::string& info : shape->infos()) {
        said = said || info.find("built in pieces") != std::string::npos;
      }
      CHECK(solids == 1);
      CHECK(valid);
      CHECK(said);
      if (!piped) {
        // The same tube wherever the profile's seam lies.
        GProp_GProps props;
        BRepGProp::VolumeProperties(s, props);
        std::printf("%s, seam turned %.4f: %d solid(s), valid %d, volume %.9f, the pipe's %.9f\n", what.c_str(),
                    seam, solids, valid ? 1 : 0, props.Mass(), piped_volume);
        CHECK_NEAR_TOL(props.Mass(), piped_volume, 1e-9);
        continue;
      }
      const double volume = precise_volume(s);
      GProp_GProps props;
      BRepGProp::VolumeProperties(s, props);
      piped_volume = props.Mass();
      BRepAlgoAPI_Check check;
      check.SetData(s, false, true);
      check.SetRunParallel(true);
      check.Perform();
      const bool apart = check.IsValid();
      std::printf("%s: %d solid(s), valid %d, faces apart %d, volume %.9f, sampled %.9f (%.2e)\n", what.c_str(),
                  solids, valid ? 1 : 0, apart ? 1 : 0, volume, reference, (volume - reference) / reference);
      CHECK(apart);
      CHECK_NEAR_TOL(volume, reference, 3e-7);
      unsigned long long seed = 4242;
      const auto next = [&] {
        seed = seed * 6364136223846793005ULL + 1442695040888963407ULL;
        return static_cast<double>(seed >> 11) / static_cast<double>(1ULL << 53);
      };
      const int points = 100;
      int wrong = 0;
      for (int k = 0; k < points; ++k) {
        const gp_Pnt p(low.X() + (high.X() - low.X()) * next(), low.Y() + (high.Y() - low.Y()) * next(),
                       low.Z() + (high.Z() - low.Z()) * next());
        BRepClass3d_SolidClassifier classifier(s, p, 1e-7);
        if ((classifier.State() == TopAbs_IN) != in_helix_sweep(p, hollow)) {
          BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(p).Vertex(), s);
          if (distance.Value() > 1e-5) {
            ++wrong;
            std::printf("  misplaced (%.6f %.6f %.6f): %s the solid, %.6f from its faces\n", p.X(), p.Y(), p.Z(),
                        classifier.State() == TopAbs_IN ? "in" : "out of", distance.Value());
          }
        }
      }
      std::printf("%s: %d of %d sampled points misplaced\n", what.c_str(), wrong, points);
      CHECK(wrong == 0);
    }
  }
}

} // namespace
} // namespace test

int main() {
  test::guarded("tight ellipse", test::test_ellipse_bend);
  test::guarded("tight spline", test::test_spline_bend);
  test::guarded("tight helix", test::test_helix_bend);
  test::guarded("polyline in no plane", test::test_skew_polyline);
  test::guarded("arc between miters", test::test_mitered_arc);
  test::guarded("tight bend in no plane", test::test_skew_tight_bend);
  test::guarded("tight arc", test::test_tight_arc);
  test::guarded("tight ring", test::test_tight_ring);
  test::guarded("tight bend", test::test_tight_bend);
  test::guarded("hollow tight bend", test::test_hollow_tight_bend);
  test::guarded("short chamfer", test::test_short_chamfer);
  test::guarded("loop back", test::test_loop_back);
  test::guarded("plain pipe unchanged", test::test_plain_pipe_unchanged);
  test::guarded("turning back refused", test::test_turning_back_refused);
  if (test::failures == 0) {
    std::puts("test_pipe_folds: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
