// SPDX-License-Identifier: MIT
#include "NativeTableCells.hpp"

#import <objc/runtime.h>

#include <algorithm>
#include <cmath>

#include <QAbstractItemModel>
#include <QAbstractItemView>
#include <QBrush>
#include <QBuffer>
#include <QByteArray>
#include <QColor>
#include <QFont>
#include <QHash>
#include <QHeaderView>
#include <QIcon>
#include <QModelIndex>
#include <QPalette>
#include <QPixmap>
#include <QSize>
#include <QSizeF>
#include <QTextDocument>
#include <QString>
#include <QStringList>
#include <QTableView>
#include <QTreeView>
#include <QVariant>
#include <QWidget>
#include <QtGlobal>
#include <QtLogging>

#include "../NativeDialogs.hpp"
#include "../../browser/DecimalAlign.hpp"
#include "../../framework/Theme.hpp"
#include "NativeColors.hpp"

// Refits a header-driven table's stretching columns after the visible area
// or a column changes. Owned by its table (an associated object).
@interface MitcadNativeColumnFit : NSObject {
@public
  __weak NSTableView* table;
  BOOL fitting;
}
- (void)fit:(NSNotification*)notification;
@end

// A header containing the shared column's name and unit editors. The
// ordinary NSTableHeaderView still handles resizing between the controls.
@interface MitcadNativeEditorHeader : NSTableHeaderView
@property(nonatomic, strong) NSArray<NSView*>* editors;
@end

@implementation MitcadNativeCurrentRowView
- (void)drawBackgroundInRect:(NSRect)dirtyRect {
  [super drawBackgroundInRect:dirtyRect];
  _paintedCurrentFocus = self.currentFocus;
  if (self.currentFocus) {
    [NSColor.controlAccentColor setStroke];
    NSBezierPath* frame = [NSBezierPath bezierPathWithRect:NSInsetRect(self.bounds, 1, 1)];
    frame.lineWidth = 2;
    [frame stroke];
  }
}
@end

@implementation MitcadNativeEditorHeader
- (void)mouseDown:(NSEvent*)event {
  // Own mouseDown when a header contains editors: the inherited header
  // tracking can swallow their presses before a popup starts tracking.
  // Preserve the actual event and leave column gaps/resizing to AppKit.
  NSView* hit = [self hitTest:[self.superview convertPoint:event.locationInWindow fromView:nil]];
  if ([hit isKindOfClass:NSControl.class]) {
    for (NSView* editor in self.editors) {
      if (hit == editor || [hit isDescendantOf:editor]) {
        [hit mouseDown:event];
        return;
      }
    }
  }
  [super mouseDown:event];
}
- (void)layout {
  [super layout];
  NSTableView* table = self.tableView;
  for (NSUInteger visual = 0; visual < table.tableColumns.count; ++visual) {
    NSTableColumn* column = table.tableColumns[visual];
    const NSInteger logical = column.identifier.integerValue;
    if (logical < 0 || logical >= static_cast<NSInteger>(self.editors.count)) { continue; }
    NSView* editor = self.editors[static_cast<NSUInteger>(logical)];
    editor.hidden = column.hidden;
    if (!column.hidden) {
      const NSRect columnRect = [self headerRectOfColumn:static_cast<NSInteger>(visual)];
      editor.frame = NSMakeRect(columnRect.origin.x + 3, 2, std::max<CGFloat>(0, columnRect.size.width - 6),
                                std::max<CGFloat>(0, self.bounds.size.height - 4));
    }
  }
}
- (void)drawRect:(NSRect)dirtyRect {
  [NSColor.windowBackgroundColor setFill];
  NSRectFill(dirtyRect);
}
@end

@implementation MitcadNativeColumnFit
- (void)fit:(NSNotification*)notification {
  (void)notification;
  NSTableView* target = table;
  if (target == nil || fitting) { return; }
  fitting = YES; // its own width changes post the same notification
  if (NSTextField* edited = mitcad::mac::editedCellField(target)) {
    mitcad::mac::logNativeCellEdit("the columns are fitted during an edit", edited);
  }
  mitcad::mac::fitNativeColumns(target);
  target.headerView.needsLayout = YES;
  [target.headerView layoutSubtreeIfNeeded];
  fitting = NO;
}
- (void)dealloc { [NSNotificationCenter.defaultCenter removeObserver:self]; }
@end

namespace mitcad::mac {
namespace {

char g_columnFitKey = 0;

std::size_t mixed(std::size_t seed, std::size_t value) {
  return seed ^ (value + static_cast<std::size_t>(0x9e3779b97f4a7c15ULL) + (seed << 6) + (seed >> 2));
}

// The tint of a semantic fill behind a cell's text, as the shared delegate
// mixes about a tenth of the error colour into the base.
constexpr CGFloat kFillAlpha = 0.12;

QPalette shade(bool dark) {
  QPalette palette;
  palette.setColor(QPalette::Window, dark ? QColor(Qt::black) : QColor(Qt::white));
  return palette;
}

QColor itemColor(const QVariant& value) {
  if (value.typeId() == QMetaType::QBrush) {
    const QBrush brush = value.value<QBrush>();
    return brush.style() == Qt::NoBrush ? QColor() : brush.color();
  }
  if (value.typeId() == QMetaType::QColor) { return value.value<QColor>(); }
  return {};
}

NSColor* srgb(const QColor& color) {
  const QColor rgb = color.toRgb();
  return [NSColor colorWithSRGBRed:rgb.redF() green:rgb.greenF() blue:rgb.blueF() alpha:rgb.alphaF()];
}

// A colour of the role table, on a light or a dark palette, is that role in
// AppKit (roleColor, mitcad#1041), which follows the appearance: the error
// text the system's red, the warning text the table's yellow, a fill its
// fill. The error's and warning's text colours used as a fill are a tenth of
// them. Other colours stay what they are.
NSColor* semanticColor(const QColor& color, const QPalette& palette, bool text) {
  if (!color.isValid()) { return nil; }
  for (const bool dark : {false, true}) {
    if (!text && (color == errorColor(shade(dark)) || color == warningColor(shade(dark)))) {
      const ColorRole role = color == errorColor(shade(dark)) ? ColorRole::ErrorText : ColorRole::WarningText;
      return [roleColor(role) colorWithAlphaComponent:kFillAlpha];
    }
  }
  for (const ColorRole role : allColorRoles()) {
    if (colorRoleUse(role) != ColorUse::View &&
        (color == themeColor(role, false) || color == themeColor(role, true))) {
      return roleColor(role);
    }
  }
  if (text) {
    if (color == palette.color(QPalette::Disabled, QPalette::Text) ||
        color == palette.color(QPalette::Disabled, QPalette::WindowText)) {
      return NSColor.disabledControlTextColor;
    }
    if (color == palette.color(QPalette::PlaceholderText)) { return NSColor.placeholderTextColor; }
    if (color == palette.color(QPalette::Text) || color == palette.color(QPalette::WindowText)) { return nil; }
  }
  return srgb(color);
}

NSFont* cellFont(const QModelIndex& index, const QAbstractItemView* view) {
  const CGFloat base = NSFont.systemFontSize;
  const QVariant value = index.data(Qt::FontRole);
  if (value.typeId() != QMetaType::QFont) { return [NSFont systemFontOfSize:base]; }
  const QFont font = value.value<QFont>();
  CGFloat size = base;
  const qreal reference = view->font().pointSizeF();
  if (font.pointSizeF() > 0 && reference > 0 && std::abs(font.pointSizeF() - reference) > 0.01) {
    size = base * font.pointSizeF() / reference; // a smaller or larger variant of the view's font
  }
  const int weight = static_cast<int>(font.weight());
  const NSFontWeight native = weight >= static_cast<int>(QFont::Bold)       ? NSFontWeightBold
                              : weight >= static_cast<int>(QFont::DemiBold) ? NSFontWeightSemibold
                              : weight >= static_cast<int>(QFont::Medium)   ? NSFontWeightMedium
                                                                            : NSFontWeightRegular;
  NSFont* result = [NSFont systemFontOfSize:size weight:native];
  if (font.italic()) { result = [NSFontManager.sharedFontManager convertFont:result toHaveTrait:NSItalicFontMask]; }
  return result;
}

} // namespace

NativeCellStyle nativeCellStyle(const QModelIndex& index, const QAbstractItemView* view) {
  NativeCellStyle style;
  style.font = cellFont(index, view);
  if (index.data(NativeRefusedRole).toBool()) {
    // As styleParameterCell: the error text on a tenth of the error colour.
    style.text = roleColor(ColorRole::ErrorText);
    style.fill = [roleColor(ColorRole::ErrorText) colorWithAlphaComponent:kFillAlpha];
    return style;
  }
  const QPalette palette = view->palette();
  style.text = semanticColor(itemColor(index.data(Qt::ForegroundRole)), palette, true);
  style.fill = semanticColor(itemColor(index.data(Qt::BackgroundRole)), palette, false);
  return style;
}

void elideNativeCell(NSTextField* field, Qt::TextElideMode mode) {
  field.usesSingleLineMode = YES;
  field.maximumNumberOfLines = 1;
  field.preferredMaxLayoutWidth = 0;
  field.cell.wraps = NO;
  field.cell.scrollable = NO;
  field.cell.truncatesLastVisibleLine = YES;
  // Last: a truncating mode keeps the cell from wrapping.
  field.lineBreakMode = mode == Qt::ElideLeft     ? NSLineBreakByTruncatingHead
                        : mode == Qt::ElideMiddle ? NSLineBreakByTruncatingMiddle
                        : mode == Qt::ElideNone   ? NSLineBreakByClipping
                                                  : NSLineBreakByTruncatingTail;
}

int nativeTextToneKind(const QWidget* widget) {
  const QString tone = widget == nullptr ? QString() : widget->property(kSemanticTextToneProperty).toString();
  return tone == QLatin1String("error") ? 1 : tone == QLatin1String("warning") ? 2 : 0;
}

NSColor* nativeTextTone(int kind) {
  return kind == 1   ? roleColor(ColorRole::ErrorText)
         : kind == 2 ? roleColor(ColorRole::WarningText)
                     : NSColor.labelColor;
}

QHeaderView* nativeHeader(const QAbstractItemView* view) {
  if (const auto* tree = qobject_cast<const QTreeView*>(view)) { return tree->header(); }
  if (const auto* table = qobject_cast<const QTableView*>(view)) { return table->horizontalHeader(); }
  return nullptr;
}

namespace {

// A header's or an item's decoration as an icon: a QIcon or a QPixmap.
QIcon headerIcon(const QVariant& value) {
  if (value.typeId() == QMetaType::QIcon) return value.value<QIcon>();
  if (value.typeId() == QMetaType::QPixmap) return QIcon(value.value<QPixmap>());
  return {};
}

// A column header's title after its icon, as the shared header draws it
// (Purge Volumes' material swatches, mitcad#1078), in the header's own font
// and text colour.
NSAttributedString* iconTitle(NSTableHeaderCell* cell, const QString& title, const QIcon& icon) {
  NSFont* font = cell.font != nil ? cell.font : [NSFont systemFontOfSize:NSFont.systemFontSize];
  NSColor* color = cell.textColor != nil ? cell.textColor : NSColor.headerTextColor;
  NSDictionary* attributes = @{NSFontAttributeName: font, NSForegroundColorAttributeName: color};
  NSMutableAttributedString* result = [NSMutableAttributedString new];
  if (NSImage* picture = nativeIconImage(icon, std::min<CGFloat>(16, std::ceil(font.pointSize + 2)))) {
    NSTextAttachment* attachment = [NSTextAttachment new];
    attachment.image = picture;
    attachment.bounds = NSMakeRect(0, std::round((font.capHeight - picture.size.height) / 2), picture.size.width,
                                   picture.size.height);
    [result appendAttributedString:[NSAttributedString attributedStringWithAttachment:attachment]];
    [result appendAttributedString:[[NSAttributedString alloc] initWithString:@" " attributes:attributes]];
  }
  [result appendAttributedString:[[NSAttributedString alloc] initWithString:title.toNSString() attributes:attributes]];
  return result;
}

CGFloat headerWidth(NSTableView* table, int section) {
  if (table.headerView == nil) return 0;
  for (NSUInteger visual = 0; visual < table.tableColumns.count; ++visual) {
    NSTableColumn* column = table.tableColumns[visual];
    if (column.identifier.integerValue != section) continue;
    NSTableHeaderCell* cell = column.headerCell;
    const NSSize size = cell.cellSize;
    const NSRect spacious = NSMakeRect(0, 0, size.width * 2 + 1, std::max(size.height, NSHeight(table.headerView.bounds)));
    const NSRect title = [cell titleRectForBounds:spacious];
    NSFont* font = cell.font != nil ? cell.font : [NSFont systemFontOfSize:NSFont.systemFontSize];
    // A header with an icon (mitcad#1078) draws its attributed title.
    const CGFloat glyphs = std::max([column.title sizeWithAttributes:@{NSFontAttributeName:font}].width,
                                    cell.attributedStringValue.size.width);
    const CGFloat preferred = std::max(size.width, glyphs + std::max<CGFloat>(0, NSWidth(spacious) - NSWidth(title)));
    // The outline column's header draws after its disclosure indentation.
    // On some AppKit versions headerRect/titleRect still report that area
    // as text space. Keep the actual indentation without counting it twice
    // when the header rectangle already excludes it.
    const NSRect rect = [table.headerView headerRectOfColumn:static_cast<NSInteger>(visual)];
    const CGFloat outside = NSWidth(rect) > 0 ? std::max<CGFloat>(0, column.width - NSWidth(rect)) : 0;
    NSOutlineView* outline = [table isKindOfClass:NSOutlineView.class] ? (NSOutlineView*)table : nil;
    const CGFloat indentation = outline.outlineTableColumn == column ? outline.indentationPerLevel : 0;
    return std::ceil(preferred + std::max(outside, indentation));
  }
  return 0;
}

CGFloat contentsWidth(NSTableView* table, const QAbstractItemView* view, int section) {
  const auto* tree = qobject_cast<const QTreeView*>(view);
  const auto* grid = qobject_cast<const QTableView*>(view);
  NSTextFieldCell* measure = [NSTextFieldCell new];
  measure.usesSingleLineMode = YES;
  CGFloat width = headerWidth(table, section);
  int remaining = nativeHeader(view)->resizeContentsPrecision();
  const bool unlimited = remaining < 0;
  const auto rows = [&](auto&& self, const QModelIndex& parent, int depth) -> void {
    for (int row = 0; row < view->model()->rowCount(parent) && (unlimited || remaining > 0); ++row) {
      if ((tree && tree->isRowHidden(row, parent)) || (grid && grid->isRowHidden(row))) continue;
      if (!unlimited) --remaining;
      const QModelIndex first = view->model()->index(row, 0, parent);
      const QModelIndex index = first.siblingAtColumn(section);
      // A spanning group label belongs to the whole row, not one column.
      if (index.isValid() && !(tree && tree->isFirstColumnSpanned(row, parent)) && view->indexWidget(index) == nullptr) {
        QString text = index.data().toString();
        if (Qt::mightBeRichText(text)) {
          QTextDocument document;
          document.setHtml(text);
          text = document.toPlainText();
        }
        measure.stringValue = text.toNSString();
        measure.font = cellFont(index, view);
        CGFloat needed = std::ceil(measure.cellSize.width) + 8; // the native cell's text insets
        if (index.flags().testFlag(Qt::ItemIsUserCheckable) && index.data(Qt::CheckStateRole).isValid()) needed += 20;
        else if (index.data(Qt::DecorationRole).isValid()) needed += std::max<CGFloat>(18, std::min<CGFloat>(80, table.rowHeight - 4)) + 4;
        if (tree && section == 0) needed += (depth + 1) * ((NSOutlineView*)table).indentationPerLevel;
        width = std::max(width, needed);
      }
      if (tree && tree->isExpanded(first)) self(self, first, depth + 1);
    }
  };
  rows(rows, view->rootIndex(), 0);
  return width;
}

} // namespace

bool applyNativeColumns(NSTableView* table, const QAbstractItemView* view, NativeColumnState& state) {
  QHeaderView* header = nativeHeader(view);
  if (table == nil || header == nullptr || view->model() == nullptr) { return false; }
  NSArray<NSTableColumn*>* columns = table.tableColumns;
  // A stretched last section is the last one shown, in visual order.
  int last = -1;
  for (int visual = header->count() - 1; visual >= 0 && last < 0; --visual) {
    const int logical = header->logicalIndex(visual);
    if (logical >= 0 && !header->isSectionHidden(logical)) { last = logical; }
  }
  // The shared state of each column, read from Qt only.
  struct Shared {
    int section = -1;
    QString title;
    QString tip;
    QIcon icon;
    bool hidden = false;
    bool stretch = false;
    QHeaderView::ResizeMode mode = QHeaderView::Interactive;
    int size = 0;
    int minimum = 16;
  };
  std::vector<Shared> shared(static_cast<std::size_t>(columns.count));
  std::size_t signature = mixed(0, static_cast<std::size_t>(columns.count));
  const quint64 contentsRequest = header->property(NativeColumnContentsRequestProperty).toULongLong();
  signature = mixed(signature, static_cast<std::size_t>(contentsRequest));
  for (std::size_t i = 0; i < shared.size(); ++i) {
    Shared& s = shared[i];
    const int section = QString::fromNSString(columns[i].identifier).toInt();
    if (section < 0 || section >= header->count()) { continue; }
    s.section = section;
    s.title = view->model()->headerData(section, Qt::Horizontal).toString();
    s.tip = view->model()->headerData(section, Qt::Horizontal, Qt::ToolTipRole).toString();
    s.icon = headerIcon(view->model()->headerData(section, Qt::Horizontal, Qt::DecorationRole));
    s.hidden = header->isSectionHidden(section);
    s.mode = header->sectionResizeMode(section);
    s.stretch = s.mode == QHeaderView::Stretch || (header->stretchLastSection() && section == last);
    // A stretching section's own size follows the hidden Qt view's layout;
    // its native width comes from fitting instead.
    s.size = s.stretch ? 0 : header->sectionSize(section);
    const int smallest = header->minimumSectionSize();
    s.minimum = std::max(16, s.stretch || s.size <= 0 ? smallest : std::min(smallest, s.size));
    for (const std::size_t part : {static_cast<std::size_t>(section), qHash(s.title), qHash(s.tip),
                                   static_cast<std::size_t>(s.icon.cacheKey()),
                                   static_cast<std::size_t>(s.hidden), static_cast<std::size_t>(s.stretch),
                                   static_cast<std::size_t>(s.mode), static_cast<std::size_t>(s.size),
                                   static_cast<std::size_t>(s.minimum)}) {
      signature = mixed(signature, part);
    }
  }
  if (signature == state.signature && state.sizes.size() == shared.size()) { return false; }
  state.signature = signature;
  if (state.sizes.size() != shared.size()) { state.sizes.assign(shared.size(), -1); }
  const bool newRequest = contentsRequest != state.contentsRequest || state.requestedSizes.size() != shared.size();
  if (newRequest) {
    state.contentsRequest = contentsRequest;
    state.requestedSizes.assign(shared.size(), 0);
    state.requestedSharedSizes.assign(shared.size(), -1);
  }
  for (std::size_t i = 0; i < shared.size(); ++i) {
    const Shared& s = shared[i];
    if (s.section < 0) { continue; }
    NSTableColumn* column = columns[i];
    column.title = s.title.toNSString();
    if (!s.icon.isNull()) { column.headerCell.attributedStringValue = iconTitle(column.headerCell, s.title, s.icon); }
    column.headerToolTip = s.tip.isEmpty() ? nil : s.tip.toNSString();
    column.hidden = s.hidden;
    // The autoresizing flag marks a stretching column for fitNativeColumns;
    // AppKit's own column autoresizing is off. As in the shared header, a
    // stretching or fixed section is not resized by hand.
    column.resizingMask = s.stretch                             ? NSTableColumnAutoresizingMask
                          : s.mode == QHeaderView::Interactive ? NSTableColumnUserResizingMask
                                                               : NSTableColumnNoResizing;
    if (newRequest && contentsRequest != 0 && s.mode == QHeaderView::Interactive) {
      state.requestedSizes[i] = static_cast<int>(std::ceil(contentsWidth(table, view, s.section)));
      state.requestedSharedSizes[i] = s.size;
    }
    const bool endedRequest = state.requestedSharedSizes[i] >= 0 &&
        (s.mode != QHeaderView::Interactive || state.requestedSharedSizes[i] != s.size);
    if (endedRequest) {
      state.requestedSizes[i] = 0;
      state.requestedSharedSizes[i] = -1;
    }
    const bool requested = contentsRequest != 0 && s.mode == QHeaderView::Interactive &&
        state.requestedSharedSizes[i] == s.size;
    column.minWidth = requested && s.stretch ? std::max(s.minimum, state.requestedSizes[i]) : s.minimum;
    // A section's width follows a change of its shared size; a native user
    // resize stays until then. A stretching column's comes from fitting.
    // ResizeToContents is a sizing policy, not a pixel width shared by
    // different font renderers and cell margins. Measure only on a changed
    // header state; unchanged refreshes still do no AppKit sizing work.
    const int taken = s.stretch ? 0 : s.mode == QHeaderView::ResizeToContents
        ? std::max(s.size, static_cast<int>(std::ceil(contentsWidth(table, view, s.section))))
        : requested ? std::max(s.size, state.requestedSizes[i]) : s.size;
    if (taken > 0 && (state.sizes[i] != taken || endedRequest || (newRequest && requested))) { column.width = taken; }
    state.sizes[i] = taken;
  }
  table.columnAutoresizingStyle = NSTableViewNoColumnAutoresizing;
  fitNativeColumns(table);
  return true;
}

void fitNativeColumns(NSTableView* table) {
  NSScrollView* scroll = table.enclosingScrollView;
  if (scroll == nil || table.columnAutoresizingStyle != NSTableViewNoColumnAutoresizing) { return; }
  const CGFloat available = scroll.contentView.bounds.size.width;
  if (available <= 0) { return; }
  CGFloat used = 0;
  NSMutableArray<NSTableColumn*>* stretching = [NSMutableArray new];
  for (NSTableColumn* column in table.tableColumns) {
    if (column.hidden) { continue; }
    used += table.intercellSpacing.width;
    if ((column.resizingMask & NSTableColumnAutoresizingMask) != 0) { [stretching addObject:column]; }
    else { used += column.width; }
  }
  if (stretching.count == 0) { return; }
  const CGFloat share = std::floor(std::max<CGFloat>(0, available - used) / static_cast<CGFloat>(stretching.count));
  for (NSTableColumn* column in stretching) {
    const CGFloat width = std::max(column.minWidth, share);
    if (std::abs(column.width - width) >= 0.5) { column.width = width; }
  }
}

void followNativeColumns(NSTableView* table) {
  NSClipView* clip = table.enclosingScrollView.contentView;
  if (clip == nil || objc_getAssociatedObject(table, &g_columnFitKey) != nil) { return; }
  MitcadNativeColumnFit* fit = [MitcadNativeColumnFit new];
  fit->table = table;
  clip.postsFrameChangedNotifications = YES;
  NSNotificationCenter* center = NSNotificationCenter.defaultCenter;
  [center addObserver:fit selector:@selector(fit:) name:NSViewFrameDidChangeNotification object:clip];
  [center addObserver:fit selector:@selector(fit:) name:NSTableViewColumnDidResizeNotification object:table];
  [center addObserver:fit selector:@selector(fit:) name:NSOutlineViewColumnDidResizeNotification object:table];
  [center addObserver:fit selector:@selector(fit:) name:NSTableViewColumnDidMoveNotification object:table];
  objc_setAssociatedObject(table, &g_columnFitKey, fit, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
}

void installNativeColumnEditors(NSTableView* table, const std::vector<NSView*>& editors) {
  if (editors.empty()) { return; }
  auto* header = [[MitcadNativeEditorHeader alloc] initWithFrame:NSMakeRect(0, 0, table.frame.size.width, 28)];
  NSMutableArray<NSView*>* holders = [NSMutableArray new];
  CGFloat height = 0;
  for (NSView* editor : editors) {
    NSView* holder = [[NSView alloc] initWithFrame:NSZeroRect];
    [holders addObject:holder];
    [header addSubview:holder];
    if (editor == nil) { continue; }
    editor.translatesAutoresizingMaskIntoConstraints = NO;
    [holder addSubview:editor];
    [NSLayoutConstraint activateConstraints:@[
      [editor.leadingAnchor constraintEqualToAnchor:holder.leadingAnchor],
      [editor.trailingAnchor constraintEqualToAnchor:holder.trailingAnchor],
      [editor.topAnchor constraintEqualToAnchor:holder.topAnchor],
      [editor.bottomAnchor constraintEqualToAnchor:holder.bottomAnchor]
    ]];
    height = std::max(height, editor.fittingSize.height);
  }
  header.editors = holders;
  [header setFrameSize:NSMakeSize(header.frame.size.width, height + 4)];
  table.headerView = header;
  header.needsLayout = YES;
  [table.enclosingScrollView tile];
}

NSTableView* owningTable(NSView* view) {
  for (NSView* parent = view.superview; parent != nil; parent = parent.superview) {
    if ([parent isKindOfClass:NSTableView.class]) { return (NSTableView*)parent; }
  }
  return nil;
}

void logNativeCellEdit(const char* what, NSView* view) {
  static const bool enabled = qEnvironmentVariableIntValue("MITCAD_LOG_NATIVE_EDITS") == 1;
  if (!enabled) { return; }
  // The cell's or the table's identifier: the nearest one up the views.
  QString name;
  for (NSView* parent = view; parent != nil && name.isEmpty(); parent = parent.superview) {
    name = QString::fromNSString(parent.accessibilityIdentifier);
  }
  QStringList frames;
  NSArray<NSString*>* stack = NSThread.callStackSymbols;
  for (NSUInteger i = 1; i < stack.count && i < 18; ++i) { frames << QString::fromNSString(stack[i]).simplified(); }
  qInfo().noquote() << QStringLiteral("Native cell edit: %1 (%2), focus %3; from %4")
                           .arg(QString::fromLatin1(what), name,
                                QString::fromNSString(NSStringFromClass(view.window.firstResponder.class)),
                                frames.join(QStringLiteral(" | ")));
}

void selectEditedText(NSTextField* field) {
  if (field.currentEditor != nil) { [field.currentEditor selectAll:nil]; }
  else { [field selectText:nil]; }
}

NSTextField* editedCellField(NSTableView* table) {
  NSResponder* responder = table.window.firstResponder;
  if (![responder isKindOfClass:NSTextView.class] || !((NSTextView*)responder).isFieldEditor) { return nil; }
  id edited = ((NSTextView*)responder).delegate;
  if (![edited isKindOfClass:NSTextField.class]) { return nil; }
  NSTextField* field = (NSTextField*)edited;
  return [field isDescendantOf:table] ? field : nil;
}

namespace {

// The row header column's identifier: no model column has a negative one,
// so whatever reads a column's section from its identifier skips it.
NSString* const kRowHeaderColumn = @"-1";
// The row labels' icons, at most.
constexpr CGFloat kRowHeaderIcon = 16;

NSFont* rowHeaderFont() { return [NSFont systemFontOfSize:NSFont.systemFontSize]; }

CGFloat textWidth(const QString& text, NSFont* font) {
  return [text.toNSString() sizeWithAttributes:@{NSFontAttributeName: font}].width;
}

// The font with digits of one width, in its size and weight.
NSFont* tabularFont(NSFont* font) {
  NSFont* base = font != nil ? font : [NSFont systemFontOfSize:NSFont.systemFontSize];
  NSDictionary* traits = [base.fontDescriptor objectForKey:NSFontTraitsAttribute];
  NSNumber* weight = traits[NSFontWeightTrait];
  return [NSFont monospacedDigitSystemFontOfSize:base.pointSize
                                          weight:weight != nil ? weight.doubleValue : NSFontWeightRegular];
}

} // namespace

NSImage* nativeIconImage(const QIcon& icon, CGFloat points) {
  if (icon.isNull() || points <= 0) { return nil; }
  const int side = static_cast<int>(std::lround(points));
  const QPixmap pixmap = icon.pixmap(QSize(side, side), 2.0);
  if (pixmap.isNull()) { return nil; }
  QByteArray data;
  QBuffer buffer(&data);
  buffer.open(QIODevice::WriteOnly);
  pixmap.save(&buffer, "PNG");
  NSImage* result = [[NSImage alloc] initWithData:[NSData dataWithBytes:data.constData()
                                                                 length:static_cast<NSUInteger>(data.size())]];
  const QSizeF logical = pixmap.deviceIndependentSize();
  if (result != nil && logical.width() > 0 && logical.height() > 0) {
    // Its own size, or down to the height asked for in its proportions.
    const CGFloat scale = std::min<CGFloat>(1, points / logical.height());
    result.size = NSMakeSize(logical.width() * scale, logical.height() * scale);
  }
  return result;
}

bool nativeRowHeaderShown(const QAbstractItemView* view) {
  const auto* table = qobject_cast<const QTableView*>(view);
  // Opted in: other tables' row numbers carry nothing. A header never shown
  // yet is not hidden: only one hidden on purpose is.
  return table != nullptr && table->model() != nullptr && view->property(NativeRowHeaderProperty).toBool() &&
         !view->property("nativeSourceList").toBool() && table->verticalHeader()->isVisibleTo(table);
}

bool isNativeRowHeader(NSTableColumn* column) { return [column.identifier isEqualToString:kRowHeaderColumn]; }

NSTableColumn* createNativeRowHeader() {
  NSTableColumn* column = [[NSTableColumn alloc] initWithIdentifier:kRowHeaderColumn];
  column.title = @"";
  column.width = 60;
  column.minWidth = 16;
  column.resizingMask = NSTableColumnUserResizingMask;
  column.editable = NO;
  return column;
}

NSView* nativeRowHeaderCell(const QAbstractItemView* view, int row) {
  NSTableCellView* cell = [NSTableCellView new];
  QAbstractItemModel* model = view->model();
  if (model == nullptr) { return cell; }
  NSTextField* field = [NSTextField labelWithString:model->headerData(row, Qt::Vertical).toString().toNSString()];
  field.font = rowHeaderFont();
  field.lineBreakMode = NSLineBreakByTruncatingTail;
  field.toolTip = model->headerData(row, Qt::Vertical, Qt::ToolTipRole).toString().toNSString();
  field.translatesAutoresizingMaskIntoConstraints = NO;
  [field setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow
                                  forOrientation:NSLayoutConstraintOrientationHorizontal];
  cell.textField = field;
  [cell addSubview:field];
  CGFloat inset = 4;
  if (NSImage* picture = nativeIconImage(headerIcon(model->headerData(row, Qt::Vertical, Qt::DecorationRole)),
                                         kRowHeaderIcon)) {
    NSImageView* image = [NSImageView imageViewWithImage:picture];
    image.translatesAutoresizingMaskIntoConstraints = NO;
    cell.imageView = image;
    [cell addSubview:image];
    [NSLayoutConstraint activateConstraints:@[
      [image.leadingAnchor constraintEqualToAnchor:cell.leadingAnchor constant:inset],
      [image.centerYAnchor constraintEqualToAnchor:cell.centerYAnchor],
      [image.widthAnchor constraintEqualToConstant:picture.size.width],
      [image.heightAnchor constraintEqualToConstant:picture.size.height],
    ]];
    inset += picture.size.width + 5;
  }
  [NSLayoutConstraint activateConstraints:@[
    [field.leadingAnchor constraintEqualToAnchor:cell.leadingAnchor constant:inset],
    [field.trailingAnchor constraintLessThanOrEqualToAnchor:cell.trailingAnchor constant:-4],
    [field.centerYAnchor constraintEqualToAnchor:cell.centerYAnchor],
  ]];
  QString identifier = model->headerData(row, Qt::Vertical, NativeControlIdentifierRole).toString();
  if (identifier.isEmpty() && !view->objectName().isEmpty()) {
    identifier = view->objectName() + QStringLiteral(".row.") + QString::number(row);
  }
  cell.accessibilityIdentifier = identifier.toNSString();
  return cell;
}

void fitNativeRowHeader(NSTableView* table, const QAbstractItemView* view) {
  NSTableColumn* column = [table tableColumnWithIdentifier:kRowHeaderColumn];
  QAbstractItemModel* model = view->model();
  if (column == nil || model == nullptr) { return; }
  const auto* grid = qobject_cast<const QTableView*>(view);
  NSFont* font = rowHeaderFont();
  CGFloat width = 24;
  for (int row = 0; row < model->rowCount(view->rootIndex()); ++row) {
    if (grid != nullptr && grid->isRowHidden(row)) { continue; }
    // The label's insets and the cell's margins.
    CGFloat needed = std::ceil(textWidth(model->headerData(row, Qt::Vertical).toString(), font)) + 12;
    if (!headerIcon(model->headerData(row, Qt::Vertical, Qt::DecorationRole)).isNull()) {
      needed += kRowHeaderIcon + 5;
    }
    width = std::max(width, needed);
  }
  if (std::abs(column.width - width) >= 0.5) { column.width = width; }
}

std::map<int, NativeDecimalColumn> nativeDecimalColumns(const QAbstractItemView* view) {
  std::map<int, NativeDecimalColumn> result;
  QAbstractItemModel* model = view->model();
  const QVariantList columns = view->property(NativeDecimalColumnsProperty).toList();
  if (model == nullptr || columns.isEmpty()) { return result; }
  NSFont* font = tabularFont(nil);
  const auto width = [font](const QString& text) { return static_cast<int>(std::ceil(textWidth(text, font))); };
  const int gap = width(QStringLiteral(" "));
  for (const QVariant& value : columns) {
    const int column = value.toInt();
    std::vector<NumberWidths> numbers;
    // All rows, as the shared delegate measures them: hidden ones too, so
    // that the axis stays when a filter changes.
    const auto rows = [&](auto&& self, const QModelIndex& parent) -> void {
      for (int row = 0; row < model->rowCount(parent); ++row) {
        if (const std::optional<NumberParts> parts = splitNumber(model->index(row, column, parent).data().toString())) {
          numbers.push_back(NumberWidths{width(parts->integer), width(parts->fraction), width(parts->unit)});
        }
        self(self, model->index(row, 0, parent));
      }
    };
    rows(rows, view->rootIndex());
    if (numbers.empty()) { continue; }
    const DecimalColumn layout = layoutDecimalColumn(numbers, gap);
    result[column] = NativeDecimalColumn{static_cast<CGFloat>(layout.axis), static_cast<CGFloat>(layout.unitStart)};
  }
  return result;
}

NSAttributedString* nativeDecimalText(const QString& text, const NativeDecimalColumn& column, NSFont* font,
                                      NSColor* color, NSLineBreakMode mode) {
  const std::optional<NumberParts> parts = splitNumber(text);
  if (!parts) { return nil; }
  qsizetype start = 0;
  while (start < text.size() && text.at(start).isSpace()) { ++start; }
  if (text.mid(start, parts->integer.size()) != parts->integer) { return nil; }
  NSFont* digits = tabularFont(font);
  // The integer part ends on the axis.
  const QString integer = text.left(start + parts->integer.size());
  const CGFloat indent = std::max<CGFloat>(0, column.axis - textWidth(integer, digits));
  NSMutableParagraphStyle* paragraph = [NSMutableParagraphStyle new];
  paragraph.firstLineHeadIndent = indent;
  paragraph.headIndent = indent;
  paragraph.lineBreakMode = mode;
  NSMutableDictionary* attributes =
      [@{NSFontAttributeName: digits, NSParagraphStyleAttributeName: paragraph} mutableCopy];
  if (color != nil) { attributes[NSForegroundColorAttributeName] = color; }
  NSMutableAttributedString* result = [[NSMutableAttributedString alloc] initWithString:text.toNSString()
                                                                             attributes:attributes];
  if (!parts->unit.isEmpty()) {
    qsizetype end = text.size();
    while (end > 0 && text.at(end - 1).isSpace()) { --end; }
    const qsizetype unit = end - parts->unit.size();
    if (unit > 0 && text.mid(unit, parts->unit.size()) == parts->unit) {
      // What precedes the unit widens or narrows, so that the units of all
      // rows start on one line.
      const CGFloat kern = column.unitStart - indent - textWidth(text.left(unit), digits);
      [result addAttribute:NSKernAttributeName value:@(kern) range:NSMakeRange(static_cast<NSUInteger>(unit - 1), 1)];
    }
  }
  return result;
}

} // namespace mitcad::mac
