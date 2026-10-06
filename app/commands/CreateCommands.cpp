// SPDX-License-Identifier: MIT
// The CREATE group of the SOLID tab (U4), in menu order.
#include "Commands.hpp"

#include "../framework/CommandRegistry.hpp"
#include "CommandFactories.hpp"

namespace mitcad {

void registerCreateCommands(CommandRegistry& registry, const CommandContext& context,
                            const CommandDef& createSketch) {
  using namespace cmd;
  registry.add(newComponentCommand(context));
  registry.add(createSketch);
  registry.add(extrudeCommand(context));
  registry.add(revolveCommand(context));
  registry.add(sweepCommand(context));
  registry.add(loftCommand(context));
  registry.add(ribCommand(context));
  registry.add(webCommand(context));
  registry.add(holeCommand(context));
  registry.add(threadCommand(context));
  registry.add(boxCommand(context));
  registry.add(cylinderCommand(context));
  registry.add(sphereCommand(context));
  registry.add(torusCommand(context));
  registry.add(coilCommand(context));
  registry.add(helixCommand(context));
  registry.add(pipeCommand(context));
  registry.add(rectangularPatternCommand(context));
  registry.add(circularPatternCommand(context));
  registry.add(pathPatternCommand(context));
  registry.add(mirrorCommand(context));
}

} // namespace mitcad
