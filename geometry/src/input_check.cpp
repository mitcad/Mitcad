// SPDX-License-Identifier: MIT
#include "mitcad/geometry/input_check.hpp"

#include <cstdio>
#include <cstdlib>
#include <algorithm>
#include <cstring>
#include <map>
#include <mutex>
#include <unordered_map>
#include <unordered_set>

#include <BRep_CurveRepresentation.hxx>
#include <BRep_GCurve.hxx>
#include <BRep_PointRepresentation.hxx>
#include <BRep_TEdge.hxx>
#include <BRep_TFace.hxx>
#include <BRep_TVertex.hxx>
#include <Geom2d_Curve.hxx>
#include <Geom_Curve.hxx>
#include <Geom_Surface.hxx>
#include <Poly_Triangulation.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS_Iterator.hxx>
#include <TopoDS_TShape.hxx>
#include <gp_Trsf.hxx>

#include "mitcad/geometry/shape.hpp"

namespace mitcad::geometry {
namespace {

bool environment_asks() {
#ifdef _MSC_VER
  char* value = nullptr;
  std::size_t length = 0;
  if (_dupenv_s(&value, &length, "MITCAD_CHECK_INPUTS") != 0 || value == nullptr) {
    return false;
  }
  const bool on = value[0] != '\0' && std::strcmp(value, "0") != 0;
  std::free(value);
  return on;
#else
  const char* value = std::getenv("MITCAD_CHECK_INPUTS");
  return value != nullptr && value[0] != '\0' && std::strcmp(value, "0") != 0;
#endif
}

// Order-dependent 64-bit hashing (splitmix64 steps).
class Hash {
public:
  void add(std::uint64_t value) {
    std::uint64_t z = m_value ^ (value + 0x9e3779b97f4a7c15ULL);
    z = (z ^ (z >> 30)) * 0xbf58476d1ce4e5b9ULL;
    z = (z ^ (z >> 27)) * 0x94d049bb133111ebULL;
    m_value = z ^ (z >> 31);
  }
  void add(double value) {
    std::uint64_t bits = 0;
    std::memcpy(&bits, &value, sizeof bits);
    add(bits);
  }
  void add(bool value) { add(static_cast<std::uint64_t>(value ? 1 : 0)); }
  void add(int value) { add(static_cast<std::uint64_t>(static_cast<std::int64_t>(value))); }
  void add(const void* pointer) {
    add(static_cast<std::uint64_t>(reinterpret_cast<std::uintptr_t>(pointer)));
  }
  void add(const TopLoc_Location& location) {
    add(location.IsIdentity());
    if (!location.IsIdentity()) {
      const gp_Trsf& trsf = location.Transformation();
      for (int row = 1; row <= 3; ++row) {
        for (int column = 1; column <= 4; ++column) {
          add(trsf.Value(row, column));
        }
      }
    }
  }
  std::uint64_t value() const { return m_value; }

private:
  std::uint64_t m_value = 0x6a09e667f3bcc908ULL;
};

class Fingerprinter {
public:
  BrepFingerprint run(const TopoDS_Shape& shape) {
    m_print = BrepFingerprint{};
    if (!shape.IsNull()) {
      m_topology.add(static_cast<int>(shape.Orientation()));
      m_topology.add(shape.Location());
      visit(shape);
    }
    m_print.shapes = static_cast<int>(m_index.size());
    m_print.surfaces = static_cast<int>(m_surfaces.size());
    m_print.topology = m_topology.value();
    m_print.geometry = m_geometry.value();
    m_print.tolerances = m_tolerances.value();
    m_print.flags = m_flags.value();
    m_print.mesh = m_mesh.value();
    m_print.bookkeeping = m_bookkeeping.value();
    return m_print;
  }

private:
  // The sub-shape's number in the order of the first visit.
  int visit(const TopoDS_Shape& shape) {
    const TopoDS_TShape* tshape = shape.TShape().get();
    const auto found = m_index.find(tshape);
    if (found != m_index.end()) {
      return found->second;
    }
    const int number = static_cast<int>(m_index.size());
    m_index.emplace(tshape, number);
    m_topology.add(number);
    m_topology.add(static_cast<int>(shape.ShapeType()));
    flag(0, tshape->Free());
    flag(1, tshape->Modified());
    flag(2, tshape->Checked());
    flag(3, tshape->Orientable());
    flag(4, tshape->Closed());
    flag(5, tshape->Infinite());
    flag(6, tshape->Convex());
    flag(7, tshape->Locked());
    switch (shape.ShapeType()) {
    case TopAbs_FACE:
      face(*static_cast<const BRep_TFace*>(tshape));
      break;
    case TopAbs_EDGE:
      edge(*static_cast<const BRep_TEdge*>(tshape));
      break;
    case TopAbs_VERTEX:
      vertex(*static_cast<const BRep_TVertex*>(tshape));
      break;
    default:
      break;
    }
    int children = 0;
    for (TopoDS_Iterator it(shape, false, false); it.More(); it.Next(), ++children) {
      const int child = visit(it.Value());
      m_topology.add(child);
      m_topology.add(static_cast<int>(it.Value().Orientation()));
      m_topology.add(it.Value().Location());
    }
    m_topology.add(children);
    return number;
  }

  void flag(int index, bool set) {
    (index < 3 ? m_bookkeeping : m_flags).add(set);
    if (set) {
      ++m_print.flagged[static_cast<std::size_t>(index)];
    }
  }

  void surface(const occ::handle<Geom_Surface>& surface) {
    m_geometry.add(surface.get());
    if (!surface.IsNull()) {
      m_surfaces.insert(surface.get());
    }
  }

  void face(const BRep_TFace& face) {
    surface(face.Surface());
    m_geometry.add(face.Location());
    m_geometry.add(face.NaturalRestriction());
    m_tolerances.add(face.Tolerance());
    m_mesh.add(face.NbTriangulations());
    for (const occ::handle<Poly_Triangulation>& triangulation : face.Triangulations()) {
      m_mesh.add(triangulation.get());
    }
    m_mesh.add(face.ActiveTriangulation().get());
    m_print.meshes += face.NbTriangulations();
  }

  void edge(const BRep_TEdge& edge) {
    m_tolerances.add(edge.Tolerance());
    flag(8, edge.SameParameter());
    flag(9, edge.SameRange());
    flag(10, edge.Degenerated());
    int count = 0;
    for (const occ::handle<BRep_CurveRepresentation>& curve : edge.Curves()) {
      ++count;
      const BRep_CurveRepresentation& c = *curve;
      if (c.IsPolygon3D() || c.IsPolygonOnTriangulation() || c.IsPolygonOnSurface()) {
        ++m_print.meshes;
        m_mesh.add(curve.get());
        m_mesh.add(c.Location());
        continue;
      }
      m_geometry.add(curve.get());
      m_geometry.add(c.Location());
      if (const auto* g = dynamic_cast<const BRep_GCurve*>(&c)) {
        m_geometry.add(g->First());
        m_geometry.add(g->Last());
      }
      if (c.IsCurve3D()) {
        m_geometry.add(c.Curve3D().get());
        if (!c.Curve3D().IsNull()) {
          ++m_print.curves;
        }
      } else if (c.IsCurveOnSurface()) {
        surface(c.Surface());
        m_geometry.add(c.PCurve().get());
        ++m_print.pcurves;
        if (c.IsCurveOnClosedSurface()) {
          m_geometry.add(c.PCurve2().get());
          ++m_print.pcurves;
        }
      } else if (c.IsRegularity()) {
        surface(c.Surface());
        surface(c.Surface2());
        m_geometry.add(c.Location2());
        m_geometry.add(static_cast<int>(c.Continuity()));
        ++m_print.continuities;
      }
    }
    m_geometry.add(count);
  }

  void vertex(const BRep_TVertex& vertex) {
    m_tolerances.add(vertex.Tolerance());
    const gp_Pnt& p = vertex.Pnt();
    m_geometry.add(p.X());
    m_geometry.add(p.Y());
    m_geometry.add(p.Z());
    int count = 0;
    for (const occ::handle<BRep_PointRepresentation>& point : vertex.Points()) {
      ++count;
      m_geometry.add(point.get());
      m_geometry.add(point->Parameter());
      m_geometry.add(point->Location());
    }
    m_geometry.add(count);
  }

  BrepFingerprint m_print;
  std::unordered_map<const TopoDS_TShape*, int> m_index;
  std::unordered_set<const Geom_Surface*> m_surfaces;
  Hash m_topology;
  Hash m_geometry;
  Hash m_tolerances;
  Hash m_flags;
  Hash m_mesh;
  Hash m_bookkeeping;
};

// The check mode's state: the shapes made while it was on, with the
// fingerprint each had when an operation last finished with it.
struct Known {
  TopoDS_Shape occt;
  bool fingerprinted = false;
  BrepFingerprint print;
};

struct Registry {
  std::mutex mutex;
  std::unordered_map<const Shape*, Known> shapes;
  std::vector<std::string> changes;
  std::size_t operations = 0;
  std::map<std::string, std::size_t> by_operation;
};

// What the run checked, printed at exit.
struct Summary {
  Registry& registry;

  ~Summary() {
    const std::lock_guard<std::mutex> lock(registry.mutex);
    if (registry.operations == 0) {
      return;
    }
    std::string text;
    for (const auto& [operation, count] : registry.by_operation) {
      text += (text.empty() ? "" : ", ") + operation + " " + std::to_string(count);
    }
    std::fprintf(stderr, "MITCAD_CHECK_INPUTS: %zu operations checked; %s\n", registry.operations,
                 text.empty() ? "no input changed" : ("inputs changed: " + text).c_str());
  }
};

// Never destroyed: shapes held by other static objects may go after it.
Registry& registry() {
  static Registry* const instance = new Registry;
  static const Summary summary{*instance};
  (void)summary;
  return *instance;
}

// One operation of this thread (the first frame stands for reads outside
// any): the shapes it read, in order, and the set of them.
struct Frame {
  const char* operation = nullptr;
  std::vector<const Shape*> read;
  std::unordered_set<const Shape*> seen;
};

thread_local std::vector<Frame> t_frames(1);
// The operation this thread finished last, for changes found outside.
thread_local const char* t_last = nullptr;

std::string describe(const Shape& shape) {
  std::string text = "a shape of " + std::to_string(shape.face_count()) + " faces";
  for (int i = 0; i < shape.face_count(); ++i) {
    if (!shape.face_names(i).empty()) {
      text += " (" + shape.face_names(i).front() + ", ...)";
      break;
    }
  }
  return text;
}

void report(Registry& r, const std::string& operation, const std::string& text) {
  ++r.by_operation[operation];
  if (r.changes.size() < 1000) {
    r.changes.push_back(text);
  }
  std::fprintf(stderr, "MITCAD_CHECK_INPUTS: %s\n", text.c_str());
}

// Compares a shape read by `operation` with what is known of it and keeps
// how it is now. `during` tells whether it may have changed while the
// operation ran or before it read it.
void compare(Registry& r, const Shape* shape, const char* operation, bool during) {
  const auto known = r.shapes.find(shape);
  if (known == r.shapes.end()) {
    return;
  }
  const BrepFingerprint now = fingerprint(known->second.occt);
  if (known->second.fingerprinted && !known->second.print.same_brep(now)) {
    const std::string name = operation != nullptr ? operation : "an unchecked operation";
    const std::string what = now.changes_since(known->second.print);
    if (during) {
      report(r, name, name + " changed an input, " + describe(*shape) + ": " + what);
    } else {
      report(r, "outside operations",
             describe(*shape) + " changed outside checked operations (after " +
                 (t_last != nullptr ? t_last : "none") + ", before " + name + "): " + what);
    }
  }
  known->second.fingerprinted = true;
  known->second.print = now;
}

// Shapes read outside operations are compared when the next one starts.
void flush_loose(const char* next) {
  Frame& loose = t_frames.front();
  if (loose.read.empty()) {
    return;
  }
  Registry& r = registry();
  const std::lock_guard<std::mutex> lock(r.mutex);
  for (const Shape* shape : loose.read) {
    compare(r, shape, next, false);
  }
  loose.read.clear();
  loose.seen.clear();
}

} // namespace

bool BrepFingerprint::same_brep(const BrepFingerprint& other) const {
  return shapes == other.shapes && surfaces == other.surfaces && curves == other.curves &&
         pcurves == other.pcurves && continuities == other.continuities &&
         meshes == other.meshes && topology == other.topology && geometry == other.geometry &&
         tolerances == other.tolerances && flags == other.flags && mesh == other.mesh &&
         std::equal(flagged.begin() + 3, flagged.end(), other.flagged.begin() + 3);
}

bool BrepFingerprint::operator==(const BrepFingerprint& other) const {
  return same_brep(other) && bookkeeping == other.bookkeeping && flagged == other.flagged;
}

std::string BrepFingerprint::changes_since(const BrepFingerprint& before) const {
  std::string text;
  const auto add = [&text](const std::string& part) { text += (text.empty() ? "" : ", ") + part; };
  const auto count = [&add](const char* what, int was, int is) {
    if (was != is) {
      add(std::string(what) + " " + std::to_string(was) + " -> " + std::to_string(is));
    }
  };
  count("sub-shapes", before.shapes, shapes);
  count("surfaces", before.surfaces, surfaces);
  count("3D curves", before.curves, curves);
  count("p-curves", before.pcurves, pcurves);
  count("continuities", before.continuities, continuities);
  count("meshes", before.meshes, meshes);
  if (before.topology != topology) {
    add("topology");
  }
  if (before.geometry != geometry) {
    add("geometry");
  }
  if (before.tolerances != tolerances) {
    add("tolerances");
  }
  if (before.flags != flags || before.bookkeeping != bookkeeping) {
    std::string which;
    for (std::size_t i = 0; i < flagged.size(); ++i) {
      if (before.flagged[i] != flagged[i]) {
        which += (which.empty() ? " (" : ", ") + std::string(kFlagNames[i]) + " " +
                 std::to_string(before.flagged[i]) + " -> " + std::to_string(flagged[i]);
      }
    }
    add("flags" + which + (which.empty() ? "" : ")"));
  }
  if (before.mesh != mesh) {
    add("mesh");
  }
  return text;
}

const char* const BrepFingerprint::kFlagNames[BrepFingerprint::kFlags] = {
    "free",   "modified", "checked", "orientable",     "closed",    "infinite",
    "convex", "locked",   "same parameter", "same range", "degenerated"};

BrepFingerprint fingerprint(const TopoDS_Shape& shape) { return Fingerprinter().run(shape); }

namespace detail {

std::atomic<bool> input_checks{environment_asks()};

void shape_made(const Shape& shape, const TopoDS_Shape& occt) {
  Registry& r = registry();
  const std::lock_guard<std::mutex> lock(r.mutex);
  Known& known = r.shapes[&shape];
  known = Known{};
  known.occt = occt;
}

void shape_gone(const Shape& shape) {
  Registry& r = registry();
  const std::lock_guard<std::mutex> lock(r.mutex);
  r.shapes.erase(&shape);
}

void shape_read(const Shape& shape) {
  Frame& frame = t_frames.back();
  if (frame.seen.count(&shape) != 0) {
    return;
  }
  Registry& r = registry();
  const std::lock_guard<std::mutex> lock(r.mutex);
  // Not yet made (its constructor reads it), or made with the check off.
  if (r.shapes.count(&shape) == 0) {
    return;
  }
  frame.seen.insert(&shape);
  // Changed since an operation last finished with it?
  compare(r, &shape, frame.operation, false);
  frame.read.push_back(&shape);
}

} // namespace detail

void enable_input_checks(bool enable) { detail::input_checks.store(enable); }

bool input_checks_enabled() { return detail::input_checks.load(std::memory_order_relaxed); }

std::vector<std::string> input_changes() {
  Registry& r = registry();
  const std::lock_guard<std::mutex> lock(r.mutex);
  return r.changes;
}

void clear_input_changes() {
  Registry& r = registry();
  const std::lock_guard<std::mutex> lock(r.mutex);
  r.changes.clear();
  r.by_operation.clear();
}

CheckedOperation::CheckedOperation(const char* operation) : m_active(input_checks_enabled()) {
  if (!m_active) {
    return;
  }
  flush_loose(operation);
  Frame frame;
  frame.operation = operation;
  t_frames.push_back(std::move(frame));
}

CheckedOperation::~CheckedOperation() {
  if (!m_active || t_frames.size() < 2) {
    return;
  }
  Frame frame = std::move(t_frames.back());
  t_frames.pop_back();
  try {
    Registry& r = registry();
    const std::lock_guard<std::mutex> lock(r.mutex);
    ++r.operations;
    for (const Shape* shape : frame.read) {
      compare(r, shape, frame.operation, true);
    }
  } catch (...) {
    // A diagnostic; it never stops the operation's own result or error.
  }
  t_last = frame.operation;
  // An enclosing operation compares them again when it ends.
  if (t_frames.size() > 1) {
    Frame& parent = t_frames.back();
    for (const Shape* shape : frame.read) {
      if (parent.seen.insert(shape).second) {
        parent.read.push_back(shape);
      }
    }
  }
}

} // namespace mitcad::geometry
