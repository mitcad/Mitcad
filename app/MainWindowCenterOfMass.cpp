// SPDX-License-Identifier: MIT
// The Center of Mass marker (mitcad#1094; app/COMMANDS.md "Center of
// Mass"): Center of Mass in the Solid tab's Inspect group adds the marker
// (or shows it again), one undo step of display state as the Origin
// folder's light bulb is; the browser's row under Origin shows, hides and
// deletes it (BrowserController). After every refresh the view draws it
// where the snapshot's `center_of_mass` answer puts the design's center of
// mass, the bodies placed by their occurrences and with their materials,
// so that it follows every recompute, placement and material.
#include "MainWindow.hpp"

#include <array>
#include <optional>

#include <QJsonObject>
#include <QStringList>
#include <QtLogging>

#include "OcctViewer.hpp"
#include "framework/Command.hpp"
#include "framework/CommandRegistry.hpp"
#include "view/CenterOfMass.hpp"

namespace mitcad {

void MainWindow::registerCenterOfMassCommand() {
  CommandDef def;
  def.id = QStringLiteral("inspect.center_of_mass");
  def.name = tr("Center of Mass");
  def.icon = QStringLiteral("center-of-mass");
  def.tooltip = tr("Shows a marker at the center of mass of the design's bodies");
  def.kind = CommandDef::Kind::Action;
  def.tab = QStringLiteral("SOLID");
  def.group = QStringLiteral("INSPECT");
  def.keywords = {QStringLiteral("center of mass"), QStringLiteral("centre of mass"), QStringLiteral("balance"),
                  QStringLiteral("centroid"), QStringLiteral("mass"), QStringLiteral("weight")};
  def.enabled = [this] { return !m_snapshot.bodies.isEmpty(); };
  def.run = [this] {
    if (!canChangeModel(true)) {
      const QString readOnly = readOnlyReason();
      showError(readOnly.isEmpty() ? changeBlockedReason() : readOnly);
      return;
    }
    if (runModelCommand({{QStringLiteral("cmd"), QStringLiteral("add_center_of_mass")}})) {
      qDebug().noquote() << QStringLiteral("Center of Mass added");
    }
  };
  m_registry->add(def);
}

void MainWindow::followCenterOfMass() {
  const std::optional<CenterOfMassShown> center = centerOfMassOf(m_snapshot.centerOfMass);
  std::optional<gp_Pnt> at;
  QString tip;
  if (center) {
    tip = centerOfMassTip(*center);
    if (center->visible && center->point) {
      const std::array<double, 3>& point = *center->point;
      at = gp_Pnt(point[0], point[1], point[2]);
    }
  }
  m_viewer->setCenterOfMass(at, tip);
  // For UI tests: where it is, and its tooltip, when they change.
  QString logged = centerOfMassLog(center);
  if (center) {
    logged += QStringLiteral("; tooltip: ") + tip;
  }
  if (logged != m_loggedCenterOfMass) {
    m_loggedCenterOfMass = logged;
    qDebug().noquote() << logged;
  }
}

} // namespace mitcad
