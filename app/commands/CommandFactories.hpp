// SPDX-License-Identifier: MIT
#pragma once

// The feature commands of the SOLID tab (U4), made by the family files and
// registered in menu order by registerCreateCommands and
// registerModifyCommands (Commands.hpp).

#include "../framework/Command.hpp"

namespace mitcad {
class DocumentHost;
}

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
// PointPatternCommands.cpp
CommandDef pointPatternCommand(const CommandContext& context);
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
CommandDef appearanceCommand(const CommandContext& context);

// JointCommands.cpp (mitcad#55)
CommandDef jointCommand(const CommandContext& context);
CommandDef asBuiltJointCommand(const CommandContext& context);
CommandDef jointOriginCommand(const CommandContext& context);
CommandDef rigidGroupCommand(const CommandContext& context);
CommandDef driveJointCommand(const CommandContext& context);
// Assembly constraints (mitcad#148).
CommandDef constraintCommand(const CommandContext& context);
// Contact sets (mitcad#454).
CommandDef contactSetCommand(const CommandContext& context);
// Motion links (mitcad#466).
CommandDef motionLinkCommand(const CommandContext& context);
// EmbossCommands.cpp (mitcad#458)
CommandDef embossCommand(const CommandContext& context);

// PlasticCommands.cpp (mitcad#459)
CommandDef plasticCommand(const CommandContext& context);

// GearCommands.cpp (mitcad#472): `host` links the joints of meshed gears.
CommandDef gearCommand(const CommandContext& context, DocumentHost& host);

// MoveCommands.cpp (mitcad#531): Move Face, faces of a body moved rigidly.
CommandDef moveFaceCommand(const CommandContext& context);

// PulleyCommands.cpp (mitcad#562): timing belt pulleys and idlers.
CommandDef pulleyCommand(const CommandContext& context);

// BearingCommands.cpp (mitcad#577): printable bearings.
CommandDef bearingCommand(const CommandContext& context);

// DriveCommands.cpp (mitcad#656): Gear Pair and Belt Drive, both parts of
// a ratio between two picked axes. GearCommands.cpp links a meshed gear's
// joint to its mate's once the model is idle (`editing` the gear, the
// newest one when empty).
CommandDef gearPairCommand(const CommandContext& context, DocumentHost& host);
CommandDef beltDriveCommand(const CommandContext& context);
void linkLater(DocumentHost& host, const QString& editing);

// DogboneCommands.cpp (mitcad#1011): relief cuts at inside corners.
CommandDef dogboneCommand(const CommandContext& context);
// MortiseTenonCommands.cpp (mitcad#1012): a tenon on one body's end face
// and its mortise in another body.
CommandDef mortiseTenonCommand(const CommandContext& context);

} // namespace mitcad::cmd
