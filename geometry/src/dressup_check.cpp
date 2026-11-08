// SPDX-License-Identifier: MIT
#include "dressup_check.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdio>
#include <map>
#include <memory>
#include <optional>
#include <set>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepGProp.hxx>
#include <BRepTools.hxx>
#include <GProp_GProps.hxx>
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
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp.hxx>
#include <gp_Dir.hxx>
#include <gp_Lin.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>

#include "face_select.hpp"
#include "util.hpp"

namespace mitcad::geometry::detail {
namespace {

using Place = Shape::NotePlace;

// Points looked at on all the made faces together, and at most on one.
constexpr int kPoints = 600;
constexpr int kFacePoints = 49;
// Distances measured to the input for points found on the wrong side, on
// all the made faces together and at most on one.
constexpr int kDistances = 60;
constexpr int kFaceDistances = 6;
// How far to the wrong side counts, of the dressup's size (mitcad#1327:
// 1 % before, which a three-fillet corner's 1.9 % tripped).
constexpr double kReach = kDressupReach;
// How far the made faces' tolerances may widen that at most, of the size,
// and a corner patch's (a made face beside three or more dressup faces,
// whose surface OCCT approximates within its tolerances).
constexpr double kTolerated = 0.05;
constexpr double kPatchTolerated = 0.25;
// A face a ring's dressup leaves of a face it ran over, smaller across
// than this of the size, is a sliver (the area of a square that wide).
constexpr double kSliver = 0.01;
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

// The worst made face of a kind found, how far it reaches and where, how
// many faces are of the kind, and the input's face it reaches past.
struct Outcome {
  int worst = -1;
  double reach = 0.0;
  gp_Pnt at;
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

// The names of the two faces in an edge's name E{a|b} (split at its bar
// outside brackets); none for another name.
std::vector<std::string> named_faces_of_edge(const std::string& edge) {
  if (edge.size() < 4 || edge.compare(0, 2, "E{") != 0 || edge.back() != '}') {
    return {};
  }
  int depth = 0;
  for (std::size_t i = 2; i + 1 < edge.size(); ++i) {
    const char c = edge[i];
    if (c == '{' || c == '(' || c == '[') {
      ++depth;
    } else if (c == '}' || c == ')' || c == ']') {
      --depth;
    } else if (c == '|' && depth == 0) {
      return {edge.substr(2, i - 2), edge.substr(i + 1, edge.size() - i - 2)};
    }
  }
  return {};
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

// Whether a face's name is a dressup's: <feature>:fillet(…), :chamfer(…)
// or :corner(…), of any feature.
bool dressup_name(const std::string& name) {
  const std::size_t colon = name.find(':');
  if (colon == std::string::npos || name.find('(') < colon) {
    return false;
  }
  for (const char* role : {"fillet(", "chamfer(", "corner("}) {
    if (name.compare(colon + 1, std::char_traits<char>::length(role), role) == 0) {
      return true;
    }
  }
  return false;
}

// Whether a face of the result lies beside three or more faces of
// dressups (this one's or earlier ones'): a corner patch where they meet,
// whose surface OCCT approximates (mitcad#1327).
bool corner_patch(const Shape& result, int f) {
  std::set<int> beside;
  for (TopExp_Explorer e(result.face(f), TopAbs_EDGE); e.More(); e.Next()) {
    const int edge = result.edge_index(e.Current());
    if (edge < 0) {
      continue;
    }
    for (const int other : result.edge_faces(edge)) {
      if (other >= 0 && other != f && !result.face_names(other).empty() &&
          dressup_name(result.face_names(other).front())) {
        beside.insert(other);
      }
    }
  }
  return beside.size() >= 3;
}

// The foot of a made face's point on the surface of the input's face
// `face`, and whether the point lies over that face: a ray from it to the
// foot meets the input's boundary (loaded into `rays`) by the foot, or the
// point lies on the surface. None where no foot is found.
struct Foot {
  gp_Pnt at;
  bool over = true;
};
std::optional<Foot> foot_on(IntCurvesFace_ShapeIntersector& rays, const gp_Pnt& point, const TopoDS_Face& face,
                            double allowed) {
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face);
  if (surface.IsNull()) {
    return std::nullopt;
  }
  GeomAPI_ProjectPointOnSurf projected(point, surface);
  if (projected.NbPoints() == 0) {
    return std::nullopt;
  }
  Foot foot;
  foot.at = projected.NearestPoint();
  if (projected.LowerDistance() <= allowed) {
    return foot;
  }
  rays.Perform(gp_Lin(point, gp_Dir(gp_Vec(point, foot.at))), 0.0, projected.LowerDistance() + allowed);
  foot.over = rays.IsDone() && rays.NbPnt() > 0;
  return foot;
}

// The input's faces that continue face `face` smoothly across one of its
// edges (a rounding of that edge, tangent to it): a dressup of a tangent
// chain runs on over them where it leaves the face (mitcad#1327).
std::vector<int> smooth_neighbours(const Shape& input, int face) {
  std::vector<int> found;
  for (TopExp_Explorer e(input.face(face), TopAbs_EDGE); e.More(); e.Next()) {
    const int edge = input.edge_index(e.Current());
    if (edge < 0) {
      continue;
    }
    for (const int other : input.edge_faces(edge)) {
      if (other < 0 || other == face || std::find(found.begin(), found.end(), other) != found.end()) {
        continue;
      }
      try {
        if (is_smooth(input.edge(edge), input.face(face), input.face(other))) {
          found.push_back(other);
        }
      } catch (const std::runtime_error&) {
        // (No normal there: not told smooth.)
      }
    }
  }
  return found;
}

// The convex edges of the input's face `face` past which the rounding of
// its concave edge `own` meets the rounding of another edge the dressup
// rounds (`rounded`) in a mitre (mitcad#1458): an edge of the face from a
// vertex of `own` whose other face holds a rounded edge from that vertex.
// Three foot edges of a block standing on a plate, two of them meeting the
// block's vertical corners: each rounding runs on past the corner, outside
// the faces whose corner it fills, until it meets the other along the
// mitre that continues the corner's edge down to the plate. What lies past
// such an edge there is the two roundings' meeting, which fills the corner
// outside the convex edge as it fills those beside it; it hangs out past
// nothing.
std::vector<int> mitre_edges(const Shape& input, int face, int own, const std::set<int>& rounded) {
  std::vector<int> found;
  TopoDS_Vertex own_first;
  TopoDS_Vertex own_last;
  TopExp::Vertices(input.edge(own), own_first, own_last);
  for (TopExp_Explorer e(input.face(face), TopAbs_EDGE); e.More(); e.Next()) {
    const int edge = input.edge_index(e.Current());
    if (edge < 0 || edge == own || std::find(found.begin(), found.end(), edge) != found.end()) {
      continue;
    }
    const std::array<int, 2> faces = input.edge_faces(edge);
    const int other = faces[0] == face ? faces[1] : faces[0];
    if (other < 0 || other == face) {
      continue;
    }
    // The vertex it shares with `own`.
    TopoDS_Vertex first;
    TopoDS_Vertex last;
    TopExp::Vertices(input.edge(edge), first, last);
    TopoDS_Vertex corner;
    for (const TopoDS_Vertex& end : {own_first, own_last}) {
      if (!end.IsNull() && ((!first.IsNull() && end.IsSame(first)) || (!last.IsNull() && end.IsSame(last)))) {
        corner = end;
      }
    }
    if (corner.IsNull() || ChFi3d::DefineConnectType(input.edge(edge), input.face(face), input.face(other), 1.0e-4,
                                                      false) != ChFiDS_Convex) {
      continue;
    }
    for (const int r : rounded) {
      const std::array<int, 2> beside = input.edge_faces(r);
      if (r == own || (beside[0] != other && beside[1] != other)) {
        continue;
      }
      TopoDS_Vertex r_first;
      TopoDS_Vertex r_last;
      TopExp::Vertices(input.edge(r), r_first, r_last);
      if ((!r_first.IsNull() && corner.IsSame(r_first)) || (!r_last.IsNull() && corner.IsSame(r_last))) {
        found.push_back(edge);
        break;
      }
    }
  }
  return found;
}

// A face of `shape` as a place, by its first name (none without one).
void add_face(std::vector<Place>& places, const Shape& shape, int f) {
  if (f >= 0 && f < shape.face_count() && !shape.face_names(f).empty()) {
    places.push_back(Place::of(Place::Kind::Face, shape.face_names(f).front()));
  }
}

void add_point(std::vector<Place>& places, const gp_Pnt& point) {
  places.push_back(Place::at(point.X(), point.Y(), point.Z()));
}

// A made face as the subject of a warning's sentence, without its name:
// "The corner of the fillet", "The fillet", "A face of the fillet".
std::string subject(const std::string& name, const std::string& feature, const char* role) {
  const std::string corner = feature + ":corner(";
  const std::string own = feature + ":" + role + "(";
  if (name.compare(0, corner.size(), corner) == 0) {
    return std::string("The corner of the ") + role;
  }
  if (name.compare(0, own.size(), own) == 0) {
    return std::string("The ") + role;
  }
  return std::string("A face of the ") + role;
}

// " (and on 2 more faces)" for three faces; nothing for one.
std::string more_faces(int faces) {
  return faces > 1 ? " (and on " + std::to_string(faces - 1) + " more " + (faces == 2 ? "face" : "faces") + ")"
                   : std::string();
}

double face_area(const TopoDS_Face& face) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(face, props);
  return props.Mass();
}

// The faces of a dressup's result of concave edges that lie on the surface
// of the input's face they are named after but face the other way
// (mitcad#1260: the underside of a chamfer hanging out past a narrow face,
// named after that face): "A face beside the chamfer is turned over on its
// surface …" for the first, with how many more, its places the face and a
// point of it; empty for none. Made faces (`made`) and faces of the input
// kept as they are are not looked at.
DressupFinding turned_over(const Shape& input, const Shape& result, const std::vector<int>& made, const char* role) {
  NCollection_Map<TopoDS_Shape, TopTools_ShapeMapHasher> kept;
  std::map<std::string, int> by_name;
  for (int f = 0; f < input.face_count(); ++f) {
    kept.Add(input.face(f));
    for (const std::string& name : input.face_names(f)) {
      by_name.emplace(name, f);
    }
  }
  int first = -1;
  gp_Pnt at;
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
          first = f;
          at = sample.point;
        }
      }
      break;
    }
  }
  DressupFinding found;
  if (turned == 0) {
    return found;
  }
  found.text = std::string("A face beside the ") + role +
               " is turned over on its surface (it faces into the body), where the " + role + " hangs out past it";
  if (turned > 1) {
    found.text += " (and " + std::to_string(turned - 1) + " more " + (turned == 2 ? "face" : "faces") + ")";
  }
  found.text += ".";
  add_face(found.places, result, first);
  add_point(found.places, at);
  return found;
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


DressupFinding dressup_check(const Shape& input, const Shape& result, const std::string& feature, const char* role,
                             DressupSide side, double size) {
  DressupFinding none;
  if (!(size > 0.0)) {
    return none;
  }
  try {
    OCC_CATCH_SIGNALS
    if (!closed_solids(input.occt())) {
      return none;
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
      return none;
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
    // Made faces reaching to the wrong side of the body; of concave edges
    // (mitcad#1260) also made faces hanging out past the body's original
    // extent, and faces of the input turned over.
    Outcome wrong;
    Outcome past;
    // The input's faces that continue each face beside an edge smoothly.
    std::map<int, std::vector<int>> smooth;
    // Of concave edges: the input's edges the made faces are named after,
    // and by an edge and a face beside it the face's edges past which the
    // edge's rounding meets another in a mitre (mitre_edges, mitcad#1458).
    std::set<int> rounded;
    if (side == DressupSide::Outside) {
      for (const int f : made) {
        const NameList& names = result.face_names(f);
        const std::string edge = names.empty() ? std::string() : own_edge(names.front(), feature, role);
        if (!edge.empty()) {
          const std::vector<int> named_edges = input.find_edges(edge);
          if (named_edges.size() == 1) {
            rounded.insert(named_edges.front());
          }
        }
      }
    }
    std::map<std::pair<int, int>, std::vector<int>> mitres;
    for (const int f : made) {
      throw_if_cancelled();
      const TopoDS_Face& face = result.face(f);
      // (Within the face's tolerances, but never by more than a twentieth of
      // the size: a corner OCCT gave tolerances wider than its rounding,
      // 1.8 mm on one of 1 mm, is no excuse. A corner patch where three or
      // more dressup faces meet, whose surface OCCT approximates, up to a
      // quarter of it, mitcad#1327.)
      const double tolerated = corner_patch(result, f) ? kPatchTolerated : kTolerated;
      const double allowed = std::max(kReach * size, std::min(2.0 * face_tolerance(face), tolerated * size));
      double deepest = 0.0;
      gp_Pnt deepest_at;
      int measured = 0;
      // The faces of the made face's edge in the input, where it has one:
      // a dressup of a concave edge fills the corner between them, every
      // point of it over both faces. (Elementary surfaces only, whose feet
      // are found at once; a B-spline face's would cost a search a point.)
      std::vector<int> beside;
      int own = -1;
      if (side == DressupSide::Outside && !result.face_names(f).empty()) {
        const std::string edge = own_edge(result.face_names(f).front(), feature, role);
        if (!edge.empty()) {
          const std::vector<int> named_edges = input.find_edges(edge);
          if (named_edges.size() == 1) {
            own = named_edges.front();
            for (const int b : input.edge_faces(named_edges.front())) {
              if (b >= 0 && elementary(input.face(b))) {
                beside.push_back(b);
              }
            }
          }
        }
      }
      double farthest_past = 0.0;
      gp_Pnt past_at;
      int past_measured = 0;
      int past_face = -1;
      for (const FacePoint& sample : face_points(face, per_face)) {
        const gp_Pnt& point = sample.point;
        for (const int b : beside) {
          if (distances >= kDistances || past_measured >= kFaceDistances) {
            break;
          }
          const std::optional<Foot> foot = foot_on(rays, point, input.face(b), allowed);
          if (!foot || foot->over) {
            continue;
          }
          // Over a face that continues it smoothly (a rounding of its edge)
          // the dressup of a tangent chain runs on: not past the body's
          // extent (mitcad#1327: a fillet's contact running from a face onto
          // the rounding of its edge, 0.057 mm off the face's surface).
          auto neighbours = smooth.find(b);
          if (neighbours == smooth.end()) {
            neighbours = smooth.emplace(b, smooth_neighbours(input, b)).first;
          }
          const bool continued =
              std::any_of(neighbours->second.begin(), neighbours->second.end(), [&](int n) {
                const std::optional<Foot> on = foot_on(rays, point, input.face(n), allowed);
                return on && on->over;
              });
          if (continued) {
            continue;
          }
          // Past a convex edge of the face where the rounding meets the
          // rounding of another of the edges in a mitre (mitcad#1458): the
          // foot no farther from that edge than from the face.
          auto mitre = mitres.find({own, b});
          if (mitre == mitres.end()) {
            mitre = mitres.emplace(std::make_pair(own, b), own >= 0 ? mitre_edges(input, b, own, rounded)
                                                                    : std::vector<int>())
                        .first;
          }
          if (!mitre->second.empty()) {
            const TopoDS_Vertex foot_vertex = BRepBuilderAPI_MakeVertex(foot->at).Vertex();
            BRepExtrema_DistShapeShape to_face(foot_vertex, input.face(b));
            const bool mitred =
                to_face.IsDone() &&
                std::any_of(mitre->second.begin(), mitre->second.end(), [&](int e) {
                  BRepExtrema_DistShapeShape to_edge(foot_vertex, input.edge(e));
                  return to_edge.IsDone() && to_edge.Value() <= to_face.Value() + allowed;
                });
            if (mitred) {
              continue;
            }
          }
          ++distances;
          ++past_measured;
          BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(foot->at).Vertex(), boundary);
          if (distance.IsDone() && distance.Value() > allowed && distance.Value() > farthest_past) {
            farthest_past = distance.Value();
            past_face = b;
            past_at = point;
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
        const bool on_wrong_side = told ? *told == (side == DressupSide::Within ? RaySide::Outside : RaySide::Inside)
                                        : classified_wrong(point);
        if (!on_wrong_side || distances >= kDistances || measured >= kFaceDistances) {
          continue;
        }
        ++distances;
        ++measured;
        BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(point).Vertex(), boundary);
        if (distance.IsDone() && distance.Value() > allowed && distance.Value() > deepest) {
          deepest = distance.Value();
          deepest_at = point;
        }
      }
      if (deepest > 0.0) {
        ++wrong.faces;
        if (deepest > wrong.reach) {
          wrong.reach = deepest;
          wrong.worst = f;
          wrong.at = deepest_at;
        }
      }
      if (farthest_past > 0.0) {
        ++past.faces;
        if (farthest_past > past.reach) {
          past.reach = farthest_past;
          past.worst = f;
          past.at = past_at;
          past.beside = past_face;
        }
      }
    }
    const auto name_of = [&](int f) {
      const NameList& names = result.face_names(f);
      return names.empty() ? std::string() : names.front();
    };
    DressupFinding found;
    std::vector<std::string> sentences;
    if (wrong.worst >= 0) {
      sentences.push_back(subject(name_of(wrong.worst), feature, role) + " reaches " + millimetres(wrong.reach) +
                          " mm " + (side == DressupSide::Within ? "out of the body, where a " : "into the body, where a ") +
                          role +
                          (side == DressupSide::Within ? " of convex edges only takes material away"
                                                       : " of concave edges only adds material") +
                          ": it folds back on itself or ends in a spike there" + more_faces(wrong.faces) + ".");
      add_face(found.places, result, wrong.worst);
      add_point(found.places, wrong.at);
    }
    if (past.worst >= 0) {
      sentences.push_back(subject(name_of(past.worst), feature, role) + " hangs " + millimetres(past.reach) +
                          " mm past the edge of the face beside it, where a " + role +
                          " of concave edges only fills the corner between its faces" + more_faces(past.faces) + ".");
      add_face(found.places, result, past.worst);
      add_face(found.places, input, past.beside);
      add_point(found.places, past.at);
    }
    if (side == DressupSide::Outside) {
      DressupFinding turned = turned_over(input, result, made, role);
      if (!turned.empty()) {
        sentences.push_back(turned.text);
        found.places.insert(found.places.end(), turned.places.begin(), turned.places.end());
      }
    }
    if (sentences.empty()) {
      return none;
    }
    for (const std::string& sentence : sentences) {
      found.text += (found.text.empty() ? "" : " ") + sentence;
    }
    found.text += " Check the result.";
    return found;
  } catch (const Standard_Failure&) {
    return none;
  }
}

std::string dressup_warning(const Shape& input, const Shape& result, const std::string& feature, const char* role,
                            DressupSide side, double size) {
  return dressup_check(input, result, feature, role, side, size).text;
}

DressupFinding ring_check(const Shape& input, const Shape& result, const std::string& feature, const char* role,
                          double size) {
  DressupFinding none;
  if (!(size > 0.0)) {
    return none;
  }
  try {
    OCC_CATCH_SIGNALS
    const std::string prefix = feature + ":";
    const auto made = [&](int f) {
      const NameList& names = result.face_names(f);
      return !names.empty() && names.front().compare(0, prefix.size(), prefix) == 0;
    };
    // A rounding meets the faces beside it tangentially; a bevel at an
    // angle, which is no edge left.
    const bool rounding = std::string(role) == "fillet";
    const double least = kSliver * size * kSliver * size;
    double sharpest = 0.0;
    int sharp_edge = -1;
    int sharp_made = -1;
    int sharp_face = -1;
    int sharp_edges = 0;
    double smallest = 0.0;
    int sliver = -1;
    int sliver_made = -1;
    int slivers = 0;
    std::set<int> looked;
    for (int e = 0; e < result.edge_count(); ++e) {
      throw_if_cancelled();
      const std::array<int, 2> faces = result.edge_faces(e);
      if (faces[0] < 0 || faces[1] < 0 || made(faces[0]) == made(faces[1])) {
        continue;
      }
      const int m = made(faces[0]) ? faces[0] : faces[1];
      const int other = m == faces[0] ? faces[1] : faces[0];
      const NameList& names = result.face_names(other);
      if (names.empty()) {
        continue;
      }
      // (The two faces of the made face's own edge: the ring rounds the
      // edge between them, meeting them as it should.)
      const std::vector<std::string> own =
          named_faces_of_edge(own_edge(result.face_names(m).front(), feature, role));
      if (std::find(own.begin(), own.end(), names.front()) != own.end()) {
        continue;
      }
      // A face it ran over: met at an angle, a rounding leaves an edge.
      if (rounding && !BRep_Tool::Degenerated(result.edge(e))) {
        double angle = 0.0;
        try {
          for (const double t : {0.1, 0.5, 0.9}) {
            angle = std::max(angle, normal_angle(result.edge(e), result.face(m), result.face(other), t));
          }
        } catch (const std::runtime_error&) {
          angle = 0.0; // (No normal there: not told.)
        }
        if (angle > kRingEdgeAngle) {
          ++sharp_edges;
          if (angle > sharpest) {
            sharpest = angle;
            sharp_edge = e;
            sharp_made = m;
            sharp_face = other;
          }
        }
      }
      // And cut down to a sliver, a notch is left of it. (A face that was
      // as small in the input is none the ring left.)
      if (looked.insert(other).second) {
        const double area = face_area(result.face(other));
        if (area < least) {
          double before = 0.0;
          for (const int k : input.find_faces(names.front())) {
            before = std::max(before, face_area(input.face(k)));
          }
          if (before > least) {
            ++slivers;
            if (sliver < 0 || area < smallest) {
              smallest = area;
              sliver = other;
              sliver_made = m;
            }
          }
        }
      }
    }
    std::vector<std::string> sentences;
    DressupFinding found;
    if (sharp_edge >= 0) {
      char degrees[16];
      std::snprintf(degrees, sizeof degrees, "%.0f", sharpest * 57.29577951308232);
      std::string text = std::string("The ") + role + " runs onto a face beside it and leaves an edge there, " +
                         degrees + " degrees between the faces";
      if (sharp_edges > 1) {
        text += " (and " + std::to_string(sharp_edges - 1) + " more " + (sharp_edges == 2 ? "edge" : "edges") + ")";
      }
      sentences.push_back(text + ".");
      add_face(found.places, result, sharp_made);
      add_face(found.places, result, sharp_face);
      if (!result.edge_name(sharp_edge).empty()) {
        found.places.push_back(Place::of(Place::Kind::Edge, result.edge_name(sharp_edge)));
      }
      const BRepAdaptor_Curve curve(result.edge(sharp_edge));
      add_point(found.places, curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter())));
    }
    if (sliver >= 0) {
      std::string text = std::string("The ") + role + " cuts a face it runs over down to a sliver: it leaves a notch there";
      if (slivers > 1) {
        text += " (and " + std::to_string(slivers - 1) + " more)";
      }
      sentences.push_back(text + ".");
      add_face(found.places, result, sliver);
      add_face(found.places, result, sliver_made);
      GProp_GProps props;
      BRepGProp::SurfaceProperties(result.face(sliver), props);
      add_point(found.places, props.CentreOfMass());
    }
    if (sentences.empty()) {
      return none;
    }
    for (const std::string& sentence : sentences) {
      found.text += (found.text.empty() ? "" : " ") + sentence;
    }
    found.text += " Check the result.";
    return found;
  } catch (const Standard_Failure&) {
    return none;
  }
}

} // namespace mitcad::geometry::detail
