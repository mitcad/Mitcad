// SPDX-License-Identifier: MIT
// The Sketch Palette's Create section (mitcad#1027): without counts the
// defaults (those of them that exist), with counts the most started tools,
// the commands pinned on the Sketch tab first, always in the ribbon's order
// and at most 14; each start of a tool is counted in the settings
// (sketchToolCounts/<id>).
#include "sketch/PaletteCreateTools.hpp"

#include <cstdio>

#include <QCoreApplication>
#include <QHash>
#include <QSettings>
#include <QString>
#include <QStringList>
#include <QTemporaryDir>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "palette_create_tools.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::sketch::choosePaletteCreateTools;
using mitcad::sketch::kPaletteCreateCount;
using mitcad::sketch::PaletteCreateChoice;

QStringList ids(const QStringList& names) {
  QStringList result;
  for (const QString& name : names) {
    result.append(QStringLiteral("sketch.") + name);
  }
  return result;
}

// The Sketch tab's Create and Modify tools in the ribbon's order (Create:
// the Lines submenu, Point (mitcad#1545), the Rectangles, Circles, Arcs,
// Other Geometry, Text, Patterns and References submenus, then Dimension;
// Modify: Fillet, Chamfer, Trim, ...).
const QStringList& ribbon() {
  static const QStringList order = ids(
      {QStringLiteral("line"), QStringLiteral("point"), QStringLiteral("rectangle"),
       QStringLiteral("rectangle_3pt"), QStringLiteral("rectangle_center"), QStringLiteral("circle"),
       QStringLiteral("circle_2pt"), QStringLiteral("circle_3pt"), QStringLiteral("arc"),
       QStringLiteral("arc_center"), QStringLiteral("arc_tangent"), QStringLiteral("ellipse"),
       QStringLiteral("polygon"), QStringLiteral("polygon_inscribed"), QStringLiteral("slot"),
       QStringLiteral("slot_overall"), QStringLiteral("spline"), QStringLiteral("spline_control"),
       QStringLiteral("text"), QStringLiteral("rectangular_pattern"), QStringLiteral("circular_pattern"),
       QStringLiteral("mirror"), QStringLiteral("project"), QStringLiteral("intersect"),
       QStringLiteral("dimension"), QStringLiteral("fillet"), QStringLiteral("chamfer"),
       QStringLiteral("trim"), QStringLiteral("extend"), QStringLiteral("offset"), QStringLiteral("move")});
  return order;
}

// The defaults, in the ribbon's order.
const QStringList& defaultsInRibbonOrder() {
  static const QStringList order =
      ids({QStringLiteral("line"), QStringLiteral("point"), QStringLiteral("rectangle"), QStringLiteral("circle"),
           QStringLiteral("arc"), QStringLiteral("polygon"), QStringLiteral("slot"), QStringLiteral("spline"),
           QStringLiteral("mirror"), QStringLiteral("dimension"), QStringLiteral("fillet"), QStringLiteral("trim"),
           QStringLiteral("extend"), QStringLiteral("offset")});
  return order;
}

// Whether `chosen` is in the order `ribbon()` has them.
bool inRibbonOrder(const QStringList& chosen) {
  for (qsizetype i = 1; i < chosen.size(); ++i) {
    if (ribbon().indexOf(chosen[i - 1]) >= ribbon().indexOf(chosen[i])) {
      return false;
    }
  }
  return true;
}

void defaultsTests() {
  // Without counts: the 14 defaults, in the ribbon's order.
  const PaletteCreateChoice choice = choosePaletteCreateTools(ribbon(), {}, {});
  CHECK(choice.ids == defaultsInRibbonOrder());
  CHECK(!choice.fromCounts);
  CHECK(mitcad::sketch::paletteCreateDefaults().size() == kPaletteCreateCount);
  // Only those of them that exist: no Mirror, no Extend.
  QStringList fewer = ribbon();
  fewer.removeAll(QStringLiteral("sketch.mirror"));
  fewer.removeAll(QStringLiteral("sketch.extend"));
  const PaletteCreateChoice some = choosePaletteCreateTools(fewer, {}, {});
  CHECK(some.ids.size() == 12);
  CHECK(!some.ids.contains(QStringLiteral("sketch.mirror")));
  CHECK(!some.ids.contains(QStringLiteral("sketch.extend")));
  // A count of a command that is not a candidate (a constraint) or of
  // nothing changes nothing.
  QHash<QString, int> other;
  other.insert(QStringLiteral("sketch.constraint.coincident"), 9);
  other.insert(QStringLiteral("sketch.line"), 0);
  const PaletteCreateChoice same = choosePaletteCreateTools(ribbon(), {}, other);
  CHECK(same.ids == defaultsInRibbonOrder());
  CHECK(!same.fromCounts);
}

void countsTests() {
  // Every tool counted: the 14 most started, in the ribbon's order, not by
  // count; a tie goes to the one earlier in the ribbon.
  QHash<QString, int> counts;
  for (qsizetype i = 0; i < ribbon().size(); ++i) {
    counts.insert(ribbon()[i], 100 + static_cast<int>(i)); // later ones more
  }
  counts.insert(QStringLiteral("sketch.line"), 1000);   // the most
  counts.insert(QStringLiteral("sketch.ellipse"), 118); // ties with Text (100 + 18) for the last place
  const PaletteCreateChoice choice = choosePaletteCreateTools(ribbon(), {}, counts);
  CHECK(choice.fromCounts);
  CHECK(choice.ids.size() == kPaletteCreateCount);
  CHECK(inRibbonOrder(choice.ids));
  CHECK(choice.ids.first() == QStringLiteral("sketch.line"));
  CHECK(choice.ids.contains(QStringLiteral("sketch.move")));       // 130
  CHECK(choice.ids.contains(QStringLiteral("sketch.ellipse")));    // 118, before Text in the ribbon
  CHECK(!choice.ids.contains(QStringLiteral("sketch.text")));      // 118
  CHECK(!choice.ids.contains(QStringLiteral("sketch.rectangle"))); // 102
  // The 14: Line, Ellipse and the twelve last of the ribbon.
  CHECK(choice.ids.mid(2) == ribbon().mid(ribbon().size() - 12));

  // A few counted tools that are not defaults come in; the defaults fill
  // the rest, the last of them left out.
  QHash<QString, int> few;
  few.insert(QStringLiteral("sketch.ellipse"), 3);
  few.insert(QStringLiteral("sketch.rectangle_center"), 2);
  few.insert(QStringLiteral("sketch.line"), 7);
  const PaletteCreateChoice some = choosePaletteCreateTools(ribbon(), {}, few);
  CHECK(some.fromCounts);
  CHECK(some.ids.size() == kPaletteCreateCount);
  CHECK(inRibbonOrder(some.ids));
  CHECK(some.ids.contains(QStringLiteral("sketch.ellipse")));
  CHECK(some.ids.contains(QStringLiteral("sketch.rectangle_center")));
  // Line is counted itself; of the defaults Mirror and Dimension, the last
  // two of the defaults' own order, made room.
  CHECK(!some.ids.contains(QStringLiteral("sketch.mirror")));
  CHECK(!some.ids.contains(QStringLiteral("sketch.dimension")));
  CHECK(some.ids.contains(QStringLiteral("sketch.fillet")));
}

void pinnedTests() {
  // Pinned commands first, whatever the counts; the counts fill the rest,
  // all in the ribbon's order. A pin that is no candidate (a constraint,
  // Finish Sketch) is left out.
  QHash<QString, int> counts;
  for (qsizetype i = 0; i < ribbon().size(); ++i) {
    counts.insert(ribbon()[i], 100 + static_cast<int>(i));
  }
  const QStringList pinned = {QStringLiteral("sketch.text"), QStringLiteral("sketch.constraint.fix"),
                              QStringLiteral("sketch.rectangle_3pt"), QStringLiteral("sketch.finish")};
  const PaletteCreateChoice choice = choosePaletteCreateTools(ribbon(), pinned, counts);
  CHECK(choice.ids.size() == kPaletteCreateCount);
  CHECK(inRibbonOrder(choice.ids));
  CHECK(choice.ids.contains(QStringLiteral("sketch.text")));
  CHECK(choice.ids.contains(QStringLiteral("sketch.rectangle_3pt"))); // 103: only as pinned
  CHECK(!choice.ids.contains(QStringLiteral("sketch.constraint.fix")));
  CHECK(!choice.ids.contains(QStringLiteral("sketch.finish")));
  // The 12 most counted beside the two pins.
  CHECK(choice.ids.mid(2) == ribbon().mid(ribbon().size() - 12));

  // More pins than places: the first 14 pinned.
  const QStringList many = ribbon().mid(0, 20);
  const PaletteCreateChoice full = choosePaletteCreateTools(ribbon(), many, {});
  CHECK(full.ids == many.mid(0, kPaletteCreateCount));
  CHECK(!full.fromCounts);
  // A smaller limit.
  // The first three of the defaults (by priority), in the ribbon's order.
  CHECK(choosePaletteCreateTools(ribbon(), {}, {}, 3).ids ==
        ids({QStringLiteral("line"), QStringLiteral("rectangle"), QStringLiteral("circle")}));
}

void settingsTests() {
  // Each start counted in the settings, read back.
  QSettings().remove(mitcad::sketch::paletteToolCountsGroup());
  CHECK(mitcad::sketch::paletteToolCounts().isEmpty());
  mitcad::sketch::countPaletteTool(QStringLiteral("sketch.ellipse"));
  mitcad::sketch::countPaletteTool(QStringLiteral("sketch.ellipse"));
  mitcad::sketch::countPaletteTool(QStringLiteral("sketch.trim"));
  const QHash<QString, int> counts = mitcad::sketch::paletteToolCounts();
  CHECK(counts.size() == 2);
  CHECK(counts.value(QStringLiteral("sketch.ellipse")) == 2);
  CHECK(counts.value(QStringLiteral("sketch.trim")) == 1);
  CHECK(QSettings().value(QStringLiteral("sketchToolCounts/sketch.ellipse")).toInt() == 2);
  const PaletteCreateChoice choice = choosePaletteCreateTools(ribbon(), {}, counts);
  CHECK(choice.fromCounts);
  CHECK(choice.ids.contains(QStringLiteral("sketch.ellipse")));
  QSettings().remove(mitcad::sketch::paletteToolCountsGroup());
  CHECK(mitcad::sketch::paletteToolName(QStringLiteral("sketch.rectangle_center")) ==
        QStringLiteral("rectangle_center"));
}

} // namespace

int paletteCreateToolsTests() {
  defaultsTests();
  countsTests();
  pinnedTests();
  // The settings in a folder of the test's own, never the user's.
  QTemporaryDir directory;
  CHECK(directory.isValid());
  const QSettings::Format format = QSettings::defaultFormat();
  const QString organization = QCoreApplication::organizationName();
  const QString application = QCoreApplication::applicationName();
  QCoreApplication::setOrganizationName(QStringLiteral("MitcadTest"));
  QCoreApplication::setApplicationName(QStringLiteral("palette_create_tools"));
  QSettings::setDefaultFormat(QSettings::IniFormat);
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, directory.path());
  CHECK(QSettings().fileName().startsWith(directory.path()));
  settingsTests();
  QSettings::setDefaultFormat(format);
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  return failures;
}
