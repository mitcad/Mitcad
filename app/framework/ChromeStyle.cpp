// SPDX-License-Identifier: MIT
#include "ChromeStyle.hpp"

#include <QtGlobal>

#include "../platform/MacChrome.hpp"

namespace mitcad {
namespace {

#ifdef Q_OS_MACOS
ChromeStyle g_style = ChromeStyle::Floating;
#else
ChromeStyle g_style = ChromeStyle::Docked;
#endif
bool g_glass = true;

} // namespace

ChromeStyle chromeStyle() { return g_style; }

void setChromeStyle(ChromeStyle style) { g_style = style; }

bool glassRequested() { return g_glass; }

void setGlassRequested(bool requested) { g_glass = requested; }

bool glassActive() { return g_glass && g_style == ChromeStyle::Floating && mac::glassAvailable(); }

} // namespace mitcad
