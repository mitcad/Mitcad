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
  QString sketch;  // DXF: the sketch's uid
  QString dxfUnit; // DXF: the file's length unit, the design's unless chosen (mitcad#1408)
  // DXF: also the sketch's construction curves and center lines, on the
  // layer construction (mitcad#1445); off unless chosen, remembered.
  bool dxfConstruction = false;
  // DXF: also the sketch's free-standing points, as POINT entities on the
  // layer POINTS (mitcad#1963: hole marks); on unless unchecked, remembered.
  bool dxfPoints = true;
  // Bodies (mitcad#19): design (where the design shows them, the
  // default) or component (each in its component's coordinates); empty
  // when the design has no occurrences to choose by. "frame" (mitcad#1775):
  // in the frame of the coordinate system `frame` (its uid).
  QString coordinates;
  QString frame;
  // 3MF (mitcad#1679), as Send to Slicer writes the design's print plates
  // (mitcad#1050): `plates` empty writes the bodies; "one_file" every plate
  // in the one file, "file_per_plate" a file per plate beside the path,
  // "plate" the plate `plate` (its id) alone.
  QString plates;
  QString plate;
  // 3MF: the slicer whose print settings go with the bodies (the model's
  // `dialect`: bambu_studio, orcaslicer, qidi_studio, prusaslicer, cura);
  // empty: none.
  QString dialect;
  // 3MF: on the print bed (`on_bed`: each piece on its print bed face,
  // else the automatic orientation's; implied by plates), centred on the
  // plate and dropped onto the bed (`center`, `drop`).
  bool onBed = false;
  bool center = true;
  bool drop = true;
};

// What Export offers a 3MF file of (mitcad#1679): the design's print plates
// (id, name; none: no plate row) and the slicer the dialog starts with (the
// design's when it has plates, as Send to Slicer's; empty: none).
struct ExportPlates {
  QVector<QPair<QString, QString>> plates;
  QString dialect;
};

// The dialects Export offers for a 3MF file, as the model's `dialect`
// takes them; the first, empty, is none.
QStringList exportDialects();
// The plates mode Export starts with (mitcad#1679): none without plates,
// else every plate in one file, or a file per plate for a slicer that keeps
// one plate (Cura).
QString exportStartPlates(bool hasPlates, const QString& dialect);

// Export: the file type, the bodies (all or the selected ones),
// the unit of a STEP or IGES file (millimetres unless chosen otherwise,
// P11), the mesh refinement or the sketch, and the file. `stem`: the file
// proposed, without its extension (the folder and the name); the type's
// extension is added. The type, the refinement, the STL format and the DXF
// file's "Include construction geometry" (mitcad#1445) start as the last
// export chose them (mitcad#1164, settings `export/*`); a sketch's
// DXF does not replace the type remembered for the bodies. `selectedBodies`:
// how many bodies the selection stands for; `sketches`: (uid, name);
// `sketch` the one to offer first, which starts the dialog as DXF (a sketch
// edited or selected); `occurrences`: the design places bodies by
// occurrences, so that the bodies' coordinates can be chosen;
// `designUnit`: the design's length unit, a DXF file's first (mitcad#1408).
// A 3MF file (mitcad#1679) offers the design's print plates of `plates`
// (all in one file, a file per plate, one plate, or the bodies without
// plates), the slicer, On the print bed, Center on Plate and Drop to Bed.
// `frames` (uid, name): the design's coordinate systems, which Coordinates
// offers beside the design's and the components' (mitcad#1775).
std::optional<ExportChoice> askExport(QWidget* parent, const QString& stem, int selectedBodies,
                                      const QVector<QPair<QString, QString>>& sketches,
                                      const QString& sketch, bool occurrences, const QString& designUnit,
                                      const ExportPlates& plates = {},
                                      const QVector<QPair<QString, QString>>& frames = {});

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

// Insert Points (mitcad#2232): the plane of a new sketch (as Insert DXF;
// none: into the edited sketch), a spline or points, and for plain points
// the scale and where the origin goes, for an airfoil (`info`'s
// detection, switchable off) the chord length, the angle of attack, where
// the leading edge goes and whether a line closes a blunt trailing edge.
// Lengths are asked in the design's unit (`unit`, `unitMillimetres` mm)
// and returned in mm. `info`: the model's `points_info` of the file.
struct PointsChoice {
  QString plane;
  bool asPoints = false;
  bool airfoil = false;
  double scale = 1.0;     // the file's numbers in the design's unit, times this
  double x = 0.0;         // mm: where the origin (an airfoil's leading edge) goes
  double y = 0.0;
  double chord = 100.0;   // mm
  double angle = 0.0;     // degrees, positive raises the leading edge
  bool closeTrailingEdge = true;
};
std::optional<PointsChoice> askPointsInsert(QWidget* parent, const QString& file,
                                            const QVector<QPair<QString, QString>>& planes,
                                            const QJsonObject& info, const QString& unit,
                                            double unitMillimetres);

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

// Insert Component: linked, the whole design or chosen parts of it
// (mitcad#2130), or a copy.
enum class InsertLink { Whole, ChooseParts, Copy };
std::optional<InsertLink> askLinked(QWidget* parent, const QString& file);

// Choose Parts… and Choose Linked Parts… (mitcad#2130): the occurrences of
// a design and the bodies of its root, as the `link_parts` query answers
// them (`tree`), each with a check box; an occurrence ticked ticks those in
// it, and one unticked inside a ticked one leaves that one partly ticked
// (it then brings only what is ticked in it). With `offerWhole`, a Whole
// Design button links the whole design again (`whole`).
struct LinkPartsChoice {
  bool whole = false;
  // `parts` of insert_component and set_link_parts: occurrence paths and
  // root body uids.
  QJsonObject parts;
};
std::optional<LinkPartsChoice> chooseLinkParts(QWidget* parent, const QString& title, const QJsonObject& tree,
                                               bool offerWhole);

// Insert Component of a file outside the design's project (mitcad#1719): a
// link to it would not be followed when the design opens again. Copy into
// the project and link (the default) the whole design or, with Choose parts…
// ticked, chosen parts of the copy (mitcad#2137); insert as a copy; or
// nothing when cancelled. `inProject`: the design is in a project (else the
// rule is its folder).
enum class OutsideLinkChoice { CopyIntoProject, CopyIntoProjectChooseParts, Copy };
std::optional<OutsideLinkChoice> askOutsideLink(QWidget* parent, const QString& file, bool inProject);

// Import or Open of a STEP file with an assembly (mitcad#1274): its parts
// as components with an occurrence per instance ("components", the
// default) or one base feature ("base"); nothing when cancelled. Its
// "Find repeated parts" box (mitcad#1277, off by default) goes to
// `repeatedParts`.
std::optional<QString> askStepStructure(QWidget* parent, const QString& file, bool* repeatedParts = nullptr);

// Import of a STEP, IGES or BRep file into a design with content
// (mitcad#1629): into a new component named after the file (true, the
// default) or straight into the active component (false), as import_file's
// `wrap`; nothing when cancelled.
std::optional<bool> askImportPlace(QWidget* parent, const QString& file);

// Import of design files of other formats (.ipt, .iam, .f3d, .FCStd;
// mitcad#1894) while a design with content is open, or of several at once:
// into this design as components (the default) or each as a new design.
// In a project with versions (`inProject`), its Linked box (on by default,
// remembered) saves each imported design in the project as a design of its
// own and links it; else, or unchecked, each is a copy. Nothing when
// cancelled. Logs "Where to Import dialog opened: <files>" and "Where to
// Import: this design (copy|linked)", "Where to Import: new design" or
// "Where to Import: cancelled".
struct DesignImportPlace {
  bool intoDesign = true;
  bool linked = false;
};
std::optional<DesignImportPlace> askDesignImportPlace(QWidget* parent, const QStringList& files, bool inProject);

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
