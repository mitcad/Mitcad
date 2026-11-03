// SPDX-License-Identifier: MIT
// The settings Add Print Value… offers for a body (mitcad#1178): the
// process keys, then the printer keys, in the key table's order under the
// Properties window's category › sub-group headings; without a key kept
// per slicer, the filament's, and those the body sets itself; each with
// the value the body prints with now. The search matches a plain name or
// a key and puts the setting named as typed first.
#include <cstdio>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QString>
#include <QStringList>
#include <QVector>

#include "print/PrintKeys.hpp"
#include "print/PrintValueChoices.hpp"

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "print_value_choices.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

QJsonObject key(const char* name, const char* set, const char* plain, const char* kind, const char* category,
                const char* group) {
  return QJsonObject{{QStringLiteral("key"), QString::fromLatin1(name)},
                     {QStringLiteral("set"), QString::fromLatin1(set)},
                     {QStringLiteral("name"), QString::fromLatin1(plain)},
                     {QStringLiteral("kind"), QString::fromLatin1(kind)},
                     {QStringLiteral("category"), QString::fromLatin1(category)},
                     {QStringLiteral("group"), QString::fromLatin1(group)}};
}

// A value of print_settings: from a profile, or a property of a level.
QJsonObject value(const char* name, const QJsonValue& raw, const char* text, const char* from, const char* level) {
  return QJsonObject{{QStringLiteral("key"), QString::fromLatin1(name)},
                     {QStringLiteral("value"), raw},
                     {QStringLiteral("text"), QString::fromLatin1(text)},
                     {QStringLiteral("from"), QString::fromLatin1(from)},
                     {QStringLiteral("level"), QString::fromLatin1(level)}};
}

QStringList keysOf(const QVector<mitcad::PrintValueChoice>& choices) {
  QStringList keys;
  for (const mitcad::PrintValueChoice& choice : choices) {
    keys << choice.key;
  }
  return keys;
}

} // namespace

int printValueChoicesTests() {
  using namespace mitcad;
  QJsonArray keys;
  keys.append(key("printer_settings_id", "printer", "Printer preset", "text", "basic", "basic"));
  keys.append(key("nozzle_diameter", "printer", "Nozzle diameter", "number", "extruder", "nozzle"));
  keys.append(key("layer_height", "process", "Layer height", "number", "quality", "layer_height"));
  keys.append(key("wall_sequence", "process", "Walls printing order", "text", "quality", "order"));
  keys.append(key("perimeters", "process", "Walls", "integer", "strength", "walls"));
  keys.append(key("fill_density", "process", "Sparse infill density", "percent", "strength", "sparse_infill"));
  keys.append(key("filament_density", "filament", "Density", "number", "filament", "basic"));
  QJsonObject preset = keys.at(0).toObject();
  preset.insert(QStringLiteral("per_dialect"), true);
  keys.replace(0, preset);
  const PrintKeyTable table(QJsonObject{{QStringLiteral("keys"), keys}});

  QJsonArray process;
  process.append(value("perimeters", 2, "2", "profile", "component"));
  process.append(value("fill_density", 20, "20 %", "property", "body"));
  QJsonArray printer;
  printer.append(value("nozzle_diameter", 0.4, "0.4 mm", "profile", "component"));
  const QJsonObject settings{
      {QStringLiteral("process"), QJsonObject{{QStringLiteral("values"), process}}},
      {QStringLiteral("printer"), QJsonObject{{QStringLiteral("values"), printer}}},
      {QStringLiteral("filament"), QJsonObject{{QStringLiteral("values"), QJsonArray()}}}};

  const QVector<PrintValueChoice> choices = printValueChoices(table, settings);
  // The process first, then the printer; the body's own Sparse infill
  // density, the per-slicer preset and the filament's density left out.
  CHECK(keysOf(choices) == QStringList({QStringLiteral("layer_height"), QStringLiteral("wall_sequence"),
                                        QStringLiteral("perimeters"), QStringLiteral("nozzle_diameter")}));
  if (choices.size() == 4) {
    const PrintValueChoice& walls = choices.at(2);
    CHECK(walls.set == QStringLiteral("process"));
    CHECK(walls.name == QStringLiteral("Walls"));
    CHECK(walls.heading == QStringLiteral("Strength › Walls"));
    CHECK(walls.text == QStringLiteral("2"));
    CHECK(walls.value == QJsonValue(2));
    // No value to start from: the slicer's default.
    CHECK(choices.at(0).text.isEmpty() && choices.at(0).value.isUndefined());
    CHECK(choices.at(0).heading == QStringLiteral("Quality › Layer height"));
    CHECK(choices.at(3).set == QStringLiteral("printer") && choices.at(3).text == QStringLiteral("0.4 mm"));
  }

  // The search: words of the name or the key, ignoring case; the setting
  // named as typed first.
  CHECK(printValueMatches(choices.value(2), QStringLiteral("walls")));
  CHECK(printValueMatches(choices.value(2), QStringLiteral("perim")));
  CHECK(!printValueMatches(choices.value(2), QStringLiteral("infill")));
  CHECK(printValueMatches(choices.value(1), QStringLiteral("walls order")));
  CHECK(printValueBestMatch(choices, QStringLiteral("Walls")) == 2);
  CHECK(printValueBestMatch(choices, QStringLiteral("wall")) == 1);
  CHECK(printValueBestMatch(choices, QString()) == 0);
  CHECK(printValueBestMatch(choices, QStringLiteral("nothing of the kind")) == -1);

  // The heading as the Properties window writes it.
  CHECK(printGroupHeading(PrintCategory{QStringLiteral("strength"), QStringLiteral("Strength"), {}},
                          PrintGroup{QStringLiteral("strength"), QStringLiteral("Strength")}) ==
        QStringLiteral("Strength"));
  return failures;
}
