// SPDX-License-Identifier: MIT
#include "RegionPicker.hpp"

#include <algorithm>

#include <QCoreApplication>
#include <QCursor>
#include <QEvent>
#include <QFontMetrics>
#include <QKeyEvent>
#include <QMouseEvent>
#include <QPainter>
#include <QPaintEvent>
#include <QPalette>
#include <QRegion>
#include <QWidget>
#include <QtLogging>

#include "report/FeedbackRegion.hpp"

namespace mitcad::report {
namespace {

// One screen's part of the overlay: a translucent, frameless window that
// stays on top, dims what it covers and passes its input to the picker.
class RegionWindow : public QWidget {
public:
  RegionWindow(RegionPicker* picker, const QRect& part, QWidget* parent)
      : QWidget(parent, Qt::Tool | Qt::FramelessWindowHint | Qt::WindowStaysOnTopHint | Qt::NoDropShadowWindowHint),
        m_picker(picker) {
    setObjectName(QStringLiteral("feedbackRegionOverlay"));
    setAttribute(Qt::WA_TranslucentBackground);
    setAttribute(Qt::WA_NoSystemBackground);
    setAutoFillBackground(false);
    setFocusPolicy(Qt::StrongFocus);
    setCursor(Qt::CrossCursor);
    setMouseTracking(true);
    setGeometry(part);
  }

protected:
  bool event(QEvent* event) override {
    // Every key is the overlay's while it is up: no shortcut of the
    // windows under it (Esc, the command's own key held) is triggered.
    if (event->type() == QEvent::ShortcutOverride) {
      event->accept();
      return true;
    }
    return QWidget::event(event);
  }

  void paintEvent(QPaintEvent*) override {
    QPainter painter(this);
    painter.setCompositionMode(QPainter::CompositionMode_Source);
    const QColor dim(0, 0, 0, 110);
    // Not wholly clear: a window's pixel without any alpha lets the mouse
    // through on some systems.
    const QColor clear(0, 0, 0, 1);
    const QRect global = m_picker->dragged();
    const QRect inside = global.isNull() ? QRect() : QRect(mapFromGlobal(global.topLeft()), global.size());
    if (inside.isNull() || !inside.intersects(rect())) {
      painter.fillRect(rect(), dim);
    } else {
      QRegion outside(rect());
      outside -= inside;
      painter.setClipRegion(outside);
      painter.fillRect(rect(), dim);
      painter.setClipping(false);
      painter.fillRect(inside.intersected(rect()), clear);
    }
    painter.setCompositionMode(QPainter::CompositionMode_SourceOver);
    painter.setRenderHint(QPainter::Antialiasing);
    const QColor accent = palette().color(QPalette::Highlight);
    if (!inside.isNull()) {
      painter.setPen(QPen(accent, 1));
      painter.setBrush(Qt::NoBrush);
      painter.drawRect(QRectF(inside).adjusted(0.5, 0.5, -0.5, -0.5));
      // The size beside it, in the picture's pixels: below its right
      // corner, or inside it where the screen ends.
      const qreal ratio = devicePixelRatioF();
      const QString size = QStringLiteral("%1 × %2")
                               .arg(qRound(global.width() * ratio))
                               .arg(qRound(global.height() * ratio));
      drawLabel(painter, size, inside.bottomRight() + QPoint(-4, 6), Qt::AlignRight | Qt::AlignTop);
    } else if (!m_picker->dragging()) {
      drawLabel(painter, RegionPicker::hint(), QPoint(width() / 2, 24), Qt::AlignHCenter | Qt::AlignTop);
    }
  }

  void mousePressEvent(QMouseEvent* event) override {
    m_picker->press(event->globalPosition().toPoint(), event->button());
    event->accept();
  }
  void mouseMoveEvent(QMouseEvent* event) override {
    m_picker->move(event->globalPosition().toPoint());
    event->accept();
  }
  void mouseReleaseEvent(QMouseEvent* event) override {
    m_picker->release(event->globalPosition().toPoint(), event->button());
    event->accept();
  }
  void mouseDoubleClickEvent(QMouseEvent* event) override { event->accept(); }
  void keyPressEvent(QKeyEvent* event) override {
    m_picker->keyPress(event->key());
    event->accept();
  }
  void keyReleaseEvent(QKeyEvent* event) override {
    m_picker->keyRelease(event->key(), event->isAutoRepeat());
    event->accept();
  }

private:
  // A line of text on a dark rounded plate, anchored at `at` as `align`
  // says, kept inside the window.
  void drawLabel(QPainter& painter, const QString& text, const QPoint& at, Qt::Alignment align) {
    const QFontMetrics metrics(font());
    const QSize size(metrics.horizontalAdvance(text) + 12, metrics.height() + 6);
    QPoint corner = at;
    if (align & Qt::AlignRight) {
      corner.rx() -= size.width();
    } else if (align & Qt::AlignHCenter) {
      corner.rx() -= size.width() / 2;
    }
    QRect plate(corner, size);
    if (plate.bottom() > height() - 2) {
      // Where the screen ends below it: just above the anchor instead.
      plate.moveBottom(at.y() - 12);
    }
    plate.moveLeft(std::clamp(plate.left(), 2, std::max(2, width() - size.width() - 2)));
    plate.moveTop(std::max(plate.top(), 2));
    painter.setPen(Qt::NoPen);
    painter.setBrush(QColor(20, 20, 20, 210));
    painter.drawRoundedRect(QRectF(plate), 4, 4);
    painter.setPen(Qt::white);
    painter.drawText(plate, Qt::AlignCenter, text);
  }

  RegionPicker* m_picker;
};

} // namespace

RegionPicker::RegionPicker(const QRect& area, const QList<QRect>& screens, int heldKey, QWidget* parent)
    : QObject(parent), m_heldKey(heldKey) {
  for (const QRect& part : regionParts(area, screens)) {
    if (!part.isEmpty()) {
      m_parts.append(part);
      m_windows.append(new RegionWindow(this, part, parent));
    }
  }
}

RegionPicker::~RegionPicker() {
  if (m_grabber) {
    m_grabber->releaseKeyboard();
    m_grabber->releaseMouse();
  }
  for (const QPointer<QWidget>& window : m_windows) {
    if (window) {
      window->hide();
      window->deleteLater();
    }
  }
}

QString RegionPicker::hint() { return tr("Drag over the part to report. Esc cancels."); }

QList<QWidget*> RegionPicker::windows() const {
  QList<QWidget*> windows;
  for (const QPointer<QWidget>& window : m_windows) {
    if (window) {
      windows.append(window.data());
    }
  }
  return windows;
}

bool RegionPicker::start() {
  if (m_windows.isEmpty()) {
    return false;
  }
  for (const QPointer<QWidget>& window : m_windows) {
    window->show();
    window->raise();
  }
  // The window under the mouse takes the keys and the mouse (each of them
  // gets the input over it anyway; the grab keeps a drag beyond them).
  m_grabber = m_windows.constFirst();
  const QPoint cursor = QCursor::pos();
  for (qsizetype i = 0; i < m_parts.size(); ++i) {
    if (m_parts.at(i).contains(cursor) && m_windows.at(i)) {
      m_grabber = m_windows.at(i);
    }
  }
  m_grabber->activateWindow();
  m_grabber->setFocus(Qt::OtherFocusReason);
  m_grabber->grabMouse(Qt::CrossCursor);
  m_grabber->grabKeyboard();
  return true;
}

QRect RegionPicker::dragged() const {
  if (!m_pressed || m_done) {
    return {};
  }
  QRect clipped;
  const QRect rect = QRect(m_from, m_to).normalized();
  for (const QRect& part : m_parts) {
    const QRect inside = rect.intersected(part);
    if (!inside.isEmpty()) {
      clipped = clipped.isNull() ? inside : clipped.united(inside);
    }
  }
  return clipped;
}

void RegionPicker::press(const QPoint& global, Qt::MouseButton button) {
  if (m_done) {
    return;
  }
  if (button != Qt::LeftButton) {
    cancel(QStringLiteral("another button"));
    return;
  }
  m_from = m_to = global;
  m_pressed = true;
  repaint();
}

void RegionPicker::move(const QPoint& global) {
  if (m_done || !m_pressed) {
    return;
  }
  m_to = global;
  repaint();
}

void RegionPicker::release(const QPoint& global, Qt::MouseButton button) {
  if (m_done || !m_pressed || button != Qt::LeftButton) {
    return;
  }
  m_to = global;
  m_pressed = false;
  const QRect region = regionFromDrag(m_from, m_to, m_parts);
  if (region.isNull()) {
    cancel(QStringLiteral("a click without a drag"));
  } else {
    finish(region);
  }
}

void RegionPicker::keyPress(int key) {
  if (!m_done && key == Qt::Key_Escape) {
    cancel(QStringLiteral("Esc"));
  }
}

void RegionPicker::keyRelease(int key, bool autoRepeat) {
  // The command's key held through the drag: its release takes the
  // region. Released before a drag (pressed once), the mouse's release
  // takes it later.
  if (m_done || autoRepeat || key != m_heldKey || m_heldKey == Qt::Key_unknown || !m_pressed) {
    return;
  }
  m_pressed = false;
  const QRect region = regionFromDrag(m_from, m_to, m_parts);
  if (region.isNull()) {
    cancel(QStringLiteral("the key released without a drag"));
  } else {
    finish(region);
  }
}

void RegionPicker::finish(const QRect& region) {
  m_done = true;
  if (m_grabber) {
    m_grabber->releaseKeyboard();
    m_grabber->releaseMouse();
  }
  for (const QPointer<QWidget>& window : m_windows) {
    if (window) {
      window->hide();
    }
  }
  emit picked(region);
}

void RegionPicker::cancel(const QString& why) {
  m_done = true;
  m_pressed = false;
  if (m_grabber) {
    m_grabber->releaseKeyboard();
    m_grabber->releaseMouse();
  }
  for (const QPointer<QWidget>& window : m_windows) {
    if (window) {
      window->hide();
    }
  }
  qInfo().noquote() << QStringLiteral("Feedback region: canceled (%1)").arg(why);
  emit canceled();
}

void RegionPicker::repaint() {
  for (const QPointer<QWidget>& window : m_windows) {
    if (window) {
      window->update();
    }
  }
}

} // namespace mitcad::report
