// SPDX-License-Identifier: MIT
#pragma once

// The tools of a drawing's sheet (mitcad#707, app/COMMANDS.md "Drawings"):
// declarative definitions as the feature commands have them. Each tool says
// what the sheet waits for (a click, a line, a circle, a drag out of a
// view, picks of model edges on a view), the hint the sheet shows
// meanwhile, and its options (drop-downs, check boxes, texts) that the
// workspace's panel shows; `command` turns the picks and the options into
// the model's command (core/model-api/src/commands.md, "Drawings in the
// design" and "Annotations on drawing views"). Plain Qt Core: app.unit
// tests it.

#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QList>
#include <QPair>
#include <QPointF>
#include <QString>
#include <QStringList>
#include <QVector>

namespace mitcad::drawings {

// What the sheet waits for while a tool is active.
enum class SheetPick {
  None,      // the selection: views and annotations are picked and dragged
  Point,     // a click on the sheet (a base view's middle, a text note's place)
  OutOfView, // a drag out of a view, or a click beside the view picked before (a projected view)
  Line,      // two clicks on a view (a section line, a break's two sides)
  Circle,    // a click on a view and a second at the radius (a detail)
  Picks,     // clicks on a view's edges, circles and corners (the model finds what each picks)
  ViewThenPlace, // a click on a view, then where what it makes goes (a table)
  Segment,   // two clicks anywhere on the sheet (a line drawn outside the model, mitcad#729)
  ArcPoints, // three clicks anywhere: an arc's centre, start and end (mitcad#729)
};

// An option of a tool's panel.
struct ToolOption {
  enum class Kind { Choice, Check, Text };
  QString id;
  QString label;
  Kind kind = Kind::Choice;
  QList<QPair<QString, QString>> choices; // (value, label)
  QString defaultValue;                   // a check box's "true" or "false"
};

// What the sheet gave a tool: the view it was used on (0: none), the
// points (sheet millimetres) and a circle's radius on the sheet. A tool
// that places its view after its line or circle (`DrawingToolDef::place`)
// has that click last: a section's third point, a detail's second. An
// annotation's tool gets the model's picks the workspace found for its
// clicks (`drawing_pick`), in order, and its placing click as its point.
// A base view along the 3D view's Current View gets the camera from the
// workspace (`{"toward_eye", "up"}`, the model's form). `subject` is
// unused by the tools since mitcad#725 (a base view's Shows option says
// what it shows) and kept for the order of the brace initialisers.
struct ToolPicks {
  int view = 0;
  QVector<QPointF> points;
  double radius = 0.0;
  QJsonObject subject;
  QJsonArray picks;
  QJsonObject camera;
};

// An annotation's tool that picks until a click finds nothing (a baseline
// or chain of dimensions): its pickCount.
constexpr int kOpenPicks = -1;

struct DrawingToolDef {
  QString id;      // "drawing.section_view"
  QString name;    // "Section View"
  QString icon;    // app/icons
  QString tooltip; // one short line
  SheetPick pick = SheetPick::None;
  QString hint; // what the sheet waits for, shown over it
  QVector<ToolOption> options;
  // The model's command for the picks and the options; empty when the
  // picks are not enough yet.
  QJsonObject (*command)(const QString& drawing, int sheet, const ToolPicks& picks,
                         const QHash<QString, QString>& options) = nullptr;
  // After its line, circle or picks, one more click where the view or
  // the annotation goes, with this hint meanwhile.
  bool place = false;
  QString placeHint;
  // An annotation's tool (SheetPick::Picks): how many picks it takes with
  // these options (kOpenPicks: until a click picks nothing, which places
  // them), and the model's annotation type its clicks pick for (the
  // `drawing_pick` query's `type`).
  int (*pickCount)(const QHash<QString, QString>& options) = nullptr;
  QString (*annotationType)(const QHash<QString, QString>& options) = nullptr;
  // On the bar as its icon alone (the annotations' tools), its name in its
  // tooltip.
  bool iconOnly = false;
};

// The tools: Base View, Projected View, Section View, Detail View, Break;
// then the annotations' tools: Dimension, Ordinate, Baseline, Center
// Mark, Centerline, Line, Arc, Center Pattern, Edge Extension
// (mitcad#729), Text, Note, Datum Label, Feature Control Frame, Surface
// Finish; then the tables' (mitcad#707 phase 3): Hole Table, Parts List,
// Balloon. The bar shows them by their groups (`toolGroup`).
//
// Line and Arc (mitcad#729) draw outside the model, on the sheet or in the
// view under their first click (their Drawn option), which they then move,
// scale and turn with; Text does too (on the sheet unless chosen).
const QVector<DrawingToolDef>& drawingTools();
const DrawingToolDef* drawingTool(const QString& id);

// The groups of the bar, in its order, each under its caption
// (`toolGroupName`, mitcad#725): Create (the views' tools), Geometry
// (Center Mark, Centerline, Line, Arc, Center Pattern, Edge Extension;
// mitcad#729), Dimensions (Dimension, Ordinate, Baseline),
// Text (Text, Note), Symbols (Datum Label, Feature Control Frame, Surface
// Finish), Tables (Hole Table, Parts List, Balloon); the bar's Export
// group comes after them.
constexpr int kToolGroups = 6;
int toolGroup(const DrawingToolDef& tool);
QString toolGroupName(int group);
// The group's name as the window's ribbon has it (mitcad#1008: the
// DRAWING tab's groups), "CREATE" … "TABLES"; empty past the last.
QString toolGroupId(int group);

// The command search's other words for a tool (mitcad#725): "leader" for
// Note, "chain" for Baseline, "datum identifier" for Datum Label,
// "surface texture" for Surface Finish, "break view" for Break, "bom" for
// Parts List, "sketch" for Line and Arc, "bolt circle" for Center Pattern,
// "virtual sharp" for Edge Extension (mitcad#729); besides "drawing" and
// "sheet", which every tool has.
QStringList toolKeywords(const QString& id);

// What a base view shows (mitcad#725): its Shows list for a drawing of
// `component` ("C0" the root's), named `name` (the design's or the
// component's name), from the `components` query (its occurrence tree)
// and the `bodies` query: first the whole component ("design"), then each
// occurrence in it ("occurrence:O2/O5", a path from the component, named
// by its names "Arm:1/Pin:2"), then each body of its own ("body:F2.b0", by
// its name). (value, label) pairs.
QList<QPair<QString, QString>> subjectChoices(const QString& component, const QString& name,
                                              const QJsonObject& components, const QJsonArray& bodies);
// A Shows choice as the model's `subject`; empty for the whole component
// (the model's default).
QJsonObject subjectOf(const QString& choice);
// A view's `subject` as its Shows choice; empty when no choice is it
// (several bodies, or a body where an occurrence places it).
QString subjectChoice(const QJsonObject& subject);

// An option's value: the chosen one (`options`: the tool's own, by option
// id), else its default.
QString optionValue(const DrawingToolDef& tool, const QHash<QString, QString>& options, const QString& id);

// The standard scales the panels offer (ISO 5455), largest first, with
// "auto" (Its Parent's: the parent's, twice it for a detail) first; with
// `fit`, "fit" (Fit the Sheet, a base view's) in its place.
QList<QPair<QString, QString>> scaleChoices(bool fit = false);

// The sides a projected view can be on, as the model names them, with
// their labels.
QList<QPair<QString, QString>> sideChoices();

// A view's properties as the selection's panel edits them: the
// edit_drawing_view command for one changed field ("scale", "hidden_lines",
// "tangent_edges", "shaded", "label", "direction", "aligned", "side",
// "letter", "hatch", "flip", "radius" (sheet mm), "center" ("x y", sheet
// mm), "subject" (the model's JSON as text), "center_marks",
// "retrieved_dimensions", "rotation" (degrees counter-clockwise,
// mitcad#729)) of view `view`, whose style is `style` now (a style is
// changed as a whole); empty for a value the field cannot take.
QJsonObject viewEdit(const QString& drawing, int view, const QString& field, const QString& value,
                     const QJsonObject& style);

// An annotation's properties as its panel edits them: the
// edit_drawing_annotation command for one changed field ("along",
// "prefix", "before", "after" (texts of the value's format; an empty
// prefix is none, "auto" the kind's own), "letter", "text", "process" and
// "finish_text" (a surface finish's, with the finish as it is now:
// `finish`), "height" (a text note's, mm); a table's "columns" (the keys,
// separated by commas), "sort" (empty: its own), "descending", "origin",
// "tags", "levels" and "title" (empty: its own)); empty for a value the
// field cannot take. Since mitcad#729 "height" is every annotation's own
// letters' height (empty: the drawing's again), "pen" a line's or an
// arc's, and "length" an edge extension's reach beyond its edge (mm,
// its `line_offset`; empty: 10).
QJsonObject annotationEdit(const QString& drawing, int annotation, const QString& field, const QString& value,
                           const QJsonObject& finish = QJsonObject());

// The drawing's own decimals and letters' height (mitcad#729) as the
// sheet's panel edits them: the edit_drawing command for "decimals"
// ("auto": none, else 0 to 6) or "text_height" (mm; empty: none); empty
// for a value the field cannot take.
QJsonObject drawingDefaultsEdit(const QString& drawing, const QString& field, const QString& value);

// The pens a line or an arc drawn outside the model takes, as the model
// names them, with their labels (mitcad#729).
QList<QPair<QString, QString>> penChoices();

// The annotation's name in the panel and the undo history: "Dimension",
// "Center Mark", "Note", … for the model's type.
QString annotationName(const QString& type);

} // namespace mitcad::drawings
