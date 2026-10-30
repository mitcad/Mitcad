// SPDX-License-Identifier: MIT
#pragma once

// The chat's own logic in Rust (mitcad#947, core/mcp/src/chat.rs) as JSON
// requests: {"op": ...} in, its answer out ("error" when it fails).

#include <cstddef>
#include <string>

#include <QByteArray>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QList>
#include <QString>

#include "mitcad_bridge/lib.h"

namespace mitcad::agent {

inline QJsonObject chatRequest(const QJsonObject& request) {
  const QByteArray text = QJsonDocument(request).toJson(QJsonDocument::Compact);
  const std::string answer(agent_chat(rust::Str(text.constData(), static_cast<std::size_t>(text.size()))));
  return QJsonDocument::fromJson(QByteArray::fromStdString(answer)).object();
}

// The panel's entries (ChatController::entries) as tool_groups reads them:
// the role, and a call's name, summary, state and (history) error.
inline QJsonArray toolGroupEntries(const QList<QJsonObject>& entries) {
  QJsonArray list;
  for (const QJsonObject& entry : entries) {
    const QString role = entry.value(QStringLiteral("role")).toString();
    QJsonObject item;
    item.insert(QStringLiteral("role"), role);
    if (role == QStringLiteral("tool")) {
      item.insert(QStringLiteral("name"), entry.value(QStringLiteral("name")));
      item.insert(QStringLiteral("summary"), entry.value(QStringLiteral("summary")));
      item.insert(QStringLiteral("state"), entry.value(QStringLiteral("state")));
      item.insert(QStringLiteral("error"), entry.value(QStringLiteral("error")));
    }
    list.append(item);
  }
  return list;
}

// The tool calls' groups (mitcad#983, core/mcp/src/chat/groups.rs): the
// calls with nothing else between them, each {"first", "last", "count",
// "failed", "running", "state" (the latest call's), "latest", "head"}.
inline QJsonArray toolGroups(const QList<QJsonObject>& entries) {
  QJsonObject request;
  request.insert(QStringLiteral("op"), QStringLiteral("tool_groups"));
  request.insert(QStringLiteral("entries"), toolGroupEntries(entries));
  return chatRequest(request).value(QStringLiteral("groups")).toArray();
}

// The group holding the entry at `index`; empty when that entry is no call.
inline QJsonObject toolGroupOf(const QList<QJsonObject>& entries, int index) {
  QJsonObject request;
  request.insert(QStringLiteral("op"), QStringLiteral("tool_groups"));
  request.insert(QStringLiteral("entries"), toolGroupEntries(entries));
  request.insert(QStringLiteral("index"), index);
  return chatRequest(request).value(QStringLiteral("group")).toObject();
}

} // namespace mitcad::agent
