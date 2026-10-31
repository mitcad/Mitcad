// SPDX-License-Identifier: MIT
// The configurations' JSON as the application reads and sends it
// (mitcad#807): the table read and written back with what the window does
// not edit (the rules) kept, the options' reports of the `configure`
// query, the moves' notice, the values typed into the choosers and the
// Rows sheet, and the options a refusal names; rows generated from a
// table and an option's values from a column (mitcad#849); a closed
// table's parameter columns added and removed (mitcad#999).
#include "browser/ConfigurationsData.hpp"

#include <cstdio>

#include <QHash>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QString>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "configurations_data.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

QJsonObject parse(const char* json) { return QJsonDocument::fromJson(QByteArray(json)).object(); }

void tableTests() {
  using namespace mitcad::configurations;
  const Table table = readTable(parse(R"({"configurations": {
      "options": ["Color", "Length"], "closed": false, "default": "Standard",
      "rows": [{"name": "Standard", "values": {"Length": "1200 mm", "Color": "'White'"},
                "designation": "Shelf", "part_number": "S-1", "note": "kept"}],
      "rules": [{"name": "Black is short", "when": "Color == 'Black'",
                 "then": [{"option": "Length", "max": "1000 mm"}]}],
      "future": 1}, "current": "Standard", "problems": []})"));
  CHECK(table.options == QStringList({QStringLiteral("Color"), QStringLiteral("Length")}));
  CHECK(!table.closed);
  CHECK(table.defaultRow == QStringLiteral("Standard"));
  CHECK(table.rows.size() == 1);
  const Row* standard = table.row(QStringLiteral("Standard"));
  CHECK(standard != nullptr);
  if (standard != nullptr) {
    // In the options' order, not the JSON object's.
    CHECK(standard->values.size() == 2 && standard->values.first().first == QStringLiteral("Color"));
    CHECK(standard->value(QStringLiteral("Length")) == QStringLiteral("1200 mm"));
    CHECK(standard->designation == QStringLiteral("Shelf"));
    CHECK(standard->partNumber == QStringLiteral("S-1"));
  }
  CHECK(table.rules().size() == 1);
  CHECK(table.nextRowName() == QStringLiteral("Row1"));

  // Written back: the rules, a field this application does not know and a
  // row's own extra field go back unchanged.
  Table edited = table;
  edited.options.append(QStringLiteral("Size"));
  edited.rows.first().setValue(QStringLiteral("Length"), QStringLiteral("800 mm"));
  edited.rows.first().setValue(QStringLiteral("Color"), QString()); // removed
  const QJsonObject json = edited.toJson();
  CHECK(json.value(QStringLiteral("options")).toArray().size() == 3);
  CHECK(json.value(QStringLiteral("rules")).toArray().first().toObject().value(QStringLiteral("name")).toString() ==
        QStringLiteral("Black is short"));
  CHECK(json.value(QStringLiteral("future")).toInt() == 1);
  const QJsonObject row = json.value(QStringLiteral("rows")).toArray().first().toObject();
  CHECK(row.value(QStringLiteral("note")).toString() == QStringLiteral("kept"));
  CHECK(row.value(QStringLiteral("values")).toObject().value(QStringLiteral("Length")).toString() ==
        QStringLiteral("800 mm"));
  CHECK(!row.value(QStringLiteral("values")).toObject().contains(QStringLiteral("Color")));
  CHECK(!row.contains(QStringLiteral("supplier_url")));

  const QJsonObject command = setTableCommand(edited);
  CHECK(command.value(QStringLiteral("cmd")).toString() == QStringLiteral("set_configurations"));
  CHECK(command.value(QStringLiteral("configurations")).isObject());
  // An empty table removes it.
  CHECK(setTableCommand(Table()).value(QStringLiteral("configurations")).isNull());
  // No table at all.
  CHECK(readTable(parse(R"({"configurations": null})")).isEmpty());
}

void reportTests() {
  using namespace mitcad::configurations;
  const Report report = readReport(parse(R"({
      "options": [
        {"name": "Color", "kind": "text", "unit": "Text", "value": "Black", "text": "Black",
         "expression": "'Black'", "driven": false,
         "domain": {"kind": "list", "values": [{"value": "Black", "text": "Black", "expression": "'Black'"},
                                              {"value": "White", "text": "White", "expression": "'White'"}]},
         "allowed": {"kind": "list", "values": [{"value": "Black", "text": "Black", "expression": "'Black'"},
                                               {"value": "White", "text": "White", "expression": "'White'"}]},
         "restricted_by": []},
        {"name": "Length", "kind": "number", "unit": "mm", "value": 1000, "text": "1000 mm",
         "expression": "1000 mm", "driven": false,
         "domain": {"kind": "range", "min": 300, "max": 2000, "step": 10,
                    "min_text": "300 mm", "max_text": "2000 mm", "step_text": "10 mm"},
         "allowed": {"kind": "range", "min": 300, "max": 1000, "step": 10,
                     "min_text": "300 mm", "max_text": "1000 mm", "step_text": "10 mm"},
         "restricted_by": ["Black is short"]},
        {"name": "Depth", "kind": "number", "unit": "mm", "value": 300, "text": "300 mm",
         "expression": "Length / 4", "driven": true, "domain": {"kind": "free"}, "allowed": {"kind": "free"}}],
      "row": null,
      "moves": [{"option": "Length", "from": "1500 mm", "to": "1000 mm", "rule": "Black is short",
                 "text": "Length 1500 mm → 1000 mm: Black is short"},
                {"option": "Power", "from": "1", "to": "0", "rule": "Power needs tables"}],
      "refused": null, "problems": []})"));
  CHECK(report.options.size() == 3);
  const OptionReport* color = report.option(QStringLiteral("Color"));
  const OptionReport* length = report.option(QStringLiteral("Length"));
  const OptionReport* depth = report.option(QStringLiteral("Depth"));
  CHECK(color != nullptr && length != nullptr && depth != nullptr);
  if (color == nullptr || length == nullptr || depth == nullptr) {
    return;
  }
  CHECK(color->text && !length->text);
  CHECK(color->allowed.kind == Domain::Kind::List && color->allowed.values.size() == 2);
  CHECK(color->allowed.summary() == QStringLiteral("Black, White"));
  CHECK(color->allowed.find(QStringLiteral("White")) != nullptr);
  CHECK(length->domain.summary() == QStringLiteral("300 mm … 2000 mm, step 10 mm"));
  CHECK(length->allowed.hasSlider() && *length->allowed.max == 1000);
  CHECK(length->restrictedBy == QStringList{QStringLiteral("Black is short")});
  CHECK(depth->driven && depth->allowed.kind == Domain::Kind::Free && !depth->allowed.hasSlider());
  CHECK(report.row.isEmpty());
  CHECK(!report.refused);
  CHECK(report.moves.size() == 2);
  CHECK(movesNotice(report.moves) ==
        QStringLiteral("Length 1500 mm → 1000 mm: Black is short; Power 1 → 0: Power needs tables"));

  // What the choosers send: a text without quotes, a bare number with the
  // option's unit, an expression as typed; a list value of a number by its
  // expression.
  CHECK(chooserValue(*color, QStringLiteral(" 'White' ")).toString() == QStringLiteral("White"));
  CHECK(chooserValue(*length, QStringLiteral("1200")).toString() == QStringLiteral("1200 mm"));
  CHECK(chooserValue(*length, QStringLiteral("2,5")).toString() == QStringLiteral("2.5 mm"));
  CHECK(chooserValue(*length, QStringLiteral("1 m")).toString() == QStringLiteral("1 m"));
  CHECK(chooserValue(*color, color->allowed.values.first()).toString() == QStringLiteral("Black"));
  const DomainValue listed{QJsonValue(500.0), QStringLiteral("500 mm"), QStringLiteral("500 mm")};
  CHECK(chooserValue(*length, listed).toString() == QStringLiteral("500 mm"));

  const QJsonObject command = configureCommand({{QStringLiteral("Color"), QStringLiteral("Black")}});
  CHECK(command.value(QStringLiteral("cmd")).toString() == QStringLiteral("configure"));
  CHECK(command.value(QStringLiteral("values")).toObject().value(QStringLiteral("Color")).toString() ==
        QStringLiteral("Black"));
  CHECK(!configureQuery().contains(QStringLiteral("values")));
  CHECK(applyCommand(QStringLiteral("Standard")).value(QStringLiteral("name")).toString() ==
        QStringLiteral("Standard"));
}

void rowValueTests() {
  using namespace mitcad::configurations;
  // The Rows sheet: typed values become expressions and back.
  CHECK(rowExpression(true, QStringLiteral("Text"), QStringLiteral("Black")) == QStringLiteral("'Black'"));
  CHECK(rowExpression(true, QStringLiteral("Text"), QStringLiteral("'Black'")) == QStringLiteral("'Black'"));
  CHECK(rowExpression(false, QStringLiteral("mm"), QStringLiteral("1500")) == QStringLiteral("1500 mm"));
  CHECK(rowExpression(false, QString(), QStringLiteral("1")) == QStringLiteral("1"));
  CHECK(rowExpression(false, QStringLiteral("mm"), QStringLiteral("  ")).isEmpty());
  CHECK(rowText(true, QStringLiteral("'Black'")) == QStringLiteral("Black"));
  CHECK(rowText(false, QStringLiteral("1500 mm")) == QStringLiteral("1500 mm"));

  // A row's values as the configure query takes them.
  Table table;
  table.options = {QStringLiteral("Color"), QStringLiteral("Length"), QStringLiteral("Power")};
  Row row;
  row.name = QStringLiteral("Short black");
  row.setValue(QStringLiteral("Color"), QStringLiteral("'Black'"));
  row.setValue(QStringLiteral("Length"), QStringLiteral("1500 mm"));
  table.rows.append(row);
  Report report;
  OptionReport color;
  color.name = QStringLiteral("Color");
  color.text = true;
  report.options.append(color);
  const QJsonObject choice = rowChoice(table, row, report);
  CHECK(choice.value(QStringLiteral("Color")).toString() == QStringLiteral("Black"));
  CHECK(choice.value(QStringLiteral("Length")).toString() == QStringLiteral("1500 mm"));
  CHECK(!choice.contains(QStringLiteral("Power"))); // the row leaves it out

  // A refusal's options: the cells the sheet marks.
  const std::optional<Refusal> refused = readRefusal(parse(R"({"refused": {"rule": "Black is short",
      "message": "", "text": "the rule Black is short does not allow Length = 1500 mm"}})")
                                                         .value(QStringLiteral("refused")));
  CHECK(refused.has_value());
  if (refused) {
    CHECK(refused->describe() == QStringLiteral("the rule Black is short does not allow Length = 1500 mm"));
    CHECK(optionsNamed(table.options, *refused) == QStringList{QStringLiteral("Length")});
  }
  const Refusal plain{QStringLiteral("No red XL"), QStringLiteral("Red is not made in XL"), QString()};
  CHECK(plain.describe() == QStringLiteral("the rule No red XL: Red is not made in XL"));
  CHECK(optionsNamed(table.options, plain).isEmpty());
  CHECK(!readRefusal(QJsonValue()).has_value());
}

// Configurations and tables (mitcad#849): Generate Rows from Table's
// command, answer and first option, and an option's values from a column.
void tableColumnTests() {
  using namespace mitcad::configurations;
  const QJsonObject command = generateRowsCommand(QStringLiteral("Screws"), QStringLiteral("Size"), QStringLiteral("Size"));
  CHECK(command.value(QStringLiteral("cmd")).toString() == QStringLiteral("generate_configuration_rows"));
  CHECK(command.value(QStringLiteral("table")).toString() == QStringLiteral("Screws"));
  CHECK(command.value(QStringLiteral("column")).toString() == QStringLiteral("Size"));
  CHECK(command.value(QStringLiteral("option")).toString() == QStringLiteral("Size"));

  const GeneratedRows first = readGeneratedRows(parse(R"({"rows": ["M3", "M4", "M5"], "added": ["M3", "M4", "M5"],
      "updated": [], "moves": []})"));
  CHECK(first.rows.size() == 3 && first.added.size() == 3 && first.updated.isEmpty() && first.moves.isEmpty());
  CHECK(first.describe(QStringLiteral("Screws")) == QStringLiteral("3 rows from Screws: added M3, M4, M5"));
  const GeneratedRows again = readGeneratedRows(parse(R"({"rows": ["M3", "M4", "M5", "M6"], "added": ["M6"],
      "updated": ["M4"], "moves": [{"option": "Size", "from": "M7", "to": "M6", "rule": "",
      "text": "Size M7 → M6"}]})"));
  CHECK(again.describe(QStringLiteral("Screws")) == QStringLiteral("4 rows from Screws: added M6; updated M4"));
  CHECK(again.moves.size() == 1 && again.moves.first().option == QStringLiteral("Size"));
  CHECK(readGeneratedRows(parse(R"({"rows": ["M3"], "added": [], "updated": []})")).describe(QStringLiteral("S")) ==
        QStringLiteral("1 row from S: all up to date"));

  // A column as an option's values: the `parameters` query's limit as a
  // text (or a list of that one reference); a list is no column.
  const QJsonObject size = parse(R"({"name": "Size", "unit": "Text", "limits": {"values": "Screws.Size"}})");
  const QJsonObject listed = parse(R"({"name": "Color", "limits": {"values": ["'Black'", "'White'"]}})");
  const QJsonObject one = parse(R"({"name": "Pitch", "limits": {"values": ["Screws.Pitch"]}})");
  const QJsonObject single = parse(R"({"name": "Finish", "limits": {"values": ["'Black'"]}})");
  CHECK(valuesColumn(size) == QStringLiteral("Screws.Size"));
  CHECK(valuesColumn(listed).isEmpty());
  CHECK(valuesColumn(one) == QStringLiteral("Screws.Pitch"));
  CHECK(valuesColumn(single).isEmpty());
  CHECK(valuesColumn(QJsonObject()).isEmpty());
  CHECK(columnReference(QStringLiteral("Screws"), QStringLiteral("Size")) == QStringLiteral("Screws.Size"));

  // The option offered first: the one whose values are the column, else
  // the option or parameter of its name, else the first option.
  QHash<QString, QJsonObject> parameters;
  parameters.insert(QStringLiteral("Key"), parse(R"({"name": "Key", "limits": {"values": "Screws.Size"}})"));
  parameters.insert(QStringLiteral("Length"), parse(R"({"name": "Length", "limits": {}})"));
  parameters.insert(QStringLiteral("Pitch"), parse(R"({"name": "Pitch", "limits": {}})"));
  const QStringList options{QStringLiteral("Length"), QStringLiteral("Key")};
  const QStringList users{QStringLiteral("Length"), QStringLiteral("Key"), QStringLiteral("Pitch")};
  CHECK(keyOption(QStringLiteral("Screws"), QStringLiteral("Size"), options, users, parameters) == QStringLiteral("Key"));
  CHECK(keyOption(QStringLiteral("Lengths"), QStringLiteral("Length"), options, users, parameters) ==
        QStringLiteral("Length"));
  CHECK(keyOption(QStringLiteral("Screws"), QStringLiteral("Pitch"), options, users, parameters) ==
        QStringLiteral("Pitch"));
  CHECK(keyOption(QStringLiteral("Screws"), QStringLiteral("dk"), options, users, parameters) ==
        QStringLiteral("Length"));
  CHECK(keyOption(QStringLiteral("Screws"), QStringLiteral("dk"), {}, users, parameters).isEmpty());
}

// A closed table's parameter columns (mitcad#999): Add Column puts a user
// parameter among `parameters` with each row's current value, Remove
// Column takes it out of the table and keeps the parameter.
void parameterColumnTests() {
  using namespace mitcad::configurations;
  const Table screws = readTable(parse(R"({"configurations": {
      "options": ["Size"], "closed": true, "parameters": ["d"],
      "rows": [{"name": "M3", "values": {"Size": "'M3'", "d": "3 mm"}},
               {"name": "M4", "values": {"Size": "'M4'", "d": "4 mm"}}],
      "default": "M4"}})"));
  const QStringList users{QStringLiteral("Size"), QStringLiteral("d"), QStringLiteral("P"), QStringLiteral("k")};
  // Neither an option nor a column already, in the model's order.
  CHECK(columnCandidates(screws, users) == QStringList({QStringLiteral("P"), QStringLiteral("k")}));

  const Table added = withColumn(screws, QStringLiteral("P"), QStringLiteral("0.5 mm"));
  CHECK(added.closed);
  CHECK(added.parameters == QStringList({QStringLiteral("d"), QStringLiteral("P")}));
  CHECK(added.rows.size() == 2);
  for (const Row& row : added.rows) {
    CHECK(row.value(QStringLiteral("P")) == QStringLiteral("0.5 mm"));
    CHECK(!row.value(QStringLiteral("d")).isEmpty()); // the other column's values stay
  }
  CHECK(added.defaultRow == QStringLiteral("M4"));
  // As set_configurations takes it: in `parameters` and in every row's values.
  const QJsonObject json = setTableCommand(added).value(QStringLiteral("configurations")).toObject();
  CHECK(json.value(QStringLiteral("parameters")).toArray().size() == 2);
  CHECK(json.value(QStringLiteral("parameters")).toArray().last().toString() == QStringLiteral("P"));
  CHECK(json.value(QStringLiteral("closed")).toBool());
  for (const QJsonValue& row : json.value(QStringLiteral("rows")).toArray()) {
    CHECK(row.toObject().value(QStringLiteral("values")).toObject().value(QStringLiteral("P")).toString() ==
          QStringLiteral("0.5 mm"));
  }
  CHECK(columnCandidates(added, users) == QStringList{QStringLiteral("k")});
  // An option or a column already: unchanged.
  CHECK(withColumn(screws, QStringLiteral("Size"), QStringLiteral("'M5'")).toJson() == screws.toJson());
  CHECK(withColumn(screws, QStringLiteral("d"), QStringLiteral("5 mm")).toJson() == screws.toJson());

  // An open table becomes closed with its first column.
  Table open = screws;
  open.closed = false;
  open.parameters.clear();
  for (Row& row : open.rows) {
    row.setValue(QStringLiteral("d"), QString());
  }
  const Table closed = withColumn(open, QStringLiteral("k"), QStringLiteral("d * 0.6"));
  CHECK(closed.closed);
  CHECK(closed.parameters == QStringList{QStringLiteral("k")});
  CHECK(closed.rows.first().value(QStringLiteral("k")) == QStringLiteral("d * 0.6"));

  // Remove Column: out of `parameters` and the rows; the options' values,
  // the other columns and the parameter itself stay (nothing but the table
  // is sent, and Add Column offers it again).
  const Table removed = withoutColumn(added, QStringLiteral("d"));
  CHECK(removed.parameters == QStringList{QStringLiteral("P")});
  CHECK(removed.closed);
  for (const Row& row : removed.rows) {
    CHECK(row.value(QStringLiteral("d")).isEmpty());
    CHECK(row.value(QStringLiteral("P")) == QStringLiteral("0.5 mm"));
    CHECK(!row.value(QStringLiteral("Size")).isEmpty());
  }
  const QJsonObject command = setTableCommand(removed);
  CHECK(command.value(QStringLiteral("cmd")).toString() == QStringLiteral("set_configurations"));
  const QJsonObject removedJson = command.value(QStringLiteral("configurations")).toObject();
  for (const QJsonValue& row : removedJson.value(QStringLiteral("rows")).toArray()) {
    CHECK(!row.toObject().value(QStringLiteral("values")).toObject().contains(QStringLiteral("d")));
  }
  CHECK(columnCandidates(removed, users) == QStringList({QStringLiteral("d"), QStringLiteral("k")}));
  // The last column out: no `parameters` field at all.
  CHECK(!withoutColumn(removed, QStringLiteral("P")).toJson().contains(QStringLiteral("parameters")));
  // No such column: unchanged.
  CHECK(withoutColumn(screws, QStringLiteral("Size")).toJson() == screws.toJson());
}

} // namespace

int configurationsDataTests() {
  failures = 0;
  tableTests();
  reportTests();
  rowValueTests();
  tableColumnTests();
  parameterColumnTests();
  if (failures == 0) {
    std::printf("configurations data tests passed\n");
  }
  return failures;
}
