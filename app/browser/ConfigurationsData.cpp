// SPDX-License-Identifier: MIT
#include "ConfigurationsData.hpp"

#include <algorithm>
#include <utility>

#include <QRegularExpression>
#include <QSet>

#include "../framework/Numbers.hpp"

namespace mitcad::configurations {
namespace {

QString text(const QJsonObject& object, const char* key) { return object.value(QLatin1String(key)).toString(); }

QStringList strings(const QJsonValue& value) {
  QStringList out;
  for (const QJsonValue& item : value.toArray()) {
    out << item.toString();
  }
  return out;
}

std::optional<double> number(const QJsonObject& object, const char* key) {
  const QJsonValue value = object.value(QLatin1String(key));
  if (value.isDouble()) {
    return value.toDouble();
  }
  return std::nullopt;
}

bool quoted(const QString& text) {
  return text.size() >= 2 && text.startsWith(QLatin1Char('\'')) && text.endsWith(QLatin1Char('\''));
}

// A bare number gets the unit ("800" in a millimetre option is "800 mm");
// anything else is kept as typed.
QString withUnit(const QString& typed, const QString& unit) {
  QString normalized;
  if (!parsePlainNumber(typed, &normalized)) {
    return typed;
  }
  return unit.isEmpty() ? normalized : normalized + QLatin1Char(' ') + unit;
}

const QStringList kTableKeys = {QStringLiteral("options"), QStringLiteral("closed"), QStringLiteral("parameters"),
                                QStringLiteral("default"), QStringLiteral("rows")};
const QStringList kRowKeys = {QStringLiteral("name"), QStringLiteral("values"), QStringLiteral("designation"),
                              QStringLiteral("part_number"), QStringLiteral("supplier_url")};

} // namespace

QString Domain::summary() const {
  switch (kind) {
  case Kind::List: {
    QStringList texts;
    for (const DomainValue& value : values) {
      texts << value.text;
    }
    return texts.isEmpty() ? QStringLiteral("no value") : texts.join(QStringLiteral(", "));
  }
  case Kind::Range: {
    QString range;
    if (min && max) {
      range = QStringLiteral("%1 … %2").arg(minText, maxText);
    } else if (min) {
      range = QStringLiteral("from %1").arg(minText);
    } else if (max) {
      range = QStringLiteral("up to %1").arg(maxText);
    }
    if (step) {
      range = range.isEmpty() ? QStringLiteral("step %1").arg(stepText)
                              : QStringLiteral("%1, step %2").arg(range, stepText);
    }
    return range.isEmpty() ? QStringLiteral("free") : range;
  }
  case Kind::Free:
    break;
  }
  return QStringLiteral("free");
}

const DomainValue* Domain::find(const QString& shown) const {
  for (const DomainValue& value : values) {
    if (value.text == shown) {
      return &value;
    }
  }
  return nullptr;
}

QString Move::describe() const {
  if (!text.isEmpty()) {
    return text;
  }
  return rule.isEmpty() ? QStringLiteral("%1 %2 → %3").arg(option, from, to)
                        : QStringLiteral("%1 %2 → %3: %4").arg(option, from, to, rule);
}

QString Refusal::describe() const {
  if (!text.isEmpty()) {
    return text;
  }
  if (rule.isEmpty()) {
    return message;
  }
  return message.isEmpty() ? QStringLiteral("the rule %1").arg(rule)
                           : QStringLiteral("the rule %1: %2").arg(rule, message);
}

const OptionReport* Report::option(const QString& name) const {
  for (const OptionReport& report : options) {
    if (report.name == name) {
      return &report;
    }
  }
  return nullptr;
}

QString Row::value(const QString& parameter) const {
  for (const auto& entry : values) {
    if (entry.first == parameter) {
      return entry.second;
    }
  }
  return {};
}

void Row::setValue(const QString& parameter, const QString& expression) {
  for (auto it = values.begin(); it != values.end(); ++it) {
    if (it->first == parameter) {
      if (expression.isEmpty()) {
        values.erase(it);
      } else {
        it->second = expression;
      }
      return;
    }
  }
  if (!expression.isEmpty()) {
    values.append({parameter, expression});
  }
}

const Row* Table::row(const QString& name) const {
  for (const Row& candidate : rows) {
    if (candidate.name == name) {
      return &candidate;
    }
  }
  return nullptr;
}

Row* Table::row(const QString& name) {
  for (Row& candidate : rows) {
    if (candidate.name == name) {
      return &candidate;
    }
  }
  return nullptr;
}

QString Table::nextRowName() const {
  for (int i = 1;; ++i) {
    const QString name = QStringLiteral("Row%1").arg(i);
    if (row(name) == nullptr) {
      return name;
    }
  }
}

QJsonObject Table::toJson() const {
  QJsonObject out = other;
  out.insert(QStringLiteral("options"), QJsonArray::fromStringList(options));
  out.insert(QStringLiteral("closed"), closed);
  if (!parameters.isEmpty()) {
    out.insert(QStringLiteral("parameters"), QJsonArray::fromStringList(parameters));
  }
  if (!defaultRow.isEmpty()) {
    out.insert(QStringLiteral("default"), defaultRow);
  }
  QJsonArray rowArray;
  for (const Row& item : rows) {
    QJsonObject json = item.other;
    json.insert(QStringLiteral("name"), item.name);
    QJsonObject values;
    for (const auto& entry : item.values) {
      values.insert(entry.first, entry.second);
    }
    json.insert(QStringLiteral("values"), values);
    if (!item.designation.isEmpty()) {
      json.insert(QStringLiteral("designation"), item.designation);
    }
    if (!item.partNumber.isEmpty()) {
      json.insert(QStringLiteral("part_number"), item.partNumber);
    }
    if (!item.supplierUrl.isEmpty()) {
      json.insert(QStringLiteral("supplier_url"), item.supplierUrl);
    }
    rowArray.append(json);
  }
  out.insert(QStringLiteral("rows"), rowArray);
  return out;
}

Table readTable(const QJsonObject& configurationsQuery) {
  Table table;
  const QJsonObject json = configurationsQuery.value(QStringLiteral("configurations")).toObject();
  if (json.isEmpty()) {
    return table;
  }
  table.options = strings(json.value(QStringLiteral("options")));
  table.closed = json.value(QStringLiteral("closed")).toBool();
  table.parameters = strings(json.value(QStringLiteral("parameters")));
  table.defaultRow = text(json, "default");
  for (auto it = json.begin(); it != json.end(); ++it) {
    if (!kTableKeys.contains(it.key())) {
      table.other.insert(it.key(), it.value());
    }
  }
  for (const QJsonValue& value : json.value(QStringLiteral("rows")).toArray()) {
    const QJsonObject rowJson = value.toObject();
    Row row;
    row.name = text(rowJson, "name");
    row.designation = text(rowJson, "designation");
    row.partNumber = text(rowJson, "part_number");
    row.supplierUrl = text(rowJson, "supplier_url");
    for (auto it = rowJson.begin(); it != rowJson.end(); ++it) {
      if (!kRowKeys.contains(it.key())) {
        row.other.insert(it.key(), it.value());
      }
    }
    // The values in the table's order: options, then parameters, then any
    // other (a problem the model reports).
    const QJsonObject values = rowJson.value(QStringLiteral("values")).toObject();
    QSet<QString> taken;
    const QStringList keys = values.keys();
    const QStringList* orders[] = {&table.options, &table.parameters, &keys};
    for (const QStringList* order : orders) {
      for (const QString& name : *order) {
        if (values.contains(name) && !taken.contains(name)) {
          taken.insert(name);
          const QJsonValue expression = values.value(name);
          row.values.append(
              {name, expression.isString() ? expression.toString() : QString::number(expression.toDouble())});
        }
      }
    }
    table.rows.append(row);
  }
  return table;
}

Domain readDomain(const QJsonValue& value) {
  Domain domain;
  const QJsonObject json = value.toObject();
  const QString kind = text(json, "kind");
  if (kind == QStringLiteral("list")) {
    domain.kind = Domain::Kind::List;
    for (const QJsonValue& item : json.value(QStringLiteral("values")).toArray()) {
      const QJsonObject entry = item.toObject();
      domain.values.append({entry.value(QStringLiteral("value")), text(entry, "text"), text(entry, "expression")});
    }
  } else if (kind == QStringLiteral("range")) {
    domain.kind = Domain::Kind::Range;
    domain.min = number(json, "min");
    domain.max = number(json, "max");
    domain.step = number(json, "step");
    domain.minText = text(json, "min_text");
    domain.maxText = text(json, "max_text");
    domain.stepText = text(json, "step_text");
  }
  return domain;
}

QVector<Move> readMoves(const QJsonValue& value) {
  QVector<Move> moves;
  for (const QJsonValue& item : value.toArray()) {
    const QJsonObject json = item.toObject();
    moves.append({text(json, "option"), text(json, "from"), text(json, "to"), text(json, "rule"), text(json, "text")});
  }
  return moves;
}

std::optional<Refusal> readRefusal(const QJsonValue& value) {
  if (value.isString() && !value.toString().isEmpty()) {
    return Refusal{QString(), QString(), value.toString()};
  }
  if (!value.isObject()) {
    return std::nullopt;
  }
  const QJsonObject json = value.toObject();
  Refusal refusal{text(json, "rule"), text(json, "message"), text(json, "text")};
  if (refusal.rule.isEmpty() && refusal.message.isEmpty() && refusal.text.isEmpty()) {
    return std::nullopt;
  }
  return refusal;
}

Report readReport(const QJsonObject& query) {
  Report report;
  for (const QJsonValue& value : query.value(QStringLiteral("options")).toArray()) {
    const QJsonObject json = value.toObject();
    OptionReport option;
    option.name = text(json, "name");
    option.text = text(json, "kind") == QStringLiteral("text");
    option.unit = text(json, "unit");
    option.value = json.value(QStringLiteral("value"));
    option.valueText = text(json, "text");
    option.expression = text(json, "expression");
    option.driven = json.value(QStringLiteral("driven")).toBool();
    option.domain = readDomain(json.value(QStringLiteral("domain")));
    option.allowed = readDomain(json.value(QStringLiteral("allowed")));
    option.restrictedBy = strings(json.value(QStringLiteral("restricted_by")));
    report.options.append(option);
  }
  report.row = query.value(QStringLiteral("row")).toString();
  report.moves = readMoves(query.value(QStringLiteral("moves")));
  report.refused = readRefusal(query.value(QStringLiteral("refused")));
  report.problems = strings(query.value(QStringLiteral("problems")));
  return report;
}

QJsonObject configurationsQuery() { return {{QStringLiteral("query"), QStringLiteral("configurations")}}; }

QJsonObject configureQuery(const QJsonObject& values) {
  QJsonObject query{{QStringLiteral("query"), QStringLiteral("configure")}};
  if (!values.isEmpty()) {
    query.insert(QStringLiteral("values"), values);
  }
  return query;
}

QJsonObject setTableCommand(const Table& table) {
  return {{QStringLiteral("cmd"), QStringLiteral("set_configurations")},
          {QStringLiteral("configurations"), table.isEmpty() ? QJsonValue() : QJsonValue(table.toJson())}};
}

QJsonObject configureCommand(const QJsonObject& values) {
  return {{QStringLiteral("cmd"), QStringLiteral("configure")}, {QStringLiteral("values"), values}};
}

QJsonObject applyCommand(const QString& row) {
  return {{QStringLiteral("cmd"), QStringLiteral("apply_configuration")}, {QStringLiteral("name"), row}};
}

QJsonValue chooserValue(const OptionReport& option, const QString& typed) {
  const QString trimmed = typed.trimmed();
  if (option.text) {
    return quoted(trimmed) ? trimmed.mid(1, trimmed.size() - 2) : trimmed;
  }
  return withUnit(trimmed, option.unit);
}

QJsonValue chooserValue(const OptionReport& option, const DomainValue& value) {
  if (option.text) {
    return value.value.isString() ? value.value : QJsonValue(value.text);
  }
  return value.expression.isEmpty() ? value.value : QJsonValue(value.expression);
}

QString rowExpression(bool isText, const QString& unit, const QString& typed) {
  const QString trimmed = typed.trimmed();
  if (trimmed.isEmpty()) {
    return trimmed;
  }
  if (isText) {
    return quoted(trimmed) ? trimmed : QStringLiteral("'%1'").arg(trimmed);
  }
  return withUnit(trimmed, unit);
}

QString rowText(bool isText, const QString& expression) {
  const QString trimmed = expression.trimmed();
  return isText && quoted(trimmed) ? trimmed.mid(1, trimmed.size() - 2) : expression;
}

QJsonObject rowChoice(const Table& table, const Row& row, const Report& report) {
  QJsonObject values;
  for (const QString& option : table.options) {
    const QString expression = row.value(option);
    if (expression.isEmpty()) {
      continue;
    }
    const OptionReport* reported = report.option(option);
    values.insert(option, reported != nullptr && reported->text ? rowText(true, expression) : expression);
  }
  return values;
}

QString movesNotice(const QVector<Move>& moves) {
  QStringList lines;
  for (const Move& move : moves) {
    lines << move.describe();
  }
  return lines.join(QStringLiteral("; "));
}

QStringList optionsNamed(const QStringList& options, const Refusal& refusal) {
  const QString haystack = QStringList{refusal.text, refusal.message}.join(QLatin1Char(' '));
  QStringList named;
  for (const QString& option : options) {
    const QRegularExpression word(QStringLiteral("(^|[^\\w])%1($|[^\\w])").arg(QRegularExpression::escape(option)));
    if (haystack.contains(word)) {
      named << option;
    }
  }
  return named;
}

QString GeneratedRows::describe(const QString& table) const {
  const QString count = rows.size() == 1 ? QStringLiteral("1 row") : QStringLiteral("%1 rows").arg(rows.size());
  QStringList parts;
  if (!added.isEmpty()) {
    parts << QStringLiteral("added %1").arg(added.join(QStringLiteral(", ")));
  }
  if (!updated.isEmpty()) {
    parts << QStringLiteral("updated %1").arg(updated.join(QStringLiteral(", ")));
  }
  if (parts.isEmpty()) {
    parts << QStringLiteral("all up to date");
  }
  if (!created.isEmpty()) {
    parts << QStringLiteral("made %1").arg(created.join(QStringLiteral(", ")));
  }
  return QStringLiteral("%1 from %2: %3").arg(count, table, parts.join(QStringLiteral("; ")));
}

QJsonObject generateRowsCommand(const QString& table, const QString& column, const QString& option) {
  return {{QStringLiteral("cmd"), QStringLiteral("generate_configuration_rows")},
          {QStringLiteral("table"), table},
          {QStringLiteral("column"), column},
          {QStringLiteral("option"), option}};
}

GeneratedRows readGeneratedRows(const QJsonObject& answer) {
  GeneratedRows generated;
  generated.rows = strings(answer.value(QStringLiteral("rows")));
  generated.added = strings(answer.value(QStringLiteral("added")));
  generated.updated = strings(answer.value(QStringLiteral("updated")));
  generated.created = strings(answer.value(QStringLiteral("created")));
  generated.moves = readMoves(answer.value(QStringLiteral("moves")));
  return generated;
}

QString valuesColumn(const QJsonObject& parameter) {
  const QJsonValue values = parameter.value(QStringLiteral("limits")).toObject().value(QStringLiteral("values"));
  if (values.isString()) {
    return values.toString();
  }
  // A list of that one entry is read as the column too (commands.md).
  const QJsonArray list = values.toArray();
  if (list.size() == 1 && list.at(0).isString()) {
    const QString only = list.at(0).toString().trimmed();
    static const QRegularExpression reference(QStringLiteral("^[A-Za-z_]\\w*\\.[A-Za-z_]\\w*$"));
    if (reference.match(only).hasMatch()) {
      return only;
    }
  }
  return {};
}

QString columnReference(const QString& table, const QString& column) {
  return QStringLiteral("%1.%2").arg(table, column);
}

QString keyOption(const QString& table, const QString& column, const QStringList& options,
                  const QStringList& userParameters, const QHash<QString, QJsonObject>& parameters) {
  const QString reference = columnReference(table, column);
  for (const QString& option : options) {
    if (valuesColumn(parameters.value(option)) == reference) {
      return option;
    }
  }
  if (options.contains(column)) {
    return column;
  }
  if (userParameters.contains(column)) {
    return column;
  }
  return options.isEmpty() ? QString() : options.first();
}

QStringList columnCandidates(const Table& table, const QStringList& userParameters) {
  QStringList out;
  for (const QString& name : userParameters) {
    if (!table.options.contains(name) && !table.parameters.contains(name)) {
      out << name;
    }
  }
  return out;
}

Table withColumn(const Table& table, const QString& parameter, const QString& expression) {
  if (parameter.isEmpty() || table.options.contains(parameter) || table.parameters.contains(parameter)) {
    return table;
  }
  Table out = table;
  out.closed = true; // an open table's rows set options only
  out.parameters.append(parameter);
  for (Row& row : out.rows) {
    row.setValue(parameter, expression);
  }
  return out;
}

Table withoutColumn(const Table& table, const QString& parameter) {
  if (!table.parameters.contains(parameter)) {
    return table;
  }
  Table out = table;
  out.parameters.removeAll(parameter);
  for (Row& row : out.rows) {
    row.setValue(parameter, QString());
  }
  return out;
}

QStringList optionCandidates(const Table& table, const QStringList& userParameters) {
  QStringList out;
  for (const QString& name : userParameters) {
    if (!table.options.contains(name)) {
      out << name;
    }
  }
  return out;
}

QString currentConstant(const QJsonObject& parameter) {
  const QString expression = text(parameter, "expression").trimmed();
  const bool alone = parameter.value(QStringLiteral("dependencies")).toArray().isEmpty() &&
                     parameter.value(QStringLiteral("table_dependencies")).toArray().isEmpty();
  if (alone && !expression.isEmpty()) {
    return expression;
  }
  const bool isText = text(parameter, "unit").trimmed().compare(QStringLiteral("Text"), Qt::CaseInsensitive) == 0;
  return rowExpression(isText, QString(), text(parameter, "text"));
}

Table withOption(const Table& table, const QString& parameter, const QString& expression) {
  if (parameter.isEmpty() || table.options.contains(parameter)) {
    return table;
  }
  Table out = table;
  out.parameters.removeAll(parameter); // a parameter column keeps its rows' values
  out.options.append(parameter);
  for (Row& row : out.rows) {
    if (row.value(parameter).trimmed().isEmpty()) {
      row.setValue(parameter, expression);
    }
  }
  return out;
}

QJsonObject generateRowsCommand(const QString& table, const QString& column, const QString& option,
                                const QStringList& columns) {
  QJsonObject command = generateRowsCommand(table, column, option);
  if (!columns.isEmpty()) {
    command.insert(QStringLiteral("columns"), QJsonArray::fromStringList(columns));
  }
  return command;
}

QJsonObject rowsCsvQuery() { return {{QStringLiteral("query"), QStringLiteral("configuration_rows_csv")}}; }

QJsonObject rowsCsvPreviewQuery(const QString& csv) {
  return {{QStringLiteral("query"), QStringLiteral("configuration_rows_csv_preview")}, {QStringLiteral("csv"), csv}};
}

QJsonObject importRowsCsvCommand(const QString& csv) {
  return {{QStringLiteral("cmd"), QStringLiteral("import_configuration_rows_csv")}, {QStringLiteral("csv"), csv}};
}

QVector<QStringList> readBlock(const QString& text) {
  QVector<QStringList> block;
  QStringList row;
  QString cell;
  bool quoted = false; // inside a quoted cell
  bool start = true;   // at the start of a cell
  const qsizetype size = text.size();
  for (qsizetype i = 0; i < size; ++i) {
    const QChar c = text.at(i);
    if (quoted) {
      if (c == QLatin1Char('"')) {
        if (i + 1 < size && text.at(i + 1) == QLatin1Char('"')) {
          cell += QLatin1Char('"');
          ++i;
        } else {
          quoted = false;
        }
      } else {
        cell += c;
      }
      continue;
    }
    if (start && c == QLatin1Char('"')) {
      quoted = true;
      start = false;
      continue;
    }
    start = false;
    if (c == QLatin1Char('\t')) {
      row << cell;
      cell.clear();
      start = true;
    } else if (c == QLatin1Char('\r') || c == QLatin1Char('\n')) {
      if (c == QLatin1Char('\r') && i + 1 < size && text.at(i + 1) == QLatin1Char('\n')) {
        ++i;
      }
      row << cell;
      block.append(row);
      row.clear();
      cell.clear();
      start = true;
    } else {
      cell += c;
    }
  }
  if (!start || !cell.isEmpty() || !row.isEmpty()) {
    row << cell;
    block.append(row);
  }
  return block;
}

QString writeBlock(const QVector<QStringList>& block) {
  QStringList lines;
  for (const QStringList& row : block) {
    QStringList cells;
    for (const QString& cell : row) {
      if (cell.contains(QLatin1Char('\t')) || cell.contains(QLatin1Char('\n')) || cell.contains(QLatin1Char('\r')) ||
          cell.startsWith(QLatin1Char('"'))) {
        QString escaped = cell;
        escaped.replace(QStringLiteral("\""), QStringLiteral("\"\""));
        cells << QStringLiteral("\"%1\"").arg(escaped);
      } else {
        cells << cell;
      }
    }
    lines << cells.join(QLatin1Char('\t'));
  }
  return lines.isEmpty() ? QString() : lines.join(QLatin1Char('\n')) + QLatin1Char('\n');
}

namespace {

// A cell typed into a row (pasted, or emptied by Delete: `typed` empty).
void setCell(Table& table, Row& row, const SheetColumn& column, const QString& typed) {
  switch (column.kind) {
  case SheetColumn::Kind::Name:
    if (!typed.isEmpty() && typed != row.name) {
      if (table.defaultRow == row.name) {
        table.defaultRow = typed;
      }
      row.name = typed;
    }
    break;
  case SheetColumn::Kind::Option:
  case SheetColumn::Kind::Parameter:
    row.setValue(column.parameter, rowExpression(column.text, column.unit, typed));
    break;
  case SheetColumn::Kind::Designation:
    row.designation = typed;
    break;
  case SheetColumn::Kind::PartNumber:
    row.partNumber = typed;
    break;
  case SheetColumn::Kind::SupplierUrl:
    row.supplierUrl = typed;
    break;
  }
}

} // namespace

Paste pasteBlock(const Table& table, const QVector<SheetColumn>& columns, int row, int column,
                 const QVector<QStringList>& block, const Row& fresh) {
  Paste out;
  out.table = table;
  const qsizetype top = std::max(0, row);
  const qsizetype left = std::max(0, column);
  QVector<QPair<qsizetype, int>> pasted;
  for (qsizetype i = 0; i < block.size(); ++i) {
    const qsizetype r = top + i;
    while (out.table.rows.size() <= r) {
      Row added = fresh;
      added.name = out.table.nextRowName();
      out.added << added.name;
      out.table.rows.append(added);
    }
    const QStringList& cells = block.at(i);
    for (qsizetype j = 0; j < cells.size(); ++j) {
      const qsizetype c = left + j;
      if (c >= columns.size()) {
        ++out.beyond;
        continue;
      }
      const QString before = out.table.rows.at(r).name;
      setCell(out.table, out.table.rows[r], columns.at(c), cells.at(j).trimmed());
      const QString after = out.table.rows.at(r).name;
      if (after != before) {
        for (QString& name : out.added) {
          if (name == before) {
            name = after;
          }
        }
      }
      pasted.append(qMakePair(r, static_cast<int>(c)));
    }
  }
  for (const auto& cell : std::as_const(pasted)) {
    out.cells.append(qMakePair(out.table.rows.at(cell.first).name, cell.second));
  }
  return out;
}

Table clearCells(const Table& table, const QVector<SheetColumn>& columns, const QVector<QPair<int, int>>& cells) {
  Table out = table;
  for (const auto& cell : cells) {
    if (cell.first < 0 || cell.first >= out.rows.size() || cell.second < 0 || cell.second >= columns.size()) {
      continue;
    }
    const SheetColumn& column = columns.at(cell.second);
    if (column.kind != SheetColumn::Kind::Name) {
      setCell(out, out.rows[cell.first], column, QString());
    }
  }
  return out;
}

ValueCell valueCell(const OptionReport* report, bool text, const QString& expression) {
  ValueCell cell;
  if (report != nullptr && report->driven) {
    // By Formula: the evaluated value; the formula only in the tooltip.
    cell.text = report->valueText;
    cell.expression = report->expression.isEmpty() ? expression : report->expression;
    cell.editable = false;
    return cell;
  }
  cell.text = report != nullptr && !report->valueText.isEmpty() ? report->valueText : rowText(text, expression);
  return cell;
}

} // namespace mitcad::configurations
