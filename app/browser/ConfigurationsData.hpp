// SPDX-License-Identifier: MIT
#pragma once

// The configurations' JSON as the application reads and sends it
// (mitcad#807): the one place that knows the model's form of the table
// (the `configurations` query, `set_configurations`), of the options'
// reports (the `configure` query) and of the cascade's moves and
// refusals (`configure`, `apply_configuration`, `set_parameter`).
// core/model-api/src/commands.md, "Configurations: options, rows and
// rules", is the model's side. Plain Qt Core: the Configurations window,
// the Configuration bar and Change Parameters read through it, and the
// unit tests (app/tests/configurations_data.cpp) check it without a model.

#include <optional>

#include <QHash>
#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QPair>
#include <QString>
#include <QStringList>
#include <QVector>

namespace mitcad::configurations {

// A value of an option's list: its value (mm or rad, or the text), as
// shown, and as an expression (`800 mm`, `'Black'`).
struct DomainValue {
  QJsonValue value;
  QString text;
  QString expression;
};

// What an option can be: its domain before the rules (the UI's Possible
// values), or what the rules allow now (Allowed values). A list, a range
// (each bound may be missing), or free.
struct Domain {
  enum class Kind { Free, List, Range };
  Kind kind = Kind::Free;
  QVector<DomainValue> values; // a list
  std::optional<double> min;   // a range, in mm or rad
  std::optional<double> max;
  std::optional<double> step;
  QString minText; // as shown, with the unit
  QString maxText;
  QString stepText;

  // "Black, White", "300 mm … 2000 mm, step 10 mm", "free".
  QString summary() const;
  // A list value whose text is `text`; null when there is none.
  const DomainValue* find(const QString& text) const;
  // Whether a slider can choose in it: a range with both bounds.
  bool hasSlider() const { return kind == Kind::Range && min && max && *max > *min; }
};

// An option as the `configure` query reports it.
struct OptionReport {
  QString name;
  bool text = false; // a text option (`kind` text), else a number
  QString unit;
  QJsonValue value;  // mm or rad, or the text
  QString valueText; // as shown: `1500 mm`, `Black`
  QString expression;
  bool driven = false;
  Domain domain;
  Domain allowed;
  QStringList restrictedBy; // the rules that narrow it now
};

// A move of the cascade: `Length 1500 mm → 1000 mm: Black is short`.
struct Move {
  QString option;
  QString from;
  QString to;
  QString rule;
  QString text;

  // The model's text, else made from the parts.
  QString describe() const;
};

// A change the cascade cannot repair: the rule and its message.
struct Refusal {
  QString rule;
  QString message;
  QString text;

  // The model's text, else "the rule <rule>: <message>".
  QString describe() const;
};

// The `configure` query's answer.
struct Report {
  QVector<OptionReport> options; // in choice order
  QString row;                   // the combination's row, empty for none
  QVector<Move> moves;
  std::optional<Refusal> refused;
  QStringList problems;

  const OptionReport* option(const QString& name) const;
};

// A row (a configuration row): a named, saved combination of option values
// (expressions), and in a closed table (Only these rows) of other
// parameters' values too, its parameter columns.
struct Row {
  QString name;
  QVector<QPair<QString, QString>> values; // parameter, expression; in the table's order
  QString designation;
  QString partNumber;
  QString supplierUrl;
  QJsonObject other; // fields this application does not edit, kept as read

  QString value(const QString& parameter) const;
  void setValue(const QString& parameter, const QString& expression);
};

// The table. What the window does not edit itself (the rules, which the
// Rules page edits, and any field it does not know) is kept as read and
// sent back unchanged.
struct Table {
  QStringList options;
  bool closed = false;
  QStringList parameters; // a closed table's other parameters
  QString defaultRow;
  QVector<Row> rows;
  QJsonObject other; // `rules` and fields this application does not edit

  bool isEmpty() const { return options.isEmpty() && rows.isEmpty() && rules().isEmpty(); }
  QJsonArray rules() const { return other.value(QStringLiteral("rules")).toArray(); }
  const Row* row(const QString& name) const;
  Row* row(const QString& name);
  // Row1, Row2, ...: the first name no row has.
  QString nextRowName() const;
  // The table in the form set_configurations takes.
  QJsonObject toJson() const;
};

// Reading the model's answers.
Table readTable(const QJsonObject& configurationsQuery);
Domain readDomain(const QJsonValue& domain);
Report readReport(const QJsonObject& configureQuery);
QVector<Move> readMoves(const QJsonValue& moves);
std::optional<Refusal> readRefusal(const QJsonValue& refused);

// What is sent to the model.
QJsonObject configurationsQuery();
// The `configure` query, with values to try (none: the current ones).
QJsonObject configureQuery(const QJsonObject& values = {});
QJsonObject setTableCommand(const Table& table);
QJsonObject configureCommand(const QJsonObject& values);
QJsonObject applyCommand(const QString& row);

// An option's value as `configure` takes it, from what the user typed or
// chose: a text option's text without quotes, a number option's
// expression with the option's unit for a bare number (`800` → `800 mm`).
QJsonValue chooserValue(const OptionReport& option, const QString& typed);
// A list value of an option as `configure` takes it.
QJsonValue chooserValue(const OptionReport& option, const DomainValue& value);
// A row's value of an option as an expression: a text option's text in
// quotes (`Black` → `'Black'`), a number option's with its unit.
QString rowExpression(bool text, const QString& unit, const QString& typed);
// A row's expression as the Rows sheet shows it: a text without quotes.
QString rowText(bool text, const QString& expression);
// A row's option values as the `configure` query takes them (a row
// leaves out the options it does not set).
QJsonObject rowChoice(const Table& table, const Row& row, const Report& report);

// The moves as one notice: "Length 1500 mm → 1000 mm: Black is short; …".
QString movesNotice(const QVector<Move>& moves);

// Configurations and tables (mitcad#806, #849; commands.md "Configurations
// and tables").

// Rows generated from a table: `generate_configuration_rows`'s answer.
struct GeneratedRows {
  QStringList rows;    // the rows of the column's keys, in the table's order
  QStringList added;   // new rows
  QStringList updated; // rows of the same name whose value changed
  QStringList created; // parameters the columns made (mitcad#1044)
  QVector<Move> moves;

  // "3 rows from Screws: added M3, M4, M5", "4 rows from Screws: added M6;
  // updated M4", "3 rows from Screws: all up to date".
  QString describe(const QString& table) const;
};

QJsonObject generateRowsCommand(const QString& table, const QString& column, const QString& option);
GeneratedRows readGeneratedRows(const QJsonObject& answer);

// The column a parameter's allowed values come from (`Screws.Size`): its
// `values` limit when that is a column (a text) rather than a list; empty
// otherwise. `parameter` is an entry of the `parameters` query.
QString valuesColumn(const QJsonObject& parameter);
// `Table.Column`, as a `values` limit and a domain name it.
QString columnReference(const QString& table, const QString& column);

// The option Generate Rows from Table offers first for a key column: the
// option whose values are that column, else the option of the column's
// name, else the user parameter of that name, else the first option.
// `parameters` are the user parameters by name (the `parameters` query's
// entries), `userParameters` their names in the model's order.
QString keyOption(const QString& table, const QString& column, const QStringList& options,
                  const QStringList& userParameters, const QHash<QString, QJsonObject>& parameters);
// The options a refusal names (its texts mention them), in the table's
// order; the cells the Rows sheet marks.
QStringList optionsNamed(const QStringList& options, const Refusal& refusal);

// A closed table's parameter columns (mitcad#999): the other parameters
// its rows set, as the d, P, s and k of a library screw.

// The user parameters Add Column offers: those that are neither an option
// nor a column already, in the model's order.
QStringList columnCandidates(const Table& table, const QStringList& userParameters);
// Add Column: the table, closed, with `parameter` among its `parameters`
// and every row's value of it `expression` (the parameter's current
// expression). An option or a column already gives the table unchanged.
Table withColumn(const Table& table, const QString& parameter, const QString& expression);
// Remove Column: the table without the column and the rows' values of it;
// the parameter itself stays. A parameter that is no column gives the
// table unchanged.
Table withoutColumn(const Table& table, const QString& parameter);

// Filling a closed table (mitcad#1044; docs/configurations.md, "Filling a
// closed table", decision 12).

// The user parameters Add Option offers: every one that is no option yet,
// a closed table's parameter columns among them, in the model's order.
QStringList optionCandidates(const Table& table, const QStringList& userParameters);
// A parameter's current value as the constant a row keeps: its expression
// when it names no other parameter or table, else its value as shown
// (`8.5 mm`, a text in quotes). `parameter` is an entry of the
// `parameters` query.
QString currentConstant(const QJsonObject& parameter);
// Add Option: the table with `parameter` appended to its options. A
// parameter column moves to the options with its rows' values; every row
// without a value of it takes `expression` (the parameter's current
// value, currentConstant), so that a closed table with rows takes it.
// An option already gives the table unchanged.
Table withOption(const Table& table, const QString& parameter, const QString& expression);

// Generate Rows from Table… with the table's other columns as the closed
// table's parameter columns (`columns`; empty: the key only).
QJsonObject generateRowsCommand(const QString& table, const QString& column, const QString& option,
                                const QStringList& columns);

// The rows as CSV: the export (`configuration_rows_csv`), the preview of an
// import and the import (`configuration_rows_csv_preview`,
// `import_configuration_rows_csv`).
QJsonObject rowsCsvQuery();
QJsonObject rowsCsvPreviewQuery(const QString& csv);
QJsonObject importRowsCsvCommand(const QString& csv);

// A block of cells as spreadsheets copy it: rows by line, cells by tab; a
// cell with a tab, a line break or a leading quote in quotes, quotes
// doubled. readBlock leaves out the line break at the end.
QVector<QStringList> readBlock(const QString& text);
QString writeBlock(const QVector<QStringList>& block);

// What a column of the Rows sheet sets.
struct SheetColumn {
  enum class Kind { Name, Option, Parameter, Designation, PartNumber, SupplierUrl };
  Kind kind = Kind::Name;
  QString parameter; // an option's or a parameter column's
  bool text = false; // a text parameter
  QString unit;      // a number parameter's, for a bare number
};

// A block pasted into the Rows sheet.
struct Paste {
  Table table;
  QStringList added;                   // rows the paste added, as their names
  QVector<QPair<QString, int>> cells;  // each pasted cell: its row's name and its column
  int beyond = 0;                      // cells right of the last column, left out
};

// Pastes `block` with its top left cell at (`row`, `column`) of the sheet
// whose columns are `columns`: each cell as typed into it (a text gets its
// quotes, a bare number its parameter's unit, an empty cell takes an
// option's or a parameter's value out; an empty name is left as it is).
// Rows below the last are added as needed, each a copy of `fresh` (the
// options' values as they are now) named as Add Row names it.
Paste pasteBlock(const Table& table, const QVector<SheetColumn>& columns, int row, int column,
                 const QVector<QStringList>& block, const Row& fresh);
// Delete in the Rows sheet: the cells (row index, column) emptied; a name
// stays.
Table clearCells(const Table& table, const QVector<SheetColumn>& columns, const QVector<QPair<int, int>>& cells);

// The Options tab's Value cell (mitcad#968; docs/ui-conventions.md,
// "Configurations"): the option's current value, not a stored default.
// An option the user sets shows the value the `configure` query reports
// (`text`: `1200 mm`, `Black`), else its parameter's constant as the Rows
// sheet shows it (`expression`, rowText); it is edited there. A By Formula
// option (`driven`) shows its evaluated value, never the formula: the
// expression that gives it is the cell's `expression` (its tooltip), and
// the cell is not edited.
struct ValueCell {
  QString text;
  QString expression; // a By Formula option's expression; empty otherwise
  bool editable = true;
};
ValueCell valueCell(const OptionReport* report, bool text, const QString& expression);

} // namespace mitcad::configurations
