// SPDX-License-Identifier: MIT
// The command layout (mitcad#1951): the shipped default reads without a
// problem and places each command once per place; the user's layer moves,
// hides, adds, switches buttons and orders, and is kept as a difference; a
// command shows in two groups; an unknown id is dropped with a log line and
// an alias becomes its command; a malformed user layer falls back to the
// default with a notice. The layout is read and validated at the start and
// again only when it changes: asking it (as a menu, a group's menu, a tab or
// the command search does) reads nothing (the load counter). An earlier version's toolbar pins move
// into the user layer once. A repeat (mitcad#1975: a command's second place, marked "repeat") is
// never the command's home while it has another place. The tabs the user hid and
// Reset Toolbar of one tab (mitcad#1975) are kept in the same layer.
#include "UnitTests.hpp"
#include "framework/CommandLayout.hpp"

#include <cstdio>

#include <QCoreApplication>
#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QSettings>
#include <QString>
#include <QStringList>
#include <QTemporaryDir>
#include <QtLogging>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "command_layout.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::CommandLayout;
using mitcad::CommandLayoutStore;
using mitcad::LayoutEntry;
using mitcad::LayoutUserLayer;

// The log lines (the dropped ids).
QStringList logged;
void collect(QtMsgType, const QMessageLogContext&, const QString& message) { logged.append(message); }

// The command ids of a place, in order, submenus' inline.
QStringList idsOf(const std::vector<LayoutEntry>* entries) {
  QStringList ids;
  if (entries == nullptr) {
    return ids;
  }
  for (const LayoutEntry& entry : *entries) {
    if (entry.kind == LayoutEntry::Kind::Command) {
      ids.append(entry.id);
    } else if (entry.kind == LayoutEntry::Kind::Submenu) {
      ids += idsOf(&entry.entries);
    }
  }
  return ids;
}

bool hasButton(const std::vector<LayoutEntry>* entries, const QString& id) {
  if (entries == nullptr) {
    return false;
  }
  for (const LayoutEntry& entry : *entries) {
    if (entry.kind == LayoutEntry::Kind::Command && entry.id == id) {
      return entry.button;
    }
  }
  return false;
}

const char* const kSmallLayout = R"({
  "version": 1,
  "tabs": [
    {"tab": "SOLID", "groups": [
      {"group": "CREATE", "entries": [{"command": "solid.extrude", "button": true}, "solid.sweep", "-",
        {"submenu": "Pattern", "entries": ["solid.rectangular_pattern", "solid.circular_pattern"]}, "solid.loft"]},
      {"group": "MODIFY", "entries": [{"command": "solid.fillet", "button": true}, "solid.shell", "solid.gone",
        "solid.render"]},
      {"group": "SELECT", "entries": [{"extra": "solid.selection"}]}]}],
  "menus": [{"menu": "FILE", "entries": ["file.open", "-", "file.old_save"]}],
  "unplaced": ["solid.hidden_tool"],
  "optional": ["solid.render"]
})";

QString writeLayout(const QTemporaryDir& directory) {
  const QString path = directory.filePath(QStringLiteral("default.json"));
  QFile file(path);
  if (file.open(QIODevice::WriteOnly | QIODevice::Truncate)) {
    file.write(kSmallLayout);
  }
  return path;
}

void shippedDefaultTests() {
  QFile file(QStringLiteral(":/layout/default.json"));
  CHECK(file.open(QIODevice::ReadOnly));
  QJsonParseError parse;
  const QJsonDocument document = QJsonDocument::fromJson(file.readAll(), &parse);
  CHECK(parse.error == QJsonParseError::NoError);
  QString error;
  const CommandLayout layout = CommandLayout::fromJson(document.object(), &error);
  CHECK(error.isEmpty());
  // The tabs and their groups as the toolbar had them in code.
  QStringList tabs;
  for (const mitcad::LayoutTab& tab : layout.tabs) {
    tabs.append(tab.key);
  }
  // The tabs of mitcad#1975: Surface and Sheet Metal after Solid, Manage
  // after Make's; the Make tab split into 3D Print, Laser and CNC (the
  // owner, 18.11.2026); the Render workspace's tab last (mitcad#1990).
  CHECK(tabs == (QStringList{QStringLiteral("START"), QStringLiteral("SOLID"), QStringLiteral("SURFACE"),
                             QStringLiteral("SHEET METAL"), QStringLiteral("BUILD"), QStringLiteral("MAKE_PRINT"),
                             QStringLiteral("MAKE_LASER"), QStringLiteral("MAKE_CNC"), QStringLiteral("MANAGE"),
                             QStringLiteral("SKETCH"), QStringLiteral("DRAWING"), QStringLiteral("CUT SHEET"),
                             QStringLiteral("RENDER")}));
  CHECK(layout.group(QStringLiteral("SOLID"), QStringLiteral("CREATE")) != nullptr);
  CHECK(layout.menu(QStringLiteral("FILE")) != nullptr);
  // A command once per place; nothing both placed and unplaced.
  for (const QString& place : layout.places()) {
    const std::vector<LayoutEntry>* entries = layout.entries(place);
    QStringList seen;
    for (const LayoutEntry& entry : *entries) {
      if (entry.kind == LayoutEntry::Kind::Command) {
        CHECK(!seen.contains(entry.id));
        seen.append(entry.id);
      }
    }
  }
  const QStringList placed = layout.placedCommands();
  for (const QString& id : layout.unplaced) {
    CHECK(!placed.contains(id));
  }
  // Homes: the first group a command is in.
  CHECK(layout.home(QStringLiteral("solid.extrude")).group == QStringLiteral("CREATE"));
  CHECK(layout.home(QStringLiteral("solid.new_component")).group == QStringLiteral("ASSEMBLE"));
  CHECK(layout.home(QStringLiteral("solid.gear")).group == QStringLiteral("CREATE")); // in a submenu
  CHECK(layout.home(QStringLiteral("build.profile")).tab == QStringLiteral("BUILD"));
  CHECK(layout.home(QStringLiteral("file.open")).isEmpty()); // the File menu only
  // Second places (mitcad#1975) leave the home where it was: New Component
  // also first in Solid › Create, Thicken also there, Material and
  // Appearance also in Solid › Modify and the Make tabs' Materials, the sheet rules
  // also on the Sheet Metal tab, Export Flat Pattern… also there, Print
  // Analysis also in Solid › Inspect, Create Sketch also on Surface and
  // Sheet Metal, Measure also in the sketch.
  const auto home = [&layout](const char* id) {
    const mitcad::LayoutPlace place = layout.home(QString::fromLatin1(id));
    return place.tab + QLatin1Char('/') + place.group;
  };
  CHECK(home("solid.new_component") == QStringLiteral("SOLID/ASSEMBLE"));
  CHECK(home("surface.thicken") == QStringLiteral("SURFACE/MODIFY"));
  // Boundary Fill (mitcad#2149): at home in Solid › Create, again in
  // Surface › Modify after Thicken.
  CHECK(home("solid.boundary_fill") == QStringLiteral("SOLID/CREATE"));
  CHECK(layout.placesOf(QStringLiteral("solid.boundary_fill")) ==
        (QStringList{QStringLiteral("SOLID/CREATE"), QStringLiteral("SURFACE/MODIFY")}));
  // Material and Appearance are tools (the owner, 17.11.2026): at home in
  // Solid › Modify, Material also in Make; Manage holds the managers.
  CHECK(home("solid.material") == QStringLiteral("SOLID/MODIFY"));
  CHECK(home("solid.appearance") == QStringLiteral("SOLID/MODIFY"));
  CHECK(home("solid.materials") == QStringLiteral("MANAGE/MATERIALS"));
  // Also in the Render workspace's Look (mitcad#1990), a repeat.
  CHECK(layout.placesOf(QStringLiteral("solid.appearance")) ==
        (QStringList{QStringLiteral("SOLID/MODIFY"), QStringLiteral("RENDER/LOOK")}));
  CHECK(home("solid.appearances") == QStringLiteral("MANAGE/MATERIALS"));
  // The Render workspace (mitcad#1990): Render Image at home in its Output,
  // and on no other tab (the owner, 18.11.2026: the Solid tab has no Render
  // group); the rendered view and its environment buttons of their own there.
  CHECK(home("file.render_image") == QStringLiteral("RENDER/OUTPUT"));
  CHECK(!layout.placesOf(QStringLiteral("file.render_image")).contains(QStringLiteral("SOLID/RENDER")));
  CHECK(home("view.rendered") == QStringLiteral("RENDER/VIEW"));
  // Print on the drawing's toolbar (mitcad#1990), beside its exports.
  CHECK(home("file.print") == QStringLiteral("DRAWING/EXPORT"));
  // Every command of the Manage tab is a button of its own but Edit Sheet
  // Rules, the same panel as Sheet Rules.
  for (const mitcad::LayoutGroup& group : layout.tab(QStringLiteral("MANAGE"))->groups) {
    for (const LayoutEntry& entry : group.entries) {
      if (entry.kind == LayoutEntry::Kind::Command) {
        CHECK(entry.button == (entry.id != QStringLiteral("sheet.edit_rules")));
      }
    }
  }
  CHECK(home("sheet.rules") == QStringLiteral("MANAGE/RULES"));
  CHECK(home("sheet.edit_rules") == QStringLiteral("MANAGE/RULES"));
  CHECK(home("sheet.export_dxf") == QStringLiteral("MAKE_LASER/EXPORT"));
  CHECK(home("inspect.print_analysis") == QStringLiteral("MAKE_PRINT/PREPARE"));
  // Draft Analysis in the Inspect group after Print Analysis (mitcad#2166).
  CHECK(home("inspect.draft_analysis") == QStringLiteral("SOLID/INSPECT"));
  // The Make workspace's tabs (the owner, 18.11.2026): each method's
  // commands at home on its own tab, every command of 3D Print's own groups
  // and of Laser's Export a button of its own; Materials, Annotate and
  // Inspect on each, repeats on Laser and CNC.
  CHECK(home("make.printers") == QStringLiteral("MAKE_PRINT/PRINTERS"));
  CHECK(home("make.plates") == QStringLiteral("MAKE_PRINT/PRINTERS"));
  // Send to Slicer first on 3D Print, alone in its Slice group (the owner,
  // 18.11.2026).
  CHECK(home("make.print3d") == QStringLiteral("MAKE_PRINT/SLICE"));
  CHECK(layout.tab(QStringLiteral("MAKE_PRINT")) != nullptr &&
        layout.tab(QStringLiteral("MAKE_PRINT"))->groups.front().key == QStringLiteral("SLICE"));
  CHECK(home("make.export_face_dxf") == QStringLiteral("MAKE_LASER/EXPORT"));
  CHECK(home("make.cam_setup") == QStringLiteral("MAKE_CNC/SETUP"));
  CHECK(home("make.cam_machines") == QStringLiteral("MAKE_CNC/SETUP"));
  CHECK(home("make.cam_contour") == QStringLiteral("MAKE_CNC/2D MILLING"));
  CHECK(home("make.cam_waterline_finish") == QStringLiteral("MAKE_CNC/3D MILLING"));
  CHECK(home("make.cam_turn_thread") == QStringLiteral("MAKE_CNC/TURNING"));
  CHECK(home("make.cam_post") == QStringLiteral("MAKE_CNC/TOOLPATH"));
  CHECK(home("make.dimension") == QStringLiteral("MAKE_PRINT/ANNOTATE"));
  CHECK(layout.placesOf(QStringLiteral("make.note")) ==
        (QStringList{QStringLiteral("MAKE_PRINT/ANNOTATE"), QStringLiteral("MAKE_LASER/ANNOTATE"),
                     QStringLiteral("MAKE_CNC/ANNOTATE")}));
  for (const char* group : {"SLICE", "PRINTERS", "PREPARE"}) {
    for (const LayoutEntry& entry : layout.group(QStringLiteral("MAKE_PRINT"), QString::fromLatin1(group))->entries) {
      CHECK(entry.kind != LayoutEntry::Kind::Command || entry.button);
    }
  }
  for (const LayoutEntry& entry : layout.group(QStringLiteral("MAKE_LASER"), QStringLiteral("EXPORT"))->entries) {
    CHECK(entry.kind != LayoutEntry::Kind::Command || entry.button);
  }
  for (const char* tab : {"MAKE_LASER", "MAKE_CNC"}) {
    for (const char* group : {"MATERIALS", "ANNOTATE", "INSPECT"}) {
      const mitcad::LayoutGroup* shared = layout.group(QString::fromLatin1(tab), QString::fromLatin1(group));
      CHECK(shared != nullptr);
      for (const LayoutEntry& entry : shared->entries) {
        CHECK(entry.kind != LayoutEntry::Kind::Command || entry.repeat);
      }
    }
  }
  CHECK(home("sketch.create") == QStringLiteral("SOLID/CREATE"));
  CHECK(home("inspect.measure") == QStringLiteral("SOLID/INSPECT"));
  CHECK(home("construct.offset_plane") == QStringLiteral("SOLID/CONSTRUCT"));
  CHECK(home("sheet.face") == QStringLiteral("SHEET METAL/CREATE"));
  CHECK(home("solid.parameters") == QStringLiteral("MANAGE/PARAMETERS"));
  CHECK(home("file.design_properties") == QStringLiteral("MANAGE/DESIGN"));
  // Off the toolbar (mitcad#1975): acted on through their objects' menus.
  for (const char* id : {"inspect.remove_section", "inspect.flip_section", "insert.calibrate_image_reference",
                         "sketch.edit_text", "sketch.edit_equal_spacing", "sketch.explode_text"}) {
    CHECK(layout.unplaced.contains(QString::fromLatin1(id)));
  }
  // The menus of mitcad#1975 (P4, P7): the sketch's Cut, Copy, Paste and
  // Delete in the Edit menu; the panels and Comments… in View › Panels;
  // Export Snapshot… in the File menu; Render Environment… once in View.
  const QStringList edit = idsOf(layout.entries(QStringLiteral("menu/EDIT")));
  for (const char* id : {"sketch.cut", "sketch.copy", "sketch.paste", "sketch.delete"}) {
    CHECK(edit.contains(QString::fromLatin1(id)));
  }
  CHECK(idsOf(layout.entries(QStringLiteral("menu/VIEW/PANELS"))) == QStringList{QStringLiteral("view.comments")});
  CHECK(!idsOf(layout.entries(QStringLiteral("menu/VIEW"))).contains(QStringLiteral("view.export_snapshot")));
  CHECK(idsOf(layout.entries(QStringLiteral("menu/FILE"))).contains(QStringLiteral("view.export_snapshot")));
  CHECK(layout.placesOf(QStringLiteral("view.render_environment")) ==
        (QStringList{QStringLiteral("RENDER/ENVIRONMENT"), QStringLiteral("menu/VIEW/VISUAL STYLE")}));
  // The extras the code provides, by key.
  CHECK(layout.extraKeys() ==
        (QStringList{QStringLiteral("solid.selection"), QStringLiteral("sketch.selection"),
                     QStringLiteral("cut_sheet.close"), QStringLiteral("cut_sheet.export"), QStringLiteral("recent"),
                     QStringLiteral("panels"),
                     QStringLiteral("window"), QStringLiteral("about"), QStringLiteral("about_qt")}));
  // Written back as read.
  CHECK(CommandLayout::fromJson(layout.toJson(), &error).toJson() == layout.toJson());
}

void resolveTests() {
  QString error;
  const CommandLayout layout =
      CommandLayout::fromJson(QJsonDocument::fromJson(QByteArray(kSmallLayout)).object(), &error);
  CHECK(error.isEmpty());
  logged.clear();
  const QtMessageHandler previous = qInstallMessageHandler(collect);
  const CommandLayout resolved = layout.resolved([](const QString& id) {
    if (id == QStringLiteral("solid.gone") || id == QStringLiteral("solid.render")) {
      return QString();
    }
    return id == QStringLiteral("file.old_save") ? QStringLiteral("file.save") : id;
  });
  qInstallMessageHandler(previous);
  // An unknown id dropped with a log line; an optional one quietly; an
  // alias its command.
  CHECK(idsOf(resolved.entries(QStringLiteral("SOLID/MODIFY"))) ==
        (QStringList{QStringLiteral("solid.fillet"), QStringLiteral("solid.shell")}));
  CHECK(logged.contains(QStringLiteral("Layout: dropped solid.gone from SOLID › MODIFY: no such command")));
  CHECK(logged.filter(QStringLiteral("solid.render")).isEmpty());
  CHECK(idsOf(resolved.entries(QStringLiteral("menu/FILE"))) ==
        (QStringList{QStringLiteral("file.open"), QStringLiteral("file.save")}));
}

void userLayerTests() {
  QString error;
  const CommandLayout base =
      CommandLayout::fromJson(QJsonDocument::fromJson(QByteArray(kSmallLayout)).object(), &error);
  const QString create = QStringLiteral("SOLID/CREATE");
  const QString modify = QStringLiteral("SOLID/MODIFY");
  const QString sweep = QStringLiteral("solid.sweep");

  // A move: gone from CREATE, in MODIFY after Fillet.
  LayoutUserLayer layer;
  layer.move(create, modify, sweep, QStringLiteral("solid.fillet"));
  CommandLayout moved = layer.applied(base);
  CHECK(!idsOf(moved.entries(create)).contains(sweep));
  CHECK(idsOf(moved.entries(modify)).mid(0, 2) == (QStringList{QStringLiteral("solid.fillet"), sweep}));
  CHECK(moved.home(sweep).group == QStringLiteral("MODIFY"));
  // The default is untouched.
  CHECK(idsOf(base.entries(create)).contains(sweep));

  // One command in two groups: added to MODIFY, kept in CREATE; its home is
  // the first.
  LayoutUserLayer twice;
  twice.add(modify, sweep);
  const CommandLayout both = twice.applied(base);
  CHECK(idsOf(both.entries(create)).contains(sweep));
  CHECK(idsOf(both.entries(modify)).last() == sweep);
  CHECK(both.home(sweep).group == QStringLiteral("CREATE"));

  // Buttons on and off, an order, a hide in a submenu.
  LayoutUserLayer other;
  other.setButton(create, sweep, true);
  other.setButton(create, QStringLiteral("solid.extrude"), false);
  other.setOrder(create, {QStringLiteral("solid.loft"), QStringLiteral("solid.extrude")});
  other.hide(QStringLiteral("SOLID/CREATE/Pattern"), QStringLiteral("solid.circular_pattern"));
  const CommandLayout changed = other.applied(base);
  CHECK(hasButton(changed.entries(create), sweep));
  CHECK(!hasButton(changed.entries(create), QStringLiteral("solid.extrude")));
  CHECK(idsOf(changed.entries(create)) ==
        (QStringList{QStringLiteral("solid.loft"), sweep, QStringLiteral("solid.rectangular_pattern"),
                     QStringLiteral("solid.extrude")}));

  // Kept as a difference: read back as written; an add undoes a hide.
  CHECK(LayoutUserLayer::fromJson(other.toJson(), &error).toJson() == other.toJson());
  CHECK(error.isEmpty());
  LayoutUserLayer undone;
  undone.hide(create, sweep);
  undone.add(create, sweep);
  CHECK(undone.isEmpty());
  // A place the layout does not have is skipped.
  LayoutUserLayer nowhere;
  nowhere.add(QStringLiteral("SOLID/NOWHERE"), sweep);
  CHECK(nowhere.applied(base).toJson() == base.toJson());
  // Malformed.
  LayoutUserLayer::fromJson(QJsonObject{{QStringLiteral("changes"), QStringLiteral("no")}}, &error);
  CHECK(!error.isEmpty());
}

void repeatTests() {
  // A repeat (mitcad#1975) before the command's own place: the home is the
  // place without the mark; a command with repeats only is at home in the
  // first; the mark is written back.
  const char* const json = R"({
    "tabs": [
      {"tab": "SOLID", "groups": [
        {"group": "CREATE", "entries": [{"command": "surface.thicken", "repeat": true},
          {"command": "sketch.create", "button": true, "repeat": true}]}]},
      {"tab": "SURFACE", "groups": [
        {"group": "MODIFY", "entries": [{"command": "surface.thicken", "button": true}]}]}],
    "menus": []
  })";
  QString error;
  const CommandLayout layout = CommandLayout::fromJson(QJsonDocument::fromJson(QByteArray(json)).object(), &error);
  CHECK(error.isEmpty());
  CHECK(layout.home(QStringLiteral("surface.thicken")).tab == QStringLiteral("SURFACE"));
  CHECK(layout.home(QStringLiteral("sketch.create")).group == QStringLiteral("CREATE"));
  const std::vector<LayoutEntry>* create = layout.entries(QStringLiteral("SOLID/CREATE"));
  CHECK(create != nullptr && create->size() == 2 && create->at(0).repeat && !create->at(0).button &&
        create->at(1).repeat && create->at(1).button);
  const CommandLayout again = CommandLayout::fromJson(layout.toJson(), &error);
  CHECK(again.toJson() == layout.toJson());
  CHECK(again.home(QStringLiteral("surface.thicken")).tab == QStringLiteral("SURFACE"));
}

void customizeTests() {
  // The tabs' menu and Customize Toolbar… (mitcad#1975): a hidden tab is
  // kept in the layer and read back; Reset Toolbar drops one tab's changes
  // and pins only; a button set back as the default leaves no change.
  QString error;
  LayoutUserLayer layer;
  layer.setTabHidden(QStringLiteral("SURFACE"), true);
  layer.setTabHidden(QStringLiteral("SURFACE"), true);
  CHECK(layer.hiddenTabs == QStringList{QStringLiteral("SURFACE")});
  CHECK(!layer.isEmpty());
  const LayoutUserLayer read = LayoutUserLayer::fromJson(layer.toJson(), &error);
  CHECK(error.isEmpty() && read.hiddenTabs == layer.hiddenTabs);
  layer.setTabHidden(QStringLiteral("SURFACE"), false);
  CHECK(layer.isEmpty());

  LayoutUserLayer tabs;
  tabs.setButton(QStringLiteral("SOLID/CREATE"), QStringLiteral("solid.sweep"), true);
  tabs.hide(QStringLiteral("SOLID/CREATE/Pattern"), QStringLiteral("solid.circular_pattern"));
  tabs.setButton(QStringLiteral("SURFACE/MODIFY"), QStringLiteral("surface.stitch"), true);
  tabs.add(QStringLiteral("menu/TOOLS"), QStringLiteral("solid.sweep"));
  tabs.pins.insert(QStringLiteral("SOLID"), {QStringLiteral("solid.loft")});
  tabs.pins.insert(QStringLiteral("MAKE_PRINT"), {QStringLiteral("make.plates")});
  tabs.setTabHidden(QStringLiteral("MANAGE"), true);
  tabs.resetTab(QStringLiteral("SOLID"));
  CHECK(tabs.changes.size() == 2);
  CHECK(tabs.changes.front().place == QStringLiteral("SURFACE/MODIFY"));
  CHECK(tabs.changes.back().place == QStringLiteral("menu/TOOLS"));
  CHECK(!tabs.pins.contains(QStringLiteral("SOLID")) && tabs.pins.contains(QStringLiteral("MAKE_PRINT")));
  CHECK(tabs.hiddenTabs == QStringList{QStringLiteral("MANAGE")});
  // "SOLID" is no prefix of another tab's places.
  tabs.resetTab(QStringLiteral("SURFACE"));
  CHECK(tabs.changes.size() == 1);

  LayoutUserLayer button;
  button.setButton(QStringLiteral("SOLID/CREATE"), QStringLiteral("solid.sweep"), true);
  button.resetButton(QStringLiteral("SOLID/CREATE"), QStringLiteral("solid.sweep"));
  CHECK(button.isEmpty());
}

void storeTests(const QTemporaryDir& directory) {
  const QString path = writeLayout(directory);
  QSettings().remove(CommandLayoutStore::settingsKey());
  const int before = CommandLayoutStore::loadCount();
  CommandLayoutStore store(path);
  // Read once at the start.
  CHECK(CommandLayoutStore::loadCount() == before + 1);
  CHECK(store.notice().isEmpty());
  CHECK(store.layout().toJson() == store.defaults().toJson());

  // Menus opened, groups' menus, tabs switched, the command search: the
  // layout held is asked, nothing is read.
  for (int i = 0; i < 100; ++i) {
    CHECK(store.layout().group(QStringLiteral("SOLID"), QStringLiteral("CREATE")) != nullptr);
    CHECK(store.layout().menu(QStringLiteral("FILE")) != nullptr);
    CHECK(store.layout().home(QStringLiteral("solid.sweep")).group == QStringLiteral("CREATE"));
    CHECK(!store.layout().placedCommands().isEmpty());
  }
  CHECK(CommandLayoutStore::loadCount() == before + 1);

  // A change: once more, saved, and the layout follows.
  int signals_ = 0;
  QObject::connect(&store, &CommandLayoutStore::changed, [&signals_] { ++signals_; });
  LayoutUserLayer layer;
  layer.move(QStringLiteral("SOLID/CREATE"), QStringLiteral("SOLID/MODIFY"), QStringLiteral("solid.sweep"));
  store.setUserLayer(layer);
  CHECK(CommandLayoutStore::loadCount() == before + 2);
  CHECK(signals_ == 1);
  CHECK(store.layout().home(QStringLiteral("solid.sweep")).group == QStringLiteral("MODIFY"));
  CHECK(QSettings().contains(CommandLayoutStore::settingsKey()));
  for (int i = 0; i < 100; ++i) {
    CHECK(store.layout().home(QStringLiteral("solid.sweep")).group == QStringLiteral("MODIFY"));
  }
  CHECK(CommandLayoutStore::loadCount() == before + 2);

  // Kept across sessions.
  CommandLayoutStore again(path);
  CHECK(again.layout().home(QStringLiteral("solid.sweep")).group == QStringLiteral("MODIFY"));
  CHECK(CommandLayoutStore::loadCount() == before + 3);

  // Restored: the default again, the setting gone.
  store.restoreDefault();
  CHECK(!QSettings().contains(CommandLayoutStore::settingsKey()));
  CHECK(store.layout().toJson() == store.defaults().toJson());
  CHECK(signals_ == 2);

  // Import Settings: read again only when the settings hold another layer.
  const int beforeImport = CommandLayoutStore::loadCount();
  CHECK(!store.reloadIfChanged());
  CHECK(CommandLayoutStore::loadCount() == beforeImport);
  QSettings().setValue(CommandLayoutStore::settingsKey(),
                       QString::fromUtf8(QJsonDocument(layer.toJson()).toJson(QJsonDocument::Compact)));
  CHECK(store.reloadIfChanged());
  CHECK(CommandLayoutStore::loadCount() == beforeImport + 1);
  CHECK(store.layout().home(QStringLiteral("solid.sweep")).group == QStringLiteral("MODIFY"));
  CHECK(!store.reloadIfChanged());

  // A malformed layer of the user's: the default with a notice in words.
  QSettings().setValue(CommandLayoutStore::settingsKey(), QStringLiteral("{\"changes\": 3"));
  store.reload();
  CHECK(!store.notice().isEmpty());
  CHECK(store.layout().toJson() == store.defaults().toJson());
  QSettings().remove(CommandLayoutStore::settingsKey());
}

void pinMigrationTests(const QTemporaryDir& directory) {
  const QString path = writeLayout(directory);
  QSettings().remove(CommandLayoutStore::settingsKey());
  // An earlier version's toolbar pins, per tab (a list as an INI file reads
  // an unquoted "a,b").
  QSettings().setValue(QStringLiteral("toolbarPins/SOLID"), QStringLiteral("solid.sweep,solid.loft"));
  QSettings().setValue(QStringLiteral("toolbarPins/BUILD"),
                       QStringList{QStringLiteral("build.profile"), QStringLiteral("build.panel")});
  logged.clear();
  const QtMessageHandler previous = qInstallMessageHandler(collect);
  {
    // The start: moved into the user layer in their order, the old keys
    // gone, logged once.
    CommandLayoutStore store(path);
    CHECK(store.pins(QStringLiteral("SOLID")) == (QStringList{QStringLiteral("solid.sweep"), QStringLiteral("solid.loft")}));
    CHECK(store.pins(QStringLiteral("BUILD")) ==
          (QStringList{QStringLiteral("build.profile"), QStringLiteral("build.panel")}));
    CHECK(store.layout().toJson() == store.defaults().toJson()); // pins place nothing
  }
  CHECK(!QSettings().contains(QStringLiteral("toolbarPins/SOLID")));
  CHECK(!QSettings().contains(QStringLiteral("toolbarPins/BUILD")));
  CHECK(QSettings().contains(CommandLayoutStore::settingsKey()));
  CHECK(logged.filter(QStringLiteral("Layout: moved 4 toolbar pins into the user layer")).size() == 1);
  // A second start: nothing to move, the pins as they were.
  logged.clear();
  {
    CommandLayoutStore again(path);
    CHECK(again.pins(QStringLiteral("SOLID")) == (QStringList{QStringLiteral("solid.sweep"), QStringLiteral("solid.loft")}));
    CHECK(again.pinTabs() == (QStringList{QStringLiteral("BUILD"), QStringLiteral("SOLID")}));
    // A change of the pins is saved in the layer, without a load.
    const int before = CommandLayoutStore::loadCount();
    again.setPins(QStringLiteral("SOLID"), {QStringLiteral("solid.loft")});
    CHECK(CommandLayoutStore::loadCount() == before);
    again.setPins(QStringLiteral("BUILD"), {});
    CHECK(again.pinTabs() == QStringList{QStringLiteral("SOLID")});
  }
  qInstallMessageHandler(previous);
  CHECK(logged.filter(QStringLiteral("toolbar pins into the user layer")).isEmpty());
  CommandLayoutStore third(path);
  CHECK(third.pins(QStringLiteral("SOLID")) == QStringList{QStringLiteral("solid.loft")});
  // Restored: no pins either.
  third.restoreDefault();
  CHECK(third.pinTabs().isEmpty());
  CHECK(!QSettings().contains(CommandLayoutStore::settingsKey()));
}

int commandLayoutTests() {
  // The settings in a folder of the test's own, never the user's.
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QSettings::Format format = QSettings::defaultFormat();
  const QString organization = QCoreApplication::organizationName();
  const QString application = QCoreApplication::applicationName();
  QCoreApplication::setOrganizationName(QStringLiteral("MitcadTest"));
  QCoreApplication::setApplicationName(QStringLiteral("command_layout"));
  QSettings::setDefaultFormat(QSettings::IniFormat);
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, directory.path());
  shippedDefaultTests();
  resolveTests();
  userLayerTests();
  repeatTests();
  customizeTests();
  storeTests(directory);
  pinMigrationTests(directory);
  QSettings::setDefaultFormat(format);
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  return failures;
}

} // namespace

MITCAD_UNIT_TEST(commandLayoutTests);
