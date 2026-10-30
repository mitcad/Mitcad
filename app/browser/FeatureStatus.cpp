// SPDX-License-Identifier: MIT
#include "FeatureStatus.hpp"

#include <initializer_list>

#include <QObject>
#include <QPainter>
#include <QPainterPath>
#include <QPen>
#include <QPixmap>
#include <QPointF>
#include <QStringList>

namespace mitcad {
namespace {

// The badges' own colours, the same on the light and the dark theme: their
// halo keeps them apart from any fill.
const QColor kBadgeFailed(0xd3, 0x2f, 0x2f);
const QColor kBadgeFailedMark(0xff, 0xff, 0xff);
const QColor kBadgeWarning(0xf2, 0xb2, 0x00);
const QColor kBadgeWarningMark(0x2a, 0x20, 0x00);

constexpr double kBadge = 12.0;     // the badge's side
constexpr double kHalo = 1.25;      // its halo's width
constexpr double kOuterRing = 2.0;  // the selection's or a match's ring
constexpr double kStatusRing = 2.0; // the red or yellow one, as wide (mitcad#961)

bool shown(const QColor& color) { return color.isValid() && color.alpha() > 0; }

} // namespace

FeatureState featureState(const QString& status) {
  if (status == QStringLiteral("error")) {
    return FeatureState::Failed;
  }
  return status == QStringLiteral("warning") ? FeatureState::Warning : FeatureState::Ok;
}

namespace timeline {

CellLook cellLook(const CellState& state, const CellColors& colors) {
  CellLook look;
  if (state.state == FeatureState::Failed) {
    look.fill = colors.errorFill;
    look.statusRing = colors.errorBorder;
    look.badge = true;
  } else if (state.state == FeatureState::Warning) {
    look.fill = colors.warningFill;
    look.statusRing = colors.warningBorder;
    look.badge = true;
  } else if (state.noted) {
    // A placeholder of an item the import skipped: suppressed, with a
    // yellow outline.
    look.statusRing = colors.warningBorder;
  }
  const bool ok = state.state == FeatureState::Ok;
  if (state.selected) {
    look.outerRing = colors.selectedBorder;
    if (ok) {
      look.fill = colors.selectedFill;
    }
  } else {
    if (state.found) {
      look.outerRing = colors.found;
    }
    if (state.hovered && ok) {
      look.fill = colors.hoverFill;
    } else if (state.hovered && !state.found) {
      // A failed or warned cell keeps its fill: a faint ring outside its
      // status ring shows the pointer on it.
      look.outerRing = colors.hoverRing;
    }
  }
  return look;
}

void paintCellFrame(QPainter& painter, const QRectF& bounds, const CellLook& look) {
  painter.save();
  if (shown(look.fill)) {
    painter.setPen(Qt::NoPen);
    painter.setBrush(look.fill);
    painter.drawRoundedRect(bounds.adjusted(0.5, 0.5, -0.5, -0.5), 4, 4);
  }
  painter.setBrush(Qt::NoBrush);
  const bool outer = shown(look.outerRing);
  if (outer) {
    const double inset = kOuterRing / 2;
    painter.setPen(QPen(look.outerRing, kOuterRing));
    painter.drawRoundedRect(bounds.adjusted(inset, inset, -inset, -inset), 4, 4);
  }
  if (shown(look.statusRing)) {
    // Inside the outer ring, so that both show (mitcad#961).
    const double inset = (outer ? kOuterRing : 0.0) + kStatusRing / 2;
    painter.setPen(QPen(look.statusRing, kStatusRing));
    painter.drawRoundedRect(bounds.adjusted(inset, inset, -inset, -inset), outer ? 2.5 : 4, outer ? 2.5 : 4);
  }
  painter.restore();
}

QRectF statusBadgeRect(const QRectF& bounds) {
  return QRectF(bounds.right() - kBadge - 0.5, bounds.top() + 0.5, kBadge, kBadge);
}

void paintStatusBadge(QPainter& painter, const QRectF& bounds, FeatureState state, const QColor& halo) {
  if (state == FeatureState::Ok) {
    return;
  }
  const QRectF badge = statusBadgeRect(bounds);
  const double middle = badge.center().x();
  painter.save();
  painter.setOpacity(1.0);
  painter.setRenderHint(QPainter::Antialiasing);
  QColor mark;
  double barTop = 0;
  double barBottom = 0;
  double dot = 0;
  double barWidth = 2.0;
  if (state == FeatureState::Failed) {
    // A red circle with a white "!".
    painter.setPen(Qt::NoPen);
    painter.setBrush(halo);
    painter.drawEllipse(badge.adjusted(-kHalo, -kHalo, kHalo, kHalo));
    painter.setBrush(kBadgeFailed);
    painter.drawEllipse(badge);
    mark = kBadgeFailedMark;
    barTop = badge.top() + kBadge * 0.22;
    barBottom = badge.top() + kBadge * 0.58;
    dot = badge.top() + kBadge * 0.80;
  } else {
    // An amber triangle with a dark "!", a little wider than the circle.
    QPainterPath triangle;
    triangle.moveTo(middle, badge.top() - 0.5);
    triangle.lineTo(badge.right() + 1.0, badge.bottom());
    triangle.lineTo(badge.left() - 1.0, badge.bottom());
    triangle.closeSubpath();
    painter.setPen(QPen(halo, 2 * kHalo + 1.0, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
    painter.setBrush(halo);
    painter.drawPath(triangle);
    painter.setPen(QPen(kBadgeWarning, 1.0, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
    painter.setBrush(kBadgeWarning);
    painter.drawPath(triangle);
    mark = kBadgeWarningMark;
    barTop = badge.top() + kBadge * 0.32;
    barBottom = badge.top() + kBadge * 0.62;
    dot = badge.top() + kBadge * 0.80;
    barWidth = 1.8;
  }
  painter.setPen(QPen(mark, barWidth, Qt::SolidLine, Qt::FlatCap));
  painter.drawLine(QPointF(middle, barTop), QPointF(middle, barBottom));
  painter.setPen(Qt::NoPen);
  painter.setBrush(mark);
  painter.drawEllipse(QPointF(middle, dot), barWidth * 0.55, barWidth * 0.55);
  painter.restore();
}

QString describeCell(const CellState& state, const CellLook& look) {
  QStringList words;
  switch (state.state) {
  case FeatureState::Failed:
    words << QStringLiteral("failed");
    break;
  case FeatureState::Warning:
    words << QStringLiteral("warning");
    break;
  case FeatureState::Ok:
    words << (state.noted ? QStringLiteral("import note") : QStringLiteral("ok"));
    break;
  }
  if (state.selected) {
    words << QStringLiteral("selected");
  }
  if (state.found) {
    words << QStringLiteral("found");
  }
  if (state.hovered) {
    words << QStringLiteral("hovered");
  }
  if (state.rolledBack) {
    words << QStringLiteral("rolled back");
  }
  if (shown(look.statusRing)) {
    words << QStringLiteral("status ring");
  }
  if (shown(look.outerRing)) {
    words << QStringLiteral("outer ring");
  }
  if (look.badge) {
    words << QStringLiteral("badge");
  }
  return words.join(QStringLiteral(", "));
}

} // namespace timeline

QColor issueBadgeColor(FeatureState state) {
  return state == FeatureState::Failed ? kBadgeFailed : kBadgeWarning;
}

QColor issueBadgeTextColor(FeatureState state) {
  return state == FeatureState::Failed ? kBadgeFailedMark : kBadgeWarningMark;
}

QString issueBadgeStyle(FeatureState state) {
  // Pressed or hovered a shade darker, the text as readable.
  const QColor fill = issueBadgeColor(state);
  return QStringLiteral("QToolButton { color: %1; background: %2; border: none; border-radius: 9px; "
                        "padding: 1px 7px; } QToolButton:hover, QToolButton:pressed { background: %3; }")
      .arg(issueBadgeTextColor(state).name(), fill.name(), fill.darker(115).name());
}

QIcon featureStateIcon(FeatureState state) {
  QIcon icon;
  if (state == FeatureState::Ok) {
    return icon;
  }
  for (const int size : {12, 16, 24, 32, 48}) {
    QPixmap pixmap(size, size);
    pixmap.fill(Qt::transparent);
    QPainter painter(&pixmap);
    painter.setRenderHint(QPainter::Antialiasing);
    // The badge's 12 units, the triangle's one more on each side and the
    // halo, in 17; the halo in the text colour of the badge it sits on.
    painter.scale(size / 17.0, size / 17.0);
    timeline::paintStatusBadge(painter, QRectF(1.5, 2.25, 14, 14), state, issueBadgeTextColor(state));
    painter.end();
    icon.addPixmap(pixmap);
  }
  return icon;
}

QString featureStatusLine(const QString& status, const QString& message, const QColor& errorColor,
                          const QColor& warningColor) {
  const FeatureState state = featureState(status);
  if (state == FeatureState::Ok) {
    return QString();
  }
  const bool failed = state == FeatureState::Failed;
  QString text;
  if (failed) {
    text = message.isEmpty() ? QObject::tr("Failed") : QObject::tr("Failed: %1").arg(message);
  } else {
    text = message.isEmpty() ? QObject::tr("Warning") : QObject::tr("Warning: %1").arg(message);
  }
  return QStringLiteral("<span style=\"color:%1\">%2</span>")
      .arg((failed ? errorColor : warningColor).name(), text.toHtmlEscaped());
}

} // namespace mitcad
