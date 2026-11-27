// SPDX-License-Identifier: MIT
// A guide step of the in-app tutorials (mitcad#2012): see GuideStep.hpp.
#include "GuideStep.hpp"

#include <algorithm>
#include <cmath>

#include <QJsonArray>
#include <QJsonValue>

namespace mitcad {
namespace {

constexpr int kMaxText = 400;

const QStringList& pickKindNames() {
  // Selection::kindName's ids of what a click in the view picks.
  static const QStringList names{
      QStringLiteral("face"),         QStringLiteral("edge"),         QStringLiteral("vertex"),
      QStringLiteral("body"),         QStringLiteral("profile"),      QStringLiteral("sketch_curve"),
      QStringLiteral("sketch_point"), QStringLiteral("plane"),        QStringLiteral("axis"),
      QStringLiteral("point"),        QStringLiteral("component"),    QStringLiteral("sketch"),
      QStringLiteral("feature")};
  return names;
}

// "face", ["face", "edge"] or true (any); false with the reason otherwise.
bool readKinds(const QJsonValue& value, const QString& where, QStringList* kinds, QString* error) {
  kinds->clear();
  if (value.isBool() && value.toBool()) {
    return true;
  }
  QJsonArray list;
  if (value.isString()) {
    list.append(value);
  } else if (value.isArray()) {
    list = value.toArray();
  } else {
    *error = QStringLiteral("%1 is true, a kind or a list of kinds").arg(where);
    return false;
  }
  for (const QJsonValue& entry : list) {
    const QString kind = entry.toString();
    if (!pickKindNames().contains(kind)) {
      *error = QStringLiteral("%1: \"%2\" is no kind of pick; the kinds are %3")
                   .arg(where, kind, pickKindNames().join(QStringLiteral(", ")));
      return false;
    }
    if (!kinds->contains(kind)) {
      kinds->append(kind);
    }
  }
  return true;
}

// The one key of `object` among `keys` that is there; empty with the
// reason when none or several are.
QString oneOf(const QJsonObject& object, const QStringList& keys, const QString& where, QString* error) {
  QStringList present;
  for (const QString& key : keys) {
    if (object.contains(key)) {
      present << key;
    }
  }
  if (present.size() != 1) {
    *error = QStringLiteral("%1 takes exactly one of %2").arg(where, keys.join(QStringLiteral(", ")));
    return QString();
  }
  return present.first();
}

// A done point (mitcad#2440): a place (guidePointPlaces()), a list of
// places or the coordinates [x, y]; false with the reason otherwise.
bool readPoint(const QJsonValue& value, GuideStep* step, QString* error) {
  const QString usage = QStringLiteral("The done point is a place, a list of places or the coordinates [x, y]; "
                                       "the places are %1")
                            .arg(guidePointPlaces().join(QStringLiteral(", ")));
  if (value.isArray()) {
    const QJsonArray pair = value.toArray();
    if (pair.size() == 2 && pair.at(0).isDouble() && pair.at(1).isDouble()) {
      step->pointAtGiven = true;
      step->pointAt = QPointF(pair.at(0).toDouble(), pair.at(1).toDouble());
      return true;
    }
  }
  QJsonArray list;
  if (value.isString()) {
    list.append(value);
  } else if (value.isArray()) {
    list = value.toArray();
  }
  if (list.isEmpty()) {
    *error = usage;
    return false;
  }
  for (const QJsonValue& entry : list) {
    const QString place = entry.toString();
    if (!guidePointPlaces().contains(place)) {
      *error = entry.isString() ? QStringLiteral("The done point: \"%1\" is no place; the places are %2")
                                      .arg(place, guidePointPlaces().join(QStringLiteral(", ")))
                                : usage;
      return false;
    }
    if (!step->pointPlaces.contains(place)) {
      step->pointPlaces.append(place);
    }
  }
  return true;
}

QString normalValue(const QString& text) {
  QString value;
  for (const QChar c : text.toLower()) {
    // Every space, a no-break space between a number and its unit too.
    if (!c.isSpace()) {
      value.append(c);
    }
  }
  if (value.endsWith(QLatin1String("mm"))) {
    value.chop(2);
  }
  return value;
}

} // namespace

const QStringList& guideModes() {
  static const QStringList modes{QStringLiteral("idle"), QStringLiteral("pick_plane"), QStringLiteral("sketch"),
                                 QStringLiteral("command")};
  return modes;
}

const QStringList& guideNavigationCommands() {
  static const QStringList ids{QStringLiteral("view.home"),        QStringLiteral("view.fit"),
                               QStringLiteral("view.front"),       QStringLiteral("view.back"),
                               QStringLiteral("view.left"),        QStringLiteral("view.right"),
                               QStringLiteral("view.top"),         QStringLiteral("view.bottom"),
                               QStringLiteral("view.look_at"),     QStringLiteral("view.orthographic"),
                               QStringLiteral("view.perspective"), QStringLiteral("view.turn_left"),
                               QStringLiteral("view.turn_right"),  QStringLiteral("view.turn_up"),
                               QStringLiteral("view.turn_down"),
                               // mitcad#2473
                               QStringLiteral("view.perspective_orthographic_faces")};
  return ids;
}

const QStringList& guideNavigationActions() {
  static const QStringList actions{QStringLiteral("orbit"), QStringLiteral("pan"), QStringLiteral("zoom")};
  return actions;
}

const QStringList& guidePointPlaces() {
  static const QStringList places{
      QStringLiteral("origin"),        QStringLiteral("x_axis"),       QStringLiteral("y_axis"),
      QStringLiteral("point"),         QStringLiteral("center"),       QStringLiteral("midpoint"),
      QStringLiteral("quadrant"),      QStringLiteral("intersection"), QStringLiteral("curve"),
      QStringLiteral("extension"),     QStringLiteral("body_vertex"),  QStringLiteral("body_midpoint"),
      QStringLiteral("body_edge"),     QStringLiteral("body_center"),  QStringLiteral("body_section"),
      QStringLiteral("grid"),          QStringLiteral("free")};
  return places;
}

const QStringList& guideOriginPlanes() {
  static const QStringList planes{QStringLiteral("xy"), QStringLiteral("xz"), QStringLiteral("yz"),
                                  QStringLiteral("face")};
  return planes;
}

bool guideValueMatches(const QString& typed, const QString& wanted) {
  const QString a = normalValue(typed);
  const QString b = normalValue(wanted);
  if (a.isEmpty()) {
    return false;
  }
  if (a == b) {
    return true;
  }
  // The same number written otherwise: "20.0", "20,0".
  bool numberA = false;
  bool numberB = false;
  const double x = QString(a).replace(QLatin1Char(','), QLatin1Char('.')).toDouble(&numberA);
  const double y = QString(b).replace(QLatin1Char(','), QLatin1Char('.')).toDouble(&numberB);
  return numberA && numberB && std::abs(x - y) <= 1e-9 * std::max(1.0, std::abs(y));
}

GuideStep GuideStep::parse(const QJsonObject& json, QString* error) {
  GuideStep step;
  const auto fail = [&step, error](const QString& reason) {
    *error = reason;
    step.id.clear();
    return step;
  };
  static const QStringList known{QStringLiteral("id"),    QStringLiteral("text"),       QStringLiteral("target"),
                                 QStringLiteral("allow"), QStringLiteral("navigation"), QStringLiteral("done"),
                                 QStringLiteral("hold")};
  for (auto it = json.begin(); it != json.end(); ++it) {
    if (!known.contains(it.key())) {
      return fail(QStringLiteral("A step has no field \"%1\"; its fields are %2")
                      .arg(it.key(), known.join(QStringLiteral(", "))));
    }
  }
  step.id = json.value(QStringLiteral("id")).toString(QStringLiteral("step")).trimmed();
  if (step.id.isEmpty()) {
    step.id = QStringLiteral("step");
  }
  step.text = json.value(QStringLiteral("text")).toString().trimmed();
  if (step.text.isEmpty()) {
    return fail(QStringLiteral("A step needs a text, the callout shown beside its target"));
  }
  if (step.text.size() > kMaxText) {
    return fail(QStringLiteral("A step's text is at most %1 characters").arg(kMaxText));
  }

  const QJsonObject targetJson = json.value(QStringLiteral("target")).toObject();
  QString reason;
  const QString targetKey =
      oneOf(targetJson,
            {QStringLiteral("command"), QStringLiteral("input"), QStringLiteral("browser"), QStringLiteral("view"),
             QStringLiteral("workspace_selector"), QStringLiteral("workspace"), QStringLiteral("tab"),
             QStringLiteral("timeline"), QStringLiteral("plane"), QStringLiteral("field")},
            QStringLiteral("A step's target"), &reason);
  if (targetKey.isEmpty()) {
    return fail(reason);
  }
  if (targetKey == QLatin1String("view")) {
    step.target = Target::View;
    if (!readKinds(targetJson.value(targetKey), QStringLiteral("The target's view"), &step.pickKinds, &reason)) {
      return fail(reason);
    }
  } else if (targetKey == QLatin1String("workspace_selector") || targetKey == QLatin1String("timeline")) {
    // A place of its own, one in a window (mitcad#2370): true.
    step.target = targetKey == QLatin1String("timeline") ? Target::Timeline : Target::WorkspaceSelector;
    const QJsonValue value = targetJson.value(targetKey);
    if (!value.isBool() || !value.toBool()) {
      return fail(QStringLiteral("The target's %1 is true").arg(targetKey));
    }
  } else {
    step.target = targetKey == QLatin1String("command")     ? Target::Command
                  : targetKey == QLatin1String("input")     ? Target::Input
                  : targetKey == QLatin1String("workspace") ? Target::Workspace
                  : targetKey == QLatin1String("tab")       ? Target::Tab
                  : targetKey == QLatin1String("plane")     ? Target::Plane
                  : targetKey == QLatin1String("field")     ? Target::Field
                                                            : Target::Browser;
    step.targetName = targetJson.value(targetKey).toString().trimmed();
    if (step.targetName.isEmpty()) {
      return fail(QStringLiteral("The target's %1 is a name").arg(targetKey));
    }
  }

  const QJsonValue allow = json.value(QStringLiteral("allow"));
  if (!allow.isUndefined()) {
    if (!allow.isArray()) {
      return fail(QStringLiteral("A step's allow is a list of command ids"));
    }
    for (const QJsonValue& id : allow.toArray()) {
      if (!id.isString() || id.toString().isEmpty()) {
        return fail(QStringLiteral("A step's allow is a list of command ids"));
      }
      step.allowed << id.toString();
    }
  }
  const QJsonValue navigationJson = json.value(QStringLiteral("navigation"));
  if (!navigationJson.isUndefined() && !navigationJson.isBool()) {
    return fail(QStringLiteral("A step's navigation is true or false"));
  }
  step.navigation = navigationJson.toBool(true);
  const QJsonValue holdJson = json.value(QStringLiteral("hold"));
  if (!holdJson.isUndefined() && !holdJson.isBool()) {
    return fail(QStringLiteral("A step's hold is true or false"));
  }
  step.hold = holdJson.toBool(false);

  const QJsonValue doneValue = json.value(QStringLiteral("done"));
  if (doneValue.isUndefined()) {
    // No done (mitcad#2370): the step points at its target until the page
    // or MCP ends it (a page's Continue), Escape or Leave Tutorial.
    step.done = Done::None;
    return step;
  }
  if (!doneValue.isObject()) {
    return fail(QStringLiteral("A step's done is an object"));
  }
  const QJsonObject doneJson = doneValue.toObject();
  const QString doneKey =
      oneOf(doneJson,
            {QStringLiteral("command"), QStringLiteral("started"), QStringLiteral("input"), QStringLiteral("pick"),
             QStringLiteral("mode"), QStringLiteral("navigate"), QStringLiteral("opened"), QStringLiteral("chosen"),
             QStringLiteral("activated"), QStringLiteral("plane"), QStringLiteral("point"), QStringLiteral("field")},
            QStringLiteral("A step's done"), &reason);
  if (doneKey.isEmpty()) {
    return fail(reason);
  }
  if (doneKey == QLatin1String("point")) {
    // The next point of a 2D sketch's drawing tool (mitcad#2440).
    step.done = Done::Point;
    if (!readPoint(doneJson.value(doneKey), &step, &reason)) {
      return fail(reason);
    }
  } else if (doneKey == QLatin1String("pick")) {
    step.done = Done::Pick;
    QStringList kinds;
    if (!readKinds(doneJson.value(doneKey), QStringLiteral("The done pick"), &kinds, &reason)) {
      return fail(reason);
    }
    // The view target's kinds and the done pick's are one filter.
    step.pickKinds = kinds.isEmpty() ? step.pickKinds : kinds;
  } else {
    step.done = doneKey == QLatin1String("command")     ? Done::Finished
                : doneKey == QLatin1String("started")   ? Done::Started
                : doneKey == QLatin1String("mode")      ? Done::Mode
                : doneKey == QLatin1String("navigate")  ? Done::Navigate
                : doneKey == QLatin1String("opened")    ? Done::Opened
                : doneKey == QLatin1String("chosen")    ? Done::Chosen
                : doneKey == QLatin1String("activated") ? Done::Activated
                : doneKey == QLatin1String("plane")     ? Done::Plane
                : doneKey == QLatin1String("field")     ? Done::Field
                                                        : Done::Value;
    step.doneName = doneJson.value(doneKey).toString().trimmed();
    if (step.doneName.isEmpty()) {
      return fail(QStringLiteral("The done %1 is a name").arg(doneKey));
    }
    if (step.done == Done::Mode && !guideModes().contains(step.doneName)) {
      return fail(QStringLiteral("The done mode is one of %1").arg(guideModes().join(QStringLiteral(", "))));
    }
    if (step.done == Done::Navigate) {
      if (!guideNavigationActions().contains(step.doneName)) {
        return fail(QStringLiteral("The done navigate is one of %1")
                        .arg(guideNavigationActions().join(QStringLiteral(", "))));
      }
      // The learner navigates to complete it: the view's navigation stays free.
      if (!step.navigation) {
        return fail(QStringLiteral("A done navigate needs the step's navigation"));
      }
    }
    if (step.done == Done::Opened && step.doneName != QLatin1String(kGuideWorkspaceSelector)) {
      return fail(QStringLiteral("The done opened is %1").arg(QLatin1String(kGuideWorkspaceSelector)));
    }
    if (step.done == Done::Value || step.done == Done::Field) {
      step.value = doneJson.value(QStringLiteral("value")).toString().trimmed();
      if (step.value.isEmpty()) {
        return fail(QStringLiteral("A done %1 needs the value typed into it").arg(doneKey));
      }
    }
    // One plane (mitcad#2440): the target's and the done's are the same.
    if (step.done == Done::Plane && step.target == Target::Plane &&
        step.doneName.compare(step.targetName, Qt::CaseInsensitive) != 0) {
      return fail(QStringLiteral("A step's target plane and its done plane are one plane"));
    }
  }
  if (step.done != Done::Value && step.done != Done::Field && doneJson.contains(QStringLiteral("value"))) {
    return fail(QStringLiteral("A done value goes with an input or a field"));
  }
  return step;
}

QJsonObject GuideStep::toJson() const {
  QJsonObject aim;
  switch (target) {
  case Target::Command: aim.insert(QStringLiteral("command"), targetName); break;
  case Target::Input: aim.insert(QStringLiteral("input"), targetName); break;
  case Target::Browser: aim.insert(QStringLiteral("browser"), targetName); break;
  case Target::View:
    aim.insert(QStringLiteral("view"), pickKinds.isEmpty() ? QJsonValue(true) : QJsonValue(QJsonArray::fromStringList(pickKinds)));
    break;
  case Target::WorkspaceSelector: aim.insert(QStringLiteral("workspace_selector"), true); break;
  case Target::Workspace: aim.insert(QStringLiteral("workspace"), targetName); break;
  case Target::Tab: aim.insert(QStringLiteral("tab"), targetName); break;
  case Target::Timeline: aim.insert(QStringLiteral("timeline"), true); break;
  case Target::Plane: aim.insert(QStringLiteral("plane"), targetName); break;
  case Target::Field: aim.insert(QStringLiteral("field"), targetName); break;
  }
  QJsonObject finished;
  switch (done) {
  case Done::Finished: finished.insert(QStringLiteral("command"), doneName); break;
  case Done::Started: finished.insert(QStringLiteral("started"), doneName); break;
  case Done::Value:
    finished.insert(QStringLiteral("input"), doneName);
    finished.insert(QStringLiteral("value"), value);
    break;
  case Done::Mode: finished.insert(QStringLiteral("mode"), doneName); break;
  case Done::Navigate: finished.insert(QStringLiteral("navigate"), doneName); break;
  case Done::Pick:
    finished.insert(QStringLiteral("pick"),
                    pickKinds.isEmpty() ? QJsonValue(true) : QJsonValue(QJsonArray::fromStringList(pickKinds)));
    break;
  case Done::Opened: finished.insert(QStringLiteral("opened"), doneName); break;
  case Done::Chosen: finished.insert(QStringLiteral("chosen"), doneName); break;
  case Done::Activated: finished.insert(QStringLiteral("activated"), doneName); break;
  case Done::None: break; // no done: left out
  case Done::Plane: finished.insert(QStringLiteral("plane"), doneName); break;
  case Done::Point:
    if (pointAtGiven) {
      finished.insert(QStringLiteral("point"), QJsonArray{pointAt.x(), pointAt.y()});
    } else {
      finished.insert(QStringLiteral("point"), pointPlaces.size() == 1
                                                   ? QJsonValue(pointPlaces.first())
                                                   : QJsonValue(QJsonArray::fromStringList(pointPlaces)));
    }
    break;
  case Done::Field:
    finished.insert(QStringLiteral("field"), doneName);
    finished.insert(QStringLiteral("value"), value);
    break;
  }
  QJsonObject json{{QStringLiteral("id"), id},
                   {QStringLiteral("text"), text},
                   {QStringLiteral("target"), aim},
                   {QStringLiteral("navigation"), navigation}};
  if (done != Done::None) {
    json.insert(QStringLiteral("done"), finished);
  }
  if (!allowed.isEmpty()) {
    json.insert(QStringLiteral("allow"), QJsonArray::fromStringList(allowed));
  }
  if (hold) {
    json.insert(QStringLiteral("hold"), true);
  }
  return json;
}

QStringList GuideStep::commands() const {
  QStringList ids;
  if (target == Target::Command) {
    ids << targetName;
  }
  if ((done == Done::Finished || done == Done::Started) && !ids.contains(doneName)) {
    ids << doneName;
  }
  return ids;
}

bool GuideStep::allowsCommand(const QString& command) const {
  if (command == QLatin1String(kLeaveTutorialCommand) || commands().contains(command) || allowed.contains(command)) {
    return true;
  }
  return navigation && guideNavigationCommands().contains(command);
}

bool GuideStep::allowsCommandWhileHeld(const QString& command) const {
  return command == QLatin1String(kLeaveTutorialCommand) ||
         (navigation && guideNavigationCommands().contains(command));
}

bool GuideStep::pickMatches(const QString& kind) const { return pickKinds.isEmpty() || pickKinds.contains(kind); }

QString GuideStep::plane() const {
  if (target == Target::Plane) {
    return targetName;
  }
  return done == Done::Plane ? doneName : QString();
}

bool GuideStep::planeMatches(const QStringList& names) const {
  const QString wanted = plane();
  if (wanted.isEmpty()) {
    return true;
  }
  return std::any_of(names.begin(), names.end(),
                     [&wanted](const QString& name) { return name.compare(wanted, Qt::CaseInsensitive) == 0; });
}

bool GuideStep::pointMatches(const GuidePoint& point) const {
  if (done != Done::Point) {
    return false;
  }
  if (pointAtGiven) {
    return std::abs(point.at.x() - pointAt.x()) <= kGuidePointTolerance &&
           std::abs(point.at.y() - pointAt.y()) <= kGuidePointTolerance;
  }
  return std::any_of(pointPlaces.begin(), pointPlaces.end(),
                     [&point](const QString& place) { return point.places.contains(place); });
}

} // namespace mitcad
