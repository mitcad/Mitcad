// SPDX-License-Identifier: MIT
// How a ribbon page's groups fit a narrow window, the rule the Qt ribbon
// (Ribbon::fitPage) and the AppKit ribbon (layoutRibbon) share
// (mitcad#1111, mitcad#1357): from the right; in a group its pinned
// buttons fold first, the last one first, and the group collapses only
// when its own buttons do not fit either; a group of pins only collapses
// once they have all folded; a group whose buttons fit under its name
// does not collapse (mitcad#696).
#include "framework/RibbonFit.hpp"

#include <cstdio>
#include <utility>
#include <vector>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "ribbon_fit.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::RibbonFit;
using mitcad::RibbonFitGroup;
using mitcad::fitRibbonGroups;

RibbonFitGroup group(int ownButtons, int collapsedWidth, std::vector<int> widths) {
  RibbonFitGroup made;
  made.collapsible = true;
  made.ownButtons = ownButtons;
  made.collapsedWidth = collapsedWidth;
  made.widths = std::move(widths);
  return made;
}

// CREATE: four buttons of its own (176) and a pinned Sweep (+40); MODIFY
// on its right without pins. The margins and the gap: 32.
void pinFoldsBeforeGroupTests() {
  const std::vector<RibbonFitGroup> groups = {group(4, 70, {176, 216}), group(6, 80, {150})};
  constexpr int fixed = 32;
  // Wide: everything shown.
  RibbonFit fit = fitRibbonGroups(groups, fixed, 400);
  CHECK(!fit.collapsed[0] && !fit.collapsed[1]);
  CHECK(fit.shown[0] == 1);
  // MODIFY collapses first, from the right, and Sweep stays.
  fit = fitRibbonGroups(groups, fixed, 360);
  CHECK(!fit.collapsed[0] && fit.collapsed[1]);
  CHECK(fit.shown[0] == 1);
  // Narrower: Sweep folds while CREATE keeps its own buttons.
  fit = fitRibbonGroups(groups, fixed, 320);
  CHECK(!fit.collapsed[0] && fit.collapsed[1]);
  CHECK(fit.shown[0] == 0);
  // Narrower still: CREATE's own buttons do not fit either.
  fit = fitRibbonGroups(groups, fixed, 250);
  CHECK(fit.collapsed[0] && fit.collapsed[1]);
  CHECK(fit.shown[0] == 0);
}

// Two pins: the last one folds first.
void lastPinFirstTests() {
  const std::vector<RibbonFitGroup> groups = {group(4, 70, {176, 216, 250})};
  RibbonFit fit = fitRibbonGroups(groups, 24, 274);
  CHECK(!fit.collapsed[0] && fit.shown[0] == 2);
  fit = fitRibbonGroups(groups, 24, 260);
  CHECK(!fit.collapsed[0] && fit.shown[0] == 1);
  fit = fitRibbonGroups(groups, 24, 220);
  CHECK(!fit.collapsed[0] && fit.shown[0] == 0);
}

// A group of pinned buttons only shows its name alone once they have all
// folded; a group whose buttons fit under its name stays as it is.
void pinsOnlyAndUnderNameTests() {
  const std::vector<RibbonFitGroup> groups = {group(0, 60, {60, 100}), group(1, 70, {70})};
  const RibbonFit fit = fitRibbonGroups(groups, 32, 100);
  CHECK(fit.collapsed[0] && fit.shown[0] == 0);
  CHECK(!fit.collapsed[1]);
  // A group without buttons is part of `fixed` and is left as it is.
  RibbonFitGroup plain;
  const RibbonFit none = fitRibbonGroups({plain}, 100, 50);
  CHECK(!none.collapsed[0] && none.shown[0] == 0);
}

} // namespace

int ribbonFitTests() {
  pinFoldsBeforeGroupTests();
  lastPinFirstTests();
  pinsOnlyAndUnderNameTests();
  return failures;
}
