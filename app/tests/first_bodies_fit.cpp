// SPDX-License-Identifier: MIT
// When the view fits the bodies coming into it (mitcad#467): a design's
// first bodies once, not a command's previews, and never the bodies shown
// again after the user hid every one (the bundle check of mitcad#529/#530).
#include "UnitTests.hpp"
#include "view/FirstBodiesFit.hpp"

#include <cstdio>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "first_bodies_fit.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::FirstBodiesFit;

void newDesignTests() {
  // A new design starts empty; a command's preview comes and goes; its
  // first body is fitted, once.
  FirstBodiesFit fit;
  fit.arm();
  CHECK(!fit.shown(true, false)); // nothing, or only a preview
  CHECK(fit.armed());
  CHECK(fit.shown(true, true)); // the first body
  CHECK(!fit.armed());
  CHECK(!fit.shown(false, true)); // more bodies
  // Every body hidden, then one shown again: the view stays.
  CHECK(!fit.shown(true, false));
  CHECK(!fit.shown(true, true));
  CHECK(!fit.shown(true, true));
}

void openedDesignTests() {
  // A design opened while the view shows the last one's bodies: the window
  // fits it itself; the fit of its first bodies is done.
  FirstBodiesFit fit;
  fit.arm();
  CHECK(!fit.shown(false, true));
  CHECK(!fit.armed());
  CHECK(!fit.shown(true, true));
  // A design opened into an empty view: fitted; then another design, armed
  // again: its first bodies too.
  fit.arm();
  CHECK(fit.shown(true, true));
  fit.arm();
  CHECK(!fit.shown(true, false));
  CHECK(fit.shown(true, true));
  // Without arm(), from the start: armed (the window's first design).
  FirstBodiesFit fresh;
  CHECK(fresh.armed());
}

void homeViewTests() {
  // A design opened at its saved home view (mitcad#2671): its first bodies,
  // also ones that come into the empty view later, leave the view there.
  FirstBodiesFit fit;
  fit.arm();
  fit.disarm();
  CHECK(!fit.armed());
  CHECK(!fit.shown(true, false));
  CHECK(!fit.shown(true, true));
  CHECK(!fit.shown(false, true));
  // The next design installed without a home view is fitted as before.
  fit.arm();
  CHECK(fit.shown(true, true));
}

} // namespace

int firstBodiesFitTests() {
  failures = 0;
  newDesignTests();
  openedDesignTests();
  homeViewTests();
  return failures;
}
MITCAD_UNIT_TEST(firstBodiesFitTests);
