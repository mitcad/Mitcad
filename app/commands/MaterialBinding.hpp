// SPDX-License-Identifier: MIT
#pragma once

#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>
#include <QVector>

namespace mitcad {

struct OptionMapping;

// The fx of the Material and Appearance tools (mitcad#1059, app/COMMANDS.md
// "Material and Appearance tools"): a body's material or appearance (a
// face's appearance) bound to an option, a row of the panel per value of
// the option and its result, written as switch(Option; value; result; ...;
// otherwise) on OK (OptionMapping, Bindings.hpp). These are the parts
// without a model or a widget: plain Qt Core, so that the unit tests use
// them.

// An option a value can vary with: one of the configuration's options (the
// `configure` query), or another user parameter with a list of allowed
// values.
struct BindingOption {
  QString name;
  bool text = false;
  bool closed = false; // a list of values: no Other row
  QStringList values;  // as expressions ('Black', 800 mm)
  QStringList shown;   // as the rows show them (Black, 800 mm)
  QString currentExpression;
  QString currentText;
};

// The options, the configuration's first and then the other user
// parameters with a list of values, from the answers of the `configure`
// query (empty without a configuration) and the `parameters` query.
QVector<BindingOption> bindingOptions(const QJsonObject& configure, const QJsonArray& parameters);
const BindingOption* findBindingOption(const QVector<BindingOption>& options, const QString& name);

// One row of the panel: an option's value and what it gives.
struct BindingRow {
  QString value;  // as an expression; empty for Other
  QString shown;  // as the panel names it
  QString result; // as an expression ('pla', 'Shop PETG'); empty: none
  bool other = false;
};

// The rows of an option, as the panel shows them.
struct BindingTable {
  QString option;
  QVector<BindingRow> rows;
  // A closed list's default kept from the expression read (no Other row).
  QString keptOtherwise;
};

// The rows of `name` (`option` its listing; null when the design no
// longer lists it): a row per value of the option, its result from
// `mapping` where it gives one, else `prefill` (what the first target has
// now, as an expression: nothing changes until a row does). Values the
// mapping names that the option does not list follow; an option of no
// closed list also gets the design's value now when no row has it, and
// Other, the switch()'s default.
BindingTable bindingTable(const QString& name, const BindingOption* option, const OptionMapping* mapping,
                          const QString& prefill);
// The expression the rows give: switch(...), or the default alone when no
// row has a result of its own; empty when there is neither.
QString bindingExpression(const BindingTable& table);
// Two values of an option as expressions: the same text, the same text
// literal, or the same number written with other spaces.
bool sameBindingValue(const QString& a, const QString& b);
// Whether `row` holds the option's value now ("Black (now)"); Other holds
// it when no other row does.
bool isCurrentRow(const BindingTable& table, const BindingOption* option, int row);

// The table in a command's state (CommandState::object) and back.
QJsonObject bindingTableJson(const BindingTable& table);
BindingTable bindingTableOf(const QJsonObject& json);

// What a picked target has: a body or a component (its default), and the
// name of what it has ("PLA Basic", "varies with Color").
struct TargetState {
  bool component = false;
  QString what;
};
// The targets by what they have, in their order: "3 bodies: PLA Basic, 1
// body: PETG"; components counted apart ("1 component: Steel"). Empty for
// no targets.
QString targetsSummary(const QVector<TargetState>& targets);

} // namespace mitcad
