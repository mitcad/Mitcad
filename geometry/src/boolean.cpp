// SPDX-License-Identifier: MIT
#include "mitcad/geometry/boolean.hpp"

#include <algorithm>
#include <chrono>
#include <cmath>
#include <cstddef>
#include <cstdio>
#include <cstdlib>
#include <exception>
#include <memory>
#include <optional>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include <BOPAlgo_Alerts.hxx>
#include <BRepAlgoAPI_Common.hxx>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepAlgoAPI_Fuse.hxx>
#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeVertex.hxx>
#include <BRepCheck_Analyzer.hxx>
#include <BRepClass_FaceClassifier.hxx>
#include <BRepClass3d_SolidClassifier.hxx>
#include <BRepExtrema_DistShapeShape.hxx>
#include <BRep_Builder.hxx>
#include <BRep_Tool.hxx>
#include <Bnd_Box.hxx>
#include <GCPnts_AbscissaPoint.hxx>
#include <ElSLib.hxx>
#include <Geom2d_Curve.hxx>
#include <NCollection_HSequence.hxx>
#include <NCollection_List.hxx>
#include <Precision.hxx>
#include <ShapeAnalysis_FreeBounds.hxx>
#include <ShapeBuild_ReShape.hxx>
#include <ShapeFix_Face.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Wire.hxx>
#include <gp_Pnt.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_XYZ.hxx>

#include "far_features.hpp"
#include "history.hpp"
#include "mitcad/analysis/common.hpp"
#include "mitcad/geometry/contact.hpp"
#include "mitcad/geometry/query.hpp"
#include "util.hpp"

namespace mitcad::geometry {
namespace {

// Shapes closer than this touch.
constexpr double kContact = 1.0e-6;
// A tool this close to a body (and no closer than kContact) still joins it:
// the file's geometry carries a few nanometres of noise.
constexpr double kNearContact = 1.0e-5;

// The distance within which the touch measuring below (touches) counts shapes as
// touching: kContact, or kNearContact within a ContactScope. Per thread: the
// import runs booleans of several definitions at once.
thread_local double t_contact = kContact;

class ContactScope {
public:
  explicit ContactScope(double contact) : m_before(t_contact) { t_contact = contact; }
  ~ContactScope() { t_contact = m_before; }
  ContactScope(const ContactScope&) = delete;
  ContactScope& operator=(const ContactScope&) = delete;

private:
  double m_before;
};

// Whether MITCAD_BOOLEAN_TRACE asks for a line per operation on standard
// error: the operands' face counts and the time of each phase (the copy,
// taking far features out, OCCT's boolean, the face merge, naming, the
// checker, putting the features back). It shows where a design's booleans
// spend their time (mitcad#247) without a profiler.
bool tracing() {
  static const bool on = [] {
#ifdef _MSC_VER
    char* value = nullptr;
    std::size_t length = 0;
    if (_dupenv_s(&value, &length, "MITCAD_BOOLEAN_TRACE") != 0 || value == nullptr) {
      return false;
    }
    std::free(value);
    return true;
#else
    return std::getenv("MITCAD_BOOLEAN_TRACE") != nullptr;
#endif
  }();
  return on;
}

// The phases of one operation, traced when it ends (tracing()).
class PhaseTrace {
public:
  explicit PhaseTrace(std::string what)
      : m_what(std::move(what)), m_start(std::chrono::steady_clock::now()), m_last(m_start) {}
  void mark(const char* phase) {
    if (!tracing()) {
      return;
    }
    const auto now = std::chrono::steady_clock::now();
    m_line += " " + std::string(phase) + "=" +
              std::to_string(std::chrono::duration<double>(now - m_last).count());
    m_last = now;
  }
  void note(const std::string& text) {
    if (tracing()) {
      m_line += " " + text;
    }
  }
  ~PhaseTrace() {
    if (tracing()) {
      mark("rest");
      const double total = std::chrono::duration<double>(std::chrono::steady_clock::now() - m_start).count();
      std::fprintf(stderr, "boolean-trace: %s total=%f%s\n", m_what.c_str(), total, m_line.c_str());
    }
  }
  PhaseTrace(const PhaseTrace&) = delete;
  PhaseTrace& operator=(const PhaseTrace&) = delete;

private:
  std::string m_what;
  std::string m_line;
  std::chrono::steady_clock::time_point m_start;
  std::chrono::steady_clock::time_point m_last;
};

const char* operation_name(BooleanOp op) {
  switch (op) {
  case BooleanOp::Join:
    return "join";
  case BooleanOp::Cut:
    return "cut";
  case BooleanOp::Intersect:
    return "intersect";
  }
  return "boolean";
}

Bnd_Box box_of(const Shape& shape) {
  Bnd_Box box;
  BRepBndLib::Add(shape.occt(), box);
  return box;
}

// Above this edge count, a planar trim witness is cheaper than measuring
// every face/face distance near the tool (notably caps with many holes).
constexpr int kPlanarEdges = 64;

// A shape's faces with their boxes (enlarged by kContact).
struct FaceBoxes {
  double contactTolerance = t_contact;
  std::vector<std::pair<TopoDS_Shape, Bnd_Box>> faces;
  // The edges of each face (as its wires use them).
  std::vector<int> edges;
  std::vector<std::pair<std::size_t, gp_Pln>> planes;

  explicit FaceBoxes(const TopoDS_Shape& shape, bool use_triangulation = true) {
    for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
      Bnd_Box box;
      BRepBndLib::Add(it.Current(), box, use_triangulation);
      box.Enlarge(t_contact);
      faces.emplace_back(it.Current(), box);
      int count = 0;
      for (TopExp_Explorer edge(it.Current(), TopAbs_EDGE); edge.More(); edge.Next()) {
        ++count;
      }
      edges.push_back(count);
      if (!use_triangulation && count >= kPlanarEdges) {
        const BRepAdaptor_Surface surface(TopoDS::Face(it.Current()), false);
        if (surface.GetType() == GeomAbs_Plane) {
          planes.emplace_back(faces.size() - 1, surface.Plane());
        }
      }
    }
  }

  // The edges of the marked faces.
  int edges_of(const std::vector<bool>& marks) const {
    int count = 0;
    for (std::size_t i = 0; i < faces.size(); ++i) {
      count += marks[i] ? edges[i] : 0;
    }
    return count;
  }

  // The faces (of those marked in `among`, else of all) that come within
  // kContact of a box.
  std::vector<bool> near(const Bnd_Box& box, const std::vector<bool>& among = {}) const {
    std::vector<bool> marks(faces.size(), false);
    for (std::size_t i = 0; i < faces.size(); ++i) {
      marks[i] = (among.empty() || among[i]) && !faces[i].second.IsOut(box);
    }
    return marks;
  }

  // The marked faces as a compound.
  TopoDS_Shape compound(const std::vector<bool>& marks) const {
    BRep_Builder builder;
    TopoDS_Compound found;
    builder.MakeCompound(found);
    for (std::size_t i = 0; i < faces.size(); ++i) {
      if (marks[i]) {
        builder.Add(found, faces[i].first);
      }
    }
    return found;
  }
};

std::shared_ptr<const FaceBoxes> contact_faces(const Shape& shape) {
  auto faces = std::static_pointer_cast<const FaceBoxes>(shape.kept_contact_faces());
  if (!faces || faces->contactTolerance != t_contact) {
    // Geometry-only bounds stay conservative when preview/display mesh
    // quality changes. No mutable triangulation is retained by this index.
    faces = std::make_shared<const FaceBoxes>(shape.occt(), false);
    shape.keep_contact_faces(faces);
  }
  return faces;
}

bool any_marked(const std::vector<bool>& marks) {
  return std::find(marks.begin(), marks.end(), true) != marks.end();
}

// Whether two shapes come within `limit` of each other (true when that
// cannot be measured). They are measured together with two vertices a
// little more than `limit` apart, far from both: OCCT's distance measures
// only the pairs of vertices, edges and faces whose boxes are nearer than
// the nearest pair found so far, and its first pairs are vertices, so the
// edges and faces whose boxes are farther apart than `limit` are left out
// (they were measured curve to curve while the shapes' vertices were far
// apart, seconds for the faces of a large body near a tool).
bool within(const TopoDS_Shape& a, const TopoDS_Shape& b, double limit) {
  Bnd_Box box;
  BRepBndLib::Add(a, box);
  BRepBndLib::Add(b, box);
  if (box.IsVoid()) {
    return true;
  }
  const gp_Pnt high = box.CornerMax();
  const double away = box.CornerMin().Distance(high) + limit + 1.0;
  const gp_Pnt start(high.X() + away, high.Y() + away, high.Z() + away);
  const gp_Pnt end(start.X() + limit + t_contact, start.Y(), start.Z());
  BRep_Builder builder;
  TopoDS_Compound with_a;
  builder.MakeCompound(with_a);
  builder.Add(with_a, a);
  builder.Add(with_a, BRepBuilderAPI_MakeVertex(start).Vertex());
  TopoDS_Compound with_b;
  builder.MakeCompound(with_b);
  builder.Add(with_b, b);
  builder.Add(with_b, BRepBuilderAPI_MakeVertex(end).Vertex());
  detail::Interrupt interrupt;
  BRepExtrema_DistShapeShape measure(with_a, with_b, Extrema_ExtFlag_MINMAX, Extrema_ExtAlgo_Grad,
                                     interrupt.range());
  throw_if_cancelled();
  return !measure.IsDone() || measure.Value() <= limit;
}

// Whether something inside `box` may come within kContact of `faces`: not
// when the faces are farther from the box's centre than half its diagonal
// and kContact. The distance of a point is quick to measure, that of two
// sets of faces is not (curve-to-curve distances between all their
// edges): measuring every copy of a 90-copy pattern against the faces
// near it took over a minute, though most were millimetres away.
bool may_reach(const Bnd_Box& box, const TopoDS_Shape& faces) {
  const gp_Pnt low = box.CornerMin();
  const gp_Pnt high = box.CornerMax();
  const gp_Pnt centre((low.XYZ() + high.XYZ()) / 2.0);
  return within(BRepBuilderAPI_MakeVertex(centre).Vertex(), faces,
                low.Distance(high) / 2.0 + t_contact);
}

// The faces of `from` (of those marked in `among`) that may come within
// kContact of faces of `to` (of those marked in `to_among`): faces of `to`
// are near their box, and may_reach does not rule them out.
std::vector<bool> reaching(const FaceBoxes& from, const std::vector<bool>& among,
                           const FaceBoxes& to, const std::vector<bool>& to_among) {
  std::vector<bool> kept(from.faces.size(), false);
  for (std::size_t i = 0; i < from.faces.size(); ++i) {
    if (!among[i]) {
      continue;
    }
    const Bnd_Box& box = from.faces[i].second;
    const std::vector<bool> near = to.near(box, to_among);
    kept[i] = any_marked(near) && may_reach(box, to.compound(near));
  }
  return kept;
}

// Whether faces of `part` come within kContact of faces of `a`. Only the
// faces of each that may reach the other's are measured (reaching: the
// faces of a large body are mostly small, those of a tool often not).
bool faces_touch(const FaceBoxes& a, const TopoDS_Shape& part, const Bnd_Box& part_box,
                 const Bnd_Box& box_a) {
  const std::vector<bool> near_a = a.near(part_box);
  if (!any_marked(near_a) || !may_reach(part_box, a.compound(near_a))) {
    return false;
  }
  const FaceBoxes faces(part);
  const std::vector<bool> kept = reaching(faces, faces.near(box_a), a, near_a);
  if (!any_marked(kept)) {
    return false;
  }
  const std::vector<bool> kept_a = reaching(a, near_a, faces, kept);
  if (!any_marked(kept_a)) {
    return false;
  }
  return within(a.compound(kept_a), faces.compound(kept), t_contact);
}

// A point of a shape (its first vertex), if it has one.
std::optional<gp_Pnt> some_point(const TopoDS_Shape& shape) {
  TopExp_Explorer vertex(shape, TopAbs_VERTEX);
  if (!vertex.More()) {
    return std::nullopt;
  }
  return BRep_Tool::Pnt(TopoDS::Vertex(vertex.Current()));
}

// Classifies points against the solids of a shape (each classifier set up
// once: on a large body that is the costly part).
class Inside {
public:
  explicit Inside(const TopoDS_Shape& shape) : m_shape(shape) {}

  bool operator()(const gp_Pnt& point) { return state(point) == TopAbs_IN; }

  // IN when the point is inside a solid, else ON when it is on the boundary
  // of one (within kContact, or within the tolerance of a vertex or edge
  // there), else OUT.
  TopAbs_State state(const gp_Pnt& point) {
    if (m_classifiers.empty()) {
      for (TopExp_Explorer solid(m_shape, TopAbs_SOLID); solid.More(); solid.Next()) {
        m_classifiers.push_back(std::make_unique<BRepClass3d_SolidClassifier>(solid.Current()));
      }
    }
    TopAbs_State found = TopAbs_OUT;
    for (const auto& classifier : m_classifiers) {
      classifier->Perform(point, t_contact);
      if (classifier->State() == TopAbs_IN) {
        return TopAbs_IN;
      }
      if (classifier->State() == TopAbs_ON) {
        found = TopAbs_ON;
      }
    }
    return found;
  }

private:
  TopoDS_Shape m_shape;
  std::vector<std::unique_ptr<BRepClass3d_SolidClassifier>> m_classifiers;
};

// Vertices of a tool part tried by vertex_touches.
constexpr int kProbedVertices = 32;
// Edges of the faces near a tool part from which its vertices are tried
// first (vertex_touches): below it measuring faces is as quick.
constexpr int kManyEdges = 1000;
// A planar face with many hole wires makes a face/face distance costly.
// A few tool vertices can instead prove contact by a plane distance and
// OCCT's trim classification; a miss proves nothing and keeps the fallback.
constexpr int kPlanarVertices = 4;

bool planar_vertex_touches(const TopoDS_Shape& part, const Bnd_Box& box_a, const FaceBoxes& faces_a) {
  if (faces_a.planes.empty()) { return false; }
  ShapeMap seen;
  int probed = 0;
  for (TopExp_Explorer vertex(part, TopAbs_VERTEX); vertex.More() && probed < kPlanarVertices; vertex.Next()) {
    const int before = seen.Extent();
    if (seen.Add(vertex.Current()) <= before) { continue; }
    ++probed;
    const gp_Pnt point = BRep_Tool::Pnt(TopoDS::Vertex(vertex.Current()));
    if (box_a.IsOut(point)) { continue; }
    for (const auto& [index, plane] : faces_a.planes) {
      const auto& candidate = faces_a.faces[index];
      if (candidate.second.IsOut(point)) { continue; }
      const TopoDS_Face& face = TopoDS::Face(candidate.first);
      if (plane.Distance(point) > t_contact) { continue; }
      double u = 0, v = 0;
      ElSLib::Parameters(plane, point, u, v);
      const BRepClass_FaceClassifier classification(face, gp_Pnt2d(u, v), Precision::PConfusion());
      // Strictly inside the trim: a point near an edge/vertex with a larger
      // stored tolerance is left to the existing exact distance check.
      if (classification.State() == TopAbs_IN) { return true; }
    }
  }
  return false;
}

// Whether a vertex of `part` shows that it touches the shape `a` (its
// classifier `inside_a`, its faces `faces_a`): a vertex inside a solid of
// it, or on its boundary within kContact (measured: the classifier also
// says ON within the tolerances of the shape's vertices and edges). Either
// means the distance faces_touch measures is within kContact, or the part
// lies inside `a`, so touches gives true without measuring faces: on a
// body with thousands of faces (a plate with a large pattern of holes),
// whose large faces come near every face of a tool, measuring took
// minutes. False says nothing. Only the first kProbedVertices vertices in
// `a`'s box are tried.
bool vertex_touches(const TopoDS_Shape& part, const Bnd_Box& box_a, Inside& inside_a,
                    const FaceBoxes& faces_a) {
  ShapeMap seen;
  int probed = 0;
  for (TopExp_Explorer it(part, TopAbs_VERTEX); it.More() && probed < kProbedVertices; it.Next()) {
    const int before = seen.Extent();
    if (seen.Add(it.Current()) <= before) {
      continue;
    }
    const gp_Pnt point = BRep_Tool::Pnt(TopoDS::Vertex(it.Current()));
    if (box_a.IsOut(point)) {
      continue;
    }
    ++probed;
    const TopAbs_State state = inside_a.state(point);
    if (state == TopAbs_IN) {
      return true;
    }
    if (state == TopAbs_ON) {
      Bnd_Box around;
      around.Add(point);
      around.Enlarge(t_contact);
      const std::vector<bool> near = faces_a.near(around);
      if (any_marked(near) && within(it.Current(), faces_a.compound(near), t_contact)) {
        return true;
      }
    }
  }
  return false;
}

// Whether the shapes touch or overlap: a solid inside the other counts.
// Each solid of `b` (a tool may be many copies) is measured against only
// the faces of `a` near it (faces_touch): the distance of a large body to
// a pattern's tool as a whole took over a minute. When those faces have
// kManyEdges edges or more, the solid's vertices are tried first
// (vertex_touches).
bool touches(const Shape& a, const Shape& b) {
  Bnd_Box box_a = box_of(a);
  box_a.Enlarge(t_contact);
  std::shared_ptr<const FaceBoxes> faces_a;
  Inside inside_a(a.occt());
  std::vector<TopoDS_Shape> parts;
  for (TopExp_Explorer solid(b.occt(), TopAbs_SOLID); solid.More(); solid.Next()) {
    parts.push_back(solid.Current());
  }
  if (parts.empty()) {
    parts.push_back(b.occt());
  }
  for (const TopoDS_Shape& part : parts) {
    Bnd_Box box;
    BRepBndLib::Add(part, box);
    box.Enlarge(t_contact);
    if (box.IsOut(box_a)) {
      continue;
    }
    if (!faces_a) {
      faces_a = contact_faces(a);
    }
    const bool crowded = faces_a->edges_of(faces_a->near(box)) >= kManyEdges;
    if (planar_vertex_touches(part, box_a, *faces_a) ||
        (crowded && vertex_touches(part, box_a, inside_a, *faces_a)) ||
        faces_touch(*faces_a, part, box, box_a)) {
      return true;
    }
    // Apart at their boundaries: one inside the other, or apart.
    const std::optional<gp_Pnt> in_part = some_point(part);
    if (in_part && !box_a.IsOut(*in_part) && inside_a(*in_part)) {
      return true;
    }
    const std::optional<gp_Pnt> in_a = some_point(a.occt());
    if (in_a && !box.IsOut(*in_a) && Inside(part)(*in_a)) {
      return true;
    }
  }
  return false;
}

// Whether the one piece a cut or an intersection left of a target is the
// target changed: their volumes differ by more than 1e-9 relative. The
// difference is first measured on the faces the two do not share, the
// piece's and the target's reversed, in one integral (together they bound
// what changed): on a large body most faces stay the target's, and
// measuring both bodies whole took a fifth of the time of the features
// that cut them. `seen` is the copy of the target the operation worked on.
bool volume_changed(const Shape& target, const TopoDS_Shape& seen, const Shape& piece) {
  const auto whole = [&] {
    const double before = volume(target);
    return std::abs(volume(piece) - before) > 1.0e-9 * std::max(1.0, before);
  };
  if (target.measured() && piece.measured()) {
    return whole();
  }
  ShapeMap own;
  std::vector<TopoDS_Shape> faces;
  for (TopExp_Explorer it(seen, TopAbs_FACE); it.More(); it.Next()) {
    if (own.Add(it.Current()) <= static_cast<int>(faces.size())) {
      return whole(); // a face used twice (by two shells)
    }
    faces.push_back(it.Current());
  }
  std::vector<bool> kept(faces.size(), false);
  BRep_Builder builder;
  TopoDS_Compound change;
  builder.MakeCompound(change);
  for (TopExp_Explorer it(piece.occt(), TopAbs_FACE); it.More(); it.Next()) {
    const int index = own.FindIndex(it.Current());
    const auto at = static_cast<std::size_t>(index - 1);
    if (index > 0 && !kept[at] && faces[at].Orientation() == it.Current().Orientation()) {
      kept[at] = true;
    } else {
      builder.Add(change, it.Current());
    }
  }
  for (std::size_t i = 0; i < faces.size(); ++i) {
    if (!kept[i]) {
      builder.Add(change, faces[i].Reversed());
    }
  }
  const double difference = std::abs(analysis::volume_properties(change).Mass());
  // The limit is 1e-9 of the larger of 1 and the target's volume, which
  // its box bounds; in between, both volumes decide.
  if (difference <= 1.0e-9) {
    return false;
  }
  const Bnd_Box box = box_of(target);
  if (box.IsVoid()) {
    return whole();
  }
  double xmin = 0, ymin = 0, zmin = 0, xmax = 0, ymax = 0, zmax = 0;
  box.Get(xmin, ymin, zmin, xmax, ymax, zmax);
  const double bound = (xmax - xmin) * (ymax - ymin) * (zmax - zmin);
  if (difference > 1.0e-9 * std::max(1.0, bound)) {
    return true;
  }
  return whole();
}

// Whether a piece of a join holds less volume than a target it merged
// (`seen`, the copy of the target the operation worked on): a union never
// does, so such a result is wrong (mitcad#964: OCCT's exact union of a
// pad's tool whose cylindrical face lies on a cylindrical face of the
// body that the body's earlier face merge made one face, partly beside
// the tool, came out with the tool's overlap with the body taken away).
// The difference is measured on the faces the two do not share, as
// volume_changed does; the limit, 1e-6 of the larger of 1 and the
// target's box, stays above the integration's noise.
bool loses_volume(const Shape& target, const TopoDS_Shape& seen, const Shape& piece) {
  ShapeMap own;
  std::vector<TopoDS_Shape> faces;
  for (TopExp_Explorer it(seen, TopAbs_FACE); it.More(); it.Next()) {
    if (own.Add(it.Current()) <= static_cast<int>(faces.size())) {
      own.Clear();
      faces.clear();
      break; // a face used twice (by two shells): both measured whole
    }
    faces.push_back(it.Current());
  }
  std::vector<bool> kept(faces.size(), false);
  BRep_Builder builder;
  TopoDS_Compound change;
  builder.MakeCompound(change);
  for (TopExp_Explorer it(piece.occt(), TopAbs_FACE); it.More(); it.Next()) {
    const int index = faces.empty() ? 0 : own.FindIndex(it.Current());
    const auto at = static_cast<std::size_t>(index - 1);
    if (index > 0 && !kept[at] && faces[at].Orientation() == it.Current().Orientation()) {
      kept[at] = true;
    } else {
      builder.Add(change, it.Current());
    }
  }
  if (faces.empty()) {
    for (TopExp_Explorer it(seen, TopAbs_FACE); it.More(); it.Next()) {
      builder.Add(change, it.Current().Reversed());
    }
  }
  for (std::size_t i = 0; i < faces.size(); ++i) {
    if (!kept[i]) {
      builder.Add(change, faces[i].Reversed());
    }
  }
  const double added = analysis::volume_properties(change).Mass();
  const Bnd_Box box = box_of(target);
  double bound = 1.0;
  if (!box.IsVoid()) {
    double xmin = 0, ymin = 0, zmin = 0, xmax = 0, ymax = 0, zmax = 0;
    box.Get(xmin, ymin, zmin, xmax, ymax, zmax);
    bound = std::max(1.0, (xmax - xmin) * (ymax - ymin) * (zmax - zmin));
  }
  return added < -1.0e-6 * bound;
}

std::unique_ptr<BRepAlgoAPI_BooleanOperation> make_operation(BooleanOp op) {
  switch (op) {
  case BooleanOp::Join:
    return std::make_unique<BRepAlgoAPI_Fuse>();
  case BooleanOp::Cut:
    return std::make_unique<BRepAlgoAPI_Cut>();
  case BooleanOp::Intersect:
    return std::make_unique<BRepAlgoAPI_Common>();
  }
  throw std::invalid_argument("unknown boolean operation");
}

// Runs the operation of the tagged targets with the tool and names the
// result. It works on a copy of them (InputCopy, all copied together): the
// inputs are cached results, the result shares the sub-shapes the boolean
// kept with them, and the face merge after it (SimplifyResult, OCCT's
// ShapeUpgrade_UnifySameDomain, also in the safe input mode the
// non-destructive boolean asks for) gives the edges of the faces it merges
// p-curves on the merged face's surface in place (T0e, input_check.hpp).
// `seen` (if given) gets the copies of the targets, whose faces the result
// shares where it kept them.
detail::FaceNamer run_operation(BooleanOp op, const std::vector<std::pair<const Shape*, int>>& targets,
                                const Shape& tool, int tool_tag, bool simplify = true,
                                std::vector<TopoDS_Shape>* seen = nullptr, bool reduce = true,
                                bool* split = nullptr, double fuzzy = 0.0, bool* lost = nullptr) {
  PhaseTrace trace(std::string(operation_name(op)));
  if (tracing()) {
    std::string faces;
    for (const auto& target : targets) {
      faces += std::to_string(target.first->face_count()) + ",";
    }
    trace.note("targets=" + faces + " tool=" + std::to_string(tool.face_count()) +
               " simplify=" + std::to_string(simplify) + " reduce=" + std::to_string(reduce));
  }
  std::vector<TopoDS_Shape> inputs;
  for (const auto& target : targets) {
    inputs.push_back(target.first->occt());
  }
  inputs.push_back(tool.occt());
  const detail::InputCopy copy(inputs);
  trace.mark("copy");
  // A single target's holes away from the tool sit out the operation
  // (far_features.hpp): on a perforated body it then works on the faces
  // near the tool with only their nearby holes.
  std::optional<detail::FarFeatures> far;
  if (reduce && targets.size() == 1 && op != BooleanOp::Intersect) {
    far = detail::FarFeatures::split(copy.shape(0), box_of(tool), targets.front().first->unified());
  }
  trace.mark("far");
  if (far) {
    trace.note("holes=" + std::to_string(far->holes()));
  }
  const auto operation = make_operation(op);
  NCollection_List<TopoDS_Shape> arguments;
  for (std::size_t i = 0; i < targets.size(); ++i) {
    arguments.Append(far ? far->reduced() : copy.shape(i));
  }
  NCollection_List<TopoDS_Shape> tools;
  tools.Append(copy.shape(targets.size()));
  operation->SetArguments(arguments);
  operation->SetTools(tools);
  if (fuzzy > 0.0) {
    operation->SetFuzzyValue(fuzzy);
  }
  // As before T0e: OCCT copies the sub-shapes whose tolerances it widens.
  operation->SetNonDestructive(true);
  // The face and edge pairs are intersected on all cores: booleans were
  // most of the import's time on large designs, on one core. (Oriented
  // boxes, SetUseOBB, changed a corpus result: a hole cut left an extra
  // edge, and the hole no longer matched the stored history.)
  operation->SetRunParallel(true);
  // The solids are not inverted (bodies are oriented, the stored ones when
  // they are built): OCCT's check of each one builds a point classifier of
  // all its faces, on one thread (mitcad#181: on bodies of 3 000–5 000 faces
  // most of a pattern's cut, 30 s of it).
  operation->SetCheckInverted(false);
  detail::build(*operation);
  trace.mark("bop");
  if (!operation->IsDone() || operation->HasErrors()) {
    throw std::runtime_error("the boolean operation failed");
  }
  // Faces the solids of the result could not be built with (a face split
  // wrongly where curves meet tangentially, edges a few micrometres apart
  // that were not taken as one): the result lacks them. Or faces the
  // result has inside its solids that its operands did not have (pieces
  // of a tool put inside the body, where the pieces of the body they bound
  // were not taken out): mitcad#235.
  if (lost != nullptr) {
    const auto internal_faces = [](const TopoDS_Shape& shape) {
      int count = 0;
      for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
        count += it.Current().Orientation() == TopAbs_INTERNAL ? 1 : 0;
      }
      return count;
    };
    int before = 0;
    for (const TopoDS_Shape& input : inputs) {
      before += internal_faces(input);
    }
    // Faces of the result outside its solids: a join that left the tool's
    // faces loose beside the target, which it gives back as it was
    // (mitcad#235).
    const auto loose_faces = [](const TopoDS_Shape& shape) {
      ShapeMap in_solids;
      for (TopExp_Explorer it(shape, TopAbs_SOLID); it.More(); it.Next()) {
        TopExp::MapShapes(it.Current(), TopAbs_FACE, in_solids);
      }
      for (TopExp_Explorer it(shape, TopAbs_FACE); it.More(); it.Next()) {
        if (!in_solids.Contains(it.Current())) {
          return true;
        }
      }
      return false;
    };
    *lost = operation->HasWarning(STANDARD_TYPE(BOPAlgo_AlertSolidBuilderUnusedFaces)) ||
            internal_faces(operation->Shape()) > before || loose_faces(operation->Shape());
    if (*lost) {
      trace.note("unused or internal faces");
    }
  }
  // Merge the coplanar faces and collinear edges the operation leaves behind.
  // The history then includes the merge.
  if (simplify) {
    operation->SimplifyResult();
  }
  trace.mark("unify");
  detail::FaceNamer namer(operation->Shape());
  for (const auto& [target, tag] : targets) {
    if (far) {
      namer.carry(*operation, *target, [&](const TopoDS_Shape& face) { return far->stand_in(copy.of(face)); },
                  tag);
    } else {
      namer.carry(*operation, *target, copy, tag);
    }
  }
  namer.carry(*operation, tool, copy, tool_tag);
  if (!far) {
    namer.finish();
  }
  trace.mark("names");
  // A cut that leaves its target as it was (no face of it modified or
  // removed, the merge included) is not checked: its result is the target's
  // faces as they were, which the caller finds unchanged (volume_changed)
  // and leaves alone (mitcad#181: checking a large body that a through-all
  // cut's tool passed by took 20–30 s).
  const bool untouched = op == BooleanOp::Cut && targets.size() == 1 && [&] {
    for (const TopoDS_Shape& argument : arguments) {
      for (TopExp_Explorer it(argument, TopAbs_FACE); it.More(); it.Next()) {
        if (operation->IsDeleted(it.Current()) || !operation->Modified(it.Current()).IsEmpty()) {
          return false;
        }
      }
    }
    return true;
  }();
  // The faces of the targets OCCT's checker found valid, as the operation
  // got them: the result's faces that are still theirs need no check
  // (is_valid_where_changed).
  ShapeMap kept;
  bool targets_valid = true;
  for (std::size_t i = 0; i < targets.size(); ++i) {
    if (!targets[i].first->checked_valid().value_or(false)) {
      targets_valid = false;
      continue;
    }
    for (TopExp_Explorer it(copy.shape(i), TopAbs_FACE); it.More(); it.Next()) {
      kept.Add(it.Current());
    }
  }
  trace.mark("kept");
  // What the checker saw, for the trace: the faces where the result changed
  // (and the solids of several shells it checked as solids), or the whole
  // result and why (util.hpp: a target the checker had not found valid, a
  // non-manifold edge, ...).
  detail::WhereChanged report;
  const bool valid_after_merge =
      !simplify || untouched || detail::is_valid_where_changed(namer.result(), kept, &report);
  trace.mark("check");
  if (simplify && !untouched) {
    trace.note(report.whole != nullptr
                   ? std::string("checked=whole-") + report.whole
                   : "checked=" + std::to_string(report.faces) + "-faces" +
                         (report.solids > 0 ? "+" + std::to_string(report.solids) + "-solids" : std::string()));
  }
  if (!valid_after_merge) {
    // The merge can break a result the operation made valid (a join of a
    // corpus design: its merged faces made a shell that cannot be
    // oriented); then the result keeps the faces as the operation left
    // them.
    return run_operation(op, targets, tool, tool_tag, false, seen, reduce, split, fuzzy, lost);
  }
  if (!simplify && !untouched) {
    detail::require_valid_where_changed(namer.result(), kept, "the boolean operation", &report);
    trace.note(report.whole != nullptr ? std::string("checked=whole-") + report.whole
                                       : "checked=" + std::to_string(report.faces) + "-faces");
  }
  trace.mark("check2");
  // Checked, or the targets as they were (a cut that left them alone).
  namer.set_valid(!untouched || targets_valid);
  if (split != nullptr) {
    // Whether a face of a target became several.
    *split = false;
    for (const auto& [target, tag] : targets) {
      for (int i = 0; i < target->face_count() && !*split; ++i) {
        const TopoDS_Shape face = far ? far->stand_in(copy.of(target->face(i))) : copy.of(target->face(i));
        int images = 0;
        if (!face.IsNull()) {
          for (const TopoDS_Shape& image : operation->Modified(face)) {
            images += namer.contains(image) ? 1 : 0;
          }
        }
        *split = images > 1;
      }
    }
  }
  if (seen != nullptr) {
    seen->clear();
    for (std::size_t i = 0; i < targets.size(); ++i) {
      seen->push_back(copy.shape(i));
    }
  }
  namer.set_unified(simplify);
  trace.mark("split");
  if (!far) {
    return namer;
  }
  // The holes back in the faces that took their faces' place: the checker
  // saw the result without them, and they are as they were (apart from the
  // tool, and from the faces it changed).
  const std::optional<detail::FarFeatures::Restored> restored = far->restore(operation->Shape(), *operation);
  trace.mark("restore");
  if (!restored) {
    return run_operation(op, targets, tool, tool_tag, simplify, seen, false, split, fuzzy, lost);
  }
  ShapeMap replaced;
  for (const auto& entry : restored->replaced) {
    replaced.Add(entry.first);
  }
  detail::FaceNamer whole(restored->shape);
  whole.adopt(namer, [&](const TopoDS_Shape& face) {
    const int index = replaced.FindIndex(face);
    return index > 0 ? restored->replaced[static_cast<std::size_t>(index - 1)].second : TopoDS_Shape();
  });
  const auto& [target, tag] = targets.front();
  for (int i = 0; i < target->face_count(); ++i) {
    const TopoDS_Shape face = copy.of(target->face(i));
    if (far->stand_in(face).IsNull()) {
      whole.add(face, target->face_names(i), tag);
    }
  }
  whole.finish();
  trace.mark("rename");
  whole.set_unified(simplify);
  // As valid as the result the checker saw: the holes are as they were.
  whole.set_valid(!untouched || targets_valid);
  return whole;
}


// Tools of at least twice this many solids are cut tile by tile, about
// this many solids at a time (tiled_cut): smaller tiles make more cuts,
// each redoing the work that grows with the body's size; larger ones
// bring back the quadratic work. Measured on one loaded machine: a plate
// with 4900 holes 41 s in one cut, 20 s by 150, 18 s by 300, 12 s by 600;
// the wall of a tube with 1225 holes (curved faces, where the parallel
// intersections dominate) 26 s in one cut, 14 s by 76, 12 s by 300, 17 s
// by 600.
constexpr std::size_t kTile = 300;
constexpr std::size_t kTiled = 2 * kTile;

// The tool's solids in groups of nearby ones: split at the median of the
// longer side of their boxes until a group has at most `size`.
void group_solids(std::vector<std::pair<gp_Pnt, int>>& solids, std::size_t begin, std::size_t end,
                  std::size_t size, std::vector<std::vector<int>>& groups) {
  if (end - begin <= size) {
    groups.emplace_back();
    for (std::size_t i = begin; i < end; ++i) {
      groups.back().push_back(solids[i].second);
    }
    return;
  }
  Bnd_Box box;
  for (std::size_t i = begin; i < end; ++i) {
    box.Add(solids[i].first);
  }
  double xmin = 0, ymin = 0, zmin = 0, xmax = 0, ymax = 0, zmax = 0;
  box.Get(xmin, ymin, zmin, xmax, ymax, zmax);
  const double extent[3] = {xmax - xmin, ymax - ymin, zmax - zmin};
  const int axis = extent[0] >= extent[1] ? (extent[0] >= extent[2] ? 0 : 2) : (extent[1] >= extent[2] ? 1 : 2);
  const std::size_t middle = begin + (end - begin) / 2;
  std::nth_element(solids.begin() + static_cast<std::ptrdiff_t>(begin),
                   solids.begin() + static_cast<std::ptrdiff_t>(middle),
                   solids.begin() + static_cast<std::ptrdiff_t>(end),
                   [axis](const auto& a, const auto& b) { return a.first.Coord(axis + 1) < b.first.Coord(axis + 1); });
  group_solids(solids, begin, middle, size, groups);
  group_solids(solids, middle, end, size, groups);
}

// A cut with a tool of many solids apart from each other (a pattern's
// copies) as several cuts, each with the solids of one region: OCCT's
// boolean and checker take time quadratic in the holes of a face, and each
// region's cut sees only the holes near it (far_features.hpp). The same
// result as one cut, as long as no face of the target splits into pieces
// (their numbers would follow the order of the cuts) and the target stays
// one solid; otherwise none (the caller cuts at once). Its solid, and
// whether the target changed (`changed`). A cancel request stops it
// between the cuts too.
std::optional<std::vector<detail::FaceNamer::Piece>> tiled_cut(const Shape& target, int tag, const Shape& tool,
                                                               int tool_tag, bool& changed) {
  std::vector<TopoDS_Shape> parts;
  for (TopExp_Explorer it(tool.occt(), TopAbs_SOLID); it.More(); it.Next()) {
    parts.push_back(it.Current());
  }
  if (parts.size() < kTiled || !detail::far_features_enabled()) {
    return std::nullopt;
  }
  int target_solids = 0;
  for (TopExp_Explorer it(target.occt(), TopAbs_SOLID); it.More(); it.Next()) {
    ++target_solids;
  }
  if (target_solids != 1) {
    return std::nullopt;
  }
  // The solids must be apart: boxes sorted along x, each against those
  // that start before it ends.
  std::vector<std::pair<Bnd_Box, int>> boxes;
  for (std::size_t i = 0; i < parts.size(); ++i) {
    Bnd_Box box;
    BRepBndLib::Add(parts[i], box);
    box.Enlarge(kContact);
    if (box.IsVoid()) {
      return std::nullopt;
    }
    boxes.emplace_back(box, static_cast<int>(i));
  }
  std::sort(boxes.begin(), boxes.end(),
            [](const auto& a, const auto& b) { return a.first.CornerMin().X() < b.first.CornerMin().X(); });
  for (std::size_t i = 0; i < boxes.size(); ++i) {
    const double end = boxes[i].first.CornerMax().X();
    for (std::size_t j = i + 1; j < boxes.size() && boxes[j].first.CornerMin().X() <= end; ++j) {
      if (!boxes[i].first.IsOut(boxes[j].first)) {
        return std::nullopt;
      }
    }
  }
  std::vector<std::pair<gp_Pnt, int>> centres;
  for (const auto& [box, i] : boxes) {
    centres.emplace_back(gp_Pnt((box.CornerMin().XYZ() + box.CornerMax().XYZ()) / 2.0), i);
  }
  std::vector<std::vector<int>> groups;
  group_solids(centres, 0, centres.size(), kTile, groups);

  changed = false;
  std::vector<detail::FaceNamer::Piece> current;
  BRep_Builder builder;
  for (const std::vector<int>& group : groups) {
    throw_if_cancelled();
    TopoDS_Compound compound;
    builder.MakeCompound(compound);
    std::vector<Shape::NamedFace> names;
    for (int i : group) {
      builder.Add(compound, parts[static_cast<std::size_t>(i)]);
      for (TopExp_Explorer f(parts[static_cast<std::size_t>(i)], TopAbs_FACE); f.More(); f.Next()) {
        names.push_back({f.Current(), tool.names_of_face(f.Current())});
      }
    }
    const Shape tile(compound, names);
    const Shape& from = current.empty() ? target : *current.front().shape;
    std::vector<TopoDS_Shape> seen;
    bool split = false;
    std::vector<detail::FaceNamer::Piece> solids =
        run_operation(BooleanOp::Cut, {{&from, tag}}, tile, tool_tag, true, &seen, true, &split).pieces();
    if (split || solids.size() != 1) {
      return std::nullopt;
    }
    changed = changed || volume_changed(from, seen.front(), *solids.front().shape);
    current = std::move(solids);
  }
  return current;
}

// Whether a face of a cylinder or cone has a seam that steps: seam edges
// (on the face twice) that do not all run along one line of the surface
// across its periodic direction (two faces merged whose seams lay a quarter
// turn apart, joined by an arc of the circle between them used twice).
bool steps_its_seam(const TopoDS_Face& face) {
  const BRepAdaptor_Surface surface(face, false);
  if (surface.GetType() != GeomAbs_Cylinder && surface.GetType() != GeomAbs_Cone) {
    return false;
  }
  std::optional<double> at;
  for (TopExp_Explorer it(face, TopAbs_EDGE); it.More(); it.Next()) {
    const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
    if (!BRep_Tool::IsClosed(edge, face)) {
      continue;
    }
    double first = 0.0;
    double last = 0.0;
    const occ::handle<Geom2d_Curve> pcurve = BRep_Tool::CurveOnSurface(edge, face, first, last);
    if (pcurve.IsNull()) {
      return false;
    }
    const gp_Pnt2d a = pcurve->Value(first);
    const gp_Pnt2d b = pcurve->Value(last);
    // Along the surface's line, u fixed (either of the seam's two sides).
    const double u = std::remainder(a.X(), surface.UPeriod());
    if (std::abs(a.X() - b.X()) > Precision::PConfusion() ||
        (at && std::abs(std::remainder(u - *at, surface.UPeriod())) > Precision::PConfusion())) {
      return true;
    }
    at = u;
  }
  return false;
}

// A copy of a target whose cylindrical and conical faces with a stepping
// seam (steps_its_seam) are rebuilt with one straight seam through a vertex
// of their boundary, the boundary edge it crosses split there in the
// neighbouring faces too, the faces' names kept; null when none steps
// (mitcad#235: OCCT's cut of a coil's helix removed nothing from the bore
// of such a face, and gave the file's state from the rebuilt one).
ShapePtr reseamed(const Shape& target) {
  std::vector<TopoDS_Face> stepping;
  for (int f = 0; f < target.face_count(); ++f) {
    if (steps_its_seam(target.face(f))) {
      stepping.push_back(target.face(f));
    }
  }
  if (stepping.empty()) {
    return nullptr;
  }
  // On a copy: healing adds p-curves to the edges it keeps.
  const ShapePtr copy = detail::working_copy(target);
  const Shape& own = *copy;
  stepping.clear();
  for (int f = 0; f < own.face_count(); ++f) {
    if (steps_its_seam(own.face(f))) {
      stepping.push_back(own.face(f));
    }
  }
  try {
    OCC_CATCH_SIGNALS
    const occ::handle<ShapeBuild_ReShape> reshape = new ShapeBuild_ReShape;
    BRep_Builder builder;
    for (const TopoDS_Face& face : stepping) {
      // The face's boundary without its seams, as closed wires.
      const occ::handle<NCollection_HSequence<TopoDS_Shape>> edges = new NCollection_HSequence<TopoDS_Shape>;
      std::vector<TopoDS_Edge> boundary;
      for (TopExp_Explorer it(face.Oriented(TopAbs_FORWARD), TopAbs_EDGE); it.More(); it.Next()) {
        const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
        if (!BRep_Tool::IsClosed(edge, face)) {
          edges->Append(edge);
          boundary.push_back(edge);
        }
      }
      const occ::handle<NCollection_HSequence<TopoDS_Shape>> wires =
          ShapeAnalysis_FreeBounds::ConnectEdgesToWires(edges, Precision::Confusion(), false);
      if (wires.IsNull() || wires->IsEmpty()) {
        return nullptr;
      }
      BRepBuilderAPI_MakeFace make(BRep_Tool::Surface(face), TopoDS::Wire(wires->Value(1)), true);
      for (int w = 2; w <= wires->Length(); ++w) {
        make.Add(TopoDS::Wire(wires->Value(w)));
      }
      if (!make.IsDone()) {
        return nullptr;
      }
      // Shape healing adds the seam and splits the edge it crosses.
      ShapeFix_Face fix(make.Face());
      fix.FixOrientationMode() = 1;
      fix.FixMissingSeamMode() = 1;
      fix.Perform();
      TopoDS_Face rebuilt = fix.Face();
      // A boundary edge the face no longer has: its pieces in the face,
      // everywhere.
      for (const TopoDS_Edge& old : boundary) {
        bool kept = false;
        for (TopExp_Explorer it(rebuilt, TopAbs_EDGE); it.More() && !kept; it.Next()) {
          kept = it.Current().IsSame(old);
        }
        if (kept) {
          continue;
        }
        TopoDS_Compound pieces;
        builder.MakeCompound(pieces);
        int count = 0;
        for (TopExp_Explorer it(rebuilt, TopAbs_EDGE); it.More(); it.Next()) {
          const TopoDS_Edge& piece = TopoDS::Edge(it.Current());
          if (BRep_Tool::IsClosed(piece, rebuilt) ||
              std::any_of(boundary.begin(), boundary.end(), [&](const TopoDS_Edge& e) { return e.IsSame(piece); })) {
            continue;
          }
          const BRepAdaptor_Curve curve(piece);
          const gp_Pnt middle = curve.Value(0.5 * (curve.FirstParameter() + curve.LastParameter()));
          BRepExtrema_DistShapeShape distance(BRepBuilderAPI_MakeVertex(middle).Vertex(), old);
          if (distance.IsDone() && distance.Value() <= 1.0e-6) {
            builder.Add(pieces, piece.Oriented(TopAbs_FORWARD));
            ++count;
          }
        }
        if (count == 0) {
          return nullptr;
        }
        reshape->Replace(old.Oriented(TopAbs_FORWARD), pieces);
      }
      rebuilt.Orientation(TopAbs_FORWARD);
      reshape->Replace(face.Oriented(TopAbs_FORWARD), rebuilt);
    }
    const TopoDS_Shape result = reshape->Apply(own.occt());
    if (result.IsNull() || !BRepCheck_Analyzer(result, true, true).IsValid()) {
      return nullptr;
    }
    detail::FaceNamer namer(result);
    namer.carry(*reshape->History(), own);
    namer.finish();
    namer.set_valid(true); // checked above
    return namer.shape();
  } catch (const Standard_Failure&) {
    return nullptr;
  }
}

// The fuzzy values a join tries up to `reach` (at least kNearContact),
// smallest first: kNearContact, then 3 and 10 times each power of ten above
// it below `reach`, then `reach` (mitcad#298). The smallest that works
// moves the least geometry.
std::vector<double> fuzzy_steps(double reach) {
  std::vector<double> steps;
  for (double decade = kNearContact; decade < reach; decade *= 10.0) {
    steps.push_back(decade);
    if (3.0 * decade < reach) {
      steps.push_back(3.0 * decade);
    }
  }
  steps.push_back(std::max(reach, kNearContact));
  return steps;
}

// The edges of a shape shorter than kNearContact between two vertices of
// their own: what a join leaves where it keeps two vertices of its operands
// apart that lie within the noise of the file's geometry of each other. A
// boss whose profile runs into a wall tangentially, its line's end found a
// micrometre along the wall from the tangent edge already there, left a
// strip of the wall that wide between the two halves of the boss's
// cylinder, its ends edges of 1.2 µm (mitcad#326).
int tiny_edges(const TopoDS_Shape& shape) {
  ShapeMap seen;
  int count = 0;
  for (TopExp_Explorer it(shape, TopAbs_EDGE); it.More(); it.Next()) {
    const int before = seen.Extent();
    if (seen.Add(it.Current()) <= before) {
      continue;
    }
    const TopoDS_Edge& edge = TopoDS::Edge(it.Current());
    if (BRep_Tool::Degenerated(edge)) {
      continue;
    }
    TopoDS_Vertex first;
    TopoDS_Vertex last;
    TopExp::Vertices(edge, first, last);
    if (first.IsNull() || last.IsNull() || first.IsSame(last) ||
        BRep_Tool::Pnt(first).Distance(BRep_Tool::Pnt(last)) >= kNearContact) {
      continue;
    }
    BRepAdaptor_Curve curve(edge);
    if (GCPnts_AbscissaPoint::Length(curve) < kNearContact) {
      ++count;
    }
  }
  return count;
}

// A cut of one target within a tolerance (mitcad#235), for an importer
// whose bodies match a file's only within its geometric tolerance: where
// the exact cut fails (an error, an invalid result, faces the result's
// solids could not be built with, which the result then lacks, or pieces
// of the tool left inside the body: section edges of a stored body's
// neighbouring faces that end micrometres apart, where the tool lies on a
// face of the body), it is retried with the smallest fuzzy value up to
// `within` (fuzzy_steps) that gives a valid result of all its faces. A
// cut that works exactly stays exact; where no fuzzy value works, the
// exact cut's result or error stands. `seen` gets the copy of the target
// the result was made from.
std::vector<detail::FaceNamer::Piece> cut_within(const Shape& target, int tag, const Shape& tool, int tool_tag,
                                                 double within, std::vector<TopoDS_Shape>& seen, PhaseTrace& trace) {
  bool lost = false;
  std::optional<std::vector<detail::FaceNamer::Piece>> exact;
  std::exception_ptr failure;
  try {
    exact = run_operation(BooleanOp::Cut, {{&target, tag}}, tool, tool_tag, true, &seen, true, nullptr, 0.0, &lost)
                .pieces();
  } catch (const Cancelled&) {
    throw;
  } catch (...) {
    failure = std::current_exception();
  }
  if (exact && !lost) {
    return *exact;
  }
  for (const double step : fuzzy_steps(within)) {
    std::vector<TopoDS_Shape> fuzzy_seen;
    bool fuzzy_lost = false;
    try {
      std::vector<detail::FaceNamer::Piece> pieces =
          run_operation(BooleanOp::Cut, {{&target, tag}}, tool, tool_tag, true, &fuzzy_seen, true, nullptr, step,
                        &fuzzy_lost)
              .pieces();
      if (!fuzzy_lost) {
        trace.note("cut fuzzy=" + std::to_string(step));
        seen = std::move(fuzzy_seen);
        return pieces;
      }
    } catch (const Cancelled&) {
      throw;
    } catch (...) {
    }
  }
  if (exact) {
    return *exact;
  }
  std::rethrow_exception(failure);
}

} // namespace

// The contacts of a new Extrude's tool (contact.cpp, mitcad#289).
bool detail::shapes_touch(const Shape& a, const Shape& b) { return touches(a, b); }

BooleanResult boolean(BooleanOp op, const std::vector<const Shape*>& targets, const Shape& tool,
                      double within) {
  for (const Shape* target : targets) {
    detail::require_surfaces(target->occt(), operation_name(op));
  }
  detail::require_surfaces(tool.occt(), operation_name(op));
  return detail::run(operation_name(op), [&] {
    PhaseTrace trace(std::string("whole-") + operation_name(op) + " targets=" + std::to_string(targets.size()));
    BooleanResult result;
    result.touched.assign(targets.size(), false);
    const Bnd_Box tool_box = box_of(tool);
    const int tool_tag = static_cast<int>(targets.size());
    const auto to_pieces = [tool_tag](const std::vector<detail::FaceNamer::Piece>& solids) {
      std::vector<BooleanPiece> pieces;
      for (const detail::FaceNamer::Piece& piece : solids) {
        BooleanPiece out{piece.shape, {}};
        for (int source : piece.sources) {
          if (source != tool_tag) {
            out.sources.push_back(static_cast<std::size_t>(source));
          }
        }
        pieces.push_back(std::move(out));
      }
      return pieces;
    };
    const auto pieces_of = [&](const detail::FaceNamer& namer) { return to_pieces(namer.pieces()); };

    if (op == BooleanOp::Join) {
      // How far a tool that touches nothing reaches: kNearContact, or the
      // caller's tolerance (mitcad#298).
      const double reach = std::max(kNearContact, within);
      // The join of `joined` with a fuzzy value: the first of `steps` from
      // `from` on that gives a result (a valid one: run_operation checks
      // it); the last failure when none does.
      const auto fuzzy_join = [&](const std::vector<std::pair<const Shape*, int>>& joined, double from) {
        const std::vector<double> steps = fuzzy_steps(reach);
        for (std::size_t s = 0; s < steps.size(); ++s) {
          if (steps[s] < from) {
            continue;
          }
          try {
            detail::FaceNamer namer =
                run_operation(op, joined, tool, tool_tag, true, nullptr, false, nullptr, steps[s]);
            trace.note("fuzzy=" + std::to_string(steps[s]));
            return namer;
          } catch (const Cancelled&) {
            throw;
          } catch (...) {
            if (s + 1 == steps.size()) {
              throw;
            }
          }
        }
        return run_operation(op, joined, tool, tool_tag, true, nullptr, false, nullptr, reach);
      };
      // Within a tolerance, an exact join that leaves more edges of zero
      // length (tiny_edges) than its operands had, `allowed`, is tried again
      // with the smallest fuzzy value that leaves no more than they had: the
      // fuzzy value merges the vertices that lie within it of each other
      // (mitcad#326). None when no fuzzy value does; the exact join stays.
      const auto operands_tiny = [&](const std::vector<std::pair<const Shape*, int>>& joined) {
        int count = tiny_edges(tool.occt());
        for (const auto& entry : joined) {
          count += tiny_edges(entry.first->occt());
        }
        return count;
      };
      const auto without_tiny_edges =
          [&](const std::vector<std::pair<const Shape*, int>>& joined,
              int allowed) -> std::optional<std::vector<detail::FaceNamer::Piece>> {
        for (const double step : fuzzy_steps(reach)) {
          std::vector<detail::FaceNamer::Piece> solids;
          try {
            solids = run_operation(op, joined, tool, tool_tag, true, nullptr, false, nullptr, step).pieces();
          } catch (const Cancelled&) {
            throw;
          } catch (...) {
            continue;
          }
          int count = 0;
          for (const detail::FaceNamer::Piece& piece : solids) {
            count += tiny_edges(piece.shape->occt());
          }
          if (count <= allowed) {
            trace.note("tiny edges fuzzy=" + std::to_string(step));
            return solids;
          }
        }
        return std::nullopt;
      };
      // Whether a piece of a join holds less volume than a target it merged
      // (loses_volume, mitcad#964); `copies` are the copies of `joined`'s
      // targets the operation worked on, in its order.
      const auto lost_volume = [&](const std::vector<detail::FaceNamer::Piece>& solids,
                                   const std::vector<std::pair<const Shape*, int>>& joined,
                                   const std::vector<TopoDS_Shape>& copies) {
        if (copies.size() != joined.size()) {
          return false;
        }
        for (const detail::FaceNamer::Piece& piece : solids) {
          if (piece.sources.count(tool_tag) == 0) {
            continue;
          }
          for (std::size_t k = 0; k < joined.size(); ++k) {
            if (piece.sources.count(joined[k].second) > 0 &&
                loses_volume(*joined[k].first, copies[k], *piece.shape)) {
              return true;
            }
          }
        }
        return false;
      };
      // A join that lost volume is wrong whatever the tolerance: it is
      // tried again with the smallest fuzzy value (fuzzy_steps, from
      // kNearContact) whose result merges every target with the tool and
      // loses none. None when no fuzzy value does; the exact join stays.
      const auto without_volume_loss = [&](const std::vector<std::pair<const Shape*, int>>& joined)
          -> std::optional<std::vector<detail::FaceNamer::Piece>> {
        for (const double step : fuzzy_steps(reach)) {
          std::vector<TopoDS_Shape> copies;
          std::vector<detail::FaceNamer::Piece> solids;
          try {
            solids = run_operation(op, joined, tool, tool_tag, true, &copies, false, nullptr, step).pieces();
          } catch (const Cancelled&) {
            throw;
          } catch (...) {
            continue;
          }
          const bool merges_all = std::all_of(joined.begin(), joined.end(), [&](const auto& entry) {
            return std::any_of(solids.begin(), solids.end(), [&](const detail::FaceNamer::Piece& piece) {
              return piece.sources.count(tool_tag) > 0 && piece.sources.count(entry.second) > 0;
            });
          });
          if (merges_all && !lost_volume(solids, joined, copies)) {
            trace.note("lost volume fuzzy=" + std::to_string(step));
            return solids;
          }
        }
        return std::nullopt;
      };
      // Only the targets the tool touches: fusing all of them would also
      // merge targets that overlap each other but not the tool.
      std::vector<std::pair<const Shape*, int>> near;
      Bnd_Box join_box = tool_box;
      join_box.Enlarge(reach);
      for (std::size_t i = 0; i < targets.size(); ++i) {
        if (!join_box.IsOut(box_of(*targets[i]))) {
          near.emplace_back(targets[i], static_cast<int>(i));
        }
      }
      if (near.empty()) {
        return result;
      }
      // One target near the tool is fused with it first, and the result
      // tells whether they touch: when a solid of it holds the target with
      // the tool, that union is the join and nothing needs measuring.
      // Several targets near the tool are measured first (touches) and only
      // those touched are fused: fusing all of them took many times as long
      // as the join itself (a large body and four small ones near the tool:
      // 9 s instead of 0.5 s), and when the tool touched two of them the
      // result could not tell, so they were fused again.
      if (near.size() == 1) {
        std::vector<detail::FaceNamer::Piece> solids;
        std::vector<TopoDS_Shape> copies;
        try {
          solids = run_operation(op, near, tool, tool_tag, true, &copies).pieces();
        } catch (const Cancelled&) {
          throw;
        } catch (...) {
          // Measured below; the union then fails again if it must.
          solids.clear();
        }
        const int target = near.front().second;
        // A target the tool swallows has no face left: measured below.
        const auto merges = [&](const std::vector<detail::FaceNamer::Piece>& pieces) {
          return std::any_of(pieces.begin(), pieces.end(), [&](const auto& piece) {
            return piece.sources.count(tool_tag) > 0 && piece.sources.count(target) > 0;
          });
        };
        const bool merged = merges(solids);
        if (merged && lost_volume(solids, near, copies)) {
          if (std::optional<std::vector<detail::FaceNamer::Piece>> fuzzy = without_volume_loss(near)) {
            solids = std::move(*fuzzy);
          }
        }
        if (merged && within > 0.0) {
          int count = 0;
          for (const detail::FaceNamer::Piece& piece : solids) {
            count += tiny_edges(piece.shape->occt());
          }
          const int allowed = operands_tiny(near);
          if (count > allowed) {
            std::optional<std::vector<detail::FaceNamer::Piece>> fuzzy = without_tiny_edges(near, allowed);
            if (fuzzy && merges(*fuzzy)) {
              solids = std::move(*fuzzy);
            }
          }
        }
        if (merged) {
          for (const detail::FaceNamer::Piece& piece : solids) {
            if (piece.sources.count(tool_tag) == 0) {
              continue; // a solid the tool does not reach, as it was
            }
            BooleanPiece out{piece.shape, {}};
            for (int source : piece.sources) {
              if (source != tool_tag) {
                out.sources.push_back(static_cast<std::size_t>(source));
                result.touched[static_cast<std::size_t>(source)] = true;
              }
            }
            result.pieces.push_back(std::move(out));
          }
          return result;
        }
      }
      trace.mark("first");
      std::vector<std::pair<const Shape*, int>> joined;
      for (const auto& [target, i] : near) {
        if (touches(*target, tool)) {
          joined.emplace_back(target, i);
          result.touched[static_cast<std::size_t>(i)] = true;
        }
      }
      trace.mark("touches");
      if (joined.empty()) {
        // Geometry the file stores differs from the model's by a few
        // nanometres (a circle projected from an edge into a sketch is
        // that far off the edge's own circle): a tool that far from a body
        // joins it, as the file's own join does. The targets within
        // kNearContact are fused with a fuzzy value that wide (the faces
        // between them are not reduced), and a target joins where the tool
        // merged with it. This is the failure path only (no target touched
        // within kContact), and tools farther away are as untouched as
        // before. A caller's tolerance (`within`, mitcad#298) widens this
        // to bodies that match a file's within it: each target is fused
        // with the smallest fuzzy value (fuzzy_steps) that merges it.
        std::vector<std::pair<const Shape*, int>> close;
        {
          const ContactScope scope(reach);
          for (const auto& [target, i] : near) {
            if (touches(*target, tool)) {
              close.emplace_back(target, i);
            }
          }
        }
        double fuzzy = kNearContact;
        for (const auto& entry : close) {
          const Shape* const target = entry.first;
          const int i = entry.second;
          for (const double step : fuzzy_steps(reach)) {
            if (step < fuzzy && step < reach) {
              continue;
            }
            std::vector<detail::FaceNamer::Piece> solids;
            try {
              solids =
                  run_operation(op, {{target, i}}, tool, tool_tag, true, nullptr, false, nullptr, step).pieces();
            } catch (const Cancelled&) {
              throw;
            } catch (...) {
              continue;
            }
            const bool merged = std::any_of(solids.begin(), solids.end(), [&](const auto& piece) {
              return piece.sources.count(tool_tag) > 0 && piece.sources.count(i) > 0;
            });
            if (merged) {
              joined.emplace_back(target, i);
              result.touched[static_cast<std::size_t>(i)] = true;
              fuzzy = std::max(fuzzy, step);
              break;
            }
          }
        }
        trace.note("near-contact fuzzy=" + std::to_string(fuzzy));
        if (joined.empty()) {
          return result;
        }
        result.pieces = pieces_of(fuzzy_join(joined, fuzzy));
        return result;
      }
      if (within <= 0.0) {
        std::vector<TopoDS_Shape> copies;
        std::vector<detail::FaceNamer::Piece> solids =
            run_operation(op, joined, tool, tool_tag, true, &copies).pieces();
        if (lost_volume(solids, joined, copies)) {
          if (std::optional<std::vector<detail::FaceNamer::Piece>> fuzzy = without_volume_loss(joined)) {
            solids = std::move(*fuzzy);
          }
        }
        result.pieces = to_pieces(solids);
        return result;
      }
      // Within a tolerance (mitcad#298), for bodies that match a file's
      // only within it: an exact join that fails (its result invalid, where
      // the operands meet within that noise) is retried with the smallest
      // fuzzy value that gives a valid result.
      std::vector<BooleanPiece> exact;
      bool lost = false;
      std::vector<TopoDS_Shape> exact_copies;
      std::vector<detail::FaceNamer::Piece> exact_solids;
      try {
        exact_solids =
            run_operation(op, joined, tool, tool_tag, true, &exact_copies, true, nullptr, 0.0, &lost).pieces();
        exact = to_pieces(exact_solids);
      } catch (const Cancelled&) {
        throw;
      } catch (...) {
        trace.note("exact join failed: fuzzy");
        result.pieces = pieces_of(fuzzy_join(joined, 0.0));
        return result;
      }
      if (!lost && lost_volume(exact_solids, joined, exact_copies)) {
        if (std::optional<std::vector<detail::FaceNamer::Piece>> fuzzy = without_volume_loss(joined)) {
          result.pieces = to_pieces(*fuzzy);
          return result;
        }
      }
      // An exact join that could not build its solids of all the faces
      // (OCCT cannot orient them and leaves faces unused: no solid at all,
      // where the tool lies on a face of the body and its sides meet the
      // body's faces micrometres apart, mitcad#235) is retried with the
      // smallest fuzzy value that builds a solid of the targets and the
      // tool with every face; where none does, the exact result stands.
      const auto holds_targets = [](const std::vector<BooleanPiece>& pieces) {
        return std::any_of(pieces.begin(), pieces.end(),
                           [](const BooleanPiece& piece) { return !piece.sources.empty(); });
      };
      if (lost || !holds_targets(exact)) {
        for (const double step : fuzzy_steps(reach)) {
          bool fuzzy_lost = false;
          std::vector<BooleanPiece> pieces;
          try {
            pieces = pieces_of(
                run_operation(op, joined, tool, tool_tag, true, nullptr, false, nullptr, step, &fuzzy_lost));
          } catch (const Cancelled&) {
            throw;
          } catch (...) {
            continue;
          }
          if (!fuzzy_lost && holds_targets(pieces)) {
            trace.note("lost faces fuzzy=" + std::to_string(step));
            result.pieces = std::move(pieces);
            return result;
          }
        }
      }
      // A tool solid the exact join leaves on its own that comes within
      // the tolerance of a target (it touches the target only along an
      // edge, its face a few micrometres off the target's) joins it too:
      // the targets near such solids are fused with the smallest fuzzy
      // value that merges every one of them. Solids farther away stay
      // apart, and a join that merged everything is the exact one.
      int strays = 0;
      int close_strays = 0;
      std::vector<std::pair<const Shape*, int>> fused = joined;
      {
        const ContactScope scope(reach);
        for (const BooleanPiece& piece : exact) {
          if (!piece.sources.empty()) {
            continue;
          }
          ++strays;
          bool close = false;
          for (const auto& [target, i] : near) {
            if (touches(*target, *piece.shape)) {
              close = true;
              const bool listed = std::any_of(fused.begin(), fused.end(),
                                              [i = i](const auto& entry) { return entry.second == i; });
              if (!listed) {
                fused.emplace_back(target, i);
              }
            }
          }
          close_strays += close ? 1 : 0;
        }
      }
      trace.mark("strays");
      if (close_strays > 0) {
        for (const double step : fuzzy_steps(reach)) {
          std::vector<BooleanPiece> pieces;
          bool fuzzy_lost = false;
          try {
            pieces = pieces_of(
                run_operation(op, fused, tool, tool_tag, true, nullptr, false, nullptr, step, &fuzzy_lost));
          } catch (const Cancelled&) {
            throw;
          } catch (...) {
            continue;
          }
          const int left = static_cast<int>(std::count_if(
              pieces.begin(), pieces.end(), [](const BooleanPiece& piece) { return piece.sources.empty(); }));
          // (Not a result that lost the targets' solids or faces.)
          if (left <= strays - close_strays && !fuzzy_lost && holds_targets(pieces)) {
            trace.note("strays fuzzy=" + std::to_string(step));
            for (const auto& entry : fused) {
              result.touched[static_cast<std::size_t>(entry.second)] = true;
            }
            result.pieces = std::move(pieces);
            return result;
          }
        }
      }
      int count = 0;
      for (const BooleanPiece& piece : exact) {
        count += tiny_edges(piece.shape->occt());
      }
      const int allowed = operands_tiny(joined);
      if (count > allowed) {
        if (const std::optional<std::vector<detail::FaceNamer::Piece>> fuzzy = without_tiny_edges(joined, allowed)) {
          result.pieces = to_pieces(*fuzzy);
          return result;
        }
      }
      result.pieces = std::move(exact);
      return result;
    }

    for (std::size_t i = 0; i < targets.size(); ++i) {
      if (tool_box.IsOut(box_of(*targets[i]))) {
        continue;
      }
      if (op == BooleanOp::Cut) {
        bool changed = false;
        if (const auto tiled = tiled_cut(*targets[i], static_cast<int>(i), tool, tool_tag, changed)) {
          if (changed) {
            result.touched[i] = true;
            for (const detail::FaceNamer::Piece& piece : *tiled) {
              result.pieces.push_back({piece.shape, {i}});
            }
          }
          continue;
        }
      }
      std::vector<TopoDS_Shape> seen;
      std::vector<detail::FaceNamer::Piece> solids;
      if (op == BooleanOp::Cut && within > 0.0) {
        solids = cut_within(*targets[i], static_cast<int>(i), tool, tool_tag, within, seen, trace);
      } else {
        solids = run_operation(op, {{targets[i], static_cast<int>(i)}}, tool, tool_tag, true, &seen).pieces();
      }
      // A single piece of the same volume is the target unchanged (a tool
      // touching a face shares the face but removes nothing); no piece at
      // all is a target cut away completely, or one the tool of an
      // intersection does not reach.
      bool changed = solids.empty() ? op == BooleanOp::Cut : solids.size() > 1;
      trace.mark("op");
      if (solids.size() == 1) {
        changed = volume_changed(*targets[i], seen.front(), *solids.front().shape);
      }
      trace.mark("volume");
      // A cut that removes nothing from a target with a stepping seam
      // (reseamed) is tried on the target rebuilt with straight seams.
      if (!changed && op == BooleanOp::Cut) {
        if (const ShapePtr straight = reseamed(*targets[i])) {
          std::vector<TopoDS_Shape> straight_seen;
          std::vector<detail::FaceNamer::Piece> straight_solids =
              within > 0.0
                  ? cut_within(*straight, static_cast<int>(i), tool, tool_tag, within, straight_seen, trace)
                  : run_operation(op, {{straight.get(), static_cast<int>(i)}}, tool, tool_tag, true, &straight_seen)
                        .pieces();
          const bool straight_changed =
              straight_solids.size() != 1 ||
              volume_changed(*straight, straight_seen.front(), *straight_solids.front().shape);
          if (straight_changed) {
            trace.note("reseamed");
            solids = std::move(straight_solids);
            changed = true;
          }
        }
      }
      if (!changed) {
        continue;
      }
      result.touched[i] = true;
      for (const detail::FaceNamer::Piece& piece : solids) {
        result.pieces.push_back({piece.shape, {i}});
      }
    }
    return result;
  });
}

} // namespace mitcad::geometry
