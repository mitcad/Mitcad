// SPDX-License-Identifier: MIT
// Simulate's stock (mitcad#1013, phase 3; app/COMMANDS.md "The CNC
// group", MainWindow.hpp): the setup's program cut out of its stock by the
// model (cam_simulate), read once with snapshots at places along
// Simulate's playback; the stock as cut drawn at the slider's place, and
// at the end shaded by the material it leaves over the part, with the
// material left in words and the legend under the slider; a lathe's
// sub-spindle drawn where it is as each operation cuts (mitcad#2405), the
// fixed and moving zones and the turret round the tool (mitcad#2412).
//
// Messages a person sees say what is wrong in words; the log lines carry
// the numbers UI tests read.
#include "../MainWindow.hpp"
#include "../framework/Json.hpp"

#include <algorithm>
#include <cstddef>
#include <exception>
#include <initializer_list>
#include <optional>
#include <utility>
#include <vector>

#include <QJsonArray>
#include <QJsonObject>
#include <QJsonValue>
#include <QList>
#include <QStringList>
#include <QtLogging>

#include "../commands/CommandSupport.hpp"
#include "../framework/ModelMessages.hpp"
#include "../framework/Theme.hpp"
#include "../framework/UserText.hpp"
#include "CamSimulateDialog.hpp"
#include "CamStock.hpp"
#include "CamToolpaths.hpp"

namespace mitcad {
namespace {

// Snapshots of the stock along the playback, and the most nodes of its
// grid: what the view redraws as the slider moves.
constexpr int kStockSnapshots = 120;
constexpr int kStockNodes = 150000;

} // namespace

void MainWindow::loadCamStock(int setup, const QString& name, const cam::WorkFrame& frame) {
  clearCamStock();
  if (m_camPlayback.empty() || !frame.valid) {
    return;
  }
  // A snapshot after the move of each of evenly spaced places, each move
  // once (an arc's places are one move).
  std::vector<int> places;
  QJsonArray frames;
  int lastOperation = -1;
  int lastMove = -1;
  for (const int place : cam::framePlaces(static_cast<int>(m_camPlayback.size()), kStockSnapshots)) {
    const cam::PlaybackPoint& point = m_camPlayback.at(static_cast<std::size_t>(place));
    const int operation = m_camPlaybackOperations.at(static_cast<std::size_t>(point.operation)).id;
    if (operation == lastOperation && point.move == lastMove) {
      places.back() = place;
      continue;
    }
    lastOperation = operation;
    lastMove = point.move;
    places.push_back(place);
    frames.append(QJsonObject{{QStringLiteral("operation"), operation}, {QStringLiteral("move"), point.move}});
  }
  QJsonObject answer;
  try {
    answer = query({{QStringLiteral("query"), QStringLiteral("cam_simulate")},
                    {QStringLiteral("setup"), setup},
                    {QStringLiteral("frames"), frames},
                    {QStringLiteral("max_nodes"), kStockNodes}})
                 .toObject();
  } catch (const std::exception& e) {
    qWarning().noquote() << QStringLiteral("CAM stock of %1 not read: %2").arg(name, QString::fromUtf8(e.what()));
    return;
  }
  // A lathe setup's stock is its turned section (mitcad#1869): drawn as the
  // section revolved about the spindle axis at the program's end, with its
  // summary in words.
  if (answer.value(QStringLiteral("turning")).toBool()) {
    const cam::StockMesh turned = cam::turnedStockMesh(answer.value(QStringLiteral("stock")).toObject(), frame);
    qInfo().noquote() << QStringLiteral("CAM turned stock overlay: %1 vertices, %2 triangles")
                             .arg(turned.vertices())
                             .arg(turned.triangleCount());
    if (!turned.empty()) {
      m_camTurnedStock = true;
      m_viewer->setCamStock(turned);
    }
    // The whole program cut out of the bar in 3D (mitcad#2319): drawn in
    // place of the revolved section when it reads, its volumes logged.
    const QJsonObject solid = answer.value(QStringLiteral("solid")).toObject();
    const cam::StockMesh cubes = cam::solidStockMesh(solid);
    qInfo().noquote() << QStringLiteral("CAM mill-turn stock: %1: %2 triangles, milled %3, left %4, gouge %5, %6 rapid "
                                        "cut(s)")
                             .arg(name)
                             .arg(cubes.triangleCount())
                             .arg(cam::shortNumber(solid.value(QStringLiteral("milled")).toDouble()),
                                  cam::shortNumber(solid.value(QStringLiteral("left")).toDouble()),
                                  cam::shortNumber(solid.value(QStringLiteral("gouge")).toDouble()))
                             .arg(solid.value(QStringLiteral("rapid_cuts")).toArray().size());
    if (!cubes.empty()) {
      m_camTurnedStock = true;
      m_viewer->setCamStock(cubes);
    }
    // The sub-spindle's outline and where it is as each operation cuts
    // (mitcad#2405): drawn at the slider's place (showCamSubSpindle).
    const QJsonObject subSpindle = answer.value(QStringLiteral("sub_spindle")).toObject();
    if (!subSpindle.isEmpty()) {
      m_camSubSpindle = CamSubSpindleDrawn{name, subSpindle, frame};
      qInfo().noquote() << QStringLiteral("CAM sub-spindle outline: %1: %2 part(s)")
                               .arg(name)
                               .arg(subSpindle.value(QStringLiteral("outline")).toArray().size());
    }
    // The fixed and moving zones and the turret's body where they are as
    // each operation cuts (mitcad#2412): drawn at the slider's place
    // (showCamMachineParts).
    const QJsonArray zonesDrawn = answer.value(QStringLiteral("zones")).toArray();
    const QJsonArray turretDrawn = answer.value(QStringLiteral("turret")).toArray();
    if (!zonesDrawn.isEmpty() || !turretDrawn.isEmpty()) {
      m_camMachineDrawn = CamMachineDrawn{name, zonesDrawn, turretDrawn, frame};
    }
    const double left = answer.value(QStringLiteral("left")).toDouble();
    const int gouges = static_cast<int>(answer.value(QStringLiteral("gouges")).toArray().size());
    QJsonArray cuts = answer.value(QStringLiteral("rapid_cuts")).toArray();
    qInfo().noquote() << QStringLiteral("CAM turned stock: %1: left %2, %3 gouge(s), %4 rapid cut(s)")
                             .arg(name, cam::shortNumber(left))
                             .arg(gouges)
                             .arg(cuts.size());
    // The live tools' cuts on the face and the diameter (mitcad#2067), in
    // words: the material they leave over the part and their cuts into it.
    QStringList liveSummary;
    for (const QJsonValue& value : answer.value(QStringLiteral("live")).toArray()) {
      const QJsonObject side = value.toObject();
      const QString kind = str(side.value(QStringLiteral("side")).toObject(), "kind");
      const bool face = kind == QStringLiteral("face");
      // A plane with the Y axis or the B head (mitcad#2319).
      const bool plane = kind == QStringLiteral("plane");
      const QJsonObject remaining = side.value(QStringLiteral("remaining")).toObject();
      const QJsonArray liveCuts = side.value(QStringLiteral("rapid_cuts")).toArray();
      const double liveLeft = remaining.value(QStringLiteral("max")).toDouble();
      const double gouge = remaining.value(QStringLiteral("gouge")).toDouble();
      // On the part's back while the sub-spindle holds it (mitcad#2375).
      const bool sub = side.value(QStringLiteral("sub_spindle")).toBool();
      const QString kindLogged = face    ? QStringLiteral("face")
                                 : plane ? QStringLiteral("plane")
                                         : QStringLiteral("diameter");
      qInfo().noquote() << QStringLiteral("CAM live tools: %1: %2, %3 operation(s), left %4, gouge %5, %6 rapid cut(s)")
                               .arg(name, sub ? kindLogged + QStringLiteral(" on the sub-spindle") : kindLogged)
                               .arg(side.value(QStringLiteral("operations")).toArray().size())
                               .arg(cam::shortNumber(liveLeft), cam::shortNumber(gouge))
                               .arg(liveCuts.size());
      const QString cut = gouge > 0.0 ? tr("They cut into the part.") : tr("They cut nothing of the part.");
      liveSummary << (sub     ? tr("The live tools on the sub-spindle: at most %1 mm left over the part's back. %2")
                      : face  ? tr("The live tools on the face: at most %1 mm left over the part. %2")
                      : plane ? tr("The live tools on a plane: at most %1 mm left over the part. %2")
                              : tr("The live tools on the diameter: at most %1 mm left over the part. %2"))
                         .arg(cam::shortNumber(liveLeft), cut);
      for (const QJsonValue& liveCut : liveCuts) {
        cuts.append(liveCut);
      }
    }
    // The moves that run into the sub-spindle's body or a zone moving with
    // it (mitcad#2392), in words: in the summary, the first as the warning.
    const QJsonArray subHits = answer.value(QStringLiteral("sub_spindle_hits")).toArray();
    qInfo().noquote() << QStringLiteral("CAM sub-spindle hits: %1: %2").arg(name).arg(subHits.size());
    for (const QJsonValue& hit : subHits) {
      qInfo().noquote() << QStringLiteral("CAM sub-spindle hit %1: %2").arg(name, str(hit.toObject(), "message"));
    }
    // The turret's body running into the main chuck or a fixed zone
    // (mitcad#2412), in words as the sub-spindle's.
    const QJsonArray turretHits = answer.value(QStringLiteral("turret_hits")).toArray();
    qInfo().noquote() << QStringLiteral("CAM turret hits: %1: %2").arg(name).arg(turretHits.size());
    for (const QJsonValue& hit : turretHits) {
      qInfo().noquote() << QStringLiteral("CAM turret hit %1: %2").arg(name, str(hit.toObject(), "message"));
    }
    if (m_camSimulate != nullptr) {
      const QString cut = gouges > 0 ? tr("The tools cut into the part.") : tr("No tool cuts into the part.");
      QString summary = tr("The turned part: at most %1 mm of material left over it. %2").arg(cam::shortNumber(left), cut);
      for (const QString& line : std::as_const(liveSummary)) {
        summary += QLatin1Char(' ') + line;
      }
      if (!subHits.isEmpty()) {
        summary += QLatin1Char(' ') + tr("%n move(s) run into the sub-spindle or a zone moving with it.", nullptr,
                                         static_cast<int>(subHits.size()));
      }
      if (!turretHits.isEmpty()) {
        summary += QLatin1Char(' ') + tr("%n move(s) bring the turret into the chuck or a fixed zone.", nullptr,
                                         static_cast<int>(turretHits.size()));
      }
      if (!cubes.empty()) {
        summary += QLatin1Char(' ') + tr("In 3D: %1 mm³ left over the part, %2 mm³ of it cut away.")
                                          .arg(cam::shortNumber(solid.value(QStringLiteral("left")).toDouble()),
                                               cam::shortNumber(solid.value(QStringLiteral("gouge")).toDouble()));
      }
      m_camSimulate->setRemaining(summary, {});
    }
    if (!cuts.isEmpty()) {
      showWarning(tr("The simulation found a problem. %1")
                      .arg(modelMessage(userText(str(cuts.first().toObject(), "message")))));
    } else if (!subHits.isEmpty()) {
      showWarning(tr("The simulation found a problem. %1")
                      .arg(modelMessage(userText(str(subHits.first().toObject(), "message")))));
    } else if (!turretHits.isEmpty()) {
      showWarning(tr("The simulation found a problem. %1")
                      .arg(modelMessage(userText(str(turretHits.first().toObject(), "message")))));
    }
    return;
  }
  CamStockPlayback stock;
  stock.setup = name;
  stock.grid = cam::StockGrid::of(answer.value(QStringLiteral("grid")).toObject());
  stock.frame = frame;
  stock.initial.assign(stock.grid.nodes(), 0.0f);
  const QJsonArray snapshots = answer.value(QStringLiteral("frames")).toArray();
  if (!stock.grid.valid() || !cam::applyRuns(answer.value(QStringLiteral("initial")).toArray(), stock.initial) ||
      snapshots.size() != static_cast<qsizetype>(places.size())) {
    qWarning().noquote() << QStringLiteral("CAM stock of %1 not read: the grid or its snapshots do not fit").arg(name);
    return;
  }
  stock.places = std::move(places);
  for (const QJsonValue& snapshot : snapshots) {
    stock.changes.push_back(snapshot.toObject().value(QStringLiteral("runs")).toArray());
  }
  const QJsonArray part = answer.value(QStringLiteral("part")).toArray();
  if (!part.isEmpty()) {
    stock.part.assign(stock.grid.nodes(), 0.0f);
    if (!cam::applyRuns(part, stock.part)) {
      stock.part.clear();
    }
  }
  stock.remaining = answer.value(QStringLiteral("remaining")).toObject();
  stock.heights = stock.initial;
  qInfo().noquote() << cam::stockLog(name, stock.grid, static_cast<int>(stock.places.size()));
  // A rapid that cuts the stock is a fault of the program.
  const QJsonArray cuts = answer.value(QStringLiteral("rapid_cuts")).toArray();
  for (const QJsonValue& cut : cuts) {
    qInfo().noquote() << QStringLiteral("CAM stock rapid cut: %1").arg(str(cut.toObject(), "message"));
  }
  if (!cuts.isEmpty()) {
    showWarning(tr("The simulation found a problem. %1")
                    .arg(modelMessage(userText(str(cuts.first().toObject(), "message")))));
  }
  m_camStock = std::move(stock);
  m_viewer->setCamStock(cam::stockMesh(m_camStock->grid, m_camStock->heights, m_camStock->frame));
}

void MainWindow::showCamStock(int index) {
  if (!m_camStock) {
    return;
  }
  CamStockPlayback& stock = *m_camStock;
  const int snapshot = cam::frameAt(stock.places, index);
  const bool atEnd = index + 1 >= static_cast<int>(m_camPlayback.size());
  if (snapshot == stock.shown && atEnd == stock.atEnd) {
    return;
  }
  if (snapshot < stock.shown) {
    // Back: from the stock before the cut again.
    stock.heights = stock.initial;
    stock.shown = -1;
  }
  for (int k = stock.shown + 1; k <= snapshot; ++k) {
    if (!cam::applyRuns(stock.changes.at(static_cast<std::size_t>(k)), stock.heights)) {
      qWarning().noquote() << QStringLiteral("CAM stock of %1: a snapshot does not fit the grid").arg(stock.setup);
      clearCamStock();
      return;
    }
  }
  stock.shown = snapshot;
  stock.atEnd = atEnd;
  const bool shaded = atEnd && !stock.part.empty() && !stock.remaining.isEmpty();
  const double tolerance = stock.remaining.value(QStringLiteral("tolerance")).toDouble(0.01);
  const double thickest = std::max({stock.remaining.value(QStringLiteral("max")).toDouble(),
                                    stock.remaining.value(QStringLiteral("beside")).toDouble(), 2.0 * tolerance});
  m_viewer->setCamStock(
      cam::stockMesh(stock.grid, stock.heights, stock.frame, shaded ? &stock.part : nullptr, tolerance, thickest));
  qDebug().noquote() << QStringLiteral("CAM stock at snapshot %1 of %2").arg(snapshot + 1).arg(stock.places.size());
  if (shaded && !stock.remainingLogged) {
    stock.remainingLogged = true;
    qInfo().noquote() << cam::remainingLog(stock.setup, stock.remaining);
  }
  if (m_camSimulate != nullptr) {
    const bool dark = m_viewer->backgroundDark();
    const QList<QColor> legend{viewColor(ColorRole::CamStock, dark), viewColor(ColorRole::CamLeftThin, dark),
                               viewColor(ColorRole::CamLeftThick, dark), viewColor(ColorRole::CamGouge, dark)};
    m_camSimulate->setRemaining(shaded ? camRemainingText(stock.remaining) : QString(), legend);
  }
}

void MainWindow::showCamSubSpindle(int operation, const QString& name) {
  m_camMachineLines.clear();
  if (!m_camSubSpindle) {
    return;
  }
  // Where its jaws' face is as the operation cuts, on the part where the
  // view draws it; not drawn where that is not known (the main spindle's
  // operations without the work zero's machine place).
  QString logged;
  if (const std::optional<double> face = cam::subSpindleFace(m_camSubSpindle->data, operation)) {
    m_camMachineLines = cam::subSpindleLines(m_camSubSpindle->data, *face, m_camSubSpindle->frame);
    logged = QStringLiteral("CAM sub-spindle drawn: %1: %2 at Z %3, %4 segments")
                 .arg(m_camSubSpindle->setup, name, cam::shortNumber(*face))
                 .arg(static_cast<int>(m_camMachineLines.size() / 6));
  } else {
    logged = QStringLiteral("CAM sub-spindle drawn: %1: %2 where it waits is not known")
                 .arg(m_camSubSpindle->setup, name);
  }
  if (logged != m_loggedCamSubSpindle) {
    m_loggedCamSubSpindle = logged;
    qInfo().noquote() << logged;
  }
}

void MainWindow::showCamMachineParts(int operation, const QString& name, const cam::Point3& tip) {
  if (!m_camMachineDrawn) {
    return;
  }
  // The zones where they are as the operation cuts, after the
  // sub-spindle's lines; the turret's body round the tool's point.
  const QJsonArray zones =
      cam::operationEntry(m_camMachineDrawn->zones, operation).value(QStringLiteral("zones")).toArray();
  const std::vector<float> zoneSegments = cam::zoneLines(zones, m_camMachineDrawn->frame);
  m_camMachineLines.insert(m_camMachineLines.end(), zoneSegments.begin(), zoneSegments.end());
  int moving = 0;
  for (const QJsonValue& value : zones) {
    moving += value.toObject().value(QStringLiteral("moving")).toBool() ? 1 : 0;
  }
  const QString zonesLogged = QStringLiteral("CAM zones drawn: %1: %2: %3 zone(s), %4 moving, %5 segments")
                                  .arg(m_camMachineDrawn->setup, name)
                                  .arg(zones.size())
                                  .arg(moving)
                                  .arg(static_cast<int>(zoneSegments.size() / 6));
  if (zonesLogged != m_loggedCamZones) {
    m_loggedCamZones = zonesLogged;
    qInfo().noquote() << zonesLogged;
    // Each zone with its Z where it is drawn, for the UI tests.
    for (const QJsonValue& value : zones) {
      const QJsonObject zone = value.toObject();
      qInfo().noquote() << QStringLiteral("CAM zone drawn: %1: %2: %3 at Z %4 to %5")
                               .arg(m_camMachineDrawn->setup, name, str(zone, "name"))
                               .arg(cam::shortNumber(zone.value(QStringLiteral("min")).toArray().at(2).toDouble()),
                                    cam::shortNumber(zone.value(QStringLiteral("max")).toArray().at(2).toDouble()));
    }
  }
  const QJsonObject turret = cam::operationEntry(m_camMachineDrawn->turret, operation);
  const std::vector<float> turretSegments = cam::turretLines(turret, tip, m_camMachineDrawn->frame);
  m_camMachineLines.insert(m_camMachineLines.end(), turretSegments.begin(), turretSegments.end());
  // Logged per operation, not per place of the tool.
  QString turretLogged;
  if (turretSegments.empty()) {
    turretLogged = QStringLiteral("CAM turret drawn: %1: %2: none").arg(m_camMachineDrawn->setup, name);
  } else {
    const QJsonArray xs = turret.value(QStringLiteral("x")).toArray();
    const QJsonArray zs = turret.value(QStringLiteral("z")).toArray();
    turretLogged = QStringLiteral("CAM turret drawn: %1: %2: X %3 to %4, Z %5 to %6 from the tool")
                       .arg(m_camMachineDrawn->setup, name, cam::shortNumber(xs.at(0).toDouble()),
                            cam::shortNumber(xs.at(1).toDouble()), cam::shortNumber(zs.at(0).toDouble()),
                            cam::shortNumber(zs.at(1).toDouble()));
  }
  if (turretLogged != m_loggedCamTurret) {
    m_loggedCamTurret = turretLogged;
    qInfo().noquote() << turretLogged;
  }
}

void MainWindow::clearCamStock() {
  const bool shown = m_camStock.has_value() || m_camTurnedStock;
  m_camStock.reset();
  m_camTurnedStock = false;
  // The sub-spindle drawn with it (mitcad#2405), the zones and the turret
  // (mitcad#2412).
  m_camSubSpindle.reset();
  m_camMachineLines.clear();
  m_loggedCamSubSpindle.clear();
  m_camMachineDrawn.reset();
  m_loggedCamZones.clear();
  m_loggedCamTurret.clear();
  if (m_camSimulate != nullptr) {
    m_camSimulate->setRemaining(QString(), {});
  }
  if (shown) {
    m_viewer->setCamStock(cam::StockMesh{});
  }
}

QString MainWindow::camRemainingText(const QJsonObject& remaining) const {
  const double tolerance = remaining.value(QStringLiteral("tolerance")).toDouble(0.01);
  const double most = remaining.value(QStringLiteral("max")).toDouble();
  const double gouge = remaining.value(QStringLiteral("gouge")).toDouble();
  const double beside = remaining.value(QStringLiteral("beside")).toDouble();
  const auto length = [this](double millimetres) { return cmd::lengthText(millimetres, *this); };
  QStringList sentences;
  if (most > tolerance) {
    sentences << tr("Material is left over the part: up to %1, %2 on average.")
                     .arg(length(most), length(remaining.value(QStringLiteral("mean")).toDouble()));
  } else {
    sentences << tr("Nothing is left over the part.");
  }
  if (gouge > 0.0) {
    sentences << tr("The part is cut into by up to %1.").arg(length(gouge));
  }
  if (beside > tolerance) {
    sentences << tr("Material is left beside the part: up to %1.").arg(length(beside));
  }
  return sentences.join(QLatin1Char(' '));
}

} // namespace mitcad
