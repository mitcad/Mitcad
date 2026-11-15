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
// the viewer's section caps (mitcad#382), Component Colors and the Browser's
// bars (mitcad#1876): the role table's component palette (mitcad#1041), so
// that they agree.
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

// The root's colour where every component is drawn in its own (mitcad#1876:
// the Browser's bars): the default orange the section caps and Component
// Colors give the root's bodies.
inline QColor rootComponentColor() { return viewColor(ColorRole::SectionCap, false); }

// The colour of a component's rows in the Browser (mitcad#1876): its
// timeline colour (componentColorIndex), the root's orange for the root;
// invalid for a component the snapshot does not have.
inline QColor componentRowColor(const QVector<DocumentSnapshot::Component>& components, const QString& uid) {
  if (uid == QLatin1String("C0") || (!components.isEmpty() && components.first().uid == uid)) {
    return rootComponentColor();
  }
  const int index = componentColorIndex(components, uid);
  return index >= 0 ? componentColor(index) : QColor();
}

} // namespace mitcad
