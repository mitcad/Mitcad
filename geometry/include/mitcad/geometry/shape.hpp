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
// other results. The library's operations read it on several threads at
// once (the .f3d import's workers, mitcad#95): they never change their
// inputs (input_check.hpp), and those whose OCCT algorithms write into
// their inputs' sub-shapes work on copies (history.hpp, InputCopy); OCCT's
// builders still set bookkeeping flags (free, modified) on the sub-shapes
// they put into a new shape, which nothing reads. Triangulating it (the
// view) writes into its faces: not while another thread computes with it.

#include <array>
#include <memory>
#include <mutex>
#include <optional>
#include <string>
#include <unordered_map>
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

  // Exact source-profile provenance for an edge of a newly made shape.
  // Separate from canonical two-face edge names: several names preserve
  // an ambiguous history instead of guessing which source survived.
  struct NamedOpenEdge {
    TopoDS_Shape edge;
    NameList names;
  };

  // What the names of the faces give the edges and vertices: each edge's
  // two faces and name, each vertex's faces and name, in TopExp order, as
  // a shape works them out when it is made (persist.cpp keeps them with a
  // stored shape, mitcad#841).
  struct DerivedNames {
    std::vector<std::array<int, 2>> edge_faces;
    std::vector<std::string> edge_names;
    std::vector<std::vector<int>> vertex_faces;
    std::vector<std::string> vertex_names;
    std::vector<NameList> open_edge_names;
  };

  // Faces of `shape` that are not listed have no name. Listed faces that are
  // not part of the shape are ignored.
  explicit Shape(TopoDS_Shape shape, const std::vector<NamedFace>& faces = {});
  // The same shape with the edge and vertex names it was given (`derived`,
  // what a Shape of the same B-rep and face names worked out) instead of
  // working them out again; when their counts do not match the shape's
  // edges and vertices they are worked out.
  Shape(TopoDS_Shape shape, const std::vector<NamedFace>& faces, DerivedNames derived);
  // The same with the shape's faces, edges and vertices mapped already
  // (TopExp::MapShapes of `shape`; the maps are taken, left empty) and the
  // faces' names by face index (`face_names[i]` the names of face(i)),
  // which spares mapping the shape again and looking each face up
  // (persist.cpp, mitcad#887); names of another count than the faces
  // name none.
  Shape(TopoDS_Shape shape, ShapeMap& faces, ShapeMap& edges, ShapeMap& vertices,
        std::vector<NameList> face_names, DerivedNames derived);
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
  const NameList& open_edge_names(int i) const {
    static const NameList empty;
    return m_openEdgeNames.empty() ? empty : m_openEdgeNames[static_cast<std::size_t>(i)];
  }
  bool has_open_edge_names() const { return !m_openEdgeNames.empty(); }
  std::vector<NamedOpenEdge> named_open_edges() const;
  // Construction metadata of a fresh output only; never called on an
  // operation's input. Unmapped edges remain without provenance.
  void set_open_edge_names(const std::vector<NamedOpenEdge>& names);
  // The faces at edge i, the one with the smaller primary name first (the
  // face a two-distance chamfer measures its first distance on); -1 when
  // missing. Both are the same face for a seam.
  std::array<int, 2> edge_faces(int i) const { return m_edgeFaces[static_cast<std::size_t>(i)]; }

  int vertex_count() const { return m_vertices.Extent(); }
  const TopoDS_Vertex& vertex(int i) const;
  const std::string& vertex_name(int i) const {
    return m_vertexNames[static_cast<std::size_t>(i)];
  }
  // The faces at vertex i, in the order the vertex's names list them.
  const std::vector<int>& vertex_faces(int i) const { return m_vertexFaces[static_cast<std::size_t>(i)]; }

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

  // Contact preparation belongs to this immutable geometry and location,
  // never to a tool/distance or an operation answer. Bounds and planes use the
  // geometry, independent of triangulation quality, keyed also by contact
  // tolerance (ordinary and near-contact joins).
  std::shared_ptr<const void> kept_contact_faces() const { return m_contactFaces.get().value_or(nullptr); }
  void keep_contact_faces(std::shared_ptr<const void> faces) const { m_contactFaces.set(std::move(faces)); }

  // Each face's surface kind and area (query.cpp's face_infos), once
  // measured: the .f3d import asks for the faces of the same large body
  // again and again (an extrusion's face lookups on a body of 5 000 faces
  // measured every face's area dozens of times); null until then.
  struct FaceMeasure {
    std::string surface;
    double area = 0.0;
  };
  using FaceMeasures = std::vector<FaceMeasure>;
  std::shared_ptr<const FaceMeasures> face_measures() const { return m_faceMeasures.get().value_or(nullptr); }
  void set_face_measures(std::shared_ptr<const FaceMeasures> measures) const {
    m_faceMeasures.set(std::move(measures));
  }

  // Each named edge's middle (query.hpp's edge_middles) and each edge's
  // curve (datum.hpp's edge_geometries), once worked out: the .f3d import
  // asks for the edges of the same large bodies before every rounding and
  // sketch (mitcad#119: bodies of 60 000 edges measured edge by edge each
  // time). Null until then; the types are those of query.hpp and
  // datum.hpp, which name this header.
  std::shared_ptr<const void> kept_edge_middles() const { return m_edgeMiddles.get().value_or(nullptr); }
  void keep_edge_middles(std::shared_ptr<const void> middles) const { m_edgeMiddles.set(std::move(middles)); }
  std::shared_ptr<const void> kept_edge_curves() const { return m_edgeCurves.get().value_or(nullptr); }
  void keep_edge_curves(std::shared_ptr<const void> curves) const { m_edgeCurves.set(std::move(curves)); }

  // Boxes {xmin, ymin, zmin, xmax, ymax, zmax} that hold each face, in face
  // order (query.hpp's face_boxes), once worked out; null until then.
  using FaceBoxes = std::vector<std::array<double, 6>>;
  std::shared_ptr<const FaceBoxes> kept_face_boxes() const { return m_faceBoxes.get().value_or(nullptr); }
  void keep_face_boxes(std::shared_ptr<const FaceBoxes> boxes) const { m_faceBoxes.set(std::move(boxes)); }

  // Whether OCCT's checker finds the shape valid, once something asked
  // (transforms check their input once instead of every copy they make).
  std::optional<bool> checked_valid() const { return m_valid.get(); }
  void set_checked_valid(bool valid) const { m_valid.set(valid); }

  // Whether a meshing of every face for display ran to its end on another
  // thread (the view's meshes, made at the end of a job, mitcad#398). A
  // face it left without a triangulation is one OCCT's mesher fails on;
  // the view then draws the faces that have one and never meshes the shape
  // again itself: meshing such a face again can take without end, and the
  // view would do it on the UI thread (mitcad#589).
  bool display_meshed() const { return m_displayMeshed.get().value_or(false); }
  void set_display_meshed() const { m_displayMeshed.set(true); }

  // The interactive preview tier of the view's meshes (mitcad#635): a
  // preview's body is drawn through a stand-in, an independent_copy of it
  // meshed for interactive previews on the model's thread, and the body's
  // own faces stay without triangulations until the job that meshes it at
  // the view's final quality, so the two qualities never share a face and
  // nothing the view reads is meshed again on another thread. The body
  // holds its stand-in weakly (whoever draws it keeps it), the stand-in
  // its body weakly too.
  std::shared_ptr<Shape> display_stand_in() const {
    return m_displayStandIn.get().value_or(std::weak_ptr<Shape>()).lock();
  }
  void set_display_stand_in(const std::shared_ptr<Shape>& stand_in) const { m_displayStandIn.set(stand_in); }
  // On a stand-in: the body it is drawn for, and that its meshes are the
  // interactive tier's.
  bool display_interactive() const { return m_standInFor.get().has_value(); }
  std::shared_ptr<Shape> stand_in_for() const {
    return m_standInFor.get().value_or(std::weak_ptr<Shape>()).lock();
  }
  void set_stand_in_for(const std::shared_ptr<Shape>& shape) const { m_standInFor.set(shape); }

  // Takes what was worked out of `other`, a Shape of the same B-rep (the
  // same TopoDS_Shape, with other face names), that does not depend on
  // face names: the measures, the box, the face measures and boxes, the
  // edges' curves, the boundary index, the checker's verdict (mitcad#244)
  // and whether it was meshed for display. Edge middles and the face index carry names; they are
  // worked out anew.
  void keep_measures_of(const Shape& other) const;

  // Whether the shape is a boolean's result with its coplanar faces and
  // collinear edges merged (OCCT's ShapeUpgrade_UnifySameDomain): what is
  // left unmerged in it stays so in the merge after a later boolean
  // (far_features.hpp). Not kept in stored results.
  bool unified() const { return m_unified; }
  void set_unified(bool unified) { m_unified = unified; }

  // What the operation that made the shape gave up to build it (a fillet
  // built as rings); the model shows them as warnings.
  const std::vector<std::string>& notes() const { return m_notes; }
  void add_note(std::string note) { m_notes.push_back(std::move(note)); }
  // What is worth knowing of how it was built but needs no action (a
  // fillet made 0.1 % smaller than asked, mitcad#1106); the model shows
  // them as information, in the feature's details.
  const std::vector<std::string>& infos() const { return m_infos; }
  void add_info(std::string info) { m_infos.push_back(std::move(info)); }
  // The other shape's notes and infos added to this one's.
  void copy_notes(const Shape& other) {
    m_notes.insert(m_notes.end(), other.m_notes.begin(), other.m_notes.end());
    m_infos.insert(m_infos.end(), other.m_infos.begin(), other.m_infos.end());
  }

private:
  void map_faces(const std::vector<NamedFace>& faces);
  void sort_face_names();
  void register_check();
  bool take_derived(DerivedNames& derived);
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
  std::vector<NameList> m_openEdgeNames;
  std::vector<std::vector<int>> m_vertexFaces;
  std::vector<std::string> m_vertexNames;
  LazyValue<std::array<double, 5>> m_measured;
  LazyValue<std::array<double, 6>> m_bounds;
  LazyValue<std::shared_ptr<void>> m_boundaryIndex;
  LazyValue<std::shared_ptr<const void>> m_contactFaces;
  LazyValue<std::shared_ptr<const FaceMeasures>> m_faceMeasures;
  LazyValue<std::shared_ptr<const void>> m_edgeMiddles;
  LazyValue<std::shared_ptr<const void>> m_edgeCurves;
  LazyValue<std::shared_ptr<const FaceBoxes>> m_faceBoxes;
  LazyValue<bool> m_valid;
  LazyValue<bool> m_displayMeshed;
  LazyValue<std::weak_ptr<Shape>> m_displayStandIn;
  LazyValue<std::weak_ptr<Shape>> m_standInFor;
  // A large shape's faces by each reference that matches one of their
  // names (naming.hpp: the name, or the name before a "#k"), made the
  // first time a face is looked for (mitcad#181: finding faces one by one
  // on a body of 23 000 faces scanned every face's names each time).
  using FaceIndex = std::unordered_map<std::string, std::vector<int>>;
  LazyValue<std::shared_ptr<const FaceIndex>> m_faceIndex;
  // A large shape's edges of each face (from m_edgeFaces), made the first
  // time an edge is looked for: find_edges goes through the edges of the
  // faces its reference names instead of every edge (mitcad#878: a sketch
  // projecting 5 000 edges of a large body took 3 s, each edge's name
  // compared with every edge's faces).
  using FaceEdges = std::vector<std::vector<int>>;
  LazyValue<std::shared_ptr<const FaceEdges>> m_faceEdges;
  // The faces whose names match a reference, in index order.
  std::vector<int> matching_faces(const std::string& reference) const;
  bool m_unified = false;
  std::vector<std::string> m_notes;
  std::vector<std::string> m_infos;
  // Registered with the input check when made.
  bool m_checked = false;
};

using ShapePtr = std::shared_ptr<Shape>;

// The solids of a shape, each with its faces' names, in a deterministic
// geometric order.
std::vector<ShapePtr> solids(const Shape& shape);

// A copy of the shape that shares no sub-shape with it: its faces, edges
// and vertices in new sub-shapes, in the same order and with the same
// names, on the same curves and surfaces, without meshes (the copy the
// operations whose OCCT algorithms write into their inputs work on). For
// work on another thread whose results are meshed there while the shape's
// own sub-shapes may be read or meshed elsewhere (Section Analysis' clips
// off the UI thread, mitcad#438).
ShapePtr independent_copy(const Shape& shape);

} // namespace mitcad::geometry
