// SPDX-License-Identifier: MIT
#include "FlexRope.hpp"

#include <algorithm>
#include <cmath>
#include <utility>

#include <QEvent>
#include <QFontMetricsF>
#include <QPaintEvent>
#include <QPainter>
#include <QPen>
#include <QPolygonF>

#include <gp_Vec.hxx>

#include "../OcctViewer.hpp"
#include "../framework/Theme.hpp"
#include "../framework/UiSizes.hpp"

namespace mitcad::flex {
namespace {

gp_Dir directionOf(const Xyz& local, const gp_Trsf& placement, const gp_Dir& fallback) {
  gp_Vec vector(local[0], local[1], local[2]);
  vector.Transform(placement);
  return vector.Magnitude() > 1e-12 ? gp_Dir(vector) : fallback;
}

gp_Pnt pointOf(const Xyz& local, const gp_Trsf& placement) {
  gp_Pnt point(local[0], local[1], local[2]);
  point.Transform(placement);
  return point;
}

// A colour between two, `share` of the way from the first to the second.
QColor between(const QColor& a, const QColor& b, double share) {
  const double t = std::clamp(share, 0.0, 1.0);
  const auto channel = [t](int from, int to) { return qRound(from + (to - from) * t); };
  return QColor::fromRgb(channel(a.red(), b.red()), channel(a.green(), b.green()), channel(a.blue(), b.blue()));
}

QPointF unitOf(const QPointF& v, const QPointF& fallback) {
  const double size = std::hypot(v.x(), v.y());
  return size > 1e-9 ? v / size : fallback;
}

} // namespace

std::vector<RopeSample> ropeSamples(const FlexItem& item, const gp_Trsf& placement) {
  std::vector<RopeSample> samples;
  if (item.frames.size() >= 2) {
    samples.reserve(item.frames.size());
    for (const FlexFrame& frame : item.frames) {
      RopeSample sample;
      sample.point = pointOf(frame.point, placement);
      sample.tangent = directionOf(frame.tangent, placement, gp_Dir(0, 0, 1));
      sample.normal = directionOf(frame.normal, placement, gp_Dir(0, 1, 0));
      samples.push_back(sample);
    }
    return samples;
  }
  const std::size_t count = item.polyline.size();
  samples.reserve(count);
  for (std::size_t i = 0; i < count; ++i) {
    const Xyz& before = item.polyline[i > 0 ? i - 1 : i];
    const Xyz& after = item.polyline[i + 1 < count ? i + 1 : i];
    const Xyz tangent = unit(sub(after, before));
    const Xyz side = std::abs(tangent[2]) < 0.9 ? Xyz{0.0, 0.0, 1.0} : Xyz{1.0, 0.0, 0.0};
    RopeSample sample;
    sample.point = pointOf(item.polyline[i], placement);
    sample.tangent = directionOf(tangent, placement, gp_Dir(0, 0, 1));
    sample.normal = directionOf(cross(cross(tangent, side), tangent), placement, gp_Dir(0, 1, 0));
    samples.push_back(sample);
  }
  return samples;
}

RopeLook ropeLook(const FlexItem& item, const QColor& color) {
  RopeLook look;
  look.round = item.round;
  look.width = item.width;
  look.height = item.height;
  look.color = color;
  return look;
}

double ropeHalfPixels(const RopeSample& sample, const RopeLook& look, const RopeView& view) {
  const double reach = look.round ? look.width : std::max(look.width, look.height);
  const double size = view.pixelSize ? view.pixelSize(sample.point) : 1.0;
  if (!std::isfinite(size) || size <= 0.0) {
    return 1.5;
  }
  return std::max(1.5, 0.5 * reach / size);
}

void paintFlexRope(QPainter& painter, const std::vector<RopeSample>& samples, const RopeLook& look,
                   const RopeView& view) {
  if (samples.size() < 2 || !view.project || !look.color.isValid()) {
    return;
  }
  painter.save();
  painter.setRenderHint(QPainter::Antialiasing);
  const QColor shade = look.color.darker(150);
  const QColor light = look.color.lighter(125);
  const QColor outline = look.color.darker(190);
  const std::size_t count = samples.size();

  if (!look.round && look.width > 0.0) {
    // A strip between the section's two edges, piece by piece: each piece
    // lighter the more it faces the viewer, so that a twist shows as the
    // strip turning over.
    QPolygonF left;
    QPolygonF right;
    for (const RopeSample& sample : samples) {
      const gp_Vec across = gp_Vec(sample.normal).Crossed(gp_Vec(sample.tangent)) * (0.5 * look.width);
      left << view.project(sample.point.Translated(across));
      right << view.project(sample.point.Translated(-across));
    }
    for (std::size_t i = 0; i + 1 < count; ++i) {
      const double facing = std::abs(gp_Vec(samples[i].normal).Dot(gp_Vec(view.towardViewer)));
      const QColor fill = between(shade, light, facing);
      const qsizetype k = static_cast<qsizetype>(i);
      QPolygonF piece;
      piece << left[k] << left[k + 1] << right[k + 1] << right[k];
      painter.setPen(QPen(fill, 0.8)); // closes the hairline between two pieces
      painter.setBrush(fill);
      painter.drawPolygon(piece);
    }
    painter.setBrush(Qt::NoBrush);
    painter.setPen(QPen(outline, 1.0));
    painter.drawPolyline(left);
    painter.drawPolyline(right);
    painter.restore();
    return;
  }

  // A tube: its outline as wide as the section shows at each sample, a
  // lit band along it and a thin highlight, towards the upper left.
  std::vector<QPointF> centres(count);
  std::vector<double> halves(count);
  for (std::size_t i = 0; i < count; ++i) {
    centres[i] = view.project(samples[i].point);
    halves[i] = ropeHalfPixels(samples[i], look, view);
  }
  QPolygonF left;
  QPolygonF right;
  QPointF along(1.0, 0.0);
  for (std::size_t i = 0; i < count; ++i) {
    along = unitOf(centres[i + 1 < count ? i + 1 : i] - centres[i > 0 ? i - 1 : i], along);
    const QPointF across(-along.y(), along.x());
    left << centres[i] + across * halves[i];
    right << centres[i] - across * halves[i];
  }
  QPolygonF body = left;
  for (qsizetype i = right.size() - 1; i >= 0; --i) {
    body << right[i];
  }
  painter.setPen(QPen(outline, 1.0));
  painter.setBrush(shade);
  painter.drawPolygon(body, Qt::WindingFill);
  painter.drawEllipse(centres.front(), halves.front(), halves.front());
  painter.drawEllipse(centres.back(), halves.back(), halves.back());
  const QPointF lit(-0.22, -0.22); // of the half width, towards the light
  for (std::size_t i = 0; i + 1 < count; ++i) {
    const double half = 0.5 * (halves[i] + halves[i + 1]);
    painter.setPen(QPen(look.color, std::max(1.0, 1.2 * half), Qt::SolidLine, Qt::RoundCap));
    painter.drawLine(centres[i] + lit * half, centres[i + 1] + lit * half);
  }
  for (std::size_t i = 0; i + 1 < count; ++i) {
    const double half = 0.5 * (halves[i] + halves[i + 1]);
    if (half < 3.0) {
      continue; // too thin for a highlight of its own
    }
    painter.setPen(QPen(light, 0.35 * half, Qt::SolidLine, Qt::RoundCap));
    painter.drawLine(centres[i] + lit * (2.0 * half), centres[i + 1] + lit * (2.0 * half));
  }
  painter.restore();
}

void paintDashedLine(QPainter& painter, const QPointF& from, const QPointF& to, const QColor& color,
                     const QColor& halo) {
  painter.save();
  painter.setRenderHint(QPainter::Antialiasing);
  QColor under = halo;
  under.setAlpha(180);
  painter.setPen(QPen(under, 3.0, Qt::SolidLine, Qt::RoundCap));
  painter.drawLine(from, to);
  QPen dash(color, 1.25, Qt::CustomDashLine, Qt::FlatCap);
  dash.setDashPattern({5.0, 4.0});
  painter.setPen(dash);
  painter.drawLine(from, to);
  painter.restore();
}

FlexRopeOverlay::FlexRopeOverlay(OcctViewer& viewer, std::function<double(const gp_Pnt&)> pixelSize)
    : QWidget(&viewer), m_viewer(viewer), m_pixelSize(std::move(pixelSize)) {
  setObjectName(QStringLiteral("flexRopeOverlay"));
  setAttribute(Qt::WA_TransparentForMouseEvents);
  setAttribute(Qt::WA_NoSystemBackground);
  setFocusPolicy(Qt::NoFocus);
  QFont small = font();
  small.setPointSizeF(9.0 * uiTextScale());
  setFont(small);
  setGeometry(viewer.rect());
  viewer.installEventFilter(this);
  connect(&viewer, &OcctViewer::viewChanged, this, [this] {
    if (m_item || m_line) {
      update();
    }
  });
  hide();
}

void FlexRopeOverlay::show(const std::optional<FlexItem>& item, const gp_Trsf& placement, const QString& text) {
  m_item = item;
  m_placement = placement;
  m_text = text;
  setGeometry(m_viewer.rect());
  setVisible(m_item.has_value() || m_line.has_value());
  if (isVisible()) {
    raise();
    update();
  }
}

void FlexRopeOverlay::showLine(const std::optional<std::pair<gp_Pnt, gp_Pnt>>& line) {
  m_line = line;
  setGeometry(m_viewer.rect());
  setVisible(m_item.has_value() || m_line.has_value());
  if (isVisible()) {
    raise();
    update();
  }
}

bool FlexRopeOverlay::eventFilter(QObject* watched, QEvent* event) {
  if (watched == &m_viewer && event->type() == QEvent::Resize) {
    setGeometry(m_viewer.rect());
  }
  return QWidget::eventFilter(watched, event);
}

void FlexRopeOverlay::paintEvent(QPaintEvent* /*event*/) {
  if (!m_item && !m_line) {
    return;
  }
  QPainter painter(this);
  painter.setRenderHint(QPainter::Antialiasing);
  const QColor halo = m_viewer.sketchHaloColor();
  const bool dark = halo.lightnessF() < 0.5;
  if (m_line) {
    paintDashedLine(painter, m_viewer.toWidgetF(m_line->first), m_viewer.toWidgetF(m_line->second),
                    viewColor(ColorRole::SketchDimension, dark), halo);
  }
  if (!m_item) {
    return;
  }
  RopeView view;
  view.project = [this](const gp_Pnt& point) { return m_viewer.toWidgetF(point); };
  view.pixelSize = [this](const gp_Pnt& point) { return m_pixelSize ? m_pixelSize(point) : 1.0; };
  view.towardViewer = m_viewer.viewDirection().Reversed();
  const std::vector<RopeSample> samples = ropeSamples(*m_item, m_placement);
  // The item has no body yet: in the colour the view draws bodies in.
  paintFlexRope(painter, samples, ropeLook(*m_item, viewColor(ColorRole::Body, dark)), view);
  if (!m_text.isEmpty() && samples.size() >= 2) {
    const QFontMetricsF metrics(font());
    QRectF box = metrics.boundingRect(m_text);
    box.adjust(-5.0, -3.0, 5.0, 3.0);
    box.moveTopLeft(view.project(samples.back().point) + QPointF(16.0, 14.0));
    painter.setPen(Qt::NoPen);
    painter.setBrush(QColor(halo.red(), halo.green(), halo.blue(), 235));
    painter.drawRoundedRect(box, 3.0, 3.0);
    painter.setPen(viewColor(ColorRole::SketchDimension, dark));
    painter.drawText(box, Qt::AlignCenter, m_text);
  }
}

} // namespace mitcad::flex
