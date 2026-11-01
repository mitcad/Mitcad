// SPDX-License-Identifier: MIT
#include "PrintKeys.hpp"

#include <algorithm>
#include <cmath>
#include <initializer_list>
#include <limits>
#include <optional>
#include <utility>

#include <QJsonArray>
#include <QLatin1String>
#include <QObject>
#include <QRegularExpression>
#include <QSet>

#include "../framework/Numbers.hpp"

namespace mitcad {
namespace {

QString text(const QJsonObject& object, const char* field) {
  return object.value(QLatin1String(field)).toString();
}

// A range from `min` and `max` (mitcad#746); a missing bound is none.
PrintRange rangeOf(const QJsonObject& object) {
  PrintRange range;
  const QJsonValue low = object.value(QStringLiteral("min"));
  const QJsonValue high = object.value(QStringLiteral("max"));
  if (low.isDouble()) {
    range.low = low.toDouble();
  }
  if (high.isDouble()) {
    range.high = high.toDouble();
  }
  return range;
}

// The first of the fields that is a string, else the value itself when it
// is one (an entry given as a plain string).
QString firstText(const QJsonValue& value, std::initializer_list<const char*> fields) {
  if (value.isString()) {
    return value.toString();
  }
  const QJsonObject object = value.toObject();
  for (const char* field : fields) {
    const QJsonValue entry = object.value(QLatin1String(field));
    if (entry.isString() && !entry.toString().isEmpty()) {
      return entry.toString();
    }
  }
  return QString();
}

PrintKey::Kind kindOf(const QString& name) {
  if (name == QStringLiteral("boolean")) {
    return PrintKey::Kind::Boolean;
  }
  if (name == QStringLiteral("integer")) {
    return PrintKey::Kind::Integer;
  }
  if (name == QStringLiteral("number")) {
    return PrintKey::Kind::Number;
  }
  if (name == QStringLiteral("percent")) {
    return PrintKey::Kind::Percent;
  }
  if (name == QStringLiteral("number_or_percent")) {
    return PrintKey::Kind::NumberOrPercent;
  }
  if (name == QStringLiteral("color")) {
    return PrintKey::Kind::Color;
  }
  if (name == QStringLiteral("choice")) {
    return PrintKey::Kind::Choice;
  }
  if (name == QStringLiteral("points")) {
    return PrintKey::Kind::Points;
  }
  return PrintKey::Kind::Text;
}

bool same(const QString& a, const QString& b) { return !a.isEmpty() && a.compare(b, Qt::CaseInsensitive) == 0; }

// The categories of a set as the layout gives them.
QVector<PrintCategory> categoriesOf(const QJsonValue& value) {
  QJsonArray entries = value.toArray();
  if (value.isObject()) {
    entries = value.toObject().value(QStringLiteral("categories")).toArray();
  }
  QVector<PrintCategory> categories;
  for (const QJsonValue& entry : std::as_const(entries)) {
    PrintCategory category;
    category.id = firstText(entry, {"id", "category", "key", "name", "title"});
    category.title = firstText(entry, {"name", "title"});
    if (category.title.isEmpty()) {
      category.title = titleOfId(category.id);
    }
    for (const QJsonValue& groupEntry : entry.toObject().value(QStringLiteral("groups")).toArray()) {
      PrintGroup group;
      group.id = firstText(groupEntry, {"id", "group", "key", "name", "title"});
      group.title = firstText(groupEntry, {"name", "title"});
      if (group.title.isEmpty()) {
        group.title = titleOfId(group.id);
      }
      if (!group.id.isEmpty()) {
        category.groups.append(group);
      }
    }
    if (!category.id.isEmpty()) {
      categories.append(category);
    }
  }
  return categories;
}

int indexOfCategory(const QVector<PrintCategory>& categories, const QString& name) {
  for (int i = 0; i < categories.size(); ++i) {
    if (same(categories[i].id, name) || same(categories[i].title, name)) {
      return i;
    }
  }
  return -1;
}

int indexOfGroup(const PrintCategory& category, const QString& name) {
  for (int i = 0; i < category.groups.size(); ++i) {
    if (same(category.groups[i].id, name) || same(category.groups[i].title, name)) {
      return i;
    }
  }
  return -1;
}

const QSet<QString>& yesWords() {
  static const QSet<QString> words{QStringLiteral("yes"), QStringLiteral("true"), QStringLiteral("1"),
                                   QStringLiteral("on")};
  return words;
}

const QSet<QString>& noWords() {
  static const QSet<QString> words{QStringLiteral("no"), QStringLiteral("false"), QStringLiteral("0"),
                                   QStringLiteral("off")};
  return words;
}

// A number of a value given as a number or as text ("0.45", "15%").
std::optional<double> numberOf(const QJsonValue& value) {
  if (value.isDouble()) {
    return value.toDouble();
  }
  if (value.isString()) {
    QString number = value.toString().trimmed();
    if (number.endsWith(QLatin1Char('%'))) {
      number.chop(1);
    }
    return parsePlainNumber(number);
  }
  return std::nullopt;
}

QString withUnit(const QString& number, const QString& unit) {
  return unit.isEmpty() ? number : QStringLiteral("%1 %2").arg(number, unit);
}

} // namespace

QString PrintKey::choiceName(const QString& value) const {
  for (const auto& [choice, plain] : choices) {
    if (choice == value) {
      return plain.isEmpty() ? choice : plain;
    }
  }
  return value;
}

PrintKeyTable::PrintKeyTable(const QJsonObject& answer) {
  for (const QJsonValue& value : answer.value(QStringLiteral("keys")).toArray()) {
    const QJsonObject object = value.toObject();
    PrintKey key;
    key.key = text(object, "key");
    if (key.key.isEmpty()) {
      continue;
    }
    key.set = text(object, "set");
    key.name = text(object, "name");
    if (key.name.isEmpty()) {
      key.name = key.key;
    }
    key.kind = kindOf(text(object, "kind"));
    key.unit = text(object, "unit");
    key.of = text(object, "of");
    for (const QJsonValue& choice : object.value(QStringLiteral("choices")).toArray()) {
      // {value, name} (mitcad#655), or a plain value.
      const QString choiceValue = firstText(choice, {"value", "id"});
      const QString plain = choice.isObject() ? text(choice.toObject(), "name") : QString();
      key.choices.append({choiceValue, plain.isEmpty() ? choiceValue : plain});
    }
    key.perDialect = object.value(QStringLiteral("per_dialect")).toBool();
    key.category = firstText(object.value(QStringLiteral("category")), {"id", "name"});
    key.group = firstText(object.value(QStringLiteral("group")), {"id", "name"});
    key.advanced = object.value(QStringLiteral("advanced")).toBool();
    key.range = rangeOf(object);
    for (const QJsonValue& dialect : object.value(QStringLiteral("dialects")).toArray()) {
      // A slicer that takes less than the key (mitcad#746).
      const QJsonObject entry = dialect.toObject();
      if (!text(entry, "dialect").isEmpty()) {
        key.dialects << text(entry, "dialect");
      }
      if (entry.contains(QStringLiteral("min")) || entry.contains(QStringLiteral("max"))) {
        key.dialectRanges.insert(text(entry, "dialect"), rangeOf(entry));
      }
      // Cura's bed is several keys: "machine_width, machine_depth, ...".
      for (const QString& part : text(dialect.toObject(), "key").split(QLatin1Char(','), Qt::SkipEmptyParts)) {
        key.dialectKeys << part.trimmed();
      }
      // A number-or-percentage key: the dialects that take a percentage.
      if (dialect.toObject().value(QStringLiteral("percent")).toBool()) {
        key.percentDialects << text(dialect.toObject(), "dialect");
      }
    }
    if (!m_names.contains(key.key)) {
      m_names.insert(key.key, key.name);
    }
    for (const QString& raw : std::as_const(key.dialectKeys)) {
      if (!m_names.contains(raw)) {
        m_names.insert(raw, key.name);
      }
    }
    m_index.insert(key.key, static_cast<int>(m_keys.size()));
    m_keys.append(key);
  }
  // The layout: by set, or a list of sets with their categories.
  const QJsonValue layout = answer.value(QStringLiteral("layout"));
  if (layout.isObject()) {
    const QJsonObject sets = layout.toObject();
    for (auto it = sets.begin(); it != sets.end(); ++it) {
      m_layout.insert(it.key(), categoriesOf(it.value()));
    }
  } else {
    for (const QJsonValue& entry : layout.toArray()) {
      m_layout.insert(text(entry.toObject(), "set"), categoriesOf(entry.toObject().value(QStringLiteral("categories"))));
    }
  }
  // Every key has its place: a category or group the layout lacks comes
  // after the layout's, in the keys' order.
  for (const PrintKey& key : std::as_const(m_keys)) {
    QVector<PrintCategory>& categories = m_layout[key.set];
    const QString categoryName = key.category.isEmpty() ? QStringLiteral("settings") : key.category;
    int category = indexOfCategory(categories, categoryName);
    if (category < 0) {
      categories.append({categoryName, titleOfId(categoryName), {}});
      category = static_cast<int>(categories.size()) - 1;
    }
    const QString groupName = key.group.isEmpty() ? categories[category].id : key.group;
    if (indexOfGroup(categories[category], groupName) < 0) {
      categories[category].groups.append({groupName, titleOfId(groupName)});
    }
  }
}

const PrintKey* PrintKeyTable::find(const QString& key) const {
  const auto it = m_index.constFind(key);
  return it == m_index.cend() ? nullptr : &m_keys[it.value()];
}

QVector<const PrintKey*> PrintKeyTable::keys(const QString& set) const {
  QVector<const PrintKey*> out;
  for (const PrintKey& key : m_keys) {
    if (key.set == set) {
      out.append(&key);
    }
  }
  return out;
}

QVector<PrintCategory> PrintKeyTable::layout(const QString& set) const { return m_layout.value(set); }

QString PrintKeyTable::categoryOf(const PrintKey& key) const {
  const QVector<PrintCategory> categories = m_layout.value(key.set);
  const int index = indexOfCategory(categories, key.category.isEmpty() ? QStringLiteral("settings") : key.category);
  return index < 0 ? QString() : categories[index].id;
}

QString PrintKeyTable::groupOf(const PrintKey& key) const {
  const QVector<PrintCategory> categories = m_layout.value(key.set);
  const int category = indexOfCategory(categories, key.category.isEmpty() ? QStringLiteral("settings") : key.category);
  if (category < 0) {
    return QString();
  }
  const int group = indexOfGroup(categories[category], key.group.isEmpty() ? categories[category].id : key.group);
  return group < 0 ? QString() : categories[category].groups[group].id;
}

QString PrintKeyTable::plainName(const QString& raw) const { return m_names.value(raw); }

QString PrintKeyTable::plainText(const QString& text) const {
  QString out = text;
  // "key: what went wrong" or "key for bambu_studio: ...": the key first,
  // whatever it looks like.
  static const QRegularExpression lead(QStringLiteral("^([A-Za-z0-9_]+)\\b"));
  const QRegularExpressionMatch first = lead.match(out);
  if (first.hasMatch() && m_names.contains(first.captured(1))) {
    out.replace(0, first.capturedLength(1), m_names.value(first.captured(1)));
  }
  // Keys in the text: words with an underscore the table knows.
  static const QRegularExpression word(QStringLiteral("\\b[A-Za-z][A-Za-z0-9]*_[A-Za-z0-9_]*\\b"));
  QString result;
  qsizetype at = 0;
  QRegularExpressionMatchIterator it = word.globalMatch(out);
  while (it.hasNext()) {
    const QRegularExpressionMatch match = it.next();
    result += out.mid(at, match.capturedStart() - at);
    result += m_names.value(match.captured(), match.captured());
    at = match.capturedEnd();
  }
  result += out.mid(at);
  // A slicer by its title (mitcad#655: the export's warnings name them by id).
  static const QRegularExpression slicer(
      QStringLiteral("\\b(bambu_studio|orcaslicer|qidi_studio|prusaslicer|cura)\\b"));
  QString titled;
  at = 0;
  QRegularExpressionMatchIterator slicers = slicer.globalMatch(result);
  while (slicers.hasNext()) {
    const QRegularExpressionMatch match = slicers.next();
    titled += result.mid(at, match.capturedStart() - at);
    titled += slicerTitle(match.captured());
    at = match.capturedEnd();
  }
  titled += result.mid(at);
  return titled;
}

QString titleOfId(const QString& id) {
  QString title = id;
  title.replace(QLatin1Char('_'), QLatin1Char(' '));
  if (!title.isEmpty()) {
    title[0] = title[0].toUpper();
  }
  return title;
}

QString slicerTitle(const QString& id) {
  static const QHash<QString, QString> names{{QStringLiteral("bambu_studio"), QStringLiteral("Bambu Studio")},
                                             {QStringLiteral("orcaslicer"), QStringLiteral("OrcaSlicer")},
                                             {QStringLiteral("qidi_studio"), QStringLiteral("QIDI Studio")},
                                             {QStringLiteral("prusaslicer"), QStringLiteral("PrusaSlicer")},
                                             {QStringLiteral("cura"), QStringLiteral("UltiMaker Cura")}};
  return names.value(id, titleOfId(id));
}

bool bambuFamily(const QString& dialect) {
  return dialect == QLatin1String("bambu_studio") || dialect == QLatin1String("orcaslicer") ||
         dialect == QLatin1String("qidi_studio");
}

QString slicerPresetOf(const QJsonValue& value, const QString& dialect, QString* from) {
  if (from != nullptr) {
    from->clear();
  }
  if (value.isString()) {
    return value.toString().trimmed();
  }
  const QJsonObject slicers = value.toObject();
  const QString own = slicers.value(dialect).toString().trimmed();
  if (!own.isEmpty() || !bambuFamily(dialect)) {
    if (from != nullptr && !own.isEmpty()) {
      *from = dialect;
    }
    return own;
  }
  for (const QString& member :
       {QStringLiteral("bambu_studio"), QStringLiteral("orcaslicer"), QStringLiteral("qidi_studio")}) {
    if (!slicers.contains(member)) {
      continue;
    }
    // The model takes the first member that has an entry, as it is.
    if (from != nullptr) {
      *from = member;
    }
    return slicers.value(member).toString().trimmed();
  }
  return QString();
}

QString printNumberText(double value) {
  if (!std::isfinite(value)) {
    return QString();
  }
  QString shown = QString::number(value, 'f', 6);
  while (shown.contains(QLatin1Char('.')) && (shown.endsWith(QLatin1Char('0')) || shown.endsWith(QLatin1Char('.')))) {
    shown.chop(1);
  }
  return shown == QStringLiteral("-0") ? QStringLiteral("0") : shown;
}

QString outlineText(const QJsonValue& points) {
  const QJsonArray list = points.toArray();
  bool first = true;
  double minX = 0.0;
  double maxX = 0.0;
  double minY = 0.0;
  double maxY = 0.0;
  for (const QJsonValue& value : list) {
    const QJsonArray point = value.toArray();
    if (point.size() < 2) {
      continue;
    }
    const double x = point.at(0).toDouble();
    const double y = point.at(1).toDouble();
    minX = first ? x : std::min(minX, x);
    maxX = first ? x : std::max(maxX, x);
    minY = first ? y : std::min(minY, y);
    maxY = first ? y : std::max(maxY, y);
    first = false;
  }
  if (first) {
    return QString();
  }
  return QStringLiteral("%1 × %2 mm").arg(printNumberText(maxX - minX), printNumberText(maxY - minY));
}

QString printValueText(const PrintKey& key, const QJsonValue& value) {
  if (value.isNull() || value.isUndefined()) {
    return QString();
  }
  if (value.isObject()) {
    // A value per slicer: a preset's name.
    QStringList parts;
    const QJsonObject slicers = value.toObject();
    for (auto it = slicers.begin(); it != slicers.end(); ++it) {
      parts << QStringLiteral("%1: %2").arg(slicerTitle(it.key()), it.value().toVariant().toString());
    }
    return parts.join(QStringLiteral("; "));
  }
  switch (key.kind) {
  case PrintKey::Kind::Boolean: {
    const bool on = value.isBool() ? value.toBool() : yesWords().contains(value.toVariant().toString().toLower());
    return on ? QObject::tr("Yes") : QObject::tr("No"); // as the editors' Yes/No (mitcad#655)
  }
  case PrintKey::Kind::Integer:
  case PrintKey::Kind::Number:
  case PrintKey::Kind::Percent: {
    const std::optional<double> number = numberOf(value);
    if (!number) {
      return value.toVariant().toString();
    }
    return withUnit(printNumberText(*number), key.kind == PrintKey::Kind::Percent ? QStringLiteral("%") : key.unit);
  }
  case PrintKey::Kind::NumberOrPercent: {
    const bool percent = value.isString() && value.toString().trimmed().endsWith(QLatin1Char('%'));
    const std::optional<double> number = numberOf(value);
    if (!number) {
      return value.toVariant().toString();
    }
    return withUnit(printNumberText(*number), percent ? QStringLiteral("%") : key.unit);
  }
  case PrintKey::Kind::Choice:
    return key.choiceName(value.toVariant().toString());
  case PrintKey::Kind::Points:
    return value.isArray() ? outlineText(value) : value.toVariant().toString();
  case PrintKey::Kind::Color:
  case PrintKey::Kind::Text:
    break;
  }
  if (value.isArray()) {
    QStringList parts;
    for (const QJsonValue& entry : value.toArray()) {
      parts << entry.toVariant().toString();
    }
    return parts.join(QStringLiteral(", "));
  }
  return value.isDouble() ? printNumberText(value.toDouble()) : value.toVariant().toString();
}

PrintRange printRange(const PrintKey& key, const QString& dialect) {
  // The model's range (mitcad#746), narrowed to the design's slicer's.
  PrintRange range = key.range;
  const auto own = key.dialectRanges.constFind(dialect);
  if (!dialect.isEmpty() && own != key.dialectRanges.constEnd()) {
    range.low = std::max(range.low, own->low);
    range.high = std::min(range.high, own->high);
  }
  return range;
}

namespace {

// Why a number is out of a range; empty when it is in it.
QString outOfRange(const PrintRange& range, double value) {
  if (value >= range.low && value <= range.high) {
    return QString();
  }
  if (std::isinf(range.high)) {
    return QObject::tr("Use a value of %1 or more").arg(printNumberText(range.low));
  }
  if (std::isinf(range.low)) {
    return QObject::tr("Use a value of %1 or less").arg(printNumberText(range.high));
  }
  return QObject::tr("Use a value from %1 to %2").arg(printNumberText(range.low), printNumberText(range.high));
}

} // namespace

QJsonValue printValueOf(const PrintKey& key, const QString& typed, QString* error, const QString& dialect) {
  const PrintRange range = printRange(key, dialect);
  const QString text = typed.trimmed();
  const auto fail = [error](const QString& message) {
    if (error != nullptr) {
      *error = message;
    }
    return QJsonValue();
  };
  if (error != nullptr) {
    error->clear();
  }
  if (text.isEmpty()) {
    return QJsonValue();
  }
  switch (key.kind) {
  case PrintKey::Kind::Boolean:
    if (yesWords().contains(text.toLower())) {
      return true;
    }
    if (noWords().contains(text.toLower())) {
      return false;
    }
    return fail(QObject::tr("Use yes or no"));
  case PrintKey::Kind::Integer: {
    const std::optional<double> number = parsePlainNumber(text);
    if (!number || std::abs(*number - std::round(*number)) > 1e-9) {
      return fail(QObject::tr("Use a whole number"));
    }
    if (const QString why = outOfRange(range, std::round(*number)); !why.isEmpty()) {
      return fail(why);
    }
    return std::round(*number);
  }
  case PrintKey::Kind::Number:
  case PrintKey::Kind::Percent: {
    QString number = text;
    if (key.kind == PrintKey::Kind::Percent && number.endsWith(QLatin1Char('%'))) {
      number.chop(1);
    }
    const std::optional<double> value = parsePlainNumber(number);
    if (!value) {
      return fail(QObject::tr("Use a number"));
    }
    if (const QString why = outOfRange(range, *value); !why.isEmpty()) {
      return fail(why);
    }
    return *value;
  }
  case PrintKey::Kind::NumberOrPercent: {
    QString number = text;
    const bool percent = number.endsWith(QLatin1Char('%'));
    if (percent) {
      number.chop(1);
    }
    QString normalized;
    const std::optional<double> value = parsePlainNumber(number, &normalized);
    if (!value) {
      return fail(QObject::tr("Use a number or a percentage"));
    }
    // The range is the numbers'; a percentage is only not negative where
    // the numbers are not.
    if (!percent) {
      if (const QString why = outOfRange(range, *value); !why.isEmpty()) {
        return fail(why);
      }
    } else if (*value < 0.0 && range.low >= 0.0) {
      return fail(QObject::tr("Use a value of 0 or more"));
    }
    return percent ? QJsonValue(printNumberText(*value) + QLatin1Char('%')) : QJsonValue(*value);
  }
  case PrintKey::Kind::Choice:
    for (const auto& [value, plain] : key.choices) {
      if (value.compare(text, Qt::CaseInsensitive) == 0 || plain.compare(text, Qt::CaseInsensitive) == 0) {
        return value;
      }
    }
    return fail(QObject::tr("Choose a value from the list"));
  case PrintKey::Kind::Color: {
    static const QRegularExpression hex(QStringLiteral("^#[0-9a-fA-F]{6}([0-9a-fA-F]{2})?$"));
    if (!hex.match(text).hasMatch()) {
      return fail(QObject::tr("Use a color"));
    }
    return text.toUpper();
  }
  case PrintKey::Kind::Points: {
    // "256 x 256": a rectangle from the origin; else "x,y; x,y; ...".
    static const QRegularExpression size(QStringLiteral("^\\s*([0-9.,]+)\\s*[x×X]\\s*([0-9.,]+)\\s*(mm)?\\s*$"));
    const QRegularExpressionMatch match = size.match(text);
    if (match.hasMatch()) {
      const std::optional<double> width = parsePlainNumber(match.captured(1));
      const std::optional<double> depth = parsePlainNumber(match.captured(2));
      if (width && depth && *width > 0.0 && *depth > 0.0) {
        return QJsonArray{QJsonArray{0.0, 0.0}, QJsonArray{*width, 0.0}, QJsonArray{*width, *depth},
                          QJsonArray{0.0, *depth}};
      }
    }
    QJsonArray points;
    for (const QString& part : text.split(QRegularExpression(QStringLiteral("[;\\n]")), Qt::SkipEmptyParts)) {
      const QStringList xy = part.split(QRegularExpression(QStringLiteral("[\\s,]+")), Qt::SkipEmptyParts);
      const std::optional<double> x = parsePlainNumber(xy.value(0));
      const std::optional<double> y = parsePlainNumber(xy.value(1));
      if (xy.size() != 2 || !x || !y) {
        return fail(QObject::tr("Use x, y for each point"));
      }
      points.append(QJsonArray{*x, *y});
    }
    if (points.size() < 3) {
      return fail(QObject::tr("Use at least three points"));
    }
    return points;
  }
  case PrintKey::Kind::Text:
    break;
  }
  return text;
}

} // namespace mitcad
