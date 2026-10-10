// SPDX-License-Identifier: MIT
#pragma once

// File > Project Settings (mitcad#89, section 6): everything about a Local
// or Cloud project in one place. Changes apply at once.
//
// - The folder, the storage (Local -> Cloud: the Cloud section and Share,
//   which sends the whole history; Cloud -> Local: Stop Syncing, the remote
//   removed), the address with Change... and Open in Browser, the state
//   with Sync Now.
// - For everyone in the project (the marker, recorded as a version and
//   sent): edit locks (idle time, poll interval) and live updates (a
//   broker and a topic prefix).
// - On this computer (.mitcad/local and the repository's configuration):
//   the author, sending each version at once, how often the remote is
//   checked, live updates here.
//
// The edit locks and live updates themselves are the lock controller's
// (a later change): the dialog reads and writes their settings and calls
// the hooks below.

#include <functional>

#include <QJsonObject>
#include <QString>

class QWidget;

namespace mitcad {

struct ProjectSettingsHost {
  // The project's kind or address changed (shared, stopped syncing, a new
  // address): the window's project and indicator follow.
  std::function<void()> projectChanged;
  // Stops the project's running remote work before work that changes its
  // remote; false (said) while a sync runs.
  std::function<bool(const QString& why)> settle;
  // A version was recorded (the shared settings): sent as a saved one is.
  std::function<void()> versionRecorded;
  // Sync Now (after the dialog closed) and Open in Browser.
  std::function<void()> sync;
  std::function<void()> openInBrowser;
  // The project's sync state for the Status row ("Up to date, last sync
  // 10:42").
  std::function<QString()> status;
  // Hooks of the lock and live-update controller: before the project stops
  // syncing (this session's lock refs released, live updates stopped), and
  // after the project's settings changed (project_settings' answer).
  std::function<void()> stoppingSync;
  std::function<void(const QJsonObject& settings)> settingsChanged;
  // Test of a broker (connect, subscribe, publish once under the prefix);
  // empty: the Test button is off.
  std::function<void(QWidget* parent, const QString& broker, const QString& prefix)> testBroker;
  // Whether the Cloud project's remote accepts Mitcad's lock refs
  // (lock_probe, once per remote): `done` gets it unless `context` is gone;
  // a remote that does not disables the Edit locks row with the message.
  std::function<void(QObject* context, std::function<void(bool accepted, const QString& message)> done)> probeLocks;
};

// The dialog for the project in `root` (a project with versions).
void showProjectSettings(QWidget* parent, const QString& root, const ProjectSettingsHost& host);

// What a project's check_minutes setting means: the interval in minutes (0:
// never), the application's default when unset.
struct ProjectSync {
  bool sendAtOnce = true;
  int checkMinutes = 10;
};
// The sync settings of the project in `root` on this computer, with the
// application's defaults for what it does not set.
ProjectSync projectSync(const QString& root);

} // namespace mitcad
