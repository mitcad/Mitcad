// SPDX-License-Identifier: MIT
#pragma once

#include <optional>

#include <QString>

namespace mitcad {

// Linked designs (mitcad#1416, app/COMMANDS.md "Linked designs"): what the
// window reads from the model's words and paths, without the model.

// A component's refusal of an edit because it is linked from another file:
// the model's "<name> is linked from <path>; it changes only with that
// file", anywhere in an error message (after a command's own words too).
struct LinkedRefusal {
  QString component; // its name
  QString path;      // the link's path as the design saves it
};
std::optional<LinkedRefusal> linkedRefusal(const QString& message);

// The file a link's path names: a relative path from the folder of the
// design file `designFile`, cleaned (`..` taken out); empty for a relative
// path of a design without a file.
QString linkedFilePath(const QString& designFile, const QString& path);

// Whether two paths name the same file: their canonical paths when both
// exist (a link, a `..`, on Windows the case of the names), else their
// cleaned absolute paths.
bool sameFile(const QString& a, const QString& b);

} // namespace mitcad
