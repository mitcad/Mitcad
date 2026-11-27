// SPDX-License-Identifier: MIT
#include "bridge/surface_quality.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <initializer_list>
#include <limits>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepTools.hxx>
#include <BRepTopAdaptor_FClass2d.hxx>
#include <BRep_Tool.hxx>
#include <Geom_Surface.hxx>
#include <NCollection_IndexedMap.hxx>
#include <Poly_Triangulation.hxx>
#include <Precision.hxx>
#include <TopAbs_State.hxx>
#include <TopExp.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "bridge/mesh.hpp"
#include "mitcad/geometry/input_check.hpp"
#include "mitcad/io/mesh.hpp"
#include "mitcad_bridge/kernel/surface_quality.h"

namespace mitcad::bridge {
namespace {

constexpr double kNaN = std::numeric_limits<double>::quiet_NaN();

// The indices of the faces named, every face's when none is named.
std::vector<int> faces_asked(const geometry::Shape& shape, rust::Slice<const rust::String> faces) {
  std::vector<int> indices;
  if (faces.empty()) {
    for (int f = 0; f < shape.face_count(); ++f) {
      indices.push_back(f);
    }
    return indices;
  }
  for (const rust::String& name : faces) {
    const std::vector<int> found = shape.find_faces(std::string(name));
    if (found.empty()) {
      throw std::invalid_argument("the body has no face " + std::string(name));
    }
    for (const int f : found) {
      if (std::find(indices.begin(), indices.end(), f) == indices.end()) {
        indices.push_back(f);
      }
    }
  }
  return indices;
}

// +1 where the face's outward normal is its surface's D1U x D1V as the
// adaptor gives them (in the shape's placement), -1 where it is the
// reverse: a reversed face, or a mirroring placement (which turns the
// cross product of the mirrored derivatives against the mirrored normal).
double outward_sign(const TopoDS_Face& face) {
  TopLoc_Location location;
  BRep_Tool::Surface(face, location);
  const bool reversed = face.Orientation() == TopAbs_REVERSED;
  const bool mirrored = location.Transformation().IsNegative();
  return reversed != mirrored ? -1.0 : 1.0;
}

// A surface point's outward normal and principal curvatures (the larger
// first), from the first and second fundamental forms, the second taken
// against the inward normal so that a face bulging outwards curves
// positively. False where the surface has no normal there (a pole, an
// apex).
struct Principal {
  gp_Pnt point;
  gp_Vec normal;
  double k1 = 0.0;
  double k2 = 0.0;
};

bool principal_at(const BRepAdaptor_Surface& surface, double sign, double u, double v, Principal& out) {
  gp_Pnt p;
  gp_Vec du;
  gp_Vec dv;
  gp_Vec duu;
  gp_Vec dvv;
  gp_Vec duv;
  surface.D2(u, v, p, du, dv, duu, dvv, duv);
  gp_Vec n = du.Crossed(dv);
  const double length = n.Magnitude();
  const double scale = du.Magnitude() * dv.Magnitude();
  if (!(length > 1e-12 * std::max(1.0, scale))) {
    return false;
  }
  n *= sign / length;
  const double e = du.Dot(du);
  const double f = du.Dot(dv);
  const double g = dv.Dot(dv);
  const double l = -duu.Dot(n);
  const double m = -duv.Dot(n);
  const double nn = -dvv.Dot(n);
  const double det = e * g - f * f;
  if (!(det > 0.0)) {
    return false;
  }
  const double gaussian = (l * nn - m * m) / det;
  const double mean = (e * nn - 2.0 * f * m + g * l) / (2.0 * det);
  const double spread = std::sqrt(std::max(0.0, mean * mean - gaussian));
  out.point = p;
  out.normal = n;
  out.k1 = mean + spread;
  out.k2 = mean - spread;
  return std::isfinite(out.k1) && std::isfinite(out.k2);
}

// principal_at, or at points a little towards the middle of the face's
// parameter range where the surface has no normal at (u, v): a sphere's
// pole, a cone's apex.
bool principal_near(const BRepAdaptor_Surface& surface, double sign, double u, double v,
                    const std::array<double, 4>& bounds, Principal& out) {
  if (principal_at(surface, sign, u, v, out)) {
    return true;
  }
  const double mu = 0.5 * (bounds[0] + bounds[1]);
  const double mv = 0.5 * (bounds[2] + bounds[3]);
  for (const double step : {1e-6, 1e-4, 1e-2}) {
    if (principal_at(surface, sign, u + step * (mu - u), v + step * (mv - v), out)) {
      return true;
    }
  }
  return false;
}

void push3(rust::Vec<double>& out, const gp_XYZ& p) {
  out.push_back(p.X());
  out.push_back(p.Y());
  out.push_back(p.Z());
}

std::uint32_t index_of(std::size_t i) {
  if (i > std::numeric_limits<std::uint32_t>::max()) {
    throw std::runtime_error("the mesh has too many vertices");
  }
  return static_cast<std::uint32_t>(i);
}

// One face's triangulation into `mesh`, with its vertices' normals and
// curvatures; a face without a surface (a mesh body's) faceted, without
// curvatures.
void sample_face(const TopoDS_Face& face, SurfaceSampleMesh& mesh, std::size_t& vertices) {
  TopLoc_Location location;
  const occ::handle<Poly_Triangulation> triangulation = BRep_Tool::Triangulation(face, location);
  if (triangulation.IsNull()) {
    throw std::runtime_error("a face could not be triangulated");
  }
  const gp_Trsf placement = location.Transformation();
  const bool reversed = face.Orientation() == TopAbs_REVERSED;
  // The triangles follow the surface's normal: turned round where the face
  // is reversed, or mirrored by its placement (as io::face_meshes).
  const bool turned = reversed != placement.IsNegative();
  const int nodes = triangulation->NbNodes();
  const int count = triangulation->NbTriangles();
  std::vector<gp_Pnt> points(static_cast<std::size_t>(nodes));
  for (int n = 1; n <= nodes; ++n) {
    points[static_cast<std::size_t>(n - 1)] = triangulation->Node(n).Transformed(placement);
  }
  std::vector<std::array<int, 3>> corners;
  corners.reserve(static_cast<std::size_t>(count));
  for (int t = 1; t <= count; ++t) {
    int a = 0;
    int b = 0;
    int c = 0;
    triangulation->Triangle(t).Get(a, b, c);
    if (turned) {
      std::swap(b, c);
    }
    corners.push_back({a - 1, b - 1, c - 1});
  }
  const auto facet = [&points](const std::array<int, 3>& t) {
    const gp_Pnt& pa = points[static_cast<std::size_t>(t[0])];
    return gp_Vec(pa, points[static_cast<std::size_t>(t[1])]).Crossed(gp_Vec(pa, points[static_cast<std::size_t>(t[2])]));
  };
  TopLoc_Location surface_location;
  const occ::handle<Geom_Surface> geometry = BRep_Tool::Surface(face, surface_location);
  if (geometry.IsNull() || !triangulation->HasUVNodes()) {
    // Faceted: each triangle with its own vertices and normal.
    for (const std::array<int, 3>& t : corners) {
      gp_Vec normal = facet(t);
      const double length = normal.Magnitude();
      if (length > 0.0) {
        normal /= length;
      }
      const std::uint32_t first = index_of(vertices);
      for (const int node : t) {
        push3(mesh.vertices, points[static_cast<std::size_t>(node)].XYZ());
        push3(mesh.normals, normal.XYZ());
        mesh.curvatures.push_back(kNaN);
        mesh.curvatures.push_back(kNaN);
        ++vertices;
      }
      for (std::uint32_t k = 0; k < 3; ++k) {
        mesh.triangles.push_back(first + k);
      }
    }
    mesh.face_triangles.push_back(static_cast<std::uint32_t>(count));
    return;
  }
  // The normals of the face's triangles around each vertex, for a point
  // where the surface has none.
  std::vector<gp_Vec> around(static_cast<std::size_t>(nodes), gp_Vec(0.0, 0.0, 0.0));
  for (const std::array<int, 3>& t : corners) {
    const gp_Vec normal = facet(t);
    for (const int node : t) {
      around[static_cast<std::size_t>(node)] += normal;
    }
  }
  const BRepAdaptor_Surface surface(face, false);
  const double sign = outward_sign(face);
  std::array<double, 4> bounds{};
  BRepTools::UVBounds(face, bounds[0], bounds[1], bounds[2], bounds[3]);
  const std::uint32_t first = index_of(vertices);
  for (int n = 1; n <= nodes; ++n) {
    const gp_Pnt2d uv = triangulation->UVNode(n);
    Principal found;
    gp_Vec normal = around[static_cast<std::size_t>(n - 1)];
    double k1 = kNaN;
    double k2 = kNaN;
    if (principal_near(surface, sign, uv.X(), uv.Y(), bounds, found)) {
      normal = found.normal;
      k1 = found.k1;
      k2 = found.k2;
    }
    const double length = normal.Magnitude();
    if (length > 0.0) {
      normal /= length;
    }
    push3(mesh.vertices, points[static_cast<std::size_t>(n - 1)].XYZ());
    push3(mesh.normals, normal.XYZ());
    mesh.curvatures.push_back(k1);
    mesh.curvatures.push_back(k2);
    ++vertices;
  }
  for (const std::array<int, 3>& t : corners) {
    for (const int node : t) {
      mesh.triangles.push_back(first + static_cast<std::uint32_t>(node));
    }
  }
  mesh.face_triangles.push_back(static_cast<std::uint32_t>(count));
}

// A point of an iso curve: where it is, the outward normal and the normal
// curvature along the curve (NaN where the surface has no normal).
struct IsoPoint {
  gp_Pnt point;
  gp_Vec normal;
  double curvature = kNaN;
};

// The iso curves of one face: the parameter `fixed` held (u for curves
// along v, v for curves along u), the other one free.
class FaceIsos {
public:
  FaceIsos(const TopoDS_Face& face, double deviation)
      : m_surface(face, false), m_classifier(face, Precision::PConfusion()), m_sign(outward_sign(face)),
        m_deviation(deviation) {
    BRepTools::UVBounds(face, m_bounds[0], m_bounds[1], m_bounds[2], m_bounds[3]);
  }

  bool usable() const { return m_bounds[1] > m_bounds[0] && m_bounds[3] > m_bounds[2]; }

  // The parameter range of u (0) or v (1).
  double low(int which) const { return m_bounds[static_cast<std::size_t>(2 * which)]; }
  double high(int which) const { return m_bounds[static_cast<std::size_t>(2 * which + 1)]; }

  // The pieces of the curve at `fixed` inside the face, along u when
  // `along_u`: each a list of parameters of the free one.
  std::vector<std::vector<double>> pieces(bool along_u, double fixed) const {
    constexpr int kSteps = 160;
    const int moving = along_u ? 0 : 1;
    const double lo = low(moving);
    const double hi = high(moving);
    std::vector<double> at(kSteps + 1);
    std::vector<bool> in(kSteps + 1);
    for (int i = 0; i <= kSteps; ++i) {
      at[static_cast<std::size_t>(i)] = lo + (hi - lo) * i / kSteps;
      in[static_cast<std::size_t>(i)] = inside(along_u, fixed, at[static_cast<std::size_t>(i)]);
    }
    std::vector<std::vector<double>> result;
    std::size_t i = 0;
    while (i < at.size()) {
      if (!in[i]) {
        ++i;
        continue;
      }
      std::size_t j = i;
      while (j + 1 < at.size() && in[j + 1]) {
        ++j;
      }
      std::vector<double> piece;
      // The ends on the face's boundary, found between a sample in and one
      // out.
      piece.push_back(i > 0 ? boundary(along_u, fixed, at[i - 1], at[i]) : at[i]);
      for (std::size_t k = i; k <= j; ++k) {
        if (at[k] > piece.back()) {
          piece.push_back(at[k]);
        }
      }
      const double end = j + 1 < at.size() ? boundary(along_u, fixed, at[j + 1], at[j]) : at[j];
      if (end > piece.back()) {
        piece.push_back(end);
      }
      if (piece.size() >= 2) {
        result.push_back(refined(along_u, fixed, piece));
      }
      i = j + 1;
    }
    return result;
  }

  IsoPoint point(bool along_u, double fixed, double param) const {
    const double u = along_u ? param : fixed;
    const double v = along_u ? fixed : param;
    IsoPoint result;
    gp_Pnt p;
    gp_Vec du;
    gp_Vec dv;
    gp_Vec duu;
    gp_Vec dvv;
    gp_Vec duv;
    m_surface.D2(u, v, p, du, dv, duu, dvv, duv);
    result.point = p;
    gp_Vec n = du.Crossed(dv);
    const double length = n.Magnitude();
    if (length > 1e-12 * std::max(1.0, du.Magnitude() * dv.Magnitude())) {
      n *= m_sign / length;
      result.normal = n;
      const gp_Vec& tangent = along_u ? du : dv;
      const gp_Vec& second = along_u ? duu : dvv;
      const double speed = tangent.Dot(tangent);
      if (speed > 1e-24) {
        result.curvature = -second.Dot(n) / speed;
      }
    }
    return result;
  }

private:
  bool inside(bool along_u, double fixed, double param) const {
    const gp_Pnt2d uv = along_u ? gp_Pnt2d(param, fixed) : gp_Pnt2d(fixed, param);
    const TopAbs_State state = m_classifier.Perform(uv);
    return state == TopAbs_IN || state == TopAbs_ON;
  }

  // The parameter between `out` (outside the face) and `in` (inside) where
  // the curve crosses the boundary, by halving.
  double boundary(bool along_u, double fixed, double out, double in) const {
    for (int k = 0; k < 30; ++k) {
      const double middle = 0.5 * (out + in);
      if (inside(along_u, fixed, middle)) {
        in = middle;
      } else {
        out = middle;
      }
    }
    return in;
  }

  // The parameters with points added where the chord between two strays
  // more than the deviation from the curve's middle point between them.
  std::vector<double> refined(bool along_u, double fixed, const std::vector<double>& coarse) const {
    std::vector<double> result{coarse.front()};
    for (std::size_t k = 1; k < coarse.size(); ++k) {
      split(along_u, fixed, coarse[k - 1], coarse[k], 0, result);
    }
    return result;
  }

  void split(bool along_u, double fixed, double a, double b, int depth, std::vector<double>& out) const {
    const double middle = 0.5 * (a + b);
    if (depth < 8) {
      const gp_Pnt pa = point(along_u, fixed, a).point;
      const gp_Pnt pb = point(along_u, fixed, b).point;
      const gp_Pnt pm = point(along_u, fixed, middle).point;
      const gp_Pnt chord((pa.XYZ() + pb.XYZ()) * 0.5);
      if (pm.Distance(chord) > m_deviation) {
        split(along_u, fixed, a, middle, depth + 1, out);
        split(along_u, fixed, middle, b, depth + 1, out);
        return;
      }
    }
    out.push_back(b);
  }

  BRepAdaptor_Surface m_surface;
  // Classifying caches nothing, but OCCT does not promise it is const.
  mutable BRepTopAdaptor_FClass2d m_classifier;
  double m_sign;
  double m_deviation;
  std::array<double, 4> m_bounds{};
};

} // namespace

SurfaceSampleMesh surface_samples(const geometry::Shape& shape, rust::Slice<const rust::String> faces,
                                  double deviation, double angle, std::uint64_t face_triangles) {
  const geometry::CheckedOperation check("curvature of faces");
  const std::vector<int> indices = faces_asked(shape, faces);
  io::MeshOptions options;
  options.linear_deflection = deviation;
  options.angular_deflection = angle;
  options.face_triangles = static_cast<std::size_t>(face_triangles);
  // A triangulated copy: the shape keeps its own triangulations. Its faces
  // are in the shape's face order.
  const TopoDS_Shape meshed = io::triangulate(shape.occt(), options);
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> map;
  TopExp::MapShapes(meshed, TopAbs_FACE, map);
  SurfaceSampleMesh mesh;
  std::size_t vertices = 0;
  for (const int f : indices) {
    if (f + 1 > map.Extent()) {
      throw std::runtime_error("the triangulated body lost a face");
    }
    const TopoDS_Face& face = TopoDS::Face(map(f + 1));
    TopLoc_Location location;
    if (BRep_Tool::Triangulation(face, location).IsNull()) {
      const io::FaceError error = io::untriangulated_face(meshed, face);
      throw std::runtime_error(untriangulated_face_message(shape, error.index(), error.surface()));
    }
    sample_face(face, mesh, vertices);
  }
  return mesh;
}

IsoCurveSet face_isocurves(const geometry::Shape& shape, rust::Slice<const rust::String> faces, std::uint32_t count,
                           double deviation) {
  const geometry::CheckedOperation check("iso curves of faces");
  if (!(deviation > 0.0)) {
    throw std::invalid_argument("the deviation of iso curves must be greater than zero");
  }
  const std::vector<int> indices = faces_asked(shape, faces);
  IsoCurveSet set;
  for (std::size_t asked = 0; asked < indices.size(); ++asked) {
    const TopoDS_Face& face = shape.face(indices[asked]);
    TopLoc_Location location;
    if (BRep_Tool::Surface(face, location).IsNull()) {
      continue; // a mesh body's face: no parameters
    }
    const FaceIsos isos(face, deviation);
    if (!isos.usable()) {
      continue;
    }
    for (const bool along_u : {true, false}) {
      // Curves along u hold v, and the other way round.
      const int held = along_u ? 1 : 0;
      for (std::uint32_t i = 0; i < count; ++i) {
        const double at = static_cast<double>(i + 1) / static_cast<double>(count + 1);
        const double fixed = isos.low(held) + at * (isos.high(held) - isos.low(held));
        for (const std::vector<double>& piece : isos.pieces(along_u, fixed)) {
          set.faces.push_back(static_cast<std::uint32_t>(asked));
          set.along_u.push_back(along_u ? 1 : 0);
          set.at.push_back(at);
          set.counts.push_back(static_cast<std::uint32_t>(piece.size()));
          for (const double param : piece) {
            const IsoPoint p = isos.point(along_u, fixed, param);
            push3(set.points, p.point.XYZ());
            push3(set.normals, p.normal.XYZ());
            set.curvatures.push_back(p.curvature);
          }
        }
      }
    }
  }
  return set;
}

} // namespace mitcad::bridge
