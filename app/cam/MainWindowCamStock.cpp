// SPDX-License-Identifier: MIT
// Simulate's stock (mitcad#1013, phase 3; app/COMMANDS.md "The CNC
// group", MainWindow.hpp): the setup's program cut out of its stock by the
// model (cam_simulate), read once with snapshots at places along
// Simulate's playback; the stock as cut drawn at the slider's place, and
// at the end shaded by the material it leaves over the part, with the
// material left in words and the legend under the slider.
//
// Messages a person sees say what is wrong in words; the log lines carry
// the numbers UI tests read.
#include "../MainWindow.hpp"
#include "../framework/Json.hpp"

#include <algorithm>
#include <cstddef>
#include <exception>
#include <initializer_list>
#include <utility>

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

void MainWindow::clearCamStock() {
  const bool shown = m_camStock.has_value();
  m_camStock.reset();
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
