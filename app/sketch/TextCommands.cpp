// SPDX-License-Identifier: MIT
// What the Text tool and Edit Text send for a text's place, its wrap and
// its lines (mitcad#2029).
#include "TextCommands.hpp"

#include <algorithm>

#include <QChar>
#include <QJsonArray>

namespace mitcad::sketch {

void insertNewTextPlace(QJsonObject& cmd, const V2& from, const V2& to, bool framed) {
  if (!framed) {
    cmd.insert(QStringLiteral("at"), QJsonArray{from.x, from.y});
    return;
  }
  // The frame from its lower left corner; the text at its top, its lines
  // broken at spaces to the frame's width.
  const V2 lo{std::min(from.x, to.x), std::min(from.y, to.y)};
  const V2 hi{std::max(from.x, to.x), std::max(from.y, to.y)};
  cmd.insert(QStringLiteral("frame"), QJsonObject{{QStringLiteral("corner"), QJsonArray{lo.x, lo.y}},
                                                  {QStringLiteral("diagonal"), QJsonArray{hi.x, hi.y}}});
  cmd.insert(QStringLiteral("valign"), QStringLiteral("top"));
  cmd.insert(QStringLiteral("wrap"), true);
}

bool wrapInFrameShown(const TextData& text, bool onPath) {
  return !text.frame.isEmpty() && !onPath;
}

std::optional<bool> changedTextWrap(const TextData& text, bool onPath, bool checked) {
  if (!wrapInFrameShown(text, onPath) || checked == text.wrap) {
    return std::nullopt;
  }
  return checked;
}

QString textWithLineBreaks(const QString& typed) {
  QString text = typed;
  text.replace(QStringLiteral("\r\n"), QStringLiteral("\n"));
  text.replace(QLatin1Char('\r'), QLatin1Char('\n'));
  text.replace(QChar(QChar::LineSeparator), QLatin1Char('\n'));
  text.replace(QChar(QChar::ParagraphSeparator), QLatin1Char('\n'));
  return text;
}

} // namespace mitcad::sketch
