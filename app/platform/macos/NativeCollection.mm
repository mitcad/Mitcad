// SPDX-License-Identifier: MIT
// Tile grids, tile catalogues, split panes and utility panels for the generic
// presenter bridge (NativeCollection.hpp; mitcad#813, mitcad#819).
#include "NativeCollection.hpp"
#include "NativeKeys.hpp"

#import <AppKit/AppKit.h>
#import <QuartzCore/QuartzCore.h>

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <limits>
#include <memory>
#include <vector>

#include <QAbstractItemModel>
#include <QAbstractItemView>
#include <QAction>
#include <QBoxLayout>
#include <QBuffer>
#include <QByteArray>
#include <QEvent>
#include <QIcon>
#include <QImage>
#include <QItemSelectionModel>
#include <QKeySequence>
#include <QLabel>
#include <QLayout>
#include <QLayoutItem>
#include <QLineEdit>
#include <QList>
#include <QListView>
#include <QMetaObject>
#include <QObject>
#include <QPersistentModelIndex>
#include <QPixmap>
#include <QPointer>
#include <QSettings>
#include <QSizePolicy>
#include <QSlider>
#include <QSpacerItem>
#include <QSplitter>
#include <QString>
#include <QStringList>
#include <QTimer>
#include <QTreeView>
#include <QVariant>
#include <QWidget>

namespace mitcad::mac {
class NativeCatalogue;
} // namespace mitcad::mac

@interface MitcadNativeTileView ()
// The picture, star and captions for the tile's size and frame.
- (void)placeSubviews;
@end

@interface MitcadNativeTileItem : NSCollectionViewItem
@end

@interface MitcadNativeTileTarget : NSObject <NSCollectionViewDataSource, NSCollectionViewDelegate> {
@public
  mitcad::mac::NativeTileGrid* owner;
}
- (void)star:(id)sender;
@end

@interface MitcadNativeGroupNode : NSObject {
@public
  QPersistentModelIndex index;
  NSMutableArray<MitcadNativeGroupNode*>* children;
}
@end

@interface MitcadNativeGroupTarget : NSObject <NSOutlineViewDataSource, NSOutlineViewDelegate> {
@public
  mitcad::mac::NativeGroupList* owner;
}
- (void)star:(id)sender;
@end

@interface MitcadNativeSearchTarget : NSObject <NSSearchFieldDelegate> {
@public
  mitcad::mac::NativeCatalogue* owner;
}
- (void)search:(id)sender;
@end

// The catalogue's container: its search field is the window's first field.
@interface MitcadNativeCatalogueView : NSView {
@public
  __weak NSSearchField* search;
}
@end

@interface MitcadNativeSplitView : NSView {
@public
  BOOL horizontal;
  NSMutableArray<NSView*>* panes;
  NSMutableArray* held; // per divider: its dragged size's constraint, else NSNull
}
- (void)dragDivider:(NSInteger)index with:(NSEvent*)event;
@end

@interface MitcadNativeSplitDivider : NSView {
@public
  __weak MitcadNativeSplitView* split;
  NSInteger index;
}
@end

namespace mitcad::mac {
namespace {

NSString* const kTileItem = @"MitcadNativeTile";
// Qt's grid gap between tiles (PartGrid::setTileSize), and the largest
// picture of the hover card (PartGrid's kHoverImage).
constexpr int kGap = 6;
constexpr int kHoverPicture = 256;
constexpr CGFloat kDivider = 7;
// Priorities below the shared layout's preferences and the window body's
// width (NativeLayout.mm, NativeWindowFit.hpp): a pane's minimum yields to
// the window's width before a scrolled body would widen; the split's
// proportions hold against a row's expanding control, which takes the
// row's spare width over the row's own hugging. No preference here may
// grow the dialog's scrolled body: its height is the shared layout's.
const NSLayoutPriority kPaneMinimum = NSLayoutPriorityDefaultHigh - 50;
const NSLayoutPriority kSplitRatio = NSLayoutPriorityDefaultLow - 10;
const NSLayoutPriority kSplitKeep = NSLayoutPriorityDefaultLow - 5;
const NSLayoutPriority kSplitDragged = NSLayoutPriorityDefaultLow - 2;
const NSLayoutPriority kRowExpanding = NSLayoutPriorityDefaultLow - 40;
const NSLayoutPriority kRowHugging = NSLayoutPriorityDefaultLow - 50;

NSString* ns(const QString& text) { return text.toNSString(); }
QString qt(NSString* text) { return text == nil ? QString() : QString::fromNSString(text); }

NSImage* nativeImage(const QPixmap& pixmap) {
  if (pixmap.isNull()) {
    return nil;
  }
  QByteArray data;
  QBuffer buffer(&data);
  buffer.open(QIODevice::WriteOnly);
  pixmap.toImage().save(&buffer, "PNG");
  NSImage* image = [[NSImage alloc] initWithData:[NSData dataWithBytes:data.constData()
                                                                length:static_cast<NSUInteger>(data.size())]];
  // The pixmap's pixels at its device pixel ratio: sharp on Retina.
  const qreal ratio = std::max<qreal>(1.0, pixmap.devicePixelRatio());
  image.size = NSMakeSize(pixmap.width() / ratio, pixmap.height() / ratio);
  return image;
}

// A model's picture (a pixmap, an image or an icon) for a tile of `side`.
QPixmap pixmapOf(const QVariant& value, int side) {
  switch (value.typeId()) {
  case QMetaType::QPixmap:
    return value.value<QPixmap>();
  case QMetaType::QImage:
    return QPixmap::fromImage(value.value<QImage>());
  case QMetaType::QIcon:
    return value.value<QIcon>().pixmap(QSize(side, side));
  default:
    return {};
  }
}

// The shared star colour of the part picker (PartStar.cpp, partStarColour).
NSColor* starColour() { return [NSColor colorWithSRGBRed:232.0 / 255.0 green:170.0 / 255.0 blue:0.0 alpha:1.0]; }

void showStar(NSButton* button, bool on) {
  button.image = [NSImage imageWithSystemSymbolName:on ? @"star.fill" : @"star" accessibilityDescription:nil];
  button.contentTintColor = on ? starColour() : NSColor.tertiaryLabelColor;
  button.accessibilityValue = on ? ns(QObject::tr("On")) : ns(QObject::tr("Off"));
}

NSButton* starButton() {
  NSButton* button = [[NSButton alloc] initWithFrame:NSZeroRect];
  [button setButtonType:NSButtonTypeMomentaryChange];
  button.bordered = NO;
  button.title = @"";
  button.imagePosition = NSImageOnly;
  button.imageScaling = NSImageScaleProportionallyUpOrDown;
  button.accessibilityLabel = ns(QObject::tr("Star"));
  return button;
}

// A property naming a model role; `fallback` when it is not set.
int roleOf(const QObject* object, const char* name, int fallback) {
  const QVariant value = object != nullptr ? object->property(name) : QVariant();
  bool ok = false;
  const int role = value.toInt(&ok);
  return value.isValid() && ok ? role : fallback;
}

// Whether the layout item is `widget` or holds it.
bool holds(QLayoutItem* item, const QWidget* widget) {
  if (item == nullptr || widget == nullptr) {
    return false;
  }
  if (QWidget* own = item->widget()) {
    return own == widget || own->isAncestorOf(widget);
  }
  if (QLayout* layout = item->layout()) {
    for (int i = 0; i < layout->count(); ++i) {
      if (holds(layout->itemAt(i), widget)) {
        return true;
      }
    }
  }
  return false;
}

// Calls the invokable `method` of `from` or of its nearest ancestor with it.
bool invokeOnAncestor(QObject* from, const QByteArray& method) {
  if (method.isEmpty()) {
    return false;
  }
  const QByteArray signature = QMetaObject::normalizedSignature(QByteArray(method + "()").constData());
  for (QObject* object = from; object != nullptr; object = object->parent()) {
    if (object->metaObject()->indexOfMethod(signature.constData()) >= 0) {
      return QMetaObject::invokeMethod(object, method.constData(), Qt::DirectConnection);
    }
  }
  return false;
}

NSStackView* stackOf(bool horizontal) {
  NSStackView* stack = [[NSStackView alloc] initWithFrame:NSZeroRect];
  stack.orientation = horizontal ? NSUserInterfaceLayoutOrientationHorizontal : NSUserInterfaceLayoutOrientationVertical;
  stack.alignment = horizontal ? NSLayoutAttributeCenterY : NSLayoutAttributeLeading;
  stack.spacing = 8;
  stack.detachesHiddenViews = YES;
  stack.translatesAutoresizingMaskIntoConstraints = NO;
  return stack;
}

void addFullWidth(NSStackView* stack, NSView* view) {
  view.translatesAutoresizingMaskIntoConstraints = NO;
  [stack addArrangedSubview:view];
  [view.leadingAnchor constraintEqualToAnchor:stack.leadingAnchor].active = YES;
  [view.trailingAnchor constraintEqualToAnchor:stack.trailingAnchor].active = YES;
}

// The generic bridge wraps labels; a toolbar caption ("Tiles:") stays on
// one line at its full width.
void singleLine(NSView* view) {
  if ([view isKindOfClass:NSTextField.class] && !((NSTextField*)view).editable) {
    NSTextField* field = (NSTextField*)view;
    field.cell.wraps = NO;
    field.cell.scrollable = NO;
    field.cell.usesSingleLineMode = YES;
    field.maximumNumberOfLines = 1;
    field.lineBreakMode = NSLineBreakByClipping;
    [field setContentCompressionResistancePriority:NSLayoutPriorityDefaultHigh
                                   forOrientation:NSLayoutConstraintOrientationHorizontal];
    [field setContentHuggingPriority:NSLayoutPriorityDefaultHigh forOrientation:NSLayoutConstraintOrientationHorizontal];
  }
  for (NSView* child in view.subviews) {
    singleLine(child);
  }
}

// A control with a Qt stretch factor takes the row's spare width (its row
// hugs below kRowExpanding, the split's proportions hold above it).
void expanding(NSView* view, CGFloat least) {
  [view.widthAnchor constraintGreaterThanOrEqualToConstant:least].active = YES;
  NSLayoutConstraint* fill = [view.widthAnchor constraintEqualToConstant:4000];
  fill.priority = kRowExpanding;
  fill.active = YES;
}

// The borderless card that shows a tile's larger picture and details while
// the pointer rests on it (PartGrid's hover). It never takes the keyboard.
NSPanel* hoverPanel() {
  NSPanel* panel = [[NSPanel alloc] initWithContentRect:NSMakeRect(0, 0, 200, 120)
                                              styleMask:NSWindowStyleMaskBorderless | NSWindowStyleMaskNonactivatingPanel
                                                backing:NSBackingStoreBuffered
                                                  defer:YES];
  panel.releasedWhenClosed = NO;
  panel.opaque = NO;
  panel.backgroundColor = NSColor.clearColor;
  panel.hasShadow = YES;
  panel.ignoresMouseEvents = YES;
  panel.hidesOnDeactivate = YES;
  panel.level = NSPopUpMenuWindowLevel;
  panel.identifier = @"native.tileHover";
  return panel;
}

void fillHoverPanel(NSPanel* panel, NSImage* image, NSString* text) {
  NSVisualEffectView* effect = [[NSVisualEffectView alloc] initWithFrame:NSZeroRect];
  effect.material = NSVisualEffectMaterialToolTip;
  effect.state = NSVisualEffectStateActive;
  effect.blendingMode = NSVisualEffectBlendingModeBehindWindow;
  effect.wantsLayer = YES;
  effect.layer.cornerRadius = 8;
  effect.layer.masksToBounds = YES;
  NSStackView* stack = stackOf(false);
  stack.spacing = 6;
  stack.edgeInsets = NSEdgeInsetsMake(10, 10, 10, 10);
  if (image != nil) {
    NSImageView* picture = [NSImageView imageViewWithImage:image];
    picture.imageScaling = NSImageScaleProportionallyDown;
    picture.translatesAutoresizingMaskIntoConstraints = NO;
    [picture.widthAnchor constraintEqualToConstant:image.size.width].active = YES;
    [picture.heightAnchor constraintEqualToConstant:image.size.height].active = YES;
    [stack addArrangedSubview:picture];
  }
  NSTextField* label = [NSTextField wrappingLabelWithString:text];
  label.font = [NSFont toolTipsFontOfSize:0];
  label.preferredMaxLayoutWidth = 300;
  label.selectable = NO;
  [stack addArrangedSubview:label];
  [effect addSubview:stack];
  [NSLayoutConstraint activateConstraints:@[
    [stack.leadingAnchor constraintEqualToAnchor:effect.leadingAnchor],
    [stack.trailingAnchor constraintEqualToAnchor:effect.trailingAnchor],
    [stack.topAnchor constraintEqualToAnchor:effect.topAnchor],
    [stack.bottomAnchor constraintEqualToAnchor:effect.bottomAnchor]
  ]];
  panel.contentView = effect;
  [effect layoutSubtreeIfNeeded];
  [panel setContentSize:stack.fittingSize];
}

// Constraints along a split view's axis.
NSLayoutConstraint* along(NSView* a, bool aEnd, NSView* b, bool bEnd, bool horizontal, CGFloat constant) {
  if (horizontal) {
    return [(aEnd ? a.trailingAnchor : a.leadingAnchor) constraintEqualToAnchor:(bEnd ? b.trailingAnchor : b.leadingAnchor)
                                                                       constant:constant];
  }
  return [(aEnd ? a.bottomAnchor : a.topAnchor) constraintEqualToAnchor:(bEnd ? b.bottomAnchor : b.topAnchor)
                                                                constant:constant];
}

NSLayoutDimension* lengthOf(NSView* view, bool horizontal) { return horizontal ? view.widthAnchor : view.heightAnchor; }

void pinAcross(NSView* view, NSView* split, bool horizontal) {
  if (horizontal) {
    [view.topAnchor constraintEqualToAnchor:split.topAnchor].active = YES;
    [view.bottomAnchor constraintEqualToAnchor:split.bottomAnchor].active = YES;
  } else {
    [view.leadingAnchor constraintEqualToAnchor:split.leadingAnchor].active = YES;
    [view.trailingAnchor constraintEqualToAnchor:split.trailingAnchor].active = YES;
  }
}

} // namespace

// ---------------------------------------------------------------------------
// The tile grid of an icon-mode list view

class NativeTileGrid final : public QObject, public NativeWidgetView {
public:
  explicit NativeTileGrid(QAbstractItemView* view) : m_view(view) {
    m_keyRole = roleOf(view, "nativeTileKeyRole", Qt::DisplayRole);
    m_imageRole = roleOf(view, "nativeTileImageRole", Qt::DecorationRole);
    m_previewRole = roleOf(view, "nativeTilePreviewRole", -1);
    m_subtitleRole = roleOf(view, "nativeTileSubtitleRole", -1);
    m_starRole = roleOf(view, "nativeTileStarRole", -1);
    m_badgeRole = roleOf(view, "nativeTileBadgeRole", -1);
    m_disabledRole = roleOf(view, "nativeTileDisabledRole", -1);
    m_target = [MitcadNativeTileTarget new];
    m_target->owner = this;
    m_layout = [NSCollectionViewFlowLayout new];
    m_layout.minimumInteritemSpacing = kGap;
    m_layout.minimumLineSpacing = kGap;
    m_layout.sectionInset = NSEdgeInsetsMake(8, 8, 8, 8);
    m_collection = [[MitcadNativeTileCollection alloc] initWithFrame:NSMakeRect(0, 0, 480, 320)];
    m_collection->owner = this;
    m_collection.collectionViewLayout = m_layout;
    m_collection.selectable = YES;
    m_collection.allowsMultipleSelection = NO;
    m_collection.allowsEmptySelection = YES;
    m_collection.backgroundColors = @[NSColor.clearColor];
    m_collection.autoresizingMask = NSViewWidthSizable;
    [m_collection registerClass:MitcadNativeTileItem.class forItemWithIdentifier:kTileItem];
    m_collection.dataSource = m_target;
    m_collection.delegate = m_target;
    m_collection.accessibilityIdentifier = ns(view->objectName());
    m_collection.accessibilityLabel = ns(view->accessibleName().isEmpty() ? view->toolTip() : view->accessibleName());
    m_scroll = [[NSScrollView alloc] initWithFrame:NSMakeRect(0, 0, 480, 320)];
    m_scroll.documentView = m_collection;
    m_scroll.hasVerticalScroller = YES;
    m_scroll.hasHorizontalScroller = NO;
    m_scroll.autohidesScrollers = YES;
    m_scroll.borderType = NSNoBorder;
    m_scroll.drawsBackground = NO;
    m_scroll.translatesAutoresizingMaskIntoConstraints = NO;
    // At least one whole tile of the largest size, as the Qt view's minimum.
    [m_scroll.widthAnchor constraintGreaterThanOrEqualToConstant:std::max(160, view->minimumWidth())].active = YES;
    [m_scroll.heightAnchor constraintGreaterThanOrEqualToConstant:std::max(160, std::min(view->minimumHeight(), 400))]
        .active = YES;
    for (QTimer* timer : {&m_reloadTimer, &m_selectionTimer, &m_sizeTimer, &m_hoverTimer}) {
      timer->setSingleShot(true);
    }
    connect(&m_reloadTimer, &QTimer::timeout, this, [this] { reload(); });
    connect(&m_selectionTimer, &QTimer::timeout, this, [this] { syncSelection(true); });
    connect(&m_sizeTimer, &QTimer::timeout, this, [this] {
      if (geometryChanged()) {
        reconfigureVisible();
      }
    });
    connect(&m_hoverTimer, &QTimer::timeout, this, [this] { showHover(); });
    // A replaced model and a silent icon size have no signal of their own.
    m_checkTimer.setInterval(750);
    connect(&m_checkTimer, &QTimer::timeout, this, [this] { check(); });
    m_checkTimer.start();
    view->installEventFilter(this);
    m_scroll.hidden = view->isHidden();
    reload();
  }

  ~NativeTileGrid() override {
    for (QTimer* timer : {&m_checkTimer, &m_reloadTimer, &m_selectionTimer, &m_sizeTimer, &m_hoverTimer}) {
      timer->stop();
    }
    if (m_hoverCard != nil) {
      [m_hoverCard orderOut:nil];
      m_hoverCard = nil;
    }
    if (m_view != nullptr) {
      m_view->removeEventFilter(this);
    }
    for (const auto& connection : m_connections) {
      disconnect(connection);
    }
    // AppKit's data source, delegate, targets and item views do not own
    // this object: clear their back pointers before any of them is used.
    m_target->owner = nullptr;
    m_collection->owner = nullptr;
    m_collection.dataSource = nil;
    m_collection.delegate = nil;
    for (MitcadNativeTileView* tile in m_tiles) {
      tile->owner = nullptr;
      tile->star.target = nil;
    }
  }

  void* nativeView() const override { return (__bridge void*)m_scroll; }
  void refresh() override {
    check();
    if (m_reloadTimer.isActive()) {
      reload();
    }
  }

  NSScrollView* scrollView() const { return m_scroll; }
  // The views whose keyboard focus counts as the grid's (its catalogue).
  void setFocusRoot(NSView* root) { m_focusRoot = root; }
  NSInteger count() const { return static_cast<NSInteger>(m_rows.size()); }

  NSCollectionViewItem* item(NSCollectionView* collection, NSIndexPath* path) {
    NSCollectionViewItem* made = [collection makeItemWithIdentifier:kTileItem forIndexPath:path];
    if ([made.view isKindOfClass:MitcadNativeTileView.class]) {
      configure((MitcadNativeTileView*)made.view, path.item);
    }
    return made;
  }

  // A press on the grid: the tile current, then the Qt view's click or,
  // for a second click, its double click and activation.
  void pointer(NSEvent* event) {
    hideHover();
    m_hoverTile = -1;
    if (m_collection.window != nil) {
      [m_collection.window makeFirstResponder:m_collection];
    }
    const NSPoint point = [m_collection convertPoint:event.locationInWindow fromView:nil];
    NSIndexPath* path = [m_collection indexPathForItemAtPoint:point];
    if (path == nil || path.item < 0 || path.item >= count()) {
      return;
    }
    choose(path.item, std::max<NSInteger>(1, event.clickCount));
  }

  // The grid's keys: the arrows, Home and End move the current tile as
  // in the Qt view; Return or Enter activates it (PartGrid's Return).
  bool key(NSEvent* event) {
    const NSEventModifierFlags modifiers =
        event.modifierFlags & (NSEventModifierFlagCommand | NSEventModifierFlagControl | NSEventModifierFlagOption);
    if (modifiers != 0) {
      return false;
    }
    switch (event.keyCode) {
    case 123:
      step(-1, 0);
      return true;
    case 124:
      step(1, 0);
      return true;
    case 125:
      step(0, 1);
      return true;
    case 126:
      step(0, -1);
      return true;
    case 115:
      stepTo(0);
      return true;
    case 119:
      stepTo(count() - 1);
      return true;
    case 36:
    case 76:
      return activateCurrent();
    default:
      return false;
    }
  }

  // Return on the focused grid before the dialog's default button, and
  // the presenter's own shortcut actions (PartGrid's Ctrl+D, Ctrl+Shift+D)
  // while the grid or its catalogue has the keyboard.
  bool keyEquivalent(NSEvent* event) {
    if (!ownsFirstResponder()) {
      return false;
    }
    const QKeySequence sequence = keySequence(event);
    if (sequence.isEmpty()) {
      return false;
    }
    const QKeyCombination combination = sequence[0];
    if (combination.keyboardModifiers() == Qt::NoModifier) {
      const bool enter = combination.key() == Qt::Key_Return || combination.key() == Qt::Key_Enter;
      return enter && m_collection.window.firstResponder == m_collection && activateCurrent();
    }
    return triggerShortcut(sequence);
  }

  // A tile's star: the tile current, as a press on the Qt star makes it,
  // and the model asked to turn its star on or off.
  void starClicked(NSInteger tile) {
    const QModelIndex index = indexOf(tile);
    if (!index.isValid() || m_starRole < 0 || m_model == nullptr) {
      return;
    }
    QPointer<NativeTileGrid> alive(this);
    QPointer<QAbstractItemModel> model(m_model.data());
    const QPersistentModelIndex kept(index);
    if (!select(index) || alive == nullptr || model == nullptr || !kept.isValid()) {
      return;
    }
    model->setData(kept, !kept.data(m_starRole).toBool(), m_starRole);
  }

  // AppKit changed the selection itself (keys the grid did not take).
  void nativeSelected() {
    NSIndexPath* path = m_collection.selectionIndexPaths.anyObject;
    if (path != nil && path.item != currentTile()) {
      select(indexOf(path.item));
    }
  }

  void hover(NSInteger tile, bool entered) {
    if (!entered) {
      if (tile == m_hoverTile) {
        m_hoverTile = -1;
        hideHover();
      }
      return;
    }
    hideHover();
    m_hoverTile = tile;
    m_hoverTimer.start(650);
  }

  void hideHover() {
    m_hoverTimer.stop();
    if (m_hoverCard != nil) {
      [m_hoverCard orderOut:nil];
    }
  }

  // Up and Down in the catalogue's search: the next or previous tile in
  // order (PartGrid::step).
  void stepLinear(int by) {
    const NSInteger total = count();
    if (total == 0) {
      return;
    }
    const NSInteger current = currentTile();
    stepTo(current < 0 ? 0 : std::clamp<NSInteger>(current + by, 0, total - 1));
  }

protected:
  bool eventFilter(QObject* watched, QEvent* event) override {
    if (watched == m_view.data() &&
        (event->type() == QEvent::Show || event->type() == QEvent::Hide || event->type() == QEvent::EnabledChange)) {
      m_scroll.hidden = m_view->isHidden();
    }
    return QObject::eventFilter(watched, event);
  }

private:
  void check() {
    if (m_view == nullptr) {
      return;
    }
    if (m_model != m_view->model() || m_selection != m_view->selectionModel()) {
      reload();
      return;
    }
    if (geometryChanged()) {
      reconfigureVisible();
    }
    if (bool(m_scroll.hidden) != m_view->isHidden()) {
      m_scroll.hidden = m_view->isHidden();
    }
  }

  void attach() {
    for (const auto& connection : m_connections) {
      disconnect(connection);
    }
    m_connections.clear();
    m_model = m_view != nullptr ? m_view->model() : nullptr;
    m_selection = m_view != nullptr ? m_view->selectionModel() : nullptr;
    if (QAbstractItemModel* model = m_model.data()) {
      const auto reshape = [this] { m_reloadTimer.start(0); };
      m_connections.push_back(connect(model, &QAbstractItemModel::modelReset, this, reshape));
      m_connections.push_back(connect(model, &QAbstractItemModel::layoutChanged, this, reshape));
      m_connections.push_back(connect(model, &QAbstractItemModel::rowsInserted, this, reshape));
      m_connections.push_back(connect(model, &QAbstractItemModel::rowsRemoved, this, reshape));
      m_connections.push_back(connect(model, &QAbstractItemModel::rowsMoved, this, reshape));
      m_connections.push_back(connect(model, &QAbstractItemModel::dataChanged, this,
                                      [this](const QModelIndex& top, const QModelIndex& bottom) { changed(top, bottom); }));
    }
    if (QItemSelectionModel* selection = m_selection.data()) {
      const auto selected = [this] { m_selectionTimer.start(0); };
      m_connections.push_back(connect(selection, &QItemSelectionModel::currentChanged, this, selected));
      m_connections.push_back(connect(selection, &QItemSelectionModel::selectionChanged, this, selected));
    }
  }

  void reload() {
    m_reloadTimer.stop();
    hideHover();
    if (m_view == nullptr) {
      return;
    }
    if (m_model != m_view->model() || m_selection != m_view->selectionModel()) {
      attach();
    }
    m_rows.clear();
    if (QAbstractItemModel* model = m_model.data()) {
      const QModelIndex root = m_view->rootIndex();
      auto* list = qobject_cast<QListView*>(m_view.data());
      for (int row = 0; row < model->rowCount(root); ++row) {
        if (list == nullptr || !list->isRowHidden(row)) {
          m_rows.push_back(row);
        }
      }
    }
    geometryChanged();
    [m_collection reloadData];
    syncSelection(true);
  }

  // The Qt view's icon and grid sizes (PartGrid's tile size): true when
  // they changed and the tiles were laid out again.
  bool geometryChanged() {
    if (m_view == nullptr) {
      return false;
    }
    QSize tile = m_view->iconSize();
    if (tile.width() <= 0 || tile.height() <= 0) {
      tile = QSize(64, 64);
    }
    auto* list = qobject_cast<QListView*>(m_view.data());
    QSize cell = list != nullptr ? list->gridSize() : QSize();
    if (cell.width() <= 0 || cell.height() <= 0) {
      cell = QSize(tile.width() + 24 + kGap, tile.height() + 50 + kGap);
    }
    if (tile == m_tile && cell == m_cell) {
      return false;
    }
    m_tile = tile;
    m_cell = cell;
    m_layout.itemSize = NSMakeSize(std::max(cell.width() - kGap, tile.width() + 8),
                                   std::max(cell.height() - kGap, tile.height() + 40));
    [m_layout invalidateLayout];
    return true;
  }

  void reconfigureVisible() {
    for (NSCollectionViewItem* shown in m_collection.visibleItems) {
      NSIndexPath* path = [m_collection indexPathForItem:shown];
      if (path != nil && [shown.view isKindOfClass:MitcadNativeTileView.class]) {
        configure((MitcadNativeTileView*)shown.view, path.item);
      }
    }
  }

  void changed(const QModelIndex& top, const QModelIndex& bottom) {
    if (m_reloadTimer.isActive()) {
      return;
    }
    for (NSCollectionViewItem* shown in m_collection.visibleItems) {
      NSIndexPath* path = [m_collection indexPathForItem:shown];
      if (path == nil || path.item < 0 || path.item >= count() || ![shown.view isKindOfClass:MitcadNativeTileView.class]) {
        continue;
      }
      const int row = m_rows[static_cast<std::size_t>(path.item)];
      if (row >= top.row() && row <= bottom.row()) {
        configure((MitcadNativeTileView*)shown.view, path.item);
      }
    }
    // A new tile size reaches the view's icon size after the model's change.
    m_sizeTimer.start(0);
  }

  QModelIndex indexOf(NSInteger tile) const {
    if (m_view == nullptr || m_model == nullptr || tile < 0 || tile >= count()) {
      return {};
    }
    auto* list = qobject_cast<QListView*>(m_view.data());
    const int column = list != nullptr ? list->modelColumn() : 0;
    return m_model->index(m_rows[static_cast<std::size_t>(tile)], column, m_view->rootIndex());
  }

  NSInteger tileOf(const QModelIndex& index) const {
    if (!index.isValid() || m_view == nullptr || index.parent() != m_view->rootIndex()) {
      return -1;
    }
    const auto found = std::find(m_rows.begin(), m_rows.end(), index.row());
    return found == m_rows.end() ? -1 : static_cast<NSInteger>(found - m_rows.begin());
  }

  NSInteger currentTile() const {
    if (m_view == nullptr || m_view->selectionModel() == nullptr) {
      return -1;
    }
    const QModelIndex current = m_view->selectionModel()->currentIndex();
    return m_view->selectionModel()->isSelected(current) ? tileOf(current) : -1;
  }

  NSRect frameOf(NSInteger tile) const {
    NSCollectionViewLayoutAttributes* attributes =
        [m_collection layoutAttributesForItemAtIndexPath:[NSIndexPath indexPathForItem:tile inSection:0]];
    return attributes != nil ? attributes.frame : NSZeroRect;
  }

  void configure(MitcadNativeTileView* view, NSInteger tile) {
    [m_tiles addObject:view];
    view->owner = this;
    view->tile = tile;
    view->side = m_tile.width();
    const QModelIndex index = indexOf(tile);
    if (!index.isValid() || m_view == nullptr) {
      view.hidden = YES;
      return;
    }
    view.hidden = NO;
    NSImage* image = imageFor(index.data(m_imageRole));
    view->picture.image = image;
    view->hasPicture = image != nil;
    const QString name = index.data(Qt::DisplayRole).toString();
    view->caption.stringValue = ns(name);
    const QString subtitle = m_subtitleRole >= 0 ? index.data(m_subtitleRole).toString() : QString();
    view->detail.stringValue = ns(subtitle);
    view->detail.hidden = subtitle.isEmpty();
    // A tile without a star role value (a library to get) has no star.
    const QVariant starred = m_starRole >= 0 ? index.data(m_starRole) : QVariant();
    view->starred = starred.toBool();
    view->star.hidden = !starred.isValid();
    showStar(view->star, starred.toBool());
    view->star.target = m_target;
    view->star.action = @selector(star:);
    view->star.tag = tile;
    const int uses = m_badgeRole >= 0 ? index.data(m_badgeRole).toInt() : 0;
    view->badge = uses > 0 ? ns(QString(QChar(0x00D7)) + QString::number(uses)) : nil;
    // A refused part is greyed, its reason in the details (PartGrid).
    const QString refused = m_disabledRole >= 0 ? index.data(m_disabledRole).toString() : QString();
    view.alphaValue = refused.isEmpty() ? 1.0 : 0.38;
    const QString key = index.data(m_keyRole).toString();
    view->key = ns(key);
    view.accessibilityIdentifier = ns(m_view->objectName() + QStringLiteral(".tile.") + key);
    view->star.accessibilityIdentifier = ns(m_view->objectName() + QStringLiteral(".star.") + key);
    view->star.toolTip = ns(QObject::tr("Star or turn the star off: favourites come first"));
    const QString details = index.data(Qt::ToolTipRole).toString();
    view.accessibilityLabel = ns(QStringList{name, subtitle}.join(QLatin1Char(' ')).trimmed());
    view.accessibilityHelp = ns(details);
    // Details show in the hover card; a grid without them keeps a tooltip.
    view.toolTip = details.isEmpty() ? ns(name) : nil;
    view->chosen = tile == currentTile();
    [view refreshCaption];
    [view placeSubviews];
    view.needsDisplay = YES;
  }

  NSImage* imageFor(const QVariant& value) {
    const QPixmap pixmap = pixmapOf(value, m_tile.width());
    if (pixmap.isNull()) {
      return nil;
    }
    NSNumber* key = @(pixmap.cacheKey());
    NSImage* image = m_images[key];
    if (image == nil) {
      if (m_images.count > 512) {
        [m_images removeAllObjects];
      }
      image = nativeImage(pixmap);
      if (image != nil) {
        m_images[key] = image;
      }
    }
    return image;
  }

  void syncSelection(bool scroll) {
    m_selectionTimer.stop();
    if (m_reloadTimer.isActive()) {
      return; // the reload selects
    }
    const NSInteger tile = currentTile();
    NSSet<NSIndexPath*>* paths =
        tile >= 0 ? [NSSet setWithObject:[NSIndexPath indexPathForItem:tile inSection:0]] : [NSSet set];
    if (![m_collection.selectionIndexPaths isEqualToSet:paths]) {
      m_collection.selectionIndexPaths = paths;
    }
    for (NSCollectionViewItem* shown in m_collection.visibleItems) {
      if (![shown.view isKindOfClass:MitcadNativeTileView.class]) {
        continue;
      }
      NSIndexPath* path = [m_collection indexPathForItem:shown];
      MitcadNativeTileView* view = (MitcadNativeTileView*)shown.view;
      const BOOL chosen = path != nil && path.item == tile;
      if (view->chosen != chosen) {
        view->chosen = chosen;
        [view refreshCaption];
        view.needsDisplay = YES;
      }
    }
    if (scroll && tile >= 0) {
      [m_collection layoutSubtreeIfNeeded];
      const NSRect frame = frameOf(tile);
      if (!NSIsEmptyRect(frame)) {
        [m_collection scrollRectToVisible:NSInsetRect(frame, 0, -8)];
      }
    }
  }

  // The tile current, as a press makes it in the Qt view.
  bool select(const QModelIndex& index) {
    if (m_view == nullptr || m_view->selectionModel() == nullptr || !index.isValid()) {
      return false;
    }
    QPointer<NativeTileGrid> alive(this);
    m_view->selectionModel()->setCurrentIndex(index, QItemSelectionModel::ClearAndSelect);
    if (alive == nullptr) {
      return false;
    }
    syncSelection(true);
    return true;
  }

  void choose(NSInteger tile, NSInteger clicks) {
    const QModelIndex index = indexOf(tile);
    if (!index.isValid()) {
      return;
    }
    const QPersistentModelIndex kept(index);
    QPointer<NativeTileGrid> alive(this);
    QPointer<QAbstractItemView> view(m_view.data());
    if (!select(index) || alive == nullptr || view == nullptr || !kept.isValid()) {
      return;
    }
    if (clicks >= 2) {
      activate(kept);
      return;
    }
    // What the Qt view's release emits (the part picker chooses the part).
    QMetaObject::invokeMethod(view.data(), "clicked", Qt::DirectConnection, Q_ARG(QModelIndex, QModelIndex(kept)));
  }

  // A double click or Return: the Qt view's doubleClicked and activated.
  void activate(const QModelIndex& index) {
    QPointer<QAbstractItemView> view(m_view.data());
    if (view == nullptr || !index.isValid()) {
      return;
    }
    const QPersistentModelIndex kept(index);
    QMetaObject::invokeMethod(view.data(), "doubleClicked", Qt::DirectConnection, Q_ARG(QModelIndex, index));
    if (view != nullptr && kept.isValid()) {
      QMetaObject::invokeMethod(view.data(), "activated", Qt::DirectConnection, Q_ARG(QModelIndex, QModelIndex(kept)));
    }
  }

  bool activateCurrent() {
    const NSInteger tile = currentTile();
    if (tile < 0) {
      return false;
    }
    activate(indexOf(tile));
    return true;
  }

  void stepTo(NSInteger tile) {
    if (tile >= 0 && tile < count()) {
      select(indexOf(tile));
    }
  }

  // Left and Right step in order; Up and Down to the nearest tile of the
  // row above or below, as QListView's icon mode does.
  void step(int dx, int dy) {
    const NSInteger total = count();
    if (total == 0) {
      return;
    }
    const NSInteger current = currentTile();
    if (current < 0) {
      stepTo(0);
      return;
    }
    if (dx != 0) {
      stepTo(std::clamp<NSInteger>(current + dx, 0, total - 1));
      return;
    }
    const NSRect from = frameOf(current);
    NSInteger best = -1;
    CGFloat bestScore = std::numeric_limits<CGFloat>::max();
    for (NSInteger tile = 0; tile < total; ++tile) {
      if (dy > 0 ? tile <= current : tile >= current) {
        continue;
      }
      const NSRect frame = frameOf(tile);
      const CGFloat rows = std::fabs(NSMidY(frame) - NSMidY(from));
      if (rows < 1) {
        continue; // the same row
      }
      const CGFloat score = rows * 10000 + std::fabs(NSMidX(frame) - NSMidX(from));
      if (score < bestScore) {
        bestScore = score;
        best = tile;
      }
    }
    if (best >= 0) {
      stepTo(best);
    }
  }

  bool ownsFirstResponder() {
    NSResponder* responder = m_collection.window.firstResponder;
    NSView* root = m_focusRoot;
    if (root == nil) {
      root = m_scroll;
    }
    if ([responder isKindOfClass:NSTextView.class] && ((NSTextView*)responder).fieldEditor) {
      id delegate = ((NSTextView*)responder).delegate;
      return [delegate isKindOfClass:NSView.class] && [(NSView*)delegate isDescendantOf:root];
    }
    return [responder isKindOfClass:NSView.class] && [(NSView*)responder isDescendantOf:root];
  }

  bool triggerShortcut(const QKeySequence& sequence) {
    for (QWidget* widget = m_view.data(); widget != nullptr; widget = widget->parentWidget()) {
      const QList<QAction*> actions = widget->actions();
      for (QAction* action : actions) {
        const Qt::ShortcutContext context = action->shortcutContext();
        if (!action->isEnabled() || context == Qt::ApplicationShortcut ||
            (context == Qt::WidgetShortcut && widget != m_view.data()) || !action->shortcuts().contains(sequence)) {
          continue;
        }
        action->trigger();
        return true;
      }
      if (widget->isWindow()) {
        break;
      }
    }
    return false;
  }

  void showHover() {
    NSWindow* window = m_collection.window;
    const QModelIndex index = indexOf(m_hoverTile);
    if (window == nil || !window.visible || !index.isValid()) {
      return;
    }
    const QString text = index.data(Qt::ToolTipRole).toString();
    if (text.isEmpty()) {
      return;
    }
    NSCollectionViewItem* shown = [m_collection itemAtIndexPath:[NSIndexPath indexPathForItem:m_hoverTile inSection:0]];
    if (shown == nil || shown.view.window == nil) {
      return;
    }
    NSImage* image = nil;
    if (m_previewRole >= 0) {
      QImage full = index.data(m_previewRole).value<QImage>();
      if (!full.isNull()) {
        if (full.width() > kHoverPicture || full.height() > kHoverPicture) {
          full = full.scaled(kHoverPicture, kHoverPicture, Qt::KeepAspectRatio, Qt::SmoothTransformation);
        }
        image = nativeImage(QPixmap::fromImage(full));
      }
    }
    if (m_hoverCard == nil) {
      m_hoverCard = hoverPanel();
    }
    fillHoverPanel(m_hoverCard, image, ns(text));
    const NSRect anchor = [shown.view.window convertRectToScreen:[shown.view convertRect:shown.view.bounds toView:nil]];
    NSScreen* screen = shown.view.window.screen != nil ? shown.view.window.screen : NSScreen.mainScreen;
    const NSRect visible = screen != nil ? screen.visibleFrame : NSZeroRect;
    NSRect frame = m_hoverCard.frame;
    frame.origin.x = NSMaxX(anchor) + 6;
    frame.origin.y = NSMaxY(anchor) - NSHeight(frame);
    if (!NSIsEmptyRect(visible)) {
      if (NSMaxX(frame) > NSMaxX(visible)) {
        frame.origin.x = NSMinX(anchor) - 6 - NSWidth(frame);
      }
      frame.origin.x = std::max(frame.origin.x, NSMinX(visible));
      frame.origin.y = std::max(frame.origin.y, NSMinY(visible));
    }
    [m_hoverCard setFrame:frame display:YES];
    [m_hoverCard orderFront:nil];
  }

  QPointer<QAbstractItemView> m_view;
  QPointer<QAbstractItemModel> m_model;
  QPointer<QItemSelectionModel> m_selection;
  std::vector<QMetaObject::Connection> m_connections;
  std::vector<int> m_rows; // the model rows shown, in order
  QSize m_tile;
  QSize m_cell;
  int m_keyRole = Qt::DisplayRole;
  int m_imageRole = Qt::DecorationRole;
  int m_previewRole = -1;
  int m_subtitleRole = -1;
  int m_starRole = -1;
  int m_badgeRole = -1;
  int m_disabledRole = -1;
  NSInteger m_hoverTile = -1;
  MitcadNativeTileTarget* m_target = nil;
  NSCollectionViewFlowLayout* m_layout = nil;
  MitcadNativeTileCollection* m_collection = nil;
  NSScrollView* m_scroll = nil;
  __weak NSView* m_focusRoot = nil;
  NSPanel* m_hoverCard = nil;
  NSMutableDictionary<NSNumber*, NSImage*>* m_images = [NSMutableDictionary dictionary];
  NSHashTable<MitcadNativeTileView*>* m_tiles = [NSHashTable weakObjectsHashTable];
  QTimer m_reloadTimer;
  QTimer m_selectionTimer;
  QTimer m_sizeTimer;
  QTimer m_hoverTimer;
  QTimer m_checkTimer;
};

// ---------------------------------------------------------------------------
// A tree of groups as a source list with stars and folding

class NativeGroupList final : public QObject, public NativeWidgetView {
public:
  explicit NativeGroupList(QTreeView* tree) : m_tree(tree) {
    m_keyRole = roleOf(tree, "nativeGroupKeyRole", Qt::DisplayRole);
    m_starRole = roleOf(tree, "nativeStarRole", -1);
    m_dimmedRole = roleOf(tree, "nativeGroupDimmedRole", -1);
    m_starMethod = tree->property("nativeStarMethod").toByteArray();
    m_target = [MitcadNativeGroupTarget new];
    m_target->owner = this;
    m_outline = [[MitcadNativeGroupOutline alloc] initWithFrame:NSMakeRect(0, 0, 220, 300)];
    m_outline->owner = this;
    NSTableColumn* column = [[NSTableColumn alloc] initWithIdentifier:@"group"];
    column.resizingMask = NSTableColumnAutoresizingMask;
    column.width = 200;
    [m_outline addTableColumn:column];
    m_outline.outlineTableColumn = column;
    m_outline.headerView = nil;
    m_outline.style = NSTableViewStyleSourceList;
    m_outline.backgroundColor = NSColor.clearColor;
    m_outline.rowSizeStyle = NSTableViewRowSizeStyleCustom;
    m_outline.rowHeight = 24;
    m_outline.indentationPerLevel = 14;
    m_outline.floatsGroupRows = NO;
    m_outline.allowsEmptySelection = YES;
    m_outline.allowsMultipleSelection = NO;
    m_outline.columnAutoresizingStyle = NSTableViewFirstColumnOnlyAutoresizingStyle;
    m_outline.autoresizesOutlineColumn = NO;
    m_outline.dataSource = m_target;
    m_outline.delegate = m_target;
    m_outline.accessibilityIdentifier = ns(tree->objectName());
    m_scroll = [[NSScrollView alloc] initWithFrame:NSMakeRect(0, 0, 220, 300)];
    m_scroll.documentView = m_outline;
    m_scroll.hasVerticalScroller = YES;
    m_scroll.hasHorizontalScroller = NO;
    m_scroll.autohidesScrollers = YES;
    m_scroll.borderType = NSNoBorder;
    m_scroll.drawsBackground = NO;
    m_scroll.translatesAutoresizingMaskIntoConstraints = NO;
    // Readable group names: the Qt tree's width range, a little wider by
    // preference (mitcad#813: never an icon-width strip).
    const int least = std::max(150, tree->minimumWidth());
    const int most = std::max(least, tree->maximumWidth() < QWIDGETSIZE_MAX ? tree->maximumWidth() : 280);
    [m_scroll.widthAnchor constraintGreaterThanOrEqualToConstant:least].active = YES;
    [m_scroll.widthAnchor constraintLessThanOrEqualToConstant:most].active = YES;
    NSLayoutConstraint* preferred = [m_scroll.widthAnchor constraintEqualToConstant:std::clamp(220, least, most)];
    preferred.priority = NSLayoutPriorityDefaultLow + 2;
    preferred.active = YES;
    [m_scroll.heightAnchor constraintGreaterThanOrEqualToConstant:120].active = YES;
    for (QTimer* timer : {&m_reloadTimer, &m_cellsTimer, &m_selectionTimer}) {
      timer->setSingleShot(true);
    }
    connect(&m_reloadTimer, &QTimer::timeout, this, [this] { reload(); });
    connect(&m_cellsTimer, &QTimer::timeout, this, [this] { refreshCells(); });
    connect(&m_selectionTimer, &QTimer::timeout, this, [this] { syncSelection(); });
    const auto folded = [this] {
      if (!m_syncing) {
        m_reloadTimer.start(0);
      }
    };
    connect(tree, &QTreeView::expanded, this, folded);
    connect(tree, &QTreeView::collapsed, this, folded);
    reload();
  }

  ~NativeGroupList() override {
    m_reloadTimer.stop();
    m_cellsTimer.stop();
    m_selectionTimer.stop();
    for (const auto& connection : m_connections) {
      disconnect(connection);
    }
    m_target->owner = nullptr;
    m_outline->owner = nullptr;
    m_outline.dataSource = nil;
    m_outline.delegate = nil;
  }

  void* nativeView() const override { return (__bridge void*)m_scroll; }
  void refresh() override {
    if (m_tree != nullptr && (m_model != m_tree->model() || m_selection != m_tree->selectionModel())) {
      reload();
    }
  }
  NSScrollView* scrollView() const { return m_scroll; }

  NSMutableArray<MitcadNativeGroupNode*>* childrenOf(id item) const {
    if (item == nil) {
      return m_nodes;
    }
    MitcadNativeGroupNode* node = item;
    return node->children;
  }

  NSView* cell(id item) {
    MitcadNativeGroupNode* node = item;
    if (node == nil || !node->index.isValid() || m_tree == nullptr) {
      return nil;
    }
    const QModelIndex index(node->index);
    NSTableCellView* view = [[NSTableCellView alloc] initWithFrame:NSMakeRect(0, 0, 200, 24)];
    const QString key = index.data(m_keyRole).toString();
    NSTextField* text = [NSTextField labelWithString:ns(index.data(Qt::DisplayRole).toString())];
    text.lineBreakMode = NSLineBreakByTruncatingTail;
    text.maximumNumberOfLines = 1;
    text.translatesAutoresizingMaskIntoConstraints = NO;
    [text setContentCompressionResistancePriority:NSLayoutPriorityDefaultLow
                                  forOrientation:NSLayoutConstraintOrientationHorizontal];
    // A group with no part under the filters, kept as the chosen one:
    // greyed (PartGrid, mitcad#1015).
    if (m_dimmedRole >= 0 && index.data(m_dimmedRole).toBool()) {
      text.textColor = NSColor.secondaryLabelColor;
    }
    [view addSubview:text];
    view.textField = text;
    NSView* mark = nil;
    // A library's group has its star (PartGrid's group star); the others
    // keep their own artwork (Favourites' star).
    const QVariant starred = m_starRole >= 0 ? index.data(m_starRole) : QVariant();
    if (starred.isValid()) {
      NSButton* button = starButton();
      showStar(button, starred.toBool());
      button.target = m_target;
      button.action = @selector(star:);
      button.accessibilityIdentifier = ns(m_tree->objectName() + QStringLiteral(".groupStar.") + key);
      button.toolTip = ns(m_tree->toolTip());
      mark = button;
    } else {
      const QPixmap pixmap = pixmapOf(index.data(Qt::DecorationRole), 16);
      NSImage* image = nativeImage(pixmap);
      if (image != nil) {
        NSImageView* icon = [NSImageView imageViewWithImage:image];
        view.imageView = icon;
        mark = icon;
      }
    }
    if (mark != nil) {
      mark.translatesAutoresizingMaskIntoConstraints = NO;
      [view addSubview:mark];
      [NSLayoutConstraint activateConstraints:@[
        [mark.leadingAnchor constraintEqualToAnchor:view.leadingAnchor constant:2],
        [mark.centerYAnchor constraintEqualToAnchor:view.centerYAnchor],
        [mark.widthAnchor constraintEqualToConstant:16],
        [mark.heightAnchor constraintEqualToConstant:16]
      ]];
    }
    [NSLayoutConstraint activateConstraints:@[
      [text.leadingAnchor constraintEqualToAnchor:view.leadingAnchor constant:24],
      [text.trailingAnchor constraintLessThanOrEqualToAnchor:view.trailingAnchor constant:-2],
      [text.centerYAnchor constraintEqualToAnchor:view.centerYAnchor]
    ]];
    view.accessibilityIdentifier = ns(m_tree->objectName() + QStringLiteral(".group.") + key);
    return view;
  }

  // A group chosen in the list: the tree's current item, as a click on it.
  void selected() {
    if (m_syncing || m_tree == nullptr) {
      return;
    }
    const NSInteger row = m_outline.selectedRow;
    MitcadNativeGroupNode* node = row >= 0 ? [m_outline itemAtRow:row] : nil;
    if (node == nil || !node->index.isValid()) {
      return;
    }
    if (m_tree->currentIndex() != QModelIndex(node->index)) {
      m_tree->setCurrentIndex(node->index);
    }
  }

  // Folded or unfolded with its arrow: the tree's item too (PartGrid keeps
  // the folding in its settings).
  void folded(id item, bool collapsed) {
    MitcadNativeGroupNode* node = item;
    if (m_syncing || m_tree == nullptr || node == nil || !node->index.isValid()) {
      return;
    }
    m_syncing = true;
    m_tree->setExpanded(node->index, !collapsed);
    m_syncing = false;
  }

  // A group's star: the group current, as a press on the Qt star makes it,
  // then the presenter's star method (nativeStarMethod).
  void star(id sender) {
    if (m_tree == nullptr || ![sender isKindOfClass:NSView.class]) {
      return;
    }
    const NSInteger row = [m_outline rowForView:(NSView*)sender];
    MitcadNativeGroupNode* node = row >= 0 ? [m_outline itemAtRow:row] : nil;
    if (node == nil || !node->index.isValid()) {
      return;
    }
    QPointer<QTreeView> tree(m_tree.data());
    QPointer<NativeGroupList> alive(this);
    const QPersistentModelIndex kept(node->index);
    if (tree->currentIndex() != QModelIndex(kept)) {
      tree->setCurrentIndex(kept);
    }
    if (tree != nullptr && alive != nullptr) {
      invokeOnAncestor(tree.data(), m_starMethod);
    }
  }

private:
  void attach() {
    for (const auto& connection : m_connections) {
      disconnect(connection);
    }
    m_connections.clear();
    m_model = m_tree != nullptr ? m_tree->model() : nullptr;
    m_selection = m_tree != nullptr ? m_tree->selectionModel() : nullptr;
    if (QAbstractItemModel* model = m_model.data()) {
      const auto reshape = [this] { m_reloadTimer.start(0); };
      m_connections.push_back(connect(model, &QAbstractItemModel::modelReset, this, reshape));
      m_connections.push_back(connect(model, &QAbstractItemModel::layoutChanged, this, reshape));
      m_connections.push_back(connect(model, &QAbstractItemModel::rowsInserted, this, reshape));
      m_connections.push_back(connect(model, &QAbstractItemModel::rowsRemoved, this, reshape));
      m_connections.push_back(connect(model, &QAbstractItemModel::rowsMoved, this, reshape));
      // Stars and names change in place (PartGrid::refreshStars).
      m_connections.push_back(connect(model, &QAbstractItemModel::dataChanged, this, [this] { m_cellsTimer.start(0); }));
    }
    if (QItemSelectionModel* selection = m_selection.data()) {
      m_connections.push_back(
          connect(selection, &QItemSelectionModel::currentChanged, this, [this] { m_selectionTimer.start(0); }));
    }
  }

  NSMutableArray<MitcadNativeGroupNode*>* nodesOf(const QModelIndex& parent) {
    NSMutableArray<MitcadNativeGroupNode*>* nodes = [NSMutableArray array];
    QAbstractItemModel* model = m_model.data();
    if (model == nullptr || m_tree == nullptr) {
      return nodes;
    }
    for (int row = 0; row < model->rowCount(parent); ++row) {
      if (m_tree->isRowHidden(row, parent)) {
        continue;
      }
      MitcadNativeGroupNode* node = [MitcadNativeGroupNode new];
      node->index = model->index(row, 0, parent);
      node->children = nodesOf(model->index(row, 0, parent));
      [nodes addObject:node];
    }
    return nodes;
  }

  void expand(NSArray<MitcadNativeGroupNode*>* nodes) {
    for (MitcadNativeGroupNode* node in nodes) {
      if (node->children.count == 0) {
        continue;
      }
      if (m_tree->isExpanded(node->index)) {
        [m_outline expandItem:node];
        expand(node->children);
      } else {
        [m_outline collapseItem:node];
      }
    }
  }

  void reload() {
    m_reloadTimer.stop();
    m_cellsTimer.stop();
    if (m_tree == nullptr) {
      return;
    }
    if (m_model != m_tree->model() || m_selection != m_tree->selectionModel()) {
      attach();
    }
    m_nodes = nodesOf(m_tree->rootIndex());
    m_syncing = true;
    [m_outline reloadData];
    expand(m_nodes);
    m_syncing = false;
    syncSelection();
  }

  void refreshCells() {
    m_cellsTimer.stop();
    const NSInteger rows = m_outline.numberOfRows;
    if (rows > 0) {
      [m_outline reloadDataForRowIndexes:[NSIndexSet indexSetWithIndexesInRange:NSMakeRange(0, static_cast<NSUInteger>(rows))]
                           columnIndexes:[NSIndexSet indexSetWithIndex:0]];
    }
  }

  MitcadNativeGroupNode* find(NSArray<MitcadNativeGroupNode*>* nodes, const QModelIndex& index) const {
    if (!index.isValid()) {
      return nil;
    }
    for (MitcadNativeGroupNode* node in nodes) {
      if (node->index == index) {
        return node;
      }
      if (MitcadNativeGroupNode* found = find(node->children, index)) {
        return found;
      }
    }
    return nil;
  }

  void syncSelection() {
    m_selectionTimer.stop();
    if (m_tree == nullptr || m_reloadTimer.isActive()) {
      return;
    }
    MitcadNativeGroupNode* node = find(m_nodes, m_tree->currentIndex().siblingAtColumn(0));
    const NSInteger row = node != nil ? [m_outline rowForItem:node] : -1;
    m_syncing = true;
    if (row >= 0) {
      if (m_outline.selectedRow != row) {
        [m_outline selectRowIndexes:[NSIndexSet indexSetWithIndex:static_cast<NSUInteger>(row)] byExtendingSelection:NO];
      }
      [m_outline scrollRowToVisible:row];
    } else if (m_outline.selectedRow >= 0) {
      [m_outline deselectAll:nil];
    }
    m_syncing = false;
  }

  QPointer<QTreeView> m_tree;
  QPointer<QAbstractItemModel> m_model;
  QPointer<QItemSelectionModel> m_selection;
  std::vector<QMetaObject::Connection> m_connections;
  int m_keyRole = Qt::DisplayRole;
  int m_starRole = -1;
  int m_dimmedRole = -1;
  QByteArray m_starMethod;
  bool m_syncing = false;
  MitcadNativeGroupTarget* m_target = nil;
  MitcadNativeGroupOutline* m_outline = nil;
  NSScrollView* m_scroll = nil;
  NSMutableArray<MitcadNativeGroupNode*>* m_nodes = [NSMutableArray array];
  QTimer m_reloadTimer;
  QTimer m_cellsTimer;
  QTimer m_selectionTimer;
};

// ---------------------------------------------------------------------------
// A tile catalogue: groups, rows of filters and the grid

class NativeCatalogue final : public QObject, public NativeWidgetView {
public:
  NativeCatalogue(QWidget* widget, QTreeView* groups, QAbstractItemView* tiles, const NativeWidgetRenderer& render)
      : m_grid(std::make_unique<NativeTileGrid>(tiles)), m_groups(std::make_unique<NativeGroupList>(groups)) {
    m_root = [[MitcadNativeCatalogueView alloc] initWithFrame:NSMakeRect(0, 0, 700, 420)];
    m_root.translatesAutoresizingMaskIntoConstraints = NO;
    NSView* list = m_groups->scrollView();
    NSView* grid = m_grid->scrollView();
    NSView* column = [[NSView alloc] initWithFrame:NSZeroRect];
    column.translatesAutoresizingMaskIntoConstraints = NO;
    NSStackView* rows = stackOf(false);
    // The rows of the tiles' column, in their order: the search with its
    // filters, the system, Star and the tile size (PartGrid's layout).
    if (QLayout* top = widget->layout()) {
      for (int i = 0; i < top->count(); ++i) {
        QLayoutItem* item = top->itemAt(i);
        QLayout* content = holds(item, tiles) ? item->layout() : nullptr;
        if (content == nullptr) {
          continue;
        }
        for (int j = 0; j < content->count(); ++j) {
          QLayoutItem* child = content->itemAt(j);
          if (holds(child, tiles)) {
            continue;
          }
          if (NSView* line = row(child, render)) {
            addFullWidth(rows, line);
          }
        }
      }
    }
    [column addSubview:rows];
    [column addSubview:grid];
    [m_root addSubview:list];
    [m_root addSubview:column];
    [NSLayoutConstraint activateConstraints:@[
      [list.leadingAnchor constraintEqualToAnchor:m_root.leadingAnchor],
      [list.topAnchor constraintEqualToAnchor:m_root.topAnchor],
      [list.bottomAnchor constraintEqualToAnchor:m_root.bottomAnchor],
      [column.leadingAnchor constraintEqualToAnchor:list.trailingAnchor constant:12],
      [column.trailingAnchor constraintEqualToAnchor:m_root.trailingAnchor],
      [column.topAnchor constraintEqualToAnchor:m_root.topAnchor],
      [column.bottomAnchor constraintEqualToAnchor:m_root.bottomAnchor],
      [rows.leadingAnchor constraintEqualToAnchor:column.leadingAnchor],
      [rows.trailingAnchor constraintEqualToAnchor:column.trailingAnchor],
      [rows.topAnchor constraintEqualToAnchor:column.topAnchor],
      [grid.leadingAnchor constraintEqualToAnchor:column.leadingAnchor],
      [grid.trailingAnchor constraintEqualToAnchor:column.trailingAnchor],
      [grid.topAnchor constraintEqualToAnchor:rows.bottomAnchor constant:8],
      [grid.bottomAnchor constraintEqualToAnchor:column.bottomAnchor],
      [m_root.heightAnchor constraintGreaterThanOrEqualToConstant:280]
    ]];
    // No height preference of its own: the shared layout gives the
    // catalogue its stretched share of the window, and the grid scrolls
    // within it rather than growing the dialog's scrolled body.
    m_root->search = m_search;
    m_grid->setFocusRoot(m_root);
  }

  ~NativeCatalogue() override {
    if (m_searchTarget != nil) {
      m_searchTarget->owner = nullptr;
    }
    if (m_search != nil) {
      m_search.delegate = nil;
      m_search.target = nil;
    }
    disconnect(m_searchConnection);
  }

  void* nativeView() const override { return (__bridge void*)m_root; }
  void refresh() override {
    m_grid->refresh();
    m_groups->refresh();
    if (m_search != nil && m_edit != nullptr) {
      if (m_search.currentEditor == nil && qt(m_search.stringValue) != m_edit->text()) {
        m_search.stringValue = ns(m_edit->text());
      }
      m_search.enabled = m_edit->isEnabled();
    }
  }

  // The native search's text into the Qt field, as typing there does.
  void searchEdited() {
    if (m_edit == nullptr || m_search == nil) {
      return;
    }
    const QString text = qt(m_search.stringValue);
    if (m_edit->text() == text) {
      return;
    }
    QPointer<QLineEdit> edit(m_edit.data());
    edit->setText(text);
    if (edit == nullptr) {
      return;
    }
    edit->setModified(true);
    QMetaObject::invokeMethod(edit.data(), "textEdited", Qt::DirectConnection, Q_ARG(QString, text));
  }

  // Up and Down in the search move through the tiles (PartGrid). Escape
  // in the modeless picker closes its panel, as in the shared QDialog;
  // ordinary catalogue searches retain NSSearchField's clear action.
  bool searchCommand(SEL command) {
    if (command == @selector(cancelOperation:) && [m_search.window isKindOfClass:MitcadNativeUtilityPanel.class]) {
      [m_search.window cancelOperation:m_search];
      return true;
    }
    if (command == @selector(moveUp:)) {
      m_grid->stepLinear(-1);
      return true;
    }
    if (command == @selector(moveDown:)) {
      m_grid->stepLinear(1);
      return true;
    }
    return false;
  }

private:
  NSView* row(QLayoutItem* item, const NativeWidgetRenderer& render) {
    if (QWidget* widget = item->widget()) {
      return control(widget, 0, render);
    }
    auto* box = qobject_cast<QBoxLayout*>(item->layout());
    if (box == nullptr) {
      return nil;
    }
    const bool horizontal = box->direction() == QBoxLayout::LeftToRight || box->direction() == QBoxLayout::RightToLeft;
    NSStackView* line = stackOf(horizontal);
    // Its expanding control (the search) takes the spare width.
    [line setHuggingPriority:kRowHugging
              forOrientation:horizontal ? NSLayoutConstraintOrientationHorizontal : NSLayoutConstraintOrientationVertical];
    // Before a stretch at the leading edge, after it at the trailing one.
    NSStackViewGravity gravity = horizontal ? NSStackViewGravityLeading : NSStackViewGravityTop;
    for (int i = 0; i < box->count(); ++i) {
      QLayoutItem* child = box->itemAt(i);
      if (QSpacerItem* spacer = child->spacerItem()) {
        const QSizePolicy::Policy policy =
            horizontal ? spacer->sizePolicy().horizontalPolicy() : spacer->sizePolicy().verticalPolicy();
        if ((int(policy) & int(QSizePolicy::ExpandFlag)) != 0) {
          gravity = horizontal ? NSStackViewGravityTrailing : NSStackViewGravityBottom;
        }
        continue;
      }
      NSView* view = child->widget() != nullptr ? control(child->widget(), box->stretch(i), render) : row(child, render);
      if (view != nil) {
        [line addView:view inGravity:gravity];
      }
    }
    return line;
  }

  NSView* control(QWidget* widget, int stretch, const NativeWidgetRenderer& render) {
    auto* edit = qobject_cast<QLineEdit*>(widget);
    if (edit != nullptr && edit->property("macSearchField").toBool() && m_search == nil) {
      NSView* field = searchField(edit);
      expanding(field, 160);
      return field;
    }
    NSView* view = render(widget);
    if (view == nil) {
      return nil;
    }
    view.translatesAutoresizingMaskIntoConstraints = NO;
    if (qobject_cast<QLabel*>(widget) != nullptr) {
      singleLine(view);
    }
    const int least = qobject_cast<QSlider*>(widget) != nullptr ? std::max(100, widget->minimumWidth()) : widget->minimumWidth();
    if (least > 0) {
      [view.widthAnchor constraintGreaterThanOrEqualToConstant:least].active = YES;
    }
    if (widget->maximumWidth() < QWIDGETSIZE_MAX) {
      [view.widthAnchor constraintLessThanOrEqualToConstant:std::max(least, widget->maximumWidth())].active = YES;
    }
    if (stretch > 0) {
      expanding(view, std::max(least, 120));
    }
    return view;
  }

  NSView* searchField(QLineEdit* edit) {
    m_edit = edit;
    m_search = [[NSSearchField alloc] initWithFrame:NSZeroRect];
    m_search.stringValue = ns(edit->text());
    m_search.placeholderString = ns(edit->placeholderText());
    m_search.toolTip = ns(edit->toolTip());
    m_search.accessibilityIdentifier = ns(edit->objectName());
    m_search.sendsSearchStringImmediately = YES;
    m_search.enabled = edit->isEnabled();
    m_search.translatesAutoresizingMaskIntoConstraints = NO;
    m_searchTarget = [MitcadNativeSearchTarget new];
    m_searchTarget->owner = this;
    m_search.delegate = m_searchTarget;
    m_search.target = m_searchTarget;
    m_search.action = @selector(search:);
    // The Qt field changed by its presenter (a part revealed clears it).
    m_searchConnection = connect(edit, &QLineEdit::textChanged, this, [this](const QString& text) {
      if (m_search != nil && m_search.currentEditor == nil && qt(m_search.stringValue) != text) {
        m_search.stringValue = ns(text);
      }
    });
    return m_search;
  }

  std::unique_ptr<NativeTileGrid> m_grid;
  std::unique_ptr<NativeGroupList> m_groups;
  QPointer<QLineEdit> m_edit;
  NSSearchField* m_search = nil;
  MitcadNativeSearchTarget* m_searchTarget = nil;
  MitcadNativeCatalogueView* m_root = nil;
  QMetaObject::Connection m_searchConnection;
};

// ---------------------------------------------------------------------------

std::unique_ptr<NativeWidgetView> createNativeTileGrid(QAbstractItemView* view) {
  auto* list = qobject_cast<QListView*>(view);
  if (list == nullptr || list->viewMode() != QListView::IconMode) {
    return {};
  }
  return std::make_unique<NativeTileGrid>(view);
}

NSView* nativeTileCatalogue(QWidget* widget, const NativeWidgetRenderer& render,
                            std::vector<std::unique_ptr<NativeWidgetView>>& owned) {
  if (widget == nullptr || widget->layout() == nullptr || !widget->property("nativeTileCatalogue").toBool()) {
    return nil;
  }
  QTreeView* groups = widget->findChild<QTreeView*>();
  QListView* tiles = nullptr;
  for (QListView* list : widget->findChildren<QListView*>()) {
    if (list->viewMode() == QListView::IconMode) {
      tiles = list;
      break;
    }
  }
  if (groups == nullptr || tiles == nullptr) {
    return nil;
  }
  auto catalogue = std::make_unique<NativeCatalogue>(widget, groups, tiles, render);
  NSView* view = (__bridge NSView*)catalogue->nativeView();
  owned.emplace_back(std::move(catalogue));
  return view;
}

NSView* nativeSplitView(QSplitter* splitter, const NativeWidgetRenderer& render) {
  const bool horizontal = splitter->orientation() == Qt::Horizontal;
  MitcadNativeSplitView* split = [[MitcadNativeSplitView alloc] initWithFrame:NSMakeRect(0, 0, 600, 400)];
  split.translatesAutoresizingMaskIntoConstraints = NO;
  split->horizontal = horizontal;
  split->panes = [NSMutableArray array];
  split->held = [NSMutableArray array];
  const QList<int> given = splitter->sizes();
  std::vector<int> stretches;
  std::vector<int> sizes;
  for (int i = 0; i < splitter->count(); ++i) {
    QWidget* widget = splitter->widget(i);
    NSView* pane = widget != nullptr ? render(widget) : nil;
    if (pane == nil) {
      continue;
    }
    pane.translatesAutoresizingMaskIntoConstraints = NO;
    NSView* previous = split->panes.lastObject;
    if (previous != nil) {
      MitcadNativeSplitDivider* divider = [[MitcadNativeSplitDivider alloc] initWithFrame:NSZeroRect];
      divider->split = split;
      divider->index = static_cast<NSInteger>(split->panes.count) - 1;
      divider.translatesAutoresizingMaskIntoConstraints = NO;
      divider.accessibilityRole = NSAccessibilitySplitterRole;
      [split addSubview:divider];
      along(divider, false, previous, true, horizontal, 0).active = YES;
      [lengthOf(divider, horizontal) constraintEqualToConstant:kDivider].active = YES;
      pinAcross(divider, split, horizontal);
      [split->held addObject:NSNull.null];
      [split addSubview:pane];
      along(pane, false, divider, true, horizontal, 0).active = YES;
    } else {
      [split addSubview:pane];
      along(pane, false, split, false, horizontal, 0).active = YES;
    }
    pinAcross(pane, split, horizontal);
    // The pane's Qt minimum, or the room its own layout needs (a details
    // column's wrapped text), up to a bound that keeps narrow windows usable.
    const QSize least = widget->minimumSize();
    const QSize hint = widget->minimumSizeHint();
    const int minimum = std::max(horizontal ? least.width() : least.height(),
                                 std::min(horizontal ? hint.width() : hint.height(), 420));
    if (minimum > 0) {
      NSLayoutConstraint* lowest = [lengthOf(pane, horizontal) constraintGreaterThanOrEqualToConstant:minimum];
      lowest.priority = kPaneMinimum;
      lowest.active = YES;
    }
    [split->panes addObject:pane];
    const QSizePolicy policy = widget->sizePolicy();
    stretches.push_back(horizontal ? policy.horizontalStretch() : policy.verticalStretch());
    sizes.push_back(i < given.size() ? given.at(i) : 0);
  }
  NSView* last = split->panes.lastObject;
  if (last == nil) {
    return split;
  }
  along(last, true, split, true, horizontal, 0).active = YES;
  // Qt's proportions: panes with a stretch factor share space by it, the
  // others keep their given size; without stretch, the given sizes.
  int stretchSum = 0;
  int sizeSum = 0;
  for (std::size_t i = 0; i < stretches.size(); ++i) {
    stretchSum += stretches[i];
    sizeSum += sizes[i];
  }
  NSView* reference = nil;
  int referenceWeight = 0;
  for (std::size_t i = 0; i < stretches.size(); ++i) {
    NSView* pane = split->panes[i];
    if (stretchSum > 0 && stretches[i] == 0) {
      if (sizes[i] > 0) {
        NSLayoutConstraint* keep = [lengthOf(pane, horizontal) constraintEqualToConstant:sizes[i]];
        keep.priority = kSplitKeep;
        keep.active = YES;
      }
      continue;
    }
    const int weight = stretchSum > 0 ? stretches[i] : sizeSum > 0 ? sizes[i] : 1;
    if (weight <= 0) {
      continue;
    }
    if (reference == nil) {
      reference = pane;
      referenceWeight = weight;
      continue;
    }
    NSLayoutConstraint* ratio = [lengthOf(pane, horizontal) constraintEqualToAnchor:lengthOf(reference, horizontal)
                                                                         multiplier:CGFloat(weight) / CGFloat(referenceWeight)];
    ratio.priority = kSplitRatio;
    ratio.active = YES;
  }
  // Its height is its stretched share of the shared layout (mitcad#812).
  [split.heightAnchor constraintGreaterThanOrEqualToConstant:160].active = YES;
  return split;
}

NSWindow* nativeUtilityPanel(NSRect content) {
  MitcadNativeUtilityPanel* panel = [[MitcadNativeUtilityPanel alloc]
      initWithContentRect:content
                styleMask:NSWindowStyleMaskTitled | NSWindowStyleMaskClosable | NSWindowStyleMaskResizable |
                          NSWindowStyleMaskUtilityWindow
                  backing:NSBackingStoreBuffered
                    defer:NO];
  // Above the document; the keyboard stays there until a field (the
  // search) needs it, as the Qt picker shows without activating.
  panel.floatingPanel = YES;
  panel.becomesKeyOnlyIfNeeded = YES;
  panel.hidesOnDeactivate = NO;
  panel.worksWhenModal = NO;
  return panel;
}

void restoreNativeUtilityFrame(NSWindow* window, QWidget* presenter, NSWindow* host) {
  if (window == nil) {
    return;
  }
  NSScreen* screen = host != nil && host.screen != nil ? host.screen : NSScreen.mainScreen;
  const NSRect visible = screen != nil ? screen.visibleFrame : NSZeroRect;
  const QString key = presenter != nullptr ? presenter->property("nativeFrameKey").toString() : QString();
  NSRect frame = NSZeroRect;
  if (!key.isEmpty()) {
    const QString text = QSettings().value(key).toString();
    if (!text.isEmpty()) {
      frame = NSRectFromString(ns(text));
    }
  }
  if (NSWidth(frame) < 240 || NSHeight(frame) < 160 || (!NSIsEmptyRect(visible) && !NSIntersectsRect(frame, visible))) {
    frame = window.frame;
    const NSRect around = host != nil ? host.frame : visible;
    if (!NSIsEmptyRect(around)) {
      frame.origin.x = NSMidX(around) - NSWidth(frame) / 2;
      frame.origin.y = NSMidY(around) - NSHeight(frame) / 2;
    }
  }
  if (!NSIsEmptyRect(visible)) {
    frame.size.width = std::min(NSWidth(frame), NSWidth(visible));
    frame.size.height = std::min(NSHeight(frame), NSHeight(visible));
    frame.origin.x = std::clamp<CGFloat>(frame.origin.x, NSMinX(visible), NSMaxX(visible) - NSWidth(frame));
    frame.origin.y = std::clamp<CGFloat>(frame.origin.y, NSMinY(visible), NSMaxY(visible) - NSHeight(frame));
  }
  [window setFrame:frame display:NO];
}

void saveNativeUtilityFrame(NSWindow* window, QWidget* presenter) {
  const QString key = presenter != nullptr ? presenter->property("nativeFrameKey").toString() : QString();
  if (window != nil && !key.isEmpty()) {
    QSettings().setValue(key, qt(NSStringFromRect(window.frame)));
  }
}

} // namespace mitcad::mac

// ---------------------------------------------------------------------------

@implementation MitcadNativeTileView
- (instancetype)initWithFrame:(NSRect)frame {
  self = [super initWithFrame:frame];
  if (self != nil) {
    side = 96;
    tile = -1;
    picture = [[NSImageView alloc] initWithFrame:NSZeroRect];
    picture.imageScaling = NSImageScaleProportionallyDown;
    picture.imageAlignment = NSImageAlignCenter;
    picture.editable = NO;
    [self addSubview:picture];
    caption = [NSTextField labelWithString:@""];
    caption.alignment = NSTextAlignmentCenter;
    caption.lineBreakMode = NSLineBreakByTruncatingTail;
    caption.maximumNumberOfLines = 1;
    caption.font = [NSFont systemFontOfSize:NSFont.systemFontSize];
    [self addSubview:caption];
    detail = [NSTextField labelWithString:@""];
    detail.alignment = NSTextAlignmentCenter;
    detail.lineBreakMode = NSLineBreakByTruncatingTail;
    detail.maximumNumberOfLines = 1;
    detail.font = [NSFont systemFontOfSize:NSFont.smallSystemFontSize];
    detail.textColor = NSColor.secondaryLabelColor;
    [self addSubview:detail];
    star = mitcad::mac::starButton();
    star.hidden = YES;
    [self addSubview:star];
    self.accessibilityElement = YES;
    self.accessibilityRole = NSAccessibilityCellRole;
  }
  return self;
}
- (BOOL)isFlipped {
  return YES;
}
- (void)setFrameSize:(NSSize)size {
  [super setFrameSize:size];
  [self placeSubviews];
}
- (void)layout {
  [super layout];
  [self placeSubviews];
}
- (void)placeSubviews {
  const CGFloat width = NSWidth(self.bounds);
  const CGFloat size = std::max<CGFloat>(16, std::min<CGFloat>(side, width - 8));
  const CGFloat x = std::floor((width - size) / 2);
  picture.frame = NSMakeRect(x, 6, size, size);
  const CGFloat mark = std::clamp<CGFloat>(std::floor(size / 5), 14, 22);
  star.frame = NSMakeRect(x + size - mark, 6, mark, mark);
  const CGFloat line = std::ceil(caption.intrinsicContentSize.height);
  caption.frame = NSMakeRect(3, 6 + size + 4, width - 6, line);
  const CGFloat small = std::ceil(detail.intrinsicContentSize.height);
  detail.frame = NSMakeRect(3, NSMaxY(caption.frame) + 1, width - 6, small);
}
- (void)refreshCaption {
  caption.textColor = chosen ? NSColor.alternateSelectedControlTextColor : NSColor.labelColor;
}
- (void)drawRect:(NSRect)dirty {
  (void)dirty;
  const NSRect image = picture.frame;
  if (!hasPicture) {
    // Not read yet, or none: PartGrid's grey placeholder.
    [[NSColor colorWithWhite:0.5 alpha:0.16] setFill];
    [[NSBezierPath bezierPathWithRoundedRect:NSInsetRect(image, 4, 4) xRadius:6 yRadius:6] fill];
  }
  if (chosen) {
    NSColor* accent = NSColor.controlAccentColor;
    NSBezierPath* ring = [NSBezierPath bezierPathWithRoundedRect:NSInsetRect(image, -4, -4) xRadius:8 yRadius:8];
    [[accent colorWithAlphaComponent:0.15] setFill];
    [ring fill];
    [accent setStroke];
    ring.lineWidth = 1;
    [ring stroke];
    const CGFloat text = std::ceil(caption.attributedStringValue.size.width);
    const CGFloat width = std::min<CGFloat>(NSWidth(caption.frame), text + 12);
    const NSRect pill = NSMakeRect(NSMidX(caption.frame) - width / 2, NSMinY(caption.frame), width, NSHeight(caption.frame));
    [NSColor.selectedContentBackgroundColor setFill];
    [[NSBezierPath bezierPathWithRoundedRect:pill xRadius:4 yRadius:4] fill];
  }
  if (badge.length > 0) {
    // How many times the design uses the part (In this design).
    NSDictionary* attributes = @{
      NSFontAttributeName : [NSFont systemFontOfSize:NSFont.smallSystemFontSize weight:NSFontWeightMedium],
      NSForegroundColorAttributeName : NSColor.alternateSelectedControlTextColor
    };
    const NSSize size = [badge sizeWithAttributes:attributes];
    const NSRect box = NSMakeRect(NSMinX(image), NSMinY(image), std::ceil(size.width) + 8, std::ceil(size.height) + 2);
    [[NSColor.controlAccentColor colorWithAlphaComponent:0.82] setFill];
    [[NSBezierPath bezierPathWithRoundedRect:box xRadius:4 yRadius:4] fill];
    [badge drawAtPoint:NSMakePoint(NSMinX(box) + 4, NSMinY(box) + 1) withAttributes:attributes];
  }
}
// The whole tile takes the pointer, except its star.
- (NSView*)hitTest:(NSPoint)point {
  NSView* hit = [super hitTest:point];
  if (hit == nil) {
    return nil;
  }
  if (!star.hidden && [hit isDescendantOf:star]) {
    return hit;
  }
  return self;
}
- (BOOL)acceptsFirstMouse:(NSEvent*)event {
  (void)event;
  return YES;
}
- (void)mouseDown:(NSEvent*)event {
  NSView* parent = self.superview;
  while (parent != nil && ![parent isKindOfClass:MitcadNativeTileCollection.class]) {
    parent = parent.superview;
  }
  if (parent != nil) {
    [parent mouseDown:event];
  } else {
    [super mouseDown:event];
  }
}
- (void)updateTrackingAreas {
  if (hoverArea != nil) {
    [self removeTrackingArea:hoverArea];
  }
  hoverArea = [[NSTrackingArea alloc] initWithRect:NSZeroRect
                                           options:NSTrackingMouseEnteredAndExited | NSTrackingActiveInActiveApp |
                                                   NSTrackingInVisibleRect
                                             owner:self
                                          userInfo:nil];
  [self addTrackingArea:hoverArea];
  [super updateTrackingAreas];
}
- (void)mouseEntered:(NSEvent*)event {
  (void)event;
  if (owner != nullptr) {
    owner->hover(tile, true);
  }
}
- (void)mouseExited:(NSEvent*)event {
  (void)event;
  if (owner != nullptr) {
    owner->hover(tile, false);
  }
}
- (void)viewDidChangeEffectiveAppearance {
  [super viewDidChangeEffectiveAppearance];
  self.needsDisplay = YES;
}
@end

@implementation MitcadNativeTileItem
- (void)loadView {
  self.view = [[MitcadNativeTileView alloc] initWithFrame:NSMakeRect(0, 0, 120, 146)];
}
- (void)setSelected:(BOOL)selected {
  [super setSelected:selected];
  if ([self.view isKindOfClass:MitcadNativeTileView.class]) {
    MitcadNativeTileView* view = (MitcadNativeTileView*)self.view;
    view->chosen = selected;
    [view refreshCaption];
    view.needsDisplay = YES;
  }
}
@end

@implementation MitcadNativeTileCollection
- (BOOL)acceptsFirstResponder {
  return YES;
}
- (BOOL)acceptsFirstMouse:(NSEvent*)event {
  (void)event;
  return YES;
}
// A click chooses without taking the keyboard from the document when the
// grid is in a utility panel (the part picker).
- (BOOL)needsPanelToBecomeKey {
  return NO;
}
- (void)mouseDown:(NSEvent*)event {
  if (owner != nullptr) {
    owner->pointer(event);
  } else {
    [super mouseDown:event];
  }
}
- (void)keyDown:(NSEvent*)event {
  if (owner == nullptr || !owner->key(event)) {
    [super keyDown:event];
  }
}
- (BOOL)performKeyEquivalent:(NSEvent*)event {
  if (owner != nullptr && owner->keyEquivalent(event)) {
    return YES;
  }
  return [super performKeyEquivalent:event];
}
- (void)scrollWheel:(NSEvent*)event {
  if (owner != nullptr) {
    owner->hideHover();
  }
  [super scrollWheel:event];
}
- (void)viewDidMoveToWindow {
  [super viewDidMoveToWindow];
  if (owner != nullptr && self.window == nil) {
    owner->hideHover();
  }
}
@end

@implementation MitcadNativeTileTarget
- (NSInteger)collectionView:(NSCollectionView*)collectionView numberOfItemsInSection:(NSInteger)section {
  (void)collectionView;
  (void)section;
  return owner == nullptr ? 0 : owner->count();
}
- (NSCollectionViewItem*)collectionView:(NSCollectionView*)collectionView
     itemForRepresentedObjectAtIndexPath:(NSIndexPath*)indexPath {
  if (owner != nullptr) {
    return owner->item(collectionView, indexPath);
  }
  return [collectionView makeItemWithIdentifier:mitcad::mac::kTileItem forIndexPath:indexPath];
}
- (void)collectionView:(NSCollectionView*)collectionView didSelectItemsAtIndexPaths:(NSSet<NSIndexPath*>*)indexPaths {
  (void)collectionView;
  (void)indexPaths;
  if (owner != nullptr) {
    owner->nativeSelected();
  }
}
- (void)star:(id)sender {
  if (owner != nullptr && [sender isKindOfClass:NSButton.class]) {
    owner->starClicked(((NSButton*)sender).tag);
  }
}
@end

@implementation MitcadNativeGroupNode
@end

@implementation MitcadNativeGroupOutline
- (BOOL)acceptsFirstMouse:(NSEvent*)event {
  (void)event;
  return YES;
}
- (BOOL)needsPanelToBecomeKey {
  return NO;
}
@end

@implementation MitcadNativeGroupTarget
- (NSInteger)outlineView:(NSOutlineView*)outlineView numberOfChildrenOfItem:(id)item {
  (void)outlineView;
  return owner == nullptr ? 0 : static_cast<NSInteger>(owner->childrenOf(item).count);
}
- (id)outlineView:(NSOutlineView*)outlineView child:(NSInteger)index ofItem:(id)item {
  (void)outlineView;
  if (owner == nullptr) {
    return nil;
  }
  NSArray<MitcadNativeGroupNode*>* children = owner->childrenOf(item);
  return index >= 0 && index < static_cast<NSInteger>(children.count) ? children[static_cast<NSUInteger>(index)] : nil;
}
- (BOOL)outlineView:(NSOutlineView*)outlineView isItemExpandable:(id)item {
  (void)outlineView;
  return owner != nullptr && owner->childrenOf(item).count > 0;
}
- (NSView*)outlineView:(NSOutlineView*)outlineView viewForTableColumn:(NSTableColumn*)tableColumn item:(id)item {
  (void)outlineView;
  (void)tableColumn;
  return owner == nullptr ? nil : owner->cell(item);
}
- (void)outlineViewSelectionDidChange:(NSNotification*)notification {
  (void)notification;
  if (owner != nullptr) {
    owner->selected();
  }
}
- (void)outlineViewItemDidExpand:(NSNotification*)notification {
  if (owner != nullptr) {
    owner->folded(notification.userInfo[@"NSObject"], false);
  }
}
- (void)outlineViewItemDidCollapse:(NSNotification*)notification {
  if (owner != nullptr) {
    owner->folded(notification.userInfo[@"NSObject"], true);
  }
}
- (void)star:(id)sender {
  if (owner != nullptr) {
    owner->star(sender);
  }
}
@end

@implementation MitcadNativeSearchTarget
- (void)search:(id)sender {
  (void)sender;
  if (owner != nullptr) {
    owner->searchEdited();
  }
}
- (void)controlTextDidChange:(NSNotification*)notification {
  (void)notification;
  if (owner != nullptr) {
    owner->searchEdited();
  }
}
- (BOOL)control:(NSControl*)control textView:(NSTextView*)textView doCommandBySelector:(SEL)commandSelector {
  (void)control;
  (void)textView;
  return owner != nullptr && owner->searchCommand(commandSelector);
}
@end

@implementation MitcadNativeCatalogueView
- (void)viewDidMoveToWindow {
  [super viewDidMoveToWindow];
  NSSearchField* field = search;
  if (field != nil && self.window != nil && self.window.initialFirstResponder == nil) {
    self.window.initialFirstResponder = field;
  }
}
@end

@implementation MitcadNativeSplitView
- (BOOL)isFlipped {
  return YES;
}
// A divider dragged: the pane before it keeps the dragged size, above the
// proportions and below each pane's minimum.
- (void)dragDivider:(NSInteger)index with:(NSEvent*)event {
  if (index < 0 || index >= static_cast<NSInteger>(panes.count) || self.window == nil) {
    return;
  }
  NSView* pane = panes[static_cast<NSUInteger>(index)];
  id kept = static_cast<NSUInteger>(index) < held.count ? held[static_cast<NSUInteger>(index)] : nil;
  NSLayoutConstraint* constraint = [kept isKindOfClass:NSLayoutConstraint.class] ? (NSLayoutConstraint*)kept : nil;
  const CGFloat start = horizontal ? NSWidth(pane.frame) : NSHeight(pane.frame);
  if (constraint == nil) {
    constraint = horizontal ? [pane.widthAnchor constraintEqualToConstant:start]
                            : [pane.heightAnchor constraintEqualToConstant:start];
    constraint.priority = mitcad::mac::kSplitDragged;
    constraint.active = YES;
    if (static_cast<NSUInteger>(index) < held.count) {
      held[static_cast<NSUInteger>(index)] = constraint;
    }
  }
  const NSPoint origin = [self convertPoint:event.locationInWindow fromView:nil];
  for (;;) {
    NSEvent* next = [self.window nextEventMatchingMask:NSEventMaskLeftMouseDragged | NSEventMaskLeftMouseUp];
    if (next == nil) {
      break;
    }
    const NSPoint point = [self convertPoint:next.locationInWindow fromView:nil];
    const CGFloat delta = horizontal ? point.x - origin.x : point.y - origin.y;
    constraint.constant = std::max<CGFloat>(0, start + delta);
    [self layoutSubtreeIfNeeded];
    if (next.type == NSEventTypeLeftMouseUp) {
      break;
    }
  }
}
@end

@implementation MitcadNativeSplitDivider
- (void)resetCursorRects {
  MitcadNativeSplitView* owner = split;
  const BOOL across = owner != nil && !owner->horizontal;
  [self addCursorRect:self.bounds cursor:across ? NSCursor.resizeUpDownCursor : NSCursor.resizeLeftRightCursor];
}
- (void)drawRect:(NSRect)dirty {
  (void)dirty;
  MitcadNativeSplitView* owner = split;
  const NSRect bounds = self.bounds;
  [NSColor.separatorColor setFill];
  if (owner == nil || owner->horizontal) {
    NSRectFill(NSMakeRect(std::floor(NSMidX(bounds)), 0, 1, NSHeight(bounds)));
  } else {
    NSRectFill(NSMakeRect(0, std::floor(NSMidY(bounds)), NSWidth(bounds), 1));
  }
}
- (void)mouseDown:(NSEvent*)event {
  MitcadNativeSplitView* owner = split;
  [owner dragDivider:index with:event];
}
@end

@implementation MitcadNativeUtilityPanel
- (BOOL)performKeyEquivalent:(NSEvent*)event {
  // The presenter's own shortcuts first (NativeDialogController).
  id<MitcadNativeShortcutTarget> target = (id<MitcadNativeShortcutTarget>)self.delegate;
  if ([target respondsToSelector:@selector(mitcadDispatchShortcut:)] && [target mitcadDispatchShortcut:event]) {
    return YES;
  }
  const NSEventModifierFlags modifiers = event.modifierFlags & (NSEventModifierFlagCommand | NSEventModifierFlagControl |
                                                                NSEventModifierFlagOption | NSEventModifierFlagShift);
  if (modifiers == NSEventModifierFlagCommand && [event.charactersIgnoringModifiers.lowercaseString isEqualToString:@"w"]) {
    if ([self makeFirstResponder:nil]) {
      [self performClose:self];
    }
    return YES;
  }
  return [super performKeyEquivalent:event];
}
- (void)keyDown:(NSEvent*)event {
  const NSEventModifierFlags modifiers = event.modifierFlags & (NSEventModifierFlagCommand | NSEventModifierFlagControl |
                                                                NSEventModifierFlagOption | NSEventModifierFlagShift);
  if (modifiers == 0 && event.keyCode == 53) {
    [self cancelOperation:self];
    return;
  }
  [super keyDown:event];
}
- (void)cancelOperation:(id)sender {
  (void)sender;
  if ([self makeFirstResponder:nil]) {
    [self performClose:self];
  }
}
@end
