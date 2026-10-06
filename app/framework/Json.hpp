// SPDX-License-Identifier: MIT
#pragma once

// Passing JSON between Qt and the model's bridge.

#include <QByteArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QString>

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

inline QString errorText(const std::exception& error) { return QString::fromUtf8(error.what()); }

} // namespace mitcad
