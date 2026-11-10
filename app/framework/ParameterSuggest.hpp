// SPDX-License-Identifier: MIT
#pragma once

// The parameters an expression field offers while it is typed in and in
// its fx browser (mitcad#567, mitcad#568): which parameters there are,
// which identifier at the caret is being typed, which parameters match it
// and in what order, what choosing one does to the text, and which choices
// the field cannot take. Text rules without widgets, so that they are unit
// tested (app/tests/parameter_suggest.cpp); the list and the fx
// button are framework/ParameterField.hpp.
//
// The rules follow the model's expressions (core/expr/src/lib.rs):
// a name is a letter or `_` and then letters, digits and `_`; units,
// functions and constants are reserved; names are case-sensitive; a word
// right after a number, a name or `)` is a unit; `name = expression` names
// the field's own parameter (commands.md, Values).

#include <functional>
#include <vector>

#include <QHash>
#include <QJsonArray>
#include <QString>
#include <QStringList>

namespace mitcad {

// What a value is: a length, an angle, a plain number (counts, factors) or
// a text (a text parameter, unit `Text`, mitcad#664).
enum class ParameterKind { Length, Angle, Unitless, Text };

// A parameter as the `parameters` query gives it.
struct SuggestedParameter {
  enum class Group { User, Model };
  QString name;
  QString expression;
  QString text; // its value in its unit: "12.5 mm"
  QString unit; // "" for unitless
  QString comment;
  QString owner;            // the feature of a model or reference parameter
  QStringList dependencies; // the names its expression uses
  Group group = Group::Model;
  bool reference = false; // a driven dimension's, read-only
  ParameterKind kind = ParameterKind::Length;
};

// The parameters of a `parameters` query's answer, in its order.
std::vector<SuggestedParameter> suggestedParameters(const QJsonArray& query);

// The kind a unit gives: none is unitless, rad, deg, ° and grad are
// angles, Text is a text, every other unit a length.
ParameterKind kindOfUnit(const QString& unit);
// "length", "angle", "number", "text".
QString parameterKindName(ParameterKind kind);

// Whether `name` can name a parameter (the model's is_valid_name): a
// letter or `_`, then letters, digits and `_`, and not a unit, a function
// or a constant.
bool isValidParameterName(const QString& name);
// Whether a name is a unit, a function or a constant of expressions.
bool isReservedName(const QString& name);

// The identifier the caret is in or right after, being typed.
struct NameFragment {
  int start = 0;  // where the identifier starts
  int length = 0; // the whole identifier, also what follows the caret
  QString typed;  // its part before the caret: what the list is filtered by
  // Whether the list is to be offered for it: false where the word is a
  // number, a unit (right after a number, a name or `)`), a function's
  // name (before `(`) or the name of `name = expression`.
  bool active = false;
  // Whether it is the name of a leading `name = …` (the field's own
  // parameter's new name, mitcad#231).
  bool assigned = false;
};
NameFragment nameFragmentAt(const QString& text, int caret);

// `text` with the parameter's name put in: in place of the fragment's
// whole identifier when there is one (`width / 2 + thi` and `thickness`
// give `width / 2 + thickness`), otherwise at the caret, in place of the
// selection when there is one (`selectionStart` >= 0, `selectionLength`
// characters). `caret` gets where the caret goes: after the name; `from`
// and `length` what the name replaces.
struct Insertion {
  QString text;
  int caret = 0;
  int from = 0;
  int length = 0;
};
Insertion insertParameter(const QString& text, int caret, int selectionStart, int selectionLength,
                          const QString& name);

// The expression part of a field's text: after a leading `name =` (a
// single `=`), trimmed.
QString expressionPart(const QString& text);
// Where expressionPart's text starts in `text`: after the `name =` and the
// spaces that follow it, where the match found them, not where the
// expression's text first occurs ("t2 = 2" starts at 5, mitcad#755).
int expressionStart(const QString& text);
// The name of a leading `name =` (a single `=`) in a field's text; empty
// when there is none (mitcad#1577).
QString namedPrefix(const QString& text);
// `d12` and the like: the names the model gives new dimension parameters,
// `d` and a number without a leading zero (mitcad#1577).
bool isAutomaticName(const QString& name);

// A choice in the list: the parameter and why the field cannot take it
// (empty when it can).
struct ParameterChoice {
  const SuggestedParameter* parameter = nullptr;
  QString problem;
};

// The field the choices are for: its kind and its own parameter (never
// offered: it cannot use itself, and those that use it would make a
// cycle).
struct SuggestContext {
  ParameterKind kind = ParameterKind::Length;
  bool anyKind = false; // takes any kind (a parameter table's row)
  QString own;
  // The own parameter is kept as a user parameter when the value uses it
  // (a sketch dimension's, mitcad#1535; a command panel field's,
  // mitcad#1536): those that use it are no cycle.
  bool keepOwn = false;
  // The sketch the field is in (its uid): the references its own driven
  // dimensions measure cannot be used in it (commands.md, Values).
  QString sketch;
};

// The parameters that match `filter` (case-insensitive): those whose name
// starts with it, then those whose name contains it, then (with
// `comments`) those whose comment contains it; everything for an empty
// filter. User parameters come before model parameters; within a group
// the better match first, then by name with numbers in order (d2 before
// d10). The field's own parameter is left out. A choice's problem says why
// the field cannot take it: a parameter that uses the field's own (a
// cycle, unless the context keeps the own one), or one of another kind when it would be the field's whole
// expression (`insertedText`: the text after the choice, as
// insertParameter gives it; an angle inside `d1 * tan(a)` is fine).
std::vector<ParameterChoice> matchParameters(const std::vector<SuggestedParameter>& parameters,
                                             const QString& filter, bool comments,
                                             const SuggestContext& context,
                                             const std::function<QString(const QString&)>& insertedText);

// Why a parameter of kind `parameter` cannot be the whole expression of a
// field of kind `field`; empty when it can (a unitless one goes anywhere).
QString kindProblem(ParameterKind field, ParameterKind parameter);

// The names that use `name`, directly or through others.
QStringList dependentsOf(const std::vector<SuggestedParameter>& parameters, const QString& name);

// Natural order of names: "d2" before "d10", case-insensitive first.
bool naturalLess(const QString& a, const QString& b);

// Why a name cannot be a new user parameter's: invalid, reserved or taken
// (`existing` gets the parameter of that name, if any); empty when it can.
QString newNameProblem(const std::vector<SuggestedParameter>& parameters, const QString& name,
                       const SuggestedParameter** existing = nullptr);

// A bare number gets the unit ("30" with "mm" is "30 mm", "2,5" is
// "2.5 mm"); anything else stays as it is.
QString numberWithUnit(const QString& expression, const QString& unit);

// Tables of the design (mitcad#667, docs/table-lookups.md): a formula
// names a column as `Table.Column`, inside XLOOKUP, MATCH, INDEX and the
// aggregates. What the fields offer for them: the table names among the
// parameters, the column names after `Table.`.
struct SuggestedTable {
  struct Column {
    QString name;
    QString unit; // "" unitless, "Text" for texts
  };
  QString name;
  QString comment;
  std::vector<Column> columns;
};

// The tables of a `formula_help` answer's `tables` ({name, columns:
// [{name, unit}]}), in its order.
std::vector<SuggestedTable> suggestedTables(const QJsonArray& tables);

// The column name at the caret after `Table.` (no spaces around the dot,
// as the model's lexer has it): the table's name, where the column's name
// starts, its whole length, and its part before the caret. Active also
// right after the dot, with nothing typed yet. Not after a number
// ("2.5") or a chain ("a.b.c").
struct ColumnFragment {
  bool active = false;
  QString table;
  int start = 0;
  int length = 0;
  QString typed;
};
ColumnFragment columnFragmentAt(const QString& text, int caret);

// The table named `name` (case-sensitive, as names are), or null.
const SuggestedTable* tableNamed(const std::vector<SuggestedTable>& tables, const QString& name);

// The columns of a table whose name starts with `filter`, then those whose
// name contains it (case-insensitive), each group in the table's order.
std::vector<SuggestedTable::Column> matchColumns(const SuggestedTable& table, const QString& filter);

// The tables whose name starts with `filter`, then those whose name (or,
// with `comments`, comment) contains it; all for an empty filter.
std::vector<const SuggestedTable*> matchTables(const std::vector<SuggestedTable>& tables, const QString& filter,
                                               bool comments);

// "Size [Text]", "dk [mm]", "n []": a column as the table CSV's header
// writes it.
QString columnLabel(const SuggestedTable::Column& column);

} // namespace mitcad
