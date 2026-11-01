// SPDX-License-Identifier: MIT
#pragma once

#include <QColor>
#include <QString>
#include <QVector>

#include "DocumentSnapshot.hpp"
#include "../framework/Theme.hpp"

namespace mitcad {

// The colours of the components as the timeline shows them (the band under
// each of a component's features); the root has none. Shared by the timeline,
// the viewer's section caps (mitcad#382) and Component Colors: the role
// table's component palette (mitcad#1041), so that they agree.
inline QColor componentColor(int index) { return componentPaletteColor(index); }

// A component's index into the palette: its position in the snapshot's
// components minus one (the root is first); -1 for the root or an unknown
// component.
inline int componentColorIndex(const QVector<DocumentSnapshot::Component>& components, const QString& uid) {
  for (int i = 1; i < components.size(); ++i) {
    if (components[i].uid == uid) {
      return i - 1;
    }
  }
  return -1;
}

} // namespace mitcad
