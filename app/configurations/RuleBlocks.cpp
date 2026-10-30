// SPDX-License-Identifier: MIT
#include "RuleBlocks.hpp"

#include <algorithm>
#include <utility>

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonValue>
#include <QSet>

namespace mitcad::rules {

namespace {

// The expression language's tokens, as far as the editor needs them to find
// the blocks (core/expr's grammar): names, numbers, texts and operators,
// each with where it is in the text.
enum class Tok { Name, Number, Text, Op, Bad };

struct Token {
  Tok type = Tok::Bad;
  int start = 0;
  int end = 0;
  QString text; // Text: without its quotes
};

bool nameStart(QChar c) {
  return c.isLetter() || c == QLatin1Char('_') || c == QChar(0x00B0); // ° is a unit
}

bool namePart(QChar c) {
  return c.isLetterOrNumber() || c == QLatin1Char('_');
}

std::vector<Token> tokenize(const QString& s) {
  std::vector<Token> out;
  const int n = static_cast<int>(s.size());
  int i = 0;
  while (i < n) {
    const QChar c = s[i];
    if (c.isSpace()) {
      ++i;
      continue;
    }
    const int start = i;
    if (c.isDigit() || (c == QLatin1Char('.') && i + 1 < n && s[i + 1].isDigit())) {
      while (i < n && s[i].isDigit()) {
        ++i;
      }
      // A decimal point, or a comma between two digits (a decimal comma).
      if (i < n && (s[i] == QLatin1Char('.') ||
                    (s[i] == QLatin1Char(',') && i > start && i + 1 < n && s[i + 1].isDigit()))) {
        ++i;
        while (i < n && s[i].isDigit()) {
          ++i;
        }
      }
      if (i < n && (s[i] == QLatin1Char('e') || s[i] == QLatin1Char('E'))) {
        int j = i + 1;
        if (j < n && (s[j] == QLatin1Char('+') || s[j] == QLatin1Char('-'))) {
          ++j;
        }
        if (j < n && s[j].isDigit()) {
          i = j;
          while (i < n && s[i].isDigit()) {
            ++i;
          }
        }
      }
      out.push_back({Tok::Number, start, i, s.mid(start, i - start)});
      continue;
    }
    if (nameStart(c)) {
      ++i;
      while (i < n && namePart(s[i])) {
        ++i;
      }
      out.push_back({Tok::Name, start, i, s.mid(start, i - start)});
      continue;
    }
    if (c == QLatin1Char('\'') || c == QLatin1Char('"')) {
      const QChar quote = c;
      QString value;
      bool closed = false;
      ++i;
      while (i < n) {
        if (s[i] == quote) {
          if (i + 1 < n && s[i + 1] == quote) {
            value += quote;
            i += 2;
            continue;
          }
          ++i;
          closed = true;
          break;
        }
        value += s[i];
        ++i;
      }
      out.push_back({closed ? Tok::Text : Tok::Bad, start, i, value});
      continue;
    }
    static const char* const pairs[] = {"==", "<>", "!=", "<=", ">="};
    bool paired = false;
    if (i + 1 < n) {
      const QString two = s.mid(i, 2);
      for (const char* pair : pairs) {
        if (two == QLatin1String(pair)) {
          out.push_back({Tok::Op, start, i + 2, two});
          i += 2;
          paired = true;
          break;
        }
      }
    }
    if (!paired) {
      out.push_back({Tok::Op, start, i + 1, QString(c)});
      ++i;
    }
  }
  return out;
}

bool isCompareText(const QString& op) {
  static const QStringList ops{QStringLiteral("=="), QStringLiteral("="),  QStringLiteral("<>"), QStringLiteral("!="),
                               QStringLiteral("<"),  QStringLiteral("<="), QStringLiteral(">"),  QStringLiteral(">=")};
  return ops.contains(op);
}

struct Parser {
  const QString& s;
  std::vector<Token> t;

  QString span(int b, int e) const { return s.mid(t[b].start, t[e - 1].end - t[b].start); }
  bool isOp(int i, const char* op) const {
    return i >= 0 && i < static_cast<int>(t.size()) && t[i].type == Tok::Op && t[i].text == QLatin1String(op);
  }
  bool isCompare(int i) const { return t[i].type == Tok::Op && isCompareText(t[i].text); }
  // The ")" that closes the "(" at `from`, before `e`; -1 when none does.
  int closing(int from, int e) const {
    int depth = 0;
    for (int i = from; i < e; ++i) {
      if (isOp(i, "(")) {
        ++depth;
      } else if (isOp(i, ")")) {
        if (--depth == 0) {
          return i;
        }
      }
    }
    return -1;
  }
  Block formula(int b, int e) const {
    Block f;
    f.kind = BlockKind::Formula;
    f.text = span(b, e);
    f.source = f.text;
    return f;
  }
  Block leaf(BlockKind kind, const QString& text, int b, int e) const {
    Block x;
    x.kind = kind;
    x.text = text;
    x.source = span(b, e);
    return x;
  }

  // A condition or a value: one comparison at the top, else an operand.
  Block parse(int b, int e) const {
    if (b >= e) {
      return {};
    }
    int depth = 0;
    std::vector<int> compares;
    for (int i = b; i < e; ++i) {
      if (isOp(i, "(")) {
        ++depth;
      } else if (isOp(i, ")")) {
        --depth;
      } else if (depth == 0 && isCompare(i)) {
        compares.push_back(i);
      }
    }
    if (compares.empty()) {
      return operand(b, e);
    }
    const int k = compares.front();
    if (compares.size() == 1 && k > b && k < e - 1) {
      Block compare;
      compare.kind = BlockKind::Compare;
      compare.text = t[k].text;
      compare.children.push_back(operand(b, k));
      compare.children.push_back(operand(k + 1, e));
      compare.source = span(b, e);
      return compare;
    }
    return formula(b, e);
  }

  Block operand(int b, int e) const {
    const int n = e - b;
    if (n == 1) {
      switch (t[b].type) {
      case Tok::Name:
        return leaf(BlockKind::Name, t[b].text, b, e);
      case Tok::Number:
        return leaf(BlockKind::Number, t[b].text, b, e);
      case Tok::Text:
        return leaf(BlockKind::Text, t[b].text, b, e);
      case Tok::Op:
      case Tok::Bad:
        break;
      }
      return formula(b, e);
    }
    // A number with a sign, a unit and the unit's power: `-5 mm`, `10 mm^2`.
    int i = b;
    if (isOp(i, "-") || isOp(i, "+")) {
      ++i;
    }
    if (i < e && t[i].type == Tok::Number) {
      int j = i + 1;
      if (j < e && t[j].type == Tok::Name && isUnit(t[j].text)) {
        ++j;
        if (j < e && isOp(j, "^")) {
          ++j;
          if (j < e && (isOp(j, "-") || isOp(j, "+"))) {
            ++j;
          }
          j = j < e && t[j].type == Tok::Number ? j + 1 : -1;
        }
      }
      if (j == e && (j > i + 1 || i > b)) {
        return leaf(BlockKind::Number, span(b, e), b, e);
      }
    }
    // A table column: `Colors.Name`, written without spaces.
    if (n == 3 && t[b].type == Tok::Name && isOp(b + 1, ".") && t[b + 2].type == Tok::Name &&
        t[b + 1].start == t[b].end && t[b + 2].start == t[b + 1].end) {
      return leaf(BlockKind::Column, span(b, e), b, e);
    }
    // Parentheses around everything: the block inside, which prints them as
    // long as it is not changed.
    if (isOp(b, "(") && closing(b, e) == e - 1) {
      Block inner = parse(b + 1, e - 1);
      if (inner.kind == BlockKind::Empty) {
        return formula(b, e);
      }
      inner.source = span(b, e);
      return inner;
    }
    if (n >= 3 && t[b].type == Tok::Name && isOp(b + 1, "(") && closing(b + 1, e) == e - 1) {
      const QString function = t[b].text.toLower();
      if (function == QLatin1String("and") || function == QLatin1String("or") || function == QLatin1String("not")) {
        std::vector<std::pair<int, int>> arguments;
        int depth = 0;
        int start = b + 2;
        for (int k = b + 2; k < e - 1; ++k) {
          if (isOp(k, "(")) {
            ++depth;
          } else if (isOp(k, ")")) {
            --depth;
          } else if (depth == 0 && (isOp(k, ";") || isOp(k, ","))) {
            arguments.emplace_back(start, k);
            start = k + 1;
          }
        }
        arguments.emplace_back(start, e - 1);
        std::vector<Block> children;
        for (const auto& [from, to] : arguments) {
          if (from >= to) {
            return formula(b, e);
          }
          children.push_back(parse(from, to));
        }
        Block logic;
        logic.text = t[b].text;
        logic.source = span(b, e);
        if (function == QLatin1String("not")) {
          if (children.size() != 1) {
            return formula(b, e);
          }
          logic.kind = BlockKind::Not;
        } else if (function == QLatin1String("or") && isOneOf(children)) {
          logic.kind = BlockKind::OneOf;
          logic.text.clear();
          std::vector<Block> values{children.front().children.front()};
          for (const Block& compare : children) {
            values.push_back(compare.children.back());
          }
          children = std::move(values);
        } else {
          logic.kind = function == QLatin1String("and") ? BlockKind::And : BlockKind::Or;
        }
        logic.children = std::move(children);
        return logic;
      }
    }
    return formula(b, e);
  }

  // `or(x == a; x == b; …)`: the same name compared with a value each time.
  static bool isOneOf(const std::vector<Block>& children) {
    if (children.size() < 2) {
      return false;
    }
    const QString* name = nullptr;
    for (const Block& compare : children) {
      if (compare.kind != BlockKind::Compare ||
          (compare.text != QLatin1String("==") && compare.text != QLatin1String("="))) {
        return false;
      }
      const Block& left = compare.children.front();
      const Block& right = compare.children.back();
      if (left.kind != BlockKind::Name || (name != nullptr && left.text != *name)) {
        return false;
      }
      if (right.kind != BlockKind::Number && right.kind != BlockKind::Text && right.kind != BlockKind::Name &&
          right.kind != BlockKind::Column) {
        return false;
      }
      name = &left.text;
    }
    return true;
  }
};

bool hasTopLevelCompare(const QString& text) {
  const std::vector<Token> tokens = tokenize(text);
  int depth = 0;
  for (const Token& token : tokens) {
    if (token.type != Tok::Op) {
      continue;
    }
    if (token.text == QLatin1String("(")) {
      ++depth;
    } else if (token.text == QLatin1String(")")) {
      --depth;
    } else if (depth == 0 && isCompareText(token.text)) {
      return true;
    }
  }
  return false;
}

bool wrapped(const QString& text) {
  if (!text.startsWith(QLatin1Char('(')) || !text.endsWith(QLatin1Char(')'))) {
    return false;
  }
  // The first "(" closes at the end, not earlier: `(a) + (b)` is not wrapped.
  int depth = 0;
  for (int i = 0; i < text.size(); ++i) {
    if (text[i] == QLatin1Char('(')) {
      ++depth;
    } else if (text[i] == QLatin1Char(')') && --depth == 0) {
      return i == text.size() - 1;
    }
  }
  return false;
}

// An operand of a comparison: a comparison in it needs parentheses.
QString operandText(const Block& block) {
  const QString text = printExpression(block);
  const bool compares = block.kind == BlockKind::Compare ||
                        (block.kind == BlockKind::Formula && hasTopLevelCompare(text));
  return compares && !wrapped(text) ? QStringLiteral("(%1)").arg(text) : text;
}

QString quoted(const QString& text) {
  QString escaped = text;
  escaped.replace(QLatin1Char('\''), QStringLiteral("''"));
  return QStringLiteral("'%1'").arg(escaped);
}

QString joined(const std::vector<Block>& blocks, const QString& separator) {
  QStringList parts;
  for (const Block& block : blocks) {
    parts << printExpression(block);
  }
  return parts.join(separator);
}

const char* actionKey(BlockKind kind) {
  switch (kind) {
  case BlockKind::AllowOnly:
    return "values";
  case BlockKind::Exclude:
    return "exclude";
  case BlockKind::Min:
    return "min";
  case BlockKind::Max:
    return "max";
  case BlockKind::Step:
    return "step";
  default:
    return nullptr;
  }
}

bool isOptionAction(BlockKind kind) {
  return actionKey(kind) != nullptr;
}

bool onlyKeys(const QJsonObject& object, const QSet<QString>& keys) {
  for (auto it = object.begin(); it != object.end(); ++it) {
    if (!keys.contains(it.key())) {
      return false;
    }
  }
  return true;
}

bool allStrings(const QJsonValue& value) {
  if (!value.isArray()) {
    return false;
  }
  const QJsonArray array = value.toArray();
  for (const QJsonValue& item : array) {
    if (!item.isString()) {
      return false;
    }
  }
  return true;
}

Block otherAction(const QJsonValue& value) {
  Block other;
  other.kind = BlockKind::OtherAction;
  other.data = value.isObject() ? value.toObject() : QJsonObject{{QStringLiteral("__raw"), value}};
  return other;
}

std::vector<Block> actionsFromJson(const QJsonArray& array, int& group) {
  std::vector<Block> actions;
  for (const QJsonValue& value : array) {
    if (!value.isObject()) {
      actions.push_back(otherAction(value));
      continue;
    }
    const QJsonObject object = value.toObject();
    const int id = group++;
    if (object.contains(QStringLiteral("require"))) {
      const QJsonValue message = object.value(QStringLiteral("message"));
      if (onlyKeys(object, {QStringLiteral("require"), QStringLiteral("message")}) &&
          object.value(QStringLiteral("require")).isString() && (message.isUndefined() || message.isString())) {
        Block require;
        require.kind = BlockKind::Require;
        require.children.push_back(parseExpression(object.value(QStringLiteral("require")).toString()));
        require.message = message.toString();
        if (!message.isUndefined()) {
          require.data.insert(QStringLiteral("message"), true);
        }
        require.group = id;
        actions.push_back(require);
        continue;
      }
    } else if (object.contains(QStringLiteral("forbid"))) {
      if (onlyKeys(object, {QStringLiteral("forbid")}) && object.value(QStringLiteral("forbid")).isString()) {
        Block forbid;
        forbid.kind = BlockKind::Forbid;
        forbid.message = object.value(QStringLiteral("forbid")).toString();
        forbid.group = id;
        actions.push_back(forbid);
        continue;
      }
    } else if (object.contains(QStringLiteral("in_table"))) {
      const QJsonValue columns = object.value(QStringLiteral("columns"));
      if (onlyKeys(object, {QStringLiteral("in_table"), QStringLiteral("columns")}) &&
          object.value(QStringLiteral("in_table")).isString() && (columns.isUndefined() || columns.isObject())) {
        Block table;
        table.kind = BlockKind::InTable;
        table.text = object.value(QStringLiteral("in_table")).toString();
        table.data = columns.toObject();
        table.group = id;
        actions.push_back(table);
        continue;
      }
    } else if (object.value(QStringLiteral("option")).isString() &&
               onlyKeys(object, {QStringLiteral("option"), QStringLiteral("values"), QStringLiteral("exclude"),
                                 QStringLiteral("min"), QStringLiteral("max"), QStringLiteral("step")})) {
      const QString option = object.value(QStringLiteral("option")).toString();
      std::vector<Block> parts;
      bool readable = true;
      for (const BlockKind kind :
           {BlockKind::AllowOnly, BlockKind::Exclude, BlockKind::Min, BlockKind::Max, BlockKind::Step}) {
        const QString key = QLatin1String(actionKey(kind));
        if (!object.contains(key)) {
          continue;
        }
        Block part;
        part.kind = kind;
        part.text = option;
        part.group = id;
        const QJsonValue item = object.value(key);
        if (kind == BlockKind::AllowOnly || kind == BlockKind::Exclude) {
          if (!allStrings(item)) {
            readable = false;
            break;
          }
          const QJsonArray values = item.toArray();
          for (const QJsonValue& entry : values) {
            part.children.push_back(parseExpression(entry.toString()));
          }
        } else {
          if (!item.isString()) {
            readable = false;
            break;
          }
          part.children.push_back(parseExpression(item.toString()));
        }
        parts.push_back(std::move(part));
      }
      if (readable && !parts.empty()) {
        for (Block& part : parts) {
          actions.push_back(std::move(part));
        }
        continue;
      }
    }
    Block other = otherAction(object);
    other.group = id;
    actions.push_back(other);
  }
  return actions;
}

QJsonValue singleActionJson(const Block& action) {
  switch (action.kind) {
  case BlockKind::Require: {
    QJsonObject object{{QStringLiteral("require"), printExpression(action.children.front())}};
    if (!action.message.isEmpty() || action.data.contains(QStringLiteral("message"))) {
      object.insert(QStringLiteral("message"), action.message);
    }
    return object;
  }
  case BlockKind::Forbid:
    return QJsonObject{{QStringLiteral("forbid"), action.message}};
  case BlockKind::InTable: {
    QJsonObject object{{QStringLiteral("in_table"), action.text}};
    if (!action.data.isEmpty()) {
      object.insert(QStringLiteral("columns"), action.data);
    }
    return object;
  }
  case BlockKind::OtherAction:
    if (action.data.size() == 1 && action.data.contains(QStringLiteral("__raw"))) {
      return action.data.value(QStringLiteral("__raw"));
    }
    return action.data;
  default:
    return QJsonObject{};
  }
}

QJsonArray actionsToJson(const std::vector<Block>& actions) {
  QJsonArray out;
  QJsonObject pending;
  int pendingGroup = -1;
  bool started = false;
  const auto flush = [&out, &pending, &started] {
    if (started) {
      out.append(pending);
    }
    pending = QJsonObject();
    started = false;
  };
  for (const Block& action : actions) {
    if (!isOptionAction(action.kind)) {
      flush();
      out.append(singleActionJson(action));
      continue;
    }
    const QString key = QLatin1String(actionKey(action.kind));
    QJsonValue value;
    if (action.kind == BlockKind::AllowOnly || action.kind == BlockKind::Exclude) {
      QJsonArray values;
      for (const Block& child : action.children) {
        if (child.kind != BlockKind::Empty) {
          values.append(printExpression(child));
        }
      }
      value = values;
    } else {
      value = action.children.empty() ? QString() : printExpression(action.children.front());
    }
    if (started && action.group >= 0 && action.group == pendingGroup &&
        pending.value(QStringLiteral("option")).toString() == action.text && !pending.contains(key)) {
      pending.insert(key, value);
      continue;
    }
    flush();
    pending = QJsonObject{{QStringLiteral("option"), action.text}, {key, value}};
    pendingGroup = action.group;
    started = true;
  }
  flush();
  return out;
}

QString sectionName(int section) {
  switch (section) {
  case 0:
    return QStringLiteral("when");
  case 1:
    return QStringLiteral("then");
  default:
    return QStringLiteral("else");
  }
}

std::vector<Block>* sectionList(Rule& rule, int section) {
  if (section == 1) {
    return &rule.then;
  }
  if (section == 2 && rule.hasElse) {
    return &rule.otherwise;
  }
  return nullptr;
}

BlockPath parentOf(const BlockPath& path) {
  BlockPath parent = path;
  if (!parent.indices.empty()) {
    parent.indices.pop_back();
  }
  return parent;
}

bool isPrefix(const std::vector<int>& prefix, const std::vector<int>& path) {
  if (prefix.size() > path.size()) {
    return false;
  }
  for (std::size_t i = 0; i < prefix.size(); ++i) {
    if (prefix[i] != path[i]) {
      return false;
    }
  }
  return true;
}

} // namespace

bool isAction(BlockKind kind) {
  switch (kind) {
  case BlockKind::AllowOnly:
  case BlockKind::Exclude:
  case BlockKind::Min:
  case BlockKind::Max:
  case BlockKind::Step:
  case BlockKind::Require:
  case BlockKind::Forbid:
  case BlockKind::InTable:
  case BlockKind::OtherAction:
    return true;
  default:
    return false;
  }
}

bool isValue(BlockKind kind) {
  return kind != BlockKind::Empty && !isAction(kind);
}

QString groupOf(BlockKind kind) {
  switch (kind) {
  case BlockKind::Compare:
  case BlockKind::And:
  case BlockKind::Or:
  case BlockKind::Not:
  case BlockKind::OneOf:
    return QStringLiteral("Logic");
  default:
    return isAction(kind) ? QStringLiteral("Actions") : QStringLiteral("Values");
  }
}

QString kindLabel(BlockKind kind) {
  switch (kind) {
  case BlockKind::Empty:
    return QStringLiteral("empty");
  case BlockKind::Name:
    return QStringLiteral("name");
  case BlockKind::Number:
    return QStringLiteral("number");
  case BlockKind::Text:
    return QStringLiteral("text");
  case BlockKind::Column:
    return QStringLiteral("table column");
  case BlockKind::Formula:
    return QStringLiteral("formula");
  case BlockKind::Compare:
    return QStringLiteral("compare");
  case BlockKind::And:
    return QStringLiteral("and");
  case BlockKind::Or:
    return QStringLiteral("or");
  case BlockKind::Not:
    return QStringLiteral("not");
  case BlockKind::OneOf:
    return QStringLiteral("is one of");
  case BlockKind::AllowOnly:
    return QStringLiteral("allow only");
  case BlockKind::Exclude:
    return QStringLiteral("exclude");
  case BlockKind::Min:
    return QStringLiteral("min");
  case BlockKind::Max:
    return QStringLiteral("max");
  case BlockKind::Step:
    return QStringLiteral("step");
  case BlockKind::Require:
    return QStringLiteral("require");
  case BlockKind::Forbid:
    return QStringLiteral("forbid");
  case BlockKind::InTable:
    return QStringLiteral("combination in table");
  case BlockKind::OtherAction:
    return QStringLiteral("action");
  }
  return {};
}

Block parseExpression(const QString& text) {
  const Parser parser{text, tokenize(text)};
  if (parser.t.empty()) {
    return {};
  }
  Block block = parser.parse(0, static_cast<int>(parser.t.size()));
  // The whole text, with any spaces around it, is the source of the top.
  block.source = text;
  return block;
}

QString printExpression(const Block& block) {
  if (!block.source.isEmpty()) {
    return block.source;
  }
  switch (block.kind) {
  case BlockKind::Name:
  case BlockKind::Number:
  case BlockKind::Column:
  case BlockKind::Formula:
    return block.text;
  case BlockKind::Text:
    return quoted(block.text);
  case BlockKind::Compare:
    if (block.children.size() != 2) {
      return {};
    }
    return QStringLiteral("%1 %2 %3")
        .arg(operandText(block.children.front()), block.text, operandText(block.children.back()));
  case BlockKind::And:
  case BlockKind::Or:
  case BlockKind::Not: {
    QString name = block.text.toLower();
    if (name.isEmpty()) {
      name = block.kind == BlockKind::And ? QStringLiteral("and")
             : block.kind == BlockKind::Or ? QStringLiteral("or")
                                            : QStringLiteral("not");
    }
    return QStringLiteral("%1(%2)").arg(name, joined(block.children, QStringLiteral("; ")));
  }
  case BlockKind::OneOf: {
    if (block.children.empty()) {
      return {};
    }
    const QString subject = printExpression(block.children.front());
    QStringList parts;
    for (std::size_t i = 1; i < block.children.size(); ++i) {
      parts << QStringLiteral("%1 == %2").arg(subject, operandText(block.children[i]));
    }
    return QStringLiteral("or(%1)").arg(parts.join(QStringLiteral("; ")));
  }
  default:
    return {};
  }
}

bool isComplete(const Block& block) {
  const auto all = [&block] {
    for (const Block& child : block.children) {
      if (!isComplete(child)) {
        return false;
      }
    }
    return true;
  };
  switch (block.kind) {
  case BlockKind::Empty:
    return false;
  case BlockKind::Name:
  case BlockKind::Number:
  case BlockKind::Column:
  case BlockKind::Formula:
    return !block.text.trimmed().isEmpty();
  case BlockKind::Text:
  case BlockKind::Forbid:
  case BlockKind::OtherAction:
    return true;
  case BlockKind::Compare:
    return block.children.size() == 2 && all();
  case BlockKind::And:
  case BlockKind::Or:
    return !block.children.empty() && all();
  case BlockKind::Not:
  case BlockKind::Require:
    return block.children.size() == 1 && all();
  case BlockKind::OneOf:
    return block.children.size() >= 2 && all();
  case BlockKind::AllowOnly:
  case BlockKind::Exclude:
    return !block.text.isEmpty() && !block.children.empty() && all();
  case BlockKind::Min:
  case BlockKind::Max:
  case BlockKind::Step:
    return !block.text.isEmpty() && block.children.size() == 1 && all();
  case BlockKind::InTable:
    return !block.text.isEmpty();
  }
  return false;
}

bool isUnit(const QString& name) {
  static const QStringList units{
      QStringLiteral("um"),   QStringLiteral("micron"), QStringLiteral("µm"), QStringLiteral("mm"),
      QStringLiteral("cm"),   QStringLiteral("m"),      QStringLiteral("km"),      QStringLiteral("in"),
      QStringLiteral("inch"), QStringLiteral("ft"),     QStringLiteral("foot"),    QStringLiteral("yd"),
      QStringLiteral("mi"),   QStringLiteral("mil"),    QStringLiteral("rad"),     QStringLiteral("deg"),
      QStringLiteral("°"), QStringLiteral("grad")};
  return units.contains(name);
}

QStringList compareOperators() {
  return {QStringLiteral("=="), QStringLiteral("<>"), QStringLiteral("<"),
          QStringLiteral("<="), QStringLiteral(">"),  QStringLiteral(">=")};
}

QString operatorLabel(const QString& op) {
  if (op == QLatin1String("==") || op == QLatin1String("=")) {
    return QStringLiteral("=");
  }
  if (op == QLatin1String("<>") || op == QLatin1String("!=")) {
    return QStringLiteral("≠");
  }
  if (op == QLatin1String("<=")) {
    return QStringLiteral("≤");
  }
  if (op == QLatin1String(">=")) {
    return QStringLiteral("≥");
  }
  return op;
}

Rule ruleFromJson(const QJsonObject& json) {
  Rule rule;
  rule.name = json.value(QStringLiteral("name")).toString();
  rule.enabledWritten = json.contains(QStringLiteral("enabled"));
  rule.enabled = json.value(QStringLiteral("enabled")).toBool(true);
  int group = 0;
  if (json.contains(QStringLiteral("when"))) {
    rule.hasWhen = true;
    rule.when = parseExpression(json.value(QStringLiteral("when")).toString());
  }
  rule.then = actionsFromJson(json.value(QStringLiteral("then")).toArray(), group);
  if (json.contains(QStringLiteral("else"))) {
    rule.hasElse = true;
    rule.otherwise = actionsFromJson(json.value(QStringLiteral("else")).toArray(), group);
  }
  rule.extra = json;
  for (const char* key : {"name", "enabled", "when", "then", "else"}) {
    rule.extra.remove(QLatin1String(key));
  }
  return rule;
}

QJsonObject ruleToJson(const Rule& rule, bool* complete) {
  QJsonObject json = rule.extra;
  json.insert(QStringLiteral("name"), rule.name);
  if (rule.enabledWritten || !rule.enabled) {
    json.insert(QStringLiteral("enabled"), rule.enabled);
  }
  if (rule.hasWhen && rule.when.kind != BlockKind::Empty) {
    json.insert(QStringLiteral("when"), printExpression(rule.when));
  }
  json.insert(QStringLiteral("then"), actionsToJson(rule.then));
  if (rule.hasElse) {
    json.insert(QStringLiteral("else"), actionsToJson(rule.otherwise));
  }
  if (complete != nullptr) {
    *complete = isComplete(rule);
  }
  return json;
}

bool isComplete(const Rule& rule) {
  // An empty else is not complete either: the model does not keep one.
  if (rule.name.trimmed().isEmpty() || (rule.hasWhen && !isComplete(rule.when)) ||
      (rule.hasElse && rule.otherwise.empty())) {
    return false;
  }
  for (const auto* list : {&rule.then, &rule.otherwise}) {
    for (const Block& action : *list) {
      if (!isComplete(action)) {
        return false;
      }
    }
  }
  return true;
}

QString actionText(const Block& action) {
  const auto value = [](const Block& block) {
    const QString text = printExpression(block);
    return text.trimmed().isEmpty() ? QStringLiteral("…") : text.trimmed();
  };
  const auto values = [&action, &value] {
    QStringList parts;
    for (const Block& child : action.children) {
      parts << value(child);
    }
    return parts.isEmpty() ? QStringLiteral("…") : parts.join(QStringLiteral("; "));
  };
  const QString option = action.text.isEmpty() ? QStringLiteral("…") : action.text;
  switch (action.kind) {
  case BlockKind::AllowOnly:
    return QStringLiteral("%1 only %2").arg(option, values());
  case BlockKind::Exclude:
    return QStringLiteral("%1 not %2").arg(option, values());
  case BlockKind::Min:
  case BlockKind::Max:
  case BlockKind::Step:
    return QStringLiteral("%1 %2 %3")
        .arg(option, kindLabel(action.kind), action.children.empty() ? QStringLiteral("…") : value(action.children.front()));
  case BlockKind::Require: {
    const QString condition = action.children.empty() ? QStringLiteral("…") : value(action.children.front());
    return action.message.isEmpty() ? QStringLiteral("require %1").arg(condition)
                                    : QStringLiteral("require %1: %2").arg(condition, action.message);
  }
  case BlockKind::Forbid:
    return QStringLiteral("forbid: %1").arg(action.message);
  case BlockKind::InTable: {
    QStringList columns;
    for (auto it = action.data.begin(); it != action.data.end(); ++it) {
      columns << QStringLiteral("%1 = %2").arg(it.key(), it.value().toString());
    }
    return columns.isEmpty() ? QStringLiteral("combination in table %1").arg(option)
                             : QStringLiteral("combination in table %1 (%2)")
                                   .arg(option, columns.join(QStringLiteral(", ")));
  }
  case BlockKind::OtherAction:
    return QString::fromUtf8(QJsonDocument(action.data).toJson(QJsonDocument::Compact));
  default:
    return value(action);
  }
}

QJsonObject inTableColumns(const QStringList& tableColumns, const QStringList& options, const QJsonObject& current) {
  QJsonObject columns;
  for (auto it = current.begin(); it != current.end(); ++it) {
    if (options.contains(it.key()) && tableColumns.contains(it.value().toString())) {
      columns.insert(it.key(), it.value());
    }
  }
  for (const QString& option : options) {
    if (columns.contains(option)) {
      continue;
    }
    if (tableColumns.contains(option)) {
      columns.insert(option, option);
      continue;
    }
    for (const QString& column : tableColumns) {
      if (column.compare(option, Qt::CaseInsensitive) == 0) {
        columns.insert(option, column);
        break;
      }
    }
  }
  return columns;
}

QStringList ruleText(const Rule& rule) {
  QStringList lines;
  if (rule.hasWhen) {
    const QString condition = printExpression(rule.when).trimmed();
    lines << QStringLiteral("when %1").arg(condition.isEmpty() ? QStringLiteral("…") : condition);
  } else {
    lines << QStringLiteral("always");
  }
  if (rule.then.empty()) {
    lines << QStringLiteral("then nothing");
  }
  for (const Block& action : rule.then) {
    lines << QStringLiteral("then %1").arg(actionText(action));
  }
  if (rule.hasElse && rule.otherwise.empty()) {
    lines << QStringLiteral("else nothing");
  }
  for (const Block& action : rule.otherwise) {
    lines << QStringLiteral("else %1").arg(actionText(action));
  }
  return lines;
}

QString BlockPath::id() const {
  QString id = sectionName(section);
  for (const int index : indices) {
    id += QStringLiteral(".%1").arg(index);
  }
  return id;
}

Block* blockAt(Rule& rule, const BlockPath& path) {
  Block* block = nullptr;
  std::size_t next = 0;
  if (path.section == 0) {
    if (!rule.hasWhen) {
      return nullptr;
    }
    block = &rule.when;
  } else {
    std::vector<Block>* list = sectionList(rule, path.section);
    if (list == nullptr || path.indices.empty() || path.indices.front() < 0 ||
        path.indices.front() >= static_cast<int>(list->size())) {
      return nullptr;
    }
    block = &(*list)[static_cast<std::size_t>(path.indices.front())];
    next = 1;
  }
  for (; next < path.indices.size(); ++next) {
    const int index = path.indices[next];
    if (index < 0 || index >= static_cast<int>(block->children.size())) {
      return nullptr;
    }
    block = &block->children[static_cast<std::size_t>(index)];
  }
  return block;
}

const Block* blockAt(const Rule& rule, const BlockPath& path) {
  return blockAt(const_cast<Rule&>(rule), path);
}

std::vector<Block>* listOf(Rule& rule, const BlockPath& path) {
  if (path.indices.empty()) {
    return nullptr;
  }
  if (path.section != 0 && path.indices.size() == 1) {
    return sectionList(rule, path.section);
  }
  Block* parent = blockAt(rule, parentOf(path));
  return parent != nullptr ? &parent->children : nullptr;
}

bool growsAtEnd(BlockKind kind) {
  switch (kind) {
  case BlockKind::And:
  case BlockKind::Or:
  case BlockKind::OneOf:
  case BlockKind::AllowOnly:
  case BlockKind::Exclude:
    return true;
  default:
    return false;
  }
}

bool removable(BlockKind parent, int index) {
  return growsAtEnd(parent) && !(parent == BlockKind::OneOf && index == 0);
}

bool fits(const Rule& rule, const Target& target, BlockKind kind) {
  const BlockPath& path = target.path;
  if (path.section == 0 && path.indices.empty()) {
    return !target.insert && rule.hasWhen && isValue(kind);
  }
  if (path.indices.empty()) {
    return false;
  }
  const int index = path.indices.back();
  if (path.section != 0 && path.indices.size() == 1) {
    const std::vector<Block>* list = sectionList(const_cast<Rule&>(rule), path.section);
    return list != nullptr && target.insert && isAction(kind) && index >= 0 &&
           index <= static_cast<int>(list->size());
  }
  const Block* parent = blockAt(rule, parentOf(path));
  if (parent == nullptr || !isValue(kind)) {
    return false;
  }
  const int size = static_cast<int>(parent->children.size());
  if (target.insert) {
    const int first = parent->kind == BlockKind::OneOf ? 1 : 0;
    return growsAtEnd(parent->kind) && index >= first && index <= size;
  }
  return index >= 0 && index < size;
}

void touch(Rule& rule, const BlockPath& path) {
  BlockPath prefix = path;
  for (;;) {
    if (Block* block = blockAt(rule, prefix)) {
      block->source.clear();
    }
    if (prefix.indices.empty() || (prefix.section != 0 && prefix.indices.size() == 1)) {
      return;
    }
    prefix.indices.pop_back();
  }
}

bool place(Rule& rule, const Target& target, Block block) {
  if (!fits(rule, target, block.kind)) {
    return false;
  }
  const BlockPath& path = target.path;
  if (path.section == 0 && path.indices.empty()) {
    rule.when = std::move(block);
    return true;
  }
  std::vector<Block>* list = listOf(rule, path);
  if (list == nullptr) {
    return false;
  }
  const auto index = static_cast<std::size_t>(path.indices.back());
  if (target.insert) {
    list->insert(list->begin() + static_cast<std::ptrdiff_t>(index), std::move(block));
  } else {
    (*list)[index] = std::move(block);
  }
  if (path.indices.size() > 1 || path.section == 0) {
    touch(rule, parentOf(path));
  }
  return true;
}

Block take(Rule& rule, const BlockPath& path) {
  if (path.section == 0 && path.indices.empty()) {
    Block taken = std::move(rule.when);
    rule.when = Block();
    return taken;
  }
  std::vector<Block>* list = listOf(rule, path);
  if (list == nullptr || path.indices.back() < 0 || path.indices.back() >= static_cast<int>(list->size())) {
    return {};
  }
  const auto index = static_cast<std::size_t>(path.indices.back());
  Block taken = std::move((*list)[index]);
  const bool inSection = path.section != 0 && path.indices.size() == 1;
  const Block* parent = inSection ? nullptr : blockAt(rule, parentOf(path));
  if (inSection || (parent != nullptr && removable(parent->kind, path.indices.back()))) {
    list->erase(list->begin() + static_cast<std::ptrdiff_t>(index));
  } else {
    (*list)[index] = Block();
  }
  if (!inSection) {
    touch(rule, parentOf(path));
  }
  return taken;
}

bool move(Rule& rule, const BlockPath& from, const Target& to) {
  const Block* block = blockAt(rule, from);
  if (block == nullptr || block->kind == BlockKind::Empty) {
    return false;
  }
  if (to.path.section == from.section && isPrefix(from.indices, to.path.indices)) {
    return false; // into itself
  }
  if (!fits(rule, to, block->kind)) {
    return false;
  }
  // Whether taking it out removes it from a list (the places after it move
  // up by one) or leaves its socket empty.
  bool erases = false;
  if (!from.indices.empty()) {
    if (from.section != 0 && from.indices.size() == 1) {
      erases = true;
    } else if (const Block* parent = blockAt(rule, parentOf(from))) {
      erases = removable(parent->kind, from.indices.back());
    }
  }
  Rule changed = rule;
  Block moved = take(changed, from);
  Target adjusted = to;
  if (erases && to.path.section == from.section && from.indices.size() <= to.path.indices.size()) {
    const std::size_t level = from.indices.size() - 1;
    bool samePrefix = true;
    for (std::size_t i = 0; i < level; ++i) {
      samePrefix = samePrefix && from.indices[i] == to.path.indices[i];
    }
    if (samePrefix && to.path.indices[level] > from.indices[level]) {
      --adjusted.path.indices[level];
    }
  }
  if (!place(changed, adjusted, std::move(moved))) {
    return false;
  }
  rule = std::move(changed);
  return true;
}

Block newBlock(const QString& paletteId, const QString& option) {
  Block block;
  const auto empties = [&block](int count) { block.children.assign(static_cast<std::size_t>(count), Block()); };
  if (paletteId == QLatin1String("compare")) {
    block.kind = BlockKind::Compare;
    block.text = QStringLiteral("==");
    empties(2);
  } else if (paletteId == QLatin1String("and") || paletteId == QLatin1String("or")) {
    block.kind = paletteId == QLatin1String("and") ? BlockKind::And : BlockKind::Or;
    block.text = paletteId;
    empties(2);
  } else if (paletteId == QLatin1String("not")) {
    block.kind = BlockKind::Not;
    block.text = paletteId;
    empties(1);
  } else if (paletteId == QLatin1String("one_of")) {
    block.kind = BlockKind::OneOf;
    empties(2);
  } else if (paletteId == QLatin1String("option") || paletteId == QLatin1String("parameter")) {
    block.kind = BlockKind::Name;
    block.text = option;
  } else if (paletteId == QLatin1String("number")) {
    block.kind = BlockKind::Number;
  } else if (paletteId == QLatin1String("text")) {
    block.kind = BlockKind::Text;
  } else if (paletteId == QLatin1String("column")) {
    block.kind = BlockKind::Column;
  } else if (paletteId == QLatin1String("formula")) {
    block.kind = BlockKind::Formula;
  } else if (paletteId == QLatin1String("allow_only") || paletteId == QLatin1String("exclude")) {
    block.kind = paletteId == QLatin1String("exclude") ? BlockKind::Exclude : BlockKind::AllowOnly;
    block.text = option;
    empties(1);
  } else if (paletteId == QLatin1String("min") || paletteId == QLatin1String("max") ||
             paletteId == QLatin1String("step")) {
    block.kind = paletteId == QLatin1String("min")   ? BlockKind::Min
                 : paletteId == QLatin1String("max") ? BlockKind::Max
                                                      : BlockKind::Step;
    block.text = option;
    empties(1);
  } else if (paletteId == QLatin1String("require")) {
    block.kind = BlockKind::Require;
    empties(1);
  } else if (paletteId == QLatin1String("forbid")) {
    block.kind = BlockKind::Forbid;
  } else if (paletteId == QLatin1String("in_table")) {
    block.kind = BlockKind::InTable;
  }
  return block;
}

std::vector<PaletteEntry> paletteEntries() {
  const QString rule = QStringLiteral("Rule");
  const QString logic = QStringLiteral("Logic");
  const QString values = QStringLiteral("Values");
  const QString actions = QStringLiteral("Actions");
  // Each tip says what the block does, with an example in the rule's text
  // form (mitcad#967).
  return {
      {QStringLiteral("when"), QStringLiteral("when … then"), rule,
       QStringLiteral("Gives the rule a condition: its actions apply only while it holds. Example: when Color == "
                      "'Black'.")},
      {QStringLiteral("always"), QStringLiteral("then (always)"), rule,
       QStringLiteral("Takes the condition away: the rule's actions always apply. Example: always then Length max "
                      "2000 mm.")},
      {QStringLiteral("else"), QStringLiteral("else"), rule,
       QStringLiteral("Adds actions for when the condition does not hold. Example: else Length min 500 mm.")},
      {QStringLiteral("and"), QStringLiteral("and"), logic,
       QStringLiteral("True when all of its conditions hold. Example: and(Color == 'Red'; Size == 'XL').")},
      {QStringLiteral("or"), QStringLiteral("or"), logic,
       QStringLiteral("True when at least one of its conditions holds. Example: or(Size == 'XL'; Length > 1500 "
                      "mm).")},
      {QStringLiteral("not"), QStringLiteral("not"), logic,
       QStringLiteral("True when its condition does not hold. Example: not(Tables == 0).")},
      {QStringLiteral("compare"), QStringLiteral("compare"), logic,
       QStringLiteral("Compares two values: equal, not equal, less or greater. Example: Length >= 1000 mm.")},
      {QStringLiteral("one_of"), QStringLiteral("is one of"), logic,
       QStringLiteral("True when the option has one of the listed values. Example: Color is one of 'Red', "
                      "'Black'.")},
      {QStringLiteral("option"), QStringLiteral("option"), values,
       QStringLiteral("An option of this design, by its name. Example: Color.")},
      {QStringLiteral("parameter"), QStringLiteral("parameter"), values,
       QStringLiteral("Any parameter of the design, also one that is no option. Example: Wall.")},
      {QStringLiteral("number"), QStringLiteral("number with unit"), values,
       QStringLiteral("A number with its unit; a length or an angle needs one. Example: 1000 mm.")},
      {QStringLiteral("text"), QStringLiteral("text"), values,
       QStringLiteral("A text, such as a value of a text option. Example: Black, written 'Black' in the text.")},
      {QStringLiteral("column"), QStringLiteral("table column"), values,
       QStringLiteral("All the values of a column of one of the design's tables. Example: Screws.Size.")},
      {QStringLiteral("formula"), QStringLiteral("formula"), values,
       QStringLiteral("Any expression, written in the Formula Editor. Example: Length / 4.")},
      {QStringLiteral("allow_only"), QStringLiteral("allow only"), actions,
       QStringLiteral("Allows an option only the listed values. Example: Power only 0.")},
      {QStringLiteral("exclude"), QStringLiteral("exclude"), actions,
       QStringLiteral("Takes the listed values away from an option. Example: Color not 'Red'.")},
      {QStringLiteral("min"), QStringLiteral("min"), actions,
       QStringLiteral("Limits a number option's smallest value. Example: Length min 300 mm.")},
      {QStringLiteral("max"), QStringLiteral("max"), actions,
       QStringLiteral("Limits an option's largest value. Example: Length max 1000 mm.")},
      {QStringLiteral("step"), QStringLiteral("step"), actions,
       QStringLiteral("Lets a number option change only in steps of this size. Example: Length step 10 mm.")},
      {QStringLiteral("require"), QStringLiteral("require"), actions,
       QStringLiteral("The combination is allowed only while this holds; otherwise the message shows. Example: "
                      "require Depth >= Length / 4: too shallow.")},
      {QStringLiteral("forbid"), QStringLiteral("forbid"), actions,
       QStringLiteral("Forbids the combination while the rule applies, with a message. Example: forbid: Red is "
                      "not made in XL.")},
      {QStringLiteral("in_table"), QStringLiteral("combination in table"), actions,
       QStringLiteral("Allows only the combinations that are rows of a table. Example: combination in table "
                      "Made.")},
  };
}

bool isRuleEntry(const QString& id) {
  return id == QLatin1String("when") || id == QLatin1String("always") || id == QLatin1String("else");
}

bool applyRuleEntry(Rule& rule, const QString& id) {
  if (id == QLatin1String("when")) {
    if (rule.hasWhen) {
      return false;
    }
    rule.hasWhen = true;
    rule.when = Block();
    return true;
  }
  if (id == QLatin1String("always")) {
    if (!rule.hasWhen) {
      return false;
    }
    rule.hasWhen = false;
    rule.when = Block();
    return true;
  }
  if (id == QLatin1String("else")) {
    if (rule.hasElse) {
      return false;
    }
    rule.hasElse = true;
    return true;
  }
  return false;
}

// Rules written as text (mitcad#965) and the rules Add makes (mitcad#966).

namespace {

const OptionInfo* findOption(const std::vector<OptionInfo>& options, const QString& name) {
  for (const OptionInfo& option : options) {
    if (option.name == name) {
      return &option;
    }
  }
  return nullptr;
}

QStringList optionNames(const std::vector<OptionInfo>& options) {
  QStringList names;
  for (const OptionInfo& option : options) {
    names << option.name;
  }
  return names;
}

// A block read from text prints canonically: `=` as `==`, `!=` as `<>`,
// `AND` as `and`.
void canonicalize(Block& block) {
  block.source.clear();
  if (block.kind == BlockKind::Compare) {
    if (block.text == QLatin1String("=")) {
      block.text = QStringLiteral("==");
    } else if (block.text == QLatin1String("!=")) {
      block.text = QStringLiteral("<>");
    }
  } else if (block.kind == BlockKind::And || block.kind == BlockKind::Or || block.kind == BlockKind::Not) {
    block.text = block.text.toLower();
  }
  for (Block& child : block.children) {
    canonicalize(child);
  }
}

Block canonicalExpression(const QString& text) {
  Block block = parseExpression(text);
  canonicalize(block);
  return block;
}

// A mistake in an expression's text that its blocks would hide in a
// formula block: a text that is not closed, a parenthesis without its pair,
// the `…` of an empty place. The index in the text (-1 when none) and why.
int expressionProblem(const QString& text, QString* reason) {
  std::vector<int> opens;
  for (const Token& token : tokenize(text)) {
    if (token.type == Tok::Bad) {
      *reason = QStringLiteral("a text is not closed: end it with %1").arg(text.at(token.start));
      return token.start;
    }
    if (token.type != Tok::Op) {
      continue;
    }
    if (token.text == QLatin1String("(")) {
      opens.push_back(token.start);
    } else if (token.text == QLatin1String(")")) {
      if (opens.empty()) {
        *reason = QStringLiteral("this ) has no ( before it");
        return token.start;
      }
      opens.pop_back();
    } else if (token.text == QStringLiteral("…")) {
      *reason = QStringLiteral("a value is missing here");
      return token.start;
    }
  }
  if (!opens.empty()) {
    *reason = QStringLiteral("this ( is not closed");
    return opens.back();
  }
  return -1;
}

// The parts of a text between its `;` that are not in parentheses: each
// part's start and end.
std::vector<std::pair<int, int>> topLevelParts(const QString& text) {
  std::vector<std::pair<int, int>> parts;
  int depth = 0;
  int start = 0;
  for (const Token& token : tokenize(text)) {
    if (token.type != Tok::Op) {
      continue;
    }
    if (token.text == QLatin1String("(")) {
      ++depth;
    } else if (token.text == QLatin1String(")")) {
      --depth;
    } else if (depth == 0 && token.text == QLatin1String(";")) {
      parts.emplace_back(start, token.start);
      start = token.end;
    }
  }
  parts.emplace_back(start, static_cast<int>(text.size()));
  return parts;
}

bool hasUnit(const QString& number) {
  for (const Token& token : tokenize(number)) {
    if (token.type == Tok::Name && isUnit(token.text)) {
      return true;
    }
  }
  return false;
}

// The index of a name in an expression's text, as a name of its own (not in
// a text); 0 when it is not found.
int nameIndex(const QString& text, const QString& name) {
  for (const Token& token : tokenize(text)) {
    if (token.type == Tok::Name && token.text == name) {
      return token.start;
    }
  }
  return 0;
}

// Reads the lines of a rule (ruleFromText).
class TextReader {
public:
  TextReader(const Rule& base, const std::vector<OptionInfo>& options, TextProblem* problem)
      : m_base(base), m_options(options), m_problem(problem), m_usedThen(base.then.size(), false),
        m_usedElse(base.otherwise.size(), false) {
    for (const auto* list : {&base.then, &base.otherwise}) {
      for (const Block& action : *list) {
        m_nextGroup = std::max(m_nextGroup, action.group + 1);
      }
    }
    m_firstNewGroup = m_nextGroup;
    m_rule.name = base.name;
    m_rule.enabled = base.enabled;
    m_rule.enabledWritten = base.enabledWritten;
    m_rule.extra = base.extra;
  }

  bool read(const QString& input) {
    QString text = input;
    text.replace(QChar(QChar::LineSeparator), QLatin1Char('\n'));
    text.replace(QChar(QChar::ParagraphSeparator), QLatin1Char('\n'));
    const QStringList lines = text.split(QLatin1Char('\n'));
    bool started = false; // a when, always, then or else line was read
    for (int i = 0; i < static_cast<int>(lines.size()); ++i) {
      m_number = i + 1;
      QString line = lines.at(i);
      if (line.endsWith(QLatin1Char('\r'))) {
        line.chop(1);
      }
      if (line.trimmed().isEmpty()) {
        continue;
      }
      const std::vector<Token> tokens = tokenize(line);
      const Token& first = tokens.front();
      const QString word = first.type == Tok::Name ? first.text.toLower() : QString();
      if (word == QLatin1String("when") || word == QLatin1String("always")) {
        if (started) {
          return fail(first.start, first.end - first.start,
                      QStringLiteral("the when or always line comes first, and only once"));
        }
        started = true;
        // `when … then <action>` (or `always then <action>`) on one line.
        std::size_t thenAt = 0;
        int depth = 0;
        for (std::size_t k = 1; k < tokens.size() && thenAt == 0; ++k) {
          const Token& token = tokens[k];
          if (token.type == Tok::Op && token.text == QLatin1String("(")) {
            ++depth;
          } else if (token.type == Tok::Op && token.text == QLatin1String(")")) {
            --depth;
          } else if (depth == 0 && token.type == Tok::Name && token.text.toLower() == QLatin1String("then")) {
            thenAt = k;
          }
        }
        const int end = thenAt > 0 ? tokens[thenAt].start : static_cast<int>(line.size());
        if (word == QLatin1String("always")) {
          if (tokens.size() > 1 && thenAt != 1) {
            return fail(tokens[1].start, tokens[1].end - tokens[1].start,
                        QStringLiteral("always has no condition: write when <condition>, or the actions on then "
                                       "lines"));
          }
          m_rule.hasWhen = false;
        } else if (!readWhen(line, first.end, end)) {
          return false;
        }
        if (thenAt > 0 && !readAction(line, tokens[thenAt].end, 1)) {
          return false;
        }
      } else if (word == QLatin1String("then") || word == QLatin1String("else")) {
        started = true;
        const int section = word == QLatin1String("then") ? 1 : 2;
        if (section == 2) {
          m_rule.hasElse = true;
        }
        if (!readAction(line, first.end, section)) {
          return false;
        }
      } else {
        return fail(first.start, first.end - first.start,
                    QStringLiteral("a line starts with when, always, then or else"));
      }
    }
    return true;
  }

  const Rule& rule() const { return m_rule; }

private:
  bool fail(int index, int length, const QString& reason) {
    if (m_problem != nullptr) {
      m_problem->line = m_number;
      m_problem->column = index + 1;
      m_problem->length = std::max(1, length);
      m_problem->reason = reason;
    }
    return false;
  }

  // The expression in line[from, to): read, canonical, checked.
  bool expression(const QString& line, int from, int to, const QString& missing, Block* out) {
    int start = from;
    while (start < to && line.at(start).isSpace()) {
      ++start;
    }
    const QString text = line.mid(start, to - start).trimmed();
    if (text.isEmpty()) {
      return fail(from, 1, missing);
    }
    QString reason;
    if (const int bad = expressionProblem(text, &reason); bad >= 0) {
      return fail(start + bad, 1, reason);
    }
    Block block = canonicalExpression(text);
    if (!checkTexts(block, text, start)) {
      return false;
    }
    *out = std::move(block);
    return true;
  }

  // A text option compared with one of its values written without quotes
  // (`Color == Black`): the value is a text.
  bool checkTexts(const Block& block, const QString& text, int start) {
    if ((block.kind == BlockKind::Compare || block.kind == BlockKind::OneOf) && !block.children.empty() &&
        block.children.front().kind == BlockKind::Name) {
      if (const OptionInfo* option = findOption(m_options, block.children.front().text);
          option != nullptr && option->text) {
        for (std::size_t i = 1; i < block.children.size(); ++i) {
          const Block& compared = block.children[i];
          if (compared.kind == BlockKind::Name && option->texts.contains(compared.text) &&
              findOption(m_options, compared.text) == nullptr) {
            return fail(start + nameIndex(text, compared.text), static_cast<int>(compared.text.size()),
                        QStringLiteral("%1 is a text: write it in quotes, '%1'").arg(compared.text));
          }
        }
      }
    }
    for (const Block& child : block.children) {
      if (!checkTexts(child, text, start)) {
        return false;
      }
    }
    return true;
  }

  // A value of an action on `option`, in line[from, to).
  bool value(const QString& line, int from, int to, const QString& missing, const OptionInfo* option, Block* out) {
    if (!expression(line, from, to, missing, out)) {
      return false;
    }
    if (option == nullptr) {
      return true;
    }
    int start = from;
    while (start < to && line.at(start).isSpace()) {
      ++start;
    }
    const int length = static_cast<int>(line.mid(start, to - start).trimmed().size());
    const Block& block = *out;
    if (option->text && block.kind == BlockKind::Name && option->texts.contains(block.text) &&
        findOption(m_options, block.text) == nullptr) {
      return fail(start, length, QStringLiteral("%1 is a text: write it in quotes, '%1'").arg(block.text));
    }
    if (option->text && block.kind == BlockKind::Number) {
      return fail(start, length, QStringLiteral("%1 is a text option: write its values in quotes").arg(option->name));
    }
    if (!option->text && block.kind == BlockKind::Text) {
      return fail(start, length,
                  QStringLiteral("%1 is a number option: '%2' is a text").arg(option->name, block.text));
    }
    if (!option->text && !option->unit.isEmpty() && block.kind == BlockKind::Number && !hasUnit(block.text)) {
      return fail(start, length,
                  QStringLiteral("%1 needs a unit: %2 is in %3 (%1 %3)").arg(block.text, option->name, option->unit));
    }
    return true;
  }

  bool readWhen(const QString& line, int from, int to) {
    m_rule.hasWhen = true;
    const QString condition = line.mid(from, to - from).trimmed();
    // Unchanged: the base's condition as it was written.
    if (m_base.hasWhen && !condition.isEmpty() && condition == printExpression(m_base.when).trimmed()) {
      m_rule.when = m_base.when;
      return true;
    }
    return expression(line, from, to, QStringLiteral("a condition is missing after when"), &m_rule.when);
  }

  bool readAction(const QString& line, int from, int section) {
    std::vector<Block>& list = section == 1 ? m_rule.then : m_rule.otherwise;
    const QString word = section == 1 ? QStringLiteral("then") : QStringLiteral("else");
    int lead = from;
    while (lead < static_cast<int>(line.size()) && line.at(lead).isSpace()) {
      ++lead;
    }
    const QString text = line.mid(lead).trimmed();
    if (text.isEmpty()) {
      return fail(from, 1, QStringLiteral("an action is missing after %1").arg(word));
    }
    // Unchanged: the base's block as it was (its text, its JSON object).
    const std::vector<Block>& baseList = section == 1 ? m_base.then : m_base.otherwise;
    std::vector<bool>& used = section == 1 ? m_usedThen : m_usedElse;
    for (std::size_t k = 0; k < baseList.size(); ++k) {
      if (!used[k] && actionText(baseList[k]) == text) {
        used[k] = true;
        list.push_back(baseList[k]);
        return true;
      }
    }
    if (text.compare(QLatin1String("nothing"), Qt::CaseInsensitive) == 0) {
      return true;
    }
    Block block;
    if (text.startsWith(QLatin1Char('{'))) {
      QJsonParseError error{};
      const QJsonDocument document = QJsonDocument::fromJson(text.toUtf8(), &error);
      if (!document.isObject()) {
        return fail(lead, static_cast<int>(text.size()),
                    QStringLiteral("this action is no JSON object: %1").arg(error.errorString()));
      }
      block = otherAction(document.object());
    } else if (!readKnownAction(line, lead, text, word, &block)) {
      return false;
    }
    // The option actions of one option written one after another are one
    // JSON object, as the model writes them (`{"option", "min", "max"}`).
    const bool joins = isOptionAction(block.kind) && !list.empty() && list.back().group >= m_firstNewGroup &&
                       isOptionAction(list.back().kind) && list.back().text == block.text &&
                       list.back().kind != block.kind;
    block.group = joins ? list.back().group : m_nextGroup++;
    list.push_back(std::move(block));
    return true;
  }

  bool readKnownAction(const QString& line, int lead, const QString& text, const QString& section, Block* out) {
    const std::vector<Token> tokens = tokenize(text);
    const Token& first = tokens.front();
    const QString word = first.type == Tok::Name ? first.text.toLower() : QString();
    Block& block = *out;
    if (word == QLatin1String("forbid")) {
      int at = first.end;
      if (tokens.size() > 1 && tokens[1].type == Tok::Op && tokens[1].text == QLatin1String(":")) {
        at = tokens[1].end;
      }
      block.kind = BlockKind::Forbid;
      block.message = text.mid(at).trimmed();
      return true;
    }
    if (word == QLatin1String("require")) {
      std::size_t colon = 0;
      int depth = 0;
      for (std::size_t k = 1; k < tokens.size() && colon == 0; ++k) {
        if (tokens[k].type != Tok::Op) {
          continue;
        }
        if (tokens[k].text == QLatin1String("(")) {
          ++depth;
        } else if (tokens[k].text == QLatin1String(")")) {
          --depth;
        } else if (depth == 0 && tokens[k].text == QLatin1String(":")) {
          colon = k;
        }
      }
      const int end = colon > 0 ? tokens[colon].start : static_cast<int>(text.size());
      Block condition;
      if (!expression(line, lead + first.end, lead + end, QStringLiteral("a condition is missing after require"),
                      &condition)) {
        return false;
      }
      block.kind = BlockKind::Require;
      block.children.push_back(std::move(condition));
      if (colon > 0) {
        block.message = text.mid(tokens[colon].end).trimmed();
        block.data.insert(QStringLiteral("message"), true);
      }
      return true;
    }
    if (word == QLatin1String("combination") || word == QLatin1String("in")) {
      const std::size_t in = word == QLatin1String("combination") ? 1 : 0;
      const auto isWord = [&tokens](std::size_t k, const char* expected) {
        return k < tokens.size() && tokens[k].type == Tok::Name &&
               tokens[k].text.toLower() == QLatin1String(expected);
      };
      if (!isWord(in, "in") || !isWord(in + 1, "table")) {
        return fail(lead, static_cast<int>(text.size()), QStringLiteral("write combination in table <table>"));
      }
      const std::size_t at = in + 2;
      if (at >= tokens.size() || (tokens[at].type != Tok::Name && tokens[at].type != Tok::Text)) {
        return fail(lead + static_cast<int>(text.size()), 1, QStringLiteral("the table's name is missing"));
      }
      block.kind = BlockKind::InTable;
      block.text = tokens[at].text;
      const int after = tokens[at].end;
      const QString columns = text.mid(after).trimmed();
      if (columns.isEmpty()) {
        return true;
      }
      const int columnsAt = lead + after + static_cast<int>(text.mid(after).indexOf(columns.at(0)));
      if (!columns.startsWith(QLatin1Char('(')) || !columns.endsWith(QLatin1Char(')'))) {
        return fail(columnsAt, static_cast<int>(columns.size()),
                    QStringLiteral("the columns are written in parentheses: (Color = Color, Length = Length)"));
      }
      const QStringList pairs = columns.mid(1, columns.size() - 2).split(QLatin1Char(','));
      for (const QString& pair : pairs) {
        if (pair.trimmed().isEmpty()) {
          continue;
        }
        const qsizetype equals = pair.indexOf(QLatin1Char('='));
        const QString option = equals > 0 ? pair.left(equals).trimmed() : QString();
        const QString column = equals > 0 ? pair.mid(equals + 1).trimmed() : QString();
        if (option.isEmpty() || column.isEmpty()) {
          return fail(columnsAt, static_cast<int>(columns.size()),
                      QStringLiteral("a column is written <option> = <column>: %1").arg(pair.trimmed()));
        }
        if (!m_options.empty() && findOption(m_options, option) == nullptr) {
          return fail(columnsAt, static_cast<int>(columns.size()), unknownOption(option));
        }
        block.data.insert(option, column);
      }
      return true;
    }
    // <option> <action> <values>
    const QString actions = QStringLiteral("write min, max, step, only, not, require, forbid or combination in table");
    if (first.type != Tok::Name) {
      return fail(lead + first.start, first.end - first.start,
                  QStringLiteral("an option or an action is missing after %1: %2").arg(section, actions));
    }
    const QString option = first.text;
    const OptionInfo* info = findOption(m_options, option);
    if (!m_options.empty() && info == nullptr) {
      return fail(lead + first.start, first.end - first.start, unknownOption(option));
    }
    if (tokens.size() < 2) {
      return fail(lead + first.end, 1, QStringLiteral("an action is missing after %1: %2").arg(option, actions));
    }
    const Token& verb = tokens[1];
    const QString name = verb.text.toLower();
    std::size_t valuesFrom = 2;
    BlockKind kind = BlockKind::Empty;
    if (verb.type == Tok::Name) {
      if (name == QLatin1String("allow") && tokens.size() > 2 && tokens[2].type == Tok::Name &&
          tokens[2].text.toLower() == QLatin1String("only")) {
        kind = BlockKind::AllowOnly;
        valuesFrom = 3;
      } else if (name == QLatin1String("only")) {
        kind = BlockKind::AllowOnly;
      } else if (name == QLatin1String("not") || name == QLatin1String("exclude")) {
        kind = BlockKind::Exclude;
      } else if (name == QLatin1String("min")) {
        kind = BlockKind::Min;
      } else if (name == QLatin1String("max")) {
        kind = BlockKind::Max;
      } else if (name == QLatin1String("step")) {
        kind = BlockKind::Step;
      }
    }
    if (kind == BlockKind::Empty) {
      return fail(lead + verb.start, verb.end - verb.start,
                  QStringLiteral("%1 is no action: %2").arg(verb.text, actions));
    }
    const bool range = kind == BlockKind::Min || kind == BlockKind::Max || kind == BlockKind::Step;
    if (range && info != nullptr && info->text) {
      return fail(lead + verb.start, verb.end - verb.start,
                  QStringLiteral("%1 is a text option: min, max and step are for numbers").arg(option));
    }
    const QString verbText = text.mid(verb.start, tokens[valuesFrom - 1].end - verb.start);
    const int valuesAt = valuesFrom < tokens.size() ? tokens[valuesFrom].start : static_cast<int>(text.size());
    block.kind = kind;
    block.text = option;
    const QString missing = QStringLiteral("a value is missing after %1").arg(verbText);
    if (range) {
      Block one;
      if (!value(line, lead + valuesAt, lead + static_cast<int>(text.size()), missing, info, &one)) {
        return false;
      }
      block.children.push_back(std::move(one));
      return true;
    }
    const QString values = text.mid(valuesAt);
    if (values.trimmed().isEmpty()) {
      return fail(lead + valuesAt, 1, missing);
    }
    for (const auto& [from, to] : topLevelParts(values)) {
      Block one;
      if (!value(line, lead + valuesAt + from, lead + valuesAt + to,
                 QStringLiteral("a value is missing between the ;"), info, &one)) {
        return false;
      }
      block.children.push_back(std::move(one));
    }
    return true;
  }

  QString unknownOption(const QString& name) const {
    return QStringLiteral("%1 is not an option of this design: the options are %2")
        .arg(name, optionNames(m_options).join(QStringLiteral(", ")));
  }

  const Rule& m_base;
  const std::vector<OptionInfo>& m_options;
  TextProblem* m_problem = nullptr;
  std::vector<bool> m_usedThen;
  std::vector<bool> m_usedElse;
  int m_nextGroup = 0;
  int m_firstNewGroup = 0;
  int m_number = 0;
  Rule m_rule;
};

// What a template makes: its rule's text and name, or why it cannot.
struct TemplatePlan {
  QStringList lines;
  QString name;
  QString why;
};

TemplatePlan planTemplate(const QString& id, const std::vector<OptionInfo>& options, const QString& table,
                          const QStringList& tableColumns) {
  TemplatePlan plan;
  const auto valued = [](const OptionInfo& option) { return !option.first.isEmpty(); };
  // The condition's option: the first text option, else the first other
  // option with a value, besides `other`.
  const auto conditionBesides = [&options, &valued](const OptionInfo* other) -> const OptionInfo* {
    for (const OptionInfo& option : options) {
      if (option.text && valued(option) && &option != other) {
        return &option;
      }
    }
    for (const OptionInfo& option : options) {
      if (valued(option) && &option != other) {
        return &option;
      }
    }
    return nullptr;
  };
  const auto otherThan = [&options, &valued](const OptionInfo* one) -> const OptionInfo* {
    for (const OptionInfo& option : options) {
      if (valued(option) && &option != one) {
        return &option;
      }
    }
    return nullptr;
  };
  const auto when = [](const OptionInfo& option) {
    return QStringLiteral("when %1 == %2").arg(option.name, option.first);
  };
  const QString noOptions = QStringLiteral("The design has no options yet (the Options tab)");
  if (id == QLatin1String("range")) {
    const OptionInfo* number = nullptr;
    for (const OptionInfo& option : options) {
      if (!option.text && (!option.max.isEmpty() || valued(option))) {
        number = &option;
        break;
      }
    }
    if (number == nullptr) {
      plan.why = QStringLiteral("The design has no number option: a range is a number's");
      return plan;
    }
    const OptionInfo* condition = conditionBesides(number);
    if (condition != nullptr) {
      plan.lines << when(*condition);
    }
    plan.lines << QStringLiteral("then %1 max %2").arg(number->name, number->max.isEmpty() ? number->first : number->max);
    plan.name = condition != nullptr
                    ? QStringLiteral("Limit %1 when %2 is %3").arg(number->name, condition->name, condition->firstText)
                    : QStringLiteral("Limit %1").arg(number->name);
  } else if (id == QLatin1String("allow")) {
    const OptionInfo* condition = conditionBesides(nullptr);
    if (condition == nullptr) {
      plan.why = noOptions;
      return plan;
    }
    if (const OptionInfo* target = otherThan(condition)) {
      plan.lines << when(*condition) << QStringLiteral("then %1 only %2").arg(target->name, target->first);
      plan.name = QStringLiteral("%1 only %2 when %3 is %4")
                      .arg(target->name, target->firstText, condition->name, condition->firstText);
    } else {
      plan.lines << QStringLiteral("then %1 only %2").arg(condition->name, condition->first);
      plan.name = QStringLiteral("%1 only %2").arg(condition->name, condition->firstText);
    }
  } else if (id == QLatin1String("forbid")) {
    const OptionInfo* a = conditionBesides(nullptr);
    if (a == nullptr) {
      plan.why = noOptions;
      return plan;
    }
    if (const OptionInfo* b = otherThan(a)) {
      plan.lines << QStringLiteral("when and(%1 == %2; %3 == %4)").arg(a->name, a->first, b->name, b->first)
                 << QStringLiteral("then forbid: %1 with %2 is not made").arg(a->firstText, b->firstText);
      plan.name = QStringLiteral("No %1 with %2").arg(a->firstText, b->firstText);
    } else {
      plan.lines << when(*a) << QStringLiteral("then forbid: %1 is not made").arg(a->firstText);
      plan.name = QStringLiteral("No %1").arg(a->firstText);
    }
  } else if (id == QLatin1String("table")) {
    if (table.isEmpty()) {
      plan.why = QStringLiteral("The design has no tables (Change Parameters, Tables)");
      return plan;
    }
    const QJsonObject columns = inTableColumns(tableColumns, optionNames(options), QJsonObject());
    QStringList pairs;
    for (auto it = columns.begin(); it != columns.end(); ++it) {
      pairs << QStringLiteral("%1 = %2").arg(it.key(), it.value().toString());
    }
    plan.lines << (pairs.isEmpty() ? QStringLiteral("then combination in table %1").arg(table)
                                   : QStringLiteral("then combination in table %1 (%2)")
                                         .arg(table, pairs.join(QStringLiteral(", "))));
    plan.name = QStringLiteral("Combinations in %1").arg(table);
  } else if (id == QLatin1String("empty")) {
    plan.name = QStringLiteral("Rule");
  } else {
    plan.why = QStringLiteral("No such template");
  }
  return plan;
}

} // namespace

QString TextProblem::text() const {
  return QStringLiteral("Line %1, column %2: %3").arg(line).arg(column).arg(reason);
}

std::vector<OptionInfo> optionInfos(const QJsonObject& configureAnswer) {
  std::vector<OptionInfo> infos;
  const QJsonArray options = configureAnswer.value(QStringLiteral("options")).toArray();
  for (const QJsonValue& entry : options) {
    const QJsonObject option = entry.toObject();
    OptionInfo info;
    info.name = option.value(QStringLiteral("name")).toString();
    if (info.name.isEmpty()) {
      continue;
    }
    info.text = option.value(QStringLiteral("kind")).toString() == QLatin1String("text");
    info.unit = info.text ? QString() : option.value(QStringLiteral("unit")).toString();
    const QJsonObject domain = option.value(QStringLiteral("domain")).toObject();
    const QString kind = domain.value(QStringLiteral("kind")).toString();
    if (kind == QLatin1String("list")) {
      double low = 0.0;
      double high = 0.0;
      bool any = false;
      const QJsonArray values = domain.value(QStringLiteral("values")).toArray();
      for (const QJsonValue& item : values) {
        const QJsonObject listed = item.toObject();
        const QString expression = listed.value(QStringLiteral("expression")).toString();
        const QString shown = listed.value(QStringLiteral("text")).toString();
        if (info.first.isEmpty()) {
          info.first = expression;
          info.firstText = shown.isEmpty() ? expression : shown;
        }
        info.texts << shown;
        const QJsonValue number = listed.value(QStringLiteral("value"));
        if (!info.text && number.isDouble()) {
          const double x = number.toDouble();
          if (!any || x < low) {
            low = x;
            info.min = expression;
          }
          if (!any || x > high) {
            high = x;
            info.max = expression;
          }
          any = true;
        }
      }
    } else if (kind == QLatin1String("range")) {
      info.min = domain.value(QStringLiteral("min_text")).toString();
      info.max = domain.value(QStringLiteral("max_text")).toString();
      info.step = domain.value(QStringLiteral("step_text")).toString();
      info.first = info.min;
      info.firstText = info.min;
    }
    if (info.first.isEmpty()) {
      info.first = option.value(QStringLiteral("expression")).toString();
      const QString shown = option.value(QStringLiteral("text")).toString();
      info.firstText = shown.isEmpty() ? info.first : shown;
    }
    infos.push_back(std::move(info));
  }
  return infos;
}

bool ruleFromText(const QString& text, const Rule& base, const std::vector<OptionInfo>& options, Rule* rule,
                  TextProblem* problem) {
  TextReader reader(base, options, problem);
  if (!reader.read(text)) {
    return false;
  }
  if (rule != nullptr) {
    *rule = reader.rule();
  }
  return true;
}

QStringList exampleRuleText(const std::vector<OptionInfo>& options) {
  const OptionInfo* text = nullptr;
  const OptionInfo* number = nullptr;
  for (const OptionInfo& option : options) {
    if (option.text && !option.first.isEmpty() && text == nullptr) {
      text = &option;
    } else if (!option.text && (!option.max.isEmpty() || !option.first.isEmpty()) && number == nullptr) {
      number = &option;
    }
  }
  return {text != nullptr ? QStringLiteral("when %1 == %2").arg(text->name, text->first)
                          : QStringLiteral("when Color == 'Black'"),
          number != nullptr
              ? QStringLiteral("then %1 max %2").arg(number->name, number->max.isEmpty() ? number->first : number->max)
              : QStringLiteral("then Length max 1000 mm")};
}

void fillSockets(Block& action, const std::vector<OptionInfo>& options) {
  QString value;
  const OptionInfo* option = findOption(options, action.text);
  switch (action.kind) {
  case BlockKind::Min:
  case BlockKind::Max:
  case BlockKind::Step:
    if (option != nullptr && !option->text) {
      if (action.kind == BlockKind::Min) {
        value = option->min.isEmpty() ? option->first : option->min;
      } else if (action.kind == BlockKind::Max) {
        value = option->max.isEmpty() ? option->first : option->max;
      } else {
        value = !option->step.isEmpty() ? option->step
                : option->unit.isEmpty() ? QStringLiteral("1")
                                         : QStringLiteral("1 %1").arg(option->unit);
      }
    }
    break;
  case BlockKind::AllowOnly:
  case BlockKind::Exclude:
    if (option != nullptr) {
      value = option->first;
    }
    break;
  case BlockKind::Require:
    for (const OptionInfo& first : options) {
      if (!first.first.isEmpty()) {
        value = QStringLiteral("%1 == %2").arg(first.name, first.first);
        break;
      }
    }
    break;
  default:
    break;
  }
  if (value.isEmpty()) {
    return;
  }
  for (Block& child : action.children) {
    if (child.kind == BlockKind::Empty) {
      child = canonicalExpression(value);
      return;
    }
  }
}

std::vector<RuleTemplate> ruleTemplates(const std::vector<OptionInfo>& options, const QString& table,
                                        const QStringList& tableColumns) {
  const std::pair<const char*, QString> entries[] = {
      {"range", QStringLiteral("&Limit a range when…")},
      {"allow", QStringLiteral("&Allow only some values when…")},
      {"forbid", QStringLiteral("&Forbid a combination…")},
      {"table", QStringLiteral("&Only the combinations of a table…")},
      {"empty", QStringLiteral("&Empty rule")},
  };
  std::vector<RuleTemplate> templates;
  for (const auto& [id, label] : entries) {
    const TemplatePlan plan = planTemplate(QLatin1String(id), options, table, tableColumns);
    RuleTemplate entry;
    entry.id = QLatin1String(id);
    entry.label = label;
    entry.enabled = plan.why.isEmpty();
    entry.why = plan.why;
    entry.lines = plan.lines;
    templates.push_back(std::move(entry));
  }
  return templates;
}

bool ruleFromTemplate(const QString& id, const std::vector<OptionInfo>& options, const QString& table,
                      const QStringList& tableColumns, Rule* rule) {
  const TemplatePlan plan = planTemplate(id, options, table, tableColumns);
  if (!plan.why.isEmpty()) {
    return false;
  }
  Rule base;
  base.name = plan.name;
  Rule made;
  if (!ruleFromText(plan.lines.join(QLatin1Char('\n')), base, options, &made, nullptr)) {
    return false;
  }
  if (rule != nullptr) {
    *rule = std::move(made);
  }
  return true;
}

} // namespace mitcad::rules
