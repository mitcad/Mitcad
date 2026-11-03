// SPDX-License-Identifier: MIT
#include "BuildModel.hpp"

#include <cmath>
#include <exception>

#include <QObject>

#include <BRepPrimAPI_MakeBox.hxx>
#include <gp_Ax2.hxx>
#include <gp_Vec.hxx>

#include "../commands/CommandSupport.hpp"
#include "../framework/ModelShapes.hpp"

namespace mitcad::build {
namespace {

const QString kRoot = QStringLiteral("C0");

std::optional<gp_Dir> directionFrom(const QJsonValue& value) {
  const QJsonArray xyz = value.toArray();
  if (xyz.size() < 3) {
    return std::nullopt;
  }
  const gp_Vec vector(xyz[0].toDouble(), xyz[1].toDouble(), xyz[2].toDouble());
  if (vector.Magnitude() < 1e-12) {
    return std::nullopt;
  }
  return gp_Dir(vector);
}

// A rotation about a part's axis (radians, as the replies give angles) in
// quarter turns, 0 to 3.
int quarterTurns(const QJsonValue& value) {
  const double radians = value.toDouble();
  if (!value.isDouble() || !std::isfinite(radians)) {
    return 0;
  }
  const long turns = std::lround(radians / (3.14159265358979323846 / 2.0));
  return static_cast<int>(((turns % 4) + 4) % 4);
}

BuildPart buildPartOf(const QJsonObject& entry) {
  BuildPart part;
  part.raw = entry;
  part.component = str(entry, "component");
  part.name = str(entry, "name");
  part.role = str(entry, "role");
  part.source = str(entry, "source");
  part.designation = str(entry, "designation");
  part.supplier = str(entry, "supplier");
  part.partNumber = str(entry, "part_number");
  part.config = str(entry, "config");
  part.addedBy = str(entry, "added_by");
  part.length = lengthOf(entry.value(QStringLiteral("length")));
  part.ends = entry.value(QStringLiteral("ends")).toObject();
  part.bound = entry.value(QStringLiteral("bound")).toObject();
  part.adds = entry.value(QStringLiteral("adds")).toArray();
  part.machining = entry.value(QStringLiteral("machining")).toArray();
  part.flip = entry.value(QStringLiteral("flip")).toBool();
  part.rotate = quarterTurns(entry.value(QStringLiteral("rotate"))) * 90;
  part.kind = str(entry, "kind");
  part.grade = str(entry, "grade");
  part.set = str(entry, "set");
  part.spacing = entry.value(QStringLiteral("spacing")).toObject();
  return part;
}

} // namespace

std::optional<gp_Trsf> worldOf(const QString& path, const CommandContext& context) {
  if (path.isEmpty()) {
    return gp_Trsf();
  }
  std::optional<gp_Trsf> found;
  cmd::eachOccurrence(context, [&](const QJsonObject& node, const QString& at) {
    if (at == path) {
      found = trsfOf(node.value(QStringLiteral("world")).toArray());
    }
  });
  return found;
}

QString componentAt(const QString& path, const CommandContext& context) {
  if (path.isEmpty()) {
    return kRoot;
  }
  QString component;
  cmd::eachOccurrence(context, [&](const QJsonObject& node, const QString& at) {
    if (at == path) {
      component = str(node, "component");
    }
  });
  return component;
}

Host buildHost(const CommandContext& context) {
  Host host;
  try {
    const QJsonObject answer = context.queryObject({{QStringLiteral("query"), QStringLiteral("build_host")}});
    host.component = answer.value(QStringLiteral("component")).toString(kRoot);
    host.name = str(answer, "name");
    host.path = str(answer, "path");
  } catch (const std::exception&) {
    // The root then.
  }
  if (host.name.isEmpty()) {
    host.name = cmd::componentName(host.component, context);
  }
  return host;
}

std::optional<QString> pathFromHost(const SelectionItem& item, const CommandContext& context) {
  const QString prefix = buildHost(context).path;
  if (prefix.isEmpty()) {
    return item.occurrence;
  }
  if (item.occurrence == prefix) {
    return QString();
  }
  if (item.occurrence.startsWith(prefix + QLatin1Char('/'))) {
    return item.occurrence.mid(prefix.size() + 1);
  }
  return std::nullopt;
}

QString rootPath(const QString& fromHost, const CommandContext& context) {
  const QString prefix = buildHost(context).path;
  if (prefix.isEmpty()) {
    return fromHost;
  }
  return fromHost.isEmpty() ? prefix : prefix + QLatin1Char('/') + fromHost;
}

gp_Pnt inHost(const gp_Pnt& world, const CommandContext& context) {
  const QString path = buildHost(context).path;
  if (path.isEmpty()) {
    return world;
  }
  const std::optional<gp_Trsf> placement = worldOf(path, context);
  if (!placement) {
    return world;
  }
  return world.Transformed(placement->Inverted());
}

QJsonObject pickRecord(const SelectionItem& item, const CommandContext& context) {
  if (!item.at) {
    return {};
  }
  gp_Pnt point((*item.at)[0], (*item.at)[1], (*item.at)[2]);
  // `at` is in the coordinates of the item's component.
  if (const std::optional<gp_Trsf> placement = worldOf(item.occurrence, context)) {
    point.Transform(*placement);
  }
  QJsonObject record{{QStringLiteral("item"), item.describe()}, {QStringLiteral("point"), pointJson(point)}};
  // The snap distance there: kSnapPixels on the screen (mitcad#417).
  const double pixel = context.pixelSize(point);
  if (std::isfinite(pixel) && pixel > 0.0) {
    record.insert(QStringLiteral("snap"), kSnapPixels * pixel);
  }
  return record;
}

void recordPick(const QString& key, const SelectionItem& item, CommandState& state, const CommandContext& context) {
  const QJsonObject record = pickRecord(item, context);
  if (!record.isEmpty()) {
    state.setObject(key + QStringLiteral("_pick"), record);
  }
}

std::optional<double> pickSnap(const QString& key, const CommandState& state) {
  const Selection& items = state.items(key);
  const QJsonObject pick = state.object(key + QStringLiteral("_pick"));
  const double snap = pick.value(QStringLiteral("snap")).toDouble(-1.0);
  if (items.isEmpty() || str(pick, "item") != items.first().describe() || !std::isfinite(snap) || snap <= 0.0) {
    return std::nullopt;
  }
  return snap;
}

std::optional<gp_Pnt> pickPoint(const QString& key, const CommandState& state, const CommandContext& context) {
  const Selection& items = state.items(key);
  if (items.isEmpty()) {
    return std::nullopt;
  }
  const QJsonObject pick = state.object(key + QStringLiteral("_pick"));
  if (str(pick, "item") == items.first().describe()) {
    if (const auto point = pointFrom(pick.value(QStringLiteral("point")))) {
      return point;
    }
  }
  return cmd::centerOf({items.first()}, context);
}

std::optional<QJsonObject> atOf(const QString& key, const CommandState& state, const CommandContext& context,
                                QString& problem) {
  const Selection& items = state.items(key);
  if (items.isEmpty()) {
    problem = QObject::tr("Pick where it goes.");
    return std::nullopt;
  }
  const SelectionItem& item = items.first();
  // A plane chosen without a point (mitcad#1196: the first part on a plane
  // before the click that says where, or Enter): at the plane's origin.
  const QJsonObject pick = state.object(key + QStringLiteral("_pick"));
  const bool atOrigin = item.kind == SelectKind::Plane && str(pick, "item") == item.describe() &&
                        !pick.contains(QStringLiteral("point"));
  const std::optional<QString> path = pathFromHost(item, context);
  if (!path) {
    // An origin or construction plane or point above the host (the root's
    // origin planes while a new design's Component1 is the host,
    // mitcad#421): the first part is grounded at the click, in the host's
    // coordinates, as Enter grounds it at the origin. (A BUILD part to go
    // on is always under the host.)
    if (item.kind == SelectKind::Plane || item.kind == SelectKind::Point) {
      std::optional<gp_Pnt> point;
      if (!atOrigin) {
        point = pickPoint(key, state, context);
      } else if (const std::optional<gp_Ax3> plane = planeOf(item, context)) {
        point = plane->Location();
      }
      if (point) {
        return QJsonObject{{QStringLiteral("point"), pointJson(inHost(*point, context))}};
      }
    }
    problem = QObject::tr("%1 is not in %2, where the Build tab places components.").arg(item.describe(), buildHost(context).name);
    return std::nullopt;
  }
  QJsonObject at{{QStringLiteral("geometry"), item.reference()}};
  if (!path->isEmpty()) {
    at.insert(QStringLiteral("occurrence"), *path);
  }
  // Without a point the model takes the plane's own origin.
  if (const std::optional<gp_Pnt> point = atOrigin ? std::nullopt : pickPoint(key, state, context)) {
    at.insert(QStringLiteral("point"), pointJson(inHost(*point, context)));
  }
  return at;
}

std::optional<gp_Ax3> planeOf(const SelectionItem& item, const CommandContext& context) {
  if (item.kind != SelectKind::Plane) {
    return std::nullopt;
  }
  const std::optional<cmd::PlaneFrame> frame = cmd::frameOf(item, context);
  if (!frame) {
    return std::nullopt;
  }
  gp_Ax3 plane(frame->origin, frame->normal, frame->x);
  // The datum's frame is in the coordinates of the item's component.
  if (const std::optional<gp_Trsf> placement = worldOf(item.occurrence, context)) {
    plane.Transform(*placement);
  }
  return plane;
}

std::optional<SelectionItem> itemOfTarget(const QJsonObject& target, const CommandContext& context) {
  const QJsonValue geometry = target.value(QStringLiteral("geometry"));
  const QString occurrence = rootPath(str(target, "occurrence"), context);
  if (geometry.isString()) {
    return SelectionItem{SelectKind::Plane, geometry.toString(), QString(), QString(), occurrence};
  }
  const QJsonObject reference = geometry.toObject();
  const QString body = str(reference, "body");
  if (body.isEmpty()) {
    return std::nullopt; // a frame (`mount`) is no item of the view
  }
  if (reference.contains(QStringLiteral("face"))) {
    return SelectionItem{SelectKind::Face, body, str(reference, "face"), QString(), occurrence};
  }
  if (reference.contains(QStringLiteral("edge"))) {
    return SelectionItem{SelectKind::Edge, body, str(reference, "edge"), QString(), occurrence};
  }
  return SelectionItem{SelectKind::Body, body, QString(), QString(), occurrence};
}

QVector<BuildPart> buildParts(const CommandContext& context) {
  QVector<BuildPart> parts;
  try {
    const QJsonObject answer = context.queryObject({{QStringLiteral("query"), QStringLiteral("build_parts")}});
    for (const QJsonValue& value : answer.value(QStringLiteral("parts")).toArray()) {
      parts.append(buildPartOf(value.toObject()));
    }
  } catch (const std::exception&) {
    // None the model can tell.
  }
  return parts;
}

bool hasBuildParts(const CommandContext& context) { return !buildParts(context).isEmpty(); }

std::optional<BuildPart> partOf(const SelectionItem& item, const CommandContext& context) {
  // The innermost BUILD component on the item's path: a part's own
  // features may put its bodies in it, never deeper.
  QStringList steps = item.kind == SelectKind::Component && !item.owner.isEmpty() && item.occurrence.isEmpty()
                          ? QStringList()
                          : item.occurrence.split(QLatin1Char('/'), Qt::SkipEmptyParts);
  const QVector<BuildPart> parts = buildParts(context);
  while (!steps.isEmpty()) {
    const QString component = componentAt(steps.join(QLatin1Char('/')), context);
    for (const BuildPart& part : parts) {
      if (part.component == component) {
        return part;
      }
    }
    steps.removeLast();
  }
  if (item.kind == SelectKind::Component) {
    for (const BuildPart& part : parts) {
      if (part.component == item.owner) {
        return part;
      }
    }
  }
  return std::nullopt;
}

QVector<BuildPart> partsOf(const Selection& items, const CommandContext& context) {
  QVector<BuildPart> parts;
  for (const SelectionItem& item : items) {
    const std::optional<BuildPart> part = partOf(item, context);
    if (!part) {
      continue;
    }
    bool known = false;
    for (const BuildPart& other : parts) {
      known = known || other.component == part->component;
    }
    if (!known) {
      parts.append(*part);
    }
  }
  return parts;
}

QJsonArray junctions(const CommandContext& context) {
  try {
    return context.queryObject({{QStringLiteral("query"), QStringLiteral("build_junctions")}})
        .value(QStringLiteral("junctions"))
        .toArray();
  } catch (const std::exception&) {
    return {};
  }
}

std::optional<gp_Pnt> junctionPoint(const QJsonObject& junction) {
  for (const char* side : {"a", "b"}) {
    // Where the two meet: the `at` frame (a's end, the slot), else the part's own.
    const QJsonObject part = junction.value(QLatin1String(side)).toObject();
    Frame frame = frameOf(part.value(QStringLiteral("at_frame")));
    if (!frame.valid) {
      frame = frameOf(part.value(QStringLiteral("frame")));
    }
    if (frame.valid) {
      return frame.origin;
    }
  }
  return std::nullopt;
}

QString junctionText(const QJsonObject& junction, const CommandContext& context) {
  const auto partName = [&context](const QJsonObject& side) {
    return cmd::componentName(str(side, "component"), context);
  };
  QString shown = QObject::tr("%1 junction of %2 and %3")
                      .arg(str(junction, "kind"), partName(junction.value(QStringLiteral("a")).toObject()),
                           partName(junction.value(QStringLiteral("b")).toObject()));
  if (!junction.value(QStringLiteral("connectors")).toArray().isEmpty()) {
    shown += QObject::tr(", connected");
  }
  return shown;
}

Frame frameOf(const QJsonValue& value) {
  // {"origin", "x_axis", "z_axis"} in the design (commands.md, "BUILD").
  Frame frame;
  const QJsonObject object = value.toObject();
  const std::optional<gp_Pnt> origin = pointFrom(object.value(QStringLiteral("origin")));
  const std::optional<gp_Dir> z = directionFrom(object.value(QStringLiteral("z_axis")));
  if (!origin || !z) {
    return frame;
  }
  frame.origin = *origin;
  frame.z = *z;
  if (const std::optional<gp_Dir> x = directionFrom(object.value(QStringLiteral("x_axis")))) {
    frame.x = *x;
  }
  if (frame.x.IsParallel(frame.z, 1e-9)) {
    frame.x = std::abs(frame.z.X()) < 0.9 ? gp_Dir(1, 0, 0) : gp_Dir(0, 1, 0);
  }
  frame.valid = true;
  return frame;
}

double lengthOf(const QJsonValue& value) {
  // Millimetres: a number, or build_preview's {"value", "target", "mode"}.
  if (value.isDouble()) {
    return value.toDouble();
  }
  const QJsonValue number = value.toObject().value(QStringLiteral("value"));
  return number.isDouble() ? number.toDouble() : std::numeric_limits<double>::quiet_NaN();
}

TopoDS_Shape boxAlong(const Frame& frame, double width, double height, double length) {
  if (!frame.valid || !(width > 0.0) || !(height > 0.0) || !(std::abs(length) > 1e-6)) {
    return TopoDS_Shape();
  }
  try {
    gp_Dir z = frame.z;
    if (length < 0.0) {
      z.Reverse();
    }
    const gp_Dir y = z.Crossed(frame.x);
    const gp_Dir x = y.Crossed(z);
    const gp_Pnt corner =
        frame.origin.Translated(gp_Vec(x) * (-width / 2.0) + gp_Vec(y) * (-height / 2.0));
    return BRepPrimAPI_MakeBox(gp_Ax2(corner, z, x), width, height, std::abs(length)).Shape();
  } catch (...) {
    return TopoDS_Shape(); // OCCT's Standard_Failure: no preview then
  }
}

QJsonArray pointJson(const gp_Pnt& point) { return QJsonArray{point.X(), point.Y(), point.Z()}; }

} // namespace mitcad::build
