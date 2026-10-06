// SPDX-License-Identifier: MIT
#include "mitcad/geometry/shape.hpp"

#include <algorithm>
#include <map>
#include <unordered_map>
#include <utility>

#include <BRep_Tool.hxx>
#include <NCollection_IndexedDataMap.hxx>
#include <NCollection_List.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>

#include "util.hpp"

namespace mitcad::geometry {
namespace {

using AncestorMap =
    NCollection_IndexedDataMap<TopoDS_Shape, NCollection_List<TopoDS_Shape>, TopTools_ShapeMapHasher>;

const NameList kNoNames;

// The faces each reference matches (name_matches): every name of a face
// and its parts before each '#'. Faces in increasing order.
using ReferenceIndex = std::unordered_map<std::string, std::vector<int>>;

ReferenceIndex index_references(const std::vector<NameList>& face_names) {
  ReferenceIndex index;
  for (std::size_t f = 0; f < face_names.size(); ++f) {
    const int face = static_cast<int>(f);
    const auto add = [&index, face](std::string reference) {
      std::vector<int>& faces = index[std::move(reference)];
      if (faces.empty() || faces.back() != face) {
        faces.push_back(face);
      }
    };
    for (const std::string& name : face_names[f]) {
      add(name);
      for (std::size_t at = name.find('#'); at != std::string::npos; at = name.find('#', at + 1)) {
        add(name.substr(0, at));
      }
    }
  }
  return index;
}

// The edges or vertices (`adjacent`, per face) of the faces a reference
// matches, in increasing order: the only ones whose faces can match a
// set of references that includes it. Trying every edge and vertex of the
// shape for each name instead made naming quadratic: the results of a
// large body's features spent a fifth of their time on it.
std::vector<int> adjacent_to(const ReferenceIndex& index, const std::string& reference,
                             const std::vector<std::vector<int>>& adjacent) {
  std::vector<int> found;
  const auto faces = index.find(reference);
  if (faces == index.end()) {
    return found;
  }
  for (int face : faces->second) {
    const std::vector<int>& near = adjacent[static_cast<std::size_t>(face)];
    found.insert(found.end(), near.begin(), near.end());
  }
  std::sort(found.begin(), found.end());
  found.erase(std::unique(found.begin(), found.end()), found.end());
  return found;
}

} // namespace

Shape::Shape(TopoDS_Shape shape, const std::vector<NamedFace>& faces) : m_shape(std::move(shape)) {
  if (m_shape.IsNull()) {
    return;
  }
  TopExp::MapShapes(m_shape, TopAbs_FACE, m_faces);
  TopExp::MapShapes(m_shape, TopAbs_EDGE, m_edges);
  TopExp::MapShapes(m_shape, TopAbs_VERTEX, m_vertices);
  m_faceNames.resize(static_cast<std::size_t>(m_faces.Extent()));
  for (const NamedFace& named : faces) {
    const int index = m_faces.FindIndex(named.face);
    if (index > 0) {
      NameList& names = m_faceNames[static_cast<std::size_t>(index - 1)];
      names.insert(names.end(), named.names.begin(), named.names.end());
    }
  }
  for (NameList& names : m_faceNames) {
    std::sort(names.begin(), names.end());
    names.erase(std::unique(names.begin(), names.end()), names.end());
  }
  derive_edge_names();
  derive_vertex_names();
  if (detail::input_checks.load(std::memory_order_relaxed)) {
    detail::shape_made(*this, m_shape);
    m_checked = true;
  }
}

Shape::~Shape() {
  if (m_checked) {
    detail::shape_gone(*this);
  }
}

const TopoDS_Face& Shape::face(int i) const {
  if (detail::input_checks.load(std::memory_order_relaxed)) {
    detail::shape_read(*this);
  }
  return TopoDS::Face(m_faces(i + 1));
}

const TopoDS_Edge& Shape::edge(int i) const {
  if (detail::input_checks.load(std::memory_order_relaxed)) {
    detail::shape_read(*this);
  }
  return TopoDS::Edge(m_edges(i + 1));
}

const TopoDS_Vertex& Shape::vertex(int i) const {
  if (detail::input_checks.load(std::memory_order_relaxed)) {
    detail::shape_read(*this);
  }
  return TopoDS::Vertex(m_vertices(i + 1));
}

const NameList& Shape::names_of_face(const TopoDS_Shape& face) const {
  const int i = face_index(face);
  return i < 0 ? kNoNames : m_faceNames[static_cast<std::size_t>(i)];
}

std::optional<std::string> Shape::name_of_edge(const TopoDS_Shape& edge) const {
  const int i = edge_index(edge);
  if (i < 0 || m_edgeNames[static_cast<std::size_t>(i)].empty()) {
    return std::nullopt;
  }
  return m_edgeNames[static_cast<std::size_t>(i)];
}

std::optional<std::string> Shape::name_of_vertex(const TopoDS_Shape& vertex) const {
  const int i = vertex_index(vertex);
  if (i < 0 || m_vertexNames[static_cast<std::size_t>(i)].empty()) {
    return std::nullopt;
  }
  return m_vertexNames[static_cast<std::size_t>(i)];
}

void Shape::derive_edge_names() {
  AncestorMap ancestors;
  TopExp::MapShapesAndUniqueAncestors(m_shape, TopAbs_EDGE, TopAbs_FACE, ancestors);
  const auto primary = [this](int f) -> const std::string& {
    static const std::string none;
    return f < 0 || m_faceNames[static_cast<std::size_t>(f)].empty()
               ? none
               : m_faceNames[static_cast<std::size_t>(f)].front();
  };

  const auto count = static_cast<std::size_t>(m_edges.Extent());
  m_edgeFaces.assign(count, {-1, -1});
  m_edgeNames.assign(count, std::string());
  std::vector<std::string> base(count);
  for (int i = 0; i < m_edges.Extent(); ++i) {
    const TopoDS_Edge& e = edge(i);
    if (BRep_Tool::Degenerated(e) || !ancestors.Contains(e)) {
      continue;
    }
    std::vector<int> faces;
    for (const TopoDS_Shape& ancestor : ancestors.FindFromKey(e)) {
      const int f = face_index(ancestor);
      if (f >= 0 && std::find(faces.begin(), faces.end(), f) == faces.end()) {
        faces.push_back(f);
      }
    }
    // Named faces first, by primary name; the edge is named after the first two.
    std::sort(faces.begin(), faces.end(), [&primary](int a, int b) {
      const std::string& pa = primary(a);
      const std::string& pb = primary(b);
      if (pa.empty() != pb.empty()) {
        return !pa.empty();
      }
      return pa != pb ? pa < pb : a < b;
    });
    std::array<int, 2> pair{-1, -1};
    if (faces.size() == 1) {
      pair = {faces[0], BRep_Tool::IsClosed(e, face(faces[0])) ? faces[0] : -1};
    } else if (faces.size() >= 2) {
      pair = {faces[0], faces[1]};
    }
    m_edgeFaces[static_cast<std::size_t>(i)] = pair;
    if (pair[0] >= 0 && pair[1] >= 0 && !primary(pair[0]).empty() && !primary(pair[1]).empty()) {
      base[static_cast<std::size_t>(i)] = make_edge_name(primary(pair[0]), primary(pair[1]));
    }
  }

  std::map<std::string, std::vector<int>> groups;
  for (std::size_t i = 0; i < count; ++i) {
    if (!base[i].empty()) {
      groups[base[i]].push_back(static_cast<int>(i));
    }
  }
  if (groups.empty()) {
    return;
  }
  const ReferenceIndex index = index_references(m_faceNames);
  std::vector<std::vector<int>> face_edges(m_faceNames.size());
  for (std::size_t i = 0; i < count; ++i) {
    const std::array<int, 2> pair = m_edgeFaces[i];
    for (int k = 0; k < 2; ++k) {
      if (pair[k] >= 0 && (k == 0 || pair[1] != pair[0])) {
        face_edges[static_cast<std::size_t>(pair[k])].push_back(static_cast<int>(i));
      }
    }
  }
  for (const auto& [name, members] : groups) {
    const std::array<int, 2> pair = m_edgeFaces[static_cast<std::size_t>(members.front())];
    const std::string& a = primary(pair[0]);
    const std::string& b = primary(pair[1]);
    // Number the edges the way a reference resolves: among all edges between
    // faces matching the two names.
    std::vector<int> candidates;
    for (int j : adjacent_to(index, a, face_edges)) {
      if (edge_matches(j, a, b)) {
        candidates.push_back(j);
      }
    }
    if (candidates.size() <= 1) {
      m_edgeNames[static_cast<std::size_t>(members.front())] = name;
      continue;
    }
    const std::vector<int> sorted = sorted_edges(candidates);
    for (int member : members) {
      const auto position = std::find(sorted.begin(), sorted.end(), member) - sorted.begin();
      m_edgeNames[static_cast<std::size_t>(member)] =
          make_edge_name(a, b, static_cast<int>(position));
    }
  }
}

void Shape::derive_vertex_names() {
  AncestorMap ancestors;
  TopExp::MapShapesAndUniqueAncestors(m_shape, TopAbs_VERTEX, TopAbs_FACE, ancestors);
  const auto count = static_cast<std::size_t>(m_vertices.Extent());
  m_vertexFaces.assign(count, {});
  m_vertexNames.assign(count, std::string());
  std::vector<NameList> primaries(count);
  for (int i = 0; i < m_vertices.Extent(); ++i) {
    const TopoDS_Vertex& v = vertex(i);
    if (!ancestors.Contains(v)) {
      continue;
    }
    std::vector<int>& faces = m_vertexFaces[static_cast<std::size_t>(i)];
    bool named = true;
    for (const TopoDS_Shape& ancestor : ancestors.FindFromKey(v)) {
      const int f = face_index(ancestor);
      if (f < 0 || std::find(faces.begin(), faces.end(), f) != faces.end()) {
        continue;
      }
      faces.push_back(f);
      const NameList& names = m_faceNames[static_cast<std::size_t>(f)];
      if (names.empty()) {
        named = false;
      } else {
        primaries[static_cast<std::size_t>(i)].push_back(names.front());
      }
    }
    if (!named || faces.empty()) {
      primaries[static_cast<std::size_t>(i)].clear();
    }
    std::sort(primaries[static_cast<std::size_t>(i)].begin(),
              primaries[static_cast<std::size_t>(i)].end());
  }

  std::map<NameList, std::vector<int>> groups;
  for (std::size_t i = 0; i < count; ++i) {
    if (!primaries[i].empty()) {
      groups[primaries[i]].push_back(static_cast<int>(i));
    }
  }
  if (groups.empty()) {
    return;
  }
  const ReferenceIndex index = index_references(m_faceNames);
  std::vector<std::vector<int>> face_vertices(m_faceNames.size());
  for (std::size_t i = 0; i < count; ++i) {
    for (int f : m_vertexFaces[i]) {
      face_vertices[static_cast<std::size_t>(f)].push_back(static_cast<int>(i));
    }
  }
  for (const auto& [faces, members] : groups) {
    std::vector<int> candidates;
    for (int j : adjacent_to(index, faces.front(), face_vertices)) {
      if (vertex_matches(j, faces)) {
        candidates.push_back(j);
      }
    }
    if (candidates.size() <= 1) {
      m_vertexNames[static_cast<std::size_t>(members.front())] = make_vertex_name(faces);
      continue;
    }
    const std::vector<int> sorted = sorted_vertices(candidates);
    for (int member : members) {
      const auto position = std::find(sorted.begin(), sorted.end(), member) - sorted.begin();
      m_vertexNames[static_cast<std::size_t>(member)] =
          make_vertex_name(faces, static_cast<int>(position));
    }
  }
}

bool Shape::edge_matches(int i, const std::string& a, const std::string& b) const {
  const std::array<int, 2> pair = m_edgeFaces[static_cast<std::size_t>(i)];
  if (pair[0] < 0 || pair[1] < 0) {
    return false;
  }
  const NameList& first = m_faceNames[static_cast<std::size_t>(pair[0])];
  const NameList& second = m_faceNames[static_cast<std::size_t>(pair[1])];
  return (face_matches(first, a) && face_matches(second, b)) ||
         (face_matches(first, b) && face_matches(second, a));
}

bool Shape::vertex_matches(int i, const NameList& faces) const {
  const std::vector<int>& at = m_vertexFaces[static_cast<std::size_t>(i)];
  if (at.size() != faces.size()) {
    return false;
  }
  std::vector<bool> used(at.size(), false);
  for (const std::string& reference : faces) {
    bool found = false;
    for (std::size_t k = 0; k < at.size() && !found; ++k) {
      if (!used[k] && face_matches(m_faceNames[static_cast<std::size_t>(at[k])], reference)) {
        used[k] = true;
        found = true;
      }
    }
    if (!found) {
      return false;
    }
  }
  return true;
}

std::vector<int> Shape::sorted_edges(std::vector<int> edges) const {
  std::vector<std::pair<detail::ShapeKey, int>> keyed;
  for (int i : edges) {
    keyed.emplace_back(detail::edge_key(edge(i)), i);
  }
  std::sort(keyed.begin(), keyed.end());
  for (std::size_t k = 0; k < keyed.size(); ++k) {
    edges[k] = keyed[k].second;
  }
  return edges;
}

std::vector<int> Shape::sorted_vertices(std::vector<int> vertices) const {
  std::vector<std::pair<detail::ShapeKey, int>> keyed;
  for (int i : vertices) {
    keyed.emplace_back(detail::shape_key(vertex(i)), i);
  }
  std::sort(keyed.begin(), keyed.end());
  for (std::size_t k = 0; k < keyed.size(); ++k) {
    vertices[k] = keyed[k].second;
  }
  return vertices;
}

std::vector<int> Shape::find_faces(const std::string& reference) const {
  std::vector<std::pair<detail::ShapeKey, int>> keyed;
  for (int i = 0; i < face_count(); ++i) {
    if (face_matches(m_faceNames[static_cast<std::size_t>(i)], reference)) {
      keyed.emplace_back(detail::shape_key(face(i)), i);
    }
  }
  std::sort(keyed.begin(), keyed.end());
  std::vector<int> found;
  for (const auto& entry : keyed) {
    found.push_back(entry.second);
  }
  return found;
}

std::vector<int> Shape::find_edges(const std::string& reference) const {
  const auto name = parse_composite_name(reference);
  if (!name || name->kind != 'E') {
    return {};
  }
  std::vector<int> candidates;
  for (int i = 0; i < edge_count(); ++i) {
    if (edge_matches(i, name->faces[0], name->faces[1])) {
      candidates.push_back(i);
    }
  }
  candidates = sorted_edges(candidates);
  if (name->index < 0) {
    return candidates;
  }
  if (static_cast<std::size_t>(name->index) >= candidates.size()) {
    return {};
  }
  return {candidates[static_cast<std::size_t>(name->index)]};
}

std::vector<int> Shape::find_vertices(const std::string& reference) const {
  const auto name = parse_composite_name(reference);
  if (!name || name->kind != 'V') {
    return {};
  }
  std::vector<int> candidates;
  for (int i = 0; i < vertex_count(); ++i) {
    if (vertex_matches(i, name->faces)) {
      candidates.push_back(i);
    }
  }
  candidates = sorted_vertices(candidates);
  if (name->index < 0) {
    return candidates;
  }
  if (static_cast<std::size_t>(name->index) >= candidates.size()) {
    return {};
  }
  return {candidates[static_cast<std::size_t>(name->index)]};
}

std::vector<ShapePtr> solids(const Shape& shape) {
  std::vector<std::pair<detail::ShapeKey, ShapePtr>> keyed;
  for (TopExp_Explorer it(shape.occt(), TopAbs_SOLID); it.More(); it.Next()) {
    std::vector<Shape::NamedFace> faces;
    for (TopExp_Explorer f(it.Current(), TopAbs_FACE); f.More(); f.Next()) {
      const NameList& names = shape.names_of_face(f.Current());
      if (!names.empty()) {
        faces.push_back({f.Current(), names});
      }
    }
    keyed.emplace_back(detail::shape_key(it.Current()),
                       std::make_shared<Shape>(it.Current(), faces));
  }
  std::stable_sort(keyed.begin(), keyed.end(),
                   [](const auto& a, const auto& b) { return a.first < b.first; });
  std::vector<ShapePtr> result;
  for (auto& entry : keyed) {
    result.push_back(std::move(entry.second));
  }
  return result;
}

} // namespace mitcad::geometry
