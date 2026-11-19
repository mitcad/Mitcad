// SPDX-License-Identifier: MIT
// What the Text tool and Edit Text send (mitcad#2029, GitHub
// mitcad/Mitcad#453): a text drawn in a frame wraps to its width, a
// clicked one is placed at its point; Edit Text's Wrap in Frame is shown
// for a text in a frame not on a path and sends `wrap` only when it
// changed it; the Text box's line breaks reach the model as `\n`.
#include "UnitTests.hpp"
#include "sketch/TextCommands.hpp"

#include <cstdio>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "sketch_text_commands.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad::sketch;

void toolTests() {
  // A drag from upper right to lower left: the frame from its lower left
  // corner, the text at its top and wrapped.
  QJsonObject framed;
  insertNewTextPlace(framed, V2{40, 30}, V2{-10, 5}, true);
  const QJsonObject frame = framed.value(QStringLiteral("frame")).toObject();
  CHECK(frame.value(QStringLiteral("corner")).toArray() == (QJsonArray{-10.0, 5.0}));
  CHECK(frame.value(QStringLiteral("diagonal")).toArray() == (QJsonArray{40.0, 30.0}));
  CHECK(framed.value(QStringLiteral("valign")).toString() == QStringLiteral("top"));
  CHECK(framed.value(QStringLiteral("wrap")).toBool(false));
  CHECK(!framed.contains(QStringLiteral("at")));
  // A click: the text at its point, nothing wrapped.
  QJsonObject placed;
  insertNewTextPlace(placed, V2{3, 4}, V2{3, 4}, false);
  CHECK(placed.value(QStringLiteral("at")).toArray() == (QJsonArray{3.0, 4.0}));
  CHECK(!placed.contains(QStringLiteral("wrap")));
  CHECK(!placed.contains(QStringLiteral("frame")));
  CHECK(!placed.contains(QStringLiteral("valign")));
}

void wrapTests() {
  TextData placed;
  placed.id = QStringLiteral("t1");
  TextData drawn = placed;
  drawn.frame = {QStringLiteral("p1"), QStringLiteral("p2"), QStringLiteral("p3")};
  drawn.wrap = true;
  TextData imported = drawn;
  imported.wrap = false;
  TextData onPath = placed;
  onPath.path = QStringLiteral("c1");
  // Shown for a text in a frame only, and not while it is put on a path.
  CHECK(wrapInFrameShown(drawn, false));
  CHECK(wrapInFrameShown(imported, false));
  CHECK(!wrapInFrameShown(drawn, true));
  CHECK(!wrapInFrameShown(placed, false));
  CHECK(!wrapInFrameShown(onPath, true));
  // Left as the text has it: nothing sent, either way.
  CHECK(!changedTextWrap(drawn, false, true));
  CHECK(!changedTextWrap(imported, false, false));
  // Turned off or on: sent.
  CHECK(changedTextWrap(drawn, false, false) == std::optional<bool>(false));
  CHECK(changedTextWrap(imported, false, true) == std::optional<bool>(true));
  // Not shown: never sent.
  CHECK(!changedTextWrap(placed, false, true));
  CHECK(!changedTextWrap(drawn, true, false));
}

void lineTests() {
  // Enter in the box gives `\n`; the other breaks become it too.
  CHECK(textWithLineBreaks(QStringLiteral("First\nSecond")) == QStringLiteral("First\nSecond"));
  CHECK(textWithLineBreaks(QStringLiteral("A\r\nB\rC")) == QStringLiteral("A\nB\nC"));
  CHECK(textWithLineBreaks(QStringLiteral("A") + QChar(QChar::LineSeparator) + QStringLiteral("B") +
                           QChar(QChar::ParagraphSeparator) + QStringLiteral("C")) == QStringLiteral("A\nB\nC"));
  CHECK(textWithLineBreaks(QStringLiteral("One line")) == QStringLiteral("One line"));
}

} // namespace

int sketchTextCommandsTests() {
  failures = 0;
  toolTests();
  wrapTests();
  lineTests();
  return failures;
}
MITCAD_UNIT_TEST(sketchTextCommandsTests);
