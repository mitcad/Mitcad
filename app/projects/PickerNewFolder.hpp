// SPDX-License-Identifier: MIT
#pragma once

// The project picker's New Folder (mitcad#1649, app/COMMANDS.md "Project
// picker"): the name it proposes, the name typed, and why the button waits.
// Plain Qt Core, unit-tested (tests/picker_new_folder.cpp); the dialog is
// projects/ProjectPicker.cpp.

#include <QString>
#include <QStringList>

namespace mitcad {

// The first of `base`, "`base` 2", "`base` 3", ... that none of `taken` is,
// case ignored (as a system that does not tell the case of letters apart
// would refuse it): New Folder, New Folder 2, ...
QString freeFolderName(const QString& base, const QStringList& taken);

// A folder's name as typed (trimmed); "" when no folder can have it: empty,
// starting or ending with a dot, or holding / \ < > : " | ? * or a control
// character. Other names the core refuses (CON, NUL and the like) come back
// as its refusal (`invalid_path`).
QString folderNameOf(const QString& typed);

// What the picker has selected, for New Folder.
enum class FolderHost {
  None,    // nothing that can hold a folder (no row, or no project listed)
  Reading, // the projects are still being read
  Naming,  // a new folder's name is being typed
  Place,   // a project or a folder of one
};

// Why New Folder waits, for its tooltip and the log; "" when it can make a
// folder. `writable`: whether the selected folder on disk can be written
// (false: a read-only project).
QString newFolderBlocked(FolderHost host, bool writable);

} // namespace mitcad
