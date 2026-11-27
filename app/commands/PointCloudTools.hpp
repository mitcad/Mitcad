// SPDX-License-Identifier: MIT
#pragma once

// Point cloud tools (mitcad#2516; app/COMMANDS.md "Point Cloud Tools"):
// Align Point Cloud, Filter Point Cloud and Reconstruct Mesh, registered
// by MainWindow::registerPointCloudCommands after Move Point Cloud.

#include <functional>

#include <QString>

#include "../framework/Command.hpp"

namespace mitcad {

class DocumentHost;

// The cloud a tool's panel starts with, taken once: the one whose Browser
// row opened it (empty: the design's first cloud).
using ChosenCloud = std::function<QString()>;

CommandDef pointCloudAlignCommand(DocumentHost& host, ChosenCloud chosen);
CommandDef pointCloudFilterCommand(ChosenCloud chosen);
CommandDef pointCloudReconstructCommand(ChosenCloud chosen);

} // namespace mitcad
