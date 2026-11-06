// SPDX-License-Identifier: MIT
#include "RibbonFit.hpp"

#include <cstddef>

namespace mitcad {

RibbonFit fitRibbonGroups(const std::vector<RibbonFitGroup>& groups, int fixed, int available) {
  const std::size_t count = groups.size();
  RibbonFit fit;
  fit.collapsed.assign(count, false);
  fit.shown.assign(count, 0);
  std::vector<int> widths(count, 0); // the groups' widths as they fold
  int total = fixed;
  for (std::size_t i = 0; i < count; ++i) {
    const RibbonFitGroup& group = groups[i];
    if (group.collapsible && !group.widths.empty()) {
      fit.shown[i] = static_cast<int>(group.widths.size()) - 1;
      widths[i] = group.widths.back();
      total += widths[i];
    }
  }
  for (std::size_t i = count; i-- > 0 && total > available;) {
    const RibbonFitGroup& group = groups[i];
    if (!group.collapsible || group.widths.empty()) {
      continue;
    }
    // A pin folds only when the group gets narrower without it (not where
    // the buttons fit under the group's name).
    int& shown = fit.shown[i];
    while (total > available && shown > 0 && group.widths[static_cast<std::size_t>(shown - 1)] < widths[i]) {
      --shown;
      const int narrower = group.widths[static_cast<std::size_t>(shown)];
      total -= widths[i] - narrower;
      widths[i] = narrower;
    }
    if (shown == 0 && group.ownButtons == 0) {
      // Nothing of its own left in the row: its name alone.
      total -= widths[i] - group.collapsedWidth;
      fit.collapsed[i] = true;
    } else if (total > available && widths[i] > group.collapsedWidth) {
      total -= widths[i] - group.collapsedWidth;
      fit.collapsed[i] = true;
    }
  }
  return fit;
}

} // namespace mitcad
