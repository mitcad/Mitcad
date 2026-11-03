// SPDX-License-Identifier: MIT
#include "platform/NativeBindingsCard.hpp"
#include "platform/NativeDialogs.hpp"

#import <AppKit/AppKit.h>

#include <QPointer>
#include <QWidget>
#include <algorithm>
#include <utility>

namespace mitcad { class AppKitBindingsCard; }

@interface MitcadBindingsPopoverDelegate : NSObject <NSPopoverDelegate> {
@public
  mitcad::AppKitBindingsCard* owner;
}
@end

namespace mitcad {

class AppKitBindingsCard final : public NativeBindingsCardView {
public:
  AppKitBindingsCard(QWidget* source, std::function<void()> closed)
      : m_source(source), m_closed(std::move(closed)) {
    m_popover = [NSPopover new];
    m_popover.animates = NO;
    m_delegate = [MitcadBindingsPopoverDelegate new];
    m_delegate->owner = this;
    m_popover.delegate = m_delegate;
    m_scroll = [[NSScrollView alloc] initWithFrame:NSMakeRect(0, 0, 460, 200)];
    m_scroll.hasVerticalScroller = YES;
    m_scroll.autohidesScrollers = YES;
    m_scroll.drawsBackground = NO;
    m_scroll.accessibilityIdentifier = @"native.bindingsCard.scroll";
    NSViewController* controller = [NSViewController new];
    controller.view = m_scroll;
    m_popover.contentViewController = controller;
  }

  ~AppKitBindingsCard() override {
    hide();
    m_delegate->owner = nullptr;
    m_popover.delegate = nil;
  }

  void show(const QRect& anchor, bool pinned) override {
    NSWindow* window = (__bridge NSWindow*)nativeDialogWindow(m_source);
    if (!m_source || !window.visible || !NSApp.active) return;
    if (!m_view) {
      m_view = createNativeWidgetView(m_source);
      NSView* root = (__bridge NSView*)m_view->nativeView();
      root.translatesAutoresizingMaskIntoConstraints = NO;
      m_scroll.documentView = root;
      m_height = [root.heightAnchor constraintEqualToConstant:1];
      [NSLayoutConstraint activateConstraints:@[
        [root.leadingAnchor constraintEqualToAnchor:m_scroll.contentView.leadingAnchor],
        [root.topAnchor constraintEqualToAnchor:m_scroll.contentView.topAnchor],
        [root.widthAnchor constraintEqualToAnchor:m_scroll.contentView.widthAnchor], m_height
      ]];
    }
    refresh();
    if (m_open) hide();
    m_popover.behavior = pinned ? NSPopoverBehaviorTransient : NSPopoverBehaviorApplicationDefined;
    const CGFloat top = NSMaxY(NSScreen.screens.firstObject.frame);
    const NSRect screen = NSMakeRect(anchor.left(), top - anchor.bottom() - 1,
                                     std::max(1, anchor.width()), std::max(1, anchor.height()));
    const NSRect local = [window.contentView convertRect:[window convertRectFromScreen:screen] fromView:nil];
    m_open = true;
    [m_popover showRelativeToRect:local ofView:window.contentView preferredEdge:NSRectEdgeMinY];
    m_scroll.window.identifier = @"native.bindingsCardWindow";
    m_scroll.window.title = m_source->windowTitle().toNSString();
    if (m_monitor == nil) {
      m_monitor = [NSEvent addLocalMonitorForEventsMatchingMask:NSEventMaskKeyDown |
          NSEventMaskLeftMouseDown | NSEventMaskRightMouseDown handler:^NSEvent*(NSEvent* event) {
        if (!m_open) return event;
        if (event.type == NSEventTypeKeyDown && event.keyCode == 53 &&
            !(event.modifierFlags & (NSEventModifierFlagCommand | NSEventModifierFlagControl |
                                    NSEventModifierFlagOption | NSEventModifierFlagShift))) {
          closed();
          return nil;
        }
        if (event.type != NSEventTypeKeyDown) {
          const NSPoint pointer = event.window ? [event.window convertPointToScreen:event.locationInWindow]
                                               : NSEvent.mouseLocation;
          if (!NSPointInRect(pointer, m_scroll.window.frame)) closed();
        }
        return event;
      }];
    }
  }

  void hide() override {
    m_open = false;
    if (m_monitor != nil) {
      [NSEvent removeMonitor:m_monitor];
      m_monitor = nil;
    }
    [m_popover close];
  }

  void closed() {
    if (!m_open) return;
    hide();
    const auto notify = m_closed;
    if (notify) notify();
  }

  void refresh() override {
    if (!m_view) return;
    m_view->refresh();
    NSView* root = (__bridge NSView*)m_view->nativeView();
    m_height.active = NO;
    [root layoutSubtreeIfNeeded];
    const CGFloat wanted = std::max(CGFloat(40), root.fittingSize.height);
    m_height.constant = wanted;
    m_height.active = YES;
    NSWindow* document = (__bridge NSWindow*)nativeDialogWindow(m_source);
    NSScreen* screen = document.screen != nil ? document.screen : NSScreen.mainScreen;
    const CGFloat available = std::max(CGFloat(160), screen.visibleFrame.size.height - 80);
    m_popover.contentSize = NSMakeSize(460, std::min(wanted, available));
  }

  bool contains(const QPoint& point) const override {
    if (!m_open || !m_scroll.window.visible) return false;
    const CGFloat top = NSMaxY(NSScreen.screens.firstObject.frame);
    return NSPointInRect(NSMakePoint(point.x(), top - point.y()), NSInsetRect(m_scroll.window.frame, -8, -8));
  }

private:
  QPointer<QWidget> m_source;
  std::function<void()> m_closed;
  std::unique_ptr<NativeWidgetView> m_view;
  NSPopover* __strong m_popover = nil;
  MitcadBindingsPopoverDelegate* __strong m_delegate = nil;
  NSScrollView* __strong m_scroll = nil;
  NSLayoutConstraint* __strong m_height = nil;
  id __strong m_monitor = nil;
  bool m_open = false;
};

std::unique_ptr<NativeBindingsCardView> createNativeBindingsCardView(QWidget* source,
                                                                  std::function<void()> closed) {
  if (!nativeDialogsActive()) return {};
  return std::make_unique<AppKitBindingsCard>(source, std::move(closed));
}

} // namespace mitcad

@implementation MitcadBindingsPopoverDelegate
- (void)popoverDidClose:(NSNotification*)notification {
  (void)notification;
  if (owner) owner->closed();
}
@end
