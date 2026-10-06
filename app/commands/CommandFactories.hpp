// SPDX-License-Identifier: MIT
#pragma once

// The feature commands of the SOLID tab (U4), made by the family files and
// registered in menu order by registerCreateCommands and
// registerModifyCommands (Commands.hpp).

#include "../framework/Command.hpp"

namespace mitcad::cmd {

// ExtrudeCommands.cpp
CommandDef extrudeCommand(const CommandContext& context);
CommandDef revolveCommand(const CommandContext& context);

// SweepCommands.cpp
CommandDef sweepCommand(const CommandContext& context);
CommandDef loftCommand(const CommandContext& context);
CommandDef ribCommand(const CommandContext& context);
CommandDef webCommand(const CommandContext& context);
CommandDef coilCommand(const CommandContext& context);
CommandDef pipeCommand(const CommandContext& context);
CommandDef helixCommand(const CommandContext& context);

// HoleCommands.cpp
CommandDef holeCommand(const CommandContext& context);
CommandDef threadCommand(const CommandContext& context);

// PrimitiveCommands.cpp
CommandDef newComponentCommand(const CommandContext& context);
CommandDef boxCommand(const CommandContext& context);
CommandDef cylinderCommand(const CommandContext& context);
CommandDef sphereCommand(const CommandContext& context);
CommandDef torusCommand(const CommandContext& context);

// PatternCommands.cpp
CommandDef rectangularPatternCommand(const CommandContext& context);
CommandDef circularPatternCommand(const CommandContext& context);
CommandDef pathPatternCommand(const CommandContext& context);
CommandDef mirrorCommand(const CommandContext& context);

// FilletCommands.cpp
CommandDef pressPullCommand(const CommandContext& context);
CommandDef filletCommand(const CommandContext& context);
CommandDef chamferCommand(const CommandContext& context);

// FaceCommands.cpp
CommandDef shellCommand(const CommandContext& context);
CommandDef draftCommand(const CommandContext& context);
CommandDef offsetFaceCommand(const CommandContext& context);
CommandDef replaceFaceCommand(const CommandContext& context);
CommandDef splitFaceCommand(const CommandContext& context);
CommandDef splitBodyCommand(const CommandContext& context);
CommandDef deleteFaceCommand(const CommandContext& context);

// MoveCommands.cpp
CommandDef scaleCommand(const CommandContext& context);
CommandDef combineCommand(const CommandContext& context);
CommandDef moveCommand(const CommandContext& context);
CommandDef alignCommand(const CommandContext& context);
CommandDef materialCommand(const CommandContext& context);
CommandDef appearanceCommand(const CommandContext& context);

} // namespace mitcad::cmd
