// SPDX-License-Identifier: MIT
// The MODIFY group of the SOLID tab (U4), in menu order; Change
// Parameters follows (the main window's).
#include "Commands.hpp"

#include "../framework/CommandRegistry.hpp"
#include "CommandFactories.hpp"

namespace mitcad {

void registerModifyCommands(CommandRegistry& registry, const CommandContext& context) {
  using namespace cmd;
  registry.add(pressPullCommand(context));
  registry.add(filletCommand(context));
  registry.add(chamferCommand(context));
  registry.add(shellCommand(context));
  registry.add(draftCommand(context));
  registry.add(scaleCommand(context));
  registry.add(combineCommand(context));
  registry.add(offsetFaceCommand(context));
  registry.add(replaceFaceCommand(context));
  registry.add(splitFaceCommand(context));
  registry.add(splitBodyCommand(context));
  registry.add(moveCommand(context));
  registry.add(alignCommand(context));
  registry.add(deleteFaceCommand(context));
  registry.add(materialCommand(context));
  registry.add(appearanceCommand(context));
}

} // namespace mitcad
