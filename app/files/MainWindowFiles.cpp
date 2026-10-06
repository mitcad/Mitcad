// SPDX-License-Identifier: MIT
// The main window's File menu (U6): New, New Project (P12d,
// MainWindowVersions.cpp), Open (projects, and any file Import takes, as a
// new document), Open Recent, Recover Documents (P8c,
// MainWindowAutosave.cpp), Close, Save, Save As, Save Version and Start
// Version History (P12d), Version History (P12e), Import, Export, Insert
// Component; the INSERT
// group's Insert Mesh, Insert DXF and Insert Component; files dropped on
// the window.
#include "../MainWindow.hpp"

#include <algorithm>
#include <functional>
#include <utility>

#include <QAction>
#include <QApplication>
#include <QDir>
#include <QDragEnterEvent>
#include <QDropEvent>
#include <QFileDialog>
#include <QFileInfo>
#include <QJsonArray>
#include <QMenu>
#include <QMenuBar>
#include <QMessageBox>
#include <QMimeData>
#include <QSettings>
#include <QTimer>
#include <QUrl>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/Json.hpp"
#include "../framework/SettingsWindow.hpp"
#include "Autosave.hpp"
#include "FileDialogs.hpp"
#include "FileFormats.hpp"
#include "F3dImport.hpp"

namespace mitcad {
namespace {

const QString kRecentKey = QStringLiteral("recentFiles");
constexpr int kRecentCount = 10;

QJsonObject cmd(const char* name, QJsonObject fields = {}) {
  fields.insert(QStringLiteral("cmd"), QLatin1String(name));
  return fields;
}

// What a selection stands for when bodies are exported: bodies, and the
// bodies of faces, edges and vertices.
QStringList selectedBodies(const Selection& selection) {
  QStringList uids;
  for (const SelectionItem& item : selection) {
    const bool onBody = item.kind == SelectKind::Body || item.kind == SelectKind::Face ||
                        item.kind == SelectKind::Edge || item.kind == SelectKind::Vertex;
    if (onBody && !uids.contains(item.owner)) {
      uids << item.owner;
    }
  }
  return uids;
}

} // namespace

// ---------------------------------------------------------------------------
// Commands and the File menu

void MainWindow::registerFileCommands() {
  const auto def = [](const char* id, const QString& name, const char* icon, const QString& tooltip,
                      std::function<void()> run) {
    CommandDef d;
    d.id = QString::fromLatin1(id);
    d.name = name;
    d.icon = QString::fromLatin1(icon);
    d.tooltip = tooltip;
    d.kind = CommandDef::Kind::Action;
    d.mode = CommandDef::Mode::Model;
    d.tab = QStringLiteral("SOLID");
    d.group = QStringLiteral("INSERT");
    d.run = std::move(run);
    return d;
  };
  CommandDef mesh = def("insert.mesh", tr("Insert Mesh"), "insert-mesh",
                        tr("Brings an STL or OBJ mesh in as a mesh body"), [this] { insertMesh(); });
  mesh.pinned = true;
  mesh.keywords = {QStringLiteral("stl"), QStringLiteral("obj"), QStringLiteral("mesh")};
  m_registry->add(mesh);
  CommandDef dxf = def("insert.dxf", tr("Insert DXF"), "insert-dxf",
                       tr("Brings a DXF drawing into a new sketch on a plane"), [this] { insertDxf(); });
  dxf.pinned = true;
  dxf.keywords = {QStringLiteral("dxf"), QStringLiteral("drawing"), QStringLiteral("2d")};
  m_registry->add(dxf);
  CommandDef dxfSketch = dxf;
  dxfSketch.id = QStringLiteral("sketch.insert_dxf");
  dxfSketch.tooltip = tr("Brings a DXF drawing into the sketch");
  dxfSketch.tab = QStringLiteral("SKETCH");
  dxfSketch.mode = CommandDef::Mode::Sketch;
  m_registry->add(dxfSketch);
  CommandDef component = def("insert.component", tr("Insert Component"), "insert-component",
                             tr("Places another Mitcad design here, linked or as a copy"),
                             [this] { insertComponent(); });
  component.keywords = {QStringLiteral("assembly"), QStringLiteral("external"), QStringLiteral("mitcad")};
  m_registry->add(component);
  CommandDef importDef = def("file.import", tr("Import..."), "import",
                             tr("STEP, IGES, BRep, STL, OBJ, DXF or another design into this one; an .f3d "
                                "design as a new document with its history, a FreeCAD document with its "
                                "bodies"),
                             [this] { importFile(); });
  importDef.keywords = {QStringLiteral("step"),    QStringLiteral("iges"),  QStringLiteral("brep"),
                        QStringLiteral("f3d"),     QStringLiteral("f3z"),   QStringLiteral("freecad"),
                        QStringLiteral("fcstd"),   QStringLiteral("open")};
  m_registry->add(importDef);
  CommandDef exportDef = def("file.export", tr("Export..."), "export",
                             tr("Bodies to STEP, IGES, STL, OBJ or BRep; a sketch to DXF"),
                             [this] { exportFile(); });
  exportDef.tab.clear(); // the File menu
  exportDef.mode = CommandDef::Mode::Any;
  exportDef.keywords = {QStringLiteral("step"), QStringLiteral("stl"), QStringLiteral("dxf"),
                        QStringLiteral("save as"), QStringLiteral("3mf")};
  m_registry->add(exportDef);
  // 3D Print (mitcad#13, MainWindowPrint.cpp): the MAKE group and the File menu.
  CommandDef print = def("make.print3d", tr("3D Print..."), "print-3d",
                         tr("Sends the selected bodies, or all visible ones, to a slicer as 3MF or STL files"),
                         [this] { print3d(); });
  print.group = QStringLiteral("MAKE");
  print.pinned = true;
  print.keywords = {QStringLiteral("print"), QStringLiteral("slicer"), QStringLiteral("3mf"),
                    QStringLiteral("stl"), QStringLiteral("3d print"), QStringLiteral("make")};
  m_registry->add(print);
  CommandDef recover = def("file.recover", tr("Recover Documents..."), "recover",
                           tr("Restores changes that were not saved when Mitcad last ended without closing"),
                           [this] { recoverDocuments(false); });
  recover.tab.clear(); // the File menu
  recover.mode = CommandDef::Mode::Any;
  recover.keywords = {QStringLiteral("autosave"), QStringLiteral("crash"), QStringLiteral("restore"),
                      QStringLiteral("unsaved"), QStringLiteral("backup")};
  m_registry->add(recover);
}

void MainWindow::createFileMenu() {
  QMenu* file = menuBar()->addMenu(tr("&File"));
  file->setObjectName(QStringLiteral("fileMenu"));
  auto add = [this, file](const QString& text, const QKeySequence& shortcut, auto slot) {
    QAction* action = file->addAction(text, this, slot);
    action->setShortcut(shortcut);
    return action;
  };
  add(tr("&New"), QKeySequence::New, &MainWindow::newDocument);
  // Projects with version history (P12d).
  QAction* project = m_registry->action(QStringLiteral("file.new_project"));
  project->setText(tr("New &Project..."));
  file->addAction(project);
  add(tr("&Open..."), QKeySequence::Open, &MainWindow::openDocument);
  m_recentMenu = file->addMenu(tr("Open &Recent"));
  connect(m_recentMenu, &QMenu::aboutToShow, this, &MainWindow::updateRecentMenu);
  updateRecentMenu();
  QAction* recover = m_registry->action(QStringLiteral("file.recover"));
  recover->setText(tr("Recover &Documents..."));
  file->addAction(recover);
  add(tr("&Close"), QKeySequence(Qt::CTRL | Qt::Key_W), &MainWindow::closeDocument);
  file->addSeparator();
  add(tr("&Save"), QKeySequence::Save, &MainWindow::save);
  // QKeySequence::SaveAs has no key on Windows.
  add(tr("Save &As..."), QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_S), &MainWindow::saveAs);
  // Versions (P12d): Save records one in a project with version history;
  // Save Version with a description; Version History (P12e) lists them.
  QAction* version = m_registry->action(QStringLiteral("file.save_version"));
  version->setText(tr("Save &Version..."));
  file->addAction(version);
  QAction* history = m_registry->action(QStringLiteral("file.start_history"));
  history->setText(tr("S&tart Version History..."));
  file->addAction(history);
  QAction* versions = m_registry->action(QStringLiteral("file.version_history"));
  versions->setText(tr("Version &History..."));
  file->addAction(versions);
  // Remote repositories (P12 remote): the project's versions shared.
  file->addSeparator();
  const std::pair<const char*, QString> remote[] = {
      {"file.connect_remote", tr("Connect Pro&ject to Remote...")},
      {"file.open_remote", tr("Open Project &from Remote...")},
      {"file.sync", tr("S&ync")},
      {"file.remote_settings", tr("Remote Settin&gs...")}};
  for (const auto& [id, text] : remote) {
    QAction* action = m_registry->action(QString::fromLatin1(id));
    action->setText(text);
    file->addAction(action);
  }
  file->addSeparator();
  QAction* importAction = m_registry->action(QStringLiteral("file.import"));
  importAction->setText(tr("&Import..."));
  file->addAction(importAction);
  QAction* exportAction = m_registry->action(QStringLiteral("file.export"));
  exportAction->setText(tr("&Export..."));
  file->addAction(exportAction);
  QAction* printAction = m_registry->action(QStringLiteral("make.print3d"));
  printAction->setText(tr("&3D Print..."));
  file->addAction(printAction);
  QAction* component = m_registry->action(QStringLiteral("insert.component"));
  component->setText(tr("Insert Co&mponent..."));
  file->addAction(component);
#ifndef Q_OS_MACOS
  // macOS moves Quit to the application menu.
  file->addSeparator();
#endif
  // QuitRole: "Quit Mitcad" (Cmd+Q) in the application menu on macOS.
  add(tr("E&xit"), QKeySequence::Quit, &MainWindow::close)->setMenuRole(QAction::QuitRole);
}

void MainWindow::addRecentFile(const QString& path) {
  QSettings settings;
  QStringList files = settings.value(kRecentKey).toStringList();
  const QString absolute = QFileInfo(path).absoluteFilePath();
  files.removeAll(absolute);
  files.prepend(absolute);
  while (files.size() > kRecentCount) {
    files.removeLast();
  }
  settings.setValue(kRecentKey, files);
}

void MainWindow::updateRecentMenu() {
  if (m_recentMenu == nullptr) {
    return;
  }
  m_recentMenu->clear();
  const QStringList files = QSettings().value(kRecentKey).toStringList();
  QStringList names;
  for (int i = 0; i < files.size(); ++i) {
    const QString path = files[i];
    QAction* action = m_recentMenu->addAction(QStringLiteral("&%1 %2").arg(i + 1).arg(QFileInfo(path).fileName()),
                                              this, [this, path] { openPath(path); });
    action->setMenuRole(QAction::NoRole); // a file name is no About or Settings
    action->setToolTip(QDir::toNativeSeparators(path));
    action->setStatusTip(QDir::toNativeSeparators(path));
    names << QFileInfo(path).fileName();
  }
  if (files.isEmpty()) {
    m_recentMenu->addAction(tr("No recent files"))->setEnabled(false);
  } else {
    m_recentMenu->addSeparator();
    m_recentMenu->addAction(tr("Clear Recent Files"), this, [] { QSettings().remove(kRecentKey); });
  }
  if (m_recentMenu->isVisible() || !names.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Recent files: %1").arg(names.join(QStringLiteral(" | ")));
  }
}

// ---------------------------------------------------------------------------
// Opening

bool MainWindow::openFile(const QString& path, QString& error) {
  const QFileInfo info(path);
  const FileKind kind = fileKind(path);
  if (kind != FileKind::Project && !info.isFile()) {
    error = tr("The file does not exist.");
    return false;
  }
  switch (kind) {
  case FileKind::Project:
    return loadProject(path, error);
  case FileKind::F3d:
  case FileKind::FreeCad:
    // After the window shows: the import has a progress dialog.
    QTimer::singleShot(0, this, [this, path] { startF3dImport(path); });
    return true;
  case FileKind::Cad:
  case FileKind::Mesh:
  case FileKind::Drawing: {
    startImportedDocument(path);
    if (!importPath(path, false, &error)) {
      return false;
    }
    addRecentFile(path);
    return true;
  }
  case FileKind::None:
    break;
  }
  error = tr("Mitcad cannot open %1 files.").arg(info.suffix());
  return false;
}

void MainWindow::openPath(const QString& path) {
  if (!maybeSave()) {
    return;
  }
  QString error;
  if (!openFile(path, error) && !error.isEmpty()) { // empty: cancelled
    const QString message = tr("Could not open %1:\n%2").arg(QDir::toNativeSeparators(path), error);
    qWarning().noquote() << message;
    sheetWarning(this, tr("Mitcad"), message);
  }
}

void MainWindow::openExternalFile(const QString& path) {
  if (isMinimized()) {
    showNormal();
  }
  raise();
  activateWindow();
  if (fileKind(path) == FileKind::None) {
    const QString message = tr("Mitcad cannot open %1 files.").arg(QFileInfo(path).suffix());
    qWarning().noquote() << QStringLiteral("Could not open %1: %2").arg(path, message);
    sheetWarning(this, tr("Mitcad"), message);
    return;
  }
  openPath(path);
}

void MainWindow::startImportedDocument(const QString& path) {
  installEmptyDocument(QFileInfo(path).completeBaseName());
}

void MainWindow::closeDocument() {
  // Cmd+W in the Settings window (macOS) closes the window; the menu's Close
  // may get the key before the window's own shortcut does.
  if (auto* settings = qobject_cast<SettingsWindow*>(QApplication::activeWindow())) {
    settings->close();
    return;
  }
  if (!maybeSave()) {
    return;
  }
  installEmptyDocument(QString());
  qInfo() << "Document closed";
  showHint(tr("Create a sketch to start modelling. S searches for commands."));
}

void MainWindow::dragEnterEvent(QDragEnterEvent* event) {
  const QList<QUrl> urls = event->mimeData()->urls();
  if (std::any_of(urls.begin(), urls.end(), [](const QUrl& url) {
        return url.isLocalFile() && fileKind(url.toLocalFile()) != FileKind::None;
      })) {
    event->acceptProposedAction();
  }
}

void MainWindow::dropEvent(QDropEvent* event) {
  QStringList paths;
  for (const QUrl& url : event->mimeData()->urls()) {
    if (url.isLocalFile() && fileKind(url.toLocalFile()) != FileKind::None) {
      paths << url.toLocalFile();
    }
  }
  if (paths.isEmpty()) {
    return;
  }
  event->acceptProposedAction();
  qDebug().noquote() << QStringLiteral("Dropped %1").arg(paths.join(QStringLiteral(", ")));
  // Later, so that the drag ends first; a project opens, the rest come in.
  QTimer::singleShot(0, this, [this, paths] {
    for (const QString& path : paths) {
      if (fileKind(path) == FileKind::Project) {
        openPath(path);
      } else if (m_mode == Mode::Idle || fileKind(path) == FileKind::Drawing) {
        importPath(path, true);
      }
    }
  });
}

// ---------------------------------------------------------------------------
// Import

void MainWindow::importFile() {
  const QString path = QFileDialog::getOpenFileName(this, tr("Import"), fileDialogDirectory(), importFilter());
  if (!path.isEmpty()) {
    importPath(path, true);
  }
}

bool MainWindow::importPath(const QString& path, bool interactive, QString* error) {
  QString reason;
  bool ok = true;
  switch (fileKind(path)) {
  case FileKind::F3d:
  case FileKind::FreeCad:
    // A design with its history is a document of its own (a FreeCAD
    // document too: its history comes later).
    if (interactive && !maybeSave()) {
      return false;
    }
    startF3dImport(path);
    return true;
  case FileKind::Project:
    insertComponent(path);
    return true;
  case FileKind::Cad:
    ok = importCad(path, 1.0, &reason);
    break;
  case FileKind::Mesh: {
    double unit = 1.0;
    if (interactive) {
      const std::optional<double> chosen = askMeshUnit(this, path);
      if (!chosen) {
        return false;
      }
      unit = *chosen;
    }
    ok = importCad(path, unit, &reason);
    break;
  }
  case FileKind::Drawing:
    ok = importDrawing(path, interactive, &reason);
    break;
  case FileKind::None:
    reason = tr("Mitcad cannot import %1 files.").arg(QFileInfo(path).suffix());
    ok = false;
    break;
  }
  if (!ok) {
    if (error != nullptr) {
      *error = reason;
    }
    if (interactive && !reason.isEmpty()) {
      sheetWarning(this, tr("Import"),
                           tr("Could not import %1:\n%2").arg(QDir::toNativeSeparators(path), reason));
    }
  }
  return ok;
}

bool MainWindow::importCad(const QString& path, double unitMillimetres, QString* error) {
  if (!canChangeModel()) {
    *error = tr("Finish the command or the sketch first.");
    return false;
  }
  QJsonObject command = cmd("import_file", {{QStringLiteral("path"), QFileInfo(path).absoluteFilePath()}});
  if (fileKind(path) == FileKind::Mesh) {
    command.insert(QStringLiteral("unit_mm"), unitMillimetres);
  }
  QJsonObject result;
  if (!runModelCommand(command, &result)) {
    *error = m_lastError;
    return false;
  }
  const QJsonArray bodies = result.value(QStringLiteral("bodies")).toArray();
  qInfo().noquote() << QStringLiteral("Imported %1 as %2: %3 body(ies)")
                           .arg(QFileInfo(path).fileName(), result.value(QStringLiteral("name")).toString())
                           .arg(bodies.size());
  showHint(tr("Imported %1.").arg(QFileInfo(path).fileName()));
  m_viewer->fitAll();
  return true;
}

bool MainWindow::importDrawing(const QString& path, bool interactive, QString* error) {
  const QString file = QFileInfo(path).absoluteFilePath();
  const auto report = [this, &path](const QJsonObject& result, const QString& sketch) {
    const QJsonObject model = queryObject({{QStringLiteral("query"), QStringLiteral("sketch")},
                                           {QStringLiteral("uid"), sketch}});
    qInfo().noquote() << QStringLiteral("Inserted %1 into %2: %3 curves, %4 points, %5 texts, %6 profiles, "
                                        "%7 warnings")
                             .arg(QFileInfo(path).fileName(), model.value(QStringLiteral("name")).toString())
                             .arg(result.value(QStringLiteral("curves")).toInt())
                             .arg(result.value(QStringLiteral("points")).toInt())
                             .arg(result.value(QStringLiteral("text_count")).toInt())
                             .arg(model.value(QStringLiteral("regions")).toArray().size())
                             .arg(result.value(QStringLiteral("warnings")).toArray().size());
  };
  // The drawing's unit, layers and extent for the dialog (P9).
  QJsonObject info;
  if (interactive) {
    try {
      info = queryObject({{QStringLiteral("query"), QStringLiteral("dxf_info")}, {QStringLiteral("path"), file}});
    } catch (const std::exception& e) {
      *error = errorText(e);
      return false;
    }
  }
  // Where it goes and which layers.
  const auto importCommand = [&file](const QString& sketch, const DxfChoice& choice) {
    QJsonObject command = cmd("sketch.import_dxf", {{QStringLiteral("sketch"), sketch},
                                                    {QStringLiteral("path"), file},
                                                    {QStringLiteral("unit"), choice.unit}});
    if (choice.x != 0.0 || choice.y != 0.0) {
      command.insert(QStringLiteral("at"), QJsonArray{choice.x, choice.y});
    }
    if (!choice.layers.isEmpty()) {
      command.insert(QStringLiteral("layers"), QJsonArray::fromStringList(choice.layers));
    }
    return command;
  };
  // In sketch mode, into the sketch being edited.
  if (m_mode == Mode::Sketch) {
    DxfChoice choice;
    choice.unit = QStringLiteral("mm");
    if (interactive) {
      const std::optional<DxfChoice> chosen = askDxfInsert(this, path, {}, info);
      if (!chosen) {
        *error = QString();
        return false;
      }
      choice = *chosen;
    }
    QJsonObject result;
    if (!runCommand(importCommand(m_sketch->uid(), choice), &result)) {
      *error = m_lastError;
      return false;
    }
    refreshScene();
    report(result, m_sketch->uid());
    return true;
  }
  if (!canChangeModel()) {
    *error = tr("Finish the command first.");
    return false;
  }
  // A new sketch on a plane: an origin plane, or the selected plane or
  // planar face.
  QVector<QPair<QString, QString>> planes;
  QJsonValue selectedPlane;
  if (m_selection.size() == 1 && (m_selection.first().kind == SelectKind::Plane ||
                                  (m_selection.first().kind == SelectKind::Face &&
                                   m_selection.first().geometry == QStringLiteral("plane"))) &&
      m_selection.first().occurrence.isEmpty()) {
    const SelectionItem& item = m_selection.first();
    selectedPlane = item.kind == SelectKind::Plane
                        ? QJsonValue(item.owner)
                        : QJsonValue(QJsonObject{{QStringLiteral("face"), item.name},
                                                 {QStringLiteral("body"), item.owner}});
    planes.append({QStringLiteral("selected"), tr("Selected: %1").arg(item.describe())});
  }
  planes.append({QStringLiteral("xy"), tr("XY plane")});
  planes.append({QStringLiteral("xz"), tr("XZ plane")});
  planes.append({QStringLiteral("yz"), tr("YZ plane")});
  DxfChoice choice;
  choice.plane = planes.first().first;
  choice.unit = QStringLiteral("mm");
  if (interactive) {
    const std::optional<DxfChoice> chosen = askDxfInsert(this, path, planes, info);
    if (!chosen) {
      *error = QString();
      return false;
    }
    choice = *chosen;
  }
  const QJsonValue plane = choice.plane == QStringLiteral("selected") ? selectedPlane : QJsonValue(choice.plane);
  const int depth = undoDepth();
  QJsonObject created;
  if (!runCommand(cmd("sketch.create", {{QStringLiteral("plane"), plane}}), &created)) {
    *error = m_lastError;
    return false;
  }
  const QString sketch = created.value(QStringLiteral("uid")).toString();
  QJsonObject result;
  if (!runCommand(importCommand(sketch, choice), &result)) {
    const QString reason = m_lastError;
    while (undoDepth() > depth) {
      runCommand(cmd("undo"));
    }
    refreshScene();
    *error = reason;
    return false;
  }
  runCommand(cmd("merge_undo", {{QStringLiteral("depth"), depth},
                                {QStringLiteral("label"), tr("Insert %1").arg(QFileInfo(path).fileName())}}));
  refreshScene();
  report(result, sketch);
  showHint(tr("Inserted %1. Extrude (E) a profile of it.").arg(QFileInfo(path).fileName()));
  m_viewer->fitAll();
  return true;
}

void MainWindow::insertMesh() {
  const QString path = QFileDialog::getOpenFileName(this, tr("Insert Mesh"), fileDialogDirectory(),
                                                    tr("Meshes (*.stl *.obj);;All files (*)"));
  if (!path.isEmpty()) {
    importPath(path, true);
  }
}

void MainWindow::insertDxf() {
  const QString path = QFileDialog::getOpenFileName(this, tr("Insert DXF"), fileDialogDirectory(),
                                                    tr("DXF drawings (*.dxf);;All files (*)"));
  if (!path.isEmpty()) {
    importPath(path, true);
  }
}

void MainWindow::insertComponent(const QString& given) {
  if (!canChangeModel()) {
    return;
  }
  QString path = given;
  if (path.isEmpty()) {
    path = QFileDialog::getOpenFileName(this, tr("Insert Component"), fileDialogDirectory(),
                                        tr("Mitcad projects (*.mitcad)"));
    if (path.isEmpty()) {
      return;
    }
  }
  const std::optional<bool> linked = askLinked(this, path);
  if (!linked) {
    return;
  }
  QJsonObject command = cmd("insert_component", {{QStringLiteral("path"), QFileInfo(path).absoluteFilePath()},
                                                 {QStringLiteral("link"), *linked}});
  if (!m_filePath.isEmpty()) {
    command.insert(QStringLiteral("base"), QFileInfo(m_filePath).absolutePath());
  }
  QJsonObject result;
  if (runModelCommand(command, &result)) {
    qInfo().noquote() << QStringLiteral("Inserted component %1 (%2) from %3")
                             .arg(result.value(QStringLiteral("name")).toString(),
                                  *linked ? QStringLiteral("linked") : QStringLiteral("copy"),
                                  QFileInfo(path).fileName());
    m_viewer->fitAll();
  }
}

void MainWindow::startF3dImport(const QString& path) {
  if (m_import) {
    showError(tr("An import is running."));
    return;
  }
  // The import runs to its end, or until its dialog stops it (T1e; the
  // time limit that Preferences once had, import/timeLimit, is not read).
  m_import = new F3dImport(path, this);
  connect(m_import, &F3dImport::finished, this,
          [this, path](bool ok, const QByteArray& project, const QJsonObject& result, const QString& error) {
            const QString name = QFileInfo(path).fileName();
            if (!ok) {
              if (error.isEmpty()) {
                showHint(tr("Import of %1 cancelled.").arg(name));
                return;
              }
              const QString message = tr("Could not import %1:\n%2").arg(QDir::toNativeSeparators(path), error);
              qWarning().noquote() << QStringLiteral("Import of %1 failed: %2").arg(name, error);
              showError(message);
              sheetWarning(this, tr("Import"), message);
              return;
            }
            // The process computed the design; this one reads and computes
            // it again, with progress and cancel.
            QString reason;
            const DocumentMaker read = [&](ModelJob& job) {
              job.setStage(tr("Reading the imported %1").arg(name));
              return load_document(rustStr(project));
            };
            if (!installDocument(name, QString(), read, reason)) {
              if (reason.isEmpty()) {
                qInfo().noquote() << QStringLiteral("Opening the imported %1 cancelled").arg(name);
                showHint(tr("Import of %1 cancelled.").arg(name));
              } else {
                sheetWarning(this, tr("Import"), tr("Could not read the imported design:\n%1").arg(reason));
              }
              return;
            }
            m_documentName = QFileInfo(path).completeBaseName();
            // Not saved anywhere yet: a project read is modified until it
            // is marked saved (P8), so closing it asks, and autosave
            // keeps it at once (a long import is not to be lost).
            updateWindowTitle();
            m_autosave->saveSoon();
            addRecentFile(path);
            const int bodies = static_cast<int>(queryArray(QStringLiteral("bodies")).size());
            const QString stop = importStop(result);
            qInfo().noquote() << QStringLiteral("Imported %1: %2 bodies; items %3%4")
                                     .arg(name)
                                     .arg(bodies)
                                     .arg(importCounts(result))
                                     .arg(stop.isEmpty() ? QString() : QStringLiteral("; stopped %1").arg(stop));
            showHint(stop.isEmpty()
                         ? tr("Imported %1.").arg(name)
                         : tr("Imported %1; stopped early, the rest came in as the file's bodies.").arg(name));
            showImportReport(this, name, result, bodies);
          });
  m_import->start();
}

// ---------------------------------------------------------------------------
// Export

void MainWindow::exportFile() {
  QVector<QPair<QString, QString>> sketches;
  for (const DocumentSnapshot::Feature& feature : std::as_const(m_snapshot.features)) {
    if (feature.isSketch() && feature.isActive()) {
      sketches.append({feature.uid, feature.name});
    }
  }
  QString sketch = m_mode == Mode::Sketch ? m_sketch->uid() : QString();
  if (sketch.isEmpty() && !m_selection.isEmpty()) {
    sketch = sketchOf(m_selection.first());
  }
  const QStringList selected = selectedBodies(m_selection);
  const QString base = m_filePath.isEmpty() ? documentName() : QFileInfo(m_filePath).completeBaseName();
  const QString suggested = QDir(fileDialogDirectory()).filePath(base + (sketch.isEmpty() ? QStringLiteral(".step")
                                                                                           : QStringLiteral(".dxf")));
  const std::optional<ExportChoice> choice = askExport(this, suggested, static_cast<int>(selected.size()), sketches,
                                                       sketch, !m_snapshot.occurrences.isEmpty());
  if (!choice) {
    return;
  }
  QJsonObject command;
  if (choice->format == QStringLiteral("dxf")) {
    command = cmd("export_sketch", {{QStringLiteral("sketch"), choice->sketch}, {QStringLiteral("path"), choice->path}});
  } else {
    command = cmd("export", {{QStringLiteral("path"), choice->path}, {QStringLiteral("format"), choice->format}});
    if (choice->selectedOnly) {
      command.insert(QStringLiteral("bodies"), QJsonArray::fromStringList(selected));
    }
    if (choice->format == QStringLiteral("step")) {
      command.insert(QStringLiteral("schema"), choice->schema);
    }
    if (choice->format == QStringLiteral("step") || choice->format == QStringLiteral("iges")) {
      command.insert(QStringLiteral("unit"), choice->unit);
    }
    if (choice->format == QStringLiteral("stl") || choice->format == QStringLiteral("obj") ||
        choice->format == QStringLiteral("3mf")) {
      command.insert(QStringLiteral("refinement"), choice->refinement);
    }
    if (choice->format == QStringLiteral("3mf")) {
      // The parts in the colours the view shows (mitcad#13).
      command.insert(QStringLiteral("colors"), appearanceColors(choice->selectedOnly ? selected : QStringList()));
    }
    if (choice->format == QStringLiteral("stl")) {
      command.insert(QStringLiteral("ascii"), choice->ascii);
    }
    if (!choice->coordinates.isEmpty()) {
      // Every format where the design shows the bodies, or in their
      // components' coordinates (mitcad#19).
      command.insert(QStringLiteral("coordinates"), choice->coordinates);
    }
  }
  QJsonObject result;
  if (!runCommand(command, &result)) {
    sheetWarning(this, tr("Export"), tr("Could not export:\n%1").arg(m_lastError));
    return;
  }
  QString what = choice->format == QStringLiteral("dxf")
                     ? tr("%1 entities").arg(result.value(QStringLiteral("entities")).toInt())
                     : tr("%1 body(ies)").arg(result.value(QStringLiteral("bodies")).toArray().size());
  if (choice->format == QStringLiteral("step") || choice->format == QStringLiteral("iges")) {
    what += QStringLiteral(", ") + choice->unit;
  }
  if (choice->format != QStringLiteral("dxf") && !choice->coordinates.isEmpty()) {
    what += QStringLiteral(", %1 coordinates").arg(choice->coordinates);
  }
  qInfo().noquote() << QStringLiteral("Exported %1: %2, %3").arg(choice->path, choice->format, what);
  showHint(tr("Exported %1 (%2).").arg(QDir::toNativeSeparators(choice->path), what));
}

void MainWindow::exportSketch(const QString& sketch) {
  const QString name = featureName(sketch);
  const QString path = QFileDialog::getSaveFileName(this, tr("Save As DXF"),
                                                    QDir(fileDialogDirectory()).filePath(name + QStringLiteral(".dxf")),
                                                    tr("DXF drawings (*.dxf)"));
  if (path.isEmpty()) {
    return;
  }
  QJsonObject result;
  if (!runCommand(cmd("export_sketch", {{QStringLiteral("sketch"), sketch}, {QStringLiteral("path"), path}}),
                  &result)) {
    sheetWarning(this, tr("Save As DXF"), tr("Could not export:\n%1").arg(m_lastError));
    return;
  }
  qInfo().noquote() << QStringLiteral("Exported %1: dxf, %2 entities")
                           .arg(path)
                           .arg(result.value(QStringLiteral("entities")).toInt());
}

} // namespace mitcad
