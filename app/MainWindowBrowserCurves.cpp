// SPDX-License-Identifier: MIT
// Sketch curves picked in the Browser (mitcad#1978; app/COMMANDS.md,
// "Browser"): while a command's active input takes sketch curves (Patch's
// boundary, Boundary Surface's curves, a surface Extrude's profile, ...),
// each sketch's row lists its curves under it, named in words ("Line 1",
// browser/SketchCurveRows.hpp). A click on a curve's row picks it for the
// input as a click on the curve in the view does (pickItems); the pointer
// over the row paints the curve in the view, as a notice's places are
// painted (hoverNoticePlaces, source "browser curve"), logged for the UI
// tests:
//   Browser sketch curves: 3 sketches, 7 curves
//   Notice hover highlighted: browser curve: sketch_curve c42 of F14 (1 drawn)
// The rows go when the command ends or its active input takes no sketch
// curves. Each sketch is read once per document state (the cache is
// emptied by refreshScene).
#include "MainWindow.hpp"

#include "browser/BrowserController.hpp"

#include <exception>
#include <utility>

#include <QJsonArray>
#include <QJsonObject>

#include "browser/SketchCurveRows.hpp"

namespace mitcad {

void MainWindow::installBrowserCurves() {
  connect(m_controller->browser(), &BrowserPanel::curveHovered, this, &MainWindow::previewBrowserCurve);
}

void MainWindow::updateBrowserSketchCurves(SelectFilter filter) {
  if (m_controller == nullptr) {
    return;
  }
  BrowserPanel* browser = m_controller->browser();
  // A feature command's input (the command's mode is set before its
  // session starts and activates its first input), not one of a command
  // inside sketch mode (the edited sketch's curves are sketch mode's own).
  const bool takes = m_mode == Mode::Command && !m_commandInSketch && bool(filter & SelectKind::SketchCurve);
  if (!takes) {
    m_browserCurveCache.clear();
    browser->setSketchCurves({});
    return;
  }
  if (modelBusy()) {
    // A preview computes: the sketches are read once the job has ended,
    // as the input is then.
    if (!m_browserCurvesWait) {
      m_browserCurvesWait = true;
      whenIdle(this, [this] {
        m_browserCurvesWait = false;
        updateBrowserSketchCurves(m_mode == Mode::Command ? m_pickPlacesFilter : SelectFilter());
      });
    }
    return;
  }
  QHash<QString, QVector<BrowserSketchCurve>> curves;
  for (const DocumentSnapshot::Feature& feature : std::as_const(m_snapshot.features)) {
    if (!feature.isAnySketch() || !feature.succeeded()) {
      continue;
    }
    auto known = m_browserCurveCache.constFind(feature.uid);
    if (known == m_browserCurveCache.cend()) {
      QVector<BrowserSketchCurve> rows;
      try {
        rows = feature.isSpatialSketch()
                   ? spatialSketchCurveRows(queryObject(
                         {{QStringLiteral("query"), QStringLiteral("sketch3d")}, {QStringLiteral("uid"), feature.uid}}))
                   : sketchCurveRows(queryObject(
                         {{QStringLiteral("query"), QStringLiteral("sketch")}, {QStringLiteral("uid"), feature.uid}}));
      } catch (const std::exception&) {
        // A sketch the model could not answer for: no rows.
      }
      known = m_browserCurveCache.insert(feature.uid, rows);
    }
    if (!known.value().isEmpty()) {
      curves.insert(feature.uid, known.value());
    }
  }
  browser->setSketchCurves(curves);
}

void MainWindow::previewBrowserCurve(const BrowserNode& node) {
  QJsonArray places;
  if (node.type == BrowserNode::Type::SketchCurve) {
    // Where its sketch row's occurrence places it; the root's (none):
    // where the view shows the sketch.
    QJsonObject place{{QStringLiteral("kind"), QStringLiteral("sketch_curve")},
                      {QStringLiteral("sketch"), node.uid},
                      {QStringLiteral("name"), node.curve}};
    if (!node.occurrence.isEmpty()) {
      place.insert(QStringLiteral("occurrence"), node.occurrence);
    }
    places.append(place);
  }
  hoverNoticePlaces(QStringLiteral("browser curve"), places, {});
}

} // namespace mitcad
