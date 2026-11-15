// SPDX-License-Identifier: MIT
#pragma once

// Forms (mitcad#1757, app/COMMANDS.md "Form"): a Form's control cage as
// the model's `form_cage` query gives it, what of it is picked in the view,
// the `form.*` commands Edit Form sends for an edit, and the lines the view
// draws of a cage and of a limit surface's preview mesh (`form_limit`, a
// preview's `limit`). Plain Qt Core: no view, no model, no OCCT, so that
// app/tests/form_cage.cpp tests it alone.

#include <array>
#include <optional>
#include <vector>

#include <QJsonObject>
#include <QString>
#include <QVector>

namespace mitcad::forms {

// A point or a direction, x, y and z in millimetres, in the coordinates of
// the Form's component.
using Point = std::array<double, 3>;
// An edge of the cage by its two vertices' ids, as the cage lists it.
using EdgeIds = std::array<qint64, 2>;

struct Vertex {
  qint64 id = 0; // negative: the mirror image of the stored vertex -id
  Point at{};
  bool mirror = false;
  bool corner = false;
};

struct Edge {
  EdgeIds vertices{};
  bool boundary = false;
  bool creased = false;
  // A semi-sharp crease's sharpness (subdivision steps, mitcad#1810); none
  // when the edge is smooth or sharp for good.
  std::optional<double> sharpness;
};

struct Face {
  qint64 id = 0;
  QVector<qint64> vertices; // in order, counter-clockwise seen from outside
  Point center{};
  Point normal{}; // outwards, unit
  bool mirror = false;
};

// The whole cage, mirrored half and all (what an editor draws and picks).
struct Cage {
  QString uid;  // the Form feature (F3)
  QString name; // Form1
  std::vector<Vertex> vertices;
  std::vector<Edge> edges;
  std::vector<Face> faces;
  bool closed = false;
  bool symmetric = false;
  Point symmetryOrigin{};
  Point symmetryNormal{};

  bool isEmpty() const { return vertices.empty(); }
  const Vertex* vertex(qint64 id) const;
  const Face* face(qint64 id) const;
  const Edge* edge(const EdgeIds& ids) const; // either way round
};

// A `form_cage` answer (or a preview's `cage`) read; an empty cage when it
// has none.
Cage cageOf(const QJsonObject& answer);

// What a click picks of the cage.
enum class Kind { Faces, Edges, Vertices };
QString kindKey(Kind kind); // "faces", "edges", "vertices": the choice's values
Kind kindOf(const QString& key);

// Whether two edges are the same, either way round.
bool sameEdge(const EdgeIds& a, const EdgeIds& b);

// The picked faces, edges and vertices of the cage, by their ids in the
// whole cage (a negative id: a mirrored element).
struct Picked {
  QVector<qint64> faces;
  QVector<EdgeIds> edges;
  QVector<qint64> vertices;

  bool isEmpty() const { return faces.isEmpty() && edges.isEmpty() && vertices.isEmpty(); }
  int count() const { return static_cast<int>(faces.size() + edges.size() + vertices.size()); }
  bool contains(Kind kind, qint64 id, const EdgeIds& edge) const;
  bool operator==(const Picked& other) const;
  bool operator!=(const Picked& other) const { return !(*this == other); }
};

// The picks as the hidden input keeps them: compact JSON
// {"faces": [..], "edges": [[a, b], ..], "vertices": [..]}, and read back
// (nothing from a text that is no such object).
QString pickedText(const Picked& picked);
Picked pickedOf(const QString& text);

// A click on an element of `kind` (its id, or an edge's two vertices):
// alone it is the only pick; with `add` (Ctrl or Shift held) it is added,
// or taken out when it was picked. Picks of another kind go.
Picked clicked(const Picked& now, Kind kind, qint64 id, const EdgeIds& edge, bool add);
// The picks the cage still has (after an edit, an undo).
Picked keptIn(const Picked& picked, const Cage& cage);

// The vertices the picks move: the faces' and edges' corners and the
// vertices themselves, each once, in the order met.
QVector<qint64> movedVertices(const Cage& cage, const Picked& picked);
// The middle of those vertices; none when nothing is picked.
std::optional<Point> centerOf(const Cage& cage, const Picked& picked);
// The mean of the picked faces' normals, unit; none without faces or when
// they cancel out.
std::optional<Point> normalOf(const Cage& cage, const Picked& picked);
// Half the diagonal of the box round the moved vertices, else of the whole
// cage; at least 1 mm. The size the handles' scale and Scale's arrow use.
double sizeOf(const Cage& cage, const Picked& picked);

// The picks in words for the panel (mitcad#1327: no ids): "2 faces",
// "1 edge", "3 vertices", joined by commas; empty when nothing is picked.
QString pickedWords(const Picked& picked);

// The edits of Edit Form (its Edit choice's values).
enum class Action {
  Move,
  Rotate,
  Scale,
  Extrude,
  InsertLoop,
  DeleteLoop,
  Crease,
  Uncrease,
  DeleteFaces,
  Symmetry,
};
QString actionKey(Action action); // "move", "rotate", ...: the choice's values
std::optional<Action> actionOf(const QString& key);
// The kind of element an edit takes, when it takes one kind only (Extrude:
// faces, Insert Edge Loop: edges); none when the Select choice says
// (Move, Rotate, Scale: any; Crease, Uncrease: edges or vertices) or the
// edit takes none (Symmetry).
std::optional<Kind> onlyKind(Action action);
// Whether the edit takes picks of `kind`.
bool takesKind(Action action, Kind kind);
// The undo step an edit makes, as the model names it ("Extrude" of
// "Extrude in Form1").
QString stepName(Action action);

// An edit's values, in millimetres and radians.
struct Edit {
  Action action = Action::Move;
  Point move{};      // Move: along x, y and z
  int axis = 2;      // Rotate: about x (0), y (1) or z (2) through the picks' middle
  double angle = 0;  // Rotate
  double factor = 1; // Scale: every axis, about the picks' middle
  double distance = 0; // Extrude: along the faces' normals
  double at = 0.5;     // Insert Edge Loop: from the edge's first vertex
  // Crease: none sharp for good; a number semi-sharp, sharp for that many
  // subdivision steps (mitcad#1810).
  std::optional<double> sharpness;
  // Symmetry: "x", "y", "z" (through the middle of the cage's box),
  // "none", or "picked": the plane `planeOrigin`, `planeNormal` (in the
  // Form's component's coordinates) when `planePicked` (mitcad#1810).
  QString plane;
  bool planePicked = false;
  Point planeOrigin{};
  Point planeNormal{};
};

// The `form.*` command of an edit of Form `form` on the picks (with
// `preview`: the model answers with the edited cage and its limit mesh and
// changes nothing); an empty object when the edit would do nothing (a Move
// of 0, a Scale of 1, nothing picked that it takes).
QJsonObject editCommand(const QString& form, const Edit& edit, const Picked& picked, bool preview);
// Why the edit cannot be applied to the picks, in words for the panel; empty
// when it can.
QString editRefusal(const Edit& edit, const Picked& picked);

// A vertex dragged in the view (mitcad#1810): where it goes, in the Form's
// component's coordinates, as the hidden input keeps it (compact JSON
// {"vertex": id, "at": [x, y, z]}) and read back (none from a text that
// is no such object or whose numbers are not finite).
struct Drag {
  qint64 vertex = 0;
  Point at{};
};
QString dragText(const Drag& drag);
std::optional<Drag> dragOf(const QString& text);
// The `form.set_positions` command that puts the dragged vertex there (one
// undo step, "Drag in Form1"); with `preview` the model only answers.
QJsonObject dragCommand(const QString& form, const Drag& drag, bool preview);

// Lines to draw over the view: each edge of the cage; each edge of a limit
// mesh (`positions`, `triangles`, `faces` as `form_limit` gives them) that
// lies between two cage faces' regions or on its boundary, so that the
// preview shows the shape without its triangles.
using Lines = std::vector<std::vector<Point>>;
Lines cageLines(const Cage& cage);
Lines limitOutline(const QJsonObject& limit);

} // namespace mitcad::forms
