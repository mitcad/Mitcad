// SPDX-License-Identifier: MIT
#pragma once

// Helpers shared by the geometry sources; not part of the public headers.

#include <array>
#include <stdexcept>
#include <string>

#include <Message_ProgressIndicator.hxx>
#include <Message_ProgressRange.hxx>
#include <Standard_ErrorHandler.hxx>
#include <Standard_Failure.hxx>
#include <TopoDS_Shape.hxx>
#include <gp_Pnt.hxx>

#include "mitcad/geometry/cancel.hpp"
#include "mitcad/geometry/input_check.hpp"

namespace mitcad::geometry::detail {

void require_positive(const char* what, double value);
void require_finite(const char* what, double value);

// For tests of OCCT's crash handlers (guard.hpp): crashes as an access
// violation inside OCCT would when the environment variable
// MITCAD_TEST_OCCT_CRASH names this operation ("fillet").
void crash_if_asked(const char* operation);

// OCCT reports errors as Standard_Failure; add context for the user. With
// OCCT's signal handlers installed (catch_occt_crashes), a crash inside the
// operation is one too: OCC_CATCH_SIGNALS gives the handlers somewhere to
// return to where OCCT has no handler of its own (on Linux; on Windows the
// handlers throw). An operation asked to stop does not start, and one that
// fails after the request was cancelled (cancel.hpp). With the input check
// on, the shapes it reads are compared before and after (input_check.hpp).
template <class Operation>
auto run(const char* operation, Operation&& op) -> decltype(op()) {
  throw_if_cancelled();
  const CheckedOperation check(operation);
  try {
    OCC_CATCH_SIGNALS
    crash_if_asked(operation);
    return op();
  } catch (const Standard_Failure& failure) {
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

// Throws when OCCT's checker finds the shape invalid. The checker cannot be
// stopped: asked to stop, this throws Cancelled before it starts.
void require_valid(const TopoDS_Shape& shape, const char* operation);
// Whether OCCT's checker finds the shape valid (Cancelled as require_valid).
bool is_valid(const TopoDS_Shape& shape);

} // namespace mitcad::geometry::detail
