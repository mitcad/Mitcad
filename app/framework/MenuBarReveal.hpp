// SPDX-License-Identifier: MIT
#pragma once

#include <functional>

#include <QHash>
#include <QList>
#include <QMetaObject>
#include <QObject>
#include <QPointer>
#include <QString>
#include <QStringList>
#include <QToolButton>

class QEvent;
class QMenu;
class QMenuBar;
class QTimer;

namespace mitcad {

// The menu bar hidden (mitcad#1620, app/COMMANDS.md "The hidden menu
// bar"): Preferences › Display › Show the menu bar, on Windows and Linux
// (on macOS the system's menu bar has the menus: not offered there). The
// setting is "view/menuBar", shown by default. Plain Qt Core.
struct MenuBarSettings {
  // Windows and Linux.
  static bool offered();
  static QString settingsKey();
  // Shown unless the user cleared it; always shown where not offered.
  static bool loadShown();
  // Saves it at once and tells MenuBarNotifier; false when it was already so.
  static bool saveShown(bool shown);
};

// Tells the windows, their ☰ buttons and MenuBarReveal that the setting
// changed (saveShown).
class MenuBarNotifier : public QObject {
  Q_OBJECT

public:
  static MenuBarNotifier& instance();

signals:
  void shownChanged(bool shown);
};

// The menu bar's menus, in its order: the entries with a menu, shown, their
// names without the mnemonic's "&" ("File", "Edit", "View", "Tools",
// "Window", "Help").
QStringList menuBarEntries(const QMenuBar& bar);
// The entries of a menu as the UI tests' logs give them: the shown
// actions that are not separators, without the mnemonic's "&".
QStringList menuEntries(const QMenu& menu);
// The menu's first entry the keys can reach (shown, no separator, enabled)
// made its active one, as a menu bar's menu opened by its keys has it: the
// arrow keys and Enter go on from there.
void activateFirstEntry(QMenu& menu);

// The ☰ button of the title row (Floating) and the Docked toolbar, shown
// while the menu bar is hidden (MainWindow shows it, wanted()): its menu
// has the window's menu bar's menus as submenus, the same menus (the same
// actions, made when it opens). Logs "Menu bar button at x,y" (in the
// window) as it shows or moves, "Context menu: File | Edit | …" as its
// menu opens and "Context menu File: …" as one of the submenus opens from
// it (ui_menu_choose).
class MenuBarButton : public QToolButton {
  Q_OBJECT

public:
  MenuBarButton(QMenuBar* bar, int iconSize, QWidget* parent = nullptr);

  // Whether the button is to show: the setting offered and cleared.
  static bool wanted();
  // Opens its menu under it, as a click does, with the first menu active
  // (the keys go on from there); with `submenu` (one of the bar's menus)
  // that one active and open beside it, as the keys open it.
  void openMenu(QMenu* submenu = nullptr);

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;
  void showEvent(QShowEvent* event) override;
  void moveEvent(QMoveEvent* event) override;
  void resizeEvent(QResizeEvent* event) override;

private:
  void fillMenu();
  void schedulePlace();
  void logPlace();

  QPointer<QMenuBar> m_bar;
  QMenu* m_menu = nullptr;
  QTimer* m_placeTimer = nullptr;
  QString m_lastPlace;
  QList<QMetaObject::Connection> m_submenuLogs;
  // The submenu openMenu opened by the keys, until the menu's window has
  // the focus (eventFilter).
  QPointer<QMenu> m_keysSubmenu;
};

// Keeps the watched menu bars hidden while the setting says so, and shows
// one again while the user reaches its menus by the keys (one for the whole
// application, an event filter on it only while the bars are hidden). A
// hidden bar is no higher than nothing (its maximum height 0) rather than
// hidden: Qt takes the shortcuts of a hidden menu bar's menus away, so its
// commands' keys and its menus' mnemonics would stop working.
//   - Alt pressed and released alone in a window shows its menu bar; where
//     the style goes to the menu bar on Alt itself (Qt's portable style on
//     Linux, the Windows styles), the bar then has the keyboard with an
//     entry highlighted, as with a shown bar; elsewhere its first menu
//     (File) opens.
//   - Alt with a menu's mnemonic (Alt+F) shows the bar before the mnemonic
//     opens that menu.
// The bar hides again once no menu of it is open and it has neither the
// keyboard nor the pointer (a menu's command chosen, Esc, a click
// elsewhere). While the shown bar has the keyboard, Esc is the bar's (it
// leaves the bar), not the window's Cancel. Logs "Menu bar shown (Alt)",
// "Menu bar shown (Alt+<key>)", "Menu bar File opened" and "Menu bar
// hidden again".
//
// A bar watched with an opener never shows (the Floating layout of Windows
// and Linux, mitcad#1990: the top bar's File, ? and ☰ hold the menus): it is
// hidden outright whatever the setting (a zero maximum height alone left
// the bar laid out at its full height on Windows, a row above the top bar),
// hidden again should anything show it, and its menus' commands with a
// shortcut are the window's actions too (keepShortcuts), so that their keys
// work although Qt takes them from a hidden bar's menus. Alt does not take
// it to the keyboard, and its keys go to the opener instead: Alt+<a menu's
// mnemonic> gives that menu (the key goes no further), Alt alone gives
// none. Logged by the opener.
class MenuBarReveal : public QObject {
  Q_OBJECT

public:
  static MenuBarReveal& instance();

  // `menu`: the bar's menu of the mnemonic pressed, null for Alt alone;
  // `how`: "Alt+F" or "Alt".
  using Opener = std::function<void(QMenu* menu, const QString& how)>;

  // Hidden while the setting says so, from now on; with an opener always
  // (a bar watched already gets it).
  void watch(QMenuBar* bar, Opener opener = {});
  // Hidden now (no higher than nothing).
  static bool isCollapsed(const QMenuBar& bar);
  // Watched with an opener: hidden for good, whoever shows the windows'
  // bars (the document frame's tabs).
  bool keepsHidden(const QMenuBar* bar) const;
  // The commands with a shortcut in the bar's menus and their submenus
  // added to the bar's window (its parent widget) where it has not got
  // them: their keys work while the bar is hidden. Again after the menus
  // were filled again (the command layout changed).
  static void keepShortcuts(QMenuBar* bar);

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  MenuBarReveal();
  void applySetting();
  void apply(QMenuBar* bar);
  // The application's event filter while a bar is hidden or has an opener.
  void updateFilter();
  // The shown (not hidden) watched bar of the active window, or the one an
  // opener keeps hidden of its shown document.
  QMenuBar* activeBar() const;
  // An opener's bar shown after all: hidden again once the show is done.
  void hideAgain(QMenuBar* bar);
  void reveal(QMenuBar* bar, const QString& how);
  void scheduleConceal();
  void concealIfDone();

  QList<QPointer<QMenuBar>> m_bars;
  QHash<const QMenuBar*, Opener> m_openers; // watch's, by bar
  bool m_filtering = false;                 // updateFilter
  int m_swallowKey = 0;                     // a mnemonic's key given to an opener
  QPointer<QMenuBar> m_revealed;
  QList<QMetaObject::Connection> m_menuConnections;
  QTimer* m_concealTimer = nullptr;
  bool m_hidden = false;   // the setting hides the bars
  bool m_altAlone = false; // Alt is down, and nothing else came since
};

} // namespace mitcad
