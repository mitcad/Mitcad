// SPDX-License-Identifier: MIT
// Construction sets (mitcad#2514, GitHub mitcad/Mitcad#656; app/COMMANDS.md
// "Construction Sets"): New Construction Set in the Surface tab's Construct
// group makes a set in the active component and makes it the active set,
// its Browser row then renamed in place; Deactivate Construction Set ends
// it, so that new items go to their usual folders again. The Browser rows
// (BrowserController) do the same, and more: Activate, Rename, Ungroup,
// Move to and Remove from Construction Set.
#include "MainWindow.hpp"

#include <QtLogging>

#include "browser/BrowserController.hpp"
#include "framework/Command.hpp"
#include "framework/CommandRegistry.hpp"

namespace mitcad {

void MainWindow::registerConstructionSetCommands() {
  CommandDef create;
  create.id = QStringLiteral("surface.construction_set");
  create.name = tr("New Construction Set");
  create.icon = QStringLiteral("construction-set");
  create.tooltip = tr("Groups new sketches, construction geometry and surfaces in the Browser");
  create.kind = CommandDef::Kind::Action;
  create.mode = CommandDef::Mode::Model;
  create.keywords = {QStringLiteral("construction set"), QStringLiteral("group"), QStringLiteral("folder"),
                     QStringLiteral("organize"), QStringLiteral("surfacing"), QStringLiteral("guide curves")};
  create.run = [this] {
    if (!canChangeModel()) {
      const QString readOnly = readOnlyReason();
      showError(readOnly.isEmpty() ? changeBlockedReason() : readOnly);
      return;
    }
    m_controller->createConstructionSet();
  };
  m_registry->add(create);

  CommandDef deactivate;
  deactivate.id = QStringLiteral("surface.deactivate_construction_set");
  deactivate.name = tr("Deactivate Construction Set");
  deactivate.icon = QStringLiteral("construction-set");
  deactivate.tooltip = tr("New items go to their usual Browser folders again");
  deactivate.kind = CommandDef::Kind::Action;
  deactivate.mode = CommandDef::Mode::Model;
  deactivate.keywords = {QStringLiteral("construction set"), QStringLiteral("end"), QStringLiteral("stop")};
  deactivate.enabled = [this] { return !m_snapshot.activeConstructionSet().isEmpty(); };
  deactivate.disabledReason = [] { return tr("No construction set is active"); };
  deactivate.run = [this] {
    if (!canChangeModel()) {
      const QString readOnly = readOnlyReason();
      showError(readOnly.isEmpty() ? changeBlockedReason() : readOnly);
      return;
    }
    m_controller->deactivateConstructionSet();
  };
  m_registry->add(deactivate);
}

} // namespace mitcad
