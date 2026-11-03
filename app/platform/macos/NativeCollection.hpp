// SPDX-License-Identifier: MIT
#pragma once

// AppKit presentations that the generic presenter bridge (NativeDialogs.mm)
// uses where converting widgets to rows and stacks would lose capabilities
// (docs/native-macos-ui.md, mitcad#813, mitcad#819):
// - an icon-mode QListView as an NSCollectionView tile grid: the model's
//   pictures, captions, stars, use counts and details, the view's tile
//   size and selection, clicks, activation and the keyboard;
// - a tile catalogue, a widget with the "nativeTileCatalogue" property (the
//   part picker's PartGrid): its group tree as a source list with stars
//   and folding, its rows of filters and its grid, in usable bounds;
// - QSplitter as panes in its sizes and stretch, with draggable dividers;
//   user sizes follow the retained presenter across native view rebuilds;
// - a modeless presenter with "nativeUtilityWindow" in a floating panel
//   that leaves the keyboard with the document until a field needs it.
// The presenters' roles and methods are named by properties; the list is
// in docs/native-macos-ui.md. Objective-C++ only: the platform's .mm files.

#import <AppKit/AppKit.h>

#include <functional>
#include <memory>
#include <vector>

#include "../NativeDialogs.hpp"

class QSplitter;
class QAbstractItemView;
class QWidget;

namespace mitcad::mac {
class NativeTileGrid;
class NativeGroupList;
} // namespace mitcad::mac

// A tile of a native grid: picture, caption, sizes, star and use count.
// Public for the in-process test probes (NativeTestInput.mm).
@interface MitcadNativeTileView : NSView {
@public
  mitcad::mac::NativeTileGrid* owner;
  NSInteger tile;
  CGFloat side;
  BOOL chosen;
  BOOL hasPicture;
  BOOL starred;
  NSString* key;
  NSString* badge;
  NSImageView* picture;
  NSTextField* caption;
  NSTextField* detail;
  NSButton* star;
  NSTrackingArea* hoverArea;
}
- (void)refreshCaption;
@end

@interface MitcadNativeTileCollection : NSCollectionView {
@public
  mitcad::mac::NativeTileGrid* owner;
}
@end

@interface MitcadNativeGroupOutline : NSOutlineView {
@public
  mitcad::mac::NativeGroupList* owner;
}
@end

// The floating panel of a utility presenter. Its window delegate answers
// the presenter's own shortcuts first (MitcadNativeShortcutTarget).
@interface MitcadNativeUtilityPanel : NSPanel
@end

@protocol MitcadNativeShortcutTarget <NSObject>
- (BOOL)mitcadDispatchShortcut:(NSEvent*)event;
@end

namespace mitcad::mac {

// Renders a presenter widget through the generic bridge, which keeps its
// bindings (text, state, visibility).
using NativeWidgetRenderer = std::function<NSView*(QWidget*)>;

// A tile grid for an icon-mode QListView; null for any other view.
std::unique_ptr<NativeWidgetView> createNativeTileGrid(QAbstractItemView* view);
// The catalogue presentation of `widget`, or nil when it is none. The
// object that keeps it bound is appended to `owned`.
NSView* nativeTileCatalogue(QWidget* widget, const NativeWidgetRenderer& render,
                            std::vector<std::unique_ptr<NativeWidgetView>>& owned);
// The splitter's panes side by side (or stacked), in its sizes and stretch
// factors, with draggable dividers.
NSView* nativeSplitView(QSplitter* splitter, const NativeWidgetRenderer& render);
// A titled, closable, resizable utility panel of this content size.
NSWindow* nativeUtilityPanel(NSRect content);
// The panel's frame from the presenter's "nativeFrameKey" setting, else
// centred on the host window, within the screen's visible area; and saving
// it when the panel closes.
void restoreNativeUtilityFrame(NSWindow* window, QWidget* presenter, NSWindow* host);
void saveNativeUtilityFrame(NSWindow* window, QWidget* presenter);

} // namespace mitcad::mac
