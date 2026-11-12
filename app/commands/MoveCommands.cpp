// SPDX-License-Identifier: MIT
// Scale, Combine, Move/Copy and Align (commands.md, "move, align, scale",
// "combine"; Appearance is MaterialCommands.cpp's, mitcad#1059). Move/Copy's
// free move has handles: arrows along X, Y and Z and rings about them, at
// the middle of what moves.
#include <algorithm>
#include <cmath>
#include <functional>
#include <vector>

#include <QDebug>
#include <QHash>
#include <QJsonArray>
#include <QKeySequence>
#include <QStringList>

#include <gp_Ax1.hxx>
#include <gp_Trsf.hxx>

#include "../framework/Appearances.hpp"
#include "CommandFactories.hpp"
#include "../framework/ModelShapes.hpp"
#include "CommandSupport.hpp"
#include "MoveDragMotion.hpp"
#include "MoveFaceText.hpp"

namespace mitcad::cmd {
namespace {

std::function<bool(const CommandState&)> choiceIs(const QString& id, const QString& value) {
  return [id, value](const CommandState& state) { return state.choice(id) == value; };
}

const Choices kMoveTypes = {{QStringLiteral("free"), QStringLiteral("Free Move")},
                            {QStringLiteral("translate"), QStringLiteral("Translate")},
                            {QStringLiteral("rotate"), QStringLiteral("Rotate")},
                            {QStringLiteral("point_to_point"), QStringLiteral("Point to Point")},
                            {QStringLiteral("point_to_position"), QStringLiteral("Point to Position")}};

// The points of a move: vertices, construction and sketch points, and the
// centres of circles and arcs, given by their edges (the model takes a
// circular edge as its centre).
constexpr SelectFilter kMovePointKinds = kPointKinds | SelectKind::Edge;
bool isMovePoint(const SelectionItem& item) { return item.kind != SelectKind::Edge || isCircular(item); }

// What moves: bodies, or occurrences of components (picked in the browser,
// or through a body they place); Move Face's faces (it has no Move Object).
QString movedKey(const CommandState& state) {
  const QString key = state.choice(QStringLiteral("move_type"));
  return key.isEmpty() ? QStringLiteral("faces") : key;
}

// The middle the free move turns about: kept from an edit, else the middle
// of what moves.
std::optional<gp_Pnt> pivotOf(const CommandState& state, const CommandContext& context) {
  const QStringList parts = state.text(QStringLiteral("pivot")).split(QLatin1Char(','));
  if (parts.size() == 3) {
    return gp_Pnt(parts[0].toDouble(), parts[1].toDouble(), parts[2].toDouble());
  }
  return centerOf(state.items(movedKey(state)), context);
}

// An edited free move as it was loaded (mitcad#1583): the feature's
// matrix (`rows`) and the texts of the inputs that hold it (`texts`). While
// the texts are those, OK gives the model that matrix back as it was,
// not the one their rounded values make.
const QString kFreeLoaded = QStringLiteral("free_loaded");

// The texts of a free move's inputs, its hidden pivot's too.
QJsonObject freeTexts(const CommandState& state) {
  QJsonObject texts;
  for (const char* id : {"tx", "ty", "tz", "rx", "ry", "rz", "pivot"}) {
    texts.insert(QLatin1String(id), state.text(QLatin1String(id)));
  }
  return texts;
}

// The panel's free move from the model's matrix, split about `pivot`
// (freeMoveOf), and kept as loaded.
void setFreeMove(const QJsonArray& rows, const gp_Pnt& pivot, CommandState& state, const CommandContext& context) {
  const FreeMove move = freeMoveOf(rows, pivot);
  // The pivot in full: the handles sit at it moved by the distances.
  const auto full = [](double value) { return QString::number(value, 'g', 17); };
  state.setText(QStringLiteral("pivot"), QStringLiteral("%1,%2,%3").arg(full(pivot.X()), full(pivot.Y()), full(pivot.Z())));
  const auto deg = [](double radians) { return degrees(radians) + QStringLiteral(" deg"); };
  state.setText(QStringLiteral("tx"), lengthText(move.moved.X(), context));
  state.setText(QStringLiteral("ty"), lengthText(move.moved.Y(), context));
  state.setText(QStringLiteral("tz"), lengthText(move.moved.Z(), context));
  state.setText(QStringLiteral("rx"), deg(move.rx));
  state.setText(QStringLiteral("ry"), deg(move.ry));
  state.setText(QStringLiteral("rz"), deg(move.rz));
  state.setObject(kFreeLoaded, {{QStringLiteral("rows"), rows}, {QStringLiteral("texts"), freeTexts(state)}});
}

// An edited free move split again once the timeline is rolled back to the
// feature (CommandDef::rolledBack, mitcad#1583): `load` ran on the model
// after the feature, where what moves is already moved, so the pivot it
// took was the moved middle and the handles, at the pivot moved by the
// distances, counted the move twice. Now the pivot is the middle of what
// moves as the feature finds it, where the handles start; the matrix, and
// so the preview and the result, stay the same.
void splitFreeMoveRolledBack(CommandState& state, const CommandContext& context) {
  if (state.choice(QStringLiteral("type")) != QStringLiteral("free")) {
    return;
  }
  const QJsonArray rows = state.object(kFreeLoaded).value(QStringLiteral("rows")).toArray();
  if (rows.size() != 3) {
    return;
  }
  const auto pivot = centerOf(state.items(movedKey(state)), context);
  if (!pivot) {
    return;
  }
  setFreeMove(rows, *pivot, state, context);
  qDebug().noquote() << QStringLiteral("Free move pivot where the feature finds what moves: %1,%2,%3")
                            .arg(pivot->X(), 0, 'f', 6)
                            .arg(pivot->Y(), 0, 'f', 6)
                            .arg(pivot->Z(), 0, 'f', 6);
}

// The occurrence path ("O1/O4") of an occurrence given by its uid or path.
QString occurrencePath(const QString& occurrence, const CommandContext& context) {
  if (occurrence.contains(QLatin1Char('/'))) {
    return occurrence;
  }
  std::function<QString(const QJsonArray&, const QString&)> find = [&](const QJsonArray& tree, const QString& prefix) {
    for (const QJsonValue& value : tree) {
      const QJsonObject node = value.toObject();
      const QString uid = node.value(QStringLiteral("uid")).toString();
      const QString path = prefix.isEmpty() ? uid : prefix + QLatin1Char('/') + uid;
      if (uid == occurrence) {
        return path;
      }
      const QString found = find(node.value(QStringLiteral("children")).toArray(), path);
      if (!found.isEmpty()) {
        return found;
      }
    }
    return QString();
  };
  try {
    const QString path = find(context.queryObject({{QStringLiteral("query"), QStringLiteral("components")}})
                                  .value(QStringLiteral("occurrences"))
                                  .toArray(),
                              QString());
    return path.isEmpty() ? occurrence : path;
  } catch (const std::exception&) {
    return occurrence;
  }
}

// The component the occurrence path ("O1/O4") places; empty when the path
// leads nowhere.
QString placedComponent(const QString& path, const CommandContext& context) {
  std::function<QString(const QJsonArray&, const QString&)> find = [&](const QJsonArray& tree, const QString& prefix) {
    for (const QJsonValue& value : tree) {
      const QJsonObject node = value.toObject();
      const QString uid = node.value(QStringLiteral("uid")).toString();
      const QString here = prefix.isEmpty() ? uid : prefix + QLatin1Char('/') + uid;
      if (here == path) {
        return node.value(QStringLiteral("component")).toString();
      }
      if (path.startsWith(here + QLatin1Char('/'))) {
        return find(node.value(QStringLiteral("children")).toArray(), here);
      }
    }
    return QString();
  };
  try {
    return find(context.queryObject({{QStringLiteral("query"), QStringLiteral("components")}})
                    .value(QStringLiteral("occurrences"))
                    .toArray(),
                QString());
  } catch (const std::exception&) {
    return QString();
  }
}

// Whether the occurrence at the path ("O1/O4") is grounded.
bool groundedAt(const QString& path, const CommandContext& context) {
  std::function<bool(const QJsonArray&, const QString&)> find = [&](const QJsonArray& tree, const QString& prefix) {
    for (const QJsonValue& value : tree) {
      const QJsonObject node = value.toObject();
      const QString uid = node.value(QStringLiteral("uid")).toString();
      const QString here = prefix.isEmpty() ? uid : prefix + QLatin1Char('/') + uid;
      if (here == path) {
        return node.value(QStringLiteral("grounded")).toBool();
      }
      if (path.startsWith(here + QLatin1Char('/'))) {
        return find(node.value(QStringLiteral("children")).toArray(), here);
      }
    }
    return false;
  };
  try {
    return find(context.queryObject({{QStringLiteral("query"), QStringLiteral("components")}})
                    .value(QStringLiteral("occurrences"))
                    .toArray(),
                QString());
  } catch (const std::exception&) {
    return false;
  }
}

// The component a move works in, kept in its hidden input "component"
// (mitcad#699): the active one for a new move, the feature's own in an
// edit (loadTransform).
const QString kMoveComponent = QStringLiteral("component");

// The active component's uid (C0, the root's, when the model cannot say).
QString activeComponent(const CommandContext& context) {
  try {
    return context.queryObject({{QStringLiteral("query"), QStringLiteral("components")}})
        .value(QStringLiteral("active"))
        .toString(QStringLiteral("C0"));
  } catch (const std::exception&) {
    return QStringLiteral("C0");
  }
}

void initMoveComponent(CommandState& state, const CommandContext& context) {
  state.setText(kMoveComponent, activeComponent(context));
}

// --- A body of another component moves as its occurrence (mitcad#1196)

// The occurrence that moves for a body or face of another component than
// the active one (a BUILD part, any occurrence's body): on the item's
// occurrence path ("O1/O4"), the occurrence placed in the active component
// (a BUILD part's own in its host); the item's own occurrence when the
// path does not pass through the active component (the model then says
// why it cannot move it); empty for the active component's own geometry,
// which moves as bodies.
QString movedOccurrence(const QString& path, const QString& active, const CommandContext& context) {
  if (path.isEmpty()) {
    return QString();
  }
  const QString own = active.isEmpty() ? QStringLiteral("C0") : active;
  const QStringList steps = path.split(QLatin1Char('/'));
  for (qsizetype i = steps.size(); i >= 0; --i) {
    const QString component =
        i == 0 ? QStringLiteral("C0") : placedComponent(steps.mid(0, i).join(QLatin1Char('/')), context);
    if (component == own) {
      return i == steps.size() ? QString() : steps.mid(0, i + 1).join(QLatin1Char('/'));
    }
  }
  return path;
}

SelectionItem occurrenceItem(const QString& path) {
  SelectionItem item{SelectKind::Component, QString(), QString(), QString()};
  item.occurrence = path;
  return item;
}

void addOccurrence(Selection& components, const QString& path) {
  if (std::none_of(components.begin(), components.end(),
                   [&path](const SelectionItem& item) { return item.occurrence == path; })) {
    components.append(occurrenceItem(path));
  }
}

// A body of another component clicked while Move Object is Bodies: its
// occurrence goes to Components instead, and Move Object turns to
// Components (the panel then shows that input), so that the part moves
// instead of the model refusing a body of another component. Not beside
// bodies of the active component picked already: those move as bodies.
void takeComponentOf(const SelectionItem& item, CommandState& state, const CommandContext& context) {
  const QString occurrence = movedOccurrence(item.occurrence, state.text(kMoveComponent), context);
  if (occurrence.isEmpty()) {
    return;
  }
  const Selection& bodies = state.items(QStringLiteral("bodies"));
  if (std::any_of(bodies.begin(), bodies.end(), [&item](const SelectionItem& body) {
        return body.owner != item.owner || body.occurrence != item.occurrence;
      })) {
    return;
  }
  Selection components = state.items(QStringLiteral("components"));
  addOccurrence(components, occurrence);
  state.setItems(QStringLiteral("bodies"), {});
  state.setItems(QStringLiteral("components"), components);
  state.setChoice(QStringLiteral("move_type"), QStringLiteral("components"));
  qInfo().noquote() << QStringLiteral("Move/Copy: %1 is of another component than the active one: moving %2 "
                                      "(Move Object: Components)")
                           .arg(item.describe(), occurrence);
}

// The selection made before the command (the view's click on a placed
// part, its context menu, a browser row): when every body or face in it
// is of another component than the active one, their occurrences go to
// Components with the components selected, and Move Object is
// Components. Any other item keeps the selection as it is.
void takeComponentsOf(const Selection& selection, CommandState& state, const CommandContext& context) {
  const QString active = activeComponent(context);
  QStringList occurrences;
  for (const SelectionItem& item : selection) {
    if (item.kind == SelectKind::Component) {
      continue; // in Components already
    }
    if (item.kind != SelectKind::Body && item.kind != SelectKind::Face) {
      return;
    }
    const QString occurrence = movedOccurrence(item.occurrence, active, context);
    if (occurrence.isEmpty()) {
      return; // the active component's own geometry: as bodies
    }
    if (!occurrences.contains(occurrence)) {
      occurrences << occurrence;
    }
  }
  if (occurrences.isEmpty()) {
    return;
  }
  Selection components = state.items(QStringLiteral("components"));
  for (const QString& occurrence : occurrences) {
    addOccurrence(components, occurrence);
  }
  state.setItems(QStringLiteral("bodies"), {});
  state.setItems(QStringLiteral("components"), components);
  state.setChoice(QStringLiteral("move_type"), QStringLiteral("components"));
  qInfo().noquote() << QStringLiteral("Move/Copy: the selection is of another component than the active one: "
                                      "moving %1 (Move Object: Components)")
                           .arg(occurrences.join(QStringLiteral(", ")));
}

// A move's point or axis picked in an occurrence of another component than
// the move's (a vertex of another part): `<key>_occurrence` beside it, the
// occurrence's path, so that the model takes it where that occurrence
// places it (commands.md, "move, align, scale", mitcad#699). Geometry of
// the move's own component, or picked in no occurrence, goes as before.
void addPickedOccurrence(QJsonObject& transform, const QString& key, const SelectionItem& item,
                         const CommandState& state, const CommandContext& context) {
  if (item.occurrence.isEmpty()) {
    return;
  }
  const QString own = state.text(kMoveComponent).isEmpty() ? QStringLiteral("C0") : state.text(kMoveComponent);
  if (placedComponent(item.occurrence, context) == own) {
    return;
  }
  transform.insert(key + QStringLiteral("_occurrence"), item.occurrence);
}

// A definition's point or axis, in the occurrence it was picked in.
SelectionItem pickedItemOf(const QJsonObject& transform, const QString& key, const CommandContext& context) {
  SelectionItem item = itemOf(transform.value(key), context);
  const QJsonValue occurrence = transform.value(key + QStringLiteral("_occurrence"));
  if (occurrence.isString()) {
    item.occurrence = occurrence.toString();
  }
  return item;
}

// The names of the occurrences by their paths ("O1/O4" → "Screw:1").
QHash<QString, QString> occurrenceNames(const CommandContext& context) {
  QHash<QString, QString> names;
  std::function<void(const QJsonArray&, const QString&)> walk = [&](const QJsonArray& tree, const QString& prefix) {
    for (const QJsonValue& value : tree) {
      const QJsonObject node = value.toObject();
      const QString uid = node.value(QStringLiteral("uid")).toString();
      const QString path = prefix.isEmpty() ? uid : prefix + QLatin1Char('/') + uid;
      names.insert(path, node.value(QStringLiteral("name")).toString(path));
      walk(node.value(QStringLiteral("children")).toArray(), path);
    }
  };
  try {
    walk(context.queryObject({{QStringLiteral("query"), QStringLiteral("components")}})
             .value(QStringLiteral("occurrences"))
             .toArray(),
         QString());
  } catch (const std::exception&) {
  }
  return names;
}

// Combine's tools by name once one is picked in another occurrence than the
// target: each such tool by its occurrence (two screws of one part,
// "Screw:1, Screw:2"; with its body when the occurrence gives two tools),
// the others by their bodies (mitcad#544). Empty otherwise: the panel
// counts them.
QString combineToolsText(const Selection& tools, const CommandState& state, const CommandContext& context) {
  const Selection& targets = state.items(QStringLiteral("target"));
  const QString here = targets.isEmpty() ? QString() : targets.first().occurrence;
  const auto elsewhere = [&here](const SelectionItem& tool) {
    return !tool.occurrence.isEmpty() && tool.occurrence != here;
  };
  if (std::none_of(tools.begin(), tools.end(), elsewhere)) {
    return QString();
  }
  const QHash<QString, QString> occurrences = occurrenceNames(context);
  const auto bodyName = [&context](const QString& uid) {
    try {
      return context.bodyName(uid);
    } catch (const std::exception&) {
      return uid;
    }
  };
  QStringList names;
  for (const SelectionItem& tool : tools) {
    if (!elsewhere(tool)) {
      names << bodyName(tool.owner);
      continue;
    }
    const QString occurrence = occurrences.value(tool.occurrence, tool.occurrence);
    const bool shared = std::count_if(tools.begin(), tools.end(), [&tool](const SelectionItem& other) {
                          return other.occurrence == tool.occurrence;
                        }) > 1;
    names << (shared ? QStringLiteral("%1 (%2)").arg(bodyName(tool.owner), occurrence) : occurrence);
  }
  return names.join(QStringLiteral(", "));
}

// What a consumed tool of another component takes away besides itself
// (mitcad#561): it leaves its own component (mitcad#104), so it is gone
// from every occurrence of that component, also where it is not a tool:
// "Body1 leaves Part: it is also gone from Part:2." Empty with Keep Tools,
// or when every occurrence of it is a tool.
QString combineNote(const CommandState& state, const CommandContext& context) {
  if (state.checked(QStringLiteral("keep_tools"))) {
    return QString();
  }
  const Selection& targets = state.items(QStringLiteral("target"));
  const QString here = targets.isEmpty() ? QString() : targets.first().occurrence;
  const Selection& tools = state.items(QStringLiteral("tools"));
  const auto elsewhere = [&here](const SelectionItem& tool) {
    return !tool.occurrence.isEmpty() && tool.occurrence != here;
  };
  if (std::none_of(tools.begin(), tools.end(), elsewhere)) {
    return QString();
  }
  // Every occurrence by its path of uids, with its name and component.
  struct Occurrence {
    QString path;
    QString name;
    QString component;
  };
  std::vector<Occurrence> occurrences;
  QHash<QString, QString> componentNames;
  std::function<void(const QJsonArray&, const QString&)> walk = [&](const QJsonArray& tree, const QString& prefix) {
    for (const QJsonValue& value : tree) {
      const QJsonObject node = value.toObject();
      const QString uid = node.value(QStringLiteral("uid")).toString();
      const QString path = prefix.isEmpty() ? uid : prefix + QLatin1Char('/') + uid;
      occurrences.push_back({path, node.value(QStringLiteral("name")).toString(path), str(node, "component")});
      walk(node.value(QStringLiteral("children")).toArray(), path);
    }
  };
  try {
    const QJsonObject answer = context.queryObject({{QStringLiteral("query"), QStringLiteral("components")}});
    for (const QJsonValue& value : answer.value(QStringLiteral("components")).toArray()) {
      const QJsonObject component = value.toObject();
      componentNames.insert(str(component, "uid"), str(component, "name"));
    }
    walk(answer.value(QStringLiteral("occurrences")).toArray(), QString());
  } catch (const std::exception&) {
    return QString();
  }
  const auto bodyName = [&context](const QString& uid) {
    try {
      return context.bodyName(uid);
    } catch (const std::exception&) {
      return uid;
    }
  };
  QStringList lines;
  QStringList told; // body@component
  for (const SelectionItem& tool : tools) {
    if (!elsewhere(tool)) {
      continue;
    }
    const auto at = std::find_if(occurrences.begin(), occurrences.end(),
                                 [&tool](const Occurrence& o) { return o.path == tool.occurrence; });
    if (at == occurrences.end() || at->component.isEmpty()) {
      continue;
    }
    const QString component = at->component;
    const QString key = tool.owner + QLatin1Char('@') + component;
    if (told.contains(key)) {
      continue;
    }
    told << key;
    QStringList others;
    for (const Occurrence& occurrence : occurrences) {
      const bool picked = std::any_of(tools.begin(), tools.end(), [&](const SelectionItem& other) {
        return other.owner == tool.owner && other.occurrence == occurrence.path;
      });
      if (occurrence.component == component && !picked) {
        others << occurrence.name;
      }
    }
    if (!others.isEmpty()) {
      lines << QObject::tr("%1 leaves %2: it is also gone from %3.")
                   .arg(bodyName(tool.owner), componentNames.value(component, component),
                        others.join(QStringLiteral(", ")));
    }
  }
  return lines.join(QLatin1Char('\n'));
}

// A free move's handles: an arrow along a model axis or a ring about it,
// in the axis' color (mitcad#1581), the arrows reaching beyond the rings
// (mitcad#1707, Manipulator::large). All of them meet at the moved middle
// (mitcad#1580): the rings turn about it, and each arrow starts from it
// without its own distance, so that its knob is there too and the arrows
// of the other axes go along with what moves; none while a distance is no
// number yet (a minus sign typed alone).
std::function<std::optional<Manipulator>(const CommandState&, const CommandContext&)>
freeHandle(int axis, bool turn) {
  return [axis, turn](const CommandState& state, const CommandContext& context) -> std::optional<Manipulator> {
    const auto pivot = pivotOf(state, context);
    if (!pivot) {
      return std::nullopt;
    }
    const gp_Dir direction(axis == 0 ? 1 : 0, axis == 1 ? 1 : 0, axis == 2 ? 1 : 0);
    const std::optional<gp_Pnt> at = turn ? freeMovedMiddle(state, *pivot) : freeArrowOrigin(state, *pivot, axis);
    if (!at) {
      return std::nullopt;
    }
    Manipulator handle = turn ? ring(*at, direction) : arrow(*at, direction);
    handle.color = axis == 0 ? ColorRole::AxisX : axis == 1 ? ColorRole::AxisY : ColorRole::AxisZ;
    handle.large = true;
    return handle;
  };
}

std::function<std::optional<Manipulator>(const CommandState&, const CommandContext&)> translateHandle(int axis) {
  return [axis](const CommandState& state, const CommandContext& context) -> std::optional<Manipulator> {
    const auto center = centerOf(state.items(movedKey(state)), context);
    if (!center) {
      return std::nullopt;
    }
    return arrow(*center, gp_Dir(axis == 0 ? 1 : 0, axis == 1 ? 1 : 0, axis == 2 ? 1 : 0));
  };
}

gp_Trsf moveFrame(const CommandState& state, const CommandContext& context);

// Rotate's axis as the model turned the shown preview about it (mitcad#838,
// keptRotateAxis): its ring runs about that axis, so that the ring's sense,
// a drag's proxy and the result agree (a circle's own axis may point the
// other way than the model's). Move/Copy's and Move Face's (mitcad#854).
const QString kRotateBasis = QStringLiteral("rotate_basis");

// A preview's report came (CommandDef::previewReported, Move/Copy's and
// Move Face's): Rotate's reported axis kept with the axis input it is of
// (rotateBasisToKeep). Nothing to compute again.
bool keepRotateBasis(QJsonObject& report, CommandState& state) {
  const Selection& axis = state.items(QStringLiteral("axis"));
  if (axis.isEmpty()) {
    return false;
  }
  const QJsonObject kept = rotateBasisToKeep(report, state.choice(QStringLiteral("type")), axis.first().reference(),
                                             axis.first().occurrence);
  if (!kept.isEmpty()) {
    state.setObject(kRotateBasis, kept);
  }
  return false;
}

// Point to Position's handles (mitcad#838): an arrow along each axis of the
// move's component, from the plane of the other two to the position, where
// the picked point goes; none until the point is picked.
std::function<std::optional<Manipulator>(const CommandState&, const CommandContext&)> positionHandle(int axis) {
  return [axis](const CommandState& state, const CommandContext& context) -> std::optional<Manipulator> {
    if (state.items(QStringLiteral("point")).isEmpty()) {
      return std::nullopt;
    }
    gp_XYZ from(state.value(QStringLiteral("px")), state.value(QStringLiteral("py")), state.value(QStringLiteral("pz")));
    if (!std::isfinite(from.X()) || !std::isfinite(from.Y()) || !std::isfinite(from.Z())) {
      return std::nullopt;
    }
    from.SetCoord(axis + 1, 0.0);
    const gp_Trsf frame = moveFrame(state, context);
    const gp_Dir direction(axis == 0 ? 1 : 0, axis == 1 ? 1 : 0, axis == 2 ? 1 : 0);
    return arrow(gp_Pnt(from).Transformed(frame), direction.Transformed(frame));
  };
}

// The model's transform of a move.
QJsonObject transformOf(const CommandState& state, const CommandContext& context, Built& error) {
  const QString type = state.choice(QStringLiteral("type"));
  if (type == QStringLiteral("free")) {
    const auto pivot = pivotOf(state, context);
    if (!pivot) {
      error = Built::failure(QObject::tr("Select what moves."), movedKey(state));
      return {};
    }
    // An edit's inputs as loaded: the feature's own matrix, unchanged by
    // the rounding of the values the panel shows (mitcad#1583).
    const QJsonObject loaded = state.object(kFreeLoaded);
    if (!loaded.isEmpty() && loaded.value(QStringLiteral("texts")).toObject() == freeTexts(state)) {
      return {{QStringLiteral("type"), type}, {QStringLiteral("matrix"), loaded.value(QStringLiteral("rows"))}};
    }
    // Turn about the pivot, then move: p -> R (p - c) + c + t.
    FreeMove move;
    move.moved = gp_XYZ(state.value(QStringLiteral("tx")), state.value(QStringLiteral("ty")),
                        state.value(QStringLiteral("tz")));
    move.rx = state.value(QStringLiteral("rx"));
    move.ry = state.value(QStringLiteral("ry"));
    move.rz = state.value(QStringLiteral("rz"));
    return {{QStringLiteral("type"), type}, {QStringLiteral("matrix"), freeMatrixOf(move, *pivot)}};
  }
  if (type == QStringLiteral("translate")) {
    return {{QStringLiteral("type"), QStringLiteral("translate_xyz")},
            {QStringLiteral("x"), state.expression(QStringLiteral("x"))},
            {QStringLiteral("y"), state.expression(QStringLiteral("y"))},
            {QStringLiteral("z"), state.expression(QStringLiteral("z"))}};
  }
  // Points and axes go with the occurrences they were picked in.
  const auto picked = [&state, &context](QJsonObject transform, const QString& key, const SelectionItem& item) {
    transform.insert(key, item.reference());
    addPickedOccurrence(transform, key, item, state, context);
    return transform;
  };
  if (type == QStringLiteral("along")) {
    // Move Face's Translate Along (mitcad#531).
    const QJsonObject along{{QStringLiteral("type"), QStringLiteral("translate_along")},
                            {QStringLiteral("distance"), state.expression(QStringLiteral("distance"))}};
    return picked(along, QStringLiteral("axis"), state.items(QStringLiteral("along_axis")).first());
  }
  if (type == QStringLiteral("rotate")) {
    const QJsonObject rotate{{QStringLiteral("type"), type},
                             {QStringLiteral("angle"), state.expression(QStringLiteral("angle"))}};
    return picked(rotate, QStringLiteral("axis"), state.items(QStringLiteral("axis")).first());
  }
  if (type == QStringLiteral("point_to_point")) {
    const QJsonObject points{{QStringLiteral("type"), type}};
    return picked(picked(points, QStringLiteral("from"), state.items(QStringLiteral("from")).first()),
                  QStringLiteral("to"), state.items(QStringLiteral("to")).first());
  }
  const QJsonObject position{{QStringLiteral("type"), type},
                             {QStringLiteral("x"), state.expression(QStringLiteral("px"))},
                             {QStringLiteral("y"), state.expression(QStringLiteral("py"))},
                             {QStringLiteral("z"), state.expression(QStringLiteral("pz"))}};
  return picked(position, QStringLiteral("point"), state.items(QStringLiteral("point")).first());
}

bool loadTransform(const QJsonObject& transform, CommandState& state, const QJsonObject& feature,
                   const CommandContext& context) {
  // The component the edited move works in (mitcad#699).
  if (feature.contains(QStringLiteral("uid"))) {
    try {
      state.setText(kMoveComponent, context
                                        .queryObject({{QStringLiteral("query"), QStringLiteral("feature")},
                                                      {QStringLiteral("uid"), feature.value(QStringLiteral("uid"))}})
                                        .value(QStringLiteral("component"))
                                        .toString(QStringLiteral("C0")));
    } catch (const std::exception&) {
    }
  }
  const QString type = str(transform, "type");
  if (type == QStringLiteral("free")) {
    state.setChoice(QStringLiteral("type"), type);
    // About the middle of what moves in the model at the marker. When the
    // feature is before the marker, what moves is already moved there: the
    // edit rolls the timeline back to the feature next and splits the
    // matrix again about the middle the feature finds (mitcad#1583,
    // splitFreeMoveRolledBack).
    const auto pivot = centerOf(state.items(movedKey(state)), context);
    setFreeMove(transform.value(QStringLiteral("matrix")).toArray(), pivot ? *pivot : gp_Pnt(0, 0, 0), state,
                context);
    return true;
  }
  if (type == QStringLiteral("translate_xyz")) {
    state.setChoice(QStringLiteral("type"), QStringLiteral("translate"));
    for (const char* axis : {"x", "y", "z"}) {
      loadValue(state, QString::fromLatin1(axis), transform.value(QLatin1String(axis)), feature, context);
    }
    return true;
  }
  if (type == QStringLiteral("rotate")) {
    state.setChoice(QStringLiteral("type"), type);
    state.setItems(QStringLiteral("axis"), {pickedItemOf(transform, QStringLiteral("axis"), context)});
    loadValue(state, QStringLiteral("angle"), transform.value(QStringLiteral("angle")), feature, context);
    return true;
  }
  if (type == QStringLiteral("point_to_point")) {
    state.setChoice(QStringLiteral("type"), type);
    state.setItems(QStringLiteral("from"), {pickedItemOf(transform, QStringLiteral("from"), context)});
    state.setItems(QStringLiteral("to"), {pickedItemOf(transform, QStringLiteral("to"), context)});
    return true;
  }
  if (type == QStringLiteral("point_to_position")) {
    state.setChoice(QStringLiteral("type"), type);
    state.setItems(QStringLiteral("point"), {pickedItemOf(transform, QStringLiteral("point"), context)});
    loadValue(state, QStringLiteral("px"), transform.value(QStringLiteral("x")), feature, context);
    loadValue(state, QStringLiteral("py"), transform.value(QStringLiteral("y")), feature, context);
    loadValue(state, QStringLiteral("pz"), transform.value(QStringLiteral("z")), feature, context);
    return true;
  }
  if (type == QStringLiteral("translate_along")) {
    // Move Face's Translate Along (Move/Copy has no inputs for it).
    state.setChoice(QStringLiteral("type"), QStringLiteral("along"));
    state.setItems(QStringLiteral("along_axis"), {pickedItemOf(transform, QStringLiteral("axis"), context)});
    loadValue(state, QStringLiteral("distance"), transform.value(QStringLiteral("distance")), feature, context);
    return true;
  }
  return false;
}

// Whether the panel can hold the motion; `along`: it has Translate Along
// (Move Face, not Move/Copy).
bool transformEditable(const QJsonObject& transform, const CommandContext& context, bool along = false) {
  const QString type = str(transform, "type");
  if (type == QStringLiteral("free") || type == QStringLiteral("translate_xyz")) {
    return true;
  }
  if (type == QStringLiteral("translate_along")) {
    return along && isShowable(transform.value(QStringLiteral("axis")), context);
  }
  if (type == QStringLiteral("rotate")) {
    return isShowable(transform.value(QStringLiteral("axis")), context);
  }
  if (type == QStringLiteral("point_to_point")) {
    return isShowable(transform.value(QStringLiteral("from")), context) &&
           isShowable(transform.value(QStringLiteral("to")), context);
  }
  if (type == QStringLiteral("point_to_position")) {
    return isShowable(transform.value(QStringLiteral("point")), context);
  }
  return false;
}

// Align's from and to: a point, an axis or a plane.
QJsonObject alignRef(const SelectionItem& item) {
  if (isPointLike(item)) {
    return {{QStringLiteral("type"), QStringLiteral("point")}, {QStringLiteral("point"), item.reference()}};
  }
  if (isPlanar(item)) {
    return {{QStringLiteral("type"), QStringLiteral("plane")}, {QStringLiteral("plane"), item.reference()}};
  }
  return {{QStringLiteral("type"), QStringLiteral("axis")}, {QStringLiteral("axis"), item.reference()}};
}

SelectionItem alignItem(const QJsonObject& ref, const CommandContext& context) {
  return itemOf(ref.value(str(ref, "type")), context);
}

// The motion's inputs, Move/Copy's and Move Face's (mitcad#531): Move Type
// of `types` and each type's inputs; `along` adds Translate Along's
// Direction and Distance (Move Face).
QVector<InputDef> motionInputs(const Choices& types, bool along) {
  const auto type = [](const char* kind) { return choiceIs(QStringLiteral("type"), QString::fromLatin1(kind)); };
  QVector<InputDef> inputs;
  inputs << choiceInput(QStringLiteral("type"), QObject::tr("Move Type"), types, QStringLiteral("free"))
         // Free Move
         << valueInput(QStringLiteral("tx"), QObject::tr("X Distance"), ValueKind::Length, QStringLiteral("0 mm"))
                .withVisible(type("free"))
                .withManipulator(freeHandle(0, false))
         << valueInput(QStringLiteral("ty"), QObject::tr("Y Distance"), ValueKind::Length, QStringLiteral("0 mm"))
                .withVisible(type("free"))
                .withManipulator(freeHandle(1, false))
         << valueInput(QStringLiteral("tz"), QObject::tr("Z Distance"), ValueKind::Length, QStringLiteral("0 mm"))
                .withVisible(type("free"))
                .withManipulator(freeHandle(2, false))
         << valueInput(QStringLiteral("rx"), QObject::tr("X Angle"), ValueKind::Angle, QStringLiteral("0 deg"))
                .withVisible(type("free"))
                .withManipulator(freeHandle(0, true))
         << valueInput(QStringLiteral("ry"), QObject::tr("Y Angle"), ValueKind::Angle, QStringLiteral("0 deg"))
                .withVisible(type("free"))
                .withManipulator(freeHandle(1, true))
         << valueInput(QStringLiteral("rz"), QObject::tr("Z Angle"), ValueKind::Angle, QStringLiteral("0 deg"))
                .withVisible(type("free"))
                .withManipulator(freeHandle(2, true))
         // Translate
         << valueInput(QStringLiteral("x"), QObject::tr("X Distance"), ValueKind::Length, QStringLiteral("0 mm"))
                .withVisible(type("translate"))
                .withManipulator(translateHandle(0))
         << valueInput(QStringLiteral("y"), QObject::tr("Y Distance"), ValueKind::Length, QStringLiteral("0 mm"))
                .withVisible(type("translate"))
                .withManipulator(translateHandle(1))
         << valueInput(QStringLiteral("z"), QObject::tr("Z Distance"), ValueKind::Length, QStringLiteral("0 mm"))
                .withVisible(type("translate"))
                .withManipulator(translateHandle(2));
  if (along) {
    // Translate Along: a distance along an axis, an edge or a face's axis;
    // 0 at the start, as every distance of the motion (mitcad#1163):
    // nothing moves until a distance is typed or the arrow dragged.
    inputs << selectionInput(QStringLiteral("along_axis"), QObject::tr("Direction"), kAxisKinds, 1, 1)
                  .withAccepts(isAxial)
                  .withVisible(type("along"))
           << valueInput(QStringLiteral("distance"), QObject::tr("Distance"), ValueKind::Length, QStringLiteral("0 mm"))
                  .withVisible(type("along"))
                  .withManipulator([](const CommandState& state, const CommandContext& model) -> std::optional<Manipulator> {
                    const Selection& axis = state.items(QStringLiteral("along_axis"));
                    const auto center = centerOf(state.items(movedKey(state)), model);
                    if (axis.isEmpty() || !center) {
                      return std::nullopt;
                    }
                    const auto line = axisOf(axis.first(), model);
                    if (!line) {
                      return std::nullopt;
                    }
                    return arrow(*center, line->second);
                  });
  }
  // Rotate, Point to Point, Point to Position
  inputs
      << selectionInput(QStringLiteral("axis"), QObject::tr("Axis"), kAxisKinds, 1, 1)
             .withAccepts(isAxial)
             .withVisible(type("rotate"))
      << valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("90 deg"))
             .withTooltip(QObject::tr("Right-handed about the axis"))
             .withVisible(type("rotate"))
             .withManipulator([](const CommandState& state, const CommandContext& model) -> std::optional<Manipulator> {
               const Selection& axis = state.items(QStringLiteral("axis"));
               const auto center = centerOf(state.items(movedKey(state)), model);
               if (axis.isEmpty() || !center) {
                 return std::nullopt;
               }
               // The axis the model turns about once a preview reported it
               // (mitcad#838; Move Face's too, mitcad#854), the picked
               // geometry's own until then.
               const SelectionItem& picked = axis.first();
               const QJsonObject kept = state.object(kRotateBasis);
               const gp_Trsf frame = kept.isEmpty() ? gp_Trsf() : moveFrame(state, model);
               const auto line = rotateRingAxis(
                   kept, picked.reference(), picked.occurrence, frame,
                   [&picked, &model]() -> std::optional<gp_Ax1> {
                     const auto own = axisOf(picked, model);
                     if (!own) {
                       return std::nullopt;
                     }
                     return gp_Ax1(own->first, own->second);
                   },
                   *center);
               if (!line) {
                 return std::nullopt;
               }
               const gp_Pnt foot = line->Location();
               Manipulator m = ring(foot, line->Direction());
               if (foot.Distance(*center) > 1e-9) {
                 m.reference = gp_Dir(gp_Vec(foot, *center));
               }
               return m;
             })
      // Point to Point
      << selectionInput(QStringLiteral("from"), QObject::tr("Origin Point"), kMovePointKinds, 1, 1)
             .withAccepts(isMovePoint)
             .withVisible(type("point_to_point"))
      << selectionInput(QStringLiteral("to"), QObject::tr("Target Point"), kMovePointKinds, 1, 1)
             .withAccepts(isMovePoint)
             .withVisible(type("point_to_point"))
      // Point to Position
      << selectionInput(QStringLiteral("point"), QObject::tr("Point"), kMovePointKinds, 1, 1)
             .withAccepts(isMovePoint)
             .withVisible(type("point_to_position"))
      << valueInput(QStringLiteral("px"), QObject::tr("X"), ValueKind::Length, QStringLiteral("0 mm"))
             .withVisible(type("point_to_position"))
             .withManipulator(positionHandle(0))
      << valueInput(QStringLiteral("py"), QObject::tr("Y"), ValueKind::Length, QStringLiteral("0 mm"))
             .withVisible(type("point_to_position"))
             .withManipulator(positionHandle(1))
      << valueInput(QStringLiteral("pz"), QObject::tr("Z"), ValueKind::Length, QStringLiteral("0 mm"))
             .withVisible(type("point_to_position"))
             .withManipulator(positionHandle(2));
  return inputs;
}

// The hidden pivot of a free move (kept from an edit).
InputDef pivotInput() {
  return textInput(QStringLiteral("pivot"), QObject::tr("Pivot")).withVisible([](const CommandState&) { return false; });
}

// The hidden component the move works in (mitcad#699).
InputDef moveComponentInput() {
  return textInput(kMoveComponent, QObject::tr("Component")).withVisible([](const CommandState&) { return false; });
}

// Where the coordinates of the component a move works in are in the
// design: the placement of its first occurrence (the identity in the root).
gp_Trsf moveFrame(const CommandState& state, const CommandContext& context) {
  const QString component = state.text(kMoveComponent);
  if (component.isEmpty() || component == QStringLiteral("C0")) {
    return gp_Trsf();
  }
  std::function<std::optional<gp_Trsf>(const QJsonArray&)> find =
      [&component, &find](const QJsonArray& tree) -> std::optional<gp_Trsf> {
    for (const QJsonValue& value : tree) {
      const QJsonObject node = value.toObject();
      if (node.value(QStringLiteral("component")).toString() == component) {
        return trsfOf(node.value(QStringLiteral("world")).toArray());
      }
      if (const auto inside = find(node.value(QStringLiteral("children")).toArray())) {
        return inside;
      }
    }
    return std::nullopt;
  };
  try {
    const auto placed = find(context.queryObject({{QStringLiteral("query"), QStringLiteral("components")}})
                                 .value(QStringLiteral("occurrences"))
                                 .toArray());
    return placed ? *placed : gp_Trsf();
  } catch (const std::exception&) {
    return gp_Trsf();
  }
}

// Move/Copy's provisional drag motion (CommandDef::dragMotion, mitcad#838):
// Free Move's handles turn about the pivot the exact preview had, and
// Free Move's and Translate's arrows move; Rotate's ring turns about the
// axis and Point to Position's arrows move from the point the model
// resolved for the exact preview; the bodies or components follow a drag
// at once, the exact preview after it. Point to Point has no value to drag.
std::optional<DragMotion> moveDragMotion(const CommandState& state, const CommandContext& context) {
  const QString type = state.choice(QStringLiteral("type"));
  DragMotion motion;
  if (type == QStringLiteral("free") || type == QStringLiteral("translate")) {
    gp_Pnt pivot;
    if (type == QStringLiteral("free")) {
      const auto middle = pivotOf(state, context);
      if (!middle) {
        return std::nullopt;
      }
      pivot = *middle;
    }
    motion.of = [pivot](const CommandState& inputs) { return moveMotion(inputs, pivot); };
  } else if (type == QStringLiteral("rotate") || type == QStringLiteral("point_to_point") ||
             type == QStringLiteral("point_to_position")) {
    // About the axis, from the points the model worked the exact preview
    // out of (its report's `motion`), so that the bodies turn during a drag
    // of the Angle as they do in the result.
    motion.resolve = [](const QJsonObject& basis) -> DragMotion::Motion {
      if (basis.isEmpty()) {
        return {};
      }
      return [basis](const CommandState& inputs) { return basisMotion(inputs, basis); };
    };
  } else {
    return std::nullopt;
  }
  motion.frame = moveFrame(state, context);
  // What moves: the occurrences, the bodies, or their copies.
  if (state.choice(QStringLiteral("move_type")) == QStringLiteral("components")) {
    for (const SelectionItem& item : state.items(QStringLiteral("components"))) {
      motion.occurrences << item.occurrence;
    }
  } else if (state.checked(QStringLiteral("copy"))) {
    motion.made = true;
  } else {
    for (const SelectionItem& item : state.items(QStringLiteral("bodies"))) {
      motion.bodies << item.owner;
    }
  }
  return motion;
}

} // namespace

// ---------------------------------------------------------------------------
// Move/Copy

CommandDef moveCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.move");
  def.name = QObject::tr("Move/Copy");
  def.icon = QStringLiteral("move");
  def.tooltip = QObject::tr("Moves or copies bodies, or moves components");
  def.shortcut = QKeySequence(Qt::Key_M);
  def.group = QStringLiteral("MODIFY");
  def.pinned = false;
  def.keywords = {QStringLiteral("translate"), QStringLiteral("rotate"), QStringLiteral("copy"),
                  QStringLiteral("transform")};
  def.featureType = QStringLiteral("move");
  def.editsAlso = {QStringLiteral("move_occurrence")};
  const auto bodies = choiceIs(QStringLiteral("move_type"), QStringLiteral("bodies"));
  // A body of another component moves as its occurrence (mitcad#1196).
  const InputDef bodyObjects =
      selectionInput(QStringLiteral("bodies"), QObject::tr("Objects"), SelectKind::Body, 1, 0)
          .withVisible(bodies)
          .withOnPick([](const SelectionItem& item, CommandState& state,
                         const CommandContext& context) { takeComponentOf(item, state, context); },
                      false);
  const InputDef componentObjects =
      selectionInput(QStringLiteral("components"), QObject::tr("Objects"), SelectKind::Component | SelectKind::Body, 1, 0)
          .withTooltip(QObject::tr("Components in the browser, or a body a component's occurrence places"))
          .withAccepts([](const SelectionItem& item) { return !item.occurrence.isEmpty(); })
          .withConvert([](const SelectionItem& item) {
            SelectionItem occurrence{SelectKind::Component, QString(), QString(), QString()};
            occurrence.occurrence = item.occurrence;
            if (item.kind == SelectKind::Component) {
              occurrence.owner = item.owner;
            }
            return occurrence;
          })
          .withVisible(choiceIs(QStringLiteral("move_type"), QStringLiteral("components")));
  def.inputs = {
      choiceInput(QStringLiteral("move_type"), QObject::tr("Move Object"),
                  {{QStringLiteral("bodies"), QStringLiteral("Bodies")},
                   {QStringLiteral("components"), QStringLiteral("Components")}},
                  QStringLiteral("bodies")),
      bodyObjects,
      componentObjects,
  };
  // At the top of the view's context menu and on the browser's rows
  // (mitcad#735): for bodies, or for placed components (occurrences), which
  // it starts with; not for a mix, which it would move as bodies only.
  def.offeredAtTop = takenWholeByOneOf({bodyObjects, componentObjects});
  // The motion, as Move Face's (mitcad#531).
  def.inputs << motionInputs(kMoveTypes, false);
  def.inputs << checkInput(QStringLiteral("copy"), QObject::tr("Create Copy"), false)
                    .withTooltip(QObject::tr("Move copies of the bodies; the originals stay"))
                    .withVisible(bodies)
             << pivotInput() << moveComponentInput();
  def.enabled = [&context] { return hasBodies(context); };
  // Bodies and faces of other components selected before (a placed BUILD
  // part clicked, mitcad#1196) move as their occurrences.
  def.preselected = [](const Selection& selection, CommandState& state, const CommandContext& context) {
    takeComponentsOf(selection, state, context);
  };
  // Components selected before (a pasted copy, P9) move as components.
  def.init = [](CommandState& state, const CommandContext& context) {
    if (!state.items(QStringLiteral("components")).isEmpty() && state.items(QStringLiteral("bodies")).isEmpty()) {
      state.setChoice(QStringLiteral("move_type"), QStringLiteral("components"));
    }
    initMoveComponent(state, context);
  };

  def.build = [](const CommandState& state, const CommandContext& context) {
    Built result;
    const QJsonObject transform = transformOf(state, context, result);
    if (!result.error.isEmpty()) {
      return result;
    }
    if (state.choice(QStringLiteral("move_type")) == QStringLiteral("components")) {
      QJsonArray occurrences;
      for (const SelectionItem& item : state.items(QStringLiteral("components"))) {
        // The definition names an occurrence by its uid, the last step of
        // its path ("O1/O3": a BUILD part in a new design's Component1,
        // mitcad#1196); `load` gives the path back (occurrencePath).
        occurrences.append(item.occurrence.section(QLatin1Char('/'), -1));
      }
      result.def = {{QStringLiteral("type"), QStringLiteral("move_occurrence")},
                    {QStringLiteral("occurrences"), occurrences},
                    {QStringLiteral("transform"), transform}};
      return result;
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("move")},
                  {QStringLiteral("bodies"), bodyUids(state.items(QStringLiteral("bodies")))},
                  {QStringLiteral("transform"), transform}};
    if (state.checked(QStringLiteral("copy"))) {
      result.def.insert(QStringLiteral("copy"), true);
    }
    return result;
  };

  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    if (str(feature, "type") == QStringLiteral("move_occurrence")) {
      state.setChoice(QStringLiteral("move_type"), QStringLiteral("components"));
      Selection items;
      for (const QJsonValue& occurrence : feature.value(QStringLiteral("occurrences")).toArray()) {
        SelectionItem item{SelectKind::Component, QString(), QString(), QString()};
        item.occurrence = occurrencePath(occurrence.toString(), context);
        items.append(item);
      }
      state.setItems(QStringLiteral("components"), items);
    } else {
      state.setItems(QStringLiteral("bodies"), bodiesOf(feature.value(QStringLiteral("bodies")).toArray()));
      state.setChecked(QStringLiteral("copy"), feature.value(QStringLiteral("copy")).toBool());
    }
    loadTransform(feature.value(QStringLiteral("transform")).toObject(), state, feature, context);
  };
  // A free move's handles where the feature finds what moves (mitcad#1583).
  def.rolledBack = splitFreeMoveRolledBack;

  def.canEdit = [&context](const QJsonObject& feature) {
    return transformEditable(feature.value(QStringLiteral("transform")).toObject(), context);
  };
  // A grounded occurrence moves and stays grounded there (mitcad#1196; the
  // model's move_occurrence): said under the inputs, not refused.
  def.note = [](const CommandState& state, const CommandContext& context) -> QString {
    if (state.choice(QStringLiteral("move_type")) != QStringLiteral("components")) {
      return QString();
    }
    const Selection& components = state.items(QStringLiteral("components"));
    const bool grounded = std::any_of(components.begin(), components.end(), [&context](const SelectionItem& item) {
      return groundedAt(item.occurrence, context);
    });
    return grounded ? QObject::tr("Grounded: it stays grounded at the new place.") : QString();
  };
  // The bodies or components follow a handle's drag at once (mitcad#838).
  def.dragMotion = moveDragMotion;
  // Rotate's ring about the axis the model turns about (mitcad#838).
  def.previewReported = keepRotateBasis;

  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("%1 %2 %3 (%4)")
        .arg(state.checked(QStringLiteral("copy")) ? QStringLiteral("Copied") : QStringLiteral("Moved"))
        .arg(state.items(movedKey(state)).size())
        .arg(state.choice(QStringLiteral("move_type")), label(kMoveTypes, state.choice(QStringLiteral("type"))));
  };
  return def;
}

// ---------------------------------------------------------------------------
// Move Face (mitcad#531; commands.md, "Move face")

namespace {

const Choices kMoveFaceTypes = {{QStringLiteral("free"), QStringLiteral("Free Move")},
                                {QStringLiteral("translate"), QStringLiteral("Translate")},
                                {QStringLiteral("along"), QStringLiteral("Translate Along")},
                                {QStringLiteral("rotate"), QStringLiteral("Rotate")},
                                {QStringLiteral("point_to_point"), QStringLiteral("Point to Point")},
                                {QStringLiteral("point_to_position"), QStringLiteral("Point to Position")}};

// A definition's faces as items of its body.
Selection movedFacesOf(const QJsonObject& feature, const CommandContext& context) {
  Selection items;
  for (const QJsonValue& face : feature.value(QStringLiteral("faces")).toArray()) {
    items.append(itemOf(QJsonObject{{QStringLiteral("body"), feature.value(QStringLiteral("body"))},
                                    {QStringLiteral("face"), face}},
                        context));
  }
  return items;
}

// The motion as the panel has it, its values as numbers (mm, radians),
// for the summary; empty while it is not complete.
QJsonObject shownMotion(const CommandState& state, const CommandContext& context) {
  Built error;
  QJsonObject transform = transformOf(state, context, error);
  if (!error.error.isEmpty()) {
    return QJsonObject();
  }
  const QString type = str(transform, "type");
  const auto set = [&transform, &state](const char* slot, const char* input) {
    transform.insert(QLatin1String(slot), state.value(QLatin1String(input)));
  };
  if (type == QStringLiteral("translate_xyz")) {
    set("x", "x");
    set("y", "y");
    set("z", "z");
  } else if (type == QStringLiteral("translate_along")) {
    set("distance", "distance");
  } else if (type == QStringLiteral("rotate")) {
    set("angle", "angle");
  } else if (type == QStringLiteral("point_to_position")) {
    set("x", "px");
    set("y", "py");
    set("z", "pz");
  }
  return transform;
}

QString shownSummary(const CommandState& state, const CommandContext& context) {
  const Selection& faces = state.items(QStringLiteral("faces"));
  const QJsonObject motion = shownMotion(state, context);
  if (faces.isEmpty() || motion.isEmpty()) {
    return QString();
  }
  const QString body = context.bodyName(faces.first().owner);
  return moveFaceSummary(QObject::tr("Move Face"), static_cast<int>(faces.size()),
                         body.isEmpty() ? faces.first().owner : body, motion);
}

} // namespace

CommandDef moveFaceCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.move_face");
  def.name = QObject::tr("Move Face");
  def.icon = QStringLiteral("move-face");
  def.tooltip = QObject::tr("Moves faces of a body rigidly; the faces around extend or shorten to follow");
  def.group = QStringLiteral("MODIFY");
  def.keywords = {QStringLiteral("move face"), QStringLiteral("tilt face"), QStringLiteral("rotate face"),
                  QStringLiteral("push"), QStringLiteral("translate"), QStringLiteral("modify")};
  def.featureType = QStringLiteral("move_face");
  def.inputs = {
      selectionInput(QStringLiteral("faces"), QObject::tr("Faces"), SelectKind::Face, 1, 0)
          .withoutMeshes()
          .withTooltip(QObject::tr("Faces of one body; they move together")),
  };
  // The motion, as Move/Copy's, with Translate Along.
  def.inputs << motionInputs(kMoveFaceTypes, true) << pivotInput() << moveComponentInput();
  def.enabled = [&context] { return hasBodies(context); };
  def.init = [](CommandState& state, const CommandContext& context) { initMoveComponent(state, context); };
  def.build = [](const CommandState& state, const CommandContext& context) {
    Built result;
    const Selection& faces = state.items(QStringLiteral("faces"));
    const QString body = oneBody(faces, result, QStringLiteral("faces"));
    if (body.isEmpty()) {
      return result;
    }
    const QJsonObject transform = transformOf(state, context, result);
    if (!result.error.isEmpty()) {
      return result;
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("move_face")},
                  {QStringLiteral("body"), body},
                  {QStringLiteral("faces"), names(faces, SelectKind::Face)},
                  {QStringLiteral("transform"), transform}};
    return result;
  };
  // An edit (made here or by the .f3d import) opens with its faces first:
  // a free move turns about their middle.
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("faces"), movedFacesOf(feature, context));
    loadTransform(feature.value(QStringLiteral("transform")).toObject(), state, feature, context);
  };
  // A free move's handles where the feature finds the faces (mitcad#1583).
  def.rolledBack = splitFreeMoveRolledBack;
  def.canEdit = [&context](const QJsonObject& feature) {
    return transformEditable(feature.value(QStringLiteral("transform")).toObject(), context, true);
  };
  // Rotate's ring, and so the angle a drag of it gives, about the axis the
  // model turns the faces about (the preview's `motion`), as Move/Copy's
  // (mitcad#854). No provisional drag motion: the faces' body is not moved
  // rigidly (the faces around extend or shorten), so a drag shows the
  // exact previews.
  def.previewReported = keepRotateBasis;
  // Under the inputs after a preview: what moves and how.
  def.note = [](const CommandState& state, const CommandContext& context) { return shownSummary(state, context); };
  def.describe = [](const CommandState& state, const CommandContext& context) {
    return QStringLiteral("Moved faces: %1").arg(shownSummary(state, context));
  };
  return def;
}

// ---------------------------------------------------------------------------
// Align

CommandDef alignCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.align");
  def.name = QObject::tr("Align");
  def.icon = QStringLiteral("align");
  def.tooltip = QObject::tr("Moves bodies so that a point, axis or plane of them meets another");
  def.group = QStringLiteral("MODIFY");
  def.featureType = QStringLiteral("align");
  const SelectFilter kinds = kPointKinds | kAxisKinds | kPlaneKinds;
  const auto fits = [](const SelectionItem& item) { return isPointLike(item) || isAxial(item) || isPlanar(item); };
  def.inputs = {
      selectionInput(QStringLiteral("bodies"), QObject::tr("Objects"), SelectKind::Body, 1, 0),
      selectionInput(QStringLiteral("from"), QObject::tr("From"), kinds, 1, 1)
          .withTooltip(QObject::tr("A point, an axis or a plane of what moves"))
          .withAccepts(fits),
      selectionInput(QStringLiteral("to"), QObject::tr("To"), kinds, 1, 1)
          .withTooltip(QObject::tr("Where it goes"))
          .withAccepts(fits),
      flipInput(QStringLiteral("flip"), QObject::tr("Flip")),
      valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("0 deg"))
          .withTooltip(QObject::tr("Turns about the target's axis or normal")),
  };
  def.enabled = [&context] { return hasBodies(context); };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    result.def = {{QStringLiteral("type"), QStringLiteral("align")},
                  {QStringLiteral("bodies"), bodyUids(state.items(QStringLiteral("bodies")))},
                  {QStringLiteral("from"), alignRef(state.items(QStringLiteral("from")).first())},
                  {QStringLiteral("to"), alignRef(state.items(QStringLiteral("to")).first())}};
    if (state.checked(QStringLiteral("flip"))) {
      result.def.insert(QStringLiteral("flip"), true);
    }
    if (nonZero(state, QStringLiteral("angle"))) {
      result.def.insert(QStringLiteral("angle"), state.expression(QStringLiteral("angle")));
    }
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("bodies"), bodiesOf(feature.value(QStringLiteral("bodies")).toArray()));
    state.setItems(QStringLiteral("from"), {alignItem(feature.value(QStringLiteral("from")).toObject(), context)});
    state.setItems(QStringLiteral("to"), {alignItem(feature.value(QStringLiteral("to")).toObject(), context)});
    state.setChecked(QStringLiteral("flip"), feature.value(QStringLiteral("flip")).toBool());
    loadValue(state, QStringLiteral("angle"), feature.value(QStringLiteral("angle")), feature, context);
  };
  def.canEdit = [&context](const QJsonObject& feature) {
    return alignItem(feature.value(QStringLiteral("from")).toObject(), context).isValid() &&
           alignItem(feature.value(QStringLiteral("to")).toObject(), context).isValid();
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Aligned %1 body(ies)").arg(state.items(QStringLiteral("bodies")).size());
  };
  return def;
}

// ---------------------------------------------------------------------------
// Scale

CommandDef scaleCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.scale");
  def.name = QObject::tr("Scale");
  def.icon = QStringLiteral("scale");
  def.tooltip = QObject::tr("Scales bodies about a point, evenly or along X, Y and Z");
  def.group = QStringLiteral("MODIFY");
  def.keywords = {QStringLiteral("resize"), QStringLiteral("enlarge")};
  def.featureType = QStringLiteral("scale");
  const auto uniform = choiceIs(QStringLiteral("scale_type"), QStringLiteral("uniform"));
  const auto other = [uniform](const CommandState& s) { return !uniform(s); };
  def.inputs = {
      selectionInput(QStringLiteral("bodies"), QObject::tr("Entities"), SelectKind::Body, 1, 0),
      selectionInput(QStringLiteral("point"), QObject::tr("Point"), kPointKinds, 1, 1)
          .withTooltip(QObject::tr("The point that stays; the origin at first")),
      choiceInput(QStringLiteral("scale_type"), QObject::tr("Scale Type"),
                  {{QStringLiteral("uniform"), QStringLiteral("Uniform")},
                   {QStringLiteral("non_uniform"), QStringLiteral("Non Uniform")}},
                  QStringLiteral("uniform")),
      valueInput(QStringLiteral("factor"), QObject::tr("Scale Factor"), ValueKind::Unitless, QStringLiteral("2"))
          .withVisible(uniform),
      valueInput(QStringLiteral("x"), QObject::tr("X Scale"), ValueKind::Unitless, QStringLiteral("1")).withVisible(other),
      valueInput(QStringLiteral("y"), QObject::tr("Y Scale"), ValueKind::Unitless, QStringLiteral("1")).withVisible(other),
      valueInput(QStringLiteral("z"), QObject::tr("Z Scale"), ValueKind::Unitless, QStringLiteral("1")).withVisible(other),
  };
  def.init = [](CommandState& state, const CommandContext&) {
    if (state.items(QStringLiteral("point")).isEmpty()) {
      state.setItems(QStringLiteral("point"), {{SelectKind::Point, QStringLiteral("origin"), QString(), QStringLiteral("point")}});
    }
  };
  def.enabled = [&context] { return hasBodies(context); };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    QJsonObject scale{{QStringLiteral("type"), state.choice(QStringLiteral("scale_type"))}};
    if (state.choice(QStringLiteral("scale_type")) == QStringLiteral("uniform")) {
      scale.insert(QStringLiteral("factor"), state.expression(QStringLiteral("factor")));
    } else {
      for (const char* axis : {"x", "y", "z"}) {
        scale.insert(QLatin1String(axis), state.expression(QString::fromLatin1(axis)));
      }
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("scale")},
                  {QStringLiteral("bodies"), bodyUids(state.items(QStringLiteral("bodies")))},
                  {QStringLiteral("point"), state.items(QStringLiteral("point")).first().reference()},
                  {QStringLiteral("scale"), scale}};
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    state.setItems(QStringLiteral("bodies"), bodiesOf(feature.value(QStringLiteral("bodies")).toArray()));
    state.setItems(QStringLiteral("point"), {itemOf(feature.value(QStringLiteral("point")), context)});
    const QJsonObject scale = feature.value(QStringLiteral("scale")).toObject();
    state.setChoice(QStringLiteral("scale_type"), str(scale, "type"));
    loadCount(state, QStringLiteral("factor"), scale.value(QStringLiteral("factor")), feature, context);
    for (const char* axis : {"x", "y", "z"}) {
      if (scale.contains(QLatin1String(axis))) {
        loadCount(state, QString::fromLatin1(axis), scale.value(QLatin1String(axis)), feature, context);
      }
    }
  };
  def.canEdit = [&context](const QJsonObject& feature) {
    return isShowable(feature.value(QStringLiteral("point")), context);
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Scaled %1 body(ies)").arg(state.items(QStringLiteral("bodies")).size());
  };
  return def;
}

// ---------------------------------------------------------------------------
// Combine

CommandDef combineCommand(const CommandContext& context) {
  CommandDef def;
  def.id = QStringLiteral("solid.combine");
  def.name = QObject::tr("Combine");
  def.icon = QStringLiteral("combine");
  def.tooltip = QObject::tr("Joins, cuts or intersects bodies with a target body");
  def.group = QStringLiteral("MODIFY");
  def.pinned = true;
  def.keywords = {QStringLiteral("boolean"), QStringLiteral("union"), QStringLiteral("subtract")};
  def.featureType = QStringLiteral("combine");
  // A pick per occurrence: two screws of one part are two tools, listed by
  // their occurrences (mitcad#544).
  InputDef toolsInput = selectionInput(QStringLiteral("tools"), QObject::tr("Tool Bodies"), SelectKind::Body, 1, 0);
  toolsInput.withoutMeshes();
  toolsInput.itemsText = combineToolsText;
  // A consumed tool leaves every occurrence of its component in the
  // preview: while Tool Bodies is active its other occurrences stay drawn
  // as tools to pick there too, and the panel says where it goes
  // (mitcad#561).
  toolsInput.withToolOverlays();
  def.inputs = {
      selectionInput(QStringLiteral("target"), QObject::tr("Target Body"), SelectKind::Body, 1, 1).withoutMeshes(),
      toolsInput,
      choiceInput(QStringLiteral("operation"), QObject::tr("Operation"),
                  {{QStringLiteral("join"), QStringLiteral("Join")},
                   {QStringLiteral("cut"), QStringLiteral("Cut")},
                   {QStringLiteral("intersect"), QStringLiteral("Intersect")}},
                  QStringLiteral("join")),
      checkInput(QStringLiteral("new_component"), QObject::tr("New Component"), false),
      checkInput(QStringLiteral("keep_tools"), QObject::tr("Keep Tools"), false),
  };
  def.enabled = [&context] { return context.queryArray(QStringLiteral("bodies")).size() > 1; };
  def.build = [](const CommandState& state, const CommandContext&) {
    Built result;
    const SelectionItem& targetItem = state.items(QStringLiteral("target")).first();
    const QString target = targetItem.owner;
    // A tool picked in another occurrence than the target (a body of
    // another component) is read where it was picked: a body in that
    // occurrence, once per occurrence (commands.md, "combine", mitcad#104,
    // mitcad#544).
    QJsonArray tools;
    for (const SelectionItem& tool : state.items(QStringLiteral("tools"))) {
      if (tool.owner == target) {
        return Built::failure(QObject::tr("The target cannot be a tool too."), QStringLiteral("tools"));
      }
      if (tool.occurrence != targetItem.occurrence) {
        QJsonObject entry{{QStringLiteral("body"), tool.owner}, {QStringLiteral("occurrence"), tool.occurrence}};
        if (!targetItem.occurrence.isEmpty()) {
          entry.insert(QStringLiteral("context"), targetItem.occurrence);
        }
        tools.append(entry);
      } else {
        tools.append(tool.owner);
      }
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("combine")},
                  {QStringLiteral("target"), target},
                  {QStringLiteral("tools"), tools},
                  {QStringLiteral("operation"), state.choice(QStringLiteral("operation"))}};
    if (state.checked(QStringLiteral("keep_tools"))) {
      result.def.insert(QStringLiteral("keep_tools"), true);
    }
    if (state.checked(QStringLiteral("new_component"))) {
      result.def.insert(QStringLiteral("new_component"), true);
    }
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext&) {
    // Tools in occurrences where they were picked, the target where they
    // see it; a plain id linked by tool_links as earlier versions wrote it
    // (mitcad#544).
    const QJsonObject links = feature.value(QStringLiteral("tool_links")).toObject();
    Selection target = bodiesOf({feature.value(QStringLiteral("target"))});
    Selection tools;
    for (const QJsonValue& value : feature.value(QStringLiteral("tools")).toArray()) {
      SelectionItem tool{SelectKind::Body, value.toString(), QString(), QString()};
      QJsonObject link;
      bool linked = false;
      if (value.isObject()) {
        const QJsonObject entry = value.toObject();
        tool.owner = str(entry, "body");
        link = {{QStringLiteral("source"), str(entry, "occurrence")},
                {QStringLiteral("target"), str(entry, "context")}};
        linked = true;
      } else if (links.contains(tool.owner)) {
        link = links.value(tool.owner).toObject();
        linked = true;
      }
      if (linked) {
        tool.occurrence = str(link, "source");
        target.first().occurrence = str(link, "target");
      }
      tools.append(tool);
    }
    state.setItems(QStringLiteral("target"), target);
    state.setItems(QStringLiteral("tools"), tools);
    state.setChoice(QStringLiteral("operation"), str(feature, "operation"));
    state.setChecked(QStringLiteral("keep_tools"), feature.value(QStringLiteral("keep_tools")).toBool());
    state.setChecked(QStringLiteral("new_component"), feature.value(QStringLiteral("new_component")).toBool());
  };
  def.note = combineNote;
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added combine (%1) of %2 tool(s)")
        .arg(state.choice(QStringLiteral("operation")))
        .arg(state.items(QStringLiteral("tools")).size());
  };
  return def;
}

} // namespace mitcad::cmd
