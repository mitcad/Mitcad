// SPDX-License-Identifier: MIT
// The main window's File menu (U6, mitcad#175): New Design (New too,
// mitcad#145, projects/MainWindowProjectsPanel.cpp), Open (designs, and any
// file Import takes, as a new document), Open from Project (mitcad#530,
// MainWindowProjects.cpp), Open Recent, Open Read-Only, Close,
// Save, Save As, Save with Description (P12d), Version History (P12e),
// Design Properties (mitcad#1088, browser/DesignPropertiesDialog.cpp), the
// Project submenu (mitcad#89, MainWindowProjects.cpp), Import, Export,
// Print (mitcad#1089, printing/MainWindowPaperPrint.cpp), Recover Designs
// (P8c, MainWindowAutosave.cpp); the INSERT group's Insert
// Mesh and Insert DXF, and Insert Component in the ASSEMBLE group; files
// dropped on the window.
#include "../MainWindow.hpp"
#include "framework/ModelMessages.hpp"
#include "../WindowManager.hpp"

#include <algorithm>
#include <functional>
#include <stdexcept>
#include <utility>

#include <QAction>
#include <QApplication>
#include <QDir>
#include <QDragEnterEvent>
#include <QDropEvent>
#include <QFileDialog>
#include <QFileInfo>
#include <QIcon>
#include <QJsonArray>
#include <QMenu>
#include <QMenuBar>
#include <QMessageBox>
#include <QLocale>
#include <QMimeData>
#include <QPushButton>
#include <QSettings>
#include <QTimer>
#include <QUrl>
#include <QtLogging>

#include "../browser/BrowserController.hpp"
#include "../browser/DocumentSnapshot.hpp"
#include "../framework/AppSettings.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/CommandRegistry.hpp"
#include "../framework/IdleCall.hpp"
#include "../framework/Json.hpp"
#include "../framework/WaitCursor.hpp"
#include "../report/ReportCenter.hpp"
#include "../web/WebPage.hpp"
#include "Autosave.hpp"
#include "FileCommands.hpp"
#include "FileDialogs.hpp"
#include "FileFormats.hpp"
#include "F3dImport.hpp"
#include "ImportDeviations.hpp"
#include "Print3d.hpp"
#include "RecentDesigns.hpp"
#include "StartPage.hpp"
#include "mitcad/io/mesh.hpp"

namespace mitcad {
namespace {

// File › Open Recent's list, the Start tab's too (files/RecentDesigns.hpp).
const QString kRecentKey = recent::recentFilesKey();

QJsonObject cmd(const char* name, QJsonObject fields = {}) {
  fields.insert(QStringLiteral("cmd"), QLatin1String(name));
  return fields;
}

// Why a file with a SolidCraft extension is not opened (mitcad#1622): a .sc
// file of another program, by its format key, "Not a SolidCraft design:
// …"; empty for a design and for any other file.
QString solidcraftRefusal(const QString& path) {
  const QByteArray file = QFileInfo(path).absoluteFilePath().toUtf8();
  const rust::String refused = solidcraft_file_check(rustStr(file));
  return QString::fromUtf8(refused.data(), static_cast<qsizetype>(refused.size()));
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

// A menu's entries as the log shows them (mitcad#175): the names without
// their underlined letters and dots, "-" for a separator, a submenu as
// "Name > [its entries]" (`collapsed`: "Name >" only).
QString menuEntries(const QMenu* menu, const QMenu* collapsed) {
  QStringList entries;
  for (QAction* action : menu->actions()) {
    if (action->isSeparator()) {
      entries << QStringLiteral("-");
      continue;
    }
    if (!action->isVisible()) {
      continue;
    }
    QString name = action->text().remove(QLatin1Char('&')).trimmed();
    // (Three dots or the single ellipsis, mitcad#645.)
    while (name.endsWith(QLatin1Char('.')) || name.endsWith(QChar(0x2026))) {
      name.chop(1);
    }
    if (const QMenu* submenu = action->menu()) {
      name += submenu == collapsed ? QStringLiteral(" >")
                                   : QStringLiteral(" > [%1]").arg(menuEntries(submenu, collapsed));
    }
    entries << name;
  }
  return entries.join(QStringLiteral(" | "));
}

// A finished import's or export's status (mitcad#1040, docs/ui-conventions.md
// "Messages and severity"): every one reports its warnings the same way, a
// status line warning with their count and the first, the details in the
// log (and its report, where it has one); without warnings, information.
void showDone(DocumentHost& host, const QString& done, const QStringList& warnings, const QString& what) {
  for (const QString& warning : warnings) {
    qWarning().noquote() << QStringLiteral("%1: %2").arg(what, warning);
  }
  if (warnings.isEmpty()) {
    host.showStatus(done, NoticeLevel::Information);
    return;
  }
  host.showStatus(QObject::tr("%1 %n warning(s): %2", nullptr, static_cast<int>(warnings.size()))
                      .arg(done, warnings.first()),
                  NoticeLevel::Warning);
}

// A result's warnings: its warning notices, or its `warnings` (mitcad#1040).
QStringList resultWarnings(const QJsonObject& result) {
  QStringList warnings;
  for (const QJsonValue& value : result.value(QStringLiteral("notices")).toArray()) {
    const QJsonObject notice = value.toObject();
    if (notice.value(QStringLiteral("level")).toString() != QStringLiteral("info")) {
      warnings << mitcad::modelMessage(notice.value(QStringLiteral("message")).toString());
    }
  }
  if (warnings.isEmpty()) {
    for (const QJsonValue& value : result.value(QStringLiteral("warnings")).toArray()) {
      warnings << value.toString();
    }
  }
  return warnings;
}

// The log's line of an import_file: its base feature and bodies, and the
// components and occurrences of a STEP assembly kept (mitcad#1274).
QString importedLine(const QString& name, const QJsonObject& result) {
  QString line = QStringLiteral("Imported %1 as %2: %3 body(ies)")
                     .arg(name, result.value(QStringLiteral("name")).toString())
                     .arg(result.value(QStringLiteral("bodies")).toArray().size());
  if (result.contains(QStringLiteral("components"))) {
    line += QStringLiteral(", %1 component(s), %2 occurrence(s)")
                .arg(result.value(QStringLiteral("components")).toArray().size())
                .arg(result.value(QStringLiteral("occurrences")).toInt());
  }
  // Find Repeated Parts at import (mitcad#1277): the components it made
  // and the copies their occurrences stand for.
  if (result.contains(QStringLiteral("repeated_parts"))) {
    const QJsonArray repeated =
        result.value(QStringLiteral("repeated_parts")).toObject().value(QStringLiteral("components")).toArray();
    qsizetype copies = 0;
    for (const QJsonValue& part : repeated) {
      copies += part.toObject().value(QStringLiteral("copies")).toArray().size();
    }
    line += QStringLiteral(", repeated parts: %1 component(s), %2 copies").arg(repeated.size()).arg(copies);
  }
  // The component the import wrapped the file in (mitcad#1629).
  if (const QJsonObject component = result.value(QStringLiteral("component")).toObject(); !component.isEmpty()) {
    line += QStringLiteral(", into component %1").arg(component.value(QStringLiteral("name")).toString());
  }
  return line;
}

// The status after an import_file (mitcad#1040: its warnings, whatever they
// are, as warnings with their count; the details in the log).
void showImported(DocumentHost& host, const QString& name, const QJsonObject& result) {
  showDone(host, QObject::tr("Imported %1.").arg(name), resultWarnings(result),
           QStringLiteral("Import of %1").arg(name));
}

} // namespace

// ---------------------------------------------------------------------------
// Commands and the File menu

void MainWindow::registerFileCommands() {
  // The File menu's document commands (mitcad#1090): Open, Close, Save,
  // Save As and Exit.
  FileCommandRuns runs;
  runs.open = [this] { openDocument(); };
  runs.close = [this] { closeDocument(); };
  runs.save = [this] { save(); };
  runs.saveAs = [this] { saveAs(); };
  runs.exit = [this] { if (m_windowManager != nullptr) m_windowManager->quit(); else close(); };
  // A change since it was saved (the title's mark, which updateWindowTitle
  // keeps; it refreshes Save's state too), an untitled design, or a
  // read-only window, whose Save offers a copy or a new version.
  runs.saveEnabled = [this] { return isWindowModified() || m_filePath.isEmpty() || windowReadOnly(); };
  for (const CommandDef& fileCommand : fileDocumentCommands(runs)) {
    m_registry->add(fileCommand);
  }
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
  // Register before createActions(), so the menu and command search share
  // the same live shortcut action in every document window (mitcad#1172).
  CommandDef newWindow = def("window.new", tr("New Window"), "new-design", tr("Opens another document window"),
                             [this] { if (m_windowManager != nullptr) m_windowManager->newWindow(this); });
  newWindow.shortcut = QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_N);
  newWindow.mode = CommandDef::Mode::Any;
  newWindow.duringCommands = true;
  newWindow.tab.clear();
  m_registry->add(newWindow);
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
  // SVG drawings (mitcad#473).
  CommandDef svg = def("insert.svg", tr("Insert SVG"), "insert-dxf",
                       tr("Brings an SVG drawing into a new sketch on a plane"),
                       [this] { insertSvg(); });
  svg.keywords = {QStringLiteral("svg"), QStringLiteral("logo"), QStringLiteral("icon"), QStringLiteral("vector"),
                  QStringLiteral("drawing")};
  m_registry->add(svg);
  CommandDef svgSketch = svg;
  svgSketch.id = QStringLiteral("sketch.insert_svg");
  svgSketch.tooltip = tr("Brings an SVG drawing into the sketch");
  svgSketch.tab = QStringLiteral("SKETCH");
  svgSketch.mode = CommandDef::Mode::Sketch;
  m_registry->add(svgSketch);
  CommandDef component = def("insert.component", tr("Insert Component"), "insert-component",
                             tr("Places another MitCad design here, linked or as a copy"),
                             [this] { insertComponent(); });
  component.group = QStringLiteral("ASSEMBLE");
  component.pinned = true;
  component.keywords = {QStringLiteral("assembly"), QStringLiteral("external"), QStringLiteral("mitcad")};
  m_registry->add(component);
  CommandDef openDesign = def("insert.open_design", tr("From an Open Design…"), "insert-component",
                              tr("Inserts another open design from its saved file, linked or as a copy"),
                              [this] { insertOpenDesign(); });
  openDesign.keywords = {QStringLiteral("window"), QStringLiteral("tab"), QStringLiteral("assembly"),
                        QStringLiteral("component")};
  m_registry->add(openDesign);
  // Component from Web… (mitcad#1579, webbrowser/MainWindowComponentWeb.cpp).
  registerComponentWebCommand();
  CommandDef importDef = def("file.import", tr("Import…"), "import",
                             tr("Brings in STEP, IGES, STL, DXF and other CAD files, or opens them as designs"),
                             [this] { importFile(); });
  importDef.keywords = {QStringLiteral("step"),    QStringLiteral("iges"),  QStringLiteral("brep"),
                        QStringLiteral("f3d"),     QStringLiteral("f3z"),   QStringLiteral("freecad"),
                        QStringLiteral("fcstd"),   QStringLiteral("ipt"),   QStringLiteral("iam"),
                        QStringLiteral("ifc"),     QStringLiteral("ifczip"), QStringLiteral("rvt"),
                        QStringLiteral("rfa"),     QStringLiteral("solidcraft"), QStringLiteral("sc"),
                        QStringLiteral("dwg"),     QStringLiteral("3dm"),   QStringLiteral("open")};
  m_registry->add(importDef);
  CommandDef exportDef = def("file.export", tr("Export…"), "export",
                             tr("Bodies to STEP, IGES, STL, OBJ or BRep; a sketch to DXF"),
                             [this] { exportFile(); });
  exportDef.tab.clear(); // the File menu
  exportDef.mode = CommandDef::Mode::Any;
  exportDef.keywords = {QStringLiteral("step"), QStringLiteral("stl"), QStringLiteral("dxf"),
                        QStringLiteral("save as"), QStringLiteral("3mf"), QStringLiteral("usdz"),
                        QStringLiteral("augmented reality")};
  m_registry->add(exportDef);
  // Drawings with views, dimensions and a hole table (mitcad#471,
  // MainWindowDrawing.cpp).
  CommandDef drawingDef = def("file.export_drawing", tr("Export Drawing…"), "export",
                              tr("The bodies as a drawing with views, sizes and a hole table: PDF, SVG, DXF or DWG"),
                              [this] { exportDrawing(); });
  drawingDef.tab.clear(); // the File menu
  drawingDef.mode = CommandDef::Mode::Any;
  drawingDef.keywords = {QStringLiteral("drawing"), QStringLiteral("pdf"), QStringLiteral("svg"),
                         QStringLiteral("dxf"), QStringLiteral("dwg"), QStringLiteral("print"),
                         QStringLiteral("blueprint")};
  m_registry->add(drawingDef);
  // Print… (mitcad#1089, printing/MainWindowPaperPrint.cpp): the drawing,
  // the BUILD lists or the open sketch on paper.
  m_registry->add(paperPrintCommand());
  // A drawing kept in the design (mitcad#707, drawings/MainWindowDrawings.cpp):
  // in the root's Drawings folder, opened at once.
  CommandDef createDrawingDef = def("file.create_drawing", tr("Create Drawing…"), "drawing",
                                    tr("Adds a drawing of the design and opens its sheet"),
                                    [this] { createDrawing(QStringLiteral("C0")); });
  createDrawingDef.tab.clear(); // the File menu
  createDrawingDef.keywords = {QStringLiteral("drawing"), QStringLiteral("sheet"), QStringLiteral("blueprint"),
                               QStringLiteral("2d"), QStringLiteral("views")};
  m_registry->add(createDrawingDef);
  registerDrawingCommands();
  // Send to Slicer (mitcad#13, mitcad#234, MainWindowPrint.cpp; once 3D
  // Print): a panel in the Make tab's 3D Print group (mitcad#1049).
  m_registry->add(print3dCommand());
  // The Printers and the Processes managers (mitcad#655, mitcad#1135):
  // next to Send to Slicer.
  m_registry->add(printersCommand());
  m_registry->add(processesCommand());
  // Plates… (mitcad#1050, MainWindowPlates.cpp): last in the 3D Print
  // group.
  m_registry->add(platesCommand());
  CommandDef recover = def("file.recover", tr("Recover Designs…"), "recover",
                           tr("Restores changes that were not saved when MitCad last ended without closing"),
                           [this] { recoverDocuments(false); });
  recover.tab.clear(); // the File menu
  recover.mode = CommandDef::Mode::Any;
  recover.keywords = {QStringLiteral("autosave"), QStringLiteral("crash"), QStringLiteral("restore"),
                      QStringLiteral("unsaved"), QStringLiteral("backup")};
  m_registry->add(recover);
  // The design's summary (the root's property set Summary) and statistics
  // (mitcad#1088, browser/DesignPropertiesDialog.cpp): in the File menu and
  // the command search; a quick-access group may show it by its id.
  CommandDef designProperties =
      def("file.design_properties", tr("Design Properties…"), "properties",
          tr("Edits the design's summary and shows when and by whom it was saved"),
          [this] { m_controller->openDesignProperties(); });
  designProperties.tab.clear(); // the File menu
  // Its statistics read in any window and mode (mitcad#1149): read-only,
  // during a command or a sketch the summary shows without editing.
  designProperties.mode = CommandDef::Mode::Any;
  designProperties.duringCommands = true;
  designProperties.keywords = {QStringLiteral("summary"),    QStringLiteral("title"),
                               QStringLiteral("author"),     QStringLiteral("subject"),
                               QStringLiteral("keywords"),   QStringLiteral("comments"),
                               QStringLiteral("metadata"),   QStringLiteral("statistics"),
                               QStringLiteral("created"),    QStringLiteral("last saved"),
                               QStringLiteral("title block"), QStringLiteral("file properties"),
                               QStringLiteral("document properties")};
  designProperties.enabled = [this] { return m_controller != nullptr; };
  m_registry->add(designProperties);
}

void MainWindow::createFileMenu() {
  // mitcad#175: the document commands, saving and versions, the Project
  // submenu, import and export, Print (mitcad#1089), recovery, Exit. Send to Slicer and Insert
  // Component are on the ribbon (the Make tab, ASSEMBLE), Render Image too
  // (RENDER) and in View > Visual Style.
  QMenu* file = menuBar()->addMenu(tr("&File"));
  file->setObjectName(QStringLiteral("fileMenu"));
  // Every entry but Open Recent is a registry command (Open, Close, Save,
  // Save As and Exit since mitcad#1090, files/FileCommands.cpp): its id,
  // key, tooltip and search, and the quick-access group can show it.
  const auto command = [this](QMenu* menu, const char* id, const QString& text) {
    QAction* action = m_registry->action(QString::fromLatin1(id));
    action->setText(text);
    menu->addAction(action);
  };
  // New and New Design are one command (Ctrl+N): a design of the current
  // project, named in the Projects panel (mitcad#145), else an untitled
  // one.
  command(file, "file.new_design", tr("&New Design…"));
  command(file, "file.open", tr("&Open…"));
  // A design of a project, its folders browsed or its name searched
  // (mitcad#530).
  command(file, "file.open_from_project", tr("Open &from Project…"));
  m_recentMenu = file->addMenu(tr("Open &Recent"));
  connect(m_recentMenu, &QMenu::aboutToShow, this, &MainWindow::updateRecentMenu);
  updateRecentMenu();
  command(file, "file.open_read_only", tr("Open Rea&d-Only…"));
  command(file, "file.close", tr("&Close"));
  file->addSeparator();
  command(file, "file.save", tr("&Save"));
  command(file, "file.save_as", tr("Save &As…"));
  // Save As into a project, a new one too (mitcad#503; File > Project >
  // Move to a Project before).
  command(file, "file.save_to_project", tr("Save to Pro&ject…"));
  // Versions (P12d): Save records one in a project; Save with Description
  // one with a description written; Version History (P12e) lists them.
  command(file, "file.save_version", tr("Save &with Description…"));
  command(file, "file.version_history", tr("Version &History…"));
  // The design's summary and statistics (mitcad#1088).
  command(file, "file.design_properties", tr("Design Proper&ties…"));
  file->addSeparator();
  // Local and Cloud projects (P12d, P12 remote, mitcad#89): made or opened;
  // the current one's versions shared; its settings.
  QMenu* project = file->addMenu(tr("&Project"));
  project->setObjectName(QStringLiteral("fileProjectMenu"));
  command(project, "file.new_project", tr("&New Project…"));
  command(project, "file.open_project", tr("&Open Project…"));
  command(project, "file.open_remote", tr("Open from C&loud…"));
  project->addSeparator();
  command(project, "file.sync", tr("&Sync"));
  command(project, "file.check_remote", tr("&Check for Newer Versions"));
  command(project, "file.remote_browser", tr("Open in &Browser"));
  project->addSeparator();
  command(project, "file.project_settings", tr("Project Se&ttings…"));
  // The project's web viewer (mitcad#423).
  project->addSeparator();
  command(project, "file.web_viewer", tr("&Web Viewer…"));
  command(project, "file.update_web_viewer", tr("&Update Web Viewer"));
  file->addSeparator();
  command(file, "file.import", tr("&Import…"));
  command(file, "file.export", tr("&Export…"));
  command(file, "file.create_drawing", tr("Create Dra&wing…"));
  command(file, "file.export_drawing", tr("Export &Drawing…"));
  file->addSeparator();
  command(file, "file.print", tr("Prin&t…"));
  file->addSeparator();
  command(file, "file.recover", tr("Recover Desi&gns…"));
#ifndef Q_OS_MACOS
  // macOS moves Quit to the application menu.
  file->addSeparator();
#endif
  // QuitRole: "Quit MitCad" (Cmd+Q) in the application menu on macOS.
  command(file, "file.exit", tr("E&xit"));
  m_registry->action(QStringLiteral("file.exit"))->setMenuRole(QAction::QuitRole);
  // For the UI tests: the entries as the menu opens.
  connect(file, &QMenu::aboutToShow, this, [this, file] {
    qDebug().noquote() << QStringLiteral("File menu: %1").arg(menuEntries(file, m_recentMenu));
  });
}

void MainWindow::addRecentFile(const QString& path) {
  // First in the list, with when it was opened or saved (the Start tab's
  // cards, mitcad#1258).
  recent::noteOpened(path);
}

void MainWindow::updateRecentMenu() {
  if (m_recentMenu == nullptr) {
    return;
  }
  m_recentMenu->clear();
  const QStringList files = QSettings().value(kRecentKey).toStringList();
  QStringList names;
  int previews = 0;
  for (int i = 0; i < files.size(); ++i) {
    const QString path = files[i];
    // A design of a project with the project's name (mitcad#89).
    const QString label = recentLabel(path);
    QAction* action = m_recentMenu->addAction(QStringLiteral("&%1 %2").arg(i + 1).arg(label), this,
                                              [this, path] { openPath(path); });
    action->setMenuRole(QAction::NoRole); // a file name is no About or Settings
    action->setToolTip(QDir::toNativeSeparators(path));
    action->setStatusTip(QDir::toNativeSeparators(path));
    // Its preview, as on the Start tab's card (mitcad#1258).
    const QString preview = recent::designPreviewPath(path);
    if (!preview.isEmpty() && QFileInfo(preview).isFile()) {
      action->setIcon(QIcon(preview));
      ++previews;
    }
    names << QString(label).replace(QChar(0x2014), QLatin1Char('-'));
  }
  if (files.isEmpty()) {
    m_recentMenu->addAction(tr("No recent files"))->setEnabled(false);
  } else {
    m_recentMenu->addSeparator();
    m_recentMenu->addAction(tr("Clear Recent Files"), this, [this] {
      recent::clearRecent(); // the pinned designs stay on the Start tab
      if (startPageShown()) {
        m_startPage->refresh();
      }
    });
  }
  if (!files.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Recent files previews: %1 of %2").arg(previews).arg(files.size());
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
  if (kind != FileKind::Design && !info.isFile()) {
    error = tr("The file does not exist.");
    return false;
  }
  // A .sc file of another program is refused before the import starts.
  if (kind == FileKind::Solidcraft) {
    if (const QString refused = solidcraftRefusal(path); !refused.isEmpty()) {
      error = refused;
      return false;
    }
  }
  switch (kind) {
  case FileKind::Design:
    return loadDesign(path, error);
  case FileKind::F3d:
  case FileKind::FreeCad:
  case FileKind::Ipt:
  case FileKind::Ifc:
  case FileKind::Rvt:
  case FileKind::Solidcraft:
  case FileKind::Dwg:
  case FileKind::ThreeDm:
    // After the window shows: the import has a progress dialog. (Opened by
    // openPath, it asks its choices, mitcad#507.)
    afterEventWhenIdle(*this, this,
                       [this, path, ask = m_askImportChoices] { startF3dImport(path, QJsonObject(), ask); });
    return true;
  case FileKind::Cad:
  case FileKind::Mesh:
  case FileKind::Drawing: {
    if (!startImportedDocument(path, error)) {
      return false;
    }
    addRecentFile(path);
    return true;
  }
  case FileKind::None:
    break;
  }
  error = tr("MitCad cannot open %1 files.").arg(info.suffix());
  return false;
}

void MainWindow::openPath(const QString& path) {
  if (deferForNativeCommand([this, path] { openPath(path); })) {
    return;
  }
  // Replacing this design asks about its changes first (as in openDocument).
  const bool replacing = m_windowManager == nullptr || GeneralSettings::load().openInCurrentWindow;
  if (replacing && !maybeSave()) {
    return;
  }
  QString error;
  m_askImportChoices = true;
  const bool opened = m_windowManager != nullptr ? m_windowManager->openFile(this, path, error, replacing) != nullptr
                                                 : openFile(path, error);
  m_askImportChoices = false;
  if (!opened && !error.isEmpty()) { // empty: cancelled
    const QString message = tr("Could not open %1:\n%2").arg(QDir::toNativeSeparators(path), error);
    qWarning().noquote() << message;
    sheetWarning(this, tr("MitCad"), message);
  }
}

void MainWindow::openExternalFile(const QString& path) {
  if (deferForNativeCommand([this, path] { openExternalFile(path); })) {
    return;
  }
  if (usesNativeUi()) {
    present();
  } else {
    // The top-level window: a document frame's while this main window is
    // one of its pages (mitcad#1172).
    QWidget* top = window();
    if (top->isMinimized()) {
      top->showNormal();
    }
    top->raise();
    activateWindow();
  }
  if (fileKind(path) == FileKind::None) {
    const QString message = tr("MitCad cannot open %1 files.").arg(QFileInfo(path).suffix());
    qWarning().noquote() << QStringLiteral("Could not open %1: %2").arg(path, message);
    sheetWarning(this, tr("MitCad"), message);
    return;
  }
  openPath(path);
}

bool MainWindow::startImportedDocument(const QString& path, QString& error) {
  const QString name = QFileInfo(path).fileName();
  const QString file = QFileInfo(path).absoluteFilePath();
  const FileKind kind = fileKind(path);
  // A STEP assembly's choice, asked when the user opens it (mitcad#1274).
  QString structure;
  bool repeatedParts = false;
  if (kind == FileKind::Cad && m_askImportChoices) {
    const std::optional<QString> chosen = askImportStructure(path, &repeatedParts);
    if (!chosen) {
      error.clear();
      return false;
    }
    structure = *chosen;
  }
  QJsonObject result;
  QString sketch;
  // Import into a document owned by the opening job. The active model,
  // its history, edit lock and recovery stay until the whole import works.
  const DocumentMaker read = [&](ModelJob& job) {
    job.setStage(tr("Importing %1").arg(name));
    rust::Box<Document> document = new_document();
    const AttachedJob attached(*document, job);
    const auto run = [&](const QJsonObject& command) {
      if (job.control().is_cancelled()) { throw ComputationCancelled(); }
      const QJsonObject answer = parseObject(document->command(rustStr(compactJson(command))));
      if (job.control().is_cancelled()) { throw ComputationCancelled(); }
      const QJsonValue failure = answer.value(QStringLiteral("error"));
      if (!failure.isUndefined() && !failure.isNull()) {
        throw std::runtime_error(failure.toString(tr("The imported geometry could not be computed.")).toStdString());
      }
      return answer;
    };
    if (kind == FileKind::Drawing) {
      const QJsonObject created = run(cmd("sketch.create", {{QStringLiteral("plane"), QStringLiteral("xy")}}));
      sketch = created.value(QStringLiteral("uid")).toString();
      result = isSvgDrawing(file) ? run(cmd("sketch.import_svg", {{QStringLiteral("sketch"), sketch},
                                                                 {QStringLiteral("path"), file}}))
                                  : run(cmd("sketch.import_dxf", {{QStringLiteral("sketch"), sketch},
                                                                 {QStringLiteral("path"), file},
                                                                 {QStringLiteral("unit"), QStringLiteral("mm")}}));
      run(cmd("merge_undo", {{QStringLiteral("depth"), 0},
                              {QStringLiteral("label"), tr("Insert %1").arg(name)}}));
    } else {
      QJsonObject command = cmd("import_file", {{QStringLiteral("path"), file}});
      if (kind == FileKind::Mesh) {
        command.insert(QStringLiteral("unit_mm"), 1.0);
      }
      if (!structure.isEmpty()) {
        command.insert(QStringLiteral("structure"), structure);
      }
      // Find Repeated Parts in the import's undo step (mitcad#1277).
      if (repeatedParts) {
        command.insert(QStringLiteral("repeated_parts"), true);
      }
      // A file opened is the design itself, not a component in it
      // (mitcad#1629; the new document is empty, so this is the default
      // too).
      command.insert(QStringLiteral("wrap"), false);
      result = run(command);
    }
    return document;
  };
  // With its root active (mitcad#426).
  m_openingImport = true;
  const bool installed = installDocument(name, QString(), read, error);
  m_openingImport = false;
  if (!installed) {
    return false;
  }
  m_documentName = QFileInfo(path).completeBaseName();
  updateWindowTitle();
  m_autosave->saveSoon();
  if (kind == FileKind::Drawing) {
    const QJsonObject model = queryObject({{QStringLiteral("query"), QStringLiteral("sketch")},
                                           {QStringLiteral("uid"), sketch}});
    qInfo().noquote() << QStringLiteral("Inserted %1 into %2: %3 curves, %4 points, %5 texts, %6 profiles, "
                                        "%7 warnings")
                             .arg(name, model.value(QStringLiteral("name")).toString())
                             .arg(result.value(QStringLiteral("curves")).toInt())
                             .arg(result.value(QStringLiteral("points")).toInt())
                             .arg(result.value(QStringLiteral("text_count")).toInt())
                             .arg(model.value(QStringLiteral("regions")).toArray().size())
                             .arg(result.value(QStringLiteral("warnings")).toArray().size());
    showHint(tr("Inserted %1. Extrude (E) a profile of it.").arg(name));
  } else {
    qInfo().noquote() << importedLine(name, result);
    showImported(*this, name, result);
  }
  return true;
}

void MainWindow::closeDocument() {
  if (m_windowManager != nullptr && m_windowManager->windows().size() > 1) {
    m_windowManager->closeWindow(this);
    return;
  }
  if (deferForNativeCommand([this] { closeDocument(); })) {
    return;
  }
#ifdef Q_OS_MACOS
  // The native menu receives Cmd+W even while a non-modal tool window is key.
  // window(): a design in a document frame (the shared Qt UI) is not a
  // window of its own; its frame is.
  if (QWidget* active = QApplication::activeWindow(); active != nullptr && active != window() && !active->isModal()) {
    active->close();
    return;
  }
#endif
  if (!maybeSave()) {
    return;
  }
  if (!installEmptyDocument(QString())) {
    return;
  }
  qInfo() << "Document closed";
  showHint(tr("Create a sketch to start modeling. S searches for commands."));
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
  openDropped(paths);
}

void MainWindow::openDropped(const QStringList& paths) {
  // Later, so that the drag ends first; a design opens, the rest come in
  // (from the Start page too, mitcad#1258, which the import leaves).
  afterEventWhenIdle(*this, this, [this, paths] {
    for (const QString& path : paths) {
      if (fileKind(path) == FileKind::Design) {
        openPath(path);
      } else if (m_mode == Mode::Idle || fileKind(path) == FileKind::Drawing) {
        leaveStartPage();
        importPath(path, true);
      }
    }
  });
}

// ---------------------------------------------------------------------------
// Import

void MainWindow::importFile() {
  const QString path = dialogOpenFileName(this, tr("Import"), dialogFolder(DialogGroup::Import), importFilter());
  if (!path.isEmpty()) {
    rememberDialogFolder(DialogGroup::Import, path);
    // What comes in shows in the 3D view (from the Start tab's Import
    // File…, mitcad#1258).
    leaveStartPage();
    importPath(path, true);
  }
}

bool MainWindow::importPath(const QString& path, bool interactive, QString* error) {
  QString reason;
  bool ok = true;
  switch (fileKind(path)) {
  case FileKind::F3d:
  case FileKind::FreeCad:
  case FileKind::Ipt:
  case FileKind::Ifc:
  case FileKind::Rvt:
  case FileKind::Solidcraft:
  case FileKind::Dwg:
  case FileKind::ThreeDm:
    // A .sc file of another program is refused before the import starts
    // (mitcad#1622).
    if (fileKind(path) == FileKind::Solidcraft) {
      reason = solidcraftRefusal(path);
      if (!reason.isEmpty()) {
        ok = false;
        break;
      }
    }
    // A design with its history is a document of its own (a FreeCAD
    // document too, and a part file, whose history comes later).
    if (interactive && !maybeSave()) {
      return false;
    }
    startF3dImport(path, QJsonObject(), interactive);
    return true;
  case FileKind::Design:
    insertComponent(path);
    return true;
  case FileKind::Cad: {
    QString structure;
    bool repeatedParts = false;
    std::optional<bool> wrap;
    if (interactive) {
      bool wrapByDefault = false;
      const std::optional<QString> chosen = askImportStructure(path, &repeatedParts, &wrapByDefault);
      if (!chosen) {
        return false;
      }
      structure = *chosen;
      // Into a design with content: a new component named after the file,
      // or the active component (mitcad#1629).
      if (wrapByDefault) {
        wrap = askImportPlace(this, path);
        if (!wrap) {
          return false;
        }
      }
    }
    ok = importCad(path, 1.0, &reason, nullptr, structure, repeatedParts, wrap);
    break;
  }
  case FileKind::Mesh: {
    MeshInsertChoice choice;
    if (interactive) {
      const std::optional<MeshInsertChoice> chosen = askMeshInsert(this, path);
      if (!chosen) {
        return false;
      }
      choice = *chosen;
    }
    const int depth = undoDepth();
    QJsonObject imported;
    ok = importCad(path, choice.unitMillimetres, &reason, &imported);
    if (ok && choice.convert) {
      convertInsertedMeshes(path, imported, choice, depth);
    }
    break;
  }
  case FileKind::Drawing:
    ok = importDrawing(path, interactive, &reason);
    break;
  case FileKind::None:
    reason = tr("MitCad cannot import %1 files.").arg(QFileInfo(path).suffix());
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

std::optional<QString> MainWindow::askImportStructure(const QString& path, bool* repeatedParts,
                                                      bool* wrapByDefault) {
  bool assembly = false;
  // Every file's answer says whether import_file wraps it in a component of
  // its own (mitcad#1629); only a STEP file's is read for an assembly.
  try {
    const QJsonObject answer = queryObject({{QStringLiteral("query"), QStringLiteral("file_structure")},
                                            {QStringLiteral("path"), QFileInfo(path).absoluteFilePath()}});
    assembly = answer.value(QStringLiteral("assembly")).toBool();
    if (wrapByDefault != nullptr) {
      *wrapByDefault = answer.value(QStringLiteral("wrap")).toBool();
    }
  } catch (const std::exception& e) {
    // The import says what is wrong with the file.
    qDebug().noquote() << QStringLiteral("file_structure of %1: %2").arg(path, errorText(e));
  }
  if (!assembly) {
    return QString();
  }
  return askStepStructure(this, path, repeatedParts);
}

bool MainWindow::importCad(const QString& path, double unitMillimetres, QString* error, QJsonObject* imported,
                           const QString& structure, bool repeatedParts, std::optional<bool> wrap) {
  if (!canChangeModel()) {
    *error = changeBlockedReason();
    return false;
  }
  QJsonObject command = cmd("import_file", {{QStringLiteral("path"), QFileInfo(path).absoluteFilePath()}});
  if (fileKind(path) == FileKind::Mesh) {
    command.insert(QStringLiteral("unit_mm"), unitMillimetres);
  }
  if (!structure.isEmpty()) {
    command.insert(QStringLiteral("structure"), structure);
  }
  // Find Repeated Parts in the import's undo step (mitcad#1277).
  if (repeatedParts) {
    command.insert(QStringLiteral("repeated_parts"), true);
  }
  // Into a new component or the active one, as asked (mitcad#1629); the
  // model's default when nothing was asked.
  if (wrap) {
    command.insert(QStringLiteral("wrap"), *wrap);
  }
  QJsonObject result;
  if (!runModelCommand(command, &result)) {
    *error = m_lastError;
    return false;
  }
  if (imported != nullptr) {
    *imported = result;
  }
  qInfo().noquote() << importedLine(QFileInfo(path).fileName(), result);
  showImported(*this, QFileInfo(path).fileName(), result);
  m_viewer->fitAll();
  return true;
}

void MainWindow::convertInsertedMeshes(const QString& path, const QJsonObject& imported,
                                       const MeshInsertChoice& choice, int depth) {
  const QString file = QFileInfo(path).fileName();
  int converted = 0;
  QStringList refused;
  for (const QJsonValue& value : imported.value(QStringLiteral("bodies")).toArray()) {
    const QJsonObject body = value.toObject();
    if (body.value(QStringLiteral("kind")).toString() != QStringLiteral("mesh")) {
      continue;
    }
    const QString uid = body.value(QStringLiteral("uid")).toString();
    const QString name = body.value(QStringLiteral("name")).toString();
    const QJsonObject def{{QStringLiteral("type"), QStringLiteral("mesh_convert")},
                          {QStringLiteral("body"), uid},
                          {QStringLiteral("tolerance"), QStringLiteral("%1 mm").arg(choice.tolerance)},
                          {QStringLiteral("mode"), QStringLiteral("faceted")},
                          {QStringLiteral("angle"), QStringLiteral("%1 deg").arg(choice.angleDegrees)}};
    QJsonObject command = cmd("add_feature");
    command.insert(QStringLiteral("def"), def);
    QJsonObject result;
    if (runModelCommand(command, &result)) {
      ++converted;
      qInfo().noquote() << QStringLiteral("Insert Mesh converted %1 (%2) to a solid by %3")
                               .arg(name, uid, result.value(QStringLiteral("name")).toString());
    } else {
      refused << QStringLiteral("%1: %2").arg(name, m_lastError);
      qWarning().noquote() << QStringLiteral("Insert Mesh did not convert %1 (%2): %3").arg(name, uid, m_lastError);
    }
  }
  if (converted > 0 && undoDepth() > depth + 1) {
    runCommand(cmd("merge_undo", {{QStringLiteral("depth"), depth}, {QStringLiteral("label"), tr("Insert %1").arg(file)}}));
  }
  if (!refused.isEmpty()) {
    // The mesh body stays, shown; the model refused the conversion and
    // changed nothing.
    showWarning(tr("Inserted %1 as a mesh body; not converted to a solid: %2")
                    .arg(file, refused.join(QStringLiteral("; "))));
  } else if (converted > 0) {
    showHint(tr("Inserted %1 as a solid; the mesh body is kept, hidden.").arg(file));
  }
}

bool MainWindow::importDrawing(const QString& path, bool interactive, QString* error) {
  const QString file = QFileInfo(path).absoluteFilePath();
  // An SVG drawing (mitcad#473) or a DXF one.
  const bool svg = isSvgDrawing(path);
  const auto report = [this, &path, svg](const QJsonObject& result, const QString& sketch) {
    const QJsonObject model = queryObject({{QStringLiteral("query"), QStringLiteral("sketch")},
                                           {QStringLiteral("uid"), sketch}});
    qInfo().noquote() << QStringLiteral("Inserted %1 into %2: %3 curves, %4 points, %5 texts, %6 profiles, "
                                        "%7 warnings%8")
                             .arg(QFileInfo(path).fileName(), model.value(QStringLiteral("name")).toString())
                             .arg(result.value(QStringLiteral("curves")).toInt())
                             .arg(result.value(QStringLiteral("points")).toInt())
                             .arg(result.value(QStringLiteral("text_count")).toInt())
                             .arg(model.value(QStringLiteral("regions")).toArray().size())
                             .arg(result.value(QStringLiteral("warnings")).toArray().size())
                             .arg(svg ? QStringLiteral(", %1 filled, %2 bands")
                                            .arg(result.value(QStringLiteral("filled")).toArray().size())
                                            .arg(result.value(QStringLiteral("bands")).toInt())
                                      : QString());
  };
  // The drawing's unit, layers and extent for the dialog (P9), or an SVG
  // drawing's size and contents.
  QJsonObject info;
  if (interactive) {
    try {
      info = queryObject({{QStringLiteral("query"), svg ? QStringLiteral("svg_info") : QStringLiteral("dxf_info")},
                          {QStringLiteral("path"), file}});
    } catch (const std::exception& e) {
      *error = errorText(e);
      return false;
    }
  }
  const auto ask = [this, &path, &info, svg](const QVector<QPair<QString, QString>>& planes) {
    return svg ? askSvgInsert(this, path, planes, info) : askDxfInsert(this, path, planes, info);
  };
  // Where it goes and which layers.
  const auto importCommand = [&file, svg](const QString& sketch, const DxfChoice& choice) {
    if (svg) {
      QJsonObject command = cmd("sketch.import_svg", {{QStringLiteral("sketch"), sketch},
                                                      {QStringLiteral("path"), file},
                                                      {QStringLiteral("scale"), choice.scale},
                                                      {QStringLiteral("stroke_outline"), choice.strokeOutline}});
      if (choice.x != 0.0 || choice.y != 0.0) {
        command.insert(QStringLiteral("at"), QJsonArray{choice.x, choice.y});
      }
      return command;
    }
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
      const std::optional<DxfChoice> chosen = ask({});
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
    *error = changeBlockedReason();
    return false;
  }
  // A new sketch on a plane: an origin plane, or the selected plane or
  // planar face.
  QVector<QPair<QString, QString>> planes;
  QJsonValue selectedPlane;
  QString selectedOccurrence; // where it was picked (mitcad#100)
  if (m_selection.size() == 1 && (m_selection.first().kind == SelectKind::Plane ||
                                  (m_selection.first().kind == SelectKind::Face &&
                                   m_selection.first().geometry == QStringLiteral("plane")))) {
    const SelectionItem& item = m_selection.first();
    selectedPlane = item.kind == SelectKind::Plane
                        ? QJsonValue(item.owner)
                        : QJsonValue(QJsonObject{{QStringLiteral("face"), item.name},
                                                 {QStringLiteral("body"), item.owner}});
    selectedOccurrence = item.occurrence;
    planes.append({QStringLiteral("selected"), tr("Selected: %1").arg(pickListText(item, m_snapshot))});
  }
  planes.append({QStringLiteral("xy"), tr("XY plane")});
  planes.append({QStringLiteral("xz"), tr("XZ plane")});
  planes.append({QStringLiteral("yz"), tr("YZ plane")});
  DxfChoice choice;
  choice.plane = planes.first().first;
  choice.unit = QStringLiteral("mm");
  if (interactive) {
    const std::optional<DxfChoice> chosen = ask(planes);
    if (!chosen) {
      *error = QString();
      return false;
    }
    choice = *chosen;
  }
  const bool selected = choice.plane == QStringLiteral("selected");
  const QJsonValue plane = selected ? selectedPlane : QJsonValue(choice.plane);
  QJsonObject fields{{QStringLiteral("plane"), plane}};
  const bool originPlane = selectedPlane.isString() &&
                           QStringList{QStringLiteral("xy"), QStringLiteral("xz"), QStringLiteral("yz")}.contains(
                               selectedPlane.toString());
  if (selected && !originPlane) {
    fields.insert(QStringLiteral("occurrence"), selectedOccurrence);
  }
  const int depth = undoDepth();
  QJsonObject created;
  if (!runCommand(cmd("sketch.create", fields), &created)) {
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
  const QJsonArray filled = result.value(QStringLiteral("filled")).toArray();
  if (svg && !filled.isEmpty()) {
    // The regions the drawing fills, selected for Extrude (mitcad#473).
    Selection profiles;
    for (const QJsonValue& key : filled) {
      profiles.append({SelectKind::Profile, sketch, key.toString(), QStringLiteral("plane")});
    }
    setSelection(profiles);
    showHint(tr("Inserted %1: its filled regions are selected. Extrude (E) them.").arg(QFileInfo(path).fileName()));
  } else {
    showHint(tr("Inserted %1. Extrude (E) a profile of it.").arg(QFileInfo(path).fileName()));
  }
  m_viewer->fitAll();
  return true;
}

void MainWindow::insertSvg() {
  const QString path = dialogOpenFileName(this, tr("Insert SVG"), dialogFolder(DialogGroup::Import),
                                          tr("SVG drawings (*.svg);;All files (*)"));
  if (!path.isEmpty()) {
    rememberDialogFolder(DialogGroup::Import, path);
    importPath(path, true);
  }
}

void MainWindow::insertMesh() {
  const QString path = dialogOpenFileName(this, tr("Insert Mesh"), dialogFolder(DialogGroup::Import),
                                          tr("Meshes (*.stl *.obj);;All files (*)"));
  if (!path.isEmpty()) {
    rememberDialogFolder(DialogGroup::Import, path);
    importPath(path, true);
  }
}

void MainWindow::insertDxf() {
  const QString path = dialogOpenFileName(this, tr("Insert DXF"), dialogFolder(DialogGroup::Import),
                                          tr("DXF drawings (*.dxf);;All files (*)"));
  if (!path.isEmpty()) {
    rememberDialogFolder(DialogGroup::Import, path);
    importPath(path, true);
  }
}

void MainWindow::insertComponent(const QString& given) {
  if (!canChangeModel()) {
    return;
  }
  QString path = given;
  if (path.isEmpty() && m_project.hasHistory()) {
    // In a project: a design of the listed projects, the current one first
    // (mitcad#176); Another File... the file dialog.
    ProjectPickerOptions options;
    options.mode = ProjectPickerOptions::Mode::Design;
    options.title = tr("Insert from a Project");
    options.text = tr("The design to insert as a component:");
    options.acceptText = tr("&Insert");
    options.otherText = tr("&Another File…");
    options.root = m_project.root;
    options.ownFile = m_filePath;
    const std::optional<ProjectPickerResult> picked = pickInProjects(this, options);
    if (!picked) {
      return;
    }
    if (!picked->other) {
      path = picked->path;
    }
  }
  if (path.isEmpty()) {
    path = dialogOpenFileName(this, tr("Insert Component"), dialogFolder(DialogGroup::Import),
                              tr("MitCad designs (*.mitcad)"));
    if (path.isEmpty()) {
      return;
    }
    rememberDialogFolder(DialogGroup::Import, path);
  }
  const QString absolute = QFileInfo(path).absoluteFilePath();
  const QString base = m_filePath.isEmpty() ? QString() : QFileInfo(m_filePath).absolutePath();
  // A link to a file outside the design's project would not be followed
  // when the design opens again (mitcad#929): said before inserting, with
  // the copy into the project offered (mitcad#1719). A design not saved yet
  // has no project to tell; it is asked as before.
  std::optional<bool> linked;
  bool copyIntoProject = false;
  QString copyFolder;
  if (!base.isEmpty()) {
    QJsonObject check;
    try {
      check = queryObject({{QStringLiteral("query"), QStringLiteral("link_check")},
                           {QStringLiteral("path"), absolute},
                           {QStringLiteral("base"), base}});
    } catch (const std::exception& e) {
      qWarning().noquote() << QStringLiteral("Insert Component: the link not checked: %1").arg(errorText(e));
    }
    if (check.value(QStringLiteral("followed")) == QJsonValue(false)) {
      copyFolder = check.value(QStringLiteral("folder")).toString();
      const QString root = QFileInfo(copyFolder).absolutePath();
      const bool inProject = QFileInfo::exists(QDir(root).filePath(QStringLiteral(".mitcad/project.json")));
      const std::optional<OutsideLinkChoice> choice = askOutsideLink(this, path, inProject);
      if (!choice) {
        return;
      }
      copyIntoProject = *choice == OutsideLinkChoice::CopyIntoProject;
      linked = copyIntoProject;
    }
  }
  if (!linked) {
    // Not asked yet: a file in the project, or a design not saved.
    linked = askLinked(this, path);
    if (!linked) {
      return;
    }
  }
  QJsonObject command = cmd("insert_component", {{QStringLiteral("path"), absolute}, {QStringLiteral("link"), *linked}});
  if (!base.isEmpty()) {
    command.insert(QStringLiteral("base"), base);
  }
  if (copyIntoProject) {
    command.insert(QStringLiteral("copy_into_project"), true);
  }
  QJsonObject result;
  if (runModelCommand(command, &result)) {
    for (const QJsonValue& value : result.value(QStringLiteral("copied")).toArray()) {
      const QJsonObject copied = value.toObject();
      const QString to = copied.value(QStringLiteral("to")).toString();
      qInfo().noquote() << QStringLiteral("Insert Component: copied %1 into the project as %2")
                               .arg(QFileInfo(copied.value(QStringLiteral("from")).toString()).fileName(),
                                    QDir(QFileInfo(copyFolder).absolutePath()).relativeFilePath(to));
    }
    qInfo().noquote() << QStringLiteral("Inserted component %1 (%2) from %3")
                             .arg(result.value(QStringLiteral("name")).toString(),
                                  *linked ? QStringLiteral("linked") : QStringLiteral("copy"),
                                  QFileInfo(path).fileName());
    if (copyIntoProject) {
      showStatus(tr("%1 is copied into the project and linked; changes to the copy come in with Update Link.")
                     .arg(QFileInfo(path).fileName()),
                 NoticeLevel::Information);
    }
    m_viewer->fitAll();
  }
}

void MainWindow::startF3dImport(const QString& path, const QJsonObject& rerun, bool ask) {
  if (m_import) {
    showError(tr("An import is running."));
    return;
  }
  // A new import of a design: nothing decided about its deviating results
  // yet (mitcad#379).
  if (rerun.isEmpty()) {
    m_deviationsPath = path;
    m_deviationsDeclined.clear();
    m_deviationsRerunFrom = -1;
  }
  // The import runs to its end, or until its dialog stops it (T1e; the
  // time limit that Preferences once had, import/timeLimit, is not read).
  m_import = new F3dImport(path, this);
  if (!rerun.isEmpty()) {
    m_import->setOptions(rerun);
  }
  m_import->setAskChoices(ask);
  connect(m_import, &F3dImport::finished, this,
          [this, path](bool ok, const QByteArray& project, const QJsonObject& result, const QString& error) {
            const QString name = QFileInfo(path).fileName();
            // (The import's object goes once this returns: no import runs
            // from now on, and one may start again, mitcad#379.)
            const QJsonObject options = m_import ? m_import->options() : QJsonObject();
            // (Its folder lasts while the import's object does: until this
            // returns.)
            const QString handedOver = m_import ? m_import->handedOverResults() : QString();
            m_import.clear();
            if (!ok) {
              if (error.isEmpty()) {
                showHint(tr("Import of %1 canceled.").arg(name));
                return;
              }
              const QString message = tr("Could not import %1:\n%2").arg(QDir::toNativeSeparators(path), error);
              qWarning().noquote() << QStringLiteral("Import of %1 failed: %2").arg(name, error);
              // An error, shown once: its dialog (mitcad#1040); the status
              // line keeps it in one line for the log.
              qDebug().noquote() << QStringLiteral("Status error: %1").arg(message.simplified());
              if (m_reports != nullptr) {
                m_reports->errorShown(message); // a crash in the kernel (mitcad#62)
              }
              sheetCritical(this, tr("Import"), message);
              return;
            }
            // The process computed the design; this one reads it, with
            // progress and cancel, and takes the results the process handed
            // over rather than evaluating the features again (mitcad#413).
            QString reason;
            const DocumentMaker read = [&](ModelJob& job) {
              job.setStage(tr("Reading the imported %1").arg(name));
              return load_document(rustStr(project));
            };
            // With its root active (mitcad#426).
            m_openingImport = true;
            const bool installed = installDocument(name, QString(), read, reason, handedOver);
            m_openingImport = false;
            if (!installed) {
              if (reason.isEmpty()) {
                qInfo().noquote() << QStringLiteral("Opening the imported %1 cancelled").arg(name);
                showHint(tr("Import of %1 canceled.").arg(name));
              } else {
                sheetWarning(this, tr("Import"), tr("Could not read the imported design:\n%1").arg(reason));
              }
              return;
            }
            m_documentName = QFileInfo(path).completeBaseName();
            // Not saved anywhere yet: a design read is modified until it
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
            // The components the import made, with their bodies (an .rvt
            // project's levels, mitcad#274).
            const QJsonArray components = queryObject({{QStringLiteral("query"), QStringLiteral("components")}})
                                              .value(QStringLiteral("components"))
                                              .toArray();
            if (components.size() > 1) {
              QStringList made;
              for (const QJsonValue& value : components) {
                const QJsonObject c = value.toObject();
                made << QStringLiteral("%1 (%2 bodies)")
                            .arg(c.value(QStringLiteral("name")).toString())
                            .arg(c.value(QStringLiteral("bodies")).isArray()
                                     ? static_cast<int>(c.value(QStringLiteral("bodies")).toArray().size())
                                     : c.value(QStringLiteral("bodies")).toInt());
              }
              qInfo().noquote() << QStringLiteral("Imported %1: components %2")
                                       .arg(name, made.join(QStringLiteral(", ")));
            }
            // An assembly's components (mitcad#507): each one's features
            // (a part's replayed timeline, or its stored bodies' base
            // feature) and bodies, with MITCAD_LOG_VOLUMES their volume.
            if (QFileInfo(path).suffix().compare(QStringLiteral("iam"), Qt::CaseInsensitive) == 0) {
              const QJsonObject volumes = bodyVolumes(*this);
              for (qsizetype i = 1; i < components.size(); ++i) {
                const QJsonObject c = components.at(i).toObject();
                const QJsonArray uids = c.value(QStringLiteral("bodies")).toArray();
                double volume = 0.0;
                for (const QJsonValue& uid : uids) {
                  volume += volumes.value(uid.toString()).toObject().value(QStringLiteral("volume")).toDouble();
                }
                qInfo().noquote() << QStringLiteral("Imported %1: component %2: %3 features, %4 bodies%5")
                                         .arg(name, c.value(QStringLiteral("name")).toString())
                                         .arg(c.value(QStringLiteral("features")).toArray().size())
                                         .arg(uids.size())
                                         .arg(volumes.isEmpty() ? QString()
                                                                : QStringLiteral(", volume %1 mm3").arg(volume, 0, 'f', 6));
              }
            }
            // The documents the design refers to (mitcad#196).
            const QJsonArray references = result.value(QStringLiteral("report"))
                                              .toObject()
                                              .value(QStringLiteral("designs"))
                                              .toArray()
                                              .first()
                                              .toObject()
                                              .value(QStringLiteral("references"))
                                              .toArray();
            if (!references.isEmpty()) {
              QStringList statuses;
              for (const QJsonValue& reference : references) {
                const QJsonObject r = reference.toObject();
                statuses << QStringLiteral("%1 %2").arg(r.value(QStringLiteral("name")).toString(),
                                                        r.value(QStringLiteral("status")).toString());
              }
              qInfo().noquote() << QStringLiteral("Imported %1: documents referred to: %2")
                                       .arg(name, statuses.join(QStringLiteral(", ")));
            }
            // Its warnings as every import's (mitcad#1040): the outcomes
            // that left something out, with their count; the details in
            // the report.
            QStringList warned = resultWarnings(result);
            if (!stop.isEmpty()) {
              warned.prepend(tr("stopped early, the rest came in as the file's bodies"));
            }
            showDone(*this, tr("Imported %1.").arg(name), warned, QStringLiteral("Import of %1").arg(name));
            showImportReport(this, name, result, bodies);
            // Items whose results deviate from the file's (mitcad#379):
            // asked once the report is closed, with the design in view.
            if (stop.isEmpty()) {
              afterEventWhenIdle(*this, this,
                                 [this, path, result, options] { offerDeviations(path, result, options); });
            }
          });
  m_import->start();
}

void MainWindow::offerDeviations(const QString& path, const QJsonObject& result, QJsonObject options) {
  if (m_import || path != m_deviationsPath) {
    return;
  }
  const QString name = QFileInfo(path).fileName();
  // The items not decided yet, and those replayed on a result kept since.
  std::vector<DeviationRow> rows;
  for (const DeviationRow& row : deviationRows(result)) {
    const bool replayed = m_deviationsRerunFrom >= 0 && row.index > m_deviationsRerunFrom;
    if (!m_deviationsDeclined.contains(row.index) || replayed) {
      rows.push_back(row);
    }
  }
  if (rows.empty()) {
    return;
  }
  const auto highlight = [this](const DeviationRow* row) {
    Selection faces;
    if (row != nullptr) {
      for (const QJsonValue& value : row->faces) {
        const QJsonObject face = value.toObject();
        const QString body = face.value(QStringLiteral("body")).toString();
        SelectionItem item;
        item.kind = SelectKind::Face;
        item.owner = body;
        item.name = face.value(QStringLiteral("face")).toString();
        item.occurrence = placingOccurrence(body.section(QLatin1Char('.'), 0, 0));
        faces.append(item);
      }
    }
    setSelection(faces);
  };
  const QSet<qint64> keep = askDeviations(this, name, rows, highlight);
  for (const DeviationRow& row : rows) {
    if (!keep.contains(row.index)) {
      m_deviationsDeclined.insert(row.index, true);
    }
  }
  if (keep.isEmpty()) {
    return;
  }
  // The items kept so far and those kept now: the import runs again with
  // them, the items after the first kept now replayed on its result.
  QJsonArray accepted = options.value(QStringLiteral("accept_deviations")).toArray();
  qint64 first = -1;
  for (const qint64 index : keep) {
    accepted.append(static_cast<double>(index));
    first = first < 0 ? index : std::min(first, index);
  }
  options.insert(QStringLiteral("accept_deviations"), accepted);
  m_deviationsRerunFrom = first;
  qInfo().noquote() << QStringLiteral("Import of %1 runs again keeping %2 items parametric").arg(name).arg(accepted.size());
  startF3dImport(path, options);
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
  // Named as Save As Mesh names its file (mitcad#1164): the selected body,
  // the selected component, else the design; askExport adds the extension
  // of the type it starts with (the last export's, or DXF for a sketch).
  const QString suggested = QDir(dialogFolder(DialogGroup::Export)).filePath(safeFileName(exportName(m_selection)));
  // The design's print plates a 3MF file can carry (mitcad#1679), and the
  // slicer Send to Slicer would write them for: the design's.
  ExportPlates plates;
  if (printPlateCount() > 0) {
    try {
      const QJsonObject answer = queryObject({{QStringLiteral("query"), QStringLiteral("print_plates")}});
      for (const QJsonValue& value : answer.value(QStringLiteral("plates")).toArray()) {
        const QJsonObject plate = value.toObject();
        plates.plates.append(
            {plate.value(QStringLiteral("id")).toString(), plate.value(QStringLiteral("name")).toString()});
      }
    } catch (const std::exception& e) {
      qWarning().noquote()
          << QStringLiteral("Export: the print plates could not be read: %1").arg(QString::fromUtf8(e.what()));
    }
    if (!plates.plates.isEmpty()) {
      plates.dialect = designDialect();
    }
  }
  const std::optional<ExportChoice> choice = askExport(this, suggested, static_cast<int>(selected.size()), sketches,
                                                       sketch, !m_snapshot.occurrences.isEmpty(), lengthUnit(), plates);
  if (!choice) {
    return;
  }
  rememberDialogFolder(DialogGroup::Export, choice->path);
  if (choice->format == QStringLiteral("html")) {
    // A web page (mitcad#423, web/WebPage.hpp): the visible bodies meshed
    // within the refinement's tolerances (as STL and OBJ), the viewer and
    // the design in one file; its size in the log and the status bar.
    const io::Refinement refinement = choice->refinement == QLatin1String("low")    ? io::Refinement::Low
                                      : choice->refinement == QLatin1String("high") ? io::Refinement::High
                                                                                    : io::Refinement::Medium;
    const io::MeshOptions tolerance = io::MeshOptions::from(refinement);
    web::PageOptions options;
    options.linearDeflection = tolerance.linear_deflection;
    options.angularDeflection = tolerance.angular_deflection;
    options.title = base;
    options.designFolder = documentFolder();
    web::PageResult page;
    {
      const WaitCursor wait(QStringLiteral("Export web page"));
      page = web::writePage(model(), choice->path, options);
    }
    if (!page.ok) {
      qWarning().noquote() << QStringLiteral("Export of %1 failed: %2").arg(choice->path, page.error);
      sheetCritical(this, tr("Export"), tr("Could not export:\n%1").arg(page.error));
      return;
    }
    qInfo().noquote() << QStringLiteral("Exported %1: html, %2 body(ies), %3 bytes")
                             .arg(choice->path)
                             .arg(page.bodies)
                             .arg(page.bytes);
    // Its warnings as every export's (mitcad#1040).
    showDone(*this,
             tr("Exported %1 (%2 body(ies), %3).")
                 .arg(QDir::toNativeSeparators(choice->path))
                 .arg(page.bodies)
                 .arg(QLocale().formattedDataSize(page.bytes)),
             page.warnings, QStringLiteral("Export"));
    return;
  }
  QJsonObject command;
  if (choice->format == QStringLiteral("dxf")) {
    command = cmd("export_sketch", {{QStringLiteral("sketch"), choice->sketch},
                                    {QStringLiteral("path"), choice->path},
                                    {QStringLiteral("units"), choice->dxfUnit},
                                    {QStringLiteral("construction"), choice->dxfConstruction}});
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
        choice->format == QStringLiteral("3mf") || choice->format == QStringLiteral("usdz")) {
      command.insert(QStringLiteral("refinement"), choice->refinement);
    }
    if (choice->format == QStringLiteral("usdz") && !m_filePath.isEmpty()) {
      // The bodies' appearances with their textures (mitcad#123), whose
      // paths are relative to the design's folder.
      command.insert(QStringLiteral("design_folder"), QFileInfo(m_filePath).absolutePath());
    }
    if (choice->format == QStringLiteral("3mf")) {
      // The parts in the colours the view shows (mitcad#13).
      command.insert(QStringLiteral("colors"), appearanceColors(choice->selectedOnly ? selected : QStringList()));
      // The design's print plates (mitcad#1679), as Send to Slicer writes
      // them (mitcad#1050): on the bed, each plate's pieces on their faces.
      if (choice->plates == QStringLiteral("plate")) {
        command.insert(QStringLiteral("plate"), choice->plate);
      } else if (!choice->plates.isEmpty()) {
        command.insert(QStringLiteral("plates"), choice->plates);
      } else if (choice->onBed) {
        command.insert(QStringLiteral("on_bed"), true);
      }
      if (!choice->dialect.isEmpty()) {
        command.insert(QStringLiteral("dialect"), choice->dialect);
      }
      // Center on Plate with the bed or a slicer (mitcad#1596), Drop to
      // Bed with the bed only (mitcad#1109).
      if (choice->onBed || !choice->dialect.isEmpty()) {
        command.insert(QStringLiteral("center"), choice->center);
      }
      if (choice->onBed) {
        command.insert(QStringLiteral("drop"), choice->drop);
      }
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
    sheetCritical(this, tr("Export"), tr("Could not export:\n%1").arg(m_lastError));
    return;
  }
  QString what = choice->format == QStringLiteral("dxf")
                     ? tr("%1 entities").arg(result.value(QStringLiteral("entities")).toInt())
                     : tr("%1 body(ies)").arg(result.value(QStringLiteral("bodies")).toArray().size());
  if (choice->format == QStringLiteral("step") || choice->format == QStringLiteral("iges")) {
    what += QStringLiteral(", ") + choice->unit;
  }
  if (choice->format == QStringLiteral("dxf")) {
    // The unit written (mitcad#1408).
    what += tr(", in %1").arg(result.value(QStringLiteral("units")).toString(choice->dxfUnit));
    // Whether construction geometry was written (mitcad#1445).
    what += result.value(QStringLiteral("construction")).toBool(choice->dxfConstruction)
                ? tr(", construction geometry included")
                : tr(", construction geometry left out");
  }
  if (choice->format != QStringLiteral("dxf") && !choice->coordinates.isEmpty()) {
    what += QStringLiteral(", %1 coordinates").arg(choice->coordinates);
  }
  if (choice->format == QStringLiteral("usdz")) {
    // The materials, and textures written in their base colour.
    what += tr(", %1 material(s)").arg(result.value(QStringLiteral("materials")).toArray().size());
  }
  // The print plates written (mitcad#1679): a file per plate names the
  // folder of its files.
  QString shown = choice->path;
  if (!choice->plates.isEmpty()) {
    QStringList files;
    for (const QJsonValue& value : result.value(QStringLiteral("files")).toArray()) {
      files << value.toString();
    }
    const qsizetype plateCount = result.value(QStringLiteral("plates")).toArray().size();
    what = files.size() > 1 ? tr("%1 plate(s) in %2 files").arg(plateCount).arg(files.size())
                            : tr("%1 plate(s)").arg(plateCount);
    if (choice->plates == QStringLiteral("file_per_plate")) {
      shown = QFileInfo(choice->path).absolutePath();
    }
    qInfo().noquote() << QStringLiteral("Exported plates %1 for %2: %3")
                             .arg(choice->plates == QStringLiteral("plate") ? choice->plate : choice->plates,
                                  choice->dialect.isEmpty() ? QStringLiteral("no slicer") : choice->dialect,
                                  files.join(QStringLiteral(", ")));
  }
  qInfo().noquote() << QStringLiteral("Exported %1: %2, %3").arg(choice->path, choice->format, what);
  // Its warnings as every export's (mitcad#1040).
  showDone(*this, tr("Exported %1 (%2).").arg(QDir::toNativeSeparators(shown), what), resultWarnings(result),
           QStringLiteral("Export"));
}

void MainWindow::exportSketch(const QString& sketch) {
  const QString name = featureName(sketch);
  const QString suggested = QDir(dialogFolder(DialogGroup::Export)).filePath(name + QStringLiteral(".dxf"));
  // In the design's length unit unless another is chosen (mitcad#1408).
  QString unit = lengthUnit();
  const QString path = dialogSaveDxf(this, tr("Export DXF"), suggested, unit);
  if (path.isEmpty()) {
    return;
  }
  rememberDialogFolder(DialogGroup::Export, path);
  QJsonObject result;
  if (!runCommand(cmd("export_sketch", {{QStringLiteral("sketch"), sketch},
                                        {QStringLiteral("path"), path},
                                        {QStringLiteral("units"), unit}}),
                  &result)) {
    sheetCritical(this, tr("Export DXF"), tr("Could not export:\n%1").arg(m_lastError));
    return;
  }
  // Construction geometry is left out here (mitcad#1445); File > Export
  // offers to include it.
  qInfo().noquote() << QStringLiteral("Exported %1: dxf, %2 entities, in %3, construction geometry %4")
                           .arg(path)
                           .arg(result.value(QStringLiteral("entities")).toInt())
                           .arg(result.value(QStringLiteral("units")).toString(unit),
                                result.value(QStringLiteral("construction")).toBool()
                                    ? QStringLiteral("included")
                                    : QStringLiteral("left out"));
}

// The top-level fields a newer MitCad wrote that this one did not read
// (mitcad#1132): all of them, or with a path those saving there drops.
QStringList MainWindow::unknownFields(const QString& path) const {
  QJsonObject request{{QStringLiteral("query"), QStringLiteral("unknown_fields")}};
  if (!path.isEmpty()) {
    request.insert(QStringLiteral("path"), path);
  }
  QStringList fields;
  try {
    for (const QJsonValue& field : queryObject(request).value(QStringLiteral("fields")).toArray()) {
      fields << field.toString();
    }
  } catch (const std::exception& e) {
    qWarning().noquote() << QStringLiteral("Unknown fields not read: %1").arg(errorText(e));
  }
  return fields;
}

void MainWindow::warnUnknownFields() {
  const QStringList fields = unknownFields(QString());
  if (fields.isEmpty()) {
    return;
  }
  const QString list = fields.join(QStringLiteral(", "));
  qInfo().noquote() << QStringLiteral("Unknown fields of %1 not read: %2").arg(m_filePath, list);
  // A warning on every surface (the status bar of Docked, the message band
  // of Floating), not a dialog: the design opens as this MitCad reads it.
  showWarning(tr("This design was written by a newer MitCad: %1 not read. Saving over it drops them.").arg(list));
}

std::optional<bool> MainWindow::askDropUnknownFields(const QString& path) {
  const QStringList fields = unknownFields(path);
  if (fields.isEmpty()) {
    return true;
  }
  const QString name = QFileInfo(path).fileName();
  const QString list = fields.join(QStringLiteral(", "));
  qInfo().noquote() << QStringLiteral("Save drops unknown fields: %1: %2").arg(path, list);
  // An agent cannot answer a question (MCP's document_save): refused, the
  // file as it is.
  if (m_mcpRequestActive) {
    showError(tr("%1 was written by a newer MitCad: saving over it would drop %2, which this MitCad did not "
                 "read. Save it in the application, which asks first, or give another path.")
                  .arg(name, list));
    return std::nullopt;
  }
  QMessageBox box(QMessageBox::Warning, tr("MitCad"),
                  tr("%1 was written by a newer MitCad. Saving over it drops what this MitCad did not read: %2.")
                      .arg(name, list),
                  QMessageBox::NoButton, this);
  box.setInformativeText(tr("Cancel keeps the file as it is. Save As writes the design to a new file and leaves "
                            "this one as it is."));
  QPushButton* drop = box.addButton(tr("&Save and Drop Them"), QMessageBox::DestructiveRole);
  QPushButton* another = box.addButton(tr("Save &As…"), QMessageBox::AcceptRole);
  box.addButton(QMessageBox::Cancel);
  box.setDefaultButton(another);
  prepareModal(&box);
  execDialog(&box);
  if (box.clickedButton() == drop) {
    qInfo().noquote() << QStringLiteral("Save drops unknown fields: save");
    return true;
  }
  if (box.clickedButton() == another) {
    qInfo().noquote() << QStringLiteral("Save drops unknown fields: save as");
    return false;
  }
  qInfo().noquote() << QStringLiteral("Save drops unknown fields: cancelled");
  showHint(tr("Save canceled: %1 is as it was.").arg(name));
  return std::nullopt;
}

} // namespace mitcad
