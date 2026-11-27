// SPDX-License-Identifier: MIT
// The rounding of the rim between a loft's side and a cylinder pulled from
// its end face (mitcad#2300), through the geometry facade as the model
// builds it.
//
// The owner's loft runs from a circle of radius 111 on XZ to a slot
// 520 x 80 mm 100 mm along +Y; the circle's end is pulled 230 mm straight
// out, a cylinder joined to the loft. The loft's side flares out beyond
// the cylinder towards the slot's ends and draws in within it towards the
// slot's flat sides, so the rim between them is concave in some places
// and convex in others, the two faces tangent at four points between.
// A rolling ball of one radius cannot run across such a point; the
// rounding vanishes there. The kernel took the rim as convex or concave
// all along, and its walk stopped where the ball's contact ran off over
// the rim itself ("stops part of the way where it rolls on an edge of a
// neighbouring face"), with the loft's Tangent Edges Merge and Keep alike.
// The fillet now splits the rim at those points (vanishing_split.hpp),
// each piece rounded on its own side and vanishing at its ends.
//
// Parts (one ctest each): the owner's case at 5 mm, at 2 and 10 mm, and at
// 5 mm with pulls of 50 and 400 mm, each with Merge and with Keep, the one
// rim edge the owner picked selected with Tangent Chain on; kernel_merge
// and kernel_keep, the kernel's fillet alone
// of the body the fillet rounds. Each result is one valid solid without a
// note or an info; no edge is left between the loft's side
// and the cylinder; at least a rounded face per piece, with Merge named
// after the rim's edges, a piece each (with Keep the side's pieces of one
// surface are joined for the rounding and split again after it, which
// names them); every rounded face lies on a ball of the radius touching
// the loft's side and the cylinder (within 0.01 mm, at its middle); and
// the result has a vertex at each point where the rim's faces are tangent,
// where the roundings vanish. And far_end_unsplit: an edge that keeps one
// convexity (the cylinder's far end) has no such point.

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <filesystem>
#include <optional>
#include <set>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <NCollection_Map.hxx>
#include <Standard_Failure.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepGProp.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <Geom_Curve.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>

#include "../src/face_join.hpp"
#include "../src/vanishing_split.hpp"
#include "check.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/loft.hpp"
#include "mitcad/geometry/naming.hpp"
#include "mitcad/geometry/press_pull.hpp"
#include "mitcad/geometry/profile.hpp"
#include "mitcad/geometry/shape.hpp"
#include "bundled_test.hpp"

namespace test {
namespace {

namespace g = mitcad::geometry;

// How far a rounded face's middle may lie off the ball of its radius
// between the two faces (mm): the kernel's walked surfaces are
// approximated within about 1e-4 mm.
constexpr double kOnBall = 1.0e-2;

const char* const kLoft = "F3";
const char* const kPull = "F5";
const char* const kFillet = "F7";

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props, 1.0e-9);
  return props.Mass();
}

int count(const TopoDS_Shape& shape, TopAbs_ShapeEnum type) {
  int n = 0;
  for (TopExp_Explorer it(shape, type); it.More(); it.Next()) {
    ++n;
  }
  return n;
}

g::Segment line(const std::string& name, double x0, double y0, double x1, double y1) {
  g::Segment s;
  s.name = name;
  s.kind = g::SegmentKind::Line;
  s.start = gp_Pnt2d(x0, y0);
  s.end = gp_Pnt2d(x1, y1);
  return s;
}

g::Segment arc(const std::string& name, double cx, double cy, double radius, double from, double to) {
  g::Segment s;
  s.name = name;
  s.kind = g::SegmentKind::Arc;
  s.center = gp_Pnt2d(cx, cy);
  s.radius = radius;
  s.start_angle = from;
  s.end_angle = to;
  return s;
}

g::Region region_of(const std::string& name, const std::vector<g::Segment>& segments) {
  g::Region region;
  region.name = name;
  region.loops.push_back(g::Loop{segments});
  return region;
}

g::Region circle(double radius) {
  g::Segment s;
  s.name = "c1";
  s.kind = g::SegmentKind::Circle;
  s.center = gp_Pnt2d(0, 0);
  s.radius = radius;
  return region_of("r{c1}", {s});
}

// A centre-point slot along the sketch's x axis about its origin, its
// arcs' centres `half` either side of it, of radius `radius`.
g::Region slot(double half, double radius) {
  return region_of("r{c2,c3,c4,c5}", {line("c2", -half, -radius, half, -radius),
                                      arc("c3", half, 0, radius, -kPi / 2, kPi / 2),
                                      line("c4", half, radius, -half, radius),
                                      arc("c5", -half, 0, radius, kPi / 2, 3 * kPi / 2)});
}

g::LoftSection section(const g::Frame& frame, const g::Region& region) {
  g::LoftSection s;
  s.frame = frame;
  s.region = region;
  return s;
}

// The owner's body: the loft from a circle of radius 110.9655561796864 on
// XZ to a slot 520 x 80 (centres 220 either side, radius 40) 100 along +Y,
// Tangent Edges Merge or Keep, its circle's end pulled `pull` straight out
// (along -Y), joined.
g::ShapePtr owner_body(bool merge, double pull) {
  g::Frame xz;
  xz.x_axis = gp_Dir(1, 0, 0);
  xz.y_axis = gp_Dir(0, 0, 1);
  g::Frame plane = xz;
  plane.origin = gp_Pnt(0, 100, 0);
  g::LoftSpec spec;
  spec.feature = kLoft;
  spec.sections = {section(xz, circle(110.9655561796864)), section(plane, slot(220, 40))};
  spec.merge_tangent = merge;
  const g::ShapePtr loft = g::loft(spec);
  g::PressPullSpec press;
  press.feature = kPull;
  press.faces = {g::face_name(kLoft, "start", "r{c1}")};
  press.run = g::PressPullRun::Straight;
  press.distance = pull;
  return g::press_pull(*loft, press);
}

bool named_with(const g::NameList& names, const std::string& prefix) {
  return std::any_of(names.begin(), names.end(),
                     [&](const std::string& name) { return name.compare(0, prefix.size(), prefix) == 0; });
}

const std::string& loft_side() {
  static const std::string prefix = std::string(kLoft) + ":side(";
  return prefix;
}

const std::string& cylinder_side() {
  static const std::string prefix = std::string(kPull) + ":side(";
  return prefix;
}

// The body's edges between a face of the loft's side and one of the
// cylinder: the rim.
std::vector<int> rim_edges(const g::Shape& body) {
  std::vector<int> rim;
  for (int e = 0; e < body.edge_count(); ++e) {
    const std::array<int, 2> faces = body.edge_faces(e);
    if (faces[0] < 0 || faces[1] < 0 || faces[0] == faces[1]) {
      continue;
    }
    const g::NameList& a = body.face_names(faces[0]);
    const g::NameList& b = body.face_names(faces[1]);
    if ((named_with(a, loft_side()) && named_with(b, cylinder_side())) ||
        (named_with(b, loft_side()) && named_with(a, cylinder_side()))) {
      rim.push_back(e);
    }
  }
  return rim;
}

// The points of the rim where its faces are tangent, the roundings
// vanishing there.
std::vector<gp_Pnt> vanishing_points(const g::Shape& body, const std::vector<int>& rim) {
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> ancestors;
  TopExp::MapShapesAndUniqueAncestors(body.occt(), TopAbs_EDGE, TopAbs_FACE, ancestors);
  std::vector<gp_Pnt> points;
  for (const int e : rim) {
    const NCollection_List<TopoDS_Shape>& faces = ancestors.FindFromKey(body.edge(e));
    CHECK(faces.Extent() == 2);
    if (faces.Extent() != 2) {
      continue;
    }
    const TopoDS_Face a = TopoDS::Face(faces.First());
    const TopoDS_Face b = TopoDS::Face(faces.Last());
    double first = 0.0;
    double last = 0.0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(body.edge(e), first, last);
    for (const double t : g::detail::vanishing_parameters(body.edge(e), a, b)) {
      points.push_back(curve->Value(t));
    }
  }
  return points;
}

// The faces of the body whose names start with `prefix`, as one compound.
TopoDS_Compound faces_named(const g::Shape& body, const std::string& prefix) {
  BRep_Builder builder;
  TopoDS_Compound faces;
  builder.MakeCompound(faces);
  for (int f = 0; f < body.face_count(); ++f) {
    if (named_with(body.face_names(f), prefix)) {
      builder.Add(faces, body.face(f));
    }
  }
  return faces;
}

double distance(const gp_Pnt& p, const TopoDS_Shape& shape) {
  BRepExtrema_DistShapeShape measure(BRepBuilderAPI_MakeVertex(p).Vertex(), shape);
  return measure.IsDone() ? measure.Value() : 1.0e100;
}

// How far the middle of a rounded face lies off the ball of `radius`
// touching both the loft's side and the cylinder: the ball's centre a
// radius from the face's middle along its normal (either way), and a
// radius from each of the two faces. Negative where the middle is not
// inside the face.
double off_ball(const TopoDS_Face& face, double radius, const TopoDS_Shape& loft, const TopoDS_Shape& cylinder) {
  double u0 = 0.0;
  double u1 = 0.0;
  double v0 = 0.0;
  double v1 = 0.0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  const gp_Pnt2d uv(0.5 * (u0 + u1), 0.5 * (v0 + v1));
  if (BRepClass_FaceClassifier(face, uv, 1.0e-7).State() != TopAbs_IN) {
    return -1.0;
  }
  const BRepAdaptor_Surface surface(face);
  gp_Pnt p;
  gp_Vec du;
  gp_Vec dv;
  surface.D1(uv.X(), uv.Y(), p, du, dv);
  gp_Vec n = du.Crossed(dv);
  if (n.Magnitude() < 1.0e-12) {
    return -1.0;
  }
  n.Normalize();
  double best = 1.0e100;
  for (const double side : {1.0, -1.0}) {
    const gp_Pnt centre = p.Translated(side * radius * n);
    best = std::min(best, std::max(std::abs(distance(centre, loft) - radius),
                                   std::abs(distance(centre, cylinder) - radius)));
  }
  return best;
}

// The owner's body with the rounding of the rim (one edge of it picked,
// Tangent Chain on) at `radius`.
void check_rim_rounded(bool merge, double pull, double radius) {
  char what[96];
  std::snprintf(what, sizeof what, "%s, pull %g mm, radius %g mm", merge ? "Merge" : "Keep", pull, radius);
  g::ShapePtr body;
  try {
    body = owner_body(merge, pull);
  } catch (const std::exception& error) {
    std::printf("%s: the body: %s\n", what, error.what());
  }
  CHECK(body != nullptr);
  if (!body) {
    return;
  }
  CHECK(BRepCheck_Analyzer(body->occt()).IsValid());
  CHECK(count(body->occt(), TopAbs_SOLID) == 1);
  const std::vector<int> rim = rim_edges(*body);
  CHECK(!rim.empty());
  if (rim.empty()) {
    std::printf("%s: no rim between the loft's side and the cylinder\n", what);
    return;
  }
  // The rim turns from convex to concave and back: an even number of
  // tangent points, four or more, inside its edges.
  const std::vector<gp_Pnt> vanishing = vanishing_points(*body, rim);
  std::printf("%s: %zu rim edges, %zu points where the rounding vanishes\n", what, rim.size(), vanishing.size());
  CHECK(vanishing.size() >= 4);
  CHECK(vanishing.size() % 2 == 0);

  g::FilletSet set;
  set.edges = {body->edge_name(rim.front())};
  set.radius = radius;
  set.tangent_chain = true;
  // With Keep the loft's side is in pieces of one periodic surface round
  // the whole section, which the fillet joins into one face before it rounds
  // (face_join.hpp, join_surface_pieces), as Merge's side is one face.
  {
    std::vector<int> pieces;
    for (int f = 0; f < body->face_count(); ++f) {
      if (named_with(body->face_names(f), loft_side()) || named_with(body->face_names(f), cylinder_side())) {
        pieces.push_back(f);
      }
    }
    const g::ShapePtr joined = g::detail::join_surface_pieces(*body, pieces);
    CHECK(joined != nullptr);
    if (joined) {
      const int sides = count(faces_named(*joined, loft_side()), TopAbs_FACE);
      std::printf("%s: the side's %zu faces with the cylinder's joined into %d side faces\n", what, pieces.size(),
                  sides);
      CHECK(sides == 1);
    } else {
      std::printf("%s: the side's faces do not join\n", what);
    }
  }
  // The split the fillet rounds on (vanishing_split.hpp): the rim's edges
  // split at the tangent points, every piece selected.
  std::string why;
  const std::optional<g::detail::VanishingSplit> split = g::detail::split_at_vanishing_points(*body, {set}, &why);
  CHECK(split.has_value());
  if (!split) {
    std::printf("%s: the rim is not split: %s\n", what, why.c_str());
  } else {
    std::size_t selected = 0;
    for (const g::FilletSet& on_split : split->sets) {
      selected += on_split.edges.size();
    }
    std::printf("%s: split into %zu pieces, %zu edges selected on the split body\n", what, split->pieces.size(),
                selected);
    CHECK(split->pieces.size() >= vanishing.size() + 1);
  }
  g::ShapePtr rounded;
  try {
    rounded = g::fillet(kFillet, *body, {set});
  } catch (const std::exception& error) {
    std::printf("%s: %s\n", what, error.what());
  }
  CHECK(rounded != nullptr);
  if (!rounded) {
    return;
  }
  CHECK(BRepCheck_Analyzer(rounded->occt()).IsValid());
  CHECK(count(rounded->occt(), TopAbs_SOLID) == 1);
  CHECK(rounded->notes().empty());
  for (const std::string& note : rounded->notes()) {
    std::printf("%s: note: %s\n", what, note.c_str());
  }
  // At the radius asked for, by the rounding itself: no info (a fillet
  // made 0.1 % smaller, or another way round a failure, says so there).
  CHECK(rounded->infos().empty());
  for (const std::string& info : rounded->infos()) {
    std::printf("%s: info: %s\n", what, info.c_str());
  }
  // The whole rim rounded: no edge left between the loft's side and the
  // cylinder.
  const std::vector<int> left = rim_edges(*rounded);
  CHECK(left.empty());
  for (const int e : left) {
    std::printf("%s: left sharp: %s\n", what, rounded->edge_name(e).c_str());
  }
  // The roundings, as many faces as the pieces the tangent points cut the
  // rim into at least. With Merge (one side face, nothing joined) each is
  // named after the rim's edge it rounds, <feature>:fillet(<edge>)#k. With
  // Keep the side's faces are pieces of one surface, which the fillet joins
  // before it rounds and splits again after (surface_pieces_joined,
  // split_joined_pieces), as for any rounding along them: the names come
  // from that split, not from the rim's pieces.
  int roundings = 0;
  int after_rim = 0;
  std::set<std::string> named;
  const std::string made = std::string(kFillet) + ":fillet(";
  for (int f = 0; f < rounded->face_count(); ++f) {
    if (!named_with(rounded->face_names(f), made)) {
      continue;
    }
    ++roundings;
    bool found = false;
    for (const std::string& name : rounded->face_names(f)) {
      for (const int e : rim) {
        if (g::name_matches(name, g::face_name(kFillet, "fillet", body->edge_name(e)))) {
          named.insert(body->edge_name(e));
          found = true;
        }
      }
    }
    if (found) {
      ++after_rim;
    } else {
      std::printf("%s: rounded face %s\n", what, rounded->face_names(f).front().c_str());
    }
  }
  std::printf("%s: %d rounded faces, %d named after %zu of the %zu rim edges\n", what, roundings, after_rim,
              named.size(), rim.size());
  CHECK(roundings >= static_cast<int>(vanishing.size()));
  if (merge) {
    CHECK(named.size() == rim.size());
    CHECK(after_rim == roundings);
  }
  // Each rounded face on the ball between the loft's side and the
  // cylinder.
  const TopoDS_Compound loft = faces_named(*body, loft_side());
  const TopoDS_Compound cylinder = faces_named(*body, cylinder_side());
  const std::string rounding = std::string(kFillet) + ":fillet(";
  int measured = 0;
  for (int f = 0; f < rounded->face_count(); ++f) {
    if (!named_with(rounded->face_names(f), rounding)) {
      continue;
    }
    const double off = off_ball(rounded->face(f), radius, loft, cylinder);
    if (off < 0.0) {
      continue;
    }
    ++measured;
    if (off > kOnBall) {
      std::printf("%s: %s lies %.3g mm off the ball\n", what, rounded->face_names(f).front().c_str(), off);
    }
    CHECK(off <= kOnBall);
  }
  CHECK(measured > 0);
  // A vertex of the result at each point where the rounding vanishes.
  for (const gp_Pnt& p : vanishing) {
    double nearest = 1.0e100;
    for (TopExp_Explorer v(rounded->occt(), TopAbs_VERTEX); v.More(); v.Next()) {
      nearest = std::min(nearest, BRep_Tool::Pnt(TopoDS::Vertex(v.Current())).Distance(p));
    }
    if (nearest > 1.0e-3) {
      std::printf("%s: no vertex at (%.4f, %.4f, %.4f): the nearest %.3g mm away\n", what, p.X(), p.Y(), p.Z(),
                  nearest);
    }
    CHECK(nearest <= 1.0e-3);
  }
  // Rounding takes material off the convex stretches and adds it in the
  // concave ones: the volume changes, by less than the radius squared
  // times the length of a circle of radius 120 (the rim's is 111).
  const double change = std::abs(volume_of(rounded->occt()) - volume_of(body->occt()));
  CHECK(change > 0.0);
  CHECK(change < 2.0 * kPi * 120.0 * radius * radius);
}

// The kernel's fillet alone (port patches 2300 and 2301) of the body the
// fillet rounds: the owner's body (pull 230) with the pieces of one surface
// joined and the rim split where its rounding vanishes, every piece
// selected, at 2, 5 and 10 mm, each build on a copy of its own (the kernel's
// fillet gives its input's edges curves in place), its contours stopped at
// turns as MitCad's fillet asks: done, and a valid shape. Without 2300 the
// convex pieces failed at the last section of their walks, taken for a
// twist where the rounding has no width; without 2301 the roundings came
// out with a contact edge on the wrong vertex where two of them vanish.
//
// The Keep loft with 2 mm (here and in radii_keep) needs port patches 2302
// and 2303 (mitcad#2390). 2302: the walk of the bottom convex piece crosses
// the seam of the loft's periodic face and ends on the arcs at its
// vanishing end; the end's contact on the loft came out one period (869.7)
// below the walk's points before it, and the approximation's start for the
// sections between them lay on the far side of the surface: the solver
// found balls 100 mm off and the approximation failed ("PerformSurf :
// Failed approximation!"). 2303: the blends were cut into a piece and a
// sliver of no width at minima of noise in their vanishing ends (widths of
// 5e-6, 0.0002 from the end); the top convex piece's build then read its
// one-edge spine past its end and crashed, and with all the pieces at once
// a convex and a concave piece's slivers crossed at their common vertex
// and both were refused as overlapping.
void check_kernel_alone(bool merge) {
  const char* kind = merge ? "Merge" : "Keep";
  const g::ShapePtr body = owner_body(merge, 230);
  const std::vector<int> rim = rim_edges(*body);
  CHECK(!rim.empty());
  if (rim.empty()) {
    return;
  }
  std::vector<int> faces;
  for (int f = 0; f < body->face_count(); ++f) {
    if (named_with(body->face_names(f), loft_side()) || named_with(body->face_names(f), cylinder_side())) {
      faces.push_back(f);
    }
  }
  const g::ShapePtr joined = g::detail::join_surface_pieces(*body, faces);
  CHECK(joined != nullptr);
  if (!joined) {
    return;
  }
  g::FilletSet set;
  set.edges = {body->edge_name(rim.front())};
  set.radius = 5;
  std::string why;
  const std::optional<g::detail::VanishingSplit> split = g::detail::split_at_vanishing_points(*joined, {set}, &why);
  CHECK(split.has_value());
  if (!split) {
    std::printf("%s: not split: %s\n", kind, why.c_str());
    return;
  }
  const g::Shape& on = *split->body;
  std::vector<int> picked;
  for (const std::string& name : split->sets.front().edges) {
    for (const int e : on.find_edges(name)) {
      picked.push_back(e);
    }
  }
  for (const double radius : {2.0, 5.0, 10.0}) {
    BRepBuilderAPI_Copy copy(on.occt());
    NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> copied;
    for (const int e : picked) {
      copied.Add(copy.Modified(on.edge(e)).First());
    }
    BRepFilletAPI_MakeFillet maker(copy.Shape());
    maker.StopChainsAtTurns(copied);
    for (const int e : picked) {
      const TopoDS_Edge edge = TopoDS::Edge(copy.Modified(on.edge(e)).First());
      if (maker.Contour(edge) == 0) {
        maker.Add(radius, edge);
      }
    }
    try {
      maker.Build();
    } catch (const Standard_Failure& failure) {
      std::printf("%s r %g: the kernel's fillet raised %s\n", kind, radius, failure.what());
    }
    const bool valid = maker.IsDone() && BRepCheck_Analyzer(maker.Shape()).IsValid();
    std::printf("%s r %g: the kernel's fillet of %zu pieces in %d contours: done %d, failure %d, valid %d\n", kind,
                radius, picked.size(), maker.NbContours(), maker.IsDone() ? 1 : 0, static_cast<int>(maker.Failure()),
                valid ? 1 : 0);
    CHECK(valid);
  }
}

// --probe <dir>: what the fillet rounds and how the kernel's fillet alone
// fares on it, with Merge and with Keep (pull 230): the body, the body with
// the pieces of one surface joined (as the fillet joins them) and that body
// split where the rim's rounding vanishes, written as BREP files into <dir>
// (for the OCCT lab); the split body's selected edges; at 2, 5 and 10 mm
// the kernel's fillet alone of all of them, each build on a copy of its own,
// with its contours stopped at turns (as MitCad's fillet asks) and without
// (as DRAW's blend); and MitCad's fillet of the body with its notes and
// infos. With --trace the walk of Keep's kernel fillet at 5 mm is traced
// (port patch 2141) on the standard error.
void set_trace(bool on) {
#ifdef _WIN32
  _putenv_s("MITCAD_TRACE_FILLET_WALK", on ? "1" : "");
#else
  if (on) {
    setenv("MITCAD_TRACE_FILLET_WALK", "1", 1);
  } else {
    unsetenv("MITCAD_TRACE_FILLET_WALK");
  }
#endif
}

void probe(const std::string& dir, bool trace) {
  // (Unbuffered: what was printed stays if the probe crashes.)
  std::setvbuf(stdout, nullptr, _IONBF, 0);
  std::filesystem::create_directories(dir);
  for (const bool merge : {true, false}) {
    const char* kind = merge ? "merge" : "keep";
    const g::ShapePtr body = owner_body(merge, 230);
    const std::vector<int> rim = rim_edges(*body);
    std::printf("== %s: %zu rim edges\n", kind, rim.size());
    if (rim.empty()) {
      continue;
    }
    std::vector<int> faces;
    for (int f = 0; f < body->face_count(); ++f) {
      if (named_with(body->face_names(f), loft_side()) || named_with(body->face_names(f), cylinder_side())) {
        faces.push_back(f);
      }
    }
    g::ShapePtr joined = g::detail::join_surface_pieces(*body, faces);
    if (!joined) {
      std::printf("%s: the pieces do not join\n", kind);
      joined = body;
    }
    g::FilletSet set;
    set.edges = {body->edge_name(rim.front())};
    set.radius = 5;
    std::string why;
    const std::optional<g::detail::VanishingSplit> split = g::detail::split_at_vanishing_points(*joined, {set}, &why);
    BRepTools::Write(body->occt(), (dir + "/" + kind + "_body.brep").c_str());
    BRepTools::Write(joined->occt(), (dir + "/" + kind + "_joined.brep").c_str());
    std::printf("%s: joined body %d faces, %d edges; rim on it %zu edges\n", kind, joined->face_count(),
                joined->edge_count(), rim_edges(*joined).size());
    if (!split) {
      std::printf("%s: not split: %s\n", kind, why.c_str());
      continue;
    }
    const g::Shape& on = *split->body;
    BRepTools::Write(on.occt(), (dir + "/" + kind + "_split.brep").c_str());
    std::vector<int> picked;
    for (const std::string& name : split->sets.front().edges) {
      for (const int e : on.find_edges(name)) {
        picked.push_back(e);
        const BRepAdaptor_Curve c(on.edge(e));
        const gp_Pnt a = c.Value(c.FirstParameter());
        const gp_Pnt b = c.Value(c.LastParameter());
        std::printf("%s: selected %s (index %d), from (%.3f %.3f %.3f) to (%.3f %.3f %.3f)\n", kind, name.c_str(), e,
                    a.X(), a.Y(), a.Z(), b.X(), b.Y(), b.Z());
      }
    }
    for (const double radius : {2.0, 5.0, 10.0}) {
      // The kernel's fillet of all the selected edges, each build on a
      // copy of its own (the fillet gives its input's edges curves in
      // place), with the contours stopped at turns as round_edges asks and
      // without (as DRAW's blend builds them).
      for (const bool stop : {true, false}) {
        BRepBuilderAPI_Copy copy(on.occt());
        NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> copied;
        for (const int e : picked) {
          copied.Add(copy.Modified(on.edge(e)).First());
        }
        BRepFilletAPI_MakeFillet all(copy.Shape());
        if (stop) {
          all.StopChainsAtTurns(copied);
        }
        for (const int e : picked) {
          const TopoDS_Edge edge = TopoDS::Edge(copy.Modified(on.edge(e)).First());
          if (all.Contour(edge) == 0) {
            all.Add(radius, edge);
          }
        }
        set_trace(trace && !merge && radius == 5.0 && stop);
        try {
          all.Build();
        } catch (const Standard_Failure& failure) {
          std::printf("%s r %g stop %d: the kernel's fillet raised %s\n", kind, radius, stop ? 1 : 0, failure.what());
        }
        set_trace(false);
        std::printf("%s r %g stop %d: the kernel's fillet: %d contours, done %d, failure %d, valid %d\n", kind, radius,
                    stop ? 1 : 0, all.NbContours(), all.IsDone() ? 1 : 0, static_cast<int>(all.Failure()),
                    all.IsDone() && BRepCheck_Analyzer(all.Shape()).IsValid() ? 1 : 0);
      }
      // MitCad's fillet of the body as the test asks it.
      g::FilletSet asked;
      asked.edges = {body->edge_name(rim.front())};
      asked.radius = radius;
      try {
        const g::ShapePtr made = g::fillet(kFillet, *body, {asked});
        std::printf("%s r %g: MitCad's fillet: %zu notes, %zu infos\n", kind, radius, made->notes().size(),
                    made->infos().size());
        for (const std::string& info : made->infos()) {
          std::printf("%s r %g: info: %s\n", kind, radius, info.c_str());
        }
      } catch (const std::exception& error) {
        std::printf("%s r %g: MitCad's fillet fails: %s\n", kind, radius, error.what());
      }
    }
  }
}

// No vanishing point on an edge that keeps one convexity: the cylinder's
// far end, convex all round.
void check_far_end_unsplit(bool merge) {
  const g::ShapePtr body = owner_body(merge, 230);
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> ancestors;
  TopExp::MapShapesAndUniqueAncestors(body->occt(), TopAbs_EDGE, TopAbs_FACE, ancestors);
  const std::string end = std::string(kPull) + ":end(";
  int ends = 0;
  for (int e = 0; e < body->edge_count(); ++e) {
    const std::array<int, 2> faces = body->edge_faces(e);
    if (faces[0] < 0 || faces[1] < 0 || faces[0] == faces[1]) {
      continue;
    }
    if (!(named_with(body->face_names(faces[0]), end) || named_with(body->face_names(faces[1]), end))) {
      continue;
    }
    const NCollection_List<TopoDS_Shape>& at = ancestors.FindFromKey(body->edge(e));
    if (at.Extent() != 2) {
      continue;
    }
    ++ends;
    CHECK(g::detail::vanishing_parameters(body->edge(e), TopoDS::Face(at.First()), TopoDS::Face(at.Last())).empty());
  }
  CHECK(ends > 0);
}

} // namespace
} // namespace test

MITCAD_BUNDLED_TEST_ARGS(fillet_loft_rim) {
  const std::string part = argc > 1 ? argv[1] : "";
  if (part == "--probe" && argc > 2) {
    test::probe(argv[2], argc > 3 && std::string(argv[3]) == "--trace");
    return 0;
  }
  const auto run = [&](const char* name, auto&& f) {
    if (part.empty() || part == name) {
      test::guarded(name, f);
    }
  };
  run("owner_merge", [] { test::check_rim_rounded(true, 230, 5); });
  run("owner_keep", [] { test::check_rim_rounded(false, 230, 5); });
  run("radii_merge", [] {
    test::check_rim_rounded(true, 230, 2);
    test::check_rim_rounded(true, 230, 10);
  });
  run("radii_keep", [] {
    test::check_rim_rounded(false, 230, 2);
    test::check_rim_rounded(false, 230, 10);
  });
  run("pulls_merge", [] {
    test::check_rim_rounded(true, 50, 5);
    test::check_rim_rounded(true, 400, 5);
  });
  run("pulls_keep", [] {
    test::check_rim_rounded(false, 50, 5);
    test::check_rim_rounded(false, 400, 5);
  });
  run("far_end_unsplit", [] {
    test::check_far_end_unsplit(true);
    test::check_far_end_unsplit(false);
  });
  run("kernel_merge", [] { test::check_kernel_alone(true); });
  run("kernel_keep", [] { test::check_kernel_alone(false); });
  if (test::failures > 0) {
    std::fprintf(stderr, "%d check(s) failed\n", test::failures);
    return 1;
  }
  std::printf("test_fillet_loft_rim: all passed\n");
  return 0;
}
