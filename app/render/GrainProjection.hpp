// SPDX-License-Identifier: MIT
#pragma once

// The `grain` projection of a texture (mitcad#2186, commands.md "Texture
// projections"): each face takes the image in a plane of its own, the
// image's width (u) along the face's longest side, so a wood's grain runs
// along each panel's length. The rule is the model's
// (core/model-base/src/appearance/grain.rs, `grain_axes`), followed here
// exactly for the shaded view (OcctViewer), the render worker
// (CyclesRenderer) and the web export (WebGlb), which work on triangles:
//
// 1. the face's area, its area-weighted normal N and the second moment of
//    its area about its centroid (exact per triangle);
// 2. u and v the moment's eigenvectors of the largest and the middle
//    eigenvalue;
// 3. u's largest component positive, v turned so that u x v points along
//    N;
// 4. a face about as wide as long (middle eigenvalue more than
//    kGrainSquare of the largest) takes as u the longest of X, Y and Z
//    seen in its plane (the first on a tie), v = (u x v) x u;
// 5. a point p's grain coordinates are (p . u, p . v) in mm; the
//    texture's rotation and size apply as in the other projections;
// 6. each placed body adds its grain offset (grainOffset, mitcad#2605) to
//    its points' grain coordinates, so neighbouring bodies of one wood do
//    not read as one board.
//
// Plain C++17, header only: the render worker is compiled without the
// application.

#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <initializer_list>
#include <string>
#include <vector>

#include "render/Renderer.hpp"

namespace mitcad::render {

constexpr double kGrainSquare = 0.9;

// A face's grain directions: unit vectors in the body's coordinates.
struct GrainAxes {
  std::array<double, 3> u{1.0, 0.0, 0.0};
  std::array<double, 3> v{0.0, 1.0, 0.0};
};

namespace grain_detail {

using V3 = std::array<double, 3>;
inline V3 add(const V3& a, const V3& b) { return {a[0] + b[0], a[1] + b[1], a[2] + b[2]}; }
inline V3 sub(const V3& a, const V3& b) { return {a[0] - b[0], a[1] - b[1], a[2] - b[2]}; }
inline V3 scaled(const V3& a, double s) { return {a[0] * s, a[1] * s, a[2] * s}; }
inline double dot(const V3& a, const V3& b) { return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]; }
inline V3 cross(const V3& a, const V3& b) {
  return {a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]};
}
inline double length(const V3& a) { return std::sqrt(dot(a, a)); }

// Eigenvalues and unit eigenvectors (columns of `r`) of a symmetric 3 x 3
// matrix by cyclic Jacobi rotations, as the model's.
inline void eigen(std::array<V3, 3> a, V3& values, std::array<V3, 3>& vectors) {
  std::array<V3, 3> r{V3{1.0, 0.0, 0.0}, V3{0.0, 1.0, 0.0}, V3{0.0, 0.0, 1.0}};
  constexpr std::array<std::array<int, 2>, 3> pairs{{{0, 1}, {0, 2}, {1, 2}}};
  for (int sweep = 0; sweep < 32; ++sweep) {
    const double off = std::abs(a[0][1]) + std::abs(a[0][2]) + std::abs(a[1][2]);
    const double size = std::abs(a[0][0]) + std::abs(a[1][1]) + std::abs(a[2][2]);
    if (off <= 1e-15 * std::max(size, 1e-300)) {
      break;
    }
    for (const auto& pq : pairs) {
      const int p = pq[0];
      const int q = pq[1];
      if (std::abs(a[p][q]) < 1e-300) {
        continue;
      }
      const double theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
      double t = (std::signbit(theta) ? -1.0 : 1.0) / (std::abs(theta) + std::sqrt(theta * theta + 1.0));
      if (theta == 0.0) {
        t = 1.0;
      }
      const double c = 1.0 / std::sqrt(t * t + 1.0);
      const double s = t * c;
      for (V3& row : a) {
        const double akp = row[p];
        const double akq = row[q];
        row[p] = c * akp - s * akq;
        row[q] = s * akp + c * akq;
      }
      const V3 ap = a[p];
      const V3 aq = a[q];
      for (int k = 0; k < 3; ++k) {
        a[p][k] = c * ap[k] - s * aq[k];
        a[q][k] = s * ap[k] + c * aq[k];
      }
      for (V3& row : r) {
        const double rp = row[p];
        const double rq = row[q];
        row[p] = c * rp - s * rq;
        row[q] = s * rp + c * rq;
      }
    }
  }
  values = {a[0][0], a[1][1], a[2][2]};
  for (int k = 0; k < 3; ++k) {
    vectors[k] = {r[0][k], r[1][k], r[2][k]};
  }
}

} // namespace grain_detail

// A face's grain directions from its triangles: `count` of them,
// `triangle(i)` giving the i-th one's corners (counter-clockwise seen from
// outside, body coordinates). A face without area gives X and Y.
template <class Triangle>
GrainAxes grainAxes(std::size_t count, const Triangle& triangle) {
  using namespace grain_detail;
  double area = 0.0;
  V3 normal{0.0, 0.0, 0.0};
  V3 first{0.0, 0.0, 0.0};
  std::array<V3, 3> second{};
  for (std::size_t i = 0; i < count; ++i) {
    const std::array<V3, 3> corners = triangle(i);
    const V3& a = corners[0];
    const V3& b = corners[1];
    const V3& c = corners[2];
    const V3 n = cross(sub(b, a), sub(c, a));
    const double t = 0.5 * length(n);
    if (std::isnan(t) || t <= 0.0) {
      continue;
    }
    area += t;
    normal = add(normal, scaled(n, 0.5));
    const V3 s = add(add(a, b), c);
    first = add(first, scaled(s, t / 3.0));
    for (const V3& p : {a, b, c, s}) {
      for (int r = 0; r < 3; ++r) {
        for (int k = 0; k < 3; ++k) {
          second[r][k] += t / 12.0 * p[r] * p[k];
        }
      }
    }
  }
  GrainAxes axes;
  if (std::isnan(area) || area <= 0.0) {
    return axes;
  }
  const V3 centroid = scaled(first, 1.0 / area);
  std::array<V3, 3> moment{};
  for (int r = 0; r < 3; ++r) {
    for (int k = 0; k < 3; ++k) {
      moment[r][k] = second[r][k] / area - centroid[r] * centroid[k];
    }
  }
  V3 values{};
  std::array<V3, 3> vectors{};
  eigen(moment, values, vectors);
  std::array<int, 3> order{0, 1, 2};
  // Largest first; a stable sort keeps equal ones in their order, as the
  // model's sort does.
  std::stable_sort(order.begin(), order.end(), [&values](int x, int y) { return values[x] > values[y]; });
  V3 u = vectors[order[0]];
  V3 v = vectors[order[1]];
  int largest = 0;
  for (int k = 1; k < 3; ++k) {
    if (std::abs(u[k]) > std::abs(u[largest]) + 1e-12) {
      largest = k;
    }
  }
  if (u[largest] < 0.0) {
    u = scaled(u, -1.0);
  }
  if (dot(cross(u, v), normal) < 0.0) {
    v = scaled(v, -1.0);
  }
  if (values[order[1]] > kGrainSquare * values[order[0]]) {
    // No longest side: the longest world axis in the plane.
    const V3 w = cross(u, v);
    V3 best{0.0, 0.0, 0.0};
    double bestLength = -1.0;
    for (const V3& axis : {V3{1.0, 0.0, 0.0}, V3{0.0, 1.0, 0.0}, V3{0.0, 0.0, 1.0}}) {
      const V3 seen = sub(axis, scaled(w, dot(axis, w)));
      const double l = length(seen);
      if (bestLength < 0.0 || l > bestLength + 1e-9) {
        best = seen;
        bestLength = l;
      }
    }
    if (bestLength > 1e-9) {
      u = scaled(best, 1.0 / bestLength);
      v = cross(w, u);
    }
  }
  axes.u = u;
  axes.v = v;
  return axes;
}

// A point's grain coordinates (mm).
inline std::array<double, 2> grainPoint(const GrainAxes& axes, const std::array<double, 3>& p) {
  return {grain_detail::dot(p, axes.u), grain_detail::dot(p, axes.v)};
}

// The span (mm) a body's grain offset lies in along u and along v
// (grainOffset): several repeats of every built-in set.
constexpr double kGrainOffsetSpan = 10000.0;

// What names a placed body for its grain offset: its uid, with '@' and the
// path of occurrence uids that places it ("F3.b0@O1/O4") when an
// occurrence does; the shaded view's and the render scene's body key (the
// model's appearance::grain_key).
inline std::string grainKey(const std::string& body, const std::string& occurrence) {
  return occurrence.empty() ? body : body + "@" + occurrence;
}

// A placed body's offset into the images it lays along the grain
// (mitcad#2605), in mm along u and along v, added to each of its points'
// grain coordinates before the rotation and the repeat apply: separate
// bodies of one wood show different parts of its images, each with its
// repeat size and grain directions. It depends on the body's key
// (grainKey) only, so a body keeps it on a redraw, a move and when the
// design is reopened; all maps and all faces of the body take the same.
// The model's rule (core/model-base/src/appearance/grain.rs,
// `grain_offset`) exactly: the key's bytes by 64-bit FNV-1a, mixed by
// MurmurHash3's final step; the low 32 bits over 2^32 times
// kGrainOffsetSpan along u, the high 32 bits along v.
inline std::array<double, 2> grainOffset(const std::string& key) {
  std::uint64_t hash = 0xcbf29ce484222325ULL;
  for (const char c : key) {
    hash ^= static_cast<std::uint64_t>(static_cast<unsigned char>(c));
    hash *= 0x100000001b3ULL;
  }
  hash ^= hash >> 33;
  hash *= 0xff51afd7ed558ccdULL;
  hash ^= hash >> 33;
  hash *= 0xc4ceb9fe1a85ec53ULL;
  hash ^= hash >> 33;
  const auto low = static_cast<double>(hash & 0xffffffffULL);
  const auto high = static_cast<double>(hash >> 32);
  return {low / 4294967296.0 * kGrainOffsetSpan, high / 4294967296.0 * kGrainOffsetSpan};
}

// Whether a render body or one of its face groups lays its images along
// the grain.
inline bool alongGrain(const BodyData& body) {
  return body.material.textureGrain || std::any_of(body.faces.begin(), body.faces.end(), [](const FaceMaterial& group) {
           return group.material.textureGrain;
         });
}

// A mesh's grain coordinates (mm, two per vertex) and each vertex's face
// directions (three floats each for u and for v), face by face
// (MeshData::faceTriangles; faces keep their own vertices). A mesh without
// faceTriangles is one face. The coordinates include the body's grain
// offset (`offset`, mm: grainOffset; none for a mesh alone).
struct GrainMesh {
  std::vector<float> coordinates;
  std::vector<float> u;
  std::vector<float> v;
};

inline GrainMesh grainMeshOf(const MeshData& mesh, const std::array<double, 2>& offset = {0.0, 0.0}) {
  GrainMesh result;
  const std::size_t vertices = mesh.positions.size() / 3;
  const std::size_t triangles = mesh.indices.size() / 3;
  result.coordinates.assign(vertices * 2, 0.0f);
  result.u.assign(vertices * 3, 0.0f);
  result.v.assign(vertices * 3, 0.0f);
  const auto point = [&mesh](std::uint32_t index) {
    return std::array<double, 3>{mesh.positions[index * 3], mesh.positions[index * 3 + 1],
                                 mesh.positions[index * 3 + 2]};
  };
  std::vector<std::uint32_t> counts = mesh.faceTriangles;
  if (counts.empty()) {
    counts.push_back(static_cast<std::uint32_t>(triangles));
  }
  std::size_t start = 0;
  for (const std::uint32_t count : counts) {
    const std::size_t n = std::min<std::size_t>(count, triangles - std::min(start, triangles));
    const GrainAxes axes = grainAxes(n, [&](std::size_t i) {
      const std::size_t t = start + i;
      return std::array<std::array<double, 3>, 3>{point(mesh.indices[t * 3]), point(mesh.indices[t * 3 + 1]),
                                                  point(mesh.indices[t * 3 + 2])};
    });
    for (std::size_t t = start; t < start + n; ++t) {
      for (int corner = 0; corner < 3; ++corner) {
        const std::uint32_t index = mesh.indices[t * 3 + corner];
        if (index >= vertices) {
          continue;
        }
        const std::array<double, 2> uv = grainPoint(axes, point(index));
        result.coordinates[index * 2] = static_cast<float>(uv[0] + offset[0]);
        result.coordinates[index * 2 + 1] = static_cast<float>(uv[1] + offset[1]);
        for (int k = 0; k < 3; ++k) {
          result.u[index * 3 + k] = static_cast<float>(axes.u[k]);
          result.v[index * 3 + k] = static_cast<float>(axes.v[k]);
        }
      }
    }
    start += n;
  }
  return result;
}

} // namespace mitcad::render
