// SPDX-License-Identifier: MIT
#include "framework/JointHighlight.hpp"

#include <utility>

#include <QChar>

namespace mitcad {

namespace {

std::array<double, 3> vectorOf(const QJsonValue& value) {
  const QJsonArray array = value.toArray();
  return {array.at(0).toDouble(), array.at(1).toDouble(), array.at(2).toDouble()};
}

std::optional<JointFrame> frameOf(const QJsonValue& value) {
  const QJsonObject frame = value.toObject();
  if (!frame.value(QStringLiteral("origin")).isArray() || !frame.value(QStringLiteral("z_axis")).isArray()) {
    return std::nullopt;
  }
  JointFrame result;
  result.origin = vectorOf(frame.value(QStringLiteral("origin")));
  result.x = vectorOf(frame.value(QStringLiteral("x_axis")));
  result.y = vectorOf(frame.value(QStringLiteral("y_axis")));
  result.z = vectorOf(frame.value(QStringLiteral("z_axis")));
  return result;
}

JointSide sideOf(const QJsonObject& value, const QString& key, int row, const QString& placing) {
  JointSide side;
  side.key = key;
  side.row = row;
  side.on = value.value(QStringLiteral("on")).toString();
  side.geometry = value.value(QStringLiteral("geometry"));
  if (side.on.isEmpty()) {
    // An answer without `on`: an as-built joint's side is its occurrence.
    side.on = side.geometry.isUndefined() ? QStringLiteral("occurrence") : QString();
  }
  for (const QJsonValue& uid : value.value(QStringLiteral("path")).toArray()) {
    side.ownPath << uid.toString();
  }
  side.path = joinedPath(placing, side.ownPath);
  side.occurrenceName = value.value(QStringLiteral("occurrence")).toString();
  const QJsonObject overrideFields = value.value(QStringLiteral("frame_override")).toObject();
  side.frameOverride = value.value(QStringLiteral("frame_override")).isObject();
  // An override that only shifts the origin along the frame follows the
  // geometry (BUILD's corners): the geometry is still shown.
  const bool moved = overrideFields.contains(QStringLiteral("origin")) ||
                     overrideFields.contains(QStringLiteral("z_axis")) ||
                     overrideFields.contains(QStringLiteral("x_axis"));
  const bool ownGeometry = side.on != QStringLiteral("fixed") && side.on != QStringLiteral("mount") &&
                           side.on != QStringLiteral("occurrence");
  side.showsGeometry = ownGeometry && !moved;
  side.showsPart = side.on == QStringLiteral("mount") || side.on == QStringLiteral("occurrence");
  return side;
}

// What the side is on, `body` naming a body uid and `name` a face, edge or
// vertex name.
QString geometryText(const JointSide& side, const std::function<QString(const QString&)>& body,
                     const std::function<QString(const QString&)>& name) {
  const QJsonObject o = side.geometry.toObject();
  const QString text = side.geometry.toString();
  const QString& on = side.on;
  for (const char* kind : {"face", "edge", "vertex"}) {
    if (on == QLatin1String(kind)) {
      return QStringLiteral("%1 %2 of %3")
          .arg(QLatin1String(kind), name(o.value(QLatin1String(kind)).toString()),
               body(o.value(QStringLiteral("body")).toString()));
    }
  }
  if (on == QStringLiteral("body")) {
    return QStringLiteral("body %1").arg(body(o.value(QStringLiteral("body")).toString()));
  }
  const QString sketch = o.value(QStringLiteral("sketch")).toString();
  if (on == QStringLiteral("sketch_curve")) {
    if (o.contains(QStringLiteral("curve"))) {
      return QStringLiteral("curve %1 of %2").arg(o.value(QStringLiteral("curve")).toString(), sketch);
    }
    QStringList curves;
    for (const QJsonValue& curve : o.value(QStringLiteral("curves")).toArray()) {
      curves << curve.toString();
    }
    return curves.isEmpty() ? QStringLiteral("curves of %1").arg(sketch)
                            : QStringLiteral("curves %1 of %2").arg(curves.join(QStringLiteral(", ")), sketch);
  }
  if (on == QStringLiteral("sketch_point")) {
    return QStringLiteral("point %1 of %2").arg(o.value(QStringLiteral("point")).toString(), sketch);
  }
  if (on == QStringLiteral("origin")) {
    if (text == QStringLiteral("origin")) {
      return QStringLiteral("the origin");
    }
    return QStringLiteral("origin %1 %2").arg(text.size() == 2 ? QStringLiteral("plane") : QStringLiteral("axis"), text);
  }
  if (on == QStringLiteral("construction")) {
    return QStringLiteral("construction geometry %1").arg(text);
  }
  if (on == QStringLiteral("joint_origin")) {
    return QStringLiteral("joint origin %1").arg(text);
  }
  if (on == QStringLiteral("fixed")) {
    return o.contains(QStringLiteral("normal"))      ? QStringLiteral("fixed plane")
           : o.contains(QStringLiteral("direction")) ? QStringLiteral("fixed axis")
                                                     : QStringLiteral("fixed point");
  }
  if (on == QStringLiteral("mount")) {
    // "F7:mount(end_a)": the frame end_a of the derived part F7.
    const QString mount = o.value(QStringLiteral("mount")).toString();
    const int open = mount.indexOf(QStringLiteral(":mount("));
    if (open > 0 && mount.endsWith(QLatin1Char(')'))) {
      const int start = open + 7;
      return QStringLiteral("mount frame %1 of %2").arg(mount.mid(start, mount.size() - start - 1), mount.left(open));
    }
    return QStringLiteral("mount frame %1").arg(mount);
  }
  if (on == QStringLiteral("occurrence")) {
    return QStringLiteral("occurrence");
  }
  return on;
}

QString sideKey(const JointSide& side) {
  return side.row > 0 ? QStringLiteral("row %1 %2").arg(side.row).arg(side.key) : side.key;
}

} // namespace

QString joinedPath(const QString& placing, const QStringList& path) {
  QStringList parts;
  if (!placing.isEmpty()) {
    parts << placing;
  }
  parts << path;
  return parts.join(QLatin1Char('/'));
}

JointHighlight jointHighlightOf(const QJsonObject& entry, const QString& placing) {
  JointHighlight joint;
  joint.uid = entry.value(QStringLiteral("uid")).toString();
  joint.name = entry.value(QStringLiteral("name")).toString();
  joint.placing = placing;
  if (entry.contains(QStringLiteral("rows"))) {
    joint.kind = QStringLiteral("constraint");
    const QJsonArray rows = entry.value(QStringLiteral("rows")).toArray();
    for (int i = 0; i < rows.size(); ++i) {
      const QJsonObject row = rows.at(i).toObject();
      for (const char* key : {"a", "a2", "b", "b2"}) {
        if (row.value(QLatin1String(key)).isObject()) {
          joint.sides.append(sideOf(row.value(QLatin1String(key)).toObject(), QLatin1String(key), i + 1, placing));
        }
      }
    }
  } else {
    joint.kind = entry.value(QStringLiteral("kind")).toString();
    const QJsonObject frames = entry.value(QStringLiteral("frames")).toObject();
    for (const char* key : {"a", "b", "origin"}) {
      if (!entry.value(QLatin1String(key)).isObject()) {
        continue;
      }
      JointSide side = sideOf(entry.value(QLatin1String(key)).toObject(), QLatin1String(key), 0, placing);
      side.frame = frameOf(frames.value(QLatin1String(key)));
      joint.sides.append(side);
    }
  }
  // The joined occurrences: the sides' own (an as-built joint's motion
  // origin lies on one of them; the component's own geometry is none).
  for (const JointSide& side : std::as_const(joint.sides)) {
    if (side.key == QStringLiteral("origin") || side.ownPath.isEmpty() || joint.occurrences.contains(side.path)) {
      continue;
    }
    joint.occurrences << side.path;
    joint.occurrenceNames << side.occurrenceName;
  }
  return joint;
}

QString jointSideGeometryText(const JointSide& side) {
  const auto same = [](const QString& text) { return text; };
  return geometryText(side, same, same);
}

QStringList jointHighlightLog(const JointHighlight& joint) {
  QStringList lines;
  for (const JointSide& side : joint.sides) {
    const QString where = side.occurrenceName.isEmpty()
                              ? QStringLiteral(" of the joint's component")
                              : QStringLiteral(" in %1 (%2)").arg(side.occurrenceName, side.path);
    QString how;
    if (side.on == QStringLiteral("mount")) {
      how = QStringLiteral(", frame and part");
    } else if (!side.showsGeometry && side.on != QStringLiteral("occurrence")) {
      how = QStringLiteral(", frame only");
    }
    lines << QStringLiteral("Joint highlight %1: %2%3%4 [%5]")
                 .arg(sideKey(side), jointSideGeometryText(side), where, how, joint.name);
  }
  QStringList occurrences;
  for (int i = 0; i < joint.occurrences.size(); ++i) {
    occurrences << QStringLiteral("%1 (%2)").arg(joint.occurrenceNames.value(i), joint.occurrences.at(i));
  }
  lines << QStringLiteral("Joint highlight occurrences: %1 [%2]")
               .arg(occurrences.isEmpty() ? QStringLiteral("none") : occurrences.join(QStringLiteral(", ")),
                    joint.name);
  return lines;
}

namespace {

// What the side is on in words, for the card (mitcad#1741): the kind of
// geometry and its owner's name, never a topological name or an id; `name`
// names a body or feature uid (empty when it cannot).
QString cardGeometryText(const JointSide& side, const std::function<QString(const QString&)>& name) {
  const QJsonObject o = side.geometry.toObject();
  const QString text = side.geometry.toString();
  const QString& on = side.on;
  const auto named = [&name](const QString& uid, const QString& otherwise) {
    const QString found = name ? name(uid) : QString();
    return found.isEmpty() ? otherwise : found;
  };
  const QString body = named(o.value(QStringLiteral("body")).toString(), QStringLiteral("a body"));
  const QString sketch = named(o.value(QStringLiteral("sketch")).toString(), QStringLiteral("a sketch"));
  for (const char* kind : {"face", "edge", "vertex"}) {
    if (on == QLatin1String(kind)) {
      return QStringLiteral("%1 of %2").arg(QLatin1String(kind), body);
    }
  }
  if (on == QStringLiteral("body")) {
    return body;
  }
  if (on == QStringLiteral("sketch_curve")) {
    return QStringLiteral("%1 of %2")
        .arg(o.value(QStringLiteral("curves")).toArray().size() > 1 ? QStringLiteral("curves") : QStringLiteral("curve"),
             sketch);
  }
  if (on == QStringLiteral("sketch_point")) {
    return QStringLiteral("point of %1").arg(sketch);
  }
  if (on == QStringLiteral("origin")) {
    if (text == QStringLiteral("origin")) {
      return QStringLiteral("the origin");
    }
    return QStringLiteral("origin %1 %2")
        .arg(text.size() == 2 ? QStringLiteral("plane") : QStringLiteral("axis"), text.toUpper());
  }
  if (on == QStringLiteral("construction")) {
    return named(text, QStringLiteral("construction geometry"));
  }
  if (on == QStringLiteral("joint_origin")) {
    return named(text, QStringLiteral("a joint origin"));
  }
  if (on == QStringLiteral("mount")) {
    // "F7:mount(end_a)": the frame end_a of the derived part F7.
    const QString mount = o.value(QStringLiteral("mount")).toString();
    const int open = mount.indexOf(QStringLiteral(":mount("));
    if (open > 0 && mount.endsWith(QLatin1Char(')'))) {
      const int start = open + 7;
      return QStringLiteral("mount frame %1 of %2")
          .arg(mount.mid(start, mount.size() - start - 1), named(mount.left(open), QStringLiteral("its part")));
    }
    return QStringLiteral("mount frame");
  }
  const auto same = [](const QString& value) { return value; };
  return geometryText(side, same, same); // fixed geometry, an occurrence: words already
}

} // namespace

QString jointCardText(const JointHighlight& joint, const std::function<QString(const QString&)>& name) {
  QStringList parts;
  parts << joint.kind.toHtmlEscaped();
  for (const JointSide& side : joint.sides) {
    const QString owner = side.occurrenceName.isEmpty() ? QStringLiteral("its component") : side.occurrenceName;
    QString text = side.on == QStringLiteral("occurrence")
                       ? owner
                       : QStringLiteral("%1, %2").arg(owner, cardGeometryText(side, name));
    if (side.on != QStringLiteral("mount") && side.on != QStringLiteral("occurrence") && !side.showsGeometry) {
      text += QStringLiteral(" (its frame)");
    }
    parts << QStringLiteral("%1 = %2").arg(sideKey(side), text).toHtmlEscaped();
  }
  return QStringLiteral("<b>%1</b>: %2").arg(joint.name.toHtmlEscaped(), parts.join(QStringLiteral(", ")));
}

} // namespace mitcad
