// SPDX-License-Identifier: MIT
// A pocket's floor rounded with a ball of exactly twice the radius of the
// concave arcs at its corners (mitcad#1368), with the port's patch 1390.
//
// The pocket, 3 deep, as a reported design's: side walls 14 apart, its far
// end a straight wall with concave arcs of 0.5 at its corners, its near end
// a long concave arc of 12 between two convex arcs of 1.25, which meet the
// side walls through concave arcs of 0.4, each turning 157 degrees. Every
// curve of the outline goes on tangentially into the next, and the floor is
// square to the walls.
//
// The floor's outline rounded with 0.8: round a concave arc of 0.4 the ball
// touches the floor 0.4 from the arc's axis on the far side and the wall
// level with its centre on the near side, so the mid-point of its contacts
// lies on the axis, for every ball round the arc. The walk's section
// condition (that mid-point in the guide's normal plane, and every normal
// plane of the arc holds its axis) then picked no ball, and the walk could
// not leave its first section: OCCT failed at exactly 0.8, built at 0.79999
// and 0.80001, and MitCad's fillet was built 0.1 % smaller. With the patch
// the condition is taken on the ball's centre where the mid-point lies on
// the guide's polar axis.
//
// - OCCT's fillet alone builds the rounding at 0.79999, 0.8 and 0.80001,
//   valid, the material it adds at 0.8 within 1e-3 mm^3 of that at both
//   radii beside it (0.1 % smaller adds about 1e-2 mm^3 less).
// - MitCad's fillet builds it at 0.8 as asked (no info of a smaller size),
//   valid, its faces round the floor within 1e-4 mm (OCCT's approximation
//   tolerance of a walked blend) of OCCT's results at 0.79999 and 0.80001
//   both ways: the walk at 0.8 gives what the walks beside it give.
// - The reported body, the case `ball-twice-arc-rounding` of
//   MITCAD_GEOMETRY_CASES (skipped without it): `body.brep` with the floor's
//   ten edges in `edges.txt` (TopExp order), and when present the stored
//   state before the rounding, `state.brep` with `state-edges.txt`; each is
//   rounded with 0.8 by OCCT's fillet alone (and at 0.79999 and 0.80001,
//   the material added at 0.8 within 1e-3 mm^3 of that at both) and by
//   MitCad's, and the stored body after the rounding, `after.brep`, is
//   measured against the result round the 0.4 arcs (printed: the stored
//   corner faces are not the rolling ball's). Past each 0.4 arc the walked
//   blend twists and is filled with a patch that comes to a point; the
//   filling's check of its tangent fields read numerical noise where they
//   vanish there, so the rounding ended in one of two ways, 0.04 mm apart,
//   as tiny differences tipped it (mitcad#1384): with the port's patch 1410
//   both arcs, at the three radii and on both bodies, end one way (the
//   area of the walked and filled faces round each arc within 1e-3 mm^2).

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <fstream>
#include <sstream>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_Map.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Circ.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>
#include <gp_Vec2d.hxx>

#include "check.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/extrude.hpp"
#include "mitcad/geometry/import.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

std::string env(const char* name) {
#ifdef _MSC_VER
  char*       value  = nullptr;
  std::size_t length = 0;
  std::string out;
  if (_dupenv_s(&value, &length, name) == 0 && value != nullptr) {
    out = value;
    std::free(value);
  }
  return out;
#else
  const char* value = std::getenv(name);
  return value != nullptr ? std::string(value) : std::string();
#endif
}

constexpr double kRadius     = 0.8;           // the rounding: twice the corner arcs' radius
constexpr double kCorner     = 0.4;           // the concave arcs at the near end's corners
constexpr double kBump       = 1.25;          // the convex arcs beside them
constexpr double kLong       = 12.0;          // the near end's long concave arc
constexpr double kLongCentre = 11.5868329805; // its centre beyond the corner arcs' centres
constexpr double kFarCorner  = 0.5;           // the concave arcs at the far end's corners
constexpr double kHalfWidth  = 7.0;           // the side walls at x = -7 and 7
constexpr double kFar        = 2.6;           // the far end's wall at y = 2.6
constexpr double kDepth      = 3.0;           // the pocket's
// OCCT's approximation tolerance of a walked blend (3d, its default).
constexpr double kApproximation = 1.0e-4;

const double kRadii[] = {0.79999, kRadius, 0.80001};

// ---- The pocket ----

double angle(const gp_Pnt2d& from, const gp_Pnt2d& to) {
  return std::atan2(to.Y() - from.Y(), to.X() - from.X());
}

gp_Pnt2d corner_centre(double side) {
  return gp_Pnt2d(side * (kHalfWidth - kCorner), 0.0);
}

// The centre of the convex arc on the side (-1 or 1): tangent to the corner's
// arc and to the long arc, on the near side of both.
gp_Pnt2d bump_centre(double side) {
  const gp_Pnt2d a = corner_centre(-1.0);
  const gp_Pnt2d c(0.0, kLongCentre);
  const double   r1 = kCorner + kBump;
  const double   r2 = kLong + kBump;
  const gp_Vec2d ac(a, c);
  const double   d    = ac.Magnitude();
  const double   l    = (d * d + r1 * r1 - r2 * r2) / (2.0 * d);
  const double   h    = std::sqrt(r1 * r1 - l * l);
  const gp_Vec2d unit = ac / d;
  const double   x    = a.X() + l * unit.X() + h * unit.Y();
  const double   y    = a.Y() + l * unit.Y() - h * unit.X();
  return gp_Pnt2d(-side * x, y);
}

Segment arc(const std::string& name, const gp_Pnt2d& centre, double radius, double from, double to) {
  Segment s;
  s.name        = name;
  s.kind        = SegmentKind::Arc;
  s.center      = centre;
  s.radius      = radius;
  s.start_angle = from;
  s.end_angle   = to;
  return s;
}

Segment line(const std::string& name, const gp_Pnt2d& from, const gp_Pnt2d& to) {
  Segment s;
  s.name  = name;
  s.kind  = SegmentKind::Line;
  s.start = from;
  s.end   = to;
  return s;
}

// The pocket's outline, round from the left side wall.
Region outline() {
  const gp_Pnt2d a = corner_centre(-1.0);
  const gp_Pnt2d b = bump_centre(-1.0);
  const gp_Pnt2d c(0.0, kLongCentre);
  const gp_Pnt2d ab(a.X() + kCorner * (b.X() - a.X()) / (kCorner + kBump),
                    a.Y() + kCorner * (b.Y() - a.Y()) / (kCorner + kBump));
  const gp_Pnt2d bc(c.X() + kLong * (b.X() - c.X()) / (kLong + kBump),
                    c.Y() + kLong * (b.Y() - c.Y()) / (kLong + kBump));
  const double   corner_end = angle(a, ab) + 2.0 * kPi; // about 337 degrees
  const double   bump_from  = angle(b, bc);             // about 67 degrees
  const double   bump_to    = angle(b, ab);             // about 157 degrees
  const double   long_from  = angle(c, bc) + 2.0 * kPi; // about 247 degrees
  const double   far_x      = kHalfWidth - kFarCorner;
  const double   far_y      = kFar - kFarCorner;

  Region region;
  region.name = "r{c1,c2,c3,c4,c5,c6,c7,c8,c9,c10}";
  Loop loop;
  loop.segments.push_back(line("c1[c10,c2]", gp_Pnt2d(-kHalfWidth, far_y), gp_Pnt2d(-kHalfWidth, 0.0)));
  loop.segments.push_back(arc("c2[c1,c3]", a, kCorner, kPi, corner_end));
  loop.segments.push_back(arc("c3[c2,c4]", b, kBump, bump_from, bump_to));
  loop.segments.push_back(arc("c4[c3,c5]", c, kLong, long_from, 3.0 * kPi - long_from));
  loop.segments.push_back(arc("c5[c4,c6]", bump_centre(1.0), kBump, kPi - bump_to, kPi - bump_from));
  loop.segments.push_back(arc("c6[c5,c7]", corner_centre(1.0), kCorner, 3.0 * kPi - corner_end, 2.0 * kPi));
  loop.segments.push_back(line("c7[c6,c8]", gp_Pnt2d(kHalfWidth, 0.0), gp_Pnt2d(kHalfWidth, far_y)));
  loop.segments.push_back(arc("c8[c7,c9]", gp_Pnt2d(far_x, far_y), kFarCorner, 0.0, kPi / 2.0));
  loop.segments.push_back(line("c9[c8,c10]", gp_Pnt2d(far_x, kFar), gp_Pnt2d(-far_x, kFar)));
  loop.segments.push_back(arc("c10[c9,c1]", gp_Pnt2d(-far_x, far_y), kFarCorner, kPi / 2.0, kPi));
  region.loops.push_back(loop);
  return region;
}

// A block with the pocket cut into its top, its floor at z = 0.
ShapePtr pocketed() {
  ExtrudeFeatureSpec spec;
  spec.feature        = "F2";
  spec.regions        = {outline()};
  spec.side1.distance = kDepth + 1.0;
  const ShapePtr pocket = extrude_feature(spec);
  const ShapePtr block =
    import_body(BRepPrimAPI_MakeBox(gp_Pnt(-10.0, -4.0, -2.0), gp_Pnt(10.0, 6.0, kDepth)).Shape(), "F1", 0);
  const BooleanResult cut = boolean(BooleanOp::Cut, {block.get()}, *pocket);
  CHECK(cut.pieces.size() == 1);
  return cut.pieces.size() == 1 ? cut.pieces.front().shape : nullptr;
}

// The body's edges round the floor (z = 0).
std::vector<int> floor_edges(const Shape& body) {
  std::vector<int> out;
  for (int e = 0; e < body.edge_count(); ++e) {
    const BRepAdaptor_Curve curve(body.edge(e));
    const double            t0 = curve.FirstParameter();
    const double            t1 = curve.LastParameter();
    if (std::abs(curve.Value(t0).Z()) < 1.0e-9 && std::abs(curve.Value(t1).Z()) < 1.0e-9 &&
        std::abs(curve.Value(0.5 * (t0 + t1)).Z()) < 1.0e-9) {
      out.push_back(e);
    }
  }
  return out;
}

// The edges among `edges` along an arc of radius kCorner.
std::vector<int> corner_edges(const Shape& body, const std::vector<int>& edges) {
  std::vector<int> out;
  for (const int e : edges) {
    const BRepAdaptor_Curve curve(body.edge(e));
    if (curve.GetType() == GeomAbs_Circle && std::abs(curve.Circle().Radius() - kCorner) < 1.0e-9) {
      out.push_back(e);
    }
  }
  return out;
}

// ---- The roundings ----

// The volume, integrated to a relative error of 1e-9.
double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1.0e-9, true);
  return props.Mass();
}

// OCCT's fillet alone of the edges with radius r on a copy of the body, as
// MitCad's dressup calls it (chains stopped at turns, carried across
// notches); null where it fails or its result is not valid.
TopoDS_Shape occt_rounded(const Shape& body, const std::vector<int>& edges, double r) {
  const ShapePtr           copy = import_body(BRepBuilderAPI_Copy(body.occt()).Shape(), "F9", 0);
  BRepFilletAPI_MakeFillet round(copy->occt());
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> selected;
  for (const int e : edges) {
    selected.Add(copy->edge(e));
  }
  round.StopChainsAtTurns(selected);
  for (const int e : edges) {
    if (round.Contour(copy->edge(e)) == 0) {
      round.Add(r, copy->edge(e));
    }
  }
  round.CarryChainsAcrossNotches();
  try {
    OCC_CATCH_SIGNALS
    round.Build();
  } catch (const Standard_Failure& failure) {
    std::printf("  OCCT %.5f: %s\n", r, failure.what());
    return TopoDS_Shape();
  }
  if (!round.IsDone()) {
    std::printf("  OCCT %.5f: not done (%d faulty contours)\n", r, round.NbFaultyContours());
    return TopoDS_Shape();
  }
  if (!BRepCheck_Analyzer(round.Shape()).IsValid()) {
    std::printf("  OCCT %.5f: not valid\n", r);
    return TopoDS_Shape();
  }
  return round.Shape();
}

// MitCad's fillet (F3) of the edges with radius r; null where it fails.
ShapePtr rounded(const Shape& body, const std::vector<int>& edges, double r) {
  FilletSet set;
  for (const int e : edges) {
    set.edges.push_back(body.edge_name(e));
  }
  set.radius = r;
  try {
    return fillet("F3", body, {set});
  } catch (const std::exception& failure) {
    std::printf("  MitCad %.5f: %s\n", r, failure.what());
    return nullptr;
  }
}

// Valid, built as asked (no note, no info such as a smaller size).
void check_as_asked(const ShapePtr& result) {
  CHECK(result != nullptr);
  if (!result) {
    return;
  }
  CHECK(BRepCheck_Analyzer(result->occt()).IsValid());
  CHECK(result->notes().empty());
  CHECK(result->infos().empty());
  for (const std::string& note : result->notes()) {
    std::printf("  note: %s\n", note.c_str());
  }
  for (const std::string& info : result->infos()) {
    std::printf("  info: %s\n", info.c_str());
  }
}

// ---- The reported body ----

TopoDS_Shape read(const std::string& path) {
  TopoDS_Shape shape;
  BRep_Builder builder;
  std::ifstream probe(path);
  if (!probe.good()) {
    return shape;
  }
  BRepTools::Read(shape, path.c_str(), builder);
  return shape;
}

// The indices a list holds, one on each line.
std::vector<int> read_indices(const std::string& path) {
  std::vector<int> out;
  std::ifstream    in(path);
  std::string      text;
  while (std::getline(in, text)) {
    std::istringstream fields(text);
    int                index = -1;
    if (fields >> index && index >= 0) {
      out.push_back(index);
    }
  }
  return out;
}

// The faces of a shape whose box meets the region.
TopoDS_Compound faces_in(const TopoDS_Shape& shape, const Bnd_Box& region) {
  TopoDS_Compound out;
  BRep_Builder    builder;
  builder.MakeCompound(out);
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    Bnd_Box box;
    BRepBndLib::Add(f.Current(), box);
    if (!box.IsOut(region)) {
      builder.Add(out, f.Current());
    }
  }
  return out;
}

// The farthest of points inside the faces of `from` in the region from the
// faces of `to` there.
double farthest(const TopoDS_Shape& from, const TopoDS_Shape& to, const Bnd_Box& region) {
  Bnd_Box wider = region;
  wider.Enlarge(1.0);
  const TopoDS_Compound target = faces_in(to, wider);
  double                worst  = 0.0;
  for (TopExp_Explorer f(faces_in(from, region), TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face face = TopoDS::Face(f.Current());
    double            u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    const BRepAdaptor_Surface surface(face);
    for (int i = 0; i <= 6; ++i) {
      for (int j = 0; j <= 6; ++j) {
        const double u = u0 + (u1 - u0) * (0.05 + 0.9 * i / 6.0);
        const double v = v0 + (v1 - v0) * (0.05 + 0.9 * j / 6.0);
        if (BRepClass_FaceClassifier(face, gp_Pnt2d(u, v), 1.0e-7).State() != TopAbs_IN) {
          continue;
        }
        const gp_Pnt p = surface.Value(u, v);
        if (region.IsOut(p)) {
          continue;
        }
        BRepExtrema_DistShapeShape d(BRepBuilderAPI_MakeVertex(p).Vertex(), target);
        if (d.IsDone()) {
          worst = std::max(worst, d.Value());
        }
      }
    }
  }
  return worst;
}

// OCCT's fillet alone at the radius and at the radii beside it (kRadii),
// each built and valid; the material each adds, printed. Null shapes where
// one failed.
std::vector<TopoDS_Shape> occt_alone(const std::string& label, const Shape& body, const std::vector<int>& edges,
                                     double added[3]) {
  const double              before = volume_of(body.occt());
  std::vector<TopoDS_Shape> out;
  for (int i = 0; i < 3; ++i) {
    const TopoDS_Shape result = occt_rounded(body, edges, kRadii[i]);
    CHECK(!result.IsNull());
    added[i] = result.IsNull() ? 0.0 : volume_of(result) - before;
    std::printf("  %s OCCT %.5f: adds %.6f mm^3\n", label.c_str(), kRadii[i], added[i]);
    out.push_back(result);
  }
  return out;
}

// The area of the B-spline faces of the shape (the walked and filled pieces
// of a rounding) whose box meets the region.
double spline_area_in(const TopoDS_Shape& shape, const Bnd_Box& region) {
  double area = 0.0;
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    Bnd_Box box;
    BRepBndLib::Add(f.Current(), box);
    if (box.IsOut(region) || BRepAdaptor_Surface(TopoDS::Face(f.Current()), false).GetType() != GeomAbs_BSplineSurface) {
      continue;
    }
    GProp_GProps props;
    BRepGProp::SurfaceProperties(f.Current(), props);
    area += props.Mass();
  }
  return area;
}

bool all_built(const std::vector<TopoDS_Shape>& shapes) {
  return std::none_of(shapes.begin(), shapes.end(), [](const TopoDS_Shape& s) { return s.IsNull(); });
}

// ---- The pocket ----

void test_pocket_floor_rounded() {
  const ShapePtr body = pocketed();
  CHECK(body != nullptr);
  if (!body) {
    return;
  }
  CHECK(BRepCheck_Analyzer(body->occt()).IsValid());
  const std::vector<int> edges = floor_edges(*body);
  CHECK(edges.size() == 10);
  CHECK(corner_edges(*body, edges).size() == 2);
  double                          added[3] = {0.0, 0.0, 0.0};
  const std::vector<TopoDS_Shape> beside   = occt_alone("pocket", *body, edges, added);
  if (all_built(beside)) {
    CHECK(std::abs(added[1] - added[0]) <= 1.0e-3);
    CHECK(std::abs(added[1] - added[2]) <= 1.0e-3);
  }
  const ShapePtr result = rounded(*body, edges, kRadius);
  check_as_asked(result);
  if (!result || !all_built(beside)) {
    return;
  }
  // Round the floor, up to the walls' contacts.
  Bnd_Box region;
  region.Update(-kHalfWidth - 0.1, -1.0, -0.1, kHalfWidth + 0.1, kFar + 0.1, kRadius + 0.1);
  for (const int i : {0, 2}) {
    const double there = farthest(result->occt(), beside[static_cast<std::size_t>(i)], region);
    const double back  = farthest(beside[static_cast<std::size_t>(i)], result->occt(), region);
    std::printf("  pocket: MitCad's 0.8 and OCCT's %.5f within %.2e and %.2e mm\n", kRadii[i], there, back);
    CHECK(there <= kApproximation);
    CHECK(back <= kApproximation);
  }
}

// A body of the case and its listed edges, rounded with exactly kRadius by
// OCCT's fillet alone and by MitCad's; the stored body after the rounding
// measured against MitCad's result round each 0.4 arc.
// The B-spline area round each 0.4 arc of the case at each radius of
// kRadii, appended to `areas` (by radius, then arc in x order).
void check_case_body(const std::string& label, const TopoDS_Shape& shape, const std::vector<int>& listed,
                     const TopoDS_Shape& after, std::vector<double>& areas) {
  const ShapePtr body = import_body(shape, "F1", 0);
  CHECK(listed.size() == 10);
  bool in_range = true;
  for (const int e : listed) {
    in_range = in_range && e < body->edge_count();
  }
  CHECK(in_range);
  if (listed.size() != 10 || !in_range) {
    return;
  }
  std::vector<int> corners = corner_edges(*body, listed);
  CHECK(corners.size() == 2);
  std::sort(corners.begin(), corners.end(), [&](int a, int b) {
    return BRepAdaptor_Curve(body->edge(a)).Circle().Location().X() <
           BRepAdaptor_Curve(body->edge(b)).Circle().Location().X();
  });
  double                          added[3] = {0.0, 0.0, 0.0};
  const std::vector<TopoDS_Shape> beside   = occt_alone(label, *body, listed, added);
  if (all_built(beside)) {
    // The rounding ends the same way at each 0.4 arc at every radius (the
    // port's patch 1410): the material added runs on smoothly.
    CHECK(std::abs(added[1] - added[0]) <= 1.0e-3);
    CHECK(std::abs(added[1] - added[2]) <= 1.0e-3);
    for (int i = 0; i < 3; ++i) {
      for (const int e : corners) {
        Bnd_Box region;
        BRepBndLib::Add(body->edge(e), region);
        region.Enlarge(kRadius + 0.1);
        areas.push_back(spline_area_in(beside[static_cast<std::size_t>(i)], region));
        std::printf("  %s OCCT %.5f: the walked and filled faces round the arc at x = %.1f, %.6f mm^2\n",
                    label.c_str(), kRadii[i], BRepAdaptor_Curve(body->edge(e)).Circle().Location().X(),
                    areas.back());
      }
    }
  }
  const ShapePtr result = rounded(*body, listed, kRadius);
  check_as_asked(result);
  if (!result || after.IsNull()) {
    return;
  }
  for (const int e : corners) {
    Bnd_Box region;
    BRepBndLib::Add(body->edge(e), region);
    region.Enlarge(kRadius + 0.1);
    const BRepAdaptor_Curve curve(body->edge(e));
    std::printf("  %s: the stored body after the rounding within %.6f mm of the result round the arc at x = %.1f\n",
                label.c_str(), farthest(after, result->occt(), region), curve.Circle().Location().X());
  }
}

void test_case_body() {
  const std::string root = env("MITCAD_GEOMETRY_CASES");
  if (root.empty()) {
    std::printf("test_case_body: skipped (MITCAD_GEOMETRY_CASES is not set)\n");
    return;
  }
  const std::string  folder = root + "/ball-twice-arc-rounding/";
  const TopoDS_Shape body   = read(folder + "body.brep");
  if (body.IsNull()) {
    std::printf("test_case_body: skipped (no ball-twice-arc-rounding in MITCAD_GEOMETRY_CASES)\n");
    return;
  }
  const TopoDS_Shape  after = read(folder + "after.brep");
  std::vector<double> areas;
  check_case_body("replay", body, read_indices(folder + "edges.txt"), after, areas);
  const TopoDS_Shape state = read(folder + "state.brep");
  if (!state.IsNull()) {
    check_case_body("stored state", state, read_indices(folder + "state-edges.txt"), after, areas);
  }
  // Both arcs, at 0.79999, 0.8 and 0.80001, on both bodies: one way (the
  // two ways differ by 0.06 mm^2; the radii by about 2e-5 mm^2).
  CHECK(!areas.empty());
  for (const double a : areas) {
    CHECK(std::abs(a - areas.front()) <= 1.0e-3);
  }
}

} // namespace
} // namespace test

int main() {
  test::guarded("test_pocket_floor_rounded", test::test_pocket_floor_rounded);
  test::guarded("test_case_body", test::test_case_body);
  if (test::failures != 0) {
    std::printf("%d check(s) failed\n", test::failures);
  }
  return test::failures == 0 ? 0 : 1;
}
