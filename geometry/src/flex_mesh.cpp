// SPDX-License-Identifier: MIT
#include "flex_mesh.hpp"

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <functional>
#include <memory>
#include <mutex>
#include <queue>
#include <utility>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepMesh_IncrementalMesh.hxx>
#include <BRep_Tool.hxx>
#include <Poly_Triangulation.hxx>
#include <TopExp.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>

namespace mitcad::geometry::detail {

namespace {

// The point of the triangle nearest to p.
gp_XYZ closest_on_triangle(const gp_XYZ& p, const gp_XYZ& a, const gp_XYZ& b, const gp_XYZ& c) {
  const gp_XYZ ab = b - a;
  const gp_XYZ ac = c - a;
  const gp_XYZ ap = p - a;
  const double d1 = ab.Dot(ap);
  const double d2 = ac.Dot(ap);
  if (d1 <= 0.0 && d2 <= 0.0) {
    return a;
  }
  const gp_XYZ bp = p - b;
  const double d3 = ab.Dot(bp);
  const double d4 = ac.Dot(bp);
  if (d3 >= 0.0 && d4 <= d3) {
    return b;
  }
  const double vc = d1 * d4 - d3 * d2;
  if (vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0) {
    return a + ab * (d1 / (d1 - d3));
  }
  const gp_XYZ cp = p - c;
  const double d5 = ab.Dot(cp);
  const double d6 = ac.Dot(cp);
  if (d6 >= 0.0 && d5 <= d6) {
    return c;
  }
  const double vb = d5 * d2 - d1 * d6;
  if (vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0) {
    return a + ac * (d2 / (d2 - d6));
  }
  const double va = d3 * d6 - d5 * d4;
  if (va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0) {
    return b + (c - b) * ((d4 - d3) / ((d4 - d3) + (d5 - d6)));
  }
  const double scale = 1.0 / (va + vb + vc);
  return a + ab * (vb * scale) + ac * (vc * scale);
}

// Where on the triangle abc the point nearest to p lies (as
// closest_on_triangle finds it): 0 inside it, 1 to 3 at its corner a, b or c,
// 4 to 6 on its edge ab, bc or ca.
int closest_feature(const gp_XYZ& p, const gp_XYZ& a, const gp_XYZ& b, const gp_XYZ& c) {
  const gp_XYZ ab = b - a;
  const gp_XYZ ac = c - a;
  const gp_XYZ ap = p - a;
  const double d1 = ab.Dot(ap);
  const double d2 = ac.Dot(ap);
  if (d1 <= 0.0 && d2 <= 0.0) {
    return 1;
  }
  const gp_XYZ bp = p - b;
  const double d3 = ab.Dot(bp);
  const double d4 = ac.Dot(bp);
  if (d3 >= 0.0 && d4 <= d3) {
    return 2;
  }
  const double vc = d1 * d4 - d3 * d2;
  if (vc <= 0.0 && d1 >= 0.0 && d3 <= 0.0) {
    return 4;
  }
  const gp_XYZ cp = p - c;
  const double d5 = ab.Dot(cp);
  const double d6 = ac.Dot(cp);
  if (d6 >= 0.0 && d5 <= d6) {
    return 3;
  }
  const double vb = d5 * d2 - d1 * d6;
  if (vb <= 0.0 && d2 >= 0.0 && d6 <= 0.0) {
    return 6;
  }
  const double va = d3 * d6 - d5 * d4;
  if (va <= 0.0 && (d4 - d3) >= 0.0 && (d5 - d6) >= 0.0) {
    return 5;
  }
  return 0;
}

std::uint64_t pair_key(int a, int b) {
  const std::uint64_t lo = static_cast<std::uint64_t>(std::min(a, b));
  const std::uint64_t hi = static_cast<std::uint64_t>(std::max(a, b));
  return (hi << 32) | lo;
}

int find_root(std::vector<int>& parent, int i) {
  while (el(parent, i) != i) {
    el(parent, i) = el(parent, el(parent, i));
    i = el(parent, i);
  }
  return i;
}

double seconds_since(std::chrono::steady_clock::time_point start) {
  return std::chrono::duration<double>(std::chrono::steady_clock::now() - start).count();
}

} // namespace

bool flex_tracing() {
  static const bool on = [] {
#ifdef _MSC_VER
    char* value = nullptr;
    std::size_t length = 0;
    if (_dupenv_s(&value, &length, "MITCAD_FLEX_TRACE") != 0 || value == nullptr) {
      return false;
    }
    std::free(value);
    return true;
#else
    return std::getenv("MITCAD_FLEX_TRACE") != nullptr;
#endif
  }();
  return on;
}

FlexMesh::FlexMesh(const std::vector<TopoDS_Shape>& obstacles, double deflection, double angle)
    : m_deflection(deflection), m_touch(1.5 * deflection + 1.0e-4), m_obstacles(obstacles) {
  for (int body = 0; body < isize(m_obstacles); ++body) {
    const auto started = std::chrono::steady_clock::now();
    const int before = isize(m_tris);
    add_body(body, el(m_obstacles, body), angle);
    if (flex_tracing()) {
      std::fprintf(stderr, "flex-trace: body %d meshed in %.3f s, %d triangles\n", body, seconds_since(started),
                   isize(m_tris) - before);
    }
  }
  const auto started = std::chrono::steady_clock::now();
  build_graph();
  if (flex_tracing()) {
    std::fprintf(stderr, "flex-trace: the triangles joined in %.3f s\n", seconds_since(started));
  }
}

namespace {

// A body's triangles as add_body made them, its corners counted from 0: the
// bodies meshed last are kept, so that a router of a set with one body more
// (the item laid before, the next item's obstacle) meshes that body alone
// (the 4 bodies before it took half of 0.73 s on the build VM, mitcad#1713).
// The same triangles: the body's own, meshed with the same deflections.
struct MeshedBody {
  TopoDS_Shape shape; // (held, so that it is not another body at its address)
  double deflection = 0.0;
  double angle = 0.0;
  std::vector<gp_XYZ> verts;
  std::vector<FlexTriangle> tris;
};

constexpr std::size_t kMeshedKept = 64;

std::mutex& meshed_mutex() {
  static std::mutex mutex;
  return mutex;
}

std::vector<std::shared_ptr<const MeshedBody>>& meshed_bodies() {
  static std::vector<std::shared_ptr<const MeshedBody>> kept;
  return kept;
}

} // namespace

void FlexMesh::add_body(int body, const TopoDS_Shape& obstacle, double angle) {
  ShapeMap faces;
  if (!obstacle.IsNull()) {
    TopExp::MapShapes(obstacle, TopAbs_FACE, faces);
  }
  m_faces.push_back(faces);
  if (faces.IsEmpty()) {
    return;
  }
  std::shared_ptr<const MeshedBody> kept;
  {
    const std::lock_guard<std::mutex> lock(meshed_mutex());
    std::vector<std::shared_ptr<const MeshedBody>>& bodies = meshed_bodies();
    for (auto it = bodies.begin(); it != bodies.end(); ++it) {
      if ((*it)->shape.IsEqual(obstacle) && (*it)->deflection == m_deflection && (*it)->angle == angle) {
        kept = *it;
        bodies.erase(it);
        bodies.push_back(kept);
        break;
      }
    }
  }
  if (kept) {
    const int offset = isize(m_verts);
    for (const gp_XYZ& p : kept->verts) {
      m_verts.push_back(p);
      m_vert_body.push_back(body);
    }
    for (FlexTriangle triangle : kept->tris) {
      for (int& v : triangle.v) {
        v += offset;
      }
      triangle.body = body;
      m_tris.push_back(triangle);
    }
    return;
  }
  const int first_vert = isize(m_verts);
  const int first_tri = isize(m_tris);
  mesh_body(body, obstacle, faces, angle);
  auto made = std::make_shared<MeshedBody>();
  made->shape = obstacle;
  made->deflection = m_deflection;
  made->angle = angle;
  made->verts.assign(m_verts.begin() + first_vert, m_verts.end());
  made->tris.assign(m_tris.begin() + first_tri, m_tris.end());
  for (FlexTriangle& triangle : made->tris) {
    for (int& v : triangle.v) {
      v -= first_vert;
    }
  }
  const std::lock_guard<std::mutex> lock(meshed_mutex());
  std::vector<std::shared_ptr<const MeshedBody>>& bodies = meshed_bodies();
  bodies.push_back(std::move(made));
  if (bodies.size() > kMeshedKept) {
    bodies.erase(bodies.begin());
  }
}

void FlexMesh::mesh_body(int body, const TopoDS_Shape& obstacle, const ShapeMap& faces, double angle) {
  // (A copy: the mesher writes its triangles into the faces it meshes.)
  const TopoDS_Shape copy = BRepBuilderAPI_Copy(obstacle, true, false).Shape();
  const BRepMesh_IncrementalMesh mesher(copy, m_deflection, false, angle, true);
  ShapeMap copied;
  TopExp::MapShapes(copy, TopAbs_FACE, copied);
  // The copy's faces come in the order of the obstacle's; were they not,
  // the triangles would carry no face.
  const bool same_order = copied.Extent() == faces.Extent();

  // The corners of the body's triangles, those of neighbouring faces at
  // one place joined.
  const double cell = 4.0e-6;
  const double join2 = 1.0e-6 * 1.0e-6;
  std::unordered_map<std::uint64_t, std::vector<int>> cells;
  const auto cell_key = [](long long ix, long long iy, long long iz) {
    return (static_cast<std::uint64_t>(ix) * 73856093ULL) ^ (static_cast<std::uint64_t>(iy) * 19349663ULL) ^
           (static_cast<std::uint64_t>(iz) * 83492791ULL);
  };
  const auto corner = [&](const gp_XYZ& p) {
    const long long ix = std::llround(p.X() / cell);
    const long long iy = std::llround(p.Y() / cell);
    const long long iz = std::llround(p.Z() / cell);
    for (long long dx = -1; dx <= 1; ++dx) {
      for (long long dy = -1; dy <= 1; ++dy) {
        for (long long dz = -1; dz <= 1; ++dz) {
          const auto found = cells.find(cell_key(ix + dx, iy + dy, iz + dz));
          if (found == cells.end()) {
            continue;
          }
          for (const int candidate : found->second) {
            if ((el(m_verts, candidate) - p).SquareModulus() <= join2) {
              return candidate;
            }
          }
        }
      }
    }
    const int made = isize(m_verts);
    m_verts.push_back(p);
    m_vert_body.push_back(body);
    cells[cell_key(ix, iy, iz)].push_back(made);
    return made;
  };

  std::vector<int> nodes;
  for (int i = 1; i <= copied.Extent(); ++i) {
    const TopoDS_Face& face = TopoDS::Face(copied(i));
    TopLoc_Location location;
    const occ::handle<Poly_Triangulation> triangulation = BRep_Tool::Triangulation(face, location);
    if (triangulation.IsNull()) {
      continue; // a face the mesher failed on
    }
    bool curved = false;
    {
      TopLoc_Location at;
      if (!BRep_Tool::Surface(face, at).IsNull()) {
        curved = BRepAdaptor_Surface(face, false).GetType() != GeomAbs_Plane;
      }
    }
    const gp_Trsf placement = location.Transformation();
    nodes.assign(static_cast<std::size_t>(triangulation->NbNodes()), 0);
    for (int k = 1; k <= triangulation->NbNodes(); ++k) {
      gp_Pnt p = triangulation->Node(k);
      if (!location.IsIdentity()) {
        p.Transform(placement);
      }
      el(nodes, k - 1) = corner(p.XYZ());
    }
    // A face's triangles follow its surface's normal: turned round where
    // the face is reversed, or mirrored by its placement.
    const bool turned = (face.Orientation() == TopAbs_REVERSED) != placement.IsNegative();
    for (int t = 1; t <= triangulation->NbTriangles(); ++t) {
      int a = 0;
      int b = 0;
      int c = 0;
      triangulation->Triangle(t).Get(a, b, c);
      if (turned) {
        std::swap(b, c);
      }
      FlexTriangle triangle;
      triangle.v = {el(nodes, a - 1), el(nodes, b - 1), el(nodes, c - 1)};
      if (triangle.v[0] == triangle.v[1] || triangle.v[1] == triangle.v[2] || triangle.v[2] == triangle.v[0]) {
        continue;
      }
      const gp_XYZ across = (el(m_verts, triangle.v[1]) - el(m_verts, triangle.v[0]))
                                .Crossed(el(m_verts, triangle.v[2]) - el(m_verts, triangle.v[0]));
      const double twice = across.Modulus();
      if (twice < 1.0e-12) {
        continue;
      }
      triangle.normal = across / twice;
      triangle.body = body;
      triangle.face = same_order ? i - 1 : -1;
      triangle.curved = curved;
      m_tris.push_back(triangle);
    }
  }
}

void FlexMesh::build_graph() {
  const int vert_count = isize(m_verts);
  m_vert_tris.assign(static_cast<std::size_t>(vert_count), std::vector<int>());
  std::unordered_map<std::uint64_t, int> edge_of;
  gp_XYZ lo(kFlexFar, kFlexFar, kFlexFar);
  gp_XYZ hi(-kFlexFar, -kFlexFar, -kFlexFar);
  for (const gp_XYZ& p : m_verts) {
    for (int c = 1; c <= 3; ++c) {
      lo.SetCoord(c, std::min(lo.Coord(c), p.Coord(c)));
      hi.SetCoord(c, std::max(hi.Coord(c), p.Coord(c)));
    }
  }
  for (int t = 0; t < isize(m_tris); ++t) {
    FlexTriangle& triangle = el(m_tris, t);
    for (int k = 0; k < 3; ++k) {
      const int a = el(triangle.v, k);
      const int b = el(triangle.v, (k + 1) % 3);
      el(m_vert_tris, a).push_back(t);
      const std::uint64_t key = pair_key(a, b);
      const auto found = edge_of.find(key);
      int edge = 0;
      if (found == edge_of.end()) {
        edge = isize(m_edges);
        Edge made;
        made.a = std::min(a, b);
        made.b = std::max(a, b);
        m_edges.push_back(made);
        m_edge_tris.emplace_back();
        edge_of.emplace(key, edge);
      } else {
        edge = found->second;
      }
      el(triangle.e, k) = edge;
      el(m_edge_tris, edge).push_back(t);
    }
  }
  // Each corner's normal for side_normal(): the triangles' round it, each by
  // its angle there.
  m_vert_normal.assign(static_cast<std::size_t>(vert_count), gp_XYZ(0.0, 0.0, 0.0));
  for (const FlexTriangle& triangle : m_tris) {
    for (int k = 0; k < 3; ++k) {
      const gp_XYZ& at = el(m_verts, el(triangle.v, k));
      const gp_XYZ u = el(m_verts, el(triangle.v, (k + 1) % 3)) - at;
      const gp_XYZ w = el(m_verts, el(triangle.v, (k + 2) % 3)) - at;
      const double angle = std::atan2(u.Crossed(w).Modulus(), u.Dot(w));
      el(m_vert_normal, el(triangle.v, k)) += triangle.normal * angle;
    }
  }
  // Each corner's normal for smooth_normal().
  m_corner_normals.resize(m_tris.size());
  for (int t = 0; t < isize(m_tris); ++t) {
    const FlexTriangle& triangle = el(m_tris, t);
    for (int k = 0; k < 3; ++k) {
      gp_XYZ sum(0.0, 0.0, 0.0);
      for (const int other : el(m_vert_tris, el(triangle.v, k))) {
        if (el(m_tris, other).normal.Dot(triangle.normal) > 0.866) {
          sum = sum + el(m_tris, other).normal;
        }
      }
      const double size = sum.Modulus();
      el(el(m_corner_normals, t), k) = size > 1.0e-9 ? sum / size : triangle.normal;
    }
  }
  // Points on the edges for the paths to cross them at: three on an edge,
  // one on a short one.
  const double diagonal = m_verts.empty() ? 0.0 : (hi - lo).Modulus();
  m_nodes = vert_count;
  for (int e = 0; e < isize(m_edges); ++e) {
    Edge& edge = el(m_edges, e);
    const double length = (el(m_verts, edge.b) - el(m_verts, edge.a)).Modulus();
    edge.first = m_nodes;
    edge.count = length < 1.0e-6 ? 0 : length < diagonal / 300.0 ? 1 : 3;
    for (int k = 0; k < edge.count; ++k) {
      m_steiner_edge.push_back(e);
    }
    m_nodes += edge.count;
  }

  m_order.resize(m_tris.size());
  for (int t = 0; t < isize(m_tris); ++t) {
    el(m_order, t) = t;
  }
  if (!m_tris.empty()) {
    build_bvh(0, isize(m_tris));
  }

  // Where bodies touch.
  if (body_count() > 1) {
    for (int v = 0; v < vert_count; ++v) {
      Filter others;
      others.not_body = el(m_vert_body, v);
      const FlexNearest hit = nearest(el(m_verts, v), m_touch, others);
      if (hit.triangle >= 0) {
        m_vert_bridge[v].push_back(hit.triangle);
        m_tri_bridge[hit.triangle].push_back(v);
      }
    }
    m_contested.assign(m_tris.size(), 0);
    const double reach = 4.0 * m_touch;
    for (int t = 0; t < isize(m_tris); ++t) {
      const FlexTriangle& triangle = el(m_tris, t);
      Box box;
      box.lo = el(m_verts, triangle.v[0]);
      box.hi = box.lo;
      for (int k = 1; k < 3; ++k) {
        const gp_XYZ& p = el(m_verts, el(triangle.v, k));
        for (int c = 1; c <= 3; ++c) {
          box.lo.SetCoord(c, std::min(box.lo.Coord(c), p.Coord(c)));
          box.hi.SetCoord(c, std::max(box.hi.Coord(c), p.Coord(c)));
        }
      }
      box.lo = box.lo - gp_XYZ(reach, reach, reach);
      box.hi = box.hi + gp_XYZ(reach, reach, reach);
      el(m_contested, t) = static_cast<char>(box_touches_other_body(box, triangle.body));
    }
  }

  // The corners a path along the surfaces joins.
  std::vector<int> parent(static_cast<std::size_t>(vert_count));
  for (int v = 0; v < vert_count; ++v) {
    el(parent, v) = v;
  }
  const auto unite = [&](int a, int b) {
    const int ra = find_root(parent, a);
    const int rb = find_root(parent, b);
    if (ra != rb) {
      el(parent, ra) = rb;
    }
  };
  for (const FlexTriangle& triangle : m_tris) {
    unite(triangle.v[0], triangle.v[1]);
    unite(triangle.v[0], triangle.v[2]);
  }
  for (const auto& [v, triangles] : m_vert_bridge) {
    for (const int t : triangles) {
      unite(v, el(m_tris, t).v[0]);
    }
  }
  m_component.resize(static_cast<std::size_t>(vert_count));
  for (int v = 0; v < vert_count; ++v) {
    el(m_component, v) = find_root(parent, v);
  }
}

int FlexMesh::build_bvh(int first, int count) {
  BvhNode node;
  node.box.lo = gp_XYZ(kFlexFar, kFlexFar, kFlexFar);
  node.box.hi = gp_XYZ(-kFlexFar, -kFlexFar, -kFlexFar);
  for (int k = first; k < first + count; ++k) {
    const FlexTriangle& triangle = el(m_tris, el(m_order, k));
    for (const int v : triangle.v) {
      const gp_XYZ& p = el(m_verts, v);
      for (int c = 1; c <= 3; ++c) {
        node.box.lo.SetCoord(c, std::min(node.box.lo.Coord(c), p.Coord(c)));
        node.box.hi.SetCoord(c, std::max(node.box.hi.Coord(c), p.Coord(c)));
      }
    }
  }
  const int slot = isize(m_bvh);
  m_bvh.push_back(node);
  if (count <= 4) {
    el(m_bvh, slot).first = first;
    el(m_bvh, slot).count = count;
    return slot;
  }
  int axis = 1;
  const gp_XYZ extent = node.box.hi - node.box.lo;
  if (extent.Y() > extent.Coord(axis)) {
    axis = 2;
  }
  if (extent.Z() > extent.Coord(axis)) {
    axis = 3;
  }
  const auto middle = [&](int t) {
    const FlexTriangle& triangle = el(m_tris, t);
    return el(m_verts, triangle.v[0]).Coord(axis) + el(m_verts, triangle.v[1]).Coord(axis) +
           el(m_verts, triangle.v[2]).Coord(axis);
  };
  const int half = first + count / 2;
  std::nth_element(m_order.begin() + first, m_order.begin() + half, m_order.begin() + (first + count),
                   [&](int l, int r) { return middle(l) < middle(r); });
  const int left = build_bvh(first, half - first);
  const int right = build_bvh(half, first + count - half);
  el(m_bvh, slot).left = left;
  el(m_bvh, slot).right = right;
  return slot;
}

namespace {

// (By the coordinates' own accessors and plain comparisons: this is most of
// the search for the nearest triangle, and with Coord(c), which checks its
// index, and std::max of a list it was most of a route's time onto the
// dense triangles of a swept wire, mitcad#1713.)
double box_distance2(const gp_XYZ& lo, const gp_XYZ& hi, const gp_XYZ& p) {
  const auto off = [](double low, double high, double v) {
    return v < low ? low - v : v > high ? v - high : 0.0;
  };
  const double dx = off(lo.X(), hi.X(), p.X());
  const double dy = off(lo.Y(), hi.Y(), p.Y());
  const double dz = off(lo.Z(), hi.Z(), p.Z());
  return dx * dx + dy * dy + dz * dz;
}

} // namespace

FlexNearest FlexMesh::nearest(const gp_XYZ& p, double within, const Filter& filter) const {
  FlexNearest best;
  if (m_bvh.empty()) {
    return best;
  }
  double best2 = within * within;
  std::array<int, 128> stack{};
  int top = 0;
  el(stack, top++) = 0;
  while (top > 0) {
    const BvhNode& node = el(m_bvh, el(stack, --top));
    if (box_distance2(node.box.lo, node.box.hi, p) >= best2) {
      continue;
    }
    if (node.count > 0) {
      for (int k = node.first; k < node.first + node.count; ++k) {
        const int t = el(m_order, k);
        const FlexTriangle& triangle = el(m_tris, t);
        if ((filter.body >= 0 && triangle.body != filter.body) ||
            (filter.face >= 0 && triangle.face != filter.face) ||
            (filter.not_body >= 0 && triangle.body == filter.not_body)) {
          continue;
        }
        const gp_XYZ q = closest_on_triangle(p, el(m_verts, triangle.v[0]), el(m_verts, triangle.v[1]),
                                             el(m_verts, triangle.v[2]));
        const double d2 = (p - q).SquareModulus();
        if (d2 < best2) {
          best2 = d2;
          best.triangle = t;
          best.point = q;
        }
      }
      continue;
    }
    if (top + 2 > isize(stack)) {
      continue; // (never with a balanced tree)
    }
    const BvhNode& left = el(m_bvh, node.left);
    const BvhNode& right = el(m_bvh, node.right);
    const double dl = box_distance2(left.box.lo, left.box.hi, p);
    const double dr = box_distance2(right.box.lo, right.box.hi, p);
    // The nearer child last, so that it is looked into first.
    if (dl < dr) {
      el(stack, top++) = node.right;
      el(stack, top++) = node.left;
    } else {
      el(stack, top++) = node.left;
      el(stack, top++) = node.right;
    }
  }
  if (best.triangle >= 0) {
    best.distance = std::sqrt(best2);
    best.inside = (p - best.point).Dot(side_normal(best.triangle, p)) < 0.0;
  }
  return best;
}

gp_XYZ FlexMesh::side_normal(int triangle, const gp_XYZ& p) const {
  const FlexTriangle& t = el(m_tris, triangle);
  const int feature = closest_feature(p, el(m_verts, t.v[0]), el(m_verts, t.v[1]), el(m_verts, t.v[2]));
  if (feature >= 1 && feature <= 3) {
    const gp_XYZ& corner = el(m_vert_normal, el(t.v, feature - 1));
    return corner.SquareModulus() > 1.0e-24 ? corner : t.normal;
  }
  if (feature >= 4) {
    gp_XYZ sum(0.0, 0.0, 0.0);
    for (const int other : el(m_edge_tris, el(t.e, feature - 4))) {
      sum = sum + el(m_tris, other).normal;
    }
    return sum.SquareModulus() > 1.0e-24 ? sum : t.normal;
  }
  return t.normal;
}

FlexNearest FlexMesh::nearest(const gp_XYZ& p, double within) const { return nearest(p, within, Filter()); }

FlexNearest FlexMesh::nearest_on(const gp_XYZ& p, int body, int face) const {
  Filter filter;
  filter.body = body;
  filter.face = face;
  return nearest(p, kFlexFar, filter);
}

bool FlexMesh::box_touches_other_body(const Box& box, int body) const {
  if (m_bvh.empty()) {
    return false;
  }
  const auto apart = [&](const gp_XYZ& lo, const gp_XYZ& hi) {
    for (int c = 1; c <= 3; ++c) {
      if (lo.Coord(c) > box.hi.Coord(c) || hi.Coord(c) < box.lo.Coord(c)) {
        return true;
      }
    }
    return false;
  };
  std::array<int, 128> stack{};
  int top = 0;
  el(stack, top++) = 0;
  while (top > 0) {
    const BvhNode& node = el(m_bvh, el(stack, --top));
    if (apart(node.box.lo, node.box.hi)) {
      continue;
    }
    if (node.count > 0) {
      for (int k = node.first; k < node.first + node.count; ++k) {
        const FlexTriangle& triangle = el(m_tris, el(m_order, k));
        if (triangle.body == body) {
          continue;
        }
        gp_XYZ lo = el(m_verts, triangle.v[0]);
        gp_XYZ hi = lo;
        for (int i = 1; i < 3; ++i) {
          const gp_XYZ& p = el(m_verts, el(triangle.v, i));
          for (int c = 1; c <= 3; ++c) {
            lo.SetCoord(c, std::min(lo.Coord(c), p.Coord(c)));
            hi.SetCoord(c, std::max(hi.Coord(c), p.Coord(c)));
          }
        }
        if (!apart(lo, hi)) {
          return true;
        }
      }
      continue;
    }
    if (top + 2 > isize(stack)) {
      continue;
    }
    el(stack, top++) = node.left;
    el(stack, top++) = node.right;
  }
  return false;
}

int FlexMesh::face_index(int body, const TopoDS_Face& face) const {
  if (body < 0 || body >= isize(m_faces) || face.IsNull()) {
    return -1;
  }
  return el(m_faces, body).FindIndex(face) - 1;
}

bool FlexMesh::connected(int a, int b) const {
  if (a < 0 || b < 0) {
    return false;
  }
  return el(m_component, el(m_tris, a).v[0]) == el(m_component, el(m_tris, b).v[0]);
}

gp_XYZ FlexMesh::smooth_normal(int triangle, const gp_XYZ& point) const {
  const FlexTriangle& t = el(m_tris, triangle);
  const gp_XYZ& a = el(m_verts, t.v[0]);
  const gp_XYZ v0 = el(m_verts, t.v[1]) - a;
  const gp_XYZ v1 = el(m_verts, t.v[2]) - a;
  const gp_XYZ v2 = point - a;
  const double d00 = v0.Dot(v0);
  const double d01 = v0.Dot(v1);
  const double d11 = v1.Dot(v1);
  const double d20 = v2.Dot(v0);
  const double d21 = v2.Dot(v1);
  const double scale = d00 * d11 - d01 * d01;
  if (scale < 1.0e-24) {
    return t.normal;
  }
  // The point's weights of the three corners, kept inside the triangle.
  const double wb = std::max((d11 * d20 - d01 * d21) / scale, 0.0);
  const double wc = std::max((d00 * d21 - d01 * d20) / scale, 0.0);
  const double wa = std::max(1.0 - wb - wc, 0.0);
  const std::array<gp_XYZ, 3>& corners = el(m_corner_normals, triangle);
  const gp_XYZ sum = corners[0] * wa + corners[1] * wb + corners[2] * wc;
  const double size = sum.Modulus();
  return size > 1.0e-9 ? sum / size : t.normal;
}

gp_XYZ FlexMesh::node_point(int node) const {
  if (node < isize(m_verts)) {
    return el(m_verts, node);
  }
  const Edge& edge = el(m_edges, el(m_steiner_edge, node - isize(m_verts)));
  const double u = (node - edge.first + 1.0) / (edge.count + 1.0);
  const gp_XYZ& a = el(m_verts, edge.a);
  return a + (el(m_verts, edge.b) - a) * u;
}

void FlexMesh::triangle_nodes(int triangle, std::vector<int>& out) const {
  const FlexTriangle& t = el(m_tris, triangle);
  for (const int v : t.v) {
    out.push_back(v);
  }
  for (const int e : t.e) {
    const Edge& edge = el(m_edges, e);
    for (int k = 0; k < edge.count; ++k) {
      out.push_back(edge.first + k);
    }
  }
}

bool FlexMesh::link_open(const gp_XYZ& from, const gp_XYZ& to, int triangle) const {
  if (m_contested.empty() || el(m_contested, triangle) == 0) {
    return true;
  }
  // A little above the triangle, the way stays out of the other bodies:
  // it does not run under a body that lies on this one.
  const FlexTriangle& t = el(m_tris, triangle);
  Filter others;
  others.not_body = t.body;
  const gp_XYZ lift = t.normal * (4.0 * m_touch);
  for (const double f : {0.25, 0.5, 0.75}) {
    const gp_XYZ probe = from + (to - from) * f + lift;
    const FlexNearest hit = nearest(probe, kFlexFar, others);
    if (hit.triangle >= 0 && hit.inside && hit.distance > 1.0e-6) {
      return false;
    }
  }
  return true;
}

std::vector<FlexSurfacePoint> FlexMesh::surface_path(const gp_XYZ& a, int ta, const gp_XYZ& b, int tb,
                                                     const gp_XYZ* heading_a, const gp_XYZ* heading_b,
                                                     double turn_cost) const {
  std::vector<FlexSurfacePoint> out;
  if (ta < 0 || tb < 0 || !connected(ta, tb)) {
    return out;
  }
  const auto path_point = [&](const gp_XYZ& p, int arriving, int leaving) {
    FlexSurfacePoint point;
    point.point = p;
    point.normal_in = el(m_tris, arriving).normal;
    point.normal_out = el(m_tris, leaving).normal;
    point.curved = el(m_tris, arriving).curved || el(m_tris, leaving).curved;
    return point;
  };
  // What a turn in the surface costs, as length: the angle between the way
  // the route runs at an end (`heading`, laid into the triangle) and the way
  // the path leaves or arrives there.
  const auto turn = [&](const gp_XYZ* heading, const gp_XYZ& way, int t) {
    if (heading == nullptr || turn_cost <= 0.0) {
      return 0.0;
    }
    const gp_XYZ& normal = el(m_tris, t).normal;
    const gp_XYZ h = *heading - normal * heading->Dot(normal);
    const gp_XYZ w = way - normal * way.Dot(normal);
    if (h.Modulus() < 1.0e-9 || w.Modulus() < 1.0e-9) {
      return 0.0;
    }
    return turn_cost * std::atan2(h.Crossed(w).Modulus(), h.Dot(w));
  };
  if (ta == tb) {
    out.push_back(path_point(a, ta, ta));
    out.push_back(path_point(b, ta, ta));
    return out;
  }

  // The shortest way over the triangles' corners and the points on their
  // edges (A*, the straight distance to b ahead): inside a triangle every
  // two of its points are joined by a segment in it.
  const std::size_t node_count = static_cast<std::size_t>(m_nodes);
  std::vector<double> cost(node_count, kFlexFar);
  std::vector<int> parent(node_count, -1);
  std::vector<int> via(node_count, -1);
  std::vector<char> done(node_count, 0);
  std::vector<char> target(node_count, 0);
  using Entry = std::pair<double, int>;
  std::priority_queue<Entry, std::vector<Entry>, std::greater<Entry>> frontier;
  std::vector<int> ring;
  // The nodes of a triangle, and the corners of other bodies that lie on it.
  const auto ring_of = [&](int t) {
    ring.clear();
    triangle_nodes(t, ring);
    const auto bridged = m_tri_bridge.find(t);
    if (bridged != m_tri_bridge.end()) {
      ring.insert(ring.end(), bridged->second.begin(), bridged->second.end());
    }
  };
  ring_of(tb);
  for (const int y : ring) {
    el(target, y) = 1;
  }
  ring_of(ta);
  for (const int y : ring) {
    const gp_XYZ py = node_point(y);
    const double g = (py - a).Modulus() + turn(heading_a, py - a, ta);
    if (g < el(cost, y) && link_open(a, py, ta)) {
      el(cost, y) = g;
      el(via, y) = ta;
      frontier.emplace(g + (py - b).Modulus(), y);
    }
  }
  int last = -1;
  double total = kFlexFar;
  const int vert_count = isize(m_verts);
  while (!frontier.empty()) {
    const double ahead = frontier.top().first;
    const int x = frontier.top().second;
    frontier.pop();
    if (el(done, x) != 0) {
      continue;
    }
    if (ahead >= total) {
      break; // (nothing left is shorter than the way found)
    }
    el(done, x) = 1;
    const gp_XYZ px = node_point(x);
    if (el(target, x) != 0 && link_open(px, b, tb)) {
      const double whole = el(cost, x) + (b - px).Modulus() + turn(heading_b, b - px, tb);
      if (whole < total) {
        total = whole;
        last = x;
      }
    }
    const double here = el(cost, x);
    const auto relax = [&](int t) {
      ring_of(t);
      for (const int y : ring) {
        if (y == x || el(done, y) != 0) {
          continue;
        }
        const gp_XYZ py = node_point(y);
        const double g = here + (py - px).Modulus();
        if (g < el(cost, y) && link_open(px, py, t)) {
          el(cost, y) = g;
          el(parent, y) = x;
          el(via, y) = t;
          frontier.emplace(g + (py - b).Modulus(), y);
        }
      }
    };
    if (x < vert_count) {
      for (const int t : el(m_vert_tris, x)) {
        relax(t);
      }
      const auto bridged = m_vert_bridge.find(x);
      if (bridged != m_vert_bridge.end()) {
        for (const int t : bridged->second) {
          relax(t);
        }
      }
    } else {
      for (const int t : el(m_edge_tris, el(m_steiner_edge, x - vert_count))) {
        // (On across the edge: not back into the triangle it came through,
        // which no shortest way does, and a way made to leave its start
        // another way round would.)
        if (t != el(via, x)) {
          relax(t);
        }
      }
    }
  }
  if (last < 0) {
    return out;
  }
  std::vector<int> chain;
  for (int x = last; x >= 0; x = el(parent, x)) {
    chain.push_back(x);
  }
  std::reverse(chain.begin(), chain.end());

  // The points of the way, and the triangle each of its segments lies in.
  const int count = isize(chain) + 2;
  std::vector<gp_XYZ> points(static_cast<std::size_t>(count));
  std::vector<int> segment(static_cast<std::size_t>(count - 1));
  points.front() = a;
  points.back() = b;
  for (int j = 0; j < isize(chain); ++j) {
    el(points, j + 1) = node_point(el(chain, j));
    el(segment, j) = el(via, el(chain, j));
  }
  segment.back() = tb;

  // Straightened: each point on an edge moved along it to where the way
  // through it is shortest, in turn, until none moves.
  for (int sweep = 0; sweep < 200; ++sweep) {
    double moved = 0.0;
    for (int j = 1; j + 1 < count; ++j) {
      const int node = el(chain, j - 1);
      if (node < vert_count) {
        continue;
      }
      const Edge& edge = el(m_edges, el(m_steiner_edge, node - vert_count));
      const gp_XYZ& origin = el(m_verts, edge.a);
      gp_XYZ along = el(m_verts, edge.b) - origin;
      const double length = along.Modulus();
      along = along / length;
      const gp_XYZ before = el(points, j - 1) - origin;
      const gp_XYZ after = el(points, j + 1) - origin;
      const double t1 = before.Dot(along);
      const double t2 = after.Dot(along);
      const double d1 = (before - along * t1).Modulus();
      const double d2 = (after - along * t2).Modulus();
      const double t = std::clamp(d1 + d2 > 1.0e-12 ? (t1 * d2 + t2 * d1) / (d1 + d2) : 0.5 * (t1 + t2), 0.0, length);
      const gp_XYZ placed = origin + along * t;
      moved = std::max(moved, (placed - el(points, j)).Modulus());
      el(points, j) = placed;
    }
    if (moved < 1.0e-9) {
      break;
    }
  }

  for (int j = 0; j < count; ++j) {
    const int arriving = el(segment, std::max(j - 1, 0));
    const int leaving = el(segment, std::min(j, count - 2));
    if (!out.empty() && (out.back().point - el(points, j)).Modulus() < 1.0e-9) {
      // Two points at one place (a point moved to its edge's end): one,
      // between the first one's triangle and the second one's.
      out.back().normal_out = el(m_tris, leaving).normal;
      out.back().curved = out.back().curved || el(m_tris, leaving).curved;
      continue;
    }
    out.push_back(path_point(el(points, j), arriving, leaving));
  }
  if (out.size() < 2) {
    out.push_back(path_point(b, tb, tb));
  }
  return out;
}

bool FlexMesh::segment_clear(const gp_XYZ& a, const gp_XYZ& b, double need) const {
  if (empty()) {
    return true;
  }
  const double length = (b - a).Modulus();
  const gp_XYZ along = length > 1.0e-12 ? (b - a) / length : gp_XYZ(0.0, 0.0, 0.0);
  // (A point `need` or more from everything stays outside for a quarter of
  // `need` further: no wall is stepped over.)
  const double least = std::max(0.25 * need, 1.0e-4);
  const double slack = 0.98 * need - 1.0e-9;
  double t = 0.0;
  for (;;) {
    const FlexNearest hit = nearest(a + along * t);
    if (hit.triangle < 0) {
      return true;
    }
    if ((hit.inside && hit.distance > 1.0e-6) || hit.distance < slack) {
      return false;
    }
    if (t >= length) {
      return true;
    }
    t = std::min(length, t + std::max(hit.distance - need, least));
  }
}

std::vector<gp_XYZ> FlexMesh::grid_path(const gp_XYZ& a, const gp_XYZ& b, double need, double pad_factor,
                                        bool& clean) const {
  clean = false;
  const double span = (b - a).Modulus();
  const double pad = pad_factor * std::max({0.75 * span, 6.0 * need, 1.0});
  gp_XYZ lo;
  gp_XYZ hi;
  for (int c = 1; c <= 3; ++c) {
    lo.SetCoord(c, std::min(a.Coord(c), b.Coord(c)) - pad);
    hi.SetCoord(c, std::max(a.Coord(c), b.Coord(c)) + pad);
  }
  const gp_XYZ size = hi - lo;
  constexpr int kCells = 40;
  const double cell = std::max({size.X(), size.Y(), size.Z()}) / kCells;
  std::array<int, 3> n{};
  for (int c = 0; c < 3; ++c) {
    el(n, c) = std::clamp(static_cast<int>(std::ceil(size.Coord(c + 1) / cell)), 1, kCells);
  }
  const int count = n[0] * n[1] * n[2];
  const auto centre = [&](int id) {
    const int i = id % n[0];
    const int j = (id / n[0]) % n[1];
    const int k = id / (n[0] * n[1]);
    return gp_XYZ(lo.X() + (i + 0.5) * cell, lo.Y() + (j + 0.5) * cell, lo.Z() + (k + 0.5) * cell);
  };
  const auto cell_of = [&](const gp_XYZ& p) {
    std::array<int, 3> at{};
    for (int c = 0; c < 3; ++c) {
      el(at, c) = std::clamp(static_cast<int>(std::floor((p.Coord(c + 1) - lo.Coord(c + 1)) / cell)), 0, el(n, c) - 1);
    }
    return (at[2] * n[1] + at[1]) * n[0] + at[0];
  };
  // A cell: 1 clear, 2 nearer to a body than `need`, 3 inside one; worked
  // out when the search first reaches it.
  std::vector<char> state(static_cast<std::size_t>(count), 0);
  const auto state_of = [&](int id) {
    char& s = el(state, id);
    if (s == 0) {
      const FlexNearest hit = nearest(centre(id));
      const int kind = hit.triangle < 0 ? 1 : (hit.inside && hit.distance > 1.0e-6) ? 3 : hit.distance < need ? 2 : 1;
      s = static_cast<char>(kind);
    }
    return s;
  };
  const int start = cell_of(a);
  const int goal = cell_of(b);
  el(state, start) = 1;
  el(state, goal) = 1;
  if (start == goal) {
    return {a, b};
  }
  // Through a cell that is not clear the way costs so much more that it is
  // taken only where there is no other.
  std::vector<double> cost(static_cast<std::size_t>(count), kFlexFar);
  std::vector<int> parent(static_cast<std::size_t>(count), -1);
  std::vector<char> done(static_cast<std::size_t>(count), 0);
  using Entry = std::pair<double, int>;
  std::priority_queue<Entry, std::vector<Entry>, std::greater<Entry>> frontier;
  const gp_XYZ goal_centre = centre(goal);
  el(cost, start) = 0.0;
  frontier.emplace((centre(start) - goal_centre).Modulus(), start);
  bool reached = false;
  while (!frontier.empty()) {
    const int x = frontier.top().second;
    frontier.pop();
    if (el(done, x) != 0) {
      continue;
    }
    el(done, x) = 1;
    if (x == goal) {
      reached = true;
      break;
    }
    const int xi = x % n[0];
    const int xj = (x / n[0]) % n[1];
    const int xk = x / (n[0] * n[1]);
    for (int dk = -1; dk <= 1; ++dk) {
      for (int dj = -1; dj <= 1; ++dj) {
        for (int di = -1; di <= 1; ++di) {
          const int yi = xi + di;
          const int yj = xj + dj;
          const int yk = xk + dk;
          if ((di == 0 && dj == 0 && dk == 0) || yi < 0 || yj < 0 || yk < 0 || yi >= n[0] || yj >= n[1] ||
              yk >= n[2]) {
            continue;
          }
          const int y = (yk * n[1] + yj) * n[0] + yi;
          if (el(done, y) != 0) {
            continue;
          }
          const char kind = state_of(y);
          const double penalty = kind == 1 ? 1.0 : kind == 2 ? 30.0 : 300.0;
          const double g = el(cost, x) + cell * std::sqrt(static_cast<double>(di * di + dj * dj + dk * dk)) * penalty;
          if (g < el(cost, y)) {
            el(cost, y) = g;
            el(parent, y) = x;
            frontier.emplace(g + (centre(y) - goal_centre).Modulus(), y);
          }
        }
      }
    }
  }
  if (!reached) {
    return {a, b};
  }
  std::vector<gp_XYZ> points{b};
  bool blocked = false;
  for (int x = el(parent, goal); x >= 0 && x != start; x = el(parent, x)) {
    points.push_back(centre(x));
    blocked = blocked || el(state, x) != 1;
  }
  points.push_back(a);
  std::reverse(points.begin(), points.end());
  // Only the corners that are needed: from each point straight to the
  // farthest one the way to which is clear.
  std::vector<gp_XYZ> shortened{points.front()};
  for (int i = 0; i + 1 < isize(points);) {
    int next = isize(points) - 1;
    while (next > i + 1 && !segment_clear(el(points, i), el(points, next), need)) {
      --next;
    }
    shortened.push_back(el(points, next));
    i = next;
  }
  clean = !blocked;
  return shortened;
}

std::vector<gp_XYZ> FlexMesh::free_path(const gp_XYZ& a, const gp_XYZ& b, double need, bool& clean) const {
  clean = true;
  if (empty() || segment_clear(a, b, need)) {
    return {a, b};
  }
  std::vector<gp_XYZ> path = grid_path(a, b, need, 1.0, clean);
  if (!clean) {
    // Round a body larger than the grid: a wider one.
    bool wider = false;
    std::vector<gp_XYZ> again = grid_path(a, b, need, 3.0, wider);
    if (wider) {
      clean = true;
      return again;
    }
  }
  return path;
}

} // namespace mitcad::geometry::detail
