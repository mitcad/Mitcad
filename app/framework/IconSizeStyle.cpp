// SPDX-License-Identifier: MIT
#include "framework/IconSizeStyle.hpp"

#include <algorithm>

#include <QAbstractItemView>
#include <QApplication>
#include <QComboBox>
#include <QPainter>
#include <QPalette>
#include <QRect>
#include <QStyle>
#include <QStyleFactory>
#include <QStyleOption>
#include <QStyleOptionMenuItem>
#include <QStyleOptionViewItem>
#include <QWidget>

#include "framework/Theme.hpp"
#include "framework/UiSizes.hpp"
#include "framework/WindowStyle.hpp"

namespace mitcad {

IconSizeStyle::IconSizeStyle(QStyle* base) : QProxyStyle(base) {}

int IconSizeStyle::pixelMetric(PixelMetric metric, const QStyleOption* option, const QWidget* widget) const {
  if (m_depth > 0 || !iconSizeMetric(metric) || ui_sizes_detail::iconPercent == kDefaultUiSize) {
    return QProxyStyle::pixelMetric(metric, option, widget);
  }
  ++m_depth;
  const int pixels = QProxyStyle::pixelMetric(metric, option, widget);
  --m_depth;
  return pixels > 0 ? iconSizeScaled(pixels) : pixels;
}

namespace {

// A combo box's menu-like list (QComboMenuDelegate: the list of a combo box
// that is not editable where the style shows it as a menu, Qt's own style's) draws
// its rows as menu items with the combo box as the widget; a combo box
// draws no other menu item.
bool comboMenuRow(QStyle::ControlElement element, const QWidget* widget) {
  return element == QStyle::CE_MenuItem && qobject_cast<const QComboBox*>(widget) != nullptr;
}

// The band of a highlighted pop-up row in `row`, inset as a panel's
// (docs/ui-conventions.md, States). A view's row reaches across its
// viewport, over every column.
QRectF popupRowBand(const QRect& row, const QWidget* widget) {
  int left = row.left();
  int right = row.right() + 1;
  if (const auto* view = qobject_cast<const QAbstractItemView*>(widget)) {
    left = 0;
    right = std::max(right, view->viewport()->width());
  }
  return QRectF(left + panel::kRowInset, row.top() + 1, std::max(0, right - left - 2 * panel::kRowInset),
                row.height() - 2);
}

} // namespace

void IconSizeStyle::drawControl(ControlElement element, const QStyleOption* option, QPainter* painter,
                                const QWidget* widget) const {
  if (option != nullptr && option->state.testFlag(State_Selected)) {
    if (comboMenuRow(element, widget)) {
      if (const auto* item = qstyleoption_cast<const QStyleOptionMenuItem*>(option)) {
        paintRowBand(painter, popupRowBand(item->rect, nullptr));
        QStyleOptionMenuItem plain = *item;
        plain.state &= ~(State_Selected | State_MouseOver);
        QProxyStyle::drawControl(element, &plain, painter, widget);
        return;
      }
    }
    if (element == CE_ItemViewItem && popupList(widget)) {
      if (const auto* item = qstyleoption_cast<const QStyleOptionViewItem*>(option)) {
        // Once a row, with its first cell (a tree's other columns come after
        // it and paint over the band only their text).
        if (item->viewItemPosition != QStyleOptionViewItem::Middle &&
            item->viewItemPosition != QStyleOptionViewItem::End) {
          paintRowBand(painter, popupRowBand(item->rect, widget));
        }
        // Without the style's own marks of it: its fill, its focus frame, and
        // the hover tint some styles (Windows 11) add to the row under the
        // mouse, which a drop-down highlights.
        QStyleOptionViewItem plain = *item;
        plain.state &= ~(State_Selected | State_HasFocus | State_MouseOver);
        QProxyStyle::drawControl(element, &plain, painter, widget);
        return;
      }
    }
  }
  QProxyStyle::drawControl(element, option, painter, widget);
}

void IconSizeStyle::drawPrimitive(PrimitiveElement element, const QStyleOption* option, QPainter* painter,
                                  const QWidget* widget) const {
  // A tree's row background (its branch, the room after its last column),
  // which a style fills with the highlight colour on a selected row.
  if (element == PE_PanelItemViewRow && option != nullptr && option->state.testFlag(State_Selected) &&
      popupList(widget)) {
    if (const auto* item = qstyleoption_cast<const QStyleOptionViewItem*>(option)) {
      QStyleOptionViewItem plain = *item;
      plain.state &= ~(State_Selected | State_HasFocus);
      QProxyStyle::drawPrimitive(element, &plain, painter, widget);
      return;
    }
  }
  QProxyStyle::drawPrimitive(element, option, painter, widget);
  // The popups' border over what the style drew: the portable style's own
  // frame is the window's colour darkened. (A combo box's container goes
  // through here twice, its frame being a menu panel there: the same band
  // twice.) A highlighted row's band is inset inside the list, so it leaves
  // the border whole.
  if (option != nullptr && painter != nullptr && popupFrameElement(element, widget) &&
      popupBorderStyle(baseStyle())) {
    drawPopupBorder(painter, option->rect, option->palette);
  }
}

bool popupList(const QWidget* widget) {
  if (qobject_cast<const QAbstractItemView*>(widget) == nullptr) {
    return false;
  }
  const QWidget* window = widget->window();
  return window != nullptr && window->windowType() == Qt::Popup;
}

bool popupBorderStyle(const QStyle* style) {
  for (int depth = 0; style != nullptr && depth < 8; ++depth) {
    if (style->inherits("QFusionStyle")) {
      return true;
    }
    const auto* proxy = qobject_cast<const QProxyStyle*>(style);
    style = proxy != nullptr ? proxy->baseStyle() : nullptr;
  }
  return false;
}

bool popupFrameElement(QStyle::PrimitiveElement element, const QWidget* widget) {
  switch (element) {
  case QStyle::PE_PanelMenu: // a menu's panel; the portable style's combo box popup container
  case QStyle::PE_FrameMenu: // a menu's frame, where the style gives it a width
    return true;
  case QStyle::PE_Frame: // a popup window's frame: a completer's list, the combo box's container
    return widget != nullptr && widget->windowType() == Qt::Popup;
  default:
    return false;
  }
}

void drawPopupBorder(QPainter* painter, const QRect& rect, const QPalette& palette) {
  if (painter == nullptr || rect.width() < 1 || rect.height() < 1) {
    return;
  }
  // Filled bands, not a pen: whole pixels at any device pixel ratio.
  const QColor color = themeColor(ColorRole::PopupBorder, palette);
  painter->fillRect(QRect(rect.left(), rect.top(), rect.width(), 1), color);
  painter->fillRect(QRect(rect.left(), rect.bottom(), rect.width(), 1), color);
  painter->fillRect(QRect(rect.left(), rect.top(), 1, rect.height()), color);
  painter->fillRect(QRect(rect.right(), rect.top(), 1, rect.height()), color);
}

bool iconSizeMetric(QStyle::PixelMetric metric) {
  switch (metric) {
  case QStyle::PM_SmallIconSize:       // menus, combo boxes, item views, line edits
  case QStyle::PM_LargeIconSize:       // icon views
  case QStyle::PM_IconViewIconSize:
  case QStyle::PM_ListViewIconSize:
  case QStyle::PM_ToolBarIconSize:     // tool bars and their buttons
  case QStyle::PM_ButtonIconSize:      // push and tool buttons
  case QStyle::PM_TabBarIconSize:
  case QStyle::PM_MessageBoxIconSize:
  case QStyle::PM_LineEditIconSize:    // a line edit's actions and clear button
    return true;
  default:
    return false;
  }
}

int iconSizeScaled(int pixels) { return uiIconSize(pixels); }

QString styleKey(const QStyle* style) {
  // A proxy's base is made on demand (Qt's default) when it has none.
  for (int depth = 0; style != nullptr && depth < 8; ++depth) {
    if (!style->name().isEmpty()) {
      return style->name();
    }
    const auto* proxy = qobject_cast<const QProxyStyle*>(style);
    style = proxy != nullptr ? proxy->baseStyle() : nullptr;
  }
  return QString();
}

QString applicationStyleKey() { return styleKey(QApplication::style()); }

bool applicationStyleWrapped() { return dynamic_cast<const IconSizeStyle*>(QApplication::style()) != nullptr; }

bool setApplicationStyle(const QString& key) {
  QStyle* base = QStyleFactory::create(key);
  if (base == nullptr) {
    return false;
  }
  QApplication::setStyle(new IconSizeStyle(base));
  return true;
}

QString applyIconSizeToApplicationStyle() {
  const QString key = applicationStyleKey();
  const int percent = ui_sizes_detail::iconPercent;
  if (applicationStyleWrapped()) {
    return QStringLiteral("%1 with the style's icons at %2%").arg(key).arg(percent);
  }
  if (key.isEmpty() || !setApplicationStyle(key)) {
    return QStringLiteral("%1, not wrapped (Qt has no style of that name)")
        .arg(key.isEmpty() ? QStringLiteral("unknown") : key);
  }
  return QStringLiteral("%1 with the style's icons at %2%").arg(applicationStyleKey()).arg(percent);
}

} // namespace mitcad
