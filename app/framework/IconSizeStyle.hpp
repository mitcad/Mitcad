// SPDX-License-Identifier: MIT
#pragma once

// Preferences › Display › Icon size (mitcad#1556) for the icons that Qt's
// style sizes (mitcad#1561): menus, tool buttons, combo boxes and item
// views without an icon size of their own, a line edit's actions and clear
// button, tab bars and message boxes.
//
// IconSizeStyle is a proxy style that multiplies the style's icon pixel
// metrics (iconSizeMetric) by Icon size, as uiIconSize does the code's own
// sizes. Where it is used:
//   - The application's style: at an Icon size other than 100 % MitCad
//     runs its style (the platform's, the shared macOS UI's or the one
//     Windows 10's app mode chose) inside an IconSizeStyle
//     (applyIconSizeToApplicationStyle, after applyTextAndIconSize). At
//     100 % it is not wrapped, so the application's style is Qt's own as
//     before. Code that replaces the application's style later
//     (WindowsTheme's light and dark app mode) does it with
//     setApplicationStyle, which wraps the new style in the same way.
//   - A widget's own style made from the application's (WindowStyle's
//     GapStyle for the main window, the macOS tool buttons and dialog
//     buttons) derives from IconSizeStyle and is made from
//     applicationStyleKey(): a proxy style has no name() of its own, so
//     QApplication::style()->name() is empty while the application's style
//     is wrapped.
// Like the rest of Icon size it is applied when MitCad starts.

#include <QProxyStyle>
#include <QString>

class QStyle;

namespace mitcad {

class IconSizeStyle : public QProxyStyle {
public:
  // Takes `base` (made from a key by QStyleFactory; null: Qt's default).
  explicit IconSizeStyle(QStyle* base = nullptr);

  int pixelMetric(PixelMetric metric, const QStyleOption* option = nullptr,
                  const QWidget* widget = nullptr) const override;

private:
  // The base style computes some icon metrics from others through its
  // proxy (a tab bar's or a list view's from the small icon size): those
  // nested queries are answered unscaled, so that the size is multiplied
  // once.
  mutable int m_depth = 0;
};

// The icon pixel metrics that Icon size multiplies.
bool iconSizeMetric(QStyle::PixelMetric metric);

// `pixels` times this run's Icon size, rounded to whole pixels (at least 1;
// as uiIconSize).
int iconSizeScaled(int pixels);

// The key QStyleFactory makes `style` from: its name(), or the name of the
// style inside it when it is a proxy (IconSizeStyle, GapStyle); empty when
// none is known.
QString styleKey(const QStyle* style);

// The key of the application's style, through Icon size's proxy.
QString applicationStyleKey();

// The application's style replaced by the style of `key`, inside an
// IconSizeStyle when Icon size is not 100 %. False (and nothing replaced)
// when Qt has no style of that key.
bool setApplicationStyle(const QString& key);

// After applyTextAndIconSize, before the first window: the application's
// style inside an IconSizeStyle when Icon size is not 100 %. Returns what
// it did for the start-up's log ("windows11, not wrapped (Icon size
// 100%)", "windows11 with the style's icons at 150%").
QString applyIconSizeToApplicationStyle();

// True when the application's style is inside an IconSizeStyle.
bool applicationStyleScalesIcons();

} // namespace mitcad
