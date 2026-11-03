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
  QString format;     // step, iges, stl, obj, brep, dxf, 3mf, usdz, html (a web page)
  QString schema;     // STEP: ap214, ap242
  QString unit;       // STEP, IGES: mm (the default), cm, m, in, ft
  QString refinement; // STL, OBJ, 3MF, USDZ, web page: low, medium, high
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
// P11), the mesh refinement or the sketch, and the file. `stem`: the file
// proposed, without its extension (the folder and the name); the type's
// extension is added. The type, the refinement and the STL format start as
// the last export chose them (mitcad#1164, settings `export/*`); a sketch's
// DXF does not replace the type remembered for the bodies. `selectedBodies`:
// how many bodies the selection stands for; `sketches`: (uid, name);
// `sketch` the one to offer first, which starts the dialog as DXF (a sketch
// edited or selected); `occurrences`: the design places bodies by
// occurrences, so that the bodies' coordinates can be chosen.
std::optional<ExportChoice> askExport(QWidget* parent, const QString& stem, int selectedBodies,
                                      const QVector<QPair<QString, QString>>& sketches,
                                      const QString& sketch, bool occurrences);

// The type Export starts with (mitcad#1164): an index into its list of
// types (STEP AP214, STEP AP242, IGES, STL, OBJ, BRep, DXF, 3MF, USDZ, web
// page). `remembered`: the type the bodies were last exported as ("step",
// "step-ap242", "3mf", …; empty or unknown: STEP AP214); with `sketch` (a
// sketch edited or selected) DXF, which only holds sketches; a remembered
// DXF only while the design has `sketches`.
int exportStartType(const QString& remembered, bool sketch, bool sketches);
// The settings key of a type index ("step", "step-ap242", "3mf", …).
QString exportTypeKey(int index);

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
  // Insert SVG (mitcad#473): x, y are where the drawing's lower left
  // corner goes; the scale, and stroked paths as bands of their width.
  double scale = 1.0;
  bool strokeOutline = false;
};
std::optional<DxfChoice> askDxfInsert(QWidget* parent, const QString& file,
                                      const QVector<QPair<QString, QString>>& planes, const QJsonObject& info);
// Insert SVG (mitcad#473): the plane of a new sketch (as Insert DXF), the
// scale, where the lower left corner goes and the stroke bands. `info`:
// the model's `svg_info` of the file.
std::optional<DxfChoice> askSvgInsert(QWidget* parent, const QString& file,
                                      const QVector<QPair<QString, QString>>& planes, const QJsonObject& info);

// Insert Mesh: millimetres per unit of the file, and whether the mesh
// bodies are converted to solids by Convert Mesh's Faceted Solid
// (mitcad#1082) with its weld tolerance (mm) and merge angle (degrees);
// the choice to convert is remembered.
struct MeshInsertChoice {
  double unitMillimetres = 1.0;
  bool convert = false;
  double tolerance = 0.01;
  double angleDegrees = 0.5;
};
std::optional<MeshInsertChoice> askMeshInsert(QWidget* parent, const QString& file);

// Insert Component: linked (true) or a copy (false).
std::optional<bool> askLinked(QWidget* parent, const QString& file);

// File > Open or Import of an .iam assembly (mitcad#507): Parts with
// their design history (true, the default) or the parts' stored bodies
// only (false); the choice is remembered (iamPartsHistory). Nothing when
// cancelled.
std::optional<bool> askIamImport(QWidget* parent, const QString& file);

// The report of an .f3d import: what came in with its history, what as
// the file's bodies, and the bodies compared with the file's.
void showImportReport(QWidget* parent, const QString& file, const QJsonObject& result, int bodies);

// A line for the log: "a parametric, b partial, c fallback, d skipped".
QString importCounts(const QJsonObject& result);

// Where an .f3d import was stopped (T1e), for the log: "at Extrude5 (item
// 12)", "after the last item"; empty when it ran to its end.
QString importStop(const QJsonObject& result);

} // namespace mitcad
