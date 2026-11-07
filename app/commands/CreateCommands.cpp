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
  // The menu's sections (CommandDef::section, mitcad#579): from profiles,
  registry.add(extrudeCommand(context));
  registry.add(revolveCommand(context));
  registry.add(sweepCommand(context));
  registry.add(loftCommand(context));
  registry.add(ribCommand(context));
  registry.add(webCommand(context));
  registry.add(embossCommand(context));
  // on a body's faces (Plastic Feature: mitcad#459),
  registry.add(holeCommand(context));
  registry.add(threadCommand(context));
  registry.add(plasticCommand(context));
  // the primitives,
  registry.add(boxCommand(context));
  registry.add(cylinderCommand(context));
  registry.add(sphereCommand(context));
  registry.add(torusCommand(context));
  registry.add(coilCommand(context));
  registry.add(helixCommand(context));
  registry.add(pipeCommand(context));
  // the curve tools' Intersection Curve (mitcad#1405),
  registry.add(intersectionCurveCommand(context));
  // the Pattern submenu and Mirror, Envelope (mitcad#1276); the Machine
  // Elements submenu (Gear, Pulley) follows from
  // MainWindow::registerCommands.
  registry.add(rectangularPatternCommand(context));
  registry.add(circularPatternCommand(context));
  registry.add(pathPatternCommand(context));
  registry.add(pointPatternCommand(context));
  registry.add(mirrorCommand(context));
  registry.add(envelopeCommand(context));
}

} // namespace mitcad
