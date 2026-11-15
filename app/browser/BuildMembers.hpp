// SPDX-License-Identifier: MIT
#pragma once

#include <QHash>
#include <QJsonArray>
#include <QString>
#include <QStringList>

namespace mitcad {

// BUILD parts deleted as members (mitcad#1811, mitcad#1820): the Browser's
// Delete and Delete Component take a BUILD part away with build_remove,
// which removes the holes and fasteners the part added along with it.

// Each BUILD part of the model's build_parts answer: its component, with the
// component of the part that added it (a connector's fastener), or an empty
// text for one nothing added.
QHash<QString, QString> buildPartAdders(const QJsonArray& parts);

// The build_remove commands that delete these BUILD parts' components, in
// their order, each once. A part whose adder is among them is left out: its
// adder's build_remove takes it, and one of its own after that would find
// it gone and be refused, undoing the whole delete (mitcad#1843). Only the
// adder itself takes it, as build_remove takes only what that part added:
// a part whose adder is itself left to its own adder keeps its command.
QJsonArray buildRemoveCommands(const QStringList& components, const QHash<QString, QString>& adders);

} // namespace mitcad
