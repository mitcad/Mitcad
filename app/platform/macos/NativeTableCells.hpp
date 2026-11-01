// SPDX-License-Identifier: MIT
#pragma once

// How native AppKit tables present a shared Qt item view's semantics
// (mitcad#814): its cells' model roles, single-line elision and the
// header's column sizes, resize modes and hidden sections. Objective-C++
// (ARC) only; the generic presenter bridge and other native views share it.

#import <AppKit/AppKit.h>

#include <cstddef>
#include <map>
#include <vector>

#include <QtCore/qnamespace.h>

class QAbstractItemView;
class QHeaderView;
class QIcon;
class QModelIndex;
class QString;
class QWidget;

// A checklist's cursor keeps warning colours intact by framing the current
// row without selecting it. Its actual native drawing state is testable.
@interface MitcadNativeCurrentRowView : NSTableRowView
@property(nonatomic) BOOL currentFocus;
// Last draw, for focused rendering assertions; never drives presentation.
@property(nonatomic, readonly) BOOL paintedCurrentFocus;
@end

namespace mitcad::mac {

// A cell's font and colours from the shared roles: NativeRefusedRole (the
// error text and fill), Qt::FontRole, Qt::ForegroundRole and
// Qt::BackgroundRole. Shared semantic colours (error, warning, the
// palette's disabled and placeholder text) become AppKit's dynamic system
// colours, so they follow the appearance. nil text: the label colour; nil
// fill: none.
struct NativeCellStyle {
  NSFont* font = nil;
  NSColor* text = nil;
  NSColor* fill = nil;
};
NativeCellStyle nativeCellStyle(const QModelIndex& index, const QAbstractItemView* view);

// One line, elided as the view's text elide mode says, never wrapped into a
// fixed-height row.
void elideNativeCell(NSTextField* field, Qt::TextElideMode mode);

// A label's tone for its kSemanticTextToneProperty (Theme.hpp): 0 none,
// 1 error, 2 warning; and its text colour.
int nativeTextToneKind(const QWidget* widget);
NSColor* nativeTextTone(int kind);

// The horizontal header of a tree or table view; nullptr for a list.
QHeaderView* nativeHeader(const QAbstractItemView* view);

// What applyNativeColumns took from the shared header last: a signature of
// its state and the section sizes. AppKit's own values are not compared,
// so an unchanged refresh writes nothing.
struct NativeColumnState {
  std::size_t signature = 0;
  std::vector<int> sizes;
  // Explicit one-time content requests cache native sizes independently
  // of later user resizes. A shared section resize ends its cached request.
  quint64 contentsRequest = 0;
  std::vector<int> requestedSizes;
  std::vector<int> requestedSharedSizes;
};

// Applies the header to the table's columns when its state changed:
// titles and header tooltips, hidden sections, the shared section sizes
// (ResizeToContents also fits the native header/font/cell insets),
// including an outline header's native disclosure indentation when AppKit
// draws it inside the reported header rectangle,
// and which columns stretch (QHeaderView::Stretch and a stretched last
// section). A section's width is set when its shared size changes, so a
// native user resize stays until then. True when it applied anything.
bool applyNativeColumns(NSTableView* table, const QAbstractItemView* view, NativeColumnState& state);

// Gives the stretching columns the width the others leave in the visible
// area, down to their minimum width; beyond that the table scrolls.
void fitNativeColumns(NSTableView* table);
// Refits when the visible area or a column's width changes.
void followNativeColumns(NSTableView* table);

// Editors in logical column order, converted from the shared header's
// NativeColumnEditorsProperty. They move and resize with their columns and
// scroll horizontally with the header, above the table's ordinary cells.
void installNativeColumnEditors(NSTableView* table, const std::vector<NSView*>& editors);

// The table a view is in, and the text field of a table cell that has the
// field editor now (nil when none).
NSTableView* owningTable(NSView* view);
NSTextField* editedCellField(NSTableView* table);
// Selects a field's whole text. A field that took the focus already has its
// field editor: selectText: would end that edit and begin another, which
// commits the cell (mitcad#814).
void selectEditedText(NSTextField* field);

// MITCAD_LOG_NATIVE_EDITS=1 logs how native cell edits and table focus
// change, with the calling frames ("Native cell edit: ..."), so that a test
// shows what ended an edit; nothing otherwise.
void logNativeCellEdit(const char* what, NSView* view);

// An item's or a header's icon as an image at most `points` high, drawn at
// twice that for Retina screens; nil for none.
NSImage* nativeIconImage(const QIcon& icon, CGFloat points);

// A table's vertical header (mitcad#1078): a QTableView that sets
// NativeRowHeaderProperty and does not hide its vertical header shows it as
// a leading column of row labels with their icons (headerData(row,
// Qt::Vertical) and its DecorationRole), as Purge Volumes' "From PLA" rows. A label's identifier is its header's
// NativeControlIdentifierRole, else the view's objectName plus ".row.<row>".
// The column's identifier is no model column's.
bool nativeRowHeaderShown(const QAbstractItemView* view);
bool isNativeRowHeader(NSTableColumn* column);
NSTableColumn* createNativeRowHeader();
NSView* nativeRowHeaderCell(const QAbstractItemView* view, int row);
// The leading column as wide as its widest label.
void fitNativeRowHeader(NSTableView* table, const QAbstractItemView* view);

// Numbers on a decimal axis (NativeDecimalColumnsProperty, mitcad#1080):
// where a column's decimal separators and units are, in points from its
// texts' start, measured over all of the view's rows in tabular digits,
// as DecimalAlignDelegate places them.
struct NativeDecimalColumn {
  CGFloat axis = 0;
  CGFloat unitStart = 0;
};
std::map<int, NativeDecimalColumn> nativeDecimalColumns(const QAbstractItemView* view);
// A cell's text on its column's axis, in `font`'s tabular digits; nil when
// it is not one number with an optional unit. Its string is the text as it
// is: the indent and the unit's place are attributes.
NSAttributedString* nativeDecimalText(const QString& text, const NativeDecimalColumn& column, NSFont* font,
                                      NSColor* color, NSLineBreakMode mode);

} // namespace mitcad::mac
