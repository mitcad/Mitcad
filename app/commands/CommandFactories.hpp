// SPDX-License-Identifier: MIT
#pragma once

// The feature commands of the SOLID tab (U4), made by the family files and
// registered in menu order by registerCreateCommands and
// registerModifyCommands (Commands.hpp).

#include <utility>

#include <QJsonObject>
#include <QVector>

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
// Intersection Curve: a 3D curve from two sketches, a path (mitcad#1405).
CommandDef intersectionCurveCommand(const CommandContext& context);
// Helix Curve: a helical 3D curve about an axis, a path (mitcad#1777).
CommandDef helixCurveCommand(const CommandContext& context);
// Helix's sizes, shared by Surface Helix (mitcad#1797): the Type (a size
// pair), Revolutions, Height and Pitch inputs and Growth, with the texts
// the values start with and the tooltips that name what rises.
struct HelixSizeTexts {
  QString revolutions;
  QString height;
  QString pitch;
  QString heightTooltip;
  QString growthTooltip;
};
QVector<InputDef> helixSizeInputs(const HelixSizeTexts& texts);
// The definition's pitch and revolutions from the size pair: a height goes
// in as their ratio ("30 mm / 10 mm" turns, "30 mm / 3" a pitch).
void addHelixSizes(QJsonObject& definition, const CommandState& state);
// The size pair, its values and the growth back from a definition.
void loadHelixSizes(const QJsonObject& feature, CommandState& state, const CommandContext& context);
// The turns and the pitch the inputs give (for the log).
std::pair<double, double> helixTurnsAndPitch(const CommandState& state);
// Whether the Growth is anything but a plain zero.
bool helixHasGrowth(const CommandState& state);
// Helix's flat ends, shared by Surface Helix (mitcad#1798): Flat Start,
// Start Transition, End Transition and Flat End, angles of zero by default.
QVector<InputDef> helixEndInputs();
// The definition's `start` and `end` (`flat_angle`, `transition_angle`)
// for each end whose flat part or transition is not a plain zero.
void addHelixEnds(QJsonObject& definition, const CommandState& state);
// The flat ends' angles back from a definition, as their expressions.
void loadHelixEnds(const QJsonObject& feature, CommandState& state, const CommandContext& context);
// Whether either end has a flat part or a transition.
bool helixHasEnds(const CommandState& state);

// HoleCommands.cpp
CommandDef holeCommand(const CommandContext& context);
CommandDef threadCommand(const CommandContext& context);

// PrimitiveCommands.cpp
CommandDef newComponentCommand(const CommandContext& context);
CommandDef boxCommand(const CommandContext& context);
CommandDef cylinderCommand(const CommandContext& context);
CommandDef sphereCommand(const CommandContext& context);
CommandDef torusCommand(const CommandContext& context);
CommandDef coneCommand(const CommandContext& context);

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

// MaterialCommands.cpp (mitcad#1059): the Material and Appearance tools.
CommandDef appearanceCommand(const CommandContext& context);
CommandDef materialCommand(const CommandContext& context);

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
// ConvertMeshCommands.cpp (mitcad#1082): a mesh body made a solid, faceted
// or analytic.
CommandDef convertMeshCommand(const CommandContext& context);
// EnvelopeCommands.cpp (mitcad#1276): a light outer skin of a component,
// an occurrence or bodies.
CommandDef envelopeCommand(const CommandContext& context);
// FormCommands.cpp (mitcad#1757): a Form, a body whose surface is the
// smooth limit surface of a control cage started from a box, a cylinder, a
// sphere, a plane or a planar face; Edit Form changes its cage
// (registerFormEditCommand).
CommandDef formCommand(const CommandContext& context);
// FillCavityCommands.cpp (mitcad#1818): a closed pocket of a body filled
// up to a plane.
CommandDef fillCavityCommand(const CommandContext& context);

// FlexCommands.cpp (mitcad#1713): a flexible item laid through the model.
CommandDef flexCommand(const CommandContext& context, DocumentHost& host);

// PanelizeCommands.cpp (mitcad#2005): a body's planar faces split off as
// panels of one thickness, step by step.
CommandDef panelizeCommand(const CommandContext& context);
// MeshToolsCommands.cpp (mitcad#1871): mesh tools for reverse engineering,
// the sections of a mesh body into sketches and the mesh edits.
CommandDef meshSectionCommand(const CommandContext& context);
CommandDef meshRepairCommand(const CommandContext& context);
CommandDef meshDecimateCommand(const CommandContext& context);
CommandDef meshRemoveCommand(const CommandContext& context);
CommandDef meshSegmentCommand(const CommandContext& context);
CommandDef meshFitSurfaceCommand(const CommandContext& context);

// BoundaryFillCommands.cpp (mitcad#2149): the space solids, surfaces and
// planes close off filled, its cells clicked in the view.
CommandDef boundaryFillCommand(const CommandContext& context);

} // namespace mitcad::cmd
