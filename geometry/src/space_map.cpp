// SPDX-License-Identifier: MIT
#include "space_map.hpp"

#include <algorithm>
#include <cmath>
#include <cstdio>
#include <memory>
#include <string>
#include <utility>
#include <vector>

#include <Adaptor3d_Curve.hxx>
#include <Adaptor3d_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepCheck_Result.hxx>
#include <BRepCheck_Status.hxx>
#include <BRepBuilderAPI_ModifyShape.hxx>
#include <BRepLib.hxx>
#include <BRepTools.hxx>
#include <BRepTools_Modification.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <Geom2dAPI_Interpolate.hxx>
#include <Geom2d_BSplineCurve.hxx>
#include <Geom2d_Curve.hxx>
#include <GeomAdaptor_Curve.hxx>
#include <GeomAdaptor_Surface.hxx>
#include <GeomConvert_ApproxCurve.hxx>
#include <GeomConvert_ApproxSurface.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_Curve.hxx>
#include <NCollection_DataMap.hxx>
#include <NCollection_HArray1.hxx>
#include <Precision.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <Standard_NotImplemented.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Solid.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Vec.hxx>

#include "mitcad/geometry/cancel.hpp"

#include "history.hpp"
#include "util.hpp"

namespace mitcad::geometry::detail {
namespace {

// The map's derivative at a point (its columns the images of the axes'
// directions), by central differences.
struct Jacobian {
  gp_Vec x;
  gp_Vec y;
  gp_Vec z;

  gp_Vec of(const gp_Vec& v) const { return x * v.X() + y * v.Y() + z * v.Z(); }
  double determinant() const { return x.Dot(y.Crossed(z)); }
  // How much it lengthens a length at most, near enough.
  double stretch() const { return std::max({x.Magnitude(), y.Magnitude(), z.Magnitude()}); }
};

class Mapper {
public:
  Mapper(const SpaceMap& map, double step) : m_map(map), m_step(step) {}

  gp_Pnt point(const gp_Pnt& p) const { return m_map(p); }

  Jacobian jacobian(const gp_Pnt& p) const {
    const auto column = [&](const gp_Vec& axis) {
      const gp_Pnt ahead = m_map(p.Translated(axis * m_step));
      const gp_Pnt behind = m_map(p.Translated(axis * -m_step));
      return gp_Vec(behind, ahead) / (2.0 * m_step);
    };
    return {column(gp_Vec(1.0, 0.0, 0.0)), column(gp_Vec(0.0, 1.0, 0.0)), column(gp_Vec(0.0, 0.0, 1.0))};
  }

private:
  SpaceMap m_map;
  double m_step;
};

// A surface carried through the map, as OCCT's approximation evaluates it:
// its points mapped, its first derivatives by the map's derivative, its
// second by differences of those.
class MappedSurface : public Adaptor3d_Surface {
public:
  MappedSurface(const occ::handle<Geom_Surface>& surface, const Mapper* mapper, double u0, double u1, double v0,
                double v1, double hu, double hv)
      : m_surface(surface), m_inner(surface, u0, u1, v0, v1), m_mapper(mapper), m_u0(u0), m_u1(u1), m_v0(v0),
        m_v1(v1), m_hu(hu), m_hv(hv) {}

  occ::handle<Adaptor3d_Surface> ShallowCopy() const override {
    return new MappedSurface(m_surface, m_mapper, m_u0, m_u1, m_v0, m_v1, m_hu, m_hv);
  }
  double FirstUParameter() const override { return m_u0; }
  double LastUParameter() const override { return m_u1; }
  double FirstVParameter() const override { return m_v0; }
  double LastVParameter() const override { return m_v1; }
  GeomAbs_Shape UContinuity() const override { return m_inner.UContinuity(); }
  GeomAbs_Shape VContinuity() const override { return m_inner.VContinuity(); }
  int NbUIntervals(const GeomAbs_Shape s) const override { return m_inner.NbUIntervals(s); }
  int NbVIntervals(const GeomAbs_Shape s) const override { return m_inner.NbVIntervals(s); }
  void UIntervals(NCollection_Array1<double>& t, const GeomAbs_Shape s) const override { m_inner.UIntervals(t, s); }
  void VIntervals(NCollection_Array1<double>& t, const GeomAbs_Shape s) const override { m_inner.VIntervals(t, s); }
  occ::handle<Adaptor3d_Surface> UTrim(const double first, const double last, const double) const override {
    return new MappedSurface(m_surface, m_mapper, first, last, m_v0, m_v1, m_hu, m_hv);
  }
  occ::handle<Adaptor3d_Surface> VTrim(const double first, const double last, const double) const override {
    return new MappedSurface(m_surface, m_mapper, m_u0, m_u1, first, last, m_hu, m_hv);
  }
  bool IsUClosed() const override { return false; }
  bool IsVClosed() const override { return false; }
  bool IsUPeriodic() const override { return false; }
  bool IsVPeriodic() const override { return false; }
  double UResolution(const double r) const override { return m_inner.UResolution(r); }
  double VResolution(const double r) const override { return m_inner.VResolution(r); }
  GeomAbs_SurfaceType GetType() const override { return GeomAbs_OtherSurface; }

  gp_Pnt EvalD0(const double u, const double v) const override { return m_mapper->point(m_surface->Value(u, v)); }

  Geom_Surface::ResD1 EvalD1(const double u, const double v) const override {
    gp_Pnt p;
    gp_Vec du;
    gp_Vec dv;
    m_surface->D1(u, v, p, du, dv);
    const Jacobian j = m_mapper->jacobian(p);
    Geom_Surface::ResD1 result;
    result.Point = m_mapper->point(p);
    result.D1U = j.of(du);
    result.D1V = j.of(dv);
    return result;
  }

  Geom_Surface::ResD2 EvalD2(const double u, const double v) const override {
    const Geom_Surface::ResD1 here = EvalD1(u, v);
    const Geom_Surface::ResD1 u_ahead = EvalD1(u + m_hu, v);
    const Geom_Surface::ResD1 u_behind = EvalD1(u - m_hu, v);
    const Geom_Surface::ResD1 v_ahead = EvalD1(u, v + m_hv);
    const Geom_Surface::ResD1 v_behind = EvalD1(u, v - m_hv);
    Geom_Surface::ResD2 result;
    result.Point = here.Point;
    result.D1U = here.D1U;
    result.D1V = here.D1V;
    result.D2U = (u_ahead.D1U - u_behind.D1U) / (2.0 * m_hu);
    result.D2V = (v_ahead.D1V - v_behind.D1V) / (2.0 * m_hv);
    result.D2UV = (v_ahead.D1U - v_behind.D1U) / (2.0 * m_hv);
    return result;
  }

  Geom_Surface::ResD3 EvalD3(const double, const double) const override {
    throw Standard_NotImplemented("a mapped surface has no third derivatives");
  }

  gp_Vec EvalDN(const double u, const double v, const int nu, const int nv) const override {
    if (nu + nv == 1) {
      const Geom_Surface::ResD1 d1 = EvalD1(u, v);
      return nu == 1 ? d1.D1U : d1.D1V;
    }
    if (nu + nv == 2) {
      const Geom_Surface::ResD2 d2 = EvalD2(u, v);
      return nu == 2 ? d2.D2U : nv == 2 ? d2.D2V : d2.D2UV;
    }
    throw Standard_NotImplemented("a mapped surface has no third derivatives");
  }

private:
  occ::handle<Geom_Surface> m_surface;
  GeomAdaptor_Surface m_inner;
  const Mapper* m_mapper;
  double m_u0;
  double m_u1;
  double m_v0;
  double m_v1;
  double m_hu; // the steps of the second derivatives' differences
  double m_hv;
};

// A curve carried through the map, likewise.
class MappedCurve : public Adaptor3d_Curve {
public:
  MappedCurve(const occ::handle<Geom_Curve>& curve, const Mapper* mapper, double first, double last, double h)
      : m_curve(curve), m_inner(curve, first, last), m_mapper(mapper), m_first(first), m_last(last), m_h(h) {}

  occ::handle<Adaptor3d_Curve> ShallowCopy() const override {
    return new MappedCurve(m_curve, m_mapper, m_first, m_last, m_h);
  }
  double FirstParameter() const override { return m_first; }
  double LastParameter() const override { return m_last; }
  GeomAbs_Shape Continuity() const override { return m_inner.Continuity(); }
  int NbIntervals(const GeomAbs_Shape s) const override { return m_inner.NbIntervals(s); }
  void Intervals(NCollection_Array1<double>& t, const GeomAbs_Shape s) const override { m_inner.Intervals(t, s); }
  occ::handle<Adaptor3d_Curve> Trim(const double first, const double last, const double) const override {
    return new MappedCurve(m_curve, m_mapper, first, last, m_h);
  }
  bool IsClosed() const override { return false; }
  bool IsPeriodic() const override { return false; }
  double Resolution(const double r) const override { return m_inner.Resolution(r); }
  GeomAbs_CurveType GetType() const override { return GeomAbs_OtherCurve; }

  gp_Pnt EvalD0(const double t) const override { return m_mapper->point(m_curve->Value(t)); }

  Geom_Curve::ResD1 EvalD1(const double t) const override {
    gp_Pnt p;
    gp_Vec d;
    m_curve->D1(t, p, d);
    Geom_Curve::ResD1 result;
    result.Point = m_mapper->point(p);
    result.D1 = m_mapper->jacobian(p).of(d);
    return result;
  }

  Geom_Curve::ResD2 EvalD2(const double t) const override {
    const Geom_Curve::ResD1 here = EvalD1(t);
    const Geom_Curve::ResD1 ahead = EvalD1(t + m_h);
    const Geom_Curve::ResD1 behind = EvalD1(t - m_h);
    Geom_Curve::ResD2 result;
    result.Point = here.Point;
    result.D1 = here.D1;
    result.D2 = (ahead.D1 - behind.D1) / (2.0 * m_h);
    return result;
  }

  Geom_Curve::ResD3 EvalD3(const double) const override {
    throw Standard_NotImplemented("a mapped curve has no third derivatives");
  }

  gp_Vec EvalDN(const double t, const int n) const override {
    if (n == 1) {
      return EvalD1(t).D1;
    }
    if (n == 2) {
      return EvalD2(t).D2;
    }
    throw Standard_NotImplemented("a mapped curve has no third derivatives");
  }

private:
  occ::handle<Geom_Curve> m_curve;
  GeomAdaptor_Curve m_inner;
  const Mapper* m_mapper;
  double m_first;
  double m_last;
  double m_h;
};

// A face's surface in the shape's coordinates (its location applied).
occ::handle<Geom_Surface> placed_surface(const TopoDS_Face& face) {
  TopLoc_Location location;
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face, location);
  if (surface.IsNull() || location.IsIdentity()) {
    return surface;
  }
  return occ::down_cast<Geom_Surface>(surface->Transformed(location.Transformation()));
}

// A 2D curve through `exact` over [first, last] at the same parameters: a
// B-spline through its points at evenly spaced parameters, more of them
// until the surface's points of the two are within `tolerance`.
occ::handle<Geom2d_Curve> fitted(const std::function<gp_Pnt2d(double)>& exact, double first, double last,
                                 const occ::handle<Geom_Surface>& surface, double tolerance) {
  for (int n = 8; n <= 4096; n *= 2) {
    const occ::handle<NCollection_HArray1<gp_Pnt2d>> points = new NCollection_HArray1<gp_Pnt2d>(1, n + 1);
    const occ::handle<NCollection_HArray1<double>> params = new NCollection_HArray1<double>(1, n + 1);
    for (int i = 0; i <= n; ++i) {
      const double t = first + (last - first) * i / n;
      points->SetValue(i + 1, exact(t));
      params->SetValue(i + 1, t);
    }
    Geom2dAPI_Interpolate interpolate(points, params, false, 1.0e-12);
    interpolate.Perform();
    if (!interpolate.IsDone()) {
      return nullptr;
    }
    const occ::handle<Geom2d_BSplineCurve> curve = interpolate.Curve();
    double worst = 0.0;
    for (int i = 0; i < n; ++i) {
      const double t = first + (last - first) * (i + 0.5) / n;
      const gp_Pnt2d a = curve->Value(t);
      const gp_Pnt2d b = exact(t);
      worst = std::max(worst, surface->Value(a.X(), a.Y()).Distance(surface->Value(b.X(), b.Y())));
    }
    if (worst <= tolerance) {
      // Converted explicitly: GCC 13 (the Linux release's build root) does
      // not take the implicit handle conversion in a return.
      return occ::handle<Geom2d_Curve>(curve);
    }
  }
  return nullptr;
}

// The images of a shape's faces, edges and vertices, for
// BRepTools_Modifier.
class Bend : public BRepTools_Modification {
public:
  Bend(const Mapper& mapper, const LaidFaces& laid, double tolerance)
      : m_mapper(mapper), m_laid(laid), m_tolerance(tolerance) {}

  // Every face's and edge's image; false where one cannot be made.
  bool prepare(const TopoDS_Shape& shape) {
    Bnd_Box box;
    BRepBndLib::Add(shape, box);
    if (box.IsVoid()) {
      return failed("the shape has no bounds");
    }
    double x0 = 0.0, y0 = 0.0, z0 = 0.0, x1 = 0.0, y1 = 0.0, z1 = 0.0;
    box.Get(x0, y0, z0, x1, y1, z1);
    // (The map is one to one over the shape: its derivative's sign is the
    // same everywhere there.)
    const double turn = m_mapper.jacobian(gp_Pnt((x0 + x1) / 2.0, (y0 + y1) / 2.0, (z0 + z1) / 2.0)).determinant();
    if (!(std::abs(turn) > 0.0)) {
      return failed("the map flattens space at the shape's middle");
    }
    m_inside_out = turn < 0.0;
    for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
      const TopoDS_Face& face = TopoDS::Face(it.Current());
      if (!m_faces.IsBound(face) && !prepare_face(face)) {
        return false; // (prepare_face says why)
      }
    }
    for (TopExp_Explorer it(shape, TopAbs_EDGE); it.More(); it.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
      if (BRep_Tool::Degenerated(edge) || m_curves.IsBound(edge)) {
        continue;
      }
      TopLoc_Location location;
      double first = 0.0;
      double last = 0.0;
      occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, location, first, last);
      if (curve.IsNull() || !(last > first)) {
        return failed("an edge has no curve or no range");
      }
      if (!location.IsIdentity()) {
        curve = occ::down_cast<Geom_Curve>(curve->Transformed(location.Transformation()));
      }
      const occ::handle<MappedCurve> image =
          new MappedCurve(curve, &m_mapper, first, last, 1.0e-4 * (last - first));
      GeomConvert_ApproxCurve approximation(image, m_tolerance, GeomAbs_C1, 200, 11);
      if (!approximation.HasResult() || !(approximation.MaxError() <= std::max(10.0 * m_tolerance, 1.0e-5))) {
        return failed(std::string("an edge's curve could not be approximated (") +
                      (approximation.HasResult() ? "error " + std::to_string(approximation.MaxError()) : "no result") +
                      ", tolerance " + std::to_string(m_tolerance) + ")");
      }
      CurveImage mapped;
      mapped.curve = approximation.Curve();
      mapped.error = approximation.MaxError();
      mapped.stretch = m_mapper.jacobian(curve->Value(first)).stretch();
      m_curves.Bind(edge, mapped);
    }
    return true;
  }

  bool NewSurface(const TopoDS_Face& F, occ::handle<Geom_Surface>& S, TopLoc_Location& L, double& Tol,
                  bool& RevWires, bool& RevFace) override {
    const FaceImage* image = m_faces.Seek(F);
    if (image == nullptr) {
      return false;
    }
    S = image->surface;
    L = TopLoc_Location();
    Tol = BRep_Tool::Tolerance(F) + image->error + m_tolerance;
    RevWires = image->turned;
    RevFace = image->reversed;
    return true;
  }

  bool NewCurve(const TopoDS_Edge& E, occ::handle<Geom_Curve>& C, TopLoc_Location& L, double& Tol) override {
    L = TopLoc_Location();
    if (BRep_Tool::Degenerated(E)) {
      C.Nullify();
      Tol = BRep_Tool::Tolerance(E) + m_tolerance;
      return true;
    }
    const CurveImage* image = m_curves.Seek(E);
    if (image == nullptr) {
      return false;
    }
    C = image->curve;
    Tol = BRep_Tool::Tolerance(E) * image->stretch + image->error + 2.0 * m_tolerance;
    return true;
  }

  bool NewPoint(const TopoDS_Vertex& V, gp_Pnt& P, double& Tol) override {
    const gp_Pnt p = BRep_Tool::Pnt(V);
    P = m_mapper.point(p);
    Tol = BRep_Tool::Tolerance(V) * m_mapper.jacobian(p).stretch() + 4.0 * m_tolerance;
    return true;
  }

  bool NewCurve2d(const TopoDS_Edge& E, const TopoDS_Face& F, const TopoDS_Edge&, const TopoDS_Face&,
                  occ::handle<Geom2d_Curve>& C, double& Tol) override {
    double first = 0.0;
    double last = 0.0;
    const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(E, F, first, last);
    if (pcurve.IsNull()) {
      return false;
    }
    Tol = BRep_Tool::Tolerance(E) + 2.0 * m_tolerance;
    const FaceImage* image = m_faces.Seek(F);
    if (image != nullptr && image->parameters) {
      // On a laid face: its points' parameters on the surface laid onto,
      // at the same parameters of the edge.
      const occ::handle<Geom_Surface> flat = placed_surface(F);
      const auto& parameters = image->parameters;
      C = fitted(
          [&](double t) {
            const gp_Pnt2d uv = pcurve->Value(t);
            return parameters(flat->Value(uv.X(), uv.Y()));
          },
          first, last, image->surface, m_tolerance);
      if (C.IsNull()) {
        throw Standard_Failure("an edge could not be laid onto its face's surface");
      }
      return true;
    }
    C = occ::down_cast<Geom2d_Curve>(pcurve->Copy());
    return true;
  }

  bool NewParameter(const TopoDS_Vertex& V, const TopoDS_Edge& E, double& P, double& Tol) override {
    if (BRep_Tool::Degenerated(E)) {
      return false;
    }
    P = BRep_Tool::Parameter(V, E);
    Tol = BRep_Tool::Tolerance(V) + 4.0 * m_tolerance;
    return true;
  }

  GeomAbs_Shape Continuity(const TopoDS_Edge& E, const TopoDS_Face& F1, const TopoDS_Face& F2, const TopoDS_Edge&,
                           const TopoDS_Face&, const TopoDS_Face&) override {
    return BRep_Tool::Continuity(E, F1, F2);
  }

private:
  struct FaceImage {
    occ::handle<Geom_Surface> surface;
    std::function<gp_Pnt2d(const gp_Pnt&)> parameters; // a laid face's
    bool reversed = false;
    // A laid face whose parameters the map turns over: its loops run the
    // other way round on the new surface and are reversed (and the face
    // with them, so that each edge keeps its sense in the shell).
    bool turned = false;
    double error = 0.0;
  };
  struct CurveImage {
    occ::handle<Geom_Curve> curve;
    double error = 0.0;
    double stretch = 1.0; // how much the map lengthens lengths at its start
  };

  bool prepare_face(const TopoDS_Face& face) {
    const occ::handle<Geom_Surface> surface = placed_surface(face);
    if (surface.IsNull()) {
      return failed("a face has no surface");
    }
    double u0 = 0.0;
    double u1 = 0.0;
    double v0 = 0.0;
    double v1 = 0.0;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    if (!(u1 > u0) || !(v1 > v0)) {
      return failed("a face has no parameter range");
    }
    FaceImage image;
    std::optional<LaidFace> laid;
    if (m_laid) {
      laid = m_laid(face);
    }
    if (laid) {
      // A map that turns the parameters over (the new surface's normal
      // against the mapped one) turns the face over, and so does one that
      // turns space inside out.
      const double u = (u0 + u1) / 2.0;
      const double v = (v0 + v1) / 2.0;
      gp_Pnt p;
      gp_Vec du;
      gp_Vec dv;
      surface->D1(u, v, p, du, dv);
      const Jacobian j = m_mapper.jacobian(p);
      const gp_Vec mapped = j.of(du).Crossed(j.of(dv));
      gp_Vec normal = mapped;
      if (laid->parameters) {
        const gp_Pnt2d at = laid->parameters(p);
        gp_Pnt q;
        gp_Vec qu;
        gp_Vec qv;
        laid->surface->D1(at.X(), at.Y(), q, qu, qv);
        normal = qu.Crossed(qv);
      } else {
        laid->surface->D1(u, v, p, du, dv);
        normal = du.Crossed(dv);
      }
      if (!(std::abs(normal.Dot(mapped)) > 0.0)) {
        return failed("a laid face's surface stands across the map's image of it");
      }
      image.surface = laid->surface;
      image.parameters = laid->parameters;
      image.turned = normal.Dot(mapped) < 0.0;
      image.reversed = image.turned != m_inside_out;
      m_faces.Bind(face, image);
      return true;
    }
    // The face's parameters and a little round them (its curves on the
    // surface reach to its bounds), within the surface's own.
    const double mu = 1.0e-6 * (u1 - u0) + Precision::PConfusion();
    const double mv = 1.0e-6 * (v1 - v0) + Precision::PConfusion();
    double su0 = 0.0;
    double su1 = 0.0;
    double sv0 = 0.0;
    double sv1 = 0.0;
    surface->Bounds(su0, su1, sv0, sv1);
    u0 -= mu;
    u1 += mu;
    v0 -= mv;
    v1 += mv;
    if (!surface->IsUPeriodic()) {
      u0 = Precision::IsInfinite(su0) ? u0 : std::max(u0, su0);
      u1 = Precision::IsInfinite(su1) ? u1 : std::min(u1, su1);
    }
    if (!surface->IsVPeriodic()) {
      v0 = Precision::IsInfinite(sv0) ? v0 : std::max(v0, sv0);
      v1 = Precision::IsInfinite(sv1) ? v1 : std::min(v1, sv1);
    }
    const occ::handle<MappedSurface> mapped =
        new MappedSurface(surface, &m_mapper, u0, u1, v0, v1, 1.0e-4 * (u1 - u0), 1.0e-4 * (v1 - v0));
    GeomConvert_ApproxSurface approximation(mapped, m_tolerance, GeomAbs_C1, GeomAbs_C1, 11, 11, 100, 1);
    if (!approximation.HasResult() || !(approximation.MaxError() <= std::max(10.0 * m_tolerance, 1.0e-5))) {
      return failed(std::string("a face's surface could not be approximated (") +
                    (approximation.HasResult() ? "error " + std::to_string(approximation.MaxError()) : "no result") +
                    ", tolerance " + std::to_string(m_tolerance) + ", type " +
                    std::to_string(static_cast<int>(GeomAdaptor_Surface(surface).GetType())) + ")");
    }
    image.surface = approximation.Surface();
    image.error = approximation.MaxError();
    // The same parameters: the new normal is the map's of the old one,
    // turned over where the map turns space inside out.
    image.reversed = m_inside_out;
    m_faces.Bind(face, image);
    return true;
  }

  bool failed(const std::string& why) {
    m_failure = why;
    return false;
  }

public:
  // Why prepare() failed.
  const std::string& failure() const { return m_failure; }

private:
  std::string m_failure;
  const Mapper& m_mapper;
  LaidFaces m_laid;
  double m_tolerance;
  bool m_inside_out = false;
  NCollection_DataMap<TopoDS_Shape, FaceImage, TopTools_ShapeMapHasher> m_faces;
  NCollection_DataMap<TopoDS_Shape, CurveImage, TopTools_ShapeMapHasher> m_curves;
};

// The steps of mapped_solid, on stderr (mitcad#1854: a wrapped emboss's
// bend that failed only in the Windows VM's run; diagnostics while it is
// looked into).
void trace(const std::string& what) {
  std::fprintf(stderr, "space map: %s\n", what.c_str());
  std::fflush(stderr);
}

// What the checker finds wrong, by the kinds of sub-shapes and their
// statuses.
std::string check_report(const TopoDS_Shape& shape) {
  const BRepCheck_Analyzer analyzer(shape);
  std::string report;
  const TopAbs_ShapeEnum kinds[] = {TopAbs_VERTEX, TopAbs_EDGE, TopAbs_WIRE, TopAbs_FACE, TopAbs_SHELL, TopAbs_SOLID};
  for (const TopAbs_ShapeEnum kind : kinds) {
    for (TopExp_Explorer it(shape, kind); it.More() && report.size() < 2000; it.Next()) {
      const occ::handle<BRepCheck_Result> result = analyzer.Result(it.Current());
      if (result.IsNull()) {
        continue;
      }
      for (const BRepCheck_Status status : result->Status()) {
        if (status != BRepCheck_NoError) {
          report += std::to_string(static_cast<int>(kind)) + ":" + std::to_string(static_cast<int>(status)) + " ";
        }
      }
    }
  }
  return report.empty() ? "(no sub-shape status)" : report;
}

// BRepBuilderAPI_ModifyShape's history over Bend's modification.
class Bending final : public BRepBuilderAPI_ModifyShape {
public:
  Bending(const TopoDS_Shape& shape, const occ::handle<BRepTools_Modification>& modification)
      : BRepBuilderAPI_ModifyShape(modification) {
    DoModif(shape);
  }
};

} // namespace

ShapePtr mapped_solid(const Shape& shape, const SpaceMap& map, const LaidFaces& laid, double tolerance) {
  if (shape.occt().IsNull()) {
    trace("no shape to bend");
    return nullptr;
  }
  try {
    OCC_CATCH_SIGNALS
    Bnd_Box box;
    BRepBndLib::Add(shape.occt(), box);
    if (box.IsVoid()) {
      trace("the shape to bend has no bounds");
      return nullptr;
    }
    const double size = std::max(1.0, std::sqrt(box.SquareExtent()));
    const Mapper mapper(map, 1.0e-5 * size);
    const occ::handle<Bend> bend = new Bend(mapper, laid, tolerance);
    if (!bend->prepare(shape.occt())) {
      trace("not prepared: " + bend->failure());
      return nullptr;
    }
    trace("prepared: " + std::to_string(shape.face_count()) + " faces, tolerance " + std::to_string(tolerance));
    Bending bending(shape.occt(), bend);
    if (!bending.IsDone() || bending.Shape().IsNull()) {
      trace("the modifier failed");
      return nullptr;
    }
    const TopoDS_Shape result = bending.Shape();
    BRepLib::SameParameter(result, tolerance, true);
    BRepLib::UpdateTolerances(result);
    // (The faces turned over as the map turns space; each solid's material
    // made sure to lie inside it.)
    for (TopExp_Explorer it(result, TopAbs_SOLID); it.More(); it.Next()) {
      TopoDS_Solid solid = TopoDS::Solid(it.Current());
      BRepLib::OrientClosedSolid(solid);
    }
    if (!is_valid(result)) {
      trace("the result is not valid: " + check_report(result));
      return nullptr;
    }
    trace("built");
    FaceNamer namer(result);
    for (int i = 0; i < shape.face_count(); ++i) {
      const TopoDS_Shape image = bending.ModifiedShape(shape.face(i));
      for (const std::string& name : shape.face_names(i)) {
        namer.add(image, name);
      }
    }
    return namer.shape();
  } catch (const Cancelled&) {
    throw;
  } catch (const Standard_Failure& failure) {
    trace(std::string("OCCT failed: ") + failure.what());
    return nullptr;
  } catch (const std::exception& error) {
    trace(std::string("failed: ") + error.what());
    throw;
  }
}

} // namespace mitcad::geometry::detail
