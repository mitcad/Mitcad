// SPDX-License-Identifier: MIT
#pragma once

// What a Move Face says of itself (mitcad#531; app/COMMANDS.md, "Move
// Face"): "MoveFace1: 2 faces of Body1, translate (-1.856, 0, -0.361) mm",
// for its panel's note, its log line and the selection card of its
// timeline cell. Plain Qt Core.

#include <functional>
#include <optional>

#include <QJsonObject>
#include <QJsonValue>
#include <QString>

namespace mitcad::cmd {

// A value slot of a definition (a number, or the name of a parameter) in
// millimetres or radians; none when it cannot be told.
using SlotValue = std::function<std::optional<double>(const QJsonValue&)>;

// A move's motion (commands.md, "move, align, scale"): "translate (5, 0, 0)
// mm", "translate 5 mm along x", "rotate 5 deg about an edge", "turn 5
// deg, translate (0, -1.74, 0.08) mm" (a free move's matrix), "point to
// point", "point to (10, 0, 0) mm". A slot `value` cannot tell shows as
// written (a parameter's name). Numbers are rounded to 1e-6.
// "up to a face" ("a body", "a point", "a line", "a plane", or an origin
// plane's or a construction plane's name: "up to xy") for
// {"type": "to_object", "object": <the model's to_object>} (mitcad#2548).
QString motionText(const QJsonObject& transform, const SlotValue& value = {});

// The motion of a Move Face definition as motionText reads it: its
// `transform`, or {"type": "to_object", "object": …} for one that moves up
// to an object (mitcad#2548).
QJsonObject moveFaceMotion(const QJsonObject& def);

// The whole line: "<name>: <n> face(s) of <body>, <motion>".
QString moveFaceSummary(const QString& name, int faces, const QString& body, const QJsonObject& transform,
                        const SlotValue& value = {});

} // namespace mitcad::cmd
