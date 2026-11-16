// SPDX-License-Identifier: MIT
#include "BusyMark.hpp"

#include <algorithm>

#include <QColor>
#include <QEvent>
#include <QFontMetrics>
#include <QPainter>
#include <QPalette>
#include <QPen>
#include <QRectF>
#include <QSizePolicy>
#include <QTimer>

namespace mitcad {
namespace {

// The spinner's steps in a turn and the time of one step: a turn a second.
constexpr int kSteps = 12;
constexpr int kStepMs = 80;

} // namespace

BusyMark::BusyMark(QWidget* parent) : QWidget(parent) {
  // Its room is kept while it is hidden: showing it moves nothing.
  QSizePolicy policy(QSizePolicy::Fixed, QSizePolicy::Fixed);
  policy.setRetainSizeWhenHidden(true);
  setSizePolicy(policy);
  setFocusPolicy(Qt::NoFocus);
  updateSize();
  m_delay = new QTimer(this);
  m_delay->setSingleShot(true);
  m_delay->setInterval(kDelayMs);
  connect(m_delay, &QTimer::timeout, this, [this] {
    if (busy()) {
      setShown(true);
    }
  });
  m_grace = new QTimer(this);
  m_grace->setSingleShot(true);
  m_grace->setInterval(kGraceMs);
  connect(m_grace, &QTimer::timeout, this, [this] {
    if (!busy()) {
      setShown(false);
    }
  });
  m_spin = new QTimer(this);
  m_spin->setInterval(kStepMs);
  connect(m_spin, &QTimer::timeout, this, [this] {
    m_step = (m_step + 1) % kSteps;
    update();
  });
  hide();
}

void BusyMark::setBusy(const QString& text) {
  if (text == m_text) {
    return;
  }
  const bool started = m_text.isEmpty() && !text.isEmpty();
  m_text = text;
  setAccessibleName(text);
  setToolTip(text);
  if (text.isEmpty()) {
    m_since.invalidate();
    m_delay->stop();
    if (m_shown) {
      m_grace->start();
    }
    return;
  }
  if (started) {
    m_since.start();
    m_grace->stop();
    // Still shown from work that has just ended: it stays.
    if (!m_shown) {
      m_delay->start();
    }
  }
}

qint64 BusyMark::busyMs() const { return m_since.isValid() ? m_since.elapsed() : -1; }

void BusyMark::setShown(bool on) {
  if (on == m_shown) {
    return;
  }
  m_shown = on;
  setVisible(on);
  if (on) {
    m_step = 0;
    m_spin->start();
    m_shownSince.start();
    emit shownChanged(true, busyMs());
  } else {
    m_spin->stop();
    emit shownChanged(false, m_shownSince.isValid() ? m_shownSince.elapsed() : 0);
    m_shownSince.invalidate();
  }
}

void BusyMark::updateSize() {
  // As high as a line of the panel's text, and as wide.
  const int side = std::max(12, fontMetrics().height());
  setFixedSize(side, side);
}

void BusyMark::changeEvent(QEvent* event) {
  if (event->type() == QEvent::FontChange) {
    updateSize();
  } else if (event->type() == QEvent::PaletteChange) {
    update();
  }
  QWidget::changeEvent(event);
}

void BusyMark::paintEvent(QPaintEvent* /*event*/) {
  QPainter painter(this);
  painter.setRenderHint(QPainter::Antialiasing);
  const qreal side = std::min(width(), height());
  const qreal penWidth = std::max<qreal>(2.0, side / 7.0);
  const qreal inset = penWidth / 2.0 + 1.0;
  const QRectF ring(inset, inset, side - 2.0 * inset, side - 2.0 * inset);
  // A faint ring and a quarter of it in the text's colour, turning: the
  // palette's colours follow the theme (light, dark, a Floating card).
  QColor color = palette().color(QPalette::WindowText);
  QColor track = color;
  track.setAlpha(50);
  QPen pen(track, penWidth, Qt::SolidLine, Qt::RoundCap);
  painter.setPen(pen);
  painter.drawEllipse(ring);
  color.setAlpha(220);
  pen.setColor(color);
  painter.setPen(pen);
  // Clockwise from the top (Qt's angles count counterclockwise, in 1/16
  // degrees).
  const int start = 90 * 16 - m_step * (360 * 16 / kSteps);
  painter.drawArc(ring, start, -90 * 16);
}

} // namespace mitcad
