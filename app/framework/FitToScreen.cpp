// SPDX-License-Identifier: MIT
#include "FitToScreen.hpp"

#include <algorithm>

#include <QCoreApplication>
#include <QEvent>
#include <QGuiApplication>
#include <QLayout>
#include <QLatin1String>
#include <QObject>
#include <QPointer>
#include <QRect>
#include <QScreen>
#include <QScrollBar>
#include <QWidget>
#include <QWindow>

namespace mitcad {
namespace {

// The frame of a window not shown yet (so not known): a title bar and
// borders of a typical size, in logical pixels.
constexpr int kFrameWidth = 16;
constexpr int kFrameHeight = 48;

QSize frameOf(const QWidget* window) {
  if (window->isVisible()) {
    const QSize frame = window->frameGeometry().size() - window->geometry().size();
    return QSize(std::max(0, frame.width()), std::max(0, frame.height()));
  }
  return QSize(kFrameWidth, kFrameHeight);
}

const QScreen* screenOf(const QWidget* window) {
  const QScreen* screen = window->screen();
  return screen != nullptr ? screen : QGuiApplication::primaryScreen();
}

// A shown window's frame moved up or left (or down or right) into the
// available area; the frame as it is now.
void moveInside(QWidget* window, const QRect& available) {
  if (!window->isVisible()) {
    return;
  }
  const QRect frame = window->frameGeometry();
  const int x = std::clamp(frame.x(), available.left(), std::max(available.left(), available.right() + 1 - frame.width()));
  const int y = std::clamp(frame.y(), available.top(), std::max(available.top(), available.bottom() + 1 - frame.height()));
  if (x != frame.x() || y != frame.y()) {
    window->move(x, y);
  }
}

// The name of keepWindowOnScreen's connections' context, a child of the window.
const char* const kKeptOnScreen = "mitcadKeptOnScreen";

} // namespace

QSize FittingScrollArea::sizeHint() const {
  QWidget* inner = widget();
  if (inner == nullptr) {
    return QScrollArea::sizeHint();
  }
  const int frame = 2 * frameWidth();
  QSize content = inner->size();
  if (widgetResizable()) {
    content = inner->sizeHint().expandedTo(inner->minimumSizeHint());
    // Word-wrapped text is taller the narrower it is: the contents' height
    // at the width they get, which is not below the minimum width set.
    const int width = std::max(content.width(), minimumWidth() - frame);
    const QLayout* layout = inner->layout();
    if (layout != nullptr && layout->hasHeightForWidth()) {
      content.setHeight(std::max(content.height(), layout->totalHeightForWidth(width)));
    }
  }
  QSize hint(content.width() + frame, content.height() + frame);
  if (verticalScrollBarPolicy() == Qt::ScrollBarAlwaysOn) {
    hint.rwidth() += verticalScrollBar()->sizeHint().width();
  }
  if (horizontalScrollBarPolicy() == Qt::ScrollBarAlwaysOn) {
    hint.rheight() += horizontalScrollBar()->sizeHint().height();
  }
  return hint;
}

bool FittingScrollArea::eventFilter(QObject* watched, QEvent* event) {
  if (watched == widget() && event->type() == QEvent::LayoutRequest) {
    updateGeometry();
  }
  return QScrollArea::eventFilter(watched, event);
}

QSize fittedWindowSize(const QSize& hint, const QSize& available, const QSize& frame) {
  return QSize(std::min(hint.width(), std::max(0, available.width() - frame.width())),
               std::min(hint.height(), std::max(0, available.height() - frame.height())));
}

void fitWindowToContents(QWidget* window, FitMode mode, const QSize& preferred) {
  if (window == nullptr) {
    return;
  }
  const QScreen* screen = screenOf(window);
  if (screen == nullptr) {
    return;
  }
  // The layout changes of the contents not handled yet (a field just shown
  // or hidden) count in the hint.
  QCoreApplication::sendPostedEvents(nullptr, QEvent::LayoutRequest);
  const QRect available = screen->availableGeometry();
  QSize wanted = window->sizeHint().expandedTo(window->minimumSize());
  if (preferred.isValid()) {
    wanted = wanted.expandedTo(preferred);
  }
  QSize target = fittedWindowSize(wanted, available.size(), frameOf(window));
  if (mode == FitMode::GrowOnly) {
    target = target.expandedTo(window->size());
  }
  if (target != window->size()) {
    window->resize(target);
  }
  // The frame as it is now, inside the available area.
  moveInside(window, available);
}

void capWindowToScreen(QWidget* window) {
  if (window == nullptr || !window->isVisible()) {
    return;
  }
  const QScreen* screen = screenOf(window);
  if (screen == nullptr) {
    return;
  }
  const QRect available = screen->availableGeometry();
  const QSize target = fittedWindowSize(window->size(), available.size(), frameOf(window));
  if (target != window->size()) {
    window->resize(target);
  }
  moveInside(window, available);
}

void capNewWindowToScreen(QWidget* window) {
  if (window == nullptr || !window->isWindow()) {
    return;
  }
  const Qt::WindowStates state = window->windowState();
  if (state.testFlag(Qt::WindowMaximized) || state.testFlag(Qt::WindowFullScreen) ||
      state.testFlag(Qt::WindowMinimized)) {
    return;
  }
  capWindowToScreen(window);
}

void keepWindowOnScreen(QWidget* window) {
  if (window == nullptr || !window->isWindow() || window->windowHandle() == nullptr ||
      window->findChild<QObject*>(QLatin1String(kKeptOnScreen), Qt::FindDirectChildrenOnly) != nullptr) {
    return;
  }
  // The connections end with the window, their context's parent.
  auto* context = new QObject(window);
  context->setObjectName(QLatin1String(kKeptOnScreen));
  const QPointer<QWidget> guard(window);
  const auto refit = [guard] {
    if (guard != nullptr && !guard->testAttribute(Qt::WA_DontShowOnScreen)) {
      capWindowToScreen(guard);
    }
  };
  QObject::connect(window->windowHandle(), &QWindow::screenChanged, context, refit);
  const auto watch = [context, refit](QScreen* screen) {
    QObject::connect(screen, &QScreen::availableGeometryChanged, context, refit);
  };
  for (QScreen* screen : QGuiApplication::screens()) {
    watch(screen);
  }
  QObject::connect(qGuiApp, &QGuiApplication::screenAdded, context, watch);
}

} // namespace mitcad
