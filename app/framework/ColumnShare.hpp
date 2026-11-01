// SPDX-License-Identifier: MIT
#pragma once

#include <vector>

namespace mitcad {

// A left card's claim on the column (mitcad#1020): the height of its
// content (`want`, header and margins included), at least its header
// (`floor`), its share's weight; a collapsed card (`fixed`) has its
// header's height and no share. The Floating layout (FloatingLayout) and
// the AppKit frontend's left column share the column this way.
struct ColumnClaim {
  int want = 0;
  int floor = 0;
  bool fixed = false;
  double weight = 1.0;
};

// The heights of the left cards in `available` pixels: the fixed ones
// first, then each card whose content is shorter than its weighted share
// of what is left keeps its content's height, again until none is; the
// others share the rest by their weights. None is lower than its floor.
std::vector<int> shareColumn(const std::vector<ColumnClaim>& claims, int available);

} // namespace mitcad
