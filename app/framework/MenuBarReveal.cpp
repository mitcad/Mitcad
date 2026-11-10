// SPDX-License-Identifier: MIT
#include "MenuBarReveal.hpp"

#include <utility>

#include <QAction>
#include <QApplication>
#include <QEvent>
#include <QKeyEvent>
#include <QKeySequence>
#include <QLayout>
#include <QMenu>
#include <QMenuBar>
#include <QSettings>
#include <QStyle>
#include <QTimer>
#include <QWidget>
#include <QtLogging>

#include "Icons.hpp"
#include "TestSync.hpp"

namespace mitcad {

// ---------------------------------------------------------------------------
// The setting

bool MenuBarSettings::offered() {
#ifdef Q_OS_MACOS
  return false;
#else
  return true;
#endif
}

QString MenuBarSettings::settingsKey() { return QStringLiteral("view/menuBar"); }

bool MenuBarSettings::loadShown() {
  return !offered() || QSettings().value(settingsKey(), true).toBool();
}

bool MenuBarSettings::saveShown(bool shown) {
  if (!offered() || loadShown() == shown) {
    return false;
  }
  {
    QSettings settings;
    settings.setValue(settingsKey(), shown);
  }
  emit MenuBarNotifier::instance().shownChanged(shown);
  return true;
}

MenuBarNotifier& MenuBarNotifier::instance() {
  static MenuBarNotifier notifier;
  return notifier;
}

// ---------------------------------------------------------------------------
// The menus' names

namespace {

QString plainText(const QAction& action) { return action.text().remove(QLatin1Char('&')); }

// The bar's entries with a menu, shown, in its order.
QList<QAction*> menuActions(const QMenuBar& bar) {
  QList<QAction*> actions;
  for (QAction* action : bar.actions()) {
    if (action->menu() != nullptr && action->isVisible()) {
      actions.append(action);
    }
  }
  return actions;
}

} // namespace

QStringList menuBarEntries(const QMenuBar& bar) {
  QStringList entries;
  for (const QAction* action : menuActions(bar)) {
    entries << plainText(*action);
  }
  return entries;
}

QStringList menuEntries(const QMenu& menu) {
  QStringList entries;
  for (const QAction* action : menu.actions()) {
    if (!action->isSeparator() && action->isVisible()) {
      entries << plainText(*action);
    }
  }
  return entries;
}

// ---------------------------------------------------------------------------
// The ☰ button

MenuBarButton::MenuBarButton(QMenuBar* bar, int iconSize, QWidget* parent) : QToolButton(parent), m_bar(bar) {
  setObjectName(QStringLiteral("menuBarButton"));
  setIcon(themeIcon(QStringLiteral("menu")));
  setIconSize(QSize(iconSize, iconSize));
  setAutoRaise(true);
  setFocusPolicy(Qt::NoFocus);
  setToolTip(tr("The menus: File, Edit, View, Tools, Window and Help"));
  setAccessibleName(tr("Menus"));
  m_menu = new QMenu(this);
  m_menu->setObjectName(QStringLiteral("menuBarButtonMenu"));
  // Opened under the button by a click (not QToolButton's own menu, whose
  // style draws an arrow beside the three lines).
  connect(this, &QToolButton::clicked, this, [this] { m_menu->popup(mapToGlobal(QPoint(0, height()))); });
  connect(m_menu, &QMenu::aboutToShow, this, &MenuBarButton::fillMenu);
  connect(m_menu, &QMenu::aboutToHide, this, [this] {
    for (const QMetaObject::Connection& connection : std::as_const(m_submenuLogs)) {
      disconnect(connection);
    }
    m_submenuLogs.clear();
  });
  m_placeTimer = new QTimer(this);
  m_placeTimer->setSingleShot(true);
  m_placeTimer->setInterval(0);
  connect(m_placeTimer, &QTimer::timeout, this, &MenuBarButton::logPlace);
  TestSync::watch(m_placeTimer);
}

bool MenuBarButton::wanted() { return MenuBarSettings::offered() && !MenuBarSettings::loadShown(); }

void MenuBarButton::fillMenu() {
  // The menu bar's own menus, as they are now (the Window menu the window
  // manager adds, the Help menu's entries): their actions, not copies.
  m_menu->clear();
  for (const QMetaObject::Connection& connection : std::as_const(m_submenuLogs)) {
    disconnect(connection);
  }
  m_submenuLogs.clear();
  if (m_bar == nullptr) {
    return;
  }
  for (QAction* action : menuActions(*m_bar)) {
    m_menu->addAction(action);
    QMenu* submenu = action->menu();
    const QString name = plainText(*action);
    m_submenuLogs.append(connect(submenu, &QMenu::aboutToShow, this, [submenu, name] {
      qDebug().noquote() << QStringLiteral("Context menu %1: %2")
                                .arg(name, menuEntries(*submenu).join(QStringLiteral(" | ")));
    }));
  }
  qDebug().noquote() << QStringLiteral("Context menu: %1").arg(menuBarEntries(*m_bar).join(QStringLiteral(" | ")));
}

void MenuBarButton::showEvent(QShowEvent* event) {
  QToolButton::showEvent(event);
  m_lastPlace.clear(); // logged again as it shows
  // Its place in the window changes as its parents move too (the bar hidden
  // moves the title row or the toolbar up): logged again then.
  for (QWidget* parent = parentWidget(); parent != nullptr && !parent->isWindow(); parent = parent->parentWidget()) {
    parent->installEventFilter(this);
  }
  schedulePlace();
}

bool MenuBarButton::eventFilter(QObject* watched, QEvent* event) {
  if (event->type() == QEvent::Move || event->type() == QEvent::Resize) {
    schedulePlace();
  }
  return QToolButton::eventFilter(watched, event);
}

void MenuBarButton::moveEvent(QMoveEvent* event) {
  QToolButton::moveEvent(event);
  schedulePlace();
}

void MenuBarButton::resizeEvent(QResizeEvent* event) {
  QToolButton::resizeEvent(event);
  schedulePlace();
}

void MenuBarButton::schedulePlace() { m_placeTimer->start(); }

void MenuBarButton::logPlace() {
  if (!isVisible()) {
    return;
  }
  // In the window, as the ribbon and the quick-access group log theirs
  // (ui_click_logged); a move inside a moving parent shows here too.
  const QPoint at = mapTo(window(), rect().center());
  const QString line = QStringLiteral("Menu bar button at %1,%2").arg(at.x()).arg(at.y());
  if (line != m_lastPlace) {
    m_lastPlace = line;
    qDebug().noquote() << line;
  }
}

// ---------------------------------------------------------------------------
// Hiding the bars, and showing one for its menus

MenuBarReveal& MenuBarReveal::instance() {
  static MenuBarReveal* reveal = new MenuBarReveal; // lives as long as the application
  return *reveal;
}

MenuBarReveal::MenuBarReveal() : QObject(qApp) {
  m_concealTimer = new QTimer(this);
  m_concealTimer->setSingleShot(true);
  m_concealTimer->setInterval(0);
  connect(m_concealTimer, &QTimer::timeout, this, &MenuBarReveal::concealIfDone);
  TestSync::watch(m_concealTimer);
  connect(&MenuBarNotifier::instance(), &MenuBarNotifier::shownChanged, this, [this](bool) { applySetting(); });
  applySetting();
}

bool MenuBarReveal::isCollapsed(const QMenuBar& bar) { return bar.maximumHeight() == 0; }

void MenuBarReveal::watch(QMenuBar* bar) {
  if (bar == nullptr || !MenuBarSettings::offered()) {
    return;
  }
  m_bars.removeAll(QPointer<QMenuBar>());
  for (const QPointer<QMenuBar>& known : std::as_const(m_bars)) {
    if (known == bar) {
      return;
    }
  }
  m_bars.append(bar);
  apply(bar);
}

void MenuBarReveal::applySetting() {
  const bool hidden = !MenuBarSettings::loadShown();
  if (hidden != m_hidden) {
    m_hidden = hidden;
    if (hidden) {
      qApp->installEventFilter(this);
    } else {
      qApp->removeEventFilter(this);
    }
  }
  m_altAlone = false;
  for (const QMetaObject::Connection& connection : std::as_const(m_menuConnections)) {
    disconnect(connection);
  }
  m_menuConnections.clear();
  m_revealed.clear();
  m_bars.removeAll(QPointer<QMenuBar>());
  for (const QPointer<QMenuBar>& bar : std::as_const(m_bars)) {
    apply(bar);
  }
}

void MenuBarReveal::apply(QMenuBar* bar) {
  if (bar == nullptr || bar == m_revealed.data()) {
    return;
  }
  const int height = m_hidden ? 0 : QWIDGETSIZE_MAX;
  if (bar->maximumHeight() != height) {
    bar->setMaximumHeight(height);
  }
}

QMenuBar* MenuBarReveal::activeBar() const {
  const QWidget* active = QApplication::activeWindow();
  if (active == nullptr) {
    return nullptr;
  }
  for (const QPointer<QMenuBar>& bar : m_bars) {
    // In a document frame the pages' bars are hidden but the shown one's.
    if (bar != nullptr && bar->window() == active && bar->isVisible() && !bar->isNativeMenuBar()) {
      return bar;
    }
  }
  return nullptr;
}

void MenuBarReveal::reveal(QMenuBar* bar, const QString& how) {
  if (bar == nullptr || bar == m_revealed.data()) {
    return;
  }
  if (m_revealed != nullptr) {
    QMenuBar* before = m_revealed;
    m_revealed.clear();
    apply(before);
  }
  m_revealed = bar;
  bar->setMaximumHeight(QWIDGETSIZE_MAX);
  // Laid out at once, so that the menu the keys open next drops from the
  // shown bar's entry.
  if (QWidget* parent = bar->parentWidget(); parent != nullptr && parent->layout() != nullptr) {
    parent->layout()->activate();
  }
  for (QAction* action : menuActions(*bar)) {
    QMenu* menu = action->menu();
    m_menuConnections.append(connect(menu, &QMenu::aboutToHide, this, &MenuBarReveal::scheduleConceal));
    const QString name = plainText(*action);
    m_menuConnections.append(connect(menu, &QMenu::aboutToShow, this, [name] {
      qDebug().noquote() << QStringLiteral("Menu bar %1 opened").arg(name);
    }));
  }
  qDebug().noquote() << QStringLiteral("Menu bar shown (%1)").arg(how);
}

void MenuBarReveal::scheduleConceal() {
  if (m_revealed != nullptr) {
    m_concealTimer->start();
  }
}

void MenuBarReveal::concealIfDone() {
  QMenuBar* bar = m_revealed;
  if (bar == nullptr) {
    return;
  }
  bool menuOpen = false;
  for (const QAction* action : menuActions(*bar)) {
    menuOpen = menuOpen || action->menu()->isVisible();
  }
  // Still in use: a menu open, the keys on the bar (Esc in a menu goes back
  // to the bar, the style's Alt), the pointer over it. An entry highlighted
  // alone is none of these.
  if (menuOpen || bar->hasFocus() || bar->underMouse()) {
    return;
  }
  for (const QMetaObject::Connection& connection : std::as_const(m_menuConnections)) {
    disconnect(connection);
  }
  m_menuConnections.clear();
  m_revealed.clear();
  apply(bar);
  qDebug().noquote() << "Menu bar hidden again";
}

bool MenuBarReveal::eventFilter(QObject* watched, QEvent* event) {
  if (!m_hidden) {
    return false;
  }
  switch (event->type()) {
  case QEvent::KeyPress: {
    const auto* key = static_cast<const QKeyEvent*>(event);
    if (key->isAutoRepeat()) {
      break;
    }
    if (key->key() == Qt::Key_Alt) {
      // A key event goes to the focus widget and then its parents: each
      // delivery comes here, the same press.
      m_altAlone = (key->modifiers() & ~Qt::KeypadModifier) == Qt::AltModifier;
    } else if (key->key() != Qt::Key_Shift && key->key() != Qt::Key_Control && key->key() != Qt::Key_Meta) {
      m_altAlone = false;
      // A key elsewhere (Esc in the view) may end the bar's use.
      scheduleConceal();
    }
    break;
  }
  case QEvent::ShortcutOverride: {
    // Before the shortcuts: Alt with a menu's mnemonic shows the bar, so
    // that the mnemonic opens its menu under the entry.
    const auto* key = static_cast<const QKeyEvent*>(event);
    if (key->key() == Qt::Key_Alt || key->key() == Qt::Key_Shift || key->key() == Qt::Key_Control ||
        key->key() == Qt::Key_Meta) {
      break;
    }
    m_altAlone = false;
    if (m_revealed != nullptr) {
      // While the shown bar has the keyboard (the style's Alt, Esc from its
      // menu), Esc is the bar's: it leaves the bar, and the window's Esc
      // (Cancel, which clears the selection) does not take it.
      if (key->key() == Qt::Key_Escape && m_revealed->hasFocus()) {
        event->accept();
        return true;
      }
      scheduleConceal();
    }
    if ((key->modifiers() & ~Qt::KeypadModifier) != Qt::AltModifier) {
      break;
    }
    QMenuBar* bar = activeBar();
    if (bar == nullptr || bar == m_revealed.data()) {
      break;
    }
    const QKeySequence pressed(QKeyCombination(Qt::AltModifier, Qt::Key(key->key())));
    for (const QAction* action : menuActions(*bar)) {
      if (QKeySequence::mnemonic(action->text()) == pressed) {
        reveal(bar, pressed.toString(QKeySequence::PortableText));
        scheduleConceal();
        break;
      }
    }
    break;
  }
  case QEvent::KeyRelease: {
    const auto* key = static_cast<const QKeyEvent*>(event);
    if (key->isAutoRepeat() || key->key() != Qt::Key_Alt) {
      break;
    }
    if (m_altAlone) {
      m_altAlone = false;
      if (QMenuBar* bar = activeBar(); bar != nullptr && bar != m_revealed.data()) {
        reveal(bar, QStringLiteral("Alt"));
        // Where the style goes to the menu bar on Alt itself (it has its own
        // filter for that, called before this one), the bar has the
        // keyboard now; elsewhere its first menu opens.
        if (!bar->style()->styleHint(QStyle::SH_MenuBar_AltKeyNavigation, nullptr, bar)) {
          const QList<QAction*> actions = menuActions(*bar);
          if (!actions.isEmpty()) {
            bar->setActiveAction(actions.first());
          }
        }
      }
    }
    scheduleConceal();
    break;
  }
  case QEvent::MouseButtonPress:
  case QEvent::Wheel:
    // Alt with the mouse (an orbit, a click) is no way to the menus.
    m_altAlone = false;
    scheduleConceal();
    break;
  case QEvent::FocusOut:
  case QEvent::Leave:
    if (watched == m_revealed.data()) {
      scheduleConceal();
    }
    break;
  case QEvent::ApplicationStateChange:
  case QEvent::WindowDeactivate:
    m_altAlone = false;
    scheduleConceal();
    break;
  default:
    break;
  }
  return false;
}

} // namespace mitcad
