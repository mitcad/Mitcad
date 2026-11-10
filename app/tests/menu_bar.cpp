// SPDX-License-Identifier: MIT
// The hidden menu bar and Enter's repeat (mitcad#1620): Show the menu bar
// is on by default and saved at once, MenuBarNotifier tells of a change
// only, a watched menu bar is no higher than nothing while the setting hides
// it (still visible to Qt, so that its menus' keys work) and gets its height
// back when it is shown; the names of a bar's menus and of a menu's entries
// as the UI tests' logs give them (no separators, hidden entries or "&");
// and the widgets that take Enter themselves, so that Enter does not
// repeat the last command there.
#include "framework/EnterRepeat.hpp"
#include "framework/MenuBarReveal.hpp"

#include <cstdio>

#include <QAction>
#include <QCoreApplication>
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
  entryTests();
  focusTests();
  QSettings().remove(MenuBarSettings::settingsKey());
  QSettings::setDefaultFormat(format);
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  return failures;
}
