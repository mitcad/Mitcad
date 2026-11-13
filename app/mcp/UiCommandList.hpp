// SPDX-License-Identifier: MIT
#pragma once

// The command registry as live MCP's ui_command shows it to an agent
// (mitcad#1269): the commands it may run, found by words, and the nearest
// ids to one the registry does not have. A chat agent reads no repository
// file, so these texts name none.

#include <functional>

#include <QJsonObject>
#include <QString>
#include <QStringList>

namespace mitcad {

class CommandRegistry;
struct CommandDef;

namespace mcp {

// Commands an agent cannot use through ui_command: they open file dialogs,
// interactive windows or the system's applications (mitcad#265). Any other
// modal dialog a command opens is closed at once (ModalGuard) and the call
// refused.
bool opensDialog(const QString& id);

// Why ui_dialog does not open a command's window for viewing (mitcad#1735),
// empty when it does: only the commands ui_command refuses for their
// dialog or window (opensDialog), except those that read or write files
// (file.*, insert.*, imports, exports, a library's publishing) or start
// another application (help.*: the browser, the file manager, mail).
QString dialogViewRefusal(const QString& id);

// The registry's ids nearest `wanted`, best first, at most `count`: those
// that share the most of its words (split at '.', '_', '-' and spaces, in
// the id or the name, any case), then by the fewest edits between the
// texts.
QStringList nearestCommandIds(const CommandRegistry& registry, const QString& wanted, int count = 5);

// What follows the refusal of an unknown id: the nearest ids with their
// names, and how to list them all.
QString nearestCommandsText(const CommandRegistry& registry, const QString& wanted);

// ui_command's listing: `commands`, each {id, name, kind (feature, action
// or tool), agents (whether ui_command runs it), why (when it does not),
// available (whether it can run now, by `availableNow`)}, in the
// registry's order; `search`'s words (all of them, any case) filter by
// id, name and keywords, an empty search lists every command. Also
// `count`, `total` and `search`.
QJsonObject commandListing(const CommandRegistry& registry, const QString& search,
                           const std::function<bool(const CommandDef&)>& availableNow);

} // namespace mcp
} // namespace mitcad
