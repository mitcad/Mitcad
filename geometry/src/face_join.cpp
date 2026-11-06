// SPDX-License-Identifier: MIT
#include "face_join.hpp"

#include <algorithm>
#include <cmath>
#include <functional>
#include <map>
#include <optional>
#include <set>
#include <string>
#include <tuple>
#include <utility>
#include <vector>

#include <BOPAlgo_BuilderFace.hxx>
#include <BOPTools_AlgoTools3D.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepLib.hxx>
#include <BRepTools.hxx>
#include <BRepTools_History.hxx>
#include <BRepTools_ReShape.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <ElSLib.hxx>
#include <GeomAbs_SurfaceType.hxx>
#include <Geom2dAPI_InterCurveCurve.hxx>
#include <Geom2dConvert.hxx>
#include <Geom2d_BSplineCurve.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom2d_Line.hxx>
#include <Geom2d_TrimmedCurve.hxx>
#include <GeomConvert.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_Curve.hxx>
#include <Geom_CylindricalSurface.hxx>
#include <Geom_Surface.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <IntRes2d_IntersectionPoint.hxx>
#include <IntTools_Context.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_HSequence.hxx>
#include <NCollection_List.hxx>
#include <NCollection_Map.hxx>
#include <Precision.hxx>
#include <ShapeAnalysis_FreeBounds.hxx>
#include <ShapeBuild_ReShape.hxx>
#include <ShapeExtend_Status.hxx>
#include <ShapeFix_Face.hxx>
#include <ShapeUpgrade_ShapeDivideClosed.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <Standard_ErrorHandler.hxx>
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
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Ax3.hxx>
#include <gp_Cone.hxx>
#include <gp_Cylinder.hxx>
#include <gp_Lin.hxx>
#include <gp_Pln.hxx>
#include <gp_Sphere.hxx>
#include <gp_Torus.hxx>
#include <gp_Trsf2d.hxx>
#include <gp.hxx>
#include <gp_Vec2d.hxx>

#include "history.hpp"
#include "mitcad/geometry/naming.hpp"
#include "util.hpp"

namespace mitcad::geometry::detail {
occ::handle<Geom_Surface> one_surface(const std::vector<occ::handle<Geom_Surface>>& pieces);

namespace {

using AncestorMap =
    NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;

// How near two ends of p-curves must lie to follow each other in a loop.
constexpr double kChain = 1.0e-6;

// One use of an edge by a piece: as the piece (taken forward) has it, with
// its curve on the surface moved into the joined face's periods.
struct Use {
  TopoDS_Edge edge;
  occ::handle<Geom2d_Curve> pcurve;
  double first = 0.0;
  double last = 0.0;
  gp_Pnt2d start;
  gp_Pnt2d end;
};

// A p-curve trimmed to its edge's range, as a B-spline.
occ::handle<Geom2d_BSplineCurve> spline_2d(const occ::handle<Geom2d_Curve>& c, double first, double last) {
  return Geom2dConvert::CurveToBSplineCurve(new Geom2d_TrimmedCurve(c, first, last));
}

occ::handle<Geom_BSplineCurve> spline_3d(const occ::handle<Geom_Curve>& c, double first, double last) {
  return GeomConvert::CurveToBSplineCurve(new Geom_TrimmedCurve(c, first, last));
}

// The pieces `group` (forward faces on one surface) joined into one face,
// or a null face where their boundary does not make closed loops.
TopoDS_Face join_faces(const std::vector<TopoDS_Face>& group) {
  TopLoc_Location location;
  std::vector<occ::handle<Geom_Surface>> surfaces;
  for (const TopoDS_Face& piece : group) {
    surfaces.push_back(BRep_Tool::Surface(piece, location));
  }
  const occ::handle<Geom_Surface> surface = one_surface(surfaces);
  if (surface.IsNull()) {
    return TopoDS_Face();
  }
  // The pieces each edge is used by, and how often.
  NCollection_IndexedDataMap<TopoDS_Shape, std::vector<std::size_t>, TopTools_ShapeMapHasher> users;
  for (std::size_t p = 0; p < group.size(); ++p) {
    for (TopExp_Explorer e(group[p], TopAbs_EDGE); e.More(); e.Next()) {
      const TopoDS_Shape edge = e.Current().Oriented(TopAbs_FORWARD);
      if (!users.Contains(edge)) {
        users.Add(edge, {});
      }
      users.ChangeFromKey(edge).push_back(p);
    }
  }
  // An edge between two pieces (once in each) is inside the joined face;
  // the others (also an edge twice in one piece, its seam) bound it.
  const auto inside = [&](const TopoDS_Shape& edge) {
    const std::vector<std::size_t>& by = users.FindFromKey(edge.Oriented(TopAbs_FORWARD));
    return by.size() == 2 && by[0] != by[1];
  };
  // The pieces' offsets in the surface's periods: a piece across an inside
  // edge from another lies where its curve of that edge meets the other's.
  const double uperiod = surface->IsUPeriodic() ? surface->UPeriod() : 0.0;
  const double vperiod = surface->IsVPeriodic() ? surface->VPeriod() : 0.0;
  const auto in_periods = [&](const gp_Vec2d& d) {
    return gp_Vec2d(uperiod > 0.0 ? uperiod * std::round(d.X() / uperiod) : 0.0,
                    vperiod > 0.0 ? vperiod * std::round(d.Y() / vperiod) : 0.0);
  };
  const auto middle_on = [](const TopoDS_Edge& edge, const TopoDS_Face& face) {
    double f = 0.0, l = 0.0;
    const occ::handle<Geom2d_Curve> c = BRep_Tool::CurveOnSurface(edge, face, f, l);
    return c.IsNull() ? std::optional<gp_Pnt2d>() : std::optional<gp_Pnt2d>(c->Value(0.5 * (f + l)));
  };
  std::vector<std::optional<gp_Vec2d>> offset(group.size());
  offset[0] = gp_Vec2d(0.0, 0.0);
  for (bool more = true; more;) {
    more = false;
    for (int k = 1; k <= users.Extent(); ++k) {
      const TopoDS_Edge edge = TopoDS::Edge(users.FindKey(k));
      if (!inside(edge)) {
        continue;
      }
      const std::size_t a = users.FindFromIndex(k)[0];
      const std::size_t b = users.FindFromIndex(k)[1];
      if (offset[a].has_value() == offset[b].has_value()) {
        continue;
      }
      const std::size_t known = offset[a] ? a : b;
      const std::size_t other = offset[a] ? b : a;
      TopoDS_Edge on_known, on_other;
      for (TopExp_Explorer e(group[known], TopAbs_EDGE); e.More(); e.Next()) {
        if (e.Current().IsSame(edge)) {
          on_known = TopoDS::Edge(e.Current());
        }
      }
      for (TopExp_Explorer e(group[other], TopAbs_EDGE); e.More(); e.Next()) {
        if (e.Current().IsSame(edge)) {
          on_other = TopoDS::Edge(e.Current());
        }
      }
      const auto pk = middle_on(on_known, group[known]);
      const auto po = middle_on(on_other, group[other]);
      if (!pk || !po) {
        return TopoDS_Face();
      }
      const gp_Vec2d d(*po, pk->Translated(*offset[known]));
      if (uperiod <= 0.0 && vperiod <= 0.0 && d.Magnitude() > kChain) {
        return TopoDS_Face();
      }
      offset[other] = in_periods(d);
      more = true;
    }
  }
  for (const auto& o : offset) {
    if (!o) {
      return TopoDS_Face();
    }
  }
  // The boundary's uses, their curves moved by their pieces' offsets.
  std::vector<Use> uses;
  double tolerance = 0.0;
  for (std::size_t p = 0; p < group.size(); ++p) {
    tolerance = std::max(tolerance, BRep_Tool::Tolerance(group[p]));
    for (TopExp_Explorer e(group[p], TopAbs_EDGE); e.More(); e.Next()) {
      const TopoDS_Edge& edge = TopoDS::Edge(e.Current());
      if (inside(edge)) {
        continue;
      }
      Use use;
      use.edge = edge;
      occ::handle<Geom2d_Curve> c = BRep_Tool::CurveOnSurface(edge, group[p], use.first, use.last);
      if (c.IsNull()) {
        return TopoDS_Face();
      }
      if (offset[p]->Magnitude() > 0.0) {
        c = occ::down_cast<Geom2d_Curve>(c->Translated(*offset[p]));
      }
      use.pcurve = c;
      const gp_Pnt2d a = c->Value(use.first);
      const gp_Pnt2d b = c->Value(use.last);
      const bool forward = edge.Orientation() != TopAbs_REVERSED;
      use.start = forward ? a : b;
      use.end = forward ? b : a;
      uses.push_back(use);
    }
  }
  // The loops: each use followed by the one that starts where it ends.
  BRep_Builder builder;
  TopoDS_Face joined;
  builder.MakeFace(joined, surface, location, tolerance);
  std::vector<bool> used(uses.size(), false);
  for (std::size_t s = 0; s < uses.size(); ++s) {
    if (used[s]) {
      continue;
    }
    TopoDS_Wire wire;
    builder.MakeWire(wire);
    std::size_t at = s;
    while (true) {
      used[at] = true;
      builder.Add(wire, uses[at].edge);
      if (uses[at].end.Distance(uses[s].start) < kChain) {
        break;
      }
      std::size_t next = uses.size();
      double best = kChain;
      for (std::size_t k = 0; k < uses.size(); ++k) {
        const double d = uses[k].start.Distance(uses[at].end);
        if (!used[k] && d < best) {
          best = d;
          next = k;
        }
      }
      if (next == uses.size()) {
        return TopoDS_Face();
      }
      at = next;
    }
    wire.Closed(true);
    builder.Add(joined, wire);
  }
  // The boundary's curves on the joined face (both of a seam's).
  for (std::size_t k = 0; k < uses.size(); ++k) {
    std::size_t twin = uses.size();
    for (std::size_t j = 0; j < uses.size(); ++j) {
      if (j != k && uses[j].edge.IsSame(uses[k].edge)) {
        twin = j;
      }
    }
    const TopoDS_Edge edge = TopoDS::Edge(uses[k].edge.Oriented(TopAbs_FORWARD));
    const double edge_tolerance = BRep_Tool::Tolerance(edge);
    if (twin == uses.size()) {
      builder.UpdateEdge(edge, uses[k].pcurve, joined, edge_tolerance);
    } else if (uses[k].edge.Orientation() == TopAbs_FORWARD) {
      builder.UpdateEdge(edge, uses[k].pcurve, uses[twin].pcurve, joined, edge_tolerance);
    } else {
      continue;
    }
    builder.Range(edge, joined, uses[k].first, uses[k].last);
  }
  return joined;
}

// The B-splines `pieces`, each starting where the one before ends, joined
// into one: each moved along its parameter to start where the one before
// ends, the joints of the degree's multiplicity (where the pieces meet, the
// one before's end kept), each piece's parameter scaled by its `scales`
// (see speed_scales). Curves of one run in 3D and on faces, of the same
// parameters piece by piece, are joined with the same parameters. Null for
// rational pieces.
template <class Curve, class Point>
occ::handle<Curve> concatenated(std::vector<occ::handle<Curve>> pieces, const std::vector<double>& scales) {
  int degree = 0;
  for (const occ::handle<Curve>& c : pieces) {
    if (c->IsRational()) {
      return nullptr;
    }
    degree = std::max(degree, c->Degree());
  }
  std::vector<Point> poles;
  std::vector<double> knots;
  std::vector<int> multiplicities;
  for (std::size_t k = 0; k < pieces.size(); ++k) {
    occ::handle<Curve>& c = pieces[k];
    if (c->Degree() < degree) {
      c->IncreaseDegree(degree);
    }
    const int from = k == 0 ? 1 : 2;
    const double start = k == 0 ? c->Knot(1) : knots.back();
    const double scale = scales[k];
    if (k > 0) {
      multiplicities.back() = degree;
    }
    for (int i = from; i <= c->NbPoles(); ++i) {
      poles.push_back(c->Pole(i));
    }
    for (int i = from; i <= c->NbKnots(); ++i) {
      knots.push_back(start + scale * (c->Knot(i) - c->Knot(1)));
      multiplicities.push_back(c->Multiplicity(i));
    }
  }
  NCollection_Array1<Point> pole_array(1, static_cast<int>(poles.size()));
  NCollection_Array1<double> knot_array(1, static_cast<int>(knots.size()));
  NCollection_Array1<int> multiplicity_array(1, static_cast<int>(knots.size()));
  for (std::size_t i = 0; i < poles.size(); ++i) {
    pole_array.SetValue(static_cast<int>(i) + 1, poles[i]);
  }
  for (std::size_t i = 0; i < knots.size(); ++i) {
    knot_array.SetValue(static_cast<int>(i) + 1, knots[i]);
    multiplicity_array.SetValue(static_cast<int>(i) + 1, multiplicities[i]);
  }
  return new Curve(pole_array, knot_array, multiplicity_array, degree);
}

// The scales of the pieces' parameters (the first's 1) under which the
// curve they join keeps its speed across each joint, as one curve swept at
// one pace does: pieces of a turn's part each, each of its own parameter
// range, would otherwise join into a curve whose speed jumps at the joints,
// tangent but not smooth in its parameter, along which a rounding's guide
// is not taken as one (mitcad#343).
std::vector<double> speed_scales(const std::vector<occ::handle<Geom_BSplineCurve>>& pieces) {
  std::vector<double> scales(pieces.size(), 1.0);
  for (std::size_t k = 1; k < pieces.size(); ++k) {
    gp_Pnt p;
    gp_Vec before, after;
    pieces[k - 1]->D1(pieces[k - 1]->LastParameter(), p, before);
    pieces[k]->D1(pieces[k]->FirstParameter(), p, after);
    const double end_speed = before.Magnitude() / scales[k - 1];
    scales[k] = end_speed > gp::Resolution() && after.Magnitude() > gp::Resolution()
                    ? after.Magnitude() / end_speed
                    : scales[k - 1];
  }
  return scales;
}

// Whether two edges meet tangentially at a vertex they share.
bool tangent_at(const TopoDS_Edge& a, const TopoDS_Edge& b, const TopoDS_Vertex& v) {
  const auto direction = [&](const TopoDS_Edge& e) {
    BRepAdaptor_Curve c(e);
    const double t = BRep_Tool::Parameter(v, e);
    gp_Pnt p;
    gp_Vec d;
    c.D1(t, p, d);
    return d;
  };
  const gp_Vec da = direction(a);
  const gp_Vec db = direction(b);
  if (da.Magnitude() < gp::Resolution() || db.Magnitude() < gp::Resolution()) {
    return false;
  }
  return da.IsParallel(db, 1.0e-6);
}

// The edges of `shape` along one curve between the same two faces, of which
// one is in `faces`, meeting tangentially at vertices of no other edge,
// joined into one edge. The history in `reshape`.
TopoDS_Shape join_edges(const TopoDS_Shape& shape, const ShapeMap& faces, const occ::handle<BRepTools_ReShape>& reshape) {
  AncestorMap edge_faces;
  TopExp::MapShapesAndUniqueAncestors(shape, TopAbs_EDGE, TopAbs_FACE, edge_faces);
  AncestorMap vertex_edges;
  TopExp::MapShapesAndUniqueAncestors(shape, TopAbs_VERTEX, TopAbs_EDGE, vertex_edges);
  // The two faces of an edge, or none.
  const auto faces_of = [&](const TopoDS_Shape& edge) -> std::optional<std::pair<TopoDS_Shape, TopoDS_Shape>> {
    const NCollection_List<TopoDS_Shape>& around = edge_faces.FindFromKey(edge);
    if (around.Extent() != 2 || around.First().IsSame(around.Last())) {
      return std::nullopt;
    }
    return std::make_pair(around.First(), around.Last());
  };
  const auto same_faces = [&](const TopoDS_Shape& a, const TopoDS_Shape& b) {
    const auto fa = faces_of(a);
    const auto fb = faces_of(b);
    return fa && fb &&
           ((fa->first.IsSame(fb->first) && fa->second.IsSame(fb->second)) ||
            (fa->first.IsSame(fb->second) && fa->second.IsSame(fb->first))) &&
           (faces.Contains(fa->first) || faces.Contains(fa->second));
  };
  // The vertices a run of edges passes through.
  ShapeMap through;
  for (int k = 1; k <= vertex_edges.Extent(); ++k) {
    const NCollection_List<TopoDS_Shape>& around = vertex_edges.FindFromIndex(k);
    if (around.Extent() != 2) {
      continue;
    }
    const TopoDS_Edge a = TopoDS::Edge(around.First());
    const TopoDS_Edge b = TopoDS::Edge(around.Last());
    if (a.IsSame(b) || BRep_Tool::Degenerated(a) || BRep_Tool::Degenerated(b) || !same_faces(a, b)) {
      continue;
    }
    const TopoDS_Vertex v = TopoDS::Vertex(vertex_edges.FindKey(k));
    if (tangent_at(a, b, v)) {
      through.Add(v);
    }
  }
  if (through.IsEmpty()) {
    return shape;
  }
  ShapeMap done;
  BRep_Builder builder;
  for (int k = 1; k <= edge_faces.Extent(); ++k) {
    const TopoDS_Edge seed = TopoDS::Edge(edge_faces.FindKey(k));
    if (done.Contains(seed) || BRep_Tool::Degenerated(seed)) {
      continue;
    }
    TopoDS_Vertex v0, v1;
    TopExp::Vertices(seed, v0, v1);
    if (!through.Contains(v0) && !through.Contains(v1)) {
      continue;
    }
    // Back from the seed to the run's start, then forward along it: each
    // edge and whether the run goes along it.
    TopoDS_Edge start = seed;
    bool start_forward = true;
    {
      TopoDS_Vertex at = v0;
      TopoDS_Edge edge = seed;
      ShapeMap seen;
      seen.Add(seed);
      while (through.Contains(at)) {
        const NCollection_List<TopoDS_Shape>& around = vertex_edges.FindFromKey(at);
        const TopoDS_Edge next = TopoDS::Edge(around.First().IsSame(edge) ? around.Last() : around.First());
        if (seen.Contains(next)) {
          break;
        }
        seen.Add(next);
        TopoDS_Vertex n0, n1;
        TopExp::Vertices(next, n0, n1);
        // The run goes along `next` where it ends at `at`.
        start_forward = n1.IsSame(at);
        at = start_forward ? n0 : n1;
        edge = next;
        start = next;
      }
      if (start.IsSame(seed)) {
        start_forward = true;
      }
    }
    std::vector<std::pair<TopoDS_Edge, bool>> run;
    {
      TopoDS_Edge edge = start;
      bool forward = start_forward;
      ShapeMap seen;
      while (true) {
        seen.Add(edge);
        run.emplace_back(edge, forward);
        TopoDS_Vertex e0, e1;
        TopExp::Vertices(edge, e0, e1);
        const TopoDS_Vertex at = forward ? e1 : e0;
        if (!through.Contains(at)) {
          break;
        }
        const NCollection_List<TopoDS_Shape>& around = vertex_edges.FindFromKey(at);
        const TopoDS_Edge next = TopoDS::Edge(around.First().IsSame(edge) ? around.Last() : around.First());
        if (seen.Contains(next)) {
          break;
        }
        TopoDS_Vertex n0, n1;
        TopExp::Vertices(next, n0, n1);
        forward = n0.IsSame(at);
        edge = next;
      }
    }
    for (const auto& [edge, forward] : run) {
      done.Add(edge);
    }
    if (run.size() < 2) {
      continue;
    }
    const auto pair = faces_of(run.front().first);
    const TopoDS_Face fa = TopoDS::Face(pair->first);
    const TopoDS_Face fb = TopoDS::Face(pair->second);
    // The curves joined along the run, in 3D and on both faces (none of
    // them a seam of its face).
    std::vector<occ::handle<Geom_BSplineCurve>> pieces3d;
    std::vector<occ::handle<Geom2d_BSplineCurve>> pieces_a, pieces_b;
    double tolerance = 0.0;
    bool ok = true;
    for (const auto& [edge, forward] : run) {
      if (BRep_Tool::IsClosed(edge, fa) || BRep_Tool::IsClosed(edge, fb)) {
        ok = false;
        break;
      }
      tolerance = std::max(tolerance, BRep_Tool::Tolerance(edge));
      double f = 0.0, l = 0.0;
      const occ::handle<Geom_Curve> c = BRep_Tool::Curve(edge, f, l);
      double fa0 = 0.0, la0 = 0.0, fb0 = 0.0, lb0 = 0.0;
      const occ::handle<Geom2d_Curve> ca = BRep_Tool::CurveOnSurface(edge, fa, fa0, la0);
      const occ::handle<Geom2d_Curve> cb = BRep_Tool::CurveOnSurface(edge, fb, fb0, lb0);
      if (c.IsNull() || ca.IsNull() || cb.IsNull()) {
        ok = false;
        break;
      }
      const occ::handle<Geom_BSplineCurve> s3 = spline_3d(c, f, l);
      const occ::handle<Geom2d_BSplineCurve> sa = spline_2d(ca, f, l);
      const occ::handle<Geom2d_BSplineCurve> sb = spline_2d(cb, f, l);
      // (Converted keeping the edge's parameter: an analytic curve's
      // conversion may change it.)
      const auto keeps = [&](double a, double b) {
        return std::abs(a - f) < Precision::PConfusion() && std::abs(b - l) < Precision::PConfusion();
      };
      if (!keeps(s3->FirstParameter(), s3->LastParameter()) || !keeps(sa->FirstParameter(), sa->LastParameter()) ||
          !keeps(sb->FirstParameter(), sb->LastParameter())) {
        ok = false;
        break;
      }
      if (!forward) {
        s3->Reverse();
        sa->Reverse();
        sb->Reverse();
      }
      pieces3d.push_back(s3);
      pieces_a.push_back(sa);
      pieces_b.push_back(sb);
    }
    if (!ok) {
      continue;
    }
    const std::vector<double> scales = speed_scales(pieces3d);
    const occ::handle<Geom_BSplineCurve> joined3d = concatenated<Geom_BSplineCurve, gp_Pnt>(pieces3d, scales);
    const occ::handle<Geom2d_BSplineCurve> joineda = concatenated<Geom2d_BSplineCurve, gp_Pnt2d>(pieces_a, scales);
    const occ::handle<Geom2d_BSplineCurve> joinedb = concatenated<Geom2d_BSplineCurve, gp_Pnt2d>(pieces_b, scales);
    if (joined3d.IsNull() || joineda.IsNull() || joinedb.IsNull()) {
      continue;
    }
    const double first = joined3d->FirstParameter();
    const double last = joined3d->LastParameter();
    TopoDS_Vertex a0, a1, b0, b1;
    TopExp::Vertices(run.front().first, a0, a1);
    TopExp::Vertices(run.back().first, b0, b1);
    const TopoDS_Vertex from = run.front().second ? a0 : a1;
    const TopoDS_Vertex to = run.back().second ? b1 : b0;
    TopoDS_Edge joined;
    builder.MakeEdge(joined, joined3d, tolerance);
    builder.Add(joined, from.Oriented(TopAbs_FORWARD));
    builder.Add(joined, to.Oriented(TopAbs_REVERSED));
    builder.UpdateEdge(joined, joineda, fa, tolerance);
    builder.UpdateEdge(joined, joinedb, fb, tolerance);
    builder.Range(joined, first, last);
    builder.SameParameter(joined, false);
    BRepLib::SameParameter(joined, tolerance);
    // (Kept in its pieces where their curves do not join into one edge
    // within their tolerance.)
    if (!BRep_Tool::SameParameter(joined) || BRep_Tool::Tolerance(joined) > std::max(10.0 * tolerance, 1.0e-5)) {
      continue;
    }
    reshape->Replace(run.front().first.Oriented(run.front().second ? TopAbs_FORWARD : TopAbs_REVERSED), joined);
    for (std::size_t i = 1; i < run.size(); ++i) {
      reshape->Remove(run[i].first);
    }
  }
  return reshape->Apply(shape);
}

} // namespace

// join_surface_pieces on a body of its own (its edges get the joined faces'
// curves in place).
static ShapePtr join_pieces(const Shape& body, const std::vector<int>& faces) {
  // The faces by their surfaces, in groups connected through their edges.
  std::vector<std::vector<int>> groups;
  {
    std::vector<int> left = faces;
    AncestorMap edge_faces;
    TopExp::MapShapesAndUniqueAncestors(body.occt(), TopAbs_EDGE, TopAbs_FACE, edge_faces);
    while (!left.empty()) {
      const int root = left.back();
      left.pop_back();
      std::vector<int> group{root};
      for (std::size_t g = 0; g < group.size(); ++g) {
        // (Across an edge, the next piece of the surface: pieces of one
        // B-spline surface overlap only where they meet.)
        TopLoc_Location lg;
        const occ::handle<Geom_Surface> sg = BRep_Tool::Surface(body.face(group[g]), lg);
        for (TopExp_Explorer e(body.face(group[g]), TopAbs_EDGE); e.More(); e.Next()) {
          for (const TopoDS_Shape& neighbour : edge_faces.FindFromKey(e.Current())) {
            const int n = body.face_index(neighbour);
            const auto it = std::find(left.begin(), left.end(), n);
            if (it == left.end()) {
              continue;
            }
            TopLoc_Location ln;
            const occ::handle<Geom_Surface> sn = BRep_Tool::Surface(TopoDS::Face(neighbour), ln);
            if (same_surface(sn, sg) && ln.IsEqual(lg) && neighbour.Orientation() == body.face(root).Orientation()) {
              group.push_back(n);
              left.erase(it);
            }
          }
        }
      }
      if (group.size() > 1) {
        groups.push_back(group);
      }
    }
  }
  if (groups.empty()) {
    return std::make_shared<Shape>(body);
  }
  // The groups joined, each into the first piece's place.
  occ::handle<BRepTools_ReShape> faces_reshape = new BRepTools_ReShape();
  std::vector<std::pair<TopoDS_Face, NameList>> joined_faces;
  for (const std::vector<int>& group : groups) {
    std::vector<TopoDS_Face> pieces;
    for (const int i : group) {
      pieces.push_back(TopoDS::Face(body.face(i).Oriented(TopAbs_FORWARD)));
    }
    TopoDS_Face joined = join_faces(pieces);
    if (joined.IsNull()) {
      return nullptr;
    }
    joined.Orientation(body.face(group.front()).Orientation());
    NameList names;
    for (const int i : group) {
      const NameList& own = body.face_names(i);
      names.insert(names.end(), own.begin(), own.end());
    }
    faces_reshape->Replace(body.face(group.front()), joined);
    for (std::size_t k = 1; k < group.size(); ++k) {
      faces_reshape->Remove(body.face(group[k]));
    }
    joined_faces.emplace_back(joined, names);
  }
  const TopoDS_Shape with_faces = faces_reshape->Apply(body.occt());
  // The names: the others' carried, the joined faces' their pieces'.
  std::vector<int> pieces_of_groups;
  for (const std::vector<int>& group : groups) {
    pieces_of_groups.insert(pieces_of_groups.end(), group.begin(), group.end());
  }
  FaceNamer face_namer(with_faces);
  face_namer.carry(*faces_reshape->History(), body, pieces_of_groups);
  for (const auto& [face, names] : joined_faces) {
    for (const std::string& name : names) {
      face_namer.add(face, name);
    }
  }
  const ShapePtr step = face_namer.shape();
  // The joined faces' edges along one curve joined.
  ShapeMap joined_set;
  for (const auto& [face, names] : joined_faces) {
    joined_set.Add(face);
  }
  occ::handle<BRepTools_ReShape> edges_reshape = new BRepTools_ReShape();
  const TopoDS_Shape result = join_edges(step->occt(), joined_set, edges_reshape);
  FaceNamer namer(result);
  namer.carry(*edges_reshape->History(), *step);
  // (The faces that changed checked: the others are the body's.)
  ShapeMap before;
  TopExp::MapShapes(body.occt(), TopAbs_FACE, before);
  BRep_Builder builder;
  TopoDS_Compound changed;
  builder.MakeCompound(changed);
  for (TopExp_Explorer f(result, TopAbs_FACE); f.More(); f.Next()) {
    if (!before.Contains(f.Current())) {
      builder.Add(changed, f.Current());
    }
  }
  if (!BRepCheck_Analyzer(changed).IsValid()) {
    return nullptr;
  }
  return namer.shape();
}

// Whether two surfaces are one: the same, copies of one (a cylinder of the
// same frame and radius), or pieces of one B-spline surface (the same along
// u, along v of ranges that meet or overlap, and the same where they do),
// so that a curve on the one is the same curve on the other.
bool same_surface(const occ::handle<Geom_Surface>& a, const occ::handle<Geom_Surface>& b) {
  if (a == b) {
    return true;
  }
  constexpr double kSame = 1.0e-9;
  const auto ca = occ::down_cast<Geom_CylindricalSurface>(a);
  const auto cb = occ::down_cast<Geom_CylindricalSurface>(b);
  if (!ca.IsNull() && !cb.IsNull()) {
    const gp_Ax3 fa = ca->Position();
    const gp_Ax3 fb = cb->Position();
    return std::abs(ca->Radius() - cb->Radius()) < kSame && fa.Location().Distance(fb.Location()) < kSame &&
           fa.Direction().IsEqual(fb.Direction(), kSame) && fa.XDirection().IsEqual(fb.XDirection(), kSame) &&
           fa.Direct() == fb.Direct();
  }
  const auto sa = occ::down_cast<Geom_BSplineSurface>(a);
  const auto sb = occ::down_cast<Geom_BSplineSurface>(b);
  if (sa.IsNull() || sb.IsNull() || sa->UDegree() != sb->UDegree() || sa->VDegree() != sb->VDegree() ||
      sa->NbUPoles() != sb->NbUPoles() || sa->NbUKnots() != sb->NbUKnots() || sa->IsURational() ||
      sa->IsVRational() || sb->IsURational() || sb->IsVRational() || sa->IsUPeriodic() || sa->IsVPeriodic() ||
      sb->IsUPeriodic() || sb->IsVPeriodic()) {
    return false;
  }
  for (int i = 1; i <= sa->NbUKnots(); ++i) {
    if (std::abs(sa->UKnot(i) - sb->UKnot(i)) > kSame || sa->UMultiplicity(i) != sb->UMultiplicity(i)) {
      return false;
    }
  }
  double au0 = 0, au1 = 0, av0 = 0, av1 = 0, bu0 = 0, bu1 = 0, bv0 = 0, bv1 = 0;
  sa->Bounds(au0, au1, av0, av1);
  sb->Bounds(bu0, bu1, bv0, bv1);
  const double low = std::max(av0, bv0);
  const double high = std::min(av1, bv1);
  if (high < low - kSame) {
    return false;
  }
  for (const double fv : {0.0, 0.5, 1.0}) {
    for (const double fu : {0.0, 0.5, 1.0}) {
      const double u = au0 + fu * (au1 - au0);
      const double v = low + fv * (std::max(high, low) - low);
      if (sa->Value(u, v).Distance(sb->Value(u, v)) > kSame) {
        return false;
      }
    }
  }
  return true;
}

// The one surface of pieces on one surface (same_surface): the surface
// itself, or the pieces of a B-spline surface joined along v, each cut in
// the middle of its overlap with the next, of the same parameters (knots of
// their joints removed where the surface is as smooth there as on either
// side). Null where they do not join.
occ::handle<Geom_Surface> one_surface(const std::vector<occ::handle<Geom_Surface>>& pieces) {
  bool all_one = true;
  for (const occ::handle<Geom_Surface>& s : pieces) {
    all_one = all_one && s == pieces.front();
  }
  if (all_one || occ::down_cast<Geom_BSplineSurface>(pieces.front()).IsNull()) {
    return pieces.front();
  }
  std::vector<occ::handle<Geom_BSplineSurface>> splines;
  for (const occ::handle<Geom_Surface>& s : pieces) {
    const auto spline = occ::down_cast<Geom_BSplineSurface>(s);
    if (spline.IsNull()) {
      return nullptr;
    }
    bool seen = false;
    for (const auto& known : splines) {
      seen = seen || known == spline;
    }
    if (!seen) {
      splines.push_back(spline);
    }
  }
  const auto v_first = [](const occ::handle<Geom_BSplineSurface>& s) {
    double u0 = 0, u1 = 0, v0 = 0, v1 = 0;
    s->Bounds(u0, u1, v0, v1);
    return v0;
  };
  std::sort(splines.begin(), splines.end(),
            [&](const auto& x, const auto& y) { return v_first(x) < v_first(y); });
  occ::handle<Geom_BSplineSurface> joined = occ::down_cast<Geom_BSplineSurface>(splines.front()->Copy());
  for (std::size_t k = 1; k < splines.size(); ++k) {
    double u0 = 0, u1 = 0, v0 = 0, v1 = 0, w0 = 0, w1 = 0, x0 = 0, x1 = 0;
    joined->Bounds(u0, u1, v0, v1);
    occ::handle<Geom_BSplineSurface> next = occ::down_cast<Geom_BSplineSurface>(splines[k]->Copy());
    next->Bounds(w0, w1, x0, x1);
    if (x1 <= v1) {
      continue;
    }
    if (x0 > v1 + 1.0e-9) {
      return nullptr;
    }
    const double cut = 0.5 * (std::max(x0, v0) + v1);
    joined->Segment(u0, u1, v0, cut);
    next->Segment(u0, u1, cut, x1);
    const int degree = joined->VDegree();
    if (next->VDegree() != degree) {
      return nullptr;
    }
    const int nu = joined->NbUPoles();
    const int na = joined->NbVPoles();
    const int nb = next->NbVPoles();
    NCollection_Array2<gp_Pnt> poles(1, nu, 1, na + nb - 1);
    for (int i = 1; i <= nu; ++i) {
      for (int j = 1; j <= na; ++j) {
        poles.SetValue(i, j, joined->Pole(i, j));
      }
      for (int j = 2; j <= nb; ++j) {
        poles.SetValue(i, na + j - 1, next->Pole(i, j));
      }
    }
    const int ka = joined->NbVKnots();
    const int kb = next->NbVKnots();
    NCollection_Array1<double> vknots(1, ka + kb - 1);
    NCollection_Array1<int> vmults(1, ka + kb - 1);
    for (int i = 1; i <= ka; ++i) {
      vknots.SetValue(i, joined->VKnot(i));
      vmults.SetValue(i, joined->VMultiplicity(i));
    }
    vmults.SetValue(ka, degree);
    for (int i = 2; i <= kb; ++i) {
      vknots.SetValue(ka + i - 1, next->VKnot(i));
      vmults.SetValue(ka + i - 1, next->VMultiplicity(i));
    }
    const NCollection_Array1<double> uknots = joined->UKnots();
    const NCollection_Array1<int> umults = joined->UMultiplicities();
    joined = new Geom_BSplineSurface(poles, uknots, vknots, umults, vmults, joined->UDegree(), degree);
    // (The joint as smooth as the surface it was cut from where it can be.)
    for (int m = 0; m < degree && !joined->RemoveVKnot(ka, m, 1.0e-9); ++m) {
    }
  }
  return joined;
}

ShapePtr join_surface_pieces(const Shape& body, const std::vector<int>& faces) {
  if (faces.size() < 2) {
    return std::make_shared<Shape>(body);
  }
  // (On a copy: the joined faces' edges get their curves on the joined
  // faces in place, and the body's pieces keep theirs.)
  const InputCopy copy(body.occt());
  FaceNamer namer(copy.shape());
  for (int i = 0; i < body.face_count(); ++i) {
    for (const std::string& name : body.face_names(i)) {
      namer.add(copy.of(body.face(i)), name);
    }
  }
  const ShapePtr own = namer.shape();
  std::vector<int> mapped;
  for (const int f : faces) {
    mapped.push_back(own->face_index(copy.of(body.face(f))));
  }
  return join_pieces(*own, mapped);
}

namespace {

double distance_to(const gp_Pnt& point, const TopoDS_Shape& shape) {
  BRepExtrema_DistShapeShape extrema(BRepBuilderAPI_MakeVertex(point).Vertex(), shape);
  return extrema.IsDone() && extrema.NbSolution() > 0 ? extrema.Value() : RealLast();
}

// The face on its own piece of its B-spline surface, over its parameters
// and a little more (of the same parameters, so its edges' curves on it
// stay): a piece of a face of many turns, on a surface of many turns, which
// later booleans near it take far longer on. Other faces as they are.
TopoDS_Face on_own_piece(const TopoDS_Face& face) {
  TopLoc_Location location;
  const auto spline = occ::down_cast<Geom_BSplineSurface>(BRep_Tool::Surface(face, location));
  if (spline.IsNull() || spline->IsUPeriodic() || spline->IsVPeriodic()) {
    return face;
  }
  double u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  double a0 = 0.0, a1 = 0.0, b0 = 0.0, b1 = 0.0;
  spline->Bounds(a0, a1, b0, b1);
  const double f0 = std::max(a0, u0 - 0.05 * (u1 - u0));
  const double f1 = std::min(a1, u1 + 0.05 * (u1 - u0));
  const double g0 = std::max(b0, v0 - 0.05 * (v1 - v0));
  const double g1 = std::min(b1, v1 + 0.05 * (v1 - v0));
  if ((f1 - f0) > 0.5 * (a1 - a0) && (g1 - g0) > 0.5 * (b1 - b0)) {
    return face;
  }
  const occ::handle<Geom_BSplineSurface> own = occ::down_cast<Geom_BSplineSurface>(spline->Copy());
  own->Segment(f0, f1, g0, g1);
  BRep_Builder builder;
  const TopoDS_Face forward = TopoDS::Face(face.Oriented(TopAbs_FORWARD));
  TopoDS_Face result;
  builder.MakeFace(result, own, location, BRep_Tool::Tolerance(face));
  for (TopExp_Explorer x(forward, TopAbs_EDGE); x.More(); x.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(x.Current());
    const double tolerance = BRep_Tool::Tolerance(edge);
    double a = 0.0, b = 0.0;
    if (BRep_Tool::IsClosed(edge, forward)) {
      const occ::handle<Geom2d_Curve> c1 =
          BRep_Tool::CurveOnSurface(TopoDS::Edge(edge.Oriented(TopAbs_FORWARD)), forward, a, b);
      const occ::handle<Geom2d_Curve> c2 =
          BRep_Tool::CurveOnSurface(TopoDS::Edge(edge.Oriented(TopAbs_REVERSED)), forward, a, b);
      builder.UpdateEdge(edge, c1, c2, result, tolerance);
    } else {
      builder.UpdateEdge(edge, BRep_Tool::CurveOnSurface(edge, forward, a, b), result, tolerance);
    }
    builder.Range(edge, result, a, b);
  }
  for (TopoDS_Iterator w(forward); w.More(); w.Next()) {
    builder.Add(result, w.Value());
  }
  result.Orientation(face.Orientation());
  return result;
}

// The cuts of faces of a shape along curves on them: the vertices where
// the cuts cross the faces' edges (an edge's own vertex where they cross it
// there) and each face's cuts as wires of one edge between two of them.
class Cuts {
public:
  using Pieces = std::pair<TopoDS_Face, NCollection_List<TopoDS_Shape>>;

  // Cuts `face` along the parts inside it of a curve on it (`curve` in 3D,
  // `pcurve` on the face's surface, of one parameter, from `first` to
  // `last`) that run from one of its edges to another.
  void add(const TopoDS_Face& face, const occ::handle<Geom_Curve>& curve, const occ::handle<Geom2d_Curve>& pcurve,
           double first, double last) {
    struct Crossing {
      double at;
      TopoDS_Edge side;
      double on_side;
    };
    std::vector<Crossing> crossings;
    const occ::handle<Geom2d_Curve> cut = new Geom2d_TrimmedCurve(pcurve, first, last);
    for (TopExp_Explorer x(face, TopAbs_EDGE); x.More(); x.Next()) {
      const TopoDS_Edge& side = TopoDS::Edge(x.Current());
      double a = 0.0, b = 0.0;
      const occ::handle<Geom2d_Curve> on_face = BRep_Tool::CurveOnSurface(side, face, a, b);
      if (on_face.IsNull() || b - a < Precision::PConfusion()) {
        continue;
      }
      Geom2dAPI_InterCurveCurve crossing(cut, new Geom2d_TrimmedCurve(on_face, a, b), 1.0e-9);
      for (int i = 1; i <= crossing.NbPoints(); ++i) {
        const IntRes2d_IntersectionPoint& point = crossing.Intersector().Point(i);
        crossings.push_back(Crossing{point.ParamOnFirst(), side, point.ParamOnSecond()});
      }
    }
    std::sort(crossings.begin(), crossings.end(), [](const Crossing& x, const Crossing& y) { return x.at < y.at; });
    const double tolerance = BRep_Tool::Tolerance(face);
    const double same = 1.0e-7 * std::max(1.0, last - first);
    for (std::size_t i = 0; i + 1 < crossings.size(); ++i) {
      const Crossing& from = crossings[i];
      const Crossing& to = crossings[i + 1];
      if (to.at - from.at < same) {
        continue;
      }
      BRepClass_FaceClassifier classifier(face, pcurve->Value(0.5 * (from.at + to.at)), tolerance);
      if (classifier.State() != TopAbs_IN) {
        continue;
      }
      const TopoDS_Vertex start = vertex_on(from.side, from.on_side);
      const TopoDS_Vertex end = vertex_on(to.side, to.on_side);
      if (start.IsSame(end)) {
        continue;
      }
      // (Its ends at the vertices, within the gap between them and the
      // curve.)
      const double gap = std::max(curve->Value(from.at).Distance(BRep_Tool::Pnt(start)),
                                  curve->Value(to.at).Distance(BRep_Tool::Pnt(end)));
      const double within = std::max(tolerance, 1.5 * gap);
      BRep_Builder builder;
      TopoDS_Edge edge;
      builder.MakeEdge(edge, curve, within);
      builder.Add(edge, start.Oriented(TopAbs_FORWARD));
      builder.Add(edge, end.Oriented(TopAbs_REVERSED));
      builder.Range(edge, from.at, to.at);
      builder.UpdateEdge(edge, pcurve, face, within);
      builder.Range(edge, face, from.at, to.at);
      BRepLib::SameParameter(edge, tolerance);
      m_wires[face].push_back(BRepBuilderAPI_MakeWire(edge).Wire());
      m_ends.emplace_back(from.side, from.on_side);
      m_ends.emplace_back(to.side, to.on_side);
    }
  }

  // Where the cuts so far end: on which edge, at which parameter.
  const std::vector<std::pair<TopoDS_Edge, double>>& ends() const { return m_ends; }

  bool empty() const { return m_wires.empty(); }

  // `shape` split along the cuts: the edges they cross split at their
  // vertices, the faces they cut built anew from their edges and the cuts
  // (both ways), each face's pieces in `pieces`. Null where a face cannot
  // be split.
  TopoDS_Shape apply(const TopoDS_Shape& shape, std::vector<Pieces>& pieces,
                     const occ::handle<BRepTools_ReShape>& reshape) const {
    BRep_Builder builder;
    std::map<const TopoDS_TShape*, std::vector<TopoDS_Edge>> edge_pieces;
    {
      std::map<const TopoDS_TShape*, std::vector<std::pair<double, TopoDS_Vertex>>> on_edges;
      std::map<const TopoDS_TShape*, TopoDS_Edge> edges;
      for (const auto& [vertex, at, edge] : m_vertices) {
        on_edges[edge.TShape().get()].emplace_back(at, vertex);
        edges[edge.TShape().get()] = TopoDS::Edge(edge.Oriented(TopAbs_FORWARD));
      }
      for (auto& [key, points] : on_edges) {
        const TopoDS_Edge& edge = edges[key];
        TopoDS_Vertex v0, v1;
        TopExp::Vertices(edge, v0, v1);
        double a = 0.0, b = 0.0;
        BRep_Tool::Range(edge, a, b);
        std::sort(points.begin(), points.end(), [](const auto& x, const auto& y) { return x.first < y.first; });
        points.insert(points.begin(), {a, v0});
        points.emplace_back(b, v1);
        TopoDS_Compound parts;
        builder.MakeCompound(parts);
        for (std::size_t i = 0; i + 1 < points.size(); ++i) {
          TopoDS_Edge part = TopoDS::Edge(edge.EmptyCopied());
          part.Orientation(TopAbs_FORWARD);
          builder.Add(part, points[i].second.Oriented(TopAbs_FORWARD));
          builder.Add(part, points[i + 1].second.Oriented(TopAbs_REVERSED));
          builder.Range(part, points[i].first, points[i + 1].first);
          for (std::size_t j : {i, i + 1}) {
            builder.UpdateVertex(points[j].second, points[j].first, part, BRep_Tool::Tolerance(points[j].second));
          }
          edge_pieces[key].push_back(part);
          builder.Add(parts, part);
        }
        reshape->Replace(edge, parts);
      }
    }
    for (const auto& [face, wires] : m_wires) {
      const TopoDS_Face forward = TopoDS::Face(face.Oriented(TopAbs_FORWARD));
      NCollection_List<TopoDS_Shape> edges;
      for (TopExp_Explorer x(forward, TopAbs_EDGE); x.More(); x.Next()) {
        const auto found = edge_pieces.find(x.Current().TShape().get());
        if (found == edge_pieces.end()) {
          edges.Append(x.Current());
          continue;
        }
        for (const TopoDS_Edge& part : found->second) {
          edges.Append(part.Oriented(x.Current().Orientation()));
        }
      }
      for (const TopoDS_Wire& wire : wires) {
        for (TopExp_Explorer x(wire, TopAbs_EDGE); x.More(); x.Next()) {
          edges.Append(x.Current().Oriented(TopAbs_FORWARD));
          edges.Append(x.Current().Oriented(TopAbs_REVERSED));
        }
      }
      BOPAlgo_BuilderFace build;
      build.SetFace(forward);
      build.SetShapes(edges);
      build.Perform();
      if (build.HasErrors() || build.Areas().Extent() < 2) {
        return TopoDS_Shape();
      }
      TopoDS_Compound areas;
      builder.MakeCompound(areas);
      NCollection_List<TopoDS_Shape> faces;
      for (const TopoDS_Shape& area : build.Areas()) {
        faces.Append(on_own_piece(TopoDS::Face(area)));
        builder.Add(areas, faces.Last());
      }
      reshape->Replace(forward, areas);
      NCollection_List<TopoDS_Shape> own;
      for (const TopoDS_Shape& area : faces) {
        own.Append(area.Oriented(face.Orientation() == TopAbs_REVERSED ? TopAbs::Reverse(area.Orientation())
                                                                        : area.Orientation()));
      }
      pieces.emplace_back(face, own);
    }
    return reshape->Apply(shape);
  }

private:
  // The vertex at a parameter of an edge: its own at its ends, else one
  // added on it (once for each point).
  TopoDS_Vertex vertex_on(const TopoDS_Edge& side, double at) {
    TopoDS_Vertex v0, v1;
    TopExp::Vertices(side, v0, v1);
    double a = 0.0, b = 0.0;
    BRep_Tool::Range(side, a, b);
    const double same = 1.0e-7 * std::max(1.0, b - a);
    if (std::abs(at - a) < same && !v0.IsNull()) {
      return v0;
    }
    if (std::abs(at - b) < same && !v1.IsNull()) {
      return v1;
    }
    for (const auto& [vertex, known, edge] : m_vertices) {
      if (edge.IsSame(side) && std::abs(known - at) < same) {
        return vertex;
      }
    }
    BRepAdaptor_Curve curve(side);
    TopoDS_Vertex vertex = BRepBuilderAPI_MakeVertex(curve.Value(at)).Vertex();
    BRep_Builder().UpdateVertex(vertex, std::max(BRep_Tool::Tolerance(side), Precision::Confusion()));
    m_vertices.emplace_back(vertex, at, side);
    return vertex;
  }

  std::vector<std::tuple<TopoDS_Vertex, double, TopoDS_Edge>> m_vertices;
  std::vector<std::pair<TopoDS_Edge, double>> m_ends;
  std::map<TopoDS_Face, std::vector<TopoDS_Wire>, std::function<bool(const TopoDS_Face&, const TopoDS_Face&)>> m_wires{
      [](const TopoDS_Face& x, const TopoDS_Face& y) { return x.TShape().get() < y.TShape().get(); }};
};

} // namespace

ShapePtr split_joined_pieces(const Shape& result, const Shape& body, const std::vector<int>& faces,
                             const std::string& feature, const std::string& role, double reach) {
  constexpr double kOn = 1.0e-4;
  std::vector<char> joined(static_cast<std::size_t>(body.face_count()), 0);
  std::set<std::string> piece_names;
  for (const int f : faces) {
    joined[static_cast<std::size_t>(f)] = 1;
    piece_names.insert(body.face_names(f).begin(), body.face_names(f).end());
  }
  // The edges where the pieces met (between two pieces on one surface),
  // and the pieces' other edges (along which roundings run).
  std::vector<int> between;
  std::vector<int> along;
  AncestorMap edge_faces;
  TopExp::MapShapesAndUniqueAncestors(body.occt(), TopAbs_EDGE, TopAbs_FACE, edge_faces);
  for (int e = 0; e < body.edge_count(); ++e) {
    if (!edge_faces.Contains(body.edge(e))) {
      continue;
    }
    std::vector<int> sides;
    for (const TopoDS_Shape& face : edge_faces.FindFromKey(body.edge(e))) {
      sides.push_back(body.face_index(face));
    }
    const auto is_piece = [&](int f) { return f >= 0 && joined[static_cast<std::size_t>(f)] != 0; };
    const bool met = sides.size() == 2 && is_piece(sides[0]) && is_piece(sides[1]) && [&] {
      TopLoc_Location la, lb;
      const occ::handle<Geom_Surface> sa = BRep_Tool::Surface(body.face(sides[0]), la);
      const occ::handle<Geom_Surface> sb = BRep_Tool::Surface(body.face(sides[1]), lb);
      return la.IsEqual(lb) && same_surface(sa, sb);
    }();
    if (met) {
      between.push_back(e);
    } else if (std::any_of(sides.begin(), sides.end(), is_piece)) {
      along.push_back(e);
    }
  }
  if (between.empty()) {
    return std::make_shared<Shape>(result);
  }
  // The cuts: over each face on the joined pieces, the edges where its
  // pieces met; across each face generated along the pieces' edges, where
  // two of those edges met, the iso-line of its surface through that point
  // (the rounding's section there). Each as its parts inside the face.
  const std::string generated = face_name(feature, role) + "(";
  Cuts cuts;
  for (const int e : between) {
    const TopoDS_Edge& edge = body.edge(e);
    const TopoDS_Face& piece = TopoDS::Face(edge_faces.FindFromKey(edge).First());
    const NameList& own = body.face_names(body.face_index(piece));
    double first = 0.0, last = 0.0;
    const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last);
    const occ::handle<Geom2d_Curve> on_piece = BRep_Tool::CurveOnSurface(edge, piece, first, last);
    if (curve.IsNull() || on_piece.IsNull()) {
      continue;
    }
    for (int f = 0; f < result.face_count(); ++f) {
      const NameList& names = result.face_names(f);
      if (std::none_of(own.begin(), own.end(), [&](const std::string& n) {
            return std::find(names.begin(), names.end(), n) != names.end();
          })) {
        continue;
      }
      // (The joined face's surface has the pieces' parameters; on a
      // periodic one, the edge in the period the face holds it in.)
      const TopoDS_Face& face = result.face(f);
      occ::handle<Geom2d_Curve> pcurve = on_piece;
      const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face);
      if (surface->IsUPeriodic()) {
        double u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0;
        BRepTools::UVBounds(face, u0, u1, v0, v1);
        const double period = surface->UPeriod();
        const gp_Pnt2d middle = on_piece->Value(0.5 * (first + last));
        for (int k = static_cast<int>(std::floor((u0 - middle.X()) / period)) - 1;
             middle.X() + k * period <= u1 + period; ++k) {
          const gp_Pnt2d moved(middle.X() + k * period, middle.Y());
          if (BRepClass_FaceClassifier(face, moved, BRep_Tool::Tolerance(face)).State() == TopAbs_IN) {
            pcurve = occ::down_cast<Geom2d_Curve>(on_piece->Translated(gp_Vec2d(k * period, 0.0)));
            break;
          }
        }
      }
      cuts.add(face, curve, pcurve, first, last);
    }
  }
  // Across the generated faces, from where those cuts end on their edges.
  AncestorMap result_edge_faces;
  TopExp::MapShapesAndUniqueAncestors(result.occt(), TopAbs_EDGE, TopAbs_FACE, result_edge_faces);
  const std::vector<std::pair<TopoDS_Edge, double>> ends = cuts.ends();
  for (const auto& [side, at] : ends) {
    if (!result_edge_faces.Contains(side)) {
      continue;
    }
    for (const TopoDS_Shape& next : result_edge_faces.FindFromKey(side)) {
      const NameList& names = result.names_of_face(next);
      if (std::none_of(names.begin(), names.end(), [&](const std::string& n) { return n.rfind(generated, 0) == 0; })) {
        continue;
      }
      const TopoDS_Face& face = TopoDS::Face(next);
      double a = 0.0, b = 0.0;
      const occ::handle<Geom2d_Curve> on_face = BRep_Tool::CurveOnSurface(side, face, a, b);
      if (on_face.IsNull()) {
        continue;
      }
      const gp_Pnt2d uv = on_face->Value(at);
      TopLoc_Location location;
      const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face, location);
      double u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0;
      BRepTools::UVBounds(face, u0, u1, v0, v1);
      double s0 = 0.0, s1 = 0.0, t0 = 0.0, t1 = 0.0;
      surface->Bounds(s0, s1, t0, t1);
      // (Along the rounding: the parameter whose iso-line runs across it,
      // the shorter one.)
      gp_Pnt p;
      gp_Vec du, dv;
      BRepAdaptor_Surface(face, false).D1(uv.X(), uv.Y(), p, du, dv);
      const bool across_v = du.Magnitude() * (u1 - u0) > dv.Magnitude() * (v1 - v0);
      const double first = across_v ? std::max(t0, v0 - 0.05 * (v1 - v0)) : std::max(s0, u0 - 0.05 * (u1 - u0));
      const double last = across_v ? std::min(t1, v1 + 0.05 * (v1 - v0)) : std::min(s1, u1 + 0.05 * (u1 - u0));
      const occ::handle<Geom_Curve> iso = across_v ? surface->UIso(uv.X()) : surface->VIso(uv.Y());
      const occ::handle<Geom2d_Curve> line = across_v ? new Geom2d_Line(gp_Pnt2d(uv.X(), 0.0), gp_Dir2d(0.0, 1.0))
                                                      : new Geom2d_Line(gp_Pnt2d(0.0, uv.Y()), gp_Dir2d(1.0, 0.0));
      cuts.add(face, occ::down_cast<Geom_Curve>(iso->Transformed(location.Transformation())), line, first, last);
    }
  }
  if (cuts.empty()) {
    return std::make_shared<Shape>(result);
  }
  std::vector<Cuts::Pieces> split_faces;
  const occ::handle<BRepTools_ReShape> reshape = new BRepTools_ReShape();
  const TopoDS_Shape split = cuts.apply(result.occt(), split_faces, reshape);
  if (split.IsNull()) {
    return nullptr;
  }
  {
    // (The faces that changed checked: the others are the result's.)
    ShapeMap before;
    TopExp::MapShapes(result.occt(), TopAbs_FACE, before);
    BRep_Builder builder;
    TopoDS_Compound changed;
    builder.MakeCompound(changed);
    for (TopExp_Explorer f(split, TopAbs_FACE); f.More(); f.Next()) {
      if (!before.Contains(f.Current())) {
        builder.Add(changed, f.Current());
      }
    }
    if (!BRepCheck_Analyzer(changed).IsValid()) {
      return nullptr;
    }
  }
  // The pieces' names: a piece of a face over the joined pieces named as
  // the piece it lies on, one of a face generated along their edges as the
  // face generated from the edge it runs along; any other piece keeps its
  // face's names.
  const occ::handle<IntTools_Context> context = new IntTools_Context();
  std::vector<Bnd_Box> along_boxes(along.size());
  for (std::size_t k = 0; k < along.size(); ++k) {
    BRepBndLib::Add(body.edge(along[k]), along_boxes[k]);
  }
  const auto on_piece = [&](int p, const gp_Pnt2d& uv) {
    const TopoDS_Face& piece = body.face(p);
    const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(piece);
    double u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0;
    BRepTools::UVBounds(piece, u0, u1, v0, v1);
    const double period = surface->IsUPeriodic() ? surface->UPeriod() : 0.0;
    const double shift = period > 0.0 ? period * std::floor((u0 - uv.X()) / period) : 0.0;
    for (double u = uv.X() + shift; u <= std::max(u1, uv.X()) + 1.0e-9; u += period > 0.0 ? period : RealLast()) {
      if (u >= u0 - 1.0e-9 &&
          BRepClass_FaceClassifier(piece, gp_Pnt2d(u, uv.Y()), BRep_Tool::Tolerance(piece)).State() == TopAbs_IN) {
        return true;
      }
      if (period <= 0.0) {
        break;
      }
    }
    return false;
  };
  FaceNamer namer(split);
  std::vector<int> split_indices;
  for (const auto& [face, modified] : split_faces) {
    split_indices.push_back(result.face_index(face));
  }
  namer.carry(*reshape->History(), result, split_indices);
  for (const auto& [face, modified] : split_faces) {
    const NameList& names = result.face_names(result.face_index(face));
    const bool over_pieces =
        std::any_of(names.begin(), names.end(), [&](const std::string& n) { return piece_names.count(n) > 0; });
    const bool along_pieces =
        !over_pieces && std::any_of(names.begin(), names.end(),
                                    [&](const std::string& n) { return n.rfind(generated, 0) == 0; });
    for (const TopoDS_Shape& image : modified) {
      gp_Pnt point;
      gp_Pnt2d uv;
      const bool inside = BOPTools_AlgoTools3D::PointInFace(TopoDS::Face(image), point, uv, context) == 0;
      NameList own = names;
      if (inside && over_pieces) {
        double nearest = kOn;
        for (const int p : faces) {
          const NameList& candidate = body.face_names(p);
          if (std::all_of(candidate.begin(), candidate.end(), [&](const std::string& n) {
                return std::find(names.begin(), names.end(), n) != names.end();
              })) {
            // (On the piece's surface of the same parameters, in one of its
            // periods where it is periodic; else by distance.)
            if (on_piece(p, uv)) {
              own = candidate;
              break;
            }
            const double d = distance_to(point, body.face(p));
            if (d < nearest) {
              nearest = d;
              own = candidate;
            }
          }
        }
      } else if (inside && along_pieces) {
        double nearest = reach;
        Bnd_Box at;
        at.Add(point);
        for (std::size_t k = 0; k < along.size(); ++k) {
          const int e = along[k];
          if (along_boxes[k].Distance(at) > nearest) {
            continue;
          }
          const double d = distance_to(point, body.edge(e));
          if (d < nearest && !body.edge_name(e).empty()) {
            nearest = d;
            own = {face_name(feature, role, body.edge_name(e))};
          }
        }
      }
      namer.add(image, own, -1);
    }
  }
  return namer.shape();
}

namespace {

bool elementary(GeomAbs_SurfaceType type) {
  return type == GeomAbs_Plane || type == GeomAbs_Cylinder || type == GeomAbs_Cone || type == GeomAbs_Sphere ||
         type == GeomAbs_Torus;
}

// Whether two elementary surfaces (in place) are one within kOneSurface
// and kOneSurfaceAngle, whatever their frames: their axes and centres, and
// their radii and angles, as the same set of points takes them.
bool one_elementary_surface(const BRepAdaptor_Surface& a, const BRepAdaptor_Surface& b) {
  if (a.GetType() != b.GetType()) {
    return false;
  }
  const auto one_axis = [](const gp_Ax1& x, const gp_Ax1& y) {
    return x.Direction().IsParallel(y.Direction(), kOneSurfaceAngle) &&
           gp_Lin(x).Distance(y.Location()) < kOneSurface;
  };
  switch (a.GetType()) {
  case GeomAbs_Plane: {
    const gp_Pln pa = a.Plane();
    const gp_Pln pb = b.Plane();
    return pa.Axis().Direction().IsParallel(pb.Axis().Direction(), kOneSurfaceAngle) &&
           pa.Distance(pb.Location()) < kOneSurface;
  }
  case GeomAbs_Cylinder: {
    const gp_Cylinder ca = a.Cylinder();
    const gp_Cylinder cb = b.Cylinder();
    return one_axis(ca.Axis(), cb.Axis()) && std::abs(ca.Radius() - cb.Radius()) < kOneSurface;
  }
  case GeomAbs_Cone: {
    // (Both nappes through the apex: the angle's sign follows the axis.)
    const gp_Cone ca = a.Cone();
    const gp_Cone cb = b.Cone();
    return one_axis(ca.Axis(), cb.Axis()) && ca.Apex().Distance(cb.Apex()) < kOneSurface &&
           std::abs(std::abs(ca.SemiAngle()) - std::abs(cb.SemiAngle())) < kOneSurfaceAngle;
  }
  case GeomAbs_Sphere: {
    const gp_Sphere sa = a.Sphere();
    const gp_Sphere sb = b.Sphere();
    return sa.Location().Distance(sb.Location()) < kOneSurface && std::abs(sa.Radius() - sb.Radius()) < kOneSurface;
  }
  case GeomAbs_Torus: {
    const gp_Torus ta = a.Torus();
    const gp_Torus tb = b.Torus();
    return one_axis(ta.Axis(), tb.Axis()) && ta.Location().Distance(tb.Location()) < kOneSurface &&
           std::abs(ta.MajorRadius() - tb.MajorRadius()) < kOneSurface &&
           std::abs(ta.MinorRadius() - tb.MinorRadius()) < kOneSurface;
  }
  default:
    return false;
  }
}

// The parameters on an elementary surface (in place) of a point on it.
gp_Pnt2d parameters_on(const BRepAdaptor_Surface& surface, const gp_Pnt& point) {
  double u = 0.0;
  double v = 0.0;
  switch (surface.GetType()) {
  case GeomAbs_Plane:
    ElSLib::Parameters(surface.Plane(), point, u, v);
    break;
  case GeomAbs_Cylinder:
    ElSLib::Parameters(surface.Cylinder(), point, u, v);
    break;
  case GeomAbs_Cone:
    ElSLib::Parameters(surface.Cone(), point, u, v);
    break;
  case GeomAbs_Sphere:
    ElSLib::Parameters(surface.Sphere(), point, u, v);
    break;
  case GeomAbs_Torus:
    ElSLib::Parameters(surface.Torus(), point, u, v);
    break;
  default:
    break;
  }
  return gp_Pnt2d(u, v);
}

// The face's normal (out of its material) at a point on its elementary
// surface; none where the surface has none (a cone's apex, a pole).
std::optional<gp_Dir> normal_at(const TopoDS_Face& face, const BRepAdaptor_Surface& surface, const gp_Pnt& point) {
  if (!elementary(surface.GetType())) {
    return std::nullopt;
  }
  const gp_Pnt2d uv = parameters_on(surface, point);
  gp_Pnt at;
  gp_Vec du;
  gp_Vec dv;
  surface.D1(uv.X(), uv.Y(), at, du, dv);
  gp_Vec normal = du.Crossed(dv);
  if (normal.Magnitude() < 1.0e-12) {
    return std::nullopt;
  }
  if (face.Orientation() == TopAbs_REVERSED) {
    normal.Reverse();
  }
  return gp_Dir(normal);
}

// The faces of a body on one elementary surface that meet along edges:
// each group's faces and the edges between them (indices).
struct SurfaceGroup {
  std::vector<int> faces;
  std::vector<int> inside;
};

// How far round its surface's period a face on a cylinder, cone, sphere
// or torus runs (radians), and whether it is closed there (has a seam).
std::pair<double, bool> turn_of(const TopoDS_Face& face) {
  double u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  bool seam = false;
  for (TopExp_Explorer x(face, TopAbs_EDGE); x.More() && !seam; x.Next()) {
    seam = BRep_Tool::IsClosed(TopoDS::Edge(x.Current()), face);
  }
  return {u1 - u0, seam};
}

constexpr double kTurn = 6.28318530717958647692;

// Whether a face winds round its surface's axis: a whole turn or more
// without being closed round it, as a turn of a thread's root or crest (a
// face each, which the thread keeps so: detail::split_joined_pieces). A
// face closed round it (its seam) may span more than a turn of parameters
// (a rim and the wall above it joined, their seams at other angles), but
// not two.

bool winds_round(const TopoDS_Face& face) {
  const GeomAbs_SurfaceType type = BRepAdaptor_Surface(face, false).GetType();
  if (type != GeomAbs_Cylinder && type != GeomAbs_Cone && type != GeomAbs_Sphere && type != GeomAbs_Torus) {
    return false;
  }
  const auto [turn, seam] = turn_of(face);
  return turn > (seam ? 2.0 * kTurn : kTurn) - kOneSurfaceAngle;
}

std::vector<SurfaceGroup> same_surface_groups(const Shape& body) {
  AncestorMap edge_faces;
  TopExp::MapShapesAndUniqueAncestors(body.occt(), TopAbs_EDGE, TopAbs_FACE, edge_faces);
  const auto count = static_cast<std::size_t>(body.face_count());
  std::vector<occ::handle<BRepAdaptor_Surface>> surfaces(count);
  const auto surface_of = [&](int f) -> const BRepAdaptor_Surface& {
    occ::handle<BRepAdaptor_Surface>& s = surfaces[static_cast<std::size_t>(f)];
    if (s.IsNull()) {
      s = new BRepAdaptor_Surface(body.face(f), false);
    }
    return *s;
  };
  std::vector<int> parent(count);
  for (std::size_t i = 0; i < count; ++i) {
    parent[i] = static_cast<int>(i);
  }
  const auto root = [&parent](int f) {
    while (parent[static_cast<std::size_t>(f)] != f) {
      parent[static_cast<std::size_t>(f)] = parent[static_cast<std::size_t>(parent[static_cast<std::size_t>(f)])];
      f = parent[static_cast<std::size_t>(f)];
    }
    return f;
  };
  std::vector<std::pair<int, int>> joints; // edge, one of its faces
  for (int i = 1; i <= edge_faces.Extent(); ++i) {
    const TopoDS_Edge& edge = TopoDS::Edge(edge_faces.FindKey(i));
    const NCollection_List<TopoDS_Shape>& faces = edge_faces.FindFromIndex(i);
    if (faces.Extent() != 2 || BRep_Tool::Degenerated(edge)) {
      continue;
    }
    const TopoDS_Face& fa = TopoDS::Face(faces.First());
    const TopoDS_Face& fb = TopoDS::Face(faces.Last());
    const int a = body.face_index(fa);
    const int b = body.face_index(fb);
    const int e = body.edge_index(edge);
    if (a < 0 || b < 0 || a == b || e < 0 || !elementary(surface_of(a).GetType()) ||
        !one_elementary_surface(surface_of(a), surface_of(b))) {
      continue;
    }
    // (Facing the same way where they meet: not a fin folded back.)
    const BRepAdaptor_Curve curve(edge);
    const gp_Pnt middle = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
    const std::optional<gp_Dir> na = normal_at(fa, surface_of(a), middle);
    const std::optional<gp_Dir> nb = normal_at(fb, surface_of(b), middle);
    if (!na || !nb || na->Dot(*nb) < 0.5) {
      continue;
    }
    parent[static_cast<std::size_t>(root(a))] = root(b);
    joints.emplace_back(e, a);
  }
  std::map<int, SurfaceGroup> by_root;
  for (std::size_t f = 0; f < count; ++f) {
    by_root[root(static_cast<int>(f))].faces.push_back(static_cast<int>(f));
  }
  for (const auto& [e, a] : joints) {
    by_root[root(a)].inside.push_back(e);
  }
  std::vector<SurfaceGroup> groups;
  for (auto& entry : by_root) {
    const std::vector<int>& faces = entry.second.faces;
    if (faces.size() > 1 && !entry.second.inside.empty() &&
        std::none_of(faces.begin(), faces.end(), [&body](int f) { return winds_round(body.face(f)); })) {
      groups.push_back(std::move(entry.second));
    }
  }
  return groups;
}

// The parameters of one cylinder, cone, sphere or torus as those of another
// of the same points (one_elementary_surface): u' = a u + b, v' = c v + d,
// a and c each 1 or -1 (an axis or a frame the other way round).
struct ParameterMap {
  double a = 1.0;
  double b = 0.0;
  double c = 1.0;
  double d = 0.0;

  gp_Pnt2d operator()(const gp_Pnt2d& p) const { return gp_Pnt2d(a * p.X() + b, c * p.Y() + d); }

  // A curve on the first surface as the same curve on the second.
  occ::handle<Geom2d_Curve> moved(const occ::handle<Geom2d_Curve>& curve) const {
    const occ::handle<Geom2d_Curve> copy = occ::down_cast<Geom2d_Curve>(curve->Copy());
    gp_Trsf2d turn;
    if (a < 0.0 && c < 0.0) {
      turn.SetRotation(gp::Origin2d(), kTurn / 2.0);
    } else if (a < 0.0) {
      turn.SetMirror(gp::OY2d());
    } else if (c < 0.0) {
      turn.SetMirror(gp::OX2d());
    }
    copy->Transform(turn);
    copy->Translate(gp_Vec2d(b, d));
    return copy;
  }
};

// The map of `from`'s parameters onto `to`'s, measured about the
// parameters `at` on `from`; none where the surfaces' parameters do not
// run along each other there.
std::optional<ParameterMap> parameter_map(const BRepAdaptor_Surface& from, const BRepAdaptor_Surface& to,
                                          const gp_Pnt2d& at) {
  constexpr double kStep = 1.0e-3;
  const auto wrapped = [](double delta, bool periodic) {
    if (periodic) {
      delta -= kTurn * std::round(delta / kTurn);
    }
    return delta;
  };
  const bool u_periodic = to.IsUPeriodic();
  const bool v_periodic = to.IsVPeriodic();
  const gp_Pnt2d p0 = parameters_on(to, from.Value(at.X(), at.Y()));
  const gp_Pnt2d pu = parameters_on(to, from.Value(at.X() + kStep, at.Y()));
  const gp_Pnt2d pv = parameters_on(to, from.Value(at.X(), at.Y() + kStep));
  const double uu = wrapped(pu.X() - p0.X(), u_periodic) / kStep;
  const double vu = wrapped(pu.Y() - p0.Y(), v_periodic) / kStep;
  const double uv = wrapped(pv.X() - p0.X(), u_periodic) / kStep;
  const double vv = wrapped(pv.Y() - p0.Y(), v_periodic) / kStep;
  if (std::abs(std::abs(uu) - 1.0) > 1.0e-4 || std::abs(std::abs(vv) - 1.0) > 1.0e-4 || std::abs(vu) > 1.0e-4 ||
      std::abs(uv) > 1.0e-4) {
    return std::nullopt;
  }
  ParameterMap map;
  map.a = uu > 0.0 ? 1.0 : -1.0;
  map.c = vv > 0.0 ? 1.0 : -1.0;
  map.b = p0.X() - map.a * at.X();
  map.d = p0.Y() - map.c * at.Y();
  return map;
}

// `piece` (a face of a group, on a cylinder, cone, sphere or torus of the
// reference's, one_elementary_surface) on the reference's own surface: its
// edges' curves on it moved onto that surface (the edges it shares with a
// face already there, `reference` or a piece moved before, keep the curve
// they have there, and set its periods), the face facing as it did. OCCT's
// merge then takes the pieces as faces of one surface; its own move of a
// face's curves onto another frame's surface knows only cylinders (the
// periods of a torus's other parameter, the length along a cone's side).
// Null where the piece cannot be moved.
TopoDS_Face on_reference_surface(const TopoDS_Face& piece, const TopoDS_Face& reference) {
  const TopoDS_Face forward = TopoDS::Face(piece.Oriented(TopAbs_FORWARD));
  TopLoc_Location location;
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(reference, location);
  const BRepAdaptor_Surface from(forward, false);
  const BRepAdaptor_Surface to(reference, false);
  double u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0;
  BRepTools::UVBounds(forward, u0, u1, v0, v1);
  gp_Pnt2d at(0.5 * (u0 + u1), 0.5 * (v0 + v1));
  if (from.GetType() == GeomAbs_Sphere) {
    at.SetY(0.5 * at.Y()); // (away from a pole)
  }
  std::optional<ParameterMap> map = parameter_map(from, to, at);
  if (!map) {
    return TopoDS_Face();
  }
  // (The periods from an edge already on the reference's surface.)
  bool aligned = false;
  for (TopExp_Explorer x(forward, TopAbs_EDGE); x.More() && !aligned; x.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(x.Current());
    double f = 0.0, l = 0.0, g = 0.0, h = 0.0;
    if (BRep_Tool::Degenerated(edge) || BRep_Tool::IsClosed(edge, forward)) {
      continue;
    }
    const occ::handle<Geom2d_Curve> there = BRep_Tool::CurveOnSurface(edge, reference, f, l);
    const occ::handle<Geom2d_Curve> here = BRep_Tool::CurveOnSurface(edge, forward, g, h);
    if (there.IsNull() || here.IsNull()) {
      continue;
    }
    const gp_Pnt2d want = there->Value(0.5 * (f + l));
    const gp_Pnt2d got = (*map)(here->Value(0.5 * (g + h)));
    double du = want.X() - got.X();
    double dv = want.Y() - got.Y();
    if (to.IsUPeriodic()) {
      const double shift = kTurn * std::round(du / kTurn);
      map->b += shift;
      du -= shift;
    }
    if (to.IsVPeriodic()) {
      const double shift = kTurn * std::round(dv / kTurn);
      map->d += shift;
      dv -= shift;
    }
    if (std::abs(du) > 1.0e-5 || std::abs(dv) > 1.0e-5) {
      return TopoDS_Face();
    }
    aligned = true;
  }
  if (!aligned) {
    return TopoDS_Face();
  }
  // Facing as it did: where the reference's surface faces the other way,
  // the face holds its wires the other way round.
  gp_Pnt point;
  gp_Vec du, dv, tu, tv;
  from.D1(at.X(), at.Y(), point, du, dv);
  const gp_Pnt2d mapped = (*map)(at);
  to.D1(mapped.X(), mapped.Y(), point, tu, tv);
  const bool flip = du.Crossed(dv).Dot(tu.Crossed(tv)) < 0.0;
  BRep_Builder builder;
  TopoDS_Face moved;
  builder.MakeFace(moved, surface, location, BRep_Tool::Tolerance(forward));
  ShapeMap done;
  for (TopExp_Explorer x(forward, TopAbs_EDGE); x.More(); x.Next()) {
    const TopoDS_Edge edge = TopoDS::Edge(x.Current().Oriented(TopAbs_FORWARD));
    const int known = done.Extent();
    if (done.Add(edge) <= known) {
      continue;
    }
    double f = 0.0, l = 0.0;
    const double tolerance = BRep_Tool::Tolerance(edge);
    if (BRep_Tool::IsClosed(edge, forward)) {
      const occ::handle<Geom2d_Curve> c1 = BRep_Tool::CurveOnSurface(edge, forward, f, l);
      const occ::handle<Geom2d_Curve> c2 =
          BRep_Tool::CurveOnSurface(TopoDS::Edge(edge.Reversed()), forward, f, l);
      if (c1.IsNull() || c2.IsNull()) {
        return TopoDS_Face();
      }
      const occ::handle<Geom2d_Curve> m1 = map->moved(c1);
      const occ::handle<Geom2d_Curve> m2 = map->moved(c2);
      if (flip) {
        builder.UpdateEdge(edge, m2, m1, moved, tolerance);
      } else {
        builder.UpdateEdge(edge, m1, m2, moved, tolerance);
      }
    } else {
      const occ::handle<Geom2d_Curve> c = BRep_Tool::CurveOnSurface(edge, forward, f, l);
      if (c.IsNull()) {
        return TopoDS_Face();
      }
      const occ::handle<Geom2d_Curve> m = map->moved(c);
      double g = 0.0, h = 0.0;
      const occ::handle<Geom2d_Curve> there = BRep_Tool::CurveOnSurface(edge, reference, g, h);
      if (!there.IsNull()) {
        // On the reference's surface already: the same curve, or a period
        // away where the pieces close round the surface, which makes the
        // edge a seam (its curve there for the way the face it came with
        // takes it, the new one for the way this one does).
        if (there->Value(0.5 * (g + h)).Distance(m->Value(0.5 * (f + l))) < 1.0e-5) {
          continue;
        }
        TopAbs_Orientation here = TopAbs_FORWARD;
        for (TopExp_Explorer y(forward, TopAbs_EDGE); y.More(); y.Next()) {
          if (y.Current().IsSame(edge)) {
            here = y.Current().Orientation();
          }
        }
        if (flip) {
          here = TopAbs::Reverse(here);
        }
        if (here == TopAbs_FORWARD) {
          builder.UpdateEdge(edge, m, there, moved, tolerance);
        } else {
          builder.UpdateEdge(edge, there, m, moved, tolerance);
        }
      } else {
        builder.UpdateEdge(edge, m, moved, tolerance);
      }
    }
    builder.Range(edge, moved, f, l);
    if (!BRep_Tool::Degenerated(edge)) {
      // (Its tolerance to the curve's distance from the reference's
      // surface: within kOneSurface.)
      builder.SameParameter(edge, false);
      BRepLib::SameParameter(edge, tolerance);
    }
  }
  for (TopoDS_Iterator w(forward); w.More(); w.Next()) {
    builder.Add(moved, flip ? w.Value().Reversed() : w.Value());
  }
  // (Its vertices within their edges' new tolerances.)
  BRepLib::UpdateTolerances(moved, false);
  const TopAbs_Orientation orientation = piece.Orientation();
  moved.Orientation(flip ? TopAbs::Reverse(orientation) : orientation);
  return moved;
}

// The pieces of the groups `chosen` of `body` (a copy) on their group's
// first piece's surface (on_reference_surface), each moved after a piece
// it shares an edge with; planes as they are (OCCT's merge puts coplanar
// faces on one plane itself), and a piece that cannot be moved.
TopoDS_Shape on_reference_surfaces(const Shape& body, const std::vector<SurfaceGroup>& groups,
                                   const std::vector<std::size_t>& chosen,
                                   const occ::handle<BRepTools_ReShape>& reshape) {
  for (const std::size_t g : chosen) {
    const SurfaceGroup& group = groups[g];
    const TopoDS_Face& reference = body.face(group.faces.front());
    if (BRepAdaptor_Surface(reference, false).GetType() == GeomAbs_Plane) {
      continue;
    }
    TopLoc_Location lr;
    const occ::handle<Geom_Surface> sr = BRep_Tool::Surface(reference, lr);
    std::vector<int> placed{group.faces.front()};
    std::vector<int> left(group.faces.begin() + 1, group.faces.end());
    // Next to a placed piece across an edge between them.
    const auto touches = [&](int f) {
      for (const int e : group.inside) {
        const TopoDS_Shape& edge = body.edge(e);
        const auto has = [&](int face) {
          for (TopExp_Explorer x(body.face(face), TopAbs_EDGE); x.More(); x.Next()) {
            if (x.Current().IsSame(edge)) {
              return true;
            }
          }
          return false;
        };
        if (has(f) && std::any_of(placed.begin(), placed.end(), has)) {
          return true;
        }
      }
      return false;
    };
    bool progress = true;
    while (!left.empty() && progress) {
      progress = false;
      for (auto it = left.begin(); it != left.end(); ++it) {
        if (!touches(*it)) {
          continue;
        }
        const TopoDS_Face& piece = body.face(*it);
        TopLoc_Location lp;
        if (BRep_Tool::Surface(piece, lp) != sr || !lp.IsEqual(lr)) {
          const TopoDS_Face moved = on_reference_surface(piece, reference);
          if (!moved.IsNull()) {
            reshape->Replace(piece, moved);
          }
        }
        placed.push_back(*it);
        left.erase(it);
        progress = true;
        break;
      }
    }
  }
  return reshape->Apply(body.occt());
}

// The body with the groups `chosen` joined, on a copy of it: null where
// the merge joined none of them or the faces it changed are not valid
// (`invalid` then set) or a group joined into a face winding round its
// axis (winds_round: `winding` gets those, to be joined again without
// them); `unjoined` gets the chosen groups the merge left in pieces. With
// `divide_closed` the faces closed round their surface (their seam) are cut
// in two first (ShapeUpgrade_ShapeDivideClosed), each piece then open:
// OCCT's merge leaves a closed face apart from an open piece of its surface
// that meets it where the closed face's seam runs on along the piece's edge
// (a pin's ring whose seam goes on along the edge of the strip of its
// cylinder below it, mitcad#1336), but joins the open pieces into one face
// closed round it.
ShapePtr joined_groups(const Shape& body, const std::vector<SurfaceGroup>& groups,
                       const std::vector<std::size_t>& chosen, std::vector<std::size_t>& unjoined,
                       std::vector<std::size_t>& winding, bool& invalid, bool divide_closed = false) {
  winding.clear();
  unjoined.clear();
  invalid = false;
  const ShapePtr own = working_copy(body);
  own_pcurves(own->occt());
  // Only the edges between the faces of a group can go, and only the edges
  // along those faces' boundaries join.
  std::vector<char> free_edge(static_cast<std::size_t>(own->edge_count()), 0);
  ShapeMap free_vertices;
  for (const std::size_t g : chosen) {
    for (const int e : groups[g].inside) {
      free_edge[static_cast<std::size_t>(e)] = 1;
    }
    for (const int f : groups[g].faces) {
      TopExp::MapShapes(own->face(f), TopAbs_VERTEX, free_vertices);
      // (A piece's seam too: the merge keeps the edges it is to keep
      // inside the faces it joins, which a seam is.)
      for (TopExp_Explorer x(own->face(f), TopAbs_EDGE); x.More(); x.Next()) {
        if (BRep_Tool::IsClosed(TopoDS::Edge(x.Current()), own->face(f))) {
          free_edge[static_cast<std::size_t>(own->edge_index(x.Current()))] = 1;
        }
      }
    }
  }
  if (divide_closed) {
    // (Every closed face's seam, as the cuts below make two pieces of
    // every closed face, which the merge joins again across their cut and
    // their seam into one face closed round its surface.)
    for (int f = 0; f < own->face_count(); ++f) {
      for (TopExp_Explorer x(own->face(f), TopAbs_EDGE); x.More(); x.Next()) {
        if (BRep_Tool::IsClosed(TopoDS::Edge(x.Current()), own->face(f))) {
          free_edge[static_cast<std::size_t>(own->edge_index(x.Current()))] = 1;
        }
      }
    }
  }
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> keep;
  for (int e = 0; e < own->edge_count(); ++e) {
    if (free_edge[static_cast<std::size_t>(e)] == 0) {
      keep.Add(own->edge(e));
    }
  }
  for (int v = 0; v < own->vertex_count(); ++v) {
    if (!free_vertices.Contains(own->vertex(v))) {
      keep.Add(own->vertex(v));
    }
  }
  const occ::handle<BRepTools_ReShape> reshape = new BRepTools_ReShape();
  TopoDS_Shape input = on_reference_surfaces(*own, groups, chosen, reshape);
  occ::handle<BRepTools_History> divided;
  if (divide_closed) {
    ShapeUpgrade_ShapeDivideClosed divide(input);
    divide.SetNbSplitPoints(1);
    divide.Perform();
    if (!divide.Status(ShapeExtend_DONE) || divide.Result().IsNull()) {
      unjoined = chosen;
      return nullptr;
    }
    input = divide.Result();
    divided = divide.GetContext()->History();
    // (The kept edges the cuts split: their pieces kept.)
    NCollection_List<TopoDS_Shape> pieces;
    for (NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher>::Iterator k(keep); k.More(); k.Next()) {
      for (const TopoDS_Shape& piece : divided->Modified(k.Key())) {
        pieces.Append(piece);
      }
    }
    for (const TopoDS_Shape& piece : pieces) {
      keep.Add(piece);
    }
  }
  ShapeUpgrade_UnifySameDomain unify(input, true, true, false);
  unify.KeepShapes(keep);
  unify.SetLinearTolerance(kOneSurface);
  unify.SetAngularTolerance(kOneSurfaceAngle);
  unify.SetSafeInputMode(true);
  unify.AllowInternalEdges(false);
  unify.Build();
  const TopoDS_Shape result = unify.Shape();
  if (result.IsNull() || unify.History().IsNull()) {
    invalid = true;
    return nullptr;
  }
  const occ::handle<BRepTools_History> history = reshape->History();
  if (!divided.IsNull()) {
    history->Merge(divided);
  }
  history->Merge(unify.History());
  ShapeMap result_faces;
  TopExp::MapShapes(result, TopAbs_FACE, result_faces);
  for (const std::size_t g : chosen) {
    ShapeMap images;
    for (const int f : groups[g].faces) {
      const TopoDS_Shape& piece = own->face(f);
      if (history->IsRemoved(piece)) {
        continue;
      }
      const NCollection_List<TopoDS_Shape>& modified = history->Modified(piece);
      if (modified.IsEmpty()) {
        images.Add(piece);
      }
      for (const TopoDS_Shape& image : modified) {
        if (result_faces.Contains(image)) {
          images.Add(image);
        }
      }
    }
    if (images.Extent() != 1) {
      unjoined.push_back(g);
    } else if (winds_round(TopoDS::Face(images(1)))) {
      winding.push_back(g);
    }
  }
  if (!winding.empty() || unjoined.size() == chosen.size()) {
    return nullptr;
  }
  // (The faces the merge left as they were are the body's: valid as it
  // was found.)
  ShapeMap kept;
  TopExp::MapShapes(own->occt(), TopAbs_FACE, kept);
  if (!is_valid_where_changed(result, kept)) {
    invalid = true;
    return nullptr;
  }
  FaceNamer namer(result);
  namer.carry(*history, *own);
  namer.set_unified(body.unified());
  const ShapePtr joined = namer.shape();
  joined->copy_notes(body);
  if (body.checked_valid().value_or(false)) {
    joined->set_checked_valid(true);
  }
  return joined;
}

// The first names of a group's faces, for a message.
std::string group_names(const Shape& body, const SurfaceGroup& group) {
  std::string names;
  for (const int f : group.faces) {
    if (!body.face_names(f).empty()) {
      names += (names.empty() ? "" : ", ") + body.face_names(f).front();
    }
  }
  return names;
}

// Whether a face closed round its surface's axis (a cylinder, cone, sphere
// or torus) has its seam at more than one angle, or holds on both of its
// sides an edge that is no seam line: pieces with their seams at other
// angles joined (OCCT's merge keeps each piece's seam), the rim between the
// seams left between them as an edge.
bool seam_in_pieces(const TopoDS_Face& face) {
  const GeomAbs_SurfaceType type = BRepAdaptor_Surface(face, false).GetType();
  if (type != GeomAbs_Cylinder && type != GeomAbs_Cone && type != GeomAbs_Sphere && type != GeomAbs_Torus) {
    return false;
  }
  const TopoDS_Face forward = TopoDS::Face(face.Oriented(TopAbs_FORWARD));
  std::optional<double> angle;
  for (TopExp_Explorer x(forward, TopAbs_EDGE); x.More(); x.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(x.Current());
    if (BRep_Tool::Degenerated(edge) || !BRep_Tool::IsClosed(edge, forward)) {
      continue;
    }
    double f = 0.0, l = 0.0;
    const occ::handle<Geom2d_Curve> curve = BRep_Tool::CurveOnSurface(edge, forward, f, l);
    if (curve.IsNull()) {
      continue;
    }
    const gp_Pnt2d a = curve->Value(f);
    const gp_Pnt2d b = curve->Value(l);
    const gp_Pnt2d m = curve->Value(0.5 * (f + l));
    // (A seam runs along v at one u.)
    if (std::abs(a.X() - b.X()) > 1.0e-9 || std::abs(a.X() - m.X()) > 1.0e-9) {
      return true;
    }
    const double u = a.X() - kTurn * std::floor(a.X() / kTurn);
    if (angle) {
      const double apart = std::abs(u - *angle);
      if (std::min(apart, kTurn - apart) > 1.0e-9) {
        return true;
      }
    }
    angle = u;
  }
  return false;
}

// `body` with each face whose seam is in pieces (seam_in_pieces) made anew
// from its other edges with one seam (OCCT's shape healing, its missing
// seam put in: the edges of the rims it crosses split there, in the
// neighbouring faces too). Faces it cannot make so stay as they are.
ShapePtr with_single_seams(const ShapePtr& body) {
  std::vector<int> faces;
  for (int i = 0; i < body->face_count(); ++i) {
    if (seam_in_pieces(body->face(i))) {
      faces.push_back(i);
    }
  }
  if (faces.empty()) {
    return body;
  }
  const ShapePtr own = working_copy(*body);
  own_pcurves(own->occt());
  const occ::handle<ShapeBuild_ReShape> context = new ShapeBuild_ReShape();
  bool any = false;
  for (const int i : faces) {
    try {
      OCC_CATCH_SIGNALS
      const TopoDS_Face forward = TopoDS::Face(own->face(i).Oriented(TopAbs_FORWARD));
      const occ::handle<NCollection_HSequence<TopoDS_Shape>> edges = new NCollection_HSequence<TopoDS_Shape>();
      for (TopExp_Explorer x(forward, TopAbs_EDGE); x.More(); x.Next()) {
        if (!BRep_Tool::IsClosed(TopoDS::Edge(x.Current()), forward)) {
          edges->Append(x.Current());
        }
      }
      const occ::handle<NCollection_HSequence<TopoDS_Shape>> wires =
          ShapeAnalysis_FreeBounds::ConnectEdgesToWires(edges, Precision::Confusion(), true);
      TopLoc_Location location;
      BRep_Builder builder;
      TopoDS_Face bare;
      builder.MakeFace(bare, BRep_Tool::Surface(forward, location), location, BRep_Tool::Tolerance(forward));
      for (const TopoDS_Shape& wire : *wires) {
        builder.Add(bare, wire);
      }
      ShapeFix_Face fix(bare);
      fix.SetContext(context);
      fix.Perform();
      const TopoDS_Face fixed = fix.Face();
      // (The rims' edges the healing split, their ranges and parameters
      // as their curves have them.)
      for (TopExp_Explorer y(fixed, TopAbs_EDGE); y.More(); y.Next()) {
        const TopoDS_Edge& edge = TopoDS::Edge(y.Current());
        if (!BRep_Tool::Degenerated(edge) && (!BRep_Tool::SameRange(edge) || !BRep_Tool::SameParameter(edge))) {
          BRepLib::SameRange(edge);
          builder.SameParameter(edge, false);
          BRepLib::SameParameter(edge, BRep_Tool::Tolerance(edge));
        }
      }
      int count = 0;
      for (TopExp_Explorer x(fixed, TopAbs_FACE); x.More(); x.Next()) {
        ++count;
      }
      if (fixed.IsNull() || count != 1 || seam_in_pieces(fixed)) {
        continue;
      }
      context->Replace(forward, fixed);
      any = true;
    } catch (const Standard_Failure&) {
    }
  }
  if (!any) {
    return body;
  }
  const TopoDS_Shape applied = context->Apply(own->occt());
  ShapeMap kept;
  TopExp::MapShapes(own->occt(), TopAbs_FACE, kept);
  // The faces it changed (the seamed ones, the neighbours whose rims it
  // split) healed again: their edges' curves on them and parameters.
  const occ::handle<ShapeBuild_ReShape> healing = new ShapeBuild_ReShape();
  for (TopExp_Explorer y(applied, TopAbs_FACE); y.More(); y.Next()) {
    if (kept.Contains(y.Current())) {
      continue;
    }
    try {
      OCC_CATCH_SIGNALS
      ShapeFix_Face fix(TopoDS::Face(y.Current()));
      fix.SetContext(healing);
      fix.Perform();
      if (!fix.Face().IsSame(y.Current())) {
        healing->Replace(y.Current(), fix.Face());
      }
    } catch (const Standard_Failure&) {
    }
  }
  const TopoDS_Shape healed = healing->Apply(applied);
  const occ::handle<BRepTools_History> history = context->History();
  history->Merge(healing->History());
  // (The rims' edges joined again where their old seams met them: OCCT's
  // merge on edges, at the vertices of the faces changed only.)
  ShapeMap free_vertices;
  for (TopExp_Explorer y(healed, TopAbs_FACE); y.More(); y.Next()) {
    if (!kept.Contains(y.Current())) {
      TopExp::MapShapes(y.Current(), TopAbs_VERTEX, free_vertices);
    }
  }
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> keep;
  for (TopExp_Explorer y(healed, TopAbs_VERTEX); y.More(); y.Next()) {
    if (!free_vertices.Contains(y.Current())) {
      keep.Add(y.Current());
    }
  }
  ShapeUpgrade_UnifySameDomain rims(healed, true, false, false);
  rims.KeepShapes(keep);
  rims.SetSafeInputMode(true);
  rims.Build();
  const TopoDS_Shape result = rims.Shape();
  if (!rims.History().IsNull()) {
    history->Merge(rims.History());
  }
  if (result.IsNull() || !is_valid_where_changed(result, kept)) {
    return body;
  }
  FaceNamer namer(result);
  namer.carry(*history, *own);
  namer.set_unified(body->unified());
  const ShapePtr seamed = namer.shape();
  seamed->copy_notes(*body);
  if (body->checked_valid().value_or(false)) {
    seamed->set_checked_valid(true);
  }
  return seamed;
}

ShapePtr joined_on_surfaces(const ShapePtr& body);

} // namespace

ShapePtr join_same_surfaces(const ShapePtr& body) {
  if (!body || body->face_count() < 2) {
    return body;
  }
  return with_single_seams(joined_on_surfaces(body));
}

namespace {

ShapePtr joined_on_surfaces(const ShapePtr& body) {
  if (!body || body->face_count() < 2) {
    return body;
  }
  const std::vector<SurfaceGroup> groups = same_surface_groups(*body);
  if (groups.empty()) {
    return body;
  }
  std::vector<std::size_t> all(groups.size());
  for (std::size_t g = 0; g < groups.size(); ++g) {
    all[g] = g;
  }
  // (A group the merge itself does not join, or that it would join into a
  // face winding round its axis, stays apart without a word: nothing is
  // wrong with it.)
  std::vector<std::size_t> winding;
  bool divide_closed = false;
  const auto attempt = [&](const std::vector<std::size_t>& chosen, std::vector<std::size_t>& left,
                           bool& bad) -> ShapePtr {
    try {
      return joined_groups(*body, groups, chosen, left, winding, bad, divide_closed);
    } catch (const Standard_Failure&) {
      bad = true;
      return nullptr;
    }
  };
  std::vector<std::size_t> unjoined;
  bool invalid = false;
  ShapePtr joined = attempt(all, unjoined, invalid);
  // (A group the merge left apart that holds a face closed round its
  // surface: all of them again with the closed faces cut in two first,
  // where that joins more of them.)
  const auto closed_face = [&](std::size_t g) {
    return std::any_of(groups[g].faces.begin(), groups[g].faces.end(), [&](int f) {
      for (TopExp_Explorer x(body->face(f), TopAbs_EDGE); x.More(); x.Next()) {
        if (BRep_Tool::IsClosed(TopoDS::Edge(x.Current()), body->face(f))) {
          return true;
        }
      }
      return false;
    });
  };
  if (!invalid && winding.empty() && std::any_of(unjoined.begin(), unjoined.end(), closed_face)) {
    divide_closed = true;
    std::vector<std::size_t> left;
    bool bad = false;
    const ShapePtr divided = attempt(all, left, bad);
    if (divided && !bad && winding.empty() && left.size() < unjoined.size()) {
      joined = divided;
      unjoined = left;
    } else {
      divide_closed = false;
      winding.clear();
    }
  }
  while (!joined && !invalid && !winding.empty()) {
    for (const std::size_t g : winding) {
      all.erase(std::remove(all.begin(), all.end(), g), all.end());
    }
    if (all.empty()) {
      return body;
    }
    joined = attempt(all, unjoined, invalid);
  }
  std::vector<std::size_t> refused;
  if (!joined && invalid) {
    // Group by group: each joined where the result stays valid with the
    // groups joined before it.
    std::vector<std::size_t> accepted;
    for (const std::size_t g : all) {
      std::vector<std::size_t> trial = accepted;
      trial.push_back(g);
      ShapePtr step = attempt(trial, unjoined, invalid);
      if (step && std::find(unjoined.begin(), unjoined.end(), g) == unjoined.end()) {
        accepted = trial;
        joined = step;
      } else if (invalid) {
        refused.push_back(g);
      }
    }
  }
  if (refused.empty()) {
    return joined ? joined : body;
  }
  if (!joined) {
    joined = std::make_shared<Shape>(*body);
  }
  for (const std::size_t g : refused) {
    const std::string names = group_names(*body, groups[g]);
    if (!names.empty()) {
      joined->add_info("the faces " + names +
                       " lie on one surface but stay apart: joined, the body would not be valid");
    }
  }
  return joined;
}

} // namespace

} // namespace mitcad::geometry::detail
