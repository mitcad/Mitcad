// SPDX-License-Identifier: MIT
// Press Pull, Fillet and Chamfer (U4): fillets and chamfers with sets of
// edges, each set its own size (commands.md, "fillet",
// "chamfer"); Press Pull turns into the feature its selection asks for
// (a face continued Straight or Along the walls: press_pull, mitcad#1850),
// on a fillet's face opens the fillet (P6) and on a feature's end may
// open that feature.
#include <algorithm>
#include <array>
#include <cmath>
#include <cstddef>
#include <exception>
#include <memory>
#include <optional>
#include <string>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QKeySequence>
#include <QRegularExpression>
#include <QString>

#include <TopoDS_Shape.hxx>
#include <gp_Dir.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "../framework/HandleStep.hpp"
#include "../framework/ModelShapes.hpp"
#include "ChamferHandles.hpp"
#include "CommandFactories.hpp"
#include "CommandSupport.hpp"
#include "mitcad/geometry/naming.hpp"

namespace mitcad::cmd {
namespace {

const QString kSets = QStringLiteral("sets");
const QString kMid = QStringLiteral("mid");
const QString kControl = QStringLiteral("control");

QString rowKey(int row, const char* id) { return CommandState::rowKey(kSets, row, QString::fromLatin1(id)); }
QString midKey(int row, const char* id) { return CommandState::rowKey(kMid, row, QString::fromLatin1(id)); }
QString controlKey(int row, const char* id) {
  return CommandState::rowKey(kControl, row, QString::fromLatin1(id));
}

// The knots of a spline radius of `count` control radii (mitcad#464): the
// numbers typed (commas or spaces between them), or with none typed a
// clamped knot vector of degree 3 at most with its inner knots evenly
// spaced. Empty when what is typed is not a list of numbers.
std::optional<QJsonArray> splineKnots(const QString& typed, int count) {
  QJsonArray knots;
  const QString text = typed.trimmed();
  if (text.isEmpty()) {
    const int degree = std::min(3, count - 1);
    const int inner = count - degree - 1;
    for (int k = 0; k <= degree; ++k) {
      knots.append(0.0);
    }
    for (int k = 1; k <= inner; ++k) {
      knots.append(static_cast<double>(k) / (inner + 1));
    }
    for (int k = 0; k <= degree; ++k) {
      knots.append(1.0);
    }
    return knots;
  }
  static const QRegularExpression separators(QStringLiteral("[,;\\s]+"));
  for (const QString& part : text.split(separators, Qt::SkipEmptyParts)) {
    bool ok = false;
    const double knot = part.toDouble(&ok);
    if (!ok) {
      return std::nullopt;
    }
    knots.append(knot);
  }
  return knots;
}

// An edge of a body where a fillet or a chamfer takes it (EdgeSides:
// its middle, direction, faces' normals and convexity there), placed as
// the view shows the body, with the body and the edge's two faces (the
// first and second face the model measures on).
struct EdgeAt {
  EdgeSides sides;
  std::shared_ptr<geometry::Shape> body;
  std::array<int, 2> faces{-1, -1};
};

std::optional<EdgeAt> edgeAt(const SelectionItem& item, const CommandContext& model) {
  if (item.kind != SelectKind::Edge) {
    return std::nullopt;
  }
  const auto body = model.bodyShape(item.owner);
  if (!body) {
    return std::nullopt;
  }
  const std::vector<int> found = body->find_edges(item.name.toStdString());
  if (found.empty()) {
    return std::nullopt;
  }
  const int index = found.front();
  const std::array<int, 2> faces = body->edge_faces(index);
  if (faces[0] < 0 || faces[1] < 0 || faces[0] == faces[1]) {
    return std::nullopt;
  }
  // Read in the body's own coordinates, where its solid classifies the
  // edge as convex or concave; then placed as the view shows the body.
  const std::optional<EdgeSides> sides =
      edgeSides(body->occt(), body->edge(index), body->face(faces[0]), body->face(faces[1]));
  if (!sides) {
    return std::nullopt;
  }
  const gp_Trsf placement = model.itemShape(item).Location().Transformation();
  return EdgeAt{placed(*sides, placement), body, faces};
}

// The size a drag of a set's handle steps by (Manipulator::scale,
// mitcad#1431): the shortest of its edges and of the faces next to them
// (shapeScale: a face's 4 area / perimeter), over the first 16 edges; 0
// when none is found. A fillet on a 10 mm cube's edge steps by 0.1 mm.
double edgesScale(const Selection& edges, const CommandContext& model) {
  constexpr int kMeasured = 16;
  double scale = 0.0;
  int measured = 0;
  const auto take = [&scale](double size) {
    if (size > 0.0 && (scale <= 0.0 || size < scale)) {
      scale = size;
    }
  };
  for (const SelectionItem& item : edges) {
    if (item.kind != SelectKind::Edge) {
      continue;
    }
    if (measured++ >= kMeasured) {
      break;
    }
    const auto body = model.bodyShape(item.owner);
    if (!body) {
      continue;
    }
    // The first edge the name gives, as the handle stands on.
    const std::vector<int> found = body->find_edges(item.name.toStdString());
    if (found.empty()) {
      continue;
    }
    take(shapeScale(body->edge(found.front())));
    for (const int face : body->edge_faces(found.front())) {
      if (face >= 0) {
        take(shapeScale(body->face(face)));
      }
    }
  }
  return scale;
}

// A chamfer set's handle for its Distance (`second` false) or its Distance
// 2 (mitcad#534), on the set's first edge that can take it: Distance at
// the middle of the bevel for an equal distance, else along the face the
// model measures the first distance on (the reference face, Flip);
// Distance 2 along the other face, for two distances only.
std::optional<Manipulator> chamferHandle(const CommandState& row, const CommandContext& model, bool second) {
  const QString type = row.choice(QStringLiteral("type"));
  // A set of all edges has no picked edge to show it on (mitcad#793).
  if ((second && type != QStringLiteral("two_distances")) || row.checked(QStringLiteral("all_edges"))) {
    return std::nullopt;
  }
  const bool equal = type == QStringLiteral("equal_distance");
  const Selection& reference = row.items(QStringLiteral("reference"));
  const bool flip = row.checked(QStringLiteral("flip"));
  for (const SelectionItem& item : row.items(QStringLiteral("edges"))) {
    const auto at = edgeAt(item, model);
    if (!at) {
      continue;
    }
    std::optional<EdgeArrow> shown;
    if (equal) {
      shown = equalChamferArrow(at->sides);
    } else {
      bool on[2] = {false, false};
      if (!reference.isEmpty() && reference.first().owner == item.owner) {
        const std::string name = reference.first().name.toStdString();
        for (std::size_t k = 0; k < 2; ++k) {
          on[k] = geometry::face_matches(at->body->face_names(at->faces[k]), name);
        }
      }
      const std::optional<int> side = measuredSide(!reference.isEmpty(), on[0], on[1], flip);
      if (!side) {
        continue;
      }
      shown = faceDistanceArrow(at->sides, second ? 1 - *side : *side);
    }
    if (shown) {
      Manipulator handle = arrow(shown->origin, shown->direction);
      handle.factor = shown->factor;
      handle.scale = edgesScale(row.items(QStringLiteral("edges")), model);
      return handle;
    }
  }
  return std::nullopt;
}

// A set of every edge of its body (`all_edges`, mitcad#793): the set's
// All Edges is on; it names the body instead of edges.
bool allEdges(const CommandState& row) { return row.checked(QStringLiteral("all_edges")); }
bool pickedEdges(const CommandState& row) { return !allEdges(row); }

// A set's All Edges check, and the body it then works on.
InputDef allEdgesInput(const QString& tip) {
  return checkInput(QStringLiteral("all_edges"), QObject::tr("All Edges"), false).withTooltip(tip);
}
InputDef allEdgesBody() {
  return selectionInput(QStringLiteral("body"), QObject::tr("Body"), SelectKind::Body, 1, 1)
      .withTooltip(QObject::tr("The body whose every edge the set takes"))
      .withVisible(allEdges);
}

// Every edge and face of the sets (or, for a set of all edges, its body),
// for the body they must share.
Selection allItems(const CommandState& state) {
  Selection items;
  for (int i = 0; i < state.rows(kSets); ++i) {
    items += allEdges(state.row(kSets, i)) ? state.items(rowKey(i, "body")) : state.items(rowKey(i, "edges"));
  }
  return items;
}

// A set of all edges is the feature's only set (the model's rule).
std::optional<Built> allEdgesAlone(const CommandState& state) {
  const int count = state.rows(kSets);
  for (int i = 0; i < count && count > 1; ++i) {
    if (allEdges(state.row(kSets, i))) {
      return Built::failure(QObject::tr("All Edges takes every edge of the body: remove the other sets."),
                            rowKey(i, "all_edges"));
    }
  }
  return std::nullopt;
}

// Whether the panel's first set is of all edges (for the timeline's text).
bool firstAllEdges(const CommandState& state) {
  return state.rows(kSets) > 0 && allEdges(state.row(kSets, 0));
}

// Edges and faces of a set of the model's definition.
Selection setItems(const QJsonObject& set, const QString& body) {
  Selection items;
  for (const QJsonValue& edge : set.value(QStringLiteral("edges")).toArray()) {
    items.append({SelectKind::Edge, body, edge.toString(), QString()});
  }
  for (const QJsonValue& face : set.value(QStringLiteral("faces")).toArray()) {
    items.append({SelectKind::Face, body, face.toString(), QString()});
  }
  return items;
}

// The set's edges and faces, or `all_edges` for a set of all edges.
void addEdges(QJsonObject& set, const CommandState& row) {
  if (allEdges(row)) {
    set.insert(QStringLiteral("all_edges"), true);
    return;
  }
  const Selection& items = row.items(QStringLiteral("edges"));
  const QJsonArray edges = names(items, SelectKind::Edge);
  const QJsonArray faces = names(items, SelectKind::Face);
  if (!edges.isEmpty()) {
    set.insert(QStringLiteral("edges"), edges);
  }
  if (!faces.isEmpty()) {
    set.insert(QStringLiteral("faces"), faces);
  }
}

// A set of the definition into the panel's row: its edges and faces, or
// All Edges on with the feature's body.
void loadEdges(CommandState& state, int row, const QJsonObject& set, const QString& body) {
  if (set.value(QStringLiteral("all_edges")).toBool()) {
    state.setChecked(rowKey(row, "all_edges"), true);
    state.setItems(rowKey(row, "body"), {{SelectKind::Body, body, QString(), QString()}});
    return;
  }
  state.setItems(rowKey(row, "edges"), setItems(set, body));
}

// The sets of a definition: its short form is one set.
QJsonArray setsOf(const QJsonObject& feature) {
  if (feature.contains(kSets)) {
    return feature.value(kSets).toArray();
  }
  QJsonObject set = feature;
  set.remove(QStringLiteral("type"));
  set.remove(QStringLiteral("body"));
  set.remove(QStringLiteral("uid"));
  return {set};
}

bool sameChain(const QJsonArray& sets) {
  bool first = true;
  bool chain = true;
  for (const QJsonValue& value : sets) {
    const bool on = value.toObject().value(QStringLiteral("tangent_chain")).toBool(true);
    if (!first && on != chain) {
      return false;
    }
    chain = on;
    first = false;
  }
  return true;
}

const Choices kFilletSizes = {{QStringLiteral("constant"), QStringLiteral("Constant")},
                              {QStringLiteral("chord_length"), QStringLiteral("Chord Length")},
                              {QStringLiteral("variable"), QStringLiteral("Variable")},
                              {QStringLiteral("asymmetric"), QStringLiteral("Asymmetric")},
                              {QStringLiteral("spline"), QStringLiteral("Spline")},
                              {QStringLiteral("full_round"), QStringLiteral("Full Round")}};

// Full Round (mitcad#2279): the radius follows from the face it rounds
// away, so a set of it has no Radius, and it takes that face's two edges,
// not the face.
bool fullRound(const CommandState& row) { return row.choice(QStringLiteral("size")) == QStringLiteral("full_round"); }

QString fullRoundTakesEdges() {
  return QObject::tr("Full Round takes the two edges of the face it rounds away, not the face: pick the face's "
                     "two edges.");
}

// Whether any set of the panel has a radius of this type (mid radii are
// the variable ones', control radii the spline ones').
bool anySize(const CommandState& state, const QString& size) {
  for (int i = 0; i < state.rows(kSets); ++i) {
    if (state.row(kSets, i).choice(QStringLiteral("size")) == size) {
      return true;
    }
  }
  return false;
}

bool anyVariable(const CommandState& state) { return anySize(state, QStringLiteral("variable")); }

bool anySpline(const CommandState& state) { return anySize(state, QStringLiteral("spline")); }

const Choices kChamferTypes = {{QStringLiteral("equal_distance"), QStringLiteral("Equal Distance")},
                               {QStringLiteral("two_distances"), QStringLiteral("Two Distances")},
                               {QStringLiteral("distance_angle"), QStringLiteral("Distance and Angle")}};

} // namespace

// ---------------------------------------------------------------------------
// Fillet

CommandDef filletCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.fillet");
  def.name = QObject::tr("Fillet");
  def.icon = QStringLiteral("fillet");
  def.tooltip = QObject::tr("Rounds edges, each set of edges with its own radius");
  def.shortcut = QKeySequence(Qt::Key_F);
  def.keywords = {QStringLiteral("round"), QStringLiteral("blend")};
  def.featureType = QStringLiteral("fillet");
  def.describeBefore = true; // the edges are gone after it
  const auto sizeIs = [](const QString& size) {
    return [size](const CommandState& row) { return row.choice(QStringLiteral("size")) == size; };
  };
  def.inputs = {
      listInput(kSets, QObject::tr("Edge Sets"),
                {allEdgesInput(QObject::tr("Round every edge of a body, also the edges a later edit adds")),
                 selectionInput(QStringLiteral("edges"), QObject::tr("Edges"),
                                SelectKind::Edge | SelectKind::Face, 1, 0)
                     .withTooltip(QObject::tr("Edges or a face's edges; Full Round: the face's two edges; click "
                                              "an item to remove it"))
                     .withHoverChain(SelectKind::Edge, QStringLiteral("tangent_chain"))
                     // Full Round takes edges only (mitcad#2279).
                     .withPickRule(nullptr,
                                   [](const SelectionItem& item, const CommandState& row) {
                                     return !(fullRound(row) && item.kind == SelectKind::Face);
                                   })
                     .withRefusedHint([](const SelectionItem& item, const CommandState&) {
                       return item.kind == SelectKind::Face ? fullRoundTakesEdges() : QString();
                     })
                     .withVisible(pickedEdges),
                 allEdgesBody(),
                 choiceInput(QStringLiteral("size"), QObject::tr("Radius Type"), kFilletSizes,
                             QStringLiteral("constant")),
                 valueInput(QStringLiteral("radius"), QObject::tr("Radius"), ValueKind::Length,
                            QStringLiteral("2 mm"))
                     .withStepMinimum(0.0, true) // a radius above 0 (mitcad#1254)
                     .withTooltip(QObject::tr("The radius, or the first size of the other radius types"))
                     // A new fillet starts with the radius of the last one (mitcad#1028).
                     .withRemembered()
                     // Full Round works its radius out (mitcad#2279).
                     .withVisible([](const CommandState& row) { return !fullRound(row); })
                     .withManipulator([](const CommandState& row,
                                         const CommandContext& model) -> std::optional<Manipulator> {
                       // A handle at the middle of the rounding of the set's
                       // first edge, for a constant radius.
                       if (row.choice(QStringLiteral("size")) != QStringLiteral("constant") || allEdges(row)) {
                         return std::nullopt;
                       }
                       for (const SelectionItem& item : row.items(QStringLiteral("edges"))) {
                         const auto at = edgeAt(item, model);
                         if (!at) {
                           continue;
                         }
                         if (const std::optional<EdgeArrow> shown = filletArrow(at->sides)) {
                           Manipulator handle = arrow(shown->origin, shown->direction);
                           handle.factor = shown->factor;
                           handle.scale = edgesScale(row.items(QStringLiteral("edges")), model);
                           return handle;
                         }
                       }
                       return std::nullopt;
                     }),
                 valueInput(QStringLiteral("end"), QObject::tr("End Radius"), ValueKind::Length,
                            QStringLiteral("4 mm")).withStepMinimum(0.0, true)
                     .withTooltip(QObject::tr("The radius at the end; a spline's last control radius"))
                     .withVisible([](const CommandState& row) {
                       const QString size = row.choice(QStringLiteral("size"));
                       return size == QStringLiteral("variable") || size == QStringLiteral("spline");
                     }),
                 textInput(QStringLiteral("knots"), QObject::tr("Knots"))
                     .withTooltip(QObject::tr("The spline's knots from 0 to 1; empty: evenly spaced"))
                     .withVisible(sizeIs(QStringLiteral("spline"))),
                 valueInput(QStringLiteral("distance2"), QObject::tr("Distance 2"), ValueKind::Length,
                            QStringLiteral("4 mm")).withStepMinimum(0.0, true)
                     .withTooltip(QObject::tr("The distance from the edge on the other face"))
                     .withVisible(sizeIs(QStringLiteral("asymmetric"))),
                 selectionInput(QStringLiteral("start_vertex"), QObject::tr("Start Vertex"),
                                SelectKind::Vertex, 0, 1)
                     .withTooltip(QObject::tr("The end of the chain where the radius starts; "
                                              "none: the geometry kernel's start"))
                     .withVisible([](const CommandState& row) {
                       const QString size = row.choice(QStringLiteral("size"));
                       return size == QStringLiteral("variable") || size == QStringLiteral("spline");
                     }),
                 selectionInput(QStringLiteral("reference"), QObject::tr("Reference Face"),
                                SelectKind::Face, 0, 1)
                     .withTooltip(QObject::tr("The face the first distance is measured on; none: "
                                              "each edge's first face"))
                     .withVisible(sizeIs(QStringLiteral("asymmetric"))),
                 flipInput(QStringLiteral("flip"), QObject::tr("Flip"))
                     .withTooltip(QObject::tr("Measure the first distance on the other face"))
                     .withVisible(sizeIs(QStringLiteral("asymmetric"))),
                 choiceInput(QStringLiteral("continuity"), QObject::tr("Continuity"),
                             {{QStringLiteral("tangent"), QStringLiteral("Tangent (G1)")},
                              {QStringLiteral("curvature"), QStringLiteral("Curvature (G2)")}},
                             QStringLiteral("tangent"))
                     // A full round is a circle's arc (G1).
                     .withVisible([](const CommandState& row) { return !fullRound(row); }),
                 valueInput(QStringLiteral("weight"), QObject::tr("Tangency Weight"), ValueKind::Unitless,
                            QStringLiteral("1"))
                     .withTooltip(QObject::tr("0.1 to 2: how far the curvature continuous rounding "
                                              "keeps to the faces before it turns"))
                     .withVisible([](const CommandState& row) {
                       return !fullRound(row) &&
                              row.choice(QStringLiteral("continuity")) == QStringLiteral("curvature");
                     })},
                QObject::tr("Add Set"), QObject::tr("Set %1")),
      listInput(kMid, QObject::tr("Mid Radii"),
                {valueInput(QStringLiteral("set"), QObject::tr("Set"), ValueKind::Unitless, QStringLiteral("1"))
                     .withTooltip(QObject::tr("The variable set (1, 2, …) the radius is on")),
                 valueInput(QStringLiteral("position"), QObject::tr("Position"), ValueKind::Unitless,
                            QStringLiteral("0.5"))
                     .withTooltip(QObject::tr("Where along the chain, 0 at the start to 1 at the end")),
                 valueInput(QStringLiteral("radius"), QObject::tr("Radius"), ValueKind::Length,
                            QStringLiteral("3 mm")).withStepMinimum(0.0, true)},
                QObject::tr("Add Mid Radius"), QObject::tr("Mid %1"), 0)
          .withVisible(anyVariable),
      // The control radii of spline sets between the first and the last
      // (mitcad#464), in order along the chain.
      listInput(kControl, QObject::tr("Control Radii"),
                {valueInput(QStringLiteral("set"), QObject::tr("Set"), ValueKind::Unitless, QStringLiteral("1"))
                     .withTooltip(QObject::tr("The spline set (1, 2, …) the control radius is of")),
                 valueInput(QStringLiteral("radius"), QObject::tr("Radius"), ValueKind::Length,
                            QStringLiteral("3 mm")).withStepMinimum(0.0, true)},
                QObject::tr("Add Control Radius"), QObject::tr("Control %1"), 0)
          .withVisible(anySpline),
      checkInput(QStringLiteral("tangent_chain"), QObject::tr("Tangent Chain"), true)
          .withTooltip(QObject::tr("Also round the edges that continue the selected ones smoothly")),
      choiceInput(QStringLiteral("corner"), QObject::tr("Corner Type"),
                  {{QStringLiteral("rolling_ball"), QStringLiteral("Rolling Ball")},
                   {QStringLiteral("setback"), QStringLiteral("Setback")}},
                  QStringLiteral("rolling_ball")),
  };
  def.enabled = [&context] { return hasBodies(context); };

  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const QString body = oneBody(allItems(state), result, rowKey(0, "edges"));
    if (body.isEmpty()) {
      return result;
    }
    if (const std::optional<Built> alone = allEdgesAlone(state)) {
      return *alone;
    }
    const bool chain = state.checked(QStringLiteral("tangent_chain"));
    const bool rolling = state.choice(QStringLiteral("corner")) == QStringLiteral("rolling_ball");
    result.def = {{QStringLiteral("type"), QStringLiteral("fillet")}, {QStringLiteral("body"), body}};
    const int count = state.rows(kSets);
    const CommandState first = state.row(kSets, 0);
    if (count == 1 && chain && rolling && !allEdges(first) &&
        first.choice(QStringLiteral("size")) == QStringLiteral("constant") &&
        first.choice(QStringLiteral("continuity")) == QStringLiteral("tangent") &&
        names(first.items(QStringLiteral("edges")), SelectKind::Face).isEmpty()) {
      // The short form: one set of edges with a constant radius.
      result.def.insert(QStringLiteral("edges"), names(first.items(QStringLiteral("edges")), SelectKind::Edge));
      result.def.insert(QStringLiteral("radius"), first.expression(QStringLiteral("radius")));
      return result;
    }
    // The mid radii of each variable set, by position.
    std::vector<std::vector<std::pair<double, QJsonValue>>> mids(static_cast<std::size_t>(count));
    for (int k = 0; k < state.rows(kMid) && anyVariable(state); ++k) {
      const double set = state.value(midKey(k, "set"));
      const double position = state.value(midKey(k, "position"));
      const int index = static_cast<int>(std::lround(set)) - 1;
      if (!(std::abs(set - std::round(set)) < 1e-9) || index < 0 || index >= count ||
          state.row(kSets, index).choice(QStringLiteral("size")) != QStringLiteral("variable")) {
        return Built::failure(QObject::tr("Mid radius %1 is not on a variable set.").arg(k + 1), midKey(k, "set"));
      }
      if (!(position > 0.0 && position < 1.0)) {
        return Built::failure(QObject::tr("Mid radius %1: the position must be between 0 and 1.").arg(k + 1),
                              midKey(k, "position"));
      }
      mids[static_cast<std::size_t>(index)].emplace_back(position, state.expression(midKey(k, "radius")));
    }
    // The control radii of each spline set between its first and its last.
    std::vector<QJsonArray> controls(static_cast<std::size_t>(count));
    for (int k = 0; k < state.rows(kControl) && anySpline(state); ++k) {
      const double set = state.value(controlKey(k, "set"));
      const int index = static_cast<int>(std::lround(set)) - 1;
      if (!(std::abs(set - std::round(set)) < 1e-9) || index < 0 || index >= count ||
          state.row(kSets, index).choice(QStringLiteral("size")) != QStringLiteral("spline")) {
        return Built::failure(QObject::tr("Control radius %1 is not on a spline set.").arg(k + 1),
                              controlKey(k, "set"));
      }
      controls[static_cast<std::size_t>(index)].append(state.expression(controlKey(k, "radius")));
    }
    QJsonArray sets;
    for (int i = 0; i < count; ++i) {
      const CommandState row = state.row(kSets, i);
      const QString size = row.choice(QStringLiteral("size"));
      QJsonObject sizeDef{{QStringLiteral("type"), size}};
      QJsonObject set;
      if (allEdges(row) && (size == QStringLiteral("variable") || size == QStringLiteral("spline")) &&
          !row.items(QStringLiteral("start_vertex")).isEmpty()) {
        return Built::failure(QObject::tr("All Edges has no start vertex: clear it."), rowKey(i, "start_vertex"));
      }
      if (size == QStringLiteral("full_round")) {
        // No values: the radius follows from the face (mitcad#2279).
        if (allEdges(row) || !names(row.items(QStringLiteral("edges")), SelectKind::Face).isEmpty()) {
          return Built::failure(fullRoundTakesEdges(), rowKey(i, "edges"));
        }
        // One edge picked: the panel waits for the face's other edge
        // instead of failing a preview of one (asks, not a failure).
        if (names(row.items(QStringLiteral("edges")), SelectKind::Edge).size() < 2) {
          return Built::asking(QObject::tr("Full Round: pick the face's edge on its other side too."),
                               rowKey(i, "edges"));
        }
      } else if (size == QStringLiteral("chord_length")) {
        sizeDef.insert(QStringLiteral("length"), row.expression(QStringLiteral("radius")));
      } else if (size == QStringLiteral("variable")) {
        sizeDef.insert(QStringLiteral("start"), row.expression(QStringLiteral("radius")));
        sizeDef.insert(QStringLiteral("end"), row.expression(QStringLiteral("end")));
        const Selection& vertex = row.items(QStringLiteral("start_vertex"));
        if (!vertex.isEmpty()) {
          if (vertex.first().owner != body) {
            return Built::failure(QObject::tr("The start vertex must be on the filleted body."),
                                  rowKey(i, "start_vertex"));
          }
          sizeDef.insert(QStringLiteral("start_vertex"), vertex.first().name);
        }
        auto& mid = mids[static_cast<std::size_t>(i)];
        std::sort(mid.begin(), mid.end(), [](const auto& a, const auto& b) { return a.first < b.first; });
        QJsonArray radii;
        for (const auto& [position, radius] : mid) {
          radii.append(QJsonObject{{QStringLiteral("position"), position}, {QStringLiteral("radius"), radius}});
        }
        if (!radii.isEmpty()) {
          sizeDef.insert(QStringLiteral("mid"), radii);
        }
      } else if (size == QStringLiteral("spline")) {
        QJsonArray radii{row.expression(QStringLiteral("radius"))};
        for (const QJsonValue& radius : controls[static_cast<std::size_t>(i)]) {
          radii.append(radius);
        }
        radii.append(row.expression(QStringLiteral("end")));
        const std::optional<QJsonArray> knots =
            splineKnots(row.text(QStringLiteral("knots")), static_cast<int>(radii.size()));
        if (!knots) {
          return Built::failure(QObject::tr("The knots must be numbers."), rowKey(i, "knots"));
        }
        sizeDef.insert(QStringLiteral("knots"), *knots);
        sizeDef.insert(QStringLiteral("radii"), radii);
        const Selection& vertex = row.items(QStringLiteral("start_vertex"));
        if (!vertex.isEmpty()) {
          if (vertex.first().owner != body) {
            return Built::failure(QObject::tr("The start vertex must be on the filleted body."),
                                  rowKey(i, "start_vertex"));
          }
          sizeDef.insert(QStringLiteral("start_vertex"), vertex.first().name);
        }
      } else if (size == QStringLiteral("asymmetric")) {
        sizeDef.insert(QStringLiteral("distance1"), row.expression(QStringLiteral("radius")));
        sizeDef.insert(QStringLiteral("distance2"), row.expression(QStringLiteral("distance2")));
        if (row.checked(QStringLiteral("flip"))) {
          sizeDef.insert(QStringLiteral("flip"), true);
        }
        const Selection& reference = row.items(QStringLiteral("reference"));
        if (!reference.isEmpty()) {
          if (reference.first().owner != body) {
            return Built::failure(QObject::tr("The reference face must be on the filleted body."),
                                  rowKey(i, "reference"));
          }
          set.insert(QStringLiteral("reference_face"), reference.first().name);
        }
      } else {
        sizeDef.insert(QStringLiteral("radius"), row.expression(QStringLiteral("radius")));
      }
      set.insert(QStringLiteral("size"), sizeDef);
      addEdges(set, row);
      if (!chain) {
        set.insert(QStringLiteral("tangent_chain"), false);
      }
      if (!fullRound(row) && row.choice(QStringLiteral("continuity")) == QStringLiteral("curvature")) {
        set.insert(QStringLiteral("continuity"), QStringLiteral("curvature"));
        set.insert(QStringLiteral("tangency_weight"), row.expression(QStringLiteral("weight")));
      }
      sets.append(set);
    }
    result.def.insert(kSets, sets);
    if (!rolling) {
      result.def.insert(QStringLiteral("rolling_ball_corners"), false);
    }
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    const QString body = str(feature, "body");
    const QJsonArray sets = setsOf(feature);
    state.setRows(kSets, static_cast<int>(sets.size()));
    int mids = 0;
    int controls = 0;
    for (int i = 0; i < sets.size(); ++i) {
      const QJsonObject set = sets[i].toObject();
      loadEdges(state, i, set, body);
      if (!feature.contains(kSets)) {
        state.setChoice(rowKey(i, "size"), QStringLiteral("constant"));
        loadValue(state, rowKey(i, "radius"), feature.value(QStringLiteral("radius")), feature, context);
        continue;
      }
      const QJsonObject size = set.value(QStringLiteral("size")).toObject();
      const QString type = str(size, "type");
      state.setChoice(rowKey(i, "size"), type);
      loadValue(state, rowKey(i, "radius"),
                type == QStringLiteral("chord_length") ? size.value(QStringLiteral("length"))
                : type == QStringLiteral("variable")   ? size.value(QStringLiteral("start"))
                : type == QStringLiteral("asymmetric") ? size.value(QStringLiteral("distance1"))
                                                       : size.value(QStringLiteral("radius")),
                feature, context);
      loadValue(state, rowKey(i, "end"), size.value(QStringLiteral("end")), feature, context);
      loadValue(state, rowKey(i, "distance2"), size.value(QStringLiteral("distance2")), feature, context);
      if (type == QStringLiteral("spline")) {
        // The first control radius is the set's radius, the last its end
        // radius, the others control radii of the panel's list.
        const QJsonArray radii = size.value(QStringLiteral("radii")).toArray();
        QStringList knots;
        for (const QJsonValue& knot : size.value(QStringLiteral("knots")).toArray()) {
          knots.append(number(knot.toDouble()));
        }
        state.setText(rowKey(i, "knots"), knots.join(QStringLiteral(", ")));
        if (!radii.isEmpty()) {
          loadValue(state, rowKey(i, "radius"), radii.first(), feature, context);
          loadValue(state, rowKey(i, "end"), radii.last(), feature, context);
        }
        for (int k = 1; k + 1 < radii.size(); ++k) {
          state.setRows(kControl, controls + 1);
          state.setText(controlKey(controls, "set"), QString::number(i + 1));
          loadValue(state, controlKey(controls, "radius"), radii[k], feature, context);
          ++controls;
        }
      }
      state.setChecked(rowKey(i, "flip"), size.value(QStringLiteral("flip")).toBool());
      if (set.contains(QStringLiteral("reference_face"))) {
        state.setItems(rowKey(i, "reference"),
                       {itemOf(QJsonObject{{QStringLiteral("body"), body},
                                           {QStringLiteral("face"), set.value(QStringLiteral("reference_face"))}},
                               context)});
      }
      if (size.contains(QStringLiteral("start_vertex"))) {
        state.setItems(rowKey(i, "start_vertex"),
                       {{SelectKind::Vertex, body, str(size, "start_vertex"), QStringLiteral("point")}});
      }
      for (const QJsonValue& value : size.value(QStringLiteral("mid")).toArray()) {
        const QJsonObject mid = value.toObject();
        state.setRows(kMid, mids + 1);
        state.setText(midKey(mids, "set"), QString::number(i + 1));
        state.setText(midKey(mids, "position"), number(mid.value(QStringLiteral("position")).toDouble()));
        loadValue(state, midKey(mids, "radius"), mid.value(QStringLiteral("radius")), feature, context);
        ++mids;
      }
      state.setChoice(rowKey(i, "continuity"),
                      set.value(QStringLiteral("continuity")).toString(QStringLiteral("tangent")));
      if (set.contains(QStringLiteral("tangency_weight"))) {
        loadCount(state, rowKey(i, "weight"), set.value(QStringLiteral("tangency_weight")), feature, context);
      }
      state.setChecked(QStringLiteral("tangent_chain"), set.value(QStringLiteral("tangent_chain")).toBool(true));
    }
    state.setChoice(QStringLiteral("corner"),
                    feature.value(QStringLiteral("rolling_ball_corners")).toBool(true)
                        ? QStringLiteral("rolling_ball")
                        : QStringLiteral("setback"));
  };

  // The tangent chain is one setting for all sets in the panel.
  def.canEdit = [](const QJsonObject& feature) { return sameChain(setsOf(feature)); };

  def.describe = [](const CommandState& state, const CommandContext& context) {
    if (firstAllEdges(state)) {
      return QStringLiteral("Added fillet on all edges");
    }
    const int count = state.rows(kSets);
    // A full round (mitcad#2279) is named so where every set is one.
    bool rounds = count > 0;
    for (int i = 0; i < count; ++i) {
      rounds = rounds && fullRound(state.row(kSets, i));
    }
    return QStringLiteral("Added %1 on %2 edge(s)%3")
        .arg(rounds ? QStringLiteral("full round fillet") : QStringLiteral("fillet"))
        .arg(context.edgeCount(allItems(state)))
        .arg(count > 1 ? QStringLiteral(" in %1 sets").arg(count) : QString());
  };
  return def;
}

// ---------------------------------------------------------------------------
// Chamfer

CommandDef chamferCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.chamfer");
  def.name = QObject::tr("Chamfer");
  def.icon = QStringLiteral("chamfer");
  def.tooltip = QObject::tr("Bevels edges, each set of edges with its own size");
  def.keywords = {QStringLiteral("bevel")};
  def.featureType = QStringLiteral("chamfer");
  def.describeBefore = true;
  const auto typeIs = [](const char* type) {
    return [value = QString::fromLatin1(type)](const CommandState& row) {
      return row.choice(QStringLiteral("type")) == value;
    };
  };
  const auto notEqual = [](const CommandState& row) {
    return row.choice(QStringLiteral("type")) != QStringLiteral("equal_distance");
  };
  def.inputs = {
      listInput(kSets, QObject::tr("Edge Sets"),
                {allEdgesInput(QObject::tr("Bevel every edge of a body, also the edges a later edit adds")),
                 selectionInput(QStringLiteral("edges"), QObject::tr("Edges"),
                                SelectKind::Edge | SelectKind::Face, 1, 0)
                     .withTooltip(QObject::tr("Bevel edges or a face's edges; click a selected item to remove it; ✕ clears the input"))
                     .withHoverChain(SelectKind::Edge, QStringLiteral("tangent_chain"))
                     .withVisible(pickedEdges),
                 allEdgesBody(),
                 choiceInput(QStringLiteral("type"), QObject::tr("Chamfer Type"), kChamferTypes,
                             QStringLiteral("equal_distance")),
                 // Each distance has an arrow in the view (mitcad#534); the
                 // angle is typed only.
                 valueInput(QStringLiteral("distance"), QObject::tr("Distance"), ValueKind::Length,
                            QStringLiteral("1 mm")).withStepMinimum(0.0, true)
                     .withTooltip(QObject::tr("From the edge on each face, or on the reference face; "
                                              "its arrow drags it"))
                     // A new chamfer starts with the sizes of the last one (mitcad#1028).
                     .withRemembered()
                     .withManipulator([](const CommandState& row, const CommandContext& model) {
                       return chamferHandle(row, model, false);
                     }),
                 valueInput(QStringLiteral("distance2"), QObject::tr("Distance 2"), ValueKind::Length,
                            QStringLiteral("1 mm")).withStepMinimum(0.0, true)
                     .withTooltip(QObject::tr("From the edge on the other face; its arrow along that "
                                              "face drags it"))
                     .withRemembered()
                     .withVisible(typeIs("two_distances"))
                     .withManipulator([](const CommandState& row, const CommandContext& model) {
                       return chamferHandle(row, model, true);
                     }),
                 valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle,
                            QStringLiteral("45 deg")).withStepMinimum(0.0, true)
                     .withTooltip(QObject::tr("The bevel's angle from the reference face"))
                     .withRemembered()
                     .withVisible(typeIs("distance_angle")),
                 selectionInput(QStringLiteral("reference"), QObject::tr("Reference Face"),
                                SelectKind::Face, 0, 1)
                     .withTooltip(QObject::tr("The face the first distance and the angle are "
                                              "measured on; none: each edge's first face"))
                     .withVisible(notEqual),
                 flipInput(QStringLiteral("flip"), QObject::tr("Flip"))
                     .withTooltip(QObject::tr("Measure the first distance on the other face"))
                     .withVisible(notEqual)},
                QObject::tr("Add Set"), QObject::tr("Set %1")),
      checkInput(QStringLiteral("tangent_chain"), QObject::tr("Tangent Chain"), true),
      choiceInput(QStringLiteral("corner"), QObject::tr("Corner Type"),
                  {{QStringLiteral("chamfer"), QStringLiteral("Chamfer")},
                   {QStringLiteral("miter"), QStringLiteral("Miter")},
                   {QStringLiteral("blend"), QStringLiteral("Blend")}},
                  QStringLiteral("chamfer"))
          .withTooltip(QObject::tr("How three bevelled edges meet: a corner face, a miter or a smooth blend")),
  };
  def.enabled = [&context] { return hasBodies(context); };

  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const QString body = oneBody(allItems(state), result, rowKey(0, "edges"));
    if (body.isEmpty()) {
      return result;
    }
    if (const std::optional<Built> alone = allEdgesAlone(state)) {
      return *alone;
    }
    const bool chain = state.checked(QStringLiteral("tangent_chain"));
    const QString corner = state.choice(QStringLiteral("corner"));
    result.def = {{QStringLiteral("type"), QStringLiteral("chamfer")}, {QStringLiteral("body"), body}};
    const int count = state.rows(kSets);
    QJsonArray sets;
    for (int i = 0; i < count; ++i) {
      const CommandState row = state.row(kSets, i);
      const QString type = row.choice(QStringLiteral("type"));
      QJsonObject size{{QStringLiteral("type"), type}};
      if (type == QStringLiteral("two_distances")) {
        size.insert(QStringLiteral("distance1"), row.expression(QStringLiteral("distance")));
        size.insert(QStringLiteral("distance2"), row.expression(QStringLiteral("distance2")));
      } else {
        size.insert(QStringLiteral("distance"), row.expression(QStringLiteral("distance")));
        if (type == QStringLiteral("distance_angle")) {
          size.insert(QStringLiteral("angle"), row.expression(QStringLiteral("angle")));
        }
      }
      const bool equal = type == QStringLiteral("equal_distance");
      QJsonObject set{{QStringLiteral("size"), size}};
      addEdges(set, row);
      const Selection& reference = row.items(QStringLiteral("reference"));
      if (!equal && !reference.isEmpty()) {
        if (reference.first().owner != body) {
          return Built::failure(QObject::tr("The reference face must be on the chamfered body."),
                                rowKey(i, "reference"));
        }
        set.insert(QStringLiteral("reference_face"), reference.first().name);
      }
      if (!equal && row.checked(QStringLiteral("flip"))) {
        set.insert(QStringLiteral("flip"), true);
      }
      if (!chain) {
        set.insert(QStringLiteral("tangent_chain"), false);
      }
      sets.append(set);
    }
    const QJsonObject only = sets.first().toObject();
    if (count == 1 && chain && corner == QStringLiteral("chamfer") && !only.contains(QStringLiteral("faces")) &&
        !only.contains(QStringLiteral("reference_face")) && !only.contains(QStringLiteral("all_edges"))) {
      // The short form: one set of edges.
      result.def.insert(QStringLiteral("edges"), only.value(QStringLiteral("edges")));
      result.def.insert(QStringLiteral("size"), only.value(QStringLiteral("size")));
      if (only.value(QStringLiteral("flip")).toBool()) {
        result.def.insert(QStringLiteral("flip"), true);
      }
      return result;
    }
    result.def.insert(kSets, sets);
    if (corner != QStringLiteral("chamfer")) {
      result.def.insert(QStringLiteral("corner"), corner);
    }
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    const QString body = str(feature, "body");
    const QJsonArray sets = setsOf(feature);
    state.setRows(kSets, static_cast<int>(sets.size()));
    for (int i = 0; i < sets.size(); ++i) {
      const QJsonObject set = sets[i].toObject();
      loadEdges(state, i, set, body);
      const QJsonObject size = set.value(QStringLiteral("size")).toObject();
      state.setChoice(rowKey(i, "type"), str(size, "type"));
      loadValue(state, rowKey(i, "distance"),
                size.contains(QStringLiteral("distance1")) ? size.value(QStringLiteral("distance1"))
                                                          : size.value(QStringLiteral("distance")),
                feature, context);
      loadValue(state, rowKey(i, "distance2"), size.value(QStringLiteral("distance2")), feature, context);
      loadValue(state, rowKey(i, "angle"), size.value(QStringLiteral("angle")), feature, context);
      state.setChecked(rowKey(i, "flip"), set.value(QStringLiteral("flip")).toBool());
      if (set.contains(QStringLiteral("reference_face"))) {
        state.setItems(rowKey(i, "reference"),
                       {itemOf(QJsonObject{{QStringLiteral("body"), body},
                                           {QStringLiteral("face"), set.value(QStringLiteral("reference_face"))}},
                               context)});
      }
      state.setChecked(QStringLiteral("tangent_chain"), set.value(QStringLiteral("tangent_chain")).toBool(true));
    }
    state.setChoice(QStringLiteral("corner"), feature.value(QStringLiteral("corner")).toString(QStringLiteral("chamfer")));
  };

  def.canEdit = [](const QJsonObject& feature) { return sameChain(setsOf(feature)); };

  def.describe = [](const CommandState& state, const CommandContext& context) {
    if (firstAllEdges(state)) {
      return QStringLiteral("Added chamfer (%1) on all edges")
          .arg(label(kChamferTypes, state.choice(rowKey(0, "type"))));
    }
    const int count = state.rows(kSets);
    return QStringLiteral("Added chamfer (%1) on %2 edge(s)%3")
        .arg(label(kChamferTypes, state.choice(rowKey(0, "type"))))
        .arg(context.edgeCount(allItems(state)))
        .arg(count > 1 ? QStringLiteral(" in %1 sets").arg(count) : QString());
  };
  return def;
}

// ---------------------------------------------------------------------------
// Press Pull

namespace {

const QString kTarget = QStringLiteral("target");
const QString kContinue = QStringLiteral("continue");
const QString kTaperType = QStringLiteral("taper_type");
// The face_run answer for the first picked face (mitcad#1850), kept in the
// state: how its walls continue and the feature whose end it is.
const QString kRun = QStringLiteral("run");
// Set while a press_pull feature is edited: Offset and a feature's own
// edit are other features, not offered then.
const QString kEditingContinued = QStringLiteral("editing_continued");

// How Press Pull continues a face (mitcad#1850): Offset is offset_face,
// Straight and Along the Walls are press_pull; "feature" (offered where the
// face is a feature's moving end) opens that feature's edit instead.
const Choices kContinueTypes = {{QStringLiteral("offset"), QStringLiteral("Offset")},
                                {QStringLiteral("straight"), QStringLiteral("Straight")},
                                {QStringLiteral("along"), QStringLiteral("Along the Walls (Face Turns)")}};
const Choices kPullTapers = {{QStringLiteral("none"), QStringLiteral("None")},
                             {QStringLiteral("draft"), QStringLiteral("Draft")},
                             {QStringLiteral("scale"), QStringLiteral("Scale")}};

bool onFaces(const CommandState& state) {
  const Selection& items = state.items(kTarget);
  return !items.isEmpty() && items.first().kind == SelectKind::Face;
}

// Straight or Along the Walls: a press_pull, which takes a taper.
bool continued(const CommandState& state) {
  const QString how = state.choice(kContinue);
  return onFaces(state) && (how == QStringLiteral("straight") || how == QStringLiteral("along"));
}

QJsonObject faceRun(const SelectionItem& face, const CommandContext& context) {
  try {
    return context.queryObject({{QStringLiteral("query"), QStringLiteral("face_run")},
                                {QStringLiteral("body"), face.owner},
                                {QStringLiteral("face"), face.name}});
  } catch (const std::exception&) {
    return {}; // not flat, or not a face of this document: no run
  }
}

// Reads the run of the first picked face into the state; a feature's
// edit that no longer applies goes back to Offset.
void readRun(CommandState& state, const CommandContext& context) {
  const Selection& items = state.items(kTarget);
  const QJsonObject run =
      !items.isEmpty() && items.first().kind == SelectKind::Face ? faceRun(items.first(), context) : QJsonObject();
  state.setObject(kRun, run);
  if (state.choice(kContinue) == QStringLiteral("feature") &&
      run.value(QStringLiteral("feature_end")).toObject().isEmpty()) {
    state.setChoice(kContinue, QStringLiteral("offset"));
  }
}

// The handle of Along the Walls (mitcad#1850): a ring about the walls'
// axis through the face's area centre, its value the arc length, for a
// turn; an arrow along the walls for a straight run. None for the others.
std::optional<Manipulator> runHandle(const CommandState& state, const CommandContext& model) {
  const Selection& items = state.items(kTarget);
  if (state.choice(kContinue) != QStringLiteral("along") || items.isEmpty()) {
    return std::nullopt;
  }
  const QJsonObject run = state.object(kRun);
  const QJsonObject along = run.value(QStringLiteral("along")).toObject();
  const QString kind = along.value(QStringLiteral("kind")).toString();
  if (run.isEmpty() || (kind != QStringLiteral("turn") && kind != QStringLiteral("straight"))) {
    return std::nullopt;
  }
  // The query answers in the body's coordinates; the view places it.
  const TopoDS_Shape placed = model.itemShape(items.first());
  const gp_Trsf place = placed.IsNull() ? gp_Trsf() : placed.Location().Transformation();
  const gp_Pnt centre = pointOf(run.value(QStringLiteral("center")));
  if (kind == QStringLiteral("straight")) {
    Manipulator handle = arrow(centre.Transformed(place),
                               gp_Dir(vectorOf(along.value(QStringLiteral("direction")))).Transformed(place));
    handle.scale = itemsScale({items.first()}, model);
    return handle;
  }
  const gp_Pnt origin = pointOf(along.value(QStringLiteral("origin")));
  const gp_Dir axis(vectorOf(along.value(QStringLiteral("axis"))));
  const double radius = along.value(QStringLiteral("radius")).toDouble();
  if (!(radius > 1e-9)) {
    return std::nullopt;
  }
  const gp_Pnt foot = origin.Translated(gp_Vec(axis) * gp_Vec(origin, centre).Dot(gp_Vec(axis)));
  Manipulator handle = ring(foot.Transformed(place), axis.Transformed(place));
  handle.reference = gp_Dir(gp_Vec(foot, centre)).Transformed(place);
  handle.radius = radius;
  handle.factor = 1.0 / radius;
  return handle;
}

} // namespace

CommandDef pressPullCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.press_pull");
  def.name = QObject::tr("Press Pull");
  def.icon = QStringLiteral("press-pull");
  def.tooltip = QObject::tr("Offsets faces, rounds edges or extrudes profiles");
  def.shortcut = QKeySequence(Qt::Key_Q);
  def.keywords = {QStringLiteral("push"), QStringLiteral("pull"), QStringLiteral("offset face")};
  // A face continued Straight or Along the walls is a press_pull of its
  // own, edited here (mitcad#1850).
  def.editsAlso = {QStringLiteral("press_pull")};
  InputDef endScale =
      valueInput(QStringLiteral("end_scale"), QObject::tr("End Scale"), ValueKind::Unitless, QStringLiteral("1"))
          .withTooltip(QObject::tr("The end's size to the face's: 1.1 is 110 %"))
          .withVisible([](const CommandState& state) {
            return continued(state) && state.choice(kTaperType) == QStringLiteral("scale");
          });
  // The angle the scale makes at the outline's farthest point.
  endScale.rowNote = [](const CommandState& state, const CommandContext&) {
    const double reach = state.object(kRun).value(QStringLiteral("reach")).toDouble();
    const double scale = state.value(QStringLiteral("end_scale"));
    const double distance = std::abs(state.value(QStringLiteral("distance")));
    if (!(reach > 0.0) || !std::isfinite(scale) || !(distance > 0.0)) {
      return QString();
    }
    return QObject::tr("Angle at the outline's farthest point: %1°")
        .arg(degrees(std::atan(reach * (scale - 1.0) / distance)));
  };
  def.inputs = {
      selectionInput(kTarget, QObject::tr("Faces, Edges or Profiles"),
                     SelectKind::Face | SelectKind::Edge | SelectKind::Profile, 1, 0)
          .withTooltip(QObject::tr("Faces to offset, edges to fillet or profiles to extrude, one "
                                   "kind at a time"))
          .withOnPick([](const SelectionItem&, CommandState& state, const CommandContext& model) {
            readRun(state, model);
          }),
      choiceInput(kContinue, QObject::tr("Continue"), kContinueTypes, QStringLiteral("offset"))
          .withTooltip(QObject::tr("Offset moves the face; Straight runs it out unturned; Along the Walls "
                                   "turns it with them"))
          .withChoices([](const CommandState& state) {
            // Editing a press_pull: Straight or Along the Walls only.
            if (state.checked(kEditingContinued)) {
              return Choices{kContinueTypes[1], kContinueTypes[2]};
            }
            Choices options = kContinueTypes;
            const QJsonObject end = state.object(kRun).value(QStringLiteral("feature_end")).toObject();
            if (!end.isEmpty()) {
              options.append({QStringLiteral("feature"), QObject::tr("Edit %1 Instead").arg(str(end, "name"))});
            }
            return options;
          })
          .withVisible(onFaces),
      valueInput(QStringLiteral("distance"), QObject::tr("Distance"), ValueKind::Length,
                 QStringLiteral("5 mm"))
          .withTooltip(QObject::tr("Out of the material for faces, the radius for edges, the "
                                   "height for profiles"))
          .withManipulator([](const CommandState& state,
                              const CommandContext& model) -> std::optional<Manipulator> {
            const Selection& items = state.items(kTarget);
            if (items.isEmpty()) {
              return std::nullopt;
            }
            // Stepped by the profiles' or the face's size (mitcad#1431).
            if (items.first().kind == SelectKind::Profile) {
              const auto base = profileNormal(items, model);
              if (!base) {
                return std::nullopt;
              }
              Manipulator handle = arrow(base->first, base->second);
              handle.scale = itemsScale(items, model);
              return handle;
            }
            if (items.first().kind == SelectKind::Face) {
              // Along the walls the handle follows the run (mitcad#1850).
              if (std::optional<Manipulator> along = runHandle(state, model)) {
                return along;
              }
              const auto frame = frameOf(items.first(), model);
              const auto center = centerOf({items.first()}, model);
              if (frame && center) {
                Manipulator handle = arrow(*center, frame->normal);
                handle.scale = itemsScale({items.first()}, model);
                return handle;
              }
            }
            return std::nullopt;
          }),
      choiceInput(kTaperType, QObject::tr("Taper Type"), kPullTapers, QStringLiteral("none"))
          .withTooltip(QObject::tr("Draft leans each new wall; Scale grows or shrinks the whole outline"))
          .withVisible(continued),
      valueInput(QStringLiteral("taper_angle"), QObject::tr("Taper Angle"), ValueKind::Angle,
                 QStringLiteral("0 deg"))
          .withTooltip(QObject::tr("How far each new wall leans from its edge on the face, outward positive"))
          .withVisible([](const CommandState& state) {
            return continued(state) && state.choice(kTaperType) == QStringLiteral("draft");
          }),
      endScale,
  };
  def.enabled = [&context] { return hasBodies(context) || hasProfiles(context); };
  def.preselected = [](const Selection&, CommandState& state, const CommandContext& model) {
    readRun(state, model);
  };
  def.rolledBack = [](CommandState& state, const CommandContext& model) { readRun(state, model); };

  def.build = [](const CommandState& state, const CommandContext& context) {
    Built result;
    const Selection& items = state.items(kTarget);
    const SelectKind kind = items.first().kind;
    if (items.size() == 1 && kind == SelectKind::Face && items.first().name.contains(QStringLiteral(":fillet("))) {
      // A fillet's face: the fillet opens for its radius,
      // on the set that rounds the face's edge.
      const QString uid = items.first().creatingFeature();
      try {
        const QJsonObject feature = context.queryObject(
            {{QStringLiteral("query"), QStringLiteral("feature")}, {QStringLiteral("uid"), uid}});
        const QJsonObject fillet = feature.value(QStringLiteral("def")).toObject();
        if (str(fillet, "type") == QStringLiteral("fillet")) {
          const QString name = items.first().name;
          const int open = name.indexOf(QStringLiteral(":fillet(")) + 8;
          const QString edge = name.mid(open, name.lastIndexOf(QLatin1Char(')')) - open);
          const QJsonArray sets = setsOf(fillet);
          int set = 0;
          for (int i = 0; i < sets.size(); ++i) {
            if (sets[i].toObject().value(QStringLiteral("edges")).toArray().contains(edge)) {
              set = i;
            }
          }
          return Built::editing(uid, rowKey(set, "radius"));
        }
      } catch (const std::exception&) {
        // not a feature of this document: as any face
      }
    }
    for (const SelectionItem& item : items) {
      if (item.kind != kind) {
        return Built::failure(QObject::tr("Select faces, edges or profiles: one kind at a time."),
                              QStringLiteral("target"));
      }
    }
    const QString distance = state.expression(QStringLiteral("distance"));
    if (kind == SelectKind::Profile) {
      if (oneSketch(items, result, QStringLiteral("target")).isEmpty()) {
        return result;
      }
      // Join, which makes a new body where it touches nothing: Press Pull
      // picks the operation.
      result.def = {{QStringLiteral("type"), QStringLiteral("extrude")},
                    {QStringLiteral("profiles"), refsOf(items)},
                    {QStringLiteral("extent"), QJsonObject{{QStringLiteral("type"), QStringLiteral("distance")},
                                                           {QStringLiteral("distance"), distance}}},
                    {QStringLiteral("operation"), QStringLiteral("join")}};
      return result;
    }
    const QString how = state.choice(kContinue);
    if (kind == SelectKind::Face && how == QStringLiteral("feature")) {
      // The face is a feature's moving end (mitcad#1850): that feature's
      // own edit at the value that moves it.
      const QJsonObject end = faceRun(items.first(), context).value(QStringLiteral("feature_end")).toObject();
      if (items.size() != 1 || end.isEmpty()) {
        return Built::failure(QObject::tr("Pick one face that ends a feature to edit that feature."),
                              QStringLiteral("target"));
      }
      return Built::editing(str(end, "uid"), str(end, "input"));
    }
    const QString body = oneBody(items, result, QStringLiteral("target"));
    if (body.isEmpty()) {
      return result;
    }
    if (kind == SelectKind::Face && (how == QStringLiteral("straight") || how == QStringLiteral("along"))) {
      // Straight or Along the walls, tapered (mitcad#1850).
      result.def = {{QStringLiteral("type"), QStringLiteral("press_pull")},
                    {QStringLiteral("body"), body},
                    {QStringLiteral("faces"), names(items, SelectKind::Face)},
                    {QStringLiteral("continue"), how},
                    {QStringLiteral("distance"), distance}};
      const QString taper = state.choice(kTaperType);
      if (taper == QStringLiteral("draft")) {
        result.def.insert(QStringLiteral("taper_type"), taper);
        result.def.insert(QStringLiteral("taper_angle"), state.expression(QStringLiteral("taper_angle")));
      } else if (taper == QStringLiteral("scale")) {
        result.def.insert(QStringLiteral("taper_type"), taper);
        result.def.insert(QStringLiteral("end_scale"), state.expression(QStringLiteral("end_scale")));
      }
    } else if (kind == SelectKind::Face) {
      result.def = {{QStringLiteral("type"), QStringLiteral("offset_face")},
                    {QStringLiteral("body"), body},
                    {QStringLiteral("faces"), names(items, SelectKind::Face)},
                    {QStringLiteral("distance"), distance}};
    } else {
      result.def = {{QStringLiteral("type"), QStringLiteral("fillet")},
                    {QStringLiteral("body"), body},
                    {QStringLiteral("edges"), names(items, SelectKind::Edge)},
                    {QStringLiteral("radius"), distance}};
    }
    return result;
  };

  // A press_pull's panel (mitcad#1850): its faces, how it continues, its
  // distance and taper.
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& model) {
    Selection items;
    for (const QJsonValue& face : feature.value(QStringLiteral("faces")).toArray()) {
      items.append(itemOf(QJsonObject{{QStringLiteral("body"), feature.value(QStringLiteral("body"))},
                                      {QStringLiteral("face"), face}},
                          model));
    }
    state.setItems(kTarget, items);
    state.setChecked(kEditingContinued, true);
    state.setChoice(kContinue, feature.value(kContinue).toString(QStringLiteral("straight")));
    loadValue(state, QStringLiteral("distance"), feature.value(QStringLiteral("distance")), feature, model);
    state.setChoice(kTaperType, feature.value(kTaperType).toString(QStringLiteral("none")));
    if (feature.contains(QStringLiteral("taper_angle"))) {
      loadValue(state, QStringLiteral("taper_angle"), feature.value(QStringLiteral("taper_angle")), feature, model);
    }
    if (feature.contains(QStringLiteral("end_scale"))) {
      loadValue(state, QStringLiteral("end_scale"), feature.value(QStringLiteral("end_scale")), feature, model);
    }
  };

  def.describe = [](const CommandState& state, const CommandContext&) {
    const Selection& items = state.items(kTarget);
    const QString how = state.choice(kContinue);
    const QString what = items.first().kind == SelectKind::Face
                             ? (how == QStringLiteral("straight") ? QStringLiteral("press pull straight")
                                : how == QStringLiteral("along")  ? QStringLiteral("press pull along the walls")
                                                                  : QStringLiteral("offset face"))
                         : items.first().kind == SelectKind::Edge ? QStringLiteral("fillet")
                                                                  : QStringLiteral("extrude");
    return QStringLiteral("Press Pull made %1, distance %2")
        .arg(what, number(state.value(QStringLiteral("distance"))));
  };
  return def;
}

} // namespace mitcad::cmd
