// SPDX-License-Identifier: MIT
#pragma once

// Resolve Sync Conflicts (P12 remote): what to keep of files changed both
// here and on the remote when a sync (or sharing a project onto a
// repository's files) stops for them. The dialogs that make, open and set
// up Cloud projects are in files/ProjectDialogs.hpp and
// files/ProjectSettings.hpp.

#include <functional>
#include <optional>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>

class QWidget;

namespace mitcad {

// For each file changed both here and on the remote (the sync's
// `conflicts`), keep mine, take theirs or save mine as a copy (the default,
// which loses nothing). `compare` gives the comparison of the remote's file
// with the project's for a conflict whose both sides have one (empty: no
// Compare). Returns {path: "mine" | "theirs" | "copy"}, nothing when
// cancelled (nothing changes then).
std::optional<QJsonObject> askResolveConflicts(QWidget* parent, const QJsonArray& conflicts,
                                               const std::function<QString(const QJsonObject& conflict)>& compare);

} // namespace mitcad
