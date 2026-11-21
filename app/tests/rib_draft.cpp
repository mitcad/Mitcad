// SPDX-License-Identifier: MIT
// The Rib's and Web's Draft Angle (mitcad#2102): the panels' input right
// after Thickness, an angle at 0 deg with the tooltip "0: no draft", and
// the definition's `draft_angle` written only when it is not 0 (an
// expression not yet evaluated as it is), so that a rib or web without a
// draft is saved as before it.
#include "UnitTests.hpp"
#include "commands/RibDraft.hpp"
#include "framework/Command.hpp"

#include <cstdio>
#include <limits>

#include <QJsonObject>
#include <QString>
#include <QVector>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "rib_draft.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using mitcad::CommandState;
using mitcad::InputDef;
using mitcad::ValueKind;
using namespace mitcad::cmd;

const QString kDraft = QStringLiteral("draft_angle");

// The ids of the Rib and Web panels' first inputs, as SweepCommands.cpp
// lists them before it inserts the Draft Angle.
QVector<InputDef> panelInputs() {
  QVector<InputDef> inputs;
  for (const char* id : {"curves", "chain", "location", "thickness", "depth_type", "depth", "flip"}) {
    InputDef input;
    input.id = QString::fromLatin1(id);
    inputs.append(input);
  }
  return inputs;
}

void inputTests() {
  QVector<InputDef> inputs = panelInputs();
  insertRibDraft(inputs);
  CHECK(inputs.size() == 8);
  CHECK(inputs.at(3).id == QStringLiteral("thickness"));
  CHECK(inputs.at(4).id == kDraft);
  const InputDef& draft = inputs.at(4);
  CHECK(draft.label == QStringLiteral("Draft Angle"));
  CHECK(draft.type == InputDef::Type::Value);
  CHECK(draft.valueKind == ValueKind::Angle);
  CHECK(draft.defaultText == QStringLiteral("0 deg"));
  CHECK(draft.tooltip == QStringLiteral("0: no draft"));
  CHECK(inputs.at(5).id == QStringLiteral("depth_type"));
  // Without a Thickness it goes last.
  QVector<InputDef> other{InputDef{}};
  insertRibDraft(other);
  CHECK(other.size() == 2 && other.last().id == kDraft);
}

void writeTests() {
  const double degree = 3.14159265358979323846 / 180.0;
  // 0 (the default): nothing written, also where an edited definition had
  // an angle.
  CommandState state;
  state.setText(kDraft, QStringLiteral("0 deg"));
  state.setValue(kDraft, 0.0);
  QJsonObject def{{QStringLiteral("type"), QStringLiteral("rib")}, {kDraft, QStringLiteral("2 deg")}};
  writeRibDraft(state, def);
  CHECK(!def.contains(kDraft));
  CHECK(def.size() == 1);
  // An angle: its expression.
  state.setText(kDraft, QStringLiteral("1.5 deg"));
  state.setValue(kDraft, 1.5 * degree);
  writeRibDraft(state, def);
  CHECK(def.value(kDraft).toString() == QStringLiteral("1.5 deg"));
  // A negative one too.
  state.setText(kDraft, QStringLiteral("-2 deg"));
  state.setValue(kDraft, -2.0 * degree);
  writeRibDraft(state, def);
  CHECK(def.value(kDraft).toString() == QStringLiteral("-2 deg"));
  // An expression not yet evaluated goes as it is, for the model to read.
  state.setText(kDraft, QStringLiteral("draft"));
  state.setValue(kDraft, std::numeric_limits<double>::quiet_NaN());
  writeRibDraft(state, def);
  CHECK(def.value(kDraft).toString() == QStringLiteral("draft"));
  // Empty: nothing.
  state.setText(kDraft, QString());
  writeRibDraft(state, def);
  CHECK(!def.contains(kDraft));
}

} // namespace

int ribDraftTests() {
  failures = 0;
  inputTests();
  writeTests();
  return failures;
}
MITCAD_UNIT_TEST(ribDraftTests);
