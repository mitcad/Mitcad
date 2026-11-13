// SPDX-License-Identifier: MIT
// The joint highlight (mitcad#523): what each side of a joint shows (its
// geometry, its frame only, its part), where (paths from the root through
// the occurrence that places the joint's component), the joined
// occurrences, the log lines and the selection card's text, from entries
// shaped as the `joints` query gives them.
#include "framework/JointHighlight.hpp"

#include <cstdio>

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QString>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "joint_highlight.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad;

QJsonObject parse(const char* text) {
  QJsonParseError error;
  const QJsonDocument document = QJsonDocument::fromJson(QByteArray(text), &error);
  if (error.error != QJsonParseError::NoError) {
    std::fprintf(stderr, "joint_highlight.cpp: bad JSON: %s\n", qPrintable(error.errorString()));
    ++failures;
  }
  return document.object();
}

// A face-to-face joint of Pin:1 on Plate:1 in the root, and a circular
// edge of Shaft:1 on the plate's face with an override that moves the
// origin.
const char* kFaceJoint = R"j({
  "uid": "F6", "name": "Joint1", "type": "joint", "kind": "revolute", "component": "C0",
  "a": {"occurrence": "Pin:1", "path": ["O2"], "component": "C2", "on": "face",
        "geometry": {"body": "F4.b0", "face": "F4:start(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})"}},
  "b": {"occurrence": "Plate:1", "path": ["O1"], "component": "C1", "on": "face",
        "geometry": {"body": "F2.b0", "face": "F2:end(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]})"}},
  "frames": {"a": {"origin": [20, 20, 10], "x_axis": [1, 0, 0], "y_axis": [0, -1, 0], "z_axis": [0, 0, -1]},
             "b": {"origin": [20, 20, 10], "x_axis": [1, 0, 0], "y_axis": [0, 1, 0], "z_axis": [0, 0, 1]}}})j";

const char* kEdgeJoint = R"j({
  "uid": "F7", "name": "Joint2", "type": "joint", "kind": "rigid", "component": "C0",
  "a": {"occurrence": "Plate:1", "path": ["O1"], "component": "C1", "on": "face",
        "geometry": {"body": "F2.b0", "face": "F2:end(r{c1})"}, "frame_override": {"origin": [5, 5, 10]}},
  "b": {"occurrence": "Shaft:1", "path": ["O3"], "component": "C3", "on": "edge",
        "geometry": {"body": "F5.b0", "edge": "E{F5:bottom|F5:side0}"}}})j";

void faceToFaceTests() {
  const JointHighlight joint = jointHighlightOf(parse(kFaceJoint), QString());
  CHECK(joint.isValid());
  CHECK(joint.name == QStringLiteral("Joint1"));
  CHECK(joint.kind == QStringLiteral("revolute"));
  CHECK(joint.sides.size() == 2);
  const JointSide& a = joint.sides.at(0);
  CHECK(a.key == QStringLiteral("a"));
  CHECK(a.on == QStringLiteral("face"));
  CHECK(a.path == QStringLiteral("O2"));
  CHECK(a.showsGeometry);
  CHECK(!a.showsPart);
  CHECK(a.frame.has_value());
  CHECK(a.frame && a.frame->origin[2] == 10.0 && a.frame->z[2] == -1.0);
  CHECK(joint.occurrences == QStringList({QStringLiteral("O2"), QStringLiteral("O1")}));
  CHECK(joint.occurrenceNames == QStringList({QStringLiteral("Pin:1"), QStringLiteral("Plate:1")}));
  const QStringList lines = jointHighlightLog(joint);
  CHECK(lines.size() == 3);
  CHECK(lines.value(0) ==
        QStringLiteral("Joint highlight a: face F4:start(r{c1[c4,c2],c2[c1,c3],c3[c2,c4],c4[c3,c1]}) of F4.b0 "
                       "in Pin:1 (O2) [Joint1]"));
  CHECK(lines.value(1).startsWith(QStringLiteral("Joint highlight b: face F2:end(")));
  CHECK(lines.value(1).endsWith(QStringLiteral(" of F2.b0 in Plate:1 (O1) [Joint1]")));
  CHECK(lines.value(2) == QStringLiteral("Joint highlight occurrences: Pin:1 (O2), Plate:1 (O1) [Joint1]"));
  // The card: the kind and each side in words (mitcad#1741), bodies by
  // name, a body without one as "a body"; no topological name or uid.
  const QString card = jointCardText(joint, [](const QString& body) {
    return body == QStringLiteral("F4.b0") ? QStringLiteral("Body2") : QString();
  });
  CHECK(card == QStringLiteral("<b>Joint1</b>: revolute, a = Pin:1, face of Body2, b = Plate:1, face of a body"));
}

void edgeAndOverrideTests() {
  const JointHighlight joint = jointHighlightOf(parse(kEdgeJoint), QString());
  CHECK(joint.sides.size() == 2);
  // An override that moves the origin: the frame only (none given here:
  // a joint that did not recompute has no frames).
  const JointSide& a = joint.sides.at(0);
  CHECK(a.frameOverride);
  CHECK(!a.showsGeometry);
  CHECK(!a.frame.has_value());
  const JointSide& b = joint.sides.at(1);
  CHECK(b.on == QStringLiteral("edge"));
  CHECK(b.showsGeometry);
  const QStringList lines = jointHighlightLog(joint);
  CHECK(lines.value(0) == QStringLiteral("Joint highlight a: face F2:end(r{c1}) of F2.b0 in Plate:1 (O1), frame only "
                                       "[Joint2]"));
  CHECK(lines.value(1) == QStringLiteral("Joint highlight b: edge E{F5:bottom|F5:side0} of F5.b0 in Shaft:1 (O3) "
                                       "[Joint2]"));
  CHECK(jointCardText(joint, {}).contains(QStringLiteral("a = Plate:1, face of a body (its frame)")));
  // Only a shift keeps the geometry: it follows it.
  QJsonObject entry = parse(kEdgeJoint);
  QJsonObject side = entry.value(QStringLiteral("a")).toObject();
  side.insert(QStringLiteral("frame_override"), parse(R"j({"shift": [0, 0, 5]})j"));
  entry.insert(QStringLiteral("a"), side);
  CHECK(jointHighlightOf(entry, QString()).sides.at(0).showsGeometry);
}

void placedTests() {
  // A joint of a component placed by O5: its sides are below O5, and its
  // own geometry is O5's.
  const JointHighlight joint = jointHighlightOf(parse(R"j({
    "uid": "F9", "name": "Joint3", "kind": "slider", "component": "C4",
    "a": {"occurrence": "Arm:1/Pin:2", "path": ["O6", "O7"], "on": "vertex",
          "geometry": {"body": "F8.b0", "vertex": "V{a|b|c}"}},
    "b": {"occurrence": "", "path": [], "on": "origin", "geometry": "z"}})j"),
                                                QStringLiteral("O5"));
  CHECK(joinedPath(QStringLiteral("O5"), {QStringLiteral("O6"), QStringLiteral("O7")}) ==
        QStringLiteral("O5/O6/O7"));
  CHECK(joinedPath(QString(), {QStringLiteral("O6")}) == QStringLiteral("O6"));
  CHECK(joinedPath(QStringLiteral("O5"), {}) == QStringLiteral("O5"));
  CHECK(joint.placing == QStringLiteral("O5"));
  CHECK(joint.sides.at(0).path == QStringLiteral("O5/O6/O7"));
  CHECK(joint.sides.at(1).path == QStringLiteral("O5"));
  CHECK(joint.sides.at(1).showsGeometry);
  // The component's own geometry joins no occurrence.
  CHECK(joint.occurrences == QStringList({QStringLiteral("O5/O6/O7")}));
  const QStringList lines = jointHighlightLog(joint);
  CHECK(lines.value(0) == QStringLiteral("Joint highlight a: vertex V{a|b|c} of F8.b0 in Arm:1/Pin:2 (O5/O6/O7) "
                                       "[Joint3]"));
  CHECK(lines.value(1) == QStringLiteral("Joint highlight b: origin axis z of the joint's component [Joint3]"));
}

void otherReferenceTests() {
  // What each kind of reference is called.
  const auto text = [](const char* on, const QJsonValue& geometry) {
    JointSide side;
    side.on = QLatin1String(on);
    side.geometry = geometry;
    return jointSideGeometryText(side);
  };
  CHECK(text("origin", QStringLiteral("xy")) == QStringLiteral("origin plane xy"));
  CHECK(text("origin", QStringLiteral("origin")) == QStringLiteral("the origin"));
  CHECK(text("joint_origin", QStringLiteral("F9")) == QStringLiteral("joint origin F9"));
  CHECK(text("construction", QStringLiteral("F3")) == QStringLiteral("construction geometry F3"));
  CHECK(text("fixed", parse(R"j({"origin": [0, 0, 0], "normal": [0, 0, 1]})j")) == QStringLiteral("fixed plane"));
  CHECK(text("fixed", parse(R"j({"origin": [0, 0, 0], "direction": [0, 0, 1]})j")) == QStringLiteral("fixed axis"));
  CHECK(text("fixed", parse(R"j({"point": [0, 0, 50]})j")) == QStringLiteral("fixed point"));
  CHECK(text("mount", parse(R"j({"mount": "F7:mount(slot_x+_1)"})j")) == QStringLiteral("mount frame slot_x+_1 of F7"));
  CHECK(text("sketch_curve", parse(R"j({"sketch": "F1", "curve": "c2"})j")) == QStringLiteral("curve c2 of F1"));
  CHECK(text("sketch_curve", parse(R"j({"sketch": "F1", "curves": ["c2", "c3"]})j")) ==
        QStringLiteral("curves c2, c3 of F1"));
  CHECK(text("sketch_point", parse(R"j({"sketch": "F1", "point": "p4"})j")) == QStringLiteral("point p4 of F1"));
  CHECK(text("body", parse(R"j({"body": "F2.b0"})j")) == QStringLiteral("body F2.b0"));
}

void buildAndAsBuiltTests() {
  // A BUILD joint: a part's mount frame on another part's mount frame with
  // an override; both show their frame and their part.
  const JointHighlight build = jointHighlightOf(parse(R"j({
    "uid": "F3", "name": "Joint1", "kind": "rigid", "component": "C0",
    "a": {"occurrence": "Timber 48x98:2", "path": ["O2"], "on": "mount", "geometry": {"mount": "F2:mount(end_a)"}},
    "b": {"occurrence": "Timber 48x98:1", "path": ["O1"], "on": "mount", "geometry": {"mount": "F1:mount(face_xp)"},
          "frame_override": {"origin": [49, 0, 100], "z_axis": [1, 0, 0], "x_axis": [0, 0, 1]}},
    "frames": {"a": {"origin": [49, 0, 100], "x_axis": [0, 0, 1], "y_axis": [0, -1, 0], "z_axis": [-1, 0, 0]},
               "b": {"origin": [49, 0, 100], "x_axis": [0, 0, 1], "y_axis": [0, 1, 0], "z_axis": [1, 0, 0]}}})j"),
                                                QString());
  for (const JointSide& side : build.sides) {
    CHECK(!side.showsGeometry);
    CHECK(side.showsPart);
    CHECK(side.frame.has_value());
  }
  const QStringList lines = jointHighlightLog(build);
  CHECK(lines.value(0) == QStringLiteral("Joint highlight a: mount frame end_a of F2 in Timber 48x98:2 (O2), frame and "
                                       "part [Joint1]"));
  CHECK(lines.value(1).startsWith(QStringLiteral("Joint highlight b: mount frame face_xp of F1 in Timber 48x98:1 (O1)")));
  CHECK(build.occurrences == QStringList({QStringLiteral("O2"), QStringLiteral("O1")}));
  // An as-built joint: its sides are its occurrences; the motion origin
  // joins nothing more.
  const JointHighlight asBuilt = jointHighlightOf(parse(R"j({
    "uid": "F8", "name": "AsBuiltJoint1", "kind": "revolute", "component": "C0",
    "a": {"occurrence": "Pin:1", "path": ["O2"], "on": "occurrence", "component": "C2"},
    "b": {"occurrence": "Plate:1", "path": ["O1"], "on": "occurrence", "component": "C1"},
    "origin": {"occurrence": "Pin:1", "path": ["O2"], "on": "face", "geometry": {"body": "F4.b0", "face": "F4:end"}}})j"),
                                                  QString());
  CHECK(asBuilt.sides.size() == 3);
  CHECK(asBuilt.sides.at(0).showsPart && !asBuilt.sides.at(0).showsGeometry);
  CHECK(asBuilt.sides.at(2).key == QStringLiteral("origin"));
  CHECK(asBuilt.sides.at(2).showsGeometry);
  CHECK(asBuilt.occurrences == QStringList({QStringLiteral("O2"), QStringLiteral("O1")}));
  CHECK(jointHighlightLog(asBuilt).value(0) ==
        QStringLiteral("Joint highlight a: occurrence in Pin:1 (O2) [AsBuiltJoint1]"));
  CHECK(jointCardText(asBuilt, {}).startsWith(QStringLiteral("<b>AsBuiltJoint1</b>: revolute, a = Pin:1, b = Plate:1, "
                                                             "origin = Pin:1, face of a body")));
}

void constraintTests() {
  // A constraint set: each row's references, named by their row.
  const JointHighlight set = jointHighlightOf(parse(R"j({
    "uid": "F10", "name": "Constraint1", "component": "C0",
    "rows": [{"row": 1, "type": "align",
              "a": {"occurrence": "Pin:1", "path": ["O2"], "on": "face", "geometry": {"body": "F4.b0", "face": "F4:start"}},
              "b": {"occurrence": "Plate:1", "path": ["O1"], "on": "face", "geometry": {"body": "F2.b0", "face": "F2:end"}}}]})j"),
                                              QString());
  CHECK(set.kind == QStringLiteral("constraint"));
  CHECK(set.sides.size() == 2);
  CHECK(set.sides.at(0).row == 1);
  CHECK(jointHighlightLog(set).value(1) ==
        QStringLiteral("Joint highlight row 1 b: face F2:end of F2.b0 in Plate:1 (O1) [Constraint1]"));
  // Not a joint at all: nothing.
  CHECK(!jointHighlightOf(QJsonObject(), QString()).isValid());
}

} // namespace

int jointHighlightTests() {
  failures = 0;
  faceToFaceTests();
  edgeAndOverrideTests();
  placedTests();
  otherReferenceTests();
  buildAndAsBuiltTests();
  constraintTests();
  return failures;
}
