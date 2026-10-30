// SPDX-License-Identifier: MIT
#pragma once

#include <optional>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>

#include <gp_Dir.hxx>
#include <gp_Trsf.hxx>

namespace mitcad::cmd {

// A sketch's normal in the design (mitcad#958). The `sketch` query gives a
// sketch's frame in its component's coordinates; a sketch of a component
// placed by a turned occurrence, nested and oblique ones too, extrudes
// along that normal moved by the occurrences on the path it is seen at.
// Extrude's and Press Pull's arrows (CommandSupport's profileNormal) point
// there, so that the arrow and its drag follow the profiles as the view
// shows them.

// The `world` placement (4x4 rows into the design) of the occurrence at
// `path` ("O1/O4", uids from the root) in the `components` query's
// `occurrences` tree: identity for the root's empty path, none when the
// path leads nowhere or its placement is not a transform.
std::optional<gp_Trsf> occurrenceWorldIn(const QJsonArray& occurrences, const QString& path);

// The normal of a `sketch` query's `frame` in the design, seen at `path`:
// none without a normal; the component's normal as it is when the path
// leads nowhere.
std::optional<gp_Dir> sketchNormalInDesign(const QJsonObject& frame, const QJsonArray& occurrences,
                                           const QString& path);

} // namespace mitcad::cmd
