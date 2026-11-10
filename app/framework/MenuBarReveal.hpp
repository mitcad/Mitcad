// SPDX-License-Identifier: MIT
#pragma once

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
class MenuBarReveal : public QObject {
  Q_OBJECT

public:
  static MenuBarReveal& instance();

  // Hidden while the setting says so, from now on.
  void watch(QMenuBar* bar);
  // Hidden now (no higher than nothing).
  static bool isCollapsed(const QMenuBar& bar);

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  MenuBarReveal();
  void applySetting();
  void apply(QMenuBar* bar);
  // The shown (not hidden) watched bar of the active window.
  QMenuBar* activeBar() const;
  void reveal(QMenuBar* bar, const QString& how);
  void scheduleConceal();
  void concealIfDone();

  QList<QPointer<QMenuBar>> m_bars;
  QPointer<QMenuBar> m_revealed;
  QList<QMetaObject::Connection> m_menuConnections;
  QTimer* m_concealTimer = nullptr;
  bool m_hidden = false;   // the setting hides the bars
  bool m_altAlone = false; // Alt is down, and nothing else came since
};

} // namespace mitcad
