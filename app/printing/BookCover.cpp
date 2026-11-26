// SPDX-License-Identifier: MIT
#include "BookCover.hpp"

#include <algorithm>
#include <cmath>

#include <QFont>
#include <QFontMetricsF>
#include <QPainter>
#include <QPen>
#include <QRectF>

namespace mitcad::printing {
namespace {

// A font whose letters are `height` units of the painter high: the painter
// paints in millimetres, so its pixel size is in them.
QFont coverFont(double height, bool bold) {
  QFont font;
  font.setPixelSize(std::max(1, static_cast<int>(std::lround(height))));
  font.setBold(bold);
  return font;
}

} // namespace

PrintPicture bookCover(const QString& title, const QStringList& contents, const QSizeF& size) {
  PrintPicture picture;
  picture.name = title;
  picture.size = size;
  picture.paint = [title, contents, size](QPainter& painter, double /*scale*/) {
    painter.save();
    painter.setRenderHint(QPainter::Antialiasing, true);
    painter.setRenderHint(QPainter::TextAntialiasing, true);
    const QRectF frame(QPointF(kCoverMargin, kCoverMargin),
                       QSizeF(size.width() - 2.0 * kCoverMargin, size.height() - 2.0 * kCoverMargin));
    QPen pen(Qt::black);
    pen.setWidthF(0.5);
    painter.setPen(pen);
    painter.setBrush(Qt::NoBrush);
    painter.drawRect(frame);

    // The title: as large as kCoverTitleHeight, smaller when it is wider
    // than the frame less a margin on each side.
    const double room = frame.width() - 2.0 * kCoverMargin;
    QFont titleFont = coverFont(kCoverTitleHeight, true);
    const double width = QFontMetricsF(titleFont).horizontalAdvance(title);
    if (width > room && width > 0.0) {
      titleFont = coverFont(std::max(3.0, kCoverTitleHeight * room / width), true);
    }
    painter.setFont(titleFont);
    const double titleTop = frame.top() + frame.height() * 0.3;
    const QRectF titleRect(frame.left() + kCoverMargin, titleTop, room, kCoverTitleHeight * 2.0);
    painter.drawText(titleRect, Qt::AlignHCenter | Qt::AlignTop | Qt::TextSingleLine, title);
    const double rule = titleTop + QFontMetricsF(titleFont).height() + 4.0;
    painter.drawLine(QPointF(frame.left() + kCoverMargin, rule), QPointF(frame.right() - kCoverMargin, rule));

    // The contents under it, one line per drawing, while they fit.
    const QFont lineFont = coverFont(kCoverLineHeight, false);
    painter.setFont(lineFont);
    const double step = QFontMetricsF(lineFont).lineSpacing();
    const double bottom = frame.bottom() - kCoverMargin;
    double y = rule + step;
    for (int i = 0; i < contents.size(); ++i) {
      const bool last = y + 2.0 * step > bottom && i + 1 < contents.size();
      const QString line = last ? QStringLiteral("…") : contents.at(i);
      painter.drawText(QRectF(frame.left() + 2.0 * kCoverMargin, y, room - 2.0 * kCoverMargin, step),
                       Qt::AlignLeft | Qt::AlignTop | Qt::TextSingleLine,
                       QFontMetricsF(lineFont).elidedText(line, Qt::ElideRight, room - 2.0 * kCoverMargin));
      y += step;
      if (last) {
        break;
      }
    }
    painter.restore();
  };
  return picture;
}

} // namespace mitcad::printing
