// SPDX-License-Identifier: MIT
#include "NoticeLevel.hpp"

#include <initializer_list>

#include <QBuffer>
#include <QByteArray>
#include <QImage>
#include <QJsonObject>
#include <QJsonValue>
#include <QObject>
#include <QPainter>
#include <QPainterPath>
#include <QPen>
#include <QPixmap>
#include <QPointF>

#include "Theme.hpp"

namespace mitcad {
namespace {

// The badges' own colors, the same on the light and the dark theme: their
// halo keeps them apart from any fill (mitcad#961). The warning's triangle
// is yellow (mitcad#1041).
QColor badgeColor(ColorRole role) { return themeColor(role, false); }

constexpr double kHalo = 1.25; // the halo's width, at a 12 px badge

int severity(NoticeLevel level) {
  switch (level) {
  case NoticeLevel::Error:
    return 3;
  case NoticeLevel::Warning:
    return 2;
  case NoticeLevel::Information:
    return 1;
  default:
    return 0;
  }
}

} // namespace

NoticeLevel noticeLevelFromModel(const QString& level, NoticeLevel fallback) {
  if (level == QStringLiteral("error")) {
    return NoticeLevel::Error;
  }
  if (level == QStringLiteral("warning")) {
    return NoticeLevel::Warning;
  }
  if (level == QStringLiteral("info")) {
    return NoticeLevel::Information;
  }
  return fallback;
}

NoticeLevel worstNoticeLevel(const QJsonArray& notices, NoticeLevel fallback) {
  bool any = false;
  NoticeLevel worst = NoticeLevel::Information;
  for (const QJsonValue& value : notices) {
    const NoticeLevel level =
        noticeLevelFromModel(value.toObject().value(QStringLiteral("level")).toString(), NoticeLevel::Information);
    if (!any || severity(level) > severity(worst)) {
      worst = level;
    }
    any = true;
  }
  return any ? worst : fallback;
}

QString noticeLevelName(NoticeLevel level) {
  switch (level) {
  case NoticeLevel::Error:
    return QStringLiteral("error");
  case NoticeLevel::Warning:
    return QStringLiteral("warning");
  case NoticeLevel::Information:
    return QStringLiteral("info");
  case NoticeLevel::Success:
    return QStringLiteral("success");
  case NoticeLevel::Progress:
    return QStringLiteral("progress");
  case NoticeLevel::Hint:
    break;
  }
  return QStringLiteral("hint");
}

bool isProblem(NoticeLevel level) { return level == NoticeLevel::Error || level == NoticeLevel::Warning; }

QString noticeLevelWord(NoticeLevel level) {
  switch (level) {
  case NoticeLevel::Error:
    return QObject::tr("Failed");
  case NoticeLevel::Warning:
    return QObject::tr("Warning");
  default:
    return QString();
  }
}

QString noticeLevelText(NoticeLevel level, const QString& message) {
  const QString word = noticeLevelWord(level);
  if (word.isEmpty()) {
    return message;
  }
  if (message.isEmpty()) {
    return word;
  }
  if (level == NoticeLevel::Error) {
    return QObject::tr("Failed: %1").arg(message);
  }
  return QObject::tr("Warning: %1").arg(message);
}

QColor noticeColor(NoticeLevel level, const QPalette& palette) {
  switch (level) {
  case NoticeLevel::Error:
    return errorColor(palette);
  case NoticeLevel::Warning:
    return warningColor(palette);
  case NoticeLevel::Success:
    return themeColor(ColorRole::SuccessText, palette);
  default:
    return themeColor(ColorRole::InfoText, palette);
  }
}

QColor noticeFill(NoticeLevel level, const QPalette& palette) {
  // mitcad#852 comment 3: the error and warning fill roles.
  switch (level) {
  case NoticeLevel::Error:
    return themeColor(ColorRole::ErrorFill, palette);
  case NoticeLevel::Warning:
    return themeColor(ColorRole::WarningFill, palette);
  default:
    return QColor();
  }
}

int noticeTimeout(NoticeLevel level) { return isProblem(level) ? 10000 : 6000; }

void paintNoticeBadge(QPainter& painter, const QRectF& badge, NoticeLevel level, const QColor& halo) {
  if (level == NoticeLevel::Hint || level == NoticeLevel::Progress) {
    return;
  }
  const double side = badge.width();
  const double unit = side / 12.0; // the shapes are drawn for a 12 px badge
  const double ring = kHalo * unit;
  const double middle = badge.center().x();
  const bool halos = halo.isValid() && halo.alpha() > 0;
  painter.save();
  painter.setOpacity(1.0);
  painter.setRenderHint(QPainter::Antialiasing);
  if (level == NoticeLevel::Warning) {
    // A yellow triangle with a dark "!", a little wider than the circle.
    QPainterPath triangle;
    triangle.moveTo(middle, badge.top() - 0.5 * unit);
    triangle.lineTo(badge.right() + unit, badge.bottom());
    triangle.lineTo(badge.left() - unit, badge.bottom());
    triangle.closeSubpath();
    if (halos) {
      painter.setPen(QPen(halo, 2 * ring + unit, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
      painter.setBrush(halo);
      painter.drawPath(triangle);
    }
    painter.setPen(QPen(badgeColor(ColorRole::WarningBadge), unit, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
    painter.setBrush(badgeColor(ColorRole::WarningBadge));
    painter.drawPath(triangle);
    const double barWidth = 1.8 * unit;
    painter.setPen(QPen(badgeColor(ColorRole::WarningMark), barWidth, Qt::SolidLine, Qt::FlatCap));
    painter.drawLine(QPointF(middle, badge.top() + side * 0.32), QPointF(middle, badge.top() + side * 0.62));
    painter.setPen(Qt::NoPen);
    painter.setBrush(badgeColor(ColorRole::WarningMark));
    painter.drawEllipse(QPointF(middle, badge.top() + side * 0.80), barWidth * 0.55, barWidth * 0.55);
    painter.restore();
    return;
  }
  // The circles: the error's, the information's and the success's.
  const QColor fill = level == NoticeLevel::Error         ? badgeColor(ColorRole::ErrorBadge)
                      : level == NoticeLevel::Information ? badgeColor(ColorRole::InfoBadge)
                                                          : badgeColor(ColorRole::SuccessBadge);
  painter.setPen(Qt::NoPen);
  if (halos) {
    painter.setBrush(halo);
    painter.drawEllipse(badge.adjusted(-ring, -ring, ring, ring));
  }
  painter.setBrush(fill);
  painter.drawEllipse(badge);
  const double barWidth = 2.0 * unit;
  if (level == NoticeLevel::Error) {
    // A white "!".
    painter.setPen(QPen(badgeColor(ColorRole::BadgeMark), barWidth, Qt::SolidLine, Qt::FlatCap));
    painter.drawLine(QPointF(middle, badge.top() + side * 0.22), QPointF(middle, badge.top() + side * 0.58));
    painter.setPen(Qt::NoPen);
    painter.setBrush(badgeColor(ColorRole::BadgeMark));
    painter.drawEllipse(QPointF(middle, badge.top() + side * 0.80), barWidth * 0.55, barWidth * 0.55);
  } else if (level == NoticeLevel::Information) {
    // A white "i": the dot above the bar.
    painter.setPen(Qt::NoPen);
    painter.setBrush(badgeColor(ColorRole::BadgeMark));
    painter.drawEllipse(QPointF(middle, badge.top() + side * 0.24), barWidth * 0.55, barWidth * 0.55);
    painter.setPen(QPen(badgeColor(ColorRole::BadgeMark), barWidth, Qt::SolidLine, Qt::FlatCap));
    painter.drawLine(QPointF(middle, badge.top() + side * 0.42), QPointF(middle, badge.top() + side * 0.80));
  } else {
    // A white check mark.
    QPainterPath check;
    check.moveTo(badge.left() + side * 0.27, badge.top() + side * 0.52);
    check.lineTo(badge.left() + side * 0.44, badge.top() + side * 0.69);
    check.lineTo(badge.left() + side * 0.74, badge.top() + side * 0.34);
    painter.setPen(
        QPen(badgeColor(ColorRole::BadgeMark), 1.6 * unit, Qt::SolidLine, Qt::RoundCap, Qt::RoundJoin));
    painter.setBrush(Qt::NoBrush);
    painter.drawPath(check);
  }
  painter.restore();
}

QIcon noticeIcon(NoticeLevel level, const QColor& halo) {
  QIcon icon;
  if (level == NoticeLevel::Hint || level == NoticeLevel::Progress) {
    return icon;
  }
  for (const int size : {12, 16, 24, 32, 48}) {
    QPixmap pixmap(size, size);
    pixmap.fill(Qt::transparent);
    QPainter painter(&pixmap);
    // The badge's 12 units, the triangle's one more on each side and the
    // halo, in 17.
    painter.scale(size / 17.0, size / 17.0);
    paintNoticeBadge(painter, QRectF(3.0, 2.75, 12, 12), level, halo);
    painter.end();
    icon.addPixmap(pixmap);
  }
  return icon;
}

QString noticeIconHtml(NoticeLevel level, int size) {
  if (level == NoticeLevel::Hint || level == NoticeLevel::Progress) {
    return QString();
  }
  // Drawn at twice the size for high density screens.
  QImage image(2 * size, 2 * size, QImage::Format_ARGB32_Premultiplied);
  image.fill(Qt::transparent);
  QPainter painter(&image);
  painter.scale(2.0 * size / 17.0, 2.0 * size / 17.0);
  paintNoticeBadge(painter, QRectF(3.0, 2.75, 12, 12), level, QColor(Qt::transparent));
  painter.end();
  QByteArray png;
  QBuffer buffer(&png);
  if (!buffer.open(QIODevice::WriteOnly)) {
    return QString();
  }
  image.save(&buffer, "PNG");
  return QStringLiteral("<img src=\"data:image/png;base64,%1\" width=\"%2\" height=\"%2\" "
                        "style=\"vertical-align: middle\">")
      .arg(QString::fromLatin1(png.toBase64()))
      .arg(size);
}

QString noticeHtml(NoticeLevel level, const QString& message, const QPalette& palette) {
  const QString text = noticeLevelText(level, message).toHtmlEscaped();
  if (level == NoticeLevel::Hint || level == NoticeLevel::Progress) {
    return text;
  }
  return QStringLiteral("%1&nbsp;<span style=\"color:%2\">%3</span>")
      .arg(noticeIconHtml(level), noticeColor(level, palette).name(), text);
}

QString noticeSymbolName(NoticeLevel level) {
  switch (level) {
  case NoticeLevel::Error:
    return QStringLiteral("exclamationmark.circle.fill");
  case NoticeLevel::Warning:
    return QStringLiteral("exclamationmark.triangle.fill");
  case NoticeLevel::Information:
    return QStringLiteral("info.circle");
  case NoticeLevel::Success:
    return QStringLiteral("checkmark.circle.fill");
  default:
    return QString();
  }
}

} // namespace mitcad
