// SPDX-License-Identifier: MIT
// ui_type's key presses (mitcad#1735): key names with modifiers, and Unicode
// text as one typed character after another (ä and ö with their own text,
// a surrogate pair as one character).
#include "mcp/UiKeys.hpp"

#include <cstdio>
#include <vector>

#include <QString>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "ui_keys.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

} // namespace

int uiKeysTests() {
  using mitcad::mcp::KeyStroke;

  // Key names, with modifiers, separated by spaces.
  std::vector<KeyStroke> keys;
  QString problem;
  CHECK(mitcad::mcp::parseKeys(QStringLiteral(" Ctrl+A  Backspace Enter Tab Shift+Tab Escape F2 Left a "), keys, problem));
  CHECK(problem.isEmpty());
  CHECK(keys.size() == 9);
  if (keys.size() == 9) {
    CHECK(keys[0].key == Qt::Key_A && keys[0].modifiers == Qt::ControlModifier && keys[0].text.isEmpty());
    CHECK(keys[0].name == QStringLiteral("Ctrl+A"));
    CHECK(keys[1].key == Qt::Key_Backspace && keys[1].text == QStringLiteral("\b"));
    CHECK(keys[2].key == Qt::Key_Enter && keys[2].text == QStringLiteral("\r"));
    CHECK(keys[3].key == Qt::Key_Tab && keys[3].modifiers == Qt::NoModifier && keys[3].text == QStringLiteral("\t"));
    CHECK(keys[4].key == Qt::Key_Tab && keys[4].modifiers == Qt::ShiftModifier);
    CHECK(keys[5].key == Qt::Key_Escape);
    CHECK(keys[6].key == Qt::Key_F2 && keys[6].text.isEmpty());
    CHECK(keys[7].key == Qt::Key_Left);
    CHECK(keys[8].key == Qt::Key_A && keys[8].text == QStringLiteral("a"));
  }
  // An unknown name is refused by its name; nothing is added.
  std::vector<KeyStroke> none;
  CHECK(!mitcad::mcp::parseKeys(QStringLiteral("Enter Flurb"), none, problem));
  CHECK(problem.startsWith(QStringLiteral("Flurb is not a key name")));
  CHECK(none.empty());
  CHECK(mitcad::mcp::parseKeys(QString(), none, problem) && none.empty());

  // Text: letters with their keys, the rest with their characters.
  const std::vector<KeyStroke> typed = mitcad::mcp::typedKeys(QString::fromUtf8("py\xc3\xb6r\xc3\xa4_D 5"));
  CHECK(typed.size() == 9);
  if (typed.size() == 9) {
    CHECK(typed[0].key == Qt::Key_P && typed[0].text == QStringLiteral("p") && typed[0].modifiers == Qt::NoModifier);
    CHECK(typed[2].key == Qt::Key_unknown && typed[2].text == QString(QChar(0x00f6)));
    CHECK(typed[4].key == Qt::Key_unknown && typed[4].text == QString(QChar(0x00e4)));
    CHECK(typed[5].key == Qt::Key_Underscore && typed[5].text == QStringLiteral("_"));
    CHECK(typed[6].key == Qt::Key_D && typed[6].modifiers == Qt::ShiftModifier && typed[6].text == QStringLiteral("D"));
    CHECK(typed[7].key == Qt::Key_Space);
    CHECK(typed[8].key == Qt::Key_5);
  }
  // A character outside the basic plane is one key press; a line break is
  // Return.
  const std::vector<KeyStroke> wide = mitcad::mcp::typedKeys(QString::fromUtf8("\xf0\x9f\x94\xa9\n"));
  CHECK(wide.size() == 2);
  if (wide.size() == 2) {
    CHECK(wide[0].key == Qt::Key_unknown && wide[0].text.size() == 2);
    CHECK(wide[1].key == Qt::Key_Return && wide[1].text == QStringLiteral("\r"));
  }
  return failures;
}
