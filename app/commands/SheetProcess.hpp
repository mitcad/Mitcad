// SPDX-License-Identifier: MIT
// Sheet process profiles (mitcad#1939; commands.md "Sheet process
// profiles", app/COMMANDS.md "Sheet process profiles"): the commands that
// belong to some profiles only are dimmed for the others, with the reason in
// words as their tooltip. The model's `sheet_process` query says which.
#pragma once

#include <QString>

#include "CommandSupport.hpp"

namespace mitcad::cmd {

// Whether the active component's sheet profile offers `command` (always,
// when it has no sheet rules); `reason` gets why not, in words.
bool processAllows(const CommandContext& context, const QString& command, QString* reason = nullptr);

// The reason `command` is dimmed, for its tooltip; empty when it is not.
QString processReason(const CommandContext& context, const QString& command);

// Sheet Metal's Fold and Flange (SheetCommands.cpp), which Kerf Bend and
// Glue Flap start from.
CommandDef sheetFoldCommand(const CommandContext& context);
CommandDef sheetFlangeCommand(const CommandContext& context);

} // namespace mitcad::cmd
