// SPDX-License-Identifier: MIT
#include "DrawingTools.hpp"

#include <cmath>

#include <QCoreApplication>
#include <QJsonArray>
#include <QJsonDocument>
#include <QStringList>

#include "../framework/Numbers.hpp"

namespace mitcad::drawings {
namespace {

// The file's texts in the context DrawingTools, where lupdate records them
// too (mitcad#1472).
struct Text {
  Q_DECLARE_TR_FUNCTIONS(DrawingTools)
};

QJsonArray point(const QPointF& p) { return QJsonArray{p.x(), p.y()}; }

bool yes(const QString& value) { return value == QStringLiteral("true"); }

// A scale option: "auto" and "fit" leave it to the model (a base view's
// fits the sheet, another's follows its parent's).
void addScale(QJsonObject& command, const QString& scale) {
  if (!scale.isEmpty() && scale != QStringLiteral("auto") && scale != QStringLiteral("fit")) {
    command.insert(QStringLiteral("scale"), scale);
  }
}

QJsonObject view(const QString& drawing, int sheet, const QString& type) {
  return {{QStringLiteral("cmd"), QStringLiteral("add_drawing_view")},
          {QStringLiteral("drawing"), drawing},
          {QStringLiteral("sheet"), sheet},
          {QStringLiteral("type"), type}};
}

QJsonObject baseView(const QString& drawing, int sheet, const ToolPicks& picks,
                     const QHash<QString, QString>& options) {
  if (picks.points.isEmpty()) {
    return {};
  }
  const DrawingToolDef& tool = *drawingTool(QStringLiteral("drawing.base_view"));
  // Along the 3D view's Current View: its camera, which the workspace
  // gives; none, nothing to place.
  const QString direction = optionValue(tool, options, QStringLiteral("direction"));
  const bool current = direction == QStringLiteral("current");
  if (current && picks.camera.isEmpty()) {
    return {};
  }
  QJsonObject command = view(drawing, sheet, QStringLiteral("base"));
  command.insert(QStringLiteral("direction"), current ? QJsonValue(picks.camera) : QJsonValue(direction));
  command.insert(QStringLiteral("at"), point(picks.points.first()));
  const QJsonObject subject = subjectOf(optionValue(tool, options, QStringLiteral("subject")));
  if (!subject.isEmpty()) {
    command.insert(QStringLiteral("subject"), subject);
  }
  addScale(command, optionValue(tool, options, QStringLiteral("scale")));
  command.insert(QStringLiteral("style"),
                 QJsonObject{{QStringLiteral("hidden_lines"), yes(optionValue(tool, options, QStringLiteral("hidden_lines")))},
                             {QStringLiteral("tangent_edges"), yes(optionValue(tool, options, QStringLiteral("tangent_edges")))},
                             {QStringLiteral("shaded"), yes(optionValue(tool, options, QStringLiteral("shaded")))}});
  return command;
}

QJsonObject projectedView(const QString& drawing, int sheet, const ToolPicks& picks,
                          const QHash<QString, QString>& /*options*/) {
  if (picks.view <= 0 || picks.points.isEmpty()) {
    return {};
  }
  QJsonObject command = view(drawing, sheet, QStringLiteral("projected"));
  command.insert(QStringLiteral("parent"), picks.view);
  command.insert(QStringLiteral("at"), point(picks.points.last()));
  return command;
}

QJsonObject sectionView(const QString& drawing, int sheet, const ToolPicks& picks,
                        const QHash<QString, QString>& options) {
  if (picks.view <= 0 || picks.points.size() < 2) {
    return {};
  }
  const DrawingToolDef& tool = *drawingTool(QStringLiteral("drawing.section_view"));
  QJsonObject command = view(drawing, sheet, QStringLiteral("section"));
  command.insert(QStringLiteral("parent"), picks.view);
  command.insert(QStringLiteral("line"), QJsonArray{point(picks.points[0]), point(picks.points[1])});
  if (picks.points.size() > 2) {
    command.insert(QStringLiteral("at"), point(picks.points[2]));
  }
  command.insert(QStringLiteral("flip"), yes(optionValue(tool, options, QStringLiteral("flip"))));
  command.insert(QStringLiteral("hatch"), yes(optionValue(tool, options, QStringLiteral("hatch"))));
  const QString letter = optionValue(tool, options, QStringLiteral("letter")).trimmed();
  if (!letter.isEmpty()) {
    command.insert(QStringLiteral("letter"), letter);
  }
  return command;
}

QJsonObject detailView(const QString& drawing, int sheet, const ToolPicks& picks,
                       const QHash<QString, QString>& options) {
  if (picks.view <= 0 || picks.points.isEmpty() || !(picks.radius > 0.0)) {
    return {};
  }
  const DrawingToolDef& tool = *drawingTool(QStringLiteral("drawing.detail_view"));
  QJsonObject command = view(drawing, sheet, QStringLiteral("detail"));
  command.insert(QStringLiteral("parent"), picks.view);
  command.insert(QStringLiteral("center"), point(picks.points.first()));
  command.insert(QStringLiteral("radius"), picks.radius);
  if (picks.points.size() > 1) {
    command.insert(QStringLiteral("at"), point(picks.points[1]));
  }
  addScale(command, optionValue(tool, options, QStringLiteral("scale")));
  const QString letter = optionValue(tool, options, QStringLiteral("letter")).trimmed();
  if (!letter.isEmpty()) {
    command.insert(QStringLiteral("letter"), letter);
  }
  return command;
}

QJsonObject breakView(const QString& drawing, int /*sheet*/, const ToolPicks& picks,
                      const QHash<QString, QString>& options) {
  if (picks.view <= 0 || picks.points.size() < 2) {
    return {};
  }
  const DrawingToolDef& tool = *drawingTool(QStringLiteral("drawing.break"));
  return {{QStringLiteral("cmd"), QStringLiteral("edit_drawing_view")},
          {QStringLiteral("drawing"), drawing},
          {QStringLiteral("view"), picks.view},
          {QStringLiteral("add_break"),
           QJsonObject{{QStringLiteral("direction"), optionValue(tool, options, QStringLiteral("direction"))},
                       {QStringLiteral("from"), point(picks.points[0])},
                       {QStringLiteral("to"), point(picks.points[1])}}}};
}

// --- The annotations' tools (mitcad#707 phase 2) ---------------------------

// An annotation on the view its picks are on: its type, the model's picks
// the workspace found, and where its placing click put it.
QJsonObject annotationOn(const QString& drawing, const ToolPicks& picks, const QString& type) {
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("add_drawing_annotation")},
                      {QStringLiteral("drawing"), drawing},
                      {QStringLiteral("view"), picks.view},
                      {QStringLiteral("type"), type},
                      {QStringLiteral("picks"), picks.picks}};
  if (!picks.points.isEmpty()) {
    command.insert(QStringLiteral("at"), point(picks.points.last()));
  }
  return command;
}

// Whether the picks are there: on a view, `count` of them (kOpenPicks: two
// at least; kAutoPicks: one or two), and the placing click when the tool
// has one.
bool picked(const ToolPicks& picks, int count, bool place) {
  if (picks.view <= 0) {
    return false;
  }
  const qsizetype n = picks.picks.size();
  const bool enough = count == kOpenPicks   ? n >= 2
                      : count == kAutoPicks ? n == 1 || n == 2
                                            : n == count;
  if (!enough) {
    return false;
  }
  return !place || !picks.points.isEmpty();
}

QString toolOption(const char* tool, const QHash<QString, QString>& options, const char* id) {
  const DrawingToolDef* def = drawingTool(QString::fromLatin1(tool));
  return def != nullptr ? optionValue(*def, options, QString::fromLatin1(id)) : QString();
}

int onePick(const QHash<QString, QString>& /*options*/) { return 1; }
int twoPicks(const QHash<QString, QString>& /*options*/) { return 2; }
int openPicks(const QHash<QString, QString>& /*options*/) { return kOpenPicks; }

int dimensionPicks(const QHash<QString, QString>& options) {
  const QString type = toolOption("drawing.dimension", options, "type");
  if (type == QStringLiteral("auto")) {
    return kAutoPicks;
  }
  return type == QStringLiteral("radius") || type == QStringLiteral("diameter") ? 1 : 2;
}

QString dimensionType(const QHash<QString, QString>& options) {
  const QString type = toolOption("drawing.dimension", options, "type");
  if (type == QStringLiteral("auto")) {
    return QStringLiteral("dimension"); // the picks choose (mitcad#1272)
  }
  return type == QStringLiteral("aligned") ? QStringLiteral("linear") : type;
}

QString ordinateType(const QHash<QString, QString>& /*options*/) { return QStringLiteral("ordinate"); }
QString linearType(const QHash<QString, QString>& /*options*/) { return QStringLiteral("linear"); }
QString centerMarkType(const QHash<QString, QString>& /*options*/) { return QStringLiteral("center_mark"); }
QString centerlineType(const QHash<QString, QString>& /*options*/) { return QStringLiteral("centerline"); }
QString noteType(const QHash<QString, QString>& /*options*/) { return QStringLiteral("note"); }
QString datumType(const QHash<QString, QString>& /*options*/) { return QStringLiteral("datum"); }
QString frameType(const QHash<QString, QString>& /*options*/) { return QStringLiteral("frame"); }
QString finishType(const QHash<QString, QString>& /*options*/) { return QStringLiteral("finish"); }

QJsonObject dimension(const QString& drawing, int /*sheet*/, const ToolPicks& picks,
                      const QHash<QString, QString>& options) {
  if (!picked(picks, dimensionPicks(options), true)) {
    return {};
  }
  QJsonObject command = annotationOn(drawing, picks, dimensionType(options));
  if (toolOption("drawing.dimension", options, "type") == QStringLiteral("aligned")) {
    command.insert(QStringLiteral("along"), QStringLiteral("aligned"));
  }
  return command;
}

QJsonObject ordinate(const QString& drawing, int /*sheet*/, const ToolPicks& picks,
                     const QHash<QString, QString>& options) {
  if (!picked(picks, 2, true)) {
    return {};
  }
  QJsonObject command = annotationOn(drawing, picks, QStringLiteral("ordinate"));
  command.insert(QStringLiteral("along"), toolOption("drawing.ordinate", options, "along"));
  return command;
}

QJsonObject baseline(const QString& drawing, int /*sheet*/, const ToolPicks& picks,
                     const QHash<QString, QString>& options) {
  if (!picked(picks, kOpenPicks, false)) {
    return {};
  }
  QJsonObject command = annotationOn(drawing, picks, QStringLiteral("linear"));
  command.insert(QStringLiteral("series"), toolOption("drawing.baseline", options, "series"));
  command.insert(QStringLiteral("along"), toolOption("drawing.baseline", options, "along"));
  return command;
}

// Model Items (mitcad#1406): the sketch dimensions and the model's
// annotations switched on, each as its check box says, for the views the
// picks name by their ids (the selected ones; none: all the sheet's).
QJsonObject modelItems(const QString& drawing, int sheet, const ToolPicks& picks,
                       const QHash<QString, QString>& options) {
  const bool sketch = toolOption("drawing.model_items", options, "sketch_dimensions") == QStringLiteral("true");
  const bool annotations = toolOption("drawing.model_items", options, "model_annotations") == QStringLiteral("true");
  if (!sketch && !annotations) {
    return {};
  }
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("drawing_model_items")},
                      {QStringLiteral("drawing"), drawing},
                      {QStringLiteral("sheet"), sheet},
                      {QStringLiteral("sketch_dimensions"), sketch},
                      {QStringLiteral("model_annotations"), annotations}};
  QJsonArray views;
  for (const QJsonValue& view : picks.picks) {
    if (view.isDouble() && view.toInt() > 0) {
      views.append(view.toInt());
    }
  }
  if (!views.isEmpty()) {
    command.insert(QStringLiteral("views"), views);
  }
  return command;
}

QJsonObject centerMark(const QString& drawing, int /*sheet*/, const ToolPicks& picks,
                       const QHash<QString, QString>& /*options*/) {
  return picked(picks, 1, false) ? annotationOn(drawing, picks, QStringLiteral("center_mark")) : QJsonObject();
}

QJsonObject centerline(const QString& drawing, int /*sheet*/, const ToolPicks& picks,
                       const QHash<QString, QString>& /*options*/) {
  return picked(picks, 2, false) ? annotationOn(drawing, picks, QStringLiteral("centerline")) : QJsonObject();
}

// What is drawn outside the model (mitcad#729): in the view under the
// first click when the tool's Drawn option says so and there is one, else
// on the sheet.
QJsonObject drawnOutside(const QString& drawing, int sheet, const ToolPicks& picks, const QString& type,
                         const QString& attach) {
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("add_drawing_annotation")},
                      {QStringLiteral("drawing"), drawing},
                      {QStringLiteral("type"), type}};
  if (attach == QStringLiteral("view") && picks.view > 0) {
    command.insert(QStringLiteral("view"), picks.view);
  } else {
    command.insert(QStringLiteral("sheet"), sheet);
  }
  return command;
}

QJsonObject textNote(const QString& drawing, int sheet, const ToolPicks& picks, const QHash<QString, QString>& options) {
  const QString text = toolOption("drawing.text", options, "text");
  if (picks.points.isEmpty() || text.trimmed().isEmpty()) {
    return {};
  }
  QJsonObject command =
      drawnOutside(drawing, sheet, picks, QStringLiteral("text"), toolOption("drawing.text", options, "attach"));
  command.insert(QStringLiteral("at"), point(picks.points.first()));
  command.insert(QStringLiteral("text"), text);
  return command;
}

QJsonObject sketchLine(const QString& drawing, int sheet, const ToolPicks& picks,
                       const QHash<QString, QString>& options) {
  if (picks.points.size() < 2) {
    return {};
  }
  QJsonObject command =
      drawnOutside(drawing, sheet, picks, QStringLiteral("line"), toolOption("drawing.line", options, "attach"));
  command.insert(QStringLiteral("points"), QJsonArray{point(picks.points[0]), point(picks.points[1])});
  command.insert(QStringLiteral("width"), toolOption("drawing.line", options, "width"));
  command.insert(QStringLiteral("pattern"), toolOption("drawing.line", options, "pattern"));
  return command;
}

QJsonObject sketchArc(const QString& drawing, int sheet, const ToolPicks& picks,
                      const QHash<QString, QString>& options) {
  if (picks.points.size() < 3) {
    return {};
  }
  QJsonObject command =
      drawnOutside(drawing, sheet, picks, QStringLiteral("arc"), toolOption("drawing.arc", options, "attach"));
  command.insert(QStringLiteral("points"),
                 QJsonArray{point(picks.points[0]), point(picks.points[1]), point(picks.points[2])});
  command.insert(QStringLiteral("width"), toolOption("drawing.arc", options, "width"));
  command.insert(QStringLiteral("pattern"), toolOption("drawing.arc", options, "pattern"));
  return command;
}

QString centerPatternType(const QHash<QString, QString>& /*options*/) { return QStringLiteral("center_pattern"); }
QString edgeExtensionType(const QHash<QString, QString>& /*options*/) { return QStringLiteral("edge_extension"); }

int extensionPicks(const QHash<QString, QString>& options) {
  return toolOption("drawing.edge_extension", options, "edges") == QStringLiteral("one") ? 1 : 2;
}

QJsonObject centerPattern(const QString& drawing, int /*sheet*/, const ToolPicks& picks,
                          const QHash<QString, QString>& /*options*/) {
  return picked(picks, 1, false) ? annotationOn(drawing, picks, QStringLiteral("center_pattern")) : QJsonObject();
}

QJsonObject edgeExtension(const QString& drawing, int /*sheet*/, const ToolPicks& picks,
                          const QHash<QString, QString>& options) {
  return picked(picks, extensionPicks(options), false) ? annotationOn(drawing, picks, QStringLiteral("edge_extension"))
                                                       : QJsonObject();
}

QJsonObject leaderNote(const QString& drawing, int /*sheet*/, const ToolPicks& picks,
                       const QHash<QString, QString>& options) {
  const QString text = toolOption("drawing.note", options, "text");
  if (!picked(picks, 1, true) || text.trimmed().isEmpty()) {
    return {};
  }
  QJsonObject command = annotationOn(drawing, picks, QStringLiteral("note"));
  command.insert(QStringLiteral("text"), text);
  return command;
}

QJsonObject datumLabel(const QString& drawing, int /*sheet*/, const ToolPicks& picks,
                       const QHash<QString, QString>& options) {
  if (!picked(picks, 1, true)) {
    return {};
  }
  QJsonObject command = annotationOn(drawing, picks, QStringLiteral("datum"));
  const QString letter = toolOption("drawing.datum", options, "letter").trimmed().toUpper();
  if (!letter.isEmpty()) {
    command.insert(QStringLiteral("letter"), letter);
  }
  // Auto: a dot on a face, a triangle on an edge (mitcad#1296).
  const QString terminator = toolOption("drawing.datum", options, "terminator");
  if (terminator == QStringLiteral("triangle") || terminator == QStringLiteral("dot")) {
    command.insert(QStringLiteral("terminator"), terminator);
  }
  return command;
}

QJsonObject controlFrame(const QString& drawing, int /*sheet*/, const ToolPicks& picks,
                         const QHash<QString, QString>& options) {
  // The frame's tolerance comes from its dialog, as the model's JSON.
  const QJsonObject frame = QJsonDocument::fromJson(options.value(QStringLiteral("frame")).toUtf8()).object();
  if (!picked(picks, 1, true) || frame.isEmpty()) {
    return {};
  }
  QJsonObject command = annotationOn(drawing, picks, QStringLiteral("frame"));
  command.insert(QStringLiteral("frame"), frame);
  return command;
}

QJsonObject surfaceFinish(const QString& drawing, int /*sheet*/, const ToolPicks& picks,
                          const QHash<QString, QString>& options) {
  if (!picked(picks, 1, false)) {
    return {};
  }
  QJsonObject command = annotationOn(drawing, picks, QStringLiteral("finish"));
  QJsonObject finish{{QStringLiteral("process"), toolOption("drawing.finish", options, "process")}};
  const QString text = toolOption("drawing.finish", options, "text").trimmed();
  if (!text.isEmpty()) {
    finish.insert(QStringLiteral("text"), text);
  }
  command.insert(QStringLiteral("finish"), finish);
  return command;
}

// --- The tables' tools (mitcad#707 phase 3) --------------------------------

QString balloonType(const QHash<QString, QString>& /*options*/) { return QStringLiteral("balloon"); }

// A table on the view clicked, where the placing click put it, with its
// settings.
QJsonObject tableOn(const QString& drawing, const ToolPicks& picks, const QString& type, const QJsonObject& table) {
  if (picks.view <= 0 || picks.points.size() < 2) {
    return {};
  }
  return {{QStringLiteral("cmd"), QStringLiteral("add_drawing_annotation")},
          {QStringLiteral("drawing"), drawing},
          {QStringLiteral("view"), picks.view},
          {QStringLiteral("type"), type},
          {QStringLiteral("at"), point(picks.points.last())},
          {QStringLiteral("table"), table}};
}

QJsonObject holeTable(const QString& drawing, int /*sheet*/, const ToolPicks& picks,
                      const QHash<QString, QString>& options) {
  return tableOn(drawing, picks, QStringLiteral("hole_table"),
                 {{QStringLiteral("origin"), toolOption("drawing.hole_table", options, "origin")},
                  {QStringLiteral("tags"), toolOption("drawing.hole_table", options, "tags")}});
}

QJsonObject partsList(const QString& drawing, int /*sheet*/, const ToolPicks& picks,
                      const QHash<QString, QString>& options) {
  QJsonObject command = tableOn(drawing, picks, QStringLiteral("parts_list"),
                                {{QStringLiteral("levels"), toolOption("drawing.parts_list", options, "levels")},
                                 {QStringLiteral("group_identical"), yes(toolOption("drawing.parts_list", options, "group_identical"))}});
  // Its balloons with it, on by default (mitcad#725).
  if (!command.isEmpty() && yes(toolOption("drawing.parts_list", options, "balloons"))) {
    command.insert(QStringLiteral("balloons"), true);
  }
  return command;
}

QJsonObject balloonOn(const QString& drawing, int /*sheet*/, const ToolPicks& picks,
                      const QHash<QString, QString>& /*options*/) {
  return picked(picks, 1, true) ? annotationOn(drawing, picks, QStringLiteral("balloon")) : QJsonObject();
}

void addTableTools(QVector<DrawingToolDef>& tools) {
  const auto choice = [](const QString& id, const QString& label, const QList<QPair<QString, QString>>& choices,
                         const QString& value) {
    return ToolOption{id, label, ToolOption::Kind::Choice, choices, value};
  };
  tools.append({QStringLiteral("drawing.hole_table"), Text::tr("Hole Table"), QStringLiteral("drawing-hole-table"),
                Text::tr("Lists the holes a view sees end on, tagged in the view"), SheetPick::ViewThenPlace,
                Text::tr("Click the view whose holes the table lists"),
                {choice(QStringLiteral("origin"), Text::tr("Origin"),
                        {{QStringLiteral("corner"), Text::tr("Lower Left Corner")},
                         {QStringLiteral("model"), Text::tr("Component Origin")}},
                        QStringLiteral("corner")),
                 choice(QStringLiteral("tags"), Text::tr("Tags"),
                        {{QStringLiteral("by_size"), Text::tr("By Size")}, {QStringLiteral("numbers"), Text::tr("Numbers")}},
                        QStringLiteral("by_size"))},
                &holeTable, true, Text::tr("Click where the table goes"), nullptr, nullptr, true});
  tools.append({QStringLiteral("drawing.parts_list"), Text::tr("Bill of Materials"), QStringLiteral("drawing-parts-list"),
                Text::tr("Lists the components a view shows, numbered"), SheetPick::ViewThenPlace,
                Text::tr("Click the view whose components the list shows"),
                {choice(QStringLiteral("levels"), Text::tr("Rows"),
                        {{QStringLiteral("top"), Text::tr("First Level")}, {QStringLiteral("parts"), Text::tr("Components Only")}},
                        QStringLiteral("top")),
                 ToolOption{QStringLiteral("group_identical"), Text::tr("Group identical parts"), ToolOption::Kind::Check, {},
                            QStringLiteral("false")},
                 ToolOption{QStringLiteral("balloons"), Text::tr("Balloons"), ToolOption::Kind::Check, {},
                            QStringLiteral("true")}},
                &partsList, true, Text::tr("Click where the list goes"), nullptr, nullptr, true});
  tools.append({QStringLiteral("drawing.balloon"), Text::tr("Balloon"), QStringLiteral("drawing-balloon"),
                Text::tr("Shows a component's number in the bill of materials"), SheetPick::Picks,
                Text::tr("Click an edge of the component on a view"), {}, &balloonOn, true,
                Text::tr("Click where the balloon goes"), &onePick, &balloonType, true});
}

void addAnnotationTools(QVector<DrawingToolDef>& tools) {
  const auto choice = [](const QString& id, const QString& label, const QList<QPair<QString, QString>>& choices,
                         const QString& value) {
    return ToolOption{id, label, ToolOption::Kind::Choice, choices, value};
  };
  const auto text = [](const QString& id, const QString& label) {
    return ToolOption{id, label, ToolOption::Kind::Text, {}, QString()};
  };
  const QList<QPair<QString, QString>> axes = {{QStringLiteral("horizontal"), Text::tr("Horizontal")},
                                               {QStringLiteral("vertical"), Text::tr("Vertical")}};
  tools.append({QStringLiteral("drawing.dimension"), Text::tr("Dimension"), QStringLiteral("dimension"),
                Text::tr("Dimensions edges, circles and corners of a view"), SheetPick::Picks,
                Text::tr("Click the edges, circles or corners to dimension on a view"),
                // Auto: the picks choose (mitcad#1272); the others
                // override it.
                {choice(QStringLiteral("type"), Text::tr("Type"),
                        {{QStringLiteral("auto"), Text::tr("Auto")},
                         {QStringLiteral("linear"), Text::tr("Linear")},
                         {QStringLiteral("aligned"), Text::tr("Aligned")},
                         {QStringLiteral("angle"), Text::tr("Angle")},
                         {QStringLiteral("radius"), Text::tr("Radius")},
                         {QStringLiteral("diameter"), Text::tr("Diameter")}},
                        QStringLiteral("auto"))},
                &dimension, true, Text::tr("Click where the dimension goes (Shift: no snap)"), &dimensionPicks, &dimensionType, true});
  tools.append({QStringLiteral("drawing.ordinate"), Text::tr("Ordinate"), QStringLiteral("drawing-ordinate"),
                Text::tr("Dimensions a point from an origin along the view"), SheetPick::Picks,
                Text::tr("Click the origin, then the point to dimension"),
                {choice(QStringLiteral("along"), Text::tr("Direction"), axes, QStringLiteral("horizontal"))}, &ordinate,
                true, Text::tr("Click where the value goes (Shift: no snap)"), &twoPicks, &ordinateType, true});
  tools.append({QStringLiteral("drawing.baseline"), Text::tr("Baseline"), QStringLiteral("drawing-baseline"),
                Text::tr("Dimensions several points from the first, or each from the last"), SheetPick::Picks,
                Text::tr("Click the points to dimension, then a free place for the dimension lines"),
                {choice(QStringLiteral("series"), Text::tr("Type"),
                        {{QStringLiteral("baseline"), Text::tr("Baseline")}, {QStringLiteral("chain"), Text::tr("Chain")}},
                        QStringLiteral("baseline")),
                 choice(QStringLiteral("along"), Text::tr("Direction"), axes, QStringLiteral("horizontal"))},
                &baseline, false, QString(), &openPicks, &linearType, true});
  // The model's items in the selected views, or all the sheet's
  // (mitcad#1406): no picks, its panel's Apply runs it.
  const auto check = [](const QString& id, const QString& label) {
    return ToolOption{id, label, ToolOption::Kind::Check, {}, QStringLiteral("true")};
  };
  tools.append({QStringLiteral("drawing.model_items"), Text::tr("Model Items"), QStringLiteral("drawing-model-items"),
                Text::tr("Brings the model's dimensions and annotations into views"), SheetPick::None,
                Text::tr("Select views (none: all the sheet's), then Apply"),
                {check(QStringLiteral("sketch_dimensions"), Text::tr("Sketch Dimensions")),
                 check(QStringLiteral("model_annotations"), Text::tr("Model Annotations"))},
                &modelItems, false, QString(), nullptr, nullptr, false});
  tools.append({QStringLiteral("drawing.center_mark"), Text::tr("Center Mark"), QStringLiteral("drawing-center-mark"),
                Text::tr("Marks a circle's center, or a round face's centerline"), SheetPick::Picks,
                Text::tr("Click a circle, or a round face seen from the side"), {}, &centerMark, false, QString(),
                &onePick, &centerMarkType, true});
  tools.append({QStringLiteral("drawing.centerline"), Text::tr("Centerline"), QStringLiteral("centerline"),
                Text::tr("Draws a centerline through two circles or between two edges"), SheetPick::Picks,
                Text::tr("Click two circles, or two parallel edges"), {}, &centerline, false, QString(), &twoPicks,
                &centerlineType, true});
  // Drawn outside the model, on the sheet or in a view (mitcad#729).
  const auto attach = [&choice](const QString& value) {
    return choice(QStringLiteral("attach"), Text::tr("Drawn"),
                  {{QStringLiteral("view"), Text::tr("In the View Clicked")}, {QStringLiteral("sheet"), Text::tr("On the Sheet")}},
                  value);
  };
  tools.append({QStringLiteral("drawing.line"), Text::tr("Line"), QStringLiteral("line"),
                Text::tr("Draws a line on the sheet or in a view, outside the design"), SheetPick::Segment,
                Text::tr("Click the line's two ends"),
                {attach(QStringLiteral("view")),
                 choice(QStringLiteral("width"), Text::tr("Width"), widthChoices(), QStringLiteral("thin")),
                 choice(QStringLiteral("pattern"), Text::tr("Pattern"), patternChoices(), QStringLiteral("continuous"))},
                &sketchLine, false, QString(), nullptr, nullptr, true});
  tools.append({QStringLiteral("drawing.arc"), Text::tr("Arc"), QStringLiteral("arc-center"),
                Text::tr("Draws an arc on the sheet or in a view, outside the design"), SheetPick::ArcPoints,
                Text::tr("Click the arc's center, its start, then its end (counter-clockwise)"),
                {attach(QStringLiteral("view")),
                 choice(QStringLiteral("width"), Text::tr("Width"), widthChoices(), QStringLiteral("thin")),
                 choice(QStringLiteral("pattern"), Text::tr("Pattern"), patternChoices(), QStringLiteral("continuous"))},
                &sketchArc, false, QString(), nullptr, nullptr, true});
  tools.append({QStringLiteral("drawing.center_pattern"), Text::tr("Center Pattern"), QStringLiteral("circular-pattern"),
                Text::tr("Marks every hole of a pattern from one of them"), SheetPick::Picks,
                Text::tr("Click one hole of the pattern, seen end on"), {}, &centerPattern, false, QString(), &onePick,
                &centerPatternType, true});
  tools.append({QStringLiteral("drawing.edge_extension"), Text::tr("Edge Extension"), QStringLiteral("extend"),
                Text::tr("Runs straight edges on to where they meet"), SheetPick::Picks,
                Text::tr("Click the straight edges to run on"),
                {choice(QStringLiteral("edges"), Text::tr("Edges"),
                        {{QStringLiteral("two"), Text::tr("Two, to Their Corner")},
                         {QStringLiteral("one"), Text::tr("One, Both Ways")}},
                        QStringLiteral("two"))},
                &edgeExtension, false, QString(), &extensionPicks, &edgeExtensionType, true});
  tools.append({QStringLiteral("drawing.text"), Text::tr("Text"), QStringLiteral("drawing-text"),
                Text::tr("Writes a note on the sheet or in a view"), SheetPick::Point, Text::tr("Click where the text goes"),
                {text(QStringLiteral("text"), Text::tr("Text")), attach(QStringLiteral("sheet"))}, &textNote, false,
                QString(), nullptr, nullptr, true});
  tools.append({QStringLiteral("drawing.note"), Text::tr("Note"), QStringLiteral("drawing-note"),
                Text::tr("Writes a note on a leader to an edge or a point"), SheetPick::Picks,
                Text::tr("Click the edge or point the note's arrow points at"), {text(QStringLiteral("text"), Text::tr("Text"))},
                &leaderNote, true, Text::tr("Click where the note's text goes"), &onePick, &noteType, true});
  tools.append({QStringLiteral("drawing.datum"), Text::tr("Datum Label"), QStringLiteral("drawing-datum"),
                Text::tr("Labels a face or an axis as a datum"), SheetPick::Picks,
                Text::tr("Click the edge of the face, the round face, or inside a face seen face on, that is the datum"),
                {text(QStringLiteral("letter"), Text::tr("Letter")),
                 // mitcad#1296: Auto, a dot on a face and a triangle on
                 // an edge.
                 choice(QStringLiteral("terminator"), Text::tr("Terminator"),
                        {{QStringLiteral("auto"), Text::tr("Auto")},
                         {QStringLiteral("triangle"), Text::tr("Triangle")},
                         {QStringLiteral("dot"), Text::tr("Dot")}},
                        QStringLiteral("auto"))},
                &datumLabel, true, Text::tr("Click where its label goes"), &onePick, &datumType, true});
  tools.append({QStringLiteral("drawing.frame"), Text::tr("Feature Control Frame"), QStringLiteral("drawing-frame"),
                Text::tr("Adds a geometric tolerance to a face or an axis"), SheetPick::Picks,
                Text::tr("Click the edge of the face, or the round face, to tolerance"), {}, &controlFrame, true,
                Text::tr("Click where the frame goes"), &onePick, &frameType, true});
  tools.append({QStringLiteral("drawing.finish"), Text::tr("Surface Finish"), QStringLiteral("drawing-finish"),
                Text::tr("Puts a surface finish symbol on a face"), SheetPick::Picks,
                Text::tr("Click the edge of the face the symbol stands on"),
                {choice(QStringLiteral("process"), Text::tr("Surface"),
                        {{QStringLiteral("any"), Text::tr("Any Process")},
                         {QStringLiteral("removal"), Text::tr("Material Removed")},
                         {QStringLiteral("no_removal"), Text::tr("No Material Removed")}},
                        QStringLiteral("removal")),
                 text(QStringLiteral("text"), Text::tr("Requirement"))},
                &surfaceFinish, false, QString(), &onePick, &finishType, true});
}

QVector<DrawingToolDef> makeTools() {
  const QList<QPair<QString, QString>> directions = {
      {QStringLiteral("front"), Text::tr("Front")},
      {QStringLiteral("top"), Text::tr("Top")},
      {QStringLiteral("right"), Text::tr("Right")},
      {QStringLiteral("back"), Text::tr("Back")},
      {QStringLiteral("bottom"), Text::tr("Bottom")},
      {QStringLiteral("left"), Text::tr("Left")},
      {QStringLiteral("iso"), Text::tr("Isometric")},
      {QStringLiteral("iso_top_left"), Text::tr("Isometric, Top Left")},
      {QStringLiteral("iso_bottom_right"), Text::tr("Isometric, Bottom Right")},
      {QStringLiteral("iso_bottom_left"), Text::tr("Isometric, Bottom Left")},
      // The 3D view's direction and up as they are (mitcad#725).
      {QStringLiteral("current"), Text::tr("Current View")},
  };
  const auto choice = [](const QString& id, const QString& label, const QList<QPair<QString, QString>>& choices,
                         const QString& value) {
    return ToolOption{id, label, ToolOption::Kind::Choice, choices, value};
  };
  const auto check = [](const QString& id, const QString& label, bool value) {
    return ToolOption{id, label, ToolOption::Kind::Check, {}, value ? QStringLiteral("true") : QStringLiteral("false")};
  };
  const auto text = [](const QString& id, const QString& label) {
    return ToolOption{id, label, ToolOption::Kind::Text, {}, QString()};
  };
  // A detail's scale follows its parent's, twice it, as the view panel
  // says.
  QList<QPair<QString, QString>> detailScales = scaleChoices();
  detailScales.first().second = Text::tr("Twice Its Parent's");
  QVector<DrawingToolDef> tools;
  tools.append({QStringLiteral("drawing.base_view"), Text::tr("Base View"), QStringLiteral("drawing-base-view"),
                Text::tr("Places a view of the design on the sheet"), SheetPick::Point,
                Text::tr("Click where the view goes"),
                // The whole component; the workspace lists its occurrences
                // and bodies by name (subjectChoices).
                {choice(QStringLiteral("subject"), Text::tr("Shows"), {{QStringLiteral("design"), Text::tr("The Design")}},
                        QStringLiteral("design")),
                 choice(QStringLiteral("direction"), Text::tr("Orientation"), directions, QStringLiteral("front")),
                 choice(QStringLiteral("scale"), Text::tr("Scale"), scaleChoices(true), QStringLiteral("fit")),
                 check(QStringLiteral("hidden_lines"), Text::tr("Hidden Edges"), false),
                 check(QStringLiteral("tangent_edges"), Text::tr("Tangent Edges"), false),
                 check(QStringLiteral("shaded"), Text::tr("Shaded"), false)},
                &baseView, false, QString(), nullptr, nullptr, false});
  tools.append({QStringLiteral("drawing.projected_view"), Text::tr("Projected View"),
                QStringLiteral("drawing-projected-view"), Text::tr("Projects a view from one beside it"),
                SheetPick::OutOfView, Text::tr("Drag out of a view, or click where the next view goes"), {},
                &projectedView, false, QString(), nullptr, nullptr, false});
  tools.append({QStringLiteral("drawing.section_view"), Text::tr("Section View"), QStringLiteral("drawing-section-view"),
                Text::tr("Cuts a view along a line drawn on it"), SheetPick::Line,
                Text::tr("Click the two ends of the section line on a view (Shift: no snap)"),
                {text(QStringLiteral("letter"), Text::tr("Letter")), check(QStringLiteral("hatch"), Text::tr("Hatching"), true),
                 check(QStringLiteral("flip"), Text::tr("Look the Other Way"), false)},
                &sectionView, true, Text::tr("Click where the section view goes"), nullptr, nullptr, false});
  tools.append({QStringLiteral("drawing.detail_view"), Text::tr("Detail View"), QStringLiteral("drawing-detail-view"),
                Text::tr("Enlarges a circle of a view"), SheetPick::Circle,
                Text::tr("Click the middle of the detail on a view, then its edge"),
                {text(QStringLiteral("letter"), Text::tr("Letter")),
                 choice(QStringLiteral("scale"), Text::tr("Scale"), detailScales, QStringLiteral("auto"))},
                &detailView, true, Text::tr("Click where the detail view goes"), nullptr, nullptr, false});
  tools.append({QStringLiteral("drawing.break"), Text::tr("Break"), QStringLiteral("drawing-break"),
                Text::tr("Shortens a view by taking a band out"), SheetPick::Line,
                Text::tr("Click the two sides of the band on a view (Shift: no snap)"),
                {choice(QStringLiteral("direction"), Text::tr("Direction"),
                        {{QStringLiteral("horizontal"), Text::tr("Horizontal")}, {QStringLiteral("vertical"), Text::tr("Vertical")}},
                        QStringLiteral("horizontal"))},
                &breakView, false, QString(), nullptr, nullptr, false});
  addAnnotationTools(tools);
  addTableTools(tools);
  return tools;
}

} // namespace

const QVector<DrawingToolDef>& drawingTools() {
  static const QVector<DrawingToolDef> tools = makeTools();
  return tools;
}

const DrawingToolDef* drawingTool(const QString& id) {
  for (const DrawingToolDef& tool : drawingTools()) {
    if (tool.id == id) {
      return &tool;
    }
  }
  return nullptr;
}

int toolGroup(const DrawingToolDef& tool) {
  const QString& id = tool.id;
  if (id == QStringLiteral("drawing.center_mark") || id == QStringLiteral("drawing.centerline") ||
      id == QStringLiteral("drawing.line") || id == QStringLiteral("drawing.arc") ||
      id == QStringLiteral("drawing.center_pattern") || id == QStringLiteral("drawing.edge_extension")) {
    return 1;
  }
  if (id == QStringLiteral("drawing.dimension") || id == QStringLiteral("drawing.ordinate") ||
      id == QStringLiteral("drawing.baseline") || id == QStringLiteral("drawing.model_items")) {
    return 2;
  }
  if (id == QStringLiteral("drawing.text") || id == QStringLiteral("drawing.note")) {
    return 3;
  }
  if (id == QStringLiteral("drawing.datum") || id == QStringLiteral("drawing.frame") ||
      id == QStringLiteral("drawing.finish")) {
    return 4;
  }
  if (id == QStringLiteral("drawing.hole_table") || id == QStringLiteral("drawing.parts_list") ||
      id == QStringLiteral("drawing.balloon")) {
    return 5;
  }
  return 0;
}

QString toolGroupName(int group) {
  switch (group) {
  case 0:
    return Text::tr("Create");
  case 1:
    return Text::tr("Geometry");
  case 2:
    return Text::tr("Dimensions");
  case 3:
    return Text::tr("Text");
  case 4:
    return Text::tr("Symbols");
  case 5:
    return Text::tr("Tables");
  default:
    return QString();
  }
}

QString toolGroupId(int group) {
  static const QStringList ids = {QStringLiteral("CREATE"), QStringLiteral("GEOMETRY"), QStringLiteral("DIMENSIONS"),
                                  QStringLiteral("TEXT"),   QStringLiteral("SYMBOLS"),  QStringLiteral("TABLES")};
  return ids.value(group);
}

QStringList toolKeywords(const QString& id) {
  QStringList out{QStringLiteral("drawing"), QStringLiteral("sheet")};
  if (id == QStringLiteral("drawing.note")) {
    out << QStringLiteral("leader");
  } else if (id == QStringLiteral("drawing.baseline")) {
    out << QStringLiteral("chain");
  } else if (id == QStringLiteral("drawing.datum")) {
    out << QStringLiteral("datum identifier");
  } else if (id == QStringLiteral("drawing.finish")) {
    out << QStringLiteral("surface texture");
  } else if (id == QStringLiteral("drawing.break")) {
    out << QStringLiteral("break view");
  } else if (id == QStringLiteral("drawing.parts_list")) {
    out << QStringLiteral("bom") << QStringLiteral("bill of materials");
  } else if (id == QStringLiteral("drawing.line") || id == QStringLiteral("drawing.arc")) {
    out << QStringLiteral("sketch");
  } else if (id == QStringLiteral("drawing.center_pattern")) {
    out << QStringLiteral("bolt circle") << QStringLiteral("centered pattern");
  } else if (id == QStringLiteral("drawing.edge_extension")) {
    out << QStringLiteral("virtual sharp") << QStringLiteral("intersection");
  } else if (id == QStringLiteral("drawing.model_items")) {
    out << QStringLiteral("model annotations") << QStringLiteral("retrieve dimensions");
  }
  return out;
}

namespace {

// The occurrences of an occurrence tree (the `components` query's, each
// `uid`, `name`, `children`), depth first, as Shows choices: their paths
// of uids and of names from the tree's top.
void addOccurrences(const QJsonArray& tree, const QString& uids, const QString& names,
                    QList<QPair<QString, QString>>& out) {
  for (const QJsonValue& value : tree) {
    const QJsonObject occurrence = value.toObject();
    const QString uid = occurrence.value(QStringLiteral("uid")).toString();
    if (uid.isEmpty()) {
      continue;
    }
    const QString path = uids.isEmpty() ? uid : uids + QLatin1Char('/') + uid;
    const QString name = occurrence.value(QStringLiteral("name")).toString();
    const QString named = names.isEmpty() ? name : names + QLatin1Char('/') + name;
    out.append({QStringLiteral("occurrence:") + path, named});
    addOccurrences(occurrence.value(QStringLiteral("children")).toArray(), path, named, out);
  }
}

// The children of the first occurrence of `component` in a tree, or
// nothing when it is not placed.
QJsonArray childrenOf(const QJsonArray& tree, const QString& component, bool* found) {
  for (const QJsonValue& value : tree) {
    const QJsonObject occurrence = value.toObject();
    const QJsonArray children = occurrence.value(QStringLiteral("children")).toArray();
    if (occurrence.value(QStringLiteral("component")).toString() == component) {
      *found = true;
      return children;
    }
    const QJsonArray inner = childrenOf(children, component, found);
    if (*found) {
      return inner;
    }
  }
  return {};
}

} // namespace

QList<QPair<QString, QString>> subjectChoices(const QString& component, const QString& name,
                                              const QJsonObject& components, const QJsonArray& bodies) {
  const bool root = component.isEmpty() || component == QStringLiteral("C0");
  QList<QPair<QString, QString>> out;
  out.append({QStringLiteral("design"), name.isEmpty() ? Text::tr("The Design") : name});
  const QJsonArray tree = components.value(QStringLiteral("occurrences")).toArray();
  bool found = false;
  addOccurrences(root ? tree : childrenOf(tree, component, &found), QString(), QString(), out);
  for (const QJsonValue& value : bodies) {
    const QJsonObject body = value.toObject();
    const QString owner = body.value(QStringLiteral("component")).toString();
    const bool own = root ? owner.isEmpty() || owner == QStringLiteral("C0") : owner == component;
    const QString uid = body.value(QStringLiteral("uid")).toString();
    if (own && !uid.isEmpty()) {
      const QString bodyName = body.value(QStringLiteral("name")).toString();
      out.append({QStringLiteral("body:") + uid, bodyName.isEmpty() ? uid : bodyName});
    }
  }
  return out;
}

QJsonObject subjectOf(const QString& choice) {
  if (choice.startsWith(QStringLiteral("occurrence:"))) {
    return {{QStringLiteral("type"), QStringLiteral("occurrence")},
            {QStringLiteral("occurrence"), choice.mid(QStringLiteral("occurrence:").size())}};
  }
  if (choice.startsWith(QStringLiteral("body:"))) {
    return {{QStringLiteral("type"), QStringLiteral("bodies")},
            {QStringLiteral("bodies"),
             QJsonArray{QJsonObject{{QStringLiteral("body"), choice.mid(QStringLiteral("body:").size())}}}}};
  }
  return {};
}

QString subjectChoice(const QJsonObject& subject) {
  const QString type = subject.value(QStringLiteral("type")).toString();
  if (type.isEmpty() || type == QStringLiteral("design")) {
    return QStringLiteral("design");
  }
  if (type == QStringLiteral("occurrence")) {
    return QStringLiteral("occurrence:") + subject.value(QStringLiteral("occurrence")).toString();
  }
  const QJsonArray bodies = subject.value(QStringLiteral("bodies")).toArray();
  if (type == QStringLiteral("bodies") && bodies.size() == 1) {
    const QJsonObject body = bodies.first().toObject();
    if (!body.contains(QStringLiteral("occurrence"))) {
      return QStringLiteral("body:") + body.value(QStringLiteral("body")).toString();
    }
  }
  return QString();
}

namespace {

using NamedBodies = QHash<QString, QVector<QPair<QString, QString>>>;

// The rows of an occurrence tree (the `components` query's) below the
// path `uids`: each occurrence, then its component's bodies and its own
// occurrences one level deeper.
void addSubjectRows(const QJsonArray& tree, const QString& uids, int depth, const NamedBodies& byComponent,
                    QVector<SubjectRow>& out) {
  for (const QJsonValue& value : tree) {
    const QJsonObject occurrence = value.toObject();
    const QString uid = occurrence.value(QStringLiteral("uid")).toString();
    if (uid.isEmpty()) {
      continue;
    }
    const QString path = uids.isEmpty() ? uid : uids + QLatin1Char('/') + uid;
    out.append(SubjectRow{occurrence.value(QStringLiteral("name")).toString(), QString(), path, depth});
    for (const auto& [bodyUid, bodyName] : byComponent.value(occurrence.value(QStringLiteral("component")).toString())) {
      out.append(SubjectRow{bodyName, bodyUid, path, depth + 1});
    }
    addSubjectRows(occurrence.value(QStringLiteral("children")).toArray(), path, depth + 1, byComponent, out);
  }
}

} // namespace

QVector<SubjectRow> subjectRows(const QString& component, const QJsonObject& components, const QJsonArray& bodies) {
  const bool root = component.isEmpty() || component == QStringLiteral("C0");
  // Each component's bodies by name (the root's under "C0").
  NamedBodies byComponent;
  for (const QJsonValue& value : bodies) {
    const QJsonObject body = value.toObject();
    const QString uid = body.value(QStringLiteral("uid")).toString();
    if (uid.isEmpty()) {
      continue;
    }
    QString owner = body.value(QStringLiteral("component")).toString();
    if (owner.isEmpty()) {
      owner = QStringLiteral("C0");
    }
    const QString bodyName = body.value(QStringLiteral("name")).toString();
    byComponent[owner].append({uid, bodyName.isEmpty() ? uid : bodyName});
  }
  QVector<SubjectRow> out;
  for (const auto& [bodyUid, bodyName] : byComponent.value(root ? QStringLiteral("C0") : component)) {
    out.append(SubjectRow{bodyName, bodyUid, QString(), 0});
  }
  const QJsonArray tree = components.value(QStringLiteral("occurrences")).toArray();
  bool found = false;
  addSubjectRows(root ? tree : childrenOf(tree, component, &found), QString(), 0, byComponent, out);
  return out;
}

bool subjectShows(const QJsonObject& subject, const QString& body, const QString& occurrence) {
  const QString type = subject.value(QStringLiteral("type")).toString();
  if (type == QStringLiteral("occurrence")) {
    const QString path = subject.value(QStringLiteral("occurrence")).toString();
    return !path.isEmpty() && (occurrence == path || occurrence.startsWith(path + QLatin1Char('/')));
  }
  if (type == QStringLiteral("bodies")) {
    for (const QJsonValue& value : subject.value(QStringLiteral("bodies")).toArray()) {
      const QJsonObject chosen = value.toObject();
      // Without an occurrence: wherever the design places it.
      if (chosen.value(QStringLiteral("body")).toString() == body &&
          (!chosen.contains(QStringLiteral("occurrence")) ||
           chosen.value(QStringLiteral("occurrence")).toString() == occurrence)) {
        return true;
      }
    }
    return false;
  }
  // The whole component, as the 3D view shows it (an unrecorded subject).
  return true;
}

QJsonObject chosenSubject(const QVector<QPair<QString, QString>>& chosen) {
  QJsonArray bodies;
  for (const auto& [bodyUid, path] : chosen) {
    QJsonObject entry{{QStringLiteral("body"), bodyUid}};
    if (!path.isEmpty()) {
      entry.insert(QStringLiteral("occurrence"), path);
    }
    bodies.append(entry);
  }
  return {{QStringLiteral("type"), QStringLiteral("bodies")}, {QStringLiteral("bodies"), bodies}};
}

QString optionValue(const DrawingToolDef& tool, const QHash<QString, QString>& options, const QString& id) {
  const auto found = options.constFind(id);
  if (found != options.constEnd()) {
    return found.value();
  }
  for (const ToolOption& option : tool.options) {
    if (option.id == id) {
      return option.defaultValue;
    }
  }
  return QString();
}

QList<QPair<QString, QString>> scaleChoices(bool fit) {
  QList<QPair<QString, QString>> out;
  if (fit) {
    out.append({QStringLiteral("fit"), Text::tr("Fit the Sheet")});
  } else {
    out.append({QStringLiteral("auto"), Text::tr("Its Parent's")});
  }
  for (const char* scale : {"10:1", "5:1", "2:1", "1:1", "1:2", "1:5", "1:10", "1:20", "1:50", "1:100"}) {
    out.append({QString::fromLatin1(scale), QString::fromLatin1(scale)});
  }
  return out;
}

std::optional<double> scaleRatio(const QString& text, QString* normalized) {
  const QStringList sides = text.split(QLatin1Char(':'));
  if (sides.size() != 2) {
    return std::nullopt;
  }
  const std::optional<double> a = parsePlainNumber(sides[0]);
  const std::optional<double> b = parsePlainNumber(sides[1]);
  if (!a || !b || !(*a > 0.0) || !(*b > 0.0) || !std::isfinite(*a / *b) || !(*a / *b > 0.0)) {
    return std::nullopt;
  }
  if (normalized != nullptr) {
    *normalized = QString::number(*a, 'g', 12) + QLatin1Char(':') + QString::number(*b, 'g', 12);
  }
  return *a / *b;
}

std::optional<QString> scaleEntry(const QString& text, const QList<QPair<QString, QString>>& choices) {
  const QString typed = text.trimmed();
  for (const auto& [value, label] : choices) {
    if (typed.compare(label, Qt::CaseInsensitive) == 0 || typed.compare(value, Qt::CaseInsensitive) == 0) {
      return value;
    }
  }
  QString ratio;
  if (!scaleRatio(typed, &ratio)) {
    return std::nullopt;
  }
  return ratio;
}

QList<QPair<QString, QString>> sideChoices() {
  return {{QStringLiteral("right"), Text::tr("Right")},
          {QStringLiteral("left"), Text::tr("Left")},
          {QStringLiteral("above"), Text::tr("Above")},
          {QStringLiteral("below"), Text::tr("Below")},
          {QStringLiteral("above_right"), Text::tr("Above Right")},
          {QStringLiteral("above_left"), Text::tr("Above Left")},
          {QStringLiteral("below_right"), Text::tr("Below Right")},
          {QStringLiteral("below_left"), Text::tr("Below Left")}};
}

QJsonObject viewEdit(const QString& drawing, int view, const QString& field, const QString& value,
                     const QJsonObject& style) {
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("edit_drawing_view")},
                      {QStringLiteral("drawing"), drawing},
                      {QStringLiteral("view"), view}};
  if (field == QStringLiteral("scale")) {
    command.insert(QStringLiteral("scale"), value == QStringLiteral("auto") ? QStringLiteral("parent") : value);
  } else if (field == QStringLiteral("hidden_lines") || field == QStringLiteral("tangent_edges") ||
             field == QStringLiteral("shaded")) {
    QJsonObject changed;
    for (const char* key : {"hidden_lines", "tangent_edges", "shaded", "polygonal"}) {
      const QString name = QString::fromLatin1(key);
      changed.insert(name, style.value(name).toBool(false));
    }
    changed.insert(field, yes(value));
    command.insert(QStringLiteral("style"), changed);
  } else if (field == QStringLiteral("aligned") || field == QStringLiteral("hatch") ||
             field == QStringLiteral("flip") || field == QStringLiteral("center_marks") ||
             field == QStringLiteral("retrieved_dimensions") || field == QStringLiteral("model_annotations")) {
    command.insert(field, yes(value));
  } else if (field == QStringLiteral("radius")) {
    bool number = false;
    const double radius = value.toDouble(&number);
    if (!number || !(radius > 0.0)) {
      return {};
    }
    command.insert(field, radius);
  } else if (field == QStringLiteral("center")) {
    // "x y", sheet mm.
    const QStringList parts = value.split(QLatin1Char(' '), Qt::SkipEmptyParts);
    bool x = false;
    bool y = false;
    const double cx = parts.size() == 2 ? parts[0].toDouble(&x) : 0.0;
    const double cy = parts.size() == 2 ? parts[1].toDouble(&y) : 0.0;
    if (!x || !y) {
      return {};
    }
    command.insert(field, QJsonArray{cx, cy});
  } else if (field == QStringLiteral("subject")) {
    const QJsonObject subject = QJsonDocument::fromJson(value.toUtf8()).object();
    if (subject.isEmpty()) {
      return {};
    }
    command.insert(field, subject);
  } else if (field == QStringLiteral("rotation")) {
    // Degrees, counter-clockwise (mitcad#729).
    bool number = false;
    const double degrees = value.toDouble(&number);
    if (!number || !std::isfinite(degrees)) {
      return {};
    }
    command.insert(field, degrees);
  } else {
    command.insert(field, value);
  }
  return command;
}

QJsonObject annotationEdit(const QString& drawing, int annotation, const QString& field, const QString& value,
                           const QJsonObject& finish) {
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("edit_drawing_annotation")},
                      {QStringLiteral("drawing"), drawing},
                      {QStringLiteral("annotation"), annotation}};
  if (field == QStringLiteral("along")) {
    command.insert(field, value);
  } else if (field == QStringLiteral("prefix")) {
    // "auto": the kind's own (Ø, R), back to the default.
    command.insert(QStringLiteral("format"),
                   QJsonObject{{field, value == QStringLiteral("auto") ? QJsonValue(QJsonValue::Null) : QJsonValue(value)}});
  } else if (field == QStringLiteral("before") || field == QStringLiteral("after")) {
    command.insert(QStringLiteral("format"), QJsonObject{{field, value.trimmed()}});
  } else if (field == QStringLiteral("replace")) {
    // A text in place of the value and its tolerance; empty: the measured
    // value again (mitcad#1292).
    const QString replace = value.trimmed();
    command.insert(QStringLiteral("format"),
                   QJsonObject{{field, replace.isEmpty() ? QJsonValue(QJsonValue::Null) : QJsonValue(replace)}});
  } else if (field == QStringLiteral("letter")) {
    const QString letter = value.trimmed().toUpper();
    if (letter.isEmpty()) {
      return {};
    }
    command.insert(field, letter);
  } else if (field == QStringLiteral("text")) {
    if (value.trimmed().isEmpty()) {
      return {};
    }
    command.insert(field, value);
  } else if (field == QStringLiteral("process") || field == QStringLiteral("finish_text")) {
    QJsonObject changed = finish;
    if (field == QStringLiteral("process")) {
      changed.insert(QStringLiteral("process"), value);
    } else if (value.trimmed().isEmpty()) {
      changed.remove(QStringLiteral("text"));
    } else {
      changed.insert(QStringLiteral("text"), value.trimmed());
    }
    command.insert(QStringLiteral("finish"), changed);
  } else if (field == QStringLiteral("height")) {
    // Empty: the drawing's letters again (mitcad#729).
    if (value.trimmed().isEmpty()) {
      command.insert(field, QJsonValue(QJsonValue::Null));
      return command;
    }
    bool number = false;
    const double height = value.toDouble(&number);
    if (!number || !(height > 0.0)) {
      return {};
    }
    command.insert(field, height);
  } else if (field == QStringLiteral("pen") || field == QStringLiteral("width") || field == QStringLiteral("pattern") ||
             field == QStringLiteral("terminator")) {
    // A line's pen, its width or its pattern (mitcad#1447); a datum
    // label's terminator (mitcad#1296).
    command.insert(field, value);
  } else if (field == QStringLiteral("length")) {
    // An edge extension's reach beyond its edge's ends, mm (mitcad#729);
    // empty: its own default.
    if (value.trimmed().isEmpty()) {
      command.insert(QStringLiteral("line_offset"), QJsonValue(QJsonValue::Null));
      return command;
    }
    bool number = false;
    const double length = value.toDouble(&number);
    if (!number || !(length > 0.0)) {
      return {};
    }
    command.insert(QStringLiteral("line_offset"), length);
  } else if (field == QStringLiteral("columns")) {
    // A table's columns, in order: none would leave the type's own.
    QJsonArray columns;
    for (const QString& column : value.split(QLatin1Char(','), Qt::SkipEmptyParts)) {
      if (!column.trimmed().isEmpty()) {
        columns.append(column.trimmed());
      }
    }
    if (columns.isEmpty()) {
      return {};
    }
    command.insert(QStringLiteral("table"), QJsonObject{{field, columns}});
  } else if (field == QStringLiteral("sort") || field == QStringLiteral("title")) {
    // Empty: the table's own.
    const QString text = value.trimmed();
    command.insert(QStringLiteral("table"),
                   QJsonObject{{field, text.isEmpty() ? QJsonValue(QJsonValue::Null) : QJsonValue(text)}});
  } else if (field == QStringLiteral("descending") || field == QStringLiteral("group_identical")) {
    command.insert(QStringLiteral("table"), QJsonObject{{field, yes(value)}});
  } else if (field == QStringLiteral("origin") || field == QStringLiteral("tags") || field == QStringLiteral("levels")) {
    command.insert(QStringLiteral("table"), QJsonObject{{field, value}});
  } else {
    return {};
  }
  return command;
}

QString annotationName(const QString& type) {
  if (type == QStringLiteral("center_mark")) {
    return Text::tr("Center Mark");
  }
  if (type == QStringLiteral("centerline")) {
    return Text::tr("Centerline");
  }
  if (type == QStringLiteral("note")) {
    return Text::tr("Note");
  }
  if (type == QStringLiteral("text")) {
    return Text::tr("Text");
  }
  if (type == QStringLiteral("datum")) {
    return Text::tr("Datum Label");
  }
  if (type == QStringLiteral("frame")) {
    return Text::tr("Feature Control Frame");
  }
  if (type == QStringLiteral("finish")) {
    return Text::tr("Surface Finish");
  }
  if (type == QStringLiteral("hole_table")) {
    return Text::tr("Hole Table");
  }
  if (type == QStringLiteral("parts_list")) {
    return Text::tr("Bill of Materials");
  }
  if (type == QStringLiteral("balloon")) {
    return Text::tr("Balloon");
  }
  if (type == QStringLiteral("line")) {
    return Text::tr("Line");
  }
  if (type == QStringLiteral("arc")) {
    return Text::tr("Arc");
  }
  if (type == QStringLiteral("center_pattern")) {
    return Text::tr("Center Pattern");
  }
  if (type == QStringLiteral("edge_extension")) {
    return Text::tr("Edge Extension");
  }
  return Text::tr("Dimension");
}

QJsonObject drawingDefaultsEdit(const QString& drawing, const QString& field, const QString& value) {
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("edit_drawing")}, {QStringLiteral("drawing"), drawing}};
  const QString text = value.trimmed();
  if (field == QStringLiteral("decimals")) {
    if (text == QStringLiteral("auto") || text.isEmpty()) {
      command.insert(field, QJsonValue(QJsonValue::Null));
      return command;
    }
    bool number = false;
    const int decimals = text.toInt(&number);
    if (!number || decimals < 0 || decimals > 6) {
      return {};
    }
    command.insert(field, decimals);
    return command;
  }
  if (field == QStringLiteral("text_height")) {
    if (text.isEmpty()) {
      command.insert(field, QJsonValue(QJsonValue::Null));
      return command;
    }
    bool number = false;
    const double height = text.toDouble(&number);
    if (!number || !(height > 0.0) || height > 100.0) {
      return {};
    }
    command.insert(field, height);
    return command;
  }
  return {};
}

QList<QPair<QString, QString>> widthChoices() {
  return {{QStringLiteral("thin"), Text::tr("Thin")},
          {QStringLiteral("thick"), Text::tr("Thick")},
          {QStringLiteral("extra_thick"), Text::tr("Extra Thick")}};
}

QList<QPair<QString, QString>> patternChoices() {
  return {{QStringLiteral("continuous"), Text::tr("Continuous")},
          {QStringLiteral("dashed"), Text::tr("Dashed")},
          {QStringLiteral("chain"), Text::tr("Center Line")}};
}

QStringList projectionLines(const QJsonArray& projections) {
  QStringList lines;
  for (const QJsonValue& value : projections) {
    const QJsonObject note = value.toObject();
    const QString how = note.value(QStringLiteral("cached")).toBool() ? QStringLiteral("from the cache")
                        : note.value(QStringLiteral("polygonal")).toBool()
                            ? QStringLiteral("projected from a triangulation")
                            : QStringLiteral("projected exactly");
    lines << QStringLiteral("Drawing projection of view %1: %2 bodies, %3 ms, %4")
                 .arg(note.value(QStringLiteral("view")).toInt())
                 .arg(note.value(QStringLiteral("bodies")).toInt())
                 .arg(note.value(QStringLiteral("ms")).toDouble(), 0, 'f', 1)
                 .arg(how);
  }
  return lines;
}

std::optional<LineAngleSnap> snapLineAngle(const QPointF& from, const QPointF& to, double within) {
  const double dx = to.x() - from.x();
  const double dy = to.y() - from.y();
  const double length = std::hypot(dx, dy);
  if (!(length > 1e-9)) {
    return std::nullopt;
  }
  constexpr double kStep = 45.0;
  constexpr double kDegrees = 57.29577951308232; // 180 / pi
  const double angle = std::atan2(dy, dx) * kDegrees;
  const double snapped = std::round(angle / kStep) * kStep;
  if (std::abs(angle - snapped) > within) {
    return std::nullopt;
  }
  const double radians = snapped / kDegrees;
  LineAngleSnap out;
  // Exactly along the axes: no rounding off them.
  const double c = std::abs(std::fmod(snapped, 180.0)) == 90.0 ? 0.0 : std::cos(radians);
  const double s = std::abs(std::fmod(snapped, 180.0)) == 0.0 ? 0.0 : std::sin(radians);
  out.to = QPointF(from.x() + length * c, from.y() + length * s);
  // 0 to 315, no -0 along the x axis.
  out.angle = std::fmod(snapped + 360.0, 360.0) + 0.0;
  return out;
}

} // namespace mitcad::drawings
