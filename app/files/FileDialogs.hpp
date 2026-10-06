// SPDX-License-Identifier: MIT
#pragma once

#include <optional>

#include <QJsonObject>
#include <QPair>
#include <QString>
#include <QStringList>
#include <QVector>

class QWidget;

namespace mitcad {

// The dialogs of the File menu's import and export (U6).

// What Export writes.
struct ExportChoice {
  QString path;
  QString format;     // step, iges, stl, obj, brep, dxf, 3mf
  QString schema;     // STEP: ap214, ap242
  QString unit;       // STEP, IGES: mm (the default), cm, m, in, ft
  QString refinement; // STL, OBJ, 3MF: low, medium, high
  bool ascii = false; // STL
  bool selectedOnly = false;
  QString sketch; // DXF: the sketch's uid
  // Bodies (mitcad#19): design (where the design shows them, the
  // default) or component (each in its component's coordinates); empty
  // when the design has no occurrences to choose by.
  QString coordinates;
};

// Export: the file type, the bodies (all or the selected ones),
// the unit of a STEP or IGES file (millimetres unless chosen otherwise,
// P11), the mesh refinement or the sketch, and the file. `selectedBodies`: how
// many bodies the selection stands for; `sketches`: (uid, name); `sketch`
// the one to offer first; `occurrences`: the design places bodies by
// occurrences, so that the bodies' coordinates can be chosen.
std::optional<ExportChoice> askExport(QWidget* parent, const QString& path, int selectedBodies,
                                      const QVector<QPair<QString, QString>>& sketches,
                                      const QString& sketch, bool occurrences);

// Insert DXF: the plane of the new sketch (`planes`: value, label; none:
// into the edited sketch), the unit of a drawing that names none, and
// (P9) where it goes, by its origin, lower left corner or centre, and
// which of its layers. `info`: the model's `dxf_info` of the file.
struct DxfChoice {
  QString plane;
  QString unit;
  double x = 0.0; // mm: the sketch point the drawing's origin goes to
  double y = 0.0;
  QStringList layers; // empty: all
};
std::optional<DxfChoice> askDxfInsert(QWidget* parent, const QString& file,
                                      const QVector<QPair<QString, QString>>& planes, const QJsonObject& info);

// Insert Mesh: millimetres per unit of the file.
std::optional<double> askMeshUnit(QWidget* parent, const QString& file);

// Insert Component: linked (true) or a copy (false).
std::optional<bool> askLinked(QWidget* parent, const QString& file);

// The report of an .f3d import: what came in with its history, what as
// the file's bodies, and the bodies compared with the file's.
void showImportReport(QWidget* parent, const QString& file, const QJsonObject& result, int bodies);

// A line for the log: "a parametric, b partial, c fallback, d skipped".
QString importCounts(const QJsonObject& result);

// Where an .f3d import was stopped (T1e), for the log: "at Extrude5 (item
// 12)", "after the last item"; empty when it ran to its end.
QString importStop(const QJsonObject& result);

} // namespace mitcad
