// SPDX-License-Identifier: MIT
// Port patch 1280: the boolean's stick vertices of a pair of intersected
// faces (BOPAlgo_PaveFiller::GetStickVertices: the vertices of the VV, VE,
// EE, VF and EF interferences between the two faces' sub-shapes) are found
// from an index of the interferences by the shapes they join, in MakeBlocks
// and PostTreatFF, instead of going through every interference of the
// operation for every pair of faces: quadratic in the interferences of a
// union of many bodies. The results are the ones of stock OCCT: a row of
// touching boxes, each pierced by a cylinder, is one valid solid with the
// volume of its parts; pyramids whose apexes meet at one point keep one
// vertex there; and the intersections of a union of 1922 bodies with 600 000
// pairs of faces and 37 440 interferences take a few seconds.

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstddef>
#include <vector>

#include <BOPAlgo_PaveFiller.hxx>
#include <BOPDS_DS.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeWedge.hxx>
#include <NCollection_List.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Ax2.hxx>
#include <gp_Dir.hxx>
#include <gp_Vec.hxx>

#include "port_test.hpp"

namespace {

struct Union {
  TopoDS_Shape shape;
  bool         done           = false;
  double       filler_seconds = 0.0;
  int          face_pairs     = 0;
  int          vertex_pairs   = 0;
};

// The union of the bodies: their intersections (OCCT's pave filler, where
// the stick vertices are found) timed on their own, then the fuse built
// from them.
Union united(const std::vector<TopoDS_Shape>& bodies) {
  Union                          result;
  NCollection_List<TopoDS_Shape> all, first, rest;
  for (const TopoDS_Shape& body : bodies) {
    all.Append(body);
    (first.IsEmpty() ? first : rest).Append(body);
  }
  BOPAlgo_PaveFiller filler;
  filler.SetArguments(all);
  filler.SetRunParallel(true);
  const auto start = std::chrono::steady_clock::now();
  filler.Perform();
  result.filler_seconds = std::chrono::duration<double>(std::chrono::steady_clock::now() - start).count();
  if (filler.HasErrors()) {
    std::fprintf(stderr, "union: the intersections failed\n");
    return result;
  }
  result.face_pairs   = filler.PDS()->InterfFF().Length();
  result.vertex_pairs = filler.PDS()->InterfVV().Length();
  BRepAlgoAPI_Fuse fuse(filler);
  fuse.SetArguments(first);
  fuse.SetTools(rest);
  fuse.SetRunParallel(true);
  fuse.Build();
  result.done  = fuse.IsDone() && !fuse.HasErrors();
  result.shape = fuse.Shape();
  return result;
}

// A unit box at x = i, each pierced along z by a cylinder of radius 0.2 and
// length 2 standing out 0.5 above and below it; the boxes touch face to face.
std::vector<TopoDS_Shape> pierced_boxes(int n) {
  std::vector<TopoDS_Shape> bodies;
  for (int i = 0; i < n; ++i) {
    bodies.push_back(BRepPrimAPI_MakeBox(gp_Pnt(i, 0, 0), 1.0, 1.0, 1.0).Shape());
    bodies.push_back(BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(i + 0.5, 0.5, -0.5), gp::DZ()), 0.2, 2.0).Shape());
  }
  return bodies;
}

constexpr double kSpikeWidth  = 1.0;
constexpr double kSpikeHeight = 10.0;

// A square pyramid (a wedge whose top is a point) of base 1 x 1 and height
// 10, its apex at `apex` and its base along `direction` from it.
TopoDS_Shape spike(const gp_Pnt& apex, const gp_Dir& direction) {
  // The wedge's y axis runs from its base to its apex, at (w / 2, h, w / 2).
  const gp_Dir y      = direction.Reversed();
  const gp_Dir helper = std::abs(direction.X()) < 0.9 ? gp_Dir(1, 0, 0) : gp_Dir(0, 1, 0);
  const gp_Dir x      = helper.Crossed(direction);
  const gp_Dir z      = x.Crossed(y);
  const gp_Pnt corner = apex.Translated(gp_Vec(direction) * kSpikeHeight - gp_Vec(x) * (kSpikeWidth / 2)
                                        - gp_Vec(z) * (kSpikeWidth / 2));
  const double middle = kSpikeWidth / 2;
  return BRepPrimAPI_MakeWedge(gp_Ax2(corner, z, x), kSpikeWidth, kSpikeHeight, kSpikeWidth, middle, middle, middle,
                               middle)
      .Shape();
}

double spike_volume() { return kSpikeWidth * kSpikeWidth * kSpikeHeight / 3.0; }

// k directions spread evenly over the sphere (a Fibonacci lattice).
std::vector<gp_Dir> directions(int k) {
  std::vector<gp_Dir> spread;
  const double        golden = test::kPi * (3.0 - std::sqrt(5.0));
  for (int i = 0; i < k; ++i) {
    const double z = 1.0 - (2.0 * i + 1.0) / k;
    const double r = std::sqrt(1.0 - z * z);
    spread.emplace_back(r * std::cos(golden * i), r * std::sin(golden * i), z);
  }
  return spread;
}

// The smallest angle between two of the directions.
double smallest_angle(const std::vector<gp_Dir>& spread) {
  double smallest = test::kPi;
  for (std::size_t i = 0; i < spread.size(); ++i) {
    for (std::size_t j = i + 1; j < spread.size(); ++j) {
      smallest = std::min(smallest, spread[i].Angle(spread[j]));
    }
  }
  return smallest;
}

// A star: k pyramids whose apexes meet at the centre and touch only there
// (each pair of apexes a VV interference with a new vertex, each pair of
// their side faces a pair of faces that meet only at that vertex).
void add_star(std::vector<TopoDS_Shape>& bodies, const gp_Pnt& centre, int k) {
  for (const gp_Dir& direction : directions(k)) {
    bodies.push_back(spike(centre, direction));
  }
}

// Two unit boxes overlapping by half their width in x, y and z: section
// edges of their own, so that the pave filler post-treats the pairs of faces
// (volume 1.875).
void add_overlapping_boxes(std::vector<TopoDS_Shape>& bodies, const gp_Pnt& corner) {
  bodies.push_back(BRepPrimAPI_MakeBox(corner, 1.0, 1.0, 1.0).Shape());
  bodies.push_back(BRepPrimAPI_MakeBox(corner.Translated(gp_Vec(0.5, 0.5, 0.5)), 1.0, 1.0, 1.0).Shape());
}

// The vertices of the shape at the point, and the edges that end at the
// first of them.
struct AtPoint {
  int vertices = 0;
  int edges    = 0;
};

AtPoint at_point(const TopoDS_Shape& shape, const gp_Pnt& point) {
  AtPoint       found;
  TopoDS_Vertex there;
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> vertices, edges;
  TopExp::MapShapes(shape, TopAbs_VERTEX, vertices);
  for (int i = 1; i <= vertices.Extent(); ++i) {
    const TopoDS_Vertex& vertex = TopoDS::Vertex(vertices(i));
    if (BRep_Tool::Pnt(vertex).Distance(point) < 1e-6) {
      ++found.vertices;
      there = vertex;
    }
  }
  if (there.IsNull()) {
    return found;
  }
  TopExp::MapShapes(shape, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    TopoDS_Vertex first, last;
    TopExp::Vertices(TopoDS::Edge(edges(i)), first, last);
    if (first.IsSame(there) || last.IsSame(there)) {
      ++found.edges;
    }
  }
  return found;
}

// A row of 12 touching boxes, each pierced by a cylinder (the stick
// vertices of the cylinders' seams on the boxes' tops and bottoms and of the
// boxes' common vertices): one valid solid with the volume of its parts.
void test_row_of_pierced_boxes_is_one_solid() {
  const int   n      = 12;
  const Union joined = united(pierced_boxes(n));
  CHECK(joined.done);
  CHECK(port::valid(joined.shape));
  CHECK(port::count_of(joined.shape, TopAbs_SOLID) == 1);
  CHECK_NEAR(port::volume_of(joined.shape), n * (1.0 + test::kPi * 0.2 * 0.2 * 1.0));
}

// One star of 8 pyramids and two overlapping boxes: the apexes are one
// vertex of the union, at which all 32 edges of the apexes end.
void test_apexes_of_a_star_are_one_vertex() {
  std::vector<TopoDS_Shape> bodies;
  const gp_Pnt              centre(0, 0, 0);
  add_star(bodies, centre, 8);
  add_overlapping_boxes(bodies, gp_Pnt(-40, 0, 0));
  CHECK(port::valid(bodies.front()));
  CHECK_NEAR(port::volume_of(bodies.front()), spike_volume());
  const Union joined = united(bodies);
  CHECK(joined.done);
  CHECK(joined.vertex_pairs == 8 * 7 / 2);
  CHECK(port::valid(joined.shape));
  CHECK(port::count_of(joined.shape, TopAbs_SOLID) == 8 + 1);
  CHECK_NEAR(port::volume_of(joined.shape), 8 * spike_volume() + 1.875);
  const AtPoint apex = at_point(joined.shape, centre);
  CHECK(apex.vertices == 1);
  CHECK(apex.edges == 8 * 4);
}

// The bound: 48 stars of 40 pyramids (each pair of a star's apexes a VV
// interference, 37 440 in all; each pair of their side faces a pair of
// faces, about 600 000) and two overlapping boxes. Stock OCCT went through
// all 37 440 interferences for each of the 600 000 pairs in the post-
// treatment (about 2.2e10 steps); with the index the intersections take a
// few seconds (their edge pairs computed in parallel). Measured on one
// machine (mitcad#1986): 49 s without the patch, 7.5 s with it; the bound,
// 25 s, leaves room for a loaded machine and stays well short of stock.
void test_union_of_forty_eight_stars_within_twenty_five_seconds() {
  const int                 stars = 48;
  const int                 k     = 40;
  const std::vector<gp_Dir> spread = directions(k);
  // The pyramids touch only at their apexes: the widest angle from a
  // pyramid's axis to its edges is atan(0.707 / 10) = 0.07.
  CHECK(smallest_angle(spread) > 0.3);
  std::vector<TopoDS_Shape> bodies;
  for (int m = 0; m < stars; ++m) {
    add_star(bodies, gp_Pnt(40.0 * m, 0, 0), k);
  }
  add_overlapping_boxes(bodies, gp_Pnt(-40, 0, 0));
  const Union joined = united(bodies);
  std::printf("intersections of %zu bodies (%d pairs of faces, %d VV interferences): %.2f s\n", bodies.size(),
              joined.face_pairs, joined.vertex_pairs, joined.filler_seconds);
  CHECK(joined.done);
  CHECK(joined.vertex_pairs >= stars * k * (k - 1) / 2);
  CHECK(joined.face_pairs >= stars * 16 * k * (k - 1) / 2);
  CHECK(port::valid(joined.shape));
  CHECK(port::count_of(joined.shape, TopAbs_SOLID) == stars * k + 1);
  CHECK_NEAR(port::volume_of(joined.shape), stars * k * spike_volume() + 1.875);
  CHECK(at_point(joined.shape, gp_Pnt(40.0 * (stars - 1), 0, 0)).vertices == 1);
  CHECK(joined.filler_seconds < 25.0);
}

} // namespace

MITCAD_PORT_TEST("1280", test_row_of_pierced_boxes_is_one_solid);
MITCAD_PORT_TEST("1280", test_apexes_of_a_star_are_one_vertex);
MITCAD_PORT_TEST("1280", test_union_of_forty_eight_stars_within_twenty_five_seconds);
