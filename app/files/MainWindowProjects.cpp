// SPDX-License-Identifier: MIT
// Local and Cloud projects (mitcad#89): the window's current project, File
// > New Project, Open Project, Open from Cloud, Open Read-Only, Project
// Settings and Save to Project, and the project indicator. The dialogs
// are files/ProjectDialogs.hpp and files/ProjectSettings.hpp; the core's
// side is commands.md, "Projects".
#include "../MainWindow.hpp"
#include "../WindowManager.hpp"

#include <utility>

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QMessageBox>
#include <QPushButton>
#include <QtLogging>

#include "../framework/AppSettings.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/Json.hpp"
#include "../projects/ProjectBrowser.hpp"
#include "../projects/ProjectsPanel.hpp"
#include "Autosave.hpp"
#include "LiveController.hpp"
#include "LockController.hpp"
#include "ProjectDialogs.hpp"
#include "ProjectIndicator.hpp"
#include "ProjectSettings.hpp"
#include "RemoteController.hpp"
#include "VersionDialogs.hpp"
#include "VersionHistory.hpp"

namespace mitcad {
namespace {

QString qstr(const rust::String& text) { return QString::fromUtf8(text.data(), static_cast<qsizetype>(text.size())); }

// The designs of an inspection (relative paths, newest first).
QStringList designsOf(const QJsonObject& inspection) {
  QStringList designs;
  for (const QJsonValue& value : inspection.value(QStringLiteral("designs")).toArray()) {
    designs << value.toObject().value(QStringLiteral("path")).toString();
  }
  return designs;
}

// Whether a file is in a folder (or below).
bool isInside(const QString& path, const QString& folder) {
  if (path.isEmpty() || folder.isEmpty()) {
    return false;
  }
  const QString cleanPath = QDir::cleanPath(QFileInfo(path).absoluteFilePath());
  const QString cleanFolder = QDir::cleanPath(folder) + QLatin1Char('/');
#ifdef _WIN32
  return cleanPath.startsWith(cleanFolder, Qt::CaseInsensitive);
#else
  return cleanPath.startsWith(cleanFolder);
#endif
}

} // namespace

// ---------------------------------------------------------------------------
// Commands

void MainWindow::registerProjectCommands() {
  const auto def = [](const char* id, const QString& name, const char* icon, const QString& tooltip,
                      std::function<void()> run) {
    CommandDef d;
    d.id = QString::fromLatin1(id);
    d.name = name;
    d.icon = QString::fromLatin1(icon);
    d.tooltip = tooltip;
    d.kind = CommandDef::Kind::Action;
    d.mode = CommandDef::Mode::Any;
    d.tab.clear(); // the File menu
    d.run = std::move(run);
    return d;
  };
  CommandDef project = def("file.new_project", tr("New Project…"), "new-project",
                           tr("A folder whose designs keep their versions, on this computer or shared"),
                           [this] { newProject(); });
  project.keywords = {QStringLiteral("project"),         QStringLiteral("folder"),
                      QStringLiteral("git"),             QStringLiteral("cloud"),
                      QStringLiteral("local"),           QStringLiteral("version history"),
                      QStringLiteral("start version history"), QStringLiteral("repository")};
  m_registry->add(project);
  CommandDef openProject = def("file.open_project", tr("Open Project…"), "folder",
                               tr("Opens a project's folder, or makes a folder of designs a project"),
                               [this] { openProjectFolder(); });
  openProject.keywords = {QStringLiteral("project"),    QStringLiteral("folder"), QStringLiteral("git"),
                          QStringLiteral("repository"), QStringLiteral("cloud"),  QStringLiteral("local")};
  m_registry->add(openProject);
  CommandDef readOnly = def("file.open_read_only", tr("Open Read-Only…"), "project",
                            tr("Opens a design without its edit lock: to look at it while someone else edits it"),
                            [this] { openReadOnly(); });
  readOnly.keywords = {QStringLiteral("view"), QStringLiteral("lock"), QStringLiteral("read only")};
  m_registry->add(readOnly);
  // A design of a project, browsed or searched (mitcad#530).
  CommandDef browse = def("file.open_from_project", tr("Open from Project…"), "folder",
                          tr("Opens a design of a project: its folders browsed, or its name searched"),
                          [this] { openFromProject(); });
  browse.keywords = {QStringLiteral("project"), QStringLiteral("browse"), QStringLiteral("folder"),
                     QStringLiteral("design"),  QStringLiteral("find"),   QStringLiteral("search"),
                     QStringLiteral("files"),   QStringLiteral("read only")};
  m_registry->add(browse);
  CommandDef cloud = def("file.open_remote", tr("Open from Cloud…"), "remote-open",
                         tr("Opens a Cloud project from its git repository into a new folder"),
                         [this] { openFromCloud(); });
  cloud.keywords = {QStringLiteral("clone"),  QStringLiteral("remote"), QStringLiteral("download"),
                    QStringLiteral("git"),    QStringLiteral("cloud"),  QStringLiteral("server"),
                    QStringLiteral("github"), QStringLiteral("gitlab"), QStringLiteral("forgejo")};
  m_registry->add(cloud);
  CommandDef settings = def("file.project_settings", tr("Project Settings…"), "settings",
                            tr("The current project's storage, sharing, edit locks and syncing"),
                            [this] { showProjectSettings(); });
  settings.keywords = {QStringLiteral("connect"), QStringLiteral("remote"),     QStringLiteral("share"),
                       QStringLiteral("git"),     QStringLiteral("cloud"),      QStringLiteral("address"),
                       QStringLiteral("url"),     QStringLiteral("disconnect"), QStringLiteral("author"),
                       QStringLiteral("lock"),    QStringLiteral("mqtt"),       QStringLiteral("github")};
  m_registry->add(settings);
  // Save to Project (mitcad#503) replaces Move to a Project; saved shortcuts
  // and scripts of the old command id keep working.
  CommandDef save = def("file.save_to_project", tr("Save to Project…"), "new-project",
                        tr("Saves the design in a project, so that its versions are kept"),
                        [this] { saveToProject(); });
  save.keywords = {QStringLiteral("start version history"), QStringLiteral("version history"),
                   QStringLiteral("versions"), QStringLiteral("project"), QStringLiteral("git"),
                   QStringLiteral("move to a project"), QStringLiteral("save as")};
  m_registry->add(save);
  m_registry->alias(QStringLiteral("file.move_to_project"), QStringLiteral("file.save_to_project"));
  // The Projects panel and File > New Design (mitcad#145).
  registerProjectsPanelCommands();
  // The project's web viewer (mitcad#423).
  registerWebViewerCommands();
}

// ---------------------------------------------------------------------------
// The current project

QString MainWindow::projectRootWithHistory() const { return m_project.hasHistory() ? m_project.root : QString(); }

void MainWindow::setCurrentProject(const ProjectState& project, bool check) {
  const bool changed = project != m_project;
  m_project = project;
  if (changed) {
    qInfo().noquote() << QStringLiteral("Project: %1").arg(project.describe());
  }
  m_indicator->setProject(m_project);
  if (changed && project.isProject()) {
    // A project made, opened or cloned here: in the Projects panel's list
    // (mitcad#145).
    ProjectsSettings::remember(project.root);
    refreshProjectsPanel();
  }
  if (changed) {
    m_remote->projectChanged(check);
    m_locks->projectChanged();
    if (!project.unfollowedRemotes.isEmpty()) {
      showHint(tr("The repository of %1 has several remotes and follows none of them: choose one in Project "
                  "Settings.")
                   .arg(project.name()));
    }
  }
  updateProjectIndicator();
}

void MainWindow::followFileProject(const QString& path) {
  if (m_project.isProject() && isInside(path, m_project.root) &&
      sameFolderPath(qstr(find_project(rustStr(path.toUtf8()))), m_project.root)) {
    updateProjectIndicator(); // the same project
    return;
  }
  // Another project's design, or a loose file (no project now).
  setCurrentProject(projectOfFile(path), false);
}

void MainWindow::updateProjectIndicator() {
  m_indicator->setProject(m_project);
  m_indicator->setRemote(m_remote->indicatorState());
  updateLive();
}

// ---------------------------------------------------------------------------
// New Project

void MainWindow::newProject() {
  if (!maybeSave()) {
    return;
  }
  NewProjectOptions options;
  handleNewProject(askNewProject(this, options), QString());
}

bool MainWindow::handleNewProject(const std::optional<NewProjectResult>& result, const QString& moving) {
  if (!result) {
    return false;
  }
  switch (result->kind) {
  case NewProjectResult::Kind::OpenProject:
    if (!moving.isEmpty()) {
      return saveIntoProject(result->folder);
    }
    openProjectFolder(result->folder, false);
    return true;
  case NewProjectResult::Kind::OpenFromCloud: {
    OpenFromCloudOptions options;
    options.url = result->url;
    options.folder = result->folder;
    openFromCloud(options, false);
    return false;
  }
  case NewProjectResult::Kind::Created:
    break;
  }
  const QString root = result->folder;
  // The author typed is MitCad's default when there was none.
  VersionSettings defaults = VersionSettings::load();
  if (!defaults.complete() && !gitIdentity().complete()) {
    try {
      const rust::Box<Project> project = open_project(rustStr(root.toUtf8()));
      const QJsonObject identity =
          parseObject(project->command(rustStr(compactJson({{QStringLiteral("cmd"), QStringLiteral("identity")}}))));
      defaults.name = identity.value(QStringLiteral("name")).toString();
      defaults.email = identity.value(QStringLiteral("email")).toString();
      if (defaults.complete()) {
        defaults.save();
      }
    } catch (const std::exception&) {
    }
  }
  if (!result->broker.isEmpty()) {
    qInfo().noquote() << QStringLiteral("New project: live updates through %1").arg(result->broker);
  }
  const ProjectState state = projectOfInspection(inspectFolder(root));
  setCurrentProject(state, false);
  qInfo().noquote() << QStringLiteral("New project %1").arg(root);
  const QString cls = errorClassOf(result->answer);
  // The project in the Projects panel (mitcad#145).
  showProjectInPanel(root);
  if (!moving.isEmpty()) {
    return saveIntoProject(root);
  }
  // The project is empty, or holds the designs its folder had: the one
  // there is opens, else the panel offers them and New Design.
  if (!openProjectDesign(root, QString())) {
    return false;
  }
  if (state.kind == ProjectState::Kind::Cloud) {
    if (!cls.isEmpty()) {
      // Versions that wait: sent when the repository can be reached.
      m_remote->retryLater();
      showError(tr("The project %1 is made, but its versions could not be sent: %2 They are sent when the "
                   "repository can be reached.")
                    .arg(state.name(), errorMessageOf(result->answer)));
      return true;
    }
  }
  const bool sendAtOnce = projectSync(root).sendAtOnce;
  showHint(state.kind == ProjectState::Kind::Cloud
               ? (sendAtOnce ? tr("Cloud project %1 is in %2: New Design adds a design; every save records a version "
                                 "and sends it.")
                             : tr("Cloud project %1 is in %2: New Design adds a design; Sync Now sends recorded "
                                  "versions."))
                     .arg(state.name(), QDir::toNativeSeparators(root))
               : tr("Local project %1 is in %2: New Design adds a design; every save records a version.")
                     .arg(state.name(), QDir::toNativeSeparators(root)));
  return true;
}

// ---------------------------------------------------------------------------
// Opening projects

void MainWindow::openProjectFolder(const QString& given, bool ask) {
  if (ask && !maybeSave()) {
    return;
  }
  QString folder = given;
  if (folder.isEmpty()) {
    // The projects the Projects panel lists (mitcad#176); Another Folder...
    // the folder dialog, for a folder not listed (made a project below).
    ProjectPickerOptions options;
    options.mode = ProjectPickerOptions::Mode::Project;
    options.title = tr("Open a Project");
    options.text = tr("The project to open:");
    options.acceptText = tr("&Open");
    options.otherText = tr("&Another Folder…");
    options.root = panelProjectSelection().first;
    const std::optional<ProjectPickerResult> picked = pickInProjects(this, options);
    if (!picked) {
      qInfo().noquote() << QStringLiteral("Open Project cancelled");
      return;
    }
    folder = picked->other ? dialogDirectory(this, tr("Open Project"), projectsDirectory()) : picked->root;
    if (folder.isEmpty()) {
      qInfo().noquote() << QStringLiteral("Open Project cancelled");
      return;
    }
  }
  folder = QDir::cleanPath(QFileInfo(folder).absoluteFilePath());
  const QJsonObject inspection = inspectFolder(folder);
  const QString kind = inspection.value(QStringLiteral("kind")).toString();
  qInfo().noquote() << QStringLiteral("Open Project: %1: %2").arg(folder, kind);
  if (!errorClassOf(inspection).isEmpty()) {
    sheetWarning(this, tr("Open Project"), errorMessageOf(inspection));
    return;
  }
  if (kind == QLatin1String("missing")) {
    sheetWarning(this, tr("Open Project"), tr("%1 does not exist.").arg(QDir::toNativeSeparators(folder)));
    return;
  }
  if (kind == QLatin1String("project") || kind == QLatin1String("inside_project")) {
    const QString root = QDir::cleanPath(inspection.value(QStringLiteral("project_root")).toString());
    ProjectState state = projectOfInspection(inspection);
    if (state.kind == ProjectState::Kind::NoHistory && state.outer.isEmpty()) {
      // An older project without versions: its history starts now.
      if (startProjectHistory(root)) {
        state = projectOfInspection(inspectFolder(root));
      }
    }
    setCurrentProject(state, false);
    openProjectDesign(root, QString());
    return;
  }
  // A git repository, a folder of designs or files, an empty folder: made a
  // project in New Project, the folder fixed.
  NewProjectOptions options;
  options.folder = folder;
  options.folderFixed = true;
  options.cloud = !inspection.value(QStringLiteral("remote")).isNull() &&
                  inspection.value(QStringLiteral("remote")).isObject();
  if (options.cloud) {
    options.url = inspection.value(QStringLiteral("remote")).toObject().value(QStringLiteral("url")).toString();
  }
  handleNewProject(askNewProject(this, options), QString());
}

bool MainWindow::startProjectHistory(const QString& root) {
  // Who records the first version: git's user, else MitCad's default
  // author, else asked (an older project without any author).
  QString author;
  const GitIdentity git = gitIdentity();
  VersionSettings settings = VersionSettings::load();
  if (git.complete()) {
    author = QStringLiteral("%1 <%2>").arg(git.name, git.email);
  } else if (settings.complete()) {
    author = settings.author();
  } else {
    const std::optional<VersionSettings> chosen = askVersionAuthor(this, settings);
    if (!chosen) {
      return false;
    }
    settings = *chosen;
    settings.save();
    author = settings.author();
  }
  try {
    const QJsonObject answer = parseObject(init_project_history(rustStr(root.toUtf8()), rustStr(author.toUtf8())));
    qInfo().noquote() << QStringLiteral("Version history started in %1: version %2 on %3")
                             .arg(QDir::cleanPath(answer.value(QStringLiteral("root")).toString()),
                                  answer.value(QStringLiteral("commit")).toString().left(7),
                                  answer.value(QStringLiteral("branch")).toString());
    showHint(tr("The project %1 keeps versions now: a git repository in its folder, with its designs as the "
                "first version.")
                 .arg(QFileInfo(root).fileName()));
    return true;
  } catch (const std::exception& e) {
    const QString message =
        tr("Could not start the version history of %1:\n%2").arg(QDir::toNativeSeparators(root), errorText(e));
    qWarning().noquote() << message;
    sheetWarning(this, tr("Open Project"), message);
    return false;
  }
}

bool MainWindow::openProjectDesign(const QString& root, const QString& preferred) {
  // The Projects panel shows the project's designs (mitcad#145); the one
  // opened last there opens (.mitcad/local/recent.json), else the newest.
  showProjectInPanel(root);
  QString design = preferred;
  if (design.isEmpty()) {
    const QJsonObject inspection = inspectFolder(root);
    const QStringList designs = designsOf(inspection);
    const QString last = inspection.value(QStringLiteral("last_design")).toString();
    if (!last.isEmpty() && designs.contains(last)) {
      design = last;
    } else if (!designs.isEmpty()) {
      design = designs.first();
    }
    if (design.isEmpty()) {
      // An empty project: an untitled design, which keeps the project as the
      // window's; New Design adds the project's first one.
      if (!installEmptyDocument(QString())) {
        return false;
      }
      qInfo().noquote() << QStringLiteral("Open Project: no designs in %1").arg(root);
      showHint(tr("%1 has no designs yet: New Design (File menu or the Projects panel) adds one.")
                   .arg(QFileInfo(root).fileName()));
      return true;
    }
    qInfo().noquote() << QStringLiteral("Open Project: %1 (%2)")
                             .arg(design, design == last ? QStringLiteral("opened last") : QStringLiteral("newest"));
  }
  const QString path = QDir(root).filePath(design);
  QString error;
  if (!loadDesign(path, error)) {
    if (!error.isEmpty()) {
      const QString message = tr("Could not open %1:\n%2").arg(QDir::toNativeSeparators(path), error);
      qWarning().noquote() << message;
      sheetWarning(this, tr("MitCad"), message);
    }
    return false;
  }
  return true;
}

void MainWindow::openFromCloud(const OpenFromCloudOptions& options, bool ask) {
  if ((ask && !maybeSave()) || !m_remote->settle(tr("open another project"))) {
    return;
  }
  const std::optional<OpenFromCloudResult> result = askOpenFromCloud(this, options);
  if (!result) {
    return;
  }
  const QString root = result->root;
  const ProjectState state = projectOfInspection(inspectFolder(root));
  setCurrentProject(state, false);
  qInfo().noquote() << QStringLiteral("Open from Cloud: opened %1 (%2): %3")
                           .arg(root, result->action, designsOf(inspectFolder(root)).join(QStringLiteral(", ")));
  if (!openProjectDesign(root, QString())) {
    return;
  }
  const QString cls = errorClassOf(result->answer);
  if (!cls.isEmpty()) {
    m_remote->retryLater();
    showError(tr("The project %1 is made, but its versions could not be sent: %2 They are sent when the "
                 "repository can be reached.")
                  .arg(state.name(), errorMessageOf(result->answer)));
  }
}

void MainWindow::openReadOnly() {
  if (m_windowManager == nullptr && !maybeSave()) {
    return;
  }
  const QString path = dialogOpenFileName(this, tr("Open Read-Only"), dialogFolder(DialogGroup::Open),
                                          tr("MitCad designs (*.mitcad)"));
  if (path.isEmpty()) {
    return;
  }
  rememberDialogFolder(DialogGroup::Open, path);
  openReadOnlyFile(path);
}

void MainWindow::openReadOnlyFile(const QString& path) {
  // The lock controller reads it while the design opens: no edit lock is
  // taken, and the window is read-only (files/LockController.hpp).
  m_openingReadOnly = true;
  QString error;
  const bool opened = m_windowManager != nullptr ? m_windowManager->openFile(this, path, error) != nullptr
                                                 : openFile(path, error);
  m_openingReadOnly = false;
  qInfo().noquote() << QStringLiteral("Open Read-Only: %1%2").arg(path, opened ? QString() : QStringLiteral(" failed"));
  if (!opened && !error.isEmpty()) {
    sheetWarning(this, tr("MitCad"), tr("Could not open %1:\n%2").arg(QDir::toNativeSeparators(path), error));
  }
}

void MainWindow::openFromProject() {
  if (deferForNativeCommand([this] { openFromProject(); })) {
    return;
  }
  // The projects browsed as folders (mitcad#530); unsaved changes are asked
  // about once a design is chosen, as Open Recent asks.
  ProjectBrowserOptions options;
  options.title = tr("Open from Project");
  options.acceptText = tr("&Open");
  options.otherText = tr("A&nother File…");
  options.readOnlyText = tr("&Read-only");
  // Where it starts without a remembered place: the Projects panel's
  // selection, else the current project.
  const std::pair<QString, QString> selected = panelProjectSelection();
  options.root = !selected.first.isEmpty() ? selected.first : projectRootWithHistory();
  options.folder = !selected.first.isEmpty() ? selected.second : QString();
  options.session = m_autosave != nullptr ? m_autosave->session() : QString();
  const std::optional<ProjectBrowserResult> chosen = browseProjects(this, options);
  if (!chosen) {
    return;
  }
  if (chosen->other) {
    // The system's file dialog, as Open (or Open Read-Only) has it.
    if (chosen->readOnly) {
      openReadOnly();
    } else {
      openDocument();
    }
    return;
  }
  if (chosen->readOnly) {
    if (m_windowManager == nullptr && !maybeSave()) {
      return;
    }
    openReadOnlyFile(chosen->path);
    return;
  }
  // The open design stays as it is; another one opens as Open Recent's.
  openPanelDesign(chosen->path);
}

// ---------------------------------------------------------------------------
// Project Settings

void MainWindow::showProjectSettings() {
  if (m_project.kind == ProjectState::Kind::None || m_project.kind == ProjectState::Kind::NoHistory) {
    const bool inside = m_project.kind == ProjectState::Kind::NoHistory;
    qInfo().noquote() << QStringLiteral("Project Settings: %1").arg(inside ? QStringLiteral("no versions")
                                                                           : QStringLiteral("not in a project"));
    QMessageBox box(QMessageBox::Information, tr("Project Settings"),
                    inside ? tr("The project %1 keeps no versions: it is inside the git repository %2.")
                                 .arg(m_project.name(), QDir::toNativeSeparators(m_project.outer))
                           : tr("%1 is in no project.").arg(documentName()),
                    QMessageBox::Close, this);
    box.setInformativeText(tr("Save to Project puts the design in a project that keeps its versions."));
    QPushButton* save = box.addButton(tr("&Save to Project…"), QMessageBox::AcceptRole);
    prepareModal(&box);
    execDialog(&box);
    if (box.clickedButton() == save) {
      saveToProject();
    }
    return;
  }
  ProjectSettingsHost host;
  host.projectChanged = [this] {
    setCurrentProject(projectOfInspection(inspectFolder(m_project.root)), true);
  };
  host.settle = [this](const QString& why) { return m_remote->settle(why); };
  host.versionRecorded = [this] {
    m_remote->versionRecorded(QDir(m_project.root).filePath(QStringLiteral(".mitcad/project.json")));
  };
  host.sync = [this] { m_remote->sync(); };
  host.openInBrowser = [this] { m_remote->openInBrowser(); };
  host.status = [this] { return m_remote->statusText(); };
  host.settingsChanged = [this](const QJsonObject&) {
    m_remote->settingsChanged();
    m_live->projectSettingsChanged(m_project.root);
    m_locks->settingsChanged();
    refreshProjectsPanel(); // the display name (mitcad#152)
  };
  // Edit locks and live updates (mitcad#89): before the project stops
  // syncing, this session's locks are released (published while the live
  // connection is still up), then live updates stop; whether the remote
  // takes lock refs, asked once; Test of a broker.
  host.stoppingSync = [this] {
    m_locks->stoppingSync();
    m_live->stoppingSync(m_project.root);
  };
  host.probeLocks = [this](QObject* context, std::function<void(bool, const QString&)> done) {
    m_locks->probe(context, std::move(done));
  };
  host.testBroker = [this](QWidget* parent, const QString& broker, const QString& prefix) {
    m_live->testBroker(parent, broker, prefix);
  };
  // Remove Project (mitcad#1562), after the dialog closed.
  const QString root = m_project.root;
  host.removeProject = [this, root] { removeProject(root); };
  mitcad::showProjectSettings(this, root, host);
  updateProjectIndicator();
}

// ---------------------------------------------------------------------------
// Save to Project (mitcad#503; Move to a Project before)

void MainWindow::saveToProject() {
  if (m_nativeUi && m_session) {
    showHint(tr("Finish the current command before saving."));
    return;
  }
  const QString path = m_filePath;
  // The project picker, as Save As has it (mitcad#176): the design's own
  // project and folder, else the panel's selection. Its own file is no
  // clash, unless the window is read-only (a copy).
  std::pair<QString, QString> place = saveAsProjectPlace();
  if (place.first.isEmpty()) {
    place = panelProjectSelection();
  }
  const QString reason =
      m_project.kind == ProjectState::Kind::NoHistory
          ? tr("Its project %1 is inside the git repository %2, so it keeps no versions: MitCad records versions "
               "only in a repository of the project's own.")
                .arg(m_project.name(), QDir::toNativeSeparators(m_project.outer))
          : QString();
  ProjectPickerOptions options;
  options.title = tr("Save to Project");
  options.text = (reason.isEmpty() ? QString() : reason + QLatin1Char('\n')) +
                 tr("The project and the folder for %1, and its name there:").arg(documentName());
  options.acceptText = tr("&Save");
  options.newProjectText = tr("New &Project…");
  options.otherText = tr("&Another Folder…");
  options.root = place.first;
  options.folder = place.second;
  options.name = path.isEmpty() ? documentName() : QFileInfo(path).completeBaseName();
  options.ownFile = windowReadOnly() ? QString() : path;
  const std::optional<ProjectPickerResult> picked = pickInProjects(this, options);
  if (!picked) {
    qInfo().noquote() << QStringLiteral("Save to Project cancelled");
    return;
  }
  if (!picked->newProject && !picked->other) {
    saveIntoProject(picked->root, picked->path);
    return;
  }
  if (picked->other) {
    // Another Folder...: a project the list does not have.
    const QString folder = dialogDirectory(this, tr("Choose a Project Folder"), projectsDirectory());
    if (folder.isEmpty()) {
      qInfo().noquote() << QStringLiteral("Save to Project cancelled");
      return;
    }
    const ProjectState project = projectOfInspection(inspectFolder(folder));
    if (!project.hasHistory()) {
      qInfo().noquote() << QStringLiteral("Save to Project: %1 is no project with versions").arg(folder);
      sheetWarning(this, tr("Save to Project"),
                   tr("%1 is not a project that keeps versions: choose a project's folder, or a new project.")
                       .arg(QDir::toNativeSeparators(folder)));
      return;
    }
    // One of the known projects from now on.
    ProjectsSettings::remember(project.root);
    saveIntoProject(project.root);
    return;
  }
  // A new project: by default the design's own folder (it becomes the
  // project, its designs in the first version), unless that is inside a
  // repository or a project, then a new folder named after the design.
  NewProjectOptions project;
  project.title = tr("Save to Project");
  const QString base = path.isEmpty() ? documentName() : QFileInfo(path).completeBaseName();
  project.folder = QDir(projectsDirectory()).filePath(base);
  if (!path.isEmpty() && m_project.kind == ProjectState::Kind::None) {
    const QString folder = QFileInfo(path).absolutePath();
    const QJsonObject inspection = inspectFolder(folder);
    if (inspection.value(QStringLiteral("outer_repository")).isNull() &&
        inspection.value(QStringLiteral("git_root")).isNull()) {
      project.folder = folder;
    }
  }
  qInfo().noquote() << QStringLiteral("Save to Project: %1 into a new project").arg(path.isEmpty() ? documentName()
                                                                                                  : path);
  handleNewProject(askNewProject(this, project), path.isEmpty() ? documentName() : path);
}

bool MainWindow::saveIntoProject(const QString& root, const QString& chosen) {
  const QString from = m_filePath;
  QString project = root;
  QString target = chosen;
  if (target.isEmpty()) {
    const QString name = from.isEmpty() ? documentName() + QStringLiteral(".mitcad") : QFileInfo(from).fileName();
    target = QDir(root).filePath(name);
    const bool own = !from.isEmpty() && sameFolderPath(QFileInfo(target).absoluteFilePath(), from);
    if (from.isEmpty() || (!own && QFileInfo::exists(target))) {
      // An untitled design's name, or another design of that name there: a
      // name of its own, asked in the project picker (mitcad#174).
      ProjectPickerOptions options;
      options.title = tr("Save in a Project");
      options.text = from.isEmpty() ? tr("The name of %1 in the project, and its folder:").arg(documentName())
                                    : tr("%1 is in the project already: another name, or another folder.").arg(name);
      options.acceptText = tr("&Save");
      options.root = root;
      options.name = QFileInfo(name).completeBaseName();
      options.ownFile = from;
      const std::optional<ProjectPickerResult> picked = pickInProjects(this, options);
      if (!picked || picked->other || picked->newProject) {
        qInfo().noquote() << QStringLiteral("Save to Project cancelled");
        return false;
      }
      project = picked->root;
      target = picked->path;
    }
  }
  const bool same = !from.isEmpty() && sameFolderPath(QFileInfo(target).absoluteFilePath(), from);
  if (!writeFile(target)) {
    return false;
  }
  const QString absolute = QFileInfo(target).absoluteFilePath();
  // As Save As: the design is that file now, and the file it came from
  // stays where it was.
  if (same) {
    qInfo().noquote() << QStringLiteral("Recorded %1 in the project %2").arg(absolute, project);
  } else {
    qInfo().noquote() << QStringLiteral("Save to Project: %1 saved in %2").arg(absolute, project);
  }
  showHint(tr("%1 is in the project %2 now: every save records a version.")
               .arg(QFileInfo(target).fileName(), QFileInfo(project).fileName()));
  // The panel shows it, selected.
  showDesignInPanel(absolute);
  return true;
}

std::pair<QString, QString> MainWindow::panelProjectSelection() const {
  if (m_projectsPanel == nullptr) {
    return {};
  }
  const ProjectRow* row = m_projectsPanel->project(m_projectsPanel->selectedRoot());
  if (row == nullptr || (row->kind != ProjectRow::Kind::Local && row->kind != ProjectRow::Kind::Cloud)) {
    return {};
  }
  return {row->root, m_projectsPanel->selectedFolder()};
}

std::pair<QString, QString> MainWindow::saveAsProjectPlace() const {
  if (!m_filePath.isEmpty()) {
    // A design of a project with versions: its project and folder.
    if (m_project.hasHistory() && isInside(m_filePath, m_project.root)) {
      return {m_project.root, folderInProject(m_project.root, QFileInfo(m_filePath).absolutePath())};
    }
    return {};
  }
  // An untitled design of a project's folder (an opened version).
  if (!m_projectDir.isEmpty()) {
    const ProjectState state = projectOfInspection(inspectFolder(m_projectDir));
    if (state.hasHistory()) {
      return {state.root, folderInProject(state.root, m_projectDir)};
    }
  }
  if (!m_project.hasHistory()) {
    return {};
  }
  // An untitled design while a project is current: the panel's selection,
  // else the current project's top.
  const std::pair<QString, QString> selected = panelProjectSelection();
  if (!selected.first.isEmpty()) {
    return selected;
  }
  return {m_project.root, QString()};
}

void MainWindow::showDesignInPanel(const QString& path) {
  if (m_projectsPanel == nullptr || !m_project.hasHistory() || !isInside(path, m_project.root)) {
    return;
  }
  setProjectsPanelShown(true, true);
  const QString file = QDir::cleanPath(QFileInfo(path).absoluteFilePath());
  m_projectsPanel->revealDesign(m_project.root, QDir(m_project.root).relativeFilePath(file));
}

// ---------------------------------------------------------------------------
// Open Recent

QString MainWindow::recentLabel(const QString& path) const {
  if (QFileInfo(path).suffix().compare(QLatin1String("mitcad"), Qt::CaseInsensitive) != 0) {
    return QFileInfo(path).fileName();
  }
  const QString root = qstr(find_project(rustStr(path.toUtf8())));
  return root.isEmpty() ? QFileInfo(path).fileName()
                        : QStringLiteral("%1 %2 %3").arg(QFileInfo(path).fileName(), QString(QChar(0x2014)),
                                                         QFileInfo(root).fileName());
}

} // namespace mitcad
