// SPDX-License-Identifier: MIT
#pragma once

// The AppKit frontend's Chat card (mitcad#1075): the shared agent::ChatPanel
// presented through the bridge (createNativeWidgetView) in a floating card
// on the right edge, as the Floating layout's chatCard. NativeMacWindow.mm
// places the card; this builds its content: the title and a host that
// keeps the conversation's latest rows in sight, as the Qt panel's
// keepAtEnd does. Included by Objective-C++ sources only.

#import <AppKit/AppKit.h>

namespace mitcad::mac {

// The card's width in a wide window, the Floating chatCard's; a narrow
// canvas narrows it, never below what the panel's controls need.
constexpr CGFloat chatCardWidth = 340;
constexpr CGFloat chatCardMinimumWidth = 280;
// The width the card takes in a canvas whose room between its margins is
// `room`: at most a third of it beyond the minimum.
CGFloat chatCardPlacedWidth(CGFloat room);

// Builds the card's content in its `body`: the title Chat and under it the
// host of the panel's native view, which it returns.
NSView* buildChatCard(NSView* body);
// Installs `content` (the bridge's view of the panel; nil clears) in the
// host, filling it with the conversation taking the extra height; the
// same content again only brings the conversation's end into sight when
// it followed its end.
void setChatCardContent(NSView* host, NSView* content);
// The message field takes the keyboard. False when it is not shown (the
// agent is not ready).
bool focusChatCard(NSView* host);

} // namespace mitcad::mac
