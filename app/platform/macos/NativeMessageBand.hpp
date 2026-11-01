// SPDX-License-Identifier: MIT
#pragma once

// The AppKit frontend's message band (mitcad#1098, the native part of
// mitcad#1042; docs/ui-conventions.md "The message band"): the content of
// the card right above the timeline. It draws what the shared MessageLog
// shows, the same levels, lifetimes, stub and history as the Qt
// MessageBand, whose MessageCenter keeps the times: a message in full (the
// level's symbol, the text with its level's word wrapped to two lines in
// the level's color, the whole text in its tooltip, and ×), or the dimmed
// one-line stub of the latest message, which opens the session's history in
// a popover. NativeMacWindow.mm places the card. Included by Objective-C++
// sources only.
//
// Identifiers: native.messageBand.text, native.messageBand.close,
// native.messageBand.stub; the history's rows native.messageHistory.row<i>
// (1: the newest), a row about a feature also native.messageHistory.<uid>
// (the newest such row first).
//
// Logged: `Native message band: full|stub <level> <text>` when what it
// shows changes; `Message history: <n> entries` when the history opens,
// `Message history row <i>: <level> <text>` (the first 50 rows),
// `Message history: selected <uid>` and `Message history: closed`, as the
// Qt history.

#import <AppKit/AppKit.h>

#include <functional>

#include <QString>

namespace mitcad {
class MessageLog;
}

@interface MITCADMessageBand : NSView <NSPopoverDelegate> {
@public
  // The × : the shared log's close.
  std::function<void()> closed;
  // A history row about a feature: its uid.
  std::function<void(const QString&)> featureChosen;
}
// What the log shows now; the band keeps the log for its history.
- (void)showLog:(const mitcad::MessageLog*)log;
- (BOOL)hasMessage;
- (BOOL)isStub;
// The card's height for a message in full: two lines of text.
- (CGFloat)fullHeight;
// The card's size for the stub: one line, as wide as its text.
- (NSSize)stubSize;
@end
