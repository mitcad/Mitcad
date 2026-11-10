// SPDX-License-Identifier: MIT
// Joints in the view (mitcad#55; commands.md, "Joints"):
//   - Dragging components: a left drag that starts on a body of a
//     component the joints move pulls the point it started at; per pointer
//     move the model's `joint_drag` says where the occurrences go while
//     every joint holds (nothing changes), and the view shows the bodies
//     there. The release keeps it with `drag_occurrence`, one undo step
//     (the driven motions take their new values). The pointer over such a
//     body shows the move cursor and a hint; a drag on a body that does
//     not drag says why in the status bar (mitcad#150, dragAnswer).
//   - Animate Joint: a joint's free motion shown through its range, frame
//     by frame, each the preview of the joint driven there (the preview's
//     `placements`); the document stays as it is.
#include "MainWindow.hpp"

#include <algorithm>
#include <cmath>
#include <functional>

#include <QJsonArray>
#include <QJsonObject>
#include <QLabel>
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

// "A", "A and B", "A, B and C".
QString listed(const QStringList& items) {
  if (items.size() <= 1) {
    return items.value(0);
  }
  return QObject::tr("%1 and %2").arg(items.mid(0, items.size() - 1).join(QStringLiteral(", ")), items.last());
}

bool isBodyItem(const SelectionItem& item) {
  return item.kind == SelectKind::Face || item.kind == SelectKind::Edge || item.kind == SelectKind::Vertex ||
         item.kind == SelectKind::Body;
}

} // namespace

void MainWindow::setUpOccurrenceDrag() {
  OccurrenceDrag drag;
  drag.accepts = [this](const SelectionItem& item) { return dragAnswer(item).start; };
  drag.refused = [this](const SelectionItem& item) {
    const QString why = dragAnswer(item).text;
    if (!why.isEmpty()) {
      showHint(why);
      m_dragHint.clear();
      qDebug().noquote() << QStringLiteral("Drag refused: %1").arg(why);
    }
  };
  drag.started = [this](const SelectionItem& item, const gp_Pnt& grabbed) { occurrenceDragStarted(item, grabbed); };
  drag.moved = [this](const gp_Pnt& target) { occurrenceDragged(target); };
  drag.finished = [this](const gp_Pnt& target, bool cancelled) { occurrenceDragFinished(target, cancelled); };
  m_viewer->setOccurrenceDrag(std::move(drag));
  connect(m_viewer, &OcctViewer::hovered, this, &MainWindow::showDragHint);
}

MainWindow::DragAnswer MainWindow::dragAnswer(const SelectionItem& item) const {
  const bool idle = m_mode == Mode::Idle && !m_session;
  const bool body = isBodyItem(item);
  // In a design without components nothing drags.
  if (m_snapshot.occurrences.isEmpty() || (item.occurrence.isEmpty() && !(body && idle))) {
    return {};
  }
  if (item.occurrence.isEmpty()) {
    return {OccurrenceDragStart::Refuse, QString(), tr("Bodies of the root component do not move")};
  }
  // The occurrences of the snapshot's tree, by uid.
  QHash<QString, const DocumentSnapshot::Occurrence*> occurrences;
  std::function<void(const QVector<DocumentSnapshot::Occurrence>&)> walk =
      [&](const QVector<DocumentSnapshot::Occurrence>& level) {
        for (const DocumentSnapshot::Occurrence& occurrence : level) {
          occurrences.insert(occurrence.uid, &occurrence);
          walk(occurrence.children);
        }
      };
  walk(m_snapshot.occurrences);
  const auto nameOf = [&](const QString& uid) {
    const DocumentSnapshot::Occurrence* occurrence = occurrences.value(uid);
    return occurrence != nullptr ? occurrence->name : uid;
  };
  const QStringList uids = item.occurrence.split(QLatin1Char('/'), Qt::SkipEmptyParts);
  // From the innermost occurrence out: the first whose parent component has
  // joints and whose unit can move drags; else the innermost reason.
  QString refusal;
  bool unjoined = false; // the innermost reason: no joints of its own
  DragAnswer answer;
  for (int depth = static_cast<int>(uids.size()); depth >= 1 && answer.path.isEmpty(); --depth) {
    const QString uid = uids.at(depth - 1);
    const DocumentSnapshot::Occurrence* above = depth == 1 ? nullptr : occurrences.value(uids.at(depth - 2));
    const QString parent = depth == 1 ? QStringLiteral("C0") : above != nullptr ? above->component : QString();
    const QJsonObject dof = m_snapshot.jointDof(parent);
    QJsonObject unit;
    for (const QJsonValue& value : dof.value(QStringLiteral("units")).toArray()) {
      if (value.toObject().value(QStringLiteral("occurrences")).toArray().contains(uid)) {
        unit = value.toObject();
      }
    }
    const QString name = nameOf(uid);
    const QJsonArray joints = unit.value(QStringLiteral("joints")).toArray();
    const DocumentSnapshot::Occurrence* self = occurrences.value(uid);
    const bool selfGrounded = self != nullptr && self->grounded;
    const bool grounded = selfGrounded || unit.value(QStringLiteral("grounded")).toBool();
    if (!grounded && (dof.isEmpty() || (!unit.isEmpty() && joints.isEmpty()))) {
      // No joints of its own in the parent component: components do not
      // drag freely (mitcad#150); an occurrence above it may drag.
      unjoined = unjoined || refusal.isEmpty();
      continue;
    }
    QString why;
    if (grounded) {
      // Grounded itself, or in a rigid group with a grounded occurrence.
      QStringList members;
      for (const QJsonValue& member : unit.value(QStringLiteral("occurrences")).toArray()) {
        const DocumentSnapshot::Occurrence* occurrence = occurrences.value(member.toString());
        if (occurrence != nullptr && occurrence->grounded && member.toString() != uid) {
          members << occurrence->name;
        }
      }
      why = selfGrounded || members.isEmpty()
                ? tr("%1 is grounded").arg(name)
                : tr("%1 is in a rigid group with %2, which is grounded").arg(name, listed(members));
    } else if (unit.isEmpty()) {
      why = tr("The joints of %1 cannot be solved where the components are").arg(m_snapshot.componentName(parent));
    } else if (unit.value(QStringLiteral("dof")).toInt() == 0 && !unit.value(QStringLiteral("mobile")).toBool(true)) {
      // Held fixed by its joints: name them.
      QStringList names;
      QStringList described;
      bool rigid = true;
      for (const QJsonValue& value : joints) {
        const QJsonObject joint = m_snapshot.joint(value.toString());
        const QString kind = joint.value(QStringLiteral("kind")).toString();
        names << joint.value(QStringLiteral("name")).toString(value.toString());
        described << QStringLiteral("%1 (%2)").arg(names.last(), kind);
        rigid = rigid && kind == QStringLiteral("rigid");
      }
      if (names.isEmpty()) {
        why = tr("%1 cannot move").arg(name);
      } else if (!rigid) {
        why = tr("%1 cannot move: %2 hold it").arg(name, listed(described));
      } else if (names.size() == 1) {
        why = tr("%1 cannot move: %2 is rigid").arg(name, names.first());
      } else {
        why = tr("%1 cannot move: %2 are rigid").arg(name, listed(names));
      }
    } else {
      // It drags: its joints' free motions in the hint.
      QStringList free;
      for (const QJsonValue& value : joints) {
        const QJsonObject joint = m_snapshot.joint(value.toString());
        QStringList motions;
        for (const QJsonValue& motion : joint.value(QStringLiteral("motions")).toArray()) {
          motions << motion.toString();
        }
        if (!motions.isEmpty()) {
          free << QStringLiteral("%1: %2").arg(joint.value(QStringLiteral("name")).toString(value.toString()),
                                               motions.join(QStringLiteral(", ")));
        }
      }
      answer.start = OccurrenceDragStart::Drag;
      answer.path = uids.mid(0, depth).join(QLatin1Char('/'));
      answer.text = free.isEmpty() ? tr("Drag to move %1").arg(name)
                                   : tr("Drag to move %1 (%2)").arg(name, free.join(QStringLiteral("; ")));
    }
    if (refusal.isEmpty() && !unjoined) {
      refusal = why;
    }
  }
  if (answer.start == OccurrenceDragStart::Drag) {
    // It would drag, but not now.
    QString now;
    if (windowReadOnly()) {
      now = readOnlyReason();
    } else if (m_mode == Mode::Sketch) {
      now = tr("Finish the sketch to drag components");
    } else if (!idle) {
      now = tr("Close the command to drag components");
    } else if (modelBusy()) {
      now = tr("Wait for the computation to finish to drag components");
    }
    if (!now.isEmpty()) {
      return {OccurrenceDragStart::Refuse, QString(), now};
    }
    return answer;
  }
  // Only bodies say why they do not drag; in a command or a sketch, a drag
  // there selects with a window as before.
  if (!body || !idle) {
    return {};
  }
  if (!unjoined && !refusal.isEmpty()) {
    return {OccurrenceDragStart::Refuse, QString(), refusal};
  }
  // The innermost occurrence has no joints in its parent component.
  const DocumentSnapshot::Occurrence* innermost = occurrences.value(uids.last());
  const DocumentSnapshot::Component* component =
      innermost != nullptr ? m_snapshot.component(innermost->component) : nullptr;
  const QString bodyName = m_snapshot.bodies.value(item.owner).name;
  if (component != nullptr && component->bodies.size() > 1 && !bodyName.isEmpty()) {
    return {OccurrenceDragStart::Refuse, QString(),
            tr("%1 is a body of %2: bodies move only with their component, and %3 has no joints in its parent "
               "component; Create Components from Bodies makes one component per body")
                .arg(bodyName, component->name, nameOf(uids.last()))};
  }
  return {OccurrenceDragStart::Refuse, QString(),
          tr("%1 has no joints in its parent component").arg(nameOf(uids.last()))};
}

void MainWindow::showDragHint(const SelectionItem& item) {
  const DragAnswer answer = item.isValid() ? dragAnswer(item) : DragAnswer();
  const bool drag = answer.start == OccurrenceDragStart::Drag;
  m_viewer->setDragCursor(drag);
  const QString hint = drag ? answer.text : QString();
  if (hint == m_dragHint) {
    return;
  }
  if (!hint.isEmpty()) {
    showHint(hint);
  } else if (m_statusLabel->text() == m_dragHint) {
    showHint(QString());
  }
  m_dragHint = hint;
  qDebug().noquote() << QStringLiteral("Drag hint: %1").arg(hint.isEmpty() ? QStringLiteral("none") : hint);
}

void MainWindow::occurrenceDragStarted(const SelectionItem& item, const gp_Pnt& grabbed) {
  const DragAnswer answer = dragAnswer(item);
  if (answer.start != OccurrenceDragStart::Drag) {
    return;
  }
  const QString path = answer.path;
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
  // A contact set stops it where the bodies touch (mitcad#454).
  const QJsonObject contact = answer.value(QStringLiteral("contact")).toObject();
  const QString touching = contact.isEmpty() ? QString() : contactText(contact);
  if (touching != m_drag->contact) {
    m_drag->contact = touching;
    if (!touching.isEmpty()) {
      showHint(tr("Stopped at contact: %1").arg(touching));
      qDebug().noquote() << QStringLiteral("Drag of %1 stopped at contact: %2").arg(m_drag->name, touching);
    }
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
    const QJsonObject contact = result.value(QStringLiteral("contact")).toObject();
    if (!contact.isEmpty()) {
      showStatus(tr("Stopped at contact: %1").arg(contactText(contact)), NoticeLevel::Information);
      qDebug().noquote() << QStringLiteral("Dragged %1 to where it touches: %2").arg(state.name, contactText(contact));
    }
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
    showStatus(tr("No joint has a free motion to animate."), NoticeLevel::Error);
    return;
  }
  animateJoint(uid);
}

void MainWindow::animateJoint(const QString& uid) {
  if (m_animation || !canChangeModel() || modelBusy()) {
    return;
  }
  // The animation's frames are previews, as a study's playback's.
  if (m_motionStudy) {
    m_motionStudy->close();
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
    showStatus(errorText(e), NoticeLevel::Error);
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
  animation.start = start;
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
  // The joint driven to the frame's value from the frame before, previewed:
  // a contact set stops it where the bodies touch (mitcad#454).
  const double from = m_animation->next == 0 ? m_animation->start : m_animation->frames.at(m_animation->next - 1);
  QJsonObject report;
  try {
    report = modelPreview(
        compactJson({{QStringLiteral("cmd"), QStringLiteral("drive_joint")},
                     {QStringLiteral("joint"), m_animation->uid},
                     {QStringLiteral("values"),
                      QJsonObject{{m_animation->motion, m_animation->frames.at(m_animation->next)}}},
                     {QStringLiteral("from"), QJsonObject{{m_animation->motion, from}}}}),
        tr("Animate %1").arg(m_animation->name));
  } catch (const std::exception& e) {
    stopAnimation(errorText(e));
    return;
  }
  if (!m_animation) {
    return; // stopped while the preview computed
  }
  if (report.value(QStringLiteral("status")).toString() != QStringLiteral("ok")) {
    stopAnimation(userText(report.value(QStringLiteral("error")).toString()));
    return;
  }
  showPreviewPlacements(report, m_animation->bodies);
  const QJsonObject contact = report.value(QStringLiteral("contact")).toObject();
  if (!contact.isEmpty()) {
    // Shown where it touches; the animation ends there.
    const double at = report.value(QStringLiteral("values"))
                          .toObject()
                          .value(m_animation->uid)
                          .toObject()
                          .value(m_animation->motion)
                          .toDouble();
    const QString text = contactText(contact);
    showStatus(tr("%1 stopped at contact: %2").arg(m_animation->name, text), NoticeLevel::Information);
    const QString name = m_animation->name;
    const QString motion = m_animation->motion;
    m_animation.reset();
    qDebug().noquote() << QStringLiteral("Animation of %1 stopped by contact at %2 %3: %4")
                              .arg(name, motion, motionValue(motion, at), text);
    // The contact shown for a moment, then the design as it is.
    TestSync::singleShot(600, this, [this] {
      if (!m_animation && !modelBusy()) {
        clearModelPreview();
        if (m_mode == Mode::Idle) {
          m_viewer->setBodies(modelBodies());
        }
      }
    });
    return;
  }
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
