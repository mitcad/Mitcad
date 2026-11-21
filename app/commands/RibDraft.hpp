// SPDX-License-Identifier: MIT
#pragma once

// The Rib's and Web's Draft Angle (mitcad#2102, app/COMMANDS.md "Rib and
// Web"): its input, right after Thickness, and the definition's
// `draft_angle`, written only when it is not 0 so that a rib or web
// without a draft is saved as before it.

#include <QJsonObject>
#include <QVector>

#include "../framework/Command.hpp"

namespace mitcad::cmd {

// The Draft Angle input: an angle, 0 deg at first ("0: no draft").
InputDef ribDraftInput();

// Puts the Draft Angle input right after Thickness (last when there is
// none).
void insertRibDraft(QVector<InputDef>& inputs);

// Writes the input's expression as `draft_angle` when its value is not 0,
// or not yet known (an expression the model evaluates); leaves it out when
// it is 0 or empty.
void writeRibDraft(const CommandState& state, QJsonObject& def);

} // namespace mitcad::cmd
