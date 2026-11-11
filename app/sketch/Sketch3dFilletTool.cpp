// SPDX-License-Identifier: MIT
// A 3D sketch's Fillet (mitcad#1548): the SKETCH tab's Fillet in a 3D
// sketch. A corner where two lines meet is picked by its point (a press on
// it) or by its two lines (a click on each); with a Radius typed the pick
// rounds it at once, otherwise the radius follows the pointer along the
// corner's bisector until a click (or the release of a drag from the
// corner) takes it. The arc is previewed live; each corner is one
// `sketch3d.fillet` and one undo step, and the tool stays for the next
// corner with the typed radius kept. Esc drops the picked corner, then
// ends the tool. The geometry is Sketch3dFillet.hpp's.

#include "Sketch3dController.hpp"

#include <algorithm>
#include <cmath>

#include <QJsonArray>
#include <QLineF>
#include <QtLogging>

#include "../OcctViewer.hpp"
#include "SketchController.hpp"

namespace mitcad::sketch {
namespace {

constexpr double kFilletPixels = 10.0; // a corner's or a line's reach on the screen

// How far `pointer` is from the segment `a`-`b` on the screen.
double pixelsToSegment(const QPointF& a, const QPointF& b, const QPointF& pointer) {
  const QPointF along = b - a;
  const double squared = QPointF::dotProduct(along, along);
  double f = squared > 1e-12 ? QPointF::dotProduct(pointer - a, along) / squared : 0.0;
  f = std::clamp(f, 0.0, 1.0);
  return QLineF(a + along * f, pointer).length();
}

// "p2 of c3 and c5": the logs' words, which UI tests read.
QString cornerWords(const FilletCorner3& corner) {
  return QStringLiteral("%1 of %2 and %3").arg(corner.point, corner.first, corner.second);
}

} // namespace

std::optional<double> Sketch3dController::filletRadius() const {
  if (m_radius.edited) {
    return m_radius.valid ? std::optional<double>(m_radius.value) : std::nullopt;
  }
  return m_filletCorner ? m_filletDragged : std::nullopt;
}

void Sketch3dController::updateFilletHover() {
  m_filletHover.reset();
  m_filletHoverLine.clear();
  m_hoverAt.reset();
  m_hoverSnap = SnapTarget();
  m_unstable = false;
  if (m_pointerInView && m_model.hasFrame) {
    // The nearest corner on the screen, else the nearest line.
    double nearest = kFilletPixels;
    for (const QString& id : filletCornerPoints(m_model)) {
      const Sketch3dPoint* point = m_model.point(id);
      const double pixels = point != nullptr ? QLineF(onScreen(point->at), m_pointer).length() : 1e300;
      if (pixels <= nearest) {
        nearest = pixels;
        m_filletHover = filletCornerAt(m_model, id);
      }
    }
    if (!m_filletHover) {
      nearest = kFilletPixels;
      for (const Sketch3dCurve& curve : m_model.curves) {
        if (curve.type != QStringLiteral("line")) {
          continue;
        }
        const Sketch3dPoint* a = m_model.point(curve.json.value(QStringLiteral("start")).toString());
        const Sketch3dPoint* b = m_model.point(curve.json.value(QStringLiteral("end")).toString());
        if (a == nullptr || b == nullptr) {
          continue;
        }
        const double pixels = pixelsToSegment(onScreen(a->at), onScreen(b->at), m_pointer);
        if (pixels <= nearest) {
          nearest = pixels;
          m_filletHoverLine = curve.id;
        }
      }
      // A second line that meets the first one picked: their corner.
      if (!m_filletLine.isEmpty() && !m_filletHoverLine.isEmpty() && m_filletHoverLine != m_filletLine) {
        m_filletHover = filletCornerOfLines(m_model, m_filletLine, m_filletHoverLine);
      }
    }
  }
  // The radius the pointer drags at the picked corner.
  m_filletDragged.reset();
  if (m_filletCorner) {
    if (const auto ray = rayAt(m_pointer)) {
      m_filletDragged = filletRadiusOnRay(*m_filletCorner, *ray);
    }
  }
  // Why the next click would be refused.
  m_problem.clear();
  const FilletCorner3* target = m_filletCorner ? &*m_filletCorner : (m_filletHover ? &*m_filletHover : nullptr);
  if (m_radius.edited && !m_radius.valid) {
    m_problem = tr("Radius: %1").arg(m_radius.problem);
  } else if (target != nullptr) {
    if (const auto radius = filletRadius()) {
      QString why;
      if (!filletArc(m_model, *target, *radius, &why)) {
        m_problem = why;
      }
    }
  } else if (!m_filletLine.isEmpty() && !m_filletHoverLine.isEmpty() && m_filletHoverLine != m_filletLine) {
    m_problem = filletLinesProblem(m_model, m_filletLine, m_filletHoverLine);
  }
  QString hovered;
  if (m_filletHover) {
    hovered = QStringLiteral("corner ") + cornerWords(*m_filletHover);
  } else if (!m_filletHoverLine.isEmpty()) {
    hovered = QStringLiteral("line ") + m_filletHoverLine;
  }
  if (hovered != m_loggedFillet) {
    m_loggedFillet = hovered;
    if (!hovered.isEmpty()) {
      toolLog(QStringLiteral("3D sketch Fillet: hover %1").arg(hovered));
    }
  }
  if (m_problem != m_loggedProblem) {
    m_loggedProblem = m_problem;
    if (!m_problem.isEmpty()) {
      toolLog(QStringLiteral("3D sketch problem: %1").arg(m_problem));
    }
  }
  m_viewer.setSketchSnapping(m_filletHover.has_value() || !m_filletHoverLine.isEmpty());
  emit toolStateChanged();
  updateOverlay();
}

void Sketch3dController::filletPress() {
  m_filletPressPicked = false;
  if (m_filletCorner || !m_filletHover) {
    return; // a picked corner: the release rounds it
  }
  m_filletCorner = m_filletHover;
  m_filletLine.clear();
  m_filletPressPicked = true;
  toolLog(QStringLiteral("3D sketch Fillet: corner %1 picked").arg(cornerWords(*m_filletCorner)));
  updateFilletHover();
}

void Sketch3dController::filletRelease(bool click) {
  if (m_filletCorner) {
    const bool picked = m_filletPressPicked;
    m_filletPressPicked = false;
    if (picked && click && !m_radius.edited) {
      // Picked by this click: the radius follows the pointer now.
      toolLog(QStringLiteral("3D sketch Fillet: drag or type the radius at %1").arg(m_filletCorner->point));
      m_host.showHint(filletHint());
      emit toolStateChanged();
      return;
    }
    commitFillet(); // a typed radius, a drag from the corner, or the next click
    return;
  }
  m_filletPressPicked = false;
  if (!click) {
    return;
  }
  if (!m_filletHoverLine.isEmpty()) {
    if (m_filletLine.isEmpty() || m_filletLine == m_filletHoverLine) {
      m_filletLine = m_filletHoverLine;
      toolLog(QStringLiteral("3D sketch Fillet: first line %1; pick the second").arg(m_filletLine));
    } else {
      // Two lines that make no corner (a corner is picked on the press).
      const QString why = filletLinesProblem(m_model, m_filletLine, m_filletHoverLine);
      m_host.showError(why);
      toolLog(QStringLiteral("3D sketch Fillet: refused: %1").arg(why));
    }
    updateFilletHover();
    return;
  }
  m_host.showHint(filletHint());
  toolLog(QStringLiteral("3D sketch Fillet: no corner or line at %1,%2")
              .arg(qRound(m_pointer.x()))
              .arg(qRound(m_pointer.y())));
}

bool Sketch3dController::confirmFillet() {
  if (!m_filletCorner && m_filletHover && m_radius.edited) {
    m_filletCorner = m_filletHover; // the corner under the pointer, the radius typed
  }
  if (!m_filletCorner) {
    m_host.showHint(filletHint());
    return false;
  }
  commitFillet();
  return true;
}

bool Sketch3dController::dropFillet() {
  const bool had = m_filletCorner.has_value() || !m_filletLine.isEmpty();
  m_filletCorner.reset();
  m_filletLine.clear();
  m_filletDragged.reset();
  m_filletPressPicked = false;
  return had;
}

void Sketch3dController::commitFillet() {
  if (!m_filletCorner) {
    return;
  }
  const FilletCorner3 corner = *m_filletCorner;
  const std::optional<double> radius = filletRadius();
  QString why;
  if (!radius) {
    why = m_radius.edited ? tr("Radius: %1").arg(m_radius.problem)
                          : tr("Move the pointer into the corner to drag the radius, or type the Radius.");
  } else {
    filletArc(m_model, corner, *radius, &why);
  }
  if (!why.isEmpty()) {
    m_host.showError(why);
    toolLog(QStringLiteral("3D sketch Fillet: refused: %1").arg(why));
    return; // the corner stays picked
  }
  // A typed radius goes as typed (a value, an expression, a parameter's
  // name); a dragged one in millimetres, to the hundredth of the unit the
  // field shows.
  QJsonValue value;
  if (m_radius.edited) {
    value = m_radius.expression;
  } else {
    const double unitMm = millimetres() > 0.0 ? millimetres() : 1.0;
    const double rounded = std::round(*radius / unitMm * 100.0) / 100.0 * unitMm;
    const bool fits = rounded > 0.0 && filletArc(m_model, corner, rounded, nullptr).has_value();
    value = fits ? rounded : *radius;
  }
  QJsonObject result;
  const bool done = commitSteps(filletCommandFor(m_uid, corner, value), {},
                                QStringLiteral("Fillet in %1").arg(m_model.name), &result);
  // The tool stays for the next corner, the typed radius with it.
  dropFillet();
  if (done) {
    const QJsonObject made = result.value(QStringLiteral("fillets")).toArray().first().toObject();
    toolLog(QStringLiteral("3D sketch Fillet: made %1 at %2, radius %3")
                .arg(made.value(QStringLiteral("arc")).toString(), made.value(QStringLiteral("corner")).toString(),
                     QString::number(value.isDouble() ? value.toDouble() : *radius, 'f', 3)));
  }
  updateFilletHover();
}

QString Sketch3dController::filletHint() const {
  if (m_filletCorner) {
    return tr("Fillet: drag the radius along the corner's bisector and click, or type the Radius and press "
              "Enter.");
  }
  if (!m_filletLine.isEmpty()) {
    return tr("Fillet: click a second line that meets %1.").arg(entityName(m_model, m_filletLine));
  }
  return tr("Fillet: click a corner where two lines meet, or the two lines; with a Radius typed the click rounds "
            "it at once. Esc ends the tool.");
}

void Sketch3dController::addFilletPreview(Sketch3dPreview& out) const {
  using Style = Sketch3dPreview::Style;
  const auto path = [&out](std::vector<Xyz> points, Style style) {
    Sketch3dPreview::Path p;
    p.points = std::move(points);
    p.style = style;
    out.paths.push_back(std::move(p));
  };
  // The first line picked of two.
  if (!m_filletLine.isEmpty()) {
    if (const Sketch3dCurve* line = m_model.curve(m_filletLine)) {
      const Sketch3dPoint* a = m_model.point(line->json.value(QStringLiteral("start")).toString());
      const Sketch3dPoint* b = m_model.point(line->json.value(QStringLiteral("end")).toString());
      if (a != nullptr && b != nullptr) {
        path({a->at, b->at}, Style::Guide);
      }
    }
  }
  const FilletCorner3* corner = m_filletCorner ? &*m_filletCorner : (m_filletHover ? &*m_filletHover : nullptr);
  out.conflict = !m_problem.isEmpty();
  if (corner == nullptr) {
    return;
  }
  out.hover = corner->at;
  out.snapped = true;
  out.badge = tr("Corner %1").arg(entityName(m_model, corner->point)); // in words (mitcad#1653)
  if (m_filletCorner) {
    out.placed.push_back(corner->at);
  }
  const std::optional<double> radius = filletRadius();
  if (!radius) {
    return;
  }
  if (const auto arc = filletArc(m_model, *corner, *radius, nullptr)) {
    // The arc, the pieces of the lines it takes away and a radius.
    path(filletArcPoints(*arc, 48), Style::Draw);
    path({arc->start, corner->at, arc->end}, Style::Guide);
    path({arc->center, arc->start}, Style::Guide);
    out.badge = tr("Corner %1: radius %2 %3")
                    .arg(entityName(m_model, corner->point), formatLength(*radius, millimetres()), unit());
  }
}

} // namespace mitcad::sketch
