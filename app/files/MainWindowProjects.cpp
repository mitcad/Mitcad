// SPDX-License-Identifier: MIT
// Local and Cloud projects (mitcad#89): the window's current project, File
// > New Project, Open Project, Open from Cloud, Open Read-Only, Project
// Settings and Move to a Project, and the project indicator. The dialogs
// are files/ProjectDialogs.hpp and files/ProjectSettings.hpp; the core's
// side is commands.md, "Projects".
#include "../MainWindow.hpp"

#include <utility>

#include <QDir>
#include <QFile>
#include <QFileDialog>
#include <QFileInfo>
#include <QJsonArray>
#include <QMessageBox>
#include <QPushButton>
#include <QtLogging>

#include "../framework/AppSettings.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/Json.hpp"
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
  CommandDef project = def("file.new_project", tr("New Project..."), "new-project",
                           tr("A folder whose designs keep their versions: Local on this computer, or Cloud in a "
                              "git repository that others share"),
                           [this] { newProject(); });
  project.keywords = {QStringLiteral("project"),         QStringLiteral("folder"),
                      QStringLiteral("git"),             QStringLiteral("cloud"),
                      QStringLiteral("local"),           QStringLiteral("version history"),
                      QStringLiteral("start version history"), QStringLiteral("repository")};
  m_registry->add(project);
  CommandDef openProject = def("file.open_project", tr("Open Project..."), "folder",
                               tr("Opens a project's folder: a design of it, or a folder of designs or a git "
                                  "repository made a project"),
                               [this] { openProjectFolder(); });
  openProject.keywords = {QStringLiteral("project"),    QStringLiteral("folder"), QStringLiteral("git"),
                          QStringLiteral("repository"), QStringLiteral("cloud"),  QStringLiteral("local")};
  m_registry->add(openProject);
  CommandDef readOnly = def("file.open_read_only", tr("Open Read-Only..."), "project",
                            tr("Opens a design without its edit lock: to look at it while someone else edits it"),
                            [this] { openReadOnly(); });
  readOnly.keywords = {QStringLiteral("view"), QStringLiteral("lock"), QStringLiteral("read only")};
  m_registry->add(readOnly);
  CommandDef cloud = def("file.open_remote", tr("Open from Cloud..."), "remote-open",
                         tr("Opens a Cloud project from its git repository into a new folder, or makes one in an "
                            "empty repository"),
                         [this] { openFromCloud(); });
  cloud.keywords = {QStringLiteral("clone"),  QStringLiteral("remote"), QStringLiteral("download"),
                    QStringLiteral("git"),    QStringLiteral("cloud"),  QStringLiteral("server"),
                    QStringLiteral("github"), QStringLiteral("gitlab"), QStringLiteral("forgejo")};
  m_registry->add(cloud);
  CommandDef settings = def("file.project_settings", tr("Project Settings..."), "settings",
                            tr("The current project: Local or Cloud (share it, stop syncing, its address), edit "
                               "locks and live updates for everyone, the author and syncing on this computer"),
                            [this] { showProjectSettings(); });
  settings.keywords = {QStringLiteral("connect"), QStringLiteral("remote"),     QStringLiteral("share"),
                       QStringLiteral("git"),     QStringLiteral("cloud"),      QStringLiteral("address"),
                       QStringLiteral("url"),     QStringLiteral("disconnect"), QStringLiteral("author"),
                       QStringLiteral("lock"),    QStringLiteral("mqtt"),       QStringLiteral("github")};
  m_registry->add(settings);
  CommandDef move = def("file.move_to_project", tr("Move to a Project..."), "new-project",
                        tr("Moves a design that is in no project into a new project or one you have: its versions "
                           "are kept from then on"),
                        [this] { moveToProject(); });
  move.keywords = {QStringLiteral("start version history"), QStringLiteral("version history"),
                   QStringLiteral("versions"), QStringLiteral("project"), QStringLiteral("git")};
  m_registry->add(move);
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
      return moveIntoProject(result->folder);
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
  // The author typed is Mitcad's default when there was none.
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
  if (!moving.isEmpty()) {
    return moveIntoProject(root);
  }
  openProjectDesign(root, result->design);
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
  showHint(state.kind == ProjectState::Kind::Cloud
               ? tr("Cloud project %1 is in %2: every save records a version and sends it.")
                     .arg(state.name(), QDir::toNativeSeparators(root))
               : tr("Local project %1 is in %2: every save records a version.")
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
    folder = QFileDialog::getExistingDirectory(this, tr("Open Project"), projectsDirectory());
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
  // Who records the first version: git's user, else Mitcad's default
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
  QString design = preferred;
  const QJsonObject inspection = inspectFolder(root);
  const QStringList designs = designsOf(inspection);
  if (design.isEmpty()) {
    if (designs.isEmpty()) {
      // A new design named after the project, saved as its first version.
      const QString name = QFileInfo(root).fileName();
      installEmptyDocument(name);
      const QString path = QDir(root).filePath(name + QStringLiteral(".mitcad"));
      qInfo().noquote() << QStringLiteral("Open Project: no designs in %1: a new one, %2").arg(root, path);
      return writeFile(path);
    }
    if (designs.size() == 1) {
      design = designs.first();
    } else {
      const QString last = inspection.value(QStringLiteral("last_design")).toString();
      std::optional<rust::Box<Project>> project = versionedProject(root);
      const auto preview = [&](const QString& relative) -> QString {
        if (!project) {
          return {};
        }
        try {
          const QJsonObject status = parseObject((*project)->command(rustStr(compactJson(
              {{QStringLiteral("cmd"), QStringLiteral("status")},
               {QStringLiteral("path"), QDir(root).filePath(relative)}}))));
          const QString path = versionThumbnailPath(status.value(QStringLiteral("head")).toString());
          return QFileInfo::exists(path) ? path : QString();
        } catch (const std::exception&) {
          return {};
        }
      };
      const std::optional<QString> chosen = askChooseDesign(this, QFileInfo(root).fileName(), designs,
                                                            last.isEmpty() ? designs.first() : last, preview);
      if (!chosen) {
        return false;
      }
      if (chosen->isEmpty()) {
        // New Design: untitled, Save puts it in the project.
        installEmptyDocument(QString());
        m_projectDir = root;
        showHint(tr("A new design of %1: Save (Ctrl+S) puts it in the project.").arg(QFileInfo(root).fileName()));
        return true;
      }
      design = *chosen;
    }
  }
  const QString path = QDir(root).filePath(design);
  QString error;
  if (!loadProject(path, error)) {
    if (!error.isEmpty()) {
      const QString message = tr("Could not open %1:\n%2").arg(QDir::toNativeSeparators(path), error);
      qWarning().noquote() << message;
      sheetWarning(this, tr("Mitcad"), message);
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
  openProjectDesign(root, result->design);
  const QString cls = errorClassOf(result->answer);
  if (!cls.isEmpty()) {
    m_remote->retryLater();
    showError(tr("The project %1 is made, but its versions could not be sent: %2 They are sent when the "
                 "repository can be reached.")
                  .arg(state.name(), errorMessageOf(result->answer)));
  }
}

void MainWindow::openReadOnly() {
  if (!maybeSave()) {
    return;
  }
  const QString path = QFileDialog::getOpenFileName(this, tr("Open Read-Only"), fileDialogDirectory(),
                                                    tr("Mitcad designs (*.mitcad)"));
  if (path.isEmpty()) {
    return;
  }
  // The lock controller reads it while the design opens: no edit lock is
  // taken, and the window is read-only (files/LockController.hpp).
  m_openingReadOnly = true;
  QString error;
  const bool opened = openFile(path, error);
  m_openingReadOnly = false;
  qInfo().noquote() << QStringLiteral("Open Read-Only: %1%2").arg(path, opened ? QString() : QStringLiteral(" failed"));
  if (!opened && !error.isEmpty()) {
    sheetWarning(this, tr("Mitcad"), tr("Could not open %1:\n%2").arg(QDir::toNativeSeparators(path), error));
  }
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
    box.setInformativeText(tr("Move to a Project puts the design in a project that keeps its versions."));
    QPushButton* move = box.addButton(tr("&Move to a Project..."), QMessageBox::AcceptRole);
    prepareModal(&box);
    box.exec();
    if (box.clickedButton() == move) {
      moveToProject();
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
  mitcad::showProjectSettings(this, m_project.root, host);
  updateProjectIndicator();
}

// ---------------------------------------------------------------------------
// Move to a Project

void MainWindow::moveToProject() {
  const QString path = m_filePath;
  if (m_project.hasHistory() && isInside(path, m_project.root)) {
    sheetInformation(this, tr("Move to a Project"),
                     tr("%1 is in the project %2 already: every save records a version.")
                         .arg(documentName(), m_project.name()));
    return;
  }
  const QString reason =
      m_project.kind == ProjectState::Kind::NoHistory
          ? tr("Its project %1 is inside the git repository %2, so it keeps no versions: Mitcad records versions "
               "only in a repository of the project's own.")
                .arg(m_project.name(), QDir::toNativeSeparators(m_project.outer))
          : QString();
  const std::optional<MoveTarget> target = askMoveToProject(this, documentName(), reason);
  if (!target) {
    return;
  }
  if (*target == MoveTarget::ExistingProject) {
    const QString folder = QFileDialog::getExistingDirectory(this, tr("Choose a Project"), projectsDirectory());
    if (folder.isEmpty()) {
      qInfo().noquote() << QStringLiteral("Move to a Project cancelled");
      return;
    }
    const ProjectState project = projectOfInspection(inspectFolder(folder));
    if (!project.hasHistory()) {
      qInfo().noquote() << QStringLiteral("Move to a Project: %1 is no project with versions").arg(folder);
      sheetWarning(this, tr("Move to a Project"),
                   tr("%1 is not a project that keeps versions: choose a project's folder, or a new project.")
                       .arg(QDir::toNativeSeparators(folder)));
      return;
    }
    moveIntoProject(project.root);
    return;
  }
  // A new project: by default the design's own folder (it becomes the
  // project, its designs in the first version), unless that is inside a
  // repository or a project, then a new folder named after the design.
  NewProjectOptions options;
  options.title = tr("Move to a Project");
  options.firstDesign = false;
  const QString base = path.isEmpty() ? documentName() : QFileInfo(path).completeBaseName();
  options.folder = QDir(projectsDirectory()).filePath(base);
  if (!path.isEmpty() && m_project.kind == ProjectState::Kind::None) {
    const QString folder = QFileInfo(path).absolutePath();
    const QJsonObject inspection = inspectFolder(folder);
    if (inspection.value(QStringLiteral("outer_repository")).isNull() &&
        inspection.value(QStringLiteral("git_root")).isNull()) {
      options.folder = folder;
    }
  }
  qInfo().noquote() << QStringLiteral("Move to a Project: %1 into a new project").arg(path.isEmpty() ? documentName()
                                                                                                    : path);
  handleNewProject(askNewProject(this, options), path.isEmpty() ? documentName() : path);
}

bool MainWindow::moveIntoProject(const QString& root) {
  const QString from = m_filePath;
  if (from.isEmpty()) {
    // An untitled design: saved in the project, under a name asked for.
    m_projectDir = root;
    qInfo().noquote() << QStringLiteral("Move to a Project: %1 saved in %2").arg(documentName(), root);
    return saveAs();
  }
  const QString name = QFileInfo(from).fileName();
  QString target = QDir(root).filePath(name);
  if (!from.isEmpty() && QFileInfo(target).absoluteFilePath() != QFileInfo(from).absoluteFilePath() &&
      QFileInfo::exists(target)) {
    // Another design of that name there: a name of its own.
    QFileDialog dialog(this, tr("Move to a Project"), root, tr("Mitcad projects (*.mitcad)"));
    dialog.setAcceptMode(QFileDialog::AcceptSave);
    dialog.setDefaultSuffix(QStringLiteral("mitcad"));
    dialog.selectFile(name);
    if (dialog.exec() != QDialog::Accepted || dialog.selectedFiles().isEmpty()) {
      qInfo().noquote() << QStringLiteral("Move to a Project cancelled");
      return false;
    }
    target = dialog.selectedFiles().constFirst();
  }
  const bool same = !from.isEmpty() && sameFolderPath(QFileInfo(target).absoluteFilePath(), from);
  if (!writeFile(target)) {
    return false;
  }
  if (!from.isEmpty() && !same) {
    // The design is the project's now; the file it came from goes.
    if (!QFile::remove(from)) {
      showError(tr("Moved %1 into the project %2, but could not remove the file it came from.")
                    .arg(QFileInfo(from).fileName(), QDir::toNativeSeparators(root)));
      return true;
    }
    qInfo().noquote() << QStringLiteral("Moved %1 to %2").arg(from, QFileInfo(target).absoluteFilePath());
  } else {
    qInfo().noquote() << QStringLiteral("Recorded %1 in the project %2").arg(QFileInfo(target).absoluteFilePath(), root);
  }
  showHint(tr("%1 is in the project %2 now: every save records a version.")
               .arg(QFileInfo(target).fileName(), QFileInfo(root).fileName()));
  return true;
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
