// SPDX-License-Identifier: MIT
#pragma once

// Between edit locks and live updates (mitcad#89): the lock controller
// (files/LockController) keeps a Cloud project's designs' edit locks with
// the git commands of the core (commands.md "Edit locks") and works
// without live updates; the live controller (files/LiveController) keeps
// the MQTT connections of the core's LiveHub (commands.md "Live updates")
// and only makes things arrive sooner. They meet here:
//
// - The lock controller tells what others should learn at once through a
//   LiveLink, which the live controller implements: a lock taken, changed
//   or released, a request, receipt, answer or withdrawal, a design opened
//   or closed (and how), a version sent. Without a live controller, or
//   while the project has no live connection, the calls do nothing.
// - The live controller passes the checked events of a project to
//   LockEvents, which the lock controller implements: a lock, request or
//   version event polls the remote's lock refs at once (only the refs grant
//   a lock), an open event updates who has a design open, a session's
//   offline will may make a lock stale (after the user confirms), and the
//   connection's state sets the poll interval and the locks' `mqtt` field.
//
// Projects are named by their folder (the project root, absolute); paths
// are relative to it with "/". Payloads are the JSON objects the core's
// commands answer and its live events carry, unchanged.

#include <QJsonObject>
#include <QString>

namespace mitcad {

// Implemented by the live controller; the lock controller calls it.
class LiveLink {
public:
  virtual ~LiveLink() = default;

  // Whether the project's live connection is up (connected and
  // subscribed): the lock controller then polls git less often and writes
  // `mqtt: true` into its locks.
  virtual bool liveConnected(const QString& root) const = 0;
  // A lock of this session taken, refreshed with a new state or handed
  // over (`lock`: the lock as the core answered it), or released (`lock`
  // empty).
  virtual void publishLock(const QString& root, const QString& path, const QJsonObject& lock) = 0;
  // A request, receipt, answer or withdrawal: the live `publish` message's
  // fields (`type`: request, receipt, answer, withdrawn; `path`, `for`,
  // `answer`, `until`, `message`).
  virtual void publishRequest(const QString& root, const QJsonObject& message) = 0;
  // This session's window of a design: `mode` "editing" or "read-only",
  // empty when the window closed.
  virtual void publishOpen(const QString& root, const QString& path, const QString& mode) = 0;
  // A version sent to the remote (after a push or a sync that pushed).
  virtual void publishVersion(const QString& root, const QString& branch, const QString& commit) = 0;
};

// Implemented by the lock controller; the live controller calls it on the
// UI thread.
class LockEvents {
public:
  virtual ~LockEvents() = default;

  // A checked live event of the project (`type`: lock, request, version,
  // open, session, subscribed, dropped), as the hub gave it.
  virtual void liveEvent(const QString& root, const QJsonObject& event) = 0;
  // The project's live connection came up or went down.
  virtual void liveStateChanged(const QString& root, bool connected) = 0;
};

} // namespace mitcad
