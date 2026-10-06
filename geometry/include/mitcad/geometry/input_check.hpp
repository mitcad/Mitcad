// SPDX-License-Identifier: MIT
#pragma once

// Operations must leave their inputs as they are (T0e). Results are cached
// and share sub-shapes with each other, so an OCCT algorithm that writes
// into the sub-shapes of its input (p-curves, tolerances, surfaces) changes
// earlier results in place, and a feature computed after other operations
// ran on its inputs gets another B-rep than the same feature computed
// without them (an .f3d import, which tries many definitions, against an
// open of the design it saved). The algorithms that do so work on copies
// (geometry/src/history.hpp, InputCopy).
//
// A fingerprint of what a B-rep holds, and a check mode for finding the
// operations that change their inputs: with MITCAD_CHECK_INPUTS=1 in the
// environment (or after enable_input_checks), every operation of the
// library (a CheckedOperation; detail::run and the analysis bridge open
// one) fingerprints each Shape it reads (occt(), face(), edge(), vertex())
// when it first reads it and again when it ends, and reports what changed
// on standard error and in input_changes(); OCCT's bookkeeping flags alone
// are no change (same_brep). A shape read outside an operation is compared
// when the next operation starts, and so is one that changed through
// another Shape sharing its sub-shapes. At exit the process prints how many
// operations were checked and which changed inputs. Without the mode, a
// read costs one flag test. It is a diagnostic: the fingerprints take time
// on large bodies (an .f3d import about a third longer). The geometry tests
// run with it and want no change.

#include <array>
#include <atomic>
#include <cstdint>
#include <string>
#include <vector>

#include <TopoDS_Shape.hxx>

namespace mitcad::geometry {

class Shape;

// Counts and hashes of the data in a shape and its sub-shapes. The hashes
// include the identities of geometry handles, so fingerprints compare
// within one process only.
struct BrepFingerprint {
  int shapes = 0;       // distinct sub-shapes, the shape itself included
  int surfaces = 0;     // distinct surfaces of faces and of curves on surfaces
  int curves = 0;       // 3D curves of edges
  int pcurves = 0;      // curves on surfaces (two on a seam)
  int continuities = 0; // continuity records of edges between two faces
  int meshes = 0;       // triangulations of faces and polygons of edges
  std::uint64_t topology = 0;   // sub-shapes, their orientations and locations
  std::uint64_t geometry = 0;   // surfaces, curves, points, parameters, ranges
  std::uint64_t tolerances = 0; // of faces, edges and vertices
  std::uint64_t flags = 0;      // shape flags; same parameter, same range, degenerated
  std::uint64_t mesh = 0;       // triangulations and polygons
  std::uint64_t bookkeeping = 0; // the free, modified and checked flags
  // Sub-shapes with each flag set, in the order of kFlagNames: first the
  // bookkeeping flags OCCT's builders set on shapes they add or update
  // (free, modified, checked), then the others.
  static constexpr int kFlags = 11;
  static const char* const kFlagNames[kFlags];
  std::array<int, kFlags> flagged{};

  bool operator==(const BrepFingerprint& other) const;
  bool operator!=(const BrepFingerprint& other) const { return !(*this == other); }
  // The same B-rep data, whatever the bookkeeping flags: OCCT's builders
  // clear the free flag of the shapes they put into a new one and set the
  // others when they update one, without changing what it holds. The input
  // check compares this.
  bool same_brep(const BrepFingerprint& other) const;
  // What differs from `before` ("p-curves 104 -> 152, surfaces 25 -> 39,
  // geometry"); empty when nothing does.
  std::string changes_since(const BrepFingerprint& before) const;
};

BrepFingerprint fingerprint(const TopoDS_Shape& shape);

// Turns the check mode on or off for the process (on at start with
// MITCAD_CHECK_INPUTS set to anything but "0"). Shapes made while it is off
// are not checked.
void enable_input_checks(bool enable);
bool input_checks_enabled();

// The changes reported so far ("join changed an input: ..."), oldest first
// (the first thousand).
std::vector<std::string> input_changes();
void clear_input_changes();

// While it lives on a thread, the Shapes that thread reads belong to the
// operation it names; when it ends, they are compared with how they were
// when the operation first read them. Operations may nest: an inner one
// reports what changed while it ran.
class CheckedOperation {
public:
  explicit CheckedOperation(const char* operation);
  ~CheckedOperation();
  CheckedOperation(const CheckedOperation&) = delete;
  CheckedOperation& operator=(const CheckedOperation&) = delete;

private:
  bool m_active;
};

namespace detail {

extern std::atomic<bool> input_checks;

// Called by Shape: made (constructed or assigned), destroyed, read.
void shape_made(const Shape& shape, const TopoDS_Shape& occt);
void shape_gone(const Shape& shape);
void shape_read(const Shape& shape);

} // namespace detail
} // namespace mitcad::geometry
