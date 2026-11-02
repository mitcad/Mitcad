// SPDX-License-Identifier: MIT
#pragma once

// The slicer processes Send to Slicer started (mitcad#1162, Use the open
// slicer window): whether one still runs, whether other processes run the
// same program, and its window brought to the front. Only processes MitCad
// started are asked about by their ids; the others are only counted.

#include <QString>
#include <QtGlobal>

namespace mitcad {

// A process by its id and its start time, so that an id the system gave
// to another process later is not taken for it.
struct SlicerProcess {
  qint64 pid = 0;
  qint64 started = 0; // the system's start time; 0: not running

  bool isValid() const { return pid > 0 && started != 0; }
};

// The process `pid` as it runs now (started 0 when it does not).
SlicerProcess slicerProcess(qint64 pid);

// Whether `process` still runs (the same id with the same start time).
bool slicerProcessRunning(const SlicerProcess& process);

// How many running processes run `program` (its executable, or a script
// an interpreter runs), `except` not counted; -1 when the system cannot
// say (then another window may be open).
int slicerProcessesOf(const QString& program, qint64 except);

// Brings the window of `pid` (or of a process it started, as an AppImage
// starts the application) to the front: Windows, and X11 with a window
// manager; false when no window was found or it cannot be done here.
bool raiseSlicerWindow(qint64 pid);

} // namespace mitcad
