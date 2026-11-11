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

#include <QString>
#include <QStringList>
#include <QVector>

#include "../framework/Selection.hpp"
#include "DocumentSnapshot.hpp"

namespace mitcad {

// One row of the table: a command whose feature changes the bodies it
// picks, and the inputs (InputDef ids, also of a list's rows) that hold
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

} // namespace mitcad
