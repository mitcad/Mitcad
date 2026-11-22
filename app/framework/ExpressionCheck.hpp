// SPDX-License-Identifier: MIT
#pragma once

// An expression checked while it is typed (mitcad#2181): Add User
// Parameter's Expression field and Edit Parameters' Expression cells ask
// the model's `formula_help` what is wrong with the text, show it in words
// under the field and mark the part it is about (an unknown name) in the
// field, with the wavy underline Edit Formula marks its error range with.

#include <algorithm>
#include <exception>
#include <functional>

#include <QByteArray>
#include <QCoreApplication>
#include <QInputMethodEvent>
#include <QJsonObject>
#include <QLineEdit>
#include <QList>
#include <QString>
#include <QTextCharFormat>

#include "ModelMessages.hpp"
#include "Theme.hpp"

namespace mitcad {

// What is wrong with an expression: the model's words (no byte offsets),
// and the part of the field's text it is about, in characters [start,
// end); -1 when it is about no part.
struct ExpressionProblem {
  QString message;
  int start = -1;
  int end = -1;
  bool isEmpty() const { return message.isEmpty(); }
};

// What is wrong with `checked`, the expression a field's text `typed`
// gives (its spaces trimmed, a bare number with the unit after it), asked
// of the model with `query` (ParameterSource::query,
// CommandContext::queryObject): its value in `unit` (a new parameter's: ""
// a number, "Text" a text), or, given `parameter`, as that parameter's new
// expression. Nothing when it is right, empty, or the model cannot answer
// now (a job computes it): the command then says it.
inline ExpressionProblem checkExpression(const std::function<QJsonObject(const QJsonObject&)>& query,
                                         const QString& typed, const QString& checked, const QString& unit,
                                         const QString& parameter = QString()) {
  ExpressionProblem problem;
  if (!query || checked.trimmed().isEmpty()) {
    return problem;
  }
  QJsonObject request{{QStringLiteral("query"), QStringLiteral("formula_help")},
                      {QStringLiteral("expression"), checked},
                      {QStringLiteral("cursor"), 0}};
  if (parameter.isEmpty()) {
    request.insert(QStringLiteral("unit"), unit);
  } else {
    request.insert(QStringLiteral("parameter"), parameter);
  }
  QJsonObject error;
  try {
    error = query(request).value(QStringLiteral("error")).toObject();
  } catch (const std::exception&) {
    return problem;
  }
  if (error.isEmpty()) {
    return problem;
  }
  problem.message = modelMessage(error.value(QStringLiteral("message")).toString());
  // The model's byte offsets in `checked` as characters of `typed`, which
  // `checked` starts like after the spaces before it.
  const QByteArray bytes = checked.toUtf8();
  int lead = 0;
  while (lead < typed.size() && typed.at(lead).isSpace()) {
    ++lead;
  }
  const auto character = [&](const QString& key) {
    const int byte = std::clamp(error.value(key).toInt(-1), 0, int(bytes.size()));
    return std::min(int(typed.size()), lead + int(QString::fromUtf8(bytes.left(byte)).size()));
  };
  problem.start = character(QStringLiteral("start"));
  problem.end = character(QStringLiteral("end"));
  if (problem.end <= problem.start) {
    problem.start = -1;
    problem.end = -1;
  }
  return problem;
}

// Marks [start, end) of the field's text with Edit Formula's error mark (a
// wavy underline in the theme's error colour), or takes the mark away when
// the range is empty. A format of the field's text layout, given as an
// input method's format without text, as QLineEdit takes formats; the
// field's text, caret and selection stay.
inline void markExpressionRange(QLineEdit* edit, int start, int end) {
  if (edit == nullptr || edit->isReadOnly()) {
    return;
  }
  QList<QInputMethodEvent::Attribute> attributes;
  if (start >= 0 && end > start) {
    QTextCharFormat format;
    format.setUnderlineStyle(QTextCharFormat::WaveUnderline);
    format.setUnderlineColor(themeColor(ColorRole::ErrorBorder, edit->palette()));
    attributes << QInputMethodEvent::Attribute(QInputMethodEvent::TextFormat, start - edit->cursorPosition(),
                                               end - start, format);
  }
  QInputMethodEvent event(QString(), attributes);
  QCoreApplication::sendEvent(edit, &event);
}

} // namespace mitcad
