// SPDX-License-Identifier: MIT
#pragma once

// The key presses of live MCP's ui_type (mitcad#1735): Unicode text as one
// typed character after another, and keys by their names ("Enter",
// "Ctrl+A", "Shift+Tab"), as Qt key events give them to the field that has
// the keyboard. Plain Qt Gui: no widgets.

#include <vector>

#include <QString>
#include <Qt>

namespace mitcad::mcp {

struct KeyStroke {
  int key = 0; // a Qt::Key
  Qt::KeyboardModifiers modifiers;
  QString text; // what the key types, as a keyboard's key event carries it
  QString name; // as given (keys) or the character (text)
};

// `keys`: key names separated by spaces, each a key with optional
// modifiers joined by '+' as Qt's portable key texts write them ("Enter",
// "Return", "Tab", "Escape", "Backspace", "Delete", "Left", "Home", "F2",
// "Space", "Ctrl+A", "Shift+Tab", "a"). False with `problem` naming the
// first name that is no key (or that is several).
bool parseKeys(const QString& keys, std::vector<KeyStroke>& strokes, QString& problem);

// One key press per character of `text` (a surrogate pair is one
// character): A to Z and a to z with their letter keys (Shift for the
// capitals), the other printable ASCII characters with their keys, any
// other character (ä, ö, Ø, °) with Qt::Key_unknown and the character as
// its text, as a keyboard layout's key types it. A line break is Return, a
// tab Tab.
std::vector<KeyStroke> typedKeys(const QString& text);

} // namespace mitcad::mcp
