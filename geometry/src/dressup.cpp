// SPDX-License-Identifier: MIT
#include "mitcad/geometry/dressup.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <exception>
#include <map>
#include <set>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepCheck_Analyzer.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepGProp.hxx>
#include <ChFi3d.hxx>
#include <GProp_GProps.hxx>
#include <Law_BSpline.hxx>
#include <Law_Interpolate.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_HArray1.hxx>
#include <Precision.hxx>
#include <ShapeBuild_ReShape.hxx>
#include <ShapeFix_Shape.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <gp_Pnt2d.hxx>

#include "blend.hpp"
#include "corner.hpp"
#include "face_select.hpp"
#include "history.hpp"
#include "mitcad/geometry/boolean.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

constexpr double kHalfPi = 1.57079632679489661923;
// Tolerance on the angle across an edge that a chord length depends on.
constexpr double kAngleTolerance = 1.0e-4;
// OCCT fails where a rounding or bevel takes a neighbouring face away
// exactly: a radius as large as the face is wide (a full round), a chamfer
// as wide as a step. Imported designs often do that, so a failed dressup is
// built again with its sizes this much smaller, which leaves a strip of
// 1e-3 of the size where the face was (far below any tolerance of the
// part). A strip of 1e-4 is too thin: OCCT corrupted its memory on one of
// the corpus' fillets with it.
constexpr double kShrink = 1.0 - 1.0e-3;

// Runs `attempt` with the sizes as given (scale 1), then, when it fails
// (not for invalid arguments), with the sizes scaled by kShrink, noting so
// on the result (the model warns about it); throws the first error when
// both fail. `what`: "fillet" or "chamfer".
template <class Attempt>
ShapePtr with_shrink_retry(const char* what, Attempt&& attempt) {
  std::exception_ptr first;
  try {
    OCC_CATCH_SIGNALS
    return attempt(1.0);
  } catch (const std::invalid_argument&) {
    throw;
  } catch (const Cancelled&) {
    throw;
  } catch (...) {
    first = std::current_exception();
  }
  try {
    OCC_CATCH_SIGNALS
    ShapePtr smaller = attempt(kShrink);
    smaller->add_note(std::string("the ") + what +
                      " is built 0.1 % smaller than asked: at its exact size it takes a face away, "
                      "which the geometry kernel cannot build");
    return smaller;
  } catch (...) {
  }
  std::rethrow_exception(first);
}

std::string describe_edge(const Shape& body, int edge) {
  const std::string& name = body.edge_name(edge);
  return name.empty() ? "an unnamed edge" : "edge " + name;
}

// The set (index) each selected edge of the body belongs to; -1 for edges
// no set selects. An edge may be in one set only.
template <class Set>
std::vector<int> select_edges(const Shape& body, const std::vector<Set>& sets) {
  if (sets.empty()) {
    throw std::invalid_argument("no edges selected");
  }
  std::vector<int> owner(static_cast<std::size_t>(body.edge_count()), -1);
  for (std::size_t s = 0; s < sets.size(); ++s) {
    for (int edge : detail::resolve_edges(body, sets[s].edges, sets[s].faces)) {
      int& set = owner[static_cast<std::size_t>(edge)];
      if (set >= 0 && set != static_cast<int>(s)) {
        throw std::invalid_argument(describe_edge(body, edge) + " is in two edge sets");
      }
      set = static_cast<int>(s);
    }
  }
  return owner;
}

// Checks the contours OCCT built from the selected edges: a contour is the
// tangent chain of its edges. Every edge in it must come from one set, and
// an edge no set selects is only allowed when that set follows tangent
// chains. Returns the set of each contour.
template <class Set>
std::vector<int> check_contours(const BRepFilletAPI_LocalOperation& maker, const Shape& body,
                                const std::vector<Set>& sets, const std::vector<int>& owner) {
  std::vector<int> contour_sets;
  for (int ic = 1; ic <= maker.NbContours(); ++ic) {
    int set = -1;
    int unselected = -1;
    int seed = -1;
    for (int ie = 1; ie <= maker.NbEdges(ic); ++ie) {
      const int edge = body.edge_index(maker.Edge(ic, ie));
      const int own = edge < 0 ? -1 : owner[static_cast<std::size_t>(edge)];
      if (own < 0) {
        unselected = edge;
      } else if (set < 0) {
        set = own;
        seed = edge;
      } else if (own != set) {
        throw std::invalid_argument("unsupported: " + describe_edge(body, edge) +
                                    " continues the tangent chain of " +
                                    describe_edge(body, seed) + " from another edge set");
      }
    }
    if (set >= 0 && unselected >= 0 && !sets[static_cast<std::size_t>(set)].tangent_chain) {
      throw std::invalid_argument(
          "unsupported: " + describe_edge(body, seed) + " continues tangentially into " +
          describe_edge(body, unselected) +
          "; without a tangent chain only the selected edges would be rounded, but the "
          "geometry kernel always follows the whole chain");
    }
    contour_sets.push_back(set);
  }
  return contour_sets;
}

// Vertices where rounded edges of at least two contours meet, three or more
// edges in all: the corners a corner type (rolling ball or setback) shapes.
std::vector<TopoDS_Shape> corner_vertices(const BRepFilletAPI_LocalOperation& maker) {
  std::map<int, std::pair<std::set<int>, int>> at; // vertex -> (contours, edges)
  ShapeMap vertices;
  for (int ic = 1; ic <= maker.NbContours(); ++ic) {
    for (int ie = 1; ie <= maker.NbEdges(ic); ++ie) {
      ShapeMap ends;
      TopExp::MapShapes(maker.Edge(ic, ie), TopAbs_VERTEX, ends);
      for (int k = 1; k <= ends.Extent(); ++k) {
        const int v = vertices.Add(ends(k));
        at[v].first.insert(ic);
        ++at[v].second;
      }
    }
  }
  std::vector<TopoDS_Shape> corners;
  for (const auto& [v, use] : at) {
    if (use.first.size() >= 2 && use.second >= 3) {
      corners.push_back(vertices(v));
    }
  }
  return corners;
}

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

// A dressup's result that BRepCheck rejects, repaired by shape healing
// with its names, or null. On imported bodies (.f3d bodies, with tolerances
// up to 0.05 mm) OCCT at times builds the rounding right but turns the
// solid inside out or leaves a small defect in a wire; healing repairs
// those. Only a repair that keeps the volume near the body's (within a
// quarter) is taken: a dressup changes it little, a wrong repair a lot.
ShapePtr healed(const Shape& result, const Shape& body) {
  try {
    OCC_CATCH_SIGNALS
    // Healing adds p-curves to edges and widens tolerances in place, and
    // the faces the dressup left are the body's (input_check.hpp).
    const ShapePtr copy = detail::working_copy(result);
    const Shape& named = *copy;
    ShapeFix_Shape fix(named.occt());
    detail::interruptible([&](const Message_ProgressRange& range) { fix.Perform(range); });
    const TopoDS_Shape fixed = fix.Shape();
    if (fixed.IsNull() || !BRepCheck_Analyzer(fixed).IsValid()) {
      return nullptr;
    }
    const double before = volume_of(body.occt());
    const double after = volume_of(fixed);
    if (!(after > 0.0) || std::abs(after - before) > 0.25 * std::abs(before)) {
      return nullptr;
    }
    detail::FaceNamer namer(fixed);
    namer.carry(*fix.Context()->History(), named);
    namer.finish();
    return namer.shape();
  } catch (const Standard_Failure&) {
    return nullptr;
  }
}

// Names the result of a fillet or chamfer: the faces generated from every
// edge of the contours (the tangent chains included) and their vertices.
ShapePtr name_result(BRepFilletAPI_LocalOperation& maker, const std::string& feature,
                     const char* role, const Shape& body) {
  detail::FaceNamer namer(maker.Shape());
  namer.carry(maker, body);
  ShapeMap vertices;
  for (int ic = 1; ic <= maker.NbContours(); ++ic) {
    for (int ie = 1; ie <= maker.NbEdges(ic); ++ie) {
      const TopoDS_Edge& edge = maker.Edge(ic, ie);
      if (const auto name = body.name_of_edge(edge)) {
        namer.generated(maker, edge, face_name(feature, role, *name));
      }
      TopExp::MapShapes(edge, TopAbs_VERTEX, vertices);
    }
  }
  for (int v = 1; v <= vertices.Extent(); ++v) {
    if (const auto name = body.name_of_vertex(vertices(v))) {
      namer.generated(maker, vertices(v), face_name(feature, "corner", *name));
    }
  }
  namer.finish();
  // One check of a valid result: on a large body BRepCheck takes about as
  // long as the fillet itself.
  if (!BRepCheck_Analyzer(namer.result()).IsValid()) {
    if (ShapePtr repaired = healed(*namer.shape(), body)) {
      return repaired;
    }
    detail::require_valid(namer.result(), role); // throws with the problem
  }
  return namer.shape();
}

// The two faces of a body edge; throws for a free edge.
std::pair<int, int> edge_face_pair(const Shape& body, int edge) {
  const std::vector<int> faces = detail::faces_at_edge(body, edge);
  if (faces.size() != 2) {
    throw std::invalid_argument(describe_edge(body, edge) + " does not lie between two faces");
  }
  return {faces[0], faces[1]};
}

// The fillet radius whose rounding is `chord` wide across the edge: with an
// angle a between the faces' normals, r = c / (2 sin(a / 2)).
double chord_radius(const Shape& body, int edge, double chord) {
  const auto [a, b] = edge_face_pair(body, edge);
  const TopoDS_Edge& e = body.edge(edge);
  const double angle = detail::normal_angle(e, body.face(a), body.face(b));
  for (const double t : {0.1, 0.9}) {
    if (std::abs(detail::normal_angle(e, body.face(a), body.face(b), t) - angle) >
        kAngleTolerance) {
      throw std::invalid_argument("unsupported: a chord length fillet on " +
                                  describe_edge(body, edge) +
                                  ", where the angle between the faces varies");
    }
  }
  if (angle < 1.0e-3) {
    throw std::invalid_argument(describe_edge(body, edge) + " joins its faces tangentially");
  }
  return chord / (2.0 * std::sin(angle / 2.0));
}

void check_fillet_set(const FilletSet& set) {
  switch (set.size) {
  case FilletSize::Constant:
    detail::require_positive("fillet radius", set.radius);
    break;
  case FilletSize::ChordLength:
    detail::require_positive("fillet chord length", set.chord);
    break;
  case FilletSize::Variable: {
    detail::require_positive("fillet start radius", set.radius);
    detail::require_positive("fillet end radius", set.radius2);
    double last = 0.0;
    for (const auto& [position, radius] : set.mid) {
      if (!(position > last && position < 1.0)) {
        throw std::invalid_argument("the mid radius positions must increase between 0 and 1");
      }
      detail::require_positive("fillet mid radius", radius);
      last = position;
    }
    break;
  }
  case FilletSize::Asymmetric:
    detail::require_positive("fillet distance", set.radius);
    detail::require_positive("second fillet distance", set.radius2);
    break;
  }
  if (set.curvature && !(set.weight >= 0.1 && set.weight <= 2.0)) {
    throw std::invalid_argument("the tangency weight must be between 0.1 and 2");
  }
}

// The radius at an abscissa of a smooth law through (abscissa, radius)
// points, level at both ends: the interpolation OCCT makes of the radii
// along a spine (ChFiDS_FilSpine with Law_Interpol), so that radii set at
// further abscissae from it leave the law as it is.
class RadiusLaw {
public:
  explicit RadiusLaw(const std::vector<gp_Pnt2d>& points) {
    const int n = static_cast<int>(points.size());
    occ::handle<NCollection_HArray1<double>> values = new NCollection_HArray1<double>(1, n);
    occ::handle<NCollection_HArray1<double>> parameters = new NCollection_HArray1<double>(1, n);
    for (int i = 0; i < n; ++i) {
      parameters->SetValue(i + 1, points[static_cast<std::size_t>(i)].X());
      values->SetValue(i + 1, points[static_cast<std::size_t>(i)].Y());
    }
    Law_Interpolate interpolation(values, parameters, false, Precision::Confusion());
    interpolation.Load(0.0, 0.0);
    interpolation.Perform();
    if (!interpolation.IsDone()) {
      throw std::runtime_error("the radii of a variable fillet could not be interpolated");
    }
    m_law = interpolation.Curve();
  }

  double operator()(double abscissa) const { return m_law->Value(abscissa); }

private:
  occ::handle<Law_BSpline> m_law;
};

// Adds a variable radius set's contour from edge `e`: the radius from the
// start (the set's start vertex, else the spine's first vertex) to the end
// of the whole chain, through the mid radii at their fractions of its length.
void add_variable(BRepFilletAPI_MakeFillet& maker, const Shape& body, const FilletSet& set,
                  const TopoDS_Edge& e, double scale) {
  const double r1 = set.radius * scale;
  const double r2 = set.radius2 * scale;
  maker.Add(r1, r2, e);
  const int ic = maker.Contour(e);
  const bool closed = maker.FirstVertex(ic).IsSame(maker.LastVertex(ic));
  bool reversed = false;
  if (!set.start_vertex.empty()) {
    const std::vector<int> start = body.find_vertices(set.start_vertex);
    if (start.size() != 1) {
      throw std::invalid_argument("the body has no vertex " + set.start_vertex);
    }
    if (closed) {
      throw std::invalid_argument("unsupported: a variable radius around a closed chain");
    }
    const TopoDS_Shape& vertex = body.vertex(start.front());
    reversed = maker.LastVertex(ic).IsSame(vertex);
    if (!reversed && !maker.FirstVertex(ic).IsSame(vertex)) {
      throw std::invalid_argument("vertex " + set.start_vertex + " is not an end of the edges' chain");
    }
  }
  if (closed) {
    if (!set.mid.empty()) {
      throw std::invalid_argument("unsupported: mid radii around a closed chain");
    }
    return; // from the start of OCCT's spine round to it
  }
  if (maker.NbEdges(ic) == 1 && set.mid.empty()) {
    if (reversed) {
      // The law runs from the spine's first vertex; start from the other end.
      maker.Remove(e);
      maker.Add(r2, r1, e);
    }
    return;
  }
  // The radii along the spine (abscissa from its first vertex).
  const double length = maker.Length(ic);
  std::vector<gp_Pnt2d> law{{0.0, r1}};
  for (const auto& [position, radius] : set.mid) {
    law.emplace_back(position * length, radius * scale);
  }
  law.emplace_back(length, r2);
  if (reversed) {
    for (gp_Pnt2d& point : law) {
      point.SetX(length - point.X());
    }
    std::reverse(law.begin(), law.end());
  }
  const RadiusLaw radius(law);
  // OCCT takes radii per edge of the spine, at fractions of the edge: each
  // edge gets the law at its ends and the radii that fall on it.
  for (int j = 1; j <= maker.NbEdges(ic); ++j) {
    const TopoDS_Edge& edge = maker.Edge(ic, j);
    double a = maker.Abscissa(ic, TopExp::FirstVertex(edge));
    double b = maker.Abscissa(ic, TopExp::LastVertex(edge));
    if (a > b) {
      std::swap(a, b);
    }
    if (!(b - a > Precision::Confusion())) {
      continue;
    }
    std::vector<gp_Pnt2d> points{{0.0, radius(a)}};
    for (const gp_Pnt2d& point : law) {
      if (point.X() > a + Precision::Confusion() && point.X() < b - Precision::Confusion()) {
        points.emplace_back((point.X() - a) / (b - a), point.Y());
      }
    }
    points.emplace_back(1.0, radius(b));
    NCollection_Array1<gp_Pnt2d> array(1, static_cast<int>(points.size()));
    for (std::size_t k = 0; k < points.size(); ++k) {
      array(static_cast<int>(k) + 1) = points[k];
    }
    maker.SetRadius(array, ic, j);
  }
}

// Setback and blend corners stop the roundings or bevels this many times
// their largest size from the vertex (Mitcad's convention until K89, K90):
// past the rolling ball's or OCCT's own corner face.
constexpr double kSetback = 1.5;

// The largest radius a set rounds an edge of the body with.
double largest_radius(const Shape& body, const FilletSet& set, int edge) {
  switch (set.size) {
  case FilletSize::Constant:
    return set.radius;
  case FilletSize::ChordLength:
    return edge < 0 ? set.chord : chord_radius(body, edge, set.chord);
  case FilletSize::Variable: {
    double largest = std::max(set.radius, set.radius2);
    for (const auto& mid : set.mid) {
      largest = std::max(largest, mid.second);
    }
    return largest;
  }
  case FilletSize::Asymmetric:
    return std::max(set.radius, set.radius2);
  }
  return set.radius;
}

// The largest distance a chamfer set bevels a face to.
double largest_distance(const ChamferSpec& spec) {
  switch (spec.type) {
  case ChamferType::EqualDistance:
    return spec.distance;
  case ChamferType::TwoDistances:
    return std::max(spec.distance, spec.distance2);
  case ChamferType::DistanceAngle:
    return spec.distance * std::max(1.0, std::tan(spec.angle));
  }
  return spec.distance;
}

// The rolling ball fillets of the sets (none asymmetric or G2) that own the
// body's edges, at their sizes times `scale`.
ShapePtr round_edges(const std::string& feature, const Shape& body, const std::vector<FilletSet>& sets,
                     const std::vector<int>& owner, bool rolling_ball_corners, double scale) {
  BRepFilletAPI_MakeFillet maker(body.occt());
  for (int edge = 0; edge < body.edge_count(); ++edge) {
    const int s = owner[static_cast<std::size_t>(edge)];
    if (s < 0 || maker.Contour(body.edge(edge)) != 0) {
      continue;
    }
    const FilletSet& set = sets[static_cast<std::size_t>(s)];
    const TopoDS_Edge& e = body.edge(edge);
    switch (set.size) {
    case FilletSize::Constant:
      maker.Add(set.radius * scale, e);
      break;
    case FilletSize::ChordLength:
      maker.Add(chord_radius(body, edge, set.chord) * scale, e);
      break;
    case FilletSize::Variable:
      add_variable(maker, body, set, e, scale);
      break;
    case FilletSize::Asymmetric:
      throw std::logic_error("an asymmetric set among the rolling ball fillets");
    }
  }
  const std::vector<int> contour_sets = check_contours(maker, body, sets, owner);
  for (std::size_t c = 0; c < contour_sets.size(); ++c) {
    const int s = contour_sets[c];
    if (s < 0 || sets[static_cast<std::size_t>(s)].size != FilletSize::ChordLength) {
      continue;
    }
    // The chord stays, so the radius would vary with the angle.
    const int ic = static_cast<int>(c) + 1;
    const double radius = maker.Radius(ic);
    for (int ie = 1; ie <= maker.NbEdges(ic); ++ie) {
      const int edge = body.edge_index(maker.Edge(ic, ie));
      if (edge >= 0 &&
          std::abs(chord_radius(body, edge, sets[static_cast<std::size_t>(s)].chord) * scale - radius) >
              1.0e-7 * std::max(1.0, radius)) {
        throw std::invalid_argument("unsupported: a chord length fillet along a chain where "
                                    "the angle between the faces varies");
      }
    }
  }
  // Setback corners: from the rolling ball ones, set back by a multiple of
  // the largest radius that meets there.
  std::vector<detail::Setback> setbacks;
  if (!rolling_ball_corners) {
    for (const TopoDS_Shape& vertex : corner_vertices(maker)) {
      double largest = 0.0;
      for (int ic = 1; ic <= maker.NbContours(); ++ic) {
        for (int ie = 1; ie <= maker.NbEdges(ic); ++ie) {
          const TopoDS_Edge& edge = maker.Edge(ic, ie);
          if (TopExp::FirstVertex(edge).IsSame(vertex) || TopExp::LastVertex(edge).IsSame(vertex)) {
            const int s = contour_sets[static_cast<std::size_t>(ic - 1)];
            largest = std::max(largest, s < 0 ? 0.0 : largest_radius(body, sets[static_cast<std::size_t>(s)],
                                                                     body.edge_index(edge)) *
                                                          scale);
          }
        }
      }
      const auto name = body.name_of_vertex(vertex);
      if (!name || !(largest > 0.0)) {
        throw std::invalid_argument("unsupported: a setback corner at an unnamed vertex");
      }
      setbacks.push_back({*name, kSetback * largest});
    }
  }
  detail::build(maker);
  if (!maker.IsDone()) {
    std::string reason = "the radius may be too large for the edges";
    if (maker.NbFaultyVertices() > 0) {
      reason = "the geometry kernel cannot end or join the rounding at a vertex";
    }
    throw std::runtime_error("fillet failed; " + reason);
  }
  const ShapePtr rounded = name_result(maker, feature, "fillet", body);
  return setbacks.empty() ? rounded : detail::setback_corners(feature, "fillet", body, *rounded, setbacks);
}

// The face of a body edge that a distance is measured on: the face that
// matches `reference`, or without one the first face of the edge's name;
// `flip` takes the other face.
int measured_face(const Shape& body, int edge, const std::string& reference, bool flip) {
  if (reference.empty()) {
    const std::array<int, 2> faces = body.edge_faces(edge);
    const int measured = flip ? faces[1] : faces[0];
    if (measured < 0) {
      throw std::invalid_argument(describe_edge(body, edge) + " has no face to measure on");
    }
    return measured;
  }
  const auto [a, b] = edge_face_pair(body, edge);
  const bool on_a = face_matches(body.face_names(a), reference);
  const bool on_b = face_matches(body.face_names(b), reference);
  if (on_a == on_b) {
    throw std::invalid_argument(describe_edge(body, edge) +
                                (on_a ? " lies between two pieces of face " : " does not border face ") +
                                reference);
  }
  return (on_a != flip) ? a : b;
}

// Adds the chamfers of the edges the sets own (with `only`, of the edges it
// marks), at their sizes times `scale`.
void add_chamfers(BRepFilletAPI_MakeChamfer& maker, const Shape& body, const std::vector<ChamferSet>& sets,
                  const std::vector<int>& owner, double scale, const std::vector<char>* only = nullptr) {
  for (int edge = 0; edge < body.edge_count(); ++edge) {
    const int s = owner[static_cast<std::size_t>(edge)];
    if (s < 0 || (only != nullptr && (*only)[static_cast<std::size_t>(edge)] == 0) ||
        maker.Contour(body.edge(edge)) != 0) {
      continue;
    }
    const ChamferSpec& spec = sets[static_cast<std::size_t>(s)].spec;
    const TopoDS_Edge& e = body.edge(edge);
    if (spec.type == ChamferType::EqualDistance) {
      maker.Add(spec.distance * scale, e);
      continue;
    }
    const int measured = measured_face(body, edge, spec.reference_face, spec.flip);
    if (spec.type == ChamferType::TwoDistances) {
      maker.Add(spec.distance * scale, spec.distance2 * scale, e, body.face(measured));
    } else {
      maker.AddDA(spec.distance * scale, spec.angle, e, body.face(measured));
    }
  }
}

// Whether the material's angle across a body edge is less than a half turn
// (a dressup of it takes material away).
bool convex_edge(const Shape& body, int edge) {
  const auto [a, b] = edge_face_pair(body, edge);
  const ChFiDS_TypeOfConcavity type =
      ChFi3d::DefineConnectType(body.edge(edge), body.face(a), body.face(b), 1.0e-4, false);
  if (type != ChFiDS_Convex && type != ChFiDS_Concave) {
    throw std::invalid_argument("unsupported: a miter corner at " + describe_edge(body, edge) +
                                ", which is neither convex nor concave along its length");
  }
  return type == ChFiDS_Convex;
}

// The chamfer with miter corners: each bevel runs on until it meets the
// others. The contours are bevelled in groups of which no two meet at a
// corner; the material the groups take away is taken away together
// (their results intersected), and the wedges they add at concave edges
// are joined. A corner where convex and concave edges meet is unsupported.
ShapePtr miter(const std::string& feature, const Shape& body, const std::vector<ChamferSet>& sets,
               const std::vector<int>& owner, const BRepFilletAPI_MakeChamfer& all,
               const std::vector<TopoDS_Shape>& corners, double scale) {
  const int contours = all.NbContours();
  std::vector<int> convex(static_cast<std::size_t>(contours) + 1, -1);
  std::vector<std::set<int>> meets(static_cast<std::size_t>(contours) + 1);
  for (const TopoDS_Shape& corner : corners) {
    std::set<int> at;
    for (int ic = 1; ic <= contours; ++ic) {
      for (int ie = 1; ie <= all.NbEdges(ic); ++ie) {
        const TopoDS_Edge& edge = all.Edge(ic, ie);
        if (TopExp::FirstVertex(edge).IsSame(corner) || TopExp::LastVertex(edge).IsSame(corner)) {
          at.insert(ic);
          if (convex[static_cast<std::size_t>(ic)] < 0) {
            const int index = body.edge_index(edge);
            if (index < 0) {
              throw std::runtime_error("a chamfered edge is not an edge of the body");
            }
            convex[static_cast<std::size_t>(ic)] = convex_edge(body, index) ? 1 : 0;
          }
        }
      }
    }
    std::set<int> kinds;
    for (int ic : at) {
      kinds.insert(convex[static_cast<std::size_t>(ic)]);
      meets[static_cast<std::size_t>(ic)].insert(at.begin(), at.end());
      meets[static_cast<std::size_t>(ic)].erase(ic);
    }
    if (kinds.size() > 1) {
      throw std::invalid_argument("unsupported: a miter corner where convex and concave edges meet");
    }
  }
  // Groups by greedy colouring: a contour takes the first group none of
  // the contours it meets is in.
  std::vector<int> group(static_cast<std::size_t>(contours) + 1, -1);
  int groups = 0;
  for (int ic = 1; ic <= contours; ++ic) {
    std::set<int> taken;
    for (int other : meets[static_cast<std::size_t>(ic)]) {
      taken.insert(group[static_cast<std::size_t>(other)]);
    }
    int g = 0;
    while (taken.count(g) > 0) {
      ++g;
    }
    group[static_cast<std::size_t>(ic)] = g;
    groups = std::max(groups, g + 1);
  }
  std::vector<ShapePtr> bevelled;
  for (int g = 0; g < groups; ++g) {
    std::vector<char> only(static_cast<std::size_t>(body.edge_count()), 0);
    for (int ic = 1; ic <= contours; ++ic) {
      if (group[static_cast<std::size_t>(ic)] != g) {
        continue;
      }
      for (int ie = 1; ie <= all.NbEdges(ic); ++ie) {
        const int index = body.edge_index(all.Edge(ic, ie));
        if (index >= 0) {
          only[static_cast<std::size_t>(index)] = 1;
        }
      }
    }
    // Each group on a copy of its own: a chamfer changes its input.
    const ShapePtr own = detail::working_copy(body);
    BRepFilletAPI_MakeChamfer maker(own->occt());
    add_chamfers(maker, *own, sets, owner, scale, &only);
    detail::build(maker);
    if (!maker.IsDone()) {
      throw std::runtime_error("chamfer failed; the distance may be too large for the edges");
    }
    bevelled.push_back(name_result(maker, feature, "chamfer", *own));
  }
  ShapePtr result = bevelled.front();
  for (std::size_t g = 1; g < bevelled.size(); ++g) {
    BooleanResult common = boolean(BooleanOp::Intersect, {result.get()}, *bevelled[g]);
    if (!common.touched.front()) {
      continue; // the group takes nothing away from what is left
    }
    if (common.pieces.size() != 1) {
      throw std::runtime_error("the miter corners split the body");
    }
    result = common.pieces.front().shape;
  }
  for (const ShapePtr& group_result : bevelled) {
    for (const BooleanPiece& wedge : boolean(BooleanOp::Cut, {group_result.get()}, body).pieces) {
      BooleanResult joined = boolean(BooleanOp::Join, {result.get()}, *wedge.shape);
      if (joined.pieces.size() != 1) {
        throw std::runtime_error("the bevels of the miter corners could not be joined");
      }
      result = joined.pieces.front().shape;
    }
  }
  return result;
}

// The distance from the edge at which a G2 set touches the faces: where
// the circular rounding of its radius (or chord) would, r tan(a / 2) with a
// the angle between the faces' normals at the middle of the edge.
double contact_distance(const Shape& body, int edge, const FilletSet& set) {
  const auto [a, b] = edge_face_pair(body, edge);
  const double angle = detail::normal_angle(body.edge(edge), body.face(a), body.face(b));
  if (angle < 1.0e-3) {
    throw std::invalid_argument(describe_edge(body, edge) + " joins its faces tangentially");
  }
  if (set.size == FilletSize::ChordLength) {
    return set.chord / (2.0 * std::cos(angle / 2.0));
  }
  return set.radius * std::tan(angle / 2.0);
}

// Fillets with asymmetric or G2 sets (`blended`): the other sets' rolling
// ball fillets first, then a chamfer of the blended sets at their contact
// distances, its bevels replaced by blend surfaces. A blended chain that
// meets another rounded edge at a vertex is unsupported.
ShapePtr round_and_blend(const std::string& feature, const Shape& body, const std::vector<FilletSet>& sets,
                         const std::vector<int>& owner, const std::vector<char>& blended,
                         bool rolling_ball_corners, double scale) {
  // The chains of all sets, to see where the blended ones end.
  BRepFilletAPI_MakeChamfer probe(body.occt());
  for (int edge = 0; edge < body.edge_count(); ++edge) {
    if (owner[static_cast<std::size_t>(edge)] >= 0 && probe.Contour(body.edge(edge)) == 0) {
      probe.Add(1.0, body.edge(edge));
    }
  }
  const std::vector<int> contour_sets = check_contours(probe, body, sets, owner);
  std::vector<ShapeMap> ends(contour_sets.size());
  for (std::size_t c = 0; c < contour_sets.size(); ++c) {
    const int ic = static_cast<int>(c) + 1;
    for (int ie = 1; ie <= probe.NbEdges(ic); ++ie) {
      TopExp::MapShapes(probe.Edge(ic, ie), TopAbs_VERTEX, ends[c]);
    }
  }
  for (std::size_t c = 0; c < contour_sets.size(); ++c) {
    if (contour_sets[c] < 0 || blended[static_cast<std::size_t>(contour_sets[c])] == 0) {
      continue;
    }
    for (std::size_t other = 0; other < contour_sets.size(); ++other) {
      for (int v = 1; other != c && v <= ends[other].Extent(); ++v) {
        if (ends[c].Contains(ends[other](v))) {
          throw std::invalid_argument("unsupported: asymmetric and curvature continuous fillets that "
                                      "meet other rounded edges at a corner");
        }
      }
    }
  }
  // The rolling ball sets.
  ShapePtr rounded;
  std::vector<int> plain = owner;
  bool any_plain = false;
  for (int& s : plain) {
    if (s >= 0 && blended[static_cast<std::size_t>(s)] != 0) {
      s = -1;
    }
    any_plain = any_plain || s >= 0;
  }
  if (any_plain) {
    rounded = round_edges(feature, body, sets, plain, rolling_ball_corners, scale);
  }
  const Shape& start = rounded ? *rounded : body;
  // The blended sets' chamfer, on the same edges by name.
  BRepFilletAPI_MakeChamfer maker(start.occt());
  std::map<std::string, int> set_of_edge;
  for (int edge = 0; edge < body.edge_count(); ++edge) {
    const int s = owner[static_cast<std::size_t>(edge)];
    if (s < 0 || blended[static_cast<std::size_t>(s)] == 0) {
      continue;
    }
    const std::string& name = body.edge_name(edge);
    const std::vector<int> found = name.empty() ? std::vector<int>() : start.find_edges(name);
    if (found.size() != 1) {
      throw std::invalid_argument("unsupported: an asymmetric or curvature continuous fillet of an "
                                  "edge the other fillets change");
    }
    const int e = found.front();
    set_of_edge[name] = s;
    if (maker.Contour(start.edge(e)) != 0) {
      continue;
    }
    const FilletSet& set = sets[static_cast<std::size_t>(s)];
    switch (set.size) {
    case FilletSize::Asymmetric:
      maker.Add(set.radius * scale, set.radius2 * scale, start.edge(e),
                start.face(measured_face(start, e, set.reference_face, set.flip)));
      break;
    case FilletSize::Constant:
    case FilletSize::ChordLength:
      maker.Add(contact_distance(start, e, set) * scale, start.edge(e));
      break;
    case FilletSize::Variable:
      throw std::invalid_argument("unsupported: curvature continuous fillets with a variable radius");
    }
  }
  // Every edge of the chains, with its set's cross-section.
  std::map<std::string, detail::BlendSection> kinds;
  for (int ic = 1; ic <= maker.NbContours(); ++ic) {
    int s = -1;
    for (int ie = 1; ie <= maker.NbEdges(ic) && s < 0; ++ie) {
      if (const auto name = start.name_of_edge(maker.Edge(ic, ie))) {
        const auto found = set_of_edge.find(*name);
        s = found == set_of_edge.end() ? -1 : found->second;
      }
    }
    if (s < 0) {
      throw std::runtime_error("a bevelled chain has no edge set");
    }
    detail::BlendSection kind;
    kind.curvature = sets[static_cast<std::size_t>(s)].curvature;
    kind.weight = sets[static_cast<std::size_t>(s)].weight;
    for (int ie = 1; ie <= maker.NbEdges(ic); ++ie) {
      if (const auto name = start.name_of_edge(maker.Edge(ic, ie))) {
        kinds[*name] = kind;
      }
    }
  }
  detail::build(maker);
  if (!maker.IsDone()) {
    throw std::runtime_error("fillet failed; the distances may be too large for the edges");
  }
  const ShapePtr chamfered = name_result(maker, feature, "fillet", start);
  return detail::blend_bevels(feature, start, *chamfered, kinds);
}

const char* corner_name(ChamferCorner corner) {
  switch (corner) {
  case ChamferCorner::Chamfer:
    return "chamfer";
  case ChamferCorner::Miter:
    return "miter";
  case ChamferCorner::Blend:
    return "blend";
  }
  return "chamfer";
}

} // namespace

ShapePtr fillet(const std::string& feature, const Shape& input, const std::vector<FilletSet>& sets,
                bool rolling_ball_corners) {
  for (const FilletSet& set : sets) {
    check_fillet_set(set);
  }
  const std::vector<int> owner = select_edges(input, sets);
  std::vector<char> blended(sets.size(), 0);
  bool any_blended = false;
  for (std::size_t s = 0; s < sets.size(); ++s) {
    blended[s] = sets[s].size == FilletSize::Asymmetric || sets[s].curvature ? 1 : 0;
    any_blended = any_blended || blended[s] != 0;
  }
  return detail::run("fillet", [&] {
    return with_shrink_retry("fillet", [&](double scale) {
      // OCCT's fillets give the edges of their input p-curves on the
      // rounding surfaces and wider tolerances (input_check.hpp): each try
      // works on a copy of its own.
      const ShapePtr copy = detail::working_copy(input);
      const Shape& body = *copy;
      if (any_blended) {
        return round_and_blend(feature, body, sets, owner, blended, rolling_ball_corners, scale);
      }
      return round_edges(feature, body, sets, owner, rolling_ball_corners, scale);
    });
  });
}

ShapePtr fillet(const std::string& feature, const Shape& body, const std::vector<std::string>& edges,
                double radius) {
  FilletSet set;
  set.edges = edges;
  set.radius = radius;
  return fillet(feature, body, {set});
}

ShapePtr chamfer(const std::string& feature, const Shape& input, const std::vector<ChamferSet>& sets,
                 ChamferCorner corner) {
  for (const ChamferSet& set : sets) {
    const ChamferSpec& spec = set.spec;
    detail::require_positive("chamfer distance", spec.distance);
    if (spec.type == ChamferType::TwoDistances) {
      detail::require_positive("second chamfer distance", spec.distance2);
    }
    if (spec.type == ChamferType::DistanceAngle && !(spec.angle > 0.0 && spec.angle < kHalfPi)) {
      throw std::invalid_argument("the chamfer angle must be between 0 and 90 degrees");
    }
  }
  const std::vector<int> owner = select_edges(input, sets);
  return detail::run("chamfer", [&] {
    return with_shrink_retry("chamfer", [&](double scale) {
      // As OCCT's fillets, its chamfers change their input: a copy.
      const ShapePtr copy = detail::working_copy(input);
      const Shape& body = *copy;
      BRepFilletAPI_MakeChamfer maker(body.occt());
      add_chamfers(maker, body, sets, owner, scale);
      const std::vector<int> contour_sets = check_contours(maker, body, sets, owner);
      const std::vector<TopoDS_Shape> corners =
          corner == ChamferCorner::Chamfer ? std::vector<TopoDS_Shape>() : corner_vertices(maker);
      if (corner == ChamferCorner::Miter && !corners.empty()) {
        return miter(feature, body, sets, owner, maker, corners, scale);
      }
      // Blend corners: OCCT's corner faces set back by a multiple of the
      // largest distance that meets there, a patch in their place.
      std::vector<detail::Setback> setbacks;
      for (const TopoDS_Shape& vertex : corners) {
        double largest = 0.0;
        for (int ic = 1; ic <= maker.NbContours(); ++ic) {
          const int s = contour_sets[static_cast<std::size_t>(ic - 1)];
          for (int ie = 1; ie <= maker.NbEdges(ic) && s >= 0; ++ie) {
            const TopoDS_Edge& edge = maker.Edge(ic, ie);
            if (TopExp::FirstVertex(edge).IsSame(vertex) || TopExp::LastVertex(edge).IsSame(vertex)) {
              largest = std::max(largest, largest_distance(sets[static_cast<std::size_t>(s)].spec) * scale);
            }
          }
        }
        const auto name = body.name_of_vertex(vertex);
        if (!name || !(largest > 0.0)) {
          throw std::invalid_argument("unsupported: a blend corner at an unnamed vertex");
        }
        setbacks.push_back({*name, kSetback * largest});
      }
      detail::build(maker);
      if (!maker.IsDone()) {
        throw std::runtime_error("chamfer failed; the distance may be too large for the edges");
      }
      ShapePtr result = name_result(maker, feature, "chamfer", body);
      if (!setbacks.empty()) {
        return detail::setback_corners(feature, "chamfer", body, *result, setbacks);
      }
      if (corner != ChamferCorner::Chamfer) {
        result->add_note(std::string("the ") + corner_name(corner) +
                         " corner type shapes nothing here: no three bevelled edges meet at a vertex");
      }
      return result;
    });
  });
}

ShapePtr chamfer(const std::string& feature, const Shape& body, const std::vector<std::string>& edges,
                 const ChamferSpec& spec) {
  ChamferSet set;
  set.edges = edges;
  set.spec = spec;
  return chamfer(feature, body, {set});
}

} // namespace mitcad::geometry
