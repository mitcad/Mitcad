// SPDX-License-Identifier: MIT
#include "PartOptions.hpp"

#include <algorithm>
#include <cmath>

#include <QJsonArray>
#include <QJsonValue>
#include <QObject>
#include <QRegularExpression>

namespace mitcad {
namespace {

constexpr double kPi = 3.14159265358979323846;

QString textOf(const QJsonObject& object, const char* key) { return object.value(QLatin1String(key)).toString(); }

double numberOf(const QJsonObject& object, const char* key) {
  const QJsonValue value = object.value(QLatin1String(key));
  return value.isDouble() ? value.toDouble() : std::numeric_limits<double>::quiet_NaN();
}

QStringList textsOf(const QJsonValue& list) {
  QStringList out;
  for (const QJsonValue& value : list.toArray()) {
    if (value.isString()) {
      out << value.toString();
    } else if (value.isObject()) {
      // A problem or a name given as an object: its words.
      const QJsonObject object = value.toObject();
      const QString text = textOf(object, "text").isEmpty() ? textOf(object, "message") : textOf(object, "text");
      out << (text.isEmpty() ? textOf(object, "name") : text);
    }
  }
  return out;
}

// A value of the answer as text: the text given, else the number in the unit.
QString shownValue(const QJsonValue& value, const QString& text, const QString& unit) {
  if (!text.isEmpty()) {
    return text;
  }
  if (value.isString()) {
    return value.toString();
  }
  if (value.isDouble()) {
    return numberText(value.toDouble(), unit);
  }
  return QString();
}

PartOption optionOf(const QJsonObject& object) {
  PartOption option;
  option.name = textOf(object, "name");
  option.text = textOf(object, "kind") == QStringLiteral("text");
  option.unit = textOf(object, "unit");
  option.expression = textOf(object, "expression");
  const QJsonValue value = object.value(QStringLiteral("value"));
  if (value.isDouble()) {
    option.value = value.toDouble();
  }
  option.valueText = shownValue(value, textOf(object, "text"), option.unit);
  if (option.valueText.isEmpty()) {
    option.valueText = valueOfExpression(option.expression);
  }
  option.parameter = textOf(object, "parameter");
  option.bound = object.value(QStringLiteral("bound")).toBool();
  // A value bound to the parent is not chosen here either.
  option.driven = object.value(QStringLiteral("driven")).toBool() || option.bound;
  option.restrictedBy = textsOf(object.value(QStringLiteral("restricted_by")));
  // What the rules allow; the domain when the answer has no `allowed`.
  QJsonObject allowed = object.value(QStringLiteral("allowed")).toObject();
  if (allowed.isEmpty()) {
    allowed = object.value(QStringLiteral("domain")).toObject();
  }
  const QString kind = textOf(allowed, "kind");
  if (kind == QStringLiteral("list")) {
    option.kind = PartOption::Kind::List;
    for (const QJsonValue& entry : allowed.value(QStringLiteral("values")).toArray()) {
      OptionValueChoice choice;
      if (entry.isObject()) {
        const QJsonObject one = entry.toObject();
        choice.expression = textOf(one, "expression");
        const QJsonValue v = one.value(QStringLiteral("value"));
        if (v.isDouble()) {
          choice.value = v.toDouble();
        }
        choice.text = shownValue(v, textOf(one, "text"), option.unit);
      } else if (entry.isString()) {
        choice.text = entry.toString();
      } else if (entry.isDouble()) {
        choice.value = entry.toDouble();
        choice.text = numberText(choice.value, option.unit);
      }
      if (choice.text.isEmpty()) {
        choice.text = valueOfExpression(choice.expression);
      }
      if (choice.expression.isEmpty()) {
        choice.expression = constantExpression(option, choice.text);
      }
      option.values.append(choice);
    }
  } else if (kind == QStringLiteral("range")) {
    option.kind = PartOption::Kind::Range;
    option.min = numberOf(allowed, "min");
    option.max = numberOf(allowed, "max");
    option.step = numberOf(allowed, "step");
    option.minText = textOf(allowed, "min_text");
    option.maxText = textOf(allowed, "max_text");
    option.stepText = textOf(allowed, "step_text");
  }
  return option;
}

} // namespace

QString PartOption::allowedText() const {
  switch (kind) {
  case Kind::List: {
    QStringList texts;
    for (const OptionValueChoice& choice : values) {
      texts << choice.text;
    }
    return texts.isEmpty() ? QObject::tr("none") : texts.join(QStringLiteral(", "));
  }
  case Kind::Range: {
    const QString low = !minText.isEmpty() ? minText : std::isfinite(min) ? numberText(min, unit) : QStringLiteral("-");
    const QString high =
        !maxText.isEmpty() ? maxText : std::isfinite(max) ? numberText(max, unit) : QStringLiteral("-");
    QString out = QStringLiteral("%1 … %2").arg(low, high);
    if (!stepText.isEmpty() || (std::isfinite(step) && step > 0.0)) {
      out += QStringLiteral(", step %1").arg(!stepText.isEmpty() ? stepText : numberText(step, unit));
    }
    return out;
  }
  case Kind::Free:
    break;
  }
  return QObject::tr("any");
}

bool PartOption::allows(const QString& candidate) const {
  if (kind == Kind::List) {
    for (const OptionValueChoice& choice : values) {
      if (choice.text == candidate || choice.expression == candidate) {
        return true;
      }
    }
    return false;
  }
  return true;
}

bool PartOption::allowsNumber(double candidate) const {
  if (!std::isfinite(candidate)) {
    return false;
  }
  if (kind == Kind::List) {
    for (const OptionValueChoice& choice : values) {
      if (std::isfinite(choice.value) && std::abs(choice.value - candidate) <= 1e-9 * std::max(1.0, std::abs(candidate))) {
        return true;
      }
    }
    return false;
  }
  if (kind != Kind::Range) {
    return true;
  }
  const double tolerance = 1e-9 * std::max(1.0, std::abs(candidate));
  if ((std::isfinite(min) && candidate < min - tolerance) || (std::isfinite(max) && candidate > max + tolerance)) {
    return false;
  }
  if (std::isfinite(step) && step > 0.0) {
    const double base = std::isfinite(min) ? min : 0.0;
    const double steps = (candidate - base) / step;
    return std::abs(steps - std::round(steps)) <= 1e-6;
  }
  return true;
}

QString OptionMove::text() const {
  QString out = QStringLiteral("%1 %2 → %3").arg(option, from, to);
  if (!rule.isEmpty()) {
    out += QStringLiteral(": ") + rule;
  }
  return out;
}

const PartOption* OptionsAnswer::option(const QString& name) const {
  for (const PartOption& one : options) {
    if (one.name == name) {
      return &one;
    }
  }
  return nullptr;
}

OptionsAnswer optionsAnswer(const QJsonObject& answer) {
  OptionsAnswer out;
  const QJsonValue error = answer.value(QStringLiteral("error"));
  if (error.isObject()) {
    out.error = textOf(error.toObject(), "message");
    if (out.error.isEmpty()) {
      out.error = QObject::tr("the options could not be read");
    }
    return out;
  }
  if (error.isString()) {
    out.error = error.toString();
    return out;
  }
  for (const QJsonValue& value : answer.value(QStringLiteral("options")).toArray()) {
    if (value.isObject()) {
      out.options.append(optionOf(value.toObject()));
    }
  }
  const QJsonValue row = answer.value(QStringLiteral("row"));
  out.row = row.isObject() ? textOf(row.toObject(), "name") : row.toString();
  for (const QJsonValue& value : answer.value(QStringLiteral("moves")).toArray()) {
    const QJsonObject move = value.toObject();
    out.moves.append({textOf(move, "option"), textOf(move, "from"), textOf(move, "to"), textOf(move, "rule")});
  }
  const QJsonValue refused = answer.value(QStringLiteral("refused"));
  if (refused.isObject()) {
    const QJsonObject why = refused.toObject();
    out.refused = textOf(why, "text");
    if (out.refused.isEmpty()) {
      out.refused = QStringList{textOf(why, "rule"), textOf(why, "message")}.join(QStringLiteral(": "));
    }
  } else if (refused.isString()) {
    out.refused = refused.toString();
  }
  out.problems = textsOf(answer.value(QStringLiteral("problems")));
  return out;
}

OptionsQuery libraryOptionsQuery(const JsonRunner& run, const QString& url, const QString& library, const QString& rev,
                                 const QString& component, const std::function<QString()>& standard) {
  return [run, url, library, rev, component, standard](const QJsonObject& values) {
    QJsonObject request{{QStringLiteral("cmd"), QStringLiteral("library_configure")},
                        {QStringLiteral("component"), component},
                        {QStringLiteral("values"), values}};
    const QString chosen = standard ? standard() : QString();
    if (!chosen.isEmpty()) {
      request.insert(QStringLiteral("standard"), chosen);
    }
    if (!url.isEmpty()) {
      request.insert(QStringLiteral("url"), url);
    }
    if (!library.isEmpty()) {
      request.insert(QStringLiteral("id"), library);
    }
    if (!rev.isEmpty()) {
      request.insert(QStringLiteral("rev"), rev);
    }
    return optionsAnswer(run(request));
  };
}

OptionsQuery componentOptionsQuery(const JsonRunner& query, const QString& component,
                                   const std::function<QJsonValue()>& parameters) {
  return [query, component, parameters](const QJsonObject& values) {
    QJsonObject request{{QStringLiteral("query"), QStringLiteral("configure")},
                        {QStringLiteral("component"), component},
                        {QStringLiteral("values"), values}};
    const QJsonValue expressions = parameters ? parameters() : QJsonValue();
    if (expressions.isObject()) {
      request.insert(QStringLiteral("parameters"), expressions);
    }
    return optionsAnswer(query(request));
  };
}

QJsonObject setComponentParametersCommand(const QString& component, const QJsonObject& parameters) {
  return {{QStringLiteral("cmd"), QStringLiteral("set_component_parameters")},
          {QStringLiteral("component"), component},
          {QStringLiteral("parameters"), parameters}};
}

bool isOpenTable(const QJsonObject& configurations) {
  // library_show gives `selectors` beside `options` for the pickers of
  // before mitcad#798; a table with `selectors` alone is of the old form,
  // closed.
  return !configurations.value(QStringLiteral("options")).toArray().isEmpty() &&
         !configurations.value(QStringLiteral("closed")).toBool();
}

QStringList tableOptions(const QJsonObject& configurations) {
  QStringList out;
  const QJsonValue options = configurations.value(QStringLiteral("options"));
  for (const QJsonValue& value : (options.isArray() ? options : configurations.value(QStringLiteral("selectors"))).toArray()) {
    if (!value.toString().isEmpty()) {
      out << value.toString();
    }
  }
  return out;
}

QString rowOptionText(const QJsonObject& row, const QString& option) {
  const QJsonValue value = row.value(QStringLiteral("values")).toObject().value(option);
  if (value.isString()) {
    return valueOfExpression(value.toString());
  }
  if (value.isDouble()) {
    return QString::number(value.toDouble(), 'g', 12);
  }
  return row.value(QStringLiteral("select")).toObject().value(option).toString();
}

QJsonObject rowOptionValues(const QJsonObject& configurations, const QJsonObject& row) {
  QJsonObject out;
  for (const QString& option : tableOptions(configurations)) {
    const QString text = rowOptionText(row, option);
    if (!text.isEmpty()) {
      out.insert(option, text);
    }
  }
  return out;
}

QString valueOfExpression(const QString& expression) {
  const QString trimmed = expression.trimmed();
  if (trimmed.size() >= 2 && trimmed.startsWith(QLatin1Char('\'')) && trimmed.endsWith(QLatin1Char('\''))) {
    QString inner = trimmed.mid(1, trimmed.size() - 2);
    inner.replace(QStringLiteral("''"), QStringLiteral("'"));
    return inner;
  }
  return trimmed;
}

bool isConstantExpression(const QString& expression) {
  const QString trimmed = expression.trimmed();
  if (trimmed.size() >= 2 && trimmed.startsWith(QLatin1Char('\'')) && trimmed.endsWith(QLatin1Char('\''))) {
    // One quoted text: no quote inside but doubled ones.
    QString inner = trimmed.mid(1, trimmed.size() - 2);
    inner.remove(QStringLiteral("''"));
    return !inner.contains(QLatin1Char('\''));
  }
  static const QRegularExpression number(
      QStringLiteral("^[+-]?(\\d+([.,]\\d*)?|[.,]\\d+)([eE][+-]?\\d+)?\\s*([A-Za-z]+)?$"));
  return number.match(trimmed).hasMatch();
}

QString constantExpression(const PartOption& option, const QString& text) {
  if (option.text) {
    QString quoted = valueOfExpression(text);
    quoted.replace(QLatin1Char('\''), QStringLiteral("''"));
    return QLatin1Char('\'') + quoted + QLatin1Char('\'');
  }
  const QString trimmed = text.trimmed();
  bool bare = false;
  trimmed.toDouble(&bare);
  if (bare && !option.unit.isEmpty()) {
    return trimmed + QLatin1Char(' ') + option.unit;
  }
  return trimmed;
}

QJsonValue queryValue(const PartOption& option, const QString& text) {
  if (option.text) {
    return valueOfExpression(text);
  }
  return constantExpression(option, text);
}

double unitFactor(const QString& unit) {
  if (unit == QStringLiteral("cm")) {
    return 10.0;
  }
  if (unit == QStringLiteral("m")) {
    return 1000.0;
  }
  if (unit == QStringLiteral("in")) {
    return 25.4;
  }
  if (unit == QStringLiteral("ft")) {
    return 304.8;
  }
  if (unit == QStringLiteral("um")) {
    return 0.001;
  }
  if (unit == QStringLiteral("deg")) {
    return kPi / 180.0;
  }
  return 1.0;
}

double constantNumber(const QString& expression, const QString& unit) {
  static const QRegularExpression number(
      QStringLiteral("^\\s*([+-]?(?:\\d+(?:[.,]\\d*)?|[.,]\\d+)(?:[eE][+-]?\\d+)?)\\s*([A-Za-z]*)\\s*$"));
  const QRegularExpressionMatch match = number.match(expression);
  if (!match.hasMatch()) {
    return std::numeric_limits<double>::quiet_NaN();
  }
  QString digits = match.captured(1);
  digits.replace(QLatin1Char(','), QLatin1Char('.'));
  bool ok = false;
  const double value = digits.toDouble(&ok);
  if (!ok) {
    return std::numeric_limits<double>::quiet_NaN();
  }
  const QString written = match.captured(2);
  return value * unitFactor(written.isEmpty() ? unit : written);
}

bool sameValue(const PartOption& option, const QJsonValue& value) {
  if (option.text) {
    return value.toString() == option.valueText;
  }
  const double number = value.isDouble() ? value.toDouble() : constantNumber(value.toString(), option.unit);
  return std::isfinite(number) && std::isfinite(option.value) &&
         std::abs(number - option.value) <= 1e-9 * std::max(1.0, std::abs(number));
}

QString numberText(double value, const QString& unit) {
  if (!std::isfinite(value)) {
    return QString();
  }
  double shown = value / unitFactor(unit);
  // Rounding noise of the unit's conversion away: 25.4 mm is 1 in.
  const double rounded = std::round(shown * 1e9) / 1e9;
  shown = std::abs(rounded) < 1e15 ? rounded : shown;
  const QString number = QString::number(shown, 'g', 12);
  return unit.isEmpty() ? number : QStringLiteral("%1 %2").arg(number, unit);
}

QString optionsSummary(const OptionsAnswer& answer) {
  QStringList parts;
  for (const PartOption& option : answer.options) {
    parts << QStringLiteral("%1 = %2%3").arg(option.name, option.valueText,
                                             option.driven ? QStringLiteral(" (driven)") : QString());
  }
  return QStringLiteral("%1 (%2)").arg(parts.join(QStringLiteral(", ")),
                                       answer.row.isEmpty() ? QStringLiteral("no row")
                                                            : QStringLiteral("row %1").arg(answer.row));
}

// --- The rows' standards and suppliers (mitcad#949)

namespace {

bool sameTag(const QString& a, const QString& b) { return a.trimmed().compare(b.trimmed(), Qt::CaseInsensitive) == 0; }

bool isNonStandard(const QString& tag) { return sameTag(tag, nonStandardTag()); }

QStringList rowStandardTags(const QJsonObject& row) {
  QStringList out;
  for (const QJsonValue& value : row.value(QStringLiteral("standards")).toArray()) {
    const QString tag = value.toString().trimmed();
    if (!tag.isEmpty()) {
      out << tag;
    }
  }
  return out;
}

// Two option values the same: the same text, or the same number ("8",
// "8 mm").
bool sameOptionText(const QString& a, const QString& b) {
  if (a.trimmed() == b.trimmed()) {
    return true;
  }
  const double x = constantNumber(a, QStringLiteral("mm"));
  const double y = constantNumber(b, QStringLiteral("mm"));
  return std::isfinite(x) && std::isfinite(y) && std::abs(x - y) <= 1e-9 * std::max(1.0, std::abs(x));
}

} // namespace

bool rowPasses(const QJsonObject& row, const RowFilter& filter) {
  if (!filter.standard.isEmpty()) {
    const QStringList tags = rowStandardTags(row);
    if (std::none_of(tags.cbegin(), tags.cend(), [&filter](const QString& tag) { return sameTag(tag, filter.standard); })) {
      return false;
    }
  }
  if (!filter.supplier.isEmpty()) {
    const QJsonArray parts = row.value(QStringLiteral("supplier_parts")).toArray();
    const bool sold = std::any_of(parts.cbegin(), parts.cend(), [&filter](const QJsonValue& part) {
      const QJsonObject one = part.toObject();
      return sameTag(textOf(one, "supplier"), filter.supplier) ||
             (!textOf(one, "supplier_name").isEmpty() && sameTag(textOf(one, "supplier_name"), filter.supplier));
    });
    if (!sold) {
      return false;
    }
  }
  return true;
}

QStringList rowStandards(const QJsonArray& rows, const QStringList& order) {
  QStringList out;
  bool odd = false;
  const auto add = [&out, &odd](const QString& tag) {
    if (isNonStandard(tag)) {
      odd = true;
      return;
    }
    if (std::none_of(out.cbegin(), out.cend(), [&tag](const QString& known) { return sameTag(known, tag); })) {
      out << tag;
    }
  };
  for (const QString& tag : order) {
    if (!tag.trimmed().isEmpty()) {
      add(tag.trimmed());
    }
  }
  for (const QJsonValue& value : rows) {
    for (const QString& tag : rowStandardTags(value.toObject())) {
      add(tag);
    }
  }
  if (odd) {
    out << nonStandardTag();
  }
  return out;
}

QList<RowSupplier> rowSuppliers(const QJsonArray& rows) {
  QList<RowSupplier> out;
  for (const QJsonValue& value : rows) {
    for (const QJsonValue& part : value.toObject().value(QStringLiteral("supplier_parts")).toArray()) {
      const QJsonObject one = part.toObject();
      const QString id = textOf(one, "supplier");
      if (id.isEmpty() ||
          std::any_of(out.cbegin(), out.cend(), [&id](const RowSupplier& known) { return known.id == id; })) {
        continue;
      }
      const QString name = textOf(one, "supplier_name");
      out.append({id, name.isEmpty() ? id : name});
    }
  }
  return out;
}

QJsonArray filteredRows(const QJsonArray& rows, const RowFilter& filter) {
  if (filter.isAll()) {
    return rows;
  }
  QJsonArray out;
  for (const QJsonValue& value : rows) {
    if (rowPasses(value.toObject(), filter)) {
      out.append(value);
    }
  }
  return out;
}

QJsonObject filteredTable(const QJsonObject& configurations, const RowFilter& filter) {
  if (filter.isAll()) {
    return configurations;
  }
  QJsonObject out = configurations;
  const QJsonArray rows = filteredRows(configurations.value(QStringLiteral("rows")).toArray(), filter);
  out.insert(QStringLiteral("rows"), rows);
  const QString wanted = textOf(configurations, "default");
  const bool kept = std::any_of(rows.cbegin(), rows.cend(),
                                [&wanted](const QJsonValue& row) { return textOf(row.toObject(), "name") == wanted; });
  if (!kept) {
    if (rows.isEmpty()) {
      out.remove(QStringLiteral("default"));
    } else {
      out.insert(QStringLiteral("default"), textOf(rows.first().toObject(), "name"));
    }
  }
  return out;
}

QStringList filteredOptionValues(const QJsonArray& rows, const QStringList& options, int index,
                                 const QJsonObject& chosen, const RowFilter& filter) {
  QStringList out;
  if (index < 0 || index >= options.size()) {
    return out;
  }
  for (const QJsonValue& value : rows) {
    const QJsonObject row = value.toObject();
    if (!rowPasses(row, filter)) {
      continue;
    }
    bool matches = true;
    for (int j = 0; j < index && matches; ++j) {
      const QJsonValue before = chosen.value(options[j]);
      if (before.isString() && !before.toString().isEmpty()) {
        matches = sameOptionText(rowOptionText(row, options[j]), before.toString());
      }
    }
    const QString text = rowOptionText(row, options[index]);
    if (matches && !text.isEmpty() && !out.contains(text)) {
      out << text;
    }
  }
  return out;
}

void narrowAnswer(OptionsAnswer& answer, const QJsonArray& rows, const RowFilter& filter) {
  if (filter.isAll() || rows.isEmpty()) {
    return;
  }
  QStringList names;
  QJsonObject chosen;
  for (const PartOption& option : answer.options) {
    names << option.name;
    chosen.insert(option.name, option.valueText);
  }
  for (int i = 0; i < answer.options.size(); ++i) {
    PartOption& option = answer.options[i];
    if (option.kind != PartOption::Kind::List || option.driven) {
      continue;
    }
    const QStringList kept = filteredOptionValues(rows, names, i, chosen, filter);
    QList<OptionValueChoice> values;
    for (const OptionValueChoice& choice : option.values) {
      if (std::any_of(kept.cbegin(), kept.cend(),
                      [&choice](const QString& text) { return sameOptionText(text, choice.text); })) {
        values.append(choice);
      }
    }
    option.values = values;
  }
}

QString namedStandard(const QJsonObject& row, const QString& chosen) {
  const QStringList tags = rowStandardTags(row);
  if (!chosen.trimmed().isEmpty()) {
    for (const QString& tag : tags) {
      if (sameTag(tag, chosen)) {
        return tag;
      }
    }
  }
  return tags.isEmpty() ? QString() : tags.first();
}

QString supplierPartsText(const QJsonArray& parts) {
  QStringList texts;
  for (const QJsonValue& value : parts) {
    const QJsonObject part = value.toObject();
    const QString who = textOf(part, "supplier_name").isEmpty() ? textOf(part, "supplier") : textOf(part, "supplier_name");
    QString text = QStringLiteral("%1 %2").arg(who, textOf(part, "part_number")).trimmed();
    QStringList what;
    static const char* const kWhat[] = {"material", "finish"};
    for (const char* key : kWhat) {
      if (!textOf(part, key).trimmed().isEmpty()) {
        what << textOf(part, key).trimmed();
      }
    }
    if (!what.isEmpty()) {
      text += QStringLiteral(" (%1)").arg(what.join(QLatin1Char(' ')));
    }
    texts << text;
  }
  return texts.join(QStringLiteral("; "));
}

QString rowFilterText(const RowFilter& filter) {
  return QStringLiteral("Standard %1, Supplier %2")
      .arg(filter.standard.isEmpty() ? QStringLiteral("All") : filter.standard,
           filter.supplier.isEmpty() ? QStringLiteral("All") : filter.supplier);
}

QList<SupplierArticle> supplierArticles(const QJsonArray& parts, const QJsonArray& suppliers,
                                        const QString& productPage, const QString& firstSupplier) {
  QList<SupplierArticle> first;
  QList<SupplierArticle> rest;
  for (const QJsonValue& value : parts) {
    const QJsonObject part = value.toObject();
    SupplierArticle article;
    article.supplier = textOf(part, "supplier").trimmed();
    article.supplierName = textOf(part, "supplier_name").trimmed();
    for (const QJsonValue& entry : suppliers) {
      const QJsonObject supplier = entry.toObject();
      if (textOf(supplier, "id").trimmed().compare(article.supplier, Qt::CaseInsensitive) != 0) {
        continue;
      }
      article.supplierUrl = textOf(supplier, "url").trimmed();
      if (article.supplierName.isEmpty()) {
        article.supplierName = textOf(supplier, "name").trimmed();
      }
    }
    if (article.supplierName.isEmpty()) {
      article.supplierName = article.supplier;
    }
    article.partNumber = textOf(part, "part_number").trimmed();
    article.material = textOf(part, "material").trimmed();
    article.finish = textOf(part, "finish").trimmed();
    article.checked = textOf(part, "checked").trimmed();
    article.productPage = productPage.trimmed();
    const bool chosen = !firstSupplier.trimmed().isEmpty() &&
                        article.supplier.compare(firstSupplier.trimmed(), Qt::CaseInsensitive) == 0;
    (chosen ? first : rest).append(article);
  }
  return first + rest;
}

QList<SupplierColumn> supplierColumns(const QList<SupplierArticle>& articles) {
  static const SupplierColumn kAll[] = {SupplierColumn::Supplier, SupplierColumn::PartNumber,
                                        SupplierColumn::Material, SupplierColumn::Finish,
                                        SupplierColumn::Checked,  SupplierColumn::ProductPage};
  QList<SupplierColumn> out;
  for (const SupplierColumn column : kAll) {
    const bool filled = std::any_of(articles.cbegin(), articles.cend(), [column](const SupplierArticle& article) {
      return !supplierCell(article, column).isEmpty();
    });
    if (filled) {
      out.append(column);
    }
  }
  return out;
}

QString supplierColumnName(SupplierColumn column) {
  switch (column) {
  case SupplierColumn::Supplier:
    return QStringLiteral("Supplier");
  case SupplierColumn::PartNumber:
    return QStringLiteral("Part number");
  case SupplierColumn::Material:
    return QStringLiteral("Material");
  case SupplierColumn::Finish:
    return QStringLiteral("Finish");
  case SupplierColumn::Checked:
    return QStringLiteral("Checked");
  case SupplierColumn::ProductPage:
    return QStringLiteral("Product page");
  }
  return QString();
}

QString supplierCell(const SupplierArticle& article, SupplierColumn column) {
  switch (column) {
  case SupplierColumn::Supplier:
    return article.supplierName;
  case SupplierColumn::PartNumber:
    return article.partNumber;
  case SupplierColumn::Material:
    return article.material;
  case SupplierColumn::Finish:
    return article.finish;
  case SupplierColumn::Checked:
    return article.checked;
  case SupplierColumn::ProductPage:
    return article.productPage;
  }
  return QString();
}

QString supplierCellLink(const SupplierArticle& article, SupplierColumn column) {
  if (column == SupplierColumn::Supplier) {
    return article.supplierUrl;
  }
  if (column == SupplierColumn::ProductPage) {
    return article.productPage;
  }
  return QString();
}

QString supplierArticlesTsv(const QList<SupplierArticle>& articles, const QList<SupplierColumn>& columns) {
  static const QRegularExpression kBreaks(QStringLiteral("[\\t\\r\\n]+"));
  const auto line = [](const QStringList& cells) {
    QStringList clean;
    for (const QString& cell : cells) {
      clean << QString(cell).replace(kBreaks, QStringLiteral(" "));
    }
    return clean.join(QLatin1Char('\t')) + QLatin1Char('\n');
  };
  QStringList headings;
  for (const SupplierColumn column : columns) {
    headings << supplierColumnName(column);
  }
  QString out = line(headings);
  for (const SupplierArticle& article : articles) {
    QStringList cells;
    for (const SupplierColumn column : columns) {
      cells << supplierCell(article, column);
    }
    out += line(cells);
  }
  return out;
}

QString supplierArticlesLog(const QString& row, const QList<SupplierArticle>& articles,
                            const QList<SupplierColumn>& columns) {
  QStringList names;
  for (const SupplierColumn column : columns) {
    names << supplierColumnName(column);
  }
  return QStringLiteral("Supplier parts of %1: %2 articles, columns %3")
      .arg(row.isEmpty() ? QStringLiteral("-") : row)
      .arg(articles.size())
      .arg(names.join(QStringLiteral(", ")));
}

} // namespace mitcad
