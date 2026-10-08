// SPDX-License-Identifier: MIT
#pragma once

// Immutable B-rep results with persistent face names. Faces carry the names
// the creating operation gave them and that later operations carried through
// their history (see naming.hpp); edge and vertex names are derived from the
// faces that meet there when the shape is constructed.
//
// A Shape is immutable once made, and its own data may be read from several
// threads at once (the model computes on a worker thread, docs/
// architecture.md); what it remembers of itself (measured(), bounds()) is
// kept behind a lock. The TopoDS shape underneath shares sub-shapes with
// other results, and OCCT's algorithms set flags on the sub-shapes of their
// inputs: a thread does not read it while another computes with it.

#include <array>
#include <memory>
#include <mutex>
#include <optional>
#include <string>
#include <vector>

#include <NCollection_IndexedMap.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>
#include <TopoDS_Vertex.hxx>

#include "mitcad/geometry/input_check.hpp"
#include "mitcad/geometry/naming.hpp"

namespace mitcad::geometry {

using ShapeMap = NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher>;

// A value a const object works out once and keeps, which threads may read
// and set at the same time. A copy takes the value and a lock of its own.
template <class T>
class LazyValue {
public:
  LazyValue() = default;
  LazyValue(const LazyValue& other) : m_value(other.get()) {}
  LazyValue& operator=(const LazyValue& other) {
    if (this != &other) {
      std::optional<T> value = other.get();
      const std::lock_guard<std::mutex> lock(m_mutex);
      m_value = std::move(value);
    }
    return *this;
  }
  ~LazyValue() = default;

  std::optional<T> get() const {
    const std::lock_guard<std::mutex> lock(m_mutex);
    return m_value;
  }
  // Threads that work the value out at once all set the same value.
  void set(const T& value) const {
    const std::lock_guard<std::mutex> lock(m_mutex);
    m_value = value;
  }

private:
  mutable std::mutex m_mutex;
  mutable std::optional<T> m_value;
};

class Shape {
public:
  struct NamedFace {
    TopoDS_Shape face;
    NameList names;
  };

  // Faces of `shape` that are not listed have no name. Listed faces that are
  // not part of the shape are ignored.
  explicit Shape(TopoDS_Shape shape, const std::vector<NamedFace>& faces = {});
  Shape(const Shape&) = default;
  Shape(Shape&&) = default;
  Shape& operator=(const Shape&) = default;
  Shape& operator=(Shape&&) = default;
  ~Shape();

  // Reads of the B-rep are the input check's (input_check.hpp).
  const TopoDS_Shape& occt() const {
    if (detail::input_checks.load(std::memory_order_relaxed)) {
      detail::shape_read(*this);
    }
    return m_shape;
  }

  // Faces, edges and vertices in TopExp order, 0-based.
  int face_count() const { return m_faces.Extent(); }
  const TopoDS_Face& face(int i) const;
  // Sorted; empty for an unnamed face.
  const NameList& face_names(int i) const { return m_faceNames[static_cast<std::size_t>(i)]; }

  int edge_count() const { return m_edges.Extent(); }
  const TopoDS_Edge& edge(int i) const;
  // Empty when a face at the edge has no name.
  const std::string& edge_name(int i) const { return m_edgeNames[static_cast<std::size_t>(i)]; }
  // The faces at edge i, the one with the smaller primary name first (the
  // face a two-distance chamfer measures its first distance on); -1 when
  // missing. Both are the same face for a seam.
  std::array<int, 2> edge_faces(int i) const { return m_edgeFaces[static_cast<std::size_t>(i)]; }

  int vertex_count() const { return m_vertices.Extent(); }
  const TopoDS_Vertex& vertex(int i) const;
  const std::string& vertex_name(int i) const {
    return m_vertexNames[static_cast<std::size_t>(i)];
  }

  // Index of a sub-shape of this shape, or -1.
  int face_index(const TopoDS_Shape& face) const { return m_faces.FindIndex(face) - 1; }
  int edge_index(const TopoDS_Shape& edge) const { return m_edges.FindIndex(edge) - 1; }
  int vertex_index(const TopoDS_Shape& vertex) const { return m_vertices.FindIndex(vertex) - 1; }

  // Names of a face of this shape; empty when unnamed or not a face of it.
  const NameList& names_of_face(const TopoDS_Shape& face) const;
  // Derived names; null when unnamed or not part of this shape.
  std::optional<std::string> name_of_edge(const TopoDS_Shape& edge) const;
  std::optional<std::string> name_of_vertex(const TopoDS_Shape& vertex) const;

  // Sub-shapes a reference resolves to (see naming.hpp), in a deterministic
  // geometric order. An edge reference without "#k" resolves to every edge
  // between matching faces.
  std::vector<int> find_faces(const std::string& reference) const;
  std::vector<int> find_edges(const std::string& reference) const;
  std::vector<int> find_vertices(const std::string& reference) const;

  // Volume, area and centre {volume, area, x, y, z} once query.cpp's
  // mass_properties has measured them; the shape never changes, and the
  // accurate integration of curved faces is costly.
  std::optional<std::array<double, 5>> measured() const { return m_measured.get(); }
  void set_measured(const std::array<double, 5>& values) const { m_measured.set(values); }
  // The bounding box {xmin, ymin, zmin, xmax, ymax, zmax} once query.cpp's
  // bounding_box has computed it: the optimal box of curved faces is
  // costly, and extrusions through all ask for the bodies' boxes again
  // and again.
  std::optional<std::array<double, 6>> bounds() const { return m_bounds.get(); }
  void set_bounds(const std::array<double, 6>& values) const { m_bounds.set(values); }
  // What the bridge's boundary distances keep of the shape between calls
  // (its faces' projections, costly to set up, and asked for again and
  // again by the .f3d import); null until the first call. The kept object
  // has a lock of its own.
  std::shared_ptr<void> boundary_index() const { return m_boundaryIndex.get().value_or(nullptr); }
  void set_boundary_index(std::shared_ptr<void> index) const { m_boundaryIndex.set(std::move(index)); }

  // Whether OCCT's checker finds the shape valid, once something asked
  // (transforms check their input once instead of every copy they make).
  std::optional<bool> checked_valid() const { return m_valid.get(); }
  void set_checked_valid(bool valid) const { m_valid.set(valid); }

  // Whether the shape is a boolean's result with its coplanar faces and
  // collinear edges merged (OCCT's ShapeUpgrade_UnifySameDomain): what is
  // left unmerged in it stays so in the merge after a later boolean
  // (far_features.hpp). Not kept in stored results.
  bool unified() const { return m_unified; }
  void set_unified(bool unified) { m_unified = unified; }

  // What the operation that made the shape gave up to build it (a fillet
  // made 0.1 % smaller than asked); the model shows them as warnings.
  const std::vector<std::string>& notes() const { return m_notes; }
  void add_note(std::string note) { m_notes.push_back(std::move(note)); }

private:
  void derive_edge_names();
  void derive_vertex_names();
  bool edge_matches(int i, const std::string& a, const std::string& b) const;
  bool vertex_matches(int i, const NameList& faces) const;
  std::vector<int> sorted_edges(std::vector<int> edges) const;
  std::vector<int> sorted_vertices(std::vector<int> vertices) const;

  TopoDS_Shape m_shape;
  ShapeMap m_faces;
  ShapeMap m_edges;
  ShapeMap m_vertices;
  std::vector<NameList> m_faceNames;
  std::vector<std::array<int, 2>> m_edgeFaces;
  std::vector<std::string> m_edgeNames;
  std::vector<std::vector<int>> m_vertexFaces;
  std::vector<std::string> m_vertexNames;
  LazyValue<std::array<double, 5>> m_measured;
  LazyValue<std::array<double, 6>> m_bounds;
  LazyValue<std::shared_ptr<void>> m_boundaryIndex;
  LazyValue<bool> m_valid;
  bool m_unified = false;
  std::vector<std::string> m_notes;
  // Registered with the input check when made.
  bool m_checked = false;
};

using ShapePtr = std::shared_ptr<Shape>;

// The solids of a shape, each with its faces' names, in a deterministic
// geometric order.
std::vector<ShapePtr> solids(const Shape& shape);

} // namespace mitcad::geometry
