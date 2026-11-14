// SPDX-License-Identifier: MIT
#pragma once

// Features that change a body, on another component's body (mitcad#1634,
// app/COMMANDS.md "Activating the picked component"). Such a feature goes
// only into the body's own component, so a first pick of another
// component's body activates that component, without a question, and the
// command goes on with the same pick. Picks of two components keep the
// model's refusal, and the panel offers to activate one of them, keeping
// only its picks. Create Sketch and the Construct commands ask instead
// (browser/ActivateQuestion): there, keeping the active component is a real
// choice.
//
// A part placed by a pick (Gear, Pulley, Bearing, Coil, Box, Cylinder,
// Sphere, Torus; mitcad#1752) is in the table too, by its Place On: its
// placement refers to its own component's geometry, so a face or vertex of
// another component's body activates that component, as a Hole's does. A
// shaft of another component (a body's cylindrical face or circular edge)
// is the exception: the part goes in linked to it (a seat with its
// occurrence link, mitcad#578), stays in the active component, and the
// panel says so.

#include <QString>
#include <QStringList>
#include <QVector>

#include "../framework/Selection.hpp"
#include "DocumentSnapshot.hpp"

namespace mitcad {

// One row of the table: a command whose feature changes the bodies it
// picks or places a part by them (Place On), and the inputs (InputDef ids, also of a list's rows) that hold
// those picks. Its other inputs (a Replace Face's new surface, a Draft's
// plane, a Combine's tools, a Hole's sketch points) may take another
// component's geometry linked, and do not activate.
struct ActivatingCommand {
  QString command; // "solid.fillet"
  QStringList inputs;
};

// The table, in the order app/COMMANDS.md lists it.
const QVector<ActivatingCommand>& activatingCommands();

// The inputs of `command` whose picks activate their component; empty when
// the command is not in the table.
QStringList activatingInputs(const QString& command);

// The component whose body a face, edge, vertex or body belongs to (the
// component's definition, also for an occurrence placed several times);
// empty for any other pick and for a body of no component (a preview's).
QString bodyComponent(const DocumentSnapshot& snapshot, const SelectionItem& item);

// A component to activate for the picks of a command in the table.
struct PickActivation {
  QString component; // its uid; empty: nothing to activate
  QString name;      // its name, as the browser shows the component
  // The pick that names it (the new pick, or the newest of that
  // component's), for the log.
  SelectionItem picked;
  // The picks of other components that the Activate button drops.
  int dropped = 0;

  bool isEmpty() const { return component.isEmpty(); }
};

// A new pick `item` into one of the command's activating inputs, before it
// goes in; `picked`: the picks of those inputs so far. Its component, when
// it is a body's of a component other than the active one and no pick so
// far belongs to a body of another component than that one (none of the
// active component's, none of a third one's). Else nothing: the pick goes
// in as before.
PickActivation activationForPick(const DocumentSnapshot& snapshot, const SelectionItem& item,
                                 const Selection& picked);

// The Activate button beside the refusal, for the picks of a command's
// activating inputs: when their bodies belong to more than one component,
// or to one other than the active one, the component of the last pick that
// is not the active component's, with how many picks of other components
// it drops. Nothing when every body pick is the active component's.
PickActivation activationForPicks(const DocumentSnapshot& snapshot, const Selection& picks);

// The panel's notice once the component is activated for a command:
// "Fillet goes into Lid (now active)." and, when the Activate button dropped
// picks, "1 pick of another component dropped." on a line of its own.
// Components by their names only (mitcad#1327).
QString activatedNotice(const QString& command, const QString& component, int dropped);

// --- Parts placed by a pick (mitcad#1752)

// Whether `command` places a part by a pick whose Place On links a shaft
// of another component rather than activating it: Gear, Pulley, Bearing,
// Coil and the primitives; not Hole, which cuts the shaft's body and so
// goes into its component.
bool linksShafts(const QString& command);
// A body's cylindrical face or circular edge: a shaft or a hole that Place
// On takes as an axis.
bool isShaftPick(const SelectionItem& item);
// Whether a pick into one of the command's activating inputs activates
// its component (activationForPick then decides which): false for a shaft
// picked into a command that links shafts.
bool pickActivates(const QString& command, const SelectionItem& item);
// The picks of `picks` that activate (pickActivates), for the panel's
// Activate button.
Selection activatingPicks(const QString& command, const Selection& picks);
// The panel's notice for a shaft of a component other than the active one
// picked into such a command's Place On: "Gear goes into Base (active),
// linked to the shaft of Lid: a part on a shaft stays out of the shaft's
// component." `name` is the command's as the panel shows it. Empty for
// any other pick (the active component's shaft, a face, another command).
QString linkedShaftNotice(const DocumentSnapshot& snapshot, const QString& command, const QString& name,
                          const SelectionItem& item);
// The notice `shown` kept after a pick into such a command's Place On that
// activates nothing and is no linked shaft: an activation's notice while it
// still holds (that component still active), else nothing (an earlier
// shaft's notice no longer applies).
QString keptNotice(const DocumentSnapshot& snapshot, const QString& name, const QString& shown);

} // namespace mitcad
