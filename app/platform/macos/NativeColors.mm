// SPDX-License-Identifier: MIT
#include "NativeColors.hpp"

namespace mitcad::mac {
namespace {

// The table's light and dark values as one color that follows the
// appearance it is drawn in.
NSColor* tableColor(ColorRole role) {
  const QColor day = themeColor(role, false);
  const QColor night = themeColor(role, true);
  if (day == night) {
    return srgbColor(day);
  }
  return [NSColor colorWithName:nil
                dynamicProvider:^NSColor*(NSAppearance* appearance) {
                  NSAppearanceName match =
                      [appearance bestMatchFromAppearancesWithNames:@[ NSAppearanceNameAqua, NSAppearanceNameDarkAqua ]];
                  return srgbColor([match isEqualToString:NSAppearanceNameDarkAqua] ? night : day);
                }];
}

} // namespace

NSColor* srgbColor(const QColor& color) {
  const QColor rgb = color.toRgb();
  return [NSColor colorWithSRGBRed:rgb.redF() green:rgb.greenF() blue:rgb.blueF() alpha:rgb.alphaF()];
}

NSColor* roleColor(ColorRole role) {
  switch (role) {
  case ColorRole::ErrorText:
  case ColorRole::ErrorBorder:
  case ColorRole::ErrorBadge:
  case ColorRole::DropRefused:
    return NSColor.systemRedColor;
  case ColorRole::WarningBadge:
    return NSColor.systemYellowColor;
  case ColorRole::SuccessBadge:
    return NSColor.systemGreenColor;
  case ColorRole::InfoBadge:
    return NSColor.systemBlueColor;
  case ColorRole::Action:
    return NSColor.controlAccentColor;
  case ColorRole::Text:
    return NSColor.labelColor;
  case ColorRole::InfoText:
  case ColorRole::TextSecondary:
  case ColorRole::TextHidden:
    return NSColor.secondaryLabelColor;
  case ColorRole::Line:
    return NSColor.separatorColor;
  default:
    return tableColor(role);
  }
}

NSColor* noticeTint(NoticeLevel level) {
  switch (level) {
  case NoticeLevel::Error:
    return roleColor(ColorRole::ErrorBadge);
  case NoticeLevel::Warning:
    return roleColor(ColorRole::WarningBadge);
  case NoticeLevel::Information:
    return roleColor(ColorRole::InfoBadge);
  case NoticeLevel::Success:
    return roleColor(ColorRole::SuccessBadge);
  default:
    return roleColor(ColorRole::InfoText);
  }
}

} // namespace mitcad::mac
