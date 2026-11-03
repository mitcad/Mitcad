// SPDX-License-Identifier: MIT
#pragma once

// The account check's decisions (Check again, mitcad#1126, mitcad#1155) as
// plain functions of the agent's readiness, so that the unit tests reach
// the cases the UI cannot (tests/chat_account_check.cpp).

#include "ChatAgent.hpp"

namespace mitcad::agent {

enum class AccountCheckStep {
  Wait,  // the setup check goes on: its readiness change or the timeout ends it
  Probe, // the agent is ready: the bounded probe turn
  Fail,  // the agent cannot be used now: the check ends
};

// What the agent's readiness means for the account check's setup check.
// `atOnce`: right after check() returned. A check that ends at once with
// the readiness the agent already had (its program gone, as at the last
// look) emits no readiness change, so its outcome is taken from here
// (mitcad#1155); an unfinished sign-in may still end in Ready, so it waits
// (bounded by the controller's timeout), while a sign-in that starts during
// the check ends it.
inline AccountCheckStep accountCheckStep(ChatAgent::Readiness readiness, bool atOnce) {
  switch (readiness) {
  case ChatAgent::Readiness::Checking: return AccountCheckStep::Wait;
  case ChatAgent::Readiness::Ready: return AccountCheckStep::Probe;
  case ChatAgent::Readiness::SigningIn: return atOnce ? AccountCheckStep::Wait : AccountCheckStep::Fail;
  case ChatAgent::Readiness::Missing:
  case ChatAgent::Readiness::Outdated:
  case ChatAgent::Readiness::SignedOut: return AccountCheckStep::Fail;
  }
  return AccountCheckStep::Fail;
}

// The agent itself cannot be used (not found, too old): the panel shows its
// setup before a known account error.
inline bool agentUnusable(ChatAgent::Readiness readiness) {
  return readiness == ChatAgent::Readiness::Missing || readiness == ChatAgent::Readiness::Outdated;
}

// Check again is offered for a known account error while the agent can be
// used and no check runs (mitcad#1155: not while the agent is missing, where
// its check would end at once).
inline bool accountCheckOffered(bool accountError, ChatAgent::Readiness readiness, bool checking) {
  return accountError && !agentUnusable(readiness) && !checking;
}

} // namespace mitcad::agent
