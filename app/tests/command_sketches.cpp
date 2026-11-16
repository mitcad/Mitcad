// SPDX-License-Identifier: MIT
// The sketches a command shows for its time (mitcad#546): an edited
// feature's sketches, else those of the profiles, curves and points a new
// command starts with; which of them the command alone shows (drawn
// lighter, logged); what each goes back to when it ends.
#include "UnitTests.hpp"
#include "view/CommandSketches.hpp"

#include <cstdio>

#include <QSet>
#include <QString>
#include <QStringList>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "command_sketches.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

SelectionItem item(SelectKind kind, const QString& owner, const QString& name) {
  SelectionItem picked;
  picked.kind = kind;
  picked.owner = owner;
  picked.name = name;
  return picked;
}

void usedByTests() {
  // An edit: the feature's sketches, each once, in its order; the
  // preselection does not count.
  const Selection profiles{item(SelectKind::Profile, QStringLiteral("F7"), QStringLiteral("r{c1}"))};
  CHECK(CommandSketches::usedBy({QStringLiteral("F3"), QStringLiteral("F1"), QStringLiteral("F3")}, profiles) ==
        (QStringList{QStringLiteral("F3"), QStringLiteral("F1")}));
  // A new command: the sketches of its profiles, curves and points, not of
  // faces, bodies or whole sketches.
  const Selection picked{item(SelectKind::Face, QStringLiteral("F2.b0"), QStringLiteral("F2:end(r{c1})")),
                         item(SelectKind::Profile, QStringLiteral("F1"), QStringLiteral("r{c1}")),
                         item(SelectKind::SketchCurve, QStringLiteral("F4"), QStringLiteral("c2")),
                         item(SelectKind::Profile, QStringLiteral("F1"), QStringLiteral("r{c5}")),
                         item(SelectKind::SketchPoint, QStringLiteral("F5"), QStringLiteral("p3")),
                         item(SelectKind::Sketch, QStringLiteral("F6"), QString()),
                         item(SelectKind::Body, QStringLiteral("F2.b0"), QString())};
  CHECK(CommandSketches::usedBy({}, picked) ==
        (QStringList{QStringLiteral("F1"), QStringLiteral("F4"), QStringLiteral("F5")}));
  // A feature without sketches (a fillet's edit), nothing selected.
  CHECK(CommandSketches::usedBy({}, {}).isEmpty());
}

void editTests() {
  // Sketch1 (F1) hidden as a used sketch is, Sketch3 (F3) shown by its
  // light bulb; the edit uses both.
  QSet<QString> shownByModel{QStringLiteral("F3")};
  const CommandSketches::Shown shown = [&shownByModel](const QString& sketch) {
    return shownByModel.contains(sketch);
  };
  CommandSketches sketches;
  CHECK(!sketches.shows(QStringLiteral("F1"), false));
  const QStringList temporary =
      sketches.begin({QStringLiteral("F1"), QStringLiteral("F3")}, shown, QStringLiteral("the edit of Extrude1"));
  CHECK(temporary == QStringList{QStringLiteral("F1")});
  CHECK(sketches.label() == QStringLiteral("the edit of Extrude1"));
  // Both shown while the panel is open; only Sketch1 lighter.
  CHECK(sketches.shows(QStringLiteral("F1"), false));
  CHECK(sketches.shows(QStringLiteral("F3"), true));
  CHECK(sketches.temporary(QStringLiteral("F1"), false));
  CHECK(!sketches.temporary(QStringLiteral("F3"), true));
  // Another sketch keeps its light bulb.
  CHECK(!sketches.shows(QStringLiteral("F5"), false));
  CHECK(sketches.shows(QStringLiteral("F5"), true));
  // The user turns Sketch1's light bulb on meanwhile: its normal look.
  shownByModel.insert(QStringLiteral("F1"));
  CHECK(!sketches.temporary(QStringLiteral("F1"), true));
  // OK or Cancel: Sketch1 stays shown by its light bulb; nothing is shown
  // for a command any more.
  const std::vector<CommandSketches::Ended> ended = sketches.end(shown);
  CHECK(ended.size() == 1);
  CHECK(!ended.empty() && ended.front().sketch == QStringLiteral("F1") && ended.front().staysShown);
  CHECK(sketches.used().isEmpty() && sketches.temporaries().isEmpty() && sketches.label().isEmpty());
  CHECK(!sketches.shows(QStringLiteral("F3"), false));
}

void cancelTests() {
  // The light bulb left off: hidden again after the panel closes.
  const CommandSketches::Shown hidden = [](const QString&) { return false; };
  CommandSketches sketches;
  sketches.begin({QStringLiteral("F1")}, hidden);
  CHECK(sketches.shows(QStringLiteral("F1"), false));
  const std::vector<CommandSketches::Ended> ended = sketches.end(hidden);
  CHECK(ended.size() == 1);
  CHECK(!ended.empty() && ended.front().sketch == QStringLiteral("F1") && !ended.front().staysShown);
  CHECK(!sketches.shows(QStringLiteral("F1"), false));
  // A second end, or one without a begin: nothing.
  CHECK(sketches.end(hidden).empty());
  // A command that uses none.
  CHECK(sketches.begin({}, hidden).isEmpty());
  CHECK(!sketches.shows(QStringLiteral("F1"), false));
  CHECK(sketches.end(hidden).empty());
}

} // namespace

int commandSketchesTests() {
  failures = 0;
  usedByTests();
  editTests();
  cancelTests();
  return failures;
}
MITCAD_UNIT_TEST(commandSketchesTests);
