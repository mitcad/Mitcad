// SPDX-License-Identifier: MIT
// Forms (mitcad#1757, commands/FormCage.hpp): a `form_cage` answer read,
// the picks kept as text and changed by clicks, the vertices they move,
// their middle, normal and size, the commands Edit Form sends (none for an
// edit that does nothing) and why an edit is refused, and the lines drawn
// of a cage and of a limit mesh's outline.
#include "UnitTests.hpp"
#include "commands/FormCage.hpp"

#include <cmath>
#include <cstdio>

#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>

namespace {

int failures = 0;

void check(bool condition, const char* expression, int line) {
  if (!condition) {
    std::fprintf(stderr, "form_cage.cpp:%d: check failed: %s\n", line, expression);
    ++failures;
  }
}

#define CHECK(expr) check((expr), #expr, __LINE__)

using namespace mitcad::forms;

// A unit cube's cage as the model's `form_cage` query gives it: vertices
// 1-8, faces 1 (bottom, -z) to 6 (top, +z), its top edges creased.
QJsonObject cubeAnswer() {
  const char* json = R"({
    "uid": "F1", "name": "Form1", "closed": true, "symmetry": null,
    "vertices": [
      {"id": 1, "at": [0, 0, 0], "mirror": false, "corner": false},
      {"id": 2, "at": [10, 0, 0], "mirror": false, "corner": false},
      {"id": 3, "at": [10, 10, 0], "mirror": false, "corner": false},
      {"id": 4, "at": [0, 10, 0], "mirror": false, "corner": false},
      {"id": 5, "at": [0, 0, 10], "mirror": false, "corner": true},
      {"id": 6, "at": [10, 0, 10], "mirror": false, "corner": false},
      {"id": 7, "at": [10, 10, 10], "mirror": false, "corner": false},
      {"id": 8, "at": [0, 10, 10], "mirror": false, "corner": false}],
    "edges": [
      {"vertices": [1, 2], "boundary": false, "creased": false},
      {"vertices": [5, 6], "boundary": false, "creased": true},
      {"vertices": [6, 7], "boundary": false, "creased": true}],
    "faces": [
      {"id": 1, "vertices": [1, 4, 3, 2], "center": [5, 5, 0], "normal": [0, 0, -1], "mirror": false},
      {"id": 2, "vertices": [1, 2, 6, 5], "center": [5, 0, 5], "normal": [0, -1, 0], "mirror": false},
      {"id": 6, "vertices": [5, 6, 7, 8], "center": [5, 5, 10], "normal": [0, 0, 1], "mirror": false}]
  })";
  return QJsonDocument::fromJson(QByteArray(json)).object();
}

void readTests() {
  const Cage cage = cageOf(cubeAnswer());
  CHECK(cage.uid == QStringLiteral("F1"));
  CHECK(cage.name == QStringLiteral("Form1"));
  CHECK(cage.vertices.size() == 8);
  CHECK(cage.edges.size() == 3);
  CHECK(cage.faces.size() == 3);
  CHECK(cage.closed);
  CHECK(!cage.symmetric);
  CHECK(cage.vertex(5) != nullptr && cage.vertex(5)->corner);
  CHECK(cage.vertex(9) == nullptr);
  CHECK(cage.face(6) != nullptr && cage.face(6)->normal[2] == 1.0);
  CHECK(cage.edge({6, 5}) != nullptr && cage.edge({6, 5})->creased); // either way round
  CHECK(cage.edge({1, 2}) != nullptr && !cage.edge({1, 2})->creased);
  // A symmetric cage.
  QJsonObject symmetric = cubeAnswer();
  symmetric.insert(QStringLiteral("symmetry"),
                   QJsonObject{{QStringLiteral("origin"), QJsonArray{5, 0, 0}},
                               {QStringLiteral("normal"), QJsonArray{1, 0, 0}}});
  const Cage mirrored = cageOf(symmetric);
  CHECK(mirrored.symmetric);
  CHECK(mirrored.symmetryOrigin[0] == 5.0 && mirrored.symmetryNormal[0] == 1.0);
  CHECK(cageOf(QJsonObject()).isEmpty());
}

void pickTests() {
  Picked picked;
  CHECK(picked.isEmpty());
  CHECK(pickedWords(picked).isEmpty());
  // A face alone, then another added, then the first taken out.
  picked = clicked(picked, Kind::Faces, 6, {}, false);
  CHECK(picked.faces == QVector<qint64>{6});
  picked = clicked(picked, Kind::Faces, 2, {}, true);
  CHECK((picked.faces == QVector<qint64>{6, 2}));
  picked = clicked(picked, Kind::Faces, 6, {}, true);
  CHECK(picked.faces == QVector<qint64>{2});
  // A click without Ctrl replaces; an edge's other kind goes.
  picked = clicked(picked, Kind::Edges, 0, {5, 6}, false);
  CHECK(picked.faces.isEmpty() && picked.edges.size() == 1);
  CHECK(picked.contains(Kind::Edges, 0, {6, 5}));
  picked = clicked(picked, Kind::Edges, 0, {6, 5}, true); // the same edge the other way round
  CHECK(picked.isEmpty());
  // Kept as text and read back; a mirrored id stays negative.
  Picked several;
  several.faces = {1, -1};
  several.edges = {{1, 2}};
  several.vertices = {5};
  CHECK(pickedOf(pickedText(several)) == several);
  CHECK(pickedOf(QStringLiteral("not json")).isEmpty());
  CHECK(several.count() == 4);
  // Only what the cage still has.
  const Cage cage = cageOf(cubeAnswer());
  Picked gone;
  gone.faces = {6, 9};
  gone.edges = {{2, 1}, {7, 8}};
  gone.vertices = {3, 12};
  const Picked kept = keptIn(gone, cage);
  CHECK(kept.faces == QVector<qint64>{6});
  CHECK(kept.edges.size() == 1);
  CHECK(kept.vertices == QVector<qint64>{3});
}

void geometryTests() {
  const Cage cage = cageOf(cubeAnswer());
  Picked top;
  top.faces = {6};
  CHECK((movedVertices(cage, top) == QVector<qint64>{5, 6, 7, 8}));
  const std::optional<Point> middle = centerOf(cage, top);
  CHECK(middle && (*middle)[0] == 5.0 && (*middle)[1] == 5.0 && (*middle)[2] == 10.0);
  const std::optional<Point> up = normalOf(cage, top);
  CHECK(up && (*up)[2] == 1.0);
  // The top's half diagonal: 10 x 10 x 0.
  CHECK(std::abs(sizeOf(cage, top) - std::sqrt(200.0) / 2.0) < 1e-9);
  // Faces that face apart cancel out: no normal.
  Picked apart;
  apart.faces = {1, 6};
  CHECK(!normalOf(cage, apart));
  // An edge and a vertex move their vertices once each.
  Picked mixed;
  mixed.edges = {{5, 6}};
  mixed.vertices = {6, 1};
  CHECK((movedVertices(cage, mixed) == QVector<qint64>{5, 6, 1}));
  CHECK(!centerOf(cage, Picked()));
  // Nothing picked: the whole cage's size.
  CHECK(std::abs(sizeOf(cage, Picked()) - std::sqrt(300.0) / 2.0) < 1e-9);
}

void commandTests() {
  Picked top;
  top.faces = {6};
  Edit edit;
  // A move of nothing does nothing.
  CHECK(editCommand(QStringLiteral("F1"), edit, top, false).isEmpty());
  edit.move = {0, 0, 5};
  const QJsonObject move = editCommand(QStringLiteral("F1"), edit, top, true);
  CHECK(move.value(QStringLiteral("cmd")).toString() == QStringLiteral("form.move"));
  CHECK(move.value(QStringLiteral("form")).toString() == QStringLiteral("F1"));
  CHECK(move.value(QStringLiteral("preview")).toBool());
  CHECK(move.value(QStringLiteral("faces")).toArray() == QJsonArray{6});
  CHECK(move.value(QStringLiteral("by")).toArray() == (QJsonArray{0, 0, 5}));
  CHECK(!editCommand(QStringLiteral("F1"), edit, top, false).contains(QStringLiteral("preview")));
  CHECK(editCommand(QStringLiteral("F1"), edit, Picked(), false).isEmpty());
  // Rotate about z through the middle.
  edit.action = Action::Rotate;
  edit.angle = 0.5;
  const QJsonObject rotate = editCommand(QStringLiteral("F1"), edit, top, false);
  CHECK(rotate.value(QStringLiteral("cmd")).toString() == QStringLiteral("form.rotate"));
  CHECK(rotate.value(QStringLiteral("axis")).toArray() == (QJsonArray{0, 0, 1}));
  CHECK(rotate.value(QStringLiteral("angle")).toDouble() == 0.5);
  // Scale: 1 does nothing, 0 is refused.
  edit.action = Action::Scale;
  CHECK(editCommand(QStringLiteral("F1"), edit, top, false).isEmpty());
  edit.factor = 0.0;
  CHECK(editCommand(QStringLiteral("F1"), edit, top, false).isEmpty());
  edit.factor = 1.5;
  CHECK(editCommand(QStringLiteral("F1"), edit, top, false).value(QStringLiteral("factor")).toDouble() == 1.5);
  // Extrude the faces.
  edit.action = Action::Extrude;
  edit.distance = 4.0;
  const QJsonObject extrude = editCommand(QStringLiteral("F1"), edit, top, false);
  CHECK(extrude.value(QStringLiteral("cmd")).toString() == QStringLiteral("form.extrude"));
  CHECK(extrude.value(QStringLiteral("distance")).toDouble() == 4.0);
  // Edge loops take one edge.
  Picked edge;
  edge.edges = {{5, 6}};
  edit.action = Action::InsertLoop;
  edit.at = 0.25;
  const QJsonObject loop = editCommand(QStringLiteral("F1"), edit, edge, false);
  CHECK(loop.value(QStringLiteral("cmd")).toString() == QStringLiteral("form.insert_loop"));
  CHECK(loop.value(QStringLiteral("edge")).toArray() == (QJsonArray{5, 6}));
  CHECK(loop.value(QStringLiteral("at")).toDouble() == 0.25);
  edit.action = Action::DeleteLoop;
  CHECK(editCommand(QStringLiteral("F1"), edit, edge, false).value(QStringLiteral("cmd")).toString() ==
        QStringLiteral("form.delete_loop"));
  CHECK(editCommand(QStringLiteral("F1"), edit, top, false).isEmpty());
  // Crease for good, uncrease with sharpness 0; a vertex made a corner.
  edit.action = Action::Crease;
  Picked crease = edge;
  crease.vertices = {7};
  const QJsonObject sharp = editCommand(QStringLiteral("F1"), edit, crease, false);
  CHECK(sharp.value(QStringLiteral("cmd")).toString() == QStringLiteral("form.crease"));
  CHECK(!sharp.contains(QStringLiteral("sharpness")));
  CHECK(sharp.value(QStringLiteral("vertices")).toArray() == QJsonArray{7});
  edit.action = Action::Uncrease;
  CHECK(editCommand(QStringLiteral("F1"), edit, crease, false).value(QStringLiteral("sharpness")).toDouble() == 0.0);
  // Symmetry: a plane by its axis, or none (null).
  edit.action = Action::Symmetry;
  edit.plane = QStringLiteral("x");
  const QJsonObject symmetry = editCommand(QStringLiteral("F1"), edit, Picked(), false);
  CHECK(symmetry.value(QStringLiteral("cmd")).toString() == QStringLiteral("form.symmetry"));
  CHECK(symmetry.value(QStringLiteral("plane")).toString() == QStringLiteral("x"));
  edit.plane = QStringLiteral("none");
  CHECK(editCommand(QStringLiteral("F1"), edit, Picked(), false).value(QStringLiteral("plane")).isNull());
  // Why an edit is refused, in words.
  Edit none;
  CHECK(!editRefusal(none, Picked()).isEmpty());
  CHECK(editRefusal(none, top).isEmpty());
  none.action = Action::Extrude;
  CHECK(!editRefusal(none, edge).isEmpty());
  none.action = Action::Symmetry;
  CHECK(editRefusal(none, Picked()).isEmpty());
  // The edits and their kinds.
  CHECK(actionOf(QStringLiteral("insert_loop")) == Action::InsertLoop);
  CHECK(!actionOf(QStringLiteral("nothing")));
  CHECK(onlyKind(Action::Extrude) == Kind::Faces);
  CHECK(!onlyKind(Action::Move));
  CHECK(takesKind(Action::Crease, Kind::Vertices));
  CHECK(!takesKind(Action::Crease, Kind::Faces));
  CHECK(!takesKind(Action::Symmetry, Kind::Faces));
  CHECK(kindOf(kindKey(Kind::Edges)) == Kind::Edges);
  CHECK(stepName(Action::InsertLoop) == QStringLiteral("Insert Edge Loop"));
}

void lineTests() {
  const Cage cage = cageOf(cubeAnswer());
  const Lines edges = cageLines(cage);
  CHECK(edges.size() == 3);
  CHECK(edges[1].size() == 2 && edges[1][0][2] == 10.0);
  // Two triangles of cage face 1 and one of face 2 sharing nodes: the
  // diagonal inside face 1 is left out, the edge between the faces and
  // the boundary are drawn.
  //   nodes 0 (0,0), 1 (1,0), 2 (1,1), 3 (0,1), 4 (2,0)
  const QJsonObject limit{
      {QStringLiteral("positions"), QJsonArray{0, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0, 2, 0, 0}},
      {QStringLiteral("triangles"), QJsonArray{0, 1, 2, 0, 2, 3, 1, 4, 2}},
      {QStringLiteral("faces"), QJsonArray{1, 1, 2}}};
  const Lines outline = limitOutline(limit);
  // Edges: 0-1, 1-2 (between 1 and 2), 0-2 (inside: left out), 2-3, 0-3,
  // 1-4, 2-4: six drawn.
  CHECK(outline.size() == 6);
  CHECK(limitOutline(QJsonObject()).empty());
}

// mitcad#1810: a semi-sharp crease read and sent with its sharpness, a
// mirror plane picked, and a vertex's drag kept as text and sent as
// `form.set_positions`.
void followUpTests() {
  // A semi-sharp edge in the answer.
  QJsonObject answer = cubeAnswer();
  QJsonArray edges = answer.value(QStringLiteral("edges")).toArray();
  QJsonObject semi = edges.at(1).toObject();
  semi.insert(QStringLiteral("sharpness"), 1.5);
  edges.replace(1, semi);
  answer.insert(QStringLiteral("edges"), edges);
  const Cage cage = cageOf(answer);
  CHECK(cage.edge({5, 6}) != nullptr && cage.edge({5, 6})->sharpness == 1.5);
  CHECK(cage.edge({6, 7}) != nullptr && !cage.edge({6, 7})->sharpness); // sharp for good
  CHECK(cage.edge({1, 2}) != nullptr && !cage.edge({1, 2})->sharpness);

  // Crease semi-sharp: the sharpness sent; none or less than zero refused.
  Picked edge;
  edge.edges = {{5, 6}};
  Edit crease;
  crease.action = Action::Crease;
  crease.sharpness = 1.5;
  const QJsonObject semiCommand = editCommand(QStringLiteral("F1"), crease, edge, false);
  CHECK(semiCommand.value(QStringLiteral("cmd")).toString() == QStringLiteral("form.crease"));
  CHECK(semiCommand.value(QStringLiteral("sharpness")).toDouble() == 1.5);
  CHECK(editRefusal(crease, edge).isEmpty());
  crease.sharpness = 0.0;
  CHECK(editCommand(QStringLiteral("F1"), crease, edge, false).isEmpty());
  CHECK(!editRefusal(crease, edge).isEmpty());
  crease.sharpness = std::nan("");
  CHECK(editCommand(QStringLiteral("F1"), crease, edge, false).isEmpty());
  CHECK(!editRefusal(crease, edge).isEmpty());
  // Uncrease takes no sharpness of its own.
  crease.action = Action::Uncrease;
  CHECK(editCommand(QStringLiteral("F1"), crease, edge, false).value(QStringLiteral("sharpness")).toDouble() == 0.0);

  // Symmetry about a picked plane: refused until one is picked, then its
  // origin and unit normal.
  Edit mirror;
  mirror.action = Action::Symmetry;
  mirror.plane = QStringLiteral("picked");
  CHECK(editCommand(QStringLiteral("F1"), mirror, Picked(), false).isEmpty());
  CHECK(!editRefusal(mirror, Picked()).isEmpty());
  mirror.planePicked = true;
  mirror.planeOrigin = {5, 0, 0};
  mirror.planeNormal = {2, 0, 0};
  const QJsonObject picked = editCommand(QStringLiteral("F1"), mirror, Picked(), true);
  CHECK(picked.value(QStringLiteral("cmd")).toString() == QStringLiteral("form.symmetry"));
  CHECK(picked.value(QStringLiteral("preview")).toBool());
  const QJsonObject plane = picked.value(QStringLiteral("plane")).toObject();
  CHECK(plane.value(QStringLiteral("origin")).toArray() == (QJsonArray{5, 0, 0}));
  CHECK(plane.value(QStringLiteral("normal")).toArray() == (QJsonArray{1, 0, 0}));
  CHECK(editRefusal(mirror, Picked()).isEmpty());
  mirror.planeNormal = {0, 0, 0};
  CHECK(editCommand(QStringLiteral("F1"), mirror, Picked(), false).isEmpty());

  // A vertex's drag: as text and back, and its command.
  Drag drag;
  drag.vertex = -3; // a mirrored vertex
  drag.at = {1.5, -2, 7};
  const std::optional<Drag> back = dragOf(dragText(drag));
  CHECK(back && back->vertex == -3 && back->at[0] == 1.5 && back->at[1] == -2.0 && back->at[2] == 7.0);
  CHECK(!dragOf(QString()));
  CHECK(!dragOf(QStringLiteral("{\"vertex\": 3}")));
  CHECK(!dragOf(QStringLiteral("{\"vertex\": 0, \"at\": [0, 0, 0]}")));
  const QJsonObject set = dragCommand(QStringLiteral("F1"), drag, false);
  CHECK(set.value(QStringLiteral("cmd")).toString() == QStringLiteral("form.set_positions"));
  CHECK(set.value(QStringLiteral("form")).toString() == QStringLiteral("F1"));
  CHECK(!set.contains(QStringLiteral("preview")));
  const QJsonObject position = set.value(QStringLiteral("positions")).toArray().at(0).toObject();
  CHECK(position.value(QStringLiteral("vertex")).toInt() == -3);
  CHECK(position.value(QStringLiteral("at")).toArray() == (QJsonArray{1.5, -2, 7}));
  CHECK(dragCommand(QStringLiteral("F1"), drag, true).value(QStringLiteral("preview")).toBool());
}

} // namespace

int formCageTests() {
  readTests();
  pickTests();
  geometryTests();
  commandTests();
  lineTests();
  followUpTests();
  return failures;
}
MITCAD_UNIT_TEST(formCageTests);
