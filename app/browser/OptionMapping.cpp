// SPDX-License-Identifier: MIT
// Text literals and option mappings of bindings (Bindings.hpp): plain Qt
// Core, so that the unit tests use them without the dialogs (mitcad#1059).
#include <algorithm>

#include <QStringList>

#include "Bindings.hpp"

namespace mitcad {
namespace {

// The arguments of a call's text between its parentheses, split at the
// semicolons outside quotes and parentheses.
QStringList splitArguments(const QString& inner) {
  QStringList parts;
  QString current;
  int depth = 0;
  QChar quote;
  for (const QChar c : inner) {
    if (!quote.isNull()) {
      current += c;
      if (c == quote) {
        quote = QChar(); // a doubled quote opens again at once
      }
      continue;
    }
    if (c == QLatin1Char('\'') || c == QLatin1Char('"')) {
      quote = c;
    } else if (c == QLatin1Char('(')) {
      ++depth;
    } else if (c == QLatin1Char(')')) {
      --depth;
    } else if (c == QLatin1Char(';') && depth == 0) {
      parts << current.trimmed();
      current.clear();
      continue;
    }
    current += c;
  }
  parts << current.trimmed();
  return parts;
}

bool isName(const QString& text) {
  if (text.isEmpty() || !(text.front().isLetter() || text.front() == QLatin1Char('_'))) {
    return false;
  }
  return std::all_of(text.begin(), text.end(),
                     [](QChar c) { return c.isLetterOrNumber() || c == QLatin1Char('_'); });
}

} // namespace

QString quotedText(const QString& text) {
  QString doubled = text;
  doubled.replace(QLatin1Char('\''), QStringLiteral("''"));
  return QLatin1Char('\'') + doubled + QLatin1Char('\'');
}

QString unquotedText(const QString& expression) {
  const QString text = expression.trimmed();
  if (text.size() >= 2 && (text.front() == QLatin1Char('\'') || text.front() == QLatin1Char('"')) &&
      text.back() == text.front()) {
    const QString quote(text.front());
    return text.mid(1, text.size() - 2).replace(quote + quote, quote);
  }
  return text;
}

QString OptionMapping::expression() const {
  QStringList parts{option};
  for (const auto& row : rows) {
    parts << row.first << row.second;
  }
  if (!otherwise.isEmpty()) {
    parts << otherwise;
  }
  return QStringLiteral("switch(%1)").arg(parts.join(QStringLiteral("; ")));
}

bool OptionMapping::parse(const QString& source, OptionMapping* mapping) {
  const QString text = source.trimmed();
  if (!text.startsWith(QStringLiteral("switch"), Qt::CaseInsensitive) || !text.endsWith(QLatin1Char(')'))) {
    return false;
  }
  const QString rest = text.mid(6).trimmed();
  if (!rest.startsWith(QLatin1Char('('))) {
    return false;
  }
  const QStringList parts = splitArguments(rest.mid(1, rest.size() - 2));
  if (parts.size() < 3 || !isName(parts.front()) ||
      std::any_of(parts.begin(), parts.end(), [](const QString& part) { return part.isEmpty(); })) {
    return false;
  }
  OptionMapping out;
  out.option = parts.front();
  int i = 1;
  for (; i + 1 < parts.size(); i += 2) {
    out.rows.append({parts[i], parts[i + 1]});
  }
  if (i < parts.size()) {
    out.otherwise = parts[i];
  }
  *mapping = out;
  return true;
}

} // namespace mitcad
