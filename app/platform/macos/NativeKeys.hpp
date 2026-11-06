// SPDX-License-Identifier: MIT
#pragma once

#import <AppKit/AppKit.h>

#include <algorithm>

#include <QKeySequence>
#include <QList>
#include <QString>

namespace mitcad::mac {

inline Qt::KeyboardModifiers qtKeyModifiers(NSEventModifierFlags flags) {
  Qt::KeyboardModifiers modifiers;
  // Qt's portable Ctrl is Command on macOS; Meta is physical Control.
  if ((flags & NSEventModifierFlagCommand) != 0) modifiers |= Qt::ControlModifier;
  if ((flags & NSEventModifierFlagControl) != 0) modifiers |= Qt::MetaModifier;
  if ((flags & NSEventModifierFlagOption) != 0) modifiers |= Qt::AltModifier;
  if ((flags & NSEventModifierFlagShift) != 0) modifiers |= Qt::ShiftModifier;
  return modifiers;
}

// A scroll event's turn as Qt's cocoa plugin gives it to a QWheelEvent's
// angleDelta (1/8 degree, 120 a notch): a mouse wheel's notch is 120
// without its acceleration, a touchpad's points are 2 each. Vertical, or
// sideways when nothing turned vertically (Shift with a mouse wheel comes
// sideways on macOS); 0 for another event. The value fields' wheel step
// (mitcad#1350, ParameterField::wheelStep) reads it as Qt's fields do.
inline int qtWheelAngle(NSEvent* event) {
  if (event == nil || event.type != NSEventTypeScrollWheel) return 0;
  CGFloat x = 0;
  CGFloat y = 0;
  if (event.hasPreciseScrollingDeltas) {
    x = event.scrollingDeltaX * 2;
    y = event.scrollingDeltaY * 2;
  } else {
    x = std::clamp<CGFloat>(event.deltaX * 10000, -120, 120);
    y = std::clamp<CGFloat>(event.deltaY * 10000, -120, 120);
  }
  const int vertical = static_cast<int>(y);
  return vertical != 0 ? vertical : static_cast<int>(x);
}

inline QKeySequence keySequence(NSEvent* event) {
  if (event == nil || (event.type != NSEventTypeKeyDown && event.type != NSEventTypeKeyUp)) return {};

  int key = 0;
  // Physical codes disambiguate Return/keypad Enter and backward/forward
  // Delete independently of the keyboard layout and the event's text.
  switch (event.keyCode) {
  case 36: key = Qt::Key_Return; break;
  case 76: key = Qt::Key_Enter; break;
  case 53: key = Qt::Key_Escape; break;
  case 48: key = Qt::Key_Tab; break;
  case 51: key = Qt::Key_Backspace; break;
  case 117: key = Qt::Key_Delete; break;
  case 71: key = Qt::Key_Clear; break;
  case 115: key = Qt::Key_Home; break;
  case 119: key = Qt::Key_End; break;
  case 116: key = Qt::Key_PageUp; break;
  case 121: key = Qt::Key_PageDown; break;
  case 123: key = Qt::Key_Left; break;
  case 124: key = Qt::Key_Right; break;
  case 125: key = Qt::Key_Down; break;
  case 126: key = Qt::Key_Up; break;
  default: break;
  }
  if (key == 0) {
    NSString* characters = event.charactersIgnoringModifiers;
    if (characters.length == 0) return {};
    const unichar character = [characters characterAtIndex:0];
    if (character >= NSF1FunctionKey && character <= NSF35FunctionKey) {
      key = Qt::Key_F1 + character - NSF1FunctionKey;
    } else {
      switch (character) {
      case NSCarriageReturnCharacter: case '\n': key = Qt::Key_Return; break;
      case NSEnterCharacter: key = Qt::Key_Enter; break;
      case 0x1b: key = Qt::Key_Escape; break;
      case NSTabCharacter: case NSBackTabCharacter: key = Qt::Key_Tab; break;
      case NSBackspaceCharacter: case NSDeleteCharacter: key = Qt::Key_Backspace; break;
      case NSDeleteFunctionKey: key = Qt::Key_Delete; break;
      case NSInsertFunctionKey: key = Qt::Key_Insert; break;
      case NSHomeFunctionKey: key = Qt::Key_Home; break;
      case NSEndFunctionKey: key = Qt::Key_End; break;
      case NSPageUpFunctionKey: key = Qt::Key_PageUp; break;
      case NSPageDownFunctionKey: key = Qt::Key_PageDown; break;
      case NSLeftArrowFunctionKey: key = Qt::Key_Left; break;
      case NSRightArrowFunctionKey: key = Qt::Key_Right; break;
      case NSUpArrowFunctionKey: key = Qt::Key_Up; break;
      case NSDownArrowFunctionKey: key = Qt::Key_Down; break;
      case NSClearLineFunctionKey: key = Qt::Key_Clear; break;
      case NSHelpFunctionKey: key = Qt::Key_Help; break;
      case NSPauseFunctionKey: key = Qt::Key_Pause; break;
      case NSPrintScreenFunctionKey: key = Qt::Key_Print; break;
      case NSScrollLockFunctionKey: key = Qt::Key_ScrollLock; break;
      default: {
        const auto text = QString::fromUtf8(characters.UTF8String).toUpper().toUcs4();
        if (text.size() == 1 && text.front() >= 0x20 && text.front() < 0xf700) {
          key = static_cast<int>(text.front());
        }
        break;
      }
      }
    }
  }
  if (key == 0) return {};
  Qt::KeyboardModifiers modifiers = qtKeyModifiers(event.modifierFlags);
  // AppKit also marks arrows/function keys as numeric-pad keys. Enter is
  // already distinct from Return; preserve Num only on printable keypad keys.
  if (key < Qt::Key_Escape && (event.modifierFlags & NSEventModifierFlagNumericPad) != 0) {
    modifiers |= Qt::KeypadModifier;
  }
  return QKeySequence(QKeyCombination(modifiers, static_cast<Qt::Key>(key)));
}

inline void setKeyEquivalent(NSMenuItem* item, const QKeySequence& sequence) {
  item.keyEquivalent = @"";
  item.keyEquivalentModifierMask = 0;
  if (sequence.isEmpty() || sequence.count() != 1) return;

  const QKeyCombination combination = sequence[0];
  const auto modifiers = combination.keyboardModifiers();
  NSEventModifierFlags flags = 0;
  if (modifiers.testFlag(Qt::ControlModifier)) flags |= NSEventModifierFlagCommand;
  if (modifiers.testFlag(Qt::MetaModifier)) flags |= NSEventModifierFlagControl;
  if (modifiers.testFlag(Qt::AltModifier)) flags |= NSEventModifierFlagOption;
  if (modifiers.testFlag(Qt::ShiftModifier)) flags |= NSEventModifierFlagShift;
  if (modifiers.testFlag(Qt::KeypadModifier)) flags |= NSEventModifierFlagNumericPad;

  const int key = combination.key();
  unichar character = 0;
  switch (key) {
  case Qt::Key_Return: character = NSCarriageReturnCharacter; break;
  case Qt::Key_Enter: character = NSEnterCharacter; break;
  case Qt::Key_Escape: character = 0x1b; break;
  // Menu equivalents deliberately differ from NSEvent's deletion characters.
  // https://developer.apple.com/documentation/appkit/nsmenuitem/keyequivalent
  case Qt::Key_Backspace: character = NSBackspaceCharacter; break;
  case Qt::Key_Delete: character = NSDeleteCharacter; break;
  case Qt::Key_Tab: character = NSTabCharacter; break;
  case Qt::Key_Backtab: character = NSTabCharacter; flags |= NSEventModifierFlagShift; break;
  case Qt::Key_Insert: character = NSInsertFunctionKey; break;
  case Qt::Key_Home: character = NSHomeFunctionKey; break;
  case Qt::Key_End: character = NSEndFunctionKey; break;
  case Qt::Key_PageUp: character = NSPageUpFunctionKey; break;
  case Qt::Key_PageDown: character = NSPageDownFunctionKey; break;
  case Qt::Key_Left: character = NSLeftArrowFunctionKey; break;
  case Qt::Key_Right: character = NSRightArrowFunctionKey; break;
  case Qt::Key_Up: character = NSUpArrowFunctionKey; break;
  case Qt::Key_Down: character = NSDownArrowFunctionKey; break;
  case Qt::Key_Clear: character = NSClearLineFunctionKey; break;
  case Qt::Key_Help: character = NSHelpFunctionKey; break;
  case Qt::Key_Pause: character = NSPauseFunctionKey; break;
  case Qt::Key_Print: character = NSPrintScreenFunctionKey; break;
  case Qt::Key_ScrollLock: character = NSScrollLockFunctionKey; break;
  default:
    if (key >= Qt::Key_F1 && key <= Qt::Key_F35) {
      character = static_cast<unichar>(NSF1FunctionKey + key - Qt::Key_F1);
    } else if (key >= 0x20 && key < 0xf700) {
      character = QChar(static_cast<ushort>(key)).toLower().unicode();
    }
    break;
  }
  if (character != 0) {
    item.keyEquivalent = [NSString stringWithCharacters:&character length:1];
    item.keyEquivalentModifierMask = flags;
  }
}

} // namespace mitcad::mac
