// SPDX-License-Identifier: MIT
// The joint highlight (mitcad#523; app/COMMANDS.md, "Joints"): a selected
// joint, as-built joint or constraint set (its browser row, its timeline
// cell) shows what its sides are built on, where the occurrences put it:
//   - each side's geometry (faces, edges, vertices, sketch geometry,
//     construction and origin planes, axes and points, joint origins),
//     side a in orange, side b in green (an as-built joint's motion origin
//     in yellow);
//   - each side's frame where the model resolved it at the marker, a small
//     triad (x red, y green, z in the side's colour);
//   - the occurrences it joins in a faint tint of their side's colour;
//   - a side on a fixed reference, or on an override that moves the
//     origin or turns the axes, shows its frame only; a mount frame (BUILD)
//     its frame and its part; an as-built joint's side its occurrence.
// The pointer over a joint's row in the browser previews it in lighter
// colours. The sides come from the `joints` query
// (framework/JointHighlight.hpp); nothing is asked of the model here.
#include "MainWindow.hpp"

#include <algorithm>
#include <array>
#include <cmath>
#include <utility>

#include <BRepBndLib.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <Bnd_Box.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include <QJsonArray>
#include <QJsonObject>
#include <QtLogging>

#include "commands/CommandSupport.hpp"
#include "framework/JointHighlight.hpp"
#include "framework/Theme.hpp"

namespace mitcad {

namespace {

// The joint's sides and frames from the role table (mitcad#1041): the X and Y
// axes as the view cube's; the motion origin off the warning's yellow.
const QColor kJointSideA = viewColor(ColorRole::JointA, false);
const QColor kJointSideB = viewColor(ColorRole::JointB, false);
const QColor kJointMotionOrigin = viewColor(ColorRole::JointOrigin, false);
const QColor kTriadX = viewColor(ColorRole::AxisX, false);
const QColor kTriadY = viewColor(ColorRole::AxisY, false);

QColor sideColor(const JointSide& side) {
  if (side.key.startsWith(QLatin1Char('a'))) {
    return kJointSideA;
  }
  if (side.key.startsWith(QLatin1Char('b'))) {
    return kJointSideB;
  }
  return kJointMotionOrigin;
}

gp_Pnt framePoint(const std::array<double, 3>& p) { return gp_Pnt(p[0], p[1], p[2]); }
gp_Vec frameVector(const std::array<double, 3>& v) { return gp_Vec(v[0], v[1], v[2]); }

// An axis of a triad: `length` along `direction` from `origin`; null for
// a direction without length.
TopoDS_Shape axisShape(const gp_Pnt& origin, const gp_Vec& direction, double length) {
  if (direction.Magnitude() < 1e-9 || length <= 0.0) {
    return TopoDS_Shape();
  }
  const gp_Pnt end = origin.Translated(direction.Normalized() * length);
  BRepBuilderAPI_MakeEdge edge(origin, end);
  return edge.IsDone() ? TopoDS_Shape(edge.Edge()) : TopoDS_Shape();
}

QString pointText(const gp_Pnt& p) {
  return QStringLiteral("%1,%2,%3").arg(p.X(), 0, 'f', 1).arg(p.Y(), 0, 'f', 1).arg(p.Z(), 0, 'f', 1);
}

} // namespace

QJsonObject MainWindow::jointEntry(const QString& uid) const {
  for (const char* key : {"joints", "constraints"}) {
    for (const QJsonValue& value : m_snapshot.joints.value(QLatin1String(key)).toArray()) {
      if (value.toObject().value(QStringLiteral("uid")).toString() == uid) {
        return value.toObject();
      }
    }
  }
  return QJsonObject();
}

void MainWindow::appendJointHighlight(const JointHighlight& joint, bool preview,
                                      std::vector<HighlightDisplay>& highlights, QStringList& lines) const {
  const auto colorOf = [preview](const QColor& color) { return preview ? color.lighter(135) : color; };
  // The joined occurrences: a faint tint (not while previewing), unless a
  // side shows the occurrence itself; their box sizes the triads.
  Bnd_Box box;
  for (const QString& path : joint.occurrences) {
    SelectionItem occurrence{SelectKind::Component, m_snapshot.occurrenceComponent(path), QString(), QString(),
                             path};
    const TopoDS_Shape shape = itemShape(occurrence);
    if (shape.IsNull()) {
      continue;
    }
    BRepBndLib::Add(shape, box);
    const auto side = std::find_if(joint.sides.cbegin(), joint.sides.cend(),
                                   [&path](const JointSide& s) { return s.path == path && !s.ownPath.isEmpty(); });
    const bool part = std::any_of(joint.sides.cbegin(), joint.sides.cend(),
                                  [&path](const JointSide& s) { return s.showsPart && s.path == path; });
    if (preview || part || side == joint.sides.cend()) {
      continue;
    }
    HighlightDisplay tint{occurrence, shape, sideColor(*side)};
    tint.pickable = false;
    tint.light = true;
    highlights.push_back(tint);
  }
  // A triad a tenth of the joined occurrences' size, within reason.
  double size = 10.0;
  if (!box.IsVoid()) {
    size = std::clamp(0.12 * std::sqrt(box.SquareExtent()), 2.0, 250.0);
  }
  const gp_Trsf placement = placementOf(joint.placing).Transformation();
  for (const JointSide& side : joint.sides) {
    const QColor color = colorOf(sideColor(side));
    // The geometry the side is on (its items placed by its occurrences).
    Selection items;
    if (side.showsGeometry) {
      const QJsonObject o = side.geometry.toObject();
      const QString sketch = o.value(QStringLiteral("sketch")).toString();
      if (side.on == QStringLiteral("sketch_curve") && o.contains(QStringLiteral("curves"))) {
        for (const QJsonValue& curve : o.value(QStringLiteral("curves")).toArray()) {
          items.append(SelectionItem{SelectKind::SketchCurve, sketch, curve.toString(), QString()});
        }
      } else if (side.on == QStringLiteral("sketch_curve") && !o.contains(QStringLiteral("curve"))) {
        items.append(SelectionItem{SelectKind::Sketch, sketch, QString(), QString()}); // all its curves
      } else {
        items.append(cmd::itemOf(side.geometry, model()));
      }
    }
    if (side.showsPart && !side.ownPath.isEmpty()) {
      items.append(SelectionItem{SelectKind::Component, m_snapshot.occurrenceComponent(side.path), QString(),
                                 QString(), side.path});
    }
    for (SelectionItem item : items) {
      if (!item.isValid()) {
        continue;
      }
      item.occurrence = side.path;
      const TopoDS_Shape shape = itemShape(item);
      if (shape.IsNull()) {
        continue;
      }
      HighlightDisplay shown{item, shape, color};
      // A click on a side's geometry selects it; the part's bodies take
      // their own clicks.
      shown.pickable = !preview && item.kind != SelectKind::Component;
      highlights.push_back(shown);
    }
    // Its frame: a small triad where the model resolved it.
    if (side.frame) {
      const gp_Pnt origin = framePoint(side.frame->origin).Transformed(placement);
      const std::array<std::pair<std::array<double, 3>, QColor>, 3> axes{
          {{side.frame->x, colorOf(kTriadX)}, {side.frame->y, colorOf(kTriadY)}, {side.frame->z, color}}};
      for (const auto& [direction, axisColor] : axes) {
        const TopoDS_Shape axis = axisShape(origin, frameVector(direction).Transformed(placement),
                                            direction == side.frame->z ? size : size * 0.6);
        if (!axis.IsNull()) {
          HighlightDisplay triad{SelectionItem(), axis, axisColor};
          triad.pickable = false;
          highlights.push_back(triad);
        }
      }
      lines << QStringLiteral("Joint highlight frame %1 at %2 [%3]")
                 .arg(side.row > 0 ? QStringLiteral("row %1 %2").arg(side.row).arg(side.key) : side.key,
                      pointText(origin), joint.name);
    }
  }
  if (!preview) {
    lines << jointHighlightLog(joint);
  }
}

void MainWindow::addJointHighlights(std::vector<HighlightDisplay>& highlights) {
  QStringList lines;
  if (m_mode == Mode::Idle) {
    for (const SelectionItem& item : std::as_const(m_selection)) {
      if (item.kind != SelectKind::Feature) {
        continue;
      }
      const QJsonObject entry = jointEntry(item.owner);
      if (!entry.isEmpty()) {
        appendJointHighlight(jointHighlightOf(entry, item.occurrence), false, highlights, lines);
      }
    }
    if (m_jointHover.isValid() && !m_selection.contains(m_jointHover)) {
      const QJsonObject entry = jointEntry(m_jointHover.owner);
      if (!entry.isEmpty()) {
        QStringList previewLog; // the hover logs itself (previewJoint)
        appendJointHighlight(jointHighlightOf(entry, m_jointHover.occurrence), true, highlights, previewLog);
      }
    }
  }
  const QString logged = lines.isEmpty() ? QStringLiteral("none") : lines.join(QLatin1Char('\n'));
  if (logged == m_loggedJointHighlight) {
    return;
  }
  m_loggedJointHighlight = logged;
  if (lines.isEmpty()) {
    qDebug().noquote() << "Joint highlight: none";
  }
  for (const QString& line : std::as_const(lines)) {
    qDebug().noquote() << line;
  }
}

QString MainWindow::jointCardBlock(const SelectionItem& item) const {
  if (item.kind != SelectKind::Feature) {
    return QString();
  }
  const QJsonObject entry = jointEntry(item.owner);
  if (entry.isEmpty()) {
    return QString();
  }
  return jointCardText(jointHighlightOf(entry, item.occurrence), [this](const QString& uid) {
    const auto found = m_snapshot.bodies.constFind(uid);
    if (found != m_snapshot.bodies.cend()) {
      return found->name;
    }
    const DocumentSnapshot::Feature* feature = m_snapshot.feature(uid);
    return feature == nullptr ? QString() : feature->name;
  });
}

void MainWindow::previewJoint(const QString& uid, const QString& occurrence) {
  if (modelBusy()) {
    return; // the scene is shown again after the job
  }
  SelectionItem item;
  const DocumentSnapshot::Feature* feature = uid.isEmpty() ? nullptr : m_snapshot.feature(uid);
  if (feature != nullptr && !jointEntry(uid).isEmpty()) {
    item = {SelectKind::Feature, uid, QString(), QString(), occurrence};
  }
  if (item == m_jointHover) {
    return;
  }
  m_jointHover = item;
  qDebug().noquote() << QStringLiteral("Joint hover preview: %1")
                            .arg(feature != nullptr && item.isValid() ? feature->name : QStringLiteral("none"));
  if (m_mode == Mode::Idle) {
    updateIdleView();
  }
}

} // namespace mitcad
