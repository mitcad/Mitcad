// SPDX-License-Identifier: MIT
#include "Cursors.hpp"

#include <algorithm>
#include <cmath>
#include <vector>

#include <QColor>
#include <QHash>
#include <QIcon>
#include <QImage>
#include <QPainter>
#include <QSettings>
#include <QtGlobal>

#include "Icons.hpp"

namespace mitcad {
namespace {

const QColor kArm(0x1b, 0x1e, 0x23);
const QColor kHalo(255, 255, 255, 217); // white at 85 %

// The parts in device pixels, at `unit` device pixels per logical pixel of
// the standard size. Lengths along an arm count from the centre line.
struct Metrics {
  int line = 1;   // width of the arms and of their halo
  int gap = 2;    // empty pixels between the centre line and an arm's halo
  int arm = 9;    // the far end of an arm
  int centre = 0; // the first pixel of the centre line, in x and in y
  int badgeOffset = 10;
  int badgeSize = 12;
  int size = 0;
};

Metrics metrics(qreal unit) {
  Metrics m;
  m.line = std::max(1, static_cast<int>(std::floor(unit)));
  m.gap = qRound(2.0 * unit);
  m.arm = qRound(9.0 * unit);
  m.centre = m.arm + m.line; // the left arm's halo starts at 0
  m.badgeOffset = qRound(10.0 * unit);
  m.badgeSize = qRound(12.0 * unit);
  m.size = std::max(m.centre + m.line + m.arm + m.line, m.centre + m.badgeOffset + m.badgeSize);
  return m;
}

qreal unitOf(qreal dpr, qreal scale) { return std::max(dpr, 0.5) * std::clamp(scale, 1.0, 2.0); }

// The dark parts of the four arms.
std::vector<QRect> armRects(const Metrics& m) {
  const int c = m.centre;
  const int length = m.arm - m.gap - m.line;
  const int inner = m.gap + m.line + 1; // the first dark pixel from the centre line
  return {
      QRect(c - m.arm, c, length, m.line),              // left
      QRect(c + m.line - 1 + inner, c, length, m.line), // right
      QRect(c, c - m.arm, m.line, length),              // up
      QRect(c, c + m.line - 1 + inner, m.line, length), // down
  };
}

// The gap: the centre line and `gap` pixels around it.
QRect gapRect(const Metrics& m) {
  return QRect(m.centre - m.gap, m.centre - m.gap, m.line + 2 * m.gap, m.line + 2 * m.gap);
}

// A frame `width` pixels wide inside `outer`.
void fillFrame(QPainter& painter, const QRect& outer, int width, const QColor& color) {
  painter.fillRect(QRect(outer.left(), outer.top(), outer.width(), width), color);
  painter.fillRect(QRect(outer.left(), outer.bottom() - width + 1, outer.width(), width), color);
  painter.fillRect(QRect(outer.left(), outer.top(), width, outer.height()), color);
  painter.fillRect(QRect(outer.right() - width + 1, outer.top(), width, outer.height()), color);
}

} // namespace

PrecisionCursorLayout precisionCursorLayout(qreal dpr, qreal scale) {
  const Metrics m = metrics(unitOf(dpr, scale));
  PrecisionCursorLayout layout;
  layout.gap = gapRect(m);
  const int reach = m.arm + m.line;
  layout.arms = QRect(m.centre - reach, m.centre - reach, 2 * reach + m.line, 2 * reach + m.line);
  layout.badge = QRect(m.centre + m.badgeOffset, m.centre + m.badgeOffset, m.badgeSize, m.badgeSize);
  const int hot = qRound(m.centre / std::max(dpr, 0.5));
  layout.hotSpot = QPoint(hot, hot);
  layout.size = m.size;
  return layout;
}

QPixmap precisionCursorPixmap(const QString& toolIcon, qreal dpr, bool snapping, qreal scale) {
  const qreal unit = unitOf(dpr, scale);
  const Metrics m = metrics(unit);
  QImage image(m.size, m.size, QImage::Format_ARGB32_Premultiplied);
  image.fill(Qt::transparent);
  QPainter painter(&image);
  // Whole device pixels: halos first, the dark lines over them.
  const std::vector<QRect> arms = armRects(m);
  for (const QRect& arm : arms) {
    painter.fillRect(arm.adjusted(-m.line, -m.line, m.line, m.line), kHalo);
  }
  const QRect square = gapRect(m);
  if (snapping) {
    // Hollow: the pick point inside stays visible.
    fillFrame(painter, square.adjusted(-m.line, -m.line, m.line, m.line), m.line, kHalo);
  }
  for (const QRect& arm : arms) {
    painter.fillRect(arm, kArm);
  }
  if (snapping) {
    fillFrame(painter, square, m.line, kArm);
  }
  // The tool's icon on a light plate, below and right of the cross.
  const QIcon icon = themeIcon(toolIcon);
  if (!icon.isNull()) {
    const QRect badge(m.centre + m.badgeOffset, m.centre + m.badgeOffset, m.badgeSize, m.badgeSize);
    painter.setRenderHint(QPainter::Antialiasing);
    painter.setPen(Qt::NoPen);
    painter.setBrush(kHalo);
    painter.drawRoundedRect(QRectF(badge), 2.0 * unit, 2.0 * unit);
    painter.drawPixmap(badge, icon.pixmap(badge.size(), 1.0));
  }
  painter.end();
  QPixmap pixmap = QPixmap::fromImage(image);
  pixmap.setDevicePixelRatio(std::max(dpr, 0.5));
  return pixmap;
}

QCursor precisionCursor(const QString& toolIcon, qreal dpr, bool snapping) {
  static const qreal scale = pointerScale();
  static QHash<QString, QCursor> cache;
  const QString key = QStringLiteral("%1|%2|%3").arg(toolIcon).arg(dpr).arg(snapping ? 1 : 0);
  const auto cached = cache.constFind(key);
  if (cached != cache.cend()) {
    return cached.value();
  }
  const PrecisionCursorLayout layout = precisionCursorLayout(dpr, scale);
  const QCursor cursor(precisionCursorPixmap(toolIcon, dpr, snapping, scale), layout.hotSpot.x(),
                       layout.hotSpot.y());
  cache.insert(key, cursor);
  return cursor;
}

qreal pointerScale() {
#ifdef Q_OS_WIN
  // Settings > Accessibility > Mouse pointer: 32 is the standard size.
  const QSettings cursors(QStringLiteral("HKEY_CURRENT_USER\\Control Panel\\Cursors"), QSettings::NativeFormat);
  const qreal size = cursors.value(QStringLiteral("CursorBaseSize"), 32).toDouble();
  const qreal standard = 32.0;
#else
  // X cursor themes: 24 is the usual size (0 when not set).
  const qreal size = qEnvironmentVariableIntValue("XCURSOR_SIZE");
  const qreal standard = 24.0;
#endif
  return size > 0.0 ? std::clamp(size / standard, 1.0, 2.0) : 1.0;
}

} // namespace mitcad
