// SPDX-License-Identifier: MIT
#include "mcp/UiKeys.hpp"

#include <QKeyCombination>
#include <QKeySequence>
#include <QRegularExpression>
#include <QStringList>

namespace mitcad::mcp {

namespace {

// What a key types without Ctrl, Alt or Meta, as a keyboard's event
// carries it: a letter in its case, the other printable characters, and the
// control characters of Return, Tab, Backspace, Escape and Delete.
QString textOf(int key, Qt::KeyboardModifiers modifiers) {
  if (modifiers & (Qt::ControlModifier | Qt::AltModifier | Qt::MetaModifier)) {
    return {};
  }
  if (key >= Qt::Key_A && key <= Qt::Key_Z) {
    const QChar letter(static_cast<char16_t>(u'a' + (key - Qt::Key_A)));
    return (modifiers & Qt::ShiftModifier) ? QString(letter.toUpper()) : QString(letter);
  }
  if (key >= 0x20 && key < 0x7f) {
    return QString(QChar(static_cast<char16_t>(key)));
  }
  switch (key) {
  case Qt::Key_Return:
  case Qt::Key_Enter:
    return QStringLiteral("\r");
  case Qt::Key_Tab:
    return QStringLiteral("\t");
  case Qt::Key_Backspace:
    return QStringLiteral("\b");
  case Qt::Key_Escape:
    return QString(QChar(static_cast<char16_t>(0x1b)));
  case Qt::Key_Delete:
    return QString(QChar(static_cast<char16_t>(0x7f)));
  default:
    return {};
  }
}

} // namespace

bool parseKeys(const QString& keys, std::vector<KeyStroke>& strokes, QString& problem) {
  static const QRegularExpression spaces(QStringLiteral("\\s+"));
  const QStringList names = keys.split(spaces, Qt::SkipEmptyParts);
  std::vector<KeyStroke> parsed;
  for (const QString& name : names) {
    const QKeySequence sequence = QKeySequence::fromString(name, QKeySequence::PortableText);
    const int key = sequence.count() == 1 ? static_cast<int>(sequence[0].key()) : 0;
    if (sequence.count() != 1 || key == 0 || key == Qt::Key_unknown) {
      problem = QStringLiteral("%1 is not a key name (write keys such as Enter, Tab, Escape, Backspace, Left, F2, "
                               "Ctrl+A or Shift+Tab, separated by spaces)")
                    .arg(name);
      return false;
    }
    KeyStroke stroke;
    stroke.key = key;
    stroke.modifiers = sequence[0].keyboardModifiers();
    stroke.text = textOf(key, stroke.modifiers);
    stroke.name = name;
    parsed.push_back(stroke);
  }
  strokes.insert(strokes.end(), parsed.begin(), parsed.end());
  return true;
}

std::vector<KeyStroke> typedKeys(const QString& text) {
  std::vector<KeyStroke> strokes;
  for (qsizetype i = 0; i < text.size(); ++i) {
    KeyStroke stroke;
    QString character(text.at(i));
    if (text.at(i).isHighSurrogate() && i + 1 < text.size() && text.at(i + 1).isLowSurrogate()) {
      character += text.at(i + 1);
      ++i;
    }
    const char16_t code = character.at(0).unicode();
    stroke.name = character;
    stroke.text = character;
    if (character.size() == 1 && code >= u'a' && code <= u'z') {
      stroke.key = Qt::Key_A + (code - u'a');
    } else if (character.size() == 1 && code >= u'A' && code <= u'Z') {
      stroke.key = Qt::Key_A + (code - u'A');
      stroke.modifiers = Qt::ShiftModifier;
    } else if (character.size() == 1 && code >= 0x20 && code < 0x7f) {
      stroke.key = code;
    } else if (code == u'\n' || code == u'\r') {
      stroke.key = Qt::Key_Return;
      stroke.text = QStringLiteral("\r");
    } else if (code == u'\t') {
      stroke.key = Qt::Key_Tab;
    } else {
      stroke.key = Qt::Key_unknown;
    }
    strokes.push_back(stroke);
  }
  return strokes;
}

} // namespace mitcad::mcp
