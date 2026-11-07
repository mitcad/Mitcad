// SPDX-License-Identifier: MIT
#include "BrowserPanel.hpp"

#include <algorithm>
#include <cmath>
#include <cstddef>
#include <functional>
#include <iterator>
#include <optional>
#include <utility>
#include <vector>

#include <QApplication>
#include <QCursor>
#include <QFont>
#include <QFontMetrics>
#include <QJsonArray>
#include <QJsonObject>
#include <QHelpEvent>
#include <QIconEngine>
#include <QItemSelectionModel>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QPainter>
#include <QPen>
#include <QPersistentModelIndex>
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
#include "../framework/LengthUnits.hpp"
#include "../framework/NoticeLevel.hpp"
#include "../framework/PanelTree.hpp"
#include "../framework/PlaceWatcher.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "../framework/WindowStyle.hpp"
#include "Bindings.hpp"
#include "FeatureStatus.hpp"
#include "../view/CenterOfMass.hpp"

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
// A feature row whose result is in other components than its sketch
// (mitcad#177): their names.
constexpr int kLinkedRole = Qt::UserRole + 3;
// A row whose item follows expressions (mitcad#862): how many of its
// properties do, shown as the fx badge after its name (`fx 3`).
constexpr int kBindingsRole = Qt::UserRole + 4;
// The light bulb's tooltip of a row that follows a Show When condition and
// of a folder that holds such rows (mitcad#863), in place of Hide / Show.
constexpr int kEyeTipRole = Qt::UserRole + 8;
// A row that follows a Show When condition itself (mitcad#863): its bulb
// has the small fx badge; a folder's never has.
constexpr int kEyeBoundRole = Qt::UserRole + 9;
// A body row's Make tab marker (mitcad#1049): what of its material and
// print settings differs from its component's, the marker's tooltip;
// empty without one.
constexpr int kMakeRole = Qt::UserRole + 10;
// A transparent body or component (mitcad#1093): its light bulb, while on,
// is the half-open eye.
constexpr int kEyeTransparentRole = Qt::UserRole + 11;
// A row's name colour role (a ColorRole as an int, mitcad#1104; see
// setNameRole).
constexpr int kTextRole = Qt::UserRole + 12;
// Every item data role of the rows, each a number of its own: two features
// that once shared one (mitcad#1143) misread each other's values, a
// transparent row's name painted red. A new role goes in this block and in
// this list.
constexpr int kRowRoles[] = {kEyeRole,      kRadioRole, kLinkedRole,         kBindingsRole, kEyeTipRole,
                             kEyeBoundRole, kMakeRole,  kEyeTransparentRole, kTextRole};
constexpr bool rowRolesDistinct() {
  for (std::size_t i = 0; i < std::size(kRowRoles); ++i) {
    for (std::size_t j = i + 1; j < std::size(kRowRoles); ++j) {
      if (kRowRoles[i] == kRowRoles[j]) {
        return false;
      }
    }
  }
  return true;
}
static_assert(rowRolesDistinct(), "two browser row roles share a number (mitcad#1143)");

// The fx badge's font and text (mitcad#862).
QFont bindingsFont(const QFont& font) {
  QFont badge = font;
  badge.setPointSizeF(std::max(6.0, font.pointSizeF() > 0 ? font.pointSizeF() * 0.85 : 8.0));
  badge.setWeight(QFont::DemiBold);
  badge.setItalic(false);
  return badge;
}
// The fx badge's colours (mitcad#862 review): dark text on a solid light
// fill (about 6.5:1), light text on a solid dark fill in a dark theme
// (about 9:1); opaque, so that a selected row's band does not thin them.
struct BindingsColors {
  QColor text;
  QColor fill;
};
BindingsColors bindingsColors(const QPalette& palette) {
  // The bound state's roles (mitcad#1041), by the base the rows sit on.
  const bool dark = palette.color(QPalette::Base).lightness() < 128;
  return {themeColor(ColorRole::BoundText, dark), themeColor(ColorRole::BoundFill, dark)};
}
QString bindingsText(int count) { return QStringLiteral("fx %1").arg(count); }
int bindingsWidth(const QFont& font, int count) {
  return QFontMetrics(bindingsFont(font)).horizontalAdvance(bindingsText(count)) + 10;
}
// The Make tab's marker's side (mitcad#1049): half an icon's.
int makeMarkSide(int iconSide) { return std::max(6, iconSide / 2); }
// The space between a light bulb and the icon, and between the name and
// the radio button.
constexpr int kGap = 4;
// The browser's width when it opens (mitcad#10).
constexpr int kDefaultWidth = 240;
// With the Rows background (mitcad#419), the room in a row's box after its
// name or radio button.
constexpr int kBoxPadding = 8;
// The indentation per level, which is also the width the style centres a
// branch's arrow in: the side panels' common one (framework/PanelTree.hpp).
constexpr int kIndentation = panel::kTreeIndentation;

// A row's name in a state's or a level's colour: a hidden row's, a failed
// one's, a warned one's (the roles of mitcad#1041: the old fixed grey and
// red read about 3:1 and 2.2:1 on the light and the dark panel). The item
// keeps the colour role, which BrowserDelegate resolves when it paints the
// row, by the palette it is painted on (rowsPalette): a colour taken when
// the row was styled kept that theme's, so that after a switch to the dark
// theme a hidden row's light-theme grey stayed on the dark card and its
// name did not show (mitcad#1104). The role is kept under kTextRole.
void setNameRole(QTreeWidgetItem* item, ColorRole role) {
  item->setData(kName, kTextRole, static_cast<int>(role));
}
void clearNameRole(QTreeWidgetItem* item) { item->setData(kName, kTextRole, QVariant()); }
// The level's text role, the one noticeColor gives the colour of.
ColorRole noticeTextRole(NoticeLevel level) {
  switch (level) {
  case NoticeLevel::Error:
    return ColorRole::ErrorText;
  case NoticeLevel::Warning:
    return ColorRole::WarningText;
  case NoticeLevel::Success:
    return ColorRole::SuccessText;
  default:
    return ColorRole::InfoText;
  }
}
// The palette the rows are painted on, for a role's light or dark value:
// that of the view's first ancestor with an opaque background (a Docked
// panel's surface, the window), else the application's (a Floating card's
// material shows through), as setThemedStyleSheet picks it (mitcad#961).
QPalette rowsPalette(const QWidget* view) {
  for (const QWidget* up = view != nullptr ? view->parentWidget() : nullptr; up != nullptr;
       up = up->parentWidget()) {
    if (up->palette().color(QPalette::Window).alpha() == 255) {
      return up->palette();
    }
  }
  return QGuiApplication::palette();
}
// A name role's colour on the rows' palette (the delegate's and the log's).
QColor nameColor(ColorRole role, const QWidget* view) { return themeColor(role, rowsPalette(view)); }

// A row's name colour role (kTextRole, mitcad#1104) as a tone another
// presentation draws with its own colours (mitcad#1079,
// PresentationRow::tone): the same source as the Qt rows' colour.
QString rowTone(const QTreeWidgetItem* item) {
  const QVariant role = item->data(kName, kTextRole);
  if (!role.isValid()) {
    return QString();
  }
  switch (static_cast<ColorRole>(role.toInt())) {
  case ColorRole::ErrorText:
    return QStringLiteral("failed");
  case ColorRole::WarningText:
    return QStringLiteral("warning");
  case ColorRole::TextHidden:
    return QStringLiteral("dimmed");
  default:
    return QString();
  }
}

// A light bulb before a row's icon, drawn as one decoration, so that the
// style lays out and highlights the row as it does any other.
class EyeIconEngine : public QIconEngine {
public:
  // `bulb`: the bulb's icon name (eye, eye-off, eye-half), which the icon
  // gives as its name (QIcon::name), for the unit test (mitcad#1143).
  EyeIconEngine(QIcon eye, QIcon icon, QString bulb)
      : m_eye(std::move(eye)), m_icon(std::move(icon)), m_bulb(std::move(bulb)) {}

  QString iconName() override { return m_bulb; }

  // The bulb of a row that follows a Show When condition (mitcad#863): the
  // whole plain bulb (open or closed eye) with a small fx badge in its lower
  // right corner, outlined in the accent colour as the row's fx badge
  // (mitcad#862), on the view's base colour, with darker text.
  void setBadge(const QColor& accent, const QColor& base, const QColor& text) {
    m_badge = true;
    m_accent = accent;
    m_base = base;
    m_text = text;
  }

  void paint(QPainter* painter, const QRect& rect, QIcon::Mode mode, QIcon::State state) override {
    const int side = rect.height();
    const QRect eye(rect.left(), rect.top(), side, side);
    m_eye.paint(painter, eye, Qt::AlignCenter, mode, QIcon::Off);
    if (m_badge) {
      paintBadge(painter, eye);
    }
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

  QIconEngine* clone() const override {
    auto* copy = new EyeIconEngine(m_eye, m_icon, m_bulb);
    if (m_badge) {
      copy->setBadge(m_accent, m_base, m_text);
    }
    return copy;
  }

private:
  // The badge in the eye's lower right corner, reaching a little into the
  // gap before the row's icon, so that it covers only the eye's corner.
  void paintBadge(QPainter* painter, const QRect& eye) const {
    const int height = std::max(7, eye.height() / 2);
    const int width = std::max(10, eye.width() * 3 / 4);
    const QRectF badge(eye.right() + 1 + kGap / 2 - width, eye.bottom() + 1 - height, width, height);
    const QRectF pill = badge.adjusted(0.5, 0.5, -0.5, -0.5);
    const qreal radius = pill.height() / 2;
    QColor tint = m_accent;
    tint.setAlphaF(0.14f);
    painter->save();
    painter->setRenderHint(QPainter::Antialiasing);
    painter->setPen(Qt::NoPen);
    painter->setBrush(m_base); // opaque: the eye does not show through
    painter->drawRoundedRect(pill, radius, radius);
    painter->setPen(QPen(m_accent, 1.0));
    painter->setBrush(tint);
    painter->drawRoundedRect(pill, radius, radius);
    QFont font = painter->font();
    font.setPixelSize(std::max(6, height - 2));
    font.setBold(true);
    font.setItalic(true);
    painter->setFont(font);
    painter->setPen(m_text);
    painter->drawText(badge, Qt::AlignCenter, QStringLiteral("fx"));
    painter->restore();
  }

  QIcon m_eye;
  QIcon m_icon;
  QString m_bulb;
  bool m_badge = false;
  QColor m_accent;
  QColor m_base;
  QColor m_text;
};

// The icon of a feature row whose result is in another component than its
// sketch (mitcad#177): the feature's icon with a small component in its
// lower right corner.
class LinkedIconEngine : public QIconEngine {
public:
  LinkedIconEngine(QIcon icon, QIcon badge) : m_icon(std::move(icon)), m_badge(std::move(badge)) {}

  void paint(QPainter* painter, const QRect& rect, QIcon::Mode mode, QIcon::State state) override {
    m_icon.paint(painter, rect, Qt::AlignCenter, mode, state);
    const int side = std::max(6, rect.height() * 3 / 5);
    m_badge.paint(painter, QRect(rect.right() + 1 - side, rect.bottom() + 1 - side, side, side), Qt::AlignCenter,
                  mode, QIcon::Off);
  }

  QPixmap pixmap(const QSize& size, QIcon::Mode mode, QIcon::State state) override {
    QPixmap pixmap(size);
    pixmap.fill(Qt::transparent);
    QPainter painter(&pixmap);
    paint(&painter, QRect(QPoint(), size), mode, state);
    return pixmap;
  }

  QIconEngine* clone() const override { return new LinkedIconEngine(m_icon, m_badge); }

private:
  QIcon m_icon;
  QIcon m_badge;
};

// Draws a row: the style's item with the light bulb in its decoration,
// and the radio button right after the name, whose text is elided early
// enough to leave room for it.
class BrowserDelegate : public TreeItemDelegate {
public:
  // Where a row's parts are, in the view's coordinates.
  struct Parts {
    QRect name;  // the name's text as drawn
    QRect eye;   // the light bulb (empty without one)
    QRect radio; // the radio button (empty without one)
    QRect fx;    // the fx badge of a bound row (mitcad#862; empty without one)
    QRect make;  // the Make tab's marker (mitcad#1049; empty without one)
  };

  using TreeItemDelegate::TreeItemDelegate;

  // The parts of a row whose option the view gave (its rect, font, state).
  Parts parts(const QStyleOptionViewItem& option, const QModelIndex& index) const {
    QStyleOptionViewItem opt = option;
    initStyleOption(&opt, index);
    return layout(opt, index);
  }

  QRect nameTextRect(const QStyleOptionViewItem& option, const QModelIndex& index) const override {
    QStyleOptionViewItem opt = option;
    initStyleOption(&opt, index);
    const Parts at = layout(opt, index);
    return QRect(at.name.left(), opt.rect.top() + (opt.rect.height() - opt.fontMetrics.height()) / 2,
                 at.name.width(), opt.fontMetrics.height());
  }

  void paint(QPainter* painter, const QStyleOptionViewItem& option, const QModelIndex& index) const override {
    QStyleOptionViewItem opt = option;
    initStyleOption(&opt, index);
    const Parts at = layout(opt, index);
    hideEditedText(opt, index);
    style(opt)->drawControl(QStyle::CE_ItemViewItem, &opt, painter, opt.widget);
    if (!editing(index) && !at.radio.isEmpty()) {
      const bool active = index.data(kRadioRole).toBool();
      themeIcon(active ? QStringLiteral("radio-on") : QStringLiteral("radio-off"))
          .paint(painter, at.radio, Qt::AlignCenter,
                 opt.state & QStyle::State_Selected ? QIcon::Selected : QIcon::Normal);
    }
    // The fx badge (mitcad#862): a solid pill with the count of the
    // properties that follow expressions.
    if (!editing(index) && !at.fx.isEmpty()) {
      const BindingsColors colors = bindingsColors(opt.palette);
      painter->save();
      painter->setRenderHint(QPainter::Antialiasing);
      painter->setPen(QPen(colors.text, 1.0));
      painter->setBrush(colors.fill);
      const QRectF pill = QRectF(at.fx).adjusted(0.5, 0.5, -0.5, -0.5);
      painter->drawRoundedRect(pill, pill.height() / 2, pill.height() / 2);
      painter->setFont(bindingsFont(opt.font));
      painter->setPen(colors.text);
      painter->drawText(at.fx, Qt::AlignCenter, bindingsText(index.data(kBindingsRole).toInt()));
      painter->restore();
    }
    // The Make tab's marker (mitcad#1049): a small dot in the accent
    // colour after the name.
    if (!editing(index) && !at.make.isEmpty()) {
      painter->save();
      painter->setRenderHint(QPainter::Antialiasing);
      painter->setPen(Qt::NoPen);
      painter->setBrush(accentColor(opt.palette));
      painter->drawEllipse(QRectF(at.make).adjusted(0.5, 0.5, -0.5, -0.5));
      painter->restore();
    }
  }

  QSize sizeHint(const QStyleOptionViewItem& option, const QModelIndex& index) const override {
    QSize size = QStyledItemDelegate::sizeHint(option, index);
    if (index.data(kRadioRole).isValid()) {
      size.rwidth() += kGap + option.decorationSize.height();
    }
    if (const int count = index.data(kBindingsRole).toInt(); count > 0) {
      size.rwidth() += kGap + bindingsWidth(option.font, count);
    }
    if (!index.data(kMakeRole).toString().isEmpty()) {
      size.rwidth() += kGap + makeMarkSide(option.decorationSize.height());
    }
    return size;
  }

  bool helpEvent(QHelpEvent* event, QAbstractItemView* view, const QStyleOptionViewItem& option,
                 const QModelIndex& index) override {
    if (event->type() == QEvent::ToolTip && index.isValid()) {
      const Parts at = parts(option, index);
      QString tip;
      if (at.eye.contains(event->pos())) {
        // A conditioned row's bulb says what a click does to it (mitcad#863).
        tip = index.data(kEyeTipRole).toString();
        if (tip.isEmpty()) {
          tip = index.data(kEyeRole).toBool() ? BrowserPanel::tr("Hide") : BrowserPanel::tr("Show");
        }
      } else if (at.radio.contains(event->pos())) {
        tip = BrowserPanel::tr("Activate");
      } else if (at.make.contains(event->pos())) {
        tip = index.data(kMakeRole).toString(); // mitcad#1049
      }
      if (!tip.isEmpty()) {
        QToolTip::showText(event->globalPos(), tip, view);
        return true;
      }
    }
    return QStyledItemDelegate::helpEvent(event, view, option, index);
  }

protected:
  QRect editorRect(const QStyleOptionViewItem& option, const QModelIndex& index) const override {
    const Parts at = parts(option, index);
    // Match the custom bulb/icon/name layout, rather than the style's
    // default item editor rectangle. The radio returns when editing ends.
    return QRect(QPoint(at.name.left() - 2, option.rect.top() + 1),
                 QPoint(option.rect.right() - 2, option.rect.bottom() - 1));
  }

  void initStyleOption(QStyleOptionViewItem* option, const QModelIndex& index) const override {
    QStyledItemDelegate::initStyleOption(option, index);
    // A hidden, failed or warned row's name (mitcad#1104): its role's colour
    // in the theme the row is painted in now.
    if (const QVariant role = index.data(kTextRole); role.isValid()) {
      option->palette.setBrush(QPalette::Text, nameColor(static_cast<ColorRole>(role.toInt()), option->widget));
    }
    const QVariant eye = index.data(kEyeRole);
    if (eye.isValid()) {
      const int side = option->decorationSize.height();
      // Two states, shown or hidden; shown and transparent (mitcad#1093) is
      // the half-open eye.
      const QString bulb = !eye.toBool()                                ? QStringLiteral("eye-off")
                           : index.data(kEyeTransparentRole).toBool() ? QStringLiteral("eye-half")
                                                                       : QStringLiteral("eye");
      auto* engine = new EyeIconEngine(themeIcon(bulb), option->icon, bulb);
      // A row that follows a condition: the fx corner badge (mitcad#863).
      if (index.data(kEyeBoundRole).toBool()) {
        const QColor accent = accentColor(option->palette);
        const QColor base = option->palette.color(QPalette::Base);
        engine->setBadge(accent, base, base.lightness() > 128 ? accent.darker(160) : accent.lighter(160));
      }
      option->icon = QIcon(engine);
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
    const int bound = index.data(kBindingsRole).toInt();
    const int badge = bound > 0 ? bindingsWidth(opt.font, bound) : 0;
    const int mark = index.data(kMakeRole).toString().isEmpty() ? 0 : makeMarkSide(side);
    const int room = qMax(0, text.width() - 2 * margin - (radio ? kGap + side : 0) - (badge > 0 ? kGap + badge : 0) -
                                 (mark > 0 ? kGap + mark : 0));
    if (opt.fontMetrics.horizontalAdvance(opt.text) > room) {
      opt.text = opt.fontMetrics.elidedText(opt.text, Qt::ElideRight, room);
    }
    const int width = qMin(opt.fontMetrics.horizontalAdvance(opt.text), room);
    parts.name = QRect(text.left() + margin, opt.rect.top(), width, opt.rect.height());
    if (radio) {
      parts.radio = QRect(parts.name.right() + 1 + kGap, opt.rect.top() + (opt.rect.height() - side) / 2,
                          side, side);
    }
    int right = radio ? parts.radio.right() : parts.name.right();
    if (mark > 0) {
      parts.make = QRect(right + 1 + kGap, opt.rect.top() + (opt.rect.height() - mark) / 2, mark, mark);
      right = parts.make.right();
    }
    if (badge > 0) {
      const int height = std::min(opt.rect.height() - 4, QFontMetrics(bindingsFont(opt.font)).height() + 3);
      const int left = right + 1 + kGap;
      parts.fx = QRect(left, opt.rect.top() + (opt.rect.height() - height) / 2, badge, height);
    }
    return parts;
  }
};

// The rows of the tree in a floating card: like a macOS sidebar, a selected
// row is a rounded band of the accent colour (translucent) behind the whole
// row, with its text in semibold, instead of the style's highlight (the
// tree's palette makes that one transparent).
// With the Rows background (mitcad#419) the row's box shows the selection
// and the pointer instead (BrowserTree::drawRow).
class SidebarDelegate : public BrowserDelegate {
public:
  using BrowserDelegate::BrowserDelegate;

  void setRows(bool rows) { m_rows = rows; }

  void paint(QPainter* painter, const QStyleOptionViewItem& option, const QModelIndex& index) const override {
    if (!m_rows && option.state.testFlag(QStyle::State_Selected)) {
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

protected:
  void initStyleOption(QStyleOptionViewItem* option, const QModelIndex& index) const override {
    BrowserDelegate::initStyleOption(option, index);
    // The band is the highlight; the name in semibold.
    if (option->state.testFlag(QStyle::State_Selected)) {
      option->state &= ~QStyle::State_Selected;
      option->font.setWeight(QFont::DemiBold);
      option->fontMetrics = QFontMetrics(option->font);
    }
    if (m_rows) {
      option->state &= ~QStyle::State_MouseOver; // the box shows it
    }
  }

private:
  bool m_rows = false;
};

// The rows of the tree in a Docked panel (mitcad#251): at least the panels'
// row height, a selected row a rounded band of the accent colour behind the
// whole row (WindowStyle) instead of the style's highlight.
class PanelDelegate : public BrowserDelegate {
public:
  using BrowserDelegate::BrowserDelegate;

  void paint(QPainter* painter, const QStyleOptionViewItem& option, const QModelIndex& index) const override {
    paintPanelRowBand(painter, option, index);
    BrowserDelegate::paint(painter, panelRowOption(option), index);
  }

  QSize sizeHint(const QStyleOptionViewItem& option, const QModelIndex& index) const override {
    QSize size = BrowserDelegate::sizeHint(option, index);
    size.setHeight(std::max(size.height(), panel::kRowHeight));
    return size;
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
  // A joint origin's datum is a plane with its frame (mitcad#55).
  if (type == QStringLiteral("construction_plane") || type == QStringLiteral("joint_origin") || type.size() == 2) {
    return SelectKind::Plane;
  }
  if (type == QStringLiteral("construction_axis") || type.size() == 1) {
    return SelectKind::Axis;
  }
  // An Intersection Curve's row picks the feature, as its curve in the view
  // does (mitcad#1405).
  if (type == QStringLiteral("intersection_curve")) {
    return SelectKind::Feature;
  }
  return SelectKind::Point;
}

QString itemKey(const SelectionItem& item) {
  return QStringLiteral("%1|%2|%3")
      .arg(static_cast<unsigned>(item.kind))
      .arg(item.owner, item.occurrence);
}

// A body's print bed face in its row's tooltip (mitcad#731): "end face of
// Extrude1" by the feature and role of its name ("a face of Base1" for
// roles without a plain word), or why it cannot lie on the bed: "gone" and
// "not planar" for the model's own two reasons (document/print_bed.rs),
// "not printed" for a surface body's, else the model's reason as it is
// (mitcad#737), such as a renamed face whose plane is not found.
QString printBedText(const DocumentSnapshot& snapshot, const DocumentSnapshot::Body& body) {
  const QString& error = body.printBedError;
  if (error == QLatin1String("the print bed face is gone")) {
    return QObject::tr("gone");
  }
  if (error == QLatin1String("the print bed face is not planar")) {
    return QObject::tr("not planar");
  }
  if (error == QLatin1String("a surface body is not printed")) {
    return QObject::tr("not used, a surface body is not printed");
  }
  if (!error.isEmpty()) {
    return error;
  }
  const QString face = body.printBedFaces.value(0, body.printBedFace);
  const QString feature = face.section(QLatin1Char(':'), 0, 0);
  QString role = face.section(QLatin1Char(':'), 1).section(QLatin1Char('('), 0, 0).section(QLatin1Char('#'), 0, 0);
  // A box's sides are side0 to side3.
  while (role.startsWith(QStringLiteral("side")) && !role.isEmpty() && role.back().isDigit()) {
    role.chop(1);
  }
  const DocumentSnapshot::Feature* made = snapshot.feature(feature);
  const QString name = made != nullptr ? made->name : feature;
  if (role == QStringLiteral("end")) {
    return QObject::tr("end face of %1").arg(name);
  }
  if (role == QStringLiteral("start")) {
    return QObject::tr("start face of %1").arg(name);
  }
  if (role == QStringLiteral("side")) {
    return QObject::tr("side face of %1").arg(name);
  }
  if (role == QStringLiteral("top")) {
    return QObject::tr("top face of %1").arg(name);
  }
  if (role == QStringLiteral("bottom")) {
    return QObject::tr("bottom face of %1").arg(name);
  }
  return QObject::tr("a face of %1").arg(name);
}

// A joint's row (mitcad#55): its kind, whether it holds and its values
// ("Joint1: revolute, placed, rz 30 deg"); a rigid group's members.
QString jointTooltip(const DocumentSnapshot& snapshot, const DocumentSnapshot::Feature& feature) {
  if (feature.type == QStringLiteral("constraint_set")) {
    for (const QJsonValue& value : snapshot.joints.value(QStringLiteral("constraints")).toArray()) {
      const QJsonObject set = value.toObject();
      if (set.value(QStringLiteral("uid")).toString() == feature.uid) {
        return QObject::tr("%1: %n row(s), %2", nullptr, set.value(QStringLiteral("rows")).toArray().size())
            .arg(feature.name, set.value(QStringLiteral("state")).toString());
      }
    }
    return feature.name;
  }
  // A motion link (mitcad#466): "MotionLink1: gear, Joint1 rz to Joint2 rz,
  // placed".
  if (feature.isMotionLink()) {
    for (const QJsonValue& value : snapshot.joints.value(QStringLiteral("links")).toArray()) {
      const QJsonObject link = value.toObject();
      if (link.value(QStringLiteral("uid")).toString() == feature.uid) {
        const auto side = [&link](const char* key) {
          const QJsonObject s = link.value(QLatin1String(key)).toObject();
          return QStringLiteral("%1 %2").arg(s.value(QStringLiteral("name")).toString(),
                                             s.value(QStringLiteral("motion")).toString());
        };
        return QObject::tr("%1: %2, %3 to %4, %5")
            .arg(feature.name, link.value(QStringLiteral("coupling")).toString(), side("a"), side("b"),
                 link.value(QStringLiteral("state")).toString());
      }
    }
    return feature.name;
  }
  if (feature.type == QStringLiteral("rigid_group")) {
    for (const QJsonValue& value : snapshot.joints.value(QStringLiteral("rigid_groups")).toArray()) {
      const QJsonObject group = value.toObject();
      if (group.value(QStringLiteral("uid")).toString() == feature.uid) {
        return QObject::tr("%1: moves %2 as one")
            .arg(feature.name, group.value(QStringLiteral("names")).toVariant().toStringList().join(QStringLiteral(", ")));
      }
    }
    return feature.name;
  }
  const QJsonObject joint = snapshot.joint(feature.uid);
  if (joint.isEmpty()) {
    return feature.name;
  }
  QStringList parts{joint.value(QStringLiteral("kind")).toString(), joint.value(QStringLiteral("state")).toString()};
  const QJsonObject values = joint.value(QStringLiteral("values")).toObject();
  for (auto it = values.begin(); it != values.end(); ++it) {
    const bool turn = it.key().startsWith(QLatin1Char('r'));
    const double value = turn ? it.value().toDouble() * 57.29577951308232 : it.value().toDouble();
    parts << QStringLiteral("%1 %2 %3")
                 .arg(it.key(), QString::number(std::round(value * 1e4) / 1e4, 'g', 10),
                      turn ? QStringLiteral("deg") : QStringLiteral("mm"));
  }
  const QString message = joint.value(QStringLiteral("message")).toString();
  if (!message.isEmpty()) {
    parts << message;
  }
  return QStringLiteral("%1: %2").arg(feature.name, parts.join(QStringLiteral(", ")));
}


} // namespace

SelectionItem BrowserNode::item() const {
  SelectionItem item;
  item.occurrence = occurrence;
  item.owner = uid;
  switch (type) {
  case Type::Body:
    item.kind = SelectKind::Body;
    if (mesh) {
      item.geometry = QStringLiteral("mesh"); // as the view's picks have it
    }
    break;
  case Type::Sketch:
    item.kind = SelectKind::Sketch;
    break;
  case Type::Datum:
  case Type::OriginDatum:
    item.kind = datumKind(type == Type::Datum ? featureType : uid);
    // An Intersection Curve's row picks its curve (mitcad#1417), as the view
    // does.
    item.geometry = item.kind == SelectKind::Feature ? featureCurveGeometry() : SelectionItem::kindName(item.kind);
    break;
  case Type::Component:
    if (occurrence.isEmpty()) {
      return SelectionItem(); // the root is everything
    }
    item.kind = SelectKind::Component;
    break;
  case Type::Joint:
    // A feature, as the timeline gives it (Drive Joint takes it).
    item.kind = SelectKind::Feature;
    break;
  case Type::SheetRules:
    item.kind = SelectKind::Feature;
    break;
  case Type::FlatPattern:
    item.kind = SelectKind::Body;
    break;
  default:
    return SelectionItem();
  }
  return item;
}

// The tree: clicks on a light bulb or a radio button do not select the row,
// F2 renames and Delete deletes. A slow second click on the name of the row
// that is selected and current renames it too (mitcad#177): when the
// double-click interval has passed without a second click, so that a
// double-click keeps its own action, and not after a drag.
class BrowserTree : public QTreeWidget {
public:
  explicit BrowserTree(BrowserPanel& panel)
      : QTreeWidget(&panel), m_panel(panel),
        m_delegate(chromeStyle() == ChromeStyle::Floating ? static_cast<BrowserDelegate*>(new SidebarDelegate(this))
                                                          : static_cast<BrowserDelegate*>(new PanelDelegate(this))) {
    setItemDelegate(m_delegate);
    if (chromeStyle() == ChromeStyle::Floating) {
      m_sidebar = static_cast<SidebarDelegate*>(m_delegate);
    }
    m_renameTimer.setSingleShot(true);
    QObject::connect(&m_renameTimer, &QTimer::timeout, this, [this] { emit m_panel.renameClicked(m_renameNode); });
    TestSync::watch(&m_renameTimer); // UI tests wait for the rename to start
  }

  QSize minimumSizeHint() const override { return dockedTreeMinimumSize(this, QTreeWidget::minimumSizeHint()); }

  // Forgets a slow second click that has not yet started a rename.
  void cancelSlowClick() {
    m_renameTimer.stop();
    m_renamePress = false;
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

  // The Rows background (mitcad#419): each row on a box of its own.
  void setRows(bool rows) {
    m_rows = rows;
    if (m_sidebar != nullptr) {
      m_sidebar->setRows(rows);
    }
    // The pointer's row is followed without a button (setHover).
    if (rows) {
      m_tracking = viewport()->hasMouseTracking();
      viewport()->setMouseTracking(true);
    } else {
      viewport()->setMouseTracking(m_tracking);
    }
    setHover(QModelIndex());
    viewport()->update();
  }

  // The row under the pointer, whose box is tinted.
  void setHover(const QModelIndex& index) {
    if (m_hover == index) {
      return;
    }
    m_hover = index;
    if (m_rows) {
      viewport()->update();
    }
  }

  // A row's box with the Rows background, in the viewport's coordinates:
  // from its arrow's place before it to the end of its name or radio
  // button, a little shorter than the row so that the view shows between
  // rows. Empty for a row not laid out.
  QRect rowBox(const QModelIndex& index) const {
    const QRect cell = visualRect(index);
    if (cell.isEmpty()) {
      return QRect();
    }
    QStyleOptionViewItem option;
    initViewItemOption(&option);
    option.rect = cell;
    if (selectionModel() != nullptr && selectionModel()->isSelected(index)) {
      option.state |= QStyle::State_Selected; // its name in semibold is wider
    }
    const BrowserDelegate::Parts parts = m_delegate->parts(option, index);
    int end = std::max(parts.name.right(), parts.radio.isEmpty() ? parts.name.right() : parts.radio.right());
    if (!parts.fx.isEmpty()) {
      end = std::max(end, parts.fx.right()); // the fx badge (mitcad#862)
    }
    if (!parts.make.isEmpty()) {
      end = std::max(end, parts.make.right()); // the Make tab's marker (mitcad#1049)
    }
    const int left = cell.left() - indentation() + 2;
    const int right = std::min(end + kBoxPadding, viewport()->width() - 1);
    return QRect(QPoint(left, cell.top() + 2), QPoint(std::max(left, right), cell.bottom() - 2));
  }

protected:
  void drawRow(QPainter* painter, const QStyleOptionViewItem& option, const QModelIndex& index) const override {
    if (m_rows) {
      paintRowBox(painter, option, index);
    }
    QTreeWidget::drawRow(painter, option, index);
  }

  // A row's fx badge has its bindings card (mitcad#862,
  // BrowserPanel::hoverBindings), not the row's tooltip.
  bool viewportEvent(QEvent* event) override {
    if (event->type() == QEvent::ToolTip) {
      const QPoint at = static_cast<QHelpEvent*>(event)->pos();
      if (QTreeWidgetItem* item = itemAt(at); item != nullptr) {
        const BrowserDelegate::Parts parts = partsOf(item);
        if (!parts.fx.isEmpty() && parts.fx.contains(at)) {
          QToolTip::hideText();
          return true;
        }
      }
    }
    return QTreeWidget::viewportEvent(event);
  }

  void mousePressEvent(QMouseEvent* event) override {
    cancelSlowClick();
    const QPoint at = event->position().toPoint();
    m_partPress = false;
    if (event->button() == Qt::LeftButton && clickPart(at, false)) {
      // The press is consumed here, so its moves and its release must not
      // reach the view either: its pressed index would still be an earlier
      // row, and a small move or the release would select a range from it
      // (mitcad#344).
      m_partPress = true;
      event->accept();
      return;
    }
    // A press on the name of the row already selected and current, without
    // modifiers: perhaps the slow second click that renames it.
    QTreeWidgetItem* item = itemAt(at);
    m_renamePress = event->button() == Qt::LeftButton && event->modifiers() == Qt::NoModifier && item != nullptr &&
                    item == currentItem() && item->isSelected() && (item->flags() & Qt::ItemIsEditable) &&
                    state() != QAbstractItemView::EditingState && partsOf(item).name.contains(at);
    m_pressAt = at;
    QTreeWidget::mousePressEvent(event);
  }

  void mouseMoveEvent(QMouseEvent* event) override {
    if (m_partPress) {
      event->accept();
      return;
    }
    QTreeWidget::mouseMoveEvent(event);
  }

  void mouseReleaseEvent(QMouseEvent* event) override {
    if (m_partPress) {
      if (event->button() == Qt::LeftButton) {
        m_partPress = false;
      }
      event->accept();
      return;
    }
    const bool slowClick = m_renamePress && event->button() == Qt::LeftButton;
    m_renamePress = false;
    QTreeWidget::mouseReleaseEvent(event);
    const QPoint at = event->position().toPoint();
    QTreeWidgetItem* item = itemAt(at);
    if (slowClick && item != nullptr && item == currentItem() &&
        (at - m_pressAt).manhattanLength() < QApplication::startDragDistance()) {
      // Renamed once the time for a double-click has passed.
      m_renameNode = m_panel.nodeOf(item);
      m_renameTimer.start(QApplication::doubleClickInterval());
    }
  }

  void mouseDoubleClickEvent(QMouseEvent* event) override {
    // A double-click keeps its own action: no slow second click.
    cancelSlowClick();
    // A quick second click on a light bulb is a click of its own.
    if (event->button() == Qt::LeftButton && clickPart(event->position().toPoint(), true)) {
      m_partPress = true;
      event->accept();
      return;
    }
    QTreeWidgetItem* item = itemAt(event->position().toPoint());
    if (item != nullptr && event->button() == Qt::LeftButton) {
      const BrowserNode node = m_panel.nodeOf(item);
      // The Annotations folder opens its window (mitcad#702).
      if ((!node.isFolder() || node.type == BrowserNode::Type::Measurements) &&
          node.type != BrowserNode::Type::Settings &&
          node.type != BrowserNode::Type::NamedViews) {
        emit m_panel.doubleClicked(node);
        return;
      }
    }
    QTreeWidget::mouseDoubleClickEvent(event);
  }

  void keyPressEvent(QKeyEvent* event) override {
    cancelSlowClick();
    if (event->matches(QKeySequence::Find) && state() != QAbstractItemView::EditingState) {
      emit m_panel.searchRequested(); // the timeline's search (mitcad#179)
      return;
    }
    QTreeWidgetItem* item = currentItem();
    if (item != nullptr && state() != QAbstractItemView::EditingState) {
      if (event->key() == Qt::Key_F2) {
        m_panel.startRename(m_panel.nodeOf(item));
        return;
      }
      if (isDeleteKey(event->key())) {
        emit m_panel.deleteRequested(m_panel.selectedNodes());
        return;
      }
    }
    QTreeWidget::keyPressEvent(event);
  }

private:
  // A row's box (rowBox): the card's material, tinted with the accent colour
  // when the row is selected (less when the window is not active, as the
  // sidebar's band) and lightly when the pointer is on it. Under the
  // row's arrow, which the tree draws after it.
  void paintRowBox(QPainter* painter, const QStyleOptionViewItem& option, const QModelIndex& index) const {
    const QRect box = rowBox(index);
    if (box.isEmpty()) {
      return;
    }
    const QPalette& colors = palette();
    const qreal hairline = 1.0 / devicePixelRatioF();
    const QRectF outline = QRectF(box).adjusted(hairline / 2, hairline / 2, -hairline / 2, -hairline / 2);
    const qreal radius = GlassCard::kBoxRadius;
    painter->save();
    painter->setRenderHint(QPainter::Antialiasing);
    painter->setPen(QPen(cardBorderColor(colors), hairline));
    painter->setBrush(cardFillColor(colors));
    painter->drawRoundedRect(outline, radius, radius);
    QColor tint;
    if (selectionModel() != nullptr && selectionModel()->isSelected(index)) {
      tint = accentColor(colors);
      tint.setAlphaF(option.state.testFlag(QStyle::State_Active) ? 0.30f : 0.16f);
    } else if (m_hover == index) {
      tint = colors.color(QPalette::Text);
      tint.setAlphaF(0.08f);
    }
    if (tint.isValid()) {
      painter->setPen(Qt::NoPen);
      painter->setBrush(tint);
      painter->drawRoundedRect(outline, radius, radius);
    }
    painter->restore();
  }

  // Clicks the light bulb or the radio button at a point; false when there
  // is none.
  bool clickPart(const QPoint& at, bool doubleClick) {
    QTreeWidgetItem* item = itemAt(at);
    if (item == nullptr) {
      return false;
    }
    const BrowserNode node = m_panel.nodeOf(item);
    const BrowserDelegate::Parts parts = partsOf(item);
    if (node.hasEye && parts.eye.contains(at)) {
      qDebug().noquote() << QStringLiteral("Eye click %1 (%2)")
                                .arg(node.path.isEmpty() ? node.name : node.path,
                                     doubleClick ? QStringLiteral("double-click") : QStringLiteral("press"));
      emit m_panel.eyeClicked(node);
      return true;
    }
    if (node.type == BrowserNode::Type::Component && parts.radio.contains(at)) {
      emit m_panel.activateClicked(node);
      return true;
    }
    // The fx badge pins the row's bindings card (mitcad#862).
    if (!parts.fx.isEmpty() && parts.fx.contains(at)) {
      qDebug().noquote() << QStringLiteral("Browser fx click %1").arg(node.path);
      emit m_panel.bindingsRequested(node, globalRect(parts.fx), true);
      return true;
    }
    return false;
  }

  // A rectangle of the viewport in global coordinates.
  QRect globalRect(const QRect& rect) const { return QRect(viewport()->mapToGlobal(rect.topLeft()), rect.size()); }

  BrowserPanel& m_panel;
  BrowserDelegate* m_delegate;
  SidebarDelegate* m_sidebar = nullptr; // the Floating layout's
  // The Rows background (mitcad#419): on, the row under the pointer, and
  // whether the viewport tracked the mouse before.
  bool m_rows = false;
  QPersistentModelIndex m_hover;
  bool m_tracking = false;
  // A press (or double-click) consumed by a light bulb or radio button: its
  // moves and release are consumed too (mitcad#344).
  bool m_partPress = false;
  // A slow second click (mitcad#177): the press that may be one, where it
  // was, and the row it renames when the timer fires.
  bool m_renamePress = false;
  QPoint m_pressAt;
  QTimer m_renameTimer;
  BrowserNode m_renameNode;
};

BrowserPanel::BrowserPanel(QWidget* parent) : QWidget(parent), m_tree(new BrowserTree(*this)) {
  m_tree->setObjectName(QStringLiteral("modelBrowserTree"));
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
  panel::setCompactIndentation(m_tree, panel::Indent::Tree);
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
  } else {
    // On the Docked panel's surface, rows as PanelDelegate paints them
    // (mitcad#251).
    stylePanelTree(m_tree);
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
  // The places are logged again when the rows scroll (they scroll to a
  // selected row, with the wheel, with the keys), and when the panel moves in
  // the window (a banner or another panel makes room above or beside it).
  connect(m_tree->verticalScrollBar(), &QScrollBar::valueChanged, this,
          [this] { TestSync::singleShot(0, this, [this] { logLayout(); }); });
  new PlaceWatcher(this, [this] { logLayout(); });
  connect(m_tree, &QTreeWidget::itemExpanded, this,
          [remember](QTreeWidgetItem* item) { remember(item, true); });
  connect(m_tree, &QTreeWidget::itemCollapsed, this,
          [remember](QTreeWidgetItem* item) { remember(item, false); });

  // With the Rows background (mitcad#419), where the rows' boxes are
  // follows every change of the rows in sight.
  const auto rowsMoved = [this] { queueRowsRegion(); };
  for (QScrollBar* bar : {m_tree->verticalScrollBar(), m_tree->horizontalScrollBar()}) {
    connect(bar, &QScrollBar::valueChanged, this, rowsMoved);
    connect(bar, &QScrollBar::rangeChanged, this, rowsMoved);
    bar->installEventFilter(this); // shown and hidden
  }
  connect(m_tree, &QTreeWidget::itemExpanded, this, rowsMoved);
  connect(m_tree, &QTreeWidget::itemCollapsed, this, rowsMoved);
  connect(m_tree, &QTreeWidget::itemSelectionChanged, this, rowsMoved);
  QAbstractItemModel* model = m_tree->model();
  connect(model, &QAbstractItemModel::rowsInserted, this, rowsMoved);
  connect(model, &QAbstractItemModel::rowsRemoved, this, rowsMoved);
  connect(model, &QAbstractItemModel::modelReset, this, rowsMoved);
  connect(model, &QAbstractItemModel::layoutChanged, this, rowsMoved);
  connect(model, &QAbstractItemModel::dataChanged, this, rowsMoved);
  // Its size, a rename editor opened and closed, and the pointer's row
  // (followed without a button: the joint rows' preview, mitcad#523).
  m_tree->viewport()->installEventFilter(this);
  m_tree->viewport()->setMouseTracking(true);
  // The pointer resting on an fx badge shows its card (mitcad#862).
  m_bindingsTimer = new QTimer(this);
  m_bindingsTimer->setSingleShot(true);
  m_bindingsTimer->setInterval(450);
  connect(m_bindingsTimer, &QTimer::timeout, this, &BrowserPanel::bindingsRested);
  TestSync::watch(m_bindingsTimer); // UI tests wait for the card
}

void BrowserPanel::hoverBindings(QTreeWidgetItem* item, const QPoint& at) {
  QString key;
  if (item != nullptr) {
    const BrowserDelegate::Parts parts = m_tree->partsOf(item);
    if (!parts.fx.isEmpty() && parts.fx.contains(at)) {
      key = keyOf(nodeOf(item));
    }
  }
  if (key == m_bindingsHover) {
    return;
  }
  m_bindingsHover = key;
  if (key.isEmpty()) {
    m_bindingsTimer->stop();
  } else {
    m_bindingsTimer->start();
  }
}

void BrowserPanel::bindingsRested() {
  const QPoint at = m_tree->viewport()->mapFromGlobal(QCursor::pos());
  QTreeWidgetItem* item = m_tree->itemAt(at);
  if (item == nullptr || m_bindingsHover.isEmpty() || keyOf(nodeOf(item)) != m_bindingsHover) {
    return;
  }
  const BrowserDelegate::Parts parts = m_tree->partsOf(item);
  if (parts.fx.isEmpty() || !parts.fx.contains(at)) {
    return;
  }
  emit bindingsRequested(nodeOf(item), QRect(m_tree->viewport()->mapToGlobal(parts.fx.topLeft()), parts.fx.size()),
                         false);
}

void BrowserPanel::setRowsBackground(bool rows) {
  if (rows == m_rows) {
    return;
  }
  m_rows = rows;
  m_tree->setRows(rows);
  if (m_topBar != nullptr) {
    // No card behind it: a box of the window's colour.
    m_topBar->setAutoFillBackground(rows);
  }
  queueRowsRegion();
  // The place beside the rows is logged, or logged out of sight.
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

QRegion BrowserPanel::rowsRegion() const {
  QRegion region;
  const QWidget* viewport = m_tree->viewport();
  const QRect inside = viewport->rect();
  const QPoint origin = viewport->mapTo(this, QPoint(0, 0));
  for (QModelIndex index = m_tree->indexAt(QPoint(0, 0)); index.isValid(); index = m_tree->indexBelow(index)) {
    const QRect row = m_tree->visualRect(index);
    if (row.top() > inside.bottom()) {
      break;
    }
    region += m_tree->rowBox(index).intersected(inside).translated(origin);
  }
  for (const QScrollBar* bar : {m_tree->verticalScrollBar(), m_tree->horizontalScrollBar(), m_leftBar}) {
    if (bar != nullptr && bar->isVisible()) {
      region += QRect(bar->mapTo(this, QPoint(0, 0)), bar->size());
    }
  }
  // A rename editor, as wide as the row.
  for (const QWidget* child : viewport->findChildren<QWidget*>(Qt::FindDirectChildrenOnly)) {
    if (child->isVisible()) {
      region += child->geometry().translated(origin);
    }
  }
  // The bar above the rows (mitcad#807), on its own box.
  if (m_topBar != nullptr && m_topBar->isVisible()) {
    region += m_topBar->geometry();
  }
  return region;
}

void BrowserPanel::setTopBar(QWidget* bar) {
  m_topBar = bar;
  static_cast<QVBoxLayout*>(layout())->insertWidget(0, bar);
  // Shown, hidden or resized: the card's height and the Rows background's
  // boxes follow it.
  bar->installEventFilter(this);
  bar->setAutoFillBackground(m_rows);
}

void BrowserPanel::queueRowsRegion() {
  if (!m_rows || m_rowsQueued) {
    return;
  }
  m_rowsQueued = true;
  // After the tree has laid out the change (its rows' layout is delayed).
  TestSync::singleShot(0, this, [this] {
    m_rowsQueued = false;
    if (m_rows) {
      emit rowsRegionChanged();
      logLayout(); // the place beside the rows may have moved
    }
  });
}

bool BrowserPanel::eventFilter(QObject* watched, QEvent* event) {
  if (m_topBar != nullptr && watched == m_topBar) {
    switch (event->type()) {
    case QEvent::Show:
    case QEvent::Hide:
    case QEvent::LayoutRequest: // its height changes (a notice shown)
      emit contentHeightChanged();
      queueRowsRegion();
      TestSync::singleShot(0, this, [this] { logLayout(); }); // the rows moved
      break;
    case QEvent::Resize:
      queueRowsRegion();
      break;
    default:
      break;
    }
    return QWidget::eventFilter(watched, event);
  }
  if (watched == m_tree->viewport()) {
    switch (event->type()) {
    case QEvent::Resize:
      queueFit(); // a design opened before the panel had its size fits now
      queueRowsRegion();
      break;
    case QEvent::Show:
      queueFit();
      break;
    case QEvent::ChildAdded:
    case QEvent::ChildRemoved:
      queueRowsRegion();
      break;
    case QEvent::MouseMove: {
      const QPoint at = static_cast<QMouseEvent*>(event)->position().toPoint();
      if (m_rows) {
        m_tree->setHover(m_tree->indexAt(at));
      }
      hoverJoint(nodeOf(m_tree->itemAt(at)));
      hoverBodies(nodeOf(m_tree->itemAt(at))); // mitcad#1022
      hoverFeature(nodeOf(m_tree->itemAt(at))); // mitcad#1327
      hoverBindings(m_tree->itemAt(at), at); // mitcad#862
      break;
    }
    case QEvent::Leave:
      m_tree->setHover(QModelIndex());
      hoverJoint(BrowserNode());
      hoverBodies(BrowserNode());
      hoverFeature(BrowserNode());
      hoverBindings(nullptr, QPoint());
      break;
    case QEvent::MouseButtonPress:
      // A click on the badge pins the card instead (mitcad#862).
      m_bindingsTimer->stop();
      m_bindingsHover.clear();
      break;
    default:
      break;
    }
  } else if (event->type() == QEvent::Show || event->type() == QEvent::Hide) {
    queueRowsRegion(); // a scroll bar
  }
  return QWidget::eventFilter(watched, event);
}

QSize BrowserPanel::sizeHint() const { return QSize(kDefaultWidth, QWidget::sizeHint().height()); }

BrowserNode BrowserPanel::nodeOf(const QTreeWidgetItem* item) const {
  return m_nodes.value(const_cast<QTreeWidgetItem*>(item));
}

void BrowserPanel::hoverNode(const BrowserNode& node) {
  hoverJoint(node);
  hoverBodies(node);
  hoverFeature(node);
}

void BrowserPanel::hoverFeature(const BrowserNode& node) {
  // A feature's, a sketch's or a construction feature's row: the places
  // its notices name (mitcad#1327).
  const bool feature = node.type == BrowserNode::Type::Feature || node.type == BrowserNode::Type::Sketch ||
                       node.type == BrowserNode::Type::Datum;
  const QString uid = feature ? node.uid : QString();
  if (uid == m_hoveredFeature) {
    return;
  }
  m_hoveredFeature = uid;
  emit featureHovered(uid);
}

void BrowserPanel::hoverJoint(const BrowserNode& node) {
  QString uid;
  QString occurrence;
  // Joints, as-built joints and constraint sets: what the view highlights.
  if (node.type == BrowserNode::Type::Joint &&
      (node.featureType == QStringLiteral("joint") || node.featureType == QStringLiteral("as_built_joint") ||
       node.featureType == QStringLiteral("constraint_set"))) {
    uid = node.uid;
    occurrence = node.occurrence;
  }
  const QString key = uid.isEmpty() ? QString() : uid + QLatin1Char('@') + occurrence;
  if (key == m_hoveredJoint) {
    return;
  }
  m_hoveredJoint = key;
  emit jointHovered(uid, occurrence);
}

void BrowserPanel::hoverBodies(const BrowserNode& row) {
  BrowserNode node;
  // A body, or an occurrence (a component's row below the root) with all
  // the bodies in it.
  if (row.type == BrowserNode::Type::Body ||
      (row.type == BrowserNode::Type::Component && !row.occurrence.isEmpty())) {
    node = row;
  }
  const QString key = node.isValid() ? keyOf(node) : QString();
  if (key == m_hoveredBodies) {
    return;
  }
  m_hoveredBodies = key;
  emit bodiesHovered(node);
}

QVector<BrowserPanel::PresentationRow> BrowserPanel::presentationRows() const {
  QVector<PresentationRow> rows;
  const auto append = [this, &rows](const auto& self, const QTreeWidgetItem* item,
                                  const QString& parent) -> void {
    const BrowserNode node = nodeOf(item);
    const QString key = keyOf(node);
    // A bound row's icon keeps the fx mark there (mitcad#862: the Qt rows
    // draw the fx badge after the name).
    const QIcon icon = item->data(kName, kBindingsRole).toInt() > 0 ? boundIcon(item->icon(0)) : item->icon(0);
    rows.append({key, parent, node, item->toolTip(0), icon, item->isExpanded(),
                 item->data(kName, kRadioRole).isValid(), item->data(kName, kRadioRole).toBool(),
                 item->data(kName, kBindingsRole).toInt(), rowTone(item),
                 item->data(kName, kEyeTipRole).toString(), item->data(kName, kEyeBoundRole).toBool(),
                 item->data(kName, kMakeRole).toString(),
                 item->data(kName, kEyeRole).toBool() && item->data(kName, kEyeTransparentRole).toBool()});
    for (int index = 0; index < item->childCount(); ++index) {
      self(self, item->child(index), key);
    }
  };
  for (int index = 0; index < m_tree->topLevelItemCount(); ++index) {
    append(append, m_tree->topLevelItem(index), QString());
  }
  return rows;
}

void BrowserPanel::setPresentationExpanded(const QString& key, bool expanded) {
  for (QTreeWidgetItemIterator it(m_tree); *it != nullptr; ++it) {
    if (keyOf(nodeOf(*it)) != key) continue;
    if ((*it)->isExpanded() != expanded) {
      (*it)->setExpanded(expanded);
      emit contentHeightChanged();
    }
    return;
  }
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
  case BrowserNode::Type::Analyses:
    return QStringLiteral("analyses");
  case BrowserNode::Type::Analysis:
    return QStringLiteral("analysis:") + node.uid;
  case BrowserNode::Type::Joints:
    return QStringLiteral("joints") + at;
  case BrowserNode::Type::Joint:
    return QStringLiteral("joint:") + node.uid + at;
  case BrowserNode::Type::Feature:
    // One row under each sketch it uses.
    return QStringLiteral("feature:%1<%2").arg(node.uid, node.sketch) + at;
  case BrowserNode::Type::SheetRules:
    return QStringLiteral("sheetrules:%1<%2").arg(node.uid, node.sketch) + at;
  case BrowserNode::Type::FlatPattern:
    return QStringLiteral("flat:") + node.uid + at;
  case BrowserNode::Type::MotionStudies:
    return QStringLiteral("studies");
  case BrowserNode::Type::MotionStudy:
    return QStringLiteral("study:") + node.uid;
  case BrowserNode::Type::ContactSets:
    return QStringLiteral("contacts");
  case BrowserNode::Type::ContactSet:
    return QStringLiteral("contact:") + node.uid;
  case BrowserNode::Type::MotionLinks:
    return QStringLiteral("links") + at;
  case BrowserNode::Type::Measurements:
    return QStringLiteral("measurements");
  case BrowserNode::Type::Drawings:
    return QStringLiteral("drawings") + at;
  case BrowserNode::Type::Drawing:
    return QStringLiteral("drawing:") + node.uid + at;
  case BrowserNode::Type::ImageReferences:
    return QStringLiteral("images");
  case BrowserNode::Type::ImageReference:
    return QStringLiteral("image:") + node.uid;
  case BrowserNode::Type::CenterOfMass:
    return QStringLiteral("center-of-mass");
  case BrowserNode::Type::None:
    break;
  }
  return QString();
}

namespace {

// A row whose item follows expressions (mitcad#809, mitcad#862): the fx
// badge after its name with the count of its bound properties (`fx 3`),
// whose card says what they follow, and log lines for the UI tests
// ("Browser fx Root/Bodies/Body1: Appearance = switch(...)", "Browser fx
// badge Root/Bodies/Body1: fx 3").
void markBound(QTreeWidgetItem* row, const QString& path, const QStringList& lines, int count) {
  if (lines.isEmpty() || count <= 0) {
    return;
  }
  row->setData(kName, kBindingsRole, count);
  const QString tip = row->toolTip(kName);
  row->setToolTip(kName, (tip.isEmpty() ? QString() : tip + QLatin1Char('\n')) +
                             BrowserPanel::tr("Follows expressions: point at fx %1 to see what").arg(count));
  qDebug().noquote() << QStringLiteral("Browser fx %1: %2").arg(path, lines.join(QStringLiteral("; ")));
  qDebug().noquote() << QStringLiteral("Browser fx badge %1: %2").arg(path, bindingsText(count));
}

// The light bulb of a row that follows a condition (`bound`: drawn with the
// fx corner badge) or of a folder that holds such rows (the plain bulb: the
// folder has no condition of its own), with a tooltip that says what a click
// does, and a log line for the UI tests ("Browser fx bulb Root/Bodies/Body2
// off: Shown when …", "Browser bulb Root/Bodies on: …").
void markConditionBulb(QTreeWidgetItem* row, const QString& path, bool visible, const QString& tip,
                       bool bound) {
  row->setData(kName, kEyeTipRole, tip);
  if (bound) {
    row->setData(kName, kEyeBoundRole, true);
  }
  qDebug().noquote() << QStringLiteral("Browser %1 %2 %3: %4")
                            .arg(bound ? QStringLiteral("fx bulb") : QStringLiteral("bulb"), path,
                                 visible ? QStringLiteral("on") : QStringLiteral("off"), tip);
}

// Bodies that follow their Show When conditions, as a folder's light bulb
// says it ("Body2 follows its Show When and is left as it is").
QString leftToConditions(const QStringList& names) {
  return names.size() == 1
             ? BrowserPanel::tr("%1 follows its Show When and is left as it is.").arg(names.first())
             : BrowserPanel::tr("%1 follow their Show When and are left as they are.")
                   .arg(names.join(QStringLiteral(", ")));
}

// The tooltip of a conditioned row's light bulb: the condition, what it
// gives now with the values of its parameters, and that a click sets the
// visibility by hand ("Shown when Holes == 'Yes'. Hidden now (Holes is
// 'No'). Clicking the light bulb shows Body2 and removes its condition.").
QString conditionBulbTip(const QString& name, bool visible, const QString& condition, const QStringList& parts) {
  QString now = visible ? BrowserPanel::tr("Shown now") : BrowserPanel::tr("Hidden now");
  if (!parts.isEmpty()) {
    now += QStringLiteral(" (%1)").arg(parts.join(QStringLiteral(", ")));
  }
  const QString click = visible ? BrowserPanel::tr("Clicking the light bulb hides %1 and removes its condition.")
                                : BrowserPanel::tr("Clicking the light bulb shows %1 and removes its condition.");
  return BrowserPanel::tr("Shown when %1. %2. %3").arg(condition, now, click.arg(name));
}

} // namespace

bool BrowserPanel::markLevel(QTreeWidgetItem* row, const DocumentSnapshot::Feature& feature) {
  const FeatureState state = featureState(feature.status);
  const QString path = m_nodes.value(row).path;
  const QString level = state == FeatureState::Ok ? QString() : noticeLevelName(featureStateLevel(state));
  if (m_loggedLevels.value(path) != level) {
    m_loggedLevels.insert(path, level);
    qDebug().noquote() << QStringLiteral("Browser level %1: %2")
                              .arg(path, level.isEmpty() ? QStringLiteral("none") : level);
  }
  if (state == FeatureState::Ok) {
    return false;
  }
  // As the timeline shows it: the badge, the colour, the word (mitcad#1040).
  const NoticeLevel notice = featureStateLevel(state);
  setNameRole(row, noticeTextRole(notice));
  row->setIcon(kName, QIcon(new LinkedIconEngine(row->icon(kName), noticeIcon(notice))));
  row->setToolTip(kName, QStringLiteral("%1: %2").arg(feature.name, noticeLevelText(notice, feature.error)));
  return true;
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
  // A transparent body or component (mitcad#1093): the half-open eye while
  // shown; logged when it comes and goes for the UI tests ("Browser eye
  // Root/Bodies/Body2: transparent", "...: opaque").
  if (node.hasEye && node.transparent) {
    item->setData(kName, kEyeTransparentRole, true);
  }
  if (node.type == BrowserNode::Type::Body || node.type == BrowserNode::Type::Component) {
    if (node.transparent != m_loggedTransparent.contains(stored.path)) {
      if (node.transparent) {
        m_loggedTransparent.insert(stored.path);
      } else {
        m_loggedTransparent.remove(stored.path);
      }
      qDebug().noquote() << QStringLiteral("Browser eye %1: %2")
                                .arg(stored.path, node.transparent ? QStringLiteral("transparent")
                                                                   : QStringLiteral("opaque"));
    }
  }
  if (!tooltip.isEmpty()) {
    item->setToolTip(kName, tooltip);
  }
  const bool renamable = node.type == BrowserNode::Type::Component ||
                         node.type == BrowserNode::Type::Body ||
                         node.type == BrowserNode::Type::Sketch ||
                         node.type == BrowserNode::Type::Datum ||
                         node.type == BrowserNode::Type::Analysis || node.type == BrowserNode::Type::Joint ||
                         node.type == BrowserNode::Type::Feature ||
                         node.type == BrowserNode::Type::MotionStudy ||
                         node.type == BrowserNode::Type::ContactSet ||
                         node.type == BrowserNode::Type::Drawing ||
                         node.type == BrowserNode::Type::ImageReference ||
                         (node.type == BrowserNode::Type::NamedView &&
                          node.uid.startsWith(QStringLiteral("named:")));
  if (renamable) {
    item->setFlags(item->flags() | Qt::ItemIsEditable);
  }
  m_nodes.insert(item, stored);
  const QString key = keyOf(node);
  const bool open = node.type == BrowserNode::Type::Component || node.type == BrowserNode::Type::Bodies ||
                    node.type == BrowserNode::Type::Sketches ||
                    node.type == BrowserNode::Type::Construction || node.type == BrowserNode::Type::Analyses ||
                    node.type == BrowserNode::Type::Joints || node.type == BrowserNode::Type::MotionStudies ||
                    node.type == BrowserNode::Type::ContactSets || node.type == BrowserNode::Type::MotionLinks ||
                    node.type == BrowserNode::Type::Measurements || node.type == BrowserNode::Type::Drawings ||
                    node.type == BrowserNode::Type::ImageReferences;
  item->setExpanded(m_expanded.value(key, open));
  return item;
}

void BrowserPanel::addComponentRows(QTreeWidgetItem* parent, const DocumentSnapshot& snapshot,
                                    const QString& component, const QString& occurrence,
                                    bool shown, const SketchUses& uses) {
  const auto gray = [](QTreeWidgetItem* item, bool visible) {
    if (!visible) {
      setNameRole(item, ColorRole::TextHidden);
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
    // The folder's light bulb and Show All / Hide All change the bodies
    // without a Show When condition (mitcad#863): its state is theirs; it
    // is greyed only when no body is shown.
    bool anyShown = false;
    bool freeShown = false;
    QStringList conditioned;
    for (const QString& uid : def->bodies) {
      const DocumentSnapshot::Body body = snapshot.bodies.value(uid);
      anyShown = anyShown || body.visible;
      if (body.visibleWhen.isEmpty()) {
        freeShown = freeShown || body.visible;
      } else {
        conditioned << (body.name.isEmpty() ? uid : body.name);
      }
    }
    const bool allConditioned = conditioned.size() == def->bodies.size();
    folder.visible = allConditioned ? anyShown : freeShown;
    QTreeWidgetItem* bodies = addRow(parent, folder, QStringLiteral("folder"));
    gray(bodies, shown && anyShown);
    // The folder has no condition of its own: the plain bulb, whose
    // tooltip names the bodies it leaves as they are.
    if (!conditioned.isEmpty()) {
      const QString left = leftToConditions(conditioned);
      QString tip = left;
      if (!allConditioned) {
        const QString click = folder.visible ? tr("Clicking the light bulb hides the other bodies.")
                                             : tr("Clicking the light bulb shows the other bodies.");
        tip = QStringLiteral("%1 %2").arg(click, left);
      }
      markConditionBulb(bodies, m_nodes.value(bodies).path, folder.visible, tip, false);
    }
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
      node.transparent = body.transparent;
      node.mesh = body.mesh;
      const QString feature = uid.section(QLatin1Char('.'), 0, 0);
      const DocumentSnapshot::Feature* made = snapshot.feature(feature);
      QString tip = tr("%1 (%2), made by %3").arg(node.name, uid, made != nullptr ? made->name : feature);
      // The face that lies on the print bed (mitcad#731).
      if (!body.printBedFace.isEmpty()) {
        tip += QLatin1Char('\n') + tr("Print bed face: %1").arg(printBedText(snapshot, body));
      }
      QTreeWidgetItem* row = addRow(bodies, node, QStringLiteral("body"), tip);
      gray(row, shown && body.visible);
      // Bindings (mitcad#809).
      if (body.bound()) {
        QStringList lines;
        if (!body.appearanceExpr.isEmpty()) {
          lines << tr("Appearance = %1").arg(body.appearanceExpr);
        }
        if (!body.materialExpr.isEmpty()) {
          lines << tr("Material = %1").arg(body.materialExpr);
        }
        if (!body.visibleWhen.isEmpty()) {
          lines << tr("Shown when %1").arg(body.visibleWhen);
        }
        for (auto face = body.faceAppearanceExprs.begin(); face != body.faceAppearanceExprs.end(); ++face) {
          lines << tr("Appearance of face %1 = %2").arg(face.key(), face.value().toString());
        }
        const int count = static_cast<int>(lines.size()); // one line per bound property
        lines << body.warnings;
        markBound(row, m_nodes.value(row).path, lines, count);
      }
      // Its light bulb follows the condition (mitcad#863).
      if (!body.visibleWhen.isEmpty()) {
        markConditionBulb(row, m_nodes.value(row).path, body.visible,
                          conditionBulbTip(node.name, body.visible, body.visibleWhen,
                                           snapshot.conditionParts(body.visibleWhen)),
                          true);
      }
      addSheetRows(row, snapshot, node, shown && body.visible);
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
    // View › Show greys a hidden kind's folder and rows, their light bulbs
    // as they are (mitcad#1287).
    const bool kindShown = sketches ? snapshot.showSketches : snapshot.showConstruction;
    gray(rows, shown && folder.visible && kindShown);
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
      if (!markLevel(row, *feature)) {
        gray(row, shown && node.visible && kindShown);
      }
      if (sketches) {
        addFeatureRows(row, snapshot, *feature, occurrence, shown, uses);
      }
    }
  }

  // Drawings (mitcad#707): the component's, of it in its own coordinates;
  // a double-click opens one over the view.
  const QJsonArray drawings = snapshot.drawingsOf(component);
  if (!drawings.isEmpty()) {
    BrowserNode drawingsFolder;
    drawingsFolder.type = BrowserNode::Type::Drawings;
    drawingsFolder.occurrence = occurrence;
    drawingsFolder.component = component;
    drawingsFolder.name = tr("Drawings");
    QTreeWidgetItem* drawingRows = addRow(parent, drawingsFolder, QStringLiteral("folder"));
    gray(drawingRows, shown);
    for (const QJsonValue& value : drawings) {
      const QJsonObject drawing = value.toObject();
      BrowserNode node;
      node.type = BrowserNode::Type::Drawing;
      node.uid = drawing.value(QStringLiteral("drawing")).toString();
      node.occurrence = occurrence;
      node.component = component;
      node.name = drawing.value(QStringLiteral("name")).toString();
      const qsizetype sheets = drawing.value(QStringLiteral("sheets")).toArray().size();
      QTreeWidgetItem* row = addRow(drawingRows, node, QStringLiteral("drawing"),
                                    tr("%1: %n sheet(s)", nullptr, static_cast<int>(sheets)).arg(node.name));
      gray(row, shown);
    }
  }

  // Joints (mitcad#55): the component's joints, as-built joints, joint
  // origins (datums, with light bulbs) and rigid groups before the marker;
  // the folder's tooltip has the component's degrees of freedom.
  QVector<const DocumentSnapshot::Feature*> joints;
  for (const DocumentSnapshot::Feature& feature : snapshot.features) {
    if (feature.component == component && feature.isActive() && feature.isJoint()) {
      joints.append(&feature);
    }
  }
  if (joints.isEmpty()) {
    return;
  }
  BrowserNode jointsFolder;
  jointsFolder.type = BrowserNode::Type::Joints;
  jointsFolder.occurrence = occurrence;
  jointsFolder.component = component;
  jointsFolder.name = tr("Joints");
  const QJsonObject dof = snapshot.jointDof(component);
  QString folderTip;
  if (!dof.isEmpty()) {
    folderTip = tr("%n degree(s) of freedom", nullptr, dof.value(QStringLiteral("total")).toInt());
    if (dof.value(QStringLiteral("overconstrained")).toBool()) {
      folderTip += tr(", conflicting joints");
    }
  }
  QTreeWidgetItem* jointRows = addRow(parent, jointsFolder, QStringLiteral("folder"), folderTip);
  gray(jointRows, shown);
  for (const DocumentSnapshot::Feature* feature : joints) {
    BrowserNode node;
    const bool origin = feature->type == QStringLiteral("joint_origin");
    node.type = origin ? BrowserNode::Type::Datum : BrowserNode::Type::Joint;
    node.uid = feature->uid;
    node.occurrence = occurrence;
    node.component = component;
    node.featureType = feature->type;
    node.name = feature->name;
    node.visible = origin ? snapshot.isShown(feature->uid) : true;
    node.hasEye = origin;
    const bool failed = feature->status == QStringLiteral("error");
    QTreeWidgetItem* row = addRow(jointRows, node, featureIcon(feature->type),
                                  failed ? QStringLiteral("%1: %2").arg(feature->name, feature->error)
                                         : jointTooltip(snapshot, *feature));
    if (!markLevel(row, *feature)) {
      gray(row, shown && node.visible && !snapshot.kindHidden(*feature)); // View › Show (mitcad#1287)
    }
    // A constraint set's rows nested under it (mitcad#148).
    if (feature->type == QStringLiteral("constraint_set")) {
      addConstraintRows(snapshot, *feature, row, shown && !failed);
    }
  }

  // Motion links (mitcad#466): the component's links before the marker,
  // in a folder of their own beside the joints.
  QVector<const DocumentSnapshot::Feature*> links;
  for (const DocumentSnapshot::Feature& feature : snapshot.features) {
    if (feature.component == component && feature.isActive() && feature.isMotionLink()) {
      links.append(&feature);
    }
  }
  if (links.isEmpty()) {
    return;
  }
  BrowserNode linksFolder;
  linksFolder.type = BrowserNode::Type::MotionLinks;
  linksFolder.occurrence = occurrence;
  linksFolder.component = component;
  linksFolder.name = tr("Motion Links");
  QTreeWidgetItem* linkRows = addRow(parent, linksFolder, QStringLiteral("folder"));
  gray(linkRows, shown);
  for (const DocumentSnapshot::Feature* feature : links) {
    BrowserNode node;
    node.type = BrowserNode::Type::Joint;
    node.uid = feature->uid;
    node.occurrence = occurrence;
    node.component = component;
    node.featureType = feature->type;
    node.name = feature->name;
    const bool failed = feature->status == QStringLiteral("error");
    QTreeWidgetItem* row = addRow(linkRows, node, featureIcon(feature->type),
                                  failed ? QStringLiteral("%1: %2").arg(feature->name, feature->error)
                                         : jointTooltip(snapshot, *feature));
    if (!markLevel(row, *feature)) {
      gray(row, shown);
    }
  }
}

void BrowserPanel::addFeatureRows(QTreeWidgetItem* parent, const DocumentSnapshot& snapshot,
                                  const DocumentSnapshot::Feature& sketch, const QString& occurrence, bool shown,
                                  const SketchUses& uses) {
  for (const DocumentSnapshot::Feature* feature : uses.features.value(sketch.uid)) {
    BrowserNode node;
    node.type = BrowserNode::Type::Feature;
    node.uid = feature->uid;
    node.sketch = sketch.uid;
    node.occurrence = occurrence;
    node.component = sketch.component;
    node.featureType = feature->type;
    node.name = feature->name;
    // The row follows the sketch it uses; a result in another component (a
    // new component's bodies) is marked as linked there.
    QSet<QString> elsewhere = uses.resultComponents.value(feature->uid);
    elsewhere.insert(feature->component);
    elsewhere.remove(sketch.component);
    QStringList linked;
    for (const QString& component : std::as_const(elsewhere)) {
      linked << snapshot.componentName(component);
    }
    linked.sort();
    const bool failed = feature->status == QStringLiteral("error");
    // Rolled back or suppressed: grey, as the timeline draws it faint.
    const bool off = feature->suppressed || feature->index >= snapshot.marker;
    QString tip = failed               ? QStringLiteral("%1: %2").arg(feature->name, feature->error)
                  : feature->suppressed ? tr("%1: suppressed").arg(feature->name)
                  : off                 ? tr("%1: rolled back").arg(feature->name)
                                        : tr("%1 uses %2").arg(feature->name, sketch.name);
    if (!linked.isEmpty()) {
      tip += tr("; its result is in %1").arg(linked.join(QStringLiteral(", ")));
    }
    QTreeWidgetItem* row = addRow(parent, node, featureIcon(feature->type), tip);
    if (!linked.isEmpty()) {
      row->setIcon(kName, QIcon(new LinkedIconEngine(themeIcon(featureIcon(feature->type)),
                                                     themeIcon(QStringLiteral("component")))));
      row->setData(kName, kLinkedRole, linked.join(QStringLiteral(", ")));
    }
    if (!markLevel(row, *feature) && (off || !shown)) {
      setNameRole(row, ColorRole::TextHidden);
    }
    // Suppressed when a condition holds (mitcad#809).
    if (!feature->suppressedWhen.isEmpty()) {
      markBound(row, m_nodes.value(row).path, {tr("Suppressed when %1").arg(feature->suppressedWhen)}, 1);
    }
  }
}

void BrowserPanel::addSheetRows(QTreeWidgetItem* bodyRow, const DocumentSnapshot& snapshot, const BrowserNode& body,
                                bool shown) {
  const QJsonObject sheet = snapshot.sheet(body.uid);
  if (sheet.isEmpty()) {
    return;
  }
  const QString rules = sheet.value(QStringLiteral("rules")).toString();
  // The lengths in the design's unit (mitcad#1165), the K-factor as it is.
  const auto length = [&sheet, &snapshot](const char* key) {
    return lengthInUnit(sheet.value(QLatin1String(key)).toDouble(), snapshot.lengthUnit);
  };
  const QString kFactor = QString::number(sheet.value(QStringLiteral("k_factor")).toDouble(), 'g', 6);
  BrowserNode node;
  node.type = BrowserNode::Type::SheetRules;
  node.uid = rules;
  node.sketch = body.uid; // the body it is listed under, for its key
  node.occurrence = body.occurrence;
  node.component = body.component;
  node.featureType = QStringLiteral("sheet_rules");
  node.name = QStringLiteral("%1 (%2, R %3, K %4)")
                  .arg(sheet.value(QStringLiteral("rules_name")).toString(rules), length("thickness"),
                       length("bend_radius"), kFactor);
  QTreeWidgetItem* row =
      addRow(bodyRow, node, QStringLiteral("sheet-rules"),
             tr("Sheet rules %1: thickness %2, bend radius %3, K-factor %4")
                 .arg(sheet.value(QStringLiteral("rules_name")).toString(rules), length("thickness"),
                      length("bend_radius"), kFactor));
  if (!shown) {
    setNameRole(row, ColorRole::TextHidden);
  }
  const QJsonValue flat = sheet.value(QStringLiteral("flat_pattern"));
  if (flat.isObject()) {
    const QJsonObject pattern = flat.toObject();
    BrowserNode item;
    item.type = BrowserNode::Type::FlatPattern;
    item.uid = body.uid;
    item.occurrence = body.occurrence;
    item.component = body.component;
    item.name = tr("Flat Pattern");
    item.hasEye = true;
    item.visible = pattern.value(QStringLiteral("shown")).toBool();
    const bool valid = pattern.value(QStringLiteral("status")).toString() == QStringLiteral("valid");
    QStringList reasons;
    for (const QJsonValue& reason : pattern.value(QStringLiteral("diagnostics")).toArray()) {
      reasons << reason.toString();
    }
    QTreeWidgetItem* flatRow = addRow(
        bodyRow, item, QStringLiteral("flat-pattern"),
        valid ? tr("Flat Pattern of %1: %2 mm2, %3 bend(s); the light bulb shows it flat")
                    .arg(body.name)
                    .arg(pattern.value(QStringLiteral("area")).toDouble(), 0, 'f', 2)
                    .arg(pattern.value(QStringLiteral("bends")).toInt())
              : tr("Flat Pattern of %1 is %2: %3")
                    .arg(body.name, pattern.value(QStringLiteral("status")).toString(), reasons.join(QStringLiteral("; "))));
    if (!valid) {
      setNameRole(flatRow, ColorRole::ErrorText);
    } else if (!shown) {
      setNameRole(flatRow, ColorRole::TextHidden);
    }
  }
  bodyRow->setExpanded(m_expanded.value(keyOf(body), true));
}

void BrowserPanel::addConstraintRows(const DocumentSnapshot& snapshot, const DocumentSnapshot::Feature& feature,
                                     QTreeWidgetItem* parent, bool shown) {
  for (const QJsonValue& value : snapshot.joints.value(QStringLiteral("constraints")).toArray()) {
    const QJsonObject set = value.toObject();
    if (set.value(QStringLiteral("uid")).toString() != feature.uid) {
      continue;
    }
    for (const QJsonValue& rowValue : set.value(QStringLiteral("rows")).toArray()) {
      const QJsonObject row = rowValue.toObject();
      QString type = row.value(QStringLiteral("type")).toString();
      type = type == QStringLiteral("insert_axis") ? tr("Insert (Axis)") : type.left(1).toUpper() + type.mid(1);
      const auto side = [&row](const char* key) {
        return row.value(QLatin1String(key)).toObject().value(QStringLiteral("occurrence")).toString();
      };
      BrowserNode node;
      node.type = BrowserNode::Type::None;
      node.component = feature.component;
      node.name = tr("%1 (%2, %3)").arg(type, side("a"), side("b"));
      const bool holds = row.value(QStringLiteral("holds")).toBool(true);
      QTreeWidgetItem* item =
          addRow(parent, node, QStringLiteral("constraint"),
                 holds ? tr("Row %1 of %2").arg(row.value(QStringLiteral("row")).toInt()).arg(feature.name)
                       : tr("Row %1 of %2: does not hold at the marker")
                             .arg(row.value(QStringLiteral("row")).toInt())
                             .arg(feature.name));
      if (!holds) {
        setNameRole(item, ColorRole::ErrorText);
      } else if (!shown) {
        setNameRole(item, ColorRole::TextHidden);
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
  node.name = tr("Design Settings");
  QTreeWidgetItem* settings = addRow(rootItem, node, QStringLiteral("settings"));
  node.type = BrowserNode::Type::Units;
  node.name = tr("Units: %1").arg(snapshot.lengthUnit);
  addRow(settings, node, QStringLiteral("units"), tr("Double-click to change the design's unit"));

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
  for (const OriginDatum& datum : kOriginDatums) {
    BrowserNode row;
    row.type = BrowserNode::Type::OriginDatum;
    row.component = rootNode.uid;
    row.uid = QString::fromLatin1(datum.uid);
    row.name = QString::fromLatin1(datum.name);
    addRow(origin, row, QString::fromLatin1(datum.icon));
  }
  m_originShown = originShown;
  m_originsKindShown = snapshot.showOrigins; // View › Show greys it too (mitcad#1287)
  styleOriginRows(origin);

  // The Center of Mass marker (mitcad#1094), right under Origin while the
  // design has one; its tooltip says where the center is.
  if (const std::optional<CenterOfMassShown> center = centerOfMassOf(snapshot.centerOfMass)) {
    BrowserNode row;
    row.type = BrowserNode::Type::CenterOfMass;
    row.component = rootNode.uid;
    row.name = tr("Center of Mass");
    row.hasEye = true;
    row.visible = center->visible;
    QTreeWidgetItem* item = addRow(rootItem, row, QStringLiteral("center-of-mass"), centerOfMassTip(*center));
    if (!center->visible) {
      setNameRole(item, ColorRole::TextHidden);
    }
  }

  // The features under each sketch they use (mitcad#177): those without rows
  // of their own (sketches, construction geometry and joints have theirs),
  // in timeline order, also suppressed or after the marker.
  SketchUses uses;
  for (const DocumentSnapshot::Feature& feature : snapshot.features) {
    if (feature.isSketch() || feature.isConstruction() || feature.isJoint()) {
      continue;
    }
    for (const QString& sketch : feature.sketches) {
      uses.features[sketch].append(&feature);
    }
  }
  for (auto it = snapshot.bodies.cbegin(); it != snapshot.bodies.cend(); ++it) {
    uses.resultComponents[it.key().section(QLatin1Char('.'), 0, 0)].insert(it.value().component);
  }

  addComponentRows(rootItem, snapshot, rootNode.uid, QString(), true, uses);

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
          row.transparent = occurrence.transparent;
          const DocumentSnapshot::Component* def = snapshot.component(occurrence.component);
          QString tip = tr("Occurrence %1 of %2").arg(occurrence.name, snapshot.componentName(occurrence.component));
          if (occurrence.grounded) {
            tip += tr(", grounded");
          }
          // What its joints leave it (mitcad#55), with the others held.
          if (const int dof = snapshot.occurrenceDof(occurrence.uid); dof >= 0) {
            tip += tr(", %n degree(s) of freedom", nullptr, dof);
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
            setNameRole(item, ColorRole::TextHidden);
          }
          item->setData(kName, kRadioRole, snapshot.activeComponent == occurrence.component);
          // Shown when a condition holds (mitcad#809).
          if (!occurrence.visibleWhen.isEmpty()) {
            markBound(item, m_nodes.value(item).path, {tr("Shown when %1").arg(occurrence.visibleWhen)}, 1);
          }
          // Its light bulb follows the condition (mitcad#863).
          if (!occurrence.visibleWhen.isEmpty()) {
            markConditionBulb(item, m_nodes.value(item).path, occurrence.visible,
                              conditionBulbTip(occurrence.name, occurrence.visible, occurrence.visibleWhen,
                                               snapshot.conditionParts(occurrence.visibleWhen)),
                              true);
          }
          addComponentRows(item, snapshot, occurrence.component, occurrence.path, visible, uses);
          place(item, occurrence.children, visible);
        }
      };
  place(rootItem, snapshot.occurrences, true);
  rootItem->setData(kName, kRadioRole, snapshot.activeComponent == rootNode.uid);

  // The analyses kept in the document (mitcad#41), one shown at a time.
  m_analysisRows.clear();
  if (!snapshot.analyses.isEmpty()) {
    BrowserNode folder;
    folder.type = BrowserNode::Type::Analyses;
    folder.component = rootNode.uid;
    folder.name = tr("Analysis");
    QTreeWidgetItem* analyses = addRow(rootItem, folder, QStringLiteral("folder"));
    for (const QJsonValue& value : snapshot.analyses) {
      const QJsonObject analysis = value.toObject();
      BrowserNode row;
      row.type = BrowserNode::Type::Analysis;
      row.component = rootNode.uid;
      row.uid = analysis.value(QStringLiteral("name")).toString();
      row.name = row.uid;
      row.visible = analysis.value(QStringLiteral("visible")).toBool();
      row.hasEye = true;
      // Section and Print Analysis (mitcad#620), each with its icon.
      row.featureType = analysis.value(QStringLiteral("type")).toString();
      const bool print = row.featureType == QStringLiteral("print");
      m_analysisRows.insert(row.name, analysis);
      styleAnalysisRow(addRow(analyses, row, print ? QStringLiteral("print-analysis") : QStringLiteral("section-analysis")),
                       analysis);
    }
  }

  // Image references (mitcad#782): the root's Images folder, a row per
  // image with its light bulb; its tooltip names the file and the size.
  if (!snapshot.imageReferences.isEmpty()) {
    BrowserNode folder;
    folder.type = BrowserNode::Type::ImageReferences;
    folder.component = rootNode.uid;
    folder.name = tr("Images");
    QTreeWidgetItem* images = addRow(rootItem, folder, QStringLiteral("folder"));
    for (const QJsonValue& value : snapshot.imageReferences) {
      const QJsonObject image = value.toObject();
      BrowserNode row;
      row.type = BrowserNode::Type::ImageReference;
      row.component = rootNode.uid;
      row.uid = image.value(QStringLiteral("name")).toString();
      row.name = row.uid;
      row.visible = image.value(QStringLiteral("visible")).toBool(true);
      row.hasEye = true;
      const QString error = image.value(QStringLiteral("error")).toString();
      const QJsonArray size = image.value(QStringLiteral("size")).toArray();
      const QString file = image.value(QStringLiteral("file_name")).toString();
      const QString about = tr("%1 × %2 mm")
                                .arg(QString::number(size.at(0).toDouble(), 'f', 1),
                                     QString::number(size.at(1).toDouble(), 'f', 1));
      const QString tooltip = !error.isEmpty() ? QStringLiteral("%1: %2").arg(row.name, error)
                              : file.isEmpty() ? QStringLiteral("%1: %2").arg(row.name, about)
                                               : QStringLiteral("%1: %2, %3").arg(row.name, file, about);
      QTreeWidgetItem* item = addRow(images, row, QStringLiteral("image-reference"), tooltip);
      if (!error.isEmpty()) {
        setNameRole(item, ColorRole::ErrorText);
      } else if (!row.visible) {
        setNameRole(item, ColorRole::TextHidden);
      }
    }
  }

  // Annotations (mitcad#702; the measurement objects of mitcad#649): one
  // folder row that opens the Annotations window, where they are listed
  // and edited; its tooltip counts them and the broken ones.
  if (!snapshot.measurements.isEmpty()) {
    BrowserNode folder;
    folder.type = BrowserNode::Type::Measurements;
    folder.component = rootNode.uid;
    folder.name = tr("Annotations");
    int broken = 0;
    for (const QJsonValue& value : snapshot.measurements) {
      broken += value.toObject().contains(QStringLiteral("error")) ? 1 : 0;
    }
    const int count = static_cast<int>(snapshot.measurements.size());
    const QString tooltip = broken > 0 ? tr("%n annotation(s), %1 broken", nullptr, count)
                                             .arg(broken)
                                       : tr("%n annotation(s)", nullptr, count);
    QTreeWidgetItem* item = addRow(rootItem, folder, QStringLiteral("measure"), tooltip);
    // Its level, logged as the feature rows log theirs (mitcad#1051).
    const QString path = m_nodes.value(item).path;
    const QString level = broken > 0 ? noticeLevelName(NoticeLevel::Warning) : QString();
    if (m_loggedLevels.value(path) != level) {
      m_loggedLevels.insert(path, level);
      qDebug().noquote() << QStringLiteral("Browser level %1: %2")
                                .arg(path, level.isEmpty() ? QStringLiteral("none") : level);
    }
    if (broken > 0) {
      // A broken annotation is a warning (mitcad#1040): the warning's
      // badge and colour.
      item->setIcon(kName, QIcon(new LinkedIconEngine(item->icon(kName), noticeIcon(NoticeLevel::Warning))));
      setNameRole(item, noticeTextRole(NoticeLevel::Warning));
    }
  }

  // Motion studies and contact sets (mitcad#454), folders of their own
  // beside the Joints.
  if (!snapshot.motionStudies.isEmpty()) {
    BrowserNode folder;
    folder.type = BrowserNode::Type::MotionStudies;
    folder.component = rootNode.uid;
    folder.name = tr("Motion Studies");
    QTreeWidgetItem* studies = addRow(rootItem, folder, QStringLiteral("folder"));
    for (const QJsonValue& value : snapshot.motionStudies) {
      const QJsonObject study = value.toObject();
      BrowserNode row;
      row.type = BrowserNode::Type::MotionStudy;
      row.component = rootNode.uid;
      row.uid = study.value(QStringLiteral("name")).toString();
      row.name = row.uid;
      QStringList keyed;
      for (const QJsonValue& key : study.value(QStringLiteral("keys")).toArray()) {
        const QJsonObject k = key.toObject();
        keyed << QStringLiteral("%1 %2").arg(k.value(QStringLiteral("joint_name")).toString(),
                                             k.value(QStringLiteral("motion")).toString());
      }
      addRow(studies, row, QStringLiteral("motion-study"),
             tr("%1: %2 steps; keys: %3")
                 .arg(row.name)
                 .arg(study.value(QStringLiteral("steps")).toInt())
                 .arg(keyed.isEmpty() ? tr("none") : keyed.join(QStringLiteral(", "))));
    }
  }
  if (!snapshot.contactSets.isEmpty()) {
    BrowserNode folder;
    folder.type = BrowserNode::Type::ContactSets;
    folder.component = rootNode.uid;
    folder.name = tr("Contact Sets");
    QTreeWidgetItem* sets = addRow(rootItem, folder, QStringLiteral("folder"));
    for (const QJsonValue& value : snapshot.contactSets) {
      const QJsonObject set = value.toObject();
      BrowserNode row;
      row.type = BrowserNode::Type::ContactSet;
      row.component = rootNode.uid;
      row.uid = set.value(QStringLiteral("name")).toString();
      row.name = row.uid;
      QStringList names;
      for (const QJsonValue& occurrence : set.value(QStringLiteral("occurrences")).toArray()) {
        names << occurrence.toObject().value(QStringLiteral("name")).toString();
      }
      const bool suppressed = set.value(QStringLiteral("suppressed")).toBool();
      QTreeWidgetItem* item =
          addRow(sets, row, QStringLiteral("contact-set"),
                 tr("%1: %2%3").arg(row.name, names.join(QStringLiteral(", ")),
                                    suppressed ? tr(" (suppressed)") : QString()));
      if (suppressed) {
        setNameRole(item, ColorRole::TextHidden);
      }
    }
  }

  for (auto it = m_nodes.cbegin(); it != m_nodes.cend(); ++it) {
    if (selected.contains(keyOf(it.value()))) {
      it.key()->setSelected(true);
    }
  }
  m_building = false;
  applyMakeMarkers(); // the new rows' markers (mitcad#1049)
  logRowColours();    // mitcad#1104
  m_tree->verticalScrollBar()->setValue(scroll);
  emit contentHeightChanged();
  QTimer::singleShot(0, this, &BrowserPanel::logLayout);
  queueFit(); // a design opened: its rows are here now (designOpened)
}

void BrowserPanel::setMakeMarkers(const QHash<QString, QString>& markers) {
  if (markers == m_makeMarkers) {
    return;
  }
  m_makeMarkers = markers;
  applyMakeMarkers();
  emit rowsRestyled(); // the AppKit outline's markers (mitcad#1352)
}

void BrowserPanel::setAnalysisNotes(const QHash<QString, AnalysisNote>& notes) {
  if (notes == m_analysisNotes) {
    return;
  }
  m_analysisNotes = notes;
  applyAnalysisNotes();
  emit rowsRestyled(); // the AppKit outline's row (mitcad#1352)
}

void BrowserPanel::applyAnalysisNotes() {
  // Not a rename: the tree's itemChanged is the name's.
  const QSignalBlocker blocker(m_tree);
  for (auto it = m_nodes.cbegin(); it != m_nodes.cend(); ++it) {
    if (it.value().type != BrowserNode::Type::Analysis) {
      continue;
    }
    const auto analysis = m_analysisRows.constFind(it.value().uid);
    if (analysis != m_analysisRows.cend()) {
      styleAnalysisRow(it.key(), analysis.value());
    }
  }
  logRowColours();
  m_tree->viewport()->update();
}

void BrowserPanel::styleAnalysisRow(QTreeWidgetItem* item, const QJsonObject& analysis) {
  const QString name = analysis.value(QStringLiteral("name")).toString();
  const bool print = analysis.value(QStringLiteral("type")).toString() == QStringLiteral("print");
  const QString error = analysis.value(QStringLiteral("error")).toString();
  const QString kind = print ? tr("Print Analysis") : tr("Section Analysis");
  const QIcon icon = themeIcon(print ? QStringLiteral("print-analysis") : QStringLiteral("section-analysis"));
  // A plane that cannot be found stays the error it is (mitcad#1230).
  const AnalysisNote note = error.isEmpty() ? m_analysisNotes.value(name) : AnalysisNote{};
  QString tooltip = QStringLiteral("%1: %2").arg(name, error.isEmpty() ? kind : error);
  if (!note.text.isEmpty()) {
    tooltip += QLatin1Char('\n') + note.text;
  }
  item->setToolTip(kName, tooltip);
  if (!error.isEmpty()) {
    item->setIcon(kName, icon);
    setNameRole(item, ColorRole::ErrorText);
  } else if (note.warning) {
    // The warning's badge and colour, as a feature row's (mitcad#1040).
    item->setIcon(kName, QIcon(new LinkedIconEngine(icon, noticeIcon(NoticeLevel::Warning))));
    setNameRole(item, noticeTextRole(NoticeLevel::Warning));
  } else {
    item->setIcon(kName, icon);
    if (!analysis.value(QStringLiteral("visible")).toBool()) {
      setNameRole(item, ColorRole::TextHidden);
    } else {
      clearNameRole(item);
    }
  }
  const QString path = m_nodes.value(item).path;
  QString logged = QStringLiteral("none");
  if (!note.text.isEmpty()) {
    logged = QStringLiteral("%1 %2").arg(note.warning ? noticeLevelName(NoticeLevel::Warning)
                                                      : noticeLevelName(NoticeLevel::Information),
                                         QString(note.text).replace(QLatin1Char('\n'), QStringLiteral(" | ")));
  }
  if (m_loggedAnalysisNotes.value(path, QStringLiteral("none")) != logged) {
    m_loggedAnalysisNotes.insert(path, logged);
    qDebug().noquote() << QStringLiteral("Browser analysis note %1: %2").arg(path, logged);
  }
}

void BrowserPanel::applyMakeMarkers() {
  // Not a rename: the tree's itemChanged is the name's.
  const QSignalBlocker blocker(m_tree);
  QStringList logged;
  for (auto it = m_nodes.cbegin(); it != m_nodes.cend(); ++it) {
    if (it.value().type != BrowserNode::Type::Body) {
      continue;
    }
    const QString tip = m_makeMarkers.value(it.value().uid);
    if (it.key()->data(kName, kMakeRole).toString() != tip) {
      it.key()->setData(kName, kMakeRole, tip.isEmpty() ? QVariant() : QVariant(tip));
    }
    if (!tip.isEmpty()) {
      logged << QStringLiteral("%1: %2").arg(it.value().path, tip);
    }
  }
  logged.sort();
  const QString line = logged.isEmpty() ? QStringLiteral("none") : logged.join(QStringLiteral(" | "));
  if (line != m_loggedMakeMarkers) {
    m_loggedMakeMarkers = line;
    qDebug().noquote() << QStringLiteral("Browser Make markers: %1").arg(line);
  }
  m_tree->viewport()->update();
}

void BrowserPanel::changeEvent(QEvent* event) {
  QWidget::changeEvent(event);
  if (event->type() == QEvent::PaletteChange || event->type() == QEvent::StyleChange) {
    m_tree->viewport()->update();
    logRowColours();
  }
}

void BrowserPanel::logRowColours() {
  // By role, in a fixed order; at most a dozen paths each.
  const std::pair<ColorRole, const char*> roles[] = {{ColorRole::TextHidden, "hidden"},
                                                     {ColorRole::ErrorText, "error"},
                                                     {ColorRole::WarningText, "warning"},
                                                     {ColorRole::SuccessText, "success"},
                                                     {ColorRole::InfoText, "info"}};
  QHash<int, QStringList> paths;
  for (auto it = m_nodes.cbegin(); it != m_nodes.cend(); ++it) {
    if (const QVariant role = it.key()->data(kName, kTextRole); role.isValid()) {
      paths[role.toInt()] << it.value().path;
    }
  }
  // A group's paths, sorted, at most a dozen.
  const auto listed = [](QStringList rows) {
    rows.sort();
    constexpr qsizetype kMost = 12;
    if (rows.size() > kMost) {
      const qsizetype more = rows.size() - kMost;
      rows = rows.mid(0, kMost);
      rows << QStringLiteral("%1 more").arg(more);
    }
    return rows.join(QStringLiteral(", "));
  };
  QStringList groups;
  QList<int> known;
  for (const auto& [role, word] : roles) {
    known << static_cast<int>(role);
    const QStringList rows = paths.value(static_cast<int>(role));
    if (rows.isEmpty()) {
      continue;
    }
    groups << QStringLiteral("%1 %2: %3").arg(QLatin1String(word), nameColor(role, m_tree).name(), listed(rows));
  }
  // A value that is none of these (mitcad#1143: another role's data under
  // kTextRole once painted a transparent row's name red): "unknown <n>".
  QList<int> unknown;
  for (auto it = paths.cbegin(); it != paths.cend(); ++it) {
    if (!known.contains(it.key())) {
      unknown << it.key();
    }
  }
  std::sort(unknown.begin(), unknown.end());
  for (const int value : unknown) {
    groups << QStringLiteral("unknown %1: %2").arg(value).arg(listed(paths.value(value)));
  }
  const QString line = QStringLiteral("Browser row colours: %1, %2")
                           .arg(isDarkPalette(rowsPalette(m_tree)) ? QStringLiteral("dark") : QStringLiteral("light"),
                                groups.isEmpty() ? QStringLiteral("none") : groups.join(QStringLiteral("; ")));
  if (line != m_loggedRowColours) {
    m_loggedRowColours = line;
    qDebug().noquote() << line;
  }
}

void BrowserPanel::styleOriginRows(QTreeWidgetItem* folder) {
  const auto style = [](QTreeWidgetItem* item, bool greyed) {
    if (greyed) {
      setNameRole(item, ColorRole::TextHidden);
    } else {
      clearNameRole(item);
    }
  };
  const bool greyed = !m_originShown || !m_originsKindShown;
  style(folder, greyed);
  for (int i = 0; i < folder->childCount(); ++i) {
    style(folder->child(i), greyed);
  }
}

void BrowserPanel::collapseAll() {
  m_foldedSelection = m_shownSelection;
  foldBelowTop();
  qDebug().noquote() << QStringLiteral("Browser collapsed all");
}

void BrowserPanel::foldBelowTop() {
  // The states of rows that are gone are forgotten too, and every row now
  // present below the top row is remembered as folded, also where a row
  // opens by default; the top row (the root) as open (mitcad#426).
  m_expanded.clear();
  const bool wasBuilding = m_building;
  m_building = true; // the signals below remember nothing
  for (QTreeWidgetItemIterator it(m_tree); *it != nullptr; ++it) {
    const bool top = (*it)->parent() == nullptr;
    (*it)->setExpanded(top);
    m_expanded.insert(keyOf(nodeOf(*it)), top);
  }
  m_building = wasBuilding;
  emit contentHeightChanged();
  QTimer::singleShot(0, this, &BrowserPanel::logLayout);
}

void BrowserPanel::designOpened() {
  // The design before's open and folded rows are not this one's.
  m_expanded.clear();
  m_foldedSelection.clear();
  m_fitPending = true;
  queueFit();
}

int BrowserPanel::availableHeight() const {
  // The layout's largest height of the panel (the tree and its frame);
  // negative before its first layout.
  const int largest = m_availableHeight ? m_availableHeight() : -1;
  return largest >= 0 ? std::max(largest - 2 * m_tree->frameWidth(), 0) : m_tree->viewport()->height();
}

void BrowserPanel::setAvailableHeight(std::function<int()> height) { m_availableHeight = std::move(height); }

void BrowserPanel::queueFit() {
  if (!m_fitPending || m_fitQueued) {
    return;
  }
  m_fitQueued = true;
  // After the rebuild and the layout in hand (a panel that shows lays out
  // its window first).
  TestSync::singleShot(0, this, [this] {
    m_fitQueued = false;
    fitOpenedDesign();
  });
}

void BrowserPanel::fitOpenedDesign() {
  // Not before the rows are there and the panel shows with its size (a
  // design opened at the start is read before the window shows): a resize
  // or a show of the rows' viewport queues it again.
  if (!m_fitPending || m_building || m_tree->topLevelItemCount() == 0 || !m_tree->viewport()->isVisible()) {
    return;
  }
  const int height = availableHeight();
  if (height <= 0) {
    return;
  }
  m_fitPending = false;
  const int rows = shownRows();
  const int visible = height / rowHeight();
  if (rows <= visible) {
    qDebug().noquote() << QStringLiteral("Browser opened expanded: %1 rows (%2 visible)").arg(rows).arg(visible);
    return;
  }
  foldBelowTop();
  m_tree->scrollToTop();
  qDebug().noquote()
      << QStringLiteral("Browser opened folded: %1 rows would not fit (%2 visible)").arg(rows).arg(visible);
}

int BrowserPanel::shownRows() const {
  int rows = 0;
  for (QTreeWidgetItemIterator it(m_tree); *it != nullptr; ++it) {
    bool shown = !(*it)->isHidden();
    for (const QTreeWidgetItem* parent = (*it)->parent(); shown && parent != nullptr; parent = parent->parent()) {
      shown = parent->isExpanded();
    }
    rows += shown ? 1 : 0;
  }
  return rows;
}

int BrowserPanel::rowHeight() const {
  return m_tree->topLevelItemCount() > 0 ? std::max(m_tree->sizeHintForRow(0), 20) : 20;
}

int BrowserPanel::contentHeight() const {
  // The rows in sight: those whose parents are all open.
  const int rows = shownRows();
  // The bar above the rows (mitcad#807) while it is shown.
  const int bar = m_topBar != nullptr && !m_topBar->isHidden()
                      ? m_topBar->sizeHint().height() + std::max(0, layout()->spacing())
                      : 0;
  return bar + (rows > 0 ? rows * rowHeight() : 0) + 2 * m_tree->frameWidth();
}

void BrowserPanel::showSelection(const Selection& items, const QStringList& features, bool scroll,
                                 bool keepPicked) {
  QSet<QString> keys;
  for (const SelectionItem& item : items) {
    keys.insert(itemKey(item));
  }
  const QSet<QString> picked(features.cbegin(), features.cend());
  // The selection Collapse All left (rows folded away): shown again by a
  // refresh, it must not scroll to its rows, which would open their parents.
  const QSet<QString> selection = keys + QSet<QString>(features.cbegin(), features.cend());
  const bool keepFolded = !m_foldedSelection.isEmpty() && selection == m_foldedSelection;
  if (!keepFolded) {
    m_foldedSelection.clear();
  }
  const QSet<QString> before = m_shownSelection;
  m_shownSelection = selection;
  const QSignalBlocker blocker(m_tree);
  // Scrolled to (mitcad#318): the row of the newest item (the last one in
  // the selection, the newest added first), else the row of the newest
  // feature (which opens its sketch's row); none when a selected row is in
  // sight already.
  QTreeWidgetItem* newest = nullptr;
  QTreeWidgetItem* newestFeature = nullptr;
  int newestRank = -1;
  int newestFeatureRank = -1;
  bool inSight = false;
  const QRect sight = m_tree->viewport()->rect();
  for (QTreeWidgetItemIterator it(m_tree); *it != nullptr; ++it) {
    const BrowserNode node = nodeOf(*it);
    const SelectionItem item = node.item();
    const bool feature = node.type == BrowserNode::Type::Feature;
    const QString key = feature ? node.uid : item.isValid() ? itemKey(item) : QString();
    const bool on = (feature ? picked.contains(node.uid) : item.isValid() && keys.contains(key)) ||
                    (keepPicked && (*it)->isSelected());
    (*it)->setSelected(on);
    if (!on) {
      continue;
    }
    bool open = !(*it)->isHidden();
    for (const QTreeWidgetItem* parent = (*it)->parent(); open && parent != nullptr; parent = parent->parent()) {
      open = parent->isExpanded() && !parent->isHidden();
    }
    // In sight as logLayout says: the middle of its row is in the view.
    const QRect row = m_tree->visualItemRect(*it);
    if (open && !row.isEmpty() && sight.contains(row.center())) {
      inSight = true;
    }
    // Newly added rows rank above those selected before, then by the place
    // in the selection.
    int rank = 0;
    if (feature) {
      rank = static_cast<int>(features.lastIndexOf(node.uid));
    } else {
      for (int i = static_cast<int>(items.size()) - 1; i >= 0; --i) {
        if (itemKey(items.at(i)) == key) {
          rank = i;
          break;
        }
      }
    }
    rank += before.contains(key) ? 0 : 1 << 20;
    int& bestRank = feature ? newestFeatureRank : newestRank;
    QTreeWidgetItem*& best = feature ? newestFeature : newest;
    if (rank > bestRank) {
      bestRank = rank;
      best = *it;
    }
  }
  QTreeWidgetItem* target = newest != nullptr ? newest : newestFeature;
  if (scroll && !keepFolded && !inSight && target != nullptr) {
    m_tree->scrollToItem(target);
    qDebug().noquote() << QStringLiteral("Browser scrolled to %1").arg(nodeOf(target).path);
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
    if (made || ((node.type == BrowserNode::Type::Sketch || node.type == BrowserNode::Type::Datum ||
                  node.type == BrowserNode::Type::Joint) &&
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
  // Its rows under the sketches it uses (mitcad#177).
  for (auto it = m_nodes.cbegin(); it != m_nodes.cend(); ++it) {
    if (it.value().type == BrowserNode::Type::Feature && it.value().uid == feature) {
      keys.insert(keyOf(it.value()));
    }
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
      if (m_presentationRenamer) {
        if (m_presentationRenamer(keyOf(node))) {
          qDebug().noquote() << QStringLiteral("Renaming %1").arg(node.path.isEmpty() ? node.name : node.path);
        }
        return;
      }
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

QVector<BrowserNode> BrowserPanel::selectedNodes() const {
  QVector<BrowserNode> nodes;
  for (QTreeWidgetItemIterator it(m_tree, QTreeWidgetItemIterator::Selected); *it; ++it) {
    nodes.append(nodeOf(*it));
  }
  return nodes;
}

QStringList BrowserPanel::selectedPresentationKeys() const {
  QStringList keys;
  for (const BrowserNode& node : selectedNodes()) keys.append(keyOf(node));
  return keys;
}

void BrowserPanel::setPresentationSelection(const QStringList& keys) {
  const QSignalBlocker blocker(m_tree);
  m_tree->clearSelection();
  for (auto it = m_nodes.cbegin(); it != m_nodes.cend(); ++it) {
    it.key()->setSelected(keys.contains(keyOf(it.value())));
  }
}

void BrowserPanel::selectionChanged() {
  if (m_building) {
    return;
  }
  logSelection();
  Selection items;
  QStringList features;
  for (QTreeWidgetItem* row : m_tree->selectedItems()) {
    const BrowserNode node = nodeOf(row);
    if (node.type == BrowserNode::Type::Feature) {
      if (!features.contains(node.uid)) {
        features.append(node.uid);
      }
      continue;
    }
    const SelectionItem item = node.item();
    if (item.isValid() && !items.contains(item)) {
      items.append(item);
    }
  }
  emit rowsPicked(items, features);
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
  QSet<QString> places;
  // The panel's width, and whether the rows need a horizontal scroll bar.
  lines << QStringLiteral("Browser width %1%2")
               .arg(width())
               .arg(m_tree->horizontalScrollBar()->isVisible() ? QStringLiteral(", scrolls sideways")
                                                               : QString());
  // The indentation per level, the same as the Projects panel's (mitcad#393).
  lines << QStringLiteral("Browser indentation %1").arg(m_tree->indentation());
  // Where the rows are scrolled to (mitcad#318: a pick in the tree keeps it).
  lines << QStringLiteral("Browser scroll %1").arg(m_tree->verticalScrollBar()->value());
  // Whether the rows overflow (mitcad#425: a floating card grows to the card
  // below it first), and the vertical scroll bar in sight: the tree's, or the
  // Rows background's at the left (setScrollBarOnLeft).
  lines << QStringLiteral("Browser scroll range %1").arg(m_tree->verticalScrollBar()->maximum());
  if (const QScrollBar* bar = (m_leftBar != nullptr && m_scrollBarLeft) ? m_leftBar : m_tree->verticalScrollBar();
      bar->isVisible()) {
    const QPoint at = GlassCard::mapToHost(bar, bar->rect().center());
    lines << QStringLiteral("Browser scroll bar at %1,%2").arg(at.x()).arg(at.y());
    places.insert(QStringLiteral("Browser scroll bar"));
  }
  const QRect viewport = m_tree->viewport()->rect();
  const auto place = [this, &lines, &places](const QString& what, const QRect& rect) {
    const QPoint point = GlassCard::mapToHost(m_tree->viewport(), rect.center());
    lines << QStringLiteral("%1 at %2,%3").arg(what).arg(point.x()).arg(point.y());
    places.insert(what);
  };
  for (QTreeWidgetItemIterator it(m_tree); *it != nullptr; ++it) {
    QTreeWidgetItem* item = *it;
    const QRect row = m_tree->visualItemRect(item);
    // A row in a folded folder, scrolled away, or cut by the view's edge so
    // that its middle is not in the view: no place to click.
    if (row.isEmpty() || !viewport.contains(row.center())) {
      continue;
    }
    const BrowserNode node = nodeOf(item);
    const BrowserDelegate::Parts parts = m_tree->partsOf(item);
    place(QStringLiteral("Browser %1").arg(node.path), parts.name);
    if (item->childCount() > 0) {
      // The disclosure arrow: the middle of the indentation before the row.
      place(QStringLiteral("Browser arrow %1").arg(node.path),
            QRect(row.left() - kIndentation, row.top(), kIndentation, row.height()));
    }
    if (!parts.eye.isEmpty()) {
      place(QStringLiteral("Browser eye %1").arg(node.path), parts.eye);
    }
    if (!parts.radio.isEmpty()) {
      place(QStringLiteral("Browser radio %1").arg(node.path), parts.radio);
    }
    if (!parts.fx.isEmpty()) {
      place(QStringLiteral("Browser fx badge %1").arg(node.path), parts.fx); // mitcad#862
    }
    // A feature row whose result is in another component (mitcad#177).
    if (const QString linked = item->data(kName, kLinkedRole).toString(); !linked.isEmpty()) {
      lines << QStringLiteral("Browser linked %1 to %2").arg(node.path, linked);
    }
  }
  if (m_rows) {
    // With the Rows background (mitcad#419): a place right of every row's
    // box, at the middle row in sight, where a click reaches the 3D view.
    int right = -1;
    std::vector<QRect> whole; // the boxes of rows whose middle is in sight
    for (QModelIndex index = m_tree->indexAt(QPoint(0, 0)); index.isValid(); index = m_tree->indexBelow(index)) {
      const QRect row = m_tree->visualRect(index);
      if (row.top() > viewport.bottom()) {
        break;
      }
      const QRect box = m_tree->rowBox(index);
      right = std::max(right, box.right());
      if (viewport.contains(row.center())) {
        whole.push_back(box);
      }
    }
    if (!whole.empty() && viewport.right() - right > 16) {
      const QRect middle = whole[whole.size() / 2];
      place(QStringLiteral("Browser view beside the rows"),
            QRect(QPoint(right + 1, middle.top()), QPoint(viewport.right(), middle.bottom())));
    }
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  // The places that were valid and are not: logged once, in a fixed order.
  QStringList gone;
  for (const QString& was : std::as_const(m_loggedPlaces)) {
    if (!places.contains(was)) {
      gone << was;
    }
  }
  gone.sort();
  if (logged != m_logged || !gone.isEmpty()) {
    m_logged = logged;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
    for (const QString& was : std::as_const(gone)) {
      qDebug().noquote() << QStringLiteral("%1 out of sight").arg(was);
    }
  }
  m_loggedPlaces = places;
}

} // namespace mitcad

namespace mitcad {

void BrowserPanel::setScrollBarOnLeft(bool left) {
  if (left == m_scrollBarLeft) {
    return;
  }
  m_scrollBarLeft = left;
  if (m_leftBar == nullptr) {
    // The tree's bar keeps the range and the value (the wheel and the keys
    // scroll it); this one mirrors it and scrolls it when dragged.
    m_leftBar = new QScrollBar(Qt::Vertical, this);
    m_leftBar->setObjectName(QStringLiteral("browserLeftScrollBar"));
    m_leftBar->hide();
    QScrollBar* tree = m_tree->verticalScrollBar();
    connect(tree, &QScrollBar::rangeChanged, this, &BrowserPanel::placeLeftScrollBar);
    connect(tree, &QScrollBar::valueChanged, m_leftBar, &QScrollBar::setValue);
    connect(m_leftBar, &QScrollBar::valueChanged, tree, &QScrollBar::setValue);
    new PlaceWatcher(m_tree, [this] { placeLeftScrollBar(); });
  }
  m_tree->setVerticalScrollBarPolicy(left ? Qt::ScrollBarAlwaysOff : Qt::ScrollBarAsNeeded);
  placeLeftScrollBar();
}

void BrowserPanel::placeLeftScrollBar() {
  if (m_leftBar == nullptr) {
    return;
  }
  const QScrollBar* tree = m_tree->verticalScrollBar();
  const bool shown = m_scrollBarLeft && tree->maximum() > tree->minimum();
  m_leftBar->setRange(tree->minimum(), tree->maximum());
  m_leftBar->setPageStep(tree->pageStep());
  m_leftBar->setSingleStep(tree->singleStep());
  m_leftBar->setValue(tree->value());
  const int width = m_leftBar->sizeHint().width();
  // The tree moves right of the bar while it is shown.
  const QMargins margins(shown ? width : 0, 0, 0, 0);
  if (layout()->contentsMargins() != margins) {
    layout()->setContentsMargins(margins);
  }
  if (shown) {
    m_leftBar->setGeometry(0, m_tree->y(), width, m_tree->height());
    m_leftBar->raise();
  }
  if (m_leftBar->isHidden() == shown) {
    m_leftBar->setVisible(shown);
  }
  queueRowsRegion();
  TestSync::singleShot(0, this, [this] { logLayout(); });
}

} // namespace mitcad
