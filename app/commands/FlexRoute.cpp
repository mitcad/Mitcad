// SPDX-License-Identifier: MIT
// The Flex command's route while it is laid (mitcad#1713): see FlexRoute.hpp.
#include "FlexRoute.hpp"

#include <algorithm>
#include <cmath>

#include <QSet>

#include <gp_Vec.hxx>

namespace mitcad::cmd {
namespace {

const QString kAt = QStringLiteral("at");
const QString kOn = QStringLiteral("on");
const QString kPort = QStringLiteral("port");
const QString kTwist = QStringLiteral("twist");
const QString kId = QStringLiteral("id");
const QString kSection = QStringLiteral("section");
const QString kWall = QStringLiteral("wall");
const QString kAppearance = QStringLiteral("appearance");

bool closeTo(double a, double b) { return std::abs(a - b) <= 1e-9 * std::max(1.0, std::max(std::abs(a), std::abs(b))); }

} // namespace

QJsonArray flexXyz(const gp_Pnt& point) {
  QJsonArray xyz;
  xyz.append(point.X());
  xyz.append(point.Y());
  xyz.append(point.Z());
  return xyz;
}

gp_Pnt flexPoint(const QJsonValue& xyz) {
  const QJsonArray values = xyz.toArray();
  return gp_Pnt(values.at(0).toDouble(), values.at(1).toDouble(), values.at(2).toDouble());
}

std::vector<FlexPort> flexPorts(const QJsonArray& answer) {
  std::vector<FlexPort> ports;
  for (const QJsonValue& value : answer) {
    const QJsonObject object = value.toObject();
    FlexPort port;
    port.feature = object.value(QStringLiteral("feature")).toString();
    port.kind = object.value(QStringLiteral("kind")).toString();
    port.end =object.value(QStringLiteral("end")).toString();
    port.ref = object.value(QStringLiteral("ref")).toObject();
    if (port.ref.value(QStringLiteral("body")).toString().isEmpty() ||
        port.ref.value(QStringLiteral("face")).toString().isEmpty()) {
      continue;
    }
    port.position = flexPoint(object.value(QStringLiteral("position")));
    const gp_Pnt along = flexPoint(object.value(QStringLiteral("direction")));
    const gp_Vec direction(along.X(), along.Y(), along.Z());
    if (direction.Magnitude() > 1e-9) {
      port.direction = gp_Dir(direction);
    }
    const QJsonArray size = object.value(QStringLiteral("size")).toArray();
    port.width = size.at(0).toDouble();
    port.height = size.at(1).toDouble();
    port.reach = std::max(port.width, port.height);
    ports.push_back(port);
  }
  return ports;
}

std::vector<FlexPort> flexPortsOf(const std::vector<FlexPort>& ports, const QString& body) {
  std::vector<FlexPort> own;
  for (const FlexPort& port : ports) {
    if (port.ref.value(QStringLiteral("body")).toString() == body) {
      own.push_back(port);
    }
  }
  return own;
}

std::optional<FlexPort> flexPortAt(const std::vector<FlexPort>& ports, const QString& body, const QString& face,
                                   const gp_Pnt& at) {
  std::optional<FlexPort> nearest;
  double distance = 0.0;
  for (const FlexPort& port : flexPortsOf(ports, body)) {
    if (!face.isEmpty() && port.ref.value(QStringLiteral("face")).toString() == face) {
      return port;
    }
    const double away = port.position.Distance(at);
    if (away <= port.reach && (!nearest || away < distance)) {
      nearest = port;
      distance = away;
    }
  }
  return nearest;
}

QJsonObject flexFreeNode(const gp_Pnt& at) { return {{kAt, flexXyz(at)}}; }

bool flexIsPort(const QJsonObject& node) { return node.contains(kPort); }

bool flexSamePlace(const QJsonObject& a, const QJsonObject& b, double tolerance) {
  if (a.value(kOn) != b.value(kOn) || a.value(kPort) != b.value(kPort)) {
    return false;
  }
  if (flexIsPort(a)) {
    return true; // a port is one place
  }
  return flexPoint(a.value(kAt)).Distance(flexPoint(b.value(kAt))) <= tolerance;
}

QJsonArray flexDefinitionNodes(const QJsonArray& nodes) {
  QSet<QString> used;
  for (const QJsonValue& value : nodes) {
    const QString id = value.toObject().value(kId).toString();
    if (!id.isEmpty()) {
      used.insert(id);
    }
  }
  int next = 1;
  QJsonArray result;
  for (const QJsonValue& value : nodes) {
    const QJsonObject node = value.toObject();
    QString id = node.value(kId).toString();
    if (id.isEmpty()) {
      do {
        id = QStringLiteral("n%1").arg(next++);
      } while (used.contains(id));
      used.insert(id);
    }
    QJsonObject out{{kId, id}, {kAt, node.value(kAt)}};
    if (node.contains(kPort)) {
      out.insert(kPort, node.value(kPort));
    } else if (node.contains(kOn)) {
      out.insert(kOn, node.value(kOn));
    }
    const double twist = node.value(kTwist).toDouble();
    if (std::isfinite(twist) && std::abs(twist) > 1e-12) {
      out.insert(kTwist, twist);
    }
    result.append(out);
  }
  return result;
}

bool flexPortInside(const QJsonArray& nodes) {
  for (int i = 1; i + 1 < nodes.size(); ++i) {
    if (flexIsPort(nodes.at(i).toObject())) {
      return true;
    }
  }
  return false;
}

QJsonObject flexItemOfType(const QJsonObject& type) {
  QJsonObject item = type;
  QJsonObject origin{{QStringLiteral("library"), type.value(QStringLiteral("library")).toString(QStringLiteral("built_in"))},
                     {QStringLiteral("id"), type.value(QStringLiteral("id"))}};
  if (type.contains(QStringLiteral("version"))) {
    origin.insert(QStringLiteral("version"), type.value(QStringLiteral("version")));
  }
  for (const char* own : {"id", "library", "built_in", "version"}) {
    item.remove(QString::fromLatin1(own));
  }
  item.insert(QStringLiteral("origin"), origin);
  return item;
}

bool flexIsRound(const QJsonObject& item) {
  return item.value(kSection).toObject().value(QStringLiteral("type")).toString(QStringLiteral("round")) ==
         QStringLiteral("round");
}

FlexSection flexSectionOf(const QJsonObject& item) {
  FlexSection shown;
  const QJsonObject section = item.value(kSection).toObject();
  shown.type = section.value(QStringLiteral("type")).toString(QStringLiteral("round"));
  shown.diameter = section.value(QStringLiteral("diameter")).toDouble();
  shown.width = section.value(QStringLiteral("width")).toDouble();
  shown.height = section.value(QStringLiteral("height")).toDouble();
  shown.cornerRadius = section.value(QStringLiteral("corner_radius")).toDouble();
  shown.wall = item.value(kWall).toDouble();
  shown.hollow = shown.wall > 0.0;
  shown.appearance = item.value(kAppearance).toString();
  return shown;
}

QJsonObject flexItemWith(const QJsonObject& base, const FlexSection& section, const QString& name) {
  const FlexSection own = flexSectionOf(base);
  bool same = section.type == own.type && section.hollow == own.hollow && section.appearance == own.appearance &&
              (!section.hollow || closeTo(section.wall, own.wall));
  if (same && section.type == QStringLiteral("round")) {
    same = closeTo(section.diameter, own.diameter);
  } else if (same && section.type == QStringLiteral("rectangular")) {
    same = closeTo(section.width, own.width) && closeTo(section.height, own.height) &&
           closeTo(section.cornerRadius, own.cornerRadius);
  }
  if (same && (name.isEmpty() || name == base.value(QStringLiteral("name")).toString())) {
    return base;
  }
  QJsonObject item = base;
  item.remove(QStringLiteral("origin"));
  if (!name.isEmpty()) {
    item.insert(QStringLiteral("name"), name);
  }
  if (section.type == QStringLiteral("round")) {
    item.insert(kSection, QJsonObject{{QStringLiteral("type"), section.type},
                                      {QStringLiteral("diameter"), section.diameter}});
  } else if (section.type == QStringLiteral("rectangular")) {
    QJsonObject rectangle{{QStringLiteral("type"), section.type},
                          {QStringLiteral("width"), section.width},
                          {QStringLiteral("height"), section.height}};
    if (section.cornerRadius > 0.0) {
      rectangle.insert(QStringLiteral("corner_radius"), section.cornerRadius);
    }
    item.insert(kSection, rectangle);
  }
  if (section.type != own.type) {
    // The limits of the other kind of section do not carry over: a round
    // one has one bend radius and no twist limit.
    if (section.type == QStringLiteral("round")) {
      item.remove(QStringLiteral("bend_radius_edge"));
      item.remove(QStringLiteral("max_twist"));
    }
  }
  if (section.type != QStringLiteral("outline")) {
    if (section.hollow) {
      item.insert(kWall, section.wall);
    } else {
      item.remove(kWall);
    }
  }
  if (section.appearance.isEmpty()) {
    item.remove(kAppearance);
  } else {
    item.insert(kAppearance, section.appearance);
  }
  return item;
}

std::optional<FlexSection> flexSectionAtPort(const FlexPort& port, const QJsonObject& feature,
                                             const FlexSection& current) {
  if (!std::isfinite(port.width) || !std::isfinite(port.height) || port.width <= 0.0 || port.height <= 0.0) {
    return std::nullopt;
  }
  const QString roundShape = QStringLiteral("round");
  const QString rectangular = QStringLiteral("rectangular");
  const QString triangular = QStringLiteral("triangular");
  // The shape of the port's section: from the feature's definition, else
  // (the query failed) a guess from the cap's sizes.
  const QString type = feature.value(QStringLiteral("type")).toString(port.kind);
  const QJsonObject def = feature.value(QStringLiteral("def")).toObject();
  QString shape;
  double corner = 0.0;
  if (type == QStringLiteral("flex") && !def.isEmpty()) {
    const QJsonObject own = def.value(QStringLiteral("item")).toObject().value(kSection).toObject();
    shape = own.value(QStringLiteral("type")).toString(roundShape);
    if (shape != roundShape && shape != rectangular) {
      return std::nullopt; // an outline: not a section the panel's sizes give
    }
    corner = own.value(QStringLiteral("corner_radius")).toDouble();
  } else if (type == QStringLiteral("pipe") && !def.isEmpty()) {
    const QString own = def.value(kSection).toString(QStringLiteral("circular"));
    shape = own == QStringLiteral("square") ? rectangular : own == triangular ? triangular : roundShape;
  } else {
    shape = closeTo(port.width, port.height) ? roundShape : rectangular;
  }
  FlexSection section = current;
  section.cornerRadius = 0.0;
  if (shape == triangular) {
    // A Flex item has no triangular section: the round one round the
    // triangle, its circumscribed circle (owner 13.11.2026). The cap's sizes
    // are the equilateral triangle's side and height, the side the larger;
    // the circle through its corners has the diameter 2 * side / sqrt(3).
    section.type = roundShape;
    section.diameter = 2.0 * std::max(port.width, port.height) / std::sqrt(3.0);
  } else if (shape == rectangular) {
    section.type = rectangular;
    section.width = port.width;
    section.height = port.height;
    if (std::isfinite(corner) && corner > 0.0 && 2.0 * corner < std::min(port.width, port.height)) {
      section.cornerRadius = corner;
    }
  } else {
    // A hollow pipe's cap is its outer boundary: the same outer diameter.
    section.type = roundShape;
    section.diameter = std::max(port.width, port.height);
  }
  // The panel's wall where the new section has room for it, else solid.
  const double smallest = section.type == roundShape ? section.diameter : std::min(section.width, section.height);
  if (section.hollow && !(std::isfinite(section.wall) && 2.0 * section.wall < smallest)) {
    section.hollow = false;
  }
  return section;
}

} // namespace mitcad::cmd
