// SPDX-License-Identifier: MIT
// When a command's Operation follows the model's automatic choice
// (AutoOperationPolicy): a new feature until the user picks one; an edit
// never, it keeps its stored operation (mitcad#868: Flip pointed an edited
// boss into its arm and its Join became a Cut).
#include "UnitTests.hpp"
#include "framework/AutoOperationPolicy.hpp"

#include <cstdio>

int autoOperationPolicyTests() {
  int failures = 0;
  const auto check = [&failures](bool ok, const char* message) {
    if (!ok) { std::fprintf(stderr, "auto operation policy: %s\n", message); ++failures; }
  };
  using mitcad::AutoOperationPolicy;
  AutoOperationPolicy state;
  check(!state.active(), "nothing is automatic before a command starts");
  state.start(true, false);
  check(state.active(), "a new feature follows the automatic choice");
  state.disable();
  check(!state.active(), "the user's own Operation, Objects or targets stop it for the rest of the command");
  state.start(true, true);
  check(!state.active(), "an edit keeps its stored operation (mitcad#868)");
  state.disable();
  check(!state.active(), "a hand change in an edit leaves it off");
  state.start(true, false);
  state.start(true, true);
  check(!state.active(), "an edit started after a new feature does not inherit its automatic mode");
  state.start(false, false);
  check(!state.active(), "commands without an automatic operation never follow one");
  return failures;
}
MITCAD_UNIT_TEST(autoOperationPolicyTests);
