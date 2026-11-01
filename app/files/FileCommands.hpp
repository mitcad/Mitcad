// SPDX-License-Identifier: MIT
#pragma once

#include <functional>
#include <vector>

#include "../framework/Command.hpp"

namespace mitcad {

// What the File menu's document commands do (MainWindow: openDocument,
// closeDocument, save, saveAs, close).
struct FileCommandRuns {
  std::function<void()> open;
  std::function<void()> close;
  std::function<void()> save;
  std::function<void()> saveAs;
  std::function<void()> exit;
  // Whether Save has something to do: a change since the design was saved,
  // an untitled design or a read-only window (which saves a copy).
  std::function<bool()> saveEnabled;
};

// The File menu's document commands as registry commands (mitcad#1090, until
// then plain menu actions): Open (file.open), Close (file.close), Save
// (file.save), Save As (file.save_as) and Exit (file.exit), with their keys
// (Ctrl+O, Ctrl+W, Ctrl+S, Ctrl+Shift+S, the platform's Quit), tooltips and
// search keywords; in the File menu (no tab), in every mode and while a
// command's panel is open, as the menu's actions were.
std::vector<CommandDef> fileDocumentCommands(const FileCommandRuns& runs);

} // namespace mitcad
