// SPDX-License-Identifier: MIT
#pragma once

// The places of the model's notices (mitcad#1327, docs/ui-conventions.md
// "Messages and severity"): a notice's message says what is wrong without
// topological names; the faces, edges, vertices, points and sketch regions
// it is about are its `places` (core/model-api/src/commands.md, "Notices"), which the view
// paints while the pointer is over the notice (the command panel's message,
// a Timeline item, a Browser row, the message band, the Docked status line).
// Plain Qt Core.

#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>

namespace mitcad {

// The places of these notices (each one's `places`), each once, in order.
QJsonArray noticePlaces(const QJsonArray& notices);

// The messages of a result's notices of a level ("warning", "error"), in
// order; none where it has no notices.
QStringList noticeMessages(const QJsonObject& result, const QString& level);

// The places for the log: "face F7:fillet(E{…}) of F2.b0, point 1.000,2.000,3.000",
// "region r{…} of F1" (its sketch), "none" for no places.
QString placesText(const QJsonArray& places);

} // namespace mitcad
