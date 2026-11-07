// SPDX-License-Identifier: MIT
// The CONSTRUCT group (U4): every construction plane, axis and point of the
// model (commands.md, "construction_plane, construction_axis,
// construction_point"), one command each in the menu. They share
// one shape: a definition type with references and values, so they are
// made from a table.
#include "Commands.hpp"

#include <cmath>
#include <exception>
#include <limits>

#include <QDebug>
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
      {"construct.plane_offset_point", "Offset Plane Through Point", "offset-plane",
       "A plane parallel to a plane or planar face, through a point", "construction_plane",
       "offset_through_point",
       {{"plane", "Plane", Field::Plane}, {"point", "Point", Field::Point}}},
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
        .withTooltip(QObject::tr("A sketch line, arc or circle, connected edges, or an intersection curve"));
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

// Tangent Plane's reference plane at first (mitcad#446): the origin plane
// whose normal, turned onto the face's axis at right angles, points closest
// to the clicked point, so that angle 0 (180 deg when it points away from
// it) gives the plane touching the face there. An origin plane at right
// angles to the axis cannot be the reference. The click is in the face's
// component's coordinates, the axis in the model's: the same for a body of
// the design itself.
void tangentReference(const SelectionItem& item, CommandState& state, const CommandContext& context) {
  const QString plane = QStringLiteral("plane");
  if (!item.at || !state.items(plane).isEmpty()) {
    return;
  }
  const auto axis = axisOf(item, context);
  if (!axis) {
    return;
  }
  const gp_Vec along(axis->second);
  gp_Vec towards(axis->first, gp_Pnt((*item.at)[0], (*item.at)[1], (*item.at)[2]));
  towards -= along * towards.Dot(along);
  if (towards.Magnitude() < 1e-9) {
    return;
  }
  towards.Normalize();
  SelectionItem best;
  double closest = 0.0;
  for (const char* origin : {"xy", "xz", "yz"}) {
    const SelectionItem candidate = itemOf(QString::fromLatin1(origin), context);
    const auto frame = frameOf(candidate, context);
    if (!frame) {
      continue;
    }
    gp_Vec normal(frame->normal);
    normal -= along * normal.Dot(along);
    if (normal.Magnitude() < 1e-6) {
      continue;
    }
    const double cosine = normal.Normalized().Dot(towards);
    if (std::abs(cosine) > std::abs(closest) + 1e-9) {
      best = candidate;
      closest = cosine;
    }
  }
  if (!best.isValid()) {
    return;
  }
  state.setItems(plane, {best});
  // The side of the face, unless an angle was typed.
  const QString angle = QStringLiteral("angle");
  const QString typed = state.text(angle).trimmed();
  if (typed.isEmpty() || typed == QStringLiteral("0 deg") || typed == QStringLiteral("180 deg")) {
    state.setText(angle, closest < 0.0 ? QStringLiteral("180 deg") : QStringLiteral("0 deg"));
    state.setValue(angle, std::numeric_limits<double>::quiet_NaN());
  }
  qDebug().noquote() << QStringLiteral("Tangent Plane: reference plane %1, angle %2")
                            .arg(best.owner, state.text(angle));
}

// Geometry of other components (mitcad#1029): the datum is made in its
// component (an edited datum's own, else the active one), seen at that
// component's first occurrence (where the view shows the finished datum),
// or at the occurrence an edited datum's links were made at while it still
// places the component. A face, edge or vertex picked in an occurrence of
// another component is linked from there (the model's `links`, commands.md
// "construction_plane, construction_axis, construction_point"), so that the
// other component need not be activated.
struct DatumHome {
  QString component = QStringLiteral("C0");
  QString occurrence; // empty: the root
};

// What an edited datum keeps for homeOf(): its component and its links'
// target.
const QString kHome = QStringLiteral("datum_home");

DatumHome homeOf(const CommandState& state, const CommandContext& context) {
  DatumHome home;
  const QJsonObject kept = state.object(kHome);
  if (kept.contains(QStringLiteral("component"))) {
    home.component = str(kept, "component");
  } else {
    try {
      home.component = componentsOf(context).value(QStringLiteral("active")).toString(QStringLiteral("C0"));
    } catch (const std::exception&) {
      // The root then.
    }
  }
  if (home.component == QStringLiteral("C0")) {
    return home;
  }
  const QString target = str(kept, "target");
  if (!target.isEmpty() && occurrenceComponent(target, context) == home.component) {
    home.occurrence = target;
    return home;
  }
  bool found = false;
  eachOccurrence(context, [&home, &found](const QJsonObject& node, const QString& at) {
    if (!found && str(node, "component") == home.component) {
      home.occurrence = at;
      found = true;
    }
  });
  return home;
}

// A face, edge or vertex picked in an occurrence of another component than
// the datum's: its body's link into `links`, `{"source": <its occurrence>,
// "target": <the datum's>}` with an empty path (the root) left out. False
// when the body is linked from another occurrence already (one link per
// body).
bool addDatumLink(QJsonObject& links, const SelectionItem& item, const DatumHome& home,
                  const CommandContext& context) {
  if (item.kind != SelectKind::Face && item.kind != SelectKind::Edge && item.kind != SelectKind::Vertex) {
    return true;
  }
  const QString component = occurrenceComponent(item.occurrence, context);
  if (component.isEmpty() || component == home.component) {
    return true;
  }
  QJsonObject link;
  if (!item.occurrence.isEmpty()) {
    link.insert(QStringLiteral("source"), item.occurrence);
  }
  if (!home.occurrence.isEmpty()) {
    link.insert(QStringLiteral("target"), home.occurrence);
  }
  if (links.contains(item.owner) && links.value(item.owner).toObject() != link) {
    return false;
  }
  links.insert(item.owner, link);
  return true;
}

// Picks the model reads from a construction feature or a sketch (the item's
// owner) rather than from a body.
bool onDatumOrSketch(const SelectionItem& item) {
  return item.kind == SelectKind::Plane || item.kind == SelectKind::Axis || item.kind == SelectKind::Point ||
         item.kind == SelectKind::SketchCurve || item.kind == SelectKind::SketchPoint;
}

// A construction plane, axis or point (mitcad#1035), or a sketch curve or
// point (mitcad#1039), of another component than the datum's: a link for
// its feature (the datum's, or the sketch) into `datumLinks` (the model's
// `datum_links`), from the occurrence the view shows it at, as
// addDatumLink() links a body. Origin datums are every component's own and
// need none.
void addDatumRefLink(QJsonObject& datumLinks, const SelectionItem& item, const DatumHome& home,
                     const CommandContext& context) {
  if (!onDatumOrSketch(item)) {
    return;
  }
  static const QStringList originDatums = {QStringLiteral("xy"), QStringLiteral("xz"), QStringLiteral("yz"),
                                           QStringLiteral("x"),  QStringLiteral("y"),  QStringLiteral("z"),
                                           QStringLiteral("origin")};
  if (originDatums.contains(item.owner)) {
    return;
  }
  QString component;
  try {
    component = context
                    .queryObject({{QStringLiteral("query"), QStringLiteral("feature")},
                                  {QStringLiteral("uid"), item.owner}})
                    .value(QStringLiteral("component"))
                    .toString(QStringLiteral("C0"));
  } catch (const std::exception&) {
    return; // not a feature the model knows: left to the model
  }
  if (component == home.component) {
    return;
  }
  // The pick's occurrence while it places the datum's component, else the
  // component's first one (where the view shows its datums).
  QString source = item.occurrence;
  if (occurrenceComponent(source, context) != component) {
    source.clear();
    bool found = component == QStringLiteral("C0");
    eachOccurrence(context, [&component, &source, &found](const QJsonObject& node, const QString& at) {
      if (!found && str(node, "component") == component) {
        source = at;
        found = true;
      }
    });
  }
  QJsonObject link;
  if (!source.isEmpty()) {
    link.insert(QStringLiteral("source"), source);
  }
  if (!home.occurrence.isEmpty()) {
    link.insert(QStringLiteral("target"), home.occurrence);
  }
  datumLinks.insert(item.owner, link);
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
    if (field.kind == Field::Angle && type == QStringLiteral("tangent")) {
      // At the clicked side of the face by default (mitcad#446).
      input.defaultText = QStringLiteral("0 deg");
    }
    if (field.kind == Field::Tangent) {
      input.withOnPick(tangentReference, false);
    }
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
      // A sketch line's own sketch plane is the reference plane at first,
      // when the sketch is of the datum's component: another component's
      // origin plane, or its face picked nowhere, is no pick of this one
      // (mitcad#1039), so the user picks the reference plane then.
      input.withOnPick(
          [](const SelectionItem& item, CommandState& state, const CommandContext& context) {
            if (item.kind != SelectKind::SketchCurve || !state.items(QStringLiteral("plane")).isEmpty()) {
              return;
            }
            try {
              const QString sketchComponent = context
                                                  .queryObject({{QStringLiteral("query"), QStringLiteral("feature")},
                                                                {QStringLiteral("uid"), item.owner}})
                                                  .value(QStringLiteral("component"))
                                                  .toString(QStringLiteral("C0"));
              if (sketchComponent != homeOf(state, context).component) {
                return;
              }
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
  def.build = [fields, type, featureType](const CommandState& state, const CommandContext& context) {
    Built result;
    QJsonObject definition{{QStringLiteral("type"), type}};
    // Picks of other components' geometry, linked from where they were
    // picked (mitcad#1029), and of their construction geometry (mitcad#1035)
    // and sketches (mitcad#1039).
    const DatumHome home = homeOf(state, context);
    QJsonObject links;
    QJsonObject datumLinks;
    for (const FieldSpec& field : fields) {
      const QString key = QString::fromLatin1(field.key);
      if (isSelection(field.kind)) {
        for (const SelectionItem& item : state.items(key)) {
          if (!addDatumLink(links, item, home, context)) {
            return Built::failure(
                QObject::tr("Pick the geometry of %1 in one occurrence of it.")
                    .arg(componentName(occurrenceComponent(item.occurrence, context), context)),
                key);
          }
          addDatumRefLink(datumLinks, item, home, context);
        }
      }
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
          // A fraction: a unitless slot (mitcad#662).
          definition.insert(key, state.expression(key));
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
    if (!links.isEmpty()) {
      result.def.insert(QStringLiteral("links"), links);
    }
    if (!datumLinks.isEmpty()) {
      result.def.insert(QStringLiteral("datum_links"), datumLinks);
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
    // Geometry of other components (mitcad#1029) where its links saw it,
    // so that the picks show and are kept as they were made; the datum's
    // component and the links' target kept for `build`.
    const QJsonObject links = feature.value(QStringLiteral("links")).toObject();
    const QJsonObject datumLinks = feature.value(QStringLiteral("datum_links")).toObject();
    for (const FieldSpec& field : fields) {
      if (!isSelection(field.kind)) {
        continue;
      }
      for (SelectionItem& item : state.items(QString::fromLatin1(field.key))) {
        const bool onBody =
            item.kind == SelectKind::Face || item.kind == SelectKind::Edge || item.kind == SelectKind::Vertex;
        if (onBody && links.contains(item.owner)) {
          item.occurrence = str(links.value(item.owner).toObject(), "source");
        } else if (onDatumOrSketch(item) && datumLinks.contains(item.owner)) {
          item.occurrence = str(datumLinks.value(item.owner).toObject(), "source");
        }
      }
    }
    QJsonObject home;
    try {
      home.insert(QStringLiteral("component"),
                  context
                      .queryObject({{QStringLiteral("query"), QStringLiteral("feature")},
                                    {QStringLiteral("uid"), feature.value(QStringLiteral("uid")).toString()}})
                      .value(QStringLiteral("component"))
                      .toString(QStringLiteral("C0")));
    } catch (const std::exception&) {
      // Not known: the active component, as for a new datum.
    }
    if (!links.isEmpty()) {
      home.insert(QStringLiteral("target"), str(links.constBegin().value().toObject(), "target"));
    } else if (!datumLinks.isEmpty()) {
      home.insert(QStringLiteral("target"), str(datumLinks.constBegin().value().toObject(), "target"));
    }
    state.setObject(kHome, home);
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
        if (!path.contains(QStringLiteral("sketch")) && !path.contains(QStringLiteral("edges")) &&
            !path.contains(QStringLiteral("feature"))) {
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
    const QString kind = datum.value(QStringLiteral("type")).toString();
    QString geometry = datumText(datum);
    // A datum of a placed component also where the view shows it, at the
    // component's first occurrence (mitcad#1029), for logs and UI tests.
    try {
      const QString component = context
                                    .queryObject({{QStringLiteral("query"), QStringLiteral("feature")},
                                                  {QStringLiteral("uid"), datum.value(QStringLiteral("uid"))}})
                                    .value(QStringLiteral("component"))
                                    .toString(QStringLiteral("C0"));
      if (component != QStringLiteral("C0")) {
        QJsonArray world;
        bool found = false;
        eachOccurrence(context, [&component, &world, &found](const QJsonObject& node, const QString&) {
          if (!found && str(node, "component") == component) {
            world = node.value(QStringLiteral("world")).toArray();
            found = true;
          }
        });
        if (found) {
          geometry += QStringLiteral("; in the design: %1").arg(datumText(placedDatum(datum, trsfOf(world))));
        }
      }
    } catch (const std::exception&) {
      // Its own coordinates only.
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
