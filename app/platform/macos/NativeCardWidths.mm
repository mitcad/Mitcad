// SPDX-License-Identifier: MIT
#include "NativeCardWidths.hpp"

#import <objc/runtime.h>

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <utility>

#include <QObject>

namespace {

// fitCardOutline's measure of an outline style's insets around its columns.
char g_outlineInsetKey = 0;

constexpr CGFloat kHandleWidth = 6;
// The cards' corner radius: the handle stays within the rounded rectangle
// that takes the card's pointer input.
constexpr CGFloat kHandleInset = 14;

NSCursor* resizeCursor() {
  if (@available(macOS 15.0, *)) {
    return NSCursor.columnResizeCursor;
  }
  return NSCursor.resizeLeftRightCursor;
}

} // namespace

// The handle along a card's inner edge (mitcad#818). It places itself as
// the card's frame changes; a drag reports the width asked for.
@interface MitcadCardWidthHandle : NSView {
@public
  std::function<void(CGFloat, bool)> dragged;
  BOOL leadingEdge; // the card's left edge (the inspector); else its right edge
}
- (void)cardResized:(NSNotification*)notification;
@end

@implementation MitcadCardWidthHandle {
  NSTrackingArea* _cursorArea;
  CGFloat _startX;
  CGFloat _startWidth;
  BOOL _dragging;
}
- (BOOL)isFlipped { return YES; }
- (BOOL)acceptsFirstMouse:(NSEvent*)event { (void)event; return YES; }
- (BOOL)mouseDownCanMoveWindow { return NO; }
- (void)dealloc {
  [NSNotificationCenter.defaultCenter removeObserver:self];
}
- (void)viewDidMoveToSuperview {
  [super viewDidMoveToSuperview];
  [NSNotificationCenter.defaultCenter removeObserver:self name:NSViewFrameDidChangeNotification object:nil];
  NSView* card = self.superview;
  if (card == nil) return;
  card.postsFrameChangedNotifications = YES;
  [NSNotificationCenter.defaultCenter addObserver:self selector:@selector(cardResized:)
                                             name:NSViewFrameDidChangeNotification object:card];
  [self cardResized:nil];
}
- (void)cardResized:(NSNotification*)notification {
  (void)notification;
  const NSRect bounds = self.superview.bounds;
  const CGFloat x = leadingEdge ? NSMinX(bounds) : NSMaxX(bounds) - kHandleWidth;
  self.frame = NSMakeRect(x, NSMinY(bounds) + kHandleInset, kHandleWidth,
                          std::max<CGFloat>(0, bounds.size.height - 2 * kHandleInset));
  [self.window invalidateCursorRectsForView:self];
}
- (void)resetCursorRects {
  [self addCursorRect:self.bounds cursor:resizeCursor()];
}
- (void)updateTrackingAreas {
  [super updateTrackingAreas];
  if (_cursorArea == nil) {
    _cursorArea = [[NSTrackingArea alloc] initWithRect:NSZeroRect
                                               options:NSTrackingCursorUpdate | NSTrackingActiveInKeyWindow |
                                                       NSTrackingInVisibleRect
                                                 owner:self userInfo:nil];
    [self addTrackingArea:_cursorArea];
  }
}
- (void)cursorUpdate:(NSEvent*)event {
  (void)event;
  [resizeCursor() set];
}
- (void)mouseDown:(NSEvent*)event {
  _dragging = YES;
  _startX = event.locationInWindow.x;
  _startWidth = self.superview.frame.size.width;
}
- (void)report:(NSEvent*)event finished:(bool)finished {
  const CGFloat delta = event.locationInWindow.x - _startX;
  const CGFloat width = _startWidth + (leadingEdge ? -delta : delta);
  if (dragged) dragged(width, finished);
}
- (void)mouseDragged:(NSEvent*)event {
  if (_dragging) [self report:event finished:false];
}
- (void)mouseUp:(NSEvent*)event {
  if (!_dragging) return;
  _dragging = NO;
  [self report:event finished:true];
}
@end

namespace mitcad::mac {
namespace {

std::size_t indexOf(NativeCard card) { return static_cast<std::size_t>(card); }

} // namespace

QString nativeCardName(NativeCard card) {
  switch (card) {
  case NativeCard::Projects: return QStringLiteral("projects");
  case NativeCard::Browser: return QStringLiteral("browser");
  case NativeCard::Inspector: return QStringLiteral("inspector");
  }
  return QString();
}

bool nativeCardNamed(const QString& name, NativeCard& card) {
  for (NativeCard candidate : {NativeCard::Projects, NativeCard::Browser, NativeCard::Inspector}) {
    if (nativeCardName(candidate) == name) {
      card = candidate;
      return true;
    }
  }
  return false;
}

CGFloat NativeCardWidths::minimum(NativeCard card) {
  switch (card) {
  case NativeCard::Projects: return 230; // its eight 24-point toolbar buttons
  case NativeCard::Browser: return 180;  // as ProjectsPanel::kMinimumCardWidth
  case NativeCard::Inspector: return 300;
  }
  return 180;
}

CGFloat NativeCardWidths::standard(NativeCard card) { return card == NativeCard::Inspector ? 300 : 260; }

CGFloat NativeCardWidths::width(NativeCard card) const { return m_widths[indexOf(card)]; }

void NativeCardWidths::restore(NativeCard card, CGFloat width) {
  m_widths[indexOf(card)] = std::max(minimum(card), std::round(width));
}

NativeCardPlacement NativeCardWidths::place(CGFloat canvasWidth, bool projectsShown, bool browserShown,
                                            bool inspectorShown) const {
  const CGFloat room = std::max<CGFloat>(0, canvasWidth - 2 * margin);
  const auto limited = [&](NativeCard card) {
    const CGFloat most = std::max(minimum(card), std::floor(room * 2 / 3));
    return std::clamp(width(card), minimum(card), most);
  };
  NativeCardPlacement placed;
  placed.projects = limited(NativeCard::Projects);
  placed.browser = limited(NativeCard::Browser);
  placed.inspector = limited(NativeCard::Inspector);
  const CGFloat left = std::max<CGFloat>(projectsShown ? placed.projects : 0, browserShown ? placed.browser : 0);
  if (left > 0 && inspectorShown) {
    // Both sides share what the free canvas leaves: the inspector yields
    // first, then the left column.
    CGFloat excess = left + placed.inspector - (room - freeCanvas);
    if (excess > 0) {
      const CGFloat yielded = std::min(excess, placed.inspector - minimum(NativeCard::Inspector));
      placed.inspector -= yielded;
      excess -= yielded;
    }
    if (excess > 0) {
      const CGFloat column = left - excess;
      placed.projects = std::max(minimum(NativeCard::Projects), std::min(placed.projects, column));
      placed.browser = std::max(minimum(NativeCard::Browser), std::min(placed.browser, column));
    }
  }
  return placed;
}

CGFloat NativeCardWidths::drag(NativeCard card, CGFloat width, CGFloat canvasWidth, bool projectsShown,
                               bool browserShown, bool inspectorShown) {
  const CGFloat room = std::max<CGFloat>(0, canvasWidth - 2 * margin);
  CGFloat most = std::max(minimum(card), std::floor(room * 2 / 3));
  // The other side's cards as they are placed now keep their room.
  const NativeCardPlacement placed = place(canvasWidth, projectsShown, browserShown, inspectorShown);
  if (card == NativeCard::Inspector) {
    const CGFloat left = std::max<CGFloat>(projectsShown ? placed.projects : 0, browserShown ? placed.browser : 0);
    if (left > 0) most = std::min(most, room - freeCanvas - left);
  } else if (inspectorShown) {
    most = std::min(most, room - freeCanvas - placed.inspector);
  }
  const CGFloat chosen = std::round(std::clamp(width, minimum(card), std::max(minimum(card), most)));
  m_widths[indexOf(card)] = chosen;
  return chosen;
}

NSView* addCardWidthHandle(NSView* card, NativeCard which, std::function<void(CGFloat, bool)> dragged) {
  MitcadCardWidthHandle* handle = [[MitcadCardWidthHandle alloc] initWithFrame:NSZeroRect];
  handle->leadingEdge = which == NativeCard::Inspector;
  handle->dragged = std::move(dragged);
  handle.accessibilityElement = YES;
  handle.accessibilityRole = NSAccessibilitySplitterRole;
  handle.accessibilityLabel = (which == NativeCard::Projects  ? QObject::tr("Projects width")
                               : which == NativeCard::Browser ? QObject::tr("Browser width")
                                                              : QObject::tr("Command panel width"))
                                  .toNSString();
  handle.accessibilityIdentifier = [card.accessibilityIdentifier stringByAppendingString:@".width"];
  [card addSubview:handle positioned:NSWindowAbove relativeTo:nil];
  return handle;
}

void detachCardWidthHandle(NSView* handle) {
  if ([handle isKindOfClass:MitcadCardWidthHandle.class]) {
    ((MitcadCardWidthHandle*)handle)->dragged = nullptr;
  }
}

void fitCardOutline(NSView* card, NSOutlineView* outline) {
  if (card == nil || card.hidden || outline == nil) return;
  [card layoutSubtreeIfNeeded];
  NSScrollView* scroll = outline.enclosingScrollView;
  NSArray<NSTableColumn*>* columns = outline.tableColumns;
  if (scroll == nil || [scroll isHiddenOrHasHiddenAncestor] || columns.count == 0) return;
  const CGFloat available = scroll.contentView.bounds.size.width;
  if (available <= 0) return;
  // The rows no wider than the viewport: the columns, their spacing and the
  // insets the table's style adds around them (a source list's). AppKit
  // tiles the table as wide as all of them where that exceeds the
  // viewport; that measure of the insets is kept for the next layouts.
  CGFloat others = outline.intercellSpacing.width * static_cast<CGFloat>(columns.count);
  for (NSUInteger i = 1; i < columns.count; ++i) {
    if (!columns[i].hidden) others += columns[i].width;
  }
  NSNumber* known = objc_getAssociatedObject(outline, &g_outlineInsetKey);
  const CGFloat inset = known != nil ? known.doubleValue : 0;
  NSTableColumn* first = columns.firstObject;
  const CGFloat width = std::max(first.minWidth, std::floor(available - others - inset));
  if (std::abs(first.width - width) >= 0.5) first.width = width;
  [outline tile];
  const CGFloat excess = NSWidth(outline.frame) - available;
  if (excess > 0.5) {
    objc_setAssociatedObject(outline, &g_outlineInsetKey,
                             @(std::max<CGFloat>(0, NSWidth(outline.frame) - others - first.width)),
                             OBJC_ASSOCIATION_RETAIN_NONATOMIC);
    first.width = std::max(first.minWidth, first.width - std::ceil(excess));
    [outline tile];
  }
}

} // namespace mitcad::mac
