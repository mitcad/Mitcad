// SPDX-License-Identifier: MIT
// ui_command's listing and its refusal of an unknown id (mitcad#1269): the
// commands with their names and kinds, whether an agent may run each and
// whether it can run now, a search by words, and the nearest ids to an id
// the registry does not have, in a text that names no repository file.
#include "mcp/UiCommandList.hpp"

#include <cstdio>

#include <QJsonArray>
#include <QJsonObject>
#include <QString>
#include <QStringList>

#include "framework/CommandRegistry.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "ui_command_list.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

mitcad::CommandDef command(const char* id, const char* name, mitcad::CommandDef::Kind kind) {
  mitcad::CommandDef def;
  def.id = QString::fromLatin1(id);
  def.name = QString::fromLatin1(name);
  def.kind = kind;
  def.mode = mitcad::CommandDef::Mode::Any;
  return def;
}

// The listing's entry of `id`, or an empty object.
QJsonObject entryOf(const QJsonObject& listing, const QString& id) {
  for (const auto& value : listing.value(QStringLiteral("commands")).toArray()) {
    if (value.toObject().value(QStringLiteral("id")).toString() == id) {
      return value.toObject();
    }
  }
  return {};
}

QStringList idsOf(const QJsonObject& listing) {
  QStringList ids;
  for (const auto& value : listing.value(QStringLiteral("commands")).toArray()) {
    ids.append(value.toObject().value(QStringLiteral("id")).toString());
  }
  return ids;
}

} // namespace

int uiCommandListTests() {
  using Kind = mitcad::CommandDef::Kind;
  mitcad::CommandRegistry registry;
  registry.add(command("file.new_design", "New Design", Kind::Action));
  registry.add(command("file.open", "Open", Kind::Action));
  registry.add(command("window.new", "New Window", Kind::Action));
  registry.add(command("solid.extrude", "Extrude", Kind::Feature));
  mitcad::CommandDef fillet = command("solid.fillet", "Fillet", Kind::Feature);
  fillet.keywords = QStringList{QStringLiteral("round"), QStringLiteral("blend")};
  registry.add(fillet);
  registry.add(command("sketch.polygon", "Polygon", Kind::Tool));
  registry.add(command("sketch.finish", "Finish Sketch", Kind::Action));
  registry.add(command("view.fit", "Fit", Kind::Action));

  // Commands that open dialogs stay refused; window.new and the model's
  // commands run.
  CHECK(mitcad::mcp::opensDialog(QStringLiteral("file.open")));
  CHECK(mitcad::mcp::opensDialog(QStringLiteral("sketch.import_dxf")));
  CHECK(!mitcad::mcp::opensDialog(QStringLiteral("window.new")));
  CHECK(!mitcad::mcp::opensDialog(QStringLiteral("solid.extrude")));

  // Every command, in the registry's order, with whether an agent may run it
  // and whether it can now.
  const auto onlySketch = [](const mitcad::CommandDef& def) { return def.id.startsWith(QStringLiteral("sketch.")); };
  const QJsonObject all = mitcad::mcp::commandListing(registry, QString(), onlySketch);
  CHECK(all.value(QStringLiteral("total")).toInt() == 8);
  CHECK(all.value(QStringLiteral("count")).toInt() == 8);
  CHECK(idsOf(all).first() == QStringLiteral("file.new_design"));
  CHECK(idsOf(all).last() == QStringLiteral("view.fit"));
  const QJsonObject open = entryOf(all, QStringLiteral("file.open"));
  CHECK(open.value(QStringLiteral("name")).toString() == QStringLiteral("Open"));
  CHECK(open.value(QStringLiteral("agents")).toBool(true) == false);
  CHECK(open.value(QStringLiteral("why")).toString().contains(QStringLiteral("dialog")));
  const QJsonObject extrude = entryOf(all, QStringLiteral("solid.extrude"));
  CHECK(extrude.value(QStringLiteral("kind")).toString() == QStringLiteral("feature"));
  CHECK(extrude.value(QStringLiteral("agents")).toBool(false));
  CHECK(!extrude.contains(QStringLiteral("why")));
  CHECK(extrude.value(QStringLiteral("available")).toBool(true) == false);
  const QJsonObject polygon = entryOf(all, QStringLiteral("sketch.polygon"));
  CHECK(polygon.value(QStringLiteral("kind")).toString() == QStringLiteral("tool"));
  CHECK(polygon.value(QStringLiteral("available")).toBool(false));
  CHECK(entryOf(all, QStringLiteral("window.new")).value(QStringLiteral("agents")).toBool(false));

  // A search by words in the id, the name or the keywords, any case, all of
  // them.
  const QJsonObject news = mitcad::mcp::commandListing(registry, QStringLiteral("NEW"), {});
  CHECK(idsOf(news) == (QStringList{QStringLiteral("file.new_design"), QStringLiteral("window.new")}));
  CHECK(news.value(QStringLiteral("count")).toInt() == 2);
  CHECK(news.value(QStringLiteral("total")).toInt() == 8);
  CHECK(entryOf(news, QStringLiteral("window.new")).value(QStringLiteral("available")).toBool(false));
  CHECK(idsOf(mitcad::mcp::commandListing(registry, QStringLiteral("blend"), {})) ==
        QStringList{QStringLiteral("solid.fillet")});
  CHECK(idsOf(mitcad::mcp::commandListing(registry, QStringLiteral("new window"), {})) ==
        QStringList{QStringLiteral("window.new")});
  CHECK(idsOf(mitcad::mcp::commandListing(registry, QStringLiteral("chamfer"), {})).isEmpty());

  // The nearest ids of an unknown one: the most shared words first, then
  // the fewest edits.
  const QStringList fileNew = mitcad::mcp::nearestCommandIds(registry, QStringLiteral("file.new"), 3);
  CHECK(fileNew.size() == 3);
  CHECK(fileNew.value(0) == QStringLiteral("file.new_design"));
  CHECK(fileNew.contains(QStringLiteral("window.new")));
  const QStringList documentNew = mitcad::mcp::nearestCommandIds(registry, QStringLiteral("document.new"), 2);
  CHECK(documentNew.contains(QStringLiteral("window.new")));
  CHECK(documentNew.contains(QStringLiteral("file.new_design")));
  CHECK(mitcad::mcp::nearestCommandIds(registry, QStringLiteral("solid.extrud")).value(0) ==
        QStringLiteral("solid.extrude"));
  // A command's name finds its id.
  CHECK(mitcad::mcp::nearestCommandIds(registry, QStringLiteral("Finish Sketch")).value(0) ==
        QStringLiteral("sketch.finish"));
  CHECK(mitcad::mcp::nearestCommandIds(registry, QStringLiteral("x"), 20).size() == 8);

  // The refusal's text: the nearest ids with their names and how to list
  // them, no repository file.
  const QString text = mitcad::mcp::nearestCommandsText(registry, QStringLiteral("file.new"));
  CHECK(text.startsWith(QStringLiteral("Nearest ids: file.new_design (New Design)")));
  CHECK(text.contains(QStringLiteral("window.new (New Window)")));
  CHECK(text.contains(QStringLiteral("ui_command with search")));
  CHECK(!text.contains(QStringLiteral(".md")));
  CHECK(!text.contains(QStringLiteral("app/")));
  mitcad::CommandRegistry empty;
  CHECK(mitcad::mcp::nearestCommandsText(empty, QStringLiteral("file.new")).startsWith(
      QStringLiteral("ui_command with search")));
  return failures;
}
