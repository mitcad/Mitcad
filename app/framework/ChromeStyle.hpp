// SPDX-License-Identifier: MIT
#pragma once

namespace mitcad {

// How the window's panels are arranged: Docked is the classic layout (docks
// around the 3D view, a ribbon in a toolbar), Floating is the macOS one (a
// full-window 3D view with panels that float over it as rounded cards).
// Floating is the default on macOS, Docked elsewhere; --chrome=docked|floating
// overrides it. Set it before the main window is created.
enum class ChromeStyle { Docked, Floating };

ChromeStyle chromeStyle();
void setChromeStyle(ChromeStyle style);

// Native Liquid Glass behind the Floating chrome's cards (macOS 26+): each
// card is then a window of its own (GlassCard::setWindowed). On by default
// where the system has it; --no-glass turns it off (the cards paint their
// material themselves, as everywhere else). glassActive() is the answer to
// ask: requested, Floating and available.
bool glassRequested();
void setGlassRequested(bool requested);
bool glassActive();

} // namespace mitcad
