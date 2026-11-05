// SPDX-License-Identifier: MIT
#pragma once

// The account check's decisions (Check again, mitcad#1126, mitcad#1155) as
// plain functions of the agent's readiness, so that the unit tests reach
// the cases the UI cannot (tests/chat_account_check.cpp), and which agents
// share one sign-in (mitcad#1297), whose account error is all of theirs.

#include <QDir>
#include <QList>
#include <QString>
#include <QStringList>

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

// The others of `folders` (each agent's ChatAgent::signInFolder) that share
// the sign-in of the one at `index` (mitcad#1297): the same configuration
// folder, its path cleaned (on Windows in any case). An empty folder, a
// sign-in of the agent's own, is shared with none.
inline QList<int> sameSignIn(const QStringList& folders, int index) {
  QList<int> others;
  if (index < 0 || index >= int(folders.size())) {
    return others;
  }
#ifdef Q_OS_WIN
  constexpr Qt::CaseSensitivity kCase = Qt::CaseInsensitive;
#else
  constexpr Qt::CaseSensitivity kCase = Qt::CaseSensitive;
#endif
  const auto cleaned = [](const QString& folder) {
    return folder.trimmed().isEmpty() ? QString() : QDir::cleanPath(QDir::fromNativeSeparators(folder.trimmed()));
  };
  const QString own = cleaned(folders.at(index));
  if (own.isEmpty()) {
    return others;
  }
  for (int other = 0; other < int(folders.size()); ++other) {
    if (other != index && cleaned(folders.at(other)).compare(own, kCase) == 0) {
      others.append(other);
    }
  }
  return others;
}

} // namespace mitcad::agent
