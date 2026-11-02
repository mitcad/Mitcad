// SPDX-License-Identifier: MIT
#pragma once

// Save Component as Design's file (mitcad#1148): the file chosen must not
// be the open design's own file or a file one of its components is linked
// from, however its path is spelled: a name that differs only in case on a
// disk that ignores case (macOS and Windows by default), a path through a
// symbolic link or with "..". Plain Qt Core and the standard library.

#include <QString>
#include <QStringList>

namespace mitcad {

// Whether two paths name the same file. When both exist: the same file on
// disk. When only one exists they are not the same file. When neither
// exists: the same cleaned absolute path, without regard to case on
// Windows and macOS.
bool sameFilePath(const QString& a, const QString& b);

// What Save Component as Design would write over.
enum class SaveComponentClash {
  None,
  Design, // the open design's own file
  Linked, // a file a component of the design is linked from
};

// Whether `target` is the open design's file (`design`, empty for a design
// without a file) or one of the `linked` files (absolute paths).
SaveComponentClash saveComponentClash(const QString& target, const QString& design, const QStringList& linked);

} // namespace mitcad
