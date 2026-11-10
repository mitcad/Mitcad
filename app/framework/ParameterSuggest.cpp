// SPDX-License-Identifier: MIT
#include "ParameterSuggest.hpp"

#include <algorithm>
#include <cstddef>
#include <initializer_list>
#include <utility>

#include <QCoreApplication>
#include <QJsonObject>
#include <QJsonValue>
#include <QRegularExpression>
#include <QSet>

#include "Numbers.hpp"

namespace mitcad {
namespace {

// The file's texts in the context ParameterSuggest, where lupdate records
// them too (mitcad#1472).
struct Text {
  Q_DECLARE_TR_FUNCTIONS(ParameterSuggest)
};

bool isNameStart(QChar c) { return c.isLetter() || c == QLatin1Char('_'); }
bool isNameChar(QChar c) { return c.isLetterOrNumber() || c == QLatin1Char('_'); }

// The model's reserved words (core/expr/src: units.rs UNIT_NAMES,
// ast.rs FUNCTIONS and Constant, mod.rs UNSUPPORTED_NAMES). Units and the
// constant E are reserved as written.
const QSet<QString>& reservedNames() {
  static const QSet<QString> names = [] {
    QSet<QString> all;
    for (const char* name : {"um", "micron", "mm", "cm", "m", "km", "in", "inch", "ft", "foot", "yd", "mi", "mil",
                             "rad", "deg", "grad", "E"}) {
      all.insert(QString::fromLatin1(name));
    }
    all.insert(QStringLiteral("µm")); // micro sign
    all.insert(QStringLiteral("μm")); // Greek mu
    all.insert(QStringLiteral("°"));  // degree sign
    return all;
  }();
  return names;
}

// Function names, PI and the unsupported names are reserved in any case
// (mitcad#663: `Max`, `SUM`, `pi`); kept in lower case.
const QSet<QString>& reservedNamesInAnyCase() {
  static const QSet<QString> names = [] {
    QSet<QString> all;
    for (const char* name :
         {"sin",     "cos",     "tan",     "asin",    "acos",   "atan",    "atan2",     "sinh",     "cosh",
          "tanh",    "asinh",   "acosh",   "atanh",   "sqrt",   "abs",     "sign",      "exp",      "ln",
          "log",     "floor",   "ceil",    "round",   "min",    "max",     "pow",       "if",       "and",
          "or",      "not",     "ceiling", "roundup", "mround", "trunc",   "rounddown", "int",      "mod",
          "even",    "odd",     "sum",     "average", "product", "median", "clamp",     "quotient", "ifs",
          "switch",  "choose",  "iferror", "let",     "value",  "radians", "degrees",   "pi",       "random",
          "gravity", "speedoflight",
          // Table lookups (mitcad#667).
          "xlookup", "match", "index"}) {
      all.insert(QString::fromLatin1(name));
    }
    return all;
  }();
  return names;
}

// ASCII letters in lower case, others as they are, as the model compares
// names in any case.
QString asciiLower(const QString& name) {
  QString lower = name;
  for (QChar& c : lower) {
    if (c.unicode() < 128) {
      c = c.toLower();
    }
  }
  return lower;
}

// The first character before `at` that is not white space, or a null
// character.
QChar previousSolid(const QString& text, int at) {
  for (int i = at - 1; i >= 0; --i) {
    if (!text.at(i).isSpace()) {
      return text.at(i);
    }
  }
  return QChar();
}

// Where the first character from `at` on that is not white space is, or
// the text's length.
int nextSolid(const QString& text, int at) {
  int i = at;
  while (i < text.size() && text.at(i).isSpace()) {
    ++i;
  }
  return i;
}

QString article(ParameterKind kind) {
  switch (kind) {
  case ParameterKind::Angle:
    return Text::tr("An angle");
  case ParameterKind::Unitless:
    return Text::tr("A number without a unit");
  case ParameterKind::Text:
    return Text::tr("A text");
  case ParameterKind::Length:
    break;
  }
  return Text::tr("A length");
}

} // namespace

std::vector<SuggestedParameter> suggestedParameters(const QJsonArray& query) {
  std::vector<SuggestedParameter> parameters;
  parameters.reserve(static_cast<std::size_t>(query.size()));
  for (const QJsonValue& value : query) {
    const QJsonObject p = value.toObject();
    SuggestedParameter parameter;
    parameter.name = p.value(QStringLiteral("name")).toString();
    if (parameter.name.isEmpty()) {
      continue;
    }
    parameter.expression = p.value(QStringLiteral("expression")).toString();
    parameter.text = p.value(QStringLiteral("text")).toString();
    parameter.unit = p.value(QStringLiteral("unit")).toString();
    parameter.comment = p.value(QStringLiteral("comment")).toString();
    parameter.owner = p.value(QStringLiteral("owner")).toString();
    for (const QJsonValue& name : p.value(QStringLiteral("dependencies")).toArray()) {
      parameter.dependencies << name.toString();
    }
    const QString kind = p.value(QStringLiteral("kind")).toString();
    parameter.group = kind == QStringLiteral("user") ? SuggestedParameter::Group::User : SuggestedParameter::Group::Model;
    parameter.reference = kind == QStringLiteral("reference");
    parameter.kind = kindOfUnit(parameter.unit);
    parameters.push_back(std::move(parameter));
  }
  return parameters;
}

ParameterKind kindOfUnit(const QString& unit) {
  const QString u = unit.trimmed();
  if (u.isEmpty()) {
    return ParameterKind::Unitless;
  }
  if (u.compare(QStringLiteral("Text"), Qt::CaseInsensitive) == 0) {
    return ParameterKind::Text;
  }
  if (u == QStringLiteral("rad") || u == QStringLiteral("deg") || u == QStringLiteral("grad") ||
      u == QStringLiteral("°")) {
    return ParameterKind::Angle;
  }
  return ParameterKind::Length;
}

QString parameterKindName(ParameterKind kind) {
  switch (kind) {
  case ParameterKind::Angle:
    return Text::tr("angle");
  case ParameterKind::Unitless:
    return Text::tr("number");
  case ParameterKind::Text:
    return Text::tr("text");
  case ParameterKind::Length:
    break;
  }
  return Text::tr("length");
}

bool isReservedName(const QString& name) {
  return reservedNames().contains(name) || reservedNamesInAnyCase().contains(asciiLower(name));
}

bool isValidParameterName(const QString& name) {
  if (name.isEmpty() || !isNameStart(name.front())) {
    return false;
  }
  return std::all_of(name.begin(), name.end(), isNameChar) && !isReservedName(name);
}

NameFragment nameFragmentAt(const QString& text, int caret) {
  NameFragment fragment;
  const int at = std::clamp(caret, 0, static_cast<int>(text.size()));
  int start = at;
  while (start > 0 && isNameChar(text.at(start - 1))) {
    --start;
  }
  int end = at;
  while (end < text.size() && isNameChar(text.at(end))) {
    ++end;
  }
  fragment.start = start;
  fragment.length = end - start;
  fragment.typed = text.mid(start, at - start);
  if (fragment.length == 0 || !isNameStart(text.at(start))) {
    return fragment; // nothing, or a number ("12", "2e3", "12mm")
  }
  const QChar before = previousSolid(text, start);
  const int next = nextSolid(text, end);
  const QChar after = next < text.size() ? text.at(next) : QChar();
  // `name = …` at the start: the field's own parameter's name.
  if (before.isNull() && after == QLatin1Char('=') &&
      (next + 1 >= text.size() || text.at(next + 1) != QLatin1Char('='))) {
    fragment.assigned = true;
    return fragment;
  }
  // A unit: right after a number, a name or `)` ("30 mm", "d1 mm",
  // "(a + b) in"); a decimal point belongs to a number.
  if (!before.isNull() && (isNameChar(before) || before == QLatin1Char(')') || before == QLatin1Char('.'))) {
    return fragment;
  }
  // A function's name.
  if (after == QLatin1Char('(')) {
    return fragment;
  }
  fragment.active = !fragment.typed.isEmpty();
  return fragment;
}

Insertion insertParameter(const QString& text, int caret, int selectionStart, int selectionLength,
                          const QString& name) {
  Insertion result;
  if (selectionStart >= 0 && selectionLength > 0 && selectionStart + selectionLength <= text.size()) {
    result.from = selectionStart;
    result.length = selectionLength;
  } else {
    const NameFragment fragment = nameFragmentAt(text, caret);
    if (fragment.active) {
      result.from = fragment.start;
      result.length = fragment.length;
    } else {
      result.from = std::clamp(caret, 0, static_cast<int>(text.size()));
      result.length = 0;
    }
  }
  result.text = text.left(result.from) + name + text.mid(result.from + result.length);
  result.caret = result.from + static_cast<int>(name.size());
  return result;
}

namespace {

QRegularExpressionMatch namedMatch(const QString& text) {
  static const QRegularExpression named(QStringLiteral(R"(^\s*(\w+)\s*=(?!=)(.*)$)"),
                                        QRegularExpression::UseUnicodePropertiesOption |
                                            QRegularExpression::DotMatchesEverythingOption);
  return named.match(text);
}

} // namespace

QString expressionPart(const QString& text) {
  const QRegularExpressionMatch match = namedMatch(text);
  return match.hasMatch() ? match.captured(2).trimmed() : text.trimmed();
}

int expressionStart(const QString& text) {
  const QRegularExpressionMatch match = namedMatch(text);
  int start = match.hasMatch() ? static_cast<int>(match.capturedStart(2)) : 0;
  const int size = static_cast<int>(text.size());
  while (start < size && text.at(start).isSpace()) {
    ++start;
  }
  return start;
}

QString namedPrefix(const QString& text) {
  const QRegularExpressionMatch match = namedMatch(text);
  return match.hasMatch() ? match.captured(1) : QString();
}

bool isAutomaticName(const QString& name) {
  if (name.size() < 2 || name.at(0) != QLatin1Char('d') || name.at(1) == QLatin1Char('0')) {
    return false;
  }
  for (int i = 1; i < name.size(); ++i) {
    if (name.at(i) < QLatin1Char('0') || name.at(i) > QLatin1Char('9')) {
      return false;
    }
  }
  return true;
}

QString kindProblem(ParameterKind field, ParameterKind parameter) {
  // A number's field takes a unitless parameter; a text and a number
  // never stand for each other (mitcad#664).
  const bool text = field == ParameterKind::Text || parameter == ParameterKind::Text;
  if (parameter == field || (parameter == ParameterKind::Unitless && !text)) {
    return QString();
  }
  return Text::tr("%1: this field takes a %2").arg(article(parameter), parameterKindName(field));
}

QStringList dependentsOf(const std::vector<SuggestedParameter>& parameters, const QString& name) {
  QHash<QString, QStringList> users;
  for (const SuggestedParameter& p : parameters) {
    for (const QString& used : p.dependencies) {
      users[used] << p.name;
    }
  }
  QStringList found;
  QSet<QString> seen{name};
  QStringList queue{name};
  while (!queue.isEmpty()) {
    const QString current = queue.takeFirst();
    for (const QString& user : users.value(current)) {
      if (!seen.contains(user)) {
        seen.insert(user);
        found << user;
        queue << user;
      }
    }
  }
  return found;
}

bool naturalLess(const QString& a, const QString& b) {
  int i = 0;
  int j = 0;
  while (i < a.size() && j < b.size()) {
    if (a.at(i).isDigit() && b.at(j).isDigit()) {
      int ie = i;
      while (ie < a.size() && a.at(ie).isDigit()) {
        ++ie;
      }
      int je = j;
      while (je < b.size() && b.at(je).isDigit()) {
        ++je;
      }
      // Without leading zeros, the shorter run of digits is the smaller
      // number.
      int is = i;
      while (is + 1 < ie && a.at(is) == QLatin1Char('0')) {
        ++is;
      }
      int js = j;
      while (js + 1 < je && b.at(js) == QLatin1Char('0')) {
        ++js;
      }
      const QString na = a.mid(is, ie - is);
      const QString nb = b.mid(js, je - js);
      if (na.size() != nb.size()) {
        return na.size() < nb.size();
      }
      if (na != nb) {
        return na < nb;
      }
      i = ie;
      j = je;
      continue;
    }
    const QChar ca = a.at(i).toLower();
    const QChar cb = b.at(j).toLower();
    if (ca != cb) {
      return ca < cb;
    }
    ++i;
    ++j;
  }
  if ((a.size() - i) != (b.size() - j)) {
    return (a.size() - i) < (b.size() - j);
  }
  return a < b; // the same but for case: a stable order
}

std::vector<ParameterChoice> matchParameters(const std::vector<SuggestedParameter>& parameters,
                                             const QString& filter, bool comments,
                                             const SuggestContext& context,
                                             const std::function<QString(const QString&)>& insertedText) {
  const QString wanted = filter.trimmed();
  // A kept own parameter (mitcad#1535) makes no cycle.
  const QStringList cycle =
      context.own.isEmpty() || context.keepOwn ? QStringList() : dependentsOf(parameters, context.own);
  struct Ranked {
    ParameterChoice choice;
    int rank = 0;
  };
  std::vector<Ranked> ranked;
  for (const SuggestedParameter& p : parameters) {
    if (!context.own.isEmpty() && p.name == context.own) {
      continue;
    }
    int rank = 0;
    if (!wanted.isEmpty()) {
      if (p.name.compare(wanted, Qt::CaseInsensitive) == 0) {
        rank = 0;
      } else if (p.name.startsWith(wanted, Qt::CaseInsensitive)) {
        rank = 1;
      } else if (p.name.contains(wanted, Qt::CaseInsensitive)) {
        rank = 2;
      } else if (comments && p.comment.contains(wanted, Qt::CaseInsensitive)) {
        rank = 3;
      } else {
        continue;
      }
    }
    ParameterChoice choice;
    choice.parameter = &p;
    if (cycle.contains(p.name)) {
      choice.problem = Text::tr("%1 uses %2, this field's own parameter: a cycle").arg(p.name, context.own);
    } else if (p.reference && !context.sketch.isEmpty() && p.owner == context.sketch) {
      choice.problem = Text::tr("Measured by a driven dimension of this sketch: the sketch cannot use it");
    } else if (!context.anyKind) {
      const QString after = insertedText ? insertedText(p.name) : p.name;
      if (expressionPart(after) == p.name) {
        choice.problem = kindProblem(context.kind, p.kind);
      }
    }
    ranked.push_back({choice, rank});
  }
  std::stable_sort(ranked.begin(), ranked.end(), [](const Ranked& a, const Ranked& b) {
    const auto ga = static_cast<int>(a.choice.parameter->group);
    const auto gb = static_cast<int>(b.choice.parameter->group);
    if (ga != gb) {
      return ga < gb;
    }
    if (a.rank != b.rank) {
      return a.rank < b.rank;
    }
    return naturalLess(a.choice.parameter->name, b.choice.parameter->name);
  });
  std::vector<ParameterChoice> choices;
  choices.reserve(ranked.size());
  for (const Ranked& r : ranked) {
    choices.push_back(r.choice);
  }
  return choices;
}

QString newNameProblem(const std::vector<SuggestedParameter>& parameters, const QString& name,
                       const SuggestedParameter** existing) {
  if (existing != nullptr) {
    *existing = nullptr;
  }
  if (name.isEmpty()) {
    return Text::tr("Give the parameter a name.");
  }
  for (const SuggestedParameter& p : parameters) {
    if (p.name == name) {
      if (existing != nullptr) {
        *existing = &p;
      }
      return Text::tr("%1 exists already (%2): use it, or give another name.")
          .arg(name, p.text.isEmpty() ? p.expression : p.text);
    }
  }
  if (isReservedName(name)) {
    return Text::tr("%1 is a unit, a function or a constant of expressions: give another name.").arg(name);
  }
  if (!isValidParameterName(name)) {
    return Text::tr("%1 is not a valid name: use letters, digits and _, starting with a letter.").arg(name);
  }
  return QString();
}

QString numberWithUnit(const QString& expression, const QString& unit) {
  QString number;
  if (!parsePlainNumber(expression, &number)) {
    return expression;
  }
  return unit.trimmed().isEmpty() ? number : number + QLatin1Char(' ') + unit.trimmed();
}

std::vector<SuggestedTable> suggestedTables(const QJsonArray& tables) {
  std::vector<SuggestedTable> result;
  result.reserve(static_cast<std::size_t>(tables.size()));
  for (const QJsonValue& value : tables) {
    const QJsonObject t = value.toObject();
    SuggestedTable table;
    table.name = t.value(QStringLiteral("name")).toString();
    if (table.name.isEmpty()) {
      continue;
    }
    table.comment = t.value(QStringLiteral("comment")).toString();
    for (const QJsonValue& c : t.value(QStringLiteral("columns")).toArray()) {
      const QJsonObject column = c.toObject();
      table.columns.push_back(
          {column.value(QStringLiteral("name")).toString(), column.value(QStringLiteral("unit")).toString()});
    }
    result.push_back(std::move(table));
  }
  return result;
}

ColumnFragment columnFragmentAt(const QString& text, int caret) {
  ColumnFragment fragment;
  const int at = std::clamp(caret, 0, static_cast<int>(text.size()));
  int start = at;
  while (start > 0 && isNameChar(text.at(start - 1))) {
    --start;
  }
  int end = at;
  while (end < text.size() && isNameChar(text.at(end))) {
    ++end;
  }
  if (start == 0 || text.at(start - 1) != QLatin1Char('.')) {
    return fragment;
  }
  if (end > start && !isNameStart(text.at(start))) {
    return fragment; // "Screws.5"
  }
  const int dot = start - 1;
  int tableStart = dot;
  while (tableStart > 0 && isNameChar(text.at(tableStart - 1))) {
    --tableStart;
  }
  if (tableStart == dot || !isNameStart(text.at(tableStart))) {
    return fragment; // ".5", "2.5"
  }
  if (tableStart > 0 && text.at(tableStart - 1) == QLatin1Char('.')) {
    return fragment; // a chain: "a.b.c"
  }
  fragment.active = true;
  fragment.table = text.mid(tableStart, dot - tableStart);
  fragment.start = start;
  fragment.length = end - start;
  fragment.typed = text.mid(start, at - start);
  return fragment;
}

const SuggestedTable* tableNamed(const std::vector<SuggestedTable>& tables, const QString& name) {
  for (const SuggestedTable& table : tables) {
    if (table.name == name) {
      return &table;
    }
  }
  return nullptr;
}

std::vector<SuggestedTable::Column> matchColumns(const SuggestedTable& table, const QString& filter) {
  const QString wanted = filter.trimmed();
  std::vector<SuggestedTable::Column> starting;
  std::vector<SuggestedTable::Column> containing;
  for (const SuggestedTable::Column& column : table.columns) {
    if (wanted.isEmpty() || column.name.startsWith(wanted, Qt::CaseInsensitive)) {
      starting.push_back(column);
    } else if (column.name.contains(wanted, Qt::CaseInsensitive)) {
      containing.push_back(column);
    }
  }
  starting.insert(starting.end(), containing.begin(), containing.end());
  return starting;
}

std::vector<const SuggestedTable*> matchTables(const std::vector<SuggestedTable>& tables, const QString& filter,
                                               bool comments) {
  const QString wanted = filter.trimmed();
  std::vector<const SuggestedTable*> starting;
  std::vector<const SuggestedTable*> containing;
  for (const SuggestedTable& table : tables) {
    if (wanted.isEmpty() || table.name.startsWith(wanted, Qt::CaseInsensitive)) {
      starting.push_back(&table);
    } else if (table.name.contains(wanted, Qt::CaseInsensitive) ||
               (comments && table.comment.contains(wanted, Qt::CaseInsensitive))) {
      containing.push_back(&table);
    }
  }
  starting.insert(starting.end(), containing.begin(), containing.end());
  return starting;
}

QString columnLabel(const SuggestedTable::Column& column) {
  return QStringLiteral("%1 [%2]").arg(column.name, column.unit);
}

} // namespace mitcad
