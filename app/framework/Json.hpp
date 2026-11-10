// SPDX-License-Identifier: MIT
#pragma once

// Passing JSON between Qt and the model's bridge.

#include <QByteArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QString>

#include "framework/UserText.hpp"
#include "rust/cxx.h"

namespace mitcad {

inline QByteArray compactJson(const QJsonObject& object) {
  return QJsonDocument(object).toJson(QJsonDocument::Compact);
}

inline rust::Str rustStr(const QByteArray& text) {
  return rust::Str(text.constData(), static_cast<std::size_t>(text.size()));
}

inline QJsonDocument parseJson(const rust::String& json) {
  return QJsonDocument::fromJson(QByteArray(json.data(), static_cast<qsizetype>(json.size())));
}

inline QJsonObject parseObject(const rust::String& json) { return parseJson(json).object(); }

// A model error's text as a person reads it (userText); the whole text,
// with the agents' detail, is modelText's, for the log.
inline QString errorText(const std::exception& error) { return userText(QString::fromUtf8(error.what())); }

inline QString modelText(const std::exception& error) { return QString::fromUtf8(error.what()); }

// What agents read of a model error (MCP, mitcad#1612): its detail with the
// names, ids and coordinates in place of the plain message (agentText).
inline QString agentErrorText(const std::exception& error) { return agentText(modelText(error)); }

// A string member of a JSON object; empty when the key is missing or not a
// string.
inline QString str(const QJsonObject& object, const char* key) {
  return object.value(QLatin1String(key)).toString();
}

} // namespace mitcad
