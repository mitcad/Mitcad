// SPDX-License-Identifier: MIT
#include "FeatureStatus.hpp"

#include <QObject>
#include <QPainter>
#include <QPen>
#include <QStringList>

#include "framework/Theme.hpp"

namespace mitcad {
namespace {

// The badges' own colours, the same on the light and the dark theme (the
// role table's, mitcad#1041): their halo keeps them apart from any fill.
QColor badge(ColorRole role) { return themeColor(role, false); }

constexpr double kBadge = 12.0;     // the badge's side
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

NoticeLevel featureStateLevel(FeatureState state) {
  switch (state) {
  case FeatureState::Failed:
    return NoticeLevel::Error;
  case FeatureState::Warning:
    return NoticeLevel::Warning;
  case FeatureState::Ok:
    break;
  }
  return NoticeLevel::Hint;
}

namespace timeline {

CellLook cellLook(const CellState& state, const CellColors& colors) {
  CellLook look;
  if (state.state == FeatureState::Failed) {
    look.fill = colors.errorFill;
    look.statusRing = colors.errorBorder;
    look.badge = true;
    look.badgeLevel = NoticeLevel::Error;
  } else if (state.state == FeatureState::Warning) {
    look.fill = colors.warningFill;
    look.statusRing = colors.warningBorder;
    look.badge = true;
    look.badgeLevel = NoticeLevel::Warning;
  } else if (state.noted) {
    // An import note (a placeholder of an item the import skipped, a
    // fallback, an item that came in partly) is information
    // (mitcad#1040): its badge, no status ring in a warning's colour.
    look.badge = true;
    look.badgeLevel = NoticeLevel::Information;
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
    } else if (state.marked) {
      // The owner's mark (mitcad#1704): the selection's colour, dashed.
      look.outerRing = colors.selectedBorder;
      look.outerDashed = true;
    }
    if (state.hovered && ok) {
      look.fill = colors.hoverFill;
    } else if (state.hovered && !state.found && !state.marked) {
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
    QPen pen(look.outerRing, kOuterRing);
    if (look.outerDashed) {
      pen.setDashPattern({2.0, 1.5}); // in pen widths: 4 px dashes, 3 px gaps
    }
    painter.setPen(pen);
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
  paintCellBadge(painter, bounds, featureStateLevel(state), halo);
}

void paintCellBadge(QPainter& painter, const QRectF& bounds, NoticeLevel level, const QColor& halo) {
  // The levels' one set of badges (NoticeLevel, mitcad#1040).
  paintNoticeBadge(painter, statusBadgeRect(bounds), level, halo);
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
    // An import note is information (mitcad#1040).
    words << (state.noted ? QStringLiteral("info") : QStringLiteral("ok"));
    break;
  }
  if (state.selected) {
    words << QStringLiteral("selected");
  }
  if (state.found) {
    words << QStringLiteral("found");
  }
  if (state.marked) {
    words << QStringLiteral("marked");
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
    words << (look.outerDashed ? QStringLiteral("dashed outer ring") : QStringLiteral("outer ring"));
  }
  if (look.badge) {
    words << QStringLiteral("badge");
  }
  return words.join(QStringLiteral(", "));
}

} // namespace timeline

QColor issueBadgeColor(FeatureState state) {
  return state == FeatureState::Failed ? badge(ColorRole::ErrorBadge) : badge(ColorRole::WarningBadge);
}

QColor issueBadgeTextColor(FeatureState state) {
  return state == FeatureState::Failed ? badge(ColorRole::BadgeMark) : badge(ColorRole::WarningMark);
}

QString issueBadgeStyle(FeatureState state) {
  // Pressed or hovered a shade darker, the text as readable.
  const QColor fill = issueBadgeColor(state);
  return QStringLiteral("QToolButton { color: %1; background: %2; border: none; border-radius: 9px; "
                        "padding: 1px 7px; } QToolButton:hover, QToolButton:pressed { background: %3; }")
      .arg(issueBadgeTextColor(state).name(), fill.name(), fill.darker(115).name());
}

QIcon featureStateIcon(FeatureState state) {
  if (state == FeatureState::Ok) {
    return QIcon();
  }
  // The halo in the text colour of the badge it sits on.
  return noticeIcon(featureStateLevel(state), issueBadgeTextColor(state));
}

QString featureStatusLine(const QString& status, const QString& message, const QColor& errorColor,
                          const QColor& warningColor) {
  const FeatureState state = featureState(status);
  if (state == FeatureState::Ok) {
    return QString();
  }
  // The level's icon and word as well as its colour (mitcad#1040).
  const NoticeLevel level = featureStateLevel(state);
  return QStringLiteral("%1&nbsp;<span style=\"color:%2\">%3</span>")
      .arg(noticeIconHtml(level),
           (state == FeatureState::Failed ? errorColor : warningColor).name(),
           noticeLevelText(level, message).toHtmlEscaped());
}

QString autoFixHint(int fixable) {
  if (fixable <= 0) {
    return QString();
  }
  // Its context menu has Auto Fix (mitcad#1223); a warning after a sketch
  // edit otherwise gave no hint of it (mitcad#1677).
  return fixable == 1 ? QObject::tr("Auto Fix (right-click) can fix this warning")
                      : QObject::tr("Auto Fix (right-click) can fix %1 of its warnings").arg(fixable);
}

QString autoFixBadgeHint(int fixable) {
  if (fixable <= 0) {
    return QString();
  }
  // A click on the badge offers Auto Fix of them (mitcad#1677).
  return fixable == 1 ? QObject::tr("Click for Auto Fix, which can fix 1 of these warnings")
                      : QObject::tr("Click for Auto Fix, which can fix %1 of these warnings").arg(fixable);
}

} // namespace mitcad
