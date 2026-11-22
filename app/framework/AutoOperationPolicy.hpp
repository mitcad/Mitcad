// SPDX-License-Identifier: MIT
#pragma once

#include <string>
#include <utility>

namespace mitcad {

// Preferences › General › "Extrude starts with" (mitcad#2164, GitHub
// mitcad/Mitcad#496; framework/ExtrudeStartPreference.hpp keeps it): the
// operation a new extrusion starts with. Automatic (the default) lets the
// model choose it from the bodies the tool touches (mitcad#289); New Body
// starts it with New Body and the model chooses nothing, so that a
// profile on a body's face makes a new body unless the user picks Join,
// Cut, Intersect or New Component.
enum class NewFeatureOperation { Automatic, NewBody };

// Whether a command's Operation and Objects follow the model's automatic
// choice (mitcad#289, Extrude's CommandDef::autoOperation). A new feature
// follows it until the user picks Operation, Objects or Affected
// components, unless the preference starts it with New Body (mitcad#2164).
// An edit never does: an edited feature keeps the operation
// stored with it, also when Flip, Start or a distance moves its tool into
// a body or out of it, and the user changes Operation by hand (the owner,
// mitcad#868, 28.10.2026; it replaces mitcad#575's edits that followed the
// geometry when the stored operation matched the automatic choice, which
// turned an edited boss's Join into a Cut when Flip pointed it into its
// arm).
//
// A direction flip whose preview fails (mitcad#2216): while a new feature
// is made and its operation no longer follows the automatic choice (chosen
// by hand, or New Body by the preference), Flip or a distance whose sign
// changes (the arrow dragged through the start) asks the model for its
// suggestion with that preview. When the preview fails with the current
// operation and the suggestion is the other one (Join or New Body → Cut
// going into a body, Cut → Join coming out of it), that operation is tried
// once; it is taken when its preview works, else the user's operation
// comes back with its failure. A flip whose preview works keeps the user's
// operation. An edit never does it (mitcad#868).
class AutoOperationPolicy {
public:
  // What the session does with the preview of a flip.
  enum class FlipAction {
    Keep,    // nothing: the preview stays as it is
    Try,     // try `operation` instead (its preview follows)
    Taken,   // the tried operation works: it stays
    Restore, // neither works: the user's operation comes back
  };
  struct FlipStep {
    FlipAction action = FlipAction::Keep;
    std::string operation; // Try's operation
  };

  // enabled: the command has an automatic operation; editing: it edits a
  // feature; preference: what a new feature starts with.
  void start(bool enabled, bool editing, NewFeatureOperation preference = NewFeatureOperation::Automatic) {
    m_offByPreference = enabled && !editing && preference == NewFeatureOperation::NewBody;
    m_active = enabled && !editing && !m_offByPreference;
    m_flips = enabled && !editing;
    m_direction = 0;
    m_flip = Flip::None;
  }
  bool active() const { return m_active; }
  // The preference, not an edit or the command, kept the automatic choice
  // off at the start (what the log says).
  bool offByPreference() const { return m_offByPreference; }
  // The user's own choice stays for the rest of the command.
  void disable() { m_active = false; }

  // The tool's direction before a preview (CommandDef::toolDirection: +1
  // or -1, 0 when it has none, which changes nothing). A change while the
  // automatic choice is off makes a flip pending; flipped back before its
  // preview came, none is.
  void observeDirection(int direction) {
    if (!m_flips || direction == 0) {
      return;
    }
    if (m_direction != 0 && direction != m_direction && !m_active) {
      m_flip = m_flip == Flip::Pending ? Flip::None : Flip::Pending;
    }
    m_direction = direction;
  }
  // The preview asks the model for its suggestion: the automatic choice,
  // or a flip whose preview may fail.
  bool asksSuggestion() const { return m_active || m_flip == Flip::Pending; }
  // A flip waits for its preview or its tried operation's.
  bool flipWaiting() const { return m_flip != Flip::None; }
  // The user changed Operation, Objects or the targets: their choice stays.
  void cancelFlip() { m_flip = Flip::None; }

  // The preview of the inputs came back: whether it works, the operation
  // it was made with and the model's suggestion for its tool.
  FlipStep flipPreviewed(bool works, const std::string& current, const std::string& suggested) {
    const Flip was = m_flip;
    m_flip = Flip::None;
    if (was == Flip::Trying) {
      return {works ? FlipAction::Taken : FlipAction::Restore, current};
    }
    if (was != Flip::Pending || works) {
      return {};
    }
    std::string other = flippedOperation(current, suggested);
    if (other.empty()) {
      return {};
    }
    m_flip = Flip::Trying;
    return {FlipAction::Try, std::move(other)};
  }

  // The operation a failed flip tries: Cut when the tool now goes into a
  // body (from Join or New Body), Join when it now comes out of one (from
  // Cut); none otherwise (Intersect, New Component, a suggestion that is
  // the current operation or New Body).
  static std::string flippedOperation(const std::string& current, const std::string& suggested) {
    if ((current == "join" || current == "new_body") && suggested == "cut") {
      return suggested;
    }
    if (current == "cut" && suggested == "join") {
      return suggested;
    }
    return {};
  }

private:
  enum class Flip { None, Pending, Trying };
  bool m_active = false;
  bool m_offByPreference = false;
  bool m_flips = false;
  int m_direction = 0;
  Flip m_flip = Flip::None;
};

} // namespace mitcad
