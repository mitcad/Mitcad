// SPDX-License-Identifier: MIT
#include "mitcad/geometry/surface_fills.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>
#include <map>
#include <memory>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_CompCurve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepOffsetAPI_MakeFilling.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRepTools.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <GeomConvert.hxx>
#include <GeomConvert_CompCurveToBSplineCurve.hxx>
#include <GeomFill_Gordon.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_Curve.hxx>
#include <Geom_Surface.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <NCollection_HArray1.hxx>
#include <NCollection_List.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>
#include <gp_Vec2d.hxx>

#include "face_select.hpp"
#include "history.hpp"
#include "mitcad/geometry/naming.hpp"
#include "sheet_edges.hpp"
#include "spine.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

// Faces fitted through points along edges are sewn to their neighbours
// within this.
constexpr double kSewing = 1.0e-3;
// Stations along a chain: about one a millimetre, between these.
constexpr int kFewestStations = 16;
constexpr int kMostStations = 200;
// Rows across a rebuilt face.
constexpr int kRows = 16;

TopoDS_Shell shell_of(const TopoDS_Face& face) {
  TopoDS_Shell shell;
  BRep_Builder builder;
  builder.MakeShell(shell);
  builder.Add(shell, face);
  return shell;
}

ShapePtr one_face(const std::string& feature, const std::string& role, const TopoDS_Face& face) {
  return std::make_shared<Shape>(shell_of(face), std::vector<Shape::NamedFace>{{face, {face_name(feature, role)}}});
}

// A chain's edges joined into one wire, in order.
TopoDS_Wire wire_of(const Shape& body, const std::vector<int>& edges) {
  NCollection_List<TopoDS_Shape> list;
  for (const int e : edges) {
    list.Append(body.edge(e));
  }
  BRepBuilderAPI_MakeWire wire;
  wire.Add(list);
  if (!wire.IsDone()) {
    throw std::invalid_argument("the edges do not join into one chain");
  }
  // Every edge must be in it: a chain, not pieces.
  int count = 0;
  for (TopExp_Explorer it(wire.Wire(), TopAbs_EDGE); it.More(); it.Next()) {
    ++count;
  }
  if (count != static_cast<int>(edges.size())) {
    throw std::invalid_argument("the edges do not join into one chain");
  }
  return wire.Wire();
}

// A point of a chain and the face beside it there: `out` continues the
// face across the chain (unit), `normal` is the face's (its orientation
// applied) and `curvature` the face's normal curvature along `out`.
struct Station {
  gp_Pnt point;
  gp_Vec out;
  gp_Vec normal;
  double curvature = 0.0;
};

// A chain of a body's edges and, for each edge, the face beside it (-1
// where none is needed or known).
struct Chain {
  ShapePtr body; // a working copy: the chain's edges are its
  TopoDS_Wire wire;
  std::map<int, int> face_of; // edge index -> face index
  double length = 0.0;
};

Chain chain_of(const EdgeChain& input, bool faces_needed, const char* what) {
  if (input.body == nullptr) {
    throw std::invalid_argument(std::string(what) + ": no body");
  }
  Chain chain;
  chain.body = detail::working_copy(*input.body);
  const Shape& body = *chain.body;
  const std::vector<int> edges = detail::sheet_edges(body, input.edges, input.openings);
  chain.wire = wire_of(body, edges);
  int face = -1;
  if (!input.face.empty()) {
    const std::vector<int> found = detail::resolve_faces(body, {input.face}, "face");
    if (found.size() != 1) {
      throw std::invalid_argument(face_error(input.face, std::string(what) + ": the face is not one face",
                                             std::string(what) + ": face " + input.face + " is not one face"));
    }
    face = found.front();
  }
  const detail::EdgeFaces edge_faces(body);
  for (const int e : edges) {
    const std::vector<int> beside = edge_faces.at(e);
    int f = -1;
    if (face >= 0) {
      if (std::find(beside.begin(), beside.end(), face) == beside.end()) {
        throw std::invalid_argument(
            placed_error(std::string(what) + ": the face is not beside an edge of the chain",
                         std::string(what) + ": face " + input.face + " is not beside edge " + body.edge_name(e),
                         {face_place(input.face), edge_place(body.edge_name(e))}));
      }
      f = face;
    } else if (beside.size() == 1) {
      f = beside.front();
    } else if (faces_needed) {
      const std::string why = " lies between two faces; name the face beside the chain";
      throw std::invalid_argument(edge_error(body.edge_name(e), std::string(what) + ": an edge" + why,
                                             std::string(what) + ": edge " + body.edge_name(e) + why));
    }
    chain.face_of[e] = f;
  }
  const BRepAdaptor_CompCurve curve(chain.wire);
  chain.length = GCPnts_AbscissaPoint::Length(curve);
  if (!(chain.length > 0.0)) {
    throw std::invalid_argument(std::string(what) + ": the chain has no length");
  }
  return chain;
}

// The station at curve parameter `param` of the chain's wire.
Station station_at(const Chain& chain, const BRepAdaptor_CompCurve& curve, double param, bool with_face) {
  Station s;
  s.point = curve.Value(param);
  if (!with_face) {
    return s;
  }
  TopoDS_Edge edge;
  double on_edge = 0.0;
  curve.Edge(param, edge, on_edge);
  const int e = chain.body->edge_index(edge);
  const auto found = chain.face_of.find(e);
  if (found == chain.face_of.end() || found->second < 0) {
    throw std::invalid_argument("a chain's edge has no face beside it");
  }
  const TopoDS_Face& face = chain.body->face(found->second);
  s.out = gp_Vec(detail::across_edge(edge, face, on_edge));
  double first = 0.0;
  double last = 0.0;
  const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, face, first, last);
  if (pcurve.IsNull()) {
    throw std::runtime_error("an edge has no curve on its face");
  }
  const gp_Pnt2d uv = pcurve->Value(on_edge);
  const BRepAdaptor_Surface surface(face);
  gp_Pnt p;
  gp_Vec du, dv, duu, dvv, duv;
  surface.D2(uv.X(), uv.Y(), p, du, dv, duu, dvv, duv);
  gp_Vec normal = du.Crossed(dv);
  if (normal.Magnitude() < 1.0e-12) {
    throw std::runtime_error("a face has no normal at a chain's edge");
  }
  normal.Normalize();
  if (face.Orientation() == TopAbs_REVERSED) {
    normal.Reverse();
  }
  s.normal = normal;
  // out = a du + b dv; its normal curvature from the second derivatives.
  const double e11 = du.Dot(du);
  const double e12 = du.Dot(dv);
  const double e22 = dv.Dot(dv);
  const double det = e11 * e22 - e12 * e12;
  if (std::abs(det) > 1.0e-18) {
    const double r1 = s.out.Dot(du);
    const double r2 = s.out.Dot(dv);
    const double a = (r1 * e22 - r2 * e12) / det;
    const double b = (r2 * e11 - r1 * e12) / det;
    const gp_Vec second = duu * (a * a) + duv * (2.0 * a * b) + dvv * (b * b);
    s.curvature = second.Dot(normal);
  }
  return s;
}

bool closed(const Chain& chain) { return BRep_Tool::IsClosed(chain.wire); }

// `count + 1` stations at equal steps of length along the chain from the
// fraction `start` of its length, the other way when `reverse` (an open
// chain run back starts at 1; a closed one wraps round).
std::vector<Station> stations(const Chain& chain, int count, bool with_face, double start = 0.0,
                              bool reverse = false) {
  const BRepAdaptor_CompCurve curve(chain.wire);
  const double first = curve.FirstParameter();
  const bool wraps = closed(chain);
  std::vector<Station> result;
  for (int i = 0; i <= count; ++i) {
    const double step = static_cast<double>(i) / count;
    double fraction = reverse ? start - step : start + step;
    if (wraps) {
      fraction -= std::floor(fraction);
      if (i == count) {
        // Back where it started.
        fraction = start - std::floor(start);
      }
    }
    fraction = std::clamp(fraction, 0.0, 1.0);
    const double param = fraction <= 0.0   ? first
                         : fraction >= 1.0 ? curve.LastParameter()
                                           : GCPnts_AbscissaPoint(curve, chain.length * fraction, first).Parameter();
    result.push_back(station_at(chain, curve, param, with_face));
  }
  return result;
}

int station_count(double length) {
  return std::clamp(static_cast<int>(std::ceil(length)), kFewestStations, kMostStations);
}

// Curves through each row of points at the same parameters (so the same
// knots), periodic with the first point not repeated at the end.
std::vector<occ::handle<Geom_BSplineCurve>> interpolate_rows(const std::vector<std::vector<gp_Pnt>>& rows,
                                                             const std::vector<double>& params, bool periodic,
                                                             const std::vector<std::pair<gp_Vec, gp_Vec>>& ends = {}) {
  std::vector<occ::handle<Geom_BSplineCurve>> curves;
  for (std::size_t r = 0; r < rows.size(); ++r) {
    const std::vector<gp_Pnt>& row = rows[r];
    const int n = static_cast<int>(row.size());
    auto points = new NCollection_HArray1<gp_Pnt>(1, n);
    for (int i = 0; i < n; ++i) {
      points->SetValue(i + 1, row[static_cast<std::size_t>(i)]);
    }
    const int np = periodic ? n + 1 : n;
    auto parameters = new NCollection_HArray1<double>(1, np);
    for (int i = 0; i < np; ++i) {
      parameters->SetValue(i + 1, params[static_cast<std::size_t>(i)]);
    }
    GeomAPI_Interpolate interpolate(points, parameters, periodic, 1.0e-9);
    if (!ends.empty()) {
      interpolate.Load(ends[r].first, ends[r].second, false);
    }
    interpolate.Perform();
    if (!interpolate.IsDone()) {
      throw std::runtime_error("points along the chain could not be interpolated");
    }
    curves.push_back(interpolate.Curve());
  }
  return curves;
}

// The tensor surface of rows of control curves (same knots) as poles
// across, with the given knots across.
occ::handle<Geom_BSplineSurface> surface_of(const std::vector<occ::handle<Geom_BSplineCurve>>& rows,
                                            const NCollection_Array1<double>& vknots,
                                            const NCollection_Array1<int>& vmults, int vdegree) {
  const occ::handle<Geom_BSplineCurve>& first = rows.front();
  const int nu = first->NbPoles();
  const int nv = static_cast<int>(rows.size());
  NCollection_Array2<gp_Pnt> poles(1, nu, 1, nv);
  for (int j = 0; j < nv; ++j) {
    const occ::handle<Geom_BSplineCurve>& row = rows[static_cast<std::size_t>(j)];
    if (row->NbPoles() != nu || row->Degree() != first->Degree()) {
      throw std::runtime_error("the rows of a fitted surface do not match");
    }
    for (int i = 1; i <= nu; ++i) {
      poles(i, j + 1) = row->Pole(i);
    }
  }
  return new Geom_BSplineSurface(poles, first->Knots(), vknots, first->Multiplicities(), vmults, first->Degree(),
                                 vdegree, first->IsPeriodic(), false);
}

TopoDS_Face face_of(const occ::handle<Geom_Surface>& surface) {
  BRepBuilderAPI_MakeFace make(surface, 1.0e-7);
  if (!make.IsDone()) {
    throw std::runtime_error("the surface makes no face");
  }
  return make.Face();
}

// --- Blend Surface

ShapePtr blend(const std::string& feature, const BlendEnd& from, const BlendEnd& to) {
  const bool from_faces = from.continuity != Continuity::G0;
  const bool to_faces = to.continuity != Continuity::G0;
  const Chain a = chain_of(from.chain, from_faces, "blend surface");
  const Chain b = chain_of(to.chain, to_faces, "blend surface");
  if (closed(a) != closed(b)) {
    throw std::invalid_argument("blend surface: one chain is closed and the other is not");
  }
  const bool periodic = closed(a);
  const int n = station_count(std::max(a.length, b.length));
  const std::vector<Station> sa = stations(a, n, from_faces);
  std::vector<Station> sb;
  if (periodic) {
    // Start the second chain nearest the first's start, running the way
    // the first does.
    const std::vector<Station> fine = stations(b, 4 * n, false);
    std::size_t nearest = 0;
    for (std::size_t i = 1; i < fine.size(); ++i) {
      if (fine[i].point.Distance(sa.front().point) < fine[nearest].point.Distance(sa.front().point)) {
        nearest = i;
      }
    }
    const double start = static_cast<double>(nearest) / (4 * n);
    const std::vector<Station> ahead = stations(b, n, false, start, false);
    const std::size_t q = static_cast<std::size_t>(n / 4);
    const bool reverse = ahead[q].point.Distance(sa[q].point) >
                         stations(b, n, false, start, true)[q].point.Distance(sa[q].point);
    sb = stations(b, n, to_faces, start, reverse);
  } else {
    const std::vector<Station> ends = stations(b, 1, false);
    const double same = sa.front().point.Distance(ends[0].point) + sa.back().point.Distance(ends[1].point);
    const double crossed = sa.front().point.Distance(ends[1].point) + sa.back().point.Distance(ends[0].point);
    sb = crossed < same ? stations(b, n, to_faces, 1.0, true) : stations(b, n, to_faces);
  }
  const bool quintic = from.continuity == Continuity::G2 || to.continuity == Continuity::G2;
  const int degree = quintic ? 5 : 3;
  const int count = periodic ? n : n + 1; // a closed chain's last station is its first
  std::vector<std::vector<gp_Pnt>> rows(static_cast<std::size_t>(degree + 1),
                                        std::vector<gp_Pnt>(static_cast<std::size_t>(count)));
  for (int i = 0; i < count; ++i) {
    const Station& p = sa[static_cast<std::size_t>(i)];
    const Station& q = sb[static_cast<std::size_t>(i)];
    const gp_Vec chord(p.point, q.point);
    const double length = chord.Magnitude();
    const gp_Vec t0 = from_faces ? p.out * (length * (1.0 + from.bulge)) : chord;
    const gp_Vec t1 = to_faces ? -q.out * (length * (1.0 + to.bulge)) : chord;
    const gp_Vec a0 = from.continuity == Continuity::G2 ? p.normal * (p.curvature * t0.SquareMagnitude()) : gp_Vec();
    const gp_Vec a1 = to.continuity == Continuity::G2 ? q.normal * (q.curvature * t1.SquareMagnitude()) : gp_Vec();
    const auto at = [&](int k) -> gp_Pnt& { return rows[static_cast<std::size_t>(k)][static_cast<std::size_t>(i)]; };
    if (quintic) {
      at(0) = p.point;
      at(1) = p.point.Translated(t0 / 5.0);
      at(2) = p.point.Translated(t0 * 0.4 + a0 / 20.0);
      at(3) = q.point.Translated(-t1 * 0.4 + a1 / 20.0);
      at(4) = q.point.Translated(-t1 / 5.0);
      at(5) = q.point;
    } else {
      at(0) = p.point;
      at(1) = p.point.Translated(t0 / 3.0);
      at(2) = q.point.Translated(-t1 / 3.0);
      at(3) = q.point;
    }
  }
  std::vector<double> params;
  for (int i = 0; i <= n; ++i) {
    params.push_back(static_cast<double>(i) / n);
  }
  const std::vector<occ::handle<Geom_BSplineCurve>> curves = interpolate_rows(rows, params, periodic);
  NCollection_Array1<double> vknots(1, 2);
  vknots(1) = 0.0;
  vknots(2) = 1.0;
  NCollection_Array1<int> vmults(1, 2);
  vmults(1) = degree + 1;
  vmults(2) = degree + 1;
  return one_face(feature, "blend", face_of(surface_of(curves, vknots, vmults, degree)));
}

// --- Boundary Surface

// A big planar face on the plane, holding the wire's box.
TopoDS_Face plane_face(const gp_Pln& plane, const TopoDS_Shape& around) {
  Bnd_Box box;
  BRepBndLib::Add(around, box);
  double x0 = 0, y0 = 0, z0 = 0, x1 = 0, y1 = 0, z1 = 0;
  box.Get(x0, y0, z0, x1, y1, z1);
  const double size = 10.0 * gp_Pnt(x0, y0, z0).Distance(gp_Pnt(x1, y1, z1)) +
                      plane.Location().Distance(gp_Pnt((x0 + x1) / 2, (y0 + y1) / 2, (z0 + z1) / 2)) + 100.0;
  return BRepBuilderAPI_MakeFace(plane, -size, size, -size, size).Face();
}

GeomAbs_Shape order_of(Continuity continuity) {
  switch (continuity) {
  case Continuity::G1:
    return GeomAbs_G1;
  case Continuity::G2:
    return GeomAbs_G2;
  case Continuity::G0:
    break;
  }
  return GeomAbs_C0;
}

std::vector<TopoDS_Edge> wire_edges(const TopoDS_Wire& wire) {
  std::vector<TopoDS_Edge> edges;
  for (BRepTools_WireExplorer it(wire); it.More(); it.Next()) {
    edges.push_back(it.Current());
  }
  return edges;
}

ShapePtr fill(const std::string& feature, const std::vector<BoundaryCurve>& loop) {
  BRepOffsetAPI_MakeFilling filling(3, 15, 2, false, 1.0e-5, 1.0e-4, 1.0e-2, 0.1, 8, 9);
  std::vector<Chain> chains; // their copies stay while the filling runs
  chains.reserve(loop.size());
  for (const BoundaryCurve& curve : loop) {
    switch (curve.kind) {
    case BoundaryCurve::Kind::Point:
      throw std::invalid_argument("boundary surface: a boundary loop is made of curves, not points");
    case BoundaryCurve::Kind::Curves: {
      const TopoDS_Wire wire = detail::path_wire(curve.curves);
      const TopoDS_Face support = plane_face(curve.plane, wire);
      for (const TopoDS_Edge& edge : wire_edges(wire)) {
        if (curve.continuity == Continuity::G0) {
          filling.Add(edge, GeomAbs_C0, true);
        } else {
          filling.Add(edge, support, order_of(curve.continuity), true);
        }
      }
      break;
    }
    case BoundaryCurve::Kind::Edges: {
      chains.push_back(chain_of(curve.edges, curve.continuity != Continuity::G0, "boundary surface"));
      const Chain& chain = chains.back();
      for (const TopoDS_Edge& edge : wire_edges(chain.wire)) {
        if (curve.continuity == Continuity::G0) {
          filling.Add(edge, GeomAbs_C0, true);
        } else {
          const int e = chain.body->edge_index(edge);
          filling.Add(edge, chain.body->face(chain.face_of.at(e)), order_of(curve.continuity), true);
        }
      }
      break;
    }
    }
  }
  detail::build(filling);
  if (!filling.IsDone()) {
    throw std::runtime_error("boundary surface: the loop could not be filled");
  }
  return one_face(feature, "boundary", TopoDS::Face(filling.Shape()));
}

// One B-spline curve along a wire's edges in order.
occ::handle<Geom_BSplineCurve> curve_of(const TopoDS_Wire& wire) {
  GeomConvert_CompCurveToBSplineCurve joined;
  bool any = false;
  for (const TopoDS_Edge& edge : wire_edges(wire)) {
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    TopLoc_Location location;
    double first = 0.0;
    double last = 0.0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, location, first, last);
    if (curve.IsNull()) {
      throw std::runtime_error("boundary surface: an edge has no curve");
    }
    occ::handle<Geom_Curve> placed = occ::down_cast<Geom_Curve>(curve->Transformed(location.Transformation()));
    occ::handle<Geom_BSplineCurve> piece =
        GeomConvert::CurveToBSplineCurve(new Geom_TrimmedCurve(placed, first, last));
    if (edge.Orientation() == TopAbs_REVERSED) {
      piece->Reverse();
    }
    if (!joined.Add(piece, 1.0e-5, true)) {
      throw std::invalid_argument("boundary surface: the curves of a boundary do not join end to end");
    }
    any = true;
  }
  if (!any) {
    throw std::invalid_argument("boundary surface: a boundary has no curves");
  }
  return joined.BSplineCurve();
}

// A boundary's wire (sketch curves or a body's edges); `keep` holds the
// copy the edges are of.
TopoDS_Wire boundary_wire(const BoundaryCurve& curve, std::vector<Chain>& keep) {
  if (curve.kind == BoundaryCurve::Kind::Curves) {
    return detail::path_wire(curve.curves);
  }
  keep.push_back(chain_of(curve.edges, false, "boundary surface"));
  return keep.back().wire;
}

ShapePtr network(const std::string& feature, const std::vector<BoundaryCurve>& u,
                 const std::vector<BoundaryCurve>& v) {
  if (u.size() < 2) {
    throw std::invalid_argument("boundary surface: a network needs two curves or more one way");
  }
  if (v.size() == 1) {
    throw std::invalid_argument("boundary surface: the crossing curves must be two or more");
  }
  std::vector<Chain> keep;
  if (v.empty()) {
    BRepOffsetAPI_ThruSections loft(false, false, 1.0e-6);
    loft.CheckCompatibility(false);
    for (std::size_t i = 0; i < u.size(); ++i) {
      const BoundaryCurve& curve = u[i];
      if (curve.kind == BoundaryCurve::Kind::Point) {
        if (i != 0 && i + 1 != u.size()) {
          throw std::invalid_argument("boundary surface: only the first or the last curve can be a point");
        }
        loft.AddVertex(BRepBuilderAPI_MakeVertex(curve.point).Vertex());
        continue;
      }
      const TopoDS_Edge edge = BRepBuilderAPI_MakeEdge(curve_of(boundary_wire(curve, keep))).Edge();
      loft.AddWire(BRepBuilderAPI_MakeWire(edge).Wire());
    }
    detail::build(loft);
    if (!loft.IsDone()) {
      throw std::runtime_error("boundary surface: the curves could not be lofted");
    }
    TopExp_Explorer faces(loft.Shape(), TopAbs_FACE);
    if (!faces.More()) {
      throw std::runtime_error("boundary surface: the loft made no face");
    }
    const TopoDS_Face face = TopoDS::Face(faces.Current());
    faces.Next();
    if (faces.More()) {
      throw std::runtime_error("boundary surface: the loft made more than one face");
    }
    return one_face(feature, "boundary", face);
  }
  NCollection_Array1<occ::handle<Geom_Curve>> profiles(1, static_cast<int>(u.size()));
  NCollection_Array1<occ::handle<Geom_Curve>> guides(1, static_cast<int>(v.size()));
  for (std::size_t i = 0; i < u.size(); ++i) {
    if (u[i].kind == BoundaryCurve::Kind::Point) {
      throw std::invalid_argument("boundary surface: a network with crossing curves has no points");
    }
    profiles(static_cast<int>(i) + 1) = curve_of(boundary_wire(u[i], keep));
  }
  for (std::size_t i = 0; i < v.size(); ++i) {
    if (v[i].kind == BoundaryCurve::Kind::Point) {
      throw std::invalid_argument("boundary surface: a network with crossing curves has no points");
    }
    guides(static_cast<int>(i) + 1) = curve_of(boundary_wire(v[i], keep));
  }
  GeomFill_Gordon gordon;
  gordon.Init(profiles, guides, 1.0e-3);
  gordon.Perform();
  if (!gordon.IsDone()) {
    throw std::invalid_argument("boundary surface: no surface runs through the network (every curve one way "
                                "must cross every curve the other way)");
  }
  return one_face(feature, "boundary", face_of(gordon.Surface()));
}

// --- Match Surface and Fair Surface

// Whether the face is bounded by its surface's parameter range alone.
bool natural(const TopoDS_Face& face) {
  double u1 = 0, u2 = 0, v1 = 0, v2 = 0;
  BRepTools::UVBounds(face, u1, u2, v1, v2);
  const double tu = 1.0e-7 * std::max(1.0, u2 - u1);
  const double tv = 1.0e-7 * std::max(1.0, v2 - v1);
  int wires = 0;
  for (TopExp_Explorer w(face, TopAbs_WIRE); w.More(); w.Next()) {
    ++wires;
  }
  if (wires != 1) {
    return false;
  }
  for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    double first = 0.0;
    double last = 0.0;
    const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, face, first, last);
    if (pcurve.IsNull()) {
      return false;
    }
    std::array<bool, 4> on{true, true, true, true};
    for (int i = 0; i <= 4; ++i) {
      const gp_Pnt2d uv = pcurve->Value(first + (last - first) * i / 4);
      on[0] = on[0] && std::abs(uv.X() - u1) <= tu;
      on[1] = on[1] && std::abs(uv.X() - u2) <= tu;
      on[2] = on[2] && std::abs(uv.Y() - v1) <= tv;
      on[3] = on[3] && std::abs(uv.Y() - v2) <= tv;
    }
    if (!on[0] && !on[1] && !on[2] && !on[3]) {
      return false;
    }
  }
  return true;
}

// The face's parameters as (s, t) in [0, 1]^2: s along a side, t from it
// to the opposite side.
struct SideMap {
  double u0 = 0, v0 = 0;   // (s, t) = (0, 0)
  gp_Vec2d along, across; // d(u, v)/ds, d(u, v)/dt

  gp_Pnt2d uv(double s, double t) const {
    return gp_Pnt2d(u0 + along.X() * s + across.X() * t, v0 + along.Y() * s + across.Y() * t);
  }
};

SideMap side_map(int side, double u1, double u2, double v1, double v2) {
  const double du = u2 - u1;
  const double dv = v2 - v1;
  switch (side) {
  case 0: // u = u1
    return {u1, v1, gp_Vec2d(0, dv), gp_Vec2d(du, 0)};
  case 1: // u = u2
    return {u2, v1, gp_Vec2d(0, dv), gp_Vec2d(-du, 0)};
  case 2: // v = v1
    return {u1, v1, gp_Vec2d(du, 0), gp_Vec2d(0, dv)};
  default: // v = v2
    return {u1, v2, gp_Vec2d(du, 0), gp_Vec2d(0, -dv)};
  }
}

// The point and the derivatives along t of the face at (s, t).
struct Sample {
  gp_Pnt p;
  gp_Vec dt;
  gp_Vec dtt;
};

Sample sample(const BRepAdaptor_Surface& surface, const SideMap& map, double s, double t) {
  const gp_Pnt2d uv = map.uv(s, t);
  gp_Pnt p;
  gp_Vec du, dv, duu, dvv, duv;
  surface.D2(uv.X(), uv.Y(), p, du, dv, duu, dvv, duv);
  const double bx = map.across.X();
  const double by = map.across.Y();
  return {p, du * bx + dv * by, duu * (bx * bx) + duv * (2.0 * bx * by) + dvv * (by * by)};
}

// Quintic Hermite bases on [0, 1]: h0 is 1 at 0, h1 has slope 1 there, h2
// a second derivative of 1; all three and their first two derivatives are
// 0 at 1, h1 and h2 are 0 at 0, as are h0's derivatives.
double h0(double t) { return 1.0 - 10.0 * t * t * t + 15.0 * t * t * t * t - 6.0 * t * t * t * t * t; }
double h1(double t) { return t - 6.0 * t * t * t + 8.0 * t * t * t * t - 3.0 * t * t * t * t * t; }
double h2(double t) { return 0.5 * (t * t - 3.0 * t * t * t + 3.0 * t * t * t * t - t * t * t * t * t); }

TopoDS_Face matched_face(const TopoDS_Face& face, const Chain& target, Continuity continuity) {
  double u1 = 0, u2 = 0, v1 = 0, v2 = 0;
  BRepTools::UVBounds(face, u1, u2, v1, v2);
  const BRepAdaptor_Surface surface(face);
  // The side nearest the chain.
  int side = -1;
  double best = 0.0;
  for (int k = 0; k < 4; ++k) {
    const SideMap map = side_map(k, u1, u2, v1, v2);
    double sum = 0.0;
    for (int i = 0; i <= 8; ++i) {
      const gp_Pnt2d uv = map.uv(i / 8.0, 0.0);
      BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(surface.Value(uv.X(), uv.Y())).Vertex(),
                                          target.wire);
      if (!distance.IsDone()) {
        throw std::runtime_error("match surface: the distance to the chain could not be measured");
      }
      sum += distance.Value();
    }
    if (side < 0 || sum < best) {
      side = k;
      best = sum;
    }
  }
  const SideMap map = side_map(side, u1, u2, v1, v2);
  const bool with_face = continuity != Continuity::G0;
  const int n = station_count(target.length);
  const gp_Pnt start = sample(surface, map, 0.0, 0.0).p;
  const gp_Pnt end = sample(surface, map, 1.0, 0.0).p;
  const std::vector<Station> ends = stations(target, 1, false);
  const bool reverse = start.Distance(ends[1].point) + end.Distance(ends[0].point) <
                       start.Distance(ends[0].point) + end.Distance(ends[1].point);
  if (closed(target)) {
    throw std::invalid_argument("unsupported: match surface to a closed chain of edges");
  }
  const std::vector<Station> seam = stations(target, n, with_face, reverse ? 1.0 : 0.0, reverse);
  std::vector<double> s_params;
  std::vector<double> t_params;
  for (int i = 0; i <= n; ++i) {
    s_params.push_back(static_cast<double>(i) / n);
  }
  for (int j = 0; j <= kRows; ++j) {
    t_params.push_back(static_cast<double>(j) / kRows);
  }
  // The corrections along the seam: position, slope and bending across.
  std::vector<gp_Vec> d(static_cast<std::size_t>(n + 1));
  std::vector<gp_Vec> e(static_cast<std::size_t>(n + 1));
  std::vector<gp_Vec> f(static_cast<std::size_t>(n + 1));
  std::vector<std::vector<gp_Pnt>> rows(static_cast<std::size_t>(kRows + 3), std::vector<gp_Pnt>(static_cast<std::size_t>(n + 1)));
  for (int i = 0; i <= n; ++i) {
    const std::size_t k = static_cast<std::size_t>(i);
    const Sample at = sample(surface, map, s_params[k], 0.0);
    const Station& to = seam[k];
    d[k] = gp_Vec(at.p, to.point);
    if (with_face) {
      const double speed = at.dt.Magnitude();
      e[k] = to.out * speed - at.dt;
      if (continuity == Continuity::G2) {
        f[k] = to.normal * (to.curvature * speed * speed - at.dtt.Dot(to.normal));
      }
    }
    for (int j = 0; j <= kRows; ++j) {
      const double t = t_params[static_cast<std::size_t>(j)];
      const Sample here = sample(surface, map, s_params[k], t);
      rows[static_cast<std::size_t>(j)][k] =
          here.p.Translated(d[k] * h0(t) + e[k] * h1(t) + f[k] * h2(t));
    }
    // The slopes across at both sides, fitted as the sides moved by them
    // (the fit is linear: their poles less the sides' are the slopes'; a
    // row of equal vectors is no row of points).
    const gp_Vec slope0 = at.dt + e[k];
    const gp_Vec slope1 = sample(surface, map, s_params[k], 1.0).dt;
    rows[static_cast<std::size_t>(kRows + 1)][k] = rows.front()[k].Translated(slope0);
    rows[static_cast<std::size_t>(kRows + 2)][k] = rows[static_cast<std::size_t>(kRows)][k].Translated(slope1);
  }
  // Each row through its points (the same knots along s), then each column
  // of their poles through t, its slopes at both ends given.
  const std::vector<occ::handle<Geom_BSplineCurve>> along = interpolate_rows(rows, s_params, false);
  const int np = along.front()->NbPoles();
  std::vector<std::vector<gp_Pnt>> columns(static_cast<std::size_t>(np), std::vector<gp_Pnt>(static_cast<std::size_t>(kRows + 1)));
  std::vector<std::pair<gp_Vec, gp_Vec>> slopes;
  for (int p = 1; p <= np; ++p) {
    for (int j = 0; j <= kRows; ++j) {
      columns[static_cast<std::size_t>(p - 1)][static_cast<std::size_t>(j)] = along[static_cast<std::size_t>(j)]->Pole(p);
    }
    slopes.emplace_back(gp_Vec(along.front()->Pole(p), along[static_cast<std::size_t>(kRows + 1)]->Pole(p)),
                        gp_Vec(along[static_cast<std::size_t>(kRows)]->Pole(p),
                               along[static_cast<std::size_t>(kRows + 2)]->Pole(p)));
  }
  const std::vector<occ::handle<Geom_BSplineCurve>> across = interpolate_rows(columns, t_params, false, slopes);
  // Poles (s, t): column p's pole q.
  const occ::handle<Geom_BSplineCurve>& first_row = along.front();
  const occ::handle<Geom_BSplineCurve>& first_column = across.front();
  NCollection_Array2<gp_Pnt> poles(1, np, 1, first_column->NbPoles());
  for (int p = 1; p <= np; ++p) {
    for (int q = 1; q <= first_column->NbPoles(); ++q) {
      poles(p, q) = across[static_cast<std::size_t>(p - 1)]->Pole(q);
    }
  }
  const occ::handle<Geom_BSplineSurface> rebuilt =
      new Geom_BSplineSurface(poles, first_row->Knots(), first_column->Knots(), first_row->Multiplicities(),
                              first_column->Multiplicities(), first_row->Degree(), first_column->Degree());
  TopoDS_Face result = face_of(rebuilt);
  // Facing as the face did.
  gp_Pnt p;
  gp_Vec a, b;
  const gp_Pnt2d mid = map.uv(0.5, 0.5);
  surface.D1(mid.X(), mid.Y(), p, a, b);
  gp_Vec was = a.Crossed(b);
  if (face.Orientation() == TopAbs_REVERSED) {
    was.Reverse();
  }
  gp_Vec du, dv;
  rebuilt->D1(0.5, 0.5, p, du, dv);
  if (du.Crossed(dv).Dot(was) < 0.0) {
    result.Reverse();
  }
  return result;
}

// The control net faired in place (see Fairing); true when it moved.
bool fair_net(Geom_BSplineSurface& surface, const Fairing& fairing) {
  const int nu = surface.NbUPoles();
  const int nv = surface.NbVPoles();
  const int fixed = fairing.keep_edges ? 2 : 1;
  if (nu <= 2 * fixed || nv <= 2 * fixed) {
    return false;
  }
  std::vector<std::vector<gp_XYZ>> net(static_cast<std::size_t>(nu), std::vector<gp_XYZ>(static_cast<std::size_t>(nv)));
  for (int i = 0; i < nu; ++i) {
    for (int j = 0; j < nv; ++j) {
      net[static_cast<std::size_t>(i)][static_cast<std::size_t>(j)] = surface.Pole(i + 1, j + 1).XYZ();
    }
  }
  const std::vector<std::vector<gp_XYZ>> original = net;
  const auto inner = [&](int i, int j) { return i > 0 && j > 0 && i < nu - 1 && j < nv - 1; };
  const auto movable = [&](int i, int j) { return i >= fixed && j >= fixed && i < nu - fixed && j < nv - fixed; };
  const double step = 0.25 * fairing.strength;
  for (int it = 0; it < fairing.iterations; ++it) {
    // The net's Laplacian at inner points (0 on the boundary), then its
    // Laplacian: a step down the bending energy.
    std::vector<std::vector<gp_XYZ>> lap(static_cast<std::size_t>(nu), std::vector<gp_XYZ>(static_cast<std::size_t>(nv)));
    const auto at = [&](const std::vector<std::vector<gp_XYZ>>& grid, int i, int j) -> const gp_XYZ& {
      return grid[static_cast<std::size_t>(i)][static_cast<std::size_t>(j)];
    };
    for (int i = 1; i < nu - 1; ++i) {
      for (int j = 1; j < nv - 1; ++j) {
        lap[static_cast<std::size_t>(i)][static_cast<std::size_t>(j)] =
            (at(net, i - 1, j) + at(net, i + 1, j) + at(net, i, j - 1) + at(net, i, j + 1)) * 0.25 - at(net, i, j);
      }
    }
    for (int i = 0; i < nu; ++i) {
      for (int j = 0; j < nv; ++j) {
        if (!movable(i, j) || !inner(i, j)) {
          continue;
        }
        const gp_XYZ bilap =
            (at(lap, i - 1, j) + at(lap, i + 1, j) + at(lap, i, j - 1) + at(lap, i, j + 1)) * 0.25 - at(lap, i, j);
        gp_XYZ moved = at(net, i, j) - bilap * step;
        const gp_XYZ from = at(original, i, j);
        const gp_XYZ offset = moved - from;
        const double length = offset.Modulus();
        if (length > fairing.tolerance) {
          moved = from + offset * (fairing.tolerance / length);
        }
        net[static_cast<std::size_t>(i)][static_cast<std::size_t>(j)] = moved;
      }
    }
  }
  bool changed = false;
  for (int i = 0; i < nu; ++i) {
    for (int j = 0; j < nv; ++j) {
      const gp_XYZ& p = net[static_cast<std::size_t>(i)][static_cast<std::size_t>(j)];
      if ((p - original[static_cast<std::size_t>(i)][static_cast<std::size_t>(j)]).Modulus() > 0.0) {
        changed = true;
        surface.SetPole(i + 1, j + 1, gp_Pnt(p));
      }
    }
  }
  return changed;
}

// The face of a B-spline surface faired, or a null face when it is of
// another type or nothing moved.
TopoDS_Face faired_face(const Shape& body, int f, const Fairing& fairing) {
  const TopoDS_Face& face = body.face(f);
  const BRepAdaptor_Surface adaptor(face);
  if (adaptor.GetType() != GeomAbs_BSplineSurface && adaptor.GetType() != GeomAbs_BezierSurface) {
    return TopoDS_Face();
  }
  if (!natural(face)) {
    const std::string why = ", which is not bounded by its surface's parameter range alone";
    throw std::invalid_argument(face_error(detail::primary_name(body, f), "unsupported: fair surface of a face" + why,
                                           "unsupported: fair surface of face " + detail::primary_name(body, f) + why));
  }
  TopLoc_Location location;
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face, location);
  occ::handle<Geom_BSplineSurface> net = GeomConvert::SurfaceToBSplineSurface(surface);
  net = occ::down_cast<Geom_BSplineSurface>(net->Copy());
  double u1 = 0, u2 = 0, v1 = 0, v2 = 0;
  BRepTools::UVBounds(face, u1, u2, v1, v2);
  net->Segment(u1, u2, v1, v2);
  if (!fair_net(*net, fairing)) {
    return TopoDS_Face();
  }
  if (!location.IsIdentity()) {
    net = occ::down_cast<Geom_BSplineSurface>(net->Transformed(location.Transformation()));
  }
  BRepBuilderAPI_MakeFace make(net, u1, u2, v1, v2, 1.0e-7);
  if (!make.IsDone()) {
    throw std::runtime_error("fair surface: the faired surface makes no face");
  }
  TopoDS_Face result = make.Face();
  if (face.Orientation() == TopAbs_REVERSED) {
    result.Reverse();
  }
  return result;
}

// The body's faces with some replaced, sewn again; names kept.
ShapePtr replaced(const Shape& body, const std::map<int, TopoDS_Face>& faces) {
  std::vector<TopoDS_Shape> parts;
  std::vector<Shape::NamedFace> named;
  for (int f = 0; f < body.face_count(); ++f) {
    const auto found = faces.find(f);
    const TopoDS_Shape face = found == faces.end() ? TopoDS_Shape(body.face(f)) : TopoDS_Shape(found->second);
    parts.push_back(face);
    named.push_back({face, body.face_names(f)});
  }
  const Shape pieces(detail::compound_of(parts), named);
  return stitch({&pieces}, kSewing);
}

void require_sheet(const Shape& body, const char* what) {
  detail::require_surfaces(body.occt(), what);
  if (TopExp_Explorer(body.occt(), TopAbs_SOLID).More()) {
    throw std::invalid_argument(std::string(what) + ": the body is a solid; only surface bodies are changed");
  }
}

} // namespace

ShapePtr blend_surface(const std::string& feature, const BlendEnd& from, const BlendEnd& to) {
  for (const BlendEnd* end : {&from, &to}) {
    if (end->chain.body == nullptr) {
      throw std::invalid_argument("blend surface: a side has no body");
    }
    detail::require_surfaces(end->chain.body->occt(), "blend surface");
    if (!(end->bulge > -1.0) || !std::isfinite(end->bulge)) {
      throw std::invalid_argument("blend surface: a bulge must be more than -1");
    }
  }
  return detail::run("blend surface", [&] { return blend(feature, from, to); });
}

ShapePtr boundary_fill(const std::string& feature, const std::vector<BoundaryCurve>& loop) {
  if (loop.empty()) {
    throw std::invalid_argument("boundary surface: a boundary loop needs its curves");
  }
  return detail::run("boundary surface", [&] { return fill(feature, loop); });
}

ShapePtr boundary_network(const std::string& feature, const std::vector<BoundaryCurve>& u,
                          const std::vector<BoundaryCurve>& v) {
  return detail::run("boundary surface", [&] { return network(feature, u, v); });
}

ShapePtr match_surface(const Shape& body, const std::string& face, const EdgeChain& target,
                       Continuity continuity) {
  require_sheet(body, "match surface");
  return detail::run("match surface", [&] {
    int f = 0;
    if (face.empty()) {
      if (body.face_count() != 1) {
        throw std::invalid_argument("match surface: the body has " + std::to_string(body.face_count()) +
                                    " faces; name the one to match");
      }
    } else {
      const std::vector<int> found = detail::resolve_faces(body, {face}, "face");
      if (found.size() != 1) {
        const std::string pieces = " is in " + std::to_string(found.size()) + " pieces";
        throw std::invalid_argument(
            face_error(face, "match surface: the face" + pieces, "match surface: face " + face + pieces));
      }
      f = found.front();
    }
    if (!natural(body.face(f))) {
      const std::string why = ", which is not bounded by its surface's parameter range alone";
      const std::string name = detail::primary_name(body, f);
      throw std::invalid_argument(face_error(name, "unsupported: match surface of a face" + why,
                                             "unsupported: match surface of face " + name + why));
    }
    const Chain chain = chain_of(target, continuity != Continuity::G0, "match surface");
    return replaced(body, {{f, matched_face(body.face(f), chain, continuity)}});
  });
}

ShapePtr fair_surface(const Shape& body, const std::vector<std::string>& faces, const Fairing& fairing) {
  require_sheet(body, "fair surface");
  if (!(fairing.strength > 0.0 && fairing.strength <= 1.0)) {
    throw std::invalid_argument("fair surface: the strength must be more than 0 and at most 1");
  }
  if (fairing.iterations < 1) {
    throw std::invalid_argument("fair surface: at least one iteration");
  }
  detail::require_positive("the fairing tolerance", fairing.tolerance);
  return detail::run("fair surface", [&] {
    std::vector<int> picked;
    if (faces.empty()) {
      for (int f = 0; f < body.face_count(); ++f) {
        picked.push_back(f);
      }
    } else {
      picked = detail::resolve_faces(body, faces, "faces");
    }
    std::map<int, TopoDS_Face> faired;
    for (const int f : picked) {
      const TopoDS_Face face = faired_face(body, f, fairing);
      if (!face.IsNull()) {
        faired[f] = face;
      }
    }
    if (faired.empty()) {
      return std::make_shared<Shape>(body);
    }
    return replaced(body, faired);
  });
}

} // namespace mitcad::geometry
