// SPDX-License-Identifier: MIT
#include "TimelineSearch.hpp"

#include <algorithm>
#include <utility>
#include <vector>

namespace mitcad {
namespace {

// A word or filter of the text: `key` is "in" or "is" for a filter (its
// value unquoted), empty for a word.
struct Token {
  QString key;
  QString value;
};

// The words and filters of the text, split at spaces outside quotes.
std::vector<Token> tokens(const QString& text) {
  std::vector<QString> raw;
  QString current;
  bool quoted = false;
  bool any = false; // the current token has begun (an empty "" too)
  for (const QChar c : text) {
    if (c == QLatin1Char('"')) {
      quoted = !quoted;
      any = true;
    } else if (c.isSpace() && !quoted) {
      if (any) {
        raw.push_back(current);
      }
      current.clear();
      any = false;
    } else {
      current += c;
      any = true;
    }
  }
  if (any) {
    raw.push_back(current);
  }
  std::vector<Token> result;
  for (const QString& token : raw) {
    const int colon = static_cast<int>(token.indexOf(QLatin1Char(':')));
    const QString key = colon > 0 ? token.left(colon).toLower() : QString();
    if (key == QStringLiteral("in") || key == QStringLiteral("component")) {
      result.push_back({QStringLiteral("in"), token.mid(colon + 1)});
    } else if (key == QStringLiteral("is") || key == QStringLiteral("status")) {
      result.push_back({QStringLiteral("is"), token.mid(colon + 1)});
    } else {
      result.push_back({QString(), token});
    }
  }
  return result;
}

QString quoted(const QString& value) {
  return value.contains(QLatin1Char(' ')) ? QStringLiteral("\"%1\"").arg(value) : value;
}

QString tokenText(const Token& token) {
  return token.key.isEmpty() ? quoted(token.value) : QStringLiteral("%1:%2").arg(token.key, quoted(token.value));
}

bool sameToken(const Token& a, const Token& b) {
  return a.key == b.key && a.value.compare(b.value, Qt::CaseInsensitive) == 0;
}

// A status by its name or an abbreviation of one; 0 for none.
int statusOf(const QString& value) {
  QString name = value.toLower();
  name.remove(QLatin1Char('-')).remove(QLatin1Char('_'));
  static const std::pair<const char*, int> names[] = {
      {"failed", TimelineQuery::Failed},         {"error", TimelineQuery::Failed},
      {"warning", TimelineQuery::Warning},       {"info", TimelineQuery::Info},
      {"note", TimelineQuery::Info},             {"suppressed", TimelineQuery::Suppressed},
      {"rolledback", TimelineQuery::RolledBack}, {"rollback", TimelineQuery::RolledBack},
  };
  for (const auto& [full, status] : names) {
    if (!name.isEmpty() && QString::fromLatin1(full).startsWith(name)) {
      return status;
    }
  }
  return 0;
}

} // namespace

TimelineQuery TimelineQuery::parse(const QString& text) {
  TimelineQuery query;
  for (const Token& token : tokens(text)) {
    const QString value = token.value.trimmed();
    if (value.isEmpty()) {
      continue; // a filter still being typed
    }
    if (token.key == QStringLiteral("in")) {
      query.components << value.toLower();
    } else if (token.key == QStringLiteral("is")) {
      const int status = statusOf(value);
      if (status == 0) {
        query.unknown << QStringLiteral("is:%1").arg(value);
      }
      query.statuses |= status;
    } else {
      query.words << value.toLower();
    }
  }
  return query;
}

bool TimelineQuery::matches(const TimelineSearchItem& item) const {
  if (!unknown.isEmpty()) {
    return false;
  }
  const QString name = item.name.toLower();
  const QString type = item.type.toLower();
  QString typeWords = type;
  typeWords.replace(QLatin1Char('_'), QLatin1Char(' '));
  for (const QString& word : words) {
    if (!name.contains(word) && !type.contains(word) && !typeWords.contains(word)) {
      return false;
    }
  }
  if (!components.isEmpty()) {
    const QString component = item.component.toLower();
    if (std::none_of(components.begin(), components.end(),
                     [&component](const QString& term) { return component.contains(term); })) {
      return false;
    }
  }
  if (statuses != 0) {
    const int has = (item.failed ? Failed : 0) | (item.warned ? Warning : 0) | (item.suppressed ? Suppressed : 0) |
                    (item.rolledBack ? RolledBack : 0) | (item.noted ? Info : 0);
    if ((has & statuses) == 0) {
      return false;
    }
  }
  return true;
}

QString timelineStatusToken(TimelineQuery::Status status) {
  switch (status) {
  case TimelineQuery::Failed:
    return QStringLiteral("is:failed");
  case TimelineQuery::Warning:
    return QStringLiteral("is:warning");
  case TimelineQuery::Suppressed:
    return QStringLiteral("is:suppressed");
  case TimelineQuery::RolledBack:
    return QStringLiteral("is:rolled-back");
  case TimelineQuery::Info:
    return QStringLiteral("is:info");
  }
  return QString();
}

QString timelineComponentToken(const QString& component) {
  return QStringLiteral("in:%1").arg(quoted(component));
}

bool hasTimelineToken(const QString& text, const QString& token) {
  const std::vector<Token> wanted = tokens(token);
  if (wanted.size() != 1) {
    return false;
  }
  const std::vector<Token> present = tokens(text);
  return std::any_of(present.begin(), present.end(), [&](const Token& t) { return sameToken(t, wanted.front()); });
}

QString toggleTimelineToken(const QString& text, const QString& token) {
  const std::vector<Token> wanted = tokens(token);
  if (wanted.size() != 1) {
    return text;
  }
  std::vector<Token> present = tokens(text);
  const auto before = present.size();
  present.erase(std::remove_if(present.begin(), present.end(),
                               [&](const Token& t) { return sameToken(t, wanted.front()); }),
                present.end());
  if (present.size() == before) {
    present.push_back(wanted.front());
  }
  QStringList parts;
  for (const Token& t : present) {
    parts << tokenText(t);
  }
  return parts.join(QLatin1Char(' '));
}

} // namespace mitcad
