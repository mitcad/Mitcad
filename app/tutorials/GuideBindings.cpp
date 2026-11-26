// SPDX-License-Identifier: MIT
// The learner's own bindings for a tutorial page (mitcad#2366, mitcad#2439):
// see GuideBindings.hpp.
#include "GuideBindings.hpp"

#include <QVector>

#include "../framework/CommandRegistry.hpp"

namespace mitcad {
namespace {

const char* inputName(NavigationInput input) {
  switch (input) {
  case NavigationInput::LeftButton: return "left";
  case NavigationInput::MiddleButton: return "middle";
  case NavigationInput::RightButton: return "right";
  case NavigationInput::TrackpadScroll: return "scroll";
  }
  return "left";
}

// The rows of the view's gestures bound to `action`, without one that needs
// more keys than another of the same input (the Custom table's rows: each
// input with each set of Shift, Ctrl and Alt, and F4 + left, resolved as the
// view resolves them), in the table's order.
QVector<NavigationBinding> boundRows(const NavigationSettings& settings, NavigationAction action) {
  QVector<NavigationBinding> bound;
  for (const NavigationBinding& row : navigationBindings(NavigationScheme::Custom)) {
    if (navigationAction(settings, row.input, row.modifiers, row.f4) == action) {
      bound.push_back(row);
    }
  }
  QVector<NavigationBinding> result;
  for (const NavigationBinding& binding : bound) {
    bool needsMore = false;
    for (const NavigationBinding& other : bound) {
      if (other.input == binding.input && other.f4 == binding.f4 && other.modifiers != binding.modifiers &&
          (other.modifiers & binding.modifiers) == other.modifiers) {
        needsMore = true;
        break;
      }
    }
    if (!needsMore) {
      result.push_back(binding);
    }
  }
  return result;
}

// One gesture of the device lists: {"device", "input", "modifiers"} with
// "fingers" for a trackpad's and "key" for F4 held.
QJsonObject deviceGesture(const char* device, const char* input, Qt::KeyboardModifiers modifiers, bool f4 = false,
                          int fingers = 0) {
  QJsonObject gesture{{QStringLiteral("device"), QLatin1String(device)},
                      {QStringLiteral("input"), QLatin1String(input)},
                      {QStringLiteral("modifiers"), guideModifierNames(modifiers)}};
  if (fingers > 0) {
    gesture.insert(QStringLiteral("fingers"), fingers);
  }
  if (f4) {
    gesture.insert(QStringLiteral("key"), QStringLiteral("F4"));
  }
  return gesture;
}

// The first form (mitcad#2366): every input of the table, the scroll
// standing for the wheel and the two-finger scroll alike.
QJsonArray tableGestures(const QVector<NavigationBinding>& rows) {
  QJsonArray result;
  for (const NavigationBinding& binding : rows) {
    QJsonObject gesture{{QStringLiteral("input"), QLatin1String(inputName(binding.input))},
                        {QStringLiteral("modifiers"), guideModifierNames(binding.modifiers)}};
    if (binding.f4) {
      gesture.insert(QStringLiteral("key"), QStringLiteral("F4"));
    }
    result.append(gesture);
  }
  return result;
}

// The mouse's gestures (mitcad#2439): its buttons' drags as the table has
// them, then the wheel, which zooms in every scheme whatever keys are held
// (OcctViewer::wheelEvent: only a trackpad's scroll follows the table).
QJsonArray mouseGestures(const QVector<NavigationBinding>& rows, NavigationAction action) {
  QJsonArray result;
  for (const NavigationBinding& binding : rows) {
    if (binding.input != NavigationInput::TrackpadScroll) {
      result.append(deviceGesture("mouse", inputName(binding.input), binding.modifiers, binding.f4));
    }
  }
  if (action == NavigationAction::Zoom) {
    result.append(deviceGesture("mouse", "wheel", Qt::NoModifier));
  }
  return result;
}

// The trackpad's gestures (mitcad#2439): the two-finger slide as the
// table's scroll rows have it (OcctViewer::trackpadScroll in the trackpad
// schemes and Custom; in the others every slide zooms, as their table
// says), the pinch, which zooms in every scheme whatever keys are held
// (OcctViewer::nativeGesture), and the pad pressed and moved, which is the
// left button's drag.
QJsonArray trackpadGestures(const QVector<NavigationBinding>& rows, NavigationAction action) {
  QJsonArray result;
  for (const NavigationBinding& binding : rows) {
    if (binding.input == NavigationInput::TrackpadScroll) {
      result.append(deviceGesture("trackpad", "slide", binding.modifiers, false, 2));
    }
  }
  if (action == NavigationAction::Zoom) {
    result.append(deviceGesture("trackpad", "pinch", Qt::NoModifier, false, 2));
  }
  for (const NavigationBinding& binding : rows) {
    if (binding.input == NavigationInput::LeftButton) {
      result.append(deviceGesture("trackpad", "drag", binding.modifiers, binding.f4, 1));
    }
  }
  return result;
}

} // namespace

QJsonArray guideModifierNames(Qt::KeyboardModifiers modifiers) {
  QJsonArray names;
  if (modifiers.testFlag(Qt::ShiftModifier)) {
    names.append(QStringLiteral("shift"));
  }
  if (modifiers.testFlag(Qt::ControlModifier)) {
    names.append(QStringLiteral("ctrl"));
  }
  if (modifiers.testFlag(Qt::AltModifier)) {
    names.append(QStringLiteral("alt"));
  }
  if (modifiers.testFlag(Qt::MetaModifier)) {
    names.append(QStringLiteral("meta"));
  }
  return names;
}

QString guidePlatformName() {
#if defined(Q_OS_MACOS)
  return QStringLiteral("macos");
#elif defined(Q_OS_WIN)
  return QStringLiteral("windows");
#else
  return QStringLiteral("linux");
#endif
}

QJsonArray guideShortcutsJson(const QList<QKeySequence>& sequences) {
  QJsonArray result;
  for (const QKeySequence& sequence : sequences) {
    if (sequence.isEmpty()) {
      continue;
    }
    QJsonArray keys;
    for (int i = 0; i < sequence.count(); ++i) {
      const QKeyCombination combination = sequence[i];
      keys.append(QJsonObject{
          {QStringLiteral("key"), QKeySequence(combination.key()).toString(QKeySequence::PortableText)},
          {QStringLiteral("modifiers"), guideModifierNames(combination.keyboardModifiers())}});
    }
    result.append(QJsonObject{{QStringLiteral("keys"), keys},
                              {QStringLiteral("text"), sequence.toString(QKeySequence::NativeText)}});
  }
  return result;
}

bool guideShortcutAnswer(const CommandRegistry& registry, const QString& id, QJsonObject* answer, QString* error) {
  const CommandDef* def = registry.find(id);
  if (def == nullptr) {
    *error = QStringLiteral("MitCad has no command %1").arg(id);
    return false;
  }
  *answer = {{QStringLiteral("command"), def->id},
             {QStringLiteral("name"), def->name},
             {QStringLiteral("shortcuts"), guideShortcutsJson(registry.effectiveShortcuts(def->id))},
             {QStringLiteral("platform"), guidePlatformName()}};
  return true;
}

QJsonObject guideNavigationJson(const NavigationSettings& settings) {
  const QVector<NavigationBinding> orbit = boundRows(settings, NavigationAction::Orbit);
  const QVector<NavigationBinding> pan = boundRows(settings, NavigationAction::Pan);
  const QVector<NavigationBinding> zoom = boundRows(settings, NavigationAction::Zoom);
  const QJsonObject mouse{{QStringLiteral("orbit"), mouseGestures(orbit, NavigationAction::Orbit)},
                          {QStringLiteral("pan"), mouseGestures(pan, NavigationAction::Pan)},
                          {QStringLiteral("zoom"), mouseGestures(zoom, NavigationAction::Zoom)}};
  const QJsonObject trackpad{{QStringLiteral("orbit"), trackpadGestures(orbit, NavigationAction::Orbit)},
                             {QStringLiteral("pan"), trackpadGestures(pan, NavigationAction::Pan)},
                             {QStringLiteral("zoom"), trackpadGestures(zoom, NavigationAction::Zoom)}};
  return {{QStringLiteral("orbit"), tableGestures(orbit)},
          {QStringLiteral("pan"), tableGestures(pan)},
          {QStringLiteral("zoom"), tableGestures(zoom)},
          {QStringLiteral("mouse"), mouse},
          {QStringLiteral("trackpad"), trackpad},
          {QStringLiteral("reverse_zoom"), settings.reverseZoom},
          {QStringLiteral("custom"), settings.scheme == NavigationScheme::Custom},
          {QStringLiteral("platform"), guidePlatformName()}};
}

} // namespace mitcad
