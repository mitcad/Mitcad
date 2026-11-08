// SPDX-License-Identifier: MIT
// CommandSession: the provisional drag presentation (mitcad#636,
// framework/DragProxy.hpp, CommandDef::dragProxy). While a handle is
// dragged, the shown exact preview's tool is changed to each dragged value
// at once and drawn provisional; the exact preview, computed on the
// model's worker as before (the bounded latest-input scheduling of
// mitcad#634, the interactive mesh tier of mitcad#635), replaces it once
// its value is the one dragged to, and stays what OK commits. The rest of
// the session is in CommandSession.cpp.
#include <algorithm>
#include <cmath>
#include <utility>
#include <vector>

#include <BRepBndLib.hxx>
#include <BRep_Builder.hxx>
#include <Bnd_Box.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS_Compound.hxx>

#include <QtLogging>

#include "CommandSession.hpp"
#include "Diagnostics.hpp"
#include "ManipulatorOverlay.hpp"

namespace mitcad {
namespace {

// A value's text for the log: millimetres.
QString millimetres(double value) {
  return QStringLiteral("%1 mm").arg(std::abs(value) < 1e-12 ? 0.0 : value, 0, 'g', 10);
}

// The time between consecutive events (ms), as median and 95th percentile
// (nearest rank); zero for fewer than two.
std::pair<double, double> intervals(const std::vector<qint64>& at) {
  std::vector<double> gaps;
  for (std::size_t i = 1; i < at.size(); ++i) {
    gaps.push_back(static_cast<double>(at[i] - at[i - 1]));
  }
  if (gaps.empty()) {
    return {0.0, 0.0};
  }
  std::sort(gaps.begin(), gaps.end());
  const auto rank = [&gaps](double fraction) {
    const auto index = static_cast<std::size_t>(std::ceil(fraction * static_cast<double>(gaps.size())));
    return gaps[std::min(gaps.size(), std::max<std::size_t>(index, 1)) - 1];
  };
  return {rank(0.5), rank(0.95)};
}

// Where a mesh is drawn at a placement: its box's middle, as "x,y,z".
QString middleOf(const ProxyMesh& mesh, const gp_Trsf& placement) {
  gp_XYZ low(1e300, 1e300, 1e300);
  gp_XYZ high(-1e300, -1e300, -1e300);
  for (const gp_Pnt& node : mesh.nodes) {
    const gp_XYZ at = node.Transformed(placement).XYZ();
    low.SetCoord(std::min(low.X(), at.X()), std::min(low.Y(), at.Y()), std::min(low.Z(), at.Z()));
    high.SetCoord(std::max(high.X(), at.X()), std::max(high.Y(), at.Y()), std::max(high.Z(), at.Z()));
  }
  const gp_XYZ middle = (low + high) / 2.0;
  const auto coordinate = [](double value) { return QString::number(std::abs(value) < 1e-6 ? 0.0 : value, 'f', 3); };
  return QStringLiteral("%1,%2,%3").arg(coordinate(middle.X()), coordinate(middle.Y()), coordinate(middle.Z()));
}

// What a drag of a motion does to a body the exact preview draws
// (mitcad#838): a body of an occurrence it moves (or of one in it) moves
// with the occurrence; a body it moves, or makes, moves in its component.
MotionSide motionSideOf(const DragMotion& motion, const BodyDisplay& body) {
  const QString occurrence = QString::fromStdString(body.occurrence);
  for (const QString& moved : motion.occurrences) {
    if (!moved.isEmpty() && (occurrence == moved || occurrence.startsWith(moved + QLatin1Char('/')))) {
      return MotionSide::Occurrence;
    }
  }
  if ((motion.made && body.style == BodyDisplay::Style::New) ||
      motion.bodies.contains(QString::fromStdString(body.uid))) {
    return MotionSide::Shape;
  }
  return MotionSide::None;
}

double medianOf(std::vector<double> values) {
  if (values.empty()) {
    return 0.0;
  }
  std::sort(values.begin(), values.end());
  return values[(values.size() - 1) / 2];
}

} // namespace

TopoDS_Shape CommandSession::placedTool(const TopoDS_Shape& tool, const std::vector<gp_Trsf>& placements) {
  if (tool.IsNull() || placements.empty()) {
    return TopoDS_Shape();
  }
  const auto placed = [&tool](const gp_Trsf& placement) {
    return placement.Form() == gp_Identity ? tool : tool.Moved(TopLoc_Location(placement));
  };
  if (placements.size() == 1) {
    return placed(placements.front());
  }
  // One copy per occurrence of its component, sharing its faces and their
  // triangulations.
  TopoDS_Compound compound;
  BRep_Builder builder;
  builder.MakeCompound(compound);
  for (const gp_Trsf& placement : placements) {
    builder.Add(compound, placed(placement));
  }
  return compound;
}

void CommandSession::exactPreviewShown(const Preview& preview, const std::vector<BodyDisplay>& drawn) {
  if (m_def.dragMotion) {
    // The bodies it draws and what a drag does to each (mitcad#838).
    ++m_motionShown;
    // The preview's own bodies (not the tool overlays after them) that
    // the motion moves.
    std::vector<MotionSide> sides;
    if (preview.motion) {
      for (std::size_t i = 0; i < preview.bodies.size() && i < drawn.size(); ++i) {
        sides.push_back(motionSideOf(*preview.motion, drawn[i]));
      }
    }
    const bool moves =
        std::any_of(sides.begin(), sides.end(), [](MotionSide side) { return side != MotionSide::None; });
    if (preview.state && (moves || !preview.motion)) {
      ProxyBasis basis;
      basis.state = *preview.state;
      basis.structure = preview.structure;
      basis.motion = preview.motion;
      basis.drawn = drawn;
      basis.sides = std::move(sides);
      basis.shown = m_motionShown;
      // Where the first moved body is, for the log: read now that its job
      // is over (never while a job owns the shapes).
      for (std::size_t i = 0; i < basis.sides.size() && i < drawn.size() && !m_host.modelBusy(); ++i) {
        if (basis.sides[i] == MotionSide::None || !drawn[i].shape) {
          continue;
        }
        // The middle of its shape's data, carried by the shape's location:
        // a body a move placed by a location (mitcad#838) keeps the same
        // middle in its data, so that where the proxy and the exact
        // preview draw it compare point for point.
        const TopoDS_Shape& shape = drawn[i].shape->occt();
        Bnd_Box box;
        BRepBndLib::Add(shape.Located(TopLoc_Location()), box, true);
        if (!box.IsVoid()) {
          basis.middle = gp_Pnt((box.CornerMin().XYZ() + box.CornerMax().XYZ()) / 2.0)
                             .Transformed(shape.Location().Transformation());
          basis.middleIndex = static_cast<int>(i);
        }
        break;
      }
      m_proxyBasis = std::move(basis);
    } else {
      m_proxyBasis.reset();
    }
    if (m_proxyStats.active && m_proxyStats.releasedAt < 0) {
      m_proxyStats.exactAt.push_back(m_proxyStats.clock.elapsed());
    }
    if (!m_proxyKey.isEmpty()) {
      updateDragProxy();
    }
    return;
  }
  if (!m_def.dragProxy) {
    return;
  }
  const std::shared_ptr<const ProxyMesh> before = m_proxyBasis ? m_proxyBasis->mesh : nullptr;
  if (preview.state && preview.toolMesh && !preview.toolPlacements.empty()) {
    ProxyBasis basis;
    basis.state = *preview.state;
    basis.structure = preview.structure;
    basis.mesh = preview.toolMesh;
    basis.placements = preview.toolPlacements;
    m_proxyBasis = std::move(basis);
  } else {
    m_proxyBasis.reset();
  }
  const std::shared_ptr<const ProxyMesh> after = m_proxyBasis ? m_proxyBasis->mesh : nullptr;
  if (m_proxyStats.active && m_proxyStats.releasedAt < 0 && after != before) {
    m_proxyStats.exactAt.push_back(m_proxyStats.clock.elapsed());
  }
  if (!m_proxyKey.isEmpty()) {
    updateDragProxy();
  }
}

void CommandSession::dragProxyMoved(const QString& key, double value) {
  if ((!m_def.dragProxy && !m_def.dragMotion) || m_done) {
    return;
  }
  if (!m_proxyKey.isEmpty() && key == m_proxyKey && value == m_proxyValue) {
    return; // the pending input's own call once the model is idle
  }
  if (!m_proxyStats.active || m_proxyStats.releasedAt >= 0) {
    // A new drag: its own counts.
    m_proxyStats = ProxyStats();
    m_proxyStats.active = true;
    m_proxyStats.clock.start();
    m_proxyStats.frames = m_host.viewer().framesDrawn();
  }
  ++m_proxyStats.inputs;
  m_proxyStats.inputAt = m_proxyStats.clock.elapsed();
  m_proxyKey = key;
  m_proxyValue = value;
  if (m_def.dragMotion) {
    // The exact preview waits while the pointer moves (mitcad#838).
    m_motionPaused = false;
    m_motionPause.start();
  }
  updateDragProxy();
}

void CommandSession::updateDragProxy() {
  if (m_proxyKey.isEmpty() || m_done) {
    return;
  }
  if (m_def.dragMotion) {
    updateMotionProxy();
    return;
  }
  QString why;
  std::optional<DragProxyRule> rule;
  if (!m_proxyBasis) {
    why = QStringLiteral("no exact preview to start from");
  } else if (m_proxyBasis->structure != m_structureGeneration) {
    why = QStringLiteral("the exact preview is of other inputs");
  } else {
    rule = m_def.dragProxy(m_proxyBasis->state, m_state, m_proxyKey, m_proxyValue);
    if (!rule) {
      why = QStringLiteral("not for these inputs");
    }
  }
  const bool dragging = m_manipulators && m_manipulators->isDragging();
  if (rule && std::abs(rule->to - rule->from) <= 1e-9) {
    // The exact preview shows the value dragged to: it is what is shown
    // and committed.
    if (m_proxyStats.active && m_proxyStats.releasedAt >= 0) {
      qDebug().noquote() << QStringLiteral("Drag proxy %1: the exact preview of %2 replaced it %3 ms after the release")
                                .arg(m_def.name, millimetres(m_proxyValue))
                                .arg(m_proxyStats.clock.elapsed() - m_proxyStats.releasedAt);
      m_proxyStats.active = false;
    }
    hideDragProxy(QStringLiteral("the exact preview shows %1").arg(millimetres(m_proxyValue)), !dragging);
    return;
  }
  if (rule && m_proxyShown && m_proxyDrawnFrom.lock() == m_proxyBasis->mesh && m_proxyDrawnValue == m_proxyValue) {
    return; // drawn so already
  }
  std::optional<Manipulator> arrow;
  if (rule) {
    arrow = m_manipulators ? m_manipulators->arrowOf(m_proxyKey) : std::nullopt;
    if (!arrow) {
      why = QStringLiteral("no arrow");
    }
  }
  std::optional<DragProxy> proxy;
  QElapsedTimer timer;
  timer.start();
  if (rule && arrow) {
    // The arrow is drawn in the design; the tool's mesh is in its
    // component's coordinates: the arrow's direction as each placement
    // gives it there, and as it is (a direction the placement keeps).
    std::vector<gp_Dir> axes;
    for (const gp_Trsf& placement : m_proxyBasis->placements) {
      axes.push_back(arrow->direction.Transformed(placement.Inverted()));
    }
    axes.push_back(arrow->direction);
    proxy = applyDragProxy(*m_proxyBasis->mesh, *rule, axes);
    if (!proxy) {
      why = QStringLiteral("the exact preview's tool is not the prism of its values");
    }
  }
  if (!proxy) {
    hideDragProxy(why, false);
    return;
  }
  OcctViewer& viewer = m_host.viewer();
  viewer.setProvisional(proxy->mesh, m_proxyBasis->placements);
  m_proxyViewer = &viewer;
  m_proxyDrawnFrom = m_proxyBasis->mesh;
  m_proxyDrawnValue = m_proxyValue;
  const double spent = static_cast<double>(timer.nsecsElapsed()) / 1e6;
  if (m_proxyStats.active) {
    m_proxyStats.proxyAt.push_back(m_proxyStats.clock.elapsed());
    m_proxyStats.updateMs.push_back(spent);
  }
  if (!m_proxyShown) {
    m_proxyShown = true;
    qDebug().noquote() << QStringLiteral("Drag proxy %1: shown for %2 %3 (the exact preview at %4) at %5")
                              .arg(m_def.name, m_proxyKey, millimetres(rule->to), millimetres(rule->from),
                                   middleOf(*proxy->mesh, m_proxyBasis->placements.front()));
  }
  m_proxyWhy.clear();
  if (timingLogged()) {
    qDebug().noquote() << QStringLiteral("Drag proxy %1 %2: %3 over the exact %4 at %5 in %6 ms")
                              .arg(m_def.name, m_proxyKey, millimetres(rule->to), millimetres(rule->from),
                                   middleOf(*proxy->mesh, m_proxyBasis->placements.front()))
                              .arg(spent, 0, 'f', 3);
  }
}

void CommandSession::hideDragProxy(const QString& why, bool forget) {
  if (m_proxyShown) {
    m_proxyShown = false;
    if (m_proxyViewer && m_def.dragMotion) {
      // The exact preview's bodies where it put them (mitcad#838); without
      // a basis the caller draws what follows.
      if (m_proxyBasis) {
        m_proxyViewer->setBodies(m_proxyBasis->drawn);
      }
    } else if (m_proxyViewer) {
      m_proxyViewer->setProvisional(nullptr, {});
    }
    qDebug().noquote() << QStringLiteral("Drag proxy %1: hidden: %2").arg(m_def.name, why);
    m_proxyWhy = why;
  } else if (!m_proxyKey.isEmpty() && why != m_proxyWhy) {
    m_proxyWhy = why;
    qDebug().noquote() << QStringLiteral("Drag proxy %1: not shown: %2").arg(m_def.name, why);
  }
  if (forget) {
    m_proxyKey.clear();
    m_proxyWhy.clear();
    if (m_proxyStats.releasedAt >= 0) {
      m_proxyStats.active = false;
    }
  }
}

void CommandSession::dragProxyFinished(const QString& key) {
  m_motionPause.stop();
  m_motionPaused = false;
  if (!m_proxyStats.active || m_proxyStats.releasedAt >= 0) {
    return;
  }
  ProxyStats& stats = m_proxyStats;
  stats.releasedAt = stats.clock.elapsed();
  const auto proxy = intervals(stats.proxyAt);
  const auto exact = intervals(stats.exactAt);
  double slowest = 0.0;
  for (const double ms : stats.updateMs) {
    slowest = std::max(slowest, ms);
  }
  // Delivered semantic inputs, proxy updates drawn and exact previews shown
  // while the button was held, and the frames submitted meanwhile: each
  // counted on its own (mitcad#636).
  qDebug().noquote() << QStringLiteral("Drag proxy %1 %2: %3 inputs, %4 proxy updates (every %5 ms median, %6 ms p95), "
                                       "%7 exact previews (every %8 ms median, %9 ms p95), %10 frames in %11 ms; "
                                       "proxy update %12 ms median, %13 ms max")
                            .arg(m_def.name, key)
                            .arg(stats.inputs)
                            .arg(stats.proxyAt.size())
                            .arg(proxy.first, 0, 'f', 1)
                            .arg(proxy.second, 0, 'f', 1)
                            .arg(stats.exactAt.size())
                            .arg(exact.first, 0, 'f', 1)
                            .arg(exact.second, 0, 'f', 1)
                            .arg(m_host.viewer().framesDrawn() - stats.frames)
                            .arg(stats.releasedAt)
                            .arg(medianOf(stats.updateMs), 0, 'f', 3)
                            .arg(slowest, 0, 'f', 3);
  if (!m_proxyShown) {
    // Nothing provisional is left: the exact preview has the value.
    stats.active = false;
    m_proxyKey.clear();
    m_proxyWhy.clear();
  }
}

QString CommandSession::proxyValueText(const QString& key, double value) const {
  const auto slot = slotOf(key);
  if (slot && slot->def->valueKind == ValueKind::Angle) {
    const double degrees = value * 45.0 / std::atan(1.0);
    return QStringLiteral("%1 deg").arg(std::abs(degrees) < 1e-9 ? 0.0 : degrees, 0, 'g', 10);
  }
  if (slot && slot->def->valueKind == ValueKind::Unitless) {
    return QString::number(value, 'g', 10);
  }
  return millimetres(value);
}

bool CommandSession::motionDeferred() const {
  return m_def.dragMotion && m_proxyShown && !m_motionPaused && m_manipulators != nullptr &&
         m_manipulators->isDragging();
}

void CommandSession::updateMotionProxy() {
  QString why;
  std::optional<gp_Trsf> from;
  std::optional<gp_Trsf> to;
  if (!m_proxyBasis) {
    why = QStringLiteral("no exact preview to start from");
  } else if (!m_proxyBasis->motion || !m_proxyBasis->motion->of) {
    // Point to Point's, which has no value to drag; a basis the model did
    // not report (mitcad#838).
    why = QStringLiteral("no motion of these inputs");
  } else if (m_proxyBasis->structure != m_structureGeneration) {
    why = QStringLiteral("the exact preview is of other inputs");
  } else {
    // The inputs as they are, the dragged one at the value dragged to.
    CommandState dragged = m_state;
    dragged.setValue(m_proxyKey, m_proxyValue);
    from = m_proxyBasis->motion->of(m_proxyBasis->state);
    to = m_proxyBasis->motion->of(dragged);
    if (!from || !to) {
      why = QStringLiteral("not for these inputs");
    }
  }
  const bool dragging = m_manipulators && m_manipulators->isDragging();
  if (from && to && sameMotion(*from, *to)) {
    // The exact preview shows where the value puts them: it is what is
    // shown and committed.
    if (m_proxyStats.active && m_proxyStats.releasedAt >= 0) {
      // How far from where the proxy last drew the first moved body the
      // exact preview draws it, when the proxy was drawn at this value:
      // the two agree (mitcad#838).
      QString apart;
      const int index = m_proxyBasis->middleIndex;
      if (m_proxyShown && m_motionAt && m_proxyDrawnValue == m_proxyValue && index >= 0 &&
          static_cast<std::size_t>(index) < m_proxyBasis->drawn.size()) {
        const gp_Pnt exact = m_proxyBasis->middle.Transformed(
            m_proxyBasis->drawn[static_cast<std::size_t>(index)].placement.Transformation());
        apart = QStringLiteral(", %1 mm from the proxy").arg(exact.Distance(*m_motionAt), 0, 'f', 6);
      }
      qDebug().noquote() << QStringLiteral("Drag proxy %1: the exact preview of %2 replaced it %3 ms after the release%4")
                                .arg(m_def.name, proxyValueText(m_proxyKey, m_proxyValue))
                                .arg(m_proxyStats.clock.elapsed() - m_proxyStats.releasedAt)
                                .arg(apart);
      m_proxyStats.active = false;
    }
    hideDragProxy(QStringLiteral("the exact preview shows %1").arg(proxyValueText(m_proxyKey, m_proxyValue)),
                  !dragging);
    return;
  }
  if (!from || !to) {
    hideDragProxy(why, false);
    return;
  }
  if (m_proxyShown && m_motionDrawnFrom == m_proxyBasis->shown && m_proxyDrawnValue == m_proxyValue) {
    return; // drawn so already
  }
  QElapsedTimer timer;
  timer.start();
  // Each moved body's meshes where the dragged value puts them: the view
  // keeps its drawn objects and only places them (no model, no meshing).
  std::vector<BodyDisplay> bodies = m_proxyBasis->drawn;
  const gp_Trsf& frame = m_proxyBasis->motion->frame;
  for (std::size_t i = 0; i < m_proxyBasis->sides.size() && i < bodies.size(); ++i) {
    const MotionSide side = m_proxyBasis->sides[i];
    if (side != MotionSide::None) {
      bodies[i].placement =
          TopLoc_Location(draggedPlacement(bodies[i].placement.Transformation(), side, *from, *to, frame));
    }
  }
  OcctViewer& viewer = m_host.viewer();
  viewer.setBodies(bodies);
  m_proxyViewer = &viewer;
  m_motionDrawnFrom = m_proxyBasis->shown;
  m_proxyDrawnValue = m_proxyValue;
  const double spent = static_cast<double>(timer.nsecsElapsed()) / 1e6;
  if (m_proxyStats.active) {
    m_proxyStats.proxyAt.push_back(m_proxyStats.clock.elapsed());
    m_proxyStats.updateMs.push_back(spent);
  }
  // Where the first moved body's middle is drawn now, for UI tests.
  QString at = QStringLiteral("-");
  m_motionAt.reset();
  if (m_proxyBasis->middleIndex >= 0 && static_cast<std::size_t>(m_proxyBasis->middleIndex) < bodies.size()) {
    const BodyDisplay& first = bodies[static_cast<std::size_t>(m_proxyBasis->middleIndex)];
    const gp_Pnt middle = m_proxyBasis->middle.Transformed(first.placement.Transformation());
    m_motionAt = middle;
    const auto coordinate = [](double value) { return QString::number(std::abs(value) < 1e-6 ? 0.0 : value, 'f', 3); };
    at = QStringLiteral("%1,%2,%3").arg(coordinate(middle.X()), coordinate(middle.Y()), coordinate(middle.Z()));
  }
  const QString shownValue = proxyValueText(m_proxyKey, m_proxyBasis->state.value(m_proxyKey));
  if (!m_proxyShown) {
    m_proxyShown = true;
    qDebug().noquote() << QStringLiteral("Drag proxy %1: shown for %2 %3 (the exact preview at %4) at %5")
                              .arg(m_def.name, m_proxyKey, proxyValueText(m_proxyKey, m_proxyValue), shownValue, at);
  }
  m_proxyWhy.clear();
  if (timingLogged()) {
    qDebug().noquote() << QStringLiteral("Drag proxy %1 %2: %3 over the exact %4 at %5 in %6 ms")
                              .arg(m_def.name, m_proxyKey, proxyValueText(m_proxyKey, m_proxyValue), shownValue, at)
                              .arg(spent, 0, 'f', 3);
  }
}

} // namespace mitcad
