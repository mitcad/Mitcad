// SPDX-License-Identifier: MIT
#include "ActivateQuestion.hpp"

#include <algorithm>

#include <QCheckBox>
#include <QDebug>
#include <QMessageBox>
#include <QPushButton>
#include <QSettings>
#include <QStringList>
#include <QTimer>
#include <QVector>

#include "../framework/Dialogs.hpp"

namespace mitcad {

namespace {

const char* const kSetting = "general/askActivateComponent";
const QString kRootComponent = QStringLiteral("C0");

// The origin's planes, axes and point: the active component's own.
bool isOriginDatum(const QString& uid) {
  static const QStringList origin = {QStringLiteral("xy"), QStringLiteral("xz"), QStringLiteral("yz"),
                                     QStringLiteral("x"),  QStringLiteral("y"),  QStringLiteral("z"),
                                     QStringLiteral("origin")};
  return origin.contains(uid);
}

// The component the picked geometry belongs to; empty when it is none's
// (a preview's, or a kind the question does not cover).
QString componentOf(const DocumentSnapshot& snapshot, const SelectionItem& item) {
  switch (item.kind) {
  case SelectKind::Face:
  case SelectKind::Edge:
  case SelectKind::Vertex:
  case SelectKind::Body: {
    const auto body = snapshot.bodies.constFind(item.owner);
    if (body != snapshot.bodies.cend() && !body->component.isEmpty()) {
      return body->component;
    }
    return snapshot.occurrenceComponent(item.occurrence);
  }
  case SelectKind::Plane:
  case SelectKind::Axis:
  case SelectKind::Point:
  case SelectKind::SketchCurve:
  case SelectKind::SketchPoint:
  case SelectKind::Profile:
  case SelectKind::Sketch: {
    const DocumentSnapshot::Feature* feature = snapshot.feature(item.owner);
    return feature != nullptr ? feature->component : QString();
  }
  default:
    return QString();
  }
}

// The occurrences an occurrence path walks through, from the root's child
// to the one it names; empty when it leads nowhere.
QVector<const DocumentSnapshot::Occurrence*> walk(const DocumentSnapshot& snapshot, const QString& path) {
  QVector<const DocumentSnapshot::Occurrence*> found;
  const QVector<DocumentSnapshot::Occurrence>* level = &snapshot.occurrences;
  for (const QString& uid : path.split(QLatin1Char('/'), Qt::SkipEmptyParts)) {
    const DocumentSnapshot::Occurrence* next = nullptr;
    for (const DocumentSnapshot::Occurrence& occurrence : *level) {
      if (occurrence.uid == uid) {
        next = &occurrence;
        break;
      }
    }
    if (next == nullptr) {
      return {};
    }
    found.append(next);
    level = &next->children;
  }
  return found;
}

// The name of the occurrence a path names: the root's component name for
// the root ("").
QString occurrenceName(const DocumentSnapshot& snapshot, const QString& path) {
  const QVector<const DocumentSnapshot::Occurrence*> occurrences = walk(snapshot, path);
  return occurrences.isEmpty() ? snapshot.componentName(kRootComponent) : occurrences.last()->name;
}

// A name for a button: its own ampersands are no mnemonics.
QString literal(const QString& name) {
  QString escaped = name;
  escaped.replace(QLatin1Char('&'), QStringLiteral("&&"));
  return escaped;
}

} // namespace

bool askToActivateComponent() {
  const QSettings settings;
  if (settings.contains(QLatin1String(kSetting))) {
    return settings.value(QLatin1String(kSetting)).toBool();
  }
  return qgetenv("MITCAD_ASK_ACTIVATE_COMPONENT").trimmed() != "0";
}

void saveAskToActivateComponent(bool on) { QSettings().setValue(QLatin1String(kSetting), on); }

QString ActivateQuestion::text() const {
  return QObject::tr("The %1 belongs to %2, but %3 is active.").arg(kind, picked, active);
}

QString ActivateQuestion::detail() const {
  return QObject::tr("If you activate it, the %1 goes into %2. If you keep %3 active, the %1 goes into %3.")
      .arg(made, picked, active);
}

QString madeBy(const QString& featureType) {
  if (featureType == QLatin1String("construction_plane")) {
    return QStringLiteral("plane");
  }
  if (featureType == QLatin1String("construction_axis")) {
    return QStringLiteral("axis");
  }
  if (featureType == QLatin1String("construction_point")) {
    return QStringLiteral("point");
  }
  return QStringLiteral("sketch");
}

QRect questionPlace(const QSize& size, const QPoint& pick, const QRect& within) {
  // The pick and the face around it, which the box keeps clear of.
  constexpr int kClear = 80;
  const QRect keep(pick.x() - kClear, pick.y() - kClear, 2 * kClear + 1, 2 * kClear + 1);
  const auto inside = [&within](QRect place) {
    place.moveLeft(std::clamp(place.left(), within.left(), std::max(within.left(), within.right() + 1 - place.width())));
    place.moveTop(std::clamp(place.top(), within.top(), std::max(within.top(), within.bottom() + 1 - place.height())));
    return place;
  };
  const int w = size.width();
  const int h = size.height();
  const QRect candidates[] = {
      QRect(QPoint(pick.x() - w / 2, keep.top() - h), size),    // above
      QRect(QPoint(pick.x() - w / 2, keep.bottom() + 1), size), // below
      QRect(QPoint(keep.right() + 1, pick.y() - h / 2), size),  // to the right
      QRect(QPoint(keep.left() - w, pick.y() - h / 2), size),   // to the left
  };
  QRect least;
  qint64 leastCovered = -1;
  for (const QRect& candidate : candidates) {
    const QRect place = inside(candidate);
    const QRect covered = place.intersected(keep);
    if (covered.isEmpty()) {
      return place;
    }
    const qint64 area = static_cast<qint64>(covered.width()) * covered.height();
    if (leastCovered < 0 || area < leastCovered) {
      least = place;
      leastCovered = area;
    }
  }
  return least;
}

ActivateQuestion activateQuestion(const DocumentSnapshot& snapshot, const SelectionItem& item,
                                  const QString& refused, bool enabled) {
  ActivateQuestion question;
  if (!enabled || !refused.isEmpty()) {
    return question;
  }
  // The origin's planes, axes and point are the active component's own.
  if ((item.kind == SelectKind::Plane || item.kind == SelectKind::Axis || item.kind == SelectKind::Point) &&
      isOriginDatum(item.owner)) {
    return question;
  }
  const QString component = componentOf(snapshot, item);
  const QString active = snapshot.activeComponent.isEmpty() ? kRootComponent : snapshot.activeComponent;
  if (component.isEmpty() || component == active) {
    return question; // the active component's own, at any of its occurrences
  }
  // Where it was picked: the item's occurrence when it leads to that
  // component, else the component's first.
  QString path = item.occurrence;
  if (snapshot.occurrenceComponent(path) != component) {
    path = snapshot.firstOccurrence(component);
    if (component != kRootComponent && path.isEmpty()) {
      return question; // not placed anywhere: nothing to activate it at
    }
  }
  question.ask = true;
  question.component = component;
  question.occurrence = path;
  question.picked = occurrenceName(snapshot, path);
  question.kind = SelectionItem::kindName(item.kind, 1);
  // The active component by the occurrence the pick lies under (its child
  // occurrence's face: the parent's occurrence above it), else its first.
  if (active == kRootComponent) {
    question.active = snapshot.componentName(kRootComponent);
  } else {
    const QVector<const DocumentSnapshot::Occurrence*> above = walk(snapshot, path);
    for (const DocumentSnapshot::Occurrence* occurrence : above) {
      if (occurrence->component == active) {
        question.active = occurrence->name;
      }
    }
    if (question.active.isEmpty()) {
      const QString first = snapshot.firstOccurrence(active);
      question.active = first.isEmpty() ? snapshot.componentName(active) : occurrenceName(snapshot, first);
    }
  }
  return question;
}

ActivateAnswer askToActivate(QWidget* parent, const ActivateQuestion& question, const std::optional<QPoint>& pick) {
  QMessageBox box(QMessageBox::Question, QObject::tr("Activate Component"), question.text(), QMessageBox::NoButton,
                  parent);
  box.setObjectName(QStringLiteral("activateComponentQuestion"));
  box.setInformativeText(question.detail());
  QPushButton* activate =
      box.addButton(QObject::tr("&Activate %1").arg(literal(question.picked)), QMessageBox::AcceptRole);
  QPushButton* keep = box.addButton(QObject::tr("&Keep in %1").arg(literal(question.active)), QMessageBox::RejectRole);
  auto* again = new QCheckBox(QObject::tr("&Don't ask again"), &box);
  again->setToolTip(QObject::tr("Preferences > General asks again"));
  box.setCheckBox(again);
  box.setDefaultButton(activate);
  box.setEscapeButton(keep);
  prepareModal(&box);
  if (pick && parent != nullptr) {
    // Clear of the picked face rather than over the middle of the window,
    // which is where the pick usually is. Placed by its estimated size
    // before it shows, then once more by the size it shows with (the
    // message box sizes itself when shown); logged for the tests.
    const QRect within = parent->window()->frameGeometry();
    const QPoint at = *pick;
    box.adjustSize();
    box.move(questionPlace(box.frameGeometry().size(), at, within).topLeft());
    QTimer::singleShot(0, &box, [&box, at, within] {
      const QRect place = questionPlace(box.frameGeometry().size(), at, within);
      if (place.topLeft() != box.frameGeometry().topLeft()) {
        box.move(place.topLeft());
      }
      const QRect shown = box.frameGeometry();
      qDebug().noquote() << QStringLiteral("Activate component question at %1,%2 %3x%4, the pick at %5,%6 (%7)")
                                .arg(shown.x())
                                .arg(shown.y())
                                .arg(shown.width())
                                .arg(shown.height())
                                .arg(at.x())
                                .arg(at.y())
                                .arg(shown.contains(at) ? QStringLiteral("over it") : QStringLiteral("clear of it"));
    });
  }
  execDialog(&box);
  ActivateAnswer answer;
  answer.activate = box.clickedButton() == activate;
  answer.dontAskAgain = again->isChecked();
  return answer;
}

} // namespace mitcad
