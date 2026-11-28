// SPDX-License-Identifier: MIT
#include "mitcad/analysis/compare.hpp"

#include <algorithm>
#include <chrono>
#include <cmath>
#include <limits>
#include <memory>
#include <optional>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepExtrema_ExtPC.hxx>
#include <BRepExtrema_ExtPF.hxx>
#include <BRepTools.hxx>
#include <BRepTopAdaptor_FClass2d.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <Extrema_ExtAlgo.hxx>
#include <Extrema_ExtFlag.hxx>
#include <GProp_GProps.hxx>
#include <Message_ProgressIndicator.hxx>
#include <Message_ProgressScope.hxx>
#include <NCollection_IndexedMap.hxx>
#include <Precision.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <NCollection_List.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Pnt2d.hxx>

#include "convert.hpp"

namespace mitcad::analysis {

namespace {

using Clock = std::chrono::steady_clock;

// When the comparison is to stop; none without a limit.
using Deadline = std::optional<Clock::time_point>;

bool over(const Deadline& deadline) { return deadline && Clock::now() >= *deadline; }

// A progress indicator that shows nothing and asks OCCT's algorithms to
// stop at the deadline.
class Timer final : public Message_ProgressIndicator {
public:
  explicit Timer(Clock::time_point deadline) : m_deadline(deadline) {}

protected:
  bool UserBreak() override { return Clock::now() >= m_deadline; }
  void Show(const Message_ProgressScope&, const bool) override {}

private:
  Clock::time_point m_deadline;
};

bool has_solid(const TopoDS_Shape& shape) { return TopExp_Explorer(shape, TopAbs_SOLID).More(); }

double solid_volume(const TopoDS_Shape& shape) {
  return std::abs(volume_properties(shape, true).Mass());
}

// Volume of a - b, or nothing when the boolean operation fails or does
// not finish by the deadline.
std::optional<double> difference(const TopoDS_Shape& a, const TopoDS_Shape& b, double fuzzy,
                                 const Deadline& deadline) {
  if (over(deadline)) {
    return std::nullopt;
  }
  if (!has_solid(a)) {
    return 0.0;
  }
  if (!has_solid(b)) {
    return solid_volume(a);
  }
  BRepAlgoAPI_Cut cut;
  NCollection_List<TopoDS_Shape> object;
  NCollection_List<TopoDS_Shape> tool;
  object.Append(a);
  tool.Append(b);
  cut.SetArguments(object);
  cut.SetTools(tool);
  // The shapes are the model's bodies, which other results share: a
  // destructive boolean gives their edges p-curves and tolerances.
  cut.SetNonDestructive(true);
  if (fuzzy > 0.0) {
    cut.SetFuzzyValue(fuzzy);
  }
  if (deadline) {
    const Handle(Message_ProgressIndicator) timer = new Timer(*deadline);
    cut.Build(timer->Start());
  } else {
    cut.Build();
  }
  if (!cut.IsDone() || cut.HasErrors()) {
    return std::nullopt;
  }
  return solid_volume(cut.Shape());
}

// The faces of a shape without its solids, so that distances are measured
// to the surface also from points inside.
TopoDS_Shape surface_of(const TopoDS_Shape& shape) {
  BRep_Builder builder;
  TopoDS_Compound faces;
  builder.MakeCompound(faces);
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    builder.Add(faces, it.Current());
  }
  return faces;
}

// Points on the faces of a shape: a grid in each face's parameter range
// with about `target` points over the whole area, plus the vertices and the
// midpoints of the edges.
std::vector<gp_Pnt> sample(const TopoDS_Shape& shape, std::size_t target, double* area = nullptr) {
  std::vector<std::pair<TopoDS_Face, double>> faces;
  double total = 0.0;
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    const GProp_GProps props = surface_properties(it.Current());
    faces.emplace_back(TopoDS::Face(it.Current()), props.Mass());
    total += props.Mass();
  }
  if (area != nullptr) {
    *area = total;
  }
  std::vector<gp_Pnt> points;
  for (const auto& [face, face_area] : faces) {
    TopLoc_Location location;
    if (BRep_Tool::Surface(face, location).IsNull()) {
      continue;
    }
    const double share = total > 0.0 ? static_cast<double>(target) * face_area / total : 0.0;
    const auto count = std::max<std::size_t>(4, static_cast<std::size_t>(std::lround(share)));
    // An odd grid samples the middle of the parameter range, where a
    // fillet or a bulge deviates most.
    const auto side = static_cast<int>(std::ceil(std::sqrt(static_cast<double>(count)))) | 1;
    double u0 = 0.0;
    double u1 = 0.0;
    double v0 = 0.0;
    double v1 = 0.0;
    BRepTools::UVBounds(face, u0, u1, v0, v1);
    const BRepAdaptor_Surface surface(face);
    const BRepTopAdaptor_FClass2d classifier(face, Precision::PConfusion());
    for (int i = 0; i < side; ++i) {
      for (int j = 0; j < side; ++j) {
        const double u = u0 + (u1 - u0) * (i + 0.5) / side;
        const double v = v0 + (v1 - v0) * (j + 0.5) / side;
        if (classifier.Perform(gp_Pnt2d(u, v)) == TopAbs_IN) {
          points.push_back(surface.Value(u, v));
        }
      }
    }
  }
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> vertices;
  TopExp::MapShapes(shape, TopAbs_VERTEX, vertices);
  for (int i = 1; i <= vertices.Extent(); ++i) {
    points.push_back(BRep_Tool::Pnt(TopoDS::Vertex(vertices(i))));
  }
  NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
  TopExp::MapShapes(shape, TopAbs_EDGE, edges);
  for (int i = 1; i <= edges.Extent(); ++i) {
    const TopoDS_Edge& edge = TopoDS::Edge(edges(i));
    if (BRep_Tool::Degenerated(edge) || !BRep_Tool::IsGeometric(edge)) {
      continue;
    }
    const BRepAdaptor_Curve curve(edge);
    points.push_back(curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter())));
  }
  return points;
}

// The squared distance of a point from a box (0 inside it, and for a void
// box, which bounds nothing it could leave out).
double square_distance(const Bnd_Box& box, const gp_Pnt& p) {
  if (box.IsVoid()) {
    return 0.0;
  }
  double x0 = 0.0;
  double y0 = 0.0;
  double z0 = 0.0;
  double x1 = 0.0;
  double y1 = 0.0;
  double z1 = 0.0;
  box.Get(x0, y0, z0, x1, y1, z1);
  const double dx = std::max({x0 - p.X(), 0.0, p.X() - x1});
  const double dy = std::max({y0 - p.Y(), 0.0, p.Y() - y1});
  const double dz = std::max({z0 - p.Z(), 0.0, p.Z() - z1});
  return dx * dx + dy * dy + dz * dz;
}

// Distances of points from the faces of a shape, their edges and their
// vertices: the nearest of the points on a face where the distance has a
// minimum inside it, the nearest points on the edges and the vertices, as
// BRepExtrema_DistShapeShape measures them from a vertex. That one sets up
// each face's projection again for every point, and for a B-spline face
// the projection first evaluates a grid of the surface's points over all
// its spans: a side lofted along rails has 320 spans of degree 3 along the
// loft and 12 of degree 9 around it, some 100 000 points, and a comparison
// of such a body with its mirror image took two minutes, all of it in
// these grids (mitcad#2595). Here each face's and edge's projection is
// set up once, its grid built at its first point and kept for the others,
// and as BRepExtrema_DistShapeShape does, a face or edge whose bounding box
// lies farther than the nearest distance found so far is left out.
class SurfaceDistance {
public:
  explicit SurfaceDistance(const TopoDS_Shape& shape) {
    NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> vertices;
    TopExp::MapShapes(shape, TopAbs_VERTEX, vertices);
    for (int i = 1; i <= vertices.Extent(); ++i) {
      m_vertices.push_back(BRep_Tool::Pnt(TopoDS::Vertex(vertices(i))));
    }
    NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> edges;
    TopExp::MapShapes(shape, TopAbs_EDGE, edges);
    for (int i = 1; i <= edges.Extent(); ++i) {
      const TopoDS_Edge& edge = TopoDS::Edge(edges(i));
      if (BRep_Tool::Degenerated(edge) || !BRep_Tool::IsGeometric(edge)) {
        continue;
      }
      Edge item;
      BRepBndLib::Add(edge, item.box, false);
      // The projection keeps the edge's curve; it does not move once set up.
      item.projection = std::make_unique<BRepExtrema_ExtPC>();
      item.projection->Initialize(edge);
      m_edges.push_back(std::move(item));
    }
    NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> faces;
    TopExp::MapShapes(shape, TopAbs_FACE, faces);
    for (int i = 1; i <= faces.Extent(); ++i) {
      Face item;
      item.face = TopoDS::Face(faces(i));
      BRepBndLib::Add(item.face, item.box, false);
      // The nearest point inside a face is a minimum of the distance (or on
      // its boundary, which the edges give): the maxima are left out. The
      // projection points to its own surface adaptor, so it stays put.
      item.projection = std::make_unique<BRepExtrema_ExtPF>();
      item.projection->Initialize(item.face, Extrema_ExtFlag_MIN, Extrema_ExtAlgo_Grad);
      m_faces.push_back(std::move(item));
    }
  }

  // The distance of `p` from the shape; nothing when no vertex, edge or
  // face gave one.
  std::optional<double> operator()(const gp_Pnt& p) {
    double best = std::numeric_limits<double>::infinity();
    for (const gp_Pnt& vertex : m_vertices) {
      best = std::min(best, p.SquareDistance(vertex));
    }
    const TopoDS_Vertex at = BRepBuilderAPI_MakeVertex(p).Vertex();
    // The faces first, nearest box first: a point on or near a face finds
    // its distance there, and the boxes of the others leave them out.
    for (const std::size_t k : nearest_first(m_faces, p)) {
      Face& item = m_faces[k];
      if (square_distance(item.box, p) > best) {
        break;
      }
      item.projection->Perform(at, item.face);
      if (!item.projection->IsDone()) {
        continue;
      }
      // The extrema the projection keeps are inside the face or on it.
      for (int i = 1; i <= item.projection->NbExt(); ++i) {
        best = std::min(best, item.projection->SquareDistance(i));
      }
    }
    for (const std::size_t k : nearest_first(m_edges, p)) {
      Edge& item = m_edges[k];
      if (square_distance(item.box, p) > best) {
        break;
      }
      item.projection->Perform(at);
      if (!item.projection->IsDone()) {
        continue;
      }
      for (int i = 1; i <= item.projection->NbExt(); ++i) {
        best = std::min(best, item.projection->SquareDistance(i));
      }
    }
    if (!std::isfinite(best)) {
      return std::nullopt;
    }
    return std::sqrt(best);
  }

private:
  struct Face {
    TopoDS_Face face;
    Bnd_Box box;
    std::unique_ptr<BRepExtrema_ExtPF> projection;
  };
  struct Edge {
    Bnd_Box box;
    std::unique_ptr<BRepExtrema_ExtPC> projection;
  };

  // The items' indices by the distance of their boxes from `p`.
  template <class Item>
  static std::vector<std::size_t> nearest_first(const std::vector<Item>& items, const gp_Pnt& p) {
    std::vector<std::pair<double, std::size_t>> order;
    order.reserve(items.size());
    for (std::size_t k = 0; k < items.size(); ++k) {
      order.emplace_back(square_distance(items[k].box, p), k);
    }
    std::sort(order.begin(), order.end());
    std::vector<std::size_t> indices;
    indices.reserve(order.size());
    for (const auto& entry : order) {
      indices.push_back(entry.second);
    }
    return indices;
  }

  std::vector<gp_Pnt> m_vertices;
  std::vector<Edge> m_edges;
  std::vector<Face> m_faces;
};

Deviation deviation(const std::vector<gp_Pnt>& points, const TopoDS_Shape& surface,
                    const Deadline& deadline) {
  Deviation result;
  if (points.empty() || !TopExp_Explorer(surface, TopAbs_FACE).More()) {
    return result;
  }
  SurfaceDistance distance(surface);
  double sum = 0.0;
  for (const gp_Pnt& p : points) {
    if (over(deadline)) {
      break;
    }
    const std::optional<double> found = distance(p);
    if (!found) {
      continue;
    }
    const double d = *found;
    sum += d * d;
    ++result.samples;
    if (d > result.max || result.samples == 1) {
      result.max = d;
      result.at = detail::vec(p);
    }
  }
  if (result.samples > 0) {
    result.rms = std::sqrt(sum / static_cast<double>(result.samples));
  }
  return result;
}

} // namespace

std::vector<std::optional<double>> surface_distances(const std::vector<gp_Pnt>& points,
                                                     const TopoDS_Shape& shape) {
  std::vector<std::optional<double>> result;
  result.reserve(points.size());
  SurfaceDistance distance(surface_of(shape));
  for (const gp_Pnt& p : points) {
    result.push_back(distance(p));
  }
  return result;
}

bool plausible_differences(double differences, double max_deviation, double area, double fuzzy) {
  return differences <= 4.0 * (max_deviation + fuzzy) * area;
}

Comparison compare(const TopoDS_Shape& a, const TopoDS_Shape& b, const CompareOptions& options) {
  if (a.IsNull() || b.IsNull()) {
    throw Error("cannot compare a null shape");
  }
  Comparison result;
  Deadline deadline;
  if (options.seconds > 0.0) {
    deadline = Clock::now() + std::chrono::duration_cast<Clock::duration>(
                                  std::chrono::duration<double>(options.seconds));
  }
  // The deviations first: with a time limit, the booleans (which may not
  // finish) get what is left.
  double area_a = 0.0;
  double area_b = 0.0;
  const std::vector<gp_Pnt> points_a = sample(a, options.samples, &area_a);
  result.a_to_b = deviation(points_a, surface_of(b), deadline);
  // Whether every point was measured both ways.
  bool sampled = !points_a.empty() && result.a_to_b.samples == points_a.size();
  if (options.symmetric) {
    const std::vector<gp_Pnt> points_b = sample(b, options.samples, &area_b);
    result.b_to_a = deviation(points_b, surface_of(a), deadline);
    sampled = sampled && !points_b.empty() && result.b_to_a.samples == points_b.size();
  } else {
    sampled = false;
  }
  result.max_deviation = std::max(result.a_to_b.max, result.b_to_a.max);

  const TopoDS_Shape solids_a = oriented_solids(a);
  const TopoDS_Shape solids_b = oriented_solids(b);
  result.volume_a = solid_volume(solids_a);
  result.volume_b = solid_volume(solids_b);
  if (!(sampled && options.booleans_above > 0.0 && result.max_deviation <= options.booleans_above)) {
    result.a_minus_b = difference(solids_a, solids_b, options.fuzzy, deadline);
    result.b_minus_a = difference(solids_b, solids_a, options.fuzzy, deadline);
  }
  // With every point measured, differences the deviations rule out are a
  // boolean that took the shapes as apart: unknown.
  if (sampled && result.a_minus_b && result.b_minus_a &&
      !plausible_differences(*result.a_minus_b + *result.b_minus_a, result.max_deviation, area_a + area_b,
                             options.fuzzy)) {
    result.a_minus_b.reset();
    result.b_minus_a.reset();
  }
  if (result.a_minus_b && result.b_minus_a) {
    const double larger = std::max(result.volume_a, result.volume_b);
    result.relative_difference = larger > 0.0 ? (*result.a_minus_b + *result.b_minus_a) / larger : 0.0;
  }

  result.bounds_a = bounds(a);
  result.bounds_b = bounds(b);
  if (!result.bounds_a.empty && !result.bounds_b.empty) {
    const Bounds& p = result.bounds_a;
    const Bounds& q = result.bounds_b;
    result.bounds_difference =
        std::max({std::abs(p.min.x - q.min.x), std::abs(p.min.y - q.min.y),
                  std::abs(p.min.z - q.min.z), std::abs(p.max.x - q.max.x),
                  std::abs(p.max.y - q.max.y), std::abs(p.max.z - q.max.z)});
  }
  return result;
}

} // namespace mitcad::analysis
