// SPDX-License-Identifier: MIT
#include "ChromeStyle.hpp"

#include <QSettings>
#include <QtGlobal>

#include "../platform/MacChrome.hpp"

namespace mitcad {
namespace {

const QString kLayoutKey = QStringLiteral("appearance/layout");
const QString kNativeUiKey = QStringLiteral("appearance/nativeMacInterface");

ChromeStyle g_style = defaultChromeStyle();
bool g_glass = true;
bool g_layoutChoosable = true;

} // namespace

ChromeStyle chromeStyle() { return g_style; }

void setChromeStyle(ChromeStyle style) { g_style = style; }

ChromeStyle defaultChromeStyle() {
  // Floating everywhere: macOS from the start, Windows and Linux since
  // mitcad#429.
  return ChromeStyle::Floating;
}

QString chromeStyleName(ChromeStyle style) {
  return style == ChromeStyle::Floating ? QStringLiteral("floating") : QStringLiteral("docked");
}

std::optional<ChromeStyle> layoutPreference() {
  const QString value = QSettings().value(kLayoutKey).toString();
  if (value == QLatin1String("docked")) {
    return ChromeStyle::Docked;
  }
  if (value == QLatin1String("floating")) {
    return ChromeStyle::Floating;
  }
  return std::nullopt;
}

void setLayoutPreference(ChromeStyle style) { QSettings().setValue(kLayoutKey, chromeStyleName(style)); }

bool layoutChoosable() { return g_layoutChoosable; }

void setLayoutChoosable(bool choosable) { g_layoutChoosable = choosable; }

bool nativeUiPreference() { return QSettings().value(kNativeUiKey, false).toBool(); }

void setNativeUiPreference(bool native) { QSettings().setValue(kNativeUiKey, native); }

bool glassRequested() { return g_glass; }

void setGlassRequested(bool requested) { g_glass = requested; }

bool glassActive() { return g_glass && g_style == ChromeStyle::Floating && mac::glassAvailable(); }

bool floatingTopCards() {
#ifdef MITCAD_MAC_UI
  return false;
#else
  return true;
#endif
}

} // namespace mitcad
