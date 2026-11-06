// SPDX-License-Identifier: MIT
#pragma once

#include <QString>

namespace mitcad {

// MitCad's own theme (mitcad#1330, GitHub mitcad/Mitcad#151): Preferences ›
// Display › Theme, the colours of MitCad's panels, cards, Browser,
// timeline, Start page and dialogs whatever the desktop's colour scheme.
// System follows the platform's scheme as before; Light and Dark ask the
// platform for that scheme (QStyleHints::setColorScheme), and where the
// platform does not change its palette for it (Qt's generic platform theme
// on Linux, a style that keeps its own palette), MitCad sets a palette of
// the theme's neutral roles itself (themePalette, framework/Theme). The 3D
// view's background is a setting of its own (Follow System follows this
// palette).
enum class ThemeChoice { System, Light, Dark };

// The setting appearance/theme: "system", "light" or "dark"; absent (or
// anything else): System.
ThemeChoice themePreference();
void setThemePreference(ThemeChoice choice);
// "system", "light" or "dark", as the setting names them, and back
// (System for any other text).
QString themeChoiceName(ThemeChoice choice);
ThemeChoice themeChoiceFromName(const QString& name);

// At start, after the QApplication exists and Windows 10's path is set up
// (platform/WindowsTheme.hpp, whose reaction to a scheme change comes
// first), before the first window: the stored choice. System asks nothing
// of the platform and logs "Theme: system (light)" (or dark) at once.
void startThemePreference();

// Applies a choice at once, without a restart (Preferences, when it
// changes): MitCad's own palette, if it set one, goes first; the choice is
// asked of the platform (System: its own scheme again,
// QStyleHints::unsetColorScheme); a palette that is not light or dark as
// wanted a moment later (250 ms, or as soon as the platform's scheme
// changes and its palette follows) is replaced by MitCad's own. Logs
// "Theme: dark (preference)", "Theme: dark (preference; MitCad's own
// palette, window #… over the platform's light one)" or
// "Theme: system (light)" once it is settled.
void applyThemePreference(ThemeChoice choice);

} // namespace mitcad
