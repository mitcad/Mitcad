// SPDX-License-Identifier: MIT
// Scale, Combine, Move/Copy, Align, Physical Material and Appearance
// (commands.md, "move, align, scale", "combine", "Visibility and
// display"). Move/Copy's free move has
// handles: arrows along X, Y and Z and rings about them, at the middle of
// what moves.
#include <algorithm>
#include <cmath>
#include <functional>

#include <QHash>
#include <QJsonArray>
#include <QKeySequence>
#include <QStringList>

#include <gp_Ax1.hxx>
#include <gp_Trsf.hxx>

#include "../framework/Appearances.hpp"
#include "CommandFactories.hpp"
#include "CommandSupport.hpp"

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
// or through a body they place).
QString movedKey(const CommandState& state) { return state.choice(QStringLiteral("move_type")); }

// The middle the free move turns about: kept from an edit, else the middle
// of what moves.
std::optional<gp_Pnt> pivotOf(const CommandState& state, const CommandContext& context) {
  const QStringList parts = state.text(QStringLiteral("pivot")).split(QLatin1Char(','));
  if (parts.size() == 3) {
    return gp_Pnt(parts[0].toDouble(), parts[1].toDouble(), parts[2].toDouble());
  }
  return centerOf(state.items(movedKey(state)), context);
}

// The rotation of a free move: about X, then Y, then Z.
gp_Trsf rotationOf(double rx, double ry, double rz) {
  gp_Trsf x;
  x.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(1, 0, 0)), rx);
  gp_Trsf y;
  y.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 1, 0)), ry);
  gp_Trsf z;
  z.SetRotation(gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1)), rz);
  return z * y * x;
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

// A free move's handles: an arrow along a model axis or a ring about it.
std::function<std::optional<Manipulator>(const CommandState&, const CommandContext&)>
freeHandle(int axis, bool turn) {
  return [axis, turn](const CommandState& state, const CommandContext& context) -> std::optional<Manipulator> {
    auto pivot = pivotOf(state, context);
    if (!pivot) {
      return std::nullopt;
    }
    const gp_Dir direction(axis == 0 ? 1 : 0, axis == 1 ? 1 : 0, axis == 2 ? 1 : 0);
    if (turn) {
      // About the moved middle.
      pivot->Translate(gp_Vec(state.value(QStringLiteral("tx")), state.value(QStringLiteral("ty")),
                              state.value(QStringLiteral("tz"))));
      return ring(*pivot, direction);
    }
    return arrow(*pivot, direction);
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

// The model's transform of a move.
QJsonObject transformOf(const CommandState& state, const CommandContext& context, Built& error) {
  const QString type = state.choice(QStringLiteral("type"));
  if (type == QStringLiteral("free")) {
    const auto pivot = pivotOf(state, context);
    if (!pivot) {
      error = Built::failure(QObject::tr("Select what moves."), movedKey(state));
      return {};
    }
    // Turn about the pivot, then move: p -> R (p - c) + c + t.
    gp_Trsf move = rotationOf(state.value(QStringLiteral("rx")), state.value(QStringLiteral("ry")),
                              state.value(QStringLiteral("rz")));
    const gp_XYZ c = pivot->XYZ();
    gp_XYZ rc = c;
    move.Transforms(rc);
    const gp_XYZ t = c - rc +
                     gp_XYZ(state.value(QStringLiteral("tx")), state.value(QStringLiteral("ty")), state.value(QStringLiteral("tz")));
    QJsonArray rows;
    for (int r = 1; r <= 3; ++r) {
      rows.append(QJsonArray{move.Value(r, 1), move.Value(r, 2), move.Value(r, 3), t.Coord(r)});
    }
    return {{QStringLiteral("type"), type}, {QStringLiteral("matrix"), rows}};
  }
  if (type == QStringLiteral("translate")) {
    return {{QStringLiteral("type"), QStringLiteral("translate_xyz")},
            {QStringLiteral("x"), state.expression(QStringLiteral("x"))},
            {QStringLiteral("y"), state.expression(QStringLiteral("y"))},
            {QStringLiteral("z"), state.expression(QStringLiteral("z"))}};
  }
  if (type == QStringLiteral("rotate")) {
    return {{QStringLiteral("type"), type},
            {QStringLiteral("axis"), state.items(QStringLiteral("axis")).first().reference()},
            {QStringLiteral("angle"), state.expression(QStringLiteral("angle"))}};
  }
  if (type == QStringLiteral("point_to_point")) {
    return {{QStringLiteral("type"), type},
            {QStringLiteral("from"), state.items(QStringLiteral("from")).first().reference()},
            {QStringLiteral("to"), state.items(QStringLiteral("to")).first().reference()}};
  }
  return {{QStringLiteral("type"), type},
          {QStringLiteral("point"), state.items(QStringLiteral("point")).first().reference()},
          {QStringLiteral("x"), state.expression(QStringLiteral("px"))},
          {QStringLiteral("y"), state.expression(QStringLiteral("py"))},
          {QStringLiteral("z"), state.expression(QStringLiteral("pz"))}};
}

bool loadTransform(const QJsonObject& transform, CommandState& state, const QJsonObject& feature,
                   const CommandContext& context) {
  const QString type = str(transform, "type");
  if (type == QStringLiteral("free")) {
    state.setChoice(QStringLiteral("type"), type);
    const QJsonArray rows = transform.value(QStringLiteral("matrix")).toArray();
    const auto at = [&rows](int r, int c) { return rows.at(r).toArray().at(c).toDouble(); };
    // R = Rz Ry Rx.
    const double ry = std::asin(std::clamp(-at(2, 0), -1.0, 1.0));
    const double rx = std::atan2(at(2, 1), at(2, 2));
    const double rz = std::atan2(at(1, 0), at(0, 0));
    // The pivot is the middle of what moves as it is now; t = t' - c + R c.
    const auto pivot = centerOf(state.items(movedKey(state)), context);
    const gp_XYZ c = pivot ? pivot->XYZ() : gp_XYZ(0, 0, 0);
    gp_XYZ rc = c;
    rotationOf(rx, ry, rz).Transforms(rc);
    const gp_XYZ t = gp_XYZ(at(0, 3), at(1, 3), at(2, 3)) - c + rc;
    state.setText(QStringLiteral("pivot"),
                  QStringLiteral("%1,%2,%3").arg(number(c.X()), number(c.Y()), number(c.Z())));
    const auto deg = [](double radians) { return degrees(radians) + QStringLiteral(" deg"); };
    state.setText(QStringLiteral("tx"), lengthText(t.X(), context));
    state.setText(QStringLiteral("ty"), lengthText(t.Y(), context));
    state.setText(QStringLiteral("tz"), lengthText(t.Z(), context));
    state.setText(QStringLiteral("rx"), deg(rx));
    state.setText(QStringLiteral("ry"), deg(ry));
    state.setText(QStringLiteral("rz"), deg(rz));
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
    state.setItems(QStringLiteral("axis"), {itemOf(transform.value(QStringLiteral("axis")), context)});
    loadValue(state, QStringLiteral("angle"), transform.value(QStringLiteral("angle")), feature, context);
    return true;
  }
  if (type == QStringLiteral("point_to_point")) {
    state.setChoice(QStringLiteral("type"), type);
    state.setItems(QStringLiteral("from"), {itemOf(transform.value(QStringLiteral("from")), context)});
    state.setItems(QStringLiteral("to"), {itemOf(transform.value(QStringLiteral("to")), context)});
    return true;
  }
  if (type == QStringLiteral("point_to_position")) {
    state.setChoice(QStringLiteral("type"), type);
    state.setItems(QStringLiteral("point"), {itemOf(transform.value(QStringLiteral("point")), context)});
    loadValue(state, QStringLiteral("px"), transform.value(QStringLiteral("x")), feature, context);
    loadValue(state, QStringLiteral("py"), transform.value(QStringLiteral("y")), feature, context);
    loadValue(state, QStringLiteral("pz"), transform.value(QStringLiteral("z")), feature, context);
    return true;
  }
  return false; // translate_along has no inputs in the panel
}

bool transformEditable(const QJsonObject& transform, const CommandContext& context) {
  const QString type = str(transform, "type");
  if (type == QStringLiteral("free") || type == QStringLiteral("translate_xyz")) {
    return true;
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
  const auto type = [](const char* kind) { return choiceIs(QStringLiteral("type"), QString::fromLatin1(kind)); };
  const auto bodies = choiceIs(QStringLiteral("move_type"), QStringLiteral("bodies"));
  def.inputs = {
      choiceInput(QStringLiteral("move_type"), QObject::tr("Move Object"),
                  {{QStringLiteral("bodies"), QStringLiteral("Bodies")},
                   {QStringLiteral("components"), QStringLiteral("Components")}},
                  QStringLiteral("bodies")),
      selectionInput(QStringLiteral("bodies"), QObject::tr("Objects"), SelectKind::Body, 1, 0)
          .withVisible(bodies),
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
          .withVisible(choiceIs(QStringLiteral("move_type"), QStringLiteral("components"))),
      choiceInput(QStringLiteral("type"), QObject::tr("Move Type"), kMoveTypes, QStringLiteral("free")),
      // Free Move
      valueInput(QStringLiteral("tx"), QObject::tr("X Distance"), ValueKind::Length, QStringLiteral("0 mm"))
          .withVisible(type("free"))
          .withManipulator(freeHandle(0, false)),
      valueInput(QStringLiteral("ty"), QObject::tr("Y Distance"), ValueKind::Length, QStringLiteral("0 mm"))
          .withVisible(type("free"))
          .withManipulator(freeHandle(1, false)),
      valueInput(QStringLiteral("tz"), QObject::tr("Z Distance"), ValueKind::Length, QStringLiteral("0 mm"))
          .withVisible(type("free"))
          .withManipulator(freeHandle(2, false)),
      valueInput(QStringLiteral("rx"), QObject::tr("X Angle"), ValueKind::Angle, QStringLiteral("0 deg"))
          .withVisible(type("free"))
          .withManipulator(freeHandle(0, true)),
      valueInput(QStringLiteral("ry"), QObject::tr("Y Angle"), ValueKind::Angle, QStringLiteral("0 deg"))
          .withVisible(type("free"))
          .withManipulator(freeHandle(1, true)),
      valueInput(QStringLiteral("rz"), QObject::tr("Z Angle"), ValueKind::Angle, QStringLiteral("0 deg"))
          .withVisible(type("free"))
          .withManipulator(freeHandle(2, true)),
      // Translate
      valueInput(QStringLiteral("x"), QObject::tr("X Distance"), ValueKind::Length, QStringLiteral("0 mm"))
          .withVisible(type("translate"))
          .withManipulator(translateHandle(0)),
      valueInput(QStringLiteral("y"), QObject::tr("Y Distance"), ValueKind::Length, QStringLiteral("0 mm"))
          .withVisible(type("translate"))
          .withManipulator(translateHandle(1)),
      valueInput(QStringLiteral("z"), QObject::tr("Z Distance"), ValueKind::Length, QStringLiteral("0 mm"))
          .withVisible(type("translate"))
          .withManipulator(translateHandle(2)),
      // Rotate
      selectionInput(QStringLiteral("axis"), QObject::tr("Axis"), kAxisKinds, 1, 1)
          .withAccepts(isAxial)
          .withVisible(type("rotate")),
      valueInput(QStringLiteral("angle"), QObject::tr("Angle"), ValueKind::Angle, QStringLiteral("90 deg"))
          .withTooltip(QObject::tr("Right-handed about the axis"))
          .withVisible(type("rotate"))
          .withManipulator([](const CommandState& state, const CommandContext& model) -> std::optional<Manipulator> {
            const Selection& axis = state.items(QStringLiteral("axis"));
            const auto center = centerOf(state.items(movedKey(state)), model);
            if (axis.isEmpty() || !center) {
              return std::nullopt;
            }
            const auto line = axisOf(axis.first(), model);
            if (!line) {
              return std::nullopt;
            }
            const gp_Vec along(line->second);
            const gp_Pnt foot = line->first.Translated(along * gp_Vec(line->first, *center).Dot(along));
            Manipulator m = ring(foot, line->second);
            if (foot.Distance(*center) > 1e-9) {
              m.reference = gp_Dir(gp_Vec(foot, *center));
            }
            return m;
          }),
      // Point to Point
      selectionInput(QStringLiteral("from"), QObject::tr("Origin Point"), kMovePointKinds, 1, 1)
          .withAccepts(isMovePoint)
          .withVisible(type("point_to_point")),
      selectionInput(QStringLiteral("to"), QObject::tr("Target Point"), kMovePointKinds, 1, 1)
          .withAccepts(isMovePoint)
          .withVisible(type("point_to_point")),
      // Point to Position
      selectionInput(QStringLiteral("point"), QObject::tr("Point"), kMovePointKinds, 1, 1)
          .withAccepts(isMovePoint)
          .withVisible(type("point_to_position")),
      valueInput(QStringLiteral("px"), QObject::tr("X"), ValueKind::Length, QStringLiteral("0 mm"))
          .withVisible(type("point_to_position")),
      valueInput(QStringLiteral("py"), QObject::tr("Y"), ValueKind::Length, QStringLiteral("0 mm"))
          .withVisible(type("point_to_position")),
      valueInput(QStringLiteral("pz"), QObject::tr("Z"), ValueKind::Length, QStringLiteral("0 mm"))
          .withVisible(type("point_to_position")),
      checkInput(QStringLiteral("copy"), QObject::tr("Create Copy"), false)
          .withTooltip(QObject::tr("Move copies of the bodies; the originals stay"))
          .withVisible(bodies),
      textInput(QStringLiteral("pivot"), QObject::tr("Pivot")).withVisible([](const CommandState&) { return false; }),
  };
  def.enabled = [&context] { return hasBodies(context); };
  // Components selected before (a pasted copy, P9) move as components.
  def.init = [](CommandState& state, const CommandContext&) {
    if (!state.items(QStringLiteral("components")).isEmpty() && state.items(QStringLiteral("bodies")).isEmpty()) {
      state.setChoice(QStringLiteral("move_type"), QStringLiteral("components"));
    }
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
        occurrences.append(item.occurrence);
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

  def.canEdit = [&context](const QJsonObject& feature) {
    return transformEditable(feature.value(QStringLiteral("transform")).toObject(), context);
  };

  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("%1 %2 %3 (%4)")
        .arg(state.checked(QStringLiteral("copy")) ? QStringLiteral("Copied") : QStringLiteral("Moved"))
        .arg(state.items(movedKey(state)).size())
        .arg(state.choice(QStringLiteral("move_type")), label(kMoveTypes, state.choice(QStringLiteral("type"))));
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
      scale.insert(QStringLiteral("factor"), countValue(state, QStringLiteral("factor")));
    } else {
      for (const char* axis : {"x", "y", "z"}) {
        scale.insert(QLatin1String(axis), countValue(state, QString::fromLatin1(axis)));
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
  def.inputs = {
      selectionInput(QStringLiteral("target"), QObject::tr("Target Body"), SelectKind::Body, 1, 1),
      selectionInput(QStringLiteral("tools"), QObject::tr("Tool Bodies"), SelectKind::Body, 1, 0),
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
    // another component) is read where it was picked, through a link
    // (commands.md, "combine", mitcad#104).
    QJsonObject links;
    for (const SelectionItem& tool : state.items(QStringLiteral("tools"))) {
      if (tool.owner == target) {
        return Built::failure(QObject::tr("The target cannot be a tool too."), QStringLiteral("tools"));
      }
      if (tool.occurrence != targetItem.occurrence) {
        QJsonObject link{{QStringLiteral("source"), tool.occurrence}};
        if (!targetItem.occurrence.isEmpty()) {
          link.insert(QStringLiteral("target"), targetItem.occurrence);
        }
        links.insert(tool.owner, link);
      }
    }
    result.def = {{QStringLiteral("type"), QStringLiteral("combine")},
                  {QStringLiteral("target"), target},
                  {QStringLiteral("tools"), bodyUids(state.items(QStringLiteral("tools")))},
                  {QStringLiteral("operation"), state.choice(QStringLiteral("operation"))}};
    if (!links.isEmpty()) {
      result.def.insert(QStringLiteral("tool_links"), links);
    }
    if (state.checked(QStringLiteral("keep_tools"))) {
      result.def.insert(QStringLiteral("keep_tools"), true);
    }
    if (state.checked(QStringLiteral("new_component"))) {
      result.def.insert(QStringLiteral("new_component"), true);
    }
    return result;
  };
  def.load = [](const QJsonObject& feature, CommandState& state, const CommandContext&) {
    // Linked tools where they were picked, the target where the links
    // see it.
    const QJsonObject links = feature.value(QStringLiteral("tool_links")).toObject();
    Selection target = bodiesOf({feature.value(QStringLiteral("target"))});
    Selection tools = bodiesOf(feature.value(QStringLiteral("tools")).toArray());
    for (SelectionItem& tool : tools) {
      const QJsonObject link = links.value(tool.owner).toObject();
      if (links.contains(tool.owner)) {
        tool.occurrence = str(link, "source");
        target.first().occurrence = str(link, "target");
      }
    }
    state.setItems(QStringLiteral("target"), target);
    state.setItems(QStringLiteral("tools"), tools);
    state.setChoice(QStringLiteral("operation"), str(feature, "operation"));
    state.setChecked(QStringLiteral("keep_tools"), feature.value(QStringLiteral("keep_tools")).toBool());
    state.setChecked(QStringLiteral("new_component"), feature.value(QStringLiteral("new_component")).toBool());
  };
  def.describe = [](const CommandState& state, const CommandContext&) {
    return QStringLiteral("Added combine (%1) of %2 tool(s)")
        .arg(state.choice(QStringLiteral("operation")))
        .arg(state.items(QStringLiteral("tools")).size());
  };
  return def;
}

// ---------------------------------------------------------------------------
// Physical Material and Appearance

namespace {

CommandDef attributeCommand(const CommandContext& context, bool material) {
  CommandDef def;
  def.id = material ? QStringLiteral("solid.material") : QStringLiteral("solid.appearance");
  def.name = material ? QObject::tr("Physical Material") : QObject::tr("Appearance");
  def.icon = material ? QStringLiteral("material") : QStringLiteral("appearance");
  def.tooltip = material ? QObject::tr("The material of bodies: their density for masses")
                         : QObject::tr("How bodies look");
  def.group = QStringLiteral("MODIFY");
  def.keywords = material ? QStringList{QStringLiteral("density"), QStringLiteral("mass")}
                          : QStringList{QStringLiteral("colour"), QStringLiteral("color"), QStringLiteral("paint")};
  def.modelCommand = true;
  InputDef value = material ? choiceInput(QStringLiteral("value"), QObject::tr("Material"), physicalMaterials(),
                                          QStringLiteral("steel"))
                            : choiceInput(QStringLiteral("value"), QObject::tr("Appearance"), {},
                                          QStringLiteral("steel_satin"));
  if (!material) {
    // The library's appearances and the document's own (mitcad#46).
    value.withChoices([&context](const CommandState&) {
      Choices choices{{QString(), QObject::tr("Default")}};
      for (const Appearance& appearance : appearancesOf(context.queryArray(QStringLiteral("appearances")))) {
        choices.append({appearance.id, appearance.name});
      }
      return choices;
    });
  }
  // Appearances also go to single faces (mitcad#53), overriding their
  // body's; Default takes a face's own away.
  if (material) {
    def.inputs = {selectionInput(QStringLiteral("bodies"), QObject::tr("Bodies"), SelectKind::Body, 1, 0), value};
  } else {
    def.inputs = {selectionInput(QStringLiteral("bodies"), QObject::tr("Bodies"), SelectKind::Body, 0, 0),
                  selectionInput(QStringLiteral("faces"), QObject::tr("Faces"), SelectKind::Face, 0, 0), value};
  }
  def.enabled = [&context] { return hasBodies(context); };
  def.build = [material, name = def.name](const CommandState& state, const CommandContext&) {
    Built result;
    const QString value = state.choice(QStringLiteral("value"));
    const QJsonValue id =
        value.isEmpty() || (material && value == QStringLiteral("steel")) ? QJsonValue() : QJsonValue(value);
    QJsonArray commands;
    QStringList owners;
    QHash<QString, QJsonArray> faces;
    if (!material) {
      for (const SelectionItem& item : state.items(QStringLiteral("faces"))) {
        if (!owners.contains(item.owner)) {
          owners << item.owner;
        }
        faces[item.owner].append(item.name);
      }
      if (owners.isEmpty() && state.items(QStringLiteral("bodies")).isEmpty()) {
        return Built::failure(QObject::tr("Select bodies or faces"), QStringLiteral("bodies"));
      }
    }
    for (const SelectionItem& item : state.items(QStringLiteral("bodies"))) {
      commands.append(QJsonObject{
          {QStringLiteral("cmd"), material ? QStringLiteral("set_body_material") : QStringLiteral("set_body_appearance")},
          {QStringLiteral("uid"), item.owner},
          {material ? QStringLiteral("material") : QStringLiteral("appearance"), id}});
    }
    for (const QString& owner : std::as_const(owners)) {
      commands.append(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("set_face_appearance")},
                                  {QStringLiteral("uid"), owner},
                                  {QStringLiteral("faces"), faces.value(owner)},
                                  {QStringLiteral("appearance"), id}});
    }
    result.def = {{QStringLiteral("commands"), commands}, {QStringLiteral("label"), name}};
    return result;
  };
  def.describe = [material](const CommandState& state, const CommandContext&) {
    const auto faces = material ? 0 : state.items(QStringLiteral("faces")).size();
    return QStringLiteral("%1 %2 for %3 body(ies)%4")
        .arg(material ? QStringLiteral("Material") : QStringLiteral("Appearance"), state.choice(QStringLiteral("value")))
        .arg(state.items(QStringLiteral("bodies")).size())
        .arg(faces > 0 ? QStringLiteral(", %1 face(s)").arg(faces) : QString());
  };
  return def;
}

} // namespace

CommandDef materialCommand(const CommandContext& context) { return attributeCommand(context, true); }
CommandDef appearanceCommand(const CommandContext& context) { return attributeCommand(context, false); }

} // namespace mitcad::cmd
