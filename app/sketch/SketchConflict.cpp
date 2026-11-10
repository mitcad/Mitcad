// SPDX-License-Identifier: MIT
#include "SketchConflict.hpp"

#include "framework/UserText.hpp"

namespace mitcad::sketch {

namespace {

// The start of the ids line (core/sketch/src/conflict.rs, TRAILER).
const QString kTrailer = QStringLiteral("\n[conflicts: ");

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
  }
  return text;
}

} // namespace mitcad::sketch
