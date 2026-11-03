// SPDX-License-Identifier: MIT
#pragma once

#include <QJsonObject>
#include <QPair>
#include <QString>
#include <QStringList>
#include <QVector>

#include "PrintKeys.hpp"

namespace mitcad {

// What an import of print settings says (mitcad#654, mitcad#655), from
// `import_print_profile`'s answer: the line under 3D Print's Import from
// slicer…, short ("plate.3mf imported", "mount.3mf imported. 1 setting not
// read": only what failed is counted; one item that came in as a copy
// names its new name, "Fine imported into My library as Fine 2",
// mitcad#830), and the whole report behind Details…: what came in by name
// (a heading in the singular for one item) and the same count (Imported;
// the slicer's own settings Mitcad keeps have no section), what was left out (Left out), and each distinct problem once by
// the plain name of its key with the number of profiles that had it (Not
// read; one without a plain name only counted); a section without
// anything is left out. No raw key reaches the user. Plain Qt Core.
struct PrintImportSummary {
  QString line;          // the short line
  QString title;         // the report's first line: the file, its slicer, where it went
  // The report's sections with their lines, only those with something:
  // the imported (no heading: the title says it), Left out, Not read.
  QVector<QPair<QString, QStringList>> sections;
  QString details;       // the whole report as text: the title, then each section
  QStringList leftOut;   // what was left out, as reported
  QStringList problems;  // each distinct problem once, with its count
  int materialsAdded = 0; // the materials the import added (Assign… in 3D Print)
};

// Each distinct text once, in the order first seen; one that came more
// than once ends with how often it came, "(4 times)".
QStringList distinctProblems(const QStringList& problems);

// `file` is the imported file's name, `into` the import's target
// (`design`, `user` or `project`), `result` the command's answer.
// `keys` gives the plain names of the problems' keys (none: as reported).
PrintImportSummary printImportSummary(const QString& file, const QString& into, const QJsonObject& result,
                                      const PrintKeyTable* keys = nullptr);

// The file dialog's filter of Import from Slicer (3D Print, the print managers;
// mitcad#704): the files `import_print_profile` reads.
QString slicerFileFilter();

} // namespace mitcad
