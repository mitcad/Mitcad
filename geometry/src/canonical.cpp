// SPDX-License-Identifier: MIT
#include "canonical.hpp"

#include <algorithm>
#include <cmath>
#include <memory>
#include <optional>
#include <utility>
#include <vector>

#include <BRepCheck_Analyzer.hxx>
#include <BRepTools.hxx>
#include <BRepTools_Modification.hxx>
#include <BRepTools_Modifier.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <GeomConvert_CurveToAnaCurve.hxx>
#include <GeomConvert_SurfToAnaSurf.hxx>
#include <GeomProjLib.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_BezierCurve.hxx>
#include <Geom_BezierSurface.hxx>
#include <Geom_ElementarySurface.hxx>
#include <Geom_RectangularTrimmedSurface.hxx>
#include <Geom_Surface.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <Geom2d_Curve.hxx>
#include <NCollection_DataMap.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>
#include <gp_Vec2d.hxx>

namespace mitcad::geometry::detail {
namespace {

template <class T>
using ShapeData = NCollection_DataMap<TopoDS_Shape, T, TopTools_ShapeMapHasher>;

// A surface or curve is exactly analytic within this (OCCT's confusion
// tolerance): the roundings OCCT walks as B-splines lie within 1e-13 of
// their cylinders, their arcs within 1e-8 of their circles.
constexpr double kExact = 1e-7;

bool spline_surface(const occ::handle<Geom_Surface>& surface) {
  return surface->IsKind(STANDARD_TYPE(Geom_BSplineSurface)) || surface->IsKind(STANDARD_TYPE(Geom_BezierSurface));
}

bool spline_curve(occ::handle<Geom_Curve> curve) {
  while (curve->IsKind(STANDARD_TYPE(Geom_TrimmedCurve))) {
    curve = occ::down_cast<Geom_TrimmedCurve>(curve)->BasisCurve();
  }
  return curve->IsKind(STANDARD_TYPE(Geom_BSplineCurve)) || curve->IsKind(STANDARD_TYPE(Geom_BezierCurve));
}

// The faces' and edges' new geometry; every pcurve on a changed face, and
// of a changed edge, is the projection of the edge's 3D curve, moved into
// the period of the old pcurve (or of the face's middle).
class Canonical : public BRepTools_Modification {
public:
  struct Face {
    occ::handle<Geom_Surface> surface; // in the face's local frame, normal the old one's way
    gp_Pnt2d middle; // the face's middle on the new surface
  };
  struct Edge {
    occ::handle<Geom_Curve> curve; // in global coordinates
    double first = 0.0;
    double last = 0.0;
  };
  ShapeData<Face> faces;
  ShapeData<Edge> edges;

  bool NewSurface(const TopoDS_Face& F, occ::handle<Geom_Surface>& S, TopLoc_Location& L, double& Tol,
                  bool& RevWires, bool& RevFace) override {
    const Face* face = faces.Seek(F);
    if (face == nullptr) {
      return false;
    }
    BRep_Tool::Surface(F, L);
    S = face->surface;
    Tol = std::max(BRep_Tool::Tolerance(F), kExact);
    RevWires = false;
    RevFace = false;
    return true;
  }

  bool NewCurve(const TopoDS_Edge& E, occ::handle<Geom_Curve>& C, TopLoc_Location& L, double& Tol) override {
    const Edge* edge = edges.Seek(E);
    if (edge == nullptr) {
      return false;
    }
    C = edge->curve;
    L = TopLoc_Location();
    Tol = std::max(BRep_Tool::Tolerance(E), kExact);
    return true;
  }

  bool NewPoint(const TopoDS_Vertex&, gp_Pnt&, double&) override { return false; }

  bool NewCurve2d(const TopoDS_Edge& E, const TopoDS_Face& F, const TopoDS_Edge&, const TopoDS_Face&,
                  occ::handle<Geom2d_Curve>& C, double& Tol) override {
    const Face* face = faces.Seek(F);
    const Edge* edge = edges.Seek(E);
    if (face == nullptr && edge == nullptr) {
      return false;
    }
    occ::handle<Geom_Curve> curve;
    double first = 0.0;
    double last = 0.0;
    if (edge != nullptr) {
      curve = edge->curve;
      first = edge->first;
      last = edge->last;
    } else {
      TopLoc_Location location;
      curve = BRep_Tool::Curve(E, location, first, last);
      if (curve.IsNull()) {
        return false;
      }
      curve = occ::down_cast<Geom_Curve>(curve->Transformed(location.Transformation()));
    }
    TopLoc_Location location;
    occ::handle<Geom_Surface> surface = BRep_Tool::Surface(F, location);
    if (face != nullptr) {
      surface = face->surface;
    }
    surface = occ::down_cast<Geom_Surface>(surface->Transformed(location.Transformation()));
    double tolerance = kExact;
    C = GeomProjLib::Curve2d(curve, first, last, surface, tolerance);
    if (C.IsNull()) {
      return false;
    }
    // Into the period where the face's other pcurves lie.
    gp_Pnt2d reference;
    if (face != nullptr) {
      reference = face->middle;
    } else {
      double f = 0.0;
      double l = 0.0;
      const occ::handle<Geom2d_Curve> old = BRep_Tool::CurveOnSurface(E, F, f, l);
      if (old.IsNull()) {
        return false;
      }
      reference = old->Value((f + l) / 2);
    }
    const gp_Pnt2d middle = C->Value((first + last) / 2);
    gp_Vec2d shift(0.0, 0.0);
    if (surface->IsUPeriodic()) {
      shift.SetX(-std::round((middle.X() - reference.X()) / surface->UPeriod()) * surface->UPeriod());
    }
    if (surface->IsVPeriodic()) {
      shift.SetY(-std::round((middle.Y() - reference.Y()) / surface->VPeriod()) * surface->VPeriod());
    }
    if (shift.Magnitude() > 0.0) {
      C->Translate(shift);
    }
    Tol = std::max({BRep_Tool::Tolerance(E), tolerance, kExact});
    return true;
  }

  bool NewParameter(const TopoDS_Vertex& V, const TopoDS_Edge& E, double& P, double& Tol) override {
    const Edge* edge = edges.Seek(E);
    if (edge == nullptr) {
      return false;
    }
    TopoDS_Vertex first;
    TopoDS_Vertex last;
    TopExp::Vertices(E, first, last);
    P = V.IsSame(first) ? edge->first : edge->last;
    Tol = std::max(BRep_Tool::Tolerance(V), kExact);
    return true;
  }

  GeomAbs_Shape Continuity(const TopoDS_Edge& E, const TopoDS_Face& F1, const TopoDS_Face& F2, const TopoDS_Edge&,
                           const TopoDS_Face&, const TopoDS_Face&) override {
    return BRep_Tool::Continuity(E, F1, F2);
  }
};

// The analytic surface of a B-spline face, or none.
std::optional<Canonical::Face> analytic_face(const TopoDS_Face& face, double exact) {
  for (TopExp_Explorer edges(face, TopAbs_EDGE); edges.More(); edges.Next()) {
    if (BRep_Tool::IsClosed(TopoDS::Edge(edges.Current()), face)) {
      return std::nullopt;
    }
  }
  TopLoc_Location location;
  occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face, location);
  if (surface.IsNull()) {
    return std::nullopt;
  }
  if (surface->IsKind(STANDARD_TYPE(Geom_RectangularTrimmedSurface))) {
    surface = occ::down_cast<Geom_RectangularTrimmedSurface>(surface)->BasisSurface();
  }
  if (!spline_surface(surface)) {
    return std::nullopt;
  }
  GeomConvert_SurfToAnaSurf convert(surface);
  const occ::handle<Geom_Surface> analytic = convert.ConvertToAnalytical(exact);
  if (analytic.IsNull() || !analytic->IsKind(STANDARD_TYPE(Geom_ElementarySurface)) || convert.Gap() > exact) {
    return std::nullopt;
  }
  // Which way the new surface's normal points, at the face's middle.
  double u0 = 0.0;
  double u1 = 0.0;
  double v0 = 0.0;
  double v1 = 0.0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  gp_Pnt point;
  gp_Vec du;
  gp_Vec dv;
  surface->D1((u0 + u1) / 2, (v0 + v1) / 2, point, du, dv);
  const gp_Vec normal = du.Crossed(dv);
  GeomAPI_ProjectPointOnSurf projection(point, analytic);
  if (projection.NbPoints() == 0 || projection.LowerDistance() > exact * 10) {
    return std::nullopt;
  }
  double u = 0.0;
  double v = 0.0;
  projection.LowerDistanceParameters(u, v);
  gp_Pnt on;
  analytic->D1(u, v, on, du, dv);
  const gp_Vec analytic_normal = du.Crossed(dv);
  if (normal.Magnitude() < 1e-12 || analytic_normal.Magnitude() < 1e-12) {
    return std::nullopt;
  }
  // The new surface's normal the old one's way (the face keeps its
  // orientation in the shell).
  if (normal.Dot(analytic_normal) < 0.0) {
    analytic->UReverse();
    GeomAPI_ProjectPointOnSurf again(point, analytic);
    if (again.NbPoints() == 0) {
      return std::nullopt;
    }
    again.LowerDistanceParameters(u, v);
  }
  Canonical::Face out;
  out.surface = analytic;
  out.middle = gp_Pnt2d(u, v);
  return out;
}

// The line or circle of a B-spline edge, or none.
std::optional<Canonical::Edge> analytic_edge(const TopoDS_Edge& edge, double exact) {
  if (BRep_Tool::Degenerated(edge)) {
    return std::nullopt;
  }
  double first = 0.0;
  double last = 0.0;
  TopLoc_Location location;
  occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, location, first, last);
  if (curve.IsNull() || !spline_curve(curve)) {
    return std::nullopt;
  }
  curve = occ::down_cast<Geom_Curve>(curve->Transformed(location.Transformation()));
  GeomConvert_CurveToAnaCurve convert(curve);
  occ::handle<Geom_Curve> analytic;
  Canonical::Edge out;
  if (!convert.ConvertToAnalytical(exact, analytic, first, last, out.first, out.last) || analytic.IsNull() ||
      convert.Gap() > exact || spline_curve(analytic) || !(out.first < out.last)) {
    return std::nullopt;
  }
  // The ends where the vertices are.
  TopoDS_Vertex start;
  TopoDS_Vertex end;
  TopExp::Vertices(edge, start, end);
  if (start.IsNull() || end.IsNull() || start.IsSame(end)) {
    return std::nullopt;
  }
  const double tolerance = std::max(BRep_Tool::Tolerance(start), BRep_Tool::Tolerance(end)) + exact;
  if (analytic->Value(out.first).Distance(BRep_Tool::Pnt(start)) > tolerance ||
      analytic->Value(out.last).Distance(BRep_Tool::Pnt(end)) > tolerance) {
    return std::nullopt;
  }
  out.curve = analytic;
  return out;
}

bool elementary(const TopoDS_Face& face) {
  TopLoc_Location location;
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face, location);
  return !surface.IsNull() && surface->IsKind(STANDARD_TYPE(Geom_ElementarySurface));
}

} // namespace

ShapePtr canonical_dressup(const Shape& input, const Shape& result, double exact) {
  try {
    OCC_CATCH_SIGNALS
    const occ::handle<Canonical> modification = new Canonical();
    std::vector<int> changed;
    for (int i = 0; i < result.face_count(); ++i) {
      const TopoDS_Face& face = result.face(i);
      if (input.face_index(face) >= 0) {
        continue;
      }
      if (std::optional<Canonical::Face> analytic = analytic_face(face, exact)) {
        modification->faces.Bind(face, *analytic);
        changed.push_back(i);
      }
    }
    if (changed.empty()) {
      return nullptr;
    }
    // The edges of those faces, where every face at the edge has an
    // analytic surface (the pcurves are then exact projections).
    NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> faces_of;
    TopExp::MapShapesAndUniqueAncestors(result.occt(), TopAbs_EDGE, TopAbs_FACE, faces_of);
    for (const int i : changed) {
      for (TopExp_Explorer edges(result.face(i), TopAbs_EDGE); edges.More(); edges.Next()) {
        const TopoDS_Edge& edge = TopoDS::Edge(edges.Current());
        if (modification->edges.IsBound(edge)) {
          continue;
        }
        const int index = faces_of.FindIndex(edge);
        if (index == 0) {
          continue;
        }
        bool all_analytic = true;
        for (const TopoDS_Shape& face : faces_of(index)) {
          all_analytic = all_analytic && !BRep_Tool::IsClosed(edge, TopoDS::Face(face)) &&
                         (modification->faces.IsBound(face) || elementary(TopoDS::Face(face)));
        }
        if (!all_analytic) {
          continue;
        }
        if (std::optional<Canonical::Edge> analytic = analytic_edge(edge, exact)) {
          modification->edges.Bind(edge, *analytic);
        }
      }
    }
    BRepTools_Modifier modifier(result.occt(), modification);
    if (!modifier.IsDone()) {
      return nullptr;
    }
    // A new edge's tolerance covers how far it lies from its faces' surfaces:
    // an unchanged face's surface can lie off the exact curve by more than
    // the old edge's tolerance (mitcad#339: a stored cone 2.3e-7 off a ring's
    // exact circle, the edge's tolerance 1.7e-7; the cone no longer checked
    // valid).
    {
      NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher> new_faces_of;
      TopExp::MapShapesAndUniqueAncestors(modifier.ModifiedShape(result.occt()), TopAbs_EDGE, TopAbs_FACE,
                                          new_faces_of);
      BRep_Builder builder;
      for (int i = 1; i <= faces_of.Extent(); ++i) {
        // With a wider tolerance (a sweep's sides) also the edges kept on
        // the changed faces: their new pcurves lie as far off their curves
        // as the sweep's surfaces lay off the analytic ones.
        const bool kept_on_changed =
            exact > kExact && std::any_of(faces_of(i).begin(), faces_of(i).end(), [&](const TopoDS_Shape& face) {
              return modification->faces.IsBound(face);
            });
        if (!modification->edges.IsBound(faces_of.FindKey(i)) && !kept_on_changed) {
          continue;
        }
        const TopoDS_Edge edge = TopoDS::Edge(modifier.ModifiedShape(faces_of.FindKey(i)));
        const int index = new_faces_of.FindIndex(edge);
        if (index == 0) {
          continue;
        }
        double first = 0.0;
        double last = 0.0;
        const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last);
        if (curve.IsNull()) {
          continue;
        }
        double deviation = 0.0;
        for (const TopoDS_Shape& face : new_faces_of(index)) {
          double f = 0.0;
          double l = 0.0;
          const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, TopoDS::Face(face), f, l);
          const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(TopoDS::Face(face));
          if (pcurve.IsNull() || surface.IsNull()) {
            continue;
          }
          for (int k = 0; k <= 20; ++k) {
            const double t = first + (last - first) * k / 20.0;
            const gp_Pnt2d uv = pcurve->Value(t);
            deviation = std::max(deviation, surface->Value(uv.X(), uv.Y()).Distance(curve->Value(t)));
          }
        }
        // (The edge is the modifier's new one; its vertices may be the
        // input's, and are left as they are.)
        if (1.1 * deviation > BRep_Tool::Tolerance(edge)) {
          builder.UpdateEdge(edge, 1.1 * deviation);
        }
      }
    }
    for (const int i : changed) {
      if (!BRepCheck_Analyzer(modifier.ModifiedShape(result.face(i))).IsValid()) {
        return nullptr;
      }
    }
    // The other faces of the new edges too (their pcurves are new).
    for (int i = 1; i <= faces_of.Extent(); ++i) {
      if (!modification->edges.IsBound(faces_of.FindKey(i))) {
        continue;
      }
      for (const TopoDS_Shape& face : faces_of(i)) {
        if (!modification->faces.IsBound(face) && !BRepCheck_Analyzer(modifier.ModifiedShape(face)).IsValid()) {
          return nullptr;
        }
      }
    }
    std::vector<Shape::NamedFace> named;
    named.reserve(static_cast<std::size_t>(result.face_count()));
    for (int i = 0; i < result.face_count(); ++i) {
      named.push_back({modifier.ModifiedShape(result.face(i)), result.face_names(i)});
    }
    auto out = std::make_shared<Shape>(modifier.ModifiedShape(result.occt()), named);
    if (out->face_count() != result.face_count()) {
      return nullptr;
    }
    out->set_unified(result.unified());
    out->copy_notes(result);
    return out;
  } catch (const Standard_Failure&) {
    return nullptr;
  }
}

} // namespace mitcad::geometry::detail
