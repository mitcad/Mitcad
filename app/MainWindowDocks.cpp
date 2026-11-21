// SPDX-License-Identifier: MIT
// The Docked layout's docks kept between starts (mitcad#2070, GitHub
// mitcad/Mitcad#451, framework/DockState.hpp): saved at quit beside the
// window's place, restored in the first window of the next start, the kept
// arrangement with its floating docks docked in a new window, and the
// default arrangement again with Restore Default Layout. The model's docks
// hidden while a drawing is open (mitcad#2082).
#include "MainWindow.hpp"

#include <QAction>
#include <QDockWidget>
#include <QSettings>
#include <QSignalBlocker>
#include <QtLogging>

#include <utility>
#include <vector>

#include "browser/BrowserController.hpp"
#include "browser/ConfigurationBar.hpp"
#include "framework/ChromeStyle.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/DockState.hpp"
#include "framework/TestSync.hpp"
#include "render/RenderWorkspace.hpp"

namespace mitcad {

bool MainWindow::keepsDockArrangement() const {
  return !m_nativeUi && m_floating == nullptr && chromeStyle() == ChromeStyle::Docked &&
         findChild<QDockWidget*>(QStringLiteral("browserDock"), Qt::FindDirectChildrenOnly) != nullptr;
}

QStringList MainWindow::dockArrangementLeftOut() const {
  // The Render workspace's docks open at their own areas (mitcad#2013).
  QStringList names;
  for (const QDockWidget* dock : m_renderDocks) {
    names << dock->objectName();
  }
  // So does a tutorial's pane, while its tutorial is open (mitcad#2012).
  names << QStringLiteral("tutorialDock");
  return names;
}

void MainWindow::captureDefaultDocks() {
  if (!m_defaultDockState.isEmpty()) {
    return;
  }
  for (QDockWidget* dock : findChildren<QDockWidget*>(QString(), Qt::FindDirectChildrenOnly)) {
    m_defaultDockAreas.insert(dock->objectName(), dockWidgetArea(dock));
  }
  // Taken before the window is first laid out: the state has no sizes of
  // the dock areas then, and restored it gives them their size hints, as a
  // new window has them.
  m_defaultDockState = saveState(kDockStateVersion);
}

void MainWindow::saveDockArrangement() {
  if (!keepsDockArrangement()) {
    return;
  }
  // The docks the Start page, the Render workspace or a drawing hid are
  // kept as the user had them outside those pages.
  QStringList shownOutside = m_startHiddenDocks;
  for (const QStringList* list : {&m_renderHiddenDocks, &m_drawingHiddenDocks}) {
    for (const QString& name : *list) {
      if (!shownOutside.contains(name)) {
        shownOutside << name;
      }
    }
  }
  const KeptDockState kept = dockStateOf(*this, dockArrangementLeftOut(), shownOutside);
  QSettings settings;
  saveDockState(settings, kept);
  QString detail = dockArrangementDescription(*this, kept.docks);
  if (!kept.shown.isEmpty()) {
    detail += QStringLiteral("; shown at the next start: %1").arg(kept.shown.join(QStringLiteral(", ")));
  }
  qInfo().noquote() << QStringLiteral("Dock state saved: %1").arg(detail);
}

void MainWindow::restoreDockArrangement(bool first, MainWindow* from) {
  if (!keepsDockArrangement()) {
    return;
  }
  // The default arrangement for Restore Default Layout, as the window made
  // its docks.
  captureDefaultDocks();
  // Another window takes the arrangement of the window it opens from, as it
  // is now; the first window the one kept at the last quit.
  const bool fromWindow = !first && from != nullptr && from != this && from->keepsDockArrangement();
  QStringList shownOutside;
  if (fromWindow) {
    shownOutside = from->m_startHiddenDocks;
    shownOutside << from->m_renderHiddenDocks;
    shownOutside << from->m_drawingHiddenDocks;
  }
  const KeptDockState kept = fromWindow ? dockStateOf(*from, from->dockArrangementLeftOut(), shownOutside)
                                        : savedDockState(QSettings());
  if (!kept.valid()) {
    qInfo().noquote() << QStringLiteral("Dock state: none saved, the default arrangement");
    return;
  }
  // The Projects panel as its own setting has it (View › Projects,
  // mitcad#2082), whatever the kept arrangement shows: the dock's show and
  // hide while the state is applied tick View › Projects, which
  // applyFollowedDockVisibility reads.
  QAction* projects = m_registry->action(QStringLiteral("view.projects"));
  const bool projectsShown = projects != nullptr && projects->isChecked();
  const DockRestore result = restoreDockState(*this, kept, m_defaultDockAreas, dockArrangementLeftOut(), first);
  if (projects != nullptr && projects->isChecked() != projectsShown) {
    const QSignalBlocker blocker(projects);
    projects->setChecked(projectsShown);
  }
  if (!result.restored) {
    qInfo().noquote() << QStringLiteral("Dock state ignored: %1; the default arrangement").arg(result.ignored);
    return;
  }
  m_dockStateRestored = true;
  applyFollowedDockVisibility();
  if (!result.misplaced.isEmpty()) {
    // Where Qt left a dock its state does not name elsewhere than at its own
    // area: moved there once the window has shown (not before, see
    // restoreDockState).
    const QStringList names = result.misplaced;
    TestSync::singleShot(500, this, [this, names] {
      if (isVisible()) {
        placeDocksAtTheirAreas(*this, names, m_defaultDockAreas);
        qInfo().noquote() << QStringLiteral("Dock state: %1 back at their own areas").arg(names.join(QStringLiteral(", ")));
      }
    });
  }
  QString detail = dockArrangementDescription(*this, kept.docks);
  for (const QString& docked : result.docked) {
    detail += QStringLiteral(", ") + docked;
  }
  qInfo().noquote() << QStringLiteral("Dock state restored: %1").arg(detail);
}

void MainWindow::applyFollowedDockVisibility() {
  // Shown while the design has options or rows (mitcad#1020).
  if (auto* dock = qobject_cast<QDockWidget*>(m_configurationContainer)) {
    const ConfigurationBar* bar = m_controller != nullptr ? m_controller->configurationBar() : nullptr;
    dock->setVisible(bar != nullptr && bar->isPresent());
  }
  // As View › Projects is checked: projects/panel (mitcad#145).
  if (auto* dock = qobject_cast<QDockWidget*>(m_projectsContainer)) {
    const QAction* action = m_registry->action(QStringLiteral("view.projects"));
    dock->setVisible(action != nullptr && action->isChecked());
  }
  // View › Chat checked as the Chat dock is shown.
  if (auto* dock = qobject_cast<QDockWidget*>(m_chatContainer.data())) {
    if (QAction* action = m_registry->action(QStringLiteral("view.chat"))) {
      action->setChecked(!dock->isHidden());
    }
  }
  // The toolbar as the Start page has it, whatever the kept state says of
  // it (mitcad#2082: kept while the page hid it).
  updateStartToolbar();
}

void MainWindow::restoreDefaultDocks() {
  if (!keepsDockArrangement() || m_defaultDockState.isEmpty()) {
    return;
  }
  auto* chat = qobject_cast<QDockWidget*>(m_chatContainer.data());
  const bool chatShown = chat != nullptr && !chat->isHidden();
  const bool start = startPageShown();
  const bool render = m_renderWorkspace != nullptr && m_renderWorkspace->isOpen();
  const QList<QDockWidget*> docks = findChildren<QDockWidget*>(QString(), Qt::FindDirectChildrenOnly);
  // The docks' own show and hide while they are arranged are no choice of
  // the user's: their close buttons' handlers (View › Projects remembered,
  // View › Chat) do not run meanwhile.
  std::vector<std::pair<QAction*, bool>> blocked;
  for (QDockWidget* dock : docks) {
    QAction* toggle = dock->toggleViewAction();
    blocked.emplace_back(toggle, toggle->blockSignals(true));
  }
  const bool restored = restoreState(m_defaultDockState, kDockStateVersion);
  if (restored) {
    for (QDockWidget* dock : docks) {
      if (dock->isFloating()) {
        dock->setFloating(false);
      }
    }
    // The Chat panel stays as View › Chat has it; the panels that follow the
    // design and View › Projects as they say.
    if (chat != nullptr) {
      chat->setVisible(chatShown);
    }
    applyFollowedDockVisibility();
    // The page in front keeps the view to itself: the docks the default
    // arrangement shows are hidden behind it, and come back when it closes.
    if (start) {
      m_startHiddenDocks.clear();
      for (QDockWidget* dock : docks) {
        if (!dock->isHidden()) {
          m_startHiddenDocks << dock->objectName();
          dock->hide();
        }
      }
    } else if (render) {
      m_renderHiddenDocks.clear();
      for (QDockWidget* dock : docks) {
        if (m_renderDocks.contains(dock)) {
          dock->show();
        } else if (!dock->isHidden()) {
          m_renderHiddenDocks << dock->objectName();
          dock->hide();
        }
      }
    } else if (m_drawingDocksHidden) {
      // The drawing keeps the model's docks the default arrangement shows.
      m_drawingHiddenDocks.clear();
      for (QDockWidget* dock : docks) {
        if (drawingHiddenDockNames().contains(dock->objectName()) && !dock->isHidden()) {
          m_drawingHiddenDocks << dock->objectName();
          dock->hide();
        }
      }
    }
  }
  for (const auto& [toggle, was] : blocked) {
    toggle->blockSignals(was);
  }
  if (!restored) {
    qWarning().noquote() << QStringLiteral("Dock state: the default arrangement did not restore");
    return;
  }
  QStringList names;
  for (const QDockWidget* dock : docks) {
    if (!m_renderDocks.contains(dock)) {
      names << dock->objectName();
    }
  }
  qInfo().noquote() << QStringLiteral("Dock state: the default arrangement restored: %1")
                           .arg(dockArrangementDescription(*this, names));
  // The view moved: its layout logged again for the UI tests.
  if (m_loggedWindowSize.isValid()) {
    TestSync::singleShot(0, this, [this] { logLayout(); });
  }
}

QStringList MainWindow::drawingHiddenDockNames() {
  return {QStringLiteral("commandDock"), QStringLiteral("timelineDock"), QStringLiteral("projectsDock"),
          QStringLiteral("configurationDock")};
}

void MainWindow::updateDrawingDocks(bool open) {
  // The Floating layout shows the Drawings card alone (showOnlyCards);
  // another drawing opened over one keeps the docks as they are.
  if (m_nativeUi || m_floating != nullptr || open == m_drawingDocksHidden) {
    return;
  }
  m_drawingDocksHidden = open;
  QStringList changed;
  if (open) {
    m_drawingHiddenDocks.clear();
    // As the Start page and the Render workspace hide them: their View ›
    // Panels entries follow, and come back with them.
    for (const QString& name : drawingHiddenDockNames()) {
      auto* dock = findChild<QDockWidget*>(name, Qt::FindDirectChildrenOnly);
      if (dock == nullptr) {
        continue;
      }
      // One the Start page (or Render) still hides is the drawing's to
      // show again, not the page's.
      if (m_startHiddenDocks.removeAll(name) > 0 || m_renderHiddenDocks.removeAll(name) > 0) {
        m_drawingHiddenDocks << name;
        continue;
      }
      if (!dock->isHidden()) {
        dock->hide();
        m_drawingHiddenDocks << name;
      }
    }
    changed = m_drawingHiddenDocks;
    qInfo().noquote() << QStringLiteral("Drawing docks hidden: %1")
                             .arg(changed.isEmpty() ? QStringLiteral("none") : changed.join(QStringLiteral(", ")));
  } else {
    changed.swap(m_drawingHiddenDocks);
    showDocksAgain(changed);
    qInfo().noquote() << QStringLiteral("Drawing docks shown again: %1")
                             .arg(changed.isEmpty() ? QStringLiteral("none") : changed.join(QStringLiteral(", ")));
  }
}

void MainWindow::showDocksAgain(const QStringList& names) {
  const bool start = startPageShown();
  const bool render = m_renderWorkspace != nullptr && m_renderWorkspace->isOpen();
  for (const QString& name : names) {
    auto* dock = findChild<QDockWidget*>(name, Qt::FindDirectChildrenOnly);
    if (dock == nullptr) {
      continue;
    }
    QStringList* keeper = nullptr;
    if (m_drawingDocksHidden && drawingHiddenDockNames().contains(name)) {
      keeper = &m_drawingHiddenDocks;
    } else if (start) {
      keeper = &m_startHiddenDocks;
    } else if (render && !m_renderDocks.contains(dock)) {
      keeper = &m_renderHiddenDocks;
    }
    if (keeper != nullptr) {
      if (!keeper->contains(name)) {
        *keeper << name;
      }
      continue;
    }
    dock->show();
  }
}

} // namespace mitcad
