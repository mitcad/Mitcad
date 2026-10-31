// SPDX-License-Identifier: MIT
#include "DrawingTools.hpp"

#include <cmath>

#include <QCoreApplication>
#include <QJsonArray>
#include <QJsonDocument>
#include <QStringList>

namespace mitcad::drawings {
namespace {

QString tr(const char* text) { return QCoreApplication::translate("DrawingTools", text); }

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
// at least), and the placing click when the tool has one.
bool picked(const ToolPicks& picks, int count, bool place) {
  if (picks.view <= 0) {
    return false;
  }
  const qsizetype n = picks.picks.size();
  if (count == kOpenPicks ? n < 2 : n != count) {
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
  return type == QStringLiteral("radius") || type == QStringLiteral("diameter") ? 1 : 2;
}

QString dimensionType(const QHash<QString, QString>& options) {
  const QString type = toolOption("drawing.dimension", options, "type");
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
  command.insert(QStringLiteral("pen"), toolOption("drawing.line", options, "pen"));
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
  command.insert(QStringLiteral("pen"), toolOption("drawing.arc", options, "pen"));
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
                                {{QStringLiteral("levels"), toolOption("drawing.parts_list", options, "levels")}});
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
  tools.append({QStringLiteral("drawing.hole_table"), tr("Hole Table"), QStringLiteral("drawing-hole-table"),
                tr("Lists the holes a view sees end on, tagged in the view"), SheetPick::ViewThenPlace,
                tr("Click the view whose holes the table lists"),
                {choice(QStringLiteral("origin"), tr("Origin"),
                        {{QStringLiteral("corner"), tr("Lower Left Corner")},
                         {QStringLiteral("model"), tr("Component Origin")}},
                        QStringLiteral("corner")),
                 choice(QStringLiteral("tags"), tr("Tags"),
                        {{QStringLiteral("by_size"), tr("By Size")}, {QStringLiteral("numbers"), tr("Numbers")}},
                        QStringLiteral("by_size"))},
                &holeTable, true, tr("Click where the table goes"), nullptr, nullptr, true});
  tools.append({QStringLiteral("drawing.parts_list"), tr("Parts List"), QStringLiteral("drawing-parts-list"),
                tr("Lists the components a view shows, numbered"), SheetPick::ViewThenPlace,
                tr("Click the view whose components the list shows"),
                {choice(QStringLiteral("levels"), tr("Rows"),
                        {{QStringLiteral("top"), tr("First Level")}, {QStringLiteral("parts"), tr("Parts Only")}},
                        QStringLiteral("top")),
                 ToolOption{QStringLiteral("balloons"), tr("Balloons"), ToolOption::Kind::Check, {},
                            QStringLiteral("true")}},
                &partsList, true, tr("Click where the list goes"), nullptr, nullptr, true});
  tools.append({QStringLiteral("drawing.balloon"), tr("Balloon"), QStringLiteral("drawing-balloon"),
                tr("Shows a component's number in the parts list"), SheetPick::Picks,
                tr("Click an edge of the component on a view"), {}, &balloonOn, true,
                tr("Click where the balloon goes"), &onePick, &balloonType, true});
}

void addAnnotationTools(QVector<DrawingToolDef>& tools) {
  const auto choice = [](const QString& id, const QString& label, const QList<QPair<QString, QString>>& choices,
                         const QString& value) {
    return ToolOption{id, label, ToolOption::Kind::Choice, choices, value};
  };
  const auto text = [](const QString& id, const QString& label) {
    return ToolOption{id, label, ToolOption::Kind::Text, {}, QString()};
  };
  const QList<QPair<QString, QString>> axes = {{QStringLiteral("horizontal"), tr("Horizontal")},
                                               {QStringLiteral("vertical"), tr("Vertical")}};
  tools.append({QStringLiteral("drawing.dimension"), tr("Dimension"), QStringLiteral("dimension"),
                tr("Dimensions edges, circles and corners of a view"), SheetPick::Picks,
                tr("Click the edges, circles or corners to dimension on a view"),
                {choice(QStringLiteral("type"), tr("Type"),
                        {{QStringLiteral("linear"), tr("Linear")},
                         {QStringLiteral("aligned"), tr("Aligned")},
                         {QStringLiteral("angle"), tr("Angle")},
                         {QStringLiteral("radius"), tr("Radius")},
                         {QStringLiteral("diameter"), tr("Diameter")}},
                        QStringLiteral("linear"))},
                &dimension, true, tr("Click where the dimension goes"), &dimensionPicks, &dimensionType, true});
  tools.append({QStringLiteral("drawing.ordinate"), tr("Ordinate"), QStringLiteral("drawing-ordinate"),
                tr("Dimensions a point from an origin along the view"), SheetPick::Picks,
                tr("Click the origin, then the point to dimension"),
                {choice(QStringLiteral("along"), tr("Direction"), axes, QStringLiteral("horizontal"))}, &ordinate,
                true, tr("Click where the value goes"), &twoPicks, &ordinateType, true});
  tools.append({QStringLiteral("drawing.baseline"), tr("Baseline"), QStringLiteral("drawing-baseline"),
                tr("Dimensions several points from the first, or each from the last"), SheetPick::Picks,
                tr("Click the points to dimension, then a free place for the dimension lines"),
                {choice(QStringLiteral("series"), tr("Type"),
                        {{QStringLiteral("baseline"), tr("Baseline")}, {QStringLiteral("chain"), tr("Chain")}},
                        QStringLiteral("baseline")),
                 choice(QStringLiteral("along"), tr("Direction"), axes, QStringLiteral("horizontal"))},
                &baseline, false, QString(), &openPicks, &linearType, true});
  tools.append({QStringLiteral("drawing.center_mark"), tr("Center Mark"), QStringLiteral("drawing-center-mark"),
                tr("Marks a circle's centre, or a round face's centreline"), SheetPick::Picks,
                tr("Click a circle, or a round face seen from the side"), {}, &centerMark, false, QString(),
                &onePick, &centerMarkType, true});
  tools.append({QStringLiteral("drawing.centerline"), tr("Centerline"), QStringLiteral("centerline"),
                tr("Draws a centreline through two circles or between two edges"), SheetPick::Picks,
                tr("Click two circles, or two parallel edges"), {}, &centerline, false, QString(), &twoPicks,
                &centerlineType, true});
  // Drawn outside the model, on the sheet or in a view (mitcad#729).
  const auto attach = [&choice](const QString& value) {
    return choice(QStringLiteral("attach"), tr("Drawn"),
                  {{QStringLiteral("view"), tr("In the View Clicked")}, {QStringLiteral("sheet"), tr("On the Sheet")}},
                  value);
  };
  tools.append({QStringLiteral("drawing.line"), tr("Line"), QStringLiteral("line"),
                tr("Draws a line on the sheet or in a view, outside the model"), SheetPick::Segment,
                tr("Click the line's two ends"),
                {attach(QStringLiteral("view")),
                 choice(QStringLiteral("pen"), tr("Line"), penChoices(), QStringLiteral("thin"))},
                &sketchLine, false, QString(), nullptr, nullptr, true});
  tools.append({QStringLiteral("drawing.arc"), tr("Arc"), QStringLiteral("arc-center"),
                tr("Draws an arc on the sheet or in a view, outside the model"), SheetPick::ArcPoints,
                tr("Click the arc's centre, its start, then its end (counter-clockwise)"),
                {attach(QStringLiteral("view")),
                 choice(QStringLiteral("pen"), tr("Line"), penChoices(), QStringLiteral("thin"))},
                &sketchArc, false, QString(), nullptr, nullptr, true});
  tools.append({QStringLiteral("drawing.center_pattern"), tr("Center Pattern"), QStringLiteral("circular-pattern"),
                tr("Marks every hole of a pattern from one of them"), SheetPick::Picks,
                tr("Click one hole of the pattern, seen end on"), {}, &centerPattern, false, QString(), &onePick,
                &centerPatternType, true});
  tools.append({QStringLiteral("drawing.edge_extension"), tr("Edge Extension"), QStringLiteral("extend"),
                tr("Runs straight edges on to where they meet"), SheetPick::Picks,
                tr("Click the straight edges to run on"),
                {choice(QStringLiteral("edges"), tr("Edges"),
                        {{QStringLiteral("two"), tr("Two, to Their Corner")},
                         {QStringLiteral("one"), tr("One, Both Ways")}},
                        QStringLiteral("two"))},
                &edgeExtension, false, QString(), &extensionPicks, &edgeExtensionType, true});
  tools.append({QStringLiteral("drawing.text"), tr("Text"), QStringLiteral("drawing-text"),
                tr("Writes a note on the sheet or in a view"), SheetPick::Point, tr("Click where the text goes"),
                {text(QStringLiteral("text"), tr("Text")), attach(QStringLiteral("sheet"))}, &textNote, false,
                QString(), nullptr, nullptr, true});
  tools.append({QStringLiteral("drawing.note"), tr("Note"), QStringLiteral("drawing-note"),
                tr("Writes a note on a leader to an edge or a point"), SheetPick::Picks,
                tr("Click the edge or point the note's arrow points at"), {text(QStringLiteral("text"), tr("Text"))},
                &leaderNote, true, tr("Click where the note's text goes"), &onePick, &noteType, true});
  tools.append({QStringLiteral("drawing.datum"), tr("Datum Label"), QStringLiteral("drawing-datum"),
                tr("Labels a face or an axis as a datum"), SheetPick::Picks,
                tr("Click the edge of the face, or the round face, that is the datum"),
                {text(QStringLiteral("letter"), tr("Letter"))}, &datumLabel, true, tr("Click where its label goes"),
                &onePick, &datumType, true});
  tools.append({QStringLiteral("drawing.frame"), tr("Feature Control Frame"), QStringLiteral("drawing-frame"),
                tr("Adds a geometric tolerance to a face or an axis"), SheetPick::Picks,
                tr("Click the edge of the face, or the round face, to tolerance"), {}, &controlFrame, true,
                tr("Click where the frame goes"), &onePick, &frameType, true});
  tools.append({QStringLiteral("drawing.finish"), tr("Surface Finish"), QStringLiteral("drawing-finish"),
                tr("Puts a surface finish symbol on a face"), SheetPick::Picks,
                tr("Click the edge of the face the symbol stands on"),
                {choice(QStringLiteral("process"), tr("Surface"),
                        {{QStringLiteral("any"), tr("Any Process")},
                         {QStringLiteral("removal"), tr("Material Removed")},
                         {QStringLiteral("no_removal"), tr("No Material Removed")}},
                        QStringLiteral("removal")),
                 text(QStringLiteral("text"), tr("Requirement"))},
                &surfaceFinish, false, QString(), &onePick, &finishType, true});
}

QVector<DrawingToolDef> makeTools() {
  const QList<QPair<QString, QString>> directions = {
      {QStringLiteral("front"), tr("Front")},
      {QStringLiteral("top"), tr("Top")},
      {QStringLiteral("right"), tr("Right")},
      {QStringLiteral("back"), tr("Back")},
      {QStringLiteral("bottom"), tr("Bottom")},
      {QStringLiteral("left"), tr("Left")},
      {QStringLiteral("iso"), tr("Isometric")},
      {QStringLiteral("iso_top_left"), tr("Isometric, Top Left")},
      {QStringLiteral("iso_bottom_right"), tr("Isometric, Bottom Right")},
      {QStringLiteral("iso_bottom_left"), tr("Isometric, Bottom Left")},
      // The 3D view's direction and up as they are (mitcad#725).
      {QStringLiteral("current"), tr("Current View")},
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
  detailScales.first().second = tr("Twice Its Parent's");
  QVector<DrawingToolDef> tools;
  tools.append({QStringLiteral("drawing.base_view"), tr("Base View"), QStringLiteral("drawing-base-view"),
                tr("Places a view of the design on the sheet"), SheetPick::Point,
                tr("Click where the view goes"),
                // The whole component; the workspace lists its occurrences
                // and bodies by name (subjectChoices).
                {choice(QStringLiteral("subject"), tr("Shows"), {{QStringLiteral("design"), tr("The Design")}},
                        QStringLiteral("design")),
                 choice(QStringLiteral("direction"), tr("Orientation"), directions, QStringLiteral("front")),
                 choice(QStringLiteral("scale"), tr("Scale"), scaleChoices(true), QStringLiteral("fit")),
                 check(QStringLiteral("hidden_lines"), tr("Hidden Edges"), false),
                 check(QStringLiteral("tangent_edges"), tr("Tangent Edges"), false),
                 check(QStringLiteral("shaded"), tr("Shaded"), false)},
                &baseView, false, QString(), nullptr, nullptr, false});
  tools.append({QStringLiteral("drawing.projected_view"), tr("Projected View"),
                QStringLiteral("drawing-projected-view"), tr("Projects a view from one beside it"),
                SheetPick::OutOfView, tr("Drag out of a view, or click where the next view goes"), {},
                &projectedView, false, QString(), nullptr, nullptr, false});
  tools.append({QStringLiteral("drawing.section_view"), tr("Section View"), QStringLiteral("drawing-section-view"),
                tr("Cuts a view along a line drawn on it"), SheetPick::Line,
                tr("Click the two ends of the section line on a view"),
                {text(QStringLiteral("letter"), tr("Letter")), check(QStringLiteral("hatch"), tr("Hatching"), true),
                 check(QStringLiteral("flip"), tr("Look the Other Way"), false)},
                &sectionView, true, tr("Click where the section view goes"), nullptr, nullptr, false});
  tools.append({QStringLiteral("drawing.detail_view"), tr("Detail View"), QStringLiteral("drawing-detail-view"),
                tr("Enlarges a circle of a view"), SheetPick::Circle,
                tr("Click the middle of the detail on a view, then its edge"),
                {text(QStringLiteral("letter"), tr("Letter")),
                 choice(QStringLiteral("scale"), tr("Scale"), detailScales, QStringLiteral("auto"))},
                &detailView, true, tr("Click where the detail view goes"), nullptr, nullptr, false});
  tools.append({QStringLiteral("drawing.break"), tr("Break"), QStringLiteral("drawing-break"),
                tr("Shortens a view by taking a band out"), SheetPick::Line,
                tr("Click the two sides of the band on a view"),
                {choice(QStringLiteral("direction"), tr("Direction"),
                        {{QStringLiteral("horizontal"), tr("Horizontal")}, {QStringLiteral("vertical"), tr("Vertical")}},
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
      id == QStringLiteral("drawing.baseline")) {
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
    return tr("Create");
  case 1:
    return tr("Geometry");
  case 2:
    return tr("Dimensions");
  case 3:
    return tr("Text");
  case 4:
    return tr("Symbols");
  case 5:
    return tr("Tables");
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
  out.append({QStringLiteral("design"), name.isEmpty() ? tr("The Design") : name});
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
    out.append({QStringLiteral("fit"), tr("Fit the Sheet")});
  } else {
    out.append({QStringLiteral("auto"), tr("Its Parent's")});
  }
  for (const char* scale : {"10:1", "5:1", "2:1", "1:1", "1:2", "1:5", "1:10", "1:20", "1:50", "1:100"}) {
    out.append({QString::fromLatin1(scale), QString::fromLatin1(scale)});
  }
  return out;
}

QList<QPair<QString, QString>> sideChoices() {
  return {{QStringLiteral("right"), tr("Right")},
          {QStringLiteral("left"), tr("Left")},
          {QStringLiteral("above"), tr("Above")},
          {QStringLiteral("below"), tr("Below")},
          {QStringLiteral("above_right"), tr("Above Right")},
          {QStringLiteral("above_left"), tr("Above Left")},
          {QStringLiteral("below_right"), tr("Below Right")},
          {QStringLiteral("below_left"), tr("Below Left")}};
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
             field == QStringLiteral("retrieved_dimensions")) {
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
  } else if (field == QStringLiteral("pen")) {
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
  } else if (field == QStringLiteral("descending")) {
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
    return tr("Center Mark");
  }
  if (type == QStringLiteral("centerline")) {
    return tr("Centerline");
  }
  if (type == QStringLiteral("note")) {
    return tr("Note");
  }
  if (type == QStringLiteral("text")) {
    return tr("Text");
  }
  if (type == QStringLiteral("datum")) {
    return tr("Datum Label");
  }
  if (type == QStringLiteral("frame")) {
    return tr("Feature Control Frame");
  }
  if (type == QStringLiteral("finish")) {
    return tr("Surface Finish");
  }
  if (type == QStringLiteral("hole_table")) {
    return tr("Hole Table");
  }
  if (type == QStringLiteral("parts_list")) {
    return tr("Parts List");
  }
  if (type == QStringLiteral("balloon")) {
    return tr("Balloon");
  }
  if (type == QStringLiteral("line")) {
    return tr("Line");
  }
  if (type == QStringLiteral("arc")) {
    return tr("Arc");
  }
  if (type == QStringLiteral("center_pattern")) {
    return tr("Center Pattern");
  }
  if (type == QStringLiteral("edge_extension")) {
    return tr("Edge Extension");
  }
  return tr("Dimension");
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

QList<QPair<QString, QString>> penChoices() {
  return {{QStringLiteral("thin"), tr("Thin")},
          {QStringLiteral("thick"), tr("Thick")},
          {QStringLiteral("hidden"), tr("Dashed")},
          {QStringLiteral("chain"), tr("Center Line")}};
}

} // namespace mitcad::drawings
