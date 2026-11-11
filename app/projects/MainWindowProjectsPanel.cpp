// SPDX-License-Identifier: MIT
// The Projects panel in the window (mitcad#145): its dock (Docked) or card
// (Floating), View > Projects, File > New Design, and what the panel asks
// for: opening a design, New Design, New Folder, renaming, moving and
// deleting designs and folders (the core's new_design, create_folder,
// move and delete, each a version, sent as Save's versions are), Version
// History, Project Settings and Sync Now of a project, Add Existing Project
// Folder and Remove from List. The panel itself is projects/ProjectsPanel.
#include "../MainWindow.hpp"
#include "../WindowManager.hpp"

#include <algorithm>
#include <utility>
#include <vector>

#include <QAction>
#include <QApplication>
#include <QDir>
#include <QDockWidget>
#include <QFileInfo>
#include <QGuiApplication>
#include <QJsonArray>
#include <QMenu>
#include <QMessageBox>
#include <QPointer>
#include <QPushButton>
#include <QSignalBlocker>
#include <QTimer>
#include <QToolButton>
#include <QUrl>
#include <QtLogging>

#include "../browser/BrowserController.hpp"
#include "../browser/BrowserPanel.hpp"
#include "../files/Autosave.hpp"
#include "../files/CloudAddress.hpp"
#include "../files/LiveController.hpp"
#include "../files/LockController.hpp"
#include "../files/ProjectIndicator.hpp"
#include "../files/ProjectSettings.hpp"
#include "../files/Projects.hpp"
#include "../files/RemoteController.hpp"
#include "../files/RemoteTask.hpp"
#include "../framework/AppSettings.hpp"
#include "../framework/ChromeStyle.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/FloatingLayout.hpp"
#include "../framework/GlassCard.hpp"
#include "../framework/Json.hpp"
#include "../framework/PlaceWatcher.hpp"
#include "../framework/TestSync.hpp"
#include "../report/ReportCenter.hpp"
#include "ProjectsPanel.hpp"
#include "ProjectsTree.hpp"

namespace mitcad {
namespace {

// Whether `path` is `folder` or inside it.
bool atOrInside(const QString& path, const QString& folder) {
  const QString a = QDir::cleanPath(path);
  const QString b = QDir::cleanPath(folder);
#ifdef _WIN32
  const Qt::CaseSensitivity cs = Qt::CaseInsensitive;
#else
  const Qt::CaseSensitivity cs = Qt::CaseSensitive;
#endif
  return a.compare(b, cs) == 0 || a.startsWith(b + QLatin1Char('/'), cs);
}

} // namespace

// ---------------------------------------------------------------------------
// Commands

void MainWindow::registerProjectsPanelCommands() {
  CommandDef panel;
  panel.id = QStringLiteral("view.projects");
  panel.name = tr("Projects");
  panel.icon = QStringLiteral("new-project");
  panel.tooltip = tr("Shows or hides the Projects panel: your projects, their folders and designs");
  panel.kind = CommandDef::Kind::Action;
  panel.mode = CommandDef::Mode::Any;
  panel.tab.clear(); // the View menu
  panel.duringCommands = true;
  panel.keywords = {QStringLiteral("projects"), QStringLiteral("panel"), QStringLiteral("designs"),
                    QStringLiteral("folders"), QStringLiteral("files")};
  panel.run = [this] { setProjectsPanelShown(!projectsPanelShown(), true); };
  m_registry->add(panel);
  CommandDef design;
  design.id = QStringLiteral("file.new_design");
  design.name = tr("New Design…");
  design.icon = QStringLiteral("add");
  design.tooltip = tr("A new design in the current project's selected folder, or an untitled one");
  design.kind = CommandDef::Kind::Action;
  design.mode = CommandDef::Mode::Any;
  design.tab.clear(); // the File menu
  // New and New Design are one command (mitcad#175).
  design.shortcut = QKeySequence(QKeySequence::New);
  design.keywords = {QStringLiteral("design"), QStringLiteral("part"),     QStringLiteral("project"),
                     QStringLiteral("folder"), QStringLiteral("new"),      QStringLiteral("untitled"),
                     QStringLiteral("document"), QStringLiteral("blank")};
  design.run = [this] { newDesign(); };
  m_registry->add(design);
}

// ---------------------------------------------------------------------------
// The panel, its dock or card

void MainWindow::createProjectsPanel() {
  if (m_projectsPanel != nullptr) {
    return;
  }
  ProjectsPanel::Host host;
  host.session = [this] { return m_autosave != nullptr ? m_autosave->session() : QString(); };
  host.openDesign = [this] { return m_filePath; };
  host.currentRoot = [this] { return m_project.isProject() ? m_project.root : QString(); };
  host.currentRemote = [this] { return m_remote != nullptr ? m_remote->indicatorState() : ProjectIndicator::Remote(); };
  host.showForNaming = [this](bool on) { showProjectsForNaming(on); };
  m_projectsPanel = new ProjectsPanel(std::move(host));
  ProjectsPanel* panel = m_projectsPanel;
  // Queued: the panel's menus, editors and drags end before a dialog shows.
  const Qt::ConnectionType queued = Qt::QueuedConnection;
  connect(panel, &ProjectsPanel::openRequested, this, [this](const QString& path) { openPanelDesign(path); }, queued);
  connect(panel, &ProjectsPanel::newDesignRequested, this,
          [this](const QString& root, const QString& path) { createDesignInProject(root, path); }, queued);
  connect(panel, &ProjectsPanel::newFolderRequested, this,
          [this](const QString& root, const QString& path) { createFolderInProject(root, path); }, queued);
  connect(panel, &ProjectsPanel::moveRequested, this,
          [this](const QString& root, const QString& from, const QString& to, bool folder) {
            moveInProject(root, from, to, folder);
          },
          queued);
  connect(panel, &ProjectsPanel::deleteRequested, this,
          [this](const QString& root, const QString& path, bool folder) { deleteInProject(root, path, folder); },
          queued);
  connect(panel, &ProjectsPanel::versionHistoryRequested, this,
          [this](const QString& root, const QString& path) { panelVersionHistory(root, path); }, queued);
  connect(panel, &ProjectsPanel::showInFolderRequested, this, [](const QString& folder) {
    qInfo().noquote() << QStringLiteral("Projects panel: show in folder %1").arg(folder);
    openExternalUrl(QUrl::fromLocalFile(folder));
  });
  connect(panel, &ProjectsPanel::projectSettingsRequested, this,
          [this](const QString& root) { panelProjectSettings(root); }, queued);
  connect(panel, &ProjectsPanel::syncRequested, this,
          [this](const QString& root) { panelProjectAction(root, QStringLiteral("file.sync")); }, queued);
  connect(panel, &ProjectsPanel::openProjectRequested, this,
          [this](const QString& root) { openProjectFolder(root, true); }, queued);
  connect(panel, &ProjectsPanel::commandRequested, this, [this](const QString& id) { trigger(id); }, queued);
  connect(panel, &ProjectsPanel::addFolderRequested, this, [this] { addExistingProjectFolder(); }, queued);
  connect(panel, &ProjectsPanel::removeFromListRequested, this, [this](const QString& root) {
    ProjectsSettings::forget(root);
    qInfo().noquote() << QStringLiteral("Projects panel: removed %1 from the list").arg(root);
    showHint(tr("%1 is no longer in the Projects panel; its folder stays as it is. Open Project or Add Existing "
                "Project Folder brings it back.")
                 .arg(QFileInfo(root).fileName()));
    refreshProjectsPanel();
  });
  connect(panel, &ProjectsPanel::removeProjectRequested, this,
          [this](const QString& root) { removeProject(root); }, queued);
  connect(panel, &ProjectsPanel::statusMessage, this, [this](const QString& message, bool error) {
    if (error) {
      showError(message);
    } else {
      showHint(message);
    }
  });
  // Read again when the window gets the focus back (files changed by
  // another program, another Mitcad or git), and after versions went out
  // or came in; the current project's sync state as the indicator's.
  connect(qApp, &QGuiApplication::applicationStateChanged, this, [this](Qt::ApplicationState state) {
    if (state == Qt::ApplicationActive) {
      refreshProjectsPanel();
    }
  });
  connect(m_remote, &RemoteController::stateChanged, panel, &ProjectsPanel::updateMarks);
  connect(m_remote, &RemoteController::versionsSent, this, [this] { refreshProjectsPanel(); });
  if (QAction* action = m_registry->action(QStringLiteral("view.projects"))) {
    action->setCheckable(true);
    if (QMenu* view = findChild<QMenu*>(QStringLiteral("viewMenu"))) {
      view->addAction(action);
    }
  }
}

void MainWindow::dockProjectsPanel(QDockWidget* below) {
  createProjectsPanel();
  if (m_projectsPanel == nullptr) {
    return;
  }
  auto* dock = new QDockWidget(tr("Projects"), this);
  dock->setObjectName(QStringLiteral("projectsDock"));
  dock->setWidget(m_projectsPanel);
  addDockWidget(Qt::LeftDockWidgetArea, dock);
  // Above the browser, in the same column: the view keeps its width (the
  // Configuration dock goes between them, mitcad#1020).
  splitDockWidget(dock, below, Qt::Vertical);
  m_projectsContainer = dock;
  m_projectsPanel->watchContainer(dock);
  const bool shown = projectsPanelShownAtStart();
  if (QAction* action = m_registry->action(QStringLiteral("view.projects"))) {
    action->setChecked(shown);
  }
  dock->setVisible(shown);
  // Its width as the user left it (mitcad#529): remembered when a drag of
  // its edge rests, restored once the window is laid out.
  connect(m_projectsPanel, &ProjectsPanel::dockWidthChosen, this, [](int width) {
    ProjectsSettings settings = ProjectsSettings::load();
    if (settings.dockWidth != width) {
      settings.dockWidth = width;
      settings.save();
      qInfo().noquote() << QStringLiteral("Projects panel: dock width %1 remembered").arg(width);
    }
  });
  TestSync::singleShot(100, this, [this] { restoreProjectsDockWidth(); });
  // Closed with the dock's own button: as View > Projects.
  // Not while the window closes or goes (its docks hide with it).
  connect(dock->toggleViewAction(), &QAction::toggled, this, [this](bool on) {
    if (!isVisible()) {
      return;
    }
    QAction* action = m_registry->action(QStringLiteral("view.projects"));
    if (action != nullptr && action->isChecked() != on) {
      setProjectsPanelShown(on, true);
    }
  });
}

void MainWindow::addProjectsCard() {
  createProjectsPanel();
  if (m_projectsPanel == nullptr || m_floating == nullptr) {
    return;
  }
  auto* card = new GlassCard;
  card->setObjectName(QStringLiteral("projectsDock"));
  card->setTitle(tr("Projects"));
  card->setCollapsible(true);
  card->setContent(m_projectsPanel, QMargins(6, 0, 6, 8));
  // Its width dragged by its right edge and remembered (mitcad#529).
  card->setWidthAdjustable(ProjectsPanel::kMinimumCardWidth);
  const int remembered = ProjectsSettings::load().cardWidth;
  if (remembered > 0) {
    qInfo().noquote() << QStringLiteral("Projects panel: card width %1 (remembered)").arg(remembered);
  }
  connect(card, &GlassCard::widthChosen, this, [card] {
    // As the layout placed it (within the view).
    ProjectsSettings settings = ProjectsSettings::load();
    settings.cardWidth = card->slot().width();
    settings.save();
    qInfo().noquote() << QStringLiteral("Projects panel: card width %1 remembered").arg(settings.cardWidth);
  });
  const bool shown = projectsPanelShownAtStart();
  // Above the browser's card (FloatingLayout stacks the upper left cards),
  // sharing the left column with it (mitcad#1020: no cap of its own; it
  // had 40 % of the view).
  m_floating->addCard(card, FloatingLayout::Anchor::TopLeft,
                      {remembered > 0 ? std::max(remembered, ProjectsPanel::kMinimumCardWidth) : 260,
                       [panel = m_projectsPanel] { return panel->contentHeight(); }, 1.0, shown});
  connect(m_projectsPanel, &ProjectsPanel::contentHeightChanged, m_floating, &FloatingLayout::requestLayout);
  // Folded and unfolded by its chevron or for a name typed in it
  // (mitcad#1268); where the chevron is, for UI tests: "Projects panel
  // chevron at x,y" (main window coordinates) when it moved.
  connect(card, &GlassCard::collapsedChanged, this, [card](bool now) {
    // Also emitted when the header changes: only a change is logged.
    if (card->property("loggedCollapsed").toBool() != now) {
      card->setProperty("loggedCollapsed", now);
      qInfo().noquote() << (now ? QStringLiteral("Projects panel: card collapsed")
                                : QStringLiteral("Projects panel: card expanded"));
    }
  });
  if (auto* chevron = card->findChild<QToolButton*>(QStringLiteral("cardChevron"))) {
    new PlaceWatcher(chevron, [chevron, logged = QString()]() mutable {
      if (!chevron->isVisible()) {
        return;
      }
      const QPoint at = GlassCard::mapToHost(chevron, chevron->rect().center());
      const QString line = QStringLiteral("Projects panel chevron at %1,%2").arg(at.x()).arg(at.y());
      if (line != logged) {
        logged = line;
        qDebug().noquote() << line;
      }
    });
  }
  m_projectsContainer = card;
  m_projectsPanel->watchContainer(card);
  if (QAction* action = m_registry->action(QStringLiteral("view.projects"))) {
    action->setChecked(shown);
  }
}

bool MainWindow::projectsPanelShownAtStart() const {
  const ProjectsSettings settings = ProjectsSettings::load();
  if (settings.panelShown.has_value()) {
    qInfo().noquote() << QStringLiteral("Projects panel at the start: %1 (as chosen)")
                             .arg(*settings.panelShown ? QStringLiteral("shown") : QStringLiteral("hidden"));
    return *settings.panelShown;
  }
  // No choice yet: hidden (owner 31.10.2026, mitcad#1020: File > Open from
  // Project does the same job; mitcad#151 showed it when there was a
  // project to list). Logged with whether there is one.
  const bool any = anyProjectConfigured(settings.known, projectsDirectory(), settings.hidden);
  qInfo().noquote() << (any ? QStringLiteral("Projects panel at the start: hidden (no choice yet, projects configured)")
                            : QStringLiteral("Projects panel at the start: hidden (no choice yet, no projects)"));
  return false;
}

bool MainWindow::projectsPanelShown() const {
#ifdef MITCAD_MACOS_NATIVE_UI
  if (m_nativeUi) return m_nativeProjectsShown;
#endif
  if (m_projectsContainer == nullptr) {
    return false;
  }
  if (auto* card = qobject_cast<GlassCard*>(m_projectsContainer)) {
    return card->isShown();
  }
  return !m_projectsContainer->isHidden();
}

void MainWindow::setProjectsPanelShown(bool shown, bool remember) {
  if (m_projectsContainer == nullptr) {
    return;
  }
  if (remember) {
    ProjectsSettings settings = ProjectsSettings::load();
    if (!settings.panelShown.has_value() || *settings.panelShown != shown) {
      settings.panelShown = shown;
      settings.save();
      qInfo().noquote() << QStringLiteral("Projects panel: %1 at the next start")
                               .arg(shown ? QStringLiteral("shown") : QStringLiteral("hidden"));
    }
  }
  if (QAction* action = m_registry->action(QStringLiteral("view.projects"))) {
    const QSignalBlocker blocker(action);
    action->setChecked(shown);
  }
  if (shown == projectsPanelShown()) {
    return;
  }
#ifdef MITCAD_MACOS_NATIVE_UI
  if (m_nativeUi) {
    m_nativeProjectsShown = shown;
    m_projectsPanel->setNativePresented(shown);
    refreshNativeProjects();
    return;
  }
#endif
  if (auto* card = qobject_cast<GlassCard*>(m_projectsContainer)) {
    card->setVisible(shown);
    m_floating->requestLayout();
  } else {
    m_projectsContainer->setVisible(shown);
    if (shown) {
      // A third of the column, the browser the rest.
      if (auto* browser = findChild<QDockWidget*>(QStringLiteral("browserDock"))) {
        auto* dock = static_cast<QDockWidget*>(m_projectsContainer);
        QTimer::singleShot(0, this, [this, dock, browser] {
          const int total = dock->height() + browser->height();
          resizeDocks({dock, browser}, {total / 3, total - total / 3}, Qt::Vertical);
          restoreProjectsDockWidth();
        });
      }
    }
  }
  qInfo().noquote() << QStringLiteral("Projects panel: %1").arg(shown ? QStringLiteral("shown") : QStringLiteral("hidden"));
  // The browser below it moved: where its rows are now (UI tests).
  TestSync::singleShot(150, this, [this] {
    if (m_controller != nullptr) {
      m_controller->browser()->logLayout();
    }
  });
}

void MainWindow::restoreProjectsDockWidth() {
  auto* dock = qobject_cast<QDockWidget*>(m_projectsContainer);
  const int width = ProjectsSettings::load().dockWidth;
  if (dock == nullptr || width <= 0 || dock->isHidden() || dock->isFloating()) {
    return;
  }
  // The column it shares with the browser's dock takes the width.
  resizeDocks({dock}, {std::max(width, ProjectsPanel::kMinimumCardWidth)}, Qt::Horizontal);
  qInfo().noquote() << QStringLiteral("Projects panel: dock width %1 restored").arg(width);
}

void MainWindow::showProjectInPanel(const QString& root) {
  if (m_projectsPanel == nullptr || root.isEmpty()) {
    return;
  }
  setProjectsPanelShown(true, true);
  m_projectsPanel->reveal(root);
}

void MainWindow::refreshProjectsPanel() {
  if (m_projectsPanel != nullptr) {
    m_projectsPanel->updateMarks();
    m_projectsPanel->refresh();
  }
}

QString MainWindow::projectsPanelFolder() const {
  if (m_projectsPanel == nullptr || !m_project.hasHistory()) {
    return {};
  }
  const QString root = m_projectsPanel->selectedRoot();
  if (root.isEmpty() || !sameFolderPath(root, m_project.root)) {
    return {};
  }
  return QDir::cleanPath(QDir(root).filePath(m_projectsPanel->selectedFolder()));
}

// ---------------------------------------------------------------------------
// New Design

void MainWindow::newDesign() {
  if (!m_project.hasHistory()) {
    // New and New Design are one command (mitcad#175): without a current
    // project an untitled design, as New was.
    qInfo().noquote() << QStringLiteral("New Design: no project, an untitled design");
    newDocument();
    return;
  }
  const QString root = m_project.root;
  QString folder;
  if (m_projectsPanel != nullptr) {
    // The panel's selected folder when it is the current project's.
    const QString selected = m_projectsPanel->selectedRoot();
    const ProjectRow* row = m_projectsPanel->project(selected);
    const bool usable =
        row != nullptr && (row->kind == ProjectRow::Kind::Local || row->kind == ProjectRow::Kind::Cloud);
    if (usable && sameFolderPath(selected, root)) {
      folder = m_projectsPanel->selectedFolder();
    }
  }
  qInfo().noquote() << QStringLiteral("New Design: in %1%2")
                           .arg(root, folder.isEmpty() ? QString() : QStringLiteral(", folder %1").arg(folder));
  if (m_projectsPanel != nullptr) {
    // Named in place in the panel, which is shown for it and put back
    // afterwards (showProjectsForNaming, mitcad#1268).
    m_projectsPanel->startNewDesign(root, folder);
    return;
  }
  // Without the panel: the name asked. Both frontends have it; the AppKit
  // one names the design in its native outline (mitcad#816).
  bool accepted = false;
  QString name =
      dialogText(this, tr("New Design"), tr("The new design's name:"), tr("Design1"), &accepted).trimmed();
  if (!accepted || name.isEmpty()) {
    qInfo().noquote() << QStringLiteral("New Design cancelled");
    return;
  }
  if (!name.endsWith(QStringLiteral(".mitcad"), Qt::CaseInsensitive)) {
    name += QStringLiteral(".mitcad");
  }
  createDesignInProject(root, joinPath(folder, name));
}

// ---------------------------------------------------------------------------
// What the panel asks for

std::optional<QJsonObject> MainWindow::projectWrite(const QString& root, QJsonObject command, const QString& title,
                                                    QJsonObject* refused) {
  const QString name = command.value(QStringLiteral("cmd")).toString();
  const bool current = m_project.hasHistory() && sameFolderPath(m_project.root, root);
  if (current) {
    // A sync moves the project's branch: the change waits for it, as Save does.
    m_remote->waitForBranch();
  }
  QJsonObject answer;
  try {
    const rust::Box<Project> project = open_project(rustStr(root.toUtf8()));
    const std::optional<QString> author = versionAuthor(*project);
    if (!author) {
      qInfo().noquote() << QStringLiteral("Projects panel: %1 cancelled (no author)").arg(name);
      return std::nullopt;
    }
    command.insert(QStringLiteral("author"), *author);
    // Sent below as a saved version is.
    command.insert(QStringLiteral("push"), false);
    if (m_autosave != nullptr) {
      // Whose edit locks are this window's.
      command.insert(QStringLiteral("session"), m_autosave->session());
    }
    answer = parseObject(project->command(rustStr(compactJson(command))));
  } catch (const std::exception& e) {
    const QString message = tr("%1 failed: %2").arg(title, errorText(e));
    qWarning().noquote() << QStringLiteral("Projects panel: %1 failed: %2").arg(name, errorText(e));
    sheetWarning(this, title, message);
    return std::nullopt;
  }
  const QString cls = errorClassOf(answer);
  if (!cls.isEmpty()) {
    qInfo().noquote() << QStringLiteral("Projects panel: %1 refused (%2): %3").arg(name, cls, errorMessageOf(answer));
    if (refused != nullptr) {
      *refused = answer;
    } else {
      sheetWarning(this, title, errorMessageOf(answer));
    }
    return std::nullopt;
  }
  const QString commit = answer.value(QStringLiteral("commit")).toString();
  qInfo().noquote() << QStringLiteral("Projects panel: %1 recorded%2")
                           .arg(name, commit.isEmpty() ? QStringLiteral(" (no new version)")
                                                       : QStringLiteral(" as %1").arg(commit.left(7)));
  for (const QJsonValue& warning : answer.value(QStringLiteral("warnings")).toArray()) {
    qWarning().noquote() << QStringLiteral("Version warning: %1").arg(warning.toString());
  }
  if (current) {
    // Sent at once (unless turned off), as Save's versions are.
    m_remote->versionRecorded(root);
  } else {
    sendPanelProjectVersions(root);
  }
  return answer;
}

void MainWindow::sendPanelProjectVersions(const QString& root) {
  if (m_projectsPanel == nullptr) {
    return;
  }
  const ProjectRow* row = m_projectsPanel->project(root);
  if (row == nullptr || row->kind != ProjectRow::Kind::Cloud || !RemoteSettings::load().autoPush) {
    return;
  }
  // Another Cloud project's version: sent in the background.
  RemoteTask* task = RemoteTask::command(root, {{QStringLiteral("cmd"), QStringLiteral("push")}}, this);
  connect(task, &RemoteTask::finished, this, [this, task, root] {
    const QJsonObject pushed = task->answer();
    task->deleteLater();
    const QString failure = errorClassOf(pushed);
    qInfo().noquote() << (failure.isEmpty() ? QStringLiteral("Projects panel: sent the versions of %1").arg(root)
                                            : QStringLiteral("Projects panel: the versions of %1 wait (%2): %3")
                                                  .arg(root, failure, errorMessageOf(pushed)));
    refreshProjectsPanel();
  });
  task->start();
}

void MainWindow::createDesignInProject(const QString& root, const QString& path) {
  const bool replacing = m_windowManager == nullptr || GeneralSettings::load().openInCurrentWindow;
  if (replacing && !maybeSave()) {
    qInfo().noquote() << QStringLiteral("New Design cancelled");
    return;
  }
  const QString file = QDir::cleanPath(QDir(root).filePath(path));
  // The open design's versions are sent and its edit lock released before
  // the new design is made (mitcad#175): a release that needs a choice
  // (Resolve Sync, Keep Lock) keeps the design and makes nothing.
  const bool wasReadOnly = windowReadOnly();
  if (replacing && !m_locks->designClosing(file, false)) {
    qInfo().noquote() << QStringLiteral("New Design cancelled");
    return;
  }
  // In the default units for new designs (Preferences > General,
  // mitcad#1036): the file is made in them, and opens in them.
  const QString units = GeneralSettings::load().newDesignUnits;
  QJsonObject refused;
  const std::optional<QJsonObject> answer =
      projectWrite(root,
                   {{QStringLiteral("cmd"), QStringLiteral("new_design")},
                    {QStringLiteral("path"), path},
                    {QStringLiteral("units"), units}},
                   tr("New Design"), &refused);
  refreshProjectsPanel();
  if (!answer) {
    // Refused: the open design stays, and takes its lock again; a name
    // that is there already is typed again in the panel (mitcad#1268).
    if (replacing) m_locks->designOpened(m_filePath, wasReadOnly);
    projectNameRefused(root, path, QString(), tr("New Design"), refused);
    return;
  }
  qInfo().noquote() << QStringLiteral("New Design: made %1 in %2").arg(file, units);
  QString error;
  // With its Component1 active, as the file stores it (mitcad#844): its
  // first sketch and feature go into it, as in the untitled new design.
  // In this window it replaces the design already asked about above: no
  // second question, which could leave the new design made but unopened
  // (mitcad#1317).
  m_openingNewDesign = true;
  MainWindow* target = m_windowManager != nullptr ? m_windowManager->openFile(this, file, error, replacing) : nullptr;
  const bool opened = m_windowManager != nullptr ? target != nullptr : openFile(file, error);
  m_openingNewDesign = false;
  if (!opened) {
    if (replacing) m_locks->designOpened(m_filePath, wasReadOnly); // the open design stays
    if (!error.isEmpty()) {
      sheetWarning(this, tr("New Design"), tr("Could not open %1:\n%2").arg(QDir::toNativeSeparators(file), error));
    }
    return;
  }
  (target != nullptr ? target : this)->showHint(tr("%1 is a new design of %2: every save records a version.")
               .arg(QFileInfo(file).fileName(), QFileInfo(root).fileName()));
}

void MainWindow::createFolderInProject(const QString& root, const QString& path) {
  QJsonObject refused;
  const std::optional<QJsonObject> answer = projectWrite(
      root, {{QStringLiteral("cmd"), QStringLiteral("create_folder")}, {QStringLiteral("path"), path}},
      tr("New Folder"), &refused);
  refreshProjectsPanel();
  if (answer) {
    showHint(tr("The folder %1 is in %2.").arg(path, QFileInfo(root).fileName()));
  } else {
    projectNameRefused(root, path, QString(), tr("New Folder"), refused);
  }
}

void MainWindow::offerNewFolder(ProjectPickerOptions& options) {
  const QPointer<MainWindow> self(this);
  options.makeFolder = [self](const QString& root, const QString& path) {
    ProjectFolderMade made;
    if (!self) {
      return made;
    }
    QJsonObject refused;
    const std::optional<QJsonObject> answer = self->projectWrite(
        root, {{QStringLiteral("cmd"), QStringLiteral("create_folder")}, {QStringLiteral("path"), path}},
        MainWindow::tr("New Folder"), &refused);
    // The panel shows the folder, also when the picker is cancelled.
    self->refreshProjectsPanel();
    made.made = answer.has_value();
    if (!made.made) {
      made.errorClass = errorClassOf(refused);
      made.message = errorMessageOf(refused);
    }
    return made;
  };
}

void MainWindow::moveInProject(const QString& root, const QString& from, const QString& to, bool folder) {
  const bool rename = from.section(QLatin1Char('/'), 0, -2) == to.section(QLatin1Char('/'), 0, -2);
  QJsonArray openDesigns;
  QJsonArray modifiedDesigns;
  std::vector<std::pair<QPointer<MainWindow>, QString>> windows;
  QString canonicalRoot = QFileInfo(root).canonicalFilePath();
  if (canonicalRoot.isEmpty()) {
    canonicalRoot = QFileInfo(root).absoluteFilePath();
  }
  QList<MainWindow*> documents;
  if (m_windowManager != nullptr) documents = m_windowManager->windows();
  else for (QWidget* widget : QApplication::topLevelWidgets()) {
    if (auto* window = qobject_cast<MainWindow*>(widget)) documents.append(window);
  }
  for (MainWindow* window : documents) {
    if (window->m_filePath.isEmpty()) {
      continue;
    }
    openDesigns.append(window->m_filePath);
    QString identity = QFileInfo(window->m_filePath).canonicalFilePath();
    if (identity.isEmpty()) {
      identity = QFileInfo(window->m_filePath).absoluteFilePath();
    }
    // Resolve aliases while the old file still exists. A modal author
    // prompt may close another window, so keep a guarded pointer.
    windows.emplace_back(QPointer<MainWindow>(window), QDir::cleanPath(identity));
    if (window->isModified()) {
      modifiedDesigns.append(window->m_filePath);
    }
  }
  QJsonObject refused;
  const std::optional<QJsonObject> answer = projectWrite(
      root,
      {{QStringLiteral("cmd"), QStringLiteral("move")}, {QStringLiteral("from"), from}, {QStringLiteral("to"), to},
       {QStringLiteral("open_designs"), openDesigns}, {QStringLiteral("modified_designs"), modifiedDesigns}},
      rename ? tr("Rename") : tr("Move"), &refused);
  if (!answer) {
    // A rename's name typed again in the panel (mitcad#1268); a drag's
    // refusal is told.
    if (rename) {
      projectNameRefused(root, to, from, tr("Rename"), refused);
    } else if (!errorClassOf(refused).isEmpty()) {
      sheetWarning(this, tr("Move"), errorMessageOf(refused));
    }
  }
  if (answer) {
    // Every window follows a moved design, so a later Save cannot
    // recreate the old filename from another window's stale path.
    for (const QJsonValue& value : answer->value(QStringLiteral("moved")).toArray()) {
      const QJsonObject moved = value.toObject();
      const QString was = QDir::cleanPath(QDir(canonicalRoot).filePath(moved.value(QStringLiteral("from")).toString()));
      const QString now = QDir::cleanPath(QDir(root).filePath(moved.value(QStringLiteral("to")).toString()));
      for (const auto& [window, identity] : windows) {
        if (window && sameFolderPath(was, identity)) {
          window->openDesignMoved(now);
        }
      }
    }
    showHint(rename ? tr("Renamed %1 to %2.").arg(from, to)
                    : tr("Moved %1 to %2.").arg(from, to.section(QLatin1Char('/'), 0, -2).isEmpty()
                                                         ? QFileInfo(root).fileName()
                                                         : to.section(QLatin1Char('/'), 0, -2)));
  }
  Q_UNUSED(folder);
  refreshProjectsPanel();
}

void MainWindow::showProjectsForNaming(bool on) {
  if (m_projectsContainer == nullptr) {
    return;
  }
  auto* card = qobject_cast<GlassCard*>(m_projectsContainer);
  if (on) {
    if (!m_projectsNaming.active) {
      m_projectsNaming.active = true;
      m_projectsNaming.hidden = !projectsPanelShown();
      m_projectsNaming.collapsed = card != nullptr && card->isCollapsed();
    }
    if (!projectsPanelShown()) {
      // Not the user's choice for the next start.
      setProjectsPanelShown(true, false);
      qInfo().noquote() << QStringLiteral("Projects panel: shown for naming");
    }
    if (card != nullptr && card->isCollapsed()) {
      card->setCollapsed(false);
      qInfo().noquote() << QStringLiteral("Projects panel: expanded for naming");
    }
    if (auto* dock = qobject_cast<QDockWidget*>(m_projectsContainer)) {
      dock->raise(); // in front of a dock it shares tabs with
    }
    return;
  }
  if (!m_projectsNaming.active) {
    return;
  }
  const ProjectsNamingState was = m_projectsNaming;
  m_projectsNaming = ProjectsNamingState();
  if (card != nullptr && was.collapsed && !card->isCollapsed()) {
    card->setCollapsed(true);
    qInfo().noquote() << QStringLiteral("Projects panel: collapsed again after naming");
  }
  if (was.hidden && projectsPanelShown()) {
    setProjectsPanelShown(false, false);
    qInfo().noquote() << QStringLiteral("Projects panel: hidden again after naming");
  }
}

void MainWindow::projectNameRefused(const QString& root, const QString& path, const QString& from,
                                    const QString& title, const QJsonObject& refused) {
  const QString cls = errorClassOf(refused);
  if (cls.isEmpty()) {
    return; // cancelled, or failed and told already
  }
  const QString message = errorMessageOf(refused);
  const bool aboutTheName = cls == QLatin1String("exists") || cls == QLatin1String("invalid_path");
  if (aboutTheName && m_projectsPanel != nullptr && m_projectsPanel->nameRefused(root, path, from, message)) {
    // No sheet: the name is typed again where it was (mitcad#1268).
    showError(message);
    return;
  }
  sheetWarning(this, title, message);
}

void MainWindow::openDesignMoved(const QString& path) {
  const QString from = m_filePath;
  m_filePath = QDir::cleanPath(QFileInfo(path).absoluteFilePath());
  m_renamedFrom.clear();
  if (const std::optional<rust::Box<Project>> project = versionedProject(m_filePath)) {
    // What Save compares the file with.
    rememberVersionBase(**project, m_filePath);
  }
  updateWindowTitle();
  addRecentFile(m_filePath);
  qInfo().noquote() << QStringLiteral("Projects panel: the open design follows: %1 -> %2").arg(from, m_filePath);
  // Its edit lock: the old path's released, the new path's taken.
  m_locks->designSaved(m_filePath);
  updateVersionStatus();
}

void MainWindow::deleteInProject(const QString& root, const QString& path, bool folder) {
  const QString absolute = QDir::cleanPath(QDir(root).filePath(path));
  const bool open = !m_filePath.isEmpty() && (folder ? atOrInside(m_filePath, absolute) : sameFolderPath(m_filePath, absolute));
  // What the panel knows of the designs that go.
  int designs = 0;
  QStringList unrecorded;
  if (m_projectsPanel != nullptr) {
    if (const ProjectRow* row = m_projectsPanel->project(root)) {
      for (const ProjectDesignRow& design : row->designs) {
        if (design.path == path || (folder && design.path.startsWith(path + QLatin1Char('/')))) {
          ++designs;
          if (design.changed) {
            unrecorded << design.path;
          }
        }
      }
    }
  }
  const QString name = folder ? path : QFileInfo(path).fileName();
  QMessageBox box(QMessageBox::Warning, tr("Delete"),
                  folder ? tr("Delete the folder %1 and the %n design(s) in it?", nullptr, designs).arg(name)
                         : tr("Delete %1?").arg(name),
                  QMessageBox::Cancel, this);
  QStringList info{tr("It leaves the project as a new version; the project's history keeps it.")};
  if (!unrecorded.isEmpty()) {
    info << tr("Changes that no version holds are lost: %1.").arg(unrecorded.join(QStringLiteral(", ")));
  }
  if (open) {
    info << (isModified() ? tr("%1 is open: it closes, and its unsaved changes are lost.").arg(documentName())
                          : tr("%1 is open: it closes.").arg(documentName()));
  }
  box.setInformativeText(info.join(QStringLiteral("\n\n")));
  QPushButton* remove = box.addButton(tr("&Delete"), QMessageBox::DestructiveRole);
  box.setDefaultButton(QMessageBox::Cancel);
  qInfo().noquote() << QStringLiteral("Delete dialog: %1 (%2 designs%3%4)")
                           .arg(path)
                           .arg(designs)
                           .arg(open ? QStringLiteral(", open") : QString(),
                                unrecorded.isEmpty() ? QString() : QStringLiteral(", changes no version holds"));
  prepareModal(&box);
  execDialog(&box);
  if (box.clickedButton() != remove) {
    qInfo().noquote() << QStringLiteral("Delete cancelled");
    return;
  }
  QJsonObject command{{QStringLiteral("cmd"), QStringLiteral("delete")}, {QStringLiteral("path"), path}};
  QJsonObject refused;
  std::optional<QJsonObject> answer = projectWrite(root, command, tr("Delete"), &refused);
  if (!answer && errorClassOf(refused) == QLatin1String("local_changes")) {
    // Changes no version holds, which the panel did not know of: asked.
    const QMessageBox::StandardButton anyway = sheetWarning(
        this, tr("Delete"),
        tr("%1\n\nDelete all the same? Those changes are lost.").arg(errorMessageOf(refused)),
        QMessageBox::Yes | QMessageBox::Cancel, QMessageBox::Cancel);
    qInfo().noquote() << QStringLiteral("Delete: changes no version holds: %1")
                             .arg(anyway == QMessageBox::Yes ? QStringLiteral("deleted all the same")
                                                             : QStringLiteral("cancelled"));
    if (anyway == QMessageBox::Yes) {
      command.insert(QStringLiteral("force"), true);
      answer = projectWrite(root, command, tr("Delete"));
    }
  } else if (!answer && !errorClassOf(refused).isEmpty()) {
    sheetWarning(this, tr("Delete"), errorMessageOf(refused));
  }
  if (answer && open) {
    // The deleted design closes; the window keeps the project.
    if (installEmptyDocument(QString())) {
      qInfo().noquote() << QStringLiteral("Projects panel: closed the deleted design %1").arg(absolute);
    }
  }
  if (answer) {
    showHint(tr("Deleted %1: the project's history keeps it.").arg(name));
  }
  refreshProjectsPanel();
}

void MainWindow::openPanelDesign(const QString& path) {
  if (!m_filePath.isEmpty() && sameFolderPath(path, m_filePath)) {
    qInfo().noquote() << QStringLiteral("Projects panel: %1 is open already").arg(path);
    return;
  }
  openPath(path);
}

void MainWindow::panelVersionHistory(const QString& root, const QString& path) {
  const QString file = QDir::cleanPath(QDir(root).filePath(path));
  if (m_filePath.isEmpty() || !sameFolderPath(file, m_filePath)) {
    // The history is the open design's: it opens first.
    if (m_windowManager == nullptr && !maybeSave()) {
      return;
    }
    QString error;
    if (m_windowManager != nullptr) {
      if (MainWindow* target = m_windowManager->openFile(this, file, error)) target->showVersionHistory();
      else if (!error.isEmpty()) sheetWarning(this, tr("Version History"), error);
      return;
    }
    if (!openFile(file, error)) {
      if (!error.isEmpty()) {
        sheetWarning(this, tr("Version History"),
                     tr("Could not open %1:\n%2").arg(QDir::toNativeSeparators(file), error));
      }
      return;
    }
  }
  showVersionHistory();
}

void MainWindow::panelProjectAction(const QString& root, const QString& id) {
  if (!(m_project.isProject() && sameFolderPath(m_project.root, root))) {
    // Another project: it opens first (its last design, as Open Project).
    openProjectFolder(root, true);
    if (!(m_project.isProject() && sameFolderPath(m_project.root, root))) {
      return;
    }
  }
  trigger(id);
}

void MainWindow::panelProjectSettings(const QString& root) {
  if (m_project.hasHistory() && sameFolderPath(m_project.root, root)) {
    // The current project: as File > Project Settings, with its remote
    // work, edit locks and live updates.
    qInfo().noquote() << QStringLiteral("Projects panel: Project Settings of the current project %1").arg(root);
    trigger(QStringLiteral("file.project_settings"));
    return;
  }
  const ProjectRow* row = m_projectsPanel != nullptr ? m_projectsPanel->project(root) : nullptr;
  if (row == nullptr || (row->kind != ProjectRow::Kind::Local && row->kind != ProjectRow::Kind::Cloud)) {
    qInfo().noquote() << QStringLiteral("Projects panel: no Project Settings for %1").arg(root);
    return;
  }
  // Another project: its settings without opening it (the open design
  // stays). This window runs no remote work, edit locks or live updates
  // for it, so there is nothing to settle or stop first.
  qInfo().noquote() << QStringLiteral("Projects panel: Project Settings of %1 (not the current project)").arg(root);
  const QString url = row->remoteUrl;
  ProjectSettingsHost host;
  host.projectChanged = [this] { refreshProjectsPanel(); };
  host.settle = [](const QString&) { return true; };
  host.versionRecorded = [this, root] { sendPanelProjectVersions(root); };
  host.sync = [this, root] { panelProjectAction(root, QStringLiteral("file.sync")); };
  host.openInBrowser = [url] {
    const QString page = addressWebPage(url);
    if (!page.isEmpty()) {
      openExternalUrl(QUrl(page));
    }
  };
  host.status = [this, root] { return m_projectsPanel != nullptr ? m_projectsPanel->stateText(root) : QString(); };
  host.settingsChanged = [this](const QJsonObject&) { refreshProjectsPanel(); };
  host.testBroker = [this](QWidget* parent, const QString& broker, const QString& prefix) {
    m_live->testBroker(parent, broker, prefix);
  };
  host.removeProject = [this, root] { removeProject(root); };
  mitcad::showProjectSettings(this, root, host);
  refreshProjectsPanel();
}

void MainWindow::addExistingProjectFolder() {
  const QString folder = dialogDirectory(this, tr("Add Existing Project Folder"), projectsDirectory());
  if (folder.isEmpty()) {
    qInfo().noquote() << QStringLiteral("Add Existing Project Folder cancelled");
    return;
  }
  const ProjectState state = projectOfInspection(inspectFolder(folder));
  if (state.isProject()) {
    ProjectsSettings::remember(state.root);
    qInfo().noquote() << QStringLiteral("Projects panel: added %1").arg(state.root);
    showProjectInPanel(state.root);
    return;
  }
  qInfo().noquote() << QStringLiteral("Add Existing Project Folder: %1 is no project").arg(folder);
  QMessageBox box(QMessageBox::Question, tr("Add Existing Project Folder"),
                  tr("%1 is not a MitCad project.").arg(QDir::toNativeSeparators(folder)), QMessageBox::Cancel, this);
  box.setInformativeText(tr("Make It a Project makes it one in New Project: the designs in it become the project's."));
  QPushButton* make = box.addButton(tr("&Make It a Project…"), QMessageBox::AcceptRole);
  prepareModal(&box);
  execDialog(&box);
  if (box.clickedButton() == make) {
    openProjectFolder(folder, true);
  }
}

// ---------------------------------------------------------------------------
// Remove Project (mitcad#1562)

void MainWindow::removeProject(const QString& root) {
  const QString name = QFileInfo(root).fileName();
  // What would happen, as the core reads the folder: nothing changes yet.
  const QJsonObject plan = projectsCommand({{QStringLiteral("cmd"), QStringLiteral("remove_project")},
                                            {QStringLiteral("dir"), root},
                                            {QStringLiteral("dry_run"), true}});
  if (!errorClassOf(plan).isEmpty()) {
    qInfo().noquote() << QStringLiteral("Remove Project refused (%1): %2").arg(errorClassOf(plan), errorMessageOf(plan));
    sheetWarning(this, tr("Remove Project"), errorMessageOf(plan));
    return;
  }
  // The windows with a design of the project open, or the project as theirs.
  QList<MainWindow*> windows;
  if (WindowManager* manager = WindowManager::instance()) {
    windows = manager->windows();
  }
  if (!windows.contains(this)) {
    windows << this;
  }
  QStringList open;
  for (MainWindow* window : windows) {
    if (!window->m_filePath.isEmpty() && atOrInside(window->m_filePath, root)) {
      open << window->documentName();
    }
  }
  const QString fate = plan.value(QStringLiteral("repository")).toString();
  const int designs = plan.value(QStringLiteral("designs")).toInt();
  const int versions = plan.value(QStringLiteral("versions")).toInt();
  QStringList rewritten;
  for (const QJsonValue& path : plan.value(QStringLiteral("rewritten")).toArray()) {
    rewritten << path.toString();
  }
  QStringList files;
  for (const QString& key : {QStringLiteral("removed"), QStringLiteral("edited")}) {
    for (const QJsonValue& path : plan.value(key).toArray()) {
      files << path.toString();
    }
  }
  const QString remote = plan.value(QStringLiteral("remote")).toObject().value(QStringLiteral("url")).toString();

  QMessageBox box(QMessageBox::Warning, tr("Remove Project"), tr("Remove the project %1?").arg(name),
                  QMessageBox::Cancel, this);
  QStringList info{tr("The folder %1 stays with its %n design(s) and other files: it is no longer a project.",
                      nullptr, designs)
                       .arg(QDir::toNativeSeparators(root))};
  if (fate == QLatin1String("removed")) {
    info << tr("Its version history (%n version(s)) is deleted: older versions and deleted designs cannot be "
               "brought back.",
               nullptr, versions);
  } else if (fate == QLatin1String("kept")) {
    info << tr("The folder was a git repository before the project: it keeps its history, and a new version takes "
               "the project's files out.");
  }
  if (!remote.isEmpty()) {
    info << tr("Only this computer's copy stops being a project: the repository at %1 is not changed.").arg(remote);
  }
  if (!rewritten.isEmpty()) {
    info << tr("%n design(s) keeping data in the project are first saved as single files: %1.", nullptr,
               static_cast<int>(rewritten.size()))
                .arg(rewritten.join(QStringLiteral(", ")));
  }
  if (!files.isEmpty()) {
    info << tr("MitCad's files are removed: %1.").arg(files.join(QStringLiteral(", ")));
  }
  if (!open.isEmpty()) {
    info << tr("Open designs of the project close: %1.").arg(open.join(QStringLiteral(", ")));
  }
  box.setInformativeText(info.join(QStringLiteral("\n\n")));
  QPushButton* remove = box.addButton(tr("&Remove Project"), QMessageBox::DestructiveRole);
  box.setDefaultButton(QMessageBox::Cancel);
  qInfo().noquote() << QStringLiteral("Remove Project dialog: %1 (%2 designs, history %3%4)")
                           .arg(root)
                           .arg(designs)
                           .arg(fate, open.isEmpty() ? QString() : QStringLiteral(", %1 open").arg(open.size()));
  prepareModal(&box);
  execDialog(&box);
  if (box.clickedButton() != remove) {
    qInfo().noquote() << QStringLiteral("Remove Project cancelled");
    return;
  }
  // Its designs close (their changes saved or not, as asked), and no window
  // keeps it as its project: its remote work and edit locks end first.
  for (MainWindow* window : windows) {
    if (!window->m_filePath.isEmpty() && atOrInside(window->m_filePath, root) && !window->maybeSave()) {
      qInfo().noquote() << QStringLiteral("Remove Project cancelled at the question about changes");
      return;
    }
  }
  for (MainWindow* window : windows) {
    if (!window->m_filePath.isEmpty() && atOrInside(window->m_filePath, root)) {
      const QString path = window->m_filePath;
      if (window->installEmptyDocument(QString())) {
        qInfo().noquote() << QStringLiteral("Remove Project: closed %1").arg(path);
      }
    }
    if (window->m_project.isProject() && sameFolderPath(window->m_project.root, root)) {
      window->setCurrentProject(ProjectState(), false);
    }
  }
  const QJsonObject answer =
      projectsCommand({{QStringLiteral("cmd"), QStringLiteral("remove_project")}, {QStringLiteral("dir"), root}});
  if (!errorClassOf(answer).isEmpty()) {
    qInfo().noquote() << QStringLiteral("Remove Project failed (%1): %2")
                             .arg(errorClassOf(answer), errorMessageOf(answer));
    sheetWarning(this, tr("Remove Project"), errorMessageOf(answer));
    refreshProjectsPanel();
    return;
  }
  ProjectsSettings::forget(root);
  qInfo().noquote() << QStringLiteral("Remove Project: removed %1 (history %2, %3 designs stay)")
                           .arg(root, answer.value(QStringLiteral("repository")).toString())
                           .arg(answer.value(QStringLiteral("designs")).toInt());
  for (const QJsonValue& warning : answer.value(QStringLiteral("warnings")).toArray()) {
    qInfo().noquote() << QStringLiteral("Remove Project: warning: %1").arg(warning.toString());
  }
  showHint(tr("%1 is no longer a project; its designs stay in the folder.").arg(name));
  for (MainWindow* window : windows) {
    window->refreshProjectsPanel();
  }
}

} // namespace mitcad
