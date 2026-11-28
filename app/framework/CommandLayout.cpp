// SPDX-License-Identifier: MIT
#include "CommandLayout.hpp"

#include <algorithm>
#include <initializer_list>
#include <utility>

#include <QFile>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonParseError>
#include <QJsonValue>
#include <QMetaType>
#include <QSettings>
#include <QVariant>
#include <QtLogging>

namespace mitcad {
namespace {

const QString kSeparator = QStringLiteral("-");
const QString kMenuPrefix = QStringLiteral("menu");
const QString kDefaultResource = QStringLiteral(":/layout/default.json");
int s_loadCount = 0;

// The path a log line shows: "SOLID › MODIFY", "menu › FILE › PROJECT".
QString shownPath(const QString& place) { return QStringList(place.split(QLatin1Char('/'))).join(QStringLiteral(" › ")); }

bool parseEntries(const QJsonValue& value, std::vector<LayoutEntry>* entries, const QString& where, QString* error);

bool parseEntry(const QJsonValue& value, LayoutEntry* entry, const QString& where, QString* error) {
  if (value.isString()) {
    const QString text = value.toString();
    if (text.isEmpty()) {
      *error = QStringLiteral("an empty entry in %1").arg(where);
      return false;
    }
    *entry = text == kSeparator ? LayoutEntry::separator() : LayoutEntry::command(text);
    return true;
  }
  if (!value.isObject()) {
    *error = QStringLiteral("an entry of %1 is neither a text nor an object").arg(where);
    return false;
  }
  const QJsonObject object = value.toObject();
  if (object.contains(QStringLiteral("command"))) {
    const QString id = object.value(QStringLiteral("command")).toString();
    if (id.isEmpty()) {
      *error = QStringLiteral("a command without an id in %1").arg(where);
      return false;
    }
    *entry = LayoutEntry::command(id, object.value(QStringLiteral("button")).toBool());
    entry->repeat = object.value(QStringLiteral("repeat")).toBool();
    return true;
  }
  if (object.contains(QStringLiteral("submenu"))) {
    entry->kind = LayoutEntry::Kind::Submenu;
    entry->id = object.value(QStringLiteral("submenu")).toString();
    entry->icon = object.value(QStringLiteral("icon")).toString();
    if (entry->id.isEmpty()) {
      *error = QStringLiteral("a submenu without a key in %1").arg(where);
      return false;
    }
    return parseEntries(object.value(QStringLiteral("entries")), &entry->entries, where + QLatin1Char('/') + entry->id,
                        error);
  }
  if (object.contains(QStringLiteral("extra"))) {
    entry->kind = LayoutEntry::Kind::Extra;
    entry->id = object.value(QStringLiteral("extra")).toString();
    if (entry->id.isEmpty()) {
      *error = QStringLiteral("an extra without a key in %1").arg(where);
      return false;
    }
    return true;
  }
  *error = QStringLiteral("an entry of %1 is no command, submenu or extra").arg(where);
  return false;
}

bool parseEntries(const QJsonValue& value, std::vector<LayoutEntry>* entries, const QString& where, QString* error) {
  if (!value.isArray()) {
    *error = QStringLiteral("%1 has no list of entries").arg(where);
    return false;
  }
  for (const QJsonValue& item : value.toArray()) {
    LayoutEntry entry;
    if (!parseEntry(item, &entry, where, error)) {
      return false;
    }
    entries->push_back(std::move(entry));
  }
  return true;
}

QJsonValue entryJson(const LayoutEntry& entry);

QJsonArray entriesJson(const std::vector<LayoutEntry>& entries) {
  QJsonArray array;
  for (const LayoutEntry& entry : entries) {
    array.append(entryJson(entry));
  }
  return array;
}

QJsonValue entryJson(const LayoutEntry& entry) {
  switch (entry.kind) {
  case LayoutEntry::Kind::Separator:
    return kSeparator;
  case LayoutEntry::Kind::Command: {
    if (!entry.button && !entry.repeat) {
      return entry.id;
    }
    QJsonObject object{{QStringLiteral("command"), entry.id}};
    if (entry.button) {
      object.insert(QStringLiteral("button"), true);
    }
    if (entry.repeat) {
      object.insert(QStringLiteral("repeat"), true);
    }
    return object;
  }
  case LayoutEntry::Kind::Extra:
    return QJsonObject{{QStringLiteral("extra"), entry.id}};
  case LayoutEntry::Kind::Submenu: {
    QJsonObject object{{QStringLiteral("submenu"), entry.id}};
    if (!entry.icon.isEmpty()) {
      object.insert(QStringLiteral("icon"), entry.icon);
    }
    object.insert(QStringLiteral("entries"), entriesJson(entry.entries));
    return object;
  }
  }
  return QJsonValue();
}

QStringList stringList(const QJsonValue& value) {
  QStringList list;
  for (const QJsonValue& item : value.toArray()) {
    if (item.isString() && !item.toString().isEmpty()) {
      list.append(item.toString());
    }
  }
  return list;
}

// A tab's place under its earlier key, as a layer saved before the rename
// names it, under the tab's key now: SHEET METAL is SHEET PARTS
// (mitcad#2602), "SHEET METAL/CREATE" is "SHEET PARTS/CREATE". Any other
// place is returned as it is.
QString renamedTabPlace(const QString& place) {
  static const QString oldKey = QStringLiteral("SHEET METAL");
  static const QString newKey = QStringLiteral("SHEET PARTS");
  if (place == oldKey) {
    return newKey;
  }
  if (place.startsWith(oldKey + QLatin1Char('/'))) {
    return newKey + place.mid(oldKey.size());
  }
  return place;
}

// Descends from a list of entries along submenu keys.
template <typename Entries>
auto descend(Entries* entries, const QStringList& keys, int from) -> Entries* {
  for (int i = from; i < keys.size() && entries != nullptr; ++i) {
    Entries* next = nullptr;
    for (auto& entry : *entries) {
      if (entry.kind == LayoutEntry::Kind::Submenu && entry.id == keys.at(i)) {
        next = &entry.entries;
        break;
      }
    }
    entries = next;
  }
  return entries;
}

void collectPlaces(const std::vector<LayoutEntry>& entries, const QString& place, QStringList* places) {
  for (const LayoutEntry& entry : entries) {
    if (entry.kind == LayoutEntry::Kind::Submenu) {
      const QString path = place + QLatin1Char('/') + entry.id;
      places->append(path);
      collectPlaces(entry.entries, path, places);
    }
  }
}

// The places of `entries` (at `place`, its submenus below it) that hold the
// command itself; `repeats`: the places where it is a repeat (mitcad#1975),
// else the others.
void collectPlacesOf(const std::vector<LayoutEntry>& entries, const QString& place, const QString& id, bool repeats,
                     QStringList* places) {
  bool here = false;
  for (const LayoutEntry& entry : entries) {
    if (entry.kind == LayoutEntry::Kind::Command && entry.id == id && entry.repeat == repeats && !here) {
      here = true;
      places->append(place);
    } else if (entry.kind == LayoutEntry::Kind::Submenu) {
      collectPlacesOf(entry.entries, place + QLatin1Char('/') + entry.id, id, repeats, places);
    }
  }
}

// Whether the entries hold the command; `repeats`: also as a repeat.
bool containsCommand(const std::vector<LayoutEntry>& entries, const QString& id, bool repeats) {
  for (const LayoutEntry& entry : entries) {
    if ((entry.kind == LayoutEntry::Kind::Command && entry.id == id && (repeats || !entry.repeat)) ||
        (entry.kind == LayoutEntry::Kind::Submenu && containsCommand(entry.entries, id, repeats))) {
      return true;
    }
  }
  return false;
}

void collectCommands(const std::vector<LayoutEntry>& entries, QStringList* ids) {
  for (const LayoutEntry& entry : entries) {
    if (entry.kind == LayoutEntry::Kind::Command && !ids->contains(entry.id)) {
      ids->append(entry.id);
    } else if (entry.kind == LayoutEntry::Kind::Submenu) {
      collectCommands(entry.entries, ids);
    }
  }
}

void collectKeys(const std::vector<LayoutEntry>& entries, LayoutEntry::Kind kind, QStringList* keys) {
  for (const LayoutEntry& entry : entries) {
    if (entry.kind == kind && !keys->contains(entry.id)) {
      keys->append(entry.id);
    }
    if (entry.kind == LayoutEntry::Kind::Submenu) {
      collectKeys(entry.entries, kind, keys);
    }
  }
}

std::vector<LayoutEntry> resolvedEntries(const std::vector<LayoutEntry>& entries, const QString& place,
                                         const std::function<QString(const QString&)>& canonical,
                                         const QStringList& optional) {
  std::vector<LayoutEntry> kept;
  for (const LayoutEntry& entry : entries) {
    if (entry.kind == LayoutEntry::Kind::Submenu) {
      LayoutEntry submenu = entry;
      submenu.entries = resolvedEntries(entry.entries, place + QLatin1Char('/') + entry.id, canonical, optional);
      kept.push_back(std::move(submenu));
      continue;
    }
    if (entry.kind != LayoutEntry::Kind::Command) {
      kept.push_back(entry);
      continue;
    }
    const QString id = canonical(entry.id);
    if (id.isEmpty()) {
      if (!optional.contains(entry.id)) {
        qInfo().noquote() << QStringLiteral("Layout: dropped %1 from %2: no such command")
                                 .arg(entry.id, shownPath(place));
      }
      continue;
    }
    LayoutEntry command = entry;
    command.id = id;
    kept.push_back(std::move(command));
  }
  return kept;
}

int commandIndex(const std::vector<LayoutEntry>& entries, const QString& id) {
  for (std::size_t i = 0; i < entries.size(); ++i) {
    if (entries[i].kind == LayoutEntry::Kind::Command && entries[i].id == id) {
      return static_cast<int>(i);
    }
  }
  return -1;
}

QString opName(LayoutChange::Op op) {
  switch (op) {
  case LayoutChange::Op::Hide:
    return QStringLiteral("hide");
  case LayoutChange::Op::Add:
    return QStringLiteral("add");
  case LayoutChange::Op::Button:
    return QStringLiteral("button");
  case LayoutChange::Op::Order:
    return QStringLiteral("order");
  case LayoutChange::Op::Group:
    return QStringLiteral("group");
  }
  return QString();
}

} // namespace

LayoutEntry LayoutEntry::command(const QString& id, bool button) {
  LayoutEntry entry;
  entry.kind = Kind::Command;
  entry.id = id;
  entry.button = button;
  return entry;
}

LayoutEntry LayoutEntry::separator() {
  LayoutEntry entry;
  entry.kind = Kind::Separator;
  return entry;
}

CommandLayout CommandLayout::fromJson(const QJsonObject& json, QString* error) {
  CommandLayout layout;
  QString problem;
  const auto fail = [&layout, error, &problem] {
    if (error != nullptr) {
      *error = problem;
    }
    layout = CommandLayout();
    return layout;
  };
  if (!json.value(QStringLiteral("tabs")).isArray() || !json.value(QStringLiteral("menus")).isArray()) {
    problem = QStringLiteral("no tabs and menus");
    return fail();
  }
  for (const QJsonValue& tabValue : json.value(QStringLiteral("tabs")).toArray()) {
    const QJsonObject tabObject = tabValue.toObject();
    LayoutTab tab;
    tab.key = tabObject.value(QStringLiteral("tab")).toString();
    if (tab.key.isEmpty() || !tabObject.value(QStringLiteral("groups")).isArray()) {
      problem = QStringLiteral("a tab without a key or groups");
      return fail();
    }
    for (const QJsonValue& groupValue : tabObject.value(QStringLiteral("groups")).toArray()) {
      const QJsonObject groupObject = groupValue.toObject();
      LayoutGroup group;
      group.key = groupObject.value(QStringLiteral("group")).toString();
      if (group.key.isEmpty()) {
        problem = QStringLiteral("a group of %1 without a key").arg(tab.key);
        return fail();
      }
      if (!parseEntries(groupObject.value(QStringLiteral("entries")), &group.entries,
                        tab.key + QLatin1Char('/') + group.key, &problem)) {
        return fail();
      }
      tab.groups.push_back(std::move(group));
    }
    layout.tabs.push_back(std::move(tab));
  }
  for (const QJsonValue& menuValue : json.value(QStringLiteral("menus")).toArray()) {
    const QJsonObject menuObject = menuValue.toObject();
    LayoutEntry menu;
    menu.kind = LayoutEntry::Kind::Submenu;
    menu.id = menuObject.value(QStringLiteral("menu")).toString();
    if (menu.id.isEmpty()) {
      problem = QStringLiteral("a menu without a key");
      return fail();
    }
    if (!parseEntries(menuObject.value(QStringLiteral("entries")), &menu.entries,
                      kMenuPrefix + QLatin1Char('/') + menu.id, &problem)) {
      return fail();
    }
    layout.menus.push_back(std::move(menu));
  }
  layout.unplaced = stringList(json.value(QStringLiteral("unplaced")));
  layout.optional = stringList(json.value(QStringLiteral("optional")));
  return layout;
}

QJsonObject CommandLayout::toJson() const {
  QJsonArray tabArray;
  for (const LayoutTab& tab : tabs) {
    QJsonArray groupArray;
    for (const LayoutGroup& group : tab.groups) {
      groupArray.append(QJsonObject{{QStringLiteral("group"), group.key},
                                    {QStringLiteral("entries"), entriesJson(group.entries)}});
    }
    tabArray.append(QJsonObject{{QStringLiteral("tab"), tab.key}, {QStringLiteral("groups"), groupArray}});
  }
  QJsonArray menuArray;
  for (const LayoutEntry& menu : menus) {
    menuArray.append(
        QJsonObject{{QStringLiteral("menu"), menu.id}, {QStringLiteral("entries"), entriesJson(menu.entries)}});
  }
  return QJsonObject{{QStringLiteral("version"), 1},
                     {QStringLiteral("tabs"), tabArray},
                     {QStringLiteral("menus"), menuArray},
                     {QStringLiteral("unplaced"), QJsonArray::fromStringList(unplaced)},
                     {QStringLiteral("optional"), QJsonArray::fromStringList(optional)}};
}

const LayoutTab* CommandLayout::tab(const QString& key) const {
  const auto found = std::find_if(tabs.begin(), tabs.end(), [&key](const LayoutTab& tab) { return tab.key == key; });
  return found != tabs.end() ? &*found : nullptr;
}

const LayoutGroup* CommandLayout::group(const QString& tabKey, const QString& groupKey) const {
  const LayoutTab* found = tab(tabKey);
  if (found == nullptr) {
    return nullptr;
  }
  for (const LayoutGroup& group : found->groups) {
    if (group.key == groupKey) {
      return &group;
    }
  }
  return nullptr;
}

const LayoutEntry* CommandLayout::menu(const QString& key) const {
  for (const LayoutEntry& menu : menus) {
    if (menu.id == key) {
      return &menu;
    }
  }
  return nullptr;
}

std::vector<LayoutEntry>* CommandLayout::entries(const QString& place) {
  const QStringList keys = place.split(QLatin1Char('/'));
  if (keys.size() < 2) {
    return nullptr;
  }
  if (keys.first() == kMenuPrefix) {
    for (LayoutEntry& menu : menus) {
      if (menu.id == keys.at(1)) {
        return descend(&menu.entries, keys, 2);
      }
    }
    return nullptr;
  }
  for (LayoutTab& tab : tabs) {
    if (tab.key != keys.first()) {
      continue;
    }
    for (LayoutGroup& group : tab.groups) {
      if (group.key == keys.at(1)) {
        return descend(&group.entries, keys, 2);
      }
    }
  }
  return nullptr;
}

const std::vector<LayoutEntry>* CommandLayout::entries(const QString& place) const {
  return const_cast<CommandLayout*>(this)->entries(place);
}

QStringList CommandLayout::places() const {
  QStringList places;
  for (const LayoutTab& tab : tabs) {
    for (const LayoutGroup& group : tab.groups) {
      const QString place = tab.key + QLatin1Char('/') + group.key;
      places.append(place);
      collectPlaces(group.entries, place, &places);
    }
  }
  for (const LayoutEntry& menu : menus) {
    const QString place = kMenuPrefix + QLatin1Char('/') + menu.id;
    places.append(place);
    collectPlaces(menu.entries, place, &places);
  }
  return places;
}

LayoutPlace CommandLayout::home(const QString& id) const {
  // Its first place that is no repeat (mitcad#1975), else its first.
  for (const bool repeats : {false, true}) {
    for (const LayoutTab& tab : tabs) {
      for (const LayoutGroup& group : tab.groups) {
        if (containsCommand(group.entries, id, repeats)) {
          return LayoutPlace{tab.key, group.key};
        }
      }
    }
  }
  return LayoutPlace();
}

QStringList CommandLayout::placesOf(const QString& id) const {
  // The tabs' places where it is no repeat first, so that the home (as
  // home() finds it) leads, then its repeats, then the menus.
  QStringList places;
  for (const bool repeats : {false, true}) {
    for (const LayoutTab& tab : tabs) {
      for (const LayoutGroup& group : tab.groups) {
        collectPlacesOf(group.entries, tab.key + QLatin1Char('/') + group.key, id, repeats, &places);
      }
    }
  }
  for (const bool repeats : {false, true}) {
    for (const LayoutEntry& menu : menus) {
      collectPlacesOf(menu.entries, kMenuPrefix + QLatin1Char('/') + menu.id, id, repeats, &places);
    }
  }
  return places;
}

QStringList CommandLayout::placedCommands() const {
  QStringList ids;
  for (const LayoutTab& tab : tabs) {
    for (const LayoutGroup& group : tab.groups) {
      collectCommands(group.entries, &ids);
    }
  }
  for (const LayoutEntry& menu : menus) {
    collectCommands(menu.entries, &ids);
  }
  return ids;
}

QStringList CommandLayout::groupKeys() const {
  QStringList keys;
  for (const LayoutTab& tab : tabs) {
    for (const LayoutGroup& group : tab.groups) {
      if (!keys.contains(group.key)) {
        keys.append(group.key);
      }
    }
  }
  return keys;
}

QStringList CommandLayout::submenuKeys() const {
  QStringList keys;
  for (const LayoutTab& tab : tabs) {
    for (const LayoutGroup& group : tab.groups) {
      collectKeys(group.entries, LayoutEntry::Kind::Submenu, &keys);
    }
  }
  for (const LayoutEntry& menu : menus) {
    if (!keys.contains(menu.id)) {
      keys.append(menu.id);
    }
    collectKeys(menu.entries, LayoutEntry::Kind::Submenu, &keys);
  }
  return keys;
}

QStringList CommandLayout::extraKeys() const {
  QStringList keys;
  for (const LayoutTab& tab : tabs) {
    for (const LayoutGroup& group : tab.groups) {
      collectKeys(group.entries, LayoutEntry::Kind::Extra, &keys);
    }
  }
  for (const LayoutEntry& menu : menus) {
    collectKeys(menu.entries, LayoutEntry::Kind::Extra, &keys);
  }
  return keys;
}

CommandLayout CommandLayout::resolved(const std::function<QString(const QString& id)>& canonical) const {
  CommandLayout layout = *this;
  for (LayoutTab& tab : layout.tabs) {
    for (LayoutGroup& group : tab.groups) {
      group.entries = resolvedEntries(group.entries, tab.key + QLatin1Char('/') + group.key, canonical, optional);
    }
  }
  for (LayoutEntry& menu : layout.menus) {
    menu.entries = resolvedEntries(menu.entries, kMenuPrefix + QLatin1Char('/') + menu.id, canonical, optional);
  }
  return layout;
}

QString CommandLayout::commandPlace(const QString& place, const QString& id) const {
  const std::vector<LayoutEntry>* found = entries(place);
  if (found == nullptr) {
    return QString();
  }
  if (commandIndex(*found, id) >= 0) {
    return place;
  }
  for (const LayoutEntry& entry : *found) {
    if (entry.kind == LayoutEntry::Kind::Submenu) {
      const QString inner = commandPlace(place + QLatin1Char('/') + entry.id, id);
      if (!inner.isEmpty()) {
        return inner;
      }
    }
  }
  return QString();
}

bool CommandLayout::hasButton(const QString& place, const QString& id) const {
  const std::vector<LayoutEntry>* found = entries(place);
  const int index = found != nullptr ? commandIndex(*found, id) : -1;
  return index >= 0 && (*found)[static_cast<std::size_t>(index)].button;
}

// ---------------------------------------------------------------------------
// The user's layer

LayoutUserLayer LayoutUserLayer::fromJson(const QJsonObject& json, QString* error) {
  LayoutUserLayer layer;
  const auto fail = [&layer, error](const QString& problem) {
    if (error != nullptr) {
      *error = problem;
    }
    layer = LayoutUserLayer();
    return layer;
  };
  if (!json.value(QStringLiteral("changes")).isArray()) {
    return fail(QStringLiteral("no list of changes"));
  }
  for (const QJsonValue& value : json.value(QStringLiteral("changes")).toArray()) {
    if (!value.isObject()) {
      return fail(QStringLiteral("a change is no object"));
    }
    const QJsonObject object = value.toObject();
    LayoutChange change;
    const QString op = object.value(QStringLiteral("op")).toString();
    if (op == QLatin1String("hide")) {
      change.op = LayoutChange::Op::Hide;
    } else if (op == QLatin1String("add")) {
      change.op = LayoutChange::Op::Add;
    } else if (op == QLatin1String("button")) {
      change.op = LayoutChange::Op::Button;
    } else if (op == QLatin1String("order")) {
      change.op = LayoutChange::Op::Order;
    } else if (op == QLatin1String("group")) {
      change.op = LayoutChange::Op::Group;
    } else {
      return fail(QStringLiteral("an unknown change \"%1\"").arg(op));
    }
    change.place = renamedTabPlace(object.value(QStringLiteral("place")).toString());
    change.id = object.value(QStringLiteral("id")).toString();
    change.after = object.value(QStringLiteral("after")).toString();
    change.on = object.value(QStringLiteral("on")).toBool(true);
    change.ids = stringList(object.value(QStringLiteral("ids")));
    if (change.place.isEmpty() || (change.op != LayoutChange::Op::Order && change.id.isEmpty())) {
      return fail(QStringLiteral("a change without its place or command"));
    }
    layer.changes.push_back(std::move(change));
  }
  layer.hiddenTabs = stringList(json.value(QStringLiteral("hiddenTabs")));
  for (QString& tab : layer.hiddenTabs) {
    tab = renamedTabPlace(tab);
  }
  layer.hiddenTabs.removeDuplicates();
  // The Make tab hidden before it became the Make workspace's three tabs
  // (the owner, 18.11.2026): all three hidden. Its pins go to their
  // commands' tabs (ToolbarPins); its other changes name places that are
  // gone and are left out with the layout's log line.
  if (layer.hiddenTabs.removeAll(QStringLiteral("MAKE")) > 0) {
    for (const QString& tab : {QStringLiteral("MAKE_PRINT"), QStringLiteral("MAKE_LASER"), QStringLiteral("MAKE_CNC")}) {
      if (!layer.hiddenTabs.contains(tab)) {
        layer.hiddenTabs.append(tab);
      }
    }
  }
  const QJsonValue pins = json.value(QStringLiteral("pins"));
  if (!pins.isUndefined() && !pins.isObject()) {
    return fail(QStringLiteral("the pins are no object"));
  }
  const QJsonObject tabs = pins.toObject();
  for (auto it = tabs.begin(); it != tabs.end(); ++it) {
    const QStringList ids = stringList(it.value());
    if (!ids.isEmpty()) {
      QStringList& kept = layer.pins[renamedTabPlace(it.key())];
      for (const QString& id : ids) {
        if (!kept.contains(id)) {
          kept.append(id);
        }
      }
    }
  }
  return layer;
}

namespace {

// The ids of an earlier version's pins value: a string of ids joined by
// commas, or a list (an INI file's unquoted "a,b", a platform store's
// string list).
QStringList pinIds(const QVariant& value) {
  const QStringList parts = value.metaType().id() == QMetaType::QStringList ? value.toStringList()
                                                                            : QStringList{value.toString()};
  QStringList ids;
  for (const QString& part : parts) {
    for (const QString& id : part.split(QLatin1Char(','), Qt::SkipEmptyParts)) {
      const QString trimmed = id.trimmed();
      if (!trimmed.isEmpty() && !ids.contains(trimmed)) {
        ids.append(trimmed);
      }
    }
  }
  return ids;
}

} // namespace

QJsonObject LayoutUserLayer::toJson() const {
  QJsonArray array;
  for (const LayoutChange& change : changes) {
    QJsonObject object{{QStringLiteral("op"), opName(change.op)}, {QStringLiteral("place"), change.place}};
    if (change.op == LayoutChange::Op::Order) {
      object.insert(QStringLiteral("ids"), QJsonArray::fromStringList(change.ids));
    } else {
      object.insert(QStringLiteral("id"), change.id);
    }
    if ((change.op == LayoutChange::Op::Add || change.op == LayoutChange::Op::Group) && !change.after.isEmpty()) {
      object.insert(QStringLiteral("after"), change.after);
    }
    if (change.op == LayoutChange::Op::Button) {
      object.insert(QStringLiteral("on"), change.on);
    }
    array.append(object);
  }
  QJsonObject json{{QStringLiteral("version"), 1}, {QStringLiteral("changes"), array}};
  if (!pins.isEmpty()) {
    QJsonObject tabs;
    for (auto it = pins.cbegin(); it != pins.cend(); ++it) {
      tabs.insert(it.key(), QJsonArray::fromStringList(it.value()));
    }
    json.insert(QStringLiteral("pins"), tabs);
  }
  if (!hiddenTabs.isEmpty()) {
    json.insert(QStringLiteral("hiddenTabs"), QJsonArray::fromStringList(hiddenTabs));
  }
  return json;
}

void LayoutUserLayer::hide(const QString& place, const QString& id) {
  // An add of the command to the same place is undone instead.
  const auto added = std::find_if(changes.begin(), changes.end(), [&](const LayoutChange& change) {
    return change.op == LayoutChange::Op::Add && change.place == place && change.id == id;
  });
  if (added != changes.end()) {
    changes.erase(added);
    return;
  }
  LayoutChange change;
  change.op = LayoutChange::Op::Hide;
  change.place = place;
  change.id = id;
  changes.push_back(change);
}

void LayoutUserLayer::add(const QString& place, const QString& id, const QString& after) {
  // A hide of the command from the same place is undone instead.
  const auto hidden = std::find_if(changes.begin(), changes.end(), [&](const LayoutChange& change) {
    return change.op == LayoutChange::Op::Hide && change.place == place && change.id == id;
  });
  if (hidden != changes.end() && after.isEmpty()) {
    changes.erase(hidden);
    return;
  }
  LayoutChange change;
  change.op = LayoutChange::Op::Add;
  change.place = place;
  change.id = id;
  change.after = after;
  changes.push_back(change);
}

void LayoutUserLayer::move(const QString& from, const QString& to, const QString& id, const QString& after) {
  hide(from, id);
  add(to, id, after);
}

void LayoutUserLayer::setButton(const QString& place, const QString& id, bool on) {
  changes.erase(std::remove_if(changes.begin(), changes.end(),
                               [&](const LayoutChange& change) {
                                 return change.op == LayoutChange::Op::Button && change.place == place &&
                                        change.id == id;
                               }),
                changes.end());
  LayoutChange change;
  change.op = LayoutChange::Op::Button;
  change.place = place;
  change.id = id;
  change.on = on;
  changes.push_back(change);
}

void LayoutUserLayer::setOrder(const QString& place, const QStringList& ids) {
  changes.erase(std::remove_if(changes.begin(), changes.end(),
                               [&](const LayoutChange& change) {
                                 return change.op == LayoutChange::Op::Order && change.place == place;
                               }),
                changes.end());
  LayoutChange change;
  change.op = LayoutChange::Op::Order;
  change.place = place;
  change.ids = ids;
  changes.push_back(change);
}

void LayoutUserLayer::resetButton(const QString& place, const QString& id) {
  changes.erase(std::remove_if(changes.begin(), changes.end(),
                               [&](const LayoutChange& change) {
                                 return change.op == LayoutChange::Op::Button && change.place == place &&
                                        change.id == id;
                               }),
                changes.end());
}

void LayoutUserLayer::setTabHidden(const QString& tab, bool hidden) {
  hiddenTabs.removeAll(tab);
  if (hidden) {
    hiddenTabs.append(tab);
  }
}

void LayoutUserLayer::resetTab(const QString& tab) {
  const QString prefix = tab + QLatin1Char('/');
  changes.erase(std::remove_if(changes.begin(), changes.end(),
                               [&prefix, &tab](const LayoutChange& change) {
                                 return change.place.startsWith(prefix) ||
                                        (change.op == LayoutChange::Op::Group && change.place == tab);
                               }),
                changes.end());
  pins.remove(tab);
}

void LayoutUserLayer::setButtonShown(const CommandLayout& defaults, const QString& place, const QString& id,
                                     bool on) {
  if (defaults.hasButton(place, id) == on) {
    resetButton(place, id);
  } else {
    setButton(place, id, on);
  }
}

bool LayoutUserLayer::buttonRemoved(const CommandLayout& defaults, const QString& place, const QString& id) const {
  if (!defaults.hasButton(place, id)) {
    return false;
  }
  return std::any_of(changes.begin(), changes.end(), [&place, &id](const LayoutChange& change) {
    return change.op == LayoutChange::Op::Button && !change.on && change.place == place && change.id == id;
  });
}

std::vector<LayoutButton> LayoutUserLayer::removedButtons(const CommandLayout& defaults, const QString& group) const {
  std::vector<LayoutButton> removed;
  const QString prefix = group + QLatin1Char('/');
  for (const LayoutChange& change : changes) {
    if (change.op != LayoutChange::Op::Button || change.on ||
        (change.place != group && !change.place.startsWith(prefix)) ||
        !defaults.hasButton(change.place, change.id)) {
      continue;
    }
    removed.push_back(LayoutButton{change.place, change.id});
  }
  return removed;
}

void LayoutUserLayer::setGroupOrder(const QString& tab, const QStringList& groups, const QStringList& defaultOrder,
                                    const QString& moved) {
  changes.erase(std::remove_if(changes.begin(), changes.end(),
                               [&tab](const LayoutChange& change) {
                                 return change.op == LayoutChange::Op::Group && change.place == tab;
                               }),
                changes.end());
  // The default's groups in the order wanted, with their places in the
  // default.
  QStringList wanted;
  std::vector<int> places;
  for (const QString& group : groups) {
    const qsizetype at = defaultOrder.indexOf(group);
    if (at >= 0 && !wanted.contains(group)) {
      wanted << group;
      places.push_back(static_cast<int>(at));
    }
  }
  // The most groups that keep the default's order among themselves (a
  // longest increasing run of their places), `moved` counted a little less
  // so that of two runs as long the one without it is kept.
  const std::size_t count = places.size();
  std::vector<int> best(count, 0);
  std::vector<int> previous(count, -1);
  int last = -1;
  for (std::size_t i = 0; i < count; ++i) {
    const int weight = wanted.at(static_cast<qsizetype>(i)) == moved ? 1 : 2;
    best[i] = weight;
    for (std::size_t j = 0; j < i; ++j) {
      if (places[j] < places[i] && best[j] + weight > best[i]) {
        best[i] = best[j] + weight;
        previous[i] = static_cast<int>(j);
      }
    }
    if (last < 0 || best[i] > best[static_cast<std::size_t>(last)]) {
      last = static_cast<int>(i);
    }
  }
  std::vector<bool> kept(count, false);
  for (int i = last; i >= 0; i = previous[static_cast<std::size_t>(i)]) {
    kept[static_cast<std::size_t>(i)] = true;
  }
  // The others moved, left to right, each after the group before it in the
  // order wanted: applied in turn to the default's order they give it.
  for (std::size_t i = 0; i < count; ++i) {
    if (kept[i]) {
      continue;
    }
    LayoutChange change;
    change.op = LayoutChange::Op::Group;
    change.place = tab;
    change.id = wanted.at(static_cast<qsizetype>(i));
    change.after = i > 0 ? wanted.at(static_cast<qsizetype>(i) - 1) : QString();
    changes.push_back(change);
  }
}

QStringList LayoutUserLayer::movedGroup(const QStringList& order, const QStringList& shown, const QString& group,
                                        int step) {
  const qsizetype at = shown.indexOf(group);
  const qsizetype next = at + step;
  if (at < 0 || (step != -1 && step != 1) || next < 0 || next >= shown.size()) {
    return {};
  }
  const qsizetype from = order.indexOf(group);
  const qsizetype to = order.indexOf(shown.at(next));
  if (from < 0 || to < 0) {
    return {};
  }
  QStringList moved = order;
  moved.swapItemsAt(from, to);
  return moved;
}

CommandLayout LayoutUserLayer::applied(const CommandLayout& base) const {
  CommandLayout layout = base;
  for (const LayoutChange& change : changes) {
    if (change.op == LayoutChange::Op::Group) {
      // The group taken out and put after `after` (first when empty); the
      // groups not moved (a later version's too) stay where they are.
      const auto tab = std::find_if(layout.tabs.begin(), layout.tabs.end(),
                                    [&change](const LayoutTab& candidate) { return candidate.key == change.place; });
      std::vector<LayoutGroup>* groups = tab != layout.tabs.end() ? &tab->groups : nullptr;
      const auto keyed = [groups](const QString& key) {
        return std::find_if(groups->begin(), groups->end(),
                            [&key](const LayoutGroup& candidate) { return candidate.key == key; });
      };
      QString missing;
      if (groups == nullptr || keyed(change.id) == groups->end()) {
        missing = change.id;
      } else if (change.after == change.id ||
                 (!change.after.isEmpty() && keyed(change.after) == groups->end())) {
        missing = change.after;
      }
      if (!missing.isEmpty()) {
        qInfo().noquote() << QStringLiteral("Layout: the user's move of the group %1 skipped: no group %2 on %3")
                                 .arg(change.id, missing, change.place);
        continue;
      }
      const auto from = keyed(change.id);
      LayoutGroup group = std::move(*from);
      groups->erase(from);
      const auto at = change.after.isEmpty() ? groups->begin() : keyed(change.after) + 1;
      groups->insert(at, std::move(group));
      continue;
    }
    std::vector<LayoutEntry>* entries = layout.entries(change.place);
    if (entries == nullptr) {
      qInfo().noquote() << QStringLiteral("Layout: the user's %1 of %2 skipped: no place %3")
                               .arg(opName(change.op), change.id, shownPath(change.place));
      continue;
    }
    switch (change.op) {
    case LayoutChange::Op::Hide:
      entries->erase(std::remove_if(entries->begin(), entries->end(),
                                    [&change](const LayoutEntry& entry) {
                                      return entry.kind == LayoutEntry::Kind::Command && entry.id == change.id;
                                    }),
                     entries->end());
      break;
    case LayoutChange::Op::Add: {
      if (commandIndex(*entries, change.id) >= 0) {
        break;
      }
      const int after = change.after.isEmpty() ? -1 : commandIndex(*entries, change.after);
      const auto at = after >= 0 ? entries->begin() + after + 1 : entries->end();
      entries->insert(at, LayoutEntry::command(change.id));
      break;
    }
    case LayoutChange::Op::Button: {
      const int index = commandIndex(*entries, change.id);
      if (index >= 0) {
        (*entries)[static_cast<std::size_t>(index)].button = change.on;
      }
      break;
    }
    case LayoutChange::Op::Order: {
      // The listed commands' places in the list, filled in the given order.
      std::vector<std::size_t> positions;
      std::vector<LayoutEntry> moved;
      for (const QString& id : change.ids) {
        const int index = commandIndex(*entries, id);
        if (index >= 0) {
          positions.push_back(static_cast<std::size_t>(index));
          moved.push_back((*entries)[static_cast<std::size_t>(index)]);
        }
      }
      std::sort(positions.begin(), positions.end());
      for (std::size_t i = 0; i < positions.size(); ++i) {
        (*entries)[positions[i]] = moved[i];
      }
      break;
    }
    case LayoutChange::Op::Group:
      break; // above
    }
  }
  return layout;
}

// ---------------------------------------------------------------------------
// The store

CommandLayoutStore::CommandLayoutStore(const QString& defaultPath, QObject* parent)
    : QObject(parent), m_defaultPath(defaultPath.isEmpty() ? kDefaultResource : defaultPath) {
  load();
}

CommandLayoutStore& CommandLayoutStore::instance() {
  // Made at the first use and kept to the end, after the windows.
  static CommandLayoutStore* store = new CommandLayoutStore();
  return *store;
}

int CommandLayoutStore::loadCount() { return s_loadCount; }

QString CommandLayoutStore::settingsKey() { return QStringLiteral("commandLayout/user"); }

QString CommandLayoutStore::oldPinsGroup() { return QStringLiteral("toolbarPins"); }

bool CommandLayoutStore::migratePins() {
  QSettings settings;
  settings.beginGroup(oldPinsGroup());
  const QStringList tabs = settings.childKeys();
  if (tabs.isEmpty()) {
    return false;
  }
  int count = 0;
  for (const QString& tab : tabs) {
    const QStringList ids = pinIds(settings.value(tab));
    // A tab the layer already has pins of keeps them (the layer is newer).
    const QString key = renamedTabPlace(tab);
    if (!ids.isEmpty() && !m_user.pins.contains(key)) {
      m_user.pins.insert(key, ids);
      count += static_cast<int>(ids.size());
    }
  }
  settings.endGroup();
  settings.remove(oldPinsGroup());
  qInfo().noquote() << QStringLiteral("Layout: moved %1 toolbar pins into the user layer").arg(count);
  return true;
}

void CommandLayoutStore::save() {
  QSettings settings;
  if (m_user.isEmpty()) {
    settings.remove(settingsKey());
    m_stored.clear();
  } else {
    m_stored = QString::fromUtf8(QJsonDocument(m_user.toJson()).toJson(QJsonDocument::Compact));
    settings.setValue(settingsKey(), m_stored);
  }
}

void CommandLayoutStore::setPins(const QString& tab, const QStringList& ids) {
  if (ids.isEmpty()) {
    m_user.pins.remove(tab);
  } else {
    m_user.pins.insert(tab, ids);
  }
  save();
}

void CommandLayoutStore::load() {
  ++s_loadCount;
  m_notice.clear();
  QFile file(m_defaultPath);
  QString error;
  if (!file.open(QIODevice::ReadOnly)) {
    qWarning().noquote() << QStringLiteral("Layout: the default layout %1 cannot be read").arg(m_defaultPath);
    m_defaults = CommandLayout();
  } else {
    QJsonParseError parse;
    const QJsonDocument document = QJsonDocument::fromJson(file.readAll(), &parse);
    m_defaults = CommandLayout::fromJson(document.object(), &error);
    if (parse.error != QJsonParseError::NoError || !error.isEmpty()) {
      qWarning().noquote() << QStringLiteral("Layout: the default layout is malformed: %1")
                                  .arg(parse.error != QJsonParseError::NoError ? parse.errorString() : error);
    }
  }
  m_user = LayoutUserLayer();
  const QString stored = QSettings().value(settingsKey()).toString();
  m_stored = stored;
  if (!stored.isEmpty()) {
    QJsonParseError parse;
    const QJsonDocument document = QJsonDocument::fromJson(stored.toUtf8(), &parse);
    error.clear();
    if (parse.error == QJsonParseError::NoError && document.isObject()) {
      m_user = LayoutUserLayer::fromJson(document.object(), &error);
    } else {
      error = parse.errorString();
    }
    if (!error.isEmpty()) {
      // The default instead; the stored layer stays until the user changes
      // or restores the layout.
      m_user = LayoutUserLayer();
      m_notice = tr("Your toolbar and menu layout could not be read, so the default layout is shown.");
      qWarning().noquote() << QStringLiteral("Layout: the user's layout is malformed (%1): the default is used")
                                  .arg(error);
    }
  }
  // The toolbar's pins of an earlier version (mitcad#1951), once.
  if (migratePins()) {
    save();
  }
  apply();
  qInfo().noquote() << QStringLiteral("Layout: loaded (%1 changes of the user's)").arg(static_cast<int>(m_user.changes.size()));
}

void CommandLayoutStore::apply() { m_layout = m_user.applied(m_defaults); }

void CommandLayoutStore::setUserLayer(const LayoutUserLayer& layer) {
  ++s_loadCount;
  m_user = layer;
  m_notice.clear();
  save();
  apply();
  qInfo().noquote() << QStringLiteral("Layout: changed (%1 changes of the user's)").arg(static_cast<int>(m_user.changes.size()));
  emit changed();
}

void CommandLayoutStore::restoreDefault() {
  setUserLayer(LayoutUserLayer());
  qInfo().noquote() << QStringLiteral("Layout: the default restored");
}

void CommandLayoutStore::reload() {
  load();
  emit changed();
}

bool CommandLayoutStore::reloadIfChanged() {
  QSettings settings;
  settings.beginGroup(oldPinsGroup());
  const bool oldPins = !settings.childKeys().isEmpty();
  settings.endGroup();
  if (!oldPins && settings.value(settingsKey()).toString() == m_stored) {
    return false;
  }
  reload();
  return true;
}

const CommandLayout& commandLayout() { return CommandLayoutStore::instance().layout(); }

LayoutPlace defaultHome(const QString& id) { return CommandLayoutStore::instance().defaults().home(id); }

} // namespace mitcad
