// SPDX-License-Identifier: MIT
#pragma once

// A sketch edit the model refuses for conflicting constraints (mitcad#1566,
// core/model-api/src/commands.md "Conflicting constraints"): its message
// says in words what cannot hold, and its last line lists the ids of the
// constraints and dimensions that block the change ("[conflicts: k5, k7]",
// "[conflicts: none]"). The application shows the message without that
// line and paints those items in the view while the message shows; the
// log keeps the model's whole text. Plain Qt Core.

#include <QString>
#include <QStringList>

namespace mitcad::sketch {

struct ConflictText {
  // The message to show: the model's text without its ids line.
  QString message;
  // The blocking constraints and dimensions (k5); empty for none.
  QStringList items;
  // Whether the text was a refusal for conflicting constraints.
  bool conflict = false;
  // The blocking items in the model's words, as the message names them
  // before "can't hold with" ("the Horizontal on a line", "the 20.00 mm
  // distance"); empty when the message names none (mitcad#2090).
  QString words;
};

// The message and the blocking items of a model error; any other text is
// its own message, with no items and `conflict` false.
ConflictText splitConflict(const QString& error);

// Replacing the constraint or dimension that blocks a new one (mitcad#2090,
// GitHub mitcad/Mitcad#475): the refusal's Replace button, and Shift held
// at the step that adds it.

// Where a refused constraint or dimension came from.
enum class RefusedBy {
  // A constraint tool's last pick (its click).
  ToolPick,
  // A constraint applied to the selection: its command with a selection
  // that fits, or its key (Shift and a letter).
  Selection,
  // A constraint a snap or an inference added while drawing (mitcad#287).
  Inferred,
  // Sketch Dimension's value accepted (a conflicting one is added driven).
  DimensionValue,
};

// The one constraint or dimension a replace swaps out: the refusal's only
// blocking item; empty for no conflict, none (fixed or projected geometry)
// or several (they tie, or none suffices alone).
QString replaceable(const ConflictText& text);

// Whether the refusal's message carries Replace: a conflict naming exactly
// one item, not a constraint a snap or an inference added, and not one of
// several sets of a selection (`severalSets`, the "n of m added" warning).
bool offersReplace(const ConflictText& text, RefusedBy by, bool severalSets);

// Whether Shift held replaces at once: where Replace is offered, at a
// constraint tool's click or at the Enter that accepts a dimension's value;
// never for the selection (its keys hold Shift already).
bool shiftReplaces(const ConflictText& text, RefusedBy by, bool severalSets, bool shift);

} // namespace mitcad::sketch
