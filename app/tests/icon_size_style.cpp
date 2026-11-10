// SPDX-License-Identifier: MIT
// Icon size for the icons Qt's style sizes (mitcad#1561,
// framework/IconSizeStyle.hpp): IconSizeStyle multiplies the style's icon
// pixel metrics by Icon size, once (a metric the base style computes from
// another through its proxy is not multiplied twice), and leaves the other
// metrics alone; a combo box and a menu in it take their icons at that
// size; the style keys are found through the proxy; the application's
// style is wrapped only at an Icon size other than 100 %. The run's Icon
// size and the application's style are put back afterwards.
#include "framework/IconSizeStyle.hpp"

#include <algorithm>
#include <cstdio>
#include <memory>

#include <QApplication>
#include <QComboBox>
#include <QMenu>
#include <QStyle>
#include <QStyleFactory>
#include <QStringList>

#include "framework/UiSizes.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "icon_size_style.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

// `pixels` times `percent`, rounded to whole pixels: counted here, not by
// uiIconSize.
int times(int pixels, int percent) { return std::max(1, (pixels * percent + 50) / 100); }

// The key of Qt's own portable style, found by its class (as
// WindowsTheme.cpp does), else the first style Qt offers.
QString portableStyleKey() {
  const QStringList keys = QStyleFactory::keys();
  for (const QString& key : keys) {
    const std::unique_ptr<QStyle> style(QStyleFactory::create(key));
    if (style != nullptr && qstrcmp(style->metaObject()->className(), "QFusionStyle") == 0) {
      return key;
    }
  }
  return keys.isEmpty() ? QString() : keys.constFirst();
}

void metricTests() {
  CHECK(mitcad::iconSizeMetric(QStyle::PM_SmallIconSize));
  CHECK(mitcad::iconSizeMetric(QStyle::PM_ToolBarIconSize));
  CHECK(mitcad::iconSizeMetric(QStyle::PM_ButtonIconSize));
  CHECK(mitcad::iconSizeMetric(QStyle::PM_ListViewIconSize));
  CHECK(mitcad::iconSizeMetric(QStyle::PM_TabBarIconSize));
  CHECK(mitcad::iconSizeMetric(QStyle::PM_MessageBoxIconSize));
  CHECK(mitcad::iconSizeMetric(QStyle::PM_LineEditIconSize));
  CHECK(!mitcad::iconSizeMetric(QStyle::PM_DefaultFrameWidth));
  CHECK(!mitcad::iconSizeMetric(QStyle::PM_ScrollBarExtent));
  CHECK(!mitcad::iconSizeMetric(QStyle::PM_DockWidgetSeparatorExtent));
}

void styleTests() {
  namespace detail = mitcad::ui_sizes_detail;
  const int icon = detail::iconPercent;
  const QString key = portableStyleKey();
  const std::unique_ptr<QStyle> plain(QStyleFactory::create(key));
  CHECK(plain != nullptr);
  if (plain == nullptr) {
    return;
  }
  const int small = plain->pixelMetric(QStyle::PM_SmallIconSize);
  const int tabBar = plain->pixelMetric(QStyle::PM_TabBarIconSize);
  const int listView = plain->pixelMetric(QStyle::PM_ListViewIconSize);
  const int toolBar = plain->pixelMetric(QStyle::PM_ToolBarIconSize);
  const int frame = plain->pixelMetric(QStyle::PM_DefaultFrameWidth);
  CHECK(small > 0);

  const std::unique_ptr<mitcad::IconSizeStyle> scaling(
      new mitcad::IconSizeStyle(QStyleFactory::create(key)));
  // Its key is the style's inside it.
  CHECK(mitcad::styleKey(scaling.get()).compare(plain->name(), Qt::CaseInsensitive) == 0);
  CHECK(mitcad::styleKey(plain.get()) == plain->name());
  CHECK(mitcad::styleKey(nullptr).isEmpty());

  // 100 %: the style's own sizes.
  detail::iconPercent = 100;
  CHECK(scaling->pixelMetric(QStyle::PM_SmallIconSize) == small);
  CHECK(scaling->pixelMetric(QStyle::PM_TabBarIconSize) == tabBar);

  for (const int percent : {150, 80, 200}) {
    detail::iconPercent = percent;
    CHECK(scaling->pixelMetric(QStyle::PM_SmallIconSize) == times(small, percent));
    CHECK(scaling->pixelMetric(QStyle::PM_ToolBarIconSize) == times(toolBar, percent));
    // Computed by the style from the small icon size through its proxy:
    // multiplied once.
    CHECK(scaling->pixelMetric(QStyle::PM_TabBarIconSize) == times(tabBar, percent));
    CHECK(scaling->pixelMetric(QStyle::PM_ListViewIconSize) == times(listView, percent));
    // Not an icon's size.
    CHECK(scaling->pixelMetric(QStyle::PM_DefaultFrameWidth) == frame);
    // A combo box and a menu without a size of their own.
    QComboBox combo;
    combo.setStyle(scaling.get());
    CHECK(combo.iconSize() == QSize(times(small, percent), times(small, percent)));
    QMenu menu;
    menu.setStyle(scaling.get());
    CHECK(menu.style()->pixelMetric(QStyle::PM_SmallIconSize, nullptr, &menu) == times(small, percent));
    // A size the code set stays as it is (uiIconSize already has Icon size).
    combo.setIconSize(QSize(20, 20));
    CHECK(combo.iconSize() == QSize(20, 20));
  }
  detail::iconPercent = icon;
}

void applicationTests() {
  namespace detail = mitcad::ui_sizes_detail;
  const int icon = detail::iconPercent;
  const QString kept = mitcad::applicationStyleKey();
  const QString key = portableStyleKey();
  CHECK(!kept.isEmpty());

  // 100 %: the application's style is not wrapped.
  detail::iconPercent = 100;
  CHECK(mitcad::setApplicationStyle(key));
  CHECK(!mitcad::applicationStyleScalesIcons());
  CHECK(mitcad::applyIconSizeToApplicationStyle().endsWith(QStringLiteral(", not wrapped (Icon size 100%)")));
  CHECK(!mitcad::applicationStyleScalesIcons());
  const int small = QApplication::style()->pixelMetric(QStyle::PM_SmallIconSize);

  // 150 %: wrapped once, the key still the style's, its icons larger.
  detail::iconPercent = 150;
  const QString applied = mitcad::applyIconSizeToApplicationStyle();
  CHECK(applied.endsWith(QStringLiteral(" with the style's icons at 150%")));
  CHECK(mitcad::applicationStyleScalesIcons());
  CHECK(mitcad::applicationStyleKey().compare(key, Qt::CaseInsensitive) == 0);
  CHECK(QApplication::style()->pixelMetric(QStyle::PM_SmallIconSize) == times(small, 150));
  const QStyle* wrapped = QApplication::style();
  CHECK(mitcad::applyIconSizeToApplicationStyle() == applied);
  CHECK(QApplication::style() == wrapped);
  QComboBox combo;
  CHECK(combo.iconSize() == QSize(times(small, 150), times(small, 150)));

  // Another style by its key (as Windows 10's app mode swaps them): wrapped
  // too.
  CHECK(mitcad::setApplicationStyle(key));
  CHECK(mitcad::applicationStyleScalesIcons());
  CHECK(QApplication::style() != wrapped);
  CHECK(!mitcad::setApplicationStyle(QStringLiteral("No Such Style 1561")));
  CHECK(mitcad::applicationStyleScalesIcons());

  // Back as it was.
  detail::iconPercent = 100;
  QApplication::setStyle(kept);
  CHECK(!mitcad::applicationStyleScalesIcons());
  detail::iconPercent = icon;
}

} // namespace

int iconSizeStyleTests() {
  metricTests();
  styleTests();
  applicationTests();
  return failures;
}
