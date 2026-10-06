// SPDX-License-Identifier: MIT
#include "BrowserPanel.hpp"

#include <algorithm>
#include <functional>
#include <utility>

#include <QApplication>
#include <QHelpEvent>
#include <QIconEngine>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QPainter>
#include <QScrollBar>
#include <QSet>
#include <QStyledItemDelegate>
#include <QTimer>
#include <QToolTip>
#include <QTreeWidget>
#include <QTreeWidgetItemIterator>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/ChromeStyle.hpp"
#include "../framework/GlassCard.hpp"
#include "../framework/Icons.hpp"
#include "../framework/Theme.hpp"

namespace mitcad {
namespace {

// Delete deletes; on macOS the delete key of the keyboard (Backspace) too.
bool isDeleteKey(int key) {
#ifdef Q_OS_MACOS
  return key == Qt::Key_Delete || key == Qt::Key_Backspace;
#else
  return key == Qt::Key_Delete;
#endif
}

// One column: the tree's indentation and arrows, then each row's light
// bulb, icon, name and (components) radio button, drawn by BrowserDelegate.
constexpr int kName = 0;
// A row's light bulb (whether it shows) and a component's radio button
// (whether it is active); none when not set.
constexpr int kEyeRole = Qt::UserRole + 1;
constexpr int kRadioRole = Qt::UserRole + 2;
// The space between a light bulb and the icon, and between the name and
// the radio button.
constexpr int kGap = 4;
// The browser's width when it opens (mitcad#10).
constexpr int kDefaultWidth = 240;
// The indentation per level, which is also the width the style centres a
// branch's arrow in. The styles' own (20 to 30 pixels: Qt's cross-platform
// style, Windows 11) waste the narrow browser's width and leave the arrows
// far from the names.
constexpr int kIndentation = 16;

const QColor kHiddenText(0x90, 0x90, 0x90);
const QColor kFailedText(0xb0, 0x00, 0x20);

// A light bulb before a row's icon, drawn as one decoration, so that the
// style lays out and highlights the row as it does any other.
class EyeIconEngine : public QIconEngine {
public:
  EyeIconEngine(QIcon eye, QIcon icon) : m_eye(std::move(eye)), m_icon(std::move(icon)) {}

  void paint(QPainter* painter, const QRect& rect, QIcon::Mode mode, QIcon::State state) override {
    const int side = rect.height();
    m_eye.paint(painter, QRect(rect.left(), rect.top(), side, side), Qt::AlignCenter, mode, QIcon::Off);
    m_icon.paint(painter, QRect(rect.right() + 1 - side, rect.top(), side, side), Qt::AlignCenter, mode,
                 state);
  }

  QPixmap pixmap(const QSize& size, QIcon::Mode mode, QIcon::State state) override {
    QPixmap pixmap(size);
    pixmap.fill(Qt::transparent);
    QPainter painter(&pixmap);
    paint(&painter, QRect(QPoint(), size), mode, state);
    return pixmap;
  }

  QIconEngine* clone() const override { return new EyeIconEngine(m_eye, m_icon); }

private:
  QIcon m_eye;
  QIcon m_icon;
};

// Draws a row: the style's item with the light bulb in its decoration,
// and the radio button right after the name, whose text is elided early
// enough to leave room for it.
class BrowserDelegate : public QStyledItemDelegate {
public:
  // Where a row's parts are, in the view's coordinates.
  struct Parts {
    QRect name;  // the name's text as drawn
    QRect eye;   // the light bulb (empty without one)
    QRect radio; // the radio button (empty without one)
  };

  using QStyledItemDelegate::QStyledItemDelegate;

  // The parts of a row whose option the view gave (its rect, font, state).
  Parts parts(const QStyleOptionViewItem& option, const QModelIndex& index) const {
    QStyleOptionViewItem opt = option;
    initStyleOption(&opt, index);
    return layout(opt, index);
  }

  void paint(QPainter* painter, const QStyleOptionViewItem& option, const QModelIndex& index) const override {
    QStyleOptionViewItem opt = option;
    initStyleOption(&opt, index);
    const Parts at = layout(opt, index);
    style(opt)->drawControl(QStyle::CE_ItemViewItem, &opt, painter, opt.widget);
    if (!at.radio.isEmpty()) {
      const bool active = index.data(kRadioRole).toBool();
      themeIcon(active ? QStringLiteral("radio-on") : QStringLiteral("radio-off"))
          .paint(painter, at.radio, Qt::AlignCenter,
                 opt.state & QStyle::State_Selected ? QIcon::Selected : QIcon::Normal);
    }
  }

  QSize sizeHint(const QStyleOptionViewItem& option, const QModelIndex& index) const override {
    QSize size = QStyledItemDelegate::sizeHint(option, index);
    if (index.data(kRadioRole).isValid()) {
      size.rwidth() += kGap + option.decorationSize.height();
    }
    return size;
  }

  bool helpEvent(QHelpEvent* event, QAbstractItemView* view, const QStyleOptionViewItem& option,
                 const QModelIndex& index) override {
    if (event->type() == QEvent::ToolTip && index.isValid()) {
      const Parts at = parts(option, index);
      QString tip;
      if (at.eye.contains(event->pos())) {
        tip = index.data(kEyeRole).toBool() ? BrowserPanel::tr("Hide") : BrowserPanel::tr("Show");
      } else if (at.radio.contains(event->pos())) {
        tip = BrowserPanel::tr("Activate");
      }
      if (!tip.isEmpty()) {
        QToolTip::showText(event->globalPos(), tip, view);
        return true;
      }
    }
    return QStyledItemDelegate::helpEvent(event, view, option, index);
  }

protected:
  void initStyleOption(QStyleOptionViewItem* option, const QModelIndex& index) const override {
    QStyledItemDelegate::initStyleOption(option, index);
    const QVariant eye = index.data(kEyeRole);
    if (eye.isValid()) {
      const int side = option->decorationSize.height();
      option->icon = QIcon(new EyeIconEngine(
          themeIcon(eye.toBool() ? QStringLiteral("eye") : QStringLiteral("eye-off")), option->icon));
      option->decorationSize = QSize(2 * side + kGap, side);
      option->features |= QStyleOptionViewItem::HasDecoration;
    }
  }

private:
  static QStyle* style(const QStyleOptionViewItem& option) {
    return option.widget != nullptr ? option.widget->style() : QApplication::style();
  }

  // The parts of a row whose option is set up; elides its text where the
  // radio button needs the room.
  Parts layout(QStyleOptionViewItem& opt, const QModelIndex& index) const {
    QStyle* style = BrowserDelegate::style(opt);
    const QRect decoration = style->subElementRect(QStyle::SE_ItemViewItemDecoration, &opt, opt.widget);
    const QRect text = style->subElementRect(QStyle::SE_ItemViewItemText, &opt, opt.widget);
    // The margin the style keeps on either side of the text.
    const int margin = style->pixelMetric(QStyle::PM_FocusFrameHMargin, nullptr, opt.widget) + 1;
    const int side = opt.decorationSize.height();
    const bool radio = index.data(kRadioRole).isValid();
    Parts parts;
    if (index.data(kEyeRole).isValid()) {
      parts.eye = QRect(decoration.left(), opt.rect.top(), side + kGap / 2, opt.rect.height());
    }
    const int room = qMax(0, text.width() - 2 * margin - (radio ? kGap + side : 0));
    if (opt.fontMetrics.horizontalAdvance(opt.text) > room) {
      opt.text = opt.fontMetrics.elidedText(opt.text, Qt::ElideRight, room);
    }
    const int width = qMin(opt.fontMetrics.horizontalAdvance(opt.text), room);
    parts.name = QRect(text.left() + margin, opt.rect.top(), width, opt.rect.height());
    if (radio) {
      parts.radio = QRect(parts.name.right() + 1 + kGap, opt.rect.top() + (opt.rect.height() - side) / 2,
                          side, side);
    }
    return parts;
  }
};

// The rows of the tree in a floating card: like a macOS sidebar, a selected
// row is a rounded band of the accent colour (translucent) behind the whole
// row, with its text in semibold, instead of the style's highlight (the
// tree's palette makes that one transparent).
class SidebarDelegate : public BrowserDelegate {
public:
  using BrowserDelegate::BrowserDelegate;

  void paint(QPainter* painter, const QStyleOptionViewItem& option, const QModelIndex& index) const override {
    if (option.state.testFlag(QStyle::State_Selected)) {
      QColor fill = accentColor(option.palette);
      fill.setAlphaF(option.state.testFlag(QStyle::State_Active) ? 0.30f : 0.16f);
      const QRect cell = option.rect;
      const QRect band(cell.left() + 2, cell.top() + 1, option.widget->width() - cell.left() - 4 - 2,
                       cell.height() - 2);
      painter->save();
      painter->setRenderHint(QPainter::Antialiasing);
      painter->setPen(Qt::NoPen);
      painter->setBrush(fill);
      painter->drawRoundedRect(band, 7, 7);
      painter->restore();
    }
    BrowserDelegate::paint(painter, option, index);
  }

  QSize sizeHint(const QStyleOptionViewItem& option, const QModelIndex& index) const override {
    QSize size = BrowserDelegate::sizeHint(option, index);
    size.setHeight(std::max(size.height(), 26));
    return size;
  }

  QWidget* createEditor(QWidget* parent, const QStyleOptionViewItem& option, const QModelIndex& index) const override {
    // The tree's base colour is transparent; an editor needs its own.
    QWidget* editor = BrowserDelegate::createEditor(parent, option, index);
    if (editor != nullptr) {
      editor->setPalette(QApplication::palette());
      editor->setAutoFillBackground(true);
    }
    return editor;
  }

protected:
  void initStyleOption(QStyleOptionViewItem* option, const QModelIndex& index) const override {
    BrowserDelegate::initStyleOption(option, index);
    // The band is the highlight; the name in semibold.
    if (option->state.testFlag(QStyle::State_Selected)) {
      option->state &= ~QStyle::State_Selected;
      option->font.setWeight(QFont::DemiBold);
      option->fontMetrics = QFontMetrics(option->font);
    }
  }
};

struct OriginDatum {
  const char* uid;
  const char* name;
  const char* icon;
};
const OriginDatum kOriginDatums[] = {
    {"origin", "O", "construction-point"}, {"x", "X", "construction-axis"},
    {"y", "Y", "construction-axis"},       {"z", "Z", "construction-axis"},
    {"xy", "XY", "offset-plane"},          {"xz", "XZ", "offset-plane"},
    {"yz", "YZ", "offset-plane"}};

SelectKind datumKind(const QString& type) {
  if (type == QStringLiteral("construction_plane") || type.size() == 2) {
    return SelectKind::Plane;
  }
  if (type == QStringLiteral("construction_axis") || type.size() == 1) {
    return SelectKind::Axis;
  }
  return SelectKind::Point;
}

QString itemKey(const SelectionItem& item) {
  return QStringLiteral("%1|%2|%3")
      .arg(static_cast<unsigned>(item.kind))
      .arg(item.owner, item.occurrence);
}


} // namespace

SelectionItem BrowserNode::item() const {
  SelectionItem item;
  item.occurrence = occurrence;
  item.owner = uid;
  switch (type) {
  case Type::Body:
    item.kind = SelectKind::Body;
    break;
  case Type::Sketch:
    item.kind = SelectKind::Sketch;
    break;
  case Type::Datum:
  case Type::OriginDatum:
    item.kind = datumKind(type == Type::Datum ? featureType : uid);
    item.geometry = SelectionItem::kindName(item.kind);
    break;
  case Type::Component:
    if (occurrence.isEmpty()) {
      return SelectionItem(); // the root is everything
    }
    item.kind = SelectKind::Component;
    break;
  default:
    return SelectionItem();
  }
  return item;
}

// The tree: clicks on a light bulb or a radio button do not select the row,
// F2 renames and Delete deletes.
class BrowserTree : public QTreeWidget {
public:
  explicit BrowserTree(BrowserPanel& panel)
      : QTreeWidget(&panel), m_panel(panel), m_delegate(chromeStyle() == ChromeStyle::Floating ? new SidebarDelegate(this)
                                                                            : new BrowserDelegate(this)) {
    setItemDelegate(m_delegate);
  }

  bool cancelEdit() {
    if (state() != QAbstractItemView::EditingState) {
      return false;
    }
    if (QWidget* editor = indexWidget(currentIndex())) {
      closeEditor(editor, QAbstractItemDelegate::RevertModelCache);
    }
    return true;
  }

  // Where a row's name, light bulb and radio button are in the viewport.
  BrowserDelegate::Parts partsOf(QTreeWidgetItem* item) const {
    const QModelIndex index = indexFromItem(item, kName);
    QStyleOptionViewItem option;
    initViewItemOption(&option);
    option.rect = visualRect(index);
    return m_delegate->parts(option, index);
  }

protected:
  void mousePressEvent(QMouseEvent* event) override {
    if (event->button() == Qt::LeftButton && clickPart(event->position().toPoint())) {
      return;
    }
    QTreeWidget::mousePressEvent(event);
  }

  void mouseDoubleClickEvent(QMouseEvent* event) override {
    // A quick second click on a light bulb is a click of its own.
    if (event->button() == Qt::LeftButton && clickPart(event->position().toPoint())) {
      return;
    }
    QTreeWidgetItem* item = itemAt(event->position().toPoint());
    if (item != nullptr && event->button() == Qt::LeftButton) {
      const BrowserNode node = m_panel.nodeOf(item);
      if (!node.isFolder() && node.type != BrowserNode::Type::Settings &&
          node.type != BrowserNode::Type::NamedViews) {
        emit m_panel.doubleClicked(node);
        return;
      }
    }
    QTreeWidget::mouseDoubleClickEvent(event);
  }

  void keyPressEvent(QKeyEvent* event) override {
    QTreeWidgetItem* item = currentItem();
    if (item != nullptr && state() != QAbstractItemView::EditingState) {
      if (event->key() == Qt::Key_F2) {
        m_panel.startRename(m_panel.nodeOf(item));
        return;
      }
      if (isDeleteKey(event->key())) {
        emit m_panel.deleteRequested(m_panel.nodeOf(item));
        return;
      }
    }
    QTreeWidget::keyPressEvent(event);
  }

private:
  // Clicks the light bulb or the radio button at a point; false when there
  // is none.
  bool clickPart(const QPoint& at) {
    QTreeWidgetItem* item = itemAt(at);
    if (item == nullptr) {
      return false;
    }
    const BrowserNode node = m_panel.nodeOf(item);
    const BrowserDelegate::Parts parts = partsOf(item);
    if (node.hasEye && parts.eye.contains(at)) {
      emit m_panel.eyeClicked(node);
      return true;
    }
    if (node.type == BrowserNode::Type::Component && parts.radio.contains(at)) {
      emit m_panel.activateClicked(node);
      return true;
    }
    return false;
  }

  BrowserPanel& m_panel;
  BrowserDelegate* m_delegate;
};

BrowserPanel::BrowserPanel(QWidget* parent) : QWidget(parent), m_tree(new BrowserTree(*this)) {
  auto* layout = new QVBoxLayout(this);
  layout->setContentsMargins(0, 0, 0, 0);
  layout->addWidget(m_tree);
  m_tree->setColumnCount(1);
  m_tree->setHeaderHidden(true);
  m_tree->setSelectionMode(QAbstractItemView::ExtendedSelection);
  m_tree->setEditTriggers(QAbstractItemView::NoEditTriggers);
  m_tree->setContextMenuPolicy(Qt::CustomContextMenu);
  m_tree->setUniformRowHeights(true);
  m_tree->setIconSize(QSize(18, 18));
  m_tree->setIndentation(kIndentation);
  m_tree->setExpandsOnDoubleClick(false);
  // Only a click gives the tree the keyboard, so that the view keeps the
  // single-letter shortcuts.
  m_tree->setFocusPolicy(Qt::ClickFocus);
  setMinimumWidth(160);
  if (chromeStyle() == ChromeStyle::Floating) {
    // Inside a card: the card's material shows through, rows as a sidebar's
    // (SidebarDelegate).
    m_tree->setFrameShape(QFrame::NoFrame);
    m_tree->setAttribute(Qt::WA_MacShowFocusRect, false);
    QPalette palette = m_tree->palette();
    for (const QPalette::ColorRole role : {QPalette::Base, QPalette::AlternateBase, QPalette::Highlight}) {
      palette.setColor(role, Qt::transparent);
    }
    m_tree->setPalette(palette);
    m_tree->viewport()->setAutoFillBackground(false);
  }

  connect(m_tree, &QTreeWidget::itemSelectionChanged, this, &BrowserPanel::selectionChanged);
  connect(m_tree, &QTreeWidget::itemChanged, this, &BrowserPanel::itemEdited);
  connect(m_tree, &QTreeWidget::customContextMenuRequested, this, [this](const QPoint& at) {
    QTreeWidgetItem* item = m_tree->itemAt(at);
    if (item == nullptr) {
      return;
    }
    if (!item->isSelected()) {
      // A right click on a row that is not selected selects it.
      m_tree->clearSelection();
      item->setSelected(true);
    }
    emit contextMenuRequested(nodeOf(item), m_tree->viewport()->mapToGlobal(at));
  });
  const auto remember = [this](QTreeWidgetItem* item, bool expanded) {
    if (!m_building) {
      m_expanded.insert(keyOf(nodeOf(item)), expanded);
      emit contentHeightChanged();
      QTimer::singleShot(0, this, &BrowserPanel::logLayout);
    }
  };
  connect(m_tree, &QTreeWidget::itemExpanded, this,
          [remember](QTreeWidgetItem* item) { remember(item, true); });
  connect(m_tree, &QTreeWidget::itemCollapsed, this,
          [remember](QTreeWidgetItem* item) { remember(item, false); });
}

QSize BrowserPanel::sizeHint() const { return QSize(kDefaultWidth, QWidget::sizeHint().height()); }

BrowserNode BrowserPanel::nodeOf(const QTreeWidgetItem* item) const {
  return m_nodes.value(const_cast<QTreeWidgetItem*>(item));
}

QString BrowserPanel::keyOf(const BrowserNode& node) const {
  const QString at = QLatin1Char('@') + node.occurrence;
  switch (node.type) {
  case BrowserNode::Type::Settings:
    return QStringLiteral("settings");
  case BrowserNode::Type::Units:
    return QStringLiteral("units");
  case BrowserNode::Type::NamedViews:
    return QStringLiteral("views");
  case BrowserNode::Type::NamedView:
    return QStringLiteral("view:") + node.uid;
  case BrowserNode::Type::Origin:
    return QStringLiteral("origin");
  case BrowserNode::Type::OriginDatum:
    return QStringLiteral("origin:") + node.uid;
  case BrowserNode::Type::Component:
    return QStringLiteral("component") + at;
  case BrowserNode::Type::Bodies:
    return QStringLiteral("bodies") + at;
  case BrowserNode::Type::Sketches:
    return QStringLiteral("sketches") + at;
  case BrowserNode::Type::Construction:
    return QStringLiteral("construction") + at;
  case BrowserNode::Type::Body:
    return QStringLiteral("body:") + node.uid + at;
  case BrowserNode::Type::Sketch:
    return QStringLiteral("sketch:") + node.uid + at;
  case BrowserNode::Type::Datum:
    return QStringLiteral("datum:") + node.uid + at;
  case BrowserNode::Type::None:
    break;
  }
  return QString();
}

QTreeWidgetItem* BrowserPanel::addRow(QTreeWidgetItem* parent, const BrowserNode& node,
                                      const QString& icon, const QString& tooltip) {
  auto* item = parent != nullptr ? new QTreeWidgetItem(parent) : new QTreeWidgetItem(m_tree);
  BrowserNode stored = node;
  stored.path = parent != nullptr ? m_nodes.value(parent).path + QLatin1Char('/') + node.name : node.name;
  item->setText(kName, node.name);
  item->setIcon(kName, themeIcon(icon));
  if (node.hasEye) {
    item->setData(kName, kEyeRole, node.visible);
  }
  if (!tooltip.isEmpty()) {
    item->setToolTip(kName, tooltip);
  }
  const bool renamable = node.type == BrowserNode::Type::Component ||
                         node.type == BrowserNode::Type::Body ||
                         node.type == BrowserNode::Type::Sketch ||
                         node.type == BrowserNode::Type::Datum ||
                         (node.type == BrowserNode::Type::NamedView &&
                          node.uid.startsWith(QStringLiteral("named:")));
  if (renamable) {
    item->setFlags(item->flags() | Qt::ItemIsEditable);
  }
  m_nodes.insert(item, stored);
  const QString key = keyOf(node);
  const bool open = node.type == BrowserNode::Type::Component || node.type == BrowserNode::Type::Bodies ||
                    node.type == BrowserNode::Type::Sketches ||
                    node.type == BrowserNode::Type::Construction;
  item->setExpanded(m_expanded.value(key, open));
  return item;
}

void BrowserPanel::addComponentRows(QTreeWidgetItem* parent, const DocumentSnapshot& snapshot,
                                    const QString& component, const QString& occurrence,
                                    bool shown) {
  const auto gray = [](QTreeWidgetItem* item, bool visible) {
    if (!visible) {
      item->setForeground(kName, kHiddenText);
    }
  };
  const DocumentSnapshot::Component* def = snapshot.component(component);
  BrowserNode folder;
  folder.occurrence = occurrence;
  folder.component = component;
  folder.hasEye = true;

  // Bodies.
  if (def != nullptr && !def->bodies.isEmpty()) {
    folder.type = BrowserNode::Type::Bodies;
    folder.name = tr("Bodies");
    folder.visible = false;
    for (const QString& uid : def->bodies) {
      folder.visible = folder.visible || snapshot.bodies.value(uid).visible;
    }
    QTreeWidgetItem* bodies = addRow(parent, folder, QStringLiteral("folder"));
    gray(bodies, shown && folder.visible);
    for (const QString& uid : def->bodies) {
      const DocumentSnapshot::Body body = snapshot.bodies.value(uid);
      BrowserNode node;
      node.type = BrowserNode::Type::Body;
      node.uid = uid;
      node.occurrence = occurrence;
      node.component = component;
      node.name = body.name.isEmpty() ? uid : body.name;
      node.visible = body.visible;
      node.hasEye = true;
      const QString feature = uid.section(QLatin1Char('.'), 0, 0);
      const DocumentSnapshot::Feature* made = snapshot.feature(feature);
      QTreeWidgetItem* row = addRow(bodies, node, QStringLiteral("body"),
                                    tr("%1 (%2), made by %3")
                                        .arg(node.name, uid, made != nullptr ? made->name : feature));
      gray(row, shown && body.visible);
    }
  }

  // Sketches and construction geometry: features of the component before
  // the marker.
  for (const bool sketches : {true, false}) {
    QVector<const DocumentSnapshot::Feature*> features;
    for (const DocumentSnapshot::Feature& feature : snapshot.features) {
      if (feature.component == component && feature.isActive() &&
          (sketches ? feature.isSketch() : feature.isConstruction())) {
        features.append(&feature);
      }
    }
    if (features.isEmpty()) {
      continue;
    }
    folder.type = sketches ? BrowserNode::Type::Sketches : BrowserNode::Type::Construction;
    folder.name = sketches ? tr("Sketches") : tr("Construction");
    folder.visible = false;
    for (const DocumentSnapshot::Feature* feature : features) {
      folder.visible = folder.visible || snapshot.isShown(feature->uid);
    }
    QTreeWidgetItem* rows = addRow(parent, folder, QStringLiteral("folder"));
    gray(rows, shown && folder.visible);
    for (const DocumentSnapshot::Feature* feature : features) {
      BrowserNode node;
      node.type = sketches ? BrowserNode::Type::Sketch : BrowserNode::Type::Datum;
      node.uid = feature->uid;
      node.occurrence = occurrence;
      node.component = component;
      node.featureType = feature->type;
      node.name = feature->name;
      node.visible = snapshot.isShown(feature->uid);
      node.hasEye = true;
      const bool failed = feature->status == QStringLiteral("error");
      // A sketch's degrees of freedom (P9): a fully constrained one has a
      // lock on its icon; the tooltip tells how many remain.
      QString icon = featureIcon(feature->type);
      QString tip = failed ? QStringLiteral("%1: %2").arg(feature->name, feature->error) : QString();
      if (sketches && !failed && feature->dof >= 0) {
        icon = feature->dof == 0 ? QStringLiteral("sketch-locked") : icon;
        tip = feature->dof == 0 ? tr("%1: fully constrained").arg(feature->name)
                                : tr("%1: %n degree(s) of freedom", nullptr, feature->dof).arg(feature->name);
      }
      QTreeWidgetItem* row = addRow(rows, node, icon, tip);
      if (failed) {
        row->setForeground(kName, kFailedText);
      } else {
        gray(row, shown && node.visible);
      }
    }
  }
}

void BrowserPanel::rebuild(const DocumentSnapshot& snapshot, bool originShown) {
  // What to keep: the selected rows and the scroll position.
  QSet<QString> selected;
  for (QTreeWidgetItem* item : m_tree->selectedItems()) {
    selected.insert(keyOf(nodeOf(item)));
  }
  const int scroll = m_tree->verticalScrollBar()->value();

  m_building = true;
  m_tree->clear();
  m_nodes.clear();

  // The root component.
  const QString root = snapshot.components.isEmpty() ? tr("Root") : snapshot.components.first().name;
  BrowserNode rootNode;
  rootNode.type = BrowserNode::Type::Component;
  rootNode.uid = QStringLiteral("C0");
  rootNode.component = rootNode.uid;
  rootNode.name = root;
  QTreeWidgetItem* rootItem = addRow(nullptr, rootNode, QStringLiteral("component"),
                                     tr("The design's root component"));

  BrowserNode node;
  node.component = rootNode.uid;
  node.type = BrowserNode::Type::Settings;
  node.name = tr("Document Settings");
  QTreeWidgetItem* settings = addRow(rootItem, node, QStringLiteral("settings"));
  node.type = BrowserNode::Type::Units;
  node.name = tr("Units: %1").arg(snapshot.lengthUnit);
  addRow(settings, node, QStringLiteral("units"), tr("Double-click to change the document's unit"));

  node.type = BrowserNode::Type::NamedViews;
  node.name = tr("Named Views");
  QTreeWidgetItem* views = addRow(rootItem, node, QStringLiteral("named-view"));
  for (const auto& [uid, name] : {std::pair{"home", "Home"}, std::pair{"top", "Top"},
                                  std::pair{"front", "Front"}, std::pair{"right", "Right"}}) {
    node.type = BrowserNode::Type::NamedView;
    node.uid = QString::fromLatin1(uid);
    node.name = tr(name);
    addRow(views, node, QStringLiteral("named-view"), tr("Double-click to look from here"));
  }
  // The document's own views (U5); its Home is the Home row's.
  for (const QJsonValue& value : snapshot.namedViews) {
    const QString name = value.toObject().value(QStringLiteral("name")).toString();
    if (name == QStringLiteral("Home")) {
      continue;
    }
    node.type = BrowserNode::Type::NamedView;
    node.uid = QStringLiteral("named:") + name;
    node.name = name;
    addRow(views, node, QStringLiteral("named-view"), tr("Double-click to look from here"));
  }

  node = BrowserNode();
  node.component = rootNode.uid;
  node.type = BrowserNode::Type::Origin;
  node.name = tr("Origin");
  node.hasEye = true;
  node.visible = originShown;
  QTreeWidgetItem* origin = addRow(rootItem, node, QStringLiteral("origin"));
  if (!originShown) {
    origin->setForeground(kName, kHiddenText);
  }
  for (const OriginDatum& datum : kOriginDatums) {
    BrowserNode row;
    row.type = BrowserNode::Type::OriginDatum;
    row.component = rootNode.uid;
    row.uid = QString::fromLatin1(datum.uid);
    row.name = QString::fromLatin1(datum.name);
    QTreeWidgetItem* item = addRow(origin, row, QString::fromLatin1(datum.icon));
    if (!originShown) {
      item->setForeground(kName, kHiddenText);
    }
  }

  addComponentRows(rootItem, snapshot, rootNode.uid, QString(), true);

  // The occurrences, each with its component's rows.
  std::function<void(QTreeWidgetItem*, const QVector<DocumentSnapshot::Occurrence>&, bool)> place =
      [&](QTreeWidgetItem* parent, const QVector<DocumentSnapshot::Occurrence>& occurrences,
          bool shown) {
        for (const DocumentSnapshot::Occurrence& occurrence : occurrences) {
          BrowserNode row;
          row.type = BrowserNode::Type::Component;
          row.uid = occurrence.component;
          row.component = occurrence.component;
          row.occurrence = occurrence.path;
          row.name = occurrence.name;
          row.visible = occurrence.visible;
          row.hasEye = true;
          const DocumentSnapshot::Component* def = snapshot.component(occurrence.component);
          QString tip = tr("Occurrence %1 of %2").arg(occurrence.name, snapshot.componentName(occurrence.component));
          if (occurrence.grounded) {
            tip += tr(", grounded");
          }
          if (def != nullptr && def->linked) {
            tip += tr(", linked to a file");
          }
          QTreeWidgetItem* item =
              addRow(parent, row,
                     occurrence.grounded ? QStringLiteral("component-grounded") : QStringLiteral("component"),
                     tip);
          const bool visible = shown && occurrence.visible;
          if (!visible) {
            item->setForeground(kName, kHiddenText);
          }
          item->setData(kName, kRadioRole, snapshot.activeComponent == occurrence.component);
          addComponentRows(item, snapshot, occurrence.component, occurrence.path, visible);
          place(item, occurrence.children, visible);
        }
      };
  place(rootItem, snapshot.occurrences, true);
  rootItem->setData(kName, kRadioRole, snapshot.activeComponent == rootNode.uid);

  for (auto it = m_nodes.cbegin(); it != m_nodes.cend(); ++it) {
    if (selected.contains(keyOf(it.value()))) {
      it.key()->setSelected(true);
    }
  }
  m_building = false;
  m_tree->verticalScrollBar()->setValue(scroll);
  emit contentHeightChanged();
  QTimer::singleShot(0, this, &BrowserPanel::logLayout);
}

int BrowserPanel::contentHeight() const {
  // The rows in sight: those whose parents are all open.
  int rows = 0;
  for (QTreeWidgetItemIterator it(m_tree); *it != nullptr; ++it) {
    bool shown = !(*it)->isHidden();
    for (const QTreeWidgetItem* parent = (*it)->parent(); shown && parent != nullptr; parent = parent->parent()) {
      shown = parent->isExpanded();
    }
    rows += shown ? 1 : 0;
  }
  const int row = rows > 0 ? m_tree->sizeHintForRow(0) : 0;
  return rows * std::max(row, 20) + 2 * m_tree->frameWidth();
}

void BrowserPanel::showSelection(const Selection& items) {
  QSet<QString> keys;
  for (const SelectionItem& item : items) {
    keys.insert(itemKey(item));
  }
  const QSignalBlocker blocker(m_tree);
  QTreeWidgetItem* first = nullptr;
  for (auto it = m_nodes.cbegin(); it != m_nodes.cend(); ++it) {
    const SelectionItem item = it.value().item();
    const bool on = item.isValid() && keys.contains(itemKey(item));
    it.key()->setSelected(on);
    if (on && first == nullptr) {
      first = it.key();
    }
  }
  if (first != nullptr) {
    m_tree->scrollToItem(first);
  }
  m_tree->viewport()->update();
  logSelection();
}

void BrowserPanel::logSelection() {
  QStringList paths;
  for (QTreeWidgetItemIterator it(m_tree); *it != nullptr; ++it) {
    if ((*it)->isSelected()) {
      paths << nodeOf(*it).path;
    }
  }
  const QString logged = paths.join(QStringLiteral(", "));
  if (logged != m_loggedSelection) {
    m_loggedSelection = logged;
    qDebug().noquote() << QStringLiteral("Browser selected: %1")
                              .arg(logged.isEmpty() ? QStringLiteral("nothing") : logged);
  }
}

QVector<BrowserNode> BrowserPanel::nodesOfFeature(const QString& feature) const {
  QVector<BrowserNode> nodes;
  for (QTreeWidgetItemIterator it(m_tree); *it != nullptr; ++it) {
    const BrowserNode node = nodeOf(*it);
    const bool made = node.type == BrowserNode::Type::Body &&
                      node.uid.startsWith(feature + QLatin1Char('.'));
    if (made || ((node.type == BrowserNode::Type::Sketch || node.type == BrowserNode::Type::Datum) &&
                 node.uid == feature)) {
      nodes.append(node);
    }
  }
  return nodes;
}

bool BrowserPanel::reveal(const QString& feature) {
  QSet<QString> keys;
  for (const BrowserNode& node : nodesOfFeature(feature)) {
    keys.insert(keyOf(node));
  }
  return revealKeys(keys);
}

bool BrowserPanel::revealItem(const SelectionItem& item) {
  SelectionItem whole = item;
  if (item.kind == SelectKind::Face || item.kind == SelectKind::Edge || item.kind == SelectKind::Vertex) {
    whole.kind = SelectKind::Body; // the body it is on
  }
  whole.name.clear();
  whole.geometry.clear();
  QSet<QString> keys;
  for (auto it = m_nodes.cbegin(); it != m_nodes.cend(); ++it) {
    const SelectionItem row = it.value().item();
    if (row.isValid() && itemKey(row) == itemKey(whole)) {
      keys.insert(keyOf(it.value()));
    }
  }
  return revealKeys(keys);
}

bool BrowserPanel::revealKeys(const QSet<QString>& keys) {
  QTreeWidgetItem* first = nullptr;
  {
    const QSignalBlocker blocker(m_tree);
    m_tree->clearSelection();
    for (auto it = m_nodes.cbegin(); it != m_nodes.cend(); ++it) {
      if (!keys.contains(keyOf(it.value()))) {
        continue;
      }
      for (QTreeWidgetItem* parent = it.key()->parent(); parent != nullptr; parent = parent->parent()) {
        parent->setExpanded(true);
      }
      it.key()->setSelected(true);
      if (first == nullptr) {
        first = it.key();
      }
    }
  }
  if (first == nullptr) {
    return false;
  }
  m_tree->scrollToItem(first);
  selectionChanged();
  return true;
}

void BrowserPanel::startRename(const BrowserNode& node) {
  for (auto it = m_nodes.cbegin(); it != m_nodes.cend(); ++it) {
    if (keyOf(it.value()) == keyOf(node) && (it.key()->flags() & Qt::ItemIsEditable)) {
      m_tree->setFocus();
      m_tree->setCurrentItem(it.key(), kName);
      m_tree->scrollToItem(it.key());
      m_tree->editItem(it.key(), kName);
      qDebug().noquote() << QStringLiteral("Renaming %1").arg(node.path.isEmpty() ? node.name : node.path);
      return;
    }
  }
}

bool BrowserPanel::cancelEditing() { return m_tree->cancelEdit(); }

void BrowserPanel::selectionChanged() {
  if (m_building) {
    return;
  }
  logSelection();
  Selection items;
  for (QTreeWidgetItem* row : m_tree->selectedItems()) {
    const SelectionItem item = nodeOf(row).item();
    if (item.isValid() && !items.contains(item)) {
      items.append(item);
    }
  }
  emit itemsPicked(items);
}

void BrowserPanel::itemEdited(QTreeWidgetItem* item, int column) {
  if (m_building || column != kName) {
    return;
  }
  const BrowserNode node = nodeOf(item);
  const QString name = item->text(kName).trimmed();
  if (!node.isValid() || name == node.name) {
    return;
  }
  if (name.isEmpty()) {
    const QSignalBlocker blocker(m_tree);
    item->setText(kName, node.name);
    return;
  }
  emit renamed(node, name);
}

void BrowserPanel::logLayout() {
  if (!isVisible()) {
    return;
  }
  QStringList lines;
  // The panel's width, and whether the rows need a horizontal scroll bar.
  lines << QStringLiteral("Browser width %1%2")
               .arg(width())
               .arg(m_tree->horizontalScrollBar()->isVisible() ? QStringLiteral(", scrolls sideways")
                                                               : QString());
  const QRect viewport = m_tree->viewport()->rect();
  const auto at = [this](const QRect& rect) {
    const QPoint point = GlassCard::mapToHost(m_tree->viewport(), rect.center());
    return QStringLiteral("%1,%2").arg(point.x()).arg(point.y());
  };
  for (QTreeWidgetItemIterator it(m_tree); *it != nullptr; ++it) {
    QTreeWidgetItem* item = *it;
    const QRect row = m_tree->visualItemRect(item);
    if (row.isEmpty() || !viewport.intersects(row)) {
      continue; // in a collapsed folder or scrolled away
    }
    const BrowserNode node = nodeOf(item);
    const BrowserDelegate::Parts parts = m_tree->partsOf(item);
    lines << QStringLiteral("Browser %1 at %2").arg(node.path, at(parts.name));
    if (!parts.eye.isEmpty()) {
      lines << QStringLiteral("Browser eye %1 at %2").arg(node.path, at(parts.eye));
    }
    if (!parts.radio.isEmpty()) {
      lines << QStringLiteral("Browser radio %1 at %2").arg(node.path, at(parts.radio));
    }
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_logged) {
    m_logged = logged;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  }
}

} // namespace mitcad
