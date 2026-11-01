// SPDX-License-Identifier: MIT
#import "NativeChatCard.hpp"

#include <algorithm>

#include <QObject>
#include <QString>

// The host of the panel's native view. The bridge replaces the view's
// controls when the panel's rows change (a new message, a tool call, an
// approval card), its conversation's scroll view with them, and a streamed
// answer makes the conversation taller: while the conversation showed its
// end, it shows the new end, as the Qt panel's keepAtEnd; scrolled back by
// the user, it stays where it is until a new conversation view comes.
@interface MITCADChatHost : NSView {
@public
  NSScrollView* __weak conversation;
  CGFloat conversationHeight;
  BOOL followsEnd;
}
- (void)followConversation;
@end

@implementation MITCADChatHost

- (BOOL)isFlipped { return YES; }

- (void)dealloc { [NSNotificationCenter.defaultCenter removeObserver:self]; }

- (void)layout {
  [super layout];
  [self followConversation];
}

- (NSScrollView*)conversationIn:(NSView*)view {
  if ([view isKindOfClass:NSScrollView.class] && [view.accessibilityIdentifier isEqualToString:@"chatConversation"])
    return (NSScrollView*)view;
  for (NSView* child in view.subviews) {
    if (NSScrollView* found = [self conversationIn:child]) return found;
  }
  return nil;
}

// The end is the bottom, whether or not the bridge's document is flipped;
// `height` is the document's height the visible part is measured against.
- (BOOL)showsEndOf:(CGFloat)height {
  NSView* document = conversation.documentView;
  if (document == nil) return YES;
  const NSRect visible = document.visibleRect;
  return document.isFlipped ? NSMaxY(visible) >= height - 8 : NSMinY(visible) <= 8;
}

- (void)scrollToEnd {
  NSView* document = conversation.documentView;
  if (document == nil) return;
  const NSRect visible = document.visibleRect;
  const CGFloat y = document.isFlipped ? std::max(CGFloat(0), NSHeight(document.bounds) - NSHeight(visible)) : 0;
  [document scrollPoint:NSMakePoint(NSMinX(visible), y)];
}

- (void)followConversation {
  NSScrollView* found = [self conversationIn:self];
  if (found != conversation) {
    NSNotificationCenter* center = NSNotificationCenter.defaultCenter;
    [center removeObserver:self name:NSViewFrameDidChangeNotification object:nil];
    [center removeObserver:self name:NSViewBoundsDidChangeNotification object:nil];
    conversation = found;
    // A new view of the conversation (the bridge rebuilt it for a new row)
    // shows its latest rows.
    followsEnd = YES;
    if (found != nil) {
      found.documentView.postsFrameChangedNotifications = YES;
      found.contentView.postsBoundsChangedNotifications = YES;
      [center addObserver:self selector:@selector(conversationResized:)
                     name:NSViewFrameDidChangeNotification object:found.documentView];
      [center addObserver:self selector:@selector(conversationScrolled:)
                     name:NSViewBoundsDidChangeNotification object:found.contentView];
      conversationHeight = NSHeight(found.documentView.frame);
    }
  }
  if (followsEnd) [self scrollToEnd];
}

- (void)conversationResized:(NSNotification*)notification {
  NSView* document = conversation.documentView;
  if (notification.object != document) return;
  // Measured against the height before the change: a flipped document
  // grows below what is in sight.
  const BOOL atEnd = followsEnd || [self showsEndOf:conversationHeight];
  conversationHeight = NSHeight(document.frame);
  followsEnd = atEnd;
  if (atEnd) [self scrollToEnd];
}

- (void)conversationScrolled:(NSNotification*)notification {
  NSView* document = conversation.documentView;
  // A scroll of the user's, not the clip view following a resize.
  if (notification.object != conversation.contentView || NSHeight(document.frame) != conversationHeight) return;
  followsEnd = [self showsEndOf:conversationHeight];
}

@end

namespace mitcad::mac {

CGFloat chatCardPlacedWidth(CGFloat room) {
  return std::max(chatCardMinimumWidth, std::min(chatCardWidth, room / 3));
}

NSView* buildChatCard(NSView* body) {
  NSTextField* title = [NSTextField labelWithString:QObject::tr("Chat").toNSString()];
  title.font = [NSFont systemFontOfSize:13 weight:NSFontWeightSemibold];
  title.translatesAutoresizingMaskIntoConstraints = NO;
  [body addSubview:title];
  MITCADChatHost* host = [[MITCADChatHost alloc] initWithFrame:NSZeroRect];
  host.translatesAutoresizingMaskIntoConstraints = NO;
  host.accessibilityIdentifier = @"native.chat";
  host.accessibilityLabel = QObject::tr("Chat").toNSString();
  [body addSubview:host];
  // The Qt card's title row, then its content margins (8, 4, 8, 8).
  [NSLayoutConstraint activateConstraints:@[
    [title.leadingAnchor constraintEqualToAnchor:body.leadingAnchor constant:14],
    [title.topAnchor constraintEqualToAnchor:body.topAnchor constant:8],
    [host.leadingAnchor constraintEqualToAnchor:body.leadingAnchor constant:8],
    [host.trailingAnchor constraintEqualToAnchor:body.trailingAnchor constant:-8],
    [host.topAnchor constraintEqualToAnchor:body.topAnchor constant:30],
    [host.bottomAnchor constraintEqualToAnchor:body.bottomAnchor constant:-8]
  ]];
  return host;
}

void setChatCardContent(NSView* host, NSView* content) {
  if (![host isKindOfClass:MITCADChatHost.class]) return;
  MITCADChatHost* chat = (MITCADChatHost*)host;
  if (content != nil && content.superview == host) {
    [chat followConversation];
    return;
  }
  for (NSView* child in [host.subviews copy]) [child removeFromSuperview];
  if (content == nil) {
    [chat followConversation];
    return;
  }
  // The panel's column fills the card: the conversation, the stretched row
  // of its layout, takes the height the other rows leave, as in the Qt card.
  if ([content isKindOfClass:NSStackView.class]) ((NSStackView*)content).distribution = NSStackViewDistributionFill;
  content.translatesAutoresizingMaskIntoConstraints = NO;
  [host addSubview:content];
  [NSLayoutConstraint activateConstraints:@[
    [content.leadingAnchor constraintEqualToAnchor:host.leadingAnchor],
    [content.trailingAnchor constraintEqualToAnchor:host.trailingAnchor],
    [content.topAnchor constraintEqualToAnchor:host.topAnchor],
    [content.bottomAnchor constraintEqualToAnchor:host.bottomAnchor]
  ]];
  host.needsLayout = YES;
}

bool focusChatCard(NSView* host) {
  if (host.window == nil || host.isHiddenOrHasHiddenAncestor) return false;
  [host layoutSubtreeIfNeeded];
  NSView* field = nil;
  const auto find = [&field](auto&& self, NSView* view) -> void {
    if (field != nil || view.hidden) return;
    if ([view isKindOfClass:NSTextView.class] && ((NSTextView*)view).editable) {
      field = view;
      return;
    }
    for (NSView* child in view.subviews) self(self, child);
  };
  find(find, host);
  return field != nil && [host.window makeFirstResponder:field];
}

} // namespace mitcad::mac
