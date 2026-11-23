// SPDX-License-Identifier: MIT
#include "full_round.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdio>
#include <limits>
#include <memory>
#include <numeric>
#include <optional>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRep_Tool.hxx>
#include <ChFi3d.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <Geom_Surface.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopAbs_Orientation.hxx>
#include <TopAbs_State.hxx>
#include <TopExp.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "face_select.hpp"
#include "mitcad/geometry/naming.hpp"
#include "mitcad/geometry/user_error.hpp"

namespace mitcad::geometry::detail {
namespace {

// Stations along each edge (relative parameters), clear of its ends where
// the side faces may end too.
constexpr std::array<double, 5> kStations = {{0.05, 0.25, 0.5, 0.75, 0.95}};
// Steps of taking the tangent planes again where the circle touches.
constexpr int kSteps = 60;

std::string face_name_of(const Shape& body, int face) {
  const NameList& names = body.face_names(face);
  return names.empty() ? std::string() : names.front();
}

std::string face_words(const Shape& body, int face) {
  const std::string name = face_name_of(body, face);
  return name.empty() ? "an unnamed face" : "face " + name;
}

std::string edge_words(const Shape& body, int edge) {
  const std::string& name = body.edge_name(edge);
  return name.empty() ? "an unnamed edge" : "edge " + name;
}

// The edges' and faces' places, for an error.
std::vector<ErrorPlace> places_of(const Shape& body, const std::vector<int>& edges, const std::vector<int>& faces) {
  std::vector<ErrorPlace> places;
  for (const int edge : edges) {
    places.push_back(edge_place(body.edge_name(edge)));
  }
  for (const int face : faces) {
    places.push_back(face_place(face_name_of(body, face)));
  }
  return places;
}

// The edges' names for an error's detail.
std::string edges_words(const Shape& body, const std::vector<int>& edges) {
  std::string words;
  for (std::size_t i = 0; i < edges.size(); ++i) {
    words += (i == 0 ? "" : i + 1 == edges.size() ? " and " : ", ") + edge_words(body, edges[i]);
  }
  return words;
}

[[noreturn]] void refuse(const std::string& said, const std::string& named, const std::vector<ErrorPlace>& places) {
  throw std::invalid_argument(placed_error(said, named, places));
}

bool share_vertex(const TopoDS_Edge& a, const TopoDS_Edge& b) {
  TopoDS_Vertex a1, a2, b1, b2;
  TopExp::Vertices(a, a1, a2);
  TopExp::Vertices(b, b1, b2);
  for (const TopoDS_Vertex& x : {a1, a2}) {
    for (const TopoDS_Vertex& y : {b1, b2}) {
      if (!x.IsNull() && !y.IsNull() && x.IsSame(y)) {
        return true;
      }
    }
  }
  return false;
}

// One of the three faces the circle touches: its surface, and on which
// side of it the circle lies (`side` +1 into the material, -1 out of it).
struct Touched {
  TopoDS_Face face;
  occ::handle<Geom_Surface> surface;
  double side = 1.0;
};

// The point of the face's surface nearest to `from`, and the normal there
// pointing to the side the circle lies on.
bool tangent_plane(const Touched& touched, const gp_Pnt& from, gp_Pnt& point, gp_Vec& normal) {
  GeomAPI_ProjectPointOnSurf projection(from, touched.surface);
  if (projection.NbPoints() == 0) {
    return false;
  }
  double u = 0.0;
  double v = 0.0;
  projection.LowerDistanceParameters(u, v);
  gp_Vec du;
  gp_Vec dv;
  touched.surface->D1(u, v, point, du, dv);
  gp_Vec out = du.Crossed(dv);
  if (out.Magnitude() < 1.0e-12) {
    return false;
  }
  out.Normalize();
  if (touched.face.Orientation() == TopAbs_REVERSED) {
    out.Reverse();
  }
  // (The outward normal reversed points into the material.)
  normal = out * -touched.side;
  return true;
}

double det3(const std::array<std::array<double, 3>, 3>& m) {
  return m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1]) - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0]) +
         m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]);
}

struct Circle {
  gp_Pnt center;
  double radius = 0.0;
  // Where it touches the middle face and the two side faces.
  std::array<gp_Pnt, 3> touches;
};

// The circle in the plane across the edge at `at` (its tangent `along`)
// that touches the three faces (the middle face first), each first taken
// at its point nearest to `start`; none where it does not settle.
std::optional<Circle> circle_at(const gp_Pnt& at, const gp_Vec& along, const std::array<Touched, 3>& faces,
                                const std::array<gp_Pnt, 3>& start) {
  // Two directions across the edge.
  const gp_Vec t = along.Normalized();
  const double ax = std::abs(t.X());
  const double ay = std::abs(t.Y());
  const double az = std::abs(t.Z());
  const gp_Vec axis = ax <= ay && ax <= az ? gp_Vec(1, 0, 0) : ay <= az ? gp_Vec(0, 1, 0) : gp_Vec(0, 0, 1);
  const gp_Vec u = t.Crossed(axis).Normalized();
  const gp_Vec v = t.Crossed(u).Normalized();
  std::array<gp_Pnt, 3> from = start;
  double last = std::numeric_limits<double>::quiet_NaN();
  for (int step = 0; step < kSteps; ++step) {
    std::array<gp_Pnt, 3> point;
    std::array<gp_Vec, 3> normal;
    for (std::size_t i = 0; i < 3; ++i) {
      if (!tangent_plane(faces[i], from[i], point[i], normal[i])) {
        return std::nullopt;
      }
    }
    // normal_i . (at + a u + b v - point_i) = r for each face.
    std::array<std::array<double, 3>, 3> m{};
    std::array<double, 3> rhs{};
    for (std::size_t i = 0; i < 3; ++i) {
      m[i] = {{normal[i].Dot(u), normal[i].Dot(v), -1.0}};
      rhs[i] = normal[i].Dot(gp_Vec(at, point[i]));
    }
    const double det = det3(m);
    if (!(std::abs(det) > 1.0e-12)) {
      return std::nullopt;
    }
    std::array<double, 3> x{};
    for (std::size_t k = 0; k < 3; ++k) {
      std::array<std::array<double, 3>, 3> mk = m;
      for (std::size_t i = 0; i < 3; ++i) {
        mk[i][k] = rhs[i];
      }
      x[k] = det3(mk) / det;
    }
    Circle circle;
    circle.radius = x[2];
    circle.center = at.Translated(u * x[0] + v * x[1]);
    double moved = 0.0;
    for (std::size_t i = 0; i < 3; ++i) {
      circle.touches[i] = circle.center.Translated(normal[i] * -circle.radius);
      moved = std::max(moved, circle.touches[i].Distance(point[i]));
    }
    if (!std::isfinite(circle.radius)) {
      return std::nullopt;
    }
    if (step > 0 && std::abs(circle.radius - last) < 1.0e-12 && moved < 1.0e-9) {
      return circle;
    }
    last = circle.radius;
    from = circle.touches;
  }
  return std::nullopt;
}

// The point of the face nearest to `p`.
std::optional<gp_Pnt> nearest_on(const TopoDS_Face& face, const gp_Pnt& p, double& distance) {
  BRepExtrema_DistShapeShape between(BRepBuilderAPI_MakeVertex(p).Vertex(), face);
  if (!between.IsDone() || between.NbSolution() == 0) {
    return std::nullopt;
  }
  distance = between.Value();
  return between.PointOnShape2(1);
}

} // namespace

FullRound full_round(const Shape& body, const FilletSet& set) {
  if (!set.faces.empty()) {
    std::vector<ErrorPlace> places;
    for (const std::string& face : set.faces) {
      places.push_back(face_place(face));
    }
    throw std::invalid_argument(placed_error(
        "a full round takes the two edges of the face it rounds away, not the face: pick the face's two edges",
        "a full round takes the two edges of the face it rounds away, not face " + set.faces.front() +
            ": pick its two edges",
        places));
  }
  const std::vector<int> picked = resolve_edges(body, set.edges);
  const EdgeFaces edge_faces(body);
  // The face every picked edge borders: the middle face.
  std::vector<int> common;
  for (std::size_t i = 0; i < picked.size(); ++i) {
    const std::vector<int> faces = edge_faces.at(picked[i]);
    if (faces.size() != 2 || faces[0] == faces[1]) {
      refuse("an edge of the full round does not lie between two faces",
             edge_words(body, picked[i]) + " of the full round does not lie between two faces",
             places_of(body, {picked[i]}, {}));
    }
    if (i == 0) {
      common = faces;
    } else {
      common.erase(std::remove_if(common.begin(), common.end(),
                                  [&](int f) { return std::find(faces.begin(), faces.end(), f) == faces.end(); }),
                   common.end());
    }
  }
  if (picked.size() < 2) {
    refuse("a full round takes the two edges of the face it rounds away: pick the face's edge on its other side too",
           "a full round takes the two edges of the face it rounds away, only " + edges_words(body, picked) +
               " is picked",
           places_of(body, picked, {}));
  }
  if (common.empty()) {
    refuse("the edges of a full round share no face: pick the two edges of the face to round away, one on each "
           "side of it",
           edges_words(body, picked) + " share no face: a full round takes the two edges of one face",
           places_of(body, picked, {}));
  }
  if (common.size() > 1) {
    refuse("the edges of a full round lie between the same two faces: pick the edges on the two sides of the face "
           "to round away",
           edges_words(body, picked) + " lie between the same two faces, " + face_words(body, common[0]) +
               " and " + face_words(body, common[1]),
           places_of(body, picked, {}));
  }
  const int middle = common.front();
  // The chains: the picked edges with those their tangent chains add.
  std::vector<int> owner(static_cast<std::size_t>(body.edge_count()), -1);
  for (const int edge : picked) {
    owner[static_cast<std::size_t>(edge)] = 0;
  }
  if (set.tangent_chain) {
    follow_tangent_chains(body, owner, std::vector<char>{1});
  }
  // Of them, the edges along the middle face, in groups that meet end to
  // end: the two chains.
  std::vector<int> along;
  for (int edge = 0; edge < body.edge_count(); ++edge) {
    if (owner[static_cast<std::size_t>(edge)] != 0) {
      continue;
    }
    const std::vector<int> faces = edge_faces.at(edge);
    if (faces.size() == 2 && faces[0] != faces[1] && std::find(faces.begin(), faces.end(), middle) != faces.end()) {
      along.push_back(edge);
    }
  }
  std::vector<int> group(along.size());
  std::iota(group.begin(), group.end(), 0);
  const auto root = [&group](int i) {
    while (group[static_cast<std::size_t>(i)] != i) {
      i = group[static_cast<std::size_t>(i)];
    }
    return i;
  };
  for (std::size_t i = 0; i < along.size(); ++i) {
    for (std::size_t j = i + 1; j < along.size(); ++j) {
      if (share_vertex(body.edge(along[i]), body.edge(along[j]))) {
        group[static_cast<std::size_t>(root(static_cast<int>(j)))] = root(static_cast<int>(i));
      }
    }
  }
  std::vector<int> roots;
  for (std::size_t i = 0; i < along.size(); ++i) {
    const int r = root(static_cast<int>(i));
    if (std::find(roots.begin(), roots.end(), r) == roots.end()) {
      roots.push_back(r);
    }
  }
  if (roots.size() == 1) {
    refuse("the edges of a full round meet at a corner of the face: pick its edges on two opposite sides",
           edges_words(body, picked) + " meet at a corner of " + face_words(body, middle) +
               ": a full round takes its edges on two opposite sides",
           places_of(body, picked, {middle}));
  }
  if (roots.size() > 2) {
    refuse("the edges of a full round lie on more than two sides of the face: pick its edges on two opposite sides",
           edges_words(body, picked) + " lie on more than two sides of " + face_words(body, middle) +
               ": a full round takes its edges on two opposite sides",
           places_of(body, picked, {middle}));
  }
  // Each chain's side faces, and the side the ball rolls on: inside the
  // material at convex edges, outside it at concave ones.
  std::array<std::vector<int>, 2> sides;
  std::vector<int> chain(along.size());
  int convex = 0;
  int concave = 0;
  for (std::size_t i = 0; i < along.size(); ++i) {
    chain[i] = root(static_cast<int>(i)) == roots[0] ? 0 : 1;
    const std::vector<int> faces = edge_faces.at(along[i]);
    const int side = faces[0] == middle ? faces[1] : faces[0];
    std::vector<int>& list = sides[static_cast<std::size_t>(chain[i])];
    if (std::find(list.begin(), list.end(), side) == list.end()) {
      list.push_back(side);
    }
    ChFiDS_TypeOfConcavity type = ChFiDS_Other;
    try {
      OCC_CATCH_SIGNALS
      type = ChFi3d::DefineConnectType(body.edge(along[i]), body.face(middle), body.face(side), 1.0e-4, false);
    } catch (const Standard_Failure&) {
      type = ChFiDS_Other;
    }
    convex += type == ChFiDS_Convex ? 1 : 0;
    concave += type == ChFiDS_Concave ? 1 : 0;
  }
  std::vector<int> all_sides = sides[0];
  all_sides.insert(all_sides.end(), sides[1].begin(), sides[1].end());
  std::vector<int> faces_painted = all_sides;
  faces_painted.insert(faces_painted.begin(), middle);
  const int n = static_cast<int>(along.size());
  if (convex != n && concave != n) {
    refuse("the edges of a full round are not both convex or both concave: no circle rounds the face away",
           edges_words(body, along) + " are not all convex or all concave: no circle rounds " +
               face_words(body, middle) + " away",
           places_of(body, along, {middle}));
  }
  const double ball = convex == n ? 1.0 : -1.0;
  const auto touched = [&](int face) {
    Touched t;
    t.face = body.face(face);
    t.surface = BRep_Tool::Surface(t.face);
    t.side = ball;
    return t;
  };
  double smallest = std::numeric_limits<double>::infinity();
  double largest = -std::numeric_limits<double>::infinity();
  double sum = 0.0;
  int count = 0;
  // Where the circle touches the middle face at the middle of each edge.
  std::vector<gp_Pnt> middle_touches;
  const auto no_circle = [&]() {
    return std::invalid_argument(
        placed_error("no circle that touches both side faces touches the face between its edges: the face cannot "
                     "be rounded away",
                     "no circle that touches the side faces of " + edges_words(body, along) + " touches " +
                         face_words(body, middle) + " between its edges",
                     places_of(body, along, faces_painted)));
  };
  for (std::size_t i = 0; i < along.size(); ++i) {
    const TopoDS_Edge& edge = body.edge(along[i]);
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    const std::vector<int> faces = edge_faces.at(along[i]);
    const int side = faces[0] == middle ? faces[1] : faces[0];
    const std::vector<int>& others = sides[static_cast<std::size_t>(1 - chain[i])];
    try {
      OCC_CATCH_SIGNALS
      const BRepAdaptor_Curve curve(edge);
      for (const double f : kStations) {
        const double t = curve.FirstParameter() + f * (curve.LastParameter() - curve.FirstParameter());
        gp_Pnt at;
        gp_Vec tangent;
        curve.D1(t, at, tangent);
        if (tangent.Magnitude() < 1.0e-12) {
          continue;
        }
        // The other chain's side face nearest to the station.
        int other = -1;
        gp_Pnt other_start;
        double nearest = std::numeric_limits<double>::infinity();
        for (const int face : others) {
          double distance = 0.0;
          if (const std::optional<gp_Pnt> point = nearest_on(body.face(face), at, distance);
              point && distance < nearest) {
            nearest = distance;
            other = face;
            other_start = *point;
          }
        }
        if (other < 0) {
          throw no_circle();
        }
        const std::array<Touched, 3> three{{touched(middle), touched(side), touched(other)}};
        const std::array<gp_Pnt, 3> start{{at, at, other_start}};
        const std::optional<Circle> circle = circle_at(at, tangent, three, start);
        if (!circle || !(circle->radius > kFullRoundTolerance)) {
          throw no_circle();
        }
        if (f == 0.5) {
          middle_touches.push_back(circle->touches[0]);
        }
        smallest = std::min(smallest, circle->radius);
        largest = std::max(largest, circle->radius);
        sum += circle->radius;
        ++count;
      }
    } catch (const Standard_Failure&) {
      throw no_circle();
    }
  }
  if (count == 0) {
    throw no_circle();
  }
  if (largest - smallest > kFullRoundTolerance) {
    char sizes[128];
    std::snprintf(sizes, sizeof sizes, "from %.6g mm to %.6g mm", smallest, largest);
    refuse(std::string("the radius of a full round would change along the edges (") + sizes +
               "): the face between them narrows, or the faces beside it taper or twist",
           "the radius of the full round of " + edges_words(body, along) + " would change along them (" + sizes +
               "): " + face_words(body, middle) + " narrows, or the faces beside it taper or twist",
           places_of(body, along, faces_painted));
  }
  // The circle touches the middle face inside it, between its edges: at
  // the middle of each edge (the section across an edge near its end may
  // pass the face's end where the end is oblique to the edges, mitcad#2279;
  // a radius that changes along the edges is refused above, as such).
  for (const gp_Pnt& touch : middle_touches) {
    try {
      OCC_CATCH_SIGNALS
      const BRepClass_FaceClassifier inside(body.face(middle), touch, 1.0e-7);
      if (inside.State() != TopAbs_IN) {
        throw no_circle();
      }
    } catch (const Standard_Failure&) {
      throw no_circle();
    }
  }
  FullRound round;
  round.radius = sum / count;
  round.middle = middle;
  round.edges = along;
  return round;
}

ShapePtr with_full_round_names(const ShapePtr& result, const Shape& body, const std::string& feature,
                               const std::vector<FullRound>& rounds) {
  if (!result) {
    return result;
  }
  std::vector<NameList> names(static_cast<std::size_t>(result->face_count()));
  for (int f = 0; f < result->face_count(); ++f) {
    names[static_cast<std::size_t>(f)] = result->face_names(f);
  }
  bool changed = false;
  for (const FullRound& round : rounds) {
    const NameList& middle = body.face_names(round.middle);
    if (middle.empty()) {
      continue;
    }
    // (A middle face the result still holds keeps its names alone.)
    bool held = false;
    for (int f = 0; f < result->face_count() && !held; ++f) {
      for (const std::string& name : middle) {
        held = held || face_matches(result->face_names(f), name);
      }
    }
    if (held) {
      continue;
    }
    std::vector<std::string> made;
    for (const int edge : round.edges) {
      const std::string& name = body.edge_name(edge);
      if (!name.empty()) {
        made.push_back(face_name(feature, "fillet", name));
      }
    }
    for (int f = 0; f < result->face_count(); ++f) {
      NameList& list = names[static_cast<std::size_t>(f)];
      const bool rounding = std::any_of(made.begin(), made.end(), [&](const std::string& name) {
        return std::find(list.begin(), list.end(), name) != list.end();
      });
      if (!rounding) {
        continue;
      }
      for (const std::string& name : middle) {
        if (std::find(list.begin(), list.end(), name) == list.end()) {
          list.push_back(name);
          changed = true;
        }
      }
    }
  }
  if (!changed) {
    return result;
  }
  std::vector<Shape::NamedFace> faces;
  for (int f = 0; f < result->face_count(); ++f) {
    faces.push_back({result->face(f), names[static_cast<std::size_t>(f)]});
  }
  ShapePtr named = std::make_shared<Shape>(result->occt(), faces);
  named->copy_notes(*result);
  return named;
}

} // namespace mitcad::geometry::detail
