// SPDX-License-Identifier: MIT
#include "far_features.hpp"

#include <algorithm>
#include <atomic>
#include <cstdlib>
#include <cmath>
#include <cstddef>
#include <cstdint>
#include <unordered_map>

#include <BOPTools_AlgoTools3D.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepTools.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <Precision.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Iterator.hxx>
#include <TopoDS_Shell.hxx>
#include <TopoDS_Solid.hxx>
#include <gp_Dir.hxx>
#include <gp_Pln.hxx>

#include "util.hpp"

namespace mitcad::geometry::detail {
namespace {

using Ancestors =
    NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;

// Below this many holes away from the tool the operation runs on the whole
// body: taking them out and putting them back costs more than it saves.
constexpr std::size_t kMinHoles = 32;

// A face with this many wires holds holes; it is never part of a feature.
constexpr int kHostWires = 4;

// Faces whose normals at an edge between them are closer than this (or
// to opposite) could be merged after the operation: their feature stays.
constexpr double kSameDirection = 1.0e-2;
constexpr double kPi = 3.14159265358979323846;

std::vector<TopoDS_Shape> children(const TopoDS_Shape& shape, TopAbs_ShapeEnum type) {
  std::vector<TopoDS_Shape> found;
  for (TopExp_Explorer it(shape, type); it.More(); it.Next()) {
    found.push_back(it.Current());
  }
  return found;
}

// Union-find over indices.
struct Groups {
  std::vector<std::size_t> parent;
  explicit Groups(std::size_t n) : parent(n) {
    for (std::size_t i = 0; i < n; ++i) {
      parent[i] = i;
    }
  }
  std::size_t find(std::size_t i) {
    while (parent[i] != i) {
      parent[i] = parent[parent[i]];
      i = parent[i];
    }
    return i;
  }
  void join(std::size_t a, std::size_t b) {
    a = find(a);
    b = find(b);
    if (a != b) {
      parent[a] = b;
    }
  }
};

// Whether the faces on both sides of an edge point nearly the same way (or
// opposite ways) in its middle: then they may lie in one surface. True when
// that cannot be measured.
bool same_direction(const TopoDS_Edge& edge, const TopoDS_Face& a, const TopoDS_Face& b) {
  try {
    double first = 0.0;
    double last = 0.0;
    if (BRep_Tool::Curve(edge, first, last).IsNull()) {
      return true;
    }
    const double middle = 0.5 * (first + last);
    gp_Dir na;
    gp_Dir nb;
    BOPTools_AlgoTools3D::GetNormalToFaceOnEdge(edge, a, middle, na);
    BOPTools_AlgoTools3D::GetNormalToFaceOnEdge(edge, b, middle, nb);
    const double angle = na.Angle(nb);
    return angle < kSameDirection || angle > kPi - kSameDirection;
  } catch (const Standard_Failure&) {
    return true;
  }
}

// Whether the holes of a face on `from` can go into a face on `to`: the
// same surface (their edges have their curves on it), or planes in the same
// place (OCCT computes the curves of edges on a plane
// when they are not stored, and Shape stores them).
bool same_surface(const TopoDS_Face& from, const TopoDS_Face& to) {
  TopLoc_Location lf;
  TopLoc_Location lt;
  const occ::handle<Geom_Surface>& sf = BRep_Tool::Surface(from, lf);
  const occ::handle<Geom_Surface>& st = BRep_Tool::Surface(to, lt);
  if (sf == st && lf.IsEqual(lt)) {
    return true;
  }
  const BRepAdaptor_Surface on_from(from, false);
  const BRepAdaptor_Surface on_to(to, false);
  if (on_from.GetType() != GeomAbs_Plane || on_to.GetType() != GeomAbs_Plane) {
    return false;
  }
  const gp_Pln a = on_from.Plane();
  const gp_Pln b = on_to.Plane();
  // Either way round: the holes keep the orientation the body used them
  // in, whichever way the plane's own normal points.
  return a.Axis().Direction().IsParallel(b.Axis().Direction(), Precision::Angular()) &&
         a.Distance(b.Location()) <= Precision::Confusion();
}

// A point in the middle of the wire's first edge.
std::optional<gp_Pnt> point_on(const TopoDS_Shape& wire) {
  for (TopExp_Explorer it(wire, TopAbs_EDGE); it.More(); it.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    const BRepAdaptor_Curve curve(edge);
    return curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
  }
  return std::nullopt;
}

// Whether MITCAD_NO_FAR_FEATURES is set.
bool disabled_by_environment() {
#ifdef _MSC_VER
  char* value = nullptr;
  std::size_t length = 0;
  if (_dupenv_s(&value, &length, "MITCAD_NO_FAR_FEATURES") != 0 || value == nullptr) {
    return false;
  }
  std::free(value);
  return true;
#else
  return std::getenv("MITCAD_NO_FAR_FEATURES") != nullptr;
#endif
}

std::atomic<bool>& enabled() {
  static std::atomic<bool> on{!disabled_by_environment()};
  return on;
}

} // namespace

bool far_features_enabled() { return enabled().load(std::memory_order_relaxed); }

void enable_far_features(bool on) { enabled().store(on, std::memory_order_relaxed); }

std::optional<FarFeatures> FarFeatures::split(const TopoDS_Shape& body, const Bnd_Box& tool_box, bool unified) {
  if (!far_features_enabled()) {
    return std::nullopt;
  }
  const std::vector<TopoDS_Shape> solids = children(body, TopAbs_SOLID);
  if (solids.size() != 1 || children(solids.front(), TopAbs_SHELL).size() != 1 || tool_box.IsVoid()) {
    return std::nullopt;
  }
  if (body.ShapeType() == TopAbs_COMPOUND) {
    // Only the solid: anything else beside it would be lost.
    int parts = 0;
    for (TopoDS_Iterator it(body); it.More(); it.Next()) {
      ++parts;
      if (it.Value().ShapeType() != TopAbs_SOLID) {
        return std::nullopt;
      }
    }
    if (parts != 1) {
      return std::nullopt;
    }
  } else if (body.ShapeType() != TopAbs_SOLID) {
    return std::nullopt;
  }
  const TopoDS_Shape& solid = solids.front();
  ShapeMap faces;
  TopExp::MapShapes(solid, TopAbs_FACE, faces);
  const auto count = static_cast<std::size_t>(faces.Extent());
  if (count < 2 * kMinHoles) {
    return std::nullopt;
  }

  // Faces of features: boxes apart from the tool's (with a margin), and
  // not themselves faces with several holes (the bottom of a plate whose
  // top a pocket cuts: its holes are taken out of it instead).
  Bnd_Box near_box = tool_box;
  near_box.Enlarge(1.0e-3 * std::max(1.0, std::sqrt(tool_box.SquareExtent())));
  std::vector<bool> far(count, false);
  for (std::size_t i = 0; i < count; ++i) {
    const TopoDS_Shape& face = faces(static_cast<int>(i) + 1);
    int wires = 0;
    for (TopoDS_Iterator w(face); w.More() && wires < kHostWires; w.Next()) {
      ++wires;
    }
    if (wires >= kHostWires) {
      continue;
    }
    Bnd_Box box;
    BRepBndLib::Add(face, box, false);
    far[i] = box.IsOut(near_box);
  }

  // Features: far faces joined through edges between far faces.
  Ancestors edge_faces;
  TopExp::MapShapesAndUniqueAncestors(solid, TopAbs_EDGE, TopAbs_FACE, edge_faces);
  const auto index_of = [&](const TopoDS_Shape& face) {
    return static_cast<std::size_t>(faces.FindIndex(face) - 1);
  };
  Groups groups(count);
  std::vector<bool> usable(count, true);
  for (int k = 1; k <= edge_faces.Extent(); ++k) {
    const TopoDS_Edge& edge = TopoDS::Edge(edge_faces.FindKey(k));
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    const NCollection_List<TopoDS_Shape>& around = edge_faces(k);
    if (around.Extent() > 2) {
      // Non-manifold: its far faces stay.
      for (const TopoDS_Shape& face : around) {
        usable[index_of(face)] = false;
      }
      continue;
    }
    if (around.Extent() != 2) {
      continue; // a seam, or a free edge
    }
    const std::size_t a = index_of(around.First());
    const std::size_t b = index_of(around.Last());
    if (far[a] && far[b]) {
      groups.join(a, b);
    }
    if (!unified && (far[a] || far[b]) &&
        same_direction(edge, TopoDS::Face(around.First()), TopoDS::Face(around.Last()))) {
      // A neighbour the merge after the operation could unite with it.
      usable[a] = usable[a] && !far[a];
      usable[b] = usable[b] && !far[b];
    }
  }
  // A vertex between only two edges that run along the same faces: the
  // merge could join the edges.
  if (!unified) {
    Ancestors vertex_edges;
    TopExp::MapShapesAndUniqueAncestors(solid, TopAbs_VERTEX, TopAbs_EDGE, vertex_edges);
    for (int k = 1; k <= vertex_edges.Extent(); ++k) {
      std::vector<TopoDS_Shape> edges;
      for (const TopoDS_Shape& edge : vertex_edges(k)) {
        if (!BRep_Tool::Degenerated(TopoDS::Edge(edge))) {
          edges.push_back(edge);
        }
      }
      if (edges.size() != 2 || !edge_faces.Contains(edges[0]) || !edge_faces.Contains(edges[1])) {
        continue;
      }
      const NCollection_List<TopoDS_Shape>& fa = edge_faces.FindFromKey(edges[0]);
      const NCollection_List<TopoDS_Shape>& fb = edge_faces.FindFromKey(edges[1]);
      bool same = fa.Extent() == fb.Extent();
      for (const TopoDS_Shape& face : fa) {
        same = same && std::any_of(fb.begin(), fb.end(), [&](const TopoDS_Shape& f) { return f.IsSame(face); });
      }
      if (same) {
        for (const TopoDS_Shape& face : fa) {
          usable[index_of(face)] = false;
        }
      }
    }
  }

  // The holes: inner wires of near faces made only of edges whose other
  // face belongs to one feature.
  std::unordered_map<std::size_t, std::size_t> feature_of_root; // root -> feature number
  std::vector<std::size_t> root_of_feature;
  std::vector<bool> feature_ok;
  std::vector<std::size_t> boundary(0); // per feature: edges on its boundary
  std::vector<std::size_t> covered;     // per feature: of them in holes
  const auto feature = [&](std::size_t face) {
    const std::size_t root = groups.find(face);
    const auto found = feature_of_root.find(root);
    if (found != feature_of_root.end()) {
      return found->second;
    }
    const std::size_t number = root_of_feature.size();
    feature_of_root.emplace(root, number);
    root_of_feature.push_back(root);
    feature_ok.push_back(true);
    boundary.push_back(0);
    covered.push_back(0);
    return number;
  };
  for (std::size_t i = 0; i < count; ++i) {
    if (far[i]) {
      const std::size_t f = feature(i);
      feature_ok[f] = feature_ok[f] && usable[i];
    }
  }
  for (int k = 1; k <= edge_faces.Extent(); ++k) {
    const NCollection_List<TopoDS_Shape>& around = edge_faces(k);
    if (around.Extent() != 2 || BRep_Tool::Degenerated(TopoDS::Edge(edge_faces.FindKey(k)))) {
      continue;
    }
    const std::size_t a = index_of(around.First());
    const std::size_t b = index_of(around.Last());
    if (far[a] != far[b]) {
      boundary[feature(far[a] ? a : b)] += 1;
    }
  }

  struct Found {
    std::size_t face;
    TopoDS_Shape wire; // the face's own (not located, oriented within it)
    std::size_t feature;
  };
  std::vector<Found> found;
  for (std::size_t i = 0; i < count; ++i) {
    if (far[i]) {
      continue;
    }
    const TopoDS_Face& face = TopoDS::Face(faces(static_cast<int>(i) + 1));
    TopoDS_Shape outer;
    bool checked_outer = false;
    for (TopoDS_Iterator w(face, false, false); w.More(); w.Next()) {
      const TopoDS_Shape& wire = w.Value();
      std::optional<std::size_t> owner;
      bool hole = true;
      std::size_t edges = 0;
      for (TopExp_Explorer e(wire, TopAbs_EDGE); e.More() && hole; e.Next()) {
        if (BRep_Tool::Degenerated(TopoDS::Edge(e.Current()))) {
          continue;
        }
        const NCollection_List<TopoDS_Shape>* around = edge_faces.Seek(e.Current());
        if (around == nullptr || around->Extent() != 2) {
          hole = false;
          break;
        }
        const std::size_t a = index_of(around->First());
        const std::size_t b = index_of(around->Last());
        const std::size_t other = a == i ? b : a;
        if (!far[other] || (a != i && b != i)) {
          hole = false;
          break;
        }
        const std::size_t f = feature(other);
        if (owner && *owner != f) {
          hole = false;
          break;
        }
        owner = f;
        ++edges;
      }
      if (!owner) {
        continue;
      }
      if (hole && !checked_outer) {
        outer = BRepTools::OuterWire(face);
        checked_outer = true;
      }
      if (!hole || wire.IsSame(outer) || !wire.Location().IsIdentity()) {
        // Part of a feature's boundary that is not a hole of this face.
        feature_ok[*owner] = false;
        continue;
      }
      covered[*owner] += edges;
      found.push_back({i, wire, *owner});
    }
  }
  std::size_t holes = 0;
  for (const Found& hole : found) {
    if (feature_ok[hole.feature] && covered[hole.feature] == boundary[hole.feature]) {
      ++holes;
    }
  }
  if (holes < kMinHoles) {
    return std::nullopt;
  }

  // The reduced body: the hosts without their holes, the features left out.
  FarFeatures result;
  std::vector<std::size_t> feature_number(root_of_feature.size(), SIZE_MAX);
  std::unordered_map<std::size_t, std::size_t> host_of_face;
  std::vector<ShapeMap> host_holes;
  for (const Found& hole : found) {
    if (!feature_ok[hole.feature] || covered[hole.feature] != boundary[hole.feature]) {
      continue;
    }
    if (feature_number[hole.feature] == SIZE_MAX) {
      feature_number[hole.feature] = result.m_features.size();
      result.m_features.emplace_back();
    }
    auto host = host_of_face.find(hole.face);
    if (host == host_of_face.end()) {
      host = host_of_face.emplace(hole.face, result.m_hosts.size()).first;
      result.m_hosts.push_back({faces(static_cast<int>(hole.face) + 1), TopoDS_Shape()});
      host_holes.emplace_back();
    }
    host_holes[host->second].Add(hole.wire);
    result.m_holes.push_back({hole.wire, host->second, feature_number[hole.feature]});
  }
  for (std::size_t i = 0; i < count; ++i) {
    if (far[i] && feature_number[feature(i)] != SIZE_MAX) {
      result.m_removed.Add(faces(static_cast<int>(i) + 1));
    }
  }

  BRep_Builder builder;
  for (std::size_t h = 0; h < result.m_hosts.size(); ++h) {
    const TopoDS_Shape& face = result.m_hosts[h].face;
    if (!face.Location().IsIdentity()) {
      return std::nullopt;
    }
    // A new face on the same surface with the face's other wires, built
    // forward (BRep_Builder::Add composes the wires with the orientation
    // of the face it adds them to).
    TopoDS_Face stand_in = TopoDS::Face(face.Oriented(TopAbs_FORWARD).EmptyCopied());
    builder.NaturalRestriction(stand_in, BRep_Tool::NaturalRestriction(TopoDS::Face(face)));
    for (TopoDS_Iterator w(face, false, false); w.More(); w.Next()) {
      if (!host_holes[h].Contains(w.Value())) {
        builder.Add(stand_in, w.Value());
      }
    }
    stand_in.Orientable(face.Orientable());
    stand_in.Closed(face.Closed());
    result.m_hosts[h].stand_in = stand_in;
  }
  // Faces as the solid uses them, in a new shell and solid.
  std::unordered_map<std::size_t, std::size_t> host_at;
  for (std::size_t h = 0; h < result.m_hosts.size(); ++h) {
    host_at.emplace(index_of(result.m_hosts[h].face), h);
  }
  TopoDS_Shell shell;
  builder.MakeShell(shell);
  for (TopExp_Explorer f(solid, TopAbs_FACE); f.More(); f.Next()) {
    const std::size_t i = index_of(f.Current());
    if (result.m_removed.Contains(f.Current())) {
      const std::size_t number = feature_number[feature(i)];
      result.m_features[number].push_back(f.Current());
      continue;
    }
    const auto host = host_at.find(i);
    if (host == host_at.end()) {
      builder.Add(shell, f.Current());
      continue;
    }
    // Hosts keep their orientation: the explorer gives them as the solid
    // uses them, and the shell and solid are forward.
    Host& h = result.m_hosts[host->second];
    h.face = f.Current();
    builder.Add(shell, h.stand_in.Oriented(f.Current().Orientation()));
  }
  shell.Closed(true);
  TopoDS_Solid reduced;
  builder.MakeSolid(reduced);
  builder.Add(reduced, shell);
  if (body.ShapeType() == TopAbs_COMPOUND) {
    TopoDS_Compound compound;
    builder.MakeCompound(compound);
    builder.Add(compound, reduced);
    result.m_reduced = compound;
  } else {
    result.m_reduced = reduced;
  }
  return result;
}

TopoDS_Shape FarFeatures::stand_in(const TopoDS_Shape& face) const {
  if (m_removed.Contains(face)) {
    return TopoDS_Shape();
  }
  for (const Host& host : m_hosts) {
    if (host.face.IsSame(face)) {
      return host.stand_in.Oriented(face.Orientation());
    }
  }
  return face;
}

std::optional<FarFeatures::Restored> FarFeatures::restore(const TopoDS_Shape& result,
                                                          BRepBuilderAPI_MakeShape& operation) const {
  ShapeMap result_faces;
  TopExp::MapShapes(result, TopAbs_FACE, result_faces);
  // The holes of each face of the result, and the shell of each feature.
  std::vector<std::vector<std::size_t>> holes_of(static_cast<std::size_t>(result_faces.Extent()));
  std::vector<int> feature_face(m_features.size(), 0);
  for (std::size_t h = 0; h < m_hosts.size(); ++h) {
    const TopoDS_Shape& stand_in = m_hosts[h].stand_in;
    std::vector<int> images;
    if (!operation.IsDeleted(stand_in)) {
      const NCollection_List<TopoDS_Shape>& modified = operation.Modified(stand_in);
      if (modified.IsEmpty()) {
        images.push_back(result_faces.FindIndex(stand_in));
      }
      for (const TopoDS_Shape& image : modified) {
        images.push_back(result_faces.FindIndex(image));
      }
    }
    images.erase(std::remove(images.begin(), images.end(), 0), images.end());
    if (images.empty()) {
      return std::nullopt;
    }
    for (std::size_t k = 0; k < m_holes.size(); ++k) {
      const Hole& hole = m_holes[k];
      if (hole.host != h) {
        continue;
      }
      int at = 0;
      if (images.size() == 1) {
        at = images.front();
      } else {
        const std::optional<gp_Pnt> point = point_on(hole.wire);
        if (!point) {
          return std::nullopt;
        }
        for (int image : images) {
          const BRepClass_FaceClassifier inside(TopoDS::Face(result_faces(image)), *point, Precision::Confusion());
          if (inside.State() == TopAbs_IN) {
            if (at != 0) {
              return std::nullopt;
            }
            at = image;
          }
        }
        if (at == 0) {
          return std::nullopt;
        }
      }
      holes_of[static_cast<std::size_t>(at - 1)].push_back(k);
      feature_face[hole.feature] = at;
    }
  }

  // The faces that get holes back, as the result uses them.
  std::vector<std::size_t> of_host(m_hosts.size(), 0);
  for (const Hole& hole : m_holes) {
    of_host[hole.host] += 1;
  }
  Restored restored;
  BRep_Builder builder;
  std::unordered_map<int, TopoDS_Shape> replacement;
  const auto replaced = [&](const TopoDS_Shape& face) -> TopoDS_Shape {
    const int index = result_faces.FindIndex(face);
    const std::vector<std::size_t>& holes = holes_of[static_cast<std::size_t>(index - 1)];
    if (holes.empty()) {
      return face;
    }
    const auto known = replacement.find(index);
    if (known != replacement.end()) {
      return known->second.Oriented(face.Orientation());
    }
    if (!face.Location().IsIdentity()) {
      return TopoDS_Shape();
    }
    TopoDS_Shape made;
    const std::size_t first = m_holes[holes.front()].host;
    const Host& host = m_hosts[first];
    // A face the operation did not change, with all its holes, is the
    // body's own face.
    if (face.IsSame(host.stand_in) && holes.size() == of_host[first] &&
        face.Orientation() == host.face.Orientation()) {
      made = host.face;
    } else {
      TopoDS_Face face_made = TopoDS::Face(face.Oriented(TopAbs_FORWARD).EmptyCopied());
      builder.NaturalRestriction(face_made, BRep_Tool::NaturalRestriction(TopoDS::Face(face)));
      for (TopoDS_Iterator w(face, false, false); w.More(); w.Next()) {
        builder.Add(face_made, w.Value());
      }
      for (std::size_t k : holes) {
        const Hole& hole = m_holes[k];
        const Host& from = m_hosts[hole.host];
        if (!same_surface(TopoDS::Face(from.face), TopoDS::Face(face))) {
          return TopoDS_Shape();
        }
        // The hole's edges keep the orientation the body gave them: within
        // the new face the wire turns when the face is used the other way
        // round than its host was.
        TopoDS_Shape wire = hole.wire;
        if (face.Orientation() != from.face.Orientation()) {
          wire.Reverse();
        }
        builder.Add(face_made, wire);
      }
      face_made.Orientable(face.Orientable());
      face_made.Closed(face.Closed());
      made = face_made.Oriented(face.Orientation());
    }
    restored.replaced.emplace_back(face, made);
    replacement.emplace(index, made.Oriented(TopAbs_FORWARD));
    return made.Oriented(face.Orientation());
  };

  // Solids of one shell, rebuilt with the faces replaced and each
  // feature's faces in the shell of the face that holds its first hole.
  std::vector<int> pending(m_features.size());
  for (std::size_t f = 0; f < m_features.size(); ++f) {
    pending[f] = feature_face[f];
    if (pending[f] == 0) {
      return std::nullopt;
    }
  }
  TopoDS_Compound compound;
  builder.MakeCompound(compound);
  std::size_t placed = 0;
  for (TopoDS_Iterator part(result); part.More(); part.Next()) {
    if (part.Value().ShapeType() != TopAbs_SOLID) {
      return std::nullopt;
    }
    const std::vector<TopoDS_Shape> shells = children(part.Value(), TopAbs_SHELL);
    if (shells.size() != 1) {
      return std::nullopt;
    }
    TopoDS_Shell shell;
    builder.MakeShell(shell);
    ShapeMap own;
    for (TopExp_Explorer f(shells.front(), TopAbs_FACE); f.More(); f.Next()) {
      const TopoDS_Shape face = replaced(f.Current());
      if (face.IsNull()) {
        return std::nullopt;
      }
      builder.Add(shell, face);
      own.Add(f.Current());
    }
    for (std::size_t f = 0; f < m_features.size(); ++f) {
      if (own.Contains(result_faces(pending[f]))) {
        for (const TopoDS_Shape& face : m_features[f]) {
          builder.Add(shell, face);
        }
        ++placed;
      }
    }
    shell.Closed(true);
    TopoDS_Solid solid;
    builder.MakeSolid(solid);
    builder.Add(solid, shell);
    builder.Add(compound, solid);
  }
  if (placed != m_features.size()) {
    return std::nullopt;
  }
  restored.shape = compound;
  return restored;
}

} // namespace mitcad::geometry::detail
