// SPDX-License-Identifier: MIT
#include "platform/MacChrome.hpp"
#include "platform/MacTestApplication.hpp"

#import <AppKit/AppKit.h>
#import <objc/runtime.h>
#include "platform/NativeDialogs.hpp"
#ifdef MITCAD_MACOS_NATIVE_UI
#include "platform/NativeMacWindow.hpp"
#endif
#include <QPointer>

#import <CoreGraphics/CoreGraphics.h>
#include <dlfcn.h>
#include <unistd.h>

#include <cstdint>
#include <vector>

#include <QCoreApplication>
#include <QAction>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QGuiApplication>
#include <QHash>
#include <QIconEngine>
#include <QImage>
#include <QList>
#include <QMenu>
#include <QPainter>
#include <QPalette>
#include <QPixmap>
#include <QRectF>
#include <QSize>
#include <QStringList>
#include <QVariant>
#include <QWidget>

#ifdef MITCAD_MAC_UI
@interface MITCADDocumentVisibilityObserver : NSObject {
@public
  QPointer<QWidget> presenter;
  std::function<void(bool)> visible;
}
- (void)occlusionChanged:(NSNotification*)notification;
@end
@implementation MITCADDocumentVisibilityObserver
- (void)occlusionChanged:(NSNotification*)notification {
  NSWindow* window = notification.object;
  if (presenter && visible) visible((window.occlusionState & NSWindowOcclusionStateVisible) != 0);
}
- (void)dealloc { [NSNotificationCenter.defaultCenter removeObserver:self]; }
@end
#endif // MITCAD_MAC_UI

namespace mitcad::mac {
#ifdef MITCAD_MAC_UI
namespace {

NSView* viewOf(QWidget* widget) {
  // Offscreen Qt window ids are opaque integers, not Cocoa objects.
  if (!widget || QGuiApplication::platformName() != QStringLiteral("cocoa")) return nil;
  return (__bridge NSView*)reinterpret_cast<void*>(widget->winId());
}

// The glass effect view of a card window, if it has one.
NSView* glassViewOf(NSView* qtView) {
  if (@available(macOS 26.0, *)) {
    for (NSView* sibling in qtView.superview.subviews) {
      if ([sibling isKindOfClass:[NSGlassEffectView class]]) {
        return sibling;
      }
    }
  }
  return nil;
}

// An icon of one SF Symbol, rendered by AppKit when it is asked for and tinted
// with the palette's colour of the moment: the symbol is a template image, so
// only its shape counts.
class SymbolIconEngine : public QIconEngine {
public:
  explicit SymbolIconEngine(QString symbol) : m_symbol(std::move(symbol)) {}

  QIconEngine* clone() const override { return new SymbolIconEngine(m_symbol); }

  QString key() const override { return QStringLiteral("mitcad-symbol"); }

  // Any size; the sizes are listed so that the icon is not taken for empty.
  QList<QSize> availableSizes(QIcon::Mode, QIcon::State) override {
    return {QSize(16, 16), QSize(24, 24), QSize(32, 32), QSize(48, 48), QSize(64, 64)};
  }

  QSize actualSize(const QSize& size, QIcon::Mode, QIcon::State) override { return size; }

  QPixmap pixmap(const QSize& size, QIcon::Mode mode, QIcon::State state) override {
    return scaledPixmap(size, mode, state, 1.0);
  }

  QPixmap scaledPixmap(const QSize& size, QIcon::Mode mode, QIcon::State, qreal scale) override {
    const QSize device(qRound(size.width() * scale), qRound(size.height() * scale));
    if (device.isEmpty()) {
      return QPixmap();
    }
    const QPalette palette = QGuiApplication::palette();
    QColor color = palette.color(QPalette::Active, QPalette::ButtonText);
    if (mode == QIcon::Disabled) {
      color = palette.color(QPalette::Disabled, QPalette::ButtonText);
    } else if (mode == QIcon::Selected) {
      color = palette.color(QPalette::Active, QPalette::HighlightedText);
    }
    const QString key = QStringLiteral("%1x%2:%3:%4")
                            .arg(device.width())
                            .arg(device.height())
                            .arg(scale)
                            .arg(color.rgba(), 0, 16);
    const auto cached = m_cache.constFind(key);
    if (cached != m_cache.cend()) {
      return cached.value();
    }

    QImage image = render(device);
    if (image.isNull()) {
      return QPixmap();
    }
    {
      // Keeps the symbol's shape (its alpha) and takes the colour.
      QPainter painter(&image);
      painter.setCompositionMode(QPainter::CompositionMode_SourceIn);
      painter.fillRect(image.rect(), color);
    }
    QPixmap pixmap = QPixmap::fromImage(image);
    pixmap.setDevicePixelRatio(scale);
    if (m_cache.size() > 32) {
      m_cache.clear();
    }
    m_cache.insert(key, pixmap);
    return pixmap;
  }

  void paint(QPainter* painter, const QRect& rect, QIcon::Mode mode, QIcon::State state) override {
    const qreal ratio = painter->device() != nullptr ? painter->device()->devicePixelRatioF() : 1.0;
    painter->drawPixmap(rect, scaledPixmap(rect.size(), mode, state, ratio));
  }

private:
  // The symbol, in black, fitted and centred in an image of `device` pixels.
  QImage render(const QSize& device) const {
    NSImage* symbol = [NSImage imageWithSystemSymbolName:m_symbol.toNSString() accessibilityDescription:nil];
    if (symbol == nil) {
      return QImage();
    }
    QImage image(device, QImage::Format_ARGB32_Premultiplied);
    image.fill(Qt::transparent);
    CGColorSpaceRef space = CGColorSpaceCreateDeviceRGB();
    CGContextRef context = CGBitmapContextCreate(
        image.bits(), size_t(device.width()), size_t(device.height()), 8, size_t(image.bytesPerLine()), space,
        kCGImageAlphaPremultipliedFirst | kCGBitmapByteOrder32Host);
    CGColorSpaceRelease(space);
    if (context == nullptr) {
      return QImage();
    }
    // Flipped, so that the image's top row is the top of the drawing.
    NSGraphicsContext* graphics = [NSGraphicsContext graphicsContextWithCGContext:context flipped:YES];
    [NSGraphicsContext saveGraphicsState];
    NSGraphicsContext.currentContext = graphics;
    CGContextTranslateCTM(context, 0, device.height());
    CGContextScaleCTM(context, 1, -1);

    const NSSize natural = symbol.size;
    const double fit = natural.width > 0 && natural.height > 0
                           ? qMin(device.width() / natural.width, device.height() / natural.height)
                           : 1.0;
    const NSSize drawn = NSMakeSize(natural.width * fit, natural.height * fit);
    const NSRect target = NSMakeRect((device.width() - drawn.width) / 2, (device.height() - drawn.height) / 2,
                                     drawn.width, drawn.height);
    [symbol drawInRect:target
              fromRect:NSZeroRect
             operation:NSCompositingOperationSourceOver
              fraction:1.0
        respectFlipped:YES
                 hints:nil];
    [NSGraphicsContext restoreGraphicsState];
    CGContextRelease(context);
    return image;
  }

  QString m_symbol;
  QHash<QString, QPixmap> m_cache;
};

void applyImageVisibility(NSMenu* menu) {
  if (@available(macOS 27.0, *)) {
    for (NSMenuItem* item in menu.itemArray) {
      item.preferredImageVisibility = NSMenuItemImageVisibilityVisible;
      if (item.submenu != nil) {
        applyImageVisibility(item.submenu);
      }
    }
  }
}

void applyImageHiding(NSMenu* menu) {
  for (NSMenuItem* item in menu.itemArray) {
    if (@available(macOS 27.0, *)) {
      item.preferredImageVisibility = NSMenuItemImageVisibilityHidden;
    }
    item.image = nil;
    if (item.submenu != nil) {
      applyImageHiding(item.submenu);
    }
  }
}

} // namespace
#endif // MITCAD_MAC_UI

bool headlessTestApplicationIsolated(QString& error) {
  if (NSApp == nil || NSApp.activationPolicy != NSApplicationActivationPolicyProhibited || NSApp.active) {
    error = QStringLiteral("headless macOS application must prohibit activation and remain inactive (policy=%1 active=%2)")
        .arg(NSApp != nil ? int(NSApp.activationPolicy) : -1).arg(bool(NSApp.active));
    return false;
  }
  for (NSWindow* window in NSApp.windows) {
    if (window.visible) {
      error = QStringLiteral("headless macOS application presented a native window");
      return false;
    }
  }
  qInfo("Headless macOS application verified: activation prohibited, inactive, no visible native windows");
  return true;
}

bool prepareHeadlessTestApplication(QString& error) {
  // A bundled executable otherwise has the Regular policy even with Qt's
  // offscreen platform. Accessory is not enough: it can still activate.
  NSApplication* application = NSApplication.sharedApplication;
  if ([application activationPolicy] != NSApplicationActivationPolicyProhibited &&
      ![application setActivationPolicy:NSApplicationActivationPolicyProhibited]) {
    error = QStringLiteral("could not prohibit activation for the headless macOS application");
    return false;
  }
  return headlessTestApplicationIsolated(error);
}

#ifdef MITCAD_MAC_UI
QIcon symbolIcon(const QString& symbol, const QIcon& fallback) {
  if (symbol.isEmpty() || [NSImage imageWithSystemSymbolName:symbol.toNSString() accessibilityDescription:nil] == nil) {
    return fallback;
  }
  return QIcon(new SymbolIconEngine(symbol));
}

void showMenuImages(QMenu* menu) {
  if (menu == nullptr) {
    return;
  }
  if (@available(macOS 27.0, *)) {
    applyImageVisibility(menu->toNSMenu());
    if (menu->property("mitcad.showMenuImagesConnected").toBool()) return;
    menu->setProperty("mitcad.showMenuImagesConnected", true);
    // The native items are made (or made again) as the menu is shown.
    QObject::connect(menu, &QMenu::aboutToShow, menu, [menu] { applyImageVisibility(menu->toNSMenu()); });
  }
}

void prepareDocumentWindowPresentation(QWidget* widget, bool separate) {
  if (!widget) return;
  NSWindow* window = (__bridge NSWindow*)nativeDialogWindow(widget);
  if (!window) window = viewOf(widget).window;
  NSWindow.allowsAutomaticWindowTabbing = YES;
  window.tabbingIdentifier = @"org.mitcad.document";
  window.tabbingMode = separate ? NSWindowTabbingModeDisallowed : NSWindowTabbingModeAutomatic;
}

void minimizeDocumentWindow(QWidget* widget) {
  if (!widget) return;
  NSWindow* window = (__bridge NSWindow*)nativeDialogWindow(widget);
  if (!window) window = viewOf(widget).window;
  [window performMiniaturize:nil];
}

void enableDocumentWindowTabs(QWidget* widget, std::function<void(bool)> visibility) {
  if (!widget) return;
  NSWindow* window = viewOf(widget).window;
  if (!window) return;
  NSWindow.allowsAutomaticWindowTabbing = YES;
  window.tabbingMode = NSWindowTabbingModeAutomatic;
  window.tabbingIdentifier = @"org.mitcad.document";
  static char observerKey;
  auto* observer = (MITCADDocumentVisibilityObserver*)objc_getAssociatedObject(window, &observerKey);
  if (!observer) {
    observer = [MITCADDocumentVisibilityObserver new];
    observer->presenter = widget;
    [NSNotificationCenter.defaultCenter addObserver:observer selector:@selector(occlusionChanged:)
                                               name:NSWindowDidChangeOcclusionStateNotification object:window];
    objc_setAssociatedObject(window, &observerKey, observer, OBJC_ASSOCIATION_RETAIN_NONATOMIC);
  }
  observer->visible = std::move(visibility);
}

int documentWindowTabCount(QWidget* widget) {
  if (!widget) return 0;
  NSWindow* window = (__bridge NSWindow*)nativeDialogWindow(widget);
  if (!window) window = viewOf(widget).window;
  if (!window) return 0;
  return window.tabbedWindows ? int(window.tabbedWindows.count) : 1;
}

bool documentWindowTabSelected(QWidget* widget) {
  if (!widget || QGuiApplication::platformName() != QStringLiteral("cocoa")) return false;
  NSWindow* window = (__bridge NSWindow*)nativeDialogWindow(widget);
  if (!window) {
    // Observation must not create a Qt native handle or present a window.
    const WId handle = widget->internalWinId();
    NSView* view = handle ? (__bridge NSView*)reinterpret_cast<void*>(handle) : nil;
    window = view.window;
  }
  if (!window || ![NSApp.windows containsObject:window]) return false;
  NSWindowTabGroup* group = window.tabGroup;
  return group.windows.count <= 1 || group.selectedWindow == window;
}

bool triggerDocumentWindowMenu(QWidget* presenter, const QString& entry, QString& error) {
  error.clear();
  if (qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT") || !presenter) {
    error = QStringLiteral("a UI script and an owned document presenter are required");
    return false;
  }
  QPointer<QWidget> guarded = presenter;
  NSWindow* bound = (__bridge NSWindow*)nativeDialogWindow(presenter);
  NSWindow* window = bound ? bound : viewOf(presenter).window;
  if (!window || ![NSApp.windows containsObject:window]) {
    error = QStringLiteral("the document presenter has no owned application window");
    return false;
  }
  // Resolve the presenter's own menu, independently of the foreground menu
  // installed by another document. This is programmatic target-action input,
  // not an assertion about WindowServer focus or physical menu tracking.
  NSMenu* menu = nil;
  if (bound) {
#ifdef MITCAD_MACOS_NATIVE_UI
    menu = (__bridge NSMenu*)nativeDocumentWindowMenu((__bridge void*)window);
#endif
  } else {
    QMenu* qtMenu = presenter->findChild<QMenu*>(QStringLiteral("windowMenu"));
    if (qtMenu) {
      menu = qtMenu->toNSMenu();
    }
  }
  [menu update]; // normal AppKit validation against the menu items' actual targets
  if (!guarded || ![NSApp.windows containsObject:window] || !menu) {
    error = QStringLiteral("the document presenter has no owned Window menu");
    return false;
  }
  const auto normalized = [](QString text) { return text.remove(QLatin1Char('&')).trimmed(); };
  NSMenuItem* selected = nil;
  QStringList available;
  for (NSMenuItem* item in menu.itemArray) {
    if (!item.separatorItem) available << QString::fromNSString(item.title);
    if (normalized(QString::fromNSString(item.title)) != normalized(entry)) continue;
    if (selected) {
      error = QStringLiteral("the owned Window menu entry '%1' is ambiguous").arg(entry);
      return false;
    }
    selected = item;
  }
  if (!selected) {
    error = QStringLiteral("the owned Window menu has no entry '%1' (entries: %2)")
                .arg(entry, available.join(QStringLiteral(" | ")));
    return false;
  }
  if (selected.hidden || selected.separatorItem || selected.submenu || !selected.enabled || !selected.action) {
    error = QStringLiteral("the owned Window menu entry '%1' is not actionable "
                           "(hidden %2, separator %3, submenu %4, enabled %5, action '%6', explicit target %7; "
                           "native document %8, owned window tabs %9, visible %10, key %11, application active %12, "
                           "menu auto-enables %13)")
                .arg(entry).arg(bool(selected.hidden)).arg(bool(selected.separatorItem)).arg(selected.submenu != nil)
                .arg(bool(selected.enabled))
                .arg(selected.action ? QString::fromNSString(NSStringFromSelector(selected.action)) : QStringLiteral("<none>"))
                .arg(selected.target != nil).arg(bound != nil)
                .arg(window.tabbedWindows ? int(window.tabbedWindows.count) : 1)
                .arg(bool(window.visible)).arg(NSApp.keyWindow == window).arg(bool(NSApp.active))
                .arg(bool(menu.autoenablesItems));
    return false;
  }
  id target = selected.target;
  if (bound) {
    // Observe the production item's owner instead of repairing it in the
    // test helper: validation and dispatch must use that same real target.
    id expected = nil;
    const SEL action = selected.action;
    if (action == @selector(toggleTabBar:) || action == @selector(toggleTabOverview:) ||
        action == @selector(mergeAllWindows:) || action == @selector(performZoom:)) expected = window;
    else if (action == @selector(arrangeInFront:)) expected = NSApp;
    if (!expected || target != expected) {
      error = QStringLiteral("the native Window menu entry '%1' has no matching owned system target").arg(entry);
      return false;
    }
  } else target = [NSApp targetForAction:selected.action to:target from:selected];
  // Qt's actual NSMenuItem routes to its normal menu-action receiver, whose
  // QAction callback already retains the owning presenter by QPointer.
  if (!target || ![target respondsToSelector:selected.action] ||
      ![NSApp sendAction:selected.action to:target from:selected]) {
    error = QStringLiteral("the owned Window menu entry '%1' could not dispatch").arg(entry);
    return false;
  }
  return true;
}

void addDocumentWindowTabActions(QMenu* menu, QWidget* presenter) {
  if (!menu || !presenter) return;
  QPointer<QWidget> guarded = presenter;
  const auto add = [menu, guarded](const QString& label, SEL selector) {
    QAction* action = menu->addAction(label);
    QObject::connect(action, &QAction::triggered, menu, [guarded, selector] {
      if (!guarded) return;
      NSWindow* window = viewOf(guarded).window;
      if (selector == @selector(arrangeInFront:)) [NSApp arrangeInFront:nil];
      else if (window) [NSApp sendAction:selector to:window from:nil];
    });
  };
  menu->addSeparator();
  add(QObject::tr("Show Tab Bar"), @selector(toggleTabBar:));
  add(QObject::tr("Show All Tabs"), @selector(toggleTabOverview:));
  add(QObject::tr("Merge All Windows"), @selector(mergeAllWindows:));
  add(QObject::tr("Zoom"), @selector(performZoom:));
  add(QObject::tr("Bring All to Front"), @selector(arrangeInFront:));
}

void hideNativeTitle(QWidget* window) {
  if (window == nullptr) {
    return;
  }
  NSView* view = viewOf(window);
  NSWindow* native = view.window;
  if (native == nil) {
    return;
  }
  native.tabbingMode = NSWindowTabbingModeAutomatic;
  native.tabbingIdentifier = @"org.mitcad.document";
  native.titleVisibility = NSWindowTitleHidden;
  native.titlebarAppearsTransparent = YES;
}

void hideMenuImages(QMenu* menu) {
  if (menu == nullptr) {
    return;
  }
  // The submenus make their native items when they show, which the parent's
  // aboutToShow does not reach.
  QList<QMenu*> menus = menu->findChildren<QMenu*>();
  menus.prepend(menu);
  for (QMenu* each : menus) {
    applyImageHiding(each->toNSMenu());
    if (each->property("mitcad.hideMenuImagesConnected").toBool()) continue;
    each->setProperty("mitcad.hideMenuImagesConnected", true);
    QObject::connect(each, &QMenu::aboutToShow, each, [each] { applyImageHiding(each->toNSMenu()); });
  }
}

bool glassAvailable() {
  if (QGuiApplication::platformName() != QStringLiteral("cocoa")) return false;
  if (@available(macOS 26.0, *)) {
    return NSClassFromString(@"NSGlassEffectView") != nil;
  }
  return false;
}

void setGlassCornerRadius(QWidget* card, qreal cornerRadius) {
  if (@available(macOS 26.0, *)) {
    NSView* view = viewOf(card);
    if (auto* glass = static_cast<NSGlassEffectView*>(glassViewOf(view))) {
      if (glass.cornerRadius != cornerRadius) {
        glass.cornerRadius = cornerRadius;
        [view.window invalidateShadow];
      }
    }
  }
}

void attachGlassWindow(QWidget* card, QWidget* mainWindow, qreal cornerRadius) {
  if (@available(macOS 26.0, *)) {
    if (card == nullptr || mainWindow == nullptr || !glassAvailable()) {
      return;
    }
    NSView* view = viewOf(card);
    NSWindow* native = view.window;
    NSWindow* parent = viewOf(mainWindow).window;
    if (native == nil || parent == nil) {
      return;
    }
    // Transparent window, the system's shadow and rounded glass.
    native.opaque = NO;
    native.backgroundColor = NSColor.clearColor;
    native.hasShadow = YES;
    // A tool window is a panel that hides when the application is not
    // active; the cards stay with the main window.
    if ([native isKindOfClass:[NSPanel class]]) {
      static_cast<NSPanel*>(native).hidesOnDeactivate = NO;
    }
    // Not a window of its own for the user: not in the Window menu, not in
    // the window cycle or Exposé, and in the full screen space of its parent.
    native.excludedFromWindowsMenu = YES;
    native.collectionBehavior = NSWindowCollectionBehaviorTransient | NSWindowCollectionBehaviorIgnoresCycle |
                                NSWindowCollectionBehaviorFullScreenAuxiliary;
    if (glassViewOf(view) == nil) {
      NSGlassEffectView* glass = [[NSGlassEffectView alloc] initWithFrame:view.frame];
      glass.cornerRadius = cornerRadius;
      glass.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
      // Below Qt's view, which draws the card's content (and nothing else)
      // over it; the glass itself cannot host Qt's view.
      [view.superview addSubview:glass positioned:NSWindowBelow relativeTo:view];
    } else {
      setGlassCornerRadius(card, cornerRadius);
    }
    if (native.parentWindow != parent) {
      [parent addChildWindow:native ordered:NSWindowAbove];
    }
    [native invalidateShadow];
  }
}

#endif // MITCAD_MAC_UI

bool captureOwnWindows(const QString& path, QRectF* capturedBounds) {
  // Obsoleted in the macOS 15 SDK (screen capture moved to ScreenCaptureKit,
  // which needs the user's permission), still there and enough for the
  // application's own windows: looked up at run time.
  using CreateImageFromArray = CGImageRef (*)(CGRect, CFArrayRef, CGWindowImageOption);
  auto fromArray = reinterpret_cast<CreateImageFromArray>(dlsym(RTLD_DEFAULT, "CGWindowListCreateImageFromArray"));
  if (fromArray == nullptr) {
    return false;
  }
  std::vector<const void*> numbers; // the window numbers themselves, not objects
  CGRect bounds = CGRectNull;
  NSArray* infos = CFBridgingRelease(CGWindowListCopyWindowInfo(kCGWindowListOptionOnScreenOnly, kCGNullWindowID));
  const pid_t self = getpid();
  for (NSDictionary* info in infos) {
    if ([info[(id)kCGWindowOwnerPID] intValue] != self) {
      continue;
    }
    CGRect rect;
    if (!CGRectMakeWithDictionaryRepresentation((__bridge CFDictionaryRef)info[(id)kCGWindowBounds], &rect) ||
        rect.size.width < 8 || rect.size.height < 8) {
      continue;
    }
    numbers.push_back(reinterpret_cast<const void*>(uintptr_t([info[(id)kCGWindowNumber] unsignedIntValue])));
    bounds = CGRectUnion(bounds, rect);
  }
  if (numbers.empty()) {
    return false;
  }
  // The shadows are outside the windows' bounds.
  bounds = CGRectInset(bounds, -48, -48);
  CFArrayRef windows = CFArrayCreate(nullptr, numbers.data(), CFIndex(numbers.size()), nullptr);
  CGImageRef image = fromArray(bounds, windows, kCGWindowImageBestResolution);
  CFRelease(windows);
  if (image == nullptr) {
    return false;
  }
  QImage result(int(CGImageGetWidth(image)), int(CGImageGetHeight(image)), QImage::Format_ARGB32_Premultiplied);
  result.fill(Qt::transparent);
  CGColorSpaceRef space = CGColorSpaceCreateDeviceRGB();
  CGContextRef context = CGBitmapContextCreate(result.bits(), size_t(result.width()), size_t(result.height()), 8,
                                               size_t(result.bytesPerLine()), space,
                                               kCGImageAlphaPremultipliedFirst | kCGBitmapByteOrder32Host);
  CGColorSpaceRelease(space);
  if (context != nullptr) {
    CGContextDrawImage(context, CGRectMake(0, 0, result.width(), result.height()), image);
    CGContextRelease(context);
  }
  CGImageRelease(image);
  const bool saved = context != nullptr && result.save(path);
  if (saved && capturedBounds != nullptr) {
    *capturedBounds = QRectF(bounds.origin.x, bounds.origin.y, bounds.size.width, bounds.size.height);
  }
  return saved;
}

#ifdef MITCAD_MAC_UI
bool chooseTestPopup(const QString& title, const std::function<void()>& open,
                     const std::function<bool()>& qtPopupVisible, QString& error,
                     const std::function<void()>& nativeClosed) {
  if (qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT")) {
    error = QStringLiteral("native popup instrumentation requires the in-process UI driver");
    return false;
  }
  __block NSMenu* tracked = nil;
  __block NSTimer* response = nil;
  __block bool began = false;
  __block bool ended = false;
  __block bool selected = false;
  __block QStringList available;
  const auto text = [](NSString* value) { return QString::fromNSString(value).remove(QLatin1Char('&')).trimmed(); };
  const QString wanted = QString(title).remove(QLatin1Char('&')).trimmed();
  NSNotificationCenter* center = NSNotificationCenter.defaultCenter;
  id begin = [center addObserverForName:NSMenuDidBeginTrackingNotification object:nil queue:nil
      usingBlock:^(NSNotification* notification) {
    if (began || ![notification.object isKindOfClass:NSMenu.class]) return;
    began = true;
    tracked = notification.object;
    response = [NSTimer timerWithTimeInterval:0.05 repeats:NO block:^(NSTimer* timer) {
      (void)timer;
      [tracked update];
      for (NSInteger index = 0; index < tracked.numberOfItems; ++index) {
        NSMenuItem* item = [tracked itemAtIndex:index];
        available << QStringLiteral("%1 (enabled=%2)").arg(text(item.title)).arg(bool(item.enabled));
        if (text(item.title) == wanted && item.enabled && item.action && !item.submenu && !item.separatorItem) {
          selected = true;
          [tracked performActionForItemAtIndex:index];
          break;
        }
      }
      [tracked cancelTracking];
    }];
    [NSRunLoop.currentRunLoop addTimer:response forMode:NSEventTrackingRunLoopMode];
    [NSRunLoop.currentRunLoop addTimer:response forMode:NSRunLoopCommonModes];
  }];
  id end = [center addObserverForName:NSMenuDidEndTrackingNotification object:nil queue:nil
      usingBlock:^(NSNotification* notification) {
    if (notification.object != tracked || ended) return;
    ended = true;
    if (selected && nativeClosed) nativeClosed();
  }];
  open();
  QElapsedTimer deadline;
  deadline.start();
  while (!ended && (began || !qtPopupVisible()) && deadline.elapsed() < 6000) {
    QCoreApplication::processEvents(QEventLoop::AllEvents, 25);
    [NSRunLoop.currentRunLoop runMode:NSDefaultRunLoopMode beforeDate:[NSDate dateWithTimeIntervalSinceNow:0.01]];
  }
  [response invalidate];
  [center removeObserver:begin];
  [center removeObserver:end];
  if (!began && qtPopupVisible()) return false;
  if (!began || !ended || !selected) {
    if (tracked) [tracked cancelTracking];
    error = QStringLiteral("native Qt choice did not select enabled '%1' after complete tracking: %2")
        .arg(title, available.join(QStringLiteral(" | ")));
    return false;
  }
  return true;
}

void showAboutPanel(const QString& credits) {
  NSMutableDictionary* options = [NSMutableDictionary dictionary];
  // The program's name, not the application name "Mitcad" that keys the
  // settings (mitcad#822).
  options[NSAboutPanelOptionApplicationName] = @"MitCad";
  options[NSAboutPanelOptionApplicationVersion] = QCoreApplication::applicationVersion().toNSString();
  // The credits' HTML in the system font and colours (the colour of labels
  // follows the light and dark appearance).
  const QString html = QStringLiteral("<style>body { font-family: -apple-system; font-size: 11px; } "
                                      "ul { margin-top: 0; padding-left: 14px; }</style>%1")
                           .arg(credits);
  NSData* data = [html.toNSString() dataUsingEncoding:NSUTF8StringEncoding];
  NSMutableAttributedString* text = [[NSMutableAttributedString alloc]
      initWithHTML:data
           options:@{NSCharacterEncodingDocumentAttribute : @(NSUTF8StringEncoding)}
documentAttributes:nil];
  if (text != nil) {
    [text addAttribute:NSForegroundColorAttributeName
                 value:NSColor.labelColor
                 range:NSMakeRange(0, text.length)];
    options[NSAboutPanelOptionCredits] = text;
  }
  [NSApp orderFrontStandardAboutPanelWithOptions:options];
}

#endif // MITCAD_MAC_UI
} // namespace mitcad::mac
