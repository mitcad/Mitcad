// SPDX-License-Identifier: MIT
// A project's web viewer (mitcad#423): File > Project > Web Viewer turns it
// on for the current project and writes its web/ folder (every design's
// page, web/WebPage.hpp, from its newest version on disk, in a job with
// progress; the thumbnails, designs.json and the index, web/ProjectWeb.hpp),
// recorded as one version with the project's setting
// (`set_project_settings` with `paths`); Update Web Viewer writes it again
// (`commit`). A Save that records a version rewrites that design's page and
// the index first (webViewerOnSave), so that they go into the same version,
// unless Project Settings turned that off. app/COMMANDS.md, "Project web
// viewer".
#include "../MainWindow.hpp"

#include <exception>
#include <utility>
#include <vector>

#include <QDateTime>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QSaveFile>
#include <QtLogging>

#include "../framework/CommandRegistry.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/Json.hpp"
#include "../framework/ModelWorker.hpp"
#include "../files/Projects.hpp"
#include "../files/RemoteController.hpp"
#include "../files/VersionHistory.hpp"
#include "ProjectWeb.hpp"
#include "WebPage.hpp"

namespace mitcad {
namespace {

// The project's setting, as inspect_folder's `settings` (the shared
// settings) has it.
struct WebViewerState {
  bool on = false;
  bool updateOnSave = true;
};

WebViewerState webViewerState(const QJsonObject& inspection) {
  const QJsonValue viewer =
      inspection.value(QStringLiteral("settings")).toObject().value(QStringLiteral("web_viewer"));
  if (!viewer.isObject()) {
    return {};
  }
  return {true, viewer.toObject().value(QStringLiteral("update_on_save")).toBool(true)};
}

// The designs of an inspection (paths in the project, '/').
QStringList designPaths(const QJsonObject& inspection) {
  QStringList designs;
  for (const QJsonValue& value : inspection.value(QStringLiteral("designs")).toArray()) {
    designs << value.toObject().value(QStringLiteral("path")).toString();
  }
  return designs;
}

QJsonObject vcsCommand(const Project& project, const QJsonObject& command) {
  return parseObject(project.command(rustStr(compactJson(command))));
}

// Who saved a design's newest version, when, and its blob (its preview's
// key); empty when it has none.
struct NewestVersion {
  QString author;
  QDateTime time;
  QString blob;
};

NewestVersion newestVersion(const Project& project, const QString& path) {
  try {
    const QJsonArray versions =
        vcsCommand(project, {{QStringLiteral("cmd"), QStringLiteral("history")}, {QStringLiteral("path"), path}})
            .value(QStringLiteral("versions"))
            .toArray();
    if (versions.isEmpty()) {
      return {};
    }
    const QJsonObject newest = versions.first().toObject();
    return {newest.value(QStringLiteral("author")).toObject().value(QStringLiteral("name")).toString(),
            QDateTime::fromSecsSinceEpoch(newest.value(QStringLiteral("time")).toInteger()).toUTC(),
            newest.value(QStringLiteral("blob")).toString()};
  } catch (const std::exception& e) {
    qWarning().noquote() << QStringLiteral("Web viewer: no history of %1: %2").arg(path, errorText(e));
    return {};
  }
}

// The name of an author "Name <email>".
QString authorName(const QString& author) {
  const qsizetype at = author.lastIndexOf(QStringLiteral(" <"));
  return at > 0 ? author.left(at).trimmed() : author.trimmed();
}

// A version's preview (versionThumbnailPath) as web/<slug>.png; else the
// one there stays. Whether the folder has one now.
bool placeThumbnail(const QString& blob, const QString& target) {
  const QString source = blob.isEmpty() ? QString() : versionThumbnailPath(blob);
  QFile in(source);
  if (!source.isEmpty() && in.open(QIODevice::ReadOnly)) {
    const QByteArray image = in.readAll();
    QSaveFile out(target);
    if (out.open(QIODevice::WriteOnly) && out.write(image) == image.size() && out.commit()) {
      return true;
    }
    qWarning().noquote() << QStringLiteral("Web viewer: could not write %1: %2").arg(target, out.errorString());
  }
  return QFileInfo::exists(target);
}

} // namespace

// ---------------------------------------------------------------------------
// Commands

void MainWindow::registerWebViewerCommands() {
  const auto def = [this](const char* id, const QString& name, const char* icon, const QString& tooltip,
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
    // Only in a project that keeps versions: the folder goes into them.
    d.enabled = [this] { return m_project.hasHistory(); };
    d.keywords = {QStringLiteral("web"),     QStringLiteral("html"),    QStringLiteral("browser"),
                  QStringLiteral("viewer"),  QStringLiteral("share"),   QStringLiteral("publish"),
                  QStringLiteral("website"), QStringLiteral("customer")};
    return d;
  };
  m_registry->add(def("file.web_viewer", tr("Web Viewer…"), "web",
                      tr("Turns on the project's web viewer: a web page per design and an index"),
                      [this] { writeWebViewer(true); }));
  m_registry->add(def("file.update_web_viewer", tr("Update Web Viewer"), "update",
                      tr("Writes every design's web page and the index again"),
                      [this] { writeWebViewer(false); }));
}

// ---------------------------------------------------------------------------
// Web Viewer and Update Web Viewer

void MainWindow::writeWebViewer(bool turnOn) {
  const QString title = turnOn ? tr("Web Viewer") : tr("Update Web Viewer");
  if (!m_project.hasHistory()) {
    qInfo().noquote() << QStringLiteral("Web viewer: no project with versions");
    showHint(tr("The web viewer is a project's: open a design of a project that keeps versions."));
    return;
  }
  const QString root = m_project.root;
  const QString name = m_project.name();
  const QJsonObject inspection = inspectFolder(root);
  const WebViewerState state = webViewerState(inspection);
  if (!turnOn && !state.on) {
    qInfo().noquote() << QStringLiteral("Web viewer: not turned on in %1").arg(root);
    showHint(tr("The web viewer of %1 is not turned on: File › Project › Web Viewer turns it on.").arg(name));
    return;
  }
  if (inspection.value(QStringLiteral("designs_truncated")).toBool()) {
    qWarning().noquote() << QStringLiteral("Web viewer: %1 has more designs than listed").arg(root);
  }
  const QStringList designs = designPaths(inspection);
  const QHash<QString, QString> slugs = web::designSlugs(designs);
  const QString folder = QDir(root).filePath(QString::fromLatin1(web::kWebFolder));
  if (!QDir().mkpath(folder)) {
    sheetWarning(this, title, tr("Could not make the folder %1.").arg(QDir::toNativeSeparators(folder)));
    return;
  }
  // The open design's preview, when the view shows its newest version.
  if (!m_filePath.isEmpty() && !isModified() && m_versionBase && m_versionBase->path == m_filePath) {
    saveVersionThumbnail(m_versionBase->file);
  }
  // Who saved each design last, when, and its preview's key.
  std::vector<NewestVersion> newest(static_cast<std::size_t>(designs.size()));
  try {
    const rust::Box<Project> project = open_project(rustStr(root.toUtf8()));
    for (qsizetype i = 0; i < designs.size(); ++i) {
      newest[static_cast<std::size_t>(i)] = newestVersion(*project, QDir(root).filePath(designs[i]));
    }
  } catch (const std::exception& e) {
    qWarning().noquote() << QStringLiteral("Web viewer: %1: %2").arg(root, errorText(e));
  }
  // The pages, from the files as saved, on the worker (each design is
  // loaded and computed without a window); Cancel stops between designs.
  std::vector<web::PageResult> pages(static_cast<std::size_t>(designs.size()));
  bool done = false;
  QString failure;
  try {
    done = runJob(QStringLiteral("web viewer"), tr("Writing the web viewer of %1").arg(name), [&](ModelJob& job) {
      web::PageOptions options;
      for (qsizetype i = 0; i < designs.size(); ++i) {
        if (job.control().is_cancelled()) {
          throw ComputationCancelled();
        }
        job.setStage(tr("Writing the page of %1 (%2 of %3)").arg(designs[i]).arg(i + 1).arg(designs.size()));
        pages[static_cast<std::size_t>(i)] = web::writePageOfFile(
            QDir(root).filePath(designs[i]), QDir(folder).filePath(slugs.value(designs[i]) + QStringLiteral(".html")),
            options);
      }
    });
  } catch (const std::exception& e) {
    failure = errorText(e);
  }
  if (!done) {
    qInfo().noquote() << QStringLiteral("Web viewer: %1")
                             .arg(failure.isEmpty() ? QStringLiteral("cancelled") : failure);
    if (failure.isEmpty()) {
      showHint(tr("%1 canceled: nothing recorded.").arg(title));
    } else {
      sheetWarning(this, title, tr("Could not write the web viewer of %1:\n%2").arg(name, failure));
    }
    return;
  }
  std::vector<web::DesignCard> cards;
  QStringList paths;
  QStringList failed;
  for (qsizetype i = 0; i < designs.size(); ++i) {
    const web::PageResult& page = pages[static_cast<std::size_t>(i)];
    const NewestVersion& version = newest[static_cast<std::size_t>(i)];
    if (!page.ok) {
      qWarning().noquote() << QStringLiteral("Web viewer: could not write the page of %1: %2")
                                  .arg(designs[i], page.error);
      failed << QStringLiteral("%1: %2").arg(designs[i], page.error);
      continue;
    }
    for (const QString& warning : page.warnings) {
      qWarning().noquote() << QStringLiteral("Web viewer: %1: %2").arg(designs[i], warning);
    }
    web::DesignCard card;
    card.name = QFileInfo(designs[i]).completeBaseName();
    card.file = designs[i];
    card.slug = slugs.value(designs[i]);
    card.thumbnail = placeThumbnail(version.blob, QDir(folder).filePath(card.thumbnailFile()));
    card.savedBy = version.author;
    card.savedAt = version.time;
    paths << QDir(folder).filePath(card.page());
    if (card.thumbnail) {
      paths << QDir(folder).filePath(card.thumbnailFile());
    }
    qInfo().noquote() << QStringLiteral("Web viewer: page of %1: %2 (%3 bytes%4)")
                             .arg(designs[i], card.page())
                             .arg(page.bytes)
                             .arg(card.thumbnail ? QStringLiteral(", thumbnail") : QString());
    cards.push_back(card);
  }
  const web::FolderUpdate update = web::writeIndex(folder, name, cards);
  if (!update.error.isEmpty()) {
    qWarning().noquote() << QStringLiteral("Web viewer: %1").arg(update.error);
    sheetWarning(this, title, tr("Could not write the web viewer of %1: %2").arg(name, update.error));
    return;
  }
  paths << update.written << update.removed;
  qInfo().noquote() << QStringLiteral("Web viewer: wrote %1 page(s) and the index in %2%3")
                           .arg(cards.size())
                           .arg(folder,
                                update.removed.isEmpty()
                                    ? QString()
                                    : QStringLiteral(", removed %1").arg(update.removed.join(QStringLiteral(", "))));
  // One version: turned on, the setting with the folder; else the folder.
  QJsonObject command;
  if (!state.on) {
    command = {{QStringLiteral("cmd"), QStringLiteral("set_project_settings")},
               {QStringLiteral("shared"),
                QJsonObject{{QStringLiteral("web_viewer"), QJsonObject{{QStringLiteral("update_on_save"), true}}}}},
               {QStringLiteral("paths"), QJsonArray::fromStringList(paths)}};
  } else {
    command = {{QStringLiteral("cmd"), QStringLiteral("commit")},
               {QStringLiteral("paths"), QJsonArray::fromStringList(paths)},
               {QStringLiteral("message"), QStringLiteral("Update the web viewer")}};
  }
  const QString commit = recordWebViewer(root, command, title);
  if (!failed.isEmpty()) {
    sheetWarning(this, title,
                 tr("The web viewer of %1 lacks %2 design(s) whose page could not be written:\n%3")
                     .arg(name)
                     .arg(failed.size())
                     .arg(failed.join(QLatin1Char('\n'))));
  }
  showHint(tr("The web viewer of %1 is in %2: open index.html there in a browser%3.")
               .arg(name, QDir::toNativeSeparators(folder),
                    commit.isEmpty() ? QString() : tr("; recorded as a version (%1)").arg(commit.left(7))));
  updateVersionStatus();
}

QString MainWindow::recordWebViewer(const QString& root, QJsonObject command, const QString& title) {
  const bool current = m_project.hasHistory() && sameFolderPath(m_project.root, root);
  if (current) {
    // A sync moves the project's branch: the version waits for it, as Save's.
    m_remote->waitForBranch();
  }
  QJsonObject answer;
  try {
    const rust::Box<Project> project = open_project(rustStr(root.toUtf8()));
    const std::optional<QString> author = versionAuthor(*project);
    if (!author) {
      qInfo().noquote() << QStringLiteral("Web viewer: not recorded (no author)");
      return {};
    }
    command.insert(QStringLiteral("author"), *author);
    answer = vcsCommand(*project, command);
  } catch (const std::exception& e) {
    qWarning().noquote() << QStringLiteral("Web viewer: not recorded: %1").arg(errorText(e));
    sheetWarning(this, title, tr("The web viewer is written, but it could not be recorded as a version:\n%1")
                                  .arg(errorText(e)));
    return {};
  }
  for (const QJsonValue& warning : answer.value(QStringLiteral("warnings")).toArray()) {
    qWarning().noquote() << QStringLiteral("Version warning: %1").arg(warning.toString());
  }
  const QString skipped = answer.value(QStringLiteral("skipped")).toString();
  if (!skipped.isEmpty()) {
    qWarning().noquote() << QStringLiteral("Web viewer: not recorded: %1").arg(skipped);
    return {};
  }
  const QString commit = answer.value(QStringLiteral("commit")).toString();
  qInfo().noquote() << (commit.isEmpty() ? QStringLiteral("Web viewer: no new version")
                                         : QStringLiteral("Web viewer: recorded as version %1").arg(commit.left(7)));
  if (!commit.isEmpty() && current) {
    // Sent at once (unless turned off), as Save's versions are.
    m_remote->versionRecorded(root);
  }
  return commit;
}

// ---------------------------------------------------------------------------
// Save

QStringList MainWindow::webViewerOnSave(const Project& project, const QString& path, const QString& author,
                                        bool renamed) {
  QString root;
  QJsonObject status;
  try {
    const QJsonObject info = vcsCommand(project, {{QStringLiteral("cmd"), QStringLiteral("info")}});
    root = QDir::cleanPath(info.value(QStringLiteral("root")).toString());
    status = vcsCommand(project, {{QStringLiteral("cmd"), QStringLiteral("status")}, {QStringLiteral("path"), path}});
  } catch (const std::exception& e) {
    qWarning().noquote() << QStringLiteral("Web viewer: %1").arg(errorText(e));
    return {};
  }
  const QJsonObject inspection = inspectFolder(root);
  const WebViewerState state = webViewerState(inspection);
  if (!state.on) {
    return {};
  }
  const QString relative = QDir(root).relativeFilePath(path);
  if (!state.updateOnSave) {
    qInfo().noquote() << QStringLiteral("Web viewer: not updated on Save of %1 (off in Project Settings)")
                             .arg(relative);
    return {};
  }
  // A Save that records no version (nothing changed) leaves the page, which
  // would differ by its time only.
  const QString blob = status.value(QStringLiteral("file")).toString();
  if (!renamed && blob == status.value(QStringLiteral("head")).toString()) {
    qInfo().noquote() << QStringLiteral("Web viewer: %1 unchanged, its page kept").arg(relative);
    return {};
  }
  QStringList designs = designPaths(inspection);
  if (!designs.contains(relative)) {
    designs << relative;
  }
  const QHash<QString, QString> slugs = web::designSlugs(designs);
  const QString folder = QDir(root).filePath(QString::fromLatin1(web::kWebFolder));
  QDir().mkpath(folder);
  web::DesignCard saved;
  saved.name = QFileInfo(relative).completeBaseName();
  saved.file = relative;
  saved.slug = slugs.value(relative);
  saved.savedBy = authorName(author);
  saved.savedAt = QDateTime::currentDateTimeUtc();
  const QString page = QDir(folder).filePath(saved.page());
  web::PageOptions options;
  // Relative texture paths are the design file's.
  options.designFolder = QFileInfo(path).absolutePath();
  web::PageResult written;
  try {
    written = web::writePage(model(), page, options);
  } catch (const std::exception& e) {
    written.error = errorText(e);
  }
  if (!written.ok) {
    qWarning().noquote() << QStringLiteral("Web viewer: could not write the page of %1: %2")
                                .arg(relative, written.error);
    return {};
  }
  for (const QString& warning : std::as_const(written.warnings)) {
    qWarning().noquote() << QStringLiteral("Web viewer: %1: %2").arg(relative, warning);
  }
  // The version's preview (kept for Version History too), as the thumbnail.
  saveVersionThumbnail(blob);
  saved.thumbnail = placeThumbnail(blob, QDir(folder).filePath(saved.thumbnailFile()));
  // The other designs as the index had them, while they are there with the
  // same page.
  std::vector<web::DesignCard> cards{saved};
  QFile previous(QDir(folder).filePath(QStringLiteral("designs.json")));
  if (previous.open(QIODevice::ReadOnly)) {
    for (const web::DesignCard& card : web::readDesignsJson(previous.readAll())) {
      if (card.file != relative && designs.contains(card.file) && slugs.value(card.file) == card.slug &&
          QFileInfo::exists(QDir(folder).filePath(card.page()))) {
        cards.push_back(card);
      }
    }
    previous.close();
  }
  const QString name = QFileInfo(root).fileName();
  const web::FolderUpdate update = web::writeIndex(folder, name, cards);
  if (!update.error.isEmpty()) {
    qWarning().noquote() << QStringLiteral("Web viewer: %1").arg(update.error);
  }
  QStringList paths{page};
  if (saved.thumbnail) {
    paths << QDir(folder).filePath(saved.thumbnailFile());
  }
  paths << update.written << update.removed;
  qInfo().noquote() << QStringLiteral("Web viewer: wrote the page of %1 and the index in %2 (%3 bytes)")
                           .arg(relative, folder)
                           .arg(written.bytes);
  return paths;
}

} // namespace mitcad
