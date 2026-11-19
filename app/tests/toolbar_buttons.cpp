// SPDX-License-Identifier: MIT
// A group's default buttons taken off the toolbar and the tab's groups
// reordered (mitcad#1991, GitHub #421), both kept in the command layout's
// user layer: Remove from Toolbar switches the default's button off and
// keeps the command in its group's menu, Show on Toolbar (the row's or the
// group caption's) switches it back and leaves no change, a submenu's split
// button is found through its submenu, an extra that is no command cannot be
// removed; Move Group Left and Right swap a group with its neighbour among
// the shown groups, kept as the fewest group moves (mitcad#1998: the group
// and the group it follows, so that a group a later version adds keeps the
// default's place); the layer is read back as written (a restart, Export
// and Import Settings) and Reset Toolbar or Restore Default Layout drop it.
#include "UnitTests.hpp"
#include "framework/CommandLayout.hpp"

#include <cstdio>
#include <vector>

#include <QByteArray>
#include <QCoreApplication>
#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QList>
#include <QPair>
#include <QSettings>
#include <QString>
#include <QStringList>
#include <QTemporaryDir>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "toolbar_buttons.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::CommandLayout;
using mitcad::CommandLayoutStore;
using mitcad::LayoutButton;
using mitcad::LayoutChange;
using mitcad::LayoutEntry;
using mitcad::LayoutGroup;
using mitcad::LayoutTab;
using mitcad::LayoutUserLayer;

const char* const kLayout = R"({
  "version": 1,
  "tabs": [
    {"tab": "SOLID", "groups": [
      {"group": "CREATE", "entries": [{"command": "solid.extrude", "button": true}, "solid.sweep", "-",
        {"submenu": "Pattern", "entries": [{"command": "solid.rectangular_pattern", "button": true},
          "solid.circular_pattern"]}]},
      {"group": "MODIFY", "entries": [{"command": "solid.fillet", "button": true},
        {"command": "solid.shell", "button": true}, "solid.draft"]},
      {"group": "INSPECT", "entries": [{"command": "inspect.measure", "button": true}]},
      {"group": "SELECT", "entries": [{"extra": "solid.selection"}]}]},
    {"tab": "BUILD", "groups": [
      {"group": "PROFILE", "entries": [{"command": "build.profile", "button": true}]}]}],
  "menus": [{"menu": "FILE", "entries": ["file.open"]}]
})";

const QString kCreate = QStringLiteral("SOLID/CREATE");
const QString kModify = QStringLiteral("SOLID/MODIFY");
const QString kPattern = QStringLiteral("SOLID/CREATE/Pattern");
const QString kShell = QStringLiteral("solid.shell");

CommandLayout defaults() {
  QString error;
  const CommandLayout layout = CommandLayout::fromJson(QJsonDocument::fromJson(QByteArray(kLayout)).object(), &error);
  CHECK(error.isEmpty());
  return layout;
}

// The command ids of a place, in order (no submenus).
QStringList idsOf(const std::vector<LayoutEntry>* entries) {
  QStringList ids;
  if (entries != nullptr) {
    for (const LayoutEntry& entry : *entries) {
      if (entry.kind == LayoutEntry::Kind::Command) {
        ids.append(entry.id);
      }
    }
  }
  return ids;
}

QStringList groupsOf(const CommandLayout& layout, const QString& tab) {
  QStringList keys;
  if (const LayoutTab* found = layout.tab(tab)) {
    for (const auto& group : found->groups) {
      keys.append(group.key);
    }
  }
  return keys;
}

void removeButtonTests() {
  const CommandLayout base = defaults();
  QString error;

  // Remove from Toolbar: the button off, the command still in the group's
  // menu, where it was.
  LayoutUserLayer layer;
  CHECK(base.hasButton(kModify, kShell));
  layer.setButtonShown(base, kModify, kShell, false);
  const CommandLayout removed = layer.applied(base);
  CHECK(!removed.hasButton(kModify, kShell));
  CHECK(removed.hasButton(kModify, QStringLiteral("solid.fillet")));
  CHECK(idsOf(removed.entries(kModify)) == idsOf(base.entries(kModify)));
  CHECK(layer.buttonRemoved(base, kModify, kShell));
  CHECK(!layer.buttonRemoved(base, kModify, QStringLiteral("solid.fillet")));
  // Listed by the group caption's Show on Toolbar; another group's is not.
  const std::vector<LayoutButton> listed = layer.removedButtons(base, kModify);
  CHECK(listed.size() == 1 && listed.front().place == kModify && listed.front().id == kShell);
  CHECK(layer.removedButtons(base, kCreate).empty());

  // Kept (a restart, Export and Import Settings): read back as written.
  const LayoutUserLayer read = LayoutUserLayer::fromJson(layer.toJson(), &error);
  CHECK(error.isEmpty());
  CHECK(read.toJson() == layer.toJson());
  CHECK(read.buttonRemoved(base, kModify, kShell));

  // Show on Toolbar: the default's button again, no change left.
  layer.setButtonShown(base, kModify, kShell, true);
  CHECK(layer.isEmpty());
  CHECK(layer.applied(base).hasButton(kModify, kShell));

  // A button the user gave a command (not the default's) taken away again
  // leaves no change and is no removed button.
  LayoutUserLayer own;
  own.setButtonShown(base, kModify, QStringLiteral("solid.draft"), true);
  CHECK(own.applied(base).hasButton(kModify, QStringLiteral("solid.draft")));
  CHECK(own.removedButtons(base, kModify).empty());
  own.setButtonShown(base, kModify, QStringLiteral("solid.draft"), false);
  CHECK(own.isEmpty());

  // Every button of a group removed: the group keeps its commands (its menu
  // button stays on the toolbar).
  LayoutUserLayer all;
  all.setButtonShown(base, QStringLiteral("SOLID/INSPECT"), QStringLiteral("inspect.measure"), false);
  const CommandLayout none = all.applied(base);
  CHECK(!none.hasButton(QStringLiteral("SOLID/INSPECT"), QStringLiteral("inspect.measure")));
  CHECK(idsOf(none.entries(QStringLiteral("SOLID/INSPECT"))) == QStringList{QStringLiteral("inspect.measure")});

  // An extra that is no command (the Select group's filters) has no button
  // to remove: nothing is kept.
  LayoutUserLayer extra;
  extra.setButtonShown(base, QStringLiteral("SOLID/SELECT"), QStringLiteral("solid.selection"), false);
  CHECK(extra.isEmpty());
}

void submenuTests() {
  const CommandLayout base = defaults();
  // A group menu's row of a submenu's command: its own entry is in the
  // submenu; the group's own commands at the group.
  CHECK(base.commandPlace(kCreate, QStringLiteral("solid.circular_pattern")) == kPattern);
  CHECK(base.commandPlace(kCreate, QStringLiteral("solid.sweep")) == kCreate);
  CHECK(base.commandPlace(kPattern, QStringLiteral("solid.rectangular_pattern")) == kPattern);
  CHECK(base.commandPlace(kCreate, QStringLiteral("solid.gone")).isEmpty());
  CHECK(base.commandPlace(QStringLiteral("SOLID/NOWHERE"), kShell).isEmpty());

  // The split button removed: listed under its group, the submenu's
  // commands kept.
  LayoutUserLayer layer;
  layer.setButtonShown(base, kPattern, QStringLiteral("solid.rectangular_pattern"), false);
  const CommandLayout removed = layer.applied(base);
  CHECK(!removed.hasButton(kPattern, QStringLiteral("solid.rectangular_pattern")));
  CHECK(idsOf(removed.entries(kPattern)) == idsOf(base.entries(kPattern)));
  const std::vector<LayoutButton> listed = layer.removedButtons(base, kCreate);
  CHECK(listed.size() == 1 && listed.front().place == kPattern);
  // "SOLID/CREATE" is no prefix of another group's places.
  CHECK(layer.removedButtons(base, QStringLiteral("SOLID/CREAT")).empty());
}

void groupOrderTests() {
  const CommandLayout base = defaults();
  QString error;
  const QStringList order = groupsOf(base, QStringLiteral("SOLID"));
  CHECK(order == (QStringList{QStringLiteral("CREATE"), QStringLiteral("MODIFY"), QStringLiteral("INSPECT"),
                              QStringLiteral("SELECT")}));

  // Move Group Right and Left: a swap with the neighbour among the shown.
  const QStringList right =
      LayoutUserLayer::movedGroup(order, order, QStringLiteral("CREATE"), 1);
  CHECK(right == (QStringList{QStringLiteral("MODIFY"), QStringLiteral("CREATE"), QStringLiteral("INSPECT"),
                              QStringLiteral("SELECT")}));
  CHECK(LayoutUserLayer::movedGroup(order, order, QStringLiteral("CREATE"), -1).isEmpty());
  CHECK(LayoutUserLayer::movedGroup(order, order, QStringLiteral("SELECT"), 1).isEmpty());
  CHECK(LayoutUserLayer::movedGroup(order, order, QStringLiteral("NOWHERE"), 1).isEmpty());
  // A group the toolbar does not show is passed over.
  const QStringList shown{QStringLiteral("CREATE"), QStringLiteral("INSPECT"), QStringLiteral("SELECT")};
  CHECK(LayoutUserLayer::movedGroup(order, shown, QStringLiteral("CREATE"), 1) ==
        (QStringList{QStringLiteral("INSPECT"), QStringLiteral("MODIFY"), QStringLiteral("CREATE"),
                     QStringLiteral("SELECT")}));

  LayoutUserLayer layer;
  layer.setGroupOrder(QStringLiteral("SOLID"), right, order, QStringLiteral("CREATE"));
  // One move recorded, of the group moved: Create after Modify.
  CHECK(layer.changes.size() == 1);
  CHECK(layer.changes.front().op == LayoutChange::Op::Group);
  CHECK(layer.changes.front().place == QStringLiteral("SOLID"));
  CHECK(layer.changes.front().id == QStringLiteral("CREATE"));
  CHECK(layer.changes.front().after == QStringLiteral("MODIFY"));
  const CommandLayout moved = layer.applied(base);
  CHECK(groupsOf(moved, QStringLiteral("SOLID")) == right);
  CHECK(groupsOf(moved, QStringLiteral("BUILD")) == groupsOf(base, QStringLiteral("BUILD")));
  // The groups keep their commands; the places are the same.
  CHECK(idsOf(moved.entries(kModify)) == idsOf(base.entries(kModify)));
  CHECK(moved.home(kShell).group == QStringLiteral("MODIFY"));
  // Kept and read back.
  const LayoutUserLayer read = LayoutUserLayer::fromJson(layer.toJson(), &error);
  CHECK(error.isEmpty());
  CHECK(read.toJson() == layer.toJson());
  CHECK(groupsOf(read.applied(base), QStringLiteral("SOLID")) == right);
  const QByteArray json = QJsonDocument(layer.toJson()).toJson(QJsonDocument::Compact);
  CHECK(json.contains(R"({"after":"MODIFY","id":"CREATE","op":"group","place":"SOLID"})"));
  // The same order with Modify the group moved: Modify first.
  LayoutUserLayer other;
  other.setGroupOrder(QStringLiteral("SOLID"), right, order, QStringLiteral("MODIFY"));
  CHECK(other.changes.size() == 1 && other.changes.front().id == QStringLiteral("MODIFY") &&
        other.changes.front().after.isEmpty());
  CHECK(groupsOf(other.applied(base), QStringLiteral("SOLID")) == right);
  // Moved back to the default's order: no change left.
  layer.setGroupOrder(QStringLiteral("SOLID"), order, order, QStringLiteral("CREATE"));
  CHECK(layer.isEmpty());

  // A group a later version adds (LATER, after Modify) keeps the default's
  // place; a move after a group the layout lacks, of a group it lacks or on
  // a tab it lacks is skipped.
  LayoutUserLayer later;
  later.setGroupOrder(QStringLiteral("SOLID"),
                      {QStringLiteral("CREATE"), QStringLiteral("INSPECT"), QStringLiteral("MODIFY"),
                       QStringLiteral("SELECT")},
                      order, QStringLiteral("INSPECT"));
  CHECK(later.changes.size() == 1 && later.changes.front().id == QStringLiteral("INSPECT") &&
        later.changes.front().after == QStringLiteral("CREATE"));
  CommandLayout newer = base;
  LayoutGroup added;
  added.key = QStringLiteral("LATER");
  for (LayoutTab& tab : newer.tabs) {
    if (tab.key == QStringLiteral("SOLID")) {
      tab.groups.insert(tab.groups.begin() + 2, added);
    }
  }
  CHECK(groupsOf(later.applied(newer), QStringLiteral("SOLID")) ==
        (QStringList{QStringLiteral("CREATE"), QStringLiteral("INSPECT"), QStringLiteral("MODIFY"),
                     QStringLiteral("LATER"), QStringLiteral("SELECT")}));
  LayoutUserLayer stale;
  LayoutChange gone;
  gone.op = LayoutChange::Op::Group;
  gone.place = QStringLiteral("SOLID");
  gone.id = QStringLiteral("INSPECT");
  gone.after = QStringLiteral("GONE");
  stale.changes.push_back(gone);
  gone.id = QStringLiteral("GONE");
  gone.after.clear();
  stale.changes.push_back(gone);
  gone.place = QStringLiteral("NOWHERE");
  gone.id = QStringLiteral("CREATE");
  stale.changes.push_back(gone);
  CHECK(stale.applied(base).toJson() == base.toJson());

  // Inspect after Modify on the application's Solid tab (the UI test's
  // case): two Move Group Left, one move recorded.
  const QStringList solid{QStringLiteral("CREATE"),  QStringLiteral("MODIFY"), QStringLiteral("ASSEMBLE"),
                          QStringLiteral("CONSTRUCT"), QStringLiteral("INSPECT"), QStringLiteral("INSERT"),
                          QStringLiteral("RENDER"),  QStringLiteral("SELECT")};
  LayoutUserLayer inspect;
  QStringList current = solid;
  for (int i = 0; i < 2; ++i) {
    current = LayoutUserLayer::movedGroup(current, current, QStringLiteral("INSPECT"), -1);
    inspect.setGroupOrder(QStringLiteral("SOLID"), current, solid, QStringLiteral("INSPECT"));
  }
  CHECK(current.indexOf(QStringLiteral("INSPECT")) == 2);
  CHECK(inspect.changes.size() == 1 && inspect.changes.front().id == QStringLiteral("INSPECT") &&
        inspect.changes.front().after == QStringLiteral("MODIFY"));

  // Any order wanted is what the moves give, however the user got there:
  // a group moved first, another moved twice, the first moved back.
  CommandLayout four;
  LayoutTab letters;
  letters.key = QStringLiteral("T");
  const QStringList abcd{QStringLiteral("A"), QStringLiteral("B"), QStringLiteral("C"), QStringLiteral("D")};
  for (const QString& key : abcd) {
    LayoutGroup group;
    group.key = key;
    letters.groups.push_back(group);
  }
  four.tabs.push_back(letters);
  const QList<QPair<QString, int>> steps{{QStringLiteral("D"), -1}, {QStringLiteral("D"), -1},
                                         {QStringLiteral("D"), -1}, {QStringLiteral("C"), -1},
                                         {QStringLiteral("C"), -1}, {QStringLiteral("D"), 1},
                                         {QStringLiteral("A"), 1},  {QStringLiteral("B"), -1}};
  LayoutUserLayer walk;
  QStringList wanted = abcd;
  for (const auto& step : steps) {
    wanted = LayoutUserLayer::movedGroup(wanted, wanted, step.first, step.second);
    walk.setGroupOrder(QStringLiteral("T"), wanted, abcd, step.first);
    CHECK(groupsOf(walk.applied(four), QStringLiteral("T")) == wanted);
    CHECK(walk.changes.size() <= 2);
  }

  // Reset Toolbar of the tab drops its order and its removed buttons, not
  // another tab's.
  LayoutUserLayer tabs;
  tabs.setGroupOrder(QStringLiteral("SOLID"), right, order, QStringLiteral("CREATE"));
  tabs.setButtonShown(base, kModify, kShell, false);
  tabs.setButtonShown(base, QStringLiteral("BUILD/PROFILE"), QStringLiteral("build.profile"), false);
  tabs.resetTab(QStringLiteral("SOLID"));
  CHECK(tabs.changes.size() == 1);
  CHECK(tabs.changes.front().place == QStringLiteral("BUILD/PROFILE"));
}

void storeTests(const QTemporaryDir& directory) {
  const QString path = directory.filePath(QStringLiteral("default.json"));
  QFile file(path);
  CHECK(file.open(QIODevice::WriteOnly | QIODevice::Truncate));
  file.write(kLayout);
  file.close();
  QSettings().remove(CommandLayoutStore::settingsKey());

  {
    CommandLayoutStore store(path);
    LayoutUserLayer layer = store.userLayer();
    layer.setButtonShown(store.defaults(), kModify, kShell, false);
    layer.setGroupOrder(QStringLiteral("SOLID"),
                        LayoutUserLayer::movedGroup(groupsOf(store.defaults(), QStringLiteral("SOLID")),
                                                    groupsOf(store.defaults(), QStringLiteral("SOLID")),
                                                    QStringLiteral("MODIFY"), -1),
                        groupsOf(store.defaults(), QStringLiteral("SOLID")), QStringLiteral("MODIFY"));
    store.setUserLayer(layer);
    CHECK(!store.layout().hasButton(kModify, kShell));
  }
  // A restart: still removed, still in the menu, the groups in the order
  // left.
  CommandLayoutStore again(path);
  CHECK(!again.layout().hasButton(kModify, kShell));
  CHECK(idsOf(again.layout().entries(kModify)).contains(kShell));
  CHECK(groupsOf(again.layout(), QStringLiteral("SOLID")).first() == QStringLiteral("MODIFY"));
  CHECK(again.userLayer().removedButtons(again.defaults(), kModify).size() == 1);
  // Restore Default Layout: the button and the order back.
  again.restoreDefault();
  CHECK(again.layout().hasButton(kModify, kShell));
  CHECK(groupsOf(again.layout(), QStringLiteral("SOLID")).first() == QStringLiteral("CREATE"));
  CHECK(!QSettings().contains(CommandLayoutStore::settingsKey()));
}

int toolbarButtonsTests() {
  // The settings in a folder of the test's own, never the user's.
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QSettings::Format format = QSettings::defaultFormat();
  const QString organization = QCoreApplication::organizationName();
  const QString application = QCoreApplication::applicationName();
  QCoreApplication::setOrganizationName(QStringLiteral("MitcadTest"));
  QCoreApplication::setApplicationName(QStringLiteral("toolbar_buttons"));
  QSettings::setDefaultFormat(QSettings::IniFormat);
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, directory.path());
  removeButtonTests();
  submenuTests();
  groupOrderTests();
  storeTests(directory);
  QSettings::setDefaultFormat(format);
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  return failures;
}

} // namespace

MITCAD_UNIT_TEST(toolbarButtonsTests);
