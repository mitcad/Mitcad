// SPDX-License-Identifier: MIT
#import "NativeMessageBand.hpp"

#include <algorithm>
#include <cmath>
#include <vector>

#include <QLocale>
#include <QObject>
#include <QtLogging>

#include "NativeColors.hpp"
#include "framework/MessageLog.hpp"
#include "framework/NoticeLevel.hpp"
#include "framework/NoticePlaces.hpp"

namespace {

using mitcad::MessageEntry;
using mitcad::NoticeLevel;

constexpr CGFloat kInset = 10;     // the band's sides
constexpr CGFloat kSymbol = 16;    // the level's symbol
constexpr CGFloat kClose = 20;     // the × button
constexpr CGFloat kRowHeight = 22; // a history row
constexpr int kRowsLogged = 50;

NSString* ns(const QString& text) { return text.toNSString(); }

NSFont* bandFont() { return [NSFont systemFontOfSize:NSFont.systemFontSize]; }

CGFloat lineHeight() {
  NSLayoutManager* layout = [[NSLayoutManager alloc] init];
  return std::ceil([layout defaultLineHeightForFont:bandFont()]);
}

// The level's word and the text on one line, and the count of a message
// repeated at once.
QString shownText(const MessageEntry& entry, bool counted) {
  QString text = mitcad::noticeLevelText(entry.level, entry.text).simplified();
  if (counted && entry.count > 1) {
    text += QStringLiteral(" (%1%2)").arg(QChar(0x00D7)).arg(entry.count);
  }
  return text;
}

// The level's symbol in its tint; nil for a level without one.
NSImage* levelImage(NoticeLevel level) {
  const QString name = mitcad::noticeSymbolName(level);
  if (name.isEmpty()) return nil;
  NSImage* image = [NSImage imageWithSystemSymbolName:ns(name)
                             accessibilityDescription:ns(mitcad::noticeLevelWord(level))];
  return [image imageWithSymbolConfiguration:[NSImageSymbolConfiguration
                                                 configurationWithPaletteColors:@[ mitcad::mac::noticeTint(level) ]]];
}

// The text color of a level: the error's and the warning's text roles (the
// Qt band's; dynamic, for light and dark), the label color for the others.
NSColor* levelColor(NoticeLevel level) {
  using mitcad::ColorRole;
  if (level == NoticeLevel::Error) return mitcad::mac::roleColor(ColorRole::ErrorText);
  if (level == NoticeLevel::Warning) return mitcad::mac::roleColor(ColorRole::WarningText);
  return NSColor.labelColor;
}

NSAttributedString* titled(const QString& text, NSColor* color) {
  NSMutableParagraphStyle* paragraph = [[NSMutableParagraphStyle alloc] init];
  paragraph.lineBreakMode = NSLineBreakByTruncatingTail;
  return [[NSAttributedString alloc] initWithString:ns(text) attributes:@{
    NSFontAttributeName : bandFont(),
    NSForegroundColorAttributeName : color,
    NSParagraphStyleAttributeName : paragraph
  }];
}

NSButton* plainButton(id target, SEL action, Class kind = nil) {
  NSButton* button = [(kind != nil ? kind : NSButton.class) buttonWithTitle:@"" target:target action:action];
  button.bordered = NO;
  button.imagePosition = NSImageLeading;
  button.alignment = NSTextAlignmentLeft;
  button.lineBreakMode = NSLineBreakByTruncatingTail;
  button.translatesAutoresizingMaskIntoConstraints = YES;
  return button;
}

// A tracking area of its own over the visible part of a view, whose
// pointer's coming, moves and going the band hears (mitcad#1379), as
// MitcadCommandMessageField's in NativeForms.mm; the moves as the Qt band's
// mouse moves, which take a message shown under a still pointer.
NSTrackingArea* hoverTracking(NSView* view) {
  NSTrackingArea* area = [[NSTrackingArea alloc]
      initWithRect:NSZeroRect
           options:NSTrackingMouseEnteredAndExited | NSTrackingMouseMoved | NSTrackingActiveInActiveApp |
                   NSTrackingInVisibleRect
             owner:view
          userInfo:nil];
  [view addTrackingArea:area];
  return area;
}

} // namespace

// The history's rows, top down.
@interface MITCADMessageRows : NSView
@end

@implementation MITCADMessageRows
- (BOOL)isFlipped { return YES; }
@end

@interface MITCADMessageBand ()
// The pointer came over the text or the stub, or left it (mitcad#1379).
- (void)pointerOver:(NSView*)view entered:(BOOL)entered;
@end

// The message shown in full.
@interface MITCADMessageText : NSTextField {
@public
  MITCADMessageBand* __weak band;
}
@end

@implementation MITCADMessageText {
  NSTrackingArea* _hover;
}
- (void)updateTrackingAreas {
  [super updateTrackingAreas];
  if (_hover == nil) _hover = hoverTracking(self);
}
- (void)mouseEntered:(NSEvent*)event {
  (void)event;
  [band pointerOver:self entered:YES];
}
- (void)mouseMoved:(NSEvent*)event {
  (void)event;
  [band pointerOver:self entered:YES];
}
- (void)mouseExited:(NSEvent*)event {
  (void)event;
  [band pointerOver:self entered:NO];
}
@end

// The stub, the latest message on one line.
@interface MITCADMessageStub : NSButton {
@public
  MITCADMessageBand* __weak band;
}
@end

@implementation MITCADMessageStub {
  NSTrackingArea* _hover;
}
- (void)updateTrackingAreas {
  [super updateTrackingAreas];
  if (_hover == nil) _hover = hoverTracking(self);
}
- (void)mouseEntered:(NSEvent*)event {
  [super mouseEntered:event];
  [band pointerOver:self entered:YES];
}
- (void)mouseMoved:(NSEvent*)event {
  [super mouseMoved:event];
  [band pointerOver:self entered:YES];
}
- (void)mouseExited:(NSEvent*)event {
  [super mouseExited:event];
  [band pointerOver:self entered:NO];
}
@end

@implementation MITCADMessageBand {
  const mitcad::MessageLog* _log;
  NSImageView* _icon;
  MITCADMessageText* _text;
  NSButton* _close;
  MITCADMessageStub* _stub;
  NSPopover* _history;
  NSBox* _backdrop;
  std::vector<QString> _rowFeatures;
  QString _logged;
  // The text or the stub under the pointer, and the feature noticeHovered
  // was last called with (mitcad#1379; mitcad#1609: a message shown in its
  // place gives its own).
  NSView* __weak _hovered;
  mitcad::NoticeHover _noticeHover;
}

- (instancetype)initWithFrame:(NSRect)frame {
  self = [super initWithFrame:frame];
  if (self == nil) return nil;
  // An opaque window background under the message, not only the card's
  // glass: the levels' text colors keep their contrast over whatever the
  // canvas shows, in light and dark.
  _backdrop = [[NSBox alloc] initWithFrame:NSZeroRect];
  _backdrop.boxType = NSBoxCustom;
  _backdrop.borderWidth = 0;
  _backdrop.fillColor = NSColor.windowBackgroundColor;
  [self addSubview:_backdrop];
  _icon = [[NSImageView alloc] initWithFrame:NSZeroRect];
  _icon.imageScaling = NSImageScaleProportionallyUpOrDown;
  _text = [MITCADMessageText wrappingLabelWithString:@""];
  _text->band = self;
  _text.font = bandFont();
  _text.maximumNumberOfLines = 2;
  _text.lineBreakMode = NSLineBreakByTruncatingTail;
  _text.cell.truncatesLastVisibleLine = YES;
  _text.accessibilityIdentifier = @"native.messageBand.text";
  _close = [NSButton buttonWithImage:[NSImage imageWithSystemSymbolName:@"xmark"
                                                accessibilityDescription:ns(QObject::tr("Close message"))]
                              target:self action:@selector(closeMessage:)];
  _close.bordered = NO;
  _close.toolTip = ns(QObject::tr("Close the message"));
  _close.accessibilityIdentifier = @"native.messageBand.close";
  _stub = (MITCADMessageStub*)plainButton(self, @selector(toggleHistory:), MITCADMessageStub.class);
  _stub->band = self;
  _stub.accessibilityIdentifier = @"native.messageBand.stub";
  _stub.accessibilityLabel = ns(QObject::tr("Message history"));
  for (NSView* view in @[ _icon, _text, _close, _stub ]) {
    view.hidden = YES;
    [self addSubview:view];
  }
  return self;
}

- (BOOL)isFlipped { return YES; }

- (BOOL)hasMessage { return _log != nullptr && _log->shown() != mitcad::MessageLog::Shown::None; }
- (BOOL)isStub { return _log != nullptr && _log->shown() == mitcad::MessageLog::Shown::Stub; }

- (CGFloat)fullHeight { return 2 * lineHeight() + 16; }

- (NSSize)stubSize {
  const NSSize fitting = _stub.fittingSize;
  return NSMakeSize(std::ceil(fitting.width) + 2 * kInset + 4, std::max(lineHeight(), fitting.height) + 10);
}

- (void)showLog:(const mitcad::MessageLog*)log {
  _log = log;
  const mitcad::MessageEntry* full = log != nullptr ? log->full() : nullptr;
  // Null when another design cleared the band (mitcad#1174).
  const mitcad::MessageEntry* latest = log != nullptr ? log->stub() : nullptr;
  QString shown;
  if (full != nullptr) {
    NSImage* image = levelImage(full->level);
    _icon.image = image;
    _icon.hidden = image == nil;
    _icon.toolTip = ns(mitcad::noticeLevelWord(full->level));
    _text.stringValue = ns(shownText(*full, false));
    _text.textColor = levelColor(full->level);
    _text.toolTip = ns(mitcad::noticeLevelText(full->level, full->text));
    _text.hidden = NO;
    _close.hidden = NO;
    _stub.hidden = YES;
    shown = QStringLiteral("full %1 %2").arg(mitcad::noticeLevelName(full->level), full->text);
  } else if (latest != nullptr) {
    // Dimmed: the secondary label color whatever the level (its symbol
    // says it).
    _stub.image = levelImage(latest->level);
    _stub.attributedTitle = titled(shownText(*latest, true), NSColor.secondaryLabelColor);
    _stub.toolTip = ns(QObject::tr("%1\nClick for this session's messages.")
                           .arg(mitcad::noticeLevelText(latest->level, latest->text)));
    _icon.hidden = YES;
    _text.hidden = YES;
    _close.hidden = YES;
    _stub.hidden = NO;
    shown = QStringLiteral("stub %1 %2").arg(mitcad::noticeLevelName(latest->level), latest->text);
  } else {
    for (NSView* view in @[ _icon, _text, _close, _stub ]) view.hidden = YES;
    if (!_logged.isEmpty()) {
      // Emptied by another design (mitcad#1174); the next message is logged
      // again even when it is the same.
      _logged.clear();
      qDebug().noquote() << QStringLiteral("Native message band: none");
    }
  }
  if (!shown.isEmpty() && shown != _logged) {
    _logged = shown;
    qDebug().noquote() << QStringLiteral("Native message band: %1").arg(shown);
  }
  // A message taken from under the pointer no longer paints its places, as
  // the Qt band's hidden text or stub (mitcad#1379); another one shown in
  // its place paints its own, or none (mitcad#1609).
  NSView* hovered = _hovered;
  if (hovered != nil && hovered.hidden) _hovered = nil;
  [self updateNoticeHover];
  self.needsLayout = YES;
}

// The pointer over the text shown in full or the stub (mitcad#1379): the
// message's feature, as the Qt band's eventFilter takes it; a message
// about no feature paints nothing.
- (void)pointerOver:(NSView*)view entered:(BOOL)entered {
  if (entered) {
    _hovered = view;
  } else if (view == _hovered) {
    _hovered = nil;
  }
  [self updateNoticeHover];
}

// The feature of the message under the pointer, given when it changed: the
// one shown now, also when it took the hovered one's place (mitcad#1609).
- (void)updateNoticeHover {
  NSView* view = _hovered;
  const mitcad::MessageEntry* entry = nullptr;
  if (_log != nullptr && view != nil && !view.hidden) entry = view == _text ? _log->full() : _log->stub();
  if (_noticeHover.update(entry != nullptr, entry != nullptr ? entry->feature : QString())) {
    if (noticeHovered) noticeHovered(_noticeHover.feature());
  }
}

- (void)layout {
  [super layout];
  const NSRect bounds = self.bounds;
  _backdrop.frame = bounds;
  _backdrop.cornerRadius = std::min(CGFloat(14), NSHeight(bounds) / 2);
  if (!_stub.hidden) {
    _stub.frame = NSInsetRect(bounds, kInset, 0);
    return;
  }
  const CGFloat middle = NSMidY(bounds);
  _close.frame = NSMakeRect(NSMaxX(bounds) - kInset - kClose, middle - kClose / 2, kClose, kClose);
  CGFloat left = kInset;
  if (!_icon.hidden) {
    _icon.frame = NSMakeRect(left, middle - kSymbol / 2, kSymbol, kSymbol);
    left += kSymbol + 8;
  }
  const CGFloat width = std::max(CGFloat(40), NSMinX(_close.frame) - 8 - left);
  _text.preferredMaxLayoutWidth = width;
  const CGFloat high = std::min(NSHeight(bounds), std::ceil([_text.cell cellSizeForBounds:NSMakeRect(0, 0, width, 1000)].height));
  _text.frame = NSMakeRect(left, middle - high / 2, width, high);
}

- (void)closeMessage:(id)sender {
  (void)sender;
  if (closed) closed();
}

// The session's messages, newest first, in a popover above the stub.
- (void)toggleHistory:(id)sender {
  (void)sender;
  if (_history.shown) {
    [_history close];
    return;
  }
  if (_log == nullptr || self.window == nil) return;
  const std::deque<mitcad::MessageEntry>& entries = _log->entries();
  const NSRect window = self.window.contentView.bounds;
  const CGFloat wide = std::max(CGFloat(200), std::min(CGFloat(560), NSWidth(window) - 24));
  const CGFloat rowsHigh = kRowHeight * CGFloat(entries.size());
  NSTextField* title = [NSTextField labelWithString:ns(QObject::tr("Messages"))];
  title.font = [NSFont boldSystemFontOfSize:NSFont.systemFontSize];
  const CGFloat titleHigh = std::ceil(title.fittingSize.height);
  const CGFloat most = std::max(CGFloat(120), NSHeight(window) * 3 / 5);
  const CGFloat scrollHigh = std::min(rowsHigh, most - titleHigh - 22);
  NSView* content = [[MITCADMessageRows alloc] initWithFrame:NSMakeRect(0, 0, wide, titleHigh + scrollHigh + 22)];
  content.accessibilityIdentifier = @"native.messageHistory";
  // An opaque window background under the rows, not the popover's
  // translucent material: the levels' text colors keep their contrast over
  // whatever the canvas shows below, in light and dark.
  NSBox* backdrop = [[NSBox alloc] initWithFrame:content.bounds];
  backdrop.boxType = NSBoxCustom;
  backdrop.borderWidth = 0;
  backdrop.fillColor = NSColor.windowBackgroundColor;
  backdrop.autoresizingMask = NSViewWidthSizable | NSViewHeightSizable;
  [content addSubview:backdrop];
  title.frame = NSMakeRect(8, 8, wide - 16, titleHigh);
  [content addSubview:title];
  NSScrollView* scroll = [[NSScrollView alloc] initWithFrame:NSMakeRect(8, titleHigh + 14, wide - 16, scrollHigh)];
  scroll.hasVerticalScroller = YES;
  scroll.autohidesScrollers = YES;
  scroll.drawsBackground = NO;
  MITCADMessageRows* rows = [[MITCADMessageRows alloc] initWithFrame:NSMakeRect(0, 0, wide - 16, rowsHigh)];
  _rowFeatures.clear();
  NSImage* blank = [[NSImage alloc] initWithSize:NSMakeSize(kSymbol, kSymbol)];
  int number = 0;
  for (const mitcad::MessageEntry& entry : entries) {
    NSButton* row = plainButton(self, @selector(chooseRow:));
    NSImage* image = levelImage(entry.level);
    row.image = image != nil ? image : blank;
    const QString time = QLocale().toString(entry.time.time(), QStringLiteral("HH:mm:ss"));
    row.attributedTitle = titled(QStringLiteral("%1   %2").arg(time, shownText(entry, true)),
                                 mitcad::isProblem(entry.level) ? levelColor(entry.level) : NSColor.labelColor);
    QString tip = mitcad::noticeLevelText(entry.level, entry.text);
    if (!entry.feature.isEmpty()) {
      tip += QLatin1Char('\n') + QObject::tr("Click to select the feature.");
      row.identifier = ns(QStringLiteral("native.messageHistory.") + entry.feature);
    }
    row.toolTip = ns(tip);
    row.tag = number;
    row.accessibilityIdentifier = ns(QStringLiteral("native.messageHistory.row%1").arg(number + 1));
    row.frame = NSMakeRect(0, kRowHeight * number, wide - 16, kRowHeight);
    [rows addSubview:row];
    _rowFeatures.push_back(entry.feature);
    ++number;
  }
  scroll.documentView = rows;
  [content addSubview:scroll];
  NSViewController* controller = [[NSViewController alloc] init];
  controller.view = content;
  _history = [[NSPopover alloc] init];
  _history.behavior = NSPopoverBehaviorTransient;
  _history.animates = NO;
  _history.contentViewController = controller;
  _history.contentSize = content.frame.size;
  _history.delegate = self;
  [_history showRelativeToRect:_stub.frame ofView:self preferredEdge:NSRectEdgeMinY];
  qDebug().noquote() << QStringLiteral("Message history: %1 entries").arg(_log->size());
  int logged = 0;
  for (const mitcad::MessageEntry& entry : entries) {
    if (logged == kRowsLogged) break;
    ++logged;
    qDebug().noquote() << QStringLiteral("Message history row %1: %2 %3")
                              .arg(logged)
                              .arg(mitcad::noticeLevelName(entry.level), entry.text.simplified());
  }
}

- (void)chooseRow:(NSButton*)row {
  const auto at = static_cast<size_t>(row.tag);
  if (at >= _rowFeatures.size() || _rowFeatures[at].isEmpty()) return; // about no feature
  const QString feature = _rowFeatures[at];
  qDebug().noquote() << QStringLiteral("Message history: selected %1").arg(feature);
  [_history close];
  if (featureChosen) featureChosen(feature);
}

- (void)popoverDidClose:(NSNotification*)notification {
  (void)notification;
  qDebug().noquote() << QStringLiteral("Message history: closed");
}

@end
