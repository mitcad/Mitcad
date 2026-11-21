// SPDX-License-Identifier: MIT
// The in-app tutorials' guide steps (mitcad#2012, tutorials/GuideStep): a
// step read from its JSON and refused with the reason, the commands it
// allows (its own, the ones it names, Leave Tutorial, the camera's unless
// navigation is off), its picks and the values that complete it.
#include "UnitTests.hpp"
#include "tutorials/GuideStep.hpp"

#include <cstdio>

#include <QJsonDocument>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "guide_step.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

QJsonObject json(const char* text) { return QJsonDocument::fromJson(QByteArray(text)).object(); }

GuideStep parsed(const char* text, QString* error = nullptr) {
  QString reason;
  GuideStep step = GuideStep::parse(json(text), &reason);
  if (error != nullptr) {
    *error = reason;
  }
  return step;
}

void commandSteps() {
  QString error;
  const GuideStep step = parsed(R"({"id": "sketch", "text": "Start a sketch.",
                                   "target": {"command": "sketch.create"},
                                   "done": {"started": "sketch.create"}})",
                                &error);
  CHECK(error.isEmpty());
  CHECK(step.id == QStringLiteral("sketch"));
  CHECK(step.target == GuideStep::Target::Command);
  CHECK(step.targetName == QStringLiteral("sketch.create"));
  CHECK(step.done == GuideStep::Done::Started);
  CHECK(step.commands() == QStringList{QStringLiteral("sketch.create")});
  CHECK(step.allowsCommand(QStringLiteral("sketch.create")));
  CHECK(!step.allowsCommand(QStringLiteral("solid.fillet")));
  CHECK(step.allowsCommand(QStringLiteral("help.leave_tutorial")));
  CHECK(step.allowsCommand(QStringLiteral("view.home"))); // navigation by default
  CHECK(!step.takesPicks());
  // Back to JSON and read again: the same step.
  const GuideStep again = GuideStep::parse(step.toJson(), &error);
  CHECK(error.isEmpty());
  CHECK(again.toJson() == step.toJson());

  const GuideStep held = parsed(R"({"text": "Extrude it.", "target": {"command": "solid.extrude"},
                                   "done": {"command": "solid.extrude"}, "navigation": false,
                                   "allow": ["edit.undo"]})",
                                &error);
  CHECK(error.isEmpty());
  CHECK(held.id == QStringLiteral("step"));
  CHECK(held.done == GuideStep::Done::Finished);
  CHECK(held.allowsCommand(QStringLiteral("edit.undo")));
  CHECK(!held.allowsCommand(QStringLiteral("view.home")));
  CHECK(held.commands() == QStringList{QStringLiteral("solid.extrude")});
}

void inputAndPickSteps() {
  QString error;
  const GuideStep value = parsed(R"({"text": "Type 20 mm.", "target": {"input": "distance"},
                                    "done": {"input": "distance", "value": "20 mm"}})",
                                 &error);
  CHECK(error.isEmpty());
  CHECK(value.target == GuideStep::Target::Input);
  CHECK(value.done == GuideStep::Done::Value);
  CHECK(value.doneName == QStringLiteral("distance"));
  CHECK(value.commands().isEmpty());
  CHECK(guideValueMatches(QStringLiteral("20"), value.value));
  CHECK(guideValueMatches(QStringLiteral(" 20 MM"), value.value));
  CHECK(guideValueMatches(QStringLiteral("20mm"), value.value));
  CHECK(!guideValueMatches(QStringLiteral("2"), value.value));
  CHECK(!guideValueMatches(QStringLiteral("200"), value.value));
  CHECK(!guideValueMatches(QString(), value.value));
  CHECK(!guideValueMatches(QStringLiteral("20 in"), value.value));
  CHECK(guideValueMatches(QStringLiteral("20\u00a0mm"), value.value));
  CHECK(guideValueMatches(QStringLiteral("20.0 mm"), value.value));
  CHECK(guideValueMatches(QStringLiteral("20,0"), value.value));

  const GuideStep pick = parsed(R"({"text": "Pick the top face.", "target": {"view": "face"},
                                   "done": {"pick": ["face"]}})",
                                &error);
  CHECK(error.isEmpty());
  CHECK(pick.takesPicks());
  CHECK(pick.pickMatches(QStringLiteral("face")));
  CHECK(!pick.pickMatches(QStringLiteral("edge")));
  const GuideStep any = parsed(R"({"text": "Pick a row.", "target": {"browser": "Sketch1"},
                                  "done": {"pick": true}})",
                               &error);
  CHECK(error.isEmpty());
  CHECK(any.target == GuideStep::Target::Browser);
  CHECK(any.pickMatches(QStringLiteral("sketch")));
}

void modeSteps() {
  QString error;
  const GuideStep sketch = parsed(R"({"id": "sketch", "text": "Click Create Sketch, then a plane.",
                                     "target": {"command": "sketch.create"}, "done": {"mode": "sketch"}})",
                                  &error);
  CHECK(error.isEmpty());
  CHECK(sketch.done == GuideStep::Done::Mode);
  CHECK(sketch.doneName == QStringLiteral("sketch"));
  CHECK(sketch.commands() == QStringList{QStringLiteral("sketch.create")});
  CHECK(GuideStep::parse(sketch.toJson(), &error).toJson() == sketch.toJson());
  CHECK(parsed(R"({"text": "x", "target": {"command": "a"}, "done": {"mode": "drawing"}})", &error).id.isEmpty());
  CHECK(error.contains(QStringLiteral("pick_plane")));
}

void refusals() {
  QString error;
  CHECK(parsed(R"({"target": {"command": "a"}, "done": {"command": "a"}})", &error).id.isEmpty());
  CHECK(error.contains(QStringLiteral("text")));
  CHECK(parsed(R"({"text": "x", "target": {"command": "a", "input": "b"}, "done": {"command": "a"}})", &error)
            .id.isEmpty());
  CHECK(error.contains(QStringLiteral("exactly one")));
  CHECK(parsed(R"({"text": "x", "target": {"command": "a"}, "done": {}})", &error).id.isEmpty());
  CHECK(parsed(R"({"text": "x", "target": {"view": "wall"}, "done": {"pick": true}})", &error).id.isEmpty());
  CHECK(error.contains(QStringLiteral("wall")));
  CHECK(parsed(R"({"text": "x", "target": {"input": "d"}, "done": {"input": "d"}})", &error).id.isEmpty());
  CHECK(parsed(R"({"text": "x", "target": {"command": "a"}, "done": {"command": "a"}, "at": [10, 20]})", &error)
            .id.isEmpty());
  CHECK(error.contains(QStringLiteral("\"at\"")));
  CHECK(parsed(R"({"text": "x", "target": {"command": "a"}, "done": {"command": "a"}, "navigation": "no"})", &error)
            .id.isEmpty());
  CHECK(parsed(R"({"text": "x", "target": {"command": "a"}, "done": {"command": "a"}, "allow": "b"})", &error)
            .id.isEmpty());
}

} // namespace

int guideStepTests() {
  failures = 0;
  commandSteps();
  inputAndPickSteps();
  modeSteps();
  refusals();
  return failures;
}
MITCAD_UNIT_TEST(guideStepTests);
