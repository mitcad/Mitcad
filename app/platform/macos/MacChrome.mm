// SPDX-License-Identifier: MIT
#include "platform/MacChrome.hpp"

#import <AppKit/AppKit.h>

#import <CoreGraphics/CoreGraphics.h>
#include <dlfcn.h>
#include <unistd.h>

#include <cstdint>
#include <vector>

#include <QCoreApplication>
#include <QGuiApplication>
#include <QHash>
#include <QIconEngine>
#include <QImage>
#include <QList>
#include <QMenu>
#include <QPainter>
#include <QPalette>
#include <QPixmap>
#include <QSize>
#include <QWidget>

namespace mitcad::mac {
namespace {

NSView* viewOf(QWidget* widget) { return (__bridge NSView*)reinterpret_cast<void*>(widget->winId()); }

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
    // The native items are made (or made again) as the menu is shown.
    QObject::connect(menu, &QMenu::aboutToShow, menu, [menu] { applyImageVisibility(menu->toNSMenu()); });
  }
}

void hideNativeTitle(QWidget* window) {
  if (window == nullptr) {
    return;
  }
  NSView* view = (__bridge NSView*)reinterpret_cast<void*>(window->winId());
  NSWindow* native = view.window;
  if (native == nil) {
    return;
  }
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
    QObject::connect(each, &QMenu::aboutToShow, each, [each] { applyImageHiding(each->toNSMenu()); });
  }
}

bool glassAvailable() {
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

bool captureOwnWindows(const QString& path) {
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
  return context != nullptr && result.save(path);
}

void showAboutPanel(const QString& credits) {
  NSMutableDictionary* options = [NSMutableDictionary dictionary];
  options[NSAboutPanelOptionApplicationName] = QGuiApplication::applicationDisplayName().toNSString();
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

} // namespace mitcad::mac
