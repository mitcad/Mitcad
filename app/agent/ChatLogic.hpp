// SPDX-License-Identifier: MIT
#pragma once

// The chat's own logic in Rust (mitcad#947, core/mcp/src/chat.rs) as JSON
// requests: {"op": ...} in, its answer out ("error" when it fails).

#include <cstddef>
#include <string>

#include <QByteArray>
#include <QJsonDocument>
#include <QJsonObject>

#include "mitcad_bridge/lib.h"

namespace mitcad::agent {

inline QJsonObject chatRequest(const QJsonObject& request) {
  const QByteArray text = QJsonDocument(request).toJson(QJsonDocument::Compact);
  const std::string answer(agent_chat(rust::Str(text.constData(), static_cast<std::size_t>(text.size()))));
  return QJsonDocument::fromJson(QByteArray::fromStdString(answer)).object();
}

} // namespace mitcad::agent
