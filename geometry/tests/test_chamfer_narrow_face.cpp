// SPDX-License-Identifier: MIT
// Chamfers wider than a narrow face beside their edge (mitcad#1232): the
// chamfer's face goes on past the narrow face's far edge, cut by the face
// beyond, and the narrow face vanishes only where the chamfer is wider.
// Synthetic bodies through the geometry facade (and OCCT's chamfer alone):
// a set screw's hexagonal socket in its end face, a narrow ring between the
// socket and the thread's end chamfer cone; a wall narrower than the
// chamfer along the whole edge; narrow faces on both sides of the edge; a
// cup's rim, a closed circle; the bore's edge at the start of a modelled
// thread, the chamfer meeting the thread's flanks continued along their
// own surfaces; a cylinder's band before a flange rising past it
// (mitcad#1238). The expected volumes are those of the body
// less its part above the chamfers' planes (or cone), cut out with OCCT's
// boolean of half-spaces or integrated exactly.

#include <cmath>
#include <cstdio>
#include <string>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepGProp.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeHalfSpace.hxx>
#include <BRepPrimAPI_MakePrism.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "check.hpp"
#include "mitcad/analysis/properties.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/naming.hpp"
#include "mitcad/geometry/thread.hpp"

namespace test {
namespace {

using namespace mitcad::geometry;

// The volume by the analysis library's integrals, which follow the spans
// of the faces' curves (OCCT's adaptive rule misses a thread's faces).
double volume_of(const TopoDS_Shape& shape) { return mitcad::analysis::physical_properties(shape).volume; }

double area_of(const TopoDS_Shape& face) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(face, props);
  return props.Mass();
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

// The index of the edge whose middle is at p (-1 when none is).
int edge_index_at(const Shape& shape, const gp_Pnt& p) {
  for (int i = 0; i < shape.edge_count(); ++i) {
    if (BRep_Tool::Degenerated(shape.edge(i))) {
      continue;
    }
    const BRepAdaptor_Curve curve(shape.edge(i));
    if (curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter())).Distance(p) < 1e-6) {
      return i;
    }
  }
  return -1;
}

std::string edge_at(const Shape& shape, const gp_Pnt& p) {
  const int i = edge_index_at(shape, p);
  return i < 0 ? std::string() : shape.edge_name(i);
}

// The name of the face all of whose vertices lie at height z (empty when
// none does).
std::string face_at_height(const Shape& shape, double z) {
  for (int f = 0; f < shape.face_count(); ++f) {
    bool all = true;
    for (TopExp_Explorer v(shape.face(f), TopAbs_VERTEX); v.More() && all; v.Next()) {
      all = std::abs(BRep_Tool::Pnt(TopoDS::Vertex(v.Current())).Z() - z) < 1e-9;
    }
    if (all && !shape.face_names(f).empty()) {
      return shape.face_names(f).front();
    }
  }
  return {};
}

bool valid(const Shape& shape) { return BRepCheck_Analyzer(shape.occt()).IsValid(); }

// How near the axis (z) the faces holding the name come (infinite for
// none).
double nearest_to_axis(const Shape& shape, const std::string& name) {
  const TopoDS_Edge axis = BRepBuilderAPI_MakeEdge(gp_Pnt(0, 0, -1000), gp_Pnt(0, 0, 1000)).Edge();
  double nearest = 1e300;
  for (int f = 0; f < shape.face_count(); ++f) {
    if (face_matches(shape.face_names(f), name)) {
      BRepExtrema_DistShapeShape distance(shape.face(f), axis);
      nearest = std::min(nearest, distance.Value());
    }
  }
  return nearest;
}

// The narrowest face's width: twice its area over its boundary's length.
double narrowest_face(const Shape& shape) {
  double narrowest = 1e300;
  for (int f = 0; f < shape.face_count(); ++f) {
    double boundary = 0.0;
    for (TopExp_Explorer ex(shape.face(f), TopAbs_EDGE); ex.More(); ex.Next()) {
      if (!BRep_Tool::Degenerated(TopoDS::Edge(ex.Current()))) {
        GProp_GProps length;
        BRepGProp::LinearProperties(ex.Current(), length);
        boundary += length.Mass();
      }
    }
    narrowest = std::min(narrowest, 2.0 * area_of(shape.face(f)) / boundary);
  }
  return narrowest;
}

// The faces holding the name or one of its pieces (name#k), and their
// total area.
int faces_named(const Shape& shape, const std::string& name, double* area = nullptr) {
  int count = 0;
  for (int f = 0; f < shape.face_count(); ++f) {
    if (face_matches(shape.face_names(f), name)) {
      ++count;
      if (area != nullptr) {
        *area += area_of(shape.face(f));
      }
    }
  }
  return count;
}

// A half-space: the side of the plane through `origin` that `normal`
// points to.
struct Half {
  gp_Pnt origin;
  gp_Dir normal; // points into the half-space
};

// The part of `body` inside every half-space, by OCCT's booleans on a box
// of 200 about the origin.
TopoDS_Shape common_with(const TopoDS_Shape& body, const std::vector<Half>& halves) {
  TopoDS_Shape region = BRepPrimAPI_MakeBox(gp_Pnt(-100, -100, -100), 200, 200, 200).Shape();
  for (const Half& half : halves) {
    const TopoDS_Face face = BRepBuilderAPI_MakeFace(gp_Pln(half.origin, half.normal)).Face();
    const TopoDS_Shape space =
        BRepPrimAPI_MakeHalfSpace(face, half.origin.Translated(gp_Vec(half.normal))).Solid();
    region = BRepAlgoAPI_Common(region, space).Shape();
  }
  return BRepAlgoAPI_Common(body, region).Shape();
}

// --- The set screw ---------------------------------------------------------

constexpr double kScrewRadius = 6.0;  // the thread's major radius
constexpr double kTopRadius = 5.0;    // the end face's radius, inside the cone
constexpr double kScrewTop = 10.0;    // the end face's height
constexpr double kSocketCorner = 4.0; // the hexagon's circumradius
constexpr double kSocketDepth = 4.0;

// A set screw: a cylinder of radius 6 (z 0..9) with its thread's end
// chamfer, a cone of 45 degrees up to the end face of radius 5 at z = 10,
// and a hexagonal socket of circumradius 4 (inradius 3.46) 4 deep in that
// face, its corners at `turn` + k 60 degrees. The end face is a ring 1.54
// wide at the hexagon's sides' middles and 1 wide at its corners.
TopoDS_Shape set_screw(double turn) {
  BRepBuilderAPI_MakePolygon profile;
  profile.Add(gp_Pnt(0, 0, 0));
  profile.Add(gp_Pnt(kScrewRadius, 0, 0));
  profile.Add(gp_Pnt(kScrewRadius, 0, kScrewTop - (kScrewRadius - kTopRadius)));
  profile.Add(gp_Pnt(kTopRadius, 0, kScrewTop));
  profile.Add(gp_Pnt(0, 0, kScrewTop));
  profile.Close();
  const TopoDS_Face section = BRepBuilderAPI_MakeFace(profile.Wire(), true).Face();
  const TopoDS_Shape screw = BRepPrimAPI_MakeRevol(section, gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1))).Shape();
  BRepBuilderAPI_MakePolygon hexagon;
  for (int k = 0; k < 6; ++k) {
    const double a = turn + k * kPi / 3;
    hexagon.Add(gp_Pnt(kSocketCorner * std::cos(a), kSocketCorner * std::sin(a), kScrewTop - kSocketDepth));
  }
  hexagon.Close();
  const TopoDS_Face base = BRepBuilderAPI_MakeFace(hexagon.Wire(), true).Face();
  const TopoDS_Shape socket = BRepPrimAPI_MakePrism(base, gp_Vec(0, 0, kSocketDepth + 1)).Shape();
  return BRepAlgoAPI_Cut(screw, socket).Shape();
}

// The middle of the socket's rim edge k and its outward normal in the plane.
gp_Pnt rim_middle(double turn, int k) {
  const double a = turn + (k + 0.5) * kPi / 3;
  const double inradius = kSocketCorner * std::cos(kPi / 6);
  return gp_Pnt(inradius * std::cos(a), inradius * std::sin(a), kScrewTop);
}

gp_Dir rim_normal(double turn, int k) {
  const double a = turn + (k + 0.5) * kPi / 3;
  return gp_Dir(std::cos(a), std::sin(a), 0);
}

// The half-space above the chamfer of rim edge k: `top` on the end face,
// `wall` down the socket's wall.
Half above_chamfer(double turn, int k, double top, double wall) {
  const gp_Pnt middle = rim_middle(turn, k);
  const gp_Dir out = rim_normal(turn, k);
  // The chamfer's plane holds its contacts, `top` out on the end face and
  // `wall` down the wall; its normal top z - wall out points to the edge.
  const gp_Pnt on_top = middle.Translated(gp_Vec(out) * top);
  return {on_top, gp_Dir(gp_Vec(out) * -wall + gp_Vec(0, 0, top))};
}

// The half-space beyond the socket's wall k (away from the socket's
// inside), or inside it.
Half socket_side(double turn, int k, bool inside) {
  const gp_Dir out = rim_normal(turn, k);
  return {rim_middle(turn, k), inside ? out.Reversed() : out};
}

// Diagnostics: what the chamfer gave.
void report(const char* what, const ShapePtr& result, double expected) {
  std::fprintf(stderr, "%s: valid %d, volume %.9f, expected %.9f, faces %d, narrowest %.3g\n", what,
               valid(*result) ? 1 : 0, volume_of(result->occt()), expected, result->face_count(),
               narrowest_face(*result));
  for (const std::string& note : result->notes()) {
    std::fprintf(stderr, "  note: %s\n", note.c_str());
  }
  for (const std::string& info : result->infos()) {
    std::fprintf(stderr, "  info: %s\n", info.c_str());
  }
}

// The owner's case (mitcad#1232): the socket's rim edges chamfered 1.5 on
// both faces, one edge or all six, corners of type Chamfer. The chamfer's
// contact on the end face runs past the ring's outer circle onto the cone
// near the hexagon's corners: there the cone cuts the chamfer and the ring
// is gone; along the sides' middles the ring stays, narrower.
void chamfer_the_socket(double turn, bool all, ChamferType type, bool flip) {
  const ShapePtr screw = named(set_screw(turn));
  CHECK(valid(*screw));
  const std::string top = face_at_height(*screw, kScrewTop);
  CHECK(!top.empty());
  std::vector<std::string> edges;
  std::vector<int> ks;
  for (int k = 0; k < (all ? 6 : 1); ++k) {
    edges.push_back(edge_at(*screw, rim_middle(turn, k)));
    ks.push_back(k);
    CHECK(!edges.back().empty());
  }
  // The distances on the end face (top) and down the wall.
  double on_top = 1.5, on_wall = 1.5;
  ChamferSet set;
  set.edges = edges;
  set.spec.type = type;
  set.spec.reference_face = top;
  set.spec.flip = flip;
  switch (type) {
  case ChamferType::EqualDistance:
    set.spec.distance = 1.5;
    break;
  case ChamferType::TwoDistances:
    set.spec.distance = 1.5;
    set.spec.distance2 = 1.0;
    on_top = flip ? 1.0 : 1.5;
    on_wall = flip ? 1.5 : 1.0;
    break;
  case ChamferType::DistanceAngle:
    set.spec.distance = 1.5;
    set.spec.angle = 30.0 * kPi / 180.0;
    on_top = flip ? 1.5 * std::tan(set.spec.angle) : 1.5;
    on_wall = flip ? 1.5 : 1.5 * std::tan(set.spec.angle);
    break;
  }
  // What the exact chamfer takes away: above every chamfer's plane, and for
  // one edge between the walls beside it.
  std::vector<Half> halves;
  for (const int k : ks) {
    halves.push_back(above_chamfer(turn, k, on_top, on_wall));
  }
  // (Not below the socket's floor, where the planes meet under the socket.)
  halves.push_back({gp_Pnt(0, 0, kScrewTop - kSocketDepth), gp_Dir(0, 0, 1)});
  if (!all) {
    halves.push_back(socket_side(turn, 0, false));
    halves.push_back(socket_side(turn, 5, true));
    halves.push_back(socket_side(turn, 1, true));
  }
  const TopoDS_Shape taken = common_with(screw->occt(), halves);
  const double expected = volume_of(screw->occt()) - volume_of(taken);
  ShapePtr result;
  try {
    result = chamfer("C", *screw, {set});
  } catch (const std::exception& e) {
    std::fprintf(stderr, "socket (turn %g, %s, type %d, flip %d): %s\n", turn, all ? "all" : "one",
                 static_cast<int>(type), flip ? 1 : 0, e.what());
    CHECK(false);
    return;
  }
  report("socket", result, expected);
  CHECK(valid(*result));
  CHECK(narrowest_face(*result) > 0.01);
  CHECK_NEAR_TOL(volume_of(result->occt()), expected, 1e-6);
  for (const std::string& e : edges) {
    CHECK(faces_named(*result, "C:chamfer(" + e + ")") >= 1);
  }
  // The ring stays (the sides' middles), smaller.
  double ring = 0.0;
  CHECK(faces_named(*result, top, &ring) >= 1);
  CHECK(result->notes().empty() && result->infos().empty());
}

void test_socket_one_edge() { chamfer_the_socket(kPi / 6, false, ChamferType::EqualDistance, false); }
void test_socket_all_edges() { chamfer_the_socket(kPi / 6, true, ChamferType::EqualDistance, false); }
void test_socket_two_distances() { chamfer_the_socket(kPi / 6, true, ChamferType::TwoDistances, false); }
void test_socket_two_distances_flipped() { chamfer_the_socket(kPi / 6, true, ChamferType::TwoDistances, true); }
void test_socket_distance_angle() { chamfer_the_socket(kPi / 6, true, ChamferType::DistanceAngle, false); }
void test_socket_corner_on_seam() { chamfer_the_socket(0.0, true, ChamferType::EqualDistance, false); }

// OCCT's chamfer alone on the socket's rim.
void test_socket_with_occt_alone() {
  const double turn = kPi / 6;
  const TopoDS_Shape screw = set_screw(turn);
  const ShapePtr shape = named(screw);
  BRepFilletAPI_MakeChamfer maker(screw);
  for (int k = 0; k < 6; ++k) {
    maker.Add(1.5, shape->edge(edge_index_at(*shape, rim_middle(turn, k))));
  }
  try {
    OCC_CATCH_SIGNALS
    maker.Build();
  } catch (const Standard_Failure& failure) {
    std::fprintf(stderr, "socket with OCCT alone: %s\n", failure.what());
  }
  std::fprintf(stderr, "socket with OCCT alone: done %d\n", maker.IsDone() ? 1 : 0);
  CHECK(maker.IsDone());
  if (maker.IsDone()) {
    std::vector<Half> halves;
    for (int k = 0; k < 6; ++k) {
      halves.push_back(above_chamfer(turn, k, 1.5, 1.5));
    }
    halves.push_back({gp_Pnt(0, 0, kScrewTop - kSocketDepth), gp_Dir(0, 0, 1)});
    const double expected = volume_of(screw) - volume_of(common_with(screw, halves));
    CHECK(BRepCheck_Analyzer(maker.Shape()).IsValid());
    CHECK_NEAR_TOL(volume_of(maker.Shape()), expected, 1e-6);
  }
}

// --- Prisms along y ---------------------------------------------------------

// The section (in x, z) extruded along y from 0 to 20.
TopoDS_Shape prism(const std::vector<std::pair<double, double>>& section) {
  BRepBuilderAPI_MakePolygon polygon;
  for (const auto& p : section) {
    polygon.Add(gp_Pnt(p.first, 0, p.second));
  }
  polygon.Close();
  const TopoDS_Face face = BRepBuilderAPI_MakeFace(polygon.Wire(), true).Face();
  return BRepPrimAPI_MakePrism(face, gp_Vec(0, 20, 0)).Shape();
}

// A wall 1 thick (x 0..1) 3 high on a floor 4 thick: its top edge on the
// floor's side (x = 1, z = 7) chamfered 2. The wall's top is narrower than
// the chamfer along the whole edge: it vanishes, the chamfer cut by the
// wall's outer face (x = 0) at z = 6. Taken away: 1.5 mm2 along 20.
void test_wall_narrower_than_the_chamfer() {
  const ShapePtr body = named(prism({{0, 0}, {30, 0}, {30, 4}, {1, 4}, {1, 7}, {0, 7}}));
  const std::string edge = edge_at(*body, gp_Pnt(1, 10, 7));
  CHECK(!edge.empty());
  const double block = 30.0 * 4 * 20 + 3.0 * 20;
  for (const ChamferType type : {ChamferType::EqualDistance, ChamferType::TwoDistances, ChamferType::DistanceAngle}) {
    ChamferSet set;
    set.edges = {edge};
    set.spec.type = type;
    set.spec.distance = 2;
    set.spec.distance2 = 1.5;
    set.spec.angle = 40.0 * kPi / 180.0;
    set.spec.reference_face = face_at_height(*body, 7); // the wall's top
    // Distances on the top (a) and down the inner face (b).
    double a = 2, b = 2;
    if (type == ChamferType::TwoDistances) {
      b = 1.5;
    } else if (type == ChamferType::DistanceAngle) {
      b = 2 * std::tan(set.spec.angle);
    }
    // Above the plane through (1 - a, 7) and (1, 7 - b): at x = 0 it is at
    // 7 - b + b / a; taken away the trapezium between x = 0 and 1.
    const double at0 = 7 - b + b / a;
    const double taken = 0.5 * ((7 - at0) + b) * 20;
    ShapePtr result;
    try {
      result = chamfer("C", *body, {set});
    } catch (const std::exception& e) {
      std::fprintf(stderr, "wall (type %d): %s\n", static_cast<int>(type), e.what());
      CHECK(false);
      continue;
    }
    report("wall", result, block - taken);
    CHECK(valid(*result));
    CHECK(narrowest_face(*result) > 0.01);
    CHECK_NEAR_TOL(volume_of(result->occt()), block - taken, 1e-6);
    CHECK(faces_named(*result, set.spec.reference_face) == 0);
    CHECK(faces_named(*result, "C:chamfer(" + edge + ")") == 1);
  }
}

// The area of a polygon (x, z).
double area_of(const std::vector<std::pair<double, double>>& polygon) {
  double twice = 0.0;
  for (std::size_t i = 0; i < polygon.size(); ++i) {
    const auto& a = polygon[i];
    const auto& b = polygon[(i + 1) % polygon.size()];
    twice += a.first * b.second - b.first * a.second;
  }
  return 0.5 * std::abs(twice);
}

// A ridge of the section, its top (z = 6, x 2..3) 1 wide with a wall going
// down beyond it (x = 2), its side (x = 3, z 5..6) 1 high: the edge between
// top and side chamfered 1.5 (the plane z = 7.5 - x) takes both away, cut
// by the wall at z = 5.5 and by the face beyond the side; taken away the
// polygon `taken` along 20.
void chamfer_the_ridge(const char* what, const std::vector<std::pair<double, double>>& section,
                       const std::vector<std::pair<double, double>>& taken) {
  const ShapePtr body = named(prism(section));
  const std::string edge = edge_at(*body, gp_Pnt(3, 10, 6));
  CHECK(!edge.empty());
  const std::string top = face_at_height(*body, 6);
  ChamferSet set;
  set.edges = {edge};
  set.spec.distance = 1.5;
  const double expected = volume_of(body->occt()) - area_of(taken) * 20;
  ShapePtr result;
  try {
    result = chamfer("C", *body, {set});
  } catch (const std::exception& e) {
    std::fprintf(stderr, "%s: %s\n", what, e.what());
    CHECK(false);
    return;
  }
  report(what, result, expected);
  CHECK(valid(*result));
  CHECK(narrowest_face(*result) > 0.01);
  CHECK_NEAR_TOL(volume_of(result->occt()), expected, 1e-6);
  CHECK(faces_named(*result, "C:chamfer(" + edge + ")") == 1);
  CHECK(faces_named(*result, top) == 0);
}

// Beyond the side an undercut, a face going back under the top to (2.5, 4):
// the chamfer meets it at (17/6, 14/3).
void test_narrow_faces_on_both_sides() {
  chamfer_the_ridge("both sides", {{2, 0}, {10, 0}, {10, 1}, {2.5, 1}, {2.5, 4}, {3, 5}, {3, 6}, {2, 6}},
                    {{2, 5.5}, {17.0 / 6, 14.0 / 3}, {3, 5}, {3, 6}, {2, 6}});
}

// Beyond the side a slope going down and out to (4, 3), across a concave
// edge: the chamfer goes on into the slope's material and meets it at
// (3.5, 4).
void test_narrow_side_before_a_slope() {
  chamfer_the_ridge("side before a slope", {{2, 0}, {10, 0}, {10, 3}, {4, 3}, {3, 5}, {3, 6}, {2, 6}},
                    {{2, 5.5}, {3.5, 4}, {3, 5}, {3, 6}, {2, 6}});
}

// Two narrow faces in a row: the wall's top (z = 7, x 0.5..1), then a step
// 0.2 down to a strip (z = 6.8, x 0..0.5) before the outer face. The wall's
// inner top edge chamfered 2 (the plane z = 6 - x) passes under both and
// meets the outer face at z = 6: both vanish.
void test_two_narrow_faces_in_a_row() {
  const std::vector<std::pair<double, double>> section{{0, 0},   {30, 0},   {30, 4},   {1, 4},
                                                       {1, 7},   {0.5, 7},  {0.5, 6.8}, {0, 6.8}};
  const ShapePtr body = named(prism(section));
  const std::string edge = edge_at(*body, gp_Pnt(1, 10, 7));
  CHECK(!edge.empty());
  const std::string top = face_at_height(*body, 7);
  const std::string strip = face_at_height(*body, 6.8);
  ChamferSet set;
  set.edges = {edge};
  set.spec.distance = 2;
  const double expected =
      volume_of(body->occt()) - area_of({{0, 6}, {1, 5}, {1, 7}, {0.5, 7}, {0.5, 6.8}, {0, 6.8}}) * 20;
  ShapePtr result;
  try {
    result = chamfer("C", *body, {set});
  } catch (const std::exception& e) {
    std::fprintf(stderr, "two narrow faces in a row: %s\n", e.what());
    CHECK(false);
    return;
  }
  report("two narrow faces in a row", result, expected);
  CHECK(valid(*result));
  CHECK(narrowest_face(*result) > 0.01);
  CHECK_NEAR_TOL(volume_of(result->occt()), expected, 1e-6);
  CHECK(faces_named(*result, top) == 0 && faces_named(*result, strip) == 0);
}

// The corner types where an overflowing chamfer meets others: the wall of
// test_wall_narrower_than_the_chamfer with its inner top edge, its end's
// top edge (y = 0) and the inner face's end edge chamfered 2, three
// bevelled edges at the vertex (1, 0, 7). The top (1 wide) and the end
// face above the floor (1 wide) are narrower than the chamfers of the inner
// top edge and of the inner face's end edge (mitcad#1238): the corner is
// built as without the overflow, on the body continued past the outer face
// (x = 0), which then cuts it. Miter: the three planes z = 6 - x,
// z = 5 + y and y = x + 1 (above the floor) meet; taken away 1.5 along 20
// and the integral of (x + 1)(2 - x) from 0 to 1 (13/6) at the corner.
// Chamfer: the geometry kernel's corner face takes a little more. Blend:
// its setback is longer than the end's top edge (1), as without the
// overflow: it fails, its error naming the corner's vertex and the corner
// types that build it. Never smaller than asked.
void test_corner_types_at_an_overflowing_chamfer() {
  const ShapePtr body = named(prism({{0, 0}, {30, 0}, {30, 4}, {1, 4}, {1, 7}, {0, 7}}));
  const std::vector<std::string> edges{edge_at(*body, gp_Pnt(1, 10, 7)), edge_at(*body, gp_Pnt(0.5, 0, 7)),
                                       edge_at(*body, gp_Pnt(1, 0, 5.5))};
  for (const std::string& e : edges) {
    CHECK(!e.empty());
  }
  const double mitred = volume_of(body->occt()) - 1.5 * 20 - 13.0 / 6;
  for (const ChamferCorner corner : {ChamferCorner::Chamfer, ChamferCorner::Miter, ChamferCorner::Blend}) {
    ChamferSet set;
    set.edges = edges;
    set.spec.distance = 2;
    ShapePtr result;
    try {
      result = chamfer("C", *body, {set}, corner);
    } catch (const std::exception& e) {
      std::fprintf(stderr, "corner types (%d): %s\n", static_cast<int>(corner), e.what());
      const std::string message = e.what();
      CHECK(corner == ChamferCorner::Blend);
      CHECK(message.find("vertex V{") != std::string::npos);
      CHECK(message.find("the corner type chamfer or miter builds it") != std::string::npos);
      continue;
    }
    report("corner types", result, mitred);
    CHECK(corner != ChamferCorner::Blend);
    CHECK(valid(*result));
    CHECK(narrowest_face(*result) > 0.01);
    for (const std::string& info : result->infos()) {
      CHECK(info.find("smaller") == std::string::npos);
    }
    CHECK(faces_named(*result, "C:chamfer(" + edges[0] + ")") == 1);
    CHECK(faces_named(*result, "C:chamfer(" + edges[2] + ")") == 1);
    // Nothing left of the material outside x = 0 the body was continued
    // into.
    Bnd_Box b;
    BRepBndLib::Add(result->occt(), b);
    double x0 = 0, y0 = 0, z0 = 0, x1 = 0, y1 = 0, z1 = 0;
    b.Get(x0, y0, z0, x1, y1, z1);
    CHECK(x0 >= -1e-6 && y0 >= -1e-6);
    if (corner == ChamferCorner::Miter) {
      CHECK_NEAR_TOL(volume_of(result->occt()), mitred, 1e-6);
    } else {
      CHECK(volume_of(result->occt()) <= mitred + 1e-6);
      CHECK(volume_of(result->occt()) >= mitred - 0.5);
    }
  }
}

// A cylinder of radius 10 (z 0..10) whose side above a flange (z 0..9) is
// a band 1 high: its top edge chamfered 2 (the cone z = 18 - r) reaches
// past the band's far edge into the flange, which rises past it
// (mitcad#1238: the band is a cylinder, no plane). The band vanishes. A
// flange of radius 12, which the cone comes out of again at r = 12, is cut
// by the cone (taken away: 2 pi times the integral of r (r - 8) from 8 to
// 10 and r (r - 9) from 10 to 12, 190 / 3); a flange of radius 16, which
// it does not come out of near the band, cuts the cone with its top
// continued inward (the plane z = 9: 2 pi times the integral of r (r - 8)
// from 8 to 9 and r from 9 to 10, 83 / 6).
void chamfer_a_band_before_a_flange(double flange, double taken) {
  BRepBuilderAPI_MakePolygon profile;
  for (const auto& p :
       std::vector<std::pair<double, double>>{{0, 0}, {flange, 0}, {flange, 9}, {10, 9}, {10, 10}, {0, 10}}) {
    profile.Add(gp_Pnt(p.first, 0, p.second));
  }
  profile.Close();
  const TopoDS_Face section = BRepBuilderAPI_MakeFace(profile.Wire(), true).Face();
  const ShapePtr body = named(BRepPrimAPI_MakeRevol(section, gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1))).Shape());
  std::string edge;
  for (int i = 0; i < body->edge_count(); ++i) {
    if (BRep_Tool::Degenerated(body->edge(i))) {
      continue;
    }
    const BRepAdaptor_Curve curve(body->edge(i));
    const gp_Pnt p = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
    if (std::abs(p.Z() - 10) < 1e-9 && std::abs(std::hypot(p.X(), p.Y()) - 10) < 1e-9) {
      edge = body->edge_name(i);
    }
  }
  CHECK(!edge.empty());
  ChamferSet set;
  set.edges = {edge};
  set.spec.distance = 2;
  const double expected = volume_of(body->occt()) - 2 * kPi * taken;
  ShapePtr result;
  try {
    result = chamfer("C", *body, {set});
  } catch (const std::exception& e) {
    std::fprintf(stderr, "band before a flange (%g): %s\n", flange, e.what());
    CHECK(false);
    return;
  }
  report("band before a flange", result, expected);
  CHECK(valid(*result));
  CHECK(narrowest_face(*result) > 0.01);
  CHECK_NEAR_TOL(volume_of(result->occt()), expected, 1e-6);
  CHECK(faces_named(*result, "C:chamfer(" + edge + ")") >= 1);
  // The band (the side above the flange) is gone.
  for (int f = 0; f < result->face_count(); ++f) {
    const BRepAdaptor_Surface surface(result->face(f));
    if (surface.GetType() == GeomAbs_Cylinder && std::abs(surface.Cylinder().Radius() - 10) < 1e-9) {
      double z0 = 0, z1 = 0, x0 = 0, y0 = 0, x1 = 0, y1 = 0;
      Bnd_Box b;
      BRepBndLib::Add(result->face(f), b);
      b.Get(x0, y0, z0, x1, y1, z1);
      CHECK(z1 <= 9 + 1e-6);
    }
  }
}

void test_band_before_a_narrow_flange() { chamfer_a_band_before_a_flange(12, 190.0 / 3); }
void test_band_before_a_wide_flange() { chamfer_a_band_before_a_flange(16, 83.0 / 6); }

// A cup: radius 10, 7 high, a pocket of radius 9 from z = 4: its rim a ring
// 1 wide. The inner rim (a whole circle) chamfered 2: the ring vanishes, the
// chamfer's cone cut by the outer cylinder at z = 6. Taken away: the
// section above z = 5 + (r - 9) between r = 9 and 10, turned about the axis.
void test_cup_rim() {
  BRepBuilderAPI_MakePolygon profile;
  for (const auto& p : std::vector<std::pair<double, double>>{{0, 0}, {10, 0}, {10, 7}, {9, 7}, {9, 4}, {0, 4}}) {
    profile.Add(gp_Pnt(p.first, 0, p.second));
  }
  profile.Close();
  const TopoDS_Face section = BRepBuilderAPI_MakeFace(profile.Wire(), true).Face();
  const ShapePtr body =
      named(BRepPrimAPI_MakeRevol(section, gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1))).Shape());
  std::string edge;
  for (int i = 0; i < body->edge_count(); ++i) {
    if (BRep_Tool::Degenerated(body->edge(i))) {
      continue;
    }
    const BRepAdaptor_Curve curve(body->edge(i));
    const gp_Pnt p = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
    if (std::abs(p.Z() - 7) < 1e-9 &&
        std::abs(std::hypot(p.X(), p.Y()) - 9) < 1e-9) {
      edge = body->edge_name(i);
    }
  }
  CHECK(!edge.empty());
  ChamferSet set;
  set.edges = {edge};
  set.spec.distance = 2;
  // 2 pi * integral of r (11 - r) from 9 to 10.
  const auto F = [](double r) { return 11 * r * r / 2 - r * r * r / 3; };
  const double expected = volume_of(body->occt()) - 2 * kPi * (F(10) - F(9));
  ShapePtr result;
  try {
    result = chamfer("C", *body, {set});
  } catch (const std::exception& e) {
    std::fprintf(stderr, "cup rim: %s\n", e.what());
    CHECK(false);
    return;
  }
  report("cup rim", result, expected);
  CHECK(valid(*result));
  CHECK(narrowest_face(*result) > 0.01);
  CHECK_NEAR_TOL(volume_of(result->occt()), expected, 1e-6);
  CHECK(faces_named(*result, "C:chamfer(" + edge + ")") >= 1);
}

// A bore of radius 36 through a ring of radius 45 (z 0..30) with a
// modelled internal thread M76x2 (the basic profile, radial clearance
// 0.15: crests at radius 37.07) along its upper 15, end chamfers at both
// ends. The thread starts at z = 15 with a plane ring from the bore's
// radius 36 out to the crests, 1.07 wide, its outer edges on the flanks.
// The bore's edge there (a whole circle) chamfered 1.1, wider than the
// ring: the ring vanishes, the chamfer's cone (z = 13.9 + (r - 36)) cut by
// the thread's faces beyond it. The cone holds where it is measured, and
// takes away the triangle of legs 1.1 turned about the axis less at most
// the sliver past the crests under the teeth.
void test_thread_start_ring() {
  TopoDS_Shape ring = BRepPrimAPI_MakeCylinder(45, 30).Shape();
  ring = BRepAlgoAPI_Cut(ring, BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -1), gp_Dir(0, 0, 1)), 36, 32).Shape())
             .Shape();
  const ShapePtr body = named(ring);
  std::string bore;
  for (int f = 0; f < body->face_count(); ++f) {
    const BRepAdaptor_Surface surface(body->face(f));
    if (surface.GetType() == GeomAbs_Cylinder && std::abs(surface.Cylinder().Radius() - 36) < 1e-9) {
      bore = body->face_names(f).front();
    }
  }
  CHECK(!bore.empty());
  const double h = std::sqrt(3.0) / 2 * 2;
  ThreadSpec spec;
  spec.feature = "T";
  spec.faces = {bore};
  spec.pitch = 2;
  spec.major = 76;
  spec.minor = 76 - 1.25 * h;
  spec.pitch_diameter = 76 - 0.75 * h;
  spec.radial_clearance = 0.15;
  spec.full_length = false;
  spec.length = 15;
  spec.offset = 0;
  spec.high_end = true;
  spec.chamfer = true;
  spec.chamfer_angle = kPi / 4;
  spec.chamfer_diameter = 76.3;
  spec.chamfer_ends = {{true, true}};
  const ShapePtr threaded = modeled_thread(*body, spec);
  CHECK(valid(*threaded));
  // The bore's edge at the thread's start, and the ring.
  std::string edge;
  for (int i = 0; i < threaded->edge_count(); ++i) {
    if (BRep_Tool::Degenerated(threaded->edge(i))) {
      continue;
    }
    const BRepAdaptor_Curve curve(threaded->edge(i));
    const gp_Pnt p = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
    if (curve.GetType() == GeomAbs_Circle && std::abs(p.Z() - 15) < 1e-6 &&
        std::abs(curve.Circle().Radius() - 36) < 1e-6) {
      edge = threaded->edge_name(i);
    }
  }
  CHECK(!edge.empty());
  const std::string ring_face = face_at_height(*threaded, 15);
  CHECK(!ring_face.empty());
  ChamferSet set;
  set.edges = {edge};
  set.spec.distance = 1.1;
  ShapePtr result;
  try {
    result = chamfer("C", *threaded, {set});
  } catch (const std::exception& e) {
    std::fprintf(stderr, "thread start ring: %s\n", e.what());
    CHECK(false);
    return;
  }
  const double wedge = 2 * kPi * (36 + 1.1 / 3) * 0.5 * 1.1 * 1.1;
  const double taken = volume_of(threaded->occt()) - volume_of(result->occt());
  report("thread start ring", result, volume_of(threaded->occt()) - wedge);
  CHECK(valid(*result));
  CHECK(faces_named(*result, "C:chamfer(" + edge + ")") >= 1);
  // The ring is gone within the chamfer's reach (the plane z = 15 beyond
  // it, at the grooves' ends, is a face of the same name).
  const double ring_left = nearest_to_axis(*result, ring_face);
  std::fprintf(stderr, "thread start ring: the ring's name holds %d faces, %.6f from the axis at least\n",
               faces_named(*result, ring_face), ring_left);
  CHECK(ring_left >= 36 + 1.1 - 1e-6);
  CHECK(taken <= wedge + 1e-6 * wedge);
  CHECK(taken >= wedge - 0.15);
  // The chamfer's contact on the thread's flanks (mitcad#1238): the edges
  // between the chamfer and a face on a flank's surface lie on that
  // surface (the flank's B-spline surface, which goes on a little past the
  // plane z = 15 where the chamfer meets it), not on a face grown straight
  // down from the flank's foot.
  {
    std::vector<std::pair<std::string, TopoDS_Face>> flanks;
    for (int f = 0; f < threaded->face_count(); ++f) {
      const BRepAdaptor_Surface surface(threaded->face(f));
      if (surface.GetType() == GeomAbs_BSplineSurface && !threaded->face_names(f).empty()) {
        flanks.emplace_back(threaded->face_names(f).front(), threaded->face(f));
      }
    }
    NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces_of;
    TopExp::MapShapesAndAncestors(result->occt(), TopAbs_EDGE, TopAbs_FACE, faces_of);
    int edges = 0;
    double worst = 0.0;
    for (int f = 0; f < result->face_count(); ++f) {
      if (!face_matches(result->face_names(f), "C:chamfer(" + edge + ")")) {
        continue;
      }
      for (TopExp_Explorer ex(result->face(f), TopAbs_EDGE); ex.More(); ex.Next()) {
        for (const TopoDS_Shape& other : faces_of.FindFromKey(ex.Current())) {
          const int o = result->face_index(other);
          if (o < 0 || o == f) {
            continue;
          }
          for (const auto& [name, flank] : flanks) {
            if (!face_matches(result->face_names(o), name)) {
              continue;
            }
            ++edges;
            const BRepAdaptor_Curve curve(TopoDS::Edge(ex.Current()));
            for (int k = 1; k < 8; ++k) {
              const gp_Pnt p =
                  curve.Value(curve.FirstParameter() + (curve.LastParameter() - curve.FirstParameter()) * k / 8);
              GeomAPI_ProjectPointOnSurf on_flank(p, BRep_Tool::Surface(flank));
              worst = std::max(worst, on_flank.NbPoints() > 0 ? on_flank.LowerDistance() : 1e300);
            }
          }
        }
      }
    }
    std::fprintf(stderr, "thread start ring: %d contact edges on the flanks, %.3g from them at most\n", edges, worst);
    CHECK(edges >= 1);
    CHECK(worst <= 1e-6);
  }
  // The cone where it is measured: a hundredth above it is outside, below
  // inside, all round.
  for (int k = 0; k < 12; ++k) {
    const double a = k * kPi / 6;
    const double r = 36.55;
    const double z = 13.9 + (r - 36);
    for (const double dz : {0.01, -0.01}) {
      BRepClass3d_SolidClassifier classifier(result->occt(), gp_Pnt(r * std::cos(a), r * std::sin(a), z + dz),
                                             1e-7);
      CHECK(classifier.State() == (dz > 0 ? TopAbs_OUT : TopAbs_IN));
    }
  }
}

} // namespace
} // namespace test

int main() {
  test::guarded("socket one edge", test::test_socket_one_edge);
  test::guarded("socket all edges", test::test_socket_all_edges);
  test::guarded("socket two distances", test::test_socket_two_distances);
  test::guarded("socket two distances flipped", test::test_socket_two_distances_flipped);
  test::guarded("socket distance angle", test::test_socket_distance_angle);
  test::guarded("socket corner on seam", test::test_socket_corner_on_seam);
  test::guarded("socket with OCCT alone", test::test_socket_with_occt_alone);
  test::guarded("wall narrower than the chamfer", test::test_wall_narrower_than_the_chamfer);
  test::guarded("narrow faces on both sides", test::test_narrow_faces_on_both_sides);
  test::guarded("narrow side before a slope", test::test_narrow_side_before_a_slope);
  test::guarded("thread start ring", test::test_thread_start_ring);
  test::guarded("two narrow faces in a row", test::test_two_narrow_faces_in_a_row);
  test::guarded("corner types", test::test_corner_types_at_an_overflowing_chamfer);
  test::guarded("band before a narrow flange", test::test_band_before_a_narrow_flange);
  test::guarded("band before a wide flange", test::test_band_before_a_wide_flange);
  test::guarded("cup rim", test::test_cup_rim);
  if (test::failures == 0) {
    std::puts("test_chamfer_narrow_face: all checks passed");
  }
  return test::failures == 0 ? 0 : 1;
}
