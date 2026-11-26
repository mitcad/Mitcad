// SPDX-License-Identifier: MIT
#pragma once

// The learner's own bindings for a tutorial page (mitcad#2366, GitHub
// mitcad/Mitcad#625; docs/tutorials.md "The learner's keys and gestures"): a
// command's keys as they are now (the shortcuts the user set, or the
// defaults) and the view's navigation gestures of the scheme chosen in
// Preferences, as structured keys, modifiers and mouse buttons, so that a
// page can draw the keycaps and the mouse of what works on this machine;
// since mitcad#2439 (GitHub mitcad/Mitcad#627) each device on its own, the
// trackpad's slide, pinch and press-and-drag beside the mouse's buttons and
// wheel. Qt Core and Gui and the command registry
// (app/tests/guide_demonstration.cpp).

#include <QJsonArray>
#include <QJsonObject>
#include <QKeySequence>
#include <QList>
#include <QString>

#include "../view/Navigation.hpp"

namespace mitcad {

class CommandRegistry;

// The modifiers' names: "shift", "ctrl", "alt", "meta" (Qt's: on macOS
// "ctrl" is the Command key, "alt" the Option key and "meta" the Control
// key).
QJsonArray guideModifierNames(Qt::KeyboardModifiers modifiers);

// The platform the modifiers' names are for (mitcad#2439): "macos",
// "windows" or "linux" (every other system).
QString guidePlatformName();

// A command's key sequences: each {"keys": [{"key": "E", "modifiers":
// ["ctrl"]}, ...], "text": <as the platform writes it>}; an empty array for
// a command with no key.
QJsonArray guideShortcutsJson(const QList<QKeySequence>& sequences);

// The bridge's `shortcut` answer (mitcad#2439): {"command": <id>, "name":
// <its name>, "shortcuts": guideShortcutsJson(<its keys now>), "platform":
// guidePlatformName()}, read from the registry at each call, so a key the
// learner changed is in the next answer. False, with the reason in `error`,
// for a command MitCad does not have.
bool guideShortcutAnswer(const CommandRegistry& registry, const QString& id, QJsonObject* answer, QString* error);

// The view's gestures: {"orbit": [...], "pan": [...], "zoom": [...],
// "mouse": {...}, "trackpad": {...}, "reverse_zoom": bool, "custom": bool,
// "platform": guidePlatformName()}.
//
// "orbit", "pan" and "zoom" are the first form (mitcad#2366), kept for the
// pages that read it: each gesture {"input": "left" | "middle" | "right" |
// "scroll", "modifiers": [...]} and, for one with a key held down, "key"
// ("F4"); "scroll" is the wheel or a two-finger scroll as the scheme's table
// has it. In the order left, middle, right button, scroll, then F4.
//
// "mouse" and "trackpad" (mitcad#2439) are {"orbit": [...], "pan": [...],
// "zoom": [...]} of one device, each gesture {"device": "mouse" |
// "trackpad", "input": ..., "modifiers": [...]} (and "key": "F4"):
// - the mouse's "left", "middle" and "right" drags, then the "wheel" (it
//   zooms in every scheme, whatever keys are held);
// - the trackpad's "slide" (two fingers, "fingers": 2), "pinch" (two
//   fingers; it zooms in every scheme, whatever keys are held) and "drag"
//   (the pad pressed, or its button, and moved: the left button;
//   "fingers": 1), in that order.
// In every list a gesture that needs more keys than another of the same
// input for the same action is left out (any Shift + scroll zooms when the
// plain scroll does).
QJsonObject guideNavigationJson(const NavigationSettings& settings);

} // namespace mitcad
