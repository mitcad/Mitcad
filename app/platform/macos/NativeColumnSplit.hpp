// SPDX-License-Identifier: MIT
#pragma once

// The AppKit frontend's left column (mitcad#1020): Projects, Configuration
// and Browser share its free height, from below the ribbon to above the
// timeline, as the Floating layout shares its left column (ColumnShare.hpp):
// a card whose content is shorter than its share keeps its content's
// height and gives the rest to the others; a collapsed card has its
// header's. The boundary between two open neighbours is dragged; the
// heights then are the shares' weights, remembered in the settings
// ("native/leftSplit"); a double-click returns to the automatic split.
// Included by Objective-C++ sources only.

#import <AppKit/AppKit.h>

#include <functional>
#include <utility>
#include <vector>

#include <QString>

namespace mitcad::mac {

// A left card's wish: its name ("projects", "configuration", "browser"),
// its content's height with its header and margins (`want`), its header
// and a row (`least`); a collapsed card's `want` is its header's height.
struct NativeColumnCard {
  QString name;
  CGFloat want = 0;
  CGFloat least = 0;
  bool collapsed = false;
};

// The user's heights of the open cards by name, in their order; empty:
// the automatic split.
using NativeColumnSplit = std::vector<std::pair<QString, int>>;

// The cards' heights in a column `height` points high (the gaps between
// them left out), `split` giving the shares' weights; a card shown since
// the split was made gets an even share.
std::vector<CGFloat> shareNativeColumn(const std::vector<NativeColumnCard>& cards, CGFloat height,
                                       const NativeColumnSplit& split);

enum class NativeSplitPhase { Began, Moved, Ended, Reset };

// A handle in the gap between two left cards, which the column places: a
// pointer drag on it shows the row resize cursor and reports how far the
// pointer moved down since the press (Began, Moved, Ended), a double-click
// Reset. An accessibility splitter.
NSView* makeColumnSplitHandle(std::function<void(NativeSplitPhase phase, CGFloat down)> dragged);
// Drops the handle's report, before its receiver goes away.
void detachColumnSplitHandle(NSView* handle);

} // namespace mitcad::mac
