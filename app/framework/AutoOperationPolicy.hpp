// SPDX-License-Identifier: MIT
#pragma once

#include <optional>
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
//
// Its mirror (mitcad#2283, GitHub mitcad/Mitcad#541): Cut or Join chosen
// by hand in a new feature (One Side) whose preview fails while the other
// direction works turns the direction once (the command's Flip). The model
// tells whether the other direction works (a scoped Cut's
// `flip_would_cut`); without its answer (a Join, a Cut without targets)
// the flipped preview tells. It stays when its preview works; else the
// direction comes back with the first failure. The policy keeps the
// direction it turned, so that its own flip is no direction flip of the
// user's (above): a flipped Cut that fails never becomes Join. A preview
// that works is never turned; the user's own Flip, distance sign,
// Direction, Operation, Objects or targets end a waiting turn. Chosen
// while the tool has no direction yet (a new Extrude starts at zero,
// mitcad#2481), the choice's direction is the first one the distance
// gives, and its preview may turn it as above.
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
    m_turn = Turn::None;
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
  // preview came, none is. Any other direction than the one a waiting turn
  // (mitcad#2283) expects is the user's own and ends the turn: the
  // turn's own flip was kept as the direction already. A hand choice made
  // before the tool had a direction (a new Extrude at zero, mitcad#2481)
  // takes the first one as its own: its turn waits from then on.
  void observeDirection(int direction) {
    if (!m_flips) {
      return;
    }
    if (m_turn == Turn::Waiting) {
      if (direction != 0) {
        m_turn = Turn::Pending;
        m_direction = direction;
      }
      return;
    }
    if (m_turn != Turn::None && direction != m_direction) {
      m_turn = Turn::None;
    }
    if (direction == 0) {
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
  // The user changed Operation, Objects or the targets: their choice stays
  // (a turn of the direction waiting for its preview ends too).
  void cancelFlip() {
    m_flip = Flip::None;
    m_turn = Turn::None;
  }

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

  // What the session does with the preview of an operation chosen by hand
  // (mitcad#2283).
  enum class TurnAction {
    Keep,    // nothing: the preview stays as it is
    Turn,    // flip the tool's direction (its preview follows)
    Taken,   // the turned direction works: it stays
    Restore, // it fails too: the direction comes back with the failure
  };

  // The user chose `operation` by hand while the tool goes `direction`
  // (CommandDef::toolDirection; 0 for two sides or a symmetric extrusion):
  // a new feature's Cut or Join of one side waits for its preview. Any
  // other operation and an edit wait for nothing. Without a direction yet
  // (a distance still zero, mitcad#2481; also two sides or symmetric) it
  // waits for the first direction `observeDirection` sees; a preview made
  // without one (two sides, symmetric) ends the wait unturned.
  void operationChosen(const std::string& operation, int direction) {
    m_turn = Turn::None;
    if (!m_flips || (operation != "cut" && operation != "join")) {
      return;
    }
    if (direction == 0) {
      m_turn = Turn::Waiting;
      return;
    }
    m_turn = Turn::Pending;
    m_direction = direction;
  }
  // A turn waits for the hand choice's direction, its preview or the
  // turned one's.
  bool turnWaiting() const { return m_turn != Turn::None; }

  // The preview came back: whether it works, and the model's answer
  // whether the other direction works (a scoped Cut's `flip_would_cut`),
  // none when it gives none (the turned preview tells then).
  TurnAction operationPreviewed(bool works, std::optional<bool> otherWay) {
    const Turn was = m_turn;
    m_turn = Turn::None;
    if (was == Turn::Trying) {
      if (works) {
        return TurnAction::Taken;
      }
      // The session turns it back: that is no flip of the user's either.
      m_direction = -m_direction;
      return TurnAction::Restore;
    }
    if (was != Turn::Pending || works || (otherWay.has_value() && !*otherWay)) {
      return TurnAction::Keep;
    }
    m_turn = Turn::Trying;
    m_direction = -m_direction;
    return TurnAction::Turn;
  }

private:
  enum class Flip { None, Pending, Trying };
  enum class Turn { None, Waiting, Pending, Trying };
  bool m_active = false;
  bool m_offByPreference = false;
  bool m_flips = false;
  int m_direction = 0;
  Flip m_flip = Flip::None;
  Turn m_turn = Turn::None;
};

} // namespace mitcad
