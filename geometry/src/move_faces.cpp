// SPDX-License-Identifier: MIT
#include "mitcad/geometry/move_faces.hpp"

#include <algorithm>
#include <cmath>
#include <limits>
#include <optional>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepCheck_Analyzer.hxx>
#include <BRepCheck_Result.hxx>
#include <BRepCheck_Status.hxx>
#include <BRepGProp.hxx>
#include <BRepLib.hxx>
#include <BRepTools_Modification.hxx>
#include <BRepTools_Modifier.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <GeomAPI_IntCS.hxx>
#include <GeomAPI_IntSS.hxx>
#include <GeomAPI_ProjectPointOnCurve.hxx>
#include <GeomProjLib.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom_Curve.hxx>
#include <Geom_Surface.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <NCollection_DataMap.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <NCollection_Map.hxx>
#include <Precision.hxx>
#include <Standard_Failure.hxx>
#include <Standard_OutOfMemory.hxx>
#include <TopAbs.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <gp_Mat.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>
#include <gp_Vec2d.hxx>

#include "face_select.hpp"
#include "history.hpp"
#include "replace.hpp"
#include "util.hpp"

namespace mitcad::geometry {

namespace {

using Ancestors = NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;
using ShapeSet = NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher>;

// How near a new edge or vertex must lie to where its curve or point is
// found to belong to it (the body's units, mm).
constexpr double kNear = 1.0e-6;

// How near a vertex's new point must lie to where the motion takes it to
// count as moved with its faces (mm): far below a vertex's tolerance.
constexpr double kRigid = 1.0e-9;

// A face's surface in global coordinates.
occ::handle<Geom_Surface> global_surface(const TopoDS_Face& face) {
  TopLoc_Location location;
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face, location);
  if (surface.IsNull() || location.IsIdentity()) {
    return surface;
  }
  return occ::down_cast<Geom_Surface>(surface->Transformed(location.Transformation()));
}

// An edge's curve in global coordinates, with its range.
occ::handle<Geom_Curve> global_curve(const TopoDS_Edge& edge, double& first, double& last) {
  TopLoc_Location location;
  occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, location, first, last);
  if (curve.IsNull() || location.IsIdentity()) {
    return curve;
  }
  return occ::down_cast<Geom_Curve>(curve->Transformed(location.Transformation()));
}

// The faces of an edge, each once.
std::vector<TopoDS_Face> faces_of(const Ancestors& map, const TopoDS_Shape& edge) {
  std::vector<TopoDS_Face> faces;
  const int index = map.FindIndex(edge);
  if (index == 0) {
    return faces;
  }
  for (const TopoDS_Shape& face : map(index)) {
    if (std::none_of(faces.begin(), faces.end(), [&](const TopoDS_Face& f) { return f.IsSame(face); })) {
      faces.push_back(TopoDS::Face(face));
    }
  }
  return faces;
}

// The parameter on `curve` of the point nearest `p`, within kNear, near
// `hint` where the curve passes `p` more than once. On a periodic curve
// with a hint, in the period nearest the hint: the projection gives one
// parameter in the curve's first period, and a closed edge's end, its
// start's point a period on, would get its start's (mitcad#2543: a boss's
// top circle moved with its faces came out an edge of no length, and
// "Geom2d_TrimmedCurve::U1 == U2").
std::optional<double> parameter_on(const occ::handle<Geom_Curve>& curve, const gp_Pnt& p, double tolerance,
                                   std::optional<double> hint = std::nullopt) {
  GeomAPI_ProjectPointOnCurve projection(p, curve);
  std::optional<double> best;
  double best_score = std::numeric_limits<double>::infinity();
  for (int i = 1; i <= projection.NbPoints(); ++i) {
    if (projection.Distance(i) > tolerance) {
      continue;
    }
    double w = projection.Parameter(i);
    if (hint && curve->IsPeriodic()) {
      const double period = curve->Period();
      w += std::round((*hint - w) / period) * period;
    }
    const double score = hint ? std::abs(w - *hint) : projection.Distance(i);
    if (score < best_score) {
      best_score = score;
      best = w;
    }
  }
  return best;
}

// The point where `curve` meets `surface` nearest `near`.
std::optional<gp_Pnt> meet(const occ::handle<Geom_Curve>& curve, const occ::handle<Geom_Surface>& surface,
                           const gp_Pnt& near) {
  GeomAPI_IntCS intersection(curve, surface);
  if (!intersection.IsDone()) {
    return std::nullopt;
  }
  std::optional<gp_Pnt> best;
  for (int i = 1; i <= intersection.NbPoints(); ++i) {
    const gp_Pnt p = intersection.Point(i);
    if (!best || p.Distance(near) < best->Distance(near)) {
      best = p;
    }
  }
  return best;
}

class MovedFaces : public BRepTools_Modification {
public:
  // An edge between a moved face and one that stays: its new curve (global)
  // and range.
  struct Edge {
    occ::handle<Geom_Curve> curve;
    double first = 0.0;
    double last = 0.0;
    // The old edge's middle, as moved with the moved faces and as it was,
    // and the part of its curve's period it spans where that is periodic
    // (else negative).
    gp_Pnt near[2];
    double fraction = -1.0;
  };

  MovedFaces(const TopoDS_Shape& body, const ShapeSet& moved, const gp_Trsf& motion)
      : m_moved(moved), m_motion(motion) {
    TopExp::MapShapesAndAncestors(body, TopAbs_EDGE, TopAbs_FACE, m_edgeFaces);
    TopExp::MapShapesAndAncestors(body, TopAbs_VERTEX, TopAbs_EDGE, m_vertexEdges);
    // The edges between moved faces and faces that stay: the surfaces'
    // intersection line next to the edge carried with its moved face, or
    // next to the edge where it was where that is nearer (mitcad#2543).
    // (The edge stays a boundary of the moved face, so the edge moved with
    // it finds the line and gives its way. The edge where it was does
    // not: once a face has moved further than the radius of its curved
    // edge, the old edge's middle lies past the new circle's centre and
    // its nearest point on the circle is on the far side, where the circle
    // runs the other way: a boss moved more than its corners' radius
    // failed, mitcad#2547.)
    for (int i = 1; i <= m_edgeFaces.Extent(); ++i) {
      const TopoDS_Edge edge = TopoDS::Edge(m_edgeFaces.FindKey(i));
      if (BRep_Tool::Degenerated(edge) || kind(edge) != Kind::Between) {
        continue;
      }
      const std::vector<TopoDS_Face> faces = faces_of(m_edgeFaces, edge);
      if (faces.size() != 2) {
        throw std::runtime_error("an edge of the moved faces does not lie between two faces");
      }
      double first = 0.0, last = 0.0;
      const occ::handle<Geom_Curve> old = global_curve(edge, first, last);
      // The new edge lies near the old one's middle, with its direction
      // there, as moved with the moved faces or as it was. A moved face's
      // edges go with it: where the faces slide along the face that stays
      // (a boss along its plate), the moved edge is the new one, while the
      // edge as it was may lie farther from the new one than the new one
      // is curved (mitcad#2543: an arc of radius 3 moved 8 mm across its
      // chord came out running the other way round its circle, the long
      // way between its vertices). The new curve is the line nearest
      // either (the moved one where both are as near); it runs the way the
      // nearer one runs.
      gp_Pnt at;
      gp_Vec along;
      old->D1(0.5 * (first + last), at, along);
      const std::pair<gp_Pnt, gp_Vec> references[2] = {
          {at.Transformed(m_motion), along.Transformed(m_motion)}, {at, along}};
      GeomAPI_IntSS intersection(surface(faces[0]), surface(faces[1]), kNear);
      if (!intersection.IsDone() || intersection.NbLines() == 0) {
        throw std::runtime_error("a moved face no longer meets its neighbour");
      }
      occ::handle<Geom_Curve> best;
      double distance = std::numeric_limits<double>::infinity();
      double parameter = 0.0;
      const gp_Vec* reference = nullptr;
      for (int k = 1; k <= intersection.NbLines(); ++k) {
        const occ::handle<Geom_Curve> line = intersection.Line(k);
        for (const auto& [point, direction] : references) {
          GeomAPI_ProjectPointOnCurve projection(point, line);
          if (projection.NbPoints() > 0 && projection.LowerDistance() < distance) {
            distance = projection.LowerDistance();
            parameter = projection.LowerDistanceParameter();
            reference = &direction;
            best = line;
          }
        }
      }
      if (best.IsNull() || reference == nullptr) {
        throw std::runtime_error("a moved face no longer meets its neighbour");
      }
      // (A trimmed line's parameters are its basis curve's.)
      gp_Pnt p;
      gp_Vec now;
      best->D1(parameter, p, now);
      if (const auto trimmed = occ::down_cast<Geom_TrimmedCurve>(best)) {
        best = trimmed->BasisCurve();
      }
      if (reference->Dot(now) < 0.0) {
        best = best->Reversed();
      }
      Edge data;
      data.curve = best;
      data.near[0] = references[0].first;
      data.near[1] = references[1].first;
      if (old->IsPeriodic()) {
        data.fraction = (last - first) / old->Period();
      }
      m_between.Bind(edge, data);
    }
    // The vertices that move, and the ranges of the edges between.
    for (int i = 1; i <= m_vertexEdges.Extent(); ++i) {
      const TopoDS_Vertex vertex = TopoDS::Vertex(m_vertexEdges.FindKey(i));
      if (const std::optional<gp_Pnt> p = new_point(vertex, m_vertexEdges(i))) {
        m_points.Bind(vertex, *p);
      }
    }
    for (NCollection_DataMap<TopoDS_Shape, Edge, TopTools_ShapeMapHasher>::Iterator it(m_between); it.More();
         it.Next()) {
      const TopoDS_Edge edge = TopoDS::Edge(it.Key());
      Edge& data = m_between.ChangeFind(edge);
      TopoDS_Vertex v1, v2;
      TopExp::Vertices(edge, v1, v2);
      double first = 0.0, last = 0.0;
      global_curve(edge, first, last);
      const double tolerance = std::max(10.0 * BRep_Tool::Tolerance(edge), 1.0e-4);
      const std::optional<double> w1 = parameter_on(data.curve, point(v1), tolerance);
      const std::optional<double> w2 = parameter_on(data.curve, point(v2), tolerance);
      if (!w1 || !w2) {
        throw std::runtime_error("an edge of a moved face does not reach its vertices");
      }
      data.first = *w1;
      data.last = *w2;
      if (data.curve->IsPeriodic()) {
        const double period = data.curve->Period();
        while (data.last <= data.first + Precision::PConfusion()) {
          data.last += period;
        }
        // (A closed edge goes once round; an open one less than once.)
        if (v1.IsSame(v2)) {
          data.last = data.first + period;
        } else if (nearer_the_other_way(data)) {
          // Between its vertices the closed curve runs either way round;
          // the way taken is the one that spans about the part of its
          // period the old edge spanned of its own (a corner's quarter
          // stays about a quarter), or where the old curve is not closed,
          // the one whose middle lies nearer the old edge's middle (as
          // moved or as it was). The tangents at the middle chose it
          // above, but where the faces turn (a boss tilted on its block)
          // the new curve's tangent there can be nearly square to the old
          // one's, and rounding then decided the way: a tilted boss's
          // corners went the long way round on Windows.
          data.curve = data.curve->Reversed();
          const std::optional<double> r1 = parameter_on(data.curve, point(v1), tolerance);
          const std::optional<double> r2 = parameter_on(data.curve, point(v2), tolerance);
          if (!r1 || !r2) {
            throw std::runtime_error("an edge of a moved face does not reach its vertices");
          }
          data.first = *r1;
          data.last = *r2;
          while (data.last <= data.first + Precision::PConfusion()) {
            data.last += period;
          }
        }
      } else if (data.last <= data.first) {
        throw std::runtime_error("an edge of a moved face would turn round");
      }
    }
  }

  // Whether the closed curve's other way round between the edge's new
  // vertices (from `last` to `first` a period on) fits the old edge better
  // than the way from `first` to `last`: spans nearer the part of its
  // period the old edge spanned, or where that is not known, has its
  // middle nearer the old edge's middle.
  static bool nearer_the_other_way(const Edge& data) {
    if (data.fraction > 0.0) {
      const double here = (data.last - data.first) / data.curve->Period();
      return std::abs((1.0 - here) - data.fraction) < std::abs(here - data.fraction);
    }
    const double middle = 0.5 * (data.first + data.last);
    const gp_Pnt here = data.curve->Value(middle);
    const gp_Pnt there = data.curve->Value(middle + 0.5 * data.curve->Period());
    const auto off = [&data](const gp_Pnt& p) {
      return std::min(p.Distance(data.near[0]), p.Distance(data.near[1]));
    };
    return off(there) < off(here);
  }

  bool NewSurface(const TopoDS_Face& F, occ::handle<Geom_Surface>& S, TopLoc_Location& L, double& Tol,
                  bool& RevWires, bool& RevFace) override {
    if (!m_moved.Contains(F)) {
      return false;
    }
    S = surface(F);
    L = TopLoc_Location();
    Tol = BRep_Tool::Tolerance(F);
    RevWires = false;
    RevFace = false;
    return true;
  }

  bool NewCurve(const TopoDS_Edge& E, occ::handle<Geom_Curve>& C, TopLoc_Location& L, double& Tol) override {
    if (BRep_Tool::Degenerated(E)) {
      return false;
    }
    switch (kind(E)) {
    case Kind::Moved: {
      double first = 0.0, last = 0.0;
      C = occ::down_cast<Geom_Curve>(global_curve(E, first, last)->Transformed(m_motion));
      break;
    }
    case Kind::Between:
      C = m_between.Find(E).curve;
      break;
    case Kind::Kept:
      return false;
    }
    L = TopLoc_Location();
    Tol = BRep_Tool::Tolerance(E);
    return true;
  }

  bool NewPoint(const TopoDS_Vertex& V, gp_Pnt& P, double& Tol) override {
    const gp_Pnt* p = m_points.Seek(V);
    if (p == nullptr) {
      return false;
    }
    P = *p;
    Tol = BRep_Tool::Tolerance(V);
    return true;
  }

  bool NewCurve2d(const TopoDS_Edge& E, const TopoDS_Face& F, const TopoDS_Edge&, const TopoDS_Face&,
                  occ::handle<Geom2d_Curve>& C, double& Tol) override {
    const Edge* data = m_between.Seek(E);
    if (data == nullptr) {
      if (!m_moved.Contains(F)) {
        return false;
      }
      // A moved face keeps its parametrization, and so the curves of its
      // edges that move with it (each side's of a seam, by the edge's
      // orientation in it), now on its new surface.
      double f = 0.0, l = 0.0;
      const occ::handle<Geom2d_Curve> old = BRep_Tool::CurveOnSurface(E, F, f, l);
      if (old.IsNull()) {
        return false;
      }
      C = occ::down_cast<Geom2d_Curve>(old->Copy());
      Tol = BRep_Tool::Tolerance(E);
      return true;
    }
    const occ::handle<Geom_Surface> S = surface(F);
    double tolerance = kNear;
    C = GeomProjLib::Curve2d(data->curve, data->first, data->last, S, tolerance);
    if (C.IsNull()) {
      throw std::runtime_error("an edge of a moved face has no curve on its face");
    }
    // Into the period of the old p-curve.
    double f = 0.0, l = 0.0;
    const occ::handle<Geom2d_Curve> old = BRep_Tool::CurveOnSurface(E, F, f, l);
    if (!old.IsNull()) {
      const gp_Pnt2d was = old->Value(0.5 * (f + l));
      const gp_Pnt2d now = C->Value(0.5 * (data->first + data->last));
      gp_Vec2d shift(0.0, 0.0);
      if (S->IsUPeriodic()) {
        shift.SetX(-std::round((now.X() - was.X()) / S->UPeriod()) * S->UPeriod());
      }
      if (S->IsVPeriodic()) {
        shift.SetY(-std::round((now.Y() - was.Y()) / S->VPeriod()) * S->VPeriod());
      }
      if (shift.Magnitude() > 0.0) {
        C->Translate(shift);
      }
    }
    Tol = std::max(BRep_Tool::Tolerance(E), tolerance);
    return true;
  }

  bool NewParameter(const TopoDS_Vertex& V, const TopoDS_Edge& E, double& P, double& Tol) override {
    if (BRep_Tool::Degenerated(E)) {
      return false;
    }
    if (const Edge* data = m_between.Seek(E)) {
      TopoDS_Vertex v1, v2;
      TopExp::Vertices(E, v1, v2);
      // (A closed edge: its vertex at the start as its own curve holds it,
      // forward in the edge taken forward.)
      TopAbs_Orientation own = V.Orientation();
      if (E.Orientation() == TopAbs_REVERSED) {
        own = TopAbs::Reverse(own);
      }
      const bool atStart = V.IsSame(v1) && (!v1.IsSame(v2) || own == TopAbs_FORWARD);
      P = atStart ? data->first : data->last;
      Tol = BRep_Tool::Tolerance(V);
      return true;
    }
    const gp_Pnt* p = m_points.Seek(V);
    if (p == nullptr) {
      return false;
    }
    const double old = BRep_Tool::Parameter(V, E);
    // An edge that moves with its faces keeps its curve's parameters (the
    // curve moved rigidly): a vertex that goes with it stays at its own,
    // also each end of a closed edge.
    if (kind(E) == Kind::Moved && p->Distance(BRep_Tool::Pnt(V).Transformed(m_motion)) <= kRigid) {
      P = old;
      Tol = BRep_Tool::Tolerance(V);
      return true;
    }
    // The vertex moves along the edge's curve (moved with the edge or not).
    double first = 0.0, last = 0.0;
    occ::handle<Geom_Curve> curve = global_curve(E, first, last);
    if (kind(E) == Kind::Moved) {
      curve = occ::down_cast<Geom_Curve>(curve->Transformed(m_motion));
    }
    const std::optional<double> w =
        parameter_on(curve, *p, std::max(10.0 * BRep_Tool::Tolerance(E), 1.0e-4), old);
    if (!w) {
      throw std::runtime_error("a vertex of a moved face leaves its edge");
    }
    P = *w;
    Tol = BRep_Tool::Tolerance(V);
    return true;
  }

  GeomAbs_Shape Continuity(const TopoDS_Edge& E, const TopoDS_Face& F1, const TopoDS_Face& F2, const TopoDS_Edge&,
                           const TopoDS_Face&, const TopoDS_Face&) override {
    return BRep_Tool::Continuity(E, F1, F2);
  }

private:
  enum class Kind { Moved, Between, Kept };

  // Whether an edge's faces all move, some, or none.
  Kind kind(const TopoDS_Edge& edge) const {
    int moved = 0, kept = 0;
    for (const TopoDS_Face& face : faces_of(m_edgeFaces, edge)) {
      (m_moved.Contains(face) ? moved : kept) += 1;
    }
    return kept == 0 ? Kind::Moved : (moved == 0 ? Kind::Kept : Kind::Between);
  }

  // A face's surface (global), moved when the face moves.
  occ::handle<Geom_Surface> surface(const TopoDS_Face& face) const {
    const occ::handle<Geom_Surface> s = global_surface(face);
    return m_moved.Contains(face) ? occ::down_cast<Geom_Surface>(s->Transformed(m_motion)) : s;
  }

  // A vertex's point, moved or not.
  gp_Pnt point(const TopoDS_Vertex& vertex) const {
    const gp_Pnt* p = m_points.Seek(vertex);
    return p != nullptr ? *p : BRep_Tool::Pnt(vertex);
  }

  // Where a vertex goes (none: it stays): with its faces where they all
  // move; on an edge that stays, where it meets a moved face; on a moved
  // edge, where it meets a face that stays; else where an edge between
  // meets another face of the vertex.
  std::optional<gp_Pnt> new_point(const TopoDS_Vertex& vertex, const NCollection_List<TopoDS_Shape>& edges) const {
    std::vector<TopoDS_Face> faces;
    for (const TopoDS_Shape& e : edges) {
      for (const TopoDS_Face& face : faces_of(m_edgeFaces, e)) {
        if (std::none_of(faces.begin(), faces.end(), [&](const TopoDS_Face& f) { return f.IsSame(face); })) {
          faces.push_back(face);
        }
      }
    }
    const gp_Pnt old = BRep_Tool::Pnt(vertex);
    const auto movedFace = [&](bool want) -> std::optional<TopoDS_Face> {
      for (const TopoDS_Face& face : faces) {
        if (m_moved.Contains(face) == want) {
          return face;
        }
      }
      return std::nullopt;
    };
    const std::optional<TopoDS_Face> aMoved = movedFace(true);
    const std::optional<TopoDS_Face> aKept = movedFace(false);
    if (!aMoved) {
      return std::nullopt;
    }
    if (!aKept) {
      return old.Transformed(m_motion);
    }
    const double reach = 1.0e3;
    for (const Kind wanted : {Kind::Kept, Kind::Moved}) {
      for (const TopoDS_Shape& e : edges) {
        const TopoDS_Edge edge = TopoDS::Edge(e);
        if (BRep_Tool::Degenerated(edge) || kind(edge) != wanted) {
          continue;
        }
        double first = 0.0, last = 0.0;
        occ::handle<Geom_Curve> curve = global_curve(edge, first, last);
        gp_Pnt near = old;
        occ::handle<Geom_Surface> other = surface(*aMoved);
        if (wanted == Kind::Moved) {
          curve = occ::down_cast<Geom_Curve>(curve->Transformed(m_motion));
          near = old.Transformed(m_motion);
          other = surface(*aKept);
        }
        if (const std::optional<gp_Pnt> p = meet(curve, other, near); p && p->Distance(near) < reach) {
          return p;
        }
      }
    }
    for (const TopoDS_Shape& e : edges) {
      const Edge* data = m_between.Seek(e);
      if (data == nullptr) {
        continue;
      }
      const std::vector<TopoDS_Face> own = faces_of(m_edgeFaces, e);
      for (const TopoDS_Face& face : faces) {
        if (std::any_of(own.begin(), own.end(), [&](const TopoDS_Face& f) { return f.IsSame(face); })) {
          continue;
        }
        if (const std::optional<gp_Pnt> p = meet(data->curve, surface(face), old); p && p->Distance(old) < reach) {
          return p;
        }
      }
    }
    throw std::runtime_error("a vertex of the moved faces has no place on the moved surfaces");
  }

  ShapeSet m_moved;
  gp_Trsf m_motion;
  Ancestors m_edgeFaces;
  Ancestors m_vertexEdges;
  NCollection_DataMap<TopoDS_Shape, Edge, TopTools_ShapeMapHasher> m_between;
  NCollection_DataMap<TopoDS_Shape, gp_Pnt, TopTools_ShapeMapHasher> m_points;
};

// Whether the checker found a face whose boundaries cross each other or
// lie outside one another: the edges the moved faces brought into a face
// that stays run past its boundary (a boss slid over the edge of the plate
// it stands on, or off it).
bool crosses_a_boundary(const BRepCheck_Analyzer& analyzer, const TopoDS_Shape& shape) {
  for (TopExp_Explorer f(shape, TopAbs_FACE); f.More(); f.Next()) {
    const occ::handle<BRepCheck_Result>& result = analyzer.Result(f.Current());
    if (result.IsNull()) {
      continue;
    }
    for (const BRepCheck_Status status : result->Status()) {
      if (status == BRepCheck_IntersectingWires || status == BRepCheck_InvalidImbricationOfWires) {
        return true;
      }
    }
  }
  return false;
}

// Whether the motion is rigid: its rotation part orthonormal, no scale.
// (A matrix given by its values is a compound transformation in OCCT's
// terms, also when it is a rotation and a translation.)
bool rigid(const gp_Trsf& motion) {
  return std::abs(motion.ScaleFactor() - 1.0) < 1.0e-9;
}

// Whether the motion turns nothing (its rotation part within 1e-12 of the
// identity): a translation.
bool translates(const gp_Trsf& motion) {
  const gp_Mat m = motion.VectorialPart();
  for (int r = 1; r <= 3; ++r) {
    for (int c = 1; c <= 3; ++c) {
      if (std::abs(m.Value(r, c) - (r == c ? 1.0 : 0.0)) > 1.0e-12) {
        return false;
      }
    }
  }
  return true;
}

// Whether the motion moves nothing (within 1e-12 of the identity).
bool still(const gp_Trsf& motion) {
  return motion.TranslationPart().Modulus() <= 1.0e-12 && translates(motion);
}

// The faces moved with the body's topology kept: their surfaces moved,
// the edges between them and the faces that stay rebuilt where the
// surfaces meet, the vertices where those edges meet.
ShapePtr rebuilt(const Shape& body, const std::vector<int>& chosen, const gp_Trsf& motion) {
  // (The faces and edges that do not change are shared with the result,
  // whose tolerances SameParameter may widen: a copy of the input's own.)
  const ShapePtr copy = detail::working_copy(body);
  ShapeSet moved;
  for (const int f : chosen) {
    moved.Add(copy->face(f));
  }
  std::optional<BRepTools_Modifier> modifier;
  try {
    const occ::handle<MovedFaces> modification = new MovedFaces(copy->occt(), moved, motion);
    modifier.emplace(copy->occt(), modification);
  } catch (const Standard_OutOfMemory&) {
    throw;
  } catch (const Standard_Failure& failure) {
    // A failure of the geometry kernel that gives no reason (the panel
    // said only "move faces:"; mitcad#2543) says what failed.
    const char* said = failure.what();
    if (said != nullptr && *said != '\0') {
      throw;
    }
    throw Standard_Failure("the neighbouring faces cannot follow the moved faces");
  }
  if (!modifier->IsDone()) {
    throw std::runtime_error("the neighbouring faces cannot follow the moved faces");
  }
  const TopoDS_Shape result = modifier->ModifiedShape(copy->occt());
  BRepLib::SameParameter(result, kNear, true);
  const BRepCheck_Analyzer analyzer(result, true, true);
  if (!analyzer.IsValid()) {
    // A face that stays whose new boundary crosses itself: the moved
    // faces reach past its edges (a boss moved over the side of the
    // block it stands on), which no rebuilt edge can mend.
    for (int i = 0; i < copy->face_count(); ++i) {
      const TopoDS_Shape& face = copy->face(i);
      if (moved.Contains(face)) {
        continue;
      }
      const TopoDS_Shape now = modifier->ModifiedShape(face);
      if (!now.IsSame(face) && !BRepCheck_Analyzer(now, true, true).IsValid()) {
        throw std::runtime_error("the neighbouring faces cannot follow the moved faces: the moved faces would "
                                 "reach past the edge of a face around them");
      }
    }
    // The edges the moved faces brought into a face that stays run past
    // its boundary (mitcad#2543: a boss slid over the edge of its plate).
    if (crosses_a_boundary(analyzer, result)) {
      throw std::runtime_error("the moved faces would run past the boundary of a neighbouring face");
    }
    try {
      detail::require_valid(result, "moving the faces");
    } catch (const std::exception& error) {
      throw std::runtime_error(std::string("the neighbouring faces cannot follow the moved faces: ") +
                               error.what());
    }
  }
  GProp_GProps props;
  BRepGProp::VolumeProperties(result, props);
  if (!(props.Mass() > 0.0)) {
    throw std::runtime_error("moving the faces leaves no body");
  }
  std::vector<Shape::NamedFace> named;
  named.reserve(static_cast<std::size_t>(body.face_count()));
  for (int i = 0; i < body.face_count(); ++i) {
    named.push_back({modifier->ModifiedShape(copy->face(i)), body.face_names(i)});
  }
  auto out = std::make_shared<Shape>(result, named);
  out->set_checked_valid(true);
  return ShapePtr(out);
}

} // namespace

ShapePtr move_faces_rigidly(const std::string& feature, const Shape& body,
                            const std::vector<std::string>& faces, const gp_Trsf& motion) {
  if (!rigid(motion)) {
    throw std::invalid_argument("the faces' motion must be rigid");
  }
  if (still(motion)) {
    throw std::invalid_argument("the faces' motion moves nothing");
  }
  detail::require_surfaces(body.occt(), "move faces");
  const std::vector<int> chosen = detail::resolve_faces(body, faces, "faces to move");
  return detail::run("move faces", [&] {
    // Planar faces translated along their normal (mitcad#2548): onto the
    // level of another planar face of the body where they end within
    // detail::kPlanarLevel of it (a typed distance rounded in its last
    // digits lands flush, not a fraction of a micrometre past), and across
    // the body's other faces where their own rebuild cannot follow.
    std::optional<gp_Pln> plane;
    bool lands = false;
    gp_Trsf exact = motion;
    if (translates(motion)) {
      const gp_Vec shift = motion.TranslationPart();
      plane = detail::movable_plane(body, chosen, shift);
      if (plane) {
        if (const std::optional<gp_Vec> landed = detail::landing_motion(body, chosen, *plane, shift)) {
          exact.SetTranslation(*landed);
          lands = true;
        }
      }
    }
    const auto own = [&] { return rebuilt(body, chosen, exact); };
    if (!plane) {
      return own();
    }
    // The faces' rebuild keeps the body's topology; where the moved faces'
    // boundaries meet or cross other faces (a raised block's end moved
    // flush with the side of the base under it, or past it), the faces
    // around are trimmed where they meet: the region the faces sweep is
    // added or removed. Faces that land on another face's level may join
    // it, which the rebuild cannot do: there that comes first. Where
    // neither works, the rebuild's reason, which says what the faces
    // around cannot do.
    const auto across = [&] {
      return detail::move_planar_faces(body, chosen, *plane, exact.TranslationPart(), feature);
    };
    return lands ? detail::first_of(across, own, 1) : detail::first_of(own, across, 0);
  });
}

} // namespace mitcad::geometry
