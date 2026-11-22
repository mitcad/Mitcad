// SPDX-License-Identifier: MIT
// When a command's Operation follows the model's automatic choice
// (AutoOperationPolicy): a new feature until the user picks one; an edit
// never, it keeps its stored operation (mitcad#868: Flip pointed an edited
// boss into its arm and its Join became a Cut). With Preferences › General
// › Extrude starts with: New Body (mitcad#2164), a new feature never
// follows it either, and the preference is kept in the user's settings.
#include "UnitTests.hpp"
#include "framework/AutoOperationPolicy.hpp"
#include "framework/ExtrudeStartPreference.hpp"

#include <cstdio>

#include <QCoreApplication>
#include <QSettings>
#include <QTemporaryDir>

namespace {

// The preference saved and read back in settings of the test's own.
int extrudeStartSettingsTests() {
  int failures = 0;
  const auto check = [&failures](bool ok, const char* message) {
    if (!ok) { std::fprintf(stderr, "extrude start preference: %s\n", message); ++failures; }
  };
  using mitcad::NewFeatureOperation;
  check(mitcad::newFeatureOperationId(NewFeatureOperation::Automatic) == QStringLiteral("automatic"),
        "Automatic is kept as \"automatic\"");
  check(mitcad::newFeatureOperationId(NewFeatureOperation::NewBody) == QStringLiteral("new_body"),
        "New Body is kept as \"new_body\"");
  check(mitcad::newFeatureOperationFromId(QStringLiteral("new_body")) == NewFeatureOperation::NewBody,
        "\"new_body\" reads as New Body");
  check(!mitcad::newFeatureOperationFromId(QStringLiteral("join")).has_value(),
        "another operation is no choice of the preference");

  QTemporaryDir directory;
  check(directory.isValid(), "a settings folder of the test's own");
  const QString organization = QCoreApplication::organizationName();
  const QString application = QCoreApplication::applicationName();
  const QSettings::Format format = QSettings::defaultFormat();
  QCoreApplication::setOrganizationName(QStringLiteral("MitcadExtrudeStartTest"));
  QCoreApplication::setApplicationName(QStringLiteral("MitcadExtrudeStartTest"));
  QSettings::setDefaultFormat(QSettings::IniFormat);
  QSettings::setPath(QSettings::IniFormat, QSettings::UserScope, directory.path());
  {
    QSettings settings;
    settings.clear();
    settings.sync();
  }
  check(mitcad::savedExtrudeStart() == NewFeatureOperation::Automatic, "nothing saved: Automatic, as before");
  check(mitcad::saveExtrudeStart(NewFeatureOperation::NewBody), "New Body saved");
  check(mitcad::savedExtrudeStart() == NewFeatureOperation::NewBody, "New Body read back");
  check(QSettings().value(QStringLiteral("extrude/startOperation")).toString() == QStringLiteral("new_body"),
        "kept as extrude/startOperation = new_body (the Preferences group of Export Settings)");
  check(!mitcad::saveExtrudeStart(NewFeatureOperation::NewBody), "the same choice again changes nothing");
  QSettings().setValue(QStringLiteral("extrude/startOperation"), QStringLiteral("cut"));
  check(mitcad::savedExtrudeStart() == NewFeatureOperation::Automatic, "an unknown value reads as Automatic");
  check(mitcad::saveExtrudeStart(NewFeatureOperation::NewBody) &&
            mitcad::saveExtrudeStart(NewFeatureOperation::Automatic) &&
            mitcad::savedExtrudeStart() == NewFeatureOperation::Automatic,
        "Automatic saved again");
  {
    QSettings settings;
    settings.clear();
    settings.sync();
  }
  QCoreApplication::setOrganizationName(organization);
  QCoreApplication::setApplicationName(application);
  QSettings::setDefaultFormat(format);
  return failures;
}

// A direction flip whose preview fails tries the model's other operation
// (mitcad#2216): only in a new feature whose operation no longer follows
// the automatic choice, only when the preview fails, and only Join or New
// Body → Cut going in, Cut → Join coming out.
int flipRetryTests() {
  int failures = 0;
  const auto check = [&failures](bool ok, const char* message) {
    if (!ok) { std::fprintf(stderr, "flip retry: %s\n", message); ++failures; }
  };
  using mitcad::AutoOperationPolicy;
  using mitcad::NewFeatureOperation;
  using Action = AutoOperationPolicy::FlipAction;

  check(AutoOperationPolicy::flippedOperation("join", "cut") == "cut", "Join going into a body tries Cut");
  check(AutoOperationPolicy::flippedOperation("new_body", "cut") == "cut", "New Body going into a body tries Cut");
  check(AutoOperationPolicy::flippedOperation("cut", "join") == "join", "Cut coming out of a body tries Join");
  check(AutoOperationPolicy::flippedOperation("cut", "new_body").empty(),
        "Cut away from every body tries nothing (New Body is no flip's operation)");
  check(AutoOperationPolicy::flippedOperation("join", "join").empty(), "the same operation is nothing to try");
  check(AutoOperationPolicy::flippedOperation("join", "new_body").empty(), "Join never becomes New Body");
  check(AutoOperationPolicy::flippedOperation("intersect", "cut").empty(), "Intersect stays");
  check(AutoOperationPolicy::flippedOperation("new_component", "cut").empty(), "New Component stays");

  // Automatic mode: the automatic choice follows the tool already, a flip
  // is never pending.
  AutoOperationPolicy state;
  state.start(true, false);
  state.observeDirection(1);
  state.observeDirection(-1);
  check(!state.flipWaiting(), "automatic mode: the automatic choice follows a flip, no retry");
  check(state.asksSuggestion(), "automatic mode asks for the suggestion");

  // A Join chosen by hand: Join, flipped into the body (fails) → Cut.
  state.disable();
  check(!state.asksSuggestion(), "a hand choice asks for no suggestion without a flip");
  state.observeDirection(-1);
  check(!state.flipWaiting(), "the direction it had when the user chose: no flip");
  state.observeDirection(1);
  check(state.flipWaiting() && state.asksSuggestion(), "a flip after the hand choice asks for the suggestion");
  AutoOperationPolicy::FlipStep step = state.flipPreviewed(false, "join", "cut");
  check(step.action == Action::Try && step.operation == "cut", "the failed Join tries Cut");
  check(state.flipWaiting() && !state.asksSuggestion(), "the tried Cut's preview is waited for");
  step = state.flipPreviewed(true, "cut", "cut");
  check(step.action == Action::Taken, "the working Cut is taken");
  check(!state.flipWaiting(), "nothing waits after it");
  check(state.flipPreviewed(false, "cut", "join").action == Action::Keep,
        "a failure without a flip changes nothing (the user typed a distance too long)");

  // Flipped back out of the body (the Cut fails) → Join.
  state.observeDirection(-1);
  step = state.flipPreviewed(false, "cut", "join");
  check(step.action == Action::Try && step.operation == "join", "the failed Cut coming out tries Join");
  check(state.flipPreviewed(true, "join", "join").action == Action::Taken, "the working Join is taken");

  // A flip whose preview works keeps the user's operation.
  state.observeDirection(1);
  check(state.flipPreviewed(true, "join", "cut").action == Action::Keep,
        "a flip that works keeps the hand-chosen Join");
  check(!state.flipWaiting(), "and waits for nothing more");

  // Neither works: the user's operation comes back.
  state.observeDirection(-1);
  step = state.flipPreviewed(false, "join", "cut");
  check(step.action == Action::Try, "a failed flip tries the other operation");
  check(state.flipPreviewed(false, "cut", "cut").action == Action::Restore,
        "the other one failing too gives the user's operation back");
  check(!state.flipWaiting(), "its failure is shown without another try");

  // Flipped and back before the preview came: no flip. (The direction is
  // -1 here.)
  state.observeDirection(1);
  state.observeDirection(-1);
  check(!state.flipWaiting(), "a flip taken back before its preview is none");
  // A direction of none (two sides, a zero distance) changes nothing.
  state.observeDirection(0);
  state.observeDirection(-1);
  check(!state.flipWaiting(), "no direction in between is no flip");
  // The user's own Operation, Objects or targets end a waiting flip.
  state.observeDirection(1);
  check(state.flipWaiting(), "flipped again");
  state.cancelFlip();
  check(!state.flipWaiting() && !state.asksSuggestion(), "a hand change ends the flip's retry");
  // A suggestion that is no flip's operation: nothing tried.
  state.observeDirection(-1);
  check(state.flipPreviewed(false, "join", "new_body").action == Action::Keep,
        "a failed Join with nothing near tries nothing");

  // Preferences › General › Extrude starts with: New Body (mitcad#2164).
  state.start(true, false, NewFeatureOperation::NewBody);
  check(!state.asksSuggestion(), "New Body by preference asks for nothing at first");
  state.observeDirection(1);
  state.observeDirection(-1);
  check(state.flipWaiting(), "a flip under the New Body preference asks for the suggestion");
  check(state.flipPreviewed(true, "new_body", "cut").action == Action::Keep,
        "a New Body that works going in stays New Body");
  state.observeDirection(1);
  check(state.flipPreviewed(false, "new_body", "join").action == Action::Keep,
        "New Body never becomes Join by a flip");

  // An edit keeps its stored operation (mitcad#868), and a command without
  // an automatic operation never flips one.
  state.start(true, true);
  state.observeDirection(1);
  state.observeDirection(-1);
  check(!state.flipWaiting() && !state.asksSuggestion(), "an edit's flip tries nothing");
  state.start(false, false);
  state.observeDirection(1);
  state.observeDirection(-1);
  check(!state.flipWaiting(), "a command without an automatic operation tries nothing");
  // A new start forgets the last command's direction.
  state.start(true, false);
  state.disable();
  state.observeDirection(-1);
  check(!state.flipWaiting(), "the first direction of a new command is no flip");
  return failures;
}

} // namespace

int autoOperationPolicyTests() {
  int failures = 0;
  const auto check = [&failures](bool ok, const char* message) {
    if (!ok) { std::fprintf(stderr, "auto operation policy: %s\n", message); ++failures; }
  };
  using mitcad::AutoOperationPolicy;
  using mitcad::NewFeatureOperation;
  AutoOperationPolicy state;
  check(!state.active(), "nothing is automatic before a command starts");
  state.start(true, false);
  check(state.active(), "a new feature follows the automatic choice");
  check(!state.offByPreference(), "the default preference leaves it on");
  state.disable();
  check(!state.active(), "the user's own Operation, Objects or targets stop it for the rest of the command");
  state.start(true, true);
  check(!state.active(), "an edit keeps its stored operation (mitcad#868)");
  state.disable();
  check(!state.active(), "a hand change in an edit leaves it off");
  state.start(true, false);
  state.start(true, true);
  check(!state.active(), "an edit started after a new feature does not inherit its automatic mode");
  state.start(false, false);
  check(!state.active(), "commands without an automatic operation never follow one");

  // Preferences › General › Extrude starts with (mitcad#2164).
  state.start(true, false, NewFeatureOperation::Automatic);
  check(state.active() && !state.offByPreference(),
        "Operation from the bodies it touches: a new feature follows the automatic choice, as before");
  state.start(true, false, NewFeatureOperation::NewBody);
  check(!state.active(), "New Body: a new feature keeps New Body, nothing is chosen automatically");
  check(state.offByPreference(), "New Body: the preference turned it off (logged)");
  state.disable();
  check(!state.active(), "a hand choice under New Body leaves it off");
  state.start(true, true, NewFeatureOperation::NewBody);
  check(!state.active() && !state.offByPreference(),
        "an edit keeps its stored operation under either preference; the preference is not why");
  state.start(false, false, NewFeatureOperation::NewBody);
  check(!state.active() && !state.offByPreference(),
        "a command without an automatic operation is not affected by the preference");
  state.start(true, false, NewFeatureOperation::NewBody);
  state.start(true, false, NewFeatureOperation::Automatic);
  check(state.active() && !state.offByPreference(),
        "the preference set back to Automatic: the next Extrude chooses again");

  return failures + extrudeStartSettingsTests() + flipRetryTests();
}
MITCAD_UNIT_TEST(autoOperationPolicyTests);
