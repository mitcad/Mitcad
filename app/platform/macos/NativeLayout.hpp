// SPDX-License-Identifier: MIT
#pragma once

#import <AppKit/AppKit.h>

#include <functional>
#include <vector>

#include <QString>

class QDialogButtonBox;
class QLayout;
class QLayoutItem;

// The shared sizing contract of converted Qt layouts (mitcad#812). Grids,
// forms and box layouts become one constraint-based AppKit container that
// keeps Qt's minimum and preferred sizes, stretch factors, size policies,
// spans, field growth, label alignment, spacing and margins. Hidden items
// leave no space, as hidden widgets do in their Qt layout.
namespace mitcad::mac::layout {

// Where a view sits in its cell along one axis when it is smaller than it.
enum class Place { Leading, Center, Trailing };

// QFormLayout::FieldGrowthPolicy of a form field.
enum class FieldGrowth { All, Expanding, None };

// One axis of an item's contract, in points; negative values mean none.
struct Axis {
  CGFloat minimum = 0;
  CGFloat preferred = -1;
  CGFloat maximum = -1;
  bool cap = false;      // never larger than preferred (or its own content)
  bool grows = true;     // its cell may take extra space
  bool expands = false;  // claims extra space before cells that only grow
  bool fill = true;      // the view fills its cell
  Place place = Place::Center;
};

struct Item {
  NSView* view = nil;
  int row = 0;
  int column = 0;
  int rowSpan = 1;
  int columnSpan = 1;
  Axis horizontal;
  Axis vertical;
  bool spacer = false;     // takes space; never content, never spaced
  bool nested = false;     // a nested layout, empty when all its items are
  bool entry = false;      // its width follows the shared hints, not its text
  bool formLabel = false;  // top-limited in a tall form row, as in Qt
  bool formField = false;
  bool regime = true;      // takes part in choosing the growing tracks
  FieldGrowth growth = FieldGrowth::All;
  CGFloat labelLimit = 0;  // a form label's largest top offset in its row
};

struct Track {
  int stretch = 0;
  CGFloat minimum = 0;
};

struct Grid {
  std::vector<Item> items;
  std::vector<Track> columns;
  std::vector<Track> rows;
  NSEdgeInsets margins{0, 0, 0, 0};
  CGFloat horizontalSpacing = 8;
  CGFloat verticalSpacing = 8;
  Place content = Place::Leading;  // the columns' place when none grows
  bool alwaysGrows = false;        // QFormLayout's unbounded maximum
  bool equalColumns = false;
  CGFloat wrapWidth = 0; // opt-in: stack row cells below the shared layout hint
};

// Renders a shared Qt layout. `render` makes the native view of a widget or
// nested layout item; spacer items become empty views here. A native grid
// may give its first column the spare width while keeping later columns
// content-sized, without modifying the shared Qt layout.
NSView* build(QLayout* layout, const std::function<NSView*(QLayoutItem*)>& render, bool equalColumns,
              bool responsive = false, bool firstColumnStretch = false);

// Suspend constraints below hidden ancestors as one batch, and invalidate
// restored layouts. Call after a presenter's visibility updates and before
// measuring or solving the full native subtree.
void updateVisibility(NSView* root);

// Button boxes keep the macOS visual role order (mitcad#817): help, reset,
// apply and other helper actions on the left; destructive, alternate,
// reject and the primary accept action on the right. Entries are indices
// into `roles` (QDialogButtonBox::ButtonRole values in buttons() order) or
// the two markers below.
constexpr int kStretch = -1;
constexpr int kGap = -2;
std::vector<int> macButtonOrder(const std::vector<int>& roles);

// The box's buttons and roles, to notice additions or role changes later.
QString buttonRoleKey(const QDialogButtonBox* box);

// An empty button row, then (again after any change) the native views of
// `box`'s buttons in buttons() order, nil for a button without one.
NSView* buttonRow();
void arrangeButtons(NSView* row, const QDialogButtonBox* box, const std::vector<NSView*>& buttons);

} // namespace mitcad::mac::layout

@interface MitcadNativeLayoutView : NSView
// An owner's description of the source content, compared to notice changes.
@property(nonatomic, copy) NSString* contentKey;
- (void)applyGrid:(const mitcad::mac::layout::Grid&)grid;
// True when no item other than spacers is shown, as QLayout::isEmpty.
- (BOOL)layoutEmpty;
// As QLayout's maximum size and expanding directions, for its shown items.
- (BOOL)growsAlong:(BOOL)horizontal;
- (BOOL)expandsAlong:(BOOL)horizontal;
@end
