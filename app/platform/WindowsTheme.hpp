// SPDX-License-Identifier: MIT
#pragma once

#include <QtGlobal>

// Windows 10's dark app mode (mitcad#1282, GitHub mitcad/Mitcad#138). On
// Windows 11 Qt's default style (windows11) follows the system's dark app
// mode; on Windows 10 the default style (windowsvista) keeps a light
// palette whatever the app mode, so the Start page, the panels, the cards
// and the dialogs stayed light. Portable code calls this without #ifdefs:
// on other platforms it is an inline stub that does nothing. The Windows
// implementation is platform/windows/WindowsTheme.cpp.
namespace mitcad::win {

#ifdef Q_OS_WIN

// On Windows 10 (build < 22000, decided at run time), while the app mode is
// dark (QStyleHints::colorScheme() == Qt::ColorScheme::Dark): Qt's own
// style, which draws every control from the palette on every platform (its
// class QFusionStyle), with a dark palette of the theme's neutral roles
// (framework/Theme), whose window is dark for isDarkPalette; and the default
// style with its own palette again when the app mode is light. Follows a
// change of the app mode while Mitcad runs (QStyleHints::
// colorSchemeChanged). Only over the platform's default style
// (windowsvista): a style chosen with -style or QT_STYLE_OVERRIDE stays.
// Call it once, after the QApplication exists and before the first window,
// so that its switch comes before the widgets' own reactions to the change
// (framework/Theme's ThemeWatcher). For tests, MITCAD_TEST_WINDOWS10=1
// takes this path on Windows 11 too (the build VM's), over its default
// style (windows11). Logs "Windows 10 app mode: ..." (app/COMMANDS.md).
void followAppColorScheme();

#else

inline void followAppColorScheme() {}

#endif

} // namespace mitcad::win
