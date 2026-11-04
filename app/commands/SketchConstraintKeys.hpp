// SPDX-License-Identifier: MIT
#pragma once

#include <QKeySequence>
#include <QString>

namespace mitcad {

// The default keys of the sketch constraints (mitcad#1229): Shift and a
// letter, in sketches only, so that they meet neither the plain letters of
// the sketch tools (L, R, C, D, T, X, O, P, M) nor the Ctrl (Cmd) keys of
// files, editing and the system. Add Comment keeps Shift+N. A text or value
// field still types the capital letter: a line edit takes Shift and a
// letter before any shortcut. `type` is the constraint's name in its
// command's id (sketch.constraint.<type>); none for any other.
inline QKeySequence sketchConstraintShortcut(const QString& type) {
  struct Key {
    const char* type;
    Qt::Key key;
  };
  static const Key keys[] = {
      {"horizontal_vertical", Qt::Key_V}, {"coincident", Qt::Key_C}, {"tangent", Qt::Key_T},
      {"equal", Qt::Key_E},               {"parallel", Qt::Key_P},   {"perpendicular", Qt::Key_R},
      {"fix", Qt::Key_F},                 {"midpoint", Qt::Key_M},   {"concentric", Qt::Key_O},
      {"collinear", Qt::Key_L},           {"symmetric", Qt::Key_S},  {"smooth", Qt::Key_G},
  };
  for (const Key& entry : keys) {
    if (type == QLatin1String(entry.type)) {
      return QKeySequence(Qt::SHIFT | entry.key);
    }
  }
  return QKeySequence();
}

} // namespace mitcad
