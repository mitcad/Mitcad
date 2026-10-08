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
  // How long the comparison may take, s (0: no limit). The boolean
  // differences stop when it is over (they are then missing) and so does
  // the sampling (fewer samples): booleans of nearly coincident shapes can
  // take many minutes.
  double seconds = 0.0;
  // Leave out the boolean differences when every point was sampled and
  // none lies farther than this from the other shape, mm (0: never). Of
  // nearly coincident shapes they are slow and say little more.
  double booleans_above = 0.0;
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
  // Volumes of A - B and B - A; empty when the boolean operation failed,
  // did not finish in time, was left out (CompareOptions::booleans_above)
  // or gave a result the sampled deviations rule out (more volume between
  // the shapes than their surfaces enclose within that distance of each
  // other: on nearly coincident shapes the booleans can take them as
  // apart).
  std::optional<double> a_minus_b;
  std::optional<double> b_minus_a;
  // (|A - B| + |B - A|) / max(|A|, |B|); empty when either difference is.
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

// Whether boolean differences of `differences` mm^3 in all (A - B and
// B - A) can be right when every sample of both shapes lies within
// `max_deviation` mm of the other shape, whose surfaces have `area` mm^2
// in all: the material one shape lacks lies between the surfaces, so
// within about that distance of them; four times that volume is allowed.
bool plausible_differences(double differences, double max_deviation, double area, double fuzzy);

} // namespace mitcad::analysis
