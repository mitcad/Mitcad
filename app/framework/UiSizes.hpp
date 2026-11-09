// SPDX-License-Identifier: MIT
#pragma once

// Preferences › Display › Text size, Icon size and Font (mitcad#1556,
// GitHub mitcad/Mitcad#207), on top of Interface size (InterfaceSize.hpp,
// Qt's scale factor for everything):
//   - Text size, 80 to 200 % in steps of 5 % (100 % by default), scales the
//     application font; the code's own fonts are derived from the widgets'
//     fonts, so they follow it.
//   - Icon size, likewise, scales the code's fixed icon sizes through
//     uiIconSize(); the icons are drawn from their SVG sources at that size.
//   - Font: the system's (empty, the default) or an installed family,
//     the application font's family at Text size's point size.
// Kept in the application's settings (appearance/textSize,
// appearance/iconSize, appearance/fontFamily) and applied when MitCad
// starts (applyTextAndIconSize, after the application object exists and
// before the first window); a change takes effect at the next start.
//
// What this run applied is kept in inline variables, so that a widget
// anywhere (also in test_app, which links only some of the app's sources)
// reads it without linking UiSizes.cpp.

#include <algorithm>

#include <QSize>
#include <QString>
#include <QVariant>

class QSettings;

namespace mitcad {

inline constexpr int kUiSizeMinimum = 80;
inline constexpr int kUiSizeMaximum = 200;
inline constexpr int kUiSizeStep = 5;
inline constexpr int kDefaultUiSize = 100;

namespace ui_sizes_detail {
// This run's Text size and Icon size in percent (applyTextAndIconSize).
inline int textPercent = kDefaultUiSize;
inline int iconPercent = kDefaultUiSize;
// The application font's point size before Text size scaled it (0: none
// known, e.g. a pixel-sized font).
inline double platformPointSize = 0.0;
// True when the user's Font family is the application font's.
inline bool fontChosen = false;
} // namespace ui_sizes_detail

// A fixed icon size of the code, in logical pixels, at this run's Icon
// size (at least 1).
inline int uiIconSize(int pixels) {
  const int percent = ui_sizes_detail::iconPercent;
  return percent == kDefaultUiSize ? pixels : std::max(1, (pixels * percent + 50) / 100);
}
inline QSize uiIconSize(int width, int height) { return QSize(uiIconSize(width), uiIconSize(height)); }
inline QSize uiIconSize(const QSize& size) { return uiIconSize(size.width(), size.height()); }

// A fixed size of the code that holds text or icons (a row's, a button's,
// a header's height): it grows with the larger of Text size and Icon size
// and never becomes smaller than at 100 %, so that nothing is clipped at
// 80 %.
inline int uiRoomSize(int pixels) {
  const int percent = std::max(ui_sizes_detail::textPercent, ui_sizes_detail::iconPercent);
  return percent <= kDefaultUiSize ? pixels : (pixels * percent + 50) / 100;
}

// This run's Text size as a factor, for a lower limit the code puts under a
// font size of its own (8 pt at 100 % is 6.4 pt at 80 %).
inline double uiTextScale() { return ui_sizes_detail::textPercent / 100.0; }

// A kept value as a size: 80 to 200 in steps of 5, else 100.
int uiSizeFromValue(const QVariant& value);

// The sizes and the family kept in `settings`; without one, the
// application's (QSettings).
int savedTextSize(const QSettings& settings);
int savedTextSize();
int savedIconSize(const QSettings& settings);
int savedIconSize();
// Empty: the system's font.
QString savedFontFamily(const QSettings& settings);
QString savedFontFamily();

// Save a choice for the next start; false when it was already so (or a size
// is not 80 to 200 in steps of 5). An empty family is the system's font.
bool saveTextSize(QSettings& settings, int percent);
bool saveTextSize(int percent);
bool saveIconSize(QSettings& settings, int percent);
bool saveIconSize(int percent);
bool saveFontFamily(QSettings& settings, const QString& family);
bool saveFontFamily(const QString& family);

// What applyTextAndIconSize set, for the start-up's log line.
struct TextAndIconSize {
  int textPercent = kDefaultUiSize;
  int iconPercent = kDefaultUiSize;
  // The family asked for (empty: the system's) and whether it is
  // installed; one that is not leaves the system's font.
  QString family;
  bool familyInstalled = false;
};

// After the application object exists, before the first window: the
// application font at Text size (and the chosen family), and uiIconSize at
// Icon size. At 100 % with the system's font the application font is not
// touched.
TextAndIconSize applyTextAndIconSize(int textPercent, int iconPercent, const QString& family);
TextAndIconSize applyTextAndIconSize();

// "Text size 80%, Icon size 150%, Font DejaVu Sans" (or "Font the
// system's", "Font X (not installed; the system's)").
QString textAndIconSizeDescription(const TextAndIconSize& sizes);

// "DejaVu Sans, 7.2 pt (Text size 80% of 9 pt)": the application font's
// family as drawn and its point size.
QString applicationFontDescription();

} // namespace mitcad
