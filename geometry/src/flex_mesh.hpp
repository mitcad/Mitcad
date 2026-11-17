// SPDX-License-Identifier: MIT
#pragma once

// The obstacles of a Flex route as triangles (mitcad#1713): what the router
// keeps of an obstacle set between its routes. The solids are meshed on
// copies (they are left as they are), the triangles of each body joined at
// their corners, and over them: the nearest triangle to a point, the
// shortest path along the surfaces between two points on them, and a path
// through the air round the bodies. Internal to the geometry library;
// flex_route.hpp is the public side. Lengths are millimetres.

#include <array>
#include <limits>
#include <unordered_map>
#include <vector>

#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>
#include <gp_XYZ.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry::detail {

// An element of a vector by an int index, and a vector's size as an int.
template <class V>
auto& el(V& v, int i) {
  return v[static_cast<std::size_t>(i)];
}

template <class V>
int isize(const V& v) {
  return static_cast<int>(v.size());
}

inline constexpr double kFlexFar = std::numeric_limits<double>::infinity();

// Whether MITCAD_FLEX_TRACE asks for the times of a router's and a route's
// phases on standard error (mitcad#1713).
bool flex_tracing();

// A triangle of an obstacle: its corners and edges (indices of the mesh),
// the obstacle and the face of it (the index in TopExp order) it comes
// from, whether that face is curved (its triangles then lie up to the
// deflection off it), and its unit normal, out of the material.
struct FlexTriangle {
  std::array<int, 3> v{};
  std::array<int, 3> e{};
  int body = -1;
  int face = -1;
  bool curved = false;
  gp_XYZ normal;
};

// The point of the mesh nearest to a point: its triangle (-1: none), the
// distance, and whether the point lies on the material's side of that
// triangle.
struct FlexNearest {
  int triangle = -1;
  gp_XYZ point;
  double distance = kFlexFar;
  bool inside = false;
};

// A point of a path along the surfaces, with the normals of the triangles
// the path arrives and leaves in (the same inside a triangle; different
// where it crosses an edge) and whether either is of a curved face.
struct FlexSurfacePoint {
  gp_XYZ point;
  gp_XYZ normal_in;
  gp_XYZ normal_out;
  bool curved = false;
};

class FlexMesh {
public:
  // Meshes copies of the obstacles with the linear deflection (mm) and the
  // angular one (radians).
  FlexMesh(const std::vector<TopoDS_Shape>& obstacles, double deflection, double angle);

  bool empty() const { return m_tris.empty(); }
  double deflection() const { return m_deflection; }
  int body_count() const { return isize(m_obstacles); }
  const TopoDS_Shape& obstacle(int body) const { return el(m_obstacles, body); }
  const FlexTriangle& triangle(int i) const { return el(m_tris, i); }

  // The index of a face of an obstacle, -1 when it is not one of its faces.
  int face_index(int body, const TopoDS_Face& face) const;

  // The nearest point of all the obstacles within `within`.
  FlexNearest nearest(const gp_XYZ& p, double within = kFlexFar) const;
  // The nearest point of one obstacle, of one of its faces unless `face`
  // is -1.
  FlexNearest nearest_on(const gp_XYZ& p, int body, int face) const;

  // The surface's normal at a point of a triangle, turning evenly over the
  // triangles of a curved face (each corner's normal the mean of the
  // triangles round it that lie within 30 degrees of this one), so that it
  // does not jump where two triangles of one face meet.
  gp_XYZ smooth_normal(int triangle, const gp_XYZ& point) const;

  // Whether a path along the surfaces joins the two triangles: they are of
  // one body, or of bodies that touch.
  bool connected(int a, int b) const;

  // The shortest path along the surfaces from `a` in triangle `ta` to `b`
  // in triangle `tb`: over the triangles' corners and points on their
  // edges, then straightened across the edges it crosses. It does not run
  // under another body (where two bodies lie against each other). Empty
  // when there is none.
  // `heading_a`, `heading_b` (null: none): the way the route runs at
  // `a` and at `b`; a path that turns from it in the surface there counts
  // as longer by `turn_cost` for each radian, so that a way with less of
  // a turn is taken even when it is longer (an item that bends badly over
  // its edge).
  std::vector<FlexSurfacePoint> surface_path(const gp_XYZ& a, int ta, const gp_XYZ& b, int tb,
                                             const gp_XYZ* heading_a = nullptr, const gp_XYZ* heading_b = nullptr,
                                             double turn_cost = 0.0) const;

  // Whether the segment stays `need` away from every obstacle (within 2 %)
  // and outside them.
  bool segment_clear(const gp_XYZ& a, const gp_XYZ& b, double need) const;

  // A path through the air from `a` to `b` that stays `need` away from the
  // obstacles: the segment when it is clear, else the cells of a coarse
  // grid round the two points, shortened to the corners that are needed.
  // `clean` is false when it had to pass nearer than `need` or through a
  // body: the best found is still returned.
  std::vector<gp_XYZ> free_path(const gp_XYZ& a, const gp_XYZ& b, double need, bool& clean) const;

private:
  struct Edge {
    int a = 0;
    int b = 0;
    int first = 0; // the first of its points among the graph's nodes
    int count = 0;
  };
  struct Box {
    gp_XYZ lo;
    gp_XYZ hi;
  };
  struct BvhNode {
    Box box;
    int left = -1;
    int right = -1;
    int first = 0;
    int count = 0; // above 0: a leaf of triangles m_order[first ...]
  };
  struct Filter {
    int body = -1;     // only this body
    int face = -1;     // only this face of it
    int not_body = -1; // every body but this
  };

  void add_body(int body, const TopoDS_Shape& obstacle, double angle);
  void mesh_body(int body, const TopoDS_Shape& obstacle, const ShapeMap& faces, double angle);
  void build_graph();
  int build_bvh(int first, int count);
  FlexNearest nearest(const gp_XYZ& p, double within, const Filter& filter) const;
  // The normal that tells which side of the surface p lies on, where its
  // nearest point is on the triangle: the triangle's inside it, on an edge
  // the sum of the edge's triangles', at a corner the corner's (the
  // triangles' round it by their angles). By the triangle's own normal alone,
  // a point 5 mm above the rim of a wire's end was inside the wire, and the
  // route's points were pushed away 40 mm a pass (mitcad#1713).
  gp_XYZ side_normal(int triangle, const gp_XYZ& p) const;
  bool box_touches_other_body(const Box& box, int body) const;
  gp_XYZ node_point(int node) const;
  void triangle_nodes(int triangle, std::vector<int>& out) const;
  bool link_open(const gp_XYZ& from, const gp_XYZ& to, int triangle) const;
  std::vector<gp_XYZ> grid_path(const gp_XYZ& a, const gp_XYZ& b, double need, double pad_factor, bool& clean) const;

  double m_deflection = 0.0;
  double m_touch = 0.0; // how near bodies count as touching
  std::vector<TopoDS_Shape> m_obstacles;
  std::vector<ShapeMap> m_faces;
  std::vector<gp_XYZ> m_verts;
  std::vector<int> m_vert_body;
  std::vector<FlexTriangle> m_tris;
  std::vector<std::array<gp_XYZ, 3>> m_corner_normals;
  std::vector<gp_XYZ> m_vert_normal;
  std::vector<Edge> m_edges;
  std::vector<std::vector<int>> m_edge_tris;
  std::vector<std::vector<int>> m_vert_tris;
  // Where bodies touch: the corners of one that lie on a triangle of
  // another, by corner and by triangle.
  std::unordered_map<int, std::vector<int>> m_vert_bridge;
  std::unordered_map<int, std::vector<int>> m_tri_bridge;
  // Triangles another body lies near: a path in one is checked against
  // running under that body.
  std::vector<char> m_contested;
  std::vector<int> m_component; // of each corner
  std::vector<int> m_steiner_edge;
  int m_nodes = 0;
  std::vector<BvhNode> m_bvh;
  std::vector<int> m_order;
};

} // namespace mitcad::geometry::detail
