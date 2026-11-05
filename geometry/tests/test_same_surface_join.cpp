// SPDX-License-Identifier: MIT
// Faces on one surface joined after a cut (mitcad#1261): a disc whose rim
// goes on above it as the wall of a cut made of eight sketch regions, the
// regions' curves on that wall eight arcs of one circle split every 45
// degrees, their centres and radii a few 1e-7 mm apart as a solved sketch
// can leave them (OCCT's merge after the boolean, exact without a fuzzy
// value, joins only pieces within about 3e-8 mm of each other). The wall
// is one cylinder: one face, holding the names of all its pieces, without
// the seams between them; a rounding of its edge named by any piece builds,
// and again when the whole is built anew.

#include <cmath>
#include <cstdio>
#include <string>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepGProp.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Cylinder.hxx>

#include "check.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "mitcad/geometry/dressup.hpp"
#include "mitcad/geometry/extrude.hpp"
#include "mitcad/geometry/naming.hpp"
#include "mitcad/geometry/profile.hpp"

namespace {

using namespace mitcad::geometry;
using test::kPi;

// The rim's radius in the reported design, and the cut's arcs' (44, the
// arcs within 3.4e-7 of it).
constexpr double kRim = 43.999999999661746;
constexpr double kArc = 44.0;
constexpr double kPlate = 60.0;
constexpr double kOuter = 70.0;
constexpr double kBottom = 200.0;
constexpr double kStep = 202.0;
constexpr double kTop = 205.13;

double volume(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

Segment arc(const std::string& name, double cx, double cy, double radius, double from, double to) {
  Segment s;
  s.name = name;
  s.kind = SegmentKind::Arc;
  s.center = gp_Pnt2d(cx, cy);
  s.radius = radius;
  s.start_angle = from;
  s.end_angle = to;
  return s;
}

Segment line(const std::string& name, const gp_Pnt2d& a, const gp_Pnt2d& b) {
  Segment s;
  s.name = name;
  s.kind = SegmentKind::Line;
  s.start = a;
  s.end = b;
  return s;
}

Region disc(int curve, double radius) {
  Region region = circle_region(0.0, 0.0, radius);
  region.loops[0].segments[0].name = "c" + std::to_string(curve);
  region.name = "r{c" + std::to_string(curve) + "}";
  return region;
}

// The inner arc's curve of sector k: c1 for the first, then c71, c72, ...
// as a sketch's curve numbers run.
std::string arc_curve(int k) { return k == 0 ? "c1" : "c" + std::to_string(70 + k); }

// The name of sector k's wall on the circle.
std::string wall_name(int k) {
  const std::string ray0 = "c" + std::to_string(90 + k);
  const std::string ray1 = "c" + std::to_string(90 + (k + 1) % 8);
  return face_name("F63", "side", arc_curve(k) + "[" + ray0 + "," + ray1 + "]");
}

// Eight ring sectors from R44 out to kOuter, each 45 degrees, sharing their
// rays; their inner arcs' centres and radii off by up to `scale` times
// 3.4e-10 (alternately exact), their ends within that of the rays'; the
// first from `turn` (radians) on.
std::vector<Region> sectors(double scale, double turn = 0.0) {
  std::vector<Region> regions;
  const double noise[8][3] = {{0.0, 0.0, 0.0},      {2.1e-11, -1.3e-11, -3.38e-10}, {0.0, 0.0, 0.0},
                              {-1.7e-11, 0.9e-11, 2.4e-10}, {0.0, 0.0, 0.0},      {1.1e-11, 2.2e-11, -3.38e-10},
                              {0.0, 0.0, 0.0},      {-2.5e-11, -0.6e-11, 1.9e-10}};
  for (int k = 0; k < 8; ++k) {
    const double a0 = k * kPi / 4.0 + turn;
    const double a1 = (k + 1) * kPi / 4.0 + turn;
    const auto at = static_cast<std::size_t>(k);
    const double cx = scale * noise[at][0];
    const double cy = scale * noise[at][1];
    const double r = kArc + scale * noise[at][2];
    const std::string inner = arc_curve(k);
    const std::string outer = "c" + std::to_string(80 + k);
    const std::string ray0 = "c" + std::to_string(90 + k);
    const std::string ray1 = "c" + std::to_string(90 + (k + 1) % 8);
    Region region;
    region.name = "r{" + inner + "," + ray1 + "," + outer + "," + ray0 + "}";
    Loop loop;
    loop.segments.push_back(arc(inner + "[" + ray0 + "," + ray1 + "]", cx, cy, r, a0, a1));
    loop.segments.push_back(line(ray1 + "[" + inner + "," + outer + "]",
                                 gp_Pnt2d(kArc * std::cos(a1), kArc * std::sin(a1)),
                                 gp_Pnt2d(kOuter * std::cos(a1), kOuter * std::sin(a1))));
    loop.segments.push_back(arc(outer + "[" + ray1 + "," + ray0 + "]", 0.0, 0.0, kOuter, a0, a1));
    loop.segments.push_back(line(ray0 + "[" + outer + "," + inner + "]",
                                 gp_Pnt2d(kOuter * std::cos(a0), kOuter * std::sin(a0)),
                                 gp_Pnt2d(kArc * std::cos(a0), kArc * std::sin(a0))));
    region.loops.push_back(loop);
    regions.push_back(region);
  }
  return regions;
}

// The regions extruded from `from` to `to` (down where `to` lies below:
// the side faces' axes then point down).
ShapePtr prism(const std::string& feature, const std::vector<Region>& regions, double from, double to) {
  ExtrudeFeatureSpec spec;
  spec.feature = feature;
  spec.regions = regions;
  spec.frame.origin = gp_Pnt(0.0, 0.0, from);
  spec.side1.distance = std::abs(to - from);
  if (to < from) {
    spec.direction = gp_Dir(0.0, 0.0, -1.0);
  }
  return extrude_feature(spec);
}

ShapePtr one_piece(const BooleanResult& result) {
  CHECK(result.pieces.size() == 1);
  return result.pieces.size() == 1 ? result.pieces.front().shape : nullptr;
}

// The faces of the body on the cylinder of radius 44 about the z axis.
std::vector<int> wall_faces(const Shape& body) {
  std::vector<int> faces;
  for (int i = 0; i < body.face_count(); ++i) {
    const BRepAdaptor_Surface surface(body.face(i));
    if (surface.GetType() == GeomAbs_Cylinder && std::abs(surface.Cylinder().Radius() - kArc) < 1.0e-6) {
      faces.push_back(i);
    }
  }
  return faces;
}

// The edges of a wall face (from `low` to the top) that are neither on its
// bottom or top rim nor its seam: edges between two of its pieces, or a
// seam at a second angle with a piece of rim between the two.
int seams_between_pieces(const TopoDS_Face& face, double low) {
  int count = 0;
  std::vector<double> seam_angles;
  for (TopExp_Explorer x(face, TopAbs_EDGE); x.More(); x.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(x.Current());
    TopoDS_Vertex v0, v1;
    TopExp::Vertices(edge, v0, v1);
    const gp_Pnt p0 = BRep_Tool::Pnt(v0);
    const gp_Pnt p1 = BRep_Tool::Pnt(v1);
    if (BRep_Tool::IsClosed(edge, face)) {
      seam_angles.push_back(std::atan2(p0.Y(), p0.X()));
      continue;
    }
    const auto at_rim = [low](const gp_Pnt& p) {
      return std::abs(p.Z() - low) < 1.0e-6 || std::abs(p.Z() - kTop) < 1.0e-6;
    };
    if (std::abs(p0.Z() - p1.Z()) > 1.0e-6 || !at_rim(p0)) {
      ++count;
    }
  }
  for (const double angle : seam_angles) {
    count += std::abs(angle - seam_angles.front()) > 1.0e-6 ? 1 : 0;
  }
  return count;
}

void check_heights(const TopoDS_Face& face, double low, double high) {
  Bnd_Box box;
  BRepBndLib::AddOptimal(face, box, false, false);
  double xmin = 0, ymin = 0, zmin = 0, xmax = 0, ymax = 0, zmax = 0;
  box.Get(xmin, ymin, zmin, xmax, ymax, zmax);
  CHECK(std::abs(zmin - low) < 1.0e-6);
  CHECK(std::abs(zmax - high) < 1.0e-6);
}

// One wall face: from `low` to the top, no seam between pieces, every
// piece's name finding it.
void check_one_wall(const Shape& body, double low, const std::vector<std::string>& names) {
  const std::vector<int> wall = wall_faces(body);
  CHECK(wall.size() == 1);
  if (wall.size() != 1) {
    std::fprintf(stderr, "  %d faces on the wall:", static_cast<int>(wall.size()));
    for (const int f : wall) {
      std::fprintf(stderr, " %s", body.face_names(f).empty() ? "?" : body.face_names(f).front().c_str());
    }
    std::fprintf(stderr, "\n");
    return;
  }
  const TopoDS_Face& face = body.face(wall.front());
  CHECK(seams_between_pieces(face, low) == 0);
  check_heights(face, low, kTop);
  for (const std::string& name : names) {
    const std::vector<int> found = body.find_faces(name);
    CHECK(found == wall);
    if (found != wall) {
      std::fprintf(stderr, "  %s finds %d faces\n", name.c_str(), static_cast<int>(found.size()));
    }
  }
}

std::vector<std::string> all_wall_names() {
  std::vector<std::string> names;
  for (int k = 0; k < 8; ++k) {
    names.push_back(wall_name(k));
  }
  return names;
}

// The eight sectors cut from a disc of radius 60 (z 200-205.13) from
// z 202 up: a boss of radius 44 stands on the disc, its wall the cut's
// eight pieces.
ShapePtr boss(double scale) {
  const ShapePtr body = prism("F35", {disc(1, kPlate)}, kBottom, kTop);
  const ShapePtr tool = prism("F63", sectors(scale), kStep, kTop + 1.0);
  return one_piece(boolean(BooleanOp::Cut, {body.get()}, *tool));
}

// A disc of the rim's radius (z 200-202) under a disc of radius 60 (z 202-
// 205.13), joined, then the sectors cut from z 202 up: the rim goes on
// above as the cut's wall, one cylinder from z 200 to the top. `down`: the
// rim extruded down from z 202, its cylinder's axis the other way round.
ShapePtr rim_and_wall(double scale, bool down, double turn = 0.0) {
  const ShapePtr rim =
      down ? prism("F35", {disc(1, kRim)}, kStep, kBottom) : prism("F35", {disc(1, kRim)}, kBottom, kStep);
  const ShapePtr plate = prism("F40", {disc(1, kPlate)}, kStep, kTop);
  const ShapePtr body = one_piece(boolean(BooleanOp::Join, {rim.get()}, *plate));
  if (!body) {
    return nullptr;
  }
  const ShapePtr tool = prism("F63", sectors(scale, turn), kStep, kTop + 1.0);
  return one_piece(boolean(BooleanOp::Cut, {body.get()}, *tool));
}

// The noise of the reported design and a thousand times as much, which
// OCCT's own merge leaves in pieces.
constexpr double kScales[] = {1.0, 1000.0};

void test_cut_walls_of_eight_arcs_are_one_face() {
  for (const double scale : kScales) {
    const ShapePtr cut = boss(scale);
    if (!cut) {
      continue;
    }
    CHECK(BRepCheck_Analyzer(cut->occt()).IsValid());
    const double expected = kPi * kPlate * kPlate * (kStep - kBottom) + kPi * kArc * kArc * (kTop - kStep);
    CHECK_NEAR_TOL(volume(cut->occt()), expected, 1.0e-7);
    // The disc's side, bottom and top ring, the boss's wall and top.
    CHECK(cut->face_count() == 5);
    check_one_wall(*cut, kStep, all_wall_names());
  }
}

void rim_wall_case(double scale, bool down, double turn) {
  const ShapePtr cut = rim_and_wall(scale, down, turn);
  if (!cut) {
    return;
  }
  CHECK(BRepCheck_Analyzer(cut->occt()).IsValid());
  CHECK_NEAR_TOL(volume(cut->occt()), kPi * kArc * kArc * (kTop - kBottom), 1.0e-7);
  // The wall, the rim's bottom and the plate's top.
  CHECK(cut->face_count() == 3);
  std::vector<std::string> names = all_wall_names();
  names.push_back(face_name("F35", "side", "c1"));
  check_one_wall(*cut, kBottom, names);
}

// The rim's seam where a sector starts, and 0.17 rad (about 10 degrees)
// before the sectors: the joined face has one seam, no piece of the rim
// between two seams left as an edge.
void test_rim_continued_by_the_cut_is_one_face() {
  for (const bool down : {false, true}) {
    for (const double turn : {0.0, 0.17}) {
      for (const double scale : kScales) {
        rim_wall_case(scale, down, turn);
      }
    }
  }
}

// Pieces 3.4e-6 apart are not one surface (more than 1e-6 mm): they stay
// apart, the body as valid as the cut made it.
void test_pieces_farther_apart_stay_apart() {
  const ShapePtr cut = boss(10000.0);
  if (!cut) {
    return;
  }
  CHECK(BRepCheck_Analyzer(cut->occt()).IsValid());
  CHECK(wall_faces(*cut).size() > 1);
}

// The wall's top edge rounded, named by a piece of the cut (c71's) and by
// the rim: each finds the whole edge, the rounding builds, and building the
// whole again finds the same edge and gives the same body.
void a_rounding_of_the_joined_wall(bool down, double turn) {
  const std::string top = face_name("F40", "end", "r{c1}");
  const std::string bottom = face_name("F35", down ? "end" : "start", "r{c1}");
  const double expected = kPi * kArc * kArc * (kTop - kBottom);
  int faces = -1;
  for (int build = 0; build < 2; ++build) {
    const ShapePtr cut = rim_and_wall(1000.0, down, turn);
    if (!cut) {
      return;
    }
    for (const auto& [face, other] : {std::make_pair(wall_name(1), top), std::make_pair(wall_name(0), bottom),
                                      std::make_pair(face_name("F35", "side", "c1"), top)}) {
      const std::string reference = make_edge_name(face, other);
      CHECK(cut->find_edges(reference).size() == 1);
    }
    const std::string reference = make_edge_name(wall_name(1), top);
    const std::vector<int> edges = cut->find_edges(reference);
    if (edges.size() != 1) {
      continue;
    }
    const ShapePtr rounded = fillet("F70", *cut, {reference}, 1.0);
    CHECK(BRepCheck_Analyzer(rounded->occt()).IsValid());
    // The section a rounding of 1 takes off, 1 - pi/4, swept round the
    // axis at its centroid's radius: (10 - 3 pi) / (12 - 3 pi) inside the
    // edge.
    const double loss = test::fillet_loss(1.0) * 2.0 * kPi * (kArc - (10.0 - 3.0 * kPi) / (12.0 - 3.0 * kPi));
    CHECK_NEAR_TOL(volume(rounded->occt()), expected - loss, 1.0e-6);
    CHECK(rounded->find_faces(face_name("F70", "fillet", cut->edge_name(edges.front()))).size() == 1);
    if (faces < 0) {
      faces = rounded->face_count();
    } else {
      CHECK(rounded->face_count() == faces);
    }
  }
}

void test_a_rounding_of_the_joined_wall_finds_its_edge() {
  for (const bool down : {false, true}) {
    for (const double turn : {0.0, 0.17}) {
      a_rounding_of_the_joined_wall(down, turn);
    }
  }
}

} // namespace

int main() {
  test::guarded("test_cut_walls_of_eight_arcs_are_one_face", test_cut_walls_of_eight_arcs_are_one_face);
  test::guarded("test_rim_continued_by_the_cut_is_one_face", test_rim_continued_by_the_cut_is_one_face);
  test::guarded("test_pieces_farther_apart_stay_apart", test_pieces_farther_apart_stay_apart);
  test::guarded("test_a_rounding_of_the_joined_wall_finds_its_edge", test_a_rounding_of_the_joined_wall_finds_its_edge);
  return test::failures == 0 ? 0 : 1;
}
