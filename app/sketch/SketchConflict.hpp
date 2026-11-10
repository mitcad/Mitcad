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
};

// The message and the blocking items of a model error; any other text is
// its own message, with no items and `conflict` false.
ConflictText splitConflict(const QString& error);

} // namespace mitcad::sketch
