// SPDX-License-Identifier: MIT
#include "mitcad/geometry/brep_import.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <map>
#include <sstream>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepCheck.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepCheck_Result.hxx>
#include <BRepLib.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <ElSLib.hxx>
#include <Geom2d_Line.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_Circle.hxx>
#include <Geom_ConicalSurface.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Geom_Ellipse.hxx>
#include <Geom_Line.hxx>
#include <Geom_Plane.hxx>
#include <Geom_SphericalSurface.hxx>
#include <Geom_SurfaceOfLinearExtrusion.hxx>
#include <Geom_SurfaceOfRevolution.hxx>
#include <Geom_ToroidalSurface.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <NCollection_HArray1.hxx>
#include <NCollection_IndexedMap.hxx>
#include <NCollection_List.hxx>
#include <ShapeFix_Face.hxx>
#include <ShapeFix_Shape.hxx>
#include <ShapeFix_Shell.hxx>
#include <ShapeFix_Solid.hxx>
#include <Standard_Failure.hxx>
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
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Sphere.hxx>
#include <gp_Trsf.hxx>

#include "mitcad/analysis/common.hpp"

namespace mitcad::brep {
namespace {

constexpr int kCurveLine = 0;
constexpr int kCurveEllipse = 1;
constexpr int kCurveBSpline = 2;
constexpr int kCurveInterpolated = 3;

constexpr int kSurfacePlane = 0;
constexpr int kSurfaceCone = 1;
constexpr int kSurfaceSphere = 2;
constexpr int kSurfaceTorus = 3;
constexpr int kSurfaceBSpline = 4;
constexpr int kSurfaceExtrusion = 5;
constexpr int kSurfaceRevolution = 6;
constexpr int kSurfaceRuled = 7;

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

Handle(Geom_Surface) make_surface(Reader& in, int kind) {
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
    if (half_angle == 0.0) {
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
    if (rational) {
      NCollection_Array2<double> weights(1, nu, 1, nv);
      for (int iu = 1; iu <= nu; ++iu) {
        for (int iv = 1; iv <= nv; ++iv) {
          weights(iu, iv) = in.real();
        }
      }
      return new Geom_BSplineSurface(poles, weights, uknots, vknots, umults, vmults, ud, vd);
    }
    return new Geom_BSplineSurface(poles, uknots, vknots, umults, vmults, ud, vd);
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
template <class Handle, class Make>
std::vector<Handle> build_all(const Body& body, const std::vector<Geometry>& items, const char* what, int& failed,
                              BuildReport& report, Make make) {
  std::vector<Handle> out(items.size());
  for (std::size_t i = 0; i < items.size(); ++i) {
    try {
      Reader in(body, items[i]);
      out[i] = make(in, items[i].kind);
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

// The parameter v of a cone's apex or a sphere's pole at `p`, if `p` is
// one (within `tol`).
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
  return false;
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

double volume_of(const TopoDS_Shape& shape) { return analysis::volume_properties(shape).Mass(); }

TopoDS_Shape build_topology(const Body& body, const BuildOptions& options, BuildReport& report, bool& all_solid) {
  const auto curves = build_all<Handle(Geom_Curve)>(body, body.curves, "curve", report.curves_failed, report,
                                                    [](Reader& in, int kind) { return make_curve(in, kind); });
  const auto surfaces =
      build_all<Handle(Geom_Surface)>(body, body.surfaces, "surface", report.surfaces_failed, report,
                                      [](Reader& in, int kind) { return make_surface(in, kind); });
  const double tol = options.tolerance;
  BRep_Builder builder;

  if (body.vertices.size() % 4 != 0) {
    throw std::runtime_error("vertex data is not a multiple of 4 values");
  }
  std::vector<TopoDS_Vertex> vertices(body.vertices.size() / 4);
  for (std::size_t i = 0; i < vertices.size(); ++i) {
    const double* v = &body.vertices[4 * i];
    builder.MakeVertex(vertices[i], gp_Pnt(v[0], v[1], v[2]), std::max(v[3], tol));
  }

  std::vector<TopoDS_Edge> edges(body.edges.size());
  for (std::size_t i = 0; i < body.edges.size(); ++i) {
    const Edge& e = body.edges[i];
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
  for (std::size_t i = 0; i < body.faces.size(); ++i) {
    const Face& f = body.faces[i];
    if (f.surface >= surfaces.size() || surfaces[f.surface].IsNull()) {
      ++report.faces_failed;
      continue;
    }
    TopoDS_Face face;
    builder.MakeFace(face, surfaces[f.surface], tol);
    bool complete = true;
    for (std::uint32_t l = f.first_loop; l < f.first_loop + f.loop_count && l < body.loops.size(); ++l) {
      const Loop& lp = body.loops[l];
      TopoDS_Wire wire;
      builder.MakeWire(wire);
      int added = 0;
      for (std::uint32_t c = lp.first_coedge; c < lp.first_coedge + lp.coedge_count && c < body.coedges.size(); ++c) {
        const Coedge& ce = body.coedges[c];
        if (ce.edge >= edges.size() || edges[ce.edge].IsNull()) {
          complete = false;
          continue;
        }
        // The face is built forward on its surface and reversed at the end,
        // so its edges are stored relative to the surface normal.
        TopAbs_Orientation o = ce.forward ? TopAbs_FORWARD : TopAbs_REVERSED;
        if (f.reversed) {
          o = TopAbs::Reverse(o);
        }
        builder.Add(wire, edges[ce.edge].Oriented(o));
        ++added;
      }
      if (added > 0) {
        wire.Closed(complete);
        builder.Add(face, wire);
      }
    }
    // Point loops: a degenerated edge at the apex of a cone or the pole of
    // a sphere, on the side of the face's other vertices.
    for (std::uint32_t k = f.first_point_loop;
         k < f.first_point_loop + f.point_loop_count && k < body.point_loops.size(); ++k) {
      const std::uint32_t vi = body.point_loops[k];
      double v = 0.0;
      double v_face = 0.0;
      bool other = false;
      for (TopExp_Explorer ex(face, TopAbs_VERTEX); ex.More() && !other && vi < vertices.size(); ex.Next()) {
        const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(ex.Current()));
        if (p.Distance(BRep_Tool::Pnt(vertices[vi])) > 10.0 * tol) {
          GeomAPI_ProjectPointOnSurf projection(p, surfaces[f.surface]);
          if (projection.NbPoints() > 0) {
            double u = 0.0;
            projection.LowerDistanceParameters(u, v_face);
            other = true;
          }
        }
      }
      if (!other || !singular_v(surfaces[f.surface], BRep_Tool::Pnt(vertices[vi]), 10.0 * tol, v)) {
        note(report, "face " + std::to_string(i) + ": point loop not at a cone apex or sphere pole");
        continue;
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

} // namespace

BuildResult build_body(const Body& body, const BuildOptions& options) {
  BuildResult result;
  BuildReport& report = result.report;
  try {
    bool all_solid = false;
    TopoDS_Shape shape = build_topology(body, options, report, all_solid);
    shape = apply_transform(shape, body.transform);
    if (options.fix) {
      Handle(ShapeFix_Shape) fix = new ShapeFix_Shape(shape);
      fix->SetPrecision(options.tolerance);
      fix->SetMaxTolerance(kMaxTolerance);
      // Keep the face orientations of the model, so that the raw volume
      // shows whether they are right; solids are oriented afterwards.
      // (Wires inside a face may still be reoriented.)
      fix->FixShellTool()->FixOrientationMode() = 0;
      fix->FixSolidTool()->FixShellOrientationMode() = 0;
      fix->FixSolidTool()->CreateOpenSolidMode() = false;
      fix->Perform();
      shape = fix->Shape();
    }
    report.solid = all_solid && count_of(shape, TopAbs_SOLID) > 0;
    if (report.solid) {
      if (options.measure) {
        report.raw_volume = volume_of(shape);
      }
      shape = orient_solids(shape);
      if (options.measure) {
        report.volume = volume_of(shape);
      }
    }
    if (!options.measure) {
      report.built = true;
      result.shape = shape;
      return result;
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
    BRepCheck_Analyzer analyzer(shape);
    report.valid = analyzer.IsValid();
    if (!report.valid) {
      for (std::string& p : check_problems(analyzer, shape)) {
        note(report, "check: " + p);
      }
    }
    report.built = true;
    result.shape = shape;
  } catch (const Standard_Failure& e) {
    report.error = failure_text(e);
  } catch (const std::exception& e) {
    report.error = e.what();
  }
  return result;
}

} // namespace mitcad::brep
