// SPDX-License-Identifier: MIT
// Change Parameters while a sketch or a feature is edited (mitcad#564):
// what is open (the log's words, the table's line), the edited feature's
// own parameters left to its panel, which commands of the table the panel
// follows, and a rename carried into the panel's expressions.
#include "UnitTests.hpp"
#include "framework/ParameterEdits.hpp"

#include <cstdio>

#include <QJsonObject>
#include <QJsonValue>
#include <QString>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "parameter_edits.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

ParameterEditScope sketchScope() {
  ParameterEditScope scope;
  scope.sketch = QStringLiteral("Sketch1");
  return scope;
}

ParameterEditScope newFeatureScope() {
  ParameterEditScope scope;
  scope.command = QStringLiteral("Extrude");
  return scope;
}

ParameterEditScope editScope() {
  ParameterEditScope scope;
  scope.command = QStringLiteral("Extrude");
  scope.editedFeature = QStringLiteral("F2");
  scope.editedName = QStringLiteral("Extrude1");
  return scope;
}

QJsonObject parameter(const char* name, const char* kind, const char* owner) {
  QJsonObject entry{{QStringLiteral("name"), QString::fromLatin1(name)},
                    {QStringLiteral("kind"), QString::fromLatin1(kind)}};
  if (owner != nullptr) {
    entry.insert(QStringLiteral("owner"), QString::fromLatin1(owner));
  }
  return entry;
}

QJsonObject setParameter(const char* name, const char* key, const QJsonValue& value) {
  return {{QStringLiteral("cmd"), QStringLiteral("set_parameter")},
          {QStringLiteral("name"), QString::fromLatin1(name)},
          {QString::fromLatin1(key), value}};
}

void scopeTests() {
  const ParameterEditScope none;
  CHECK(none.isEmpty());
  CHECK(none.describe() == QStringLiteral("no edit"));
  CHECK(none.note().isEmpty());
  CHECK(sketchScope().describe() == QStringLiteral("sketch Sketch1"));
  CHECK(newFeatureScope().describe() == QStringLiteral("command Extrude"));
  CHECK(editScope().describe() == QStringLiteral("command Extrude editing Extrude1"));
  ParameterEditScope inSketch = newFeatureScope();
  inSketch.command = QStringLiteral("Sketch Fillet");
  inSketch.sketch = QStringLiteral("Sketch1");
  CHECK(inSketch.describe() == QStringLiteral("command Sketch Fillet in sketch Sketch1"));
  // Without the feature's name (not in the snapshot): its uid.
  ParameterEditScope unnamed = editScope();
  unnamed.editedName.clear();
  CHECK(unnamed.describe() == QStringLiteral("command Extrude editing F2"));

  // The line above the rows says what stays open and how to go back.
  CHECK(sketchScope().note().startsWith(QStringLiteral("Sketch1 stays open.")));
  CHECK(sketchScope().note().contains(QStringLiteral("OK returns to the sketch.")));
  CHECK(newFeatureScope().note().startsWith(QStringLiteral("Extrude stays open.")));
  CHECK(newFeatureScope().note().contains(QStringLiteral("OK returns to the panel.")));
  CHECK(!newFeatureScope().note().contains(QStringLiteral("in its panel")));
  CHECK(editScope().note().startsWith(QStringLiteral("The edit of Extrude1 stays open.")));
  CHECK(editScope().note().contains(QStringLiteral("Change the names and expressions of Extrude1 in its panel.")));

  CHECK(editScope() == editScope());
  CHECK(editScope() != newFeatureScope());
  CHECK(sketchScope() != ParameterEditScope());
}

void holdTests() {
  // The edited feature's own parameters, not a user parameter or
  // another feature's.
  CHECK(editScope().holds(parameter("d3", "model", "F2")));
  CHECK(editScope().holds(parameter("d7", "reference", "F2")));
  CHECK(!editScope().holds(parameter("d1", "model", "F1")));
  CHECK(!editScope().holds(parameter("depth", "user", nullptr)));
  CHECK(!editScope().holds(parameter("depth", "user", "F2")));
  // A new feature has none yet; sketch mode holds nothing.
  CHECK(!newFeatureScope().holds(parameter("d3", "model", "F2")));
  CHECK(!sketchScope().holds(parameter("d1", "model", "F1")));
  CHECK(editScope().holdReason(QStringLiteral("d3")) ==
        QStringLiteral("d3 is a value of Extrude1, whose panel is open: change it there."));

  // What the window refuses of the table's commands on such a parameter.
  QString reason;
  CHECK(editScope().refuses(setParameter("d3", "expression", QStringLiteral("30 mm")), QStringLiteral("F2"), &reason));
  CHECK(reason == editScope().holdReason(QStringLiteral("d3")));
  CHECK(editScope().refuses(setParameter("d3", "unit", QStringLiteral("cm")), QStringLiteral("F2")));
  CHECK(editScope().refuses(setParameter("d3", "value", 30.0), QStringLiteral("F2")));
  CHECK(editScope().refuses({{QStringLiteral("cmd"), QStringLiteral("rename_parameter")},
                             {QStringLiteral("name"), QStringLiteral("d3")},
                             {QStringLiteral("new_name"), QStringLiteral("height")}},
                            QStringLiteral("F2")));
  CHECK(editScope().refuses({{QStringLiteral("cmd"), QStringLiteral("delete_parameter")},
                             {QStringLiteral("name"), QStringLiteral("d3")}},
                            QStringLiteral("F2")));
  // A comment and the star stay the table's.
  reason.clear();
  CHECK(!editScope().refuses(setParameter("d3", "comment", QStringLiteral("Height")), QStringLiteral("F2"), &reason));
  CHECK(reason.isEmpty());
  CHECK(!editScope().refuses(setParameter("d3", "favorite", true), QStringLiteral("F2")));
  // Another feature's, a user parameter, no edit: nothing refused.
  CHECK(!editScope().refuses(setParameter("d1", "expression", QStringLiteral("50 mm")), QStringLiteral("F1")));
  CHECK(!editScope().refuses(setParameter("depth", "expression", QStringLiteral("30 mm")), QString()));
  CHECK(!newFeatureScope().refuses(setParameter("d3", "expression", QStringLiteral("30 mm")), QStringLiteral("F2")));
  CHECK(!sketchScope().refuses(setParameter("d1", "expression", QStringLiteral("50 mm")), QStringLiteral("F1")));
}

void changeTests() {
  CHECK(parameterCommandChangesValues({{QStringLiteral("cmd"), QStringLiteral("add_parameter")},
                                       {QStringLiteral("name"), QStringLiteral("depth")},
                                       {QStringLiteral("expression"), QStringLiteral("20 mm")}}));
  CHECK(parameterCommandChangesValues(
      {{QStringLiteral("cmd"), QStringLiteral("delete_parameter")}, {QStringLiteral("name"), QStringLiteral("w")}}));
  CHECK(parameterCommandChangesValues({{QStringLiteral("cmd"), QStringLiteral("rename_parameter")},
                                       {QStringLiteral("name"), QStringLiteral("w")},
                                       {QStringLiteral("new_name"), QStringLiteral("width")}}));
  CHECK(parameterCommandChangesValues(setParameter("depth", "expression", QStringLiteral("30 mm"))));
  CHECK(parameterCommandChangesValues(setParameter("depth", "unit", QStringLiteral("cm"))));
  CHECK(parameterCommandChangesValues(setParameter("depth", "value", 30.0)));
  CHECK(!parameterCommandChangesValues(setParameter("depth", "comment", QStringLiteral("Depth"))));
  CHECK(!parameterCommandChangesValues(setParameter("depth", "favorite", true)));
  // Limits (mitcad#665).
  CHECK(parameterCommandChangesValues(setParameter("depth", "max", QStringLiteral("30 mm"))));
  CHECK(parameterCommandChangesValues(setParameter("depth", "strict", true)));
  CHECK(!parameterCommandChangesValues({{QStringLiteral("cmd"), QStringLiteral("set_feature_visible")}}));
}

void renameTests() {
  const QString from = QStringLiteral("depth");
  const QString to = QStringLiteral("dp");
  CHECK(renameParameterIn(QStringLiteral("depth"), from, to) == QStringLiteral("dp"));
  // Whole names only: not a longer name that starts or ends with it.
  CHECK(renameParameterIn(QStringLiteral("depth * 2 + depth_2 - mydepth"), from, to) ==
        QStringLiteral("dp * 2 + depth_2 - mydepth"));
  CHECK(renameParameterIn(QStringLiteral("max(depth; 3 mm)+depth"), from, to) == QStringLiteral("max(dp; 3 mm)+dp"));
  // `name = value` (mitcad#231): both sides.
  CHECK(renameParameterIn(QStringLiteral("height = depth + 5 mm"), from, to) == QStringLiteral("height = dp + 5 mm"));
  CHECK(renameParameterIn(QStringLiteral("d10 + d1*2"), QStringLiteral("d1"), QStringLiteral("w")) ==
        QStringLiteral("d10 + w*2"));
  // A number with its unit is no name.
  CHECK(renameParameterIn(QStringLiteral("5mm + mm"), QStringLiteral("mm"), QStringLiteral("x")) ==
        QStringLiteral("5mm + x"));
  CHECK(renameParameterIn(QStringLiteral("20 mm"), from, to) == QStringLiteral("20 mm"));
  CHECK(renameParameterIn(QString(), from, to).isEmpty());
  CHECK(renameParameterIn(QStringLiteral("depth"), QString(), to) == QStringLiteral("depth"));
  CHECK(renameParameterIn(QStringLiteral("depth"), from, from) == QStringLiteral("depth"));
}

} // namespace

int parameterEditsTests() {
  scopeTests();
  holdTests();
  changeTests();
  renameTests();
  return failures;
}
MITCAD_UNIT_TEST(parameterEditsTests);
