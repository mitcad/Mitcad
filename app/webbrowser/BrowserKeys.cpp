// SPDX-License-Identifier: MIT
#include "webbrowser/BrowserKeys.hpp"

namespace mitcad {

KeyPlatform currentKeyPlatform() {
#if defined(_WIN32)
  return KeyPlatform::Windows;
#elif defined(__APPLE__)
  return KeyPlatform::MacOS;
#else
  return KeyPlatform::Linux;
#endif
}

namespace {

// Text a page would insert: not empty, and no control character (Control+C
// gives U+0003, Control+Backspace U+007F).
bool printable(const QString& text) {
  if (text.isEmpty()) {
    return false;
  }
  for (const QChar c : text) {
    if (c.unicode() < 0x20 || c.unicode() == 0x7F) {
      return false;
    }
  }
  return true;
}

} // namespace

BrowserKeyRoles browserKeyRoles(Qt::KeyboardModifiers modifiers, const QString& text, KeyPlatform platform) {
  const bool control = modifiers.testFlag(Qt::ControlModifier);
  const bool alt = modifiers.testFlag(Qt::AltModifier);
  const bool typed = printable(text);
  BrowserKeyRoles roles;
  if (platform == KeyPlatform::MacOS) {
    // Command (Qt's Control) is always a command, with Option too
    // (Command+Option+I); Option alone types characters (@ on Finnish and
    // German layouts), so it is a system key only when it types nothing.
    roles.characters = !control;
    roles.systemKey = alt && !typed;
    return roles;
  }
  // Windows reports AltGr as Control and Alt together, with the character
  // in the text (AltGr+2 is @ on a Finnish layout): a typed character, not
  // a Control shortcut. Control and Alt without a printable text stays a
  // command.
  roles.altGr = control && alt && typed;
  roles.characters = !control || roles.altGr;
  // Alt without Control is a system key (Windows' WM_SYSKEYDOWN and
  // WM_SYSCHAR, which the page never types); with Control, as AltGr is,
  // Windows sends ordinary key and character messages.
  roles.systemKey = alt && !control;
  return roles;
}

} // namespace mitcad
