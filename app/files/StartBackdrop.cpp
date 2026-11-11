// SPDX-License-Identifier: MIT
#include "StartBackdrop.hpp"

#include <algorithm>

#include <QBrush>
#include <QLinearGradient>
#include <QPaintDevice>
#include <QPainter>
#include <QPen>
#include <QRect>

namespace mitcad::start {

QMargins backdropMargins(int inset, int top, int below, int gap) {
  return QMargins(inset, std::max(0, top) + inset, inset, below > 0 ? std::max(inset, below + gap) : inset);
}

QRectF backdropRect(const QSize& size, int inset, int top, int below, int gap) {
  const QMargins margins = backdropMargins(inset, top, below, gap);
  const QRectF rect(QPointF(margins.left(), margins.top()),
                    QPointF(size.width() - margins.right(), size.height() - margins.bottom()));
  return rect.isValid() ? rect : QRectF();
}

void paintBackdrop(QPainter& painter, const QSize& size, const QRectF& rect, const Backdrop& backdrop) {
  // The view's background: its gradient runs over the view's height, which
  // is the page's.
  QLinearGradient gradient(QPointF(0, 0), QPointF(0, std::max(1, size.height())));
  gradient.setColorAt(0, backdrop.viewTop);
  gradient.setColorAt(1, backdrop.viewBottom);
  painter.fillRect(QRect(QPoint(0, 0), size), gradient);
  if (rect.isEmpty()) {
    return;
  }
  // As a floating card's material (GlassCard::paintMaterial): a hairline
  // border inside the rectangle, the fill within it.
  painter.save();
  painter.setRenderHint(QPainter::Antialiasing);
  const qreal ratio = painter.device() != nullptr ? painter.device()->devicePixelRatioF() : 1.0;
  const qreal hairline = 1 / std::max<qreal>(1, ratio);
  const qreal radius = std::clamp<qreal>(backdrop.radius, 0, std::min(rect.width(), rect.height()) / 2);
  const qreal innerRadius = std::max<qreal>(0, radius - hairline / 2);
  const QRectF inner = rect.adjusted(hairline / 2, hairline / 2, -hairline / 2, -hairline / 2);
  painter.setPen(QPen(backdrop.border, hairline));
  painter.setBrush(backdrop.fill);
  painter.drawRoundedRect(inner, innerRadius, innerRadius);
  painter.restore();
}

} // namespace mitcad::start
