// SPDX-License-Identifier: MIT
#pragma once

// Preferences › General › "Extrude starts with" (mitcad#2164, GitHub
// mitcad/Mitcad#496): "Operation from the bodies it touches" (Automatic,
// the default: Extrude's automatic operation, mitcad#289) or "New Body".
// Kept per user in the application's settings ("extrude/startOperation":
// "automatic" or "new_body"; the Preferences group of Export Settings).
// Read when a new Extrude starts (CommandSession::startAutoOperation,
// AutoOperationPolicy), so a change applies from the next Extrude on; an
// edit keeps its stored operation and Repeat the one of the last OK either
// way, and JSON and MCP commands carry their own operation.

#include <optional>

#include <QString>

#include "AutoOperationPolicy.hpp"

namespace mitcad {

// The id kept in the settings and logged, and back; nullopt for another.
QString newFeatureOperationId(NewFeatureOperation choice);
std::optional<NewFeatureOperation> newFeatureOperationFromId(const QString& id);

// The user's choice; Automatic when none or an unknown one is kept.
NewFeatureOperation savedExtrudeStart();
// Saves the choice; false when it was already so.
bool saveExtrudeStart(NewFeatureOperation choice);

} // namespace mitcad
