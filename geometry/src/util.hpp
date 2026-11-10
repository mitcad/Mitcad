// SPDX-License-Identifier: MIT
#pragma once

// Helpers shared by the geometry sources; not part of the public headers.

#include <array>
#include <cstddef>
#include <memory>
#include <new>
#include <optional>
#include <stdexcept>
#include <string>
#include <type_traits>
#include <vector>

#include <Message_ProgressIndicator.hxx>
#include <Message_ProgressRange.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <Standard_OutOfMemory.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Shape.hxx>
#include <gp_Pnt.hxx>

#include "mitcad/geometry/cancel.hpp"
#include "mitcad/geometry/guard.hpp"
#include "mitcad/geometry/input_check.hpp"
#include "mitcad/geometry/shape.hpp"
// Errors a person reads, with their places (mitcad#1612).
#include "mitcad/geometry/user_error.hpp"

namespace mitcad::geometry::detail {

void require_positive(const char* what, double value);
void require_finite(const char* what, double value);

// Whether the shape has a face without a surface: a mesh body's triangles
// (STL, OBJ, the meshes of imported models; import.hpp's BodyKind::Mesh).
bool has_mesh_faces(const TopoDS_Shape& shape);

// Throws "<operation>: a mesh body (faces without surfaces) cannot be
// used; ..." when the shape has such a face (mitcad#399). OCCT's booleans,
// distances, fillets and face operations read the surface of every face
// they get and fail on a null one, inside the boolean's face intersection
// with an access violation on Windows (mitcad#392). The model refuses mesh
// bodies by name before it calls the kernel; this keeps OCCT from getting
// one from any other caller.
void require_surfaces(const TopoDS_Shape& shape, const char* operation);

// For tests of OCCT's crash handlers (guard.hpp): crashes as an access
// violation inside OCCT would when the environment variable
// MITCAD_TEST_OCCT_CRASH names this operation ("fillet").
void crash_if_asked(const char* operation);

// For tests of allocations that fail (mitcad#80): throws std::bad_alloc
// when the environment variable MITCAD_TEST_OCCT_OUT_OF_MEMORY names this
// operation ("extrude").
void fail_allocation_if_asked(const char* operation);

// For tests of allocations that fail inside OCCT (mitcad#132): when the
// environment variable MITCAD_TEST_OCCT_ALLOCATION_FAILS names this
// operation ("heal"), an allocation of OCCT's collections fails, and the
// failure is caught as OCCT's checker catches its own.
void fail_occt_allocation_if_asked(const char* operation);

// Throws std::bad_alloc when an allocation inside OCCT has failed under
// this thread's calls since failed_allocations_in_thread() (guard.hpp) was
// `before`.
void throw_if_allocation_failed(std::size_t before);

// An operation that catches OCCT's exceptions runs on this thread while it
// lives: on Windows OCCT's handlers turn a fault into an exception only
// then (guard.cpp; elsewhere OCC_CATCH_SIGNALS says so).
class OperationScope {
public:
  OperationScope();
  ~OperationScope();
  OperationScope(const OperationScope&) = delete;
  OperationScope& operator=(const OperationScope&) = delete;
};

// Calls `call(work)` as an operation's work (mitcad#1429). On Windows it is
// also the point the thread comes back to from a crash that OCCT's handler
// turned into an exception but that the stack cannot unwind to the
// operation: a crash in a destructor, whose exception would end the
// process (std::terminate; a destructor is noexcept, also where it is
// inlined). The thread then comes back here without unwinding the frames
// between, as OCCT's handler jumps back to OCC_CATCH_SIGNALS on Linux and
// macOS, and this throws the crash's exception (a Standard_Failure with
// OCCT's message) instead. Elsewhere it only calls.
void call_recoverable(void (*call)(void*), void* work);

template <class Work>
void recoverable(Work& work) {
  call_recoverable([](void* data) { (*static_cast<Work*>(data))(); }, &work);
}

// OCCT reports errors as Standard_Failure; add context for the user. With
// OCCT's signal handlers installed (catch_occt_crashes), a crash inside the
// operation is one too: OCC_CATCH_SIGNALS gives the handlers somewhere to
// return to where OCCT has no handler of its own (on Linux; on Windows the
// handlers throw, and where that exception cannot unwind to here the
// thread comes back to call_recoverable). An operation asked to stop does not start, and one that
// fails after the request was cancelled (cancel.hpp), unless an allocation
// failed in it (the import stops on that). An allocation that
// fails (OCCT's Standard_OutOfMemory, std::bad_alloc) is an error
// "<operation>: out of memory" (mitcad#80; the model's
// KernelError::OUT_OF_MEMORY): the operation's memory is freed as it
// unwinds. So is one OCCT caught inside the operation (mitcad#132, its
// checker reports the shape invalid instead), also in the jobs of OCCT's
// thread pool the operation ran (guard.hpp's
// failed_allocations_in_thread). With the input check on, the shapes it
// reads are compared before and after (input_check.hpp).
template <class Operation>
auto run(const char* operation, Operation&& op) -> decltype(op()) {
  throw_if_cancelled();
  const CheckedOperation check(operation);
  const OperationScope scope;
  const std::size_t failed = failed_allocations_in_thread();
  using Result = decltype(op());
  try {
    OCC_CATCH_SIGNALS
    const auto start = [operation] {
      crash_if_asked(operation);
      fail_allocation_if_asked(operation);
      fail_occt_allocation_if_asked(operation);
    };
    if constexpr (std::is_void_v<Result>) {
      auto work = [&] {
        start();
        op();
      };
      recoverable(work);
      throw_if_allocation_failed(failed);
    } else if constexpr (std::is_reference_v<Result>) {
      std::add_pointer_t<std::remove_reference_t<Result>> result = nullptr;
      auto work = [&] {
        start();
        result = std::addressof(op());
      };
      recoverable(work);
      throw_if_allocation_failed(failed);
      return static_cast<Result>(*result);
    } else {
      std::optional<Result> result;
      auto work = [&] {
        start();
        result.emplace(op());
      };
      recoverable(work);
      throw_if_allocation_failed(failed);
      return std::move(*result);
    }
  } catch (const Cancelled&) {
    // Stopped after an allocation it caught inside failed: the import's
    // memory guard saw the count and asked it to stop (mitcad#132). The
    // operation ran out of memory.
    if (failed_allocations_in_thread() != failed) {
      throw std::runtime_error(std::string(operation) + ": out of memory");
    }
    throw;
  } catch (const Standard_OutOfMemory&) {
    throw std::runtime_error(std::string(operation) + ": out of memory");
  } catch (const std::bad_alloc&) {
    throw std::runtime_error(std::string(operation) + ": out of memory");
  } catch (const Standard_Failure& failure) {
    if (failed_allocations_in_thread() != failed) {
      // (A failure OCCT made of the allocation it caught, also one that
      // stopped as asked after it.)
      throw std::runtime_error(std::string(operation) + ": out of memory");
    }
    throw_if_cancelled();
    throw std::runtime_error(std::string(operation) + ": " + failure.what());
  }
}

// A progress range for one run of an OCCT algorithm, whose user break is
// this thread's cancel request (cancel.hpp); without a CancelScope an empty
// range, which stops nothing. It must live while the algorithm runs.
class Interrupt {
public:
  Interrupt();
  Interrupt(const Interrupt&) = delete;
  Interrupt& operator=(const Interrupt&) = delete;

  // The range to run the algorithm in; one per Interrupt.
  Message_ProgressRange range();

private:
  occ::handle<Message_ProgressIndicator> m_indicator;
};

// Runs `step` with a range that stops it on a cancel request, then throws
// Cancelled if one came: an algorithm stopped early has not finished, and
// what it made goes.
template <class Step>
void interruptible(Step&& step) {
  Interrupt interrupt;
  step(interrupt.range());
  throw_if_cancelled();
}

// Builds an OCCT algorithm (Build(range) of the shape builders and the
// booleans) so that a cancel request stops it.
template <class Algorithm>
void build(Algorithm& algorithm) {
  interruptible([&](const Message_ProgressRange& range) { algorithm.Build(range); });
}

// Sort key of a shape's position, used wherever several pieces need a
// deterministic geometric order: the centre of mass (of a solid, face or
// edge), then the lower corner of the bounding box, rounded to 1e-6 mm so
// that equal positions compare equal.
using ShapeKey = std::array<long long, 6>;
ShapeKey shape_key(const TopoDS_Shape& shape);

// A point in the middle of an edge (by parameter), as a sort key.
ShapeKey edge_key(const TopoDS_Shape& edge);

// How far apart (mm, and mm^2 or mm^3 for a piece's size) two pieces'
// places must lie for piece_order to tell them apart by that component:
// far above the noise of the booleans (nanometres, differing between
// platforms and upstream rounding), far below the distance between two
// pieces of one face.
constexpr double kPieceTie = 1.0e-4;

// The order of the pieces that share a name: the "#k" FaceNamer::finish
// gives them, and the order Shape::find_faces lists the faces a reference
// matches (mitcad#895). The positions in `pieces`, ordered by shape_key's
// components (the centre of mass in X, Y and Z, then the lower corner of
// the bounding box), then by the piece's size (area, volume or length)
// and the upper corner of its box, each component telling two pieces
// apart only where they differ by more than kPieceTie: sorted by the
// component, the pieces fall into runs whose neighbours lie within
// kPieceTie of each other, and each run is ordered by the next component.
// So pieces lying symmetrically (their centres alike in X, differing in Y)
// are not ordered by the nanometres of rounding noise in X. Pieces alike
// in every component keep shape_key's order, then their order in
// `pieces`. Where the components that decide differ by more than
// kPieceTie, the order is shape_key's, as before mitcad#895, so the pieces
// of existing designs keep their numbers.
std::vector<std::size_t> piece_order(const std::vector<TopoDS_Shape>& pieces);

// The order of the edges, or of the vertices, that share a name: the "#k"
// Shape gives them and the order find_edges and find_vertices list the
// ones a reference matches (mitcad#896). As piece_order, by the
// components edge_key and shape_key round (an edge's middle, then its
// centre of mass; a vertex's point, then the lower corner of its box),
// each telling two apart only where they differ by more than kPieceTie,
// then an edge's length and its box. So symmetric edges between the same
// two faces are not ordered by the nanometres of noise in their middles.
// Ones alike in every component keep the order of the rounded key, then
// their order in the list; where the deciding components differ by more
// than kPieceTie the order is the rounded key's, as before mitcad#896, so
// existing designs keep their references.
std::vector<std::size_t> edge_order(const std::vector<TopoDS_Shape>& edges);
std::vector<std::size_t> vertex_order(const std::vector<TopoDS_Shape>& vertices);

// Throws when OCCT's checker finds the shape invalid. The checker cannot be
// stopped: asked to stop, this throws Cancelled before it starts.
void require_valid(const TopoDS_Shape& shape, const char* operation);
// Whether OCCT's checker finds the shape valid (Cancelled as require_valid).
bool is_valid(const TopoDS_Shape& shape);

// is_valid and require_valid for the result of an operation whose inputs
// OCCT's checker found valid, where `kept` holds the inputs' faces (as the
// inputs hold them, with their orientations) that the result may share.
// A face the result shares, and every edge and vertex that only such faces
// use, is as it was checked; the checker sees the other faces and the faces
// that meet them at a vertex, so every edge and vertex any of those use in
// all its faces. That each shell is connected, holds no face twice and has
// one face on each side of each edge running along it in opposite
// directions is counted over the whole result. A solid of several shells
// (a body with a void) is also checked as a solid (how its shells lie in
// each other, BRepCheck_Solid), as the whole check does (mitcad#855).
// Where the result is not solids of shells, or anything of that does not
// hold, or most faces changed, the whole shape is checked as before.
// Mitcad's operations do not modify their inputs (input_check.hpp), and
// OCCT's face merge changes only what the faces it merges use, so the
// answer is the checker's on the whole shape (mitcad#121: on bodies of
// 5 000 faces the whole check took seconds after every join and cut near a
// few of them).
// What a check saw, for traces: the faces it gave the checker (0: the whole
// shape), why it checked the whole shape (null when it did not), and how
// many solids of several shells it checked as solids.
struct WhereChanged {
  std::size_t faces = 0;
  const char* whole = nullptr;
  std::size_t solids = 0;
};
bool is_valid_where_changed(const TopoDS_Shape& result, const ShapeMap& kept, WhereChanged* report = nullptr);
void require_valid_where_changed(const TopoDS_Shape& result, const ShapeMap& kept, const char* operation,
                                 WhereChanged* report = nullptr);
// How many checks so far (in the process) saw only the faces that changed,
// and how many faces they gave the checker (for tests and traces).
struct ChecksWhereChanged {
  std::size_t checks = 0;
  std::size_t faces = 0;
};
ChecksWhereChanged checks_where_changed();

// The vertices of a shape whose tolerance is inflated: above 1 mm and ten
// times what the vertex's edges need (how far the vertex lies from their
// curves' and p-curves' ends, or the edges' own tolerances) (mitcad#235: a
// rounding left a corner's vertex a tolerance of 481 mm where its edge ends
// 0.12 mm from it, and OCCT's booleans then took the vertex for touching
// most of the body). With `tighten`, each such tolerance is set to what its
// edges need, in place: only on a shape of one's own (a working copy).
int inflated_vertices(const TopoDS_Shape& shape, bool tighten);

// The pieces of the input's edges in a dressup's result (new edges on an
// input edge's 3D curve, which OCCT's build shares with it) whose curves on
// their faces lie off the 3D curve beyond their tolerance, but no farther
// than the input edge's own curves lie off it: the input's edge was already
// off its tolerance, which OCCT's checker, sampling the whole edge at other
// points, did not see (mitcad#343: a stored body's edge whose curve on a
// cylinder lies 1e-5 beyond its tolerance of 2.4e-4, cut back by a rounding
// at its end). Their tolerances are widened to their deviation, in place:
// only the result's own edges, not the input's (dressup.cpp). The number
// widened.
int widen_inherited_curve_tolerances(const TopoDS_Shape& result, const TopoDS_Shape& input);

// The volume of a shape by OCCT's BRepGProp, negative for a solid turned
// inside out. (brep_import.cpp's analysed_volume_of integrates with the
// analysis library instead, so that the import's reports agree with the
// queries.)
double volume_of(const TopoDS_Shape& shape);

// A compound of the shapes, also of one.
TopoDS_Compound compound_of(const std::vector<TopoDS_Shape>& shapes);
// A compound of the shapes, or the shape itself when there is one.
TopoDS_Shape compound_or_single(const std::vector<TopoDS_Shape>& shapes);

} // namespace mitcad::geometry::detail
