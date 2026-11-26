// SPDX-License-Identifier: MIT
// The in-app tutorials' guide steps (mitcad#2012, tutorials/GuideStep): a
// step read from its JSON and refused with the reason, the commands it
// allows (its own, the ones it names, Leave Tutorial, the camera's unless
// navigation is off), its picks and the values that complete it; the
// toolbar's places and their done conditions (mitcad#2370).
#include "UnitTests.hpp"
#include "tutorials/GuideStep.hpp"

#include <cstdio>

#include <QJsonDocument>
#include <QJsonObject>

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

// The toolbar's places (mitcad#2370): the workspace selector opened, a
// workspace chosen, a tab activated, the timeline without done.
void toolbarSteps() {
  QString error;
  const GuideStep selector = parsed(R"({"id": "selector", "text": "Click the workspace selector.",
                                       "target": {"workspace_selector": true},
                                       "done": {"opened": "workspace_selector"}})",
                                    &error);
  CHECK(error.isEmpty());
  CHECK(selector.target == GuideStep::Target::WorkspaceSelector);
  CHECK(selector.targetName.isEmpty());
  CHECK(selector.done == GuideStep::Done::Opened);
  CHECK(selector.doneName == QLatin1String(kGuideWorkspaceSelector));
  CHECK(selector.commands().isEmpty());
  CHECK(!selector.takesPicks());
  CHECK(GuideStep::parse(selector.toJson(), &error).toJson() == selector.toJson());

  const GuideStep workspace = parsed(R"({"id": "design", "text": "Choose Design.",
                                        "target": {"workspace": " Design "}, "done": {"chosen": "Design"}})",
                                     &error);
  CHECK(error.isEmpty());
  CHECK(workspace.target == GuideStep::Target::Workspace);
  CHECK(workspace.targetName == QStringLiteral("Design")); // MainWindow resolves it to DESIGN
  CHECK(workspace.done == GuideStep::Done::Chosen);
  CHECK(workspace.doneName == QStringLiteral("Design"));
  CHECK(workspace.commands().isEmpty());
  CHECK(GuideStep::parse(workspace.toJson(), &error).toJson() == workspace.toJson());

  const GuideStep tab = parsed(R"({"text": "Open the Surface tab.", "target": {"tab": "Surface"},
                                  "done": {"activated": "Surface"}, "allow": ["view.fit"]})",
                               &error);
  CHECK(error.isEmpty());
  CHECK(tab.target == GuideStep::Target::Tab);
  CHECK(tab.done == GuideStep::Done::Activated);
  CHECK(tab.doneName == QStringLiteral("Surface"));
  CHECK(tab.allowsCommand(QStringLiteral("view.fit")));
  CHECK(!tab.allowsCommand(QStringLiteral("solid.extrude")));
  CHECK(GuideStep::parse(tab.toJson(), &error).toJson() == tab.toJson());

  // A place to point at: no done, ended by the page.
  const GuideStep timeline = parsed(R"({"id": "timeline", "text": "This is the timeline.",
                                       "target": {"timeline": true}})",
                                    &error);
  CHECK(error.isEmpty());
  CHECK(timeline.target == GuideStep::Target::Timeline);
  CHECK(timeline.done == GuideStep::Done::None);
  CHECK(timeline.commands().isEmpty());
  CHECK(!timeline.toJson().contains(QStringLiteral("done")));
  CHECK(timeline.toJson().value(QStringLiteral("target")).toObject().value(QStringLiteral("timeline")).toBool());
  CHECK(GuideStep::parse(timeline.toJson(), &error).toJson() == timeline.toJson());
  // Any target may go without done; a timeline step may have one.
  CHECK(parsed(R"({"text": "x", "target": {"command": "solid.extrude"}})", &error).done == GuideStep::Done::None);
  CHECK(parsed(R"({"text": "x", "target": {"timeline": true}, "done": {"mode": "sketch"}})", &error).done ==
        GuideStep::Done::Mode);

  // Refused: the selector and the timeline are true, a workspace and a tab
  // are names, opened names the selector, done is an object.
  CHECK(parsed(R"({"text": "x", "target": {"workspace_selector": "Design"}})", &error).id.isEmpty());
  CHECK(error.contains(QStringLiteral("workspace_selector is true")));
  CHECK(parsed(R"({"text": "x", "target": {"timeline": false}})", &error).id.isEmpty());
  CHECK(parsed(R"({"text": "x", "target": {"workspace": ""}, "done": {"chosen": "Design"}})", &error).id.isEmpty());
  CHECK(parsed(R"({"text": "x", "target": {"tab": 3}, "done": {"activated": "Solid"}})", &error).id.isEmpty());
  CHECK(parsed(R"({"text": "x", "target": {"workspace_selector": true}, "done": {"opened": "menu"}})", &error)
            .id.isEmpty());
  CHECK(error.contains(QStringLiteral("workspace_selector")));
  CHECK(parsed(R"({"text": "x", "target": {"tab": "Solid"}, "done": {"activated": ""}})", &error).id.isEmpty());
  CHECK(parsed(R"({"text": "x", "target": {"tab": "Solid"}, "done": "Solid"})", &error).id.isEmpty());
  CHECK(error.contains(QStringLiteral("object")));
  CHECK(parsed(R"({"text": "x", "target": {"tab": "Solid", "timeline": true}})", &error).id.isEmpty());
  CHECK(error.contains(QStringLiteral("exactly one")));
}

// A step done by a navigation of the view (mitcad#2369).
void navigateSteps() {
  QString error;
  const GuideStep orbit = parsed(R"({"id": "orbit", "text": "Orbit the view.", "target": {"view": true},
                                    "done": {"navigate": "orbit"}})",
                                 &error);
  CHECK(error.isEmpty());
  CHECK(orbit.done == GuideStep::Done::Navigate);
  CHECK(orbit.doneName == QStringLiteral("orbit"));
  CHECK(orbit.navigation);
  CHECK(orbit.commands().isEmpty());
  CHECK(orbit.toJson().value(QStringLiteral("done")).toObject().value(QStringLiteral("navigate")).toString() ==
        QStringLiteral("orbit"));
  CHECK(GuideStep::parse(orbit.toJson(), &error).toJson() == orbit.toJson());
  for (const char* text : {R"({"text": "x", "target": {"view": true}, "done": {"navigate": "pan"}})",
                           R"({"text": "x", "target": {"command": "a"}, "done": {"navigate": "zoom"}})"}) {
    CHECK(!parsed(text, &error).id.isEmpty());
  }
  CHECK(guideNavigationActions() ==
        (QStringList{QStringLiteral("orbit"), QStringLiteral("pan"), QStringLiteral("zoom")}));
  CHECK(parsed(R"({"text": "x", "target": {"view": true}, "done": {"navigate": "roll"}})", &error).id.isEmpty());
  CHECK(error.contains(QStringLiteral("orbit, pan, zoom")));
  CHECK(parsed(R"({"text": "x", "target": {"view": true}, "done": {"navigate": "orbit"}, "navigation": false})",
               &error)
            .id.isEmpty());
  CHECK(error.contains(QStringLiteral("navigation")));
  CHECK(parsed(R"({"text": "x", "target": {"view": true}, "done": {"navigate": "orbit", "value": "1"}})", &error)
            .id.isEmpty());
  // The arrow keys' turns are the camera's commands, allowed with navigation.
  CHECK(orbit.allowsCommand(QStringLiteral("view.turn_left")));
  CHECK(orbit.allowsCommand(QStringLiteral("view.turn_down")));
  CHECK(!orbit.allowsCommand(QStringLiteral("sketch.create")));
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

// "hold": true (mitcad#2368): read and written back; while held only Leave
// Tutorial and the camera's commands run, not the step's own nor the ones it
// allows.
void holdSteps() {
  QString error;
  const GuideStep held = parsed(R"({"id": "tool", "text": "Pick the rectangle tool.",
                                   "target": {"command": "sketch.rectangle"},
                                   "done": {"command": "sketch.rectangle"}, "allow": ["edit.undo"],
                                   "hold": true})",
                                &error);
  CHECK(error.isEmpty());
  CHECK(held.hold);
  CHECK(held.toJson().value(QStringLiteral("hold")).toBool());
  CHECK(GuideStep::parse(held.toJson(), &error).toJson() == held.toJson());
  CHECK(held.allowsCommand(QStringLiteral("sketch.rectangle")));
  CHECK(!held.allowsCommandWhileHeld(QStringLiteral("sketch.rectangle")));
  CHECK(!held.allowsCommandWhileHeld(QStringLiteral("edit.undo")));
  CHECK(!held.allowsCommandWhileHeld(QStringLiteral("solid.extrude")));
  CHECK(held.allowsCommandWhileHeld(QStringLiteral("help.leave_tutorial")));
  CHECK(held.allowsCommandWhileHeld(QStringLiteral("view.fit")));

  const GuideStep still = parsed(R"({"text": "x", "target": {"command": "a"}, "done": {"command": "a"},
                                    "navigation": false, "hold": true})",
                                 &error);
  CHECK(error.isEmpty());
  CHECK(!still.allowsCommandWhileHeld(QStringLiteral("view.fit")));
  CHECK(still.allowsCommandWhileHeld(QStringLiteral("help.leave_tutorial")));

  // No hold by default, and none written.
  const GuideStep plain = parsed(R"({"text": "x", "target": {"command": "a"}, "done": {"command": "a"}})", &error);
  CHECK(error.isEmpty());
  CHECK(!plain.hold);
  CHECK(!plain.toJson().contains(QStringLiteral("hold")));
  CHECK(parsed(R"({"text": "x", "target": {"command": "a"}, "done": {"command": "a"}, "hold": "yes"})", &error)
            .id.isEmpty());
  CHECK(error.contains(QStringLiteral("hold")));
}

// The learner's sketch input (mitcad#2440): a plane as target and done, a
// drawing tool's point by its snap or its coordinates, a value box's value;
// each read, written back and refused with the reason.
void sketchInputSteps() {
  QString error;
  const GuideStep plane = parsed(R"({"id": "plane", "text": "Pick the XY plane.", "target": {"plane": "XY"},
                                    "done": {"plane": "xy"}, "allow": ["sketch.create"]})",
                                 &error);
  CHECK(error.isEmpty());
  CHECK(plane.target == GuideStep::Target::Plane);
  CHECK(plane.done == GuideStep::Done::Plane);
  CHECK(plane.takesPicks());
  CHECK(plane.plane() == QStringLiteral("XY"));
  CHECK(plane.planeMatches({QStringLiteral("xy")}));
  CHECK(!plane.planeMatches({QStringLiteral("xz")}));
  CHECK(!plane.planeMatches({QStringLiteral("face"), QStringLiteral("F2:top")}));
  CHECK(plane.allowsCommand(QStringLiteral("sketch.create")));
  CHECK(GuideStep::parse(plane.toJson(), &error).toJson() == plane.toJson());
  CHECK(plane.toJson().value(QStringLiteral("done")).toObject().value(QStringLiteral("plane")).toString() ==
        QStringLiteral("xy"));
  // A construction plane by its name or uid, any face, a face by its
  // reference; no plane named: any plane.
  const GuideStep named = parsed(R"({"text": "x", "target": {"command": "sketch.create"},
                                    "done": {"plane": "Plane1"}})",
                                 &error);
  CHECK(error.isEmpty());
  CHECK(named.target == GuideStep::Target::Command);
  CHECK(!named.takesPicks());
  CHECK(named.planeMatches({QStringLiteral("F4"), QStringLiteral("plane1")}));
  CHECK(!named.planeMatches({QStringLiteral("F5"), QStringLiteral("Plane2")}));
  const GuideStep face = parsed(R"({"text": "x", "target": {"plane": "face"}, "done": {"mode": "sketch"}})", &error);
  CHECK(error.isEmpty());
  CHECK(face.plane() == QStringLiteral("face"));
  CHECK(face.planeMatches({QStringLiteral("face"), QStringLiteral("F2:top")}));
  CHECK(!face.planeMatches({QStringLiteral("yz")}));
  const GuideStep top = parsed(R"({"text": "x", "target": {"view": "face"}, "done": {"plane": "F2:top"}})", &error);
  CHECK(error.isEmpty());
  CHECK(top.planeMatches({QStringLiteral("face"), QStringLiteral("F2:top")}));
  CHECK(!top.planeMatches({QStringLiteral("face"), QStringLiteral("F2:bottom")}));
  CHECK(named.toJson().contains(QStringLiteral("done")));
  const GuideStep anyPlane = parsed(R"({"text": "x", "target": {"command": "a"}, "done": {"command": "a"}})", &error);
  CHECK(anyPlane.plane().isEmpty());
  CHECK(anyPlane.planeMatches({QStringLiteral("yz")}));
  CHECK(parsed(R"({"text": "x", "target": {"plane": "XY"}, "done": {"plane": "XZ"}})", &error).id.isEmpty());
  CHECK(error.contains(QStringLiteral("one plane")));
  CHECK(parsed(R"({"text": "x", "target": {"plane": ""}})", &error).id.isEmpty());
  CHECK(parsed(R"({"text": "x", "target": {"command": "a"}, "done": {"plane": ""}})", &error).id.isEmpty());

  // A point by its snap: the origin, held back elsewhere.
  const GuideStep origin = parsed(R"({"id": "corner", "text": "Click the origin.", "target": {"view": true},
                                     "done": {"point": "origin"}})",
                                  &error);
  CHECK(error.isEmpty());
  CHECK(origin.done == GuideStep::Done::Point);
  CHECK(origin.pointPlaces == QStringList{QStringLiteral("origin")});
  CHECK(!origin.pointAtGiven);
  CHECK(origin.pointMatches({{QStringLiteral("origin"), QStringLiteral("x_axis"), QStringLiteral("y_axis"),
                              QStringLiteral("point")},
                             QPointF(0, 0)}));
  CHECK(!origin.pointMatches({{QStringLiteral("grid")}, QPointF(10, 5)}));
  CHECK(!origin.pointMatches({{QStringLiteral("x_axis")}, QPointF(10, 0)}));
  CHECK(GuideStep::parse(origin.toJson(), &error).toJson() == origin.toJson());
  CHECK(origin.toJson().value(QStringLiteral("done")).toObject().value(QStringLiteral("point")).toString() ==
        QStringLiteral("origin"));
  // Several places: any of them.
  const GuideStep axes = parsed(R"({"text": "x", "target": {"view": true}, "done": {"point": ["x_axis", "y_axis"]}})",
                                &error);
  CHECK(error.isEmpty());
  CHECK(axes.pointMatches({{QStringLiteral("y_axis"), QStringLiteral("point")}, QPointF(0, 7)}));
  CHECK(!axes.pointMatches({{QStringLiteral("curve")}, QPointF(3, 7)}));
  CHECK(GuideStep::parse(axes.toJson(), &error).toJson() == axes.toJson());
  // Coordinates: where it snapped, within the tolerance.
  const GuideStep at = parsed(R"({"text": "x", "target": {"view": true}, "done": {"point": [20, -10.5]}})", &error);
  CHECK(error.isEmpty());
  CHECK(at.pointAtGiven);
  CHECK(at.pointMatches({{QStringLiteral("grid")}, QPointF(20, -10.5)}));
  CHECK(at.pointMatches({{QStringLiteral("free")}, QPointF(20.0005, -10.5)}));
  CHECK(!at.pointMatches({{QStringLiteral("grid")}, QPointF(20, -10)}));
  CHECK(GuideStep::parse(at.toJson(), &error).toJson() == at.toJson());
  // Only a Point step matches a point.
  CHECK(!plane.pointMatches({{QStringLiteral("origin")}, QPointF(0, 0)}));
  CHECK(parsed(R"({"text": "x", "target": {"view": true}, "done": {"point": "corner"}})", &error).id.isEmpty());
  CHECK(error.contains(QStringLiteral("corner")) && error.contains(QStringLiteral("x_axis")));
  CHECK(parsed(R"({"text": "x", "target": {"view": true}, "done": {"point": []}})", &error).id.isEmpty());
  CHECK(parsed(R"({"text": "x", "target": {"view": true}, "done": {"point": [1, 2, 3]}})", &error).id.isEmpty());
  CHECK(parsed(R"({"text": "x", "target": {"view": true}, "done": {"point": true}})", &error).id.isEmpty());
  CHECK(parsed(R"({"text": "x", "target": {"view": true}, "done": {"point": "origin", "value": "1"}})", &error)
            .id.isEmpty());

  // A value box's value: read, written back; it needs its value.
  const GuideStep width = parsed(R"({"id": "width", "text": "Type the width 40.", "target": {"field": "width"},
                                    "done": {"field": "width", "value": "40 mm"}})",
                                 &error);
  CHECK(error.isEmpty());
  CHECK(width.target == GuideStep::Target::Field);
  CHECK(width.targetName == QStringLiteral("width"));
  CHECK(width.done == GuideStep::Done::Field);
  CHECK(width.doneName == QStringLiteral("width"));
  CHECK(width.value == QStringLiteral("40 mm"));
  CHECK(width.commands().isEmpty());
  CHECK(!width.takesPicks());
  CHECK(guideValueMatches(QStringLiteral("40"), width.value));
  CHECK(!guideValueMatches(QStringLiteral("4"), width.value));
  CHECK(GuideStep::parse(width.toJson(), &error).toJson() == width.toJson());
  CHECK(parsed(R"({"text": "x", "target": {"field": "width"}, "done": {"field": "width"}})", &error).id.isEmpty());
  CHECK(error.contains(QStringLiteral("field")));
  CHECK(parsed(R"({"text": "x", "target": {"field": ""}, "done": {"field": "width", "value": "1"}})", &error)
            .id.isEmpty());
}

} // namespace

int guideStepTests() {
  failures = 0;
  commandSteps();
  inputAndPickSteps();
  modeSteps();
  toolbarSteps();
  navigateSteps();
  refusals();
  holdSteps();
  sketchInputSteps();
  return failures;
}
MITCAD_UNIT_TEST(guideStepTests);
