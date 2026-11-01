// SPDX-License-Identifier: MIT
// The fx of the Material and Appearance tools (mitcad#1059): the options a
// value can vary with (the configuration's first, then the user parameters
// with a list of values), the rows of an option from a binding read back,
// the expression the rows give, the table kept in a command's state, and
// the summary of what the picked targets have.
#include "commands/MaterialBinding.hpp"

#include <cstdio>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>
#include <QVector>

#include "browser/Bindings.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "material_tool.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::BindingOption;
using mitcad::BindingTable;
using mitcad::OptionMapping;
using mitcad::TargetState;

QJsonObject listedValue(const char* expression, const char* text) {
  return QJsonObject{{QStringLiteral("expression"), QString::fromUtf8(expression)},
                     {QStringLiteral("text"), QString::fromUtf8(text)}};
}

// A configuration with Color ('Red', 'Black'; now Black) and a user
// parameter Size with a list of values; Count is a plain parameter.
QVector<BindingOption> options() {
  QJsonArray colorValues;
  colorValues.append(listedValue("'Red'", "Red"));
  colorValues.append(listedValue("'Black'", "Black"));
  QJsonObject color{{QStringLiteral("name"), QStringLiteral("Color")},
                    {QStringLiteral("kind"), QStringLiteral("text")},
                    {QStringLiteral("expression"), QStringLiteral("'Black'")},
                    {QStringLiteral("text"), QStringLiteral("Black")},
                    {QStringLiteral("domain"), QJsonObject{{QStringLiteral("kind"), QStringLiteral("list")},
                                                           {QStringLiteral("values"), colorValues}}}};
  QJsonArray configured;
  configured.append(color);
  const QJsonObject configure{{QStringLiteral("options"), configured}};
  QJsonArray sizes;
  sizes.append(QStringLiteral("10 mm"));
  sizes.append(QStringLiteral("20 mm"));
  QJsonArray parameters;
  parameters.append(QJsonObject{{QStringLiteral("name"), QStringLiteral("Size")},
                                {QStringLiteral("kind"), QStringLiteral("user")},
                                {QStringLiteral("expression"), QStringLiteral("20 mm")},
                                {QStringLiteral("limits"), QJsonObject{{QStringLiteral("values"), sizes}}}});
  parameters.append(QJsonObject{{QStringLiteral("name"), QStringLiteral("Count")},
                                {QStringLiteral("kind"), QStringLiteral("user")},
                                {QStringLiteral("expression"), QStringLiteral("4")}});
  // The configuration's option again as a parameter: listed once.
  QJsonArray colors;
  colors.append(QStringLiteral("'Red'"));
  parameters.append(QJsonObject{{QStringLiteral("name"), QStringLiteral("Color")},
                                {QStringLiteral("kind"), QStringLiteral("user")},
                                {QStringLiteral("limits"), QJsonObject{{QStringLiteral("values"), colors}}}});
  return mitcad::bindingOptions(configure, parameters);
}

void optionTests() {
  const QVector<BindingOption> listed = options();
  CHECK(listed.size() == 2);
  CHECK(listed.value(0).name == QStringLiteral("Color"));
  CHECK(listed.value(0).closed);
  CHECK(listed.value(0).values == (QStringList{QStringLiteral("'Red'"), QStringLiteral("'Black'")}));
  CHECK(listed.value(0).shown == (QStringList{QStringLiteral("Red"), QStringLiteral("Black")}));
  CHECK(listed.value(1).name == QStringLiteral("Size"));
  CHECK(listed.value(1).closed);
  CHECK(listed.value(1).shown == (QStringList{QStringLiteral("10 mm"), QStringLiteral("20 mm")}));
  CHECK(mitcad::findBindingOption(listed, QStringLiteral("Count")) == nullptr);
  CHECK(mitcad::sameBindingValue(QStringLiteral("20mm"), QStringLiteral("20 mm")));
  CHECK(mitcad::sameBindingValue(QStringLiteral("'Red'"), QStringLiteral("\"Red\"")));
  CHECK(!mitcad::sameBindingValue(QStringLiteral("'Red'"), QStringLiteral("'Black'")));
}

void tableTests() {
  const QVector<BindingOption> listed = options();
  const BindingOption* color = mitcad::findBindingOption(listed, QStringLiteral("Color"));
  CHECK(color != nullptr);
  // A binding made earlier (in the windows, mitcad#861) read back: its
  // rows in the option's order, the default kept for a closed list.
  OptionMapping mapping;
  CHECK(OptionMapping::parse(QStringLiteral("switch(Color; 'Black'; 'petg'; 'Red'; 'Shop PETG'; 'pla')"), &mapping));
  BindingTable table = mitcad::bindingTable(QStringLiteral("Color"), color, &mapping, QString());
  CHECK(table.rows.size() == 2);
  CHECK(table.rows.value(0).shown == QStringLiteral("Red"));
  CHECK(table.rows.value(0).result == QStringLiteral("'Shop PETG'"));
  CHECK(table.rows.value(1).result == QStringLiteral("'petg'"));
  CHECK(table.keptOtherwise == QStringLiteral("'pla'"));
  CHECK(!mitcad::isCurrentRow(table, color, 0));
  CHECK(mitcad::isCurrentRow(table, color, 1)); // Black is the design's value now
  // Written back in the option's order, the default last.
  CHECK(mitcad::bindingExpression(table) == QStringLiteral("switch(Color; 'Red'; 'Shop PETG'; 'Black'; 'petg'; 'pla')"));
  // A row left as the default is left out; none left: the default alone.
  table.rows[0].result.clear();
  CHECK(mitcad::bindingExpression(table) == QStringLiteral("switch(Color; 'Black'; 'petg'; 'pla')"));
  table.rows[1].result.clear();
  CHECK(mitcad::bindingExpression(table) == QStringLiteral("'pla'"));
  table.keptOtherwise.clear();
  CHECK(mitcad::bindingExpression(table).isEmpty());

  // A new table of an option: every value prefilled, nothing changes until
  // a row does.
  const BindingTable fresh = mitcad::bindingTable(QStringLiteral("Color"), color, nullptr, QStringLiteral("'steel'"));
  CHECK(fresh.rows.size() == 2);
  CHECK(fresh.rows.value(0).result == QStringLiteral("'steel'"));
  CHECK(fresh.keptOtherwise.isEmpty());

  // An option the design no longer lists: the mapping's values and Other.
  OptionMapping gone;
  CHECK(OptionMapping::parse(QStringLiteral("switch(Finish; 'Matt'; 'paint_black'; 'paint_white')"), &gone));
  const BindingTable old = mitcad::bindingTable(QStringLiteral("Finish"), nullptr, &gone, QString());
  CHECK(old.rows.size() == 2);
  CHECK(old.rows.value(0).shown == QStringLiteral("Matt"));
  CHECK(old.rows.value(1).other && old.rows.value(1).result == QStringLiteral("'paint_white'"));
  CHECK(mitcad::bindingExpression(old) == QStringLiteral("switch(Finish; 'Matt'; 'paint_black'; 'paint_white')"));

  // Kept in a command's state and read back.
  const BindingTable back = mitcad::bindingTableOf(mitcad::bindingTableJson(old));
  CHECK(back.option == QStringLiteral("Finish"));
  CHECK(back.rows.size() == 2);
  CHECK(back.rows.value(1).other);
  CHECK(mitcad::bindingExpression(back) == mitcad::bindingExpression(old));
}

void summaryTests() {
  QVector<TargetState> targets;
  CHECK(mitcad::targetsSummary(targets).isEmpty());
  targets.append(TargetState{false, QStringLiteral("PLA Basic")});
  CHECK(mitcad::targetsSummary(targets) == QStringLiteral("1 body: PLA Basic"));
  targets.append(TargetState{false, QStringLiteral("PETG")});
  targets.append(TargetState{false, QStringLiteral("PLA Basic")});
  targets.append(TargetState{false, QStringLiteral("PLA Basic")});
  CHECK(mitcad::targetsSummary(targets) == QStringLiteral("3 bodies: PLA Basic, 1 body: PETG"));
  targets.append(TargetState{true, QStringLiteral("Steel")});
  CHECK(mitcad::targetsSummary(targets) == QStringLiteral("3 bodies: PLA Basic, 1 body: PETG, 1 component: Steel"));
}

} // namespace

int materialToolTests() {
  optionTests();
  tableTests();
  summaryTests();
  return failures;
}
