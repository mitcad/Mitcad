// SPDX-License-Identifier: MIT
#include "framework/WindowPlace.hpp"

#include "framework/InterfaceSize.hpp"

#include <algorithm>
#include <cstdlib>

#include <QDebug>
#include <QEvent>
#include <QGuiApplication>
#include <QObject>
#include <QPointer>
#include <QScreen>
#include <QSettings>
#include <QStringList>
#include <QTimer>
#include <QVariant>
#include <QWidget>
#include <QtLogging>

namespace mitcad {
namespace {

constexpr const char* kGeometryKey = "mainWindow/geometry";
constexpr const char* kFrameKey = "mainWindow/frame";
constexpr const char* kScreenKey = "mainWindow/screen";
constexpr const char* kMaximizedKey = "mainWindow/maximized";
constexpr const char* kFullScreenKey = "mainWindow/fullScreen";

// A kept size beyond any screen is not a window's.
constexpr int kLargest = 100000;
// The title bar and borders of a window whose own were never measured: a
// typical size, in logical pixels (as FitToScreen's).
constexpr QMargins kTypicalFrame(8, 40, 8, 8);
// How far a new window is cascaded where the title bar's height is not
// known (no window manager draws one).
constexpr int kCascadeStep = 28;

// The connected screens' available areas, the index of the screen named
// `name` (else the primary's) and of `screen` (else -1).
struct Screens {
  QList<QRect> available;
  int named = -1;
  int primary = 0;
  int of = -1;
};

Screens connectedScreens(const QString& name, const QScreen* screen = nullptr) {
  Screens result;
  const QList<QScreen*> screens = QGuiApplication::screens();
  const QScreen* primary = QGuiApplication::primaryScreen();
  for (int i = 0; i < screens.size(); ++i) {
    result.available.append(screens.at(i)->availableGeometry());
    if (screens.at(i) == primary) {
      result.primary = i;
    }
    if (result.named < 0 && !name.isEmpty() && screens.at(i)->name() == name) {
      result.named = i;
    }
    if (screens.at(i) == screen) {
      result.of = i;
    }
  }
  return result;
}

// The window's own area inside `frame`, at least one pixel.
QRect innerOf(const QRect& frame, const QMargins& margins) {
  const QRect inner = frame.marginsRemoved(margins);
  return QRect(inner.topLeft(), inner.size().expandedTo(QSize(1, 1)));
}

// Gives a top-level window a state (maximized, full screen) once it has
// shown in its normal state at its place, in the event loop after its
// first show; then ends. The platform takes that place as the window's
// normal one, where leaving the state brings it back.
class StateAfterShow final : public QObject {
public:
  StateAfterShow(QWidget* window, Qt::WindowStates state) : QObject(window), m_state(state) {
    window->installEventFilter(this);
  }

protected:
  bool eventFilter(QObject* watched, QEvent* event) override {
    if (event->type() == QEvent::Show && watched == parent()) {
      watched->removeEventFilter(this);
      const QPointer<QWidget> window(static_cast<QWidget*>(watched));
      const Qt::WindowStates state = m_state;
      QTimer::singleShot(0, window.data(), [window, state] {
        if (window != nullptr && window->isVisible()) {
          window->setWindowState(
              (window->windowState() & ~(Qt::WindowMaximized | Qt::WindowFullScreen | Qt::WindowMinimized)) | state);
        }
      });
      deleteLater();
    }
    return QObject::eventFilter(watched, event);
  }

private:
  Qt::WindowStates m_state;
};

QString rectText(const QRect& rect) {
  return QStringLiteral("%1,%2 %3x%4").arg(rect.x()).arg(rect.y()).arg(rect.width()).arg(rect.height());
}

} // namespace

QRect fitFrameToScreens(const QRect& frame, const QList<QRect>& available, int preferred) {
  if (available.isEmpty() || !frame.isValid()) {
    return frame;
  }
  for (const QRect& area : available) {
    if (area.contains(frame)) {
      return frame;
    }
  }
  const QRect area = available.at(preferred >= 0 && preferred < available.size() ? preferred : 0);
  const int width = std::min(frame.width(), area.width());
  const int height = std::min(frame.height(), area.height());
  // The top left corner first: the title bar is on the screen also where
  // the window is larger than it.
  const int x = std::clamp(frame.x(), area.x(), area.x() + area.width() - width);
  const int y = std::clamp(frame.y(), area.y(), area.y() + area.height() - height);
  return QRect(x, y, width, height);
}

QRect cascadedFrame(const QRect& from, const QSize& frameSize, int step, const QRect& available) {
  QPoint at = from.topLeft() + QPoint(step, step);
  if (available.isValid()) {
    if (at.x() + frameSize.width() > available.x() + available.width()) {
      at.setX(available.x());
    }
    if (at.y() + frameSize.height() > available.y() + available.height()) {
      at.setY(available.y());
    }
  }
  return QRect(at, frameSize);
}

bool frameMarginsFromText(const QString& text, QMargins& margins) {
  const QStringList parts = text.split(QLatin1Char(','));
  if (parts.size() != 4) {
    return false;
  }
  int values[4] = {0, 0, 0, 0};
  for (int i = 0; i < 4; ++i) {
    bool ok = false;
    values[i] = parts.at(i).trimmed().toInt(&ok);
    if (!ok || values[i] < 0 || values[i] > 500) {
      return false;
    }
  }
  margins = QMargins(values[0], values[1], values[2], values[3]);
  return true;
}

QString frameMarginsText(const QMargins& margins) {
  return QStringLiteral("%1,%2,%3,%4").arg(margins.left()).arg(margins.top()).arg(margins.right()).arg(margins.bottom());
}

WindowPlace savedWindowPlace(const QSettings& settings) {
  WindowPlace place;
  const QRect geometry = settings.value(QLatin1String(kGeometryKey)).toRect();
  if (geometry.width() < 1 || geometry.height() < 1 || geometry.width() > kLargest || geometry.height() > kLargest ||
      std::abs(geometry.x()) > kLargest || std::abs(geometry.y()) > kLargest) {
    return place;
  }
  place.geometry = geometry;
  // An INI file written by hand without quotes holds it as a list.
  const QVariant frame = settings.value(QLatin1String(kFrameKey));
  const QString frameText = frame.typeId() == QMetaType::QStringList ? frame.toStringList().join(QLatin1Char(','))
                                                                      : frame.toString();
  place.frameKnown = frameMarginsFromText(frameText, place.frame);
  place.screen = settings.value(QLatin1String(kScreenKey)).toString();
  place.maximized = settings.value(QLatin1String(kMaximizedKey), false).toBool();
  place.fullScreen = settings.value(QLatin1String(kFullScreenKey), false).toBool();
  return place;
}

void saveWindowPlace(QSettings& settings, const WindowPlace& place) {
  if (!place.valid()) {
    return;
  }
  settings.setValue(QLatin1String(kGeometryKey), place.geometry);
  if (place.frameKnown) {
    settings.setValue(QLatin1String(kFrameKey), frameMarginsText(place.frame));
  }
  settings.setValue(QLatin1String(kScreenKey), place.screen);
  settings.setValue(QLatin1String(kMaximizedKey), place.maximized);
  settings.setValue(QLatin1String(kFullScreenKey), place.fullScreen);
}

WindowPlace windowPlaceOf(const QWidget* window) {
  WindowPlace place;
  if (window == nullptr || !window->isWindow()) {
    return place;
  }
  const Qt::WindowStates state = window->windowState();
  if (state.testFlag(Qt::WindowMinimized)) {
    return place;
  }
  place.maximized = state.testFlag(Qt::WindowMaximized);
  place.fullScreen = state.testFlag(Qt::WindowFullScreen);
  const bool normal = !place.maximized && !place.fullScreen;
  place.geometry = normal ? window->geometry() : window->normalGeometry();
  if (!place.geometry.isValid()) {
    place.geometry = window->geometry();
  }
  if (normal && window->isVisible()) {
    const QRect frame = window->frameGeometry();
    const QRect inner = window->geometry();
    const QMargins margins(inner.left() - frame.left(), inner.top() - frame.top(), frame.right() - inner.right(),
                           frame.bottom() - inner.bottom());
    if (margins.left() >= 0 && margins.top() >= 0 && margins.right() >= 0 && margins.bottom() >= 0) {
      place.frame = margins;
      place.frameKnown = true;
    }
  }
  if (const QScreen* screen = window->screen()) {
    place.screen = screen->name();
  }
  return place;
}

void saveWindowPlace(const QWidget* window) {
  if (window == nullptr || !window->isWindow() || window->windowHandle() == nullptr) {
    return;
  }
  const WindowPlace place = windowPlaceOf(window);
  if (!place.valid()) {
    return;
  }
  QSettings settings;
  saveWindowPlace(settings, place);
  qInfo().noquote() << QStringLiteral("Window place saved: %1").arg(windowPlaceDescription(place));
}

void restoreWindowPlace(QWidget* window) {
  if (window == nullptr || !window->isWindow()) {
    return;
  }
  const WindowPlace saved = savedWindowPlace(QSettings());
  if (!saved.valid()) {
    const QSize size = startWindowSize();
    window->resize(size);
    qInfo().noquote() << QStringLiteral("Window place: none saved, %1x%2").arg(size.width()).arg(size.height());
    return;
  }
  const Screens screens = connectedScreens(saved.screen);
  const QMargins frame = saved.frameKnown ? saved.frame : kTypicalFrame;
  const QRect kept = saved.geometry.marginsAdded(frame);
  const QRect fitted = fitFrameToScreens(kept, screens.available, screens.named >= 0 ? screens.named : screens.primary);
  WindowPlace restored = saved;
  restored.geometry = innerOf(fitted, frame);
  window->setGeometry(restored.geometry);
  window->setWindowState(window->windowState() & ~(Qt::WindowMaximized | Qt::WindowFullScreen | Qt::WindowMinimized));
  // Maximized or full screen only once the window has shown at its normal
  // place: a window first shown maximized on Windows took the maximized
  // frame as its normal one, and leaving maximized kept it there.
  Qt::WindowStates state;
  if (saved.maximized) {
    state |= Qt::WindowMaximized;
  }
  if (saved.fullScreen) {
    state |= Qt::WindowFullScreen;
  }
  if (state != Qt::WindowStates()) {
    new StateAfterShow(window, state);
  }
  QString detail = windowPlaceDescription(restored);
  if (fitted != kept) {
    detail += QStringLiteral(", moved onto the screen from %1").arg(rectText(saved.geometry));
  }
  qInfo().noquote() << QStringLiteral("Window place restored: %1").arg(detail);
}

void cascadeWindowPlace(QWidget* window, const QWidget* from) {
  if (window == nullptr || !window->isWindow()) {
    return;
  }
  if (from == nullptr || !from->isVisible()) {
    restoreWindowPlace(window);
    return;
  }
  // The window it opens from has the last place.
  saveWindowPlace(from);
  const WindowPlace current = windowPlaceOf(from);
  const WindowPlace saved = savedWindowPlace(QSettings());
  const QMargins frame = current.frameKnown ? current.frame : saved.frameKnown ? saved.frame : kTypicalFrame;
  const QSize size = current.valid() ? current.geometry.size()
                                     : saved.valid() ? saved.geometry.size() : startWindowSize();
  const QRect fromFrame = current.valid() && current.frameKnown
                              ? from->frameGeometry()
                              : (current.valid() ? current.geometry : from->geometry()).marginsAdded(frame);
  const int step = frame.top() >= 16 ? frame.top() : kCascadeStep;
  const Screens screens = connectedScreens(QString(), from->screen());
  const int preferred = screens.of >= 0 ? screens.of : screens.primary;
  QRect target = cascadedFrame(fromFrame, size.grownBy(frame), step, screens.available.value(preferred));
  target = fitFrameToScreens(target, screens.available, preferred);
  WindowPlace place;
  place.geometry = innerOf(target, frame);
  place.frame = frame;
  place.frameKnown = true;
  place.screen = current.screen;
  window->setGeometry(place.geometry);
  window->setWindowState(window->windowState() & ~(Qt::WindowMaximized | Qt::WindowFullScreen | Qt::WindowMinimized));
  qInfo().noquote() << QStringLiteral("Window place cascaded: %1").arg(windowPlaceDescription(place));
}

QString windowPlaceDescription(const WindowPlace& place) {
  QString text = rectText(place.geometry);
  if (place.frameKnown) {
    text += QStringLiteral(" (frame %1)").arg(frameMarginsText(place.frame));
  }
  if (!place.screen.isEmpty()) {
    text += QStringLiteral(" on %1").arg(place.screen);
  }
  text += place.fullScreen ? QStringLiteral(", full screen")
          : place.maximized ? QStringLiteral(", maximized")
                            : QStringLiteral(", normal");
  return text;
}

} // namespace mitcad
