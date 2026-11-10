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
#include <QStandardPaths>
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

namespace mitcad {
namespace {

// The module loaded in this run (one per process, never unloaded) and the
// component it came from.
struct LoadedModule {
  CreateComponentBrowserFunction create = nullptr;
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
  const auto abi = reinterpret_cast<ComponentBrowserAbiFunction>(library->resolve(kComponentBrowserAbiSymbol));
  const auto create =
      reinterpret_cast<CreateComponentBrowserFunction>(library->resolve(kCreateComponentBrowserSymbol));
  if (abi == nullptr || create == nullptr) {
    *error = library->errorString();
    qWarning().noquote() << QStringLiteral("Component from Web: the browser module did not load: %1").arg(*error);
    return nullptr;
  }
  if (abi() != kComponentBrowserAbi) {
    *error = browserInterfaceMessage(kComponentBrowserAbi, abi());
    qWarning().noquote() << QStringLiteral("Component from Web: %1 has interface %2, not %3")
                                .arg(QDir::toNativeSeparators(path))
                                .arg(abi())
                                .arg(kComponentBrowserAbi);
    return nullptr;
  }
  loaded.create = create;
  loaded.folder = component.folder;
  loaded.version = component.version;
  qInfo().noquote() << QStringLiteral("Component from Web: loaded %1 (component %2, CEF %3)")
                           .arg(QDir::toNativeSeparators(path), component.version, component.cef);
  return create;
}

// The browsing profile's folder: cookies, sign-ins, cache.
QString browserProfileFolder() {
  const QString given = qEnvironmentVariable("MITCAD_WEB_PROFILE_DIR");
  return given.isEmpty()
             ? QDir(QStandardPaths::writableLocation(QStandardPaths::AppDataLocation)).filePath(QStringLiteral("web"))
             : QDir::cleanPath(given);
}

// What the module's window asks of MitCad: the import, the bridge's zip
// reader, MitCad's dialogs, menus, window sizes and colors, the folders.
// Owned by the window (its child), so it lives as long as the window.
class BrowserHost final : public QObject, public ComponentBrowserHost {
public:
  using Import = std::function<bool(const QString& path, WebFileKind kind, QString* error)>;
  BrowserHost(Import import, QString folder) : m_import(std::move(import)), m_folder(std::move(folder)) {}

  bool importDownload(const QString& path, WebFileKind kind, QString* error) override {
    return m_import(path, kind, error);
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

void MainWindow::registerComponentWebCommand() {
  // Always offered: the first use downloads the web browser component.
  CommandDef web;
  web.id = QStringLiteral("insert.component_web");
  web.name = tr("Component from Web…");
  web.icon = QStringLiteral("insert-component");
  web.tooltip = tr("Opens a browser whose downloads come into the design as components");
  web.kind = CommandDef::Kind::Action;
  web.mode = CommandDef::Mode::Model;
  web.tab = QStringLiteral("SOLID");
  web.group = QStringLiteral("INSERT");
  web.keywords = {QStringLiteral("web"),      QStringLiteral("browser"), QStringLiteral("download"),
                  QStringLiteral("supplier"), QStringLiteral("step"),    QStringLiteral("internet"),
                  QStringLiteral("model"),    QStringLiteral("part")};
  web.run = [this] { openComponentBrowser(); };
  m_registry->add(web);
}

void MainWindow::openComponentBrowser() {
  if (!m_componentBrowser.isNull()) {
    m_componentBrowser->show();
    m_componentBrowser->raise();
    m_componentBrowser->activateWindow();
    return;
  }
  if (!m_browserDownload.isNull()) {
    return; // the first download is under way
  }
  const QString root = browserComponentsRoot();
  QString problem;
  const std::optional<BrowserComponentInfo> installed = installedBrowserComponent(root, &problem);
  if (installed && installed->abi == kComponentBrowserAbi) {
    if (showComponentBrowser(*installed) && !installed->fromBuild) {
      updateBrowserComponent(*installed);
    }
    return;
  }
  if (installed && installed->fromBuild) {
    // A build's component (the tests'): never replaced by a download.
    sheetWarning(this, tr("Component from Web"), browserInterfaceMessage(kComponentBrowserAbi, installed->abi));
    return;
  }
  if (installed) {
    qInfo().noquote() << QStringLiteral("Component from Web: the installed component %1 has interface %2, not %3")
                             .arg(installed->version)
                             .arg(installed->abi)
                             .arg(kComponentBrowserAbi);
    downloadBrowserComponent(browserInterfaceMessage(kComponentBrowserAbi, installed->abi));
    return;
  }
  qInfo().noquote() << QStringLiteral("Component from Web: %1").arg(problem);
  downloadBrowserComponent(QString());
}

bool MainWindow::showComponentBrowser(const BrowserComponentInfo& component) {
  QString error;
  const CreateComponentBrowserFunction create = loadBrowserModule(component, &error);
  if (create == nullptr) {
    sheetWarning(this, tr("Component from Web"), tr("The web browser could not be loaded:\n%1").arg(error));
    return false;
  }
  auto* host = new BrowserHost(
      [this](const QString& path, WebFileKind kind, QString* reason) { return importWebDownload(path, kind, reason); },
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

void MainWindow::downloadBrowserComponent(const QString& why) {
  const UpdateSource source = UpdateSource::fromEnvironment();
  if (source.platform.isEmpty()) {
    sheetInformation(this, tr("Component from Web"),
                     tr("No web browser component is published for this kind of computer."));
    return;
  }
  auto* download = new BrowserComponentDownload(this);
  m_browserDownload = download;
  connect(download, &BrowserComponentDownload::failed, this, [this, download](const QString& message) {
    download->deleteLater();
    sheetWarning(this, tr("Component from Web"), message);
  });
  connect(download, &BrowserComponentDownload::manifestFetched, this,
          [this, download, source, why](const QByteArray& manifest) {
    QString error;
    const BrowserOffer offer =
        checkBrowserManifest(manifest, source.keys, source.platform, kComponentBrowserAbi, QString(), &error);
    switch (offer.status) {
    case BrowserOffer::Status::Invalid:
      download->deleteLater();
      sheetWarning(this, tr("Component from Web"),
                   tr("The web browser component could not be checked: %1")
                       .arg(error.isEmpty() ? tr("the update manifest is not valid") : error));
      return;
    case BrowserOffer::Status::None:
      download->deleteLater();
      sheetInformation(this, tr("Component from Web"),
                       tr("No web browser component is published for this kind of computer."));
      return;
    case BrowserOffer::Status::OtherInterface:
      download->deleteLater();
      sheetWarning(this, tr("Component from Web"),
                   browserPublishedInterfaceMessage(kComponentBrowserAbi, offer.abis));
      return;
    case BrowserOffer::Status::Available:
    case BrowserOffer::Status::Current:
      break;
    }
    // Asked once, with what it is and its size.
    QString question = tr("Component from Web needs MitCad's web browser component (the Chromium Embedded "
                          "Framework %1), a download of %2. Download it now?")
                           .arg(offer.cef, browserDownloadSize(offer.size));
    if (!why.isEmpty()) {
      question = why + QStringLiteral("\n\n") + question;
    }
    if (sheetQuestion(this, tr("Component from Web"), question) != QMessageBox::Yes) {
      qInfo().noquote() << QStringLiteral("Component from Web: the download of the web browser component was declined");
      download->deleteLater();
      return;
    }
    const QString root = browserComponentsRoot();
    QDir().mkpath(root);
    const QString zip = QDir(root).filePath(offer.version + QStringLiteral(".download.zip"));
    auto* progress = new QProgressDialog(tr("Downloading the web browser component…"), tr("Cancel"), 0, 1000, this);
    progress->setWindowTitle(tr("Component from Web"));
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
            [this, download, progress, manifest, source, offer, root](const QString& path) {
      progress->deleteLater();
      download->deleteLater();
      QString installError;
      const bool installed = installBrowserPackage(manifest, source.keys, source.platform, kComponentBrowserAbi, offer,
                                                   path, root, &installError);
      QFile::remove(path);
      if (!installed) {
        qWarning().noquote() << QStringLiteral("Component from Web: %1").arg(installError);
        sheetWarning(this, tr("Component from Web"), installError);
        return;
      }
      QString problem;
      const std::optional<BrowserComponentInfo> component = installedBrowserComponent(root, &problem);
      if (!component) {
        sheetWarning(this, tr("Component from Web"), problem);
        return;
      }
      showComponentBrowser(*component);
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

bool MainWindow::importWebDownload(const QString& path, WebFileKind kind, QString* error) {
  const QString file = QFileInfo(path).fileName();
  if (kind == WebFileKind::Document) {
    qInfo().noquote() << QStringLiteral("Component from Web: %1 opens as a design of its own").arg(file);
    openPath(path);
    return true;
  }
  if (m_mode != Mode::Idle) {
    *error = tr("Finish or cancel the open command first.");
    return false;
  }
  if (!canChangeModel()) {
    *error = changeBlockedReason();
    return false;
  }
  leaveStartPage();
  const QString name = webComponentName(file);
  const QString partNumber = webPartNumber(name);
  const QString absolute = QFileInfo(path).absoluteFilePath();
  const int depth = undoDepth();
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
    QJsonObject command = cmd("import_file", {{QStringLiteral("path"), absolute}});
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
  // Placed with Move: the design's window in front, the new occurrence
  // selected.
  SelectionItem item;
  item.kind = SelectKind::Component;
  item.owner = component;
  item.occurrence = occurrencePath(
      queryObject({{QStringLiteral("query"), QStringLiteral("components")}}).value(QStringLiteral("occurrences")).toArray(),
      occurrence);
  QWidget* top = window();
  if (top->isMinimized()) {
    top->showNormal();
  }
  top->raise();
  top->activateWindow();
  startCommand(QStringLiteral("solid.move"), {item}, nullptr);
  if (m_mode != Mode::Command) {
    qInfo().noquote() << QStringLiteral("Component from Web: Move did not start for %1").arg(shownName);
  }
  return true;
}

} // namespace mitcad
