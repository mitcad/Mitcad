// SPDX-License-Identifier: MIT
// Joints in the view (mitcad#55; commands.md, "Joints"):
//   - Dragging components: a left drag that starts on a body of a
//     component the joints move pulls the point it started at; per pointer
//     move the model's `joint_drag` says where the occurrences go while
//     every joint holds (nothing changes), and the view shows the bodies
//     there. The release keeps it with `drag_occurrence`, one undo step
//     (the driven motions take their new values).
//   - Animate Joint: a joint's free motion shown through its range, frame
//     by frame, each the preview of the joint driven there (the preview's
//     `placements`); the document stays as it is.
#include "MainWindow.hpp"

#include <algorithm>
#include <cmath>
#include <functional>

#include <QJsonArray>
#include <QJsonObject>
#include <QtLogging>

#include "browser/BrowserController.hpp"
#include "browser/TimelineWidget.hpp"
#include "framework/Json.hpp"
#include "framework/ModelShapes.hpp"
#include "framework/TestSync.hpp"

namespace mitcad {
namespace {

QJsonArray xyz(const gp_Pnt& point) { return QJsonArray{point.X(), point.Y(), point.Z()}; }

QString rounded(double value) { return QString::number(std::round(value * 1000.0) / 1000.0, 'g', 10); }

constexpr double kDegreesPerRadian = 57.29577951308232;
constexpr double kPi = 3.141592653589793;
// The animation: this many frames through the range, this far apart.
constexpr int kAnimationFrames = 36;
constexpr int kFrameMs = 40;
// A slide without limits moves this far either way (mm).
constexpr double kFreeSlide = 25.0;

QString motionValue(const QString& motion, double value) {
  return motion.startsWith(QLatin1Char('r')) ? rounded(value * kDegreesPerRadian) + QStringLiteral(" deg")
                                             : rounded(value) + QStringLiteral(" mm");
}

} // namespace

void MainWindow::setUpOccurrenceDrag() {
  OccurrenceDrag drag;
  drag.accepts = [this](const SelectionItem& item) {
    return m_mode == Mode::Idle && !m_session && !modelBusy() && canChangeModel() &&
           !item.occurrence.isEmpty() && !dragOccurrenceOf(item.occurrence).isEmpty();
  };
  drag.started = [this](const SelectionItem& item, const gp_Pnt& grabbed) { occurrenceDragStarted(item, grabbed); };
  drag.moved = [this](const gp_Pnt& target) { occurrenceDragged(target); };
  drag.finished = [this](const gp_Pnt& target, bool cancelled) { occurrenceDragFinished(target, cancelled); };
  m_viewer->setOccurrenceDrag(std::move(drag));
}

QString MainWindow::dragOccurrenceOf(const QString& path) const {
  // The component each occurrence places, from the snapshot's tree.
  QHash<QString, QString> components;
  std::function<void(const QVector<DocumentSnapshot::Occurrence>&)> walk =
      [&](const QVector<DocumentSnapshot::Occurrence>& level) {
        for (const DocumentSnapshot::Occurrence& occurrence : level) {
          components.insert(occurrence.uid, occurrence.component);
          walk(occurrence.children);
        }
      };
  walk(m_snapshot.occurrences);
  const QStringList uids = path.split(QLatin1Char('/'), Qt::SkipEmptyParts);
  // From the innermost occurrence out: the first whose parent component has
  // joints and whose unit can move.
  for (int depth = static_cast<int>(uids.size()); depth >= 1; --depth) {
    const QString uid = uids.at(depth - 1);
    const QString parent = depth == 1 ? QStringLiteral("C0") : components.value(uids.at(depth - 2));
    const QJsonObject dof = m_snapshot.jointDof(parent);
    for (const QJsonValue& value : dof.value(QStringLiteral("units")).toArray()) {
      const QJsonObject unit = value.toObject();
      if (unit.value(QStringLiteral("occurrences")).toArray().contains(uid) &&
          !unit.value(QStringLiteral("grounded")).toBool()) {
        return uids.mid(0, depth).join(QLatin1Char('/'));
      }
    }
  }
  return QString();
}

void MainWindow::occurrenceDragStarted(const SelectionItem& item, const gp_Pnt& grabbed) {
  const QString path = dragOccurrenceOf(item.occurrence);
  if (path.isEmpty()) {
    return;
  }
  OccurrenceDragState state;
  state.path = path;
  const QString parent = path.section(QLatin1Char('/'), 0, -2);
  state.parent = parent.isEmpty() ? gp_Trsf() : m_placements.value(parent);
  state.grabbed = grabbed.Transformed(state.parent.Inverted());
  state.target = state.grabbed;
  state.bodies = m_viewer->shownBodies();
  std::function<QString(const QVector<DocumentSnapshot::Occurrence>&)> nameOf =
      [&](const QVector<DocumentSnapshot::Occurrence>& level) {
        for (const DocumentSnapshot::Occurrence& occurrence : level) {
          if (occurrence.path == path) {
            return occurrence.name;
          }
          const QString inside = nameOf(occurrence.children);
          if (!inside.isEmpty()) {
            return inside;
          }
        }
        return QString();
      };
  state.name = nameOf(m_snapshot.occurrences);
  m_drag = state;
  qDebug().noquote() << QStringLiteral("Drag of %1 started at (%2, %3, %4)")
                            .arg(state.name, rounded(state.grabbed.X()), rounded(state.grabbed.Y()),
                                 rounded(state.grabbed.Z()));
}

void MainWindow::occurrenceDragged(const gp_Pnt& target) {
  if (!m_drag || modelBusy()) {
    return;
  }
  m_drag->target = target.Transformed(m_drag->parent.Inverted());
  QJsonObject answer;
  try {
    answer = queryObject({{QStringLiteral("query"), QStringLiteral("joint_drag")},
                          {QStringLiteral("occurrence"), m_drag->path},
                          {QStringLiteral("point"), xyz(m_drag->grabbed)},
                          {QStringLiteral("target"), xyz(m_drag->target)}});
  } catch (const std::exception&) {
    return; // the bodies stay where the last move put them
  }
  // Each occurrence that moves: from where it is shown to where the drag
  // puts it, in the design; what is inside it moves along.
  const QString parent = m_drag->path.section(QLatin1Char('/'), 0, -2);
  QHash<QString, gp_Trsf> moves;
  for (const QJsonValue& value : answer.value(QStringLiteral("placements")).toArray()) {
    const QJsonObject placement = value.toObject();
    const QString uid = placement.value(QStringLiteral("occurrence")).toString();
    const QString path = parent.isEmpty() ? uid : parent + QLatin1Char('/') + uid;
    const gp_Trsf now = m_drag->parent * trsfOf(placement.value(QStringLiteral("transform")).toArray());
    moves.insert(path, now * m_placements.value(path).Inverted());
  }
  std::vector<BodyDisplay> bodies = m_drag->bodies;
  for (BodyDisplay& body : bodies) {
    const QString occurrence = QString::fromStdString(body.occurrence);
    for (auto it = moves.cbegin(); it != moves.cend(); ++it) {
      if (occurrence == it.key() || occurrence.startsWith(it.key() + QLatin1Char('/'))) {
        body.placement = TopLoc_Location(it.value() * body.placement.Transformation());
        break;
      }
    }
  }
  m_viewer->setBodies(bodies);
}

void MainWindow::occurrenceDragFinished(const gp_Pnt& target, bool cancelled) {
  if (!m_drag) {
    return;
  }
  OccurrenceDragState state = *m_drag;
  m_drag.reset();
  if (cancelled) {
    qDebug().noquote() << QStringLiteral("Drag of %1 cancelled").arg(state.name);
    whenIdle(this, [this] { refreshScene(); });
    return;
  }
  state.target = target.Transformed(state.parent.Inverted());
  // Not inside the view's mouse event: the command is a job.
  whenIdle(this, [this, state] {
    const QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("drag_occurrence")},
                              {QStringLiteral("occurrence"), state.path},
                              {QStringLiteral("point"), xyz(state.grabbed)},
                              {QStringLiteral("target"), xyz(state.target)}};
    QJsonObject result;
    if (!runModelCommand(command, &result)) {
      refreshScene(); // the bodies back where they are
      return;
    }
    QStringList values;
    const QJsonObject joints = result.value(QStringLiteral("values")).toObject();
    for (auto it = joints.begin(); it != joints.end(); ++it) {
      const QJsonObject motions = it.value().toObject();
      for (auto motion = motions.begin(); motion != motions.end(); ++motion) {
        const bool turn = motion.key().startsWith(QLatin1Char('r'));
        const double value = motion.value().toDouble() * (turn ? 57.29577951308232 : 1.0);
        values << QStringLiteral("%1 %2 %3 %4")
                      .arg(m_snapshot.feature(it.key()) != nullptr ? m_snapshot.feature(it.key())->name : it.key(),
                           motion.key(), rounded(value), turn ? QStringLiteral("deg") : QStringLiteral("mm"));
      }
    }
    qDebug().noquote() << QStringLiteral("Dragged %1 to (%2, %3, %4)%5")
                              .arg(state.name, rounded(state.target.X()), rounded(state.target.Y()),
                                   rounded(state.target.Z()),
                                   values.isEmpty() ? QString() : QStringLiteral(": ") + values.join(QStringLiteral(", ")));
  });
}

// ---------------------------------------------------------------------------
// Animate Joint

void MainWindow::animateSelectedJoint() {
  QString uid;
  for (const QString& selected : m_controller->timeline()->selectedFeatures()) {
    if (!m_snapshot.joint(selected).value(QStringLiteral("motions")).toArray().isEmpty()) {
      uid = selected;
    }
  }
  if (uid.isEmpty()) {
    int newest = -1;
    for (const QJsonValue& value : m_snapshot.joints.value(QStringLiteral("joints")).toArray()) {
      const QJsonObject joint = value.toObject();
      const QString candidate = joint.value(QStringLiteral("uid")).toString();
      if (!joint.value(QStringLiteral("motions")).toArray().isEmpty() && candidate.mid(1).toInt() > newest) {
        newest = candidate.mid(1).toInt();
        uid = candidate;
      }
    }
  }
  if (uid.isEmpty()) {
    showStatus(tr("No joint has a free motion to animate."), true);
    return;
  }
  animateJoint(uid);
}

void MainWindow::animateJoint(const QString& uid) {
  if (m_animation || !canChangeModel() || modelBusy()) {
    return;
  }
  const QJsonObject joint = m_snapshot.joint(uid);
  const QJsonArray motions = joint.value(QStringLiteral("motions")).toArray();
  if (motions.isEmpty()) {
    return;
  }
  JointAnimation animation;
  animation.uid = uid;
  animation.name = joint.value(QStringLiteral("name")).toString();
  animation.motion = motions.first().toString();
  try {
    animation.def = queryObject({{QStringLiteral("query"), QStringLiteral("feature")}, {QStringLiteral("uid"), uid}})
                        .value(QStringLiteral("def"))
                        .toObject();
  } catch (const std::exception& e) {
    showStatus(errorText(e), true);
    return;
  }
  const QString& motion = animation.motion;
  const bool turn = motion.startsWith(QLatin1Char('r'));
  const QJsonValue at = joint.value(QStringLiteral("values")).toObject().value(motion);
  const double start =
      at.isDouble() ? at.toDouble() : joint.value(QStringLiteral("position")).toObject().value(motion).toDouble();
  const QJsonObject limit = joint.value(QStringLiteral("limits")).toObject().value(motion).toObject();
  const bool limited = limit.contains(QStringLiteral("min")) && limit.contains(QStringLiteral("max"));
  if (turn && !limited) {
    // Once round from where it is.
    for (int i = 1; i <= kAnimationFrames; ++i) {
      animation.frames.append(start + 2.0 * kPi * i / kAnimationFrames);
    }
  } else {
    // To one end, to the other and back, at an even pace.
    const double low = limited ? limit.value(QStringLiteral("min")).toDouble() : start - kFreeSlide;
    const double high = limited ? limit.value(QStringLiteral("max")).toDouble() : start + kFreeSlide;
    const double from = std::clamp(start, low, high);
    const double up = high - from;
    const double across = high - low;
    const double length = up + across + (from - low);
    for (int i = 1; i <= kAnimationFrames; ++i) {
      const double along = length * i / kAnimationFrames;
      const double value = along <= up            ? from + along
                           : along <= up + across ? high - (along - up)
                                                  : low + (along - up - across);
      animation.frames.append(std::clamp(value, low, high));
    }
  }
  animation.bodies = m_viewer->shownBodies();
  qDebug().noquote() << QStringLiteral("Animating %1 %2: %3 frames from %4 to %5")
                            .arg(animation.name, motion)
                            .arg(animation.frames.size())
                            .arg(motionValue(motion, start), motionValue(motion, animation.frames.last()));
  m_animation = std::move(animation);
  TestSync::singleShot(kFrameMs, this, [this] { animationFrame(); });
}

void MainWindow::animationFrame() {
  if (!m_animation) {
    return;
  }
  if (m_mode != Mode::Idle || m_session || modelBusy()) {
    stopAnimation(tr("the design is busy"));
    return;
  }
  // The joint driven to the frame's value, previewed.
  QJsonObject def = m_animation->def;
  QJsonObject position = def.value(QStringLiteral("position")).toObject();
  position.insert(m_animation->motion, m_animation->frames.at(m_animation->next));
  def.insert(QStringLiteral("position"), position);
  QJsonObject report;
  try {
    report = modelPreview(compactJson({{QStringLiteral("cmd"), QStringLiteral("edit_feature")},
                                       {QStringLiteral("uid"), m_animation->uid},
                                       {QStringLiteral("def"), def}}),
                          tr("Animate %1").arg(m_animation->name));
  } catch (const std::exception& e) {
    stopAnimation(errorText(e));
    return;
  }
  if (!m_animation) {
    return; // stopped while the preview computed
  }
  if (report.value(QStringLiteral("status")).toString() != QStringLiteral("ok")) {
    stopAnimation(report.value(QStringLiteral("error")).toString());
    return;
  }
  QHash<QString, QJsonArray> placed;
  for (const QJsonValue& value : report.value(QStringLiteral("placements")).toArray()) {
    placed.insert(value.toObject().value(QStringLiteral("path")).toString(),
                  value.toObject().value(QStringLiteral("transform")).toArray());
  }
  std::vector<BodyDisplay> bodies = m_animation->bodies;
  for (BodyDisplay& body : bodies) {
    const auto moved = placed.constFind(QString::fromStdString(body.occurrence));
    if (moved != placed.cend()) {
      body.placement = isIdentity(moved.value()) ? TopLoc_Location() : TopLoc_Location(trsfOf(moved.value()));
    }
  }
  m_viewer->setBodies(bodies);
  if (++m_animation->next < m_animation->frames.size()) {
    TestSync::singleShot(kFrameMs, this, [this] { animationFrame(); });
  } else {
    stopAnimation(QString());
  }
}

void MainWindow::stopAnimation(const QString& why) {
  if (!m_animation) {
    return;
  }
  const QString name = m_animation->name;
  const int shown = m_animation->next;
  m_animation.reset();
  if (!modelBusy()) {
    clearModelPreview();
    if (m_mode == Mode::Idle) {
      m_viewer->setBodies(modelBodies()); // the design as it is
    }
  }
  qDebug().noquote() << (why.isEmpty() ? QStringLiteral("Animated %1: %2 frames").arg(name).arg(shown)
                                       : QStringLiteral("Animation of %1 stopped: %2").arg(name, why));
}

} // namespace mitcad
