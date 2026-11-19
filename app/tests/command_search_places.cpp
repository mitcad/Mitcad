// SPDX-License-Identifier: MIT
// The command search's keywords and place line (mitcad#1983): the names
// users know a function by elsewhere find the command that does it (Change
// Parameters finds Edit Parameters…), the registry adds them to each
// command's keywords once, and each result says where the command sits,
// read from the layout held in memory: its home first, a command the user
// moved at its new place, a menu bar command in its menu, one on no
// toolbar said so.
#include "UnitTests.hpp"
#include "framework/CommandLayout.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/CommandSearchRank.hpp"

#include <cstdio>
#include <vector>

#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QSet>
#include <QString>
#include <QStringList>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "command_search_places.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::CommandDef;
using mitcad::CommandLayout;
using mitcad::CommandLayoutStore;
using mitcad::CommandRegistry;
using mitcad::LayoutUserLayer;
namespace search = mitcad::search;

void add(CommandRegistry& registry, const char* id, const char* name, const QStringList& keywords = {}) {
  CommandDef def;
  def.id = QString::fromLatin1(id);
  def.name = QString::fromUtf8(name);
  def.keywords = keywords;
  registry.add(def);
}

// The id of the command the search lists first for `typed`; empty for none.
QString first(const CommandRegistry& registry, const char* typed) {
  std::vector<search::Candidate> candidates;
  for (const auto& def : registry.commands()) {
    search::Candidate candidate;
    candidate.id = def->id;
    candidate.shown = def->name;
    candidate.english = def->name;
    candidate.keywords = def->keywords;
    candidates.push_back(candidate);
  }
  const search::Listing listing = search::list(candidates, search::normalizedText(QString::fromLatin1(typed)));
  if (listing.rows.empty()) {
    return QString();
  }
  return candidates[static_cast<size_t>(listing.rows.front())].id;
}

void keywordTests() {
  CommandRegistry registry;
  // The commands of the examples, with their own keywords, and others whose
  // names share words with the other names.
  add(registry, "solid.parameters", "Edit Parameters\xe2\x80\xa6", {QStringLiteral("parameters")});
  add(registry, "sketch.parameters", "Sketch Parameters");
  add(registry, "solid.materials", "Edit Materials\xe2\x80\xa6", {QStringLiteral("material")});
  add(registry, "solid.appearances", "Edit Appearances\xe2\x80\xa6");
  add(registry, "solid.material", "Physical Material");
  add(registry, "insert.image_reference", "Image Reference", {QStringLiteral("Canvas")});
  add(registry, "insert.calibrate_image_reference", "Calibrate Image Reference");
  add(registry, "assemble.derived_component", "Derived Component");
  add(registry, "insert.component", "Insert Component");
  add(registry, "assemble.drive_joint", "Drive Joint");
  add(registry, "assemble.joint", "Joint");
  add(registry, "assemble.animate_joint", "Animate Joint");
  add(registry, "assemble.contact_set", "Contact Set");
  add(registry, "inspect.interference", "Interference");
  add(registry, "manage.recompute_all", "Recompute All", {QStringLiteral("rebuild")});
  add(registry, "solid.form", "Form");

  // The names users know these functions by find them first.
  CHECK(first(registry, "change parameters") == QStringLiteral("solid.parameters"));
  CHECK(first(registry, "Change Parameters...") == QStringLiteral("solid.parameters"));
  CHECK(first(registry, "manage materials") == QStringLiteral("solid.materials"));
  CHECK(first(registry, "canvas") == QStringLiteral("insert.image_reference"));
  CHECK(first(registry, "derive") == QStringLiteral("assemble.derived_component"));
  CHECK(first(registry, "drive joints") == QStringLiteral("assemble.drive_joint"));
  CHECK(first(registry, "contact sets") == QStringLiteral("assemble.contact_set"));
  CHECK(first(registry, "compute all") == QStringLiteral("manage.recompute_all"));
  CHECK(first(registry, "create form") == QStringLiteral("solid.form"));
  CHECK(first(registry, "insert derive") == QStringLiteral("assemble.derived_component"));

  // Added to the command's keywords after its own, each word once (Canvas
  // is the command's own already).
  const CommandDef* image = registry.find(QStringLiteral("insert.image_reference"));
  CHECK(image != nullptr);
  if (image != nullptr) {
    CHECK(image->keywords.front() == QStringLiteral("Canvas"));
    CHECK(image->keywords.count(QStringLiteral("canvas")) == 0);
    CHECK(image->keywords.contains(QStringLiteral("underlay")));
  }
  const CommandDef* parameters = registry.find(QStringLiteral("solid.parameters"));
  CHECK(parameters != nullptr && parameters->keywords.contains(QStringLiteral("change parameters")));
  // A command without other names keeps its keywords as they were.
  CHECK(search::withOtherNames(QStringLiteral("sketch.parameters"), {QStringLiteral("a")}) ==
        QStringList{QStringLiteral("a")});

  // The table: each command once, its words in lower case, and every word
  // finds its command.
  QSet<QString> ids;
  for (const auto& [id, words] : search::otherNames()) {
    CHECK(!ids.contains(id));
    ids.insert(id);
    CHECK(!words.isEmpty());
    search::Candidate candidate;
    candidate.id = id;
    candidate.shown = id;
    candidate.english = id;
    candidate.keywords = search::withOtherNames(id, {});
    for (const QString& word : words) {
      CHECK(word == word.toLower());
      CHECK(search::score(candidate, search::normalizedText(word)) >= 0);
    }
  }
}

CommandLayout shippedLayout() {
  QFile file(QStringLiteral(":/layout/default.json"));
  CHECK(file.open(QIODevice::ReadOnly));
  QString error;
  CommandLayout layout = CommandLayout::fromJson(QJsonDocument::fromJson(file.readAll()).object(), &error);
  CHECK(error.isEmpty());
  return layout;
}

void placeTests() {
  search::PlaceCaptions captions;
  // The keys as they are; a menu's title with its mnemonic, which the line
  // leaves out.
  captions.ribbon = [](const QString& key) { return key; };
  captions.menu = [](const QString& key) { return QStringLiteral("&") + key.left(1) + key.mid(1).toLower(); };
  captions.menuBar = QStringLiteral("Menu bar");
  captions.nowhere = QStringLiteral("No toolbar place");
  const QString sweep = QStringLiteral("solid.sweep");
  const QString parameters = QStringLiteral("solid.parameters");

  const CommandLayout base = shippedLayout();
  const int loads = CommandLayoutStore::loadCount();
  // A toolbar command: its tab and group.
  CHECK(search::placeLine(base, sweep, captions) == QStringLiteral("SOLID › CREATE"));
  // From a submenu: its group.
  CHECK(base.placesOf(QStringLiteral("solid.rectangular_pattern")) ==
        QStringList{QStringLiteral("SOLID/CREATE/Pattern")});
  CHECK(search::placeLine(base, QStringLiteral("solid.rectangular_pattern"), captions) ==
        QStringLiteral("SOLID › CREATE"));
  // A menu bar command, without the mnemonic.
  CHECK(search::placeLine(base, QStringLiteral("file.open"), captions) ==
        QStringLiteral("Menu bar › File"));
  // On the toolbar and in a menu: the toolbar's place first.
  CHECK(search::placeLine(base, parameters, captions) ==
        QStringLiteral("MANAGE › PARAMETERS, Menu bar › Edit"));
  // A repeat (mitcad#1975) never leads: New Component's home is Assemble,
  // although its repeat in Create comes first in the layout.
  CHECK(base.placesOf(QStringLiteral("solid.new_component")) ==
        (QStringList{QStringLiteral("SOLID/ASSEMBLE"), QStringLiteral("SOLID/CREATE")}));
  CHECK(search::placeLine(base, QStringLiteral("solid.new_component"), captions) ==
        QStringLiteral("SOLID › ASSEMBLE, SOLID › CREATE"));
  // Material: a tool at home in Solid › Modify, also on the Make
  // workspace's three tabs (the owner, 18.11.2026); Edit Materials… in
  // Manage (the owner, 17.11.2026).
  CHECK(search::placeLine(base, QStringLiteral("solid.material"), captions) ==
        QStringLiteral("SOLID › MODIFY, MAKE_PRINT › MATERIALS, MAKE_LASER › MATERIALS, MAKE_CNC › MATERIALS"));
  CHECK(search::placeLine(base, QStringLiteral("solid.materials"), captions) == QStringLiteral("MANAGE › MATERIALS"));
  // Create Sketch: Solid first, then its repeats on the new tabs.
  CHECK(base.placesOf(QStringLiteral("sketch.create")).front() == QStringLiteral("SOLID/CREATE"));
  CHECK(base.placesOf(QStringLiteral("sketch.create")).contains(QStringLiteral("SURFACE/CREATE")));
  // On no toolbar and in no menu.
  CHECK(search::placeLine(base, QStringLiteral("sketch.create_3d"), captions) ==
        QStringLiteral("No toolbar place"));

  // The user's layer: a moved command at its new place only.
  LayoutUserLayer moved;
  moved.move(QStringLiteral("SOLID/CREATE"), QStringLiteral("SOLID/MODIFY"), sweep);
  const CommandLayout afterMove = moved.applied(base);
  CHECK(search::placeLine(afterMove, sweep, captions) == QStringLiteral("SOLID › MODIFY"));

  // A command in several groups: its home first, then the others in the
  // layout's order, then the menus.
  LayoutUserLayer also;
  also.add(QStringLiteral("BUILD/OUTPUT"), parameters);
  const CommandLayout twice = also.applied(base);
  CHECK(search::placeTexts(twice, parameters, captions) ==
        (QStringList{QStringLiteral("BUILD › OUTPUT"), QStringLiteral("MANAGE › PARAMETERS"),
                     QStringLiteral("Menu bar › Edit")}));
  // Its home moved away: the group it is left in is its home now.
  also.move(QStringLiteral("MANAGE/PARAMETERS"), QStringLiteral("MAKE_PRINT/ANNOTATE"), parameters);
  const CommandLayout away = also.applied(base);
  CHECK(away.home(parameters).tab == QStringLiteral("BUILD"));
  CHECK(search::placeTexts(away, parameters, captions).front() == QStringLiteral("BUILD › OUTPUT"));
  CHECK(!search::placeLine(away, parameters, captions).contains(QStringLiteral("MANAGE")));

  // The line is made from the layout given: no layout file is read.
  CHECK(CommandLayoutStore::loadCount() == loads);
}

int commandSearchPlacesTests() {
  keywordTests();
  placeTests();
  return failures;
}

} // namespace

MITCAD_UNIT_TEST(commandSearchPlacesTests);
