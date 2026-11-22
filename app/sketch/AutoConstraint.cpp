// SPDX-License-Identifier: MIT
#include "AutoConstraint.hpp"

#include <cmath>

#include <QObject>

namespace mitcad::sketch {
namespace {

const QString kEqual = QStringLiteral("equal");
const QString kAlign = QStringLiteral("horizontal_vertical");
const QString kConcentric = QStringLiteral("concentric");
const QString kParallel = QStringLiteral("parallel");
const QString kPerpendicular = QStringLiteral("perpendicular");

QJsonObject pair(const QString& type, const QString& a, const QString& b) {
  return {{QStringLiteral("type"), type}, {QStringLiteral("a"), a}, {QStringLiteral("b"), b}};
}

} // namespace

QString otherDirection(const QString& type) {
  if (type == QStringLiteral("horizontal")) {
    return QStringLiteral("vertical");
  }
  if (type == QStringLiteral("vertical")) {
    return QStringLiteral("horizontal");
  }
  if (type == QStringLiteral("horizontal_points")) {
    return QStringLiteral("vertical_points");
  }
  if (type == QStringLiteral("vertical_points")) {
    return QStringLiteral("horizontal_points");
  }
  return {};
}

QString horizontalVerticalDirection(const QString& nearer, const std::function<bool(const QString&)>& takes) {
  const QString other = otherDirection(nearer);
  if (other.isEmpty() || !takes || takes(nearer)) {
    return nearer;
  }
  return takes(other) ? other : nearer;
}

QStringList autoConstraintTypes() { return {kEqual, kAlign, kConcentric, kParallel, kPerpendicular}; }

QStringList autoConstraintTypes(AutoKind kind) {
  switch (kind) {
  case AutoKind::Line:
    return {kEqual, kParallel, kPerpendicular};
  case AutoKind::Circular:
    return {kEqual, kAlign, kConcentric};
  case AutoKind::None:
    break;
  }
  return {};
}

QString autoConstraintLabel(const QString& type) {
  if (type == kEqual) {
    return QObject::tr("Equal");
  }
  if (type == kAlign) {
    return QObject::tr("Horizontal/Vertical alignment");
  }
  if (type == kConcentric) {
    return QObject::tr("Concentric");
  }
  if (type == kParallel) {
    return QObject::tr("Parallel");
  }
  if (type == kPerpendicular) {
    return QObject::tr("Perpendicular");
  }
  return type;
}

QString autoConstraintExclusive(const QString& type) {
  if (type == kParallel) {
    return kPerpendicular;
  }
  if (type == kPerpendicular) {
    return kParallel;
  }
  if (type == kConcentric) {
    return kAlign;
  }
  if (type == kAlign) {
    return kConcentric;
  }
  return {};
}

QJsonObject autoConstraintDefinition(const QString& type, const AutoEntity& reference, const AutoEntity& made) {
  if (reference.id.isEmpty() || made.id.isEmpty() || reference.id == made.id || reference.kind != made.kind) {
    return {};
  }
  const bool lines = made.kind == AutoKind::Line;
  const bool circular = made.kind == AutoKind::Circular;
  if (type == kEqual && (lines || circular)) {
    return pair(kEqual, reference.id, made.id);
  }
  if ((type == kParallel || type == kPerpendicular) && lines) {
    return pair(type, reference.id, made.id);
  }
  if (type == kConcentric && circular) {
    return pair(kConcentric, reference.id, made.id);
  }
  if (type == kAlign && circular) {
    if (reference.centerPoint.isEmpty() || made.centerPoint.isEmpty() ||
        reference.centerPoint == made.centerPoint) {
      return {};
    }
    // The centre points, as Horizontal/Vertical takes two points.
    return pair(alignsHorizontally(reference.center, made.center) ? QStringLiteral("horizontal_points")
                                                                  : QStringLiteral("vertical_points"),
                reference.centerPoint, made.centerPoint);
  }
  return {};
}

bool autoConstraintShared(const QString& type, const AutoEntity& reference, const AutoEntity& made) {
  return (type == kAlign || type == kConcentric) && reference.kind == AutoKind::Circular &&
         made.kind == AutoKind::Circular && !reference.centerPoint.isEmpty() &&
         reference.centerPoint == made.centerPoint;
}

V2 autoLineEnd(const V2& start, const V2& end, const AutoEntity& reference, const QStringList& types,
               bool keepLength) {
  if (reference.kind != AutoKind::Line) {
    return end;
  }
  const bool parallel = types.contains(kParallel);
  const bool square = !parallel && types.contains(kPerpendicular);
  const bool equal = types.contains(kEqual) && !keepLength;
  if (!parallel && !square && !equal) {
    return end;
  }
  const V2 along = unit(reference.b - reference.a);
  const V2 drawn = end - start;
  V2 direction = unit(drawn); // (1, 0) when the end is on the start
  double length = norm(drawn);
  if (parallel || square) {
    const V2 d = parallel ? along : perp(along);
    const double onto = dot(drawn, d);
    direction = onto >= 0.0 ? d : -d;
    if (!keepLength) {
      length = std::abs(onto);
    }
  }
  if (equal) {
    length = norm(reference.b - reference.a);
  }
  return start + direction * length;
}

AutoCircle autoCircle(const AutoCircle& drawn, const AutoEntity& reference, const QStringList& types,
                      bool keepRadius, const std::optional<V2>& rim) {
  if (reference.kind != AutoKind::Circular) {
    return drawn;
  }
  AutoCircle circle = drawn;
  if (types.contains(kConcentric)) {
    circle.center = reference.center;
  } else if (types.contains(kAlign)) {
    if (alignsHorizontally(reference.center, drawn.center)) {
      circle.center.y = reference.center.y;
    } else {
      circle.center.x = reference.center.x;
    }
  }
  if (!keepRadius) {
    if (types.contains(kEqual)) {
      circle.radius = reference.radius;
    } else if (rim && circle.center != drawn.center) {
      circle.radius = dist(circle.center, *rim);
    }
  }
  return circle;
}

void AutoConstraintSession::reset(AutoKind kind) {
  m_kind = kind;
  m_link = Link::Last;
  m_checked.clear();
  m_entities.clear();
}

bool AutoConstraintSession::setChecked(const QString& type, bool on) {
  if (!types().contains(type) || checked(type) == on) {
    return false;
  }
  if (on) {
    m_checked.insert(type);
    m_checked.remove(autoConstraintExclusive(type));
  } else {
    m_checked.remove(type);
  }
  return true;
}

QStringList AutoConstraintSession::checkedTypes() const {
  QStringList checkedOnes;
  for (const QString& type : types()) {
    if (checked(type)) {
      checkedOnes << type;
    }
  }
  return checkedOnes;
}

void AutoConstraintSession::record(const QString& id, const Exists& exists) {
  if (id.isEmpty()) {
    return;
  }
  // An id taken back by undo may come again for the new entity: it counts
  // where it is drawn now.
  QStringList kept;
  for (const QString& entity : m_entities) {
    if (entity != id && exists(entity)) {
      kept << entity;
    }
  }
  kept << id;
  m_entities = kept;
}

QString AutoConstraintSession::reference(const Exists& exists) const {
  if (m_link == Link::First) {
    for (const QString& entity : m_entities) {
      if (exists(entity)) {
        return entity;
      }
    }
    return {};
  }
  for (auto entity = m_entities.crbegin(); entity != m_entities.crend(); ++entity) {
    if (exists(*entity)) {
      return *entity;
    }
  }
  return {};
}

} // namespace mitcad::sketch
