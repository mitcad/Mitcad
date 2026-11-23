// SPDX-License-Identifier: MIT
#pragma once

#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>
#include <QVector>

#include "../framework/Selection.hpp"
#include "BrowserPanel.hpp"

namespace mitcad {

// Selection sets (mitcad#2192, core/model-api/src/commands.md "Selection
// sets", app/COMMANDS.md "Selection Sets"): what of a selection or of
// Browser rows a set takes, as the model's members (`{"body": "F2.b0"}`,
// `{"feature": "F5", "occurrence": "O1"}`, `{"occurrence": "O1/O4"}`).

// A selected item's member: a body, a component (an occurrence), a sketch,
// construction geometry, a joint or another feature, with the occurrence
// path that places it; empty for what a set does not take (faces, edges,
// vertices, sketch entities, the origin's planes and axes, fixed planes).
QJsonObject setMemberOf(const SelectionItem& item);

// A Browser row's member: a body's, a component's (not the root's), a
// sketch's, a datum's, a joint's or a feature's row, also a set's member
// row (its item); empty for any other row.
QJsonObject setMemberOf(const BrowserNode& node);

// The members of items, each once, in their order; empty when one of them
// is not a member a set takes (Create Selection Set is offered only when
// every item is one).
QJsonArray setMembersOf(const Selection& items);

// The members of rows, each once, in their order; the rows a set does not
// take are left out, and so are the bodies of a feature whose own row is
// among them (selecting a feature's row selects its bodies' rows too).
QJsonArray setMembersOf(const QVector<BrowserNode>& nodes);

// Delete on selection set member rows (mitcad#2219): their links go, as
// Remove from Set, never their items. One `set_selection_set_members`
// command per set, in the order of the rows (`{"cmd":
// "set_selection_set_members", "name": "Favourites", "remove": [members]}`);
// the rows of no set give none.
QJsonArray setMemberRemovals(const QVector<BrowserNode>& nodes);

// A member as a short text for the log ("body F2.b0", "feature F5 in O1",
// "occurrence O1/O4").
QString setMemberText(const QJsonObject& member);

} // namespace mitcad
