// SPDX-License-Identifier: MIT
#pragma once

// Auto constraints while drawing (mitcad#189): the sketch palette's Auto
// constraint section lets a drawing tool constrain each new entity to the
// first or the last entity drawn in the same tool session. This part is
// plain Qt Core and plane geometry (no model, no widgets): which
// constraints a tool offers, their definitions, the geometry adjusted so
// that a new entity satisfies them as it is made, and the session's
// choices and entities.

#include <functional>
#include <optional>

#include <QJsonObject>
#include <QSet>
#include <QString>
#include <QStringList>

#include "SketchGeometry.hpp"

namespace mitcad::sketch {

// What a drawing tool makes, as far as auto constraints go.
enum class AutoKind {
  None,     // the tool takes no auto constraints
  Line,     // lines
  Circular, // circles and arcs
};

// What the auto constraints need to know of an entity.
struct AutoEntity {
  QString id; // the curve (c4)
  AutoKind kind = AutoKind::None;
  V2 a, b;             // a line's ends
  V2 center;           // a circle's or arc's centre
  double radius = 0.0; // a circle's or arc's radius
  QString centerPoint; // its centre point (p3)
};

// A circle (or an arc's circle) being drawn.
struct AutoCircle {
  V2 center;
  double radius = 0.0;
};

// Every auto constraint type, in panel order: equal, horizontal_vertical
// (centres aligned), concentric, parallel, perpendicular.
QStringList autoConstraintTypes();
// The types a tool's entities take, in panel order.
QStringList autoConstraintTypes(AutoKind kind);
// The panel's short label of a type ("Equal").
QString autoConstraintLabel(const QString& type);
// The type that cannot be checked together with `type` (perpendicular for
// parallel, concentric for the centre alignment), or empty.
QString autoConstraintExclusive(const QString& type);

// The rule of Horizontal/Vertical on two points: horizontal when they are
// at least as far apart along x as along y.
inline bool alignsHorizontally(const V2& a, const V2& b) {
  const V2 d = b - a;
  return std::abs(d.x) >= std::abs(d.y);
}

// Horizontal/Vertical's other direction (mitcad#2184): `vertical` for
// `horizontal` and the other way round, likewise `vertical_points` and
// `horizontal_points`; empty for any other type.
QString otherDirection(const QString& type);
// The direction Horizontal/Vertical adds (mitcad#2184): `nearer` (the
// type of a line's or two points' nearer direction, as the tool chooses it
// by their slope) when the sketch takes it; else the other direction when
// the sketch takes that, because the nearer one is already implied or
// conflicts while the other is neither; else `nearer`, whose refusal then
// says why. `takes` says whether the sketch would add a constraint of that
// type on the same picks, asked without changing anything
// (`sketch.add_constraint` with `check`); it is not asked about the other
// direction when the nearer one is taken.
QString horizontalVerticalDirection(const QString& nearer, const std::function<bool(const QString&)>& takes);

// The constraint of `type` between the reference and a new entity, as
// `sketch.add_constraint` takes it; empty when the two do not take it.
QJsonObject autoConstraintDefinition(const QString& type, const AutoEntity& reference, const AutoEntity& made);
// Whether the two already satisfy `type` by sharing a point (a centre
// alignment or concentric circles on one centre point), so that the
// constraint would only be redundant.
bool autoConstraintShared(const QString& type, const AutoEntity& reference, const AutoEntity& made);

// The end of a line from `start` towards `end`, moved so that it satisfies
// the checked types to the reference line: along it or square to it
// (parallel, perpendicular) and as long (equal). `keepLength`: a typed
// length that equal does not change. The end itself when nothing applies.
V2 autoLineEnd(const V2& start, const V2& end, const AutoEntity& reference, const QStringList& types,
               bool keepLength);
// A circle moved and sized so that it satisfies the checked types to the
// reference circle or arc: its centre on the reference's (concentric) or
// level or plumb with it (the alignment, by alignsHorizontally), its radius
// the reference's (equal) unless `keepRadius` (a typed size). A circle
// drawn from its centre through `rim` keeps going through it when its
// centre moves and its radius is not otherwise given.
AutoCircle autoCircle(const AutoCircle& drawn, const AutoEntity& reference, const QStringList& types,
                      bool keepRadius, const std::optional<V2>& rim = std::nullopt);

// The choices and entities of one drawing tool's session: the reference
// (Link to first or Link to last, last by default), the checked types,
// and the entities drawn so far, oldest first. A new tool starts a new
// session.
class AutoConstraintSession {
public:
  enum class Link { First, Last };
  using Exists = std::function<bool(const QString&)>;

  // A new session of a tool making `kind`: Link to last, nothing checked,
  // nothing drawn.
  void reset(AutoKind kind = AutoKind::None);
  AutoKind kind() const { return m_kind; }
  // The types the tool offers.
  QStringList types() const { return autoConstraintTypes(m_kind); }

  Link link() const { return m_link; }
  void setLink(Link link) { m_link = link; }
  bool checked(const QString& type) const { return m_checked.contains(type); }
  // Checks or unchecks a type the tool offers; checking one unchecks the
  // type it excludes. False when nothing changed.
  bool setChecked(const QString& type, bool on);
  // The checked types, in panel order.
  QStringList checkedTypes() const;

  // A new entity was drawn: it becomes the last (and the first, when no
  // other drawn entity still exists). Entities that no longer exist (taken
  // back by undo) are forgotten.
  void record(const QString& id, const Exists& exists);
  // The reference among the drawn entities that still exist, or empty:
  // nothing is drawn yet, or undo took everything back.
  QString reference(const Exists& exists) const;
  const QStringList& entities() const { return m_entities; }

private:
  AutoKind m_kind = AutoKind::None;
  Link m_link = Link::Last;
  QSet<QString> m_checked;
  QStringList m_entities;
};

// What the next entity gets: its reference and the checked types.
struct AutoPlan {
  AutoEntity reference;
  QStringList types;

  bool has(const QString& type) const { return types.contains(type); }
};

} // namespace mitcad::sketch
