// SPDX-License-Identifier: MIT
#pragma once

#include <optional>

#include <QString>

namespace mitcad {

// How the window's panels are arranged: Docked is the classic layout (docks
// around the 3D view, a ribbon in a toolbar), Floating the modern one (a
// full-window 3D view with panels that float over it as rounded cards).
// Floating is the default everywhere (Windows and Linux since mitcad#429;
// Docked keeps working on every platform); Preferences' Layout
// (layoutPreference) chooses another for the next starts, and
// --chrome=docked|floating overrides both for one run. Set it before the
// main window is created.
enum class ChromeStyle { Docked, Floating };

ChromeStyle chromeStyle();
void setChromeStyle(ChromeStyle style);
// The layout when none is chosen: Floating (mitcad#429: on Windows and Linux
// too).
ChromeStyle defaultChromeStyle();
// "docked" or "floating", as --chrome and the setting name them.
QString chromeStyleName(ChromeStyle style);

// The layout chosen in Preferences, Display (mitcad#419): the setting
// appearance/layout, "docked" or "floating"; none when it was never chosen
// (or holds something else), and the platform's default applies. main()
// reads it before the window is made; it takes effect at the next start.
std::optional<ChromeStyle> layoutPreference();
void setLayoutPreference(ChromeStyle style);
// Whether this run's window has one of the two layouts, so that Preferences
// offers the choice: not in the native macOS frontend, which has its own.
bool layoutChoosable();
void setLayoutChoosable(bool choosable);

// The native macOS interface (mitcad#1056; Settings › General in macOS
// builds with the AppKit frontend): the setting appearance/nativeMacInterface,
// off unless chosen, so that MitCad starts in Qt. main() reads it before the
// application object exists; --ui=native|qt (and --chrome, a Qt layout)
// overrides it for one run, and a change applies at the next start.
bool nativeUiPreference();
void setNativeUiPreference(bool native);

// Native Liquid Glass behind the Floating chrome's cards (macOS 26+): each
// card is then a window of its own (GlassCard::setWindowed). On by default
// where the system has it; --no-glass turns it off (the cards paint their
// material themselves, as everywhere else). glassActive() is the answer to
// ask: requested, Floating and available.
bool glassRequested();
void setGlassRequested(bool requested);
bool glassActive();

// The Floating layout's top (mitcad#425): on macOS the title bar row and the
// toolbar's groups are a bar above the view (the content extends under
// the native title bar there); elsewhere only the menu bar is a bar, and the
// title row and each of the toolbar's groups float over the view as cards.
bool floatingTopCards();

} // namespace mitcad
