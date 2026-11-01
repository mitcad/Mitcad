// SPDX-License-Identifier: MIT
#include "NativeColumnSplit.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>

#include "framework/ColumnShare.hpp"

namespace {

NSCursor* rowResizeCursor() {
  if (@available(macOS 15.0, *)) {
    return NSCursor.rowResizeCursor;
  }
  return NSCursor.resizeUpDownCursor;
}

} // namespace

// The boundary between two left cards (mitcad#1020): placed by the column
// in the gap between them; a drag reports how far the pointer moved down.
@interface MitcadColumnSplitHandle : NSView {
@public
  std::function<void(mitcad::mac::NativeSplitPhase, CGFloat)> dragged;
}
@end

@implementation MitcadColumnSplitHandle {
  NSTrackingArea* _cursorArea;
  CGFloat _startY;
  BOOL _dragging;
}
- (BOOL)isFlipped { return YES; }
- (BOOL)acceptsFirstMouse:(NSEvent*)event { (void)event; return YES; }
- (BOOL)mouseDownCanMoveWindow { return NO; }
- (void)setFrame:(NSRect)frame {
  [super setFrame:frame];
  [self.window invalidateCursorRectsForView:self];
}
- (void)resetCursorRects {
  [self addCursorRect:self.bounds cursor:rowResizeCursor()];
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
  [rowResizeCursor() set];
}
- (void)mouseDown:(NSEvent*)event {
  if (event.clickCount >= 2) {
    _dragging = NO;
    if (dragged) dragged(mitcad::mac::NativeSplitPhase::Reset, 0);
    return;
  }
  _dragging = YES;
  _startY = event.locationInWindow.y;
  if (dragged) dragged(mitcad::mac::NativeSplitPhase::Began, 0);
}
- (void)mouseDragged:(NSEvent*)event {
  // Window coordinates grow upwards: down is the start less the pointer.
  if (_dragging && dragged) dragged(mitcad::mac::NativeSplitPhase::Moved, _startY - event.locationInWindow.y);
}
- (void)mouseUp:(NSEvent*)event {
  if (!_dragging) return;
  _dragging = NO;
  if (dragged) dragged(mitcad::mac::NativeSplitPhase::Ended, _startY - event.locationInWindow.y);
}
@end

namespace mitcad::mac {

std::vector<CGFloat> shareNativeColumn(const std::vector<NativeColumnCard>& cards, CGFloat height,
                                       const NativeColumnSplit& split) {
  std::vector<ColumnClaim> claims;
  int open = 0;
  for (const NativeColumnCard& card : cards) {
    ColumnClaim claim;
    claim.want = int(std::ceil(card.want));
    claim.floor = int(std::ceil(std::min(card.least, card.want)));
    claim.fixed = card.collapsed;
    open += card.collapsed ? 0 : 1;
    claims.push_back(claim);
  }
  const int column = std::max(0, int(std::floor(height)));
  if (!split.empty()) {
    // The user's heights are the shares' weights, as in the Floating
    // layout; a card shown since gets an even share of the column.
    const double even = double(std::max(1, column)) / std::max(1, open);
    for (std::size_t i = 0; i < cards.size(); ++i) {
      const auto found = std::find_if(split.begin(), split.end(), [&cards, i](const std::pair<QString, int>& kept) {
        return kept.first == cards[i].name;
      });
      claims[i].weight = found != split.end() ? std::max(1, found->second) : even;
    }
  }
  const std::vector<int> shared = shareColumn(claims, column);
  return std::vector<CGFloat>(shared.begin(), shared.end());
}

NSView* makeColumnSplitHandle(std::function<void(NativeSplitPhase, CGFloat)> dragged) {
  MitcadColumnSplitHandle* handle = [[MitcadColumnSplitHandle alloc] initWithFrame:NSZeroRect];
  handle->dragged = std::move(dragged);
  handle.accessibilityElement = YES;
  handle.accessibilityRole = NSAccessibilitySplitterRole;
  handle.hidden = YES;
  return handle;
}

void detachColumnSplitHandle(NSView* handle) {
  if ([handle isKindOfClass:MitcadColumnSplitHandle.class]) ((MitcadColumnSplitHandle*)handle)->dragged = nullptr;
}

} // namespace mitcad::mac
