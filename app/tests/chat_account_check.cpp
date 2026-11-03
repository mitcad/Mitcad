// SPDX-License-Identifier: MIT
// The chat's account check (Check again, mitcad#1155, agent/AccountCheck.hpp):
// right after the agent's check() a readiness it already had (the program
// gone, as at the last look; too old; signed out) ends the check, since no
// readiness change will come; Checking waits for one; an unfinished sign-in
// waits (the controller's timeout bounds it) but one that starts during the
// check ends it; Ready goes on to the probe. Check again is offered only for
// an account error while the agent can be used and no check runs.
#include "agent/AccountCheck.hpp"

#include <cstdio>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "chat_account_check.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

} // namespace

int chatAccountCheckTests() {
  using mitcad::agent::AccountCheckStep;
  using mitcad::agent::accountCheckOffered;
  using mitcad::agent::accountCheckStep;
  using mitcad::agent::agentUnusable;
  using Readiness = mitcad::agent::ChatAgent::Readiness;

  // Right after check() returned: nothing else will say how it ended.
  CHECK(accountCheckStep(Readiness::Missing, true) == AccountCheckStep::Fail);
  CHECK(accountCheckStep(Readiness::Outdated, true) == AccountCheckStep::Fail);
  CHECK(accountCheckStep(Readiness::SignedOut, true) == AccountCheckStep::Fail);
  CHECK(accountCheckStep(Readiness::Checking, true) == AccountCheckStep::Wait);
  CHECK(accountCheckStep(Readiness::SigningIn, true) == AccountCheckStep::Wait);
  CHECK(accountCheckStep(Readiness::Ready, true) == AccountCheckStep::Probe);

  // A readiness change during the check.
  CHECK(accountCheckStep(Readiness::Checking, false) == AccountCheckStep::Wait);
  CHECK(accountCheckStep(Readiness::Ready, false) == AccountCheckStep::Probe);
  CHECK(accountCheckStep(Readiness::Missing, false) == AccountCheckStep::Fail);
  CHECK(accountCheckStep(Readiness::Outdated, false) == AccountCheckStep::Fail);
  CHECK(accountCheckStep(Readiness::SignedOut, false) == AccountCheckStep::Fail);
  CHECK(accountCheckStep(Readiness::SigningIn, false) == AccountCheckStep::Fail);

  // The agent's own setup comes before the account error.
  CHECK(agentUnusable(Readiness::Missing));
  CHECK(agentUnusable(Readiness::Outdated));
  CHECK(!agentUnusable(Readiness::Ready));
  CHECK(!agentUnusable(Readiness::Checking));
  CHECK(!agentUnusable(Readiness::SignedOut));
  CHECK(!agentUnusable(Readiness::SigningIn));

  // Check again: an account error, the agent usable, no check running.
  CHECK(accountCheckOffered(true, Readiness::Ready, false));
  CHECK(accountCheckOffered(true, Readiness::SignedOut, false));
  CHECK(accountCheckOffered(true, Readiness::SigningIn, false));
  CHECK(!accountCheckOffered(false, Readiness::Ready, false));
  CHECK(!accountCheckOffered(true, Readiness::Ready, true));
  CHECK(!accountCheckOffered(true, Readiness::Missing, false));
  CHECK(!accountCheckOffered(true, Readiness::Outdated, false));
  return failures;
}
