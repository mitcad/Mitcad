// SPDX-License-Identifier: MIT
#include "mitcad/geometry/trimmed_box.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>
#include <exception>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepTopAdaptor_FClass2d.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GeomAbs_Shape.hxx>
#include <GeomAbs_SurfaceType.hxx>
#include <OSD_Parallel.hxx>
#include <Precision.hxx>
#include <TopAbs_State.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Edge.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>

namespace mitcad::geometry {
namespace {

// A side of OCCT's box farther out than the boundary's by less than this
// is taken as the boundary's, with no search inside the face: a tenth of a
// nanometre, below every tolerance a measure or a test allows.
constexpr double kOpenSide = 1.0e-7;

// The fewest samples of the face's parameter grid that must lie inside the
// face before its surface is searched, and the most along each parameter
// the grid is refined to for a narrow face.
constexpr int kInsideSamples = 16;
constexpr int kMostSamples = 128;

// Sides as Bnd_Box::Get gives them: xmin, ymin, zmin, xmax, ymax, zmax.
using Sides = std::array<double, 6>;

Sides sides_of(const Bnd_Box& box) {
  Sides s{};
  box.Get(s[0], s[1], s[2], s[3], s[4], s[5]);
  return s;
}

// Samples along a parameter: four per span of the surface where it is
// twice differentiable (a B-spline's knot spans), at least 16, at most 48.
int samples_for(int spans) { return std::clamp(4 * spans, 16, 48); }

// A coordinate of the surface (times +1 or -1, so that its extreme is a
// maximum) and its first and second derivatives in the parameters.
struct Local {
  double f = 0.0;
  double gu = 0.0;
  double gv = 0.0;
  double huu = 0.0;
  double huv = 0.0;
  double hvv = 0.0;
};

// The face's surface over the rectangle of parameters its boundary spans,
// and whether a point of that rectangle lies in the face.
class FaceDomain {
public:
  explicit FaceDomain(const TopoDS_Face& face)
      : m_surface(face), m_u0(m_surface.FirstUParameter()), m_u1(m_surface.LastUParameter()),
        m_v0(m_surface.FirstVParameter()), m_v1(m_surface.LastVParameter()),
        m_classify(face, resolution(m_surface, BRep_Tool::Tolerance(face))) {}

  double u0() const { return m_u0; }
  double v0() const { return m_v0; }
  double width() const { return m_u1 - m_u0; }
  double height() const { return m_v1 - m_v0; }
  int u_spans() const { return m_surface.NbUIntervals(GeomAbs_C2); }
  int v_spans() const { return m_surface.NbVIntervals(GeomAbs_C2); }

  // Strictly inside the face: a point on its boundary (within the face's
  // tolerance) is the edges' and counted with them.
  bool inside(double u, double v) const {
    return u >= m_u0 && u <= m_u1 && v >= m_v0 && v <= m_v1 &&
           m_classify.Perform(gp_Pnt2d(u, v)) == TopAbs_IN;
  }

  gp_Pnt point(double u, double v) const { return m_surface.Value(u, v); }

  Local local(double u, double v, int axis, double sign) const {
    gp_Pnt p;
    gp_Vec du, dv, duu, dvv, duv;
    m_surface.D2(u, v, p, du, dv, duu, dvv, duv);
    const int k = axis + 1;
    return {sign * p.Coord(k),   sign * du.Coord(k),  sign * dv.Coord(k),
            sign * duu.Coord(k), sign * duv.Coord(k), sign * dvv.Coord(k)};
  }

  // The largest value of the coordinate `axis` (times `sign`) reached by
  // climbing from (u, v), a point inside the face, without leaving it:
  // Newton's steps where the surface is locally a cap, else along the
  // gradient; each step shortened until it stays inside the face and the
  // value does not fall. `cell` is a step along the gradient as a share of
  // the rectangle's sides (the sampling grid's spacing).
  double climb(double u, double v, int axis, double sign, double cell) const {
    const double ru = width();
    const double rv = height();
    Local at = local(u, v, axis, sign);
    for (int iteration = 0; iteration < 100; ++iteration) {
      // In the rectangle's own scale (both sides 1), so that the steps of
      // a long narrow rectangle are alike in both directions.
      const double ga = at.gu * ru;
      const double gb = at.gv * rv;
      const double haa = at.huu * ru * ru;
      const double hab = at.huv * ru * rv;
      const double hbb = at.hvv * rv * rv;
      const double gg = ga * ga + gb * gb;
      if (!(gg > 0.0)) {
        break;
      }
      double sa = 0.0;
      double sb = 0.0;
      const double det = haa * hbb - hab * hab;
      if (haa < 0.0 && det > 0.0) {
        sa = -(hbb * ga - hab * gb) / det;
        sb = -(haa * gb - hab * ga) / det;
      } else {
        // Along the gradient: to the top of the value's parabola on that
        // line where it bends down, else one cell.
        const double bend = ga * (haa * ga + hab * gb) + gb * (hab * ga + hbb * gb);
        const double t = bend < 0.0 ? -gg / bend : cell / std::sqrt(gg);
        sa = t * ga;
        sb = t * gb;
      }
      bool moved = false;
      for (int half = 0; half < 40; ++half) {
        const double un = u + sa * ru;
        const double vn = v + sb * rv;
        if (inside(un, vn)) {
          const Local next = local(un, vn, axis, sign);
          if (next.f >= at.f) {
            u = un;
            v = vn;
            at = next;
            moved = true;
            break;
          }
        }
        sa *= 0.5;
        sb *= 0.5;
      }
      if (!moved || std::hypot(sa, sb) < 1.0e-13) {
        break;
      }
    }
    return at.f;
  }

private:
  static double resolution(const BRepAdaptor_Surface& surface, double tolerance) {
    return std::max({surface.UResolution(tolerance), surface.VResolution(tolerance), Precision::PConfusion()});
  }

  BRepAdaptor_Surface m_surface;
  double m_u0;
  double m_u1;
  double m_v0;
  double m_v1;
  BRepTopAdaptor_FClass2d m_classify;
};

// The face's parameter grid: each sample's point and whether it lies
// inside the face.
struct Grid {
  int nu = 0;
  int nv = 0;
  std::vector<gp_Pnt> points;
  std::vector<bool> in;
  int inside = 0;

  double u(const FaceDomain& d, int i) const { return d.u0() + d.width() * (i + 0.5) / nu; }
  double v(const FaceDomain& d, int j) const { return d.v0() + d.height() * (j + 0.5) / nv; }
};

Grid sample(const FaceDomain& domain, int nu, int nv) {
  Grid grid;
  grid.nu = nu;
  grid.nv = nv;
  grid.points.resize(static_cast<std::size_t>(nu) * static_cast<std::size_t>(nv));
  grid.in.resize(grid.points.size());
  for (int i = 0; i < nu; ++i) {
    for (int j = 0; j < nv; ++j) {
      const std::size_t k = static_cast<std::size_t>(i) * static_cast<std::size_t>(nv) + static_cast<std::size_t>(j);
      const double u = grid.u(domain, i);
      const double v = grid.v(domain, j);
      grid.in[k] = domain.inside(u, v);
      if (grid.in[k]) {
        grid.points[k] = domain.point(u, v);
        ++grid.inside;
      }
    }
  }
  return grid;
}

// The largest value of the coordinate `axis` (times `sign`) over the
// face's interior: the best sample, and the tops reached by climbing from
// the samples that are highest among their neighbours inside the face.
double interior_top(const FaceDomain& domain, const Grid& grid, int axis, double sign) {
  const int k1 = axis + 1;
  auto at = [&](int i, int j) {
    return static_cast<std::size_t>(i) * static_cast<std::size_t>(grid.nv) + static_cast<std::size_t>(j);
  };
  double best = -Precision::Infinite();
  std::vector<std::pair<double, std::size_t>> starts;
  for (int i = 0; i < grid.nu; ++i) {
    for (int j = 0; j < grid.nv; ++j) {
      const std::size_t k = at(i, j);
      if (!grid.in[k]) {
        continue;
      }
      const double f = sign * grid.points[k].Coord(k1);
      best = std::max(best, f);
      bool top = true;
      for (int di = -1; di <= 1 && top; ++di) {
        for (int dj = -1; dj <= 1 && top; ++dj) {
          const int ni = i + di;
          const int nj = j + dj;
          if ((di == 0 && dj == 0) || ni < 0 || nj < 0 || ni >= grid.nu || nj >= grid.nv) {
            continue;
          }
          const std::size_t n = at(ni, nj);
          top = !grid.in[n] || sign * grid.points[n].Coord(k1) <= f;
        }
      }
      if (top) {
        starts.emplace_back(f, k);
      }
    }
  }
  // The eight highest starts: a face has few caps along an axis.
  std::sort(starts.begin(), starts.end(), [](const auto& a, const auto& b) { return a.first > b.first; });
  if (starts.size() > 8) {
    starts.resize(8);
  }
  const double cell = 1.0 / std::max(grid.nu, grid.nv);
  for (const auto& start : starts) {
    const int i = static_cast<int>(start.second / static_cast<std::size_t>(grid.nv));
    const int j = static_cast<int>(start.second % static_cast<std::size_t>(grid.nv));
    best = std::max(best, domain.climb(grid.u(domain, i), grid.v(domain, j), axis, sign, cell));
  }
  return best;
}

// Whether the surface is ruled along one of its parameters: straight along
// it, so that a coordinate is linear on each such line. Through any point
// inside a face the line runs to the face's boundary both ways, and the
// coordinate is as large at one end: the face's extremes along every axis
// lie on its boundary, and its box is its boundary's. A plane, a cylinder,
// a cone, an extrusion, a spline of one straight span along a parameter.
// The search inside such a face is skipped: on a planar face of a gear's
// 120 edges it took 7 s (each step classifies the point against every
// edge), and a design with a gear and a pulley opened in 8 s instead of
// under one (0.66.0-rc1).
bool ruled(const BRepAdaptor_Surface& surface) {
  switch (surface.GetType()) {
  case GeomAbs_Plane:
  case GeomAbs_Cylinder:
  case GeomAbs_Cone:
  case GeomAbs_SurfaceOfExtrusion:
    return true;
  case GeomAbs_BezierSurface:
  case GeomAbs_BSplineSurface:
    return (surface.UDegree() == 1 && surface.NbUPoles() == 2) ||
           (surface.VDegree() == 1 && surface.NbVPoles() == 2);
  default:
    return false;
  }
}

} // namespace

Bnd_Box trimmed_face_box(const TopoDS_Face& face) {
  Bnd_Box occt;
  BRepBndLib::AddOptimal(face, occt, false, false);
  TopLoc_Location location;
  if (occt.IsVoid() || BRep_Tool::Surface(face, location).IsNull()) {
    return occt;
  }
  // The boundary: its edges' curves, exact.
  Bnd_Box boundary;
  for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    if (!BRep_Tool::IsGeometric(edge)) {
      // An edge with no curve in space: its points are not known here.
      return occt;
    }
    BRepBndLib::AddOptimal(edge, boundary, false, false);
  }
  if (boundary.IsVoid()) {
    // A closed surface without boundary (a whole sphere): its own box.
    return occt;
  }
  const Sides outer = sides_of(occt);
  Sides result = sides_of(boundary);
  // The sides where OCCT's box reaches past the boundary: the face reaches
  // there only through an interior point where its surface turns back
  // along that axis, if at all.
  std::array<bool, 6> open{};
  bool any = false;
  for (std::size_t k = 0; k < 3; ++k) {
    open[k] = result[k] - outer[k] > kOpenSide;
    open[k + 3] = outer[k + 3] - result[k + 3] > kOpenSide;
    any = any || open[k] || open[k + 3];
  }
  if (!any) {
    return occt;
  }
  if (ruled(BRepAdaptor_Surface(face, false))) {
    // No interior extreme to search for: the boundary's sides where OCCT's
    // box reaches past them, OCCT's elsewhere.
    for (std::size_t k = 0; k < 6; ++k) {
      if (!open[k]) {
        result[k] = outer[k];
      }
    }
    Bnd_Box box;
    box.Update(result[0], result[1], result[2], result[3], result[4], result[5]);
    return box;
  }
  const FaceDomain domain(face);
  if (!(domain.width() > 0.0) || !(domain.height() > 0.0)) {
    return occt;
  }
  int nu = samples_for(domain.u_spans());
  int nv = samples_for(domain.v_spans());
  Grid grid = sample(domain, nu, nv);
  // A narrow face: a finer grid until enough samples fall inside it.
  while (grid.inside < kInsideSamples && std::max(nu, nv) < kMostSamples) {
    nu *= 2;
    nv *= 2;
    grid = sample(domain, nu, nv);
  }
  for (int axis = 0; axis < 3; ++axis) {
    const auto low = static_cast<std::size_t>(axis);
    const auto high = static_cast<std::size_t>(axis + 3);
    if (open[low] && grid.inside > 0) {
      result[low] = std::min(result[low], -interior_top(domain, grid, axis, -1.0));
    }
    if (open[high] && grid.inside > 0) {
      result[high] = std::max(result[high], interior_top(domain, grid, axis, 1.0));
    }
    // A side where OCCT's box is the boundary's within kOpenSide keeps it.
    if (!open[low]) {
      result[low] = outer[low];
    }
    if (!open[high]) {
      result[high] = outer[high];
    }
  }
  Bnd_Box box;
  box.Update(result[0], result[1], result[2], result[3], result[4], result[5]);
  return box;
}

Bnd_Box trimmed_box(const TopoDS_Shape& shape) {
  std::vector<TopoDS_Face> faces;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    faces.push_back(TopoDS::Face(it.Current()));
  }
  // On all cores: the optimal box of a spline face is a search over its
  // surface, seconds for a body of thousands of faces.
  std::vector<Bnd_Box> boxes(faces.size());
  std::vector<std::exception_ptr> errors(faces.size());
  OSD_Parallel::For(
      0, static_cast<int>(faces.size()),
      [&](int i) {
        const auto k = static_cast<std::size_t>(i);
        try {
          boxes[k] = trimmed_face_box(faces[k]);
        } catch (...) {
          errors[k] = std::current_exception();
        }
      },
      faces.size() < 8);
  Bnd_Box box;
  for (std::size_t k = 0; k < faces.size(); ++k) {
    if (errors[k]) {
      std::rethrow_exception(errors[k]);
    }
    if (!boxes[k].IsVoid()) {
      const Sides s = sides_of(boxes[k]);
      box.Update(s[0], s[1], s[2], s[3], s[4], s[5]);
    }
  }
  // The edges not in faces and the vertices not in edges, as
  // BRepBndLib::AddOptimal adds them.
  BRep_Builder builder;
  TopoDS_Compound rest;
  builder.MakeCompound(rest);
  bool any = false;
  for (TopExp_Explorer it(shape, TopAbs_EDGE, TopAbs_FACE); it.More(); it.Next()) {
    builder.Add(rest, it.Current());
    any = true;
  }
  for (TopExp_Explorer it(shape, TopAbs_VERTEX, TopAbs_EDGE); it.More(); it.Next()) {
    builder.Add(rest, it.Current());
    any = true;
  }
  if (any) {
    BRepBndLib::AddOptimal(rest, box, false, false);
  }
  return box;
}

} // namespace mitcad::geometry
