// SPDX-License-Identifier: MIT
#include "ParameterPicker.hpp"

#include <algorithm>
#include <cmath>
#include <exception>
#include <utility>

#include <QFontMetricsF>
#include <QJsonArray>
#include <QJsonObject>
#include <QMouseEvent>
#include <QPainter>
#include <QPointer>
#include <QStringList>
#include <QtLogging>

#include <Standard_Failure.hxx>

#include "../OcctViewer.hpp"
#include "CommandSession.hpp"
#include "Theme.hpp"

namespace mitcad {
namespace {

// A sketch's dimensions in the selection's blue, the colour of picking; the
// one under the pointer in the hover's, as in the 3D view (mitcad#1041). Their
// texts in light boxes: the light background's values.
const QColor kDimensionColor = viewColor(ColorRole::Select, false);
const QColor kHoverColor = viewColor(ColorRole::Hover, false);
const QColor kBoxColor = [] {
  QColor box = viewColor(ColorRole::Surface, false);
  box.setAlpha(235);
  return box;
}();
const QColor kTextColor = viewColor(ColorRole::SketchDimension, false);
const QColor kDrivenTextColor = viewColor(ColorRole::SketchDriven, false);
constexpr double kPadding = 4.0;

double screenDistance(const QPointF& a, const QPointF& b) { return std::hypot(a.x() - b.x(), a.y() - b.y()); }

QPointF unitOf(const QPointF& p) {
  const double n = std::hypot(p.x(), p.y());
  return n > 1e-12 ? QPointF(p.x() / n, p.y() / n) : QPointF(1, 0);
}

} // namespace

ParameterPicker::ParameterPicker(OcctViewer& viewer, const QString& command)
    : QWidget(&viewer), m_viewer(viewer), m_command(command) {
  setObjectName(QStringLiteral("parameterPicker"));
  setAttribute(Qt::WA_TransparentForMouseEvents);
  setAttribute(Qt::WA_NoSystemBackground);
  setFocusPolicy(Qt::NoFocus);
  QFont small = font();
  small.setPointSizeF(9.0);
  setFont(small);
  resize(viewer.size());
  show();
  raise();
  m_viewerFilter.install(&viewer);
  connect(&viewer, &OcctViewer::viewChanged, this, [this] {
    update();
    logPlaces();
  });
}

ParameterPicker::~ParameterPicker() = default;

void ParameterPicker::setContent(std::vector<Sketch> sketches, QHash<QString, QString> texts) {
  m_sketches = std::move(sketches);
  m_texts = std::move(texts);
  m_hover = -1;
  QStringList shown;
  for (const Label& label : layout()) {
    shown << QStringLiteral("%1 %2").arg(label.source, label.name);
  }
  qDebug().noquote() << QStringLiteral("Parameter labels for %1: %2")
                            .arg(m_command, shown.isEmpty() ? QStringLiteral("none") : shown.join(QStringLiteral(", ")));
  // Without labels, not over the view at all: Qt paints a widget over the
  // view again at each frame the view draws, an empty one as much as any
  // (mitcad#768).
  setVisible(!shown.isEmpty());
  m_logged.clear();
  update();
  logPlaces();
}

std::vector<ParameterPicker::Label> ParameterPicker::layout() const {
  std::vector<Label> labels;
  const QFontMetricsF metrics(font());
  const auto box = [&metrics](const QPointF& at, const QString& text) {
    const double across = metrics.horizontalAdvance(text) + 2.0 * kPadding;
    const double tall = metrics.height() + kPadding;
    return QRectF(at.x() - across / 2.0, at.y() - tall / 2.0, across, tall);
  };
  for (const Sketch& s : m_sketches) {
    const sketch::SketchModel& model = s.model;
    const gp_Trsf placement = s.placement;
    const OcctViewer& viewer = m_viewer;
    const auto toScreen = [&viewer, &model, placement](const sketch::V2& p) {
      return viewer.toWidgetF(sketch::toModel(model.frame, p).Transformed(placement));
    };
    const QPointF origin = toScreen(sketch::V2{0.0, 0.0});
    const double perMm = std::max(screenDistance(toScreen(sketch::V2{1.0, 0.0}), origin),
                                  screenDistance(toScreen(sketch::V2{0.0, 1.0}), origin));
    const sketch::Projection projection{toScreen, perMm > 1e-9 ? 1.0 / perMm : 1.0};
    for (const sketch::DimensionData& d : model.dimensions) {
      if (d.parameter.isEmpty()) {
        continue;
      }
      Label label;
      label.name = d.parameter;
      label.source = QStringLiteral("%1 %2").arg(model.name, d.id);
      label.driven = d.driven;
      label.text = QStringLiteral("%1 = %2").arg(d.parameter, m_texts.value(d.parameter));
      if (d.driven) {
        label.text = QStringLiteral("(%1)").arg(label.text);
      }
      label.shape = sketch::dimensionShape(d, model, projection, label.text);
      if (!label.shape.valid) {
        // Only the value then, where the dimension shows it.
        label.shape.textAt = toScreen(d.text ? *d.text : sketch::defaultTextPosition(d, model, projection.pixel));
      }
      label.rect = box(label.shape.textAt, label.text);
      labels.push_back(std::move(label));
    }
  }
  return labels;
}

int ParameterPicker::labelAt(const QPointF& position) const {
  const std::vector<Label> labels = layout();
  // The one drawn last is on top.
  for (int i = static_cast<int>(labels.size()) - 1; i >= 0; --i) {
    if (labels[static_cast<std::size_t>(i)].rect.adjusted(-2, -2, 2, 2).contains(position)) {
      return i;
    }
  }
  return -1;
}

bool ParameterPicker::eventFilter(QObject* watched, QEvent* event) {
  if (watched != &m_viewer) {
    return QWidget::eventFilter(watched, event);
  }
  // An exception must not leave a Qt event handler: geometry that cannot be
  // drawn takes no clicks.
  try {
    switch (event->type()) {
    case QEvent::Resize:
      resize(m_viewer.size());
      return false;
    case QEvent::MouseButtonPress:
    case QEvent::MouseButtonDblClick: {
      auto* e = static_cast<QMouseEvent*>(event);
      if (e->button() != Qt::LeftButton) {
        return false;
      }
      const int at = labelAt(e->position());
      if (at < 0) {
        return false;
      }
      m_pressed = true;
      if (event->type() == QEvent::MouseButtonPress) {
        const Label label = layout()[static_cast<std::size_t>(at)];
        qDebug().noquote() << QStringLiteral("Parameter label picked: %1 (%2)").arg(label.name, label.source);
        const QPointer<ParameterPicker> alive(this);
        emit picked(label.name, label.source);
        if (!alive) {
          return true;
        }
      }
      return true;
    }
    case QEvent::MouseButtonRelease: {
      auto* e = static_cast<QMouseEvent*>(event);
      if (e->button() == Qt::LeftButton && m_pressed) {
        m_pressed = false;
        return true;
      }
      return false;
    }
    case QEvent::MouseMove: {
      auto* e = static_cast<QMouseEvent*>(event);
      const int at = labelAt(e->position());
      if (at != m_hover) {
        m_hover = at;
        update();
      }
      return m_pressed;
    }
    default:
      return false;
    }
  } catch (const Standard_Failure&) {
    return false;
  } catch (const std::exception&) {
    return false;
  }
}

void ParameterPicker::paintEvent(QPaintEvent*) {
  QPainter painter(this);
  painter.setRenderHint(QPainter::Antialiasing);
  std::vector<Label> labels;
  try {
    labels = layout();
  } catch (const Standard_Failure&) {
    return;
  } catch (const std::exception&) {
    return;
  }
  for (std::size_t i = 0; i < labels.size(); ++i) {
    const Label& label = labels[i];
    const bool hover = static_cast<int>(i) == m_hover;
    const QColor color = hover ? kHoverColor : kDimensionColor;
    if (label.shape.valid) {
      painter.setPen(QPen(color, 1.0));
      painter.setBrush(Qt::NoBrush);
      for (const QLineF& line : label.shape.lines) {
        painter.drawLine(line);
      }
      for (const QPolygonF& arc : label.shape.arcs) {
        painter.drawPolyline(arc);
      }
      painter.setBrush(color);
      for (const auto& [tip, towards] : label.shape.arrows) {
        const QPointF along = unitOf(towards);
        const QPointF across(-along.y(), along.x());
        QPolygonF head;
        head << tip << tip - along * 8.0 + across * 3.0 << tip - along * 8.0 - across * 3.0;
        painter.drawPolygon(head);
      }
    }
    painter.setPen(QPen(color, hover ? 2.0 : 1.0));
    painter.setBrush(kBoxColor);
    painter.drawRoundedRect(label.rect, 3.0, 3.0);
    painter.setPen(label.driven ? kDrivenTextColor : kTextColor);
    painter.drawText(label.rect, Qt::AlignCenter, label.text);
  }
}

void ParameterPicker::logPlaces() {
  if (!isVisible()) {
    return;
  }
  QStringList lines;
  try {
    for (const Label& label : layout()) {
      const QPoint at = mapTo(window(), label.rect.center().toPoint());
      lines << QStringLiteral("Parameter label %1 %2 at %3,%4").arg(label.source, label.name).arg(at.x()).arg(at.y());
    }
  } catch (const Standard_Failure&) {
    return;
  } catch (const std::exception&) {
    return;
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged != m_logged) {
    m_logged = logged;
    for (const QString& line : lines) {
      qDebug().noquote() << line;
    }
  }
}

void gatherParameterLabels(const CommandHost& host, std::vector<ParameterPicker::Sketch>& sketches,
                           QHash<QString, QString>& texts) {
  // Every parameter's value in its unit.
  try {
    for (const QJsonValue& entry : host.queryArray(QStringLiteral("parameters"))) {
      const QJsonObject p = entry.toObject();
      texts.insert(p.value(QStringLiteral("name")).toString(), p.value(QStringLiteral("text")).toString());
    }
  } catch (const std::exception&) {
    return;
  }

  // The dimensions of the sketches shown.
  for (const CommandHost::ShownSketch& shown : host.shownSketches()) {
    ParameterPicker::Sketch entry;
    try {
      if (!entry.model.load(host.queryObject(
              {{QStringLiteral("query"), QStringLiteral("sketch")}, {QStringLiteral("uid"), shown.uid}}))) {
        continue;
      }
    } catch (const std::exception&) {
      continue; // a sketch that was just taken back
    }
    const auto& dimensions = entry.model.dimensions;
    if (std::none_of(dimensions.begin(), dimensions.end(),
                     [](const sketch::DimensionData& d) { return !d.parameter.isEmpty(); })) {
      continue;
    }
    entry.placement = shown.placement;
    sketches.push_back(std::move(entry));
  }
}

} // namespace mitcad
