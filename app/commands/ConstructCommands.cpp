// SPDX-License-Identifier: MIT
// The CONSTRUCT group (U4): every construction plane, axis and point of the
// model (commands.md, "construction_plane, construction_axis,
// construction_point"), one command each in the menu. They share
// one shape: a definition type with references and values, so they are
// made from a table.
#include "Commands.hpp"

#include <cmath>

#include <QJsonArray>

#include "../framework/CommandRegistry.hpp"
#include "../framework/ModelShapes.hpp"
#include "CommandSupport.hpp"

namespace mitcad {
namespace {

using namespace cmd;

// What a field of a definition takes.
enum class Field {
  Plane,    // a plane or planar face
  Line,     // an axis, a straight edge or sketch line
  Point,    // a point, vertex or sketch point
  Round,    // a cylinder, cone or torus face
  Tangent,  // a cylinder or cone face
  Face,     // any face
  Center,   // a circular edge or sketch circle, a sphere or torus face
  Path,     // a sketch curve or connected edges
  Distance, // a length
  Angle,    // an angle
  Along,    // a fraction of a path, or a length along it
};

struct FieldSpec {
  const char* key;
  const char* label;
  Field kind;
};

struct Spec {
  const char* id;
  const char* name;
  const char* icon;
  const char* tooltip;
  const char* featureType;
  const char* type;
  std::vector<FieldSpec> fields;
  bool pinned = false;
};

const std::vector<Spec>& specs() {
  static const std::vector<Spec> all = {
      // Planes
      {"construct.offset_plane", "Offset Plane", "offset-plane",
       "A construction plane parallel to a plane or planar face", "construction_plane", "offset",
       {{"plane", "Plane", Field::Plane}, {"distance", "Distance", Field::Distance}}, true},
      {"construct.plane_angle", "Plane at Angle", "plane-angle",
       "A plane through a line, turned from a reference plane", "construction_plane", "angle",
       {{"line", "Line", Field::Line}, {"angle", "Angle", Field::Angle},
        {"plane", "Reference Plane", Field::Plane}}, true},
      {"construct.tangent_plane", "Tangent Plane", "plane-tangent",
       "A plane touching a cylinder or cone, turned from a reference plane", "construction_plane",
       "tangent",
       {{"face", "Face", Field::Tangent}, {"angle", "Angle", Field::Angle},
        {"plane", "Reference Plane", Field::Plane}}},
      {"construct.midplane", "Midplane", "plane-mid", "A plane halfway between two planes",
       "construction_plane", "midplane",
       {{"plane1", "Plane 1", Field::Plane}, {"plane2", "Plane 2", Field::Plane}}, true},
      {"construct.plane_two_edges", "Plane Through Two Edges", "plane-edges",
       "A plane through two lines that lie in one plane", "construction_plane", "two_edges",
       {{"line1", "Line 1", Field::Line}, {"line2", "Line 2", Field::Line}}},
      {"construct.plane_three_points", "Plane Through Three Points", "plane-points",
       "A plane through three points", "construction_plane", "three_points",
       {{"point1", "Point 1", Field::Point}, {"point2", "Point 2", Field::Point},
        {"point3", "Point 3", Field::Point}}},
      {"construct.plane_tangent_point", "Plane Tangent to Face at Point", "plane-tangent",
       "A plane touching a face where it is nearest to a point", "construction_plane",
       "tangent_at_point", {{"face", "Face", Field::Face}, {"point", "Point", Field::Point}}},
      {"construct.plane_along_path", "Plane Along Path", "plane-path",
       "A plane across a path, part way along it", "construction_plane", "along_path",
       {{"path", "Path", Field::Path}, {"distance", "Distance", Field::Along}}},
      {"construct.plane_edge_point", "Plane Through Edge and Point", "plane-edges",
       "A plane through a line and a point", "construction_plane", "edge_and_point",
       {{"line", "Line", Field::Line}, {"point", "Point", Field::Point}}},
      {"construct.plane_normal_point", "Plane Normal to Path at Point", "plane-path",
       "A plane across a path where it is nearest to a point", "construction_plane",
       "normal_at_point", {{"path", "Path", Field::Path}, {"point", "Point", Field::Point}}},
      // Axes
      {"construct.axis_cylinder", "Axis Through Cylinder/Cone/Torus", "construction-axis",
       "The axis of a cylinder, cone or torus", "construction_axis", "circular_face",
       {{"face", "Face", Field::Round}}},
      {"construct.axis_perpendicular", "Axis Perpendicular at Point", "construction-axis",
       "Normal to a face where it is nearest to a point", "construction_axis",
       "perpendicular_at_point", {{"face", "Face", Field::Face}, {"point", "Point", Field::Point}}},
      {"construct.axis_two_planes", "Axis Through Two Planes", "construction-axis",
       "Where two planes meet", "construction_axis", "two_planes",
       {{"plane1", "Plane 1", Field::Plane}, {"plane2", "Plane 2", Field::Plane}}},
      {"construct.axis_two_points", "Axis Through Two Points", "construction-axis",
       "From one point to another", "construction_axis", "two_points",
       {{"point1", "Point 1", Field::Point}, {"point2", "Point 2", Field::Point}}},
      {"construct.axis_edge", "Axis Through Edge", "construction-axis", "Along a straight edge",
       "construction_axis", "edge", {{"edge", "Edge", Field::Line}}},
      {"construct.axis_normal_face", "Axis Perpendicular to Face at Point", "construction-axis",
       "Through a point, along the face's normal nearest to it", "construction_axis",
       "normal_to_face_at_point", {{"face", "Face", Field::Face}, {"point", "Point", Field::Point}}},
      // Points
      {"construct.point_vertex", "Point at Vertex", "construction-point",
       "At a vertex or another point", "construction_point", "point",
       {{"point", "Point", Field::Point}}},
      {"construct.point_two_edges", "Point Through Two Edges", "construction-point",
       "Where two lines (extended) meet", "construction_point", "two_edges",
       {{"line1", "Line 1", Field::Line}, {"line2", "Line 2", Field::Line}}},
      {"construct.point_three_planes", "Point Through Three Planes", "construction-point",
       "Where three planes meet", "construction_point", "three_planes",
       {{"plane1", "Plane 1", Field::Plane}, {"plane2", "Plane 2", Field::Plane},
        {"plane3", "Plane 3", Field::Plane}}},
      {"construct.point_center", "Point at Center of Circle/Sphere/Torus", "construction-point",
       "The centre of a circular edge, a sphere or a torus", "construction_point", "center",
       {{"entity", "Entity", Field::Center}}},
      {"construct.point_edge_plane", "Point at Edge and Plane", "construction-point",
       "Where a line meets a plane", "construction_point", "edge_and_plane",
       {{"line", "Line", Field::Line}, {"plane", "Plane", Field::Plane}}},
      {"construct.point_along_path", "Point Along Path", "construction-point",
       "Part way along a path", "construction_point", "along_path",
       {{"path", "Path", Field::Path}, {"distance", "Distance", Field::Along}}},
  };
  return all;
}

bool isSelection(Field kind) {
  return kind != Field::Distance && kind != Field::Angle && kind != Field::Along;
}

InputDef fieldInput(const FieldSpec& field) {
  const QString key = QString::fromLatin1(field.key);
  const QString name = QObject::tr(field.label);
  const auto face = [&](std::function<bool(const QString&)> geometry) {
    return selectionInput(key, name, SelectKind::Face, 1, 1)
        .withAccepts([geometry](const SelectionItem& item) { return geometry(item.geometry); });
  };
  switch (field.kind) {
  case Field::Plane:
    return selectionInput(key, name, kPlaneKinds, 1, 1)
        .withTooltip(QObject::tr("A plane or a planar face"))
        .withAccepts(isPlanar);
  case Field::Line:
    return selectionInput(key, name, SelectKind::Axis | SelectKind::Edge | SelectKind::SketchCurve, 1, 1)
        .withTooltip(QObject::tr("An axis, a straight edge or a sketch line"))
        .withAccepts([](const SelectionItem& item) {
          return item.kind == SelectKind::Axis || isLine(item);
        });
  case Field::Point:
    return selectionInput(key, name, kPointKinds, 1, 1)
        .withTooltip(QObject::tr("A vertex, a sketch point or a construction point"));
  case Field::Round:
    return face([](const QString& g) {
      return g == QStringLiteral("cylinder") || g == QStringLiteral("cone") || g == QStringLiteral("torus");
    });
  case Field::Tangent:
    return face([](const QString& g) { return g == QStringLiteral("cylinder") || g == QStringLiteral("cone"); });
  case Field::Face:
    return selectionInput(key, name, SelectKind::Face, 1, 1);
  case Field::Center:
    return selectionInput(key, name, SelectKind::Edge | SelectKind::SketchCurve | SelectKind::Face, 1, 1)
        .withTooltip(QObject::tr("A circular edge or sketch circle, a sphere or a torus"))
        .withAccepts([](const SelectionItem& item) {
          return isCircular(item) || item.geometry == QStringLiteral("sphere") ||
                 item.geometry == QStringLiteral("torus");
        });
  case Field::Path:
    return selectionInput(key, name, kPathKinds, 1, 0)
        .withTooltip(QObject::tr("A sketch line, arc or circle, or connected edges"));
  case Field::Distance:
    return valueInput(key, name, ValueKind::Length, QStringLiteral("10 mm"));
  case Field::Angle:
    return valueInput(key, name, ValueKind::Angle, QStringLiteral("45 deg"));
  case Field::Along:
    break;
  }
  return valueInput(key, name, ValueKind::Unitless, QStringLiteral("0.5"));
}

// The selection input a manipulator stands on.
QString firstSelection(const Spec& spec) {
  for (const FieldSpec& field : spec.fields) {
    if (isSelection(field.kind)) {
      return QString::fromLatin1(field.key);
    }
  }
  return QString();
}

QJsonValue pathRef(const Selection& items, Built& error) {
  if (items.first().kind == SelectKind::SketchCurve) {
    if (items.size() != 1) {
      error = Built::failure(QObject::tr("Select one sketch curve, or edges."), QStringLiteral("path"));
      return QJsonValue();
    }
    return items.first().reference();
  }
  return pathOf(items, false, error, QStringLiteral("path"));
}

CommandDef construction(const Spec& spec) {
  CommandDef def;
  def.id = QString::fromLatin1(spec.id);
  def.name = QObject::tr(spec.name);
  def.icon = QString::fromLatin1(spec.icon);
  def.tooltip = QObject::tr(spec.tooltip);
  def.group = QStringLiteral("CONSTRUCT");
  def.pinned = spec.pinned;
  def.keywords = {QStringLiteral("construction"), QStringLiteral("datum"),
                  QStringLiteral("work plane"), QStringLiteral("reference")};
  def.featureType = QString::fromLatin1(spec.featureType);
  const QString type = QString::fromLatin1(spec.type);
  const bool plane = def.featureType == QStringLiteral("construction_plane");
  const QString base = firstSelection(spec);
  for (const FieldSpec& field : spec.fields) {
    InputDef input = fieldInput(field);
    if (field.kind == Field::Distance) {
      // An arrow along the plane's normal from the plane picked.
      input.withManipulator([base](const CommandState& state,
                                   const CommandContext& context) -> std::optional<Manipulator> {
        const Selection& items = state.items(base);
        if (items.isEmpty()) {
          return std::nullopt;
        }
        const auto frame = frameOf(items.first(), context);
        const auto center = centerOf(items, context);
        if (!frame || !center) {
          return std::nullopt;
        }
        return arrow(*center, frame->normal);
      });
    } else if (field.kind == Field::Angle) {
      // A ring about the line (or the face's axis).
      input.withManipulator([base](const CommandState& state,
                                   const CommandContext& context) -> std::optional<Manipulator> {
        const Selection& items = state.items(base);
        if (items.isEmpty()) {
          return std::nullopt;
        }
        const auto axis = axisOf(items.first(), context);
        const auto center = centerOf(items, context);
        if (!axis || !center) {
          return std::nullopt;
        }
        Manipulator handle = ring(*center, axis->second);
        // Angle 0 lies in the reference plane, across the line.
        const Selection& reference = state.items(QStringLiteral("plane"));
        if (!reference.isEmpty()) {
          if (const auto frame = frameOf(reference.first(), context)) {
            const gp_Vec across = gp_Vec(frame->normal).Crossed(gp_Vec(axis->second));
            if (across.Magnitude() > 1e-9) {
              handle.reference = gp_Dir(across);
            }
          }
        }
        return handle;
      });
    } else if (field.kind == Field::Line && type == QStringLiteral("angle")) {
      // A sketch line's own sketch plane is the reference plane at first.
      input.withOnPick(
          [](const SelectionItem& item, CommandState& state, const CommandContext& context) {
            if (item.kind != SelectKind::SketchCurve || !state.items(QStringLiteral("plane")).isEmpty()) {
              return;
            }
            try {
              const QJsonValue sketchPlane =
                  context
                      .queryObject({{QStringLiteral("query"), QStringLiteral("sketch")},
                                    {QStringLiteral("uid"), item.owner}})
                      .value(QStringLiteral("plane"));
              const SelectionItem reference = itemOf(sketchPlane, context);
              if (reference.isValid()) {
                state.setItems(QStringLiteral("plane"), {reference});
              }
            } catch (const std::exception&) {
              // No reference plane: the user picks one.
            }
          },
          false);
    }
    def.inputs.append(input);
    if (field.kind == Field::Along) {
      // The distance type: a fraction of the path, or a length.
      def.inputs.append(checkInput(QStringLiteral("physical"), QObject::tr("Physical Distance"), false)
                            .withTooltip(QObject::tr("A length along the path instead of a fraction")));
      def.inputs.append(valueInput(QStringLiteral("length"), QObject::tr("Length"), ValueKind::Length,
                                   QStringLiteral("10 mm"))
                            .withVisible([](const CommandState& state) {
                              return state.checked(QStringLiteral("physical"));
                            }));
      def.inputs[def.inputs.size() - 3].withVisible([](const CommandState& state) {
        return !state.checked(QStringLiteral("physical"));
      });
    }
  }
  if (def.featureType != QStringLiteral("construction_point")) {
    def.inputs.append(flipInput(QStringLiteral("flip"), QObject::tr("Flip"))
                          .withTooltip(plane ? QObject::tr("Turn the plane's normal round")
                                             : QObject::tr("Turn the axis round")));
  }

  const std::vector<FieldSpec> fields = spec.fields;
  const QString featureType = def.featureType;
  def.build = [fields, type, featureType](const CommandState& state, const CommandContext&) {
    Built result;
    QJsonObject definition{{QStringLiteral("type"), type}};
    for (const FieldSpec& field : fields) {
      const QString key = QString::fromLatin1(field.key);
      switch (field.kind) {
      case Field::Distance:
      case Field::Angle:
        definition.insert(key, state.expression(key));
        break;
      case Field::Along:
        if (state.checked(QStringLiteral("physical"))) {
          definition.insert(key, state.expression(QStringLiteral("length")));
          definition.insert(QStringLiteral("physical"), true);
        } else {
          // A fraction, as a number: the slot is a length slot.
          definition.insert(key, state.value(key));
        }
        break;
      case Field::Path: {
        const QJsonValue path = pathRef(state.items(key), result);
        if (!result.error.isEmpty()) {
          return result;
        }
        definition.insert(key, path);
        break;
      }
      default:
        definition.insert(key, state.items(key).first().reference());
        break;
      }
    }
    result.def = {{QStringLiteral("type"), featureType}, {QStringLiteral("definition"), definition}};
    if (state.checked(QStringLiteral("flip"))) {
      result.def.insert(QStringLiteral("flip"), true);
    }
    return result;
  };

  def.load = [fields](const QJsonObject& feature, CommandState& state, const CommandContext& context) {
    const QJsonObject definition = feature.value(QStringLiteral("definition")).toObject();
    for (const FieldSpec& field : fields) {
      const QString key = QString::fromLatin1(field.key);
      const QJsonValue slot = definition.value(key);
      switch (field.kind) {
      case Field::Distance:
      case Field::Angle:
        loadValue(state, key, slot, feature, context);
        break;
      case Field::Along:
        if (definition.value(QStringLiteral("physical")).toBool()) {
          state.setChecked(QStringLiteral("physical"), true);
          loadValue(state, QStringLiteral("length"), slot, feature, context);
        } else {
          loadCount(state, key, slot, feature, context);
        }
        break;
      case Field::Path:
        if (const auto items = pathItems(slot, context)) {
          state.setItems(key, *items);
        }
        break;
      default:
        state.setItems(key, {itemOf(slot, context)});
        break;
      }
    }
    state.setChecked(QStringLiteral("flip"), feature.value(QStringLiteral("flip")).toBool());
  };

  def.canEdit = [fields, type](const QJsonObject& feature) {
    const QJsonObject definition = feature.value(QStringLiteral("definition")).toObject();
    if (definition.value(QStringLiteral("type")).toString() != type) {
      return false;
    }
    for (const FieldSpec& field : fields) {
      const QJsonValue slot = definition.value(QString::fromLatin1(field.key));
      if (field.kind == Field::Path) {
        const QJsonObject path = slot.toObject();
        if (!path.contains(QStringLiteral("sketch")) && !path.contains(QStringLiteral("edges"))) {
          return false; // a fixed line
        }
      } else if (isSelection(field.kind)) {
        const QJsonObject object = slot.toObject();
        // Fixed geometry has no item to show.
        if (object.contains(QStringLiteral("origin")) ||
            (object.contains(QStringLiteral("point")) && !object.contains(QStringLiteral("sketch")))) {
          return false;
        }
      }
    }
    return true;
  };

  def.describe = [featureType](const CommandState&, const CommandContext& context) {
    // The datum just added: the last one.
    const QJsonArray datums = context.query({{QStringLiteral("query"), QStringLiteral("datums")}}).toArray();
    if (datums.isEmpty()) {
      return QStringLiteral("Added %1").arg(featureType);
    }
    const QJsonObject datum = datums.last().toObject();
    const auto xyz = [&datum](const char* key) {
      const QJsonArray v = datum.value(QLatin1String(key)).toArray();
      QStringList parts;
      for (const QJsonValue& c : v) {
        parts << QString::number(std::abs(c.toDouble()) < 1e-9 ? 0.0 : c.toDouble(), 'g', 6);
      }
      return QStringLiteral("(%1)").arg(parts.join(QStringLiteral(", ")));
    };
    const QString kind = datum.value(QStringLiteral("type")).toString();
    QString geometry;
    if (kind == QStringLiteral("plane")) {
      geometry = QStringLiteral("origin %1, normal %2").arg(xyz("origin"), xyz("normal"));
    } else if (kind == QStringLiteral("axis")) {
      geometry = QStringLiteral("origin %1, direction %2").arg(xyz("origin"), xyz("direction"));
    } else {
      geometry = QStringLiteral("point %1").arg(xyz("point"));
    }
    return QStringLiteral("Added %1 %2: %3")
        .arg(kind, datum.value(QStringLiteral("name")).toString(), geometry);
  };
  return def;
}

} // namespace

void registerConstructCommands(CommandRegistry& registry, const CommandContext& context) {
  Q_UNUSED(context);
  for (const Spec& spec : specs()) {
    registry.add(construction(spec));
  }
}

} // namespace mitcad
