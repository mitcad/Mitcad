// SPDX-License-Identifier: MIT
#pragma once

// Interference: the volumes where bodies overlap.

#include <cstddef>
#include <vector>

#include <Message_ProgressRange.hxx>

#include "mitcad/analysis/common.hpp"

namespace mitcad::analysis {

struct InterferenceOptions {
  // Overlaps smaller than this (mm^3) are not reported; bodies that only
  // touch have zero overlap.
  double min_volume = 1e-6;
  // Fuzzy tolerance of the boolean operation, mm (0: exact).
  double fuzzy = 0.0;
  // Keep the common solid of each pair (for display).
  bool keep_shapes = true;
};

// One interfering pair.
struct Interference {
  std::size_t first = 0; // indices into the body list, first < second
  std::size_t second = 0;
  double volume = 0.0;   // mm^3
  TopoDS_Shape common;   // the overlap, when kept
};

// Every pair of bodies whose solids overlap by more than `min_volume`.
// Pairs with disjoint bounding boxes are skipped without a boolean.
// Throws Error when the boolean operation of a pair fails, and when
// `range`'s user break stops the check (a cancelled model job, mitcad#2540:
// it is asked before each pair and inside each pair's boolean).
std::vector<Interference> interferences(const std::vector<TopoDS_Shape>& bodies,
                                        const InterferenceOptions& options = {},
                                        const Message_ProgressRange& range = Message_ProgressRange());

} // namespace mitcad::analysis
