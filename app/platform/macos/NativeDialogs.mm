// SPDX-License-Identifier: MIT
#include "NativeKeys.hpp"
#include "NativeTableCells.hpp"
#include "NativeWindowFit.hpp"
#include "NativeCollection.hpp"
#include "NativeRuleCanvas.hpp"
#include "NativeNavigationGuide.hpp"
#include "NativeLayout.hpp"
#include "../NativeDialogs.hpp"
#include "../NativeStructure.hpp"
#include "NativeControls.hpp"
#include "NativeColorField.hpp"

#import <AppKit/AppKit.h>
#import <UniformTypeIdentifiers/UniformTypeIdentifiers.h>

#include <algorithm>
#include <cmath>
#include <functional>
#include <memory>
#include <vector>

#include <QAbstractButton>
#include <QAbstractItemView>
#include <QAbstractSpinBox>
#include <QAction>
#include <QApplication>
#include <QBoxLayout>
#include <QBuffer>
#include <QCheckBox>
#include <QComboBox>
#include <QCoreApplication>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDoubleSpinBox>
#include <QDir>
#include <QEnterEvent>
#include <QEventLoop>
#include <QFileDialog>
#include <QFileInfo>
#include <QFormLayout>
#include <QFont>
#include <QFontMetrics>
#include <QFocusEvent>
#include <QFrame>
#include <QGridLayout>
#include <QGroupBox>
#include <QHash>
#include <QHeaderView>
#include <QIcon>
#include <QItemSelectionModel>
#include <QKeyEvent>
#include <QKeySequence>
#include <QShortcut>
#include <QSet>
#include <QKeySequenceEdit>
#include <QLabel>
#include <QLineEdit>
#include <QListView>
#include <QListWidget>
#include <QMenu>
#include <QMessageBox>
#include <QPersistentModelIndex>
#include <QPlainTextEdit>
#include <QPointer>
#include <QProgressBar>
#include <QProgressDialog>
#include <QPushButton>
#include <QRadioButton>
#include <QRegularExpression>
#include <QScrollArea>
#include <QScrollBar>
#include <QShowEvent>
#include <QSignalBlocker>
#include <QSlider>
#include <QSpinBox>
#include <QSplitter>
#include <QStackedWidget>
#include <QTabWidget>
#include <QTabBar>
#include <QTableView>
#include <QTextDocument>
#include <QTextCursor>
#include <QTextBlock>
#include <QTextFragment>
#include <QTextEdit>
#include <QTimer>
#include <QToolButton>
#include <QToolBar>
#include <QTreeView>
#include <QValidator>
#include <QWidget>
#include <QtLogging>

#include "../../report/ReportDialogs.hpp"
#include "../../framework/SettingsWindow.hpp"

namespace mitcad {
class AppKitWidgetView;
class NativeDialogController;
class NativeFilePanel;
class NativeColorPanel;
class NativeMenuPopup;
}

@interface MitcadNativeMenuTarget : NSObject <NSMenuDelegate, NSMenuItemValidation> {
@public
  mitcad::NativeMenuPopup* owner;
}
- (void)perform:(NSMenuItem*)sender;
@end

@interface MitcadNativeModelNode : NSObject {
@public
  QPersistentModelIndex index;
  QString identifier; // NativeControlIdentifierRole, which outlives a model reset
  NSMutableArray<MitcadNativeModelNode*>* children;
}
@end
@implementation MitcadNativeModelNode
@end

@interface MitcadNativeComboItem : NSObject {
@public
  QPersistentModelIndex index;
}
@end
@implementation MitcadNativeComboItem
@end

@interface MitcadNativeAlertDetails : NSObject {
@public
  NSAlert* alert;
  NSView* details;
  NSButton* disclosure;
  NSString* showTitle;
  NSString* hideTitle;
}
- (void)toggle:(id)sender;
@end

@interface MitcadNativeWidgetTarget : NSObject <NSTextFieldDelegate, NSComboBoxDelegate, NSTextViewDelegate, NSTabViewDelegate,
                                               NSTableViewDataSource, NSTableViewDelegate,
                                               NSOutlineViewDataSource, NSOutlineViewDelegate> {
@public
  mitcad::AppKitWidgetView* owner;
}
- (void)perform:(id)sender;
- (void)doubleClick:(id)sender;
@end

@interface MitcadNativeModelCell : NSTableCellView <NSTextFieldDelegate> {
@public
  mitcad::AppKitWidgetView* owner;
  int binding;
  QPersistentModelIndex index;
  // The shared roles' text colour and fill outside an emphasized selection
  // (nil: the defaults), and whether an edit is being discarded.
  NSColor* tone;
  NSColor* fill;
  BOOL discarding;
  BOOL finishing; // Return, Escape or Tab ends the edit (finishEditing:, modelEditNext)
  BOOL userEdited; // a real text change, rather than a model refresh behind the editor
  // The row's stable NativeControlIdentifierRole (its first column's), which
  // finds the row again after the model rebuilt it (mitcad#814).
  QString rowIdentifier;
}
- (void)checked:(id)sender;
// Ends this cell's text edit: commits it or discards it, keeping the table
// focused. NO when the cell is not being edited.
- (BOOL)finishEditing:(BOOL)commit;
- (void)refreshUnchangedEdit;
- (void)applyTone;
@end

// Editable fields publish their cell's selection before AppKit starts
// the field editor and consumes the table's normal pointer selection.
@interface MitcadNativeModelField : NSTextField
@end

// Only opted-in models forward hover. Persistent indices become invalid
// when an asynchronous preview replaces the checklist's rows.
@interface MitcadNativeModelHoverButton : NSButton {
@public
  QPointer<QAbstractItemView> presenter;
  QPersistentModelIndex modelIndex;
  NSTrackingArea* hoverArea;
}
- (void)publishHover:(BOOL)entered;
@end

@interface MitcadNativeWindowTarget : NSObject <NSWindowDelegate, MitcadNativeShortcutTarget> {
@public
  mitcad::NativeDialogController* owner;
}
@end

@interface MitcadNativeSecondaryWindow : NSWindow
- (BOOL)performKeyEquivalent:(NSEvent*)event;
- (void)cancelOperation:(id)sender;
@end

@interface MitcadNativeFilePanelTarget : NSObject <NSOpenSavePanelDelegate> {
@public
  mitcad::NativeFilePanel* owner;
}
- (void)filterChanged:(id)sender;
@end

@interface MitcadNativeColorPanelTarget : NSObject <NSWindowDelegate> {
@public
  mitcad::NativeColorPanel* owner;
}
- (void)accept:(id)sender;
- (void)cancel:(id)sender;
@end

// The shared view's hint while it has no rows (NativeEmptyTextProperty),
// a label over the table's rows area that the table keeps in place.
@protocol MitcadNativeEmptyText
@property(nonatomic, strong) NSTextField* emptyLabel;
@end
@interface MitcadNativeTable : NSTableView <MitcadNativeEmptyText>
- (NSInteger)presenterSelectedRows;
@end
@interface MitcadNativeOutline : NSOutlineView <MitcadNativeEmptyText>
- (NSInteger)presenterSelectedRows;
@end
@interface MitcadNativeSpannedRowView : MitcadNativeCurrentRowView
@end

@interface MitcadNativeShortcutField : NSTextField {
@public
  QPointer<QKeySequenceEdit> edit;
}
- (void)clearShortcut:(id)sender;
@end

@interface MitcadNativeSlider : NSSlider
@property(nonatomic) NSInteger keyboardStep;
@property(nonatomic) BOOL pointerTracking;
@end
@interface MitcadNativePopup : NSPopUpButton
// The shared combo's placeholderText, drawn while no item is selected
// (mitcad#976), as a QComboBox shows it at currentIndex -1.
@property(nonatomic, copy) NSString* placeholderString;
@end

@interface MitcadNativeActionButton : NSButton
@end

// The Settings section selector (mitcad#971): each pane's shared icon above
// its name, as the shared header shows them. A click selects the page in the
// tab view, whose delegate shows the shared pane; sync follows the tab view.
@interface MitcadNativePaneSelector : NSStackView
@property(nonatomic, weak) NSTabView* pages;
- (void)choose:(NSButton*)sender;
- (void)sync;
@end

@interface MitcadNativePaneCell : NSButtonCell
@end

@interface MitcadNativePaneButton : NSButton
@property(nonatomic) BOOL chosen;
@end

@interface MitcadNativeCompletionTable : NSTableView
- (QListWidget*)sharedList;
- (NSInteger)presenterSelectedRows;
- (NSInteger)presenterSelectedRow;
@end

@interface MitcadNativeTextView : NSTextView <NSTableViewDataSource, NSTableViewDelegate> {
  NSPopover* completionPopover;
  NSTableView* completionTable;
  id completionMonitor;
}
@property(nonatomic, copy) NSString* placeholderString;
- (BOOL)sharedKey:(NSEvent*)event;
- (mitcad::AppKitWidgetView*)presenterOwner;
- (QListWidget*)sharedCompletions;
- (void)refreshCompletions;
- (void)closeCompletions;
- (void)chooseCompletion:(id)sender;
@end

// Opt-in hover forwarding for presenters that preview a button's candidates.
@interface MitcadNativeHoverButton : MitcadNativeActionButton {
@public
  QPointer<QAbstractButton> presenter;
  NSTrackingArea* hoverArea;
}
@end

@interface MitcadNativeScreenshot : NSView {
@public
  QPointer<mitcad::report::ScreenshotView> presenter;
  NSImage* bitmap;
  qint64 imageKey;
  NSPoint from;
  NSPoint to;
  BOOL dragging;
  QRect drawnCrop;
}
- (void)refresh;
@end

namespace mitcad {
namespace {

__weak NSWindow* g_documentWindow = nil;
std::vector<NSWindow*> g_modalWindows;
quint64 g_widgetPropertyWrites = 0;

NSString* ns(const QString& text) {
  const QByteArray utf8 = text.toUtf8();
  return [[NSString alloc] initWithBytes:utf8.constData() length:static_cast<NSUInteger>(utf8.size())
                               encoding:NSUTF8StringEncoding];
}
QString qt(NSString* text) { return text == nil ? QString() : QString::fromUtf8(text.UTF8String); }
// Compare UTF-16 in place. Converting an unchanged QString to NSString on
// every safety-net pass would keep allocating even with guarded setters.
bool sameText(NSString* current, const QString& value) {
  if (current.length != static_cast<NSUInteger>(value.size())) { return false; }
  for (qsizetype i = 0; i < value.size(); ++i) {
    if ([current characterAtIndex:static_cast<NSUInteger>(i)] != value.at(i).unicode()) { return false; }
  }
  return true;
}
#define NATIVE_SET(object, property, value) do { \
  const auto desired = (value); \
  if ((object).property != desired) { (object).property = desired; ++g_widgetPropertyWrites; } \
} while (false)
#define NATIVE_STRING(object, property, value) do { \
  const QString desiredText = (value); \
  if (!sameText((object).property, desiredText)) { (object).property = ns(desiredText); ++g_widgetPropertyWrites; } \
} while (false)
QString title(QString text) {
  text.replace(QStringLiteral("&&"), QStringLiteral("\x01"));
  text.remove(QLatin1Char('&'));
  text.replace(QChar(1), QLatin1Char('&'));
  return text;
}
QString plain(const QString& text) {
  if (!Qt::mightBeRichText(text)) {
    return text;
  }
  QTextDocument document;
  document.setHtml(text);
  return document.toPlainText();
}
bool richLabelText(const QLabel* caption) {
  return caption->textFormat() == Qt::RichText ||
         (caption->textFormat() == Qt::AutoText && Qt::mightBeRichText(caption->text()));
}
QString labelText(const QLabel* caption) {
  QString text = caption->text();
  if (richLabelText(caption)) {
    QTextDocument document;
    document.setHtml(text);
    text = document.toPlainText();
  }
  return caption->buddy() != nullptr ? title(text) : text;
}
CGFloat labelPixmapHeight(const QLabel* caption) {
  // Labels also carry compact swatches and warning icons. Keep their
  // logical image size instead of giving every picture a preview's height.
  const QPixmap pixmap = caption->pixmap();
  return std::min<CGFloat>(240, std::max<CGFloat>(caption->minimumHeight(),
                                                std::ceil(pixmap.deviceIndependentSize().height())));
}
NSImage* image(const QPixmap& pixmap) {
  if (pixmap.isNull()) {
    return nil;
  }
  QByteArray data;
  QBuffer buffer(&data);
  buffer.open(QIODevice::WriteOnly);
  pixmap.save(&buffer, "PNG");
  return [[NSImage alloc] initWithData:[NSData dataWithBytes:data.constData()
                                                    length:static_cast<NSUInteger>(data.size())]];
}
// A Settings pane's icon at the shared header's 24 points, drawn at twice
// that for Retina screens. The header paints these icons as masks in its
// text or accent colour, so the image is a template: AppKit tints it for the
// appearance and the selection (mitcad#971).
NSImage* paneImage(const QIcon& icon, const QString& name) {
  constexpr int points = 24;
  NSImage* result = icon.isNull() ? nil : image(icon.pixmap(QSize(points, points), 2.0));
  if (result == nil) {
    return nil;
  }
  result.size = NSMakeSize(points, points);
  [result setTemplate:YES];
  result.accessibilityDescription = name.toNSString();
  return result;
}
NSStackView* stack(bool horizontal = false) {
  NSStackView* result = [[NSStackView alloc] initWithFrame:NSZeroRect];
  result.orientation = horizontal ? NSUserInterfaceLayoutOrientationHorizontal : NSUserInterfaceLayoutOrientationVertical;
  result.alignment = horizontal ? NSLayoutAttributeCenterY : NSLayoutAttributeLeading;
  result.spacing = 8;
  result.detachesHiddenViews = YES;
  result.translatesAutoresizingMaskIntoConstraints = NO;
  return result;
}
NSTextField* label(const QString& text, bool interpretRichText = true) {
  NSTextField* result = [NSTextField wrappingLabelWithString:ns(interpretRichText ? plain(text) : text)];
  result.font = [NSFont systemFontOfSize:NSFont.systemFontSize];
  result.selectable = YES;
  result.translatesAutoresizingMaskIntoConstraints = NO;
  [result setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow
                                   forOrientation:NSLayoutConstraintOrientationHorizontal];
  return result;
}
// One scrolling line, as Qt's line edit and spin box editor. A text field
// made in code wraps: a narrow one moved a spin box's unit suffix (" mm")
// and a line edit's trailing words onto an unseen second line (mitcad#975,
// mitcad#982).
void singleScrollingLine(NSTextField* field) {
  field.usesSingleLineMode = YES;
  field.cell.wraps = NO;
  field.cell.scrollable = YES;
}
NSFont* labelFont(const QFont& font, const QFont& reference) {
  CGFloat size = NSFont.systemFontSize;
  if (font.pointSizeF() > 0 && reference.pointSizeF() > 0)
    size *= font.pointSizeF() / reference.pointSizeF();
  const NSFontWeight weight = font.weight() >= QFont::Bold ? NSFontWeightBold
      : font.weight() >= QFont::DemiBold ? NSFontWeightSemibold
      : font.weight() >= QFont::Medium ? NSFontWeightMedium : NSFontWeightRegular;
  NSFont* native = [NSFont systemFontOfSize:size weight:weight];
  if (font.italic()) native = [NSFontManager.sharedFontManager convertFont:native toHaveTrait:NSItalicFontMask];
  return native;
}
void add(NSStackView* parent, NSView* child) {
  if (child == nil) {
    return;
  }
  child.translatesAutoresizingMaskIntoConstraints = NO;
  [parent addArrangedSubview:child];
  if (parent.orientation == NSUserInterfaceLayoutOrientationVertical) {
    [child.leadingAnchor constraintEqualToAnchor:parent.leadingAnchor constant:parent.edgeInsets.left].active = YES;
    [child.trailingAnchor constraintEqualToAnchor:parent.trailingAnchor constant:-parent.edgeInsets.right].active = YES;
  }
}
NSScrollView* scroll(NSView* document, CGFloat height = 160, bool horizontal = false) {
  NSScrollView* view = [[NSScrollView alloc] initWithFrame:NSMakeRect(0, 0, 420, height)];
  view.borderType = NSBezelBorder;
  view.hasVerticalScroller = YES;
  view.hasHorizontalScroller = YES;
  view.autohidesScrollers = YES;
  view.documentView = document;
  if ([document isKindOfClass:NSStackView.class] || [document isKindOfClass:MitcadNativeLayoutView.class]) {
    document.translatesAutoresizingMaskIntoConstraints = NO;
    [document.leadingAnchor constraintEqualToAnchor:view.contentView.leadingAnchor].active = YES;
    [document.topAnchor constraintEqualToAnchor:view.contentView.topAnchor].active = YES;
    NSLayoutConstraint* width = [document.widthAnchor constraintEqualToAnchor:view.contentView.widthAnchor];
    // A scrollable source keeps its document's minimum independently of
    // the viewport, so a wide form can scroll instead of widening a window.
    if (horizontal) {
      width.priority = 499;
      [document.widthAnchor constraintGreaterThanOrEqualToAnchor:view.contentView.widthAnchor].active = YES;
    }
    width.active = YES;
    [document.heightAnchor constraintGreaterThanOrEqualToAnchor:view.contentView.heightAnchor].active = YES;
    view.hasHorizontalScroller = horizontal;
  }
  view.translatesAutoresizingMaskIntoConstraints = NO;
  [view.heightAnchor constraintGreaterThanOrEqualToConstant:height].active = YES;
  return view;
}
NSWindow* currentHost(QWidget* widget);

void pumpNativeEvents(const std::function<bool()>& complete) {
  // A nested QEventLoop::exec can transfer control to NSApplication::run
  // in Qt's Cocoa dispatcher. Explicit bounded flushes keep Qt timers,
  // worker completions and AppKit input progressing together.
  while (!complete()) {
    @autoreleasepool {
      QCoreApplication::processEvents(QEventLoop::AllEvents);
      QCoreApplication::sendPostedEvents(nullptr, QEvent::DeferredDelete);
      if (complete()) { break; }
      NSEvent* event = [NSApp nextEventMatchingMask:NSEventMaskAny
                                        untilDate:[NSDate dateWithTimeIntervalSinceNow:0.01]
                                           inMode:NSDefaultRunLoopMode dequeue:YES];
      if (event != nil) { [NSApp sendEvent:event]; }
      [NSApp updateWindows];
    }
  }
}

// A tool button with its text under its icon is a tile (MitcadNativeTileButton,
// mitcad#815: the selection card's Create row), with its own icon size.
bool tileButton(const QAbstractButton* button) {
  const auto* tool = qobject_cast<const QToolButton*>(button);
  return tool != nullptr && tool->toolButtonStyle() == Qt::ToolButtonTextUnderIcon &&
         !button->property("nativeHoverEvents").toBool();
}

int buttonIconPixels(const QAbstractButton* button) {
  return tileButton(button) ? std::max(16, button->iconSize().width()) : 18;
}

void updateButtonPresentation(NSButton* native, QAbstractButton* button) {
  const auto* tool = qobject_cast<QToolButton*>(button);
  const auto style = tool == nullptr ? Qt::ToolButtonTextBesideIcon : tool->toolButtonStyle();
  const bool iconOnly = style == Qt::ToolButtonIconOnly && !button->icon().isNull();
  const bool textOnly = style == Qt::ToolButtonTextOnly;
  NATIVE_STRING(native, title, iconOnly ? QString() : title(button->text()));
  NATIVE_SET(native, imagePosition, textOnly ? NSNoImage : iconOnly || button->text().isEmpty() ? NSImageOnly
                                            : style == Qt::ToolButtonTextUnderIcon ? NSImageAbove : NSImageLeading);
  // A tile's caption may break its name over two lines; it is one name.
  const QString name = tileButton(button) && !button->accessibleName().isEmpty() ? button->accessibleName()
                       : button->text().isEmpty() ? button->toolTip() : button->text();
  NATIVE_STRING(native, accessibilityLabel, plain(title(name)));
  NATIVE_STRING(native, toolTip, plain(button->toolTip().isEmpty() ? button->text() : button->toolTip()));
}

void updateColorField(NSButton* native, QAbstractButton* button) {
  if (![native isKindOfClass:MitcadNativeColorField.class]) return;
  auto* field = (MitcadNativeColorField*)native;
  const QColor color = button->property(NativeColorFieldProperty).value<QColor>();
  NSColor* swatch = color.isValid()
      ? [NSColor colorWithSRGBRed:color.redF() green:color.greenF() blue:color.blueF() alpha:color.alphaF()] : nil;
  const QString words = button->property(NativeColorFieldTextProperty).toString();
  const bool placeholder = button->property(NativeColorFieldPlaceholderProperty).toBool();
  if ((field.swatchColor != swatch && ![field.swatchColor isEqual:swatch]) ||
      !sameText(field.words, words) || field.placeholder != placeholder) {
    field.swatchColor = swatch;
    field.words = ns(words);
    field.placeholder = placeholder;
    field.needsDisplay = YES;
    ++g_widgetPropertyWrites;
  }
  NATIVE_STRING(field, accessibilityLabel, button->accessibleName());
}

// Presenters whose native outline moves rows by dragging (mitcad#816).
QHash<const QAbstractItemView*, NativeRowMoves>& rowMovesRegistry() {
  static QHash<const QAbstractItemView*, NativeRowMoves> moves;
  return moves;
}
NSPasteboardType const kNativeRowDragType = @"org.mitcad.native-row";

// A widget the bridge presents as nothing: no control it maps, no layout
// and no child widgets. A plain QWidget or QFrame is a spacer or an empty
// box; any other class (one that paints itself, a Qt control the bridge
// does not map) loses what it shows. Logged once per class and name; the
// native UI tests fail on the line unless native-coverage.txt marks the
// class pending (docs/native-macos-ui.md, Coverage).
void reportUnpresented(const QWidget* widget) {
  const QMetaObject* meta = widget->metaObject();
  if (meta == &QWidget::staticMetaObject || meta == &QFrame::staticMetaObject) {
    return;
  }
  static QSet<QString> reported;
  const QString className = QString::fromLatin1(meta->className());
  if (reported.contains(className + QLatin1Char('/') + widget->objectName())) {
    return;
  }
  reported.insert(className + QLatin1Char('/') + widget->objectName());
  const QWidget* window = widget->window();
  qWarning().noquote() << QStringLiteral("Native presenter: unpresented widget %1 '%2' in %3 '%4'")
                              .arg(className, widget->objectName(),
                                   QString::fromLatin1(window->metaObject()->className()), window->windowTitle());
}

// An empty view's hint where the shared view paints it: 12 points inside
// the visible part of the rows, wrapped to its width.
void placeEmptyLabel(NSTableView* table, NSTextField* hint) {
  if (hint == nil || hint.hidden) return;
  const NSRect visible = table.enclosingScrollView != nil ? table.enclosingScrollView.contentView.bounds : table.bounds;
  const CGFloat width = std::max<CGFloat>(40, NSWidth(visible) - 24);
  hint.preferredMaxLayoutWidth = width;
  const NSSize size = [hint.cell cellSizeForBounds:NSMakeRect(0, 0, width, CGFLOAT_MAX)];
  const NSRect frame = NSMakeRect(NSMinX(visible) + 12, 12, width, std::ceil(size.height));
  if (!NSEqualRects(hint.frame, frame)) hint.frame = frame;
}

} // namespace

// A converted control's real AppKit position, kept only while its shared
// presenter action runs. Nested menu actions restore the previous anchor.
class NativeMenuAnchor final {
public:
  NativeMenuAnchor(QWidget* presenter, NSControl* control) {
    if (presenter == nullptr || control.window == nil) return;
    m_window = presenter->window();
    m_before = m_window->property("nativeMenuPosition");
    const NSRect bounds = control.bounds;
    const NSPoint bottom = NSMakePoint(NSMinX(bounds), control.flipped ? NSMaxY(bounds) : NSMinY(bounds));
    const NSPoint screen = [control.window convertPointToScreen:[control convertPoint:bottom toView:nil]];
    m_point = QPoint(qRound(screen.x), qRound(NSMaxY(NSScreen.screens.firstObject.frame) - screen.y));
    m_window->setProperty("nativeMenuPosition", m_point);
  }
  ~NativeMenuAnchor() { if (m_window) m_window->setProperty("nativeMenuPosition", m_before); }
  QPoint point(const QPoint& fallback) const { return m_window ? m_point : fallback; }
private:
  QPointer<QWidget> m_window;
  QVariant m_before;
  QPoint m_point;
};

class AppKitWidgetView final : public QObject, public NativeWidgetView {
public:
  explicit AppKitWidgetView(QWidget* source, bool compact = false) : m_source(source) {
    m_target = [MitcadNativeWidgetTarget new];
    m_target->owner = this;
    m_root = stack();
    m_root.edgeInsets = compact || source->property("nativeCompactPanel").toBool()
                            ? NSEdgeInsetsMake(0, 0, 0, 0) : NSEdgeInsetsMake(16, 16, 16, 16);
    if (source->property("nativeCompactPanel").toBool()) m_root.distribution = NSStackViewDistributionFill;
    m_root.accessibilityIdentifier = ns(compact ? QStringLiteral("native.cell.") + source->objectName()
                                               : source->objectName());
    connect(source, &QObject::destroyed, this, [this] {
      // Hiding ends AppKit field editing. The presenter is already being
      // destroyed: retire its callbacks before that can emit editingFinished.
      retire();
      m_root.hidden = YES;
    });
    m_refreshTimer.setSingleShot(true);
    connect(&m_refreshTimer, &QTimer::timeout, this, [this] { refreshPresenters(); });
    rebuild();
    // Qt has no notify signal for several presenter properties (label and
    // button text, spin ranges, read-only state, model replacement, row
    // hiding and index widgets). Only those gaps need a slow safety net.
    m_timer.setInterval(500);
    connect(&m_timer, &QTimer::timeout, this, [this] { checkModelPresentation(); refreshPresenters(); });
    m_timer.start();
  }
  ~AppKitWidgetView() override { retire(); }
  void retire() {
    m_timer.stop();
    m_refreshTimer.stop();
    m_target->owner = nullptr;
    for (MitcadNativeModelCell* cell in m_cells) { cell->owner = nullptr; }
    for (auto& embedded : m_cellViews) { embedded.second->retire(); }
    detachBindings();
  }
  void detachBindings() {
    for (Binding& binding : m_bindings) {
      if ([binding.text isKindOfClass:MitcadNativeTextView.class]) [(MitcadNativeTextView*)binding.text closeCompletions];
      for (const auto& connection : binding.connections) { disconnect(connection); }
      for (const auto& connection : binding.modelConnections) { disconnect(connection); }
      for (const auto& connection : binding.selectionConnections) { disconnect(connection); }
      if ([binding.control isKindOfClass:MitcadNativeHoverButton.class]) {
        ((MitcadNativeHoverButton*)binding.control)->presenter.clear();
      }
      if (binding.control != nil) { binding.control.target = nil; binding.control.action = nullptr; }
      if (binding.stepper != nil) { binding.stepper.target = nil; binding.stepper.action = nullptr; }
      if ([binding.control isKindOfClass:NSTextField.class]) {
        ((NSTextField*)binding.control).delegate = nil;
      }
      binding.tabs.delegate = nil;
      binding.text.delegate = nil;
      binding.linkText.delegate = nil;
      if (binding.table != nil) {
        binding.table.delegate = nil;
        binding.table.dataSource = nil;
      }
    }
  }
  void* nativeView() const override { return (__bridge void*)m_root; }
  QWidget* source() const { return m_source; }
  // A secondary window keeps the presenter's trailing dialog buttons below
  // its scrolling body (NativeWindowFit.hpp), visible however tall the body
  // is: their stack, empty for a presenter without such buttons.
  NSStackView* detachFooter() {
    if (m_footer == nil) {
      m_footer = stack();
      m_footer.accessibilityIdentifier = @"native.secondary.footer";
      placeFooter();
    }
    return m_footer;
  }
  void placeFooter() {
    for (NSView* child in [m_footer.arrangedSubviews copy]) { [m_footer removeArrangedSubview:child]; [child removeFromSuperview]; }
    auto* layout = m_source != nullptr ? qobject_cast<QBoxLayout*>(m_source->layout()) : nullptr;
    QLayoutItem* last = layout != nullptr && layout->direction() == QBoxLayout::TopToBottom && layout->count() > 0
                            ? layout->itemAt(layout->count() - 1) : nullptr;
    auto* buttons = last != nullptr ? qobject_cast<QDialogButtonBox*>(last->widget()) : nullptr;
    // A closing row may also hold helpers beside the button box (Parameters'
    // stretch, the part picker's Pin, the project picker's New Project).
    // Keep that whole row outside the scrolling body (mitcad#812).
    auto* closing = last != nullptr && buttons == nullptr ? qobject_cast<QBoxLayout*>(last->layout()) : nullptr;
    if (closing != nullptr && closing->direction() != QBoxLayout::LeftToRight &&
        closing->direction() != QBoxLayout::RightToLeft) {
      closing = nullptr;
    }
    for (int i = 0; closing != nullptr && buttons == nullptr && i < closing->count(); ++i) {
      buttons = qobject_cast<QDialogButtonBox*>(closing->itemAt(i)->widget());
    }
    NSView* row = nil;
    for (const Binding& binding : m_bindings) {
      if (buttons != nullptr && binding.widget.data() == buttons) { row = binding.view; break; }
    }
    if (row != nil && closing != nullptr) {
      // The closing row is a direct item of the window's converted layout.
      NSView* line = row.superview;
      const bool nested = [line isKindOfClass:MitcadNativeLayoutView.class] &&
                          [line.superview isKindOfClass:MitcadNativeLayoutView.class];
      row = nested ? line : nil;
    }
    m_footer.edgeInsets = row != nil ? NSEdgeInsetsMake(8, 16, 16, 16) : NSEdgeInsetsMake(0, 0, 0, 0);
    m_footer.hidden = row == nil;
    if (row != nil) {
      if ([row.superview isKindOfClass:NSStackView.class]) { [(NSStackView*)row.superview removeArrangedSubview:row]; }
      [row removeFromSuperview];
      add(m_footer, row);
    }
  }
  void focusInitial() {
    QWidget* wanted = m_source != nullptr ? m_source->focusWidget() : nullptr;
    // A retained dialog is not Qt's active window, so hasFocus() cannot
    // report an explicitly requested list/table focus there. Its stored
    // focusWidget (or the table's actual focus proxy) still records it.
    for (const Binding& binding : m_bindings) {
      QWidget* widget = binding.widget;
      if (widget != nullptr && widget == wanted && binding.text != nil && binding.text.editable &&
          widget->isVisibleTo(m_source) && widget->isEnabled() && binding.text.window != nil &&
          ![binding.text isHiddenOrHasHiddenAncestor] && [binding.text.window makeFirstResponder:binding.text]) return;
      if (wanted == nullptr || widget == nullptr || binding.table == nil ||
          !widget->isVisibleTo(m_source) || !widget->isEnabled() || !binding.table.enabled ||
          binding.table.window == nil || [binding.table isHiddenOrHasHiddenAncestor]) { continue; }
      if (widget != wanted && widget->focusProxy() != wanted && wanted->focusProxy() != widget) { continue; }
      if ([binding.table.window makeFirstResponder:binding.table]) {
        if (widget->property(NativeCurrentRowNavigationProperty).toBool()) { updateCurrentRows(binding.table); }
        return;
      }
    }
    NSControl* first = nil;
    for (const Binding& binding : m_bindings) {
      if (binding.widget == nullptr || !binding.widget->isVisibleTo(m_source) || !binding.widget->isEnabled()) { continue; }
      if (binding.table != nil && binding.widget->property(NativeCurrentRowNavigationProperty).toBool()) {
        [binding.table.window makeFirstResponder:binding.table];
        updateCurrentRows(binding.table);
        return;
      }
      if ([binding.control isKindOfClass:NSTextField.class] && ((NSTextField*)binding.control).editable) {
        if (first == nil) { first = binding.control; }
        if (binding.widget->hasFocus()) { first = binding.control; break; }
      }
    }
    if (first != nil) { [first.window makeFirstResponder:first]; [(NSTextField*)first selectText:nil]; }
  }

  bool focusNextField(NSControl* sender, bool backwards) {
    const NSInteger tag = sender.tag;
    if (tag < 0 || static_cast<std::size_t>(tag) >= m_bindings.size() ||
        m_bindings[static_cast<std::size_t>(tag)].control != sender || !m_source) return false;
    const QPointer<AppKitWidgetView> alive(this);
    const QPointer<QWidget> origin(m_bindings[static_cast<std::size_t>(tag)].widget);
    if (!origin) return false;
    QSet<QWidget*> visited{origin.data()};
    QWidget* cursor = backwards ? origin->previousInFocusChain() : origin->nextInFocusChain();
    while (cursor != nullptr && !visited.contains(cursor)) {
      visited.insert(cursor);
      if (m_source->isAncestorOf(cursor) && cursor->isVisibleTo(m_source) && cursor->isEnabled()) {
        const Binding* mapped = nullptr;
        for (const Binding& binding : m_bindings) {
          QWidget* widget = binding.widget;
          if (widget == nullptr || binding.control == nil) continue;
          const auto* spin = qobject_cast<QAbstractSpinBox*>(widget);
          if (widget == cursor || widget->focusProxy() == cursor || cursor->focusProxy() == widget ||
              (spin != nullptr && spin->isAncestorOf(cursor))) { mapped = &binding; break; }
        }
        QWidget* field = mapped != nullptr ? mapped->widget.data() : cursor;
        if (field != origin && (int(field->focusPolicy()) & int(Qt::TabFocus)) != 0) {
          // Dedicated adapters keep their own key handling. Only route a
          // shared field when this owner has its actual native control.
          if (mapped == nullptr) return false;
          NSControl* control = mapped->control;
          if (field->isEnabled() && field->isVisibleTo(m_source) && control.enabled &&
              control.window == sender.window && !control.isHiddenOrHasHiddenAncestor) {
            const QPointer<QWidget> target(field);
            const QString from = origin->objectName();
            const QString to = field->objectName();
            // Establish the ordinary native link too. Responder transfer
            // ends the current field editor through its normal validation
            // and commit delegates before accepting the next responder.
            if (backwards) control.nextKeyView = sender;
            else sender.nextKeyView = control;
            const bool accepted = [sender.window makeFirstResponder:control];
            if (!alive || !origin || !target) return true;
            if (!qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT")) {
              const QString kind = [sender isKindOfClass:NSPopUpButton.class]
                  ? QStringLiteral("popup") : QStringLiteral("field");
              qInfo().noquote() << QStringLiteral("Native %1 Tab: %2 -> %3 backward=%4 accepted=%5")
                  .arg(kind, from, to).arg(backwards).arg(accepted);
            }
            return accepted;
          }
        }
      }
      cursor = backwards ? cursor->previousInFocusChain() : cursor->nextInFocusChain();
    }
    return false;
  }

  void refresh() override {
    // Explicit caller refreshes also honour silent row/index-widget changes.
    checkModelPresentation();
    refreshPresenters();
  }
  // The presenter widgets this view shows. Child windows (context menus,
  // message boxes) never are: their coming and going must not replace the
  // native controls and their keyboard focus (mitcad#816). A scroll area's
  // own children come as a set: an item view's header, viewport and scroll
  // bar containers changing places is no change (mitcad#986).
  QList<QWidget*> structureWidgets() const {
    QList<QWidget*> widgets = m_source->findChildren<QWidget*>();
    const QWidget* window = m_source->window();
    widgets.removeIf([window](const QWidget* widget) {
      if (widget->window() != window) return true;
      for (const QWidget* parent = widget->parentWidget(); parent; parent = parent->parentWidget()) {
        if (parent->property("nativeRuleArtwork").toBool()) return true;
      }
      return false;
    });
    return nativeStructureOrder(m_source, widgets);
  }
  void refreshPresenters() {
    if (m_source == nullptr || m_refreshing) {
      return;
    }
    m_refreshTimer.stop();
    m_refreshing = true;
    if (m_structureDirty) {
      const auto widgets = structureWidgets();
      bool changed = widgets.size() != static_cast<qsizetype>(m_widgetTree.size());
      for (qsizetype i = 0; !changed && i < widgets.size(); ++i) {
        changed = widgets[i] != m_widgetTree[static_cast<std::size_t>(i)];
      }
      // A rebuild would replace an edited cell: it waits (mitcad#814).
      if (changed && editingCell()) {
        mac::logNativeCellEdit("a presenter structure change waits for the edit", m_root);
      } else {
        m_structureDirty = false;
        if (changed && !qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT")) {
          qInfo().noquote() << QStringLiteral("Native presenter structure rebuild: %1 (%2 widgets, was %3)")
              .arg(m_source->window()->windowTitle()).arg(qlonglong(widgets.size()))
              .arg(qlonglong(m_widgetTree.size()));
        }
        if (changed) { rebuild(); }
      }
    }
    // Actions may replace/delete controls or models. All presenter access
    // uses QPointer; retained AppKit views never keep raw QWidget pointers.
    for (std::size_t i = 0; i < m_bindings.size(); ++i) {
      Binding& binding = m_bindings[i];
      QWidget* widget = binding.widget;
      if (widget == nullptr) {
        NATIVE_SET(binding.view, hidden, YES);
        continue;
      }
      // QToolBar may hide generated buttons when its retained Qt layout
      // overflows. The native toolbar has its own layout and uses actions.
      const auto* toolButton = qobject_cast<QToolButton*>(widget);
      const QAction* toolbarAction = toolButton && qobject_cast<QToolBar*>(widget->parentWidget())
                                         ? toolButton->defaultAction() : nullptr;
      NATIVE_SET(binding.view, hidden, toolbarAction ? !toolbarAction->isVisible()
                                                    : widget != m_source && widget->isHidden());
      if (qobject_cast<QAbstractButton*>(widget) == nullptr) { NATIVE_STRING(binding.view, toolTip, widget->toolTip()); }
      if (binding.control != nil) {
        NATIVE_SET(binding.control, enabled, widget->isEnabled());
      }
      if (qobject_cast<report::ScreenshotView*>(widget) != nullptr) {
        [(MitcadNativeScreenshot*)binding.view refresh];
      } else if (auto* edit = qobject_cast<QLineEdit*>(widget)) {
        NSTextField* field = (NSTextField*)binding.control;
        NATIVE_SET(field, editable, !edit->isReadOnly());
        NATIVE_SET(field, selectable, YES);
        NATIVE_STRING(field, placeholderString, edit->placeholderText());
        const int tone = mac::nativeTextToneKind(edit);
        if (binding.textTone != tone) {
          field.textColor = mac::nativeTextTone(tone);
          binding.textTone = tone;
          ++g_widgetPropertyWrites;
        }
        if (field.currentEditor == nil) { NATIVE_STRING(field, stringValue, edit->text()); }
        presentLineText(binding, edit);
      } else if (auto* button = qobject_cast<QAbstractButton*>(widget)) {
        NSButton* native = (NSButton*)binding.control;
        const auto* tool = qobject_cast<QToolButton*>(button);
        const int style = tool == nullptr ? int(Qt::ToolButtonTextBesideIcon) : int(tool->toolButtonStyle());
        const bool iconChanged = binding.iconKey != button->icon().cacheKey();
        if (iconChanged) {
          // AppKit can reset imagePosition while replacing the image. Apply
          // the source's final caption/icon presentation after that setter.
          native.image = button->icon().isNull() ? nil
                                                 : image(button->icon().pixmap(buttonIconPixels(button),
                                                                               buttonIconPixels(button)));
          ++g_widgetPropertyWrites;
          binding.iconKey = button->icon().cacheKey();
        }
        if (!binding.presentationInitialized || binding.labelText != button->text() ||
            binding.buttonTip != button->toolTip() || binding.buttonStyle != style ||
            iconChanged) {
          updateButtonPresentation(native, button);
          binding.labelText = button->text();
          binding.buttonTip = button->toolTip();
          binding.buttonStyle = style;
          binding.presentationInitialized = true;
        }
        updateColorField(native, button);
        auto state = button->isChecked() ? NSControlStateValueOn : NSControlStateValueOff;
        if (auto* check = qobject_cast<QCheckBox*>(widget)) {
          NATIVE_SET(native, allowsMixedState, check->isTristate());
          if (check->checkState() == Qt::PartiallyChecked) {
            state = NSControlStateValueMixed;
          }
        }
        NATIVE_SET(native, state, state);
        if (auto* push = qobject_cast<QPushButton*>(widget)) {
          if (push->isDefault()) { NATIVE_STRING(native, keyEquivalent, QStringLiteral("\r")); }
          else if ([native.keyEquivalent isEqualToString:@"\r"]) {
            // A rejecting button that stops being the default keeps Escape (mitcad#817).
            auto* box = qobject_cast<QDialogButtonBox*>(push->parentWidget());
            const bool rejecting = box != nullptr && box->buttonRole(push) == QDialogButtonBox::RejectRole;
            NATIVE_STRING(native, keyEquivalent, rejecting ? QStringLiteral("\033") : QString());
          }
        }
      } else if (auto* combo = qobject_cast<QComboBox*>(widget)) {
        observeModel(binding, combo->model(), static_cast<int>(i));
        const bool choicesDirty = binding.dirty || binding.items.size() != combo->count();
        QStringList items = binding.items;
        if (choicesDirty) {
          items.clear();
          for (int j = 0; j < combo->count(); ++j) { items.append(combo->itemText(j)); }
        }
        if (combo->isEditable()) {
          NSComboBox* native = (NSComboBox*)binding.control;
          if (items != binding.items) {
            [native removeAllItems];
            for (const QString& item : items) { [native addItemWithObjectValue:ns(item)]; }
            binding.items = items;
          }
          if (native.currentEditor == nil) { NATIVE_STRING(native, stringValue, combo->currentText()); }
        } else {
          NSPopUpButton* native = (NSPopUpButton*)binding.control;
          bool same = native.numberOfItems == combo->count();
          for (int j = 0; same && j < combo->count(); ++j) {
            NSMenuItem* item = [native itemAtIndex:j];
            MitcadNativeComboItem* value = item.representedObject;
            same = sameText(item.title, items[j]) && value != nil &&
                   value->index == combo->model()->index(j, combo->modelColumn(), combo->rootModelIndex());
          }
          if (!same) {
            [native removeAllItems];
            for (int j = 0; j < combo->count(); ++j) {
              NSMenuItem* item = [[NSMenuItem alloc] initWithTitle:ns(items[j]) action:nil keyEquivalent:@""];
              MitcadNativeComboItem* value = [MitcadNativeComboItem new];
              value->index = combo->model()->index(j, combo->modelColumn(), combo->rootModelIndex());
              item.representedObject = value;
              [native.menu addItem:item];
            }
          }
          if (combo->currentIndex() >= 0 && combo->currentIndex() < native.numberOfItems &&
              native.indexOfSelectedItem != combo->currentIndex()) {
            [native selectItemAtIndex:combo->currentIndex()];
            ++g_widgetPropertyWrites;
          } else if (combo->currentIndex() < 0 && native.selectedItem != nil) {
            [native selectItem:nil];
            native.needsDisplay = YES; // the placeholder replaces the last title
            ++g_widgetPropertyWrites;
          }
          // Unset shows the shared placeholder ("Slicer default") without an
          // item for it: the choices keep their indices (mitcad#976).
          if ([native isKindOfClass:MitcadNativePopup.class]) {
            NATIVE_STRING(((MitcadNativePopup*)native), placeholderString, combo->placeholderText());
          }
        }
        binding.items = items;
        binding.dirty = false;
      } else if (auto* text = qobject_cast<QLabel*>(widget)) {
        const QPixmap pixmap = text->pixmap();
        const int tone = mac::nativeTextToneKind(text);
        const bool links = richLabelText(text) && text->text().contains(QStringLiteral("<a"), Qt::CaseInsensitive) &&
                           (text->textInteractionFlags() & (Qt::LinksAccessibleByMouse | Qt::LinksAccessibleByKeyboard));
        if (links && binding.linkText == nil) {
          NSTextView* native = [[NSTextView alloc] initWithFrame:NSMakeRect(0, 0, 440, 55)];
          native.editable = NO;
          native.selectable = YES;
          native.drawsBackground = NO;
          native.delegate = m_target;
          native.identifier = ns(QString::number(i));
          native.autoresizingMask = NSViewWidthSizable;
          native.textContainer.widthTracksTextView = YES;
          [native.heightAnchor constraintGreaterThanOrEqualToConstant:55].active = YES;
          add((NSStackView*)binding.view, native);
          binding.linkText = native;
          binding.presentationInitialized = false;
        }
        NATIVE_SET(binding.image, hidden, pixmap.isNull());
        NATIVE_SET(binding.imageHeight, constant, labelPixmapHeight(text));
        NATIVE_SET(binding.control, hidden, links || !pixmap.isNull());
        if (binding.pixmapKey != pixmap.cacheKey()) {
          binding.image.image = image(pixmap);
          ++g_widgetPropertyWrites;
          binding.pixmapKey = pixmap.cacheKey();
        }
        if (!binding.presentationInitialized || binding.labelText != text->text() ||
            binding.labelFormat != text->textFormat() || binding.labelBuddy != (text->buddy() != nullptr) ||
            binding.labelInteraction != text->textInteractionFlags() || binding.labelFont != text->font() ||
            binding.textTone != tone) {
          binding.labelText = text->text();
          binding.labelFormat = text->textFormat();
          binding.labelBuddy = text->buddy() != nullptr;
          binding.labelInteraction = text->textInteractionFlags();
          binding.labelFont = text->font();
          binding.presentationInitialized = true;
          if (links) {
            [binding.linkText.textStorage setAttributedString:richLabel(text)];
            ++g_widgetPropertyWrites;
          } else if (richLabelText(text)) {
            ((NSTextField*)binding.control).attributedStringValue = richLabel(text);
            ++g_widgetPropertyWrites;
          } else {
            NATIVE_STRING(((NSTextField*)binding.control), stringValue, labelText(text));
            ((NSTextField*)binding.control).font = labelFont(text->font(), QApplication::font(text));
            ++g_widgetPropertyWrites;
          }
        }
        if (binding.linkText != nil) { NATIVE_SET(binding.linkText, hidden, !links || !pixmap.isNull()); }
        // The theme's semantic error/warning text, such as a refusal's
        // in-dialog reason (mitcad#814).
        // Compares the applied state, not AppKit's colour objects.
        if (binding.textTone != tone) {
          ((NSTextField*)binding.control).textColor = mac::nativeTextTone(tone);
          binding.textTone = tone;
          ++g_widgetPropertyWrites;
        }
      } else if (auto* group = qobject_cast<QGroupBox*>(widget)) {
        if (group->isCheckable() && binding.control != nil) {
          if (!binding.presentationInitialized || binding.labelText != group->title()) {
            binding.labelText = group->title();
            binding.presentationInitialized = true;
            NATIVE_STRING(((NSButton*)binding.control), title, title(group->title()));
          }
          NATIVE_SET(((NSButton*)binding.control), state, group->isChecked() ? NSControlStateValueOn : NSControlStateValueOff);
        }
      } else if (auto* box = qobject_cast<QDialogButtonBox*>(widget)) {
        // A button added, removed or given another role (mitcad#817).
        MitcadNativeLayoutView* row = (MitcadNativeLayoutView*)binding.view;
        if ([row isKindOfClass:MitcadNativeLayoutView.class] && !sameText(row.contentKey, mac::layout::buttonRoleKey(box))) {
          arrangeButtons(box, row);
          ++g_widgetPropertyWrites;
        }
      } else if (auto* spin = qobject_cast<QSpinBox*>(widget)) {
        NATIVE_SET(binding.stepper, minValue, spin->minimum());
        NATIVE_SET(binding.stepper, maxValue, spin->maximum());
        NATIVE_SET(binding.stepper, increment, spin->singleStep());
        NATIVE_SET(binding.stepper, valueWraps, spin->wrapping());
        NATIVE_SET(binding.stepper, autorepeat, YES);
        NATIVE_SET(binding.stepper, doubleValue, spin->value());
        NATIVE_SET(binding.stepper, enabled, spin->isEnabled() && !spin->isReadOnly());
        presentSpinText(binding, spin);
      } else if (auto* spin = qobject_cast<QDoubleSpinBox*>(widget)) {
        NATIVE_SET(binding.stepper, minValue, spin->minimum());
        NATIVE_SET(binding.stepper, maxValue, spin->maximum());
        NATIVE_SET(binding.stepper, increment, spin->singleStep());
        NATIVE_SET(binding.stepper, valueWraps, spin->wrapping());
        NATIVE_SET(binding.stepper, autorepeat, YES);
        NATIVE_SET(binding.stepper, doubleValue, spin->value());
        NATIVE_SET(binding.stepper, enabled, spin->isEnabled() && !spin->isReadOnly());
        presentSpinText(binding, spin);
      } else if (auto* slider = qobject_cast<QSlider*>(widget)) {
        MitcadNativeSlider* native = (MitcadNativeSlider*)binding.control;
        if (native.pointerTracking && native.integerValue != slider->value() &&
            !qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT"))
          qInfo().noquote() << QStringLiteral("Native slider tracking refresh: %1 native=%2 shared=%3")
              .arg(slider->objectName()).arg(native.integerValue).arg(slider->value());
        NATIVE_SET(native, minValue, slider->minimum());
        NATIVE_SET(native, maxValue, slider->maximum());
        NATIVE_SET(native, keyboardStep, slider->singleStep());
        NATIVE_SET(native, integerValue, slider->value());
      } else if (auto* progress = qobject_cast<QProgressBar*>(widget)) {
        const bool indeterminate = progress->minimum() == progress->maximum();
        const bool modeChanged = !binding.presentationInitialized || bool(binding.progress.indeterminate) != indeterminate;
        NATIVE_SET(binding.progress, indeterminate, indeterminate);
        NATIVE_SET(binding.progress, minValue, progress->minimum());
        NATIVE_SET(binding.progress, maxValue, progress->maximum());
        NATIVE_SET(binding.progress, doubleValue, std::clamp<double>(progress->value(), progress->minimum(), progress->maximum()));
        if (modeChanged && indeterminate) { [binding.progress startAnimation:nil]; ++g_widgetPropertyWrites; }
        else if (modeChanged) { [binding.progress stopAnimation:nil]; ++g_widgetPropertyWrites; }
        binding.presentationInitialized = true;
      } else if (auto* bar = qobject_cast<QTabBar*>(widget)) {
        NSPopUpButton* native = (NSPopUpButton*)binding.control;
        QStringList state;
        for (int j = 0; j < bar->count(); ++j) {
          state.append(QStringLiteral("%1|%2|%3|%4|%5").arg(bar->tabText(j), bar->tabToolTip(j))
                           .arg(bar->tabIcon(j).cacheKey()).arg(bar->isTabEnabled(j)).arg(bar->isTabVisible(j)));
        }
        if (state != binding.items) {
          [native removeAllItems];
          for (int j = 0; j < bar->count(); ++j) {
            [native addItemWithTitle:ns(title(bar->tabText(j)))];
            NSMenuItem* item = [native itemAtIndex:j];
            item.toolTip = ns(bar->tabToolTip(j));
            if (!bar->tabIcon(j).isNull()) item.image = image(bar->tabIcon(j).pixmap(16, 16));
            item.enabled = bar->isTabEnabled(j);
            item.hidden = !bar->isTabVisible(j);
          }
          binding.items = state;
        }
        if (bar->currentIndex() >= 0 && native.indexOfSelectedItem != bar->currentIndex())
          [native selectItemAtIndex:bar->currentIndex()];
      } else if (auto* tabs = qobject_cast<QTabWidget*>(widget)) {
        if ([binding.tabs indexOfTabViewItem:binding.tabs.selectedTabViewItem] != tabs->currentIndex()) {
          [binding.tabs selectTabViewItemAtIndex:tabs->currentIndex()];
          ++g_widgetPropertyWrites;
        }
      } else if (auto* pages = qobject_cast<QStackedWidget*>(widget)) {
        if (binding.tabs != nil && [binding.tabs indexOfTabViewItem:binding.tabs.selectedTabViewItem] != pages->currentIndex()) {
          [binding.tabs selectTabViewItemAtIndex:pages->currentIndex()];
          ++g_widgetPropertyWrites;
        }
      } else if (auto* edit = qobject_cast<QKeySequenceEdit*>(widget)) {
        if (((NSTextField*)binding.control).currentEditor == nil &&
            (!binding.presentationInitialized || binding.keySequence != edit->keySequence())) {
          binding.keySequence = edit->keySequence();
          binding.presentationInitialized = true;
          NATIVE_STRING(((NSTextField*)binding.control), stringValue, binding.keySequence.toString(QKeySequence::NativeText));
        }
      } else if (auto* text = qobject_cast<QPlainTextEdit*>(widget)) {
        NATIVE_SET(binding.text, editable, !text->isReadOnly());
        NATIVE_STRING(((MitcadNativeTextView*)binding.text), placeholderString, text->placeholderText());
        if (binding.text.window.firstResponder != binding.text &&
            (binding.document != text->document() || binding.textRevision != text->document()->revision())) {
          NATIVE_STRING(binding.text, string, text->toPlainText());
          binding.document = text->document();
          binding.textRevision = text->document()->revision();
          const QTextCursor cursor = text->textCursor();
          binding.text.selectedRange = NSMakeRange(cursor.selectionStart(), cursor.selectionEnd() - cursor.selectionStart());
        }
      } else if (auto* text = qobject_cast<QTextEdit*>(widget)) {
        NATIVE_SET(binding.text, editable, !text->isReadOnly());
        NATIVE_STRING(((MitcadNativeTextView*)binding.text), placeholderString, text->placeholderText());
        if (binding.text.window.firstResponder != binding.text &&
            (binding.document != text->document() || binding.textRevision != text->document()->revision())) {
          NATIVE_STRING(binding.text, string, text->toPlainText());
          binding.document = text->document();
          binding.textRevision = text->document()->revision();
        }
      } else if (auto* view = qobject_cast<QAbstractItemView*>(widget)) {
        refreshModel(binding, view, static_cast<int>(i));
      }
    }
    for (const auto& view : m_ruleViews) view->refresh();
    for (const auto& view : m_artworkViews) view->refresh();
    // Visibility KVO only invalidates converted layouts. Solve once after
    // every binding has its final state, before a caller measures the form.
    mac::layout::updateVisibility(m_root);
    [m_root updateConstraintsForSubtreeIfNeeded];
    for (Binding& binding : m_bindings) { followSharedScroll(binding); }
    m_refreshing = false;
    // The editor takes the keyboard before its completion list is presented:
    // the list opens only beside the editor that has it.
    if (m_textFocusRequested) {
      const QPointer<QPlainTextEdit> requested = m_textFocusRequested;
      m_textFocusRequested.clear();
      for (const Binding& binding : m_bindings) {
        if (binding.widget == requested && binding.text.window.isKeyWindow && requested->isVisibleTo(m_source)) {
          [binding.text.window makeFirstResponder:binding.text];
          break;
        }
      }
    }
    for (const Binding& binding : m_bindings) {
      if ([binding.text isKindOfClass:MitcadNativeTextView.class]) {
        [(MitcadNativeTextView*)binding.text refreshCompletions];
      }
    }
  }

  void action(id sender) {
    QPointer<AppKitWidgetView> alive(this);
    const NSInteger tag = [sender tag];
    if (tag < 0 || static_cast<std::size_t>(tag) >= m_bindings.size()) { return; }
    const Binding& binding = m_bindings[static_cast<std::size_t>(tag)];
    if (sender != binding.control && sender != binding.stepper) { return; }
    const QPointer<QWidget> pointer = m_bindings[static_cast<std::size_t>(tag)].widget;
    QWidget* widget = pointer;
    if (widget == nullptr || !widget->isEnabled()) { return; }
    bool rejecting = false;
    if (auto* button = qobject_cast<QAbstractButton*>(widget)) {
      if (auto* box = qobject_cast<QDialogButtonBox*>(button->parentWidget())) {
        rejecting = box->buttonRole(button) == QDialogButtonBox::RejectRole;
      }
    }
    NSWindow* window = [sender window];
    NSTableView* rowTable = mac::owningTable(sender);
    MitcadNativeWidgetTarget* rowTarget = rowTable != nil &&
        [rowTable.delegate isKindOfClass:MitcadNativeWidgetTarget.class] ? (MitcadNativeWidgetTarget*)rowTable.delegate : nil;
    QPointer<AppKitWidgetView> rowOwner = rowTarget != nil ? rowTarget->owner : nullptr;
    const bool rowChoice = rowOwner != nullptr && qobject_cast<QComboBox*>(widget) != nullptr &&
        rowOwner->currentRowEditorFocused(rowTable, sender, widget);
    // Accepting actions commit pending name/texture/value edits. Cancel
    // remains usable when a required field contains incomplete input.
    // Tab/mouse focus already committed the previous text editor. A slider
    // keeps its own keyboard focus while its arrow action changes a value.
    // A focused popup likewise keeps the place Tab should leave next.
    // Editable combos and actions from other controls still end text edits.
    const bool focusedSlider = qobject_cast<QSlider*>(widget) != nullptr && window.firstResponder == sender;
    const auto* comboSource = qobject_cast<QComboBox*>(widget);
    const bool focusedPopup = comboSource != nullptr && !comboSource->isEditable() &&
        [sender isKindOfClass:NSPopUpButton.class] && window.firstResponder == sender;
    if (!rejecting && !focusedSlider && !focusedPopup && window != nil && ![window makeFirstResponder:nil]) { return; }
    if (alive == nullptr || pointer == nullptr) { return; }
    const NativeMenuAnchor menuAnchor(widget, sender);
    if (auto* button = qobject_cast<QAbstractButton*>(widget)) {
      QMenu* menu = nullptr;
      if (auto* tool = qobject_cast<QToolButton*>(widget)) { menu = tool->menu(); }
      if (auto* push = qobject_cast<QPushButton*>(widget)) { menu = push->menu(); }
      if (menu != nullptr) {
        execNativeMenu(menu, menuAnchor.point(widget->mapToGlobal(QPoint(0, widget->height()))));
      } else { button->click(); }
    } else if (auto* bar = qobject_cast<QTabBar*>(widget)) {
      const int index = static_cast<int>([(NSPopUpButton*)sender indexOfSelectedItem]);
      if (index >= 0 && index < bar->count() && bar->isTabEnabled(index) && bar->isTabVisible(index))
        bar->setCurrentIndex(index);
    } else if (auto* combo = qobject_cast<QComboBox*>(widget)) {
      int index = -1;
      if (combo->isEditable()) { index = static_cast<int>([(NSComboBox*)sender indexOfSelectedItem]); }
      else {
        MitcadNativeComboItem* value = ((NSPopUpButton*)sender).selectedItem.representedObject;
        if (value != nil && value->index.isValid() && value->index.model() == combo->model() &&
            value->index.parent() == combo->rootModelIndex()) { index = value->index.row(); }
      }
      combo->setCurrentIndex(index);
      if (alive == nullptr || pointer == nullptr) { return; }
      if (combo->isEditable()) { combo->setEditText(qt([(NSComboBox*)sender stringValue])); }
      if (alive == nullptr || pointer == nullptr) { return; }
      QMetaObject::invokeMethod(combo, "activated", Qt::DirectConnection, Q_ARG(int, index));
      if (rowChoice && rowOwner != nullptr && pointer != nullptr) {
        [window makeFirstResponder:sender];
        rowOwner->updateCurrentRows(rowTable);
      }
    } else if (auto* spin = qobject_cast<QSpinBox*>(widget)) {
      spin->setValue(static_cast<int>([sender doubleValue]));
      if (alive == nullptr || pointer == nullptr) { return; }
      QMetaObject::invokeMethod(spin, "editingFinished", Qt::DirectConnection);
    } else if (auto* spin = qobject_cast<QDoubleSpinBox*>(widget)) {
      spin->setValue([sender doubleValue]);
      if (alive == nullptr || pointer == nullptr) { return; }
      QMetaObject::invokeMethod(spin, "editingFinished", Qt::DirectConnection);
    } else if (auto* slider = qobject_cast<QSlider*>(widget)) {
      auto* native = (MitcadNativeSlider*)sender;
      if (native.pointerTracking && !qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT"))
        qInfo().noquote() << QStringLiteral("Native slider tracking action: %1 native=%2 shared-before=%3")
            .arg(slider->objectName()).arg(native.integerValue).arg(slider->value());
      // Keep valueChanged observers inside the same gesture as Qt's
      // sliderReleased observers, rather than committing once for each.
      slider->setSliderDown(true);
      slider->setValue(static_cast<int>([sender integerValue]));
      if (alive == nullptr || pointer == nullptr) { return; }
      slider->setSliderDown(false);
    } else if (auto* group = qobject_cast<QGroupBox*>(widget)) {
      group->setChecked([(NSButton*)sender state] == NSControlStateValueOn);
      if (alive == nullptr || pointer == nullptr) { return; }
      QMetaObject::invokeMethod(group, "clicked", Qt::DirectConnection, Q_ARG(bool, group->isChecked()));
    }
    if (alive != nullptr && pointer != nullptr) { refresh(); }
  }

  bool commitReturn(NSControl* sender) {
    const NSInteger tag = sender.tag;
    if (tag < 0 || static_cast<std::size_t>(tag) >= m_bindings.size()) { return false; }
    QWidget* field = m_bindings[static_cast<std::size_t>(tag)].widget;
    auto* target = field != nullptr ? qobject_cast<QWidget*>(field->property(NativeReturnTargetProperty).value<QObject*>()) : nullptr;
    if (target == nullptr) { return false; }
    // Finishing the AppKit field invokes the shared editingFinished path.
    // Its queued model command may replace the header, so retain only the
    // native target while the editor gives up focus.
    NSTableView* table = nil;
    for (const Binding& binding : m_bindings) {
      if (binding.widget == target) { table = binding.table; break; }
    }
    NSWindow* window = sender.window;
    if ([window makeFirstResponder:nil] && table != nil) { [window makeFirstResponder:table]; }
    return true;
  }

  bool textChanged(NSControl* sender, bool finished) {
    QPointer<AppKitWidgetView> alive(this);
    const NSInteger tag = sender.tag;
    if (tag < 0 || static_cast<std::size_t>(tag) >= m_bindings.size()) { return true; }
    const QPointer<QWidget> pointer = m_bindings[static_cast<std::size_t>(tag)].widget;
    QWidget* widget = pointer;
    const QString value = qt([(NSTextField*)sender stringValue]);
    if (auto* edit = qobject_cast<QLineEdit*>(widget)) {
      if (edit->isReadOnly()) { return true; }
      QString proposed = value;
      int cursor = proposed.size();
      if (const QValidator* validator = edit->validator()) {
        if (validator->validate(proposed, cursor) == QValidator::Invalid) {
          [(NSTextField*)sender setStringValue:ns(edit->text())];
          return false;
        }
      }
      if (!finished) {
        edit->setText(value);
        if (alive == nullptr || pointer == nullptr) { return true; }
        edit->setModified(true);
        QMetaObject::invokeMethod(edit, "textEdited", Qt::DirectConnection, Q_ARG(QString, value));
      } else if (edit->hasAcceptableInput()) {
        QMetaObject::invokeMethod(edit, "editingFinished", Qt::DirectConnection);
        if (alive == nullptr || pointer == nullptr) { return true; }
        // The shared handler can normalize or reject the committed text.
        // Refresh normally preserves an active editor, so reflect that
        // completed validation here, as for spin boxes below.
        NATIVE_STRING(((NSTextField*)sender), stringValue, edit->text());
        scheduleRefresh();
      } else { return false; }
    } else if (auto* spin = qobject_cast<QAbstractSpinBox*>(widget)) {
      // Interpret through the presenter's virtual validator/valueFromText
      // path, including DecimalSpinBox's comma support and fixups.
      if (auto* line = spin->findChild<QLineEdit*>()) {
        line->setText(value);
        if (alive == nullptr || pointer == nullptr) { return true; }
        if (spin->keyboardTracking() || finished) { spin->interpretText(); }
        if (alive == nullptr || pointer == nullptr) { return true; }
        if (finished) { QMetaObject::invokeMethod(spin, "editingFinished", Qt::DirectConnection); }
        if (alive == nullptr || pointer == nullptr) { return true; }
        // The finished field shows the presenter's own text with its prefix
        // and suffix, also when the value did not change and no refresh
        // follows; Qt reformats its editor the same way (mitcad#975).
        if (finished) {
          NATIVE_STRING(((NSTextField*)sender), stringValue, spin->text());
          scheduleRefresh();
        }
      }
    } else if (auto* combo = qobject_cast<QComboBox*>(widget)) {
      combo->setEditText(value);
    }
    return true;
  }

  bool textCanEnd(NSControl* sender) {
    const NSInteger tag = sender.tag;
    if (tag < 0 || static_cast<std::size_t>(tag) >= m_bindings.size()) { return true; }
    if (auto* edit = qobject_cast<QLineEdit*>(m_bindings[static_cast<std::size_t>(tag)].widget)) {
      return edit->hasAcceptableInput();
    }
    return true;
  }

  QWidget* textPresenter(NSTextView* sender) const {
    const int id = qt(sender.identifier).toInt();
    if (id < 0 || static_cast<std::size_t>(id) >= m_bindings.size() ||
        m_bindings[static_cast<std::size_t>(id)].text != sender) return nullptr;
    return m_bindings[static_cast<std::size_t>(id)].widget;
  }
  NativeTextEditorCommands* textCommands(NSTextView* sender) const {
    return dynamic_cast<NativeTextEditorCommands*>(textPresenter(sender));
  }
  QListWidget* textCompletions(NSTextView* sender) const {
    const auto* commands = textCommands(sender);
    return commands != nullptr ? commands->nativeTextCompletions() : nullptr;
  }
  void textViewChanged(NSTextView* sender) {
    if (m_refreshing) return;
    QPointer<QWidget> widget = textPresenter(sender);
    auto* edit = qobject_cast<QPlainTextEdit*>(widget);
    if (edit == nullptr) {
      if (auto* rich = qobject_cast<QTextEdit*>(widget)) rich->setPlainText(qt(sender.string));
      return;
    }
    const QString value = qt(sender.string);
    const bool changed = edit->toPlainText() != value;
    const QTextCursor before = edit->textCursor();
    const NSRange range = sender.selectedRange;
    const int start = static_cast<int>(std::min<NSUInteger>(range.location, value.size()));
    const int length = static_cast<int>(std::min<NSUInteger>(range.length, value.size() - start));
    // Publish one complete edit. Formula assistance must see the actual
    // caret when textChanged is emitted, rather than setPlainText's zero.
    {
      const QSignalBlocker block(edit);
      if (changed) edit->setPlainText(value);
      QTextCursor cursor = edit->textCursor();
      cursor.setPosition(start);
      cursor.setPosition(start + length, QTextCursor::KeepAnchor);
      edit->setTextCursor(cursor);
    }
    const QTextCursor after = edit->textCursor();
    if (changed) emit edit->textChanged();
    if (!widget) return;
    if (before.position() != after.position()) emit edit->cursorPositionChanged();
    if (!widget) return;
    if (before.anchor() != after.anchor() || before.position() != after.position()) emit edit->selectionChanged();
    scheduleRefresh();
  }
  void applyTextState(NSTextView* sender) {
    auto* edit = qobject_cast<QPlainTextEdit*>(textPresenter(sender));
    if (edit == nullptr) return;
    const bool before = m_refreshing;
    m_refreshing = true;
    NATIVE_STRING(sender, string, edit->toPlainText());
    const QTextCursor cursor = edit->textCursor();
    sender.selectedRange = NSMakeRange(cursor.selectionStart(), cursor.selectionEnd() - cursor.selectionStart());
    [sender scrollRangeToVisible:sender.selectedRange];
    m_refreshing = before;
  }
  bool textKey(NSTextView* sender, NSEvent* event) {
    if (m_refreshing || sender.hasMarkedText || !sender.editable) return false;
    NativeTextEditorCommands* commands = textCommands(sender);
    const QKeySequence sequence = mac::keySequence(event);
    if (commands == nullptr || sequence.isEmpty()) return false;
    const QPointer<AppKitWidgetView> alive(this);
    const QPointer<QWidget> source(textPresenter(sender));
    textViewChanged(sender);
    if (!alive || !source) return false;
    QKeyEvent key(QEvent::KeyPress, sequence[0].key(), sequence[0].keyboardModifiers(),
                  qt(event.characters), event.isARepeat);
    const bool handled = commands->nativeTextKey(&key);
    if (alive && source) {
      if (handled) applyTextState(sender);
      refresh();
    }
    return handled;
  }
  void completeText(NSTextView* sender, int row) {
    NativeTextEditorCommands* commands = textCommands(sender);
    const QPointer<AppKitWidgetView> alive(this);
    const QPointer<QWidget> source(textPresenter(sender));
    if (commands == nullptr || !source) return;
    commands->nativeTextComplete(row);
    if (!alive || !source) return;
    applyTextState(sender);
    refresh();
    if (sender.window.visible && !sender.window.attachedSheet) {
      [sender.window makeKeyWindow];
      [sender.window makeFirstResponder:sender];
    }
  }
  void leaveText(NSTextView* sender) {
    if (QWidget* source = textPresenter(sender)) {
      QFocusEvent event(QEvent::FocusOut, Qt::OtherFocusReason);
      QCoreApplication::sendEvent(source, &event);
    }
  }

  bool linkClicked(NSTextView* sender, id link) {
    const int id = qt(sender.identifier).toInt();
    if (id < 0 || static_cast<std::size_t>(id) >= m_bindings.size()) { return false; }
    auto* caption = qobject_cast<QLabel*>(m_bindings[static_cast<std::size_t>(id)].widget);
    if (caption == nullptr) { return false; }
    NSString* destination = [link isKindOfClass:NSURL.class] ? [(NSURL*)link absoluteString] : (NSString*)link;
    if (caption->openExternalLinks()) { [NSWorkspace.sharedWorkspace openURL:[NSURL URLWithString:destination]]; }
    else { QMetaObject::invokeMethod(caption, "linkActivated", Qt::DirectConnection, Q_ARG(QString, qt(destination))); }
    return true;
  }

  void tabChanged(NSTabView* sender, NSTabViewItem* item) {
    const int id = qt(sender.identifier).toInt();
    if (id < 0 || static_cast<std::size_t>(id) >= m_bindings.size()) { return; }
    // The section selector follows every change, a refresh's too.
    [m_bindings[static_cast<std::size_t>(id)].panes sync];
    if (m_refreshing) { return; }
    QWidget* widget = m_bindings[static_cast<std::size_t>(id)].widget;
    const int index = qt(item.identifier).toInt();
    if (auto* tabs = qobject_cast<QTabWidget*>(widget)) { tabs->setCurrentIndex(index); }
    else if (auto* pages = qobject_cast<QStackedWidget*>(widget)) {
      if (auto* settings = qobject_cast<SettingsWindow*>(pages->parentWidget())) { settings->showPane(index); }
      else { pages->setCurrentIndex(index); }
    }
  }

  int modelBinding(NSTableView* table) const { return qt(table.identifier).toInt(); }
  NSInteger presenterSelectedRows(NSTableView* table) const {
    const int id = modelBinding(table);
    if (id < 0 || static_cast<std::size_t>(id) >= m_bindings.size()) return -1;
    const Binding& binding = m_bindings[static_cast<std::size_t>(id)];
    const auto* view = qobject_cast<QAbstractItemView*>(binding.widget);
    if (binding.table != table || view == nullptr || view->selectionModel() == nullptr) return -1;
    return view->selectionModel()->selectedRows().size();
  }
  QAbstractItemView* currentRowPresenter(NSTableView* table) const {
    const int id = modelBinding(table);
    if (id < 0 || static_cast<std::size_t>(id) >= m_bindings.size()) return nullptr;
    auto* view = qobject_cast<QAbstractItemView*>(m_bindings[static_cast<std::size_t>(id)].widget);
    return view != nullptr && view->isEnabled() && view->selectionMode() == QAbstractItemView::NoSelection &&
        view->property(NativeCurrentRowNavigationProperty).toBool() ? view : nullptr;
  }
  NSView* nativeFocusTarget(QWidget* source) {
    if (source == nullptr) return nil;
    for (const Binding& binding : m_bindings) {
      if (binding.widget == source) return binding.table != nil ? (NSView*)binding.table : binding.control;
    }
    for (const auto& embedded : m_cellViews) {
      if (NSView* found = embedded.second->nativeFocusTarget(source)) return found;
    }
    return nil;
  }
  QWidget* currentRowEditor(QAbstractItemView* view) const {
    const QModelIndex current = view->currentIndex();
    for (int column = 1; current.isValid() && column < view->model()->columnCount(current.parent()); ++column) {
      if (QWidget* editor = view->indexWidget(current.siblingAtColumn(column))) {
        // A cell can contain both a label and an editor. Its focus proxy,
        // rather than the layout wrapper, receives the shared key/focus
        // events and maps to the native control.
        QSet<QWidget*> visited{editor};
        while (QWidget* proxy = editor->focusProxy()) {
          if (visited.contains(proxy) || visited.size() >= 64) return nullptr;
          visited.insert(proxy);
          editor = proxy;
        }
        return editor;
      }
    }
    return nullptr;
  }
  void updateCurrentRows(NSTableView* table) {
    auto* view = currentRowPresenter(table);
    if (view == nullptr) return;
    NSResponder* focus = table.window.firstResponder;
    for (NSInteger row = 0; row < table.numberOfRows; ++row) {
      NSTableRowView* native = [table rowViewAtRow:row makeIfNecessary:NO];
      if (![native isKindOfClass:MitcadNativeCurrentRowView.class]) continue;
      MitcadNativeModelNode* node = nodeAt(table, row);
      const bool focused = focus == table || ([focus isKindOfClass:NSView.class] &&
          [(NSView*)focus isDescendantOf:native]);
      const bool current = node != nil && view->currentIndex().isValid() &&
          view->currentIndex().siblingAtColumn(0) == node->index && focused;
      auto* cursor = (MitcadNativeCurrentRowView*)native;
      if (cursor.currentFocus != current) { cursor.currentFocus = current; cursor.needsDisplay = YES; }
    }
  }
  // A row's real native editor publishes the same current row and focus
  // event that its shared widget receives, including pointer activation.
  bool currentRowEditorFocused(NSTableView* table, NSView* editor, QWidget* source) {
    auto* view = currentRowPresenter(table);
    const NSInteger row = [table rowForView:editor];
    if (view == nullptr || row < 0 || source == nullptr) return false;
    const QModelIndex index = modelIndex(table, row, 0);
    if (!index.isValid()) return false;
    view->setCurrentIndex(index);
    QFocusEvent focus(QEvent::FocusIn, Qt::TabFocusReason);
    QCoreApplication::sendEvent(source, &focus);
    updateCurrentRows(table);
    return true;
  }
  // The hidden Qt tree need not have a laid-out viewport. Navigate the
  // enabled rows the native outline actually shows, and publish only the
  // shared cursor: NoSelection and warning colours stay unchanged.
  void moveCurrentRow(NSTableView* table, QAbstractItemView* view, int key) {
    const QModelIndex current = view->currentIndex().siblingAtColumn(0);
    NSInteger row = -1;
    for (NSInteger candidate = 0; candidate < table.numberOfRows; ++candidate) {
      if (liveIndex(table, nodeAt(table, candidate)) == current) { row = candidate; break; }
    }
    NSInteger target = row;
    if (key == Qt::Key_Home) target = 0;
    else if (key == Qt::Key_End) target = table.numberOfRows - 1;
    else if (key == Qt::Key_Up || key == Qt::Key_Down) target += key == Qt::Key_Up ? -1 : 1;
    else if ([table isKindOfClass:NSOutlineView.class]) {
      auto* outline = (NSOutlineView*)table;
      MitcadNativeModelNode* node = nodeAt(table, row);
      if (node == nil) return;
      if (key == Qt::Key_Left) {
        if ([outline isItemExpanded:node]) { [outline collapseItem:node]; return; }
        const QModelIndex parent = current.parent();
        for (NSInteger candidate = 0; parent.isValid() && candidate < table.numberOfRows; ++candidate) {
          if (liveIndex(table, nodeAt(table, candidate)) == parent) { target = candidate; break; }
        }
      } else if (key == Qt::Key_Right && node->children.count > 0) {
        if (![outline isItemExpanded:node]) { [outline expandItem:node]; return; }
        target = row + 1;
      }
    }
    const NSInteger direction = key == Qt::Key_Up || key == Qt::Key_End ? -1 : 1;
    for (; target >= 0 && target < table.numberOfRows; target += direction) {
      const QModelIndex next = liveIndex(table, nodeAt(table, target));
      if (!next.isValid() || !next.flags().testFlag(Qt::ItemIsEnabled)) continue;
      view->setCurrentIndex(next);
      [table scrollRowToVisible:target];
      if (!qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT"))
        qDebug().noquote() << QStringLiteral("Native current row: %1 key=%2 row=%3 text=%4")
            .arg(view->objectName()).arg(key).arg(target).arg(next.data().toString());
      break;
    }
  }
  bool currentRowKey(NSEvent* event) {
    const QKeySequence sequence = mac::keySequence(event);
    if (sequence.isEmpty()) return false;
    const auto modifiers = sequence[0].keyboardModifiers();
    if (modifiers != Qt::NoModifier && modifiers != Qt::ShiftModifier) return false;
    const int key = sequence[0].key();
    const bool tab = key == Qt::Key_Tab || key == Qt::Key_Backtab;
    for (const Binding& binding : m_bindings) {
      NSTableView* table = binding.table;
      auto* view = table != nil ? currentRowPresenter(table) : nullptr;
      if (view == nullptr) continue;
      NSResponder* responder = table.window.firstResponder;
      const bool inList = responder == table;
      if (!inList && (![responder isKindOfClass:NSView.class] || ![(NSView*)responder isDescendantOf:table])) continue;
      if (!inList && !tab) return false;
      if (inList && !tab && key != Qt::Key_Up && key != Qt::Key_Down && key != Qt::Key_Left &&
          key != Qt::Key_Right && key != Qt::Key_Home && key != Qt::Key_End && key != Qt::Key_Space) return false;
      QWidget* source = inList ? view : currentRowEditor(view);
      if (source == nullptr) return false;
      const bool backwards = key == Qt::Key_Backtab || modifiers.testFlag(Qt::ShiftModifier);
      // Space/Tab remain the shared event filter's actions. Cursor arrows
      // use visible native rows rather than the hidden Qt viewport layout.
      if (inList && !tab && key != Qt::Key_Space) moveCurrentRow(table, view, key);
      else {
        QKeyEvent press(QEvent::KeyPress, backwards && tab ? Qt::Key_Backtab : key, modifiers);
        QCoreApplication::sendEvent(source, &press);
      }
      if (tab) {
        QWidget* target = inList && !backwards ? currentRowEditor(view) : nullptr;
        if (!inList && backwards) target = view;
        if (target == nullptr && !backwards)
          target = qobject_cast<QWidget*>(view->property(NativeCurrentRowNextProperty).value<QObject*>());
        if (NSView* native = nativeFocusTarget(target)) {
          [table.window makeFirstResponder:native];
          if (target != view && inList) currentRowEditorFocused(table, native, target);
        } else if (backwards) [table.window selectPreviousKeyView:table];
      } else {
        for (NSInteger row = 0; row < table.numberOfRows; ++row) {
          MitcadNativeModelNode* node = nodeAt(table, row);
          if (node != nil && node->index == view->currentIndex().siblingAtColumn(0)) {
            [table scrollRowToVisible:row]; break;
          }
        }
      }
      refresh();
      updateCurrentRows(table);
      return true;
    }
    return false;
  }
  NSTableRowView* currentRowView(NSTableView* table, MitcadNativeModelNode* node) {
    auto* view = currentRowPresenter(table);
    const bool spanning = modelSpansRow(table, node);
    if (view == nullptr && !spanning) return nil;
    MitcadNativeCurrentRowView* row = spanning ? [MitcadNativeSpannedRowView new] : [MitcadNativeCurrentRowView new];
    row.groupRowStyle = spanning;
    row.currentFocus = view != nullptr && table.window.firstResponder == table && node != nil &&
        view->currentIndex().siblingAtColumn(0) == node->index;
    return row;
  }
  bool currentRowClicked(NSTableView* table, NSInteger row) {
    auto* view = currentRowPresenter(table);
    const QModelIndex index = view != nullptr ? modelIndex(table, row, 0) : QModelIndex();
    if (!index.isValid()) return false;
    view->setCurrentIndex(index);
    [table.window makeFirstResponder:table];
    updateCurrentRows(table);
    return true;
  }
  NSMutableArray<MitcadNativeModelNode*>* children(int id, MitcadNativeModelNode* node) {
    if (id < 0 || static_cast<std::size_t>(id) >= m_bindings.size()) { return nil; }
    return node == nil ? m_bindings[static_cast<std::size_t>(id)].nodes : node->children;
  }
  MitcadNativeModelNode* nodeAt(NSTableView* table, NSInteger row) {
    if (row < 0) { return nil; }
    if ([table isKindOfClass:NSOutlineView.class]) { return [(NSOutlineView*)table itemAtRow:row]; }
    auto* nodes = children(modelBinding(table), nil);
    return row < static_cast<NSInteger>(nodes.count) ? nodes[static_cast<NSUInteger>(row)] : nil;
  }
  bool modelSpansRow(NSTableView* table, MitcadNativeModelNode* node) const {
    const int id = modelBinding(table);
    if (node == nil || !node->index.isValid() || id < 0 || static_cast<std::size_t>(id) >= m_bindings.size()) return false;
    const auto* tree = qobject_cast<QTreeView*>(m_bindings[static_cast<std::size_t>(id)].widget);
    return tree != nullptr && tree->isFirstColumnSpanned(node->index.row(), node->index.parent());
  }
  bool modelSelectable(NSTableView* table, MitcadNativeModelNode* node) const {
    const int id = modelBinding(table);
    if (node == nil || !node->index.isValid() || id < 0 || static_cast<std::size_t>(id) >= m_bindings.size()) return false;
    const auto* view = qobject_cast<QAbstractItemView*>(m_bindings[static_cast<std::size_t>(id)].widget);
    return view != nullptr && view->isEnabled() && view->selectionMode() != QAbstractItemView::NoSelection &&
        node->index.flags().testFlag(Qt::ItemIsEnabled) && node->index.flags().testFlag(Qt::ItemIsSelectable);
  }
  CGFloat listRowHeight(QAbstractItemView* view, int row, CGFloat minimum) const {
    const QModelIndex index = view->model()->index(row, 0, view->rootIndex());
    const int lines = static_cast<int>(plain(index.data().toString()).count(QLatin1Char('\n'))) + 1;
    NSFont* font = mac::nativeCellStyle(index, view).font;
    const CGFloat line = [[NSLayoutManager new] defaultLineHeightForFont:font];
    return std::max<CGFloat>({minimum, static_cast<CGFloat>(view->sizeHintForRow(row)), std::ceil(lines * line)});
  }
  CGFloat modelRowHeight(NSTableView* table, NSInteger row) {
    const int id = modelBinding(table);
    if (id < 0 || static_cast<std::size_t>(id) >= m_bindings.size()) return table.rowHeight;
    auto* view = qobject_cast<QListView*>(m_bindings[static_cast<std::size_t>(id)].widget);
    if (view == nullptr || view->property(NativeListRowLimitProperty).toInt() <= 0) return table.rowHeight;
    const QModelIndex index = modelIndex(table, row, 0);
    if (!index.isValid()) return table.rowHeight;
    return listRowHeight(view, index.row(), table.rowHeight);
  }
  NSView* modelCell(NSTableView* table, NSTableColumn* column, MitcadNativeModelNode* node) {
    const int id = modelBinding(table);
    if (node == nil || id < 0 || static_cast<std::size_t>(id) >= m_bindings.size()) { return nil; }
    auto* view = qobject_cast<QAbstractItemView*>(m_bindings[static_cast<std::size_t>(id)].widget);
    if (view == nullptr || !node->index.isValid()) { return nil; }
    // AppKit gives group rows one full-width cell, with a nil column. The
    // shared tree's explicit first-column span is that same contract.
    const bool spanning = modelSpansRow(table, node);
    if (column == nil && !spanning) return nil;
    const int col = column != nil ? qt(column.identifier).toInt() : 0;
    const QModelIndex index = QModelIndex(node->index).siblingAtColumn(col);
    if (QWidget* widget = view->indexWidget(index)) {
      for (auto& embedded : m_cellViews) {
        if (embedded.first == widget) {
          embedded.second->refresh();
          return (__bridge NSView*)embedded.second->nativeView();
        }
      }
      // A table cell owns its insets. Dialog margins here would leave an
      // embedded choice too narrow to read even when its column fits it.
      auto embedded = std::make_unique<AppKitWidgetView>(widget, true);
      NSView* native = (__bridge NSView*)embedded->nativeView();
      embedded->refresh();
      m_cellViews.emplace_back(QPointer<QWidget>(widget), std::move(embedded));
      return native;
    }
    MitcadNativeModelCell* cell = [MitcadNativeModelCell new];
    cell->owner = this;
    cell->binding = id;
    cell->index = index;
    cell->rowIdentifier = index.siblingAtColumn(0).data(NativeControlIdentifierRole).toString();
    [m_cells addObject:cell];
    if (index.flags().testFlag(Qt::ItemIsUserCheckable) && index.data(Qt::CheckStateRole).isValid()) {
      NSButton* check;
      if (view->property("nativeModelHoverEvents").toBool()) {
        MitcadNativeModelHoverButton* hover = [MitcadNativeModelHoverButton new];
        hover->presenter = view;
        hover->modelIndex = index;
        [hover setButtonType:NSButtonTypeSwitch];
        hover.title = ns(index.data().toString());
        hover.target = cell;
        hover.action = @selector(checked:);
        check = hover;
      } else {
        check = [NSButton checkboxWithTitle:ns(index.data().toString()) target:cell action:@selector(checked:)];
      }
      const int state = index.data(Qt::CheckStateRole).toInt();
      check.allowsMixedState = state == Qt::PartiallyChecked;
      check.state = state == Qt::PartiallyChecked ? NSControlStateValueMixed
                    : state == Qt::Checked ? NSControlStateValueOn : NSControlStateValueOff;
      check.enabled = view->isEnabled() && index.flags().testFlag(Qt::ItemIsEnabled);
      const auto style = mac::nativeCellStyle(index, view);
      check.font = style.font;
      if (style.text != nil) {
        check.attributedTitle = [[NSAttributedString alloc] initWithString:check.title
            attributes:@{NSForegroundColorAttributeName:style.text, NSFontAttributeName:style.font}];
      }
      check.toolTip = ns(plain(index.data(Qt::ToolTipRole).toString()));
      check.accessibilityIdentifier = ns(index.data(NativeControlIdentifierRole).toString());
      check.translatesAutoresizingMaskIntoConstraints = NO;
      [cell addSubview:check];
      [check.leadingAnchor constraintEqualToAnchor:cell.leadingAnchor constant:3].active = YES;
      [check.trailingAnchor constraintLessThanOrEqualToAnchor:cell.trailingAnchor constant:-4].active = YES;
      [check.centerYAnchor constraintEqualToAnchor:cell.centerYAnchor].active = YES;
    } else {
      cell.accessibilityIdentifier = ns(index.data(NativeControlIdentifierRole).toString());
      const QVariant permission = index.data(NativeEditableRole);
      const bool editable = permission.isValid() ? permission.toBool()
          : index.flags().testFlag(Qt::ItemIsEditable) && view->editTriggers() != QAbstractItemView::NoEditTriggers;
      NSTextField* field = nil;
      if (editable) {
        field = [MitcadNativeModelField new];
        field.stringValue = ns(plain(index.data().toString()));
        field.bezeled = NO;
        field.bordered = NO;
        field.drawsBackground = NO;
        field.font = [NSFont systemFontOfSize:NSFont.systemFontSize];
        field.translatesAutoresizingMaskIntoConstraints = NO;
        [field setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationHorizontal];
      } else { field = label(index.data().toString()); }
      if (view->property("nativeSourceList").toBool()) {
        field.maximumNumberOfLines = 1;
        field.lineBreakMode = NSLineBreakByTruncatingMiddle;
      }
      field.editable = editable;
      // A display-only item selects its row on a press, as in Qt. A
      // selectable NSTextField would consume that press for text selection
      // before the table can publish its selection to the shared presenter.
      field.selectable = field.editable;
      field.delegate = cell;
      field.enabled = index.flags().testFlag(Qt::ItemIsEnabled);
      field.toolTip = ns(plain(index.data(Qt::ToolTipRole).toString()));
      cell.textField = field;
      [cell addSubview:field];
      CGFloat inset = 4;
      const QVariant decoration = index.data(Qt::DecorationRole);
      QPixmap icon;
      if (decoration.canConvert<QIcon>()) { icon = decoration.value<QIcon>().pixmap(std::max(18, std::min(80, view->iconSize().height())), std::max(18, std::min(80, view->iconSize().height()))); }
      else if (decoration.canConvert<QPixmap>()) { icon = decoration.value<QPixmap>(); }
      if (!icon.isNull()) {
        NSImageView* preview = [NSImageView new];
        preview.image = image(icon);
        preview.imageScaling = NSImageScaleProportionallyUpOrDown;
        preview.translatesAutoresizingMaskIntoConstraints = NO;
        cell.imageView = preview;
        [cell addSubview:preview];
        const CGFloat side = std::max<CGFloat>(18, std::min<CGFloat>(80, table.rowHeight - 4));
        [preview.leadingAnchor constraintEqualToAnchor:cell.leadingAnchor constant:3].active = YES;
        [preview.centerYAnchor constraintEqualToAnchor:cell.centerYAnchor].active = YES;
        [preview.widthAnchor constraintEqualToConstant:side].active = YES;
        [preview.heightAnchor constraintEqualToConstant:side].active = YES;
        inset = side + 8;
      }
      [field.leadingAnchor constraintEqualToAnchor:cell.leadingAnchor constant:inset].active = YES;
      [field.trailingAnchor constraintEqualToAnchor:cell.trailingAnchor constant:-4].active = YES;
      const Qt::Alignment alignment = Qt::Alignment(index.data(Qt::TextAlignmentRole).toInt());
      if (spanning) {
        field.alignment = alignment.testFlag(Qt::AlignRight) ? NSTextAlignmentRight
                         : alignment.testFlag(Qt::AlignHCenter) ? NSTextAlignmentCenter : NSTextAlignmentNatural;
      }
      if (spanning && alignment.testFlag(Qt::AlignTop)) {
        [field.topAnchor constraintEqualToAnchor:cell.topAnchor constant:2].active = YES;
      } else if (spanning && alignment.testFlag(Qt::AlignBottom)) {
        [field.bottomAnchor constraintEqualToAnchor:cell.bottomAnchor constant:-2].active = YES;
      } else { [field.centerYAnchor constraintEqualToAnchor:cell.centerYAnchor].active = YES; }
      // The shared delegate's cell states (mitcad#814): refused values,
      // fonts and colours. Compact multiline lists retain explicit breaks;
      // other cells keep the existing single-line elision.
      if (view->property(NativeListRowLimitProperty).toInt() > 0 && qobject_cast<QListView*>(view) != nullptr) {
        field.usesSingleLineMode = NO;
        field.maximumNumberOfLines = 0;
        field.cell.wraps = NO; // preserve explicit lines, as wordWrap(false)
        field.cell.scrollable = NO;
        field.cell.truncatesLastVisibleLine = YES;
        field.lineBreakMode = view->textElideMode() == Qt::ElideLeft ? NSLineBreakByTruncatingHead
                            : view->textElideMode() == Qt::ElideMiddle ? NSLineBreakByTruncatingMiddle
                            : view->textElideMode() == Qt::ElideNone ? NSLineBreakByClipping : NSLineBreakByTruncatingTail;
      } else if (!view->property("nativeSourceList").toBool()) { mac::elideNativeCell(field, view->textElideMode()); }
      const mac::NativeCellStyle style = mac::nativeCellStyle(index, view);
      field.font = style.font;
      cell->tone = style.text;
      cell->fill = style.fill;
      [cell applyTone];
      if (spanning && field.toolTip.length == 0) {
        field.toolTip = field.stringValue; // a section row's whole name
      }
    }
    return cell;
  }
  void modelEdit(int id, const QPersistentModelIndex& index, const QVariant& value, int role) {
    if (id < 0 || static_cast<std::size_t>(id) >= m_bindings.size() || !index.isValid()) { return; }
    auto* view = qobject_cast<QAbstractItemView*>(m_bindings[static_cast<std::size_t>(id)].widget);
    if (view != nullptr && view->isEnabled() && index.flags().testFlag(Qt::ItemIsEnabled) && view->model() == index.model()) {
      QPointer<AppKitWidgetView> alive(this);
      view->model()->setData(index, value, role);
      // A reload deferred while this cell was edited follows now.
      if (alive != nullptr) { scheduleRefresh(); }
    }
  }
  // A cell edit ended by its keys: what waited for it follows (mitcad#814).
  void cellEditEnded() { scheduleRefresh(); }
  // The current row with a stable NativeControlIdentifierRole in its first
  // column, such as an edited row after the model rebuilt its rows.
  static QModelIndex stableRow(const QAbstractItemView* view, const QString& identifier) {
    QAbstractItemModel* model = view->model();
    if (identifier.isEmpty() || model == nullptr || model->rowCount(view->rootIndex()) == 0) { return {}; }
    const QModelIndexList found = model->match(model->index(0, 0, view->rootIndex()), NativeControlIdentifierRole,
                                               identifier, 1, Qt::MatchExactly | Qt::MatchRecursive);
    return found.isEmpty() ? QModelIndex() : found.first();
  }
  // Whether one of this view's model cells has the field editor. Its edit
  // keeps its cell until it ends, as the shared view keeps its editor.
  bool editingCell() const {
    NSResponder* responder = m_root.window.firstResponder;
    if (![responder isKindOfClass:NSTextView.class] || !((NSTextView*)responder).isFieldEditor) { return false; }
    id edited = ((NSTextView*)responder).delegate;
    return [edited isKindOfClass:NSTextField.class] &&
           [((NSTextField*)edited).delegate isKindOfClass:MitcadNativeModelCell.class] &&
           [(NSView*)edited isDescendantOf:m_root];
  }
  // Starts editing a cell's own text field, as the shared view's editor.
  bool beginCellEdit(NSTableView* table, NSInteger row, NSInteger column) {
    if (row < 0 || column < 0 || row >= table.numberOfRows || column >= table.numberOfColumns ||
        table.tableColumns[static_cast<NSUInteger>(column)].hidden) { return false; }
    NSView* cell = [table viewAtColumn:column row:row makeIfNecessary:YES];
    if (![cell isKindOfClass:NSTableCellView.class]) { return false; }
    NSTextField* field = ((NSTableCellView*)cell).textField;
    if (field == nil || !field.editable || !field.enabled) { return false; }
    // A cell of a column scrolled out of a narrow table is only a temporary
    // view: edit the one the table shows once the cell is scrolled in.
    [table scrollRowToVisible:row];
    [table scrollColumnToVisible:column];
    table.needsLayout = YES;
    [table layoutSubtreeIfNeeded];
    [table displayIfNeeded];
    if (NSView* live = [table viewAtColumn:column row:row makeIfNecessary:NO];
        [live isKindOfClass:NSTableCellView.class] && ((NSTableCellView*)live).textField != nil) {
      field = ((NSTableCellView*)live).textField;
    }
    if (field.window != nil && [table.window makeFirstResponder:field]) {
      mac::selectEditedText(field);
      return true;
    }
    // AppKit's own cell editing finds the view the table shows.
    [table editColumn:column row:row withEvent:nil select:YES];
    NSView* edited = [table viewAtColumn:column row:row makeIfNecessary:NO];
    return [edited isKindOfClass:NSTableCellView.class] && ((NSTableCellView*)edited).textField.currentEditor != nil;
  }
  // F2 on a focused table edits its current cell, as in the shared view;
  // a cell without a native editor lets the presenter answer the key (it
  // may say why the cell cannot be edited).
  bool modelEditKey(NSTableView* table) {
    QPointer<AppKitWidgetView> alive(this);
    const int id = modelBinding(table);
    if (m_refreshing || id < 0 || static_cast<std::size_t>(id) >= m_bindings.size()) { return false; }
    QPointer<QAbstractItemView> view = qobject_cast<QAbstractItemView*>(m_bindings[static_cast<std::size_t>(id)].widget);
    const NSInteger row = table.selectedRow;
    MitcadNativeModelNode* node = nodeAt(table, row);
    if (view == nullptr || node == nil || !node->index.isValid()) { return false; }
    const QModelIndex current = view->currentIndex();
    const int column = current.isValid() && current.siblingAtColumn(0) == QModelIndex(node->index).siblingAtColumn(0)
                           ? current.column() : 0;
    if (beginCellEdit(table, row, [table columnWithIdentifier:ns(QString::number(column))])) { return true; }
    if (alive == nullptr || view == nullptr) { return true; }
    QKeyEvent press(QEvent::KeyPress, Qt::Key_F2, Qt::NoModifier);
    QCoreApplication::sendEvent(view.data(), &press);
    return true;
  }
  // Tab and Shift+Tab in a cell editor: commit, then edit the next editable
  // cell of the row; past the last one the table keeps the focus.
  bool modelEditNext(NSControl* field, bool backwards) {
    QPointer<AppKitWidgetView> alive(this);
    NSTableView* table = mac::owningTable(field);
    if (table == nil) { return false; }
    const NSInteger row = [table rowForView:field];
    const NSInteger column = [table columnForView:field];
    if (row < 0 || column < 0) { return false; }
    NSWindow* window = table.window;
    NSObject* delegate = (NSObject*)((NSTextField*)field).delegate;
    MitcadNativeModelCell* cell =
        [delegate isKindOfClass:MitcadNativeModelCell.class] ? (MitcadNativeModelCell*)delegate : nil;
    if (cell != nil) { cell->finishing = YES; }
    const BOOL committed = [window makeFirstResponder:table]; // commits through the cell
    if (cell != nil) { cell->finishing = NO; }
    if (!committed || alive == nullptr) { return true; }
    scheduleRefresh();
    for (NSInteger next = column + (backwards ? -1 : 1); next >= 0 && next < table.numberOfColumns;
         next += backwards ? -1 : 1) {
      if (beginCellEdit(table, row, next)) { return true; }
    }
    // Document cards leave keys to the document window, as after Return.
    if (![window isKindOfClass:MitcadNativeSecondaryWindow.class]) { [window makeFirstResponder:nil]; }
    return true;
  }
  void modelCellPressed(MitcadNativeModelCell* cell) {
    const int id = cell->binding;
    if (m_refreshing || id < 0 || static_cast<std::size_t>(id) >= m_bindings.size()) return;
    NSTableView* table = m_bindings[static_cast<std::size_t>(id)].table;
    auto* view = qobject_cast<QAbstractItemView*>(m_bindings[static_cast<std::size_t>(id)].widget);
    const QPersistentModelIndex current = cell->index;
    const NSInteger row = table != nil ? [table rowForView:cell] : -1;
    if (!qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT"))
      qInfo().noquote() << QStringLiteral("Native model field press: %1 native-row=%2 column=%3 editor=%4 responder=%5")
          .arg(current.data(NativeControlIdentifierRole).toString()).arg(qlonglong(row)).arg(current.column())
          .arg(cell.textField.currentEditor != nil).arg(qt(NSStringFromClass(table.window.firstResponder.class)));
    if (view == nullptr || !view->isEnabled() || view->selectionModel() == nullptr || !current.isValid() ||
        current.model() != view->model() || table != mac::owningTable(cell) || row < 0 ||
        !cell.textField.enabled || !cell.textField.editable || modelIndex(table, row, current.column()) != current) return;
    const QPointer<AppKitWidgetView> alive(this);
    const QPointer<QAbstractItemView> presenter(view);
    const QPointer<QItemSelectionModel> selection(view->selectionModel());
    // An editable NSTextField handles the press before its table can
    // select the row. Publish the actual pressed cell, then let the same
    // native row selection callback notify the shared selection observers.
    selection->setCurrentIndex(current, QItemSelectionModel::NoUpdate);
    if (!alive || !presenter || !selection || !current.isValid()) return;
    if (view->selectionMode() != QAbstractItemView::NoSelection && current.flags().testFlag(Qt::ItemIsSelectable)) {
      // Editing one cell of an already selected set keeps that set.
      if (![table.selectedRowIndexes containsIndex:static_cast<NSUInteger>(row)]) {
        [table selectRowIndexes:[NSIndexSet indexSetWithIndex:static_cast<NSUInteger>(row)] byExtendingSelection:NO];
      }
      if (!alive || !presenter || !selection || !current.isValid()) return;
      if (!selection->isSelected(current)) modelSelection(table);
      if (!alive || !presenter || !selection || !current.isValid()) return;
      // The row callback may have no clickedColumn when its field took
      // the original press. F2/Tab still need this exact pressed column.
      selection->setCurrentIndex(current, QItemSelectionModel::NoUpdate);
    }
    if (!alive || !presenter || !selection || !current.isValid()) return;
    if (!qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT"))
      qInfo().noquote() << QStringLiteral("Native model field selected: %1 row=%2 column=%3 selected=%4 rows=%5")
          .arg(view->objectName()).arg(current.row()).arg(current.column())
          .arg(selection->isSelected(current)).arg(selection->selectedRows().size());
    scheduleRefresh();
  }
  void modelSelection(NSTableView* table) {
    QPointer<AppKitWidgetView> alive(this);
    const int id = modelBinding(table);
    const bool trace = !qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT");
    if (trace) qDebug().noquote() << QStringLiteral("Native model selection: binding=%1 selected=%2 count=%3 clicked=%4 refreshing=%5")
        .arg(id).arg(table.selectedRow).arg(table.selectedRowIndexes.count).arg(table.clickedRow).arg(m_refreshing);
    if (m_refreshing || id < 0 || static_cast<std::size_t>(id) >= m_bindings.size()) { return; }
    // Pending edits elsewhere end first. In a dialog the table keeps the
    // keyboard focus for arrow keys and F2, as the shared view does
    // (mitcad#814); document cards leave keys to the document window unless
    // the table has them already, for its next arrow, Return or rename key
    // (mitcad#816).
    NSResponder* focus = [table.window isKindOfClass:MitcadNativeSecondaryWindow.class] ? table : nil;
    // A selection that keeps the edited cell's row keeps its edit (mitcad#814).
    NSTextField* edited = mac::editedCellField(table);
    const NSInteger editedRow = edited != nil ? [table rowForView:edited] : -1;
    const bool keepEdit = editedRow >= 0 && [table.selectedRowIndexes containsIndex:static_cast<NSUInteger>(editedRow)];
    if (edited != nil) {
      mac::logNativeCellEdit(keepEdit ? "a selection keeps the edited row" : "a selection of another row ends the edit",
                             edited);
    }
    if (!keepEdit && table.window != nil && table.window.firstResponder != table &&
        ![table.window makeFirstResponder:focus]) { return; }
    if (alive == nullptr) { return; }
    auto* view = qobject_cast<QAbstractItemView*>(m_bindings[static_cast<std::size_t>(id)].widget);
    if (view == nullptr || view->selectionModel() == nullptr) { return; }
    const NSInteger currentRow = table.clickedRow >= 0 && [table.selectedRowIndexes containsIndex:static_cast<NSUInteger>(table.clickedRow)]
                                     ? table.clickedRow : table.selectedRow;
    const QModelIndex clicked = modelIndex(table, currentRow, std::max<NSInteger>(0, table.clickedColumn));
    QPointer<QAbstractItemView> presenter(view);
    const QPersistentModelIndex current = clicked.isValid() ? clicked : modelIndex(table, table.selectedRow, 0);
    // Selection handlers may use currentItem() to identify the selected
    // row (Configurations' Edit Domain does). Publish that current index
    // first, as a shared view does for a mouse selection, so the selection
    // signal sees the new current row rather than the previous one.
    if (current.isValid()) {
      view->selectionModel()->setCurrentIndex(current, QItemSelectionModel::NoUpdate);
      if (alive == nullptr || presenter == nullptr) { return; }
    }
    QItemSelection selection;
    for (NSUInteger row = table.selectedRowIndexes.firstIndex; row != NSNotFound;
         row = [table.selectedRowIndexes indexGreaterThanIndex:row]) {
      const QModelIndex index = liveIndex(table, nodeAt(table, static_cast<NSInteger>(row)));
      if (index.isValid()) { selection.select(index, index); }
    }
    view->selectionModel()->select(selection, QItemSelectionModel::ClearAndSelect | QItemSelectionModel::Rows);
    if (alive == nullptr || presenter == nullptr) { return; }
    if (trace) {
      const QModelIndex actual = view->selectionModel()->currentIndex();
      qDebug().noquote() << QStringLiteral("Native model selection applied: %1 current=%2,%3 selected=%4 rows=%5")
          .arg(view->objectName()).arg(actual.row()).arg(actual.column())
          .arg(view->selectionModel()->isSelected(actual)).arg(view->selectionModel()->selectedRows().size());
    }
    if (current.isValid()) {
      QMetaObject::invokeMethod(view, "clicked", Qt::DirectConnection, Q_ARG(QModelIndex, QModelIndex(current)));
    }
  }
  QModelIndex modelIndex(NSTableView* table, NSInteger row, NSInteger col) {
    const QModelIndex index = liveIndex(table, nodeAt(table, row));
    return index.isValid() ? index.siblingAtColumn(static_cast<int>(col)) : QModelIndex();
  }
  // A model reset invalidates the native nodes until the queued refresh
  // presents the new rows. Meanwhile a row with a stable identifier still
  // finds its new index, so input then reaches the row it was meant for.
  QModelIndex liveIndex(NSTableView* table, MitcadNativeModelNode* node) {
    if (node == nil) { return {}; }
    if (node->index.isValid()) { return QModelIndex(node->index); }
    const int id = modelBinding(table);
    if (node->identifier.isEmpty() || id < 0 || static_cast<std::size_t>(id) >= m_bindings.size()) { return {}; }
    auto* view = qobject_cast<QAbstractItemView*>(m_bindings[static_cast<std::size_t>(id)].widget);
    QAbstractItemModel* model = view == nullptr ? nullptr : view->model();
    if (model == nullptr || model->rowCount(view->rootIndex()) == 0) { return {}; }
    const QModelIndexList found = model->match(model->index(0, 0, view->rootIndex()), NativeControlIdentifierRole,
                                               node->identifier, 1, Qt::MatchExactly | Qt::MatchRecursive);
    if (found.isEmpty()) { return {}; }
    node->index = found.first();
    return found.first();
  }
  void modelActivated(NSTableView* table) {
    QPointer<AppKitWidgetView> alive(this);
    if (table.window != nil && table.window.firstResponder != table && ![table.window makeFirstResponder:nil]) { return; }
    if (alive == nullptr) { return; }
    const int id = modelBinding(table);
    if (id < 0 || static_cast<std::size_t>(id) >= m_bindings.size()) { return; }
    auto* view = qobject_cast<QAbstractItemView*>(m_bindings[static_cast<std::size_t>(id)].widget);
    const QModelIndex index = modelIndex(table, table.clickedRow >= 0 ? table.clickedRow : table.selectedRow,
                                        std::max<NSInteger>(0, table.clickedColumn));
    if (view != nullptr && index.isValid()) {
      if (index.data(NativeEditableRole).isValid() && index.data(NativeEditableRole).toBool()) {
        const NSInteger row = table.clickedRow >= 0 ? table.clickedRow : table.selectedRow;
        const NSInteger column = std::max<NSInteger>(0, table.clickedColumn);
        NSView* cell = [table viewAtColumn:column row:row makeIfNecessary:YES];
        if ([cell isKindOfClass:NSTableCellView.class]) {
          NSTextField* field = ((NSTableCellView*)cell).textField;
          if (field.editable) { [table.window makeFirstResponder:field]; mac::selectEditedText(field); return; }
        }
      }
      QPointer<QAbstractItemView> presenter(view);
      QMetaObject::invokeMethod(view, "doubleClicked", Qt::DirectConnection, Q_ARG(QModelIndex, index));
      if (alive == nullptr || presenter == nullptr) { return; }
      QMetaObject::invokeMethod(view, "activated", Qt::DirectConnection, Q_ARG(QModelIndex, index));
    }
  }
  void modelContext(NSTableView* table, const NSPoint& point) {
    QPointer<AppKitWidgetView> alive(this);
    const int id = modelBinding(table);
    if (id < 0 || static_cast<std::size_t>(id) >= m_bindings.size()) { return; }
    auto* view = qobject_cast<QAbstractItemView*>(m_bindings[static_cast<std::size_t>(id)].widget);
    const NSInteger row = [table rowAtPoint:point];
    const QModelIndex index = modelIndex(table, row, std::max<NSInteger>(0, [table columnAtPoint:point]));
    if (view != nullptr && index.isValid()) {
      QPointer<QAbstractItemView> presenter(view);
      view->selectionModel()->setCurrentIndex(index, QItemSelectionModel::ClearAndSelect | QItemSelectionModel::Rows);
      if (alive == nullptr || presenter == nullptr) { return; }
      // Existing custom-context handlers resolve indexAt against the
      // presenter view's layout, which remains the authoritative model.
      view->doItemsLayout();
      const QPoint local = view->visualRect(index).center();
      QMetaObject::invokeMethod(view, "customContextMenuRequested", Qt::DirectConnection, Q_ARG(QPoint, local));
    }
  }
  void modelExpanded(NSOutlineView* table, MitcadNativeModelNode* node, bool expanded) {
    const int id = modelBinding(table);
    if (m_refreshing || node == nil || id < 0 || static_cast<std::size_t>(id) >= m_bindings.size()) { return; }
    if (auto* view = qobject_cast<QTreeView*>(m_bindings[static_cast<std::size_t>(id)].widget)) {
      view->setExpanded(node->index, expanded);
    }
  }
  // Row drags within an outline whose presenter opted in (mitcad#816). The
  // presenter decides every target; the outline only carries the gesture,
  // with the row under the pointer as the target as in the default tree.
  const NativeRowMoves* rowMoves(NSTableView* table) const {
    const int id = modelBinding(table);
    if (id < 0 || static_cast<std::size_t>(id) >= m_bindings.size()) { return nullptr; }
    auto* view = qobject_cast<QAbstractItemView*>(m_bindings[static_cast<std::size_t>(id)].widget.data());
    if (view == nullptr) { return nullptr; }
    const auto found = rowMovesRegistry().constFind(view);
    return found == rowMovesRegistry().cend() ? nullptr : &found.value();
  }
  bool rowsDraggable(NSTableView* table, NSIndexSet* rows) {
    const NativeRowMoves* moves = rowMoves(table);
    if (moves == nullptr || !moves->draggable || rows.count != 1) { return false; }
    const auto draggable = moves->draggable;
    return draggable(liveIndex(table, nodeAt(table, static_cast<NSInteger>(rows.firstIndex))));
  }
  id<NSPasteboardWriting> rowDragWriter(NSTableView* table, MitcadNativeModelNode* node) {
    const NativeRowMoves* moves = rowMoves(table);
    const QModelIndex index = liveIndex(table, node);
    if (moves == nullptr || !moves->draggable) { return nil; }
    const auto draggable = moves->draggable;
    if (!draggable(index)) { return nil; }
    m_draggedRow = index;
    NSPasteboardItem* item = [NSPasteboardItem new];
    [item setString:ns(index.data(NativeControlIdentifierRole).toString()) forType:kNativeRowDragType];
    return item;
  }
  void rowDragBegan(NSTableView* table) {
    const NativeRowMoves* moves = rowMoves(table);
    if (moves == nullptr || !m_draggedRow.isValid() || m_rowDragging) { return; }
    m_rowDragging = true;
    if (moves->dragging) {
      const auto dragging = moves->dragging;
      dragging(QModelIndex(m_draggedRow), true);
    }
  }
  MitcadNativeModelNode* rowUnder(NSOutlineView* table, id<NSDraggingInfo> info) {
    const NSInteger row = [table rowAtPoint:[table convertPoint:info.draggingLocation fromView:nil]];
    return row < 0 ? nil : (MitcadNativeModelNode*)[table itemAtRow:row];
  }
  NSDragOperation rowDropValidate(NSOutlineView* table, id<NSDraggingInfo> info, id proposed, NSInteger child) {
    const NativeRowMoves* moves = rowMoves(table);
    if (moves == nullptr || !moves->allowed || info.draggingSource != table || !m_draggedRow.isValid()) {
      return NSDragOperationNone;
    }
    MitcadNativeModelNode* over = rowUnder(table, info);
    const auto allowed = moves->allowed;
    if (over == nil || !allowed(QModelIndex(m_draggedRow), liveIndex(table, over))) { return NSDragOperationNone; }
    // Never between rows: the presenter maps a design to its own folder.
    if (over != proposed || child != NSOutlineViewDropOnItemIndex) {
      [table setDropItem:over dropChildIndex:NSOutlineViewDropOnItemIndex];
    }
    return NSDragOperationMove;
  }
  bool rowDropAccept(NSOutlineView* table, id<NSDraggingInfo> info) {
    const NativeRowMoves* moves = rowMoves(table);
    if (moves == nullptr || !moves->allowed || !moves->drop || info.draggingSource != table || !m_draggedRow.isValid()) {
      return false;
    }
    MitcadNativeModelNode* over = rowUnder(table, info);
    const auto allowed = moves->allowed;
    const auto drop = moves->drop;
    const QModelIndex dragged(m_draggedRow);
    if (over == nil || !allowed(dragged, liveIndex(table, over))) { return false; }
    m_draggedRow = QPersistentModelIndex();
    drop(dragged, liveIndex(table, over));
    return true;
  }
  void rowDragEnded(NSOutlineView* table, NSPoint screenPoint, NSDragOperation operation) {
    const NativeRowMoves* moves = rowMoves(table);
    const QPersistentModelIndex dragged = m_draggedRow;
    const bool began = m_rowDragging;
    m_draggedRow = QPersistentModelIndex();
    m_rowDragging = false;
    if (moves == nullptr) { return; }
    const auto drop = moves->drop;
    const auto dragging = moves->dragging;
    if (operation == NSDragOperationNone && dragged.isValid() && drop) {
      // A refused release gets the presenter's own feedback.
      MitcadNativeModelNode* over = nil;
      if (table.window != nil) {
        const NSPoint local = [table convertPoint:[table.window convertPointFromScreen:screenPoint] fromView:nil];
        const NSInteger row = [table rowAtPoint:local];
        if (row >= 0) { over = (MitcadNativeModelNode*)[table itemAtRow:row]; }
      }
      drop(QModelIndex(dragged), liveIndex(table, over));
    }
    if (began && dragging) { dragging(QModelIndex(dragged), false); }
  }

protected:
  bool eventFilter(QObject* watched, QEvent* event) override {
    switch (event->type()) {
    case QEvent::ChildAdded:
    case QEvent::ChildRemoved:
    case QEvent::ChildPolished:
      m_structureDirty = true;
      scheduleRefresh();
      break;
    case QEvent::Show:
    case QEvent::Hide:
    case QEvent::EnabledChange:
    case QEvent::ToolTipChange:
    case QEvent::DynamicPropertyChange:
    case QEvent::LayoutRequest:
    case QEvent::StyleChange:
    case QEvent::FontChange:
    case QEvent::ReadOnlyChange:
    case QEvent::UpdateRequest:
      // A view's row visibility and index widgets have no notify signal.
      // Its update/layout events provide an early check before the fallback.
      if (qobject_cast<QAbstractItemView*>(watched) != nullptr) { checkModelPresentation(); }
      scheduleRefresh();
      break;
    default: break;
    }
    return false;
  }

private:
  struct Binding {
    QPointer<QWidget> widget;
    NSView* view = nil;
    NSControl* control = nil;
    NSStepper* stepper = nil;
    NSLayoutConstraint* fieldWidth = nil; // a spin box or line edit field's minimum width
    QString fieldMeasured;                // the shared text fieldWidth last measured
    NSImageView* image = nil;
    NSLayoutConstraint* imageHeight = nil;
    NSProgressIndicator* progress = nil;
    NSTextView* text = nil;
    NSTextView* linkText = nil;
    NSLayoutConstraint* listHeight = nil;
    NSTabView* tabs = nil;
    MitcadNativePaneSelector* panes = nil;
    NSTableView* table = nil;
    NSMutableArray<MitcadNativeModelNode*>* nodes = nil;
    QPointer<QAbstractItemModel> model;
    QPointer<QItemSelectionModel> selection;
    QPointer<QTextDocument> document;
    QPersistentModelIndex rootIndex;
    std::vector<QMetaObject::Connection> connections;
    std::vector<QMetaObject::Connection> modelConnections;
    std::vector<QMetaObject::Connection> selectionConnections;
    QStringList items;
    QString labelText;
    QFont labelFont;
    QString buttonTip;
    QKeySequence keySequence;
    int buttonStyle = -1;
    int textRevision = -1;
    int labelFormat = -1;
    bool labelBuddy = false;
    Qt::TextInteractionFlags labelInteraction;
    quint64 presentation = 0;
    qint64 pixmapKey = 0;
    qint64 iconKey = 0;
    bool presentationInitialized = false;
    bool dirty = true;
    bool selectionDirty = true;
    mac::NativeColumnState sections; // the shared header last applied to the columns
    int textTone = 0;                // a label's applied semantic tone (nativeTextToneKind)
    bool scrollMoved = true;         // the shared view's vertical scroll moved (followSharedScroll)
  };
  void scheduleRefresh() {
    if (m_source != nullptr && !m_refreshTimer.isActive()) { m_refreshTimer.start(0); }
  }
  // The field widens to its own cell's size for the whole text, in its
  // actual font, bezel and padding, up to limit, and does not narrow again
  // while the text changes, as Qt's fixed hint.
  static void widenToText(Binding& binding, const QString& text, CGFloat limit) {
    if (binding.fieldWidth == nil || binding.fieldMeasured == text) { return; }
    binding.fieldMeasured = text;
    NSTextFieldCell* probe = [((NSTextField*)binding.control).cell copy];
    probe.stringValue = ns(text);
    CGFloat needed = std::ceil([probe cellSizeForBounds:NSMakeRect(0, 0, CGFLOAT_MAX, CGFLOAT_MAX)].width);
    needed = std::min(needed, limit);
    if (needed > binding.fieldWidth.constant) {
      binding.fieldWidth.constant = needed;
      ++g_widgetPropertyWrites;
    }
  }
  // A spin box's shared text keeps its prefix and suffix, such as an
  // option's " mm", whole (mitcad#975).
  void presentSpinText(Binding& binding, const QAbstractSpinBox* spin) {
    NSTextField* field = (NSTextField*)binding.control;
    NATIVE_SET(field, editable, !spin->isReadOnly());
    const QString text = spin->text();
    if (field.currentEditor == nil) { NATIVE_STRING(field, stringValue, text); }
    widenToText(binding, text, CGFLOAT_MAX);
  }
  // A line edit's text that fits its presenter's Qt hint stays whole, as
  // in its Qt field, such as a committed multiword appearance name; AppKit
  // draws it in its own font and cell insets, which can need a few points
  // more than the shared 17-character width (mitcad#982). A longer text
  // keeps the shared width and scrolls on its single line, as in Qt.
  void presentLineText(Binding& binding, const QLineEdit* edit) {
    const QString text = edit->text();
    if (edit->echoMode() != QLineEdit::Normal || text.isEmpty() ||
        QFontMetrics(edit->font()).horizontalAdvance(text) > edit->sizeHint().width()) {
      return;
    }
    widenToText(binding, text, static_cast<CGFloat>(edit->maximumWidth()));
  }
  void observeModel(Binding& binding, QAbstractItemModel* model, int id) {
    if (binding.model == model) { return; }
    for (const auto& connection : binding.modelConnections) { disconnect(connection); }
    binding.modelConnections.clear();
    binding.model = model;
    binding.dirty = true;
    binding.selectionDirty = true;
    if (model == nullptr) { return; }
    const auto generation = m_generation;
    auto dirty = [this, id, generation, observed = QPointer<QAbstractItemModel>(model)] {
      if (generation != m_generation || static_cast<std::size_t>(id) >= m_bindings.size()) { return; }
      if (m_bindings[static_cast<std::size_t>(id)].model != observed) { return; }
      m_bindings[static_cast<std::size_t>(id)].dirty = true;
      m_bindings[static_cast<std::size_t>(id)].selectionDirty = true;
      scheduleRefresh();
    };
    auto& c = binding.modelConnections;
    c.push_back(connect(model, &QAbstractItemModel::dataChanged, this, dirty));
    c.push_back(connect(model, &QAbstractItemModel::headerDataChanged, this, dirty));
    c.push_back(connect(model, &QAbstractItemModel::modelReset, this, dirty));
    c.push_back(connect(model, &QAbstractItemModel::rowsInserted, this, dirty));
    c.push_back(connect(model, &QAbstractItemModel::rowsRemoved, this, dirty));
    c.push_back(connect(model, &QAbstractItemModel::rowsMoved, this, dirty));
    c.push_back(connect(model, &QAbstractItemModel::columnsInserted, this, dirty));
    c.push_back(connect(model, &QAbstractItemModel::columnsRemoved, this, dirty));
    c.push_back(connect(model, &QAbstractItemModel::columnsMoved, this, dirty));
    c.push_back(connect(model, &QAbstractItemModel::layoutChanged, this, dirty));
    c.push_back(connect(model, &QObject::destroyed, this, dirty));
  }
  void observeSelection(Binding& binding, QAbstractItemView* view, int id) {
    if (binding.selection == view->selectionModel()) { return; }
    for (const auto& connection : binding.selectionConnections) { disconnect(connection); }
    binding.selectionConnections.clear();
    binding.selection = view->selectionModel();
    binding.selectionDirty = true;
    if (!binding.selection) { return; }
    const auto generation = m_generation;
    auto changed = [this, id, generation, observed = binding.selection] {
      if (generation != m_generation || static_cast<std::size_t>(id) >= m_bindings.size()) { return; }
      if (m_bindings[static_cast<std::size_t>(id)].selection != observed) { return; }
      m_bindings[static_cast<std::size_t>(id)].selectionDirty = true;
      scheduleRefresh();
    };
    binding.selectionConnections.push_back(connect(binding.selection, &QItemSelectionModel::selectionChanged, this, changed));
    binding.selectionConnections.push_back(connect(binding.selection, &QItemSelectionModel::currentChanged, this, changed));
  }
  quint64 modelPresentation(QAbstractItemView* view) const {
    // No model data, strings, node allocation or AppKit messages. The
    // fallback only checks otherwise silent row hiding/index-widget swaps.
    quint64 value = 1469598103934665603ULL;
    const auto mix = [&value](quint64 part) { value = (value ^ part) * 1099511628211ULL; };
    mix(view->isEnabled());
    const auto describe = [&](auto&& self, const QModelIndex& parent) -> void {
      const int count = view->model()->rowCount(parent);
      mix(static_cast<quint64>(count));
      for (int row = 0; row < count; ++row) {
        const QModelIndex index = view->model()->index(row, 0, parent);
        if (auto* tree = qobject_cast<QTreeView*>(view)) {
          mix(tree->isRowHidden(row, parent));
          mix(tree->isExpanded(index));
          mix(tree->isFirstColumnSpanned(row, parent));
        }
        if (auto* table = qobject_cast<QTableView*>(view)) { mix(table->isRowHidden(row)); }
        if (auto* list = qobject_cast<QListView*>(view)) { mix(list->isRowHidden(row)); }
        for (int col = 0; col < view->model()->columnCount(parent); ++col) {
          mix(reinterpret_cast<quintptr>(view->indexWidget(index.siblingAtColumn(col))));
        }
        self(self, index);
      }
    };
    if (view->model() != nullptr) { describe(describe, view->rootIndex()); }
    return value;
  }
  void checkModelPresentation() {
    for (auto& binding : m_bindings) {
      auto* view = qobject_cast<QAbstractItemView*>(binding.widget.data());
      if (view == nullptr) { continue; }
      const quint64 value = modelPresentation(view);
      if (binding.presentation != value) { binding.dirty = true; binding.presentation = value; }
    }
  }
  int bind(QWidget* widget, NSView* view, NSControl* control = nil) {
    // The parent grid is built next: give it the source's actual visibility
    // immediately, before filtered settings join the window's layout graph.
    view.hidden = widget != m_source && widget->isHidden();
    Binding binding;
    binding.widget = widget;
    binding.view = view;
    binding.control = control;
    const int id = static_cast<int>(m_bindings.size());
    m_bindings.push_back(std::move(binding));
    auto changed = [this] { scheduleRefresh(); };
    auto& c = m_bindings.back().connections;
    if (auto* button = qobject_cast<QAbstractButton*>(widget)) { c.push_back(connect(button, &QAbstractButton::toggled, this, changed)); }
    if (auto* edit = qobject_cast<QLineEdit*>(widget)) { c.push_back(connect(edit, &QLineEdit::textChanged, this, changed)); }
    if (auto* combo = qobject_cast<QComboBox*>(widget)) {
      c.push_back(connect(combo, &QComboBox::currentIndexChanged, this, changed));
      c.push_back(connect(combo, &QComboBox::editTextChanged, this, changed));
    }
    if (auto* group = qobject_cast<QGroupBox*>(widget)) { c.push_back(connect(group, &QGroupBox::toggled, this, changed)); }
    if (auto* spin = qobject_cast<QSpinBox*>(widget)) { c.push_back(connect(spin, &QSpinBox::valueChanged, this, changed)); }
    if (auto* spin = qobject_cast<QDoubleSpinBox*>(widget)) { c.push_back(connect(spin, &QDoubleSpinBox::valueChanged, this, changed)); }
    if (auto* slider = qobject_cast<QSlider*>(widget)) { c.push_back(connect(slider, &QSlider::valueChanged, this, changed)); }
    if (auto* progress = qobject_cast<QProgressBar*>(widget)) { c.push_back(connect(progress, &QProgressBar::valueChanged, this, changed)); }
    if (auto* tabs = qobject_cast<QTabWidget*>(widget)) { c.push_back(connect(tabs, &QTabWidget::currentChanged, this, changed)); }
    if (auto* bar = qobject_cast<QTabBar*>(widget)) { c.push_back(connect(bar, &QTabBar::currentChanged, this, changed)); }
    if (auto* pages = qobject_cast<QStackedWidget*>(widget)) { c.push_back(connect(pages, &QStackedWidget::currentChanged, this, changed)); }
    if (auto* edit = qobject_cast<QKeySequenceEdit*>(widget)) { c.push_back(connect(edit, &QKeySequenceEdit::keySequenceChanged, this, changed)); }
    if (auto* edit = qobject_cast<QPlainTextEdit*>(widget)) { c.push_back(connect(edit, &QPlainTextEdit::textChanged, this, changed)); }
    if (auto* edit = qobject_cast<QPlainTextEdit*>(widget); edit != nullptr && dynamic_cast<NativeTextEditorCommands*>(edit)) {
      c.push_back(connect(edit, &QPlainTextEdit::textChanged, this, [this, edit, id] {
        if (m_refreshing || m_source == nullptr || m_source->focusWidget() != edit) return;
        const Binding& current = m_bindings[static_cast<std::size_t>(id)];
        if (current.text != nil && current.text.window.firstResponder != current.text) m_textFocusRequested = edit;
      }));
    }
    if (auto* edit = qobject_cast<QTextEdit*>(widget)) { c.push_back(connect(edit, &QTextEdit::textChanged, this, changed)); }
    if (auto* tree = qobject_cast<QTreeView*>(widget)) {
      const auto generation = m_generation;
      auto expanded = [this, id, generation] {
        if (generation != m_generation || static_cast<std::size_t>(id) >= m_bindings.size()) { return; }
        m_bindings[static_cast<std::size_t>(id)].dirty = true;
        scheduleRefresh();
      };
      c.push_back(connect(tree, &QTreeView::expanded, this, expanded));
      c.push_back(connect(tree, &QTreeView::collapsed, this, expanded));
    }
    if (control == nil || control == view) { view.accessibilityIdentifier = ns(widget->objectName()); }
    else { control.accessibilityIdentifier = ns(widget->objectName()); }
    view.toolTip = ns(widget->toolTip());
    if (control != nil) { control.tag = id; }
    return id;
  }
  void target(NSControl* control) {
    control.target = m_target;
    control.action = @selector(perform:);
  }
  NSView* renderLayout(QLayout* layout) {
    if (layout == nullptr) { return nil; }
    // A window's grids, forms and boxes share one sizing contract
    // (mitcad#812): Qt's minimum/preferred sizes, stretch, size policies,
    // spans, field growth and label alignment. Pages hosted in the document's
    // cards (inspector, sketch palette) keep their stacks within the cards'
    // own width rules.
    if (!m_source->isWindow() && !m_source->property("nativeLayoutContract").toBool()) {
      return renderStackLayout(layout);
    }
    // The axis names take the spare width; On and Reverse keep adjacent,
    // content-sized columns. This native adapter leaves the Qt grid intact.
    auto* axes = qobject_cast<QGridLayout*>(layout);
    const bool axisColumns = axes != nullptr && axes->columnCount() == 3 &&
        axes->parentWidget() != nullptr && axes->parentWidget()->objectName() == QStringLiteral("navigationSpaceMouse");
    return mac::layout::build(layout, [this](QLayoutItem* item) { return renderItem(item); }, false,
                              m_source->property("nativeResponsiveLayout").toBool(), axisColumns);
  }
  NSView* renderStackLayout(QLayout* layout) {
    if (auto* grid = qobject_cast<QGridLayout*>(layout)) {
      NSStackView* rows = stack();
      for (int row = 0; row < grid->rowCount(); ++row) {
        NSStackView* line = stack(true);
        if (m_source->objectName() == QStringLiteral("sketchPalette") && grid->columnCount() == 6) {
          line.distribution = NSStackViewDistributionFillEqually;
        }
        for (int col = 0; col < grid->columnCount(); ++col) {
          QLayoutItem* item = grid->itemAtPosition(row, col);
          if (item == nullptr) { continue; }
          int itemIndex = -1;
          for (int i = 0; i < grid->count(); ++i) { if (grid->itemAt(i) == item) { itemIndex = i; break; } }
          int itemRow = row, itemCol = col, rowSpan, colSpan;
          if (itemIndex >= 0) { grid->getItemPosition(itemIndex, &itemRow, &itemCol, &rowSpan, &colSpan); }
          if (itemRow == row && itemCol == col) {
            add(line, renderItem(item));
          }
        }
        add(rows, line);
      }
      return rows;
    }
    if (auto* form = qobject_cast<QFormLayout*>(layout)) {
      NSStackView* rows = stack();
      for (int row = 0; row < form->rowCount(); ++row) {
        QLayoutItem* field = form->itemAt(row, QFormLayout::FieldRole);
        QLayoutItem* caption = form->itemAt(row, QFormLayout::LabelRole);
        QLayoutItem* spanning = form->itemAt(row, QFormLayout::SpanningRole);
        if (spanning != nullptr) { add(rows, renderItem(spanning)); }
        else {
          NSStackView* line = stack(true);
          if (caption != nullptr) {
            NSView* nativeCaption = renderItem(caption);
            if (auto* text = qobject_cast<QLabel*>(caption->widget()); text && !text->wordWrap()) {
              [nativeCaption.widthAnchor constraintGreaterThanOrEqualToConstant:text->sizeHint().width()].active = YES;
            }
            add(line, nativeCaption);
          }
          if (field != nullptr) { add(line, renderItem(field)); }
          add(rows, line);
        }
      }
      return rows;
    }
    auto* box = qobject_cast<QBoxLayout*>(layout);
    const bool horizontal = box != nullptr && (box->direction() == QBoxLayout::LeftToRight || box->direction() == QBoxLayout::RightToLeft);
    NSStackView* result = stack(horizontal);
    const QMargins margins = layout->contentsMargins();
    result.edgeInsets = NSEdgeInsetsMake(margins.top(), margins.left(), margins.bottom(), margins.right());
    result.spacing = std::max(6, layout->spacing());
    for (int i = 0; i < layout->count(); ++i) { add(result, renderItem(layout->itemAt(i))); }
    return result;
  }
  // Orders the box's bound buttons by role again, also after additions,
  // removals and role changes. Escape belongs to the rejecting buttons;
  // the refresh gives Return to the presenter's default button.
  void arrangeButtons(QDialogButtonBox* box, NSView* row) {
    std::vector<NSView*> natives;
    for (QAbstractButton* button : box->buttons()) {
      NSView* native = nil;
      for (const Binding& binding : m_bindings) {
        if (binding.widget.data() == button) { native = binding.view; break; }
      }
      if ([native isKindOfClass:NSButton.class]) {
        NSButton* control = (NSButton*)native;
        if (box->buttonRole(button) == QDialogButtonBox::RejectRole) {
          if (![control.keyEquivalent isEqualToString:@"\r"]) { control.keyEquivalent = @"\033"; }
        } else if ([control.keyEquivalent isEqualToString:@"\033"]) {
          control.keyEquivalent = @"";
        }
      }
      natives.push_back(native);
    }
    mac::layout::arrangeButtons(row, box, natives);
  }
  NSView* renderItem(QLayoutItem* item) {
    if (item == nullptr) { return nil; }
    if (item->widget() != nullptr) { return renderWidget(item->widget()); }
    if (item->layout() != nullptr) { return renderLayout(item->layout()); }
    NSView* spring = [NSView new];
    [spring setContentHuggingPriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationHorizontal];
    return spring;
  }
  NSView* renderWidget(QWidget* widget) {
    if (widget == nullptr) { return nil; }
    if (auto* settings = qobject_cast<SettingsWindow*>(widget)) {
      return renderWidget(settings->findChild<QStackedWidget*>(QString(), Qt::FindDirectChildrenOnly));
    }
    if (auto* screenshot = qobject_cast<report::ScreenshotView*>(widget)) {
      MitcadNativeScreenshot* view = [MitcadNativeScreenshot new];
      view->presenter = screenshot;
      [view.heightAnchor constraintGreaterThanOrEqualToConstant:240].active = YES;
      view.accessibilityLabel = ns(tr("Screenshot. Drag to choose the area to include."));
      const int id = bind(widget, view);
      m_bindings[static_cast<std::size_t>(id)].connections.push_back(
        connect(screenshot, &report::ScreenshotView::cropped, this, [view] { [view refresh]; }));
      [view refresh];
      return view;
    }
    if (auto guide = mac::createNativeNavigationGuide(widget)) {
      NSView* view = (__bridge NSView*)guide->nativeView();
      bind(widget, view);
      m_artworkViews.push_back(std::move(guide));
      return view;
    }
    if (auto rules = mac::createNativeRuleView(widget)) {
      widget->setProperty("nativeRuleArtwork", true);
      NSView* view = (__bridge NSView*)rules->nativeView();
      bind(widget, view);
      m_ruleViews.push_back(std::move(rules));
      return view;
    }
    if (auto* text = qobject_cast<QLabel*>(widget)) {
      NSStackView* contents = stack();
      NSTextField* caption = label(labelText(text), false);
      if (!text->wordWrap() && !qt(caption.stringValue).contains(QLatin1Char('\n'))) {
        caption.usesSingleLineMode = YES;
        caption.maximumNumberOfLines = 1;
        caption.cell.wraps = NO;
        caption.lineBreakMode = NSLineBreakByTruncatingTail;
        [caption setContentCompressionResistancePriority:NSLayoutPriorityDefaultHigh
                                         forOrientation:NSLayoutConstraintOrientationHorizontal];
      }
      caption.font = labelFont(text->font(), QApplication::font(text));
      NSImageView* preview = [NSImageView new];
      preview.imageScaling = NSImageScaleProportionallyUpOrDown;
      preview.hidden = text->pixmap().isNull();
      caption.hidden = !text->pixmap().isNull();
      add(contents, caption);
      add(contents, preview);
      NSLayoutConstraint* imageHeight = [preview.heightAnchor constraintGreaterThanOrEqualToConstant:labelPixmapHeight(text)];
      imageHeight.active = YES;
      const int id = bind(widget, contents, caption);
      m_bindings[static_cast<std::size_t>(id)].image = preview;
      m_bindings[static_cast<std::size_t>(id)].imageHeight = imageHeight;
      return contents;
    }
    if (auto* edit = qobject_cast<QLineEdit*>(widget)) {
      NSTextField* field = edit->echoMode() != QLineEdit::Normal
                               ? [[NSSecureTextField alloc] initWithFrame:NSZeroRect]
                               : [[NSTextField alloc] initWithFrame:NSZeroRect];
      singleScrollingLine(field);
      field.stringValue = ns(edit->text());
      field.delegate = m_target;
      field.editable = !edit->isReadOnly();
      field.placeholderString = ns(edit->placeholderText());
      const int id = bind(widget, field, field);
      Binding& binding = m_bindings[static_cast<std::size_t>(id)];
      // Below required, as the shared layout's own limits: a window that
      // cannot widen keeps the field at its width instead of breaking.
      binding.fieldWidth = [field.widthAnchor constraintGreaterThanOrEqualToConstant:0];
      binding.fieldWidth.priority = NSLayoutPriorityRequired - 1;
      binding.fieldWidth.active = YES;
      return field;
    }
    if (auto* edit = qobject_cast<QKeySequenceEdit*>(widget)) {
      MitcadNativeShortcutField* field = [[MitcadNativeShortcutField alloc] initWithFrame:NSZeroRect];
      field->edit = edit;
      field.editable = NO;
      field.selectable = NO;
      field.stringValue = ns(edit->keySequence().toString(QKeySequence::NativeText));
      field.placeholderString = ns(tr("Press a shortcut"));
      NSButton* clear = [NSButton buttonWithTitle:ns(tr("Clear")) target:field action:@selector(clearShortcut:)];
      clear.toolTip = ns(tr("Clear shortcut; Delete and Backspace can be recorded as keys"));
      NSStackView* row = [NSStackView stackViewWithViews:@[field, clear]];
      row.orientation = NSUserInterfaceLayoutOrientationHorizontal;
      row.spacing = 6;
      [field setContentHuggingPriority:NSLayoutPriorityDefaultLow forOrientation:NSLayoutConstraintOrientationHorizontal];
      bind(widget, row, field);
      return row;
    }
    if (auto* combo = qobject_cast<QComboBox*>(widget)) {
      NSControl* view;
      if (combo->isEditable()) {
        NSComboBox* field = [[NSComboBox alloc] initWithFrame:NSZeroRect];
        field.delegate = m_target;
        view = field;
      } else { view = [[MitcadNativePopup alloc] initWithFrame:NSZeroRect pullsDown:NO]; }
      target(view);
      bind(widget, view, view);
      return view;
    }
    if (auto* button = qobject_cast<QAbstractButton*>(widget)) {
      NSButton* view;
      if (button->property("nativeHoverEvents").toBool()) {
        MitcadNativeHoverButton* hover = [[MitcadNativeHoverButton alloc] initWithFrame:NSZeroRect];
        hover->presenter = button;
        hover.title = ns(title(button->text()));
        hover.target = m_target;
        hover.action = @selector(perform:);
        view = hover;
      } else if (tileButton(button)) {
        MitcadNativeTileButton* tile = [[MitcadNativeTileButton alloc] initWithIconSize:buttonIconPixels(button)];
        tile.target = m_target;
        tile.action = @selector(perform:);
        view = tile;
      } else if (button->property(NativeColorFieldProperty).isValid()) {
        auto* field = [[MitcadNativeColorField alloc] initWithFrame:NSZeroRect];
        field.font = [NSFont systemFontOfSize:NSFont.systemFontSize];
        // This is an expanding input, not an empty-caption action button.
        // Its bezel must fill the shared editor's cell so the inherited
        // value's words and the whole pointer target remain reachable.
        [field setContentHuggingPriority:NSLayoutPriorityDefaultLow
                         forOrientation:NSLayoutConstraintOrientationHorizontal];
        field.target = m_target;
        field.action = @selector(perform:);
        view = field;
      } else {
        view = [[MitcadNativeActionButton alloc] initWithFrame:NSZeroRect];
        view.title = ns(title(button->text()));
        view.target = m_target;
        view.action = @selector(perform:);
      }
      if (!tileButton(button)) { view.bezelStyle = NSBezelStyleRounded; }
      if (qobject_cast<QCheckBox*>(widget) != nullptr) { [view setButtonType:NSButtonTypeSwitch]; }
      else if (qobject_cast<QRadioButton*>(widget) != nullptr) { [view setButtonType:NSButtonTypeRadio]; }
      else if (button->isCheckable()) { [view setButtonType:NSButtonTypePushOnPushOff]; }
      if (!button->icon().isNull()) {
        view.image = image(button->icon().pixmap(buttonIconPixels(button), buttonIconPixels(button)));
      }
      updateButtonPresentation(view, button);
      updateColorField(view, button);
      if (widget->objectName().startsWith(QStringLiteral("palette_"))) {
        [view.widthAnchor constraintGreaterThanOrEqualToConstant:34].active = YES;
        [view.heightAnchor constraintGreaterThanOrEqualToConstant:30].active = YES;
      }
      if (auto* box = qobject_cast<QDialogButtonBox*>(widget->parentWidget())) {
        const auto role = box->buttonRole(button);
        if (role == QDialogButtonBox::RejectRole) { view.keyEquivalent = @"\033"; }
      }
      bind(widget, view, view);
      return view;
    }
    if (qobject_cast<QSpinBox*>(widget) != nullptr || qobject_cast<QDoubleSpinBox*>(widget) != nullptr) {
      NSStackView* row = stack(true);
      NSTextField* field = [[NSTextField alloc] initWithFrame:NSZeroRect];
      field.delegate = m_target;
      singleScrollingLine(field);
      NSStepper* stepper = [[NSStepper alloc] initWithFrame:NSZeroRect];
      add(row, field);
      add(row, stepper);
      const int id = bind(widget, row, field);
      Binding& binding = m_bindings[static_cast<std::size_t>(id)];
      binding.stepper = stepper;
      stepper.tag = id;
      target(stepper);
      binding.fieldWidth = [field.widthAnchor constraintGreaterThanOrEqualToConstant:80];
      binding.fieldWidth.active = YES;
      return row;
    }
    if (auto* slider = qobject_cast<QSlider*>(widget)) {
      MitcadNativeSlider* view = [[MitcadNativeSlider alloc] initWithFrame:NSZeroRect];
      view.minValue = slider->minimum();
      view.maxValue = slider->maximum();
      view.integerValue = slider->value();
      view.target = m_target;
      view.action = @selector(perform:);
      view.keyboardStep = slider->singleStep();
      view.continuous = !slider->property("nativeSliderOnRelease").toBool();
      bind(widget, view, view);
      return view;
    }
    if (qobject_cast<QProgressBar*>(widget) != nullptr) {
      NSProgressIndicator* view = [[NSProgressIndicator alloc] initWithFrame:NSZeroRect];
      view.style = NSProgressIndicatorStyleBar;
      const int id = bind(widget, view);
      m_bindings[static_cast<std::size_t>(id)].progress = view;
      return view;
    }
    if (auto* group = qobject_cast<QGroupBox*>(widget)) {
      NSStackView* body = stack();
      if (group->isCheckable()) {
        NSButton* check = [NSButton checkboxWithTitle:ns(title(group->title())) target:m_target action:@selector(perform:)];
        bind(widget, body, check);
        add(body, check);
      } else if (!group->title().isEmpty()) {
        NSTextField* caption = label(title(group->title()));
        caption.font = [NSFont systemFontOfSize:NSFont.systemFontSize weight:NSFontWeightSemibold];
        add(body, caption);
        bind(widget, body);
      }
      add(body, renderLayout(group->layout()));
      return body;
    }
    if (auto* buttons = qobject_cast<QDialogButtonBox*>(widget)) {
      // The macOS role order, not buttons()'s grouping by role (mitcad#817).
      for (QAbstractButton* button : buttons->buttons()) { renderWidget(button); }
      NSView* row = mac::layout::buttonRow();
      bind(widget, row);
      arrangeButtons(buttons, row);
      return row;
    }
    if (auto* toolbar = qobject_cast<QToolBar*>(widget)) {
      NSStackView* row = stack(true);
      row.spacing = 2;
      bind(widget, row);
      for (QAction* action : toolbar->actions()) {
        if (action->isSeparator()) {
          NSBox* divider = [NSBox new];
          divider.boxType = NSBoxSeparator;
          add(row, divider);
        } else if (QWidget* control = toolbar->widgetForAction(action)) {
          // QToolBar's generated buttons have no name of their own.
          // Retain the presenter's stable action identity in AppKit too.
          if (control->objectName().isEmpty()) control->setObjectName(action->objectName());
          NSView* native = renderWidget(control);
          if ([native isKindOfClass:NSButton.class]) {
            NSButton* button = (NSButton*)native;
            button.bordered = NO;
            [button.widthAnchor constraintEqualToConstant:24].active = YES;
            [button.heightAnchor constraintEqualToConstant:24].active = YES;
          }
          add(row, native);
        }
      }
      return row;
    }
    if (widget->property("nativeTileCatalogue").toBool()) {
      // A tile catalogue keeps its groups, rows and pictured grid (mitcad#813).
      if (NSView* catalogue = mac::nativeTileCatalogue(widget, [this](QWidget* child) { return renderWidget(child); },
                                                       m_tileViews)) {
        bind(widget, catalogue);
        return catalogue;
      }
    }
    if (auto* view = qobject_cast<QAbstractItemView*>(widget)) { return renderModel(view); }
    if (qobject_cast<QPlainTextEdit*>(widget) != nullptr || qobject_cast<QTextEdit*>(widget) != nullptr) {
      MitcadNativeTextView* view = [[MitcadNativeTextView alloc] initWithFrame:NSMakeRect(0, 0, 440, 130)];
      view.font = [NSFont systemFontOfSize:NSFont.systemFontSize];
      view.richText = NO;
      if (dynamic_cast<NativeTextEditorCommands*>(widget) != nullptr) {
        // Formula literals and operators must survive typing and focus loss
        // verbatim; prose substitutions can change valid expressions.
        view.automaticQuoteSubstitutionEnabled = NO;
        view.automaticDashSubstitutionEnabled = NO;
        view.automaticTextReplacementEnabled = NO;
        view.automaticSpellingCorrectionEnabled = NO;
      }
      view.delegate = m_target;
      view.verticallyResizable = YES;
      view.horizontallyResizable = NO;
      view.autoresizingMask = NSViewWidthSizable;
      view.textContainer.widthTracksTextView = YES;
      const int minimum = std::clamp(widget->minimumSizeHint().height(), widget->minimumHeight(), widget->maximumHeight());
      NSScrollView* box = scroll(view, minimum);
      const int preferred = std::clamp(widget->sizeHint().height(), minimum, widget->maximumHeight());
      NSLayoutConstraint* readingHeight = [box.heightAnchor constraintEqualToConstant:preferred];
      readingHeight.priority = 490; // shared reading height yields when the window needs to shrink
      readingHeight.active = YES;
      const int id = bind(widget, box);
      m_bindings[static_cast<std::size_t>(id)].text = view;
      view.identifier = ns(QString::number(id));
      return box;
    }
    if (auto* bar = qobject_cast<QTabBar*>(widget)) {
      NSPopUpButton* control = [[NSPopUpButton alloc] initWithFrame:NSZeroRect pullsDown:NO];
      control.menu.autoenablesItems = NO;
      control.accessibilityLabel = ns(bar->accessibleName().isEmpty() ? tr("Settings category") : bar->accessibleName());
      target(control);
      bind(widget, control, control);
      return control;
    }
    if (auto* tabs = qobject_cast<QTabWidget*>(widget)) { return renderTabs(widget, tabs->count(), [tabs](int i) { return tabs->widget(i); }, [tabs](int i) { return tabs->tabText(i); }); }
    if (auto* pages = qobject_cast<QStackedWidget*>(widget)) {
      const bool named = pages->count() > 0 && !pages->widget(0)->property("nativePaneTitle").toString().isEmpty();
      if (named) {
        return renderTabs(widget, pages->count(), [pages](int i) { return pages->widget(i); },
                          [pages](int i) { return pages->widget(i)->property("nativePaneTitle").toString(); },
                          [pages](int i) { return pages->widget(i)->property("nativePaneIcon").value<QIcon>(); });
      }
      NSStackView* body = stack();
      bind(widget, body);
      for (int i = 0; i < pages->count(); ++i) { add(body, renderWidget(pages->widget(i))); }
      return body;
    }
    if (auto* area = qobject_cast<QScrollArea*>(widget)) {
      // A body that scrolls only to keep its Qt window on the screen: the
      // secondary window scrolls its whole body (NativeWindowFit.hpp).
      if (area->property("nativeWindowBody").toBool()) { return renderWidget(area->widget()); }
      NSView* content = renderWidget(area->widget());
      const int minimum = std::clamp(area->minimumSizeHint().height(), area->minimumHeight(), area->maximumHeight());
      NSScrollView* view = scroll(content, minimum, area->horizontalScrollBarPolicy() != Qt::ScrollBarAlwaysOff);
      view.hasVerticalScroller = area->verticalScrollBarPolicy() != Qt::ScrollBarAlwaysOff;
      view.autohidesScrollers = area->horizontalScrollBarPolicy() != Qt::ScrollBarAlwaysOn &&
                                area->verticalScrollBarPolicy() != Qt::ScrollBarAlwaysOn;
      view.borderType = area->frameShape() == QFrame::NoFrame ? NSNoBorder : NSBezelBorder;
      const int preferred = std::clamp(area->sizeHint().height(), minimum, area->maximumHeight());
      NSLayoutConstraint* readingHeight = [view.heightAnchor constraintEqualToConstant:preferred];
      readingHeight.priority = 490; // shared preferred height yields to a smaller viewport
      readingHeight.active = YES;
      if (!qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT")) {
        qInfo().noquote() << QStringLiteral("Native scroll contract: %1 minimum height %2, preferred %3, horizontal %4")
            .arg(area->objectName()).arg(minimum).arg(preferred).arg(bool(view.hasHorizontalScroller));
      }
      bind(widget, view);
      return view;
    }
    if (auto* split = qobject_cast<QSplitter*>(widget)) {
      // Panes in the splitter's sizes and stretch, with dividers (mitcad#813).
      NSView* view = mac::nativeSplitView(split, [this](QWidget* pane) { return renderWidget(pane); });
      bind(widget, view);
      return view;
    }
    if (auto* frame = qobject_cast<QFrame*>(widget)) {
      if (frame->frameShape() == QFrame::HLine || frame->frameShape() == QFrame::VLine) {
        NSBox* view = [NSBox new];
        view.boxType = NSBoxSeparator;
        bind(widget, view);
        return view;
      }
    }
    if (widget->property("nativeCompactPanel").toBool()) {
      // A source list stretches into its bounded viewport. A generic form
      // stack sizes by intrinsic heights and can leave project rows clipped.
      auto* toolbar = widget->findChild<QToolBar*>(QString(), Qt::FindDirectChildrenOnly);
      auto* tree = widget->findChild<QTreeView*>(QString(), Qt::FindDirectChildrenOnly);
      if (toolbar && tree) {
        NSView* panel = [NSView new];
        NSView* buttons = renderWidget(toolbar);
        NSView* outline = renderWidget(tree);
        buttons.translatesAutoresizingMaskIntoConstraints = NO;
        outline.translatesAutoresizingMaskIntoConstraints = NO;
        [panel addSubview:buttons];
        [panel addSubview:outline];
        [NSLayoutConstraint activateConstraints:@[
          [buttons.leadingAnchor constraintEqualToAnchor:panel.leadingAnchor],
          [buttons.trailingAnchor constraintLessThanOrEqualToAnchor:panel.trailingAnchor],
          [buttons.topAnchor constraintEqualToAnchor:panel.topAnchor],
          [buttons.heightAnchor constraintEqualToConstant:24],
          [outline.leadingAnchor constraintEqualToAnchor:panel.leadingAnchor],
          [outline.trailingAnchor constraintEqualToAnchor:panel.trailingAnchor],
          [outline.topAnchor constraintEqualToAnchor:buttons.bottomAnchor constant:2],
          [outline.bottomAnchor constraintEqualToAnchor:panel.bottomAnchor]
        ]];
        bind(widget, panel);
        return panel;
      }
    }
    NSView* view = renderLayout(widget->layout());
    if (view == nil) {
      NSStackView* contents = stack();
      const auto children = widget->findChildren<QWidget*>(QString(), Qt::FindDirectChildrenOnly);
      for (QWidget* child : children) { add(contents, renderWidget(child)); }
      if (children.isEmpty()) { reportUnpresented(widget); }
      view = contents;
    }
    bind(widget, view);
    return view;
  }
  NSAttributedString* richLabel(const QLabel* source) {
    QTextDocument document;
    document.setDefaultFont(source->font());
    document.setHtml(source->text());
    if (source->buddy() != nullptr) {
      const QString text = document.toPlainText();
      std::vector<int> markers;
      for (qsizetype i = 0; i < text.size(); ++i) {
        if (text.at(i) == QLatin1Char('&')) {
          markers.push_back(static_cast<int>(i));
          if (i + 1 < text.size() && text.at(i + 1) == QLatin1Char('&')) { ++i; }
        }
      }
      for (auto i = markers.rbegin(); i != markers.rend(); ++i) {
        QTextCursor cursor(&document);
        cursor.setPosition(*i);
        cursor.setPosition(*i + 1, QTextCursor::KeepAnchor);
        cursor.removeSelectedText();
      }
    }
    NSMutableAttributedString* result = [[NSMutableAttributedString alloc] initWithString:ns(document.toPlainText())
      attributes:@{NSFontAttributeName: labelFont(source->font(), QApplication::font(source)),
                   NSForegroundColorAttributeName:mac::nativeTextTone(mac::nativeTextToneKind(source))}];
    for (QTextBlock block = document.begin(); block.isValid(); block = block.next()) {
      for (auto it = block.begin(); !it.atEnd(); ++it) {
        const QTextFragment fragment = it.fragment();
        if (!fragment.isValid()) { continue; }
        const NSRange range = NSMakeRange(static_cast<NSUInteger>(fragment.position()), static_cast<NSUInteger>(fragment.length()));
        if (NSMaxRange(range) <= result.length) {
          const QTextCharFormat format = fragment.charFormat();
          const QFont font = format.font().resolve(source->font());
          [result addAttribute:NSFontAttributeName value:labelFont(font, QApplication::font(source)) range:range];
          if (format.fontUnderline()) [result addAttribute:NSUnderlineStyleAttributeName value:@(NSUnderlineStyleSingle) range:range];
          if (format.isAnchor()) [result addAttribute:NSLinkAttributeName value:ns(format.anchorHref()) range:range];
        }
      }
    }
    return result;
  }
  NSView* renderTabs(QWidget* widget, int count, std::function<QWidget*(int)> page, std::function<QString(int)> name,
                     std::function<QIcon(int)> icon = nullptr) {
    NSTabView* tabs = [[NSTabView alloc] initWithFrame:NSMakeRect(0, 0, 500, 340)];
    bool icons = false;
    for (int i = 0; icon && i < count && !icons; ++i) { icons = !icon(i).isNull(); }
    // Pages with the shared icons (the Settings panes) are chosen in a row
    // of icons above names instead of text-only tabs (mitcad#971).
    MitcadNativePaneSelector* panes = icons ? paneSelector(tabs) : nil;
    const int id = bind(widget, panes != nil ? panes.superview : tabs);
    m_bindings[static_cast<std::size_t>(id)].tabs = tabs;
    m_bindings[static_cast<std::size_t>(id)].panes = panes;
    tabs.identifier = ns(QString::number(id));
    int selected = 0;
    if (auto* source = qobject_cast<QTabWidget*>(widget)) { selected = source->currentIndex(); }
    if (auto* source = qobject_cast<QStackedWidget*>(widget)) { selected = source->currentIndex(); }
    for (int i = 0; i < count; ++i) {
      NSTabViewItem* item = [[NSTabViewItem alloc] initWithIdentifier:ns(QString::number(i))];
      item.label = ns(title(name(i)));
      item.view = renderWidget(page(i));
      [tabs addTabViewItem:item];
      if (panes != nil) {
        MitcadNativePaneButton* button = [[MitcadNativePaneButton alloc] initWithFrame:NSZeroRect];
        button.cell = [[MitcadNativePaneCell alloc] initTextCell:@""];
        button.bordered = NO;
        button.title = item.label;
        button.font = [NSFont systemFontOfSize:NSFont.smallSystemFontSize];
        button.image = paneImage(icon(i), title(name(i)));
        button.imagePosition = button.image != nil ? NSImageAbove : NSNoImage;
        button.tag = i;
        button.target = panes;
        button.action = @selector(choose:);
        button.toolTip = item.label;
        button.accessibilityIdentifier = ns(QStringLiteral("native.settings.pane.%1").arg(i));
        button.accessibilityRole = NSAccessibilityRadioButtonRole;
        // Room for the icon and the tint around a short name (PaneButton).
        [button.widthAnchor constraintGreaterThanOrEqualToConstant:64].active = YES;
        [button.heightAnchor constraintEqualToConstant:58].active = YES;
        [panes addArrangedSubview:button];
      }
    }
    if (selected >= 0 && selected < count) { [tabs selectTabViewItemAtIndex:selected]; }
    [panes sync];
    tabs.delegate = m_target;
    return panes != nil ? panes.superview : tabs;
  }
  // The section row above a separator line and the pages, centred as in the
  // shared header; the window is at least as wide as the complete row.
  MitcadNativePaneSelector* paneSelector(NSTabView* tabs) {
    tabs.tabViewType = NSNoTabsNoBorder;
    tabs.translatesAutoresizingMaskIntoConstraints = NO;
    MitcadNativePaneSelector* panes = [MitcadNativePaneSelector new];
    panes.orientation = NSUserInterfaceLayoutOrientationHorizontal;
    panes.alignment = NSLayoutAttributeCenterY;
    panes.spacing = 4;
    panes.edgeInsets = NSEdgeInsetsMake(8, 12, 8, 12);
    panes.translatesAutoresizingMaskIntoConstraints = NO;
    [panes setHuggingPriority:NSLayoutPriorityDefaultHigh forOrientation:NSLayoutConstraintOrientationHorizontal];
    panes.pages = tabs;
    panes.accessibilityIdentifier = @"native.settings.panes";
    panes.accessibilityElement = YES;
    panes.accessibilityRole = NSAccessibilityRadioGroupRole;
    panes.accessibilityLabel = ns(tr("Settings category"));
    NSBox* line = [NSBox new];
    line.boxType = NSBoxSeparator;
    line.translatesAutoresizingMaskIntoConstraints = NO;
    NSView* container = [NSView new];
    container.translatesAutoresizingMaskIntoConstraints = NO;
    [container addSubview:panes];
    [container addSubview:line];
    [container addSubview:tabs];
    [NSLayoutConstraint activateConstraints:@[
      [panes.topAnchor constraintEqualToAnchor:container.topAnchor],
      [panes.centerXAnchor constraintEqualToAnchor:container.centerXAnchor],
      [panes.leadingAnchor constraintGreaterThanOrEqualToAnchor:container.leadingAnchor],
      [panes.trailingAnchor constraintLessThanOrEqualToAnchor:container.trailingAnchor],
      [line.topAnchor constraintEqualToAnchor:panes.bottomAnchor],
      [line.leadingAnchor constraintEqualToAnchor:container.leadingAnchor],
      [line.trailingAnchor constraintEqualToAnchor:container.trailingAnchor],
      [tabs.topAnchor constraintEqualToAnchor:line.bottomAnchor],
      [tabs.leadingAnchor constraintEqualToAnchor:container.leadingAnchor],
      [tabs.trailingAnchor constraintEqualToAnchor:container.trailingAnchor],
      [tabs.bottomAnchor constraintEqualToAnchor:container.bottomAnchor],
    ]];
    return panes;
  }
  NSView* renderModel(QAbstractItemView* view) {
    // Icon-mode lists are tile grids with their pictures (mitcad#813).
    if (auto tiles = mac::createNativeTileGrid(view)) {
      NSView* grid = (__bridge NSView*)tiles->nativeView();
      m_tileViews.push_back(std::move(tiles));
      return grid;
    }
    const bool hierarchical = qobject_cast<QTreeView*>(view) != nullptr;
    NSTableView* table = hierarchical ? [[MitcadNativeOutline alloc] initWithFrame:NSMakeRect(0, 0, 520, 180)]
                                     : [[MitcadNativeTable alloc] initWithFrame:NSMakeRect(0, 0, 520, 180)];
    table.rowHeight = std::max(25, std::min(100, view->iconSize().height() + 4));
    table.usesAlternatingRowBackgroundColors = !view->property("nativeSourceList").toBool();
    if (view->property("nativeSourceList").toBool()) {
      table.style = NSTableViewStyleSourceList;
      table.backgroundColor = NSColor.clearColor;
      table.rowHeight = 24;
      table.intercellSpacing = NSMakeSize(0, 0);
      table.frame = NSMakeRect(0, 0, 248, 180);
      table.autoresizingMask = NSViewWidthSizable;
    }
    table.allowsMultipleSelection = view->selectionMode() == QAbstractItemView::ExtendedSelection || view->selectionMode() == QAbstractItemView::MultiSelection;
    // Qt's spanned headings scroll with their rows, rather than floating
    // over later content as AppKit group rows otherwise may do.
    table.floatsGroupRows = NO;
    table.delegate = m_target;
    table.dataSource = m_target;
    table.target = m_target;
    table.doubleAction = @selector(doubleClick:);
    // A source list's name column takes a width change; its compact marks
    // keep theirs, as in the shared panel (mitcad#818).
    table.columnAutoresizingStyle = view->property("nativeSourceList").toBool()
                                        ? NSTableViewFirstColumnOnlyAutoresizingStyle
                                        : NSTableViewUniformColumnAutoresizingStyle;
    const int count = view->model() == nullptr ? 1 : std::max(1, view->model()->columnCount(view->rootIndex()));
    for (int i = 0; i < count; ++i) {
      NSTableColumn* column = [[NSTableColumn alloc] initWithIdentifier:ns(QString::number(i))];
      column.title = ns(view->model() == nullptr ? QString() : view->model()->headerData(i, Qt::Horizontal).toString());
      column.width = view->property("nativeSourceList").toBool() ? (i == 0 ? 190 : 40) : (i == 0 ? 220 : 130);
      column.minWidth = view->property("nativeSourceList").toBool() ? 16 : 70;
      [table addTableColumn:column];
      if (hierarchical && i == 0) { ((NSOutlineView*)table).outlineTableColumn = column; }
    }
    auto* tree = qobject_cast<QTreeView*>(view);
    if (qobject_cast<QListView*>(view) != nullptr || (tree != nullptr && tree->isHeaderHidden())) { table.headerView = nil; }
    // The native default must fit within the presenter's required upper bound.
    const int minimumHeight = view->property("nativeSourceList").toBool() ? 24
        : std::min(view->maximumHeight(), std::max(120, std::min(280, view->minimumHeight())));
    const bool boundedList = qobject_cast<QListView*>(view) != nullptr && view->property(NativeListRowLimitProperty).toInt() > 0;
    NSScrollView* box = scroll(table, boundedList ? 0 : minimumHeight);
    if (view->horizontalScrollBarPolicy() == Qt::ScrollBarAlwaysOff) box.hasHorizontalScroller = NO;
    if (view->property("nativeSourceList").toBool()) {
      box.borderType = NSNoBorder;
      box.drawsBackground = NO;
      box.automaticallyAdjustsContentInsets = NO;
      box.contentInsets = NSEdgeInsetsMake(0, 0, 0, 0);
    }
    if (!boundedList && view->maximumHeight() < QWIDGETSIZE_MAX) {
      [box.heightAnchor constraintLessThanOrEqualToConstant:view->maximumHeight()].active = YES;
    }
    if (hierarchical && rowMovesRegistry().contains(view)) {
      // Rows move by dragging within this outline only (mitcad#816).
      [table registerForDraggedTypes:@[kNativeRowDragType]];
      [table setDraggingSourceOperationMask:NSDragOperationMove forLocal:YES];
      [table setDraggingSourceOperationMask:NSDragOperationNone forLocal:NO];
    }
    const int id = bind(view, box);
    m_bindings[static_cast<std::size_t>(id)].table = table;
    if (view->property(NativeEmptyTextProperty).isValid()) {
      // refreshModel shows it while the model has no rows.
      NSTextField* hint = [NSTextField wrappingLabelWithString:@""];
      hint.textColor = NSColor.placeholderTextColor;
      hint.selectable = NO;
      hint.hidden = YES;
      hint.accessibilityIdentifier = ns(view->objectName() + QStringLiteral(".empty"));
      [table addSubview:hint];
      ((NSTableView<MitcadNativeEmptyText>*)table).emptyLabel = hint;
    }
    if (QScrollBar* bar = view->verticalScrollBar()) {
      // The shared view's scroll requests reach the native viewport
      // (followSharedScroll, mitcad#972).
      const auto generation = m_generation;
      m_bindings[static_cast<std::size_t>(id)].connections.push_back(
          connect(bar, &QScrollBar::valueChanged, this, [this, id, generation] {
            if (generation != m_generation || static_cast<std::size_t>(id) >= m_bindings.size()) { return; }
            m_bindings[static_cast<std::size_t>(id)].scrollMoved = true;
            scheduleRefresh();
          }));
    }
    if (boundedList) {
      // A shared compact list grows/shrinks after records or Clear. Do not
      // freeze its initially empty QWidget maximumHeight into the native view.
      auto& binding = m_bindings[static_cast<std::size_t>(id)];
      binding.listHeight = [box.heightAnchor constraintEqualToConstant:table.rowHeight];
      binding.listHeight.active = YES;
    }
    table.identifier = ns(QString::number(id));
    // A tree or table keeps its shared header's section sizes, stretching
    // columns and hidden sections, in flat rows like the shared view's,
    // instead of generic widths that scroll sideways (mitcad#814).
    if (!view->property("nativeSourceList").toBool() && mac::nativeHeader(view) != nullptr) {
      const CGFloat rowHeight = table.rowHeight;
      table.style = NSTableViewStylePlain;
      table.rowHeight = rowHeight;
      table.intercellSpacing = NSMakeSize(0, table.intercellSpacing.height);
      if (auto* grid = qobject_cast<QTableView*>(view)) {
        // The shared table's own row height, such as a list table's 28.
        table.rowHeight = std::max<CGFloat>(table.rowHeight,
                                            grid->verticalHeader()->defaultSectionSize() - table.intercellSpacing.height);
      }
      if (tree != nullptr) {
        // The shared first section's width includes the tree's indentation;
        // AppKit would widen the outline column for deeper rows instead.
        auto* outline = (NSOutlineView*)table;
        outline.autoresizesOutlineColumn = NO;
        outline.indentationPerLevel = tree->indentation();
      }
      mac::applyNativeColumns(table, view, m_bindings[static_cast<std::size_t>(id)].sections);
      mac::followNativeColumns(table);
    }
    if (qobject_cast<QListView*>(view) != nullptr && count == 1 && !view->property("nativeSourceList").toBool()) {
      // A one-column list uses its viewport, rather than retaining the
      // generic initial width after the surrounding form grows.
      table.columnAutoresizingStyle = NSTableViewNoColumnAutoresizing;
      table.tableColumns.firstObject.resizingMask = NSTableColumnAutoresizingMask;
      mac::fitNativeColumns(table);
      mac::followNativeColumns(table);
    }
    if (const QHeaderView* header = mac::nativeHeader(view)) {
      const QVariantList sources = header->property(NativeColumnEditorsProperty).toList();
      std::vector<NSView*> editors;
      for (const QVariant& source : sources) {
        auto* widget = qobject_cast<QWidget*>(source.value<QObject*>());
        editors.push_back(widget != nullptr ? renderWidget(widget) : nil);
      }
      mac::installNativeColumnEditors(table, editors);
    }
    return box;
  }
  NSMutableArray<MitcadNativeModelNode*>* modelNodes(QAbstractItemView* view, const QModelIndex& parent) {
    auto* nodes = [NSMutableArray<MitcadNativeModelNode*> new];
    for (int row = 0; row < view->model()->rowCount(parent); ++row) {
      if (auto* tree = qobject_cast<QTreeView*>(view)) { if (tree->isRowHidden(row, parent)) { continue; } }
      if (auto* table = qobject_cast<QTableView*>(view)) { if (table->isRowHidden(row)) { continue; } }
      if (auto* list = qobject_cast<QListView*>(view)) { if (list->isRowHidden(row)) { continue; } }
      MitcadNativeModelNode* node = [MitcadNativeModelNode new];
      node->index = view->model()->index(row, 0, parent);
      node->identifier = node->index.data(NativeControlIdentifierRole).toString();
      node->children = modelNodes(view, node->index);
      [nodes addObject:node];
    }
    return nodes;
  }
  void refreshModel(Binding& binding, QAbstractItemView* view, int id) {
    observeModel(binding, view->model(), id);
    observeSelection(binding, view, id);
    auto* list = qobject_cast<QListView*>(view);
    if (binding.listHeight != nil && list != nullptr && view->model() != nullptr) {
      const int limit = std::max(1, view->property(NativeListRowLimitProperty).toInt());
      CGFloat height = 2 * view->frameWidth();
      int shown = 0;
      for (int row = 0; row < view->model()->rowCount(view->rootIndex()) && shown < limit; ++row) {
        if (list->isRowHidden(row)) continue;
        height += listRowHeight(view, row, binding.table.rowHeight) + binding.table.intercellSpacing.height;
        ++shown;
      }
      if (shown == 0) height += binding.table.rowHeight;
      NATIVE_SET(binding.listHeight, constant, height);
    }
    updateCurrentRows(binding.table);
    if (binding.rootIndex != view->rootIndex()) {
      binding.rootIndex = view->rootIndex();
      binding.dirty = true;
    }
    // The shared header's sizes, stretch and hidden sections (mitcad#814).
    if (!view->property("nativeSourceList").toBool() &&
        mac::applyNativeColumns(binding.table, view, binding.sections)) { ++g_widgetPropertyWrites; }
    // An open cell editor keeps its row until the edit ends (modelEdit then
    // refreshes), unless the model replaced that row: the shared view closes
    // such an editor without committing, and its table keeps the focus.
    bool refocus = false;
    if (binding.dirty && view->model() != nullptr) {
      if (NSTextField* edited = mac::editedCellField(binding.table)) {
        NSObject* delegate = (NSObject*)edited.delegate;
        MitcadNativeModelCell* cell =
            [delegate isKindOfClass:MitcadNativeModelCell.class] ? (MitcadNativeModelCell*)delegate : nil;
        if (cell != nil && cell->index.isValid()) {
          [cell refreshUnchangedEdit];
          mac::logNativeCellEdit("a model refresh waits for the edit", edited);
          return;
        }
        // A rebuilt model has new indices for the same rows: the edited row,
        // found by its stable identifier, keeps its edit (mitcad#814).
        const NSInteger nativeColumn = [binding.table columnForView:edited];
        const QModelIndex row = cell != nil ? stableRow(view, cell->rowIdentifier) : QModelIndex();
        if (row.isValid() && nativeColumn >= 0) {
          cell->index = row.siblingAtColumn(
              qt(binding.table.tableColumns[static_cast<NSUInteger>(nativeColumn)].identifier).toInt());
          [cell refreshUnchangedEdit];
          mac::logNativeCellEdit("a model refresh rebuilt the edited row; its edit stays", edited);
          return;
        }
        mac::logNativeCellEdit("a model refresh removed the edited row; its edit is discarded", edited);
        if (cell != nil) { cell->discarding = YES; }
        [edited abortEditing];
        if (cell != nil) { cell->discarding = NO; }
        refocus = true;
      }
    }
    if (binding.dirty) {
      binding.selectionDirty = true;
      if (view->model() == nullptr) {
        binding.nodes = nil;
        binding.dirty = false;
        [binding.table reloadData];
        ++g_widgetPropertyWrites;
        return;
      }
      binding.presentation = modelPresentation(view);
      binding.nodes = modelNodes(view, view->rootIndex());
      binding.dirty = false;
      [binding.table reloadData];
      ++g_widgetPropertyWrites;
      if ([binding.table isKindOfClass:NSOutlineView.class]) {
        NSOutlineView* outline = (NSOutlineView*)binding.table;
        auto expand = [&](auto&& self, NSArray<MitcadNativeModelNode*>* nodes) -> void {
          auto* tree = qobject_cast<QTreeView*>(view);
          for (MitcadNativeModelNode* node in nodes) {
            if (tree->isExpanded(node->index)) { [outline expandItem:node]; }
            self(self, node->children);
          }
        };
        expand(expand, binding.nodes);
      }
      if (refocus && [binding.table.window isKindOfClass:MitcadNativeSecondaryWindow.class]) {
        [binding.table.window makeFirstResponder:binding.table];
      }
    }
    if ([binding.table conformsToProtocol:@protocol(MitcadNativeEmptyText)]) {
      if (NSTextField* hint = ((NSTableView<MitcadNativeEmptyText>*)binding.table).emptyLabel) {
        // The shared view's hint while its model has no rows, as it paints it.
        const bool empty = view->model() == nullptr || view->model()->rowCount(view->rootIndex()) == 0;
        NATIVE_STRING(hint, stringValue, view->property(NativeEmptyTextProperty).toString());
        NATIVE_SET(hint, hidden, BOOL(!empty));
        placeEmptyLabel(binding.table, hint);
      }
    }
    // AppKit ends a cell's edit when it selects rows: the shared selection
    // follows once the edit ends (mitcad#814).
    if (binding.selectionDirty && view->selectionModel() != nullptr) {
      if (NSTextField* edited = mac::editedCellField(binding.table)) {
        mac::logNativeCellEdit("a selection update waits for the edit", edited);
        return;
      }
      binding.selectionDirty = false;
      NSMutableIndexSet* selected = [NSMutableIndexSet new];
      for (NSInteger row = 0; row < binding.table.numberOfRows; ++row) {
        MitcadNativeModelNode* node = nodeAt(binding.table, row);
        // The native table presents rows. A shared SelectItems view can
        // restore just its current cell after a model refresh; that row
        // still needs its native highlight, without selecting its other
        // cells in the presenter. Selection callbacks stay suppressed by
        // m_refreshing while this native state is restored.
        const bool selectedRow = node != nil && (view->selectionBehavior() == QAbstractItemView::SelectItems
            ? view->selectionModel()->rowIntersectsSelection(node->index.row(), node->index.parent())
            : view->selectionModel()->isRowSelected(node->index.row(), node->index.parent()));
        if (selectedRow) {
          [selected addIndex:static_cast<NSUInteger>(row)];
        }
      }
      if (![selected isEqualToIndexSet:binding.table.selectedRowIndexes]) {
        [binding.table selectRowIndexes:selected byExtendingSelection:NO];
        ++g_widgetPropertyWrites;
      }
      // Source-list reloads rebuild native nodes when a project opens or
      // refreshes. Keep its restored selection inside the bounded viewport.
      if (view->property("nativeSourceList").toBool() && binding.table.selectedRow >= 0)
        [binding.table scrollRowToVisible:binding.table.selectedRow];
    }
  }
  // The native viewport follows its shared view's own scroll requests, such
  // as a presenter's scrollTo() of the first row worth showing (Settings >
  // Navigation's first binding, mitcad#972): when the shared view's new top
  // row is not wholly in the native view, it becomes the native top row.
  // Only a move of the shared position counts, never the native one, so the
  // user's own scrolling stays. A shared move that just keeps its current
  // row in sight, as after native input chose a row the native view already
  // shows, leaves the native position too.
  void followSharedScroll(Binding& binding) {
    NSTableView* table = binding.table;
    auto* view = qobject_cast<QAbstractItemView*>(binding.widget.data());
    if (!binding.scrollMoved || table == nil || view == nullptr) { return; }
    // A reload or an edit still pending, or a table outside a window (a
    // hidden tab), keeps the request for a later refresh.
    if (binding.dirty || table.window == nil || mac::editedCellField(table) != nil) { return; }
    binding.scrollMoved = false;
    NSScrollView* scroll = table.enclosingScrollView;
    if (scroll == nil || view->model() == nullptr || table.numberOfRows == 0) { return; }
    const auto nativeRow = [&](const QModelIndex& index) -> NSInteger {
      if (!index.isValid()) { return -1; }
      const QModelIndex first = index.siblingAtColumn(0);
      for (NSInteger row = 0; row < table.numberOfRows; ++row) {
        MitcadNativeModelNode* node = nodeAt(table, row);
        if (node != nil && node->index == first) { return row; }
      }
      return -1;
    };
    QModelIndex top;
    if (auto* grid = qobject_cast<QTableView*>(view)) {
      const int row = grid->rowAt(0);
      if (row >= 0) { top = view->model()->index(row, 0, view->rootIndex()); }
    } else {
      top = view->indexAt(QPoint(0, 0));
    }
    const NSInteger target = nativeRow(top);
    if (target < 0) { return; }
    // The header and other insets cover the clip view's top.
    [scroll tile];
    NSClipView* clip = scroll.contentView;
    const NSEdgeInsets insets = clip.contentInsets;
    NSRect shown = clip.bounds;
    shown.origin.y += insets.top;
    shown.size.height = std::max<CGFloat>(0, shown.size.height - insets.top - insets.bottom);
    shown = [table convertRect:shown fromView:clip];
    if (NSContainsRect(NSInsetRect(shown, -0.5, -0.5), [table rectOfRow:target])) { return; }
    const QModelIndex current = view->currentIndex();
    const NSInteger currentRow = nativeRow(current);
    const QRect currentRect = current.isValid() ? view->visualRect(current) : QRect();
    if (currentRow >= 0 && NSIntersectsRect(shown, [table rectOfRow:currentRow]) && currentRect.isValid() &&
        currentRect.bottom() >= 0 && currentRect.top() < view->viewport()->height()) { return; }
    NSRect bounds = clip.bounds;
    bounds.origin.y = NSMinY([clip convertRect:[table rectOfRow:target] fromView:table]) - insets.top;
    const NSPoint origin = [clip constrainBoundsRect:bounds].origin;
    if (std::abs(origin.y - clip.bounds.origin.y) < 0.5) { return; }
    [clip scrollToPoint:origin];
    [scroll reflectScrolledClipView:clip];
    ++g_widgetPropertyWrites;
    if (!qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT"))
      qInfo().noquote() << QStringLiteral("Native table follows its view's scroll: %1 top row %2 (%3)")
          .arg(view->objectName()).arg(qlonglong(target)).arg(top.data().toString());
  }
  void rebuild() {
    // An option row may be replaced while its catalogue search is being
    // typed. Keep the surviving presenter's edit across the native rebuild,
    // while still showing the new options immediately.
    NSWindow* window = m_root.window;
    NSResponder* responder = window.firstResponder;
    NSTextView* editor = [responder isKindOfClass:NSTextView.class] && ((NSTextView*)responder).isFieldEditor
        ? (NSTextView*)responder : nil;
    NSTextField* field = [editor.delegate isKindOfClass:NSTextField.class] ? (NSTextField*)editor.delegate : nil;
    const QString identifier = field != nil && [field isDescendantOf:m_root] ? qt(field.accessibilityIdentifier) : QString();
    const auto edits = identifier.isEmpty() ? QList<QLineEdit*>() : m_source->findChildren<QLineEdit*>(identifier);
    const QPointer<QLineEdit> keptEdit = edits.size() == 1 ? edits.first() : nullptr;
    const NSRange selection = editor != nil ? editor.selectedRange : NSMakeRange(0, 0);
    // A multiline editor (Formula Text) keeps the keyboard likewise. Its
    // presenter already holds the text and selection; the refresh presents
    // them in the new view and then hands that view the focus (mitcad#984).
    if ([responder isKindOfClass:MitcadNativeTextView.class] && [(NSView*)responder isDescendantOf:m_root]) {
      if (auto* text = qobject_cast<QPlainTextEdit*>(textPresenter((NSTextView*)responder))) m_textFocusRequested = text;
    }
    detachBindings();
    for (MitcadNativeModelCell* cell in m_cells) { cell->owner = nullptr; }
    m_cellViews.clear();
    m_tileViews.clear();
    m_ruleViews.clear();
    m_artworkViews.clear();
    for (NSView* child in [m_root.arrangedSubviews copy]) { [m_root removeArrangedSubview:child]; [child removeFromSuperview]; }
    m_bindings.clear();
    ++m_generation;
    m_structureDirty = false;
    for (const auto& object : m_observed) { if (object) { object->removeEventFilter(this); } }
    m_observed.clear();
    // A window's root fills it, so stretched and expanding rows take extra
    // height as in the Qt layout (mitcad#812).
    if (m_source->isWindow()) { m_root.distribution = NSStackViewDistributionFill; }
    add(m_root, renderWidget(m_source));
    // Initial construction happens before the root joins a window. Hidden
    // categories retain their controls, but not a connected constraint tree.
    mac::layout::updateVisibility(m_root);
    if (m_footer != nil) { placeFooter(); }
    m_widgetTree.clear();
    for (QWidget* widget : structureWidgets()) { m_widgetTree.emplace_back(widget); }
    // ChildAdded arrives before QWidget construction finishes. Observe the
    // completed tree in this coalesced rebuild, never cast that child early.
    m_observed.push_back(m_source.data());
    const auto objects = m_source->findChildren<QObject*>();
    for (QObject* object : objects) { m_observed.push_back(object); }
    for (const auto& object : m_observed) { if (object) { object->installEventFilter(this); } }
    if (keptEdit != nullptr && keptEdit->isEnabled() && !keptEdit->isReadOnly() && keptEdit->isVisibleTo(m_source) &&
        m_root.window == window && window.isKeyWindow) {
      std::vector<NSTextField*> matches;
      const auto find = [&](auto&& self, NSView* view) -> void {
        if ([view isKindOfClass:NSTextField.class] && qt(view.accessibilityIdentifier) == identifier &&
            !view.isHiddenOrHasHiddenAncestor) matches.push_back((NSTextField*)view);
        for (NSView* child in view.subviews) self(self, child);
      };
      find(find, m_root);
      NSTextField* restored = matches.size() == 1 ? matches.front() : nil;
      if (restored.editable && restored.enabled && [window makeFirstResponder:restored]) {
        NSText* current = restored.currentEditor;
        if ([current isKindOfClass:NSTextView.class]) {
          const NSUInteger start = std::min(selection.location, current.string.length);
          ((NSTextView*)current).selectedRange = NSMakeRange(start, std::min(selection.length, current.string.length - start));
        }
      }
    }
  }
  QPointer<QWidget> m_source;
  QPointer<QPlainTextEdit> m_textFocusRequested;
  QTimer m_timer;
  QTimer m_refreshTimer;
  MitcadNativeWidgetTarget* m_target = nil;
  NSStackView* m_root = nil;
  NSStackView* m_footer = nil; // detachFooter
  std::vector<Binding> m_bindings;
  std::vector<std::pair<QPointer<QWidget>, std::unique_ptr<AppKitWidgetView>>> m_cellViews;
  // Tile grids and catalogues (NativeCollection.hpp) keep their own bindings.
  std::vector<std::unique_ptr<NativeWidgetView>> m_tileViews;
  std::vector<std::unique_ptr<NativeWidgetView>> m_ruleViews;
  std::vector<std::unique_ptr<NativeWidgetView>> m_artworkViews;
  NSHashTable<MitcadNativeModelCell*>* m_cells = [NSHashTable weakObjectsHashTable];
  std::vector<QPointer<QObject>> m_observed;
  std::vector<QPointer<QWidget>> m_widgetTree;
  quint64 m_generation = 0;
  bool m_structureDirty = false;
  bool m_refreshing = false;
  QPersistentModelIndex m_draggedRow; // an outline row being dragged (mitcad#816)
  bool m_rowDragging = false;
};

#undef NATIVE_SET
#undef NATIVE_STRING

quint64 nativeWidgetPropertyWrites() { return g_widgetPropertyWrites; }

void setNativeRowMoves(QAbstractItemView* view, NativeRowMoves moves) {
  if (view == nullptr) { return; }
  if (!rowMovesRegistry().contains(view)) {
    QObject::connect(view, &QObject::destroyed, [view] { rowMovesRegistry().remove(view); });
  }
  rowMovesRegistry().insert(view, std::move(moves));
}

class NativeDialogController final : public QObject {
public:
  NativeDialogController(QWidget* widget, bool modal)
      : m_widget(widget), m_modal(modal),
        m_presentAsSheet(modal || widget->windowModality() != Qt::NonModal || widget->property("nativePresentAsSheet").toBool()),
        m_presenterModality(widget->windowModality()) {
    m_target = [MitcadNativeWindowTarget new];
    m_target->owner = this;
    m_view = std::make_unique<AppKitWidgetView>(widget);
    NSView* view = (__bridge NSView*)m_view->nativeView();
    const QSize requested = widget->size().expandedTo(widget->sizeHint());
    const CGFloat width = std::clamp<CGFloat>(requested.width(), 380, 1000);
    // Settings keeps the chosen shared pane's natural request. The native
    // window fitter also measures its body and caps it to the real screen;
    // a prototype 740pt cap here can discard a pane's last rows (mitcad#992).
    const bool settings = qobject_cast<SettingsWindow*>(widget) != nullptr;
    const CGFloat height = settings ? std::max<CGFloat>(requested.height(), 140)
                                    : std::clamp<CGFloat>(requested.height(), 140, 740);
    if (settings) {
      qInfo().noquote() << QStringLiteral("Native Settings initial size: hidden presenter %1 x %2, hint %3 x %4, request %5 x %6")
          .arg(widget->width()).arg(widget->height()).arg(widget->sizeHint().width()).arg(widget->sizeHint().height())
          .arg(width).arg(height);
    }
    // A modeless utility presenter (the part picker) floats in a panel that
    // leaves the keyboard with the document (mitcad#819).
    m_window = widget->property("nativeUtilityWindow").toBool() && !modal
        ? mac::nativeUtilityPanel(NSMakeRect(0, 0, width, height))
        : [[MitcadNativeSecondaryWindow alloc] initWithContentRect:NSMakeRect(0, 0, width, height)
                                         styleMask:NSWindowStyleMaskTitled | NSWindowStyleMaskClosable | NSWindowStyleMaskResizable
                                           backing:NSBackingStoreBuffered defer:NO];
    m_window.releasedWhenClosed = NO;
    m_window.title = ns(widget->windowTitle());
    m_window.identifier = @"native.secondary";
    m_window.delegate = m_target;
    // The body scrolls above the buttons; show() fits the window to its
    // content within the visible screen (mitcad#818).
    m_window.contentView = nativeScrollingContent(view, m_view->detachFooter());
    m_window.minSize = NSMakeSize(360, 160);
    if (auto* dialog = qobject_cast<QDialog*>(widget)) {
      connect(dialog, &QDialog::finished, this, [this](int result) { finish(result); });
    }
    connect(widget, &QObject::destroyed, this, [this] { finish(QDialog::Rejected); });
    widget->installEventFilter(this);
  }
  ~NativeDialogController() override {
    if (m_widget != nullptr) {
      m_widget->removeEventFilter(this);
      m_widget->setWindowModality(m_presenterModality);
    }
    m_target->owner = nullptr;
    m_window.delegate = nil;
    if (m_window.sheetParent != nil) { [m_window.sheetParent endSheet:m_window]; }
    [m_window orderOut:nil];
    m_window.contentView = nil;
    m_view.reset();
  }
  NSWindow* window() const { return m_window; }
  QWidget* widget() const { return m_widget; }
  void show() {
    // Presenter show events initialize dialogs and settings without showing
    // Qt controls. The live interface is composed entirely of AppKit views.
    m_host = currentHost(m_widget);
    m_widget->setWindowModality(Qt::NonModal);
    m_widget->setAttribute(Qt::WA_DontShowOnScreen, true);
    m_widget->setAttribute(Qt::WA_ShowWithoutActivating, true);
    m_widget->show();
    m_view->refresh();
    NSWindow* host = m_host;
    if (m_presentAsSheet && host != nil) {
      fitNativeWindowToScreen(m_window, host);
      [host beginSheet:m_window completionHandler:nil];
    } else if ([m_window isKindOfClass:MitcadNativeUtilityPanel.class]) {
      // In its kept place, in front, without taking the keyboard (mitcad#819).
      mac::restoreNativeUtilityFrame(m_window, m_widget, host);
      [m_window orderFront:nil];
    } else { [m_window center]; fitNativeWindowToScreen(m_window, nil); [m_window makeKeyAndOrderFront:nil]; }
    m_view->focusInitial();
  }
  int exec() {
    show();
    g_modalWindows.push_back(m_window);
    pumpNativeEvents([this] { return m_finished; });
    g_modalWindows.pop_back();
    return m_result;
  }
  bool dispatchShortcut(NSEvent* event) {
    if (!m_widget || m_finished || m_window.attachedSheet || !m_window.isKeyWindow) return false;
    if (m_view->currentRowKey(event)) return true;
    const QKeySequence sequence = mac::keySequence(event);
    if (sequence.isEmpty() || sequence[0].keyboardModifiers() == Qt::NoModifier) return false;
    // Only this presenter's explicit shortcuts participate. Text-view
    // Return remains a newline; Command+Return reaches the save shortcut.
    QPointer<QShortcut> match;
    for (QShortcut* shortcut : m_widget->findChildren<QShortcut*>()) {
      if (!shortcut->isEnabled() || !shortcut->keys().contains(sequence) ||
          shortcut->context() == Qt::WidgetShortcut || shortcut->context() == Qt::WidgetWithChildrenShortcut) continue;
      if (match) return false; // Do not choose arbitrarily between aliases.
      match = shortcut;
    }
    if (!match) return false;
    if (![m_window makeFirstResponder:nil]) return true;
    QTimer::singleShot(0, match, [match] { if (match && match->isEnabled()) emit match->activated(); });
    return true;
  }
  bool closeRequested() {
    if (qobject_cast<SettingsWindow*>(m_widget) != nullptr) {
      // Settings applies edits as they finish. Its hidden Qt focus does not
      // follow AppKit's field editor, including when the red close button
      // bypasses the shortcut path. A rejected edit keeps this live window
      // open; dialog cancellation below must remain possible without it.
      QPointer<NativeDialogController> alive(this);
      if (![m_window makeFirstResponder:nil] || alive == nullptr || m_widget == nullptr) { return NO; }
    }
    m_restoreHost = m_restoreHost || m_window.isKeyWindow;
    if (qobject_cast<QProgressDialog*>(m_widget) != nullptr) { m_widget->close(); }
    else if (auto* dialog = qobject_cast<QDialog*>(m_widget)) { dialog->reject(); }
    else if (m_widget != nullptr) { m_widget->close(); }
    if (!m_finished && m_widget == nullptr) { finish(QDialog::Rejected); }
    return NO; // finish() handles removal or the presenter vetoes closing.
  }
  void finish(int result) {
    if (m_finished) { return; }
    m_finished = true;
    m_result = result;
    NSWindow* key = NSApp.keyWindow;
    const bool sheet = m_window.sheetParent != nil;
    const bool restore = m_restoreHost || m_window.isKeyWindow ||
        (sheet && (key == nil || key == m_host || key == g_documentWindow));
    if ([m_window isKindOfClass:MitcadNativeUtilityPanel.class]) { mac::saveNativeUtilityFrame(m_window, m_widget); }
    if (m_window.sheetParent != nil) { [m_window.sheetParent endSheet:m_window returnCode:result]; }
    [m_window orderOut:nil];
    if (restore && m_host != nil) {
      // Qt's hidden presenter can queue parent activation while hiding.
      // Restore its visible AppKit host after that work, without moving
      // focus away from a different native window or a later user event.
      __weak NSWindow* host = m_host;
      __weak NSWindow* closed = m_window;
      __weak NSWindow* previousKey = key;
      const NSTimeInterval eventTime = NSApp.currentEvent.timestamp;
      QTimer::singleShot(0, QCoreApplication::instance(), [host, closed, previousKey, eventTime] {
        if (host == nil || !host.visible || host.attachedSheet != nil) { return; }
        NSWindow* current = NSApp.keyWindow;
        if (current != nil && current != host && current != closed &&
            current != previousKey && current != g_documentWindow) { return; }
        NSEvent* event = NSApp.currentEvent;
        const bool input = event.type == NSEventTypeKeyDown || event.type == NSEventTypeLeftMouseDown ||
                           event.type == NSEventTypeRightMouseDown || event.type == NSEventTypeOtherMouseDown;
        if (input && event.timestamp > eventTime) { return; }
        [host makeKeyWindow];
      });
    }
    if (!m_modal) { QTimer::singleShot(0, this, [this] { delete this; }); }
  }
protected:
  bool eventFilter(QObject* watched, QEvent* event) override {
    if (watched == m_widget && event->type() == QEvent::Hide && !m_finished) {
      m_restoreHost = m_restoreHost || m_window.isKeyWindow;
      // QDialog::done hides before emitting finished; let that signal
      // carry the accepted/rejected result before handling a bare hide.
      QTimer::singleShot(0, this, [this] {
        if (!m_finished && (m_widget == nullptr || !m_widget->isVisible())) {
          auto* dialog = qobject_cast<QDialog*>(m_widget);
          finish(dialog == nullptr ? QDialog::Rejected : dialog->result());
        }
      });
    }
    if (watched == m_widget && event->type() == QEvent::WindowTitleChange) { m_window.title = ns(m_widget->windowTitle()); }
    return QObject::eventFilter(watched, event);
  }
private:
  QPointer<QWidget> m_widget;
  bool m_modal = false;
  bool m_presentAsSheet = false;
  Qt::WindowModality m_presenterModality = Qt::NonModal;
  bool m_finished = false;
  int m_result = QDialog::Rejected;
  std::unique_ptr<AppKitWidgetView> m_view;
  MitcadNativeWindowTarget* m_target = nil;
  NSWindow* m_window = nil;
  __weak NSWindow* m_host = nil;
  bool m_restoreHost = false;
};

namespace {
std::vector<NativeDialogController*> g_modeless;
NSWindow* currentHost(QWidget* widget) {
  for (QWidget* parent = widget == nullptr ? nullptr : widget->parentWidget(); parent != nullptr; parent = parent->parentWidget()) {
    for (NativeDialogController* controller : g_modeless) {
      if (controller->widget() == parent) { return controller->window(); }
    }
  }
  if (!g_modalWindows.empty()) { return g_modalWindows.back(); }
  for (NativeDialogController* controller : g_modeless) {
    if (controller->window() == NSApp.keyWindow) { return controller->window(); }
  }
  return g_documentWindow;
}

int messageBox(QMessageBox* box) {
  QPointer<QMessageBox> presenter = box;
  NSAlert* alert = [NSAlert new];
  alert.messageText = ns(plain(box->text()));
  alert.informativeText = ns(plain(box->informativeText()));
  alert.alertStyle = box->icon() == QMessageBox::Critical ? NSAlertStyleCritical
                    : box->icon() == QMessageBox::Warning ? NSAlertStyleWarning : NSAlertStyleInformational;
  QList<QAbstractButton*> buttons;
  NSButton* implicitDefault = nil;
  bool hasDefault = false;
  for (QAbstractButton* button : box->buttons()) {
    const auto role = box->buttonRole(button);
    // QMessageBox adds its own ActionRole details toggle. It is a
    // disclosure in the accessory, never a response that ends NSAlert.
    if (!box->detailedText().isEmpty() && role == QMessageBox::ActionRole &&
        (title(button->text()).replace(QStringLiteral("..."), QStringLiteral("…")) ==
             title(QMessageBox::tr("Show Details...")).replace(QStringLiteral("..."), QStringLiteral("…")) ||
         title(button->text()).replace(QStringLiteral("..."), QStringLiteral("…")) ==
             title(QMessageBox::tr("Hide Details...")).replace(QStringLiteral("..."), QStringLiteral("…")))) {
      continue;
    }
    NSButton* native = [alert addButtonWithTitle:ns(title(button->text()))];
    native.accessibilityIdentifier = ns(QStringLiteral("native.alert.response.%1").arg(buttons.size()));
    buttons.append(button);
    native.enabled = button->isEnabled();
    if (implicitDefault == nil && (role == QMessageBox::AcceptRole || role == QMessageBox::YesRole)) { implicitDefault = native; }
    if (button == box->defaultButton()) { native.keyEquivalent = @"\r"; hasDefault = true; }
    else if (button == box->escapeButton() || box->standardButton(button) == QMessageBox::Cancel) { native.keyEquivalent = @"\033"; }
    else { native.keyEquivalent = @""; }
  }
  if (!hasDefault && implicitDefault != nil) { implicitDefault.keyEquivalent = @"\r"; }
  std::unique_ptr<NativeWidgetView> check;
  NSStackView* accessories = stack();
  MitcadNativeAlertDetails* detailsTarget = nil;
  if (!box->detailedText().isEmpty()) {
    detailsTarget = [MitcadNativeAlertDetails new];
    detailsTarget->alert = alert;
    detailsTarget->showTitle = ns(QMessageBox::tr("Show Details…"));
    detailsTarget->hideTitle = ns(QMessageBox::tr("Hide Details…"));
    NSButton* disclosure = [NSButton checkboxWithTitle:detailsTarget->showTitle
                                              target:detailsTarget action:@selector(toggle:)];
    disclosure.buttonType = NSButtonTypePushOnPushOff;
    disclosure.bezelStyle = NSBezelStyleRecessed;
    disclosure.accessibilityIdentifier = @"native.alert.details.toggle";
    detailsTarget->disclosure = disclosure;
    add(accessories, disclosure);
    NSTextView* diagnostic = [[NSTextView alloc] initWithFrame:NSMakeRect(0, 0, 420, 180)];
    diagnostic.editable = NO;
    diagnostic.selectable = YES;
    diagnostic.string = ns(box->detailedText());
    diagnostic.font = [NSFont monospacedSystemFontOfSize:NSFont.smallSystemFontSize weight:NSFontWeightRegular];
    NSScrollView* details = scroll(diagnostic, 180);
    details.accessibilityIdentifier = @"native.alert.details.text";
    details.hidden = YES;
    detailsTarget->details = details;
    add(accessories, details);
  }
  if (box->checkBox() != nullptr) {
    check = createNativeWidgetView(box->checkBox());
    add(accessories, (__bridge NSView*)check->nativeView());
  }
  if (accessories.arrangedSubviews.count > 0) {
    [accessories.widthAnchor constraintEqualToConstant:420].active = YES;
    alert.accessoryView = accessories;
  }
  struct Completion { NSModalResponse response = NSModalResponseCancel; bool done = false; };
  const auto completion = std::make_shared<Completion>();
  NSWindow* host = currentHost(box);
  QObject lifetime;
  const auto destroyed = QObject::connect(box, &QObject::destroyed, &lifetime, [&, completion] {
    if (alert.window.sheetParent != nil) { [alert.window.sheetParent endSheet:alert.window]; }
    [alert.window orderOut:nil];
    completion->done = true;
  });
  if (host != nil) {
    [alert beginSheetModalForWindow:host completionHandler:^(NSModalResponse result) {
      completion->response = result;
      completion->done = true;
    }];
    g_modalWindows.push_back(alert.window);
    pumpNativeEvents([completion] { return completion->done; });
    g_modalWindows.pop_back();
  } else { completion->response = [alert runModal]; }
  if (detailsTarget != nil) { detailsTarget->disclosure.target = nil; }
  QObject::disconnect(destroyed);
  if (presenter == nullptr) { return QDialog::Rejected; }
  const NSInteger index = completion->response - NSAlertFirstButtonReturn;
  if (index >= 0 && index < buttons.size()) {
    buttons[static_cast<int>(index)]->click();
  } else { box->reject(); }
  return box->result();
}
} // namespace

// Native panels use the visible AppKit window explicitly. Hidden presenter
// QWindows are not sheet hosts, including when a file chooser is opened from
// a material, render, report or settings window.
class NativeFilePanel final : public QObject {
public:
  explicit NativeFilePanel(QFileDialog* dialog) : m_dialog(dialog) {
    m_target = [MitcadNativeFilePanelTarget new];
    m_target->owner = this;
    m_panel = dialog->acceptMode() == QFileDialog::AcceptSave ? [NSSavePanel savePanel] : [NSOpenPanel openPanel];
    m_panel.delegate = m_target;
    m_panel.title = ns(dialog->windowTitle());
    m_panel.canCreateDirectories = !dialog->testOption(QFileDialog::ReadOnly);
    m_panel.showsHiddenFiles = dialog->filter().testFlag(QDir::Hidden);
    m_panel.canSelectHiddenExtension = YES;
    m_panel.extensionHidden = NO;
    m_panel.prompt = ns(title(dialog->labelText(QFileDialog::Accept)));
    m_panel.nameFieldLabel = ns(title(dialog->labelText(QFileDialog::FileName)));
    m_panel.directoryURL = [NSURL fileURLWithPath:ns(dialog->directory().absolutePath()) isDirectory:YES];
    const QStringList selected = dialog->selectedFiles();
    if (!selected.isEmpty()) {
      const QFileInfo initial(selected.first());
      if (initial.isAbsolute()) {
        const QString directory = initial.isDir() ? initial.absoluteFilePath() : initial.absolutePath();
        m_panel.directoryURL = [NSURL fileURLWithPath:ns(directory) isDirectory:YES];
      }
      if (!initial.isDir()) { m_panel.nameFieldStringValue = ns(initial.fileName()); }
    }
    if ([m_panel isKindOfClass:NSOpenPanel.class]) {
      NSOpenPanel* open = (NSOpenPanel*)m_panel;
      const bool directory = dialog->fileMode() == QFileDialog::Directory || dialog->testOption(QFileDialog::ShowDirsOnly);
      open.canChooseDirectories = directory;
      open.canChooseFiles = !directory;
      open.allowsMultipleSelection = dialog->fileMode() == QFileDialog::ExistingFiles;
      open.resolvesAliases = !dialog->testOption(QFileDialog::DontResolveSymlinks);
    }
    m_filters = dialog->nameFilters();
    m_filterIndex = std::max<qsizetype>(0, m_filters.indexOf(dialog->selectedNameFilter()));
    if (m_filters.size() > 1 && dialog->fileMode() != QFileDialog::Directory) {
      NSStackView* accessory = stack(true);
      add(accessory, label(title(dialog->labelText(QFileDialog::FileType))));
      m_popup = [[NSPopUpButton alloc] initWithFrame:NSMakeRect(0, 0, 280, 26) pullsDown:NO];
      for (QString filter : m_filters) {
        if (dialog->testOption(QFileDialog::HideNameFilterDetails)) {
          filter.remove(QRegularExpression(QStringLiteral("\\s*\\([^()]*\\)\\s*$")));
        }
        [m_popup addItemWithTitle:ns(filter)];
      }
      [m_popup selectItemAtIndex:m_filterIndex];
      m_popup.target = m_target;
      m_popup.action = @selector(filterChanged:);
      m_popup.accessibilityIdentifier = @"native.file.filter";
      add(accessory, m_popup);
      [accessory.widthAnchor constraintEqualToConstant:380].active = YES;
      accessory.frame = NSMakeRect(0, 0, 380, std::max<CGFloat>(40, accessory.fittingSize.height));
      m_panel.accessoryView = accessory;
      if ([m_panel isKindOfClass:NSOpenPanel.class]) { ((NSOpenPanel*)m_panel).accessoryViewDisclosed = YES; }
    }
    updateFilter(false);
    connect(dialog, &QObject::destroyed, this, [this] { cancel(); });
    connect(dialog, &QDialog::finished, this, [this] { if (!m_complete) { cancel(); } });
  }
  ~NativeFilePanel() override {
    m_target->owner = nullptr;
    m_popup.target = nil;
    m_panel.delegate = nil;
    m_panel.accessoryView = nil;
    if (m_panel.sheetParent != nil) { [m_panel.sheetParent endSheet:m_panel]; }
    [m_panel orderOut:nil];
  }
  QStringList patterns() const {
    if (m_filters.isEmpty()) { return {}; }
    QString filter = m_filters.value(m_filterIndex);
    const auto match = QRegularExpression(QStringLiteral("\\(([^()]*)\\)\\s*$")).match(filter);
    if (match.hasMatch()) { filter = match.captured(1); }
    return filter.split(QRegularExpression(QStringLiteral("\\s+")), Qt::SkipEmptyParts);
  }
  void updateFilter(bool changed) {
    if (m_dialog == nullptr) { return; }
    if (m_popup != nil) { m_filterIndex = m_popup.indexOfSelectedItem; }
    NSMutableArray<UTType*>* types = [NSMutableArray new];
    const QStringList wildcards = patterns();
    bool unrestricted = wildcards.isEmpty();
    for (const QString& pattern : wildcards) {
      if (!pattern.startsWith(QStringLiteral("*.")) || pattern.count(QLatin1Char('*')) != 1
          || pattern.contains(QLatin1Char('?')) || pattern.mid(2).contains(QLatin1Char('.'))) {
        unrestricted = true;
        break;
      }
      UTType* type = [UTType typeWithFilenameExtension:ns(pattern.mid(2))];
      if (type == nil) { unrestricted = true; break; }
      [types addObject:type];
    }
    m_panel.allowedContentTypes = unrestricted ? @[] : types;
    m_panel.allowsOtherFileTypes = unrestricted;
    [m_panel validateVisibleColumns];
    if (changed && !m_filters.isEmpty()) {
      const QPointer<NativeFilePanel> alive = this;
      m_dialog->selectNameFilter(m_filters.value(m_filterIndex));
      if (alive == nullptr || m_dialog == nullptr) { return; }
      QMetaObject::invokeMethod(m_dialog, "filterSelected", Qt::DirectConnection,
                                Q_ARG(QString, m_filters.value(m_filterIndex)));
    }
  }
  bool shouldEnable(NSURL* url) const {
    if (m_dialog == nullptr) { return false; }
    const QFileInfo file(qt(url.path));
    if (file.isDir() && (m_panel.treatsFilePackagesAsDirectories || ![NSWorkspace.sharedWorkspace isFilePackageAtPath:url.path])) {
      return true;
    }
    const QStringList wildcards = patterns();
    return wildcards.isEmpty() || wildcards.contains(QStringLiteral("*.*")) || QDir::match(wildcards, file.fileName());
  }
  void currentChanged() {
    if (m_dialog == nullptr || ![m_panel isKindOfClass:NSOpenPanel.class]) { return; }
    const QPointer<NativeFilePanel> alive = this;
    const QUrl url = QUrl::fromLocalFile(qt(m_panel.URL.path));
    QMetaObject::invokeMethod(m_dialog, "currentUrlChanged", Qt::DirectConnection, Q_ARG(QUrl, url));
    if (alive == nullptr || m_dialog == nullptr) { return; }
    QMetaObject::invokeMethod(m_dialog, "currentChanged", Qt::DirectConnection, Q_ARG(QString, url.toLocalFile()));
  }
  void directoryChanged() {
    if (m_dialog == nullptr) { return; }
    const QPointer<NativeFilePanel> alive = this;
    const QString directory = qt(m_panel.directoryURL.path);
    m_dialog->setDirectory(directory);
    if (alive == nullptr || m_dialog == nullptr) { return; }
    QMetaObject::invokeMethod(m_dialog, "directoryEntered", Qt::DirectConnection, Q_ARG(QString, directory));
    if (alive == nullptr || m_dialog == nullptr) { return; }
    const QUrl url = QUrl::fromLocalFile(directory);
    QMetaObject::invokeMethod(m_dialog, "directoryUrlEntered", Qt::DirectConnection, Q_ARG(QUrl, url));
  }
  QString fileName(QString path) const {
    if (m_dialog == nullptr || m_dialog->acceptMode() != QFileDialog::AcceptSave
        || QFileInfo(path).fileName().contains(QLatin1Char('.'))) { return path; }
    QString suffix = m_dialog->defaultSuffix();
    if (suffix.isEmpty()) {
      const auto wildcards = patterns();
      if (!wildcards.isEmpty() && wildcards.first().startsWith(QStringLiteral("*."))
          && !wildcards.first().mid(2).contains(QLatin1Char('*')) && !wildcards.first().contains(QLatin1Char('?'))) {
        suffix = wildcards.first().mid(2);
      }
    }
    if (!suffix.isEmpty()) { path += QLatin1Char('.') + suffix; }
    return path;
  }
  int exec() {
    NSWindow* host = currentHost(m_dialog);
    if (host != nil) {
      const QPointer<NativeFilePanel> owner = this;
      [m_panel beginSheetModalForWindow:host completionHandler:^(NSModalResponse response) {
        if (owner != nullptr) { owner->completed(response); }
      }];
      g_modalWindows.push_back(m_panel);
      pumpNativeEvents([this] { return m_complete; });
      g_modalWindows.pop_back();
    } else { completed([m_panel runModal]); }
    if (m_dialog == nullptr) { return QDialog::Rejected; }
    if (m_response != NSModalResponseOK) { m_dialog->reject(); return QDialog::Rejected; }
    QList<QUrl> urls;
    NSArray<NSURL*>* selected = [m_panel isKindOfClass:NSOpenPanel.class] ? ((NSOpenPanel*)m_panel).URLs
                                                                      : (m_panel.URL == nil ? @[] : @[m_panel.URL]);
    for (NSURL* url in selected) {
      urls += QUrl::fromLocalFile(fileName(qt(url.path).normalized(QString::NormalizationForm_C)));
    }
    if (urls.isEmpty()) { m_dialog->reject(); return QDialog::Rejected; }
    // Public QFileDialog setters preserve the existing single-file callers.
    // The result property also carries all files for the shared get-file helpers.
    m_dialog->setOption(QFileDialog::DontUseNativeDialog, true);
    m_dialog->setDirectory(qt(m_panel.directoryURL.path));
    if (!m_filters.isEmpty()) { m_dialog->selectNameFilter(m_filters.value(m_filterIndex)); }
    m_dialog->selectFile(urls.first().toLocalFile());
    m_dialog->setProperty("nativeSelectedUrls", QVariant::fromValue(urls));
    const QPointer<QFileDialog> presenter = m_dialog;
    QStringList paths;
    for (const QUrl& url : urls) { paths += url.toLocalFile(); }
    QMetaObject::invokeMethod(presenter, "urlsSelected", Qt::DirectConnection, Q_ARG(QList<QUrl>, urls));
    if (presenter == nullptr) { return QDialog::Rejected; }
    QMetaObject::invokeMethod(presenter, "filesSelected", Qt::DirectConnection, Q_ARG(QStringList, paths));
    if (presenter == nullptr) { return QDialog::Rejected; }
    if (urls.size() == 1) {
      const QUrl url = urls.first();
      QMetaObject::invokeMethod(presenter, "urlSelected", Qt::DirectConnection, Q_ARG(QUrl, url));
      if (presenter == nullptr) { return QDialog::Rejected; }
      const QString path = paths.first();
      QMetaObject::invokeMethod(presenter, "fileSelected", Qt::DirectConnection, Q_ARG(QString, path));
      if (presenter == nullptr) { return QDialog::Rejected; }
    }
    presenter->QDialog::done(QDialog::Accepted);
    return QDialog::Accepted;
  }
  void completed(NSModalResponse response) {
    if (m_complete) { return; }
    m_complete = true;
    m_response = response;
  }
  void cancel() {
    if (m_panel.sheetParent != nil) { [m_panel.sheetParent endSheet:m_panel returnCode:NSModalResponseCancel]; }
    [m_panel orderOut:nil];
    completed(NSModalResponseCancel);
  }
private:
  QPointer<QFileDialog> m_dialog;
  NSSavePanel* m_panel = nil;
  NSPopUpButton* m_popup = nil;
  MitcadNativeFilePanelTarget* m_target = nil;
  QStringList m_filters;
  qsizetype m_filterIndex = 0;
  NSModalResponse m_response = NSModalResponseCancel;
  bool m_complete = false;
};

class NativeColorPanel final : public QObject {
public:
  NativeColorPanel(const QColor& initial, QWidget* parent, const QString& text,
                   QColorDialog::ColorDialogOptions options) : m_parent(parent) {
    m_panel = NSColorPanel.sharedColorPanel;
    m_oldDelegate = m_panel.delegate;
    m_oldAccessory = m_panel.accessoryView;
    m_oldAlpha = m_panel.showsAlpha;
    m_oldTitle = m_panel.title;
    m_target = [MitcadNativeColorPanelTarget new];
    m_target->owner = this;
    m_panel.delegate = m_target;
    [m_panel setTarget:nil];
    [m_panel setAction:nil];
    m_panel.title = ns(text);
    m_panel.showsAlpha = options.testFlag(QColorDialog::ShowAlphaChannel);
    m_panel.color = [NSColor colorWithSRGBRed:initial.redF() green:initial.greenF() blue:initial.blueF() alpha:initial.alphaF()];
    NSStackView* buttons = stack(true);
    NSButton* cancel = [NSButton buttonWithTitle:ns(QColorDialog::tr("Cancel")) target:m_target action:@selector(cancel:)];
    cancel.keyEquivalent = @"\033";
    cancel.accessibilityIdentifier = @"native.color.cancel";
    add(buttons, cancel);
    NSButton* accept = [NSButton buttonWithTitle:ns(QColorDialog::tr("OK")) target:m_target action:@selector(accept:)];
    accept.keyEquivalent = @"\r";
    accept.accessibilityIdentifier = @"native.color.accept";
    add(buttons, accept);
    buttons.edgeInsets = NSEdgeInsetsMake(8, 8, 8, 8);
    buttons.frame = NSMakeRect(0, 0, std::max<CGFloat>(240, buttons.fittingSize.width),
                               std::max<CGFloat>(50, buttons.fittingSize.height));
    m_panel.accessoryView = buttons;
    if (parent != nullptr) { connect(parent, &QObject::destroyed, this, [this] { finish(false); }); }
  }
  ~NativeColorPanel() override {
    m_target->owner = nullptr;
    if (m_panel.sheetParent != nil) { [m_panel.sheetParent endSheet:m_panel]; }
    [m_panel orderOut:nil];
    m_panel.delegate = m_oldDelegate;
    [m_panel setTarget:nil];
    [m_panel setAction:nil];
    m_panel.accessoryView = m_oldAccessory;
    m_panel.showsAlpha = m_oldAlpha;
    m_panel.title = m_oldTitle;
  }
  QColor exec() {
    NSWindow* host = currentHost(m_parent);
    if (host != nil) { [host beginSheet:m_panel completionHandler:nil]; }
    else { [m_panel center]; [m_panel makeKeyAndOrderFront:nil]; }
    g_modalWindows.push_back(m_panel);
    pumpNativeEvents([this] { return m_complete; });
    g_modalWindows.pop_back();
    if (!m_accepted) { return {}; }
    NSColor* color = [m_panel.color colorUsingColorSpace:NSColorSpace.sRGBColorSpace];
    return color == nil ? QColor() : QColor::fromRgbF(color.redComponent, color.greenComponent, color.blueComponent, color.alphaComponent);
  }
  void finish(bool accepted) {
    if (m_complete) { return; }
    if (accepted && ![m_panel makeFirstResponder:nil]) { return; }
    m_complete = true;
    m_accepted = accepted;
    if (m_panel.sheetParent != nil) { [m_panel.sheetParent endSheet:m_panel returnCode:accepted ? NSModalResponseOK : NSModalResponseCancel]; }
    [m_panel orderOut:nil];
  }
private:
  QPointer<QWidget> m_parent;
  NSColorPanel* m_panel = nil;
  MitcadNativeColorPanelTarget* m_target = nil;
  id<NSWindowDelegate> m_oldDelegate = nil;
  NSView* m_oldAccessory = nil;
  NSString* m_oldTitle = nil;
  BOOL m_oldAlpha = NO;
  bool m_complete = false;
  bool m_accepted = false;
};

void setNativeDialogWindow(void* window) { g_documentWindow = (__bridge NSWindow*)window; }
void* nativeDialogWindow() { return (__bridge void*)g_documentWindow; }
bool nativeDialogsActive() { return g_documentWindow != nil; }
bool nativeDialogModalActive() { return !g_modalWindows.empty() || NSApp.modalWindow != nil; }
void waitNativeUntil(const std::function<bool()>& completed) { pumpNativeEvents(completed); }
std::unique_ptr<NativeWidgetView> createNativeWidgetView(QWidget* widget) {
  if (widget == nullptr) { return {}; }
  auto view = std::make_unique<AppKitWidgetView>(widget);
  view->refresh();
  return view;
}
int execNativeDialog(QDialog* dialog) {
  if (dialog == nullptr) { return QDialog::Rejected; }
  if (auto* file = qobject_cast<QFileDialog*>(dialog)) { return execNativeFileDialog(file); }
  if (auto* box = qobject_cast<QMessageBox*>(dialog)) { return messageBox(box); }
  NativeDialogController controller(dialog, true);
  return controller.exec();
}
int execNativeFileDialog(QFileDialog* dialog) {
  if (dialog == nullptr) { return QDialog::Rejected; }
  dialog->setProperty("nativeSelectedUrls", QVariant());
  NativeFilePanel panel(dialog);
  return panel.exec();
}
void* nativeFileDialogPanel() {
  for (auto it = g_modalWindows.rbegin(); it != g_modalWindows.rend(); ++it) {
    if ([*it isKindOfClass:NSSavePanel.class]) { return (__bridge void*)*it; }
  }
  return nullptr;
}
QList<QUrl> nativeFileDialogUrls(const QFileDialog* dialog) {
  if (dialog == nullptr) { return {}; }
  const QVariant result = dialog->property("nativeSelectedUrls");
  return result.isValid() ? result.value<QList<QUrl>>() : dialog->selectedUrls();
}
QColor getNativeColor(const QColor& initial, QWidget* parent, const QString& text, QColorDialog::ColorDialogOptions options) {
  NativeColorPanel panel(initial, parent, text, options);
  return panel.exec();
}
void showNativeWidget(QWidget* widget) {
  if (widget == nullptr) { return; }
  for (NativeDialogController* controller : g_modeless) {
    if (controller->widget() != widget) { continue; }
    if ([controller->window() isKindOfClass:MitcadNativeUtilityPanel.class]) {
      // Brought forward without the keyboard; a closed panel is being
      // destroyed and a new one replaces it (mitcad#819).
      if (!controller->window().visible) { continue; }
      [controller->window() orderFront:nil];
      return;
    }
    [controller->window() makeKeyAndOrderFront:nil];
    return;
  }
  auto* controller = new NativeDialogController(widget, false);
  g_modeless.push_back(controller);
  QObject::connect(controller, &QObject::destroyed, [controller] {
    g_modeless.erase(std::remove(g_modeless.begin(), g_modeless.end(), controller), g_modeless.end());
  });
  controller->show();
}

// QMenu's Cocoa items dispatch through Qt's window responder chain. The
// native document window has its own chain, so popup actions need an
// explicit target. Keep Qt actions guarded and invoke the chosen action
// after tracking, when it may safely rebuild or destroy the source menu.
class NativeMenuPopup {
public:
  explicit NativeMenuPopup(QMenu* menu) : m_source(menu) {
    m_target = [[MitcadNativeMenuTarget alloc] init];
    m_target->owner = this;
    m_native = menuFor(menu);
  }

  ~NativeMenuPopup() {
    m_target->owner = nullptr;
    for (const auto& binding : m_menus) binding->native.delegate = nil;
  }

  QAction* exec(const QPoint& point) {
    update(m_native);
    if (m_source && m_native.numberOfItems > 0) {
      const NSRect screen = NSScreen.screens.firstObject.frame;
      [m_native popUpMenuPositioningItem:nil atLocation:NSMakePoint(point.x(), NSMaxY(screen) - point.y()) inView:nil];
    }
    const std::size_t menuCount = m_menus.size();
    for (std::size_t i = 0; i < menuCount; ++i) close(m_menus[i]->native);
    if (m_source && m_chosen && m_chosen->isEnabled() && m_chosen->isVisible()) m_chosen->trigger();
    return m_chosen;
  }

  void choose(NSMenuItem* item) {
    const QPointer<QAction> action = actionFor(item);
    if (m_source && action && action->isEnabled() && action->isVisible() && !action->menu()) m_chosen = action;
  }

  bool enabled(NSMenuItem* item) const {
    const QPointer<QAction> action = actionFor(item);
    return m_source && action && action->isEnabled() && action->isVisible();
  }

  void update(NSMenu* native, bool announce = true) {
    MenuBinding* binding = bindingFor(native);
    if (!binding || binding->refreshing) return;
    binding->refreshing = true;
    if (announce && binding->source && !binding->announced) {
      binding->announced = true;
      emit binding->source->aboutToShow();
    }
    [native removeAllItems];
    const QPointer<QMenu> source = binding->source;
    if (m_source && source) {
      const auto actions = source->actions();
      const bool collapseSeparators = source->separatorsCollapsible();
      bool separatorPending = false;
      for (QAction* raw : actions) {
        QPointer<QAction> action(raw);
        if (!action || !action->isVisible()) continue;
        if (action->isSeparator()) {
          if (collapseSeparators) separatorPending = native.numberOfItems > 0;
          else [native addItem:NSMenuItem.separatorItem];
          continue;
        }
        if (separatorPending) {
          [native addItem:NSMenuItem.separatorItem];
          separatorPending = false;
        }
        NSMenuItem* item = [[NSMenuItem alloc] initWithTitle:ns(title(action->text()))
                                                    action:@selector(perform:) keyEquivalent:@""];
        item.target = m_target;
        item.tag = actionIndex(action);
        item.enabled = action->isEnabled();
        item.state = action->isCheckable() && action->isChecked() ? NSControlStateValueOn : NSControlStateValueOff;
        item.toolTip = ns(plain(action->toolTip()));
        setShortcut(item, action->shortcut());
        if (action->isIconVisibleInMenu() && !action->icon().isNull()) {
          item.image = image(action->icon().pixmap(QSize(32, 32)));
          item.image.size = NSMakeSize(16, 16);
          if (@available(macOS 27.0, *)) item.preferredImageVisibility = NSMenuItemImageVisibilityVisible;
        }
        if (QMenu* submenu = action->menu()) item.submenu = menuFor(submenu);
        [native addItem:item];
      }
    }
    binding->refreshing = false;
  }

  void close(NSMenu* native) {
    MenuBinding* binding = bindingFor(native);
    if (!binding || !binding->announced) return;
    binding->announced = false;
    if (binding->source) emit binding->source->aboutToHide();
  }

private:
  struct MenuBinding {
    QPointer<QMenu> source;
    NSMenu* __strong native = nil;
    bool announced = false;
    bool refreshing = false;
  };

  MenuBinding* bindingFor(NSMenu* native) const {
    for (const auto& binding : m_menus) if (binding->native == native) return binding.get();
    return nullptr;
  }

  NSMenu* menuFor(QMenu* source) {
    for (const auto& binding : m_menus) if (binding->source == source) return binding->native;
    auto binding = std::make_unique<MenuBinding>();
    binding->source = source;
    binding->native = [[NSMenu alloc] initWithTitle:ns(title(source->title()))];
    binding->native.delegate = m_target;
    NSMenu* native = binding->native;
    m_menus.push_back(std::move(binding));
    // Submenus need their current items before AppKit validates their
    // parent, but dynamic aboutToShow callbacks run only when opened.
    update(native, false);
    return native;
  }

  NSInteger actionIndex(QAction* action) {
    for (std::size_t i = 0; i < m_actions.size(); ++i) if (m_actions[i] == action) return static_cast<NSInteger>(i);
    m_actions.emplace_back(action);
    return static_cast<NSInteger>(m_actions.size() - 1);
  }

  QPointer<QAction> actionFor(NSMenuItem* item) const {
    const NSInteger index = item.tag;
    return item.target == m_target && index >= 0 && static_cast<std::size_t>(index) < m_actions.size()
               ? m_actions[static_cast<std::size_t>(index)] : QPointer<QAction>();
  }

  static void setShortcut(NSMenuItem* item, const QKeySequence& sequence) {
    mac::setKeyEquivalent(item, sequence);
  }

  QPointer<QMenu> m_source;
  QPointer<QAction> m_chosen;
  MitcadNativeMenuTarget* __strong m_target = nil;
  NSMenu* __strong m_native = nil;
  std::vector<std::unique_ptr<MenuBinding>> m_menus;
  std::vector<QPointer<QAction>> m_actions;
};

QAction* execNativeMenu(QMenu* menu, const QPoint& point) {
  if (menu == nullptr) { return nullptr; }
  NativeMenuPopup popup(menu);
  return popup.exec(point);
}

} // namespace mitcad

@implementation MitcadNativeColorField
- (BOOL)wantsUpdateLayer { return NO; }
- (NSRect)swatchRect {
  const CGFloat side = std::max<CGFloat>(8, NSHeight(self.bounds) - 10);
  return NSMakeRect(7, (NSHeight(self.bounds) - side) / 2, side + 8, side);
}
- (NSRect)wordsRect {
  const CGFloat left = NSMaxX(self.swatchRect) + 8;
  NSFont* font = self.font != nil ? self.font : [NSFont systemFontOfSize:NSFont.systemFontSize];
  const CGFloat height = [@"Ag" sizeWithAttributes:@{NSFontAttributeName: font}].height;
  return NSMakeRect(left, (NSHeight(self.bounds) - height) / 2,
                    std::max<CGFloat>(0, NSWidth(self.bounds) - left - 9), height);
}
- (void)drawRect:(NSRect)dirty {
  [super drawRect:dirty]; // AppKit's bezel, pressed state and focus ring
  const NSRect swatch = self.swatchRect;
  NSBezierPath* shape = [NSBezierPath bezierPathWithRoundedRect:swatch xRadius:2 yRadius:2];
  [NSGraphicsContext saveGraphicsState];
  [shape addClip];
  if (self.swatchColor == nil || self.swatchColor.alphaComponent < 1) {
    for (CGFloat y = NSMinY(swatch); y < NSMaxY(swatch); y += 3) {
      for (CGFloat x = NSMinX(swatch); x < NSMaxX(swatch); x += 3) {
        const int tile = int((x - NSMinX(swatch)) / 3) + int((y - NSMinY(swatch)) / 3);
        [(tile % 2 == 0 ? NSColor.controlBackgroundColor : NSColor.separatorColor) setFill];
        NSRectFill(NSMakeRect(x, y, 3, 3));
      }
    }
  }
  if (self.swatchColor != nil) {
    [self.swatchColor setFill];
    [shape fill];
  }
  [NSGraphicsContext restoreGraphicsState];
  [NSColor.separatorColor setStroke];
  shape.lineWidth = 1;
  [shape stroke];
  if (self.words.length > 0) {
    NSMutableParagraphStyle* paragraph = [NSMutableParagraphStyle new];
    paragraph.lineBreakMode = NSLineBreakByTruncatingTail;
    NSColor* ink = !self.enabled ? NSColor.disabledControlTextColor
                   : self.placeholder ? NSColor.placeholderTextColor : NSColor.controlTextColor;
    [self.words drawInRect:self.wordsRect withAttributes:@{
      NSFontAttributeName: self.font != nil ? self.font : [NSFont systemFontOfSize:NSFont.systemFontSize],
      NSForegroundColorAttributeName: ink, NSParagraphStyleAttributeName: paragraph
    }];
  }
}
- (void)viewDidChangeEffectiveAppearance {
  [super viewDidChangeEffectiveAppearance];
  self.needsDisplay = YES;
}
@end

@implementation MitcadNativeMenuTarget
- (void)perform:(NSMenuItem*)sender { if (owner) owner->choose(sender); }
- (BOOL)validateMenuItem:(NSMenuItem*)item { return owner && owner->enabled(item); }
- (void)menuNeedsUpdate:(NSMenu*)menu { if (owner) owner->update(menu); }
- (void)menuDidClose:(NSMenu*)menu { if (owner) owner->close(menu); }
@end

@implementation MitcadNativeAlertDetails
- (void)toggle:(id)sender {
  (void)sender;
  const BOOL expanded = disclosure.state == NSControlStateValueOn;
  details.hidden = !expanded;
  disclosure.title = expanded ? hideTitle : showTitle;
  [alert.accessoryView layoutSubtreeIfNeeded];
  alert.accessoryView.frame = NSMakeRect(0, 0, 420, alert.accessoryView.fittingSize.height);
  [alert layout];
}
@end

@implementation MitcadNativeCompletionTable
- (BOOL)acceptsFirstResponder { return NO; }
// The live shared list this table shows: the selection probe reads its
// presenter's selection through the table itself (mitcad#984).
- (QListWidget*)sharedList {
  id source = self.dataSource;
  return [source isKindOfClass:MitcadNativeTextView.class] ? [(MitcadNativeTextView*)source sharedCompletions] : nullptr;
}
- (NSInteger)presenterSelectedRows {
  QListWidget* list = [self sharedList];
  if (list == nullptr || list->selectionModel() == nullptr) return -1;
  return static_cast<NSInteger>(list->selectionModel()->selectedRows().size());
}
- (NSInteger)presenterSelectedRow {
  QListWidget* list = [self sharedList];
  if (list == nullptr || list->selectionModel() == nullptr) return -1;
  const QModelIndexList rows = list->selectionModel()->selectedRows();
  return rows.isEmpty() ? -1 : static_cast<NSInteger>(rows.first().row());
}
@end

@implementation MitcadNativeTextView
@synthesize placeholderString = _placeholderString;
- (mitcad::AppKitWidgetView*)presenterOwner {
  return [self.delegate isKindOfClass:MitcadNativeWidgetTarget.class]
      ? ((MitcadNativeWidgetTarget*)self.delegate)->owner : nullptr;
}
- (QListWidget*)sharedCompletions {
  auto* owner = [self presenterOwner];
  return owner != nullptr ? owner->textCompletions(self) : nullptr;
}
- (BOOL)sharedKey:(NSEvent*)event {
  auto* owner = [self presenterOwner];
  return owner != nullptr && owner->textKey(self, event);
}
- (void)keyDown:(NSEvent*)event {
  if (![self sharedKey:event]) [super keyDown:event];
}
- (void)closeCompletions {
  if (completionMonitor != nil) {
    [NSEvent removeMonitor:completionMonitor];
    completionMonitor = nil;
  }
  [completionPopover close];
}
- (void)refreshCompletions {
  QListWidget* list = [self sharedCompletions];
  if (list == nullptr || !list->isVisible() || list->count() == 0 || !self.window.visible ||
      [self isHiddenOrHasHiddenAncestor]) {
    [self closeCompletions];
    return;
  }
  if (completionPopover == nil) {
    completionTable = [[MitcadNativeCompletionTable alloc] initWithFrame:NSZeroRect];
    completionTable.headerView = nil;
    completionTable.dataSource = self;
    completionTable.delegate = self;
    completionTable.target = self;
    completionTable.action = @selector(chooseCompletion:);
    completionTable.accessibilityIdentifier = @"formulaCompletions";
    NSTableColumn* column = [[NSTableColumn alloc] initWithIdentifier:@"completion"];
    [completionTable addTableColumn:column];
    NSScrollView* scroll = [[NSScrollView alloc] initWithFrame:NSZeroRect];
    scroll.hasVerticalScroller = YES;
    scroll.autohidesScrollers = YES;
    scroll.documentView = completionTable;
    NSViewController* controller = [NSViewController new];
    controller.view = scroll;
    completionPopover = [NSPopover new];
    completionPopover.animates = NO;
    completionPopover.behavior = NSPopoverBehaviorApplicationDefined;
    completionPopover.contentViewController = controller;
  }
  // The shared completion widget owns its width, row count and reading
  // height; its current row and strings are the only completion state.
  const CGFloat width = std::max(1, list->width());
  const CGFloat height = std::max(1, list->height());
  completionTable.rowHeight = list->fontMetrics().height() + 6;
  completionTable.tableColumns.firstObject.width = width;
  [completionTable reloadData];
  [completionTable setFrameSize:NSMakeSize(width, std::max(height, list->count() * completionTable.rowHeight))];
  // The native selection follows the shared current row after every filter
  // change and Up/Down move, and is cleared with it, so it never keeps a
  // row of an earlier prefix (mitcad#984).
  const int current = list->currentRow();
  if (current >= 0 && current < list->count()) {
    [completionTable selectRowIndexes:[NSIndexSet indexSetWithIndex:NSUInteger(current)] byExtendingSelection:NO];
    [completionTable scrollRowToVisible:current];
  } else {
    [completionTable deselectAll:nil];
  }
  completionPopover.contentSize = NSMakeSize(width, height);
  if (!completionPopover.shown && self.window.firstResponder == self) {
    const NSRect caret = [self firstRectForCharacterRange:self.selectedRange actualRange:nullptr];
    const NSRect local = [self convertRect:[self.window convertRectFromScreen:caret] fromView:nil];
    [completionPopover showRelativeToRect:local ofView:self preferredEdge:NSRectEdgeMinY];
    // Completion never takes the keyboard from the multiline editor.
    [self.window makeKeyWindow];
    [self.window makeFirstResponder:self];
  }
  if (completionMonitor == nil && completionPopover.shown) {
    __weak MitcadNativeTextView* editor = self;
    completionMonitor = [NSEvent addLocalMonitorForEventsMatchingMask:NSEventMaskLeftMouseDown |
        NSEventMaskRightMouseDown handler:^NSEvent*(NSEvent* event) {
      MitcadNativeTextView* text = editor;
      if (text == nil) return event;
      const NSPoint point = event.window ? [event.window convertPointToScreen:event.locationInWindow] : NSEvent.mouseLocation;
      const NSRect editorFrame = [text.window convertRectToScreen:[text convertRect:text.visibleRect toView:nil]];
      if (!NSPointInRect(point, editorFrame) &&
          !NSPointInRect(point, text->completionPopover.contentViewController.view.window.frame)) {
        auto* owner = [text presenterOwner];
        if (owner != nullptr) owner->leaveText(text);
        [text closeCompletions];
      }
      return event;
    }];
  }
}
- (NSInteger)numberOfRowsInTableView:(NSTableView*)table {
  (void)table;
  QListWidget* list = [self sharedCompletions];
  return list != nullptr ? list->count() : 0;
}
- (NSView*)tableView:(NSTableView*)table viewForTableColumn:(NSTableColumn*)column row:(NSInteger)row {
  (void)table; (void)column;
  QListWidget* list = [self sharedCompletions];
  if (list == nullptr || row < 0 || row >= list->count()) return nil;
  NSTextField* field = [NSTextField labelWithString:list->item(static_cast<int>(row))->text().toNSString()];
  field.lineBreakMode = NSLineBreakByTruncatingTail;
  return field;
}
- (void)chooseCompletion:(id)sender {
  (void)sender;
  const NSInteger row = completionTable.clickedRow;
  auto* owner = [self presenterOwner];
  if (owner != nullptr && row >= 0) owner->completeText(self, static_cast<int>(row));
}
- (void)setPlaceholderString:(NSString*)text {
  if ([_placeholderString isEqualToString:text]) return;
  _placeholderString = [text copy];
  self.needsDisplay = YES;
}
- (void)setString:(NSString*)text {
  [super setString:text];
  self.needsDisplay = YES;
}
- (void)didChangeText {
  [super didChangeText];
  self.needsDisplay = YES;
}
- (void)drawRect:(NSRect)dirtyRect {
  [super drawRect:dirtyRect];
  if (self.string.length != 0 || self.placeholderString.length == 0) return;
  const NSPoint origin = self.textContainerOrigin;
  const CGFloat padding = self.textContainer.lineFragmentPadding;
  const NSRect rect = NSMakeRect(origin.x + padding, origin.y,
      std::max<CGFloat>(0, self.textContainer.size.width - 2 * padding),
      std::max<CGFloat>(0, NSHeight(self.bounds) - origin.y));
  [self.placeholderString drawInRect:rect withAttributes:@{
    NSFontAttributeName: self.font != nil ? self.font : [NSFont systemFontOfSize:NSFont.systemFontSize],
    NSForegroundColorAttributeName: NSColor.placeholderTextColor
  }];
}
@end

@implementation MitcadNativeWidgetTarget
- (void)perform:(id)sender { if (owner != nullptr) { owner->action(sender); } }
- (void)doubleClick:(id)sender { if (owner != nullptr) { owner->modelActivated(sender); } }
- (void)controlTextDidChange:(NSNotification*)notification { if (owner != nullptr) { owner->textChanged(notification.object, false); } }
- (void)controlTextDidEndEditing:(NSNotification*)notification { if (owner != nullptr) { owner->textChanged(notification.object, true); } }
- (BOOL)control:(NSControl*)control textShouldEndEditing:(NSText*)editor {
  (void)editor;
  return owner == nullptr || owner->textCanEnd(control);
}
- (BOOL)control:(NSControl*)control textView:(NSTextView*)textView doCommandBySelector:(SEL)command {
  (void)textView;
  if (!qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT"))
    qInfo().noquote() << QStringLiteral("Native field command: %1 selector=%2")
        .arg(QString::fromNSString(control.accessibilityIdentifier), QString::fromNSString(NSStringFromSelector(command)));
  if (owner != nullptr && (command == @selector(insertTab:) || command == @selector(insertBacktab:))) {
    return owner->focusNextField(control, command == @selector(insertBacktab:));
  }
  return NO;
}
- (void)textDidChange:(NSNotification*)notification { if (owner != nullptr) { owner->textViewChanged(notification.object); } }
- (void)textViewDidChangeSelection:(NSNotification*)notification {
  if (owner != nullptr) owner->textViewChanged(notification.object);
}
- (BOOL)textView:(NSTextView*)view clickedOnLink:(id)link atIndex:(NSUInteger)index {
  (void)index;
  return owner != nullptr && owner->linkClicked(view, link);
}
- (void)tabView:(NSTabView*)view didSelectTabViewItem:(NSTabViewItem*)item { if (owner != nullptr) { owner->tabChanged(view, item); } }
- (BOOL)tabView:(NSTabView*)view shouldSelectTabViewItem:(NSTabViewItem*)item {
  (void)item;
  return view.window == nil || [view.window makeFirstResponder:nil];
}
- (NSInteger)numberOfRowsInTableView:(NSTableView*)view { return owner == nullptr ? 0 : owner->children(owner->modelBinding(view), nil).count; }
- (NSView*)tableView:(NSTableView*)view viewForTableColumn:(NSTableColumn*)column row:(NSInteger)row {
  return owner == nullptr ? nil : owner->modelCell(view, column, owner->nodeAt(view, row));
}
- (BOOL)tableView:(NSTableView*)view shouldSelectRow:(NSInteger)row {
  return owner != nullptr && owner->modelSelectable(view, owner->nodeAt(view, row));
}
- (CGFloat)tableView:(NSTableView*)view heightOfRow:(NSInteger)row {
  return owner != nullptr ? owner->modelRowHeight(view, row) : view.rowHeight;
}
- (void)tableViewSelectionDidChange:(NSNotification*)notification { if (owner != nullptr) { owner->modelSelection(notification.object); } }
- (NSTableRowView*)tableView:(NSTableView*)view rowViewForRow:(NSInteger)row {
  return owner != nullptr ? owner->currentRowView(view, owner->nodeAt(view, row)) : nil;
}
- (NSInteger)outlineView:(NSOutlineView*)view numberOfChildrenOfItem:(id)item {
  return owner == nullptr ? 0 : owner->children(owner->modelBinding(view), item).count;
}
- (id)outlineView:(NSOutlineView*)view child:(NSInteger)index ofItem:(id)item {
  return owner == nullptr ? nil : owner->children(owner->modelBinding(view), item)[static_cast<NSUInteger>(index)];
}
- (BOOL)outlineView:(NSOutlineView*)view isItemExpandable:(id)item {
  return owner != nullptr && owner->children(owner->modelBinding(view), item).count > 0;
}
- (NSView*)outlineView:(NSOutlineView*)view viewForTableColumn:(NSTableColumn*)column item:(id)item {
  return owner == nullptr ? nil : owner->modelCell(view, column, item);
}
- (BOOL)outlineView:(NSOutlineView*)view isGroupItem:(id)item {
  return owner != nullptr && owner->modelSpansRow(view, item);
}
- (BOOL)outlineView:(NSOutlineView*)view shouldSelectItem:(id)item {
  return owner != nullptr && owner->modelSelectable(view, item);
}
- (CGFloat)outlineView:(NSOutlineView*)view heightOfRowByItem:(id)item {
  (void)item;
  return view.rowHeight;
}
- (void)outlineViewSelectionDidChange:(NSNotification*)notification { if (owner != nullptr) { owner->modelSelection(notification.object); } }
- (NSTableRowView*)outlineView:(NSOutlineView*)view rowViewForItem:(id)item {
  return owner != nullptr ? owner->currentRowView(view, item) : nil;
}
- (void)outlineViewItemDidExpand:(NSNotification*)notification {
  if (owner != nullptr) { owner->modelExpanded(notification.object, notification.userInfo[@"NSObject"], true); }
}
- (void)outlineViewItemDidCollapse:(NSNotification*)notification {
  if (owner != nullptr) { owner->modelExpanded(notification.object, notification.userInfo[@"NSObject"], false); }
}
// Opted-in row drags within one outline (mitcad#816).
- (id<NSPasteboardWriting>)outlineView:(NSOutlineView*)view pasteboardWriterForItem:(id)item {
  return owner == nullptr ? nil : owner->rowDragWriter(view, item);
}
- (void)outlineView:(NSOutlineView*)view draggingSession:(NSDraggingSession*)session
    willBeginAtPoint:(NSPoint)point forItems:(NSArray*)items {
  (void)session; (void)point; (void)items;
  if (owner != nullptr) { owner->rowDragBegan(view); }
}
- (NSDragOperation)outlineView:(NSOutlineView*)view validateDrop:(id<NSDraggingInfo>)info
                  proposedItem:(id)item proposedChildIndex:(NSInteger)index {
  return owner == nullptr ? NSDragOperationNone : owner->rowDropValidate(view, info, item, index);
}
- (BOOL)outlineView:(NSOutlineView*)view acceptDrop:(id<NSDraggingInfo>)info item:(id)item childIndex:(NSInteger)index {
  (void)item; (void)index;
  return owner != nullptr && owner->rowDropAccept(view, info);
}
- (void)outlineView:(NSOutlineView*)view draggingSession:(NSDraggingSession*)session
       endedAtPoint:(NSPoint)point operation:(NSDragOperation)operation {
  (void)session;
  if (owner != nullptr) { owner->rowDragEnded(view, point, operation); }
}
@end

@implementation MitcadNativeModelField
- (void)mouseDown:(NSEvent*)event {
  MitcadNativeModelField* field = self;
  NSWindow* window = self.window;
  const NSEventModifierFlags modifiers = event.modifierFlags &
      (NSEventModifierFlagCommand | NSEventModifierFlagControl | NSEventModifierFlagOption | NSEventModifierFlagShift);
  if (event.type == NSEventTypeLeftMouseDown && self.enabled && self.editable &&
      modifiers == 0 &&
      [self.delegate isKindOfClass:MitcadNativeModelCell.class]) {
    auto* cell = (MitcadNativeModelCell*)self.delegate;
    if (cell->owner != nullptr && cell.textField == self) cell->owner->modelCellPressed(cell);
  }
  if (field.window == window && window.visible && field.enabled && field.editable) [super mouseDown:event];
}
@end

@implementation MitcadNativeModelCell
- (void)controlTextDidBeginEditing:(NSNotification*)notification {
  (void)notification;
  userEdited = NO;
  [self refreshUnchangedEdit];
}
- (void)controlTextDidChange:(NSNotification*)notification {
  (void)notification;
  userEdited = YES;
}
- (void)refreshUnchangedEdit {
  NSTextField* field = self.textField;
  NSText* editor = field.currentEditor;
  if (userEdited || editor == nil || !index.isValid()) { return; }
  const QString value = mitcad::plain(index.data().toString());
  if (mitcad::qt(field.stringValue) == value && mitcad::qt(editor.string) == value) { return; }
  // Modal helpers can update the model while AppKit restores this cell's
  // field editor. An untouched editor follows that value; a real edit
  // keeps its text until the user commits or cancels it.
  const NSRange selection = editor.selectedRange;
  field.stringValue = mitcad::ns(value);
  editor.string = mitcad::ns(value);
  const NSUInteger start = std::min(selection.location, editor.string.length);
  editor.selectedRange = NSMakeRange(start, std::min(selection.length, editor.string.length - start));
  mitcad::mac::logNativeCellEdit("an untouched editor follows its model value", field);
}
- (void)controlTextDidEndEditing:(NSNotification*)notification {
  if (!finishing) { mitcad::mac::logNativeCellEdit("an edit ends without Return, Escape or Tab", self); }
  // A read-only cell's text is only selected, and a discarded edit is not
  // the model's (mitcad#814).
  if (discarding || ![(NSTextField*)notification.object isEditable]) { return; }
  const QPointer<mitcad::AppKitWidgetView> alive(owner);
  const QString text = QString::fromUtf8([(NSTextField*)notification.object stringValue].UTF8String);
  // A focused cell does not become a user edit when a modal helper updates
  // the model behind it. In particular, never write its old expression back.
  const bool changed = userEdited;
  userEdited = NO;
  if (changed && owner != nullptr && index.isValid() && text != mitcad::plain(index.data().toString())) {
    owner->modelEdit(binding, index, text, Qt::EditRole);
  }
  if (alive != nullptr) { alive->cellEditEnded(); }
}
- (void)viewWillMoveToWindow:(NSWindow*)window {
  if (window == nil && self.textField.currentEditor != nil) {
    mitcad::mac::logNativeCellEdit("an edited cell leaves its window", self);
  }
  [super viewWillMoveToWindow:window];
}
// The shared editor's keys: Return commits, Escape discards, Tab commits
// and moves on, each without reaching the dialog's default or cancel button.
- (BOOL)control:(NSControl*)control textView:(NSTextView*)textView doCommandBySelector:(SEL)command {
  (void)textView;
  if (control != self.textField || !self.textField.editable) { return NO; }
  if (command == @selector(insertNewline:)) { return [self finishEditing:YES]; }
  if (command == @selector(cancelOperation:)) { return [self finishEditing:NO]; }
  if (command == @selector(insertTab:) || command == @selector(insertBacktab:)) {
    return owner != nullptr && owner->modelEditNext(control, command == @selector(insertBacktab:));
  }
  return NO;
}
- (BOOL)finishEditing:(BOOL)commit {
  NSTextField* field = self.textField;
  NSWindow* window = self.window;
  if (field == nil || window == nil || field.currentEditor == nil || !field.editable) { return NO; }
  finishing = YES;
  if (!commit) {
    discarding = YES;
    [field abortEditing];
    discarding = NO;
    if (index.isValid()) { field.stringValue = mitcad::ns(mitcad::plain(index.data().toString())); }
  }
  // Committing ends the field editor through controlTextDidEndEditing. In a
  // dialog the focus returns to the table, as the shared view's editor
  // closes; document cards leave keys to the document window.
  NSTableView* table = mitcad::mac::owningTable(self);
  const BOOL dialog = [window isKindOfClass:MitcadNativeSecondaryWindow.class];
  [window makeFirstResponder:(dialog && table != nil) ? (NSResponder*)table : nil];
  finishing = NO;
  // A reload, selection or structure change that waited for the edit.
  if (owner != nullptr) { owner->cellEditEnded(); }
  return YES;
}
- (void)applyTone {
  // An emphasized selection shows the selection's own text, as the shared
  // delegate leaves selected cells to the highlighted-text colour.
  const BOOL emphasized = self.backgroundStyle == NSBackgroundStyleEmphasized;
  NSColor* color = (tone == nil || emphasized) ? NSColor.labelColor : tone;
  if (self.textField != nil && ![self.textField.textColor isEqual:color]) { self.textField.textColor = color; }
  if (fill != nil) { self.needsDisplay = YES; }
}
- (void)setBackgroundStyle:(NSBackgroundStyle)style {
  [super setBackgroundStyle:style];
  [self applyTone];
}
- (void)drawRect:(NSRect)dirty {
  if (fill != nil && self.backgroundStyle != NSBackgroundStyleEmphasized) {
    [fill setFill];
    NSRectFillUsingOperation(NSIntersectionRect(self.bounds, dirty), NSCompositingOperationSourceOver);
  }
  [super drawRect:dirty];
}
- (void)checked:(id)sender {
  NSTableView* table = mitcad::mac::owningTable(self);
  if (owner != nullptr && table != nil) owner->currentRowClicked(table, [table rowForView:self]);
  if (owner != nullptr && index.isValid()) { owner->modelEdit(binding, index, [(NSButton*)sender state] == NSControlStateValueOn ? Qt::Checked : Qt::Unchecked, Qt::CheckStateRole); }
}
@end

@implementation MitcadNativeModelHoverButton
- (BOOL)becomeFirstResponder {
  const BOOL accepted = [super becomeFirstResponder];
  if (accepted) { [self publishHover:YES]; }
  return accepted;
}
- (BOOL)resignFirstResponder {
  const BOOL accepted = [super resignFirstResponder];
  if (accepted) { [self publishHover:NO]; }
  return accepted;
}
- (void)updateTrackingAreas {
  if (hoverArea != nil) { [self removeTrackingArea:hoverArea]; }
  hoverArea = [[NSTrackingArea alloc] initWithRect:NSZeroRect
      options:NSTrackingMouseEnteredAndExited | NSTrackingActiveInActiveApp | NSTrackingInVisibleRect
      owner:self userInfo:nil];
  [self addTrackingArea:hoverArea];
  [super updateTrackingAreas];
}
- (void)mouseEntered:(NSEvent*)event {
  (void)event;
  [self publishHover:YES];
}
- (void)mouseExited:(NSEvent*)event {
  (void)event;
  [self publishHover:NO];
}
- (void)publishHover:(BOOL)entered {
  if (presenter != nullptr && (!entered || modelIndex.isValid())) {
    const QModelIndex index = entered ? QModelIndex(modelIndex) : QModelIndex();
    QMetaObject::invokeMethod(presenter.data(), "entered", Qt::DirectConnection,
                              Q_ARG(QModelIndex, index));
  }
}
@end

@implementation MitcadNativeWindowTarget
- (BOOL)windowShouldClose:(NSWindow*)sender { (void)sender; return owner == nullptr ? YES : owner->closeRequested(); }
// A utility panel's presenter shortcuts (MitcadNativeUtilityPanel, mitcad#819).
- (BOOL)mitcadDispatchShortcut:(NSEvent*)event { return owner != nullptr && owner->dispatchShortcut(event); }
@end

namespace {
// Table keys come before the window's key equivalents (mitcad#814): Return
// and Escape in a cell editor end that edit rather than pressing the
// dialog's default or cancel button; F2 on a focused table edits its
// current cell, as in the shared view.
BOOL tableKeyEquivalent(NSWindow* window, NSEvent* event) {
  const NSEventModifierFlags modifiers = event.modifierFlags &
      (NSEventModifierFlagCommand | NSEventModifierFlagControl | NSEventModifierFlagOption | NSEventModifierFlagShift);
  if (event.type != NSEventTypeKeyDown || modifiers != 0) { return NO; }
  NSResponder* responder = window.firstResponder;
  const unsigned short key = event.keyCode;
  if (key == 36 || key == 76 || key == 53) { // Return, keypad Enter, Escape
    if (![responder isKindOfClass:NSTextView.class] || !((NSTextView*)responder).isFieldEditor) { return NO; }
    id edited = ((NSTextView*)responder).delegate;
    if (![edited isKindOfClass:NSTextField.class]) { return NO; }
    id cell = ((NSTextField*)edited).delegate;
    if (key != 53 && [cell isKindOfClass:MitcadNativeWidgetTarget.class]) {
      MitcadNativeWidgetTarget* widgets = (MitcadNativeWidgetTarget*)cell;
      return widgets->owner != nullptr && widgets->owner->commitReturn((NSControl*)edited);
    }
    return [cell isKindOfClass:MitcadNativeModelCell.class] && [(MitcadNativeModelCell*)cell finishEditing:key != 53];
  }
  if (key == 120 && [responder isKindOfClass:NSTableView.class]) { // F2
    id target = ((NSTableView*)responder).delegate;
    if ([target isKindOfClass:MitcadNativeWidgetTarget.class]) {
      MitcadNativeWidgetTarget* widgets = (MitcadNativeWidgetTarget*)target;
      return widgets->owner != nullptr && widgets->owner->modelEditKey((NSTableView*)responder);
    }
  }
  return NO;
}
} // namespace

@implementation MitcadNativeSecondaryWindow
- (BOOL)performKeyEquivalent:(NSEvent*)event {
  if ([self.firstResponder isKindOfClass:MitcadNativeShortcutField.class])
    return [(MitcadNativeShortcutField*)self.firstResponder performKeyEquivalent:event];
  if (mitcad::mac::routeNativeRuleKey(self, event)) return YES;
  if ([self.firstResponder isKindOfClass:MitcadNativeTextView.class] &&
      [(MitcadNativeTextView*)self.firstResponder sharedKey:event]) return YES;
  if (tableKeyEquivalent(self, event)) return YES;
  const QKeySequence sequence = mitcad::mac::keySequence(event);
  if (!sequence.isEmpty() && sequence[0].keyboardModifiers() == Qt::NoModifier &&
      (sequence[0].key() == Qt::Key_Return || sequence[0].key() == Qt::Key_Enter) &&
      [self.firstResponder isKindOfClass:NSTextView.class]) {
    NSTextView* text = (NSTextView*)self.firstResponder;
    if (text.editable && !text.isFieldEditor) {
      [text insertNewline:self];
      return YES;
    }
  }
  if ([self.delegate isKindOfClass:MitcadNativeWindowTarget.class]) {
    MitcadNativeWindowTarget* target = (MitcadNativeWindowTarget*)self.delegate;
    if (target->owner && target->owner->dispatchShortcut(event)) return YES;
  }
  const NSEventModifierFlags modifiers = event.modifierFlags &
      (NSEventModifierFlagCommand | NSEventModifierFlagControl | NSEventModifierFlagOption | NSEventModifierFlagShift);
  if (modifiers == NSEventModifierFlagCommand &&
      [event.charactersIgnoringModifiers.lowercaseString isEqualToString:@"w"]) {
    // This window owns its close key even while application activation
    // or the hidden Qt presenter's focus bookkeeping is still pending.
    if ([self makeFirstResponder:nil]) { [self performClose:self]; }
    return YES;
  }
  return [super performKeyEquivalent:event];
}
- (void)keyDown:(NSEvent*)event {
  const NSEventModifierFlags modifiers = event.modifierFlags &
      (NSEventModifierFlagCommand | NSEventModifierFlagControl | NSEventModifierFlagOption | NSEventModifierFlagShift);
  if (modifiers == 0 && event.keyCode == 53) { [self cancelOperation:self]; return; }
  [super keyDown:event];
}
- (void)cancelOperation:(id)sender {
  (void)sender;
  if ([self.delegate isKindOfClass:MitcadNativeWindowTarget.class]) {
    MitcadNativeWindowTarget* target = (MitcadNativeWindowTarget*)self.delegate;
    if (target->owner != nullptr) { target->owner->closeRequested(); }
  }
}
@end

@implementation MitcadNativeFilePanelTarget
- (void)filterChanged:(id)sender { (void)sender; if (owner != nullptr) { owner->updateFilter(true); } }
- (BOOL)panel:(id)sender shouldEnableURL:(NSURL*)url { (void)sender; return owner != nullptr && owner->shouldEnable(url); }
- (void)panelSelectionDidChange:(id)sender { (void)sender; if (owner != nullptr) { owner->currentChanged(); } }
- (void)panel:(id)sender didChangeToDirectoryURL:(NSURL*)url { (void)sender; (void)url; if (owner != nullptr) { owner->directoryChanged(); } }
- (NSString*)panel:(id)sender userEnteredFilename:(NSString*)filename confirmed:(BOOL)confirmed {
  (void)sender; (void)confirmed;
  return owner == nullptr ? filename : mitcad::ns(owner->fileName(mitcad::qt(filename)));
}
@end

@implementation MitcadNativeColorPanelTarget
- (void)accept:(id)sender { (void)sender; if (owner != nullptr) { owner->finish(true); } }
- (void)cancel:(id)sender { (void)sender; if (owner != nullptr) { owner->finish(false); } }
- (BOOL)windowShouldClose:(NSWindow*)sender { (void)sender; if (owner != nullptr) { owner->finish(false); } return NO; }
@end

@implementation MitcadNativeTable
@synthesize emptyLabel = _emptyLabel;
- (void)tile {
  [super tile];
  mitcad::placeEmptyLabel(self, _emptyLabel);
}
- (void)setFrameSize:(NSSize)size {
  [super setFrameSize:size];
  mitcad::placeEmptyLabel(self, _emptyLabel);
}
- (NSInteger)presenterSelectedRows {
  MitcadNativeWidgetTarget* target = (MitcadNativeWidgetTarget*)self.delegate;
  return target != nil && target->owner != nullptr ? target->owner->presenterSelectedRows(self) : -1;
}
- (void)rightMouseDown:(NSEvent*)event {
  MitcadNativeWidgetTarget* target = (MitcadNativeWidgetTarget*)self.delegate;
  if (target != nil && target->owner != nullptr) { target->owner->modelContext(self, [self convertPoint:event.locationInWindow fromView:nil]); }
}
// MITCAD_LOG_NATIVE_EDITS: who gives a table the focus (mitcad#814).
- (BOOL)becomeFirstResponder {
  const BOOL accepted = [super becomeFirstResponder];
  if (accepted) { mitcad::mac::logNativeCellEdit("the table takes the keyboard focus", self); }
  return accepted;
}
@end

@implementation MitcadNativeOutline
@synthesize emptyLabel = _emptyLabel;
- (void)tile {
  [super tile];
  mitcad::placeEmptyLabel(self, _emptyLabel);
}
- (void)setFrameSize:(NSSize)size {
  [super setFrameSize:size];
  mitcad::placeEmptyLabel(self, _emptyLabel);
}
- (NSInteger)presenterSelectedRows {
  MitcadNativeWidgetTarget* target = (MitcadNativeWidgetTarget*)self.delegate;
  return target != nil && target->owner != nullptr ? target->owner->presenterSelectedRows(self) : -1;
}
- (void)mouseDown:(NSEvent*)event {
  MitcadNativeWidgetTarget* target = (MitcadNativeWidgetTarget*)self.delegate;
  if (target != nil && target->owner != nullptr)
    target->owner->currentRowClicked(self, [self rowAtPoint:[self convertPoint:event.locationInWindow fromView:nil]]);
  [super mouseDown:event];
}
- (BOOL)becomeFirstResponder {
  const BOOL accepted = [super becomeFirstResponder];
  if (accepted) { mitcad::mac::logNativeCellEdit("the outline takes the keyboard focus", self); }
  return accepted;
}
- (void)rightMouseDown:(NSEvent*)event {
  MitcadNativeWidgetTarget* target = (MitcadNativeWidgetTarget*)self.delegate;
  if (target != nil && target->owner != nullptr) { target->owner->modelContext(self, [self convertPoint:event.locationInWindow fromView:nil]); }
}
// Control-click opens the presenter's own context menu too (mitcad#816).
- (NSMenu*)menuForEvent:(NSEvent*)event {
  MitcadNativeWidgetTarget* target = (MitcadNativeWidgetTarget*)self.delegate;
  if (target != nil && target->owner != nullptr) { target->owner->modelContext(self, [self convertPoint:event.locationInWindow fromView:nil]); }
  return nil;
}
// Only rows of a presenter that moves them by dragging (mitcad#816). The
// presenter alone decides, from anywhere on the row as the default tree's
// press-and-move does; AppKit's point heuristics for the cell under the
// pointer (such as its selectable label) must not refuse a row it offers.
- (BOOL)canDragRowsWithIndexes:(NSIndexSet*)rows atPoint:(NSPoint)point {
  (void)point;
  MitcadNativeWidgetTarget* target = (MitcadNativeWidgetTarget*)self.delegate;
  return target != nil && target->owner != nullptr && target->owner->rowsDraggable(self, rows);
}
@end

@implementation MitcadNativeSpannedRowView
- (void)layout {
  [super layout];
  // NSTableView lays a group cell out at the columns' combined width,
  // even when its full row is wider. Apply the shared first-column span
  // after that layout, retaining AppKit's leading disclosure inset.
  for (NSView* child in self.subviews) {
    if (![child isKindOfClass:NSTableCellView.class]) continue;
    NSRect frame = child.frame;
    const CGFloat width = std::max<CGFloat>(0, NSMaxX(self.bounds) - NSMinX(frame));
    if (std::abs(NSWidth(frame) - width) < 0.5) continue;
    frame.size.width = width;
    child.frame = frame;
    [child layoutSubtreeIfNeeded];
  }
}
@end

@implementation MitcadNativeScreenshot
- (BOOL)isFlipped { return YES; }
- (NSRect)shownRect {
  if (presenter == nullptr || presenter->sourceImage().isNull()) { return NSZeroRect; }
  const QImage& image = presenter->sourceImage();
  const CGFloat scale = std::min(self.bounds.size.width / image.width(), self.bounds.size.height / image.height());
  const NSSize size = NSMakeSize(image.width() * scale, image.height() * scale);
  return NSMakeRect((self.bounds.size.width - size.width) / 2, (self.bounds.size.height - size.height) / 2,
                    size.width, size.height);
}
- (void)refresh {
  if (presenter == nullptr) {
    if (bitmap != nil) { bitmap = nil; self.needsDisplay = YES; ++mitcad::g_widgetPropertyWrites; }
    return;
  }
  const QImage& source = presenter->sourceImage();
  bool changed = false;
  if (imageKey != source.cacheKey()) {
    imageKey = source.cacheKey();
    bitmap = mitcad::image(QPixmap::fromImage(source));
    changed = true;
  }
  if (drawnCrop != presenter->cropRect()) {
    drawnCrop = presenter->cropRect();
    changed = true;
  }
  if (changed) { self.needsDisplay = YES; ++mitcad::g_widgetPropertyWrites; }
}
- (void)drawRect:(NSRect)dirty {
  (void)dirty;
  const NSRect shown = [self shownRect];
  if (NSIsEmptyRect(shown) || bitmap == nil) { return; }
  [bitmap drawInRect:shown fromRect:NSZeroRect operation:NSCompositingOperationSourceOver fraction:1
     respectFlipped:YES hints:nil];
  NSRect keep = NSZeroRect;
  if (dragging) {
    keep = NSIntersectionRect(shown, NSMakeRect(std::min(from.x, to.x), std::min(from.y, to.y),
                                               std::abs(to.x - from.x), std::abs(to.y - from.y)));
  } else if (presenter != nullptr && !presenter->cropRect().isNull()) {
    const QRect crop = presenter->cropRect();
    const CGFloat scale = shown.size.width / presenter->sourceImage().width();
    keep = NSMakeRect(shown.origin.x + crop.x() * scale, shown.origin.y + crop.y() * scale,
                      crop.width() * scale, crop.height() * scale);
  }
  if (!NSIsEmptyRect(keep)) {
    NSBezierPath* outside = [NSBezierPath bezierPathWithRect:shown];
    [outside appendBezierPathWithRect:keep];
    outside.windingRule = NSWindingRuleEvenOdd;
    [[NSColor colorWithWhite:0 alpha:0.45] setFill];
    [outside fill];
    [NSColor.controlAccentColor setStroke];
    NSBezierPath* border = [NSBezierPath bezierPathWithRect:keep];
    border.lineWidth = 2;
    [border stroke];
  }
}
- (void)mouseDown:(NSEvent*)event {
  if (presenter == nullptr || presenter->sourceImage().isNull()) { return; }
  from = to = [self convertPoint:event.locationInWindow fromView:nil];
  dragging = YES;
  self.needsDisplay = YES;
}
- (void)mouseDragged:(NSEvent*)event {
  if (!dragging) { return; }
  to = [self convertPoint:event.locationInWindow fromView:nil];
  self.needsDisplay = YES;
}
- (void)mouseUp:(NSEvent*)event {
  if (!dragging || presenter == nullptr) { return; }
  to = [self convertPoint:event.locationInWindow fromView:nil];
  dragging = NO;
  const NSRect shown = [self shownRect];
  if (!NSIsEmptyRect(shown)) {
    const CGFloat scale = presenter->sourceImage().width() / shown.size.width;
    const auto point = [&](NSPoint point) {
      return QPoint(static_cast<int>((point.x - shown.origin.x) * scale),
                    static_cast<int>((point.y - shown.origin.y) * scale));
    };
    const QRect crop = QRect(point(from), point(to)).normalized().intersected(presenter->sourceImage().rect());
    if (crop.width() >= 8 && crop.height() >= 8) { presenter->setCropRect(crop); }
  }
  self.needsDisplay = YES;
}
@end

@implementation MitcadNativePaneSelector
- (void)choose:(NSButton*)sender {
  NSTabView* pages = self.pages;
  if (pages != nil && sender.tag >= 0 && sender.tag < pages.numberOfTabViewItems) {
    [pages selectTabViewItemAtIndex:sender.tag];
  }
  [self sync]; // also when the page kept the selection (a refused edit)
}
- (void)sync {
  NSTabView* pages = self.pages;
  const NSInteger shown = pages.selectedTabViewItem != nil ? [pages indexOfTabViewItem:pages.selectedTabViewItem] : -1;
  for (NSView* view in self.arrangedSubviews) {
    if (![view isKindOfClass:MitcadNativePaneButton.class]) continue;
    MitcadNativePaneButton* button = (MitcadNativePaneButton*)view;
    const BOOL on = button.tag == shown;
    // A click toggles a button's own state; the tab view's page decides.
    button.state = on ? NSControlStateValueOn : NSControlStateValueOff;
    if (button.chosen == on && button.accessibilityValue != nil) continue;
    // The shared header's chosen pane: its icon and name in the accent.
    button.chosen = on;
    button.contentTintColor = on ? NSColor.controlAccentColor : nil;
    button.accessibilityValue = @(on ? 1 : 0);
    button.needsDisplay = YES;
  }
}
@end

// The same icon and caption bands as the shared Settings header. AppKit's
// NSImageAbove centres the pair and loses the tint's top gap (mitcad#992).
@implementation MitcadNativePaneCell
- (NSRect)imageRectForBounds:(NSRect)bounds {
  const CGFloat y = self.controlView.flipped ? NSMinY(bounds) + 7 : NSMaxY(bounds) - 7 - 24;
  return NSMakeRect(NSMidX(bounds) - 12, y, 24, 24);
}
- (NSRect)titleRectForBounds:(NSRect)bounds {
  const CGFloat y = self.controlView.flipped ? NSMinY(bounds) + 34 : NSMaxY(bounds) - 34 - 18;
  return NSMakeRect(NSMinX(bounds), y, NSWidth(bounds), 18);
}
- (void)drawInteriorWithFrame:(NSRect)frame inView:(NSView*)view {
  [self drawImage:self.image withFrame:[self imageRectForBounds:frame] inView:view];
  NSColor* tint = ((NSButton*)view).contentTintColor;
  NSColor* ink = self.enabled ? (tint != nil ? tint : NSColor.labelColor) : NSColor.disabledControlTextColor;
  NSMutableParagraphStyle* centered = [NSMutableParagraphStyle new];
  centered.alignment = NSTextAlignmentCenter;
  NSAttributedString* caption = [[NSAttributedString alloc] initWithString:self.title
      attributes:@{NSFontAttributeName: self.font, NSForegroundColorAttributeName: ink, NSParagraphStyleAttributeName: centered}];
  [self drawTitle:caption withFrame:[self titleRectForBounds:frame] inView:view];
}
@end

@implementation MitcadNativePaneButton
- (BOOL)wantsUpdateLayer { return NO; }
- (void)drawRect:(NSRect)dirty {
  if (self.chosen) {
    // The shared header's rounded accent tint behind the chosen pane.
    [[NSColor.controlAccentColor colorWithAlphaComponent:0.16] setFill];
    [[NSBezierPath bezierPathWithRoundedRect:NSInsetRect(self.bounds, 2, 2) xRadius:9 yRadius:9] fill];
  }
  [super drawRect:dirty];
}
- (void)viewDidChangeEffectiveAppearance {
  [super viewDidChangeEffectiveAppearance];
  self.needsDisplay = YES;
}
@end

@implementation MitcadNativeActionButton
- (void)keyDown:(NSEvent*)event {
  const QKeySequence sequence = mitcad::mac::keySequence(event);
  if (self.enabled && !sequence.isEmpty() && sequence[0].key() == Qt::Key_Tab &&
      (sequence[0].keyboardModifiers() & ~Qt::ShiftModifier) == Qt::NoModifier &&
      [self.target isKindOfClass:MitcadNativeWidgetTarget.class]) {
    auto* target = (MitcadNativeWidgetTarget*)self.target;
    if (target->owner != nullptr && target->owner->focusNextField(
        self, sequence[0].keyboardModifiers().testFlag(Qt::ShiftModifier))) return;
  }
  [super keyDown:event];
}
@end

@implementation MitcadNativeHoverButton
- (void)updateTrackingAreas {
  if (hoverArea != nil) { [self removeTrackingArea:hoverArea]; }
  hoverArea = [[NSTrackingArea alloc] initWithRect:NSZeroRect
                                       options:NSTrackingMouseEnteredAndExited | NSTrackingActiveInKeyWindow | NSTrackingInVisibleRect
                                         owner:self userInfo:nil];
  [self addTrackingArea:hoverArea];
  [super updateTrackingAreas];
}
- (void)mouseEntered:(NSEvent*)event {
  (void)event;
  if (presenter != nullptr && self.enabled && !self.hidden) {
    QEnterEvent enter{QPointF(), QPointF(), QPointF()};
    QCoreApplication::sendEvent(presenter, &enter);
  }
}
- (void)mouseExited:(NSEvent*)event {
  (void)event;
  if (presenter != nullptr) {
    QEvent leave(QEvent::Leave);
    QCoreApplication::sendEvent(presenter, &leave);
  }
}
@end

@implementation MitcadNativePopup
@synthesize placeholderString = _placeholderString;
- (void)setPlaceholderString:(NSString*)text {
  if ([_placeholderString isEqualToString:text] || (_placeholderString.length == 0 && text.length == 0)) return;
  _placeholderString = [text copy];
  // VoiceOver and the native probes read what an unset popup shows.
  self.accessibilityPlaceholderValue = text.length > 0 ? text : nil;
  [self invalidateIntrinsicContentSize];
  self.needsDisplay = YES;
}
// A detached cell with the placeholder as its only, selected item: AppKit's
// own title geometry for this popup's font, size and bezel. The popup's menu
// gets no such item, so the user can never choose it.
- (NSPopUpButtonCell*)placeholderCell {
  NSPopUpButtonCell* own = (NSPopUpButtonCell*)self.cell;
  NSPopUpButtonCell* cell = [[NSPopUpButtonCell alloc] initTextCell:@"" pullsDown:NO];
  [cell addItemWithTitle:_placeholderString];
  [cell selectItemAtIndex:0];
  cell.font = self.font;
  cell.controlSize = own.controlSize;
  cell.bezelStyle = own.bezelStyle;
  cell.bordered = own.bordered;
  cell.arrowPosition = own.arrowPosition;
  return cell;
}
- (NSSize)intrinsicContentSize {
  NSSize size = [super intrinsicContentSize];
  // "Slicer default" stays whole beside short choices such as Yes and No.
  if (_placeholderString.length > 0) size.width = std::max<CGFloat>(size.width, std::ceil([self placeholderCell].cellSize.width));
  return size;
}
- (void)drawRect:(NSRect)dirtyRect {
  [super drawRect:dirtyRect];
  if (self.selectedItem != nil || _placeholderString.length == 0) return;
  NSFont* font = self.font != nil ? self.font
                                  : [NSFont systemFontOfSize:[NSFont systemFontSizeForControlSize:self.controlSize]];
  const NSRect title = [[self placeholderCell] titleRectForBounds:self.bounds];
  const CGFloat line = std::ceil(font.ascender - font.descender + font.leading);
  NSMutableParagraphStyle* paragraph = [NSMutableParagraphStyle new];
  paragraph.lineBreakMode = NSLineBreakByTruncatingTail;
  // The theme's placeholder grey in either appearance, as the shared combo
  // and the native colour field draw an unset value.
  NSColor* ink = self.enabled ? NSColor.placeholderTextColor : NSColor.disabledControlTextColor;
  [_placeholderString drawInRect:NSMakeRect(NSMinX(title), NSMidY(title) - line / 2, NSWidth(title), line)
                  withAttributes:@{NSFontAttributeName: font, NSForegroundColorAttributeName: ink,
                                   NSParagraphStyleAttributeName: paragraph}];
}
- (void)viewDidChangeEffectiveAppearance {
  [super viewDidChangeEffectiveAppearance];
  self.needsDisplay = YES;
}
- (BOOL)acceptsFirstResponder { return self.enabled; }
- (void)mouseDown:(NSEvent*)event {
  // Choosing a value with the pointer also establishes the next Tab stop.
  // Keep AppKit's actual menu tracking and the shared combo action intact.
  if (self.enabled) [self.window makeFirstResponder:self];
  [super mouseDown:event];
}
- (void)keyDown:(NSEvent*)event {
  const QKeySequence sequence = mitcad::mac::keySequence(event);
  if (!sequence.isEmpty() && sequence[0].key() == Qt::Key_Tab &&
      (sequence[0].keyboardModifiers() & ~Qt::ShiftModifier) == Qt::NoModifier &&
      [self.target isKindOfClass:MitcadNativeWidgetTarget.class]) {
    auto* target = (MitcadNativeWidgetTarget*)self.target;
    if (target->owner != nullptr && target->owner->focusNextField(self, sequence[0].keyboardModifiers().testFlag(Qt::ShiftModifier))) return;
  }
  [super keyDown:event];
}
@end

@implementation MitcadNativeSlider
- (BOOL)acceptsFirstResponder { return self.enabled; }
- (void)mouseDown:(NSEvent*)event {
  // AppKit's slider tracking can change the value without taking keyboard
  // focus. A real click must also establish where Tab and arrow keys start,
  // before a continuous action asks the presenter to refresh this control.
  if (self.enabled) [self.window makeFirstResponder:self];
  self.pointerTracking = YES;
  if (!qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT"))
    qInfo().noquote() << QStringLiteral("Native slider tracking begin: %1 value=%2 continuous=%3 event=%4 current=%5 pressed=%6")
        .arg(QString::fromNSString(self.accessibilityIdentifier)).arg(self.doubleValue).arg(self.continuous)
        .arg(int(event.type)).arg(int(NSApp.currentEvent.type)).arg(qulonglong(NSEvent.pressedMouseButtons));
  [super mouseDown:event];
  if (!qEnvironmentVariableIsEmpty("MITCAD_TEST_INPUT"))
    qInfo().noquote() << QStringLiteral("Native slider tracking end: %1 value=%2")
        .arg(QString::fromNSString(self.accessibilityIdentifier)).arg(self.doubleValue);
  self.pointerTracking = NO;
}
- (void)keyDown:(NSEvent*)event {
  const QKeySequence sequence = mitcad::mac::keySequence(event);
  if (self.enabled && !sequence.isEmpty() && sequence[0].key() == Qt::Key_Tab &&
      (sequence[0].keyboardModifiers() & ~Qt::ShiftModifier) == Qt::NoModifier &&
      [self.target isKindOfClass:MitcadNativeWidgetTarget.class]) {
    auto* target = (MitcadNativeWidgetTarget*)self.target;
    if (target->owner != nullptr && target->owner->focusNextField(self, sequence[0].keyboardModifiers().testFlag(Qt::ShiftModifier))) return;
  }
  if (!self.enabled || sequence.isEmpty() || sequence[0].keyboardModifiers() != Qt::NoModifier) {
    [super keyDown:event];
    return;
  }
  // Match the shared integer slider's documented single step, rather than
  // AppKit's fractional default increment (which can round asymmetrically).
  NSInteger value = self.integerValue;
  switch (sequence[0].key()) {
  case Qt::Key_Left: case Qt::Key_Down: value -= self.keyboardStep; break;
  case Qt::Key_Right: case Qt::Key_Up: value += self.keyboardStep; break;
  case Qt::Key_Home: value = static_cast<NSInteger>(self.minValue); break;
  case Qt::Key_End: value = static_cast<NSInteger>(self.maxValue); break;
  default: [super keyDown:event]; return;
  }
  value = std::clamp(value, static_cast<NSInteger>(self.minValue), static_cast<NSInteger>(self.maxValue));
  if (value != self.integerValue) {
    self.integerValue = value;
    [self sendAction:self.action to:self.target];
  }
}
@end

@implementation MitcadNativeShortcutField
- (void)clearShortcut:(id)sender {
  (void)sender;
  if (edit) edit->clear();
  self.stringValue = @"";
}
- (BOOL)becomeFirstResponder { return YES; }
- (BOOL)acceptsFirstResponder { return YES; }
- (void)mouseDown:(NSEvent*)event { (void)event; [self.window makeFirstResponder:self]; }
- (BOOL)performKeyEquivalent:(NSEvent*)event {
  if (self.window.firstResponder != self) { return NO; }
  [self keyDown:event];
  return YES;
}
- (void)keyDown:(NSEvent*)event {
  if (edit == nullptr) { return; }
  const QKeySequence sequence = mitcad::mac::keySequence(event);
  if (sequence.isEmpty()) return;
  edit->setKeySequence(sequence);
  self.stringValue = sequence.toString(QKeySequence::NativeText).toNSString();
}
@end
