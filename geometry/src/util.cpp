// SPDX-License-Identifier: MIT
#include "util.hpp"

#include <algorithm>
#include <array>
#include <atomic>
#include <cctype>
#include <cmath>
#include <cstddef>
#include <optional>
#include <sstream>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BRepAdaptor_Curve.hxx>
#include <BRepBndLib.hxx>
#include <BRepCheck.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepCheck_Result.hxx>
#include <BRepCheck_Solid.hxx>
#include <TopExp_Explorer.hxx>
#include <BRepGProp.hxx>
#include <BRep_Builder.hxx>
#include <BRep_TVertex.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GProp_GProps.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom_Curve.hxx>
#include <Geom_Surface.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <Precision.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopExp.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Iterator.hxx>
#include <TopoDS_Vertex.hxx>
#include <gp_Pnt2d.hxx>

namespace mitcad::geometry::detail {
namespace {

long long rounded(double value) { return std::llround(value * 1.0e6); }

} // namespace

void require_positive(const char* what, double value) {
  if (!(value > 0.0) || !std::isfinite(value)) {
    throw std::invalid_argument(std::string(what) + " must be greater than zero");
  }
}

void require_finite(const char* what, double value) {
  if (!std::isfinite(value)) {
    throw std::invalid_argument(std::string(what) + " must be a finite number");
  }
}

bool has_mesh_faces(const TopoDS_Shape& shape) {
  for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
    TopLoc_Location location;
    if (BRep_Tool::Surface(TopoDS::Face(it.Current()), location).IsNull()) {
      return true;
    }
  }
  return false;
}

void require_surfaces(const TopoDS_Shape& shape, const char* operation) {
  if (has_mesh_faces(shape)) {
    throw std::invalid_argument(std::string(operation) +
                                ": a mesh body (faces without surfaces) cannot be used; the operation "
                                "works on solid and surface bodies");
  }
}

namespace {

// What shape_key and piece_order measure of a shape: its centre of mass,
// its size (volume, area or length; 0 for a vertex) and its bounding box.
struct Measure {
  gp_Pnt centre;
  double size = 0.0;
  double xmin = 0.0, ymin = 0.0, zmin = 0.0, xmax = 0.0, ymax = 0.0, zmax = 0.0;
};

Measure measure(const TopoDS_Shape& shape) {
  GProp_GProps props;
  switch (shape.ShapeType()) {
  case TopAbs_SOLID:
  case TopAbs_COMPSOLID:
  case TopAbs_COMPOUND:
    BRepGProp::VolumeProperties(shape, props);
    break;
  case TopAbs_FACE:
  case TopAbs_SHELL:
    BRepGProp::SurfaceProperties(shape, props);
    break;
  case TopAbs_EDGE:
  case TopAbs_WIRE:
    BRepGProp::LinearProperties(shape, props);
    break;
  default:
    break;
  }
  Measure m;
  if (shape.ShapeType() == TopAbs_VERTEX) {
    m.centre = BRep_Tool::Pnt(TopoDS::Vertex(shape));
  } else {
    m.centre = props.CentreOfMass();
    m.size = props.Mass();
  }
  Bnd_Box box;
  BRepBndLib::Add(shape, box);
  if (!box.IsVoid()) {
    box.Get(m.xmin, m.ymin, m.zmin, m.xmax, m.ymax, m.zmax);
  }
  return m;
}

ShapeKey key_of(const Measure& m) {
  return {rounded(m.centre.X()), rounded(m.centre.Y()), rounded(m.centre.Z()),
          rounded(m.xmin),       rounded(m.ymin),       rounded(m.zmin)};
}

// A piece, edge or vertex as piece_order, edge_order and vertex_order
// order it: its N components in their order, its earlier sort key (the
// rounded components shape_key or edge_key gives), and its position among
// the ones ordered.
template <std::size_t N>
struct PlaceKey {
  std::array<double, N> place{};
  ShapeKey legacy{};
  std::size_t position = 0;
};

// A component to compare: a value that is no number (a centre of nothing)
// compares as 0, so that the order stays a strict one.
double comparable(double value) { return std::isfinite(value) ? value : 0.0; }

// Orders `keys` from `first` to `last` by their places' components from
// `component` on, as piece_order describes.
template <std::size_t N>
void order_places(std::vector<PlaceKey<N>>& keys, std::size_t first, std::size_t last, std::size_t component) {
  if (last - first < 2) {
    return;
  }
  const auto begin = keys.begin() + static_cast<std::ptrdiff_t>(first);
  const auto end = keys.begin() + static_cast<std::ptrdiff_t>(last);
  if (component == N) {
    std::sort(begin, end, [](const PlaceKey<N>& a, const PlaceKey<N>& b) {
      return a.legacy != b.legacy ? a.legacy < b.legacy : a.position < b.position;
    });
    return;
  }
  std::sort(begin, end, [component](const PlaceKey<N>& a, const PlaceKey<N>& b) {
    return a.place[component] < b.place[component];
  });
  std::size_t run = first;
  for (std::size_t k = first + 1; k <= last; ++k) {
    if (k == last || keys[k].place[component] - keys[k - 1].place[component] > kPieceTie) {
      order_places(keys, run, k, component + 1);
      run = k;
    }
  }
}

// The positions of `keys` in their order (order_places from the first
// component).
template <std::size_t N>
std::vector<std::size_t> place_order(std::vector<PlaceKey<N>>& keys) {
  order_places(keys, 0, keys.size(), 0);
  std::vector<std::size_t> order;
  order.reserve(keys.size());
  for (const PlaceKey<N>& key : keys) {
    order.push_back(key.position);
  }
  return order;
}

// The point in the middle of an edge (by parameter); none for a
// degenerated edge, which edge_key places by shape_key instead.
std::optional<gp_Pnt> middle_of(const TopoDS_Edge& edge) {
  if (BRep_Tool::Degenerated(edge)) {
    return std::nullopt;
  }
  const BRepAdaptor_Curve curve(edge);
  return curve.Value((curve.FirstParameter() + curve.LastParameter()) / 2.0);
}

// edge_key of an edge whose middle is `middle` and whose measure is `m`.
ShapeKey edge_key_of(const std::optional<gp_Pnt>& middle, const Measure& m) {
  const ShapeKey whole = key_of(m);
  if (!middle) {
    return whole;
  }
  return {rounded(middle->X()), rounded(middle->Y()), rounded(middle->Z()),
          whole[0],             whole[1],             whole[2]};
}

} // namespace

ShapeKey shape_key(const TopoDS_Shape& shape) { return key_of(measure(shape)); }

std::vector<std::size_t> piece_order(const std::vector<TopoDS_Shape>& pieces) {
  std::vector<PlaceKey<10>> keys(pieces.size());
  for (std::size_t i = 0; i < pieces.size(); ++i) {
    const Measure m = measure(pieces[i]);
    keys[i].place = {comparable(m.centre.X()), comparable(m.centre.Y()), comparable(m.centre.Z()),
                     comparable(m.xmin),       comparable(m.ymin),       comparable(m.zmin),
                     comparable(m.size),       comparable(m.xmax),       comparable(m.ymax),
                     comparable(m.zmax)};
    keys[i].legacy = key_of(m);
    keys[i].position = i;
  }
  return place_order(keys);
}

std::vector<std::size_t> edge_order(const std::vector<TopoDS_Shape>& edges) {
  std::vector<PlaceKey<13>> keys(edges.size());
  for (std::size_t i = 0; i < edges.size(); ++i) {
    const Measure m = measure(edges[i]);
    const std::optional<gp_Pnt> middle = middle_of(TopoDS::Edge(edges[i]));
    // The first six components are the ones edge_key rounds: the middle
    // and the centre of mass, or for a degenerated edge the centre and the
    // lower corner of its box.
    const gp_Pnt first = middle ? *middle : m.centre;
    const gp_Pnt second = middle ? m.centre : gp_Pnt(m.xmin, m.ymin, m.zmin);
    keys[i].place = {comparable(first.X()),  comparable(first.Y()),  comparable(first.Z()),
                     comparable(second.X()), comparable(second.Y()), comparable(second.Z()),
                     comparable(m.size),     comparable(m.xmin),     comparable(m.ymin),
                     comparable(m.zmin),     comparable(m.xmax),     comparable(m.ymax),
                     comparable(m.zmax)};
    keys[i].legacy = edge_key_of(middle, m);
    keys[i].position = i;
  }
  return place_order(keys);
}

std::vector<std::size_t> vertex_order(const std::vector<TopoDS_Shape>& vertices) {
  std::vector<PlaceKey<6>> keys(vertices.size());
  for (std::size_t i = 0; i < vertices.size(); ++i) {
    const Measure m = measure(vertices[i]);
    keys[i].place = {comparable(m.centre.X()), comparable(m.centre.Y()), comparable(m.centre.Z()),
                     comparable(m.xmin),       comparable(m.ymin),       comparable(m.zmin)};
    keys[i].legacy = key_of(m);
    keys[i].position = i;
  }
  return place_order(keys);
}

ShapeKey edge_key(const TopoDS_Shape& edge) {
  return edge_key_of(middle_of(TopoDS::Edge(edge)), measure(edge));
}

namespace {

std::string status_name(BRepCheck_Status status) {
  std::ostringstream text;
  BRepCheck::Print(status, text);
  std::string name = text.str();
  while (!name.empty() && std::isspace(static_cast<unsigned char>(name.back()))) {
    name.pop_back();
  }
  return name;
}

// What made the checker call `face` unorientable (mitcad#1504): the first
// problem of one of its edges or wires in the face (an edge's curve off the
// face's surface, a wire not closed in the face's parameters), which the
// statuses of the face itself do not tell, as "; its <kind>: BRepCheck_...".
std::string unorientable_cause(const BRepCheck_Analyzer& analyzer, const TopoDS_Shape& face) {
  static const std::pair<TopAbs_ShapeEnum, const char*> kinds[] = {
      {TopAbs_EDGE, "edge"}, {TopAbs_WIRE, "wire"}, {TopAbs_VERTEX, "vertex"}};
  for (const auto& [type, kind] : kinds) {
    for (TopExp_Explorer it(face, type); it.More(); it.Next()) {
      const occ::handle<BRepCheck_Result>& result = analyzer.Result(it.Current());
      if (result.IsNull() || !result->IsStatusOnShape(face)) {
        continue;
      }
      for (const BRepCheck_Status status : result->StatusOnShape(face)) {
        if (status != BRepCheck_NoError) {
          return std::string("; its ") + kind + ": " + status_name(status);
        }
      }
    }
  }
  return "";
}

// The first problem the analyzer found, as " (<kind>: BRepCheck_...)" for
// messages: of a sub-shape itself, or of one in the context of another.
std::string first_problem(const BRepCheck_Analyzer& analyzer, const TopoDS_Shape& shape) {
  static const std::pair<TopAbs_ShapeEnum, const char*> kinds[] = {
      {TopAbs_VERTEX, "vertex"}, {TopAbs_EDGE, "edge"},   {TopAbs_WIRE, "wire"},
      {TopAbs_FACE, "face"},     {TopAbs_SHELL, "shell"}, {TopAbs_SOLID, "solid"}};
  for (const auto& [type, kind] : kinds) {
    for (TopExp_Explorer it(shape, type); it.More(); it.Next()) {
      const occ::handle<BRepCheck_Result>& result = analyzer.Result(it.Current());
      if (result.IsNull()) {
        continue;
      }
      for (const BRepCheck_Status status : result->Status()) {
        if (status != BRepCheck_NoError) {
          const std::string cause =
              status == BRepCheck_UnorientableShape && type == TopAbs_FACE ? unorientable_cause(analyzer, it.Current())
                                                                           : std::string();
          return std::string(" (") + kind + ": " + status_name(status) + cause + ")";
        }
      }
      // Sub-shapes checked in this one's context (wires of a face).
      for (const auto& [sub_type, sub_kind] : kinds) {
        for (TopExp_Explorer sub(it.Current(), sub_type); sub.More(); sub.Next()) {
          if (!result->IsStatusOnShape(sub.Current())) {
            continue;
          }
          for (const BRepCheck_Status status : result->StatusOnShape(sub.Current())) {
            if (status != BRepCheck_NoError) {
              return std::string(" (") + sub_kind + " of a " + kind + ": " + status_name(status) + ")";
            }
          }
        }
      }
    }
  }
  return "";
}

} // namespace

// (The checker catches an allocation that fails and calls the sub-shape
// invalid, mitcad#132: that is no answer. It runs on OCCT's thread pool,
// whose jobs count their failed allocations as the launching thread's:
// on a large body the check took as long as the operation, mitcad#181.)
bool is_valid(const TopoDS_Shape& shape) {
  throw_if_cancelled();
  const std::size_t failed = failed_allocations_in_thread();
  const bool valid = BRepCheck_Analyzer(shape, true, true).IsValid();
  throw_if_allocation_failed(failed);
  return valid;
}

void require_valid(const TopoDS_Shape& shape, const char* operation) {
  throw_if_cancelled();
  const std::size_t failed = failed_allocations_in_thread();
  const BRepCheck_Analyzer analyzer(shape, true, true);
  throw_if_allocation_failed(failed);
  if (!analyzer.IsValid()) {
    throw std::runtime_error(std::string(operation) + " produced an invalid shape" + first_problem(analyzer, shape));
  }
}

namespace {

// The faces is_valid_where_changed gives the checker (in a compound), or
// none where the whole shape is to be checked, with the reason: the
// targets were not all found valid (`kept` is empty), the result is not
// solids of shells, a face is not forward or reversed or is there twice,
// an edge of a shell does not have exactly one forward and one reversed
// use in it (a seam: both in its face), a shell is not connected, or more
// than half of the faces changed. In any of those cases the checker on the
// whole shape decides, so what it would say is never guessed. Solids of
// several shells (a body with a void) are listed: the checker sees how
// their shells lie in each other (BRepCheck_Solid) besides the faces.
struct LocalCheck {
  std::optional<TopoDS_Compound> faces;
  const char* whole = nullptr;
  std::vector<TopoDS_Shape> solids;
};

LocalCheck whole_check(const char* reason) {
  LocalCheck check;
  check.whole = reason;
  return check;
}

LocalCheck faces_to_check(const TopoDS_Shape& result, const ShapeMap& kept) {
  if (kept.IsEmpty()) {
    return whole_check("unchecked-target");
  }
  // The shells, as the solids hold them.
  std::vector<TopoDS_Shape> shells;
  std::vector<TopoDS_Shape> several;
  const auto add_solid = [&](const TopoDS_Shape& solid) {
    int count = 0;
    for (TopoDS_Iterator it(solid); it.More(); it.Next()) {
      if (it.Value().ShapeType() != TopAbs_SHELL) {
        return false;
      }
      shells.push_back(it.Value());
      ++count;
    }
    if (count > 1) {
      several.push_back(solid);
    }
    return count > 0;
  };
  if (result.ShapeType() == TopAbs_SOLID) {
    if (!add_solid(result)) {
      return whole_check("solid-not-of-shells");
    }
  } else if (result.ShapeType() == TopAbs_COMPOUND) {
    for (TopoDS_Iterator it(result); it.More(); it.Next()) {
      if (it.Value().ShapeType() != TopAbs_SOLID) {
        return whole_check("not-solids");
      }
      if (!add_solid(it.Value())) {
        return whole_check("solid-not-of-shells");
      }
    }
  } else {
    return whole_check("not-solids");
  }
  if (shells.empty()) {
    return whole_check("no-shells");
  }

  using Uses = NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<int>, TopTools_ShapeMapHasher>;
  ShapeMap all;
  std::vector<TopoDS_Shape> faces;
  std::vector<int> parent;
  const auto find = [&parent](int i) {
    while (parent[static_cast<std::size_t>(i)] != i) {
      parent[static_cast<std::size_t>(i)] = parent[static_cast<std::size_t>(parent[static_cast<std::size_t>(i)])];
      i = parent[static_cast<std::size_t>(i)];
    }
    return i;
  };
  for (const TopoDS_Shape& shell : shells) {
    const int first = static_cast<int>(faces.size());
    // Each edge's forward and reversed uses in the shell, and the first
    // face that uses it.
    NCollection_IndexedDataMap<TopoDS_Shape, std::array<int, 3>, TopTools_ShapeMapHasher> edges;
    for (TopExp_Explorer f(shell, TopAbs_FACE); f.More(); f.Next()) {
      const TopoDS_Shape& face = f.Current();
      if (face.Orientation() != TopAbs_FORWARD && face.Orientation() != TopAbs_REVERSED) {
        return whole_check("face-not-oriented");
      }
      const int before = all.Extent();
      if (all.Add(face) <= before) {
        return whole_check("face-twice");
      }
      const int index = static_cast<int>(faces.size());
      faces.push_back(face);
      parent.push_back(index);
      for (TopExp_Explorer e(face, TopAbs_EDGE); e.More(); e.Next()) {
        const TopoDS_Shape& edge = e.Current();
        if (BRep_Tool::Degenerated(TopoDS::Edge(edge))) {
          continue;
        }
        const TopAbs_Orientation orientation = edge.Orientation();
        if (orientation != TopAbs_FORWARD && orientation != TopAbs_REVERSED) {
          return whole_check("edge-not-oriented");
        }
        int k = edges.FindIndex(edge);
        if (k == 0) {
          k = edges.Add(edge, {0, 0, index});
        }
        std::array<int, 3>& use = edges.ChangeFromIndex(k);
        use[orientation == TopAbs_FORWARD ? 0 : 1] += 1;
        const int a = find(index);
        const int b = find(use[2]);
        if (a != b) {
          parent[static_cast<std::size_t>(a)] = b;
        }
      }
    }
    for (int k = 1; k <= edges.Extent(); ++k) {
      const std::array<int, 3>& use = edges(k);
      if (use[0] != 1 || use[1] != 1) {
        // A free edge, an edge of more than two faces (non-manifold), or
        // two faces at an edge running along it the same way.
        const int uses = use[0] + use[1];
        return whole_check(uses == 1 ? "free-edge" : uses > 2 ? "non-manifold-edge" : "edge-uses-one-way");
      }
    }
    const int root = find(first);
    for (int i = first; i < static_cast<int>(faces.size()); ++i) {
      if (find(i) != root) {
        return whole_check("shell-not-connected");
      }
    }
  }

  // The faces the result does not share with the inputs, and those that
  // meet them at a vertex.
  std::vector<bool> check(faces.size(), false);
  std::size_t changed = 0;
  Uses vertices;
  for (std::size_t i = 0; i < faces.size(); ++i) {
    const int k = kept.FindIndex(faces[i]);
    if (k > 0 && kept(k).IsEqual(faces[i])) {
      continue;
    }
    check[i] = true;
    ++changed;
  }
  if (2 * changed > faces.size()) {
    return whole_check("most-faces-changed");
  }
  for (std::size_t i = 0; i < faces.size(); ++i) {
    ShapeMap own;
    TopExp::MapShapes(faces[i], TopAbs_VERTEX, own);
    for (int v = 1; v <= own.Extent(); ++v) {
      int k = vertices.FindIndex(own(v));
      if (k == 0) {
        k = vertices.Add(own(v), NCollection_List<int>());
      }
      vertices.ChangeFromIndex(k).Append(static_cast<int>(i));
    }
  }
  std::vector<bool> near = check;
  for (int k = 1; k <= vertices.Extent(); ++k) {
    const NCollection_List<int>& users = vertices(k);
    bool touched = false;
    for (const int i : users) {
      touched = touched || check[static_cast<std::size_t>(i)];
    }
    if (touched) {
      for (const int i : users) {
        near[static_cast<std::size_t>(i)] = true;
      }
    }
  }
  BRep_Builder builder;
  TopoDS_Compound compound;
  builder.MakeCompound(compound);
  for (std::size_t i = 0; i < faces.size(); ++i) {
    if (near[i]) {
      builder.Add(compound, faces[i]);
    }
  }
  LocalCheck local;
  local.faces = compound;
  local.solids = std::move(several);
  return local;
}

std::atomic<std::size_t> g_local_checks{0};
std::atomic<std::size_t> g_local_faces{0};

void note_counts(const LocalCheck& check, WhereChanged* report) {
  std::size_t n = 0;
  if (check.faces) {
    g_local_checks += 1;
    for (TopoDS_Iterator it(*check.faces); it.More(); it.Next()) {
      ++n;
    }
    g_local_faces += n;
  }
  if (report != nullptr) {
    report->faces = n;
    report->whole = check.whole;
    report->solids = check.solids.size();
  }
}

// The first problem OCCT's check of a solid finds (how its shells lie in
// each other and face), as " (solid: BRepCheck_...)"; empty when none.
// Failed allocations are no answer, as in is_valid.
std::string solids_problem(const std::vector<TopoDS_Shape>& solids) {
  for (const TopoDS_Shape& solid : solids) {
    throw_if_cancelled();
    const std::size_t failed = failed_allocations_in_thread();
    const occ::handle<BRepCheck_Solid> check = new BRepCheck_Solid(TopoDS::Solid(solid));
    throw_if_allocation_failed(failed);
    for (const BRepCheck_Status status : check->Status()) {
      if (status != BRepCheck_NoError) {
        return " (solid: " + status_name(status) + ")";
      }
    }
  }
  return "";
}

} // namespace

bool is_valid_where_changed(const TopoDS_Shape& result, const ShapeMap& kept, WhereChanged* report) {
  throw_if_cancelled();
  const LocalCheck check = faces_to_check(result, kept);
  note_counts(check, report);
  if (!check.faces) {
    return is_valid(result);
  }
  return is_valid(*check.faces) && solids_problem(check.solids).empty();
}

void require_valid_where_changed(const TopoDS_Shape& result, const ShapeMap& kept, const char* operation,
                                 WhereChanged* report) {
  throw_if_cancelled();
  const LocalCheck check = faces_to_check(result, kept);
  note_counts(check, report);
  if (!check.faces) {
    require_valid(result, operation);
    return;
  }
  require_valid(*check.faces, operation);
  const std::string problem = solids_problem(check.solids);
  if (!problem.empty()) {
    throw std::runtime_error(std::string(operation) + " produced an invalid shape" + problem);
  }
}

ChecksWhereChanged checks_where_changed() { return {g_local_checks.load(), g_local_faces.load()}; }

int inflated_vertices(const TopoDS_Shape& shape, bool tighten) {
  // Above this (mm) a tolerance may be inflated.
  constexpr double kInflated = 1.0;
  using Ancestors = NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;
  Ancestors edges_of;
  Ancestors faces_of;
  TopExp::MapShapesAndUniqueAncestors(shape, TopAbs_VERTEX, TopAbs_EDGE, edges_of);
  int count = 0;
  for (int i = 1; i <= edges_of.Extent(); ++i) {
    const TopoDS_Vertex& vertex = TopoDS::Vertex(edges_of.FindKey(i));
    const double tolerance = BRep_Tool::Tolerance(vertex);
    if (tolerance <= kInflated) {
      continue;
    }
    if (faces_of.IsEmpty()) {
      TopExp::MapShapesAndUniqueAncestors(shape, TopAbs_EDGE, TopAbs_FACE, faces_of);
    }
    // What the edges need: the vertex's distance from their ends, on their
    // curves and on their p-curves' surfaces, and their own tolerances.
    const gp_Pnt at = BRep_Tool::Pnt(vertex);
    double needed = 0.0;
    for (const TopoDS_Shape& e : edges_of(i)) {
      const TopoDS_Edge& edge = TopoDS::Edge(e);
      needed = std::max(needed, BRep_Tool::Tolerance(edge));
      if (BRep_Tool::Degenerated(edge)) {
        continue;
      }
      const double t = BRep_Tool::Parameter(vertex, edge);
      double first = 0.0;
      double last = 0.0;
      if (const occ::handle<Geom_Curve> curve = BRep_Tool::Curve(edge, first, last); !curve.IsNull()) {
        needed = std::max(needed, curve->Value(t).Distance(at));
      }
      const int index = faces_of.FindIndex(edge);
      if (index == 0) {
        continue;
      }
      for (const TopoDS_Shape& f : faces_of(index)) {
        const TopoDS_Face& face = TopoDS::Face(f);
        const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, face, first, last);
        if (!pcurve.IsNull()) {
          const gp_Pnt2d uv = pcurve->Value(t);
          needed = std::max(needed, BRep_Tool::Surface(face)->Value(uv.X(), uv.Y()).Distance(at));
        }
      }
    }
    needed = 1.01 * needed + Precision::Confusion();
    if (tolerance <= 10.0 * needed) {
      continue;
    }
    ++count;
    if (tighten) {
      // (Set directly: BRep_Builder only widens a tolerance.)
      occ::down_cast<BRep_TVertex>(vertex.TShape())->Tolerance(needed);
    }
  }
  return count;
}

double volume_of(const TopoDS_Shape& shape) {
  GProp_GProps props;
  BRepGProp::VolumeProperties(shape, props);
  return props.Mass();
}

TopoDS_Compound compound_of(const std::vector<TopoDS_Shape>& shapes) {
  BRep_Builder builder;
  TopoDS_Compound compound;
  builder.MakeCompound(compound);
  for (const TopoDS_Shape& shape : shapes) {
    builder.Add(compound, shape);
  }
  return compound;
}

TopoDS_Shape compound_or_single(const std::vector<TopoDS_Shape>& shapes) {
  if (shapes.size() == 1) {
    return shapes.front();
  }
  return compound_of(shapes);
}

} // namespace mitcad::geometry::detail
