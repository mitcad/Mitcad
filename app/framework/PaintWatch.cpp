// SPDX-License-Identifier: MIT
// Whether the widgets over the 3D view reach the screen (mitcad#2031): see
// PaintWatch.hpp.
#include "framework/PaintWatch.hpp"

#include <QCoreApplication>
#include <QEvent>
#include <QGuiApplication>
#include <QOpenGLWidget>
#include <QRect>
#include <QScreen>
#include <QStringList>
#include <QWidget>
#include <QWindow>
#include <QtLogging>

#include <algorithm>

namespace mitcad {

namespace {

QString scaleText(qreal ratio) { return QString::number(ratio, 'g', 3); }

// "the window exposed, platform wayland" for the window `widget` is in.
QString exposureText(const QWidget* widget) {
  const QWindow* handle = widget != nullptr ? widget->window()->windowHandle() : nullptr;
  return QStringLiteral("%1, platform %2")
      .arg(handle == nullptr        ? QStringLiteral("the window has no native window")
           : handle->isExposed()    ? QStringLiteral("the window exposed")
                                    : QStringLiteral("the window not exposed"),
           QGuiApplication::platformName());
}

// "12,48 1000x121": `widget`'s place in its window.
QString placeText(const QWidget* widget) {
  const QRect place(widget->mapTo(widget->window(), QPoint(0, 0)), widget->size());
  return QStringLiteral("%1,%2 %3x%4").arg(place.x()).arg(place.y()).arg(place.width()).arg(place.height());
}

} // namespace

PaintWatch::PaintWatch(QOpenGLWidget* view, QObject* parent) : QObject(parent), m_view(view) {
  m_check.setSingleShot(true);
  m_check.setInterval(kCheckMs);
  connect(&m_check, &QTimer::timeout, this, &PaintWatch::finish);
  if (view != nullptr) {
    // Emitted once the window has composed the view with the widgets and
    // flushed the frame (Qt's QOpenGLWidget).
    connect(view, &QOpenGLWidget::frameSwapped, this, &PaintWatch::framePresented);
  }
}

PaintWatch::~PaintWatch() {
  if (m_filtering && QCoreApplication::instance() != nullptr) {
    QCoreApplication::instance()->removeEventFilter(this);
  }
}

void PaintWatch::watch(QWidget* widget, const QString& name) {
  if (widget == nullptr) {
    return;
  }
  const bool known = std::any_of(m_entries.cbegin(), m_entries.cend(),
                                 [widget](const Entry& entry) { return entry.widget == widget; });
  if (!known) {
    Entry entry;
    entry.widget = widget;
    entry.name = name;
    m_entries.push_back(entry);
  }
}

void PaintWatch::arm(const QString& reason) {
  // After the layouts that the change queued (FloatingLayout's, the tool
  // bar's): what shows is placed by then.
  QTimer::singleShot(0, this, [this, reason] { start(reason); });
}

void PaintWatch::start(const QString& reason) {
  if (m_armed) {
    finish(); // the earlier round as far as it came
  }
  m_entries.erase(std::remove_if(m_entries.begin(), m_entries.end(),
                                 [](const Entry& entry) { return entry.widget.isNull(); }),
                  m_entries.end());
  m_reason = reason;
  m_armed = true;
  if (!m_filtering && QCoreApplication::instance() != nullptr) {
    QCoreApplication::instance()->installEventFilter(this);
    m_filtering = true;
  }
  for (Entry& entry : m_entries) {
    entry.painted = false;
    entry.presented = false;
    entry.expected = entry.widget->isVisible();
    if (entry.expected) {
      // Painted again whether or not its content changed.
      entry.widget->update();
    }
  }
  m_exposed = -1;
  logExposure();
  m_check.start();
  finishIfDone();
}

bool PaintWatch::eventFilter(QObject* watched, QEvent* event) {
  if (!m_armed) {
    return false;
  }
  if (event->type() == QEvent::Paint) {
    if (auto* widget = qobject_cast<QWidget*>(watched)) {
      painted(widget);
    }
  } else if (event->type() == QEvent::Expose && watched == viewWindow()) {
    logExposure();
  }
  return false;
}

void PaintWatch::painted(QWidget* widget) {
  // The innermost watched widget that holds it.
  for (const QWidget* at = widget; at != nullptr; at = at->parentWidget()) {
    const auto entry = std::find_if(m_entries.begin(), m_entries.end(),
                                    [at](const Entry& candidate) { return candidate.widget == at; });
    if (entry == m_entries.end()) {
      if (at->isWindow()) {
        return;
      }
      continue;
    }
    if (entry->painted) {
      return;
    }
    entry->painted = true;
    entry->expected = true;
    const QWidget* shown = entry->widget.data();
    qInfo().noquote() << QStringLiteral("Painted: %1 at %2, scale %3 (%4)")
                             .arg(entry->name, placeText(shown), scaleText(shown->devicePixelRatioF()),
                                  exposureText(shown));
    // A widget in a window of its own (a windowed card) is not in the
    // view's frame; without a view nothing is presented through it.
    if (m_view.isNull() || shown->window() != m_view->window()) {
      entry->presented = true;
      finishIfDone();
    }
    return;
  }
}

void PaintWatch::framePresented() {
  if (!m_armed) {
    return;
  }
  QStringList names;
  for (Entry& entry : m_entries) {
    if (entry.painted && !entry.presented) {
      entry.presented = true;
      names << entry.name;
    }
  }
  if (!names.isEmpty()) {
    qInfo().noquote() << QStringLiteral("Frame presented with: %1 (platform %2)")
                             .arg(names.join(QStringLiteral(", ")), QGuiApplication::platformName());
  }
  finishIfDone();
}

void PaintWatch::finishIfDone() {
  if (!m_armed) {
    return;
  }
  for (const Entry& entry : m_entries) {
    if (!entry.expected || entry.widget.isNull() || !entry.widget->isVisible()) {
      continue;
    }
    // Not painted while it has an unobscured part, or not presented yet.
    if (entry.painted ? !entry.presented : !entry.widget->visibleRegion().isEmpty()) {
      return;
    }
  }
  finish();
}

void PaintWatch::finish() {
  if (!m_armed) {
    return;
  }
  m_check.stop();
  m_armed = false;
  if (m_filtering && QCoreApplication::instance() != nullptr) {
    QCoreApplication::instance()->removeEventFilter(this);
    m_filtering = false;
  }
  QStringList presented;
  QStringList notPainted;
  QStringList notPresented;
  QStringList covered;
  for (const Entry& entry : m_entries) {
    if (!entry.expected || entry.widget.isNull() || !entry.widget->isVisible()) {
      continue; // not shown, or hidden meanwhile
    }
    if (!entry.painted && entry.widget->visibleRegion().isEmpty()) {
      // Shown, but other widgets or the window's edge leave nothing of it
      // to paint.
      covered << QStringLiteral("%1 (at %2)").arg(entry.name, placeText(entry.widget.data()));
    } else if (!entry.painted) {
      notPainted << QStringLiteral("%1 (shown at %2)").arg(entry.name, placeText(entry.widget.data()));
    } else if (!entry.presented) {
      notPresented << entry.name;
    } else {
      presented << entry.name;
    }
  }
  QStringList parts;
  if (!presented.isEmpty()) {
    parts << QStringLiteral("painted and presented: %1").arg(presented.join(QStringLiteral(", ")));
  }
  if (!notPainted.isEmpty()) {
    parts << QStringLiteral("not painted: %1").arg(notPainted.join(QStringLiteral(", ")));
  }
  if (!notPresented.isEmpty()) {
    parts << QStringLiteral("painted, not presented: %1").arg(notPresented.join(QStringLiteral(", ")));
  }
  if (!covered.isEmpty()) {
    parts << QStringLiteral("shown but covered: %1").arg(covered.join(QStringLiteral(", ")));
  }
  if (parts.isEmpty()) {
    parts << QStringLiteral("nothing shown");
  }
  qInfo().noquote() << QStringLiteral("Paint check %1: %2 (%3)")
                           .arg(m_reason, parts.join(QStringLiteral("; ")), windowText());
}

QWindow* PaintWatch::viewWindow() const {
  const QWidget* reference = m_view.data();
  if (reference == nullptr) {
    for (const Entry& entry : m_entries) {
      if (!entry.widget.isNull()) {
        reference = entry.widget.data();
        break;
      }
    }
  }
  return reference != nullptr ? reference->window()->windowHandle() : nullptr;
}

QString PaintWatch::windowText() const {
  const QWindow* handle = viewWindow();
  QString text = handle == nullptr        ? QStringLiteral("the window has no native window")
                 : handle->isExposed()    ? QStringLiteral("the window exposed")
                                          : QStringLiteral("the window not exposed");
  text += QStringLiteral(", platform %1").arg(QGuiApplication::platformName());
  if (handle != nullptr) {
    text += QStringLiteral(", scale %1").arg(scaleText(handle->devicePixelRatio()));
  }
  if (m_view.isNull()) {
    text += QStringLiteral("; no 3D view, a paint counts as presented");
  }
  return text;
}

void PaintWatch::logExposure() {
  const QWindow* handle = viewWindow();
  if (handle == nullptr) {
    return;
  }
  const int exposed = handle->isExposed() ? 1 : 0;
  if (exposed == m_exposed) {
    return;
  }
  m_exposed = exposed;
  QString screenText;
  if (const QScreen* screen = handle->screen()) {
    const QSize pixels = screen->geometry().size() * screen->devicePixelRatio();
    screenText = QStringLiteral(" on a %1x%2 screen").arg(pixels.width()).arg(pixels.height());
  }
  qInfo().noquote() << QStringLiteral("Window exposed: %1, %2x%3 at scale %4%5 (platform %6)")
                           .arg(exposed != 0 ? QStringLiteral("yes") : QStringLiteral("no"))
                           .arg(handle->width())
                           .arg(handle->height())
                           .arg(scaleText(handle->devicePixelRatio()), screenText, QGuiApplication::platformName());
}

} // namespace mitcad
