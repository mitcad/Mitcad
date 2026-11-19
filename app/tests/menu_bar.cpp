// SPDX-License-Identifier: MIT
// The hidden menu bar and Enter's repeat (mitcad#1620): Show the menu bar
// is on by default and saved at once, MenuBarNotifier tells of a change
// only, a watched menu bar is no higher than nothing while the setting hides
// it (still visible to Qt, so that its menus' keys work) and gets its height
// back when it is shown; the names of a bar's menus and of a menu's entries
// as the UI tests' logs give them (no separators, hidden entries or "&");
// a bar watched with an opener (the Floating top bar, mitcad#1990) hidden
// outright, its menus' commands with a shortcut its window's actions;
// and the widgets that take Enter themselves, so that Enter does not
// repeat the last command there; the command Repeat's own key
// (mitcad#2052): such a widget gets it as a key press, the view keeps it
// for the log, no key bound and a disabled Repeat take nothing.
#include "UnitTests.hpp"
#include "framework/EnterRepeat.hpp"
#include "framework/MenuBarReveal.hpp"

#include <cstdio>

#include <QAction>
#include <QCheckBox>
#include <QCoreApplication>
#include <QKeyEvent>
#include <QKeySequence>
#include <QLabel>
#include <QLineEdit>
#include <QMenu>
#include <QMenuBar>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QSettings>
#include <QSpinBox>
#include <QStringList>
#include <QTemporaryDir>
#include <QTreeWidget>
#include <QWidget>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "menu_bar.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::MenuBarNotifier;
using mitcad::MenuBarReveal;
using mitcad::MenuBarSettings;

void settingTests() {
  QSettings().remove(MenuBarSettings::settingsKey());
  CHECK(MenuBarSettings::loadShown());
  int told = 0;
  bool toldShown = true;
  const QMetaObject::Connection connection =
      QObject::connect(&MenuBarNotifier::instance(), &MenuBarNotifier::shownChanged, [&](bool shown) {
        ++told;
        toldShown = shown;
      });
  // Already shown: nothing saved, nothing told.
  CHECK(!MenuBarSettings::saveShown(true));
  CHECK(told == 0);
  if (MenuBarSettings::offered()) {
    CHECK(MenuBarSettings::saveShown(false));
    CHECK(!MenuBarSettings::loadShown());
    CHECK(told == 1 && !toldShown);
    CHECK(QSettings().value(MenuBarSettings::settingsKey()).toBool() == false);
    CHECK(!MenuBarSettings::saveShown(false));
    CHECK(told == 1);
    CHECK(MenuBarSettings::saveShown(true));
    CHECK(MenuBarSettings::loadShown());
    CHECK(told == 2 && toldShown);
  } else {
    // macOS: the system's menu bar; nothing to hide.
    CHECK(!MenuBarSettings::saveShown(false));
    CHECK(MenuBarSettings::loadShown());
  }
  QObject::disconnect(connection);
}

void collapseTests() {
  QWidget window;
  auto* bar = new QMenuBar(&window);
  bar->addMenu(QStringLiteral("&File"));
  MenuBarReveal::instance().watch(bar);
  CHECK(!MenuBarReveal::isCollapsed(*bar));
  if (!MenuBarSettings::offered()) {
    return;
  }
  MenuBarSettings::saveShown(false);
  CHECK(MenuBarReveal::isCollapsed(*bar));
  // Hidden by its height, not by Qt's hide: its menus' keys stay.
  CHECK(!bar->isHidden());
  // A bar watched while the setting hides the bars is hidden at once.
  auto* second = new QMenuBar(&window);
  MenuBarReveal::instance().watch(second);
  CHECK(MenuBarReveal::isCollapsed(*second));
  MenuBarSettings::saveShown(true);
  CHECK(!MenuBarReveal::isCollapsed(*bar));
  CHECK(!MenuBarReveal::isCollapsed(*second));
  CHECK(bar->maximumHeight() == QWIDGETSIZE_MAX);
}

void openerTests() {
  if (!MenuBarSettings::offered()) {
    return;
  }
  QWidget window;
  auto* bar = new QMenuBar(&window);
  QMenu* file = bar->addMenu(QStringLiteral("&File"));
  QAction* save = file->addAction(QStringLiteral("&Save"));
  save->setShortcut(QKeySequence(Qt::CTRL | Qt::Key_S));
  QAction* plain = file->addAction(QStringLiteral("&About"));
  QMenu* recent = file->addMenu(QStringLiteral("&Recent"));
  QAction* reopen = recent->addAction(QStringLiteral("Reopen"));
  reopen->setShortcut(QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_T));
  // Already the window's (the command registry's actions): not again.
  QAction* undo = file->addAction(QStringLiteral("&Undo"));
  undo->setShortcut(QKeySequence(Qt::CTRL | Qt::Key_Z));
  window.addAction(undo);
  MenuBarReveal& reveal = MenuBarReveal::instance();
  reveal.watch(bar);
  CHECK(!reveal.keepsHidden(bar));
  CHECK(!bar->isHidden());
  reveal.watch(bar, [](QMenu*, const QString&) {});
  CHECK(reveal.keepsHidden(bar));
  // Hidden by Qt's hide, whatever the setting: no row above the top bar.
  CHECK(bar->isHidden());
  CHECK(MenuBarReveal::isCollapsed(*bar));
  CHECK(MenuBarSettings::loadShown());
  // Its menus' keys are the window's: the commands with a shortcut, in the
  // submenus too, once.
  CHECK(window.actions().contains(save));
  CHECK(window.actions().contains(reopen));
  CHECK(!window.actions().contains(plain));
  CHECK(window.actions().count(undo) == 1);
  // A command filled in later (the command layout changed).
  QAction* print = file->addAction(QStringLiteral("&Print…"));
  print->setShortcut(QKeySequence(Qt::CTRL | Qt::Key_P));
  MenuBarReveal::keepShortcuts(bar);
  CHECK(window.actions().contains(print));
  CHECK(window.actions().count(save) == 1);
}

void entryTests() {
  QMenuBar bar;
  QMenu* file = bar.addMenu(QStringLiteral("&File"));
  bar.addMenu(QStringLiteral("&Edit"));
  QMenu* hidden = bar.addMenu(QStringLiteral("&Debug"));
  hidden->menuAction()->setVisible(false);
  bar.addAction(QStringLiteral("Plain")); // no menu: not one of the menus
  bar.addMenu(QStringLiteral("&Help"));
  CHECK(mitcad::menuBarEntries(bar) ==
        (QStringList{QStringLiteral("File"), QStringLiteral("Edit"), QStringLiteral("Help")}));
  file->addAction(QStringLiteral("&New Design…"));
  file->addSeparator();
  file->addAction(QStringLiteral("&Open…"));
  QAction* gone = file->addAction(QStringLiteral("Gone"));
  gone->setVisible(false);
  CHECK(mitcad::menuEntries(*file) == (QStringList{QStringLiteral("New Design…"), QStringLiteral("Open…")}));
}

void focusTests() {
  using mitcad::focusTakesEnter;
  CHECK(!focusTakesEnter(nullptr));
  QWidget plain;
  CHECK(!focusTakesEnter(&plain));
  QLabel label;
  CHECK(!focusTakesEnter(&label));
  QLineEdit field; // a rename's editor too
  CHECK(focusTakesEnter(&field));
  QSpinBox number;
  CHECK(focusTakesEnter(&number));
  QPlainTextEdit text;
  CHECK(focusTakesEnter(&text));
  QTreeWidget tree; // the browser, the Projects panel
  CHECK(focusTakesEnter(&tree));
  QPushButton button;
  CHECK(focusTakesEnter(&button));
}

// A key offered as a shortcut to `to`, which has the focus, as Qt offers
// it before a press: whether something took it as a key press.
bool offered(QWidget* to, Qt::Key key) {
  to->setFocus(Qt::OtherFocusReason);
  const QString text = key == Qt::Key_Space ? QStringLiteral(" ") : QStringLiteral("a");
  QKeyEvent offer(QEvent::ShortcutOverride, key, Qt::NoModifier, text);
  offer.ignore();
  QCoreApplication::sendEvent(to, &offer);
  return offer.isAccepted();
}

void repeatKeyTests() {
  QWidget window;
  auto* view = new QWidget(&window); // the 3D view: takes no Enter
  view->setFocusPolicy(Qt::StrongFocus);
  auto* box = new QCheckBox(QStringLiteral("Box"), &window);
  auto* button = new QPushButton(QStringLiteral("Button"), &window);
  auto* repeat = new QAction(QStringLiteral("Repeat"), &window);
  window.addAction(repeat);
  mitcad::RepeatKeyFilter keys(repeat, &window);
  window.show();
  const QKeySequence space(Qt::Key_Space);

  // No key bound (the default): Space is nobody's here.
  CHECK(!offered(box, Qt::Key_Space));
  CHECK(!offered(view, Qt::Key_Space));
  CHECK(keys.takeKey().isEmpty());

  // Space bound: a check box and a button get it as a key press (toggled,
  // pressed); the view lets it be Repeat's, and the run names it once.
  repeat->setShortcut(space);
  CHECK(offered(box, Qt::Key_Space));
  CHECK(offered(button, Qt::Key_Space));
  CHECK(keys.takeKey().isEmpty());
  CHECK(!offered(view, Qt::Key_Space));
  CHECK(keys.takeKey() == space);
  CHECK(keys.takeKey().isEmpty());
  // Another key is not Repeat's, and forgets an untaken one.
  CHECK(!offered(view, Qt::Key_Space));
  CHECK(!offered(view, Qt::Key_A));
  CHECK(keys.takeKey().isEmpty());
  // Offered on to the view's parent (nobody took it): only the focus
  // decides.
  view->setFocus(Qt::OtherFocusReason);
  QKeyEvent onward(QEvent::ShortcutOverride, Qt::Key_Space, Qt::NoModifier, QStringLiteral(" "));
  onward.ignore();
  QCoreApplication::sendEvent(&window, &onward);
  CHECK(!onward.isAccepted());
  CHECK(keys.takeKey().isEmpty());

  // Disabled (in a sketch, a panel open, nothing to repeat): the key is
  // left alone everywhere.
  repeat->setEnabled(false);
  CHECK(!offered(box, Qt::Key_Space));
  CHECK(!offered(view, Qt::Key_Space));
  CHECK(keys.takeKey().isEmpty());
}

} // namespace

int menuBarTests() {
  // The settings in a folder of the test's own, never the user's.
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QSettings::Format format = QSettings::defaultFormat();
  const QString organization = QCoreApplication::organizationName();
  const QString application = QCoreApplication::applicationName();
  QCoreApplication::setOrganizationName(QStringLiteral("MitcadTest"));
  QCoreApplication::setApplicationName(QStringLiteral("menu_bar"));
  QSettings::setDefaultFormat(QSettings::IniFormat);
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, directory.path());
  CHECK(QSettings().fileName().startsWith(directory.path()));
  settingTests();
  collapseTests();
  openerTests();
  entryTests();
  focusTests();
  repeatKeyTests();
  QSettings().remove(MenuBarSettings::settingsKey());
  QSettings::setDefaultFormat(format);
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  return failures;
}
MITCAD_UNIT_TEST(menuBarTests);
