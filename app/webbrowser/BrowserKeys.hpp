// SPDX-License-Identifier: MIT
#pragma once

// Insert › Component from Web (mitcad#1579, mitcad#1840): what a key press
// is to the component browser's page, decided from Qt's modifiers and the
// typed text alone, in plain Qt Core, so that the unit tests
// (app/tests/browser_keys.cpp) use it without the browser. CefView turns a
// key press into CEF's key events by it.

#include <QString>
#include <Qt>

namespace mitcad {

// The platform whose keyboard conventions apply.
enum class KeyPlatform {
  Windows, // AltGr arrives as Control and Alt together; Alt alone is a system key
  MacOS,   // Qt's Control is Command; Option (Alt) types characters
  Linux,   // AltGr is its own modifier, seldom Control and Alt
};

// The platform MitCad runs on.
KeyPlatform currentKeyPlatform();

// What a key press sends to the page.
struct BrowserKeyRoles {
  // The typed text goes to the page as character events (false: the key is
  // a command, as Control+C, or types nothing).
  bool characters = false;
  // The key events are system keys (Windows' Alt combinations, which the
  // page takes as menu keys and never types).
  bool systemKey = false;
  // The characters were typed with AltGr (Control and Alt together with a
  // printable text): their character events carry AltGr, not Control and
  // Alt, so that the page inserts them instead of taking a shortcut.
  bool altGr = false;
};

// The roles of a key press with the modifiers and the text Qt reports.
BrowserKeyRoles browserKeyRoles(Qt::KeyboardModifiers modifiers, const QString& text, KeyPlatform platform);

} // namespace mitcad
