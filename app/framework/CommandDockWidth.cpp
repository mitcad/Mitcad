// SPDX-License-Identifier: MIT
#include "CommandDockWidth.hpp"

#include <QDebug>
#include <QDockWidget>
#include <QEvent>
#include <QGuiApplication>
#include <QList>
#include <QMainWindow>
#include <QMouseEvent>
#include <QPoint>
#include <QRect>
#include <QScrollArea>
#include <QScrollBar>
#include <QSettings>
#include <QStringList>
#include <QTimer>
#include <QtLogging>

#include <algorithm>
#include <utility>

#include "GlassCard.hpp"
#include "TestSync.hpp"
#include "UiSizes.hpp"

namespace mitcad {
namespace {

const QString& widthKey() {
  static const QString key = QStringLiteral("commandPanel/dockWidth");
  return key;
}

} // namespace

void keepUnscrolledSideways(QScrollArea* scroll) {
  QScrollBar* bar = scroll->horizontalScrollBar();
  QObject::connect(bar, &QScrollBar::valueChanged, bar, [bar](int value) {
    if (value != bar->minimum()) {
      bar->setValue(bar->minimum());
    }
  });
}

int rememberedCommandDockWidth() {
  const QSettings settings;
  return std::max(0, settings.value(widthKey()).toInt());
}

void rememberCommandDockWidth(int width) {
  QSettings settings;
  settings.setValue(widthKey(), width);
}

CommandPages::CommandPages(QWidget* parent)
    : QStackedWidget(parent), m_hintWidth(commandPanelWidth(rememberedCommandDockWidth())) {
  // Never narrower than the default contents need at this run's sizes; a
  // page's own minimum does not widen the dock (an explicit minimum stands
  // for the pages' in the dock's layout).
  setMinimumWidth(commandPanelMinimumWidth());
}

QSize CommandPages::sizeHint() const {
  // As wide as the panel opens, whichever page is in it: the dock's first
  // layout takes this width, and a page that opens later does not ask for
  // another.
  return QSize(m_hintWidth, QStackedWidget::sizeHint().height());
}

QSize CommandPages::minimumSizeHint() const {
  return QSize(commandPanelMinimumWidth(), QStackedWidget::minimumSizeHint().height());
}

CommandDockWidth::CommandDockWidth(QMainWindow& window, QDockWidget& dock)
    : QObject(&dock), m_window(window), m_dock(&dock) {
  m_rest = new QTimer(this);
  m_rest->setSingleShot(true);
  m_rest->setInterval(400);
  connect(m_rest, &QTimer::timeout, this, [this] { rest(); });
  // The remembered line is logged before a test's next step.
  TestSync::watch(m_rest);
  // The separator between the view and the dock is the window's own: its
  // press and release reach the window (real input and the test driver's).
  window.installEventFilter(this);
  dock.installEventFilter(this);
}

void CommandDockWidth::restore() {
  const int remembered = rememberedCommandDockWidth();
  if (m_dock.isNull() || remembered <= 0 || m_dock->isHidden() || m_dock->isFloating()) {
    return;
  }
  const int width = commandPanelWidth(remembered);
  m_window.resizeDocks({m_dock.data()}, {width}, Qt::Horizontal);
  qInfo().noquote() << QStringLiteral("Command panel: dock width %1 restored").arg(width);
  scheduleLog();
}

bool CommandDockWidth::eventFilter(QObject* watched, QEvent* event) {
  if (watched == &m_window) {
    if (event->type() == QEvent::MouseButtonPress &&
        static_cast<QMouseEvent*>(event)->button() == Qt::LeftButton) {
      m_pressed = true;
      m_dragged = false;
    } else if (event->type() == QEvent::MouseButtonRelease &&
               static_cast<QMouseEvent*>(event)->button() == Qt::LeftButton) {
      m_pressed = false;
      if (m_dragged) {
        m_rest->start();
      }
    }
  } else if (watched == m_dock.data()) {
    if (event->type() == QEvent::Resize) {
      // Resized while the left button is held: a drag of its edge (the
      // window's own layout resizes it without).
      const bool held = m_pressed || (QGuiApplication::mouseButtons() & Qt::LeftButton) != 0;
      if (held && m_dock->isVisible() && !m_dock->isFloating()) {
        m_dragged = true;
        m_rest->start();
      }
      scheduleLog();
    } else if (event->type() == QEvent::Move || event->type() == QEvent::Show) {
      scheduleLog();
    }
  }
  return QObject::eventFilter(watched, event);
}

void CommandDockWidth::rest() {
  if (m_pressed) {
    return; // the release starts it again
  }
  if (!m_dragged || m_dock.isNull() || !m_dock->isVisible() || m_dock->isFloating()) {
    return;
  }
  m_dragged = false;
  const int width = m_dock->width();
  if (width == rememberedCommandDockWidth()) {
    return;
  }
  rememberCommandDockWidth(width);
  qInfo().noquote() << QStringLiteral("Command panel: dock width %1 remembered").arg(width);
}

void CommandDockWidth::scheduleLog() {
  TestSync::singleShot(50, this, [this] { logLayout(); });
}

void CommandDockWidth::logLayout() {
  if (m_dock.isNull() || !m_dock->isVisible() || m_dock->isFloating()) {
    return;
  }
  QStringList lines;
  const int width = m_dock->width();
  lines << QStringLiteral("Command panel width %1 (at least %2)").arg(width).arg(commandPanelMinimumWidth());
  // Where its edge is dragged: the window's separator between the view and
  // the dock (the dock on the right).
  if (const QWidget* central = m_window.centralWidget()) {
    const QRect docked = m_dock->geometry();
    const QRect middle = central->geometry();
    if (docked.left() > middle.right()) {
      const int x = docked.left() > middle.right() + 1 ? (middle.right() + 1 + docked.left()) / 2 : docked.left();
      const QPoint point = GlassCard::mapToHost(&m_window, QPoint(x, docked.center().y()));
      lines << QStringLiteral("Command edge at %1,%2").arg(point.x()).arg(point.y());
    }
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_logged) {
    m_logged = logged;
    for (const QString& line : std::as_const(lines)) {
      qDebug().noquote() << line;
    }
  }
  if (width != m_loggedWidth) {
    m_loggedWidth = width;
    if (resized) {
      resized();
    }
  }
}

} // namespace mitcad
