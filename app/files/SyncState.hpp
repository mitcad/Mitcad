// SPDX-License-Identifier: MIT
#pragma once

// A Cloud project's sync state in words (mitcad#1895): what the project
// indicator's tooltip, the Projects panel's row and the status bar say
// while its versions go to the project's server and after.
//
//   Sending to the project's server…
//   Up to date, last sync 14:02
//   Offline: changes are kept and sent later (2 versions waiting)
//
// Plain Qt Core, unit-tested in app.unit (tests/version_queue.cpp).

#include <QDateTime>
#include <QString>

namespace mitcad {

// What the remote work knows of a project's server.
struct SyncState {
  int ahead = -1; // versions to send, -1 not known yet
  int behind = -1; // newer versions on the server, -1 not known yet
  QString problem; // the class of the latest failure, "" none
  QString busy;    // a running task ("push", "fetch", "sync", ...), "" none
  bool queued = false; // `busy` waits for other work of the project
  bool conflict = false; // a sync stopped for choices
  QDateTime lastDone; // the latest fetch, push or sync that went well
};

struct SyncWords {
  QString text;   // for people (translated)
  QString logged; // for the log ("sending", "up to date, last sync 14:02")
  bool problem = false; // a failure: the status bar's message is red
};

// `state` in words; the last sync's time as "14:02" when it is on `today`,
// else "2026-10-05 14:02".
SyncWords syncWords(const SyncState& state, const QDate& today = QDate::currentDate());

} // namespace mitcad
