// SPDX-License-Identifier: MIT
// Material ▸ of the context menus (mitcad#1046): the design's materials
// and the targets' current one first, then the library materials assigned
// last that the design does not have (the newest first, at most eight);
// the check mark only when every target has the same material; the recent
// list in the settings, the design's own materials never in it; the
// Replace Appearance question only for an appearance set by hand that the
// new material does not offer (mitcad#1067).
#include "print/MaterialMenuChoices.hpp"

#include <cstdio>

#include <QCoreApplication>
#include <QJsonArray>
#include <QJsonObject>
#include <QSettings>
#include <QString>
#include <QStringList>
#include <QTemporaryDir>
#include <QVector>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "material_menu.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::MaterialChoice;
using mitcad::MaterialMenuRows;

MaterialChoice choice(const char* key, bool inDesign) {
  MaterialChoice result;
  result.key = QString::fromLatin1(key);
  result.inDesign = inDesign;
  return result;
}

// As `library_items` lists them: the built-ins first, then by name.
QVector<MaterialChoice> items() {
  QVector<MaterialChoice> list;
  list.append(choice("mitcad/steel", false)); // 0: unused built-in
  list.append(choice("mitcad/abs", true));    // 1: a body uses it
  list.append(choice("mitcad/pla", false));   // 2
  list.append(choice("design/acme_petg", true)); // 3: the design's own
  list.append(choice("user/bench_pla", false));  // 4: in My library, not in the design
  list.append(choice("mitcad/petg", true));      // 5: the base library's, copied in
  list.append(choice("project/shop_asa", false)); // 6
  return list;
}

void rowTests() {
  // The design's in the items' order; the current one (default steel)
  // with them although no body names it.
  MaterialMenuRows rows = mitcad::materialMenuRows(items(), {QStringLiteral("mitcad/steel")}, {});
  CHECK(rows.design == (QVector<int>{0, 1, 3, 5}));
  CHECK(rows.recent.isEmpty());
  // Recent: the newest first; one in the design (petg, abs) and one gone
  // (nylon) left out.
  rows = mitcad::materialMenuRows(items(), {},
                                  {QStringLiteral("project/shop_asa"), QStringLiteral("mitcad/petg"),
                                   QStringLiteral("user/nylon"), QStringLiteral("user/bench_pla"),
                                   QStringLiteral("mitcad/abs"), QStringLiteral("mitcad/pla")});
  CHECK(rows.design == (QVector<int>{1, 3, 5}));
  CHECK(rows.recent == (QVector<int>{6, 4, 2}));
  // At most `limit` of them.
  rows = mitcad::materialMenuRows(items(), {},
                                  {QStringLiteral("project/shop_asa"), QStringLiteral("user/bench_pla"),
                                   QStringLiteral("mitcad/pla")},
                                  2);
  CHECK(rows.recent == (QVector<int>{6, 4}));
  // A recent one a target is made of is the design's, not recent.
  rows = mitcad::materialMenuRows(items(), {QStringLiteral("mitcad/pla")}, {QStringLiteral("mitcad/pla")});
  CHECK(rows.design == (QVector<int>{1, 2, 3, 5}));
  CHECK(rows.recent.isEmpty());
}

void checkMarkTests() {
  CHECK(mitcad::commonMaterial({}).isEmpty());
  CHECK(mitcad::commonMaterial({QStringLiteral("mitcad/abs")}) == QStringLiteral("mitcad/abs"));
  CHECK(mitcad::commonMaterial({QStringLiteral("mitcad/abs"), QStringLiteral("mitcad/abs")}) ==
        QStringLiteral("mitcad/abs"));
  // Two bodies of different materials, or one of none: no check mark.
  CHECK(mitcad::commonMaterial({QStringLiteral("mitcad/abs"), QStringLiteral("mitcad/pla")}).isEmpty());
  CHECK(mitcad::commonMaterial({QString(), QString()}).isEmpty());
}

void recentListTests() {
  const QStringList list{QStringLiteral("a"), QStringLiteral("b"), QStringLiteral("c")};
  CHECK(mitcad::withRecentMaterial(list, QStringLiteral("c")) ==
        (QStringList{QStringLiteral("c"), QStringLiteral("a"), QStringLiteral("b")}));
  CHECK(mitcad::withRecentMaterial(list, QStringLiteral("d"), 3) ==
        (QStringList{QStringLiteral("d"), QStringLiteral("a"), QStringLiteral("b")}));
  QStringList many;
  for (int i = 0; i < 12; ++i) {
    many = mitcad::withRecentMaterial(many, QStringLiteral("user/m%1").arg(i));
  }
  CHECK(many.size() == mitcad::kRecentMaterials);
  CHECK(many.first() == QStringLiteral("user/m11"));
  CHECK(many.last() == QStringLiteral("user/m4"));

  CHECK(mitcad::isLibraryPlace(QStringLiteral("mitcad")));
  CHECK(mitcad::isLibraryPlace(QStringLiteral("user")));
  CHECK(mitcad::isLibraryPlace(QStringLiteral("project")));
  CHECK(!mitcad::isLibraryPlace(QStringLiteral("design")));
  CHECK(!mitcad::isLibraryPlace(QString()));

  // Kept in the settings and read back; the design's own are not kept.
  CHECK(mitcad::recentMaterials().isEmpty());
  mitcad::rememberRecentMaterial(QStringLiteral("mitcad"), QStringLiteral("mitcad/petg"));
  mitcad::rememberRecentMaterial(QStringLiteral("design"), QStringLiteral("design/acme_petg"));
  mitcad::rememberRecentMaterial(QStringLiteral("user"), QStringLiteral("user/bench_pla"));
  mitcad::rememberRecentMaterial(QStringLiteral("mitcad"), QStringLiteral("mitcad/petg"));
  CHECK(mitcad::recentMaterials() == (QStringList{QStringLiteral("mitcad/petg"), QStringLiteral("user/bench_pla")}));
  QSettings().remove(QStringLiteral("materials"));
}

// The Appearance tool's recent appearances (mitcad#1685): the latest
// first, each once, at most five, kept in the settings; the picker lists
// those it knows.
void recentAppearanceTests() {
  CHECK(mitcad::recentAppearances().isEmpty());
  mitcad::rememberRecentAppearance(QStringLiteral("filament_black"));
  mitcad::rememberRecentAppearance(QStringLiteral("paint_red"));
  CHECK(mitcad::recentAppearances() ==
        (QStringList{QStringLiteral("paint_red"), QStringLiteral("filament_black")}));
  // Chosen again: first again, not twice; nothing for an empty id.
  mitcad::rememberRecentAppearance(QStringLiteral("filament_black"));
  mitcad::rememberRecentAppearance(QString());
  CHECK(mitcad::recentAppearances() ==
        (QStringList{QStringLiteral("filament_black"), QStringLiteral("paint_red")}));
  for (int i = 0; i < 7; ++i) {
    mitcad::rememberRecentAppearance(QStringLiteral("look%1").arg(i));
  }
  CHECK(mitcad::recentAppearances().size() == mitcad::kRecentAppearances);
  CHECK(mitcad::recentAppearances().first() == QStringLiteral("look6"));
  CHECK(mitcad::recentAppearances().last() == QStringLiteral("look2"));
  // Kept apart from the recent materials.
  CHECK(mitcad::recentMaterials().isEmpty());

  // The picker's rows: only the appearances it lists (a design's own of
  // another design, a deleted one: left out), in the recent order.
  const QStringList known{QStringLiteral("chrome"), QStringLiteral("paint_red"), QStringLiteral("filament_black")};
  const QStringList recent{QStringLiteral("filament_black"), QStringLiteral("custom1"), QStringLiteral("paint_red")};
  CHECK(mitcad::recentAppearanceRows(known, recent) ==
        (QStringList{QStringLiteral("filament_black"), QStringLiteral("paint_red")}));
  CHECK(mitcad::recentAppearanceRows(known, recent, 1) == QStringList{QStringLiteral("filament_black")});
  CHECK(mitcad::recentAppearanceRows(known, QStringList()).isEmpty());
  QSettings().remove(QStringLiteral("appearances/recent"));
  CHECK(mitcad::recentAppearances().isEmpty());
}

QJsonObject bodyRow(const char* appearance, bool byHand) {
  QJsonObject row{{QStringLiteral("uid"), QStringLiteral("b1")}, {QStringLiteral("name"), QStringLiteral("Body1")}};
  if (appearance != nullptr) {
    row.insert(QStringLiteral("appearance"), QString::fromLatin1(appearance));
  }
  if (byHand) {
    row.insert(QStringLiteral("appearance_set_by_hand"), true);
  }
  return row;
}

// Replace Appearance (mitcad#1067): asked only for an appearance set by
// hand that the new material does not offer.
void replaceAppearanceTests() {
  // A material's list from its fields: the default first, once, whether
  // the list has it or not.
  const QJsonObject pla{{QStringLiteral("appearance"), QStringLiteral("bright_green")},
                        {QStringLiteral("appearances"),
                         QJsonArray{QStringLiteral("plastic_black"), QStringLiteral("translucent_blue")}}};
  const QStringList offered{QStringLiteral("bright_green"), QStringLiteral("plastic_black"),
                            QStringLiteral("translucent_blue")};
  CHECK(mitcad::materialAppearances(pla) == offered);
  const QJsonObject row{{QStringLiteral("appearance"), QStringLiteral("bright_green")},
                        {QStringLiteral("appearances"),
                         QJsonArray{QStringLiteral("bright_green"), QStringLiteral("plastic_black"),
                                    QStringLiteral("translucent_blue")}}};
  CHECK(mitcad::materialAppearances(row) == offered);
  CHECK(mitcad::materialAppearances(QJsonObject{{QStringLiteral("appearance"), QStringLiteral("steel")}}) ==
        QStringList{QStringLiteral("steel")});
  CHECK(mitcad::materialAppearances(QJsonObject()).isEmpty());

  // Set by hand: asked unless the material offers it, its default or
  // another of its list.
  CHECK(mitcad::asksToReplaceAppearance(bodyRow("paint_red", true), offered));
  CHECK(!mitcad::asksToReplaceAppearance(bodyRow("bright_green", true), offered));
  CHECK(!mitcad::asksToReplaceAppearance(bodyRow("translucent_blue", true), offered));
  // Not set by hand (the material's own): never asked.
  CHECK(!mitcad::asksToReplaceAppearance(bodyRow("paint_red", false), offered));
  CHECK(!mitcad::asksToReplaceAppearance(bodyRow(nullptr, false), offered));
  // A material that offers none: asked about any set by hand.
  CHECK(mitcad::asksToReplaceAppearance(bodyRow("translucent_blue", true), QStringList()));
}

} // namespace

int materialMenuTests() {
  rowTests();
  checkMarkTests();
  replaceAppearanceTests();
  // The settings in a folder of the test's own, never the user's.
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QSettings::Format format = QSettings::defaultFormat();
  const QString organization = QCoreApplication::organizationName();
  const QString application = QCoreApplication::applicationName();
  QCoreApplication::setOrganizationName(QStringLiteral("MitcadTest"));
  QCoreApplication::setApplicationName(QStringLiteral("material_menu"));
  QSettings::setDefaultFormat(QSettings::IniFormat);
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, directory.path());
  CHECK(QSettings().fileName().startsWith(directory.path()));
  recentListTests();
  recentAppearanceTests();
  QSettings::setDefaultFormat(format);
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  return failures;
}
