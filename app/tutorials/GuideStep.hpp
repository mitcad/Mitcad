// SPDX-License-Identifier: MIT
#pragma once

// A guide step of the in-app tutorials (mitcad#2012, docs/tutorials.md
// "Guide steps"): one engine for a tutorial page's bridge (window.mitcad)
// and live MCP's ui_guide. A step names its target as the UI names it (a
// command id at its toolbar button or menu item, an input of the open
// command's panel by its key, a Browser row by its name, the view with a
// pick filter), shows a short callout text beside it, allows its own
// commands and holds back the rest, and is done by what the user does (a
// command finished or its panel opened, a value typed into an input, a pick
// of a kind). Never pixels: a step says what, the application finds where.
//
// What a step is and when it is done, in plain Qt Core
// (app/tests/guide_step.cpp); MainWindowGuide.cpp shows it and watches the
// window.

#include <QJsonObject>
#include <QString>
#include <QStringList>

namespace mitcad {

struct GuideStep {
  enum class Target { Command, Input, Browser, View };
  enum class Done { Finished, Started, Value, Pick, Mode };

  QString id;   // the author's name of the step, echoed in its events
  QString text; // the callout's text
  Target target = Target::Command;
  // The target: a command id (Command), an input's key (Input), a Browser
  // row's name (Browser); empty for the view.
  QString targetName;
  // The kinds a pick in the view may be of ("face", "edge"; Selection's
  // kindName), for a View target or a Pick step; empty: any.
  QStringList pickKinds;
  // The commands it allows beyond its own (the target's and the done
  // command's): "allow".
  QStringList allowed;
  // View navigation (the wheel, the middle and right buttons, the camera
  // commands): allowed unless the step says "navigation": false.
  bool navigation = true;
  Done done = Done::Finished;
  // The command a Finished or Started step waits for, the input of a
  // Value step (of the open command), the mode a Mode step waits for
  // (guideModes()).
  QString doneName;
  // The value a Value step waits for, as typed ("20 mm").
  QString value;

  // The step from its JSON (docs/tutorials.md "Guide steps"); empty `id`
  // and the reason in `error` when it is not one. Its commands are not
  // checked here (MainWindow knows them).
  static GuideStep parse(const QJsonObject& json, QString* error);
  QJsonObject toJson() const;

  // The command ids it names (target, done), for MainWindow's check that
  // they exist.
  QStringList commands() const;
  // Whether `command` (an id) may run during the step: its own, the ones it
  // allows, Leave Tutorial, and the camera's while navigation is allowed.
  bool allowsCommand(const QString& command) const;
  // Whether the step takes picks in the view (a View target or a Pick step).
  bool takesPicks() const { return target == Target::View || done == Done::Pick; }
  // Whether a pick of `kind` (Selection::kindName) is one it waits for.
  bool pickMatches(const QString& kind) const;
};

// The window's modes a step can wait for: idle, pick_plane, sketch, command.
const QStringList& guideModes();
// The camera's commands, allowed while a step allows navigation.
const QStringList& guideNavigationCommands();
// Whether a value typed matches the one a step waits for: the same text
// without spaces and case, a length's "mm" (the default unit) optional
// ("20" is "20 mm" and "20mm").
bool guideValueMatches(const QString& typed, const QString& wanted);
// The command that leaves a tutorial, always allowed.
inline constexpr char kLeaveTutorialCommand[] = "help.leave_tutorial";

} // namespace mitcad
