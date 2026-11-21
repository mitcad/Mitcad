// SPDX-License-Identifier: MIT
#include "SketchConflict.hpp"

#include "framework/UserText.hpp"

namespace mitcad::sketch {

namespace {

// The start of the ids line (core/sketch/src/conflict.rs, TRAILER).
const QString kTrailer = QStringLiteral("\n[conflicts: ");
// Between the blocking items' words and the change's (conflict.rs, blocked).
const QString kHoldsNot = QStringLiteral(" can't hold with ");

} // namespace

ConflictText splitConflict(const QString& error) {
  ConflictText text;
  // Only what a person reads (mitcad#1612): the text before a tail with
  // the places and the agents' detail.
  text.message = userText(error);
  const qsizetype at = error.lastIndexOf(kTrailer);
  if (at < 0 || !error.endsWith(QLatin1Char(']'))) {
    return text;
  }
  const qsizetype from = at + kTrailer.size();
  const QString listed = error.mid(from, error.size() - from - 1);
  text.message = userText(error.left(at));
  text.conflict = true;
  if (listed != QStringLiteral("none")) {
    text.items = listed.split(QStringLiteral(", "), Qt::SkipEmptyParts);
    // "<the blocking items> can't hold with <the change>"
    // (core/sketch/src/conflict.rs, blocked).
    const qsizetype words = text.message.indexOf(kHoldsNot);
    if (words > 0) {
      text.words = text.message.left(words);
    }
  }
  return text;
}

QString replaceable(const ConflictText& text) {
  return text.conflict && text.items.size() == 1 ? text.items.first() : QString();
}

bool offersReplace(const ConflictText& text, RefusedBy by, bool severalSets) {
  return !replaceable(text).isEmpty() && by != RefusedBy::Inferred && !severalSets;
}

bool shiftReplaces(const ConflictText& text, RefusedBy by, bool severalSets, bool shift) {
  return shift && (by == RefusedBy::ToolPick || by == RefusedBy::DimensionValue) &&
         offersReplace(text, by, severalSets);
}

} // namespace mitcad::sketch
