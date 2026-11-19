// SPDX-License-Identifier: MIT
// The ghost of a joint's side A (mitcad#147, mitcad#2027;
// framework/ViewGhosts.hpp): a pick on a part placed directly in the
// joint's component ghosts that occurrence with everything inside it; a
// deeper pick before a holder is chosen ghosts only its own occurrence's
// bodies, matched exactly; a chosen holder or an edit ghosts the occurrence
// placed directly in that component. And which bodies an entry covers.
#include "UnitTests.hpp"
#include "framework/ViewGhosts.hpp"

#include <cstdio>
#include <optional>

#include <QString>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "joint_ghost.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

QString s(const char* text) { return QString::fromLatin1(text); }

// Root active: O1 a part, O2 a sub-assembly with its own body and the
// parts O2/O3 and O2/O4.
void decisionTests() {
  // A part placed directly in the joint's component: the top-level
  // occurrence, by prefix (as before).
  CHECK(jointSideGhost(s("O1"), s("O1"), false, false) == s("O1"));
  // A pick inside a sub-assembly, no holder yet: its own occurrence only.
  CHECK(jointSideGhost(s("O2/O3"), s("O2/O3"), false, false) == s("=O2/O3"));
  // Deeper still: the pick's own occurrence, not the one between.
  CHECK(jointSideGhost(s("O2/O3/O7"), s("O2/O3/O7"), false, false) == s("=O2/O3/O7"));
  // The sub-assembly's own body: its own bodies only, not its parts.
  CHECK(jointSideGhost(s("O2"), s("O2"), false, true) == s("=O2"));
  // A chosen holder (O2's component, placed at O2) or an edit's component:
  // the occurrence placed directly in it, by prefix.
  CHECK(jointSideGhost(s("O2/O3"), s("O3"), true, false) == s("O2/O3"));
  CHECK(jointSideGhost(s("O2/O3/O7"), s("O3/O7"), true, false) == s("O2/O3"));
  CHECK(jointSideGhost(s("O2/O3"), s("O2/O3"), true, false) == s("O2"));
  CHECK(jointSideGhost(s("O2"), s("O2"), true, true) == s("O2"));
  // The component's own geometry, or a pick outside it: no ghost.
  CHECK(jointSideGhost(s("O2"), s(""), true, false).isEmpty());
  CHECK(jointSideGhost(s("O1"), std::nullopt, false, false).isEmpty());
}

void coverTests() {
  // By prefix: the occurrence and those inside it.
  CHECK(ghostCovers(s("O2"), s("O2")));
  CHECK(ghostCovers(s("O2"), s("O2/O3")));
  CHECK(!ghostCovers(s("O2"), s("O22")));
  CHECK(!ghostCovers(s("O2"), s("")));
  // Exact: the occurrence's own bodies only.
  CHECK(ghostCovers(ownBodiesGhost(s("O2")), s("O2")));
  CHECK(!ghostCovers(ownBodiesGhost(s("O2")), s("O2/O3")));
  CHECK(!ghostCovers(ownBodiesGhost(s("O2/O3")), s("O2")));
  CHECK(!ghostCovers(ownBodiesGhost(s("O2/O3")), s("O2/O4")));
  // The log's names.
  CHECK(ghostLabel(s("O2")) == s("O2"));
  CHECK(ghostLabel(ownBodiesGhost(s("O2/O3"))) == s("only O2/O3"));
}

} // namespace

int jointGhostTests() {
  failures = 0;
  decisionTests();
  coverTests();
  return failures;
}
MITCAD_UNIT_TEST(jointGhostTests);
