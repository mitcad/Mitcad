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
#include <QProxyStyle>
#include <QSet>
#include <QSettings>
#include <QStyle>
#include <QTimer>
#include <QWidget>
#include <QWindow>
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

void activateFirstEntry(QMenu& menu) {
  for (QAction* action : menu.actions()) {
    if (!action->isSeparator() && action->isVisible() && action->isEnabled()) {
      menu.setActiveAction(action);
      return;
    }
  }
}

namespace {

// A bar with an opener (MenuBarReveal::watch): Alt alone does not take it
// to the keyboard, as the style would where it goes to the menu bar on Alt
// (the Windows styles, Qt's portable style); the window's menus open
// instead. The application's style for the rest.
class NoAltNavigationStyle : public QProxyStyle {
public:
  int styleHint(StyleHint hint, const QStyleOption* option, const QWidget* widget,
                QStyleHintReturn* returnData) const override {
    if (hint == QStyle::SH_MenuBar_AltKeyNavigation) {
      return 0;
    }
    return QProxyStyle::styleHint(hint, option, widget, returnData);
  }
};

} // namespace

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
    m_keysSubmenu.clear();
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

void MenuBarButton::openMenu(QMenu* submenu) {
  m_keysSubmenu.clear();
  m_menu->popup(mapToGlobal(QPoint(0, height()))); // filled as it shows (fillMenu)
  QAction* target = submenu != nullptr ? submenu->menuAction() : nullptr;
  if (target == nullptr || !m_menu->actions().contains(target)) {
    activateFirstEntry(*m_menu);
    return;
  }
  m_menu->setActiveAction(target);
  // Opened beside it as the arrow key opens it, its first entry active.
  QKeyEvent open(QEvent::KeyPress, layoutDirection() == Qt::RightToLeft ? Qt::Key_Left : Qt::Key_Right,
                 Qt::NoModifier);
  QCoreApplication::sendEvent(m_menu, &open);
  // Where the window system gives the menu's window the focus only after
  // the submenu opened (X11), that focus makes the menu the popup the keys
  // go to again: the next key (the submenu's mnemonic, an arrow) went to
  // the menu (0.67.0-rc3, ui-browse-components: Alt+T, then L). The
  // submenu takes the focus back then (eventFilter).
  m_keysSubmenu = submenu;
  if (QWindow* window = m_menu->windowHandle()) {
    window->removeEventFilter(this);
    window->installEventFilter(this);
  }
}

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
  if (event->type() == QEvent::FocusIn && m_menu->windowHandle() == watched) {
    QPointer<QMenu> submenu = m_keysSubmenu;
    m_keysSubmenu.clear();
    if (submenu != nullptr && submenu->isVisible() && QApplication::activePopupWidget() == m_menu) {
      submenu->activateWindow();
    }
    return QToolButton::eventFilter(watched, event);
  }
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

void MenuBarReveal::updateFilter() {
  const bool filtering = m_hidden || !m_openers.isEmpty();
  if (filtering == m_filtering) {
    return;
  }
  m_filtering = filtering;
  if (filtering) {
    qApp->installEventFilter(this);
  } else {
    qApp->removeEventFilter(this);
  }
}

bool MenuBarReveal::isCollapsed(const QMenuBar& bar) { return bar.maximumHeight() == 0; }

void MenuBarReveal::watch(QMenuBar* bar, Opener opener) {
  if (bar == nullptr || !MenuBarSettings::offered()) {
    return;
  }
  m_bars.removeAll(QPointer<QMenuBar>());
  if (opener && !m_openers.contains(bar)) {
    // Its keys go to the opener, never to the bar itself.
    auto* style = new NoAltNavigationStyle;
    style->setParent(bar);
    bar->setStyle(style);
    connect(bar, &QObject::destroyed, this, [this, bar] {
      m_openers.remove(bar);
      updateFilter();
    });
    // Hidden from now on (apply): its menus' keys are the window's.
    keepShortcuts(bar);
  }
  if (opener) {
    m_openers.insert(bar, std::move(opener));
    updateFilter();
  }
  bool known = false;
  for (const QPointer<QMenuBar>& watched : std::as_const(m_bars)) {
    known = known || watched == bar;
  }
  if (!known) {
    m_bars.append(bar);
  }
  apply(bar);
}

void MenuBarReveal::applySetting() {
  m_hidden = !MenuBarSettings::loadShown();
  updateFilter();
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
  const bool kept = m_openers.contains(bar);
  const int height = m_hidden || kept ? 0 : QWIDGETSIZE_MAX;
  if (bar->maximumHeight() != height) {
    bar->setMaximumHeight(height);
  }
  // An opener's bar is hidden outright: a visible bar no higher than
  // nothing was still laid out at its own height on Windows, a row of
  // File Edit View … above the top bar (mitcad#1990).
  if (kept && !bar->isHidden()) {
    bar->hide();
  }
}

bool MenuBarReveal::keepsHidden(const QMenuBar* bar) const { return bar != nullptr && m_openers.contains(bar); }

namespace {

void addMenuShortcuts(QWidget& window, const QList<QAction*>& actions, QSet<const QMenu*>& seen) {
  for (QAction* action : actions) {
    if (QMenu* menu = action->menu()) {
      if (!seen.contains(menu)) {
        seen.insert(menu);
        addMenuShortcuts(window, menu->actions(), seen);
      }
    } else if (!action->isSeparator() && !action->shortcuts().isEmpty() && !window.actions().contains(action)) {
      window.addAction(action);
    }
  }
}

} // namespace

void MenuBarReveal::keepShortcuts(QMenuBar* bar) {
  QWidget* window = bar != nullptr ? bar->parentWidget() : nullptr;
  if (window == nullptr) {
    return;
  }
  QSet<const QMenu*> seen;
  addMenuShortcuts(*window, bar->actions(), seen);
}

void MenuBarReveal::hideAgain(QMenuBar* bar) {
  QPointer<QMenuBar> guarded = bar;
  TestSync::singleShot(0, bar, [this, guarded] {
    if (guarded != nullptr && keepsHidden(guarded) && !guarded->isHidden()) {
      guarded->hide();
      qDebug().noquote() << "Menu bar kept hidden: Floating";
    }
  });
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
  // An opener's bar is hidden for good: the one of the shown document (its
  // window shown, the frame's other pages hidden).
  for (const QPointer<QMenuBar>& bar : m_bars) {
    const QWidget* owner = bar != nullptr ? bar->parentWidget() : nullptr;
    if (owner != nullptr && bar->window() == active && keepsHidden(bar) && owner->isVisible() &&
        !bar->isNativeMenuBar()) {
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
  if (!m_filtering) {
    return false;
  }
  switch (event->type()) {
  case QEvent::KeyPress: {
    const auto* key = static_cast<const QKeyEvent*>(event);
    // The mnemonic's key that opened a menu through an opener goes no
    // further (each delivery of it comes here).
    if (m_swallowKey != 0 && key->key() == m_swallowKey) {
      return true;
    }
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
    const Opener opener = m_openers.value(bar);
    if (!opener && !m_hidden) {
      break; // a shown bar's own mnemonics
    }
    const QKeySequence pressed(QKeyCombination(Qt::AltModifier, Qt::Key(key->key())));
    for (const QAction* action : menuActions(*bar)) {
      if (QKeySequence::mnemonic(action->text()) == pressed) {
        const QString how = pressed.toString(QKeySequence::PortableText);
        if (opener) {
          // Not the bar's shortcut: the opener's menu, once the key is done
          // with.
          event->accept();
          m_swallowKey = key->key();
          QPointer<QMenu> menu = action->menu();
          TestSync::singleShot(0, bar, [opener, menu, how] {
            if (menu != nullptr) {
              opener(menu, how);
            }
          });
          return true;
        }
        reveal(bar, how);
        scheduleConceal();
        break;
      }
    }
    break;
  }
  case QEvent::KeyRelease: {
    const auto* key = static_cast<const QKeyEvent*>(event);
    if (m_swallowKey != 0 && key->key() == m_swallowKey) {
      if (!key->isAutoRepeat()) {
        m_swallowKey = 0;
      }
      return true;
    }
    if (key->isAutoRepeat() || key->key() != Qt::Key_Alt) {
      break;
    }
    if (m_altAlone) {
      m_altAlone = false;
      QMenuBar* bar = activeBar();
      const Opener opener = bar != nullptr ? m_openers.value(bar) : Opener();
      if (opener) {
        // The opener's menus (the window's ☰), once the key is done with.
        TestSync::singleShot(0, bar, [opener] { opener(nullptr, QStringLiteral("Alt")); });
      } else if (m_hidden && bar != nullptr && bar != m_revealed.data()) {
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
  case QEvent::Show:
    // Something showed an opener's bar (a window's bars follow the document
    // frame's tabs): hidden again.
    if (auto* bar = qobject_cast<QMenuBar*>(watched); bar != nullptr && keepsHidden(bar)) {
      hideAgain(bar);
    }
    break;
  case QEvent::ActionAdded:
    // A menu added to an opener's bar later (the Window menu): its keys the
    // window's too.
    if (auto* bar = qobject_cast<QMenuBar*>(watched); bar != nullptr && keepsHidden(bar)) {
      QPointer<QMenuBar> guarded = bar;
      TestSync::singleShot(0, bar, [guarded] {
        if (guarded != nullptr) {
          keepShortcuts(guarded);
        }
      });
    }
    break;
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
