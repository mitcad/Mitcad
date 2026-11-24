// SPDX-License-Identifier: MIT
// Weld symbols' tools (mitcad#2350): Weld Symbol's options (the symbols
// chosen from icons first, then their values) and the model's command
// they make, a selected weld symbol's fields edited one at a time, and
// Propose Welds' command for the members picked on a view.
#include "UnitTests.hpp"
#include "drawings/DrawingTools.hpp"

#include <cstdio>
#include <utility>

#include <QJsonArray>
#include <QJsonObject>
#include <QStringList>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "drawing_weld_tools.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

mitcad::drawings::ToolPicks pickedOn(int view, QVector<QPointF> points, int count) {
  mitcad::drawings::ToolPicks out;
  out.view = view;
  out.points = std::move(points);
  for (int i = 0; i < count; ++i) {
    out.picks.append(QJsonObject{{QStringLiteral("body"), QStringLiteral("F2.b0")},
                                 {QStringLiteral("edge"), QStringLiteral("E%1").arg(i)}});
  }
  return out;
}

} // namespace

int drawingWeldToolsTests() {
  using namespace mitcad::drawings;
  const QHash<QString, QString> none;
  // Weld Symbol: in Symbols, one pick, then where its reference line
  // starts; its first option the arrow side's symbol from icons.
  const DrawingToolDef* weld = drawingTool(QStringLiteral("drawing.weld"));
  CHECK(weld != nullptr);
  if (weld != nullptr) {
    CHECK(weld->name == QStringLiteral("Weld Symbol") && toolGroup(*weld) == 4);
    CHECK(weld->pick == SheetPick::Picks && weld->place && weld->iconOnly);
    CHECK(weld->pickCount(none) == 1 && weld->annotationType(none) == QStringLiteral("weld"));
    CHECK(!weld->options.isEmpty() && weld->options.first().id == QStringLiteral("arrow_symbol"));
    CHECK(weld->options.first().kind == ToolOption::Kind::Icons);
    CHECK(optionValue(*weld, none, QStringLiteral("arrow_symbol")) == QStringLiteral("fillet"));
    CHECK(optionValue(*weld, none, QStringLiteral("other_symbol")) == QStringLiteral("none"));
    // Every symbol with its icon.
    for (const ToolOption& option : weld->options) {
      if (option.kind == ToolOption::Kind::Icons) {
        CHECK(option.choices.size() == 16);
        CHECK(choiceIcon(option, QStringLiteral("flare_v")) == QStringLiteral("weld-flare-v"));
        CHECK(choiceIcon(option, QStringLiteral("none")) == QStringLiteral("weld-none"));
      }
    }
    CHECK(weldSymbolChoices(false).size() == 15 && weldSymbolChoices(false).first().first == QStringLiteral("fillet"));
    // Nothing before its pick and its place.
    CHECK(weld->command(QStringLiteral("D1"), 0, pickedOn(1, {}, 1), none).isEmpty());
    // A 5 mm fillet all around with its process.
    QJsonObject command = weld->command(QStringLiteral("D1"), 0, pickedOn(1, {QPointF(140, 140)}, 1),
                                        {{QStringLiteral("size"), QStringLiteral("5")},
                                         {QStringLiteral("all_around"), QStringLiteral("true")},
                                         {QStringLiteral("tail"), QStringLiteral(" 141 ")}});
    CHECK(command.value(QStringLiteral("cmd")).toString() == QStringLiteral("add_drawing_annotation"));
    CHECK(command.value(QStringLiteral("type")).toString() == QStringLiteral("weld"));
    CHECK(command.value(QStringLiteral("at")).toArray() == QJsonArray({140.0, 140.0}));
    const QJsonObject symbol = command.value(QStringLiteral("weld")).toObject();
    const QJsonObject arrow = symbol.value(QStringLiteral("arrow_side")).toObject();
    CHECK(arrow.value(QStringLiteral("symbol")).toString() == QStringLiteral("fillet"));
    CHECK(arrow.value(QStringLiteral("size")).toDouble() == 5.0);
    CHECK(!arrow.contains(QStringLiteral("measure")) && !arrow.contains(QStringLiteral("contour")));
    CHECK(!symbol.contains(QStringLiteral("other_side")));
    CHECK(symbol.value(QStringLiteral("all_around")).toBool());
    CHECK(!symbol.contains(QStringLiteral("field")));
    CHECK(symbol.value(QStringLiteral("tail")).toString() == QStringLiteral("141"));
    // A size that is no number: no command.
    CHECK(weld->command(QStringLiteral("D1"), 0, pickedOn(1, {QPointF(1, 1)}, 1),
                        {{QStringLiteral("size"), QStringLiteral("five")}})
              .isEmpty());
  }
  // The symbol's fields one at a time: a V on both sides, an intermittent
  // weld's lengths on both, a decimal comma, a leg only for a fillet.
  QJsonObject symbol = weldOf({{QStringLiteral("arrow_symbol"), QStringLiteral("fillet")},
                               {QStringLiteral("measure"), QStringLiteral("leg")},
                               {QStringLiteral("size"), QStringLiteral("4,5")}});
  CHECK(symbol.value(QStringLiteral("arrow_side")).toObject().value(QStringLiteral("measure")).toString() ==
        QStringLiteral("leg"));
  CHECK(symbol.value(QStringLiteral("arrow_side")).toObject().value(QStringLiteral("size")).toDouble() == 4.5);
  symbol = weldEdit(symbol, QStringLiteral("arrow_symbol"), QStringLiteral("v"));
  CHECK(symbol.value(QStringLiteral("arrow_side")).toObject().value(QStringLiteral("symbol")).toString() ==
        QStringLiteral("v"));
  CHECK(!symbol.value(QStringLiteral("arrow_side")).toObject().contains(QStringLiteral("measure")));
  symbol = weldEdit(symbol, QStringLiteral("other_symbol"), QStringLiteral("v"));
  symbol = weldEdit(symbol, QStringLiteral("length"), QStringLiteral("50"));
  symbol = weldEdit(symbol, QStringLiteral("pitch"), QStringLiteral("150"));
  const QJsonObject other = symbol.value(QStringLiteral("other_side")).toObject();
  CHECK(other.value(QStringLiteral("length")).toDouble() == 50.0 && other.value(QStringLiteral("pitch")).toDouble() == 150.0);
  symbol = weldEdit(symbol, QStringLiteral("count"), QStringLiteral("3"));
  CHECK(symbol.value(QStringLiteral("arrow_side")).toObject().value(QStringLiteral("count")).toInt() == 3);
  CHECK(weldEdit(symbol, QStringLiteral("count"), QStringLiteral("2.5")).isEmpty());
  CHECK(weldEdit(symbol, QStringLiteral("length"), QStringLiteral("-1")).isEmpty());
  symbol = weldEdit(symbol, QStringLiteral("contour"), QStringLiteral("convex"));
  symbol = weldEdit(symbol, QStringLiteral("staggered"), QStringLiteral("true"));
  CHECK(symbol.value(QStringLiteral("staggered")).toBool());
  // Its fields as the panel shows them, and back.
  const QHash<QString, QString> fields = weldFields(symbol);
  CHECK(fields.value(QStringLiteral("arrow_symbol")) == QStringLiteral("v"));
  CHECK(fields.value(QStringLiteral("other_symbol")) == QStringLiteral("v"));
  CHECK(fields.value(QStringLiteral("length")) == QStringLiteral("50"));
  CHECK(fields.value(QStringLiteral("contour")) == QStringLiteral("convex"));
  CHECK(fields.value(QStringLiteral("finish")) == QStringLiteral("none"));
  CHECK(fields.value(QStringLiteral("staggered")) == QStringLiteral("true"));
  CHECK(weldOf(fields) == symbol);
  // The other side's weld taken away; a field of a side without a weld.
  symbol = weldEdit(symbol, QStringLiteral("other_symbol"), QStringLiteral("none"));
  CHECK(!symbol.contains(QStringLiteral("other_side")));
  CHECK(weldEdit(symbol, QStringLiteral("other_size"), QStringLiteral("3")).isEmpty());
  // A proposal's joint stays through an edit.
  QJsonObject proposal{{QStringLiteral("arrow_side"), QJsonObject{{QStringLiteral("symbol"), QStringLiteral("fillet")}}},
                       {QStringLiteral("joint"), QStringLiteral("C3:end_a|C2:side")}};
  proposal = weldEdit(proposal, QStringLiteral("size"), QStringLiteral("6"));
  CHECK(proposal.value(QStringLiteral("joint")).toString() == QStringLiteral("C3:end_a|C2:side"));
  // Propose Welds: the members clicked on a view (one at least), then a
  // click beside them; a fillet of the panel's size and tail.
  const DrawingToolDef* propose = drawingTool(QStringLiteral("drawing.propose_welds"));
  CHECK(propose != nullptr);
  if (propose != nullptr) {
    CHECK(propose->name == QStringLiteral("Propose Welds") && toolGroup(*propose) == 4);
    CHECK(propose->pick == SheetPick::Picks && !propose->place && propose->pickCount(none) == kOpenPicks);
    CHECK(propose->command(QStringLiteral("D1"), 0, pickedOn(1, {QPointF(9, 9)}, 0), none).isEmpty());
    const QJsonObject command = propose->command(QStringLiteral("D1"), 0, pickedOn(2, {QPointF(9, 9)}, 2),
                                                 {{QStringLiteral("size"), QStringLiteral("4")},
                                                  {QStringLiteral("tail"), QStringLiteral("135")}});
    CHECK(command.value(QStringLiteral("cmd")).toString() == QStringLiteral("propose_drawing_welds"));
    CHECK(command.value(QStringLiteral("view")).toInt() == 2);
    CHECK(command.value(QStringLiteral("picks")).toArray().size() == 2);
    const QJsonObject asked = command.value(QStringLiteral("weld")).toObject();
    CHECK(asked.value(QStringLiteral("arrow_side")).toObject().value(QStringLiteral("size")).toDouble() == 4.0);
    CHECK(asked.value(QStringLiteral("tail")).toString() == QStringLiteral("135"));
  }
  CHECK(annotationName(QStringLiteral("weld")) == QStringLiteral("Weld Symbol"));
  CHECK(toolKeywords(QStringLiteral("drawing.weld")).contains(QStringLiteral("welding")));
  return failures;
}
MITCAD_UNIT_TEST(drawingWeldToolsTests);
