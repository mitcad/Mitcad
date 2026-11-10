// SPDX-License-Identifier: MIT
#include "SketchConflict.hpp"

namespace mitcad::sketch {

namespace {

// The start of the ids line (core/sketch/src/conflict.rs, TRAILER).
const QString kTrailer = QStringLiteral("\n[conflicts: ");

} // namespace

ConflictText splitConflict(const QString& error) {
  ConflictText text;
  text.message = error;
  const qsizetype at = error.lastIndexOf(kTrailer);
  if (at < 0 || !error.endsWith(QLatin1Char(']'))) {
    return text;
  }
  const qsizetype from = at + kTrailer.size();
  const QString listed = error.mid(from, error.size() - from - 1);
  text.message = error.left(at);
  text.conflict = true;
  if (listed != QStringLiteral("none")) {
    text.items = listed.split(QStringLiteral(", "), Qt::SkipEmptyParts);
  }
  return text;
}

} // namespace mitcad::sketch
