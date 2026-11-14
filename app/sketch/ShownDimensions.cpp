// SPDX-License-Identifier: MIT
#include "ShownDimensions.hpp"

#include <algorithm>
#include <cmath>
#include <utility>

#include <QDebug>
#include <QEvent>
#include <QFontMetricsF>
#include <QMouseEvent>
#include <QPainter>
#include <QResizeEvent>
#include <QStringList>
#include <QToolTip>

#include "../OcctViewer.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/UiSizes.hpp"

namespace mitcad::sketch {

ShownDimensions::ShownDimensions(OcctViewer& viewer) : QWidget(&viewer), m_viewer(viewer) {
  setObjectName(QStringLiteral("shownSketchDimensions"));
  setAttribute(Qt::WA_TransparentForMouseEvents);
  setAttribute(Qt::WA_NoSystemBackground);
  setFocusPolicy(Qt::NoFocus);
  // The font of sketch mode's dimensions (SketchOverlay).
  QFont small = font();
  small.setPointSizeF(9.0 * uiTextScale());
  setFont(small);
  resize(viewer.size());
  hide();
  viewer.installEventFilter(this);
  // After the frame, which may have moved the camera.
  connect(
      &viewer, &OcctViewer::viewChanged, this,
      [this] {
        if (!m_sketches.empty()) {
          update();
        }
      },
      Qt::QueuedConnection);
  connect(&viewer, &OcctViewer::backgroundToneChanged, this, qOverload<>(&QWidget::update));
  m_logTimer.setSingleShot(true);
  m_logTimer.setInterval(150);
  connect(&m_logTimer, &QTimer::timeout, this, &ShownDimensions::logPlaces);
  TestSync::watch(&m_logTimer);
  connect(&viewer, &OcctViewer::cameraRested, this, [this] { m_logTimer.start(); });
}

void ShownDimensions::setSketches(std::vector<ShownSketch> sketches, double lengthScale, const QString& lengthUnit,
                                  int textMode) {
  m_sketches = std::move(sketches);
  m_lengthScale = lengthScale > 0.0 ? lengthScale : 1.0;
  m_lengthUnit = lengthUnit;
  m_textMode = textMode;
  QStringList shown;
  for (const ShownSketch& sketch : m_sketches) {
    shown << QStringLiteral("%1 (%2)").arg(sketch.name).arg(sketch.model.dimensions.size());
  }
  const QString line = shown.isEmpty() ? QStringLiteral("none") : shown.join(QStringLiteral(", "));
  if (line != m_loggedShown) {
    m_loggedShown = line;
    qDebug().noquote() << QStringLiteral("Sketch dimensions shown outside sketch mode: %1").arg(line);
  }
  if (m_sketches.empty()) {
    m_hovered.clear();
    hide();
  } else {
    resize(m_viewer.size());
    show();
  }
  update();
  m_logTimer.start();
}

Projection ShownDimensions::projection(const SketchModel& model) const {
  const geometry::Frame frame = model.frame;
  const OcctViewer* viewer = &m_viewer;
  const auto toScreen = [viewer, frame](const V2& p) { return viewer->toWidgetF(toModel(frame, p)); };
  // Millimetres per pixel in the sketch's plane, as SketchController::pixel.
  const QPointF o = toScreen(V2{});
  const QPointF x = toScreen(V2{1.0, 0.0}) - o;
  const QPointF y = toScreen(V2{0.0, 1.0}) - o;
  const double perMm = std::max(std::hypot(x.x(), x.y()), std::hypot(y.x(), y.y()));
  return {toScreen, perMm > 1e-9 ? 1.0 / perMm : 1.0};
}

std::vector<ShownDimensions::Layout> ShownDimensions::layouts() const {
  std::vector<Layout> result;
  const QFontMetricsF metrics(font());
  for (std::size_t s = 0; s < m_sketches.size(); ++s) {
    const SketchModel& model = m_sketches[s].model;
    const Projection proj = projection(model);
    for (std::size_t i = 0; i < model.dimensions.size(); ++i) {
      const DimensionData& d = model.dimensions[i];
      Layout layout;
      layout.sketch = s;
      layout.dimension = i;
      layout.shape = dimensionShape(d, model, proj, SketchOverlay::dimensionText(d, m_lengthScale, m_textMode));
      if (!layout.shape.valid) {
        continue;
      }
      layout.textRect = SketchOverlay::dimensionTextRect(metrics, layout.shape.textAt, layout.shape.text);
      result.push_back(std::move(layout));
    }
  }
  return result;
}

QString ShownDimensions::key(const Layout& layout) const {
  return QStringLiteral("%1 %2").arg(m_sketches[layout.sketch].uid,
                                     m_sketches[layout.sketch].model.dimensions[layout.dimension].id);
}

QString ShownDimensions::valueAt(const QPointF& position) const {
  if (m_sketches.empty()) {
    return QString();
  }
  // The last drawn is on top.
  const std::vector<Layout> all = layouts();
  for (auto it = all.rbegin(); it != all.rend(); ++it) {
    if (it->textRect.adjusted(-2, -2, 2, 2).contains(position)) {
      return key(*it);
    }
  }
  return QString();
}

void ShownDimensions::paintEvent(QPaintEvent* event) {
  Q_UNUSED(event);
  if (m_sketches.empty()) {
    return;
  }
  QPainter painter(this);
  painter.setRenderHint(QPainter::Antialiasing);
  const std::vector<Layout> all = layouts();
  // All the halos first, then the lines, so that a halo never cuts another
  // dimension's line (mitcad#633).
  const QColor halo = m_viewer.sketchHaloColor();
  for (const Layout& layout : all) {
    SketchOverlay::paintDimensionHalo(painter, layout.shape, halo);
  }
  const bool dark = m_viewer.backgroundDark();
  for (const Layout& layout : all) {
    const DimensionData& d = m_sketches[layout.sketch].model.dimensions[layout.dimension];
    const auto [color, lines] = SketchOverlay::dimensionColors(d.driven, !d.active, key(layout) == m_hovered, dark);
    SketchOverlay::paintDimension(painter, layout.shape, layout.textRect, color, lines, d.driven, !d.active);
  }
  m_logTimer.start();
}

void ShownDimensions::hover(const QPointF& position, const QPoint& global) {
  const QString now = valueAt(position);
  if (now == m_hovered) {
    return;
  }
  m_hovered = now;
  update();
  if (now.isEmpty()) {
    QToolTip::hideText();
    return;
  }
  const QStringList parts = now.split(QLatin1Char(' '));
  for (const ShownSketch& sketch : m_sketches) {
    if (sketch.uid != parts.value(0)) {
      continue;
    }
    if (const DimensionData* d = sketch.model.dimension(parts.value(1))) {
      const QString tip = SketchOverlay::dimensionTooltip(*d, m_lengthScale, m_lengthUnit);
      QToolTip::showText(global, tip, &m_viewer);
      qDebug().noquote() << QStringLiteral("Shown dimension hover: %1 %2: %3").arg(sketch.name, d->id, tip);
    }
  }
}

bool ShownDimensions::eventFilter(QObject* watched, QEvent* event) {
  if (watched != &m_viewer) {
    return QWidget::eventFilter(watched, event);
  }
  switch (event->type()) {
  case QEvent::Resize:
    resize(static_cast<QResizeEvent*>(event)->size());
    break;
  case QEvent::MouseMove: {
    const auto* e = static_cast<QMouseEvent*>(event);
    if (!m_sketches.empty() && e->buttons() == Qt::NoButton) {
      hover(e->position(), e->globalPosition().toPoint());
    }
    break;
  }
  case QEvent::Leave:
    if (!m_hovered.isEmpty()) {
      m_hovered.clear();
      QToolTip::hideText();
      update();
    }
    break;
  case QEvent::MouseButtonDblClick: {
    const auto* e = static_cast<QMouseEvent*>(event);
    if (e->button() != Qt::LeftButton || m_sketches.empty() || !m_editAllowed || !m_editAllowed()) {
      break;
    }
    const QString at = valueAt(e->position());
    if (at.isEmpty()) {
      break;
    }
    const QStringList parts = at.split(QLatin1Char(' '));
    qDebug().noquote() << QStringLiteral("Double-click on shown dimension %1 of %2").arg(parts.value(1), parts.value(0));
    m_hovered.clear();
    QToolTip::hideText();
    emit editRequested(parts.value(0), parts.value(1));
    return true;
  }
  default:
    break;
  }
  return QWidget::eventFilter(watched, event);
}

void ShownDimensions::logPlaces() {
  QStringList drawn;
  QStringList places;
  if (isVisible()) {
    const std::vector<Layout> all = layouts();
    QWidget* top = window();
    std::vector<int> counts(m_sketches.size(), 0);
    for (const Layout& layout : all) {
      ++counts[layout.sketch];
      const QPoint at = mapTo(top, layout.textRect.center().toPoint());
      places << QStringLiteral("Shown dimension %1 %2 at %3,%4")
                    .arg(m_sketches[layout.sketch].name,
                         m_sketches[layout.sketch].model.dimensions[layout.dimension].id)
                    .arg(at.x())
                    .arg(at.y());
    }
    for (std::size_t s = 0; s < m_sketches.size(); ++s) {
      if (counts[s] > 0) {
        drawn << QStringLiteral("%1 (%2)").arg(m_sketches[s].name).arg(counts[s]);
      }
    }
  }
  const QString line = drawn.isEmpty() ? QStringLiteral("none") : drawn.join(QStringLiteral(", "));
  if (line != m_loggedDrawn) {
    m_loggedDrawn = line;
    qDebug().noquote() << QStringLiteral("Sketch dimensions drawn outside sketch mode: %1").arg(line);
  }
  const QString joined = places.join(QLatin1Char('\n'));
  if (joined != m_loggedPlaces) {
    m_loggedPlaces = joined;
    for (const QString& place : std::as_const(places)) {
      qDebug().noquote() << place;
    }
  }
}

} // namespace mitcad::sketch
