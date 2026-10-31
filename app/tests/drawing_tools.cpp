// SPDX-License-Identifier: MIT
// The drawing tools (mitcad#707): their definitions and the model commands
// their picks and options make (core/model-api/src/commands.md, "Drawings
// in the design"), and the view properties' edits.
#include "drawings/DrawingTemplates.hpp"
#include "drawings/DrawingTools.hpp"

#include <algorithm>
#include <cstdio>
#include <utility>

#include <QJsonArray>
#include <QJsonObject>
#include <QStringList>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "drawing_tools.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

mitcad::drawings::ToolPicks picks(int view, QVector<QPointF> points, double radius = 0.0,
                                  QJsonObject subject = QJsonObject()) {
  mitcad::drawings::ToolPicks out;
  out.view = view;
  out.points = std::move(points);
  out.radius = radius;
  out.subject = std::move(subject);
  return out;
}

} // namespace

int drawingToolsTests() {
  using namespace mitcad::drawings;
  // The tools in the bar's order, each with its icon and what it waits for.
  QStringList names;
  for (const DrawingToolDef& tool : drawingTools()) {
    names << tool.name;
    CHECK(!tool.icon.isEmpty() && !tool.tooltip.isEmpty() && !tool.hint.isEmpty());
    CHECK(tool.command != nullptr);
    CHECK(tool.tooltip.size() <= 80);
    CHECK(tool.place == !tool.placeHint.isEmpty());
  }
  CHECK(names == QStringList({QStringLiteral("Base View"), QStringLiteral("Projected View"),
                              QStringLiteral("Section View"), QStringLiteral("Detail View"),
                              QStringLiteral("Break"), QStringLiteral("Dimension"), QStringLiteral("Ordinate"),
                              QStringLiteral("Baseline"), QStringLiteral("Center Mark"),
                              QStringLiteral("Centerline"), QStringLiteral("Line"), QStringLiteral("Arc"),
                              QStringLiteral("Center Pattern"), QStringLiteral("Edge Extension"),
                              QStringLiteral("Text"), QStringLiteral("Note"),
                              QStringLiteral("Datum Label"), QStringLiteral("Feature Control Frame"),
                              QStringLiteral("Surface Finish"), QStringLiteral("Hole Table"),
                              QStringLiteral("Parts List"), QStringLiteral("Balloon")}));
  // The annotations' and the tables' tools pick on views (the model says
  // what) or take a view, and show their icons only; the views' tools
  // their names too.
  QList<int> groups;
  for (const DrawingToolDef& tool : drawingTools()) {
    const bool picking = tool.pick == SheetPick::Picks;
    CHECK(picking == (tool.pickCount != nullptr));
    CHECK(picking == (tool.annotationType != nullptr));
    CHECK(tool.iconOnly == (picking || tool.id == QStringLiteral("drawing.text") ||
                            tool.pick == SheetPick::ViewThenPlace || tool.pick == SheetPick::Segment ||
                            tool.pick == SheetPick::ArcPoints));
    groups << toolGroup(tool);
  }
  // The bar's groups (mitcad#725): Create, Geometry, Dimensions, Text,
  // Symbols, Tables, each with its caption; Geometry with Line, Arc, Center
  // Pattern and Edge Extension (mitcad#729).
  CHECK(groups.count(0) == 5 && groups.count(1) == 6 && groups.count(2) == 3 && groups.count(3) == 2 &&
        groups.count(4) == 3 && groups.count(5) == 3);
  CHECK(toolGroupName(0) == QStringLiteral("Create") && toolGroupName(1) == QStringLiteral("Geometry") &&
        toolGroupName(2) == QStringLiteral("Dimensions") && toolGroupName(3) == QStringLiteral("Text") &&
        toolGroupName(4) == QStringLiteral("Symbols") && toolGroupName(5) == QStringLiteral("Tables"));
  CHECK(toolGroupName(kToolGroups).isEmpty());
  // The ribbon's DRAWING tab's groups (mitcad#1008): each group's name in
  // capitals.
  for (int g = 0; g < kToolGroups; ++g) {
    CHECK(toolGroupId(g) == toolGroupName(g).toUpper());
  }
  CHECK(toolGroupId(kToolGroups).isEmpty());
  // The command search's other words (mitcad#725).
  CHECK(toolKeywords(QStringLiteral("drawing.note")).contains(QStringLiteral("leader")));
  CHECK(toolKeywords(QStringLiteral("drawing.baseline")).contains(QStringLiteral("chain")));
  CHECK(toolKeywords(QStringLiteral("drawing.datum")).contains(QStringLiteral("datum identifier")));
  CHECK(toolKeywords(QStringLiteral("drawing.finish")).contains(QStringLiteral("surface texture")));
  CHECK(toolKeywords(QStringLiteral("drawing.break")).contains(QStringLiteral("break view")));
  CHECK(toolKeywords(QStringLiteral("drawing.parts_list")).contains(QStringLiteral("bom")));
  CHECK(toolKeywords(QStringLiteral("drawing.base_view")) ==
        QStringList({QStringLiteral("drawing"), QStringLiteral("sheet")}));
  const DrawingToolDef* base = drawingTool(QStringLiteral("drawing.base_view"));
  const DrawingToolDef* projected = drawingTool(QStringLiteral("drawing.projected_view"));
  const DrawingToolDef* section = drawingTool(QStringLiteral("drawing.section_view"));
  const DrawingToolDef* detail = drawingTool(QStringLiteral("drawing.detail_view"));
  const DrawingToolDef* cut = drawingTool(QStringLiteral("drawing.break"));
  CHECK(base && projected && section && detail && cut);
  CHECK(drawingTool(QStringLiteral("drawing.nothing")) == nullptr);
  if (!(base && projected && section && detail && cut)) {
    return failures;
  }
  CHECK(base->pick == SheetPick::Point && projected->pick == SheetPick::OutOfView);
  CHECK(section->pick == SheetPick::Line && detail->pick == SheetPick::Circle && cut->pick == SheetPick::Line);
  // Sections and details are placed by a click after their line or circle.
  CHECK(section->place && detail->place && !cut->place && !base->place);

  // A base view: the front, its scale left to the model (Fit the Sheet).
  for (const ToolOption& option : base->options) {
    if (option.id == QStringLiteral("scale")) {
      CHECK(option.defaultValue == QStringLiteral("fit") && option.choices.first().second == QStringLiteral("Fit the Sheet"));
    }
  }
  const QHash<QString, QString> none;
  QJsonObject command = base->command(QStringLiteral("D1"), 0, picks(0, {QPointF(150, 120)}), none);
  CHECK(command.value(QStringLiteral("cmd")).toString() == QStringLiteral("add_drawing_view"));
  CHECK(command.value(QStringLiteral("type")).toString() == QStringLiteral("base"));
  CHECK(command.value(QStringLiteral("direction")).toString() == QStringLiteral("front"));
  CHECK(command.value(QStringLiteral("at")).toArray() == QJsonArray({150.0, 120.0}));
  CHECK(!command.contains(QStringLiteral("scale")));
  CHECK(!command.contains(QStringLiteral("subject")));
  CHECK(command.value(QStringLiteral("style")).toObject().value(QStringLiteral("hidden_lines")).toBool() == false);
  QHash<QString, QString> options{{QStringLiteral("direction"), QStringLiteral("iso")},
                                  {QStringLiteral("scale"), QStringLiteral("1:2")},
                                  {QStringLiteral("shaded"), QStringLiteral("true")}};
  command = base->command(QStringLiteral("D1"), 1, picks(0, {QPointF(10, 20)}), options);
  CHECK(command.value(QStringLiteral("direction")).toString() == QStringLiteral("iso"));
  CHECK(command.value(QStringLiteral("scale")).toString() == QStringLiteral("1:2"));
  CHECK(command.value(QStringLiteral("sheet")).toInt() == 1);
  CHECK(command.value(QStringLiteral("style")).toObject().value(QStringLiteral("shaded")).toBool());
  // Nothing without a point.
  CHECK(base->command(QStringLiteral("D1"), 0, ToolPicks(), none).isEmpty());
  // What it shows, by its Shows choice (mitcad#725): a body of the
  // drawing's component, an occurrence in it.
  const QJsonObject bodies{{QStringLiteral("type"), QStringLiteral("bodies")},
                           {QStringLiteral("bodies"), QJsonArray{QJsonObject{{QStringLiteral("body"), QStringLiteral("F2.b0")}}}}};
  command = base->command(QStringLiteral("D1"), 0, picks(0, {QPointF(10, 20)}),
                          {{QStringLiteral("subject"), QStringLiteral("body:F2.b0")}});
  CHECK(command.value(QStringLiteral("subject")).toObject() == bodies);
  command = base->command(QStringLiteral("D1"), 0, picks(0, {QPointF(10, 20)}),
                          {{QStringLiteral("subject"), QStringLiteral("occurrence:O1/O4")}});
  CHECK(command.value(QStringLiteral("subject")).toObject() ==
        QJsonObject({{QStringLiteral("type"), QStringLiteral("occurrence")},
                     {QStringLiteral("occurrence"), QStringLiteral("O1/O4")}}));
  CHECK(subjectChoice(bodies) == QStringLiteral("body:F2.b0"));
  CHECK(subjectChoice(QJsonObject{{QStringLiteral("type"), QStringLiteral("design")}}) == QStringLiteral("design"));
  CHECK(subjectChoice(QJsonObject{{QStringLiteral("type"), QStringLiteral("occurrence")},
                                  {QStringLiteral("occurrence"), QStringLiteral("O2")}}) ==
        QStringLiteral("occurrence:O2"));
  CHECK(subjectOf(QStringLiteral("design")).isEmpty());
  // Several bodies are no single choice.
  CHECK(subjectChoice(QJsonObject{{QStringLiteral("type"), QStringLiteral("bodies")},
                                  {QStringLiteral("bodies"), QJsonArray{QJsonObject{{QStringLiteral("body"), QStringLiteral("a")}},
                                                                        QJsonObject{{QStringLiteral("body"), QStringLiteral("b")}}}}})
            .isEmpty());
  // The Shows list: the component by name, its occurrences by their paths
  // of names, its own bodies by name.
  const QJsonObject components{
      {QStringLiteral("occurrences"),
       QJsonArray{QJsonObject{{QStringLiteral("uid"), QStringLiteral("O1")},
                              {QStringLiteral("name"), QStringLiteral("Arm:1")},
                              {QStringLiteral("component"), QStringLiteral("C1")},
                              {QStringLiteral("children"),
                               QJsonArray{QJsonObject{{QStringLiteral("uid"), QStringLiteral("O4")},
                                                      {QStringLiteral("name"), QStringLiteral("Pin:2")},
                                                      {QStringLiteral("component"), QStringLiteral("C2")}}}}}}}};
  const QJsonArray allBodies{QJsonObject{{QStringLiteral("uid"), QStringLiteral("F2.b0")}, {QStringLiteral("name"), QStringLiteral("Base")}},
                             QJsonObject{{QStringLiteral("uid"), QStringLiteral("F5.b0")},
                                         {QStringLiteral("name"), QStringLiteral("Arm body")},
                                         {QStringLiteral("component"), QStringLiteral("C1")}}};
  const QList<QPair<QString, QString>> rootList = subjectChoices(QStringLiteral("C0"), QStringLiteral("robot"), components, allBodies);
  CHECK(rootList.size() == 4);
  CHECK(rootList.value(0) == qMakePair(QStringLiteral("design"), QStringLiteral("robot")));
  CHECK(rootList.value(1) == qMakePair(QStringLiteral("occurrence:O1"), QStringLiteral("Arm:1")));
  CHECK(rootList.value(2) == qMakePair(QStringLiteral("occurrence:O1/O4"), QStringLiteral("Arm:1/Pin:2")));
  CHECK(rootList.value(3) == qMakePair(QStringLiteral("body:F2.b0"), QStringLiteral("Base")));
  // A component's drawing: what is inside it, its own bodies.
  const QList<QPair<QString, QString>> armList = subjectChoices(QStringLiteral("C1"), QStringLiteral("Arm"), components, allBodies);
  CHECK(armList.size() == 3);
  CHECK(armList.value(1) == qMakePair(QStringLiteral("occurrence:O4"), QStringLiteral("Pin:2")));
  CHECK(armList.value(2) == qMakePair(QStringLiteral("body:F5.b0"), QStringLiteral("Arm body")));
  // Current View: the 3D view's direction the workspace gives; nothing
  // without it. The isometric views from below too.
  const QHash<QString, QString> current{{QStringLiteral("direction"), QStringLiteral("current")}};
  CHECK(base->command(QStringLiteral("D1"), 0, picks(0, {QPointF(10, 20)}), current).isEmpty());
  ToolPicks camera = picks(0, {QPointF(10, 20)});
  camera.camera = QJsonObject{{QStringLiteral("toward_eye"), QJsonArray{0.0, -1.0, 0.0}},
                              {QStringLiteral("up"), QJsonArray{0.0, 0.0, 1.0}}};
  command = base->command(QStringLiteral("D1"), 0, camera, current);
  CHECK(command.value(QStringLiteral("direction")).toObject() == camera.camera);
  QStringList directions;
  for (const ToolOption& option : base->options) {
    if (option.id == QStringLiteral("direction")) {
      for (const auto& choice : option.choices) {
        directions << choice.first;
      }
    }
  }
  CHECK(directions.contains(QStringLiteral("iso_bottom_right")) && directions.contains(QStringLiteral("iso_bottom_left")));
  CHECK(directions.last() == QStringLiteral("current"));

  // A projected view: its parent and where it was dropped.
  command = projected->command(QStringLiteral("D1"), 0, picks(3, {QPointF(250, 120)}), none);
  CHECK(command.value(QStringLiteral("type")).toString() == QStringLiteral("projected"));
  CHECK(command.value(QStringLiteral("parent")).toInt() == 3);
  CHECK(projected->command(QStringLiteral("D1"), 0, picks(0, {QPointF(1, 1)}), none).isEmpty());

  // A section: two points on a view, hatched, looking the default way;
  // the third click places it.
  command = section->command(QStringLiteral("D1"), 0, picks(1, {QPointF(85, 105), QPointF(85, 135)}), none);
  CHECK(command.value(QStringLiteral("type")).toString() == QStringLiteral("section"));
  CHECK(command.value(QStringLiteral("line")).toArray().size() == 2);
  CHECK(command.value(QStringLiteral("hatch")).toBool() && !command.value(QStringLiteral("flip")).toBool());
  CHECK(!command.contains(QStringLiteral("letter")) && !command.contains(QStringLiteral("at")));
  CHECK(section->command(QStringLiteral("D1"), 0, picks(1, {QPointF(85, 105)}), none).isEmpty());
  command = section->command(QStringLiteral("D1"), 0, picks(1, {QPointF(0, 0), QPointF(0, 9), QPointF(200, 50)}),
                             {{QStringLiteral("letter"), QStringLiteral(" K ")}, {QStringLiteral("flip"), QStringLiteral("true")}});
  CHECK(command.value(QStringLiteral("letter")).toString() == QStringLiteral("K"));
  CHECK(command.value(QStringLiteral("flip")).toBool());
  CHECK(command.value(QStringLiteral("at")).toArray() == QJsonArray({200.0, 50.0}));

  // A detail: a circle on a view, then where it goes.
  command = detail->command(QStringLiteral("D1"), 0, picks(1, {QPointF(70, 115)}, 8.0), none);
  CHECK(command.value(QStringLiteral("type")).toString() == QStringLiteral("detail"));
  CHECK(command.value(QStringLiteral("radius")).toDouble() == 8.0);
  CHECK(!command.contains(QStringLiteral("at")));
  CHECK(detail->command(QStringLiteral("D1"), 0, picks(1, {QPointF(70, 115)}), none).isEmpty());
  command = detail->command(QStringLiteral("D1"), 0, picks(1, {QPointF(70, 115), QPointF(250, 60)}, 8.0), none);
  CHECK(command.value(QStringLiteral("at")).toArray() == QJsonArray({250.0, 60.0}));

  // A break: an edit of the view it was drawn on.
  command = cut->command(QStringLiteral("D1"), 0, picks(2, {QPointF(85, 120), QPointF(115, 120)}), none);
  CHECK(command.value(QStringLiteral("cmd")).toString() == QStringLiteral("edit_drawing_view"));
  CHECK(command.value(QStringLiteral("view")).toInt() == 2);
  CHECK(command.value(QStringLiteral("add_break")).toObject().value(QStringLiteral("direction")).toString() ==
        QStringLiteral("horizontal"));

  // The view properties: a style is changed as a whole, a scale back to
  // the parent's.
  const QJsonObject style{{QStringLiteral("tangent_edges"), true}};
  command = viewEdit(QStringLiteral("D1"), 4, QStringLiteral("hidden_lines"), QStringLiteral("true"), style);
  const QJsonObject changed = command.value(QStringLiteral("style")).toObject();
  CHECK(changed.value(QStringLiteral("hidden_lines")).toBool() && changed.value(QStringLiteral("tangent_edges")).toBool());
  CHECK(!changed.value(QStringLiteral("shaded")).toBool());
  command = viewEdit(QStringLiteral("D1"), 4, QStringLiteral("scale"), QStringLiteral("auto"), style);
  CHECK(command.value(QStringLiteral("scale")).toString() == QStringLiteral("parent"));
  command = viewEdit(QStringLiteral("D1"), 4, QStringLiteral("scale"), QStringLiteral("fit"), style);
  CHECK(command.value(QStringLiteral("scale")).toString() == QStringLiteral("fit"));
  command = viewEdit(QStringLiteral("D1"), 4, QStringLiteral("label"), QStringLiteral("VIEW X"), style);
  CHECK(command.value(QStringLiteral("label")).toString() == QStringLiteral("VIEW X"));
  // A section's hatching and direction, a detail's circle, a projected
  // view's side, a base view's subject.
  command = viewEdit(QStringLiteral("D1"), 3, QStringLiteral("hatch"), QStringLiteral("false"), style);
  CHECK(command.value(QStringLiteral("hatch")) == QJsonValue(false));
  command = viewEdit(QStringLiteral("D1"), 3, QStringLiteral("flip"), QStringLiteral("true"), style);
  CHECK(command.value(QStringLiteral("flip")) == QJsonValue(true));
  command = viewEdit(QStringLiteral("D1"), 4, QStringLiteral("radius"), QStringLiteral("12.5"), style);
  CHECK(command.value(QStringLiteral("radius")).toDouble() == 12.5);
  CHECK(viewEdit(QStringLiteral("D1"), 4, QStringLiteral("radius"), QStringLiteral("big"), style).isEmpty());
  command = viewEdit(QStringLiteral("D1"), 2, QStringLiteral("side"), QStringLiteral("above"), style);
  CHECK(command.value(QStringLiteral("side")).toString() == QStringLiteral("above"));
  command = viewEdit(QStringLiteral("D1"), 1, QStringLiteral("subject"), QStringLiteral(R"({"type": "design"})"), style);
  CHECK(command.value(QStringLiteral("subject")).toObject().value(QStringLiteral("type")).toString() ==
        QStringLiteral("design"));
  // A detail's circle moved from the panel: its centre on the sheet.
  command = viewEdit(QStringLiteral("D1"), 4, QStringLiteral("center"), QStringLiteral("12.5 -3"), style);
  CHECK(command.value(QStringLiteral("center")).toArray() == QJsonArray({12.5, -3.0}));
  CHECK(viewEdit(QStringLiteral("D1"), 4, QStringLiteral("center"), QStringLiteral("12.5"), style).isEmpty());
  CHECK(scaleChoices().first().first == QStringLiteral("auto"));
  CHECK(scaleChoices().first().second == QStringLiteral("Its Parent's"));
  CHECK(scaleChoices(true).first().first == QStringLiteral("fit"));
  // The Detail tool's automatic scale is named as the view panel names
  // it (mitcad#715).
  for (const ToolOption& option : detail->options) {
    if (option.id == QStringLiteral("scale")) {
      CHECK(option.defaultValue == QStringLiteral("auto"));
      CHECK(option.choices.first().second == QStringLiteral("Twice Its Parent's"));
    }
  }
  CHECK(sideChoices().size() == 8);
  command = viewEdit(QStringLiteral("D1"), 2, QStringLiteral("center_marks"), QStringLiteral("false"), style);
  CHECK(command.value(QStringLiteral("center_marks")) == QJsonValue(false));

  // The annotations' tools (mitcad#707 phase 2): the model's picks the
  // workspace found, then the placing click.
  const DrawingToolDef* dimension = drawingTool(QStringLiteral("drawing.dimension"));
  const DrawingToolDef* baseline = drawingTool(QStringLiteral("drawing.baseline"));
  const DrawingToolDef* mark = drawingTool(QStringLiteral("drawing.center_mark"));
  const DrawingToolDef* text = drawingTool(QStringLiteral("drawing.text"));
  const DrawingToolDef* note = drawingTool(QStringLiteral("drawing.note"));
  const DrawingToolDef* finish = drawingTool(QStringLiteral("drawing.finish"));
  const DrawingToolDef* frame = drawingTool(QStringLiteral("drawing.frame"));
  CHECK(dimension && baseline && mark && text && note && finish && frame);
  if (!(dimension && baseline && mark && text && note && finish && frame)) {
    return failures;
  }
  const QJsonObject edge{{QStringLiteral("body"), QStringLiteral("F2.b0")},
                         {QStringLiteral("edge"), QStringLiteral("E{F2:a|F2:b}")}};
  const auto annotationPicks = [&edge](int count, QVector<QPointF> points) {
    ToolPicks out = picks(1, std::move(points));
    for (int i = 0; i < count; ++i) {
      out.picks.append(edge);
    }
    return out;
  };
  CHECK(dimension->pickCount(none) == 2 && dimension->annotationType(none) == QStringLiteral("linear"));
  const QHash<QString, QString> diameter{{QStringLiteral("type"), QStringLiteral("diameter")}};
  CHECK(dimension->pickCount(diameter) == 1 && dimension->annotationType(diameter) == QStringLiteral("diameter"));
  const QHash<QString, QString> aligned{{QStringLiteral("type"), QStringLiteral("aligned")}};
  CHECK(dimension->annotationType(aligned) == QStringLiteral("linear"));
  // Not without its picks and its place.
  CHECK(dimension->command(QStringLiteral("D1"), 0, annotationPicks(1, {QPointF(1, 2)}), none).isEmpty());
  CHECK(dimension->command(QStringLiteral("D1"), 0, annotationPicks(2, {}), none).isEmpty());
  command = dimension->command(QStringLiteral("D1"), 0, annotationPicks(2, {QPointF(150, 90)}), none);
  CHECK(command.value(QStringLiteral("cmd")).toString() == QStringLiteral("add_drawing_annotation"));
  CHECK(command.value(QStringLiteral("type")).toString() == QStringLiteral("linear"));
  CHECK(command.value(QStringLiteral("view")).toInt() == 1);
  CHECK(command.value(QStringLiteral("picks")).toArray().size() == 2);
  CHECK(command.value(QStringLiteral("at")).toArray() == QJsonArray({150.0, 90.0}));
  CHECK(!command.contains(QStringLiteral("along")));
  command = dimension->command(QStringLiteral("D1"), 0, annotationPicks(2, {QPointF(1, 2)}), aligned);
  CHECK(command.value(QStringLiteral("along")).toString() == QStringLiteral("aligned"));
  // A baseline: two picks at least, placed by the free click.
  CHECK(baseline->pickCount(none) == kOpenPicks && !baseline->place);
  CHECK(baseline->command(QStringLiteral("D1"), 0, annotationPicks(1, {QPointF(1, 2)}), none).isEmpty());
  command = baseline->command(QStringLiteral("D1"), 0, annotationPicks(3, {QPointF(1, 2)}), none);
  CHECK(command.value(QStringLiteral("series")).toString() == QStringLiteral("baseline"));
  CHECK(command.value(QStringLiteral("along")).toString() == QStringLiteral("horizontal"));
  // A centre mark at once, on its pick.
  command = mark->command(QStringLiteral("D1"), 0, annotationPicks(1, {}), none);
  CHECK(command.value(QStringLiteral("type")).toString() == QStringLiteral("center_mark"));
  // Text: on the sheet, with its text (a dialog asks when the option has
  // none); a note on its leader.
  CHECK(text->command(QStringLiteral("D1"), 2, picks(0, {QPointF(30, 40)}), none).isEmpty());
  command = text->command(QStringLiteral("D1"), 2, picks(0, {QPointF(30, 40)}),
                          {{QStringLiteral("text"), QStringLiteral("GENERAL NOTES")}});
  CHECK(command.value(QStringLiteral("type")).toString() == QStringLiteral("text"));
  CHECK(command.value(QStringLiteral("sheet")).toInt() == 2);
  CHECK(command.value(QStringLiteral("text")).toString() == QStringLiteral("GENERAL NOTES"));
  command = note->command(QStringLiteral("D1"), 0, annotationPicks(1, {QPointF(5, 6)}),
                          {{QStringLiteral("text"), QStringLiteral("DEBURR")}});
  CHECK(command.value(QStringLiteral("type")).toString() == QStringLiteral("note"));
  CHECK(command.value(QStringLiteral("text")).toString() == QStringLiteral("DEBURR"));
  // A frame takes its tolerance from its dialog, as the model's JSON.
  CHECK(frame->command(QStringLiteral("D1"), 0, annotationPicks(1, {QPointF(5, 6)}), none).isEmpty());
  command = frame->command(QStringLiteral("D1"), 0, annotationPicks(1, {QPointF(5, 6)}),
                           {{QStringLiteral("frame"), QStringLiteral(R"({"characteristic": "flatness", "value": 0.1})")}});
  CHECK(command.value(QStringLiteral("frame")).toObject().value(QStringLiteral("value")).toDouble() == 0.1);
  // A surface finish symbol: its process and requirement.
  command = finish->command(QStringLiteral("D1"), 0, annotationPicks(1, {}),
                            {{QStringLiteral("text"), QStringLiteral(" Ra 1.6 ")}});
  const QJsonObject requirement = command.value(QStringLiteral("finish")).toObject();
  CHECK(requirement.value(QStringLiteral("process")).toString() == QStringLiteral("removal"));
  CHECK(requirement.value(QStringLiteral("text")).toString() == QStringLiteral("Ra 1.6"));

  // An annotation's properties as its panel edits them.
  command = annotationEdit(QStringLiteral("D1"), 3, QStringLiteral("along"), QStringLiteral("vertical"));
  CHECK(command.value(QStringLiteral("cmd")).toString() == QStringLiteral("edit_drawing_annotation"));
  CHECK(command.value(QStringLiteral("annotation")).toInt() == 3);
  CHECK(command.value(QStringLiteral("along")).toString() == QStringLiteral("vertical"));
  command = annotationEdit(QStringLiteral("D1"), 3, QStringLiteral("before"), QStringLiteral(" 4 x "));
  CHECK(command.value(QStringLiteral("format")).toObject().value(QStringLiteral("before")).toString() ==
        QStringLiteral("4 x"));
  command = annotationEdit(QStringLiteral("D1"), 3, QStringLiteral("prefix"), QStringLiteral("auto"));
  CHECK(command.value(QStringLiteral("format")).toObject().value(QStringLiteral("prefix")).isNull());
  CHECK(annotationEdit(QStringLiteral("D1"), 3, QStringLiteral("letter"), QStringLiteral(" ")).isEmpty());
  command = annotationEdit(QStringLiteral("D1"), 3, QStringLiteral("finish_text"), QStringLiteral(""),
                           QJsonObject{{QStringLiteral("process"), QStringLiteral("any")},
                                       {QStringLiteral("text"), QStringLiteral("Ra 3.2")}});
  CHECK(command.value(QStringLiteral("finish")).toObject() ==
        QJsonObject({{QStringLiteral("process"), QStringLiteral("any")}}));
  CHECK(annotationEdit(QStringLiteral("D1"), 3, QStringLiteral("height"), QStringLiteral("0")).isEmpty());
  CHECK(annotationName(QStringLiteral("linear")) == QStringLiteral("Dimension"));
  CHECK(annotationName(QStringLiteral("center_mark")) == QStringLiteral("Center Mark"));
  CHECK(annotationName(QStringLiteral("datum")) == QStringLiteral("Datum Label"));

  // The tables' tools (mitcad#707 phase 3): a click on a view, then where
  // the table goes; a balloon on its pick, then its place.
  const DrawingToolDef* holes = drawingTool(QStringLiteral("drawing.hole_table"));
  const DrawingToolDef* parts = drawingTool(QStringLiteral("drawing.parts_list"));
  const DrawingToolDef* balloon = drawingTool(QStringLiteral("drawing.balloon"));
  CHECK(holes && parts && balloon);
  if (!(holes && parts && balloon)) {
    return failures;
  }
  CHECK(holes->pick == SheetPick::ViewThenPlace && parts->pick == SheetPick::ViewThenPlace);
  CHECK(holes->place && parts->place && balloon->place);
  CHECK(holes->command(QStringLiteral("D1"), 0, picks(2, {QPointF(80, 160)}), none).isEmpty());
  command = holes->command(QStringLiteral("D1"), 0, picks(2, {QPointF(80, 160), QPointF(250, 270)}), none);
  CHECK(command.value(QStringLiteral("cmd")).toString() == QStringLiteral("add_drawing_annotation"));
  CHECK(command.value(QStringLiteral("type")).toString() == QStringLiteral("hole_table"));
  CHECK(command.value(QStringLiteral("view")).toInt() == 2);
  CHECK(command.value(QStringLiteral("at")).toArray() == QJsonArray({250.0, 270.0}));
  const QJsonObject holeSettings = command.value(QStringLiteral("table")).toObject();
  CHECK(holeSettings.value(QStringLiteral("origin")).toString() == QStringLiteral("corner"));
  CHECK(holeSettings.value(QStringLiteral("tags")).toString() == QStringLiteral("by_size"));
  command = parts->command(QStringLiteral("D1"), 0, picks(1, {QPointF(80, 160), QPointF(300, 280)}),
                           {{QStringLiteral("levels"), QStringLiteral("parts")}});
  CHECK(command.value(QStringLiteral("type")).toString() == QStringLiteral("parts_list"));
  CHECK(command.value(QStringLiteral("table")).toObject().value(QStringLiteral("levels")).toString() ==
        QStringLiteral("parts"));
  // Its balloons with it unless the option is off (mitcad#725).
  CHECK(command.value(QStringLiteral("balloons")) == QJsonValue(true));
  command = parts->command(QStringLiteral("D1"), 0, picks(1, {QPointF(80, 160), QPointF(300, 280)}),
                           {{QStringLiteral("balloons"), QStringLiteral("false")}});
  CHECK(!command.contains(QStringLiteral("balloons")));
  CHECK(balloon->pickCount(none) == 1 && balloon->annotationType(none) == QStringLiteral("balloon"));
  CHECK(balloon->command(QStringLiteral("D1"), 0, annotationPicks(1, {}), none).isEmpty());
  command = balloon->command(QStringLiteral("D1"), 0, annotationPicks(1, {QPointF(5, 6)}), none);
  CHECK(command.value(QStringLiteral("type")).toString() == QStringLiteral("balloon"));
  CHECK(command.value(QStringLiteral("picks")).toArray().size() == 1);
  // A table's settings as its panel edits them.
  command = annotationEdit(QStringLiteral("D1"), 4, QStringLiteral("columns"), QStringLiteral("item, name ,Vendor"));
  CHECK(command.value(QStringLiteral("table")).toObject().value(QStringLiteral("columns")).toArray() ==
        QJsonArray({QStringLiteral("item"), QStringLiteral("name"), QStringLiteral("Vendor")}));
  CHECK(annotationEdit(QStringLiteral("D1"), 4, QStringLiteral("columns"), QStringLiteral(" , ")).isEmpty());
  command = annotationEdit(QStringLiteral("D1"), 4, QStringLiteral("sort"), QString());
  CHECK(command.value(QStringLiteral("table")).toObject().value(QStringLiteral("sort")).isNull());
  command = annotationEdit(QStringLiteral("D1"), 4, QStringLiteral("descending"), QStringLiteral("true"));
  CHECK(command.value(QStringLiteral("table")).toObject().value(QStringLiteral("descending")) == QJsonValue(true));
  command = annotationEdit(QStringLiteral("D1"), 4, QStringLiteral("tags"), QStringLiteral("numbers"));
  CHECK(command.value(QStringLiteral("table")).toObject().value(QStringLiteral("tags")).toString() ==
        QStringLiteral("numbers"));
  CHECK(annotationName(QStringLiteral("parts_list")) == QStringLiteral("Parts List"));
  CHECK(annotationName(QStringLiteral("balloon")) == QStringLiteral("Balloon"));
  // Drawn outside the model (mitcad#729): a line in the view under its
  // first click, or on the sheet; an arc of three points; a text in a
  // view when chosen.
  const DrawingToolDef* line = drawingTool(QStringLiteral("drawing.line"));
  const DrawingToolDef* arc = drawingTool(QStringLiteral("drawing.arc"));
  const DrawingToolDef* pattern = drawingTool(QStringLiteral("drawing.center_pattern"));
  const DrawingToolDef* extension = drawingTool(QStringLiteral("drawing.edge_extension"));
  CHECK(line && arc && pattern && extension);
  if (!(line && arc && pattern && extension)) {
    return failures;
  }
  CHECK(line->pick == SheetPick::Segment && arc->pick == SheetPick::ArcPoints);
  CHECK(line->command(QStringLiteral("D1"), 0, picks(2, {QPointF(1, 2)}), {}).isEmpty());
  QJsonObject drawn = line->command(QStringLiteral("D1"), 1, picks(2, {QPointF(1, 2), QPointF(3, 4)}), {});
  CHECK(drawn.value(QStringLiteral("type")).toString() == QStringLiteral("line"));
  CHECK(drawn.value(QStringLiteral("view")).toInt() == 2 && !drawn.contains(QStringLiteral("sheet")));
  CHECK(drawn.value(QStringLiteral("points")).toArray().size() == 2);
  CHECK(drawn.value(QStringLiteral("pen")).toString() == QStringLiteral("thin"));
  drawn = line->command(QStringLiteral("D1"), 1, picks(0, {QPointF(1, 2), QPointF(3, 4)}),
                        {{QStringLiteral("pen"), QStringLiteral("thick")}});
  CHECK(drawn.value(QStringLiteral("sheet")).toInt() == 1 && !drawn.contains(QStringLiteral("view")));
  CHECK(drawn.value(QStringLiteral("pen")).toString() == QStringLiteral("thick"));
  drawn = line->command(QStringLiteral("D1"), 1, picks(2, {QPointF(1, 2), QPointF(3, 4)}),
                        {{QStringLiteral("attach"), QStringLiteral("sheet")}});
  CHECK(drawn.value(QStringLiteral("sheet")).toInt() == 1);
  CHECK(arc->command(QStringLiteral("D1"), 0, picks(2, {QPointF(0, 0), QPointF(1, 0)}), {}).isEmpty());
  drawn = arc->command(QStringLiteral("D1"), 0, picks(2, {QPointF(0, 0), QPointF(1, 0), QPointF(0, 1)}), {});
  CHECK(drawn.value(QStringLiteral("type")).toString() == QStringLiteral("arc"));
  CHECK(drawn.value(QStringLiteral("points")).toArray().size() == 3);
  drawn = text->command(QStringLiteral("D1"), 0, picks(3, {QPointF(30, 40)}),
                        {{QStringLiteral("text"), QStringLiteral("KEEP CLEAR")},
                         {QStringLiteral("attach"), QStringLiteral("view")}});
  CHECK(drawn.value(QStringLiteral("view")).toInt() == 3 && !drawn.contains(QStringLiteral("sheet")));
  // A centre pattern of one hole; edge extensions of two edges, or one.
  drawn = pattern->command(QStringLiteral("D1"), 0, annotationPicks(1, {}), {});
  CHECK(drawn.value(QStringLiteral("type")).toString() == QStringLiteral("center_pattern"));
  CHECK(pattern->annotationType({}) == QStringLiteral("center_pattern"));
  CHECK(extension->pickCount({}) == 2);
  CHECK(extension->pickCount({{QStringLiteral("edges"), QStringLiteral("one")}}) == 1);
  CHECK(extension->command(QStringLiteral("D1"), 0, annotationPicks(1, {}), {}).isEmpty());
  drawn = extension->command(QStringLiteral("D1"), 0, annotationPicks(2, {}), {});
  CHECK(drawn.value(QStringLiteral("type")).toString() == QStringLiteral("edge_extension"));
  CHECK(toolGroup(*line) == 1 && toolGroup(*pattern) == 1 && toolGroup(*extension) == 1);
  CHECK(toolKeywords(QStringLiteral("drawing.center_pattern")).contains(QStringLiteral("bolt circle")));
  CHECK(annotationName(QStringLiteral("edge_extension")) == QStringLiteral("Edge Extension"));
  CHECK(annotationName(QStringLiteral("arc")) == QStringLiteral("Arc"));
  // A view turned (mitcad#729); an annotation's letters back to the
  // drawing's; a line's pen; the drawing's decimals and letters.
  command = viewEdit(QStringLiteral("D1"), 1, QStringLiteral("rotation"), QStringLiteral("-90"), QJsonObject());
  CHECK(command.value(QStringLiteral("rotation")).toDouble() == -90.0);
  CHECK(viewEdit(QStringLiteral("D1"), 1, QStringLiteral("rotation"), QStringLiteral("left"), QJsonObject()).isEmpty());
  command = annotationEdit(QStringLiteral("D1"), 3, QStringLiteral("height"), QString());
  CHECK(command.value(QStringLiteral("height")).isNull() && command.contains(QStringLiteral("height")));
  command = annotationEdit(QStringLiteral("D1"), 3, QStringLiteral("pen"), QStringLiteral("hidden"));
  CHECK(command.value(QStringLiteral("pen")).toString() == QStringLiteral("hidden"));
  command = drawingDefaultsEdit(QStringLiteral("D1"), QStringLiteral("decimals"), QStringLiteral("2"));
  CHECK(command.value(QStringLiteral("cmd")).toString() == QStringLiteral("edit_drawing"));
  CHECK(command.value(QStringLiteral("decimals")).toInt() == 2);
  command = drawingDefaultsEdit(QStringLiteral("D1"), QStringLiteral("decimals"), QStringLiteral("auto"));
  CHECK(command.value(QStringLiteral("decimals")).isNull());
  CHECK(drawingDefaultsEdit(QStringLiteral("D1"), QStringLiteral("decimals"), QStringLiteral("7")).isEmpty());
  command = drawingDefaultsEdit(QStringLiteral("D1"), QStringLiteral("text_height"), QStringLiteral("5"));
  CHECK(command.value(QStringLiteral("text_height")).toDouble() == 5.0);
  command = drawingDefaultsEdit(QStringLiteral("D1"), QStringLiteral("text_height"), QString());
  CHECK(command.value(QStringLiteral("text_height")).isNull());
  CHECK(drawingDefaultsEdit(QStringLiteral("D1"), QStringLiteral("text_height"), QStringLiteral("0")).isEmpty());
  CHECK(penChoices().size() == 4 && penChoices().first().first == QStringLiteral("thin"));
  // Sheet templates (mitcad#707 phase 4): the query's built-in ones by id, a
  // library's as its data, then the simple title block.
  const QJsonObject templates{
      {QStringLiteral("templates"),
       QJsonArray{QJsonObject{{QStringLiteral("id"), QStringLiteral("iso")},
                              {QStringLiteral("name"), QStringLiteral("ISO")},
                              {QStringLiteral("library"), QStringLiteral("builtin")},
                              {QStringLiteral("size"), QStringLiteral("A3")},
                              {QStringLiteral("projection"), QStringLiteral("first_angle")},
                              {QStringLiteral("choice"), QStringLiteral("iso")}},
                  QJsonObject{{QStringLiteral("id"), QStringLiteral("company-a3")},
                              {QStringLiteral("name"), QStringLiteral("Company A3")},
                              {QStringLiteral("library"), QStringLiteral("project")},
                              {QStringLiteral("choice"), QStringLiteral("project:company-a3")}}}}};
  const QList<TemplateChoice> choices = templateChoices(templates);
  CHECK(choices.size() == 3);
  CHECK(choices.at(0).value == QJsonValue(QStringLiteral("iso")) && choices.at(0).size == QStringLiteral("A3"));
  // Its projection method with it (mitcad#725); a template without one none.
  CHECK(choices.at(0).projection == QStringLiteral("first_angle") && choices.at(1).projection.isEmpty());
  CHECK(choices.at(1).label == QStringLiteral("Company A3 (Project)"));
  CHECK(choices.at(1).value.toObject().value(QStringLiteral("id")).toString() == QStringLiteral("company-a3"));
  CHECK(!choices.at(1).value.toObject().contains(QStringLiteral("choice")));
  CHECK(choices.at(2).choice.isEmpty() && choices.at(2).value == QJsonValue(QString()));
  CHECK(templateIndex(choices, QStringLiteral("project:company-a3")) == 1);
  CHECK(templateIndex(choices, QStringLiteral("din")) == -1);
  CHECK(defaultTemplate(QStringLiteral("ANSI_B")) == QStringLiteral("ansi"));
  CHECK(defaultTemplate(QStringLiteral("A4")) == QStringLiteral("iso"));
  return failures;
}
