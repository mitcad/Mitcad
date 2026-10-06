// SPDX-License-Identifier: MIT
// Versions (P12d): in a project with version history (a folder with
// .mitcad/project.json at the root of a git repository; core/vcs), Save
// records a version of the saved file, a commit whose message is made of
// the undo steps since the last save, and File > Save Version one with a
// description. The author is git's configured user or Mitcad's settings,
// shown once and asked when neither has one. A file changed outside Mitcad
// since it was opened is noticed before Save writes over it, and a file
// renamed outside Mitcad takes its display state along and gets its rename
// recorded. File > New Project makes a project with history, File > Start
// Version History gives a design outside one history. Autosave never
// records a version (files/Autosave.hpp). File > Version History (P12e,
// files/VersionHistory.hpp) lists a file's versions and compares them;
// Open makes one a new design, Restore the latest version, Save Copy As a
// file of its own; Save keeps a preview of each version.
#include "../MainWindow.hpp"

#include <utility>

#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QFileDialog>
#include <QFileInfo>
#include <QImage>
#include <QJsonArray>
#include <QLabel>
#include <QMessageBox>
#include <QPushButton>
#include <QSaveFile>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "../framework/AppSettings.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/Json.hpp"
#include "../framework/ResultCache.hpp"
#include "RemoteController.hpp"
#include "VersionDialogs.hpp"
#include "VersionHistory.hpp"

namespace mitcad {
namespace {

QString qstr(const rust::String& text) { return QString::fromUtf8(text.data(), static_cast<qsizetype>(text.size())); }

// A version history command (core/model/src/api/commands.md, "Version
// history"); throws rust::Error when it fails.
QJsonObject vcs(const Project& project, const QJsonObject& command) {
  return parseObject(project.command(rustStr(compactJson(command))));
}

QJsonObject withPath(const char* name, const QString& path) {
  return {{QStringLiteral("cmd"), QLatin1String(name)}, {QStringLiteral("path"), path}};
}

QString shortId(const QString& id) { return id.left(7); }

// Whether two paths name the same folder.
bool sameFolder(const QString& a, const QString& b) {
  const auto normal = [](const QString& path) {
    const QFileInfo info(path);
    const QString canonical = info.canonicalFilePath();
    return canonical.isEmpty() ? QDir::cleanPath(info.absoluteFilePath()) : canonical;
  };
#ifdef _WIN32
  return normal(a).compare(normal(b), Qt::CaseInsensitive) == 0;
#else
  return normal(a) == normal(b);
#endif
}

// The number of versions of a file (its history's length).
int versionCount(const Project& project, const QString& path) {
  try {
    return static_cast<int>(vcs(project, withPath("history", path)).value(QStringLiteral("versions")).toArray().size());
  } catch (const std::exception&) {
    return 0;
  }
}

QString projectRoot(const Project& project) {
  return vcs(project, {{QStringLiteral("cmd"), QStringLiteral("info")}}).value(QStringLiteral("root")).toString();
}

// The size of a version's preview (its longer side).
constexpr int kThumbnailSize = 256;

void markDocumentSaved(Document& document) {
  document.command(rustStr(compactJson({{QStringLiteral("cmd"), QStringLiteral("mark_saved")}})));
}

// Whether two paths name the same file.
bool sameFile(const QString& a, const QString& b) {
  const auto normal = [](const QString& path) { return QDir::cleanPath(QFileInfo(path).absoluteFilePath()); };
#ifdef _WIN32
  return normal(a).compare(normal(b), Qt::CaseInsensitive) == 0;
#else
  return normal(a) == normal(b);
#endif
}

} // namespace

// ---------------------------------------------------------------------------
// Commands

void MainWindow::registerVersionCommands() {
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
                           tr("A new folder whose designs keep their versions (a git repository)"),
                           [this] { newProject(); });
  project.keywords = {QStringLiteral("project"), QStringLiteral("folder"), QStringLiteral("git"),
                      QStringLiteral("version history")};
  m_registry->add(project);
  CommandDef version = def("file.save_version", tr("Save Version..."), "save-version",
                           tr("Saves the design and records a version of it with a description"),
                           [this] { saveVersion(); });
  version.shortcut = QKeySequence(Qt::CTRL | Qt::ALT | Qt::Key_S);
  version.keywords = {QStringLiteral("version"), QStringLiteral("commit"), QStringLiteral("description"),
                      QStringLiteral("milestone"), QStringLiteral("git")};
  m_registry->add(version);
  CommandDef history = def("file.start_history", tr("Start Version History..."), "version-history",
                           tr("Keeps the versions of this design: its project or folder gets version history, "
                              "or it moves into a new project"),
                           [this] { startVersionHistory(); });
  history.keywords = {QStringLiteral("version history"), QStringLiteral("versions"), QStringLiteral("git"),
                      QStringLiteral("project")};
  m_registry->add(history);
  // P12e.
  CommandDef versions = def("file.version_history", tr("Version History..."), "versions",
                            tr("The versions of this design: compare one with another or with the open design, "
                               "open, restore or save a copy of one"),
                            [this] { showVersionHistory(); });
  versions.shortcut = QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_H);
  versions.keywords = {QStringLiteral("versions"), QStringLiteral("history"), QStringLiteral("compare"),
                       QStringLiteral("restore"), QStringLiteral("older"), QStringLiteral("git"),
                       QStringLiteral("log")};
  m_registry->add(versions);
  // Remote repositories (P12 remote): Connect, Open from Remote, Sync, ...
  m_remote->registerCommands(*m_registry);
}

// ---------------------------------------------------------------------------
// Projects

std::optional<rust::Box<Project>> MainWindow::versionedProject(const QString& path, QString* why) const {
  if (path.isEmpty()) {
    return std::nullopt;
  }
  try {
    return std::optional<rust::Box<Project>>(open_project(rustStr(path.toUtf8())));
  } catch (const std::exception& e) {
    if (why != nullptr) {
      *why = errorText(e);
    }
    return std::nullopt;
  }
}

std::optional<QString> MainWindow::chooseNewProject(const QString& title, const QString& name) {
  const QString location = projectsDirectory();
  QString suggested = name;
  for (int n = 1; suggested.isEmpty(); ++n) {
    const QString candidate = tr("Project%1").arg(n);
    if (!QFileInfo::exists(QDir(location).filePath(candidate))) {
      suggested = candidate;
    }
  }
  const std::optional<QString> dir = askNewProject(this, title, suggested, location);
  if (!dir) {
    return std::nullopt;
  }
  const QString outer = qstr(git_repository_root(rustStr(dir->toUtf8())));
  if (!outer.isEmpty()) {
    qInfo().noquote() << QStringLiteral("New project inside the git repository %1").arg(outer);
    QMessageBox box(QMessageBox::Warning, title,
                    tr("%1 is inside the git repository %2.").arg(QDir::toNativeSeparators(*dir),
                                                                 QDir::toNativeSeparators(outer)),
                    QMessageBox::Yes | QMessageBox::No, this);
    box.setInformativeText(tr("The project gets a repository of its own there, which the other one does not "
                              "record. Make the project there anyway?"));
    box.setDefaultButton(QMessageBox::No);
    prepareModal(&box);
    if (box.exec() != QMessageBox::Yes) {
      return std::nullopt;
    }
  }
  return dir;
}

bool MainWindow::makeProject(const QString& dir, bool removeOnCancel) {
  const bool existed = QFileInfo::exists(dir);
  // What making the project adds to a folder, and of it what was there.
  const QDir folder(dir);
  const QStringList made = {QStringLiteral(".git"), QStringLiteral(".mitcad/project.json"),
                            QStringLiteral(".gitattributes"), QStringLiteral(".gitignore")};
  QStringList there;
  for (const QString& name : made) {
    if (QFileInfo::exists(folder.filePath(name))) {
      there << name;
    }
  }
  // Cancelled or failed: a folder made for the project goes, and in a
  // folder that was there what was added (lines added to a .gitignore or
  // .gitattributes of its own stay).
  const auto undo = [&] {
    if (!existed) {
      if (removeOnCancel) {
        QDir(dir).removeRecursively();
      }
      return;
    }
    for (const QString& name : made) {
      const QString path = folder.filePath(name);
      if (there.contains(name)) {
        continue;
      }
      if (QFileInfo(path).isDir()) {
        QDir(path).removeRecursively();
      } else {
        QFile::remove(path);
      }
    }
    folder.rmdir(QStringLiteral(".mitcad")); // only when empty
  };
  const auto fail = [&](const QString& reason) {
    undo();
    const QString message = tr("Could not make %1 a project with version history:\n%2")
                                .arg(QDir::toNativeSeparators(dir), reason);
    qWarning().noquote() << message;
    sheetWarning(this, tr("Mitcad"), message);
    return false;
  };
  QString author;
  try {
    // The project and its repository first, so that the author git's
    // configuration gives there can be shown before the first version.
    const rust::Box<Project> project = create_project_repository(rustStr(dir.toUtf8()));
    const std::optional<QString> chosen = versionAuthor(*project);
    if (!chosen) {
      undo();
      return false;
    }
    author = *chosen;
  } catch (const std::exception& e) {
    return fail(errorText(e));
  }
  QJsonObject answer;
  try {
    answer = parseObject(init_project_history(rustStr(dir.toUtf8()), rustStr(author.toUtf8())));
  } catch (const std::exception& e) {
    return fail(errorText(e));
  }
  qInfo().noquote() << QStringLiteral("Version history started in %1: version %2 on %3")
                           .arg(QDir::cleanPath(answer.value(QStringLiteral("root")).toString()),
                                shortId(answer.value(QStringLiteral("commit")).toString()),
                                answer.value(QStringLiteral("branch")).toString());
  return true;
}

void MainWindow::newProject() {
  if (!maybeSave()) {
    return;
  }
  const std::optional<QString> dir = chooseNewProject(tr("New Project"), QString());
  if (!dir || !makeProject(*dir, true)) {
    return;
  }
  // A new design, which Save puts in the project.
  const QString name = QFileInfo(*dir).fileName();
  installEmptyDocument(name);
  m_projectDir = *dir;
  qInfo().noquote() << QStringLiteral("New project %1").arg(*dir);
  showHint(tr("Project %1 is in %2. Save (Ctrl+S) puts the design in it.")
               .arg(name, QDir::toNativeSeparators(*dir)));
}

void MainWindow::startVersionHistory() {
  if (m_filePath.isEmpty()) {
    // An untitled design is saved into a new project.
    const std::optional<QString> dir = chooseNewProject(tr("Save in a New Project"), QString());
    if (!dir || !makeProject(*dir, true)) {
      return;
    }
    m_projectDir = *dir;
    saveAs();
    return;
  }
  const QString path = m_filePath;
  const QString name = QFileInfo(path).fileName();
  if (versionedProject(path)) {
    qInfo().noquote() << QStringLiteral("Start Version History: %1 has version history already").arg(name);
    sheetInformation(this, tr("Start Version History"),
                             tr("%1 has version history already: every save records a version.").arg(name));
    return;
  }
  // In a project without history: its folder gets a repository.
  const QString root = qstr(find_project(rustStr(path.toUtf8())));
  if (!root.isEmpty()) {
    const QString outer = qstr(git_repository_root(rustStr(root.toUtf8())));
    if (!outer.isEmpty() && !sameFolder(outer, root)) {
      qInfo().noquote() << QStringLiteral("Start Version History dialog: the project %1 is inside %2").arg(root, outer);
      QMessageBox box(QMessageBox::Question, tr("Start Version History"),
                      tr("The project %1 is inside the git repository %2.")
                          .arg(QDir::toNativeSeparators(root), QDir::toNativeSeparators(outer)),
                      QMessageBox::Cancel, this);
      box.setInformativeText(tr("Mitcad records versions only in a repository of the project's own. Move %1 into "
                                "a new project?")
                                 .arg(name));
      QPushButton* move = box.addButton(tr("&Move to a New Project..."), QMessageBox::AcceptRole);
      prepareModal(&box);
      box.exec();
      if (box.clickedButton() == move) {
        moveToNewProject(path);
      }
      return;
    }
    qInfo().noquote() << QStringLiteral("Start Version History dialog: the project %1").arg(root);
    QMessageBox box(QMessageBox::Question, tr("Start Version History"),
                    tr("Start version history for the project %1?").arg(QDir::toNativeSeparators(root)),
                    QMessageBox::Yes | QMessageBox::Cancel, this);
    box.setInformativeText(tr("Mitcad makes a git repository in its folder, records %1 as a version now and each "
                              "of the project's designs when it is saved.")
                               .arg(name));
    box.setDefaultButton(QMessageBox::Yes);
    prepareModal(&box);
    if (box.exec() == QMessageBox::Yes && makeProject(root, false)) {
      writeFile(path);
    }
    return;
  }
  // Outside projects: the file's folder, or a new project.
  const QString folder = QFileInfo(path).absolutePath();
  QString outer = qstr(git_repository_root(rustStr(folder.toUtf8())));
  const bool repository = !outer.isEmpty() && sameFolder(outer, folder);
  if (repository) {
    outer.clear();
  }
  const std::optional<HistoryStart> choice = askStartHistory(this, name, folder, outer, repository);
  if (!choice) {
    return;
  }
  if (*choice == HistoryStart::UseFolder) {
    if (makeProject(folder, false)) {
      writeFile(path);
    }
    return;
  }
  moveToNewProject(path);
}

bool MainWindow::moveToNewProject(const QString& path) {
  const QFileInfo info(path);
  const std::optional<QString> dir = chooseNewProject(tr("Move to a New Project"), info.completeBaseName());
  if (!dir || !makeProject(*dir, true)) {
    return false;
  }
  const QString target = QDir(*dir).filePath(info.fileName());
  if (!writeFile(target)) {
    return false;
  }
  // The design is the project's now; the file it came from goes.
  if (!QFile::remove(info.absoluteFilePath())) {
    showError(tr("Moved %1 into the project %2, but could not remove the file it came from.")
                  .arg(info.fileName(), QDir::toNativeSeparators(*dir)));
    return true;
  }
  qInfo().noquote() << QStringLiteral("Moved %1 to %2").arg(info.absoluteFilePath(), target);
  showHint(tr("Moved %1 into the project %2: every save records a version.")
               .arg(info.fileName(), QDir::toNativeSeparators(*dir)));
  return true;
}

// ---------------------------------------------------------------------------
// Saving versions

std::optional<QString> MainWindow::versionAuthor(const Project& project) {
  VersionSettings settings = VersionSettings::load();
  QString gitName;
  QString gitEmail;
  try {
    // Git's configured user; an error when it has none.
    const QJsonObject identity = vcs(project, {{QStringLiteral("cmd"), QStringLiteral("identity")}});
    gitName = identity.value(QStringLiteral("name")).toString();
    gitEmail = identity.value(QStringLiteral("email")).toString();
  } catch (const std::exception&) {
  }
  const bool hasGit = !gitName.isEmpty() && !gitEmail.isEmpty();
  const bool known = (settings.useGit && hasGit) || settings.complete();
  if (!settings.confirmed || !known) {
    // Shown once: the name and email go into every version.
    const std::optional<VersionSettings> chosen = askVersionAuthor(this, gitName, gitEmail, settings);
    if (!chosen) {
      return std::nullopt;
    }
    settings = *chosen;
    settings.save();
  }
  const bool git = settings.useGit && hasGit;
  const QString author = git ? QStringLiteral("%1 <%2>").arg(gitName, gitEmail) : settings.author();
  qInfo().noquote() << QStringLiteral("Version author: %1 (%2)")
                           .arg(author, git ? QStringLiteral("git") : QStringLiteral("settings"));
  return author;
}

bool MainWindow::saveVersion() {
  QString why;
  if (m_filePath.isEmpty() || !versionedProject(m_filePath, &why)) {
    qInfo().noquote() << QStringLiteral("Save Version: no version history for %1").arg(documentName());
    QMessageBox box(QMessageBox::Question, tr("Save Version"),
                    tr("%1 has no version history.").arg(documentName()), QMessageBox::Cancel, this);
    box.setInformativeText(m_filePath.isEmpty() ? tr("Save it in a project to keep its versions.")
                                                : tr("Versions are kept for the designs of projects."));
    QPushButton* start = box.addButton(tr("&Start Version History..."), QMessageBox::AcceptRole);
    box.setDefaultButton(start);
    prepareModal(&box);
    box.exec();
    if (box.clickedButton() == start) {
      startVersionHistory();
    }
    return false;
  }
  const QJsonObject changes = queryObject({{QStringLiteral("query"), QStringLiteral("changes_since_saved")}});
  QStringList steps;
  for (const QJsonValue& step : changes.value(QStringLiteral("steps")).toArray()) {
    steps << step.toString();
  }
  const std::optional<QString> description =
      askVersionDescription(this, documentName(), automaticVersionMessage(documentName(), steps));
  if (!description) {
    return false;
  }
  return writeFile(m_filePath, *description);
}

std::optional<bool> MainWindow::askVersionConflict(const Project& project, const QString& path, const QString& author) {
  if (!m_versionBase || m_versionBase->path != path) {
    return true; // not opened from or saved to this file in its project
  }
  QJsonObject status;
  try {
    status = vcs(project, withPath("status", path));
  } catch (const std::exception&) {
    return true;
  }
  const QString file = status.value(QStringLiteral("file")).toString(); // "" when gone
  const QString head = status.value(QStringLiteral("head")).toString();
  const bool fileChanged = file != m_versionBase->file;
  // A version neither as opened nor as recorded then.
  const bool newer = head != m_versionBase->head && head != m_versionBase->file;
  if (!fileChanged && !newer) {
    return true;
  }
  // A change in the folder that no version holds yet.
  const bool unrecorded = fileChanged && !file.isEmpty() && file != head;
  const QString name = QFileInfo(path).fileName();
  qInfo().noquote() << QStringLiteral("Save conflict: %1 changed outside Mitcad (%2)")
                           .arg(path, newer ? QStringLiteral("a newer version") : QStringLiteral("the file"));
  for (;;) {
    const std::optional<ExternalChange> choice = askExternalChange(this, name, newer, unrecorded);
    if (!choice) {
      qInfo().noquote() << QStringLiteral("Save conflict: cancelled");
      return std::nullopt;
    }
    switch (*choice) {
    case ExternalChange::SaveAs:
      qInfo().noquote() << QStringLiteral("Save conflict: save as");
      return false;
    case ExternalChange::NewVersion:
      qInfo().noquote() << QStringLiteral("Save conflict: new version");
      if (unrecorded) {
        recordVersion(project, path, {path}, tr("Save %1 as changed outside Mitcad").arg(name), author);
      }
      return true;
    case ExternalChange::Compare:
      break;
    }
    // The design as it is in the folder (or, when it is gone, in the latest
    // version) against the open one; nothing is computed.
    try {
      const bool inFolder = QFileInfo::exists(path);
      const rust::Box<Document> theirs =
          inFolder ? load_project(rustStr(path.toUtf8())) : load_version(project, "HEAD", rustStr(path.toUtf8()));
      const QJsonObject options{
          {QStringLiteral("text"), true},
          {QStringLiteral("from"), inFolder ? tr("the file in the folder") : tr("the latest version")},
          {QStringLiteral("to"), tr("the open design")}};
      const QString text = qstr(diff_documents(*theirs, idleDocument(), rustStr(compactJson(options))));
      QStringList lines = text.split(QLatin1Char('\n'), Qt::SkipEmptyParts);
      qInfo().noquote() << QStringLiteral("Save conflict: compare: %1").arg(lines.join(QStringLiteral(" | ")));
      showComparison(this, tr("Compare %1").arg(name), text);
    } catch (const std::exception& e) {
      showError(tr("Could not compare %1: %2").arg(name, errorText(e)));
    }
  }
}

QStringList MainWindow::recordRename(const Project& project, const QString& path, const QString& author) {
  const QString from = m_renamedFrom;
  m_renamedFrom.clear();
  if (from.isEmpty() || QFileInfo::exists(from)) {
    return {};
  }
  try {
    // Still the content it had there: the rename is a version of its own,
    // so that the history follows it.
    const QString root = projectRoot(project);
    const QString renamed = vcs(project, withPath("status", path)).value(QStringLiteral("renamed_from")).toString();
    if (!renamed.isEmpty() && QDir::cleanPath(QDir(root).filePath(renamed)) == QDir::cleanPath(from)) {
      const QString to = QDir(root).relativeFilePath(path);
      recordVersion(project, path, {path, from}, tr("Rename %1 to %2").arg(renamed, to), author);
      return {};
    }
  } catch (const std::exception& e) {
    qWarning().noquote() << QStringLiteral("Version warning: %1").arg(errorText(e));
  }
  return {from}; // the old path goes with the version of the change
}

std::pair<QString, bool> MainWindow::recordVersion(const Project& project, const QString& path, QStringList paths,
                                                   const QString& message, const QString& author) {
  const QString name = QFileInfo(path).fileName();
  QJsonObject result;
  try {
    result = vcs(project, {{QStringLiteral("cmd"), QStringLiteral("commit")},
                           {QStringLiteral("paths"), QJsonArray::fromStringList(paths)},
                           {QStringLiteral("message"), message},
                           {QStringLiteral("author"), author}});
  } catch (const std::exception& e) {
    // The file is saved; only its version is missing.
    qWarning().noquote() << QStringLiteral("Version failed: %1").arg(errorText(e));
    return {tr("Saved %1, but the version could not be recorded: %2").arg(name, errorText(e)), true};
  }
  QStringList warnings;
  for (const QJsonValue& warning : result.value(QStringLiteral("warnings")).toArray()) {
    warnings << warning.toString();
    qWarning().noquote() << QStringLiteral("Version warning: %1").arg(warning.toString());
  }
  const QString skipped = result.value(QStringLiteral("skipped")).toString();
  if (!skipped.isEmpty()) {
    qWarning().noquote() << QStringLiteral("Version not recorded: %1").arg(skipped);
    return {tr("Saved %1, but the version was not recorded: %2").arg(name, skipped), true};
  }
  const QString commit = result.value(QStringLiteral("commit")).toString();
  const int number = versionCount(project, path);
  QString status;
  if (commit.isEmpty()) {
    qInfo().noquote() << QStringLiteral("Version unchanged: %1 (v%2)").arg(name).arg(number);
    status = number > 0 ? tr("Saved %1, no changes since version %2").arg(name).arg(number) : tr("Saved %1").arg(name);
  } else {
    qInfo().noquote() << QStringLiteral("Version recorded: %1 %2 v%3: %4")
                             .arg(name, shortId(commit))
                             .arg(number)
                             .arg(message.section(QLatin1Char('\n'), 0, 0));
    status = tr("Saved %1 as version %2 (%3)").arg(name).arg(number).arg(shortId(commit));
  }
  if (!warnings.isEmpty()) {
    return {tr("%1, but: %2").arg(status, warnings.join(QStringLiteral("; "))), true};
  }
  return {status + QLatin1Char('.'), false};
}

void MainWindow::rememberVersionBase(const Project& project, const QString& path) {
  try {
    const QJsonObject status = vcs(project, withPath("status", path));
    m_versionBase = VersionBase{path, status.value(QStringLiteral("file")).toString(),
                                status.value(QStringLiteral("head")).toString()};
  } catch (const std::exception&) {
    m_versionBase.reset();
  }
}

QString MainWindow::followRename(const Project& project, const QString& path) {
  try {
    const QJsonObject answer = vcs(project, withPath("follow_rename", path));
    const QString from = answer.value(QStringLiteral("renamed_from")).toString();
    if (from.isEmpty()) {
      return {};
    }
    qInfo().noquote() << QStringLiteral("Renamed from %1%2")
                             .arg(from, answer.value(QStringLiteral("display_moved")).toBool()
                                            ? QStringLiteral(": its display state moved along")
                                            : QString());
    return QDir::cleanPath(QDir(projectRoot(project)).filePath(from));
  } catch (const std::exception& e) {
    qWarning().noquote() << QStringLiteral("Version warning: %1").arg(errorText(e));
    return {};
  }
}

void MainWindow::updateVersionStatus() {
  QString text;
  QString logged = QStringLiteral("none");
  QString tip;
  if (std::optional<rust::Box<Project>> project = versionedProject(m_filePath)) {
    try {
      const QJsonObject info = vcs(**project, {{QStringLiteral("cmd"), QStringLiteral("info")}});
      const QString root = info.value(QStringLiteral("root")).toString();
      const QString name = QFileInfo(root).fileName();
      const QString branch = info.value(QStringLiteral("branch")).isString()
                                 ? info.value(QStringLiteral("branch")).toString()
                                 : tr("detached");
      const QJsonArray versions =
          vcs(**project, withPath("history", m_filePath)).value(QStringLiteral("versions")).toArray();
      // A middle dot between the parts (ASCII source).
      const QString dot = QStringLiteral(" ") + QChar(0x00B7) + QStringLiteral(" ");
      if (versions.isEmpty()) {
        text = name + dot + branch + dot + tr("no version yet");
        logged = QStringLiteral("%1, %2, no version").arg(name, branch);
        tip = tr("Project %1 on branch %2; %3 has no version yet")
                  .arg(QDir::toNativeSeparators(root), branch, documentName());
      } else {
        const QJsonObject latest = versions.first().toObject();
        const QString when = QDateTime::fromSecsSinceEpoch(latest.value(QStringLiteral("time")).toInteger())
                                 .toLocalTime()
                                 .toString(QStringLiteral("yyyy-MM-dd HH:mm"));
        text = name + dot + branch + dot + tr("v%1, %2").arg(versions.size()).arg(when);
        logged = QStringLiteral("%1, %2, v%3").arg(name, branch).arg(versions.size());
        tip = tr("Project %1 on branch %2\nVersion %3 of %4 (%5), %6 by %7:\n%8")
                  .arg(QDir::toNativeSeparators(root), branch)
                  .arg(versions.size())
                  .arg(documentName(), latest.value(QStringLiteral("short_id")).toString(),
                       latest.value(QStringLiteral("date")).toString(),
                       latest.value(QStringLiteral("author")).toObject().value(QStringLiteral("name")).toString(),
                       latest.value(QStringLiteral("summary")).toString());
      }
    } catch (const std::exception& e) {
      qWarning().noquote() << QStringLiteral("Version warning: %1").arg(errorText(e));
    }
  }
  m_versionLabel->setText(text);
  m_versionLabel->setToolTip(tip);
  m_versionLabel->setVisible(!text.isEmpty());
  if (logged != m_loggedVersionStatus) {
    m_loggedVersionStatus = logged;
    qInfo().noquote() << QStringLiteral("Version status: %1").arg(logged);
  }
  // And its remote's (P12 remote).
  m_remote->refreshStatus();
}

// ---------------------------------------------------------------------------
// Remote repositories (P12 remote)

void MainWindow::createRemote() {
  RemoteController::Host host;
  host.filePath = [this] { return m_filePath; };
  host.modified = [this] { return isModified(); };
  host.save = [this] { return save(); };
  host.maybeSave = [this] { return maybeSave(); };
  host.author = [this](const Project& project) { return versionAuthor(project); };
  host.open = [this](const QString& path, QString& error) { return openFile(path, error); };
  host.versionsChanged = [this] {
    if (const std::optional<rust::Box<Project>> project = versionedProject(m_filePath)) {
      rememberVersionBase(**project, m_filePath);
    }
    updateVersionStatus();
  };
  host.status = [this](const QString& message, bool error) {
    if (error) {
      showError(message);
    } else {
      showHint(message);
    }
  };
  host.startHistory = [this] { startVersionHistory(); };
  host.whenIdle = [this](std::function<void()> call) { whenIdle(m_remote, std::move(call)); };
  m_remote = new RemoteController(*this, std::move(host), this);
}

// ---------------------------------------------------------------------------
// Version History (P12e)

void MainWindow::showVersionHistory() {
  QString why;
  if (m_filePath.isEmpty() || !versionedProject(m_filePath, &why)) {
    qInfo().noquote() << QStringLiteral("Version History: no version history for %1").arg(documentName());
    QMessageBox box(QMessageBox::Question, tr("Version History"),
                    tr("%1 has no version history.").arg(documentName()), QMessageBox::Cancel, this);
    box.setInformativeText(m_filePath.isEmpty() ? tr("Save it in a project to keep its versions.")
                                                : tr("Versions are kept for the designs of projects."));
    QPushButton* start = box.addButton(tr("&Start Version History..."), QMessageBox::AcceptRole);
    box.setDefaultButton(start);
    prepareModal(&box);
    box.exec();
    if (box.clickedButton() == start) {
      startVersionHistory();
    }
    return;
  }
  const QString file = m_filePath;
  VersionHistoryHost host;
  host.compareWithOpen = [this, file](const FileVersion& version) { return compareWithOpenDesign(file, version); };
  host.compareGeometry = [this, file](const FileVersion* before, const FileVersion& version) {
    return compareVersionGeometry(file, before, version);
  };
  host.saveCopy = [this, file](QWidget* parent, const FileVersion& version) {
    saveVersionCopy(parent, file, version);
  };
  VersionHistoryDialog::Choice choice = VersionHistoryDialog::Choice::None;
  FileVersion chosen;
  int count = 0;
  {
    // The window and its thread are gone before the choice is carried out.
    VersionHistoryDialog dialog(this, file, isModified(), host);
    prepareModal(&dialog);
    dialog.exec();
    choice = dialog.choice();
    chosen = dialog.chosen();
    count = dialog.versionCount();
  }
  switch (choice) {
  case VersionHistoryDialog::Choice::Open:
    openVersion(file, chosen);
    break;
  case VersionHistoryDialog::Choice::Restore:
    restoreVersion(file, chosen, count + 1);
    break;
  case VersionHistoryDialog::Choice::None:
    qInfo().noquote() << QStringLiteral("Version History closed");
    break;
  }
}

QString MainWindow::compareWithOpenDesign(const QString& file, const FileVersion& version) {
  try {
    // Read from its commit, nothing computed; the open design as it is.
    const rust::Box<Project> project = open_project(rustStr(file.toUtf8()));
    const rust::Box<Document> old =
        load_version(*project, rustStr(version.id.toUtf8()), rustStr(file.toUtf8()));
    const QJsonObject options{{QStringLiteral("text"), true}};
    return qstr(diff_documents(*old, idleDocument(), rustStr(compactJson(options))));
  } catch (const std::exception& e) {
    return tr("Could not compare them: %1").arg(errorText(e));
  }
}

std::optional<QString> MainWindow::compareVersionGeometry(const QString& file, const FileVersion* before,
                                                         const FileVersion& version) {
  const QByteArray path = file.toUtf8();
  const QByteArray id = version.id.toUtf8();
  const QByteArray beforeId = before != nullptr ? before->id.toUtf8() : QByteArray();
  const QByteArray options = compactJson({{QStringLiteral("text"), true}, {QStringLiteral("geometry"), true}});
  // The versions take what the result store has (P7d).
  const QByteArray store = resultStoreDirectory().toUtf8();
  const QByteArray buildId = resultStoreBuildId().toUtf8();
  const QByteArray storeLabel = QFileInfo(file).fileName().toUtf8();
  const QString labels = before != nullptr ? tr("%1 and %2").arg(before->label(), version.label())
                                           : tr("%1 and the open design").arg(version.label());
  Document& open = *m_document;
  QString text;
  const bool done = runJob(QStringLiteral("compare"), tr("Comparing %1").arg(labels), [&](ModelJob& job) {
    // A project of the worker's own: a project is used by one thread.
    const rust::Box<Project> project = open_project(rustStr(path));
    const auto computed = [&](const QByteArray& rev, const QString& name) {
      job.setStage(tr("Computing %1").arg(name));
      rust::Box<Document> document = load_version(*project, rustStr(rev), rustStr(path));
      document->set_result_store(rustStr(store), rustStr(buildId), rustStr(storeLabel));
      {
        const AttachedJob attached(*document, job);
        document->command(rustStr(compactJson({{QStringLiteral("cmd"), QStringLiteral("recompute")}})));
      }
      return document;
    };
    const rust::Box<Document> to = computed(id, version.label());
    if (before != nullptr) {
      const rust::Box<Document> from = computed(beforeId, before->label());
      text = qstr(diff_documents(*from, *to, rustStr(options)));
    } else {
      // The open design's bodies as last computed; read on the worker,
      // which has the document during the job.
      job.setStage(tr("Comparing %1").arg(labels));
      text = qstr(diff_documents(*to, open, rustStr(options)));
    }
  });
  if (!done) {
    return std::nullopt;
  }
  return text;
}

void MainWindow::openVersion(const QString& file, const FileVersion& version) {
  if (!maybeSave()) {
    return;
  }
  const QString name = QStringLiteral("%1 %2").arg(QFileInfo(file).completeBaseName(), version.label());
  const QByteArray path = file.toUtf8();
  const QByteArray id = version.id.toUtf8();
  QString root;
  if (const std::optional<rust::Box<Project>> project = versionedProject(file)) {
    try {
      root = projectRoot(**project);
    } catch (const std::exception&) {
    }
  }
  QStringList warnings;
  const DocumentMaker read = [&](ModelJob& job) {
    job.setStage(tr("Reading %1").arg(name));
    // From the version's commit, its B-rep data from the same tree; links
    // are not followed (its bodies are those saved with it).
    const rust::Box<Project> project = open_project(rustStr(path));
    rust::Box<Document> document = load_version(*project, rustStr(id), rustStr(path));
    for (const rust::String& warning : document->load_warnings()) {
      warnings << qstr(warning);
    }
    // As recorded: nothing to save until it changes.
    markDocumentSaved(*document);
    return document;
  };
  QString error;
  if (!installDocument(name, QString(), read, error)) {
    if (error.isEmpty()) {
      qInfo().noquote() << QStringLiteral("Opening %1 cancelled").arg(name);
      showHint(tr("Opening %1 cancelled.").arg(name));
    } else {
      const QString message =
          tr("Could not open %1 of %2:\n%3").arg(version.label(), QFileInfo(file).fileName(), error);
      qWarning().noquote() << message;
      sheetWarning(this, tr("Mitcad"), message);
    }
    return;
  }
  m_documentName = name;
  // Save As offers the project's folder.
  m_projectDir = root;
  updateWindowTitle();
  for (const QString& warning : std::as_const(warnings)) {
    qWarning().noquote() << QStringLiteral("Version warning: %1").arg(warning);
  }
  qInfo().noquote() << QStringLiteral("Opened version %1 of %2 as %3")
                           .arg(version.labelWithId(), QFileInfo(file).fileName(), name);
  showHint(tr("Opened %1 of %2 as a new design; Save As keeps it as a file of its own.")
               .arg(version.label(), QFileInfo(file).fileName()));
  // A version saved elsewhere gets its preview here.
  saveVersionThumbnail(version.blob);
}

void MainWindow::restoreVersion(const QString& file, const FileVersion& version, int next) {
  const QString name = QFileInfo(file).fileName();
  const bool modified = sameFile(m_filePath, file) && isModified();
  const std::optional<bool> saveFirst =
      askRestoreVersion(this, name, version.labelWithId(),
                        version.time.toString(QStringLiteral("yyyy-MM-dd HH:mm")), next, modified);
  if (!saveFirst) {
    return;
  }
  if (modified) {
    if (*saveFirst) {
      // The changes are a version of their own, before the restore.
      if (!writeFile(file)) {
        showHint(tr("Restore cancelled."));
        return;
      }
    } else {
      qInfo().noquote() << QStringLiteral("Restore: the unsaved changes dropped");
    }
  }
  QString why;
  const std::optional<rust::Box<Project>> project = versionedProject(file, &why);
  if (!project) {
    showError(tr("Could not restore %1 of %2: %3").arg(version.label(), name, why));
    return;
  }
  // A sync moves the project's branch: the restore waits for it.
  m_remote->waitForBranch();
  const std::optional<QString> author = versionAuthor(**project);
  if (!author) {
    showHint(tr("Restore cancelled."));
    return;
  }
  try {
    // The restore writes over the file: a change made there by another
    // program, which no version holds, is recorded first.
    const QJsonObject status = vcs(**project, withPath("status", file));
    const QString inFolder = status.value(QStringLiteral("file")).toString();
    if (!inFolder.isEmpty() && inFolder != status.value(QStringLiteral("head")).toString()) {
      qInfo().noquote() << QStringLiteral("Restore: %1 changed outside Mitcad, recorded first").arg(name);
      recordVersion(**project, file, {file}, tr("Save %1 as changed outside Mitcad").arg(name), *author);
    }
  } catch (const std::exception& e) {
    qWarning().noquote() << QStringLiteral("Version warning: %1").arg(errorText(e));
  }
  const QString message = tr("Restore %1 of %2 (%3)").arg(version.label(), name, version.shortId);
  QJsonObject result;
  try {
    result = vcs(**project, {{QStringLiteral("cmd"), QStringLiteral("restore")},
                             {QStringLiteral("path"), file},
                             {QStringLiteral("rev"), version.id},
                             {QStringLiteral("message"), message},
                             {QStringLiteral("author"), *author}});
  } catch (const std::exception& e) {
    const QString failure = tr("Could not restore %1 of %2:\n%3").arg(version.label(), name, errorText(e));
    qWarning().noquote() << failure;
    sheetWarning(this, tr("Mitcad"), failure);
    return;
  }
  QStringList warnings;
  for (const QJsonValue& warning : result.value(QStringLiteral("warnings")).toArray()) {
    warnings << warning.toString();
    qWarning().noquote() << QStringLiteral("Version warning: %1").arg(warning.toString());
  }
  const QString skipped = result.value(QStringLiteral("skipped")).toString();
  const QString commit = result.value(QStringLiteral("commit")).toString();
  const int number = versionCount(**project, file);
  QString status;
  bool problem = false;
  if (!skipped.isEmpty()) {
    qWarning().noquote() << QStringLiteral("Version not recorded: %1").arg(skipped);
    status = tr("Restored %1 of %2 into the file, but the version was not recorded: %3")
                 .arg(version.label(), name, skipped);
    problem = true;
  } else if (commit.isEmpty()) {
    qInfo().noquote() << QStringLiteral("Version unchanged: %1 (v%2)").arg(name).arg(number);
    status = tr("%1 of %2 is the latest version's design already.").arg(version.label(), name);
  } else {
    qInfo().noquote() << QStringLiteral("Version restored: %1 %2 as v%3 (%4)")
                             .arg(name, version.labelWithId())
                             .arg(number)
                             .arg(shortId(commit));
    status = tr("Restored %1 of %2 as version %3 (%4).").arg(version.label(), name).arg(number).arg(shortId(commit));
  }
  if (!warnings.isEmpty()) {
    status += QLatin1Char(' ') + warnings.join(QStringLiteral("; "));
    problem = true;
  }
  // The window's design is the restored one now: the file as written, with
  // what Save compares it with (rememberVersionBase) and the status bar.
  QString error;
  if (loadProject(file, error)) {
    saveVersionThumbnail(version.blob);
  } else if (error.isEmpty()) {
    status += QLatin1Char(' ') + tr("Opening it was cancelled: open %1 to see it.").arg(name);
    problem = true;
  } else {
    status += QLatin1Char(' ') + tr("It could not be opened: %1").arg(error);
    problem = true;
  }
  if (problem) {
    showError(status);
  } else {
    showHint(status);
  }
}

void MainWindow::saveVersionCopy(QWidget* parent, const QString& file, const FileVersion& version) {
  const QFileInfo info(file);
  QFileDialog dialog(parent, tr("Save Copy As"), info.absolutePath(), tr("Mitcad projects (*.mitcad)"));
  dialog.setAcceptMode(QFileDialog::AcceptSave);
  dialog.setDefaultSuffix(QStringLiteral("mitcad"));
  const QString suggested = QStringLiteral("%1 %2.mitcad").arg(info.completeBaseName(), version.label());
  dialog.selectFile(suggested);
  qInfo().noquote() << QStringLiteral("Save Copy As dialog: %1").arg(suggested);
  if (dialog.exec() != QDialog::Accepted || dialog.selectedFiles().isEmpty()) {
    qInfo().noquote() << QStringLiteral("Save Copy As cancelled");
    return;
  }
  const QString target = QFileInfo(dialog.selectedFiles().constFirst()).absoluteFilePath();
  const QString name = info.fileName();
  if (sameFile(target, file) || sameFile(target, m_filePath)) {
    sheetInformation(parent, tr("Save Copy As"),
                             tr("%1 is the design's own file: Restore makes %2 its latest version.")
                                 .arg(QFileInfo(target).fileName(), version.label()));
    return;
  }
  // The version as recorded, nothing computed: in a project its B-rep data
  // goes to the project's store, elsewhere into the file (P12a).
  try {
    const rust::Box<Project> project = open_project(rustStr(file.toUtf8()));
    const rust::Box<Document> document =
        load_version(*project, rustStr(version.id.toUtf8()), rustStr(file.toUtf8()));
    document->save_project(rustStr(target.toUtf8()), "auto");
  } catch (const std::exception& e) {
    const QString failure = tr("Could not save %1 of %2 as %3:\n%4")
                                .arg(version.label(), name, QDir::toNativeSeparators(target), errorText(e));
    qWarning().noquote() << failure;
    sheetWarning(parent, tr("Save Copy As"), failure);
    return;
  }
  qInfo().noquote() << QStringLiteral("Saved a copy of %1 %2 as %3").arg(name, version.labelWithId(), target);
  std::pair<QString, bool> status{
      tr("Saved %1 of %2 as %3.").arg(version.label(), name, QDir::toNativeSeparators(target)), false};
  // In a project with version history the copy is a version of its own.
  if (const std::optional<rust::Box<Project>> project = versionedProject(target)) {
    if (const std::optional<QString> author = versionAuthor(**project)) {
      const QString copy = QFileInfo(target).fileName();
      status = recordVersion(**project, target, {target},
                             tr("Save %1: a copy of %2 of %3 (%4)").arg(copy, version.label(), name, version.shortId),
                             *author);
    }
  }
  if (status.second) {
    showError(status.first);
  } else {
    showHint(status.first);
  }
}

void MainWindow::saveVersionThumbnail(const QString& blob) {
  const QString path = versionThumbnailPath(blob);
  if (path.isEmpty() || QFileInfo::exists(path) || !m_viewer->isVisible()) {
    return;
  }
  // The view as it is now (with Mesa in tests and in session 0 too).
  const QImage frame = m_viewer->grabFramebuffer();
  if (frame.isNull()) {
    return;
  }
  const QImage image = frame.scaled(kThumbnailSize, kThumbnailSize, Qt::KeepAspectRatio, Qt::SmoothTransformation);
  QDir().mkpath(QFileInfo(path).absolutePath());
  QSaveFile out(path);
  if (!out.open(QIODevice::WriteOnly) || !image.save(&out, "PNG") || !out.commit()) {
    qWarning().noquote() << QStringLiteral("Version preview not saved: %1").arg(out.errorString());
    return;
  }
  qInfo().noquote() << QStringLiteral("Version preview saved: %1 (%2 x %3)")
                           .arg(blob.left(7))
                           .arg(image.width())
                           .arg(image.height());
}

} // namespace mitcad
