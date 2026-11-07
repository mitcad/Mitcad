// SPDX-License-Identifier: MIT
#include "mitcad/geometry/brep_import.hpp"

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <cstring>
#include <functional>
#include <map>
#include <memory>
#include <new>
#include <sstream>
#include <stdexcept>
#include <string>
#include <unordered_map>
#include <unordered_set>
#include <utility>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepCheck.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepCheck_Result.hxx>
#include <BRepCheck_Wire.hxx>
#include <BRepLib.hxx>
#include <BRep_Builder.hxx>
#include <BRep_TVertex.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <ElCLib.hxx>
#include <ElSLib.hxx>
#include <Geom2d_BSplineCurve.hxx>
#include <Geom2d_Line.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <GeomAPI_ProjectPointOnCurve.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_BoundedSurface.hxx>
#include <Geom_Circle.hxx>
#include <Geom_ConicalSurface.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Geom_Ellipse.hxx>
#include <Geom_Line.hxx>
#include <Geom_OffsetSurface.hxx>
#include <Geom_Plane.hxx>
#include <Geom_SphericalSurface.hxx>
#include <Geom_SurfaceOfLinearExtrusion.hxx>
#include <Geom_SurfaceOfRevolution.hxx>
#include <Geom_ToroidalSurface.hxx>
#include <Message_ProgressIndicator.hxx>
#include <Message_ProgressScope.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <NCollection_HArray1.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_IndexedMap.hxx>
#include <NCollection_List.hxx>
#include <Precision.hxx>
#include <ShapeBuild_ReShape.hxx>
#include <ShapeFix.hxx>
#include <ShapeFix_Edge.hxx>
#include <ShapeFix_Face.hxx>
#include <ShapeFix_Shape.hxx>
#include <ShapeFix_Shell.hxx>
#include <ShapeFix_Solid.hxx>
#include <Standard_Failure.hxx>
#include <Standard_OutOfMemory.hxx>
#include <TopAbs.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Iterator.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <TopoDS_TShape.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax1.hxx>
#include <gp_Ax2.hxx>
#include <gp_Ax3.hxx>
#include <gp_Circ.hxx>
#include <gp_Cone.hxx>
#include <gp_Dir.hxx>
#include <gp_Dir2d.hxx>
#include <gp_Elips.hxx>
#include <gp_Lin.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Sphere.hxx>
#include <gp_Torus.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "mitcad/analysis/common.hpp"
#include "blend_surfaces.hpp"
#include "mitcad/geometry/guard.hpp"
#include "util.hpp"

namespace mitcad::brep {
namespace {

constexpr int kCurveLine = 0;
constexpr int kCurveEllipse = 1;
constexpr int kCurveBSpline = 2;
constexpr int kCurveInterpolated = 3;
constexpr int kCurveOnSurface = 4;
constexpr int kCurveUnknown = 5;

constexpr int kSurfacePlane = 0;
constexpr int kSurfaceCone = 1;
constexpr int kSurfaceSphere = 2;
constexpr int kSurfaceTorus = 3;
constexpr int kSurfaceBSpline = 4;
constexpr int kSurfaceExtrusion = 5;
constexpr int kSurfaceRevolution = 6;
constexpr int kSurfaceRuled = 7;
constexpr int kSurfaceArcSweep = 8;
constexpr int kSurfaceRollingBall = 9;
constexpr int kSurfaceOffset = 10;
constexpr int kSurfaceUnknown = 11;

constexpr double kPi = 3.14159265358979323846;

// Largest tolerance ShapeFix may give to vertices and edges (mm).
constexpr double kMaxTolerance = 0.5;
// Messages kept per body.
constexpr std::size_t kMaxMessages = 30;

// Sequential reader of one curve's or surface's slice of the data arrays.
class Reader {
public:
  Reader(const Body& body, const Geometry& g)
      : m_ints(body.ints), m_reals(body.reals), m_ip(g.int_offset), m_iend(g.int_offset + g.int_count),
        m_rp(g.real_offset), m_rend(g.real_offset + g.real_count) {
    if (m_iend > m_ints.size() || m_rend > m_reals.size()) {
      throw std::runtime_error("geometry data outside the body's arrays");
    }
  }

  int integer() {
    if (m_ip >= m_iend) {
      throw std::runtime_error("geometry data too short (integers)");
    }
    return m_ints[m_ip++];
  }

  int count(int max = 10'000'000) {
    const int n = integer();
    if (n < 0 || n > max) {
      throw std::runtime_error("bad count in geometry data");
    }
    return n;
  }

  double real() {
    if (m_rp >= m_rend) {
      throw std::runtime_error("geometry data too short (reals)");
    }
    return m_reals[m_rp++];
  }

  gp_Pnt point() {
    const double x = real();
    const double y = real();
    const double z = real();
    return gp_Pnt(x, y, z);
  }

  gp_Dir dir() {
    const gp_Pnt p = point();
    return gp_Dir(p.X(), p.Y(), p.Z());
  }

private:
  const std::vector<std::int32_t>& m_ints;
  const std::vector<double>& m_reals;
  std::size_t m_ip;
  std::size_t m_iend;
  std::size_t m_rp;
  std::size_t m_rend;
};

Handle(Geom_Surface) make_surface(Reader& in, int kind);

// The depth of make_surface calls on this thread (a blend's supports and
// the surfaces of curves on surfaces are made inside), and why the
// outermost one took a rolling-ball blend's stored approximation for its
// surface (mitcad#622; empty when it did not).
thread_local int surface_depth = 0;
thread_local std::string stored_reason;

struct SurfaceDepth {
  SurfaceDepth() { ++surface_depth; }
  ~SurfaceDepth() { --surface_depth; }
  SurfaceDepth(const SurfaceDepth&) = delete;
  SurfaceDepth& operator=(const SurfaceDepth&) = delete;
};

// The start of the report message saying that a surface is a blend's
// stored approximation (read by the importers' reports).
constexpr const char* kStoredApproximation = "stored approximation of surface ";

Handle(Geom_Curve) make_curve(Reader& in, int kind) {
  switch (kind) {
  case kCurveLine: {
    const gp_Pnt origin = in.point();
    const gp_Dir dir = in.dir();
    return new Geom_Line(origin, dir);
  }
  case kCurveEllipse: {
    const gp_Pnt center = in.point();
    const gp_Dir normal = in.dir();
    const gp_Dir major_dir = in.dir();
    const double major = in.real();
    const double minor = in.real();
    const gp_Ax2 ax(center, normal, major_dir);
    if (std::abs(major - minor) <= 1e-12 * std::abs(major)) {
      return new Geom_Circle(ax, major);
    }
    return new Geom_Ellipse(ax, major, minor);
  }
  case kCurveBSpline: {
    const int degree = in.integer();
    const int nk = in.count();
    const int np = in.count();
    const bool rational = in.integer() != 0;
    const bool periodic = in.integer() != 0;
    NCollection_Array1<int> mults(1, nk);
    for (int i = 1; i <= nk; ++i) {
      mults(i) = in.integer();
    }
    NCollection_Array1<double> knots(1, nk);
    for (int i = 1; i <= nk; ++i) {
      knots(i) = in.real();
    }
    NCollection_Array1<gp_Pnt> poles(1, np);
    for (int i = 1; i <= np; ++i) {
      poles(i) = in.point();
    }
    Handle(Geom_BSplineCurve) curve;
    if (rational) {
      NCollection_Array1<double> weights(1, np);
      for (int i = 1; i <= np; ++i) {
        weights(i) = in.real();
      }
      curve = new Geom_BSplineCurve(poles, weights, knots, mults, degree);
    } else {
      curve = new Geom_BSplineCurve(poles, knots, mults, degree);
    }
    // Edges may cross the seam of a periodic curve; make the parameter wrap.
    if (periodic && curve->IsClosed()) {
      curve->SetPeriodic();
    }
    return Handle(Geom_Curve)(curve);
  }
  case kCurveInterpolated: {
    const int n = in.count();
    if (n < 2) {
      throw std::runtime_error("interpolated curve needs two points");
    }
    Handle(NCollection_HArray1<double>) params = new NCollection_HArray1<double>(1, n);
    for (int i = 1; i <= n; ++i) {
      params->SetValue(i, in.real());
    }
    Handle(NCollection_HArray1<gp_Pnt>) points = new NCollection_HArray1<gp_Pnt>(1, n);
    for (int i = 1; i <= n; ++i) {
      points->SetValue(i, in.point());
    }
    GeomAPI_Interpolate interpolate(points, params, false, 1e-9);
    interpolate.Perform();
    if (!interpolate.IsDone()) {
      throw std::runtime_error("curve interpolation failed");
    }
    return Handle(Geom_Curve)(interpolate.Curve());
  }
  case kCurveOnSurface: {
    // A surface the builder makes along a curve in its parameters (a
    // rolling-ball blend's contact tracks, mitcad#621).
    const int surface_kind = in.integer();
    const Handle(Geom_Surface) surface = make_surface(in, surface_kind);
    const int degree = in.integer();
    const int nk = in.count();
    const int np = in.count();
    const bool rational = in.integer() != 0;
    NCollection_Array1<int> mults(1, nk);
    for (int i = 1; i <= nk; ++i) {
      mults(i) = in.integer();
    }
    NCollection_Array1<double> knots(1, nk);
    for (int i = 1; i <= nk; ++i) {
      knots(i) = in.real();
    }
    NCollection_Array1<gp_Pnt2d> poles(1, np);
    for (int i = 1; i <= np; ++i) {
      const double u = in.real();
      const double v = in.real();
      poles(i) = gp_Pnt2d(u, v);
    }
    Handle(Geom2d_BSplineCurve) pcurve;
    if (rational) {
      NCollection_Array1<double> weights(1, np);
      for (int i = 1; i <= np; ++i) {
        weights(i) = in.real();
      }
      pcurve = new Geom2d_BSplineCurve(poles, weights, knots, mults, degree);
    } else {
      pcurve = new Geom2d_BSplineCurve(poles, knots, mults, degree);
    }
    return curve_on_surface(surface, pcurve);
  }
  default:
    throw std::runtime_error("unknown curve kind " + std::to_string(kind));
  }
}

// The ruled surface between two B-spline curves with the same knots:
// S(u, v) = (1 - u) from(v) + u to(v), so du x dv is the natural normal.
Handle(Geom_Surface) make_ruled(const Handle(Geom_Curve)& from, const Handle(Geom_Curve)& to) {
  const Handle(Geom_BSplineCurve) a = Handle(Geom_BSplineCurve)::DownCast(from);
  const Handle(Geom_BSplineCurve) b = Handle(Geom_BSplineCurve)::DownCast(to);
  if (a.IsNull() || b.IsNull() || a->IsRational() || b->IsRational() || a->Degree() != b->Degree() ||
      a->NbPoles() != b->NbPoles() || a->NbKnots() != b->NbKnots()) {
    throw std::runtime_error("ruled surface: the curves are not compatible B-splines");
  }
  const int nk = a->NbKnots();
  NCollection_Array1<double> vknots(1, nk);
  NCollection_Array1<int> vmults(1, nk);
  for (int i = 1; i <= nk; ++i) {
    const double ka = a->Knot(i);
    if (a->Multiplicity(i) != b->Multiplicity(i) || std::abs(ka - b->Knot(i)) > 1e-12 * (1.0 + std::abs(ka))) {
      throw std::runtime_error("ruled surface: the curves have different knots");
    }
    vknots(i) = ka;
    vmults(i) = a->Multiplicity(i);
  }
  const int np = a->NbPoles();
  NCollection_Array2<gp_Pnt> poles(1, 2, 1, np);
  for (int i = 1; i <= np; ++i) {
    poles(1, i) = a->Pole(i);
    poles(2, i) = b->Pole(i);
  }
  NCollection_Array1<double> uknots(1, 2);
  uknots(1) = 0.0;
  uknots(2) = 1.0;
  NCollection_Array1<int> umults(1, 2);
  umults(1) = 2;
  umults(2) = 2;
  return new Geom_BSplineSurface(poles, uknots, vknots, umults, vmults, 1, a->Degree());
}

// Circular arcs through the corresponding points of B-spline curves with
// the same knots: rational quadratic in u (one arc per two spans, knots
// spaced evenly in [0, 1]) with the given weights, the curves' degree in v.
Handle(Geom_Surface) make_arc_sweep(const std::vector<Handle(Geom_Curve)>& sections,
                                    const std::vector<double>& weights) {
  const std::size_t n = sections.size();
  if (n < 3 || n % 2 == 0 || weights.size() != n) {
    throw std::runtime_error("arc sweep: an odd number of sections, at least three, each with a weight");
  }
  std::vector<Handle(Geom_BSplineCurve)> curves;
  for (const Handle(Geom_Curve)& c : sections) {
    const Handle(Geom_BSplineCurve) b = Handle(Geom_BSplineCurve)::DownCast(c);
    if (b.IsNull() || b->IsRational()) {
      throw std::runtime_error("arc sweep: a section is not a polynomial B-spline");
    }
    curves.push_back(b);
  }
  const Handle(Geom_BSplineCurve)& first = curves.front();
  const int nk = first->NbKnots();
  const int np = first->NbPoles();
  NCollection_Array1<double> vknots(1, nk);
  NCollection_Array1<int> vmults(1, nk);
  for (int i = 1; i <= nk; ++i) {
    vknots(i) = first->Knot(i);
    vmults(i) = first->Multiplicity(i);
  }
  for (const Handle(Geom_BSplineCurve)& b : curves) {
    if (b->Degree() != first->Degree() || b->NbPoles() != np || b->NbKnots() != nk) {
      throw std::runtime_error("arc sweep: the sections are not compatible B-splines");
    }
    for (int i = 1; i <= nk; ++i) {
      if (b->Multiplicity(i) != vmults(i) || std::abs(b->Knot(i) - vknots(i)) > 1e-12 * (1.0 + std::abs(vknots(i)))) {
        throw std::runtime_error("arc sweep: the sections have different knots");
      }
    }
  }
  const int nu = static_cast<int>(n);
  NCollection_Array2<gp_Pnt> poles(1, nu, 1, np);
  NCollection_Array2<double> pole_weights(1, nu, 1, np);
  for (int i = 1; i <= nu; ++i) {
    for (int j = 1; j <= np; ++j) {
      poles(i, j) = curves[static_cast<std::size_t>(i - 1)]->Pole(j);
      pole_weights(i, j) = weights[static_cast<std::size_t>(i - 1)];
    }
  }
  const int arcs = (nu - 1) / 2;
  NCollection_Array1<double> uknots(1, arcs + 1);
  NCollection_Array1<int> umults(1, arcs + 1);
  for (int k = 0; k <= arcs; ++k) {
    uknots(k + 1) = static_cast<double>(k) / arcs;
    umults(k + 1) = k == 0 || k == arcs ? 3 : 2;
  }
  return new Geom_BSplineSurface(poles, pole_weights, uknots, vknots, umults, vmults, 2, first->Degree());
}

Handle(Geom_Surface) make_surface(Reader& in, int kind) {
  const SurfaceDepth depth;
  switch (kind) {
  case kSurfacePlane: {
    const gp_Pnt origin = in.point();
    const gp_Dir normal = in.dir();
    const gp_Dir u_dir = in.dir();
    return new Geom_Plane(gp_Ax3(origin, normal, u_dir));
  }
  case kSurfaceCone: {
    const gp_Pnt origin = in.point();
    const gp_Dir axis = in.dir();
    const gp_Dir ref = in.dir();
    const double radius = in.real();
    const double half_angle = in.real();
    const gp_Ax3 frame(origin, axis, ref);
    // A cone whose half angle is zero within the angular precision is the
    // cylinder it parametrizes the same way: a file may keep a thread's
    // cylinder as a cone of 1e-13 rad, its apex 1e14 mm away, on which
    // OCCT's intersections with other faces of revolution about its axis
    // lose their curves (mitcad#389).
    if (std::abs(half_angle) <= Precision::Angular()) {
      return new Geom_CylindricalSurface(frame, radius);
    }
    return new Geom_ConicalSurface(frame, half_angle, radius);
  }
  case kSurfaceSphere: {
    const gp_Pnt center = in.point();
    const gp_Dir axis = in.dir();
    const gp_Dir ref = in.dir();
    return new Geom_SphericalSurface(gp_Ax3(center, axis, ref), in.real());
  }
  case kSurfaceTorus: {
    const gp_Pnt center = in.point();
    const gp_Dir axis = in.dir();
    const gp_Dir ref = in.dir();
    const double major = in.real();
    const double minor = in.real();
    return new Geom_ToroidalSurface(gp_Ax3(center, axis, ref), major, minor);
  }
  case kSurfaceBSpline: {
    const int ud = in.integer();
    const int vd = in.integer();
    const int nuk = in.count();
    const int nvk = in.count();
    const int nu = in.count();
    const int nv = in.count();
    const bool rational = in.integer() != 0;
    const bool u_periodic = in.integer() != 0;
    const bool v_periodic = in.integer() != 0;
    NCollection_Array1<int> umults(1, nuk);
    for (int i = 1; i <= nuk; ++i) {
      umults(i) = in.integer();
    }
    NCollection_Array1<int> vmults(1, nvk);
    for (int i = 1; i <= nvk; ++i) {
      vmults(i) = in.integer();
    }
    NCollection_Array1<double> uknots(1, nuk);
    for (int i = 1; i <= nuk; ++i) {
      uknots(i) = in.real();
    }
    NCollection_Array1<double> vknots(1, nvk);
    for (int i = 1; i <= nvk; ++i) {
      vknots(i) = in.real();
    }
    NCollection_Array2<gp_Pnt> poles(1, nu, 1, nv);
    for (int iu = 1; iu <= nu; ++iu) {
      for (int iv = 1; iv <= nv; ++iv) {
        poles(iu, iv) = in.point();
      }
    }
    Handle(Geom_BSplineSurface) surface;
    if (rational) {
      NCollection_Array2<double> weights(1, nu, 1, nv);
      for (int iu = 1; iu <= nu; ++iu) {
        for (int iv = 1; iv <= nv; ++iv) {
          weights(iu, iv) = in.real();
        }
      }
      surface = new Geom_BSplineSurface(poles, weights, uknots, vknots, umults, vmults, ud, vd);
    } else {
      surface = new Geom_BSplineSurface(poles, uknots, vknots, umults, vmults, ud, vd);
    }
    // A face may cross the seam of a periodic surface (it then has an edge
    // on the seam from one side only), also of one the file keeps closed
    // but not periodic (mitcad#499: left unwrapped, its parameter curves
    // past the seam put the face on the surface's extrapolation, of
    // kilometres); make the parameter wrap.
    if (u_periodic && surface->IsUClosed()) {
      surface->SetUPeriodic();
    }
    if (v_periodic && surface->IsVClosed()) {
      surface->SetVPeriodic();
    }
    return Handle(Geom_Surface)(surface);
  }
  case kSurfaceExtrusion: {
    const gp_Dir dir = in.dir();
    const int curve_kind = in.integer();
    return new Geom_SurfaceOfLinearExtrusion(make_curve(in, curve_kind), dir);
  }
  case kSurfaceRevolution: {
    const gp_Pnt origin = in.point();
    const gp_Dir axis = in.dir();
    const int curve_kind = in.integer();
    return new Geom_SurfaceOfRevolution(make_curve(in, curve_kind), gp_Ax1(origin, axis));
  }
  case kSurfaceRuled: {
    const int from_kind = in.integer();
    const Handle(Geom_Curve) from = make_curve(in, from_kind);
    const int to_kind = in.integer();
    const Handle(Geom_Curve) to = make_curve(in, to_kind);
    return make_ruled(from, to);
  }
  case kSurfaceArcSweep: {
    const int n = in.count(1000);
    std::vector<double> weights(static_cast<std::size_t>(n));
    for (double& w : weights) {
      w = in.real();
    }
    std::vector<Handle(Geom_Curve)> sections;
    for (int i = 0; i < n; ++i) {
      const int section_kind = in.integer();
      sections.push_back(make_curve(in, section_kind));
    }
    return make_arc_sweep(sections, weights);
  }
  case kSurfaceRollingBall: {
    // A rolling-ball blend stored without its approximation (mitcad#621).
    const double radius = in.real();
    const double v0 = in.real();
    const double v1 = in.real();
    const double n0 = in.real();
    const double n1 = in.real();
    BlendSupport supports[2];
    for (BlendSupport& support : supports) {
      const bool is_curve = in.integer() != 0;
      const int support_kind = in.integer();
      if (is_curve) {
        support.curve = make_curve(in, support_kind);
      } else {
        support.surface = make_surface(in, support_kind);
      }
    }
    const int spine_kind = in.integer();
    const Handle(Geom_Curve) spine = make_curve(in, spine_kind);
    // The file's approximation, the surface where OCCT cannot walk the ball
    // (mitcad#622: kept where it is too far off the blend); the report says
    // so, for the surface of a face.
    Handle(Geom_Surface) stored;
    if (in.integer() != 0) {
      const int stored_kind = in.integer();
      stored = make_surface(in, stored_kind);
    }
    if (stored.IsNull()) {
      return rolling_ball_surface(supports[0], supports[1], spine, radius, v0, v1, n0, n1);
    }
    try {
      return rolling_ball_surface(supports[0], supports[1], spine, radius, v0, v1, n0, n1);
    } catch (const std::exception& e) {
      if (surface_depth == 1) {
        stored_reason = e.what();
      }
      return stored;
    }
  }
  case kSurfaceOffset: {
    // An offset surface stored without its approximation
    // (mitcad_private#198): the basis moved along its normal.
    const double distance = in.real();
    const int basis_kind = in.integer();
    const Handle(Geom_Surface) basis = make_surface(in, basis_kind);
    return new Geom_OffsetSurface(basis, distance);
  }
  default:
    throw std::runtime_error("unknown surface kind " + std::to_string(kind));
  }
}

std::string failure_text(const Standard_Failure& e) {
  const char* text = e.what();
  return text != nullptr && *text != '\0' ? std::string("OCCT: ") + text : std::string("OCCT failure");
}

void note(BuildReport& report, std::string message) {
  if (report.messages.size() < kMaxMessages) {
    report.messages.push_back(std::move(message));
  }
}

// Builds geometry objects, recording failures; null handles for those.
// With `needed`, only those it marks.
template <class Handle, class Make>
std::vector<Handle> build_all(const Body& body, const std::vector<Geometry>& items, const char* what, int& failed,
                              BuildReport& report, Make make, const std::vector<bool>* needed = nullptr) {
  std::vector<Handle> out(items.size());
  for (std::size_t i = 0; i < items.size(); ++i) {
    if (needed != nullptr && !(*needed)[i]) {
      continue;
    }
    try {
      Reader in(body, items[i]);
      out[i] = make(in, items[i].kind, i);
    } catch (const Standard_Failure& e) {
      ++failed;
      note(report, std::string(what) + " " + std::to_string(i) + ": " + failure_text(e));
    } catch (const std::exception& e) {
      ++failed;
      note(report, std::string(what) + " " + std::to_string(i) + ": " + e.what());
    }
  }
  return out;
}

int count_of(const TopoDS_Shape& shape, TopAbs_ShapeEnum type) {
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> map;
  TopExp::MapShapes(shape, type, map);
  return map.Extent();
}

// BRepCheck problems of the shape and its sub-shapes, counted by kind:
// "EDGE: BRepCheck_InvalidSameParameterFlag x2".
std::vector<std::string> check_problems(const BRepCheck_Analyzer& analyzer, const TopoDS_Shape& shape) {
  std::map<std::string, int> counts;
  const std::pair<TopAbs_ShapeEnum, const char*> kinds[] = {
      {TopAbs_VERTEX, "VERTEX"}, {TopAbs_EDGE, "EDGE"},   {TopAbs_WIRE, "WIRE"},
      {TopAbs_FACE, "FACE"},     {TopAbs_SHELL, "SHELL"}, {TopAbs_SOLID, "SOLID"}};
  auto add = [&](const std::string& kind, const NCollection_List<BRepCheck_Status>& statuses) {
    for (const BRepCheck_Status s : statuses) {
      if (s != BRepCheck_NoError) {
        std::ostringstream text;
        BRepCheck::Print(s, text);
        std::string name = text.str();
        name.erase(std::remove(name.begin(), name.end(), '\n'), name.end());
        ++counts[kind + ": " + name];
      }
    }
  };
  // Faces are named with their surface type.
  auto face_kind = [](const TopoDS_Shape& face) {
    static const char* const names[] = {"plane",  "cylinder",   "cone",      "sphere",    "torus",  "bezier",
                                        "bspline", "revolution", "extrusion", "offset",    "other"};
    const int type = static_cast<int>(BRepAdaptor_Surface(TopoDS::Face(face), false).GetType());
    return std::string("FACE (") + (type >= 0 && type < 11 ? names[type] : "?") + ")";
  };
  for (const auto& [type, kind] : kinds) {
    NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> map;
    TopExp::MapShapes(shape, type, map);
    for (int i = 1; i <= map.Extent(); ++i) {
      const Handle(BRepCheck_Result)& result = analyzer.Result(map(i));
      if (result.IsNull()) {
        continue;
      }
      const std::string name = type == TopAbs_FACE ? face_kind(map(i)) : std::string(kind);
      add(name, result->Status());
      for (result->InitContextIterator(); result->MoreShapeInContext(); result->NextShapeInContext()) {
        const TopoDS_Shape& context = result->ContextualShape();
        add(context.ShapeType() == TopAbs_FACE ? name + " on " + face_kind(context) : name, result->StatusOnShape());
      }
    }
  }
  std::vector<std::string> out;
  for (const auto& [name, n] : counts) {
    out.push_back(name + " x" + std::to_string(n));
  }
  return out;
}

// Edges on circles and ellipses are kept inside the curve's period
// [0, 2 pi]: BRepCheck clips pcurves to it when it looks for intersecting
// wires, so that an edge at negative parameters seemed to cross other
// wires. The range is shifted by whole periods; when the edge then crosses
// the seam, a circle is turned to start at the edge, an ellipse by half a
// turn (its axes must stay). Returns the curve to use (a copy if turned).
Handle(Geom_Curve) normalize_conic(const Handle(Geom_Curve)& curve, double& t0, double& t1) {
  const double period = 2.0 * kPi;
  constexpr double eps = 1e-9;
  const Handle(Geom_Circle) circle = Handle(Geom_Circle)::DownCast(curve);
  const Handle(Geom_Ellipse) ellipse = Handle(Geom_Ellipse)::DownCast(curve);
  if (circle.IsNull() && ellipse.IsNull()) {
    return curve;
  }
  const double shift = std::floor((t0 + eps) / period) * period;
  t0 -= shift;
  t1 -= shift;
  if (t1 <= period + eps) {
    return curve;
  }
  if (!circle.IsNull()) {
    gp_Circ c = circle->Circ();
    c.Rotate(c.Axis(), t0);
    t1 -= t0;
    t0 = 0.0;
    return new Geom_Circle(c);
  }
  if (t0 >= kPi - eps) {
    gp_Elips e = ellipse->Elips();
    e.Rotate(e.Axis(), kPi);
    t0 -= kPi;
    t1 -= kPi;
    return new Geom_Ellipse(e);
  }
  return curve;
}

// The parameter v of a cone's apex, a sphere's pole or a pole of a torus or
// a surface of revolution at `p`, if `p` is one (within `tol`).
bool singular_v(const Handle(Geom_Surface)& surface, const gp_Pnt& p, double tol, double& v) {
  if (const Handle(Geom_ConicalSurface) cone = Handle(Geom_ConicalSurface)::DownCast(surface); !cone.IsNull()) {
    const gp_Cone c = cone->Cone();
    v = -c.RefRadius() / std::sin(c.SemiAngle());
    return c.Apex().Distance(p) <= tol;
  }
  if (const Handle(Geom_SphericalSurface) sphere = Handle(Geom_SphericalSurface)::DownCast(surface);
      !sphere.IsNull()) {
    const gp_Sphere s = sphere->Sphere();
    double u = 0.0;
    ElSLib::Parameters(s, p, u, v);
    v = v >= 0.0 ? kPi / 2 : -kPi / 2;
    return ElSLib::Value(0.0, v, s).Distance(p) <= tol;
  }
  // A pole of a torus whose minor radius is larger than its major one: where
  // its meridian circle meets the axis.
  if (const Handle(Geom_ToroidalSurface) torus = Handle(Geom_ToroidalSurface)::DownCast(surface);
      !torus.IsNull()) {
    const gp_Torus t = torus->Torus();
    if (t.MinorRadius() <= t.MajorRadius() || gp_Lin(t.Axis()).Distance(p) > tol) {
      return false;
    }
    const double height = gp_Vec(t.Location(), p).Dot(gp_Vec(t.Axis().Direction()));
    v = std::atan2(height, -t.MajorRadius());
    return ElSLib::Value(0.0, v, t).Distance(p) <= tol;
  }
  // A pole of a surface of revolution (of a lemon torus): where its
  // meridian meets the axis.
  if (const Handle(Geom_SurfaceOfRevolution) revolution = Handle(Geom_SurfaceOfRevolution)::DownCast(surface);
      !revolution.IsNull()) {
    if (gp_Lin(revolution->Axis()).Distance(p) > tol) {
      return false;
    }
    GeomAPI_ProjectPointOnCurve projection(p, revolution->BasisCurve());
    if (projection.NbPoints() == 0 || projection.LowerDistance() > tol) {
      return false;
    }
    v = projection.LowerDistanceParameter();
    return true;
  }
  return false;
}

// The copy of a pole's parameter v of a surface of revolution or a torus,
// periodic in v, that a face at v_face reaches along its meridian (u = 0)
// without crossing the axis: the one below v_face or the one above it.
double pole_beyond(const Handle(Geom_Surface)& surface, double v, double v_face) {
  gp_Ax1 axis_of;
  if (const Handle(Geom_SurfaceOfRevolution) revolution = Handle(Geom_SurfaceOfRevolution)::DownCast(surface);
      !revolution.IsNull()) {
    axis_of = revolution->Axis();
  } else if (const Handle(Geom_ToroidalSurface) torus = Handle(Geom_ToroidalSurface)::DownCast(surface);
             !torus.IsNull()) {
    axis_of = torus->Torus().Axis();
  } else {
    return v;
  }
  const double period = surface->VPeriod();
  const double below = v + std::floor((v_face - v) / period) * period;
  const gp_Lin axis(axis_of);
  // The side of the axis a meridian point is on, in the meridian's plane.
  const auto side = [&](double t) {
    const gp_Pnt p = surface->Value(0.0, t);
    const gp_Pnt foot = ElCLib::Value(ElCLib::Parameter(axis, p), axis);
    return gp_Vec(foot, p);
  };
  const gp_Vec face_side = side(v_face);
  return side(0.5 * (below + v_face)).Dot(face_side) > 0.0 ? below : below + period;
}

// A degenerated edge closing the face at a singular point of its surface:
// its pcurve runs along the full u period at that v, so that the face (on
// the side of v_face) lies on its left.
TopoDS_Edge singular_edge(const TopoDS_Face& face, const TopoDS_Vertex& vertex, double v, double v_face,
                          double tol) {
  BRep_Builder builder;
  TopoDS_Edge edge;
  builder.MakeEdge(edge);
  builder.Add(edge, vertex.Oriented(TopAbs_FORWARD));
  builder.Add(edge, vertex.Oriented(TopAbs_REVERSED));
  const bool above = v > v_face;
  const Handle(Geom2d_Line) line =
      new Geom2d_Line(gp_Pnt2d(above ? 2.0 * kPi : 0.0, v), gp_Dir2d(above ? -1.0 : 1.0, 0.0));
  builder.UpdateEdge(edge, line, face, tol);
  builder.Range(edge, 0.0, 2.0 * kPi);
  builder.Degenerated(edge, true);
  return edge;
}

// The volume by the analysis library's integrals, as the queries report it
// (detail::volume_of in util.hpp is OCCT's own integration).
double analysed_volume_of(const TopoDS_Shape& shape) { return analysis::volume_properties(shape).Mass(); }

// The vertices, edges and faces of a body by their index in its data.
struct Parts {
  std::vector<TopoDS_Vertex> vertices;
  std::vector<TopoDS_Edge> edges;
  // Edges taken as they are (copies of healed ones), per edge of the data:
  // the edges it became (several where healing split it), in its
  // direction; empty where the edge is built.
  std::vector<std::vector<TopoDS_Edge>> chains;
  std::vector<TopoDS_Face> faces;
};

// The body's topology from its data. `preset`: parts taken as they are in
// place of building them (null entries are built); `built`: the parts the
// shape is made of.
TopoDS_Shape build_topology(const Body& body, const BuildOptions& options, BuildReport& report, bool& all_solid,
                            const Parts* preset = nullptr, Parts* built = nullptr) {
  // Only the geometry of the parts built.
  std::vector<bool> curve_needed;
  std::vector<bool> surface_needed;
  if (preset != nullptr) {
    curve_needed.assign(body.curves.size(), false);
    surface_needed.assign(body.surfaces.size(), false);
    for (std::size_t i = 0; i < body.edges.size(); ++i) {
      if (preset->chains[i].empty() && body.edges[i].curve < curve_needed.size()) {
        curve_needed[body.edges[i].curve] = true;
      }
    }
    for (std::size_t i = 0; i < body.faces.size(); ++i) {
      if (preset->faces[i].IsNull() && body.faces[i].surface < surface_needed.size()) {
        surface_needed[body.faces[i].surface] = true;
      }
    }
  }
  // Curves and surfaces the reader could not decode stay null without
  // counting as failed (the reader's issues say so; mitcad_private#198), and
  // so do their edges and faces.
  const auto curves = build_all<Handle(Geom_Curve)>(
      body, body.curves, "curve", report.curves_failed, report,
      [](Reader& in, int kind, std::size_t) {
        return kind == kCurveUnknown ? Handle(Geom_Curve)() : make_curve(in, kind);
      },
      preset != nullptr ? &curve_needed : nullptr);
  const auto unknown_curve = [&body](std::uint32_t curve) {
    return curve < body.curves.size() && body.curves[curve].kind == kCurveUnknown;
  };
  const auto unknown_surface = [&body](std::uint32_t surface) {
    return surface < body.surfaces.size() && body.surfaces[surface].kind == kSurfaceUnknown;
  };
  const auto surfaces = build_all<Handle(Geom_Surface)>(
      body, body.surfaces, "surface", report.surfaces_failed, report,
      [&report](Reader& in, int kind, std::size_t i) {
        if (kind == kSurfaceUnknown) {
          return Handle(Geom_Surface)();
        }
        stored_reason.clear();
        Handle(Geom_Surface) s = make_surface(in, kind);
        if (!stored_reason.empty()) {
          // Said whatever else the report holds (one per surface).
          report.messages.push_back(kStoredApproximation + std::to_string(i) + ": " + stored_reason);
        }
        return s;
      },
      preset != nullptr ? &surface_needed : nullptr);
  const double tol = options.tolerance;
  BRep_Builder builder;

  if (body.vertices.size() % 4 != 0) {
    throw std::runtime_error("vertex data is not a multiple of 4 values");
  }
  std::vector<TopoDS_Vertex> vertices(body.vertices.size() / 4);
  for (std::size_t i = 0; i < vertices.size(); ++i) {
    if (preset != nullptr && !preset->vertices[i].IsNull()) {
      vertices[i] = preset->vertices[i];
      continue;
    }
    const double* v = &body.vertices[4 * i];
    builder.MakeVertex(vertices[i], gp_Pnt(v[0], v[1], v[2]), std::max(v[3], tol));
  }

  std::vector<TopoDS_Edge> edges(body.edges.size());
  for (std::size_t i = 0; i < body.edges.size(); ++i) {
    if (preset != nullptr && !preset->chains[i].empty()) {
      continue;
    }
    const Edge& e = body.edges[i];
    if (unknown_curve(e.curve)) {
      continue;
    }
    if (e.curve >= curves.size() || curves[e.curve].IsNull() || e.v0 >= vertices.size() ||
        e.v1 >= vertices.size()) {
      ++report.edges_failed;
      continue;
    }
    try {
      double t0 = e.t0;
      double t1 = e.t1;
      const Handle(Geom_Curve) curve = normalize_conic(curves[e.curve], t0, t1);
      TopoDS_Edge edge;
      builder.MakeEdge(edge, curve, std::max(e.tolerance, tol));
      TopoDS_Vertex v0 = vertices[e.v0];
      TopoDS_Vertex v1 = vertices[e.v1];
      builder.Add(edge, v0.Oriented(TopAbs_FORWARD));
      builder.Add(edge, v1.Oriented(TopAbs_REVERSED));
      builder.Range(edge, t0, t1);
      // Let the vertex tolerances cover the gaps to the curve ends.
      for (const auto& [vertex, t] : {std::pair{v0, t0}, std::pair{v1, t1}}) {
        const double gap = curve->Value(t).Distance(BRep_Tool::Pnt(vertex));
        if (gap > BRep_Tool::Tolerance(vertex)) {
          builder.UpdateVertex(vertex, gap * 1.01);
        }
      }
      edges[i] = edge;
    } catch (const Standard_Failure& ex) {
      ++report.edges_failed;
      note(report, "edge " + std::to_string(i) + ": " + failure_text(ex));
    }
  }

  std::vector<TopoDS_Face> faces(body.faces.size());
  std::vector<bool> surface_used(surfaces.size(), false);
  for (std::size_t i = 0; i < body.faces.size(); ++i) {
    if (preset != nullptr && !preset->faces[i].IsNull()) {
      faces[i] = preset->faces[i];
      continue;
    }
    const Face& f = body.faces[i];
    if (unknown_surface(f.surface)) {
      continue;
    }
    if (f.surface >= surfaces.size() || surfaces[f.surface].IsNull()) {
      ++report.faces_failed;
      continue;
    }
    // Faces on one closed surface get a copy each: an edge's curve on a
    // surface is kept once per surface, and an edge on the seam that two
    // faces share needs it at u (or v) 0 for one and at the period for
    // the other.
    Handle(Geom_Surface) surface = surfaces[f.surface];
    if (surface_used[f.surface] && (surface->IsUClosed() || surface->IsVClosed())) {
      surface = Handle(Geom_Surface)::DownCast(surface->Copy());
    }
    surface_used[f.surface] = true;
    TopoDS_Face face;
    builder.MakeFace(face, surface, tol);
    // No loops on a bounded surface (a B-spline): the whole surface, with
    // its seams. (Healing bounds surfaces periodic both ways so.)
    const bool bounded = !Handle(Geom_BoundedSurface)::DownCast(surface).IsNull();
    if (f.loop_count == 0 && f.point_loop_count == 0 && bounded && !(surface->IsUPeriodic() && surface->IsVPeriodic())) {
      BRepBuilderAPI_MakeFace whole(surface, tol);
      if (whole.IsDone()) {
        face = whole.Face();
      }
    }
    bool complete = true;
    int wires = 0;
    for (std::uint32_t l = f.first_loop; l < f.first_loop + f.loop_count && l < body.loops.size(); ++l) {
      const Loop& lp = body.loops[l];
      TopoDS_Wire wire;
      builder.MakeWire(wire);
      int added = 0;
      for (std::uint32_t c = lp.first_coedge; c < lp.first_coedge + lp.coedge_count && c < body.coedges.size(); ++c) {
        const Coedge& ce = body.coedges[c];
        const std::vector<TopoDS_Edge>* chain =
            preset != nullptr && ce.edge < preset->chains.size() && !preset->chains[ce.edge].empty()
                ? &preset->chains[ce.edge]
                : nullptr;
        if (chain == nullptr && (ce.edge >= edges.size() || edges[ce.edge].IsNull())) {
          complete = false;
          continue;
        }
        // The face is built forward on its surface and reversed at the end,
        // so its edges are stored relative to the surface normal.
        TopAbs_Orientation o = ce.forward ? TopAbs_FORWARD : TopAbs_REVERSED;
        if (f.reversed) {
          o = TopAbs::Reverse(o);
        }
        if (chain == nullptr) {
          builder.Add(wire, edges[ce.edge].Oriented(o));
        } else if (o == TopAbs_FORWARD) {
          for (const TopoDS_Edge& piece : *chain) {
            builder.Add(wire, piece);
          }
        } else {
          for (auto piece = chain->rbegin(); piece != chain->rend(); ++piece) {
            builder.Add(wire, piece->Reversed());
          }
        }
        ++added;
      }
      if (added > 0) {
        wire.Closed(complete);
        builder.Add(face, wire);
        ++wires;
      }
    }
    // A face whose loops all lost every edge would be its whole surface,
    // unbounded on a plane or a cylinder (mitcad#589: a body of such faces
    // came in as a solid of infinite planes): it is not built.
    if (f.loop_count > 0 && wires == 0) {
      ++report.faces_failed;
      note(report, "face " + std::to_string(i) + ": none of its loops could be built");
      continue;
    }
    // Point loops: a degenerated edge at the apex of a cone or the pole of
    // a sphere or of a surface of revolution, on the side of the face's
    // other vertices.
    for (std::uint32_t k = f.first_point_loop;
         k < f.first_point_loop + f.point_loop_count && k < body.point_loops.size(); ++k) {
      const std::uint32_t vi = body.point_loops[k];
      double v = 0.0;
      double v_face = 0.0;
      bool other = false;
      for (TopExp_Explorer ex(face, TopAbs_VERTEX); ex.More() && !other && vi < vertices.size(); ex.Next()) {
        const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(ex.Current()));
        if (p.Distance(BRep_Tool::Pnt(vertices[vi])) > 10.0 * tol) {
          GeomAPI_ProjectPointOnSurf projection(p, surface);
          if (projection.NbPoints() > 0) {
            double u = 0.0;
            projection.LowerDistanceParameters(u, v_face);
            other = true;
          }
        }
      }
      if (!other || !singular_v(surface, BRep_Tool::Pnt(vertices[vi]), 10.0 * tol, v)) {
        note(report, "face " + std::to_string(i) + ": point loop not at a cone apex or a pole");
        continue;
      }
      // On a surface periodic in v (the revolution of a circle) the face
      // reaches the pole along the meridian without crossing the axis.
      if (surface->IsVPeriodic()) {
        v = pole_beyond(surface, v, v_face);
      }
      TopoDS_Wire wire;
      builder.MakeWire(wire);
      builder.Add(wire, singular_edge(face, vertices[vi], v, v_face, tol));
      wire.Closed(true);
      builder.Add(face, wire);
    }
    if (!complete) {
      note(report, "face " + std::to_string(i) + " has edges that could not be built");
    }
    if (f.reversed) {
      face.Reverse();
    }
    faces[i] = face;
  }

  if (built != nullptr) {
    built->vertices = vertices;
    built->edges = edges;
    built->faces = faces;
    built->chains = preset != nullptr ? preset->chains : std::vector<std::vector<TopoDS_Edge>>(edges.size());
  }

  // Lumps: solids when all their shells are closed, otherwise shells.
  std::vector<std::vector<TopoDS_Shell>> lump_shells(body.lump_count);
  std::vector<bool> lump_closed(body.lump_count, true);
  for (const Shell& s : body.shells) {
    if (s.lump >= body.lump_count) {
      throw std::runtime_error("shell refers to a missing lump");
    }
    TopoDS_Shell shell;
    builder.MakeShell(shell);
    int added = 0;
    for (std::uint32_t k = s.first_face; k < s.first_face + s.face_count && k < body.shell_faces.size(); ++k) {
      const std::uint32_t fi = body.shell_faces[k];
      if (fi < faces.size() && !faces[fi].IsNull()) {
        builder.Add(shell, faces[fi]);
        ++added;
      } else {
        lump_closed[s.lump] = false;
      }
    }
    if (added == 0) {
      continue;
    }
    shell.Closed(s.closed);
    if (!s.closed) {
      lump_closed[s.lump] = false;
    }
    lump_shells[s.lump].push_back(shell);
  }
  std::vector<TopoDS_Shape> parts;
  all_solid = body.lump_count > 0;
  for (std::uint32_t l = 0; l < body.lump_count; ++l) {
    if (lump_shells[l].empty()) {
      all_solid = false;
      continue;
    }
    if (lump_closed[l]) {
      TopoDS_Solid solid;
      builder.MakeSolid(solid);
      for (const TopoDS_Shell& s : lump_shells[l]) {
        builder.Add(solid, s);
      }
      parts.push_back(solid);
    } else {
      all_solid = false;
      for (const TopoDS_Shell& s : lump_shells[l]) {
        parts.push_back(s);
      }
    }
  }
  if (parts.size() == 1) {
    return parts.front();
  }
  TopoDS_Compound compound;
  builder.MakeCompound(compound);
  for (const TopoDS_Shape& p : parts) {
    builder.Add(compound, p);
  }
  return compound;
}

TopoDS_Shape apply_transform(const TopoDS_Shape& shape, const std::vector<double>& m) {
  if (m.empty()) {
    return shape;
  }
  if (m.size() != 12) {
    throw std::runtime_error("transform needs 12 values");
  }
  gp_Trsf trsf;
  trsf.SetValues(m[0], m[1], m[2], m[9], m[3], m[4], m[5], m[10], m[6], m[7], m[8], m[11]);
  return shape.Moved(TopLoc_Location(trsf));
}

// Orients every solid so that its material is inside; returns the result.
TopoDS_Shape orient_solids(const TopoDS_Shape& shape) {
  if (shape.ShapeType() == TopAbs_SOLID) {
    TopoDS_Solid solid = TopoDS::Solid(shape);
    BRepLib::OrientClosedSolid(solid);
    return solid;
  }
  if (shape.ShapeType() != TopAbs_COMPOUND) {
    return shape;
  }
  BRep_Builder builder;
  TopoDS_Compound out;
  builder.MakeCompound(out);
  for (TopoDS_Iterator it(shape); it.More(); it.Next()) {
    builder.Add(out, orient_solids(it.Value()));
  }
  return out;
}

// OCCT's progress of the build passed on to BuildOptions::progress, each
// time it has advanced by a ten-thousandth of the work (ShapeFix advances
// face by face). It never asks to stop.
class Advance final : public Message_ProgressIndicator {
public:
  explicit Advance(const std::function<void()>& progress) : m_progress(progress) {}

protected:
  void Show(const Message_ProgressScope&, const bool) override {
    // (Called under the indicator's lock, never at once.)
    const double position = GetPosition();
    if (position >= m_shown + kStep) {
      m_shown = position;
      m_progress();
    }
  }

private:
  static constexpr double kStep = 1e-4;
  const std::function<void()>& m_progress;
  double m_shown = 0.0;
};

// Whether two wires of a face share a vertex.
bool has_touching_wires(const TopoDS_Shape& shape) {
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> seen;
    for (TopoDS_Iterator w(f.Current()); w.More(); w.Next()) {
      NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> own;
      TopExp::MapShapes(w.Value(), TopAbs_VERTEX, own);
      for (int i = 1; i <= own.Extent(); ++i) {
        if (seen.Contains(own(i))) {
          return true;
        }
      }
      for (int i = 1; i <= own.Extent(); ++i) {
        seen.Add(own(i));
      }
    }
  }
  return false;
}

// Whether an edge bounds more than two faces of a shell (non-manifold).
bool has_crowded_edges(const TopoDS_Shape& shape) {
  for (TopExp_Explorer s(shape, TopAbs_SHELL); s.More(); s.Next()) {
    NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces;
    TopExp::MapShapesAndUniqueAncestors(s.Current(), TopAbs_EDGE, TopAbs_FACE, faces);
    for (int i = 1; i <= faces.Extent(); ++i) {
      if (faces(i).Extent() > 2) {
        return true;
      }
    }
  }
  return false;
}

// ShapeFix's healing of the built shape; `intersecting_wires`: with its
// repair of wires that intersect; `orient_shells`: shells oriented by
// ShapeFix, which splits them at edges of more than two of their faces;
// `context`: set to what it replaced, when given.
TopoDS_Shape heal(const TopoDS_Shape& shape, const BuildOptions& options, bool intersecting_wires,
                  bool orient_shells, Handle(ShapeBuild_ReShape)* context = nullptr) {
  Handle(ShapeFix_Shape) fix = new ShapeFix_Shape(shape);
  fix->SetPrecision(options.tolerance);
  fix->SetMaxTolerance(kMaxTolerance);
  // Keep the face orientations of the model, so that the raw volume
  // shows whether they are right; solids are oriented afterwards.
  // (Wires inside a face may still be reoriented.)
  fix->FixShellTool()->FixOrientationMode() = 0;
  fix->FixSolidTool()->FixShellOrientationMode() = 0;
  fix->FixSolidTool()->CreateOpenSolidMode() = false;
  if (!intersecting_wires) {
    fix->FixFaceTool()->FixIntersectingWiresMode() = 0;
  }
  if (orient_shells) {
    fix->FixShellTool()->FixOrientationMode() = 1;
    fix->FixSolidTool()->FixShellOrientationMode() = 1;
  }
  occ::handle<Message_ProgressIndicator> advance;
  if (options.progress) {
    advance = new Advance(options.progress);
  }
  fix->Perform(Message_ProgressIndicator::Start(advance));
  geometry::detail::fail_occt_allocation_if_asked("heal");
  if (context != nullptr) {
    *context = fix->Context();
  }
  return fix->Shape();
}

// The largest tolerance (mm) `widen_cusps` gives a vertex.
constexpr double kMaxCuspTolerance = 0.05;

// Two edges of a loop that leave their shared vertex in the same direction
// (a cusp: an arc tangent to a short arc that turns back along it) stay
// within the tolerance of each other for a while beyond the vertex, and
// OCCT then takes the loop for self-intersecting. The tolerance of such a
// vertex is raised (doubled, up to kMaxCuspTolerance) until its edges no
// longer cross outside it; a vertex whose edges still cross at that
// tolerance keeps its own. The number of vertices raised, and the largest
// tolerance given in `widest`.
int widen_cusps(const TopoDS_Shape& shape, double& widest) {
  NCollection_IndexedDataMap<TopoDS_Shape, double, TopTools_ShapeMapHasher> raised; // vertex -> its own
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face& face = TopoDS::Face(f.Current());
    for (TopExp_Explorer w(face, TopAbs_WIRE); w.More(); w.Next()) {
      for (int round = 0; round < 64; ++round) {
        Handle(BRepCheck_Wire) check = new BRepCheck_Wire(TopoDS::Wire(w.Current()));
        TopoDS_Edge e1;
        TopoDS_Edge e2;
        if (check->SelfIntersect(face, e1, e2, false) != BRepCheck_SelfIntersectingWire || e1.IsNull() ||
            e2.IsNull()) {
          break;
        }
        TopoDS_Vertex shared;
        if (!TopExp::CommonVertex(e1, e2, shared)) {
          break;
        }
        const double tolerance = 2.0 * BRep_Tool::Tolerance(shared);
        if (tolerance > kMaxCuspTolerance) {
          // Not a cusp: the vertex keeps its own tolerance.
          if (const double* own = raised.Seek(shared)) {
            Handle(BRep_TVertex)::DownCast(shared.TShape())->Tolerance(*own);
            raised.RemoveKey(shared);
          }
          break;
        }
        if (!raised.Contains(shared)) {
          raised.Add(shared, BRep_Tool::Tolerance(shared));
        }
        BRep_Builder().UpdateVertex(shared, tolerance);
      }
    }
  }
  for (int i = 1; i <= raised.Extent(); ++i) {
    widest = std::max(widest, BRep_Tool::Tolerance(TopoDS::Vertex(raised.FindKey(i))));
  }
  return raised.Extent();
}

} // namespace

// Reusing what the builds of a body's other states healed (mitcad#181).

namespace reuse {

// The content of a part's data: two independent 64-bit hashes.
struct Key {
  std::uint64_t a = 0;
  std::uint64_t b = 0;
  bool operator==(const Key& other) const { return a == other.a && b == other.b; }
};

struct KeyHash {
  std::size_t operator()(const Key& k) const { return static_cast<std::size_t>(k.a ^ (k.b >> 1)); }
};

std::uint64_t mix(std::uint64_t x) {
  x ^= x >> 30;
  x *= 0xbf58476d1ce4e5b9ULL;
  x ^= x >> 27;
  x *= 0x94d049bb133111ebULL;
  x ^= x >> 31;
  return x;
}

class Hasher {
public:
  void add(std::uint64_t v) {
    m_a = mix(m_a ^ v);
    m_b = mix(m_b + v * 0x9e3779b97f4a7c15ULL + 0x632be59bd9b4e019ULL);
  }
  void add(double d) {
    // (The same for 0 and -0.)
    std::uint64_t u = 0;
    if (d != 0.0) {
      std::memcpy(&u, &d, sizeof u);
    }
    add(u);
  }
  void add(const Key& k) {
    add(k.a);
    add(k.b);
  }
  Key key() const { return Key{m_a, m_b}; }

private:
  std::uint64_t m_a = 0xcbf29ce484222325ULL;
  std::uint64_t m_b = 0x84222325cbf29ce4ULL;
};

// The keys of a body's parts by index; a face whose data refers to parts
// that are not there has none (`face_ok`).
struct BodyKeys {
  std::vector<Key> vertices;
  std::vector<Key> edges;
  std::vector<Key> faces;
  std::vector<bool> face_ok;
};

Key geometry_key(const Body& body, const Geometry& g) {
  Hasher h;
  h.add(static_cast<std::uint64_t>(static_cast<std::int64_t>(g.kind)));
  h.add(static_cast<std::uint64_t>(g.int_count));
  for (std::uint32_t i = 0; i < g.int_count && g.int_offset + i < body.ints.size(); ++i) {
    h.add(static_cast<std::uint64_t>(static_cast<std::int64_t>(body.ints[g.int_offset + i])));
  }
  h.add(static_cast<std::uint64_t>(g.real_count));
  for (std::uint32_t i = 0; i < g.real_count && g.real_offset + i < body.reals.size(); ++i) {
    h.add(body.reals[g.real_offset + i]);
  }
  return h.key();
}

BodyKeys body_keys(const Body& body) {
  BodyKeys keys;
  keys.vertices.resize(body.vertices.size() / 4);
  for (std::size_t i = 0; i < keys.vertices.size(); ++i) {
    Hasher h;
    for (int k = 0; k < 4; ++k) {
      h.add(body.vertices[4 * i + k]);
    }
    keys.vertices[i] = h.key();
  }
  std::vector<Key> curves(body.curves.size());
  for (std::size_t i = 0; i < curves.size(); ++i) {
    curves[i] = geometry_key(body, body.curves[i]);
  }
  keys.edges.resize(body.edges.size());
  std::vector<bool> edge_ok(body.edges.size(), true);
  for (std::size_t i = 0; i < body.edges.size(); ++i) {
    const Edge& e = body.edges[i];
    if (e.curve >= curves.size() || e.v0 >= keys.vertices.size() || e.v1 >= keys.vertices.size()) {
      edge_ok[i] = false;
      continue;
    }
    Hasher h;
    h.add(curves[e.curve]);
    h.add(keys.vertices[e.v0]);
    h.add(keys.vertices[e.v1]);
    h.add(e.t0);
    h.add(e.t1);
    h.add(e.tolerance);
    keys.edges[i] = h.key();
  }
  keys.faces.resize(body.faces.size());
  keys.face_ok.assign(body.faces.size(), false);
  for (std::size_t i = 0; i < body.faces.size(); ++i) {
    const Face& f = body.faces[i];
    if (f.surface >= body.surfaces.size()) {
      continue;
    }
    Hasher h;
    h.add(geometry_key(body, body.surfaces[f.surface]));
    h.add(static_cast<std::uint64_t>(f.reversed));
    h.add(static_cast<std::uint64_t>(f.double_sided));
    h.add(static_cast<std::uint64_t>(f.loop_count));
    bool ok = true;
    for (std::uint32_t l = f.first_loop; l < f.first_loop + f.loop_count; ++l) {
      if (l >= body.loops.size()) {
        ok = false;
        break;
      }
      const Loop& lp = body.loops[l];
      h.add(static_cast<std::uint64_t>(lp.coedge_count));
      for (std::uint32_t c = lp.first_coedge; c < lp.first_coedge + lp.coedge_count; ++c) {
        if (c >= body.coedges.size() || body.coedges[c].edge >= keys.edges.size() ||
            !edge_ok[body.coedges[c].edge]) {
          ok = false;
          break;
        }
        h.add(keys.edges[body.coedges[c].edge]);
        h.add(static_cast<std::uint64_t>(body.coedges[c].forward));
      }
    }
    h.add(static_cast<std::uint64_t>(f.point_loop_count));
    for (std::uint32_t k = f.first_point_loop; k < f.first_point_loop + f.point_loop_count; ++k) {
      if (k >= body.point_loops.size() || body.point_loops[k] >= keys.vertices.size()) {
        ok = false;
        break;
      }
      h.add(keys.vertices[body.point_loops[k]]);
    }
    keys.faces[i] = h.key();
    keys.face_ok[i] = ok;
  }
  return keys;
}

// How many times each key occurs.
std::unordered_map<Key, int, KeyHash> counts(const std::vector<Key>& keys) {
  std::unordered_map<Key, int, KeyHash> out;
  out.reserve(keys.size());
  for (const Key& k : keys) {
    ++out[k];
  }
  return out;
}

using TShapes = std::unordered_set<const TopoDS_TShape*>;

// The TShapes of a shape's sub-shapes of a type.
TShapes tshapes(const TopoDS_Shape& shape, TopAbs_ShapeEnum type) {
  TShapes out;
  for (TopExp_Explorer ex(shape, type); ex.More(); ex.Next()) {
    out.insert(ex.Current().TShape().get());
  }
  return out;
}

// The edges (of the body's data) and vertices a face's data refers to.
void face_parts(const Body& body, std::size_t face, std::vector<std::uint32_t>& edges,
                std::vector<std::uint32_t>& vertices) {
  const Face& f = body.faces[face];
  for (std::uint32_t l = f.first_loop; l < f.first_loop + f.loop_count && l < body.loops.size(); ++l) {
    const Loop& lp = body.loops[l];
    for (std::uint32_t c = lp.first_coedge; c < lp.first_coedge + lp.coedge_count && c < body.coedges.size(); ++c) {
      const std::uint32_t e = body.coedges[c].edge;
      if (e < body.edges.size()) {
        edges.push_back(e);
        vertices.push_back(body.edges[e].v0);
        vertices.push_back(body.edges[e].v1);
      }
    }
  }
  for (std::uint32_t k = f.first_point_loop; k < f.first_point_loop + f.point_loop_count && k < body.point_loops.size();
       ++k) {
    vertices.push_back(body.point_loops[k]);
  }
}

} // namespace reuse

struct Healed {
  // The body built (it keeps the parts below alive).
  TopoDS_Shape shape;
  std::vector<double> transform;
  double tolerance = 0.0;
  // Its faces that healing kept (as built, before the body's transform),
  // and their edges and vertices, by the keys of their data (an edge as
  // the edges healing made of it, in its direction: several where it split
  // it); parts whose key the body has more than once are left out.
  std::unordered_map<reuse::Key, TopoDS_Face, reuse::KeyHash> faces;
  std::unordered_map<reuse::Key, std::vector<TopoDS_Edge>, reuse::KeyHash> edges;
  std::unordered_map<reuse::Key, TopoDS_Vertex, reuse::KeyHash> vertices;
  // The keys of all its faces (for the trace of why few are shared).
  std::unordered_set<reuse::Key, reuse::KeyHash> all_faces;
  // Faces not kept, for the trace: healing split or removed them, they are
  // not in the body, their edges or their vertices are others.
  std::array<int, 6> lost{};
};

namespace {

// What healing put in place of `s` (`context`: what it replaced; null:
// nothing), following replacements of replacements; `s` itself when it was
// kept, null when it was removed.
TopoDS_Shape healed_value(const Handle(ShapeBuild_ReShape)& context, const TopoDS_Shape& s) {
  TopoDS_Shape value = s;
  if (context.IsNull()) {
    return value;
  }
  for (int i = 0; i < 16 && !value.IsNull() && context->IsRecorded(value); ++i) {
    const TopoDS_Shape next = context->Value(value);
    if (next.IsEqual(value)) {
      break;
    }
    value = next;
  }
  return value;
}

// The edges healing made of `piece`, in its direction: itself or what it
// put in its place, or the edges it split it into, in order along it
// (pieces of its curve); none when it removed it or made other edges of it.
std::vector<TopoDS_Edge> healed_pieces(const Handle(ShapeBuild_ReShape)& context, const TopoDS_Edge& piece) {
  const TopoDS_Shape value = healed_value(context, piece);
  if (value.IsNull()) {
    return {};
  }
  if (value.ShapeType() == TopAbs_EDGE) {
    return {TopoDS::Edge(value)};
  }
  TopLoc_Location location;
  double first = 0.0;
  double last = 0.0;
  const Handle(Geom_Curve) curve = BRep_Tool::Curve(piece, location, first, last);
  if (curve.IsNull()) {
    return {};
  }
  std::vector<std::pair<double, TopoDS_Edge>> split;
  for (TopExp_Explorer ex(value, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(ex.Current());
    TopLoc_Location own;
    double from = 0.0;
    double to = 0.0;
    if (BRep_Tool::Curve(edge, own, from, to) != curve || !(own == location)) {
      return {};
    }
    split.emplace_back(from, TopoDS::Edge(edge.Oriented(TopAbs_FORWARD)));
  }
  std::sort(split.begin(), split.end(), [](const auto& a, const auto& b) { return a.first < b.first; });
  std::vector<TopoDS_Edge> out;
  for (const auto& entry : split) {
    out.push_back(entry.second);
  }
  if (piece.Orientation() == TopAbs_REVERSED) {
    std::reverse(out.begin(), out.end());
    for (TopoDS_Edge& edge : out) {
      edge.Reverse();
    }
  }
  return out;
}

// What a body built from `parts` keeps for its other states' builds: the
// faces of its data that healing left one face whose edges are what it made
// of the face's edges (`context`: what it replaced), as they are in the
// healed shape before its solids were oriented (`unoriented`), with their
// edges and vertices; `shape`, the body, keeps them alive.
std::shared_ptr<const Healed> keep_healed(const Body& body, const reuse::BodyKeys& keys, const Parts& parts,
                                          const TopoDS_Shape& unoriented, const TopoDS_Shape& shape,
                                          const Handle(ShapeBuild_ReShape)& context, const BuildOptions& options) {
  auto healed = std::make_shared<Healed>();
  healed->shape = shape;
  healed->transform = body.transform;
  healed->tolerance = options.tolerance;
  healed->all_faces.insert(keys.faces.begin(), keys.faces.end());
  std::unordered_map<const TopoDS_TShape*, TopoDS_Face> in_shape;
  for (TopExp_Explorer ex(unoriented, TopAbs_FACE); ex.More(); ex.Next()) {
    in_shape.emplace(ex.Current().TShape().get(), TopoDS::Face(ex.Current()));
  }
  // What healing made of each edge and vertex of the data (empty, null:
  // nothing usable).
  std::vector<std::vector<TopoDS_Edge>> edges(parts.edges.size());
  for (std::size_t i = 0; i < parts.edges.size(); ++i) {
    std::vector<TopoDS_Edge> pieces;
    if (i < parts.chains.size() && !parts.chains[i].empty()) {
      pieces = parts.chains[i];
    } else if (!parts.edges[i].IsNull()) {
      pieces.push_back(parts.edges[i]);
    } else {
      continue;
    }
    for (const TopoDS_Edge& piece : pieces) {
      const std::vector<TopoDS_Edge> made = healed_pieces(context, piece);
      if (made.empty()) {
        edges[i].clear();
        ++healed->lost[5];
        break;
      }
      edges[i].insert(edges[i].end(), made.begin(), made.end());
    }
    if (edges[i].size() > pieces.size()) {
      ++healed->lost[4];
    }
  }
  std::vector<TopoDS_Shape> vertices(parts.vertices.size());
  for (std::size_t i = 0; i < parts.vertices.size(); ++i) {
    if (!parts.vertices[i].IsNull()) {
      const TopoDS_Shape value = healed_value(context, parts.vertices[i]);
      if (!value.IsNull() && value.ShapeType() == TopAbs_VERTEX) {
        vertices[i] = value;
      }
    }
  }
  const auto face_counts = reuse::counts(keys.faces);
  reuse::TShapes edges_kept;
  reuse::TShapes vertices_kept;
  for (std::size_t i = 0; i < parts.faces.size(); ++i) {
    if (!keys.face_ok[i] || parts.faces[i].IsNull() || face_counts.at(keys.faces[i]) != 1) {
      continue;
    }
    const TopoDS_Shape value = healed_value(context, parts.faces[i]);
    if (value.IsNull() || value.ShapeType() != TopAbs_FACE) {
      ++healed->lost[0];
      continue;
    }
    const auto found = in_shape.find(value.TShape().get());
    if (found == in_shape.end()) {
      ++healed->lost[1];
      continue;
    }
    const TopoDS_Face& face = found->second;
    // Its edges (but the degenerated ones at singular points, which come
    // with the face) are what healing made of its data's edges, and its
    // data's vertices are among its vertices.
    std::vector<std::uint32_t> data_edges;
    std::vector<std::uint32_t> data_vertices;
    reuse::face_parts(body, i, data_edges, data_vertices);
    reuse::TShapes expected;
    bool whole = true;
    for (const std::uint32_t e : data_edges) {
      if (e >= edges.size() || edges[e].empty()) {
        whole = false;
        break;
      }
      for (const TopoDS_Edge& piece : edges[e]) {
        expected.insert(piece.TShape().get());
      }
    }
    // (Nor the seams healing added to a face on a closed surface, which
    // only the face has: thousands of round holes in a body.)
    reuse::TShapes actual;
    for (TopExp_Explorer ex(face, TopAbs_EDGE); whole && ex.More(); ex.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(ex.Current());
      const bool own_seam = expected.count(edge.TShape().get()) == 0 && BRep_Tool::IsClosed(edge, face);
      if (!BRep_Tool::Degenerated(edge) && !own_seam) {
        actual.insert(edge.TShape().get());
      }
    }
    if (!whole || actual != expected) {
      ++healed->lost[2];
      continue;
    }
    const reuse::TShapes face_vertices = reuse::tshapes(face, TopAbs_VERTEX);
    for (const std::uint32_t v : data_vertices) {
      if (v >= vertices.size() || vertices[v].IsNull() || face_vertices.count(vertices[v].TShape().get()) == 0) {
        whole = false;
        break;
      }
    }
    if (!whole) {
      ++healed->lost[3];
      continue;
    }
    healed->faces.emplace(keys.faces[i], face);
    edges_kept.insert(expected.begin(), expected.end());
    vertices_kept.insert(face_vertices.begin(), face_vertices.end());
  }
  const auto edge_counts = reuse::counts(keys.edges);
  for (std::size_t i = 0; i < edges.size(); ++i) {
    if (!edges[i].empty() && edge_counts.at(keys.edges[i]) == 1 &&
        std::all_of(edges[i].begin(), edges[i].end(),
                    [&](const TopoDS_Edge& piece) { return edges_kept.count(piece.TShape().get()) > 0; })) {
      healed->edges.emplace(keys.edges[i], edges[i]);
    }
  }
  const auto vertex_counts = reuse::counts(keys.vertices);
  for (std::size_t i = 0; i < vertices.size(); ++i) {
    if (!vertices[i].IsNull() && vertex_counts.at(keys.vertices[i]) == 1 &&
        vertices_kept.count(vertices[i].TShape().get()) > 0) {
      healed->vertices.emplace(keys.vertices[i], TopoDS::Vertex(vertices[i]));
    }
  }
  return healed;
}

// What a build kept, for the report.
std::string kept_text(const Healed& healed) {
  return "kept " + std::to_string(healed.faces.size()) + " faces healed (not: " + std::to_string(healed.lost[0]) +
         " split or removed, " + std::to_string(healed.lost[1]) + " not in the body, " +
         std::to_string(healed.lost[2]) + " other edges, " + std::to_string(healed.lost[3]) +
         " other vertices; edges: " + std::to_string(healed.lost[4]) + " split, " + std::to_string(healed.lost[5]) +
         " not kept)";
}

// The parts of `body` taken from the base that shares the most faces with
// it, as copies (the base's shapes are shared by other builds and the
// replay, and healing the other faces changes the edges and vertices they
// share with them): false when it is less than half of the faces.
bool reused_parts(const Body& body, const reuse::BodyKeys& keys, const BuildOptions& options, Parts& preset,
                  std::vector<bool>& reused, std::string& why) {
  const auto face_counts = reuse::counts(keys.faces);
  const auto edge_counts = reuse::counts(keys.edges);
  const auto vertex_counts = reuse::counts(keys.vertices);
  // A face is taken when its data is the same as a kept face's, and its
  // edges and vertices are kept and once each in this body (so the faces
  // healed here meet it in them).
  const auto takes = [&](const Healed& base, std::size_t i) {
    if (!keys.face_ok[i] || face_counts.at(keys.faces[i]) != 1 || base.faces.count(keys.faces[i]) == 0) {
      return false;
    }
    std::vector<std::uint32_t> edges;
    std::vector<std::uint32_t> vertices;
    reuse::face_parts(body, i, edges, vertices);
    for (const std::uint32_t e : edges) {
      if (edge_counts.at(keys.edges[e]) != 1 || base.edges.count(keys.edges[e]) == 0) {
        return false;
      }
    }
    for (const std::uint32_t v : vertices) {
      if (v >= keys.vertices.size() || vertex_counts.at(keys.vertices[v]) != 1 ||
          base.vertices.count(keys.vertices[v]) == 0) {
        return false;
      }
    }
    return true;
  };
  const Healed* best = nullptr;
  std::vector<bool> best_taken;
  std::size_t best_count = 0;
  for (const auto& base : options.bases) {
    if (!base || base->transform != body.transform || base->tolerance != options.tolerance) {
      continue;
    }
    std::vector<bool> taken(body.faces.size(), false);
    std::size_t count = 0;
    for (std::size_t i = 0; i < body.faces.size(); ++i) {
      if (takes(*base, i)) {
        taken[i] = true;
        ++count;
      }
    }
    if (count > best_count) {
      best = base.get();
      best_taken = std::move(taken);
      best_count = count;
    }
  }
  if (best == nullptr || 2 * best_count < body.faces.size()) {
    // (For the trace: how many have the data of a face of a base, and how
    // many faces that base kept.)
    std::size_t same = 0;
    std::size_t kept = 0;
    for (const auto& base : options.bases) {
      std::size_t n = 0;
      for (const reuse::Key& k : keys.faces) {
        n += base->all_faces.count(k);
      }
      if (n >= same) {
        same = n;
        kept = base->faces.size();
      }
    }
    why = std::to_string(best_count) + " of " + std::to_string(body.faces.size()) + " faces shared (" +
          std::to_string(same) + " the same as a base's, which kept " + std::to_string(kept) + ")";
    return false;
  }
  // The faces taken, copied with their edges and vertices.
  BRep_Builder builder;
  TopoDS_Compound taken;
  builder.MakeCompound(taken);
  for (std::size_t i = 0; i < body.faces.size(); ++i) {
    if (best_taken[i]) {
      builder.Add(taken, best->faces.at(keys.faces[i]));
    }
  }
  BRepBuilderAPI_Copy copy(taken, true, false);
  const reuse::TShapes taken_edges = reuse::tshapes(taken, TopAbs_EDGE);
  const reuse::TShapes taken_vertices = reuse::tshapes(taken, TopAbs_VERTEX);
  preset.faces.assign(body.faces.size(), TopoDS_Face());
  preset.edges.assign(body.edges.size(), TopoDS_Edge());
  preset.chains.assign(body.edges.size(), std::vector<TopoDS_Edge>());
  preset.vertices.assign(keys.vertices.size(), TopoDS_Vertex());
  for (std::size_t i = 0; i < body.faces.size(); ++i) {
    if (best_taken[i]) {
      const TopoDS_Face& face = best->faces.at(keys.faces[i]);
      preset.faces[i] = TopoDS::Face(copy.ModifiedShape(face).Oriented(face.Orientation()));
    }
  }
  // Edges and vertices of the faces taken, also where faces healed here
  // meet them (once in this body, as the base's).
  for (std::size_t i = 0; i < body.edges.size(); ++i) {
    if (edge_counts.at(keys.edges[i]) != 1) {
      continue;
    }
    const auto found = best->edges.find(keys.edges[i]);
    if (found != best->edges.end() &&
        std::all_of(found->second.begin(), found->second.end(),
                    [&](const TopoDS_Edge& piece) { return taken_edges.count(piece.TShape().get()) > 0; })) {
      for (const TopoDS_Edge& piece : found->second) {
        preset.chains[i].push_back(TopoDS::Edge(copy.ModifiedShape(piece).Oriented(piece.Orientation())));
      }
    }
  }
  for (std::size_t i = 0; i < keys.vertices.size(); ++i) {
    if (vertex_counts.at(keys.vertices[i]) != 1) {
      continue;
    }
    const auto found = best->vertices.find(keys.vertices[i]);
    if (found != best->vertices.end() && taken_vertices.count(found->second.TShape().get()) > 0) {
      preset.vertices[i] = TopoDS::Vertex(copy.ModifiedShape(found->second).Oriented(found->second.Orientation()));
    }
  }
  reused = std::move(best_taken);
  return true;
}

} // namespace

namespace {

// ShapeFix's healing as `heal` does it (with its repair of intersecting
// wires), of the faces built here (`fresh`) only: the others are copies of
// faces healed in another state of the body (`reused`). Edges are made
// same-parameter and vertex tolerances fixed on the faces healed here, so
// also where they meet the others.
TopoDS_Shape heal_fresh(const TopoDS_Shape& shape, const BuildOptions& options, const reuse::TShapes& fresh,
                        const reuse::TShapes& reused, Handle(ShapeBuild_ReShape)& context) {
  Handle(ShapeFix_Shape) fix = new ShapeFix_Shape(shape);
  fix->SetPrecision(options.tolerance);
  fix->SetMaxTolerance(kMaxTolerance);
  fix->FixShellTool()->FixOrientationMode() = 0;
  fix->FixSolidTool()->FixShellOrientationMode() = 0;
  fix->FixSolidTool()->CreateOpenSolidMode() = false;
  fix->FixShellTool()->FixFaceMode() = 0;
  fix->FixSameParameterMode() = 0;
  fix->FixVertexTolMode() = 0;
  const Handle(ShapeFix_Face) face_fix = fix->FixFaceTool();
  face_fix->SetContext(fix->Context());
  for (TopExp_Explorer ex(shape, TopAbs_FACE); ex.More(); ex.Next()) {
    if (fresh.count(ex.Current().TShape().get()) == 0) {
      continue;
    }
    face_fix->Init(TopoDS::Face(ex.Current()));
    face_fix->Perform();
    if (options.progress) {
      options.progress();
    }
  }
  fix->Perform();
  geometry::detail::fail_occt_allocation_if_asked("heal");
  const TopoDS_Shape healed = fix->Shape();
  context = fix->Context();
  BRep_Builder builder;
  TopoDS_Compound changed;
  builder.MakeCompound(changed);
  for (TopExp_Explorer ex(healed, TopAbs_FACE); ex.More(); ex.Next()) {
    if (reused.count(ex.Current().TShape().get()) == 0) {
      builder.Add(changed, ex.Current());
    }
  }
  ShapeFix::SameParameter(changed, false);
  const Handle(ShapeFix_Edge) edge_fix = new ShapeFix_Edge;
  for (TopExp_Explorer f(changed, TopAbs_FACE); f.More(); f.Next()) {
    const TopoDS_Face& face = TopoDS::Face(f.Current());
    for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
      edge_fix->FixVertexTolerance(TopoDS::Edge(e.Current()), face);
    }
  }
  return healed;
}

// Builds the body with the faces another state's build healed (mitcad#181):
// false, leaving `result` as it is, when too few faces are shared or the
// result is not a valid solid (the body is then built as a whole).
bool build_reusing(const Body& body, const reuse::BodyKeys& keys, const BuildOptions& options,
                   BuildResult& result) {
  std::string why;
  try {
    Parts preset;
    std::vector<bool> reused;
    const auto t0 = std::chrono::steady_clock::now();
    if (!reused_parts(body, keys, options, preset, reused, why)) {
      note(result.report, "healed as a whole: " + why);
      return false;
    }
    const auto t1 = std::chrono::steady_clock::now();
    BuildResult attempt;
    BuildReport& report = attempt.report;
    Parts parts;
    bool all_solid = false;
    TopoDS_Shape shape = build_topology(body, options, report, all_solid, &preset, &parts);
    if (!all_solid) {
      note(result.report, "healed as a whole: not all solid with the faces shared");
      return false;
    }
    shape = apply_transform(shape, body.transform);
    reuse::TShapes fresh;
    reuse::TShapes taken;
    for (std::size_t i = 0; i < parts.faces.size(); ++i) {
      if (parts.faces[i].IsNull()) {
        continue;
      }
      if (reused[i]) {
        taken.insert(parts.faces[i].TShape().get());
        ++report.reused_faces;
      } else {
        fresh.insert(parts.faces[i].TShape().get());
      }
    }
    const auto t2 = std::chrono::steady_clock::now();
    Handle(ShapeBuild_ReShape) context;
    shape = heal_fresh(shape, options, fresh, taken, context);
    const auto t3 = std::chrono::steady_clock::now();
    if (count_of(shape, TopAbs_SOLID) == 0) {
      note(result.report, "healed as a whole: no solid with the faces shared");
      return false;
    }
    if (const BRepCheck_Analyzer analyzer(shape, true, true); !analyzer.IsValid()) {
      std::string problems;
      for (const std::string& p : check_problems(analyzer, shape)) {
        problems += (problems.empty() ? "" : ", ") + p;
      }
      note(result.report, "healed as a whole: not valid with " + std::to_string(report.reused_faces) +
                              " faces shared (" + problems + ")");
      return false;
    }
    report.solid = true;
    report.valid = true;
    const auto seconds = [](std::chrono::steady_clock::time_point a, std::chrono::steady_clock::time_point b) {
      return std::to_string(std::chrono::duration<double>(b - a).count()).substr(0, 5);
    };
    note(report, "taken in " + seconds(t0, t1) + " s, built in " + seconds(t1, t2) + " s, healed in " +
                     seconds(t2, t3) + " s, checked in " + seconds(t3, std::chrono::steady_clock::now()) + " s");
    const TopoDS_Shape unoriented = shape;
    shape = orient_solids(shape);
    report.built = true;
    attempt.shape = shape;
    if (options.keep) {
      attempt.healed = keep_healed(body, keys, parts, unoriented, shape, context, options);
      note(report, kept_text(*attempt.healed));
    }
    result = std::move(attempt);
    return true;
  } catch (const Standard_OutOfMemory&) {
    throw;
  } catch (const Standard_Failure& e) {
    note(result.report, "healed as a whole: " + failure_text(e));
    return false;
  } catch (const std::bad_alloc&) {
    throw;
  } catch (const std::exception& e) {
    note(result.report, std::string("healed as a whole: ") + e.what());
    return false;
  }
}

// build_body without its check of the memory.
void build_into(const Body& body, const BuildOptions& options, BuildResult& result) {
  BuildReport& report = result.report;
  try {
    // Large bodies keep and reuse what was healed (mitcad#181).
    const bool reusing = options.fix && !options.measure && body.faces.size() >= options.reuse_min_faces &&
                         (options.keep || !options.bases.empty());
    reuse::BodyKeys keys;
    if (reusing) {
      keys = reuse::body_keys(body);
      if (!options.bases.empty() && build_reusing(body, keys, options, result)) {
        return;
      }
    }
    // Kept for the other states: a valid solid healed once.
    bool keep = reusing && options.keep && body.transform.empty();
    Handle(ShapeBuild_ReShape) context;
    Parts parts;
    bool all_solid = false;
    TopoDS_Shape shape = build_topology(body, options, report, all_solid, nullptr, keep ? &parts : nullptr);
    shape = apply_transform(shape, body.transform);
    if (options.fix) {
      // Two of ShapeFix's repairs leave some bodies invalid; they are
      // healed again without them when the first healing leaves them
      // invalid:
      // - Wires of a face that touch at a vertex (a hole touching the
      //   boundary): the repair of intersecting wires puts edges of no
      //   length at the touching point.
      // - Edges of more than two faces of a shell (solids touching along
      //   an edge): keeping the faces' orientation keeps the shell
      //   non-manifold; ShapeFix orients them instead, which splits the
      //   shell, when that keeps the volume.
      const bool touching = has_touching_wires(shape);
      const bool crowded = has_crowded_edges(shape);
      shape = heal(shape, options, true, false, keep ? &context : nullptr);
      // (The checks run on OCCT's thread pool, mitcad#181: checking a body
      // of 23 000 faces took 37 s on one thread, more than healing it.)
      if ((touching || crowded) && !BRepCheck_Analyzer(shape, true, true).IsValid()) {
        BuildReport scratch;
        bool unused = false;
        TopoDS_Shape again = apply_transform(build_topology(body, options, scratch, unused), body.transform);
        again = heal(again, options, !touching, crowded);
        const auto solid_volume = [](const TopoDS_Shape& s) {
          return count_of(s, TopAbs_SOLID) > 0 ? std::abs(analysed_volume_of(orient_solids(s))) : 0.0;
        };
        if (BRepCheck_Analyzer(again, true, true).IsValid()) {
          const double before = solid_volume(shape);
          const double after = solid_volume(again);
          if (std::abs(after - before) <= 1e-6 * std::max(before, after)) {
            shape = again;
            keep = false;
            note(report, std::string("healed again") + (touching ? ", wires touching at a vertex kept" : "") +
                             (crowded ? ", shells split at edges of more than two faces" : ""));
          }
        }
      }
      if (BRepCheck_Analyzer(shape, true, true).IsValid()) {
        // (Orienting the solids below keeps them valid.)
        report.valid = true;
      } else {
        keep = false;
        double widest = 0.0;
        const int cusps = widen_cusps(shape, widest);
        if (cusps > 0) {
          std::ostringstream text;
          text << "tolerance of " << cusps << " vertices at cusps raised, up to " << widest << " mm";
          note(report, text.str());
        }
      }
    }
    report.solid = all_solid && count_of(shape, TopAbs_SOLID) > 0;
    const TopoDS_Shape unoriented = shape;
    if (report.solid) {
      if (options.measure) {
        report.raw_volume = analysed_volume_of(shape);
      }
      shape = orient_solids(shape);
      if (options.measure) {
        report.volume = analysed_volume_of(shape);
      }
    }
    if (!options.measure) {
      report.built = true;
      result.shape = shape;
      if (keep && report.solid) {
        result.healed = keep_healed(body, keys, parts, unoriented, shape, context, options);
        note(report, kept_text(*result.healed));
      }
      return;
    }
    report.area = analysis::surface_properties(shape).Mass();
    Bnd_Box box;
    BRepBndLib::AddOptimal(shape, box, false, false);
    if (!box.IsVoid()) {
      box.Get(report.bbox[0], report.bbox[1], report.bbox[2], report.bbox[3], report.bbox[4], report.bbox[5]);
    }
    report.faces = count_of(shape, TopAbs_FACE);
    report.edges = count_of(shape, TopAbs_EDGE);
    report.vertices = count_of(shape, TopAbs_VERTEX);
    BRepCheck_Analyzer analyzer(shape, true, true);
    report.valid = analyzer.IsValid();
    if (!report.valid) {
      for (std::string& p : check_problems(analyzer, shape)) {
        note(report, "check: " + p);
      }
    }
    report.built = true;
    result.shape = shape;
  } catch (const Standard_OutOfMemory&) {
    report.out_of_memory = true;
  } catch (const std::bad_alloc&) {
    report.out_of_memory = true;
  } catch (const Standard_Failure& e) {
    report.error = failure_text(e);
  } catch (const std::exception& e) {
    report.error = e.what();
  }
}

} // namespace

BuildResult build_body(const Body& body, const BuildOptions& options) {
  // An allocation that failed, also one the healing or the checker caught
  // inside (mitcad#132: they go on, and the checker calls the shape
  // invalid), leaves no body: the caller learns that memory ran out.
  const std::size_t failed = geometry::failed_allocations_in_thread();
  BuildResult result;
  try {
    build_into(body, options, result);
  } catch (const std::bad_alloc&) {
    // (Also while the report was written.)
    result.report.out_of_memory = true;
  }
  if (result.report.out_of_memory || geometry::failed_allocations_in_thread() != failed) {
    result.report.out_of_memory = true;
    result.report.built = false;
    result.report.error = "out of memory";
    result.shape.Nullify();
  }
  return result;
}

} // namespace mitcad::brep
