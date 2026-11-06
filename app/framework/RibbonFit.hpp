// SPDX-License-Identifier: MIT
#pragma once

#include <vector>

namespace mitcad {

// How a ribbon page's groups fit a narrow window: the one rule of the Qt
// ribbon (Ribbon::fitPage) and the AppKit ribbon (NativeMacWindow's
// layoutRibbon, mitcad#1357). From the right, so that the tab's first
// groups (Create, Modify) stay in sight the longest, Create the longest.
// In each group the commands the user pinned fold first, the last one
// first (mitcad#1111: a pinned command never hides its group's own
// buttons); the group collapses to its name only when its own buttons do
// not fit either, and only when it gets narrower that way (a group whose
// buttons fit under its name is no wider with them, mitcad#696). A group
// of pinned buttons only collapses once they have all folded.
struct RibbonFitGroup {
  bool collapsible = false; // it has buttons (its own or pinned) to hide
  int ownButtons = 0;       // its buttons before the user's pins
  int collapsedWidth = 0;   // its name alone
  // Its width with its own buttons and its first k pinned buttons shown,
  // k = 0 .. its pins (the last one is its full width).
  std::vector<int> widths;
};

struct RibbonFit {
  std::vector<bool> collapsed; // per group: its name alone
  std::vector<int> shown;      // per group: its pinned buttons shown, from the left
};

// `fixed` is the width of what is not a collapsible group (the margins,
// the gaps, the groups without buttons), `available` the room.
RibbonFit fitRibbonGroups(const std::vector<RibbonFitGroup>& groups, int fixed, int available);

} // namespace mitcad
