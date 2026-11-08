// SPDX-License-Identifier: MIT
#include "mitcad/geometry/item_origin.hpp"

#include <algorithm>
#include <cstddef>
#include <limits>
#include <optional>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepBndLib.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRepTools.hxx>
#include <BRepTopAdaptor_FClass2d.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <Precision.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pnt2d.hxx>

#include "mitcad/geometry/naming.hpp"

namespace mitcad::geometry {
namespace {

using Kind = ItemOrigin::Kind;

ItemOrigin origin_of(Kind kind, std::string name = {}) {
  ItemOrigin origin;
  origin.kind = kind;
  origin.name = std::move(name);
  return origin;
}

// The item found on the body before: the same name, or another.
ItemOrigin found_as(const std::string& name, const std::string& before) {
  if (before.empty()) {
    return origin_of(Kind::Lost);
  }
  return origin_of(before == name ? Kind::Same : Kind::Modified, before);
}

// Where no one item is told: the name as it is when the body before
// resolves it (a reference to several items there, as a definition may
// hold one), none otherwise.
ItemOrigin as_named(bool resolves, const std::string& name) {
  return resolves ? origin_of(Kind::Same, name) : origin_of(Kind::Lost);
}

// A face name without its last piece number "#k"; empty when it ends in
// none (a "#k" inside its key, an edge's, is not one).
std::string whole_of(const std::string& name) {
  const std::size_t hash = name.rfind('#');
  if (hash == std::string::npos || hash + 1 >= name.size() ||
      !std::all_of(name.begin() + static_cast<std::ptrdiff_t>(hash + 1), name.end(),
                   [](char c) { return c >= '0' && c <= '9'; })) {
    return {};
  }
  return name.substr(0, hash);
}

// A face of the result on the body before, by name: the narrowest name
// the body before has (the name itself, or the face it is a piece of, a
// split's "#k" taken off one after another) and the widest (with every
// piece of that face the body before has). Both empty when the body before
// has no such face: the feature made it.
struct FaceBefore {
  std::string narrow;
  std::string wide;
};

FaceBefore face_before(const Shape& before, const std::string& name) {
  FaceBefore found;
  for (std::string current = name; !current.empty(); current = whole_of(current)) {
    if (!before.matching_faces(current).empty()) {
      if (found.narrow.empty()) {
        found.narrow = current;
      }
      found.wide = current;
    } else if (!found.narrow.empty()) {
      break;
    }
  }
  return found;
}

// A point inside a face: the cell centre of a grid of its parameters that
// falls in it nearest to the parameters' middle, or that middle.
gp_Pnt inner_point(const TopoDS_Face& face) {
  double u0 = 0.0;
  double u1 = 0.0;
  double v0 = 0.0;
  double v1 = 0.0;
  BRepTools::UVBounds(face, u0, u1, v0, v1);
  const BRepAdaptor_Surface surface(face, false);
  const double um = 0.5 * (u0 + u1);
  const double vm = 0.5 * (v0 + v1);
  if (!(u1 > u0) || !(v1 > v0)) {
    return surface.Value(um, vm);
  }
  BRepTopAdaptor_FClass2d inside(face, Precision::PConfusion());
  constexpr int kCells = 7;
  std::optional<gp_Pnt2d> best;
  double closest = std::numeric_limits<double>::infinity();
  for (int i = 0; i < kCells; ++i) {
    for (int j = 0; j < kCells; ++j) {
      const gp_Pnt2d uv(u0 + (u1 - u0) * (i + 0.5) / kCells, v0 + (v1 - v0) * (j + 0.5) / kCells);
      const double off = uv.SquareDistance(gp_Pnt2d(um, vm));
      if (off < closest && inside.Perform(uv) == TopAbs_IN) {
        closest = off;
        best = uv;
      }
    }
  }
  return best ? surface.Value(best->X(), best->Y()) : surface.Value(um, vm);
}

gp_Pnt middle(const TopoDS_Edge& edge) {
  const BRepAdaptor_Curve curve(edge);
  return curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
}

// The distance from `at` to a box: none inside it.
double box_distance(const Bnd_Box& box, const gp_Pnt& at) {
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
  const auto off = [](double value, double low, double high) {
    return value < low ? low - value : (value > high ? value - high : 0.0);
  };
  return gp_Pnt(off(at.X(), x0, x1), off(at.Y(), y0, y1), off(at.Z(), z0, z1)).Distance(gp_Pnt(0.0, 0.0, 0.0));
}

// Of the candidates (index, sub-shape of the body before), the index of
// the one nearest to `at` when it is nearer than every other; -1 otherwise.
// The candidates are measured nearest box first, and one whose box lies
// farther than the second nearest found so far is not measured: it can be
// neither (the 63 pieces of a modelled thread's face, mitcad#1464).
int nearest(const std::vector<std::pair<int, TopoDS_Shape>>& candidates, const gp_Pnt& at) {
  std::vector<std::pair<double, std::size_t>> boxes;
  boxes.reserve(candidates.size());
  for (std::size_t k = 0; k < candidates.size(); ++k) {
    Bnd_Box box;
    BRepBndLib::Add(candidates[k].second, box);
    boxes.emplace_back(box_distance(box, at), k);
  }
  std::sort(boxes.begin(), boxes.end());
  const TopoDS_Vertex point = BRepBuilderAPI_MakeVertex(at).Vertex();
  int best = -1;
  double first = std::numeric_limits<double>::infinity();
  double second = std::numeric_limits<double>::infinity();
  for (const auto& [bound, k] : boxes) {
    if (bound > second) {
      break;
    }
    BRepExtrema_DistShapeShape distance(point, candidates[k].second);
    if (!distance.IsDone()) {
      continue;
    }
    const double value = distance.Value();
    if (value < first) {
      second = first;
      first = value;
      best = candidates[k].first;
    } else if (value < second) {
      second = value;
    }
  }
  return best >= 0 && second - first > Precision::Confusion() ? best : -1;
}

// Faces are looked up by matching_faces, not find_faces: which face, not
// the order of several, is asked, and the order measures every one.
ItemOrigin face_origin(const Shape& before, const Shape* after, const std::string& name) {
  const std::vector<int> mine = after != nullptr ? after->matching_faces(name) : std::vector<int>();
  // A face the feature left as it was is the same face on the body before.
  if (mine.size() == 1) {
    const int same = before.face_index(after->face(mine.front()));
    if (same >= 0 && !before.face_names(same).empty()) {
      return found_as(name, before.face_names(same).front());
    }
  }
  const FaceBefore face = face_before(before, name);
  if (face.narrow.empty()) {
    return origin_of(Kind::Made);
  }
  const std::vector<int> wide = before.matching_faces(face.wide);
  int found = wide.size() == 1 ? wide.front() : -1;
  if (found < 0 && mine.size() == 1) {
    std::vector<std::pair<int, TopoDS_Shape>> candidates;
    for (const int f : wide) {
      candidates.emplace_back(f, before.face(f));
    }
    found = nearest(candidates, inner_point(after->face(mine.front())));
  }
  if (found < 0) {
    const std::vector<int> narrow = before.matching_faces(face.narrow);
    found = narrow.size() == 1 ? narrow.front() : -1;
  }
  if (found < 0) {
    return as_named(!before.matching_faces(name).empty(), name);
  }
  const NameList& names = before.face_names(found);
  return found_as(name, names.empty() ? std::string() : names.front());
}

ItemOrigin edge_origin(const Shape& before, const Shape* after, const std::string& name) {
  const std::optional<CompositeName> parsed = parse_composite_name(name);
  if (!parsed || parsed->kind != 'E' || parsed->faces.size() != 2) {
    return origin_of(Kind::Lost);
  }
  const std::vector<int> mine = after != nullptr ? after->find_edges(name) : std::vector<int>();
  // An edge the feature left as it was is the same edge on the body before.
  if (mine.size() == 1) {
    const int same = before.edge_index(after->edge(mine.front()));
    if (same >= 0 && !before.edge_name(same).empty()) {
      return found_as(name, before.edge_name(same));
    }
  }
  const FaceBefore a = face_before(before, parsed->faces[0]);
  const FaceBefore b = face_before(before, parsed->faces[1]);
  if (a.narrow.empty() || b.narrow.empty()) {
    return origin_of(Kind::Made);
  }
  // (Which edges, not their order: matching_edges.)
  const std::vector<int> wide = before.matching_edges(make_edge_name(a.wide, b.wide));
  int found = wide.size() == 1 ? wide.front() : -1;
  if (found < 0 && wide.size() > 1 && mine.size() == 1) {
    std::vector<std::pair<int, TopoDS_Shape>> candidates;
    for (const int e : wide) {
      candidates.emplace_back(e, before.edge(e));
    }
    found = nearest(candidates, middle(after->edge(mine.front())));
  }
  if (found < 0 && !wide.empty()) {
    // As the name gives it: between its faces' narrowest names, by its "#k".
    const std::vector<int> narrow = before.find_edges(make_edge_name(a.narrow, b.narrow, parsed->index));
    found = narrow.size() == 1 ? narrow.front() : -1;
  }
  if (found < 0) {
    return as_named(!before.find_edges(name).empty(), name);
  }
  return found_as(name, before.edge_name(found));
}

ItemOrigin vertex_origin(const Shape& before, const Shape* after, const std::string& name) {
  const std::optional<CompositeName> parsed = parse_composite_name(name);
  if (!parsed || parsed->kind != 'V' || parsed->faces.empty()) {
    return origin_of(Kind::Lost);
  }
  const std::vector<int> mine = after != nullptr ? after->find_vertices(name) : std::vector<int>();
  // A vertex the feature left as it was is the same vertex on the body before.
  if (mine.size() == 1) {
    const int same = before.vertex_index(after->vertex(mine.front()));
    if (same >= 0 && !before.vertex_name(same).empty()) {
      return found_as(name, before.vertex_name(same));
    }
  }
  NameList narrow_faces;
  NameList wide_faces;
  for (const std::string& face : parsed->faces) {
    const FaceBefore found = face_before(before, face);
    if (found.narrow.empty()) {
      return origin_of(Kind::Made);
    }
    narrow_faces.push_back(found.narrow);
    wide_faces.push_back(found.wide);
  }
  const std::vector<int> wide = before.find_vertices(make_vertex_name(wide_faces));
  int found = wide.size() == 1 ? wide.front() : -1;
  if (found < 0 && wide.size() > 1 && mine.size() == 1) {
    std::vector<std::pair<int, TopoDS_Shape>> candidates;
    for (const int v : wide) {
      candidates.emplace_back(v, before.vertex(v));
    }
    found = nearest(candidates, BRep_Tool::Pnt(after->vertex(mine.front())));
  }
  if (found < 0 && !wide.empty()) {
    const std::vector<int> narrow = before.find_vertices(make_vertex_name(narrow_faces, parsed->index));
    found = narrow.size() == 1 ? narrow.front() : -1;
  }
  if (found < 0) {
    return as_named(!before.find_vertices(name).empty(), name);
  }
  return found_as(name, before.vertex_name(found));
}

} // namespace

ItemOrigin item_origin(const Shape& before, const Shape* after, char kind, const std::string& name) {
  try {
    OCC_CATCH_SIGNALS
    switch (kind) {
    case 'F':
      return face_origin(before, after, name);
    case 'E':
      return edge_origin(before, after, name);
    case 'V':
      return vertex_origin(before, after, name);
    default:
      return origin_of(Kind::Lost);
    }
  } catch (const Standard_Failure&) {
    return origin_of(Kind::Lost);
  }
}

} // namespace mitcad::geometry
