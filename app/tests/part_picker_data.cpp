// SPDX-License-Identifier: MIT
// The part picker's data (mitcad#418): the order of parts and groups with
// stars (starred first, starred groups first in Favourites, stable within
// each class), the stars of configuration rows (mitcad#491), the stars'
// round trip through the settings (an INI file in a folder of the test's
// own), the groups' tree of two libraries' parts (mitcad#527: shared
// groups by name, a part in two groups listed once, a group's star over
// its subtree) and the design's used library parts read from the model's
// `library_usage` answer (core/model-tests/links/src/library_tests.rs and
// build/source_tests.rs check the model's side), and the grid's Supplier
// filter (mitcad#949).
#include "library/PartFavourites.hpp"
#include "library/PartGroups.hpp"
#include "library/PartSuppliers.hpp"
#include "library/UsedParts.hpp"

#include <cstdio>
#include <memory>
#include <stdexcept>

#include <QCoreApplication>
#include <QJsonArray>
#include <QJsonObject>
#include <QSettings>
#include <QString>
#include <QTemporaryDir>

#include "framework/Command.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "part_picker_data.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

// A model that answers `library_usage` with a fixed answer, or refuses it.
class FakeModel : public CommandContext {
public:
  QJsonObject answer;
  bool refuses = false;
  mutable QJsonObject asked;

  QJsonValue query(const QJsonObject& query) const override {
    asked = query;
    if (refuses) {
      throw std::runtime_error("unknown variant `library_usage`");
    }
    return answer;
  }
  std::shared_ptr<geometry::Shape> bodyShape(const QString&) const override { return nullptr; }
  TopoDS_Shape itemShape(const SelectionItem&) const override { return TopoDS_Shape(); }
  TopoDS_Shape analysisShape(const QJsonObject&) const override { return TopoDS_Shape(); }
};

void usedPartsTests() {
  FakeModel model;
  model.answer = QJsonObject{
      {QStringLiteral("parts"),
       QJsonArray{QJsonObject{{QStringLiteral("library"), QStringLiteral("parts")},
                              {QStringLiteral("url"), QStringLiteral("https://example.org/parts.git")},
                              {QStringLiteral("component"), QStringLiteral("p30")},
                              {QStringLiteral("count"), 4}},
                  QJsonObject{{QStringLiteral("library"), QStringLiteral("parts")},
                              {QStringLiteral("url"), QStringLiteral("")},
                              {QStringLiteral("component"), QStringLiteral("nut-m6")},
                              {QStringLiteral("count"), 1}},
                  // A timber family used in two sizes (mitcad#518): the
                  // component's total and each row's uses; a row with no
                  // name or no uses is left out.
                  QJsonObject{{QStringLiteral("library"), QStringLiteral("timber")},
                              {QStringLiteral("url"), QStringLiteral("")},
                              {QStringLiteral("component"), QStringLiteral("dimensioned-48")},
                              {QStringLiteral("count"), 37},
                              {QStringLiteral("rows"),
                               QJsonArray{QJsonObject{{QStringLiteral("config"), QStringLiteral("48x98")},
                                                      {QStringLiteral("count"), 35}},
                                          QJsonObject{{QStringLiteral("config"), QStringLiteral("48x48")},
                                                      {QStringLiteral("count"), 2}},
                                          QJsonObject{{QStringLiteral("config"), QStringLiteral("")},
                                                      {QStringLiteral("count"), 4}},
                                          QJsonObject{{QStringLiteral("config"), QStringLiteral("48x198")},
                                                      {QStringLiteral("count"), 0}}}}},
                  // Not a part: left out.
                  QJsonObject{{QStringLiteral("library"), QStringLiteral("parts")}}}}};
  const QList<UsedPart> parts = usedLibraryParts(model);
  CHECK(model.asked.value(QStringLiteral("query")).toString() == QStringLiteral("library_usage"));
  CHECK(parts.size() == 3);
  if (parts.size() == 3) {
    CHECK(parts[0].library == QStringLiteral("parts"));
    CHECK(parts[0].url == QStringLiteral("https://example.org/parts.git"));
    CHECK(parts[0].component == QStringLiteral("p30"));
    CHECK(parts[0].count == 4);
    CHECK(parts[0].rows.isEmpty());
    CHECK(parts[1].component == QStringLiteral("nut-m6"));
    CHECK(parts[1].url.isEmpty());
    CHECK(parts[1].count == 1);
    CHECK(parts[2].component == QStringLiteral("dimensioned-48"));
    CHECK(parts[2].count == 37);
    CHECK(parts[2].rows.size() == 2);
    if (parts[2].rows.size() == 2) {
      CHECK(parts[2].rows[0].config == QStringLiteral("48x98"));
      CHECK(parts[2].rows[0].count == 35);
      CHECK(parts[2].rows[1].config == QStringLiteral("48x48"));
      CHECK(parts[2].rows[1].count == 2);
    }
  }
  // The tiles' counts per size (mitcad#518): the size's own uses, none for
  // the sizes the design does not use, the component's total for a part
  // without rows.
  UsedCounts counts;
  counts.set(parts);
  const QString timber = QStringLiteral("timber");
  const QString family = QStringLiteral("dimensioned-48");
  CHECK(counts.of(timber, family, QStringLiteral("48x98")) == 35);
  CHECK(counts.of(timber, family, QStringLiteral("48x48")) == 2);
  CHECK(counts.of(timber, family, QStringLiteral("48x198")) == 0);
  CHECK(counts.of(timber, family) == 37);
  CHECK(counts.of(QStringLiteral("parts"), QStringLiteral("p30")) == 4);
  CHECK(counts.of(QStringLiteral("parts"), QStringLiteral("p40")) == 0);
  // Uses placed without a row are the default row's.
  CHECK(counts.of(timber, family, QStringLiteral("48x98"), true) == 35);
  UsedPart mixed;
  mixed.library = timber;
  mixed.component = QStringLiteral("batten");
  mixed.count = 6;
  mixed.rows.append({QStringLiteral("28x70"), 2});
  counts.set({mixed});
  CHECK(counts.of(timber, QStringLiteral("batten"), QStringLiteral("28x70")) == 2);
  CHECK(counts.of(timber, QStringLiteral("batten"), QStringLiteral("28x95"), true) == 4);
  CHECK(counts.of(timber, QStringLiteral("batten"), QStringLiteral("28x95"), false) == 0);
  CHECK(counts.of(timber, QStringLiteral("batten")) == 6);
  counts.set({});
  CHECK(counts.of(timber, QStringLiteral("batten")) == 0);
  // A design without library parts, and a model that refuses the query.
  model.answer = QJsonObject{{QStringLiteral("parts"), QJsonArray()}};
  CHECK(usedLibraryParts(model).isEmpty());
  model.refuses = true;
  CHECK(usedLibraryParts(model).isEmpty());
}

QList<OrderedPart> sampleParts() {
  return {
      {QStringLiteral("parts"), QStringLiteral("p30"), QStringLiteral("profiles"), QString(), false},   // 0
      {QStringLiteral("parts"), QStringLiteral("m6x12"), QStringLiteral("screws"), QString(), false},   // 1
      {QStringLiteral("parts"), QStringLiteral("p40"), QStringLiteral("profiles"), QString(), false},   // 2
      {QStringLiteral("other"), QStringLiteral("m6x12"), QStringLiteral("screws"), QString(), false},   // 3
      {QStringLiteral("parts"), QStringLiteral("nut-m6"), QStringLiteral("nuts"), QString(), false},    // 4
      {QStringLiteral("parts"), QStringLiteral("m6x16"), QStringLiteral("screws"), QString(), false},   // 5
  };
}

void orderAndSettingsTests() {
  PartFavourites& favourites = PartFavourites::instance();
  const QList<OrderedPart> parts = sampleParts();
  // Nothing starred: the given order.
  CHECK(pickerOrder(parts, false) == (QList<int>{0, 1, 2, 3, 4, 5}));
  CHECK(groupOrder({QStringLiteral("profiles"), QStringLiteral("screws"), QStringLiteral("nuts")}) ==
        (QStringList{QStringLiteral("profiles"), QStringLiteral("screws"), QStringLiteral("nuts")}));

  int changes = 0;
  const QMetaObject::Connection counting =
      QObject::connect(&favourites, &PartFavourites::changed, [&changes] { ++changes; });
  favourites.setStarred(QStringLiteral("parts"), QStringLiteral("nut-m6"), true);
  favourites.setStarred(QStringLiteral("parts"), QStringLiteral("p40"), true);
  favourites.setStarred(QStringLiteral("parts"), QStringLiteral("m6x16"), true);
  // Starring again changes nothing.
  favourites.setStarred(QStringLiteral("parts"), QStringLiteral("p40"), true);
  CHECK(changes == 3);
  QObject::disconnect(counting);
  // Keyed by library and component: the other library's m6x12 is not the
  // starred one.
  CHECK(favourites.starred(QStringLiteral("parts"), QStringLiteral("m6x16")));
  CHECK(!favourites.starred(QStringLiteral("other"), QStringLiteral("m6x16")));
  // Starred first, each class in the given order.
  CHECK(pickerOrder(parts, false) == (QList<int>{2, 4, 5, 0, 1, 3}));

  // A starred group comes first among the groups, and in Favourites its
  // parts first among the starred ones; elsewhere the group changes nothing.
  favourites.setGroupStarred(QStringLiteral("screws"), true);
  CHECK(favourites.groupStarred(QStringLiteral("screws")));
  CHECK(groupOrder({QStringLiteral("profiles"), QStringLiteral("screws"), QStringLiteral("nuts")}) ==
        (QStringList{QStringLiteral("screws"), QStringLiteral("profiles"), QStringLiteral("nuts")}));
  CHECK(pickerOrder(parts, true) == (QList<int>{5, 2, 4, 0, 1, 3}));
  CHECK(pickerOrder(parts, false) == (QList<int>{2, 4, 5, 0, 1, 3}));
  CHECK(pickerOrder({}, true).isEmpty());

  // The stars are in the settings: another reader finds them, in the order
  // they were starred.
  {
    const PartFavourites reread;
    CHECK(reread.starred(QStringLiteral("parts"), QStringLiteral("p40")));
    CHECK(reread.groupStarred(QStringLiteral("screws")));
    CHECK(!reread.groupStarred(QStringLiteral("nuts")));
    const QList<PartStar> expected{{QStringLiteral("parts"), QStringLiteral("nut-m6"), QString()},
                                   {QStringLiteral("parts"), QStringLiteral("p40"), QString()},
                                   {QStringLiteral("parts"), QStringLiteral("m6x16"), QString()}};
    CHECK(reread.starredParts() == expected);
  }
  CHECK(QSettings().value(QStringLiteral("partPicker/starredParts")).isValid());

  // Unstarred, and read back so.
  favourites.setStarred(QStringLiteral("parts"), QStringLiteral("p40"), false);
  favourites.setGroupStarred(QStringLiteral("screws"), false);
  CHECK(!favourites.starred(QStringLiteral("parts"), QStringLiteral("p40")));
  CHECK(pickerOrder(parts, true) == (QList<int>{4, 5, 0, 1, 2, 3}));
  {
    const PartFavourites reread;
    CHECK(!reread.starred(QStringLiteral("parts"), QStringLiteral("p40")));
    CHECK(!reread.groupStarred(QStringLiteral("screws")));
    CHECK(reread.starredParts().size() == 2);
  }
  // Settings written by hand with damage: what can be read is kept.
  QSettings().setValue(QStringLiteral("partPicker/starredParts"),
                       QByteArray(R"([{"library":"parts","component":"p30"},{"library":""},"x",)"
                                  R"({"library":"parts","component":"p30"}])"));
  {
    const PartFavourites reread;
    CHECK(reread.starredParts().size() == 1);
    CHECK(reread.starred(QStringLiteral("parts"), QStringLiteral("p30")));
  }
  QSettings().setValue(QStringLiteral("partPicker/starredParts"), QByteArray("not json"));
  CHECK(PartFavourites().starredParts().isEmpty());

  favourites.setStarred(QStringLiteral("parts"), QStringLiteral("nut-m6"), false);
  favourites.setStarred(QStringLiteral("parts"), QStringLiteral("m6x16"), false);
  CHECK(favourites.starredParts().isEmpty());
}

// A timber family's sizes, each a tile of its own (mitcad#484), its
// default row 48x148, and a part without rows.
QList<OrderedPart> timberTiles() {
  const QString library = QStringLiteral("timber");
  const QString family = QStringLiteral("dimensioned-48");
  const QString group = QStringLiteral("dimensioned-timber");
  return {
      {library, family, group, QStringLiteral("48x98"), false},                // 0
      {library, family, group, QStringLiteral("48x148"), true},                // 1
      {library, family, group, QStringLiteral("48x198"), false},               // 2
      {library, QStringLiteral("batten-28"), group, QString(), false},         // 3
  };
}

// The tiles of `parts` starred.
QList<int> starredOf(const PartFavourites& favourites, const QList<OrderedPart>& parts) {
  QList<int> starred;
  for (int i = 0; i < parts.size(); ++i) {
    const OrderedPart& part = parts.at(i);
    if (favourites.starred(part.library, part.component, part.row, part.defaultRow)) {
      starred << i;
    }
  }
  return starred;
}

// Stars of configuration rows (mitcad#491): a star stars one size, an old
// star without a row marks only the default row, a part without rows keeps
// its star as it was.
void rowTests() {
  PartFavourites& favourites = PartFavourites::instance();
  const QList<OrderedPart> tiles = timberTiles();
  const QString library = QStringLiteral("timber");
  const QString family = QStringLiteral("dimensioned-48");
  CHECK(favourites.starredParts().isEmpty());

  // One size starred: only its tile, first; the family without a row is not.
  favourites.setStarred(library, family, QStringLiteral("48x198"), false, true);
  CHECK(starredOf(favourites, tiles) == (QList<int>{2}));
  CHECK(!favourites.starred(library, family));
  CHECK(pickerOrder(tiles, false) == (QList<int>{2, 0, 1, 3}));
  CHECK(pickerOrder(tiles, true) == (QList<int>{2, 0, 1, 3}));
  // Kept with its row: another reader finds that size only.
  {
    const PartFavourites reread;
    CHECK(reread.starredParts() == (QList<PartStar>{{library, family, QStringLiteral("48x198")}}));
    CHECK(starredOf(reread, tiles) == (QList<int>{2}));
  }
  CHECK(QSettings().value(QStringLiteral("partPicker/starredParts")).toByteArray().contains("\"row\":\"48x198\""));
  // A second size, then the first one's star off: the second stays.
  favourites.setStarred(library, family, QStringLiteral("48x98"), false, true);
  CHECK(starredOf(favourites, tiles) == (QList<int>{0, 2}));
  favourites.setStarred(library, family, QStringLiteral("48x198"), false, false);
  CHECK(starredOf(favourites, tiles) == (QList<int>{0}));
  favourites.setStarred(library, family, QStringLiteral("48x98"), false, false);
  CHECK(favourites.starredParts().isEmpty());

  // Stars kept before mitcad#491, without rows: the family's marks only its
  // default row; the part without rows is starred as before.
  QSettings().setValue(QStringLiteral("partPicker/starredParts"),
                       QByteArray(R"([{"library":"timber","component":"dimensioned-48"},)"
                                  R"({"library":"timber","component":"batten-28"}])"));
  {
    const PartFavourites reread;
    CHECK(reread.starredParts().size() == 2);
    CHECK(starredOf(reread, tiles) == (QList<int>{1, 3}));
    CHECK(reread.starred(library, QStringLiteral("batten-28")));
    CHECK(reread.starred(library, family, QStringLiteral("48x148"), true));
    CHECK(!reread.starred(library, family, QStringLiteral("48x98"), false));
    CHECK(!reread.starred(library, family, QStringLiteral("48x198"), false));
  }
  // The same in the picker's order (the old stars made here as they were
  // kept: without a row).
  favourites.setStarred(library, family, true);
  favourites.setStarred(library, QStringLiteral("batten-28"), true);
  CHECK(starredOf(favourites, tiles) == (QList<int>{1, 3}));
  CHECK(pickerOrder(tiles, false) == (QList<int>{1, 3, 0, 2}));
  // Starring the default row again changes nothing; its star off turns the
  // old star off too; the part without rows keeps its own.
  int changes = 0;
  const QMetaObject::Connection counting =
      QObject::connect(&favourites, &PartFavourites::changed, [&changes] { ++changes; });
  favourites.setStarred(library, family, QStringLiteral("48x148"), true, true);
  CHECK(changes == 0);
  favourites.setStarred(library, family, QStringLiteral("48x148"), true, false);
  CHECK(changes == 1);
  QObject::disconnect(counting);
  CHECK(starredOf(favourites, tiles) == (QList<int>{3}));
  CHECK(!favourites.starred(library, family));
  CHECK(favourites.starredParts() == (QList<PartStar>{{library, QStringLiteral("batten-28"), QString()}}));
  // Groups' stars as before.
  favourites.setGroupStarred(QStringLiteral("dimensioned-timber"), true);
  CHECK(pickerOrder(tiles, true) == (QList<int>{3, 0, 1, 2}));
  favourites.setGroupStarred(QStringLiteral("dimensioned-timber"), false);
  favourites.setStarred(library, QStringLiteral("batten-28"), false);
  CHECK(favourites.starredParts().isEmpty());
}

PartGroupPath group(const QString& category, const QStringList& names, const QList<int>& order) {
  return PartGroupPath{category, names, order};
}

// The groups' tree from two libraries' parts (mitcad#527), as
// `library_search` gives their groups: the first library's Wood (a parent
// by id), a slot screw in two groups (Slot profile and General fasteners),
// a nut, a category of no name; the second library's "wood" (the same
// group by its name, any case) with its sheets, and a Workshop of its own.
void groupTreeTests() {
  CHECK(groupKey({QStringLiteral(" Wood "), QStringLiteral("Dimensioned  timber")}) ==
        QStringLiteral("wood/dimensioned timber"));
  CHECK(groupLabel(QStringLiteral("linear-parts")) == QStringLiteral("Linear parts"));
  CHECK(groupLabel(QStringLiteral("Sheets")) == QStringLiteral("Sheets"));
  CHECK(groupLabel(QStringLiteral("M5 nuts")) == QStringLiteral("M5 nuts"));

  // The parts: their groups, the first their home.
  const QList<QList<PartGroupPath>> parts{
      // 0: a beam of the first library
      {group(QStringLiteral("timber"), {QStringLiteral("Wood"), QStringLiteral("Dimensioned timber")}, {0, 1})},
      // 1: a slot screw in two groups
      {group(QStringLiteral("slot-screws"), {QStringLiteral("Slot profile"), QStringLiteral("Screws")}, {3, 3}),
       group(QStringLiteral("general-screws"), {QStringLiteral("General fasteners"), QStringLiteral("Screws")},
             {4, 4})},
      // 2: a nut
      {group(QStringLiteral("nuts"), {QStringLiteral("General fasteners"), QStringLiteral("Nuts")}, {5, 5})},
      // 3: a category of no name
      {group(QStringLiteral("linear-parts"), {QStringLiteral("linear-parts")}, {9})},
      // 4: the second library's sheets in its "wood"
      {group(QStringLiteral("sheets"), {QStringLiteral("wood"), QStringLiteral("Sheets")}, {0, 1})},
      // 5: its workshop
      {group(QStringLiteral("workshop"), {QStringLiteral("Workshop")}, {2})},
  };
  PartGroupTree tree;
  for (const QList<PartGroupPath>& groups : parts) {
    for (const PartGroupPath& one : groups) {
      tree.add(one);
    }
  }
  tree.finish();
  // The top groups by the libraries' order, then by name; one Wood.
  CHECK(tree.top() == (QStringList{QStringLiteral("wood"), QStringLiteral("workshop"), QStringLiteral("slot profile"),
                                   QStringLiteral("general fasteners"), QStringLiteral("linear-parts")}));
  const PartGroupTree::Node* wood = tree.node(QStringLiteral("wood"));
  CHECK(wood != nullptr);
  if (wood != nullptr) {
    CHECK(wood->name == QStringLiteral("Wood"));
    CHECK(wood->parent.isEmpty());
    CHECK(wood->children ==
          (QStringList{QStringLiteral("wood/dimensioned timber"), QStringLiteral("wood/sheets")}));
  }
  const PartGroupTree::Node* screws = tree.node(QStringLiteral("general fasteners/screws"));
  CHECK(screws != nullptr);
  if (screws != nullptr) {
    CHECK(screws->name == QStringLiteral("Screws"));
    CHECK(screws->path == QStringLiteral("General fasteners/Screws"));
    CHECK(screws->parent == QStringLiteral("general fasteners"));
    CHECK(screws->categories == QStringList{QStringLiteral("general-screws")});
    CHECK(screws->group.names == (QStringList{QStringLiteral("General fasteners"), QStringLiteral("Screws")}));
  }
  const PartGroupTree::Node* linear = tree.node(QStringLiteral("linear-parts"));
  CHECK(linear != nullptr && linear->name == QStringLiteral("Linear parts") && linear->children.isEmpty());
  CHECK(tree.subtree(QStringLiteral("general fasteners")) ==
        (QStringList{QStringLiteral("general fasteners"), QStringLiteral("general fasteners/screws"),
                     QStringLiteral("general fasteners/nuts")}));
  CHECK(tree.subtree(QStringLiteral("no such group")).isEmpty());
  CHECK(!tree.contains(QStringLiteral("screws")));

  // A group lists its own parts and its subgroups', each part once: the
  // slot screw in both of its groups and once in General fasteners.
  const auto members = [&parts](const QString& key) {
    QList<int> in;
    for (int i = 0; i < parts.size(); ++i) {
      if (inGroup(parts.at(i), key)) {
        in << i;
      }
    }
    return in;
  };
  CHECK(members(QStringLiteral("wood")) == (QList<int>{0, 4}));
  CHECK(members(QStringLiteral("wood/sheets")) == QList<int>{4});
  CHECK(members(QStringLiteral("slot profile/screws")) == QList<int>{1});
  CHECK(members(QStringLiteral("general fasteners/screws")) == QList<int>{1});
  CHECK(members(QStringLiteral("general fasteners")) == (QList<int>{1, 2}));
  CHECK(members(QStringLiteral("general")).isEmpty());
  CHECK(members(QString()).isEmpty());

  // Stars: a group's star stars its subtree (one change); in Favourites a
  // part listed once, first when any of its groups is starred.
  PartFavourites& favourites = PartFavourites::instance();
  const QString library = QStringLiteral("parts");
  QList<OrderedPart> order;
  QList<QStringList> groups;
  const QStringList components{QStringLiteral("beam"), QStringLiteral("slot-screw"), QStringLiteral("nut"),
                               QStringLiteral("rail"), QStringLiteral("plywood"), QStringLiteral("clamp")};
  for (int i = 0; i < parts.size(); ++i) {
    QStringList keys;
    for (const PartGroupPath& one : parts.at(i)) {
      keys << groupKey(one.names);
    }
    order.append({library, components.at(i), keys.first(), QString(), false});
    groups.append(keys);
  }
  favourites.setStarred(library, QStringLiteral("nut"), true);
  favourites.setStarred(library, QStringLiteral("slot-screw"), true);
  CHECK(pickerOrder(order, true, groups) == (QList<int>{1, 2, 0, 3, 4, 5}));
  int changes = 0;
  const QMetaObject::Connection counting =
      QObject::connect(&favourites, &PartFavourites::changed, [&changes] { ++changes; });
  favourites.setGroupsStarred(tree.subtree(QStringLiteral("general fasteners")), true);
  CHECK(changes == 1);
  CHECK(favourites.groupStarred(QStringLiteral("general fasteners/nuts")));
  // The nut's group and the screw's second group are starred: both first,
  // the screw once, in the given order.
  CHECK(pickerOrder(order, true, groups) == (QList<int>{1, 2, 0, 3, 4, 5}));
  // Only by its home group the screw would not be first.
  CHECK(pickerOrder(order, true) == (QList<int>{2, 1, 0, 3, 4, 5}));
  favourites.setGroupsStarred(tree.subtree(QStringLiteral("general fasteners")), false);
  favourites.setGroupsStarred(tree.subtree(QStringLiteral("general fasteners")), false);
  CHECK(changes == 2);
  QObject::disconnect(counting);
  CHECK(!favourites.groupStarred(QStringLiteral("general fasteners")));
  {
    const PartFavourites reread;
    CHECK(!reread.groupStarred(QStringLiteral("general fasteners/screws")));
  }
  favourites.setStarred(library, QStringLiteral("nut"), false);
  favourites.setStarred(library, QStringLiteral("slot-screw"), false);
  CHECK(favourites.starredParts().isEmpty());
}

// The grid's Supplier filter (mitcad#949): a component's suppliers from
// library_search (its rows' and its own), a tile of one row's, the filter's
// choices, a component no row of which the supplier sells hidden, and the
// choice kept in the settings.
void supplierFilterTests() {
  const QJsonObject screw{
      {QStringLiteral("id"), QStringLiteral("iso4762")},
      {QStringLiteral("suppliers"),
       QJsonArray{QJsonObject{{QStringLiteral("id"), QStringLiteral("wurth")}, {QStringLiteral("name"), QStringLiteral("Würth")}},
                  QJsonObject{{QStringLiteral("id"), QStringLiteral("etra")}, {QStringLiteral("name"), QStringLiteral("Etra")}}}}};
  QJsonObject nut{{QStringLiteral("id"), QStringLiteral("iso4032")}};
  nut.insert(QStringLiteral("suppliers"),
             QJsonArray{QJsonObject{{QStringLiteral("id"), QStringLiteral("etra")}, {QStringLiteral("name"), QStringLiteral("Etra")}}});
  // A BUILD part of a maker of its own, without rows' articles.
  const QJsonObject profile{{QStringLiteral("id"), QStringLiteral("profile-30")},
                            {QStringLiteral("supplier"), QStringLiteral("Profiles Inc")}};
  const QJsonObject washer{{QStringLiteral("id"), QStringLiteral("iso7089")}};
  const QList<PartSupplier> screwSuppliers = componentSuppliers(screw);
  CHECK(screwSuppliers.size() == 2 && screwSuppliers.first().name == QStringLiteral("Würth"));
  const QList<PartSupplier> profileSuppliers = componentSuppliers(profile);
  CHECK(profileSuppliers.size() == 1 && profileSuppliers.first().id == QStringLiteral("Profiles Inc"));
  CHECK(componentSuppliers(washer).isEmpty());
  // The choices: each supplier once, by name.
  const QList<QList<PartSupplier>> lists{screwSuppliers, componentSuppliers(nut), profileSuppliers,
                                         componentSuppliers(washer)};
  const QList<PartSupplier> choices = supplierChoices(lists);
  QStringList names;
  for (const PartSupplier& one : choices) {
    names << one.name;
  }
  CHECK(names == (QStringList{QStringLiteral("Etra"), QStringLiteral("Profiles Inc"), QStringLiteral("Würth")}));
  // Würth chosen: the nut and the washer are hidden, the screw stays.
  const auto shown = [&lists](const QString& supplier) {
    QList<int> kept;
    for (int i = 0; i < lists.size(); ++i) {
      if (soldBy(lists.at(i), supplier)) {
        kept << i;
      }
    }
    return kept;
  };
  CHECK(shown(QStringLiteral("wurth")) == QList<int>{0});
  CHECK(shown(QStringLiteral("etra")) == (QList<int>{0, 1}));
  CHECK(shown(QString()) == (QList<int>{0, 1, 2, 3}));
  CHECK(supplierNames(screwSuppliers) == QStringLiteral("Würth, Etra"));
  // A tile of one row: that row's suppliers, then the component's own.
  const QJsonObject table{
      {QStringLiteral("rows"),
       QJsonArray{QJsonObject{{QStringLiteral("name"), QStringLiteral("M6x8")},
                              {QStringLiteral("supplier_parts"),
                               QJsonArray{QJsonObject{{QStringLiteral("supplier"), QStringLiteral("wurth")},
                                                      {QStringLiteral("supplier_name"), QStringLiteral("Würth")},
                                                      {QStringLiteral("part_number"), QStringLiteral("0084 6 8")}}}}},
                  QJsonObject{{QStringLiteral("name"), QStringLiteral("M6x10")}}}}};
  const QList<PartSupplier> row = rowPartSuppliers(table, QStringLiteral("M6x8"), QStringLiteral("Maker"));
  CHECK(row.size() == 2 && row.first().id == QStringLiteral("wurth") && row.last().id == QStringLiteral("Maker"));
  CHECK(!soldBy(rowPartSuppliers(table, QStringLiteral("M6x10"), QString()), QStringLiteral("wurth")));
  // The choice kept in the settings, All when cleared.
  CHECK(savedSupplierFilter().isEmpty());
  saveSupplierFilter(QStringLiteral("wurth"));
  CHECK(savedSupplierFilter() == QStringLiteral("wurth"));
  saveSupplierFilter(QString());
  CHECK(savedSupplierFilter().isEmpty());
}

// The groups' counts under the filters (mitcad#1015): each group counts
// the parts the Supplier filter and the search leave, its subgroups'
// included and a part in two of them once; the empty groups hide (a parent
// with only empty subgroups too, the picker's own rows only while
// filtering), the chosen group and the groups above it stay.
void groupCountTests() {
  CHECK(searchMatches(QStringLiteral("Hexagon nut M5"), QStringLiteral("m5  NUT")));
  CHECK(!searchMatches(QStringLiteral("Hexagon nut M5"), QStringLiteral("m6")));
  CHECK(searchMatches(QStringLiteral("Hexagon nut M5"), QString()));

  struct Part {
    QList<PartGroupPath> groups;
    QList<PartSupplier> suppliers;
    QString fields;
  };
  const PartSupplier wurth{QStringLiteral("wurth"), QStringLiteral("Würth")};
  const PartSupplier etra{QStringLiteral("etra"), QStringLiteral("Etra")};
  const QStringList fasteners{QStringLiteral("General fasteners")};
  const auto in = [](const QStringList& names) { return group(QString(), names, QList<int>{0, 0}); };
  const QList<Part> parts{
      // 0: a beam of no supplier
      {{in({QStringLiteral("Wood"), QStringLiteral("Beams")})}, {}, QStringLiteral("Beam timber")},
      // 1: plywood of one supplier
      {{in({QStringLiteral("Wood"), QStringLiteral("Sheets")})}, {etra}, QStringLiteral("Plywood sheet")},
      // 2: a slot screw in two groups
      {{in({QStringLiteral("Slot profile"), QStringLiteral("Screws")}), in(fasteners + QStringList{QStringLiteral("Screws")})},
       {wurth},
       QStringLiteral("Slot screw M5")},
      // 3: a socket head cap screw of both suppliers
      {{in(fasteners + QStringList{QStringLiteral("Screws")})}, {wurth, etra}, QStringLiteral("Socket head cap screw M5")},
      // 4: a nut
      {{in(fasteners + QStringList{QStringLiteral("Nuts")})}, {etra}, QStringLiteral("Hexagon nut M5")},
      // 5: a clamp of no group (Other)
      {{}, {}, QStringLiteral("Clamp")},
      // 6: a kit in two subgroups of General fasteners
      {{in(fasteners + QStringList{QStringLiteral("Screws")}), in(fasteners + QStringList{QStringLiteral("Nuts")})},
       {wurth},
       QStringLiteral("Kit M6")},
  };
  PartGroupTree tree;
  for (const Part& part : parts) {
    for (const PartGroupPath& one : part.groups) {
      tree.add(one);
    }
  }
  tree.finish();
  const QString other = QStringLiteral(":other");
  const QString favourites = QStringLiteral(":favourites");
  // The counts of the parts the supplier and the search leave, Other's too.
  const auto counted = [&parts, &other](const QString& supplier, const QString& search) {
    QList<QList<PartGroupPath>> grouped;
    int others = 0;
    for (const Part& part : parts) {
      if (soldBy(part.suppliers, supplier) && searchMatches(part.fields, search)) {
        grouped.append(part.groups);
        others += part.groups.isEmpty() ? 1 : 0;
      }
    }
    QHash<QString, int> counts = groupCounts(grouped);
    if (others > 0) {
      counts.insert(other, others);
    }
    return counts;
  };
  const QStringList special{favourites, other};
  const auto hidden = [&tree, &special](const QHash<QString, int>& counts, const QString& chosen, bool filtering) {
    QStringList keys = hiddenGroups(tree, counts, chosen, filtering, special).values();
    keys.sort();
    return keys;
  };

  // No filter: every group with its subgroups' parts, the kit once in
  // General fasteners; nothing hidden, Favourites (0) neither.
  const QHash<QString, int> all = counted(QString(), QString());
  CHECK(all.value(QStringLiteral("wood")) == 2);
  CHECK(all.value(QStringLiteral("wood/beams")) == 1);
  CHECK(all.value(QStringLiteral("slot profile")) == 1);
  CHECK(all.value(QStringLiteral("general fasteners")) == 4);
  CHECK(all.value(QStringLiteral("general fasteners/screws")) == 3);
  CHECK(all.value(QStringLiteral("general fasteners/nuts")) == 2);
  CHECK(all.value(other) == 1);
  CHECK(hidden(all, QStringLiteral(":all"), false).isEmpty());

  // Supplier Etra: Wood has its sheets, Beams and Slot profile none and
  // hide (Slot profile with its only, empty subgroup), so do Favourites
  // and Other.
  const QHash<QString, int> byEtra = counted(QStringLiteral("etra"), QString());
  CHECK(byEtra.value(QStringLiteral("wood")) == 1);
  CHECK(byEtra.value(QStringLiteral("wood/sheets")) == 1);
  CHECK(byEtra.value(QStringLiteral("general fasteners")) == 2);
  CHECK(byEtra.value(QStringLiteral("general fasteners/screws")) == 1);
  CHECK(byEtra.value(QStringLiteral("general fasteners/nuts")) == 1);
  CHECK(!byEtra.contains(QStringLiteral("wood/beams")) && !byEtra.contains(QStringLiteral("slot profile")));
  CHECK(hidden(byEtra, QStringLiteral(":all"), true) ==
        (QStringList{favourites, other, QStringLiteral("slot profile"), QStringLiteral("slot profile/screws"),
                     QStringLiteral("wood/beams")}));

  // Etra and the search "m5": only the screw and the nut. The chosen
  // Beams stays with (0), and Wood above it; Sheets hides.
  const QHash<QString, int> searched = counted(QStringLiteral("etra"), QStringLiteral("m5"));
  CHECK(searched.value(QStringLiteral("general fasteners")) == 2);
  CHECK(searched.value(QStringLiteral("wood/beams")) == 0 && searched.value(QStringLiteral("wood")) == 0);
  CHECK(hidden(searched, QStringLiteral("wood/beams"), true) ==
        (QStringList{favourites, other, QStringLiteral("slot profile"), QStringLiteral("slot profile/screws"),
                     QStringLiteral("wood/sheets")}));
  // Another group chosen: Beams and Wood hide too.
  CHECK(hidden(searched, QStringLiteral("general fasteners/nuts"), true) ==
        (QStringList{favourites, other, QStringLiteral("slot profile"), QStringLiteral("slot profile/screws"),
                     QStringLiteral("wood"), QStringLiteral("wood/beams"), QStringLiteral("wood/sheets")}));
  // Other chosen while it is empty: it stays.
  CHECK(!hidden(searched, other, true).contains(other));

  // Würth: the slot screw in both of its groups, the kit once in General
  // fasteners and in each of its subgroups.
  const QHash<QString, int> byWurth = counted(QStringLiteral("wurth"), QString());
  CHECK(byWurth.value(QStringLiteral("general fasteners")) == 3);
  CHECK(byWurth.value(QStringLiteral("general fasteners/screws")) == 3);
  CHECK(byWurth.value(QStringLiteral("general fasteners/nuts")) == 1);
  CHECK(byWurth.value(QStringLiteral("slot profile/screws")) == 1);
  CHECK(!byWurth.contains(QStringLiteral("wood")));
}

} // namespace

int partPickerDataTests() {
  usedPartsTests();
  // The stars in a settings folder of the test's own, never the user's.
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QSettings::Format format = QSettings::defaultFormat();
  const QString organization = QCoreApplication::organizationName();
  const QString application = QCoreApplication::applicationName();
  QCoreApplication::setOrganizationName(QStringLiteral("MitcadTest"));
  QCoreApplication::setApplicationName(QStringLiteral("part_picker_data"));
  QSettings::setDefaultFormat(QSettings::IniFormat);
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, directory.path());
  CHECK(QSettings().fileName().startsWith(directory.path()));
  orderAndSettingsTests();
  rowTests();
  groupTreeTests();
  supplierFilterTests();
  groupCountTests();
  QSettings::setDefaultFormat(format);
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  return failures;
}
