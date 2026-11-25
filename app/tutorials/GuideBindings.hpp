// SPDX-License-Identifier: MIT
#pragma once

// The learner's own bindings for a tutorial page (mitcad#2366, GitHub
// mitcad/Mitcad#625; docs/tutorials.md "The learner's keys and gestures"): a
// command's keys as they are now (the shortcuts the user set, or the
// defaults) and the view's navigation gestures of the scheme chosen in
// Preferences, as structured keys, modifiers and mouse buttons, so that a
// page can draw the keycaps and the mouse of what works on this machine.
// Plain Qt Core and Gui (app/tests/guide_demonstration.cpp).

#include <QJsonArray>
#include <QJsonObject>
#include <QKeySequence>
#include <QList>

#include "../view/Navigation.hpp"

namespace mitcad {

// The modifiers' names: "shift", "ctrl", "alt", "meta" (Qt's: on macOS
// "ctrl" is the Command key and "meta" the Control key).
QJsonArray guideModifierNames(Qt::KeyboardModifiers modifiers);

// A command's key sequences: each {"keys": [{"key": "E", "modifiers":
// ["ctrl"]}, ...], "text": <as the platform writes it>}; an empty array for
// a command with no key.
QJsonArray guideShortcutsJson(const QList<QKeySequence>& sequences);

// The view's gestures: {"orbit": [...], "pan": [...], "zoom": [...],
// "reverse_zoom": bool, "custom": bool}, each gesture {"input": "left" |
// "middle" | "right" | "scroll", "modifiers": [...]} and, for one with a key
// held down, "key" ("F4"). "scroll" is the wheel or a two-finger scroll. In
// the order left, middle, right button, scroll; a gesture that needs more
// keys than another of the same input for the same action is left out (any
// Shift + scroll zooms when the plain scroll does).
QJsonObject guideNavigationJson(const NavigationSettings& settings);

} // namespace mitcad
