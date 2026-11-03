// SPDX-License-Identifier: MIT
#include "platform/NativeMacWindow.hpp"
#include "NativeKeys.hpp"
#include "NativeCardWidths.hpp"
#include "NativeChatCard.hpp"
#include "NativeColumnSplit.hpp"
#include "NativeColors.hpp"
#include "NativeMessageBand.hpp"
#include "NativeProjects.hpp"
#include "framework/Icons.hpp"
#include "framework/PanelTree.hpp"
#include "framework/Theme.hpp"
#include "browser/TimelinePresentation.hpp"

#import <AppKit/AppKit.h>
#import <QuartzCore/QuartzCore.h>

#include <algorithm>
#include <cmath>
#include <cstring>
#include <limits>
#include <utility>

#include <QApplication>
#include <QHash>
#include <QKeySequence>
#include <QImage>
#include <QLineEdit>
#include <QPlainTextEdit>
#include <QTextEdit>
#include <QAbstractSpinBox>
#include <QNativeGestureEvent>
#include <QOpenGLWidget>
#include <QPointingDevice>
#include <QPointer>
#include <QPalette>
#include <QSet>
#include <QWidget>
#include <QWindow>
#include <QtLogging>

namespace mitcad::mac {
struct NativeMacWindowState;
}

@interface MITCADDocumentWindow : NSWindow {
@public
  mitcad::mac::NativeMacWindowState* state;
}
- (BOOL)performKeyEquivalent:(NSEvent*)event;
- (BOOL)dispatchConfirmation;
- (BOOL)dispatchCancellation;
- (BOOL)dispatchKeyboardEvent:(NSEvent*)event;
@end

@interface MITCADViewportHost : NSView {
@public
  mitcad::mac::NativeMacWindowState* state;
}
@end

@interface MITCADCanvasRoot : NSView {
@public
  mitcad::mac::NativeMacWindowState* state;
}
@end

@interface MITCADCommandButton : NSButton
@property(nonatomic, copy) NSString* representedObject;
@end
@implementation MITCADCommandButton
@end

// A binding badge is its own native hit target, preserving row selection
// and timeline dragging everywhere outside the small fx pill.
@interface MITCADBindingBadge : MITCADCommandButton {
@public
  mitcad::mac::NativeMacWindowState* state;
  bool history;
  NSTimer* __strong hoverTimer;
  NSTrackingArea* __strong tracking;
}
- (void)requestCard:(BOOL)pinned;
- (void)pinCard:(id)sender;
@end

// The outer view owns the shadow; the system material owns its content.
@interface MITCADFloatingCard : NSView
@property(nonatomic, strong) NSView* body;
@property(nonatomic) CGFloat cornerRadius;
@end

@implementation MITCADFloatingCard
- (BOOL)isFlipped { return YES; }
- (NSView*)hitTest:(NSPoint)point {
  NSPoint local = [self convertPoint:point fromView:self.superview];
  if (![[NSBezierPath bezierPathWithRoundedRect:self.bounds xRadius:self.cornerRadius yRadius:self.cornerRadius]
          containsPoint:local]) return nil;
  return [super hitTest:point];
}
@end

// A navigation bar button (mitcad#1099): checked (Layout Grid, Snap to
// Grid on) tinted and outlined in the accent colour, as the Qt bar's.
@interface MITCADNavigationButton : MITCADCommandButton
@end

@implementation MITCADNavigationButton
- (void)drawRect:(NSRect)dirty {
  if (self.state == NSControlStateValueOn) {
    NSBezierPath* outline = [NSBezierPath bezierPathWithRoundedRect:NSInsetRect(self.bounds, 0.5, 0.5)
                                                            xRadius:6 yRadius:6];
    [[NSColor.controlAccentColor colorWithAlphaComponent:0.2] setFill];
    [outline fill];
    [NSColor.controlAccentColor setStroke];
    [outline stroke];
  }
  [super drawRect:dirty];
}
@end

@interface MITCADModelOutlineView : NSOutlineView {
@public
  mitcad::mac::NativeMacWindowState* state;
  NSTrackingArea* __strong hoverArea; // the row under the pointer (mitcad#1072)
}
@end

@interface MITCADHistoryStrip : NSView {
@public
  mitcad::mac::NativeMacWindowState* state;
}
@end

// The timeline has one scrolling axis. A vertical wheel, including over a
// feature button, moves along that axis just like the shared Qt timeline.
@interface MITCADHistoryScroll : NSScrollView
@end

@interface MITCADHistoryFeatureButton : NSButton <NSDraggingSource> {
@public
  mitcad::mac::NativeMacWindowState* state;
  NSPoint pressPoint;
  BOOL pressed;
  NSTrackingArea* hoverTracking; // the timeline's hover (mitcad#1076)
}
@property(nonatomic, copy) NSString* featureKey;
@property(nonatomic, copy) NSString* statusKey;
- (void)selectHistoryFeature;
- (void)activateHistoryFeature;
- (void)reorderHistoryFeatureToIndex:(NSInteger)index;
@end

@interface MITCADHistoryMarkerButton : NSButton {
@public
  mitcad::mac::NativeMacWindowState* state;
}
- (BOOL)beginHistoryMarkerDrag;
- (void)dragHistoryMarkerToIndex:(NSInteger)index;
- (void)endHistoryMarkerDrag;
@end

// A timeline group (mitcad#1077): the band over an open group's cells, or
// the one folder cell of a folded group, as the shared timeline shows it.
@interface MITCADHistoryGroupView : NSView {
@public
  mitcad::mac::NativeMacWindowState* state;
  BOOL pressed;
  NSTrackingArea* hoverTracking;
}
@property(nonatomic, copy) NSString* groupName;
@property(nonatomic, copy) NSString* firstKey; // the feature whose cell it folds into
@property(nonatomic) BOOL collapsed;
@property(nonatomic, strong) NSImageView* folder;
@end

@interface MITCADInspectorDocumentView : NSView
@end

@implementation MITCADInspectorDocumentView
- (BOOL)isFlipped { return YES; }
@end

// A bounded card's scrolled document. A change of its content's height (the
// selection card's text and Create row filled in after the card is shown)
// lays the cards out again, so the card follows its content (mitcad#815).
@interface MITCADInspectorContentView : MITCADInspectorDocumentView {
@public
  mitcad::mac::NativeMacWindowState* state;
}
@end

@interface MITCADTreeNode : NSObject
@property(nonatomic, copy) NSString* key;
@property(nonatomic, strong) NSMutableArray<MITCADTreeNode*>* children;
@end

@implementation MITCADTreeNode
@end

@interface MITCADNativeWindowDelegate : NSObject <NSApplicationDelegate, NSWindowDelegate, NSToolbarDelegate,
    NSOutlineViewDataSource, NSOutlineViewDelegate, NSTableViewDataSource, NSTableViewDelegate,
    NSControlTextEditingDelegate, NSTextFieldDelegate, NSSearchFieldDelegate,
    NSMenuItemValidation, NSToolbarItemValidation, NSPopoverDelegate> {
@public
  mitcad::mac::NativeMacWindowState* state;
}
- (void)performAction:(id)sender;
- (void)toggleSidebar:(id)sender;
- (void)toggleInspector:(id)sender;
- (void)toggleHistory:(id)sender;
- (void)changeWorkspace:(id)sender;
- (void)collapseProjects:(id)sender;
- (void)collapseBrowser:(id)sender;
- (void)turnCube:(id)sender;
- (void)showBrowserIssues:(id)sender;
- (void)openSearch:(id)sender;
- (void)chooseSearch:(id)sender;
- (void)searchChanged:(id)sender;
- (void)openTimelineSearchFilter:(id)sender;
- (void)openTimelineSearch:(id)sender;
- (void)stepTimelineSearch:(id)sender;
- (void)activateTree:(id)sender;
- (void)clickHistoryFeature:(id)sender;
- (void)stepHistory:(id)sender;
- (void)openHistoryFilter:(id)sender;
- (void)openProjectMenu:(id)sender;
- (void)clickAgentIndicator:(id)sender;
- (void)openAgentMenu:(id)sender;
- (void)toggleTreeVisibility:(id)sender;
- (void)activateComponent:(id)sender;
- (void)renameTree:(id)sender;
- (void)closeDocument:(id)sender;
- (void)quitApplication:(id)sender;
- (void)menuTrackingChanged:(NSNotification*)notification;
// A group menu's Pin to Toolbar and Change Keyboard Shortcut entries, a
// pinned button's menu (mitcad#1074): representedObject is the tab, the
// command and the menu's place.
- (void)pinCommand:(id)sender;
- (void)changeShortcut:(id)sender;
- (void)collapseConfiguration:(id)sender;
// Their callback, once the menu has closed: the tab, the command and
// whether to pin, or the command alone for its shortcut.
- (void)applyRowCommand:(NSArray*)command;
// The quick-access group's ⋮ and its menu's checks and Reset to Default
// (mitcad#1113); applied once the menu has closed: the command and whether
// it is checked, nothing for the reset, or @"hide" for Hide Quick-Access
// Toolbar (mitcad#1189).
- (void)openQuickAccessMenu:(id)sender;
- (void)chooseQuickAccess:(id)sender;
- (void)resetQuickAccess:(id)sender;
- (void)hideQuickAccess:(id)sender;
- (void)applyQuickAccess:(NSArray*)choice;
// The navigation bar's Visual Style button (mitcad#1099): its pop-up.
- (void)openNavigationStyles:(id)sender;
@end

// The agent indicator's button (mitcad#710): a click connects an agent or
// opens the agents' menu, a right click always opens the menu.
@interface MITCADAgentButton : NSButton
@end
@implementation MITCADAgentButton
- (void)rightMouseDown:(NSEvent*)event {
  (void)event;
  [NSApp sendAction:@selector(openAgentMenu:) to:self.target from:self];
}
@end

namespace mitcad::mac {
namespace {

NSString* ns(const QString& text) { return text.toNSString(); }
QString qt(NSString* text) { return QString::fromNSString(text != nil ? text : @""); }

// A workspace's tab as the Qt ribbon names it: "CUT SHEET" -> "Cut Sheet".
QString workspaceCaption(const QString& workspace) {
  QStringList words = workspace.toLower().split(QLatin1Char(' '));
  for (QString& word : words) {
    if (!word.isEmpty()) word[0] = word[0].toUpper();
  }
  return words.join(QLatin1Char(' '));
}

constexpr CGFloat ribbonButtonHeight = 36;
constexpr CGFloat ribbonGroupGap = 8;
constexpr CGFloat ribbonInset = 12;
// The entries a group menu adds after the shared menu's (Pin to Toolbar,
// Change Keyboard Shortcut), so that tests compare only the shared ones.
constexpr NSInteger rowCommandsTag = 1001;
constexpr CGFloat historyInset = timeline::padding;
constexpr CGFloat historySlot = timeline::cell + timeline::gap;
constexpr CGFloat historyRunSlot = 11;
constexpr CGFloat historyStripHeight = timeline::height;

// A light bulb that follows a Show When condition (mitcad#1079): the bulb
// with the fx mark in its lower right corner, as the Qt row's badge.
NSImage* conditionBulb(NSImage* bulb) {
  NSImage* image = [NSImage imageWithSize:bulb.size flipped:NO drawingHandler:^BOOL(NSRect rect) {
    [bulb drawInRect:rect];
    NSAttributedString* mark = [[NSAttributedString alloc] initWithString:@"fx" attributes:@{
      NSFontAttributeName: [NSFont systemFontOfSize:7 weight:NSFontWeightBold],
      NSForegroundColorAttributeName: NSColor.controlAccentColor
    }];
    const NSSize size = mark.size;
    const NSRect badge = NSMakeRect(NSMaxX(rect) - size.width - 2, NSMinY(rect), size.width + 2, size.height);
    [NSColor.controlBackgroundColor setFill];
    [[NSBezierPath bezierPathWithRoundedRect:badge xRadius:2 yRadius:2] fill];
    [mark drawAtPoint:NSMakePoint(NSMinX(badge) + 1, NSMinY(badge))];
    return YES;
  }];
  image.accessibilityDescription = nil;
  return image;
}

NSImage* symbol(const QString& name, NSString* description = nil) {
  NSImage* image = [NSImage imageWithSystemSymbolName:ns(name) accessibilityDescription:description];
  if (image == nil) {
    image = [NSImage imageWithSystemSymbolName:@"square.dashed" accessibilityDescription:description];
  }
  [image setTemplate:YES];
  return image;
}

NSImage* renderActionImage(const NativeMacAction& action) {
  if (!action.symbol.isEmpty()) {
    NSImage* image = [NSImage imageWithSystemSymbolName:ns(action.symbol) accessibilityDescription:ns(action.label)];
    if (image) { [image setTemplate:YES]; return image; }
  }
  if (!action.icon.isNull()) {
    QImage image = action.icon.pixmap(48, 48).toImage().convertToFormat(QImage::Format_RGBA8888);
    // QImage's RGBA8888 stores straight alpha; AppKit must use the same
    // format to preserve the CAD colours along antialiased edges.
    NSBitmapImageRep* bitmap = [[NSBitmapImageRep alloc] initWithBitmapDataPlanes:nil
       pixelsWide:image.width() pixelsHigh:image.height() bitsPerSample:8 samplesPerPixel:4
       hasAlpha:YES isPlanar:NO colorSpaceName:NSDeviceRGBColorSpace bitmapFormat:NSBitmapFormatAlphaNonpremultiplied
       bytesPerRow:image.bytesPerLine() bitsPerPixel:32];
    if (bitmap) {
      memcpy(bitmap.bitmapData, image.constBits(), size_t(image.sizeInBytes()));
      NSImage* result = [[NSImage alloc] initWithSize:NSMakeSize(24, 24)];
      [result addRepresentation:bitmap];
      [result setTemplate:action.iconTemplate];
      result.accessibilityDescription = ns(action.label);
      return result;
    }
  }
  return symbol(QStringLiteral("square.dashed"), ns(action.label));
}

QString actionMenuPath(const NativeMacAction& item) { return item.menu; }

bool isAppAction(const NativeMacAction& item) {
  return item.id.startsWith(QStringLiteral("app.")) ||
         item.menu.section(QLatin1Char('/'), 0, 0) == QStringLiteral("MitCad");
}

SEL editingSelector(const NativeMacAction* action) {
  if (!action) return nullptr;
  const QKeySequence shortcut(action->shortcut, QKeySequence::PortableText);
  for (const auto& pair : {std::pair<QKeySequence::StandardKey, SEL>(QKeySequence::Cut, @selector(cut:)),
                           std::pair<QKeySequence::StandardKey, SEL>(QKeySequence::Copy, @selector(copy:)),
                           std::pair<QKeySequence::StandardKey, SEL>(QKeySequence::Paste, @selector(paste:)),
                           std::pair<QKeySequence::StandardKey, SEL>(QKeySequence::SelectAll, @selector(selectAll:))}) {
    if (!shortcut.isEmpty() && shortcut == QKeySequence(pair.first)) return pair.second;
  }
  return nullptr;
}

QPoint screenPoint(NSEvent* event) {
  const NSPoint point = [event.window convertPointToScreen:event.locationInWindow];
  const CGFloat top = NSScreen.screens.firstObject.frame.size.height;
  return QPoint(int(std::lround(point.x)), int(std::lround(top - point.y)));
}

QPoint menuPoint(NSView* control) {
  const NSPoint local = [control convertPoint:NSMakePoint(NSMinX(control.bounds), NSMaxY(control.bounds)) toView:nil];
  const NSPoint point = [control.window convertPointToScreen:local];
  const CGFloat top = NSScreen.screens.firstObject.frame.size.height;
  return QPoint(int(std::lround(point.x)), int(std::lround(top - point.y)));
}

NSTextField* label(NSString* text, NSFont* font = nil) {
  NSTextField* field = [NSTextField labelWithString:text];
  field.font = font != nil ? font : [NSFont systemFontOfSize:NSFont.systemFontSize];
  field.translatesAutoresizingMaskIntoConstraints = NO;
  return field;
}

NSScrollView* scroll(NSView* document) {
  NSScrollView* view = [[NSScrollView alloc] initWithFrame:NSZeroRect];
  view.documentView = document;
  view.hasVerticalScroller = YES;
  view.autohidesScrollers = YES;
  view.drawsBackground = NO;
  view.translatesAutoresizingMaskIntoConstraints = NO;
  return view;
}

void fill(NSView* child, NSView* parent, CGFloat inset = 0) {
  child.translatesAutoresizingMaskIntoConstraints = NO;
  [parent addSubview:child];
  [NSLayoutConstraint activateConstraints:@[
    [child.leadingAnchor constraintEqualToAnchor:parent.leadingAnchor constant:inset],
    [child.trailingAnchor constraintEqualToAnchor:parent.trailingAnchor constant:-inset],
    [child.topAnchor constraintEqualToAnchor:parent.topAnchor constant:inset],
    [child.bottomAnchor constraintEqualToAnchor:parent.bottomAnchor constant:-inset]
  ]];
}

// A card of the given corner radius; half its side makes it round.
MITCADFloatingCard* floatingCard(NSString* identifier, CGFloat radius = 14) {
  MITCADFloatingCard* card = [[MITCADFloatingCard alloc] initWithFrame:NSZeroRect];
  card.accessibilityIdentifier = identifier;
  card.cornerRadius = radius;
  card.wantsLayer = YES;
  card.layer.shadowColor = NSColor.blackColor.CGColor;
  card.layer.shadowOpacity = 0.15;
  card.layer.shadowRadius = 12;
  card.layer.shadowOffset = CGSizeMake(0, -3);
  NSView* body = [[MITCADInspectorDocumentView alloc] initWithFrame:NSZeroRect];
  card.body = body;
  if (@available(macOS 26.0, *)) {
    NSGlassEffectView* glass = [[NSGlassEffectView alloc] initWithFrame:NSZeroRect];
    glass.style = NSGlassEffectViewStyleRegular;
    glass.cornerRadius = radius;
    glass.contentView = body;
    fill(glass, card);
    return card;
  }
  NSVisualEffectView* material = [[NSVisualEffectView alloc] initWithFrame:NSZeroRect];
  material.material = NSVisualEffectMaterialPopover;
  material.blendingMode = NSVisualEffectBlendingModeWithinWindow;
  material.state = NSVisualEffectStateFollowsWindowActiveState;
  material.wantsLayer = YES;
  material.layer.cornerRadius = radius;
  material.layer.masksToBounds = YES;
  fill(material, card);
  fill(body, material);
  return card;
}

NSOutlineView* outlineIn(NSView* view) {
  if ([view isKindOfClass:NSOutlineView.class]) return (NSOutlineView*)view;
  for (NSView* child in view.subviews) {
    if (NSOutlineView* outline = outlineIn(child)) return outline;
  }
  return nil;
}

void setShortcut(NSMenuItem* item, const QString& shortcut) {
  setKeyEquivalent(item, QKeySequence::fromString(shortcut, QKeySequence::PortableText));
}

int searchScore(const NativeMacAction& action, const QString& query) {
  const QString name = action.label.toCaseFolded();
  if (query.isEmpty()) return 5;
  if (name.startsWith(query)) return 0;
  for (const QString& word : name.split(QLatin1Char(' '), Qt::SkipEmptyParts)) {
    if (word.startsWith(query)) return 1;
  }
  if (name.contains(query)) return 2;
  for (const QString& keyword : action.keywords) {
    if (keyword.toCaseFolded().startsWith(query)) return 3;
  }
  int offset = 0;
  for (const QChar character : query) {
    offset = int(name.indexOf(character, offset));
    if (offset < 0) return -1;
    ++offset;
  }
  return 4;
}

NSString* const workspaceID = @"mitcad.workspaces";
NSString* const documentTitleID = @"mitcad.documentTitle";
NSString* const sidebarID = @"mitcad.sidebar";
NSString* const inspectorID = @"mitcad.inspector";
NSString* const historyID = @"mitcad.history";
NSString* const projectID = @"mitcad.project";
NSString* const agentID = @"mitcad.agent";
NSString* const createID = @"mitcad.create";
NSString* const modifyID = @"mitcad.modify";
NSString* const searchID = @"mitcad.search";
NSString* const constraintsID = @"mitcad.constraints";

} // namespace

namespace {
std::vector<NativeMacWindowState*> documentStates;
id<NSApplicationDelegate> __strong originalAppDelegate = nil;
NSMenu* __strong originalMainMenu = nil;
NSMenu* __strong originalServicesMenu = nil;
NSMenu* __strong originalWindowsMenu = nil;
}

struct NativeMacWindowState {
  QPointer<QWidget> viewport;
  std::unique_ptr<QWindow> foreignWindow;
  NativeMacWindowCallbacks callbacks;
  QString error;
  QList<NativeMacAction> actions;
  QList<QList<QKeySequence>> actionShortcuts;
  QHash<QString, int> actionIndex;
  quint64 shortcutDispatchCount = 0;
  NSCache<NSString*, NSImage*>* __strong actionImages = [[NSCache alloc] init];
  qint64 imagePaletteKey = 0;
  QList<NativeMacTreeItem> treeItems;
  QHash<QString, int> treeIndex;
  QList<NativeMacHistoryItem> historyItems;
  QList<int> searchResults;
  QList<NativeMacHistoryItem> timelineResults;
  QSet<QString> timelineMatches;
  bool timelineSearch = false;
  NSPoint savedHistoryScroll = NSZeroPoint;
  NSPoint savedBrowserScroll = NSZeroPoint;
  int historyMarker = 0;
  QStringList selectedHistory;
  QString historySelectionAnchor;
  bool historyMarkerDragging = false;
  int historyDragPosition = 0;
  int historyDropBoundary = -1;
  QList<CGFloat> historyBoundaries;
  QString projectText;
  QString projectTooltip;
  // The agent indicator (mitcad#710).
  QString agentText;
  QString agentTooltip;
  bool agentConnected = false;
  bool agentAvailable = true;
  bool sketching = false;
  bool browserCollapsed = false;
  bool projectsCollapsed = false;
  QList<NativeMacRibbonGroup> ribbonPresentation;
  QStringList workspaces;
  int workspaceIndex = 0;
  NSSegmentedControl* __strong workspaceTabs = nil;
  // The quick-access group after the tabs (mitcad#1113): its presentation,
  // its row of buttons, what was logged of each and whether it is folded.
  NativeMacQuickAccess quickAccess;
  NSStackView* __strong quickGroup = nil;
  QHash<QString, QString> quickLogged;
  bool quickFolded = false;
  bool quickFoldScheduled = false;
  NSTextField* __strong documentTitle = nil;
  MITCADFloatingCard* __strong projectsCard = nil;
  NSView* __strong projectsHost = nil;
  NSLayoutConstraint* __strong projectsHostBottom = nil;
  NSView* __strong projectsContent = nil;
  NSScrollView* __strong browserScroll = nil;
  NSStackView* __strong browserIssues = nil;
  NSScrollView* __strong configurationScroll = nil;
  NSView* __strong configurationContent = nil;
  MITCADInspectorContentView* __strong configurationDocument = nil;
  NSArray<NSButton*>* __strong browserIssueButtons = nil;
  NSLayoutConstraint* __strong browserTreeTop = nil;
  bool updatingSelection = false;
  bool closing = false;
  bool frameInitialized = false;
  bool layingOut = false;
  QSize viewportSize;
  CGFloat viewportScale = 0;
  bool historyAppearanceValid = false;
  bool historyAppearanceCanChange = false;
  bool historyAppearanceBlocked = false;
  bool historyAppearanceDragging = false;
  int historyAppearanceMarker = -1;
  QStringList historyAppearanceSelection;
  QSet<QString> historyAppearanceMatches;
  qint64 historyAppearancePaletteKey = 0;
  QPointer<QWidget> gestureReceiver;
  unsigned gestureKinds = 0;
  // The side cards' widths, dragged by their inner edges (mitcad#818).
  NativeCardWidths cardWidths;
  NSArray<NSView*>* __strong cardWidthHandles = nil;

  NSEvent* __strong lastShortcutEvent = nil;
  bool lastShortcutHandled = false;
  NSWindow* __strong window = nil;
  MITCADNativeWindowDelegate* __strong delegate = nil;
  MITCADViewportHost* __strong viewportHost = nil;
  MITCADCanvasRoot* __strong canvasRoot = nil;
  MITCADFloatingCard* __strong sidebarCard = nil;
  MITCADFloatingCard* __strong inspectorCard = nil;
  MITCADFloatingCard* __strong historyCard = nil;
  // The message band above the timeline (mitcad#1098): its card and its
  // content, which draws the shared MessageLog.
  MITCADFloatingCard* __strong messageCard = nil;
  MITCADMessageBand* __strong messageBand = nil;
  QString loggedMessageBand; // its place, as last logged
  // The orientation cube's arrows (setCubeArrows), by their ids.
  QList<NativeMacCubeArrow> cubeArrowPresentation;
  NSMutableDictionary<NSString*, MITCADFloatingCard*>* __strong cubeArrows = nil;
  NSVisualEffectView* __strong ribbon = nil;
  NSScrollView* __strong ribbonScroll = nil;
  MITCADInspectorDocumentView* __strong ribbonGroups = nil;
  NSMutableDictionary<NSString*, MITCADCommandButton*>* __strong ribbonButtons = nil;
  NSMutableDictionary<NSString*, NSPopUpButton*>* __strong ribbonMenus = nil;
  NSMutableDictionary<NSString*, NSBox*>* __strong ribbonSeparators = nil;
  NSMutableDictionary<NSString*, NSBox*>* __strong ribbonBackgrounds = nil;
  NSMutableDictionary<NSString*, NSPopUpButton*>* __strong ribbonSplits = nil;
  QStringList ribbonGroupOrder;
  QHash<QString, QStringList> ribbonGroupButtons;
  MITCADModelOutlineView* __strong tree = nil;
  NSMutableArray<MITCADTreeNode*>* __strong treeRoots = nil;
  NSMutableDictionary<NSString*, MITCADTreeNode*>* __strong treeNodes = nil;
  MITCADHistoryStrip* __strong history = nil;
  NSScrollView* __strong historyScroll = nil;
  NSMutableDictionary<NSString*, MITCADHistoryFeatureButton*>* __strong historyButtons = nil;
  MITCADHistoryMarkerButton* __strong historyMarkerButton = nil;
  NSArray<NSButton*>* __strong historySteps = nil;
  NSButton* __strong historyFilterButton = nil;
  NSButton* __strong historySearchButton = nil; // timeline search opens above it (mitcad#1021)
  NSMutableArray<NSImageView*>* __strong historyRunMarks = nil;
  NSBox* __strong historyDropLine = nil;
  NSView* __strong inspectorHost = nil;
  NSScrollView* __strong inspectorScroll = nil;
  NSView* __strong inspectorContent = nil;
  MITCADInspectorDocumentView* __strong inspectorDocument = nil;
  NSView* __strong inspectorFooter = nil;        // under the scrolling inspector (mitcad#815)
  NSView* __strong inspectorFooterContent = nil; // a command form's OK and Cancel
  NSPopover* __strong searchPopover = nil;
  NSSearchField* __strong searchField = nil;
  NSTableView* __strong searchTable = nil;
  NSStackView* __strong timelineSearchControls = nil;
  NSTextField* __strong timelineSearchCount = nil;
  NSArray<NSLayoutConstraint*>* __strong commandSearchLayout = nil;
  NSArray<NSLayoutConstraint*>* __strong timelineSearchLayout = nil;
  NSMenu* __strong mainMenu = nil;
  NSMenu* __strong servicesMenu = nil;
  NSMenu* __strong windowsMenu = nil;
  NSMenu* __strong previousMenu = nil;
  NSMenu* __strong previousServicesMenu = nil;
  NSMenu* __strong previousWindowsMenu = nil;
  id<NSApplicationDelegate> __strong previousAppDelegate = nil;
  NSHashTable<MITCADBindingBadge*>* __strong bindingBadges = [NSHashTable weakObjectsHashTable];
  // The Chat card (mitcad#1075): the shared panel's native view in its host
  // (NativeChatCard.hpp), on the right edge with the inspector left of it.
  MITCADFloatingCard* __strong chatCard = nil;
  NSView* __strong chatHost = nil;
  NSView* __strong chatContent = nil;
  // The timeline's hover (mitcad#1076): the feature reported as hovered.
  // Its groups (mitcad#1077), from the items, with their views.
  QString historyHover;
  struct HistoryGroup {
    QString name;
    int first = 0;
    int last = 0;
    bool collapsed = false;
  };
  QList<HistoryGroup> historyGroups;
  NSMutableDictionary<NSString*, MITCADHistoryGroupView*>* __strong historyGroupViews = [NSMutableDictionary dictionary];
  QString loggedHistoryGroups;
  // While a drawing is open (setDrawingOpen, mitcad#1073): the cards it
  // hides and the shown state each was asked for meanwhile.
  bool drawingOpen = false;
  NSMapTable<NSView*, NSNumber*>* __strong suppressedCards = [NSMapTable strongToStrongObjectsMapTable];
  // The Browser row under the pointer, as last reported (mitcad#1072).
  QString hoveredTreeRow;
  // The Configuration card between Projects and Browser and the left
  // column's split (mitcad#1020).
  MITCADFloatingCard* __strong configurationCard = nil;
  NSButton* __strong configurationDisclosure = nil;
  NSLayoutConstraint* __strong configurationScrollBottom = nil;
  bool configurationCollapsed = false;
  NativeColumnSplit columnSplit;      // the user's heights; empty: automatic
  NativeColumnSplit splitDragStart;   // the open cards' heights at the press
  std::vector<std::pair<QString, QString>> splitPairs; // each handle's cards
  std::pair<QString, QString> splitDragPair; // the cards of the handle pressed
  bool splitDragging = false;
  bool splitMoved = false;
  std::vector<NativeColumnCard> columnCards; // as last laid out
  NSMutableArray<NSView*>* __strong columnSplitHandles = nil;
  // layoutOverlays runs in the canvas's own layout pass (MITCADCanvasRoot),
  // where the inspector is not laid out again; a follow-up is queued.
  bool inCanvasLayout = false;
  bool overlayLayoutQueued = false;
  // The navigation bar (mitcad#1099): its card at the bottom centre of the
  // view, its buttons in order, and what it last logged.
  NativeMacNavigationBar navigationBar;
  MITCADFloatingCard* __strong navigationCard = nil;
  NSMutableArray<MITCADNavigationButton*>* __strong navigationButtons = nil;
  QString loggedNavigationPlace;
  QString loggedNavigationTips;
  QString loggedNavigationStates;

  explicit NativeMacWindowState(QWidget* widget, NativeMacWindowCallbacks calls)
      : viewport(widget), callbacks(std::move(calls)) {
    actionImages.countLimit = 1024;
    delegate = [[MITCADNativeWindowDelegate alloc] init];
    delegate->state = this;
    if (documentStates.empty()) {
      originalAppDelegate = NSApp.delegate;
      originalMainMenu = NSApp.mainMenu;
      originalServicesMenu = NSApp.servicesMenu;
      originalWindowsMenu = NSApp.windowsMenu;
    }
    previousAppDelegate = originalAppDelegate;
    documentStates.push_back(this);
    NSApp.delegate = delegate;
    buildWindow();
    embedViewport();
    rebuildMenus();
    // AppKit's menu tracking loop can consume a release without Qt's
    // popup/WindowBlocked lifecycle. Observe all menus in this process,
    // including the ribbon and context menus bridged from QMenu.
    NSNotificationCenter* notifications = NSNotificationCenter.defaultCenter;
    [notifications addObserver:delegate selector:@selector(menuTrackingChanged:)
                         name:NSMenuDidBeginTrackingNotification object:nil];
    [notifications addObserver:delegate selector:@selector(menuTrackingChanged:)
                         name:NSMenuDidEndTrackingNotification object:nil];
  }

  ~NativeMacWindowState() {
    finishHistoryMarkerDrag();
    closing = true;
    messageBand->closed = nullptr;
    messageBand->featureChosen = nullptr;
    for (MITCADBindingBadge* badge in bindingBadges.allObjects) {
      badge->state = nullptr;
      [badge->hoverTimer invalidate];
    }
    [NSNotificationCenter.defaultCenter removeObserver:delegate name:NSMenuDidBeginTrackingNotification object:nil];
    [NSNotificationCenter.defaultCenter removeObserver:delegate name:NSMenuDidEndTrackingNotification object:nil];
    delegate->state = nullptr;
    ((MITCADDocumentWindow*)window)->state = nullptr;
    viewportHost->state = nullptr;
    canvasRoot->state = nullptr;
    if ([inspectorDocument isKindOfClass:MITCADInspectorContentView.class])
      ((MITCADInspectorContentView*)inspectorDocument)->state = nullptr;
    if (configurationDocument) configurationDocument->state = nullptr;
    tree->state = nullptr;
    history->state = nullptr;
    historyMarkerButton->state = nullptr;
    for (MITCADHistoryFeatureButton* button in historyButtons.allValues) {
      button->state = nullptr;
      [button removeAllToolTips];
    }
    for (MITCADHistoryGroupView* view in historyGroupViews.allValues) {
      view->state = nullptr;
      [view removeAllToolTips];
    }
    for (NSView* handle in cardWidthHandles) detachCardWidthHandle(handle);
    for (NSView* handle in columnSplitHandles) detachColumnSplitHandle(handle);
    [searchPopover close];
    window.delegate = nil;
    window.toolbar.delegate = nil;
    tree.delegate = nil;
    tree.dataSource = nil;
    searchTable.delegate = nil;
    searchTable.dataSource = nil;
    // Detach Qt while its foreign parent NSView still exists. Qt owns its
    // child handle, AppKit owns the parent, and neither owns the other.
    if (viewport && viewport->windowHandle()) {
      viewport->hide();
      viewport->windowHandle()->setParent(nullptr);
    }
    foreignWindow.reset();
    documentStates.erase(std::remove(documentStates.begin(), documentStates.end(), this), documentStates.end());
    if (documentStates.empty()) {
      NSApp.delegate = originalAppDelegate;
      NSApp.mainMenu = originalMainMenu;
      NSApp.servicesMenu = originalServicesMenu;
      NSApp.windowsMenu = originalWindowsMenu;
      originalAppDelegate = nil;
      originalMainMenu = nil;
      originalServicesMenu = nil;
      originalWindowsMenu = nil;
    } else if (NSApp.delegate == delegate || NSApp.mainMenu == mainMenu) {
      documentStates.back()->installMenus();
    }
    [window orderOut:nil];
    [window close];
  }

  const NativeMacAction* action(const QString& id) const {
    if (id.isEmpty()) return nullptr;
    const auto found = actionIndex.constFind(id);
    return found == actionIndex.cend() ? nullptr : &actions[*found];
  }

  NSImage* actionImage(const NativeMacAction& item, int size = 24) {
    const qint64 paletteKey = QApplication::palette().cacheKey();
    if (imagePaletteKey != paletteKey) {
      [actionImages removeAllObjects];
      imagePaletteKey = paletteKey;
    }
    // Each cached image has one immutable logical size: ribbon and history
    // controls must not resize artwork shared with a menu or toolbar item.
    NSString* key = ns(QStringLiteral("%1/%2/%3/%4/%5/%6")
                          .arg(item.icon.cacheKey()).arg(paletteKey).arg(item.iconTemplate)
                          .arg(size).arg(item.symbol.size()).arg(item.symbol));
    NSImage* image = [actionImages objectForKey:key];
    if (!image) {
      image = [renderActionImage(item) copy];
      image.size = NSMakeSize(size, size);
      // Labels belong to the controls; one artwork may serve many commands.
      image.accessibilityDescription = nil;
      [actionImages setObject:image forKey:key];
    }
    return image;
  }

  const NativeMacTreeItem* treeItem(NSString* key) const {
    const auto found = treeIndex.constFind(qt(key));
    return found == treeIndex.cend() ? nullptr : &treeItems[*found];
  }

  bool activateBrowserSelection() {
    if (closing || nativeModalBlocked() || tree.editedRow >= 0 || tree.selectedRow < 0 || !callbacks.activate) return false;
    MITCADTreeNode* node = [tree itemAtRow:tree.selectedRow];
    if (!node) return false;
    callbacks.activate(qt(node.key));
    return true;
  }

  void invoke(const QString& id) {
    const NativeMacAction* item = action(id);
    if (item && item->enabled && callbacks.action) callbacks.action(id);
  }

  void cancelViewportPointerInteraction() {
    gestureReceiver.clear();
    gestureKinds = 0;
    if (!closing && callbacks.cancelPointerInteraction) callbacks.cancelPointerInteraction();
  }

  bool nativeModalBlocked() const {
    NSWindow* key = NSApp.keyWindow;
    return NSApp.modalWindow != nil || window.attachedSheet != nil ||
           (key != nil && (key.attachedSheet != nil || key.sheetParent != nil));
  }

  bool textEditing() const {
    if ([window.firstResponder isKindOfClass:NSTextView.class]) return true;
    // Qt editors can live inside the embedded viewport as CAD overlays.
    NSView* responder = [window.firstResponder isKindOfClass:NSView.class] ? (NSView*)window.firstResponder : nil;
    if (responder && [responder isDescendantOf:viewportHost]) {
      QWidget* focus = QApplication::focusWidget();
      // A retained utility panel can leave Qt's focus on its offscreen
      // presenter after AppKit has returned the keyboard to the CAD view.
      // Only an editor belonging to that view owns its document keys.
      if (viewport && focus && (focus == viewport.data() || viewport->isAncestorOf(focus))) {
        return qobject_cast<QLineEdit*>(focus) || qobject_cast<QPlainTextEdit*>(focus) ||
               qobject_cast<QTextEdit*>(focus) || qobject_cast<QAbstractSpinBox*>(focus);
      }
    }
    return false;
  }

  bool dispatchShortcutOnce(NSEvent* event) {
    if (event.type != NSEventTypeKeyDown) return false;
    // AppKit can offer one event as a key equivalent and through sendEvent:.
    // Keep both entry points for plain keys, but inspect each event once.
    if (event == lastShortcutEvent) return lastShortcutHandled;
    const bool handled = dispatchShortcut(event);
    lastShortcutEvent = event;
    lastShortcutHandled = handled;
    return handled;
  }

  bool dispatchShortcut(NSEvent* event) {
    ++shortcutDispatchCount;
    if (event.type != NSEventTypeKeyDown || event.window != window ||
        closing || nativeModalBlocked() || QApplication::activeModalWidget()) return false;
    // A Projects name in its row editor ends on Return or Escape before any
    // button's key equivalent; other text keeps its keys (mitcad#816).
    if (projectsNameEditorKey(event)) return true;
    if (textEditing()) return false;
    const QKeySequence pressed = keySequence(event);
    if (pressed.isEmpty()) return false;
    NSResponder* focus = window.firstResponder;
    const bool browserFocus = focus == tree;
    const bool projectsFocus = projectsContent != nil && [focus isKindOfClass:NSOutlineView.class] &&
                               [(NSView*)focus isDescendantOf:projectsContent];
    const bool historyFocus = focus == history || [focus isKindOfClass:MITCADHistoryFeatureButton.class];
    const bool inHistory = [focus isKindOfClass:NSView.class] && [(NSView*)focus isDescendantOf:history];
    const auto combination = pressed[0];
    // AppKit offers the default button its key equivalent before keyDown:
    // reaches the outline. Browser activation must win at that stage too.
    if (browserFocus && combination.keyboardModifiers() == Qt::NoModifier &&
        (combination.key() == Qt::Key_Return || combination.key() == Qt::Key_Enter) && activateBrowserSelection()) return true;
    // The focused Projects outline's Return, F2 and Delete belong to the
    // shared panel, as the default tree's keys do (mitcad#816).
    if (projectsFocus && combination.keyboardModifiers() == Qt::NoModifier && callbacks.projectsKey &&
        callbacks.projectsKey(int(combination.key()))) return true;
    // Other document shortcuts still require the active key window.
    if (NSApp.keyWindow != window) return false;
    if ((browserFocus || historyFocus || inHistory) && pressed == QKeySequence(QKeySequence::Find)) {
      openSearch(true);
      return true;
    }
    if ((browserFocus || historyFocus) && combination.keyboardModifiers() == Qt::NoModifier) {
      const auto key = combination.key();
      if (key == Qt::Key_Delete || key == Qt::Key_Backspace || key == Qt::Key_F2) {
        // Focus belongs to the collection itself, never its field editor.
        if (!historyCanChange()) return true;
        if (browserFocus && tree.selectedRow >= 0) {
          MITCADTreeNode* node = [tree itemAtRow:tree.selectedRow];
          const auto* item = node ? treeItem(node.key) : nullptr;
          if (item && item->enabled) {
            if (key == Qt::Key_F2) {
              if (item->editable) [tree editColumn:0 row:tree.selectedRow withEvent:nil select:YES];
            } else if (callbacks.treeDelete) {
              QStringList selected;
              for (NSUInteger row = tree.selectedRowIndexes.firstIndex; row != NSNotFound;
                   row = [tree.selectedRowIndexes indexGreaterThanIndex:row]) {
                MITCADTreeNode* selectedNode = [tree itemAtRow:NSInteger(row)];
                const auto* selectedItem = selectedNode ? treeItem(selectedNode.key) : nullptr;
                if (selectedItem) selected.append(selectedItem->id);
              }
              callbacks.treeDelete(selected);
            }
          }
        } else if (historyFocus && !selectedHistory.isEmpty()) {
          const QString uid = selectedHistory.back();
          if (key == Qt::Key_F2) { if (callbacks.historyRename) callbacks.historyRename(uid); }
          else if (callbacks.historyDelete) callbacks.historyDelete(selectedHistory);
        }
        return true;
      }
    }
    const NativeMacAction* match = nullptr;
    for (int index = 0; index < actions.size(); ++index) {
      const auto& candidate = actions[index];
      if (!candidate.enabled || candidate.id == QStringLiteral("file.close")) continue;
      for (const QKeySequence& shortcut : actionShortcuts[index]) {
        if (shortcut != pressed) continue;
        if (match && match->id != candidate.id) return false;
        match = &candidate;
      }
    }
    if (!match) return false;
    invoke(match->id);
    return true;
  }

  bool historyCanChange() const {
    return !closing && !nativeModalBlocked() && callbacks.historyCanChange && callbacks.historyCanChange();
  }

  int historyIndex(const QString& key) const {
    for (int index = 0; index < historyItems.size(); ++index) {
      if (historyItems[index].id == key) return index;
    }
    return -1;
  }

  bool historyShownBoundary(int index) const {
    return index <= 0 || index >= historyItems.size() ||
           !historyItems[index].filtered || !historyItems[index - 1].filtered;
  }

  int historyStep(int direction) const {
    for (int index = historyMarker + direction; index > 0 && index < historyItems.size(); index += direction) {
      if (historyShownBoundary(index)) return index;
    }
    return direction < 0 ? 0 : int(historyItems.size());
  }

  void layoutHistory() {
    if (!history) return;
    historyBoundaries.clear();
    historyBoundaries.reserve(historyItems.size() + 1);
    CGFloat x = historyInset;
    int run = 0;
    CGFloat markerX = x;
    for (int index = 0; index < historyItems.size(); ++index) {
      const bool hidden = historyItems[index].filtered;
      const HistoryGroup* folded = historyFoldedGroup(index);
      MITCADHistoryFeatureButton* button = historyButtons[ns(historyItems[index].id)];
      button.hidden = hidden || folded;
      historyBoundaries.append(x);
      if (index == historyMarker) {
        markerX = x;
        x += timeline::marker + timeline::gap;
      }
      if (folded) {
        // A folded group's cells take one cell's room, its folder cell
        // (mitcad#1077).
        if (index == folded->first) {
          historyGroupViews[ns(folded->name)].frame = NSMakeRect(x, timeline::top, timeline::cell, timeline::cell);
          x += historySlot;
        }
        continue;
      }
      if (!hidden) {
        button.frame = NSMakeRect(x, timeline::top, timeline::cell, timeline::cell);
        x += historySlot;
        continue;
      }
      if (index > 0 && historyItems[index - 1].filtered && index != historyMarker) {
        // Internal boundaries share the run's end; pointer interaction
        // considers only its two outer boundaries.
        continue;
      }
      NSImageView* mark = NSUInteger(run) < historyRunMarks.count ? historyRunMarks[NSUInteger(run)] : nil;
      if (!mark) {
        mark = [[NSImageView alloc] initWithFrame:NSZeroRect];
        mark.image = symbol(QStringLiteral("ellipsis"));
        mark.contentTintColor = NSColor.tertiaryLabelColor;
        [history addSubview:mark positioned:NSWindowBelow relativeTo:historyMarkerButton];
        [historyRunMarks addObject:mark];
      }
      int count = 1;
      while (index + count < historyItems.size() && historyItems[index + count].filtered && index + count != historyMarker) ++count;
      mark.hidden = NO;
      mark.frame = NSMakeRect(x + 1, 10, historyRunSlot - 2, 14);
      mark.toolTip = ns(QObject::tr("%1 feature(s) hidden by the timeline filter").arg(count));
      mark.accessibilityLabel = mark.toolTip;
      mark.accessibilityIdentifier = ns(QStringLiteral("native.history.filteredRun.%1").arg(index));
      ++run;
      x += historyRunSlot;
    }
    historyBoundaries.append(x);
    if (historyMarker == historyItems.size()) { markerX = x; x += timeline::marker; }
    for (NSUInteger index = NSUInteger(run); index < historyRunMarks.count; ++index) historyRunMarks[index].hidden = YES;
    const CGFloat width = std::max(historyScroll.contentView.bounds.size.width, x + historyInset);
    history.frame = NSMakeRect(0, 0, width, historyStripHeight);
    historyMarkerButton.frame = NSMakeRect(markerX, 0, timeline::marker, historyStripHeight);
    historyDropLine.frame = NSMakeRect(historyBoundaries[std::clamp(historyDropBoundary, 0, int(historyItems.size()))] - 1, 3, 2, historyStripHeight - 4);
    historyDropLine.hidden = historyDropBoundary < 0;
    layoutHistoryGroups();
  }

  void updateHistoryAppearance() {
    const bool canChange = historyCanChange();
    const bool blocked = nativeModalBlocked();
    const qint64 paletteKey = QApplication::palette().cacheKey();
    if (historyAppearanceValid && historyAppearanceCanChange == canChange &&
        historyAppearanceBlocked == blocked && historyAppearanceDragging == historyMarkerDragging &&
        historyAppearanceMarker == historyMarker && historyAppearanceSelection == selectedHistory &&
        historyAppearanceMatches == timelineMatches && historyAppearancePaletteKey == paletteKey) return;
    historyAppearanceValid = true;
    historyAppearanceCanChange = canChange;
    historyAppearanceBlocked = blocked;
    historyAppearanceDragging = historyMarkerDragging;
    historyAppearanceMarker = historyMarker;
    historyAppearanceSelection = selectedHistory;
    historyAppearanceMatches = timelineMatches;
    historyAppearancePaletteKey = paletteKey;
    for (int index = 0; index < historyItems.size(); ++index) {
      const auto& item = historyItems[index];
      MITCADHistoryFeatureButton* button = historyButtons[ns(item.id)];
      button.enabled = item.enabled && !historyCellHidden(index) && !blocked;
      button.state = selectedHistory.contains(item.id) ? NSControlStateValueOn : NSControlStateValueOff;
      button.wantsLayer = YES;
      button.layer.cornerRadius = 5;
      const bool failed = item.status == QStringLiteral("error");
      const bool warned = item.status == QStringLiteral("warning");
      const bool selected = selectedHistory.contains(item.id);
      const bool found = timelineMatches.contains(item.id);
      // The status rings and tints follow the appearance (mitcad#1041: no
      // static copy of the palette's colour).
      NSColor* issue = failed ? roleColor(ColorRole::ErrorBorder)
                             : roleColor(ColorRole::WarningBorder);
      button.layer.backgroundColor = (failed || warned ? [issue colorWithAlphaComponent:0.22]
                                                      : selected ? [NSColor.controlAccentColor colorWithAlphaComponent:0.18]
                                                                 : NSColor.clearColor).CGColor;
      // An import note is information (mitcad#1040): its badge, no ring in
      // a warning's colour.
      button.layer.borderWidth = selected || found ? 2 : failed || warned ? 1.2 : 0;
      button.layer.borderColor = (selected ? NSColor.controlAccentColor
                                          : found ? roleColor(ColorRole::SearchMatch) : issue).CGColor;
      button.alphaValue = item.suppressed || index >= historyMarker ? kStateDimOpacity : 1.0;
      QStringList description = {item.title};
      if (item.suppressed) description.append(QStringLiteral("Suppressed"));
      else if (index >= historyMarker) description.append(QStringLiteral("Rolled back"));
      // The level's word (mitcad#1040).
      if (item.status == QStringLiteral("error")) description.append(noticeLevelWord(NoticeLevel::Error));
      else if (item.status == QStringLiteral("warning")) description.append(noticeLevelWord(NoticeLevel::Warning));
      else if (item.noted) description.append(QStringLiteral("Import note"));
      button.accessibilityLabel = ns(description.join(QStringLiteral(", ")));
    }
    updateHistoryGroupAppearance();
    // An accepted drag remains active during recomputes, so its final target
    // and shared undo merge cannot be lost when a snapshot refreshes.
    historyMarkerButton.enabled = historyMarkerDragging || (canChange && !historyItems.isEmpty());
    const int hiddenAfter = int(std::count_if(historyItems.cbegin() + historyMarker, historyItems.cend(),
                                            [](const auto& item) { return item.filtered; }));
    historyMarkerButton.toolTip = ns(QObject::tr("Timeline marker: %1 of %2\nDrag to roll back or forward%3")
                                      .arg(historyMarker).arg(historyItems.size())
                                      .arg(hiddenAfter > 0 ? QObject::tr("\n%1 hidden feature(s) after the marker").arg(hiddenAfter) : QString()));
    historyFilterButton.enabled = !blocked && !historyMarkerDragging;
    for (NSButton* button in historySteps) {
      const bool beginning = button.tag < 2;
      button.enabled = canChange && !historyMarkerDragging &&
                       (beginning ? historyMarker > 0 : historyMarker < historyItems.size());
    }
    layoutHistory();
  }

  void setHistorySelected(const QStringList& keys, bool scroll = true) {
    QStringList valid;
    for (int index = 0; index < historyItems.size(); ++index) {
      if (!historyCellHidden(index) && keys.contains(historyItems[index].id)) valid.append(historyItems[index].id);
    }
    const bool changed = valid != selectedHistory;
    selectedHistory = valid;
    if (!selectedHistory.contains(historySelectionAnchor)) {
      historySelectionAnchor = selectedHistory.isEmpty() ? QString() : selectedHistory.front();
    }
    updateHistoryAppearance();
    if (scroll && changed && !historyMarkerDragging && !selectedHistory.isEmpty()) {
      MITCADHistoryFeatureButton* button = historyButtons[ns(selectedHistory.back())];
      [button scrollRectToVisible:NSInsetRect(button.bounds, -8, -4)];
    }
  }

  void selectHistoryFeature(const QString& key, NSEventModifierFlags modifiers) {
    const int index = historyIndex(key);
    if (index < 0 || nativeModalBlocked() || historyCellHidden(index) || !historyItems[index].enabled) return;
    QStringList keys = selectedHistory;
    const int anchor = historyIndex(historySelectionAnchor);
    if ((modifiers & NSEventModifierFlagShift) && anchor >= 0) {
      if (!(modifiers & NSEventModifierFlagCommand)) keys.clear();
      for (int at = std::min(anchor, index); at <= std::max(anchor, index); ++at) {
        if (!historyCellHidden(at) && historyItems[at].enabled && !keys.contains(historyItems[at].id)) keys.append(historyItems[at].id);
      }
    } else if (modifiers & NSEventModifierFlagCommand) {
      if (keys.contains(key)) keys.removeAll(key);
      else keys.append(key);
      historySelectionAnchor = key;
    } else {
      keys = {key};
      historySelectionAnchor = key;
    }
    setHistorySelected(keys);
    [window makeFirstResponder:historyButtons[ns(key)]];
    if (!updatingSelection && callbacks.historySelection) callbacks.historySelection(selectedHistory);
  }

  void activateHistoryFeature(const QString& key) {
    const int index = historyIndex(key);
    if (index < 0 || historyCellHidden(index) || !historyItems[index].enabled || !historyCanChange()) return;
    selectHistoryFeature(key, 0);
    if (callbacks.historyActivated) callbacks.historyActivated(key);
  }

  void navigateHistory(const QString& key, int direction, NSEventModifierFlags modifiers) {
    if (historyItems.isEmpty()) return;
    const int current = std::max(0, historyIndex(key));
    int target = direction == -2 ? 0 : direction == 2 ? int(historyItems.size()) - 1 : current + direction;
    const int step = direction < 0 && direction != -2 ? -1 : direction == 2 ? -1 : 1;
    while (target >= 0 && target < historyItems.size()) {
      if (!historyCellHidden(target)) { selectHistoryFeature(historyItems[target].id, modifiers); return; }
      target += step;
    }
  }

  int historyBoundary(NSPoint windowPoint, bool autoScroll = false) {
    if (autoScroll) {
      NSClipView* clip = historyScroll.contentView;
      const NSPoint inClip = [clip convertPoint:windowPoint fromView:nil];
      NSPoint origin = clip.bounds.origin;
      if (inClip.x < NSMinX(clip.bounds) + 16) origin.x -= historySlot / 2;
      else if (inClip.x > NSMaxX(clip.bounds) - 16) origin.x += historySlot / 2;
      origin.x = std::clamp(origin.x, CGFloat(0), std::max(CGFloat(0), history.bounds.size.width - clip.bounds.size.width));
      [clip scrollToPoint:origin];
      [historyScroll reflectScrolledClipView:clip];
    }
    const NSPoint local = [history convertPoint:windowPoint fromView:nil];
    int nearest = 0;
    CGFloat distance = std::abs(local.x - historyBoundaries.front());
    for (int index = 1; index < historyBoundaries.size(); ++index) {
      if (!historyShownBoundary(index) || historyInsideFolded(index)) continue;
      const CGFloat candidate = std::abs(local.x - historyBoundaries[index]);
      if (candidate < distance) { nearest = index; distance = candidate; }
    }
    return nearest;
  }

  bool beginHistoryMarkerDrag() {
    if (historyMarkerDragging || !historyCanChange() || historyItems.isEmpty() ||
        !callbacks.historyMarkerDragStarted || !callbacks.historyMarkerDragged || !callbacks.historyMarkerDragFinished) return false;
    historyMarkerDragging = true;
    historyDragPosition = historyMarker;
    hoverHistoryCell(QString());
    callbacks.historyMarkerDragStarted();
    updateHistoryAppearance();
    return true;
  }

  void dragHistoryMarker(int position) {
    if (!historyMarkerDragging) return;
    if (closing || nativeModalBlocked() || historyCard.hidden) { finishHistoryMarkerDrag(); return; }
    position = std::clamp(position, 0, int(historyItems.size()));
    if (position == historyDragPosition) return;
    historyDragPosition = historyMarker = position;
    updateHistoryAppearance();
    callbacks.historyMarkerDragged(position);
  }

  void finishHistoryMarkerDrag() {
    if (!historyMarkerDragging) return;
    const int position = historyDragPosition;
    historyMarkerDragging = false;
    if (callbacks.historyMarkerDragFinished) callbacks.historyMarkerDragFinished(position);
    updateHistoryAppearance();
  }

  void requestHistoryMarker(int position) {
    if (!historyCanChange() || historyMarkerDragging || !callbacks.historyMarkerChanged) return;
    position = std::clamp(position, 0, int(historyItems.size()));
    if (position != historyMarker) callbacks.historyMarkerChanged(position);
  }

  void reorderHistoryFeature(const QString& key, int destination) {
    const int from = historyIndex(key);
    if (!historyCanChange() || !callbacks.historyReordered || from < 0 || historyItems.isEmpty() || historyCellHidden(from)) return;
    destination = std::clamp(destination, 0, int(historyItems.size()) - 1);
    if (destination != from) callbacks.historyReordered(key, destination);
  }

  void rebuildHistory(const QList<NativeMacHistoryItem>& items, int marker) {
    // A rebuild ends the hover, as the shared timeline's does (mitcad#1076).
    hoverHistoryCell(QString());
    if (items.size() != historyItems.size()) historyAppearanceValid = false;
    else for (int index = 0; index < items.size(); ++index) {
      const auto& a = items[index]; const auto& b = historyItems[index];
      if (a.id != b.id || a.title != b.title || a.status != b.status || a.noted != b.noted ||
          a.suppressed != b.suppressed || a.enabled != b.enabled || a.filtered != b.filtered ||
          a.bindings != b.bindings || a.group != b.group || a.groupFirst != b.groupFirst ||
          a.groupCollapsed != b.groupCollapsed) {
        historyAppearanceValid = false;
        break;
      }
    }
    historyItems = items;
    historyMarker = historyMarkerDragging ? std::clamp(historyDragPosition, 0, int(items.size())) :
                    marker < 0 ? int(items.size()) : std::clamp(marker, 0, int(items.size()));
    if (historyMarkerDragging) historyDragPosition = historyMarker;
    NSMutableSet<NSString*>* present = [NSMutableSet set];
    for (const auto& item : items) {
      NSString* key = ns(item.id);
      [present addObject:key];
      MITCADHistoryFeatureButton* button = historyButtons[key];
      if (!button) {
        button = [[MITCADHistoryFeatureButton alloc] initWithFrame:NSZeroRect];
        button->state = this;
        button.featureKey = key;
        button.title = @"";
        button.bordered = NO;
        [button setButtonType:NSButtonTypePushOnPushOff];
        button.imagePosition = NSImageOnly;
        button.imageScaling = NSImageScaleProportionallyDown;
        button.target = delegate;
        button.action = @selector(clickHistoryFeature:);
        button.accessibilityIdentifier = ns(QStringLiteral("native.history.") + item.id);
        [history addSubview:button positioned:NSWindowBelow relativeTo:historyMarkerButton];
        historyButtons[key] = button;
      }
      NativeMacAction artwork;
      artwork.label = item.title;
      artwork.icon = item.icon;
      if (item.icon.isNull()) artwork.symbol = item.symbol;
      NSImage* image = actionImage(artwork, 22);
      if (button.image != image) button.image = image;
      MITCADBindingBadge* bindingBadge = nil;
      for (NSView* child in button.subviews) {
        if ([child isKindOfClass:MITCADBindingBadge.class]) bindingBadge = (MITCADBindingBadge*)child;
      }
      if (!bindingBadge && item.bindings > 0) {
        bindingBadge = [[MITCADBindingBadge alloc] initWithFrame:NSMakeRect(3, 17, 25, 11)];
        bindingBadge->state = this;
        bindingBadge->history = true;
        [bindingBadges addObject:bindingBadge];
        bindingBadge.target = bindingBadge;
        bindingBadge.action = @selector(pinCard:);
        bindingBadge.font = [NSFont systemFontOfSize:8 weight:NSFontWeightSemibold];
        [button addSubview:bindingBadge];
      }
      if (bindingBadge) {
        bindingBadge.hidden = item.bindings <= 0;
        bindingBadge.representedObject = key;
        bindingBadge.title = ns(QStringLiteral("fx %1").arg(item.bindings));
        bindingBadge.accessibilityLabel = ns(QObject::tr("%1 follows %2 bindings").arg(item.title).arg(item.bindings));
        bindingBadge.accessibilityIdentifier = ns(QStringLiteral("native.history.fx.") + item.id);
      }
      // A native system badge communicates the level (mitcad#1040: failure,
      // warning, an import note's information) without replacing or tinting
      // the shared coloured CAD artwork; the levels' one symbol each.
      const QString statusKey = item.noted ? item.status + QStringLiteral(":noted") : item.status;
      if (qt(button.statusKey) != statusKey) {
        button.statusKey = ns(statusKey);
        for (NSView* badge in button.subviews.copy) {
          if ([badge.identifier isEqualToString:@"historyStatusBadge"]) [badge removeFromSuperview];
        }
        const NoticeLevel level = item.status == QStringLiteral("error")     ? NoticeLevel::Error
                                  : item.status == QStringLiteral("warning") ? NoticeLevel::Warning
                                  : item.noted                               ? NoticeLevel::Information
                                                                             : NoticeLevel::Hint;
        if (level != NoticeLevel::Hint) {
          NSImageView* badge = [[NSImageView alloc] initWithFrame:NSMakeRect(17, 0, 11, 11)];
          badge.image = symbol(noticeSymbolName(level));
          badge.contentTintColor = noticeTint(level);
          badge.identifier = @"historyStatusBadge";
          badge.accessibilityElement = NO;
          [button addSubview:badge];
        }
      }
    }
    for (NSString* key in historyButtons.allKeys) {
      if ([present containsObject:key]) continue;
      MITCADHistoryFeatureButton* button = historyButtons[key];
      button->state = nullptr;
      [button removeAllToolTips];
      [button removeFromSuperview];
      [historyButtons removeObjectForKey:key];
    }
    rebuildHistoryGroups();
    setHistorySelected(selectedHistory, false);
  }

  // The timeline's hover (mitcad#1076): the feature of the cell under the
  // pointer (a folded group's first feature), or none. Not while the
  // marker is dragged or a sheet is up.
  void hoverHistoryCell(const QString& key) {
    if (!key.isEmpty() && (closing || historyMarkerDragging || nativeModalBlocked() || historyIndex(key) < 0)) return;
    if (key == historyHover) return;
    historyHover = key;
    if (callbacks.historyHovered) callbacks.historyHovered(key);
  }

  // A cell's tooltip, asked for when AppKit shows it: the shared
  // timeline's (mitcad#1076), else the item's detail.
  QString historyToolTip(const QString& key) const {
    const int index = historyIndex(key);
    if (index < 0) return QString();
    const QString shared = callbacks.historyToolTip ? callbacks.historyToolTip(key) : QString();
    return shared.isEmpty() ? historyItems[index].detail : shared;
  }

  // The timeline groups (mitcad#1077). The folded group a feature's cell
  // is in, or nullptr; a folded group's cells are not shown, its folder
  // cell stands for them.
  const HistoryGroup* historyFoldedGroup(int index) const {
    if (index < 0 || index >= historyItems.size() || historyItems[index].filtered || !historyItems[index].groupCollapsed) {
      return nullptr;
    }
    for (const HistoryGroup& group : historyGroups) {
      if (group.collapsed && index >= group.first && index <= group.last) return &group;
    }
    return nullptr;
  }
  bool historyCellHidden(int index) const { return historyItems[index].filtered || historyFoldedGroup(index); }
  // A boundary between two cells of a folded group: no drop or marker goes there.
  bool historyInsideFolded(int boundary) const {
    return std::any_of(historyGroups.cbegin(), historyGroups.cend(), [boundary](const HistoryGroup& group) {
      return group.collapsed && boundary > group.first && boundary <= group.last;
    });
  }

  void rebuildHistoryGroups() {
    historyGroups.clear();
    for (int index = 0; index < historyItems.size(); ++index) {
      const NativeMacHistoryItem& item = historyItems[index];
      if (item.group.isEmpty()) continue;
      auto group = std::find_if(historyGroups.begin(), historyGroups.end(),
                                [&item](const HistoryGroup& known) { return known.name == item.group; });
      if (group == historyGroups.end()) historyGroups.append(HistoryGroup{item.group, index, index, item.groupCollapsed});
      else group->last = index;
    }
    NSMutableSet<NSString*>* present = [NSMutableSet set];
    for (const HistoryGroup& group : historyGroups) {
      NSString* name = ns(group.name);
      [present addObject:name];
      MITCADHistoryGroupView* view = historyGroupViews[name];
      if (!view) {
        view = [[MITCADHistoryGroupView alloc] initWithFrame:NSZeroRect];
        view->state = this;
        view.groupName = name;
        constexpr CGFloat inset = (timeline::cell - timeline::icon) / 2.0;
        view.folder = [[NSImageView alloc] initWithFrame:NSMakeRect(inset, inset, timeline::icon, timeline::icon)];
        view.folder.imageScaling = NSImageScaleProportionallyDown;
        view.folder.accessibilityElement = NO;
        view.folder.hidden = YES; // shown while the group is folded
        [view addSubview:view.folder];
        view.accessibilityIdentifier = ns(QStringLiteral("native.history.group.") + group.name);
        [history addSubview:view positioned:NSWindowBelow relativeTo:historyMarkerButton];
        historyGroupViews[name] = view;
      }
      NativeMacAction artwork;
      artwork.label = group.name;
      artwork.icon = themeIcon(QStringLiteral("folder"));
      NSImage* image = actionImage(artwork, timeline::icon);
      if (view.folder.image != image) view.folder.image = image;
      const auto& first = historyItems[group.first];
      view.firstKey = ns(first.groupFirst.isEmpty() ? first.id : first.groupFirst);
      view.collapsed = group.collapsed;
    }
    for (NSString* name in historyGroupViews.allKeys) {
      if ([present containsObject:name]) continue;
      MITCADHistoryGroupView* view = historyGroupViews[name];
      view->state = nullptr;
      [view removeAllToolTips];
      [view removeFromSuperview];
      [historyGroupViews removeObjectForKey:name];
    }
  }

  // An open group's band over its cells; a folded group's cell is placed
  // with the cells (layoutHistory). Logged as the shared timeline logs
  // them, in the window's points from its top left, when they change.
  void layoutHistoryGroups() {
    QStringList lines;
    for (const HistoryGroup& group : historyGroups) {
      MITCADHistoryGroupView* view = historyGroupViews[ns(group.name)];
      if (!group.collapsed) {
        MITCADHistoryFeatureButton* first = historyButtons[ns(historyItems[group.first].id)];
        MITCADHistoryFeatureButton* last = historyButtons[ns(historyItems[group.last].id)];
        view.frame = NSMakeRect(NSMinX(first.frame), 0, std::max(CGFloat(1), NSMaxX(last.frame) - NSMinX(first.frame)),
                                timeline::top);
      }
      if (historyCard.hidden || !view.window) continue;
      const NSPoint at = [view convertPoint:NSMakePoint(NSMidX(view.bounds), NSMidY(view.bounds)) toView:nil];
      lines << QStringLiteral("Timeline group %1 at %2,%3")
                   .arg(group.name)
                   .arg(std::lround(at.x))
                   .arg(std::lround(NSHeight(window.contentView.frame) - at.y));
    }
    const QString logged = lines.join(QLatin1Char('\n'));
    if (logged != loggedHistoryGroups) {
      loggedHistoryGroups = logged;
      for (const QString& line : std::as_const(lines)) qDebug().noquote() << line;
    }
  }

  // A folded group's cell (mitcad#1077): the folder artwork in the worst
  // state of its features, dimmed when none of them is built, as the
  // shared timeline paints it.
  void updateHistoryGroupAppearance() {
    for (const HistoryGroup& group : historyGroups) {
      MITCADHistoryGroupView* view = historyGroupViews[ns(group.name)];
      bool failed = false;
      bool warned = false;
      bool off = true;
      for (int index = group.first; index <= group.last; ++index) {
        const NativeMacHistoryItem& item = historyItems[index];
        failed = failed || item.status == QStringLiteral("error");
        warned = warned || item.status == QStringLiteral("warning");
        off = off && (item.suppressed || index >= historyMarker);
      }
      const bool marked = group.collapsed && (failed || warned);
      NSColor* issue = failed ? roleColor(ColorRole::ErrorBorder) : roleColor(ColorRole::WarningBorder);
      view.wantsLayer = YES;
      view.layer.cornerRadius = 5;
      view.layer.backgroundColor = (marked ? [issue colorWithAlphaComponent:0.22] : NSColor.clearColor).CGColor;
      view.layer.borderWidth = marked ? 1.2 : 0;
      view.layer.borderColor = issue.CGColor;
      view.folder.alphaValue = off ? kStateDimOpacity : 1.0;
      const int size = group.last - group.first + 1;
      view.accessibilityLabel = ns(group.collapsed ? QObject::tr("%1, folded group of %2 features").arg(group.name).arg(size)
                                                   : QObject::tr("%1, group of %2 features").arg(group.name).arg(size));
    }
  }

  void buildWindow() {
    window = [[MITCADDocumentWindow alloc] initWithContentRect:NSMakeRect(0, 0, 1280, 800)
                                       styleMask:NSWindowStyleMaskTitled | NSWindowStyleMaskClosable |
                                                 NSWindowStyleMaskMiniaturizable | NSWindowStyleMaskResizable
                                         backing:NSBackingStoreBuffered defer:NO];
    ((MITCADDocumentWindow*)window)->state = this;
    window.releasedWhenClosed = NO;
    window.delegate = delegate;
    window.title = ns(QObject::tr("Untitled"));
    window.identifier = @"native.documentWindow";
    // The document and frame have their own recovery. An older system UI
    // snapshot must not overwrite the current frame autosave at launch.
    window.restorable = NO;
    window.minSize = NSMakeSize(820, 560);
    NSWindow.allowsAutomaticWindowTabbing = YES;
    window.tabbingMode = NSWindowTabbingModeAutomatic;
    window.tabbingIdentifier = @"org.mitcad.document";
    window.collectionBehavior = NSWindowCollectionBehaviorFullScreenPrimary;
    window.toolbarStyle = NSWindowToolbarStyleUnified;
    window.titleVisibility = NSWindowTitleHidden;
    window.titlebarAppearsTransparent = YES;

    const NSSize contentSize = [window contentRectForFrameRect:window.frame].size;
    canvasRoot = [[MITCADCanvasRoot alloc] initWithFrame:NSMakeRect(0, 0, contentSize.width, contentSize.height)];
    canvasRoot->state = this;
    window.contentView = canvasRoot;
    viewportHost = [[MITCADViewportHost alloc] initWithFrame:canvasRoot.bounds];
    viewportHost->state = this;
    // A labelled element of its own: the canvas's focus reads as the 3D
    // view, not the bare window (mitcad#1026).
    viewportHost.accessibilityElement = YES;
    viewportHost.accessibilityRole = NSAccessibilityGroupRole;
    viewportHost.accessibilityLabel = ns(QObject::tr("3D view"));
    viewportHost.accessibilityIdentifier = @"native.viewportHost";
    [canvasRoot addSubview:viewportHost];

    ribbon = [[NSVisualEffectView alloc] initWithFrame:NSZeroRect];
    ribbon.material = NSVisualEffectMaterialHeaderView;
    ribbon.blendingMode = NSVisualEffectBlendingModeWithinWindow;
    ribbon.state = NSVisualEffectStateFollowsWindowActiveState;
    ribbon.accessibilityIdentifier = @"native.ribbon";
    ribbonScroll = scroll(nil);
    ribbonScroll.hasVerticalScroller = NO;
    ribbonScroll.hasHorizontalScroller = YES;
    ribbonScroll.scrollerStyle = NSScrollerStyleOverlay;
    fill(ribbonScroll, ribbon);
    [canvasRoot addSubview:ribbon];

    projectsCard = floatingCard(@"native.projectsCard");
    projectsCard.hidden = YES;
    [canvasRoot addSubview:projectsCard];
    NSTextField* projectsTitle = label(ns(QObject::tr("Projects")), [NSFont systemFontOfSize:13 weight:NSFontWeightSemibold]);
    [projectsCard.body addSubview:projectsTitle];
    NSButton* projectsClose = [NSButton buttonWithImage:symbol(QStringLiteral("chevron.down"))
                                                target:delegate action:@selector(collapseProjects:)];
    projectsClose.bordered = NO;
    projectsClose.translatesAutoresizingMaskIntoConstraints = NO;
    projectsClose.accessibilityIdentifier = @"native.projects.disclosure";
    projectsClose.accessibilityLabel = ns(QObject::tr("Collapse Projects"));
    [projectsCard.body addSubview:projectsClose];
    projectsHost = [[NSView alloc] initWithFrame:NSZeroRect];
    projectsHost.translatesAutoresizingMaskIntoConstraints = NO;
    projectsHost.wantsLayer = YES;
    projectsHost.layer.masksToBounds = YES;
    [projectsCard.body addSubview:projectsHost];
    projectsHostBottom = [projectsHost.bottomAnchor constraintEqualToAnchor:projectsCard.body.bottomAnchor constant:-8];
    // Hidden/collapsed content keeps its intrinsic size outside the header;
    // it must not force a zero-height or collapsed card to grow underneath.
    [NSLayoutConstraint activateConstraints:@[
      [projectsClose.leadingAnchor constraintEqualToAnchor:projectsCard.body.leadingAnchor constant:8],
      [projectsClose.topAnchor constraintEqualToAnchor:projectsCard.body.topAnchor constant:5],
      [projectsClose.widthAnchor constraintEqualToConstant:18],
      [projectsClose.heightAnchor constraintEqualToConstant:20],
      [projectsTitle.leadingAnchor constraintEqualToAnchor:projectsClose.trailingAnchor constant:6],
      [projectsTitle.centerYAnchor constraintEqualToAnchor:projectsClose.centerYAnchor],
      [projectsHost.leadingAnchor constraintEqualToAnchor:projectsCard.body.leadingAnchor constant:6],
      [projectsHost.trailingAnchor constraintEqualToAnchor:projectsCard.body.trailingAnchor constant:-6],
      [projectsHost.topAnchor constraintEqualToAnchor:projectsCard.body.topAnchor constant:30]
    ]];
    buildConfigurationCard();
    sidebarCard = floatingCard(@"native.sidebar");
    [canvasRoot addSubview:sidebarCard];
    NSTextField* browserTitle = label(ns(QObject::tr("Browser")), [NSFont systemFontOfSize:13 weight:NSFontWeightSemibold]);
    [sidebarCard.body addSubview:browserTitle];
    NSButton* browserClose = [NSButton buttonWithImage:symbol(QStringLiteral("chevron.down"), ns(QObject::tr("Collapse Browser")))
                                             target:delegate action:@selector(collapseBrowser:)];
    browserClose.bordered = NO;
    browserClose.translatesAutoresizingMaskIntoConstraints = NO;
    browserClose.accessibilityLabel = ns(QObject::tr("Collapse Browser"));
    [sidebarCard.body addSubview:browserClose];
    [NSLayoutConstraint activateConstraints:@[
      [browserTitle.leadingAnchor constraintEqualToAnchor:browserClose.trailingAnchor constant:6],
      [browserTitle.topAnchor constraintEqualToAnchor:sidebarCard.body.topAnchor constant:8],
      [browserClose.leadingAnchor constraintEqualToAnchor:sidebarCard.body.leadingAnchor constant:8],
      [browserClose.centerYAnchor constraintEqualToAnchor:browserTitle.centerYAnchor],
      [browserClose.widthAnchor constraintEqualToConstant:20],
      [browserClose.heightAnchor constraintEqualToConstant:20]
    ]];

    MITCADCommandButton* find = [[MITCADCommandButton alloc] initWithFrame:NSZeroRect];
    find.image = symbol(QStringLiteral("magnifyingglass"));
    find.bordered = NO;
    find.translatesAutoresizingMaskIntoConstraints = NO;
    find.representedObject = @"view.find_in_design";
    find.target = delegate;
    find.action = @selector(performAction:);
    find.accessibilityIdentifier = @"native.browser.find";
    find.toolTip = ns(QObject::tr("Find in Design: components and bodies by name"));
    [sidebarCard.body addSubview:find];
    [NSLayoutConstraint activateConstraints:@[
      [find.trailingAnchor constraintEqualToAnchor:sidebarCard.body.trailingAnchor constant:-8],
      [find.centerYAnchor constraintEqualToAnchor:browserTitle.centerYAnchor],
      [find.widthAnchor constraintEqualToConstant:20],
      [find.heightAnchor constraintEqualToConstant:20]
    ]];
    browserIssues = [NSStackView stackViewWithViews:@[]];
    browserIssues.spacing = 6;
    browserIssues.translatesAutoresizingMaskIntoConstraints = NO;
    browserIssues.hidden = YES;
    NSMutableArray<NSButton*>* issueButtons = [NSMutableArray array];
    for (int index = 0; index < 2; ++index) {
      NSButton* button = [NSButton buttonWithTitle:@"" target:delegate action:@selector(showBrowserIssues:)];
      button.tag = index;
      button.bezelStyle = NSBezelStyleRounded;
      button.font = [NSFont systemFontOfSize:11 weight:NSFontWeightMedium];
      // The levels' one symbol each (mitcad#1040).
      const NoticeLevel level = index == 0 ? NoticeLevel::Error : NoticeLevel::Warning;
      button.image = symbol(noticeSymbolName(level));
      button.imagePosition = NSImageLeft;
      button.contentTintColor = noticeTint(level);
      button.accessibilityIdentifier = index == 0 ? @"native.browser.failures" : @"native.browser.warnings";
      button.lineBreakMode = NSLineBreakByTruncatingTail;
      [button.widthAnchor constraintLessThanOrEqualToConstant:116].active = YES;
      [button.heightAnchor constraintEqualToConstant:24].active = YES;
      [browserIssues addArrangedSubview:button];
      [issueButtons addObject:button];
    }
    browserIssueButtons = issueButtons;
    [sidebarCard.body addSubview:browserIssues];
    [NSLayoutConstraint activateConstraints:@[
      [browserIssues.leadingAnchor constraintEqualToAnchor:sidebarCard.body.leadingAnchor constant:10],
      [browserIssues.topAnchor constraintEqualToAnchor:sidebarCard.body.topAnchor constant:32],
      [browserIssues.trailingAnchor constraintLessThanOrEqualToAnchor:sidebarCard.body.trailingAnchor constant:-10]
    ]];
    tree = [[MITCADModelOutlineView alloc] initWithFrame:NSZeroRect];
    tree->state = this;
    NSTableColumn* column = [[NSTableColumn alloc] initWithIdentifier:@"model"];
    [tree addTableColumn:column];
    tree.outlineTableColumn = column;
    tree.headerView = nil;
    tree.style = NSTableViewStyleSourceList;
    tree.rowHeight = 24;
    tree.indentationPerLevel = mitcad::panel::kTreeIndentation; // as the Browser's
    tree.backgroundColor = NSColor.clearColor;
    tree.usesAlternatingRowBackgroundColors = NO;
    tree.allowsMultipleSelection = YES;
    tree.allowsEmptySelection = YES;
    tree.dataSource = delegate;
    tree.delegate = delegate;
    tree.target = delegate;
    tree.doubleAction = @selector(activateTree:);
    tree.autosaveExpandedItems = NO; // The shared Browser owns expansion state.
    tree.accessibilityLabel = ns(QObject::tr("Browser"));
    tree.accessibilityIdentifier = @"native.modelTree";
    tree.menu = [[NSMenu alloc] initWithTitle:ns(QObject::tr("Browser"))];
    NSMenuItem* edit = [[NSMenuItem alloc] initWithTitle:ns(QObject::tr("Edit")) action:@selector(activateTree:) keyEquivalent:@""];
    edit.target = delegate;
    [tree.menu addItem:edit];
    NSMenuItem* rename = [[NSMenuItem alloc] initWithTitle:ns(QObject::tr("Rename")) action:@selector(renameTree:) keyEquivalent:@""];
    rename.target = delegate;
    [tree.menu addItem:rename];
    NSMenuItem* visibility = [[NSMenuItem alloc] initWithTitle:ns(QObject::tr("Show / Hide")) action:@selector(toggleTreeVisibility:) keyEquivalent:@""];
    visibility.target = delegate;
    [tree.menu addItem:visibility];
    NSScrollView* treeScroll = scroll(tree);
    browserScroll = treeScroll;
    treeScroll.backgroundColor = NSColor.clearColor;
    [sidebarCard.body addSubview:treeScroll];
    browserTreeTop = [treeScroll.topAnchor constraintEqualToAnchor:sidebarCard.body.topAnchor constant:38];
    [NSLayoutConstraint activateConstraints:@[
      [treeScroll.leadingAnchor constraintEqualToAnchor:sidebarCard.body.leadingAnchor constant:6],
      [treeScroll.trailingAnchor constraintEqualToAnchor:sidebarCard.body.trailingAnchor constant:-6],
      browserTreeTop,
      [treeScroll.bottomAnchor constraintEqualToAnchor:sidebarCard.body.bottomAnchor constant:-8]
    ]];
    treeRoots = [NSMutableArray array];
    treeNodes = [NSMutableDictionary dictionary];

    // The message band (mitcad#1098): placed above the timeline by
    // layoutOverlays once there is a message.
    messageCard = floatingCard(@"native.messageBand");
    messageCard.hidden = YES;
    messageBand = [[MITCADMessageBand alloc] initWithFrame:NSZeroRect];
    messageBand->closed = [this] {
      if (callbacks.messageClosed) callbacks.messageClosed();
    };
    messageBand->featureChosen = [this](const QString& uid) {
      if (callbacks.messageFeatureChosen) callbacks.messageFeatureChosen(uid);
    };
    fill(messageBand, messageCard.body);
    [canvasRoot addSubview:messageCard];
    cubeArrows = [NSMutableDictionary dictionary];

    historyCard = floatingCard(@"native.historyCard");
    historyCard.hidden = NO;
    [canvasRoot addSubview:historyCard];
    NSView* historyView = historyCard.body;
    history = [[MITCADHistoryStrip alloc] initWithFrame:NSMakeRect(0, 0, 28, historyStripHeight)];
    history->state = this;
    [history registerForDraggedTypes:@[@"com.mitcad.feature-history"]];
    history.accessibilityLabel = ns(QObject::tr("Timeline"));
    history.accessibilityIdentifier = @"native.historyStrip";
    historyButtons = [NSMutableDictionary dictionary];
    historyRunMarks = [NSMutableArray array];
    historyScroll = [[MITCADHistoryScroll alloc] initWithFrame:NSZeroRect];
    historyScroll.documentView = history;
    historyScroll.autohidesScrollers = YES;
    historyScroll.translatesAutoresizingMaskIntoConstraints = NO;
    historyScroll.accessibilityIdentifier = @"native.historyScroll";
    historyScroll.hasVerticalScroller = NO;
    historyScroll.hasHorizontalScroller = YES;
    historyScroll.scrollerStyle = NSScrollerStyleOverlay;
    historyScroll.drawsBackground = NO;
    historyScroll.contentView.drawsBackground = NO;
    historyScroll.borderType = NSNoBorder;
    [historyView addSubview:historyScroll];
    historyMarkerButton = [[MITCADHistoryMarkerButton alloc] initWithFrame:NSMakeRect(6, 0, 14, historyStripHeight)];
    historyMarkerButton->state = this;
    historyMarkerButton.title = @"";
    historyMarkerButton.bordered = NO;
    historyMarkerButton.accessibilityLabel = ns(QObject::tr("Timeline marker"));
    historyMarkerButton.accessibilityIdentifier = @"native.historyMarker";
    NSBox* markerHandle = [[NSBox alloc] initWithFrame:NSMakeRect(2, 0, 10, 10)];
    markerHandle.boxType = NSBoxCustom;
    markerHandle.borderWidth = 0;
    markerHandle.fillColor = roleColor(ColorRole::Handle);
    markerHandle.accessibilityElement = NO;
    [historyMarkerButton addSubview:markerHandle];
    NSBox* markerLine = [[NSBox alloc] initWithFrame:NSMakeRect(6, 9, 2, historyStripHeight - 10)];
    markerLine.boxType = NSBoxCustom;
    markerLine.borderWidth = 0;
    markerLine.fillColor = NSColor.labelColor;
    markerLine.accessibilityElement = NO;
    [historyMarkerButton addSubview:markerLine];
    [history addSubview:historyMarkerButton];
    historyDropLine = [[NSBox alloc] initWithFrame:NSZeroRect];
    historyDropLine.boxType = NSBoxCustom;
    historyDropLine.borderWidth = 0;
    historyDropLine.fillColor = NSColor.controlAccentColor;
    historyDropLine.hidden = YES;
    historyDropLine.accessibilityElement = NO;
    [history addSubview:historyDropLine];
    NSStackView* playback = [[NSStackView alloc] init];
    playback.orientation = NSUserInterfaceLayoutOrientationHorizontal;
    playback.spacing = 2;
    playback.translatesAutoresizingMaskIntoConstraints = NO;
    NSMutableArray<NSButton*>* steps = [NSMutableArray array];
    const QStringList symbols = {QStringLiteral("backward.end.fill"), QStringLiteral("backward.fill"),
                                 QStringLiteral("forward.fill"), QStringLiteral("forward.end.fill")};
    const QStringList names = {QStringLiteral("start"), QStringLiteral("back"), QStringLiteral("forward"), QStringLiteral("end")};
    const QStringList labels = {QStringLiteral("Roll back to start"), QStringLiteral("Previous feature"),
                                QStringLiteral("Next feature"), QStringLiteral("Roll forward to end")};
    for (int index = 0; index < names.size(); ++index) {
      NSButton* button = [NSButton buttonWithImage:symbol(symbols[index], ns(labels[index])) target:delegate action:@selector(stepHistory:)];
      button.bordered = NO;
      button.tag = index;
      button.translatesAutoresizingMaskIntoConstraints = NO;
      button.accessibilityIdentifier = ns(QStringLiteral("native.history.") + names[index]);
      button.accessibilityLabel = ns(labels[index]);
      button.toolTip = ns(labels[index]);
      [button.widthAnchor constraintEqualToConstant:24].active = YES;
      [button.heightAnchor constraintEqualToConstant:28].active = YES;
      [playback addArrangedSubview:button];
      [steps addObject:button];
    }
    historySteps = steps;
    NativeMacAction filterArtwork; filterArtwork.icon = themeIcon(QStringLiteral("filter"));
    historyFilterButton = [NSButton buttonWithImage:actionImage(filterArtwork, 16)
                                           target:delegate action:@selector(openHistoryFilter:)];
    historyFilterButton.bordered = NO;
    [historyFilterButton setButtonType:NSButtonTypePushOnPushOff];
    historyFilterButton.translatesAutoresizingMaskIntoConstraints = NO;
    historyFilterButton.accessibilityIdentifier = @"native.history.filter";
    historyFilterButton.accessibilityLabel = ns(QObject::tr("Timeline Filter"));
    [historyFilterButton.widthAnchor constraintEqualToConstant:24].active = YES;
    [historyFilterButton.heightAnchor constraintEqualToConstant:28].active = YES;
    [playback addArrangedSubview:historyFilterButton];
    NSButton* searchHistory = [NSButton buttonWithImage:symbol(QStringLiteral("magnifyingglass"))
                                                target:delegate action:@selector(openTimelineSearch:)];
    searchHistory.bordered = NO;
    searchHistory.translatesAutoresizingMaskIntoConstraints = NO;
    searchHistory.accessibilityIdentifier = @"native.history.search";
    searchHistory.toolTip = ns(QObject::tr("Search Timeline"));
    [searchHistory.widthAnchor constraintEqualToConstant:24].active = YES;
    [searchHistory.heightAnchor constraintEqualToConstant:28].active = YES;
    [playback addArrangedSubview:searchHistory];
    historySearchButton = searchHistory;
    [historyView addSubview:playback];
    [NSLayoutConstraint activateConstraints:@[
      [playback.leadingAnchor constraintEqualToAnchor:historyView.leadingAnchor constant:18],
      [playback.centerYAnchor constraintEqualToAnchor:historyView.centerYAnchor],
      [historyScroll.leadingAnchor constraintEqualToAnchor:playback.trailingAnchor constant:3],
      [historyScroll.trailingAnchor constraintEqualToAnchor:historyView.trailingAnchor constant:-8],
      [historyScroll.topAnchor constraintEqualToAnchor:historyView.topAnchor constant:4],
      [historyScroll.bottomAnchor constraintEqualToAnchor:historyView.bottomAnchor constant:-4]
    ]];
    updateHistoryAppearance();
    // The Chat card (mitcad#1075) before the inspector: made whenever the
    // inspector is, which layoutOverlays waits for.
    chatCard = floatingCard(@"native.chatCard");
    chatCard.hidden = YES;
    [canvasRoot addSubview:chatCard];
    chatHost = buildChatCard(chatCard.body);
    inspectorCard = floatingCard(@"native.inspectorCard");
    inspectorCard.hidden = YES;
    [canvasRoot addSubview:inspectorCard];
    inspectorHost = [[NSView alloc] initWithFrame:NSMakeRect(0, 0, 280, 700)];
    inspectorHost.accessibilityLabel = ns(QObject::tr("Command Panel"));
    inspectorHost.accessibilityIdentifier = @"native.inspector";
    inspectorScroll = scroll(nil);
    buildInspectorFooter();
    fill(inspectorHost, inspectorCard.body);
    addCardWidthHandles();
    layoutOverlays();
  }

  // A command form's OK and Cancel stay in view under its inputs, which
  // scroll above them, as the shared panel's page keeps them (mitcad#655,
  // mitcad#815). Without a form's footer the footer is empty and has no
  // height, and the scroll view fills the inspector.
  void buildInspectorFooter() {
    inspectorFooter = [[NSView alloc] initWithFrame:NSZeroRect];
    inspectorFooter.translatesAutoresizingMaskIntoConstraints = NO;
    inspectorFooter.accessibilityIdentifier = @"native.inspectorFooter";
    [inspectorHost addSubview:inspectorScroll];
    [inspectorHost addSubview:inspectorFooter];
    NSLayoutConstraint* empty = [inspectorFooter.heightAnchor constraintEqualToConstant:0];
    empty.priority = NSLayoutPriorityDefaultLow;
    [NSLayoutConstraint activateConstraints:@[
      [inspectorScroll.leadingAnchor constraintEqualToAnchor:inspectorHost.leadingAnchor],
      [inspectorScroll.trailingAnchor constraintEqualToAnchor:inspectorHost.trailingAnchor],
      [inspectorScroll.topAnchor constraintEqualToAnchor:inspectorHost.topAnchor],
      [inspectorScroll.bottomAnchor constraintEqualToAnchor:inspectorFooter.topAnchor],
      [inspectorFooter.leadingAnchor constraintEqualToAnchor:inspectorHost.leadingAnchor],
      [inspectorFooter.trailingAnchor constraintEqualToAnchor:inspectorHost.trailingAnchor],
      [inspectorFooter.bottomAnchor constraintEqualToAnchor:inspectorHost.bottomAnchor],
      empty
    ]];
  }

  // The inspector's content height at the card's actual width, with a
  // command form's footer: AppKit wraps text to that width, which the
  // content's own fittingSize does not (mitcad#815).
  CGFloat inspectorContentHeight() {
    if (!inspectorContent) return 180;
    if (inCanvasLayout) {
      // Inside the window's own layout pass, nested passes of the inspector
      // can keep marking the window for another constraints pass until
      // AppKit throws (Send to Slicer with Purge volumes, mitcad#1078): its
      // frames as last laid out now, measured afresh on the next turn.
      queueOverlayLayout();
    } else {
      // Wrapping labels take their width in the first pass, their height in
      // the second.
      [inspectorHost layoutSubtreeIfNeeded];
      [inspectorHost layoutSubtreeIfNeeded];
    }
    const CGFloat content = inspectorDocument ? NSHeight(inspectorDocument.frame) : inspectorContent.fittingSize.height;
    // Use the space actually reserved by the footer's layout, including
    // the button frames outside their alignment rectangles (mitcad#993).
    const CGFloat footer = NSHeight(inspectorFooter.frame);
    return std::ceil(content + footer);
  }

  // layoutOverlays once more on the next run-loop turn, outside any layout
  // pass; several requests make one.
  void queueOverlayLayout() {
    if (overlayLayoutQueued || closing) return;
    overlayLayoutQueued = true;
    __weak MITCADNativeWindowDelegate* owner = delegate;
    dispatch_async(dispatch_get_main_queue(), ^{
      MITCADNativeWindowDelegate* alive = owner;
      if (alive == nil || alive->state == nullptr) return;
      alive->state->overlayLayoutQueued = false;
      if (!alive->state->closing) alive->state->layoutOverlays();
    });
  }

  // The message band (mitcad#1098) as the Floating layout places it: right
  // above the timeline (`timeline`, its card or the room it would take), as
  // wide as it and at least as high, room for two lines of text; the stub
  // keeps that row, one line high and as wide as its text, centred and
  // dimmed. Returns the floor of the cards above it. Logs `Native message
  // band placed: full|stub x,y wxh, row <h>, timeline x,y wxh` (the canvas's
  // points, y down) when it changes.
  CGFloat placeMessageBand(CGFloat floor, NSRect timeline, CGFloat gap) {
    messageCard.hidden = ![messageBand hasMessage];
    if (messageCard.hidden) return floor;
    const CGFloat row = std::max(NSHeight(timeline), [messageBand fullHeight]);
    const bool stubbed = [messageBand isStub];
    if (stubbed) {
      const NSSize stub = [messageBand stubSize];
      const CGFloat wide = std::min(NSWidth(timeline), stub.width);
      messageCard.frame = NSMakeRect(NSMinX(timeline) + (NSWidth(timeline) - wide) / 2, floor - stub.height, wide,
                                     stub.height);
      messageCard.alphaValue = 0.8;
    } else {
      messageCard.frame = NSMakeRect(NSMinX(timeline), floor - row, NSWidth(timeline), row);
      messageCard.alphaValue = 1;
    }
    const auto rect = [](NSRect r) {
      return QStringLiteral("%1,%2 %3x%4")
          .arg(std::lround(NSMinX(r)))
          .arg(std::lround(NSMinY(r)))
          .arg(std::lround(NSWidth(r)))
          .arg(std::lround(NSHeight(r)));
    };
    const QString placed = QStringLiteral("Native message band placed: %1 %2, row %3, timeline %4")
                               .arg(stubbed ? QStringLiteral("stub") : QStringLiteral("full"), rect(messageCard.frame))
                               .arg(std::lround(row))
                               .arg(rect(timeline));
    if (placed != loggedMessageBand) {
      loggedMessageBand = placed;
      qDebug().noquote() << placed;
    }
    return floor - row - gap;
  }

  void layoutOverlays() {
    if (layingOut || !inspectorCard) return;
    layingOut = true;
    constexpr CGFloat ribbonHeight = 76;
    constexpr CGFloat margin = 12;
    constexpr CGFloat gap = 8;
    const CGFloat width = canvasRoot.bounds.size.width;
    const CGFloat height = canvasRoot.bounds.size.height;
    // The Chat card on the right edge (mitcad#1075); the other cards share
    // the canvas left of it.
    const CGFloat chatWidth = chatCard.hidden ? 0 : chatCardPlacedWidth(width - margin * 2);
    const CGFloat chatRoom = chatSpace();
    // The side cards' widths as dragged, within the canvas (mitcad#818).
    const NativeCardPlacement cards = cardWidths.place(width - chatRoom, !projectsCard.hidden, !sidebarCard.hidden,
                                                       !inspectorCard.hidden);
    ribbon.frame = NSMakeRect(0, 0, width, ribbonHeight);
    layoutRibbon(width);
    viewportHost.frame = NSMakeRect(0, ribbonHeight, width, std::max(CGFloat(1), height - ribbonHeight));
    constexpr CGFloat historyHeight = timeline::cardHeight;
    historyCard.frame = NSMakeRect(margin, height - margin - historyHeight,
                                   std::max(CGFloat(1), width - margin * 2), historyHeight);
    [historyCard layoutSubtreeIfNeeded];
    layoutHistory();
    const CGFloat floor = placeMessageBand(historyCard.hidden ? height - margin : historyCard.frame.origin.y - gap,
                                           historyCard.hidden ? NSMakeRect(margin, 0, std::max(CGFloat(1), width - margin * 2), 0)
                                                              : historyCard.frame, gap);
    // The Configuration card is as wide as Browser, at least as wide as
    // its one-row options need (Projects' least), within the free canvas.
    CGFloat configurationWidth = std::max(cards.browser, NativeCardWidths::minimum(NativeCard::Projects));
    if (!inspectorCard.hidden) {
      configurationWidth = std::max(cards.browser, std::min(configurationWidth, width - chatRoom - 2 * margin -
                                                            NativeCardWidths::freeCanvas - cards.inspector));
    }
    layoutLeftColumn(ribbonHeight + margin, floor, margin, gap, cards, configurationWidth);
    CGFloat rightTop = ribbonHeight + 190;
    CGFloat available = std::max(CGFloat(1), floor - rightTop);
    inspectorCard.frame = NSMakeRect(width - margin - chatRoom - cards.inspector, rightTop, cards.inspector, available);
    const CGFloat formHeight = inspectorContentHeight();
    // Like the shared floating layout, a long form first grows above the
    // orientation cube before it needs a scrollbar.
    if (formHeight + 4 > available) {
      rightTop = ribbonHeight + margin;
      available = std::max(CGFloat(1), floor - rightTop);
    }
    inspectorCard.frame = NSMakeRect(width - margin - chatRoom - cards.inspector, rightTop, cards.inspector,
                                     std::min(available, std::max(CGFloat(160), formHeight + 4)));
    // From below the toolbar's rows to above the timeline, as the Floating
    // chatCard.
    const CGFloat chatTop = ribbonHeight + margin;
    chatCard.frame = NSMakeRect(width - margin - chatWidth, chatTop, chatWidth, std::max(CGFloat(1), floor - chatTop));
    // The navigation bar centred just above the message band's row
    // (mitcad#1099, mitcad#1098), as the Floating layout places it.
    placeNavigationBar(width - chatRoom, floor, gap);
    if (callbacks.viewportOverlays) {
      std::vector<QRect> covered;
      for (NSView* card in @[projectsCard, configurationCard, sidebarCard, inspectorCard, historyCard, chatCard, messageCard]) {
        if (card.hidden) continue;
        const NSRect rect = card.frame;
        covered.emplace_back(int(std::lround(rect.origin.x)), int(std::lround(rect.origin.y - ribbonHeight)),
                             int(std::lround(rect.size.width)), int(std::lround(rect.size.height)));
      }
      if (navigationCard != nil && !navigationCard.hidden) {
        const NSRect rect = navigationCard.frame;
        covered.emplace_back(int(std::lround(rect.origin.x)), int(std::lround(rect.origin.y - ribbonHeight)),
                             int(std::lround(rect.size.width)), int(std::lround(rect.size.height)));
      }
      const int bottom = historyCard.hidden ? 0 : int(std::lround(height - floor));
      // The cube and its arrows left of the inspector when it reaches the
      // top, and left of the Chat card.
      const int cubeRight = !inspectorCard.hidden && rightTop < ribbonHeight + 190
                              ? int(std::lround(cards.inspector + margin + gap + chatRoom))
                              : chatCard.hidden ? 0 : int(std::lround(chatRoom + margin));
      callbacks.viewportOverlays(QMargins(0, 0, cubeRight, bottom), covered);
    }
    syncViewport();
    placeCubeArrows();
    layingOut = false;
  }

  // The Qt view's round glass buttons around the cube (GlassButton): cards
  // of the floating cards' material, as large as the circle Qt draws in
  // the button's place, with the arrow's SF Symbol.
  void setCubeArrows(const QList<NativeMacCubeArrow>& arrows) {
    QStringList shown;
    for (const NativeMacCubeArrow& arrow : arrows) {
      NSString* key = ns(arrow.id);
      MITCADFloatingCard* card = cubeArrows[key];
      if (card == nil) {
        const CGFloat side = std::max(1, std::min(arrow.geometry.width(), arrow.geometry.height()) - 4);
        card = floatingCard(ns(QStringLiteral("native.cubeArrow.") + arrow.id), side / 2);
        card.layer.shadowRadius = 3;
        card.layer.shadowOffset = CGSizeMake(0, -1);
        MITCADCommandButton* button = [MITCADCommandButton buttonWithImage:symbol(arrow.symbol)
                                                                    target:delegate
                                                                    action:@selector(turnCube:)];
        button.bordered = NO;
        button.refusesFirstResponder = YES;
        button.imagePosition = NSImageOnly;
        button.symbolConfiguration = [NSImageSymbolConfiguration configurationWithPointSize:11
                                                                                      weight:NSFontWeightSemibold];
        button.representedObject = key;
        // The Qt arrow's name, so that tests address either frontend's.
        button.accessibilityIdentifier = ns(QStringLiteral("orientationCube_") + arrow.id);
        fill(button, card.body);
        // Over the canvas, under the cards that float over it.
        [canvasRoot addSubview:card positioned:NSWindowAbove relativeTo:viewportHost];
        cubeArrows[key] = card;
      }
      for (NSView* child in card.body.subviews) {
        if (![child isKindOfClass:MITCADCommandButton.class]) continue;
        child.toolTip = ns(arrow.toolTip);
        child.accessibilityLabel = ns(arrow.toolTip);
      }
      card.hidden = drawingOpen; // the view is under the drawing's sheet
      shown.append(arrow.id);
    }
    bool gone = false;
    for (NSString* key in cubeArrows) {
      MITCADFloatingCard* card = cubeArrows[key];
      if (!card.hidden && !shown.contains(qt(key))) {
        gone = true;
        card.hidden = YES;
      }
    }
    const bool changed = gone || cubeArrowPresentation.size() != arrows.size();
    cubeArrowPresentation = arrows;
    placeCubeArrows();
    if (changed) {
      qInfo().noquote() << (shown.isEmpty() ? QStringLiteral("Native cube arrows hidden")
                                            : QStringLiteral("Native cube arrows: %1").arg(shown.join(QStringLiteral(", "))));
    }
  }

  // The cards over the canvas while a drawing is open (mitcad#1073): the
  // Browser only, from which another drawing opens; the cube's arrows go
  // with the 3D view under the sheet.
  void setDrawingOpen(bool open) {
    if (open == drawingOpen) return;
    drawingOpen = open;
    if (open) {
      NSMutableArray<NSView*>* cards = [NSMutableArray arrayWithObjects:projectsCard, inspectorCard, historyCard, nil];
      // The Chat card too (mitcad#1075), as the Qt layout's showOnlyCards.
      if (chatCard != nil) [cards addObject:chatCard];
      // And the Configuration card (mitcad#1020), as the Qt layout's.
      if (configurationCard != nil) [cards addObject:configurationCard];
      // And the navigation bar (mitcad#1099).
      if (navigationCard != nil) [cards addObject:navigationCard];
      for (NSView* card in cards) {
        const bool shown = !card.hidden;
        setCardVisible(card, false);
        [suppressedCards setObject:@(shown) forKey:card];
      }
    } else {
      NSMapTable<NSView*, NSNumber*>* wanted = suppressedCards;
      suppressedCards = [NSMapTable strongToStrongObjectsMapTable];
      for (NSView* card in wanted) setCardVisible(card, [wanted objectForKey:card].boolValue);
    }
    for (const NativeMacCubeArrow& arrow : cubeArrowPresentation) cubeArrows[ns(arrow.id)].hidden = open;
    layoutOverlays();
    applyChatButtons();
    qInfo().noquote() << (open ? QStringLiteral("Native cards: Browser only (a drawing is open)")
                               : QStringLiteral("Native cards: all (the drawing closed)"));
  }

  // At the Qt arrows' places in the viewport, inset to the circle drawn.
  void placeCubeArrows() {
    const NSPoint origin = viewportHost.frame.origin;
    for (const NativeMacCubeArrow& arrow : cubeArrowPresentation) {
      const QRect& at = arrow.geometry;
      cubeArrows[ns(arrow.id)].frame =
          NSInsetRect(NSMakeRect(origin.x + at.x(), origin.y + at.y(), at.width(), at.height()), 2, 2);
    }
  }

  // The navigation bar (mitcad#1099): the Qt bar's capsule card, its
  // buttons 18-point artwork in 28-point cells, 2 points apart, 10 points
  // from the card's ends.
  static constexpr CGFloat navigationButton = 28;
  static constexpr CGFloat navigationSpacing = 2;
  static constexpr CGFloat navigationEnd = 10;
  static constexpr CGFloat navigationHeight = navigationButton + 6;

  void setNavigationBar(const NativeMacNavigationBar& bar) {
    const bool rebuild = navigationCard == nil || bar.commands != navigationBar.commands ||
                         bar.style.id != navigationBar.style.id;
    navigationBar = bar;
    if (navigationCard == nil) {
      navigationCard = floatingCard(@"navigationBar", navigationHeight / 2);
      navigationCard.hidden = YES;
      navigationCard.accessibilityLabel = ns(QObject::tr("Navigation bar"));
      // Over the canvas with the other cards, under the timeline's search.
      [canvasRoot addSubview:navigationCard positioned:NSWindowBelow relativeTo:historyCard];
      navigationButtons = [NSMutableArray array];
    }
    if (rebuild) {
      for (NSView* button in navigationButtons) [button removeFromSuperview];
      [navigationButtons removeAllObjects];
      for (const QString& id : bar.commands) {
        MITCADNavigationButton* button = [[MITCADNavigationButton alloc] initWithFrame:NSZeroRect];
        button.bordered = NO;
        button.imagePosition = NSImageOnly;
        button.imageScaling = NSImageScaleProportionallyDown;
        button.refusesFirstResponder = YES;
        button.representedObject = ns(id);
        button.target = delegate;
        button.action = id == bar.style.id ? @selector(openNavigationStyles:) : @selector(performAction:);
        // The Qt bar's button names, so that tests address either frontend's.
        button.accessibilityIdentifier = ns(QStringLiteral("navigationBar_") + id);
        [navigationCard.body addSubview:button];
        [navigationButtons addObject:button];
      }
    }
    updateNavigationBar();
    // While a drawing is open, shown once it closes (setCardVisible).
    setCardVisible(navigationCard, bar.shown && !bar.commands.isEmpty());
    layoutOverlays();
  }

  // The main menu's item of a command, as the View menu shows it.
  static NSMenuItem* menuItemOf(NSMenu* menu, NSString* command) {
    for (NSMenuItem* item in menu.itemArray) {
      if (item.submenu) {
        if (NSMenuItem* found = menuItemOf(item.submenu, command)) return found;
      } else if (item.action == @selector(performAction:) && [item.representedObject isEqual:command]) {
        return item;
      }
    }
    return nil;
  }

  // The buttons' artwork, checks, enabled states and tooltips from the
  // action snapshot (setActions).
  void updateNavigationBar() {
    if (navigationCard == nil) return;
    QString current;
    for (const QString& id : navigationBar.styles) {
      if (const NativeMacAction* style = action(id); style && style->checked) current = style->label;
    }
    QStringList tips;
    QStringList states;
    for (MITCADNavigationButton* button in navigationButtons) {
      const QString id = qt(button.representedObject);
      const bool styleButton = id == navigationBar.style.id;
      const NativeMacAction* item = styleButton ? &navigationBar.style : action(id);
      if (item == nullptr) {
        button.enabled = NO;
        continue;
      }
      NSImage* image = actionImage(*item, 18);
      if (button.image != image) button.image = image;
      const QString tip = styleButton ? (current.isEmpty() ? item->label : QStringLiteral("%1: %2").arg(item->label, current))
                          : item->tooltip.isEmpty() ? item->label : item->tooltip;
      if (qt(button.toolTip) != tip) button.toolTip = ns(tip);
      if (qt(button.accessibilityLabel) != item->label) button.accessibilityLabel = ns(item->label);
      const bool enabled = styleButton || item->enabled;
      if (bool(button.enabled) != enabled) button.enabled = enabled;
      const NSControlStateValue value = !styleButton && item->checked ? NSControlStateValueOn : NSControlStateValueOff;
      if (button.state != value) {
        button.state = value;
        button.needsDisplay = YES;
      }
      tips.append(tip);
      if (!styleButton) {
        NSMenuItem* menuItem = menuItemOf(mainMenu, button.representedObject);
        states.append(QStringLiteral("%1 %2 (View menu %3)")
                          .arg(item->label, value == NSControlStateValueOn ? QStringLiteral("on") : QStringLiteral("off"),
                               menuItem == nil ? QStringLiteral("none")
                               : menuItem.state == NSControlStateValueOn ? QStringLiteral("on") : QStringLiteral("off")));
      }
    }
    states.append(QStringLiteral("Visual Style %1").arg(current.isEmpty() ? QStringLiteral("none") : current));
    const QString tipLine = QStringLiteral("Native navigation bar tooltips: %1").arg(tips.join(QStringLiteral(" | ")));
    if (tipLine != loggedNavigationTips) {
      loggedNavigationTips = tipLine;
      qInfo().noquote() << tipLine;
    }
    const QString stateLine = QStringLiteral("Native navigation bar states: %1").arg(states.join(QStringLiteral(", ")));
    if (stateLine != loggedNavigationStates) {
      loggedNavigationStates = stateLine;
      qInfo().noquote() << stateLine;
    }
  }

  // Places the bar centred in `width`, its bottom `gap` above `floor`;
  // returns how far the status pill moves up over it (0: no bar shown).
  CGFloat placeNavigationBar(CGFloat width, CGFloat floor, CGFloat gap) {
    if (navigationCard == nil) return 0;
    const CGFloat count = CGFloat(navigationButtons.count);
    const CGFloat barWidth = 2 * navigationEnd + count * navigationButton + std::max(CGFloat(0), count - 1) * navigationSpacing;
    navigationCard.frame = NSMakeRect(std::round((width - barWidth) / 2), floor - navigationHeight, barWidth, navigationHeight);
    CGFloat x = navigationEnd;
    for (MITCADNavigationButton* button in navigationButtons) {
      button.frame = NSMakeRect(x, (navigationHeight - navigationButton) / 2, navigationButton, navigationButton);
      x += navigationButton + navigationSpacing;
    }
    const NSRect frame = navigationCard.frame;
    const QString place = navigationCard.hidden
        ? QStringLiteral("Native navigation bar hidden")
        : QStringLiteral("Native navigation bar shown at %1,%2 %3x%4")
              .arg(std::lround(frame.origin.x)).arg(std::lround(frame.origin.y))
              .arg(std::lround(frame.size.width)).arg(std::lround(frame.size.height));
    if (place != loggedNavigationPlace) {
      loggedNavigationPlace = place;
      qInfo().noquote() << place;
    }
    return navigationCard.hidden ? 0 : navigationHeight + gap;
  }

  // The Visual Style button's pop-up: the View menu's style commands, the
  // current one checked, chosen as their View menu items are.
  void popUpNavigationStyles(NSView* button) {
    NSMenu* menu = [[NSMenu alloc] initWithTitle:ns(navigationBar.style.label)];
    NSMenuItem* checked = nil;
    QStringList entries;
    for (const QString& id : navigationBar.styles) {
      if (id.isEmpty()) {
        [menu addItem:NSMenuItem.separatorItem];
        continue;
      }
      const NativeMacAction* style = action(id);
      if (style == nullptr) continue;
      NSMenuItem* item = menuAction(*style);
      [menu addItem:item];
      if (style->checked) checked = item;
      entries.append(style->label);
    }
    qInfo().noquote() << QStringLiteral("Native navigation bar styles: %1").arg(entries.join(QStringLiteral(" | ")));
    [menu popUpMenuPositioningItem:checked atLocation:NSMakePoint(0, NSHeight(button.bounds)) inView:button];
  }

  void setCardVisible(NSView* card, bool shown) {
    // A card an open drawing hides is shown as asked once it closes.
    if ([suppressedCards objectForKey:card] != nil) {
      [suppressedCards setObject:@(shown) forKey:card];
      shown = false;
    }
    if (card.hidden == !shown) return;
    if (card == historyCard && !shown) finishHistoryMarkerDrag();
    if (card == historyCard && !shown) hoverHistoryCell(QString());
    if (!shown) {
      NSResponder* responder = window.firstResponder;
      NSView* editingView = [responder isKindOfClass:NSView.class] ? (NSView*)responder : nil;
      if ([responder isKindOfClass:NSTextView.class] && ((NSTextView*)responder).fieldEditor) {
        id control = ((NSTextView*)responder).delegate;
        if ([control isKindOfClass:NSView.class]) editingView = (NSView*)control;
      }
      if ([editingView isDescendantOf:card] && viewport) {
        NSView* canvas = (__bridge NSView*)reinterpret_cast<void*>(viewport->winId());
        if (![window makeFirstResponder:canvas]) return;
      }
    }
    card.hidden = !shown;
    if (card == sidebarCard && !shown) hoverTree(nil);
    layoutOverlays();
  }

  // The Browser row under the pointer (mitcad#1072): nil when it is over
  // none, the outline or the card left. Reported when it changes.
  void hoverTree(NSString* key) {
    const QString row = key != nil ? qt(key) : QString();
    if (row == hoveredTreeRow) return;
    hoveredTreeRow = row;
    if (!closing && callbacks.treeHovered) callbacks.treeHovered(row);
  }

  // The Configuration card (mitcad#1020): the shared Configuration bar
  // between Projects and Browser, shown while the design has options or
  // rows (setConfigurationView), its content folded under its header by
  // the disclosure (configuration/panelCollapsed, shared with Qt).
  void buildConfigurationCard() {
    configurationCard = floatingCard(@"native.configurationCard");
    configurationCard.hidden = YES;
    [canvasRoot addSubview:configurationCard];
    NSTextField* title = label(ns(QObject::tr("Configuration")), [NSFont systemFontOfSize:13 weight:NSFontWeightSemibold]);
    [configurationCard.body addSubview:title];
    configurationDisclosure = [NSButton buttonWithImage:symbol(QStringLiteral("chevron.down"))
                                                 target:delegate action:@selector(collapseConfiguration:)];
    configurationDisclosure.bordered = NO;
    configurationDisclosure.translatesAutoresizingMaskIntoConstraints = NO;
    configurationDisclosure.accessibilityIdentifier = @"native.configuration.disclosure";
    configurationDisclosure.accessibilityLabel = ns(QObject::tr("Collapse Configuration"));
    [configurationCard.body addSubview:configurationDisclosure];
    configurationScroll = [[NSScrollView alloc] initWithFrame:NSZeroRect];
    configurationScroll.translatesAutoresizingMaskIntoConstraints = NO;
    configurationScroll.drawsBackground = NO;
    configurationScroll.hasVerticalScroller = YES;
    configurationScroll.hasHorizontalScroller = NO;
    configurationScroll.automaticallyAdjustsContentInsets = NO;
    configurationScroll.autohidesScrollers = YES;
    configurationScroll.hidden = YES;
    configurationScroll.accessibilityIdentifier = @"native.configuration";
    [configurationCard.body addSubview:configurationScroll];
    // Folded or hidden content must not make the header-high card grow.
    configurationScrollBottom = [configurationScroll.bottomAnchor constraintEqualToAnchor:configurationCard.body.bottomAnchor
                                                                                constant:-8];
    [NSLayoutConstraint activateConstraints:@[
      [configurationDisclosure.leadingAnchor constraintEqualToAnchor:configurationCard.body.leadingAnchor constant:8],
      [configurationDisclosure.topAnchor constraintEqualToAnchor:configurationCard.body.topAnchor constant:5],
      [configurationDisclosure.widthAnchor constraintEqualToConstant:18],
      [configurationDisclosure.heightAnchor constraintEqualToConstant:20],
      [title.leadingAnchor constraintEqualToAnchor:configurationDisclosure.trailingAnchor constant:6],
      [title.centerYAnchor constraintEqualToAnchor:configurationDisclosure.centerYAnchor],
      [configurationScroll.leadingAnchor constraintEqualToAnchor:configurationCard.body.leadingAnchor constant:6],
      [configurationScroll.trailingAnchor constraintEqualToAnchor:configurationCard.body.trailingAnchor constant:-6],
      [configurationScroll.topAnchor constraintEqualToAnchor:configurationCard.body.topAnchor constant:30]
    ]];
  }

  // The Configuration card's content folded under its header or shown.
  void showConfigurationCollapsed(bool collapsed) {
    configurationCollapsed = collapsed;
    configurationScroll.hidden = collapsed || configurationContent == nil;
    configurationScrollBottom.active = !configurationScroll.hidden;
    configurationDisclosure.image = symbol(collapsed ? QStringLiteral("chevron.right") : QStringLiteral("chevron.down"));
    configurationDisclosure.accessibilityLabel =
        collapsed ? ns(QObject::tr("Expand Configuration")) : ns(QObject::tr("Collapse Configuration"));
  }

  // A left card by its name in the column ("projects", "configuration",
  // "browser").
  NSView* columnCard(const QString& name) const {
    if (name == QLatin1String("projects")) return projectsCard;
    if (name == QLatin1String("configuration")) return configurationCard;
    return sidebarCard;
  }

  // The left column (mitcad#1020): Projects, Configuration and Browser
  // share its free height from `top` to `bottom` without fixed caps, as the
  // Floating layout's left cards do (shareNativeColumn): a card whose
  // content is shorter than its share keeps its content's height and the
  // others share the rest, by the user's split when there is one; a
  // collapsed card keeps its header. A handle in each gap between two open
  // neighbours drags the split.
  void layoutLeftColumn(CGFloat top, CGFloat bottom, CGFloat margin, CGFloat gap, const NativeCardPlacement& cards,
                        CGFloat configurationWidth) {
    std::vector<NativeColumnCard> column;
    std::vector<std::pair<NSView*, CGFloat>> placed; // each card and its width
    // Projects: its AppKit rows measured, not the hidden Qt tree's hints.
    NSOutlineView* projectsOutline = nil;
    CGFloat projectsChrome = 0;
    CGFloat projectsRow = 24;
    if (!projectsCard.hidden) {
      projectsOutline = outlineIn(projectsContent);
      const NSInteger count = projectsOutline.numberOfRows;
      const NSRect first = count > 0 ? [projectsOutline rectOfRow:0] : NSZeroRect;
      projectsRow = count > 1 ? [projectsOutline rectOfRow:1].origin.y - first.origin.y
                              : projectsOutline ? projectsOutline.rowHeight : 24;
      projectsRow = std::max(CGFloat(1), projectsRow);
      // Source-list style adds its own leading inset before the first row.
      projectsChrome = 38 + 24 + 2 + std::max(CGFloat(0), first.origin.y);
      NativeColumnCard card;
      card.name = QStringLiteral("projects");
      card.collapsed = projectsCollapsed;
      card.want = projectsCollapsed ? 30 : projectsChrome + CGFloat(std::max<NSInteger>(2, count)) * projectsRow;
      card.least = projectsCollapsed ? 30 : projectsChrome + projectsRow;
      column.push_back(card);
      placed.emplace_back(projectsCard, cards.projects);
    }
    if (!configurationCard.hidden) {
      CGFloat content = 0;
      if (!configurationCollapsed) {
        // Its width first. The first pass gives labels their bounded
        // width, the second their wrapped height, as for the inspector's
        // shared form content.
        configurationCard.frame = NSMakeRect(margin, top, configurationWidth,
                                             std::max(CGFloat(60), configurationCard.frame.size.height));
        [configurationCard layoutSubtreeIfNeeded];
        [configurationCard layoutSubtreeIfNeeded];
        content = std::ceil(configurationDocument.frame.size.height);
      }
      NativeColumnCard card;
      card.name = QStringLiteral("configuration");
      card.collapsed = configurationCollapsed;
      card.want = configurationCollapsed ? 30 : 30 + content + 8;
      card.least = configurationCollapsed ? 30 : 30 + std::min(content, CGFloat(32)) + 8;
      column.push_back(card);
      placed.emplace_back(configurationCard, configurationWidth);
    }
    const CGFloat headerHeight = browserIssues.hidden ? 38 : 66;
    if (!sidebarCard.hidden) {
      // Its width before measuring its rows.
      sidebarCard.frame = NSMakeRect(margin, top, cards.browser, sidebarCard.frame.size.height);
      // Source-list styles can leave an inset before their first row. The
      // rowHeight alone omits that inset, intercell spacing and scroll-view
      // borders, leaving only the top of Root visible.
      CGFloat rowSpace = tree.rowHeight;
      CGFloat rows = 0;
      if (tree.numberOfRows > 0) {
        const NSRect row = [tree rectOfRow:0];
        const CGFloat inset = std::max(CGFloat(0), NSMinY(row) - NSMinY(tree.visibleRect));
        const CGFloat chrome = std::max(CGFloat(0), NSHeight(browserScroll.bounds) - NSHeight(browserScroll.contentView.bounds));
        rowSpace = std::ceil(NSHeight(row) + tree.intercellSpacing.height + inset + chrome);
        rows = std::ceil(NSMaxY([tree rectOfRow:tree.numberOfRows - 1]) + chrome);
      }
      const CGFloat collapsed = browserIssues.hidden ? 30 : 60;
      NativeColumnCard card;
      card.name = QStringLiteral("browser");
      card.collapsed = browserCollapsed;
      card.want = browserCollapsed ? collapsed : std::max(CGFloat(100), std::max(rows + 4, rowSpace) + headerHeight + 8);
      card.least = browserCollapsed ? collapsed : headerHeight + rowSpace + 8;
      column.push_back(card);
      placed.emplace_back(sidebarCard, cards.browser);
    }
    const CGFloat gaps = gap * CGFloat(std::max<std::size_t>(1, column.size()) - 1);
    std::vector<CGFloat> heights = shareNativeColumn(column, std::max(CGFloat(0), bottom - top - gaps), columnSplit);
    // Projects shows whole rows; what that leaves goes to the next open
    // card that wants more.
    if (!projectsCard.hidden && !projectsCollapsed && !heights.empty()) {
      const int fit = std::max(1, int(std::floor((heights[0] - projectsChrome) / projectsRow)));
      const CGFloat snapped = projectsChrome + fit * projectsRow;
      if (snapped < heights[0]) {
        for (std::size_t i = 1; i < column.size(); ++i) {
          if (!column[i].collapsed && heights[i] < column[i].want) {
            heights[i] += heights[0] - snapped;
            break;
          }
        }
        heights[0] = snapped;
      }
    }
    CGFloat y = top;
    for (std::size_t i = 0; i < placed.size(); ++i) {
      NSView* card = placed[i].first;
      const bool resized = card.frame.size.height != heights[i];
      card.frame = NSMakeRect(margin, y, placed[i].second, heights[i]);
      if (card == projectsCard) {
        [projectsCard layoutSubtreeIfNeeded];
        fitCardOutline(projectsCard, projectsOutline); // its names take the card's width (mitcad#818)
        if (resized && !projectsCollapsed && projectsOutline.selectedRow >= 0)
          [projectsOutline scrollRowToVisible:projectsOutline.selectedRow];
      }
      y += heights[i] + gap;
    }
    browserTreeTop.constant = headerHeight;
    fitCardOutline(sidebarCard, tree);
    columnCards = column;
    placeColumnSplitHandles(placed, margin);
  }

  // The handles in the gaps between open neighbours of the left column,
  // named by their cards: native.columnSplit.<above>.<below>.
  void placeColumnSplitHandles(const std::vector<std::pair<NSView*, CGFloat>>& placed, CGFloat margin) {
    const auto title = [](const QString& name) {
      return name == QLatin1String("projects")        ? QObject::tr("Projects")
             : name == QLatin1String("configuration") ? QObject::tr("Configuration")
                                                      : QObject::tr("Browser");
    };
    if (columnSplitHandles == nil) columnSplitHandles = [NSMutableArray array];
    splitPairs.clear();
    NSUInteger used = 0;
    for (std::size_t i = 0; i + 1 < placed.size(); ++i) {
      if (columnCards[i].collapsed || columnCards[i + 1].collapsed) continue;
      if (used == columnSplitHandles.count) {
        const std::size_t index = used;
        NSView* handle = makeColumnSplitHandle([this, index](NativeSplitPhase phase, CGFloat down) {
          dragColumnSplit(index, phase, down);
        });
        // Over the canvas in the gap; the cards beside it keep theirs.
        [canvasRoot addSubview:handle positioned:NSWindowAbove relativeTo:viewportHost];
        [columnSplitHandles addObject:handle];
      }
      NSView* handle = columnSplitHandles[used++];
      const NSRect above = placed[i].first.frame;
      const NSRect below = placed[i + 1].first.frame;
      handle.frame = NSMakeRect(margin, NSMaxY(above), std::max(NSWidth(above), NSWidth(below)),
                                std::max(CGFloat(1), NSMinY(below) - NSMaxY(above)));
      handle.hidden = NO;
      handle.accessibilityIdentifier =
          ns(QStringLiteral("native.columnSplit.%1.%2").arg(columnCards[i].name, columnCards[i + 1].name));
      handle.accessibilityLabel =
          ns(QObject::tr("%1 and %2 height").arg(title(columnCards[i].name), title(columnCards[i + 1].name)));
      splitPairs.emplace_back(columnCards[i].name, columnCards[i + 1].name);
    }
    for (NSUInteger i = used; i < columnSplitHandles.count; ++i) columnSplitHandles[i].hidden = YES;
  }

  // A boundary of the left column pressed, dragged `down` points, released
  // or double-clicked (mitcad#1020): the drag moves height between its two
  // cards, each keeping its header and a row; the others keep theirs.
  void dragColumnSplit(std::size_t index, NativeSplitPhase phase, CGFloat down) {
    if (phase == NativeSplitPhase::Reset) {
      splitDragging = false;
      columnSplit.clear();
      layoutOverlays();
      if (callbacks.columnSplitChosen) callbacks.columnSplitChosen(columnSplit);
      return;
    }
    if (phase == NativeSplitPhase::Began) {
      splitDragging = false;
      splitMoved = false;
      splitDragStart.clear();
      if (index >= splitPairs.size()) return;
      splitDragPair = splitPairs[index];
      splitDragging = true;
      for (const NativeColumnCard& card : columnCards) {
        if (!card.collapsed)
          splitDragStart.emplace_back(card.name, int(std::lround(columnCard(card.name).frame.size.height)));
      }
      return;
    }
    if (!splitDragging) return;
    const auto find = [this](const QString& name) {
      return std::find_if(splitDragStart.begin(), splitDragStart.end(),
                          [&name](const std::pair<QString, int>& kept) { return kept.first == name; });
    };
    const auto above = find(splitDragPair.first);
    const auto below = find(splitDragPair.second);
    if (above == splitDragStart.end() || below == splitDragStart.end()) return;
    const auto least = [this](const QString& name, int height) {
      for (const NativeColumnCard& card : columnCards) {
        if (card.name == name) return std::min(height, int(std::ceil(card.least)));
      }
      return height;
    };
    // A card's height for its content (mitcad#1152): the shares give a card
    // whose weight reaches it exactly that height and the rest to all the
    // other cards, so a drag past it would move the other boundaries.
    const auto want = [this](const QString& name) {
      for (const NativeColumnCard& card : columnCards) {
        if (card.name == name && card.want > 0) return int(std::floor(card.want));
      }
      return std::numeric_limits<int>::max() / 4;
    };
    const int total = above->second + below->second;
    const int lowest = std::max(least(above->first, above->second), total - want(below->first));
    const int highest = std::min(total - least(below->first, below->second), want(above->first));
    const int upper = std::clamp(above->second + int(std::lround(down)), std::min(lowest, above->second),
                                 std::max(highest, above->second));
    splitMoved = splitMoved || std::lround(down) != 0;
    columnSplit = splitDragStart;
    for (auto& [name, height] : columnSplit) {
      if (name == splitDragPair.first) height = upper;
      else if (name == splitDragPair.second) height = total - upper;
    }
    layoutOverlays();
    if (phase == NativeSplitPhase::Ended) {
      splitDragging = false;
      if (splitMoved && callbacks.columnSplitChosen) callbacks.columnSplitChosen(columnSplit);
    }
  }

  // Width handles along the side cards' inner edges (mitcad#818): a drag
  // places the card at once, a release remembers its width.
  void addCardWidthHandles() {
    const auto handle = [this](NSView* card, NativeCard which) {
      return addCardWidthHandle(card, which, [this, which](CGFloat width, bool finished) {
        dragCardWidth(which, width, finished);
      });
    };
    cardWidthHandles = @[handle(projectsCard, NativeCard::Projects), handle(sidebarCard, NativeCard::Browser),
                         handle(inspectorCard, NativeCard::Inspector)];
  }

  void dragCardWidth(NativeCard card, CGFloat width, bool finished) {
    const CGFloat chosen = cardWidths.drag(card, width, canvasRoot.bounds.size.width - chatSpace(), !projectsCard.hidden,
                                           !sidebarCard.hidden, !inspectorCard.hidden);
    layoutOverlays();
    if (finished && callbacks.cardWidthChosen) callbacks.cardWidthChosen(nativeCardName(card), int(std::lround(chosen)));
  }

  void embedViewport() {
    if (!viewport || viewport->parentWidget()) {
      error = QStringLiteral("The native Mac canvas requires a standalone QWidget.");
    } else {
      viewport->setAttribute(Qt::WA_NativeWindow);
      // VoiceOver names the canvas as its host (mitcad#1026).
      if (viewport->accessibleName().isEmpty()) viewport->setAccessibleName(QObject::tr("3D view"));
      (void)viewport->winId();
      foreignWindow.reset(QWindow::fromWinId(WId((__bridge void*)viewportHost)));
      if (!foreignWindow || !viewport->windowHandle()) {
        error = QStringLiteral("Qt could not wrap the native Mac canvas host.");
      } else {
        viewport->windowHandle()->setParent(foreignWindow.get());
        // A Qt child window owns its existing OpenGL framebuffer; AppKit
        // only positions its NSView. No private Qt platform API is used.
        viewport->show();
        syncViewport();
        NSView* embedded = (__bridge NSView*)reinterpret_cast<void*>(viewport->winId());
        if (![embedded isDescendantOf:viewportHost]) {
          viewport->hide();
          viewport->windowHandle()->setParent(nullptr);
          foreignWindow.reset();
          error = QStringLiteral("Qt did not attach the CAD view to its native Mac host.");
        }
      }
    }
    if (!error.isEmpty()) {
      qCritical().noquote() << "Native Mac canvas:" << error;
      NSTextField* failure = label(ns(error));
      failure.lineBreakMode = NSLineBreakByWordWrapping;
      failure.textColor = NSColor.systemRedColor;
      fill(failure, viewportHost, 24);
    }
  }

  void syncViewport() {
    if (!viewport || !foreignWindow || !viewport->windowHandle()) return;
    const NSSize size = viewportHost.bounds.size;
    const int width = std::max(1, int(std::lround(size.width)));
    const int height = std::max(1, int(std::lround(size.height)));
    const QSize dimensions(width, height);
    const CGFloat scale = viewportHost.window.backingScaleFactor;
    if (viewportSize == dimensions && viewportScale == scale) return;
    if (viewportSize != dimensions) {
      viewport->resize(width, height);
      viewport->windowHandle()->setGeometry(0, 0, width, height);
      viewportSize = dimensions;
    }
    viewportScale = scale;
    viewport->update();
  }

  bool dispatchGesture(NSEvent* event) {
    const NSEventType type = event.type;
    if (type != NSEventTypeMagnify && type != NSEventTypeRotate && type != NSEventTypeSmartMagnify) return false;
    if (event.window != window) return false;
    if (closing || nativeModalBlocked() || !viewport || !viewport->isVisible() || !viewport->isEnabled()) {
      gestureReceiver.clear();
      gestureKinds = 0;
      return true;
    }
    NSView* embedded = (__bridge NSView*)reinterpret_cast<void*>(viewport->internalWinId());
    if (!embedded || ![embedded isDescendantOf:viewportHost]) return false;
    NSPoint point = event.locationInWindow;
    if (!std::isfinite(point.x) || !std::isfinite(point.y)) point = [window convertPointFromScreen:NSEvent.mouseLocation];
    const NSPoint nativeLocal = [embedded convertPoint:point fromView:nil];
    if (!std::isfinite(nativeLocal.x) || !std::isfinite(nativeLocal.y)) return bool(gestureReceiver);
    // NSView coordinates are logical points. OcctViewer applies its backing
    // scale when handling the Qt gesture, just as it does for wheel events.
    const QPointF local(nativeLocal.x, embedded.flipped ? nativeLocal.y : embedded.bounds.size.height - nativeLocal.y);
    const auto receiverAtPoint = [&]() -> QWidget* {
      NSView* parent = canvasRoot.superview;
      const NSPoint hitPoint = parent != nil ? [parent convertPoint:point fromView:nil] : point;
      NSView* hit = [canvasRoot hitTest:hitPoint];
      if (!hit || (hit != viewportHost && ![hit isDescendantOf:viewportHost])) return nullptr;
      QWidget* receiver = viewport->childAt(local);
      if (!receiver) receiver = viewport;
      // Qt's CAD overlays may be under the pointer; their GL parent owns
      // navigation. Native floating cards have already failed the hit test.
      while (receiver && !qobject_cast<QOpenGLWidget*>(receiver)) {
        if (receiver == viewport) return nullptr;
        receiver = receiver->parentWidget();
      }
      return receiver && receiver->isVisible() && receiver->isEnabled() ? receiver : nullptr;
    };
    const auto send = [&](QWidget* receiver, Qt::NativeGestureType gesture, qreal value) {
      if (!receiver) return false;
      const QPointF receiverLocal = receiver->mapFrom(viewport, local);
      QNativeGestureEvent translated(gesture, QPointingDevice::primaryPointingDevice(), 2,
                                     receiverLocal, local, viewport->mapToGlobal(local), value, QPointF());
      translated.setTimestamp(static_cast<ulong>(event.timestamp * 1000));
      QApplication::sendEvent(receiver, &translated);
      return true;
    };
    if (type == NSEventTypeSmartMagnify) return send(receiverAtPoint(), Qt::SmartZoomNativeGesture, 1.0);

    const unsigned kind = type == NSEventTypeMagnify ? 1u : 2u;
    const NSEventPhase phase = event.phase;
    const bool began = phase & NSEventPhaseBegan;
    const bool cancelled = phase & NSEventPhaseCancelled;
    const bool ended = cancelled || (phase & NSEventPhaseEnded);
    const qreal value = type == NSEventTypeMagnify ? event.magnification : -event.rotation;
    if (phase == NSEventPhaseNone) {
      QWidget* receiver = receiverAtPoint();
      return receiver && (!std::isfinite(value) || send(receiver, type == NSEventTypeMagnify ? Qt::ZoomNativeGesture : Qt::RotateNativeGesture, value));
    }
    // A second begin of the same kind starts a fresh sequence. Magnification
    // and rotation can interleave and retain the same original receiver.
    if (began && (gestureKinds & kind)) { gestureReceiver.clear(); gestureKinds = 0; }
    if (!gestureKinds && !ended) gestureReceiver = receiverAtPoint();
    if (!ended) gestureKinds |= kind;
    const bool captured = bool(gestureReceiver);
    if (began) send(gestureReceiver, Qt::BeginNativeGesture, 0.0);
    else if (ended) send(gestureReceiver, Qt::EndNativeGesture, 0.0);
    else if (std::isfinite(value)) send(gestureReceiver, type == NSEventTypeMagnify ? Qt::ZoomNativeGesture : Qt::RotateNativeGesture, value);
    if (ended) {
      gestureKinds = cancelled ? 0 : gestureKinds & ~kind;
      if (!gestureKinds) gestureReceiver.clear();
    }
    // Consume captured events so Qt's Cocoa view cannot deliver them again.
    // A sequence begun on a native card remains AppKit-only until its end.
    return captured;
  }

  NSMenuItem* menuAction(const NativeMacAction& item, bool image = false) {
    if (item.separator) return NSMenuItem.separatorItem;
    NSMenuItem* menuItem = [[NSMenuItem alloc] initWithTitle:ns(item.label) action:@selector(performAction:) keyEquivalent:@""];
    menuItem.target = delegate;
    menuItem.representedObject = ns(item.id);
    menuItem.enabled = item.enabled;
    menuItem.state = item.checked ? NSControlStateValueOn : NSControlStateValueOff;
    menuItem.toolTip = ns(item.tooltip);
    setShortcut(menuItem, item.shortcut);
    if (image) {
      menuItem.image = actionImage(item);
      if (@available(macOS 27.0, *)) menuItem.preferredImageVisibility = NSMenuItemImageVisibilityVisible;
    }
    return menuItem;
  }

  void addResponderItem(NSMenu* menu, NSString* title, SEL selector, NSString* key, id target = nil) {
    NSMenuItem* item = [[NSMenuItem alloc] initWithTitle:title action:selector keyEquivalent:key];
    item.target = target;
    [menu addItem:item];
  }

  void installMenus() {
    if (closing) return;
    NSApp.delegate = delegate;
    NSApp.mainMenu = mainMenu;
    NSApp.servicesMenu = servicesMenu;
    NSApp.windowsMenu = windowsMenu;
  }

  void rebuildMenus() {
    if (!mainMenu) {
      previousMenu = NSApp.mainMenu;
      previousServicesMenu = NSApp.servicesMenu;
      previousWindowsMenu = NSApp.windowsMenu;
    }
    mainMenu = [[NSMenu alloc] initWithTitle:@"Main Menu"];
    NSMenu* appMenu = [[NSMenu alloc] initWithTitle:@"MitCad"];
    NSMenuItem* appItem = [[NSMenuItem alloc] initWithTitle:@"MitCad" action:nil keyEquivalent:@""];
    appItem.submenu = appMenu;
    [mainMenu addItem:appItem];
    for (const auto& item : actions) if (isAppAction(item)) [appMenu addItem:menuAction(item)];
    [appMenu addItem:NSMenuItem.separatorItem];
    NSMenu* services = [[NSMenu alloc] initWithTitle:@"Services"];
    NSMenuItem* servicesItem = [[NSMenuItem alloc] initWithTitle:@"Services" action:nil keyEquivalent:@""];
    servicesItem.submenu = services;
    [appMenu addItem:servicesItem];
    servicesMenu = services;
    [appMenu addItem:NSMenuItem.separatorItem];
    addResponderItem(appMenu, @"Hide MitCad", @selector(hide:), @"h");
    NSMenuItem* hideOthers = [[NSMenuItem alloc] initWithTitle:@"Hide Others" action:@selector(hideOtherApplications:) keyEquivalent:@"h"];
    hideOthers.keyEquivalentModifierMask = NSEventModifierFlagCommand | NSEventModifierFlagOption;
    [appMenu addItem:hideOthers];
    addResponderItem(appMenu, @"Show All", @selector(unhideAllApplications:), @"");
    [appMenu addItem:NSMenuItem.separatorItem];
    NSMenuItem* quit = [[NSMenuItem alloc] initWithTitle:@"Quit MitCad" action:@selector(quitApplication:) keyEquivalent:@"q"];
    quit.target = delegate;
    [appMenu addItem:quit];

    QStringList titles = {QStringLiteral("File"), QStringLiteral("Edit"), QStringLiteral("View")};
    for (const NativeMacAction& item : actions) {
      if (isAppAction(item)) continue;
      const QString title = actionMenuPath(item).section(QLatin1Char('/'), 0, 0);
      if (title.isEmpty()) continue;
      if (!titles.contains(title)) titles.append(title);
    }
    const int help = titles.indexOf(QStringLiteral("Help"));
    if (!titles.contains(QStringLiteral("Window"))) titles.insert(help < 0 ? titles.size() : help, QStringLiteral("Window"));
    for (const QString& title : titles) {
      NSMenu* menu = [[NSMenu alloc] initWithTitle:ns(title)];
      NSMenuItem* top = [[NSMenuItem alloc] initWithTitle:ns(title) action:nil keyEquivalent:@""];
      top.submenu = menu;
      [mainMenu addItem:top];
      if (title == QStringLiteral("Window")) {
        for (const NativeMacAction& item : actions) {
          if (actionMenuPath(item).section(QLatin1Char('/'), 0, 0) == title) [menu addItem:menuAction(item)];
        }
        [menu addItem:NSMenuItem.separatorItem];
        // Validation and dispatch belong to this document's real window,
        // even while another window owns the application responder chain.
        addResponderItem(menu, @"Show Tab Bar", @selector(toggleTabBar:), @"", window);
        addResponderItem(menu, @"Show All Tabs", @selector(toggleTabOverview:), @"", window);
        addResponderItem(menu, @"Merge All Windows", @selector(mergeAllWindows:), @"", window);
        [menu addItem:NSMenuItem.separatorItem];
        addResponderItem(menu, @"Zoom", @selector(performZoom:), @"", window);
        [menu addItem:NSMenuItem.separatorItem];
        addResponderItem(menu, @"Bring All to Front", @selector(arrangeInFront:), @"", NSApp);
        windowsMenu = menu;
        continue;
      }
      QHash<QString, NSMenu*> submenus;
      submenus.insert(title, menu);
      for (const NativeMacAction& item : actions) {
        if (isAppAction(item)) continue;
        // A popup parent is represented by the submenu created from its
        // children's paths; it must not also become a no-op menu command.
        if (!item.submenu.isEmpty()) continue;
        const QString path = actionMenuPath(item);
        if (path.isEmpty()) continue;
        const QStringList parts = path.split(QLatin1Char('/'), Qt::SkipEmptyParts);
        if (parts.isEmpty() || parts.front() != title) continue;
        QString prefix = title;
        NSMenu* target = menu;
        for (int index = 1; index < parts.size(); ++index) {
          prefix += QLatin1Char('/') + parts[index];
          if (submenus.contains(prefix)) { target = submenus.value(prefix); continue; }
          NSMenu* child = [[NSMenu alloc] initWithTitle:ns(parts[index])];
          NSMenuItem* childItem = [[NSMenuItem alloc] initWithTitle:ns(parts[index]) action:nil keyEquivalent:@""];
          childItem.submenu = child;
          [target addItem:childItem];
          target = child;
          submenus.insert(prefix, child);
        }
        if (item.id == QStringLiteral("file.close")) {
          NSMenuItem* close = [[NSMenuItem alloc] initWithTitle:ns(item.label) action:@selector(closeDocument:) keyEquivalent:@"w"];
          close.target = delegate;
          [target addItem:close];
        } else {
          [target addItem:menuAction(item)];
        }
      }
      if (title == QStringLiteral("Edit")) {
        // AppKit text fields take Cut, Copy, Paste and Select All (and
        // their Command keys) only from the menu bar; Qt's editors handle
        // them themselves. After the shared Edit commands, which keep their
        // order; the canvas's own shortcuts are offered to the window first.
        [menu addItem:NSMenuItem.separatorItem];
        addResponderItem(menu, @"Cut", @selector(cut:), @"x");
        addResponderItem(menu, @"Copy", @selector(copy:), @"c");
        addResponderItem(menu, @"Paste", @selector(paste:), @"v");
        addResponderItem(menu, @"Select All", @selector(selectAll:), @"a");
      }
    }
    if (window.keyWindow || documentStates.size() == 1) installMenus();
  }

  // The agent indicator's button (mitcad#710): its text, dot and tooltip.
  void applyAgentStatus(NSButton* button) const {
    button.title = ns(agentText);
    button.toolTip = ns(agentTooltip);
    button.accessibilityLabel = ns(QObject::tr("AI assistant: %1").arg(agentText));
    button.image = symbol(agentConnected   ? QStringLiteral("circle.fill")
                          : agentAvailable ? QStringLiteral("circle")
                                           : QStringLiteral("circle.slash"));
    button.contentTintColor = agentConnected   ? NSColor.controlAccentColor
                              : agentAvailable ? NSColor.secondaryLabelColor
                                               : NSColor.tertiaryLabelColor;
  }

  void installToolbar() {
    NSToolbar* toolbar = [[NSToolbar alloc] initWithIdentifier:@"com.mitcad.toolbar.familiar.v1"];
    toolbar.delegate = delegate;
    toolbar.allowsUserCustomization = YES;
    toolbar.autosavesConfiguration = YES;
    toolbar.displayMode = NSToolbarDisplayModeIconOnly;
    window.toolbar = toolbar;
    buildRibbon();
  }

  QString workspace() const { return workspaces.value(workspaceIndex, QStringLiteral("SOLID")); }

  NSMenu* presentationMenu(const QList<NativeMacMenuItem>& entries) {
    NSMenu* menu = [[NSMenu alloc] initWithTitle:@""];
    for (const auto& entry : entries) {
      if (entry.separator) { [menu addItem:NSMenuItem.separatorItem]; continue; }
      const NativeMacAction* command = action(entry.action);
      NSMenuItem* item = command ? menuAction(*command, true)
                                : [[NSMenuItem alloc] initWithTitle:ns(entry.title) action:nil keyEquivalent:@""];
      if (entry.submenu) item.submenu = presentationMenu(entry.children);
      if (!entry.icon.isNull() && !command) {
        NativeMacAction artwork; artwork.icon = entry.icon;
        item.image = actionImage(artwork, 16);
      }
      [menu addItem:item];
    }
    return menu;
  }

  NSMenu* groupMenu(const QString& group) {
    for (const auto& item : ribbonPresentation) {
      if (item.workspace == workspace() && item.name == group) return presentationMenu(item.menu);
    }
    return [[NSMenu alloc] initWithTitle:ns(group)];
  }

  const NativeMacRibbonGroup* ribbonGroup(const QString& name) const {
    for (const auto& group : ribbonPresentation) {
      if (group.workspace == workspace() && group.name == name) return &group;
    }
    return nullptr;
  }

  // A row's Pin to Toolbar or Change Keyboard Shortcut… for a command: its
  // tab, id and the menu's place ("SOLID/CREATE") for the log.
  NSMenuItem* rowCommand(const NativeMacAction& command, const QString& tab, const QString& place, bool pin,
                         const QString& title) {
    NSMenuItem* item = [[NSMenuItem alloc] initWithTitle:ns(title)
                                                  action:pin ? @selector(pinCommand:) : @selector(changeShortcut:)
                                           keyEquivalent:@""];
    item.target = delegate;
    item.representedObject = @[ns(tab), ns(command.id), ns(place)];
    return item;
  }

  // The commands of a group menu's entries, for its Pin to Toolbar (checked
  // where pinned, disabled for a command that is a button of its group
  // already: mitcad#1084) or Change Keyboard Shortcut submenu; a
  // subgroup's in a submenu of its own, rows that are not commands (the
  // selection filters) left out, as Qt's rows without the ⋮ button.
  NSMenu* rowCommandsMenu(const QList<NativeMacMenuItem>& entries, const QString& tab, const QString& place, bool pin) {
    NSMenu* menu = [[NSMenu alloc] initWithTitle:@""];
    for (const auto& entry : entries) {
      if (entry.separator) continue;
      if (entry.submenu) {
        NSMenu* children = rowCommandsMenu(entry.children, tab, place + QLatin1Char('/') + entry.title, pin);
        if (children.numberOfItems == 0) continue;
        NSMenuItem* item = [[NSMenuItem alloc] initWithTitle:ns(entry.title) action:nil keyEquivalent:@""];
        item.submenu = children;
        [menu addItem:item];
        continue;
      }
      const NativeMacAction* command = action(entry.action);
      if (!command || command->group.isEmpty()) continue;
      NSMenuItem* item = rowCommand(*command, tab, place, pin, command->label);
      if (pin) {
        const bool pinned = command->pinnedToToolbar;
        item.state = pinned ? NSControlStateValueOn : NSControlStateValueOff;
        item.toolTip = pinned ? ns(QObject::tr("Unpin from Toolbar")) : ns(QObject::tr("Pin to Toolbar"));
      }
      item.image = actionImage(*command);
      if (@available(macOS 27.0, *)) item.preferredImageVisibility = NSMenuItemImageVisibilityVisible;
      [menu addItem:item];
    }
    return menu;
  }

  // Qt's group menu rows each have a ⋮ button with Pin to Toolbar (Unpin
  // from Toolbar) and Change Keyboard Shortcut… (mitcad#1001). An NSMenu
  // row has no button or right-click of its own, so a group menu ends with
  // the two as submenus listing its commands (mitcad#1074).
  void addRowCommands(NSMenu* menu, const QList<NativeMacMenuItem>& entries, const QString& tab, const QString& place) {
    NSMenu* pins = rowCommandsMenu(entries, tab, place, true);
    if (pins.numberOfItems == 0) return;
    NSMenuItem* separator = NSMenuItem.separatorItem;
    separator.tag = rowCommandsTag;
    [menu addItem:separator];
    NSMenuItem* pin = [[NSMenuItem alloc] initWithTitle:ns(QObject::tr("Pin to Toolbar")) action:nil keyEquivalent:@""];
    pin.tag = rowCommandsTag;
    pin.submenu = pins;
    [menu addItem:pin];
    NSMenuItem* shortcut = [[NSMenuItem alloc] initWithTitle:ns(QObject::tr("Change Keyboard Shortcut"))
                                                      action:nil keyEquivalent:@""];
    shortcut.tag = rowCommandsTag;
    shortcut.submenu = rowCommandsMenu(entries, tab, place, false);
    [menu addItem:shortcut];
  }

  CGFloat buttonRowWidth(const NativeMacRibbonGroup& group, int count) const {
    CGFloat width = 0;
    for (int i = 0; i < count; ++i) width += group.buttons[i].width;
    return width;
  }

  CGFloat groupWidth(const NativeMacRibbonGroup& group, int count, CGFloat caption) const {
      const int shared = count > 0 ? group.fullWidth : group.collapsedWidth;
      return shared > 0 ? shared : std::max(caption + 16, buttonRowWidth(group, count) + 16);
  }

  void buildRibbon() {
    ribbonGroups = [[MITCADInspectorDocumentView alloc] initWithFrame:NSZeroRect];
    ribbonButtons = [NSMutableDictionary dictionary];
    ribbonMenus = [NSMutableDictionary dictionary];
    ribbonSeparators = [NSMutableDictionary dictionary];
    ribbonBackgrounds = [NSMutableDictionary dictionary];
    ribbonSplits = [NSMutableDictionary dictionary];
    ribbonGroupOrder.clear();
    ribbonGroupButtons.clear();
    for (const auto& group : ribbonPresentation) {
      if (group.workspace != workspace()) continue;
      ribbonGroupOrder.append(group.name);
      NSBox* background = [[NSBox alloc] initWithFrame:NSZeroRect];
      background.boxType = NSBoxCustom;
      background.borderWidth = 0.5;
      background.cornerRadius = 12;
      background.fillColor = NSColor.controlBackgroundColor;
      background.borderColor = NSColor.separatorColor;
      background.titlePosition = NSNoTitle;
      [ribbonGroups addSubview:background];
      ribbonBackgrounds[ns(group.name)] = background;
      NSPopUpButton* caption = [[NSPopUpButton alloc] initWithFrame:NSZeroRect pullsDown:YES];
      caption.bordered = NO;
      caption.font = [NSFont systemFontOfSize:11];
      caption.accessibilityLabel = ns(group.title);
      caption.accessibilityIdentifier = ns(QStringLiteral("native.ribbon.group.") + group.name.toLower());
      NSMenu* menu = presentationMenu(group.menu);
      [menu insertItem:[[NSMenuItem alloc] initWithTitle:ns(group.title) action:nil keyEquivalent:@""] atIndex:0];
      const QString place = group.workspace + QLatin1Char('/') + group.name;
      addRowCommands(menu, group.menu, group.workspace, place);
      caption.menu = menu;
      [ribbonGroups addSubview:caption];
      ribbonMenus[ns(group.name)] = caption;
      for (const auto& shown : group.buttons) {
        const auto* item = action(shown.action);
        if (!item) continue;
        MITCADCommandButton* button = [[MITCADCommandButton alloc] initWithFrame:NSZeroRect];
        button.bordered = NO;
        [button setButtonType:NSButtonTypePushOnPushOff];
        button.image = actionImage(*item, shown.iconSize);
        button.imagePosition = NSImageOnly;
        button.imageScaling = NSImageScaleProportionallyDown;
        button.representedObject = ns(item->id);
        button.target = delegate;
        button.action = @selector(performAction:);
        button.accessibilityIdentifier = ns(QStringLiteral("native.ribbon.") + item->id);
        button.accessibilityLabel = ns(item->label);
        button.toolTip = ns(item->tooltip.isEmpty() ? item->label : item->tooltip);
        button.enabled = item->enabled;
        button.state = item->checked ? NSControlStateValueOn : NSControlStateValueOff;
        [ribbonGroups addSubview:button];
        ribbonButtons[ns(item->id)] = button;
        ribbonGroupButtons[group.name].append(item->id);
        if (shown.pinned) {
          // A command the user pinned in its group (Qt's ribbonPinned_<id>,
          // mitcad#1084) and its right-click: Unpin from Toolbar, Change
          // Keyboard Shortcut….
          button.accessibilityIdentifier = ns(QStringLiteral("native.ribbon.pinned.") + item->id);
          NSMenu* context = [[NSMenu alloc] initWithTitle:@""];
          [context addItem:rowCommand(*item, group.workspace, place, true, QObject::tr("Unpin from Toolbar"))];
          [context addItem:rowCommand(*item, group.workspace, place, false, QObject::tr("Change Keyboard Shortcut…"))];
          button.menu = context;
          continue;
        }
          if (!shown.menuOnly && !shown.menu.isEmpty()) {
          NSPopUpButton* arrow = [[NSPopUpButton alloc] initWithFrame:NSZeroRect pullsDown:YES];
          arrow.bordered = NO;
          NSMenu* submenu = presentationMenu(shown.menu);
          [submenu insertItem:[[NSMenuItem alloc] initWithTitle:@"" action:nil keyEquivalent:@""] atIndex:0];
          // The subgroup's place, as its submenu in the group menu has it.
          QString subgroup = item->label;
          for (const auto& entry : group.menu) {
            for (const auto& child : entry.children) {
              if (entry.submenu && child.action == item->id) subgroup = entry.title;
            }
          }
          addRowCommands(submenu, shown.menu, group.workspace, place + QLatin1Char('/') + subgroup);
          arrow.menu = submenu;
          arrow.accessibilityIdentifier = ns(QStringLiteral("native.ribbon.arrow.") + item->id);
          arrow.accessibilityLabel = ns(item->label + QStringLiteral(" options"));
          [ribbonGroups addSubview:arrow];
          ribbonSplits[ns(item->id)] = arrow;
        }
      }
    }
    ribbonScroll.documentView = ribbonGroups;
    layoutOverlays();
  }

  void layoutRibbon(CGFloat availableWidth) {
    if (!ribbonGroups) return;
    QList<int> counts;
    QList<CGFloat> captions;
    for (const auto& name : ribbonGroupOrder) {
      const auto* group = ribbonGroup(name);
      counts.append(group ? group->buttons.size() : 0);
      captions.append(ribbonMenus[ns(name)].intrinsicContentSize.width);
    }
    const auto totalWidth = [&] {
      CGFloat width = 2 * ribbonInset + std::max(0, int(counts.size()) - 1) * ribbonGroupGap;
      for (int i = 0; i < counts.size(); ++i) width += groupWidth(*ribbonGroup(ribbonGroupOrder[i]), counts[i], captions[i]);
      return width;
    };
    // Match Qt's right-to-left collapse of whole button rows. Every caption
    // remains a native menu and Finish Sketch remains visible.
    for (int i = int(counts.size()) - 1; i >= 0 && totalWidth() > availableWidth; --i) {
      const auto* group = ribbonGroup(ribbonGroupOrder[i]);
      if (groupWidth(*group, counts[i], captions[i]) > groupWidth(*group, 0, captions[i])) counts[i] = 0;
    }
    CGFloat x = ribbonInset;
    for (int i = 0; i < ribbonGroupOrder.size(); ++i) {
      const auto* group = ribbonGroup(ribbonGroupOrder[i]);
      const CGFloat width = groupWidth(*group, counts[i], captions[i]);
      ribbonBackgrounds[ns(group->name)].frame = NSMakeRect(x, 4, width, 67);
      ribbonMenus[ns(group->name)].frame = NSMakeRect(x + (width - captions[i]) / 2, 43, captions[i], 22);
      CGFloat buttonX = x + (width - buttonRowWidth(*group, counts[i])) / 2;
      for (int j = 0; j < group->buttons.size(); ++j) {
        const auto& shown = group->buttons[j];
        MITCADCommandButton* button = ribbonButtons[ns(shown.action)];
        NSPopUpButton* arrow = shown.pinned ? nil : ribbonSplits[ns(shown.action)];
        button.hidden = j >= counts[i];
        if (arrow) arrow.hidden = button.hidden;
        const CGFloat arrowWidth = arrow ? 14 : 0;
        button.frame = NSMakeRect(buttonX, 9, shown.width - arrowWidth, ribbonButtonHeight);
        if (arrow) arrow.frame = NSMakeRect(buttonX + shown.width - arrowWidth, 10, arrowWidth, 32);
        buttonX += shown.width;
      }
      x += width + ribbonGroupGap;
    }
    ribbonGroups.frame = NSMakeRect(0, 0, std::max(availableWidth, x - ribbonGroupGap + ribbonInset), 76);
  }

  void updateToolbar(bool structureChanged = false) {
    for (const auto& command : actions) {
      const auto update = [this, &command](MITCADCommandButton* button) {
        if (!button) return;
        if (button.enabled != command.enabled) button.enabled = command.enabled;
        const NSControlStateValue state = command.checked ? NSControlStateValueOn : NSControlStateValueOff;
        if (button.state != state) button.state = state;
        NSImage* image = actionImage(command, 26);
        if (button.image != image) button.image = image;
        if (qt(button.accessibilityLabel) != command.label) button.accessibilityLabel = ns(command.label);
        const QString tooltip = command.tooltip.isEmpty() ? command.label : command.tooltip;
        if (qt(button.toolTip) != tooltip) button.toolTip = ns(tooltip);
      };
      update(ribbonButtons[ns(command.id)]);
    }
    // Menu validation reads the latest command snapshot. Replacing popup
    // menus while a user is tracking one would interrupt that interaction.
    for (NSToolbarItem* item in window.toolbar.items) {
      const QString identifier = qt(item.itemIdentifier);
      if (identifier.startsWith(QStringLiteral("command."))) {
        if (const auto* command = action(identifier.mid(8))) {
          if (item.enabled != command->enabled) item.enabled = command->enabled;
          if (qt(item.label) != command->label) item.label = ns(command->label);
          const QString tooltip = command->tooltip.isEmpty() ? command->label : command->tooltip;
          if (qt(item.toolTip) != tooltip) item.toolTip = ns(tooltip);
          NSImage* image = actionImage(*command);
          if (item.image != image) item.image = image;
        }
      } else if (structureChanged && [item isKindOfClass:NSMenuToolbarItem.class]) {
        NSString* group = [item.itemIdentifier isEqualToString:modifyID] ? @"MODIFY" :
                          ([item.itemIdentifier isEqualToString:constraintsID] ? @"CONSTRAINTS" : @"CREATE");
        ((NSMenuToolbarItem*)item).menu = groupMenu(qt(group));
      }
    }
  }

  void rebuildTree(const QList<NativeMacTreeItem>& items) {
    QStringList selected;
    for (NSInteger row = 0; row < tree.numberOfRows; ++row) {
      MITCADTreeNode* node = [tree itemAtRow:row];
      if ([tree.selectedRowIndexes containsIndex:NSUInteger(row)]) selected.append(qt(node.key));
    }
    treeItems = items;
    treeIndex.clear();
    treeRoots = [NSMutableArray array];
    treeNodes = [NSMutableDictionary dictionary];
    for (int index = 0; index < items.size(); ++index) {
      const auto& item = items[index];
      if (item.id.isEmpty() || treeIndex.contains(item.id)) {
        qWarning().noquote() << "Native Mac model tree: duplicate/empty key" << item.id;
        continue;
      }
      treeIndex.insert(item.id, index);
      MITCADTreeNode* node = [[MITCADTreeNode alloc] init];
      node.key = ns(item.id);
      node.children = [NSMutableArray array];
      treeNodes[node.key] = node;
    }
    for (const auto& item : items) {
      MITCADTreeNode* node = treeNodes[ns(item.id)];
      if (node == nil) continue;
      MITCADTreeNode* parent = treeNodes[ns(item.parentId)];
      // Defend a presentation error: cycles must never reach NSOutlineView.
      QSet<QString> visited{item.id};
      QString ancestor = item.parentId;
      bool cycle = false;
      while (!ancestor.isEmpty() && treeIndex.contains(ancestor)) {
        if (visited.contains(ancestor)) { cycle = true; break; }
        visited.insert(ancestor);
        ancestor = items[treeIndex.value(ancestor)].parentId;
      }
      if (parent && !cycle) [parent.children addObject:node];
      else [treeRoots addObject:node];
    }
    updatingSelection = true;
    [tree reloadData];
    for (const auto& item : items) {
      if (!treeNodes[ns(item.id)]) continue;
      if (item.expanded) [tree expandItem:treeNodes[ns(item.id)]];
      else [tree collapseItem:treeNodes[ns(item.id)]];
    }
    setSelected(selected);
    updatingSelection = false;
  }

  void setSelected(const QStringList& keys) {
    const bool previous = updatingSelection;
    updatingSelection = true;
    NSMutableIndexSet* rows = [NSMutableIndexSet indexSet];
    for (const QString& key : keys) {
      MITCADTreeNode* node = treeNodes[ns(key)];
      if (node == nil) continue;
      NSInteger row = [tree rowForItem:node];
      if (row >= 0) [rows addIndex:NSUInteger(row)];
    }
    [tree selectRowIndexes:rows byExtendingSelection:NO];
    updatingSelection = previous;
  }

  MITCADTreeNode* contextNode() {
    const NSInteger row = tree.clickedRow >= 0 ? tree.clickedRow : tree.selectedRow;
    return row >= 0 ? [tree itemAtRow:row] : nil;
  }

  void installInspector(NSView* content, NSView* footer = nil) {
    if (inspectorContent == content && inspectorFooterContent == footer) return;
    // A removed control must not keep the keyboard or VoiceOver's focus
    // (mitcad#1026): AppKit would leave the window itself first responder
    // and VoiceOver on a dead element. The canvas takes it, as when a card
    // with the focus hides, and VoiceOver is told. So it does when the
    // window itself still has it from an earlier removal: no live element.
    const bool removed = inspectorContent != nil && inspectorContent != content;
    const bool refocus = (removed && (focusIsIn(inspectorContent) || window.firstResponder == window)) ||
                         (inspectorFooterContent != footer && focusIsIn(inspectorFooterContent));
    NSView* canvas = refocus && viewport ? (__bridge NSView*)reinterpret_cast<void*>(viewport->winId()) : nil;
    if (canvas != nil && canvas.window == window) [window makeFirstResponder:canvas];
    [inspectorFooterContent removeFromSuperview];
    inspectorFooterContent = footer;
    if (footer) fill(footer, inspectorFooter);
    inspectorScroll.documentView = nil;
    [inspectorContent removeFromSuperview];
    inspectorContent = content;
    if ([inspectorDocument isKindOfClass:MITCADInspectorContentView.class])
      ((MITCADInspectorContentView*)inspectorDocument)->state = nullptr;
    inspectorDocument = nil;
    if (content) {
      // A flipped document starts long command forms at their heading and
      // first input, rather than at the bottom of AppKit's scroll range.
      MITCADInspectorContentView* document = [[MITCADInspectorContentView alloc] initWithFrame:NSZeroRect];
      document->state = this;
      inspectorDocument = document;
      inspectorDocument.translatesAutoresizingMaskIntoConstraints = NO;
      fill(content, inspectorDocument);
      inspectorScroll.documentView = inspectorDocument;
      [NSLayoutConstraint activateConstraints:@[
        [inspectorDocument.leadingAnchor constraintEqualToAnchor:inspectorScroll.contentView.leadingAnchor],
        [inspectorDocument.topAnchor constraintEqualToAnchor:inspectorScroll.contentView.topAnchor],
        [inspectorDocument.widthAnchor constraintEqualToAnchor:inspectorScroll.contentView.widthAnchor]
      ]];
      [inspectorHost layoutSubtreeIfNeeded];
      [inspectorScroll.contentView scrollToPoint:NSZeroPoint];
      [inspectorScroll reflectScrolledClipView:inspectorScroll.contentView];
    }
    layoutOverlays();
    if (refocus) {
      NSResponder* focused = window.firstResponder;
      id element = [focused isKindOfClass:NSView.class] ? ((NSView*)focused).accessibilityFocusedUIElement : nil;
      NSAccessibilityPostNotification(element != nil ? element : window, NSAccessibilityFocusedUIElementChangedNotification);
    }
  }

  // The keyboard is in `view`: its first responder, or the control whose
  // field editor that is.
  bool focusIsIn(NSView* view) const {
    if (view == nil) return false;
    NSResponder* responder = window.firstResponder;
    if ([responder isKindOfClass:NSTextView.class] && ((NSTextView*)responder).fieldEditor) {
      id control = ((NSTextView*)responder).delegate;
      if ([control isKindOfClass:NSView.class]) responder = (NSView*)control;
    }
    return [responder isKindOfClass:NSView.class] && [(NSView*)responder isDescendantOf:view];
  }

  void buildSearch() {
    if (searchPopover) return;
    searchPopover = [[NSPopover alloc] init];
    searchPopover.behavior = NSPopoverBehaviorTransient;
    searchPopover.contentSize = NSMakeSize(420, 350);
    searchPopover.animates = !qEnvironmentVariableIsSet("MITCAD_TEST_INPUT");
    searchPopover.delegate = delegate;
    NSViewController* controller = [[NSViewController alloc] init];
    NSView* view = [[NSView alloc] initWithFrame:NSMakeRect(0, 0, 420, 350)];
    controller.view = view;
    searchPopover.contentViewController = controller;
    searchField = [[NSSearchField alloc] initWithFrame:NSZeroRect];
    searchField.translatesAutoresizingMaskIntoConstraints = NO;
    searchField.placeholderString = ns(QObject::tr("Search commands"));
    searchField.delegate = delegate;
    searchField.target = delegate;
    searchField.action = @selector(searchChanged:);
    searchField.sendsSearchStringImmediately = YES;
    searchField.accessibilityIdentifier = @"native.commandSearch";
    searchField.identifier = @"native.commandSearch";
    [view addSubview:searchField];
    searchTable = [[NSTableView alloc] initWithFrame:NSZeroRect];
    NSTableColumn* column = [[NSTableColumn alloc] initWithIdentifier:@"command"];
    [searchTable addTableColumn:column];
    searchTable.headerView = nil;
    searchTable.rowHeight = 30;
    searchTable.style = NSTableViewStyleFullWidth;
    searchTable.dataSource = delegate;
    searchTable.delegate = delegate;
    searchTable.target = delegate;
    searchTable.doubleAction = @selector(chooseSearch:);
    searchTable.action = @selector(chooseSearch:);
    searchTable.accessibilityLabel = ns(QObject::tr("Commands"));
    NSScrollView* results = scroll(searchTable);
    [view addSubview:results];
    timelineSearchCount = label(@"");
    NSButton* filters = [NSButton buttonWithTitle:ns(QObject::tr("Filters…")) target:delegate action:@selector(openTimelineSearchFilter:)];
    NSButton* previous = [NSButton buttonWithTitle:ns(QObject::tr("Previous")) target:delegate action:@selector(stepTimelineSearch:)];
    NSButton* next = [NSButton buttonWithTitle:ns(QObject::tr("Next")) target:delegate action:@selector(stepTimelineSearch:)];
    previous.tag = -1;
    next.tag = 1;
    timelineSearchControls = [NSStackView stackViewWithViews:@[filters, previous, next, timelineSearchCount]];
    timelineSearchControls.translatesAutoresizingMaskIntoConstraints = NO;
    timelineSearchControls.orientation = NSUserInterfaceLayoutOrientationHorizontal;
    timelineSearchControls.spacing = 8;
    timelineSearchControls.hidden = YES;
    [view addSubview:timelineSearchControls];
    [NSLayoutConstraint activateConstraints:@[
      [searchField.leadingAnchor constraintEqualToAnchor:view.leadingAnchor constant:12],
      [searchField.trailingAnchor constraintEqualToAnchor:view.trailingAnchor constant:-12],
      [results.leadingAnchor constraintEqualToAnchor:view.leadingAnchor constant:6],
      [results.trailingAnchor constraintEqualToAnchor:view.trailingAnchor constant:-6],
      [timelineSearchControls.leadingAnchor constraintEqualToAnchor:view.leadingAnchor constant:12],
      [timelineSearchControls.trailingAnchor constraintEqualToAnchor:view.trailingAnchor constant:-12]
    ]];
    // Command search hangs below its control with the field on top. Timeline
    // search rises from the timeline's magnifier, so its field is at the
    // bottom next to the timeline, with the step controls and results above
    // it, as the default frontend's field sits in the timeline bar (mitcad#1021).
    commandSearchLayout = @[
      [searchField.topAnchor constraintEqualToAnchor:view.topAnchor constant:12],
      [results.topAnchor constraintEqualToAnchor:searchField.bottomAnchor constant:8],
      [results.bottomAnchor constraintEqualToAnchor:view.bottomAnchor constant:-42],
      [timelineSearchControls.bottomAnchor constraintEqualToAnchor:view.bottomAnchor constant:-8]
    ];
    timelineSearchLayout = @[
      [results.topAnchor constraintEqualToAnchor:view.topAnchor constant:6],
      [results.bottomAnchor constraintEqualToAnchor:timelineSearchControls.topAnchor constant:-8],
      [timelineSearchControls.bottomAnchor constraintEqualToAnchor:searchField.topAnchor constant:-8],
      [searchField.bottomAnchor constraintEqualToAnchor:view.bottomAnchor constant:-12]
    ];
    [NSLayoutConstraint activateConstraints:commandSearchLayout];
  }

  // The visible control a search was opened from, or nil for a shortcut, a
  // menu item or an image-only toolbar item, which have no view of their own.
  static NSView* searchAnchor(id sender) {
    NSView* view = nil;
    if ([sender isKindOfClass:NSView.class]) view = (NSView*)sender;
    else if ([sender isKindOfClass:NSToolbarItem.class]) view = ((NSToolbarItem*)sender).view;
    if (view.window == nil || !view.window.visible || [view isHiddenOrHasHiddenAncestor]) return nil;
    return view;
  }

  void updateMenuState(NSMenu* menu) {
    bool commandItems = false;
    for (NSMenuItem* item in menu.itemArray) {
      if (item.target == delegate && item.action == @selector(performAction:)) commandItems = true;
      if (item.submenu) updateMenuState(item.submenu);
    }
    if (commandItems) [menu update];
  }

  void setActions(const QList<NativeMacAction>& snapshot) {
    // Availability changes do not recreate the menu bar while a user has a
    // menu or an editable control open. Validation reads the latest snapshot.
    bool structureChanged = snapshot.size() != actions.size() || imagePaletteKey != QApplication::palette().cacheKey();
    bool menuStateChanged = false;
    if (!structureChanged) {
      for (int index = 0; index < snapshot.size(); ++index) {
        const auto& a = snapshot[index]; const auto& b = actions[index];
        if (a.id != b.id || a.label != b.label || a.menu != b.menu || a.group != b.group ||
            a.workspace != b.workspace || a.shortcut != b.shortcut || a.shortcuts != b.shortcuts || a.submenu != b.submenu ||
            a.separator != b.separator || a.toolbarPinned != b.toolbarPinned || a.toolbarOnly != b.toolbarOnly || a.toolbarOrder != b.toolbarOrder ||
            a.section != b.section) {
          structureChanged = true;
          break;
        }
        if (a.enabled != b.enabled || a.checked != b.checked || a.tooltip != b.tooltip) menuStateChanged = true;
      }
    }
    actions = snapshot;
    if (structureChanged) {
      actionIndex.clear();
      for (int index = 0; index < snapshot.size(); ++index) actionIndex.insert(snapshot[index].id, index);
      actionShortcuts.clear();
      for (const auto& item : snapshot) {
        QList<QKeySequence> parsed;
        for (const QString& shortcut : item.shortcuts) parsed.append(QKeySequence(shortcut, QKeySequence::PortableText));
        actionShortcuts.append(parsed);
      }
    }
    if (structureChanged) rebuildMenus();
    if (!window.toolbar) installToolbar();
    else if (structureChanged) buildRibbon();
    updateToolbar(structureChanged);
    // Async model changes can enable a key equivalent before AppKit next
    // validates its menu. Refresh retained menus without replacing them.
    if (menuStateChanged || structureChanged) {
      updateMenuState(mainMenu);
      for (NSPopUpButton* popup in ribbonMenus.allValues) updateMenuState(popup.menu);
      for (NSToolbarItem* item in window.toolbar.items) {
        if ([item isKindOfClass:NSMenuToolbarItem.class]) updateMenuState(((NSMenuToolbarItem*)item).menu);
      }
    }
    // The navigation bar's checks after the View menu's (mitcad#1099).
    updateNavigationBar();
    if (historyMarkerDragging && nativeModalBlocked()) finishHistoryMarkerDrag();
    updateHistoryAppearance();
    if (searchPopover.shown) fillSearch();
  }

  void fillSearch() {
    if (timelineSearch) {
      if (callbacks.timelineSearchChanged) callbacks.timelineSearchChanged(qt(searchField.stringValue));
      [searchTable reloadData];
      return;
    }
    searchResults.clear();
    const QString query = qt(searchField.stringValue).trimmed().toCaseFolded();
    for (int index = 0; index < actions.size(); ++index) {
      if (!actions[index].separator && actions[index].submenu.isEmpty() &&
          searchScore(actions[index], query) >= 0) searchResults.append(index);
    }
    std::stable_sort(searchResults.begin(), searchResults.end(), [&](int left, int right) {
      const auto& a = actions[left];
      const auto& b = actions[right];
      if (a.enabled != b.enabled) return a.enabled;
      const int rankA = searchScore(a, query), rankB = searchScore(b, query);
      return rankA == rankB ? a.label.localeAwareCompare(b.label) < 0 : rankA < rankB;
    });
    [searchTable reloadData];
    if (!searchResults.isEmpty() && actions[searchResults.front()].enabled) {
      [searchTable selectRowIndexes:[NSIndexSet indexSetWithIndex:0] byExtendingSelection:NO];
    }
  }

  // Timeline search opens above the timeline's magnifier, also from Command+F
  // and the menu; command search under the control that opened it, such as
  // the Search Commands toolbar item, and at the top centre of the window
  // from a shortcut or a menu (mitcad#1021).
  void openSearch(bool timeline = false, id sender = nil) {
    if (nativeModalBlocked()) return;
    buildSearch();
    if (searchPopover.shown && timelineSearch && !timeline && callbacks.timelineSearchClosed) callbacks.timelineSearchClosed();
    const bool reopening = searchPopover.shown && timelineSearch == timeline;
    timelineSearch = timeline;
    timelineSearchControls.hidden = !timeline;
    searchField.placeholderString = timeline ? ns(QObject::tr("Search timeline: name, type, in:component, is:status")) : ns(QObject::tr("Search commands"));
    searchField.accessibilityIdentifier = timeline ? @"native.timelineSearch" : @"native.commandSearch";
    searchField.identifier = searchField.accessibilityIdentifier;
    searchTable.accessibilityLabel = timeline ? ns(QObject::tr("Timeline search results")) : ns(QObject::tr("Commands"));
    if (timeline && callbacks.timelineSearchOpened) callbacks.timelineSearchOpened();
    if (timeline) setCardVisible(historyCard, true);
    if (!reopening) searchField.stringValue = @"";
    [NSLayoutConstraint deactivateConstraints:timeline ? commandSearchLayout : timelineSearchLayout];
    [NSLayoutConstraint activateConstraints:timeline ? timelineSearchLayout : commandSearchLayout];
    fillSearch();
    if (timeline) [window.contentView layoutSubtreeIfNeeded];
    // Popover edges follow the anchor's coordinates: in a flipped view the
    // minimum Y edge is the top one. Timeline search opens above its anchor,
    // command search below.
    NSView* anchor = searchAnchor(timeline ? (id)historySearchButton : sender);
    if (anchor != nil) {
      [searchPopover showRelativeToRect:anchor.bounds ofView:anchor
                          preferredEdge:timeline == bool(anchor.flipped) ? NSRectEdgeMinY : NSRectEdgeMaxY];
    } else {
      [searchPopover showRelativeToRect:NSMakeRect(window.contentView.bounds.size.width / 2 - 1, 1, 2, 2)
                                ofView:window.contentView preferredEdge:NSRectEdgeMaxY];
    }
    searchField.window.identifier = timeline ? @"native.timelineSearchWindow" : @"native.commandSearchWindow";
    [searchField.window makeKeyWindow];
    [searchField.window makeFirstResponder:searchField];
    [searchField selectText:nil];
    if (!searchPopover.shown || searchField.window == nil)
      qWarning() << "Native command search could not attach to its document window";
  }

  // The width the Chat card and its gap take from the right of the canvas
  // (mitcad#1075); none while it is hidden.
  CGFloat chatSpace() const {
    constexpr CGFloat margin = 12;
    constexpr CGFloat gap = 8;
    return chatCard.hidden ? 0 : chatCardPlacedWidth(canvasRoot.bounds.size.width - margin * 2) + gap;
  }

  // The toolbar's Chat button: the command's CAD artwork, its tooltip, on
  // while the card shows.
  void applyChatButton(NSButton* button) {
    const NativeMacAction* chat = action(QStringLiteral("view.chat"));
    NSImage* artwork = chat != nullptr && !chat->icon.isNull() ? actionImage(*chat, 18) : nil;
    button.image = artwork != nil ? artwork : symbol(QStringLiteral("bubble.left.and.bubble.right"), ns(QObject::tr("Chat")));
    const QString tip = chat != nullptr && !chat->tooltip.isEmpty() ? chat->tooltip : QObject::tr("Chat");
    if (qt(button.toolTip) != tip) button.toolTip = ns(tip);
    button.accessibilityLabel = ns(QObject::tr("Chat"));
    const NSControlStateValue value = chatCard != nil && !chatCard.hidden ? NSControlStateValueOn : NSControlStateValueOff;
    if (button.state != value) button.state = value;
  }

  void applyChatButtons() {
    for (NSToolbarItem* item in window.toolbar.items) {
      if (![item.itemIdentifier isEqualToString:agentID]) continue;
      for (NSView* child in item.view.subviews) {
        if ([child.accessibilityIdentifier isEqualToString:@"native.chatButton"]) applyChatButton((NSButton*)child);
      }
    }
  }

  // The quick-access group (mitcad#1113), as the Qt QuickAccessGroup: a
  // button per shown command, a hairline and the ⋮ (DotsButton's three
  // dots), in the workspace item after its tabs.
  static constexpr CGFloat quickButtonWidth = 28;
  static constexpr CGFloat quickSpacing = 2;
  static constexpr CGFloat quickDotsWidth = 20;

  NSImage* quickDotsImage() const {
    NSImage* image = [NSImage imageWithSize:NSMakeSize(12, 18) flipped:NO drawingHandler:^BOOL(NSRect rect) {
      [NSColor.blackColor setFill];
      for (int dot = -1; dot <= 1; ++dot) {
        [[NSBezierPath bezierPathWithOvalInRect:NSMakeRect(NSMidX(rect) - 1.4, NSMidY(rect) + dot * 4.5 - 1.4, 2.8, 2.8)] fill];
      }
      return YES;
    }];
    [image setTemplate:YES]; // (template is a C++ keyword)
    image.accessibilityDescription = ns(QObject::tr("Choose the commands of this group"));
    return image;
  }

  NSStackView* makeQuickGroup() {
    NSStackView* group = [NSStackView stackViewWithViews:@[]];
    group.orientation = NSUserInterfaceLayoutOrientationHorizontal;
    group.alignment = NSLayoutAttributeCenterY;
    group.spacing = quickSpacing;
    group.accessibilityIdentifier = @"native.quickAccess";
    return group;
  }

  // The group's buttons made again for the shown commands.
  void rebuildQuickGroup() {
    if (!quickGroup) return;
    for (NSView* view in [quickGroup.arrangedSubviews copy]) {
      [quickGroup removeArrangedSubview:view];
      [view removeFromSuperview];
    }
    for (const QString& id : std::as_const(quickAccess.shown)) {
      MITCADCommandButton* button = [[MITCADCommandButton alloc] initWithFrame:NSZeroRect];
      button.representedObject = ns(id);
      button.target = delegate;
      button.action = @selector(performAction:);
      button.bezelStyle = NSBezelStyleRecessed;
      button.showsBorderOnlyWhileMouseInside = YES;
      button.imagePosition = NSImageOnly;
      button.translatesAutoresizingMaskIntoConstraints = NO;
      button.accessibilityIdentifier = ns(QStringLiteral("native.quickAccess.") + id);
      [button.widthAnchor constraintEqualToConstant:quickButtonWidth].active = YES;
      [button.heightAnchor constraintEqualToConstant:26].active = YES;
      [quickGroup addArrangedSubview:button];
    }
    NSBox* hairline = [[NSBox alloc] initWithFrame:NSMakeRect(0, 0, 1, 16)];
    hairline.boxType = NSBoxSeparator;
    hairline.translatesAutoresizingMaskIntoConstraints = NO;
    hairline.accessibilityIdentifier = @"native.quickAccessHairline";
    [hairline.widthAnchor constraintEqualToConstant:1].active = YES;
    [hairline.heightAnchor constraintEqualToConstant:16].active = YES;
    [quickGroup addArrangedSubview:hairline];
    NSButton* dots = [NSButton buttonWithImage:quickDotsImage() target:delegate action:@selector(openQuickAccessMenu:)];
    dots.bezelStyle = NSBezelStyleRecessed;
    dots.showsBorderOnlyWhileMouseInside = YES;
    dots.translatesAutoresizingMaskIntoConstraints = NO;
    dots.toolTip = ns(QObject::tr("Choose the commands of this group"));
    dots.accessibilityIdentifier = @"native.quickAccessMenu";
    [dots.widthAnchor constraintEqualToConstant:quickDotsWidth].active = YES;
    [dots.heightAnchor constraintEqualToConstant:26].active = YES;
    [quickGroup addArrangedSubview:dots];
    applyQuickFolded();
    // Hidden whole by the preference (mitcad#1189); the tabs' stack view
    // detaches a hidden view, so the item closes up.
    quickGroup.hidden = !quickAccess.visible;
  }

  // Each button's artwork and state from the latest actions, its tooltip;
  // a change logged with the tooltip.
  void applyQuickState() {
    if (!quickGroup) return;
    for (NSView* view in quickGroup.arrangedSubviews) {
      if (![view isKindOfClass:MITCADCommandButton.class]) continue;
      MITCADCommandButton* button = (MITCADCommandButton*)view;
      const QString id = qt(button.representedObject);
      const NativeMacAction* command = action(id);
      const int index = int(quickAccess.shown.indexOf(id));
      const bool enabled = command != nullptr && command->enabled;
      if (button.enabled != enabled) button.enabled = enabled;
      NSImage* image = command != nullptr ? actionImage(*command, 18) : nil;
      if (button.image != image) button.image = image;
      const QString tooltip = quickAccess.tooltips.value(index);
      if (qt(button.toolTip) != tooltip) button.toolTip = ns(tooltip);
      button.accessibilityLabel = ns(command != nullptr ? command->label : id);
      const QString logged = QStringLiteral("%1: %2").arg(enabled ? QStringLiteral("enabled") : QStringLiteral("disabled"), tooltip);
      if (quickLogged.value(id) != logged) {
        quickLogged.insert(id, logged);
        qDebug().noquote() << QStringLiteral("Native quick access %1 %2").arg(id, logged);
      }
    }
  }

  void applyQuickFolded() {
    if (!quickGroup) return;
    for (NSView* view in quickGroup.arrangedSubviews) {
      if (![view.accessibilityIdentifier isEqualToString:@"native.quickAccessMenu"]) {
        view.hidden = quickFolded || ([view isKindOfClass:NSBox.class] && quickAccess.shown.isEmpty());
      }
    }
  }

  // The group's width with every button shown.
  CGFloat quickUnfoldedWidth() const {
    const CGFloat buttons = quickAccess.shown.isEmpty() ? 0 : CGFloat(quickAccess.shown.size()) * (quickButtonWidth + quickSpacing) + 1 + quickSpacing;
    return buttons + quickDotsWidth;
  }

  void scheduleQuickFold() {
    // On the next run-loop turn, outside any layout pass, as
    // queueOverlayLayout.
    if (quickFoldScheduled || closing) return;
    quickFoldScheduled = true;
    __weak MITCADNativeWindowDelegate* owner = delegate;
    dispatch_async(dispatch_get_main_queue(), ^{
      MITCADNativeWindowDelegate* alive = owner;
      if (alive == nil || alive->state == nullptr) return;
      alive->state->quickFoldScheduled = false;
      alive->state->updateQuickFold();
    });
  }

  // As TitleBar::updateQuickFold: folded when the document's name, from
  // the unfolded group's right to the project, agent and search items, 16
  // points off each, would have less room than it needs.
  void updateQuickFold() {
    if (closing || !quickGroup || !workspaceTabs || !documentTitle || quickGroup.window == nil) return;
    // A hidden group (mitcad#1189) takes no room: nothing to fold.
    if (!quickAccess.visible) return;
    [window.contentView.superview layoutSubtreeIfNeeded];
    const CGFloat tabsRight = NSMaxX([workspaceTabs convertRect:workspaceTabs.bounds toView:nil]);
    CGFloat right = window.frame.size.width;
    for (NSToolbarItem* item in window.toolbar.items) {
      NSView* view = item.view;
      if (view.window == nil || view.hiddenOrHasHiddenAncestor) continue;
      if ([item.itemIdentifier isEqualToString:projectID] || [item.itemIdentifier isEqualToString:agentID] ||
          [item.itemIdentifier isEqualToString:searchID]) {
        right = std::min(right, NSMinX([view convertRect:view.bounds toView:nil]));
      }
    }
    const CGFloat freeLeft = tabsRight + 8 + quickUnfoldedWidth() + 16;
    const CGFloat freeRight = right - 16;
    const CGFloat title = std::min<CGFloat>(220, std::ceil(documentTitle.attributedStringValue.size.width));
    const bool folded = freeRight - freeLeft < title;
    if (folded == quickFolded) return;
    quickFolded = folded;
    applyQuickFolded();
    qDebug().noquote() << (folded ? QStringLiteral("Native quick access folded") : QStringLiteral("Native quick access unfolded"));
  }

  // The group's menu: while folded its commands first, then the File,
  // Edit, View and Tools menus' commands as checks, Reset to Default.
  NSMenu* quickChecks(const QList<NativeMacMenuItem>& entries) {
    NSMenu* menu = [[NSMenu alloc] initWithTitle:@""];
    for (const auto& entry : entries) {
      NSMenuItem* item = [[NSMenuItem alloc] initWithTitle:ns(entry.title) action:nil keyEquivalent:@""];
      if (entry.submenu) {
        item.submenu = quickChecks(entry.children);
      } else {
        item.action = @selector(chooseQuickAccess:);
        item.target = delegate;
        item.representedObject = ns(entry.action);
        item.state = quickAccess.chosen.contains(entry.action) ? NSControlStateValueOn : NSControlStateValueOff;
      }
      [menu addItem:item];
    }
    return menu;
  }

  NSMenu* quickMenu() {
    NSMenu* menu = [[NSMenu alloc] initWithTitle:@""];
    QStringList top;
    if (quickFolded && !quickAccess.shown.isEmpty()) {
      for (const QString& id : std::as_const(quickAccess.shown)) {
        if (const NativeMacAction* command = action(id)) {
          [menu addItem:menuAction(*command, true)];
          top.append(command->label);
        }
      }
      [menu addItem:NSMenuItem.separatorItem];
    }
    for (const auto& entry : std::as_const(quickAccess.menus)) {
      NSMenuItem* item = [[NSMenuItem alloc] initWithTitle:ns(entry.title) action:nil keyEquivalent:@""];
      item.submenu = quickChecks(entry.children);
      [menu addItem:item];
      top.append(entry.title);
    }
    [menu addItem:NSMenuItem.separatorItem];
    NSMenuItem* reset = [[NSMenuItem alloc] initWithTitle:ns(QObject::tr("Reset to Default"))
                                                   action:@selector(resetQuickAccess:) keyEquivalent:@""];
    reset.target = delegate;
    [menu addItem:reset];
    top.append(qt(reset.title));
    // The group hidden (mitcad#1189); Settings › Display › Show
    // quick-access toolbar brings it back.
    NSMenuItem* hide = [[NSMenuItem alloc] initWithTitle:ns(QObject::tr("Hide Quick-Access Toolbar"))
                                                  action:@selector(hideQuickAccess:) keyEquivalent:@""];
    hide.target = delegate;
    [menu addItem:hide];
    top.append(qt(hide.title));
    qDebug().noquote() << QStringLiteral("Native quick access menu: %1").arg(top.join(QStringLiteral(" | ")));
    return menu;
  }

  void setQuickAccess(const NativeMacQuickAccess& group) {
    const bool rebuild = group.shown != quickAccess.shown || (quickGroup && quickGroup.arrangedSubviews.count == 0);
    const bool visibility = group.visible != quickAccess.visible;
    quickAccess = group;
    if (visibility) {
      // Preferences or the group's menu (mitcad#1189).
      if (quickGroup) quickGroup.hidden = !group.visible;
      qDebug().noquote() << (group.visible ? QStringLiteral("Native quick access shown")
                                           : QStringLiteral("Native quick access hidden"));
    }
    if (rebuild) {
      qDebug().noquote() << QStringLiteral("Native quick access: %1")
                                .arg(group.shown.isEmpty() ? QStringLiteral("none") : group.shown.join(QStringLiteral(", ")));
      quickLogged.clear();
      rebuildQuickGroup();
    }
    applyQuickState();
    scheduleQuickFold();
  }
};

NativeMacWindow::NativeMacWindow(QWidget* viewport, NativeMacWindowCallbacks callbacks)
    : m_state(std::make_unique<NativeMacWindowState>(viewport, std::move(callbacks))) {
  setInspectorText(QStringLiteral("Command Panel"), QStringLiteral("Select an object to inspect it, or start a command."));
}
NativeMacWindow::~NativeMacWindow() = default;
bool NativeMacWindow::isEmbedded() const { return m_state->error.isEmpty() && bool(m_state->foreignWindow); }
bool NativeMacWindow::modalBlocked() const { return m_state->nativeModalBlocked(); }
QString NativeMacWindow::embeddingError() const { return m_state->error; }
void NativeMacWindow::show() {
  [NSApp setActivationPolicy:NSApplicationActivationPolicyRegular];
  const bool separate = m_state->viewport && m_state->viewport->property("mitcadSeparateWindow").toBool();
  m_state->window.tabbingMode = separate ? NSWindowTabbingModeDisallowed : NSWindowTabbingModeAutomatic;
  // Ordering a window does not necessarily select its tab while the app
  // is inactive. Present the requested document in the actual tab group.
  NSWindowTabGroup* group = m_state->window.tabGroup;
  if (group.windows.count > 1 && [group.windows containsObject:m_state->window]) {
    group.selectedWindow = m_state->window;
  }
  [m_state->window makeKeyAndOrderFront:nil];
  m_state->window.tabbingMode = NSWindowTabbingModeAutomatic;
  [NSApp activate];
  [m_state->window.contentView layoutSubtreeIfNeeded];
  if (!m_state->frameInitialized) {
    // The first presentation settles AppKit's toolbar and title geometry.
    // Restore afterwards, before enabling automatic frame saves.
    const BOOL restoredFrame = [m_state->window setFrameUsingName:@"MitcadNativeDocumentWindow"];
    if (!restoredFrame) {
      [m_state->window setContentSize:NSMakeSize(1280, 800)];
      [m_state->window center];
    }
    [m_state->window setFrameAutosaveName:@"MitcadNativeDocumentWindow"];
    m_state->frameInitialized = true;
    [m_state->window.contentView layoutSubtreeIfNeeded];
  }
  m_state->syncViewport();
  focusViewport();
}
bool NativeMacWindow::close() { [m_state->window performClose:nil]; return m_state->closing; }
void NativeMacWindow::focusViewport() {
  if (m_state->viewport) {
    m_state->viewport->setFocus(Qt::OtherFocusReason);
    NSView* native = (__bridge NSView*)reinterpret_cast<void*>(m_state->viewport->winId());
    [m_state->window makeFirstResponder:native];
  }
}
void NativeMacWindow::setActions(const QList<NativeMacAction>& actions) { m_state->setActions(actions); }
void NativeMacWindow::setRibbon(const QList<NativeMacRibbonGroup>& groups, const QStringList& workspaces, int selected) {
  const bool changed = m_state->ribbonPresentation != groups || m_state->workspaces != workspaces ||
                       m_state->workspaceIndex != selected;
  if ((m_state->workspaces != workspaces || m_state->workspaceIndex != selected) && !m_state->workspaces.isEmpty()) {
    // The selector's tabs and the one shown, for UI tests: a drawing's
    // alone while it is open (mitcad#1073), the model's back when it closes.
    QStringList captions;
    for (const QString& workspace : workspaces) captions.append(workspaceCaption(workspace));
    qInfo().noquote() << QStringLiteral("Native workspaces: %1 (shown %2)")
                             .arg(captions.join(QStringLiteral(", ")), workspaceCaption(workspaces.value(selected)));
  }
  m_state->ribbonPresentation = groups;
  m_state->workspaces = workspaces;
  m_state->workspaceIndex = selected;
  NSSegmentedControl* tabs = m_state->workspaceTabs;
  if (tabs) {
    if (tabs.segmentCount != workspaces.size()) tabs.segmentCount = workspaces.size();
    for (int i = 0; i < workspaces.size(); ++i) {
      const QString caption = workspaceCaption(workspaces[i]);
      if (qt([tabs labelForSegment:i]) != caption) [tabs setLabel:ns(caption) forSegment:i];
    }
    if (tabs.selectedSegment != selected) tabs.selectedSegment = selected;
  }
  if (changed) m_state->buildRibbon();
}

void NativeMacWindow::setProjectsView(void* view, bool shown) {
  NSView* content = (__bridge NSView*)view;
  if (m_state->projectsContent != content) {
    [m_state->projectsContent removeFromSuperview];
    m_state->projectsContent = content;
    if (content) fill(content, m_state->projectsHost);
  }
  m_state->projectsHostBottom.active = shown && !m_state->projectsCollapsed;
  m_state->setCardVisible(m_state->projectsCard, shown);
  m_state->layoutOverlays();
  [m_state->projectsCard setNeedsDisplay:YES];
  checkProjectsNameEditor();
}

bool NativeMacWindow::editProjectsRow(const QString& identifier, const QString& text,
                                      std::function<void(const QString&, bool)> finished) {
  if (m_state->closing || m_state->projectsCard.hidden || !m_state->projectsContent) return false;
  if (m_state->projectsCollapsed) {
    // A name typed in place needs its row in sight: disclose the content.
    for (NSView* child in m_state->projectsCard.body.subviews) {
      if ([child isKindOfClass:NSButton.class] && [child.accessibilityIdentifier isEqualToString:@"native.projects.disclosure"]) {
        [m_state->delegate collapseProjects:child];
        break;
      }
    }
    if (m_state->projectsCollapsed) return false;
    [m_state->projectsCard layoutSubtreeIfNeeded];
  }
  NSOutlineView* outline = outlineIn(m_state->projectsContent);
  return outline != nil && beginProjectsNameEditor(outline, identifier, text, std::move(finished));
}

void NativeMacWindow::closeProjectsEditor() { closeProjectsNameEditor(); }

void NativeMacWindow::setBrowserIssues(const QString& failures, const QString& failureDetails,
                                      const QString& warnings, const QString& warningDetails) {
  const QStringList labels = {failures, warnings};
  const QStringList details = {failureDetails, warningDetails};
  const bool shown = !failures.isEmpty() || !warnings.isEmpty();
  for (int index = 0; index < 2; ++index) {
    NSButton* button = m_state->browserIssueButtons[NSUInteger(index)];
    if (qt(button.title) != labels[index]) button.title = ns(labels[index]);
    if (qt(button.toolTip) != details[index]) button.toolTip = ns(details[index]);
    button.hidden = labels[index].isEmpty();
  }
  if (m_state->browserIssues.hidden == !shown) return;
  m_state->browserIssues.hidden = !shown;
  m_state->browserTreeTop.constant = shown ? 66 : 38;
  m_state->layoutOverlays();
}

void NativeMacWindow::setConfigurationView(void* view) {
  NSView* content = (__bridge NSView*)view;
  if (m_state->configurationContent != content) {
    if (m_state->configurationDocument) m_state->configurationDocument->state = nullptr;
    m_state->configurationScroll.documentView = nil;
    [m_state->configurationContent removeFromSuperview];
    m_state->configurationDocument = nil;
    m_state->configurationContent = content;
    if (content) {
      MITCADInspectorContentView* document = [[MITCADInspectorContentView alloc] initWithFrame:NSZeroRect];
      document->state = m_state.get();
      m_state->configurationDocument = document;
      document.translatesAutoresizingMaskIntoConstraints = NO;
      fill(content, document);
      m_state->configurationScroll.documentView = document;
      [NSLayoutConstraint activateConstraints:@[
        [document.leadingAnchor constraintEqualToAnchor:m_state->configurationScroll.contentView.leadingAnchor],
        [document.topAnchor constraintEqualToAnchor:m_state->configurationScroll.contentView.topAnchor],
        [document.widthAnchor constraintEqualToAnchor:m_state->configurationScroll.contentView.widthAnchor]
      ]];
    }
  }
  // Its own card between Projects and Browser (mitcad#1020).
  m_state->showConfigurationCollapsed(m_state->configurationCollapsed);
  m_state->setCardVisible(m_state->configurationCard, content != nil);
  m_state->layoutOverlays();
}

bool NativeMacWindow::projectsShown() const { return !m_state->projectsCard.hidden; }

void NativeMacWindow::setModelItems(const QList<NativeMacTreeItem>& items) {
  m_state->rebuildTree(items);
  m_state->layoutOverlays();
}
void NativeMacWindow::setSelection(const QStringList& keys) { m_state->setSelected(keys); }
void NativeMacWindow::setHistory(const QList<NativeMacHistoryItem>& items, int marker) {
  m_state->rebuildHistory(items, marker);
}
void NativeMacWindow::setDocument(const QString& title, const QString& path, bool edited) {
  m_state->window.title = ns(title.isEmpty() ? QStringLiteral("Untitled") : title);
  m_state->documentTitle.stringValue = m_state->window.title;
  m_state->window.representedURL = path.isEmpty() ? nil : [NSURL fileURLWithPath:ns(path)];
  m_state->window.documentEdited = edited;
  m_state->scheduleQuickFold();
}
void NativeMacWindow::setHistorySelection(const QStringList& keys) {
  m_state->setHistorySelected(keys);
}
void NativeMacWindow::setHistoryFilter(const QString& label, bool active) {
  m_state->historyFilterButton.state = active ? NSControlStateValueOn : NSControlStateValueOff;
  m_state->historyFilterButton.toolTip = ns(QObject::tr("Timeline Filter: %1").arg(label));
}
void NativeMacWindow::setProjectStatus(const QString& text, const QString& tooltip) {
  m_state->projectText = text;
  m_state->projectTooltip = tooltip;
  // The toolbar owns its current controls. Palette samples and removed
  // items must neither receive live status nor determine item membership.
  for (NSToolbarItem* item in m_state->window.toolbar.items) {
    if (![item.itemIdentifier isEqualToString:projectID] || ![item.view isKindOfClass:NSButton.class]) continue;
    NSButton* button = (NSButton*)item.view;
    button.title = ns(text);
    button.toolTip = ns(tooltip);
    button.accessibilityLabel = ns(QObject::tr("Project: %1").arg(text));
  }
}
void NativeMacWindow::setAgentStatus(const QString& text, const QString& tooltip, bool connected, bool available) {
  m_state->agentText = text;
  m_state->agentTooltip = tooltip;
  m_state->agentConnected = connected;
  m_state->agentAvailable = available;
  for (NSToolbarItem* item in m_state->window.toolbar.items) {
    if (![item.itemIdentifier isEqualToString:agentID]) continue;
    for (NSView* child in item.view.subviews) {
      if ([child isKindOfClass:MITCADAgentButton.class]) m_state->applyAgentStatus((NSButton*)child);
    }
  }
}
void NativeMacWindow::setSketchMode(bool sketching) { m_state->sketching = sketching; }
void NativeMacWindow::setMessages(const MessageLog* log) {
  [m_state->messageBand showLog:log];
  m_state->layoutOverlays();
}
void NativeMacWindow::setInspectorText(const QString& title, const QString& text) {
  NSStackView* content = [[NSStackView alloc] initWithFrame:NSZeroRect];
  content.orientation = NSUserInterfaceLayoutOrientationVertical;
  content.alignment = NSLayoutAttributeLeading;
  content.spacing = 12;
  content.edgeInsets = NSEdgeInsetsMake(18, 16, 18, 16);
  NSTextField* heading = label(ns(title), [NSFont systemFontOfSize:18 weight:NSFontWeightSemibold]);
  NSTextField* message = [NSTextField wrappingLabelWithString:ns(text)];
  message.textColor = NSColor.secondaryLabelColor;
  message.selectable = YES;
  message.translatesAutoresizingMaskIntoConstraints = NO;
  [content addArrangedSubview:heading];
  [content addArrangedSubview:message];
  NSView* space = [[NSView alloc] initWithFrame:NSZeroRect];
  [content addArrangedSubview:space];
  [message.widthAnchor constraintEqualToAnchor:content.widthAnchor constant:-32].active = YES;
  m_state->installInspector(content);
}
void NativeMacWindow::setInspectorView(void* view, void* footer) {
  m_state->installInspector((__bridge NSView*)view, (__bridge NSView*)footer);
}
void NativeMacWindow::showInspector(bool shown) { m_state->setCardVisible(m_state->inspectorCard, shown); }
void NativeMacWindow::showSidebar(bool shown) { m_state->setCardVisible(m_state->sidebarCard, shown); }
void NativeMacWindow::showHistory(bool shown) { m_state->setCardVisible(m_state->historyCard, shown); }
void NativeMacWindow::showCommandSearch() { m_state->openSearch(); }
void NativeMacWindow::showTimelineSearch() { m_state->openSearch(true); }
void NativeMacWindow::setTimelineSearchResults(const QString& text, const QString& count,
                                               const QList<NativeMacHistoryItem>& results, int selected) {
  auto& state = *m_state;
  state.timelineResults = results;
  state.timelineMatches.clear();
  for (const auto& item : results) state.timelineMatches.insert(item.id);
  state.updateHistoryAppearance();
  if (!state.timelineSearch) return;
  if (qt(state.searchField.stringValue) != text) state.searchField.stringValue = ns(text);
  state.timelineSearchCount.stringValue = ns(count);
  [state.searchTable reloadData];
  if (selected >= 0 && selected < results.size()) {
    [state.searchTable selectRowIndexes:[NSIndexSet indexSetWithIndex:NSUInteger(selected)] byExtendingSelection:NO];
    [state.searchTable scrollRowToVisible:selected];
  } else [state.searchTable deselectAll:nil];
}
void NativeMacWindow::saveTimelineSearchScroll() {
  m_state->savedHistoryScroll = m_state->historyScroll.contentView.bounds.origin;
  m_state->savedBrowserScroll = m_state->tree.enclosingScrollView.contentView.bounds.origin;
}
void NativeMacWindow::restoreTimelineSearchScroll() {
  [m_state->historyScroll.contentView scrollToPoint:m_state->savedHistoryScroll];
  [m_state->historyScroll reflectScrolledClipView:m_state->historyScroll.contentView];
  NSScrollView* browser = m_state->tree.enclosingScrollView;
  [browser.contentView scrollToPoint:m_state->savedBrowserScroll];
  [browser reflectScrolledClipView:browser.contentView];
}
void* NativeMacWindow::nativeWindow() const { return (__bridge void*)m_state->window; }
void* NativeMacWindow::nativeInspectorHost() const { return (__bridge void*)m_state->inspectorHost; }
void NativeMacWindow::setCardWidth(const QString& card, int width) {
  NativeCard which = NativeCard::Projects;
  if (!nativeCardNamed(card, which) || width <= 0) return;
  m_state->cardWidths.restore(which, width);
  m_state->layoutOverlays();
}

void NativeMacWindow::setColumnSplit(const std::vector<std::pair<QString, int>>& split) {
  m_state->columnSplit = split;
  m_state->layoutOverlays();
}

void NativeMacWindow::setConfigurationCollapsed(bool collapsed) {
  m_state->showConfigurationCollapsed(collapsed);
  m_state->layoutOverlays();
}

void NativeMacWindow::setQuickAccess(const NativeMacQuickAccess& group) { m_state->setQuickAccess(group); }

void NativeMacWindow::setCubeArrows(const QList<NativeMacCubeArrow>& arrows) {
  m_state->setCubeArrows(arrows);
}

void NativeMacWindow::setChatView(void* view) {
  NSView* content = (__bridge NSView*)view;
  m_state->chatContent = content;
  setChatCardContent(m_state->chatHost, content);
}

void NativeMacWindow::showChat(bool shown) {
  auto& state = *m_state;
  const bool wasShown = !state.chatCard.hidden;
  state.setCardVisible(state.chatCard, shown);
  const bool actualShown = !state.chatCard.hidden;
  if (shown && !actualShown) {
    qInfo().noquote() << QStringLiteral("Native chat card deferred (a drawing is open)");
  } else if (wasShown != actualShown) {
    if (actualShown) {
      [state.chatCard layoutSubtreeIfNeeded];
      setChatCardContent(state.chatHost, state.chatContent);
      const NSSize size = state.chatCard.frame.size;
      qInfo().noquote() << QStringLiteral("Native chat card shown: %1 x %2")
                               .arg(std::lround(size.width)).arg(std::lround(size.height));
    } else {
      qInfo().noquote() << QStringLiteral("Native chat card hidden");
    }
  }
  if (shown && actualShown && !focusChatCard(state.chatHost)) {
    qInfo().noquote() << QStringLiteral("Native chat card: no message field to focus");
  }
  state.applyChatButtons();
}

bool NativeMacWindow::chatShown() const { return !m_state->chatCard.hidden; }
void NativeMacWindow::setDrawingOpen(bool open) { m_state->setDrawingOpen(open); }
void NativeMacWindow::setNavigationBar(const NativeMacNavigationBar& bar) { m_state->setNavigationBar(bar); }

void* nativeDocumentWindowMenu(void* nativeWindow) {
  NSWindow* window = (__bridge NSWindow*)nativeWindow;
  for (NativeMacWindowState* state : documentStates) {
    if (state->window == window && !state->closing) return (__bridge void*)state->windowsMenu;
  }
  return nullptr;
}

bool nativeWindowRefreshContract(void* nativeWindow, QString& error) {
  MITCADDocumentWindow* window = (__bridge MITCADDocumentWindow*)nativeWindow;
  if (![window isKindOfClass:MITCADDocumentWindow.class] || !window->state) {
    error = QStringLiteral("Native refresh contract requires a document window");
    return false;
  }
  auto& state = *window->state;
  const QList<NativeMacAction> snapshot = state.actions;
  NSArray<NSToolbarItemIdentifier>* allowed = [window.toolbar.delegate toolbarAllowedItemIdentifiers:window.toolbar];
  for (const auto& action : snapshot) {
    NSString* identifier = ns(QStringLiteral("command.") + action.id);
    if (!action.submenu.isEmpty() && [allowed containsObject:identifier]) {
      error = QStringLiteral("Native toolbar palette offered a popup without a standalone menu control");
      return false;
    }
  }
  state.setActions(snapshot);
  MITCADCommandButton* button = state.ribbonButtons.allValues.firstObject;
  if (!button || !button.image) {
    error = QStringLiteral("Native refresh contract requires a ribbon image");
    return false;
  }
  NSImage* image = button.image;
  NSView* ribbon = state.ribbonGroups;
  NSMenu* mainMenu = state.mainMenu;
  NSMutableArray<NSMenu*>* toolbarMenus = [NSMutableArray array];
  for (NSToolbarItem* item in window.toolbar.items) {
    if ([item isKindOfClass:NSMenuToolbarItem.class]) [toolbarMenus addObject:((NSMenuToolbarItem*)item).menu];
  }
  state.setActions(snapshot);
  if (button.image != image || state.ribbonGroups != ribbon || state.mainMenu != mainMenu) {
    error = QStringLiteral("Identical native actions replaced ribbon artwork or menus");
    return false;
  }
  NSUInteger index = 0;
  for (NSToolbarItem* item in window.toolbar.items) {
    if (![item isKindOfClass:NSMenuToolbarItem.class]) continue;
    if (index >= toolbarMenus.count || ((NSMenuToolbarItem*)item).menu != toolbarMenus[index++]) {
      error = QStringLiteral("Identical native actions replaced a toolbar popup menu");
      return false;
    }
  }
  QList<NativeMacAction> changed = snapshot;
  const int commandIndex = state.actionIndex.value(qt(button.representedObject), -1);
  if (commandIndex < 0) {
    error = QStringLiteral("Native refresh contract ribbon command is missing");
    return false;
  }
  auto& command = changed[commandIndex];
  command.enabled = !command.enabled;
  command.checked = !command.checked;
  command.tooltip = QStringLiteral("Native refresh contract tooltip");
  state.setActions(changed);
  const bool updated = button.image == image && state.ribbonGroups == ribbon &&
                       button.enabled == command.enabled &&
                       button.state == (command.checked ? NSControlStateValueOn : NSControlStateValueOff) &&
                       qt(button.toolTip) == command.tooltip;
  state.setActions(snapshot);
  if (!updated) {
    error = QStringLiteral("Native refresh did not update command state while retaining its image");
    return false;
  }
  NSMenuItem* redoItem = nil;
  for (NSMenuItem* top in state.mainMenu.itemArray) {
    if (![top.title isEqualToString:@"Edit"]) continue;
    for (NSMenuItem* item in top.submenu.itemArray) {
      if ([item.representedObject isEqual:@"edit.redo"]) redoItem = item;
    }
  }
  const int redoIndex = state.actionIndex.value(QStringLiteral("edit.redo"), -1);
  if (!redoItem || redoIndex < 0) {
    error = QStringLiteral("Native refresh contract requires the retained Redo menu item");
    return false;
  }
  changed = snapshot;
  for (bool enabled : {true, false}) {
    changed[redoIndex].enabled = enabled;
    state.setActions(changed);
    // Read the stored state before any test-side validation or key event.
    const bool menuUpdated = redoItem.enabled == enabled && state.mainMenu == mainMenu;
    if (!menuUpdated) {
      state.setActions(snapshot);
      error = QStringLiteral("Native refresh left the retained Redo key equivalent in a stale enabled state");
      return false;
    }
  }
  state.setActions(snapshot);
  // Offer the same unmatched key through both AppKit entry points. Count
  // dispatch attempts, including rejected keys, rather than action effects.
  NSString* characters = [NSString stringWithFormat:@"%C", unichar(NSF35FunctionKey)];
  NSEvent* event = [NSEvent keyEventWithType:NSEventTypeKeyDown location:NSZeroPoint
                             modifierFlags:NSEventModifierFlagCommand | NSEventModifierFlagControl |
                                           NSEventModifierFlagOption | NSEventModifierFlagShift
                                 timestamp:NSProcessInfo.processInfo.systemUptime
                              windowNumber:window.windowNumber context:nil characters:characters
               charactersIgnoringModifiers:characters isARepeat:NO keyCode:0];
  const quint64 before = state.shortcutDispatchCount;
  [window sendEvent:event];
  [window performKeyEquivalent:event];
  if (state.shortcutDispatchCount != before + 1) {
    error = QStringLiteral("One native key event reached shortcut dispatch more than once");
    return false;
  }
  return true;
}

} // namespace mitcad::mac

using namespace mitcad::mac;

@implementation MITCADBindingBadge
- (void)updateTrackingAreas {
  [super updateTrackingAreas];
  if (tracking) [self removeTrackingArea:tracking];
  tracking = nil;
  // A descendant's visibleRect may extend across its whole outline row.
  // NSTrackingInVisibleRect ignores the supplied rectangle, so use the
  // exposed part of the pill explicitly instead of tracking nearby names.
  const NSRect exposed = NSIntersectionRect(self.bounds, self.visibleRect);
  if (NSIsEmptyRect(exposed)) return;
  tracking = [[NSTrackingArea alloc] initWithRect:exposed
      options:NSTrackingMouseEnteredAndExited | NSTrackingActiveInActiveApp
        owner:self userInfo:nil];
  [self addTrackingArea:tracking];
}
- (void)viewWillMoveToWindow:(NSWindow*)window {
  if (!window) { [hoverTimer invalidate]; hoverTimer = nil; }
  [super viewWillMoveToWindow:window];
}
- (void)mouseEntered:(NSEvent*)event {
  (void)event;
  [hoverTimer invalidate];
  __weak MITCADBindingBadge* weakSelf = self;
  NSString* enteredKey = [self.representedObject copy];
  hoverTimer = [NSTimer scheduledTimerWithTimeInterval:0.45 repeats:NO block:^(NSTimer* timer) {
    (void)timer;
    MITCADBindingBadge* badge = weakSelf;
    if (badge && [badge.representedObject isEqual:enteredKey]) [badge requestCard:NO];
  }];
}
- (void)mouseExited:(NSEvent*)event {
  (void)event;
  [hoverTimer invalidate];
  hoverTimer = nil;
}
- (void)pinCard:(id)sender {
  (void)sender;
  [hoverTimer invalidate];
  hoverTimer = nil;
  [self requestCard:YES];
}
- (void)requestCard:(BOOL)pinned {
  if (!state || state->closing || state->nativeModalBlocked() || !NSApp.active ||
      !self.window.visible || !self.enabled || [self isHiddenOrHasHiddenAncestor]) return;
  if (!pinned) {
    // Revalidate after the hover delay: a recycled/scrolled row or a new
    // window must not turn a former entry into another item's card.
    const NSPoint screen = NSEvent.mouseLocation;
    const NSPoint local = [self convertPoint:[self.window convertPointFromScreen:screen] fromView:nil];
    const NSRect exposed = NSIntersectionRect(self.bounds, self.visibleRect);
    if (NSIsEmptyRect(exposed) || !NSPointInRect(local, exposed) ||
        [NSWindow windowNumberAtPoint:screen belowWindowWithWindowNumber:0] != self.window.windowNumber) return;
    NSView* root = self.window.contentView;
    NSView* hit = [root hitTest:[self convertPoint:local toView:root.superview]];
    if (hit != self && ![hit isDescendantOf:self]) return;
  }
  const QString key = qt(self.representedObject);
  const auto callback = history ? state->callbacks.historyBindings : state->callbacks.treeBindings;
  if (!callback) return;
  const NSRect screen = [self.window convertRectToScreen:[self convertRect:self.bounds toView:nil]];
  const CGFloat top = NSMaxY(NSScreen.screens.firstObject.frame);
  const QRect anchor(qRound(screen.origin.x), qRound(top - NSMaxY(screen)),
                     std::max(1, qRound(screen.size.width)), std::max(1, qRound(screen.size.height)));
  qDebug().noquote() << QStringLiteral("Native %1 fx %2: %3 (%4)")
      .arg(history ? QStringLiteral("history") : QStringLiteral("browser"), key, qt(self.title),
           pinned ? QStringLiteral("pinned") : QStringLiteral("hover"));
  callback(key, anchor, pinned);
}
- (void)drawRect:(NSRect)dirty {
  (void)dirty;
  [NSColor.controlBackgroundColor setFill];
  [[NSBezierPath bezierPathWithRoundedRect:NSInsetRect(self.bounds, 0.5, 0.5) xRadius:4 yRadius:4] fill];
  NSDictionary* attributes = @{NSFontAttributeName:self.font != nil ? self.font : [NSFont systemFontOfSize:NSFont.smallSystemFontSize],
                               NSForegroundColorAttributeName:NSColor.labelColor};
  const NSSize size = [self.title sizeWithAttributes:attributes];
  [self.title drawAtPoint:NSMakePoint((self.bounds.size.width - size.width) / 2,
                                     (self.bounds.size.height - size.height) / 2)
          withAttributes:attributes];
}
@end

@implementation MITCADDocumentWindow
- (BOOL)performKeyEquivalent:(NSEvent*)event {
  if (state && state->dispatchShortcutOnce(event)) return YES;
  return [super performKeyEquivalent:event];
}
- (void)sendEvent:(NSEvent*)event {
  if (state && (state->dispatchGesture(event) || state->dispatchShortcutOnce(event))) return;
  // The embedded QNSView consumes its keys into Qt rather than offering an
  // unhandled Escape to this window's responder fallback. Its standalone
  // viewport does not own the retained MainWindow's Cancel QAction either.
  // Route only the actual document canvas's plain Escape through the shared
  // cancellation callback; native panels and CAD text editors keep theirs.
  if (state && event.type == NSEventTypeKeyDown && event.window == self &&
      NSApp.active && NSApp.keyWindow == self && !state->closing &&
      !QApplication::activeModalWidget() && !state->textEditing()) {
    NSResponder* responder = self.firstResponder;
    NSView* view = [responder isKindOfClass:NSView.class] ? (NSView*)responder : nil;
    const QKeySequence pressed = keySequence(event);
    if (view && [view isDescendantOf:state->viewportHost] && !pressed.isEmpty() &&
        pressed[0].keyboardModifiers() == Qt::NoModifier && pressed[0].key() == Qt::Key_Escape &&
        [self dispatchCancellation]) {
      if (!qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT"))
        qInfo() << "Native document Escape: viewport -> shared cancellation";
      return;
    }
  }
  [super sendEvent:event];
}
- (BOOL)dispatchConfirmation {
  if (!state || state->nativeModalBlocked()) return NO;
  if ([self.firstResponder isKindOfClass:NSTextView.class] && ((NSTextView*)self.firstResponder).editable) return NO;
  return state->callbacks.confirm && state->callbacks.confirm();
}
- (BOOL)dispatchCancellation {
  if (!state || state->nativeModalBlocked()) return NO;
  if ([self.firstResponder isKindOfClass:NSTextView.class] && ((NSTextView*)self.firstResponder).editable) return NO;
  return state->callbacks.cancel && state->callbacks.cancel();
}
- (BOOL)dispatchKeyboardEvent:(NSEvent*)event {
  // AppKit sends unhandled keys up the responder chain. Text delegates and
  // table row editing get first refusal before the document's CAD actions.
  const NSEventModifierFlags modifiers = event.modifierFlags &
      (NSEventModifierFlagCommand | NSEventModifierFlagControl | NSEventModifierFlagOption | NSEventModifierFlagShift);
  const unichar key = event.charactersIgnoringModifiers.length ? [event.charactersIgnoringModifiers characterAtIndex:0] : 0;
  if (modifiers == 0 && (key == NSCarriageReturnCharacter || key == NSEnterCharacter)) return [self dispatchConfirmation];
  if (modifiers == 0 && key == 0x1b) return [self dispatchCancellation];
  return NO;
}
- (void)keyDown:(NSEvent*)event {
  if ([self dispatchKeyboardEvent:event]) return;
  [super keyDown:event];
}
- (void)insertNewline:(id)sender {
  if (![self dispatchConfirmation]) [self.nextResponder tryToPerform:_cmd with:sender];
}
- (void)cancelOperation:(id)sender {
  if (![self dispatchCancellation]) [self.nextResponder tryToPerform:_cmd with:sender];
}
@end

@implementation MITCADViewportHost
- (BOOL)isFlipped { return YES; }
- (void)setFrameSize:(NSSize)size {
  [super setFrameSize:size];
  if (state) state->syncViewport();
}
- (void)viewDidChangeBackingProperties {
  [super viewDidChangeBackingProperties];
  if (state) state->syncViewport();
}
@end

@implementation MITCADInspectorContentView
- (void)setFrameSize:(NSSize)size {
  const bool resized = std::abs(size.height - self.frame.size.height) > 0.5;
  [super setFrameSize:size];
  // layoutOverlays measures after its own layout pass; any other change of
  // the content's height asks for one.
  if (resized && state && !state->layingOut && state->canvasRoot) state->canvasRoot.needsLayout = YES;
}
@end

@implementation MITCADCanvasRoot
- (BOOL)isFlipped { return YES; }
- (void)layout {
  [super layout];
  if (state) {
    // In the window's layout pass: no nested inspector passes (mitcad#1078).
    const bool nested = state->inCanvasLayout;
    state->inCanvasLayout = true;
    state->layoutOverlays();
    state->inCanvasLayout = nested;
  }
}
- (void)setFrameSize:(NSSize)size {
  [super setFrameSize:size];
  if (state) state->layoutOverlays();
}
@end

@implementation MITCADModelOutlineView
- (void)keyDown:(NSEvent*)event {
  if (self.editedRow < 0 && [(MITCADDocumentWindow*)self.window dispatchKeyboardEvent:event]) return;
  [super keyDown:event];
}
- (void)cancelOperation:(id)sender {
  if (self.editedRow < 0 && [(MITCADDocumentWindow*)self.window dispatchCancellation]) return;
  [self.nextResponder tryToPerform:_cmd with:sender];
}
// The row under the pointer (mitcad#1072): the moves over the visible
// rows report its key, leaving the outline none.
- (void)updateTrackingAreas {
  [super updateTrackingAreas];
  if (hoverArea == nil) {
    hoverArea = [[NSTrackingArea alloc] initWithRect:NSZeroRect
                                             options:NSTrackingMouseMoved | NSTrackingMouseEnteredAndExited |
                                                     NSTrackingActiveInKeyWindow | NSTrackingInVisibleRect
                                               owner:self userInfo:nil];
    [self addTrackingArea:hoverArea];
  }
}
- (void)hoverAt:(NSEvent*)event {
  if (!state) return;
  const NSPoint point = [self convertPoint:event.locationInWindow fromView:nil];
  if (!NSPointInRect(point, self.visibleRect)) {
    state->hoverTree(nil);
    return;
  }
  const NSInteger row = [self rowAtPoint:point];
  MITCADTreeNode* node = row >= 0 ? [self itemAtRow:row] : nil;
  state->hoverTree(node.key);
}
- (void)mouseMoved:(NSEvent*)event {
  [super mouseMoved:event];
  [self hoverAt:event];
}
- (void)mouseEntered:(NSEvent*)event {
  [super mouseEntered:event];
  [self hoverAt:event];
}
- (void)mouseExited:(NSEvent*)event {
  [super mouseExited:event];
  // The table's own areas exit too; only a pointer out of the rows counts.
  [self hoverAt:event];
}
- (NSMenu*)menuForEvent:(NSEvent*)event {
  if (!state || !state->callbacks.treeContextMenu) return [super menuForEvent:event];
  const NSInteger row = [self rowAtPoint:[self convertPoint:event.locationInWindow fromView:nil]];
  MITCADTreeNode* node = row >= 0 ? [self itemAtRow:row] : nil;
  if (node && ![self.selectedRowIndexes containsIndex:NSUInteger(row)])
    [self selectRowIndexes:[NSIndexSet indexSetWithIndex:NSUInteger(row)] byExtendingSelection:NO];
  state->callbacks.treeContextMenu(node ? qt(node.key) : QString(), screenPoint(event));
  return nil;
}
@end

@implementation MITCADHistoryScroll
- (void)scrollWheel:(NSEvent*)event {
  NSClipView* clip = self.contentView;
  const CGFloat maximum = std::max<CGFloat>(0, self.documentView.frame.size.width - clip.bounds.size.width);
  const CGFloat delta = std::abs(event.scrollingDeltaY) > std::abs(event.scrollingDeltaX)
                            ? event.scrollingDeltaY : event.scrollingDeltaX;
  // Precise trackpad deltas are points. A wheel's delta is in lines.
  const CGFloat pixels = event.hasPreciseScrollingDeltas ? delta : delta * self.horizontalLineScroll;
  NSPoint point = clip.bounds.origin;
  point.x = std::clamp(point.x - pixels, CGFloat(0), maximum);
  [clip scrollToPoint:point];
  [self reflectScrolledClipView:clip];
  // Consume the event at either end too: it must not zoom the CAD canvas.
}
@end

@implementation MITCADHistoryStrip
- (BOOL)isFlipped { return YES; }
- (BOOL)isOpaque { return NO; }
- (BOOL)acceptsFirstResponder { return YES; }
- (void)mouseDown:(NSEvent*)event {
  (void)event;
  if (!state || state->nativeModalBlocked()) return;
  [self.window makeFirstResponder:self];
  state->setHistorySelected({});
  if (state->callbacks.historySelection) state->callbacks.historySelection({});
}
- (NSDragOperation)draggingEntered:(id<NSDraggingInfo>)info { return [self draggingUpdated:info]; }
- (NSDragOperation)draggingUpdated:(id<NSDraggingInfo>)info {
  if (!state || !state->historyCanChange() || ![info.draggingSource isKindOfClass:MITCADHistoryFeatureButton.class] ||
      ((MITCADHistoryFeatureButton*)info.draggingSource)->state != state) return NSDragOperationNone;
  const QString key = qt([info.draggingPasteboard stringForType:@"com.mitcad.feature-history"]);
  if (state->historyIndex(key) < 0) return NSDragOperationNone;
  state->historyDropBoundary = state->historyBoundary(info.draggingLocation, true);
  state->layoutHistory();
  return NSDragOperationMove;
}
- (void)draggingExited:(id<NSDraggingInfo>)info {
  (void)info;
  if (state) { state->historyDropBoundary = -1; state->layoutHistory(); }
}
- (BOOL)prepareForDragOperation:(id<NSDraggingInfo>)info {
  return [self draggingUpdated:info] == NSDragOperationMove;
}
- (BOOL)performDragOperation:(id<NSDraggingInfo>)info {
  if ([self draggingUpdated:info] != NSDragOperationMove) return NO;
  const QString key = qt([info.draggingPasteboard stringForType:@"com.mitcad.feature-history"]);
  const int from = state->historyIndex(key);
  const int boundary = state->historyDropBoundary;
  state->historyDropBoundary = -1;
  state->layoutHistory();
  state->reorderHistoryFeature(key, boundary - (boundary > from ? 1 : 0));
  return YES;
}
- (void)concludeDragOperation:(id<NSDraggingInfo>)info {
  (void)info;
  if (state) { state->historyDropBoundary = -1; state->layoutHistory(); }
}
- (void)keyDown:(NSEvent*)event {
  if (!state) return;
  if (!state->historyItems.isEmpty() && event.charactersIgnoringModifiers.length) {
    const unichar key = [event.charactersIgnoringModifiers characterAtIndex:0];
    if (key == NSLeftArrowFunctionKey || key == NSRightArrowFunctionKey || key == NSHomeFunctionKey || key == NSEndFunctionKey) {
      const QString current = state->selectedHistory.isEmpty() ? state->historyItems.front().id : state->selectedHistory.back();
      state->navigateHistory(current, key == NSHomeFunctionKey ? -2 : key == NSEndFunctionKey ? 2 : key == NSLeftArrowFunctionKey ? -1 : 1,
                             event.modifierFlags);
      return;
    }
  }
  if ([(MITCADDocumentWindow*)self.window dispatchKeyboardEvent:event]) return;
  [super keyDown:event];
}
@end

@implementation MITCADHistoryFeatureButton
- (BOOL)isFlipped { return YES; }
- (BOOL)acceptsFirstResponder { return self.enabled; }
- (NSView*)hitTest:(NSPoint)point {
  const NSPoint local = [self convertPoint:point fromView:self.superview];
  if (!NSPointInRect(local, self.bounds)) return nil;
  for (NSView* child in self.subviews) {
    if ([child isKindOfClass:MITCADBindingBadge.class] && !child.hidden && NSPointInRect(local, child.frame))
      return [child hitTest:local];
  }
  return self;
}
- (void)selectHistoryFeature {
  if (state) state->selectHistoryFeature(qt(self.featureKey), 0);
}
- (void)activateHistoryFeature {
  if (state) state->activateHistoryFeature(qt(self.featureKey));
}
- (void)reorderHistoryFeatureToIndex:(NSInteger)index {
  if (state) state->reorderHistoryFeature(qt(self.featureKey), int(index));
}
// The timeline's hover (mitcad#1076): the pointer over the cell previews
// its feature; leaving, a press or a drag ends it.
- (void)updateTrackingAreas {
  [super updateTrackingAreas];
  if (hoverTracking) [self removeTrackingArea:hoverTracking];
  hoverTracking = [[NSTrackingArea alloc] initWithRect:NSZeroRect
      options:NSTrackingMouseEnteredAndExited | NSTrackingMouseMoved | NSTrackingActiveInActiveApp | NSTrackingInVisibleRect
        owner:self userInfo:nil];
  [self addTrackingArea:hoverTracking];
}
- (void)mouseEntered:(NSEvent*)event {
  (void)event;
  if (state && !pressed) state->hoverHistoryCell(qt(self.featureKey));
}
- (void)mouseMoved:(NSEvent*)event {
  // After a rebuild ended the hover, the next move over the cell starts it again.
  (void)event;
  if (state && !pressed) state->hoverHistoryCell(qt(self.featureKey));
}
- (void)mouseExited:(NSEvent*)event {
  (void)event;
  if (state && state->historyHover == qt(self.featureKey)) state->hoverHistoryCell(QString());
}
// The cell's tooltip is the shared timeline's, made when AppKit shows it
// (mitcad#1076): the feature details read the model.
- (void)setFrameSize:(NSSize)size {
  const BOOL changed = !NSEqualSizes(size, self.frame.size);
  [super setFrameSize:size];
  if (!changed) return;
  [self removeAllToolTips];
  if (!NSIsEmptyRect(self.bounds)) [self addToolTipRect:self.bounds owner:(id)self userData:nullptr];
}
- (NSString*)view:(NSView*)view stringForToolTip:(NSToolTipTag)tag point:(NSPoint)point userData:(void*)data {
  (void)view; (void)tag; (void)point; (void)data;
  return state ? ns(state->historyToolTip(qt(self.featureKey))) : @"";
}
- (void)mouseDown:(NSEvent*)event {
  if (!state || !self.enabled || state->nativeModalBlocked()) return;
  state->hoverHistoryCell(QString());
  [self.window makeFirstResponder:self];
  if (event.clickCount > 1) { [self activateHistoryFeature]; return; }
  pressed = YES;
  pressPoint = event.locationInWindow;
  [self highlight:YES];
}
- (void)mouseDragged:(NSEvent*)event {
  if (!pressed || !state || !state->historyCanChange() || !state->callbacks.historyReordered) return;
  if (std::hypot(event.locationInWindow.x - pressPoint.x, event.locationInWindow.y - pressPoint.y) < 5) return;
  pressed = NO;
  [self highlight:NO];
  NSPasteboardItem* pasteboard = [[NSPasteboardItem alloc] init];
  [pasteboard setString:self.featureKey forType:@"com.mitcad.feature-history"];
  NSDraggingItem* item = [[NSDraggingItem alloc] initWithPasteboardWriter:pasteboard];
  [item setDraggingFrame:self.bounds contents:self.image];
  [self beginDraggingSessionWithItems:@[item] event:event source:self];
}
- (void)mouseUp:(NSEvent*)event {
  const BOOL select = pressed;
  pressed = NO;
  [self highlight:NO];
  if (select && state && NSPointInRect([self convertPoint:event.locationInWindow fromView:nil], self.bounds))
    state->selectHistoryFeature(qt(self.featureKey), event.modifierFlags);
}
- (NSDragOperation)draggingSession:(NSDraggingSession*)session sourceOperationMaskForDraggingContext:(NSDraggingContext)context {
  (void)session;
  return context == NSDraggingContextWithinApplication && state && state->historyCanChange() ? NSDragOperationMove : NSDragOperationNone;
}
- (void)draggingSession:(NSDraggingSession*)session endedAtPoint:(NSPoint)point operation:(NSDragOperation)operation {
  (void)session; (void)point; (void)operation;
  if (state) { state->historyDropBoundary = -1; state->layoutHistory(); }
}
- (BOOL)ignoreModifierKeysForDraggingSession:(NSDraggingSession*)session { (void)session; return YES; }
- (NSMenu*)menuForEvent:(NSEvent*)event {
  if (!state || state->nativeModalBlocked() || !state->callbacks.historyContextMenu) return nil;
  const QString key = qt(self.featureKey);
  if (!state->selectedHistory.contains(key)) state->selectHistoryFeature(key, 0);
  state->callbacks.historyContextMenu(key, screenPoint(event));
  return nil;
}
- (void)selectAll:(id)sender {
  (void)sender;
  if (!state || state->nativeModalBlocked()) return;
  QStringList keys;
  for (const auto& item : state->historyItems) if (!item.filtered && item.enabled) keys.append(item.id);
  state->setHistorySelected(keys);
  if (state->callbacks.historySelection) state->callbacks.historySelection(state->selectedHistory);
}
- (void)keyDown:(NSEvent*)event {
  if (!state) return;
  if (event.charactersIgnoringModifiers.length) {
    const unichar key = [event.charactersIgnoringModifiers characterAtIndex:0];
    if (key == NSLeftArrowFunctionKey || key == NSRightArrowFunctionKey || key == NSHomeFunctionKey || key == NSEndFunctionKey) {
      state->navigateHistory(qt(self.featureKey), key == NSHomeFunctionKey ? -2 : key == NSEndFunctionKey ? 2 : key == NSLeftArrowFunctionKey ? -1 : 1,
                             event.modifierFlags);
      return;
    }
    if (key == NSCarriageReturnCharacter || key == NSEnterCharacter) { [self activateHistoryFeature]; return; }
    if (key == ' ') { state->selectHistoryFeature(qt(self.featureKey), event.modifierFlags); return; }
  }
  if ([(MITCADDocumentWindow*)self.window dispatchKeyboardEvent:event]) return;
  [super keyDown:event];
}
@end

@implementation MITCADHistoryMarkerButton
- (BOOL)isFlipped { return YES; }
- (BOOL)acceptsFirstResponder { return self.enabled; }
- (NSView*)hitTest:(NSPoint)point {
  return NSPointInRect([self convertPoint:point fromView:self.superview], self.bounds) ? self : nil;
}
- (BOOL)beginHistoryMarkerDrag {
  if (!state || !state->beginHistoryMarkerDrag()) return NO;
  [self.window makeFirstResponder:self];
  return YES;
}
- (void)dragHistoryMarkerToIndex:(NSInteger)index { if (state) state->dragHistoryMarker(int(index)); }
- (void)endHistoryMarkerDrag { if (state) state->finishHistoryMarkerDrag(); }
- (void)mouseDown:(NSEvent*)event { (void)event; [self beginHistoryMarkerDrag]; }
- (void)mouseDragged:(NSEvent*)event {
  if (state && state->historyMarkerDragging) state->dragHistoryMarker(state->historyBoundary(event.locationInWindow, true));
}
- (void)mouseUp:(NSEvent*)event {
  if (!state || !state->historyMarkerDragging) return;
  state->dragHistoryMarker(state->historyBoundary(event.locationInWindow));
  [self endHistoryMarkerDrag];
}
- (NSString*)accessibilityRole { return NSAccessibilitySliderRole; }
- (id)accessibilityValue { return state ? @(state->historyMarker) : @0; }
- (id)accessibilityMinValue { return @0; }
- (id)accessibilityMaxValue { return state ? @(state->historyItems.size()) : @0; }
- (void)setAccessibilityValue:(id)value {
  if (state && [value respondsToSelector:@selector(integerValue)]) state->requestHistoryMarker(int([value integerValue]));
}
- (BOOL)accessibilityPerformIncrement {
  if (!state || !state->historyCanChange() || state->historyMarker >= state->historyItems.size()) return NO;
  state->requestHistoryMarker(state->historyStep(1));
  return YES;
}
- (BOOL)accessibilityPerformDecrement {
  if (!state || !state->historyCanChange() || state->historyMarker <= 0) return NO;
  state->requestHistoryMarker(state->historyStep(-1));
  return YES;
}
- (void)keyDown:(NSEvent*)event {
  if (!state) return;
  if (event.charactersIgnoringModifiers.length) {
    const unichar key = [event.charactersIgnoringModifiers characterAtIndex:0];
    if (key == NSLeftArrowFunctionKey || key == NSRightArrowFunctionKey || key == NSHomeFunctionKey || key == NSEndFunctionKey) {
      state->requestHistoryMarker(key == NSHomeFunctionKey ? 0 : key == NSEndFunctionKey ? int(state->historyItems.size()) :
                                  state->historyStep(key == NSLeftArrowFunctionKey ? -1 : 1));
      return;
    }
  }
  if ([(MITCADDocumentWindow*)self.window dispatchKeyboardEvent:event]) return;
  [super keyDown:event];
}
- (void)cancelOperation:(id)sender {
  if (state && state->historyMarkerDragging) { [self endHistoryMarkerDrag]; return; }
  [self.nextResponder tryToPerform:_cmd with:sender];
}
@end

@implementation MITCADHistoryGroupView
- (BOOL)isFlipped { return YES; }
- (BOOL)isOpaque { return NO; }
- (NSView*)hitTest:(NSPoint)point {
  return NSPointInRect([self convertPoint:point fromView:self.superview], self.bounds) ? self : nil;
}
- (void)setCollapsed:(BOOL)collapsed {
  if (_collapsed == collapsed) return;
  _collapsed = collapsed;
  self.folder.hidden = !collapsed;
  [self refreshToolTip];
  [self setNeedsDisplay:YES];
}
- (void)setFrameSize:(NSSize)size {
  const BOOL changed = !NSEqualSizes(size, self.frame.size);
  [super setFrameSize:size];
  if (changed) [self refreshToolTip];
}
// A folded group's cell: the shared timeline's tooltip for the group.
// The band has none, as in the shared timeline.
- (void)refreshToolTip {
  [self removeAllToolTips];
  if (_collapsed && !NSIsEmptyRect(self.bounds)) [self addToolTipRect:self.bounds owner:(id)self userData:nullptr];
}
- (NSString*)view:(NSView*)view stringForToolTip:(NSToolTipTag)tag point:(NSPoint)point userData:(void*)data {
  (void)view; (void)tag; (void)point; (void)data;
  return state && _collapsed ? ns(state->historyToolTip(qt(self.firstKey))) : @"";
}
- (void)drawRect:(NSRect)dirty {
  (void)dirty;
  if (_collapsed) return; // the folder cell: its artwork and layer
  // The band over an open group's cells, as the shared timeline draws it.
  [roleColor(mitcad::ColorRole::GroupBand) setFill];
  const NSRect band = NSMakeRect(1, 1, std::max<CGFloat>(0, NSWidth(self.bounds) - 2), mitcad::timeline::top - 2);
  [[NSBezierPath bezierPathWithRoundedRect:band xRadius:1.5 yRadius:1.5] fill];
}
// Hovering a folded group's cell previews its features (mitcad#1076).
- (void)updateTrackingAreas {
  [super updateTrackingAreas];
  if (hoverTracking) [self removeTrackingArea:hoverTracking];
  hoverTracking = [[NSTrackingArea alloc] initWithRect:NSZeroRect
      options:NSTrackingMouseEnteredAndExited | NSTrackingMouseMoved | NSTrackingActiveInActiveApp | NSTrackingInVisibleRect
        owner:self userInfo:nil];
  [self addTrackingArea:hoverTracking];
}
- (void)mouseEntered:(NSEvent*)event {
  (void)event;
  if (state && _collapsed && !pressed) state->hoverHistoryCell(qt(self.firstKey));
}
- (void)mouseMoved:(NSEvent*)event {
  (void)event;
  if (state && _collapsed && !pressed) state->hoverHistoryCell(qt(self.firstKey));
}
- (void)mouseExited:(NSEvent*)event {
  (void)event;
  if (state && _collapsed && state->historyHover == qt(self.firstKey)) state->hoverHistoryCell(QString());
}
// A click on an open group's band folds it (on release over it); a
// double-click on a folded group's cell opens it again (mitcad#1077).
- (void)mouseDown:(NSEvent*)event {
  if (!state || state->nativeModalBlocked()) return;
  state->hoverHistoryCell(QString());
  pressed = YES;
  if (_collapsed && event.clickCount > 1) {
    pressed = NO;
    if (state->callbacks.historyGroupCollapsed) state->callbacks.historyGroupCollapsed(qt(self.groupName), false);
  }
}
- (void)mouseUp:(NSEvent*)event {
  const BOOL click = pressed;
  pressed = NO;
  if (!click || _collapsed || !state || !state->callbacks.historyGroupCollapsed ||
      !NSPointInRect([self convertPoint:event.locationInWindow fromView:nil], self.bounds)) return;
  state->callbacks.historyGroupCollapsed(qt(self.groupName), true);
}
// A right click on the band or the folded cell: the shared group menu.
- (NSMenu*)menuForEvent:(NSEvent*)event {
  if (!state || state->nativeModalBlocked() || !state->callbacks.historyGroupMenu) return nil;
  state->hoverHistoryCell(QString());
  state->callbacks.historyGroupMenu(qt(self.groupName), screenPoint(event));
  return nil;
}
- (BOOL)isAccessibilityElement { return YES; }
- (NSAccessibilityRole)accessibilityRole { return NSAccessibilityButtonRole; }
- (BOOL)accessibilityPerformPress {
  if (!state || state->nativeModalBlocked() || !state->callbacks.historyGroupCollapsed) return NO;
  state->callbacks.historyGroupCollapsed(qt(self.groupName), !_collapsed);
  return YES;
}
@end

@implementation MITCADNativeWindowDelegate
- (BOOL)respondsToSelector:(SEL)selector {
  return [super respondsToSelector:selector] ||
      (state && [state->previousAppDelegate respondsToSelector:selector]);
}
- (id)forwardingTargetForSelector:(SEL)selector {
  // Qt still owns activation, hover updates and secure state restoration.
  // Keep every delegate method the shell does not override in that chain.
  if (state && [state->previousAppDelegate respondsToSelector:selector]) return state->previousAppDelegate;
  return [super forwardingTargetForSelector:selector];
}
- (void)application:(NSApplication*)application openFiles:(NSArray<NSString*>*)files {
  // Cocoa can replay command-line values as launch-time file requests.
  // Qt filters those before delivering genuine Finder/Dock file opens to
  // FileOpenFilter, which queues them for the process window manager.
  if (state && [state->previousAppDelegate respondsToSelector:_cmd]) {
    [state->previousAppDelegate application:application openFiles:files];
    return;
  }
  bool handled = state && bool(state->callbacks.openFile);
  if (handled) for (NSString* file in files) state->callbacks.openFile(qt(file));
  [application replyToOpenOrPrint:handled ? NSApplicationDelegateReplySuccess : NSApplicationDelegateReplyFailure];
}
- (void)application:(NSApplication*)application openURLs:(NSArray<NSURL*>*)urls {
  if (state && [state->previousAppDelegate respondsToSelector:_cmd]) {
    [state->previousAppDelegate application:application openURLs:urls];
    return;
  }
  // LaunchServices also sends local files as URLs. Qt's openFiles handler
  // owns launch filtering and the queued FileOpenFilter path for both forms.
  NSMutableArray<NSString*>* files = [NSMutableArray array];
  for (NSURL* url in urls) if (url.fileURL && url.path) [files addObject:url.path];
  if (files.count > 0) [self application:application openFiles:files];
}
- (BOOL)applicationShouldHandleReopen:(NSApplication*)application hasVisibleWindows:(BOOL)visible {
  // Qt updates its application state here even without an activation event.
  if (state && [state->previousAppDelegate respondsToSelector:_cmd]) {
    [state->previousAppDelegate applicationShouldHandleReopen:application hasVisibleWindows:visible];
  }
  if (!state || state->closing) return NO;
  if (state->window.miniaturized) [state->window deminiaturize:nil];
  [state->window makeKeyAndOrderFront:nil];
  return YES;
}
- (NSApplicationTerminateReply)applicationShouldTerminate:(NSApplication*)application {
  (void)application;
  if (state && state->callbacks.requestQuit) state->callbacks.requestQuit();
  return NSTerminateCancel;
}
- (BOOL)applicationShouldTerminateAfterLastWindowClosed:(NSApplication*)application {
  (void)application;
  // The process window manager decides when the final document has closed.
  return NO;
}
- (BOOL)windowShouldClose:(NSWindow*)window {
  (void)window;
  if (!state || state->closing) return YES;
  state->finishHistoryMarkerDrag();
  return !state->callbacks.requestClose || state->callbacks.requestClose();
}
- (void)windowWillClose:(NSNotification*)notification {
  (void)notification;
  if (!state || state->closing) return;
  state->finishHistoryMarkerDrag();
  state->closing = true;
  if (state->callbacks.closed) state->callbacks.closed();
}
- (void)windowWillBeginSheet:(NSNotification*)notification {
  (void)notification;
  if (state) {
    state->finishHistoryMarkerDrag();
    state->cancelViewportPointerInteraction();
    state->updateHistoryAppearance();
  }
}
- (void)windowDidEndSheet:(NSNotification*)notification {
  (void)notification;
  if (state) state->updateHistoryAppearance();
}
- (void)windowDidBecomeKey:(NSNotification*)notification {
  (void)notification;
  if (!state || state->closing) return;
  state->installMenus();
  if (state->callbacks.activated) state->callbacks.activated();
}
- (void)windowDidChangeOcclusionState:(NSNotification*)notification {
  (void)notification;
  if (state && state->callbacks.presentationVisible) {
    state->callbacks.presentationVisible((state->window.occlusionState & NSWindowOcclusionStateVisible) != 0);
  }
}
- (void)windowDidResignKey:(NSNotification*)notification {
  (void)notification;
  if (state) {
    state->finishHistoryMarkerDrag();
    state->cancelViewportPointerInteraction();
  }
}
- (void)menuTrackingChanged:(NSNotification*)notification {
  (void)notification;
  if (!state || state->closing) return;
  state->finishHistoryMarkerDrag();
  state->cancelViewportPointerInteraction();
}
- (void)windowDidResize:(NSNotification*)notification {
  (void)notification;
  if (state) state->layoutOverlays();
  if (state) state->scheduleQuickFold();
}
- (void)windowDidEnterFullScreen:(NSNotification*)notification {
  (void)notification;
  if (state) state->syncViewport();
}
- (void)windowDidExitFullScreen:(NSNotification*)notification {
  (void)notification;
  if (state) state->syncViewport();
}
- (void)outlineViewItemDidExpand:(NSNotification*)notification {
  if (!state) return;
  MITCADTreeNode* node = notification.userInfo[@"NSObject"];
  if (!state->updatingSelection && node && state->callbacks.treeExpanded) state->callbacks.treeExpanded(qt(node.key), true);
  state->layoutOverlays();
}
- (void)outlineViewItemDidCollapse:(NSNotification*)notification {
  if (!state) return;
  MITCADTreeNode* node = notification.userInfo[@"NSObject"];
  if (!state->updatingSelection && node && state->callbacks.treeExpanded) state->callbacks.treeExpanded(qt(node.key), false);
  state->layoutOverlays();
}
- (void)showBrowserIssues:(id)sender {
  if (state && !state->nativeModalBlocked() && state->callbacks.browserIssues)
    state->callbacks.browserIssues(((NSButton*)sender).tag == 1, menuPoint((NSView*)sender));
}
- (void)changeWorkspace:(id)sender {
  if (state && state->callbacks.workspaceChanged) state->callbacks.workspaceChanged(int(((NSSegmentedControl*)sender).selectedSegment));
}
- (void)pinCommand:(id)sender {
  NSArray* row = [sender representedObject];
  if (!state || state->nativeModalBlocked() || !state->callbacks.pinCommand || ![row isKindOfClass:NSArray.class] ||
      row.count != 3) return;
  const QString id = qt(row[1]);
  // Unpin where the command is pinned, as Qt's row menu offers.
  const auto* command = state->action(id);
  if (!command) return;
  const bool pin = !command->pinnedToToolbar;
  qDebug().noquote() << QStringLiteral("Native toolbar menu %1 %2: %3")
                            .arg(qt(row[2]), id, pin ? QStringLiteral("Pin to Toolbar") : QStringLiteral("Unpin from Toolbar"));
  [self performSelector:@selector(applyRowCommand:) withObject:@[row[0], row[1], @(pin)] afterDelay:0];
}
- (void)changeShortcut:(id)sender {
  NSArray* row = [sender representedObject];
  if (!state || state->nativeModalBlocked() || !state->callbacks.commandShortcut || ![row isKindOfClass:NSArray.class] ||
      row.count != 3) return;
  qDebug().noquote() << QStringLiteral("Native toolbar menu %1 %2: Change Keyboard Shortcut…").arg(qt(row[2]), qt(row[1]));
  [self performSelector:@selector(applyRowCommand:) withObject:@[row[1]] afterDelay:0];
}
- (void)applyRowCommand:(NSArray*)command {
  // Run in the default run loop mode, after AppKit's menu tracking ends:
  // the ribbon remade by a pin, or the modal Keyboard Shortcuts, must not
  // start inside the menu's loop (a Qt timer fires in it too).
  if (!state) return;
  if (command.count == 3 && state->callbacks.pinCommand) {
    state->callbacks.pinCommand(qt(command[0]), qt(command[1]), [command[2] boolValue]);
  } else if (command.count == 1 && state->callbacks.commandShortcut) {
    state->callbacks.commandShortcut(qt(command[0]));
  }
}
- (BOOL)validateToolbarItem:(NSToolbarItem*)item {
  if (!state || state->nativeModalBlocked()) return NO;
  QString key = qt(item.itemIdentifier);
  if (!key.startsWith(QStringLiteral("command."))) return YES;
  key = key.mid(8);
  if ([NSApp.keyWindow.firstResponder isKindOfClass:NSTextView.class]) {
    if (key == QStringLiteral("edit.undo")) return NSApp.keyWindow.firstResponder.undoManager.canUndo;
    if (key == QStringLiteral("edit.redo")) return NSApp.keyWindow.firstResponder.undoManager.canRedo;
  }
  const auto* command = state->action(key);
  return command && command->enabled;
}
- (void)openTimelineSearch:(id)sender {
  (void)sender;
  if (state) state->openSearch(true);
}
- (void)collapseProjects:(id)sender {
  if (!state || state->nativeModalBlocked() || ![state->window makeFirstResponder:nil]) return;
  state->projectsCollapsed = !state->projectsCollapsed;
  state->projectsHostBottom.active = !state->projectsCollapsed;
  state->projectsHost.hidden = state->projectsCollapsed;
  NSButton* button = (NSButton*)sender;
  button.image = symbol(state->projectsCollapsed ? QStringLiteral("chevron.right") : QStringLiteral("chevron.down"));
  button.accessibilityLabel = state->projectsCollapsed ? ns(QObject::tr("Expand Projects")) : ns(QObject::tr("Collapse Projects"));
  state->layoutOverlays();
}
// The Configuration card's disclosure (mitcad#1020), remembered as the
// shared Floating card's.
- (void)collapseConfiguration:(id)sender {
  (void)sender;
  if (!state || state->nativeModalBlocked() || ![state->window makeFirstResponder:nil]) return;
  state->showConfigurationCollapsed(!state->configurationCollapsed);
  state->layoutOverlays();
  if (state->callbacks.configurationCollapsed) state->callbacks.configurationCollapsed(state->configurationCollapsed);
}
- (void)collapseBrowser:(id)sender {
  if (!state) return;
  state->browserCollapsed = !state->browserCollapsed;
  state->browserScroll.hidden = state->browserCollapsed;
  if (state->browserCollapsed) state->hoverTree(nil);
  ((NSButton*)sender).image = symbol(state->browserCollapsed ? QStringLiteral("chevron.right") : QStringLiteral("chevron.down"));
  state->layoutOverlays();
}
- (void)turnCube:(id)sender {
  if (!state || state->nativeModalBlocked() || !state->callbacks.cubeArrow ||
      ![sender isKindOfClass:MITCADCommandButton.class]) return;
  state->callbacks.cubeArrow(qt(((MITCADCommandButton*)sender).representedObject));
}
- (void)openNavigationStyles:(id)sender {
  if (!state || state->nativeModalBlocked() || ![sender isKindOfClass:NSView.class]) return;
  state->popUpNavigationStyles((NSView*)sender);
}
- (void)performAction:(id)sender {
  if (!state) return;
  NSString* identifier = [sender isKindOfClass:NSToolbarItem.class] ? [sender itemIdentifier] : [sender representedObject];
  QString key = qt(identifier);
  if (key.startsWith(QStringLiteral("command."))) key = key.mid(8);
  if ([sender isKindOfClass:NSView.class] && !state->nativeModalBlocked()) {
    for (const auto& group : state->ribbonPresentation) {
      if (group.workspace != state->workspace()) continue;
      for (const auto& shown : group.buttons) {
        if (shown.action != key || !shown.menuOnly || shown.menu.isEmpty()) continue;
        NSView* view = (NSView*)sender;
        [state->presentationMenu(shown.menu) popUpMenuPositioningItem:nil
             atLocation:NSMakePoint(0, view.bounds.size.height + 4) inView:view];
        return;
      }
    }
  }
  if (const NativeMacAction* action = state->action(key); action && !action->submenu.isEmpty() && action->enabled) {
    if (state->nativeModalBlocked()) return;
    if ([sender isKindOfClass:NSView.class]) {
      NSMenu* menu = [[NSMenu alloc] initWithTitle:ns(action->label)];
      for (const NativeMacAction& child : state->actions) {
        if (child.menu == action->submenu) [menu addItem:state->menuAction(child)];
      }
      NSView* view = (NSView*)sender;
      [menu popUpMenuPositioningItem:nil atLocation:NSMakePoint(0, view.bounds.size.height + 4) inView:view];
    }
    return;
  }
  if ([NSApp.keyWindow.firstResponder isKindOfClass:NSTextView.class]) {
    if (key == QStringLiteral("edit.undo")) { [NSApp sendAction:@selector(undo:) to:nil from:sender]; return; }
    if (key == QStringLiteral("edit.redo")) { [NSApp sendAction:@selector(redo:) to:nil from:sender]; return; }
    if (SEL selector = editingSelector(state->action(key))) {
      [NSApp sendAction:selector to:nil from:sender];
      return;
    }
  }
  if (state->nativeModalBlocked()) return;
  // End secondary/native field editing before an action changes document
  // state. A control that rejects validation keeps its editor and focus.
  if ([NSApp.keyWindow.firstResponder isKindOfClass:NSTextView.class] &&
      ![NSApp.keyWindow makeFirstResponder:nil]) return;
  if (key == QStringLiteral("tools.search")) state->openSearch(false, sender);
  else state->invoke(key);
}
- (void)toggleSidebar:(id)sender { (void)sender; if (state) state->setCardVisible(state->sidebarCard, state->sidebarCard.hidden); }
- (void)toggleInspector:(id)sender { (void)sender; if (state) state->setCardVisible(state->inspectorCard, state->inspectorCard.hidden); }
- (void)toggleHistory:(id)sender { (void)sender; if (state) state->setCardVisible(state->historyCard, state->historyCard.hidden); }
- (void)openSearch:(id)sender { if (state) state->openSearch(false, sender); }
- (void)closeDocument:(id)sender {
  if (!state) return;
  NSWindow* window = NSApp.keyWindow != nil ? NSApp.keyWindow : state->window;
  // A menu equivalent can make the document key before dispatching while
  // its search popover is still shown. Both windows dismiss that search.
  if (state->searchPopover.shown && (window == state->searchField.window || window == state->window)) {
    [state->searchPopover close];
    // Return keyboard routing to the document's existing responder. A
    // hidden popover's field editor must not disable plain CAD shortcuts.
    [state->window makeKeyWindow];
    return;
  }
  if ([window.identifier hasPrefix:@"native.inline."]) window = state->window;
  // Settings and other native secondary windows own Command-W while key.
  // Ending the native field editor commits values and honors validation.
  if ([window makeFirstResponder:nil]) [window performClose:sender];
}
- (void)quitApplication:(id)sender {
  (void)sender;
  if (!state) return;
  if (state->callbacks.requestQuit) state->callbacks.requestQuit();
}
- (BOOL)validateMenuItem:(NSMenuItem*)item {
  if (!state) return NO;
  if (item.action == @selector(performAction:)) {
    const auto* action = state->action(qt(item.representedObject));
    if (!action) return NO;
    const NSControlStateValue value = action->checked ? NSControlStateValueOn : NSControlStateValueOff;
    if (item.state != value) item.state = value;
    if (qt(item.toolTip) != action->tooltip) item.toolTip = ns(action->tooltip);
    if (item.image) {
      NSImage* image = state->actionImage(*action);
      if (item.image != image) item.image = image;
    }
    // Plain CAD tool keys must leave the native field editor free to type.
    const bool textEditor = [NSApp.keyWindow.firstResponder isKindOfClass:NSTextView.class];
    if (textEditor && action->id == QStringLiteral("edit.undo")) return NSApp.keyWindow.firstResponder.undoManager.canUndo;
    if (textEditor && action->id == QStringLiteral("edit.redo")) return NSApp.keyWindow.firstResponder.undoManager.canRedo;
    if (textEditor) if (SEL selector = editingSelector(action))
      return [NSApp targetForAction:selector to:nil from:item] != nil;
    if (state->nativeModalBlocked()) return NO;
    const bool plainShortcut = item.keyEquivalent.length > 0 &&
        !(item.keyEquivalentModifierMask & (NSEventModifierFlagCommand | NSEventModifierFlagControl));
    return action->enabled && !(textEditor && plainShortcut);
  }
  if (item.action == @selector(pinCommand:) || item.action == @selector(changeShortcut:)) {
    const bool handled = item.action == @selector(pinCommand:) ? bool(state->callbacks.pinCommand)
                                                               : bool(state->callbacks.commandShortcut);
    if (!handled || state->nativeModalBlocked()) return NO;
    if (item.action == @selector(changeShortcut:)) return YES;
    // Pin to Toolbar is disabled for a command that is a button of its
    // group already (mitcad#1084); Unpin always works.
    NSArray* row = item.representedObject;
    const auto* command = [row isKindOfClass:NSArray.class] && row.count == 3 ? state->action(qt(row[1])) : nullptr;
    return command && (command->pinnedToToolbar || command->pinnable);
  }
  if (item.action == @selector(toggleSidebar:)) item.title = state->sidebarCard.hidden ? ns(QObject::tr("Show Sidebar")) : ns(QObject::tr("Hide Sidebar"));
  if (item.action == @selector(toggleInspector:)) item.title = state->inspectorCard.hidden ? ns(QObject::tr("Show Command Panel")) : ns(QObject::tr("Hide Command Panel"));
  if (item.action == @selector(toggleHistory:)) item.title = state->historyCard.hidden ? ns(QObject::tr("Show Timeline")) : ns(QObject::tr("Hide Timeline"));
  if (item.action == @selector(activateTree:) || item.action == @selector(renameTree:) || item.action == @selector(toggleTreeVisibility:)) {
    MITCADTreeNode* node = state->contextNode();
    const auto* modelItem = node ? state->treeItem(node.key) : nullptr;
    if (!modelItem || !modelItem->enabled) return NO;
    if (item.action == @selector(renameTree:)) return modelItem->editable && bool(state->callbacks.rename);
    if (item.action == @selector(toggleTreeVisibility:)) {
      const bool visible = modelItem->hasVisibility ? modelItem->checked : modelItem->visible;
      item.title = visible ? ns(QObject::tr("Hide")) : ns(QObject::tr("Show"));
      return modelItem->canToggleVisibility &&
             (bool(state->callbacks.visibilityChanged) || bool(state->callbacks.treeVisibility));
    }
    return bool(state->callbacks.activate) && modelItem->kind != QStringLiteral("folder");
  }
  return YES;
}
- (NSArray<NSToolbarItemIdentifier>*)toolbarDefaultItemIdentifiers:(NSToolbar*)toolbar {
  (void)toolbar;
  return @[@"command.edit.undo", @"command.edit.redo", workspaceID, NSToolbarFlexibleSpaceItemIdentifier,
           documentTitleID, NSToolbarFlexibleSpaceItemIdentifier, projectID, agentID, searchID];
}
- (NSArray<NSToolbarItemIdentifier>*)toolbarAllowedItemIdentifiers:(NSToolbar*)toolbar {
  (void)toolbar;
  NSMutableArray* identifiers = [NSMutableArray arrayWithArray:@[
    workspaceID, documentTitleID, sidebarID, createID, modifyID, constraintsID, searchID, historyID, inspectorID, projectID, agentID,
    NSToolbarFlexibleSpaceItemIdentifier, NSToolbarSpaceItemIdentifier
  ]];
  if (state) for (const auto& item : state->actions) {
    // Popup parents need their ribbon view to present a menu; the palette
    // offers only actions that work as standalone toolbar commands.
    if (!item.separator && !item.id.isEmpty() && item.submenu.isEmpty()) {
      [identifiers addObject:ns(QStringLiteral("command.") + item.id)];
    }
  }
  return identifiers;
}
- (NSToolbarItem*)toolbar:(NSToolbar*)toolbar itemForItemIdentifier:(NSToolbarItemIdentifier)identifier
         willBeInsertedIntoToolbar:(BOOL)inserted {
  (void)toolbar;
  if (!state) return nil;
  NSString* title = nil;
  NSString* image = nil;
  SEL selector = nil;
  if ([identifier isEqualToString:workspaceID]) {
    NSToolbarItem* item = [[NSToolbarItem alloc] initWithItemIdentifier:identifier];
    item.label = ns(QObject::tr("Workspace"));
    NSSegmentedControl* tabs = [NSSegmentedControl segmentedControlWithLabels:@[ns(QObject::tr("Solid")), ns(QObject::tr("Build"))]
                            trackingMode:NSSegmentSwitchTrackingSelectOne target:self action:@selector(changeWorkspace:)];
    tabs.segmentStyle = NSSegmentStyleRounded;
    tabs.selectedSegment = state->workspaceIndex;
    tabs.accessibilityIdentifier = @"native.workspaces";
    if (inserted) state->workspaceTabs = tabs;
    item.view = tabs;
    if (inserted) {
      // The quick-access group after the tabs, in their item (mitcad#1113).
      state->quickGroup = state->makeQuickGroup();
      NSStackView* row = [NSStackView stackViewWithViews:@[tabs, state->quickGroup]];
      row.spacing = 8;
      item.view = row;
      state->rebuildQuickGroup();
      state->applyQuickState();
    }
    return item;
  }
  if ([identifier isEqualToString:documentTitleID]) {
    NSToolbarItem* item = [[NSToolbarItem alloc] initWithItemIdentifier:identifier];
    item.label = ns(QObject::tr("Document"));
    NSTextField* title = label(state->window.title, [NSFont systemFontOfSize:13 weight:NSFontWeightSemibold]);
    title.alignment = NSTextAlignmentCenter;
    title.lineBreakMode = NSLineBreakByTruncatingMiddle;
    title.accessibilityIdentifier = @"native.documentTitle";
    [title.widthAnchor constraintLessThanOrEqualToConstant:220].active = YES;
    if (inserted) state->documentTitle = title;
    item.view = title;
    return item;
  }
  if ([identifier isEqualToString:searchID]) {
    NSToolbarItem* item = [[NSToolbarItem alloc] initWithItemIdentifier:identifier];
    item.label = ns(QObject::tr("Command Search"));
    NSButton* button = [NSButton buttonWithTitle:ns(QObject::tr("Command Search")) target:self action:@selector(openSearch:)];
    button.image = symbol(QStringLiteral("magnifyingglass"));
    button.imagePosition = NSImageLeft;
    button.alignment = NSTextAlignmentLeft;
    button.bezelStyle = NSBezelStyleRounded;
    button.font = [NSFont systemFontOfSize:12];
    button.translatesAutoresizingMaskIntoConstraints = NO;
    button.accessibilityIdentifier = @"native.commandSearchButton";
    [button.widthAnchor constraintEqualToConstant:220].active = YES;
    [button.heightAnchor constraintEqualToConstant:30].active = YES;
    item.view = button;
    return item;
  }
  if ([identifier isEqualToString:projectID]) {
    NSToolbarItem* item = [[NSToolbarItem alloc] initWithItemIdentifier:identifier];
    item.label = ns(QObject::tr("Project"));
    item.paletteLabel = ns(QObject::tr("Project Status"));
    NSButton* button = [NSButton buttonWithTitle:ns(state->projectText) target:self action:@selector(openProjectMenu:)];
    button.translatesAutoresizingMaskIntoConstraints = NO;
    button.bezelStyle = NSBezelStyleRounded;
    button.image = symbol(QStringLiteral("chevron.down"));
    button.imagePosition = NSImageRight;
    button.font = [NSFont systemFontOfSize:11 weight:NSFontWeightMedium];
    button.lineBreakMode = NSLineBreakByTruncatingTail;
    button.toolTip = ns(state->projectTooltip);
    button.accessibilityIdentifier = @"native.projectIndicator";
    button.accessibilityLabel = ns(QObject::tr("Project: %1").arg(state->projectText));
    [button setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationHorizontal];
    [button.widthAnchor constraintLessThanOrEqualToConstant:190].active = YES;
    [button.heightAnchor constraintEqualToConstant:26].active = YES;
    item.view = button;
    return item;
  }
  if ([identifier isEqualToString:agentID]) {
    NSToolbarItem* item = [[NSToolbarItem alloc] initWithItemIdentifier:identifier];
    item.label = ns(QObject::tr("AI Assistants"));
    item.paletteLabel = ns(QObject::tr("AI Assistant Status"));
    MITCADAgentButton* button = [[MITCADAgentButton alloc] initWithFrame:NSZeroRect];
    button.target = self;
    button.action = @selector(clickAgentIndicator:);
    button.translatesAutoresizingMaskIntoConstraints = NO;
    button.bezelStyle = NSBezelStyleRounded;
    button.imagePosition = NSImageLeft;
    button.font = [NSFont systemFontOfSize:11 weight:NSFontWeightMedium];
    button.lineBreakMode = NSLineBreakByTruncatingTail;
    button.accessibilityIdentifier = @"native.agentIndicator";
    state->applyAgentStatus(button);
    [button setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationHorizontal];
    [button.widthAnchor constraintLessThanOrEqualToConstant:160].active = YES;
    [button.heightAnchor constraintEqualToConstant:26].active = YES;
    NSButton* arrow = [NSButton buttonWithImage:symbol(QStringLiteral("chevron.down")) target:self action:@selector(openAgentMenu:)];
    arrow.bezelStyle = NSBezelStyleRounded;
    arrow.accessibilityIdentifier = @"native.agentMenu";
    arrow.accessibilityLabel = ns(QObject::tr("AI assistants menu"));
    [arrow.widthAnchor constraintEqualToConstant:24].active = YES;
    [arrow.heightAnchor constraintEqualToConstant:26].active = YES;
    // Chat beside the indicator (mitcad#1075), as the Qt title row's
    // titleChat: View › Chat.
    MITCADCommandButton* chat = [[MITCADCommandButton alloc] initWithFrame:NSZeroRect];
    chat.representedObject = @"view.chat";
    chat.target = self;
    chat.action = @selector(performAction:);
    chat.bezelStyle = NSBezelStyleRounded;
    [chat setButtonType:NSButtonTypePushOnPushOff];
    chat.imagePosition = NSImageOnly;
    chat.translatesAutoresizingMaskIntoConstraints = NO;
    chat.accessibilityIdentifier = @"native.chatButton";
    state->applyChatButton(chat);
    [chat.widthAnchor constraintEqualToConstant:30].active = YES;
    [chat.heightAnchor constraintEqualToConstant:26].active = YES;
    NSStackView* row = [NSStackView stackViewWithViews:@[button, arrow, chat]];
    row.spacing = 0;
    [row setCustomSpacing:6 afterView:arrow];
    item.view = row;
    return item;
  }
  if ([identifier isEqualToString:sidebarID]) { title = ns(QObject::tr("Sidebar")); image = @"sidebar.left"; selector = @selector(toggleSidebar:); }
  else if ([identifier isEqualToString:inspectorID]) { title = ns(QObject::tr("Command Panel")); image = @"sidebar.right"; selector = @selector(toggleInspector:); }
  else if ([identifier isEqualToString:historyID]) { title = ns(QObject::tr("Timeline")); image = @"clock.arrow.circlepath"; selector = @selector(toggleHistory:); }
  else if ([identifier isEqualToString:searchID]) { title = ns(QObject::tr("Command Search")); image = @"magnifyingglass"; selector = @selector(openSearch:); }
  else if ([identifier isEqualToString:createID] || [identifier isEqualToString:modifyID] || [identifier isEqualToString:constraintsID]) {
    NSMenuToolbarItem* item = [[NSMenuToolbarItem alloc] initWithItemIdentifier:identifier];
    const bool modify = [identifier isEqualToString:modifyID];
    const bool constrain = [identifier isEqualToString:constraintsID];
    item.label = constrain ? ns(QObject::tr("Constraints")) : (modify ? ns(QObject::tr("Modify")) : ns(QObject::tr("Create")));
    item.paletteLabel = item.label;
    item.image = symbol(qt(constrain ? @"link" : (modify ? @"wrench.and.screwdriver" : @"plus")), item.label);
    item.menu = state->groupMenu(constrain ? QStringLiteral("CONSTRAINTS") : (modify ? QStringLiteral("MODIFY") : QStringLiteral("CREATE")));
    item.toolTip = item.label;
    return item;
  } else {
    const QString key = qt(identifier);
    if (!key.startsWith(QStringLiteral("command."))) return nil;
    const auto* action = state->action(key.mid(8));
    if (!action || !action->submenu.isEmpty()) return nil;
    NSToolbarItem* item = [[NSToolbarItem alloc] initWithItemIdentifier:identifier];
    item.label = ns(action->label);
    item.paletteLabel = item.label;
    item.toolTip = ns(action->tooltip.isEmpty() ? action->label : action->tooltip);
    item.image = state->actionImage(*action);
    item.target = self;
    item.action = @selector(performAction:);
    item.enabled = action->enabled;
    return item;
  }
  NSToolbarItem* item = [[NSToolbarItem alloc] initWithItemIdentifier:identifier];
  item.label = title;
  item.paletteLabel = title;
  item.toolTip = title;
  item.image = symbol(qt(image), title);
  item.target = self;
  item.action = selector;
  return item;
}
- (NSInteger)outlineView:(NSOutlineView*)outline numberOfChildrenOfItem:(id)item {
  (void)outline;
  if (!state) return 0;
  return item ? ((MITCADTreeNode*)item).children.count : state->treeRoots.count;
}
- (id)outlineView:(NSOutlineView*)outline child:(NSInteger)index ofItem:(id)item {
  (void)outline;
  return item ? ((MITCADTreeNode*)item).children[NSUInteger(index)] : state->treeRoots[NSUInteger(index)];
}
- (BOOL)outlineView:(NSOutlineView*)outline isItemExpandable:(id)item {
  (void)outline; return ((MITCADTreeNode*)item).children.count > 0;
}
- (id)outlineView:(NSOutlineView*)outline persistentObjectForItem:(id)item { (void)outline; return ((MITCADTreeNode*)item).key; }
- (id)outlineView:(NSOutlineView*)outline itemForPersistentObject:(id)object { (void)outline; return state ? state->treeNodes[object] : nil; }
- (NSView*)outlineView:(NSOutlineView*)outline viewForTableColumn:(NSTableColumn*)column item:(id)object {
  (void)column;
  MITCADTreeNode* node = object;
  const auto* item = state ? state->treeItem(node.key) : nullptr;
  if (!item) return nil;
  NSTableCellView* cell = [outline makeViewWithIdentifier:@"modelCell" owner:self];
  if (!cell) {
    cell = [[NSTableCellView alloc] initWithFrame:NSZeroRect];
    cell.identifier = @"modelCell";
      NSStackView* row = [NSStackView stackViewWithViews:@[]];
      row.orientation = NSUserInterfaceLayoutOrientationHorizontal;
      row.alignment = NSLayoutAttributeCenterY;
      row.spacing = 4;
      row.translatesAutoresizingMaskIntoConstraints = NO;
      [cell addSubview:row];
      MITCADCommandButton* eye = [MITCADCommandButton buttonWithTitle:@"" target:self action:@selector(toggleTreeVisibility:)];
      eye.bordered = NO;
      eye.tag = 1;
      [eye.widthAnchor constraintEqualToConstant:16].active = YES;
      [eye.heightAnchor constraintEqualToConstant:18].active = YES;
      [row addArrangedSubview:eye];
      NSImageView* icon = [NSImageView new];
      [icon.widthAnchor constraintEqualToConstant:16].active = YES;
      [icon.heightAnchor constraintEqualToConstant:16].active = YES;
      cell.imageView = icon;
      [row addArrangedSubview:icon];
      NSTextField* title = label(@"");
      title.delegate = self;
      title.lineBreakMode = NSLineBreakByTruncatingMiddle;
      [title setContentHuggingPriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationHorizontal];
      cell.textField = title;
      [row addArrangedSubview:title];
      MITCADCommandButton* active = [MITCADCommandButton buttonWithTitle:@"" target:self action:@selector(activateComponent:)];
      active.bordered = NO;
      active.tag = 2;
      [active.widthAnchor constraintEqualToConstant:16].active = YES;
      [active.heightAnchor constraintEqualToConstant:18].active = YES;
      MITCADBindingBadge* bindings = [[MITCADBindingBadge alloc] initWithFrame:NSZeroRect];
      bindings->state = state;
      bindings->history = false;
      [state->bindingBadges addObject:bindings];
      bindings.target = bindings;
      bindings.action = @selector(pinCard:);
      bindings.tag = 3;
      bindings.font = [NSFont systemFontOfSize:10 weight:NSFontWeightSemibold];
      [bindings.widthAnchor constraintGreaterThanOrEqualToConstant:32].active = YES;
      [bindings.heightAnchor constraintEqualToConstant:18].active = YES;
      [row addArrangedSubview:bindings];
      [row addArrangedSubview:active];
      [NSLayoutConstraint activateConstraints:@[
        [row.leadingAnchor constraintEqualToAnchor:cell.leadingAnchor constant:2],
        [row.trailingAnchor constraintEqualToAnchor:cell.trailingAnchor constant:-4],
        [row.centerYAnchor constraintEqualToAnchor:cell.centerYAnchor]
      ]];
  }
  cell.textField.stringValue = ns(item->title);
  cell.textField.identifier = node.key;
  cell.textField.editable = item->editable && bool(state->callbacks.rename);
  cell.textField.selectable = cell.textField.editable;
  // The shared row's colour (mitcad#1079): a failed row red, a warned
  // one the warning's, a suppressed, rolled-back or hidden one grey.
  NSColor* tone = item->tone == QLatin1String("failed")    ? roleColor(mitcad::ColorRole::ErrorText)
                  : item->tone == QLatin1String("warning") ? roleColor(mitcad::ColorRole::WarningText)
                  : item->tone == QLatin1String("dimmed")  ? roleColor(mitcad::ColorRole::TextHidden)
                                                           : nil;
  cell.textField.textColor = !item->enabled ? NSColor.tertiaryLabelColor : tone != nil ? tone : NSColor.labelColor;
  NativeMacAction artwork;
  artwork.icon = item->icon;
  artwork.label = item->title;
  if (artwork.icon.isNull()) artwork.symbol = item->symbol;
  cell.imageView.image = state->actionImage(artwork, 16);
  const bool showsVisibility = item->canToggleVisibility || item->hasVisibility;
  const bool visible = item->hasVisibility ? item->checked : item->visible;
  NSStackView* row = (NSStackView*)cell.subviews.firstObject;
  for (NSView* child in row.arrangedSubviews) if ([child isKindOfClass:NSButton.class]) {
    MITCADCommandButton* button = (MITCADCommandButton*)child;
    button.representedObject = node.key;
    NativeMacAction drawing;
    if (button.tag == 1) {
      button.hidden = !showsVisibility;
      button.enabled = item->enabled && item->canToggleVisibility;
      drawing.icon = mitcad::themeIcon(visible ? QStringLiteral("eye") : QStringLiteral("eye-off"));
      button.toolTip = visible ? ns(QObject::tr("Hide")) : ns(QObject::tr("Show"));
      button.accessibilityIdentifier = ns(QStringLiteral("native.model.eye.") + item->id);
    } else if (button.tag == 3) {
      button.hidden = item->bindings <= 0;
      button.enabled = item->enabled;
      button.title = ns(QStringLiteral("fx %1").arg(item->bindings));
      button.accessibilityIdentifier = ns(QStringLiteral("native.model.fx.") + item->id);
      button.accessibilityLabel = ns(QObject::tr("%1 follows %2 bindings").arg(item->title).arg(item->bindings));
      button.toolTip = ns(QObject::tr("Follows expressions: point here to see what"));
      continue;
    } else {
      button.hidden = !item->hasActivation;
      button.enabled = item->enabled;
      drawing.icon = mitcad::themeIcon(item->active ? QStringLiteral("radio-on") : QStringLiteral("radio-off"));
      button.toolTip = ns(QObject::tr("Activate Component"));
      button.accessibilityIdentifier = ns(QStringLiteral("native.model.activate.") + item->id);
    }
    button.image = state->actionImage(drawing, 16);
    button.accessibilityLabel = [NSString stringWithFormat:@"%@ %@", button.toolTip, ns(item->title)];
    if (button.tag == 1) {
      // A bulb that follows a condition says what a click does, and its
      // own condition has the fx mark (mitcad#863, mitcad#1079).
      if (item->eyeBound) button.image = conditionBulb(button.image);
      button.accessibilityValue = item->eyeBound ? ns(QObject::tr("follows a condition")) : nil;
      if (!item->eyeTip.isEmpty()) {
        button.accessibilityLabel = [NSString stringWithFormat:@"%@ %@", visible ? ns(QObject::tr("Hide")) : ns(QObject::tr("Show")),
                                                               ns(item->title)];
        button.toolTip = ns(item->eyeTip);
      }
      button.accessibilityHelp = item->eyeTip.isEmpty() ? nil : ns(item->eyeTip);
    }
  }
  cell.alphaValue = showsVisibility && !visible ? 0.5 : 1.0;
  cell.toolTip = ns(item->detail);
  cell.accessibilityLabel = ns(item->title);
  cell.accessibilityValue = item->tone.isEmpty() ? nil : ns(item->tone); // mitcad#1079
  cell.accessibilityIdentifier = ns(QStringLiteral("native.model.") + item->id);
  return cell;
}
- (BOOL)outlineView:(NSOutlineView*)outline shouldSelectItem:(id)object {
  (void)outline;
  const auto* item = state ? state->treeItem(((MITCADTreeNode*)object).key) : nullptr;
  return item && item->enabled;
}
- (void)outlineViewSelectionDidChange:(NSNotification*)notification {
  (void)notification;
  if (!state || state->updatingSelection || !state->callbacks.selection) return;
  QStringList keys;
  NSUInteger row = state->tree.selectedRowIndexes.firstIndex;
  while (row != NSNotFound) {
    MITCADTreeNode* node = [state->tree itemAtRow:NSInteger(row)];
    if (node) keys.append(qt(node.key));
    row = [state->tree.selectedRowIndexes indexGreaterThanIndex:row];
  }
  state->callbacks.selection(keys);
}
- (void)activateTree:(id)sender {
  (void)sender;
  if (!state || !state->callbacks.activate) return;
  MITCADTreeNode* node = state->contextNode();
  if (node) state->callbacks.activate(qt(node.key));
}
- (void)renameTree:(id)sender {
  (void)sender;
  if (!state) return;
  MITCADTreeNode* node = state->contextNode();
  const auto* item = node ? state->treeItem(node.key) : nullptr;
  if (item && item->editable) [state->tree editColumn:0 row:[state->tree rowForItem:node] withEvent:nil select:YES];
}
- (void)activateComponent:(id)sender {
  if (state && state->callbacks.componentActivated) state->callbacks.componentActivated(qt([(MITCADCommandButton*)sender representedObject]));
}
- (void)toggleTreeVisibility:(id)sender {
  if (!state) return;
  MITCADTreeNode* node = [sender isKindOfClass:MITCADCommandButton.class] ? state->treeNodes[[sender representedObject]] : state->contextNode();
  const auto* item = node ? state->treeItem(node.key) : nullptr;
  if (item && item->canToggleVisibility) {
    if (state->callbacks.treeVisibility) state->callbacks.treeVisibility(item->id);
    else if (state->callbacks.visibilityChanged) state->callbacks.visibilityChanged(item->id, !(item->hasVisibility ? item->checked : item->visible));
  }
}
- (NSInteger)numberOfRowsInTableView:(NSTableView*)table {
  return state && table == state->searchTable ? (state->timelineSearch ? state->timelineResults.size() : state->searchResults.size()) : 0;
}
- (NSView*)tableView:(NSTableView*)table viewForTableColumn:(NSTableColumn*)column row:(NSInteger)row {
  (void)column;
  if (!state || table != state->searchTable || row < 0) return nil;
  if (state->timelineSearch) {
    if (row >= state->timelineResults.size()) return nil;
    const auto& result = state->timelineResults[int(row)];
    NSTextField* field = label(ns(result.title + QStringLiteral("   ") + result.detail));
    field.lineBreakMode = NSLineBreakByTruncatingTail;
    field.toolTip = ns(result.title + QStringLiteral("\n") + result.detail);
    field.accessibilityIdentifier = ns(QStringLiteral("native.timelineSearchResult.") + result.id);
    return field;
  }
  if (row >= state->searchResults.size()) return nil;
  const NativeMacAction* command = &state->actions[state->searchResults[int(row)]];
  NSTableCellView* cell = [table makeViewWithIdentifier:@"nativeListCell" owner:self];
  if (!cell) {
    cell = [[NSTableCellView alloc] initWithFrame:NSZeroRect];
    cell.identifier = @"nativeListCell";
    // NSTableCellView's outlets are non-owning. Retain the views locally until
    // the cell owns them as subviews, including under optimized ARC builds.
    // The command's shared CAD icon at the shared search's 18 points, then
    // its name and shortcut in the remaining width (mitcad#974).
    NSImageView* icon = [[NSImageView alloc] initWithFrame:NSMakeRect(0, 0, 18, 18)];
    icon.translatesAutoresizingMaskIntoConstraints = NO;
    icon.imageScaling = NSImageScaleProportionallyDown;
    NSTextField* text = label(@"");
    text.lineBreakMode = NSLineBreakByTruncatingTail;
    [text setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationHorizontal];
    [cell addSubview:icon];
    [cell addSubview:text];
    [NSLayoutConstraint activateConstraints:@[
      [icon.leadingAnchor constraintEqualToAnchor:cell.leadingAnchor constant:6],
      [icon.centerYAnchor constraintEqualToAnchor:cell.centerYAnchor],
      [icon.widthAnchor constraintEqualToConstant:18],
      [icon.heightAnchor constraintEqualToConstant:18],
      [text.leadingAnchor constraintEqualToAnchor:icon.trailingAnchor constant:6],
      [text.trailingAnchor constraintEqualToAnchor:cell.trailingAnchor constant:-6],
      [text.centerYAnchor constraintEqualToAnchor:cell.centerYAnchor]
    ]];
    cell.imageView = icon;
    cell.textField = text;
  }
  QString title = command->label;
  if (!command->shortcut.isEmpty()) title += QStringLiteral("   ") + QKeySequence(command->shortcut).toString(QKeySequence::NativeText);
  cell.textField.stringValue = ns(title);
  cell.textField.textColor = command->enabled ? NSColor.labelColor : NSColor.tertiaryLabelColor;
  // The same artwork as the ribbon and menus: coloured CAD icons follow the
  // palette's light or dark rendering, masks stay templates that AppKit tints
  // for the appearance and the selected row. A command without artwork keeps
  // the slot empty, so the names stay aligned; one that cannot run is dimmed.
  const bool artwork = !command->icon.isNull() || !command->symbol.isEmpty();
  cell.imageView.image = artwork ? state->actionImage(*command, 18) : nil;
  cell.imageView.alphaValue = command->enabled ? 1.0 : 0.4;
  cell.toolTip = ns(command->tooltip);
  cell.accessibilityIdentifier = ns(QStringLiteral("native.command.") + command->id);
  return cell;
}
- (BOOL)tableView:(NSTableView*)table shouldSelectRow:(NSInteger)row {
  if (state && table == state->searchTable && state->timelineSearch) return row >= 0 && row < state->timelineResults.size();
  return state && table == state->searchTable && row >= 0 && row < state->searchResults.size() &&
         state->actions[state->searchResults[int(row)]].enabled;
}
- (void)clickHistoryFeature:(id)sender {
  if ([sender isKindOfClass:MITCADHistoryFeatureButton.class]) [(MITCADHistoryFeatureButton*)sender selectHistoryFeature];
}
- (void)stepHistory:(id)sender {
  if (!state || ![sender isKindOfClass:NSButton.class]) return;
  const NSInteger step = [sender tag];
  state->requestHistoryMarker(step == 0 ? 0 : step == 3 ? int(state->historyItems.size()) :
                             state->historyStep(step == 1 ? -1 : 1));
}
- (void)openHistoryFilter:(id)sender {
  if (!state || state->nativeModalBlocked() || state->historyMarkerDragging || !state->callbacks.historyFilterMenu) return;
  state->callbacks.historyFilterMenu(menuPoint((NSView*)sender));
}
- (void)openProjectMenu:(id)sender {
  if (!state || state->nativeModalBlocked() || !state->callbacks.projectMenu) return;
  state->callbacks.projectMenu(menuPoint((NSView*)sender));
}
- (void)clickAgentIndicator:(id)sender {
  if (!state || state->nativeModalBlocked()) return;
  // A Control-click is a right click.
  if ((NSApp.currentEvent.modifierFlags & NSEventModifierFlagControl) != 0) {
    [self openAgentMenu:sender];
    return;
  }
  if (state->callbacks.agentClicked) state->callbacks.agentClicked(menuPoint((NSView*)sender));
}
- (void)openAgentMenu:(id)sender {
  if (!state || state->nativeModalBlocked() || !state->callbacks.agentMenu) return;
  state->callbacks.agentMenu(menuPoint((NSView*)sender));
}
- (void)openQuickAccessMenu:(id)sender {
  if (!state || state->nativeModalBlocked() || ![sender isKindOfClass:NSView.class]) return;
  NSView* view = (NSView*)sender;
  [state->quickMenu() popUpMenuPositioningItem:nil atLocation:NSMakePoint(0, view.bounds.size.height + 4) inView:view];
}
- (void)chooseQuickAccess:(id)sender {
  if (!state || !state->callbacks.quickAccessChosen || ![sender isKindOfClass:NSMenuItem.class]) return;
  NSMenuItem* item = sender;
  const bool on = item.state != NSControlStateValueOn;
  [self performSelector:@selector(applyQuickAccess:) withObject:@[item.representedObject, @(on)] afterDelay:0];
}
- (void)resetQuickAccess:(id)sender {
  (void)sender;
  if (!state || !state->callbacks.quickAccessReset) return;
  [self performSelector:@selector(applyQuickAccess:) withObject:@[] afterDelay:0];
}
- (void)hideQuickAccess:(id)sender {
  (void)sender;
  if (!state || !state->callbacks.quickAccessHidden) return;
  [self performSelector:@selector(applyQuickAccess:) withObject:@[@"hide"] afterDelay:0];
}
- (void)applyQuickAccess:(NSArray*)choice {
  if (!state || state->closing) return;
  if (choice.count == 2 && state->callbacks.quickAccessChosen) {
    state->callbacks.quickAccessChosen(qt(choice[0]), [choice[1] boolValue]);
  } else if (choice.count == 1 && state->callbacks.quickAccessHidden) {
    state->callbacks.quickAccessHidden();
  } else if (choice.count == 0 && state->callbacks.quickAccessReset) {
    state->callbacks.quickAccessReset();
  }
}
- (void)chooseSearch:(id)sender {
  (void)sender;
  if (!state) return;
  NSInteger row = state->searchTable.selectedRow;
  if (state->timelineSearch) {
    if (row >= 0 && row < state->timelineResults.size() && state->callbacks.timelineSearchSelected)
      state->callbacks.timelineSearchSelected(int(row));
    [state->searchField.window makeFirstResponder:state->searchField];
    return;
  }
  // Command search keeps its existing double-click/Return activation.
  if ([sender isKindOfClass:NSTableView.class] && NSApp.currentEvent.clickCount < 2) return;
  if (row < 0 || row >= state->searchResults.size()) return;
  const auto& command = state->actions[state->searchResults[int(row)]];
  if (!command.enabled) return;
  const QString id = command.id;
  [state->searchPopover close];
  state->invoke(id);
}
- (void)openTimelineSearchFilter:(id)sender {
  if (state && state->timelineSearch && state->callbacks.timelineSearchFilterMenu)
    state->callbacks.timelineSearchFilterMenu(menuPoint((NSView*)sender));
  if (state && state->searchPopover.shown) [state->searchField.window makeFirstResponder:state->searchField];
}
- (void)stepTimelineSearch:(id)sender {
  if (state && state->timelineSearch && state->callbacks.timelineSearchStepped)
    state->callbacks.timelineSearchStepped(int([sender tag]));
  if (state) [state->searchField.window makeFirstResponder:state->searchField];
}
- (void)popoverDidClose:(NSNotification*)notification {
  if (!state || notification.object != state->searchPopover || !state->timelineSearch) return;
  state->timelineSearch = false;
  state->timelineMatches.clear();
  state->updateHistoryAppearance();
  if (!state->closing && state->callbacks.timelineSearchClosed) state->callbacks.timelineSearchClosed();
}
- (void)searchChanged:(id)sender { (void)sender; if (state) state->fillSearch(); }
- (void)controlTextDidChange:(NSNotification*)notification {
  if (state && notification.object == state->searchField) state->fillSearch();
}
- (void)controlTextDidEndEditing:(NSNotification*)notification {
  if (!state || notification.object == state->searchField || !state->callbacks.rename) return;
  NSTextField* field = notification.object;
  const auto* item = state->treeItem(field.identifier);
  if (item && item->editable && qt(field.stringValue) != item->title) {
    const QString key = item->id;
    const QString text = qt(field.stringValue).trimmed();
    if (!text.isEmpty()) state->callbacks.rename(key, text);
    else field.stringValue = ns(item->title);
  }
}
- (BOOL)control:(NSControl*)control textView:(NSTextView*)textView doCommandBySelector:(SEL)selector {
  (void)textView;
  if (!state) return NO;
  if (control != state->searchField) {
    if (selector == @selector(cancelOperation:) && [control isKindOfClass:NSTextField.class]) {
      const auto* item = state->treeItem(control.identifier);
      if (item) {
        ((NSTextField*)control).stringValue = ns(item->title);
        [control.window makeFirstResponder:state->tree];
        return YES;
      }
    }
    return NO;
  }
  if (selector == @selector(cancelOperation:)) { [state->searchPopover close]; return YES; }
  if (state->timelineSearch && (selector == @selector(insertNewline:) || selector == @selector(moveDown:) ||
                                selector == @selector(moveUp:) || selector == @selector(insertNewlineIgnoringFieldEditor:))) {
    const bool previous = selector == @selector(moveUp:) || (NSApp.currentEvent.modifierFlags & NSEventModifierFlagShift);
    if (state->callbacks.timelineSearchStepped) state->callbacks.timelineSearchStepped(previous ? -1 : 1);
    return YES;
  }
  if (selector == @selector(insertNewline:)) { [self chooseSearch:control]; return YES; }
  if (selector == @selector(moveDown:) || selector == @selector(moveUp:)) {
    const int step = selector == @selector(moveDown:) ? 1 : -1;
    const NSInteger selected = state->searchTable.selectedRow;
    for (NSInteger row = selected + step; row >= 0 && row < state->searchResults.size(); row += step) {
      if (state->actions[state->searchResults[int(row)]].enabled) {
        [state->searchTable selectRowIndexes:[NSIndexSet indexSetWithIndex:NSUInteger(row)] byExtendingSelection:NO];
        [state->searchTable scrollRowToVisible:row];
        break;
      }
    }
    return YES;
  }
  return NO;
}
@end
