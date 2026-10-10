// SPDX-License-Identifier: MIT
#include "MainWindow.hpp"

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <functional>
#include <initializer_list>
#include <map>
#include <utility>

#include <QAction>
#include <QApplication>
#include <QCloseEvent>
#include <QDir>
#include <QDockWidget>
#include <QElapsedTimer>
#include <QFile>
#include <QFileDialog>
#include <QFileInfo>
#include <QJsonDocument>
#include <QLabel>
#include <QMenu>
#include <QMenuBar>
#include <QMessageBox>
#include <QPair>
#include <QPushButton>
#include <QScrollArea>
#include <QStackedWidget>
#include <QStandardPaths>
#include <QStatusBar>
#include <QTimer>
#include <QToolBar>
#include <QToolButton>
#include <QVBoxLayout>
#include <QtLogging>

#include <BRepAdaptor_Curve.hxx>
#include <BRepAdaptor_Surface.hxx>
#include <BRepBndLib.hxx>
#include <BOPTools_AlgoTools3D.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRep_Builder.hxx>
#include <Bnd_Box.hxx>
#include <IntTools_Context.hxx>
#include <BRep_Tool.hxx>
#include <NCollection_IndexedMap.hxx>
#include <TopExp.hxx>
#include <TopExp_Explorer.hxx>
#include <TopLoc_Location.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <TopoDS_Edge.hxx>
#include <TopoDS_Shape.hxx>
#include <TopTools_ShapeMapHasher.hxx>
#include <Precision.hxx>
#include <gp.hxx>
#include <gp_Ax2.hxx>
#include <gp_Ax3.hxx>
#include <gp_Circ.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Trsf.hxx>

#include "AboutDialog.hpp"
#include "browser/BrowserController.hpp"
#include "browser/TimelineWidget.hpp"
#include "commands/Commands.hpp"
#include "files/Autosave.hpp"
#include "files/FileFormats.hpp"
#include "files/LiveController.hpp"
#include "files/LockController.hpp"
#include "files/ProjectIndicator.hpp"
#include "files/RemoteController.hpp"
#include "files/VersionDialogs.hpp"
#include "framework/Appearances.hpp"
#include "framework/Dialogs.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/ChromeStyle.hpp"
#include "framework/CommandSearch.hpp"
#include "framework/FloatingLayout.hpp"
#include "framework/GlassCard.hpp"
#include "framework/Icons.hpp"
#include "framework/Json.hpp"
#include "framework/CacheDiagnostics.hpp"
#include "framework/ModelShapes.hpp"
#include "framework/ResultCache.hpp"
#include "framework/Ribbon.hpp"
#include "framework/TitleBar.hpp"
#include "framework/ShortcutDialog.hpp"
#include "framework/Theme.hpp"
#include "framework/Diagnostics.hpp"
#include "framework/TestSync.hpp"
#include "platform/MacChrome.hpp"
#include "sketch/SketchPalette.hpp"
#include "sketch/SketchTool.hpp"
#include "update/UpdateController.hpp"
#include "report/ReportCenter.hpp"
#include "view/ViewController.hpp"

namespace mitcad {
namespace {

const QString kFileSuffix = QStringLiteral("mitcad");
const QColor kSelectionColor(0x1f, 0x7a, 0xff);
// Size of the planes and axes that stand for datums, mm.
constexpr double kDatumSize = 100.0;
const QStringList kOriginDatums = {QStringLiteral("xy"), QStringLiteral("xz"), QStringLiteral("yz"),
                                   QStringLiteral("x"),  QStringLiteral("y"),  QStringLiteral("z"),
                                   QStringLiteral("origin")};

std::string utf8(const QString& text) { return text.toStdString(); }

TopoDS_Shape occtShape(const std::shared_ptr<geometry::Shape>& shape) {
  return shape ? shape->occt() : TopoDS_Shape();
}

QString pathOf(const QJsonValue& uids) {
  QStringList path;
  for (const QJsonValue& uid : uids.toArray()) {
    path << uid.toString();
  }
  return path.join(QLatin1Char('/'));
}

SelectKind datumKind(const QString& type) {
  if (type == QStringLiteral("plane")) {
    return SelectKind::Plane;
  }
  return type == QStringLiteral("axis") ? SelectKind::Axis : SelectKind::Point;
}

bool isSketchKind(SelectKind kind) {
  return kind == SelectKind::SketchCurve || kind == SelectKind::SketchPoint ||
         kind == SelectKind::SketchConstraint || kind == SelectKind::SketchDimension;
}

// A plane to sketch on: a datum plane or a planar face.
bool isSketchPlane(const SelectionItem& item) {
  return item.kind == SelectKind::Plane ||
         (item.kind == SelectKind::Face && item.geometry == QStringLiteral("plane"));
}

QJsonValue planeOf(const SelectionItem& item) {
  if (item.kind == SelectKind::Plane) {
    return item.owner;
  }
  return QJsonObject{{QStringLiteral("face"), item.name}, {QStringLiteral("body"), item.owner}};
}

// The sketch.create fields for a plane to sketch on, with where it was
// picked: another component's face or plane is linked into the active
// component (mitcad#100). The origin planes are the active component's.
QJsonObject sketchPlaneFields(const SelectionItem& item) {
  QJsonObject fields{{QStringLiteral("plane"), planeOf(item)}};
  if (item.kind == SelectKind::Face || !kOriginDatums.contains(item.owner)) {
    fields.insert(QStringLiteral("occurrence"), item.occurrence);
  }
  return fields;
}

// The document's state is its project file's, as saved or opened: the
// model then reports it unmodified until its revision changes (P8).
void markSaved(Document& document) {
  document.command(rustStr(compactJson({{QStringLiteral("cmd"), QStringLiteral("mark_saved")}})));
}

// Gives the actions of a menu and its submenus that have no menu role none.
void noGuessedMenuRoles(QMenu* menu) {
  for (QAction* action : menu->actions()) {
    if (action->menuRole() == QAction::TextHeuristicRole) {
      action->setMenuRole(QAction::NoRole);
    }
    if (action->menu() != nullptr) {
      noGuessedMenuRoles(action->menu());
    }
  }
}

} // namespace

MainWindow::MainWindow(bool demo, QWidget* parent)
    : QMainWindow(parent), m_document(demo ? new_demo_document() : new_document()),
      m_viewer(new OcctViewer(this)), m_statusLabel(new QLabel(this)),
      m_rendererLabel(new QLabel(this)) {
  markSaved(*m_document); // the demo's commands are no change to save
  // The result store (P7d), as every document gets it (installDocument);
  // what is old in it goes meanwhile.
  m_document->set_result_store(rustStr(resultStoreDirectory().toUtf8()), rustStr(resultStoreBuildId().toUtf8()),
                               rustStr(tr("Untitled").toUtf8()));
  m_document->set_memory_cache_budget(static_cast<std::uint64_t>(CacheSettings::load().memoryMegabytes));
  collectResultStoreGarbage();
  m_worker = new ModelWorker(this);
  m_worker->start();
  setAcceptDrops(true);

  // The project indicator (mitcad#89): the current project, its kind, the
  // open design's version and its remote's state (P12 remote), with the
  // project's menu.
  m_indicator = new ProjectIndicator([this](const QString& id) { trigger(id); }, this);
  createRemote();
  createLocks();
  if (chromeStyle() == ChromeStyle::Floating) {
    // No status bar: the view is the central widget of createPanels(),
    // messages go to a transient pill over it. The labels stay as the store
    // of the texts (the log), in a widget that is never shown; the project
    // indicator goes to the title bar row (createPanels).
    auto* store = new QWidget(this);
    store->setObjectName(QStringLiteral("hiddenStore"));
    store->hide();
    for (QWidget* widget : std::initializer_list<QWidget*>{m_statusLabel, m_indicator, m_rendererLabel}) {
      widget->setParent(store);
    }
  } else {
    setCentralWidget(m_viewer);
    statusBar()->addWidget(m_statusLabel, 1);
    statusBar()->addPermanentWidget(m_indicator);
    statusBar()->addPermanentWidget(m_rendererLabel);
  }
  connect(m_viewer, &OcctViewer::glInitialized, this, [this](const QString& renderer) {
    m_rendererLabel->setText(tr("OpenGL: %1").arg(renderer));
    qInfo().noquote() << "OpenGL renderer:" << renderer;
    if (m_reports != nullptr) {
      m_reports->setGraphics(renderer, m_viewer->glVersion());
    }
  });
  connect(m_viewer, &OcctViewer::glFailed, this, &MainWindow::showError);
  // Picks arrive while the view is painting; react afterwards.
  connect(m_viewer, &OcctViewer::picked, this, &MainWindow::onPicked, Qt::QueuedConnection);
  connect(m_viewer, &OcctViewer::contextMenuRequested, this, &MainWindow::showContextMenu,
          Qt::QueuedConnection);
  connect(m_viewer, &OcctViewer::viewChanged, this, &MainWindow::logBodyPlaces);
  // Components the joints move are dragged in the view (mitcad#55).
  setUpOccurrenceDrag();

  m_sketch = new sketch::SketchController(*this, *m_viewer, this);
  connect(m_sketch, &sketch::SketchController::toolChanged, this, [this] { updateIdleView(); });

  m_registry = new CommandRegistry(this);
  // Help > Send Feedback and the error reports (mitcad#61, mitcad#62).
  m_reports = new ReportCenter(
      *this,
      {[this] { return modelBusy() || !m_import.isNull(); },
       [this](std::function<void()> call) {
         runJob(QStringLiteral("test crash"), tr("Crashing for a test"), [call](ModelJob&) { call(); },
                JobLog::Always);
       },
       [this](const QString& message) { showHint(message); }},
      this);
  // Automatic updates (mitcad#9): Help > Check for Updates and the notice.
  m_updates = new UpdateController(
      *this, {[this] { return close(); }, [this](std::function<void()> call) { whenIdle(this, std::move(call)); }},
      this);
  m_viewController =
      new ViewController(*m_viewer, *m_registry, *this, [this] { lookAtSelection(); }, this);
  m_viewController->gridSnapChanged = [this](bool on, double step) {
    m_sketch->setGridSnap(on);
    m_sketch->setGridStep(step);
  };
  // The sketch palette's Snap to grid is the View menu's too.
  connect(m_sketch, &sketch::SketchController::optionsChanged, this,
          [this] { m_viewController->setSnapToGrid(m_sketch->gridSnap()); });
  connect(m_viewer, &OcctViewer::orientationCubeMenuRequested, m_viewController,
          &ViewController::showOrientationCubeMenu, Qt::QueuedConnection);
  registerCommands();
  m_registry->createActions(this, [this](const QString& id) { trigger(id); });

  // Enter confirms the active command (or what a sketch tool has typed)
  // wherever the focus is; Esc cancels it, or steps back in a
  // sketch tool, or clears the selection.
  m_confirmAction = new QAction(tr("OK"), this);
  m_confirmAction->setShortcuts({QKeySequence(Qt::Key_Return), QKeySequence(Qt::Key_Enter)});
  connect(m_confirmAction, &QAction::triggered, this, &MainWindow::confirm);
  addAction(m_confirmAction);
  m_cancelAction = new QAction(tr("Cancel"), this);
  m_cancelAction->setShortcut(Qt::Key_Escape);
  connect(m_cancelAction, &QAction::triggered, this, &MainWindow::cancel);
  addAction(m_cancelAction);

  createMenus();
  createRibbon();
  createPanels();
  if (chromeStyle() == ChromeStyle::Docked) {
    statusBar()->insertPermanentWidget(0, m_controller->failures());
  } else if (auto* titleBar = findChild<TitleBar*>()) {
    // Beside the document's name in the title bar row.
    titleBar->setIndicator(m_indicator);
  }
  m_search = new CommandSearch(*m_registry, this);
  connect(m_search, &CommandSearch::chosen, this, &MainWindow::trigger, Qt::QueuedConnection);
  createAutosave();
  createLive();

  recompute();
  showHint(tr("Create a sketch to start modelling. S searches for commands."));
  TestSync::singleShot(500, this, [this] { m_ribbon->logLayout(); });
  m_updates->startUp();
  m_remote->startUp();
  m_live->startUp();
  m_locks->startUp();
  m_reports->startUp();
  if (m_header != nullptr) {
    // The update, remote, live and edit-lock notices under the ribbon, not
    // above the title bar row, which the window's content extends under.
    for (const char* name : {"updateBar", "remoteBar", "liveBar", "lockBar"}) {
      if (auto* bar = findChild<QToolBar*>(QLatin1String(name), Qt::FindDirectChildrenOnly)) {
        const bool shown = !bar->isHidden();
        removeToolBar(bar);
        m_header->layout()->addWidget(bar);
        bar->setVisible(shown);
      }
    }
  }
}

MainWindow::~MainWindow() {
  // The session refers to the window as its host.
  delete m_session;
}

// ---------------------------------------------------------------------------
// Commands

void MainWindow::registerCommands() {
  const auto action = [](const char* id, const QString& name, const char* icon,
                         CommandDef::Mode mode, std::function<void()> run) {
    CommandDef def;
    def.id = QString::fromLatin1(id);
    def.name = name;
    def.icon = QString::fromLatin1(icon);
    def.kind = CommandDef::Kind::Action;
    def.mode = mode;
    def.tab.clear(); // menus only, unless placed below
    def.run = std::move(run);
    return def;
  };

  CommandDef sketch = action("sketch.create", tr("Create Sketch"), "sketch", CommandDef::Mode::Model,
                             [this] { startSketch(); });
  sketch.tooltip = tr("Starts a sketch on a plane or a planar face: pick it, or select it first");
  sketch.tab = QStringLiteral("SOLID");
  sketch.group = QStringLiteral("CREATE");
  sketch.pinned = true;
  sketch.keywords = {QStringLiteral("new sketch"), QStringLiteral("draw")};

  // The SOLID tab's families (U4), in menu order: Create Sketch goes
  // into CREATE after New Component.
  registerCreateCommands(*m_registry, *this, sketch);
  registerModifyCommands(*m_registry, *this);
  registerAssembleCommands(*m_registry, *this);
  registerConstructCommands(*m_registry, *this);
  registerInspectCommands(*m_registry, *this, this, [this] { return m_selection; });
  registerSketchCommands(*m_registry, *m_sketch);

  CommandDef removeDef =
      action("inspect.remove_section", tr("Remove Section Analysis"), "section-analysis",
             CommandDef::Mode::Model, [this] { removeSection(); });
  removeDef.tooltip = tr("Hides the section analysis shown: the bodies whole again");
  removeDef.tab = QStringLiteral("SOLID");
  removeDef.group = QStringLiteral("INSPECT");
  removeDef.keywords = {QStringLiteral("section"), QStringLiteral("clip")};
  removeDef.enabled = [this] { return !m_snapshot.shownAnalysis().isEmpty(); };
  m_registry->add(removeDef);
  CommandDef flip = action("inspect.flip_section", tr("Flip Section Analysis"), "flip",
                           CommandDef::Mode::Model, [this] { flipSection(); });
  flip.tooltip = tr("Shows the other side of the section");
  flip.tab = QStringLiteral("SOLID");
  flip.group = QStringLiteral("INSPECT");
  flip.keywords = {QStringLiteral("section"), QStringLiteral("clip"), QStringLiteral("other side")};
  flip.enabled = [this] { return !m_snapshot.shownAnalysis().isEmpty(); };
  m_registry->add(flip);
  // ASSEMBLE (mitcad#55): after Drive Joint.
  CommandDef animate = action("assemble.animate_joint", tr("Animate Joint"), "drive-joint",
                              CommandDef::Mode::Model, [this] { animateSelectedJoint(); });
  animate.tooltip = tr("Shows a joint's free motion through its range: the joint selected in the "
                       "timeline, else the newest; nothing changes");
  animate.tab = QStringLiteral("SOLID");
  animate.group = QStringLiteral("ASSEMBLE");
  animate.keywords = {QStringLiteral("motion"), QStringLiteral("play"), QStringLiteral("simulate")};
  animate.enabled = [this] {
    const QJsonArray joints = m_snapshot.joints.value(QStringLiteral("joints")).toArray();
    return std::any_of(joints.begin(), joints.end(), [](const QJsonValue& joint) {
      return !joint.toObject().value(QStringLiteral("motions")).toArray().isEmpty();
    });
  };
  m_registry->add(animate);

  CommandDef finish = action("sketch.finish", tr("Finish Sketch"), "finish-sketch",
                             CommandDef::Mode::Sketch, [this] { finishSketch(); });
  finish.shortcut = QKeySequence(Qt::CTRL | Qt::Key_Return);
  finish.tab = QStringLiteral("SKETCH");
  finish.group = QStringLiteral("FINISH SKETCH");
  finish.pinned = true;
  m_registry->add(finish);

  CommandDef undoDef =
      action("edit.undo", tr("Undo"), "undo", CommandDef::Mode::Any, [this] { undo(); });
  undoDef.shortcut = QKeySequence::Undo;
  undoDef.enabled = [this] {
    return query({{QStringLiteral("query"), QStringLiteral("document")}})
        .toObject()
        .value(QStringLiteral("undo"))
        .isString();
  };
  m_registry->add(undoDef);
  CommandDef redoDef =
      action("edit.redo", tr("Redo"), "redo", CommandDef::Mode::Any, [this] { redo(); });
#ifdef Q_OS_MACOS
  redoDef.shortcut = QKeySequence::Redo; // Shift+Cmd+Z
#else
  // Ctrl+Y (Windows' standard key) stays; Ctrl+Shift+Z (the standard key of
  // Linux desktops and macOS) works too.
  redoDef.shortcut = QKeySequence(Qt::CTRL | Qt::Key_Y);
  redoDef.alternates = {QKeySequence(Qt::CTRL | Qt::SHIFT | Qt::Key_Z)};
#endif
  redoDef.enabled = [this] {
    return query({{QStringLiteral("query"), QStringLiteral("document")}})
        .toObject()
        .value(QStringLiteral("redo"))
        .isString();
  };
  m_registry->add(redoDef);

  CommandDef fit = action("view.fit", tr("Fit"), "fit", CommandDef::Mode::Any,
                          [this] { m_viewer->fitAll(); });
#ifdef Q_OS_MACOS
  // The F keys are media keys on Mac laptops: Cmd+0 fits, F6 still does.
  fit.shortcut = QKeySequence(Qt::CTRL | Qt::Key_0);
  fit.alternates = {QKeySequence(Qt::Key_F6)};
#else
  fit.shortcut = QKeySequence(Qt::Key_F6);
#endif
  fit.keywords = {QStringLiteral("zoom all"), QStringLiteral("view")};
  fit.duringCommands = true;
  m_registry->add(fit);
  // Standard views, visual styles, the camera, the grid, the environment,
  // named views, Preferences and the shortcut overview (U5).
  m_viewController->registerCommands();
  // Import, export and the INSERT group (U6).
  registerFileCommands();
  // New Project, Save Version, Start Version History (P12d).
  registerVersionCommands();
  // Component libraries (mitcad#64, mitcad#63).
  registerLibraryCommands();

  CommandDef search = action("tools.search", tr("Command Search"), "search", CommandDef::Mode::Any,
                             [this] { openCommandSearch(); });
  search.shortcut = QKeySequence(Qt::Key_S);
  search.tooltip = tr("Find a command by name");
  m_registry->add(search);

  CommandDef shortcuts = action("tools.shortcuts", tr("Keyboard Shortcuts..."), "keyboard",
                                CommandDef::Mode::Any, [this] { editShortcuts(); });
  shortcuts.keywords = {QStringLiteral("keys"), QStringLiteral("hotkeys")};
  m_registry->add(shortcuts);

  // The browser and the timeline (U3).
  CommandDef parameters = action("solid.parameters", tr("Change Parameters"), "parameters",
                                 CommandDef::Mode::Model, [this] { m_controller->openParameters(); });
  parameters.tooltip = tr("User and model parameters: names, expressions, units and comments");
  parameters.tab = QStringLiteral("SOLID");
  parameters.group = QStringLiteral("MODIFY");
  parameters.keywords = {QStringLiteral("parameters"), QStringLiteral("user parameter"),
                         QStringLiteral("expression"), QStringLiteral("fx")};
  m_registry->add(parameters);
  // Appearances with their physically based parameters (mitcad#46).
  CommandDef appearances = action("solid.appearances", tr("Edit Appearances..."), "appearance",
                                  CommandDef::Mode::Model, [this] { m_controller->openAppearances(appearanceTargets()); });
  appearances.tooltip = tr("The appearance library and this design's own appearances: colour, metalness, "
                           "roughness, transmission and more; assigns one to the selected bodies");
  appearances.tab = QStringLiteral("SOLID");
  appearances.group = QStringLiteral("MODIFY");
  appearances.keywords = {QStringLiteral("material"), QStringLiteral("colour"), QStringLiteral("color"),
                          QStringLiteral("render"), QStringLiteral("glass"), QStringLiteral("metal")};
  m_registry->add(appearances);

  // The caches of computed results (P7d).
  CommandDef diagnostics = action("help.diagnostics", tr("Diagnostics..."), "measure", CommandDef::Mode::Any,
                                  [this] { showDiagnostics(); });
  diagnostics.tooltip = tr("What the caches of computed results hold and do, and the program's memory");
  diagnostics.keywords = {QStringLiteral("cache"), QStringLiteral("memory"), QStringLiteral("disk"),
                          QStringLiteral("performance")};
  diagnostics.duringCommands = true;
  m_registry->add(diagnostics);
  m_viewController->showDiagnostics = [this] { showDiagnostics(); };
  // Help > Check for Updates (mitcad#9).
  m_updates->registerCommands(*m_registry);
  // Help > Send Feedback (mitcad#61).
  m_reports->registerCommands(*m_registry);
}

void MainWindow::showDiagnostics() {
  CacheDiagnosticsDialog::Host host;
  host.report = [this] {
    return query({{QStringLiteral("query"), QStringLiteral("cache")}, {QStringLiteral("disk"), true}}).toObject();
  };
  host.clearMemory = [this] {
    try {
      return command({{QStringLiteral("cmd"), QStringLiteral("clear_cache")}});
    } catch (const std::exception& e) {
      showError(errorText(e));
      return QJsonObject();
    }
  };
  CacheDiagnosticsDialog dialog(std::move(host), QApplication::activeModalWidget() != nullptr
                                                      ? QApplication::activeModalWidget()
                                                      : this);
  prepareModal(&dialog);
  dialog.exec();
}

void MainWindow::createMenus() {
  createFileMenu();

  QMenu* edit = menuBar()->addMenu(tr("&Edit"));
  edit->addAction(m_registry->action(QStringLiteral("edit.undo")));
  edit->addAction(m_registry->action(QStringLiteral("edit.redo")));
  edit->addSeparator();
  edit->addAction(m_registry->action(QStringLiteral("solid.parameters")));
  QMenu* view = menuBar()->addMenu(tr("&View"));
  view->setObjectName(QStringLiteral("viewMenu"));
  view->addAction(m_registry->action(QStringLiteral("view.fit")));
  QMenu* tools = menuBar()->addMenu(tr("&Tools"));
  tools->addAction(m_registry->action(QStringLiteral("tools.search")));
  tools->addAction(m_registry->action(QStringLiteral("tools.shortcuts")));
  // Component libraries (mitcad#64, mitcad#63).
  createLibraryMenu(tools);
#ifdef Q_OS_MACOS
  createWindowMenu();
#endif
  QMenu* help = menuBar()->addMenu(tr("&Help"));
  m_viewController->createMenus(view, tools, help);
  help->addAction(m_registry->action(QStringLiteral("help.diagnostics")));
  help->addAction(m_registry->action(QStringLiteral("help.check_updates")));
  help->addAction(m_registry->action(QStringLiteral("help.send_feedback")));
  // About and About Qt: on macOS Qt moves them to the application menu
  // (AboutRole, AboutQtRole); elsewhere they end the Help menu.
#ifndef Q_OS_MACOS
  help->addSeparator();
#endif
  QAction* about = help->addAction(tr("&About Mitcad"));
  about->setMenuRole(QAction::AboutRole);
  connect(about, &QAction::triggered, this, [this] { showAboutDialog(this); });
  QAction* aboutQt = help->addAction(tr("About &Qt"));
  aboutQt->setMenuRole(QAction::AboutQtRole);
  connect(aboutQt, &QAction::triggered, this, [] { QApplication::aboutQt(); });

  // On macOS Qt guesses a role for every action without one from its text
  // (a name beginning or ending with "Settings", "Options", "Setup", "About"
  // moves to the application menu): the actions with a role (Exit, About,
  // Preferences) keep it, all others get none.
  for (QAction* top : menuBar()->actions()) {
    if (top->menu() != nullptr) {
      noGuessedMenuRoles(top->menu());
    }
  }
  // The menu bar's menus show no icons on macOS, although the actions have
  // them for the ribbon, whose menus (and the context menus) keep theirs.
  // The actions are shared, so the images are hidden in the native menus,
  // not by AA_DontShowIconsInMenus or setIconVisibleInMenu, which would
  // take them from the ribbon's menus too.
  for (QAction* top : menuBar()->actions()) {
    mac::hideMenuImages(top->menu());
  }
}

#ifdef Q_OS_MACOS
// The standard Window menu of macOS applications.
void MainWindow::createWindowMenu() {
  QMenu* window = menuBar()->addMenu(tr("&Window"));
  QAction* minimize = window->addAction(tr("Minimize"));
  minimize->setShortcut(QKeySequence(Qt::CTRL | Qt::Key_M));
  connect(minimize, &QAction::triggered, this, [this] { showMinimized(); });
  QAction* zoom = window->addAction(tr("Zoom"));
  connect(zoom, &QAction::triggered, this, [this] {
    if (isMaximized() || isFullScreen()) {
      showNormal();
    } else {
      showMaximized();
    }
  });
  window->addSeparator();
  QAction* front = window->addAction(tr("Bring All to Front"));
  connect(front, &QAction::triggered, this, [this] {
    if (isMinimized()) {
      showNormal();
    }
    for (QWidget* top : QApplication::topLevelWidgets()) {
      if (top->isVisible() && !top->isMinimized()) {
        top->raise();
      }
    }
    raise();
    activateWindow();
  });
}
#endif

void MainWindow::createRibbon() {
  const bool floating = chromeStyle() == ChromeStyle::Floating;
  m_ribbon = new Ribbon(*m_registry, floating ? Ribbon::Presentation::Mac : Ribbon::Presentation::Classic,
                        this);

  // The SELECT group: the selection filters.
  auto* filterMenu = new QMenu(tr("Selection Filters"), this);
  const QList<QPair<SelectFilter, QString>> filters = {
      {SelectKind::Body, tr("Bodies")},
      {SelectKind::Face, tr("Faces")},
      {SelectKind::Edge, tr("Edges")},
      {SelectKind::Vertex, tr("Vertices")},
      {SelectKind::Profile, tr("Profiles")},
      {SelectKind::SketchCurve, tr("Sketch Curves")},
      {SelectKind::SketchPoint, tr("Sketch Points")},
      {kConstructionGeometry, tr("Construction Geometry")}};
  for (const auto& entry : filters) {
    // Not a structured binding: the lambda below captures the name, which
    // C++17 allows only for plain variables.
    const SelectFilter kinds = entry.first;
    const QString name = entry.second;
    QAction* filter = filterMenu->addAction(name);
    filter->setCheckable(true);
    // Faces win over bodies where both could be picked; bodies are off so
    // that a click on a body picks its face (faces first).
    filter->setChecked(kinds != SelectFilter(SelectKind::Body));
    connect(filter, &QAction::toggled, this, [this, name](bool on) {
      qDebug().noquote() << QStringLiteral("Selection filter %1: %2")
                                .arg(name, on ? QStringLiteral("on") : QStringLiteral("off"));
      updateIdleView();
    });
    m_filterActions.insert(kinds.toInt(), filter);
  }
  filterMenu->addSeparator();
  filterMenu->addAction(tr("Select All Filters"), this, [this] {
    for (QAction* filter : std::as_const(m_filterActions)) {
      filter->setChecked(true);
    }
  });
  auto* filterAction = new QAction(themeIcon(QStringLiteral("filter")), tr("Selection Filters"), this);
  filterAction->setMenu(filterMenu);
  filterAction->setToolTip(tr("What a click in the view can select"));
  auto* windowAction = new QAction(themeIcon(QStringLiteral("select")), tr("Window Selection"), this);
  windowAction->setToolTip(tr("Drag to the right: what lies inside; to the left: what it crosses"));
  windowAction->setCheckable(true);
  windowAction->setChecked(true);
  windowAction->setEnabled(false);
  auto* clearAction = new QAction(tr("Clear Selection"), this);
  connect(clearAction, &QAction::triggered, this, [this] { setSelection({}); });
  for (const QString& tab : {QStringLiteral("SOLID"), QStringLiteral("SKETCH")}) {
    m_ribbon->addGroupExtras(tab, QStringLiteral("SELECT"),
                             {windowAction, filterAction, clearAction}, {filterAction});
  }
  m_ribbon->build();

  // Undo and Redo get the system's symbol on macOS, the drawing elsewhere.
  for (const char* id : {"edit.undo", "edit.redo"}) {
    QAction* action = m_registry->action(QString::fromLatin1(id));
    action->setIcon(mac::symbolIcon(
        QLatin1String(id) == QLatin1String("edit.undo") ? QStringLiteral("arrow.uturn.backward")
                                                          : QStringLiteral("arrow.uturn.forward"),
        action->icon()));
  }

  if (floating) {
#if defined(Q_OS_MACOS) && QT_VERSION >= QT_VERSION_CHECK(6, 9, 0)
    // The content goes under the native title bar, which the title bar row
    // replaces (the traffic lights stay). Before the window is shown.
    setWindowFlag(Qt::ExpandedClientAreaHint, true);
    setWindowFlag(Qt::NoTitleBarBackgroundHint, true);
    // Qt would keep the content below the title bar by a content margin.
    setAttribute(Qt::WA_ContentsMarginsRespectsSafeArea, false);
#endif
    m_header = new QWidget;
    m_header->setObjectName(QStringLiteral("ribbonHeader"));
    auto* column = new QVBoxLayout(m_header);
    column->setContentsMargins(0, 0, 0, 0);
    column->setSpacing(0);
    auto* titleBar = new TitleBar(m_ribbon, m_registry->action(QStringLiteral("edit.undo")),
                                  m_registry->action(QStringLiteral("edit.redo")),
                                  m_registry->action(QStringLiteral("tools.search")),
                                  m_registry->action(QStringLiteral("sketch.finish")));
    column->addWidget(titleBar);
    column->addWidget(m_ribbon);
    return;
  }

  QToolBar* bar = addToolBar(tr("Toolbar"));
  bar->setObjectName(QStringLiteral("ribbonToolbar"));
  bar->setMovable(false);
  bar->setFloatable(false);
  // Undo and Redo at hand in the application bar.
  for (const char* id : {"edit.undo", "edit.redo"}) {
    QAction* action = m_registry->action(QString::fromLatin1(id));
    bar->addAction(action);
    if (auto* button = qobject_cast<QToolButton*>(bar->widgetForAction(action))) {
      button->setFocusPolicy(Qt::NoFocus);
    }
  }
  bar->addSeparator();
  bar->addWidget(m_ribbon);
}

void MainWindow::createPanels() {
  if (chromeStyle() == ChromeStyle::Floating) {
    createFloatingPanels();
    return;
  }
  // The browser on the left and the timeline at the bottom.
  m_controller = new BrowserController(*this, this);
  auto* browserDock = new QDockWidget(tr("Browser"), this);
  browserDock->setObjectName(QStringLiteral("browserDock"));
  browserDock->setWidget(m_controller->browser());
  addDockWidget(Qt::LeftDockWidgetArea, browserDock);
  auto* timelineDock = new QDockWidget(tr("Timeline"), this);
  timelineDock->setObjectName(QStringLiteral("timelineDock"));
  timelineDock->setWidget(m_controller->timeline());
  addDockWidget(Qt::BottomDockWidgetArea, timelineDock);
  if (QMenu* view = findChild<QMenu*>(QStringLiteral("viewMenu"))) {
    view->addAction(browserDock->toggleViewAction());
    view->addAction(timelineDock->toggleViewAction());
  }

  m_commandPages = new QStackedWidget;
  // A fixed width, so that the view keeps its size when a panel opens.
  m_commandPages->setFixedWidth(300);
  m_idleInfo = new QLabel;
  m_idleInfo->setWordWrap(true);
  m_idleInfo->setAlignment(Qt::AlignTop | Qt::AlignLeft);
  m_idleInfo->setMargin(6);
  m_commandPages->addWidget(m_idleInfo);
  m_palette = new sketch::SketchPalette(*m_sketch, *m_registry, sketchConstraintCommands());
  m_commandPages->addWidget(m_palette);
  auto* commandDock = new QDockWidget(tr("Command"), this);
  commandDock->setObjectName(QStringLiteral("commandDock"));
  commandDock->setWidget(m_commandPages);
  addDockWidget(Qt::RightDockWidgetArea, commandDock);
}

void MainWindow::createFloatingPanels() {
  // Cards over the view instead of docks; the same panels inside.
  m_controller = new BrowserController(*this, this);

  auto* browserCard = new GlassCard;
  browserCard->setObjectName(QStringLiteral("browserDock"));
  browserCard->setTitle(tr("Browser"));
  browserCard->setCollapsible(true);
  // The failed features' badge, in the header: red, small, a capsule.
  QToolButton* failures = m_controller->failures();
  setErrorStyleSheet(failures, QStringLiteral("QToolButton { color: %1; border: 1px solid %1; "
                                              "border-radius: 9px; padding: 0px 6px; }"));
  failures->setAutoRaise(false);
  failures->setIconSize(QSize(12, 12));
  browserCard->addHeaderWidget(failures);
  browserCard->setContent(m_controller->browser(), QMargins(6, 0, 6, 8));

  auto* timelineCard = new GlassCard;
  timelineCard->setObjectName(QStringLiteral("timelineDock"));
  timelineCard->setCapsule(true);
  timelineCard->setContent(m_controller->timeline(), QMargins(18, 4, 18, 4));

  m_commandPages = new QStackedWidget;
  m_idleInfo = new QLabel;
  m_idleInfo->setWordWrap(true);
  m_idleInfo->setAlignment(Qt::AlignTop | Qt::AlignLeft);
  m_idleInfo->setMargin(6);
  m_commandPages->addWidget(m_idleInfo);
  m_palette = new sketch::SketchPalette(*m_sketch, *m_registry, sketchConstraintCommands());
  m_commandPages->addWidget(m_palette);
  m_commandCard = new GlassCard;
  m_commandCard->setObjectName(QStringLiteral("commandDock"));
  m_commandCard->setContent(m_commandPages, QMargins(8, 8, 8, 8));
  m_commandCard->hide(); // nothing to say while idle (updateCommandCard)

  m_floating = new FloatingLayout(m_viewer, m_header, this);
  m_floating->addCard(browserCard, FloatingLayout::Anchor::TopLeft,
                      {260,
                       [browser = m_controller->browser()] { return browser->contentHeight(); },
                       0.55});
  m_floating->addCard(timelineCard, FloatingLayout::Anchor::Bottom);
  m_floating->addCard(m_commandCard, FloatingLayout::Anchor::TopRight,
                      {300, [this] { return commandContentHeight(); }, 1.0, false});
  connect(m_controller->browser(), &BrowserPanel::contentHeightChanged, m_floating,
          &FloatingLayout::requestLayout);
  connect(m_commandPages, &QStackedWidget::currentChanged, this, [this] { updateCommandCard(); });
  setCentralWidget(m_floating);
  m_floating->watchContent(m_idleInfo);
  m_floating->watchContent(m_palette);

  // The View menu shows and hides the cards as it did the docks.
  if (QMenu* view = findChild<QMenu*>(QStringLiteral("viewMenu"))) {
    for (const auto& [text, card] : {std::pair<QString, GlassCard*>{tr("Browser"), browserCard},
                                     std::pair<QString, GlassCard*>{tr("Timeline"), timelineCard}}) {
      QAction* action = view->addAction(text);
      action->setCheckable(true);
      action->setChecked(true);
      connect(action, &QAction::toggled, card, &QWidget::setVisible);
    }
  }
}

int MainWindow::commandContentHeight() const {
  QWidget* page = m_commandPages->currentWidget();
  if (auto* scroll = qobject_cast<QScrollArea*>(page)) {
    return scroll->widget() != nullptr ? scroll->widget()->sizeHint().height() + 2 : 0;
  }
  if (page == m_idleInfo) {
    return m_idleInfo->heightForWidth(m_commandCard->width() - 16);
  }
  return page != nullptr ? page->sizeHint().height() : 0;
}

void MainWindow::updateCommandCard() {
  if (m_commandCard == nullptr) {
    return;
  }
  // Idle with nothing to tell (no selection): no card.
  const bool idle = m_commandPages->currentWidget() == m_idleInfo;
  m_commandCard->setVisible(!(idle && m_idleInfo->text().isEmpty()));
  m_floating->requestLayout();
}

void MainWindow::updateStatusPill(bool error) {
  if (m_floating != nullptr) {
    m_floating->showStatus(m_statusLabel->text(), error, error ? 10000 : 6000);
  }
}

bool MainWindow::isAvailable(const CommandDef& def) const {
  // A read-only window (mitcad#89): only what changes no design.
  if (windowReadOnly() && !readOnlyCommand(def)) {
    return false;
  }
  if (m_mode == Mode::Command) {
    return def.duringCommands && (!def.enabled || def.enabled());
  }
  switch (def.mode) {
  case CommandDef::Mode::Model:
    if (m_mode == Mode::Sketch && def.kind == CommandDef::Kind::Feature) {
      // Extrude and Revolve from a sketch finish it first.
      const InputDef* input = def.primarySelection();
      if (input == nullptr || !input->filter.testFlag(SelectKind::Profile)) {
        return false;
      }
      break;
    }
    if (m_mode != Mode::Idle) {
      return false;
    }
    break;
  case CommandDef::Mode::Sketch:
    if (m_mode != Mode::Sketch) {
      return false;
    }
    break;
  case CommandDef::Mode::Any:
    break;
  }
  return !def.enabled || def.enabled();
}

void MainWindow::updateActions() {
  // The commands' availability checks share their queries' answers.
  QHash<QString, QJsonValue> memo;
  m_queryMemo = &memo;
  struct Reset {
    MainWindow* window;
    ~Reset() { window->m_queryMemo = nullptr; }
  } reset{this};
  m_registry->updateEnabled([this](const CommandDef& def) { return isAvailable(def); });
  m_confirmAction->setEnabled(m_mode == Mode::Command || m_mode == Mode::Sketch);
  updateWindowTitle();
}

void MainWindow::trigger(const QString& id) {
  if (modelBusy()) {
    // From the command search (queued) while a job computes: after it.
    whenIdle(this, [this, id] { trigger(id); });
    return;
  }
  const CommandDef* def = m_registry->find(id);
  if (def != nullptr && windowReadOnly() && !readOnlyCommand(*def)) {
    refuseReadOnly(id);
    return;
  }
  if (def == nullptr || !isAvailable(*def)) {
    return;
  }
  if (m_reports != nullptr) {
    m_reports->noteAction(QStringLiteral("command ") + id);
  }
  m_locks->activity();
  if (def->kind == CommandDef::Kind::Feature) {
    startFeatureCommand(*def);
  } else if (def->run) {
    def->run();
  }
}

void MainWindow::startFeatureCommand(const CommandDef& def, const QString& editUid,
                                     const QJsonObject& analysis) {
  if (!readOnlyCommand(def) && refuseReadOnly(editUid.isEmpty() ? def.id : QStringLiteral("editing %1").arg(editUid))) {
    return;
  }
  if (m_mode == Mode::Sketch && def.mode == CommandDef::Mode::Model) {
    // A feature from sketch mode (Extrude): the sketch is finished first,
    // its selected profiles go to the feature.
    Selection profiles;
    for (const SelectionItem& item : m_selection) {
      if (item.kind == SelectKind::Profile) {
        profiles.append(item);
      }
    }
    if (!finishSketch()) {
      return; // cancelled: the sketch stays open
    }
    m_selection = profiles;
  }
  if (m_mode != Mode::Idle && m_mode != Mode::Sketch) {
    return;
  }
  m_commandInSketch = m_mode == Mode::Sketch;
  if (m_commandInSketch) {
    m_sketch->stopTool();
    m_sketch->setPaused(true);
  }
  const Selection preselection = editUid.isEmpty() && analysis.isEmpty() ? m_selection : Selection();
  setSelection({});
  m_mode = Mode::Command;
  auto* session = new CommandSession(def, *this, editUid, this);
  if (!analysis.isEmpty()) {
    session->editAnalysis(analysis);
  }
  QString error;
  if (!session->start(preselection, error)) {
    delete session;
    m_sketch->setPaused(false);
    m_mode = m_commandInSketch ? Mode::Sketch : Mode::Idle;
    showError(error);
    refreshScene();
    return;
  }
  m_session = session;
  m_lastCommand = def.id;
  connect(session, &CommandSession::finished, this, &MainWindow::onCommandFinished);
  // A long panel scrolls rather than squeezing its rows or growing the
  // window.
  auto* scroll = new QScrollArea;
  scroll->setWidgetResizable(true);
  scroll->setFrameShape(QFrame::NoFrame);
  scroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  CommandPanel* panel = session->createPanel(scroll);
  scroll->setWidget(panel);
  if (m_floating != nullptr) {
    // The card's material shows through; a changed panel resizes the card.
    scroll->viewport()->setAutoFillBackground(false);
    panel->setAutoFillBackground(false);
    m_floating->watchContent(panel);
  }
  m_commandPages->addWidget(scroll);
  m_commandPages->setCurrentWidget(scroll);
  m_panelScroll = scroll;
  if (!session->startsWithValues() || !panel->focusFirstValue()) {
    m_viewer->setFocus();
  }
  updateActions();
}

const CommandDef* MainWindow::editorOf(const QString& uid, QString* name) const {
  try {
    const QJsonObject feature =
        query({{QStringLiteral("query"), QStringLiteral("feature")}, {QStringLiteral("uid"), uid}})
            .toObject();
    if (name != nullptr) {
      *name = feature.value(QStringLiteral("name")).toString(uid);
    }
    return m_registry->editorOf(feature.value(QStringLiteral("def")).toObject());
  } catch (const std::exception&) {
    return nullptr;
  }
}

void MainWindow::editFeature(const QString& uid) {
  if (m_mode != Mode::Idle) {
    return;
  }
  try {
    const QJsonObject feature =
        queryObject({{QStringLiteral("query"), QStringLiteral("feature")}, {QStringLiteral("uid"), uid}});
    if (feature.value(QStringLiteral("type")).toString() == QStringLiteral("sketch")) {
      editSketch(uid);
      return;
    }
  } catch (const std::exception&) {
    return;
  }
  QString name = uid;
  const CommandDef* def = editorOf(uid, &name);
  if (def == nullptr) {
    // Inputs Mitcad's panels do not have yet (U4) would be lost.
    showError(tr("%1 cannot be edited in Mitcad yet.").arg(name));
    return;
  }
  startFeatureCommand(*def, uid);
}

void MainWindow::editAnalysis(const QString& name) {
  if (m_mode != Mode::Idle) {
    return;
  }
  const QJsonObject analysis = m_snapshot.analysis(name);
  const CommandDef* def = m_registry->find(QStringLiteral("inspect.section"));
  if (analysis.isEmpty() || def == nullptr ||
      analysis.value(QStringLiteral("type")).toString() != QStringLiteral("section")) {
    return;
  }
  startFeatureCommand(*def, QString(), analysis);
}

void MainWindow::editInstead(const QString& uid, const QString& input) {
  // Not inside the running command's own call: it goes first.
  whenIdle(this, [this, uid, input] {
    if (m_session) {
      m_session->cancel();
    }
    editFeature(uid);
    if (m_session && m_session->isEditing()) {
      m_session->focusValue(input);
    }
  });
}

void MainWindow::startCommand(const QString& id, const Selection& preselection,
                              std::function<void(bool committed)> finished) {
  const CommandDef* def = m_registry->find(id);
  if (def == nullptr || def->kind != CommandDef::Kind::Feature || m_mode != Mode::Idle) {
    if (finished) {
      finished(false);
    }
    return;
  }
  setSelection(preselection);
  if (!isAvailable(*def)) {
    if (finished) {
      finished(false);
    }
    return;
  }
  m_commandFinished = std::move(finished);
  startFeatureCommand(*def);
  if (!m_session && m_commandFinished) {
    std::exchange(m_commandFinished, nullptr)(false);
  }
}

void MainWindow::onCommandFinished(bool committed) {
  CommandSession* session = m_session;
  m_session = nullptr;
  m_mode = m_commandInSketch && m_sketch->isActive() ? Mode::Sketch : Mode::Idle;
  m_commandInSketch = false;
  m_sketch->setPaused(false);
  m_commandPages->setCurrentWidget(m_mode == Mode::Sketch ? static_cast<QWidget*>(m_palette)
                                                          : static_cast<QWidget*>(m_idleInfo));
  if (session != nullptr) {
    session->deleteLater();
  }
  if (m_panelScroll) {
    m_panelScroll->deleteLater();
  }
  refreshScene();
  m_viewer->setFocus();
  showHint(committed ? tr("Done.") : tr("Command cancelled."));
  if (m_commandFinished) {
    std::exchange(m_commandFinished, nullptr)(committed);
  }
}

void MainWindow::confirm() {
  if (m_session) {
    m_session->commit();
    return;
  }
  if (m_mode == Mode::Sketch) {
    m_sketch->confirm();
  }
}

void MainWindow::cancel() {
  if (m_controller->cancelEditing()) {
    return; // a rename in the browser or the timeline
  }
  if (m_session) {
    m_session->cancel();
    return;
  }
  if (m_mode == Mode::PickPlane) {
    m_mode = Mode::Idle;
    inputActivated(SelectFilter());
    updateIdleView();
    updateActions();
    showHint(tr("Create Sketch cancelled."));
    qDebug() << "Create Sketch cancelled";
    return;
  }
  if (m_mode == Mode::Sketch) {
    if (!m_sketch->cancel()) {
      setSelection({});
    }
    return;
  }
  setSelection({});
}

void MainWindow::openCommandSearch() {
  if (m_mode == Mode::Command) {
    return;
  }
  m_search->popup(m_viewer->mapToGlobal(m_viewer->rect().center()));
}

void MainWindow::editShortcuts() {
  ShortcutDialog dialog(*m_registry, this);
  prepareModal(&dialog);
  dialog.exec();
}

// ---------------------------------------------------------------------------
// Model access

const Document& MainWindow::idleDocument() const {
  if (modelBusy()) {
    // The worker's now: the UI thread may only draw what it showed before
    // (docs/architecture.md, "Background computation").
    qCritical() << "The document was read while a job computes it";
    Q_ASSERT_X(false, "MainWindow::idleDocument", "the document was read while a job computes it");
    throw ModelBusy();
  }
  return *m_document;
}

bool MainWindow::runCommand(const QJsonObject& cmd, QJsonObject* result) {
  const QString name = cmd.value(QStringLiteral("cmd")).toString();
  if (modelBusy()) {
    // A call from a timer or a queued signal while a job computes the model
    // (what is known to come so waits: whenIdle): refused, as the model
    // refuses what it cannot do.
    m_lastError = errorText(ModelBusy());
    m_lastCancelled = false;
    qWarning().noquote() << QStringLiteral("Command %1 refused: %2").arg(name, m_lastError);
    return false;
  }
  QElapsedTimer timer;
  timer.start();
  QJsonObject answer;
  if (m_reports != nullptr) {
    m_reports->noteAction(QStringLiteral("model ") + name);
  }
  try {
    answer = command(cmd);
  } catch (const ComputationCancelled& cancelled) {
    // Not an error: the model is as it was before the command.
    m_lastError = errorText(cancelled);
    m_lastCancelled = true;
    showHint(tr("Computation cancelled."));
    qInfo().noquote() << QStringLiteral("Command %1 cancelled").arg(name);
    return false;
  } catch (const rust::Error& error) {
    m_lastError = errorText(error);
    m_lastCancelled = false;
    showError(m_lastError);
    return false;
  } catch (const ModelBusy&) {
    throw;
  } catch (const ReadOnlyError& error) {
    // A read-only window (mitcad#89): nothing changed.
    m_lastError = errorText(error);
    m_lastCancelled = false;
    qInfo().noquote() << QStringLiteral("Read-only: refused %1").arg(name);
    showError(m_lastError);
    return false;
  } catch (const std::exception& error) {
    // Not the model's answer: an internal error (mitcad#62).
    m_lastError = errorText(error);
    m_lastCancelled = false;
    if (m_reports != nullptr) {
      m_reports->internalError(QStringLiteral("model command %1").arg(name), m_lastError);
    }
    showError(m_lastError);
    return false;
  }
  reportRecompute(answer, static_cast<double>(timer.nsecsElapsed()) / 1.0e6);
  if (result != nullptr) {
    *result = answer;
  }
  m_locks->activity(); // an edit keeps the edit lock active (mitcad#89)
  return true;
}

bool MainWindow::runCommands(const QJsonArray& commands, const QString& label) {
  return runModelCommands(commands, label);
}

void MainWindow::showSection(const QJsonValue& plane) {
  if (setSection(plane)) {
    showBodies();
    updateActions();
  }
}

bool MainWindow::setSection(const QJsonValue& plane) {
  if (plane == m_section) {
    return false;
  }
  m_section = plane;
  qDebug().noquote() << (plane.isNull() ? QStringLiteral("Section analysis off")
                                        : QStringLiteral("Section analysis at %1")
                                              .arg(QString::fromUtf8(compactJson(
                                                  QJsonObject{{QStringLiteral("plane"), plane}}))));
  return true;
}

void MainWindow::followAnalyses() {
  if (m_session && m_session->def().inspect) {
    return; // the panel shows its own section until it closes
  }
  // The shown analysis' plane at the marker; none when its plane is gone.
  const QJsonValue plane = m_snapshot.shownAnalysis().value(QStringLiteral("section"));
  setSection(plane.isObject() ? plane : QJsonValue());
}

QJsonValue MainWindow::cutPlane() const {
  if (m_mode != Mode::Sketch || !m_sketch->hideAbove()) {
    return m_section;
  }
  const geometry::Frame& frame = m_sketch->model().frame;
  const gp_Dir normal = frame.normal();
  return QJsonObject{{QStringLiteral("origin"), QJsonArray{frame.origin.X(), frame.origin.Y(), frame.origin.Z()}},
                     {QStringLiteral("normal"), QJsonArray{normal.X(), normal.Y(), normal.Z()}}};
}

void MainWindow::showSectionCaps(const std::vector<BodyDisplay>& bodies) {
  const QJsonValue cut = cutPlane();
  if (cut.isNull()) {
    m_viewer->setSectionCaps(TopoDS_Shape(), gp_Ax3());
    m_loggedCaps.clear();
    return;
  }
  // The plane in design coordinates (Section Analysis gives it as an origin
  // and a normal; Hide Above Sketch's is the sketch's, whose axes the
  // hatch follows).
  const QJsonObject plane = cut.toObject();
  const gp_Pnt origin = pointOf(plane.value(QStringLiteral("origin")));
  const gp_Vec normal = vectorOf(plane.value(QStringLiteral("normal")));
  if (normal.Magnitude() < 1e-12) {
    m_viewer->setSectionCaps(TopoDS_Shape(), gp_Ax3());
    return;
  }
  const gp_Dir n(normal);
  gp_Ax3 axes(origin, n);
  if (cut != m_section) {
    const geometry::Frame& frame = m_sketch->model().frame;
    axes = gp_Ax3(frame.origin, frame.normal(), frame.x_axis);
  }
  // The faces of a body that lie in the plane, in the body's coordinates.
  const auto inPlane = [&](const TopoDS_Shape& shape, const TopLoc_Location& placement) {
    std::vector<TopoDS_Face> found;
    for (TopExp_Explorer faces(shape, TopAbs_FACE); faces.More(); faces.Next()) {
      const TopoDS_Face& face = TopoDS::Face(faces.Current());
      const BRepAdaptor_Surface surface(face);
      if (surface.GetType() != GeomAbs_Plane) {
        continue;
      }
      const gp_Pln pln = surface.Plane();
      const gp_Pnt at = pln.Location().Transformed(placement.Transformation());
      gp_Dir axis = pln.Axis().Direction();
      axis.Transform(placement.Transformation());
      if (std::abs(axis.Dot(n)) > 1.0 - 1e-9 && std::abs(gp_Vec(origin, at).Dot(gp_Vec(n))) < 1e-6) {
        found.push_back(face);
      }
    }
    return found;
  };
  // The caps: the clipped bodies' faces in the plane, but not those the
  // whole body has there too (a sketch on a body's face).
  TopoDS_Compound caps;
  BRep_Builder builder;
  builder.MakeCompound(caps);
  int count = 0;
  const occ::handle<IntTools_Context> context = new IntTools_Context();
  for (const BodyDisplay& body : bodies) {
    const auto whole = idleDocument().body_shape(body.uid);
    if (whole && whole->occt().IsSame(body.shape->occt())) {
      continue; // not cut
    }
    const std::vector<TopoDS_Face> own = whole ? inPlane(whole->occt(), body.placement)
                                               : std::vector<TopoDS_Face>();
    for (const TopoDS_Face& face : inPlane(body.shape->occt(), body.placement)) {
      gp_Pnt inside;
      gp_Pnt2d uv;
      const bool original =
          !own.empty() && BOPTools_AlgoTools3D::PointInFace(face, inside, uv, context) == 0 &&
          std::any_of(own.begin(), own.end(), [&](const TopoDS_Face& mine) {
            return context->IsValidPointForFace(inside, mine, 1e-6);
          });
      if (!original) {
        builder.Add(caps, face.Moved(body.placement));
        ++count;
      }
    }
  }
  m_viewer->setSectionCaps(count > 0 ? TopoDS_Shape(caps) : TopoDS_Shape(), axes);
  const QString logged = QStringLiteral("Section caps: %1 face(s) at %2")
                             .arg(count)
                             .arg(QString::fromUtf8(compactJson(plane)));
  if (logged != m_loggedCaps) {
    m_loggedCaps = logged;
    qDebug().noquote() << logged;
  }
}

void MainWindow::logSketchReach(const std::vector<BodyDisplay>& bodies) {
  if (m_mode != Mode::Sketch) {
    m_loggedReach.clear();
    return;
  }
  const geometry::Frame& frame = m_sketch->model().frame;
  const gp_Vec normal(frame.normal());
  QStringList reach;
  for (const BodyDisplay& body : bodies) {
    Bnd_Box box;
    BRepBndLib::Add(body.shape->occt().Moved(body.placement), box, false);
    if (box.IsVoid()) {
      continue;
    }
    const gp_Pnt low = box.CornerMin();
    const gp_Pnt high = box.CornerMax();
    double front = -1e100;
    for (const double x : {low.X(), high.X()}) {
      for (const double y : {low.Y(), high.Y()}) {
        for (const double z : {low.Z(), high.Z()}) {
          front = std::max(front, gp_Vec(frame.origin, gp_Pnt(x, y, z)).Dot(normal));
        }
      }
    }
    if (std::abs(front) < 0.05) {
      front = 0.0; // no "-0.0"
    }
    reach << QStringLiteral("%1 %2").arg(QString::fromStdString(body.uid)).arg(front, 0, 'f', 1);
  }
  const QString logged = QStringLiteral("In front of the sketch plane: %1").arg(reach.join(QStringLiteral(", ")));
  if (logged != m_loggedReach) {
    m_loggedReach = logged;
    qDebug().noquote() << logged;
  }
}

void MainWindow::showThreads(const std::vector<BodyDisplay>& bodies) {
  // Only thread features and holes make threads.
  const bool threaded =
      std::any_of(m_snapshot.features.cbegin(), m_snapshot.features.cend(), [](const auto& feature) {
        return feature.type == QStringLiteral("thread") || feature.type == QStringLiteral("hole");
      });
  QJsonArray threads;
  if (threaded) {
    try {
      threads = queryArray(QStringLiteral("threads"));
    } catch (const std::exception&) {
      // Drawn again after the next change.
    }
  }
  // A cosmetic thread: rings on the threaded face, one
  // per pitch from where the thread begins to where it ends (a modelled
  // thread is in the geometry).
  TopoDS_Compound rings;
  BRep_Builder builder;
  builder.MakeCompound(rings);
  int shown = 0;
  for (const QJsonValue& value : std::as_const(threads)) {
    const QJsonObject thread = value.toObject();
    const gp_Pnt start = pointOf(thread.value(QStringLiteral("start")));
    const gp_Pnt end = pointOf(thread.value(QStringLiteral("end")));
    const double radius = thread.value(QStringLiteral("radius")).toDouble();
    const double pitch = thread.value(QStringLiteral("pitch")).toDouble();
    const double length = start.Distance(end);
    if (thread.value(QStringLiteral("modeled")).toBool() || !(radius > 0.0) || !(pitch > 0.0) ||
        length < Precision::Confusion()) {
      continue;
    }
    const gp_Dir axis(gp_Vec(start, end));
    const int count = std::min(500, static_cast<int>(length / pitch) + 1);
    const QString body = thread.value(QStringLiteral("body")).toString();
    for (const BodyDisplay& shownBody : bodies) {
      if (QString::fromStdString(shownBody.uid) != body) {
        continue;
      }
      for (int i = 0; i < count; ++i) {
        const gp_Pnt center = start.Translated(gp_Vec(axis) * (i * pitch));
        const TopoDS_Edge ring = BRepBuilderAPI_MakeEdge(gp_Circ(gp_Ax2(center, axis), radius)).Edge();
        builder.Add(rings, ring.Moved(shownBody.placement));
      }
      ++shown;
    }
  }
  m_viewer->setThreads(shown > 0 ? TopoDS_Shape(rings) : TopoDS_Shape());
  const QString logged = QStringLiteral("Cosmetic threads shown: %1").arg(shown);
  if (logged != m_loggedThreads) {
    m_loggedThreads = logged;
    qDebug().noquote() << logged;
  }
}

void MainWindow::flipSection() {
  const QJsonObject shown = m_snapshot.shownAnalysis();
  if (shown.isEmpty() || !canChangeModel()) {
    return;
  }
  // Its definition with the other side.
  QJsonObject def = shown;
  for (const char* key : {"name", "visible", "section", "error"}) {
    def.remove(QLatin1String(key));
  }
  def.insert(QStringLiteral("flip"), !shown.value(QStringLiteral("flip")).toBool());
  runModelCommand({{QStringLiteral("cmd"), QStringLiteral("edit_analysis")},
                   {QStringLiteral("name"), shown.value(QStringLiteral("name"))},
                   {QStringLiteral("def"), def}});
}

void MainWindow::removeSection() {
  const QString name = m_snapshot.shownAnalysis().value(QStringLiteral("name")).toString();
  if (name.isEmpty() || !canChangeModel()) {
    return;
  }
  // Hidden, not deleted: it stays in the browser's Analysis folder.
  runModelCommand({{QStringLiteral("cmd"), QStringLiteral("set_analysis_visible")},
                   {QStringLiteral("name"), name},
                   {QStringLiteral("visible"), false}});
}

TopoDS_Shape MainWindow::analysisShape(const QJsonObject& request) const {
  try {
    return occtShape(idleDocument().analysis_shape(rustStr(compactJson(request))));
  } catch (const std::exception& e) {
    qWarning().noquote() << "Analysis shape not made:" << errorText(e);
    return TopoDS_Shape();
  }
}

bool MainWindow::takesFeatures() const {
  return m_session && m_session->activeTakes(SelectKind::Feature);
}

QJsonValue MainWindow::query(const QJsonObject& request) const {
  // While the actions are updated, their availability checks ask the same
  // queries (bodies, profiles) many times of a document that does not
  // change meanwhile: each is answered once (updateActions).
  QString key;
  if (m_queryMemo) {
    key = QString::fromUtf8(compactJson(request));
    const auto memo = m_queryMemo->constFind(key);
    if (memo != m_queryMemo->cend()) {
      return memo.value();
    }
  }
  const QJsonDocument answer = parseJson(idleDocument().query(rustStr(compactJson(request))));
  const QJsonValue value = answer.isArray() ? QJsonValue(answer.array()) : QJsonValue(answer.object());
  if (m_queryMemo) {
    m_queryMemo->insert(key, value);
  }
  return value;
}

int MainWindow::undoDepth() const {
  return query({{QStringLiteral("query"), QStringLiteral("document")}})
      .toObject()
      .value(QStringLiteral("undo_depth"))
      .toInt();
}

std::shared_ptr<geometry::Shape> MainWindow::bodyShape(const QString& uid) const {
  return idleDocument().body_shape(utf8(uid));
}

void MainWindow::clearModelPreview() {
  // Computes nothing: no job (and none may run now).
  if (modelBusy()) {
    throw ModelBusy();
  }
  m_document->clear_preview();
}

std::shared_ptr<geometry::Shape> MainWindow::previewBodyShape(const std::string& uid) const {
  return idleDocument().preview_body_shape(uid);
}

std::shared_ptr<geometry::Shape> MainWindow::previewToolShape() const {
  return idleDocument().preview_tool_shape();
}

QStringList MainWindow::bodyUids() const {
  QStringList uids;
  for (const QJsonValue& value : queryArray(QStringLiteral("bodies"))) {
    uids << value.toObject().value(QStringLiteral("uid")).toString();
  }
  return uids;
}

bool MainWindow::reportRecompute(const QJsonObject& result, double milliseconds) {
  const QString error = result.value(QStringLiteral("error")).toString();
  if (!error.isEmpty()) {
    showError(tr("Recompute failed: %1").arg(error));
    return false;
  }
  const int recomputed = result.value(QStringLiteral("recomputed")).toInt();
  setErrorStyleSheet(m_statusLabel, QString());
  m_statusLabel->setText(tr("Recomputed %n feature(s) in %1 ms", nullptr, recomputed)
                             .arg(milliseconds, 0, 'f', 1));
  qDebug().noquote() << m_statusLabel->text();
  return true;
}

TopoDS_Shape MainWindow::itemShape(const SelectionItem& item) const {
  if (!item.occurrence.isEmpty()) {
    // In the component's coordinates, placed by the occurrence.
    if (!m_placements.contains(item.occurrence)) {
      return TopoDS_Shape(); // the occurrence is gone
    }
    if (item.kind != SelectKind::Component) {
      SelectionItem own = item;
      own.occurrence.clear();
      const TopoDS_Shape shape = itemShape(own);
      return shape.IsNull() ? shape : shape.Moved(placementOf(item.occurrence));
    }
  }
  try {
    switch (item.kind) {
    case SelectKind::Component: {
      // The bodies the occurrence places, its sub-occurrences' too.
      TopoDS_Compound compound;
      BRep_Builder builder;
      builder.MakeCompound(compound);
      for (const BodyDisplay& body : modelBodies()) {
        const QString path = QString::fromStdString(body.occurrence);
        if (path == item.occurrence || path.startsWith(item.occurrence + QLatin1Char('/'))) {
          builder.Add(compound, body.shape->occt().Moved(body.placement));
        }
      }
      return compound;
    }
    case SelectKind::Sketch: {
      const QJsonObject sketch = queryObject(
          {{QStringLiteral("query"), QStringLiteral("sketch")}, {QStringLiteral("uid"), item.owner}});
      TopoDS_Compound compound;
      BRep_Builder builder;
      builder.MakeCompound(compound);
      for (const SketchEntityDisplay& entity : sketchEntities(sketch)) {
        if (!entity.shape.IsNull()) {
          builder.Add(compound, entity.shape);
        }
      }
      return compound;
    }
    case SelectKind::Face:
    case SelectKind::Edge:
    case SelectKind::Vertex: {
      const auto shape = bodyShape(item.owner);
      if (!shape) {
        return TopoDS_Shape();
      }
      const std::string name = utf8(item.name);
      const std::vector<int> found = item.kind == SelectKind::Face   ? shape->find_faces(name)
                                     : item.kind == SelectKind::Edge ? shape->find_edges(name)
                                                                     : shape->find_vertices(name);
      if (found.empty()) {
        return TopoDS_Shape();
      }
      TopoDS_Compound compound;
      BRep_Builder builder;
      builder.MakeCompound(compound);
      for (const int index : found) {
        builder.Add(compound, item.kind == SelectKind::Face   ? TopoDS_Shape(shape->face(index))
                              : item.kind == SelectKind::Edge ? TopoDS_Shape(shape->edge(index))
                                                              : TopoDS_Shape(shape->vertex(index)));
      }
      return compound;
    }
    case SelectKind::Body:
      return occtShape(bodyShape(item.owner));
    case SelectKind::Feature: {
      // The faces the feature made, on the bodies at the marker.
      TopoDS_Compound compound;
      BRep_Builder builder;
      builder.MakeCompound(compound);
      const std::string prefix = utf8(item.owner) + ":";
      for (const QString& uid : bodyUids()) {
        const auto shape = bodyShape(uid);
        if (!shape) {
          continue;
        }
        for (int i = 0; i < shape->face_count(); ++i) {
          for (const std::string& name : shape->face_names(i)) {
            if (name.rfind(prefix, 0) == 0) {
              builder.Add(compound, shape->face(i));
              break;
            }
          }
        }
      }
      return compound;
    }
    case SelectKind::Profile:
      return occtShape(idleDocument().profile_shape(utf8(item.owner), utf8(item.name)));
    case SelectKind::SketchCurve:
    case SelectKind::SketchPoint: {
      const QJsonObject sketch = queryObject(
          {{QStringLiteral("query"), QStringLiteral("sketch")}, {QStringLiteral("uid"), item.owner}});
      for (const SketchEntityDisplay& entity : sketchEntities(sketch)) {
        if (entity.id == item.name) {
          return entity.shape;
        }
      }
      return TopoDS_Shape();
    }
    case SelectKind::Plane:
    case SelectKind::Axis:
    case SelectKind::Point:
      return datumShape(queryObject({{QStringLiteral("query"), QStringLiteral("datum")},
                                     {QStringLiteral("uid"), item.owner}}),
                        kDatumSize);
    default:
      return TopoDS_Shape();
    }
  } catch (const std::exception&) {
    return TopoDS_Shape(); // gone from the model
  }
}

// ---------------------------------------------------------------------------
// Model and panels

void MainWindow::recompute() {
  // A job: a busy cursor and then the progress dialog when it takes long.
  runCommand({{QStringLiteral("cmd"), QStringLiteral("recompute")}});
  refreshScene();
}

void MainWindow::refreshScene() {
  ScopedTiming total("refresh");
  ScopedTiming snapshot("refresh snapshot");
  try {
    m_snapshot = DocumentSnapshot::read(*this);
  } catch (const std::exception& e) {
    qWarning().noquote() << "Document structure not read:" << errorText(e);
  }
  snapshot.finish();
  // The sketch being edited went with another change, such as its
  // component deleted (mitcad#99): sketch mode ends instead of editing
  // what is no longer there.
  if (m_mode == Mode::Sketch && m_sketch->isActive() && m_snapshot.feature(m_sketch->uid()) == nullptr) {
    qDebug().noquote() << QStringLiteral("Sketch %1 is gone; sketch mode ends").arg(m_sketch->uid());
    if (m_session) {
      m_session->discard();
    }
    leaveSketchMode();
  }
  // The section of the analysis shown (mitcad#41).
  followAnalyses();
  // The Origin folder and Isolate are the document's (P9).
  m_originShown = m_snapshot.originShown;
  m_isolation = m_snapshot.isolation;
  // The document's home view (U5), if it has one.
  std::optional<CameraState> home;
  for (const QJsonValue& view : std::as_const(m_snapshot.namedViews)) {
    if (view.toObject().value(QStringLiteral("name")).toString() == QStringLiteral("Home")) {
      home = ViewController::cameraOf(view.toObject());
    }
  }
  m_viewer->setHome(home);
  // The rendered view's light, background, ground and film (mitcad#47).
  m_viewController->setDocumentName(m_filePath.isEmpty() ? QString() : QFileInfo(m_filePath).completeBaseName());
  try {
    m_viewController->setRenderSettings(
        queryObject({{QStringLiteral("query"), QStringLiteral("render_settings")}}),
        m_filePath.isEmpty() ? QString() : QFileInfo(m_filePath).absolutePath());
  } catch (const std::exception& e) {
    qWarning().noquote() << "Render settings not read:" << errorText(e);
  }
  m_placements.clear();
  std::function<void(const QVector<DocumentSnapshot::Occurrence>&)> place =
      [this, &place](const QVector<DocumentSnapshot::Occurrence>& occurrences) {
        for (const DocumentSnapshot::Occurrence& occurrence : occurrences) {
          m_placements.insert(occurrence.path, trsfOf(occurrence.world));
          place(occurrence.children);
        }
      };
  place(m_snapshot.occurrences);

  ScopedTiming bodies("refresh bodies");
  showBodies();
  bodies.finish();
  ScopedTiming sketches("refresh sketches");
  ScopedTiming profiles("refresh profiles");
  showProfiles();
  profiles.finish();
  ScopedTiming entities("refresh sketch entities");
  showSketches();
  entities.finish();
  ScopedTiming datums("refresh datums");
  showDatums();
  datums.finish();
  if (m_sketch->isActive()) {
    m_sketch->reload();
  }
  sketches.finish();

  ScopedTiming browser("refresh browser");
  m_controller->refresh(m_snapshot);
  browser.finish();
  ScopedTiming actions("refresh actions");
  if (m_mode != Mode::Command) {
    // What was selected may be gone.
    Selection kept;
    for (const SelectionItem& item : m_selection) {
      if (keepSelected(item)) {
        kept.append(item);
      }
    }
    m_selection = kept;
    updateIdleView();
  }
  updateActions();
}

bool MainWindow::keepSelected(const SelectionItem& item) const {
  if (item.kind == SelectKind::SketchConstraint || item.kind == SelectKind::SketchDimension) {
    if (!m_sketch->isActive() || item.owner != m_sketch->uid()) {
      return false;
    }
    const sketch::SketchModel& model = m_sketch->model();
    if (item.kind == SelectKind::SketchDimension) {
      return model.dimension(item.name) != nullptr;
    }
    if (item.name.startsWith(QStringLiteral("fix:"))) {
      const QString entity = item.name.mid(4);
      const sketch::PointData* p = model.point(entity);
      const sketch::CurveData* c = model.curve(entity);
      return (p != nullptr && p->fixed) || (c != nullptr && c->fixed);
    }
    return model.constraint(item.name) != nullptr;
  }
  if (item.kind == SelectKind::Feature) {
    // A feature selected on the timeline (mitcad#28) while it has faces on
    // the bodies at the marker.
    return TopExp_Explorer(itemShape(item), TopAbs_FACE).More();
  }
  return !itemShape(item).IsNull();
}

std::vector<BodyDisplay> MainWindow::modelBodies() const {
  // Every visible body as its occurrences place it (components, F6), with
  // the occurrence, so that picks name the body's own faces and edges; in
  // its appearance's colour, and cut by a section analysis' plane or the
  // sketch's (Hide Above Sketch).
  const QJsonValue cut = cutPlane();
  // Each body's appearance (mitcad#46); an unknown id (an imported one)
  // shows the default look. Textures with the image files the renderer
  // reads (mitcad#53).
  QHash<QString, Appearance> known = appearancesById(appearancesOf(queryArray(QStringLiteral("appearances"))));
  const QString folder = documentFolder();
  for (Appearance& appearance : known) {
    if (appearance.hasTexture) {
      const QString id = appearance.id;
      resolveTexture(appearance.texture, folder, [this, id] {
        try {
          return QByteArray::fromBase64(queryObject({{QStringLiteral("query"), QStringLiteral("appearance_image")},
                                                     {QStringLiteral("id"), id}})
                                            .value(QStringLiteral("data"))
                                            .toString()
                                            .toLatin1());
        } catch (const std::exception&) {
          return QByteArray();
        }
      });
    }
  }
  QHash<QString, Appearance> looks;
  // Faces with appearances of their own (mitcad#53): the names they find
  // at the marker, by appearance.
  QHash<QString, QVector<QPair<QStringList, QString>>> faceAppearances;
  for (const QJsonValue& value : queryArray(QStringLiteral("bodies"))) {
    const QJsonObject body = value.toObject();
    const QString uid = body.value(QStringLiteral("uid")).toString();
    const auto appearance = known.constFind(body.value(QStringLiteral("appearance")).toString());
    if (appearance != known.cend()) {
      looks.insert(uid, appearance.value());
    }
    for (const QJsonValue& face : body.value(QStringLiteral("face_appearances")).toArray()) {
      QStringList names;
      for (const QJsonValue& name : face.toObject().value(QStringLiteral("faces")).toArray()) {
        names << name.toString();
      }
      faceAppearances[uid].append({names, face.toObject().value(QStringLiteral("appearance")).toString()});
    }
  }
  // The faces of a shown shape by their names, grouped by appearance (a
  // face named twice takes the later one).
  const auto faceLooksOf = [&](const QString& uid, const geometry::Shape& shape) {
    std::vector<BodyDisplay::FaceLook> faceLooks;
    std::map<int, QString> byFace;
    for (const auto& [names, id] : faceAppearances.value(uid)) {
      for (const QString& name : names) {
        for (const int face : shape.find_faces(utf8(name))) {
          byFace[face] = id;
        }
      }
    }
    std::map<QString, std::vector<int>> byAppearance;
    for (const auto& [face, id] : byFace) {
      byAppearance[id].push_back(face);
    }
    for (const auto& [id, faces] : byAppearance) {
      BodyDisplay::FaceLook look;
      look.faces = faces;
      // An unknown id: the default look.
      if (const auto appearance = known.constFind(id); appearance != known.cend()) {
        look.appearance = appearance.value();
        look.color = appearance->displayColor;
      }
      faceLooks.push_back(std::move(look));
    }
    return faceLooks;
  };
  std::vector<BodyDisplay> bodies;
  for (const QJsonValue& value : queryArray(QStringLiteral("instances"))) {
    const QJsonObject instance = value.toObject();
    const QString uid = instance.value(QStringLiteral("body")).toString();
    const QString occurrence = pathOf(instance.value(QStringLiteral("path")));
    if (!isIsolated(uid, occurrence)) {
      continue;
    }
    auto shape = idleDocument().body_shape(utf8(uid));
    if (!shape) {
      continue;
    }
    if (!cut.isNull()) {
      try {
        const auto clipped = idleDocument().analysis_shape(
            rustStr(compactJson({{QStringLiteral("shape"), QStringLiteral("clip")},
                                 {QStringLiteral("plane"), cut},
                                 {QStringLiteral("body"), uid}})));
        if (clipped) {
          shape = clipped;
        }
      } catch (const std::exception&) {
        // A body the plane does not cut is shown whole.
      }
    }
    const QJsonArray rows = instance.value(QStringLiteral("transform")).toArray();
    BodyDisplay display;
    display.uid = utf8(uid);
    display.shape = shape;
    if (const auto look = looks.constFind(uid); look != looks.cend()) {
      display.color = look->displayColor;
      display.appearance = look.value();
    }
    if (faceAppearances.contains(uid)) {
      display.faceLooks = faceLooksOf(uid, *shape);
    }
    if (!isIdentity(rows)) {
      display.placement = TopLoc_Location(trsfOf(rows));
    }
    display.occurrence = utf8(occurrence);
    bodies.push_back(display);
  }
  return bodies;
}

QString MainWindow::documentFolder() const {
  return m_filePath.isEmpty() ? QString() : QFileInfo(m_filePath).absolutePath();
}

Selection MainWindow::appearanceTargets() const {
  // Faces get appearances of their own (mitcad#53); bodies, edges and
  // vertices stand for their bodies.
  Selection targets;
  for (const SelectionItem& item : m_selection) {
    const bool onBody = item.kind == SelectKind::Body || item.kind == SelectKind::Face ||
                        item.kind == SelectKind::Edge || item.kind == SelectKind::Vertex;
    if (!onBody || item.owner.isEmpty()) {
      continue;
    }
    SelectionItem target = item;
    if (item.kind != SelectKind::Face) {
      target = SelectionItem{SelectKind::Body, item.owner, QString(), QString()};
    }
    target.occurrence.clear();
    target.at.reset();
    if (!targets.contains(target)) {
      targets << target;
    }
  }
  return targets;
}

bool MainWindow::isIsolated(const QString& body, const QString& occurrence) const {
  if (m_isolation.isEmpty()) {
    return true;
  }
  return std::any_of(m_isolation.begin(), m_isolation.end(), [&](const SelectionItem& item) {
    if (item.kind == SelectKind::Body) {
      return item.owner == body && item.occurrence == occurrence;
    }
    return item.kind == SelectKind::Component &&
           (occurrence == item.occurrence || occurrence.startsWith(item.occurrence + QLatin1Char('/')));
  });
}

TopLoc_Location MainWindow::placementOf(const QString& occurrence) const {
  const auto placement = m_placements.constFind(occurrence);
  return placement == m_placements.cend() ? TopLoc_Location() : TopLoc_Location(placement.value());
}

void MainWindow::showBodies() {
  ScopedTiming instances("bodies instances");
  const std::vector<BodyDisplay> bodies = modelBodies();
  instances.finish();
  m_viewer->setBodies(bodies);
  showSectionCaps(bodies);
  logSketchReach(bodies);
  ScopedTiming threads("bodies threads");
  showThreads(bodies);
  threads.finish();
  // What is shown, and the middle of each body for UI tests to click on
  // (logged by logBodyPlaces as the view moves).
  QStringList shown;
  m_bodyCenters.clear();
  QHash<const void*, gp_Pnt> centers; // of the shapes, kept for the next time
  for (const BodyDisplay& body : bodies) {
    const QString uid = QString::fromStdString(body.uid);
    const QString occurrence = QString::fromStdString(body.occurrence);
    const QString label = occurrence.isEmpty() ? uid : QStringLiteral("%1 in %2").arg(uid, occurrence);
    shown << label;
    const TopoDS_Shape& shape = body.shape->occt();
    const void* key = shape.TShape().get();
    if (!m_localCenters.contains(key)) {
      Bnd_Box box;
      BRepBndLib::Add(shape, box);
      if (box.IsVoid()) {
        continue;
      }
      m_localCenters.insert(key, gp_Pnt((box.CornerMin().XYZ() + box.CornerMax().XYZ()) / 2.0));
    }
    const gp_Pnt center = m_localCenters.value(key);
    centers.insert(key, center);
    m_bodyCenters.append({label, center.Transformed(body.placement.Transformation())});
  }
  m_localCenters = centers;
  const QString logged = shown.join(QStringLiteral(", "));
  if (logged != m_loggedBodies) {
    m_loggedBodies = logged;
    qDebug().noquote() << QStringLiteral("Bodies shown: %1")
                              .arg(logged.isEmpty() ? QStringLiteral("none") : logged);
  }
  logBodyPlaces();
}

void MainWindow::logBodyPlaces() {
  if (!m_viewer->isVisible()) {
    return;
  }
  QStringList places;
  for (const auto& [label, center] : std::as_const(m_bodyCenters)) {
    const QPoint at = m_viewer->mapTo(this, m_viewer->toWidget(center));
    places << QStringLiteral("Body %1 at %2,%3").arg(label).arg(at.x()).arg(at.y());
  }
  const QString logged = places.join(QLatin1Char('\n'));
  if (logged != m_loggedBodyPlaces) {
    m_loggedBodyPlaces = logged;
    for (const QString& line : std::as_const(places)) {
      qDebug().noquote() << line;
    }
  }
}

// A sketch's profiles are hidden once they have been extruded; the
// sketch being edited shows all of them (unless the palette says not to),
// and so does one the user showed. Hidden sketches show none.
void MainWindow::showProfiles() {
  const QString active = m_sketch->isActive() ? m_sketch->uid() : QString();
  std::vector<std::pair<ProfileKey, TopoDS_Shape>> profiles;
  std::map<std::string, QString> occurrences; // of sketches of placed components
  QHash<QString, ProfileFace> faces;
  const QJsonArray all =
      query({{QStringLiteral("query"), QStringLiteral("profiles")}, {QStringLiteral("hashes"), true}}).toArray();
  for (const QJsonValue& value : all) {
    const QJsonObject profile = value.toObject();
    const QString sketch = profile.value(QStringLiteral("sketch")).toString();
    if (sketch == active) {
      if (!m_sketch->showProfiles()) {
        continue;
      }
    } else {
      const DocumentSnapshot::Feature* feature = m_snapshot.feature(sketch);
      const bool shownByUser = feature != nullptr && feature->visibleSet && feature->visible;
      if (!m_snapshot.isShown(sketch) ||
          (!shownByUser && profile.value(QStringLiteral("consumed")).toBool())) {
        continue;
      }
    }
    const std::string region = utf8(profile.value(QStringLiteral("region")).toString());
    const QString occurrence = placingOccurrence(sketch);
    if (!occurrence.isEmpty()) {
      if (!m_snapshot.isOccurrenceShown(occurrence)) {
        continue; // the component's occurrence is hidden
      }
      occurrences[utf8(sketch)] = occurrence;
    }
    // A face built before is used again while the region's geometry and
    // place (the model's hash) and its occurrence's placement stay, so the
    // view keeps its display of it.
    const QString key = sketch + QLatin1Char('|') + QString::fromStdString(region);
    const QString hash = profile.value(QStringLiteral("hash")).toString();
    const gp_Trsf placement = placementOf(occurrence).Transformation();
    const auto cached = m_profileFaces.constFind(key);
    TopoDS_Shape face;
    if (cached != m_profileFaces.cend() && !hash.isEmpty() && cached->hash == hash &&
        sameTransformation(cached->placement, placement)) {
      face = cached->face;
    } else {
      // Null when OCCT cannot build it, e.g. 1e-9 mm from a hand-edited file.
      face = occtShape(idleDocument().profile_shape(utf8(sketch), region));
      if (face.IsNull()) {
        qWarning().noquote() << "Profile not shown:" << QString::fromStdString(region);
        continue;
      }
      if (!occurrence.isEmpty()) {
        face = face.Moved(TopLoc_Location(placement));
      }
    }
    faces.insert(key, {hash, placement, face});
    profiles.emplace_back(ProfileKey{utf8(sketch), region}, face);
  }
  m_profileFaces = faces;
  m_viewer->setProfiles(profiles, occurrences);
}

QString MainWindow::placingOccurrence(const QString& feature) const {
  if (m_sketch->isActive() && feature == m_sketch->uid()) {
    return QString(); // sketch mode works in the sketch's own coordinates
  }
  const DocumentSnapshot::Feature* entry = m_snapshot.feature(feature);
  return entry != nullptr ? m_snapshot.firstOccurrence(entry->component) : QString();
}

// The shown sketches (the browser's light bulbs, else the default rule:
// DocumentSnapshot::featureShown), and the one being edited.
void MainWindow::showSketches() {
  QStringList sketches;
  if (m_sketch->isActive()) {
    sketches << m_sketch->uid();
  }
  for (const DocumentSnapshot::Feature& feature : std::as_const(m_snapshot.features)) {
    if (feature.isSketch() && feature.succeeded() &&
        m_snapshot.isShown(feature.uid) && !sketches.contains(feature.uid)) {
      sketches << feature.uid;
    }
  }
  std::vector<SketchEntityDisplay> entities;
  QStringList shown;
  for (const QString& sketch : sketches) {
    try {
      std::vector<SketchEntityDisplay> some = sketchEntities(queryObject(
          {{QStringLiteral("query"), QStringLiteral("sketch")}, {QStringLiteral("uid"), sketch}}));
      if (!m_sketch->isActive() || sketch != m_sketch->uid()) {
        // Only the sketch being edited shows how constrained it is.
        for (SketchEntityDisplay& entity : some) {
          entity.constrained = false;
        }
      }
      // A component's sketch where its occurrence places it.
      const QString occurrence = placingOccurrence(sketch);
      QString placed;
      if (!occurrence.isEmpty()) {
        if (!m_snapshot.isOccurrenceShown(occurrence)) {
          continue; // the component's occurrence is hidden
        }
        const TopLoc_Location placement = placementOf(occurrence);
        for (SketchEntityDisplay& entity : some) {
          entity.shape = entity.shape.Moved(placement);
          entity.occurrence = occurrence;
        }
        const gp_Trsf& trsf = placement.Transformation();
        for (int row = 1; row <= 3; ++row) {
          for (int column = 1; column <= 4; ++column) {
            placed += QStringLiteral(" %1").arg(trsf.Value(row, column), 0, 'g', 17);
          }
        }
      }
      // The signature also has how the entity is drawn and placed.
      for (SketchEntityDisplay& entity : some) {
        entity.signature += QStringLiteral("|%1|%2").arg(entity.constrained ? 1 : 0).arg(placed);
      }
      entities.insert(entities.end(), some.begin(), some.end());
      shown << sketch;
    } catch (const std::exception&) {
      // A sketch that was just taken back.
    }
  }
  m_viewer->setSketchEntities(entities);
  m_sketchDisplay = std::move(entities);
  // For UI tests: whose curves the view shows, when that changes.
  const QString logged = shown.join(QStringLiteral(", "));
  if (logged != m_loggedSketches) {
    m_loggedSketches = logged;
    qDebug().noquote() << QStringLiteral("Sketches shown: %1")
                              .arg(logged.isEmpty() ? QStringLiteral("none") : logged);
  }
}

void MainWindow::showDatums() {
  std::vector<DatumDisplay> datums;
  ScopedTiming timing("datums query");
  const QJsonArray all =
      query({{QStringLiteral("query"), QStringLiteral("datums")}, {QStringLiteral("origin"), true}})
          .toArray();
  timing.finish();
  // The origin while an input takes construction geometry or its light
  // bulb is on; construction features unless hidden.
  const bool origin = m_originVisible || m_originShown;
  const bool picks = qEnvironmentVariableIsSet("MITCAD_LOG_PICKS");
  QVector<QJsonObject> logged;
  for (const QJsonValue& value : all) {
    const QJsonObject datum = value.toObject();
    const QString uid = datum.value(QStringLiteral("uid")).toString();
    if (kOriginDatums.contains(uid) ? !origin : !m_snapshot.featureShown.value(uid, true)) {
      continue;
    }
    const SelectKind kind = datumKind(datum.value(QStringLiteral("type")).toString());
    DatumDisplay display{uid, kind, datumShape(datum, kDatumSize)};
    // A component's datum where its occurrence places it.
    display.occurrence = kOriginDatums.contains(uid) ? QString() : placingOccurrence(uid);
    if (!display.occurrence.isEmpty()) {
      if (!m_snapshot.isOccurrenceShown(display.occurrence)) {
        continue; // the component's occurrence is hidden
      }
      if (!display.shape.IsNull()) {
        display.shape = display.shape.Moved(placementOf(display.occurrence));
      }
    }
    datums.push_back(display);
    if (kOriginDatums.contains(uid) ? origin : (picks && display.occurrence.isEmpty())) {
      logged.append(datum);
    }
  }
  m_viewer->setDatums(datums);

  // Where a click picks each datum (the origin's while they are shown;
  // with MITCAD_LOG_PICKS also the root's construction features, at the
  // first place tried where a click would pick the datum now), for UI
  // tests.
  for (const QJsonObject& datum : std::as_const(logged)) {
    const QString uid = datum.value(QStringLiteral("uid")).toString();
    const SelectKind kind = datumKind(datum.value(QStringLiteral("type")).toString());
    std::vector<gp_Pnt> candidates;
    if (kind == SelectKind::Plane && !kOriginDatums.contains(uid)) {
      // A construction plane's square, away from its middle.
      const gp_Pnt middle = pointOf(datum.value(QStringLiteral("origin")));
      const gp_Vec x = vectorOf(datum.value(QStringLiteral("x_axis")));
      const gp_Vec y = vectorOf(datum.value(QStringLiteral("y_axis")));
      for (const double fraction : {0.3, 0.2, 0.1}) {
        for (const gp_Vec& direction : {x + y, x - y, y - x, (x + y).Reversed()}) {
          candidates.push_back(middle.Translated(direction * (kDatumSize * fraction)));
        }
      }
    } else if (kind == SelectKind::Plane) {
      candidates = m_viewer->planePickCandidates(pointOf(datum.value(QStringLiteral("origin"))),
                                                 gp_Dir(vectorOf(datum.value(QStringLiteral("normal")))),
                                                 kDatumSize);
      candidates.insert(candidates.begin(),
                        m_viewer->planePickPoint(pointOf(datum.value(QStringLiteral("origin"))),
                                                 gp_Dir(vectorOf(datum.value(QStringLiteral("normal")))),
                                                 kDatumSize));
    } else if (kind == SelectKind::Axis) {
      // Along the axis, nearer to its middle or the other way.
      const gp_Pnt middle = pointOf(datum.value(QStringLiteral("origin")));
      const gp_Vec along = vectorOf(datum.value(QStringLiteral("direction")));
      for (const double fraction : {0.4, 0.3, 0.2, 0.12, 0.06}) {
        for (const double sign : {1.0, -1.0}) {
          candidates.push_back(middle.Translated(along * (kDatumSize * fraction * sign)));
        }
      }
    } else {
      candidates.push_back(pointOf(datum.value(QStringLiteral("point"))));
    }
    // The first place that picks the datum where no other datum and no
    // edge lies (another may be selected and highlighted over it later,
    // and an edge a pixel away may win the click), else the first that
    // picks it. Tried at the whole pixel a click there hits.
    gp_Pnt at = candidates.front();
    bool found = false;
    for (const gp_Pnt& candidate : candidates) {
      if (!picks) {
        break;
      }
      const std::vector<SelectionItem> items = m_viewer->itemsAt(QPointF(m_viewer->toWidget(candidate)));
      if (items.empty() || items.front().owner != uid || items.front().kind != kind) {
        continue;
      }
      const bool alone = std::none_of(items.begin() + 1, items.end(), [&uid](const SelectionItem& other) {
        return ((other.kind == SelectKind::Plane || other.kind == SelectKind::Axis ||
                 other.kind == SelectKind::Point) &&
                other.owner != uid) ||
               other.kind == SelectKind::Edge;
      });
      if (!found) {
        at = candidate;
        found = true;
      }
      if (alone) {
        at = candidate;
        break;
      }
    }
    const QPoint point = m_viewer->mapTo(this, m_viewer->toWidget(at));
    qDebug().noquote() << QStringLiteral("Datum %1 at %2,%3").arg(uid).arg(point.x()).arg(point.y());
  }
}

void MainWindow::inputActivated(SelectFilter filter) {
  const bool origin = bool(filter & kConstructionGeometry);
  if (origin != m_originVisible) {
    m_originVisible = origin;
    showDatums();
  }
  if ((filter & (SelectKind::Face | SelectKind::Edge | SelectKind::Vertex | SelectKind::Profile)) &&
      qEnvironmentVariableIsSet("MITCAD_LOG_PICKS")) {
    // Where a click picks each face, edge, vertex and profile the input
    // takes, for UI tests (only on request: every item is tried in the view).
    qDebug().noquote() << "Pick places:";
    for (const auto& [item, at] : m_viewer->pickPoints(filter)) {
      const QPoint point = m_viewer->mapTo(this, at);
      qDebug().noquote() << QStringLiteral("Pick %1 %2/%3 at %4,%5")
                                .arg(SelectionItem::kindName(item.kind), item.owner, item.name)
                                .arg(point.x())
                                .arg(point.y());
    }
  }
  if (filter & (SelectKind::SketchCurve | SelectKind::SketchPoint)) {
    // Where a click picks each sketch curve and point, for UI tests.
    for (const SketchEntityDisplay& entity : m_sketchDisplay) {
      gp_Pnt at;
      if (entity.shape.IsNull()) {
        continue;
      }
      if (entity.shape.ShapeType() == TopAbs_VERTEX) {
        at = BRep_Tool::Pnt(TopoDS::Vertex(entity.shape));
      } else {
        // A text is a compound of its outline edges: its first edge picks it.
        const TopExp_Explorer edges(entity.shape, TopAbs_EDGE);
        if (!edges.More()) {
          continue;
        }
        const BRepAdaptor_Curve curve(TopoDS::Edge(edges.Current()));
        at = curve.Value((curve.FirstParameter() + curve.LastParameter()) / 2.0);
      }
      const QPoint point = m_viewer->mapTo(this, m_viewer->toWidget(at));
      qDebug().noquote() << QStringLiteral("Sketch entity %1/%2 at %3,%4")
                                .arg(entity.sketch, entity.id)
                                .arg(point.x())
                                .arg(point.y());
    }
  }
}

bool MainWindow::setParameterValue(const QString& name, double value) {
  bool known = false;
  for (const QJsonValue& parameter : queryArray(QStringLiteral("parameters"))) {
    known = known || parameter.toObject().value(QStringLiteral("name")).toString() == name;
  }
  if (!known) {
    return false;
  }
  return runModelCommand({{QStringLiteral("cmd"), QStringLiteral("set_parameter")},
                          {QStringLiteral("name"), name},
                          {QStringLiteral("value"), value}});
}

// ---------------------------------------------------------------------------
// The browser, the timeline and the parameters dialog (DocumentHost)

bool MainWindow::runModelCommand(const QJsonObject& command, QJsonObject* result) {
  if (modelBusy()) {
    return runCommand(command, result); // refused: the model is a job's
  }
  const QJsonObject before = bodyVolumes(*this);
  const bool ok = runCommand(command, result);
  if (ok) {
    logVolumeChanges(*this, before);
  }
  refreshScene();
  return ok;
}

bool MainWindow::runModelCommands(const QJsonArray& commands, const QString& label) {
  if (modelBusy()) {
    return commands.isEmpty() || runCommand(commands.first().toObject()); // refused
  }
  const QJsonObject before = bodyVolumes(*this);
  const int depth = undoDepth();
  for (const QJsonValue& value : commands) {
    if (!runCommand(value.toObject())) {
      // A cancel takes back the whole step too: the model is as it was.
      const QString error = m_lastError;
      const bool cancelled = m_lastCancelled;
      for (int guard = 0; guard < commands.size() && undoDepth() > depth; ++guard) {
        runCommand({{QStringLiteral("cmd"), QStringLiteral("undo")}});
      }
      m_lastError = error;
      m_lastCancelled = cancelled;
      refreshScene();
      if (cancelled) {
        showHint(tr("Computation cancelled."));
      } else {
        showError(error);
      }
      return false;
    }
  }
  if (undoDepth() > depth + 1) {
    runCommand({{QStringLiteral("cmd"), QStringLiteral("merge_undo")},
                {QStringLiteral("depth"), depth},
                {QStringLiteral("label"), label}});
  }
  logVolumeChanges(*this, before);
  refreshScene();
  return true;
}

bool MainWindow::canChangeModel(bool visibilityOnly) const {
  if (!visibilityOnly && windowReadOnly()) {
    return false; // mitcad#89; what shows changes in the window's own state
  }
  return m_mode == Mode::Idle || (visibilityOnly && m_mode == Mode::Sketch);
}

void MainWindow::showStatus(const QString& message, bool error) {
  if (error) {
    showError(message);
  } else {
    showHint(message);
  }
}

void MainWindow::createSketchOn(const SelectionItem& plane) {
  if (m_mode == Mode::Idle && isSketchPlane(plane)) {
    createSketch(sketchPlaneFields(plane));
  }
}

void MainWindow::pickItems(const Selection& items) {
  if (m_session) {
    // A row picked in the browser goes to the command's active input, as
    // a click in the view would.
    for (const SelectionItem& item : items) {
      m_session->picked({item}, Qt::NoModifier, false);
    }
    m_controller->showSelection({});
    return;
  }
  if (m_mode == Mode::PickPlane) {
    for (const SelectionItem& item : items) {
      if (isSketchPlane(item)) {
        qDebug().noquote() << QStringLiteral("Create Sketch picked %1").arg(summarize({item}));
        createSketch(sketchPlaneFields(item));
        return;
      }
    }
    return;
  }
  // Features without faces at the marker (rolled back, consumed, suppressed)
  // are left out.
  Selection kept;
  for (const SelectionItem& item : items) {
    if (item.kind != SelectKind::Feature || keepSelected(item)) {
      kept.append(item);
    }
  }
  setSelection(kept);
}

void MainWindow::lookAt(const SelectionItem& item) {
  try {
    gp_Dir normal;
    gp_Dir up;
    if (item.kind == SelectKind::Plane) {
      const QJsonObject datum = queryObject(
          {{QStringLiteral("query"), QStringLiteral("datum")}, {QStringLiteral("uid"), item.owner}});
      normal = gp_Dir(vectorOf(datum.value(QStringLiteral("normal"))));
      up = gp_Dir(vectorOf(datum.value(QStringLiteral("y_axis"))));
    } else if (item.kind == SelectKind::Face) {
      // A planar face from outside the body, with the up direction nearest
      // the view's.
      SelectionItem own = item;
      own.occurrence.clear();
      const TopoDS_Shape shape = itemShape(own);
      TopExp_Explorer faces(shape, TopAbs_FACE);
      if (!faces.More()) {
        return;
      }
      const TopoDS_Face& face = TopoDS::Face(faces.Current());
      const BRepAdaptor_Surface surface(face);
      if (surface.GetType() != GeomAbs_Plane) {
        return;
      }
      const gp_Pln plane = surface.Plane();
      normal = plane.Axis().Direction();
      if (face.Orientation() == TopAbs_REVERSED) {
        normal.Reverse();
      }
      const gp_Dir viewUp = m_viewer->viewUp();
      const gp_Vec inPlane = gp_Vec(viewUp) - gp_Vec(normal) * viewUp.Dot(normal);
      up = inPlane.Magnitude() > 1e-6 ? gp_Dir(inPlane) : plane.YAxis().Direction();
    } else if (item.kind == SelectKind::Sketch || item.kind == SelectKind::Profile) {
      geometry::Frame frame;
      if (!sketchFrame(queryObject({{QStringLiteral("query"), QStringLiteral("sketch")},
                                    {QStringLiteral("uid"), item.owner}}),
                       frame)) {
        return;
      }
      const gp_Ax3 plane = toAx3(frame);
      normal = plane.Direction();
      up = plane.YDirection();
    } else {
      return;
    }
    if (!item.occurrence.isEmpty()) {
      const gp_Trsf placement = placementOf(item.occurrence).Transformation();
      normal.Transform(placement);
      up.Transform(placement);
    }
    m_viewer->lookAlong(normal, up);
    qDebug().noquote() << QStringLiteral("Look at %1").arg(item.describe());
  } catch (const std::exception& e) {
    showError(errorText(e));
  }
}

void MainWindow::showNamedView(const QString& view) {
  if (view.startsWith(QStringLiteral("named:"))) {
    const QString name = view.mid(6);
    for (const QJsonValue& value : std::as_const(m_snapshot.namedViews)) {
      if (value.toObject().value(QStringLiteral("name")).toString() == name) {
        m_viewer->setCamera(ViewController::cameraOf(value.toObject()));
        qDebug().noquote() << QStringLiteral("View %1").arg(name);
        return;
      }
    }
    return;
  }
  if (view == QStringLiteral("home")) {
    m_viewer->goHome();
    qDebug().noquote() << "View home";
    return;
  }
  m_viewController->showStandardView(view);
}

void MainWindow::saveNamedView(const QString& name, bool replace) {
  m_viewController->saveNamedView(name, replace);
}

void MainWindow::lookAtSelection() {
  if (m_mode == Mode::Sketch) {
    m_sketch->lookAt();
    qDebug().noquote() << QStringLiteral("Look at sketch %1").arg(m_sketch->uid());
    return;
  }
  for (const SelectionItem& item : std::as_const(m_selection)) {
    if (item.kind == SelectKind::Plane || item.kind == SelectKind::Sketch ||
        item.kind == SelectKind::Profile || isSketchPlane(item)) {
      lookAt(item);
      return;
    }
  }
  showHint(tr("Look At: select a plane, a planar face or a sketch first."));
}

// Isolate and the Origin folder's light bulb are saved in the document
// (P9): model commands, undo steps that recompute nothing; refreshScene
// shows what the document then says.
void MainWindow::setIsolation(const Selection& items) {
  if (!canChangeModel(true)) {
    return;
  }
  QJsonArray isolated;
  for (const SelectionItem& item : items) {
    QJsonObject entry;
    if (item.kind == SelectKind::Body) {
      entry.insert(QStringLiteral("body"), item.owner);
    }
    if (!item.occurrence.isEmpty()) {
      entry.insert(QStringLiteral("occurrence"), item.occurrence);
    }
    isolated.append(entry);
  }
  runModelCommand({{QStringLiteral("cmd"), QStringLiteral("set_isolation")}, {QStringLiteral("items"), isolated}});
  updateIdleView();
}

void MainWindow::setOriginShown(bool shown) {
  if (!canChangeModel(true)) {
    return;
  }
  runModelCommand({{QStringLiteral("cmd"), QStringLiteral("set_origin_visible")}, {QStringLiteral("visible"), shown}});
}

void MainWindow::showHint(const QString& message) {
  setErrorStyleSheet(m_statusLabel, QString());
  m_statusLabel->setText(message);
  updateStatusPill(false);
}

void MainWindow::showError(const QString& message) {
  setErrorStyleSheet(m_statusLabel);
  m_statusLabel->setText(message);
  updateStatusPill(true);
  qWarning().noquote() << message;
  // A crash in the geometry kernel is an internal error (mitcad#62).
  if (m_reports != nullptr) {
    m_reports->errorShown(message);
  }
}

// ---------------------------------------------------------------------------
// Selection outside commands

SelectFilter MainWindow::idleFilter() const {
  SelectFilter filter;
  for (auto it = m_filterActions.cbegin(); it != m_filterActions.cend(); ++it) {
    if (it.value()->isChecked()) {
      filter |= SelectFilter::fromInt(it.key());
    }
  }
  return filter;
}

void MainWindow::pick(const Selection& items, Qt::KeyboardModifiers modifiers) {
  onPicked(items, modifiers, false);
}

void MainWindow::onPicked(const Selection& items, Qt::KeyboardModifiers modifiers, bool window) {
  if (modelBusy()) {
    // A pick queued before a job began: it takes effect after the job.
    whenIdle(this, [this, items, modifiers, window] { onPicked(items, modifiers, window); });
    return;
  }
  if (m_session) {
    m_session->picked(items, modifiers, window);
    return;
  }
  if (m_mode == Mode::PickPlane) {
    qDebug().noquote() << QStringLiteral("Create Sketch picked %1").arg(summarize(items));
    for (const SelectionItem& item : items) {
      if (isSketchPlane(item)) {
        createSketch(sketchPlaneFields(item));
        return;
      }
    }
    return;
  }
  if (m_mode == Mode::Sketch && m_sketch->tool() != nullptr) {
    return;
  }
  // A click selects, Ctrl or Shift adds or removes; a window
  // selects what it holds of one kind.
  const bool add = modifiers & (Qt::ControlModifier | Qt::ShiftModifier);
  Selection next = add ? m_selection : Selection();
  if (window) {
    const SelectKind kind = preferredKind(items);
    for (const SelectionItem& item : items) {
      if (item.kind == kind && !next.contains(item)) {
        next.append(item);
      }
    }
  } else if (!items.isEmpty()) {
    const SelectionItem& item = items.first();
    if (add && next.contains(item)) {
      next.removeAll(item);
    } else if (!next.contains(item)) {
      next.append(item);
    }
  }
  setSelection(next);
}

void MainWindow::setSelection(const Selection& items) {
  const bool changed = items != m_selection;
  m_selection = items;
  if (changed) {
    m_locks->activity(); // a selection keeps the edit lock active (mitcad#89)
    QStringList names;
    for (const SelectionItem& item : items) {
      names << item.describe();
    }
    qDebug().noquote() << QStringLiteral("Selected: %1%2").arg(
        summarize(items),
        names.isEmpty() ? QString() : QStringLiteral(" [%1]").arg(names.join(QStringLiteral("; "))));
  }
  updateIdleView();
  m_sketch->refreshView();
  m_controller->showSelection(m_selection);
}

void MainWindow::updateIdleView() {
  if (m_mode == Mode::Command) {
    return;
  }
  m_viewer->setHoverColor(QColor(0x2f, 0x9b, 0xff));
  if (m_mode == Mode::PickPlane) {
    m_viewer->setPickFilter(SelectKind::Plane | SelectKind::Face, isSketchPlane);
    m_viewer->setHighlights({});
    return;
  }
  // A sketch tool takes the clicks; nothing is picked or highlighted. In
  // sketch mode, only the edited sketch's geometry can be picked.
  const bool drawing = m_mode == Mode::Sketch && m_sketch->tool() != nullptr;
  std::function<bool(const SelectionItem&)> test;
  if (m_mode == Mode::Sketch) {
    const QString uid = m_sketch->uid();
    test = [uid](const SelectionItem& item) { return !isSketchKind(item.kind) || item.owner == uid; };
  }
  m_viewer->setPickFilter(drawing ? SelectFilter() : idleFilter(), test);
  std::vector<HighlightDisplay> highlights;
  QStringList featureFaces; // "F4 1": a selected feature's faces (mitcad#28)
  for (const SelectionItem& item : m_selection) {
    if (item.kind != SelectKind::SketchConstraint && item.kind != SelectKind::SketchDimension) {
      const TopoDS_Shape shape = itemShape(item);
      if (item.kind == SelectKind::Feature) {
        NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> faces;
        TopExp::MapShapes(shape, TopAbs_FACE, faces);
        featureFaces << QStringLiteral("%1 %2").arg(item.owner).arg(faces.Extent());
      }
      highlights.push_back({item, shape, kSelectionColor});
    }
  }
  m_viewer->setHighlights(highlights);
  const QString logged = featureFaces.isEmpty() ? QStringLiteral("none") : featureFaces.join(QStringLiteral(", "));
  if (logged != m_loggedFeatureFaces) {
    m_loggedFeatureFaces = logged;
    qDebug().noquote() << QStringLiteral("Feature faces highlighted: %1").arg(logged);
  }

  QString text = tr("<b>No command</b><br>Press S to search for a command, or right-click the "
                    "selection for the commands that fit it.");
  if (!m_selection.isEmpty()) {
    text += tr("<br><br>Selected: %1").arg(summarize(m_selection));
    QStringList lines;
    for (const SelectionItem& item : m_selection.mid(0, 8)) {
      lines << item.describe().toHtmlEscaped();
    }
    text += QStringLiteral("<br><span style='color:gray'>%1</span>").arg(lines.join(QStringLiteral("<br>")));
  }
  if (chromeStyle() == ChromeStyle::Floating) {
    // The card is for what there is to say: the selection, not the hint.
    text = m_selection.isEmpty() ? QString() : text.mid(text.indexOf(QStringLiteral("<br><br>")) + 8);
  }
  m_idleInfo->setText(text);
  updateCommandCard();
}

void MainWindow::showContextMenu(const QPoint& globalPosition, const SelectionItem& item) {
  if (modelBusy()) {
    qDebug() << "Context menu dropped: the model is computing";
    return; // asked for before a job began; a menu after it would surprise
  }
  QMenu menu(this);
  QStringList entries;
  const auto add = [&menu, &entries](QAction* action) {
    menu.addAction(action);
    entries << action->text().remove(QLatin1Char('&'));
  };
  const auto addCall = [&menu, &add](const QString& icon, const QString& text, std::function<void()> call) {
    QAction* action = new QAction(themeIcon(icon), text, &menu);
    connect(action, &QAction::triggered, &menu, std::move(call));
    add(action);
  };
  // The feature and sketch commands whose first input takes everything
  // selected.
  const auto addFitting = [this, &add] {
    for (const auto& def : m_registry->commands()) {
      const InputDef* input = def->primarySelection();
      if (def->kind != CommandDef::Kind::Feature || input == nullptr || !isAvailable(*def)) {
        continue;
      }
      if (std::all_of(m_selection.begin(), m_selection.end(),
                      [input](const SelectionItem& s) { return input->accepts(s); })) {
        add(m_registry->action(def->id));
      }
    }
  };

  if (m_session) {
    addCall(QStringLiteral("ok"), tr("OK"), [this] { confirm(); });
    addCall(QStringLiteral("cancel"), tr("Cancel"), [this] { cancel(); });
  } else if (m_mode == Mode::PickPlane) {
    addCall(QStringLiteral("cancel"), tr("Cancel"), [this] { cancel(); });
  } else if (m_mode == Mode::Sketch) {
    if (m_sketch->tool() != nullptr) {
      addCall(QStringLiteral("ok"), tr("OK"), [this] { confirm(); });
      addCall(QStringLiteral("cancel"), tr("Cancel"), [this] { cancel(); });
    } else {
      if (item.isValid() && !m_selection.contains(item)) {
        setSelection({item});
      }
      const bool sketchItems = std::any_of(m_selection.begin(), m_selection.end(),
                                           [](const SelectionItem& s) { return isSketchKind(s.kind); });
      if (sketchItems) {
        add(m_registry->action(QStringLiteral("sketch.delete")));
        add(m_registry->action(QStringLiteral("sketch.construction")));
        add(m_registry->action(QStringLiteral("sketch.constraint.fix")));
        if (m_selection.size() == 1 && m_selection.first().kind == SelectKind::SketchDimension) {
          const QString id = m_selection.first().name;
          addCall(QStringLiteral("dimension"), tr("Edit Dimension"), [this, id] { m_sketch->editDimension(id); });
        }
        // Texts, patterns and offsets have edit panels (P3, P4).
        if (m_selection.size() == 1) {
          const QString id = m_selection.first().name;
          const sketch::SketchModel& model = m_sketch->model();
          if (model.text(id) != nullptr) {
            add(m_registry->action(QStringLiteral("sketch.edit_text")));
          }
          if (model.patternOf(id) != nullptr) {
            add(m_registry->action(QStringLiteral("sketch.edit_pattern")));
          }
          if (model.offsetOf(id) != nullptr) {
            add(m_registry->action(QStringLiteral("sketch.edit_offset")));
          }
        }
        addFitting();
        menu.addSeparator();
      } else if (!m_selection.isEmpty()) {
        // The model's faces, edges and vertices go into the sketch.
        const CommandDef* project = m_registry->find(QStringLiteral("sketch.project"));
        const InputDef* input = project != nullptr ? project->primarySelection() : nullptr;
        if (input != nullptr && isAvailable(*project) &&
            std::all_of(m_selection.begin(), m_selection.end(),
                        [input](const SelectionItem& s) { return input->accepts(s); })) {
          add(m_registry->action(project->id));
          menu.addSeparator();
        }
      }
      for (const char* id : {"sketch.line", "sketch.rectangle", "sketch.circle", "sketch.dimension", "sketch.trim"}) {
        add(m_registry->action(QString::fromLatin1(id)));
      }
    }
    menu.addSeparator();
    add(m_registry->action(QStringLiteral("sketch.finish")));
    add(m_registry->action(QStringLiteral("edit.undo")));
  } else {
    if (item.isValid() && !m_selection.contains(item)) {
      // A right click on something not selected selects it.
      setSelection({item});
    }
    if (const CommandDef* last = m_registry->find(m_lastCommand); last && isAvailable(*last)) {
      const QString id = last->id;
      addCall(last->icon, tr("Repeat %1").arg(last->name), [this, id] { trigger(id); });
      menu.addSeparator();
    }
    if (!m_selection.isEmpty()) {
      addFitting();
      const SelectionItem& first = m_selection.first();
      if (m_selection.size() == 1 && isSketchPlane(first)) {
        add(m_registry->action(QStringLiteral("sketch.create")));
      }
      const QString feature = first.creatingFeature();
      QString name;
      if (m_selection.size() == 1 && !feature.isEmpty() && editorOf(feature, &name) != nullptr) {
        addCall(QStringLiteral("edit"), tr("Edit %1").arg(name), [this, feature] { editFeature(feature); });
      }
      const QString sketch = sketchOf(first);
      if (m_selection.size() == 1 && !sketch.isEmpty()) {
        addCall(QStringLiteral("sketch"), tr("Edit Sketch"), [this, sketch] { editSketch(sketch); });
        addCall(QStringLiteral("export"), tr("Save As DXF"), [this, sketch] { exportSketch(sketch); });
      }
      const bool onBody = first.kind == SelectKind::Face || first.kind == SelectKind::Edge ||
                          first.kind == SelectKind::Vertex || first.kind == SelectKind::Body;
      // Appearances of the bodies selected or picked on (mitcad#46).
      if (const Selection targets = appearanceTargets(); !targets.isEmpty()) {
        addCall(QStringLiteral("appearance"), tr("Appearance..."),
                [this, targets] { m_controller->openAppearances(targets); });
      }
      // A feature selected as its faces (mitcad#28) has no row of its own.
      if (m_selection.size() == 1 && first.kind != SelectKind::Feature &&
          (onBody || !first.creatingFeature().isEmpty())) {
        addCall(QStringLiteral("browser"), tr("Find in Browser"),
                [this, first] { m_controller->findInBrowser(first); });
      }
      menu.addSeparator();
      addCall(QString(), tr("Clear Selection"), [this] { setSelection({}); });
    }
    add(m_registry->action(QStringLiteral("edit.undo")));
  }
  menu.addSeparator();
  add(m_registry->action(QStringLiteral("view.fit")));
  add(m_registry->action(QStringLiteral("tools.search")));
  qDebug().noquote() << QStringLiteral("Context menu: %1").arg(entries.join(QStringLiteral(" | ")));
  menu.exec(globalPosition);
}

// ---------------------------------------------------------------------------
// Sketch

QString MainWindow::sketchOf(const SelectionItem& item) {
  switch (item.kind) {
  case SelectKind::Profile:
  case SelectKind::SketchCurve:
  case SelectKind::SketchPoint:
  case SelectKind::Sketch:
    return item.owner;
  default:
    return QString();
  }
}

void MainWindow::startSketch() {
  if (m_mode != Mode::Idle || refuseReadOnly(QStringLiteral("sketch.create"))) {
    return;
  }
  // On the selected plane or planar face; else the user is asked for one.
  if (m_selection.size() == 1 && isSketchPlane(m_selection.first())) {
    createSketch(sketchPlaneFields(m_selection.first()));
    return;
  }
  m_mode = Mode::PickPlane;
  setSelection({});
  inputActivated(kConstructionGeometry); // shows the origin planes
  updateIdleView();
  updateActions();
  showHint(tr("Create Sketch: select a plane or a planar face (Esc cancels)."));
  qDebug() << "Create Sketch: select a plane";
}

void MainWindow::createSketch(const QJsonObject& fields) {
  QJsonObject request = fields;
  request.insert(QStringLiteral("cmd"), QStringLiteral("sketch.create"));
  const int depth = undoDepth();
  QJsonObject result;
  if (!runCommand(request, &result)) {
    return; // the reason is shown; a plane can still be picked
  }
  const QString uid = result.value(QStringLiteral("uid")).toString();
  // A sketch whose plane cannot be found where it is added is taken back
  // with the reason (mitcad#99): nothing is left half made, and the
  // command can go on with another plane.
  const QString error = featureError(uid);
  if (!error.isEmpty()) {
    while (undoDepth() > depth) {
      if (!runCommand({{QStringLiteral("cmd"), QStringLiteral("undo")}})) {
        break;
      }
    }
    refreshScene();
    showError(tr("Create Sketch: %1").arg(error));
    qDebug().noquote() << QStringLiteral("Create Sketch taken back: %1").arg(error);
    return;
  }
  m_mode = Mode::Idle;
  enterSketch(uid, false);
  const gp_Pnt origin = m_sketch->model().frame.origin;
  const QJsonValue plane = fields.value(QStringLiteral("plane"));
  const QString on = plane.isString()
                         ? plane.toString()
                         : QString::fromUtf8(QJsonDocument(plane.toObject()).toJson(QJsonDocument::Compact));
  const QString occurrence = fields.value(QStringLiteral("occurrence")).toString();
  qDebug().noquote() << QStringLiteral("Sketch started on %1%2, origin (%3, %4, %5)")
                            .arg(on, occurrence.isEmpty() ? QString() : QStringLiteral(" in ") + occurrence)
                            .arg(origin.X())
                            .arg(origin.Y())
                            .arg(origin.Z());
}

QString MainWindow::featureError(const QString& uid) const {
  const QJsonObject timeline = query({{QStringLiteral("query"), QStringLiteral("timeline")}}).toObject();
  for (const QJsonValue& value : timeline.value(QStringLiteral("features")).toArray()) {
    const QJsonObject feature = value.toObject();
    if (feature.value(QStringLiteral("uid")).toString() == uid &&
        feature.value(QStringLiteral("status")).toString() == QStringLiteral("error")) {
      return feature.value(QStringLiteral("error")).toString();
    }
  }
  return QString();
}

void MainWindow::enterSketch(const QString& uid, bool existing) {
  m_sketchExisting = existing;
  m_mode = Mode::Sketch;
  m_selection.clear();
  inputActivated(SelectFilter()); // hides the origin planes
  m_sketch->enter(uid);
  m_sketchEnterDepth = undoDepth();
  m_viewer->lookAtSketchPlane();
  m_viewer->setGridPlane(toAx3(m_sketch->model().frame)); // the grid on the sketch plane
  m_ribbon->setSketchMode(true);
  m_commandPages->setCurrentWidget(m_palette);
  refreshScene();
  showHint(tr("Sketch: Line (L), Rectangle (R), Circle (C), Sketch Dimension (D); "
              "Finish Sketch with Ctrl+Enter."));
  TestSync::singleShot(300, this, [this] {
    m_ribbon->logLayout();
    m_palette->logLayout();
  });
}

void MainWindow::editSketch(const QString& uid) {
  if (m_mode != Mode::Idle || refuseReadOnly(QStringLiteral("editing %1").arg(uid))) {
    return;
  }
  const QJsonObject timeline = queryObject({{QStringLiteral("query"), QStringLiteral("timeline")}});
  const QJsonArray features = timeline.value(QStringLiteral("features")).toArray();
  const int marker = timeline.value(QStringLiteral("marker")).toInt();
  int position = -1;
  for (int i = 0; i < features.size(); ++i) {
    if (features[i].toObject().value(QStringLiteral("uid")).toString() == uid) {
      position = i;
    }
  }
  if (position < 0) {
    return;
  }
  m_sketchRestoreMarker = -1;
  if (marker != position + 1) {
    // The model as the sketch finds it while it is edited.
    if (!runCommand({{QStringLiteral("cmd"), QStringLiteral("set_marker")},
                     {QStringLiteral("position"), position + 1}})) {
      return;
    }
    m_sketchRestoreMarker = marker;
  }
  enterSketch(uid, true);
  qDebug().noquote() << QStringLiteral("Editing sketch %1 (%2)").arg(uid, m_sketch->model().name);
}

bool MainWindow::sketchExists(const QString& uid) const {
  const QJsonObject timeline =
      query({{QStringLiteral("query"), QStringLiteral("timeline")}}).toObject();
  const QJsonArray features = timeline.value(QStringLiteral("features")).toArray();
  return std::any_of(features.begin(), features.end(), [&uid](const QJsonValue& feature) {
    return feature.toObject().value(QStringLiteral("uid")).toString() == uid;
  });
}

void MainWindow::leaveSketchMode() {
  m_sketch->leave();
  m_mode = Mode::Idle;
  m_viewer->restoreCamera();
  m_viewer->setGridPlane(gp_Ax3()); // back on XY
  m_ribbon->setSketchMode(false);
  m_commandPages->setCurrentWidget(m_idleInfo);
  m_sketchExisting = false;
  m_sketchRestoreMarker = -1;
}

bool MainWindow::finishSketch() {
  if (m_mode != Mode::Sketch) {
    return true;
  }
  const QString uid = m_sketch->uid();
  m_sketch->stopTool();
  const sketch::SketchModel& model = m_sketch->model();
  const QString name = model.name;
  const bool empty = model.points.empty() && model.curves.empty() && model.texts.empty();
  if (empty && !m_sketchExisting && sketchExists(uid)) {
    // An empty sketch is of no use: take back its creation if that was the
    // last command, else delete it.
    const QJsonObject document =
        query({{QStringLiteral("query"), QStringLiteral("document")}}).toObject();
    const bool lastCommand = document.value(QStringLiteral("undo")).toString() ==
                             QStringLiteral("Add ") + featureName(uid);
    runCommand(lastCommand ? QJsonObject{{QStringLiteral("cmd"), QStringLiteral("undo")}}
                           : QJsonObject{{QStringLiteral("cmd"), QStringLiteral("delete_feature")},
                                         {QStringLiteral("uid"), uid}});
  }
  if (m_sketchRestoreMarker >= 0) {
    // The timeline rolls forward again; the edit is one undo step.
    const QJsonObject document =
        query({{QStringLiteral("query"), QStringLiteral("document")}}).toObject();
    bool rolled = false;
    if (document.value(QStringLiteral("undo_depth")).toInt() == m_sketchEnterDepth &&
        document.value(QStringLiteral("undo")).toString() == QStringLiteral("Move Timeline Marker")) {
      rolled = runCommand({{QStringLiteral("cmd"), QStringLiteral("undo")}});
    } else if (runCommand({{QStringLiteral("cmd"), QStringLiteral("set_marker")},
                           {QStringLiteral("position"), m_sketchRestoreMarker}})) {
      rolled = true;
      runCommand({{QStringLiteral("cmd"), QStringLiteral("merge_undo")},
                  {QStringLiteral("depth"), m_sketchEnterDepth - 1},
                  {QStringLiteral("label"), tr("Edit %1").arg(name)}});
    }
    if (!rolled && m_lastCancelled) {
      // The features after the sketch were not computed: the sketch stays
      // open, as it was.
      refreshScene();
      showHint(tr("Computation cancelled; Finish Sketch computes the features after %1.").arg(name));
      qInfo().noquote() << QStringLiteral("Finish Sketch cancelled: %1 stays open").arg(name);
      return false;
    }
  }
  const bool edited = m_sketchExisting;
  leaveSketchMode();
  recompute();
  showHint(tr("Extrude (E) a profile, or create another sketch."));
  qDebug() << "Sketch finished";
  if (edited && volumesLogged()) {
    // What the edit did to the bodies, measured by the model.
    try {
      for (const QJsonValue& value :
           query({{QStringLiteral("query"), QStringLiteral("bodies")}, {QStringLiteral("volumes"), true}})
               .toArray()) {
        const QJsonObject body = value.toObject();
        qDebug().noquote() << QStringLiteral("Body %1 (%2): volume %3 mm3")
                                  .arg(body.value(QStringLiteral("name")).toString(),
                                       body.value(QStringLiteral("uid")).toString())
                                  .arg(body.value(QStringLiteral("volume")).toDouble(), 0, 'f', 3);
      }
    } catch (const std::exception&) {
      // Volumes are only logged.
    }
  }
  return true;
}

// ---------------------------------------------------------------------------
// Undo

void MainWindow::undo() {
  if (m_mode == Mode::Command || m_mode == Mode::PickPlane) {
    return;
  }
  if (m_mode == Mode::Sketch) {
    // What a tool has half drawn goes first.
    m_sketch->resetTool();
    if (m_sketchRestoreMarker >= 0 && undoDepth() <= m_sketchEnterDepth) {
      // Nothing done in the edited sketch: leave it as it was.
      finishSketch();
      return;
    }
  }
  QJsonObject result;
  if (!runCommand({{QStringLiteral("cmd"), QStringLiteral("undo")}}, &result)) {
    return;
  }
  qDebug().noquote() << "Undo:" << result.value(QStringLiteral("label")).toString();
  if (m_mode == Mode::Sketch && !sketchExists(m_sketch->uid())) {
    // The active sketch itself was taken back.
    leaveSketchMode();
  }
  refreshScene();
}

void MainWindow::redo() {
  if (m_mode == Mode::Command || m_mode == Mode::PickPlane) {
    return;
  }
  QJsonObject result;
  if (!runCommand({{QStringLiteral("cmd"), QStringLiteral("redo")}}, &result)) {
    return;
  }
  qDebug().noquote() << "Redo:" << result.value(QStringLiteral("label")).toString();
  refreshScene();
}

// ---------------------------------------------------------------------------
// Project files

void MainWindow::closeEvent(QCloseEvent* event) {
  if (modelBusy()) {
    // The job stops first; the window closes when it is back (runJob).
    event->ignore();
    m_closePending = true;
    m_job->control().cancel();
    qDebug() << "Close while computing: cancelling";
    return;
  }
  if (m_session) {
    m_session->cancel(); // a rolled-back edit is no change to save
  }
  // The design's versions sent and its edit lock released (mitcad#89).
  if (maybeSave() && m_locks->quitting()) {
    // Remote work stops; versions not sent yet go at the next start.
    m_remote->shutDown();
    // Live updates leave the brokers cleanly (mitcad#89).
    m_live->shutDown();
    event->accept();
  } else {
    event->ignore();
  }
}

void MainWindow::newDocument() {
  if (!maybeSave()) {
    return;
  }
  installEmptyDocument(QString());
  qInfo() << "New document";
  showHint(tr("Create a sketch to start modelling. S searches for commands."));
}

void MainWindow::openDocument() {
  if (!maybeSave()) {
    return;
  }
  const QString path =
      QFileDialog::getOpenFileName(this, tr("Open"), fileDialogDirectory(), openFilter());
  if (path.isEmpty()) {
    return;
  }
  QString error;
  if (!openFile(path, error) && !error.isEmpty()) {
    const QString message = tr("Could not open %1:\n%2").arg(QDir::toNativeSeparators(path), error);
    qWarning().noquote() << message;
    sheetWarning(this, tr("Mitcad"), message);
  }
}

bool MainWindow::loadProject(const QString& path, QString& error) {
  ScopedTiming timing("open"); // reading, recomputing and showing it
  const QString name = QFileInfo(path).fileName();
  // In a project with version history (P12d): a file renamed outside
  // Mitcad takes its display state along first, which reading it reads.
  const std::optional<rust::Box<Project>> project = versionedProject(QFileInfo(path).absoluteFilePath());
  const QString renamedFrom = project ? followRename(**project, QFileInfo(path).absoluteFilePath()) : QString();
  QFile file(path);
  if (!file.open(QIODevice::ReadOnly)) {
    error = file.errorString();
    return false;
  }
  const QByteArray json = file.readAll();
  if (!json.isValidUtf8()) {
    error = tr("The file is not UTF-8 text.");
    return false;
  }
  // Linked components follow their files (relative to the project's).
  const QByteArray links = compactJson({{QStringLiteral("cmd"), QStringLiteral("update_links")},
                                        {QStringLiteral("base"), QFileInfo(path).absolutePath()}});
  QJsonArray messages;
  QString digest; // of the file as read, for autosave (P8)
  // A version 3 file's B-rep data comes from its project's store (P12a).
  const QByteArray location = QFileInfo(path).absoluteFilePath().toUtf8();
  // On the worker: the file is parsed and its links followed there, then
  // installDocument computes it.
  const DocumentMaker read = [&](ModelJob& job) {
    job.setStage(tr("Reading %1").arg(name));
    digest = fileDigest(json);
    ScopedTiming loading("open load");
    rust::Box<Document> document = load_document_at(rustStr(json), rustStr(location));
    loading.setDetail(QStringLiteral("%1 bytes").arg(json.size()));
    loading.finish();
    job.setStage(tr("Opening %1").arg(name));
    {
      const AttachedJob attached(*document, job);
      messages = parseObject(document->command(rustStr(links))).value(QStringLiteral("messages")).toArray();
    }
    // As opened, with its links followed: no change to save (a recompute
    // changes no revision).
    markSaved(*document);
    return document;
  };
  if (isVisible()) {
    showHint(tr("Opening %1...").arg(name));
  }
  if (!installDocument(name, QFileInfo(path).absoluteFilePath(), read, error)) {
    if (error.isEmpty()) {
      qInfo().noquote() << QStringLiteral("Opening %1 cancelled").arg(name);
      showHint(tr("Opening %1 cancelled.").arg(name));
    }
    return false;
  }
  m_autosave->saved(digest);
  if (project) {
    // What Save compares the file with (a change outside Mitcad), and a
    // rename to record with the next version.
    rememberVersionBase(**project, m_filePath);
    m_renamedFrom = renamedFrom;
  }
  for (const QJsonValue& message : std::as_const(messages)) {
    qInfo().noquote() << message.toString();
  }
  qInfo().noquote() << "Opened" << m_filePath;
  addRecentFile(m_filePath);
  // Its project (the current one, installDocument) remembers the design
  // opened last: Open Project selects it (mitcad#89).
  if (project && m_project.hasHistory()) {
    try {
      (*project)->command(rustStr(compactJson(
          {{QStringLiteral("cmd"), QStringLiteral("remember_design")}, {QStringLiteral("path"), m_filePath}})));
    } catch (const std::exception& e) {
      qWarning().noquote() << QStringLiteral("Version warning: %1").arg(errorText(e));
    }
  }
  // Its project's remote: its state, and a check for newer versions.
  m_remote->fileOpened(m_filePath);
  return true;
}

bool MainWindow::save() {
  // A read-only window (mitcad#89): Save as Copy or as a new version.
  if (const std::optional<bool> readOnlySaved = readOnlySave(false)) {
    return *readOnlySaved;
  }
  if (m_filePath.isEmpty()) {
    return saveAs();
  }
  // A recovered document's file may have changed since (P8c).
  const std::optional<bool> overwrite = askOverwriteChanged();
  if (!overwrite) {
    return false;
  }
  return *overwrite ? writeFile(m_filePath) : saveAs();
}

bool MainWindow::saveAs() {
  QFileDialog dialog(this, tr("Save As"), fileDialogDirectory(),
                     tr("Mitcad projects (*.%1)").arg(kFileSuffix));
  dialog.setAcceptMode(QFileDialog::AcceptSave);
  dialog.setDefaultSuffix(kFileSuffix);
  dialog.selectFile(m_filePath.isEmpty() ? documentName() + QLatin1Char('.') + kFileSuffix
                                         : documentName());
  if (dialog.exec() != QDialog::Accepted || dialog.selectedFiles().isEmpty()) {
    return false;
  }
  return writeFile(dialog.selectedFiles().constFirst());
}

bool MainWindow::writeFile(const QString& path, const QString& description, VersionRecord* required) {
  const QString absolute = QFileInfo(path).absoluteFilePath();
  // Stopped by an error (shown here, red): what a caller requiring the
  // version gets as why.
  const auto fail = [this, required](const QString& failure) {
    showError(failure);
    if (required != nullptr) {
      required->failure = failure;
    }
    return false;
  };
  // A read-only window writes over its design's file only for Save as New
  // Version (mitcad#89); Save As writes a copy.
  if (windowReadOnly() && !m_writeAnyway && !m_filePath.isEmpty() && sameFolderPath(absolute, m_filePath)) {
    refuseReadOnly(QStringLiteral("writing %1").arg(absolute));
    return false;
  }
  // In a project with version history (P12d) Save records a version too:
  // by the author (shown once), after a change made outside Mitcad was
  // dealt with and a rename made outside it recorded.
  QString projectError;
  const std::optional<rust::Box<Project>> project = versionedProject(absolute, &projectError);
  if (!project && absolute == m_filePath &&
      (m_project.hasHistory() || (m_versionBase && m_versionBase->path == absolute))) {
    return fail(tr("Could not check %1 before saving: %2. Save As can keep your design in another file.")
                    .arg(QFileInfo(absolute).fileName(), projectError));
  }
  QString author;
  QStringList paths{absolute};
  QString message = description;
  if (project) {
    // A sync moves the project's branch: Save waits for it (P12 remote).
    m_remote->waitForBranch();
    const std::optional<QString> chosen = versionAuthor(**project);
    if (!chosen) {
      showHint(tr("Save cancelled."));
      return false;
    }
    author = *chosen;
    if (absolute == m_filePath) {
      QString conflictFailure;
      const std::optional<bool> write = askVersionConflict(**project, absolute, author, conflictFailure);
      if (!write) {
        if (!conflictFailure.isEmpty()) {
          return fail(conflictFailure);
        }
        showHint(tr("Save cancelled."));
        return false;
      }
      if (!*write) {
        return saveAs();
      }
      paths << recordRename(**project, absolute, author);
    }
    if (message.isEmpty()) {
      // The undo steps since the last save, before saving marks it.
      QStringList steps;
      const QJsonObject changes = queryObject({{QStringLiteral("query"), QStringLiteral("changes_since_saved")}});
      for (const QJsonValue& step : changes.value(QStringLiteral("steps")).toArray()) {
        steps << step.toString();
      }
      message = automaticVersionMessage(QFileInfo(absolute).fileName(), steps);
    }
  }
  // Whole or not at all (a temporary file renamed over it); in a project
  // (P12a) version 3, with base features' B-rep data in the project's
  // store, else one file.
  std::optional<rust::String> json;
  QString error;
  try {
    json = idleDocument().save_project(rustStr(path.toUtf8()), "auto");
  } catch (const std::exception& e) {
    error = QString::fromUtf8(e.what());
  }
  if (!json) {
    const QString failure = tr("Could not save %1:\n%2").arg(QDir::toNativeSeparators(path), error);
    qWarning().noquote() << failure;
    sheetWarning(this, tr("Mitcad"), failure);
    if (required != nullptr) {
      required->failure = failure;
    }
    return false;
  }
  VersionRecord status;
  if (project) {
    status = recordVersion(**project, absolute, paths, message, author);
  }
  afterSaved(path, QByteArrayView(json->data(), static_cast<qsizetype>(json->size())), status.status, status.problem);
  if (project) {
    rememberVersionBase(**project, absolute);
    // The version's preview for Version History (P12e).
    if (m_versionBase) {
      saveVersionThumbnail(m_versionBase->file);
    }
  } else {
    m_versionBase.reset();
    m_renamedFrom.clear();
  }
  // Saved into another project, or outside projects: the current project
  // follows the file (mitcad#89).
  followFileProject(absolute);
  updateVersionStatus();
  if (project) {
    // Sent to the project's remote, if it has one (P12 remote).
    m_remote->versionRecorded(absolute);
  }
  // Under another name, the design is that file now, with its own edit
  // lock (Save As, Save as Copy).
  m_locks->designSaved(absolute);
  if (required == nullptr) {
    return true;
  }
  if (!project) {
    status.failure = tr("%1 has no version history.").arg(QFileInfo(absolute).fileName());
  }
  *required = status;
  return project && status.preserved;
}

void MainWindow::afterSaved(const QString& path, QByteArrayView content, const QString& status, bool problem) {
  m_filePath = QFileInfo(path).absoluteFilePath();
  m_projectDir.clear();
  markSaved(*m_document);
  // Saved, the recovery data of the document goes (P8).
  m_autosave->saved(fileDigest(content));
  m_recoveredDigest.reset();
  updateWindowTitle();
  addRecentFile(m_filePath);
  qInfo().noquote() << "Saved" << m_filePath;
  if (status.isEmpty()) {
    showHint(tr("Saved %1").arg(QDir::toNativeSeparators(m_filePath)));
  } else if (problem) {
    showError(status);
  } else {
    showHint(status);
  }
  // What was computed since it opened, for the next time (P7d).
  persistResults(QFileInfo(m_filePath).fileName());
}

bool MainWindow::maybeSave() {
  // A read-only window (mitcad#89): only changes kept from editing count.
  if (const std::optional<bool> readOnlySaved = readOnlySave(true)) {
    return *readOnlySaved;
  }
  if (!isModified()) {
    return true;
  }
  QMessageBox box(QMessageBox::Warning, tr("Mitcad"), tr("Save changes to %1?").arg(documentName()),
                  QMessageBox::Save | QMessageBox::Discard | QMessageBox::Cancel, this);
  box.setInformativeText(tr("Your changes will be lost if you don't save them."));
  // The same words on every platform, with keys for the keyboard.
  box.button(QMessageBox::Save)->setText(tr("&Save"));
  box.button(QMessageBox::Discard)->setText(tr("&Don't Save"));
  box.setDefaultButton(QMessageBox::Save);
  prepareModal(&box);
  switch (box.exec()) {
  case QMessageBox::Save:
    return save();
  case QMessageBox::Discard:
    return true;
  default:
    return false;
  }
}

bool MainWindow::isModified() const {
  // The model compares its revision with the one marked saved (P8), so no
  // project file is made for it.
  return query({{QStringLiteral("query"), QStringLiteral("document")}})
      .toObject()
      .value(QStringLiteral("modified"))
      .toBool();
}

QString MainWindow::documentName() const {
  if (m_filePath.isEmpty()) {
    return m_documentName.isEmpty() ? tr("Untitled") : m_documentName;
  }
  return QFileInfo(m_filePath).fileName();
}

QString MainWindow::fileDialogDirectory() const {
  if (!m_filePath.isEmpty()) {
    return QFileInfo(m_filePath).absolutePath();
  }
  // A new design of a project goes in it (P12d, mitcad#89).
  if (!m_projectDir.isEmpty()) {
    return m_projectDir;
  }
  if (m_project.isProject()) {
    return m_project.root;
  }
  return QStandardPaths::writableLocation(QStandardPaths::DocumentsLocation);
}

bool MainWindow::installDocument(const QString& label, const QString& path, const DocumentMaker& make,
                                 QString& error) {
  std::optional<rust::Box<Document>> made;
  QJsonObject recomputed;
  QJsonObject persisted;
  double milliseconds = 0.0;
  // The result store (P7d): costly features come from it, and what was
  // costly to compute goes to it.
  const QByteArray store = resultStoreDirectory().toUtf8();
  const QByteArray buildId = resultStoreBuildId().toUtf8();
  const QByteArray storeLabel = label.toUtf8();
  const double minMs = resultStoreMinMs();
  const auto memoryMb = static_cast<std::uint64_t>(CacheSettings::load().memoryMegabytes);
  try {
    const bool done = runJob(label, tr("Opening %1").arg(label), [&](ModelJob& job) {
      rust::Box<Document> document = make(job);
      document->set_result_store(rustStr(store), rustStr(buildId), rustStr(storeLabel));
      document->set_memory_cache_budget(memoryMb);
      {
        const AttachedJob attached(*document, job);
        QElapsedTimer timer;
        timer.start();
        recomputed = parseObject(document->command(rustStr(compactJson(
            {{QStringLiteral("cmd"), QStringLiteral("recompute")}}))));
        milliseconds = static_cast<double>(timer.nsecsElapsed()) / 1.0e6;
      }
      if (!store.isEmpty() && recomputed.value(QStringLiteral("recomputed")).toInt() > 0) {
        job.setStage(tr("Saving results of %1").arg(label));
        persisted = parseObject(document->persist_results(minMs));
      }
      made.emplace(std::move(document));
    });
    if (!done) {
      error.clear();
      return false;
    }
  } catch (const std::exception& e) {
    error = errorText(e);
    return false;
  }
  // Another design: the old one's versions are sent and its edit lock
  // released first (mitcad#89); a sync that needs a choice may keep it.
  const bool readOnlyOpen = m_openingReadOnly;
  if (!m_locks->designClosing(path, readOnlyOpen)) {
    error.clear();
    return false;
  }
  // The new document is this thread's now; the old one goes, and with it
  // any command or sketch on it, and its recovery data (P8).
  resetCommandState();
  m_document = std::move(*made);
  m_autosave->replaced();
  m_recoveredDigest.reset();
  m_filePath = path;
  m_documentName.clear();
  m_versionBase.reset();
  m_renamedFrom.clear();
  m_projectDir.clear();
  m_changeNoted = false;
  if (!path.isEmpty()) {
    // A design's file (opened, recovered): its project is the current one
    // (mitcad#89); an untitled design keeps the project.
    followFileProject(path);
  }
  updateVersionStatus();
  m_profileFaces.clear();
  reportRecompute(recomputed, milliseconds);
  if (!store.isEmpty()) {
    reportStored(recomputed, persisted);
  }
  refreshScene();
  if (!m_keepView) {
    m_viewer->fitAll();
  }
  // Its edit lock (mitcad#89): taken, or the same design's kept.
  m_locks->designOpened(path, readOnlyOpen);
  updateActions();
  return true;
}

void MainWindow::reportStored(const QJsonObject& recomputed, const QJsonObject& persisted) {
  const int results = persisted.value(QStringLiteral("results")).toInt();
  qInfo().noquote() << QStringLiteral("Result store: restored %1, evaluated %2; stored %3 (%4 bytes) in %5 ms")
                           .arg(recomputed.value(QStringLiteral("restored")).toInt())
                           .arg(recomputed.value(QStringLiteral("recomputed")).toInt())
                           .arg(results)
                           .arg(persisted.value(QStringLiteral("bytes")).toInteger())
                           .arg(persisted.value(QStringLiteral("ms")).toDouble(), 0, 'f', 1);
  for (const QJsonValue& failure : persisted.value(QStringLiteral("errors")).toArray()) {
    qWarning().noquote() << "Result store:" << failure.toString();
  }
  if (results > 0) {
    collectResultStoreGarbage();
  }
}

void MainWindow::applyCacheSettings() {
  if (modelBusy()) {
    return; // Preferences is not open during a job
  }
  m_document->set_result_store(rustStr(resultStoreDirectory().toUtf8()), rustStr(resultStoreBuildId().toUtf8()),
                               rustStr(documentName().toUtf8()));
  m_document->set_memory_cache_budget(static_cast<std::uint64_t>(CacheSettings::load().memoryMegabytes));
  collectResultStoreGarbage();
}

void MainWindow::persistResults(const QString& label) {
  const QByteArray store = resultStoreDirectory().toUtf8();
  if (store.isEmpty()) {
    return;
  }
  const QByteArray buildId = resultStoreBuildId().toUtf8();
  const QByteArray storeLabel = label.toUtf8();
  const double minMs = resultStoreMinMs();
  Document& document = *m_document;
  QJsonObject persisted;
  try {
    runJob(
        QStringLiteral("results"), tr("Saving results of %1").arg(label),
        [&](ModelJob&) {
          // Saved under another name, the results are that file's now.
          document.set_result_store(rustStr(store), rustStr(buildId), rustStr(storeLabel));
          persisted = parseObject(document.persist_results(minMs));
        },
        JobLog::WhenSlow);
  } catch (const std::exception& e) {
    qWarning().noquote() << "Result store:" << errorText(e);
    return;
  }
  reportStored({}, persisted);
}

void MainWindow::installEmptyDocument(const QString& name) {
  QString error;
  installDocument(name.isEmpty() ? tr("Untitled") : name, QString(),
                  [](ModelJob&) { return new_document(); }, error);
  m_documentName = name;
  updateWindowTitle();
}

// Leaves any command or sketch, so the next document starts idle.
void MainWindow::resetCommandState() {
  m_commandFinished = nullptr; // the document it would follow up on goes
  if (m_session) {
    m_session->discard(); // nothing to restore in a document that goes
  }
  if (m_mode == Mode::Sketch) {
    leaveSketchMode();
  }
  if (m_mode == Mode::PickPlane) {
    m_mode = Mode::Idle;
    inputActivated(SelectFilter());
  }
  m_selection.clear();
  m_section = QJsonValue();
}

void MainWindow::updateWindowTitle() {
  ScopedTiming timing("title");
  // The title text is set explicitly, so the file path below changes no
  // visible text ("[*]" is Qt's placeholder for the modified mark, shown
  // once, on every platform); on macOS it makes the proxy icon (Cmd-click
  // for the folders, drag the file from the title bar) and the dot in the
  // close button. An unsaved document has no path.
  const QString filePath = m_filePath.isEmpty() ? QString() : QFileInfo(m_filePath).absoluteFilePath();
  if (windowFilePath() != filePath) {
    setWindowFilePath(filePath);
  }
  setWindowTitle(QStringLiteral("%1[*] - Mitcad").arg(documentName()));
  const bool modified = isModified();
  setWindowModified(modified);
  // The first change since opening: a newer version on the remote is
  // looked for (P12 remote).
  if (modified && !m_changeNoted && !m_filePath.isEmpty() && m_remote != nullptr) {
    m_changeNoted = true;
    m_remote->firstChange();
  }
}

} // namespace mitcad
