// SPDX-License-Identifier: MIT
#include "platform/NativeTestInput.hpp"

#import <AppKit/AppKit.h>
#import <CoreGraphics/CoreGraphics.h>
#import <QuartzCore/QuartzCore.h>

#include <QApplication>
#include <QComboBox>
#include <QCheckBox>
#include <QDialog>
#include <QDoubleSpinBox>
#include <QElapsedTimer>
#include <QEvent>
#include <QEventLoop>
#include <QAction>
#include <QMenu>
#include <QLabel>
#include <QLineEdit>
#include <QMessageBox>
#include <QProgressBar>
#include <QPushButton>
#include <QSpinBox>
#include <QStandardItemModel>
#include <QTableView>
#include <QTreeView>
#include <QHeaderView>
#include <QTimer>
#include <QRegularExpression>
#include <QImage>
#include <QJsonArray>
#include <QJsonObject>
#include <QItemSelectionModel>
#include <QIntValidator>
#include <QOpenGLWidget>
#include <QPainter>
#include <QPalette>
#include <QRegion>
#include <QRectF>
#include <QTemporaryDir>
#include <QToolButton>
#include <QVBoxLayout>
#include <QtLogging>
#include <QtMath>
#include <algorithm>
#include <cmath>
#include <functional>
#include <memory>
#include <vector>
#include <unistd.h>

#include "platform/MacChrome.hpp"
#include "platform/NativeDialogs.hpp"
#include "platform/NativeForms.hpp"
#include "platform/NativeMacWindow.hpp"
#include "NativeNavigationGuide.hpp"
#include "NativeColorField.hpp"
#include "NativeTableCells.hpp"
#include "browser/DocumentSnapshot.hpp"
#include "browser/TimelinePresentation.hpp"
#include "framework/CachePreferences.hpp"
#include "framework/CommandSession.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/Icons.hpp"
#include "framework/SettingsWindow.hpp"
#include "MainWindow.hpp"

// Public selectors on the same feature controls used by pointer input.
@protocol MITCADNativeHistoryFeatureTest <NSObject>
- (void)selectHistoryFeature;
- (void)activateHistoryFeature;
@end

// Read-only diagnostic of the actual NSSlider mouseDown stack, maintained
// by the production control around AppKit's native cell tracking.
@protocol MITCADNativeSliderTrackingTest <NSObject>
- (BOOL)pointerTracking;
@end

// Read the selection of this exact table's live shared presenter, even
// when retained dialogs contain other controls with the same object name.
@protocol MITCADNativeModelSelectionTest <NSObject>
- (NSInteger)presenterSelectedRows;
@end

// NSEvent exposes magnification and phase as read-only properties and has
// no public magnify factory. This in-process test double exercises the real
// document NSWindow dispatch entry without injecting system-wide input.
@interface MITCADTestMagnificationEvent : NSEvent {
@public
  NSWindow* __weak target;
  NSPoint point;
  CGFloat amount;
  NSEventPhase gesturePhase;
  NSTimeInterval eventTime;
}
@end

@implementation MITCADTestMagnificationEvent
- (NSEventType)type { return NSEventTypeMagnify; }
- (NSWindow*)window { return target; }
- (NSInteger)windowNumber { return target.windowNumber; }
- (NSPoint)locationInWindow { return point; }
- (CGFloat)magnification { return amount; }
- (NSEventPhase)phase { return gesturePhase; }
- (NSEventModifierFlags)modifierFlags { return 0; }
- (NSTimeInterval)timestamp { return eventTime; }
// The Cocoa gesture reader uses zero for the default trackpad device.
- (NSUInteger)deviceID { return 0; }
@end

@interface MITCADTestScrollEvent : NSEvent {
@public
  NSWindow* __weak target;
  NSPoint point;
  CGFloat dx;
  CGFloat dy;
  BOOL precise;
}
@end

@implementation MITCADTestScrollEvent
- (NSEventType)type { return NSEventTypeScrollWheel; }
- (NSWindow*)window { return target; }
- (NSInteger)windowNumber { return target.windowNumber; }
- (NSPoint)locationInWindow { return point; }
- (CGFloat)deltaX { return dx; }
- (CGFloat)deltaY { return dy; }
- (CGFloat)scrollingDeltaX { return dx; }
- (CGFloat)scrollingDeltaY { return dy; }
- (BOOL)hasPreciseScrollingDeltas { return precise; }
- (NSEventPhase)phase { return precise ? NSEventPhaseChanged : NSEventPhaseNone; }
- (NSEventPhase)momentumPhase { return NSEventPhaseNone; }
- (NSEventModifierFlags)modifierFlags { return 0; }
- (NSTimeInterval)timestamp { return NSProcessInfo.processInfo.systemUptime; }
@end

namespace mitcad::mac::test {
namespace {

NSWindow* __weak focusedWindow = nil;
NSWindow* __weak keyEventWindow = nil;
NSView* __weak draggedHistoryMarker = nil;
NSPoint historyDragPoint = NSZeroPoint;

QString qt(NSString* text) { return QString::fromNSString(text != nil ? text : @""); }

NSArray<NSWindow*>* nativeWindows() {
  NSMutableOrderedSet<NSWindow*>* windows = [NSMutableOrderedSet orderedSet];
  [windows addObjectsFromArray:NSApp.orderedWindows];
  [windows addObjectsFromArray:NSApp.windows];
  if (NSApp.keyWindow) [windows addObject:NSApp.keyWindow];
  if (NSApp.modalWindow) [windows addObject:NSApp.modalWindow];
  if (void* panel = nativeFileDialogPanel()) [windows addObject:(__bridge NSWindow*)panel];
  if (focusedWindow) [windows addObject:focusedWindow];
  for (NSUInteger index = 0; index < windows.count; ++index) {
    NSWindow* sheet = windows[index].attachedSheet;
    if (sheet) [windows addObject:sheet];
  }
  return windows.array;
}

bool presented(NSWindow* window) {
  return window.visible || window.sheetParent != nil || window == NSApp.modalWindow ||
         window == (__bridge NSWindow*)nativeFileDialogPanel();
}

NSWindow* documentWindow() {
  for (NSWindow* window in NSApp.windows) {
    if ([window.identifier isEqualToString:@"native.documentWindow"]) return window;
  }
  return nil;
}

NSWindow* targetWindow() {
  const auto deepestSheet = [](NSWindow* window) {
    while (window.attachedSheet != nil) window = window.attachedSheet;
    return window;
  };
  if (void* panel = nativeFileDialogPanel()) return deepestSheet((__bridge NSWindow*)panel);
  if (focusedWindow && focusedWindow.visible) return deepestSheet(focusedWindow);
  NSWindow* main = documentWindow();
  if (main.attachedSheet) return deepestSheet(main);
  if (NSApp.modalWindow) return deepestSheet(NSApp.modalWindow);
  return deepestSheet(NSApp.keyWindow != nil ? NSApp.keyWindow : main);
}

NSSavePanel* filePanel() {
  NSWindow* target = targetWindow();
  return [target isKindOfClass:NSSavePanel.class] ? (NSSavePanel*)target : nil;
}

NSView* findView(NSView* root, const std::function<bool(NSView*)>& matches) {
  if (!root || root.hidden) return nil;
  if (matches(root)) return root;
  for (NSView* child in root.subviews) {
    if (NSView* found = findView(child, matches)) return found;
  }
  return nil;
}

// Layout checks need to address the actual card even while it is hidden.
// Unlike findControl, this never materializes or scrolls table rows.
NSView* identifiedView(NSView* root, const QString& identifier) {
  if (!root) return nil;
  if (qt(root.accessibilityIdentifier) == identifier || qt(root.identifier) == identifier) return root;
  for (NSView* child in root.subviews) {
    if (NSView* found = identifiedView(child, identifier)) return found;
  }
  return nil;
}

bool viewVisible(NSView* view) {
  return view && presented(view.window) && ![view isHiddenOrHasHiddenAncestor] &&
         !NSIsEmptyRect(NSIntersectionRect(view.visibleRect, view.bounds));
}

NSMenuItem* findAction(NSMenu* menu, const QString& identifier);

QString commandIdentifier(QAction* action) {
  return action->objectName().startsWith(QStringLiteral("command_"))
      ? action->objectName().mid(8)
      : action->property("mitcadNativeActionId").toString();
}

QMenu* retainedRibbonMenu(const QString& workspace, const QString& group) {
  const QString name = QStringLiteral("ribbonMenu_") + workspace + QLatin1Char('/') + group;
  for (QWidget* widget : QApplication::allWidgets()) {
    if (widget->objectName() == name) {
      if (auto* button = qobject_cast<QToolButton*>(widget)) return button->menu();
    }
  }
  return nullptr;
}

bool sameCommandMenu(NSMenu* native, QMenu* retained, bool caption, const QString& path, QString& error) {
  [native update]; // Validate the same current states that opening it presents.
  QList<QAction*> expected;
  for (QAction* action : retained->actions()) {
    if (action->isVisible()) expected.append(action);
  }
  NSMutableArray<NSMenuItem*>* actual = [NSMutableArray array];
  for (NSUInteger index = caption ? 1 : 0; index < native.itemArray.count; ++index) {
    NSMenuItem* item = native.itemArray[index];
    [actual addObject:item];
  }
  if (actual.count != NSUInteger(expected.size())) {
    error = QStringLiteral("native menu '%1' has %2 commands; the retained CAD menu has %3")
        .arg(path).arg(qulonglong(actual.count)).arg(expected.size());
    return false;
  }
  for (int index = 0; index < expected.size(); ++index) {
    QAction* action = expected[index];
    NSMenuItem* item = actual[NSUInteger(index)];
    if (action->isSeparator() || [item isSeparatorItem]) {
      if (action->isSeparator() == bool([item isSeparatorItem])) continue;
      error = QStringLiteral("native menu '%1' section boundary at %2: Qt '%3' separator=%4, AppKit '%5' separator=%6")
                  .arg(path).arg(index).arg(action->text()).arg(action->isSeparator()).arg(qt(item.title))
                  .arg(bool([item isSeparatorItem]));
      return false;
    }
    QString title = action->text();
    title.remove(QLatin1Char('&'));
    const QString identifier = commandIdentifier(action);
    if (qt([item.representedObject isKindOfClass:NSString.class] ? item.representedObject : nil) != identifier ||
        qt(item.title) != title || bool(item.submenu) != bool(action->menu())) {
      error = QStringLiteral("native menu '%1' item %2 differs from CAD command '%3' or its submenu shape")
          .arg(path).arg(index + 1).arg(identifier);
      return false;
    }
    if (action->isCheckable() && item.state != (action->isChecked() ? NSControlStateValueOn : NSControlStateValueOff)) {
      error = QStringLiteral("native menu '%1/%2' has a different checked state").arg(path, title);
      return false;
    }
    if (action->menu() && !sameCommandMenu(item.submenu, action->menu(), false, path + QLatin1Char('/') + title, error)) return false;
  }
  return true;
}

bool nativeMenuPlacement(QString& error) {
  NSMenu* appMenu = NSApp.mainMenu.itemArray.firstObject.submenu;
  QStringList titles;
  for (NSMenuItem* top in NSApp.mainMenu.itemArray) {
    titles.append(qt(top.title));
    if (![top.title isEqualToString:@"Create"] && ![top.title isEqualToString:@"Sketch"]) continue;
    for (NSMenuItem* item in top.submenu.itemArray) {
      const QString id = qt([item.representedObject isKindOfClass:NSString.class] ? item.representedObject : nil);
      if (id.startsWith(QStringLiteral("tools.")) || id.startsWith(QStringLiteral("view.")) ||
          id.startsWith(QStringLiteral("file.")) || id.startsWith(QStringLiteral("help."))) {
        error = QStringLiteral("menu-only command '%1' leaked into native %2").arg(id, qt(top.title));
        return false;
      }
    }
  }
  const QStringList expected = {titles.value(0), QStringLiteral("File"), QStringLiteral("Edit"),
                                QStringLiteral("View"), QStringLiteral("Tools"), QStringLiteral("Window"), QStringLiteral("Help")};
  if (titles != expected) {
    error = QStringLiteral("native menu bar differs from the shared File/Edit/View/Tools/Window/Help order: %1")
                .arg(titles.join(QLatin1Char('/')));
    return false;
  }
  if (!appMenu || !findAction(appMenu, QStringLiteral("tools.preferences"))) {
    error = QStringLiteral("native Settings is missing from the MitCad application menu");
    return false;
  }
  if (findAction(NSApp.mainMenu, QStringLiteral("tools.preferences.cache"))) {
    error = QStringLiteral("the search-only Cache preferences page leaked into the native menu bar");
    return false;
  }
  return true;
}

bool ribbonAppearance(NSView* ribbon, QString& error) {
  NSView* caption = findView(ribbon, [](NSView* view) {
    return [view isKindOfClass:NSPopUpButton.class] &&
           qt(view.accessibilityIdentifier).startsWith(QStringLiteral("native.ribbon.group."));
  });
  NSView* document = caption.superview;
  if (!document) {
    error = QStringLiteral("the native ribbon has no command groups");
    return false;
  }
  std::vector<NSView*> buttons;
  std::vector<NSView*> captions;
  std::vector<NSView*> separators;
  for (NSView* child in document.subviews) {
    if (child.hidden) continue;
    if ([child isKindOfClass:NSPopUpButton.class] &&
        qt(child.accessibilityIdentifier).startsWith(QStringLiteral("native.ribbon.group."))) captions.push_back(child);
    else if ([child isKindOfClass:NSButton.class]) buttons.push_back(child);
    else if ([child isKindOfClass:NSBox.class] && ((NSBox*)child).boxType == NSBoxSeparator) separators.push_back(child);
  }
  const auto leftToRight = [](NSView* a, NSView* b) { return NSMinX(a.frame) < NSMinX(b.frame); };
  std::sort(buttons.begin(), buttons.end(), leftToRight);
  std::sort(captions.begin(), captions.end(), leftToRight);
  NSView* frame = documentWindow().contentView.superview;
  NSSegmentedControl* tabs = (NSSegmentedControl*)identifiedView(frame, QStringLiteral("native.workspaces"));
  const QString workspace = tabs ? qt([tabs labelForSegment:tabs.selectedSegment]).toUpper() : QStringLiteral("SOLID");
  for (NSView* group : captions) {
    const QString name = qt(group.accessibilityIdentifier).mid(QStringLiteral("native.ribbon.group.").size()).toUpper();
    QMenu* retained = retainedRibbonMenu(workspace, name);
    if (!retained) {
      error = QStringLiteral("the retained CAD menu '%1/%2' is missing").arg(workspace, name);
      return false;
    }
    if (!sameCommandMenu(((NSPopUpButton*)group).menu, retained, true, workspace + QLatin1Char('/') + name, error)) return false;
  }
  if (!nativeMenuPlacement(error)) return false;
  for (size_t i = 1; i < buttons.size(); ++i) {
    if (NSMinX(buttons[i].frame) - NSMaxX(buttons[i - 1].frame) < -0.1) {
      error = QStringLiteral("native ribbon buttons '%1' and '%2' overlap")
          .arg(qt(buttons[i - 1].accessibilityIdentifier), qt(buttons[i].accessibilityIdentifier));
      return false;
    }
  }
  for (size_t i = 0; i < captions.size(); ++i) {
    NSView* group = captions[i];
    if (!NSContainsRect(document.bounds, group.frame) ||
        group.frame.size.width + 1 < group.intrinsicContentSize.width ||
        (i > 0 && NSMaxX(captions[i - 1].frame) >= NSMinX(group.frame))) {
      error = QStringLiteral("native ribbon caption '%1' is clipped or overlaps another group")
          .arg(qt(group.accessibilityIdentifier));
      return false;
    }
    for (NSView* separator : separators) {
      if (NSIntersectsRect(group.frame, separator.frame)) {
        error = QStringLiteral("native ribbon separator overlaps caption '%1'").arg(qt(group.accessibilityIdentifier));
        return false;
      }
    }
  }
  return true;
}

bool coloredIconMatches(NSButton* native, const QIcon& sharedIcon, QString& error) {
  if (!native.image || [native.image isTemplate] || sharedIcon.isNull()) {
    error = QStringLiteral("native icon '%1' is missing or monochrome templated").arg(qt(native.accessibilityIdentifier));
    return false;
  }
  NSBitmapImageRep* bitmap = nil;
  for (NSImageRep* representation in native.image.representations) {
    if ([representation isKindOfClass:NSBitmapImageRep.class]) {
      bitmap = (NSBitmapImageRep*)representation;
      break;
    }
  }
  NSData* png = [bitmap representationUsingType:NSBitmapImageFileTypePNG properties:@{}];
  QImage actual = QImage::fromData(static_cast<const uchar*>(png.bytes), int(png.length), "PNG")
                      .convertToFormat(QImage::Format_RGBA8888_Premultiplied);
  if (actual.isNull()) {
    error = QStringLiteral("native icon '%1' has no readable bitmap").arg(qt(native.accessibilityIdentifier));
    return false;
  }
  // These are physical bitmap pixels, including Retina scaling. Render
  // the shared CAD asset at that exact size with DPR 1 so QIcon does not
  // multiply the captured size by the display's scale a second time.
  const QImage expected = sharedIcon.pixmap(actual.size(), 1.0).toImage()
                              .convertToFormat(QImage::Format_RGBA8888_Premultiplied);
  if (actual.size() != expected.size()) {
    error = QStringLiteral("native icon '%1' differs in bitmap size from the shared palette asset")
        .arg(qt(native.accessibilityIdentifier));
    return false;
  }
  bool colored = false;
  for (int y = 0; y < actual.height(); ++y) {
    const uchar* a = actual.constScanLine(y);
    const uchar* e = expected.constScanLine(y);
    for (int x = 0; x < actual.width(); ++x) {
      for (int channel = 0; channel < 4; ++channel) {
        if (std::abs(int(a[x * 4 + channel]) - int(e[x * 4 + channel])) > 2) {
          error = QStringLiteral("native icon '%1' differs from the shared palette asset at %2,%3")
              .arg(qt(native.accessibilityIdentifier)).arg(x).arg(y);
          return false;
        }
      }
      colored = colored || (a[x * 4 + 3] > 128 &&
          std::max({a[x * 4], a[x * 4 + 1], a[x * 4 + 2]}) -
          std::min({a[x * 4], a[x * 4 + 1], a[x * 4 + 2]}) > 16);
    }
  }
  if (!colored) error = QStringLiteral("native icon '%1' lost its CAD colours").arg(qt(native.accessibilityIdentifier));
  return colored;
}

bool sketchIconAppearance(NSView* content, QString& error) {
  const QString id = QStringLiteral("sketch.constraint.horizontal_vertical");
  NSView* palette = identifiedView(content, QStringLiteral("palette_") + id);
  if (!viewVisible(palette)) return true; // Non-sketch inspector or hidden palette.
  NSView* ribbon = identifiedView(content, QStringLiteral("native.ribbon.") + id);
  QToolButton* presenter = nullptr;
  for (QWidget* widget : QApplication::allWidgets()) {
    if (widget->objectName() == QStringLiteral("palette_") + id) {
      presenter = qobject_cast<QToolButton*>(widget);
      break;
    }
  }
  if (![ribbon isKindOfClass:NSButton.class] || ![palette isKindOfClass:NSButton.class] || !presenter) {
    error = QStringLiteral("the sketch constraint icons are missing their shared palette presenter");
    return false;
  }
  return coloredIconMatches((NSButton*)ribbon, presenter->icon(), error) &&
         coloredIconMatches((NSButton*)palette, presenter->icon(), error);
}

NSView* findControlView(NSView* root, const std::function<bool(NSView*)>& matches,
                        NSTableView** rowTable, NSInteger* rowIndex) {
  if (!root || root.hidden) return nil;
  if (matches(root)) return root;
  if ([root isKindOfClass:NSTableView.class]) {
    NSTableView* table = (NSTableView*)root;
    // View-based AppKit tables only instantiate visible cells. Prepare an
    // offscreen row using the public API, then scroll it into view so input
    // and readiness checks can address the same real row below the fold.
    // A column right of a narrow table's visible width is scrolled into view
    // too; otherwise the prepared cell is a temporary view that the table
    // replaces when it lays out the scrolled columns (mitcad#814).
    for (NSInteger row = 0; row < table.numberOfRows; ++row) {
      for (NSInteger column = 0; column < table.numberOfColumns; ++column) {
        NSView* cell = [table viewAtColumn:column row:row makeIfNecessary:YES];
        if (NSView* found = findView(cell, matches)) {
          [table scrollRowToVisible:row];
          [table scrollColumnToVisible:column];
          [table layoutSubtreeIfNeeded];
          if (NSView* live = [table viewAtColumn:column row:row makeIfNecessary:NO]) {
            if (NSView* actual = findView(live, matches)) found = actual;
          }
          if (rowTable) *rowTable = table;
          if (rowIndex) *rowIndex = row;
          return found;
        }
      }
    }
  }
  for (NSView* child in root.subviews) {
    if (NSView* found = findControlView(child, matches, rowTable, rowIndex)) return found;
  }
  return nil;
}

NSView* findControl(const QString& identifier, NSTableView** rowTable = nullptr, NSInteger* rowIndex = nullptr) {
  if (rowTable) *rowTable = nil;
  if (rowIndex) *rowIndex = -1;
  NSWindow* target = targetWindow();
  const auto match = [&](NSView* view) {
    return qt(view.accessibilityIdentifier) == identifier || qt(view.identifier) == identifier;
  };
  const auto inWindow = [&](NSWindow* window) -> NSView* {
    // AppKit can host toolbar custom views outside the content/titlebar
    // hierarchy, or copy their identifiers onto a non-control wrapper.
    // Prefer actual inserted views through the public API; palette samples
    // are not in this list and must never receive test input or updates.
    for (NSToolbarItem* item in window.toolbar.items) {
      if (qt(item.itemIdentifier) == identifier && item.view) return item.view;
      if (NSView* found = identifiedView(item.view, identifier)) return found;
    }
    if (NSView* found = findControlView(window.contentView.superview != nil ? window.contentView.superview : window.contentView,
                                       match, rowTable, rowIndex)) return found;
    return nil;
  };
  if (NSView* found = inWindow(target)) return found;
  // A transient command-search popover has its own AppKit window. It may
  // not become key until its search field starts editing.
  // NSApp.windows also includes NSPopover's panels on macOS versions
  // where those panels do not participate in orderedWindows.
  for (NSWindow* window in nativeWindows()) {
    if (window == target || !presented(window)) continue;
    if (NSView* found = inWindow(window)) return found;
  }
  return nil;
}

NSView* tableCell(const QString& identifier, QString* detail = nullptr);

NSButton* findButton(const QRegularExpression& expression) {
  const auto match = [&](NSView* view) {
    if (![view isKindOfClass:NSButton.class] || !((NSButton*)view).enabled) return false;
    QString title = qt(((NSButton*)view).title);
    title.remove(QLatin1Char('&'));
    return expression.match(title).hasMatch() || expression.match(qt(view.accessibilityLabel)).hasMatch();
  };
  NSView* content = targetWindow().contentView;
  return (NSButton*)findView(content.superview != nil ? content.superview : content, match);
}

bool windowMatches(NSWindow* window, const QRegularExpression& expression) {
  if (!presented(window) || window == documentWindow()) return false;
  if (expression.match(qt(window.title)).hasMatch()) return true;
  if ([window isKindOfClass:NSSavePanel.class] && expression.match(qt(((NSSavePanel*)window).prompt)).hasMatch()) return true;
  if (expression.match(qt(window.accessibilityIdentifier)).hasMatch()) return true;
  // Native sheets such as NSAlert have no useful window title. Match their
  // visible label text in the same way as the portable QMessageBox route.
  return findView(window.contentView, [&](NSView* view) {
    return [view isKindOfClass:NSTextField.class] && !((NSTextField*)view).editable &&
           expression.match(qt(((NSTextField*)view).stringValue)).hasMatch();
  }) != nil;
}

NSMenuItem* findAction(NSMenu* menu, const QString& identifier) {
  for (NSMenuItem* item in menu.itemArray) {
    if (qt([item.representedObject isKindOfClass:NSString.class] ? item.representedObject : nil) == identifier) return item;
    if (item.submenu) if (NSMenuItem* child = findAction(item.submenu, identifier)) return child;
  }
  return nil;
}

bool isQtResponder(NSResponder* responder) {
  if (!responder) return false;
  const QString name = qt(NSStringFromClass(responder.class));
  return name.startsWith(QStringLiteral("QNS")) || name.startsWith(QStringLiteral("QCocoa"));
}

NSString* characters(int key, const QString& text) {
  if (!text.isEmpty()) return text.toNSString();
  unichar character = 0;
  switch (key) {
  case Qt::Key_Return: case Qt::Key_Enter: character = NSCarriageReturnCharacter; break;
  case Qt::Key_Escape: character = 0x1b; break;
  case Qt::Key_Tab: character = NSTabCharacter; break;
  case Qt::Key_Backtab: character = NSBackTabCharacter; break;
  case Qt::Key_Backspace: character = NSBackspaceCharacter; break;
  case Qt::Key_Delete: character = NSDeleteFunctionKey; break;
  case Qt::Key_Left: character = NSLeftArrowFunctionKey; break;
  case Qt::Key_Right: character = NSRightArrowFunctionKey; break;
  case Qt::Key_Up: character = NSUpArrowFunctionKey; break;
  case Qt::Key_Down: character = NSDownArrowFunctionKey; break;
  case Qt::Key_Home: character = NSHomeFunctionKey; break;
  case Qt::Key_End: character = NSEndFunctionKey; break;
  case Qt::Key_PageUp: character = NSPageUpFunctionKey; break;
  case Qt::Key_PageDown: character = NSPageDownFunctionKey; break;
  default:
    if (key >= Qt::Key_F1 && key <= Qt::Key_F35) character = NSF1FunctionKey + key - Qt::Key_F1;
    else if (key > 0 && key < 0x10000) character = QChar(ushort(key)).toLower().unicode();
    break;
  }
  return character == 0 ? @"" : [NSString stringWithCharacters:&character length:1];
}

unsigned short virtualKey(int key) {
  switch (key) {
  case Qt::Key_G: return 5;
  case Qt::Key_Z: return 6;
  case Qt::Key_Return: return 36;
  case Qt::Key_Enter: return 76;
  case Qt::Key_Tab: case Qt::Key_Backtab: return 48;
  case Qt::Key_Space: return 49;
  case Qt::Key_Backspace: return 51;
  case Qt::Key_Delete: return 117;
  case Qt::Key_F2: return 120;
  case Qt::Key_Escape: return 53;
  case Qt::Key_Left: return 123;
  case Qt::Key_Right: return 124;
  case Qt::Key_Down: return 125;
  case Qt::Key_Up: return 126;
  default: return 0;
  }
}

} // namespace

bool active() { return nativeDialogsActive() && documentWindow() != nil; }

bool nativeFocused() {
  return active() && targetWindow() && !isQtResponder(targetWindow().firstResponder);
}

bool applicationDelegate(QString& error) {
  id<NSApplicationDelegate> delegate = NSApp.delegate;
  const bool native = active();
  const bool resign = [delegate respondsToSelector:@selector(applicationDidResignActive:)];
  const bool become = [delegate respondsToSelector:@selector(applicationDidBecomeActive:)];
  const bool reopen = [delegate respondsToSelector:@selector(applicationShouldHandleReopen:hasVisibleWindows:)];
  const bool secureSelector = [delegate respondsToSelector:@selector(applicationSupportsSecureRestorableState:)];
  const bool secure = secureSelector && [delegate applicationSupportsSecureRestorableState:NSApp];
  const auto identity = [](id object) {
    return object ? QStringLiteral("%1@%2").arg(qt(NSStringFromClass([object class])))
                       .arg(quintptr((__bridge void*)object), 0, 16) : QStringLiteral("nil");
  };
  const QString details = QStringLiteral("native=%1 delegate=%2 resign=%3 become=%4 reopen=%5 secureSelector=%6 secure=%7 "
                                         "forwardResign=%8 forwardSecure=%9 qtState=%10 appActive=%11")
      .arg(native).arg(identity(delegate)).arg(resign).arg(become).arg(reopen).arg(secureSelector).arg(secure)
      .arg(identity([(NSObject*)delegate forwardingTargetForSelector:@selector(applicationDidResignActive:)]))
      .arg(identity([(NSObject*)delegate forwardingTargetForSelector:@selector(applicationSupportsSecureRestorableState:)]))
      .arg(int(qApp->applicationState())).arg(bool(NSApp.active));
  qInfo().noquote() << QStringLiteral("Native application delegate diagnostic: %1").arg(details);
  if (!native || !resign || !become || !reopen || !secureSelector || !secure) {
    error = QStringLiteral("native application delegate lost Qt activation or secure state restoration: %1").arg(details);
    return false;
  }
  QList<Qt::ApplicationState> changes;
  class ActivationEvents : public QObject {
  public:
    int deactivations = 0;
    bool eventFilter(QObject*, QEvent* event) override {
      if (event->type() == QEvent::ApplicationDeactivate) ++deactivations;
      return false;
    }
  } events;
  qApp->installEventFilter(&events);
  QObject::connect(qApp, &QGuiApplication::applicationStateChanged, &events,
                   [&changes](Qt::ApplicationState state) { changes.append(state); });
  const auto settle = [](Qt::ApplicationState expected) {
    QElapsedTimer deadline;
    deadline.start();
    while (qApp->applicationState() != expected && deadline.elapsed() < 2000) {
      QCoreApplication::processEvents(QEventLoop::AllEvents, 25);
    }
    return qApp->applicationState() == expected;
  };
  const auto notification = [](NSNotificationName name) {
    return [NSNotification notificationWithName:name object:NSApp];
  };
  [delegate applicationDidBecomeActive:notification(NSApplicationDidBecomeActiveNotification)];
  if (!settle(Qt::ApplicationActive)) {
    error = QStringLiteral("native application activation did not reach Qt");
    return false;
  }
  changes.clear();
  events.deactivations = 0;
  [delegate applicationDidResignActive:notification(NSApplicationDidResignActiveNotification)];
  if (!settle(Qt::ApplicationInactive)) {
    error = QStringLiteral("native application deactivation did not reach Qt");
    return false;
  }
  [delegate applicationDidBecomeActive:notification(NSApplicationDidBecomeActiveNotification)];
  if (!settle(Qt::ApplicationActive)) {
    error = QStringLiteral("native application reactivation did not reach Qt");
    return false;
  }
  [delegate applicationDidResignActive:notification(NSApplicationDidResignActiveNotification)];
  if (!settle(Qt::ApplicationInactive) ||
      ![delegate applicationShouldHandleReopen:NSApp hasVisibleWindows:YES] ||
      !settle(Qt::ApplicationActive)) {
    error = QStringLiteral("native Dock reopen did not restore Qt's active application state");
    return false;
  }
  const QList<Qt::ApplicationState> expected = {
      Qt::ApplicationInactive, Qt::ApplicationActive, Qt::ApplicationInactive, Qt::ApplicationActive};
  if (changes != expected || events.deactivations != 2) {
    error = QStringLiteral("native application delegate lost or duplicated Qt state signals or deactivate events");
    return false;
  }
  qInfo() << "Native application delegate: activation, deactivation, reopen and secure state passed";
  return true;
}

bool controlExists(const QString& identifier, QString* detail) {
  if (!active()) return false;
  for (NSToolbarItem* item in documentWindow().toolbar.items) {
    if (qt(item.itemIdentifier) == identifier) return true;
  }
  return tableCell(identifier, detail) != nil;
}

bool textOf(const QString& identifier, QString& value) {
  if (identifier == QStringLiteral("native.window.origin") || identifier == QStringLiteral("native.window.size")) {
    NSWindow* window = documentWindow();
    if (!window) return false;
    if (identifier == QStringLiteral("native.window.origin")) {
      const NSPoint origin = window.frame.origin;
      const NSPoint screen = NSScreen.mainScreen.visibleFrame.origin;
      value = QStringLiteral("%1,%2").arg(qRound(origin.x - screen.x)).arg(qRound(origin.y - screen.y));
    } else {
      const NSSize size = window.contentView.frame.size;
      value = QStringLiteral("%1x%2").arg(qRound(size.width)).arg(qRound(size.height));
    }
    qDebug().noquote() << identifier << value;
    return true;
  }
  if (identifier == QStringLiteral("native.file.name") || identifier == QStringLiteral("native.file.directory")) {
    NSSavePanel* panel = filePanel();
    if (!panel) return false;
    value = qt(identifier == QStringLiteral("native.file.name") ? panel.nameFieldStringValue : panel.directoryURL.path);
    return true;
  }
  NSView* view = findControl(identifier);
  if ([view isKindOfClass:NSScrollView.class] &&
      [((NSScrollView*)view).documentView isKindOfClass:NSTextView.class]) view = ((NSScrollView*)view).documentView;
  // A model cell's identifier is on its cell view; read its shown text.
  if ([view isKindOfClass:NSTableCellView.class] && ((NSTableCellView*)view).textField) view = ((NSTableCellView*)view).textField;
  NSString* text = nil;
  if ([view isKindOfClass:NSTextField.class]) text = ((NSTextField*)view).stringValue;
  else if ([view isKindOfClass:NSTextView.class]) text = ((NSTextView*)view).string;
  // An outline row's cell: its name as shown (mitcad#816).
  else if ([view isKindOfClass:NSTableCellView.class]) text = ((NSTableCellView*)view).textField.stringValue;
  else if ([view isKindOfClass:NSPopUpButton.class]) {
    NSPopUpButton* popup = (NSPopUpButton*)view;
    text = popup.pullsDown ? popup.title : (popup.selectedItem != nil ? popup.titleOfSelectedItem : @"");
  }
  // A read-only probe of the live control, independent of the Qt presenter.
  else if ([view isKindOfClass:NSSlider.class]) {
    value = QString::number(((NSSlider*)view).integerValue);
    return true;
  }
  else if ([view isKindOfClass:NSButton.class]) text = ((NSButton*)view).title;
  else if ([view isKindOfClass:NSImageView.class]) text = ((NSImageView*)view).image.accessibilityDescription;
  if (!text) return false;
  value = qt(text);
  return true;
}

bool textMatches(const QString& identifier, const QRegularExpression& expression) {
  QString text;
  return textOf(identifier, text) && expression.match(text).hasMatch();
}

bool enabled(const QString& identifier, bool expected) {
  NSView* view = findControl(identifier);
  return [view isKindOfClass:NSControl.class] && bool(((NSControl*)view).enabled) == expected;
}

bool visible(const QString& identifier, bool expected) {
  NSWindow* window = documentWindow();
  NSView* root = window.contentView.superview != nil ? window.contentView.superview : window.contentView;
  NSView* view = identifiedView(root, identifier);
  if (view) return viewVisible(view) == expected;
  // Search popovers and secondary controls live in separate AppKit windows.
  // Keep this a visibility query: do not materialize or scroll table rows.
  for (NSWindow* secondary in nativeWindows()) {
    if (secondary == window) continue;
    NSView* content = secondary.contentView;
    NSView* secondaryRoot = content.superview != nil ? content.superview : content;
    if (NSView* found = identifiedView(secondaryRoot, identifier); viewVisible(found)) return expected;
  }
  return !expected;
}

bool present(const QString& identifier, bool expected) {
  NSWindow* window = documentWindow();
  if (!window) return false;
  NSView* root = window.contentView.superview != nil ? window.contentView.superview : window.contentView;
  return (identifiedView(root, identifier) != nil) == expected;
}

bool resizeDocument(int width, int height, QString& error) {
  NSWindow* window = documentWindow();
  if (!window || width < 820 || height < 520) {
    error = QStringLiteral("native-resize requires a document and content size at least 820 x 520");
    return false;
  }
  [window setContentSize:NSMakeSize(width, height)];
  [window.contentView layoutSubtreeIfNeeded];
  return true;
}

bool toolbarItemPresent(const QString& identifier, bool expected) {
  NSToolbar* toolbar = documentWindow().toolbar;
  if (!toolbar) return false;
  bool found = false;
  for (NSToolbarItem* item in toolbar.items) {
    if (qt(item.itemIdentifier) == identifier) found = true;
  }
  return found == expected;
}

bool configureToolbarItem(const QString& identifier, const QString& operation, QString& error) {
  NSToolbar* toolbar = documentWindow().toolbar;
  if (!toolbar) {
    error = QStringLiteral("native toolbar is not installed");
    return false;
  }
  NSString* itemID = identifier.toNSString();
  if (operation == QStringLiteral("palette")) {
    for (NSString* allowed in [toolbar.delegate toolbarAllowedItemIdentifiers:toolbar]) {
      if ([allowed hasPrefix:@"command.native.action."]) {
        error = QStringLiteral("toolbar customization offered an identifier tied to a QAction pointer");
        return false;
      }
    }
    // Customization asks for samples that are never inserted. Keep one
    // alive until the next event-loop turn to expose accidental tracking.
    NSToolbarItem* sample = [toolbar.delegate toolbar:toolbar itemForItemIdentifier:itemID willBeInsertedIntoToolbar:NO];
    if (!sample) { error = QStringLiteral("native toolbar palette item is unavailable"); return false; }
    dispatch_async(dispatch_get_main_queue(), ^{ (void)sample.view; });
    return true;
  }
  if (operation != QStringLiteral("insert") && operation != QStringLiteral("remove")) {
    error = QStringLiteral("native-toolbar-item expects insert, remove or palette");
    return false;
  }
  // Use the same public mutations as the customization UI. Restoring a
  // dictionary configures one toolbar but need not autosave that change.
  for (NSInteger index = NSInteger(toolbar.items.count) - 1; index >= 0; --index) {
    if ([toolbar.items[NSUInteger(index)].itemIdentifier isEqualToString:itemID]) [toolbar removeItemAtIndex:index];
  }
  if (operation == QStringLiteral("insert")) {
    [toolbar insertItemWithItemIdentifier:itemID atIndex:std::min(NSUInteger(3), toolbar.items.count)];
  }
  [NSUserDefaults.standardUserDefaults synchronize];
  if (!toolbarItemPresent(identifier, operation == QStringLiteral("insert"))) {
    error = QStringLiteral("native toolbar configuration did not %1 '%2'").arg(operation, identifier);
    return false;
  }
  return true;
}

bool moveDocument(int x, int y, QString& error) {
  NSWindow* window = documentWindow();
  NSScreen* screen = NSScreen.mainScreen;
  if (!window || !screen) {
    error = QStringLiteral("native-move requires a document and a screen");
    return false;
  }
  const NSPoint origin = screen.visibleFrame.origin;
  [window setFrameOrigin:NSMakePoint(origin.x + x, origin.y + y)];
  return true;
}

QString documentState() {
  NSWindow* window = documentWindow();
  if (!window) return {};
  if (window.miniaturized) return QStringLiteral("minimized");
  return (window.styleMask & NSWindowStyleMaskFullScreen) ? QStringLiteral("fullscreen") : QStringLiteral("normal");
}

bool setDocumentState(const QString& state, QString& error) {
  NSWindow* window = documentWindow();
  if (!window || (state != QStringLiteral("normal") && state != QStringLiteral("minimized") &&
                  state != QStringLiteral("fullscreen"))) {
    error = QStringLiteral("window-state requires a document and normal|minimized|fullscreen");
    return false;
  }
  if (state == documentState()) return true;
  if (state == QStringLiteral("minimized")) {
    [window miniaturize:nil];
    return true;
  }
  if (window.miniaturized) [window deminiaturize:nil];
  if ((state == QStringLiteral("fullscreen")) != bool(window.styleMask & NSWindowStyleMaskFullScreen)) {
    [window toggleFullScreen:nil];
  }
  return true;
}

bool pinch(QOpenGLWidget* viewport, const QPointF& position, double magnification, QString& error) {
  NSWindow* window = documentWindow();
  if (!active() || !viewport || !std::isfinite(magnification)) {
    error = QStringLiteral("native-pinch requires the native document and a finite magnification");
    return false;
  }
  // The standalone Qt root already owns the embedded native handle.
  // Giving the QOpenGLWidget its own winId would create a new native child
  // and change the gesture routing this regression is meant to exercise.
  QWidget* root = viewport->window();
  const WId rootId = root->internalWinId();
  if (!rootId) {
    error = QStringLiteral("native-pinch requires an existing native Qt root handle");
    return false;
  }
  NSView* native = (__bridge NSView*)reinterpret_cast<void*>(rootId);
  if (native.window != window) {
    error = QStringLiteral("native-pinch Qt root is not embedded in its document window");
    return false;
  }
  const QPointF rootPosition = viewport->mapTo(root, position);
  NSPoint local = NSMakePoint(rootPosition.x(), rootPosition.y());
  if (!native.flipped) local.y = NSMaxY(native.bounds) - local.y;
  const NSPoint point = [native convertPoint:local toView:nil];
  [window makeKeyAndOrderFront:nil];
  @try {
    for (NSEventPhase phase : {NSEventPhaseBegan, NSEventPhaseChanged, NSEventPhaseEnded}) {
      MITCADTestMagnificationEvent* event = [MITCADTestMagnificationEvent new];
      event->target = window;
      event->point = point;
      event->amount = phase == NSEventPhaseChanged ? magnification : 0;
      event->gesturePhase = phase;
      event->eventTime = NSProcessInfo.processInfo.systemUptime;
      [window sendEvent:event];
    }
  } @catch (NSException* exception) {
    error = QStringLiteral("native-pinch AppKit dispatch rejected the test event: %1").arg(qt(exception.reason));
    return false;
  }
  return true;
}

bool floatingLayout(QString& error) {
  NSWindow* window = documentWindow();
  NSView* content = window.contentView;
  [content layoutSubtreeIfNeeded];
  NSView* canvas = identifiedView(content, QStringLiteral("native.viewportHost"));
  if (!viewVisible(canvas)) {
    error = QStringLiteral("the native canvas is not visible");
    return false;
  }
  const NSRect canvasRect = [canvas convertRect:canvas.bounds toView:content];
  const NSRect bounds = content.bounds;
  const auto close = [](CGFloat a, CGFloat b) { return std::abs(a - b) <= 1; };
  NSView* ribbon = identifiedView(content, QStringLiteral("native.ribbon"));
  if (!viewVisible(ribbon)) {
    error = QStringLiteral("the native CAD ribbon is not visible");
    return false;
  }
  if (!ribbonAppearance(ribbon, error) || !sketchIconAppearance(content, error)) return false;
  const NSRect ribbonRect = [ribbon convertRect:ribbon.bounds toView:content];
  const bool vertical = content.flipped
      ? close(NSMinY(canvasRect), NSMaxY(ribbonRect)) && close(NSMaxY(canvasRect), NSMaxY(bounds))
      : close(NSMaxY(canvasRect), NSMinY(ribbonRect)) && close(NSMinY(canvasRect), NSMinY(bounds));
  if (!close(NSMinX(canvasRect), NSMinX(bounds)) || !close(canvasRect.size.width, bounds.size.width) || !vertical) {
    error = QStringLiteral("the native canvas does not fill the document below the ribbon (%1 x %2 versus %3 x %4)")
        .arg(canvasRect.size.width).arg(canvasRect.size.height).arg(bounds.size.width).arg(bounds.size.height);
    return false;
  }
  NSView* sidebar = identifiedView(content, QStringLiteral("native.sidebar"));
  NSView* inspector = identifiedView(content, QStringLiteral("native.inspectorCard"));
  if (!sidebar || !inspector) {
    error = QStringLiteral("the native floating sidebar or inspector card is missing");
    return false;
  }
  NSView* model = identifiedView(content, QStringLiteral("native.modelTree"));
  if (![model isKindOfClass:NSOutlineView.class] || !model.enclosingScrollView ||
      ((NSOutlineView*)model).backgroundColor.alphaComponent > 0 ||
      model.enclosingScrollView.drawsBackground) {
    error = QStringLiteral("the native browser draws an opaque inset background over its card");
    return false;
  }
  for (NSString* identifier in @[@"native.projectsCard", @"native.sidebar", @"native.inspectorCard", @"native.historyCard", @"native.ribbon"]) {
    NSView* card = identifiedView(content, qt(identifier));
    if (!card || !viewVisible(card)) continue;
    const NSRect rect = [card convertRect:card.bounds toView:content];
    if (rect.size.width <= 1 || rect.size.height <= 1 ||
        !NSContainsRect(NSInsetRect(bounds, -1, -1), rect)) {
      error = QStringLiteral("native overlay '%1' is empty or outside the document").arg(qt(identifier));
      return false;
    }
    const NSPoint center = NSMakePoint(NSMidX(card.bounds), NSMidY(card.bounds));
    NSView* hit = [content hitTest:[card convertPoint:center toView:content.superview]];
    if (hit != card && ![hit isDescendantOf:card]) {
      error = QStringLiteral("native overlay '%1' cannot receive pointer input above the canvas").arg(qt(identifier));
      return false;
    }
  }
  if (viewVisible(sidebar) && viewVisible(inspector)) {
    const NSRect left = [sidebar convertRect:sidebar.bounds toView:content];
    const NSRect right = [inspector convertRect:inspector.bounds toView:content];
    if (NSMinX(left) <= NSMinX(bounds) || NSMaxX(right) >= NSMaxX(bounds) || NSIntersectsRect(left, right)) {
      error = QStringLiteral("native side cards lack their floating edge insets or overlap each other");
      return false;
    }
  }
  NSView* projects = identifiedView(content, QStringLiteral("native.projectsCard"));
  if (viewVisible(projects)) {
    if (viewVisible(sidebar) && NSIntersectsRect(projects.frame, sidebar.frame)) {
      error = QStringLiteral("native Projects overlaps Browser");
      return false;
    }
    NSView* toolbar = identifiedView(projects, QStringLiteral("projectsToolbar"));
    NSView* projectTree = identifiedView(projects, QStringLiteral("projectsTree"));
    const bool expanded = viewVisible(toolbar);
    if (!expanded) {
      if (viewVisible(projectTree) || std::abs(projects.bounds.size.height - 30) > 1) {
        error = QStringLiteral("collapsed native Projects must retain only its header");
        return false;
      }
    } else {
      if (![toolbar isKindOfClass:NSStackView.class]) {
        error = QStringLiteral("the native Projects toolbar is missing");
        return false;
      }
      for (NSView* button in ((NSStackView*)toolbar).arrangedSubviews) {
        if (button.hidden) continue;
        const NSRect rect = [button convertRect:button.bounds toView:projects];
        if (NSIsEmptyRect(rect) || !NSContainsRect(projects.bounds, rect)) {
          error = QStringLiteral("a native Projects toolbar control is clipped");
          return false;
        }
      }
      if (![projectTree isKindOfClass:NSScrollView.class] || !viewVisible(projectTree)) {
        error = QStringLiteral("native Projects has no bounded source-list viewport");
        return false;
      }
      NSScrollView* scroll = (NSScrollView*)projectTree;
      NSOutlineView* outline = (NSOutlineView*)scroll.documentView;
      const NSRect viewport = [scroll convertRect:scroll.bounds toView:projects];
      const NSRect buttons = [toolbar convertRect:toolbar.bounds toView:projects];
      const NSRect first = outline.numberOfRows > 0 ? [outline rectOfRow:0] : NSZeroRect;
      const CGFloat stride = outline.numberOfRows > 1 ? [outline rectOfRow:1].origin.y - first.origin.y
                                                     : outline.rowHeight;
      const CGFloat rows = (scroll.contentView.bounds.size.height - first.origin.y) / stride;
      if (!NSContainsRect(projects.bounds, viewport) || NSIntersectsRect(viewport, buttons) ||
          std::abs(rows - std::round(rows)) > 0.02 || rows < 1) {
        error = QStringLiteral("native Projects clips its rows or toolbar (viewport height %1, row height %2)")
                    .arg(scroll.contentView.bounds.size.height).arg(outline.rowHeight);
        return false;
      }
      const NSRange visible = [outline rowsInRect:outline.visibleRect];
      if (visible.length > 0) {
        for (NSUInteger row : {visible.location, NSMaxRange(visible) - 1}) {
          const NSRect rect = [outline rectOfRow:NSInteger(row)];
          if (NSIsEmptyRect(NSIntersectionRect(outline.visibleRect, rect))) continue;
          if (outline.visibleRect.origin.y < 1 && !NSContainsRect(NSInsetRect(outline.visibleRect, -1, -1), rect)) {
            error = QStringLiteral("native Projects clips row %1 (%2,%3 %4x%5; viewport %6,%7 %8x%9)")
                        .arg(row).arg(rect.origin.x).arg(rect.origin.y).arg(rect.size.width).arg(rect.size.height)
                        .arg(outline.visibleRect.origin.x).arg(outline.visibleRect.origin.y)
                        .arg(outline.visibleRect.size.width).arg(outline.visibleRect.size.height);
            return false;
          }
          const NSRect exposed = NSIntersectionRect(outline.visibleRect, rect);
          const NSPoint point = [outline convertPoint:NSMakePoint(NSMidX(exposed), NSMidY(exposed))
                                               toView:content.superview];
          NSView* hit = [content hitTest:point];
          if (hit != outline && ![hit isDescendantOf:outline]) {
            error = QStringLiteral("native Projects rows fall below another overlay");
            return false;
          }
        }
      }
      if (outline.selectedRow >= 0) {
        const NSRect selected = [outline rectOfRow:outline.selectedRow];
        if (!NSContainsRect(NSInsetRect(outline.visibleRect, -1, -1), selected)) {
          NSView* cell = [outline viewAtColumn:0 row:outline.selectedRow makeIfNecessary:YES];
          error = QStringLiteral("native Projects clips the selected row %1 '%2' (%3,%4 %5x%6; viewport %7,%8 %9x%10)")
                      .arg(outline.selectedRow).arg(qt(cell.accessibilityIdentifier))
                      .arg(selected.origin.x).arg(selected.origin.y).arg(selected.size.width).arg(selected.size.height)
                      .arg(outline.visibleRect.origin.x).arg(outline.visibleRect.origin.y)
                      .arg(outline.visibleRect.size.width).arg(outline.visibleRect.size.height);
          return false;
        }
      }
    }
  }
  // Retained Qt badges must not leak into the canvas beneath Browser.
  for (const auto& entry : {qMakePair(QStringLiteral("failuresBadge"), QStringLiteral("native.browser.failures")),
                            qMakePair(QStringLiteral("warningsBadge"), QStringLiteral("native.browser.warnings"))}) {
    QToolButton* presenter = nullptr;
    for (QWidget* widget : QApplication::allWidgets()) {
      if (widget->objectName() == entry.first) presenter = qobject_cast<QToolButton*>(widget);
    }
    if (!presenter) continue;
    if (presenter->isVisible()) {
      error = QStringLiteral("a retained Qt issue badge is painting below the native Browser");
      return false;
    }
    NSView* issue = identifiedView(sidebar, entry.second);
    const bool expected = !presenter->isHidden() && viewVisible(sidebar);
    if (viewVisible(issue) != expected) {
      error = QStringLiteral("native Browser issue '%1' has the wrong visibility").arg(entry.second);
      return false;
    }
    if (!expected) continue;
    const NSRect rect = [issue convertRect:issue.bounds toView:sidebar];
    const NSRect outline = [model.enclosingScrollView convertRect:model.enclosingScrollView.bounds toView:sidebar];
    if (!NSContainsRect(sidebar.bounds, rect) || (!model.enclosingScrollView.hidden && NSIntersectsRect(rect, outline))) {
      error = QStringLiteral("native Browser issue '%1' is clipped or covered by the outline").arg(entry.second);
      return false;
    }
    NSView* hit = [content hitTest:[issue convertPoint:NSMakePoint(NSMidX(issue.bounds), NSMidY(issue.bounds)) toView:content.superview]];
    if (hit != issue || qt(((NSButton*)issue).title) != presenter->text() || qt(issue.toolTip) != presenter->toolTip()) {
      error = QStringLiteral("native Browser issue '%1' lost its shared message or input target").arg(entry.second);
      return false;
    }
  }
  // The edge gaps must still pass pointer input to the full-width canvas.
  for (CGFloat x : {NSMinX(bounds) + 4, NSMaxX(bounds) - 4}) {
    const NSPoint point = NSMakePoint(x, NSMidY(bounds));
    NSView* hit = [content hitTest:[content convertPoint:point toView:content.superview]];
    if (hit != canvas && ![hit isDescendantOf:canvas]) {
      error = QStringLiteral("the canvas edge gap is blocked by an overlay");
      return false;
    }
  }
  return true;
}

bool setAppearance(const QString& name, QString& error) {
  if (name != QStringLiteral("light") && name != QStringLiteral("dark")) {
    error = QStringLiteral("native appearance must be light or dark");
    return false;
  }
  NSAppearance* appearance = [NSAppearance appearanceNamed:(name == QStringLiteral("dark") ? NSAppearanceNameDarkAqua : NSAppearanceNameAqua)];
  if (!appearance) { error = QStringLiteral("the requested native appearance is unavailable"); return false; }
  NSApp.appearance = appearance;
  // Resolve real system roles in this app's appearance, without changing
  // macOS defaults. The application's normal palette watcher must then
  // replace its cached native CAD bitmaps.
  __block QPalette palette = QApplication::palette();
  [appearance performAsCurrentDrawingAppearance:^{
    const auto color = [](NSColor* source) {
      NSColor* rgb = [source colorUsingColorSpace:NSColorSpace.sRGBColorSpace];
      return QColor::fromRgbF(rgb.redComponent, rgb.greenComponent, rgb.blueComponent, rgb.alphaComponent);
    };
    palette.setColor(QPalette::Window, color(NSColor.windowBackgroundColor));
    palette.setColor(QPalette::WindowText, color(NSColor.labelColor));
    palette.setColor(QPalette::Base, color(NSColor.textBackgroundColor));
    palette.setColor(QPalette::Text, color(NSColor.textColor));
    palette.setColor(QPalette::Button, color(NSColor.controlColor));
    palette.setColor(QPalette::ButtonText, color(NSColor.controlTextColor));
    palette.setColor(QPalette::Highlight, color(NSColor.selectedContentBackgroundColor));
    palette.setColor(QPalette::HighlightedText, color(NSColor.selectedTextColor));
    palette.setColor(QPalette::ToolTipBase, color(NSColor.controlBackgroundColor));
    palette.setColor(QPalette::ToolTipText, color(NSColor.labelColor));
    for (QPalette::ColorRole role : {QPalette::WindowText, QPalette::Text, QPalette::ButtonText}) {
      palette.setColor(QPalette::Disabled, role, color(NSColor.disabledControlTextColor));
    }
  }];
  QApplication::setPalette(palette);
  return true;
}

bool historyLayout(const QJsonObject& timeline, const QSet<QString>& filtered, const QStringList& selected,
                   bool checkSelection, QString& error) {
  NSView* content = documentWindow().contentView;
  [content layoutSubtreeIfNeeded];
  NSView* card = identifiedView(content, QStringLiteral("native.historyCard"));
  NSView* strip = identifiedView(content, QStringLiteral("native.historyStrip"));
  NSView* marker = identifiedView(content, QStringLiteral("native.historyMarker"));
  NSScrollView* scroll = strip.enclosingScrollView;
  if (!viewVisible(card) || !viewVisible(strip) || !viewVisible(marker) || !scroll ||
      ![marker isKindOfClass:NSControl.class] || [marker isKindOfClass:NSSlider.class]) {
    error = QStringLiteral("the native history is missing its horizontal feature strip or in-row marker");
    return false;
  }
  if (card.bounds.size.height > timeline::cardHeight || strip.bounds.size.height > timeline::height ||
      scroll.drawsBackground || scroll.contentView.drawsBackground || strip.opaque ||
      scroll.borderType != NSNoBorder) {
    error = QStringLiteral("native history is oversized or draws an opaque inset surface");
    return false;
  }
  const QJsonArray features = timeline.value(QStringLiteral("features")).toArray();
  const int position = timeline.value(QStringLiteral("marker")).toInt(-1);
  if (position < 0 || position > features.size()) {
    error = QStringLiteral("the model has an invalid timeline marker");
    return false;
  }
  int nativeCount = 0;
  for (NSView* child in strip.subviews) {
    if ([child respondsToSelector:@selector(activateHistoryFeature)]) ++nativeCount;
  }
  if (nativeCount != features.size()) {
    error = QStringLiteral("native history has %1 feature icons for %2 model features").arg(nativeCount).arg(features.size());
    return false;
  }
  struct Cell {
    int first;
    NSRect frame;
    bool hiddenRun;
  };
  std::vector<Cell> cells;
  int runs = 0;
  QStringList actualSelected;
  for (int index = 0; index < features.size(); ++index) {
    const QJsonObject feature = features[index].toObject();
    const QString uid = feature.value(QStringLiteral("uid")).toString();
    NSView* view = identifiedView(strip, QStringLiteral("native.history.") + uid);
    if (![view isKindOfClass:NSButton.class] || ![view respondsToSelector:@selector(activateHistoryFeature)]) {
      error = QStringLiteral("history feature '%1' is not a native CAD icon button").arg(uid);
      return false;
    }
    NSButton* button = (NSButton*)view;
    const QString status = feature.value(QStringLiteral("status")).toString();
    if ((status == QStringLiteral("error") || status == QStringLiteral("warning")) &&
        (!button.layer || button.layer.borderWidth < 1 || !button.layer.backgroundColor ||
         CGColorGetAlpha(button.layer.backgroundColor) < 0.15)) {
      error = QStringLiteral("history issue '%1' lacks a visible cell fill and outline").arg(uid);
      return false;
    }
    if (filtered.contains(uid)) {
      if (!button.hidden || button.enabled || button.state == NSControlStateValueOn) {
        error = QStringLiteral("filtered history feature '%1' remains visible, enabled or selected").arg(uid);
        return false;
      }
      const QString previous = index > 0 ? features[index - 1].toObject().value(QStringLiteral("uid")).toString()
                                         : QString();
      if (index > 0 && filtered.contains(previous)) continue;
      NSView* mark = identifiedView(strip, QStringLiteral("native.history.filteredRun.%1").arg(index));
      const NSRect frame = [mark convertRect:mark.bounds toView:strip];
      if (![mark isKindOfClass:NSImageView.class] || !viewVisible(mark) ||
          frame.size.width <= 0 || frame.size.width > 16 || !NSContainsRect(strip.bounds, frame) ||
          (!cells.empty() && NSMaxX(cells.back().frame) >= NSMinX(frame))) {
        error = QStringLiteral("filtered history run at %1 has no compact native mark").arg(index);
        return false;
      }
      int count = 1;
      while (index + count < features.size() &&
             filtered.contains(features[index + count].toObject().value(QStringLiteral("uid")).toString())) ++count;
      if (!qt(mark.toolTip).startsWith(QStringLiteral("%1 feature(s) hidden").arg(count))) {
        error = QStringLiteral("filtered history run at %1 reports the wrong hidden count").arg(index);
        return false;
      }
      cells.push_back({index, frame, true});
      ++runs;
      continue;
    }
    if (button.hidden) {
      error = QStringLiteral("unfiltered history feature '%1' is hidden").arg(uid);
      return false;
    }
    const NSRect frame = [button convertRect:button.bounds toView:strip];
    if (std::abs(frame.size.width - timeline::cell) > 1 || std::abs(frame.size.height - timeline::cell) > 1 ||
        button.bordered || !NSContainsRect(strip.bounds, frame) ||
        (!cells.empty() && (NSMaxX(cells.back().frame) >= NSMinX(frame) ||
                            std::abs(NSMidY(cells.back().frame) - NSMidY(frame)) > 1))) {
      error = QStringLiteral("native history feature '%1' is clipped, overlapping or outside the horizontal row").arg(uid);
      return false;
    }
    cells.push_back({index, frame, false});
    NSView* hit = [content hitTest:[button convertPoint:NSMakePoint(NSMidX(button.bounds), NSMidY(button.bounds))
                                               toView:content.superview]];
    if (hit != button) {
      error = QStringLiteral("native history feature '%1' cannot receive pointer input above the canvas").arg(uid);
      return false;
    }
    if (!coloredIconMatches(button, themeIcon(featureIcon(feature.value(QStringLiteral("type")).toString())), error)) return false;
    const bool dimmed = index >= position || feature.value(QStringLiteral("suppressed")).toBool();
    if (dimmed ? button.alphaValue > 0.6 || button.alphaValue <= 0.1 : button.alphaValue < 0.95) {
      error = QStringLiteral("native history feature '%1' does not visually reflect rollback/suppression").arg(uid);
      return false;
    }
    if (button.state == NSControlStateValueOn) actualSelected.append(uid);
  }
  int nativeRuns = 0;
  for (NSView* child in strip.subviews) {
    if (qt(child.accessibilityIdentifier).startsWith(QStringLiteral("native.history.filteredRun.")) && !child.hidden) ++nativeRuns;
  }
  if (nativeRuns != runs) {
    error = QStringLiteral("native history has %1 run marks for %2 hidden runs").arg(nativeRuns).arg(runs);
    return false;
  }
  const NSRect markerFrame = [marker convertRect:marker.bounds toView:strip];
  NSView* markerHit = [content hitTest:[marker convertPoint:NSMakePoint(NSMidX(marker.bounds), NSMidY(marker.bounds))
                                                 toView:content.superview]];
  if (markerHit != marker) {
    error = QStringLiteral("the native history marker cannot receive pointer input above the canvas");
    return false;
  }
  bool between = NSContainsRect(strip.bounds, markerFrame);
  for (const Cell& cell : cells) {
    const CGFloat left = cell.hiddenRun ? NSMidX(markerFrame) : NSMinX(markerFrame) + 1;
    const CGFloat right = cell.hiddenRun ? NSMidX(markerFrame) : NSMaxX(markerFrame) - 1;
    between = between && (cell.first < position ? left >= NSMaxX(cell.frame) : right <= NSMinX(cell.frame));
  }
  if (!between) {
    error = QStringLiteral("the native marker is not between feature icons at model position %1").arg(position);
    return false;
  }
  if (checkSelection && actualSelected != selected) {
    error = QStringLiteral("native selected feature icons '%1' differ from '%2'")
        .arg(actualSelected.join(QLatin1Char(',')), selected.join(QLatin1Char(',')));
    return false;
  }
  return true;
}

bool historyWheel(QString& error) {
  NSWindow* window = documentWindow();
  NSView* view = identifiedView(window.contentView, QStringLiteral("native.historyScroll"));
  if (![view isKindOfClass:NSScrollView.class]) {
    error = QStringLiteral("the native history scroll view is missing");
    return false;
  }
  NSScrollView* scroll = (NSScrollView*)view;
  [window layoutIfNeeded];
  [window.contentView layoutSubtreeIfNeeded];
  NSClipView* clip = scroll.contentView;
  const CGFloat maximum = scroll.documentView.frame.size.width - clip.bounds.size.width;
  if (maximum < 50) {
    error = QStringLiteral("the history fixture does not overflow its viewport");
    return false;
  }
  const auto send = [&](CGFloat x, CGFloat y, bool precise) {
    const NSPoint middle = NSMakePoint(NSMidX(clip.bounds), NSMidY(clip.bounds));
    const NSPoint point = [clip convertPoint:middle toView:nil];
    NSView* hit = [window.contentView hitTest:[window.contentView.superview convertPoint:point fromView:nil]];
    if (hit == nil || (hit != scroll && ![hit isDescendantOf:scroll])) { return false; }
    MITCADTestScrollEvent* event = [MITCADTestScrollEvent new];
    event->target = window;
    event->point = point;
    event->dx = x;
    event->dy = y;
    event->precise = precise;
    [hit scrollWheel:event];
    return true;
  };
  [clip scrollToPoint:NSZeroPoint];
  [scroll reflectScrolledClipView:clip];
  if (!send(0, -1, false) || clip.bounds.origin.x <= 0) {
    error = QStringLiteral("a vertical mouse wheel did not move the native history towards newer features");
    return false;
  }
  const CGFloat oneLine = clip.bounds.origin.x;
  for (int i = 0; i < int(std::ceil(maximum / std::max<CGFloat>(oneLine, 1))) + 2; ++i) {
    if (!send(0, -1, false)) { error = QStringLiteral("the history wheel left its viewport"); return false; }
  }
  if (std::abs(clip.bounds.origin.x - maximum) > 1 || !send(0, -2, false) ||
      std::abs(clip.bounds.origin.x - maximum) > 1) {
    error = QStringLiteral("the native history wheel did not stop at the newer end");
    return false;
  }
  if (!send(0, maximum + 100, true) || std::abs(clip.bounds.origin.x) > 1) {
    error = QStringLiteral("precise history scrolling did not reach the older end");
    return false;
  }
  if (!send(-17.5, -2, true) || std::abs(clip.bounds.origin.x - 17.5) > 1) {
    error = QStringLiteral("horizontal/diagonal trackpad history scrolling lost its point delta");
    return false;
  }
  if (!send(0, 100, true) || std::abs(clip.bounds.origin.x) > 1) {
    error = QStringLiteral("the native history did not return to its older end");
    return false;
  }
  qInfo().noquote() << QStringLiteral("Native history wheel: mouse and precise trackpad, both endpoints, maximum %1 points").arg(maximum);
  return true;
}

bool historyMarkerEvent(const QString& phase, int index, QString& error) {
  NSView* marker = phase == QStringLiteral("begin")
      ? identifiedView(documentWindow().contentView, QStringLiteral("native.historyMarker")) : draggedHistoryMarker;
  NSView* strip = marker.superview;
  if (!viewVisible(marker) || ![marker isKindOfClass:NSControl.class] || !((NSControl*)marker).enabled ||
      ![marker respondsToSelector:@selector(beginHistoryMarkerDrag)]) {
    error = QStringLiteral("no draggable native history marker");
    return false;
  }
  NSEventType type = NSEventTypeLeftMouseDragged;
  if (phase == QStringLiteral("begin")) {
    draggedHistoryMarker = marker;
    historyDragPoint = [marker convertPoint:NSMakePoint(NSMidX(marker.bounds), NSMidY(marker.bounds)) toView:nil];
    [marker.window makeKeyAndOrderFront:nil];
    type = NSEventTypeLeftMouseDown;
  } else if (phase == QStringLiteral("to")) {
    std::vector<NSView*> features;
    for (NSView* child in strip.subviews) {
      if ([child respondsToSelector:@selector(activateHistoryFeature)]) features.push_back(child);
    }
    std::sort(features.begin(), features.end(), [](NSView* a, NSView* b) { return NSMinX(a.frame) < NSMinX(b.frame); });
    if (features.empty() || index < 0 || index > int(features.size())) {
      error = QStringLiteral("native history drag index %1 is outside the feature row").arg(index);
      return false;
    }
    CGFloat x = 0;
    if (index == 0) {
      const CGFloat gap = features.size() > 1 ? NSMinX(features[1].frame) - NSMaxX(features[0].frame) : marker.bounds.size.width;
      x = NSMinX(features[0].frame) - gap / 2;
    } else if (index == int(features.size())) {
      const CGFloat gap = features.size() > 1 ? NSMinX(features.back().frame) - NSMaxX(features[features.size() - 2].frame)
                                             : marker.bounds.size.width;
      x = NSMaxX(features.back().frame) + gap / 2;
    } else {
      x = (NSMaxX(features[size_t(index - 1)].frame) + NSMinX(features[size_t(index)].frame)) / 2;
    }
    historyDragPoint = [strip convertPoint:NSMakePoint(x, NSMidY(marker.frame)) toView:nil];
  } else if (phase == QStringLiteral("end")) {
    type = NSEventTypeLeftMouseUp;
  } else {
    error = QStringLiteral("native history drag phase must be begin, to or end");
    return false;
  }
  NSEvent* event = [NSEvent mouseEventWithType:type location:historyDragPoint modifierFlags:0
      timestamp:NSProcessInfo.processInfo.systemUptime windowNumber:marker.window.windowNumber
      context:nil eventNumber:0 clickCount:1 pressure:(type == NSEventTypeLeftMouseUp ? 0 : 1)];
  if (type == NSEventTypeLeftMouseDown) [marker mouseDown:event];
  else if (type == NSEventTypeLeftMouseDragged) [marker mouseDragged:event];
  else { [marker mouseUp:event]; draggedHistoryMarker = nil; }
  return true;
}

bool clickImmediately(const QString& identifier, QString& error) {
  // Job-start input must reach the existing document button before a
  // progress sheet or performClick's animation starts another event loop.
  NSView* view = identifiedView(documentWindow().contentView, identifier);
  if (![view isKindOfClass:NSButton.class] || [view isHiddenOrHasHiddenAncestor] || !((NSButton*)view).enabled) {
    error = QStringLiteral("no enabled native document button '%1'").arg(identifier);
    return false;
  }
  NSButton* button = (NSButton*)view;
  if (!button.action || ![button sendAction:button.action to:button.target]) {
    error = QStringLiteral("native button '%1' could not dispatch its action").arg(identifier);
    return false;
  }
  return true;
}

namespace {
// A model cell's text field, in the view its table shows once the cell is
// scrolled in (mitcad#814): a column right of a narrow table's visible width
// first has only a prepared view, which the table replaces when it lays out
// the scrolled columns. AppKit's own editColumn:row: is the fallback. The
// log line says what happened.
bool clickTableCell(NSTableView* table, NSInteger row, NSTableCellView* prepared, const QString& identifier) {
  NSInteger column = [table columnForView:prepared];
  for (NSInteger candidate = 0; column < 0 && candidate < table.numberOfColumns; ++candidate) {
    if (qt([table viewAtColumn:candidate row:row makeIfNecessary:YES].accessibilityIdentifier) == identifier) column = candidate;
  }
  [table.window makeKeyAndOrderFront:nil];
  [table scrollRowToVisible:row];
  if (column >= 0) [table scrollColumnToVisible:column];
  table.needsLayout = YES;
  [table layoutSubtreeIfNeeded];
  [table displayIfNeeded];
  NSView* live = column >= 0 ? [table viewAtColumn:column row:row makeIfNecessary:NO] : nil;
  NSTextField* field = [live isKindOfClass:NSTableCellView.class] ? ((NSTableCellView*)live).textField : prepared.textField;
  BOOL responder = NO;
  if (field.window != nil) {
    [field scrollRectToVisible:field.bounds];
    responder = [field.window makeFirstResponder:field];
    // selectText: on the field editor's own field would end that edit and
    // begin another, committing the cell: select within the edit instead.
    if (responder && field.currentEditor != nil) [field.currentEditor selectAll:nil];
    else if (responder) [field selectText:nil];
  }
  BOOL viaTable = NO;
  if (field.currentEditor == nil && field.editable && column >= 0) {
    [table editColumn:column row:row withEvent:nil select:YES];
    viaTable = YES;
    NSView* edited = [table viewAtColumn:column row:row makeIfNecessary:NO];
    if ([edited isKindOfClass:NSTableCellView.class] && ((NSTableCellView*)edited).textField) field = ((NSTableCellView*)edited).textField;
  }
  qInfo().noquote() << QStringLiteral("Native test: click cell %1 row %2 column %3: live view %4, in window %5, "
                                      "editable %6, first responder %7, editColumn %8, editing %9")
                           .arg(identifier).arg(qlonglong(row)).arg(qlonglong(column)).arg(live != nil)
                           .arg(field.window != nil).arg(bool(field.editable)).arg(bool(responder)).arg(bool(viaTable))
                           .arg(field.currentEditor != nil);
  return true;
}
} // namespace

bool click(const QString& identifier, QString& error) {
  for (NSToolbarItem* item in documentWindow().toolbar.items) {
    if (qt(item.itemIdentifier) != identifier || !item.action) continue;
    if (!item.enabled) { error = QStringLiteral("native toolbar item '%1' is disabled").arg(identifier); return false; }
    return [NSApp sendAction:item.action to:item.target from:item];
  }
  NSTableView* table = nil;
  NSInteger row = -1;
  NSView* view = findControl(identifier, &table, &row);
  if (!view) { error = QStringLiteral("no native control '%1'").arg(identifier); return false; }
  if ([view isKindOfClass:NSTableCellView.class] && ((NSTableCellView*)view).textField) {
    if (table != nil && row >= 0) return clickTableCell(table, row, (NSTableCellView*)view, identifier);
    view = ((NSTableCellView*)view).textField;
  }
  [view scrollRectToVisible:view.bounds];
  [view.window makeKeyAndOrderFront:nil];
  if ([view isKindOfClass:NSButton.class]) {
    NSButton* button = (NSButton*)view;
    if (!button.enabled) { error = QStringLiteral("native button '%1' is disabled").arg(identifier); return false; }
    [button performClick:nil];
    return true;
  }
  if ([view isKindOfClass:NSTextField.class]) {
    [view.window makeFirstResponder:view];
    [(NSTextField*)view selectText:nil];
    return true;
  }
  if ([view isKindOfClass:NSControl.class]) {
    NSControl* control = (NSControl*)view;
    if (!control.enabled) { error = QStringLiteral("native control '%1' is disabled").arg(identifier); return false; }
    [control sendAction:control.action to:control.target];
    return true;
  }
  error = QStringLiteral("native view '%1' is not an actionable control").arg(identifier);
  return false;
}

bool hover(const QString& identifier, bool enter, QString& error) {
  NSView* view = findControl(identifier);
  if (!view) { error = QStringLiteral("no native control '%1'").arg(identifier); return false; }
  [view.window makeKeyAndOrderFront:nil];
  [view updateTrackingAreas];
  for (NSTrackingArea* area in view.trackingAreas) {
    if ((area.options & NSTrackingMouseEnteredAndExited) == 0 || area.owner != view) { continue; }
    const NSPoint location = [view convertPoint:NSMakePoint(NSMidX(view.bounds), NSMidY(view.bounds)) toView:nil];
    NSEvent* event = [NSEvent enterExitEventWithType:enter ? NSEventTypeMouseEntered : NSEventTypeMouseExited
                                        location:location modifierFlags:0 timestamp:NSProcessInfo.processInfo.systemUptime
                                    windowNumber:view.window.windowNumber context:nil eventNumber:0 trackingNumber:0 userData:nullptr];
    // Exercise the real AppKit tracking owner; it is responsible for
    // forwarding the event to the hidden Qt presenter.
    if (enter) { [view mouseEntered:event]; }
    else { [view mouseExited:event]; }
    return true;
  }
  error = QStringLiteral("native control '%1' has no hover tracking area").arg(identifier);
  return false;
}

bool replaceText(const QString& identifier, const QString& text, QString& error) {
  NSView* view = findControl(identifier);
  if (![view isKindOfClass:NSTextField.class] || !((NSTextField*)view).editable || !((NSTextField*)view).enabled) {
    error = QStringLiteral("no editable native field '%1'").arg(identifier);
    return false;
  }
  NSTextField* field = (NSTextField*)view;
  [field scrollRectToVisible:field.bounds];
  [field.window makeKeyAndOrderFront:nil];
  [field.window makeFirstResponder:field];
  [field selectText:nil];
  NSTextView* editor = (NSTextView*)field.currentEditor;
  if (!editor) { error = QStringLiteral("native field '%1' has no field editor").arg(identifier); return false; }
  // AppKit's own field editor emits the real editing notifications, which
  // drive command state, validation and preview through the presenter.
  [editor insertText:text.toNSString() replacementRange:NSMakeRange(0, editor.string.length)];
  return true;
}

bool choose(const QString& identifier, const QString& title, QString& error) {
  NSView* view = findControl(identifier);
  if ([view isKindOfClass:NSTabView.class]) {
    NSTabView* tabs = (NSTabView*)view;
    for (NSTabViewItem* item in tabs.tabViewItems) {
      if (qt(item.label).compare(title, Qt::CaseInsensitive) == 0) {
        [tabs selectTabViewItem:item];
        return true;
      }
    }
    error = QStringLiteral("native tabs '%1' have no '%2' page").arg(identifier, title);
    return false;
  }
  if ([view isKindOfClass:NSSegmentedControl.class]) {
    NSSegmentedControl* segments = (NSSegmentedControl*)view;
    for (NSInteger i = 0; segments.enabled && i < segments.segmentCount; ++i) {
      if (qt([segments labelForSegment:i]).compare(title, Qt::CaseInsensitive) == 0) {
        segments.selectedSegment = i;
        [segments sendAction:segments.action to:segments.target];
        return true;
      }
    }
    error = QStringLiteral("native workspace '%1' has no '%2' segment").arg(identifier, title);
    return false;
  }
  if (![view isKindOfClass:NSPopUpButton.class] || !((NSPopUpButton*)view).enabled) {
    error = QStringLiteral("no native choice '%1'").arg(identifier); return false;
  }
  NSPopUpButton* popup = (NSPopUpButton*)view;
  const NSInteger index = [popup indexOfItemWithTitle:title.toNSString()];
  if (index < 0) { error = QStringLiteral("native choice '%1' has no '%2' item").arg(identifier, title); return false; }
  if (popup.pullsDown) {
    [popup.menu performActionForItemAtIndex:index];
    return true;
  }
  [popup selectItemAtIndex:index];
  [popup sendAction:popup.action to:popup.target];
  return true;
}

bool disclosureArtwork(NSButton* button, QString& error) {
  // Compare rendered cells with a fresh native disclosure. Checking only
  // bezelStyle misses a cell that still draws inherited switch artwork.
  NSButton* reference = [[NSButton alloc] initWithFrame:NSMakeRect(0, 0, 24, 24)];
  [reference setButtonType:NSButtonTypePushOnPushOff];
  reference.bezelStyle = NSBezelStyleDisclosure;
  reference.title = @"";
  reference.font = button.font;
  reference.controlSize = button.controlSize;
  reference.appearance = button.effectiveAppearance;
  reference.state = button.state;
  const auto pixels = [](NSButton* control) {
    NSBitmapImageRep* bitmap = [[NSBitmapImageRep alloc] initWithBitmapDataPlanes:nil
        pixelsWide:24 pixelsHigh:24 bitsPerSample:8 samplesPerPixel:4 hasAlpha:YES isPlanar:NO
        colorSpaceName:NSDeviceRGBColorSpace bytesPerRow:96 bitsPerPixel:32];
    [NSGraphicsContext saveGraphicsState];
    NSGraphicsContext.currentContext = [NSGraphicsContext graphicsContextWithBitmapImageRep:bitmap];
    [NSColor.clearColor setFill];
    NSRectFillUsingOperation(NSMakeRect(0, 0, 24, 24), NSCompositingOperationCopy);
    [control.cell drawWithFrame:NSMakeRect(0, 0, 24, 24) inView:control];
    [NSGraphicsContext restoreGraphicsState];
    NSData* png = [bitmap representationUsingType:NSBitmapImageFileTypePNG properties:@{}];
    return QImage::fromData(static_cast<const uchar*>(png.bytes), int(png.length), "PNG");
  };
  const QImage actual = pixels(button);
  const QImage expected = pixels(reference);
  if (actual.isNull() || expected.isNull() || actual != expected) {
    error = QStringLiteral("native disclosure does not render the system disclosure triangle");
    return false;
  }
  return true;
}

bool formControls(CommandHost& host, QString& error) {
  // BUILD no longer has a More section. Exercise the reusable disclosure
  // contract here, independently of any command's current arrangement.
  CommandDef def;
  def.id = QStringLiteral("test.native_form_controls");
  def.name = QStringLiteral("Native form controls regression");
  def.inspect = [](const CommandState&, const CommandContext&) { return Inspection{}; };
  def.inputs = {
      choiceInput(QStringLiteral("axis"), QStringLiteral("Direction"),
                  {{QStringLiteral("x"), QStringLiteral("Along X")},
                   {QStringLiteral("y"), QStringLiteral("Along Y")},
                   {QStringLiteral("z"), QStringLiteral("Along Z")}}, QStringLiteral("z"))
          .withChoiceIcons({{QStringLiteral("x"), QStringLiteral("build-axis-x")},
                            {QStringLiteral("y"), QStringLiteral("build-axis-y")},
                            {QStringLiteral("z"), QStringLiteral("build-axis-z")}})
          .withTooltip(QStringLiteral("The axis")),
      checkInput(QStringLiteral("more"), QStringLiteral("More")).withDisclosure()};
  for (int index = 0; index < 8; ++index) {
    def.inputs.append(textInput(QStringLiteral("detail%1").arg(index), QStringLiteral("Detail"))
                          .withVisible([](const CommandState& state) {
                            return state.checked(QStringLiteral("more"));
                          }));
  }
  const auto check = [&error](bool passed, const QString& message) {
    if (!passed) error = message;
    return passed;
  };
  {
    CommandSession session(def, host);
    auto form = createNativeCommandForm(session);
    NSView* root = (__bridge NSView*)form->nativeView();
    [root.widthAnchor constraintEqualToConstant:280].active = YES;
    CommandState state;
    state.setChoice(QStringLiteral("axis"), QStringLiteral("z"));
    form->refresh(state);
    const auto button = [root](const QString& identifier) -> NSButton* {
      NSView* view = identifiedView(root, identifier);
      return [view isKindOfClass:NSButton.class] ? (NSButton*)view : nil;
    };
    NSButton* x = button(QStringLiteral("input_axis_x"));
    NSButton* y = button(QStringLiteral("input_axis_y"));
    NSButton* z = button(QStringLiteral("input_axis_z"));
    NSButton* more = button(QStringLiteral("input_more"));
    if (!check(x && y && z && more && x.enabled && y.enabled && z.enabled &&
               x.state == NSControlStateValueOff && z.state == NSControlStateValueOn &&
               qt(x.toolTip) == QStringLiteral("Along X: The axis") &&
               qt(x.accessibilityLabel) == QStringLiteral("Along X") &&
               more.bezelStyle == NSBezelStyleDisclosure && qt(more.accessibilityLabel) == QStringLiteral("More"),
               QStringLiteral("native icon choices or disclosure have the wrong controls, tooltips or initial state"))) return false;
    for (const auto& axis : {QStringLiteral("x"), QStringLiteral("y"), QStringLiteral("z")}) {
      if (!coloredIconMatches(button(QStringLiteral("input_axis_") + axis),
                              themeIcon(QStringLiteral("build-axis-") + axis), error)) return false;
    }
    QString submitted;
    CommandPresenter::Actions actions = form->actions();
    actions.choiceChanged = [&](const QString& key, const QString& value) {
      submitted = value;
      state.setChoice(key, value);
      form->refresh(state);
    };
    actions.checkChanged = [&](const QString& key, bool checked) {
      state.setChecked(key, checked);
      form->refresh(state);
    };
    form->setActions(std::move(actions));
    for (NSButton* selected in @[x, y, y, z]) {
      // Offscreen contract checks dispatch synchronously. performClick's
      // animation spins an event loop and can advance the UI script before
      // this fixture has been installed in the document.
      selected.state = selected.state == NSControlStateValueOn ? NSControlStateValueOff : NSControlStateValueOn;
      [selected sendAction:selected.action to:selected.target];
      if (!check(submitted == qt(selected.identifier).section(QLatin1Char('|'), 2) &&
                 selected.state == NSControlStateValueOn &&
                 (x.state == NSControlStateValueOn) + (y.state == NSControlStateValueOn) +
                     (z.state == NSControlStateValueOn) == 1,
                 QStringLiteral("native icon choice lost its submitted value or exclusive selection"))) return false;
    }
    NSView* detail = identifiedView(root, QStringLiteral("row_detail0"));
    [root layoutSubtreeIfNeeded];
    const CGFloat collapsedHeight = root.fittingSize.height;
    if (!check(detail.hidden && more.state == NSControlStateValueOff,
               QStringLiteral("native disclosure did not initially hide its dependent rows"))) return false;
    if (!disclosureArtwork(more, error)) return false;
    more.state = NSControlStateValueOn;
    [more sendAction:more.action to:more.target];
    [root layoutSubtreeIfNeeded];
    if (!check(state.checked(QStringLiteral("more")) && !detail.hidden &&
               more.state == NSControlStateValueOn && root.fittingSize.height > collapsedHeight + 100,
               QStringLiteral("native disclosure did not expand its dependent rows and form height"))) return false;
    if (!disclosureArtwork(more, error)) return false;
    more.state = NSControlStateValueOff;
    [more sendAction:more.action to:more.target];
    [root layoutSubtreeIfNeeded];
    if (!check(!state.checked(QStringLiteral("more")) && detail.hidden &&
               std::abs(root.fittingSize.height - collapsedHeight) < 1,
               QStringLiteral("native disclosure left empty layout space after collapse"))) return false;
    state.setChoice(QStringLiteral("axis"), QStringLiteral("x"));
    form->refresh(state);
    if (!check(x.state == NSControlStateValueOn && z.state == NSControlStateValueOff,
               QStringLiteral("native icon choice did not follow a programmatic state refresh"))) return false;
    qDebug() << "Native form controls: shared artwork, exclusive choice values and disclosure layout verified";
  }
  // Mount the same fixture through the ordinary command controller so the
  // script also exercises the real inspector's cap and scrolling.
  auto* window = dynamic_cast<MainWindow*>(&host);
  auto* registry = window ? window->findChild<CommandRegistry*>() : nullptr;
  if (!check(registry != nullptr, QStringLiteral("native form fixture has no command registry"))) return false;
  if (!registry->find(def.id)) { registry->add(def); }
  window->startCommand(def.id, {}, {});
  return true;
}

bool disclosureLayout(QString& error) {
  NSView* content = documentWindow().contentView;
  [content layoutSubtreeIfNeeded];
  NSView* card = identifiedView(content, QStringLiteral("native.inspectorCard"));
  NSView* root = identifiedView(card, QStringLiteral("commandPanel"));
  NSView* last = identifiedView(root, QStringLiteral("input_detail7"));
  NSView* more = identifiedView(root, QStringLiteral("input_more"));
  if (!root || !last || ![more isKindOfClass:NSButton.class] || !root.enclosingScrollView) {
    error = QStringLiteral("mounted disclosure fixture is missing");
    return false;
  }
  const bool expanded = ((NSButton*)more).state == NSControlStateValueOn;
  NSView* history = identifiedView(content, QStringLiteral("native.historyCard"));
  const CGFloat floor = !history || history.hidden ? content.bounds.size.height - 12
                                                   : history.frame.origin.y - 8;
  const CGFloat available = std::max(CGFloat(1), floor - card.frame.origin.y);
  // The form at the card's width, with its OK and Cancel under it (mitcad#815).
  NSView* footer = identifiedView(card, QStringLiteral("commandButtons"));
  const CGFloat form = std::ceil(NSHeight(root.superview.frame) + (footer ? footer.fittingSize.height : 0));
  const CGFloat expected = std::min(available, std::max(CGFloat(160), form + 4));
  if (std::abs(card.frame.size.height - expected) > 1 ||
      static_cast<bool>(last.hiddenOrHasHiddenAncestor) == expanded) {
    error = QStringLiteral("mounted disclosure height %1, expected %2 (form %3, available %4, expanded %5, last hidden %6)")
                .arg(card.frame.size.height).arg(expected).arg(form).arg(available)
                .arg(expanded).arg(static_cast<bool>(last.hiddenOrHasHiddenAncestor));
    return false;
  }
  if (expanded) {
    [last scrollRectToVisible:last.bounds];
    [content layoutSubtreeIfNeeded];
    if (NSIsEmptyRect(NSIntersectionRect(last.visibleRect, last.bounds))) {
      error = QStringLiteral("expanded disclosure's final input cannot be reached by scrolling");
      return false;
    }
    [more scrollRectToVisible:more.bounds];
  }
  return true;
}

bool stepperLimitsContract(QString& error) {
  QWidget presenter;
  presenter.setAttribute(Qt::WA_DontShowOnScreen);
  auto* layout = new QVBoxLayout(&presenter);
  auto* integer = new QSpinBox;
  integer->setObjectName(QStringLiteral("limits.integer"));
  integer->setRange(1, 65536);
  auto* decimal = new QDoubleSpinBox;
  decimal->setObjectName(QStringLiteral("limits.decimal"));
  decimal->setRange(0.5, 10);
  decimal->setSingleStep(0.5);
  layout->addWidget(integer);
  layout->addWidget(decimal);
  auto view = createNativeWidgetView(&presenter);
  view->refresh();
  NSView* root = (__bridge NSView*)view->nativeView();
  NSWindow* fixture = [[NSWindow alloc] initWithContentRect:NSMakeRect(0, 0, 420, 160)
                                                styleMask:NSWindowStyleMaskBorderless
                                                  backing:NSBackingStoreBuffered defer:NO];
  fixture.releasedWhenClosed = NO;
  fixture.contentView = root;
  [root layoutSubtreeIfNeeded];
  const auto stepperIn = [](NSView* parent, const auto& self) -> NSStepper* {
    if ([parent isKindOfClass:NSStepper.class]) { return (NSStepper*)parent; }
    for (NSView* child in parent.subviews) {
      if (NSStepper* found = self(child, self)) { return found; }
    }
    return nil;
  };
  const auto press = [fixture](NSStepper* stepper, bool up) {
    const CGFloat y = NSMinY(stepper.bounds) + stepper.bounds.size.height * (up ? 0.75 : 0.25);
    const NSPoint point = [stepper convertPoint:NSMakePoint(NSMidX(stepper.bounds), y) toView:nil];
    const NSTimeInterval timestamp = NSProcessInfo.processInfo.systemUptime;
    NSEvent* down = [NSEvent mouseEventWithType:NSEventTypeLeftMouseDown location:point modifierFlags:0
        timestamp:timestamp windowNumber:fixture.windowNumber context:nil eventNumber:0 clickCount:1 pressure:1];
    NSEvent* release = [NSEvent mouseEventWithType:NSEventTypeLeftMouseUp location:point modifierFlags:0
        timestamp:timestamp + 0.01 windowNumber:fixture.windowNumber context:nil eventNumber:0 clickCount:1 pressure:0];
    [NSApp postEvent:release atStart:YES];
    [stepper mouseDown:down];
  };
  bool passed = true;
  for (QAbstractSpinBox* spin : {static_cast<QAbstractSpinBox*>(integer), static_cast<QAbstractSpinBox*>(decimal)}) {
    // Composite spin controls identify their editable field. The stepper is
    // its sibling in the same row, not a descendant of the field.
    NSView* field = identifiedView(root, spin->objectName());
    NSStepper* native = stepperIn(field.superview, stepperIn);
    if (native == nil || native.valueWraps || !native.autorepeat) { passed = false; break; }
    const bool integral = spin == integer;
    if (integral) { integer->setValue(integer->minimum()); }
    else { decimal->setValue(decimal->minimum()); }
    view->refresh();
    press(native, false);
    if ((integral ? integer->value() : decimal->value()) != (integral ? integer->minimum() : decimal->minimum())) {
      passed = false; break;
    }
    if (integral) { integer->setValue(integer->maximum()); }
    else { decimal->setValue(decimal->maximum()); }
    view->refresh();
    press(native, true);
    if ((integral ? integer->value() : decimal->value()) != (integral ? integer->maximum() : decimal->maximum())) {
      passed = false; break;
    }
    spin->setWrapping(true);
    view->refresh();
    if (!native.valueWraps) { passed = false; break; }
  }
  [fixture close];
  if (!passed) { error = QStringLiteral("native spin boxes did not preserve range limits, wrapping or autorepeat"); }
  return passed;
}

bool labelTextContract(QString& error) {
  QWidget presenter;
  presenter.setAttribute(Qt::WA_DontShowOnScreen);
  auto* layout = new QVBoxLayout(&presenter);
  auto* literal = new QLabel(QStringLiteral("R&D.iam"));
  auto* mnemonic = new QLabel(QStringLiteral("&Name && value"));
  auto* buddy = new QLineEdit;
  mnemonic->setBuddy(buddy);
  const QString markup = QStringLiteral("<a href=\"https://example.invalid/?a=1&amp;b=2\">R&amp;D</a>");
  auto* plain = new QLabel(markup);
  plain->setTextFormat(Qt::PlainText);
  auto* rich = new QLabel(markup);
  rich->setTextFormat(Qt::RichText);
  QString activated;
  QObject::connect(rich, &QLabel::linkActivated, &presenter, [&activated](const QString& url) { activated = url; });
  for (auto* caption : {literal, mnemonic, plain, rich}) {
    caption->setObjectName(QStringLiteral("label.%1").arg(layout->count()));
    layout->addWidget(caption);
  }
  layout->addWidget(buddy);
  auto view = createNativeWidgetView(&presenter);
  view->refresh();
  NSView* root = (__bridge NSView*)view->nativeView();
  const auto field = [root](const QLabel* caption) -> NSTextField* {
    return (NSTextField*)identifiedView(root, caption->objectName());
  };
  const auto textViewIn = [](NSView* parent, const auto& self) -> NSTextView* {
    if ([parent isKindOfClass:NSTextView.class]) { return (NSTextView*)parent; }
    for (NSView* child in parent.subviews) {
      if (NSTextView* found = self(child, self)) { return found; }
    }
    return nil;
  };
  if (qt(field(literal).stringValue) != QStringLiteral("R&D.iam") ||
      qt(field(mnemonic).stringValue) != QStringLiteral("Name & value") ||
      qt(field(plain).stringValue) != markup) {
    error = QStringLiteral("native labels stripped literal ampersands or parsed PlainText as HTML");
    return false;
  }
  // The link's displayed entity and destination survive HTML decoding.
  NSTextField* richField = field(rich);
  NSTextView* links = textViewIn(richField.superview, textViewIn);
  if (links == nil || qt(links.string) != QStringLiteral("R&D")) {
    error = QStringLiteral("native rich label lost escaped ampersands");
    return false;
  }
  id destination = [links.textStorage attribute:NSLinkAttributeName atIndex:0 effectiveRange:nullptr];
  if (destination == nil || ![links.delegate textView:links clickedOnLink:destination atIndex:0] ||
      activated != QStringLiteral("https://example.invalid/?a=1&b=2")) {
    error = QStringLiteral("native rich label did not preserve its linkActivated destination");
    return false;
  }
  rich->setTextFormat(Qt::PlainText);
  literal->setBuddy(buddy);
  view->refresh();
  if (!links.hidden || richField.hidden || qt(richField.stringValue) != markup ||
      qt(field(literal).stringValue) != QStringLiteral("RD.iam")) {
    error = QStringLiteral("native label refresh ignored text format or buddy changes");
    return false;
  }
  rich->setTextFormat(Qt::RichText);
  literal->setBuddy(nullptr);
  view->refresh();
  if (links.hidden || qt(field(literal).stringValue) != QStringLiteral("R&D.iam")) {
    error = QStringLiteral("native label refresh did not restore links and literal ampersands");
    return false;
  }
  return true;
}

bool settingsCloseContract(QString& error) {
  QPointer<SettingsWindow> settings = new SettingsWindow(nullptr);
  settings->setWindowTitle(QStringLiteral("Native cache close regression"));
  auto* cache = new CachePreferencesBox({});
  settings->addPane(QStringLiteral("Cache"), {}, cache);
  settings->showPane(0);
  QSpinBox* disk = nullptr;
  QSpinBox* memory = nullptr;
  for (QSpinBox* spin : cache->findChildren<QSpinBox*>()) {
    // Keep an actual cache edit pending without applying budgets to the
    // model or disk store in this focused native close regression.
    spin->setKeyboardTracking(false);
    if (spin->minimum() == CacheSettings::kMinDiskGigabytes) { disk = spin; }
    else { memory = spin; }
  }
  const auto fail = [&](const QString& message) {
    error = message;
    delete settings.data();
    QCoreApplication::processEvents(QEventLoop::AllEvents);
    return false;
  };
  if (disk == nullptr || memory == nullptr) { return fail(QStringLiteral("cache close fixture has no budget controls")); }
  disk->setObjectName(QStringLiteral("close.cache.disk"));
  cache->findChild<QCheckBox*>()->setChecked(true);
  disk->setValue(6);
  memory->setFocus(); // Qt focus intentionally differs from native editing.
  qint64 appliedBudget = cache->chosen().diskMegabytes;
  int changes = 0;
  QObject::connect(cache, &CachePreferencesBox::changed, settings, [&] {
    appliedBudget = cache->chosen().diskMegabytes;
    ++changes;
  });
  settings->setClosedHandler([settings] {
    if (settings && settings->focusWidget()) { settings->focusWidget()->clearFocus(); }
  });
  showNativeWidget(settings);
  if (!replaceText(disk->objectName(), QStringLiteral("20"), error)) {
    const QString detail = error;
    return fail(detail);
  }
  if (disk->value() != 6 || changes != 0 || appliedBudget != 6 * 1024) {
    return fail(QStringLiteral("native cache budget applied before editing finished"));
  }
  NSView* field = findControl(disk->objectName());
  NSWindow* window = field.window;
  NSButton* close = [window standardWindowButton:NSWindowCloseButton];
  if (close == nil || !close.enabled) { return fail(QStringLiteral("cache close fixture has no red close button")); }
  [close performClick:nil];
  const bool committed = (!settings || !settings->isVisible()) && changes == 1 && appliedBudget == 20 * 1024;
  delete settings.data();
  QCoreApplication::processEvents(QEventLoop::AllEvents);
  if (!committed) { error = QStringLiteral("red close discarded the pending native cache budget"); return false; }

  // Closing a dialog is cancellation, even with an incomplete field that
  // would prevent an accepting action from ending native editing.
  QDialog cancelled;
  cancelled.setWindowTitle(QStringLiteral("Native cancel validation regression"));
  auto* layout = new QVBoxLayout(&cancelled);
  auto* value = new QLineEdit(QStringLiteral("3"));
  value->setObjectName(QStringLiteral("close.cancel.value"));
  value->setValidator(new QIntValidator(1, 9, value));
  layout->addWidget(value);
  cancelled.setResult(QDialog::Accepted);
  showNativeWidget(&cancelled);
  if (!replaceText(value->objectName(), QString(), error)) { cancelled.reject(); return false; }
  field = findControl(value->objectName());
  window = field.window;
  close = [window standardWindowButton:NSWindowCloseButton];
  [close performClick:nil];
  const bool rejected = !cancelled.isVisible() && cancelled.result() == QDialog::Rejected;
  cancelled.reject();
  QCoreApplication::processEvents(QEventLoop::AllEvents);
  if (!rejected) { error = QStringLiteral("red close failed to cancel a dialog with an incomplete editor"); return false; }
  qDebug() << "Native Settings red close: pending cache budget committed and dialog cancellation preserved";
  return true;
}

bool messageDetailsContract(QString& error) {
  QMessageBox presenter(QMessageBox::Warning, QStringLiteral("Native details regression"),
                       QStringLiteral("Synthetic diagnostic message"), QMessageBox::Ok);
  presenter.setDetailedText(QStringLiteral("Diagnostic R&D\n<plain details>"));
  presenter.setDefaultButton(QMessageBox::Ok);
  QTimer inspect;
  inspect.setInterval(10);
  QElapsedTimer elapsed;
  elapsed.start();
  bool checked = false;
  bool passed = false;
  QObject::connect(&inspect, &QTimer::timeout, &presenter, [&] {
    NSWindow* window = documentWindow().attachedSheet;
    NSButton* toggle = (NSButton*)identifiedView(window.contentView, QStringLiteral("native.alert.details.toggle"));
    NSView* details = identifiedView(window.contentView, QStringLiteral("native.alert.details.text"));
    NSButton* response = (NSButton*)identifiedView(window.contentView, QStringLiteral("native.alert.response.0"));
    if (toggle == nil || details == nil || response == nil) {
      if (elapsed.elapsed() < 5000) { return; }
      inspect.stop();
      error = QStringLiteral("native message box details fixture did not appear");
      if (window.sheetParent != nil) { [window.sheetParent endSheet:window returnCode:NSModalResponseCancel]; }
      else { [NSApp abortModal]; }
      return;
    }
    inspect.stop();
    checked = true;
    passed = details.hidden && presenter.clickedButton() == nullptr &&
             identifiedView(window.contentView, QStringLiteral("native.alert.response.1")) == nil;
    [toggle performClick:nil];
    passed = passed && !details.hidden && window.sheetParent != nil && presenter.clickedButton() == nullptr;
    NSTextView* diagnostic = (NSTextView*)((NSScrollView*)details).documentView;
    passed = passed && qt(diagnostic.string) == presenter.detailedText();
    [toggle performClick:nil];
    passed = passed && details.hidden && window.sheetParent != nil && presenter.clickedButton() == nullptr;
    if (!passed) { error = QStringLiteral("native details disclosure dismissed the alert or changed its response"); }
    [response performClick:nil];
  });
  inspect.start();
  const int result = execNativeDialog(&presenter);
  inspect.stop();
  if (!checked || !passed) { return false; }
  if (result != QMessageBox::Ok || presenter.standardButton(presenter.clickedButton()) != QMessageBox::Ok) {
    error = QStringLiteral("native alert response no longer matches its QMessageBox button");
    return false;
  }
  return true;
}

bool widgetRefreshContract(QString& error) {
  QWidget presenter;
  presenter.setAttribute(Qt::WA_DontShowOnScreen);
  auto* layout = new QVBoxLayout(&presenter);
  const auto named = [layout](QWidget* widget, const QString& name) {
    widget->setObjectName(name);
    layout->addWidget(widget);
  };
  auto* caption = new QLabel(QStringLiteral("<b>Initial label</b>"));
  auto* button = new QPushButton(QStringLiteral("&Initial button"));
  auto* check = new QCheckBox(QStringLiteral("Check"));
  auto* edit = new QLineEdit(QStringLiteral("Initial value"));
  auto* spin = new QSpinBox;
  auto* progress = new QProgressBar;
  auto* combo = new QComboBox;
  auto* table = new QTableView;
  auto* tree = new QTreeView;
  named(caption, QStringLiteral("refresh.caption"));
  named(button, QStringLiteral("refresh.button"));
  named(check, QStringLiteral("refresh.check"));
  named(edit, QStringLiteral("refresh.edit"));
  named(spin, QStringLiteral("refresh.spin"));
  named(progress, QStringLiteral("refresh.progress"));
  named(combo, QStringLiteral("refresh.combo"));
  named(table, QStringLiteral("refresh.table"));
  named(tree, QStringLiteral("refresh.tree"));
  const auto tool = [&named](const QString& name, Qt::ToolButtonStyle style) {
    auto* result = new QToolButton;
    result->setText(QStringLiteral("Refresh action"));
    result->setIcon(swatchIcon(Qt::red));
    result->setToolButtonStyle(style);
    named(result, name);
    return result;
  };
  const std::vector<std::pair<QToolButton*, NSCellImagePosition>> toolModes{
      {tool(QStringLiteral("refresh.tool-text"), Qt::ToolButtonTextOnly), NSNoImage},
      {tool(QStringLiteral("refresh.tool-beside"), Qt::ToolButtonTextBesideIcon), NSImageLeading},
      {tool(QStringLiteral("refresh.tool-icon"), Qt::ToolButtonIconOnly), NSImageOnly},
      {tool(QStringLiteral("refresh.tool-tile"), Qt::ToolButtonTextUnderIcon), NSImageAbove}};
  combo->addItems({QStringLiteral("First"), QStringLiteral("Second")});
  QStandardItemModel model(2, 1), replacement(1, 1);
  model.setData(model.index(0, 0), QStringLiteral("Row one"));
  model.setData(model.index(1, 0), QStringLiteral("Row two"));
  replacement.setData(replacement.index(0, 0), QStringLiteral("Replacement"));
  table->setModel(&model);
  tree->setModel(&model);
  auto view = createNativeWidgetView(&presenter);
  view->refresh();
  NSView* root = (__bridge NSView*)view->nativeView();
  const auto control = [&root](const QString& name) { return identifiedView(root, name); };
  const auto fail = [&error](const QString& message) { error = message; return false; };
  const auto unchanged = [&view, &fail]() {
    const quint64 before = nativeWidgetPropertyWrites();
    for (int i = 0; i < 3; ++i) { view->refresh(); }
    return nativeWidgetPropertyWrites() == before || fail(QStringLiteral("unchanged widget refresh assigned AppKit properties"));
  };
  if (!unchanged()) { return false; }
  const auto toolPresentation = [&toolModes, &control, &fail] {
    for (const auto& [source, position] : toolModes) {
      NSButton* native = (NSButton*)control(source->objectName());
      const QString caption = position == NSImageOnly ? QString() : source->text();
      if (native == nil || native.image == nil || native.imagePosition != position || qt(native.title) != caption ||
          qt(native.accessibilityLabel) != source->text()) {
        return fail(QStringLiteral("native tool button '%1' lost its caption/icon mode after an image update")
                        .arg(source->objectName()));
      }
    }
    return true;
  };
  if (!toolPresentation()) { return false; }
  // A new image with unchanged text/style must restore the final native
  // image position even when the source presentation cache already matches.
  for (const auto& [source, position] : toolModes) {
    (void)position;
    source->setIcon(swatchIcon(Qt::blue));
  }
  view->refresh();
  if (!toolPresentation() || !unchanged()) { return false; }

  // All of these signals/events must arrive before the slow fallback.
  caption->setText(QStringLiteral("<b>Changed label</b>"));
  button->setText(QStringLiteral("&Changed button"));
  button->setToolTip(QStringLiteral("Changed button tooltip"));
  edit->setText(QStringLiteral("Changed value"));
  edit->setToolTip(QStringLiteral("Changed tooltip"));
  edit->setEnabled(false);
  check->setChecked(true);
  spin->setValue(17);
  combo->setCurrentIndex(1);
  table->selectionModel()->select(model.index(1, 0), QItemSelectionModel::ClearAndSelect | QItemSelectionModel::Rows);
  QCoreApplication::processEvents(QEventLoop::AllEvents);
  root = (__bridge NSView*)view->nativeView();
  NSTextField* field = (NSTextField*)control(QStringLiteral("refresh.edit"));
  NSButton* nativeCheck = (NSButton*)control(QStringLiteral("refresh.check"));
  NSPopUpButton* popup = (NSPopUpButton*)control(QStringLiteral("refresh.combo"));
  NSTextField* nativeCaption = (NSTextField*)control(QStringLiteral("refresh.caption"));
  NSButton* nativeButton = (NSButton*)control(QStringLiteral("refresh.button"));
  if (qt(field.stringValue) != edit->text() || field.enabled || qt(field.toolTip) != edit->toolTip() ||
      nativeCheck.state != NSControlStateValueOn || popup.indexOfSelectedItem != 1 ||
      qt(nativeCaption.stringValue) != QStringLiteral("Changed label") ||
      qt(nativeButton.title) != QStringLiteral("Changed button") || qt(nativeButton.toolTip) != button->toolTip()) {
    return fail(QStringLiteral("presenter signals/events did not refresh native controls"));
  }
  const auto tableIn = [](NSView* parent, const auto& self) -> NSTableView* {
    if ([parent isKindOfClass:NSTableView.class]) { return (NSTableView*)parent; }
    for (NSView* child in parent.subviews) { if (NSTableView* found = self(child, self)) { return found; } }
    return nil;
  };
  NSTableView* nativeTable = tableIn(control(QStringLiteral("refresh.table")), tableIn);
  if (nativeTable.numberOfRows != 2 || nativeTable.selectedRow != 1) {
    return fail(QStringLiteral("model selection signal did not refresh native rows"));
  }
  model.setData(model.index(0, 0), QStringLiteral("Changed row"));
  model.insertRow(2);
  QCoreApplication::processEvents(QEventLoop::AllEvents);
  if (nativeTable.numberOfRows != 3) { return fail(QStringLiteral("model insertion did not refresh native rows")); }
  if (!unchanged()) { return false; }
  table->setRowHidden(0, true);
  view->refresh();
  if (nativeTable.numberOfRows != 2) { return fail(QStringLiteral("silent row hiding was not reflected by an explicit refresh")); }
  table->setRowHidden(0, false);
  view->refresh();

  auto* selection = new QItemSelectionModel(&model, table);
  table->setSelectionModel(selection);
  view->refresh();
  selection->select(model.index(0, 0), QItemSelectionModel::ClearAndSelect | QItemSelectionModel::Rows);
  QCoreApplication::processEvents(QEventLoop::AllEvents);
  if (nativeTable.selectedRow != 0) { return fail(QStringLiteral("replacement selection model was not observed")); }

  // A model and selection-model swap must rebind safely. Updates from the
  // previous model cannot keep dirtying the new table or stale binding IDs.
  table->setModel(&replacement);
  view->refresh();
  replacement.insertRow(1);
  QCoreApplication::processEvents(QEventLoop::AllEvents);
  root = (__bridge NSView*)view->nativeView();
  nativeTable = tableIn(control(QStringLiteral("refresh.table")), tableIn);
  if (nativeTable.numberOfRows != 2) { return fail(QStringLiteral("replacement model signals were not rebound")); }
  auto* embedded = new QLineEdit(QStringLiteral("Cell value"));
  embedded->setObjectName(QStringLiteral("refresh.cell"));
  table->setIndexWidget(replacement.index(0, 0), embedded);
  view->refresh();
  root = (__bridge NSView*)view->nativeView();
  nativeTable = tableIn(control(QStringLiteral("refresh.table")), tableIn);
  [nativeTable reloadData];
  NSView* cell = [nativeTable viewAtColumn:0 row:0 makeIfNecessary:YES];
  if (identifiedView(cell, QStringLiteral("refresh.cell")) == nil) {
    return fail(QStringLiteral("index widget was lost after a presenter-tree change"));
  }
  if (!unchanged()) { return false; }
  delete embedded;
  QCoreApplication::processEvents(QEventLoop::AllEvents);
  if (!unchanged()) { return false; }
  qDebug() << "Native widget refresh: unchanged properties, event updates, model replacement and index widgets verified";
  return true;
}

bool duplicateChoices(CommandHost& host, QString& error) {
  if (!mac::nativeWindowRefreshContract((__bridge void*)documentWindow(), error) ||
      !stepperLimitsContract(error) || !labelTextContract(error) || !settingsCloseContract(error) ||
      !messageDetailsContract(error) ||
      !widgetRefreshContract(error)) { return false; }
  // Exercise the real presenters with synthetic feature UIDs, independently
  // of the geometry needed to produce two identically named features.
  const auto popupIn = [](NSView* root, const auto& self) -> NSPopUpButton* {
    if ([root isKindOfClass:NSPopUpButton.class]) return (NSPopUpButton*)root;
    for (NSView* child in root.subviews) {
      if (NSPopUpButton* popup = self(child, self)) return popup;
    }
    return nil;
  };
  const auto check = [&error](bool passed, const QString& message) {
    if (!passed) error = message;
    return passed;
  };
  const QString first = QStringLiteral("F101");
  const QString second = QStringLiteral("F202");
  CommandDef def;
  def.name = QStringLiteral("Duplicate choices regression");
  def.inputs = {choiceInput(QStringLiteral("feature"), QStringLiteral("Feature"),
                           {{first, QStringLiteral("Same name")}, {second, QStringLiteral("Same name")}}, first)};
  CommandSession session(def, host);
  auto form = createNativeCommandForm(session);
  NSPopUpButton* formPopup = popupIn((__bridge NSView*)form->nativeView(), popupIn);
  if (!check(formPopup != nil && formPopup.numberOfItems == 2 &&
             [[formPopup itemAtIndex:0].title isEqualToString:[formPopup itemAtIndex:1].title],
             QStringLiteral("command form collapsed same-titled choices"))) return false;
  NSMenuItem* firstItem = [formPopup itemAtIndex:0];
  NSMenuItem* secondItem = [formPopup itemAtIndex:1];
  for (NSInteger index : {1, 0}) {
    [formPopup selectItemAtIndex:index];
    [formPopup sendAction:formPopup.action to:formPopup.target];
    const QString expected = index == 0 ? first : second;
    if (!check(session.state().choice(QStringLiteral("feature")) == expected,
               QStringLiteral("command form submitted the wrong feature UID"))) return false;
    for (int refresh = 0; refresh < 3; ++refresh) form->refresh(session.state());
    if (!check([formPopup itemAtIndex:0] == firstItem && [formPopup itemAtIndex:1] == secondItem &&
               qt((NSString*)formPopup.selectedItem.representedObject) == expected,
               QStringLiteral("command form rebuilt unchanged choices or lost the selected UID"))) return false;
  }

  QDialog dialog;
  auto* layout = new QVBoxLayout(&dialog);
  auto* combo = new QComboBox(&dialog);
  layout->addWidget(combo);
  combo->addItem(QStringLiteral("Same name"), first);
  combo->addItem(QStringLiteral("Same name"), second);
  combo->setCurrentIndex(1);
  auto dialogView = createNativeWidgetView(&dialog);
  dialogView->refresh();
  NSPopUpButton* dialogPopup = popupIn((__bridge NSView*)dialogView->nativeView(), popupIn);
  if (!check(dialogPopup != nil && dialogPopup.numberOfItems == 2 && dialogPopup.indexOfSelectedItem == 1 &&
             [[dialogPopup itemAtIndex:0].title isEqualToString:[dialogPopup itemAtIndex:1].title],
             QStringLiteral("dialog binding collapsed same-titled choices or lost its initial selection"))) return false;
  NSMenuItem* dialogFirst = [dialogPopup itemAtIndex:0];
  NSMenuItem* dialogSecond = [dialogPopup itemAtIndex:1];
  int activated = -1;
  QObject::connect(combo, &QComboBox::activated, &dialog, [&activated](int index) { activated = index; });
  for (NSInteger index : {0, 1}) {
    [dialogPopup selectItemAtIndex:index];
    [dialogPopup sendAction:dialogPopup.action to:dialogPopup.target];
    const QString expected = index == 0 ? first : second;
    if (!check(combo->currentData().toString() == expected && activated == index,
               QStringLiteral("dialog popup submitted the wrong feature UID or activated row"))) return false;
    for (int refresh = 0; refresh < 3; ++refresh) dialogView->refresh();
    if (!check([dialogPopup itemAtIndex:0] == dialogFirst && [dialogPopup itemAtIndex:1] == dialogSecond &&
               dialogPopup.indexOfSelectedItem == index,
               QStringLiteral("dialog binding rebuilt unchanged choices or lost its selection"))) return false;
  }
  // A model insertion shifts the persistent identities. Subsequent input
  // must still activate the row carrying the selected feature's data.
  combo->insertItem(0, QStringLiteral("Same name"), QStringLiteral("F303"));
  [dialogPopup selectItemAtIndex:0];
  [dialogPopup sendAction:dialogPopup.action to:dialogPopup.target];
  if (!check(combo->currentData().toString() == first && activated == 1,
             QStringLiteral("dialog popup used a stale menu row after a model insertion"))) return false;
  combo->setCurrentIndex(2);
  dialogView->refresh();
  if (!check(dialogPopup.numberOfItems == 3 && dialogPopup.indexOfSelectedItem == 2,
             QStringLiteral("dialog popup did not follow an inserted same-titled row"))) return false;
  [dialogPopup selectItemAtIndex:1];
  [dialogPopup sendAction:dialogPopup.action to:dialogPopup.target];
  if (!check(combo->currentData().toString() == first && activated == 1,
             QStringLiteral("dialog popup submitted the wrong UID after a model insertion"))) return false;
  combo->setCurrentIndex(-1);
  dialogView->refresh();
  if (!check(dialogPopup.selectedItem == nil,
             QStringLiteral("dialog popup retained a selection for an unset model index"))) return false;
  qDebug() << "Native duplicate choices: both presenters preserved UIDs, duplicate labels and unchanged menu items";
  return true;
}

bool triggerAction(const QString& identifier, QString& error) {
  NSMenuItem* item = findAction(NSApp.mainMenu, identifier);
  if (!item) {
    findView(documentWindow().contentView, [&](NSView* view) {
      if ([view isKindOfClass:NSPopUpButton.class]) item = findAction(((NSPopUpButton*)view).menu, identifier);
      return item != nil;
    });
  }
  if (!item) { error = QStringLiteral("no native menu action '%1'").arg(identifier); return false; }
  if ([item.target respondsToSelector:@selector(validateMenuItem:)] &&
      ![(id<NSMenuItemValidation>)item.target validateMenuItem:item]) {
    error = QStringLiteral("native menu action '%1' is disabled").arg(identifier); return false;
  }
  if (![NSApp sendAction:item.action to:item.target from:item]) {
    error = QStringLiteral("native menu action '%1' could not dispatch").arg(identifier); return false;
  }
  return true;
}

bool triggerMenu(const QString& path, QString& error) {
  const auto normalized = [](QString text) {
    text.remove(QLatin1Char('&'));
    while (text.endsWith(QLatin1Char('.')) || text.endsWith(QChar(0x2026))) text.chop(1);
    return text.trimmed();
  };
  NSMenu* menu = NSApp.mainMenu;
  const QStringList parts = path.split(QLatin1Char('/'), Qt::SkipEmptyParts);
  NSMenuItem* found = nil;
  for (int index = 0; index < parts.size(); ++index) {
    found = nil;
    [menu update];
    for (NSMenuItem* item in menu.itemArray) {
      if (normalized(qt(item.title)) == normalized(parts[index])) { found = item; break; }
    }
    if (!found || (index + 1 < parts.size() && !found.submenu)) {
      error = QStringLiteral("no native menu entry '%1'").arg(path); return false;
    }
    if (index + 1 < parts.size()) menu = found.submenu;
  }
  if (!found || !found.enabled || !found.action || ![NSApp sendAction:found.action to:found.target from:found]) {
    error = QStringLiteral("native menu entry '%1' is disabled or could not dispatch").arg(path); return false;
  }
  return true;
}

bool activate(const QString& identifier, QString& error) {
  NSTableView* table = nil;
  NSInteger row = -1;
  NSView* view = findControl(identifier, &table, &row);
  if (!view) { error = QStringLiteral("no native item '%1'").arg(identifier); return false; }
  if ([view respondsToSelector:@selector(activateHistoryFeature)]) {
    [view scrollRectToVisible:view.bounds];
    [view.window makeKeyAndOrderFront:nil];
    const NSPoint point = [view convertPoint:NSMakePoint(NSMidX(view.bounds), NSMidY(view.bounds)) toView:nil];
    for (NSEventType type : {NSEventTypeLeftMouseDown, NSEventTypeLeftMouseUp}) {
      NSEvent* event = [NSEvent mouseEventWithType:type location:point modifierFlags:0
          timestamp:NSProcessInfo.processInfo.systemUptime windowNumber:view.window.windowNumber
          context:nil eventNumber:0 clickCount:2 pressure:(type == NSEventTypeLeftMouseDown ? 1 : 0)];
      if (type == NSEventTypeLeftMouseDown) [view mouseDown:event];
      else [view mouseUp:event];
    }
    return true;
  }
  for (NSView* parent = view; !table && parent; parent = parent.superview) {
    if ([parent isKindOfClass:NSTableView.class]) { table = (NSTableView*)parent; break; }
  }
  if (row < 0 && table) row = [table rowForView:view];
  if (!table || row < 0 || !table.doubleAction) {
    error = QStringLiteral("native item '%1' is not an activatable table row").arg(identifier); return false;
  }
  [table selectRowIndexes:[NSIndexSet indexSetWithIndex:NSUInteger(row)] byExtendingSelection:NO];
  [table scrollRowToVisible:row];
  return [NSApp sendAction:table.doubleAction to:table.target from:table];
}
bool trackContextMenu(const QString& choice, const std::function<void()>& open, QString& error,
                      const std::function<void()>& onClosed, const QString& screenshotPath) {
  __block NSMenu* tracked = nil;
  __block NSTimer* response = nil;
  __block bool began = false;
  __block bool ended = false;
  __block bool selected = false;
  __block bool captured = screenshotPath.isEmpty();
  __block QStringList available;
  const bool cancel = choice == QStringLiteral("cancel");
  NSString* title = choice.toNSString();
  // "Camera/Perspective" chooses a leaf of a submenu; a top-level title that
  // itself contains the slash still matches first.
  QStringList path;
  for (QString part : choice.split(QLatin1Char('/'), Qt::SkipEmptyParts)) {
    path << part.remove(QLatin1Char('&')).trimmed();
  }
  NSNotificationCenter* center = NSNotificationCenter.defaultCenter;
  id beginObserver = [center addObserverForName:NSMenuDidBeginTrackingNotification object:nil queue:nil
      usingBlock:^(NSNotification* notice) {
    if (began || ![notice.object isKindOfClass:NSMenu.class]) return;
    began = true;
    tracked = notice.object;
    // The popup owns a nested AppKit tracking loop. Respond in that loop,
    // after the real menu is displayed, rather than dispatching a QAction
    // directly or allowing the script's next step to race its closure.
    response = [NSTimer timerWithTimeInterval:0.05 repeats:NO block:^(NSTimer* timer) {
      (void)timer;
      if (!screenshotPath.isEmpty()) captured = captureOwnWindows(screenshotPath);
      if (!cancel) {
        const auto label = [](NSString* text) { return qt(text).remove(QLatin1Char('&')).trimmed(); };
        // Performs the enabled leaf titled `wanted` in `menu`, listing what
        // it saw for the error message.
        const auto leaf = [&label](NSMenu* menu, const QString& wanted, QStringList& seen) {
          [menu update];
          for (NSInteger index = 0; index < menu.numberOfItems; ++index) {
            NSMenuItem* item = [menu itemAtIndex:index];
            seen.append(QStringLiteral("%1 (enabled=%2, action=%3, target=%4, submenu=%5)")
                .arg(qt(item.title)).arg(bool(item.enabled)).arg(qt(NSStringFromSelector(item.action)))
                .arg(qt(NSStringFromClass([item.target class]))).arg(bool(item.submenu)));
            if (label(item.title) == wanted && item.enabled && item.action && !item.submenu) {
              [menu performActionForItemAtIndex:index];
              return true;
            }
          }
          return false;
        };
        selected = leaf(tracked, label(title), available);
        if (!selected && path.size() > 1) {
          NSMenu* menu = tracked;
          for (int depth = 0; menu != nil && depth + 1 < path.size(); ++depth) {
            NSMenu* next = nil;
            for (NSMenuItem* item in menu.itemArray) {
              if (item.submenu != nil && label(item.title) == path.at(depth)) {
                next = item.submenu;
                break;
              }
            }
            menu = next;
          }
          if (menu != nil) {
            selected = leaf(menu, path.last(), available);
          }
        }
      }
      [tracked cancelTracking];
    }];
    [NSRunLoop.currentRunLoop addTimer:response forMode:NSEventTrackingRunLoopMode];
    [NSRunLoop.currentRunLoop addTimer:response forMode:NSRunLoopCommonModes];
  }];
  id endObserver = [center addObserverForName:NSMenuDidEndTrackingNotification object:nil queue:nil
      usingBlock:^(NSNotification* notice) {
    if (notice.object == tracked && !ended) {
      ended = true;
      // A popup's shared action can synchronously enter another modal
      // loop. Its script must resume after tracking, before open() returns.
      if (onClosed) onClosed();
    }
  }];
  open();
  // Browser and viewport owners intentionally queue their context-menu
  // slots. Let that real delivery open the popup before removing observers.
  QElapsedTimer deadline;
  deadline.start();
  while (!ended && deadline.elapsed() < 6000) {
    QCoreApplication::processEvents(QEventLoop::AllEvents, 25);
    [NSRunLoop.currentRunLoop runMode:NSDefaultRunLoopMode
                          beforeDate:[NSDate dateWithTimeIntervalSinceNow:0.01]];
  }
  [response invalidate];
  [center removeObserver:beginObserver];
  [center removeObserver:endObserver];
  if (!began || !ended) {
    error = QStringLiteral("the real native context menu did not begin and finish tracking"); return false;
  }
  if (!captured) {
    error = QStringLiteral("could not capture the tracking native menu at '%1'").arg(screenshotPath); return false;
  }
  if (!cancel && !selected) {
    error = QStringLiteral("native context menu has no enabled leaf action '%1': %2")
        .arg(choice, available.join(QStringLiteral(" | "))); return false;
  }
  qInfo().noquote() << QStringLiteral("Native test: context menu tracked and %1")
      .arg(cancel ? QStringLiteral("cancelled") : QStringLiteral("selected '%1'").arg(choice));
  return true;
}

bool contextMenu(const QString& identifier, const QString& choice, QString& error) {
  NSTableView* table = nil;
  NSInteger row = -1;
  NSView* view = findControl(identifier, &table, &row);
  if (!view || !viewVisible(view)) {
    error = QStringLiteral("no visible native context menu target '%1'").arg(identifier); return false;
  }
  NSView* receiver = table != nil ? table : view;
  [view scrollRectToVisible:view.bounds];
  [receiver.window makeKeyAndOrderFront:nil];
  const NSPoint point = [view convertPoint:NSMakePoint(NSMidX(view.bounds), NSMidY(view.bounds)) toView:nil];
  NSEvent* event = [NSEvent mouseEventWithType:NSEventTypeRightMouseDown location:point modifierFlags:0
      timestamp:NSProcessInfo.processInfo.systemUptime windowNumber:receiver.window.windowNumber
      context:nil eventNumber:0 clickCount:1 pressure:1];
  return trackContextMenu(choice, [receiver, event] {
    NSMenu* menu = [receiver menuForEvent:event];
    if (menu) [NSMenu popUpContextMenu:menu withEvent:event forView:receiver];
  }, error);
}

bool select(const QString& identifier, QString& error, Qt::KeyboardModifiers modifiers) {
  NSTableView* table = nil;
  NSInteger row = -1;
  NSView* view = findControl(identifier, &table, &row);
  if ([view respondsToSelector:@selector(selectHistoryFeature)]) {
    [view scrollRectToVisible:view.bounds];
    [view.window makeKeyAndOrderFront:nil];
    [view.window makeFirstResponder:view];
    if (!modifiers) {
      [(id<MITCADNativeHistoryFeatureTest>)view selectHistoryFeature];
    } else {
      const NSPoint point = [view convertPoint:NSMakePoint(NSMidX(view.bounds), NSMidY(view.bounds)) toView:nil];
      NSEventModifierFlags flags = 0;
      if (modifiers & Qt::ShiftModifier) flags |= NSEventModifierFlagShift;
      if (modifiers & Qt::ControlModifier) flags |= NSEventModifierFlagCommand;
      for (NSEventType type : {NSEventTypeLeftMouseDown, NSEventTypeLeftMouseUp}) {
        NSEvent* event = [NSEvent mouseEventWithType:type location:point modifierFlags:flags
            timestamp:NSProcessInfo.processInfo.systemUptime windowNumber:view.window.windowNumber
            context:nil eventNumber:0 clickCount:1 pressure:(type == NSEventTypeLeftMouseDown ? 1 : 0)];
        if (type == NSEventTypeLeftMouseDown) [view mouseDown:event];
        else [view mouseUp:event];
      }
    }
    return true;
  }
  for (NSView* parent = view; !table && parent; parent = parent.superview) {
    if ([parent isKindOfClass:NSTableView.class]) { table = (NSTableView*)parent; break; }
  }
  if (row < 0 && table) row = [table rowForView:view];
  if (!table || row < 0) { error = QStringLiteral("native item '%1' is not a visible table row").arg(identifier); return false; }
  bool selectable = true;
  if ([table isKindOfClass:NSOutlineView.class]) {
    NSOutlineView* outline = (NSOutlineView*)table;
    if ([outline.delegate respondsToSelector:@selector(outlineView:shouldSelectItem:)])
      selectable = [outline.delegate outlineView:outline shouldSelectItem:[outline itemAtRow:row]];
  } else if ([table.delegate respondsToSelector:@selector(tableView:shouldSelectRow:)]) {
    selectable = [table.delegate tableView:table shouldSelectRow:row];
  }
  if (!selectable) {
    error = QStringLiteral("native item '%1' is not selectable").arg(identifier); return false;
  }
  [table.window makeKeyAndOrderFront:nil];
  [table.window makeFirstResponder:table];
  // The semantic selection helper already honours these modifiers for
  // history items. Keep Command/Control-toggle selection for table rows too;
  // otherwise a requested multi-body assignment silently becomes one body.
  const bool toggle = bool(modifiers & Qt::ControlModifier);
  if (toggle && [table.selectedRowIndexes containsIndex:NSUInteger(row)]) [table deselectRow:row];
  else [table selectRowIndexes:[NSIndexSet indexSetWithIndex:NSUInteger(row)] byExtendingSelection:toggle];
  [table scrollRowToVisible:row];
  return true;
}
bool expand(const QString& identifier, QString& error) {
  NSView* view = findControl(identifier);
  if ([view isKindOfClass:NSScrollView.class]) view = ((NSScrollView*)view).documentView;
  if (![view isKindOfClass:NSOutlineView.class]) {
    error = QStringLiteral("native view '%1' is not an outline").arg(identifier); return false;
  }
  [(NSOutlineView*)view expandItem:nil expandChildren:YES];
  return true;
}

bool buttonExists(const QRegularExpression& expression) { return active() && findButton(expression) != nil; }
bool clickButton(const QRegularExpression& expression, QString& error) {
  NSButton* button = findButton(expression);
  if (!button) { error = QStringLiteral("no native button matching '%1'").arg(expression.pattern()); return false; }
  [button scrollRectToVisible:button.bounds];
  [button performClick:nil];
  return true;
}

bool dialogExists(const QRegularExpression& expression) {
  if (!active()) return false;
  for (NSWindow* window in nativeWindows()) if (windowMatches(window, expression)) return true;
  return false;
}
bool focusDialog(const QRegularExpression& expression) {
  for (NSWindow* window in nativeWindows()) {
    if (windowMatches(window, expression)) {
      focusedWindow = window;
      [NSApp activate];
      [window makeKeyAndOrderFront:nil];
      [window makeKeyWindow];
      return true;
    }
  }
  return false;
}
bool replaceFileName(const QString& name, QString& error) {
  NSSavePanel* panel = filePanel();
  if (!panel || [panel isKindOfClass:NSOpenPanel.class]) {
    error = QStringLiteral("no focused native save panel"); return false;
  }
  // Public panel state identifies its real filename input; no private
  // AppKit class names, selectors or view identifiers are assumed.
  NSTextField* field = (NSTextField*)findView(panel.contentView, [&](NSView* view) {
    return [view isKindOfClass:NSTextField.class] && ![view isKindOfClass:NSSearchField.class] &&
           ((NSTextField*)view).editable && [((NSTextField*)view).stringValue isEqualToString:panel.nameFieldStringValue];
  });
  if (!field) {
    // Sandboxed/remote AppKit choosers keep this field in NSRemoteView's
    // service. The public panel property still edits that native chooser;
    // its normal ok: validation supplies the actual URL and result.
    panel.nameFieldStringValue = name.toNSString();
    if (qt(panel.nameFieldStringValue) == name) return true;
    error = QStringLiteral("native save panel rejected its filename"); return false;
  }
  [panel makeKeyAndOrderFront:nil];
  [panel makeFirstResponder:field];
  [field selectText:nil];
  NSTextView* editor = (NSTextView*)field.currentEditor;
  if (!editor) { error = QStringLiteral("native save filename has no field editor"); return false; }
  [editor insertText:name.toNSString() replacementRange:NSMakeRange(0, editor.string.length)];
  return true;
}
bool setFileDirectory(const QString& directory, QString& error) {
  NSSavePanel* panel = filePanel();
  BOOL isDirectory = NO;
  NSString* path = directory.toNSString();
  if (!panel) { error = QStringLiteral("no focused native file panel"); return false; }
  if (!path.absolutePath || ![NSFileManager.defaultManager fileExistsAtPath:path isDirectory:&isDirectory] || !isDirectory) {
    error = QStringLiteral("native file directory '%1' does not exist").arg(directory); return false;
  }
  panel.directoryURL = [NSURL fileURLWithPath:path isDirectory:YES];
  return true;
}
QString filePanelDiagnostic() {
  NSSavePanel* panel = (__bridge NSSavePanel*)nativeFileDialogPanel();
  if (panel == nil) return QStringLiteral("panel=nil; %1").arg(keyEventFocusDiagnostic());
  QStringList urls;
  if ([panel isKindOfClass:NSOpenPanel.class]) {
    for (NSURL* url in ((NSOpenPanel*)panel).URLs) urls << qt(url.path);
  } else if (panel.URL != nil) {
    urls << qt(panel.URL.path);
  }
  QStringList fields;
  const auto collect = [&fields](NSView* view, const auto& self) -> void {
    if (view == nil || view.hidden) return;
    if ([view isKindOfClass:NSTextField.class] && ((NSTextField*)view).editable) {
      fields << QStringLiteral("%1=%2").arg(qt(NSStringFromClass(view.class)), qt(((NSTextField*)view).stringValue));
    }
    for (NSView* child in view.subviews) self(child, self);
  };
  collect(panel.contentView, collect);
  collect(panel.attachedSheet.contentView, collect);
  NSButtonCell* defaultButton = panel.defaultButtonCell;
  const QString defaultState = defaultButton == nil ? QStringLiteral("remote/unavailable")
      : QStringLiteral("title=%1 enabled=%2 view=%3")
          .arg(qt(defaultButton.title)).arg(bool(defaultButton.enabled))
          .arg(qt(NSStringFromClass(defaultButton.controlView.class)));
  return QStringLiteral("panel=%1 title=%2 directory=%3 names=%4 selected=[%5] editable=[%6] default=[%7] prompt=%8; %9")
      .arg(qt(NSStringFromClass(panel.class)), qt(panel.title), qt(panel.directoryURL.path),
           qt(panel.nameFieldStringValue), urls.join(QLatin1Char('|')), fields.join(QLatin1Char('|')),
           defaultState, qt(panel.prompt), keyEventFocusDiagnostic());
}
bool cancelFilePanel(QString& error) {
  NSSavePanel* panel = filePanel();
  if (!panel) { error = QStringLiteral("no focused native file panel"); return false; }
  [panel cancel:nil];
  return true;
}
void focusMain() {
  focusedWindow = nil;
  NSWindow* window = documentWindow();
  [window makeKeyAndOrderFront:nil];
  NSView* canvas = findView(window.contentView, [](NSView* view) {
    return isQtResponder(view) && view.acceptsFirstResponder;
  });
  if (canvas) [window makeFirstResponder:canvas];
}

void requestKeyEventFocus() {
  keyEventWindow = nil;
  if (!active()) {
    qInfo().noquote() << QStringLiteral("Native application key focus requested: %1").arg(keyEventFocusDiagnostic());
    return;
  }
  // Whole-event checks require actual AppKit activation, not just the Qt
  // state exercised by applicationDelegate(). Only activate our test app;
  // do not alter other applications or synthesize activation notifications.
  keyEventWindow = targetWindow();
  [NSApp activateIgnoringOtherApps:YES];
  [keyEventWindow makeKeyAndOrderFront:nil];
  qInfo().noquote() << QStringLiteral("Native application key focus requested: %1").arg(keyEventFocusDiagnostic());
}

bool keyEventFocusReady() {
  return active() && keyEventWindow != nil && NSApp.active &&
         NSApp.keyWindow == keyEventWindow && targetWindow() == keyEventWindow;
}

QString keyEventFocusDiagnostic() {
  const auto windowDetails = [](NSWindow* window) {
    if (!window) return QStringLiteral("nil");
    return QStringLiteral("%1#%2[id=%3 visible=%4 keyCapable=%5 minimized=%6 sheetParent=%7 responder=%8]")
        .arg(qt(NSStringFromClass(window.class))).arg(qlonglong(window.windowNumber)).arg(qt(window.identifier))
        .arg(bool(window.visible)).arg(bool(window.canBecomeKeyWindow)).arg(bool(window.miniaturized))
        .arg(window.sheetParent ? qlonglong(window.sheetParent.windowNumber) : -1)
        .arg(qt(NSStringFromClass(window.firstResponder.class)));
  };
  NSRunningApplication* front = NSWorkspace.sharedWorkspace.frontmostApplication;
  NSWindow* document = documentWindow();
  return QStringLiteral("native=%1 appActive=%2 qtState=%3 policy=%4 selfPid=%5 frontPid=%6 frontBundle=%7 "
                        "requested=%8 key=%9 current=%10 document=%11 modal=%12 attachedSheet=%13")
      .arg(active()).arg(bool(NSApp.active)).arg(int(qApp->applicationState())).arg(int(NSApp.activationPolicy))
      .arg(qlonglong(getpid())).arg(front ? qlonglong(front.processIdentifier) : -1).arg(qt(front.bundleIdentifier))
      .arg(windowDetails(keyEventWindow)).arg(windowDetails(NSApp.keyWindow)).arg(windowDetails(targetWindow()))
      .arg(windowDetails(document)).arg(windowDetails(NSApp.modalWindow)).arg(windowDetails(document.attachedSheet));
}

namespace {
bool sendNativeKey(int key, Qt::KeyboardModifiers modifiers, const QString& text, bool applicationEvent) {
  if (applicationEvent ? !active() : !nativeFocused()) return false;
  NSWindow* window = applicationEvent ? keyEventWindow : targetWindow();
  // Menu equivalents route to NSApp.keyWindow. Synchronize that public
  // AppKit focus with the script's native target after a child sheet closes.
  if (applicationEvent) {
    if (!keyEventFocusReady()) return false;
  } else {
    [NSApp activate];
    [window makeKeyWindow];
  }
  if (applicationEvent) {
    qInfo().noquote() << QStringLiteral("Native application key event: key=%1 target=%2 keyWindow=%3 active=%4 responder=%5")
        .arg(key).arg(qlonglong(window.windowNumber)).arg(qlonglong(NSApp.keyWindow.windowNumber))
        .arg(bool(NSApp.active)).arg(qt(NSStringFromClass(window.firstResponder.class)));
  }
  NSEventModifierFlags flags = 0;
  if (modifiers.testFlag(Qt::ControlModifier)) flags |= NSEventModifierFlagCommand;
  if (modifiers.testFlag(Qt::MetaModifier)) flags |= NSEventModifierFlagControl;
  if (modifiers.testFlag(Qt::AltModifier)) flags |= NSEventModifierFlagOption;
  if (modifiers.testFlag(Qt::ShiftModifier) || key == Qt::Key_Backtab) flags |= NSEventModifierFlagShift;
  // AppKit ignores Command/Control/Option here, but preserves Shift. A
  // lowercase Command-Shift-Z event can match Undo instead of Redo.
  const bool tab = key == Qt::Key_Tab || key == Qt::Key_Backtab;
  const bool backtab = tab && (flags & NSEventModifierFlagShift) != 0;
  NSString* ignoringModifiers = characters(backtab ? Qt::Key_Backtab : key, QString());
  if ((flags & NSEventModifierFlagShift) && key >= Qt::Key_A && key <= Qt::Key_Z) {
    ignoringModifiers = ignoringModifiers.uppercaseString;
  }
  // The portable parser supplies '\t' for Shift+Tab. AppKit's standard
  // text bindings require NSBackTabCharacter (0x19) for insertBacktab:,
  // rather than NSTabCharacter (0x09), in both character fields.
  NSString* string = (backtab || text.isEmpty()) ? ignoringModifiers : characters(key, text);
  if (tab)
    qInfo().noquote() << QStringLiteral("Native Tab input: backward=%1 characters=%2 ignoring=%3 physical=%4")
        .arg(backtab).arg(string.length ? int([string characterAtIndex:0]) : -1)
        .arg(ignoringModifiers.length ? int([ignoringModifiers characterAtIndex:0]) : -1).arg(virtualKey(key));
  NSEvent* down = [NSEvent keyEventWithType:NSEventTypeKeyDown location:NSZeroPoint modifierFlags:flags
        timestamp:NSProcessInfo.processInfo.systemUptime windowNumber:window.windowNumber context:nil
        characters:string charactersIgnoringModifiers:ignoringModifiers isARepeat:NO keyCode:virtualKey(key)];
  // The full-event regression leaves all equivalent/responder routing to
  // NSApplication. Ordinary script keys offer the target its equivalents.
  // A secondary window owns Command-W even when an asynchronous sheet close
  // has temporarily restored NSApp.keyWindow to the document.
  if (!applicationEvent && [window performKeyEquivalent:down]) return true;
  // The shell's validation keeps plain CAD keys out of text field editors.
  if (!applicationEvent && [NSApp.mainMenu performKeyEquivalent:down]) return true;
  // Let the real event loop dequeue these keys and establish currentEvent.
  // Native field delegates and Qt's key mapper read its modifier flags.
  [NSApp postEvent:down atStart:NO];
  NSEvent* up = [NSEvent keyEventWithType:NSEventTypeKeyUp location:NSZeroPoint modifierFlags:flags
        timestamp:NSProcessInfo.processInfo.systemUptime windowNumber:window.windowNumber context:nil
        characters:string charactersIgnoringModifiers:ignoringModifiers isARepeat:NO keyCode:virtualKey(key)];
  [NSApp postEvent:up atStart:NO];
  return true;
}
} // namespace

bool sendKey(int key, Qt::KeyboardModifiers modifiers, const QString& text) {
  return sendNativeKey(key, modifiers, text, false);
}

bool sendKeyEvent(int key, Qt::KeyboardModifiers modifiers, const QString& text) {
  return sendNativeKey(key, modifiers, text, true);
}

namespace {
bool systemFileTarget(NSWindow* expected, QString& error) {
  NSSavePanel* panel = (__bridge NSSavePanel*)nativeFileDialogPanel();
  const auto ownActivePanel = [panel, expected] {
    NSWindow* target = panel;
    while (target.attachedSheet != nil) target = target.attachedSheet;
    NSRunningApplication* front = NSWorkspace.sharedWorkspace.frontmostApplication;
    return !qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT") && active() && panel != nil &&
        panel == (__bridge NSSavePanel*)nativeFileDialogPanel() && panel.visible && target == expected &&
        NSApp.active && front != nil && front.processIdentifier == getpid();
  };
  // Closing the XPC Go to Folder sheet can leave AppKit's key window nil.
  // Recover only this still-visible panel directly attached to our visible
  // document, while our app remains active and frontmost. Ordering it also
  // requests WindowServer focus for the XPC view; makeKeyWindow alone only
  // restored AppKit's local key-window state after Go to Folder closed.
  // A different key window, modal window or sheet must never be displaced.
  NSWindow* document = documentWindow();
  if (ownActivePanel() && NSApp.keyWindow == nil && expected == panel && panel.attachedSheet == nil &&
      document.visible && panel.sheetParent == document && document.attachedSheet == panel &&
      (NSApp.modalWindow == nil || NSApp.modalWindow == panel) && panel.canBecomeKeyWindow) {
    NSResponder* remoteResponder = panel.firstResponder;
    [panel makeKeyAndOrderFront:nil];
    // Let the public ordering request and remote view notifications run
    // before posting WindowServer input. The driver pauses only its step
    // timer around this helper, so the next input cannot reenter this turn.
    QElapsedTimer turn;
    turn.start();
    while (turn.elapsed() < 100 && ownActivePanel() && NSApp.keyWindow == expected &&
           expected.firstResponder == remoteResponder) {
      [NSRunLoop.currentRunLoop runMode:NSDefaultRunLoopMode
                            beforeDate:[NSDate dateWithTimeIntervalSinceNow:0.01]];
    }
    const bool retained = expected.firstResponder == remoteResponder;
    qInfo().noquote() << QStringLiteral("Native system file input: own attached panel ordered front, key focus restored=%1 remote responder retained=%2")
                            .arg(NSApp.keyWindow == panel).arg(retained);
    if (!retained || !document.visible || panel.sheetParent != document || document.attachedSheet != panel ||
        (NSApp.modalWindow != nil && NSApp.modalWindow != panel)) {
      error = QStringLiteral("native file panel changed while its WindowServer focus was restored: %1")
          .arg(filePanelDiagnostic());
      return false;
    }
  }
  if (!ownActivePanel() || NSApp.keyWindow != expected) {
    error = QStringLiteral("system file input requires this test app's active, frontmost native file panel: %1")
        .arg(filePanelDiagnostic());
    return false;
  }
  return true;
}

bool systemFileAccess(NSWindow* expected, QString& error) {
  if (!systemFileTarget(expected, error)) return false;
  bool allowed = CGPreflightPostEventAccess();
  if (!allowed && qEnvironmentVariableIntValue("MITCAD_TEST_REQUEST_EVENT_ACCESS") == 1) {
    allowed = CGRequestPostEventAccess();
  }
  qInfo().noquote() << QStringLiteral("Native system file input: postEventAccess=%1 target=%2 frame=%3,%4 %5x%6")
      .arg(allowed).arg(qlonglong(expected.windowNumber)).arg(expected.frame.origin.x).arg(expected.frame.origin.y)
      .arg(expected.frame.size.width).arg(expected.frame.size.height);
  if (!allowed) {
    error = QStringLiteral("macOS event posting permission is required for native file panel tests; "
                           "allow the Mitcad test application in System Settings > Privacy & Security > Accessibility. "
                           "MITCAD_TEST_REQUEST_EVENT_ACCESS=1 explicitly requests access from this app.");
    return false;
  }
  // A permission prompt may have changed the active application.
  return systemFileTarget(expected, error);
}

bool postSystemFilePair(NSWindow* expected, CGKeyCode code, CGEventFlags flags, const QString& text,
                        QString& error) {
  if (!systemFileTarget(expected, error)) return false;
  CGEventSourceRef source = CGEventSourceCreate(kCGEventSourceStatePrivate);
  CGEventRef down = source ? CGEventCreateKeyboardEvent(source, code, true) : nullptr;
  CGEventRef up = source ? CGEventCreateKeyboardEvent(source, code, false) : nullptr;
  if (source) CFRelease(source);
  if (down == nullptr || up == nullptr) {
    if (down) CFRelease(down);
    if (up) CFRelease(up);
    error = QStringLiteral("could not create a native file panel system key event");
    return false;
  }
  CGEventSetFlags(down, flags);
  CGEventSetFlags(up, 0);
  if (!text.isEmpty()) {
    CGEventKeyboardSetUnicodeString(down, text.size(), reinterpret_cast<const UniChar*>(text.utf16()));
  }
  // This is the real WindowServer route, including the file panel's XPC
  // editor. It never substitutes a model import or sets a selected URL.
  CGEventPost(kCGHIDEventTap, down);
  CGEventPost(kCGHIDEventTap, up);
  CFRelease(down);
  CFRelease(up);
  return true;
}
} // namespace

bool sendSystemFileKey(int key, Qt::KeyboardModifiers modifiers, QString& error) {
  NSWindow* expected = targetWindow();
  if (!systemFileAccess(expected, error)) return false;
  // Physical ANSI positions for ordinary shortcut letters. A is code zero,
  // so an unsupported key must be rejected rather than using that default.
  static constexpr CGKeyCode letters[] = {
      0, 11, 8, 2, 14, 3, 5, 4, 34, 38, 40, 37, 46, 45, 31, 35, 12, 15, 1, 17, 32, 9, 13, 7, 16, 6};
  CGKeyCode code = 0;
  if (key >= Qt::Key_A && key <= Qt::Key_Z) {
    code = letters[key - Qt::Key_A];
  } else {
    switch (key) {
    case Qt::Key_Return: case Qt::Key_Enter: case Qt::Key_Tab: case Qt::Key_Backtab:
    case Qt::Key_Space: case Qt::Key_Backspace: case Qt::Key_Delete: case Qt::Key_Escape:
    case Qt::Key_Left: case Qt::Key_Right: case Qt::Key_Up: case Qt::Key_Down:
      code = virtualKey(key);
      break;
    default:
      error = QStringLiteral("unsupported native system file key %1").arg(key);
      return false;
    }
  }
  CGEventFlags flags = 0;
  if (modifiers.testFlag(Qt::ControlModifier)) flags |= kCGEventFlagMaskCommand;
  if (modifiers.testFlag(Qt::MetaModifier)) flags |= kCGEventFlagMaskControl;
  if (modifiers.testFlag(Qt::AltModifier)) flags |= kCGEventFlagMaskAlternate;
  if (modifiers.testFlag(Qt::ShiftModifier) || key == Qt::Key_Backtab) flags |= kCGEventFlagMaskShift;
  return postSystemFilePair(expected, code, flags, {}, error);
}

bool sendSystemFileText(const QString& text, QString& error) {
  NSWindow* expected = targetWindow();
  if (!systemFileAccess(expected, error)) return false;
  if (text.size() > 4096) { error = QStringLiteral("native system file text exceeds 4096 UTF-16 units"); return false; }
  // Short Unicode packets preserve arbitrary local filenames without
  // keyboard-layout assumptions. Never split a UTF-16 surrogate pair.
  for (qsizetype start = 0; start < text.size();) {
    qsizetype length = std::min<qsizetype>(16, text.size() - start);
    if (start + length < text.size() && text.at(start + length - 1).isHighSurrogate()) --length;
    if (!postSystemFilePair(expected, 0, 0, text.mid(start, length), error)) return false;
    start += length;
  }
  return true;
}

bool sendSystemPointer(const QString& identifier, QString& error) {
  NSView* view = findControl(identifier);
  NSWindow* window = view.window;
  const auto ownedActive = [view, window] {
    NSRunningApplication* front = NSWorkspace.sharedWorkspace.frontmostApplication;
    return active() && !qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT") && view != nil &&
        view.window == window && window.visible && [NSApp.windows containsObject:window] &&
        viewVisible(view) && NSApp.active && front != nil && front.processIdentifier == getpid();
  };
  if (!ownedActive() || nativeDialogModalActive()) {
    error = QStringLiteral("system pointer requires this test app's active, visible native control '%1'").arg(identifier);
    return false;
  }
  if (!CGPreflightPostEventAccess()) {
    error = QStringLiteral("system pointer needs the already granted macOS event-posting permission");
    return false;
  }
  [window layoutIfNeeded];
  [window.contentView layoutSubtreeIfNeeded];
  // AppKit can return a clipping ancestor's visible rectangle outside a
  // descendant's own bounds. Its centre then points at a neighbouring
  // control, even though this exact target is fully visible.
  const NSRect exposed = NSIntersectionRect(view.bounds, view.visibleRect);
  if (NSIsEmptyRect(exposed)) {
    error = QStringLiteral("system pointer target '%1' has no visible area inside its bounds").arg(identifier);
    return false;
  }
  const NSPoint local = NSMakePoint(NSMidX(exposed), NSMidY(exposed));
  const NSPoint inWindow = [view convertPoint:local toView:nil];
  NSView* hit = [window.contentView hitTest:[window.contentView.superview convertPoint:inWindow fromView:nil]];
  if (hit != view && ![hit isDescendantOf:view]) {
    const auto describe = [window](NSView* candidate) {
      if (candidate == nil) return QStringLiteral("nil");
      const NSRect inWindow = [candidate convertRect:candidate.bounds toView:nil];
      const NSRect onScreen = [window convertRectToScreen:inWindow];
      return QStringLiteral("%1 id='%2' identifier='%3' frame=%4 bounds=%5 visible=%6 window=%7 screen=%8")
          .arg(qt(NSStringFromClass(candidate.class)), qt(candidate.accessibilityIdentifier), qt(candidate.identifier),
               qt(NSStringFromRect(candidate.frame)), qt(NSStringFromRect(candidate.bounds)),
               qt(NSStringFromRect(candidate.visibleRect)), qt(NSStringFromRect(inWindow)), qt(NSStringFromRect(onScreen)));
    };
    QStringList ancestry;
    for (NSView* parent = hit; parent != nil && ancestry.size() < 8; parent = parent.superview) {
      ancestry.append(describe(parent));
    }
    error = QStringLiteral("system pointer target '%1' is covered; target {%2}; hit ancestry {%3}; "
                           "point local=%4 window=%5, owned window=%6")
        .arg(identifier, describe(view), ancestry.join(QStringLiteral(" -> ")),
             qt(NSStringFromPoint(local)), qt(NSStringFromPoint(inWindow)))
        .arg(qlonglong(window.windowNumber));
    return false;
  }
  const NSPoint screen = [window convertPointToScreen:inWindow];
  const CGPoint point = CGPointMake(screen.x, NSMaxY(NSScreen.screens.firstObject.frame) - screen.y);
  // AppKit returns the window that would actually receive mouseDown,
  // excluding input-transparent system overlays whose CG bounds can span
  // the entire display. A different app's input target is still refused.
  const NSInteger recipient = [NSWindow windowNumberAtPoint:screen belowWindowWithWindowNumber:0];
  if (recipient != window.windowNumber || NSEvent.pressedMouseButtons != 0 || !ownedActive()) {
    error = QStringLiteral("system pointer '%1' requires own mouse recipient %2, found %3, pressed=%4")
        .arg(identifier).arg(qlonglong(window.windowNumber)).arg(qlonglong(recipient))
        .arg(qulonglong(NSEvent.pressedMouseButtons));
    return false;
  }
  CGEventRef event = CGEventCreateMouseEvent(nullptr, kCGEventMouseMoved, point, kCGMouseButtonLeft);
  if (event == nullptr) {
    error = QStringLiteral("could not create the system pointer event");
    return false;
  }
  CGEventSetFlags(event, 0);
  CGEventPost(kCGHIDEventTap, event);
  CFRelease(event);
  qInfo().noquote() << QStringLiteral("Native system pointer: %1 at %2,%3 own window %4, access=1")
      .arg(identifier).arg(point.x).arg(point.y).arg(qlonglong(window.windowNumber));
  return true;
}

bool sendText(const QString& text) {
  if (!nativeFocused()) return false;
  NSResponder* responder = targetWindow().firstResponder;
  if ([responder conformsToProtocol:@protocol(NSTextInputClient)]) {
    [(id<NSTextInputClient>)responder insertText:text.toNSString() replacementRange:NSMakeRange(NSNotFound, 0)];
    return true;
  }
  return false;
}

bool screenshot(const QString& path, QOpenGLWidget* viewport) {
  if (!active() || !captureOwnWindows(path)) return false;
  if (!viewport) return true;
  // The window server may omit an embedded Qt/OpenGL surface, especially
  // with the VM's software GL. Keep the real AppKit capture and insert the
  // public framebuffer at the same screen rectangle, preserving any native
  // windows above the document rather than covering their controls.
  QImage shot(path);
  const QImage frame = viewport->grabFramebuffer();
  if (shot.isNull() || frame.isNull()) return false;
  CGRect bounds = CGRectNull;
  QRegion covered;
  bool aboveDocument = true;
  const NSInteger documentNumber = documentWindow().windowNumber;
  NSArray* infos = CFBridgingRelease(CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly, kCGNullWindowID));
  for (NSDictionary* info in infos) {
    if ([info[(id)kCGWindowOwnerPID] intValue] != getpid()) continue;
    CGRect rect;
    if (!CGRectMakeWithDictionaryRepresentation((__bridge CFDictionaryRef)info[(id)kCGWindowBounds], &rect) ||
        rect.size.width < 8 || rect.size.height < 8) continue;
    bounds = CGRectUnion(bounds, rect);
    const NSInteger number = [info[(id)kCGWindowNumber] integerValue];
    if (number == documentNumber) aboveDocument = false;
    else if (aboveDocument) covered += QRect(qFloor(rect.origin.x), qFloor(rect.origin.y),
                                           qCeil(rect.size.width), qCeil(rect.size.height));
  }
  if (CGRectIsNull(bounds)) return false;
  bounds = CGRectInset(bounds, -48, -48); // same shadow margin as captureOwnWindows
  const QPoint origin = viewport->mapToGlobal(QPoint());
  const QRect canvas(origin, viewport->size());
  NSView* content = documentWindow().contentView;
  NSView* host = identifiedView(content, QStringLiteral("native.viewportHost"));
  if (host && host.bounds.size.width > 0 && host.bounds.size.height > 0) {
    // The model, inspector and ribbon float inside this same NSWindow.
    // Preserve their real AppKit capture when inserting the underlying
    // OpenGL framebuffer; separate-window exclusion alone is insufficient.
    const CGFloat scaleX = viewport->width() / host.bounds.size.width;
    const CGFloat scaleY = viewport->height() / host.bounds.size.height;
    for (NSString* identifier in @[@"native.projectsCard", @"native.sidebar", @"native.inspectorCard", @"native.historyCard", @"native.ribbon", @"native.statusCard"]) {
      NSView* overlay = identifiedView(content, qt(identifier));
      if (!viewVisible(overlay)) continue;
      NSRect rect = [overlay convertRect:overlay.bounds toView:host];
      if (!host.flipped) rect.origin.y = NSMaxY(host.bounds) - NSMaxY(rect);
      covered += QRect(origin.x() + qFloor(rect.origin.x * scaleX), origin.y() + qFloor(rect.origin.y * scaleY),
                       qCeil(rect.size.width * scaleX), qCeil(rect.size.height * scaleY));
    }
  }
  QPainter painter(&shot);
  painter.scale(shot.width() / bounds.size.width, shot.height() / bounds.size.height);
  painter.translate(-bounds.origin.x, -bounds.origin.y);
  painter.setClipRegion(QRegion(canvas).subtracted(covered));
  painter.drawImage(canvas, frame);
  // CAD overlays remain Qt-painted content by design; precision fields are
  // hidden presenters and their visible AppKit child panels were protected
  // by the covered region above.
  for (QWidget* overlay : viewport->findChildren<QWidget*>(Qt::FindDirectChildrenOnly)) {
    if (overlay->isVisibleTo(viewport))
      overlay->render(&painter, overlay->mapToGlobal(QPoint()), QRegion(), QWidget::DrawChildren);
  }
  painter.end();
  return shot.save(path);
}

// Native table semantics (mitcad#814): what a cell or a label actually
// shows, the shared header's column sizes and the focused dialog's size.
namespace {

// "<table id>/<row>/<column>" addresses a cell by its native row and model
// column, for tables without identifiers of their cells.
NSView* tableCell(const QString& identifier, QString* detail) {
  // An exact identifier may itself contain slashes, such as occurrence
  // paths. Only interpret a positional locator after that lookup misses.
  if (NSView* direct = findControl(identifier)) return direct;
  const QStringList parts = identifier.split(QLatin1Char('/'));
  if (parts.size() != 3) {
    if (detail) *detail = QStringLiteral("no native control '%1'").arg(identifier);
    return nil;
  }
  bool rowValid = false;
  bool columnValid = false;
  const NSInteger row = parts.at(1).toInt(&rowValid);
  const NSInteger column = parts.at(2).toInt(&columnValid);
  NSView* view = findControl(parts.at(0));
  NSScrollView* scroll = [view isKindOfClass:NSScrollView.class] ? (NSScrollView*)view : view.enclosingScrollView;
  NSTableView* table = [scroll.documentView isKindOfClass:NSTableView.class] ? (NSTableView*)scroll.documentView : nil;
  if (detail) {
    *detail = QStringLiteral("cell locator '%1': base %2, document %3, native rows %4 columns %5, requested %6/%7")
                  .arg(identifier, view ? qt(NSStringFromClass(view.class)) : QStringLiteral("nil"),
                       scroll.documentView ? qt(NSStringFromClass(scroll.documentView.class)) : QStringLiteral("nil"))
                  .arg(table ? table.numberOfRows : -1).arg(table ? table.numberOfColumns : -1).arg(row).arg(column);
  }
  if (!rowValid || !columnValid || !table || row < 0 || column < 0 || row >= table.numberOfRows ||
      column >= table.numberOfColumns) return nil;
  [table scrollRowToVisible:row];
  [table scrollColumnToVisible:column];
  [table layoutSubtreeIfNeeded];
  NSView* live = [table viewAtColumn:column row:row makeIfNecessary:NO];
  return live ? live : [table viewAtColumn:column row:row makeIfNecessary:YES];
}

// Dynamic system colours resolve in the view's own appearance.
bool sameColor(NSColor* actual, NSColor* expected, NSAppearance* appearance) {
  __block bool same = false;
  [appearance performAsCurrentDrawingAppearance:^{
    NSColor* a = [actual colorUsingColorSpace:NSColorSpace.sRGBColorSpace];
    NSColor* b = [expected colorUsingColorSpace:NSColorSpace.sRGBColorSpace];
    same = a != nil && b != nil && std::abs(a.redComponent - b.redComponent) < 0.02 &&
           std::abs(a.greenComponent - b.greenComponent) < 0.02 && std::abs(a.blueComponent - b.blueComponent) < 0.02;
  }];
  return same;
}

// A Command Search result row (mitcad#974): the command's shared icon at
// 18 points or more, coloured artwork backed by Retina pixels, wholly in the
// visible row and left of its name and shortcut, which have width of their
// own; `enabled` or `disabled` also checks the row's dimming, `selected`
// that the row is the table's selected one.
bool searchResultIcon(NSView* view, const QStringList& states, QString& detail) {
  NSTableCellView* cell = [view isKindOfClass:NSTableCellView.class] ? (NSTableCellView*)view : nil;
  NSImageView* icon = cell.imageView;
  NSTextField* text = cell.textField;
  if (cell == nil || icon == nil || text == nil) { detail = QStringLiteral("no native result row with an icon and a name"); return false; }
  [cell layoutSubtreeIfNeeded];
  NSImage* picture = icon.image;
  NSImageRep* pixels = picture.representations.firstObject;
  const NSRect bounds = cell.bounds;
  const NSRect iconFrame = icon.frame;
  const NSRect textFrame = text.frame;
  const bool sharp = picture.isTemplate || (CGFloat(pixels.pixelsWide) >= 2 * picture.size.width - 1 &&
                                            CGFloat(pixels.pixelsHigh) >= 2 * picture.size.height - 1);
  const bool artwork = picture != nil && !icon.hidden && picture.size.width >= 16 && picture.size.height >= 16 &&
      NSWidth(iconFrame) >= 16 && NSHeight(iconFrame) >= 16 && sharp &&
      NSContainsRect(NSInsetRect(bounds, -0.5, -0.5), iconFrame);
  const bool apart = NSMinX(textFrame) >= NSMaxX(iconFrame) && NSWidth(textFrame) > 0 &&
      text.stringValue.length > 0 && NSContainsRect(NSInsetRect(bounds, -0.5, -0.5), textFrame);
  const bool labelText = [text.textColor isEqual:NSColor.labelColor];
  bool matches = true;
  if (states.contains(QStringLiteral("enabled"))) matches = labelText && icon.alphaValue > 0.99;
  if (states.contains(QStringLiteral("disabled"))) matches = !labelText && icon.alphaValue < 0.99;
  NSTableView* table = mac::owningTable(cell);
  const NSInteger row = table != nil ? [table rowForView:cell] : -1;
  const bool selected = row >= 0 && table.selectedRow == row;
  if (states.contains(QStringLiteral("selected")) && !selected) matches = false;
  detail = QStringLiteral("row %12 '%1'%13: image %2x%3 pt %4x%5 px template %6 at %7, alpha %8, name at %9, "
                          "cell %10, visible %11")
      .arg(qt(text.stringValue)).arg(picture.size.width).arg(picture.size.height)
      .arg(pixels.pixelsWide).arg(pixels.pixelsHigh).arg(bool(picture.isTemplate))
      .arg(qt(NSStringFromRect(iconFrame))).arg(icon.alphaValue).arg(qt(NSStringFromRect(textFrame)))
      .arg(qt(NSStringFromRect(bounds))).arg(viewVisible(cell)).arg(row)
      .arg(selected ? QStringLiteral(" (selected)") : QString());
  return artwork && apart && matches && viewVisible(cell);
}

// The cell's own drawing at its leading edge, left of any text or icon:
// its alpha, and whether it is a red tint.
CGFloat cellFill(NSView* cell, bool* red) {
  *red = false;
  const NSRect bounds = cell.bounds;
  if (bounds.size.width < 4 || bounds.size.height < 4) return 0;
  NSBitmapImageRep* bitmap = [cell bitmapImageRepForCachingDisplayInRect:bounds];
  if (!bitmap) return 0;
  [cell cacheDisplayInRect:bounds toBitmapImageRep:bitmap];
  NSColor* pixel = [[bitmap colorAtX:1 y:bitmap.pixelsHigh / 2] colorUsingColorSpace:NSColorSpace.sRGBColorSpace];
  if (!pixel) return 0;
  // Either alpha representation keeps red well above green and blue.
  *red = pixel.redComponent > 1.5 * pixel.greenComponent && pixel.redComponent > 1.5 * pixel.blueComponent;
  return pixel.alphaComponent;
}

double opaqueCellTextContrast(NSTableCellView* cell) {
  if (cell == nil || cell.textField == nil || NSWidth(cell.bounds) < 4 || NSHeight(cell.bounds) < 4) return 0;
  NSBitmapImageRep* bitmap = [cell bitmapImageRepForCachingDisplayInRect:cell.bounds];
  if (bitmap == nil) return 0;
  __block double result = 0;
  [cell.effectiveAppearance performAsCurrentDrawingAppearance:^{
    [cell cacheDisplayInRect:cell.bounds toBitmapImageRep:bitmap];
    NSColor* fill = [[bitmap colorAtX:1 y:bitmap.pixelsHigh / 2] colorUsingColorSpace:NSColorSpace.sRGBColorSpace];
    NSColor* ink = [cell.textField.textColor colorUsingColorSpace:NSColorSpace.sRGBColorSpace];
    // A transparent cached cell needs its parent composited before its
    // contrast can be known. This contract is for an actual opaque fill.
    if (fill == nil || ink == nil || fill.alphaComponent < 0.99) return;
    const auto channel = [](double value) {
      return value <= 0.04045 ? value / 12.92 : std::pow((value + 0.055) / 1.055, 2.4);
    };
    const auto luminance = [&](NSColor* color) {
      return 0.2126 * channel(color.redComponent) + 0.7152 * channel(color.greenComponent) +
             0.0722 * channel(color.blueComponent);
    };
    const double background = luminance(fill);
    const double foreground = luminance([ink blendedColorWithFraction:1 - ink.alphaComponent ofColor:fill]);
    result = (std::max(background, foreground) + 0.05) / (std::min(background, foreground) + 0.05);
  }];
  return result;
}

bool boldFont(NSFont* font) {
  NSDictionary* traits = [font.fontDescriptor objectForKey:NSFontTraitsAttribute];
  const double weight = [traits[NSFontWeightTrait] doubleValue];
  return weight >= 0.2 || (font.fontDescriptor.symbolicTraits & NSFontDescriptorTraitBold) != 0;
}

struct DisplayedFontWeight {
  NSUInteger bold = 0;
  NSUInteger regular = 0;
  NSUInteger unknown = 0;

  bool allBold() const { return bold > 0 && regular == 0 && unknown == 0; }
  bool allRegular() const { return regular > 0 && bold == 0 && unknown == 0; }
};

DisplayedFontWeight displayedFontWeight(NSTextField* field) {
  // Rich labels carry the displayed font on attributed glyph runs, while
  // NSControl.font can still be the plain base font. A mixed label must
  // satisfy neither whole-label bold nor whole-label regular.
  __block DisplayedFontWeight result;
  const auto record = [](DisplayedFontWeight& weights, NSFont* font) {
    if (font == nil) ++weights.unknown;
    else if (boldFont(font)) ++weights.bold;
    else ++weights.regular;
  };
  NSAttributedString* displayed = field.attributedStringValue;
  NSCharacterSet* glyphs = NSCharacterSet.whitespaceAndNewlineCharacterSet.invertedSet;
  [displayed enumerateAttribute:NSFontAttributeName inRange:NSMakeRange(0, displayed.length) options:0
      usingBlock:^(id value, NSRange range, BOOL* stop) {
    (void)stop;
    if ([displayed.string rangeOfCharacterFromSet:glyphs options:0 range:range].location == NSNotFound) return;
    NSFont* font = [value isKindOfClass:NSFont.class] ? (NSFont*)value : field.font;
    record(result, font);
  }];
  if (result.bold + result.regular + result.unknown == 0) record(result, field.font);
  return result;
}

QHeaderView* sharedHeader(const QString& identifier) {
  for (QWidget* widget : QApplication::allWidgets()) {
    if (widget->objectName() != identifier) continue;
    if (auto* tree = qobject_cast<QTreeView*>(widget)) return tree->header();
    if (auto* grid = qobject_cast<QTableView*>(widget)) return grid->horizontalHeader();
  }
  return nullptr;
}

// The Settings section selector (mitcad#971): every section button shows
// its template icon above its complete name, both within the visible
// button, the icon at its points backed by Retina pixels; exactly one
// section is chosen, the one whose page the tab view shows, and its name is
// `chosen` when that is given.
bool paneIcons(NSView* view, const QString& chosen, QString& detail) {
  NSStackView* selector = [view isKindOfClass:NSStackView.class] ? (NSStackView*)view : nil;
  if (selector == nil) { detail = QStringLiteral("no native section selector"); return false; }
  NSTabView* pages = nil;
  for (NSView* sibling in selector.superview.subviews) {
    if ([sibling isKindOfClass:NSTabView.class]) pages = (NSTabView*)sibling;
  }
  const NSInteger shown = pages.selectedTabViewItem != nil ? [pages indexOfTabViewItem:pages.selectedTabViewItem] : -1;
  QStringList sections;
  NSInteger index = -1;
  int selected = 0;
  bool complete = true;
  bool matches = chosen.isEmpty();
  for (NSView* child in selector.arrangedSubviews) {
    NSButton* button = [child isKindOfClass:NSButton.class] ? (NSButton*)child : nil;
    if (button == nil) continue;
    ++index;
    NSImage* picture = button.image;
    NSImageRep* pixels = picture.representations.firstObject;
    const NSRect bounds = button.bounds;
    const NSRect icon = [(NSButtonCell*)button.cell imageRectForBounds:bounds];
    const NSRect caption = [(NSButtonCell*)button.cell titleRectForBounds:bounds];
    NSFont* font = button.font != nil ? button.font : [NSFont systemFontOfSize:0];
    const NSSize glyphs = [button.title sizeWithAttributes:@{NSFontAttributeName: font}];
    const bool artwork = picture != nil && picture.isTemplate && button.imagePosition == NSImageAbove &&
        picture.size.width >= 16 && picture.size.height >= 16 &&
        CGFloat(pixels.pixelsWide) >= 2 * picture.size.width - 1 &&
        CGFloat(pixels.pixelsHigh) >= 2 * picture.size.height - 1 &&
        NSWidth(icon) >= 16 && NSHeight(icon) >= 16 && NSContainsRect(NSInsetRect(bounds, -0.5, -0.5), icon);
    const bool readable = button.title.length > 0 && glyphs.width <= NSWidth(caption) + 0.5 &&
        glyphs.height <= NSHeight(caption) + 0.5 && NSContainsRect(NSInsetRect(bounds, -0.5, -0.5), caption);
    const bool on = button.state == NSControlStateValueOn;
    if (on) {
      ++selected;
      if (index != shown) complete = false;
      if (!chosen.isEmpty() && qt(button.title) == chosen) matches = true;
    }
    if (!artwork || !readable || !viewVisible(button)) complete = false;
    sections << QStringLiteral("%1%2 [image %3x%4 pt %5x%6 px template %7 at %8, title %9x%10 in %11, visible %12]")
        .arg(qt(button.title), on ? QStringLiteral(" (chosen)") : QString())
        .arg(picture.size.width).arg(picture.size.height).arg(pixels.pixelsWide).arg(pixels.pixelsHigh)
        .arg(bool(picture.isTemplate)).arg(qt(NSStringFromRect(icon)))
        .arg(glyphs.width).arg(glyphs.height).arg(qt(NSStringFromRect(caption))).arg(viewVisible(button));
  }
  detail = QStringLiteral("%1 sections, %2 chosen, page %3 shown: %4")
      .arg(index + 1).arg(selected).arg(shown).arg(sections.join(QStringLiteral("; ")));
  return index >= 1 && selected == 1 && complete && matches;
}

} // namespace

bool nativeStyle(const QString& identifier, const QStringList& states, QString& detail) {
  NSView* view = tableCell(identifier);
  if (states.value(0) == QLatin1String("pane-icons")) return paneIcons(view, states.value(1), detail);
  if (states == QStringList{QStringLiteral("artwork")}) return mac::navigationGuideArtwork(view, detail);
  if (states == QStringList{QStringLiteral("caption")}) {
    NSButton* button = [view isKindOfClass:NSButton.class] ? (NSButton*)view : nil;
    if (button == nil) { detail = QStringLiteral("no native button for its displayed caption"); return false; }
    NSButtonCell* cell = (NSButtonCell*)button.cell;
    const NSRect titleFrame = [cell titleRectForBounds:button.bounds];
    NSAttributedString* attributed = button.attributedTitle;
    NSFont* font = button.font != nil ? button.font : [NSFont systemFontOfSize:0];
    const NSSize glyphs = attributed.length > 0 ? attributed.size
        : [button.title sizeWithAttributes:@{NSFontAttributeName: font}];
    // A title or accessibility label can survive NSImageOnly while AppKit
    // draws no caption. Check the actual cell mode and finite title area.
    const bool displayed = button.imagePosition != NSImageOnly && button.title.length > 0;
    const bool finite = std::isfinite(NSMinX(titleFrame)) && std::isfinite(NSMinY(titleFrame)) &&
                        std::isfinite(NSWidth(titleFrame)) && std::isfinite(NSHeight(titleFrame));
    const bool fits = finite && glyphs.width > 0 && glyphs.height > 0 &&
        glyphs.width <= NSWidth(titleFrame) + 0.5 && glyphs.height <= NSHeight(titleFrame) + 0.5 &&
        NSContainsRect(NSInsetRect(button.visibleRect, -0.5, -0.5), titleFrame);
    detail = QStringLiteral("button '%1': image position %2, image %3, displayed caption %4, "
                            "title area %5, glyphs %6x%7, visible %8, fits %9")
        .arg(qt(button.title)).arg(button.imagePosition).arg(button.image != nil).arg(displayed)
        .arg(qt(NSStringFromRect(titleFrame))).arg(glyphs.width).arg(glyphs.height)
        .arg(viewVisible(button)).arg(fits);
    return displayed && viewVisible(button) && fits;
  }
  if (states.value(0) == QLatin1String("result-icon")) return searchResultIcon(view, states, detail);
  if (states.contains(QStringLiteral("current-row"))) {
    NSTableView* table = mac::owningTable(view);
    const NSInteger row = table != nil ? [table rowForView:view] : -1;
    NSTableRowView* native = row >= 0 ? [table rowViewAtRow:row makeIfNecessary:NO] : nil;
    auto* cursor = [native isKindOfClass:MitcadNativeCurrentRowView.class] ? (MitcadNativeCurrentRowView*)native : nil;
    [cursor displayIfNeeded];
    NSButton* check = [view isKindOfClass:NSButton.class] ? (NSButton*)view : nil;
    if (check == nil) {
      for (NSView* child in view.subviews) if ([child isKindOfClass:NSButton.class]) { check = (NSButton*)child; break; }
    }
    NSColor* titleColor = check.attributedTitle.length > 0
        ? [check.attributedTitle attribute:NSForegroundColorAttributeName atIndex:0 effectiveRange:nullptr] : nil;
    const bool warning = titleColor != nil && sameColor(titleColor, NSColor.systemOrangeColor, check.effectiveAppearance);
    const bool framed = cursor != nil && cursor.currentFocus && cursor.paintedCurrentFocus && !cursor.selected &&
        !NSIsEmptyRect(cursor.visibleRect) && cursor.window != nil;
    detail = QStringLiteral("current-row frame %1, painted %2, selected %3, checkbox state %4, warning %5, row %6")
        .arg(framed).arg(bool(cursor.paintedCurrentFocus)).arg(bool(cursor.selected))
        .arg(check != nil ? check.state : -2).arg(warning).arg(row);
    for (const QString& state : states) {
      const bool matches = state == QLatin1String("current-row") ? framed :
          state == QLatin1String("checked") ? check != nil && check.state == NSControlStateValueOn :
          state == QLatin1String("unchecked") ? check != nil && check.state == NSControlStateValueOff :
          state == QLatin1String("warning") ? warning : false;
      if (!matches) return false;
    }
    return true;
  }
  NSTableCellView* cell = [view isKindOfClass:NSTableCellView.class] ? (NSTableCellView*)view : nil;
  NSTextField* field = cell ? cell.textField : [view isKindOfClass:NSTextField.class] ? (NSTextField*)view : nil;
  if (!field) { detail = QStringLiteral("no native text cell or label"); return false; }
  NSAppearance* appearance = field.effectiveAppearance;
  const DisplayedFontWeight weight = displayedFontWeight(field);
  const bool redText = sameColor(field.textColor, NSColor.systemRedColor, appearance);
  const bool orangeText = sameColor(field.textColor, NSColor.systemOrangeColor, appearance);
  bool redFill = false;
  const CGFloat fill = cell ? cellFill(cell, &redFill) : 0;
  NSTableView* table = nil;
  for (NSView* parent = field.superview; parent && !table; parent = parent.superview) {
    if ([parent isKindOfClass:NSTableView.class]) table = (NSTableView*)parent;
  }
  const NSInteger nativeRow = cell != nil && table != nil ? [table rowForView:cell] : -1;
  NSTableRowView* rowView = nativeRow >= 0 ? [table rowViewAtRow:nativeRow makeIfNecessary:NO] : nil;
  const bool rowSelected = rowView != nil && rowView.selected &&
      [table.selectedRowIndexes containsIndex:static_cast<NSUInteger>(nativeRow)];
  bool sourceSingleCell = false;
  if (states.contains(QStringLiteral("source-single-cell")) && table != nil && cell != nil) {
    const QString tableId = qt(table.enclosingScrollView.accessibilityIdentifier);
    const QString cellId = qt(cell.accessibilityIdentifier);
    for (QWidget* widget : QApplication::allWidgets()) {
      const auto* source = qobject_cast<QAbstractItemView*>(widget);
      if (source == nullptr || source->objectName() != tableId || source->selectionModel() == nullptr) continue;
      const QModelIndex current = source->selectionModel()->currentIndex();
      sourceSingleCell = source->selectionBehavior() == QAbstractItemView::SelectItems && current.isValid() &&
          current.model()->columnCount(current.parent()) > 1 &&
          current.data(NativeControlIdentifierRole).toString() == cellId &&
          source->selectionModel()->isSelected(current) && source->selectionModel()->selectedIndexes().size() == 1;
      break;
    }
  }
  const NSRect frame = cell ? [field convertRect:field.bounds toView:cell] : field.bounds;
  const NSLineBreakMode mode = field.lineBreakMode;
  const bool truncating = mode == NSLineBreakByTruncatingTail || mode == NSLineBreakByTruncatingMiddle ||
                          mode == NSLineBreakByTruncatingHead;
  const bool singleLine = field.maximumNumberOfLines == 1 && !field.cell.wraps && truncating &&
                          field.intrinsicContentSize.height <= NSHeight(field.bounds) + 1 &&
                          (!cell || (NSMinY(frame) >= -0.5 && NSMaxY(frame) <= NSHeight(cell.bounds) + 0.5));
  const bool truncated = field.cell.cellSize.width > NSWidth(field.bounds) + 1;
  bool inView = false;
  if (table) {
    const NSRect inTable = [field convertRect:field.bounds toView:table];
    const NSRect visible = table.visibleRect;
    inView = NSMinX(inTable) >= NSMinX(visible) - 1 && NSMaxX(inTable) <= NSMaxX(visible) + 1 &&
             NSMinY(inTable) >= NSMinY(visible) - 1 && NSMaxY(inTable) <= NSMaxY(visible) + 1;
  }
  // Where the keyboard focus is, for an edit that did not start or ended.
  NSResponder* focus = field.window.firstResponder;
  QString focused = focus != nil ? qt(NSStringFromClass(focus.class)) : QStringLiteral("none");
  if ([focus isKindOfClass:NSTextView.class] && ((NSTextView*)focus).isFieldEditor) {
    id edited = ((NSTextView*)focus).delegate;
    NSView* holder = [edited isKindOfClass:NSView.class] ? ((NSView*)edited).superview : nil;
    focused += QStringLiteral(" editing '%1'").arg(qt(holder.accessibilityIdentifier));
  }
  detail = QStringLiteral("text '%1', red %2, orange %3, fill alpha %4 red %5, bold %6, editable %7, editing %8, "
                          "single line %9, truncated %10, in view %11, in window %12, focus %13")
               .arg(qt(field.stringValue)).arg(redText).arg(orangeText).arg(double(fill), 0, 'f', 2).arg(redFill)
               .arg(weight.allBold()).arg(bool(field.editable)).arg(field.currentEditor != nil).arg(singleLine)
               .arg(truncated).arg(inView).arg(field.window != nil).arg(focused);
  detail += QStringLiteral(", displayed font runs bold=%1 regular=%2 unknown=%3, base bold=%4")
      .arg(qulonglong(weight.bold)).arg(qulonglong(weight.regular)).arg(qulonglong(weight.unknown))
      .arg(boldFont(field.font));
  if (states.contains(QStringLiteral("row-selected")) || states.contains(QStringLiteral("source-single-cell")))
    detail += QStringLiteral(", native row selected %1, presenter single current cell selected %2")
        .arg(rowSelected).arg(sourceSingleCell);
  const double textContrast = states.contains(QStringLiteral("readable-fill")) ? opaqueCellTextContrast(cell) : 0;
  if (states.contains(QStringLiteral("readable-fill")))
    detail += QStringLiteral(", opaque fill text contrast %1").arg(textContrast, 0, 'f', 2);
  for (const QString& state : states) {
    const bool matches =
        state == QLatin1String("error") ? redText && (!cell || (fill > 0.04 && redFill)) :
        state == QLatin1String("warning") ? orangeText :
        state == QLatin1String("normal") ? !redText && !orangeText && fill <= 0.04 :
        state == QLatin1String("bold") ? weight.allBold() :
        state == QLatin1String("regular") ? weight.allRegular() :
        state == QLatin1String("editable") ? bool(field.editable) :
        state == QLatin1String("readonly") ? !field.editable :
        state == QLatin1String("selectable") ? bool(field.selectable) :
        state == QLatin1String("selected") ? field.currentEditor != nil && field.currentEditor.selectedRange.length > 0 :
        state == QLatin1String("row-selected") ? rowSelected :
        state == QLatin1String("source-single-cell") ? sourceSingleCell :
        state == QLatin1String("readable-fill") ? textContrast >= 4.5 :
        state == QLatin1String("editing") ? field.currentEditor != nil :
        state == QLatin1String("not-editing") ? field.currentEditor == nil :
        state == QLatin1String("single-line") ? singleLine :
        state == QLatin1String("truncated") ? truncated :
        state == QLatin1String("untruncated") ? !truncated :
        state == QLatin1String("in-view") ? inView : false;
    if (!matches) {
      detail = QStringLiteral("not %1: %2").arg(state, detail);
      return false;
    }
  }
  return true;
}

bool nativeColumns(const QString& identifier, const QString& mode, QString& detail) {
  NSView* view = findControl(identifier);
  NSScrollView* scroll = [view isKindOfClass:NSScrollView.class] ? (NSScrollView*)view : view.enclosingScrollView;
  NSTableView* table = [scroll.documentView isKindOfClass:NSTableView.class] ? (NSTableView*)scroll.documentView : nil;
  QHeaderView* header = sharedHeader(identifier);
  if (!table || !header) { detail = QStringLiteral("no native table with a shared header"); return false; }
  [scroll layoutSubtreeIfNeeded];
  const CGFloat visible = scroll.contentView.bounds.size.width;
  CGFloat total = 0;
  int stretching = 0;
  bool atMinimum = true;
  for (NSTableColumn* column in table.tableColumns) {
    const int section = qt(column.identifier).toInt();
    if (section < 0 || section >= header->count()) continue;
    if (bool(column.hidden) != header->isSectionHidden(section)) {
      detail = QStringLiteral("column %1 hidden %2, the shared section %3").arg(section).arg(bool(column.hidden))
                   .arg(header->isSectionHidden(section));
      return false;
    }
    if (column.hidden) continue;
    const bool contentSized = header->sectionResizeMode(section) == QHeaderView::ResizeToContents ||
        (header->sectionResizeMode(section) == QHeaderView::Interactive &&
         header->property(NativeColumnContentsRequestProperty).toULongLong() != 0);
    if (contentSized && table.headerView != nil) {
      const NSUInteger visual = [table.tableColumns indexOfObjectIdenticalTo:column];
      NSRect frame = [table.headerView headerRectOfColumn:static_cast<NSInteger>(visual)];
      NSOutlineView* outline = [table isKindOfClass:NSOutlineView.class] ? (NSOutlineView*)table : nil;
      const CGFloat indentation = outline.outlineTableColumn == column ? outline.indentationPerLevel : 0;
      const CGFloat outside = std::max<CGFloat>(0, column.width - NSWidth(frame));
      const CGFloat unreported = std::max<CGFloat>(0, indentation - outside);
      frame.origin.x += unreported;
      frame.size.width = std::max<CGFloat>(0, NSWidth(frame) - unreported);
      const NSRect title = [column.headerCell titleRectForBounds:frame];
      NSFont* font = column.headerCell.font != nil ? column.headerCell.font : [NSFont systemFontOfSize:NSFont.systemFontSize];
      const NSSize glyphs = [column.title sizeWithAttributes:@{NSFontAttributeName:font}];
      detail = QStringLiteral("header '%1' needs %2x%3, its native title rect %4 in column %5, outline reserve %6 (%7 unreported)")
          .arg(qt(column.title)).arg(glyphs.width).arg(glyphs.height).arg(qt(NSStringFromRect(title))).arg(column.width)
          .arg(indentation).arg(unreported);
      if (NSWidth(title) + 1 < glyphs.width || NSHeight(title) + 1 < glyphs.height) return false;
      qInfo().noquote() << QStringLiteral("Native content-sized %1: %2").arg(identifier, detail);
    }
    total += column.width + table.intercellSpacing.width;
    if ((column.resizingMask & NSTableColumnAutoresizingMask) != 0) {
      ++stretching;
      atMinimum = atMinimum && column.width <= column.minWidth + 1;
    } else if (contentSized
                   ? column.width + 1 < header->sectionSize(section)
                   : std::abs(column.width - header->sectionSize(section)) > 1) {
      detail = QStringLiteral("column %1 is %2 wide, its shared section %3").arg(section).arg(double(column.width))
                   .arg(header->sectionSize(section));
      return false;
    }
  }
  const bool fits = total <= visible + 1;
  detail = QStringLiteral("columns %1 wide in %2, %3 stretching").arg(double(total)).arg(double(visible)).arg(stretching);
  if (fits && stretching > 0 && total < visible - stretching - 1) {
    detail += QStringLiteral(", the stretching columns leave space");
    return false;
  }
  if (!fits && stretching > 0 && !atMinimum) {
    detail += QStringLiteral(", wider before the stretching columns reach their minimum");
    return false;
  }
  if (!fits && (!scroll.hasHorizontalScroller || NSWidth(table.bounds) + 1 < total)) {
    detail += QStringLiteral(", the overflow cannot be scrolled");
    return false;
  }
  if (mode == QLatin1String("fit")) return fits;
  if (mode == QLatin1String("scroll")) return !fits;
  return true;
}

bool resizeNativeDialog(int width, int height, QString& error) {
  NSWindow* window = targetWindow();
  if (!window || window == documentWindow() || [window isKindOfClass:NSSavePanel.class]) {
    error = QStringLiteral("native-dialog-size requires a focused native dialog");
    return false;
  }
  [window setContentSize:NSMakeSize(width, height)];
  [window.contentView layoutSubtreeIfNeeded];
  const NSSize size = window.contentView.frame.size;
  qInfo().noquote() << QStringLiteral("Native dialog %1 content %2 x %3")
                           .arg(qt(window.title)).arg(double(size.width)).arg(double(size.height));
  return true;
}

} // namespace mitcad::mac::test

// --- Card widths and windows within the screen (mitcad#818).
#include "NativeCardWidths.hpp"
#include "NativeWindowFit.hpp"

namespace mitcad::mac::test {
namespace {

// A visible side card of the document window and its width handle.
bool cardAndHandle(const QString& identifier, NSView* __strong& card, NSView* __strong& handle, QString& error) {
  NSWindow* window = documentWindow();
  card = window != nil ? identifiedView(window.contentView, identifier) : nil;
  handle = card != nil ? identifiedView(card, identifier + QStringLiteral(".width")) : nil;
  if (!viewVisible(card) || handle == nil || handle.hidden) {
    error = QStringLiteral("no visible native card '%1' with a width handle").arg(identifier);
    return false;
  }
  return true;
}

NSEvent* pointerEvent(NSEventType type, NSPoint location, NSWindow* window) {
  return [NSEvent mouseEventWithType:type location:location modifierFlags:0
                           timestamp:NSProcessInfo.processInfo.systemUptime windowNumber:window.windowNumber
                             context:nil eventNumber:0 clickCount:1
                            pressure:(type == NSEventTypeLeftMouseUp ? 0 : 1)];
}

// The view that shows a card's content, which follows the card's width.
NSView* cardContent(NSView* card, const QString& identifier) {
  const QString content = identifier == QStringLiteral("native.projectsCard") ? QStringLiteral("projectsTree")
                          : identifier == QStringLiteral("native.sidebar")    ? QStringLiteral("native.modelTree")
                                                                               : QStringLiteral("native.inspector");
  NSView* view = identifiedView(card, content);
  return [view isKindOfClass:NSOutlineView.class] ? view.enclosingScrollView : view;
}

} // namespace

bool dragCardWidth(const QString& identifier, int width, QString& error) {
  NSView* card = nil;
  NSView* handle = nil;
  if (!cardAndHandle(identifier, card, handle, error)) return false;
  NSWindow* window = card.window;
  NSView* content = window.contentView;
  [content layoutSubtreeIfNeeded];
  // The pointer reaches the handle above the card and the canvas, and
  // the handle shows the resize cursor.
  const NSPoint start = [handle convertPoint:NSMakePoint(NSMidX(handle.bounds), NSMidY(handle.bounds)) toView:nil];
  NSView* hit = [content hitTest:[content.superview convertPoint:start fromView:nil]];
  if (hit != handle) {
    error = QStringLiteral("the width handle of '%1' does not receive the pointer").arg(identifier);
    return false;
  }
  [handle updateTrackingAreas];
  bool cursor = false;
  for (NSTrackingArea* area in handle.trackingAreas) {
    cursor = cursor || (area.owner == handle && (area.options & NSTrackingCursorUpdate) != 0);
  }
  if (!cursor || ![handle respondsToSelector:@selector(cursorUpdate:)]) {
    error = QStringLiteral("the width handle of '%1' has no resize cursor").arg(identifier);
    return false;
  }
  const bool leading = NSMidX(handle.frame) < NSMidX(card.bounds);
  const CGFloat delta = CGFloat(width) - card.frame.size.width;
  const NSPoint end = NSMakePoint(start.x + (leading ? -delta : delta), start.y);
  [window makeKeyAndOrderFront:nil];
  [handle mouseDown:pointerEvent(NSEventTypeLeftMouseDown, start, window)];
  [handle mouseDragged:pointerEvent(NSEventTypeLeftMouseDragged, NSMakePoint((start.x + end.x) / 2, start.y), window)];
  [handle mouseDragged:pointerEvent(NSEventTypeLeftMouseDragged, end, window)];
  [handle mouseUp:pointerEvent(NSEventTypeLeftMouseUp, end, window)];
  return true;
}

bool cardWidth(const QString& identifier, int minimum, int maximum, QString& detail) {
  NSView* card = nil;
  NSView* handle = nil;
  if (!cardAndHandle(identifier, card, handle, detail)) return false;
  NSView* content = card.window.contentView;
  [content layoutSubtreeIfNeeded];
  const CGFloat width = card.frame.size.width;
  detail = QStringLiteral("native card '%1' is %2 points wide").arg(identifier).arg(width);
  if (width < minimum - 1 || width > maximum + 1) return false;
  // The handle stays on the card's inner edge.
  const NSRect grip = handle.frame;
  if (std::min(std::abs(NSMinX(grip)), std::abs(NSMaxX(grip) - width)) > 1 || grip.size.height <= 0) {
    detail += QStringLiteral(", its width handle is off its edge");
    return false;
  }
  // The card's content follows its width.
  NSView* inner = cardContent(card, identifier);
  const CGFloat innerWidth = inner != nil ? [inner convertRect:inner.bounds toView:card].size.width : 0;
  if (viewVisible(inner) && innerWidth < width - 20) {
    detail += QStringLiteral(", its content is %1 points wide").arg(innerWidth);
    return false;
  }
  // Cards on both sides leave the free canvas between them.
  CGFloat left = 0;
  for (NSString* side in @[@"native.projectsCard", @"native.sidebar"]) {
    NSView* view = identifiedView(content, qt(side));
    if (viewVisible(view)) left = std::max(left, NSMaxX([view convertRect:view.bounds toView:content]));
  }
  NSView* inspector = identifiedView(content, QStringLiteral("native.inspectorCard"));
  if (left > 0 && viewVisible(inspector)) {
    const CGFloat between = NSMinX([inspector convertRect:inspector.bounds toView:content]) - left;
    if (between < NativeCardWidths::freeCanvas - 1) {
      detail += QStringLiteral(", only %1 points of canvas between the side cards").arg(between);
      return false;
    }
  }
  return true;
}

bool nameFits(const QRegularExpression& expression, bool expected, QString& detail) {
  NSWindow* window = documentWindow();
  NSView* found = window == nil ? nil : findView(window.contentView, [&](NSView* view) {
    return [view isKindOfClass:NSTextField.class] && expression.match(qt(((NSTextField*)view).stringValue)).hasMatch() &&
           viewVisible(view);
  });
  if (found == nil) {
    detail = QStringLiteral("no visible native text matches");
    return false;
  }
  NSTextField* field = (NSTextField*)found;
  const CGFloat needed = [field.cell cellSizeForBounds:NSMakeRect(0, 0, CGFLOAT_MAX, NSHeight(field.bounds))].width;
  const bool fits = needed <= NSWidth(field.bounds) + 0.5;
  detail = QStringLiteral("'%1' needs %2 of its %3 points").arg(qt(field.stringValue)).arg(needed).arg(NSWidth(field.bounds));
  return fits == expected;
}

bool setScreenLimit(int width, int height, QString& error) {
  if (width < 0 || height < 0 || (width > 0) != (height > 0)) {
    error = QStringLiteral("native-screen-limit expects both sizes positive, or 0 0 for none");
    return false;
  }
  setNativeTestScreenLimit(NSMakeSize(width, height));
  // As a display change: the application's windows follow it.
  [NSNotificationCenter.defaultCenter postNotificationName:NSApplicationDidChangeScreenParametersNotification
                                                    object:NSApp];
  return true;
}

bool windowFits(const QRegularExpression& expression, QString& detail) {
  NSWindow* window = nil;
  for (NSWindow* candidate in nativeWindows()) {
    if (windowMatches(candidate, expression)) {
      window = candidate;
      break;
    }
  }
  if (window == nil) {
    detail = QStringLiteral("no native window matches");
    return false;
  }
  const NSRect area = nativeWindowAvailableFrame(window);
  const NSRect frame = window.frame;
  detail = QStringLiteral("window %1,%2 %3x%4 in visible %5,%6 %7x%8")
               .arg(frame.origin.x).arg(frame.origin.y).arg(frame.size.width).arg(frame.size.height)
               .arg(area.origin.x).arg(area.origin.y).arg(area.size.width).arg(area.size.height);
  if (!NSContainsRect(NSInsetRect(area, -1, -1), frame)) return false;
  NSView* content = window.contentView;
  [content layoutSubtreeIfNeeded];
  NSView* body = identifiedView(content, QStringLiteral("native.secondary.body"));
  if (![body isKindOfClass:NSScrollView.class] || !viewVisible(body)) {
    detail += QStringLiteral(", no scrolling body");
    return false;
  }
  // The dialog's buttons stay visible below the body and take the pointer.
  NSView* footer = identifiedView(content, QStringLiteral("native.secondary.footer"));
  int buttons = 0;
  NSMutableArray<NSView*>* pending = [NSMutableArray array];
  if (footer != nil && !footer.hidden) [pending addObject:footer];
  while (pending.count > 0) {
    NSView* view = pending.lastObject;
    [pending removeLastObject];
    if (view.hidden) continue;
    if (![view isKindOfClass:NSButton.class]) {
      [pending addObjectsFromArray:view.subviews];
      continue;
    }
    const NSRect rect = [view convertRect:view.bounds toView:content];
    NSView* hit = [content hitTest:[view convertPoint:NSMakePoint(NSMidX(view.bounds), NSMidY(view.bounds))
                                               toView:content.superview]];
    if (!NSContainsRect(NSInsetRect(content.bounds, -1, -1), rect) || (hit != view && ![hit isDescendantOf:view])) {
      detail += QStringLiteral(", button '%1' is clipped or covered").arg(qt(((NSButton*)view).title));
      return false;
    }
    ++buttons;
  }
  if (buttons == 0) {
    detail += QStringLiteral(", no visible dialog buttons below the body");
    return false;
  }
  // All of the body can be scrolled into view.
  NSScrollView* scroll = (NSScrollView*)body;
  NSView* document = scroll.documentView;
  const NSRect last = NSMakeRect(0, NSMaxY(document.bounds) - 1, 1, 1);
  [document scrollRectToVisible:last];
  const bool reached = NSIntersectsRect(scroll.contentView.documentVisibleRect, last);
  [document scrollPoint:NSZeroPoint];
  detail += QStringLiteral(", body %1 of %2 points").arg(scroll.contentView.bounds.size.height).arg(document.frame.size.height);
  if (!reached) {
    detail += QStringLiteral(", its end cannot be scrolled into view");
    return false;
  }
  qInfo().noquote() << QStringLiteral("Native window fits: %1").arg(detail);
  return true;
}

} // namespace mitcad::mac::test

// ---------------------------------------------------------------------------
// Native Projects rows (mitcad#816)

// The destination-side state of a drag inside one outline. AppKit's own
// NSDraggingInfo exists only within a pointer-driven drag session; the
// outline's data source reads just these properties of it.
@interface MITCADTestRowDrag : NSObject
@property(nonatomic, weak) id draggingSource;
@property(nonatomic) NSPoint draggingLocation;
@property(nonatomic, strong) NSPasteboard* draggingPasteboard;
@property(nonatomic, weak) NSWindow* draggingDestinationWindow;
@property(nonatomic) NSDragOperation draggingSourceOperationMask;
@end
@implementation MITCADTestRowDrag
@end

namespace mitcad::mac::test {

namespace {
// An outline row of the document by its cell's identifier. Unlike
// findControl it never scrolls: a drag between rows leaves the outline's
// scroll position, and so its selection's visibility, as the user had it.
NSOutlineView* outlineRow(const QString& identifier, NSInteger& row) {
  row = -1;
  NSWindow* window = documentWindow();
  NSView* root = window.contentView.superview != nil ? window.contentView.superview : window.contentView;
  NSString* wanted = identifier.toNSString();
  NSOutlineView* found = nil;
  findView(root, [&](NSView* view) {
    if (![view isKindOfClass:NSOutlineView.class]) return false;
    NSOutlineView* outline = (NSOutlineView*)view;
    for (NSInteger index = 0; index < outline.numberOfRows; ++index) {
      NSView* cell = [outline viewAtColumn:0 row:index makeIfNecessary:YES];
      if ([cell.accessibilityIdentifier isEqualToString:wanted]) {
        found = outline;
        row = index;
        return true;
      }
    }
    return false;
  });
  return found;
}
} // namespace

bool rowsPresent(const QStringList& identifiers) {
  if (!active()) return false;
  for (const QString& identifier : identifiers) {
    NSInteger row = -1;
    if (outlineRow(identifier, row) == nil) return false;
  }
  return true;
}

bool dragRow(const QString& from, const QString& to, QString& error) {
  NSInteger fromRow = -1;
  NSInteger toRow = -1;
  NSOutlineView* outline = outlineRow(from, fromRow);
  NSOutlineView* toOutline = outlineRow(to, toRow);
  if (outline == nil || outline != toOutline || fromRow < 0 || toRow < 0) {
    error = QStringLiteral("native rows '%1' and '%2' are not rows of one outline").arg(from, to);
    return false;
  }
  id<NSOutlineViewDataSource> source = outline.dataSource;
  if (outline.registeredDraggedTypes.count == 0 ||
      ![source respondsToSelector:@selector(outlineView:pasteboardWriterForItem:)] ||
      ![source respondsToSelector:@selector(outlineView:validateDrop:proposedItem:proposedChildIndex:)] ||
      ![source respondsToSelector:@selector(outlineView:acceptDrop:item:childIndex:)]) {
    error = QStringLiteral("the native outline of '%1' does not move rows by dragging").arg(from);
    return false;
  }
  [outline.window makeKeyAndOrderFront:nil];
  id dragged = [outline itemAtRow:fromRow];
  id target = [outline itemAtRow:toRow];
  const NSRect fromRect = [outline rectOfRow:fromRow];
  const NSRect toRect = [outline rectOfRow:toRow];
  const NSPoint press = NSMakePoint(NSMidX(fromRect), NSMidY(fromRect));
  // The pointer over the target row's middle, in window coordinates.
  const NSPoint over = [outline convertPoint:NSMakePoint(NSMidX(toRect), NSMidY(toRect)) toView:nil];
  QString outcome = QStringLiteral("not draggable");
  id<NSPasteboardWriting> writer = nil;
  const bool offered = [outline canDragRowsWithIndexes:[NSIndexSet indexSetWithIndex:NSUInteger(fromRow)] atPoint:press];
  if (offered) writer = [source outlineView:outline pasteboardWriterForItem:dragged];
  if (writer == nil) {
    qInfo().noquote() << QStringLiteral("Native test: row drag %1: %2 (row %3 of %4)")
        .arg(from, offered ? QStringLiteral("no pasteboard row") : QStringLiteral("the presenter does not offer the row"))
        .arg(fromRow).arg(outline.numberOfRows);
  }
  if (writer != nil) {
    NSPasteboard* pasteboard = [NSPasteboard pasteboardWithUniqueName];
    [pasteboard clearContents];
    [pasteboard writeObjects:@[writer]];
    // Only a pointer-driven drag has an AppKit session.
    NSDraggingSession* session = nil;
    if ([source respondsToSelector:@selector(outlineView:draggingSession:willBeginAtPoint:forItems:)]) {
      const NSPoint start = [outline.window convertPointToScreen:[outline convertPoint:press toView:nil]];
      [source outlineView:outline draggingSession:session willBeginAtPoint:start forItems:@[dragged]];
    }
    MITCADTestRowDrag* info = [MITCADTestRowDrag new];
    info.draggingSource = outline;
    info.draggingLocation = over;
    info.draggingPasteboard = pasteboard;
    info.draggingDestinationWindow = outline.window;
    info.draggingSourceOperationMask = NSDragOperationMove;
    const NSDragOperation operation = [source outlineView:outline validateDrop:(id<NSDraggingInfo>)info
                                             proposedItem:target proposedChildIndex:NSOutlineViewDropOnItemIndex];
    bool accepted = false;
    if (operation != NSDragOperationNone) {
      accepted = [source outlineView:outline acceptDrop:(id<NSDraggingInfo>)info item:target
                          childIndex:NSOutlineViewDropOnItemIndex];
    }
    if ([source respondsToSelector:@selector(outlineView:draggingSession:endedAtPoint:operation:)]) {
      [source outlineView:outline draggingSession:session endedAtPoint:[outline.window convertPointToScreen:over]
                operation:accepted ? NSDragOperationMove : NSDragOperationNone];
    }
    [pasteboard releaseGlobally];
    outcome = accepted ? QStringLiteral("moved") : QStringLiteral("refused");
  }
  qInfo().noquote() << QStringLiteral("Native test: row drag %1 onto %2: %3").arg(from, to, outcome);
  return true;
}

bool focusedIn(const QString& identifier, const QRegularExpression* editor, QString& detail) {
  NSWindow* window = targetWindow();
  if (!active() || window == nil) return false;
  NSView* root = window.contentView.superview != nil ? window.contentView.superview : window.contentView;
  NSView* view = identifiedView(root, identifier);
  NSResponder* responder = window.firstResponder;
  NSTextView* fieldEditor = [responder isKindOfClass:NSTextView.class] && ((NSTextView*)responder).fieldEditor
                                ? (NSTextView*)responder : nil;
  NSView* focus = [responder isKindOfClass:NSView.class] ? (NSView*)responder : nil;
  // A field editor stands for the text field it edits.
  id edited = fieldEditor.delegate;
  if (fieldEditor != nil && [edited isKindOfClass:NSView.class]) focus = (NSView*)edited;
  detail = QStringLiteral("first responder %1 '%2'%3")
               .arg(qt(NSStringFromClass(responder.class)), qt(focus.accessibilityIdentifier),
                    fieldEditor != nil ? QStringLiteral(" editing \"%1\"").arg(qt(fieldEditor.string)) : QString());
  const bool inside = view != nil && focus != nil && (focus == view || [focus isDescendantOf:view]) && viewVisible(focus);
  if (editor == nullptr) return inside && fieldEditor == nil;
  return inside && fieldEditor != nil && editor->match(qt(fieldEditor.string)).hasMatch();
}

bool selectedRow(const QString& outlineIdentifier, const QString& row, QString& detail) {
  bool counted = false;
  const int expected = row.toInt(&counted);
  if (counted && expected >= 0) {
    NSView* found = findControl(outlineIdentifier);
    if ([found isKindOfClass:NSScrollView.class]) found = ((NSScrollView*)found).documentView;
    if (![found isKindOfClass:NSTableView.class] || !viewVisible(found)) {
      detail = QStringLiteral("no visible native table '%1'").arg(outlineIdentifier);
      return false;
    }
    NSTableView* table = (NSTableView*)found;
    const NSInteger shared = [table respondsToSelector:@selector(presenterSelectedRows)]
        ? [(id<MITCADNativeModelSelectionTest>)table presenterSelectedRows] : -1;
    detail = QStringLiteral("table '%1': native selected %2/%3 rows, exact live presenter selected %4")
                 .arg(outlineIdentifier).arg(table.selectedRowIndexes.count).arg(table.numberOfRows)
                 .arg(shared);
    return shared == expected && table.selectedRowIndexes.count == NSUInteger(expected);
  }
  NSWindow* window = documentWindow();
  if (!active() || window == nil) return false;
  NSView* root = window.contentView.superview != nil ? window.contentView.superview : window.contentView;
  NSView* found = identifiedView(root, outlineIdentifier);
  if ([found isKindOfClass:NSScrollView.class]) found = ((NSScrollView*)found).documentView;
  if (![found isKindOfClass:NSOutlineView.class]) {
    detail = QStringLiteral("no native outline");
    return false;
  }
  NSOutlineView* outline = (NSOutlineView*)found;
  // The hidden presenter's selected rows, by their stable identifiers.
  QStringList shared;
  for (QWidget* widget : QApplication::allWidgets()) {
    auto* view = qobject_cast<QAbstractItemView*>(widget);
    if (view == nullptr || view->objectName() != outlineIdentifier || view->selectionModel() == nullptr) continue;
    for (const QModelIndex& index : view->selectionModel()->selectedIndexes()) {
      if (index.column() == 0) shared << index.data(NativeControlIdentifierRole).toString();
    }
  }
  QStringList native;
  for (NSUInteger index = outline.selectedRowIndexes.firstIndex; index != NSNotFound;
       index = [outline.selectedRowIndexes indexGreaterThanIndex:index]) {
    const NSInteger selected = NSInteger(index);
    NSView* cell = [outline viewAtColumn:0 row:selected makeIfNecessary:YES];
    QString name = qt(cell.accessibilityIdentifier);
    // In sight within the bounded viewport.
    if (!NSContainsRect(NSInsetRect(outline.visibleRect, -1, -1), [outline rectOfRow:selected])) {
      name += QStringLiteral(" (out of sight)");
    }
    native << name;
  }
  const QString nativeText = native.isEmpty() ? QStringLiteral("-") : native.join(QStringLiteral(", "));
  const QString sharedText = shared.isEmpty() ? QStringLiteral("-") : shared.join(QStringLiteral(", "));
  detail = QStringLiteral("native %1, presenter %2").arg(nativeText, sharedText);
  // "*": whichever row the presenter selected, also natively and in sight.
  if (row == QStringLiteral("*")) return nativeText == sharedText;
  return nativeText == row && sharedText == row;
}

// mitcad#815: the selection card's Create actions and the command forms'
// compact rows and actions, checked as actual AppKit bounds and hit tests.
namespace {

// The window's hit test at the view's centre reaches it: a real pointer
// target, not covered by another overlay or scrolled away.
bool reachable(NSView* view) {
  NSView* content = view.window.contentView;
  if (content == nil) return false;
  const NSPoint centre = [view convertPoint:NSMakePoint(NSMidX(view.bounds), NSMidY(view.bounds))
                                     toView:content.superview];
  NSView* hit = [content hitTest:centre];
  return hit == view || [hit isDescendantOf:view];
}

QString describeRect(NSRect rect) {
  return QStringLiteral("%1,%2 %3x%4").arg(rect.origin.x).arg(rect.origin.y).arg(rect.size.width).arg(rect.size.height);
}

QString describeIn(NSView* view, NSView* space) {
  return view == nil ? QStringLiteral("missing") : describeRect([view convertRect:view.bounds toView:space]);
}

// The view lies whole inside `area` of `space`.
bool wholeIn(NSView* view, NSView* space, NSRect area) {
  const NSRect rect = [view convertRect:view.bounds toView:space];
  return rect.size.width > 1 && rect.size.height > 1 && NSContainsRect(NSInsetRect(area, -0.5, -0.5), rect);
}

void collectViews(NSView* root, const std::function<bool(NSView*)>& matches, NSMutableArray<NSView*>* found) {
  if (root == nil) return;
  if (matches(root)) [found addObject:root];
  for (NSView* child in root.subviews) collectViews(child, matches, found);
}

} // namespace

bool selectionCard(int expected, QString& error) {
  NSView* content = documentWindow().contentView;
  [content layoutSubtreeIfNeeded];
  NSView* card = identifiedView(content, QStringLiteral("native.inspectorCard"));
  NSView* panel = identifiedView(card, QStringLiteral("selectionCreatePanel"));
  if (!viewVisible(card) || !viewVisible(panel)) {
    error = QStringLiteral("the selection card does not show its Create row");
    return false;
  }
  NSScrollView* scroll = panel.enclosingScrollView;
  NSView* document = scroll.documentView;
  if (scroll == nil || document == nil) {
    error = QStringLiteral("the selection card has no inspector viewport");
    return false;
  }
  // The card fits its actual content: nothing to scroll to reach the row.
  const NSRect viewport = scroll.contentView.bounds;
  if (NSHeight(document.frame) > NSHeight(viewport) + 1 || NSMinY(viewport) > 1) {
    error = QStringLiteral("the selection card needs scrolling: content %1, viewport %2 points")
                .arg(NSHeight(document.frame)).arg(NSHeight(viewport));
    return false;
  }
  const NSRect visible = [scroll.contentView convertRect:viewport toView:card];
  QStringList names;
  for (int index = 0; index < 4; ++index) {
    NSView* view = identifiedView(card, QStringLiteral("selectionCreate%1").arg(index));
    if (view == nil || view.hiddenOrHasHiddenAncestor) continue;
    if (![view isKindOfClass:NSButton.class]) {
      error = QStringLiteral("Create action %1 is not an AppKit button").arg(index);
      return false;
    }
    NSButton* tile = (NSButton*)view;
    const QString name = qt(tile.accessibilityLabel);
    if (!wholeIn(tile, card, card.bounds) || !wholeIn(tile, card, visible)) {
      error = QStringLiteral("Create action %1 (%2) is clipped: %3 in a card of %4 x %5")
                  .arg(index)
                  .arg(name, describeIn(tile, card))
                  .arg(card.bounds.size.width)
                  .arg(card.bounds.size.height);
      return false;
    }
    // Its label is whole, as the shared buttons' two lines: not elided.
    NSView* text = findView(tile, [](NSView* child) { return [child isKindOfClass:NSTextField.class]; });
    if (![text isKindOfClass:NSTextField.class] || qt(((NSTextField*)text).stringValue).trimmed().isEmpty()) {
      error = QStringLiteral("Create action %1 (%2) has no visible label").arg(index).arg(name);
      return false;
    }
    NSTextField* caption = (NSTextField*)text;
    const NSSize needed = [caption.cell cellSizeForBounds:NSMakeRect(0, 0, CGFLOAT_MAX, CGFLOAT_MAX)];
    if (caption.bounds.size.width + 0.5 < needed.width || caption.bounds.size.height + 0.5 < needed.height) {
      error = QStringLiteral("Create action %1 (%2) elides its label: %3 x %4 of %5 x %6 points")
                  .arg(index)
                  .arg(name)
                  .arg(caption.bounds.size.width)
                  .arg(caption.bounds.size.height)
                  .arg(needed.width)
                  .arg(needed.height);
      return false;
    }
    if (!reachable(tile)) {
      error = QStringLiteral("Create action %1 (%2) is not a pointer target above the canvas").arg(index).arg(name);
      return false;
    }
    names << name;
  }
  if (names.size() != expected) {
    error = QStringLiteral("the selection card shows %1 Create actions, expected %2: %3")
                .arg(names.size())
                .arg(expected)
                .arg(names.join(QStringLiteral(" | ")));
    return false;
  }
  qInfo().noquote() << QStringLiteral("Native selection card: %1 whole, reachable Create actions (%2) in %3 x %4 points")
                           .arg(names.size())
                           .arg(names.join(QStringLiteral(" | ")))
                           .arg(card.bounds.size.width)
                           .arg(card.bounds.size.height);
  return true;
}

bool commandLayout(const QStringList& required, QString& error) {
  NSView* content = documentWindow().contentView;
  [content layoutSubtreeIfNeeded];
  NSView* card = identifiedView(content, QStringLiteral("native.inspectorCard"));
  NSView* root = identifiedView(card, QStringLiteral("commandPanel"));
  NSView* footer = identifiedView(card, QStringLiteral("commandButtons"));
  NSScrollView* scroll = root.enclosingScrollView;
  if (!viewVisible(card) || root == nil || scroll == nil || !viewVisible(footer)) {
    error = QStringLiteral("no shown command form with its OK and Cancel under its inputs");
    return false;
  }
  if (!viewVisible(identifiedView(root, QStringLiteral("commandIcon")))) {
    error = QStringLiteral("the command form's title row lacks the command's CAD artwork");
    return false;
  }
  const NSRect inputs = [scroll convertRect:scroll.bounds toView:card];
  const NSRect visible = [scroll.contentView convertRect:scroll.contentView.bounds toView:card];
  // OK and Cancel: whole, under the scrolling inputs, real pointer targets.
  for (const QString& identifier : {QStringLiteral("commandCancel"), QStringLiteral("commandOk")}) {
    NSView* button = identifiedView(footer, identifier);
    if (!viewVisible(button) || !wholeIn(button, card, card.bounds) ||
        NSIntersectsRect([button convertRect:button.bounds toView:card], inputs) || !reachable(button)) {
      error = QStringLiteral("'%1' is clipped, under the inputs or not a pointer target: %2 in a card of %3 x %4, inputs %5")
                  .arg(identifier, describeIn(button, card))
                  .arg(card.bounds.size.width)
                  .arg(card.bounds.size.height)
                  .arg(describeRect(inputs));
      return false;
    }
  }
  // Captions beside their controls in aligned columns, as the shared form's
  // rows, not above them; a list row's form has columns of its own.
  NSMutableArray<NSView*>* captions = [NSMutableArray array];
  collectViews(root, [](NSView* view) { return [view.accessibilityIdentifier hasPrefix:@"caption_"]; }, captions);
  CGFloat captionLeft = -1;
  CGFloat fieldLeft = -1;
  int rows = 0;
  for (NSView* caption in captions) {
    if (caption.hiddenOrHasHiddenAncestor) continue;
    const QString key = qt(caption.accessibilityIdentifier).mid(8);
    NSView* field = identifiedView(root, QStringLiteral("row_") + key);
    if (field == nil || field.hiddenOrHasHiddenAncestor) {
      error = QStringLiteral("the caption of '%1' has no shown controls").arg(key);
      return false;
    }
    const NSRect left = [caption convertRect:caption.bounds toView:root];
    const NSRect right = [field convertRect:field.bounds toView:root];
    if (NSMaxX(left) > NSMinX(right) + 0.5 || NSMidY(left) < NSMinY(right) - 1 || NSMidY(left) > NSMaxY(right) + 1) {
      error = QStringLiteral("the caption of '%1' is not beside its controls: caption %2, controls %3")
                  .arg(key, describeRect(left), describeRect(right));
      return false;
    }
    if (key.contains(QLatin1Char('.'))) continue;
    if (captionLeft < 0) {
      captionLeft = NSMinX(left);
      fieldLeft = NSMinX(right);
    } else if (std::abs(NSMinX(left) - captionLeft) > 1 || std::abs(NSMinX(right) - fieldLeft) > 1) {
      error = QStringLiteral("'%1' is not in the form's aligned columns: caption %2, controls %3")
                  .arg(key, describeRect(left), describeRect(right));
      return false;
    }
    ++rows;
  }
  if (rows == 0) {
    error = QStringLiteral("the command form has no captioned rows");
    return false;
  }
  // Compact clear and flip controls, as the shared panel's: icons, not text.
  NSMutableArray<NSView*>* compact = [NSMutableArray array];
  collectViews(root, [](NSView* view) {
    return [view isKindOfClass:NSButton.class] &&
           ([view.identifier hasPrefix:@"clear|"] || [view.identifier hasPrefix:@"check|"]);
  }, compact);
  for (NSView* view in compact) {
    NSButton* button = (NSButton*)view;
    const bool clear = [button.identifier hasPrefix:@"clear|"];
    if (button.hiddenOrHasHiddenAncestor) continue;
    if (button.title.length > 0 || (clear && (button.image == nil || NSWidth(button.bounds) > 28))) {
      error = QStringLiteral("'%1' is a text button (%2), not a compact control")
                  .arg(qt(button.identifier), qt(button.title));
      return false;
    }
  }
  // What must be in view as the form opens: whole without scrolling and a
  // real pointer target (Local bodies, a value's fx tools).
  for (const QString& identifier : required) {
    NSView* view = identifiedView(root, identifier);
    if (!viewVisible(view) || !wholeIn(view, card, visible) || !reachable(view)) {
      error = QStringLiteral("'%1' needs scrolling, is clipped or is not a pointer target: %2, visible inputs %3")
                  .arg(identifier, describeIn(view, card), describeRect(visible));
      return false;
    }
  }
  qInfo().noquote() << QStringLiteral("Native command layout: %1 captioned rows in aligned columns, OK and Cancel "
                                      "whole under the inputs, %2 required controls in view, card %3 x %4 points")
                           .arg(rows)
                           .arg(required.size())
                           .arg(card.bounds.size.width)
                           .arg(card.bounds.size.height);
  return true;
}

} // namespace mitcad::mac::test

// ---------------------------------------------------------------------------
// Native tile grids, group lists and utility panels (mitcad#813, mitcad#819).
// Actions use the views' own AppKit input paths; checks read what is shown.

#include <QWindow>

#include "platform/macos/NativeCollection.hpp"

namespace mitcad::mac::test {
namespace {

MitcadNativeTileCollection* tileGrid(const QString& identifier) {
  NSView* view = findControl(identifier);
  if ([view isKindOfClass:NSScrollView.class]) view = ((NSScrollView*)view).documentView;
  return [view isKindOfClass:MitcadNativeTileCollection.class] ? (MitcadNativeTileCollection*)view : nil;
}

// A tile's view, laid out and scrolled into sight (AppKit makes only the
// tiles in sight).
MitcadNativeTileView* tileView(MitcadNativeTileCollection* grid, NSInteger tile) {
  NSIndexPath* path = [NSIndexPath indexPathForItem:tile inSection:0];
  NSCollectionViewLayoutAttributes* attributes = [grid layoutAttributesForItemAtIndexPath:path];
  if (attributes != nil) [grid scrollRectToVisible:attributes.frame];
  [grid layoutSubtreeIfNeeded];
  NSCollectionViewItem* item = [grid itemAtIndexPath:path];
  return [item.view isKindOfClass:MitcadNativeTileView.class] ? (MitcadNativeTileView*)item.view : nil;
}

// The tile whose key contains `name` or whose caption is `name`.
NSInteger findTile(MitcadNativeTileCollection* grid, const QString& name) {
  const NSInteger count = [grid numberOfItemsInSection:0];
  for (NSInteger tile = 0; tile < count; ++tile) {
    MitcadNativeTileView* view = tileView(grid, tile);
    if (view != nil && (qt(view->key).contains(name) || qt(view->caption.stringValue) == name)) return tile;
  }
  return -1;
}

MitcadNativeGroupOutline* groupList(const QString& identifier) {
  NSView* view = findControl(identifier);
  if ([view isKindOfClass:NSScrollView.class]) view = ((NSScrollView*)view).documentView;
  return [view isKindOfClass:MitcadNativeGroupOutline.class] ? (MitcadNativeGroupOutline*)view : nil;
}

NSInteger findGroup(NSOutlineView* outline, const QString& name) {
  for (NSInteger row = 0; row < outline.numberOfRows; ++row) {
    NSView* cell = [outline viewAtColumn:0 row:row makeIfNecessary:YES];
    if ([cell isKindOfClass:NSTableCellView.class] && qt(((NSTableCellView*)cell).textField.stringValue) == name) return row;
  }
  return -1;
}

CGFloat lumaOf(NSColor* colour) {
  NSColor* rgb = [colour colorUsingColorSpace:NSColorSpace.sRGBColorSpace];
  if (rgb == nil) return 0;
  return 0.2126 * rgb.redComponent + 0.7152 * rgb.greenComponent + 0.0722 * rgb.blueComponent;
}

// A picture with visible content, not a flat placeholder.
bool pictured(NSImage* image) {
  CGImageRef picture = image != nil ? [image CGImageForProposedRect:nullptr context:nil hints:nil] : nullptr;
  if (picture == nullptr) return false;
  NSBitmapImageRep* bitmap = [[NSBitmapImageRep alloc] initWithCGImage:picture];
  CGFloat darkest = 1;
  CGFloat lightest = 0;
  const NSInteger step = std::max<NSInteger>(1, bitmap.pixelsWide / 32);
  for (NSInteger y = 0; y < bitmap.pixelsHigh; y += step) {
    for (NSInteger x = 0; x < bitmap.pixelsWide; x += step) {
      NSColor* colour = [bitmap colorAtX:x y:y];
      if (colour == nil || colour.alphaComponent < 0.5) continue;
      const CGFloat luma = lumaOf(colour);
      darkest = std::min(darkest, luma);
      lightest = std::max(lightest, luma);
    }
  }
  return lightest - darkest > 0.1;
}

NSWindow* utilityPanel(const QRegularExpression& title) {
  for (NSWindow* window in nativeWindows()) {
    if ([window isKindOfClass:MitcadNativeUtilityPanel.class] && window.visible && title.match(qt(window.title)).hasMatch()) {
      return window;
    }
  }
  return nil;
}

// Controls outside lists and grids lie within the window, without
// scrolling its body (NativeWindowFit.hpp), and keep a usable size; long
// wrapped texts (a part's details) a readable width.
bool usableControls(NSWindow* window, QString& detail) {
  NSView* content = window.contentView;
  const NSRect bounds = NSInsetRect(content.bounds, -1, -1);
  bool usable = true;
  findView(content, [&](NSView* view) {
    NSScrollView* scroll = view.enclosingScrollView;
    const bool listed = scroll != nil && ![scroll.accessibilityIdentifier isEqualToString:@"native.secondary.body"];
    if (!usable || [view isHiddenOrHasHiddenAncestor] || listed || ![view isKindOfClass:NSControl.class]) {
      return false;
    }
    const NSRect frame = [view convertRect:view.bounds toView:content];
    const bool text = [view isKindOfClass:NSTextField.class];
    const bool editable = text && ((NSTextField*)view).editable;
    const bool wrapped = text && !editable && ((NSTextField*)view).cell.wraps && ((NSTextField*)view).stringValue.length > 40;
    const CGFloat least = editable ? 120 : wrapped ? 220 : [view isKindOfClass:NSSlider.class] ? 80 : 12;
    if (!NSContainsRect(bounds, frame) || NSWidth(frame) < least || NSHeight(frame) < 8) {
      detail = QStringLiteral("control '%1' (%2) at %3 is clipped or narrower than %4")
                   .arg(qt(view.accessibilityIdentifier), qt(NSStringFromClass(view.class)), qt(NSStringFromRect(frame)))
                   .arg(least);
      usable = false;
    }
    return false;
  });
  return usable;
}

} // namespace

bool tileAction(const QStringList& args, QString& error) {
  const QString action = args.value(0);
  if (action == QStringLiteral("resize")) {
    NSWindow* window = targetWindow();
    const int width = args.value(1).toInt();
    const int height = args.value(2).toInt();
    if (window == nil || window == documentWindow() || width < 300 || height < 200) {
      error = QStringLiteral("native-tiles resize needs a focused native dialog and its size");
      return false;
    }
    [window setContentSize:NSMakeSize(width, height)];
    [window.contentView layoutSubtreeIfNeeded];
    return true;
  }
  if (action == QStringLiteral("press") || action == QStringLiteral("slider")) {
    NSView* view = findControl(args.value(1));
    if (action == QStringLiteral("slider")) {
      if (![view isKindOfClass:NSSlider.class] || !((NSSlider*)view).enabled) {
        error = QStringLiteral("no enabled native slider '%1'").arg(args.value(1));
        return false;
      }
      NSSlider* slider = (NSSlider*)view;
      slider.doubleValue = args.value(2).toDouble();
      [slider sendAction:slider.action to:slider.target];
      return true;
    }
    if (![view isKindOfClass:NSButton.class] || !((NSButton*)view).enabled) {
      error = QStringLiteral("no enabled native button '%1'").arg(args.value(1));
      return false;
    }
    // AppKit's own click on the button, without making its window key.
    [(NSButton*)view performClick:nil];
    return true;
  }
  if (action == QStringLiteral("group") || action == QStringLiteral("group-star") || action == QStringLiteral("fold")) {
    MitcadNativeGroupOutline* outline = groupList(args.value(1));
    const NSInteger row = outline != nil ? findGroup(outline, args.value(2)) : -1;
    if (row < 0) {
      error = QStringLiteral("no group '%1' in the native group list '%2'").arg(args.value(2), args.value(1));
      return false;
    }
    if (action == QStringLiteral("group")) {
      [outline.window makeFirstResponder:outline];
      [outline selectRowIndexes:[NSIndexSet indexSetWithIndex:NSUInteger(row)] byExtendingSelection:NO];
      [outline scrollRowToVisible:row];
      return true;
    }
    if (action == QStringLiteral("fold")) {
      id item = [outline itemAtRow:row];
      if (args.value(3) == QStringLiteral("closed")) [outline collapseItem:item];
      else [outline expandItem:item];
      return true;
    }
    NSView* cell = [outline viewAtColumn:0 row:row makeIfNecessary:YES];
    NSView* star = findView(cell, [](NSView* view) { return bool([view isKindOfClass:NSButton.class]); });
    if (star == nil) {
      error = QStringLiteral("native group '%1' has no star").arg(args.value(2));
      return false;
    }
    [(NSButton*)star performClick:nil];
    return true;
  }
  MitcadNativeTileCollection* grid = tileGrid(args.value(1));
  const NSInteger tile = grid != nil ? findTile(grid, args.value(2)) : -1;
  MitcadNativeTileView* view = tile >= 0 ? tileView(grid, tile) : nil;
  if (view == nil) {
    error = QStringLiteral("no tile '%1' in the native grid '%2'").arg(args.value(2), args.value(1));
    return false;
  }
  if (action == QStringLiteral("click") || action == QStringLiteral("dblclick")) {
    // A press on the tile, as the pointer gives it: the tile forwards it to
    // its grid, which chooses (a second press activates).
    const NSInteger presses = action == QStringLiteral("dblclick") ? 2 : 1;
    const NSPoint point = [view convertPoint:NSMakePoint(NSMidX(view.bounds), NSMidY(view.bounds)) toView:nil];
    for (NSInteger clicks = 1; clicks <= presses; ++clicks) {
      NSEvent* event = [NSEvent mouseEventWithType:NSEventTypeLeftMouseDown location:point modifierFlags:0
          timestamp:NSProcessInfo.processInfo.systemUptime windowNumber:view.window.windowNumber
          context:nil eventNumber:0 clickCount:clicks pressure:1];
      [view mouseDown:event];
    }
    return true;
  }
  if (action == QStringLiteral("star")) {
    if (view->star.hidden) {
      error = QStringLiteral("native tile '%1' has no star").arg(args.value(2));
      return false;
    }
    [view->star performClick:nil];
    return true;
  }
  if (action == QStringLiteral("hover")) {
    const bool enter = args.value(3) != QStringLiteral("leave");
    const NSPoint point = [view convertPoint:NSMakePoint(NSMidX(view.bounds), NSMidY(view.bounds)) toView:nil];
    NSEvent* event = [NSEvent enterExitEventWithType:enter ? NSEventTypeMouseEntered : NSEventTypeMouseExited
                                            location:point modifierFlags:0 timestamp:NSProcessInfo.processInfo.systemUptime
                                        windowNumber:view.window.windowNumber context:nil eventNumber:0 trackingNumber:0
                                            userData:nullptr];
    if (enter) [view mouseEntered:event];
    else [view mouseExited:event];
    return true;
  }
  error = QStringLiteral("unknown native-tiles action '%1'").arg(action);
  return false;
}

bool tileCheck(const QStringList& args, QString& detail) {
  const QString check = args.value(0);
  if (check == QStringLiteral("hover")) {
    bool shown = false;
    for (NSWindow* window in NSApp.windows) {
      shown = shown || ([window.identifier isEqualToString:@"native.tileHover"] && window.visible);
    }
    detail = QStringLiteral("hover card shown=%1").arg(shown);
    return shown == (args.value(1) == QStringLiteral("true"));
  }
  if (check == QStringLiteral("focus")) {
    // The utility panel in front, the keyboard with the document (its
    // command form or canvas), as the Qt picker shows without activating.
    const QRegularExpression title(args.value(1));
    NSWindow* panel = utilityPanel(title);
    NSWindow* document = documentWindow();
    NSWindow* key = NSApp.keyWindow;
    detail = QStringLiteral("panel=%1 floating=%2 panelKey=%3 key=%4 document=%5 active=%6")
                 .arg(panel != nil).arg(panel != nil && panel.level >= NSFloatingWindowLevel)
                 .arg(panel != nil && panel.keyWindow).arg(qt(key.title), qt(document.title)).arg(bool(NSApp.active));
    return panel != nil && panel.level >= NSFloatingWindowLevel && !panel.keyWindow && key == document;
  }
  if (check == QStringLiteral("document")) {
    // The keyboard returned to the document once a panel closed. A live
    // command field editor is valid recovery too; a detached panel editor
    // or a stale field must not pass just because the document is key.
    NSWindow* key = NSApp.keyWindow;
    NSResponder* responder = key.firstResponder;
    NSTextView* editor = [responder isKindOfClass:NSTextView.class] && ((NSTextView*)responder).isFieldEditor
        ? (NSTextView*)responder : nil;
    NSTextField* field = [editor.delegate isKindOfClass:NSTextField.class] ? (NSTextField*)editor.delegate : nil;
    const bool liveEditor = editor == nil || (field != nil && field.window == key && field.currentEditor == editor &&
        [field isDescendantOf:key.contentView] && !field.isHiddenOrHasHiddenAncestor);
    detail = QStringLiteral("key=%1 active=%2 responder=%3 field=%4 field-window=%5 live-editor=%6")
        .arg(qt(key.title)).arg(bool(NSApp.active)).arg(qt(NSStringFromClass(responder.class)), qt(field.accessibilityIdentifier))
        .arg(field != nil ? qlonglong(field.window.windowNumber) : -1).arg(liveEditor);
    qInfo().noquote() << QStringLiteral("Native picker document focus: %1").arg(detail);
    return key != nil && key == documentWindow() && liveEditor;
  }
  if (check == QStringLiteral("qt-hidden")) {
    // The presenter is never on the screen: no Qt window takes input.
    for (QWidget* widget : QApplication::topLevelWidgets()) {
      if (widget->objectName() != args.value(1)) continue;
      const bool offscreen = widget->testAttribute(Qt::WA_DontShowOnScreen);
      const bool exposed = widget->windowHandle() != nullptr && widget->windowHandle()->isExposed();
      if (!offscreen || exposed) {
        detail = QStringLiteral("Qt presenter '%1' offscreen=%2 exposed=%3").arg(args.value(1)).arg(offscreen).arg(exposed);
        return false;
      }
    }
    const QRegularExpression title(args.value(2));
    for (NSWindow* window in NSApp.windows) {
      const QString kind = qt(NSStringFromClass(window.class));
      if (window.visible && (kind.startsWith(QStringLiteral("QNS")) || kind.startsWith(QStringLiteral("QCocoa"))) &&
          title.match(qt(window.title)).hasMatch()) {
        detail = QStringLiteral("a visible Qt window '%1' (%2)").arg(qt(window.title), kind);
        return false;
      }
    }
    detail.clear();
    return true;
  }
  if (check == QStringLiteral("groups")) {
    // A source list wide enough for its names, with at least this many rows.
    MitcadNativeGroupOutline* outline = groupList(args.value(1));
    NSScrollView* scroll = outline.enclosingScrollView;
    if (outline == nil || scroll == nil) {
      detail = QStringLiteral("no native group list '%1'").arg(args.value(1));
      return false;
    }
    const NSRect frame = [scroll convertRect:scroll.bounds toView:nil];
    detail = QStringLiteral("group list %1x%2 with %3 rows").arg(NSWidth(frame)).arg(NSHeight(frame)).arg(outline.numberOfRows);
    if (NSWidth(frame) < 150 || NSHeight(frame) < 100 || outline.numberOfRows < args.value(2).toInt()) return false;
    for (NSInteger row = 0; row < outline.numberOfRows; ++row) {
      NSView* cell = [outline viewAtColumn:0 row:row makeIfNecessary:YES];
      NSTextField* text = [cell isKindOfClass:NSTableCellView.class] ? ((NSTableCellView*)cell).textField : nil;
      if (text == nil || text.stringValue.length == 0 ||
          NSWidth(text.frame) < std::min<CGFloat>(80, text.intrinsicContentSize.width)) {
        detail += QStringLiteral("; row %1 is not readable").arg(row);
        return false;
      }
    }
    return true;
  }
  MitcadNativeTileCollection* grid = tileGrid(args.value(1));
  if (grid == nil) {
    detail = QStringLiteral("no native tile grid '%1'").arg(args.value(1));
    return false;
  }
  const NSInteger count = [grid numberOfItemsInSection:0];
  if (check == QStringLiteral("count")) {
    detail = QStringLiteral("%1 tiles").arg(count);
    return count == args.value(2).toInt();
  }
  if (check == QStringLiteral("pictures")) {
    int pictures = 0;
    for (NSInteger tile = 0; tile < count; ++tile) {
      MitcadNativeTileView* view = tileView(grid, tile);
      if (view != nil && view->hasPicture && pictured(view->picture.image) && NSWidth(view->picture.frame) >= 16) ++pictures;
    }
    detail = QStringLiteral("%1 of %2 tiles have pictures").arg(pictures).arg(count);
    return pictures >= args.value(2).toInt();
  }
  if (check == QStringLiteral("captions")) {
    for (NSInteger tile = 0; tile < count; ++tile) {
      MitcadNativeTileView* view = tileView(grid, tile);
      if (view == nil) {
        detail = QStringLiteral("tile %1 is not shown").arg(tile);
        return false;
      }
      NSTextField* caption = view->caption;
      const BOOL chosen = view->chosen;
      __block CGFloat contrast = 0;
      [view.effectiveAppearance performAsCurrentDrawingAppearance:^{
        NSColor* behind = chosen ? NSColor.selectedContentBackgroundColor : NSColor.windowBackgroundColor;
        contrast = std::fabs(lumaOf(caption.textColor) - lumaOf(behind));
      }];
      const NSRect frame = caption.frame;
      detail = QStringLiteral("tile %1 caption '%2' %3 font %4 contrast %5")
                   .arg(tile).arg(qt(caption.stringValue), qt(NSStringFromRect(frame))).arg(caption.font.pointSize).arg(contrast);
      if (caption.hidden || caption.stringValue.length == 0 || caption.font.pointSize < 11 || NSHeight(frame) < 12 ||
          NSWidth(frame) < std::min<CGFloat>(60, caption.intrinsicContentSize.width) ||
          !NSContainsRect(NSInsetRect(view.bounds, -1, -1), frame) || contrast < 0.35) {
        return false;
      }
    }
    return true;
  }
  if (check == QStringLiteral("current")) {
    const NSInteger tile = findTile(grid, args.value(2));
    NSIndexPath* selected = grid.selectionIndexPaths.anyObject;
    detail = QStringLiteral("tile %1, selected %2").arg(tile).arg(selected != nil ? selected.item : NSInteger(-1));
    return tile >= 0 && selected != nil && selected.item == tile;
  }
  if (check == QStringLiteral("star")) {
    const NSInteger tile = findTile(grid, args.value(2));
    MitcadNativeTileView* view = tile >= 0 ? tileView(grid, tile) : nil;
    detail = QStringLiteral("tile %1 starred=%2").arg(tile).arg(view != nil && view->starred);
    return view != nil && !view->star.hidden && bool(view->starred) == (args.value(3) == QStringLiteral("on"));
  }
  if (check == QStringLiteral("tile-size")) {
    MitcadNativeTileView* view = count > 0 ? tileView(grid, 0) : nil;
    const CGFloat side = args.value(2).toDouble();
    detail = QStringLiteral("picture %1, tile %2").arg(view != nil ? qt(NSStringFromRect(view->picture.frame)) : QString(),
                                                      view != nil ? qt(NSStringFromRect(view.frame)) : QString());
    return view != nil && std::fabs(view->side - side) < 0.5 && std::fabs(NSWidth(view->picture.frame) - side) < 0.5 &&
           NSWidth(view.frame) >= side;
  }
  if (check == QStringLiteral("layout")) {
    // Usable bounds: at least one whole tile in sight, readable groups and
    // every control of the window inside it.
    NSScrollView* scroll = grid.enclosingScrollView;
    NSWindow* window = grid.window;
    const NSSize item = ((NSCollectionViewFlowLayout*)grid.collectionViewLayout).itemSize;
    const NSRect frame = scroll != nil ? [scroll convertRect:scroll.bounds toView:window.contentView] : NSZeroRect;
    detail = QStringLiteral("grid %1, tile %2x%3, window %4")
                 .arg(qt(NSStringFromRect(frame))).arg(item.width).arg(item.height)
                 .arg(qt(NSStringFromRect(window.contentView.bounds)));
    if (window == nil || NSWidth(frame) < item.width + 16 || NSHeight(frame) < item.height ||
        !NSContainsRect(NSInsetRect(window.contentView.bounds, -1, -1), frame)) {
      return false;
    }
    NSView* list = findView(window.contentView, [](NSView* view) { return bool([view isKindOfClass:MitcadNativeGroupOutline.class]); });
    if (list != nil && NSWidth(list.enclosingScrollView.frame) < 150) {
      detail += QStringLiteral("; the group list is %1 wide").arg(NSWidth(list.enclosingScrollView.frame));
      return false;
    }
    QString controls;
    if (!usableControls(window, controls)) {
      detail += QStringLiteral("; ") + controls;
      return false;
    }
    return true;
  }
  detail = QStringLiteral("unknown native tile check '%1'").arg(check);
  return false;
}

} // namespace mitcad::mac::test

// --- Shared layout bounds, real pointer input and dialog button order
// (mitcad#812, mitcad#817).

#include <QDialogButtonBox>
#include <QFormLayout>
#include <QGridLayout>
#include <QHBoxLayout>
#include <QSlider>

#include "platform/macos/NativeLayout.hpp"

namespace mitcad::mac::test {
namespace {

NSRect frameIn(NSView* view, NSView* reference) { return [view convertRect:view.bounds toView:reference]; }

QString describeLayoutRect(const NSRect& rect) {
  return QStringLiteral("x=%1 y=%2 width=%3 height=%4")
      .arg(rect.origin.x, 0, 'f', 1).arg(rect.origin.y, 0, 'f', 1)
      .arg(rect.size.width, 0, 'f', 1).arg(rect.size.height, 0, 'f', 1);
}

bool sameRow(const NSRect& a, const NSRect& b) { return NSMaxY(a) > NSMinY(b) && NSMinY(a) < NSMaxY(b); }

NSView* buttonTitled(NSView* root, const QRegularExpression& expression, bool enabledOnly) {
  return findView(root, [&](NSView* view) {
    if (![view isKindOfClass:NSButton.class] || !viewVisible(view)) return false;
    if (enabledOnly && !((NSButton*)view).enabled) return false;
    QString title = qt(((NSButton*)view).title);
    title.remove(QLatin1Char('&'));
    return expression.match(title).hasMatch();
  });
}

// Real AppKit mouse input: the window hit-tests the press and the control
// tracks the drags and release, as for input from the pointer.
bool pointerEvents(NSView* view, NSPoint from, NSPoint to, QString& error, int clickCount = 1,
                   bool centerVisible = false, const QString& identifier = {}) {
  NSWindow* window = view.window;
  if (window == nil) { error = QStringLiteral("the native pointer target has no window"); return false; }
  // Clear of a scrolled body's edges, where overlay scrollers appear.
  [view scrollRectToVisible:NSInsetRect(view.bounds, -24, -40)];
  [NSApp activate];
  [window makeKeyAndOrderFront:nil];
  [window layoutIfNeeded];
  NSView* content = window.contentView;
  [content layoutSubtreeIfNeeded];
  if (centerVisible) {
    // Preparing the window after the first press can replace its row views.
    // Resolve the same identifier after that layout too, before hit-testing;
    // a retained old cell must never receive the second press.
    NSView* live = tableCell(identifier, &error);
    if (live == nil || live.window != window) {
      if (error.isEmpty()) error = QStringLiteral("the native double-click target '%1' lost its window during layout").arg(identifier);
      return false;
    }
    if (live != view)
      qInfo().noquote() << QStringLiteral("Native pointer live row resolved after layout: %1, old-window-live=%2")
          .arg(identifier).arg(view.window == window);
    view = live;
    // A row can be taller than its short library viewport. Scroll/layout
    // first, then aim at the part actually shown, rather than a stale full
    // bounds centre hidden behind the surrounding form (mitcad#864).
    const NSRect visible = NSIntersectionRect(view.bounds, view.visibleRect);
    NSTableView* table = mac::owningTable(view);
    if (view.window != window || !viewVisible(view) || NSIsEmptyRect(visible) ||
        ([view isKindOfClass:NSControl.class] && !((NSControl*)view).enabled) ||
        (table != nil && !table.enabled)) {
      error = QStringLiteral("the native double-click target '%1' has no live visible enabled area (bounds %2, visible %3, same-window %4)")
          .arg(identifier, qt(NSStringFromRect(view.bounds)), qt(NSStringFromRect(view.visibleRect)))
          .arg(view.window == window);
      return false;
    }
    from = to = NSMakePoint(NSMidX(visible), NSMidY(visible));
    qInfo().noquote() << QStringLiteral("Native pointer visible centre: %1 bounds=%2 clipped=%3 point=%4")
        .arg(qt(view.accessibilityIdentifier), qt(NSStringFromRect(view.bounds)),
             qt(NSStringFromRect(visible)), qt(NSStringFromPoint(from)));
  }
  // A release left queued by earlier input would end this control's
  // tracking at its press.
  const NSEventMask mouse = NSEventMaskLeftMouseDown | NSEventMaskLeftMouseUp | NSEventMaskLeftMouseDragged;
  while ([NSApp nextEventMatchingMask:mouse untilDate:[NSDate distantPast] inMode:NSDefaultRunLoopMode dequeue:YES] != nil) {}
  const NSPoint start = [view convertPoint:from toView:nil];
  const NSPoint end = [view convertPoint:to toView:nil];
  NSView* hit = [content hitTest:[content.superview convertPoint:start fromView:nil]];
  bool exactTableCellHit = false;
  if (centerVisible && [view isKindOfClass:NSTableCellView.class]) {
    // Display-only cells can leave mouse handling to their table. Qualify
    // that actual hit by the exact live row/column and view identity;
    // another cell or covering view cannot satisfy this check.
    NSTableView* table = mac::owningTable(view);
    if (table != nil && hit == table && table.enabled && table.window == window) {
      const NSPoint at = [table convertPoint:start fromView:nil];
      const NSInteger row = [table rowForView:view];
      const NSInteger column = [table columnForView:view];
      exactTableCellHit = row >= 0 && column >= 0 && [table rowAtPoint:at] == row &&
          [table columnAtPoint:at] == column &&
          [table viewAtColumn:column row:row makeIfNecessary:NO] == view;
      if (exactTableCellHit)
        qInfo().noquote() << QStringLiteral("Native pointer table cell hit: %1 row=%2 column=%3 live=1")
            .arg(qt(view.accessibilityIdentifier)).arg(row).arg(column);
    }
  }
  if (hit != view && ![hit isDescendantOf:view] && !exactTableCellHit) {
    error = QStringLiteral("the native pointer target is covered by %1 (target %2 '%3', bounds %4, visible %5, local point %6)")
        .arg(qt(NSStringFromClass(hit.class)), qt(NSStringFromClass(view.class)), qt(view.accessibilityIdentifier),
             qt(NSStringFromRect(view.bounds)), qt(NSStringFromRect(view.visibleRect)), qt(NSStringFromPoint(from)));
    return false;
  }
  // Keep the second press later than the first click's queued release,
  // without pumping the next script step between a double-click's phases.
  const NSTimeInterval time = NSProcessInfo.processInfo.systemUptime + (clickCount - 1) * 0.02;
  const int steps = NSEqualPoints(start, end) ? 0 : 6;
  const bool paced = [view isKindOfClass:NSSlider.class];
  const auto postFollowup = [window, start, end, steps, clickCount](int step, NSTimeInterval timestamp) {
    const bool release = step > steps;
    const CGFloat part = release ? 1 : CGFloat(step) / CGFloat(steps);
    const NSPoint point = NSMakePoint(start.x + (end.x - start.x) * part, start.y + (end.y - start.y) * part);
    [NSApp postEvent:[NSEvent mouseEventWithType:release ? NSEventTypeLeftMouseUp : NSEventTypeLeftMouseDragged
        location:point modifierFlags:0 timestamp:timestamp windowNumber:window.windowNumber context:nil
        eventNumber:0 clickCount:clickCount pressure:release ? 0 : 1] atStart:NO];
  };
  __block int delivered = 0;
  NSTimer* trackingInput = nil;
  if (paced) {
    // Focus acquisition during mouseDown may discard already queued mouse
    // events. Real future motion arrives after that transition, while the
    // slider tracks; produce it there instead of pre-queuing future input.
    trackingInput = [NSTimer timerWithTimeInterval:0.01 repeats:YES block:^(NSTimer* timer) {
      postFollowup(++delivered, NSProcessInfo.processInfo.systemUptime);
      if (delivered > steps) [timer invalidate];
    }];
    [NSRunLoop.currentRunLoop addTimer:trackingInput forMode:NSEventTrackingRunLoopMode];
  } else {
    for (int step = 1; step <= steps + 1; ++step) postFollowup(step, time + 0.01 * step);
  }
  NSEvent* down = [NSEvent mouseEventWithType:NSEventTypeLeftMouseDown location:start modifierFlags:0
      timestamp:time windowNumber:window.windowNumber context:nil eventNumber:0 clickCount:clickCount pressure:1];
  if (paced) {
    // Retrieve this press through the application's real event queue, not
    // only sendEvent: (which can retain an older AppKit currentEvent).
    // The driver timer stays paused; subsequent slider motion still comes
    // from the tracking-mode timer, and no control state is synthesized.
    [NSApp postEvent:down atStart:YES];
    NSEvent* queued = [NSApp nextEventMatchingMask:NSEventMaskLeftMouseDown untilDate:[NSDate distantPast]
                                          inMode:NSDefaultRunLoopMode dequeue:YES];
    // AppKit can copy an event while queueing it. Attribute the press by
    // its owned window and input metadata, not its object address.
    // CoreGraphics/CGEventTypes.h defines CGEventTimestamp as uint64_t
    // nanoseconds since startup. An AppKit seconds-to-CG copy can round by
    // one such unit; compare timestamps at that representational precision.
    const auto samePress = [down, window](NSEvent* event) {
      return event != nil && event.windowNumber == window.windowNumber &&
          [NSApp windowWithWindowNumber:event.windowNumber] == window &&
          event.type == down.type && std::abs(event.timestamp - down.timestamp) * 1e9 <= 1 &&
          NSEqualPoints(event.locationInWindow, down.locationInWindow) &&
          (down.eventNumber == 0 || event.eventNumber == down.eventNumber);
    };
    const NSPoint observed = queued.locationInWindow;
    const NSPoint expected = down.locationInWindow;
    const QString fields = QStringLiteral("window=%1/%2 type=%3/%4 timestamp=%5/%6 point=%7,%8/%9,%10 number=%11/%12 marker-available=%13 timestamp-delta-ns=%14")
        .arg(qlonglong(queued.windowNumber)).arg(qlonglong(down.windowNumber))
        .arg(int(queued.type)).arg(int(down.type))
        .arg(queued.timestamp, 0, 'g', 17).arg(down.timestamp, 0, 'g', 17)
        .arg(observed.x, 0, 'g', 17).arg(observed.y, 0, 'g', 17)
        .arg(expected.x, 0, 'g', 17).arg(expected.y, 0, 'g', 17)
        .arg(qlonglong(queued.eventNumber)).arg(qlonglong(down.eventNumber)).arg(down.eventNumber != 0)
        .arg(std::abs(queued.timestamp - down.timestamp) * 1e9, 0, 'g', 17);
    if (view.window != window || !samePress(queued)) {
      [trackingInput invalidate];
      if (queued != nil) [NSApp postEvent:queued atStart:YES];
      error = QStringLiteral("native slider press queue mismatch: %1").arg(fields);
      return false;
    }
    qInfo().noquote() << QStringLiteral("Native pointer queue fields: %1").arg(fields);
    qInfo().noquote() << QStringLiteral("Native pointer queue: %1 semantic-match=1 same-object=%2 current=%3 current-matches-press=%4")
        .arg(qt(view.accessibilityIdentifier)).arg(queued == down)
        .arg(int(NSApp.currentEvent.type)).arg(samePress(NSApp.currentEvent));
    [NSApp sendEvent:queued];
  } else {
    // Ordinary presses must pass through the application's event queue,
    // too. Dispatching directly to NSWindow bypasses local monitors,
    // including a popover's outside-click dismissal. The drain below
    // retrieves and sends the press before its queued release.
    [NSApp postEvent:down atStart:YES];
  }
  if (paced)
    qInfo().noquote() << QStringLiteral("Native pointer tracking returned: %1 follow-up events=%2/%3 value=%4")
        .arg(qt(view.accessibilityIdentifier)).arg(delivered).arg(steps + 1).arg(((NSSlider*)view).doubleValue);
  // If tracking returned without consuming the sequence, finish through
  // ordinary window input before the next driver step. No control values
  // or responders are changed by this test helper.
  if (paced && delivered <= steps) {
    [NSRunLoop.currentRunLoop addTimer:trackingInput forMode:NSDefaultRunLoopMode];
  }
  do {
    while (NSEvent* pending = [NSApp nextEventMatchingMask:mouse untilDate:[NSDate distantPast]
                                                    inMode:NSDefaultRunLoopMode dequeue:YES]) {
      [NSApp sendEvent:pending];
    }
    if (!paced || delivered > steps || NSProcessInfo.processInfo.systemUptime - time >= 6) break;
    [NSRunLoop.currentRunLoop runMode:NSDefaultRunLoopMode beforeDate:[NSDate dateWithTimeIntervalSinceNow:0.01]];
  } while (true);
  [trackingInput invalidate];
  if (paced) {
    qInfo().noquote() << QStringLiteral("Native pointer tracking: %1 follow-up events=%2/%3 focus=%4")
        .arg(qt(view.accessibilityIdentifier)).arg(delivered).arg(steps + 1).arg(window.firstResponder == view);
    if (delivered != steps + 1) {
      postFollowup(steps + 1, NSProcessInfo.processInfo.systemUptime);
      while (NSEvent* pending = [NSApp nextEventMatchingMask:mouse untilDate:[NSDate distantPast]
                                                      inMode:NSDefaultRunLoopMode dequeue:YES]) {
        [NSApp sendEvent:pending];
      }
      error = QStringLiteral("native slider pointer input timed out after %1/%2 follow-up events")
          .arg(delivered).arg(steps + 1);
      return false;
    }
  }
  return true;
}

bool buttonOrderIn(NSView* content, const QStringList& patterns, QString& error) {
  if (content == nil || patterns.isEmpty()) { error = QStringLiteral("no native window for the button order"); return false; }
  [content layoutSubtreeIfNeeded];
  std::vector<NSRect> frames;
  QStringList placed;
  NSView* last = nil;
  for (const QString& pattern : patterns) {
    NSView* button = buttonTitled(content, QRegularExpression(pattern), false);
    if (button == nil) { error = QStringLiteral("no visible native button matching '%1'").arg(pattern); return false; }
    const NSRect frame = frameIn(button, content);
    placed << QStringLiteral("%1 [%2]").arg(qt(((NSButton*)button).title), describeLayoutRect(frame));
    if (!NSContainsRect(NSInsetRect(content.bounds, -0.5, -0.5), frame) ||
        (!frames.empty() && (NSMinX(frame) < NSMaxX(frames.back()) - 0.5 || !sameRow(frame, frames.back())))) {
      error = QStringLiteral("native buttons are not left to right as %1: %2")
                  .arg(patterns.join(QStringLiteral(", ")), placed.join(QStringLiteral("; ")));
      return false;
    }
    frames.push_back(frame);
    last = button;
  }
  // The last expected action is the rightmost of its row.
  NSView* beyond = findView(content, [&](NSView* view) {
    if (view == last || ![view isKindOfClass:NSButton.class] || !viewVisible(view)) return false;
    const NSRect rect = frameIn(view, content);
    return NSMinX(rect) >= NSMaxX(frames.back()) - 0.5 && sameRow(rect, frames.back());
  });
  if (beyond != nil) {
    error = QStringLiteral("native button '%1' is right of '%2'")
                .arg(qt(((NSButton*)beyond).title), qt(((NSButton*)last).title));
    return false;
  }
  qInfo().noquote() << QStringLiteral("Native button order: %1").arg(placed.join(QStringLiteral("; ")));
  return true;
}

} // namespace

bool controlBounds(const QString& identifier, double minimumWidth, const QString& labelPattern, QString& error) {
  NSView* view = findControl(identifier);
  if (view == nil || view.window == nil) { error = QStringLiteral("no native control '%1'").arg(identifier); return false; }
  NSView* content = view.window.contentView;
  [content layoutSubtreeIfNeeded];
  [view scrollRectToVisible:view.bounds];
  const NSRect frame = frameIn(view, content);
  const QString where = QStringLiteral("native '%1' at %2 in %3x%4")
                            .arg(identifier, describeLayoutRect(frame))
                            .arg(content.bounds.size.width, 0, 'f', 0)
                            .arg(content.bounds.size.height, 0, 'f', 0);
  if (!viewVisible(view)) { error = QStringLiteral("%1 is not visible").arg(where); return false; }
  const NSRect shown = view.visibleRect;
  if (shown.size.width + 0.5 < view.bounds.size.width || shown.size.height + 0.5 < view.bounds.size.height ||
      !NSContainsRect(NSInsetRect(content.bounds, -0.5, -0.5), frame)) {
    error = QStringLiteral("%1 is clipped to %2").arg(where, describeLayoutRect(shown));
    return false;
  }
  if (view.bounds.size.width + 0.5 < minimumWidth) {
    error = QStringLiteral("%1 is narrower than %2").arg(where).arg(minimumWidth);
    return false;
  }
  const NSPoint centre = [view convertPoint:NSMakePoint(NSMidX(view.bounds), NSMidY(view.bounds)) toView:content.superview];
  NSView* hit = [content hitTest:centre];
  if (hit != view && ![hit isDescendantOf:view]) {
    error = QStringLiteral("%1 is covered by %2").arg(where, qt(NSStringFromClass(hit.class)));
    return false;
  }
  if (!labelPattern.isEmpty()) {
    const QRegularExpression expression(labelPattern);
    NSView* label = findView(content, [&](NSView* candidate) {
      if (![candidate isKindOfClass:NSTextField.class] || ((NSTextField*)candidate).editable || !viewVisible(candidate)) {
        return false;
      }
      const NSRect rect = frameIn(candidate, content);
      return expression.match(qt(((NSTextField*)candidate).stringValue)).hasMatch() &&
             NSMaxX(rect) <= NSMinX(frame) + 1 && sameRow(rect, frame);
    });
    if (label == nil) {
      error = QStringLiteral("%1 has no label matching '%2' before it on its row").arg(where, labelPattern);
      return false;
    }
  }
  qInfo().noquote() << QStringLiteral("Native bounds: %1").arg(where);
  return true;
}

bool placeholderBounds(const QString& identifier, const QString& labelPattern, QString& error) {
  NSView* view = findControl(identifier);
  NSTextField* field = [view isKindOfClass:NSTextField.class] ? (NSTextField*)view : nil;
  if (field == nil || field.window == nil || field.placeholderString.length == 0) {
    error = QStringLiteral("no native text field '%1' with a placeholder").arg(identifier);
    return false;
  }
  // The size AppKit's own cell needs for the whole placeholder, as if it
  // were the field's text; a copy leaves the field and its text alone.
  NSTextFieldCell* probe = [field.cell copy];
  probe.stringValue = field.placeholderString;
  const double needed = [probe cellSizeForBounds:NSMakeRect(0, 0, CGFLOAT_MAX, CGFLOAT_MAX)].width;
  qInfo().noquote() << QStringLiteral("Native placeholder width: %1 '%2' needs %3, has %4")
                           .arg(identifier, qt(field.placeholderString))
                           .arg(needed, 0, 'f', 1)
                           .arg(field.bounds.size.width, 0, 'f', 1);
  return controlBounds(identifier, needed, labelPattern, error);
}

bool formLayout(const QStringList& entries, QString& error) {
  NSView* content = targetWindow().contentView;
  [content layoutSubtreeIfNeeded];
  std::vector<std::pair<QString, NSRect>> frames;
  const auto fullyVisible = [content](NSView* view) {
    if (view == nil || !viewVisible(view)) return false;
    const NSRect shown = view.visibleRect;
    return shown.size.width + 0.5 >= view.bounds.size.width &&
           shown.size.height + 0.5 >= view.bounds.size.height &&
           NSContainsRect(NSInsetRect(content.bounds, -0.5, -0.5), frameIn(view, content));
  };
  for (const QString& entry : entries) {
    const qsizetype separator = entry.indexOf(QLatin1Char('='));
    const QString id = separator < 0 ? entry : entry.left(separator);
    const QString caption = separator < 0 ? QString() : entry.mid(separator + 1);
    // Unlike findControl/controlBounds, identifying a field here cannot
    // scroll a different field out of sight and hide an overlap or clip.
    NSView* view = identifiedView(content, id);
    const NSRect frame = frameIn(view, content);
    if (!fullyVisible(view)) {
      error = QStringLiteral("native form field '%1' is not fully visible at %2")
                  .arg(id, describeLayoutRect(frame));
      return false;
    }
    for (const auto& previous : frames) {
      if (NSIntersectsRect(NSInsetRect(previous.second, 0.5, 0.5), frame)) {
        error = QStringLiteral("native form fields '%1' and '%2' overlap (%3, %4)")
                    .arg(previous.first, id, describeLayoutRect(previous.second), describeLayoutRect(frame));
        return false;
      }
    }
    frames.emplace_back(id, frame);
    if ([view isKindOfClass:NSPopUpButton.class] && view.bounds.size.width + 1 < ((NSPopUpButton*)view).cell.cellSize.width) {
      error = QStringLiteral("native choice '%1' clips its title at width %2 (needs %3)")
                  .arg(id).arg(view.bounds.size.width).arg(((NSPopUpButton*)view).cell.cellSize.width);
      return false;
    }
    if (caption.isEmpty()) continue;
    NSTextField* label = (NSTextField*)findView(content, [&](NSView* candidate) {
      if (![candidate isKindOfClass:NSTextField.class] || ((NSTextField*)candidate).editable || !fullyVisible(candidate)) return false;
      const NSRect labelFrame = frameIn(candidate, content);
      return qt(((NSTextField*)candidate).stringValue) == caption &&
             NSMaxX(labelFrame) <= NSMinX(frame) + 1 && sameRow(labelFrame, frame);
    });
    const NSSize text = [label.stringValue sizeWithAttributes:@{
        NSFontAttributeName: label.font != nil ? label.font : [NSFont systemFontOfSize:NSFont.systemFontSize]}];
    if (label == nil || label.maximumNumberOfLines != 1 || label.cell.wraps ||
        label.bounds.size.width + 1 < text.width || label.bounds.size.height + 1 < text.height) {
      error = QStringLiteral("native form field '%1' has no readable one-line '%2' label").arg(id, caption);
      return false;
    }
  }
  qInfo().noquote() << QStringLiteral("Native form layout: %1 simultaneously visible, distinct controls with readable labels")
                           .arg(frames.size());
  return true;
}

bool configurationLayout(QString& detail) {
  NSView* host = findControl(QStringLiteral("native.configuration"));
  NSScrollView* scroll = [host isKindOfClass:NSScrollView.class] ? (NSScrollView*)host : nil;
  QWidget* source = nullptr;
  for (QWidget* widget : QApplication::allWidgets()) {
    if (widget->objectName() == QStringLiteral("configurationBar")) { source = widget; break; }
  }
  if (!scroll || !source || !viewVisible(scroll)) {
    detail = QStringLiteral("no visible native configuration card with its shared presenter"); return false;
  }
  [scroll layoutSubtreeIfNeeded];
  const NSRect viewport = NSInsetRect(scroll.contentView.bounds, -1, -1);
  const auto contained = [&](NSView* view, NSRect rect) {
    return viewVisible(view) && NSContainsRect(viewport, [view convertRect:rect toView:scroll.contentView]);
  };
  const auto reachablePoint = [&](NSView* view, NSPoint point) {
    NSView* content = view.window.contentView;
    NSPoint at = [view convertPoint:point toView:content.superview];
    NSView* hit = [content hitTest:at];
    return hit == view || [hit isDescendantOf:view];
  };
  NSView* model = identifiedView(documentWindow().contentView, QStringLiteral("native.modelTree"));
  NSOutlineView* tree = [model isKindOfClass:NSOutlineView.class] ? (NSOutlineView*)model : nil;
  if (tree == nil || tree.numberOfRows == 0) {
    detail = QStringLiteral("configuration card leaves no Browser row to inspect"); return false;
  }
  const NSRect firstRow = [tree rectOfRow:0];
  NSScrollView* browser = tree.enclosingScrollView;
  const NSRect rowInClip = [tree convertRect:firstRow toView:browser.contentView];
  if (!viewVisible(tree) || !NSContainsRect(NSInsetRect(tree.visibleRect, -0.5, -0.5), firstRow) ||
      !NSContainsRect(NSInsetRect(browser.contentView.bounds, -0.5, -0.5), rowInClip) ||
      !reachablePoint(tree, NSMakePoint(NSMidX(firstRow), NSMidY(firstRow)))) {
    detail = QStringLiteral("configuration clips the first Browser row: row %1, visible %2, clip %3")
                 .arg(qt(NSStringFromRect(firstRow)), qt(NSStringFromRect(tree.visibleRect)),
                      qt(NSStringFromRect(browser.contentView.bounds)));
    return false;
  }
  int controls = 0;
  int captions = 0;
  int sliders = 0;
  for (QWidget* widget : source->findChildren<QWidget*>()) {
    if (!widget->isVisibleTo(source)) continue;
    auto* label = qobject_cast<QLabel*>(widget);
    const bool input = qobject_cast<QLineEdit*>(widget) || qobject_cast<QComboBox*>(widget) || qobject_cast<QSlider*>(widget);
    if (!label && !input) continue;
    if (label && label->text().isEmpty()) continue;
    NSView* native = widget->objectName().isEmpty() ? nil : identifiedView(scroll.documentView, widget->objectName());
    if (label) {
      // QLabel's identifier belongs to its wrapper; inspect the actual
      // text field's drawn rectangle and wrapping, including unnamed labels.
      native = findView(native ? native : scroll.documentView, [&](NSView* view) {
        return [view isKindOfClass:NSTextField.class] && !((NSTextField*)view).editable &&
            qt(((NSTextField*)view).stringValue) == label->text();
      });
    }
    if (!native || !contained(native, native.bounds)) {
      detail = QStringLiteral("configuration '%1' is not fully in view: %2")
                   .arg(widget->objectName().isEmpty() && label ? label->text() : widget->objectName(),
                        native ? qt(NSStringFromRect([native convertRect:native.bounds toView:scroll.contentView])) : QStringLiteral("missing"));
      return false;
    }
    if ([native isKindOfClass:NSPopUpButton.class]) {
      NSPopUpButton* popup = (NSPopUpButton*)native;
      const NSRect title = [popup.cell titleRectForBounds:popup.bounds];
      const CGFloat width = [popup.titleOfSelectedItem sizeWithAttributes:@{NSFontAttributeName: popup.font}].width;
      if (width > NSWidth(title) + 1) {
        detail = QStringLiteral("configuration popup '%1' clips '%2': text %3 in %4")
                     .arg(widget->objectName(), qt(popup.titleOfSelectedItem)).arg(width).arg(NSWidth(title)); return false;
      }
    } else if ([native isKindOfClass:NSTextField.class]) {
      NSTextField* field = (NSTextField*)native;
      const NSRect bounds = NSMakeRect(0, 0, label && label->wordWrap() ? NSWidth(field.bounds) : CGFLOAT_MAX, CGFLOAT_MAX);
      const NSSize needed = [field.cell cellSizeForBounds:bounds];
      const bool fits = needed.height <= NSHeight(field.bounds) + 1 &&
                        (label && label->wordWrap() ? field.cell.wraps : needed.width <= NSWidth(field.bounds) + 1);
      if (!fits) {
        detail = QStringLiteral("configuration text '%1' is not fully readable: needs %2x%3 in %4")
                     .arg(qt(field.stringValue)).arg(needed.width).arg(needed.height).arg(qt(NSStringFromRect(field.bounds)));
        return false;
      }
    } else if ([native isKindOfClass:NSSlider.class]) {
      NSSlider* slider = (NSSlider*)native;
      [slider displayIfNeeded];
      NSSliderCell* cell = (NSSliderCell*)slider.cell;
      const NSRect track = [cell barRectFlipped:slider.flipped];
      const NSRect knob = [cell knobRectFlipped:slider.flipped];
      if (NSWidth(track) < 2 * NSWidth(knob) || !contained(slider, track) || !contained(slider, knob) ||
          !reachablePoint(slider, NSMakePoint(NSMidX(knob), NSMidY(knob))) ||
          !reachablePoint(slider, NSMakePoint(NSMinX(track) + 1, NSMidY(track))) ||
          !reachablePoint(slider, NSMakePoint(NSMaxX(track) - 1, NSMidY(track)))) {
        detail = QStringLiteral("configuration slider '%1' has clipped or unreachable travel/knob: %2, %3")
                     .arg(widget->objectName(), qt(NSStringFromRect(track)), qt(NSStringFromRect(knob))); return false;
      }
      ++sliders;
    }
    if (label) ++captions; else ++controls;
  }
  detail = QStringLiteral("configuration layout: %1 complete controls, %2 readable captions, %3 exposed sliders in %4; full reachable Browser row %5")
               .arg(controls).arg(captions).arg(sliders).arg(qt(NSStringFromRect(scroll.contentView.bounds)), qt(NSStringFromRect(firstRow)));
  if (controls == 0 || captions == 0) return false;
  qInfo().noquote() << detail;
  return true;
}

bool spannedHeading(const QString& identifier, const QRegularExpression& pattern, QString& detail) {
  QTreeView* source = nullptr;
  for (QWidget* widget : QApplication::allWidgets()) {
    if (widget->objectName() == identifier) source = qobject_cast<QTreeView*>(widget);
    if (source != nullptr) break;
  }
  NSView* native = findControl(identifier);
  NSScrollView* scroll = [native isKindOfClass:NSScrollView.class] ? (NSScrollView*)native : native.enclosingScrollView;
  NSOutlineView* outline = [scroll.documentView isKindOfClass:NSOutlineView.class] ? (NSOutlineView*)scroll.documentView : nil;
  if (source == nullptr || source->model() == nullptr || outline == nil) {
    detail = QStringLiteral("no native outline with a shared tree '%1'").arg(identifier);
    return false;
  }
  const auto shared = [source, &pattern](const QModelIndex& parent, const auto& self) -> QModelIndex {
    for (int row = 0; row < source->model()->rowCount(parent); ++row) {
      const QModelIndex index = source->model()->index(row, 0, parent);
      if (source->isFirstColumnSpanned(row, parent) && pattern.match(index.data().toString()).hasMatch()) return index;
      if (const QModelIndex child = self(index, self); child.isValid()) return child;
    }
    return {};
  };
  const QModelIndex index = shared(source->rootIndex(), shared);
  if (!index.isValid()) { detail = QStringLiteral("the shared tree has no matching spanned heading"); return false; }
  const QString expected = index.data().toString();
  [outline layoutSubtreeIfNeeded];
  __block NSTableRowView* rowView = nil;
  __block NSTextField* caption = nil;
  __block NSInteger row = -1;
  [outline enumerateAvailableRowViewsUsingBlock:^(NSTableRowView* candidate, NSInteger number) {
    if (caption != nil) return;
    NSView* field = findView(candidate, [&](NSView* view) {
      return [view isKindOfClass:NSTextField.class] && viewVisible(view) &&
          qt(((NSTextField*)view).stringValue) == expected;
    });
    if (field != nil) { rowView = candidate; caption = (NSTextField*)field; row = number; }
  }];
  if (caption == nil) { detail = QStringLiteral("heading '%1' is not in the visible native rows").arg(expected); return false; }
  NSTableCellView* cell = nil;
  for (NSView* parent = caption.superview; parent != nil && parent != rowView; parent = parent.superview) {
    if ([parent isKindOfClass:NSTableCellView.class]) { cell = (NSTableCellView*)parent; break; }
  }
  const NSRect frame = cell != nil ? frameIn(cell, outline) : NSZeroRect;
  const NSRect first = [outline rectOfColumn:0];
  const NSRect rowFrame = [outline rectOfRow:row];
  const NSSize text = [caption.stringValue sizeWithAttributes:@{
      NSFontAttributeName: caption.font != nil ? caption.font : [NSFont systemFontOfSize:NSFont.systemFontSize]}];
  const bool whole = NSWidth(caption.bounds) + 1 >= text.width && NSHeight(caption.bounds) + 1 >= text.height &&
      NSContainsRect(NSInsetRect(caption.visibleRect, -1, -1), caption.bounds);
  const bool spans = rowView.groupRowStyle && cell != nil && NSMaxX(frame) + 1 >= NSMaxX(rowFrame);
  // NSLayoutAnchors constrain alignment rects, not control frames. A
  // borderless NSTextField's frame includes AppKit's alignment insets;
  // checking its raw frame would reject the existing four-point anchors.
  // Read both alignment rects in the same coordinate system. The separate
  // whole-caption check above still checks the real visible view bounds.
  const auto alignmentIn = [](NSView* view, NSView* reference) {
    return view != nil && view.superview != nil
        ? [view.superview convertRect:[view alignmentRectForFrame:view.frame] toView:reference] : NSZeroRect;
  };
  const NSRect inner = alignmentIn(caption, cell);
  const NSRect outer = alignmentIn(cell, cell);
  const CGFloat leading = NSMinX(inner) - NSMinX(outer);
  const CGFloat trailing = NSMaxX(outer) - NSMaxX(inner);
  const bool padded = cell != nil && leading >= 3 && trailing >= 3 &&
      NSMinY(inner) >= NSMinY(outer) - 1 && NSMaxY(inner) <= NSMaxY(outer) + 1;
  const QVariant font = index.data(Qt::FontRole);
  const bool sameWeight = !font.isValid() || boldFont(caption.font) == font.value<QFont>().bold();
  const bool selectable = source->isEnabled() && source->selectionMode() != QAbstractItemView::NoSelection &&
      index.flags().testFlag(Qt::ItemIsEnabled) && index.flags().testFlag(Qt::ItemIsSelectable);
  id<NSOutlineViewDelegate> delegate = outline.delegate;
  const bool nativeSelectable = [delegate respondsToSelector:@selector(outlineView:shouldSelectItem:)] &&
      [delegate outlineView:outline shouldSelectItem:[outline itemAtRow:row]];
  const Qt::Alignment alignment = Qt::Alignment(index.data(Qt::TextAlignmentRole).toInt());
  const NSTextAlignment expectedAlignment = alignment.testFlag(Qt::AlignRight) ? NSTextAlignmentRight
      : alignment.testFlag(Qt::AlignHCenter) ? NSTextAlignmentCenter : NSTextAlignmentNatural;
  detail = QStringLiteral("heading '%1': whole=%2 span=%3 width=%4 first-column=%5 selectable=%6 shared=%7 alignment=%8 padding=%9 font=%10")
      .arg(expected).arg(whole).arg(spans).arg(NSWidth(frame)).arg(NSWidth(first))
      .arg(nativeSelectable).arg(selectable).arg(caption.alignment == expectedAlignment).arg(padded).arg(sameWeight);
  detail += QStringLiteral(" frame=[%1] aligned=[%2] cell-aligned=[%3] margins=%4/%5")
      .arg(describeLayoutRect(frameIn(caption, cell)), describeLayoutRect(inner), describeLayoutRect(outer))
      .arg(leading).arg(trailing);
  detail += QStringLiteral(" group-row=%1 row=[%2] cell=[%3]")
      .arg(bool(rowView.groupRowStyle)).arg(describeLayoutRect(rowFrame), describeLayoutRect(frame));
  if (!whole || !spans || nativeSelectable != selectable || caption.alignment != expectedAlignment ||
      !padded || !sameWeight || caption.editable || caption.selectable || outline.floatsGroupRows) return false;
  qInfo().noquote() << QStringLiteral("Native spanned heading: %1").arg(detail);
  return true;
}

bool firstVisibleRow(const QString& identifier, const QRegularExpression& pattern, QString& detail) {
  NSView* native = findControl(identifier);
  NSScrollView* scroll = [native isKindOfClass:NSScrollView.class] ? (NSScrollView*)native : native.enclosingScrollView;
  NSTableView* table = [scroll.documentView isKindOfClass:NSTableView.class] ? (NSTableView*)scroll.documentView : nil;
  if (table == nil || !viewVisible(table)) {
    detail = QStringLiteral("no visible native table '%1'").arg(identifier);
    return false;
  }
  [scroll layoutSubtreeIfNeeded];
  // The first row wholly inside the clip view, below the header and other
  // insets; read only, nothing is scrolled.
  NSClipView* clip = scroll.contentView;
  NSRect shown = clip.bounds;
  shown.origin.y += clip.contentInsets.top;
  shown.size.height = std::max<CGFloat>(0, shown.size.height - clip.contentInsets.top - clip.contentInsets.bottom);
  shown = [table convertRect:shown fromView:clip];
  const NSRange rows = [table rowsInRect:shown];
  const NSInteger column = std::max<NSInteger>(0, [table columnWithIdentifier:@"0"]);
  for (NSUInteger row = rows.location; row < NSMaxRange(rows); ++row) {
    const NSInteger number = static_cast<NSInteger>(row);
    if (!NSContainsRect(NSInsetRect(shown, -0.5, -0.5), [table rectOfRow:number])) continue;
    NSView* cell = [table viewAtColumn:column row:number makeIfNecessary:NO];
    NSTextField* field = [cell isKindOfClass:NSTableCellView.class] ? ((NSTableCellView*)cell).textField : nil;
    const QString text = field != nil ? qt(field.stringValue) : QString();
    detail = QStringLiteral("native '%1' first visible row %2 is '%3' (rows %4..%5 in view)")
                 .arg(identifier).arg(qlonglong(number)).arg(text)
                 .arg(qulonglong(rows.location)).arg(qulonglong(NSMaxRange(rows)) - 1);
    if (!pattern.match(text).hasMatch()) return false;
    qInfo().noquote() << QStringLiteral("Native first visible row: %1").arg(detail);
    return true;
  }
  detail = QStringLiteral("native '%1' shows no whole row").arg(identifier);
  return false;
}

bool colorField(const QString& identifier, const QString& color, const QString& words, bool placeholder,
                const QString& accessibilityLabel, QString& error) {
  NSView* view = findControl(identifier);
  if (![view isKindOfClass:MitcadNativeColorField.class] || !viewVisible(view)) {
    error = QStringLiteral("'%1' is not a visible native colour field").arg(identifier); return false;
  }
  auto* field = (MitcadNativeColorField*)view;
  if (!accessibilityLabel.isEmpty() && qt(field.accessibilityLabel) != accessibilityLabel) {
    error = QStringLiteral("native colour field '%1' has accessibility label '%2', expected '%3'")
        .arg(identifier, qt(field.accessibilityLabel), accessibilityLabel);
    return false;
  }
  const NSRect visible = field.visibleRect;
  if (NSWidth(visible) + 0.5 < NSWidth(field.bounds) || NSHeight(visible) + 0.5 < NSHeight(field.bounds) ||
      qt(field.words) != words || field.placeholder != placeholder) {
    error = QStringLiteral("native colour field '%1' is clipped or has different placeholder text/state").arg(identifier);
    return false;
  }
  // Inspect the composed pixels users see. Replaying an isolated NSButton
  // through cacheDisplay can differ from its visible backing layer; the
  // own-window capture already used by screenshot-screen preserves it.
  QTemporaryDir capture;
  const QString path = capture.filePath(QStringLiteral("color-field.png"));
  QRectF capturedBounds;
  if (!capture.isValid() || !captureOwnWindows(path, &capturedBounds)) {
    error = QStringLiteral("native colour field's own-window capture failed"); return false;
  }
  const QImage pixels(path);
  if (pixels.isNull() || capturedBounds.isEmpty()) {
    error = QStringLiteral("native colour field's own-window image is empty"); return false;
  }
  const CGFloat scaleX = pixels.width() / capturedBounds.width();
  const CGFloat scaleY = pixels.height() / capturedBounds.height();
  const CGFloat screenTop = NSMaxY(NSScreen.screens.firstObject.frame);
  const auto screenPoint = [field, screenTop](CGFloat x, CGFloat y) {
    const NSPoint point = [field.window convertPointToScreen:[field convertPoint:NSMakePoint(x, y) toView:nil]];
    return QPointF(point.x, screenTop - point.y);
  };
  const auto pixel = [&pixels, capturedBounds, scaleX, scaleY, screenPoint](CGFloat x, CGFloat y) {
    const QPointF point = screenPoint(x, y) - capturedBounds.topLeft();
    const QPoint at(qFloor(point.x() * scaleX), qFloor(point.y() * scaleY));
    return pixels.rect().contains(at) ? pixels.pixelColor(at) : QColor();
  };
  const NSRect swatch = NSInsetRect(field.swatchRect, 2, 2);
  const QColor expected = color == QLatin1String("none") ? QColor() : QColor(color);
  if (color != QLatin1String("none") && !expected.isValid()) {
    error = QStringLiteral("bad expected colour '%1'").arg(color); return false;
  }
  if (expected.isValid()) {
    const QColor actual = pixel(NSMidX(swatch), NSMidY(swatch));
    if (!actual.isValid() || std::abs(actual.redF() - expected.redF()) > 0.08 ||
        std::abs(actual.greenF() - expected.greenF()) > 0.08 ||
        std::abs(actual.blueF() - expected.blueF()) > 0.08) {
      error = QStringLiteral("native colour field '%1' did not paint %2: pixel %3, source %4, "
                             "swatch %5, bounds %6, image %7x%8, capture %9, screen sample %10")
          .arg(identifier, color, actual.isValid() ? actual.name(QColor::HexArgb) : QStringLiteral("outside image"),
               qt(field.swatchColor.description),
               qt(NSStringFromRect(swatch)), qt(NSStringFromRect(field.bounds)))
          .arg(pixels.width()).arg(pixels.height())
          .arg(QStringLiteral("%1,%2 %3x%4").arg(capturedBounds.x()).arg(capturedBounds.y())
                   .arg(capturedBounds.width()).arg(capturedBounds.height()))
          .arg(QStringLiteral("%1,%2").arg(screenPoint(NSMidX(swatch), NSMidY(swatch)).x())
                   .arg(screenPoint(NSMidX(swatch), NSMidY(swatch)).y()));
      return false;
    }
  } else {
    CGFloat darkest = 1;
    CGFloat lightest = 0;
    for (CGFloat x = NSMinX(swatch); x < NSMaxX(swatch); x += 1) {
      const QColor actual = pixel(x, NSMidY(swatch));
      if (!actual.isValid()) {
        error = QStringLiteral("native colour checker '%1' falls outside the captured windows").arg(identifier);
        return false;
      }
      const CGFloat shade = (actual.redF() + actual.greenF() + actual.blueF()) / 3;
      darkest = std::min(darkest, shade);
      lightest = std::max(lightest, shade);
    }
    if (lightest - darkest < 0.04) {
      error = QStringLiteral("native unset colour field '%1' has no visible checker: contrast %2, shades %3..%4")
          .arg(identifier).arg(lightest - darkest).arg(darkest).arg(lightest);
      return false;
    }
  }
  if (!words.isEmpty()) {
    const NSRect text = field.wordsRect;
    const CGFloat width = [field.words sizeWithAttributes:@{NSFontAttributeName: field.font}].width;
    if (NSWidth(text) < width + 3) {
      error = QStringLiteral("native colour placeholder '%1' is not fully readable: width %2, needs %3")
          .arg(words).arg(NSWidth(text)).arg(width + 3);
      return false;
    }
    // Compare each text row with its empty trailing background. This checks
    // actual glyph ink, independently of the semantic string/state above.
    int ink = 0;
    for (CGFloat y = NSMinY(text); y < NSMaxY(text); y += 1) {
      const QColor background = pixel(NSMaxX(text) - 1, y);
      for (CGFloat x = NSMinX(text); x < NSMinX(text) + width; x += 1) {
        const QColor actual = pixel(x, y);
        if (!actual.isValid() || !background.isValid()) {
          error = QStringLiteral("native colour placeholder '%1' falls outside the captured windows").arg(words);
          return false;
        }
        const CGFloat difference = std::abs(actual.redF() - background.redF()) +
            std::abs(actual.greenF() - background.greenF()) +
            std::abs(actual.blueF() - background.blueF());
        if (difference > 0.06) ++ink;
      }
    }
    if (ink < 8) {
      error = QStringLiteral("native colour placeholder '%1' has no visible glyphs: %2 ink samples in %3x%4 image")
          .arg(words).arg(ink).arg(pixels.width()).arg(pixels.height());
      return false;
    }
  }
  qInfo().noquote() << QStringLiteral("Native colour field: %1 paints %2, words '%3', %4")
      .arg(identifier, color, words, placeholder ? QStringLiteral("placeholder") : QStringLiteral("explicit"));
  return true;
}

bool buttonOrder(const QStringList& patterns, QString& error) {
  NSWindow* window = targetWindow();
  return buttonOrderIn(window.contentView, patterns, error);
}

bool focused(const QString& identifier) {
  NSView* view = findControl(identifier);
  NSResponder* responder = view.window.firstResponder;
  if (view == nil || responder == nil) return false;
  if (responder == view) return true;
  if ([view isKindOfClass:NSTextField.class]) {
    NSText* editor = ((NSTextField*)view).currentEditor;
    return editor != nil && responder == editor;
  }
  return [responder isKindOfClass:NSView.class] && [(NSView*)responder isDescendantOf:view];
}

bool pointerClick(const QString& identifier, QString& error, double x, double y) {
  if (!(x >= 0 && x <= 1 && y >= 0 && y <= 1)) {
    error = QStringLiteral("native pointer coordinates must be fractions from 0 to 1");
    return false;
  }
  // Keep the same real hit/visibility checks for a cell addressed by its
  // table/row/column, rather than clicking the scroll view's empty area.
  NSView* view = tableCell(identifier, &error);
  if (view == nil) return false;
  if ([view isKindOfClass:NSControl.class] && !((NSControl*)view).enabled) {
    error = QStringLiteral("native control '%1' is disabled").arg(identifier);
    return false;
  }
  const NSPoint point = NSMakePoint(NSMinX(view.bounds) + NSWidth(view.bounds) * x,
                                    NSMinY(view.bounds) + NSHeight(view.bounds) * y);
  return pointerEvents(view, point, point, error);
}

bool pointerDoubleClick(const QString& identifier, QString& error) {
  if (!active() || qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT")) {
    error = QStringLiteral("native pointer double click requires the native UI test driver"); return false;
  }
  NSWindow* window = nil;
  for (int count = 1; count <= 2; ++count) {
    // A single click can refresh the model's row views. Resolve the exact
    // identifier (then its positional cell locator) anew for each press.
    NSView* view = tableCell(identifier, &error);
    NSTableView* table = mac::owningTable(view);
    if (!viewVisible(view) || ([view isKindOfClass:NSControl.class] && !((NSControl*)view).enabled) ||
        (table != nil && !table.enabled) || (window != nil && view.window != window)) {
      error = QStringLiteral("native double-click target '%1' is not a live, visible, enabled control in the same window")
          .arg(identifier); return false;
    }
    window = view.window;
    qInfo().noquote() << QStringLiteral("Native pointer double click: %1 click-count=%2")
        .arg(identifier).arg(count);
    // The window hit-tests both genuine down/up pairs at the current
    // visible centre, resolved after scrolling/layout for each press. No
    // selector, table selection or double-action is invoked by this helper.
    if (!pointerEvents(view, NSZeroPoint, NSZeroPoint, error, count, true, identifier)) return false;
  }
  return true;
}

bool pointerChoice(const QString& identifier, const QString& expected, const QStringList& keys, QString& error) {
  if (!active() || qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT")) {
    error = QStringLiteral("native pointer choice requires the native UI test driver"); return false;
  }
  NSView* view = findControl(identifier);
  if (![view isKindOfClass:NSPopUpButton.class] || !viewVisible(view) || !((NSPopUpButton*)view).enabled) {
    error = QStringLiteral("no visible enabled native popup '%1'").arg(identifier); return false;
  }
  NSPopUpButton* popup = (NSPopUpButton*)view;
  NSMenuItem* wanted = [popup itemWithTitle:expected.toNSString()];
  if (popup.pullsDown || wanted == nil || !wanted.enabled || keys.size() < 2 || keys.size() > 32 ||
      keys.last() != QStringLiteral("Return")) {
    error = QStringLiteral("native-pointer-choice needs an enabled popup item and Up/Down keys followed by Return");
    return false;
  }
  for (qsizetype index = 0; index + 1 < keys.size(); ++index) {
    if (keys[index] != QStringLiteral("Up") && keys[index] != QStringLiteral("Down")) {
      error = QStringLiteral("native pointer choice navigation accepts only Up/Down before Return"); return false;
    }
  }
  const QString before = qt(popup.titleOfSelectedItem);
  NSWindow* window = popup.window;
  QElapsedTimer deadline;
  deadline.start();
  __block NSTimer* response = nil;
  __block bool began = false;
  __block bool ended = false;
  __block bool timedOut = false;
  __block qsizetype delivered = 0;
  NSNotificationCenter* center = NSNotificationCenter.defaultCenter;
  id beginObserver = [center addObserverForName:NSMenuDidBeginTrackingNotification object:popup.menu queue:nil
      usingBlock:^(NSNotification* notice) {
    (void)notice;
    if (began) return;
    began = true;
    // QTimer does not reliably run inside AppKit's nested menu tracking.
    // Deliver actual key down/up there after the pointer opened this menu.
    response = [NSTimer timerWithTimeInterval:0.05 repeats:YES block:^(NSTimer* timer) {
      (void)timer;
      if (ended) return;
      if (deadline.elapsed() >= 6000 || !NSApp.active || !window.visible) {
        timedOut = true;
        [popup.menu cancelTracking];
        return;
      }
      if (delivered >= keys.size()) return;
      const QString text = keys[delivered++];
      const int key = text == QStringLiteral("Up") ? Qt::Key_Up
                      : text == QStringLiteral("Down") ? Qt::Key_Down : Qt::Key_Return;
      NSString* chars = characters(key, QString());
      for (NSEventType type : {NSEventTypeKeyDown, NSEventTypeKeyUp}) {
        [NSApp postEvent:[NSEvent keyEventWithType:type location:NSZeroPoint modifierFlags:0
            timestamp:NSProcessInfo.processInfo.systemUptime windowNumber:window.windowNumber context:nil
            characters:chars charactersIgnoringModifiers:chars isARepeat:NO keyCode:virtualKey(key)] atStart:NO];
      }
    }];
    [NSRunLoop.currentRunLoop addTimer:response forMode:NSEventTrackingRunLoopMode];
    [NSRunLoop.currentRunLoop addTimer:response forMode:NSRunLoopCommonModes];
  }];
  id endObserver = [center addObserverForName:NSMenuDidEndTrackingNotification object:popup.menu queue:nil
      usingBlock:^(NSNotification* notice) { (void)notice; ended = true; }];
  // Install the watchdog before mouseDown can enter a nested tracking loop.
  // The begin notification and response timer are evidence, not prerequisites
  // for bounded failure cleanup; never cancel another control's menu.
  NSTimer* watchdog = [NSTimer timerWithTimeInterval:0.05 repeats:YES block:^(NSTimer* timer) {
    (void)timer;
    if (!ended && (deadline.elapsed() >= 6000 || !NSApp.active || !window.visible)) {
      timedOut = true;
      [popup.menu cancelTracking];
    }
  }];
  [NSRunLoop.currentRunLoop addTimer:watchdog forMode:NSEventTrackingRunLoopMode];
  [NSRunLoop.currentRunLoop addTimer:watchdog forMode:NSRunLoopCommonModes];
  const bool clicked = pointerClick(identifier, error);
  while (clicked && !ended && deadline.elapsed() < 6000) {
    QCoreApplication::processEvents(QEventLoop::AllEvents, 25);
    [NSRunLoop.currentRunLoop runMode:NSDefaultRunLoopMode
                          beforeDate:[NSDate dateWithTimeIntervalSinceNow:0.01]];
  }
  if (began && !ended) { timedOut = true; [popup.menu cancelTracking]; }
  [watchdog invalidate];
  [response invalidate];
  [center removeObserver:beginObserver];
  [center removeObserver:endObserver];
  if (!clicked) return false;
  const QString after = qt(popup.titleOfSelectedItem);
  if (!began || !ended || timedOut || delivered != keys.size() || after != expected) {
    error = QStringLiteral("native popup '%1': began=%2 ended=%3 timeout=%4 keys=%5/%6, displayed '%7', expected '%8'")
        .arg(identifier).arg(began).arg(ended).arg(timedOut).arg(delivered).arg(keys.size()).arg(after, expected);
    return false;
  }
  qInfo().noquote() << QStringLiteral("Native pointer choice: %1 '%2' -> '%3', keys %4, menu tracked")
      .arg(identifier, before, after, keys.join(QLatin1Char(' ')));
  return true;
}

bool pointerButton(const QRegularExpression& expression, QString& error) {
  NSView* content = targetWindow().contentView;
  NSView* button = buttonTitled(content.superview != nil ? content.superview : content, expression, true);
  if (button == nil) {
    error = QStringLiteral("no enabled native button matching '%1'").arg(expression.pattern());
    return false;
  }
  const NSPoint centre = NSMakePoint(NSMidX(button.bounds), NSMidY(button.bounds));
  return pointerEvents(button, centre, centre, error);
}

namespace {

bool sliderPointerTarget(NSSlider* slider, NSWindow* window, CGPoint point, bool hitKnob,
                         bool& checkedWindowList, QString& error) {
  NSRunningApplication* front = NSWorkspace.sharedWorkspace.frontmostApplication;
  if (qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT") || !active() || !NSApp.active ||
      front == nil || front.processIdentifier != getpid() || window == nil || !window.visible ||
      NSApp.keyWindow != window || slider.window != window || !viewVisible(slider) || !slider.enabled ||
      [NSApp windowWithWindowNumber:window.windowNumber] != window ||
      findControl(qt(slider.accessibilityIdentifier)) != slider) {
    error = QStringLiteral("WindowServer slider target changed: active=%1 front=%2 self=%3 key=%4 expected=%5 visible=%6 enabled=%7")
        .arg(bool(NSApp.active)).arg(front ? qlonglong(front.processIdentifier) : -1).arg(qlonglong(getpid()))
        .arg(qlonglong(NSApp.keyWindow.windowNumber)).arg(qlonglong(window.windowNumber))
        .arg(bool(window.visible)).arg(bool(slider.enabled)); return false;
  }
  if (!CGPreflightPostEventAccess()) {
    error = QStringLiteral("WindowServer slider input requires already granted event-posting access"); return false;
  }
  if ((NSEvent.pressedMouseButtons & ~NSUInteger(1)) != 0) {
    error = QStringLiteral("another mouse button is pressed during the slider gesture"); return false;
  }
  const CGFloat top = NSMaxY(NSScreen.screens.firstObject.frame);
  const NSPoint screen = NSMakePoint(point.x, top - point.y);
  if (!NSPointInRect(screen, window.frame)) {
    error = QStringLiteral("WindowServer slider pointer would leave its owned window"); return false;
  }
  const NSInteger recipient = [NSWindow windowNumberAtPoint:screen belowWindowWithWindowNumber:0];
  if (recipient != window.windowNumber) {
    error = QStringLiteral("WindowServer slider requires own mouse recipient %1, found %2")
        .arg(qlonglong(window.windowNumber)).arg(qlonglong(recipient)); return false;
  }
  NSView* content = window.contentView;
  const NSRect bounds = [slider convertRect:slider.bounds toView:content];
  if (!NSContainsRect(NSInsetRect(content.bounds, -0.5, -0.5), bounds) ||
      NSWidth(slider.visibleRect) + 0.5 < NSWidth(slider.bounds) ||
      NSHeight(slider.visibleRect) + 0.5 < NSHeight(slider.bounds)) {
    error = QStringLiteral("WindowServer slider input requires its fully visible control"); return false;
  }
  if (hitKnob) {
    const NSPoint local = [slider convertPoint:[window convertPointFromScreen:screen] fromView:nil];
    const NSRect knob = [(NSSliderCell*)slider.cell knobRectFlipped:slider.flipped];
    NSView* hit = [content hitTest:[content.superview convertPoint:[window convertPointFromScreen:screen] fromView:nil]];
    if (!NSPointInRect(local, knob) || (hit != slider && ![hit isDescendantOf:slider])) {
      error = QStringLiteral("WindowServer slider press does not hit its actual exposed knob"); return false;
    }
  }
  NSArray* infos = CFBridgingRelease(CGWindowListCopyWindowInfo(
      kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements, kCGNullWindowID));
  if (infos == nil || infos.count == 0) return true; // metadata unavailable; never request another permission
  checkedWindowList = true;
  for (NSDictionary* info in infos) {
    const NSInteger number = [info[(id)kCGWindowNumber] integerValue];
    if (number == window.windowNumber) {
      CGRect rect;
      if (!CGRectMakeWithDictionaryRepresentation((__bridge CFDictionaryRef)info[(id)kCGWindowBounds], &rect) ||
          !CGRectContainsPoint(rect, point)) {
        error = QStringLiteral("slider screen coordinates leave the actual WindowServer window bounds"); return false;
      }
      return true;
    }
  }
  error = QStringLiteral("the owned slider window is absent from the on-screen window metadata"); return false;
}

bool systemSliderDrag(NSSlider* slider, NSPoint from, NSPoint to, QString& error) {
  NSWindow* window = slider.window;
  [window layoutIfNeeded];
  [window.contentView layoutSubtreeIfNeeded];
  const CGFloat top = NSMaxY(NSScreen.screens.firstObject.frame);
  const auto screenPoint = [slider, window, top](NSPoint point) {
    const NSPoint screen = [window convertPointToScreen:[slider convertPoint:point toView:nil]];
    return CGPointMake(screen.x, top - screen.y);
  };
  const CGPoint start = screenPoint(from);
  const CGPoint end = screenPoint(to);
  bool checkedWindowList = false;
  if (!sliderPointerTarget(slider, window, start, true, checkedWindowList, error) ||
      !sliderPointerTarget(slider, window, end, false, checkedWindowList, error)) return false;
  if (NSEvent.pressedMouseButtons != 0 ||
      CGEventSourceButtonState(kCGEventSourceStateCombinedSessionState, kCGMouseButtonLeft)) {
    error = QStringLiteral("WindowServer slider input will not replace an existing mouse press"); return false;
  }
  struct State {
    CGEventSourceRef source = nullptr;
    CGEventRef release = nullptr;
    QElapsedTimer clock;
    CGPoint last{};
    int postedDrags = 0;
    int receivedDrags = 0;
    bool downPosted = false;
    bool downSeen = false;
    bool trackingSeen = false;
    bool upPosted = false;
    bool upSeen = false;
    bool checkedWindows = false;
    qint64 marker = 0;
    QString failure;
    ~State() { if (release) CFRelease(release); if (source) CFRelease(source); }
  };
  auto state = std::make_shared<State>();
  static quint32 serial = 0;
  state->marker = (qint64(getpid()) << 32) | ++serial;
  state->last = start;
  state->checkedWindows = checkedWindowList;
  state->source = CGEventSourceCreate(kCGEventSourceStateHIDSystemState);
  state->release = state->source ? CGEventCreateMouseEvent(state->source, kCGEventLeftMouseUp, start, kCGMouseButtonLeft) : nullptr;
  CGEventRef down = state->source ? CGEventCreateMouseEvent(state->source, kCGEventLeftMouseDown, start, kCGMouseButtonLeft) : nullptr;
  if (!state->release || !down) {
    if (down) CFRelease(down);
    error = QStringLiteral("could not prepare both owned slider press and balancing release"); return false;
  }
  const auto tag = [state](CGEventRef event) {
    CGEventSetFlags(event, 0);
    CGEventSetIntegerValueField(event, kCGEventSourceUserData, state->marker);
    CGEventSetIntegerValueField(event, kCGMouseEventClickState, 1);
  };
  tag(down);
  tag(state->release);
  // The balancing release is allocated before Down, and remains available
  // even if the app/window changes or a later event allocation fails.
  const auto release = [state](bool force = false) {
    if (state->downPosted && (!state->upPosted || force)) {
      CGEventSetLocation(state->release, state->last);
      CGEventPost(kCGHIDEventTap, state->release);
      state->upPosted = true;
    }
  };
  const auto tracking = [slider] {
    return [slider respondsToSelector:@selector(pointerTracking)] &&
        [(id<MITCADNativeSliderTrackingTest>)slider pointerTracking];
  };
  const auto finished = [state, tracking] {
    return state->downSeen && state->trackingSeen && state->upPosted && !tracking() &&
        (NSEvent.pressedMouseButtons & NSUInteger(1)) == 0;
  };
  const NSEventMask mask = NSEventMaskLeftMouseDown | NSEventMaskLeftMouseDragged | NSEventMaskLeftMouseUp;
  id monitor = [NSEvent addLocalMonitorForEventsMatchingMask:mask handler:^NSEvent*(NSEvent* event) {
    if (!state->downPosted || event.windowNumber != window.windowNumber) return event;
    CGEventRef raw = event.CGEvent;
    const int64_t marker = raw ? CGEventGetIntegerValueField(raw, kCGEventSourceUserData) : 0;
    if (marker != 0 && marker != state->marker) return event;
    if (event.type == NSEventTypeLeftMouseDown) {
      state->downSeen = true;
      qInfo().noquote() << QStringLiteral("Native slider WindowServer received: down window=%1 pressed=%2 marker=%3")
          .arg(qlonglong(window.windowNumber)).arg(qulonglong(NSEvent.pressedMouseButtons)).arg(qlonglong(marker));
    } else if (event.type == NSEventTypeLeftMouseDragged) { ++state->receivedDrags; }
    else if (event.type == NSEventTypeLeftMouseUp) { state->upSeen = true; }
    return event;
  }];
  if (monitor == nil) {
    CFRelease(down);
    error = QStringLiteral("could not observe the owned slider's application input"); return false;
  }
  state->clock.start();
  NSTimer* timer = [NSTimer timerWithTimeInterval:0.04 repeats:YES block:^(NSTimer* tick) {
    state->trackingSeen = state->trackingSeen || tracking();
    if (finished()) { [tick invalidate]; return; }
    if (!state->failure.isEmpty()) { release(); return; }
    if (!state->downSeen) return;
    const CGFloat fraction = CGFloat(state->postedDrags + 1) / 6;
    const CGPoint point = state->postedDrags < 6
        ? CGPointMake(start.x + (end.x - start.x) * fraction, start.y + (end.y - start.y) * fraction) : end;
    if (!sliderPointerTarget(slider, window, point, false, state->checkedWindows, state->failure)) {
      release(); return;
    }
    if (state->postedDrags < 6) {
      CGEventRef drag = CGEventCreateMouseEvent(state->source, kCGEventLeftMouseDragged, point, kCGMouseButtonLeft);
      if (!drag) { state->failure = QStringLiteral("could not create an owned slider drag event"); release(); return; }
      tag(drag);
      CGEventPost(kCGHIDEventTap, drag);
      CFRelease(drag);
      state->last = point;
      ++state->postedDrags;
    } else { release(); }
  }];
  NSTimer* watchdog = [NSTimer timerWithTimeInterval:0.05 repeats:YES block:^(NSTimer* tick) {
    if (finished()) { [tick invalidate]; return; }
    if (state->clock.elapsed() >= 5800) {
      state->failure = QStringLiteral("WindowServer slider gesture exceeded its six-second input budget");
      release(true); [tick invalidate];
    }
  }];
  [NSRunLoop.currentRunLoop addTimer:timer forMode:NSRunLoopCommonModes];
  [NSRunLoop.currentRunLoop addTimer:timer forMode:NSEventTrackingRunLoopMode];
  [NSRunLoop.currentRunLoop addTimer:watchdog forMode:NSRunLoopCommonModes];
  [NSRunLoop.currentRunLoop addTimer:watchdog forMode:NSEventTrackingRunLoopMode];
  qInfo().noquote() << QStringLiteral("Native slider WindowServer begin: %1 window=%2 start=%3,%4 end=%5,%6 occlusion-metadata=%7")
      .arg(qt(slider.accessibilityIdentifier)).arg(qlonglong(window.windowNumber))
      .arg(start.x).arg(start.y).arg(end.x).arg(end.y).arg(state->checkedWindows);
  if (sliderPointerTarget(slider, window, start, true, state->checkedWindows, state->failure)) {
    CGEventPost(kCGHIDEventTap, down);
    state->downPosted = true;
  }
  CFRelease(down);
  while (state->downPosted && !finished() && state->failure.isEmpty() && state->clock.elapsed() < 6000) {
    QCoreApplication::processEvents(QEventLoop::AllEvents, 25);
    [NSRunLoop.currentRunLoop runMode:NSDefaultRunLoopMode beforeDate:[NSDate dateWithTimeIntervalSinceNow:0.01]];
  }
  [timer invalidate];
  [watchdog invalidate];
  if (state->downPosted && (!state->upPosted || tracking() || (NSEvent.pressedMouseButtons & NSUInteger(1)) != 0)) release(true);
  while (state->downPosted && (tracking() || (NSEvent.pressedMouseButtons & NSUInteger(1)) != 0) && state->clock.elapsed() < 6000) {
    QCoreApplication::processEvents(QEventLoop::AllEvents, 10);
    [NSRunLoop.currentRunLoop runMode:NSDefaultRunLoopMode beforeDate:[NSDate dateWithTimeIntervalSinceNow:0.01]];
  }
  [NSEvent removeMonitor:monitor];
  // NSEvent.h explicitly excludes nested control-tracking events from
  // local monitors. Their counts are diagnostic; actual tracking must have
  // started, ended after our release, and left the OS button released.
  qInfo().noquote() << QStringLiteral("Native slider WindowServer end: down=%1 drags=%2/6 received=%3 up=%4/%5 pressed=%6 tracking-seen=%7 tracking-now=%8")
      .arg(state->downSeen).arg(state->postedDrags).arg(state->receivedDrags)
      .arg(state->upPosted).arg(state->upSeen).arg(qulonglong(NSEvent.pressedMouseButtons))
      .arg(state->trackingSeen).arg(tracking());
  if (!state->failure.isEmpty() || state->postedDrags != 6 || !finished()) {
    error = !state->failure.isEmpty() ? state->failure : QStringLiteral("WindowServer slider input did not finish its actual owned press/drag/release");
    return false;
  }
  return true;
}

} // namespace

bool pointerDrag(const QString& identifier, double from, double to, QString& error, bool fromKnob) {
  NSView* view = findControl(identifier);
  if (view == nil) { error = QStringLiteral("no native control '%1'").arg(identifier); return false; }
  const NSRect bounds = view.bounds;
  const CGFloat y = NSMidY(bounds);
  NSPoint start = NSMakePoint(NSMinX(bounds) + bounds.size.width * from, y);
  if (fromKnob) {
    if (![view isKindOfClass:NSSlider.class] || ![((NSSlider*)view).cell isKindOfClass:NSSliderCell.class]) {
      error = QStringLiteral("native knob drag requires an NSSlider '%1'").arg(identifier); return false;
    }
    NSSlider* slider = (NSSlider*)view;
    [slider layoutSubtreeIfNeeded];
    const NSRect knob = [(NSSliderCell*)slider.cell knobRectFlipped:slider.flipped];
    start = NSMakePoint(NSMidX(knob), NSMidY(knob));
    if (NSIsEmptyRect(knob) || !NSPointInRect(start, bounds) || !NSPointInRect(start, view.visibleRect)) {
      error = QStringLiteral("native slider '%1' has no exposed actual knob centre: %2")
          .arg(identifier, describeLayoutRect(knob)); return false;
    }
    qInfo().noquote() << QStringLiteral("Native slider knob grab: %1 knob=%2 value=%3 continuous=%4")
        .arg(identifier, describeLayoutRect(knob)).arg(slider.doubleValue).arg(slider.continuous);
  }
  const NSPoint end = NSMakePoint(NSMinX(bounds) + bounds.size.width * to, y);
  const bool isSlider = [view isKindOfClass:NSSlider.class];
  if (isSlider && !fromKnob) {
    error = QStringLiteral("held native slider input requires the actual 'knob' start"); return false;
  }
  if (!(isSlider ? systemSliderDrag((NSSlider*)view, start, end, error) : pointerEvents(view, start, end, error))) {
    return false;
  }
  const NSRect frame = [view convertRect:view.bounds toView:nil];
  // A drag past either end of a slider's track leaves it at that end.
  if ([view isKindOfClass:NSSlider.class] && (to <= 0 || to >= 1)) {
    NSSlider* slider = (NSSlider*)view;
    const double expected = to <= 0 ? slider.minValue : slider.maxValue;
    if (std::abs(slider.doubleValue - expected) > 1e-6) {
      error = QStringLiteral("native slider '%1' stopped at %2, not %3 (window frame %4, key %5)")
                  .arg(identifier).arg(slider.doubleValue).arg(expected)
                  .arg(describeLayoutRect(frame)).arg(view.window.keyWindow ? 1 : 0);
      return false;
    }
  }
  return true;
}

bool resizeDialog(int width, int height, QString& error) {
  NSWindow* window = targetWindow();
  if (window == nil || window == documentWindow()) {
    error = QStringLiteral("native-dialog-resize needs a focused native dialog");
    return false;
  }
  // Content constraints above the window's own size priority may enlarge
  // it again: that actual size is the narrowest the dialog supports.
  [window setContentSize:NSMakeSize(width, height)];
  [window layoutIfNeeded];
  const NSSize size = window.contentView.frame.size;
  qInfo().noquote() << QStringLiteral("Native dialog size: requested %1x%2, actual %3x%4")
                           .arg(width).arg(height).arg(qRound(size.width)).arg(qRound(size.height));
  return true;
}

namespace {

// Synthetic presenters with the three audited shapes: a search field after
// a check box, two equal fields in one form row, and slider rows in two
// forms side by side; plus a spanning row over stretched grid columns.
bool columnSizingContract(QString& error) {
  QWidget presenter;
  presenter.setAttribute(Qt::WA_DontShowOnScreen);
  auto* layout = new QVBoxLayout(&presenter);
  auto* table = new QTableView;
  table->setObjectName(QStringLiteral("contract.columns"));
  layout->addWidget(table);
  QStandardItemModel model(1, 1);
  model.setHeaderData(0, Qt::Horizontal, QStringLiteral("Native header content"));
  model.setData(model.index(0, 0), QStringLiteral("Value"));
  table->setModel(&model);
  QHeaderView* header = table->horizontalHeader();
  header->setStretchLastSection(false);
  header->setSectionResizeMode(QHeaderView::Interactive);
  header->setMinimumSectionSize(1);
  header->resizeSection(0, 1);
  requestNativeColumnContents(header);
  auto view = createNativeWidgetView(&presenter);
  view->refresh();
  NSView* root = (__bridge NSView*)view->nativeView();
  NSView* found = identifiedView(root, table->objectName());
  NSScrollView* scroll = [found isKindOfClass:NSScrollView.class] ? (NSScrollView*)found : nil;
  NSTableView* native = [scroll.documentView isKindOfClass:NSTableView.class] ? (NSTableView*)scroll.documentView : nil;
  if (native.tableColumns.count != 1) { error = QStringLiteral("content-size fixture has no native column"); return false; }
  NSTableColumn* column = native.tableColumns.firstObject;
  const int content = static_cast<int>(std::ceil(column.width));
  const auto check = [&error](bool passed, const QString& message) {
    if (!passed) error = message;
    return passed;
  };
  if (!check(content > header->sectionSize(0) && header->sectionResizeMode(0) == QHeaderView::Interactive &&
                 column.resizingMask == NSTableColumnUserResizingMask,
             QStringLiteral("one-time content sizing lost its native content width or interactive resize policy"))) return false;
  // A native header drag changes this same public column width. Ordinary
  // presenter refreshes must retain it; a fresh request restores contents.
  column.width = content + 40;
  for (int pass = 0; pass < 3; ++pass) view->refresh();
  if (!check(std::abs(column.width - content - 40) <= 0.5,
             QStringLiteral("ordinary refresh discarded a native interactive column width"))) return false;
  requestNativeColumnContents(header);
  view->refresh();
  if (!check(std::abs(column.width - content) <= 0.5,
             QStringLiteral("a repeated content request did not restore native contents width"))) return false;
  // The new shared width equals the old automatic native width, while the
  // actual native width is different: comparing only cached preferences
  // must not swallow this explicit resize.
  column.width = content + 40;
  header->resizeSection(0, content);
  view->refresh();
  if (!check(std::abs(column.width - content) <= 0.5,
             QStringLiteral("explicit shared resize to the prior native preference left the user width unchanged"))) return false;
  header->resizeSection(0, content + 20);
  view->refresh();
  if (!check(std::abs(column.width - content - 20) <= 0.5,
             QStringLiteral("an ended content request overrode a later explicit shared column resize"))) return false;
  qInfo() << "Native column contents: native header width, retained user resize, repeated request and explicit shared resize verified";
  return true;
}

bool layoutWidthContract(QString& error) {
  QWidget presenter;
  presenter.setAttribute(Qt::WA_DontShowOnScreen);
  auto* column = new QVBoxLayout(&presenter);
  auto* top = new QHBoxLayout;
  top->addStretch(1);
  auto* only = new QCheckBox(QStringLiteral("Only limited"));
  only->setObjectName(QStringLiteral("contract.only"));
  auto* search = new QLineEdit;
  search->setObjectName(QStringLiteral("contract.search"));
  search->setMinimumWidth(180);
  search->setMaximumWidth(260);
  top->addWidget(only);
  top->addWidget(search);
  column->addLayout(top);
  auto* form = new QFormLayout;
  auto* display = new QLineEdit;
  display->setObjectName(QStringLiteral("contract.display"));
  auto* name = new QLineEdit;
  name->setObjectName(QStringLiteral("contract.name"));
  auto* email = new QLineEdit;
  email->setObjectName(QStringLiteral("contract.email"));
  auto* author = new QHBoxLayout;
  author->addWidget(name, 1);
  author->addWidget(email, 1);
  form->addRow(QStringLiteral("Display name:"), display);
  form->addRow(QStringLiteral("Author:"), author);
  column->addLayout(form);
  // As SettingField's colour input: an expanding button in a fixed-width
  // editor, followed by addStretch(0). The empty-caption native button's
  // intrinsic width must not divert the editor's spare width to that spacer.
  auto* fixedEditor = new QWidget;
  fixedEditor->setObjectName(QStringLiteral("contract.fixed-editor"));
  fixedEditor->setFixedWidth(240);
  auto* editorRow = new QHBoxLayout(fixedEditor);
  editorRow->setContentsMargins(0, 0, 0, 0);
  auto* color = new QPushButton;
  color->setObjectName(QStringLiteral("contract.stretched-color"));
  color->setAutoDefault(false);
  color->setSizePolicy(QSizePolicy::Expanding, QSizePolicy::Fixed);
  color->setProperty(NativeColorFieldProperty, QColor(QStringLiteral("#2a70b8")));
  color->setProperty(NativeColorFieldTextProperty, QStringLiteral("Inherited colour"));
  color->setProperty(NativeColorFieldPlaceholderProperty, true);
  editorRow->addWidget(color, 1);
  editorRow->addStretch(0);
  column->addWidget(fixedEditor);
  // The allocation rule is generic: a text input with a preferred width
  // must also grow past that hint before the zero-stretch spacer grows.
  auto* textEditor = new QWidget;
  textEditor->setObjectName(QStringLiteral("contract.fixed-text-editor"));
  textEditor->setFixedWidth(240);
  auto* textRow = new QHBoxLayout(textEditor);
  textRow->setContentsMargins(0, 0, 0, 0);
  auto* stretchedText = new QLineEdit;
  stretchedText->setObjectName(QStringLiteral("contract.stretched-text"));
  textRow->addWidget(stretchedText, 1);
  textRow->addStretch(0);
  column->addWidget(textEditor);
  auto* speeds = new QHBoxLayout;
  for (const QString& side : {QStringLiteral("left"), QStringLiteral("right")}) {
    auto* speedForm = new QFormLayout;
    for (int index = 0; index < 2; ++index) {
      auto* row = new QHBoxLayout;
      auto* slider = new QSlider(Qt::Horizontal);
      slider->setObjectName(QStringLiteral("contract.%1%2").arg(side).arg(index));
      slider->setMinimumWidth(70);
      slider->setRange(10, 400);
      auto* value = new QLabel(QStringLiteral("1.00×"));
      value->setMinimumWidth(48);
      row->addWidget(slider, 1);
      row->addWidget(value);
      row->addWidget(new QPushButton(QStringLiteral("Reset")));
      speedForm->addRow(index == 0 ? QStringLiteral("Orbit:") : QStringLiteral("Scroll speed:"), row);
    }
    speeds->addLayout(speedForm);
  }
  column->addLayout(speeds);
  auto* grid = new QGridLayout;
  auto* span = new QLineEdit;
  span->setObjectName(QStringLiteral("contract.span"));
  auto* narrow = new QLineEdit;
  narrow->setObjectName(QStringLiteral("contract.narrow"));
  auto* wide = new QLineEdit;
  wide->setObjectName(QStringLiteral("contract.wide"));
  grid->addWidget(span, 0, 0, 1, 2);
  grid->addWidget(narrow, 1, 0);
  grid->addWidget(wide, 1, 1);
  grid->setColumnStretch(0, 1);
  grid->setColumnStretch(1, 2);
  column->addLayout(grid);
  // Like SettingsForm: filtering hides a category wrapper, while its
  // child's own hidden flag remains false.
  auto* category = new QWidget;
  category->setObjectName(QStringLiteral("contract.category"));
  auto* categoryLayout = new QVBoxLayout(category);
  auto* categoryField = new QLineEdit;
  categoryField->setObjectName(QStringLiteral("contract.category.field"));
  categoryLayout->addWidget(categoryField);
  column->addWidget(category);

  auto view = createNativeWidgetView(&presenter);
  view->refresh();
  NSView* root = (__bridge NSView*)view->nativeView();
  NSWindow* fixture = [[NSWindow alloc] initWithContentRect:NSMakeRect(0, 0, 900, 640)
                                                  styleMask:NSWindowStyleMaskBorderless
                                                    backing:NSBackingStoreBuffered defer:NO];
  fixture.releasedWhenClosed = NO;
  fixture.contentView = root;
  const auto settle = [&view, root] {
    view->refresh();
    for (int i = 0; i < 4; ++i) { QCoreApplication::processEvents(QEventLoop::AllEvents); }
    [root.window layoutIfNeeded];
    [root layoutSubtreeIfNeeded];
  };
  const auto rect = [root](const QString& identifier) {
    NSView* control = identifiedView(root, identifier);
    return control == nil || [control isHiddenOrHasHiddenAncestor] ? NSZeroRect : frameIn(control, root);
  };
  const auto caption = [root](const QString& text) {
    NSView* found = findView(root, [&](NSView* candidate) {
      return [candidate isKindOfClass:NSTextField.class] && !((NSTextField*)candidate).editable &&
             qt(((NSTextField*)candidate).stringValue) == text;
    });
    return found == nil ? NSZeroRect : frameIn(found, root);
  };
  QString failure;
  const auto check = [&failure](bool passed, const QString& message) {
    if (!passed && failure.isEmpty()) { failure = message; }
  };
  const auto ambiguous = [](NSView* parent, const auto& self) -> NSView* {
    if (parent.hidden) return nil;
    if ([parent isKindOfClass:MitcadNativeLayoutView.class] && [parent hasAmbiguousLayout]) return parent;
    for (NSView* child in parent.subviews) {
      if (NSView* found = self(child, self)) return found;
    }
    return nil;
  };
  NSRect wideName = NSZeroRect;
  NSRect wideEmail = NSZeroRect;
  for (const CGFloat width : {CGFloat(900), CGFloat(640)}) {
    [fixture setContentSize:NSMakeSize(width, 640)];
    settle();
    const CGFloat actual = root.bounds.size.width;
    const NSRect onlyRect = rect(QStringLiteral("contract.only"));
    const NSRect searchRect = rect(QStringLiteral("contract.search"));
    const NSRect displayRect = rect(QStringLiteral("contract.display"));
    const NSRect nameRect = rect(QStringLiteral("contract.name"));
    const NSRect emailRect = rect(QStringLiteral("contract.email"));
    const NSRect spanRect = rect(QStringLiteral("contract.span"));
    const NSRect narrowRect = rect(QStringLiteral("contract.narrow"));
    const NSRect wideRect = rect(QStringLiteral("contract.wide"));
    const NSRect displayCaption = caption(QStringLiteral("Display name:"));
    const NSRect authorCaption = caption(QStringLiteral("Author:"));
    const QString at = QStringLiteral(" at width %1").arg(actual, 0, 'f', 0);
    const NSRect editorRect = rect(QStringLiteral("contract.fixed-editor"));
    const NSRect colorRect = rect(QStringLiteral("contract.stretched-color"));
    check(std::abs(NSWidth(editorRect) - 240) <= 0.5 &&
              std::abs(NSWidth(colorRect) - NSWidth(editorRect)) <= 0.5 &&
              std::abs(NSMinX(colorRect) - NSMinX(editorRect)) <= 0.5,
          QStringLiteral("zero-stretch spacer took the expanding colour input's editor width (%1, %2)%3")
              .arg(describeLayoutRect(editorRect), describeLayoutRect(colorRect), at));
    const NSRect textEditorRect = rect(QStringLiteral("contract.fixed-text-editor"));
    const NSRect textRect = rect(QStringLiteral("contract.stretched-text"));
    check(std::abs(NSWidth(textEditorRect) - 240) <= 0.5 &&
              std::abs(NSWidth(textRect) - NSWidth(textEditorRect)) <= 0.5 &&
              std::abs(NSMinX(textRect) - NSMinX(textEditorRect)) <= 0.5,
          QStringLiteral("zero-stretch spacer took the expanding text input's editor width (%1, %2)%3")
              .arg(describeLayoutRect(textEditorRect), describeLayoutRect(textRect), at));
    check(searchRect.size.width >= 179.5 && searchRect.size.width <= 260.5 && sameRow(onlyRect, searchRect) &&
              NSMaxX(onlyRect) <= NSMinX(searchRect) + 0.5 && NSMinX(onlyRect) > actual / 3,
          QStringLiteral("search after the check box lost its shared width or place (%1)%2")
              .arg(describeLayoutRect(searchRect), at));
    check(nameRect.size.width >= 80 && std::abs(nameRect.size.width - emailRect.size.width) <= 1 &&
              sameRow(nameRect, emailRect) && NSMaxX(nameRect) <= NSMinX(emailRect) + 0.5,
          QStringLiteral("stretched form fields are not equal halves (%1, %2)%3")
              .arg(describeLayoutRect(nameRect), describeLayoutRect(emailRect), at));
    check(std::abs(NSMinX(displayRect) - NSMinX(nameRect)) <= 1 &&
              (std::abs(NSMaxX(displayCaption) - NSMaxX(authorCaption)) <= 1 ||
               std::abs(NSMinX(displayCaption) - NSMinX(authorCaption)) <= 1) &&
              NSMaxX(authorCaption) <= NSMinX(nameRect) && displayCaption.size.width > 0,
          QStringLiteral("form labels and fields lost their column alignment%1").arg(at));
    for (int index = 0; index < 2; ++index) {
      const NSRect left = rect(QStringLiteral("contract.left%1").arg(index));
      const NSRect right = rect(QStringLiteral("contract.right%1").arg(index));
      check(left.size.width >= 69.5 && right.size.width >= 69.5 && std::abs(left.size.width - right.size.width) <= 2 &&
                NSMaxX(left) < NSMinX(right),
            QStringLiteral("side-by-side slider rows are not usable and symmetric (%1, %2)%3")
                .arg(describeLayoutRect(left), describeLayoutRect(right), at));
    }
    check(spanRect.size.width + 1 >= narrowRect.size.width + wideRect.size.width &&
              std::abs(wideRect.size.width - 2 * narrowRect.size.width) <= 2,
          QStringLiteral("grid span or column stretch was lost (%1, %2, %3)%4")
              .arg(describeLayoutRect(spanRect), describeLayoutRect(narrowRect), describeLayoutRect(wideRect), at));
    if (NSView* found = ambiguous(root, ambiguous)) {
      check(false, QStringLiteral("a converted layout is ambiguous (%1)%2").arg(describeLayoutRect(frameIn(found, root)), at));
    }
    if (width > 800) {
      wideName = nameRect;
      wideEmail = emailRect;
    }
    qInfo().noquote() << QStringLiteral("Native layout contract%1: search %2, name %3, email %4")
                             .arg(at, describeLayoutRect(searchRect), describeLayoutRect(nameRect), describeLayoutRect(emailRect));
  }
  // A hidden field leaves no space; its neighbour keeps its own width in a
  // form whose fields stay at their size hints, as in the Qt layout.
  [fixture setContentSize:NSMakeSize(900, 640)];
  email->hide();
  settle();
  const NSRect alone = rect(QStringLiteral("contract.name"));
  check(rect(QStringLiteral("contract.email")).size.width == 0 && alone.size.width >= 80 &&
            std::abs(alone.size.width - wideName.size.width) <= 1,
        QStringLiteral("a hidden field changed its neighbour's width (%1)").arg(describeLayoutRect(alone)));
  email->show();
  settle();
  const NSRect again = rect(QStringLiteral("contract.email"));
  check(std::abs(again.size.width - wideEmail.size.width) <= 1,
        QStringLiteral("a shown field did not regain its share (%1)").arg(describeLayoutRect(again)));
  NSView* nativeCategory = identifiedView(root, QStringLiteral("contract.category"));
  category->hide();
  settle();
  check(!categoryField->isHidden() && nativeCategory.constraints.count == 0,
        QStringLiteral("a hidden ancestor retained its descendant layout constraints"));
  category->show();
  settle();
  check(nativeCategory.constraints.count > 0 && rect(QStringLiteral("contract.category.field")).size.width >= 80,
        QStringLiteral("a restored ancestor did not rebuild its descendant layout constraints"));
  if (failure.isEmpty()) qInfo() << "Native layout suspension: hidden ancestor removed and restored descendant constraints";
  fixture.contentView = nil;
  [fixture close];
  if (!failure.isEmpty()) {
    error = failure;
    return false;
  }
  return true;
}

// A real native dialog whose insertion order differs from the role order.
bool buttonOrderContract(QString& error) {
  int accepted = 0;
  int rejected = 0;
  int helped = 0;
  QDialog dialog;
  dialog.setWindowTitle(QStringLiteral("Native button order regression"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* field = new QLineEdit;
  field->setObjectName(QStringLiteral("order.field"));
  layout->addWidget(field);
  auto* box = new QDialogButtonBox;
  QPushButton* primary = box->addButton(QStringLiteral("Primary"), QDialogButtonBox::AcceptRole);
  box->addButton(QStringLiteral("Dismiss"), QDialogButtonBox::RejectRole);
  QPushButton* helper = box->addButton(QStringLiteral("Helper"), QDialogButtonBox::ActionRole);
  QPushButton* restore = box->addButton(QStringLiteral("Restore"), QDialogButtonBox::ResetRole);
  box->addButton(QStringLiteral("Guide"), QDialogButtonBox::HelpRole);
  box->addButton(QStringLiteral("Discard"), QDialogButtonBox::DestructiveRole);
  box->addButton(QStringLiteral("Use"), QDialogButtonBox::ApplyRole);
  layout->addWidget(box);
  primary->setAutoDefault(true);
  primary->setDefault(true);
  QObject::connect(box, &QDialogButtonBox::accepted, &dialog, [&accepted] { ++accepted; });
  QObject::connect(box, &QDialogButtonBox::rejected, &dialog, [&rejected] { ++rejected; });
  QObject::connect(helper, &QPushButton::clicked, &dialog, [&helped] { ++helped; });
  const auto settle = [] {
    for (int i = 0; i < 4; ++i) { QCoreApplication::processEvents(QEventLoop::AllEvents); }
  };
  showNativeWidget(&dialog);
  settle();
  NSView* anchor = findControl(QStringLiteral("order.field"));
  NSWindow* window = anchor.window;
  const auto fail = [&](const QString& message) {
    error = message;
    dialog.reject();
    settle();
    return false;
  };
  if (window == nil) { return fail(QStringLiteral("the button order fixture has no native window")); }
  [NSApp activate];
  [window makeKeyAndOrderFront:nil];
  [window layoutIfNeeded];
  const auto button = [window](const QString& title) {
    return (NSButton*)buttonTitled(window.contentView, QRegularExpression(QStringLiteral("^%1$").arg(title)), false);
  };
  const auto key = [window](unsigned short code, NSString* characters) {
    NSEvent* event = [NSEvent keyEventWithType:NSEventTypeKeyDown location:NSZeroPoint modifierFlags:0
        timestamp:NSProcessInfo.processInfo.systemUptime windowNumber:window.windowNumber context:nil
        characters:characters charactersIgnoringModifiers:characters isARepeat:NO keyCode:code];
    return bool([window performKeyEquivalent:event]);
  };
  QString detail;
  if (!buttonOrderIn(window.contentView, {QStringLiteral("^Guide$"), QStringLiteral("^Restore$"), QStringLiteral("^Use$"),
                                          QStringLiteral("^Helper$"), QStringLiteral("^Discard$"),
                                          QStringLiteral("^Dismiss$"), QStringLiteral("^Primary$")}, detail)) {
    return fail(detail);
  }
  const NSRect helperRect = frameIn(button(QStringLiteral("Helper")), window.contentView);
  const NSRect discardRect = frameIn(button(QStringLiteral("Discard")), window.contentView);
  const NSRect dismissRect = frameIn(button(QStringLiteral("Dismiss")), window.contentView);
  if (NSMinX(discardRect) - NSMaxX(helperRect) < 36 || NSMinX(dismissRect) - NSMaxX(discardRect) < 36) {
    return fail(QStringLiteral("the destructive action lost its macOS gaps"));
  }
  if (![button(QStringLiteral("Dismiss")).keyEquivalent isEqualToString:@"\033"] ||
      ![button(QStringLiteral("Primary")).keyEquivalent isEqualToString:@"\r"] ||
      button(QStringLiteral("Helper")).keyEquivalent.length != 0) {
    return fail(QStringLiteral("Return and Escape are not on the default and rejecting actions"));
  }
  NSButton* helperButton = button(QStringLiteral("Helper"));
  const NSPoint centre = NSMakePoint(NSMidX(helperButton.bounds), NSMidY(helperButton.bounds));
  if (!pointerEvents(helperButton, centre, centre, detail)) { return fail(detail); }
  settle();
  if (helped != 1 || accepted != 0 || rejected != 0 || !dialog.isVisible()) {
    return fail(QStringLiteral("a helper action accepted or dismissed the dialog"));
  }
  key(36, @"\r");
  settle();
  if (accepted != 1 || rejected != 0) { return fail(QStringLiteral("Return did not trigger only the default action")); }
  primary->setEnabled(false);
  settle();
  key(36, @"\r");
  settle();
  if (accepted != 1 || !dialog.isVisible()) { return fail(QStringLiteral("Return accepted through a disabled default")); }
  primary->setEnabled(true);
  settle();
  key(53, @"\033");
  settle();
  if (rejected != 1 || accepted != 1) { return fail(QStringLiteral("Escape did not reject through the rejecting action")); }
  // Buttons added, hidden or given another role after creation.
  box->removeButton(helper);
  box->addButton(helper, QDialogButtonBox::RejectRole);
  box->addButton(QStringLiteral("Later"), QDialogButtonBox::ActionRole);
  restore->hide();
  settle();
  [window layoutIfNeeded];
  if (!buttonOrderIn(window.contentView, {QStringLiteral("^Guide$"), QStringLiteral("^Use$"), QStringLiteral("^Later$"),
                                          QStringLiteral("^Discard$"), QStringLiteral("^Helper$"),
                                          QStringLiteral("^Dismiss$"), QStringLiteral("^Primary$")}, detail)) {
    return fail(QStringLiteral("changed buttons kept a stale order: %1").arg(detail));
  }
  const NSRect guideRect = frameIn(button(QStringLiteral("Guide")), window.contentView);
  const NSRect useRect = frameIn(button(QStringLiteral("Use")), window.contentView);
  if (button(QStringLiteral("Restore")) != nil || NSMinX(useRect) - NSMaxX(guideRect) > 13) {
    return fail(QStringLiteral("a hidden helper action left a gap"));
  }
  if (![button(QStringLiteral("Helper")).keyEquivalent isEqualToString:@"\033"]) {
    return fail(QStringLiteral("a button given the rejecting role did not take Escape"));
  }
  dialog.reject();
  settle();
  qInfo().noquote() << QStringLiteral("Native button order contract: role order, gaps, keys, helpers and changes verified");
  return true;
}

} // namespace

bool layoutContract(QString& error) {
  if (!layoutWidthContract(error) || !columnSizingContract(error)) return false;
  qInfo().noquote() << QStringLiteral("Native layout contract: widths, stretch, spans, alignment and hiding verified");
  return buttonOrderContract(error);
}

} // namespace mitcad::mac::test
