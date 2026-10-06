// SPDX-License-Identifier: MIT
#pragma once

// Comparison of two shapes in the same coordinate system, for example
// Mitcad's replay of an .f3d design against a STEP export of it. The
// shapes may differ in topology (face splits, seams) and orientation.

#include <cstddef>
#include <optional>

#include "mitcad/analysis/common.hpp"

namespace mitcad::analysis {

struct CompareOptions {
  // Points sampled on the faces of each shape (spread by area), plus the
  // vertices and edge midpoints.
  std::size_t samples = 2000;
  // Fuzzy tolerance of the boolean differences, mm. It lets coincident
  // faces of slightly different shapes match.
  double fuzzy = 1e-4;
  // Also sample B against A, so that material only B has is seen.
  bool symmetric = true;
};

// Distances of sample points of one shape from the surface of the other.
struct Deviation {
  double max = 0.0; // mm
  double rms = 0.0; // mm
  Vec3 at;          // the sample point with the largest distance
  std::size_t samples = 0;
};

struct Comparison {
  double volume_a = 0.0; // mm^3
  double volume_b = 0.0;
  // Volumes of A - B and B - A; empty when the boolean operation failed.
  std::optional<double> a_minus_b;
  std::optional<double> b_minus_a;
  // (|A - B| + |B - A|) / max(|A|, |B|); empty when a boolean failed.
  std::optional<double> relative_difference;
  Deviation a_to_b;
  Deviation b_to_a; // samples == 0 unless symmetric
  double max_deviation = 0.0; // the larger of the two
  Bounds bounds_a;
  Bounds bounds_b;
  // Largest difference of the bounding box corner coordinates, mm.
  double bounds_difference = 0.0;
};

Comparison compare(const TopoDS_Shape& a, const TopoDS_Shape& b,
                   const CompareOptions& options = {});

} // namespace mitcad::analysis
