// SPDX-License-Identifier: MIT
#include "SketchFieldStyle.hpp"

#include "../framework/Theme.hpp"

namespace mitcad::sketch {

QString valueFieldStyle(bool invalid) {
  const auto light = [](ColorRole role) { return themeColor(role, false).name(); };
  return QStringLiteral("QLineEdit { background: %1; color: %2; border: 1px solid %3;"
                        " border-radius: 3px; padding: 1px 4px; selection-background-color: %4;"
                        " selection-color: %5; }")
      .arg(invalid ? light(ColorRole::ErrorFill) : QStringLiteral("rgba(255, 255, 255, 240)"),
           light(ColorRole::Text), invalid ? light(ColorRole::ErrorBorder) : light(ColorRole::Hover),
           light(ColorRole::Action), light(ColorRole::AccentText));
}

} // namespace mitcad::sketch
