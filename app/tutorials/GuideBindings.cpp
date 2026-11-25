// SPDX-License-Identifier: MIT
// The learner's own bindings for a tutorial page (mitcad#2366): see
// GuideBindings.hpp.
#include "GuideBindings.hpp"

#include <QString>

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

QJsonArray gestures(const NavigationSettings& settings, NavigationAction action) {
  // Every gesture the view knows (the Custom table's rows: each input with
  // each set of Shift, Ctrl and Alt, and F4 + left), resolved as the view
  // resolves it.
  QVector<NavigationBinding> bound;
  for (const NavigationBinding& row : navigationBindings(NavigationScheme::Custom)) {
    if (navigationAction(settings, row.input, row.modifiers, row.f4) == action) {
      bound.push_back(row);
    }
  }
  QJsonArray result;
  for (const NavigationBinding& binding : bound) {
    bool needsMore = false;
    for (const NavigationBinding& other : bound) {
      if (other.input == binding.input && other.f4 == binding.f4 && other.modifiers != binding.modifiers &&
          (other.modifiers & binding.modifiers) == other.modifiers) {
        needsMore = true;
        break;
      }
    }
    if (needsMore) {
      continue;
    }
    QJsonObject gesture{{QStringLiteral("input"), QLatin1String(inputName(binding.input))},
                        {QStringLiteral("modifiers"), guideModifierNames(binding.modifiers)}};
    if (binding.f4) {
      gesture.insert(QStringLiteral("key"), QStringLiteral("F4"));
    }
    result.append(gesture);
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

QJsonObject guideNavigationJson(const NavigationSettings& settings) {
  return {{QStringLiteral("orbit"), gestures(settings, NavigationAction::Orbit)},
          {QStringLiteral("pan"), gestures(settings, NavigationAction::Pan)},
          {QStringLiteral("zoom"), gestures(settings, NavigationAction::Zoom)},
          {QStringLiteral("reverse_zoom"), settings.reverseZoom},
          {QStringLiteral("custom"), settings.scheme == NavigationScheme::Custom}};
}

} // namespace mitcad
