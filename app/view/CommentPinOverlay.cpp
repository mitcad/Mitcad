// SPDX-License-Identifier: MIT
// The comments' pins over the 3D view (mitcad#1119, app/COMMANDS.md
// "Comments"): drawn with QPainter where their points are on the view, as
// the camera moves; a click on one is taken.
#include "CommentPinOverlay.hpp"

#include <cmath>
#include <utility>

#include <QEvent>
#include <QHelpEvent>
#include <QMouseEvent>
#include <QPainter>
#include <QStringList>
#include <QToolTip>
#include <QtLogging>

#include "../OcctViewer.hpp"

namespace mitcad {

CommentPinOverlay::CommentPinOverlay(OcctViewer* viewer) : QWidget(viewer), m_viewer(viewer) {
  setObjectName(QStringLiteral("commentPinOverlay"));
  setAttribute(Qt::WA_TransparentForMouseEvents);
  setAttribute(Qt::WA_NoSystemBackground);
  setAttribute(Qt::WA_TranslucentBackground);
  setFocusPolicy(Qt::NoFocus);
  hide();
  viewer->installEventFilter(this);
  // The camera moved (a frame was drawn): the pins follow their points.
  connect(viewer, &OcctViewer::viewChanged, this, [this] {
    if (isVisible()) {
      follow();
    }
  });
  // The halo takes the background's tone.
  connect(viewer, &OcctViewer::backgroundToneChanged, this, [this] { update(); });
}

void CommentPinOverlay::setPins(const std::vector<Pin>& pins) {
  m_pins.clear();
  for (const Pin& pin : pins) {
    if (pin.look != CommentPinLook::Hidden) {
      m_pins.push_back(pin);
    }
  }
  refresh();
}

void CommentPinOverlay::setPinsShown(bool shown) {
  if (shown == m_shown) {
    return;
  }
  m_shown = shown;
  refresh();
}

void CommentPinOverlay::setHighlighted(const QString& id) {
  if (id == m_highlighted) {
    return;
  }
  m_highlighted = id;
  update();
}

void CommentPinOverlay::refresh() {
  QStringList drawn;
  for (const Pin& pin : m_pins) {
    drawn << QStringLiteral("%1 %2").arg(pin.id, commentPinLookName(pin.look));
  }
  QString logged = QStringLiteral("Comment pins drawn: %1")
                       .arg(drawn.isEmpty() ? QStringLiteral("none") : drawn.join(QStringLiteral(" | ")));
  if (!m_shown) {
    logged = QStringLiteral("Comment pins drawn: hidden (%1)").arg(m_pins.size());
  }
  if (logged != m_loggedDrawn) {
    m_loggedDrawn = logged;
    qDebug().noquote() << logged;
  }
  if (m_pins.empty() || !m_shown) {
    m_pressed.clear();
    if (m_hovering) {
      m_hovering = false;
      m_viewer->restoreCursor();
    }
    hide();
    m_loggedPlaces.clear();
    return;
  }
  show();
  lower(); // under the view's own buttons and cards
  if (geometry() != m_viewer->rect()) {
    setGeometry(m_viewer->rect());
  }
  m_drawnKey = placesKey();
  update();
  logPlaces();
}

QString CommentPinOverlay::placesKey() const {
  QStringList places;
  for (const Pin& pin : m_pins) {
    const QPointF at = m_viewer->toWidgetF(pin.at);
    places << QStringLiteral("%1,%2").arg(std::lround(at.x())).arg(std::lround(at.y()));
  }
  return places.join(QLatin1Char(' '));
}

void CommentPinOverlay::follow() {
  bool moved = false;
  if (geometry() != m_viewer->rect()) {
    setGeometry(m_viewer->rect());
    update();
    moved = true;
  }
  const QString key = placesKey();
  if (key != m_drawnKey) {
    m_drawnKey = key;
    update();
    moved = true;
  }
  if (moved) {
    logPlaces();
  }
}

void CommentPinOverlay::logPlaces() {
  QStringList lines;
  for (const Pin& pin : m_pins) {
    const QPointF tip = m_viewer->toWidgetF(pin.at);
    const QPoint head = mapTo(window(), commentPinHead(tip).toPoint());
    lines << QStringLiteral("Comment pin %1 at %2,%3").arg(pin.id).arg(head.x()).arg(head.y());
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_loggedPlaces) {
    m_loggedPlaces = logged;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  }
}

QString CommentPinOverlay::hitAt(const QPointF& at) const {
  for (auto pin = m_pins.rbegin(); pin != m_pins.rend(); ++pin) {
    if (commentPinHit(m_viewer->toWidgetF(pin->at), at)) {
      return pin->id;
    }
  }
  return QString();
}

bool CommentPinOverlay::mouse(QEvent* event) {
  if (active && !active() && m_pressed.isEmpty()) {
    if (m_hovering) {
      m_hovering = false;
      m_viewer->restoreCursor();
    }
    return false;
  }
  switch (event->type()) {
  case QEvent::MouseButtonPress: {
    auto* press = static_cast<QMouseEvent*>(event);
    if (press->button() != Qt::LeftButton) {
      return false;
    }
    const QString id = hitAt(press->position());
    if (id.isEmpty()) {
      return false;
    }
    m_pressed = id;
    return true;
  }
  case QEvent::MouseButtonDblClick: {
    auto* click = static_cast<QMouseEvent*>(event);
    return click->button() == Qt::LeftButton && !hitAt(click->position()).isEmpty();
  }
  case QEvent::MouseMove: {
    auto* move = static_cast<QMouseEvent*>(event);
    if (!m_pressed.isEmpty()) {
      return true;
    }
    const bool over = move->buttons() == Qt::NoButton && !hitAt(move->position()).isEmpty();
    if (over != m_hovering) {
      m_hovering = over;
      if (over) {
        m_viewer->setCursor(Qt::PointingHandCursor);
      } else {
        m_viewer->restoreCursor();
      }
    }
    return false;
  }
  case QEvent::MouseButtonRelease: {
    if (m_pressed.isEmpty()) {
      return false;
    }
    const QString pressed = m_pressed;
    m_pressed.clear();
    auto* release = static_cast<QMouseEvent*>(event);
    if (hitAt(release->position()) != pressed) {
      return true; // let go elsewhere: nothing
    }
    qDebug().noquote() << QStringLiteral("Comment pin clicked: %1").arg(pressed);
    if (clicked) {
      clicked(pressed);
    }
    return true;
  }
  case QEvent::ToolTip: {
    auto* help = static_cast<QHelpEvent*>(event);
    const QString id = hitAt(QPointF(help->pos()));
    for (const Pin& pin : m_pins) {
      if (pin.id == id && !pin.tip.isEmpty()) {
        QToolTip::showText(help->globalPos(), pin.tip, m_viewer);
        return true;
      }
    }
    return false;
  }
  default:
    return false;
  }
}

bool CommentPinOverlay::eventFilter(QObject* watched, QEvent* event) {
  if (watched == m_viewer) {
    if (event->type() == QEvent::Resize && isVisible()) {
      follow();
    }
    if (isVisible() && mouse(event)) {
      return true;
    }
  }
  return QWidget::eventFilter(watched, event);
}

void CommentPinOverlay::paintEvent(QPaintEvent* /*event*/) {
  QPainter painter(this);
  const bool dark = m_viewer->backgroundDark();
  for (const Pin& pin : m_pins) {
    paintCommentPin(painter, m_viewer->toWidgetF(pin.at), pin.number, pin.look, dark, pin.id == m_highlighted);
  }
}

} // namespace mitcad
