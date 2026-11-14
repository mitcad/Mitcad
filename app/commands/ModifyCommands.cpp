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
  // Registration order is the menu's order: each section's commands
  // together, the sections in their order (CommandDef::section).
  const auto add = [&registry](CommandDef def, const char* section) {
    def.section = QString::fromLatin1(section);
    registry.add(def);
  };
  // What changes the shape of a body.
  add(pressPullCommand(context), "shape");
  add(filletCommand(context), "shape");
  add(chamferCommand(context), "shape");
  add(shellCommand(context), "shape");
  add(draftCommand(context), "shape");
  // Its faces.
  add(offsetFaceCommand(context), "faces");
  add(moveFaceCommand(context), "faces"); // mitcad#531
  add(replaceFaceCommand(context), "faces");
  add(splitFaceCommand(context), "faces");
  add(deleteFaceCommand(context), "faces");
  // Whole bodies.
  add(combineCommand(context), "bodies");
  add(splitBodyCommand(context), "bodies");
  add(scaleCommand(context), "bodies");
  add(convertMeshCommand(context), "bodies"); // mitcad#1082
  // Moving bodies and components.
  add(moveCommand(context), "move");
  add(alignCommand(context), "move");
  // Joinery.
  add(dogboneCommand(context), "joinery");      // mitcad#1011
  add(mortiseTenonCommand(context), "joinery"); // mitcad#1012
  // The look: the Material tool (mitcad#1059), Appearance, then the main
  // window's Edit Appearances; its Edit Parameters and Configurations are
  // the last section, "parameters".
  add(materialCommand(context), "look");
  add(appearanceCommand(context), "look");
}

} // namespace mitcad
