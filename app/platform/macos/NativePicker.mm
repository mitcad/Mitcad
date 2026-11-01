// SPDX-License-Identifier: MIT
// The pickers' AppKit popover (NativePicker.hpp; mitcad#1066, mitcad#1068):
// a search field over the entries of a Qt picker presenter, bound to its
// search field and list.
#include "platform/NativePicker.hpp"
#include "platform/NativeDialogs.hpp"
#include "browser/LibraryPicker.hpp"

#import <AppKit/AppKit.h>

#include <algorithm>
#include <cstddef>
#include <vector>

#include <QBuffer>
#include <QByteArray>
#include <QCoreApplication>
#include <QEvent>
#include <QFont>
#include <QIODevice>
#include <QIcon>
#include <QKeyEvent>
#include <QLineEdit>
#include <QListWidget>
#include <QListWidgetItem>
#include <QObject>
#include <QPixmap>
#include <QPointer>
#include <QSize>
#include <QWidget>
#include <QtGlobal>
#include <QtLogging>

namespace mitcad {
class AppKitPicker;
}

// The entries: Return and Escape go to the presenter as in its search field.
@interface MitcadNativePickerTable : NSTableView {
@public
  mitcad::AppKitPicker* owner;
}
@end

@interface MitcadNativePickerTarget
    : NSObject <NSPopoverDelegate, NSSearchFieldDelegate, NSTableViewDataSource, NSTableViewDelegate> {
@public
  mitcad::AppKitPicker* owner;
}
- (void)searched:(id)sender;
- (void)clicked:(id)sender;
@end

namespace mitcad {
namespace {

NSString* ns(const QString& text) { return text.toNSString(); }
QString qt(NSString* text) { return text == nil ? QString() : QString::fromNSString(text); }

// An entry's icon at `points` logical points, drawn at twice that for
// Retina screens.
NSImage* iconImage(const QIcon& icon, int points) {
  if (icon.isNull() || points <= 0) {
    return nil;
  }
  QByteArray data;
  QBuffer buffer(&data);
  buffer.open(QIODevice::WriteOnly);
  icon.pixmap(points * 2, points * 2).save(&buffer, "PNG");
  NSImage* image = [[NSImage alloc] initWithData:[NSData dataWithBytes:data.constData()
                                                              length:static_cast<NSUInteger>(data.size())]];
  image.size = NSMakeSize(points, points);
  return image;
}

NSView* identified(NSView* root, NSString* wanted) {
  if (root == nil || root.hidden) {
    return nil;
  }
  if ([root.accessibilityIdentifier isEqualToString:wanted]) {
    return root;
  }
  for (NSView* child in root.subviews) {
    if (NSView* found = identified(child, wanted)) {
      return found;
    }
  }
  return nil;
}

// The visible control a picker hangs from: in the key window first, then
// the main window and the others in front to back order.
NSView* anchorView(const QString& identifier) {
  if (identifier.isEmpty()) {
    return nil;
  }
  NSMutableOrderedSet<NSWindow*>* windows = [NSMutableOrderedSet orderedSet];
  if (NSApp.keyWindow != nil) {
    [windows addObject:NSApp.keyWindow];
  }
  if (NSApp.mainWindow != nil) {
    [windows addObject:NSApp.mainWindow];
  }
  [windows addObjectsFromArray:NSApp.orderedWindows];
  NSString* wanted = ns(identifier);
  for (NSWindow* window in windows) {
    if (!window.visible) {
      continue;
    }
    NSView* root = window.contentView.superview != nil ? window.contentView.superview : window.contentView;
    NSView* found = identified(root, wanted);
    if (found != nil && !found.hiddenOrHasHiddenAncestor && found.window != nil) {
      return found;
    }
  }
  return nil;
}

// An entry that cannot be chosen: a heading.
bool isHeading(const QListWidgetItem* item) {
  return item != nullptr && !(item->flags() & Qt::ItemIsSelectable);
}

// What names an entry: its accessible text (an appearance's name without
// the check mark of one already there), else its text.
QString entryName(const QListWidgetItem* item) {
  const QString accessible = item->data(Qt::AccessibleTextRole).toString();
  return accessible.isEmpty() ? item->text() : accessible;
}

AppKitPicker* g_picker = nullptr; // the one open; a new one closes it

} // namespace

class AppKitPicker final : public QObject {
public:
  AppKitPicker(QWidget* presenter, QLineEdit* search, QListWidget* list)
      : m_presenter(presenter), m_search(search), m_list(list) {
    m_target = [MitcadNativePickerTarget new];
    m_target->owner = this;
    m_images = [NSMutableDictionary dictionary];
    // The presenter's own size: 320 x 400 for the tools', 280 x 360 for the
    // Materials window's.
    const QSize size = presenter->size().expandedTo(QSize(240, 240)).boundedTo(QSize(480, 560));
    const CGFloat width = size.width();
    const CGFloat height = size.height();
    NSView* content = [[NSView alloc] initWithFrame:NSMakeRect(0, 0, width, height)];
    m_field = [[NSSearchField alloc] initWithFrame:NSZeroRect];
    m_field.translatesAutoresizingMaskIntoConstraints = NO;
    m_field.placeholderString = ns(search->placeholderText());
    m_field.stringValue = ns(search->text());
    m_field.sendsSearchStringImmediately = YES;
    m_field.delegate = m_target;
    m_field.target = m_target;
    m_field.action = @selector(searched:);
    m_field.identifier = ns(search->objectName());
    m_field.accessibilityIdentifier = m_field.identifier;
    [content addSubview:m_field];
    m_table = [[MitcadNativePickerTable alloc] initWithFrame:NSMakeRect(0, 0, width, height)];
    m_table->owner = this;
    NSTableColumn* column = [[NSTableColumn alloc] initWithIdentifier:@"entry"];
    column.width = width - 12;
    column.resizingMask = NSTableColumnAutoresizingMask;
    [m_table addTableColumn:column];
    m_table.headerView = nil;
    m_table.columnAutoresizingStyle = NSTableViewLastColumnOnlyAutoresizingStyle;
    m_table.style = NSTableViewStyleFullWidth;
    m_table.allowsEmptySelection = YES;
    m_table.allowsMultipleSelection = NO;
    m_table.rowHeight = std::max(22, list->iconSize().height() + 6);
    m_table.dataSource = m_target;
    m_table.delegate = m_target;
    m_table.target = m_target;
    m_table.action = @selector(clicked:);
    m_table.identifier = ns(list->objectName());
    m_table.accessibilityIdentifier = m_table.identifier;
    m_table.accessibilityLabel = ns(search->placeholderText());
    NSScrollView* scroll = [[NSScrollView alloc] initWithFrame:NSMakeRect(0, 0, width, height)];
    scroll.translatesAutoresizingMaskIntoConstraints = NO;
    scroll.documentView = m_table;
    scroll.hasVerticalScroller = YES;
    scroll.hasHorizontalScroller = NO;
    scroll.autohidesScrollers = YES;
    scroll.borderType = NSNoBorder;
    scroll.drawsBackground = NO;
    [content addSubview:scroll];
    [NSLayoutConstraint activateConstraints:@[
      [m_field.topAnchor constraintEqualToAnchor:content.topAnchor constant:10],
      [m_field.leadingAnchor constraintEqualToAnchor:content.leadingAnchor constant:10],
      [m_field.trailingAnchor constraintEqualToAnchor:content.trailingAnchor constant:-10],
      [scroll.topAnchor constraintEqualToAnchor:m_field.bottomAnchor constant:6],
      [scroll.leadingAnchor constraintEqualToAnchor:content.leadingAnchor],
      [scroll.trailingAnchor constraintEqualToAnchor:content.trailingAnchor],
      [scroll.bottomAnchor constraintEqualToAnchor:content.bottomAnchor constant:-4]
    ]];
    NSViewController* controller = [NSViewController new];
    controller.view = content;
    m_popover = [NSPopover new];
    m_popover.behavior = NSPopoverBehaviorTransient;
    m_popover.animates = qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT");
    m_popover.contentSize = NSMakeSize(width, height);
    m_popover.contentViewController = controller;
    m_popover.delegate = m_target;
    presenter->installEventFilter(this);
    connect(presenter, &QObject::destroyed, this, [this] { close(); });
    connect(list, &QListWidget::currentItemChanged, this, [this] { selectCurrent(); });
  }

  ~AppKitPicker() override {
    if (g_picker == this) {
      g_picker = nullptr;
    }
    if (m_presenter != nullptr) {
      m_presenter->removeEventFilter(this);
    }
    // AppKit's delegates and targets do not own this object.
    m_target->owner = nullptr;
    m_table->owner = nullptr;
    m_popover.delegate = nil;
    m_table.dataSource = nil;
    m_table.delegate = nil;
    m_table.target = nil;
    m_field.delegate = nil;
    m_field.target = nil;
    if (m_popover.shown) {
      [m_popover close];
    }
  }

  // Shows it under the control `anchor` names, else under the pointer.
  void open(const QString& anchor) {
    reload();
    NSView* view = anchorView(anchor);
    if (view != nil) {
      // Below the control: in a flipped view the maximum Y edge is the
      // bottom one.
      [m_popover showRelativeToRect:view.bounds ofView:view
                      preferredEdge:view.flipped ? NSRectEdgeMaxY : NSRectEdgeMinY];
    } else {
      NSWindow* window = NSApp.keyWindow != nil ? NSApp.keyWindow : (__bridge NSWindow*)nativeDialogWindow();
      NSView* content = window.contentView;
      if (content == nil) {
        closed();
        return;
      }
      const NSPoint at = [content convertPoint:[window convertPointFromScreen:NSEvent.mouseLocation] fromView:nil];
      [m_popover showRelativeToRect:NSMakeRect(at.x - 1, at.y - 1, 2, 2) ofView:content
                      preferredEdge:content.flipped ? NSRectEdgeMaxY : NSRectEdgeMinY];
    }
    NSWindow* window = m_field.window;
    if (window == nil) {
      closed();
      return;
    }
    // Found by focus-dialog as the Qt picker window is.
    window.title = ns(m_presenter->windowTitle());
    window.identifier = ns(m_presenter->objectName());
    window.accessibilityIdentifier = window.identifier;
    [window makeKeyWindow];
    [window makeFirstResponder:m_field];
    qInfo().noquote() << QStringLiteral("Native picker %1 at %2: %3 rows")
                             .arg(m_presenter->objectName(), view != nil ? anchor : QStringLiteral("the pointer"))
                             .arg(m_rows.size());
  }

  // The list's shown entries, its current one selected.
  void reload() {
    m_rows.clear();
    if (m_list != nullptr) {
      for (int row = 0; row < m_list->count(); ++row) {
        if (!m_list->item(row)->isHidden()) {
          m_rows.push_back(row);
        }
      }
    }
    [m_table reloadData];
    selectCurrent();
  }

  void selectCurrent() {
    const QListWidgetItem* current = m_list != nullptr ? m_list->currentItem() : nullptr;
    NSInteger at = -1;
    for (std::size_t i = 0; current != nullptr && i < m_rows.size(); ++i) {
      if (m_list->item(m_rows[i]) == current) {
        at = static_cast<NSInteger>(i);
      }
    }
    m_syncing = true;
    if (at >= 0) {
      [m_table selectRowIndexes:[NSIndexSet indexSetWithIndex:static_cast<NSUInteger>(at)] byExtendingSelection:NO];
      [m_table scrollRowToVisible:at];
    } else {
      [m_table deselectAll:nil];
    }
    m_syncing = false;
  }

  QListWidgetItem* item(NSInteger row) const {
    if (m_list == nullptr || row < 0 || row >= static_cast<NSInteger>(m_rows.size())) {
      return nullptr;
    }
    return m_list->item(m_rows[static_cast<std::size_t>(row)]);
  }

  NSInteger rowCount() const { return static_cast<NSInteger>(m_rows.size()); }
  bool selectable(NSInteger row) const { return item(row) != nullptr && !isHeading(item(row)); }

  NSView* cell(NSInteger row) {
    const QListWidgetItem* entry = item(row);
    if (entry == nullptr) {
      return nil;
    }
    NSTableCellView* view = [[NSTableCellView alloc] initWithFrame:NSZeroRect];
    NSTextField* text = [NSTextField labelWithString:ns(entry->text())];
    text.lineBreakMode = NSLineBreakByTruncatingTail;
    [text setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow
                                   forOrientation:NSLayoutConstraintOrientationHorizontal];
    if (isHeading(entry) || entry->font().bold()) {
      // A heading of the entries under it, as the Qt list's bold line.
      text.font = [NSFont boldSystemFontOfSize:NSFont.smallSystemFontSize];
      text.textColor = NSColor.secondaryLabelColor;
    }
    NSStackView* line = [NSStackView stackViewWithViews:@[]];
    line.orientation = NSUserInterfaceLayoutOrientationHorizontal;
    line.alignment = NSLayoutAttributeCenterY;
    line.spacing = 6;
    line.translatesAutoresizingMaskIntoConstraints = NO;
    if (NSImage* image = imageOf(entry)) {
      NSImageView* picture = [NSImageView imageViewWithImage:image];
      picture.imageScaling = NSImageScaleProportionallyDown;
      [picture setContentHuggingPriority:NSLayoutPriorityRequired
                          forOrientation:NSLayoutConstraintOrientationHorizontal];
      [line addArrangedSubview:picture];
      view.imageView = picture;
    }
    [line addArrangedSubview:text];
    const bool library = m_list->objectName() == QStringLiteral("libraryPickerList");
    const QString tag = library ? entry->data(PickerTagRole).toString() : QString();
    if (!tag.isEmpty()) {
      NSView* space = [[NSView alloc] initWithFrame:NSZeroRect];
      [space setContentHuggingPriority:1 forOrientation:NSLayoutConstraintOrientationHorizontal];
      [line addArrangedSubview:space];
      NSTextField* source = [NSTextField labelWithString:ns(tag)];
      source.font = [NSFont systemFontOfSize:NSFont.smallSystemFontSize];
      source.textColor = NSColor.secondaryLabelColor;
      [source setContentCompressionResistancePriority:NSLayoutPriorityRequired
                                       forOrientation:NSLayoutConstraintOrientationHorizontal];
      [line addArrangedSubview:source];
    }
    view.textField = text;
    [view addSubview:line];
    [NSLayoutConstraint activateConstraints:@[
      [line.leadingAnchor constraintEqualToAnchor:view.leadingAnchor constant:isHeading(entry) ? 4 : 8],
      [line.trailingAnchor constraintEqualToAnchor:view.trailingAnchor constant:-8],
      [line.centerYAnchor constraintEqualToAnchor:view.centerYAnchor constant:
          library && entry->data(PickerLineRole).toBool() ? 3 : 0]
    ]];
    if (library && entry->data(PickerLineRole).toBool()) {
      NSBox* separator = [[NSBox alloc] initWithFrame:NSZeroRect];
      separator.boxType = NSBoxSeparator;
      separator.translatesAutoresizingMaskIntoConstraints = NO;
      [view addSubview:separator];
      [NSLayoutConstraint activateConstraints:@[
        [separator.leadingAnchor constraintEqualToAnchor:view.leadingAnchor constant:8],
        [separator.trailingAnchor constraintEqualToAnchor:view.trailingAnchor constant:-8],
        [separator.topAnchor constraintEqualToAnchor:view.topAnchor constant:2]
      ]];
    }
    const QString name = entryName(entry);
    view.toolTip = entry->toolTip().isEmpty() ? nil : ns(entry->toolTip());
    view.identifier = ns(m_list->objectName() + QStringLiteral(".item.") + name);
    view.accessibilityIdentifier = view.identifier;
    view.accessibilityLabel = ns(tag.isEmpty() ? name : name + QStringLiteral(", ") + tag);
    return view;
  }

  // A selection made in the table: the list's current entry.
  void selectionChanged() {
    if (m_syncing || m_list == nullptr) {
      return;
    }
    QListWidgetItem* entry = item(m_table.selectedRow);
    if (entry != nullptr && !isHeading(entry) && entry != m_list->currentItem()) {
      m_list->setCurrentItem(entry);
    }
  }

  // The text typed in the search field: the presenter filters.
  void typed(const QString& text) {
    if (m_search == nullptr || m_search->text() == text) {
      return;
    }
    m_search->setText(text);
    reload();
  }

  // Up and Down in the search field: the next or previous shown entry.
  void step(int by) {
    const NSInteger count = rowCount();
    NSInteger at = m_table.selectedRow;
    for (NSInteger tried = 0; tried < count; ++tried) {
      at = at < 0 ? (by > 0 ? 0 : count - 1) : at + by;
      if (at < 0 || at >= count) {
        return; // stays at the first or the last
      }
      if (selectable(at)) {
        m_list->setCurrentItem(item(at));
        return;
      }
    }
  }

  // A click on an entry chooses it as Return does.
  void activate(NSInteger row) {
    if (!selectable(row)) {
      return;
    }
    m_list->setCurrentItem(item(row));
    chooseCurrent();
  }

  void chooseCurrent() { send(m_list, Qt::Key_Return); }
  void cancel() { send(m_search, Qt::Key_Escape); }

  void close() {
    if (m_popover.shown) {
      [m_popover close]; // popoverDidClose: closed()
    } else {
      closed();
    }
  }

  // The popover closed (a click outside, or the presenter hid): the
  // presenter closes too, as at Escape, and this goes.
  void closed() {
    if (m_done) {
      return;
    }
    m_done = true;
    if (m_presenter != nullptr && m_presenter->isVisible()) {
      cancel();
    }
    if (m_presenter != nullptr && m_presenter->isVisible()) {
      m_presenter->hide();
    }
    if (g_picker == this) {
      g_picker = nullptr;
    }
    deleteLater();
  }

protected:
  bool eventFilter(QObject* watched, QEvent* event) override {
    if (watched == m_presenter && event->type() == QEvent::Hide) {
      close();
    }
    return QObject::eventFilter(watched, event);
  }

private:
  // The key goes to the presenter's own key handling (its event filter).
  void send(QWidget* to, int key) {
    if (to == nullptr) {
      return;
    }
    QKeyEvent press(QEvent::KeyPress, key, Qt::NoModifier);
    QCoreApplication::sendEvent(to, &press);
  }

  NSImage* imageOf(const QListWidgetItem* entry) {
    const QIcon icon = entry->icon();
    if (icon.isNull()) {
      return nil;
    }
    NSNumber* key = @(icon.cacheKey());
    NSImage* image = m_images[key];
    if (image == nil) {
      image = iconImage(icon, m_list->iconSize().height());
      if (image != nil) {
        m_images[key] = image;
      }
    }
    return image;
  }

  QPointer<QWidget> m_presenter;
  QPointer<QLineEdit> m_search;
  QPointer<QListWidget> m_list;
  std::vector<int> m_rows; // the list's rows shown, in order
  MitcadNativePickerTarget* __strong m_target = nil;
  NSPopover* __strong m_popover = nil;
  NSSearchField* __strong m_field = nil;
  MitcadNativePickerTable* __strong m_table = nil;
  NSMutableDictionary<NSNumber*, NSImage*>* __strong m_images = nil;
  bool m_syncing = false;
  bool m_done = false;
};

bool showNativePicker(QWidget* presenter, QLineEdit* search, QListWidget* list, const QString& anchor) {
  if (!nativeDialogsActive() || presenter == nullptr || search == nullptr || list == nullptr) {
    return false;
  }
  if (AppKitPicker* open = g_picker) {
    g_picker = nullptr;
    open->close();
  }
  // Shown for its own show, filtering and logs, never on the screen.
  presenter->setAttribute(Qt::WA_DontShowOnScreen, true);
  presenter->setAttribute(Qt::WA_ShowWithoutActivating, true);
  presenter->show();
  auto* picker = new AppKitPicker(presenter, search, list);
  g_picker = picker;
  picker->open(anchor);
  return true;
}

} // namespace mitcad

@implementation MitcadNativePickerTable
- (void)keyDown:(NSEvent*)event {
  const bool plain = !(event.modifierFlags & (NSEventModifierFlagCommand | NSEventModifierFlagControl |
                                              NSEventModifierFlagOption));
  if (owner != nullptr && plain && (event.keyCode == 36 || event.keyCode == 76)) {
    owner->chooseCurrent();
    return;
  }
  if (owner != nullptr && plain && event.keyCode == 53) {
    owner->cancel();
    return;
  }
  [super keyDown:event];
}
@end

@implementation MitcadNativePickerTarget
- (void)searched:(id)sender {
  if (owner != nullptr) {
    owner->typed(mitcad::qt(((NSSearchField*)sender).stringValue));
  }
}
- (void)controlTextDidChange:(NSNotification*)notification {
  if (owner != nullptr && [notification.object isKindOfClass:NSSearchField.class]) {
    owner->typed(mitcad::qt(((NSSearchField*)notification.object).stringValue));
  }
}
- (BOOL)control:(NSControl*)control textView:(NSTextView*)textView doCommandBySelector:(SEL)selector {
  (void)control;
  (void)textView;
  if (owner == nullptr) {
    return NO;
  }
  if (selector == @selector(moveDown:)) {
    owner->step(1);
    return YES;
  }
  if (selector == @selector(moveUp:)) {
    owner->step(-1);
    return YES;
  }
  if (selector == @selector(insertNewline:) || selector == @selector(insertNewlineIgnoringFieldEditor:)) {
    owner->chooseCurrent();
    return YES;
  }
  if (selector == @selector(cancelOperation:)) {
    owner->cancel();
    return YES;
  }
  return NO;
}
- (void)clicked:(id)sender {
  if (owner != nullptr) {
    owner->activate(((NSTableView*)sender).clickedRow);
  }
}
- (NSInteger)numberOfRowsInTableView:(NSTableView*)tableView {
  (void)tableView;
  return owner != nullptr ? owner->rowCount() : 0;
}
- (NSView*)tableView:(NSTableView*)tableView viewForTableColumn:(NSTableColumn*)tableColumn row:(NSInteger)row {
  (void)tableView;
  (void)tableColumn;
  return owner != nullptr ? owner->cell(row) : nil;
}
- (BOOL)tableView:(NSTableView*)tableView shouldSelectRow:(NSInteger)row {
  (void)tableView;
  return owner != nullptr && owner->selectable(row);
}
- (void)tableViewSelectionDidChange:(NSNotification*)notification {
  (void)notification;
  if (owner != nullptr) {
    owner->selectionChanged();
  }
}
- (void)popoverDidClose:(NSNotification*)notification {
  (void)notification;
  if (owner != nullptr) {
    owner->closed();
  }
}
@end
