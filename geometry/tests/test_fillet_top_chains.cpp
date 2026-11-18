// SPDX-License-Identifier: MIT
// A U-shaped fin's top rounded along its outer and its inner edges, two
// chains whose blends lie on the one top face, with OCCT's fillet alone
// (mitcad#1926, the port's patches 1710 and 1711).
//
// The case `fillet-wall-run-on` of MITCAD_GEOMETRY_CASES: `body.brep`, a
// reported design's body before its rounding (GitHub #371's design, the
// state before Fillet1); see test_fillet_wall_run_on.cpp for the body. The
// fin's top is one plane face, a U whose arms are 2 wide; its outer edges
// (eight) and its inner edges (seven) each run round the U, so that every
// edge of both chains bounds the top.
//
// 1710: rounded with 0.5, the two chains' blends lie 1 apart on the top.
// OCCT judged them overlapping on the top (each contact measured against
// the first edge of the other chain on the top, on the other arm, instead
// of the edge the other blend runs along) and failed. Checks: the rounding
// builds, valid, the material it takes away that of the outer chain and the
// inner chain rounded alone, within a thousandth (the two meet only at the
// arms' ends, a millimetre apart).
//
// 1711: with the foot's rounding of 3 as well (which consumes each arm's
// outer side, port patch 1700), the corner where an arm's outer side, its
// top and the plate meet met the top's outer edge twice in its parse round
// the vertex, read past a stripe's surfdata and did not return. 1728: the
// top's outer 0.5 now rides on from where its contact on the wall crosses
// the foot's, over the foot's rounding down to the plate, which cuts both.
// Checks: built, valid, the 0.5's ends on the plate where the walk puts
// them.
//
// The design's whole rounding, fifteen sets: twelve of them build together
// with the port's patches, valid (test_twelve_sets); the other three need a
// rounding that rolls on another rounding's surface.
//
// 1722: the tower's top, a horseshoe 2.127 wide at the legs and 3 (2 + 1)
// at its apex, rounded along its inner edge with 2 and its outer edge with
// 1 (the design's sets 4 and 5) by OCCT's fillet alone: each rounding fits
// on the top alone, their contacts pass each other on it (touching at the
// apex), and the smaller ball rides tangent on the larger rounding all
// along (port patches 1720 and 1722; before 1722 the mitred ends at the
// legs' fronts failed). Checks: built, valid; the outer rounding's contact
// on the outer side at a leg's front lies where the file's stored body has
// it (z = -505.566, within a thousandth).
//
// 1723: with the legs' inner front edges rounded with 0.5 too (set 12),
// which reach past the ride's point there, the corner of three blends:
// built, valid, its points at a leg's front where the stored body has them;
// and the twelve sets with set 5 (thirteen of the fifteen) build together,
// valid.
//
// 1725: with set 6 too (the tower's outer foot of 2, whose rider is the
// plate's rim rounded all round, a closed contour), fourteen of the fifteen
// build together, valid.
//
// 1726: the fin's foot rounded with 3 beside the pit inside the U, whose
// edge with the plate is rounded with 5: 0700's stripes under another's
// ball took that edge for one under the foot's rounding (its ball sought
// with the pit's radius, so never found, and the test skipped) and dropped
// the pit's rounding, the result done and valid without it. Checks: the
// two build together, valid, the pit's cylinder of 5 in the result, the
// material that of the two made alone. The pit's five edges (its edge, its
// sides' edges with the arms' inner walls and those walls' feet) now take
// part in the sets and fail at the pit corners: the twelve, thirteen and
// fourteen sets build together, valid, without them, and with them are
// never done without the pit's rounding.
//
// 1727: the inner arm ends, the arms' inner feet rounded with 1 (with the
// pit's sides' edges) and the top's edges with 0.5: at each inner arm end
// the arm's inner wall narrows to nothing between the two roundings before
// the end vertex, and the top's 0.5 rides on over the pit corner's patch
// and the foot's rounding, cut by the plate at the end (before, the corner
// failed). Checks: built, valid; the 0.5's end on the plate where the
// foot's rounding meets the plate (y 435.283, where the file's stored body
// has the foot's end, 435.28) and where its contact on the top meets the
// plate, on the top's edge with the plate prolonged past the end vertex.
//
// 1728: the whole of the design's Fillet1 in one fillet (the twelve sets
// with sets 5, 6 and 2): built, valid, its volume within 3 mm3 of the
// body stored in the file.
//
// 1781 (mitcad#1987): the same with the top's 0.5 given right after the
// foot's 3, as MitCad's fillet gives the file's sets: the corner patches
// beside the pit (the wall's foot rounded with 1 or 0.8 between the
// plate's rounding of 5 and the arm's end) had vertices 0.54 and 0.66 off
// in that order. Checks: every vertex within 0.05, and the same volume in
// both orders.

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <string>
#include <iterator>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_IndexedMap.hxx>
#include <NCollection_List.hxx>
#include <NCollection_Map.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Ax1.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>

namespace {

int failures = 0;

void check(bool ok, const char* what) {
  if (!ok) {
    std::fprintf(stderr, "check failed: %s\n", what);
    ++failures;
  }
}

std::string cases_dir() {
#ifdef _MSC_VER
  char* value = nullptr;
  std::size_t length = 0;
  std::string out;
  if (_dupenv_s(&value, &length, "MITCAD_GEOMETRY_CASES") == 0 && value != nullptr) {
    out = value;
    std::free(value);
  }
  return out;
#else
  const char* value = std::getenv("MITCAD_GEOMETRY_CASES");
  return value != nullptr ? std::string(value) : std::string();
#endif
}

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

const double kPlate = -547.0;
const gp_Dir kTopNormal(0.0, 0.5, std::sqrt(3.0) / 2.0);

gp_Pnt middle_of(const TopoDS_Edge& e) {
  const BRepAdaptor_Curve c(e);
  return c.Value(0.5 * (c.FirstParameter() + c.LastParameter()));
}

bool on_plate(const TopoDS_Edge& e) {
  const BRepAdaptor_Curve c(e);
  return std::abs(c.Value(c.FirstParameter()).Z() - kPlate) < 1e-6 &&
         std::abs(c.Value(c.LastParameter()).Z() - kPlate) < 1e-6 && std::abs(middle_of(e).Z() - kPlate) < 1e-6;
}

// The fin's top: the plane with the top's normal.
TopoDS_Face top_of(const TopoDS_Shape& body) {
  for (TopExp_Explorer ex(body, TopAbs_FACE); ex.More(); ex.Next()) {
    const BRepAdaptor_Surface s(TopoDS::Face(ex.Current()));
    if (s.GetType() == GeomAbs_Plane && s.Plane().Axis().Direction().IsParallel(kTopNormal, 1e-6)) {
      return TopoDS::Face(ex.Current());
    }
  }
  return TopoDS_Face();
}

// The top's edges but those with the plate at the arms' ends: the outer
// chain (the edges whose other face is an outer side or the curved sides
// round the U's bottom, outside the inner sides) and the inner one.
void top_chains(const TopoDS_Shape& body, std::vector<TopoDS_Edge>& outer, std::vector<TopoDS_Edge>& inner) {
  const TopoDS_Face top = top_of(body);
  if (top.IsNull()) {
    return;
  }
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces_of;
  TopExp::MapShapesAndAncestors(body, TopAbs_EDGE, TopAbs_FACE, faces_of);
  // The inner chain: the edges reachable from the inner sides' edges
  // (x = -287.931 and x = -275.431) through the top's vertices, the U's
  // inside y >= 424.272 between them; the outer one all the others.
  for (TopExp_Explorer ex(top, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge& e = TopoDS::Edge(ex.Current());
    if (BRep_Tool::Degenerated(e) || on_plate(e)) {
      continue;
    }
    const gp_Pnt m = middle_of(e);
    const bool is_inner = m.X() > -287.94 && m.X() < -275.42 && m.Y() > 424.0;
    (is_inner ? inner : outer).push_back(e);
  }
}

// Whether `shape` has a vertex within 1e-3 of `p`.
bool has_vertex_near(const TopoDS_Shape& shape, const gp_Pnt& p);

// Rounds `edges` of `body` with `radius`; the result, or a null shape.
TopoDS_Shape round_with(const std::vector<TopoDS_Edge>& edges, double radius, BRepFilletAPI_MakeFillet& maker) {
  for (const TopoDS_Edge& e : edges) {
    if (maker.Contour(e) == 0) {
      maker.Add(radius, e);
    }
  }
  try {
    OCC_CATCH_SIGNALS
    maker.Build();
  } catch (const Standard_Failure& e) {
    std::fprintf(stderr, "the fillet throws: %s\n", e.what());
  }
  return maker.IsDone() ? maker.Shape() : TopoDS_Shape();
}

// The outer foot, as test_fillet_wall_run_on.cpp takes it.
std::vector<TopoDS_Edge> outer_foot(const TopoDS_Shape& body) {
  std::vector<TopoDS_Edge> edges;
  NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces_of;
  TopExp::MapShapesAndAncestors(body, TopAbs_EDGE, TopAbs_FACE, faces_of);
  for (int i = 1; i <= faces_of.Extent(); ++i) {
    const TopoDS_Edge& e = TopoDS::Edge(faces_of.FindKey(i));
    if (faces_of(i).Extent() != 2 || BRep_Tool::Degenerated(e) || !on_plate(e)) {
      continue;
    }
    const BRepAdaptor_Curve c(e);
    const gp_Pnt a = c.Value(c.FirstParameter());
    const gp_Pnt b = c.Value(c.LastParameter());
    const gp_Pnt m = middle_of(e);
    if (m.X() < -290.0 || m.X() > -273.4 || m.Y() < 418.0 || m.Y() > 435.4) {
      continue;
    }
    if (std::abs(a.Y() - b.Y()) < 1e-6 && std::abs(a.Y() - 435.417) < 1e-3) {
      continue;
    }
    if (m.X() > -287.94 && m.X() < -275.42 && m.Y() > 422.5) {
      continue;
    }
    edges.push_back(e);
  }
  return edges;
}

void test_reported_body() {
  const std::string dir = cases_dir();
  if (dir.empty()) {
    std::printf("test_reported_body: skipped (MITCAD_GEOMETRY_CASES is not set)\n");
    return;
  }
  TopoDS_Shape body;
  BRep_Builder builder;
  if (!BRepTools::Read(body, (dir + "/fillet-wall-run-on/body.brep").c_str(), builder) || body.IsNull()) {
    std::printf("test_reported_body: skipped (no case fillet-wall-run-on)\n");
    return;
  }
  std::vector<TopoDS_Edge> outer;
  std::vector<TopoDS_Edge> inner;
  top_chains(body, outer, inner);
  check(outer.size() == 8, "the top's outer chain has eight edges");
  check(inner.size() == 7, "the top's inner chain has seven edges");
  const double before = volume_of(body);

  // 1710: both chains in one fillet.
  std::vector<TopoDS_Edge> both = outer;
  both.insert(both.end(), inner.begin(), inner.end());
  BRepFilletAPI_MakeFillet together(body);
  const TopoDS_Shape rounded = round_with(both, 0.5, together);
  check(!rounded.IsNull(), "the top's outer and inner edges round together");
  if (!rounded.IsNull()) {
    check(BRepCheck_Analyzer(rounded).IsValid(), "the rounding is valid");
    BRepFilletAPI_MakeFillet outer_maker(body);
    BRepFilletAPI_MakeFillet inner_maker(body);
    const TopoDS_Shape outer_only = round_with(outer, 0.5, outer_maker);
    const TopoDS_Shape inner_only = round_with(inner, 0.5, inner_maker);
    check(!outer_only.IsNull() && !inner_only.IsNull(), "each chain rounds alone");
    if (!outer_only.IsNull() && !inner_only.IsNull()) {
      const double expected = (before - volume_of(outer_only)) + (before - volume_of(inner_only));
      const double got = before - volume_of(rounded);
      const bool near = expected > 0.0 && std::abs(got - expected) <= 1e-3 * expected;
      check(near, "the rounding takes away what the two chains take alone");
      if (!near) {
        std::fprintf(stderr, "taken away %.9g, expected %.9g\n", got, expected);
      }
    }
  }

  // 1711, 1728: with the foot's rounding of 3. At the outer arm ends the
  // top's outer 0.5 rides on from where its contact on the wall crosses
  // the foot's down over the foot's rounding to the plate, which cuts both.
  BRepFilletAPI_MakeFillet with_foot(body);
  for (const TopoDS_Edge& e : outer_foot(body)) {
    if (with_foot.Contour(e) == 0) {
      with_foot.Add(3.0, e);
    }
  }
  const TopoDS_Shape all = round_with(both, 0.5, with_foot);
  check(!all.IsNull(), "with the foot's rounding, the outer arm ends build");
  if (all.IsNull()) {
    return;
  }
  check(BRepCheck_Analyzer(all).IsValid(), "with the foot's rounding, the result is valid");
  // (Each arm: the 0.5's end where it comes down to the foot's contact with
  // the plate, and where its contact on the top meets the plate.)
  check(has_vertex_near(all, gp_Pnt(-292.9307, 435.2831, kPlate)), "the left arm's 0.5 ends on the foot's contact");
  check(has_vertex_near(all, gp_Pnt(-270.4307, 435.2831, kPlate)), "the right arm's 0.5 ends on the foot's contact");
  check(has_vertex_near(all, gp_Pnt(-292.2492, 435.417, kPlate)), "the left arm's top contact meets the plate");
  check(has_vertex_near(all, gp_Pnt(-271.1122, 435.417, kPlate)), "the right arm's top contact meets the plate");
}

// One edge of the design's rounding: its radius, the middle of the edge
// and its length (the edges as the file's rounding lists them, set by set).
struct RoundedEdge {
  double radius;
  double x;
  double y;
  double z;
  double length;
};

// The design's whole rounding has fifteen sets. Three of them need a
// rounding that rolls on another rounding's surface where a face between
// them is narrower than both, which OCCT's fillet does not build: the
// fin's top edges with 0.5 (on the foot's rounding of 3 and the inner
// sides' roundings of 1 at the arms' ends), the tower's top outer edge with
// 1 (on its top inner edge's rounding of 2: the top is 2.127 to 3 wide),
// and the plate's edge with the tower rounded with 2 (the plate's back
// edge's rounding of 1 rolls on it: the plate is 2 wide there). The other
// twelve sets, in the file's order: the foot (3), the inner sides at the
// arms' ends (1), the tower's top inner edge (2), the slot's edge (5), the
// tower's inner foot (2), the support (5, 2, 1, 0.5, 0.5), the plate's rim
// (1) and one inner side's foot (0.8).
const RoundedEdge kTwelveSets[] = {
    {3, -289.931, 432.327, -547.000, 6.180},    {3, -289.266, 424.657, -547.000, 9.286},
    {3, -285.571, 418.855, -547.000, 4.767},    {3, -281.681, 418.247, -547.000, 3.234},
    {3, -277.788, 418.856, -547.000, 4.767},    {3, -274.554, 423.335, -547.000, 6.487},
    {3, -273.491, 427.839, -547.000, 2.799},    {3, -273.431, 432.327, -547.000, 6.180},
    {1, -287.931, 431.409, -548.500, 4.243},    {1, -287.931, 434.163, -547.000, 2.508},
    {1, -275.431, 431.409, -548.500, 4.243},    {2, -281.681, 444.909, -504.000, 10.545},
    {5, -281.681, 432.909, -547.000, 12.500},   {2, -285.554, 439.909, -547.000, 4.000},
    {2, -281.681, 444.909, -547.000, 10.545},   {2, -277.808, 439.909, -547.000, 4.000},
    {5, -286.617, 441.909, -543.000, 2.127},    {5, -276.744, 441.909, -543.000, 2.127},
    {2, -276.181, 444.309, -535.000, 3.000},    {2, -287.180, 444.309, -535.000, 3.000},
    {1, -276.777, 441.909, -535.000, 3.000},    {1, -286.584, 441.909, -535.000, 3.000},
    {0.5, -277.808, 441.909, -523.500, 39.000}, {0.5, -285.554, 441.909, -523.500, 39.000},
    {0.5, -277.808, 439.909, -545.000, 5.657},  {0.5, -275.681, 439.909, -545.000, 5.657},
    {0.5, -285.554, 439.909, -545.000, 5.657},  {0.5, -287.681, 439.909, -545.000, 5.657},
    {1, -294.181, 449.909, -547.000, 8.500},    {1, -299.845, 449.323, -547.000, 3.140},
    {1, -300.431, 432.363, -547.000, 31.092},   {1, -300.301, 415.948, -547.000, 1.764},
    {1, -267.121, 410.264, -547.000, 2.948},    {1, -264.737, 413.199, -547.000, 4.697},
    {1, -263.060, 415.948, -547.000, 1.764},    {1, -262.931, 432.363, -547.000, 31.092},
    {1, -263.516, 449.323, -547.000, 3.140},    {1, -298.624, 413.199, -547.000, 4.697},
    {1, -296.241, 410.264, -547.000, 2.948},    {1, -281.681, 409.909, -547.000, 26.289},
    {1, -269.181, 449.909, -547.000, 8.500},    {1, -281.681, 449.909, -547.000, 16.500},
    {0.8, -275.431, 434.163, -547.000, 2.508},
};

double length_of(const TopoDS_Edge& e) {
  GProp_GProps props;
  BRepGProp::LinearProperties(e, props);
  return props.Mass();
}

// The pit inside the fin's U: its edge with the plate (5), its sides'
// edges with the arms' inner walls (1) and those walls' feet (1, 0.8). Since
// 1726 they take part in the sets (before, 0700's stripes under another's
// ball dropped the pit's rounding of 5 beside the foot's rounding of 3, and
// the pit corners never met); OCCT's corner of three blends at each pit
// corner leaves the sides' and the feet's roundings crossing on the wall.
bool in_pit(const RoundedEdge& e) {
  return e.x > -288.0 && e.x < -275.4 && e.y > 429.0 && e.y < 435.0 && e.z < -546.9;
}

// The edge of the body listed by want, or a null edge.
TopoDS_Edge listed_edge(const TopoDS_Shape& body, const RoundedEdge& want) {
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(body, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    const TopoDS_Edge& e = TopoDS::Edge(edges(i));
    if (!BRep_Tool::Degenerated(e) && middle_of(e).Distance(gp_Pnt(want.x, want.y, want.z)) < 2e-3 &&
        std::abs(length_of(e) - want.length) < 2e-3) {
      return e;
    }
  }
  return TopoDS_Edge();
}

// Whether the shape has the pit's rounding of 5: a cylinder of radius 5
// about the line 5 below the plate's top behind the pit's edge.
bool has_pit_rounding(const TopoDS_Shape& shape) {
  for (TopExp_Explorer ex(shape, TopAbs_FACE); ex.More(); ex.Next()) {
    const BRepAdaptor_Surface s(TopoDS::Face(ex.Current()));
    if (s.GetType() != GeomAbs_Cylinder || std::abs(s.Cylinder().Radius() - 5.0) > 1e-6) {
      continue;
    }
    const gp_Ax1 axis = s.Cylinder().Axis();
    if (axis.Direction().IsParallel(gp_Dir(1.0, 0.0, 0.0), 1e-6) && std::abs(axis.Location().Y() - 434.980) < 1e-3 &&
        std::abs(axis.Location().Z() - (kPlate - 5.0)) < 1e-3) {
      return true;
    }
  }
  return false;
}

// The listed sets in one fillet, their contours stopped at turns as
// MitCad's fillet does. Without the pit's edges they build, valid; with
// them, the result is never done without the pit's rounding.
void test_sets(const std::string& what, const std::vector<RoundedEdge>& wanted, bool with_pit) {
  const std::string dir = cases_dir();
  if (dir.empty()) {
    std::printf("test_sets: skipped (MITCAD_GEOMETRY_CASES is not set)\n");
    return;
  }
  TopoDS_Shape body;
  BRep_Builder builder;
  if (!BRepTools::Read(body, (dir + "/fillet-wall-run-on/body.brep").c_str(), builder) || body.IsNull()) {
    std::printf("test_sets: skipped (no case fillet-wall-run-on)\n");
    return;
  }
  std::vector<TopoDS_Edge> picked;
  std::vector<double> radii;
  for (const RoundedEdge& want : wanted) {
    if (!with_pit && in_pit(want)) {
      continue;
    }
    const TopoDS_Edge found = listed_edge(body, want);
    check(!found.IsNull(), "each listed edge is on the body");
    if (!found.IsNull()) {
      picked.push_back(found);
      radii.push_back(want.radius);
    }
  }
  BRepFilletAPI_MakeFillet maker(body);
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> selected;
  for (const TopoDS_Edge& e : picked) {
    selected.Add(e);
  }
  maker.StopChainsAtTurns(selected);
  for (std::size_t i = 0; i < picked.size(); ++i) {
    if (maker.Contour(picked[i]) == 0) {
      maker.Add(radii[i], picked[i]);
    }
  }
  try {
    OCC_CATCH_SIGNALS
    maker.Build();
  } catch (const Standard_Failure& e) {
    std::fprintf(stderr, "%s throw: %s\n", what.c_str(), e.what());
  }
  if (with_pit) {
    check(!maker.IsDone() || has_pit_rounding(maker.Shape()),
          (what + " with the pit's edges: never done without the pit's rounding").c_str());
    return;
  }
  check(maker.IsDone(), (what + " build together").c_str());
  if (maker.IsDone()) {
    check(BRepCheck_Analyzer(maker.Shape()).IsValid(), (what + ": the result is valid").c_str());
  }
}

// 1726: the fin's foot rounded with 3 and the pit's edge with 5, in one
// fillet: the pit's rounding is built (before, it was left out and the
// result, the foot's rounding alone, was done and valid).
void test_pit_beside_the_foot() {
  const std::string dir = cases_dir();
  if (dir.empty()) {
    std::printf("test_pit_beside_the_foot: skipped (MITCAD_GEOMETRY_CASES is not set)\n");
    return;
  }
  TopoDS_Shape body;
  BRep_Builder builder;
  if (!BRepTools::Read(body, (dir + "/fillet-wall-run-on/body.brep").c_str(), builder) || body.IsNull()) {
    std::printf("test_pit_beside_the_foot: skipped (no case fillet-wall-run-on)\n");
    return;
  }
  const TopoDS_Edge pit = listed_edge(body, kTwelveSets[12]);
  check(!pit.IsNull(), "the pit's edge is on the body");
  const std::vector<TopoDS_Edge> foot = outer_foot(body);
  check(foot.size() == 8, "the fin's outer foot has eight edges");
  if (pit.IsNull() || foot.empty()) {
    return;
  }
  BRepFilletAPI_MakeFillet both(body);
  both.Add(5.0, pit);
  const TopoDS_Shape rounded = round_with(foot, 3.0, both);
  check(!rounded.IsNull(), "the foot's and the pit's roundings build together");
  if (rounded.IsNull()) {
    return;
  }
  check(BRepCheck_Analyzer(rounded).IsValid(), "the foot's and the pit's roundings are valid");
  check(has_pit_rounding(rounded), "the pit's rounding of 5 is in the result");
  BRepFilletAPI_MakeFillet foot_maker(body);
  const TopoDS_Shape foot_only = round_with(foot, 3.0, foot_maker);
  BRepFilletAPI_MakeFillet pit_maker(body);
  pit_maker.Add(5.0, pit);
  pit_maker.Build();
  check(!foot_only.IsNull() && pit_maker.IsDone(), "each rounds alone");
  if (!foot_only.IsNull() && pit_maker.IsDone()) {
    const double expected = volume_of(foot_only) + volume_of(pit_maker.Shape()) - volume_of(body);
    const double got = volume_of(rounded);
    const bool near = std::abs(got - expected) <= 1e-2;
    check(near, "the material is that of the two roundings made alone");
    if (!near) {
      std::fprintf(stderr, "volume %.9g, expected %.9g\n", got, expected);
    }
  }
}

// Sets 4 and 5 of the design's rounding: the tower's top inner edge with 2
// and its outer edge with 1, alone.
void test_tower_top(bool with_legs) {
  const std::string dir = cases_dir();
  if (dir.empty()) {
    std::printf("test_tower_top: skipped (MITCAD_GEOMETRY_CASES is not set)\n");
    return;
  }
  TopoDS_Shape body;
  BRep_Builder builder;
  if (!BRepTools::Read(body, (dir + "/fillet-wall-run-on/body.brep").c_str(), builder) || body.IsNull()) {
    std::printf("test_tower_top: skipped (no case fillet-wall-run-on)\n");
    return;
  }
  std::vector<RoundedEdge> wanted{{2, -281.681, 444.909, -504.000, 10.545}, {1, -281.681, 447.909, -504.000, 18.850}};
  if (with_legs) {
    wanted.push_back({0.5, -277.808, 441.909, -523.500, 39.000});
    wanted.push_back({0.5, -285.554, 441.909, -523.500, 39.000});
  }
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(body, TopAbs_EDGE, edges);
  BRepFilletAPI_MakeFillet maker(body);
  for (const RoundedEdge& want : wanted) {
    TopoDS_Edge found;
    for (int i = 1; i <= edges.Extent() && found.IsNull(); ++i) {
      const TopoDS_Edge& e = TopoDS::Edge(edges(i));
      if (!BRep_Tool::Degenerated(e) && middle_of(e).Distance(gp_Pnt(want.x, want.y, want.z)) < 2e-3 &&
          std::abs(length_of(e) - want.length) < 2e-3) {
        found = e;
      }
    }
    check(!found.IsNull(), "the tower's top edge is on the body");
    if (found.IsNull()) {
      return;
    }
    maker.Add(want.radius, found);
  }
  try {
    OCC_CATCH_SIGNALS
    maker.Build();
  } catch (const Standard_Failure& e) {
    std::fprintf(stderr, "the tower's top throws: %s\n", e.what());
  }
  check(maker.IsDone(), "the tower's top rounds with 2 and 1");
  if (!maker.IsDone()) {
    return;
  }
  check(BRepCheck_Analyzer(maker.Shape()).IsValid(), "the tower's top is valid");
  // The outer rounding's contact on the outer side at a leg's front.
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> vertices;
  TopExp::MapShapes(maker.Shape(), TopAbs_VERTEX, vertices);
  bool found = false;
  for (int i = 1; i <= vertices.Extent(); ++i) {
    const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(vertices(i)));
    if (std::abs(p.X() + 287.681) < 1e-3 && std::abs(p.Y() - 441.909) < 1e-3 && p.Z() > -507.0 && p.Z() < -504.5) {
      found = found || std::abs(p.Z() + 505.566) < 1e-3;
      if (std::abs(p.Z() + 505.566) >= 1e-3) {
        std::fprintf(stderr, "the outer contact at the leg's front at z = %.6f\n", p.Z());
      }
    }
  }
  check(found, "the outer rounding's contact at the leg's front lies where the stored body has it");
  if (with_legs) {
    // The corner of three blends at the leg's front: where the ride's line
    // meets the leg's rounding, and where that rounding's contact on the
    // front meets the outer rounding, as the stored body has them.
    const gp_Pnt stored[] = {gp_Pnt(-285.840332, 441.915837, -504.979744), gp_Pnt(-285.923309, 441.908904, -504.865480)};
    for (const gp_Pnt& want : stored) {
      bool near = false;
      for (int i = 1; i <= vertices.Extent(); ++i) {
        near = near || BRep_Tool::Pnt(TopoDS::Vertex(vertices(i))).Distance(want) < 1e-3;
      }
      check(near, "the corner of three blends has its points where the stored body has them");
    }
  }
}

// The twelve sets and set 5 (the tower's top outer edge) in one fillet;
// with set 6 (the tower's outer foot) the fourteen.
void test_thirteen_sets(bool with_set6, bool with_pit) {
  std::vector<RoundedEdge> wanted(std::begin(kTwelveSets), std::end(kTwelveSets));
  wanted.push_back({1, -281.681, 447.909, -504.000, 18.850});
  if (with_set6) {
    wanted.push_back({2, -287.681, 439.909, -547.000, 4.000});
    wanted.push_back({2, -281.681, 447.909, -547.000, 18.850});
    wanted.push_back({2, -275.681, 439.909, -547.000, 4.000});
  }
  test_sets(with_set6 ? "the fourteen sets" : "the thirteen sets", wanted, with_pit);
}

// 1727: the inner feet's roundings of 1 (with the pit's sides' edges) and
// the top's edges with 0.5, whose ends at the inner arm ends ride on over
// the pit corners' patches and the feet's roundings.
bool has_vertex_near(const TopoDS_Shape& shape, const gp_Pnt& p) {
  for (TopExp_Explorer ex(shape, TopAbs_VERTEX); ex.More(); ex.Next()) {
    if (BRep_Tool::Pnt(TopoDS::Vertex(ex.Current())).Distance(p) < 1e-3) {
      return true;
    }
  }
  return false;
}

void test_inner_arm_ends() {
  const std::string dir = cases_dir();
  if (dir.empty()) {
    std::printf("test_inner_arm_ends: skipped (MITCAD_GEOMETRY_CASES is not set)\n");
    return;
  }
  TopoDS_Shape body;
  BRep_Builder builder;
  if (!BRepTools::Read(body, (dir + "/fillet-wall-run-on/body.brep").c_str(), builder) || body.IsNull()) {
    std::printf("test_inner_arm_ends: skipped (no case fillet-wall-run-on)\n");
    return;
  }
  BRepFilletAPI_MakeFillet maker(body);
  for (const int i : {12, 8, 9, 10, 42}) {
    const TopoDS_Edge e = listed_edge(body, kTwelveSets[i]);
    check(!e.IsNull(), "an inner foot's or the pit's edge is on the body");
    if (e.IsNull()) {
      return;
    }
    maker.Add(kTwelveSets[i].radius, e);
  }
  std::vector<TopoDS_Edge> top_edges;
  const TopoDS_Face top = top_of(body);
  for (TopExp_Explorer ex(top, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge& e = TopoDS::Edge(ex.Current());
    if (!BRep_Tool::Degenerated(e) && !on_plate(e)) {
      top_edges.push_back(e);
    }
  }
  check(top_edges.size() == 15, "the top has fifteen edges off the plate");
  const TopoDS_Shape rounded = round_with(top_edges, 0.5, maker);
  check(!rounded.IsNull(), "the inner arm ends build");
  if (rounded.IsNull()) {
    return;
  }
  check(BRepCheck_Analyzer(rounded).IsValid(), "the inner arm ends are valid");
  // (Each arm: the 0.5's end where the foot's rounding meets the plate, and
  // where its contact on the top meets the plate.)
  check(has_vertex_near(rounded, gp_Pnt(-286.9307, 435.2831, kPlate)), "the left arm's 0.5 ends at the foot's end");
  check(has_vertex_near(rounded, gp_Pnt(-276.2307, 435.2831, kPlate)), "the right arm's 0.5 ends at the foot's end");
  check(has_vertex_near(rounded, gp_Pnt(-287.3739, 435.417, kPlate)), "the left arm's top contact meets the plate");
  check(has_vertex_near(rounded, gp_Pnt(-275.8187, 435.417, kPlate)), "the right arm's top contact meets the plate");
}

// 1729: the pit's edge, one inner arm's foot rounded (with its pit side's
// edge) and the other's not, and the top's edges: the arm end whose foot is
// not rounded was done but not valid (the plate's wire open). Never done
// and not valid; either arm, and the pit's edge with the top's edges alone.
void test_one_inner_arm() {
  const std::string dir = cases_dir();
  if (dir.empty()) {
    std::printf("test_one_inner_arm: skipped (MITCAD_GEOMETRY_CASES is not set)\n");
    return;
  }
  TopoDS_Shape body;
  BRep_Builder builder;
  if (!BRepTools::Read(body, (dir + "/fillet-wall-run-on/body.brep").c_str(), builder) || body.IsNull()) {
    std::printf("test_one_inner_arm: skipped (no case fillet-wall-run-on)\n");
    return;
  }
  std::vector<TopoDS_Edge> top_edges;
  const TopoDS_Face top = top_of(body);
  for (TopExp_Explorer ex(top, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge& e = TopoDS::Edge(ex.Current());
    if (!BRep_Tool::Degenerated(e) && !on_plate(e)) {
      top_edges.push_back(e);
    }
  }
  const std::vector<std::vector<int>> cases = {{12, 8, 9}, {12, 10, 42}, {12}};
  for (const std::vector<int>& sets : cases) {
    BRepFilletAPI_MakeFillet maker(body);
    for (const int i : sets) {
      const TopoDS_Edge e = listed_edge(body, kTwelveSets[i]);
      check(!e.IsNull(), "an inner foot's or the pit's edge is on the body");
      if (e.IsNull()) {
        return;
      }
      maker.Add(kTwelveSets[i].radius, e);
    }
    const TopoDS_Shape rounded = round_with(top_edges, 0.5, maker);
    check(rounded.IsNull() || BRepCheck_Analyzer(rounded).IsValid(),
          "one inner arm's foot rounded: never done and not valid");
    if (rounded.IsNull()) {
      check(maker.Failure() == ChFi3d_Builder::Failure_Corner, "one inner arm's foot rounded: fails at a corner");
    }
  }
}

// 1728: the whole of the design's Fillet1, the twelve sets with sets 5, 6
// and the top's edges (set 2), in one fillet: built, valid, its volume
// within 3 mm3 of the body stored in the file (6092.117: the tower slots'
// corners and the outer arm ends' last pieces differ a little).
// The whole of Fillet1 with its contours in the order the test lists them
// (top_first false: the foot's 3, the twelve sets, the tower top, the
// plate's 2 and the top's 0.5 last), or with the top's 0.5 right after the
// foot's 3, as MitCad's fillet gives the file's sets (top_first). Its
// volume (out), or a negative one where it fails.
double whole_fillet1(const TopoDS_Shape& body, bool top_first) {
  std::vector<RoundedEdge> wanted(std::begin(kTwelveSets), std::end(kTwelveSets));
  wanted.push_back({1, -281.681, 447.909, -504.000, 18.850});
  wanted.push_back({2, -287.681, 439.909, -547.000, 4.000});
  wanted.push_back({2, -281.681, 447.909, -547.000, 18.850});
  wanted.push_back({2, -275.681, 439.909, -547.000, 4.000});
  std::vector<TopoDS_Edge> picked;
  std::vector<double> radii;
  std::vector<TopoDS_Edge> top_edges;
  const TopoDS_Face top = top_of(body);
  for (TopExp_Explorer ex(top, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge& e = TopoDS::Edge(ex.Current());
    if (!BRep_Tool::Degenerated(e) && !on_plate(e)) {
      top_edges.push_back(e);
    }
  }
  for (std::size_t i = 0; i < wanted.size(); ++i) {
    // (The foot's 3 are the first eight of the twelve sets.)
    if (top_first && i == 8) {
      for (const TopoDS_Edge& e : top_edges) {
        picked.push_back(e);
        radii.push_back(0.5);
      }
    }
    const TopoDS_Edge found = listed_edge(body, wanted[i]);
    check(!found.IsNull(), "each edge of Fillet1 is on the body");
    if (found.IsNull()) {
      return -1.0;
    }
    picked.push_back(found);
    radii.push_back(wanted[i].radius);
  }
  if (!top_first) {
    for (const TopoDS_Edge& e : top_edges) {
      picked.push_back(e);
      radii.push_back(0.5);
    }
  }
  BRepFilletAPI_MakeFillet maker(body);
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> selected;
  for (const TopoDS_Edge& e : picked) {
    selected.Add(e);
  }
  maker.StopChainsAtTurns(selected);
  for (std::size_t i = 0; i < picked.size(); ++i) {
    if (maker.Contour(picked[i]) == 0) {
      maker.Add(radii[i], picked[i]);
    }
  }
  try {
    OCC_CATCH_SIGNALS
    maker.Build();
  } catch (const Standard_Failure& e) {
    std::fprintf(stderr, "Fillet1 throws: %s\n", e.what());
  }
  check(maker.IsDone(), "the whole of Fillet1 builds");
  if (!maker.IsDone()) {
    return -1.0;
  }
  check(BRepCheck_Analyzer(maker.Shape()).IsValid(), "the whole of Fillet1 is valid");
  // Its vertices tight: the corner patches beside the pit, where the
  // wall's foot rounded with 1 or 0.8 meets the plate's rounding of 5,
  // had vertices half a millimetre off with the top's 0.5 given first
  // (port patch 1781, mitcad#1987). (The legs' fronts keep a few
  // hundredths, the corners of their foot's 2 and top's 0.5.)
  double widest = 0.0;
  for (TopExp_Explorer ex(maker.Shape(), TopAbs_VERTEX); ex.More(); ex.Next()) {
    widest = std::max(widest, BRep_Tool::Tolerance(TopoDS::Vertex(ex.Current())));
  }
  check(widest < 0.05, "Fillet1's vertices are within 0.05 of their edges");
  if (widest >= 0.05) {
    std::fprintf(stderr, "Fillet1's widest vertex tolerance %.4f\n", widest);
  }
  GProp_GProps props;
  BRepGProp::VolumeProperties(maker.Shape(), props);
  const bool near = std::abs(props.Mass() - 6092.117) < 3.0;
  check(near, "the whole of Fillet1 holds the stored body's material within 3 mm3");
  if (!near) {
    std::fprintf(stderr, "Fillet1's volume %.6f, the stored body's 6092.117\n", props.Mass());
  }
  return props.Mass();
}

void test_whole_fillet1() {
  const std::string dir = cases_dir();
  if (dir.empty()) {
    std::printf("test_whole_fillet1: skipped (MITCAD_GEOMETRY_CASES is not set)\n");
    return;
  }
  TopoDS_Shape body;
  BRep_Builder builder;
  if (!BRepTools::Read(body, (dir + "/fillet-wall-run-on/body.brep").c_str(), builder) || body.IsNull()) {
    std::printf("test_whole_fillet1: skipped (no case fillet-wall-run-on)\n");
    return;
  }
  const double listed = whole_fillet1(body, false);
  const double top_first = whole_fillet1(body, true);
  // The same body in either order (the corners of three stripes before
  // those of two, 1781).
  check(listed > 0 && std::abs(top_first - listed) < 1e-6 * listed, "Fillet1 does not depend on its contours' order");
  if (std::abs(top_first - listed) >= 1e-6 * listed) {
    std::fprintf(stderr, "Fillet1's volume %.6f in the listed order, %.6f with the top first\n", listed, top_first);
  }
}

} // namespace

int main() {
  try {
    test_reported_body();
    const std::vector<RoundedEdge> twelve(std::begin(kTwelveSets), std::end(kTwelveSets));
    test_sets("the twelve sets", twelve, false);
    test_sets("the twelve sets", twelve, true);
    test_tower_top(false);
    test_tower_top(true);
    test_thirteen_sets(false, false);
    test_thirteen_sets(true, false);
    test_thirteen_sets(true, true);
    test_pit_beside_the_foot();
    test_inner_arm_ends();
    test_one_inner_arm();
    test_whole_fillet1();
  } catch (const Standard_Failure& e) {
    std::fprintf(stderr, "unexpected OCCT error: %s\n", e.what());
    ++failures;
  }
  if (failures > 0) {
    std::fprintf(stderr, "%d check(s) failed\n", failures);
    return 1;
  }
  std::printf("test_fillet_top_chains: all checks passed\n");
  return 0;
}
