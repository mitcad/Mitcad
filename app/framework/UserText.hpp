// SPDX-License-Identifier: MIT
#pragma once

// The text a person reads of a model's message, and the one agents read,
// without the bridge (framework/Json.hpp includes it; code that needs no
// cxx, such as sketch/SketchConflict.cpp and its test, or the CLI,
// includes this alone).

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QString>
#include <QStringList>

namespace mitcad {

namespace usertext {

inline const QString& tail() {
  static const QString kTail = QStringLiteral("\n[placed: ");
  return kTail;
}

// The end of the tail starting at `at` (the index after its closing ']'),
// or -1 where none is there: its JSON object by its brackets outside
// strings, then "]".
inline qsizetype tailEnd(const QString& text, qsizetype at) {
  int depth = 0;
  bool quoted = false;
  bool escaped = false;
  for (qsizetype i = at + tail().size(); i < text.size(); ++i) {
    const QChar c = text.at(i);
    if (quoted) {
      if (escaped) {
        escaped = false;
      } else if (c == QLatin1Char('\\')) {
        escaped = true;
      } else if (c == QLatin1Char('"')) {
        quoted = false;
      }
      continue;
    }
    if (c == QLatin1Char('"')) {
      quoted = true;
    } else if (c == QLatin1Char('{') || c == QLatin1Char('[')) {
      ++depth;
    } else if (c == QLatin1Char('}') || c == QLatin1Char(']')) {
      --depth;
      if (depth == 0) {
        const qsizetype end = i + 1;
        return end < text.size() && text.at(end) == QLatin1Char(']') ? end + 1 : -1;
      }
    }
  }
  return -1;
}

} // namespace usertext

// The text a person reads of a model's error or refusal (mitcad#1612,
// docs/ui-conventions.md "What a message says"): the text before each last
// line "\n[placed: {…}]" that carries its places and the agents' detail
// (core/model-base/src/notice.rs, PLACED_TAIL); any text after such a
// line is kept. Other text as it is.
inline QString userText(const QString& text) {
  QString shown;
  qsizetype from = 0;
  for (;;) {
    const qsizetype at = text.indexOf(usertext::tail(), from);
    const qsizetype end = at < 0 ? -1 : usertext::tailEnd(text, at);
    if (end < 0) {
      break; // no tail, or not one
    }
    shown += text.mid(from, at - from);
    from = end;
  }
  return shown + text.mid(from);
}

// The text agents and the log read of the same (mitcad#1612): each tail's
// detail in place of the message it explains, as the model's
// FeatureError::agent_text gives it ("Body1: a face could not be
// triangulated: F1:side0 (cylinder)"). Other text as it is.
inline QString agentText(const QString& text) {
  QString agents;
  qsizetype from = 0;
  for (;;) {
    const qsizetype at = text.indexOf(usertext::tail(), from);
    const qsizetype end = at < 0 ? -1 : usertext::tailEnd(text, at);
    if (end < 0) {
      break;
    }
    const qsizetype json = at + usertext::tail().size();
    const QJsonObject inner =
        QJsonDocument::fromJson(text.mid(json, end - 1 - json).toUtf8()).object();
    const QString message = inner.value(QStringLiteral("message")).toString();
    const QString detail = inner.value(QStringLiteral("detail")).toString();
    QString before = text.mid(from, at - from);
    if (!detail.isEmpty()) {
      if (before.endsWith(message)) {
        before = before.left(before.size() - message.size()) + detail;
      } else {
        before += QStringLiteral(" (") + detail + QLatin1Char(')');
      }
    }
    agents += before;
    from = end;
  }
  return agents + text.mid(from);
}

// The texts a person reads of a model answer's list `key` (a sketch
// command's "warnings" or "notes"): its plain-word list `shown`
// ("warning_messages", "note_messages", mitcad#1612) where the model gives
// one, else userText of each. The list `key` itself is the agents'.
inline QStringList shownTexts(const QJsonObject& answer, const QString& key, const QString& shown) {
  QStringList texts;
  const QJsonValue plain = answer.value(shown);
  for (const QJsonValue& text : (plain.isArray() ? plain : answer.value(key)).toArray()) {
    texts << userText(text.toString());
  }
  return texts;
}

} // namespace mitcad
