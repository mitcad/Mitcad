// SPDX-License-Identifier: MIT
// Insert › Component from Web's keys (mitcad#1840): AltGr's characters (@,
// {, [, ], }, \, |, € on Finnish and German layouts) reach the page on
// Windows, where Qt reports AltGr as Control and Alt; Control shortcuts type
// nothing; Option's characters on macOS are not system keys; Alt alone on
// Windows stays one.
#include "UnitTests.hpp"
#include "webbrowser/BrowserKeys.hpp"

#include <cstdio>

#include <QString>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "browser_keys.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

const Qt::KeyboardModifiers controlAlt = Qt::ControlModifier | Qt::AltModifier;

void windowsTests() {
  using mitcad::browserKeyRoles;
  using mitcad::KeyPlatform;
  // AltGr+2 on a Finnish layout: @, a character with AltGr, not a command.
  const auto at = browserKeyRoles(controlAlt, QStringLiteral("@"), KeyPlatform::Windows);
  CHECK(at.characters);
  CHECK(at.altGr);
  CHECK(!at.systemKey);
  // The rest of AltGr's characters, the euro sign too.
  for (const QString& c : {QStringLiteral("{"), QStringLiteral("["), QStringLiteral("]"), QStringLiteral("}"),
                           QStringLiteral("\\"), QStringLiteral("|"), QString(QChar(0x20AC))}) {
    const auto roles = browserKeyRoles(controlAlt, c, KeyPlatform::Windows);
    CHECK(roles.characters && roles.altGr && !roles.systemKey);
  }
  // With Shift as well (AltGr+Shift on some layouts).
  CHECK(browserKeyRoles(controlAlt | Qt::ShiftModifier, QStringLiteral("@"), KeyPlatform::Windows).characters);
  // Control+C: a command, whether Qt's text is the control character or empty.
  const auto copy = browserKeyRoles(Qt::ControlModifier, QString(QChar(0x03)), KeyPlatform::Windows);
  CHECK(!copy.characters);
  CHECK(!copy.altGr);
  CHECK(!copy.systemKey);
  CHECK(!browserKeyRoles(Qt::ControlModifier, QStringLiteral("c"), KeyPlatform::Windows).characters);
  CHECK(!browserKeyRoles(Qt::ControlModifier, QString(), KeyPlatform::Windows).characters);
  // Control and Alt without a printable text: a command, not AltGr.
  CHECK(!browserKeyRoles(controlAlt, QString(), KeyPlatform::Windows).characters);
  CHECK(!browserKeyRoles(controlAlt, QString(QChar(0x7F)), KeyPlatform::Windows).altGr);
  // Alt alone: Windows' system key (its menu keys), never AltGr.
  const auto altF = browserKeyRoles(Qt::AltModifier, QStringLiteral("f"), KeyPlatform::Windows);
  CHECK(altF.systemKey);
  CHECK(!altF.altGr);
  // Plain typing.
  const auto plain = browserKeyRoles(Qt::NoModifier, QStringLiteral("a"), KeyPlatform::Windows);
  CHECK(plain.characters && !plain.systemKey && !plain.altGr);
  CHECK(browserKeyRoles(Qt::ShiftModifier, QStringLiteral("A"), KeyPlatform::Windows).characters);
}

void macTests() {
  using mitcad::browserKeyRoles;
  using mitcad::KeyPlatform;
  // Option+2 on a Finnish layout types @: characters, and not a system key.
  const auto at = browserKeyRoles(Qt::AltModifier, QStringLiteral("@"), KeyPlatform::MacOS);
  CHECK(at.characters);
  CHECK(!at.systemKey);
  CHECK(!at.altGr);
  // Option with a dead key (nothing typed yet) is a system key.
  CHECK(browserKeyRoles(Qt::AltModifier, QString(), KeyPlatform::MacOS).systemKey);
  // Command+C (Qt's Control) and Command+Option+I: commands.
  CHECK(!browserKeyRoles(Qt::ControlModifier, QStringLiteral("c"), KeyPlatform::MacOS).characters);
  CHECK(!browserKeyRoles(controlAlt, QStringLiteral("@"), KeyPlatform::MacOS).characters);
  CHECK(!browserKeyRoles(controlAlt, QStringLiteral("@"), KeyPlatform::MacOS).altGr);
}

void linuxTests() {
  using mitcad::browserKeyRoles;
  using mitcad::KeyPlatform;
  // AltGr as its own modifier: plain characters.
  const auto at = browserKeyRoles(Qt::GroupSwitchModifier, QStringLiteral("@"), KeyPlatform::Linux);
  CHECK(at.characters && !at.systemKey && !at.altGr);
  // Reported as Control and Alt: characters with AltGr, as on Windows.
  CHECK(browserKeyRoles(controlAlt, QStringLiteral("@"), KeyPlatform::Linux).altGr);
  // Control+C stays a command.
  CHECK(!browserKeyRoles(Qt::ControlModifier, QString(QChar(0x03)), KeyPlatform::Linux).characters);
}

} // namespace

int browserKeysTests() {
  windowsTests();
  macTests();
  linuxTests();
  return failures;
}
MITCAD_UNIT_TEST(browserKeysTests);
