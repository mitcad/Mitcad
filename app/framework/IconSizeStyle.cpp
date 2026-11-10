// SPDX-License-Identifier: MIT
#include "framework/IconSizeStyle.hpp"

#include <QApplication>
#include <QStyle>
#include <QStyleFactory>

#include "framework/UiSizes.hpp"

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

bool applicationStyleScalesIcons() { return dynamic_cast<const IconSizeStyle*>(QApplication::style()) != nullptr; }

bool setApplicationStyle(const QString& key) {
  QStyle* base = QStyleFactory::create(key);
  if (base == nullptr) {
    return false;
  }
  if (ui_sizes_detail::iconPercent == kDefaultUiSize) {
    QApplication::setStyle(base);
  } else {
    QApplication::setStyle(new IconSizeStyle(base));
  }
  return true;
}

QString applyIconSizeToApplicationStyle() {
  const QString key = applicationStyleKey();
  const int percent = ui_sizes_detail::iconPercent;
  if (percent == kDefaultUiSize) {
    return QStringLiteral("%1, not wrapped (Icon size 100%)").arg(key);
  }
  if (applicationStyleScalesIcons()) {
    return QStringLiteral("%1 with the style's icons at %2%").arg(key).arg(percent);
  }
  if (key.isEmpty() || !setApplicationStyle(key)) {
    return QStringLiteral("%1, not wrapped (Qt has no style of that name)")
        .arg(key.isEmpty() ? QStringLiteral("unknown") : key);
  }
  return QStringLiteral("%1 with the style's icons at %2%").arg(applicationStyleKey()).arg(percent);
}

} // namespace mitcad
