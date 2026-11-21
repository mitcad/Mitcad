// SPDX-License-Identifier: MIT
#pragma once

#include <QHash>
#include <QJsonArray>
#include <QString>
#include <QStringList>
#include <QVector>

#include "DocumentSnapshot.hpp"

namespace mitcad {

// Folders of the Browser's component rows (mitcad#2103): organization
// only, a name kept on each occurrence in one (`set_occurrence_folder`).
// A component's occurrences are listed with its folders first, in the
// order of their first occurrences, each with its occurrences in the
// model's order, then the occurrences in no folder in theirs. Plain Qt
// Core: what the Browser's rows, menus and drops share.
namespace browser_order {

// An occurrence among the occurrences of its parent: its path ("O1/O4"),
// name (Rail:1) and the folder it is in (empty: none).
struct Sibling {
  QString path;
  QString name;
  QString folder;
};

// The path of the occurrence an occurrence is placed in ("O1/O4": "O1";
// "O1": "", the root).
QString parentPath(const QString& path);

// The occurrences placed in the component of the occurrence `parent` (the
// root's for an empty path), in the model's order; empty when there is no
// such occurrence.
QVector<Sibling> siblingsIn(const DocumentSnapshot& snapshot, const QString& parent);

// The folders of these occurrences, in the order of their first
// occurrences: the Browser's order of the folders' rows.
QStringList folderOrder(const QVector<Sibling>& siblings);

// The paths of the occurrences in `folder` (empty: those in no folder), in
// the model's order: their order in the Browser.
QStringList groupOf(const QVector<Sibling>& siblings, const QString& folder);

// The name a New Folder proposes (mitcad#2103): Folder1, Folder2, …, the
// first one `taken` does not have.
QString newFolderName(const QStringList& taken);

// The folder commands that put these occurrences (paths) into `folder`, or
// into none with an empty `folder`, leaving out those already there; one
// undo step together (runModelCommands).
QJsonArray occurrenceFolderCommands(const QVector<Sibling>& siblings, const QStringList& paths,
                                    const QString& folder);

// The same for bodies (uids) of a component's Bodies folder: `folders` has
// each body's folder now (empty: none), by uid.
QJsonArray bodyFolderCommands(const QHash<QString, QString>& folders, const QStringList& bodies,
                              const QString& folder);

// The order of the occurrences among their siblings (mitcad#2104): the
// row before (`step` -1) or after (+1) `path` in its group (its folder, or
// the rows in no folder) in the Browser's order; empty at the group's end,
// where Move Up or Move Down cannot go.
QString neighbor(const QVector<Sibling>& siblings, const QString& path, int step);

// The model commands that put the occurrences `moved` (paths of siblings,
// in the order they keep) into the group `folder` (empty: no folder) just
// before the row `anchor` of that group, or just after it with `after`, or
// at the group's end without an anchor: the folder commands of those that
// change their folder, then one `reorder_occurrence`. The folders keep
// their places among the folders (the order of their first occurrences): a
// folder's occurrences are placed together where its first one is; rows in
// no folder keep the order of the others. Empty when nothing changes; one
// undo step together (runNamedModelCommands).
QJsonArray moveCommands(const QVector<Sibling>& siblings, const QStringList& moved, const QString& folder,
                        const QString& anchor, bool after);

} // namespace browser_order
} // namespace mitcad
