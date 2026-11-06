// SPDX-License-Identifier: MIT
#pragma once

// Which render worker the application starts (mitcad#1332, docs/rendering.md
// "Building an OptiX worker yourself"): MITCAD_RENDER_WORKER (tests) over
// the worker picked in Preferences (the user's settings, render/worker: one
// the user built, such as mitcad-render-optix), over the shipped
// mitcad-render next to the application. A picked worker that is not there,
// does not start or speaks another protocol version gives way to the
// shipped one for the rest of the run. Qt Core only.

#include <QString>

namespace mitcad::render {

// The worker picked in the settings: an executable's path; empty for the
// shipped one (the default).
QString pickedWorker();
void setPickedWorker(const QString& path);

// The worker to start, as chooseWorker found it.
struct WorkerChoice {
  QString path; // the executable; empty when there is none
  bool picked = false; // the picked worker, which gives way to the shipped one when it fails
  QString fallback; // a picked worker that is not used and why, for the log and the status bar
};

// The choice: `forced` (MITCAD_RENDER_WORKER) when set, used as it is
// (an executable file, else none); else `picked` when it is an executable
// file and not `failed` (the picked worker that failed in this run);
// else `shipped`. A picked worker that is not an executable file gives a
// `fallback` text; one that failed was reported when it failed.
WorkerChoice chooseWorker(const QString& forced, const QString& picked, const QString& failed,
                          const QString& shipped);

// The absolute path of an executable file, else empty.
QString executableFile(const QString& path);

// The picked worker that failed in this run (empty: none), and a failure
// to remember: the worker's path and why it is not used. Returns the
// message for the status bar and logs it.
QString failedPickedWorker();
QString pickedWorkerFailed(const QString& worker, const QString& reason);

} // namespace mitcad::render
