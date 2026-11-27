// SPDX-License-Identifier: MIT
#include "mitcad/geometry/surface_offsets.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>
#include <limits>
#include <map>
#include <memory>
#include <set>
#include <stdexcept>
#include <string>
#include <utility>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepBuilderAPI_Copy.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakePolygon.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepFill.hxx>
#include <BRepGProp.hxx>
#include <BRepGProp_Face.hxx>
#include <BRepOffset_MakeOffset.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRepTools.hxx>
#include <BRepTools_History.hxx>
#include <BRepTools_WireExplorer.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <GProp_GProps.hxx>
#include <GeomAPI_Interpolate.hxx>
#include <GeomAPI_ProjectPointOnSurf.hxx>
#include <GeomConvert.hxx>
#include <GeomLib.hxx>
#include <Geom_BSplineCurve.hxx>
#include <Geom_BSplineSurface.hxx>
#include <Geom_BoundedSurface.hxx>
#include <Geom_RectangularTrimmedSurface.hxx>
#include <Geom_Surface.hxx>
#include <Geom2d_Curve.hxx>
#include <NCollection_Array1.hxx>
#include <NCollection_HArray1.hxx>
#include <NCollection_List.hxx>
#include <ShapeUpgrade_UnifySameDomain.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Ax1.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Vec.hxx>
#include <gp_Vec2d.hxx>

// Extend past trimmed boundaries of curved faces (mitcad#2525).
#include <BRepLib.hxx>
#include <Geom2dAPI_Interpolate.hxx>
#include <Geom2d_BSplineCurve.hxx>
#include <Geom_BezierSurface.hxx>
#include <Geom_OffsetSurface.hxx>
#include <NCollection_Array2.hxx>
#include <Precision.hxx>
#include <TopoDS_Iterator.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Trsf.hxx>

#include "face_select.hpp"
#include "free_loops.hpp"
#include "history.hpp"
#include "mitcad/geometry/surfaces.hpp"
#include "sheet_edges.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

constexpr double kTolerance = 1.0e-6;
// Faces sewn after a ruled strip or an extension: their edges meet within
// this.
constexpr double kSewing = 1.0e-4;

// The faces' images in an operation's history.
std::vector<TopoDS_Shape> faces_from(const NCollection_List<TopoDS_Shape>& shapes) {
  std::vector<TopoDS_Shape> faces;
  for (const TopoDS_Shape& s : shapes) {
    if (s.ShapeType() == TopAbs_FACE) {
      faces.push_back(s);
    }
  }
  return faces;
}

// A point in the middle of the face's parameters and the unit normal
// there, the face's orientation applied; false where there is none.
bool middle_normal(const TopoDS_Face& face, gp_Pnt& point, gp_Vec& normal) {
  double u1 = 0, u2 = 0, v1 = 0, v2 = 0;
  BRepTools::UVBounds(face, u1, u2, v1, v2);
  BRepGProp_Face props(face);
  props.Normal((u1 + u2) / 2, (v1 + v2) / 2, point, normal);
  if (normal.Magnitude() < 1.0e-12) {
    return false;
  }
  normal.Normalize();
  return true;
}

// Whether `image` lies `distance` along the normal of `original` (the
// point that far from its middle is on the image's surface).
bool along_normal(const TopoDS_Face& original, const TopoDS_Shape& image, double distance, double tolerance) {
  gp_Pnt p;
  gp_Vec n;
  if (!middle_normal(original, p, n)) {
    return true;
  }
  const occ::handle<Geom_Surface> target = BRep_Tool::Surface(TopoDS::Face(image));
  if (target.IsNull()) {
    return true;
  }
  GeomAPI_ProjectPointOnSurf projection(p.Translated(n * distance), target);
  return projection.NbPoints() > 0 && projection.LowerDistance() <= tolerance;
}

// --- Ruled Surface

// An edge of the chain and the face it leaves.
struct ChainEdge {
  int edge = -1;
  int face = -1;
};

// The edge indices in chains (each in order; a closed loop from any edge),
// with the vertex each one ends at as its chain runs. Throws where they
// branch.
std::vector<std::vector<std::pair<int, TopoDS_Vertex>>> chained(const Shape& body, const std::vector<int>& edges) {
  ShapeMap vertices;
  std::map<int, std::vector<int>> at; // vertex -> edges there
  std::map<int, std::array<int, 2>> ends;
  for (const int e : edges) {
    TopoDS_Vertex a;
    TopoDS_Vertex b;
    TopExp::Vertices(body.edge(e), a, b);
    if (a.IsNull() || b.IsNull()) {
      throw std::invalid_argument("ruled surface: an edge has no ends");
    }
    const int ia = vertices.Add(a);
    const int ib = vertices.Add(b);
    ends[e] = {ia, ib};
    at[ia].push_back(e);
    if (ib != ia) {
      at[ib].push_back(e);
    }
  }
  for (const auto& [vertex, list] : at) {
    if (list.size() > 2) {
      throw std::invalid_argument("ruled surface: the edges branch; they must join into chains");
    }
  }
  std::vector<std::vector<std::pair<int, TopoDS_Vertex>>> chains;
  std::set<int> used;
  while (used.size() < edges.size()) {
    // Start at an end of an open chain, else anywhere.
    int start = -1;
    int from = -1;
    for (const auto& [vertex, list] : at) {
      if (list.size() == 1 && !used.count(list.front())) {
        start = list.front();
        from = vertex;
        break;
      }
    }
    if (start < 0) {
      for (const int e : edges) {
        if (!used.count(e)) {
          start = e;
          from = ends[e][0];
          break;
        }
      }
    }
    std::vector<std::pair<int, TopoDS_Vertex>> order;
    int current = start;
    int vertex = from;
    while (current >= 0 && !used.count(current)) {
      used.insert(current);
      const std::array<int, 2> e = ends[current];
      vertex = e[0] == vertex ? e[1] : e[0];
      order.emplace_back(current, TopoDS::Vertex(vertices.FindKey(vertex)));
      int next = -1;
      for (const int candidate : at[vertex]) {
        if (!used.count(candidate)) {
          next = candidate;
        }
      }
      current = next;
    }
    chains.push_back(std::move(order));
  }
  return chains;
}

// The direction a strip leaves the edge at curve parameter t.
gp_Dir strip_direction(const RuledSpec& spec, const TopoDS_Edge& edge, const TopoDS_Face& face, double t) {
  const BRepAdaptor_Curve curve(edge);
  gp_Pnt p;
  gp_Vec tangent;
  curve.D1(t, p, tangent);
  if (tangent.Magnitude() < 1.0e-12) {
    throw std::runtime_error("ruled surface: an edge has no tangent");
  }
  tangent.Normalize();
  // The face's continuation away from it: across the edge in the face's
  // parameters, to the side where the face is not.
  double first = 0.0;
  double last = 0.0;
  const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, face, first, last);
  if (pcurve.IsNull()) {
    throw std::runtime_error("ruled surface: an edge has no curve on its face");
  }
  gp_Pnt2d uv;
  gp_Vec2d duv;
  pcurve->D1(t, uv, duv);
  double u1 = 0, u2 = 0, v1 = 0, v2 = 0;
  BRepTools::UVBounds(face, u1, u2, v1, v2);
  const double step = 1.0e-4 * std::max(u2 - u1, v2 - v1);
  gp_Vec2d across(-duv.Y(), duv.X());
  if (across.Magnitude() < 1.0e-15) {
    throw std::runtime_error("ruled surface: an edge has no direction on its face");
  }
  across.Normalize();
  // Which side the face is on, found in the middle of the edge (at an end
  // a step across can land on the next edge).
  gp_Pnt2d mid;
  gp_Vec2d dmid;
  pcurve->D1((first + last) / 2, mid, dmid);
  gp_Vec2d side(-dmid.Y(), dmid.X());
  if (side.Magnitude() > 1.0e-15) {
    side.Normalize();
    const gp_Pnt2d left(mid.X() + side.X() * step, mid.Y() + side.Y() * step);
    if (BRepClass_FaceClassifier(face, left, kTolerance).State() == TopAbs_IN) {
      across.Reverse();
    }
  }
  const BRepAdaptor_Surface surface(face);
  gp_Pnt on;
  gp_Vec du;
  gp_Vec dv;
  surface.D1(uv.X(), uv.Y(), on, du, dv);
  gp_Vec outward = du * across.X() + dv * across.Y();
  outward -= tangent * outward.Dot(tangent);
  if (outward.Magnitude() < 1.0e-12) {
    throw std::runtime_error("ruled surface: the face has no direction across the edge");
  }
  outward.Normalize();
  gp_Vec normal = du.Crossed(dv);
  if (face.Orientation() == TopAbs_REVERSED) {
    normal.Reverse();
  }
  normal.Normalize();
  gp_Vec d;
  switch (spec.mode) {
  case RuledMode::Normal:
    d = normal;
    break;
  case RuledMode::Tangent:
    d = outward;
    break;
  case RuledMode::Direction:
    d = gp_Vec(*spec.direction);
    break;
  case RuledMode::Angle: {
    gp_Vec reference = spec.direction ? gp_Vec(*spec.direction) : normal;
    reference -= tangent * reference.Dot(tangent);
    if (reference.Magnitude() < 1.0e-12) {
      throw std::invalid_argument("ruled surface: the angle's reference runs along an edge");
    }
    reference.Normalize();
    gp_Vec turned = tangent.Crossed(reference);
    if (turned.Dot(outward) < 0.0) {
      turned.Reverse();
    }
    d = reference * std::cos(spec.angle) + turned * std::sin(spec.angle);
    break;
  }
  }
  if (spec.flip) {
    d.Reverse();
  }
  return gp_Dir(d);
}

// The strip off an edge: its points moved by `distance` along the
// directions, the ends' directions `start` and `end` (their lengths
// scale the distance there).
TopoDS_Face strip_face(const RuledSpec& spec, const TopoDS_Edge& edge, const TopoDS_Face& face, const gp_Vec& start,
                       const gp_Vec& end) {
  const BRepAdaptor_Curve curve(edge);
  const double first = curve.FirstParameter();
  const double last = curve.LastParameter();
  const bool straight = curve.GetType() == GeomAbs_Line;
  const int samples = straight ? 1 : 24;
  occ::handle<NCollection_HArray1<gp_Pnt>> points = new NCollection_HArray1<gp_Pnt>(1, samples + 1);
  for (int i = 0; i <= samples; ++i) {
    const double t = first + (last - first) * i / samples;
    gp_Vec d;
    if (i == 0) {
      d = start;
    } else if (i == samples) {
      d = end;
    } else {
      d = gp_Vec(strip_direction(spec, edge, face, t));
    }
    points->SetValue(i + 1, curve.Value(t).Translated(d * spec.distance));
  }
  TopoDS_Edge far_edge;
  if (straight) {
    far_edge = BRepBuilderAPI_MakeEdge(points->Value(1), points->Value(2)).Edge();
  } else {
    GeomAPI_Interpolate interpolate(points, false, kTolerance);
    interpolate.Perform();
    if (!interpolate.IsDone()) {
      throw std::runtime_error("ruled surface: the strip's far edge could not be fitted");
    }
    far_edge = BRepBuilderAPI_MakeEdge(interpolate.Curve()).Edge();
  }
  // The near edge as the curve runs, without its orientation in a wire.
  TopoDS_Edge near_edge = TopoDS::Edge(edge.Oriented(TopAbs_FORWARD));
  const TopoDS_Face strip = BRepFill::Face(near_edge, far_edge);
  if (strip.IsNull()) {
    throw std::runtime_error("ruled surface: a strip could not be built");
  }
  return strip;
}

// --- Extend

// The free edges of openings, as edge indices of the body.
std::vector<int> opening_edges(const Shape& body, const std::vector<OpeningEdges>& openings) {
  std::vector<int> edges;
  if (openings.empty()) {
    return edges;
  }
  const std::vector<std::vector<int>> loops = detail::free_edge_loops(body);
  const detail::EdgeFaces edge_faces(body);
  for (const OpeningEdges& opening : openings) {
    if (opening.index < 0 || static_cast<std::size_t>(opening.index) >= loops.size()) {
      throw std::invalid_argument("the body has no opening " + std::to_string(opening.index) + " (it has " +
                                  std::to_string(loops.size()) + ")");
    }
    std::vector<int> wanted;
    for (const int e : loops[static_cast<std::size_t>(opening.index)]) {
      const std::vector<int> faces = edge_faces.at(e);
      if (opening.faces.empty() ||
          std::any_of(faces.begin(), faces.end(), [&](int f) {
            return std::find(opening.faces.begin(), opening.faces.end(), detail::primary_name(body, f)) !=
                   opening.faces.end();
          })) {
        wanted.push_back(e);
      }
    }
    if (!opening.at.empty()) {
      // The edge nearest each point.
      std::vector<int> nearest;
      for (const gp_Pnt& point : opening.at) {
        const TopoDS_Vertex vertex = BRepBuilderAPI_MakeVertex(point).Vertex();
        int best = -1;
        double distance = 0.0;
        for (const int e : wanted) {
          BRepExtrema_DistShapeShape measure(vertex, body.edge(e));
          if (measure.IsDone() && (best < 0 || measure.Value() < distance)) {
            best = e;
            distance = measure.Value();
          }
        }
        if (best >= 0 && std::find(nearest.begin(), nearest.end(), best) == nearest.end()) {
          nearest.push_back(best);
        }
      }
      wanted = nearest;
    }
    for (const int e : wanted) {
      if (std::find(edges.begin(), edges.end(), e) == edges.end()) {
        edges.push_back(e);
      }
    }
  }
  return edges;
}

// The named edges and the edges of the openings, each once.
std::vector<int> picked_edges(const Shape& body, const std::vector<std::string>& names,
                              const std::vector<OpeningEdges>& openings) {
  std::vector<int> edges;
  if (!names.empty()) {
    edges = detail::resolve_edges(body, names);
  }
  for (const int e : opening_edges(body, openings)) {
    if (std::find(edges.begin(), edges.end(), e) == edges.end()) {
      edges.push_back(e);
    }
  }
  if (edges.empty()) {
    throw std::invalid_argument("no edges selected");
  }
  return edges;
}

enum Side { UMin = 0, UMax = 1, VMin = 2, VMax = 3 };

// The side of the face's parameter range the edge runs along, or -1.
int side_of(const TopoDS_Edge& edge, const TopoDS_Face& face) {
  double first = 0.0;
  double last = 0.0;
  const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, face, first, last);
  if (pcurve.IsNull()) {
    return -1;
  }
  double u1 = 0, u2 = 0, v1 = 0, v2 = 0;
  BRepTools::UVBounds(face, u1, u2, v1, v2);
  const double tu = 1.0e-7 * std::max(1.0, u2 - u1);
  const double tv = 1.0e-7 * std::max(1.0, v2 - v1);
  std::array<bool, 4> on{true, true, true, true};
  for (int i = 0; i <= 4; ++i) {
    const gp_Pnt2d uv = pcurve->Value(first + (last - first) * i / 4);
    on[UMin] = on[UMin] && std::abs(uv.X() - u1) <= tu;
    on[UMax] = on[UMax] && std::abs(uv.X() - u2) <= tu;
    on[VMin] = on[VMin] && std::abs(uv.Y() - v1) <= tv;
    on[VMax] = on[VMax] && std::abs(uv.Y() - v2) <= tv;
  }
  for (int s = 0; s < 4; ++s) {
    if (on[static_cast<std::size_t>(s)]) {
      return s;
    }
  }
  return -1;
}

double face_area(const TopoDS_Shape& face) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(face, props);
  return props.Mass();
}

// Whether the face is bounded by its surface's parameter range only (one
// wire, the whole range's area): what lies outside the trims of another
// face would come back when its range is widened.
bool untrimmed(const TopoDS_Face& face) {
  double u1 = 0, u2 = 0, v1 = 0, v2 = 0;
  BRepTools::UVBounds(face, u1, u2, v1, v2);
  TopLoc_Location location;
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face, location);
  if (surface.IsNull()) {
    return false;
  }
  int wires = 0;
  for (TopExp_Explorer w(face, TopAbs_WIRE); w.More(); w.Next()) {
    ++wires;
  }
  const BRepBuilderAPI_MakeFace whole(surface, u1, u2, v1, v2, kTolerance);
  return wires == 1 && whole.IsDone() &&
         std::abs(face_area(whole.Face()) - face_area(face)) <= 1.0e-6 * std::max(1.0, face_area(face));
}

// Whether the face lies in a plane.
bool planar(const TopoDS_Face& face) {
  return BRepAdaptor_Surface(face, false).GetType() == GeomAbs_Plane;
}

// The smallest angle, in degrees, at which a side of a planar face's
// extension may meet the extension's end line (mitcad#2056): a neighbouring
// edge continued, or the mitre of two picked edges, that meets it more
// flatly would make a long sliver, and the side is square to the edge
// instead.
constexpr double kMinEndAngleDegrees = 15.0;

// One end of a picked straight edge of a planar face, and where the side of
// its extension reaches the end line.
struct PlanarEnd {
  TopoDS_Vertex vertex;
  gp_Pnt at;  // the vertex's point
  gp_Pnt far; // on the end line
  // How `far` was found: square to the edge, along the neighbouring
  // edge continued, or at the mitre with the picked neighbour `partner`.
  enum Kind { Square, Continued, Mitre } kind = Square;
  int partner = -1; // with Mitre: the neighbour's index among the picked edges
};

// A picked straight edge of a planar face: its two ends (curve parameters
// first and last) and the unit direction square to it, in the plane, away
// from the face.
struct PlanarStrip {
  TopoDS_Edge edge;
  gp_Vec out;
  std::array<PlanarEnd, 2> ends;
};

// The face's one other edge at `vertex` besides `edge`; null when there is
// none or more than one.
TopoDS_Edge neighbour_at(const TopoDS_Face& face, const TopoDS_Edge& edge, const TopoDS_Vertex& vertex) {
  TopoDS_Edge found;
  for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
    const TopoDS_Edge& other = TopoDS::Edge(e.Current());
    if (other.IsSame(edge) || (!found.IsNull() && other.IsSame(found))) {
      continue;
    }
    TopoDS_Vertex a;
    TopoDS_Vertex b;
    TopExp::Vertices(other, a, b);
    if (!a.IsSame(vertex) && !b.IsSame(vertex)) {
      continue;
    }
    if (!found.IsNull()) {
      return TopoDS_Edge();
    }
    found = other;
  }
  return found;
}

// The direction in which the edge `neighbour` runs on past its end at
// `vertex` (its tangent there, leaving the edge), put into the plane of
// `normal`; false for a closed edge or a tangent of no length.
bool continuation(const TopoDS_Edge& neighbour, const TopoDS_Vertex& vertex, const gp_Dir& normal, gp_Vec& along) {
  TopoDS_Vertex first;
  TopoDS_Vertex last;
  TopExp::Vertices(TopoDS::Edge(neighbour.Oriented(TopAbs_FORWARD)), first, last);
  if (first.IsSame(last)) {
    return false;
  }
  const BRepAdaptor_Curve curve(neighbour);
  gp_Pnt p;
  gp_Vec tangent;
  if (last.IsSame(vertex)) {
    curve.D1(curve.LastParameter(), p, tangent);
  } else if (first.IsSame(vertex)) {
    curve.D1(curve.FirstParameter(), p, tangent);
    tangent.Reverse();
  } else {
    return false;
  }
  tangent -= gp_Vec(normal) * tangent.Dot(gp_Vec(normal));
  if (tangent.Magnitude() < 1.0e-12) {
    return false;
  }
  along = tangent.Normalized();
  return true;
}

// The ends of the strips past the picked straight edges of a planar face
// (mitcad#2056): each side of a strip continues the face's neighbouring
// edge where that one is not picked, meets the neighbouring strip at the
// mitre of their end lines where it is, and is square to the edge where
// neither can be used: the continuation turns back into the face (a reflex
// corner) or meets the end line more flatly than kMinEndAngleDegrees, or
// the strip's two sides would cross before the end line.
std::vector<PlanarStrip> planar_strips(const TopoDS_Face& face, const std::vector<TopoDS_Edge>& edges,
                                       const gp_Dir& normal, double distance) {
  std::vector<PlanarStrip> strips;
  for (const TopoDS_Edge& edge : edges) {
    const BRepAdaptor_Curve curve(edge);
    if (curve.GetType() != GeomAbs_Line) {
      throw std::invalid_argument("unsupported: extend of a curved edge of a face trimmed inside its plane");
    }
    PlanarStrip strip;
    strip.edge = edge;
    strip.out = gp_Vec(detail::across_edge(edge, face, (curve.FirstParameter() + curve.LastParameter()) / 2));
    TopExp::Vertices(TopoDS::Edge(edge.Oriented(TopAbs_FORWARD)), strip.ends[0].vertex, strip.ends[1].vertex);
    if (strip.ends[0].vertex.IsNull() || strip.ends[1].vertex.IsNull() ||
        strip.ends[0].vertex.IsSame(strip.ends[1].vertex)) {
      throw std::runtime_error("extend: the face's extension past an edge could not be built");
    }
    for (PlanarEnd& end : strip.ends) {
      end.at = BRep_Tool::Pnt(end.vertex);
      end.far = end.at.Translated(strip.out * distance);
    }
    strips.push_back(strip);
  }
  const double flattest = std::sin(kMinEndAngleDegrees * std::acos(-1.0) / 180.0);
  for (std::size_t i = 0; i < strips.size(); ++i) {
    PlanarStrip& strip = strips[i];
    for (PlanarEnd& end : strip.ends) {
      const TopoDS_Edge neighbour = neighbour_at(face, strip.edge, end.vertex);
      if (neighbour.IsNull()) {
        continue;
      }
      int picked = -1;
      for (std::size_t k = 0; k < strips.size(); ++k) {
        if (k != i && strips[k].edge.IsSame(neighbour)) {
          picked = static_cast<int>(k);
        }
      }
      if (picked >= 0) {
        // Both end lines, `distance` from their edges, meet at the mitre:
        // the point m with m.out = m.other = distance.
        const gp_Vec& other = strips[static_cast<std::size_t>(picked)].out;
        const double cosine = strip.out.Dot(other);
        if (1.0 + cosine <= 1.0e-12) {
          continue;
        }
        const gp_Vec mitre = (strip.out + other) * (distance / (1.0 + cosine));
        if (mitre.Normalized().Dot(strip.out) >= flattest) {
          end.far = end.at.Translated(mitre);
          end.kind = PlanarEnd::Mitre;
          end.partner = picked;
        }
        continue;
      }
      gp_Vec along;
      if (!continuation(neighbour, end.vertex, normal, along)) {
        continue;
      }
      const double rise = along.Dot(strip.out);
      if (rise >= flattest) {
        end.far = end.at.Translated(along * (distance / rise));
        end.kind = PlanarEnd::Continued;
      }
    }
  }
  // A strip whose sides would cross before its end line (both turned in,
  // the edge short) is square at both ends, and so is the picked neighbour
  // at a mitre it gives up; until no strip crosses (square sides never do).
  bool changed = true;
  while (changed) {
    changed = false;
    for (std::size_t i = 0; i < strips.size(); ++i) {
      PlanarStrip& strip = strips[i];
      const gp_Vec along_edge(strip.ends[0].at, strip.ends[1].at);
      const gp_Vec along_end(strip.ends[0].far, strip.ends[1].far);
      if (along_end.Dot(along_edge) > kSewing * along_edge.Magnitude()) {
        continue;
      }
      for (PlanarEnd& end : strip.ends) {
        if (end.kind == PlanarEnd::Mitre) {
          for (PlanarEnd& other : strips[static_cast<std::size_t>(end.partner)].ends) {
            if (other.kind == PlanarEnd::Mitre && other.partner == static_cast<int>(i) &&
                other.vertex.IsSame(end.vertex)) {
              other.kind = PlanarEnd::Square;
              other.far = other.at.Translated(strips[static_cast<std::size_t>(end.partner)].out * distance);
            }
          }
        }
        if (end.kind != PlanarEnd::Square) {
          end.kind = PlanarEnd::Square;
          end.far = end.at.Translated(strip.out * distance);
          changed = true;
        }
      }
    }
  }
  return strips;
}

// A planar face trimmed inside its plane (mitcad#1955, GitHub
// mitcad/Mitcad#407: a wall cut at a slant): the plane continued past each
// straight edge up to the line `distance` from it, along the face's
// neighbouring edges continued and mitred where two picked edges meet
// (mitcad#2056, GitHub mitcad/Mitcad#462; see planar_strips), joined to
// the face as one face. Every mode continues a plane alike.
TopoDS_Face extended_planar_face(const TopoDS_Face& face, const std::vector<TopoDS_Edge>& edges, double distance) {
  const BRepAdaptor_Surface surface(face, true);
  const gp_Pln plane = surface.Plane();
  gp_Dir normal = plane.Axis().Direction();
  if (face.Orientation() == TopAbs_REVERSED) {
    normal.Reverse();
  }
  // The strips on a plane facing as the face does.
  const gp_Pln facing(plane.Location(), normal);
  // The fuse puts p-curves on the strips' surfaces onto the edges it shares
  // with them, and the unify rebuilds what it merges: a copy of the face,
  // never the input's own (MITCAD_CHECK_INPUTS). The input face is only
  // read, for the directions across its edges and its neighbouring edges.
  TopoDS_Shape joined = BRepBuilderAPI_Copy(face, true, false).Shape();
  for (const PlanarStrip& strip : planar_strips(face, edges, normal, distance)) {
    const PlanarEnd& a = strip.ends[0];
    const PlanarEnd& b = strip.ends[1];
    BRepBuilderAPI_MakePolygon outline(a.at, b.at, b.far, a.far, true);
    if (!outline.IsDone()) {
      throw std::runtime_error("extend: the face's extension past an edge could not be built");
    }
    const BRepBuilderAPI_MakeFace piece(facing, outline.Wire(), true);
    if (!piece.IsDone()) {
      throw std::runtime_error("extend: the face's extension past an edge could not be built");
    }
    BRepAlgoAPI_Fuse fuse(joined, piece.Face());
    if (!fuse.IsDone() || fuse.HasErrors()) {
      throw std::runtime_error("extend: the extension could not be joined to the face");
    }
    joined = fuse.Shape();
  }
  // The face and its strips, all in one plane, as one face.
  ShapeUpgrade_UnifySameDomain unify(joined, true, true, false);
  unify.Build();
  TopoDS_Face result;
  int count = 0;
  for (TopExp_Explorer f(unify.Shape(), TopAbs_FACE); f.More(); f.Next()) {
    result = TopoDS::Face(f.Current());
    ++count;
  }
  if (count != 1) {
    throw std::runtime_error("extend: the face and its extension did not join into one face");
  }
  gp_Dir made = BRepAdaptor_Surface(result, true).Plane().Axis().Direction();
  if (result.Orientation() == TopAbs_REVERSED) {
    made.Reverse();
  }
  if (made.Dot(normal) < 0.0) {
    result.Reverse();
  }
  return result;
}

// The face extended past the sides by `distance` along its surface.
TopoDS_Face extended_face(const TopoDS_Face& face, const std::array<bool, 4>& sides, double distance,
                          ExtendMode mode) {
  double u1 = 0, u2 = 0, v1 = 0, v2 = 0;
  BRepTools::UVBounds(face, u1, u2, v1, v2);
  TopLoc_Location location;
  const occ::handle<Geom_Surface> surface = BRep_Tool::Surface(face, location);
  if (surface.IsNull()) {
    throw std::invalid_argument("extend: a face has no surface");
  }
  // Only a face bounded by its parameter range: what lies outside its
  // trims would come back.
  if (!untrimmed(face)) {
    throw std::invalid_argument("unsupported: extend of a face that is trimmed inside its surface's "
                                "parameter range");
  }
  TopoDS_Face result;
  if (mode == ExtendMode::Natural && !surface->IsKind(STANDARD_TYPE(Geom_BSplineSurface))) {
    // The surface continued: the parameter range widened by the distance
    // over the speed of the parameter at each side's middle.
    const BRepAdaptor_Surface adaptor(face, false);
    std::array<double, 4> range{u1, u2, v1, v2};
    for (int s = 0; s < 4; ++s) {
      if (!sides[static_cast<std::size_t>(s)]) {
        continue;
      }
      const bool inU = s == UMin || s == UMax;
      const double u = s == UMin ? u1 : s == UMax ? u2 : (u1 + u2) / 2;
      const double v = s == VMin ? v1 : s == VMax ? v2 : (v1 + v2) / 2;
      gp_Pnt p;
      gp_Vec du;
      gp_Vec dv;
      adaptor.D1(u, v, p, du, dv);
      const double speed = (inU ? du : dv).Magnitude();
      if (speed < 1.0e-12) {
        throw std::invalid_argument("extend: the surface does not move across a side");
      }
      const double delta = distance / speed;
      range[static_cast<std::size_t>(s)] += (s == UMin || s == VMin) ? -delta : delta;
    }
    if ((surface->IsUPeriodic() && range[UMax] - range[UMin] > surface->UPeriod() + 1.0e-9) ||
        (surface->IsVPeriodic() && range[VMax] - range[VMin] > surface->VPeriod() + 1.0e-9)) {
      throw std::invalid_argument("extend: the extension would run round the surface onto itself");
    }
    const BRepBuilderAPI_MakeFace extended(surface, range[UMin], range[UMax], range[VMin], range[VMax], kTolerance);
    if (!extended.IsDone()) {
      throw std::runtime_error("extend: the surface could not be continued");
    }
    result = extended.Face();
  } else {
    // A B-spline of the face, extended by GeomLib (natural: the
    // polynomials continued, of the most derivatives GeomLib keeps).
    occ::handle<Geom_BoundedSurface> bounded =
        GeomConvert::SurfaceToBSplineSurface(new Geom_RectangularTrimmedSurface(surface, u1, u2, v1, v2));
    const int continuity = mode == ExtendMode::Tangent ? 1 : mode == ExtendMode::Curvature ? 2 : 3;
    for (int s = 0; s < 4; ++s) {
      if (sides[static_cast<std::size_t>(s)]) {
        GeomLib::ExtendSurfByLength(bounded, distance, continuity, s == UMin || s == UMax, s == UMax || s == VMax);
      }
    }
    const BRepBuilderAPI_MakeFace extended(bounded, kTolerance);
    if (!extended.IsDone()) {
      throw std::runtime_error("extend: the surface could not be extended");
    }
    result = extended.Face();
  }
  result.Location(location);
  if (face.Orientation() == TopAbs_REVERSED) {
    result.Reverse();
  }
  return result;
}

// --- Extend past trimmed boundaries (mitcad#2525, GitHub mitcad/Mitcad#689)
//
// A face whose picked edges do not run along the sides of its surface's
// parameter range, or whose surface reaches past its trims (a rounded
// corner of an offset shell, a curved edge of a plane), is extended
// Natural along its surface: the surface continued as it runs (an analytic
// surface is itself, a B-spline its end spans' polynomials carried on, an
// offset surface the offset of its continued basis), and on it, from each
// point of the edge, a geodesic square to the edge `distance` long. The
// face keeps its other edges (its trims elsewhere); the picked edges give
// way to the far curve through the geodesics' ends and two sides, one at
// each end of a run of picked edges, that continue the face's neighbouring
// edges as the planar path's sides do.

// Far corners of the extensions of neighbouring faces closer than this are
// sewn together: the import's geometric tolerance (mitcad#224). Two
// surfaces tangent along their shared edge, each continued its own way,
// part a little past the edge's end.
constexpr double kJoin = 0.01;

// The tolerance of the edges built on a continued surface.
constexpr double kEdgeTolerance = 1.0e-6;

// The length of a geodesic's steps (fourth order: 0.05 mm keeps a path on
// a 1 mm radius within a nanometre).
constexpr double kWalkStep = 0.05;

// A control point in homogeneous coordinates: x w, y w, z w, w.
using Homogeneous = std::array<double, 4>;

// The control points of the Bezier polynomial `points` over [0, t] (`left`)
// or over [t, 1]: its de Casteljau triangle at t. A t outside [0, 1]
// carries the polynomial on past that end.
std::vector<Homogeneous> subdivided(std::vector<Homogeneous> points, double t, bool left) {
  const std::size_t n = points.size();
  std::vector<Homogeneous> result(n);
  for (std::size_t level = 0; level < n; ++level) {
    if (left) {
      result[level] = points.front();
    } else {
      result[n - 1 - level] = points[n - 1 - level];
    }
    for (std::size_t i = 0; i + 1 < n - level; ++i) {
      for (std::size_t k = 0; k < 4; ++k) {
        points[i][k] = (1.0 - t) * points[i][k] + t * points[i + 1][k];
      }
    }
  }
  return result;
}

// The B-spline continued `delta` (in its parameter) past the start or the
// end (`after`) of its U or V range, not periodic that way: its end span's
// polynomial carried on. Over its own range it is the same surface of the
// same parameters.
occ::handle<Geom_BSplineSurface> continued(const occ::handle<Geom_BSplineSurface>& spline, bool in_u, bool after,
                                           double delta) {
  const occ::handle<Geom_BSplineSurface> s = occ::down_cast<Geom_BSplineSurface>(spline->Copy());
  const int degree = in_u ? s->UDegree() : s->VDegree();
  const int knots = in_u ? s->NbUKnots() : s->NbVKnots();
  // The end span's inner knot raised to the degree: the span's own poles
  // are then its polynomial's (a Bezier piece).
  const int inner = after ? knots - 1 : 2;
  int inner_multiplicity = degree;
  if (knots > 2) {
    inner_multiplicity = in_u ? s->UMultiplicity(inner) : s->VMultiplicity(inner);
  }
  if (inner_multiplicity < degree) {
    if (in_u) {
      s->IncreaseUMultiplicity(inner, degree);
    } else {
      s->IncreaseVMultiplicity(inner, degree);
    }
  }
  const int nu = s->NbUPoles();
  const int nv = s->NbVPoles();
  NCollection_Array2<gp_Pnt> poles(1, nu, 1, nv);
  NCollection_Array2<double> weights(1, nu, 1, nv);
  for (int i = 1; i <= nu; ++i) {
    for (int j = 1; j <= nv; ++j) {
      poles(i, j) = s->Pole(i, j);
      weights(i, j) = s->Weight(i, j);
    }
  }
  NCollection_Array1<double> uknots(1, s->NbUKnots());
  NCollection_Array1<int> umults(1, s->NbUKnots());
  for (int i = 1; i <= s->NbUKnots(); ++i) {
    uknots(i) = s->UKnot(i);
    umults(i) = s->UMultiplicity(i);
  }
  NCollection_Array1<double> vknots(1, s->NbVKnots());
  NCollection_Array1<int> vmults(1, s->NbVKnots());
  for (int i = 1; i <= s->NbVKnots(); ++i) {
    vknots(i) = s->VKnot(i);
    vmults(i) = s->VMultiplicity(i);
  }
  NCollection_Array1<double>& along = in_u ? uknots : vknots;
  const double lo = along(after ? knots - 1 : 1);
  const double hi = along(after ? knots : 2);
  // The span [lo, hi] as [0, 1]: the polynomial over [lo, hi + delta] is
  // its left part at t > 1, over [lo - delta, hi] its right part at t < 0.
  const double t = after ? (hi + delta - lo) / (hi - lo) : -delta / (hi - lo);
  const int count = in_u ? nu : nv;
  const int across = in_u ? nv : nu;
  const int first = after ? count - degree : 1;
  for (int j = 1; j <= across; ++j) {
    std::vector<Homogeneous> bezier;
    for (int i = first; i <= first + degree; ++i) {
      const gp_Pnt& p = in_u ? poles(i, j) : poles(j, i);
      const double w = in_u ? weights(i, j) : weights(j, i);
      bezier.push_back({p.X() * w, p.Y() * w, p.Z() * w, w});
    }
    const std::vector<Homogeneous> part = subdivided(bezier, t, after);
    for (int k = 0; k <= degree; ++k) {
      const Homogeneous& h = part[static_cast<std::size_t>(k)];
      if (!(h[3] > 0.0)) {
        throw std::runtime_error("extend: the surface could not be continued (its weights turn negative)");
      }
      const gp_Pnt p(h[0] / h[3], h[1] / h[3], h[2] / h[3]);
      if (in_u) {
        poles(first + k, j) = p;
        weights(first + k, j) = h[3];
      } else {
        poles(j, first + k) = p;
        weights(j, first + k) = h[3];
      }
    }
  }
  along(after ? knots : 1) = after ? hi + delta : lo - delta;
  occ::handle<Geom_BSplineSurface> result;
  if (s->IsURational() || s->IsVRational()) {
    result = new Geom_BSplineSurface(poles, weights, uknots, vknots, umults, vmults, s->UDegree(), s->VDegree(),
                                     s->IsUPeriodic(), s->IsVPeriodic());
  } else {
    result = new Geom_BSplineSurface(poles, uknots, vknots, umults, vmults, s->UDegree(), s->VDegree(),
                                     s->IsUPeriodic(), s->IsVPeriodic());
  }
  // The inner knot back to its own multiplicity: the surface did not
  // change there (left as it is where the removal finds it would).
  if (knots > 2 && inner_multiplicity < degree) {
    if (in_u) {
      result->RemoveUKnot(inner, inner_multiplicity, Precision::Confusion());
    } else {
      result->RemoveVKnot(inner, inner_multiplicity, Precision::Confusion());
    }
  }
  return result;
}

// How far past the start or the end of its U or V parameters the B-spline
// must be continued to run `reach` on along the surface: by the slowest
// speed of the parameter across that end, no more than 19 times the end
// span (the continued span's control points grow as that ratio to the
// power of the degree).
double parameter_reach(const Geom_BSplineSurface& spline, bool in_u, bool after, double reach) {
  double u1 = 0, u2 = 0, v1 = 0, v2 = 0;
  spline.Bounds(u1, u2, v1, v2);
  double slowest = std::numeric_limits<double>::max();
  double fastest = 0.0;
  for (int i = 0; i <= 8; ++i) {
    const double s = i / 8.0;
    const double u = in_u ? (after ? u2 : u1) : u1 + (u2 - u1) * s;
    const double v = in_u ? v1 + (v2 - v1) * s : (after ? v2 : v1);
    gp_Pnt p;
    gp_Vec du;
    gp_Vec dv;
    spline.D1(u, v, p, du, dv);
    const double speed = (in_u ? du : dv).Magnitude();
    slowest = std::min(slowest, speed);
    fastest = std::max(fastest, speed);
  }
  const int knots = in_u ? spline.NbUKnots() : spline.NbVKnots();
  const auto knot = [&](int i) { return in_u ? spline.UKnot(i) : spline.VKnot(i); };
  const double span = after ? knot(knots) - knot(knots - 1) : knot(2) - knot(1);
  const double speed = std::max(slowest, 1.0e-3 * fastest);
  if (!(speed > 1.0e-12)) {
    return span;
  }
  return std::min(1.5 * reach / speed, 19.0 * span);
}

// The surface continued past its bounds as it runs (Natural), at least
// `reach` along it where it is bounded: an analytic surface is itself
// (unbounded or periodic, a sphere stops at its poles), a B-spline or a
// Bezier its end spans' polynomials carried on, an offset surface the
// offset of its basis continued. Of the same parameters over its own
// range, so that a face's curves on it stay.
occ::handle<Geom_Surface> natural_surface(const occ::handle<Geom_Surface>& surface, double reach) {
  const occ::handle<Geom_RectangularTrimmedSurface> trimmed = occ::down_cast<Geom_RectangularTrimmedSurface>(surface);
  if (!trimmed.IsNull()) {
    return natural_surface(trimmed->BasisSurface(), reach);
  }
  const occ::handle<Geom_OffsetSurface> offset = occ::down_cast<Geom_OffsetSurface>(surface);
  if (!offset.IsNull()) {
    const occ::handle<Geom_Surface> basis = natural_surface(offset->BasisSurface(), reach);
    if (basis == offset->BasisSurface()) {
      return surface;
    }
    return new Geom_OffsetSurface(basis, offset->Offset(), true);
  }
  occ::handle<Geom_BSplineSurface> spline = occ::down_cast<Geom_BSplineSurface>(surface);
  if (spline.IsNull()) {
    const occ::handle<Geom_BezierSurface> bezier = occ::down_cast<Geom_BezierSurface>(surface);
    if (bezier.IsNull()) {
      return surface;
    }
    spline = GeomConvert::SurfaceToBSplineSurface(bezier);
  }
  const occ::handle<Geom_BSplineSurface> own = spline;
  for (const bool in_u : {true, false}) {
    if (in_u ? spline->IsUPeriodic() : spline->IsVPeriodic()) {
      continue;
    }
    for (const bool after : {false, true}) {
      spline = continued(spline, in_u, after, parameter_reach(*spline, in_u, after, reach));
    }
  }
  // The same surface over its own range.
  double u1 = 0, u2 = 0, v1 = 0, v2 = 0;
  own->Bounds(u1, u2, v1, v2);
  for (int i = 0; i <= 4; ++i) {
    for (int j = 0; j <= 4; ++j) {
      const double u = u1 + (u2 - u1) * i / 4.0;
      const double v = v1 + (v2 - v1) * j / 4.0;
      const gp_Pnt was = own->Value(u, v);
      if (was.Distance(spline->Value(u, v)) > 1.0e-6 + 1.0e-12 * gp_Vec(was.XYZ()).Magnitude()) {
        throw std::runtime_error("extend: the surface could not be continued");
      }
    }
  }
  return spline;
}

// Geodesics on a surface: paths that run straight on along it, in its
// parameters (the geodesic equations, fourth-order Runge-Kutta by arc
// length).
class Walk {
public:
  explicit Walk(occ::handle<Geom_Surface> surface) : m_surface(std::move(surface)) {
    m_surface->Bounds(m_u1, m_u2, m_v1, m_v2);
  }

  gp_Pnt point(const gp_Pnt2d& uv) const { return m_surface->Value(uv.X(), uv.Y()); }

  // The unit normal at uv, of the surface's parameters.
  gp_Vec normal(const gp_Pnt2d& uv) const {
    gp_Pnt p;
    gp_Vec du;
    gp_Vec dv;
    m_surface->D1(uv.X(), uv.Y(), p, du, dv);
    const gp_Vec n = du.Crossed(dv);
    if (n.Magnitude() < 1.0e-12) {
      throw std::runtime_error("extend: the surface has no normal where the extension starts");
    }
    return n.Normalized();
  }

  // The geodesic leaving uv along `direction` (put into the tangent
  // plane), `length` long: its points in the parameters, uv first.
  std::vector<gp_Pnt2d> path(const gp_Pnt2d& uv, const gp_Vec& direction, double length) const {
    gp_Pnt p;
    gp_Vec su;
    gp_Vec sv;
    m_surface->D1(uv.X(), uv.Y(), p, su, sv);
    const gp_Vec n = normal(uv);
    gp_Vec d = direction - n * direction.Dot(n);
    if (d.Magnitude() < 1.0e-12) {
      throw std::runtime_error("extend: a direction across an edge leaves the surface");
    }
    d.Normalize();
    const double e = su.Dot(su);
    const double f = su.Dot(sv);
    const double g = sv.Dot(sv);
    const double det = e * g - f * f;
    const double a = d.Dot(su);
    const double b = d.Dot(sv);
    State y{uv.X(), uv.Y(), (g * a - f * b) / det, (e * b - f * a) / det};
    const int steps = std::clamp(static_cast<int>(std::ceil(length / kWalkStep)), 16, 512);
    const double h = length / steps;
    std::vector<gp_Pnt2d> points{uv};
    for (int i = 0; i < steps; ++i) {
      const State k1 = rate(y);
      const State k2 = rate(moved(y, k1, h / 2));
      const State k3 = rate(moved(y, k2, h / 2));
      const State k4 = rate(moved(y, k3, h));
      for (std::size_t k = 0; k < 4; ++k) {
        y[k] += h / 6.0 * (k1[k] + 2.0 * k2[k] + 2.0 * k3[k] + k4[k]);
      }
      inside(y[0], y[1]);
      points.emplace_back(y[0], y[1]);
    }
    return points;
  }

private:
  // u, v and their rates by arc length.
  using State = std::array<double, 4>;

  static State moved(const State& y, const State& rate, double h) {
    return {y[0] + rate[0] * h, y[1] + rate[1] * h, y[2] + rate[2] * h, y[3] + rate[3] * h};
  }

  // The geodesic's rates: the surface's acceleration along the path has
  // no part in its tangent plane.
  State rate(const State& y) const {
    inside(y[0], y[1]);
    gp_Pnt p;
    gp_Vec su;
    gp_Vec sv;
    gp_Vec suu;
    gp_Vec svv;
    gp_Vec suv;
    m_surface->D2(y[0], y[1], p, su, sv, suu, svv, suv);
    const double e = su.Dot(su);
    const double f = su.Dot(sv);
    const double g = sv.Dot(sv);
    const double det = e * g - f * f;
    if (!(det > 1.0e-18 * std::max(1.0, e * g))) {
      throw std::runtime_error("extend: the surface has no direction where the extension reaches");
    }
    const gp_Vec acceleration = suu * (y[2] * y[2]) + suv * (2.0 * y[2] * y[3]) + svv * (y[3] * y[3]);
    const double ru = -acceleration.Dot(su);
    const double rv = -acceleration.Dot(sv);
    return {y[2], y[3], (g * ru - f * rv) / det, (e * rv - f * ru) / det};
  }

  void inside(double u, double v) const {
    const double tu = 1.0e-9 * std::max(1.0, std::abs(u));
    const double tv = 1.0e-9 * std::max(1.0, std::abs(v));
    if ((!m_surface->IsUPeriodic() && (u < m_u1 - tu || u > m_u2 + tu)) ||
        (!m_surface->IsVPeriodic() && (v < m_v1 - tv || v > m_v2 + tv))) {
      throw std::invalid_argument("unsupported: extend past where the face's surface ends");
    }
  }

  occ::handle<Geom_Surface> m_surface;
  double m_u1 = 0.0;
  double m_u2 = 0.0;
  double m_v1 = 0.0;
  double m_v2 = 0.0;
};

// The tangent of the edge, as its wire runs, where the wire enters it or
// leaves it (`at_end`).
gp_Vec running(const TopoDS_Edge& edge, bool at_end) {
  const BRepAdaptor_Curve curve(edge);
  const bool reversed = edge.Orientation() == TopAbs_REVERSED;
  gp_Pnt p;
  gp_Vec d;
  curve.D1(at_end != reversed ? curve.LastParameter() : curve.FirstParameter(), p, d);
  if (reversed) {
    d.Reverse();
  }
  return d;
}

// A picked edge of a curved face's wire and its extension.
struct Reach {
  TopoDS_Edge edge; // as the wire runs
  occ::handle<Geom2d_Curve> pcurve;
  double from = 0.0; // its parameters where the wire enters it
  double to = 0.0;   // and leaves it
  std::vector<gp_Pnt2d> far; // `distance` across it, as the wire runs
  gp_Vec out_from;           // across it, away from the face, at its ends
  gp_Vec out_to;
};

Reach reach_of(const TopoDS_Edge& edge, const TopoDS_Face& face, const Walk& walk, double distance) {
  Reach reach;
  reach.edge = edge;
  double first = 0.0;
  double last = 0.0;
  reach.pcurve = BRep_Tool::CurveOnSurface(edge, face, first, last);
  if (reach.pcurve.IsNull()) {
    throw std::runtime_error("extend: an edge has no curve on its face");
  }
  const bool reversed = edge.Orientation() == TopAbs_REVERSED;
  reach.from = reversed ? last : first;
  reach.to = reversed ? first : last;
  constexpr int samples = 32;
  for (int i = 0; i <= samples; ++i) {
    const double t = reach.from + (reach.to - reach.from) * i / samples;
    const gp_Vec out(detail::across_edge(edge, face, t));
    reach.far.push_back(walk.path(reach.pcurve->Value(t), out, distance).back());
    if (i == 0) {
      reach.out_from = out;
    }
    if (i == samples) {
      reach.out_to = out;
    }
  }
  return reach;
}

// sin(kMinEndAngleDegrees): a side continues a neighbouring edge, and two
// picked edges meet in a mitre, only as steep as this to the end line.
double flattest() { return std::sin(kMinEndAngleDegrees * std::acos(-1.0) / 180.0); }

// Where the extensions of two picked edges of a face that follow each
// other in a wire meet, `a` entering their vertex and `b` leaving it: the
// mitre, along the geodesic between their directions across them. Smooth
// there, `distance` across both.
gp_Pnt2d joint(const Reach& a, const Reach& b, const Walk& walk, double distance) {
  const gp_Pnt2d uv = a.pcurve->Value(a.to);
  const double cosine = std::clamp(a.out_to.Dot(b.out_from), -1.0, 1.0);
  if (cosine < 1.0 - 1.0e-6) {
    if (running(b.edge, false).Dot(a.out_to) > 0.0) {
      throw std::invalid_argument("unsupported: extend round an inner corner between picked edges of a curved face");
    }
    // The mitre meets the end lines at half the turn.
    if (std::sqrt((1.0 + cosine) / 2.0) < flattest()) {
      throw std::invalid_argument("unsupported: extend round a sharp corner between picked edges of a curved face");
    }
  }
  return walk.path(uv, a.out_to + b.out_from, distance * std::sqrt(2.0 / (1.0 + cosine))).back();
}

// The side of a run of picked edges at its end `vertex` (at uv), where the
// face's unpicked edge `neighbour` meets it: the geodesic that continues
// the neighbour up to the end line (as far as `distance` across the
// edge), or square to the edge (`out`, away from the face) where the
// neighbour turns back into the face or meets the end line more flatly
// than kMinEndAngleDegrees.
std::vector<gp_Pnt2d> side(const TopoDS_Edge& neighbour, const TopoDS_Vertex& vertex, const gp_Pnt2d& uv,
                           const gp_Vec& out, const Walk& walk, double distance) {
  gp_Vec along;
  if (!neighbour.IsNull() && !BRep_Tool::Degenerated(neighbour) &&
      continuation(neighbour, vertex, gp_Dir(walk.normal(uv)), along)) {
    const double rise = along.Dot(out);
    if (rise >= flattest()) {
      return walk.path(uv, along, distance / rise);
    }
  }
  return walk.path(uv, out, distance);
}

TopoDS_Vertex vertex_at(const gp_Pnt& point) {
  TopoDS_Vertex vertex;
  BRep_Builder().MakeVertex(vertex, point, kEdgeTolerance);
  return vertex;
}

// An edge of the face (built on its continued surface) from `from` to `to`
// through the points in its parameters.
TopoDS_Edge edge_through(const std::vector<gp_Pnt2d>& points, const TopoDS_Vertex& from, const TopoDS_Vertex& to,
                         const TopoDS_Face& face) {
  std::vector<gp_Pnt2d> kept;
  for (const gp_Pnt2d& p : points) {
    if (kept.empty() || kept.back().Distance(p) > 1.0e-12) {
      kept.push_back(p);
    }
  }
  if (kept.size() < 2) {
    throw std::runtime_error("extend: an edge of the extension has no length");
  }
  occ::handle<NCollection_HArray1<gp_Pnt2d>> array =
      new NCollection_HArray1<gp_Pnt2d>(1, static_cast<int>(kept.size()));
  for (std::size_t i = 0; i < kept.size(); ++i) {
    array->SetValue(static_cast<int>(i) + 1, kept[i]);
  }
  Geom2dAPI_Interpolate interpolate(array, false, 1.0e-12);
  interpolate.Perform();
  if (!interpolate.IsDone()) {
    throw std::runtime_error("extend: an edge of the extension could not be fitted");
  }
  const occ::handle<Geom2d_BSplineCurve> curve = interpolate.Curve();
  BRep_Builder builder;
  TopoDS_Edge edge;
  builder.MakeEdge(edge);
  builder.UpdateEdge(edge, curve, face, kEdgeTolerance);
  builder.Add(edge, from.Oriented(TopAbs_FORWARD));
  builder.Add(edge, to.Oriented(TopAbs_REVERSED));
  builder.Range(edge, curve->FirstParameter(), curve->LastParameter());
  if (!BRepLib::BuildCurve3d(edge, kEdgeTolerance)) {
    throw std::runtime_error("extend: an edge of the extension could not be built");
  }
  return edge;
}

bool among(const std::vector<TopoDS_Shape>& shapes, const TopoDS_Shape& shape) {
  return std::any_of(shapes.begin(), shapes.end(), [&](const TopoDS_Shape& s) { return s.IsSame(shape); });
}

// A wire of the face (`forward`: the face's own orientation) for `result`
// (the face on its continued surface): each run of picked edges replaced
// by a side from its first vertex, the far curves and a side back to its
// last vertex; a wire of picked edges only by its far curves.
TopoDS_Wire extended_wire(const TopoDS_Wire& wire, const TopoDS_Face& forward, const TopoDS_Face& result,
                          const std::vector<TopoDS_Shape>& picked, const Walk& walk, double distance) {
  std::vector<TopoDS_Edge> order;
  for (BRepTools_WireExplorer x(wire, forward); x.More(); x.Next()) {
    order.push_back(x.Current());
  }
  const std::size_t n = order.size();
  std::vector<bool> marks;
  for (const TopoDS_Edge& edge : order) {
    marks.push_back(among(picked, edge));
  }
  const bool all = std::all_of(marks.begin(), marks.end(), [](bool m) { return m; });
  if (n == 0 || std::none_of(marks.begin(), marks.end(), [](bool m) { return m; })) {
    return wire;
  }
  // From an unpicked edge, so that no run wraps round the start.
  std::size_t start = 0;
  while (!all && marks[start]) {
    ++start;
  }
  BRep_Builder builder;
  TopoDS_Wire made;
  builder.MakeWire(made);
  std::size_t i = 0;
  while (i < n) {
    const std::size_t at = (start + i) % n;
    if (!marks[at]) {
      builder.Add(made, order[at]);
      ++i;
      continue;
    }
    std::vector<Reach> run;
    while (i < n && marks[(start + i) % n]) {
      run.push_back(reach_of(order[(start + i) % n], forward, walk, distance));
      ++i;
    }
    // The far vertices where the run's edges meet (a closed run: also its
    // last and first).
    std::vector<TopoDS_Vertex> joints;
    const std::size_t m = run.size();
    for (std::size_t k = 0; k + 1 < m || (all && k < m); ++k) {
      Reach& a = run[k];
      Reach& b = run[(k + 1) % m];
      const gp_Pnt2d uv = joint(a, b, walk, distance);
      a.far.back() = uv;
      b.far.front() = uv;
      joints.push_back(vertex_at(walk.point(uv)));
    }
    if (all) {
      for (std::size_t k = 0; k < m; ++k) {
        builder.Add(made, edge_through(run[k].far, joints[(k + m - 1) % m], joints[k], result));
      }
      continue;
    }
    const TopoDS_Edge& before = order[(at + n - 1) % n];
    const TopoDS_Edge& after = order[(start + i) % n];
    const TopoDS_Vertex first = TopExp::FirstVertex(run.front().edge, true);
    const TopoDS_Vertex last = TopExp::LastVertex(run.back().edge, true);
    const std::vector<gp_Pnt2d> rise = side(before, first, run.front().pcurve->Value(run.front().from),
                                            run.front().out_from, walk, distance);
    std::vector<gp_Pnt2d> fall =
        side(after, last, run.back().pcurve->Value(run.back().to), run.back().out_to, walk, distance);
    run.front().far.front() = rise.back();
    run.back().far.back() = fall.back();
    const TopoDS_Vertex rise_end = vertex_at(walk.point(rise.back()));
    const TopoDS_Vertex fall_end = vertex_at(walk.point(fall.back()));
    builder.Add(made, edge_through(rise, first, rise_end, result));
    for (std::size_t k = 0; k < m; ++k) {
      builder.Add(made, edge_through(run[k].far, k == 0 ? rise_end : joints[k - 1],
                                     k + 1 == m ? fall_end : joints[k], result));
    }
    std::reverse(fall.begin(), fall.end());
    builder.Add(made, edge_through(fall, fall_end, last, result));
  }
  return made;
}

// The face extended Natural `distance` along its surface past the picked
// edges (see above), its other edges kept. Built on a copy: the input face
// is only read.
TopoDS_Face extended_past_trims(const TopoDS_Face& face, const std::vector<TopoDS_Edge>& edges, double distance) {
  BRepBuilderAPI_Copy copier(face, true, false);
  const TopoDS_Face copy = TopoDS::Face(copier.Shape());
  std::vector<TopoDS_Shape> picked;
  for (const TopoDS_Edge& edge : edges) {
    picked.push_back(copier.ModifiedShape(edge));
  }
  TopLoc_Location location;
  const occ::handle<Geom_Surface> own = BRep_Tool::Surface(copy, location);
  if (own.IsNull()) {
    throw std::invalid_argument("extend: a face has no surface");
  }
  // Long enough for a side that continues a neighbour at the flattest.
  const occ::handle<Geom_Surface> surface = natural_surface(own, 4.0 * distance);
  const Walk walk(location.IsIdentity()
                      ? surface
                      : occ::handle<Geom_Surface>(
                            occ::down_cast<Geom_Surface>(surface->Transformed(location.Transformation()))));
  BRep_Builder builder;
  TopoDS_Face result;
  builder.MakeFace(result, surface, location, BRep_Tool::Tolerance(copy));
  const TopoDS_Face forward = TopoDS::Face(copy.Oriented(TopAbs_FORWARD));
  // The kept edges on the continued surface, with their curves on the
  // face's own (of the same parameters).
  for (TopExp_Explorer x(forward, TopAbs_EDGE); x.More(); x.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(x.Current());
    if (among(picked, edge)) {
      continue;
    }
    const double tolerance = BRep_Tool::Tolerance(edge);
    double a = 0.0;
    double b = 0.0;
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
    if (w.Value().ShapeType() == TopAbs_WIRE) {
      builder.Add(result, extended_wire(TopoDS::Wire(w.Value()), forward, result, picked, walk, distance));
    }
  }
  BRepLib::UpdateTolerances(result);
  result.Orientation(face.Orientation());
  return result;
}

// The tolerance the faces are sewn with after an extension: kSewing, or,
// where far corners of the extensions of different faces lie a little
// apart (two surfaces tangent along their shared edge, each continued its
// own way), up to kJoin, as far apart as they lie.
double joining_tolerance(const Shape& body, const std::vector<TopoDS_Shape>& extended) {
  std::vector<gp_Pnt> old;
  for (int i = 0; i < body.vertex_count(); ++i) {
    old.push_back(BRep_Tool::Pnt(body.vertex(i)));
  }
  std::vector<std::pair<std::size_t, gp_Pnt>> fresh;
  for (std::size_t k = 0; k < extended.size(); ++k) {
    for (TopExp_Explorer v(extended[k], TopAbs_VERTEX); v.More(); v.Next()) {
      const gp_Pnt p = BRep_Tool::Pnt(TopoDS::Vertex(v.Current()));
      if (std::none_of(old.begin(), old.end(), [&](const gp_Pnt& q) { return q.Distance(p) <= kSewing; })) {
        fresh.emplace_back(k, p);
      }
    }
  }
  double gap = 0.0;
  for (std::size_t i = 0; i < fresh.size(); ++i) {
    for (std::size_t j = i + 1; j < fresh.size(); ++j) {
      const double d = fresh[i].second.Distance(fresh[j].second);
      if (fresh[i].first != fresh[j].first && d > kSewing && d <= kJoin) {
        gap = std::max(gap, d);
      }
    }
  }
  return gap > 0.0 ? std::min(kJoin, 1.5 * gap) : kSewing;
}

} // namespace

ShapePtr offset_surface(const std::string& feature, const Shape& body, const std::vector<std::string>& faces,
                        double distance, bool chain) {
  detail::require_finite("the offset distance", distance);
  detail::require_surfaces(body.occt(), "offset surface");
  return detail::run("offset surface", [&] {
    std::vector<int> picked;
    if (faces.empty()) {
      for (int i = 0; i < body.face_count(); ++i) {
        picked.push_back(i);
      }
    } else {
      picked = detail::resolve_faces(body, faces);
    }
    if (chain) {
      picked = detail::tangent_closure(body, picked);
    }
    if (picked.empty()) {
      throw std::invalid_argument("offset surface: the body has no faces");
    }
    const double tolerance = 1.0e-3 + 1.0e-3 * std::abs(distance);
    for (const double d : {distance, -distance}) {
      // A shell of copies of the faces (OCCT's offset writes into them).
      const ShapePtr copy = detail::working_copy(body);
      BRep_Builder builder;
      TopoDS_Shell shell;
      builder.MakeShell(shell);
      for (const int i : picked) {
        builder.Add(shell, copy->face(i));
      }
      if (distance == 0.0) {
        std::vector<Shape::NamedFace> named;
        for (const int i : picked) {
          const std::string face = detail::primary_name(body, i);
          if (!face.empty()) {
            named.push_back({copy->face(i), {face_name(feature, "offset", face)}});
          }
        }
        return std::make_shared<Shape>(shell, named);
      }
      BRepOffset_MakeOffset maker;
      maker.Initialize(shell, d, kTolerance, BRepOffset_Skin, false, false, GeomAbs_Intersection, false);
      detail::interruptible([&](const Message_ProgressRange& range) { maker.MakeOffsetShape(range); });
      if (!maker.IsDone()) {
        throw std::runtime_error("offset surface: the faces could not be offset (a face may be more tightly "
                                 "curved than the distance)");
      }
      NCollection_List<TopoDS_Shape> arguments;
      arguments.Append(shell);
      const BRepTools_History history(arguments, maker);
      detail::FaceNamer namer(maker.Shape());
      bool checked = false;
      bool right_side = true;
      for (const int i : picked) {
        const std::string face = detail::primary_name(body, i);
        std::vector<TopoDS_Shape> images = faces_from(history.Generated(copy->face(i)));
        for (const TopoDS_Shape& image : faces_from(history.Modified(copy->face(i)))) {
          images.push_back(image);
        }
        for (const TopoDS_Shape& image : images) {
          if (!namer.contains(image)) {
            continue;
          }
          if (!checked) {
            checked = true;
            right_side = along_normal(copy->face(i), image, distance, tolerance);
          }
          if (!face.empty() && !namer.named(image)) {
            namer.add(image, face_name(feature, "offset", face));
          }
        }
      }
      if (right_side) {
        namer.finish();
        return namer.shape();
      }
    }
    throw std::runtime_error("offset surface: the faces could not be offset to the side asked for");
  });
}

ShapePtr ruled_surface(const Shape& body, const RuledSpec& spec) {
  detail::require_positive("the ruled surface's distance", spec.distance);
  detail::require_finite("the ruled surface's angle", spec.angle);
  if (spec.mode == RuledMode::Direction && !spec.direction) {
    throw std::invalid_argument("ruled surface: the direction mode needs a direction");
  }
  detail::require_surfaces(body.occt(), "ruled surface");
  return detail::run("ruled surface", [&] {
    // The strips are built on a copy: BRepFill gives the edges they use
    // curves on the strips.
    const ShapePtr copy = detail::working_copy(body);
    const std::vector<std::vector<std::pair<int, TopoDS_Vertex>>> chains =
        chained(*copy, picked_edges(*copy, spec.edges, spec.openings));
    const detail::EdgeFaces edge_faces(*copy);
    int beside = -1;
    if (!spec.face.empty()) {
      const std::vector<int> found = detail::resolve_faces(*copy, {spec.face}, "face");
      beside = found.front();
    }
    std::vector<std::pair<TopoDS_Shape, std::string>> named;
    // Each chain of its own: its strips and corners.
    for (const std::vector<std::pair<int, TopoDS_Vertex>>& order : chains) {
      std::vector<ChainEdge> chain;
      for (const auto& link : order) {
        const int e = link.first;
        std::vector<int> faces = edge_faces.at(e);
        faces.erase(std::unique(faces.begin(), faces.end()), faces.end());
        ChainEdge entry{e, -1};
        if (beside >= 0 && std::find(faces.begin(), faces.end(), beside) != faces.end()) {
          entry.face = beside;
        } else if (faces.size() == 1) {
          entry.face = faces.front();
        } else {
          const std::string why = " lies between two faces; give the face beside the chain";
          throw std::invalid_argument(edge_error(copy->edge_name(e), "ruled surface: an edge" + why,
                                                 "ruled surface: edge " + copy->edge_name(e) + why));
        }
        chain.push_back(entry);
      }
      // Each edge's directions at its two ends (curve parameters first and
      // last), then mitred where neighbours meet at a sharp corner.
      struct Ends {
        gp_Vec first;
        gp_Vec last;
      };
      std::vector<Ends> ends;
      for (const ChainEdge& c : chain) {
        const TopoDS_Edge& edge = copy->edge(c.edge);
        const TopoDS_Face& face = copy->face(c.face);
        const BRepAdaptor_Curve curve(edge);
        ends.push_back({gp_Vec(strip_direction(spec, edge, face, curve.FirstParameter())),
                        gp_Vec(strip_direction(spec, edge, face, curve.LastParameter()))});
      }
      // The end of edge i at vertex v: its first parameter's or its last's.
      const auto at_vertex = [&](int i, const TopoDS_Vertex& v, bool leaving) -> gp_Vec& {
        TopoDS_Vertex a;
        TopoDS_Vertex b;
        TopExp::Vertices(TopoDS::Edge(copy->edge(chain[static_cast<std::size_t>(i)].edge).Oriented(TopAbs_FORWARD)),
                         a, b);
        Ends& e = ends[static_cast<std::size_t>(i)];
        if (a.IsSame(b)) {
          return leaving ? e.first : e.last; // a closed edge
        }
        return a.IsSame(v) ? e.first : e.last;
      };
      const int count = static_cast<int>(chain.size());
      // A closed chain ends where it starts.
      TopoDS_Vertex a0;
      TopoDS_Vertex b0;
      TopExp::Vertices(copy->edge(chain.front().edge), a0, b0);
      const TopoDS_Vertex& last_end = order.back().second;
      const TopoDS_Vertex start = a0.IsSame(order.front().second) ? b0 : a0;
      const bool closed = count > 1 && last_end.IsSame(start);
      for (int i = 0; i + 1 < count || (closed && i < count); ++i) {
        const int j = (i + 1) % count;
        // The vertex edge i ends at and edge j starts from.
        const TopoDS_Vertex& v = order[static_cast<std::size_t>(i)].second;
        gp_Vec& d1 = at_vertex(i, v, false);
        gp_Vec& d2 = at_vertex(j, v, true);
        const double cosine = std::clamp(gp_Dir(d1).Dot(gp_Dir(d2)), -1.0, 1.0);
        if (cosine > 1.0 - 1.0e-9) {
          continue;
        }
        if (cosine < -1.0 + 1.0e-6) {
          throw std::invalid_argument("ruled surface: the strips turn back on each other at a corner");
        }
        if (spec.corners == RuledCorners::Mitre) {
          const gp_Vec mitre = (d1 + d2) / (1.0 + cosine);
          d1 = mitre;
          d2 = mitre;
        } else {
          // A sector turned about the corner from one strip's end to the
          // other's.
          const gp_Pnt corner = BRep_Tool::Pnt(v);
          const gp_Vec axis = d1.Crossed(d2);
          const TopoDS_Edge radius =
              BRepBuilderAPI_MakeEdge(corner, corner.Translated(gp_Dir(d1).XYZ() * spec.distance)).Edge();
          BRepPrimAPI_MakeRevol sector(radius, gp_Ax1(corner, gp_Dir(axis)), std::acos(cosine));
          if (!sector.IsDone()) {
            throw std::runtime_error("ruled surface: a round corner could not be built");
          }
          for (TopExp_Explorer f(sector.Shape(), TopAbs_FACE); f.More(); f.Next()) {
            const std::string vertex = copy->vertex_name(copy->vertex_index(v));
            named.emplace_back(f.Current(), face_name(spec.feature, "corner", vertex));
          }
        }
      }
      for (int i = 0; i < count; ++i) {
        const ChainEdge& c = chain[static_cast<std::size_t>(i)];
        const Ends& e = ends[static_cast<std::size_t>(i)];
        const TopoDS_Face strip = strip_face(spec, copy->edge(c.edge), copy->face(c.face), e.first, e.last);
        // A free edge has no name: the strip is named after its face.
        const std::string& edge = copy->edge_name(c.edge);
        named.emplace_back(strip, face_name(spec.feature, "ruled",
                                            edge.empty() ? detail::primary_name(*copy, c.face) : edge));
      }
    }
    // One shape of the faces with their names, sewn.
    std::vector<TopoDS_Shape> parts;
    std::vector<Shape::NamedFace> faces;
    for (const auto& [face, name] : named) {
      parts.push_back(face);
      faces.push_back({face, {name}});
    }
    const Shape pieces(detail::compound_or_single(parts), faces);
    ShapePtr sewn = stitch({&pieces}, kSewing);
    return sewn;
  });
}

ShapePtr extend_surface(const Shape& body, const std::vector<std::string>& edges,
                        const std::vector<OpeningEdges>& openings, double distance, ExtendMode mode) {
  detail::require_positive("the extension", distance);
  detail::require_surfaces(body.occt(), "extend");
  if (TopExp_Explorer(body.occt(), TopAbs_SOLID).More()) {
    throw std::invalid_argument("extend: the body is a solid; only surface bodies are extended");
  }
  return detail::run("extend", [&] {
    const detail::EdgeFaces edge_faces(body);
    std::map<int, std::array<bool, 4>> sides; // face -> sides to extend
    // Planar faces trimmed inside their planes -> their edges to extend
    // past (mitcad#1955).
    std::map<int, std::vector<TopoDS_Edge>> planes;
    // Faces extended Natural past edges that need not run along their
    // parameter range's sides -> those edges (mitcad#2525): curved faces
    // trimmed inside their surfaces, and planes past curved edges.
    std::map<int, std::vector<TopoDS_Edge>> trims;
    // The picked edges by face, in the order picked.
    std::map<int, std::vector<int>> by_face;
    for (const int e : picked_edges(body, edges, openings)) {
      const std::vector<int> faces = edge_faces.at(e);
      if (faces.size() != 1 || BRep_Tool::IsClosed(body.edge(e), body.face(faces.front()))) {
        const std::string why = " is not on the surface's open boundary";
        throw std::invalid_argument(
            edge_error(body.edge_name(e), "extend: an edge" + why, "extend: edge " + body.edge_name(e) + why));
      }
      by_face[faces.front()].push_back(e);
    }
    for (const auto& [f, picked] : by_face) {
      const TopoDS_Face& face = body.face(f);
      std::vector<TopoDS_Edge> list;
      for (const int e : picked) {
        list.push_back(body.edge(e));
      }
      if (planar(face) && !untrimmed(face)) {
        // Straight edges as the planar path takes them; past a curved one
        // the plane continues as itself in every mode.
        const bool straight = std::all_of(list.begin(), list.end(), [](const TopoDS_Edge& edge) {
          return BRepAdaptor_Curve(edge).GetType() == GeomAbs_Line;
        });
        (straight ? planes : trims)[f] = list;
        continue;
      }
      std::vector<int> along;
      for (const int e : picked) {
        along.push_back(side_of(body.edge(e), face));
      }
      if (mode == ExtendMode::Natural &&
          (!untrimmed(face) || std::any_of(along.begin(), along.end(), [](int side) { return side < 0; }))) {
        trims[f] = list;
        continue;
      }
      for (std::size_t i = 0; i < picked.size(); ++i) {
        const int e = picked[i];
        if (along[i] < 0) {
          const std::string why = ", which does not run along a side of its face's parameter range";
          throw std::invalid_argument(edge_error(body.edge_name(e), "unsupported: extend of an edge" + why,
                                                 "unsupported: extend of edge " + body.edge_name(e) + why));
        }
        sides[f][static_cast<std::size_t>(along[i])] = true;
      }
    }
    std::vector<TopoDS_Shape> parts;
    std::vector<Shape::NamedFace> named;
    std::vector<TopoDS_Shape> extended;
    for (int f = 0; f < body.face_count(); ++f) {
      const auto found = sides.find(f);
      const auto plane = planes.find(f);
      const auto trim = trims.find(f);
      const TopoDS_Shape face =
          plane != planes.end()  ? TopoDS_Shape(extended_planar_face(body.face(f), plane->second, distance))
          : trim != trims.end()  ? TopoDS_Shape(extended_past_trims(body.face(f), trim->second, distance))
          : found == sides.end() ? TopoDS_Shape(body.face(f))
                                 : TopoDS_Shape(extended_face(body.face(f), found->second, distance, mode));
      parts.push_back(face);
      named.push_back({face, body.face_names(f)});
      if (plane != planes.end() || trim != trims.end() || found != sides.end()) {
        extended.push_back(face);
      }
    }
    const Shape faces(detail::compound_or_single(parts), named);
    return stitch({&faces}, joining_tolerance(body, extended));
  });
}

std::vector<int> detail::sheet_edges(const Shape& body, const std::vector<std::string>& names,
                                     const std::vector<OpeningEdges>& openings) {
  return picked_edges(body, names, openings);
}

gp_Dir detail::across_edge(const TopoDS_Edge& edge, const TopoDS_Face& face, double t) {
  RuledSpec tangent;
  tangent.mode = RuledMode::Tangent;
  return strip_direction(tangent, edge, face, t);
}

// Curves of free edges (mitcad#2395).
std::vector<FreeEdgeCurve> opening_curves(const Shape& body, const OpeningEdges& opening) {
  return detail::run("free edge curves", [&] {
    std::vector<FreeEdgeCurve> curves;
    const detail::EdgeFaces edge_faces(body);
    for (const int e : opening_edges(body, {opening})) {
      FreeEdgeCurve free;
      free.curve = edge_curve(body.edge(e));
      const std::vector<int> faces = edge_faces.at(e);
      if (!faces.empty()) {
        free.face = detail::primary_name(body, faces.front());
      }
      curves.push_back(std::move(free));
    }
    return curves;
  });
}

} // namespace mitcad::geometry
