// SPDX-License-Identifier: MIT
#include "remnant_patch.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <exception>
#include <initializer_list>
#include <limits>
#include <map>
#include <optional>
#include <set>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAlgoAPI_Section.hxx>
#include <BRepBuilderAPI_MakeSolid.hxx>
#include <BRepBuilderAPI_Sewing.hxx>
#include <BRepFeat_SplitShape.hxx>
#include <BRepGProp.hxx>
#include <BRepLib.hxx>
#include <BRepOffsetAPI_MakeFilling.hxx>
#include <BRep_Tool.hxx>
#include <Extrema_ExtPC.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <GProp_GProps.hxx>
#include <NCollection_DataMap.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt.hxx>
#include <gp_Vec.hxx>

#include "mitcad/geometry/naming.hpp"

#include "face_select.hpp"
#include "history.hpp"
#include "util.hpp"

namespace mitcad::geometry::detail {
namespace {

using Ancestors = NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;

// How closely a patch must meet the faces round it: position (mm) and the
// angle between the normals (radians), as a setback corner's (corner.cpp).
constexpr double kPatchGap = 1.0e-4;
constexpr double kPatchAngle = 1.0e-2;

// The images of a face of the body in the split shape: its pieces, or
// itself when the split left it (or there was nothing to split).
std::vector<TopoDS_Shape> images(BRepFeat_SplitShape* split, const TopoDS_Shape& face) {
  std::vector<TopoDS_Shape> found;
  if (split == nullptr) {
    found.push_back(face);
    return found;
  }
  if (split->IsDeleted(face)) {
    return found;
  }
  const NCollection_List<TopoDS_Shape>& modified = split->Modified(face);
  if (modified.IsEmpty()) {
    found.push_back(face);
  } else {
    found.assign(modified.begin(), modified.end());
  }
  return found;
}

double area_of(const TopoDS_Shape& face) {
  GProp_GProps props;
  BRepGProp::SurfaceProperties(face, props);
  return props.Mass();
}

// What a name of a face the dressup made says it dresses, its pieces'
// "#k" taken off: the edge of <feature>:<role>(<edge>) (second true) or the
// vertex of <feature>:corner(<vertex>) (second false); empty for any other
// name.
std::pair<std::string, bool> dressed_by(const std::string& name, const std::string& feature) {
  std::string base = name;
  for (;;) {
    const std::size_t hash = base.rfind('#');
    if (hash == std::string::npos || hash == 0 || hash + 1 == base.size() || base[hash - 1] != ')' ||
        !std::all_of(base.begin() + static_cast<std::ptrdiff_t>(hash) + 1, base.end(),
                     [](char c) { return c >= '0' && c <= '9'; })) {
      break;
    }
    base.erase(hash);
  }
  const std::string prefix = feature + ":";
  if (base.size() <= prefix.size() + 2 || base.compare(0, prefix.size(), prefix) != 0 || base.back() != ')') {
    return {};
  }
  const std::size_t open = base.find('(', prefix.size());
  if (open == std::string::npos || open + 2 > base.size()) {
    return {};
  }
  const std::string role = base.substr(prefix.size(), open - prefix.size());
  return {base.substr(open + 1, base.size() - open - 2), role != "corner"};
}

// The parameter of the point of the curve nearest `p` (its ends included).
double nearest_parameter(const BRepAdaptor_Curve& curve, const gp_Pnt& p, double* distance = nullptr) {
  double best = curve.FirstParameter();
  double nearest = std::numeric_limits<double>::infinity();
  for (const double t : {curve.FirstParameter(), curve.LastParameter()}) {
    const double d = curve.Value(t).SquareDistance(p);
    if (d < nearest) {
      nearest = d;
      best = t;
    }
  }
  const Extrema_ExtPC extrema(p, curve);
  if (extrema.IsDone()) {
    for (int i = 1; i <= extrema.NbExt(); ++i) {
      if (extrema.SquareDistance(i) < nearest) {
        nearest = extrema.SquareDistance(i);
        best = extrema.Point(i).Parameter();
      }
    }
  }
  if (distance != nullptr) {
    *distance = std::sqrt(nearest);
  }
  return best;
}

// Points of a face: its vertices and its edges' middles.
std::vector<gp_Pnt> points_of(const TopoDS_Shape& face) {
  std::vector<gp_Pnt> points;
  ShapeMap vertices;
  TopExp::MapShapes(face, TopAbs_VERTEX, vertices);
  for (int v = 1; v <= vertices.Extent(); ++v) {
    points.push_back(BRep_Tool::Pnt(TopoDS::Vertex(vertices(v))));
  }
  for (TopExp_Explorer ex(face, TopAbs_EDGE); ex.More(); ex.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(ex.Current());
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    const BRepAdaptor_Curve curve(edge);
    points.push_back(curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter())));
  }
  return points;
}

// The range of parameters of the curve the points are nearest to, and
// their mean.
struct Span {
  double low = std::numeric_limits<double>::infinity();
  double high = -std::numeric_limits<double>::infinity();
  double middle = 0.0;
};

Span span_on(const BRepAdaptor_Curve& curve, const std::vector<gp_Pnt>& points) {
  Span span;
  for (const gp_Pnt& p : points) {
    const double t = nearest_parameter(curve, p);
    span.low = std::min(span.low, t);
    span.high = std::max(span.high, t);
    span.middle += t;
  }
  span.middle /= static_cast<double>(std::max<std::size_t>(points.size(), 1));
  return span;
}

// Where a face of the dressup along the input's edge `edge` is cut across,
// `distance` from the parameter `from` of the edge, towards its first
// parameter where `down`: the point and the edge's tangent there. A face of
// a chain of tangent edges runs on past the edge's end, so where the
// distance reaches past the end while the face's points (`own`) reach that
// end, the rest of it is taken along the input's edge there that goes on
// tangentially (within kTurn) and that the face also runs along. None where
// the face ends first, or the chain does with no such edge.
constexpr double kTurn = 1.0e-2;

std::optional<std::pair<gp_Pnt, gp_Vec>> cut_along(const Ancestors& edges_of_vertex, TopoDS_Edge edge, double from,
                                                   double distance, bool down, const std::vector<gp_Pnt>& own) {
  ShapeMap visited;
  for (;;) {
    visited.Add(edge);
    const BRepAdaptor_Curve curve(edge);
    const Span face = span_on(curve, own);
    const double first = curve.FirstParameter();
    const double last = curve.LastParameter();
    const double margin = 1.0e-3 * (last - first);
    const GCPnts_AbscissaPoint at(curve, down ? -distance : distance, from);
    if (!at.IsDone()) {
      return std::nullopt;
    }
    const double t = at.Parameter();
    if (down ? t >= first : t <= last) {
      // Within the edge: the face must run on past the cut.
      const double inside = 1.0e-3 * (face.high - face.low);
      if (!(down ? t > face.low + inside : t < face.high - inside)) {
        return std::nullopt;
      }
      gp_Pnt p;
      gp_Vec tangent;
      curve.D1(t, p, tangent);
      if (tangent.Magnitude() <= 1.0e-12) {
        return std::nullopt;
      }
      return std::make_pair(p, tangent);
    }
    // Past the edge's end: on along the chain where the face reaches it.
    const double end = down ? first : last;
    if (!(down ? face.low <= first + margin : face.high >= last - margin)) {
      return std::nullopt;
    }
    distance -= GCPnts_AbscissaPoint::Length(curve, std::min(from, end), std::max(from, end));
    // (The vertices at the curve's first and last parameters, whatever the
    // edge's orientation.)
    TopoDS_Vertex v_first;
    TopoDS_Vertex v_last;
    TopExp::Vertices(edge, v_first, v_last);
    const TopoDS_Vertex& vertex = down ? v_first : v_last;
    if (vertex.IsNull() || !edges_of_vertex.Contains(vertex)) {
      return std::nullopt;
    }
    gp_Pnt p;
    gp_Vec tangent;
    curve.D1(end, p, tangent);
    std::optional<TopoDS_Edge> next;
    bool next_down = false;
    double next_from = 0.0;
    for (const TopoDS_Shape& candidate : edges_of_vertex.FindFromKey(vertex)) {
      const TopoDS_Edge& other = TopoDS::Edge(candidate);
      // (An edge is listed once for each face it is met in.)
      if (visited.Contains(other) || BRep_Tool::Degenerated(other) || (next && next->IsSame(other))) {
        continue;
      }
      TopoDS_Vertex o_first;
      TopoDS_Vertex o_last;
      TopExp::Vertices(other, o_first, o_last);
      const BRepAdaptor_Curve along(other);
      const bool at_first = o_first.IsSame(vertex);
      if (!at_first && !o_last.IsSame(vertex)) {
        continue;
      }
      const double t_at = at_first ? along.FirstParameter() : along.LastParameter();
      gp_Pnt q;
      gp_Vec turn;
      along.D1(t_at, q, turn);
      if (turn.Magnitude() <= 1.0e-12 || tangent.Magnitude() <= 1.0e-12) {
        continue;
      }
      // Going on means leaving the vertex the way this edge left it.
      const gp_Vec ahead = down ? -tangent : tangent;
      const gp_Vec onward = at_first ? turn : -turn;
      if (ahead.Angle(onward) > kTurn) {
        continue;
      }
      // The face runs along it too: some of its points lie off the vertex.
      const Span on = span_on(along, own);
      const double reach = at_first ? on.high - along.FirstParameter() : along.LastParameter() - on.low;
      if (!(reach > 1.0e-3 * (along.LastParameter() - along.FirstParameter()))) {
        continue;
      }
      if (next) {
        return std::nullopt; // (two ways on: no one place to cut)
      }
      next = other;
      next_down = !at_first;
      next_from = t_at;
    }
    if (!next || !(distance > 0.0)) {
      return std::nullopt;
    }
    edge = *next;
    from = next_from;
    down = next_down;
  }
}

// One place: pieces that share edges, the dressup's faces beside them, and
// where each of those is cut (or taken whole).
struct Place {
  std::vector<int> pieces;
  std::vector<int> beside; // the dressup's faces beside the pieces
  std::set<int> whole;     // of those, the ones taken whole
  gp_Pnt centre;
  std::string name; // the patch's
};

} // namespace

ShapePtr patch_remnants(const Shape& result, const Shape& input, const std::vector<int>& pieces,
                        const std::vector<char>& made, const std::string& feature, const char* role,
                        double contact) {
  if (pieces.empty() || !(contact > 0.0)) {
    return nullptr;
  }
  try {
    OCC_CATCH_SIGNALS
    int solids = 0;
    for (TopExp_Explorer ex(result.occt(), TopAbs_SOLID); ex.More(); ex.Next()) {
      ++solids;
    }
    if (solids != 1) {
      return nullptr;
    }
    const bool rounding = std::string(role) == "fillet";
    // (On a copy: the split, the patch's boundary and the sewing write into
    // the shapes they work on, input_check.hpp. The same face numbers.)
    const ShapePtr copy = working_copy(result);
    const Shape& body = *copy;
    const auto is_made = [&](int f) {
      return f >= 0 && f < static_cast<int>(made.size()) && made[static_cast<std::size_t>(f)] != 0;
    };
    const auto is_piece = [&](int f) { return std::find(pieces.begin(), pieces.end(), f) != pieces.end(); };
    Ancestors faces_of_edge;
    TopExp::MapShapesAndAncestors(body.occt(), TopAbs_EDGE, TopAbs_FACE, faces_of_edge);
    const auto beside = [&](int f) {
      std::set<int> found;
      for (TopExp_Explorer ex(body.face(f), TopAbs_EDGE); ex.More(); ex.Next()) {
        const int i = faces_of_edge.FindIndex(ex.Current());
        if (i == 0) {
          continue;
        }
        for (const TopoDS_Shape& other : faces_of_edge.FindFromIndex(i)) {
          const int g = body.face_index(other);
          if (g >= 0 && g != f) {
            found.insert(g);
          }
        }
      }
      return found;
    };

    Ancestors edges_of_vertex;
    TopExp::MapShapesAndAncestors(input.occt(), TopAbs_VERTEX, TopAbs_EDGE, edges_of_vertex);

    // The places: pieces that share an edge go together.
    const std::size_t n = pieces.size();
    std::vector<std::size_t> parent(n);
    for (std::size_t i = 0; i < n; ++i) {
      parent[i] = i;
    }
    const auto root = [&](std::size_t i) {
      while (parent[i] != i) {
        i = parent[i];
      }
      return i;
    };
    for (std::size_t i = 0; i < n; ++i) {
      const std::set<int> adjacent = beside(pieces[i]);
      for (std::size_t j = i + 1; j < n; ++j) {
        if (adjacent.count(pieces[j]) != 0) {
          parent[root(j)] = root(i);
        }
      }
    }
    std::map<std::size_t, Place> by_root;
    for (std::size_t i = 0; i < n; ++i) {
      by_root[root(i)].pieces.push_back(pieces[i]);
    }
    std::vector<Place> places;
    for (auto& entry : by_root) {
      places.push_back(std::move(entry.second));
    }

    // At each place: its middle and size, the dressup's faces beside it,
    // the cuts across them, and the patch's name.
    std::vector<std::pair<int, TopoDS_Edge>> cuts;
    std::set<int> taken_whole;
    for (Place& place : places) {
      throw_if_cancelled();
      std::vector<gp_Pnt> points;
      double area = 0.0;
      gp_XYZ weighted(0.0, 0.0, 0.0);
      std::set<int> nearby;
      for (const int f : place.pieces) {
        GProp_GProps props;
        BRepGProp::SurfaceProperties(body.face(f), props);
        area += props.Mass();
        weighted += props.CentreOfMass().XYZ() * props.Mass();
        const std::vector<gp_Pnt> own = points_of(body.face(f));
        points.insert(points.end(), own.begin(), own.end());
        for (const int g : beside(f)) {
          if (is_made(g) && !is_piece(g)) {
            nearby.insert(g);
          }
        }
      }
      if (points.empty() || nearby.empty() || !(area > 0.0)) {
        return nullptr;
      }
      place.centre = gp_Pnt(weighted / area);
      place.beside.assign(nearby.begin(), nearby.end());
      double size = 0.0;
      for (std::size_t i = 0; i < points.size(); ++i) {
        for (std::size_t j = i + 1; j < points.size(); ++j) {
          size = std::max(size, points[i].Distance(points[j]));
        }
      }
      const double setback = 0.5 * contact + size;

      // The input's vertices the patch may be named after: those of the
      // edges the faces beside it round, and the vertices of corners, each
      // with the faces beside the place that lead to it.
      std::map<std::string, std::pair<gp_Pnt, std::set<int>>> vertices;
      int beside_face = -1;
      const auto add_vertex = [&](const TopoDS_Shape& v) {
        if (const auto name = input.name_of_vertex(v)) {
          auto& entry = vertices.emplace(*name, std::make_pair(BRep_Tool::Pnt(TopoDS::Vertex(v)), std::set<int>()))
                            .first->second;
          entry.second.insert(beside_face);
        }
      };
      for (const int g : place.beside) {
        beside_face = g;
        // The input's edge the face rounds or bevels, the one nearest the
        // place where its name finds several.
        int edge = -1;
        double nearest = std::numeric_limits<double>::infinity();
        for (const std::string& name : body.face_names(g)) {
          const auto [reference, of_edge] = dressed_by(name, feature);
          if (reference.empty()) {
            continue;
          }
          std::vector<int> found;
          try {
            found = of_edge ? input.find_edges(reference) : input.find_vertices(reference);
          } catch (const std::exception&) {
            found.clear();
          }
          for (const int e : found) {
            if (!of_edge) {
              add_vertex(input.vertex(e));
              continue;
            }
            const TopoDS_Edge& candidate = input.edge(e);
            if (BRep_Tool::Degenerated(candidate)) {
              continue;
            }
            TopoDS_Vertex first;
            TopoDS_Vertex last;
            TopExp::Vertices(candidate, first, last);
            for (const TopoDS_Vertex& v : {first, last}) {
              if (!v.IsNull()) {
                add_vertex(v);
              }
            }
            double distance = 0.0;
            nearest_parameter(BRepAdaptor_Curve(candidate), place.centre, &distance);
            if (distance < nearest) {
              nearest = distance;
              edge = e;
            }
          }
        }
        // Cut across the edge `setback` past the place, on the side where
        // the face runs on (on along the chain's next tangent edge where
        // the face goes on past the edge's end, cut_along); taken whole
        // where it ends before that (or has no edge: a corner), if it is
        // small.
        std::optional<TopoDS_Edge> across;
        if (edge >= 0) {
          const BRepAdaptor_Curve curve(input.edge(edge));
          const Span piece = span_on(curve, points);
          const std::vector<gp_Pnt> own = points_of(body.face(g));
          const bool down = span_on(curve, own).middle < piece.middle;
          if (const auto at = cut_along(edges_of_vertex, input.edge(edge), down ? piece.low : piece.high, setback,
                                        down, own)) {
            BRepAlgoAPI_Section section(body.face(g), gp_Pln(at->first, gp_Dir(at->second)), false);
            section.ComputePCurveOn1(true);
            section.Approximation(true);
            section.SetNonDestructive(true);
            detail::build(section);
            if (section.IsDone()) {
              for (TopExp_Explorer ex(section.Shape(), TopAbs_EDGE); ex.More(); ex.Next()) {
                cuts.emplace_back(g, TopoDS::Edge(ex.Current()));
                across = TopoDS::Edge(ex.Current());
              }
            }
          }
        }
        if (!across) {
          if (area_of(body.face(g)) > 4.0 * contact * contact || !taken_whole.insert(g).second) {
            return nullptr;
          }
          place.whole.insert(g);
        }
      }
      // The input's vertex that most of the faces beside the place lead
      // to (where their edges meet), the nearest of those, names the patch;
      // the piece's own name (as a corner of the dressup) where no vertex
      // is found.
      std::string vertex;
      std::size_t shared = 0;
      double nearest = std::numeric_limits<double>::infinity();
      for (const auto& [name, entry] : vertices) {
        const double distance = entry.first.Distance(place.centre);
        if (entry.second.size() > shared || (entry.second.size() == shared && distance < nearest)) {
          shared = entry.second.size();
          nearest = distance;
          vertex = name;
        }
      }
      if (vertex.empty()) {
        const NameList& own = body.face_names(place.pieces.front());
        if (own.empty()) {
          return nullptr;
        }
        vertex = own.front();
      }
      place.name = face_name(feature, "corner", vertex);
    }

    // The dressup's faces cut across.
    std::optional<BRepFeat_SplitShape> split;
    if (!cuts.empty()) {
      split.emplace(body.occt());
      for (const auto& [face, edge] : cuts) {
        split->Add(edge, body.face(face));
      }
      detail::build(*split);
      if (!split->IsDone()) {
        return nullptr;
      }
    }
    BRepFeat_SplitShape* const splitting = split ? &*split : nullptr;
    const TopoDS_Shape cut = split ? split->Shape() : body.occt();
    Ancestors faces_of_cut_edge;
    TopExp::MapShapesAndAncestors(cut, TopAbs_EDGE, TopAbs_FACE, faces_of_cut_edge);
    // The body's face each face of the cut shape is (a piece of).
    NCollection_DataMap<TopoDS_Shape, int, TopTools_ShapeMapHasher> origin;
    for (int f = 0; f < body.face_count(); ++f) {
      for (const TopoDS_Shape& image : images(splitting, body.face(f))) {
        origin.Bind(image, f);
      }
    }
    const auto touches = [&](const TopoDS_Shape& face, const ShapeMap& set) {
      for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
        const int i = faces_of_cut_edge.FindIndex(it.Current());
        if (i == 0) {
          continue;
        }
        for (const TopoDS_Shape& other : faces_of_cut_edge.FindFromIndex(i)) {
          if (!other.IsSame(face) && set.Contains(other)) {
            return true;
          }
        }
      }
      return false;
    };

    // Each place's hole: its pieces and the parts of the faces beside them
    // next to them; the patch through the hole's boundary.
    ShapeMap removed;
    double removed_area = 0.0;
    std::vector<std::pair<TopoDS_Face, std::string>> patches;
    for (const Place& place : places) {
      throw_if_cancelled();
      ShapeMap hole;
      for (const int f : place.pieces) {
        for (const TopoDS_Shape& image : images(splitting, body.face(f))) {
          hole.Add(image);
        }
      }
      const ShapeMap own = hole;
      for (const int g : place.beside) {
        const std::vector<TopoDS_Shape> parts = images(splitting, body.face(g));
        if (place.whole.count(g) != 0) {
          for (const TopoDS_Shape& part : parts) {
            hole.Add(part);
          }
          continue;
        }
        int next_to = 0;
        for (const TopoDS_Shape& part : parts) {
          if (touches(part, own)) {
            hole.Add(part);
            ++next_to;
          }
        }
        if (parts.size() < 2 || next_to != 1) {
          return nullptr;
        }
      }
      double area = 0.0;
      for (int h = 1; h <= hole.Extent(); ++h) {
        if (removed.Contains(hole(h))) {
          return nullptr; // (two places would take the same face)
        }
        area += area_of(hole(h));
      }
      // Tangent where the faces outside should run on into it, through the
      // edges elsewhere.
      BRepOffsetAPI_MakeFilling filling(3, 15, 2, false, 1.0e-5, kPatchGap, kPatchAngle, 0.1, 8, 9);
      int bounds = 0;
      bool tangent = false;
      for (int h = 1; h <= hole.Extent(); ++h) {
        const TopoDS_Face& inside = TopoDS::Face(hole(h));
        for (TopExp_Explorer it(inside, TopAbs_EDGE); it.More(); it.Next()) {
          const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
          if (BRep_Tool::Degenerated(edge)) {
            continue;
          }
          const int i = faces_of_cut_edge.FindIndex(edge);
          if (i == 0) {
            continue;
          }
          for (const TopoDS_Shape& other : faces_of_cut_edge.FindFromIndex(i)) {
            if (other.IsSame(inside) || hole.Contains(other)) {
              continue;
            }
            const TopoDS_Face& outside = TopoDS::Face(other);
            const bool smooth = normal_angle(edge, inside, outside) < kPatchAngle;
            const bool of_dressup = origin.IsBound(other) && is_made(origin.Find(other));
            // (A fillet rounds the piece's edges with the faces beyond it too:
            // a piece of the chain's edge left sharp there.)
            const bool g1 = smooth || of_dressup || (rounding && own.Contains(inside));
            filling.Add(edge, outside, g1 ? GeomAbs_G1 : GeomAbs_C0, true);
            ++bounds;
            tangent = tangent || g1;
          }
        }
      }
      if (bounds < 2) {
        return nullptr;
      }
      detail::build(filling);
      if (!filling.IsDone() || filling.G0Error() > 10 * kPatchGap ||
          (tangent && filling.G1Error() > 10 * kPatchAngle)) {
        return nullptr;
      }
      const TopoDS_Face patch = TopoDS::Face(filling.Shape());
      // (A patch that bulges far out of its place is no closure.)
      if (!(area_of(patch) <= 1.5 * area)) {
        return nullptr;
      }
      patches.emplace_back(patch, place.name);
      for (int h = 1; h <= hole.Extent(); ++h) {
        removed.Add(hole(h));
      }
      removed_area += area;
    }

    // The faces left and the patches sewn into the solid.
    BRepBuilderAPI_Sewing sewing(10 * kPatchGap);
    ShapeMap kept;
    for (TopExp_Explorer it(cut, TopAbs_FACE); it.More(); it.Next()) {
      const int before = kept.Extent();
      if (!removed.Contains(it.Current()) && kept.Add(it.Current()) > before) {
        sewing.Add(it.Current());
      }
    }
    for (const auto& patch : patches) {
      sewing.Add(patch.first);
    }
    detail::interruptible([&](const Message_ProgressRange& range) { sewing.Perform(range); });
    TopoDS_Shell shell;
    int shells = 0;
    for (TopExp_Explorer it(sewing.SewedShape(), TopAbs_SHELL); it.More(); it.Next(), ++shells) {
      shell = TopoDS::Shell(it.Current());
    }
    if (shells != 1 || !BRep_Tool::IsClosed(shell)) {
      return nullptr;
    }
    BRepBuilderAPI_MakeSolid make(shell);
    if (!make.IsDone()) {
      return nullptr;
    }
    TopoDS_Solid solid = make.Solid();
    BRepLib::OrientClosedSolid(solid);
    if (!is_valid_where_changed(solid, kept)) {
      return nullptr;
    }
    const double before = volume_of(result.occt());
    if (!(std::abs(volume_of(solid) - before) <= contact * removed_area)) {
      return nullptr;
    }

    // Names: the kept faces' through the split and the sewing, the patches'.
    FaceNamer namer(solid);
    const auto image = [&](const TopoDS_Shape& face) {
      return sewing.IsModified(face) ? sewing.Modified(face) : face;
    };
    for (int f = 0; f < body.face_count(); ++f) {
      for (const TopoDS_Shape& piece : images(splitting, body.face(f))) {
        if (!removed.Contains(piece)) {
          for (const std::string& name : body.face_names(f)) {
            namer.add(image(piece), name);
          }
        }
      }
    }
    for (const auto& [patch, name] : patches) {
      namer.add(image(patch), name);
    }
    namer.finish();
    ShapePtr out = namer.shape();
    out->copy_notes(result);
    return out;
  } catch (const Cancelled&) {
    throw;
  } catch (const Standard_Failure&) {
    return nullptr;
  } catch (const std::exception&) {
    return nullptr;
  }
}

} // namespace mitcad::geometry::detail
