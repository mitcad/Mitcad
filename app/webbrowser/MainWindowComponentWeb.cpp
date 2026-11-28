// SPDX-License-Identifier: MIT
// Insert › Component from Web (mitcad#1579, app/COMMANDS.md "Component from
// Web"): the INSERT group's command; the web browser component (the module
// mitcad-webbrowser with the Chromium Embedded Framework's runtime, a
// package of its own: BrowserComponent.hpp) downloaded on first use, asked
// once with its size, verified with the release key and loaded at run time
// (MitCad links nothing of CEF), and a newer one of this MitCad's interface
// fetched in the background; the host the window asks for MitCad's parts;
// and a downloaded file brought into this design: a component named after
// the file, its part number from the name's first part, placed with Move.
// A design of another format (.f3d, .FCStd, ...) opens as File › Open opens
// it.
#include "../MainWindow.hpp"

#include <exception>
#include <functional>
#include <optional>
#include <utility>

#include <QAction>
#include <QCoreApplication>
#include <QDialog>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonObject>
#include <QLibrary>
#include <QMenu>
#include <QObject>
#include <QPalette>
#include <QProgressDialog>
#include <QScopeGuard>
#include <QStandardPaths>
#include <QTimer>
#include <QtLogging>

#include "../files/FileFormats.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/FitToScreen.hpp"
#include "../framework/Json.hpp"
#include "../framework/Theme.hpp"
#include "../update/UpdateSettings.hpp"
#include "BrowserComponent.hpp"
#include "BrowserComponentDownload.hpp"
#include "BrowserComponentInstall.hpp"
#include "BrowserModuleLoad.hpp"
#include "ComponentBrowserHost.hpp"
#include "ComponentWebFiles.hpp"
#include "TutorialPageRules.hpp"

namespace mitcad {
namespace {

// The module loaded in this run (one per process, never unloaded) and the
// component it came from.
struct LoadedModule {
  CreateComponentBrowserFunction create = nullptr;
  // The tutorial pane's page (mitcad#2012).
  CreateTutorialPageFunction createTutorial = nullptr;
  // The rules the module keeps for tutorial pages (mitcad#2615,
  // TutorialPageRules.hpp); 0 for a module from before them.
  int tutorialGuards = 0;
  QString folder;
  QString version;
};
LoadedModule& loadedModule() {
  static LoadedModule loaded;
  return loaded;
}

// The component's module, loaded once; null with the reason in `error`
// when it does not load or is of another interface.
CreateComponentBrowserFunction loadBrowserModule(const BrowserComponentInfo& component, QString* error) {
  LoadedModule& loaded = loadedModule();
  if (loaded.create != nullptr) {
    return loaded.create;
  }
  const QString path = QDir(component.folder).filePath(component.module);
  QLibrary* library = loadBrowserModuleFile(path, error);
  if (library == nullptr) {
    qWarning().noquote() << QStringLiteral("Component from Web: the browser module did not load: %1").arg(*error);
    return nullptr;
  }
  // The interface first (mitcad#2289): a module of another one may lack
  // this interface's entry points (interface 2 had no tutorial page), and
  // is then named by its interface, in words, rather than by a missing
  // symbol.
  const auto abi = reinterpret_cast<ComponentBrowserAbiFunction>(library->resolve(kComponentBrowserAbiSymbol));
  if (abi == nullptr) {
    *error = library->errorString();
    qWarning().noquote() << QStringLiteral("Component from Web: the browser module did not load: %1").arg(*error);
    return nullptr;
  }
  const int moduleAbi = abi();
  if (moduleAbi != kComponentBrowserAbi) {
    *error = browserModuleInterfaceMessage(kComponentBrowserAbi, moduleAbi);
    qWarning().noquote() << QStringLiteral("Component from Web: %1 has interface %2, not %3")
                                .arg(QDir::toNativeSeparators(path))
                                .arg(moduleAbi)
                                .arg(kComponentBrowserAbi);
    return nullptr;
  }
  const auto create =
      reinterpret_cast<CreateComponentBrowserFunction>(library->resolve(kCreateComponentBrowserSymbol));
  const auto createTutorial =
      reinterpret_cast<CreateTutorialPageFunction>(library->resolve(kCreateTutorialPageSymbol));
  if (create == nullptr || createTutorial == nullptr) {
    *error = library->errorString();
    qWarning().noquote() << QStringLiteral("Component from Web: the browser module did not load: %1").arg(*error);
    return nullptr;
  }
  loaded.create = create;
  loaded.createTutorial = createTutorial;
  // Not part of the interface: a component of the same interface built
  // before mitcad#2615 has no such entry and shows no tutorials.
  const auto guards = reinterpret_cast<tutorialrules::TutorialPageGuardsFunction>(
      library->resolve(tutorialrules::kTutorialPageGuardsSymbol));
  loaded.tutorialGuards = guards != nullptr ? guards() : 0;
  loaded.folder = component.folder;
  loaded.version = component.version;
  qInfo().noquote() << QStringLiteral("Component from Web: loaded %1 (component %2, CEF %3)")
                           .arg(QDir::toNativeSeparators(path), component.version, component.cef);
  return create;
}

// What the module's window asks of MitCad: the import, the bridge's zip
// reader, MitCad's dialogs, menus, window sizes and colors, the folders.
// Owned by the window (its child), so it lives as long as the window.
class BrowserHost final : public QObject, public ComponentBrowserHost {
public:
  using Import = std::function<WebImportResult(const QString& path, WebFileKind kind, QString* note)>;
  BrowserHost(Import import, QString folder) : m_import(std::move(import)), m_folder(std::move(folder)) {}

  WebImportResult importDownload(const QString& path, WebFileKind kind, QString* note) override {
    return m_import(path, kind, note);
  }
  QString zipList(const QString& zip, QString* error) override {
    try {
      const QByteArray path = zip.toUtf8();
      const rust::String json = zip_list(rustStr(path));
      return QString::fromUtf8(json.data(), static_cast<qsizetype>(json.size()));
    } catch (const std::exception& e) {
      *error = errorText(e);
      return QString();
    }
  }
  bool zipExtract(const QString& zip, const QString& name, const QString& target, QString* error) override {
    try {
      const QByteArray zipPath = zip.toUtf8();
      const QByteArray entry = name.toUtf8();
      const QByteArray targetPath = target.toUtf8();
      zip_extract(rustStr(zipPath), rustStr(entry), rustStr(targetPath));
      return true;
    } catch (const std::exception& e) {
      *error = errorText(e);
      return false;
    }
  }
  int execDialog(QDialog* dialog) override {
    prepareModal(dialog);
    return mitcad::execDialog(dialog);
  }
  QAction* execMenu(QMenu* menu, const QPoint& at) override { return mitcad::execMenu(menu, at); }
  bool ask(QWidget* parent, const QString& title, const QString& text) override {
    return sheetQuestion(parent, title, text) == QMessageBox::Yes;
  }
  void fitWindow(QWidget* window, const QSize& preferred) override {
    fitWindowToContents(window, FitMode::Fit, preferred);
  }
  ComponentBrowserColors colors(const QPalette& palette) override {
    return {themeColor(ColorRole::Window, palette), themeColor(ColorRole::Text, palette),
            themeColor(ColorRole::TextSecondary, palette), themeColor(ColorRole::Action, palette),
            themeColor(ColorRole::WarningText, palette)};
  }
  QString componentFolder() override { return m_folder; }
  QString profileFolder() override { return browserProfileFolder(); }

private:
  Import m_import;
  QString m_folder;
};

QJsonObject cmd(const char* name, QJsonObject fields = {}) {
  fields.insert(QStringLiteral("cmd"), QLatin1String(name));
  return fields;
}

// The occurrence's path of uids from the root ("O1/O4"), from the
// components query's tree; empty when it is not there.
QString occurrencePath(const QJsonArray& level, const QString& uid, const QString& parent = QString()) {
  for (const QJsonValue& value : level) {
    const QJsonObject placed = value.toObject();
    const QString own = placed.value(QStringLiteral("uid")).toString();
    const QString path = parent.isEmpty() ? own : parent + QLatin1Char('/') + own;
    if (own == uid) {
      return path;
    }
    const QString inside = occurrencePath(placed.value(QStringLiteral("children")).toArray(), uid, path);
    if (!inside.isEmpty()) {
      return inside;
    }
  }
  return QString();
}

} // namespace

QString browserProfileFolder() {
  const QString given = qEnvironmentVariable("MITCAD_WEB_PROFILE_DIR");
  return given.isEmpty()
             ? QDir(QStandardPaths::writableLocation(QStandardPaths::AppDataLocation)).filePath(QStringLiteral("web"))
             : QDir::cleanPath(given);
}

CreateTutorialPageFunction loadTutorialPageModule(const BrowserComponentInfo& component, QString* folder,
                                                  QString* error) {
  if (loadBrowserModule(component, error) == nullptr) {
    return nullptr;
  }
  // Tutorial pages only on a module that keeps their rules (mitcad#2615):
  // an older one let a page leave its folder and click for the user.
  if (loadedModule().tutorialGuards < tutorialrules::kTutorialPageGuards) {
    qWarning().noquote() << QStringLiteral("Tutorials: the web browser component %1 keeps tutorial rules %2, not %3")
                                .arg(loadedModule().version)
                                .arg(loadedModule().tutorialGuards)
                                .arg(tutorialrules::kTutorialPageGuards);
    *error = QCoreApplication::translate(
        "MainWindow", "This web browser component is too old to show tutorials safely. MitCad updates it by "
                      "itself when a newer one is available; start MitCad again later and open the tutorial then.");
    return nullptr;
  }
  *folder = loadedModule().folder;
  return loadedModule().createTutorial;
}

void MainWindow::registerComponentWebCommand() {
  // Always offered: the first use downloads the web browser component.
  CommandDef web;
  web.id = QStringLiteral("insert.component_web");
  web.name = tr("Component from Web…");
  web.icon = QStringLiteral("insert-component");
  web.tooltip = tr("Opens a browser whose downloads come into the design as components");
  web.kind = CommandDef::Kind::Action;
  web.mode = CommandDef::Mode::Model;
  web.keywords = {QStringLiteral("web"),      QStringLiteral("browser"), QStringLiteral("download"),
                  QStringLiteral("supplier"), QStringLiteral("step"),    QStringLiteral("internet"),
                  QStringLiteral("model"),    QStringLiteral("part")};
  web.run = [this] { openComponentBrowser(); };
  m_registry->add(web);

  // For tests (mitcad#1663): with MITCAD_TEST_WEB_FILE naming a local file,
  // Insert Web File (Test) brings it in as a download the browser handed
  // over, through the same import, without the web browser component.
  const QString testFile = qEnvironmentVariable("MITCAD_TEST_WEB_FILE");
  if (testFile.isEmpty()) {
    return;
  }
  CommandDef test;
  test.id = QStringLiteral("test.component_web_file");
  test.name = QStringLiteral("Insert Web File (Test)");
  test.icon = QStringLiteral("insert-component");
  test.tooltip = QStringLiteral("For tests: brings in MITCAD_TEST_WEB_FILE as Component from Web does");
  test.kind = CommandDef::Kind::Action;
  // Also while a command or a sketch is open, as a download arrives then
  // (mitcad#2281: it comes in beside an inspection, else waits).
  test.mode = CommandDef::Mode::Any;
  test.duringCommands = true;
  test.run = [this, testFile] {
    const QString file = QFileInfo(testFile).fileName();
    const WebFileCheck check = checkWebFileName(file);
    if (check.kind != WebFileKind::Component && check.kind != WebFileKind::Document) {
      qWarning().noquote() << QStringLiteral("Component from Web: the test file %1 is refused: %2").arg(file, check.note);
      return;
    }
    QString note;
    const WebImportResult result = importWebDownload(testFile, check.kind, &note);
    if (result == WebImportResult::Failed) {
      qWarning().noquote() << QStringLiteral("Component from Web: the test file %1 was not imported: %2").arg(file, note);
    } else if (result == WebImportResult::Waiting) {
      // The note the browser shows under its page.
      qInfo().noquote() << QStringLiteral("Component from Web: the test file's note: %1").arg(note);
    }
  };
  m_registry->add(test);
}

void MainWindow::openComponentBrowser() {
  if (!m_componentBrowser.isNull()) {
    m_componentBrowser->show();
    m_componentBrowser->raise();
    m_componentBrowser->activateWindow();
    return;
  }
  withBrowserComponent(tr("Component from Web"),
                       tr("Component from Web needs MitCad's web browser component (the Chromium Embedded "
                          "Framework %1), a download of %2. Download it now?"),
                       [this](const BrowserComponentInfo& component) { return showComponentBrowser(component); });
}

void MainWindow::withBrowserComponent(const QString& title, const QString& question,
                                      const std::function<bool(const BrowserComponentInfo&)>& then) {
  if (!m_browserDownload.isNull()) {
    return; // the first download is under way
  }
  const QString root = browserComponentsRoot();
  QString problem;
  const std::optional<BrowserComponentInfo> installed = installedBrowserComponent(root, &problem);
  if (installed && installed->abi == kComponentBrowserAbi) {
    if (then(*installed) && !installed->fromBuild) {
      updateBrowserComponent(*installed);
    }
    return;
  }
  if (installed && installed->fromBuild) {
    // A build's component (the tests'): never replaced by a download, so
    // the message promises none (mitcad#2289).
    qInfo().noquote() << QStringLiteral("Component from Web: the build's component %1 has interface %2, not %3")
                             .arg(QDir::toNativeSeparators(installed->folder))
                             .arg(installed->abi)
                             .arg(kComponentBrowserAbi);
    sheetWarning(this, title, browserModuleInterfaceMessage(kComponentBrowserAbi, installed->abi));
    return;
  }
  if (installed) {
    qInfo().noquote() << QStringLiteral("Component from Web: the installed component %1 has interface %2, not %3")
                             .arg(installed->version)
                             .arg(installed->abi)
                             .arg(kComponentBrowserAbi);
    downloadBrowserComponent(browserInterfaceMessage(kComponentBrowserAbi, installed->abi), title, question, then);
    return;
  }
  qInfo().noquote() << QStringLiteral("Component from Web: %1").arg(problem);
  downloadBrowserComponent(QString(), title, question, then);
}

bool MainWindow::showComponentBrowser(const BrowserComponentInfo& component) {
  QString error;
  const CreateComponentBrowserFunction create = loadBrowserModule(component, &error);
  if (create == nullptr) {
    sheetWarning(this, tr("Component from Web"), tr("The web browser could not be loaded:\n%1").arg(error));
    return false;
  }
  auto* host = new BrowserHost(
      [this](const QString& path, WebFileKind kind, QString* note) { return importWebDownload(path, kind, note); },
      loadedModule().folder);
  // A window of its own beside the design's, not kept above it, so that
  // the design's window comes to the front for Move; it ends with this
  // window (~MainWindow), its host with it.
  QDialog* window = create(host, nullptr);
  if (window == nullptr) {
    delete host;
    sheetWarning(this, tr("Component from Web"), tr("The web browser could not be started."));
    return false;
  }
  host->setParent(window);
  m_componentBrowser = window;
  m_componentBrowser->show();
  m_componentBrowser->raise();
  m_componentBrowser->activateWindow();
  return true;
}

void MainWindow::downloadBrowserComponent(const QString& why, const QString& title, const QString& question,
                                          const std::function<bool(const BrowserComponentInfo&)>& then) {
  const UpdateSource source = UpdateSource::fromEnvironment();
  if (source.platform.isEmpty()) {
    sheetInformation(this, title, tr("No web browser component is published for this kind of computer."));
    return;
  }
  auto* download = new BrowserComponentDownload(this);
  m_browserDownload = download;
  connect(download, &BrowserComponentDownload::failed, this, [this, download, title](const QString& message) {
    download->deleteLater();
    sheetWarning(this, title, message);
  });
  connect(download, &BrowserComponentDownload::manifestFetched, this,
          [this, download, source, why, title, question, then](const QByteArray& manifest) {
    QString error;
    const BrowserOffer offer =
        checkBrowserManifest(manifest, source.keys, source.platform, kComponentBrowserAbi, QString(), &error);
    switch (offer.status) {
    case BrowserOffer::Status::Invalid:
      download->deleteLater();
      sheetWarning(this, title,
                   tr("The web browser component could not be checked: %1")
                       .arg(error.isEmpty() ? tr("the update manifest is not valid") : error));
      return;
    case BrowserOffer::Status::None:
      download->deleteLater();
      sheetInformation(this, title,
                       tr("No web browser component is published for this kind of computer."));
      return;
    case BrowserOffer::Status::OtherInterface:
      download->deleteLater();
      sheetWarning(this, title,
                   browserPublishedInterfaceMessage(kComponentBrowserAbi, offer.abis));
      return;
    case BrowserOffer::Status::Available:
    case BrowserOffer::Status::Current:
      break;
    }
    // Asked once, with what it is and its size.
    QString asked = question.arg(offer.cef, browserDownloadSize(offer.size));
    if (!why.isEmpty()) {
      asked = why + QStringLiteral("\n\n") + asked;
    }
    if (sheetQuestion(this, title, asked) != QMessageBox::Yes) {
      qInfo().noquote() << QStringLiteral("Component from Web: the download of the web browser component was declined");
      download->deleteLater();
      return;
    }
    const QString root = browserComponentsRoot();
    QDir().mkpath(root);
    const QString zip = QDir(root).filePath(offer.version + QStringLiteral(".download.zip"));
    auto* progress = new QProgressDialog(tr("Downloading the web browser component…"), tr("Cancel"), 0, 1000, this);
    progress->setWindowTitle(title);
    progress->setMinimumDuration(0);
    prepareProgressDialog(progress);
    progress->setValue(0);
    connect(progress, &QProgressDialog::canceled, download, [download, progress] {
      qInfo().noquote() << QStringLiteral("Component from Web: the download of the web browser component was canceled");
      download->cancel();
      download->deleteLater();
      progress->deleteLater();
    });
    connect(download, &BrowserComponentDownload::failed, progress, &QObject::deleteLater);
    connect(download, &BrowserComponentDownload::progress, progress, [progress](qint64 received, qint64 total) {
      if (total > 0) {
        progress->setValue(static_cast<int>(received * 1000 / total));
      }
    });
    connect(download, &BrowserComponentDownload::downloaded, this,
            [this, download, progress, manifest, source, offer, root, title, then](const QString& path) {
      progress->deleteLater();
      download->deleteLater();
      QString installError;
      const bool installed = installBrowserPackage(manifest, source.keys, source.platform, kComponentBrowserAbi, offer,
                                                   path, root, &installError);
      QFile::remove(path);
      if (!installed) {
        qWarning().noquote() << QStringLiteral("Component from Web: %1").arg(installError);
        sheetWarning(this, title, installError);
        return;
      }
      QString problem;
      const std::optional<BrowserComponentInfo> component = installedBrowserComponent(root, &problem);
      if (!component) {
        sheetWarning(this, title, problem);
        return;
      }
      then(*component);
    });
    download->download(offer.url, zip, offer.size);
  });
  download->fetchManifest(source.manifestUrl);
}

void MainWindow::updateBrowserComponent(const BrowserComponentInfo& component) {
  // Once a run: a newer package of this MitCad's interface is downloaded,
  // verified and installed beside the one in use, which this run keeps;
  // MitCad uses it from its next start.
  static bool checked = false;
  if (checked) {
    return;
  }
  checked = true;
  const UpdateSource source = UpdateSource::fromEnvironment();
  if (source.platform.isEmpty() || source.keys.isEmpty()) {
    return;
  }
  auto* download = new BrowserComponentDownload(this);
  connect(download, &BrowserComponentDownload::failed, download, &QObject::deleteLater);
  const QString installedVersion = component.version;
  connect(download, &BrowserComponentDownload::manifestFetched, this,
          [this, download, source, installedVersion](const QByteArray& manifest) {
    QString error;
    const BrowserOffer offer = checkBrowserManifest(manifest, source.keys, source.platform, kComponentBrowserAbi,
                                                    installedVersion, &error);
    if (offer.status != BrowserOffer::Status::Available || offer.expired) {
      qInfo().noquote() << QStringLiteral("Component from Web: the web browser component %1 is current%2")
                               .arg(installedVersion,
                                    error.isEmpty() ? QString() : QStringLiteral(" (%1)").arg(error));
      download->deleteLater();
      return;
    }
    const QString root = browserComponentsRoot();
    const QString zip = QDir(root).filePath(offer.version + QStringLiteral(".download.zip"));
    connect(download, &BrowserComponentDownload::downloaded, this,
            [this, download, manifest, source, offer, root, installedVersion](const QString& path) {
      download->deleteLater();
      QString installError;
      const bool installed = installBrowserPackage(manifest, source.keys, source.platform, kComponentBrowserAbi, offer,
                                                   path, root, &installError);
      QFile::remove(path);
      if (!installed) {
        qWarning().noquote() << QStringLiteral("Component from Web: the update of the web browser component failed: %1")
                                    .arg(installError);
        return;
      }
      removeOldBrowserComponents(root, installedVersion);
      showStatus(tr("The web browser component was updated; MitCad uses it from its next start."),
                 NoticeLevel::Information);
    });
    download->download(offer.url, zip, offer.size);
  });
  download->fetchManifest(source.manifestUrl);
}

bool MainWindow::insertWebDownload(const QString& path, QString* error) {
  const QString file = QFileInfo(path).fileName();
  // From Idle, or beside a command that changes nothing in the design
  // (importWebDownload's gate, mitcad#2281).
  if (!canChangeModel()) {
    *error = changeBlockedReason();
    return false;
  }
  // Move waits while a file comes in (startWebDownloadMove), and starts
  // from the event loop after it, on this file or the last one brought in.
  m_webImporting = true;
  const auto moveAfter = qScopeGuard([this] {
    m_webImporting = false;
    if (m_webMovePending) {
      QTimer::singleShot(0, this, [this] { startWebDownloadMove(); });
    }
  });
  leaveStartPage();
  const QString name = webComponentName(file);
  const QString partNumber = webPartNumber(name);
  const QString absolute = QFileInfo(path).absoluteFilePath();
  const int depth = undoDepth();
  // Hidden parents the insert shows (mitcad#1995), said again with Move.
  m_shownParentsNote.clear();
  const auto undoBack = [this, depth] {
    while (undoDepth() > depth && runModelCommand(cmd("undo"))) {
    }
  };
  QJsonObject made;
  qsizetype bodies = 0;
  if (fileKind(path) == FileKind::Design) {
    // A copy, as Insert Component makes one: the design keeps it when the
    // downloaded file is gone.
    QJsonObject command = cmd("insert_component", {{QStringLiteral("path"), absolute},
                                                   {QStringLiteral("link"), false},
                                                   {QStringLiteral("name"), name}});
    if (!runModelCommand(command, &made)) {
      *error = m_lastError;
      return false;
    }
    bodies = made.value(QStringLiteral("bodies")).toArray().size();
  } else {
    // A new component of the active one holding the file's bodies (a STEP
    // assembly's parts as components inside it), as Import reads the file.
    const QString active =
        queryObject({{QStringLiteral("query"), QStringLiteral("components")}}).value(QStringLiteral("active")).toString();
    if (!runModelCommand(cmd("create_component", {{QStringLiteral("name"), name}}), &made) &&
        !runModelCommand(cmd("create_component"), &made)) {
      *error = m_lastError;
      return false;
    }
    // Straight into the component just made, not into one more of its own
    // (import_file's `wrap`, mitcad#1629).
    QJsonObject command =
        cmd("import_file", {{QStringLiteral("path"), absolute}, {QStringLiteral("wrap"), false}});
    if (fileKind(path) == FileKind::Mesh) {
      command.insert(QStringLiteral("unit_mm"), 1.0);
    }
    QJsonObject imported;
    const bool ok = runModelCommand(command, &imported);
    const QString importError = m_lastError;
    if (!active.isEmpty()) {
      runModelCommand(cmd("activate_component", {{QStringLiteral("component"), active}}));
    }
    if (!ok) {
      undoBack();
      *error = importError;
      return false;
    }
    bodies = imported.value(QStringLiteral("bodies")).toArray().size();
  }
  const QString component = made.value(QStringLiteral("component")).toString();
  const QString occurrence = made.value(QStringLiteral("occurrence")).toString();
  const QString shownName = made.value(QStringLiteral("name")).toString();
  QString storedNumber;
  if (!partNumber.isEmpty()) {
    QJsonObject set;
    if (runModelCommand(cmd("set_property", {{QStringLiteral("owner"), component},
                                             {QStringLiteral("set"), QStringLiteral("Identity")},
                                             {QStringLiteral("key"), QStringLiteral("Part number")},
                                             {QStringLiteral("type"), QStringLiteral("text")},
                                             {QStringLiteral("value"), partNumber}}),
                        &set)) {
      storedNumber = set.value(QStringLiteral("value")).toString();
    }
  }
  if (undoDepth() > depth + 1) {
    runModelCommand(cmd("merge_undo", {{QStringLiteral("depth"), depth},
                                       {QStringLiteral("label"), tr("Insert %1").arg(file)}}));
  }
  qInfo().noquote() << QStringLiteral("Component from Web: inserted %1 as component %2 (%3, occurrence %4, %5 body(ies)), "
                                      "part number %6")
                           .arg(file, shownName, component, occurrence)
                           .arg(bodies)
                           .arg(storedNumber.isEmpty() ? QStringLiteral("none") : storedNumber);
  m_viewer->fitAll();
  // Placed with Move, started from the event loop (mitcad#1839): a zip's
  // Import brings in its checked files one after another (as do the files
  // that waited for a command, mitcad#2281), and Move, a command itself,
  // would make the next one wait; so Move waits until the last is in and
  // then starts once, on it.
  m_webMoveComponent = component;
  m_webMoveOccurrence = occurrence;
  m_webMoveName = shownName;
  m_webMoveNote = m_shownParentsNote;
  m_webMovePending = true;
  return true;
}

void MainWindow::startWebDownloadMove() {
  // During a later file's import (should it turn the event loop), that
  // import's own timer starts Move after it.
  if (!m_webMovePending || m_webImporting) {
    return;
  }
  // Brought in beside an inspection (mitcad#2281): Move starts when it
  // closes (followArrivals), on the last file brought in.
  if (m_mode != Mode::Idle && arrivalGate() == ArrivalGate::Beside) {
    if (!m_webMoveWaits) {
      m_webMoveWaits = true;
      qInfo().noquote() << QStringLiteral("Component from Web: Move on %1 waits until %2 closes")
                               .arg(m_webMoveName, arrivalWaitsFor());
    }
    return;
  }
  m_webMovePending = false;
  m_webMoveWaits = false;
  // The design's window in front, the new occurrence selected.
  SelectionItem item;
  item.kind = SelectKind::Component;
  item.owner = m_webMoveComponent;
  item.occurrence = occurrencePath(
      queryObject({{QStringLiteral("query"), QStringLiteral("components")}}).value(QStringLiteral("occurrences")).toArray(),
      m_webMoveOccurrence);
  if (item.occurrence.isEmpty()) {
    // Taken back meanwhile (an undo before the inspection closed).
    qInfo().noquote() << QStringLiteral("Component from Web: Move did not start for %1: it is no longer in the design")
                             .arg(m_webMoveName);
    return;
  }
  if (m_mode != Mode::Idle) {
    qInfo().noquote() << QStringLiteral("Component from Web: Move did not start for %1").arg(m_webMoveName);
    return;
  }
  QWidget* top = window();
  if (top->isMinimized()) {
    top->showNormal();
  }
  top->raise();
  top->activateWindow();
  startCommand(QStringLiteral("solid.move"), {item}, nullptr);
  if (m_mode != Mode::Command) {
    qInfo().noquote() << QStringLiteral("Component from Web: Move did not start for %1").arg(m_webMoveName);
  } else if (!m_webMoveNote.isEmpty()) {
    // The parents the insert showed (mitcad#1995): said with Move's start,
    // and again when it closes.
    m_placementNote = m_webMoveNote;
    showHint(m_placementNote);
  }
  m_webMoveNote.clear();
}

} // namespace mitcad
