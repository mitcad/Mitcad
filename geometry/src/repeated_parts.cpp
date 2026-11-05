// SPDX-License-Identifier: MIT
#include "mitcad/geometry/repeated_parts.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>
#include <initializer_list>
#include <map>
#include <numeric>
#include <optional>
#include <tuple>
#include <utility>
#include <vector>

#include <BRepGProp.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <Standard_Failure.hxx>
#include <TopAbs.hxx>
#include <TopExp_Explorer.hxx>
#include <gp_Mat.hxx>
#include <gp_Pnt.hxx>

#include "mitcad/geometry/cancel.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

using Vec = std::array<double, 3>;
// Row-major, as Affine's linear part.
using Mat = std::array<Vec, 3>;

template <std::size_t N>
using Square = std::array<std::array<double, N>, N>;

// Principal moments closer than this part of the largest are equal: the
// body turns about an axis of symmetry (two equal) or has no axis of its
// own (three).
constexpr double kEqualMoments = 1.0e-4;
// The farthest points of a body are those within this part of its size
// (beside twice the tolerance) of the farthest one.
constexpr double kFarthest = 1.0e-6;
// At most this many frames of a candidate are laid on the first body's.
constexpr std::size_t kMostFrames = 512;
// Rounds of the least-squares fit of a frame's rotation to the points it
// matches.
constexpr int kFits = 2;
// Points are paired for the fit within this part of the body's size (or
// ten times the tolerance): a frame of principal axes is that near where
// moments are nearly equal.
constexpr double kPairing = 2.0e-2;

Vec add(const Vec& a, const Vec& b) { return {a[0] + b[0], a[1] + b[1], a[2] + b[2]}; }
Vec sub(const Vec& a, const Vec& b) { return {a[0] - b[0], a[1] - b[1], a[2] - b[2]}; }
Vec scaled(const Vec& a, double s) { return {a[0] * s, a[1] * s, a[2] * s}; }
double dot(const Vec& a, const Vec& b) { return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]; }
double length(const Vec& a) { return std::sqrt(dot(a, a)); }
Vec cross(const Vec& a, const Vec& b) {
  return {a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]};
}
Vec unit(const Vec& a) { return scaled(a, 1.0 / length(a)); }
Vec mul(const Mat& m, const Vec& v) { return {dot(m[0], v), dot(m[1], v), dot(m[2], v)}; }
Vec column(const Mat& m, std::size_t c) { return {m[0][c], m[1][c], m[2][c]}; }
// The matrix whose columns are x, y and z.
Mat of_columns(const Vec& x, const Vec& y, const Vec& z) {
  return {{{x[0], y[0], z[0]}, {x[1], y[1], z[1]}, {x[2], y[2], z[2]}}};
}
Mat times(const Mat& a, const Mat& b) {
  Mat out{};
  for (std::size_t r = 0; r < 3; ++r) {
    for (std::size_t c = 0; c < 3; ++c) {
      out[r][c] = a[r][0] * b[0][c] + a[r][1] * b[1][c] + a[r][2] * b[2][c];
    }
  }
  return out;
}
Mat transposed(const Mat& m) {
  Mat out{};
  for (std::size_t r = 0; r < 3; ++r) {
    for (std::size_t c = 0; c < 3; ++c) {
      out[r][c] = m[c][r];
    }
  }
  return out;
}
// A unit vector square to the unit vector v.
Vec perpendicular(const Vec& v) {
  const Vec other = std::abs(v[0]) < 0.9 ? Vec{1.0, 0.0, 0.0} : Vec{0.0, 1.0, 0.0};
  return unit(cross(v, other));
}
bool finite(const Vec& v) { return std::isfinite(v[0]) && std::isfinite(v[1]) && std::isfinite(v[2]); }

// The eigenvalues of a symmetric matrix in ascending order, with their
// unit eigenvectors as the columns of `vectors` in the same order (cyclic
// Jacobi rotations).
template <std::size_t N>
void eigen(Square<N> a, std::array<double, N>& values, Square<N>& vectors) {
  Square<N> v{};
  for (std::size_t i = 0; i < N; ++i) {
    v[i][i] = 1.0;
  }
  for (int sweep = 0; sweep < 64; ++sweep) {
    double off = 0.0;
    double diagonal = 0.0;
    for (std::size_t p = 0; p < N; ++p) {
      diagonal += a[p][p] * a[p][p];
      for (std::size_t q = p + 1; q < N; ++q) {
        off += a[p][q] * a[p][q];
      }
    }
    if (off <= 1.0e-32 * diagonal || off == 0.0) {
      break;
    }
    for (std::size_t p = 0; p < N; ++p) {
      for (std::size_t q = p + 1; q < N; ++q) {
        if (a[p][q] == 0.0) {
          continue;
        }
        const double theta = (a[q][q] - a[p][p]) / (2.0 * a[p][q]);
        const double t = std::abs(theta) > 1.0e150
                             ? 0.5 / theta
                             : (theta >= 0.0 ? 1.0 : -1.0) / (std::abs(theta) + std::sqrt(theta * theta + 1.0));
        const double c = 1.0 / std::sqrt(t * t + 1.0);
        const double s = t * c;
        for (std::size_t k = 0; k < N; ++k) {
          const double kp = a[k][p];
          const double kq = a[k][q];
          a[k][p] = c * kp - s * kq;
          a[k][q] = s * kp + c * kq;
        }
        for (std::size_t k = 0; k < N; ++k) {
          const double pk = a[p][k];
          const double qk = a[q][k];
          a[p][k] = c * pk - s * qk;
          a[q][k] = s * pk + c * qk;
        }
        for (std::size_t k = 0; k < N; ++k) {
          const double kp = v[k][p];
          const double kq = v[k][q];
          v[k][p] = c * kp - s * kq;
          v[k][q] = s * kp + c * kq;
        }
      }
    }
  }
  std::array<std::size_t, N> order{};
  std::iota(order.begin(), order.end(), std::size_t{0});
  std::sort(order.begin(), order.end(), [&a](std::size_t i, std::size_t j) { return a[i][i] < a[j][j]; });
  for (std::size_t k = 0; k < N; ++k) {
    values[k] = a[order[k]][order[k]];
    for (std::size_t r = 0; r < N; ++r) {
      vectors[r][k] = v[r][order[k]];
    }
  }
}

struct Point {
  Vec at{};
  // A vertex's own tolerance; none for a face centroid.
  double tolerance = 0.0;
};

// Points with their order by x, for lookups near a point.
struct Points {
  std::vector<Point> items;
  std::vector<std::pair<double, std::size_t>> by_x;

  void index() {
    by_x.clear();
    by_x.reserve(items.size());
    for (std::size_t i = 0; i < items.size(); ++i) {
      by_x.emplace_back(items[i].at[0], i);
    }
    std::sort(by_x.begin(), by_x.end());
  }

  // The nearest point within `radius` of `at` that is not taken, if any.
  std::optional<std::size_t> nearest(const Vec& at, double radius, const std::vector<char>* taken) const {
    auto it = std::lower_bound(by_x.begin(), by_x.end(), std::make_pair(at[0] - radius, std::size_t{0}));
    std::optional<std::size_t> best;
    double best_distance = radius;
    for (; it != by_x.end() && it->first <= at[0] + radius; ++it) {
      const std::size_t i = it->second;
      if (taken != nullptr && (*taken)[i] != 0) {
        continue;
      }
      const double distance = length(sub(items[i].at, at));
      if (distance <= best_distance) {
        best_distance = distance;
        best = i;
      }
    }
    return best;
  }
};

// What the comparison knows of a body.
struct Body {
  bool usable = false;
  // Measured by its volume; a body without solids by its area.
  bool solid = false;
  int faces = 0;
  int edges = 0;
  int vertices = 0;
  double volume = 0.0;
  double area = 0.0;
  Vec center{};
  // Ascending, with their axes as the columns of `axes` (right-handed).
  std::array<double, 3> moments{};
  Mat axes{};
  Points corners;
  Points centroids;
  // The distance of its farthest point from the centre.
  double size = 0.0;
  // The largest of its vertices' own tolerances.
  double vertex_tolerance = 0.0;
};

Vec point_of(const gp_Pnt& p) { return {p.X(), p.Y(), p.Z()}; }

Body measure(const Shape& shape) {
  Body body;
  const TopoDS_Shape& occt = shape.occt();
  if (occt.IsNull() || detail::has_mesh_faces(occt) || shape.face_count() == 0) {
    return body;
  }
  body.faces = shape.face_count();
  body.edges = shape.edge_count();
  body.vertices = shape.vertex_count();
  body.solid = TopExp_Explorer(occt, TopAbs_SOLID).More();
  GProp_GProps surface;
  BRepGProp::SurfaceProperties(occt, surface);
  body.area = surface.Mass();
  GProp_GProps volume;
  if (body.solid) {
    BRepGProp::VolumeProperties(occt, volume);
  }
  const GProp_GProps& props = body.solid ? volume : surface;
  body.volume = body.solid ? std::abs(volume.Mass()) : 0.0;
  // (A solid of reversed orientation measures negative.)
  const double sign = props.Mass() < 0.0 ? -1.0 : 1.0;
  body.center = point_of(props.CentreOfMass());
  const gp_Mat inertia = props.MatrixOfInertia();
  Square<3> matrix{};
  for (std::size_t r = 0; r < 3; ++r) {
    for (std::size_t c = 0; c < 3; ++c) {
      matrix[r][c] = sign * inertia.Value(static_cast<int>(r) + 1, static_cast<int>(c) + 1);
    }
  }
  Square<3> axes{};
  eigen<3>(matrix, body.moments, axes);
  const Vec x{axes[0][0], axes[1][0], axes[2][0]};
  const Vec y{axes[0][1], axes[1][1], axes[2][1]};
  body.axes = of_columns(x, y, cross(x, y));
  for (int i = 0; i < shape.vertex_count(); ++i) {
    const TopoDS_Vertex& vertex = shape.vertex(i);
    Point point;
    point.at = point_of(BRep_Tool::Pnt(vertex));
    point.tolerance = BRep_Tool::Tolerance(vertex);
    body.vertex_tolerance = std::max(body.vertex_tolerance, point.tolerance);
    body.corners.items.push_back(point);
  }
  for (int i = 0; i < shape.face_count(); ++i) {
    GProp_GProps face;
    BRepGProp::SurfaceProperties(shape.face(i), face);
    if (face.Mass() > 0.0) {
      Point point;
      point.at = point_of(face.CentreOfMass());
      if (finite(point.at)) {
        body.centroids.items.push_back(point);
      }
    }
  }
  body.corners.index();
  body.centroids.index();
  for (const Points* points : {&body.corners, &body.centroids}) {
    for (const Point& point : points->items) {
      body.size = std::max(body.size, length(sub(point.at, body.center)));
    }
  }
  if (!(body.size > 0.0)) {
    body.size = std::sqrt(std::max(body.area, 0.0));
  }
  body.usable = body.area > 0.0 && std::isfinite(body.area) && std::isfinite(body.volume) && finite(body.center) &&
                std::all_of(body.moments.begin(), body.moments.end(), [](double m) { return std::isfinite(m); }) &&
                body.moments[2] > 0.0 && body.size > 0.0;
  return body;
}

// How the principal moments of a body tell its frames.
enum class Kind {
  // Three different moments: the principal axes, with their signs.
  Distinct,
  // The two smaller ones equal: about the third axis, the directions to the
  // farthest points across it.
  AboutLargest,
  // The two larger ones equal: about the first axis, likewise.
  AboutSmallest,
  // All three equal: the directions to the farthest points, and across
  // each the directions to the farthest points across it.
  Free,
};

Kind kind_of(const Body& body) {
  const double top = body.moments[2];
  const bool low = body.moments[1] - body.moments[0] < kEqualMoments * top;
  const bool high = body.moments[2] - body.moments[1] < kEqualMoments * top;
  if (low && high) {
    return Kind::Free;
  }
  if (low) {
    return Kind::AboutLargest;
  }
  if (high) {
    return Kind::AboutSmallest;
  }
  return Kind::Distinct;
}

// Unit directions from the centre to the points farthest from it (with
// `axis`, measured square to that axis), each once; with `first`, only the
// farthest one. None when every point is within `band` of the centre
// (or of the axis).
std::vector<Vec> farthest(const Body& body, const Vec* axis, double band, bool first) {
  std::vector<std::pair<double, Vec>> offsets;
  for (const Points* points : {&body.corners, &body.centroids}) {
    for (const Point& point : points->items) {
      Vec offset = sub(point.at, body.center);
      if (axis != nullptr) {
        offset = sub(offset, scaled(*axis, dot(offset, *axis)));
      }
      offsets.emplace_back(length(offset), offset);
    }
  }
  double most = 0.0;
  for (const auto& offset : offsets) {
    most = std::max(most, offset.first);
  }
  std::vector<Vec> directions;
  if (!(most > band)) {
    return directions;
  }
  if (first) {
    for (const auto& offset : offsets) {
      if (offset.first == most) {
        directions.push_back(unit(offset.second));
        break;
      }
    }
    return directions;
  }
  for (const auto& offset : offsets) {
    if (offset.first < most - band) {
      continue;
    }
    const Vec direction = unit(offset.second);
    const bool known = std::any_of(directions.begin(), directions.end(),
                                   [&direction](const Vec& d) { return dot(d, direction) > 1.0 - 1.0e-12; });
    if (!known) {
      directions.push_back(direction);
    }
  }
  return directions;
}

// The frames of a body of the kind: rotations whose columns are the frame's
// x, y and z axes (right-handed, so a mirror image never matches). With
// `first`, only one: the one the first body of a comparison lays on the
// others' frames.
std::vector<Mat> frames(const Body& body, Kind kind, double band, bool first) {
  std::vector<Mat> out;
  const auto about = [&](const Vec& z) {
    std::vector<Vec> across = farthest(body, &z, band, first);
    if (across.empty()) {
      across.push_back(perpendicular(z));
    }
    for (const Vec& x : across) {
      out.push_back(of_columns(x, cross(z, x), z));
    }
  };
  switch (kind) {
  case Kind::Distinct: {
    const Vec x = column(body.axes, 0);
    const Vec y = column(body.axes, 1);
    for (double sx : {1.0, -1.0}) {
      for (double sy : {1.0, -1.0}) {
        const Vec fx = scaled(x, sx);
        const Vec fy = scaled(y, sy);
        out.push_back(of_columns(fx, fy, cross(fx, fy)));
        if (first) {
          return out;
        }
      }
    }
    break;
  }
  case Kind::AboutLargest:
  case Kind::AboutSmallest: {
    const Vec axis = column(body.axes, kind == Kind::AboutLargest ? 2 : 0);
    about(axis);
    if (!first) {
      about(scaled(axis, -1.0));
    }
    break;
  }
  case Kind::Free: {
    std::vector<Vec> zs = farthest(body, nullptr, band, first);
    if (zs.empty()) {
      zs.push_back({0.0, 0.0, 1.0});
    }
    for (const Vec& z : zs) {
      about(z);
      if (out.size() >= kMostFrames) {
        break;
      }
    }
    break;
  }
  }
  if (out.size() > kMostFrames) {
    out.resize(kMostFrames);
  }
  return out;
}

struct Rigid {
  Mat linear{{{1.0, 0.0, 0.0}, {0.0, 1.0, 0.0}, {0.0, 0.0, 1.0}}};
  Vec translation{};
};

Vec place(const Rigid& rigid, const Vec& p) { return add(mul(rigid.linear, p), rigid.translation); }

// The rotation and translation that move the first points of the pairs onto
// the second ones best, by least squares (Horn's quaternion method); none
// for fewer than three pairs.
std::optional<Rigid> fit(const std::vector<std::pair<Vec, Vec>>& pairs) {
  if (pairs.size() < 3) {
    return std::nullopt;
  }
  Vec ca{};
  Vec cb{};
  for (const auto& pair : pairs) {
    ca = add(ca, pair.first);
    cb = add(cb, pair.second);
  }
  const double n = static_cast<double>(pairs.size());
  ca = scaled(ca, 1.0 / n);
  cb = scaled(cb, 1.0 / n);
  Mat s{};
  for (const auto& pair : pairs) {
    const Vec a = sub(pair.first, ca);
    const Vec b = sub(pair.second, cb);
    for (std::size_t r = 0; r < 3; ++r) {
      for (std::size_t c = 0; c < 3; ++c) {
        s[r][c] += a[r] * b[c];
      }
    }
  }
  const double xx = s[0][0];
  const double xy = s[0][1];
  const double xz = s[0][2];
  const double yx = s[1][0];
  const double yy = s[1][1];
  const double yz = s[1][2];
  const double zx = s[2][0];
  const double zy = s[2][1];
  const double zz = s[2][2];
  const Square<4> horn{{{xx + yy + zz, yz - zy, zx - xz, xy - yx},
                        {yz - zy, xx - yy - zz, xy + yx, zx + xz},
                        {zx - xz, xy + yx, -xx + yy - zz, yz + zy},
                        {xy - yx, zx + xz, yz + zy, -xx - yy + zz}}};
  std::array<double, 4> values{};
  Square<4> vectors{};
  eigen<4>(horn, values, vectors);
  double w = vectors[0][3];
  double qx = vectors[1][3];
  double qy = vectors[2][3];
  double qz = vectors[3][3];
  const double norm = std::sqrt(w * w + qx * qx + qy * qy + qz * qz);
  if (!(norm > 0.0) || !std::isfinite(norm)) {
    return std::nullopt;
  }
  w /= norm;
  qx /= norm;
  qy /= norm;
  qz /= norm;
  Rigid rigid;
  rigid.linear = {{{w * w + qx * qx - qy * qy - qz * qz, 2.0 * (qx * qy - w * qz), 2.0 * (qx * qz + w * qy)},
                   {2.0 * (qy * qx + w * qz), w * w - qx * qx + qy * qy - qz * qz, 2.0 * (qy * qz - w * qx)},
                   {2.0 * (qz * qx - w * qy), 2.0 * (qz * qy + w * qx), w * w - qx * qx - qy * qy + qz * qz}}};
  rigid.translation = sub(cb, mul(rigid.linear, ca));
  return rigid;
}

// The points of `a` moved by `rigid` paired with the nearest points of `b`
// within `radius` (each point of `b` once), with the centres.
std::vector<std::pair<Vec, Vec>> pairs_of(const Body& a, const Body& b, const Rigid& rigid, double radius) {
  std::vector<std::pair<Vec, Vec>> pairs;
  pairs.emplace_back(a.center, b.center);
  const std::array<std::pair<const Points*, const Points*>, 2> sets{
      {{&a.corners, &b.corners}, {&a.centroids, &b.centroids}}};
  for (const auto& set : sets) {
    std::vector<char> taken(set.second->items.size(), 0);
    for (const Point& point : set.first->items) {
      if (const auto found = set.second->nearest(place(rigid, point.at), radius, &taken)) {
        taken[*found] = 1;
        pairs.emplace_back(point.at, set.second->items[*found].at);
      }
    }
  }
  return pairs;
}

// Whether `rigid` puts every vertex and face centroid of `a` within the
// tolerance (with the vertices' own) of one of `b`'s, each of `b`'s once.
bool covers(const Body& a, const Body& b, const Rigid& rigid, double tolerance) {
  const double faces = tolerance + a.vertex_tolerance + b.vertex_tolerance;
  {
    std::vector<char> taken(b.corners.items.size(), 0);
    for (const Point& point : a.corners.items) {
      const double radius = tolerance + point.tolerance + b.vertex_tolerance;
      const auto found = b.corners.nearest(place(rigid, point.at), radius, &taken);
      if (!found) {
        return false;
      }
      taken[*found] = 1;
    }
  }
  std::vector<char> taken(b.centroids.items.size(), 0);
  for (const Point& point : a.centroids.items) {
    const auto found = b.centroids.nearest(place(rigid, point.at), faces, &taken);
    if (!found) {
      return false;
    }
    taken[*found] = 1;
  }
  return true;
}

// Whether the measures of two bodies allow one to be a copy of the other.
bool alike(const Body& a, const Body& b, double tolerance) {
  if (!a.usable || !b.usable || a.solid != b.solid || a.faces != b.faces || a.edges != b.edges ||
      a.vertices != b.vertices || a.corners.items.size() != b.corners.items.size() ||
      a.centroids.items.size() != b.centroids.items.size()) {
    return false;
  }
  const double slack = tolerance + a.vertex_tolerance + b.vertex_tolerance;
  const double size = std::max(a.size, b.size);
  if (std::abs(a.size - b.size) > 2.0 * slack + kFarthest * size) {
    return false;
  }
  // Faces moved by the slack change the area and the volume by about so
  // much; integration differs by far less.
  if (std::abs(a.area - b.area) > 1.0e-6 * a.area + 8.0 * a.area * slack / size) {
    return false;
  }
  if (a.solid && std::abs(a.volume - b.volume) > 1.0e-6 * a.volume + 2.0 * a.area * slack) {
    return false;
  }
  const double moments = a.moments[2] * (1.0e-5 + 10.0 * slack / size);
  for (std::size_t i = 0; i < 3; ++i) {
    if (std::abs(a.moments[i] - b.moments[i]) > moments) {
      return false;
    }
  }
  return true;
}

// How `b` is `a` moved: the frame of `a` laid on each of `b`'s frames of
// the same kind, fitted to the points it pairs, until one covers.
std::optional<Rigid> match(const Body& a, const Mat& frame, Kind kind, const Body& b, double tolerance) {
  if (!alike(a, b, tolerance)) {
    return std::nullopt;
  }
  const double slack = tolerance + a.vertex_tolerance + b.vertex_tolerance;
  const double band = 2.0 * slack + kFarthest * b.size;
  const double radius = std::max(10.0 * slack, kPairing * b.size);
  const Mat back = transposed(frame);
  for (const Mat& other : frames(b, kind, band, false)) {
    Rigid laid;
    laid.linear = times(other, back);
    laid.translation = sub(b.center, mul(laid.linear, a.center));
    Rigid fitted = laid;
    for (int round = 0; round < kFits; ++round) {
      const auto better = fit(pairs_of(a, b, fitted, radius));
      if (!better) {
        break;
      }
      fitted = *better;
    }
    if (covers(a, b, fitted, tolerance)) {
      return fitted;
    }
    if (covers(a, b, laid, tolerance)) {
      return laid;
    }
  }
  return std::nullopt;
}

Affine affine_of(const Rigid& rigid) {
  Affine out;
  out.linear = rigid.linear;
  out.translation = rigid.translation;
  return out;
}

} // namespace

std::vector<RigidCopy> rigid_copies(const std::vector<const Shape*>& bodies, double tolerance) {
  detail::require_positive("the tolerance of repeated parts", tolerance);
  return detail::run("repeated parts", [&] {
    std::vector<Body> measured;
    measured.reserve(bodies.size());
    for (const Shape* shape : bodies) {
      Body body;
      if (shape != nullptr) {
        try {
          body = measure(*shape);
        } catch (const Standard_Failure&) {
          // (A body OCCT cannot measure is a copy of none.)
          body = Body{};
        }
      }
      measured.push_back(std::move(body));
    }
    std::vector<RigidCopy> copies(bodies.size());
    for (std::size_t i = 0; i < copies.size(); ++i) {
      copies[i].of = i;
    }
    // Bodies of the same topology counts, in order.
    std::map<std::tuple<bool, int, int, int>, std::vector<std::size_t>> buckets;
    for (std::size_t i = 0; i < measured.size(); ++i) {
      const Body& body = measured[i];
      if (body.usable) {
        buckets[std::make_tuple(body.solid, body.faces, body.edges, body.vertices)].push_back(i);
      }
    }
    std::vector<char> placed(measured.size(), 0);
    for (const auto& bucket : buckets) {
      const std::vector<std::size_t>& members = bucket.second;
      for (std::size_t k = 0; k < members.size(); ++k) {
        const std::size_t first = members[k];
        if (placed[first] != 0) {
          continue;
        }
        throw_if_cancelled();
        const Body& a = measured[first];
        const Kind kind = kind_of(a);
        const double band = 2.0 * (tolerance + 2.0 * a.vertex_tolerance) + kFarthest * a.size;
        const std::vector<Mat> own = frames(a, kind, band, true);
        if (own.empty()) {
          continue;
        }
        for (std::size_t m = k + 1; m < members.size(); ++m) {
          const std::size_t other = members[m];
          if (placed[other] != 0) {
            continue;
          }
          if (const auto rigid = match(a, own.front(), kind, measured[other], tolerance)) {
            copies[other].of = first;
            copies[other].placement = affine_of(*rigid);
            placed[other] = 1;
          }
        }
      }
    }
    return copies;
  });
}

} // namespace mitcad::geometry
