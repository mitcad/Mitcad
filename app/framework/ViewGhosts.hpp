// SPDX-License-Identifier: MIT
#pragma once

// The ghosts a command draws (mitcad#147; OcctViewer::setGhosts, the
// `withGhosts` hook of an input): which bodies an entry of the list covers,
// and the ghost of a joint's side A before its other side is picked
// (mitcad#2027). Plain Qt Core, header only.

#include <optional>

#include <QChar>
#include <QString>

namespace mitcad {

// A ghost entry names an occurrence (a path of uids from the root,
// `O1/O4`): its bodies and those of the occurrences inside it. An entry
// made by ownBodiesGhost() names the occurrence's own bodies only, matched
// exactly: not the occurrences inside it (mitcad#2027: the parts mounted on
// a frame stay pickable while the frame is a ghost).
inline QString ownBodiesGhost(const QString& occurrence) { return QString(QLatin1Char('=')) + occurrence; }

inline bool isOwnBodiesGhost(const QString& ghost) { return ghost.startsWith(QLatin1Char('=')); }

// Whether a ghost entry covers the bodies an occurrence places directly.
inline bool ghostCovers(const QString& ghost, const QString& occurrence) {
  if (occurrence.isEmpty() || ghost.isEmpty()) {
    return false;
  }
  if (isOwnBodiesGhost(ghost)) {
    return occurrence == ghost.mid(1);
  }
  return occurrence == ghost || occurrence.startsWith(ghost + QLatin1Char('/'));
}

// A ghost entry as the log names it: `O1/O4`, or `only O1/O4` for an
// occurrence's own bodies.
inline QString ghostLabel(const QString& ghost) {
  return isOwnBodiesGhost(ghost) ? QStringLiteral("only ") + ghost.mid(1) : ghost;
}

// The ghost of a joint's side A until B is picked (Joint, As-Built Joint,
// Constraint Set). `pick` is the pick's occurrence from the root,
// `inComponent` its path from the occurrence that places the joint's
// component (none: outside it; empty: the component's own geometry),
// `chosen` whether that component is chosen (a holder, mitcad#1856, or an
// edit's own) rather than the active one by default, and `placesOccurrences`
// whether the pick's occurrence holds occurrences of its own.
//
// A chosen component, and a pick on a part placed directly in it, ghost the
// occurrence placed directly in the component that the pick lies in, with
// everything inside it. Before a holder is known (the active component),
// a pick that lies deeper, or on the own bodies of a sub-assembly, ghosts
// only the bodies its own occurrence places: the second pick may lie
// inside the same sub-assembly, whose holder is asked about after it
// (mitcad#2027). Empty: no ghost.
inline QString jointSideGhost(const QString& pick, const std::optional<QString>& inComponent, bool chosen,
                              bool placesOccurrences) {
  if (!inComponent || inComponent->isEmpty() || !pick.endsWith(*inComponent)) {
    return QString();
  }
  // The pick's path from the root ends with its path from the component.
  const QString top = pick.left(pick.size() - inComponent->size()) + inComponent->section(QLatin1Char('/'), 0, 0);
  if (chosen) {
    return top;
  }
  const bool deeper = inComponent->contains(QLatin1Char('/'));
  return deeper || placesOccurrences ? ownBodiesGhost(pick) : top;
}

} // namespace mitcad
