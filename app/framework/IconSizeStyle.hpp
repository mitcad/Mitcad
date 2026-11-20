// SPDX-License-Identifier: MIT
#pragma once

// MitCad's proxy around the application's style: Preferences › Display ›
// Icon size (mitcad#1556) for the icons that Qt's style sizes (mitcad#1561),
// and the highlighted row of a pop-up list as the States convention's band
// (docs/ui-conventions.md, mitcad#2084).
//
// Icon size: IconSizeStyle multiplies the style's icon pixel metrics
// (iconSizeMetric) by Icon size, as uiIconSize does the code's own sizes:
// menus, tool buttons, combo boxes and item views without an icon size of
// their own, a line edit's actions and clear button, tab bars and message
// boxes. At 100 % the metrics are the style's own.
//
// Pop-up lists: the row a drop-down's list (a QComboBox's, editable or
// not), a completer's list or another list in a pop-up window (Command
// Search) highlights is a band of the accent inset 2 px with the control
// radius (paintRowBand), with the row's text in its usual colour, instead
// of the style's square band in the system's highlight colour. Qt draws
// these rows through the combo box's or the view's style (a combo box's
// menu-like list as CE_MenuItem with the combo box as the widget, the
// others as CE_ItemViewItem and PE_PanelItemViewRow with the view), so
// every such list follows it in either theme and layout, with no delegate
// of its own. A list outside a pop-up window keeps the style's look.
//
// Where it is used:
//   - The application's style: MitCad runs its style (the platform's, the
//     shared macOS UI's or the one Windows 10's app mode chose) inside an
//     IconSizeStyle at every Icon size (applyIconSizeToApplicationStyle,
//     after applyTextAndIconSize; until mitcad#2084 only at an Icon size
//     other than 100 %). Code that replaces the application's style later
//     (WindowsTheme's light and dark app mode) does it with
//     setApplicationStyle, which wraps the new style in the same way.
//   - A widget's own style made from the application's (WindowStyle's
//     GapStyle for the main window, the macOS tool buttons and dialog
//     buttons) derives from IconSizeStyle and is made from
//     applicationStyleKey(): a proxy style has no name() of its own, so
//     QApplication::style()->name() is empty while the application's style
//     is wrapped.
// Like the rest of Icon size it is applied when MitCad starts.
//
// The popups' border (mitcad#2081): over Qt's portable style (QFusionStyle:
// Linux, Windows 10's dark app mode, or chosen with `-style`) IconSizeStyle
// also draws the 1 px frame of every popup in the theme's PopupBorder role
// (light or dark by isDarkPalette of the palette it is drawn with), the
// fill staying the palette's: the style's own frame, the window's colour
// darkened, is about 1.2:1 against a dark panel. The frames it draws over:
// PE_PanelMenu and PE_FrameMenu (menus, context menus and submenus; a combo
// box's drop-down list, whose container the style draws as a menu panel)
// and PE_Frame of a popup window (a completer's list, the combo box's
// container). The widget-local styles above, made from the same key, draw
// the border too. Over other styles (windows11, windowsvista, macOS)
// nothing is drawn. A highlighted pop-up row's band is inset inside the
// list, so it leaves the border whole.

#include <QProxyStyle>
#include <QString>

class QPainter;
class QPalette;
class QRect;
class QStyle;
class QWidget;

namespace mitcad {

class IconSizeStyle : public QProxyStyle {
public:
  // Takes `base` (made from a key by QStyleFactory; null: Qt's default).
  explicit IconSizeStyle(QStyle* base = nullptr);

  int pixelMetric(PixelMetric metric, const QStyleOption* option = nullptr,
                  const QWidget* widget = nullptr) const override;
  void drawControl(ControlElement element, const QStyleOption* option, QPainter* painter,
                   const QWidget* widget = nullptr) const override;

  // The base style's primitive (a pop-up list's selected row background
  // without its highlight), then the popups' border over a popup's frame
  // when the base style is Qt's portable style (popupBorderStyle).
  void drawPrimitive(PrimitiveElement element, const QStyleOption* option, QPainter* painter,
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

// Whether `style` (or the style inside it, through proxies) is Qt's
// portable style (QFusionStyle), over which IconSizeStyle draws the popups'
// border.
bool popupBorderStyle(const QStyle* style);

// Whether `element`, drawn for `widget`, is a popup's frame: a menu's panel
// or frame, or the frame of a popup window.
bool popupFrameElement(QStyle::PrimitiveElement element, const QWidget* widget);

// The popups' border: the 1 px band inside `rect` in PopupBorder, light or
// dark by `palette`.
void drawPopupBorder(QPainter* painter, const QRect& rect, const QPalette& palette);

// The key of the application's style, through Icon size's proxy.
QString applicationStyleKey();

// The application's style replaced by the style of `key`, inside an
// IconSizeStyle. False (and nothing replaced) when Qt has no style of that
// key.
bool setApplicationStyle(const QString& key);

// After applyTextAndIconSize, before the first window: the application's
// style inside an IconSizeStyle. Returns what it did for the start-up's log
// ("windows11 with the style's icons at 100%", "windows11 with the style's
// icons at 150%"; "windows11, not wrapped (Qt has no style of that name)").
QString applyIconSizeToApplicationStyle();

// True when the application's style is inside an IconSizeStyle.
bool applicationStyleWrapped();

// True when `widget` is a list in a pop-up window (a drop-down's list, a
// completer's, Command Search's), whose highlighted row IconSizeStyle
// paints as the accent's band.
bool popupList(const QWidget* widget);

} // namespace mitcad
