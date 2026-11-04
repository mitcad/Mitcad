// SPDX-License-Identifier: MIT
#include "bridge/analysis.hpp"

#include <algorithm>
#include <array>
#include <atomic>
#include <cmath>
#include <cstdio>
#include <limits>
#include <map>
#include <memory>
#include <mutex>
#include <optional>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <Extrema_ExtPC.hxx>
#include <Standard_Failure.hxx>
#include <gp.hxx>
#include <BRepTools.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <GeomAdaptor_Surface.hxx>
#include <Extrema_GenLocateExtPS.hxx>
#include <Extrema_POnSurf.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepClass_FClassifier.hxx>
#include <BRepClass_FaceExplorer.hxx>
#include <BRepGProp_Face.hxx>
#include <IntCurvesFace_Intersector.hxx>
#include <gp_Lin.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRep_Builder.hxx>
#include <Poly_Triangulation.hxx>
#include <Bnd_Box.hxx>
#include <NCollection_IndexedMap.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <BRepTopAdaptor_FClass2d.hxx>
#include <BRep_Tool.hxx>
#include <ShapeAnalysis_Surface.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Pnt2d.hxx>
#include <OSD_Parallel.hxx>
// OSD_Parallel.hxx brings in windows.h on Windows, whose empty near and far
// macros would remove those names below.
#ifdef near
#undef near
#endif
#ifdef far
#undef far
#endif

#include "mitcad/analysis/compare.hpp"
#include "mitcad/analysis/measure.hpp"
#include "mitcad/analysis/properties.hpp"
#include "mitcad/geometry/cancel.hpp"
#include "mitcad/geometry/guard.hpp"
#include "mitcad/geometry/query.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad/io/step.hpp"
#include "mitcad_bridge/kernel/analysis.h"

namespace mitcad::bridge {
namespace {

// A point this near a face is on the boundary (boundary_distances).
constexpr double kOnBoundary = 1.0e-6;

// A point this near a face has its distance to that face, though another
// may be nearer still (boundary_distances): the import tells points on
// the boundary from others by 1e-4 mm and more.
constexpr double kNearBoundary = 1.0e-5;

std::array<double, 3> xyz(const analysis::Vec3& v) { return {v.x, v.y, v.z}; }

analysis::Vec3 vec(const std::array<double, 3>& v) { return {v[0], v[1], v[2]}; }

TopoDS_Shape compound(const std::vector<TopoDS_Shape>& shapes) {
  if (shapes.size() == 1) {
    return shapes.front();
  }
  BRep_Builder builder;
  TopoDS_Compound result;
  builder.MakeCompound(result);
  for (const TopoDS_Shape& shape : shapes) {
    builder.Add(result, shape);
  }
  return result;
}

// What a selection resolves to: one sub-shape, or a compound of all the
// pieces a name means.
struct Selected {
  TopoDS_Shape shape;
  std::string kind;
  std::size_t count = 1;
};

const char* kind_of(const TopoDS_Shape& shape) {
  switch (shape.ShapeType()) {
  case TopAbs_FACE:
    return "face";
  case TopAbs_EDGE:
    return "edge";
  case TopAbs_VERTEX:
    return "vertex";
  default:
    return "body";
  }
}

Selected select(const geometry::Shape& shape, const std::string& name) {
  if (shape.occt().IsNull()) {
    throw std::runtime_error("the body has no shape");
  }
  if (name.empty()) {
    return {shape.occt(), kind_of(shape.occt()), 1};
  }
  Selected result;
  std::vector<TopoDS_Shape> found;
  if (name.rfind("E{", 0) == 0) {
    result.kind = "edge";
    for (const int i : shape.find_edges(name)) {
      found.push_back(shape.edge(i));
    }
  } else if (name.rfind("V{", 0) == 0) {
    result.kind = "vertex";
    for (const int i : shape.find_vertices(name)) {
      found.push_back(shape.vertex(i));
    }
  } else {
    result.kind = "face";
    for (const int i : shape.find_faces(name)) {
      found.push_back(shape.face(i));
    }
  }
  if (found.empty()) {
    throw std::runtime_error("the body has no " + result.kind + " " + name);
  }
  result.count = found.size();
  result.shape = compound(found);
  return result;
}

analysis::Plane plane_of(const AnalysisPlane& plane) {
  analysis::Plane result;
  result.origin = vec(plane.origin);
  result.normal = vec(plane.normal);
  return result;
}

// The list's shapes (their measures kept with each, mitcad#658).
std::vector<const geometry::Shape*> shapes_of(const ShapeList& list) {
  std::vector<const geometry::Shape*> shapes;
  for (const auto& shape : list.items()) {
    if (!shape || shape->occt().IsNull()) {
      throw std::runtime_error("a body has no shape");
    }
    shapes.push_back(shape.get());
  }
  return shapes;
}

std::vector<TopoDS_Shape> occt(const ShapeList& list) {
  std::vector<TopoDS_Shape> shapes;
  for (const auto& shape : list.items()) {
    if (!shape || shape->occt().IsNull()) {
      throw std::runtime_error("a body has no shape");
    }
    shapes.push_back(shape->occt());
  }
  return shapes;
}

bool is_index(const std::string& text) {
  return !text.empty() && text.size() < 10 &&
         std::all_of(text.begin(), text.end(), [](char c) { return c >= '0' && c <= '9'; });
}

// The points of a free-form surface's grid at most, and per direction.
constexpr std::size_t kGridPoints = 40000;
constexpr int kGridSide = 400;
// The size of a spline's projection search grid (points) above which the
// projection is let go after each query and points are placed on it by a
// local search only (see BoundaryIndex::Probe::large): a body of a few
// dozen coils otherwise held gigabytes in such grids.
constexpr double kLargeSpline = 20000.0;
// The local searches at most that place a point on a free-form face (see
// BoundaryIndex::free_form_distance).
constexpr std::size_t kSeeds = 64;

// How many times OCCT's distance query measured a point against a face
// (analysis_whole_face_searches).
std::atomic<std::size_t> whole_face_searches{0};

// What boundary_distances keeps of a shape between calls: its faces with
// their boxes, each face's projection and classifier once a point near it
// asks for them, and the distance query with the faces loaded. Used under
// its lock: OCCT's projections keep state of their own.
struct BoundaryIndex {
  // A point on the boundary (most points asked about are) is found on a
  // face whose box holds it: projected onto its surface and classified in
  // its parameter space, without measuring the other faces.
  struct Probe {
    TopoDS_Face face;
    Bnd_Box box;
    // Planes, cylinders, cones, spheres and tori project directly; they
    // are asked first.
    bool analytic = false;
    Handle(ShapeAnalysis_Surface) surface;
    std::unique_ptr<BRepTopAdaptor_FClass2d> inside;
    // Other surfaces: points over the face's parameter range (with their
    // parameters), from the nearest of which a point is projected (the
    // general projection searches the whole surface for every point:
    // ten milliseconds and more on a stored thread's splines), and how far
    // a point of the surface can be from the nearest of them.
    std::vector<gp_Pnt> grid;
    std::vector<gp_Pnt2d> grid_uv;
    double reach = 0.0;
    // The grid's points in u and in v (grid[i * grid_v + j]).
    int grid_u = 0;
    int grid_v = 0;
    // The grid is made (the projection may have been let go since).
    bool prepared = false;
    // A spline of many knot spans: its projection's own search grid (one
    // point per span and degree in each direction, held for the shape's
    // life) takes megabytes and more, so it is never searched as a whole
    // and is let go after each query (a thread or coil swept along a
    // sampled helix).
    bool large = false;
    // A plane, cylinder or sphere: the projection onto the surface is the
    // nearest point of the whole surface, and the distance has no other
    // local minimum on it, so a point whose projection is off the face is
    // nearest to the face's boundary (see nearest).
    bool simple = false;
    // The face's index in the Shape.
    int index = -1;
    // The face's edges with their boxes, once a point off the face asks.
    std::vector<std::pair<TopoDS_Edge, Bnd_Box>> edges;
    bool edges_listed = false;
    // OCCT's exact classifier's view of the face, set up once (`inside`
    // works on polygons of the edges, which can be off by more than the
    // distances asked about: a point 5e-4 inside a cylinder's face came
    // out outside).
    std::unique_ptr<BRepClass_FaceExplorer> explorer;
    // The face's parameter range {u0, u1, v0, v1}, with the explorer.
    std::array<double, 4> bounds{};
  };

  explicit BoundaryIndex(const geometry::Shape& shape) {
    // The faces only, so that a point inside a solid has its distance to
    // the boundary, not zero.
    for (int i = 0; i < shape.face_count(); ++i) {
      Probe probe;
      probe.face = shape.face(i);
      probe.index = i;
      BRepBndLib::Add(probe.face, probe.box);
      probe.box.Enlarge(kNearBoundary);
      switch (BRepAdaptor_Surface(probe.face, false).GetType()) {
      case GeomAbs_Plane:
      case GeomAbs_Cylinder:
      case GeomAbs_Sphere:
        probe.simple = true;
        probe.analytic = true;
        break;
      case GeomAbs_Cone:
      case GeomAbs_Torus:
        probe.analytic = true;
        break;
      default:
        break;
      }
      probes.push_back(std::move(probe));
    }
    std::stable_partition(probes.begin(), probes.end(),
                          [](const Probe& probe) { return probe.analytic; });
    by_face.assign(static_cast<std::size_t>(shape.face_count()), -1);
    for (std::size_t k = 0; k < probes.size(); ++k) {
      by_face[static_cast<std::size_t>(probes[k].index)] = static_cast<int>(k);
    }
  }

  // The index kept with the shape, made by the first call.
  static std::shared_ptr<BoundaryIndex> of(const geometry::Shape& shape) {
    std::shared_ptr<BoundaryIndex> index = std::static_pointer_cast<BoundaryIndex>(shape.boundary_index());
    if (!index) {
      index = std::make_shared<BoundaryIndex>(shape);
      shape.set_boundary_index(index);
    }
    return index;
  }

  // Where a simple face's surface is nearest to `p`, when that lies on the
  // face (inside it, or on its boundary within kOnBoundary).
  static std::optional<gp_Pnt2d> projection_on_face(Probe& probe, const gp_Pnt& p) {
    if (!probe.prepared) {
      prepare(probe);
    }
    const gp_Pnt2d uv = probe.surface->ValueOfUV(p, kOnBoundary);
    // On an edge of the face (see analysis_boundary_distances), or inside
    // by OCCT's exact classifier (as its distance query classifies, with
    // the face's tolerance), its view of the face kept.
    if (boundary_distance(probe, probe.surface->Value(uv), kNearBoundary) < kNearBoundary) {
      return uv;
    }
    if (!on_face(probe, uv)) {
      return std::nullopt;
    }
    return uv;
  }

  // Whether the surface's point at `uv` is on the face (inside it or on its
  // boundary), by OCCT's exact classifier as its distance query classifies
  // (with the face's tolerance), its view of the face kept.
  static bool on_face(Probe& probe, const gp_Pnt2d& uv) {
    if (!probe.explorer) {
      probe.explorer = std::make_unique<BRepClass_FaceExplorer>(probe.face);
      BRepTools::UVBounds(probe.face, probe.bounds[0], probe.bounds[1], probe.bounds[2], probe.bounds[3]);
    }
    // A periodic surface's parameters in the face's period (the projection
    // gives the surface's own).
    const GeomAdaptor_Surface& surface = *probe.surface->Adaptor3d();
    const auto in_period = [](double t, bool periodic, double period, double low) {
      return periodic && period > 0.0 ? low + std::fmod(std::fmod(t - low, period) + period, period) : t;
    };
    const gp_Pnt2d at(in_period(uv.X(), surface.IsUPeriodic(), surface.IsUPeriodic() ? surface.UPeriod() : 0.0,
                                probe.bounds[0] - 1e-9),
                      in_period(uv.Y(), surface.IsVPeriodic(), surface.IsVPeriodic() ? surface.VPeriod() : 0.0,
                                probe.bounds[2] - 1e-9));
    BRepClass_FClassifier classifier;
    classifier.Perform(*probe.explorer, at, BRep_Tool::Tolerance(probe.face));
    return classifier.State() != TopAbs_OUT;
  }

  // The distance from `p` to the edges of the face, those whose boxes are
  // nearer than `bound` only (`bound` when none is nearer).
  static double boundary_distance(Probe& probe, const gp_Pnt& p, double bound) {
    if (!probe.edges_listed) {
      for (TopExp_Explorer it(probe.face, TopAbs_EDGE); it.More(); it.Next()) {
        // From the curve, not a display mesh: the box bounds the edge.
        Bnd_Box box;
        BRepBndLib::Add(it.Current(), box, false);
        box.Enlarge(kNearBoundary);
        probe.edges.emplace_back(TopoDS::Edge(it.Current()), box);
      }
      probe.edges_listed = true;
    }
    double best = bound;
    for (const auto& [edge, box] : probe.edges) {
      if (box_distance(box, p) >= best) {
        continue;
      }
      for (TopExp_Explorer v(edge, TopAbs_VERTEX); v.More(); v.Next()) {
        best = std::min(best, p.Distance(BRep_Tool::Pnt(TopoDS::Vertex(v.Current()))));
      }
      if (BRep_Tool::Degenerated(edge)) {
        continue;
      }
      const BRepAdaptor_Curve curve(edge);
      best = std::min({best, p.Distance(curve.Value(curve.FirstParameter())),
                       p.Distance(curve.Value(curve.LastParameter()))});
      const Extrema_ExtPC extrema(p, curve);
      if (extrema.IsDone()) {
        for (int i = 1; i <= extrema.NbExt(); ++i) {
          if (extrema.IsMin(i)) {
            best = std::min(best, std::sqrt(extrema.SquareDistance(i)));
          }
        }
      }
    }
    return best;
  }

  // The distance from `p` to the face (`bound` or less when it is nearer
  // than `bound`): a simple face's projection where it lies on the face,
  // or its edges where nearer (an edge's curve may lie off the surface
  // within its tolerance, and OCCT's distance query measures it too);
  // free-form faces from their grids (free_form_distance); cones and tori
  // measured by OCCT's distance query.
  static double face_distance(Probe& probe, const gp_Pnt& p, double bound) {
    if (probe.simple) {
      double d = bound;
      if (const std::optional<gp_Pnt2d> uv = projection_on_face(probe, p)) {
        d = std::min(d, p.Distance(probe.surface->Value(*uv)));
      }
      return boundary_distance(probe, p, d);
    }
    if (!probe.analytic) {
      if (const std::optional<double> d = free_form_distance(probe, p, bound)) {
        return *d;
      }
    }
    return whole_face_distance(probe, p);
  }

  // OCCT's distance query from `p` to the face. On a free-form face it
  // builds a search grid over the whole surface and classifies every
  // extremum it finds (one a turn on a thread's flank) with a classifier set
  // up anew, for every point: tens of milliseconds a point on a stored
  // thread's splines (mitcad#442).
  static double whole_face_distance(Probe& probe, const gp_Pnt& p) {
    ++whole_face_searches;
    BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(p).Vertex(), probe.face);
    return distance.IsDone() && distance.NbSolution() > 0 ? distance.Value()
                                                          : std::numeric_limits<double>::infinity();
  }

  // The distance from `p` to a free-form face (`bound` or less when it is
  // nearer than `bound`) from what the probe keeps of the face (its grid,
  // surface, edges and classifier), as OCCT's distance query measures it:
  // the nearer of the face's edges and the surface's extrema on the face.
  // Any extremum nearer than the edges has a grid point within `reach` of
  // it, so each region of grid points nearer than the edges plus `reach`
  // holds a grid point nearer than its neighbours: the extrema are searched
  // locally from those only, the nearest first. None when the regions hold
  // more such points than kSeeds (a point about as far from much of the
  // face, e.g. on a surface of revolution's axis): the distance query
  // measures it.
  static std::optional<double> free_form_distance(Probe& probe, const gp_Pnt& p, double bound) {
    if (!probe.prepared) {
      prepare(probe);
    } else if (probe.surface.IsNull()) {
      probe.surface = new ShapeAnalysis_Surface(BRep_Tool::Surface(probe.face));
    }
    const std::size_t nu = static_cast<std::size_t>(probe.grid_u);
    const std::size_t nv = static_cast<std::size_t>(probe.grid_v);
    if (probe.grid.empty() || nu * nv != probe.grid.size()) {
      return std::nullopt;
    }
    double best = boundary_distance(probe, p, bound);
    std::vector<double> squared(probe.grid.size());
    for (std::size_t k = 0; k < probe.grid.size(); ++k) {
      squared[k] = p.SquareDistance(probe.grid[k]);
    }
    const double limit = best + probe.reach;
    std::vector<std::pair<double, std::size_t>> seeds;
    for (std::size_t i = 0; i < nu; ++i) {
      for (std::size_t j = 0; j < nv; ++j) {
        const std::size_t k = i * nv + j;
        if (!(std::sqrt(squared[k]) < limit)) {
          continue;
        }
        bool lowest = true;
        for (std::size_t a = i > 0 ? i - 1 : 0; lowest && a <= std::min(i + 1, nu - 1); ++a) {
          for (std::size_t b = j > 0 ? j - 1 : 0; b <= std::min(j + 1, nv - 1); ++b) {
            if (squared[a * nv + b] < squared[k]) {
              lowest = false;
              break;
            }
          }
        }
        if (lowest) {
          seeds.emplace_back(squared[k], k);
          if (seeds.size() > kSeeds) {
            return std::nullopt;
          }
        }
      }
    }
    std::sort(seeds.begin(), seeds.end());
    const GeomAdaptor_Surface& surface = *probe.surface->Adaptor3d();
    for (const auto& [squared_distance, k] : seeds) {
      if (std::sqrt(squared_distance) >= best + probe.reach) {
        break;
      }
      Extrema_GenLocateExtPS locate(surface);
      locate.Perform(p, probe.grid_uv[k].X(), probe.grid_uv[k].Y());
      if (!locate.IsDone()) {
        continue;
      }
      const double d = std::sqrt(locate.SquareDistance());
      if (d >= best) {
        continue;
      }
      double u = 0.0;
      double v = 0.0;
      locate.Point().Parameter(u, v);
      if (on_face(probe, gp_Pnt2d(u, v))) {
        best = d;
      }
    }
    return best;
  }

  // The distance from `p` to the nearest face, the faces taken by the
  // distance of their boxes: those farther than the nearest face found so
  // far are not measured. (OCCT's distance query over all faces builds a
  // classifier of every face whose box holds the point, for every point: a
  // face of a thousand holes took a second a point.)
  double nearest(const gp_Pnt& p) {
    std::vector<std::pair<double, std::size_t>> order;
    order.reserve(probes.size());
    for (std::size_t k = 0; k < probes.size(); ++k) {
      order.emplace_back(box_distance(probes[k].box, p), k);
    }
    std::sort(order.begin(), order.end());
    double best = std::numeric_limits<double>::infinity();
    for (const auto& [lower, k] : order) {
      if (lower >= best) {
        break;
      }
      best = std::min(best, face_distance(probes[k], p, best));
    }
    return best;
  }

  // How far `p` is from the box (0 inside it, or for a void box).
  static double box_distance(const Bnd_Box& box, const gp_Pnt& p) {
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
    return std::sqrt(dx * dx + dy * dy + dz * dz);
  }

  // The face's projection, classifier and (not analytic) grid.
  static void prepare(Probe& probe) {
    probe.surface = new ShapeAnalysis_Surface(BRep_Tool::Surface(probe.face));
    probe.inside = std::make_unique<BRepTopAdaptor_FClass2d>(probe.face, kOnBoundary);
    probe.prepared = true;
    if (probe.analytic) {
      return;
    }
    double u0 = 0.0;
    double u1 = 0.0;
    double v0 = 0.0;
    double v1 = 0.0;
    BRepTools::UVBounds(probe.face, u0, u1, v0, v1);
    const GeomAdaptor_Surface& surface = *probe.surface->Adaptor3d();
    // A spline's knot spans, each sampled by its degree; else a fixed grid.
    int nu = 32;
    int nv = 32;
    if (surface.GetType() == GeomAbs_BSplineSurface) {
      nu = std::clamp((surface.NbUKnots() - 1) * (surface.UDegree() + 1) + 1, 9, kGridSide);
      nv = std::clamp((surface.NbVKnots() - 1) * (surface.VDegree() + 1) + 1, 9, kGridSide);
      // The search grid's size: the knot spans times the degree, at least
      // 44, in each direction (OCCT's Extrema_GenExtPS).
      const double grid_u = std::max(44.0, static_cast<double>(surface.NbUKnots()) * surface.UDegree());
      const double grid_v = std::max(44.0, static_cast<double>(surface.NbVKnots()) * surface.VDegree());
      probe.large = grid_u * grid_v > kLargeSpline;
    }
    while (static_cast<std::size_t>(nu) * static_cast<std::size_t>(nv) > kGridPoints) {
      if (nu >= nv) {
        nu = std::max(9, nu * 3 / 4);
      } else {
        nv = std::max(9, nv * 3 / 4);
      }
    }
    probe.grid_u = nu;
    probe.grid_v = nv;
    probe.grid.reserve(static_cast<std::size_t>(nu) * static_cast<std::size_t>(nv));
    for (int i = 0; i < nu; ++i) {
      for (int j = 0; j < nv; ++j) {
        const double u = u0 + (u1 - u0) * static_cast<double>(i) / static_cast<double>(nu - 1);
        const double v = v0 + (v1 - v0) * static_cast<double>(j) / static_cast<double>(nv - 1);
        probe.grid_uv.emplace_back(u, v);
        probe.grid.push_back(surface.Value(u, v));
      }
    }
    // A point of a cell is about as near one of its corners as its
    // diagonals are long (half again, for its curvature).
    const auto at = [&](int i, int j) -> const gp_Pnt& {
      return probe.grid[static_cast<std::size_t>(i) * static_cast<std::size_t>(nv) +
                        static_cast<std::size_t>(j)];
    };
    double diagonal = 0.0;
    for (int i = 0; i + 1 < nu; ++i) {
      for (int j = 0; j + 1 < nv; ++j) {
        diagonal = std::max({diagonal, at(i, j).Distance(at(i + 1, j + 1)),
                             at(i + 1, j).Distance(at(i, j + 1))});
      }
    }
    probe.reach = 1.5 * diagonal + kNearBoundary;
  }

  std::mutex mutex;
  std::vector<Probe> probes;
  // The Shape's face index -> its probe.
  std::vector<int> by_face;
  // The distances found, by point: the import asks about the same points
  // (the edges of a body it did not change) for several items.
  std::map<std::array<double, 3>, double> known;
};

AnalysisDeviation deviation(const analysis::Deviation& d) {
  AnalysisDeviation result;
  result.max = d.max;
  result.rms = d.rms;
  result.at = xyz(d.at);
  result.samples = d.samples;
  return result;
}

// Runs an analysis the application asks for (Section Analysis, Measure,
// Interference, Physical Properties) as an operation of the geometry
// library: a crash inside OCCT is an error, also on the UI thread, where
// Section Analysis clips the bodies shown (mitcad#392).
template <class Work>
auto guarded(const char* operation, Work&& work) -> decltype(work()) {
  std::optional<decltype(work())> result;
  geometry::run_operation(operation, [&] { result.emplace(work()); });
  return std::move(*result);
}

// The plane passes this much farther from a body than its tolerances
// before its section is taken as empty without a boolean.
constexpr double kSectionMargin = 1.0e-6;

// The body's exact box, kept with it once measured (geometry::bounding_box:
// Section Analysis asks again at every distance); none for an empty one.
std::optional<analysis::Bounds> kept_bounds(const geometry::Shape& shape) {
  const geometry::BoundingBox box = geometry::bounding_box(shape);
  if (box.empty) {
    return std::nullopt;
  }
  analysis::Bounds bounds;
  bounds.min = {box.min.X(), box.min.Y(), box.min.Z()};
  bounds.max = {box.max.X(), box.max.Y(), box.max.Z()};
  bounds.empty = false;
  return bounds;
}

// The largest tolerance of the shape's vertices, edges and faces: a
// boolean meets the plane within it.
double max_tolerance(const TopoDS_Shape& shape) {
  double tolerance = 0.0;
  for (TopExp_Explorer it(shape, TopAbs_VERTEX); it.More(); it.Next()) {
    tolerance = std::max(tolerance, BRep_Tool::Tolerance(TopoDS::Vertex(it.Current())));
  }
  for (TopExp_Explorer it(shape, TopAbs_EDGE); it.More(); it.Next()) {
    tolerance = std::max(tolerance, BRep_Tool::Tolerance(TopoDS::Edge(it.Current())));
  }
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    tolerance = std::max(tolerance, BRep_Tool::Tolerance(TopoDS::Face(it.Current())));
  }
  return tolerance;
}

// The section of a body as analysis::section takes it; an empty one,
// without a boolean, when the plane passes the body's box farther than its
// tolerances (mitcad#438: Section Analysis measures every body shown, and
// most lie on one side of the plane).
// Whether the plane passes the body's box farther than the body's
// tolerances: its section is empty, and its clip keeps or removes it whole.
bool plane_passes(const geometry::Shape& shape, const analysis::Bounds& box, const analysis::Plane& plane) {
  const analysis::PlaneReach reach = analysis::plane_reach(box, plane);
  const double gap = std::max(reach.min, -reach.max);
  return gap > kSectionMargin && gap > max_tolerance(shape.occt()) + kSectionMargin &&
         analysis::clip_side(box, plane) != analysis::ClipSide::Cut;
}

analysis::Section section_of(const geometry::Shape& shape, const analysis::Plane& plane) {
  if (const std::optional<analysis::Bounds> box = kept_bounds(shape)) {
    if (plane_passes(shape, *box, plane)) {
      analysis::Section none;
      BRep_Builder builder;
      TopoDS_Compound curves;
      builder.MakeCompound(curves);
      none.curves = curves;
      TopoDS_Compound faces;
      builder.MakeCompound(faces);
      none.faces = faces;
      return none;
    }
  }
  return guarded("section", [&] { return analysis::section(shape.occt(), plane); });
}

// What Section Analysis' worker may read of a body on another thread
// (mitcad#438), made on the thread that owns the body: null when the plane
// passes it (plane_passes), its box now kept with it, so that its clip and
// section are told from that box alone; else a copy that shares no face,
// edge or vertex with it (geometry::independent_copy: the same names, the
// same order), its box set from the body's, and its mesh faces' triangles
// copied (a mesh body has no surfaces to mesh again). Nothing the view or a
// model job writes into the body (triangulations, polygons, flags) is then
// read on the other thread.
std::shared_ptr<geometry::Shape> detached_of(const geometry::Shape& shape, const analysis::Plane& plane) {
  const std::optional<analysis::Bounds> box = kept_bounds(shape);
  if (box && plane_passes(shape, *box, plane)) {
    return nullptr;
  }
  return guarded("copy", [&] {
    const std::shared_ptr<geometry::Shape> copy = geometry::independent_copy(shape);
    BRep_Builder builder;
    for (int i = 0; i < shape.face_count() && i < copy->face_count(); ++i) {
      TopLoc_Location location;
      const TopoDS_Face& face = shape.face(i);
      if (!BRep_Tool::Surface(face, location).IsNull()) {
        continue;
      }
      const occ::handle<Poly_Triangulation>& triangles = BRep_Tool::Triangulation(face, location);
      if (!triangles.IsNull()) {
        builder.UpdateFace(copy->face(i), triangles->Copy());
      }
    }
    if (const auto bounds = shape.bounds()) {
      copy->set_bounds(*bounds);
    }
    return copy;
  });
}

// How a clip that needed more than the plain cut was made (mitcad#1230),
// kept as the clipped shape's information (geometry::Shape::infos), which
// the application logs: "clipped at attempt 2, with a fuzzy value of
// 1e-06 mm (plain cut: the boolean failed)".
std::string clip_note(const analysis::ClipReport& report) {
  char text[128];
  if (report.attempt == analysis::ClipAttempt::Fuzzy) {
    std::snprintf(text, sizeof text, "clipped at attempt 2, with a fuzzy value of %g mm", report.fuzzy);
  } else {
    std::snprintf(text, sizeof text, "clipped at attempt 3, with the plane moved by %g mm along its normal",
                  report.shift);
  }
  return std::string(text) + " (" + report.failed + ")";
}

// What Section Analysis shows of a body cut at the plane: null when the
// plane leaves it whole, for the caller to show the body's own Shape (the
// same B-rep, which the view keeps drawn as it is, mitcad#438), and no
// faces when the plane removes all of it. A body the plane does not reach
// is told from its box, without a boolean. What is left of each face keeps
// the body's names, and so do the edges and vertices between them: picks
// on the clipped view refer to the body (mitcad#233). The caps have none
// and cannot be picked.
std::shared_ptr<geometry::Shape> clip_of(const geometry::Shape& shape, const analysis::Plane& plane) {
  if (const std::optional<analysis::Bounds> box = kept_bounds(shape)) {
    switch (analysis::clip_side(*box, plane)) {
    case analysis::ClipSide::Kept:
      return nullptr;
    case analysis::ClipSide::Removed: {
      TopoDS_Compound nothing;
      BRep_Builder().MakeCompound(nothing);
      return std::make_shared<geometry::Shape>(nothing);
    }
    case analysis::ClipSide::Cut:
      break;
    }
  }
  return guarded("clip", [&]() -> std::shared_ptr<geometry::Shape> {
    std::vector<analysis::ClippedFace> faces;
    analysis::ClipReport report;
    TopoDS_Shape clipped = analysis::clip(shape.occt(), plane, &faces, &report);
    if (clipped.IsEqual(shape.occt())) {
      return nullptr;
    }
    std::vector<geometry::Shape::NamedFace> named;
    for (const analysis::ClippedFace& face : faces) {
      const int i = shape.face_index(face.face);
      if (i < 0 || shape.face_names(i).empty()) {
        continue;
      }
      for (const TopoDS_Shape& piece : face.pieces) {
        named.push_back({piece, shape.face_names(i)});
      }
    }
    auto result = std::make_shared<geometry::Shape>(std::move(clipped), named);
    if (report.attempt != analysis::ClipAttempt::Plain) {
      result->add_info(clip_note(report));
    }
    return result;
  });
}

// Runs work(i) for each of `count` items on all cores (OCCT's thread
// pool), each item's work an operation of its own (clip_of, section_of):
// its error, also a crash inside OCCT, is its own, and the other items go
// on. What tells the calling thread's operations to stop tells each
// item's too (mitcad#438: Section Analysis off the UI thread stops for a
// newer plane), and an item not started by then fails as cancelled.
// Returns the errors, empty for the items that worked.
template <class Work>
std::vector<std::string> each_on_all_cores(std::size_t count, Work&& work) {
  std::vector<std::string> errors(count);
  const geometry::CancelSource* source = geometry::cancel_source();
  OSD_Parallel::For(
      0, static_cast<int>(count),
      [&](int i) {
        const auto k = static_cast<std::size_t>(i);
        try {
          if (source != nullptr) {
            const geometry::CancelScope scope(*source);
            geometry::throw_if_cancelled();
            work(k);
          } else {
            work(k);
          }
        } catch (const std::exception& error) {
          errors[k] = error.what();
          if (errors[k].empty()) {
            errors[k] = "failed";
          }
        } catch (...) {
          errors[k] = "failed";
        }
      },
      count < 2);
  return errors;
}

std::vector<analysis::Plane> planes_of(const ShapeList& shapes, rust::Slice<const AnalysisPlane> planes) {
  if (planes.size() != shapes.size()) {
    throw std::invalid_argument("one plane per shape is needed");
  }
  std::vector<analysis::Plane> result;
  result.reserve(planes.size());
  for (const AnalysisPlane& plane : planes) {
    result.push_back(plane_of(plane));
  }
  return result;
}

} // namespace

std::shared_ptr<geometry::Shape> AnalysisInterferences::shape(std::size_t index) const {
  return std::make_shared<geometry::Shape>(m_found.at(index).common);
}

std::shared_ptr<geometry::Shape> AnalysisSection::curves() const {
  return std::make_shared<geometry::Shape>(m_section.curves);
}

std::shared_ptr<geometry::Shape> AnalysisSection::faces() const {
  return std::make_shared<geometry::Shape>(m_section.faces);
}

AnalysisProperties analysis_properties(const geometry::Shape& shape, double density) {
  if (!(density > 0.0)) {
    throw std::invalid_argument("the density must be greater than zero");
  }
  const analysis::PhysicalProperties p = guarded(
      "physical properties", [&] { return analysis::physical_properties(shape.occt(), density); });
  AnalysisProperties result{};
  result.volume = p.volume;
  result.area = p.area;
  result.mass = p.mass;
  result.center = xyz(p.center_of_mass);
  for (std::size_t i = 0; i < 3; ++i) {
    for (std::size_t j = 0; j < 3; ++j) {
      result.inertia[3 * i + j] = p.inertia[i][j];
    }
    const std::array<double, 3> axis = xyz(p.principal_axes[i]);
    for (std::size_t k = 0; k < 3; ++k) {
      result.principal_axes[3 * i + k] = axis[k];
    }
  }
  result.principal_moments = {p.principal_moments[0], p.principal_moments[1],
                              p.principal_moments[2]};
  return result;
}

AnalysisMeasure analysis_measure(const geometry::Shape& shape, rust::Str name) {
  return guarded("measure", [&] {
    const Selected selected = select(shape, std::string(name));
    AnalysisMeasure result{};
    result.kind = rust::String(selected.kind);
    if (selected.kind == "body") {
      const analysis::PhysicalProperties p = analysis::physical_properties(selected.shape, 1.0);
      result.has_volume = true;
      result.volume = p.volume;
      result.has_area = true;
      result.area = p.area;
    } else if (selected.kind == "face") {
      result.has_area = true;
      result.area = analysis::area(selected.shape);
    } else if (selected.kind == "edge") {
      result.has_length = true;
      result.length = analysis::length(selected.shape);
    } else if (selected.count == 1) {
      result.has_point = true;
      result.point = xyz(analysis::point(selected.shape));
    }
    if (selected.count == 1 && (selected.kind == "face" || selected.kind == "edge")) {
      if (const std::optional<analysis::Circle> circle = analysis::circle(selected.shape)) {
        result.has_circle = true;
        result.center = xyz(circle->center);
        result.axis = xyz(circle->axis);
        result.radius = circle->radius;
        result.sweep = circle->sweep;
      }
    }
    return result;
  });
}

AnalysisSeparation analysis_between(const geometry::Shape& a, rust::Str a_name,
                                    const geometry::Shape& b, rust::Str b_name) {
  return guarded("distance between", [&] {
    const Selected first = select(a, std::string(a_name));
    const Selected second = select(b, std::string(b_name));
    const analysis::Distance distance = analysis::min_distance(first.shape, second.shape);
    AnalysisSeparation result{};
    result.distance = distance.value;
    result.on_a = xyz(distance.on_a);
    result.on_b = xyz(distance.on_b);
    result.inside = distance.inside;
    try {
      result.angle = analysis::angle(first.shape, second.shape);
      result.has_angle = true;
    } catch (const analysis::Error&) {
      // Only planar faces and straight edges have an angle.
    }
    return result;
  });
}

AnalysisFaceGap analysis_face_gap(const geometry::Shape& a, rust::Str a_name,
                                  const geometry::Shape& b, rust::Str b_name) {
  return guarded("maximum face gap", [&] {
    const Selected first = select(a, std::string(a_name));
    const Selected second = select(b, std::string(b_name));
    AnalysisFaceGap result{};
    if (first.kind != "face" || second.kind != "face" || first.count != 1 || second.count != 1) {
      return result;
    }
    const TopoDS_Face fa = TopoDS::Face(first.shape);
    const TopoDS_Face fb = TopoDS::Face(second.shape);
    const auto polygon = [](const TopoDS_Face& face, rust::Vec<double>& vertices) {
      if (BRepAdaptor_Surface(face).GetType() != GeomAbs_Plane) {
        return false;
      }
      TopExp_Explorer wires(face, TopAbs_WIRE);
      if (!wires.More()) {
        return false;
      }
      const TopoDS_Wire wire = TopoDS::Wire(wires.Current());
      wires.Next();
      if (wires.More() || !wire.Closed()) {
        return false; // A hole or open boundary needs a different maximum solver.
      }
      std::size_t edges = 0;
      for (TopExp_Explorer it(wire, TopAbs_EDGE); it.More(); it.Next()) {
        ++edges;
      }
      std::size_t traversed = 0;
      for (BRepTools_WireExplorer it(wire, face); it.More(); it.Next()) {
        if (BRep_Tool::Degenerated(it.Current()) ||
            BRepAdaptor_Curve(it.Current()).GetType() != GeomAbs_Line || it.CurrentVertex().IsNull()) {
          return false;
        }
        const gp_Pnt p = BRep_Tool::Pnt(it.CurrentVertex());
        vertices.push_back(p.X());
        vertices.push_back(p.Y());
        vertices.push_back(p.Z());
        ++traversed;
      }
      return traversed >= 3 && traversed == edges;
    };
    if (!polygon(fa, result.vertices_a) || !polygon(fb, result.vertices_b)) {
      return result;
    }
    const auto gaps = [&result](const rust::Vec<double>& vertices, const TopoDS_Face& target) {
      for (std::size_t i = 0; i < vertices.size(); i += 3) {
        const TopoDS_Vertex vertex =
            BRepBuilderAPI_MakeVertex(gp_Pnt(vertices[i], vertices[i + 1], vertices[i + 2]));
        result.distances.push_back(analysis::min_distance(vertex, target).value);
      }
    };
    gaps(result.vertices_a, fb);
    gaps(result.vertices_b, fa);
    result.supported = true;
    return result;
  });
}

std::unique_ptr<AnalysisInterferences> analysis_interferences(const ShapeList& bodies,
                                                              double min_volume) {
  analysis::InterferenceOptions options;
  options.min_volume = min_volume;
  return guarded("interference", [&] {
    return std::make_unique<AnalysisInterferences>(analysis::interferences(occt(bodies), options));
  });
}

std::unique_ptr<AnalysisSection> analysis_section(const geometry::Shape& shape,
                                                  const AnalysisPlane& plane) {
  return std::make_unique<AnalysisSection>(section_of(shape, plane_of(plane)));
}

std::shared_ptr<geometry::Shape> analysis_clip(const geometry::Shape& shape,
                                               const AnalysisPlane& plane) {
  return clip_of(shape, plane_of(plane));
}

std::unique_ptr<AnalysisSection> AnalysisSections::section(std::size_t index) const {
  const std::optional<analysis::Section>& found = m_sections.at(index);
  return found ? std::make_unique<AnalysisSection>(*found) : nullptr;
}

std::unique_ptr<AnalysisSections> analysis_section_all(const ShapeList& shapes,
                                                       rust::Slice<const AnalysisPlane> planes) {
  const std::vector<analysis::Plane> each = planes_of(shapes, planes);
  std::vector<std::optional<analysis::Section>> sections(each.size());
  std::vector<std::string> errors = each_on_all_cores(each.size(), [&](std::size_t k) {
    const std::shared_ptr<geometry::Shape> shape = shapes.at(k);
    if (!shape) {
      throw std::invalid_argument("the body has no shape");
    }
    sections[k] = section_of(*shape, each[k]);
  });
  return std::make_unique<AnalysisSections>(std::move(sections), std::move(errors));
}

std::unique_ptr<AnalysisClips> analysis_clip_all(const ShapeList& shapes,
                                                 rust::Slice<const AnalysisPlane> planes) {
  const std::vector<analysis::Plane> each = planes_of(shapes, planes);
  std::vector<std::shared_ptr<geometry::Shape>> clipped(each.size());
  std::vector<std::string> errors = each_on_all_cores(each.size(), [&](std::size_t k) {
    const std::shared_ptr<geometry::Shape> shape = shapes.at(k);
    if (!shape) {
      throw std::invalid_argument("the body has no shape");
    }
    clipped[k] = clip_of(*shape, each[k]);
  });
  return std::make_unique<AnalysisClips>(std::move(clipped), std::move(errors));
}

std::unique_ptr<AnalysisClips> analysis_detach_all(const ShapeList& shapes,
                                                   rust::Slice<const AnalysisPlane> planes) {
  const std::vector<analysis::Plane> each = planes_of(shapes, planes);
  std::vector<std::shared_ptr<geometry::Shape>> detached(each.size());
  std::vector<std::string> errors = each_on_all_cores(each.size(), [&](std::size_t k) {
    const std::shared_ptr<geometry::Shape> shape = shapes.at(k);
    if (!shape) {
      throw std::invalid_argument("the body has no shape");
    }
    detached[k] = detached_of(*shape, each[k]);
  });
  return std::make_unique<AnalysisClips>(std::move(detached), std::move(errors));
}

AnalysisComparison analysis_compare_step(const ShapeList& bodies, rust::Str path,
                                         rust::Str step_body, std::size_t samples, double fuzzy) {
  const geometry::CheckedOperation check("comparison with a STEP file");
  const std::vector<TopoDS_Shape> mine = occt(bodies);
  if (mine.empty()) {
    throw std::invalid_argument("there are no bodies to compare");
  }
  const std::vector<io::Body> file_bodies = io::read_step(std::string(path));
  if (file_bodies.empty()) {
    throw std::runtime_error("the STEP file has no bodies");
  }
  const std::string wanted(step_body);
  std::vector<const io::Body*> chosen;
  for (const io::Body& body : file_bodies) {
    if (wanted.empty() || body.name == wanted) {
      chosen.push_back(&body);
    }
  }
  if (chosen.empty() && is_index(wanted) && std::stoul(wanted) < file_bodies.size()) {
    chosen.push_back(&file_bodies[std::stoul(wanted)]);
  }
  if (chosen.empty()) {
    std::string names;
    for (const io::Body& body : file_bodies) {
      names += (names.empty() ? "" : ", ") + body.name;
    }
    throw std::runtime_error("the STEP file has no body '" + wanted + "' (it has " + names + ")");
  }
  std::vector<TopoDS_Shape> theirs;
  AnalysisComparison result{};
  for (const io::Body* body : chosen) {
    theirs.push_back(body->shape);
    result.step_bodies.push_back(rust::String(body->name));
  }
  analysis::CompareOptions options;
  options.samples = samples;
  options.fuzzy = fuzzy;
  const analysis::Comparison c = analysis::compare(compound(mine), compound(theirs), options);
  result.volume_a = c.volume_a;
  result.volume_b = c.volume_b;
  result.has_differences = c.a_minus_b.has_value() && c.b_minus_a.has_value();
  result.a_minus_b = c.a_minus_b.value_or(0.0);
  result.b_minus_a = c.b_minus_a.value_or(0.0);
  result.relative_difference = c.relative_difference.value_or(0.0);
  result.a_to_b = deviation(c.a_to_b);
  result.b_to_a = deviation(c.b_to_a);
  result.max_deviation = c.max_deviation;
  result.bounds_difference = c.bounds_difference;
  return result;
}

// .f3d import (T1).

AnalysisComparison analysis_compare_shapes(const ShapeList& a, const ShapeList& b,
                                           std::size_t samples, double fuzzy, double seconds,
                                           double booleans_above) {
  const geometry::CheckedOperation check("comparison");
  const std::vector<TopoDS_Shape> mine = occt(a);
  const std::vector<TopoDS_Shape> theirs = occt(b);
  if (mine.empty() || theirs.empty()) {
    throw std::invalid_argument("there are no bodies to compare");
  }
  analysis::CompareOptions options;
  options.samples = samples;
  options.fuzzy = fuzzy;
  options.seconds = seconds;
  options.booleans_above = booleans_above;
  const analysis::Comparison c = analysis::compare(compound(mine), compound(theirs), options);
  AnalysisComparison result{};
  result.volume_a = c.volume_a;
  result.volume_b = c.volume_b;
  result.has_differences = c.a_minus_b.has_value() && c.b_minus_a.has_value();
  result.a_minus_b = c.a_minus_b.value_or(0.0);
  result.b_minus_a = c.b_minus_a.value_or(0.0);
  result.relative_difference = c.relative_difference.value_or(0.0);
  result.a_to_b = deviation(c.a_to_b);
  result.b_to_a = deviation(c.b_to_a);
  result.max_deviation = c.max_deviation;
  result.bounds_difference = c.bounds_difference;
  return result;
}

rust::Vec<double> analysis_boundary_distances(const geometry::Shape& shape,
                                              rust::Slice<const double> points) {
  const geometry::CheckedOperation check("boundary distances");
  if (points.size() % 3 != 0) {
    throw std::invalid_argument("points come as x, y, z triples");
  }
  // The shape's boundary index, made by the first call and kept with the
  // shape (it never changes): the import asks about the same history
  // states for item after item, and setting up the faces' projections
  // takes most of the time.
  const std::shared_ptr<BoundaryIndex> index = BoundaryIndex::of(shape);
  const std::lock_guard<std::mutex> lock(index->mutex);
  std::vector<BoundaryIndex::Probe>& probes = index->probes;
  // The point's distance from the face's surface where it projects inside
  // the face, else none (also when it is far from a free-form surface's
  // grid, or the projection from the nearest grid point finds no nearer
  // point: BoundaryIndex::nearest then measures it).
  const auto gap = [](BoundaryIndex::Probe& probe, const gp_Pnt& p) -> std::optional<double> {
    if (!probe.prepared) {
      BoundaryIndex::prepare(probe);
    } else if (probe.surface.IsNull()) {
      probe.surface = new ShapeAnalysis_Surface(BRep_Tool::Surface(probe.face));
    }
    gp_Pnt2d uv;
    if (probe.analytic) {
      uv = probe.surface->ValueOfUV(p, kOnBoundary);
    } else {
      std::size_t nearest = 0;
      double best = std::numeric_limits<double>::infinity();
      for (std::size_t k = 0; k < probe.grid.size(); ++k) {
        const double d = p.SquareDistance(probe.grid[k]);
        if (d < best) {
          best = d;
          nearest = k;
        }
      }
      if (probe.grid.empty() || std::sqrt(best) > probe.reach) {
        return std::nullopt;
      }
      if (probe.large) {
        // A local search from the nearest grid point only: when it fails,
        // NextValueOfUV falls back to the general projection, whose
        // search grid on such a spline takes gigabytes (memory limits
        // then end the process in OCCT). BoundaryIndex::nearest measures a
        // point it does not place.
        Extrema_GenLocateExtPS locate(*probe.surface->Adaptor3d());
        locate.Perform(p, probe.grid_uv[nearest].X(), probe.grid_uv[nearest].Y());
        if (!locate.IsDone()) {
          return std::nullopt;
        }
        double u = 0.0;
        double v = 0.0;
        locate.Point().Parameter(u, v);
        uv.SetCoord(u, v);
      } else {
        uv = probe.surface->NextValueOfUV(probe.grid_uv[nearest], p, kOnBoundary, -1.0);
      }
    }
    const double g = p.Distance(probe.surface->Value(uv));
    if (g > kNearBoundary) {
      return std::nullopt;
    }
    // A point on one of the face's edges is on the face. (Most points
    // asked about are edges' middles: the classifier finds them on the
    // boundary only by OCCT's exact classifier, which it sets up for the
    // whole face every time, a second a point on a face of a thousand
    // holes.)
    if (BoundaryIndex::boundary_distance(probe, p, kNearBoundary) < kNearBoundary) {
      return g;
    }
    if (probe.inside->Perform(uv) == TopAbs_OUT) {
      return std::nullopt;
    }
    return g;
  };
  const auto measure = [&](const gp_Pnt& p) {
    // On a face: zero. Else near one (a stored surface that the replay's
    // approximates), or measured.
    double near = std::numeric_limits<double>::infinity();
    for (BoundaryIndex::Probe& probe : probes) {
      if (probe.box.IsOut(p)) {
        continue;
      }
      const std::optional<double> g = gap(probe, p);
      if (!g) {
        continue;
      }
      if (*g <= kOnBoundary) {
        return 0.0;
      }
      near = std::min(near, *g);
    }
    if (near <= kNearBoundary) {
      return near;
    }
    return index->nearest(p);
  };
  rust::Vec<double> out;
  for (std::size_t i = 0; i < points.size(); i += 3) {
    const std::array<double, 3> key{points[i], points[i + 1], points[i + 2]};
    const auto known = index->known.find(key);
    if (known != index->known.end()) {
      out.push_back(known->second);
      continue;
    }
    const double d = measure(gp_Pnt(key[0], key[1], key[2]));
    index->known.emplace(key, d);
    out.push_back(d);
  }
  for (BoundaryIndex::Probe& probe : probes) {
    if (probe.large) {
      probe.surface.Nullify();
    }
  }
  return out;
}

std::size_t analysis_whole_face_searches() { return whole_face_searches.load(); }

bool analysis_face_point(const geometry::Shape& shape, const std::vector<int>& faces,
                         const std::array<double, 3>& near, std::array<double, 3>& point,
                         std::array<double, 3>& normal) {
  if (faces.empty()) {
    return false;
  }
  try {
    const std::shared_ptr<BoundaryIndex> index = BoundaryIndex::of(shape);
    const std::lock_guard<std::mutex> lock(index->mutex);
    const gp_Pnt p(near[0], near[1], near[2]);
    double best = std::numeric_limits<double>::infinity();
    bool found = false;
    for (const int face : faces) {
      if (face < 0 || static_cast<std::size_t>(face) >= index->by_face.size() ||
          index->by_face[static_cast<std::size_t>(face)] < 0) {
        return false;
      }
      BoundaryIndex::Probe& probe = index->probes[static_cast<std::size_t>(index->by_face[static_cast<std::size_t>(face)])];
      if (!probe.simple) {
        return false;
      }
      // Off the face, the nearest point is on its boundary: the distance
      // query finds it.
      const std::optional<gp_Pnt2d> uv = BoundaryIndex::projection_on_face(probe, p);
      if (!uv) {
        return false;
      }
      const gp_Pnt on = probe.surface->Value(*uv);
      const double d = p.Distance(on);
      // An edge's curve nearer than the surface (off it within the edge's
      // tolerance): OCCT's distance query finds that point.
      if (BoundaryIndex::boundary_distance(probe, p, d) < d - 1e-9) {
        return false;
      }
      if (d >= best) {
        continue;
      }
      gp_Pnt at;
      gp_Vec direction;
      BRepGProp_Face(probe.face).Normal(uv->X(), uv->Y(), at, direction);
      if (direction.Magnitude() <= gp::Resolution()) {
        continue;
      }
      const gp_Dir unit(direction);
      best = d;
      point = {on.X(), on.Y(), on.Z()};
      normal = {unit.X(), unit.Y(), unit.Z()};
      found = true;
    }
    return found;
  } catch (const Standard_Failure&) {
    return false;
  }
}

rust::Vec<bool> analysis_points_inside(const geometry::Shape& shape, rust::Slice<const double> points) {
  const geometry::CheckedOperation check("points inside");
  if (points.size() % 3 != 0) {
    throw std::invalid_argument("points come as x, y, z triples");
  }
  const std::size_t count = points.size() / 3;
  std::vector<bool> inside(count, false);
  for (TopExp_Explorer it(shape.occt(), TopAbs_SOLID); it.More(); it.Next()) {
    // One classifier per solid (its set-up is the expensive part), and
    // points outside the solid's box are outside.
    Bnd_Box box;
    BRepBndLib::Add(it.Current(), box);
    std::vector<std::size_t> candidates;
    for (std::size_t i = 0; i < count; ++i) {
      if (!inside[i] && !box.IsOut(gp_Pnt(points[3 * i], points[3 * i + 1], points[3 * i + 2]))) {
        candidates.push_back(i);
      }
    }
    if (candidates.empty()) {
      continue;
    }
    BRepClass3d_SolidClassifier classifier(it.Current());
    for (const std::size_t i : candidates) {
      classifier.Perform(gp_Pnt(points[3 * i], points[3 * i + 1], points[3 * i + 2]), 1e-7);
      inside[i] = classifier.State() == TopAbs_IN;
    }
  }
  rust::Vec<bool> out;
  for (const bool in : inside) {
    out.push_back(in);
  }
  return out;
}

rust::Vec<double> analysis_segment_crossings(const geometry::Shape& shape, rust::Slice<const double> segment) {
  const geometry::CheckedOperation check("segment crossings");
  if (segment.size() != 6) {
    throw std::invalid_argument("a segment comes as two x, y, z triples");
  }
  const gp_Pnt from(segment[0], segment[1], segment[2]);
  const gp_Pnt to(segment[3], segment[4], segment[5]);
  const double length = from.Distance(to);
  if (length < Precision::Confusion()) {
    throw std::invalid_argument("the segment has no length");
  }
  const gp_Lin line(from, gp_Dir(gp_Vec(from, to)));
  Bnd_Box reach;
  reach.Add(from);
  reach.Add(to);
  reach.Enlarge(1e-6);
  // Only the faces whose boxes the segment reaches are intersected: their
  // set-up is the expensive part, and a classifier sets up every face (a
  // modelled thread's stored body has hundreds of spline faces).
  std::vector<std::pair<double, double>> found;
  for (TopExp_Explorer it(shape.occt(), TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    Bnd_Box box;
    BRepBndLib::Add(face, box);
    if (box.IsOut(reach)) {
      continue;
    }
    IntCurvesFace_Intersector intersector(face, 1e-7);
    intersector.Perform(line, -1e-9, length + 1e-9);
    if (!intersector.IsDone()) {
      continue;
    }
    const BRepGProp_Face oriented(face);
    for (int i = 1; i <= intersector.NbPnt(); ++i) {
      gp_Pnt point;
      gp_Vec normal;
      // The normal points out of the material (the face's orientation).
      oriented.Normal(intersector.UParameter(i), intersector.VParameter(i), point, normal);
      const double along = normal.Dot(gp_Vec(line.Direction()));
      if (std::abs(along) <= 1e-9 * normal.Magnitude()) {
        continue; // touches the face without crossing it
      }
      found.emplace_back(intersector.WParameter(i) / length, along < 0.0 ? -1.0 : 1.0);
    }
  }
  std::sort(found.begin(), found.end());
  rust::Vec<double> out;
  for (std::size_t i = 0; i < found.size(); ++i) {
    // A crossing on an edge is found on each of its faces.
    if (i > 0 && std::abs(found[i].first - found[i - 1].first) < 1e-9 && found[i].second == found[i - 1].second) {
      continue;
    }
    out.push_back(found[i].first);
    out.push_back(found[i].second);
  }
  return out;
}

rust::Vec<double> analysis_segments_crossings(const geometry::Shape& shape, rust::Slice<const double> segments) {
  const geometry::CheckedOperation check("segment crossings");
  if (segments.size() % 6 != 0) {
    throw std::invalid_argument("segments come as two x, y, z triples each");
  }
  const std::size_t count = segments.size() / 6;
  struct Segment {
    gp_Lin line;
    double length = 0.0;
    Bnd_Box reach;
  };
  std::vector<Segment> lines(count);
  for (std::size_t s = 0; s < count; ++s) {
    const gp_Pnt from(segments[6 * s], segments[6 * s + 1], segments[6 * s + 2]);
    const gp_Pnt to(segments[6 * s + 3], segments[6 * s + 4], segments[6 * s + 5]);
    lines[s].length = from.Distance(to);
    if (lines[s].length < Precision::Confusion()) {
      throw std::invalid_argument("a segment has no length");
    }
    lines[s].line = gp_Lin(from, gp_Dir(gp_Vec(from, to)));
    lines[s].reach.Add(from);
    lines[s].reach.Add(to);
    lines[s].reach.Enlarge(1e-6);
  }
  // As analysis_segment_crossings for each segment, with each face's set-up
  // (its intersector and the classifier of its wires, the expensive part on
  // a face with hundreds of holes) made once for all the segments that
  // reach its box (mitcad#181: the probes of a sketch's regions on a body of
  // 5 000 faces took 1.5 s each).
  std::vector<std::vector<std::pair<double, double>>> found(count);
  for (TopExp_Explorer it(shape.occt(), TopAbs_FACE); it.More(); it.Next()) {
    const TopoDS_Face& face = TopoDS::Face(it.Current());
    Bnd_Box box;
    BRepBndLib::Add(face, box);
    std::optional<IntCurvesFace_Intersector> intersector;
    std::optional<BRepGProp_Face> oriented;
    for (std::size_t s = 0; s < count; ++s) {
      const Segment& segment = lines[s];
      if (box.IsOut(segment.reach)) {
        continue;
      }
      if (!intersector) {
        intersector.emplace(face, 1e-7);
        oriented.emplace(face);
      }
      intersector->Perform(segment.line, -1e-9, segment.length + 1e-9);
      if (!intersector->IsDone()) {
        continue;
      }
      for (int i = 1; i <= intersector->NbPnt(); ++i) {
        gp_Pnt point;
        gp_Vec normal;
        oriented->Normal(intersector->UParameter(i), intersector->VParameter(i), point, normal);
        const double along = normal.Dot(gp_Vec(segment.line.Direction()));
        if (std::abs(along) <= 1e-9 * normal.Magnitude()) {
          continue;
        }
        found[s].emplace_back(intersector->WParameter(i) / segment.length, along < 0.0 ? -1.0 : 1.0);
      }
    }
  }
  rust::Vec<double> out;
  for (auto& crossings : found) {
    std::sort(crossings.begin(), crossings.end());
    std::vector<double> kept;
    for (std::size_t i = 0; i < crossings.size(); ++i) {
      if (i > 0 && std::abs(crossings[i].first - crossings[i - 1].first) < 1e-9 &&
          crossings[i].second == crossings[i - 1].second) {
        continue;
      }
      kept.push_back(crossings[i].first);
      kept.push_back(crossings[i].second);
    }
    out.push_back(static_cast<double>(kept.size() / 2));
    for (const double v : kept) {
      out.push_back(v);
    }
  }
  return out;
}

rust::Vec<AnalysisEdgeMiddle> analysis_edge_middles(const geometry::Shape& shape) {
  const geometry::CheckedOperation check("edge middles");
  rust::Vec<AnalysisEdgeMiddle> out;
  for (const geometry::EdgeMiddle& middle : geometry::edge_middles(shape)) {
    AnalysisEdgeMiddle m;
    m.name = rust::String(middle.name);
    m.point = {middle.point.X(), middle.point.Y(), middle.point.Z()};
    m.tangent = {middle.tangent.X(), middle.tangent.Y(), middle.tangent.Z()};
    m.length = middle.length;
    out.push_back(std::move(m));
  }
  return out;
}

rust::Vec<AnalysisFacePoints> analysis_face_points(const geometry::Shape& shape) {
  const geometry::CheckedOperation check("face points");
  rust::Vec<AnalysisFacePoints> out;
  for (const geometry::FacePoints& face : geometry::face_points(shape)) {
    AnalysisFacePoints f;
    f.name = rust::String(face.name);
    for (const gp_Pnt& p : face.points) {
      f.points.push_back(p.X());
      f.points.push_back(p.Y());
      f.points.push_back(p.Z());
    }
    out.push_back(std::move(f));
  }
  return out;
}

rust::Vec<AnalysisNewFace> analysis_new_faces(const ShapeList& before, const geometry::Shape& after,
                                              std::size_t count) {
  const geometry::CheckedOperation check("new faces");
  const int points = static_cast<int>(std::min<std::size_t>(count, 1024));
  rust::Vec<AnalysisNewFace> out;
  for (const geometry::NewFace& face : geometry::new_faces(shapes_of(before), after, points)) {
    AnalysisNewFace f;
    f.area = face.area;
    for (const gp_Pnt& p : face.points) {
      f.points.push_back(p.X());
      f.points.push_back(p.Y());
      f.points.push_back(p.Z());
    }
    out.push_back(std::move(f));
  }
  return out;
}

rust::Vec<double> analysis_changed_volume(const ShapeList& before, const ShapeList& after) {
  const geometry::CheckedOperation check("changed volume");
  const geometry::ChangedVolume changed = geometry::changed_volume(shapes_of(before), shapes_of(after));
  rust::Vec<double> out;
  out.push_back(changed.volume);
  out.push_back(changed.slack);
  return out;
}

std::size_t analysis_face_count(const geometry::Shape& shape) {
  // The shape's own map of its faces (TopExp's, made with it): the .f3d
  // import counts the faces of every body it measures, and mapping a body
  // of 23 000 faces again each time took a tenth of its replay (mitcad#119).
  return static_cast<std::size_t>(shape.face_count());
}

// Face boxes (the .f3d import's face lookups through a point, mitcad#119).

rust::Vec<double> analysis_face_boxes(const geometry::Shape& shape) {
  rust::Vec<double> out;
  const std::vector<std::array<double, 6>> boxes = geometry::face_boxes(shape);
  out.reserve(6 * boxes.size());
  for (const std::array<double, 6>& box : boxes) {
    for (const double value : box) {
      out.push_back(value);
    }
  }
  return out;
}

} // namespace mitcad::bridge
