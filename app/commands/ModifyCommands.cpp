// SPDX-License-Identifier: MIT
// The MODIFY group of the SOLID tab (U4), in menu order and in the menu's
// sections (mitcad#1778); Edit Appearances, Edit Parameters and
// Configurations follow (the main window's).
#include "Commands.hpp"

#include "../framework/CommandRegistry.hpp"
#include "CommandFactories.hpp"

namespace mitcad {

void registerModifyCommands(CommandRegistry& registry, const CommandContext& context) {
  using namespace cmd;
  // Their places on the toolbar are the layout's (mitcad#1951).
  // What changes the shape of a body.
  registry.add(pressPullCommand(context));
  registry.add(filletCommand(context));
  registry.add(chamferCommand(context));
  registry.add(shellCommand(context));
  registry.add(draftCommand(context));
  // Its faces.
  registry.add(offsetFaceCommand(context));
  registry.add(moveFaceCommand(context)); // mitcad#531
  registry.add(replaceFaceCommand(context));
  registry.add(fillCavityCommand(context)); // mitcad#1818
  registry.add(splitFaceCommand(context));
  registry.add(deleteFaceCommand(context));
  // Whole bodies.
  registry.add(combineCommand(context));
  registry.add(splitBodyCommand(context));
  registry.add(panelizeCommand(context)); // mitcad#2005
  registry.add(scaleCommand(context));
  registry.add(convertMeshCommand(context)); // mitcad#1082
  // Mesh tools for reverse engineering (mitcad#1871).
  registry.add(meshSectionCommand(context));
  registry.add(meshRepairCommand(context));
  registry.add(meshDecimateCommand(context));
  registry.add(meshRemoveCommand(context));
  registry.add(meshSegmentCommand(context));
  registry.add(meshFitSurfaceCommand(context));
  // Mesh tools that change a mesh's shape (mitcad#2515).
  registry.add(meshSmoothCommand(context));
  registry.add(meshCombineCommand(context));
  registry.add(meshOffsetCommand(context));
  registry.add(meshAlignCommand(context));
  registry.add(meshPlaneCutCommand(context));
  // Moving bodies and components.
  registry.add(moveCommand(context));
  registry.add(alignCommand(context));
  // Joinery.
  registry.add(dogboneCommand(context));      // mitcad#1011
  registry.add(mortiseTenonCommand(context)); // mitcad#1012
  // The look: the Material tool (mitcad#1059), Appearance, then the main
  // window's Edit Appearances; its Edit Parameters and Configurations are
  // the last section, "parameters".
  registry.add(materialCommand(context));
  registry.add(appearanceCommand(context));
}

} // namespace mitcad
