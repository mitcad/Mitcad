// SPDX-License-Identifier: MIT
// Nearness of shapes and points on surfaces without more measuring than
// the answer needs (proximity.hpp, mitcad#1785).
#include "proximity.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <tuple>
#include <utility>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepTools.hxx>
#include <BRepExtrema_DistanceSS.hxx>
#include <BRepExtrema_SolutionElem.hxx>
#include <BRep_Tool.hxx>
#include <GeomConvert.hxx>
#include <GeomConvert_BSplineCurveToBezierCurve.hxx>
#include <Geom_Circle.hxx>
#include <Geom_Ellipse.hxx>
#include <Geom_Line.hxx>
#include <GeomConvert_BSplineSurfaceToBezierSurface.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_BezierCurve.hxx>
#include <Geom_BezierSurface.hxx>
#include <Geom_RectangularTrimmedSurface.hxx>
#include <Geom_TrimmedCurve.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <gp_Trsf.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_Array2.hxx>
#include <NCollection_Sequence.hxx>
#include <Precision.hxx>
#include <TopAbs_ShapeEnum.hxx>
#include <TopExp.hxx>
#include <TopoDS.hxx>
#include <gp.hxx>
#include <gp_Lin.hxx>
#include <gp_Vec.hxx>

namespace mitcad::geometry::detail {
namespace {

// The tolerance BRepExtrema_DistShapeShape classifies a vertex in a solid
// with.
constexpr double kInsideTolerance = 0.001;

// The box of poles, moved by `location` and grown by `tolerance`.
Bnd_Box poles_box(const NCollection_Array1<gp_Pnt>& poles, const TopLoc_Location& location, double tolerance) {
  Bnd_Box box;
  const gp_Trsf move = location.Transformation();
  for (int i = poles.Lower(); i <= poles.Upper(); ++i) {
    box.Add(location.IsIdentity() ? poles(i) : poles(i).Transformed(move));
  }
  box.Enlarge(tolerance);
  return box;
}

Bnd_Box poles_box(const NCollection_Array2<gp_Pnt>& poles, const TopLoc_Location& location, double tolerance) {
  Bnd_Box box;
  const gp_Trsf move = location.Transformation();
  for (int i = poles.LowerRow(); i <= poles.UpperRow(); ++i) {
    for (int j = poles.LowerCol(); j <= poles.UpperCol(); ++j) {
      box.Add(location.IsIdentity() ? poles(i, j) : poles(i, j).Transformed(move));
    }
  }
  box.Enlarge(tolerance);
  return box;
}

using Hull = std::vector<std::array<double, 4>>;

// The homogeneous poles of a Bezier piece moved by `location`; none where a
// weight is not positive (the curve need not lie in the hull of its poles
// then).
Hull hull_of(const occ::handle<Geom_BezierCurve>& arc, const TopLoc_Location& location) {
  Hull hull;
  const gp_Trsf move = location.Transformation();
  for (int i = 1; i <= arc->NbPoles(); ++i) {
    const double w = arc->Weight(i);
    if (!(w > 0.0)) {
      return {};
    }
    const gp_Pnt p = location.IsIdentity() ? arc->Pole(i) : arc->Pole(i).Transformed(move);
    hull.push_back({p.X() * w, p.Y() * w, p.Z() * w, w});
  }
  return hull;
}

// The box of a piece's poles, grown by `tolerance`.
Bnd_Box hull_box(const Hull& hull, double tolerance) {
  Bnd_Box box;
  for (const std::array<double, 4>& p : hull) {
    box.Add(gp_Pnt(p[0] / p[3], p[1] / p[3], p[2] / p[3]));
  }
  box.Enlarge(tolerance);
  return box;
}

// The poles of a piece's two halves (de Casteljau at the middle of its
// parameter, in homogeneous coordinates: the weights stay positive).
std::pair<Hull, Hull> halves(const Hull& hull) {
  Hull work = hull;
  const std::size_t n = work.size();
  Hull low(n), high(n);
  for (std::size_t level = 0; level < n; ++level) {
    low[level] = work[0];
    high[n - 1 - level] = work[n - 1 - level];
    for (std::size_t i = 0; i + 1 < n - level; ++i) {
      for (std::size_t c = 0; c < 4; ++c) {
        work[i][c] = 0.5 * (work[i][c] + work[i + 1][c]);
      }
    }
  }
  return {low, high};
}

// Whether two curve pieces may come within `limit` of each other: their
// poles' boxes (which hold them) do, and so do those of a half of one and
// a half of the other, the larger halved again and again, until the boxes
// are this small or `budget` box pairs are spent (then they may).
constexpr double kHullSmall = 1.0e-4;
constexpr int kHullDepth = 30;

bool hulls_near(const Hull& a, double ta, const Hull& b, double tb, double limit, int depth, int& budget) {
  const Bnd_Box box_a = hull_box(a, ta);
  const Bnd_Box box_b = hull_box(b, tb);
  if (box_a.Distance(box_b) - limit >= Precision::Confusion()) {
    return false;
  }
  if (depth <= 0 || --budget <= 0) {
    return true;
  }
  const double size_a = box_a.SquareExtent();
  const double size_b = box_b.SquareExtent();
  if (std::max(size_a, size_b) <= kHullSmall * kHullSmall) {
    return true;
  }
  if (size_a >= size_b) {
    const std::pair<Hull, Hull> split = halves(a);
    return hulls_near(split.first, ta, b, tb, limit, depth - 1, budget) ||
           hulls_near(split.second, ta, b, tb, limit, depth - 1, budget);
  }
  const std::pair<Hull, Hull> split = halves(b);
  return hulls_near(a, ta, split.first, tb, limit, depth - 1, budget) ||
         hulls_near(a, ta, split.second, tb, limit, depth - 1, budget);
}

// Whether two edges' curves, as their pieces' poles, may come within
// `limit`: any pair of pieces that hulls_near cannot tell apart (true
// where either has no pieces).
constexpr int kHullBudget = 4096;

bool edges_may_meet(const std::vector<Hull>& a, double ta, const std::vector<Hull>& b, double tb, double limit) {
  if (a.empty() || b.empty()) {
    return true;
  }
  int budget = kHullBudget;
  for (const Hull& one : a) {
    for (const Hull& two : b) {
      if (hulls_near(one, ta, two, tb, limit, kHullDepth, budget)) {
        return true;
      }
      if (budget <= 0) {
        return true;
      }
    }
  }
  return false;
}

// The boxes of the Bezier pieces of the edge's curve between its ends (a
// curve lies within the hull of its poles), grown by the edge's
// tolerance; none for other curves or where the pieces cannot be made.
// `hulls` gets the pieces' poles (none where a weight is not positive).
std::vector<Bnd_Box> edge_pieces(const TopoDS_Edge& edge, std::vector<Hull>* hulls = nullptr) {
  std::vector<Bnd_Box> pieces;
  try {
    OCC_CATCH_SIGNALS
    if (BRep_Tool::Degenerated(edge)) {
      return pieces;
    }
    TopLoc_Location location;
    double first = 0.0, last = 0.0;
    occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, location, first, last);
    while (const occ::handle<Geom_TrimmedCurve> trimmed = occ::down_cast<Geom_TrimmedCurve>(curve)) {
      curve = trimmed->BasisCurve();
    }
    const double tolerance = BRep_Tool::Tolerance(edge);
    if (const occ::handle<Geom_BSplineCurve> spline = occ::down_cast<Geom_BSplineCurve>(curve)) {
      if (!(first < last) || first < spline->FirstParameter() || last > spline->LastParameter()) {
        return pieces;
      }
      GeomConvert_BSplineCurveToBezierCurve arcs(spline, first, last, Precision::PConfusion());
      for (int i = 1; i <= arcs.NbArcs(); ++i) {
        pieces.push_back(poles_box(arcs.Arc(i)->Poles(), location, tolerance));
        if (hulls != nullptr) {
          hulls->push_back(hull_of(arcs.Arc(i), location));
        }
      }
    } else if (const occ::handle<Geom_BezierCurve> bezier = occ::down_cast<Geom_BezierCurve>(curve)) {
      pieces.push_back(poles_box(bezier->Poles(), location, tolerance));
      if (hulls != nullptr) {
        hulls->push_back(hull_of(bezier, location));
      }
    } else if (hulls != nullptr && first < last &&
               (curve->IsKind(STANDARD_TYPE(Geom_Line)) || curve->IsKind(STANDARD_TYPE(Geom_Circle)) ||
                curve->IsKind(STANDARD_TYPE(Geom_Ellipse)))) {
      // A line's, circle's or ellipse's part as the pieces of its exact
      // (rational) B-spline form, for the hulls only: the edge's boxes stay
      // as they were.
      const occ::handle<Geom_BSplineCurve> exact =
          GeomConvert::CurveToBSplineCurve(new Geom_TrimmedCurve(curve, first, last));
      if (!exact.IsNull()) {
        GeomConvert_BSplineCurveToBezierCurve arcs(exact);
        for (int i = 1; i <= arcs.NbArcs(); ++i) {
          hulls->push_back(hull_of(arcs.Arc(i), location));
        }
      }
    }
    // (A piece without a hull: the edge is measured as before.)
    if (hulls != nullptr &&
        std::any_of(hulls->begin(), hulls->end(), [](const Hull& hull) { return hull.empty(); })) {
      hulls->clear();
    }
  } catch (const Standard_Failure&) {
    pieces.clear();
    if (hulls != nullptr) {
      hulls->clear();
    }
  }
  return pieces;
}

// The same for a face's surface within the face's parameter bounds, grown
// by the face's tolerance.
std::vector<Bnd_Box> face_pieces(const TopoDS_Face& face) {
  std::vector<Bnd_Box> pieces;
  try {
    OCC_CATCH_SIGNALS
    TopLoc_Location location;
    occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face, location);
    while (const occ::handle<Geom_RectangularTrimmedSurface> trimmed =
               occ::down_cast<Geom_RectangularTrimmedSurface>(surface)) {
      surface = trimmed->BasisSurface();
    }
    const double tolerance = BRep_Tool::Tolerance(face);
    if (const occ::handle<Geom_BSplineSurface> spline = occ::down_cast<Geom_BSplineSurface>(surface)) {
      double u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0;
      BRepTools::UVBounds(face, u0, u1, v0, v1);
      double su0 = 0.0, su1 = 0.0, sv0 = 0.0, sv1 = 0.0;
      spline->Bounds(su0, su1, sv0, sv1);
      if (!(u0 < u1 && v0 < v1) || u0 < su0 || u1 > su1 || v0 < sv0 || v1 > sv1) {
        return pieces;
      }
      GeomConvert_BSplineSurfaceToBezierSurface patches(spline, u0, u1, v0, v1, Precision::PConfusion());
      for (int i = 1; i <= patches.NbUPatches(); ++i) {
        for (int j = 1; j <= patches.NbVPatches(); ++j) {
          pieces.push_back(poles_box(patches.Patch(i, j)->Poles(), location, tolerance));
        }
      }
    } else if (const occ::handle<Geom_BezierSurface> bezier = occ::down_cast<Geom_BezierSurface>(surface)) {
      pieces.push_back(poles_box(bezier->Poles(), location, tolerance));
    }
  } catch (const Standard_Failure&) {
    pieces.clear();
  }
  return pieces;
}

// Whether two parts' boxes come within `limit` (+ the measures'
// precision) of each other: by their pieces' boxes where they have pieces.
bool boxes_near(const Bnd_Box& box1, const std::vector<Bnd_Box>* pieces1, const Bnd_Box& box2,
                const std::vector<Bnd_Box>* pieces2, double limit) {
  const auto within_limit = [limit](const Bnd_Box& a, const Bnd_Box& b) {
    return a.Distance(b) - limit < Precision::Confusion();
  };
  if (!within_limit(box1, box2)) {
    return false;
  }
  const bool split1 = pieces1 != nullptr && !pieces1->empty();
  const bool split2 = pieces2 != nullptr && !pieces2->empty();
  if (!split1 && !split2) {
    return true;
  }
  const std::vector<Bnd_Box> whole1{box1};
  const std::vector<Bnd_Box> whole2{box2};
  for (const Bnd_Box& a : split1 ? *pieces1 : whole1) {
    if (!within_limit(a, box2)) {
      continue;
    }
    for (const Bnd_Box& b : split2 ? *pieces2 : whole2) {
      if (within_limit(a, b)) {
        return true;
      }
    }
  }
  return false;
}

} // namespace

Proximity::Proximity(const TopoDS_Shape& of)
    : shape_(of), solid_(of.ShapeType() == TopAbs_SOLID || of.ShapeType() == TopAbs_COMPSOLID) {
  // (As BRepExtrema_DistShapeShape decomposes a shape and boxes its parts.)
  const auto take = [&of](TopAbs_ShapeEnum type, Map& map, std::vector<Bnd_Box>& boxes) {
    TopExp::MapShapes(of, type, map);
    boxes.resize(static_cast<std::size_t>(map.Extent()));
    for (int i = 1; i <= map.Extent(); ++i) {
      BRepBndLib::Add(map.FindKey(i), boxes[static_cast<std::size_t>(i - 1)]);
    }
  };
  take(TopAbs_VERTEX, vertices_, vertex_boxes_);
  take(TopAbs_EDGE, edges_, edge_boxes_);
  take(TopAbs_FACE, faces_, face_boxes_);
  for (int i = 1; i <= edges_.Extent(); ++i) {
    const TopoDS_Edge& edge = TopoDS::Edge(edges_.FindKey(i));
    edge_hulls_.emplace_back();
    edge_pieces_.push_back(edge_pieces(edge, &edge_hulls_.back()));
    edge_tolerances_.push_back(BRep_Tool::Tolerance(edge));
  }
  for (int i = 1; i <= faces_.Extent(); ++i) {
    face_pieces_.push_back(face_pieces(TopoDS::Face(faces_.FindKey(i))));
  }
  if (solid_) {
    BRepBndLib::Add(of, bounds_);
    bounds_.Enlarge(kInsideTolerance);
  }
}

Proximity::~Proximity() = default;

bool Proximity::holds_vertex_of(const Proximity& points) const {
  if (!solid_) {
    return false;
  }
  for (int i = 1; i <= points.vertices_.Extent(); ++i) {
    const TopoDS_Shape& vertex = points.vertices_.FindKey(i);
    const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(vertex));
    // (Outside the solid's box it is outside the solid.)
    if (bounds_.IsOut(p)) {
      continue;
    }
    if (const bool* known = inside_.Seek(vertex)) {
      if (*known) {
        return true;
      }
      continue;
    }
    if (!classifier_) {
      classifier_ = std::make_unique<BRepClass3d_SolidClassifier>(shape_);
    }
    classifier_->Perform(p, kInsideTolerance);
    const bool in = classifier_->State() == TopAbs_IN;
    inside_.Bind(vertex, in);
    if (in) {
      return true;
    }
  }
  return false;
}

bool within_distance(const Proximity& a, const Proximity& b, double limit) {
  for (int i = 1; i <= a.vertices_.Extent(); ++i) {
    const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(a.vertices_.FindKey(i)));
    for (int j = 1; j <= b.vertices_.Extent(); ++j) {
      if (p.Distance(BRep_Tool::Pnt(TopoDS::Vertex(b.vertices_.FindKey(j)))) <= limit) {
        return true;
      }
    }
  }
  if (a.holds_vertex_of(b) || b.holds_vertex_of(a)) {
    return true;
  }
  // The pairs in BRepExtrema_DistShapeShape's order, those whose boxes are
  // within the limit (+ its precision) of each other measured. (Its boxes
  // where there are no pieces; a pair whose pieces' boxes are farther
  // apart is farther apart, since the pieces' boxes hold the geometry.)
  using Pieces = std::vector<std::vector<Bnd_Box>>;
  // (Two edges whose pieces' boxes come near, told apart by the boxes of
  // their halves without measuring them, mitcad#1796: the helices of a
  // modelled thread's faces against the edges of a region beside them,
  // tens of milliseconds a pair measured.)
  const auto pairs = [&a, &b, limit](const Proximity::Map& one, const std::vector<Bnd_Box>& one_boxes,
                                     const Pieces* one_pieces, const Proximity::Map& two,
                                     const std::vector<Bnd_Box>& two_boxes, const Pieces* two_pieces) {
    const bool both_edges = &one == &a.edges_ && &two == &b.edges_;
    for (int i = 1; i <= one.Extent(); ++i) {
      const auto at1 = static_cast<std::size_t>(i - 1);
      const Bnd_Box& box1 = one_boxes[at1];
      if (box1.IsVoid()) {
        continue;
      }
      for (int j = 1; j <= two.Extent(); ++j) {
        const auto at2 = static_cast<std::size_t>(j - 1);
        const Bnd_Box& box2 = two_boxes[at2];
        if (box2.IsVoid() || !boxes_near(box1, one_pieces != nullptr ? &(*one_pieces)[at1] : nullptr, box2,
                                         two_pieces != nullptr ? &(*two_pieces)[at2] : nullptr, limit)) {
          continue;
        }
        if (both_edges && !edges_may_meet(a.edge_hulls_[at1], a.edge_tolerances_[at1], b.edge_hulls_[at2],
                                          b.edge_tolerances_[at2], limit)) {
          continue;
        }
        const BRepExtrema_DistanceSS measured(one.FindKey(i), two.FindKey(j), box1, box2, limit);
        if (!measured.IsDone()) {
          continue;
        }
        const NCollection_Sequence<BRepExtrema_SolutionElem>& solutions = measured.Seq1Value();
        for (int k = 1; k <= solutions.Length(); ++k) {
          if (solutions.Value(k).Dist() <= limit) {
            return true;
          }
        }
      }
    }
    return false;
  };
  using Part = std::tuple<const Proximity::Map*, const std::vector<Bnd_Box>*, const Pieces*>;
  const Part a_vertices{&a.vertices_, &a.vertex_boxes_, nullptr};
  const Part a_edges{&a.edges_, &a.edge_boxes_, &a.edge_pieces_};
  const Part a_faces{&a.faces_, &a.face_boxes_, &a.face_pieces_};
  const Part b_vertices{&b.vertices_, &b.vertex_boxes_, nullptr};
  const Part b_edges{&b.edges_, &b.edge_boxes_, &b.edge_pieces_};
  const Part b_faces{&b.faces_, &b.face_boxes_, &b.face_pieces_};
  const auto measure = [&pairs](const Part& one, const Part& two) {
    return pairs(*std::get<0>(one), *std::get<1>(one), std::get<2>(one), *std::get<0>(two), *std::get<1>(two),
                 std::get<2>(two));
  };
  return measure(a_vertices, b_edges) || measure(a_edges, b_vertices) || measure(a_vertices, b_faces) ||
         measure(a_faces, b_vertices) || measure(a_edges, b_edges) || measure(a_edges, b_faces) ||
         measure(a_faces, b_edges) || measure(a_faces, b_faces);
}

bool within_distance(const TopoDS_Shape& a, const TopoDS_Shape& b, double limit) {
  return within_distance(Proximity(a), Proximity(b), limit);
}

SurfaceProbe::SurfaceProbe(const occ::handle<Geom_Surface>& surface) : surface_(surface) {
  // (A B-spline or Bezier surface lies within the hull of its poles.)
  if (const occ::handle<Geom_BSplineSurface> spline = occ::down_cast<Geom_BSplineSurface>(surface)) {
    for (int i = 1; i <= spline->NbUPoles(); ++i) {
      for (int j = 1; j <= spline->NbVPoles(); ++j) {
        poles_.Add(spline->Pole(i, j));
      }
    }
  } else if (const occ::handle<Geom_BezierSurface> bezier = occ::down_cast<Geom_BezierSurface>(surface)) {
    for (int i = 1; i <= bezier->NbUPoles(); ++i) {
      for (int j = 1; j <= bezier->NbVPoles(); ++j) {
        poles_.Add(bezier->Pole(i, j));
      }
    }
  }
}

SurfaceProbe::SurfaceProbe(SurfaceProbe&&) noexcept = default;
SurfaceProbe& SurfaceProbe::operator=(SurfaceProbe&&) noexcept = default;
SurfaceProbe::~SurfaceProbe() = default;

bool SurfaceProbe::holds(const std::vector<gp_Pnt>& points, double tolerance) {
  if (points.empty() || surface_.IsNull()) {
    return false;
  }
  Bnd_Box hull = poles_;
  if (!hull.IsVoid()) {
    hull.Enlarge(tolerance);
  }
  for (const gp_Pnt& p : points) {
    if (!hull.IsVoid() && hull.IsOut(p)) {
      return false;
    }
    if (!projection_) {
      // (As GeomAPI_ProjectPointOnSurf(p, surface) sets itself up.)
      double u0 = 0.0, u1 = 0.0, v0 = 0.0, v1 = 0.0;
      surface_->Bounds(u0, u1, v0, v1);
      projection_ = std::make_unique<GeomAPI_ProjectPointOnSurf>();
      projection_->Init(surface_, u0, u1, v0, v1, Precision::Confusion());
    }
    projection_->Perform(p);
    if (projection_->NbPoints() == 0 || projection_->LowerDistance() > tolerance) {
      return false;
    }
  }
  return true;
}

namespace {

// The outward normal of a face at its parameters, none where its surface
// is singular.
std::optional<gp_Dir> outward_normal_at(const TopoDS_Face& face, double u, double v) {
  const BRepAdaptor_Surface surface(face, false);
  gp_Pnt point;
  gp_Vec du;
  gp_Vec dv;
  surface.D1(u, v, point, du, dv);
  const gp_Vec normal = du.Crossed(dv);
  if (normal.Magnitude() <= gp::Resolution()) {
    return std::nullopt;
  }
  const gp_Dir direction(normal);
  return face.Orientation() == TopAbs_REVERSED ? direction.Reversed() : direction;
}

// The directions of skew_ray_side's rays, skew to the axes and to each
// other.
constexpr std::array<std::array<double, 3>, 6> kSkew{{{0.267, 0.535, 0.802},
                                                      {-0.802, 0.267, 0.535},
                                                      {0.535, -0.802, 0.267},
                                                      {-0.371, -0.557, 0.743},
                                                      {0.743, -0.371, -0.557},
                                                      {-0.557, 0.743, -0.371}}};

} // namespace

std::optional<RaySide> ray_side(IntCurvesFace_ShapeIntersector& boundary, const gp_Pnt& point,
                                const gp_Dir& direction, double close, double reach) {
  boundary.Perform(gp_Lin(point, direction), -close, reach);
  if (!boundary.IsDone() || boundary.NbPnt() == 0) {
    return std::nullopt;
  }
  int first = 1;
  for (int i = 2; i <= boundary.NbPnt(); ++i) {
    if (boundary.WParameter(i) < boundary.WParameter(first)) {
      first = i;
    }
  }
  const double along_ray = boundary.WParameter(first);
  if (along_ray <= close) {
    return RaySide::Near;
  }
  // (On an edge or a vertex, two faces' crossings at once.)
  if (boundary.State(first) != TopAbs_IN) {
    return std::nullopt;
  }
  for (int i = 1; i <= boundary.NbPnt(); ++i) {
    if (i != first && boundary.WParameter(i) - along_ray <= Precision::Confusion()) {
      return std::nullopt;
    }
  }
  const std::optional<gp_Dir> normal =
      outward_normal_at(boundary.Face(first), boundary.UParameter(first), boundary.VParameter(first));
  if (!normal) {
    return std::nullopt;
  }
  const double cosine = normal->Dot(direction);
  // (Grazing: too near a tangent to tell.)
  if (std::abs(cosine) < kRayTransverse) {
    return std::nullopt;
  }
  return cosine > 0.0 ? RaySide::Inside : RaySide::Outside;
}

std::optional<RaySide> skew_ray_side(IntCurvesFace_ShapeIntersector& boundary, const gp_Pnt& point,
                                     double tolerance, double through) {
  if (!(through > 0.0)) {
    return std::nullopt;
  }
  // (A face within `tolerance` that a ray crosses at a cosine of at least
  // kRayTransverse it crosses within this of the point.)
  const double close = tolerance / kRayTransverse;
  for (const std::array<double, 3>& d : kSkew) {
    const std::optional<RaySide> told = ray_side(boundary, point, gp_Dir(d[0], d[1], d[2]), close, through);
    if (told) {
      return told;
    }
  }
  return std::nullopt;
}

double length_through(const TopoDS_Shape& shape) {
  Bnd_Box box;
  BRepBndLib::Add(shape, box);
  return box.IsVoid() ? 0.0 : 2.0 * std::sqrt(box.SquareExtent()) + 1.0;
}

PointSide::PointSide(const TopoDS_Shape& body) : body_(body) {}

PointSide::~PointSide() = default;

TopAbs_State PointSide::state(const gp_Pnt& point, double tolerance) {
  if (!rays_) {
    rays_ = std::make_unique<IntCurvesFace_ShapeIntersector>();
    rays_->Load(body_, Precision::Confusion());
    through_ = length_through(body_);
  }
  const std::optional<RaySide> told = skew_ray_side(*rays_, point, tolerance, through_);
  if (told && *told != RaySide::Near) {
    return *told == RaySide::Inside ? TopAbs_IN : TopAbs_OUT;
  }
  if (!classifier_) {
    classifier_ = std::make_unique<BRepClass3d_SolidClassifier>(body_);
  }
  classifier_->Perform(point, tolerance);
  return classifier_->State();
}

} // namespace mitcad::geometry::detail
