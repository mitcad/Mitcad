// SPDX-License-Identifier: MIT
#pragma once

#include <QIcon>
#include <QString>
#include <QtGlobal>

class QMenu;
class QWidget;

// The macOS-only parts of the interface that need AppKit. Portable code calls
// these functions without #ifdefs: on other platforms they are inline stubs
// that do nothing. The macOS implementation is platform/macos/MacChrome.mm.
namespace mitcad::mac {

#ifdef Q_OS_MACOS

// An SF Symbol (e.g. "arrow.uturn.backward") as an icon: a template image
// drawn in the application palette's ButtonText colour (the palette's
// disabled text colour in the Disabled mode, HighlightedText in Selected),
// rendered when it is asked for at the size and device pixel ratio it is
// asked for, so that it is sharp on Retina screens and follows palette
// changes (dark mode, accent). `fallback` when the system has no such symbol.
QIcon symbolIcon(const QString& symbol, const QIcon& fallback = {});

// Makes the images of a menu's items (and its submenus') visible: since
// macOS 27 AppKit hides them unless an item asks for them. Best effort: Qt
// creates the native items when the menu is shown or changed, so the menu is
// set up again whenever it is about to show. A no-op before macOS 27.
void showMenuImages(QMenu* menu);

// Hides the native window title text and makes the title bar transparent, for
// a window whose client area extends under the title bar and draws its own
// title (the traffic lights stay). The window must have a native handle.
void hideNativeTitle(QWidget* window);

// Conventions of the menu bar and the standard About panel.

// Hides the images of a menu's items (and its submenus'): the menus of the
// menu bar show none (the Human Interface Guidelines; macOS 27 hides them
// itself) although the actions have icons for the ribbon, whose menus keep
// them. Best effort like showMenuImages: set up again whenever a menu is
// about to show.
void hideMenuImages(QMenu* menu);

// The standard About panel (name, icon and version of the application, the
// copyright of its bundle) with `credits`, rich text as HTML.
void showAboutPanel(const QString& credits);

// Liquid Glass behind a card (macOS 26+). Qt draws the 3D view with OpenGL,
// which a native view in the same window cannot show through, so a card that
// wants real glass is a window of its own: `card` is a frameless, translucent
// Qt top-level window (GlassCard::setWindowed), which is made a child window
// of `mainWindow` here, with an NSGlassEffectView of `cornerRadius` below Qt's
// view in it. The window is without a shadow of Qt's own, and gets the
// system's; it does not take part in the Window menu, window cycling or
// Mission Control. Safe to call again for the same card (as it is shown
// again); both windows must have native handles.
bool glassAvailable();
void attachGlassWindow(QWidget* card, QWidget* mainWindow, qreal cornerRadius);
// The radius of the card's glass (a capsule's follows its height).
void setGlassCornerRadius(QWidget* card, qreal cornerRadius);

// Saves what is on the screen of the application's own windows, as the window
// server composes them (child windows and glass included; the Qt widgets'
// grab() has neither), as a PNG. No screen recording permission is needed for
// a process's own windows. False if it failed.
bool captureOwnWindows(const QString& path);

#else

inline bool glassAvailable() { return false; }
inline void attachGlassWindow(QWidget*, QWidget*, qreal) {}
inline void setGlassCornerRadius(QWidget*, qreal) {}
inline bool captureOwnWindows(const QString&) { return false; }
inline QIcon symbolIcon(const QString&, const QIcon& fallback = {}) { return fallback; }
inline void showMenuImages(QMenu*) {}
inline void hideNativeTitle(QWidget*) {}
inline void hideMenuImages(QMenu*) {}
inline void showAboutPanel(const QString&) {}

#endif

} // namespace mitcad::mac
