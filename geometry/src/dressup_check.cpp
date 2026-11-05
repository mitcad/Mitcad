// SPDX-License-Identifier: MIT
#include "dressup_check.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdio>
#include <map>
#include <memory>
#include <optional>
#include <string>
#include <vector>

#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepTools.hxx>
#include <BRepTopAdaptor_FClass2d.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <Geom_Surface.hxx>
#include <NCollection_Map.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <IntCurvesFace_ShapeIntersector.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Precision.hxx>
#include <ChFi3d.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <gp.hxx>
#include <gp_Dir.hxx>
#include <gp_Lin.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>

#include "util.hpp"

namespace mitcad::geometry::detail {
namespace {

// Points looked at on all the made faces together, and at most on one.
constexpr int kPoints = 600;
constexpr int kFacePoints = 49;
// Distances measured to the input for points found on the wrong side, on
// all the made faces together and at most on one.
constexpr int kDistances = 60;
constexpr int kFaceDistances = 6;
// How far to the wrong side counts, of the dressup's size.
constexpr double kReach = 0.01;
// How far the made faces' tolerances may widen that at most, of the size.
constexpr double kTolerated = 0.05;
// How far a ray from a point of a made face looks for the input's boundary,
// of the size, and the least cosine of its angle with the boundary's normal
// there that tells the side.
constexpr double kRayReach = 4.0;
constexpr double kTransverse = 0.05;

// Whether a shape is closed solids only (as the dressup's own check).
bool closed_solids(const TopoDS_Shape& shape) {
  int solids = 0;
  int faces_in_solids = 0;
  for (TopExp_Explorer solid(shape, TopAbs_SOLID); solid.More(); solid.Next()) {
    ++solids;
    for (TopExp_Explorer shell(solid.Current(), TopAbs_SHELL); shell.More(); shell.Next()) {
      if (!BRep_Tool::IsClosed(shell.Current())) {
        return false;
      }
    }
    for (TopExp_Explorer face(solid.Current(), TopAbs_FACE); face.More(); face.Next()) {
      ++faces_in_solids;
    }
  }
  int faces = 0;
  for (TopExp_Explorer face(shape, TopAbs_FACE); face.More(); face.Next()) {
    ++faces;
  }
  return solids > 0 && faces == faces_in_solids;
}

// The largest tolerance of a face's edges and vertices.
double face_tolerance(const TopoDS_Face& face) {
  double tolerance = BRep_Tool::Tolerance(face);
  for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
    tolerance = std::max(tolerance, BRep_Tool::Tolerance(TopoDS::Edge(e.Current())));
  }
  for (TopExp_Explorer v(face, TopAbs_VERTEX); v.More(); v.Next()) {
    tolerance = std::max(tolerance, BRep_Tool::Tolerance(TopoDS::Vertex(v.Current())));
  }
  return tolerance;
}

// A point of a face and the face's outward normal there (none where the
// surface has none).
struct FacePoint {
  gp_Pnt point;
  std::optional<gp_Dir> normal;
};

// The outward normal of a face at its parameters, none where its surface
// is singular.
std::optional<gp_Dir> outward_normal(const BRepAdaptor_Surface& surface, const TopoDS_Face& face, double u,
                                     double v) {
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

// Points inside a face on a grid of its parameters, about `wanted` of them
// (the grid's points that fall in the face).
std::vector<FacePoint> face_points(const TopoDS_Face& face, int wanted) {
  std::vector<FacePoint> points;
  double u0 = 0.0;
  double u1 = 0.0;
  double v0 = 0.0;
  double v1 = 0.0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  if (!(u1 > u0) || !(v1 > v0)) {
    return points;
  }
  const BRepAdaptor_Surface surface(face, false);
  BRepTopAdaptor_FClass2d inside(face, Precision::PConfusion());
  const int n = std::max(2, static_cast<int>(std::lround(std::sqrt(static_cast<double>(wanted)))));
  for (int i = 0; i < n; ++i) {
    for (int j = 0; j < n; ++j) {
      // (Cell centres: never on the face's parameter bounds.)
      const double u = u0 + (u1 - u0) * (i + 0.5) / n;
      const double v = v0 + (v1 - v0) * (j + 0.5) / n;
      if (inside.Perform(gp_Pnt2d(u, v)) == TopAbs_IN) {
        points.push_back({surface.Value(u, v), outward_normal(surface, face, u, v)});
      }
    }
  }
  return points;
}

// Which side of a closed body's boundary a point lies on, as a ray from it
// first crosses the boundary (mitcad#1257).
enum class RaySide { Inside, Outside, Near };

// The side of `point` by the first crossing of the boundary (loaded into
// `boundary`) along a ray in `direction`: leaving the body there (along the
// face's outward normal), the point lies inside it; entering it, outside.
// Near where the boundary lies within `close` of the point along the ray:
// the point is then no farther than that from it on either side. None where
// the ray meets nothing up to `reach`, or meets the boundary at a tangent
// or on a face's edge: a solid classifier tells then. (A classifier's rays
// run through the whole body: on a body of long spline faces, a thread's,
// it took 60 ms a point in a debug build, 40 times as long as the rounding
// it checked; a short ray meets only the faces near the point.)
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
  const TopoDS_Face& face = boundary.Face(first);
  const BRepAdaptor_Surface surface(face, false);
  const std::optional<gp_Dir> normal =
      outward_normal(surface, face, boundary.UParameter(first), boundary.VParameter(first));
  if (!normal) {
    return std::nullopt;
  }
  const double cosine = normal->Dot(direction);
  // (Grazing: too near a tangent to tell.)
  if (std::abs(cosine) < kTransverse) {
    return std::nullopt;
  }
  return cosine > 0.0 ? RaySide::Inside : RaySide::Outside;
}

std::string millimetres(double value) {
  char text[32];
  std::snprintf(text, sizeof text, "%.2g", value);
  return text;
}

// The worst made face of a kind found, how far it reaches, how many faces
// are of the kind, and the input's face it reaches past.
struct Outcome {
  int worst = -1;
  double reach = 0.0;
  int faces = 0;
  int beside = -1;
};

// The edge a made face is named after (<feature>:<role>(E{…}), a piece's
// "#k" left out); empty for a corner or another name.
std::string own_edge(const std::string& name, const std::string& feature, const char* role) {
  const std::string own = feature + ":" + role + "(";
  if (name.compare(0, own.size(), own) != 0) {
    return {};
  }
  std::string text = name.substr(own.size());
  const std::size_t hash = text.rfind(")#");
  if (hash != std::string::npos) {
    text.erase(hash + 1);
  }
  if (!text.empty() && text.back() == ')') {
    text.pop_back();
  }
  return text.compare(0, 2, "E{") == 0 ? text : std::string();
}

// Whether the face's surface is a plane, cylinder, cone, sphere or torus.
bool elementary(const TopoDS_Face& face) {
  switch (BRepAdaptor_Surface(face, false).GetType()) {
  case GeomAbs_Plane:
  case GeomAbs_Cylinder:
  case GeomAbs_Cone:
  case GeomAbs_Sphere:
  case GeomAbs_Torus:
    return true;
  default:
    return false;
  }
}

// Whether a point of a made face of a concave edge lies over the input's
// face `face` beside the edge (mitcad#1260): a ray from it to its foot on
// the face's surface meets the input's boundary (`boundary`, loaded into
// `rays`) by the foot. None where it does, or where the point lies on that
// surface; else how far the foot lies from the input (the made face hangs
// out past the body's original extent there).
std::optional<double> past_extent(IntCurvesFace_ShapeIntersector& rays, const TopoDS_Shape& boundary,
                                  const gp_Pnt& point, const TopoDS_Face& face, double allowed) {
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face);
  if (surface.IsNull()) {
    return std::nullopt;
  }
  GeomAPI_ProjectPointOnSurf foot(point, surface);
  if (foot.NbPoints() == 0 || foot.LowerDistance() <= allowed) {
    return std::nullopt;
  }
  const gp_Pnt on = foot.NearestPoint();
  rays.Perform(gp_Lin(point, gp_Dir(gp_Vec(point, on))), 0.0, foot.LowerDistance() + allowed);
  if (rays.IsDone() && rays.NbPnt() > 0) {
    return std::nullopt;
  }
  BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(on).Vertex(), boundary);
  return distance.IsDone() ? std::optional<double>(distance.Value()) : std::nullopt;
}

// The faces of a dressup's result of concave edges that lie on the surface
// of the input's face they are named after but face the other way
// (mitcad#1260: the underside of a chamfer hanging out past a narrow face,
// named after that face): "face F… is turned over …" for the first, with
// how many more; empty for none. Made faces (`made`) and faces of the
// input kept as they are are not looked at.
std::string turned_over(const Shape& input, const Shape& result, const std::vector<int>& made, const char* role) {
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> kept;
  std::map<std::string, int> by_name;
  for (int f = 0; f < input.face_count(); ++f) {
    kept.Add(input.face(f));
    for (const std::string& name : input.face_names(f)) {
      by_name.emplace(name, f);
    }
  }
  std::string first;
  int turned = 0;
  for (int f = 0; f < result.face_count(); ++f) {
    const NameList& names = result.face_names(f);
    if (names.empty() || kept.Contains(result.face(f)) || std::find(made.begin(), made.end(), f) != made.end()) {
      continue;
    }
    const auto named = by_name.find(names.front());
    if (named == by_name.end()) {
      continue;
    }
    const TopoDS_Face& face = result.face(f);
    const TopoDS_Face& original = input.face(named->second);
    const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(original);
    if (surface.IsNull()) {
      continue;
    }
    const double tolerance = std::max(1.0e-6, 2.0 * std::max(face_tolerance(face), face_tolerance(original)));
    for (const FacePoint& sample : face_points(face, 9)) {
      if (!sample.normal) {
        continue;
      }
      GeomAPI_ProjectPointOnSurf foot(sample.point, surface);
      if (foot.NbPoints() == 0 || foot.LowerDistance() > tolerance) {
        break; // (Not on the surface it is named after.)
      }
      double u = 0.0;
      double v = 0.0;
      foot.LowerDistanceParameters(u, v);
      const std::optional<gp_Dir> normal = outward_normal(BRepAdaptor_Surface(original, false), original, u, v);
      if (normal && normal->Dot(*sample.normal) < -0.5) {
        if (turned++ == 0) {
          first = "face " + names.front() + " is turned over on its surface (it faces into the body), where the " +
                  role + " hangs out past it";
        }
      }
      break;
    }
  }
  if (turned > 1) {
    first += " (and " + std::to_string(turned - 1) + " more " + (turned == 2 ? "face" : "faces") + ")";
  }
  return first;
}

} // namespace

std::string made_face(const std::string& name, const std::string& feature, const char* role, bool vertex_word) {
  const std::string own = feature + ":" + role + "(";
  const std::string corner = feature + ":corner(";
  const auto inner = [&](std::size_t start) {
    std::string text = name.substr(start);
    // (A piece's "#k" and the closing bracket.)
    const std::size_t hash = text.rfind(")#");
    if (hash != std::string::npos) {
      text.erase(hash + 1);
    }
    if (!text.empty() && text.back() == ')') {
      text.pop_back();
    }
    return text;
  };
  if (name.compare(0, own.size(), own) == 0) {
    return std::string("the ") + role + " of edge " + inner(own.size());
  }
  if (name.compare(0, corner.size(), corner) == 0) {
    return std::string("the corner of the ") + role + (vertex_word ? " at vertex " : " at ") + inner(corner.size());
  }
  if (name.compare(0, feature.size() + 1, feature + ":") == 0) {
    return "face " + name + " of the " + role;
  }
  return "face " + name;
}

std::optional<DressupSide> dressup_side(const Shape& body, const std::vector<int>& owner) {
  std::optional<DressupSide> side;
  try {
    OCC_CATCH_SIGNALS
    for (int e = 0; e < body.edge_count() && e < static_cast<int>(owner.size()); ++e) {
      if (owner[static_cast<std::size_t>(e)] < 0) {
        continue;
      }
      const std::array<int, 2> faces = body.edge_faces(e);
      if (faces[0] < 0 || faces[1] < 0) {
        return std::nullopt;
      }
      const ChFiDS_TypeOfConcavity type = ChFi3d::DefineConnectType(TopoDS::Edge(body.edge(e)), body.face(faces[0]),
                                                                    body.face(faces[1]), 1.0e-4, false);
      if (type != ChFiDS_Convex && type != ChFiDS_Concave) {
        return std::nullopt;
      }
      const DressupSide here = type == ChFiDS_Convex ? DressupSide::Within : DressupSide::Outside;
      if (side && *side != here) {
        return std::nullopt;
      }
      side = here;
    }
  } catch (const Standard_Failure&) {
    return std::nullopt;
  }
  return side;
}

std::string dressup_warning(const Shape& input, const Shape& result, const std::string& feature, const char* role,
                            DressupSide side, double size) {
  if (!(size > 0.0)) {
    return {};
  }
  try {
    OCC_CATCH_SIGNALS
    if (!closed_solids(input.occt())) {
      return {};
    }
    const std::string prefix = feature + ":";
    std::vector<int> made;
    for (int f = 0; f < result.face_count(); ++f) {
      const NameList& names = result.face_names(f);
      if (std::any_of(names.begin(), names.end(),
                      [&](const std::string& name) { return name.compare(0, prefix.size(), prefix) == 0; })) {
        made.push_back(f);
      }
    }
    if (made.empty()) {
      return {};
    }
    // The corners first: where a rounding goes wrong, it is mostly there.
    const std::string corner = feature + ":corner(";
    std::stable_partition(made.begin(), made.end(), [&](int f) {
      const NameList& names = result.face_names(f);
      return !names.empty() && names.front().compare(0, corner.size(), corner) == 0;
    });
    // The rays first; the solids' classifiers only where a ray cannot tell.
    IntCurvesFace_ShapeIntersector rays;
    rays.Load(input.occt(), Precision::Confusion());
    std::vector<std::unique_ptr<BRepClass3d_SolidClassifier>> solids;
    const auto classified_wrong = [&](const gp_Pnt& point) {
      if (solids.empty()) {
        for (TopExp_Explorer solid(input.occt(), TopAbs_SOLID); solid.More(); solid.Next()) {
          solids.push_back(std::make_unique<BRepClass3d_SolidClassifier>(solid.Current()));
        }
      }
      // On the wrong side of every solid (outside each), or of one (inside).
      for (const auto& classifier : solids) {
        classifier->Perform(point, Precision::Confusion());
        const TopAbs_State state = classifier->State();
        if (side == DressupSide::Within && state != TopAbs_OUT) {
          return false;
        }
        if (side == DressupSide::Outside && state == TopAbs_IN) {
          return true;
        }
      }
      return side == DressupSide::Within;
    };
    // (Distances to the input's faces: to a solid, a point inside it is at
    // no distance.)
    BRep_Builder builder;
    TopoDS_Compound boundary;
    builder.MakeCompound(boundary);
    for (int f = 0; f < input.face_count(); ++f) {
      builder.Add(boundary, input.face(f));
    }
    const int per_face = std::clamp(kPoints / static_cast<int>(made.size()), 1, kFacePoints);
    int distances = 0;
    int worst = -1;
    double worst_reach = 0.0;
    int faces_out = 0;
    // Of concave edges (mitcad#1260): made faces hanging out past the
    // body's original extent, and faces of the input turned over.
    Outcome past;
    for (const int f : made) {
      throw_if_cancelled();
      const TopoDS_Face& face = result.face(f);
      // (Within the face's tolerances, but never by more than a twentieth of
      // the size: a corner OCCT gave tolerances wider than its rounding,
      // 1.8 mm on one of 1 mm, is no excuse.)
      const double allowed = std::max(kReach * size, std::min(2.0 * face_tolerance(face), kTolerated * size));
      double deepest = 0.0;
      int measured = 0;
      // The faces of the made face's edge in the input, where it has one:
      // a dressup of a concave edge fills the corner between them, every
      // point of it over both faces. (Elementary surfaces only, whose feet
      // are found at once; a B-spline face's would cost a search a point.)
      std::vector<int> beside;
      if (side == DressupSide::Outside && !result.face_names(f).empty()) {
        const std::string edge = own_edge(result.face_names(f).front(), feature, role);
        if (!edge.empty()) {
          const std::vector<int> named_edges = input.find_edges(edge);
          if (named_edges.size() == 1) {
            for (const int b : input.edge_faces(named_edges.front())) {
              if (b >= 0 && elementary(input.face(b))) {
                beside.push_back(b);
              }
            }
          }
        }
      }
      double farthest_past = 0.0;
      int past_measured = 0;
      int past_face = -1;
      for (const FacePoint& sample : face_points(face, per_face)) {
        const gp_Pnt& point = sample.point;
        for (const int b : beside) {
          if (distances >= kDistances || past_measured >= kFaceDistances) {
            break;
          }
          const std::optional<double> out = past_extent(rays, boundary, point, input.face(b), allowed);
          if (out) {
            ++distances;
            ++past_measured;
            if (*out > allowed && *out > farthest_past) {
              farthest_past = *out;
              past_face = b;
            }
          }
        }
        // (The ray into what a rounding of convex edges takes away, or out
        // of what one of concave edges adds: the input's boundary is near
        // that way.)
        std::optional<RaySide> told;
        if (sample.normal) {
          told = ray_side(rays, point, side == DressupSide::Within ? *sample.normal : sample.normal->Reversed(),
                          allowed, kRayReach * size);
        }
        const bool wrong = told ? *told == (side == DressupSide::Within ? RaySide::Outside : RaySide::Inside)
                                : classified_wrong(point);
        if (!wrong || distances >= kDistances || measured >= kFaceDistances) {
          continue;
        }
        ++distances;
        ++measured;
        BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(point).Vertex(), boundary);
        if (distance.IsDone() && distance.Value() > allowed) {
          deepest = std::max(deepest, distance.Value());
        }
      }
      if (deepest > 0.0) {
        ++faces_out;
        if (deepest > worst_reach) {
          worst_reach = deepest;
          worst = f;
        }
      }
      if (farthest_past > 0.0) {
        ++past.faces;
        if (farthest_past > past.reach) {
          past.reach = farthest_past;
          past.worst = f;
          past.beside = past_face;
        }
      }
    }
    const auto more = [](int faces) {
      return faces > 1 ? " (and on " + std::to_string(faces - 1) + " more " + (faces == 2 ? "face" : "faces") + ")"
                       : std::string();
    };
    std::vector<std::string> found;
    if (worst >= 0) {
      const NameList& names = result.face_names(worst);
      found.push_back(made_face(names.empty() ? std::string("?") : names.front(), feature, role, true) + " reaches " +
                      millimetres(worst_reach) + " mm " +
                      (side == DressupSide::Within ? "out of the body, where a " : "into the body, where a ") + role +
                      (side == DressupSide::Within ? " of convex edges only takes material away"
                                                   : " of concave edges only adds material") +
                      ": its surface is folded or spiked there" + more(faces_out));
    }
    if (past.worst >= 0) {
      const NameList& names = result.face_names(past.worst);
      const NameList& beside = input.face_names(past.beside);
      found.push_back(made_face(names.empty() ? std::string("?") : names.front(), feature, role, true) + " reaches " +
                      millimetres(past.reach) + " mm past the body's original extent, where a " + role +
                      " of concave edges only fills the corner between its faces: it hangs out past the edge of " +
                      "face " + (beside.empty() ? std::string("?") : beside.front()) + more(past.faces));
    }
    if (side == DressupSide::Outside) {
      const std::string turned = turned_over(input, result, made, role);
      if (!turned.empty()) {
        found.push_back(turned);
      }
    }
    if (found.empty()) {
      return {};
    }
    std::string text = found.front();
    for (std::size_t k = 1; k < found.size(); ++k) {
      text += "; " + found[k];
    }
    return text + "; check the result";
  } catch (const Standard_Failure&) {
    return {};
  }
}

} // namespace mitcad::geometry::detail
