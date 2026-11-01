// SPDX-License-Identifier: MIT
#include "ModeBanner.hpp"

#include <algorithm>

#include <QEvent>
#include <QFontMetrics>
#include <QPainter>
#include <QPainterPath>
#include <QtLogging>

#include "../OcctViewer.hpp"

namespace mitcad {
namespace {

constexpr int kMargin = 12;  // from the view's edges and the cards over it
constexpr int kPadding = 14; // the text's side margins in the card
constexpr int kHeight = 30;

} // namespace

ModeBanner::ModeBanner(OcctViewer* viewer) : QWidget(viewer), m_viewer(viewer) {
  setObjectName(QStringLiteral("modeBanner"));
  setAttribute(Qt::WA_TransparentForMouseEvents);
  setAttribute(Qt::WA_NoSystemBackground);
  setAttribute(Qt::WA_TranslucentBackground);
  setFocusPolicy(Qt::NoFocus);
  QFont bold = font();
  bold.setBold(true);
  setFont(bold);
  hide();
  viewer->installEventFilter(this);
  // The cards over the view's top may move (overlay insets): the banner
  // follows when the view draws again.
  connect(viewer, &OcctViewer::viewChanged, this, [this] {
    if (isVisible()) {
      place();
    }
  });
}

void ModeBanner::setText(const QString& text) {
  if (text == m_text) {
    return;
  }
  m_text = text;
  if (text.isEmpty()) {
    hide();
    m_loggedPlace.clear(); // logged again when it shows
    qDebug() << "View banner hidden";
    return;
  }
  place();
  show();
  raise();
  update();
  qDebug().noquote() << QStringLiteral("View banner: %1").arg(text);
}

QRect ModeBanner::cardRect() const {
  const QMargins insets = m_viewer->overlayInsets();
  const int left = insets.left() + kMargin;
  const int room = std::max(120, m_viewer->width() - insets.left() - insets.right() - 2 * kMargin);
  const int width = std::min(room, QFontMetrics(font()).horizontalAdvance(m_text) + 2 * kPadding);
  return {left + (room - width) / 2, insets.top() + kMargin, width, kHeight};
}

void ModeBanner::place() {
  const QRect rect = cardRect();
  if (rect != geometry()) {
    setGeometry(rect);
  }
  // Where it is in the window, for UI tests (when it moved): below the
  // cards over the view's top.
  const QPoint at = m_viewer->mapTo(m_viewer->window(), rect.topLeft());
  const QString place =
      QStringLiteral("View banner at %1,%2 %3x%4").arg(at.x()).arg(at.y()).arg(rect.width()).arg(rect.height());
  if (place != m_loggedPlace) {
    m_loggedPlace = place;
    qDebug().noquote() << place;
  }
}

bool ModeBanner::eventFilter(QObject* watched, QEvent* event) {
  if (watched == m_viewer && event->type() == QEvent::Resize && !m_text.isEmpty()) {
    place();
  }
  return QWidget::eventFilter(watched, event);
}

void ModeBanner::paintEvent(QPaintEvent* /*event*/) {
  QPainter painter(this);
  painter.setRenderHint(QPainter::Antialiasing);
  const QRectF card = QRectF(rect()).adjusted(0.5, 0.5, -0.5, -0.5);
  QColor background = palette().color(QPalette::Window);
  background.setAlpha(235);
  const QColor accent = viewColor(ColorRole::Hover, false); // the plane pick's hover colour
  QPainterPath path;
  path.addRoundedRect(card, card.height() / 2.0, card.height() / 2.0);
  painter.fillPath(path, background);
  painter.setPen(QPen(accent, 1.5));
  painter.drawPath(path);
  painter.setPen(palette().color(QPalette::WindowText));
  const QRect text = rect().adjusted(kPadding, 0, -kPadding, 0);
  painter.drawText(text, Qt::AlignCenter,
                   QFontMetrics(font()).elidedText(m_text, Qt::ElideMiddle, text.width()));
}

} // namespace mitcad
