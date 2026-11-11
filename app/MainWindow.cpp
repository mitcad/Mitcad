// SPDX-License-Identifier: MIT
#include "MainWindow.hpp"
#include "WindowManager.hpp"

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <functional>
#include <initializer_list>
#include <limits>
#include <map>
#include <utility>

#include <QAction>
#include <QAbstractSpinBox>
#include <QApplication>
#include <QCloseEvent>
#include <QDir>
#include <QDockWidget>
#include <QElapsedTimer>
#include <QFile>
#include <QFileDialog>
#include <QFileInfo>
#include <QHBoxLayout>
#include <QJsonDocument>
#include <QKeyEvent>
#include <QLineEdit>
#include <QMargins>
#include <QLabel>
#include <QMenu>
#include <QMenuBar>
#include <QMessageBox>
#include <QPair>
#include <QPushButton>
#include <QPlainTextEdit>
#include <QResizeEvent>
#include <QScrollArea>
#include <QScrollBar>
#include <QSet>
#include <QStackedWidget>
#include <QStandardPaths>
#include <QStatusBar>
#include <QTimer>
#include <QToolBar>
#include <QToolButton>
#include <QTextEdit>
#include <QUrl>
#include <QUuid>
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
#include <Standard_Failure.hxx>
#include <gp.hxx>
#include <gp_Ax2.hxx>
#include <gp_Ax3.hxx>
#include <gp_Circ.hxx>
#include <gp_Pln.hxx>
#include <gp_Pnt2d.hxx>
#include <gp_Trsf.hxx>
#include <gp_Vec.hxx>

#include "AboutDialog.hpp"
#include "browser/BrowserBackground.hpp"
#include "browser/BrowserController.hpp"
#include "browser/ComponentColors.hpp"
#include "browser/ConfigurationBar.hpp"
#include "browser/TimelineWidget.hpp"
#include "browser/FeaturePresentation.hpp"
#include "browser/FeatureStatus.hpp"
#include "commands/Commands.hpp"
#include "drawings/DrawingTools.hpp"
#include "files/Autosave.hpp"
#include "files/FileFormats.hpp"
#include "files/LiveController.hpp"
#include "files/LockController.hpp"
#include "files/Print3d.hpp"
#include "files/ProjectIndicator.hpp"
#include "mcp/AgentIndicator.hpp"
#include "files/RemoteController.hpp"
#include "projects/ProjectsPanel.hpp"
#include "files/VersionDialogs.hpp"
#include "framework/AppSettings.hpp"
#include "framework/ModelMessages.hpp"
#include "framework/Appearances.hpp"
#include "framework/AutoselectPanel.hpp"
#include "framework/Dialogs.hpp"
#include "framework/DisplayMeshes.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/ChromeStyle.hpp"
#include "framework/CommandSearch.hpp"
#include "framework/DebugLog.hpp"
#include "framework/FitToScreen.hpp"
#include "framework/FloatingLayout.hpp"
#include "framework/GlassCard.hpp"
#include "framework/MacToolButtonStyle.hpp"
#include "framework/MessageBand.hpp"
#include "framework/PlaceWatcher.hpp"
#include "view/MeasurementDrawing.hpp"
#include "framework/Icons.hpp"
#include "framework/Json.hpp"
#include "framework/CacheDiagnostics.hpp"
#include "framework/ModelShapes.hpp"
#include "framework/ParameterField.hpp"
#include "framework/ResultCache.hpp"
#include "framework/Restart.hpp"
#include "framework/QuickAccessGroup.hpp"
#include "framework/EnterRepeat.hpp"
#include "framework/MenuBarReveal.hpp"
#include "framework/Ribbon.hpp"
#include "framework/SectionPreview.hpp"
#include "framework/SectionWorker.hpp"
#include "framework/SelectionCreate.hpp"
#include "framework/TitleBar.hpp"
#include "framework/ShortcutDialog.hpp"
#include "framework/Theme.hpp"
#include "framework/Diagnostics.hpp"
#include "framework/TestSync.hpp"
#include "framework/WindowStyle.hpp"
#include "library/PartPicker.hpp"
#include "platform/MacChrome.hpp"
#include "print/MakeBodyCard.hpp"
#ifdef MITCAD_MACOS_NATIVE_UI
#include "platform/NativeMacWindow.hpp"
#include "platform/NativeForms.hpp"
#include "platform/NativeDialogs.hpp"
#endif
#include "sketch/PaletteCreateTools.hpp"
#include "sketch/Sketch3dController.hpp"
#include "sketch/Sketch3dPalette.hpp"
#include "sketch/SketchPalette.hpp"
#include "sketch/SketchTool.hpp"
// Click adds to the selection in sketches (mitcad#1403).
#include "sketch/SketchPreferences.hpp"
#include "update/UpdateController.hpp"
#include "report/ReportCenter.hpp"
#include "view/ModeBanner.hpp"
#include "view/SmallCirclesPreference.hpp"
#include "view/SpaceMouseController.hpp"
#include "view/ViewController.hpp"
#include "view/ViewTessellation.hpp"
#include "view/VersionWatermark.hpp"
#include "mitcad/geometry/datum.hpp"
#include "mitcad/io/mesh.hpp"
#include "framework/UiSizes.hpp"

namespace mitcad {
namespace {

const QString kFileSuffix = QStringLiteral("mitcad");
// The view's selection and hover (the role table's, mitcad#1041).
const QColor kSelectionColor = viewColor(ColorRole::Select, false);
const QColor kHoverColor = viewColor(ColorRole::Hover, false);
// A Fillet's or Chamfer's own faces while its picked edges are drawn where
// they were in the selection colour (mitcad#957): amber, apart from those
// lines on the light and the dark background; lighter on the timeline's
// hover.
const QColor kFeatureFacesColor = viewColor(ColorRole::FeatureFaces, false);
const QColor kFeatureFacesHoverColor = viewColor(ColorRole::FeatureFacesHover, false);
// A body's print bed face shown while a face's context menu is open
// (mitcad#731).
const QColor kPrintBedColor = printBedColor();
// Size of the planes and axes that stand for datums, mm.
constexpr double kDatumSize = 100.0;
const QStringList kOriginDatums = {QStringLiteral("xy"), QStringLiteral("xz"), QStringLiteral("yz"),
                                   QStringLiteral("x"),  QStringLiteral("y"),  QStringLiteral("z"),
                                   QStringLiteral("origin")};
// The pick places a 3D sketch's drawing tool logs for UI tests: the faces
// a point snaps onto and the body vertices a point is attached to.
constexpr SelectFilter kSketch3dSourcePlaces = SelectKind::Face | SelectKind::Vertex;

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

// The document's state is its design file's, as saved or opened: the
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

// What the view draws normally while a component other than the root is
// active (mitcad#420): its occurrences, the sketch being edited and the
// origin's datums (the active component's own); the rest is drawn as
// inactive ghosts. Off while the root is active.
OcctViewer::ActiveScope activeScopeOf(const DocumentSnapshot& snapshot, const QString& editedSketch) {
  OcctViewer::ActiveScope scope;
  if (snapshot.activeComponent.isEmpty() || snapshot.activeComponent == QStringLiteral("C0")) {
    return scope;
  }
  QStringList names;
  std::function<void(const QVector<DocumentSnapshot::Occurrence>&)> visit =
      [&](const QVector<DocumentSnapshot::Occurrence>& occurrences) {
        for (const DocumentSnapshot::Occurrence& occurrence : occurrences) {
          if (occurrence.component == snapshot.activeComponent) {
            scope.occurrences << occurrence.path;
            names << occurrence.name;
          } else {
            visit(occurrence.children);
          }
        }
      };
  visit(snapshot.occurrences);
  scope.on = true;
  scope.name = names.isEmpty() ? snapshot.componentName(snapshot.activeComponent) : names.join(QStringLiteral(", "));
  scope.normal = kOriginDatums;
  if (!editedSketch.isEmpty()) {
    scope.normal << editedSketch;
  }
  return scope;
}

bool hasMeshFaces(const TopoDS_Shape& shape) {
  for (TopExp_Explorer faces(shape, TopAbs_FACE); faces.More(); faces.Next()) {
    TopLoc_Location location;
    if (BRep_Tool::Surface(TopoDS::Face(faces.Current()), location).IsNull()) {
      return true;
    }
  }
  return false;
}

// Meshes the clipped bodies as the view would (mitcad#438): together, on
// all cores, before they are shown, so that the view keeps these
// triangulations instead of meshing body after body as it draws them. The
// shapes are clips of the bodies' detached copies (clipBodies), the
// caller's alone: the faces a clip left as they were take the meshes of the
// bodies they were copied from (`donors`, only read), the others are meshed;
// mesh bodies (and their caps) are left to the view, as before.
void meshForView(const std::vector<TopoDS_Shape>& shapes, const std::vector<TopoDS_Shape>& donors) {
  // Each body by itself, as the view draws it: its deviation is its own
  // size's (io::shape_deviation, mitcad#1275), not the whole compound's.
  std::vector<TopoDS_Shape> pending;
  for (const TopoDS_Shape& shape : shapes) {
    if (!io::has_triangulation(shape) && !hasMeshFaces(shape)) {
      pending.push_back(shape);
    }
  }
  if (pending.empty()) {
    return;
  }
  try {
    for (const TopoDS_Shape& shape : pending) {
      io::reuse_triangulations(shape, donors);
    }
    io::mesh_all_in_place(pending, viewMeshOptions());
  } catch (const Standard_Failure&) {
    // The view meshes them when it draws them, as before.
  } catch (const std::exception&) {
  }
}

// The bodies a section's plane cuts (Section Analysis, Hide Above Sketch),
// `which` of `bodies`, each with its target in `targets` ({body,
// occurrence}), clipped at `plane` (design coordinates) all at once: the
// model clips them on all cores (`analysis_clips`, mitcad#438). For each of
// `which`, what is left of it: its own shape when the plane leaves it whole
// (the view keeps it drawn as it is), null when its clip failed and it is
// shown whole (logged, its error in `errors`; a crash inside OCCT is its
// error, mitcad#392). The new shapes are meshed for the view. The bodies
// are clipped as detached copies (`prepare_clips`, as a section round's):
// a clip keeps the faces it leaves as they were, and meshing it here would
// write the new faces' polygons into the bodies' own edges, which the
// background evaluation may be reading meanwhile (mitcad#1553).
std::vector<std::shared_ptr<geometry::Shape>> clipBodies(const Document& document, const QJsonValue& plane,
                                                         const QJsonArray& targets,
                                                         const std::vector<std::size_t>& which,
                                                         const std::vector<BodyDisplay>& bodies,
                                                         QStringList& errors) {
  ScopedTiming timing("section clips");
  errors = QStringList();
  for (std::size_t i = 0; i < which.size(); ++i) {
    errors << QStringLiteral("not clipped");
  }
  const auto shownWhole = [&](std::size_t i, const QString& error) {
    errors[static_cast<qsizetype>(i)] = error;
    qWarning().noquote() << QStringLiteral("Section: %1 shown whole: %2")
                                .arg(QString::fromStdString(bodies[which[i]].uid), error);
  };
  std::vector<std::shared_ptr<geometry::Shape>> left(which.size());
  std::vector<TopoDS_Shape> clipped;
  std::vector<TopoDS_Shape> donors; // the bodies clipped, as the view shows them
  try {
    const rust::Box<ClippedBodies> results =
        document
            .prepare_clips(
                rustStr(compactJson({{QStringLiteral("plane"), plane}, {QStringLiteral("bodies"), targets}})))
            ->run();
    for (std::size_t i = 0; i < which.size() && i < results->count(); ++i) {
      const BodyDisplay& body = bodies[which[i]];
      const QString uid = QString::fromStdString(body.uid);
      const std::shared_ptr<geometry::Shape> shape = results->shape(i);
      if (!shape) {
        const rust::String error = results->error(i);
        shownWhole(i, QString::fromUtf8(error.data(), static_cast<qsizetype>(error.size())));
        continue;
      }
      // A body the test hook names fails as a clip OCCT cannot make would
      // (mitcad#1230).
      if (const QString forced = testClipFailure(uid, shape, body.shape); !forced.isEmpty()) {
        shownWhole(i, forced);
        continue;
      }
      logClipAttempts(uid, shape);
      if (!shape->occt().IsSame(body.shape->occt())) {
        clipped.push_back(shape->occt());
        donors.push_back(body.shape->occt());
      }
      left[i] = shape;
      errors[static_cast<qsizetype>(i)].clear();
    }
  } catch (const std::exception& e) {
    for (std::size_t i = 0; i < which.size(); ++i) {
      shownWhole(i, errorText(e));
    }
    return left;
  }
  timing.setDetail(QStringLiteral("%1 bodies, %2 new").arg(which.size()).arg(clipped.size()));
  meshForView(clipped, donors);
  return left;
}

// A body's key in the kept clips (MainWindow::SectionState): its clip
// depends on its shape (compared apart) and its occurrence's placement.
QString sectionClipKey(const QString& uid, const QString& occurrence, const QJsonArray& transform) {
  return uid + QLatin1Char('|') + occurrence + QLatin1Char('|') +
         QString::fromUtf8(QJsonDocument(transform).toJson(QJsonDocument::Compact));
}

// The Sketch Palette in a scroll area: it keeps its rows at their size hint
// and scrolls when its place is lower, in the docked command panel and in
// the floating command card (mitcad#412, mitcad#470). Finish Sketch stays
// under the scroll area, in sight however long the palette is (mitcad#1027:
// with its Create section the palette outgrew the Docked panel at 1280 x
// 800 and Finish Sketch scrolled away), as a command panel's OK and Cancel
// stay under its inputs. `transparent`: the card's glass shows through
// (the floating layout).
QWidget* wrapPalette(sketch::SketchPalette* palette, bool transparent) {
  auto* scroll = new QScrollArea;
  scroll->setObjectName(QStringLiteral("sketchPaletteScroll"));
  scroll->setWidgetResizable(true);
  scroll->setFrameShape(QFrame::NoFrame);
  scroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
  scroll->setWidget(palette);
  if (transparent) {
    scroll->setAutoFillBackground(false);
    scroll->viewport()->setAutoFillBackground(false);
    palette->setAutoFillBackground(false);
  }
  // The Auto constraint section, which shows once a drawing tool has drawn,
  // comes into view; the palette's places are logged again when it
  // scrolls, for UI tests.
  QObject::connect(palette, &sketch::SketchPalette::autoSectionShown, scroll,
                   [scroll](QWidget* section) { scroll->ensureWidgetVisible(section, 0, 0); });
  QObject::connect(scroll->verticalScrollBar(), &QScrollBar::valueChanged, palette, [scroll, palette] {
    TestSync::singleShot(0, palette, [scroll, palette] {
      if (palette->isVisible()) {
        const QScrollBar* bar = scroll->verticalScrollBar();
        qDebug().noquote() << QStringLiteral("Palette scrolled to %1 of %2 (view %3 high, palette %4)")
                                  .arg(bar->value())
                                  .arg(bar->maximum())
                                  .arg(scroll->viewport()->height())
                                  .arg(palette->height());
        palette->logPlaces();
      }
    });
  });
  QWidget* finish = palette->takeFinish();
  if (finish == nullptr) {
    return scroll;
  }
  auto* page = new QWidget;
  page->setObjectName(QStringLiteral("sketchPalettePage"));
  auto* column = new QVBoxLayout(page);
  column->setContentsMargins(0, 0, 0, 0);
  column->setSpacing(0);
  column->addWidget(scroll, 1);
  // At the right, inset as the palette's rows are.
  auto* row = new QHBoxLayout;
  const QMargins inset = palette->layout()->contentsMargins();
  row->setContentsMargins(inset.left(), 4, inset.right(), inset.bottom());
  row->addStretch(1);
  row->addWidget(finish);
  column->addLayout(row);
  return page;
}

} // namespace

MainWindow::MainWindow(bool demo, QWidget* parent, bool nativeUi)
    : QMainWindow(parent), m_nativeUi(nativeUi), m_documentSession(new DocumentSession(demo, this)),
      m_mcpDocumentId(QUuid::createUuid().toString(QUuid::WithoutBraces)),
      m_viewer(new OcctViewer(this)), m_statusLabel(new QLabel(this)),
      m_rendererLabel(new QLabel(this)) {
  m_viewer->setObjectName(QStringLiteral("modelView"));
  // What a fit frames in a view with nothing to fit, and a new design's
  // first view (mitcad#1000): the box of the origin planes, as showDatums
  // draws them (kDatumSize, centred on the origin).
  {
    Bnd_Box originBox;
    const double half = kDatumSize / 2.0;
    originBox.Update(-half, -half, -half, half, half, half);
    m_viewer->setOriginBox(originBox);
  }
  m_documentSession->setWaitHandler([this](const QString& label, ModelJob& job, const QElapsedTimer& clock,
                                          bool& logged) { waitForJob(label, job, clock, logged); });
  // Section Analysis off the UI thread (mitcad#438): its rounds read only
  // what was detached from the bodies, so a job may run beside them, but
  // what they would show may be stale after it: they are told to stop
  // before every job and done again afterwards.
  m_sectionWorker = new SectionWorker(this);
  m_documentSession->setBeforeJob([this] { redoSectionWork(); });
  connect(m_documentSession, &DocumentSession::busyChanged, this, [this](bool busy) {
    m_viewer->setPickingSuspended(busy);
    if (!busy && m_closePending) {
      m_closePending = false;
      QTimer::singleShot(0, this, [this] {
#ifdef MITCAD_MACOS_NATIVE_UI
        if (m_nativeWindow) {
          m_nativeWindow->close();
          return;
        }
#endif
        close();
      });
    }
  });
  // Preferences > Display > Small circles changed (mitcad#879): the shown
  // bodies' display meshes made again with the new choice.
  connect(&SmallCirclesNotifier::instance(), &SmallCirclesNotifier::changed, this, [this] { remeshDisplay(); });
  setAcceptDrops(true);

  // The project indicator (mitcad#89): the current project, its kind, the
  // open design's version and its remote's state (P12 remote), with the
  // project's menu.
  m_indicator = new ProjectIndicator([this](const QString& id) { trigger(id); }, this);
  // Beside it, the agents connected through live MCP (mitcad#710).
  // Its menu's Outgoing connections are the chat panel's agents
  // (mitcad#1299, chatOutgoingAgents).
  m_agentIndicator = new mcp::AgentIndicator(
      {[this](const QString& id) { trigger(id); },
       [this](const QStringList& ids) {
         if (m_mcpDisconnect) {
           m_mcpDisconnect(ids);
         }
       },
       [this] { return chatOutgoingAgents(); },
       [this](const QString& id, bool offered) { offerChatAgent(id, offered); }},
      this);
  createRemote();
  createLocks();
  // The level's icon before the status line's text (mitcad#1040).
  m_statusIcon = new QLabel(this);
  m_statusIcon->setObjectName(QStringLiteral("statusIcon"));
  m_statusIcon->hide();
  // The window's messages and their history (mitcad#1042): the Floating
  // message band shows them (createFloatingPanels), the Docked status bar's
  // button opens their history.
  m_messages = new MessageCenter(this);
  m_messageHistory = new MessageHistory(*m_messages, this);
  connect(m_messageHistory, &MessageHistory::featureChosen, this, [this](const QString& uid) {
    if (m_controller == nullptr || !m_controller->selectFeature(uid)) {
      showStatus(tr("That feature is no longer in the design."), NoticeLevel::Information);
    }
  });
  if (m_nativeUi || chromeStyle() == ChromeStyle::Floating) {
    // No status bar: the view is the central widget of createPanels(),
    // messages go to the message band over it. The labels stay as the store
    // of the texts (the log), in a widget that is never shown; the project
    // and agent indicators go to the title bar row (createPanels).
    auto* store = new QWidget(this);
    store->setObjectName(QStringLiteral("hiddenStore"));
    store->hide();
    for (QWidget* widget : std::initializer_list<QWidget*>{m_statusIcon, m_statusLabel, m_indicator,
                                                           m_agentIndicator, m_rendererLabel}) {
      widget->setParent(store);
    }
  } else {
    setCentralWidget(m_viewer);
    statusBar()->addWidget(m_statusIcon);
    statusBar()->addWidget(m_statusLabel, 1);
    // The button of a message that has one (mitcad#1416: Open Linked
    // Design, Update), right after the message, as the Floating band has it;
    // its place logged as the band's (`Message band action <label> at x,y`).
    m_statusActionButton = new QToolButton(this);
    m_statusActionButton->setObjectName(QStringLiteral("statusActionButton"));
    m_statusActionButton->setToolButtonStyle(Qt::ToolButtonTextOnly);
    m_statusActionButton->setFocusPolicy(Qt::NoFocus);
    m_statusActionButton->hide();
    connect(m_statusActionButton, &QToolButton::clicked, this, [this] {
      const std::function<void()> run = std::exchange(m_statusButtonRun, nullptr);
      const QString label = m_statusActionButton->text();
      m_statusActionButton->hide();
      if (!run) {
        return;
      }
      qDebug().noquote() << QStringLiteral("Message band action %1 pressed: %2").arg(label, m_statusLabel->text());
      if (m_messages != nullptr) {
        m_messages->close(); // the band's copy of the message: answered
      }
      run();
    });
    new PlaceWatcher(m_statusActionButton, [this] { logStatusActionPlace(); });
    statusBar()->addWidget(m_statusActionButton);
    // The message history's button (mitcad#1042), after the message.
    m_messageHistoryButton = new QToolButton(this);
    m_messageHistoryButton->setObjectName(QStringLiteral("messageHistoryButton"));
    m_messageHistoryButton->setAutoRaise(true);
    m_messageHistoryButton->setIcon(themeIcon(QStringLiteral("version-history")));
    m_messageHistoryButton->setToolTip(tr("Message history: this session's messages"));
    m_messageHistoryButton->setAccessibleName(tr("Message history"));
    m_messageHistoryButton->setFocusPolicy(Qt::NoFocus);
    connect(m_messageHistoryButton, &QToolButton::clicked, this, [this] {
      const QRect anchor(GlassCard::mapToHost(m_messageHistoryButton, QPoint(0, 0)), m_messageHistoryButton->size());
      toggleMessageHistory(anchor, m_messageHistoryButton);
    });
    new PlaceWatcher(m_messageHistoryButton, [this] {
      const QPoint at =
          GlassCard::mapToHost(m_messageHistoryButton, m_messageHistoryButton->rect().center());
      const QString line = QStringLiteral("Message history button at %1,%2").arg(at.x()).arg(at.y());
      if (line != m_loggedHistoryButton) {
        m_loggedHistoryButton = line;
        qDebug().noquote() << line;
      }
    });
    statusBar()->addPermanentWidget(m_messageHistoryButton);
    statusBar()->addPermanentWidget(m_indicator);
    statusBar()->addPermanentWidget(m_agentIndicator);
    statusBar()->addPermanentWidget(m_rendererLabel);
  }
  connect(m_viewer, &OcctViewer::glInitialized, this, [this](const QString& renderer) {
    m_rendererLabel->setText(tr("OpenGL: %1").arg(renderer));
    qInfo().noquote() << "OpenGL renderer:" << renderer;
    // The debug log's header (mitcad#269): the GPU, as its driver names it.
    DebugLog::instance().setFact(QStringLiteral("Graphics"),
                                 QStringLiteral("%1, OpenGL %2").arg(renderer, m_viewer->glVersion()));
    if (m_reports != nullptr) {
      m_reports->setGraphics(renderer, m_viewer->glVersion());
    }
  });
  connect(m_viewer, &OcctViewer::glFailed, this, &MainWindow::showError);
  // Picks arrive while the view is painting; react afterwards.
  connect(m_viewer, &OcctViewer::picked, this, &MainWindow::onPicked, Qt::QueuedConnection);
  // The pick places of the active input again once the cards over the view
  // have moved (the command's card laid out after its input took the
  // picks), so that none is under a card (mitcad#1086); only with
  // MITCAD_LOG_PICKS.
  if (qEnvironmentVariableIsSet("MITCAD_LOG_PICKS")) {
    // A release can end navigation on its fallback timer before a paint.
    // The viewer also guards pending input until FlushViewEvents; retry
    // after viewChanged, queued behind the actual picked signal.
    const auto retryPickPlaces = [this] {
      if (m_pickPlacesDeferred) logPickPlaces(m_pickPlacesFilter, true);
    };
    connect(m_viewer, &OcctViewer::navigationFinished, this, retryPickPlaces, Qt::QueuedConnection);
    connect(m_viewer, &OcctViewer::viewChanged, this, retryPickPlaces, Qt::QueuedConnection);
    connect(m_viewer, &OcctViewer::coveredChanged, this, [this] {
      if (!(m_pickPlacesFilter & (SelectKind::Face | SelectKind::Edge | SelectKind::Vertex | SelectKind::Profile)) ||
          m_pickPlacesQueued) {
        return;
      }
      m_pickPlacesQueued = true;
      TestSync::singleShot(0, this, [this] {
        m_pickPlacesQueued = false;
        if (m_mode == Mode::Command) {
          logPickPlaces(m_pickPlacesFilter, true);
        }
      });
    });
    // The datums' places keep clear of the cards as they are placed
    // (mitcad#1524: the command card that shows Create Sketch's kind);
    // logged again only when they moved.
    connect(
        m_viewer, &OcctViewer::coveredChanged, this, [this] { logDatumPlaces(); }, Qt::QueuedConnection);
  }
  connect(m_viewer, &OcctViewer::contextMenuRequested, this, &MainWindow::showContextMenu,
          Qt::QueuedConnection);
  connect(m_viewer, &OcctViewer::viewChanged, this, &MainWindow::logBodyPlaces);
  // Components the joints move are dragged in the view (mitcad#55).
  setUpOccurrenceDrag();
  // Sketch selection filters and Shift's wider clicks (mitcad#217).
  setUpSelectionScope();
  // What Create Sketch waits for, over the view (mitcad#474).
  m_modeBanner = new ModeBanner(m_viewer);

  m_registry = new CommandRegistry(this);
  m_sketch = new sketch::SketchController(*this, *m_viewer, *m_registry, this);
  connect(m_sketch, &sketch::SketchController::toolChanged, this, [this] { updateIdleView(); });
  // The tool on shows checked on the ribbon and the palette (mitcad#1027).
  connect(m_sketch, &sketch::SketchController::toolChanged, this,
          [this](const QString& name) { markSketchTool(name); });
  connect(m_sketch, &sketch::SketchController::geometryDragStarted, this,
          [this] {
            m_viewer->removeSketchProfiles(m_sketch->uid());
            updateIdleView(); // selected profiles must not keep their old filled highlight
          });

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

  // 3D mice (mitcad#951): the view moves by the device's axes (pan and
  // zoom only in sketch mode), its buttons run view commands as their
  // actions do. It starts once the saved navigation is applied, and follows
  // Preferences.
  SpaceMouseController::Hooks spaceMouseHooks;
  spaceMouseHooks.settings = [this] { return m_viewer->navigation().spaceMouse; };
  spaceMouseHooks.flat = [this] { return m_sketch->isActive(); };
  spaceMouseHooks.rates = [this](const SpaceMouseRates& rates) { m_viewer->setSpaceMouseRates(rates); };
  spaceMouseHooks.runCommand = [this](const QString& id) {
    QAction* command = m_registry->action(id);
    if (command == nullptr || !command->isEnabled()) {
      qDebug().noquote() << QStringLiteral("3D mouse: %1 is not available now").arg(id);
      return;
    }
    command->trigger();
  };
  m_spaceMouse = new SpaceMouseController(std::move(spaceMouseHooks), &createSpaceMouseBackend, this);
  connect(m_viewer, &OcctViewer::navigationChanged, m_spaceMouse, [this] { m_spaceMouse->refresh(); });
  QTimer::singleShot(0, m_spaceMouse, [this] { m_spaceMouse->refresh(); });

  // Enter confirms the active command (or what a sketch tool has typed)
  // wherever the focus is; Esc cancels it, or steps back in a
  // sketch tool, or clears the selection. With nothing to confirm, Enter
  // repeats the last command (mitcad#1620) unless the focus takes Enter
  // itself: then the action is disabled, and the focus gets the key.
  m_confirmAction = new QAction(tr("OK"), this);
  m_confirmAction->setShortcuts({QKeySequence(Qt::Key_Return), QKeySequence(Qt::Key_Enter)});
  connect(m_confirmAction, &QAction::triggered, this, &MainWindow::confirm);
  addAction(m_confirmAction);
  m_focusRepeat = connect(qApp, &QApplication::focusChanged, this, [this] {
    if (m_mode == Mode::Idle) {
      updateConfirmEnabled();
    }
  });
  m_cancelAction = new QAction(tr("Cancel"), this);
  m_cancelAction->setShortcut(Qt::Key_Escape);
  connect(m_cancelAction, &QAction::triggered, this, &MainWindow::cancel);
  addAction(m_cancelAction);

  // What expression fields offer while they are typed in, and their Create
  // User Parameter (mitcad#567, mitcad#568; framework/ParameterField.hpp):
  // the design's parameters now, and a user parameter as the parameter
  // table adds one in the window's mode (runParameterCommand, mitcad#564:
  // a panel's session runs it outside its undo range and previews again; in
  // a sketch it is an undo step of the sketch).
  // The reads throw ModelBusy while a job computes the model (a field's
  // assistance timer may fire during a preview): the fields keep what they
  // read last and skip the assistance until the next change.
  setParameterSource(this, {[this] {
                              if (modelBusy()) throw ModelBusy();
                              return queryArray(QStringLiteral("parameters"));
                            },
                            [this](const QJsonObject& command, QString* error) {
                              if (runParameterCommand(command)) {
                                return true;
                              }
                              if (error != nullptr) {
                                *error = lastCancelled() ? tr("The computation was canceled.") : mitcad::modelMessage(lastError());
                              }
                              return false;
                            },
                            // The unit the window last read while a job
                            // computes: Up or Down in a panel's length field
                            // during its preview read the document here
                            // and ended the application (mitcad#1360).
                            [this] { return lengthUnit(); },
                            [this](const QJsonObject& query) {
                              if (modelBusy()) throw ModelBusy();
                              return queryObject(query);
                            },
                            [this](const QJsonObject& parameter) {
                              const ParameterEditScope scope=parameterScope();
                              return scope.holds(parameter)
                                  ? scope.holdReason(parameter.value(QStringLiteral("name")).toString()) : QString();
                            }});

  if (m_nativeUi) {
    menuBar()->setNativeMenuBar(false);
    menuBar()->hide();
  }
  createMenus();
  createRibbon();
  createPanels();
  setUpSketch3d(); // 3D sketches (mitcad#1524): their palette page and Create Sketch's kind row
  m_controller->timeline()->setFeatureDetailsProvider([this](const QString& uid) {
    const auto* feature = m_snapshot.feature(uid);
    if (!feature) return QString();
    return modelBusy() ? feature->name.toHtmlEscaped()
                       : featurePresentation(*feature, m_snapshot, *this, *m_registry);
  });
  connect(m_controller->timeline(), &TimelineWidget::featuresHovered, this, [this](const QStringList& features) {
    m_timelineHover = features;
    // The places its warning names (mitcad#1327), with its tooltip.
    QJsonArray places;
    for (const QString& uid : features) {
      if (const auto* feature = m_snapshot.feature(uid)) {
        for (const QJsonValue& place : feature->places) {
          places.append(place);
        }
      }
    }
    if (!places.isEmpty() || m_noticeHoverSource == QStringLiteral("timeline")) {
      hoverNoticePlaces(QStringLiteral("timeline"), places, {});
    }
    if (!modelBusy()) updateIdleView();
  });
  // The status line's message paints its feature's notice places while
  // the pointer is over it (mitcad#1327).
  installNoticeHover();
  installMacToolButtonStyle(this);
  if (!m_nativeUi && chromeStyle() == ChromeStyle::Docked) {
    statusBar()->insertPermanentWidget(0, m_controller->failures());
    statusBar()->insertPermanentWidget(1, m_controller->warnings());
    // Raised panels, their titles, one accent colour, the Windows 11 frame
    // (mitcad#251).
    WindowStyle::install(this);
    // The magnifier on the Browser's title row, before its float and close
    // buttons (mitcad#350: Find in Design).
    if (auto* dock = findChild<QDockWidget*>(QStringLiteral("browserDock"), Qt::FindDirectChildrenOnly)) {
      if (auto* bar = qobject_cast<PanelTitleBar*>(dock->titleBarWidget())) {
        QToolButton* find = m_controller->findButton();
        find->setIconSize(uiIconSize(14, 14));
        bar->addButton(find);
      }
    }
  } else if (auto* titleBar = findChild<TitleBar*>()) {
    // Beside the document's name in the title bar row.
    titleBar->setIndicator(m_indicator);
    titleBar->setAgentIndicator(m_agentIndicator);
  }
  // Look At, Fit, Visual Style and the grid's toggles at the bottom of the
  // view (mitcad#1086, MainWindowNavigationBar.cpp).
  createNavigationBar();
  // The version over the view's lower right (mitcad#1209).
  m_versionWatermark = new VersionWatermark(m_viewer);
  m_search = new CommandSearch(*m_registry, this);
  connect(m_search, &CommandSearch::chosen, this, &MainWindow::trigger, Qt::QueuedConnection);
  createAutosave();
  createLive();
  // The chat panel (mitcad#947).
  createChat();
  // The debug log in the project indicator (mitcad#269).
  createDebugLog();
  // A wait cursor left on is cleared once the window is idle (mitcad#268).
  createWaitCursorRecovery();
  // A dock or a toolbar shown or hidden (its close button, the window's
  // menu of its panels, View) moves the view without a resize: the layout
  // is logged again then too, once the first line is (mitcad#1451: a
  // hidden ribbon left no line of its own).
  if (!m_nativeUi) {
    for (QWidget* child : findChildren<QWidget*>(QString(), Qt::FindDirectChildrenOnly)) {
      QAction* toggle = nullptr;
      if (auto* dock = qobject_cast<QDockWidget*>(child)) {
        toggle = dock->toggleViewAction();
      } else if (auto* bar = qobject_cast<QToolBar*>(child)) {
        toggle = bar->toggleViewAction();
      }
      if (toggle != nullptr) {
        connect(toggle, &QAction::toggled, this, [this] {
          if (m_loggedWindowSize.isValid()) {
            TestSync::singleShot(0, this, [this] { logLayout(); });
          }
        });
      }
    }
  }

#ifdef MITCAD_MACOS_NATIVE_UI
  if (m_nativeUi) {
    createNativeWindow();
  }
#endif

  recompute();
  // The untitled design at the start is a new design too (mitcad#898),
  // framed as every new design is (mitcad#1000).
  if (!demo) {
    logNewDesignActive();
    m_viewer->frameNewDesign();
  }
  showHint(m_registry->shortcutWarnings().isEmpty()
               ? tr("Create a sketch to start modeling. %1 finds commands.")
                     .arg(m_registry->shortcutHint(QStringLiteral("tools.search")))
               : tr("Some saved keyboard shortcuts conflict and are disabled. Open Keyboard Shortcuts "
                    "to repair them; commands remain available in menus."));
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
  } else if (m_titleRow != nullptr && m_floating != nullptr) {
    // Windows and Linux (mitcad#425): the notices in a card of their own
    // below the group cards, shown while one of them is.
    auto* notices = new QWidget;
    auto* column = new QVBoxLayout(notices);
    column->setContentsMargins(0, 0, 0, 0);
    column->setSpacing(0);
    QList<QWidget*> bars;
    for (const char* name : {"updateBar", "remoteBar", "liveBar", "lockBar"}) {
      if (auto* bar = findChild<QToolBar*>(QLatin1String(name), Qt::FindDirectChildrenOnly)) {
        const bool shown = !bar->isHidden();
        removeToolBar(bar);
        column->addWidget(bar);
        bar->setVisible(shown);
        bars << bar;
      }
    }
    if (bars.isEmpty()) {
      delete notices;
    } else {
      auto* card = new GlassCard;
      card->setObjectName(QStringLiteral("noticeCard"));
      card->setContent(notices, QMargins(8, 4, 8, 4));
      m_floating->addTopRow(card, {}, bars);
    }
  }
}

MainWindow::~MainWindow() {
  // Insert › Component from Web's window (mitcad#1579) is a window of its
  // own whose imports go into this one: it ends first, its downloads' files
  // with it.
  delete m_componentBrowser.data();
  // No more Enter's state on focus changes while the children go.
  disconnect(m_focusRepeat);
  // The section worker's rounds end first (mitcad#438): their ends call
  // back into the window.
  m_documentSession->setBeforeJob({});
  delete m_sectionWorker;
  m_sectionWorker = nullptr;
#ifdef MITCAD_MACOS_NATIVE_UI
  m_nativeForm.reset();
  m_nativeIdleForm.reset();
  m_nativeSketchFooterForm.reset();
  m_nativeWindow.reset();
  bindNativeDialogWindow(this, nullptr);
#endif
  // The Selection Filter's menu ends while the window's members still live:
  // left to the window's children, it ended after the sketch controller,
  // and handing its panel back (QWidgetAction) changed the panel's palette,
  // whose refresh asked the gone controller for the scope; an access
  // violation at the exit when the freed memory was gone (mitcad#270).
  if (m_sketchFilterAction != nullptr) {
    delete m_sketchFilterAction->menu();
  }
  m_selectionFilterPanel = nullptr;
  // The session refers to the window as its host.
  delete m_session;
}

void MainWindow::present() {
#ifdef MITCAD_MACOS_NATIVE_UI
  if (m_nativeWindow) {
    m_nativeWindow->show();
    return;
  }
#endif
  mac::prepareDocumentWindowPresentation(this, property("mitcadSeparateWindow").toBool());
  const bool shownBefore = isVisible();
  show();
  // Its frame within the screen once it is known (mitcad#1556).
  if (!shownBefore) capNewWindowToScreen(this);
  mac::enableDocumentWindowTabs(this, [this](bool visible) { setDocumentVisible(visible); });
}

void MainWindow::logLayout() const {
  const auto place = [](const QWidget* widget) {
    const QRect r = widget->geometry();
    return QStringLiteral("%1,%2 %3x%4").arg(r.x()).arg(r.y()).arg(r.width()).arg(r.height());
  };
  const QSize minimum = minimumSizeHint().expandedTo(minimumSize());
  QStringList parts{QStringLiteral("window %1x%2, minimum %3x%4")
                        .arg(width())
                        .arg(height())
                        .arg(minimum.width())
                        .arg(minimum.height())};
  if (menuWidget() != nullptr && menuWidget()->isVisible()) {
    parts << QStringLiteral("menu ") + place(menuWidget());
  }
  if (centralWidget() != nullptr) {
    parts << QStringLiteral("central ") + place(centralWidget());
  }
  // The bars and docks, by name, so that the order of the children does
  // not count.
  QStringList bars;
  for (const QWidget* child : findChildren<QWidget*>(QString(), Qt::FindDirectChildrenOnly)) {
    if ((qobject_cast<const QDockWidget*>(child) != nullptr || qobject_cast<const QToolBar*>(child) != nullptr ||
         qobject_cast<const QStatusBar*>(child) != nullptr) &&
        child->isVisible()) {
      const QString name = child->objectName();
      bars << (name.isEmpty() ? QString::fromLatin1(child->metaObject()->className()) : name) + QLatin1Char(' ') +
                  place(child);
    }
  }
  bars.sort();
  parts << bars;
  m_loggedWindowSize = size();
  qDebug().noquote() << QStringLiteral("Window layout: ") + parts.join(QStringLiteral("; "));
}

QMenu* MainWindow::createPopupMenu() {
  QMenu* menu = QMainWindow::createPopupMenu();
  if (menu == nullptr) {
    return nullptr;
  }
  QStringList entries;
  for (const QAction* entry : menu->actions()) {
    if (!entry->isSeparator() && entry->isVisible()) {
      QString text = entry->text();
      entries << text.remove(QLatin1Char('&'));
    }
  }
  // Only the hidden toggles of the notice bars and the ribbon's toolbar:
  // no menu at all rather than an empty one.
  if (entries.isEmpty()) {
    delete menu;
    return nullptr;
  }
  menu->setObjectName(QStringLiteral("windowContextMenu"));
  qDebug().noquote() << QStringLiteral("Window context menu: ") + entries.join(QStringLiteral(" | "));
  return menu;
}

void MainWindow::resizeEvent(QResizeEvent* event) {
  QMainWindow::resizeEvent(event);
  // Only after the first line (main.cpp logs it once the window shows), and
  // once the resizes in a row are done.
  if (!m_nativeUi && m_loggedWindowSize.isValid() && size() != m_loggedWindowSize) {
    TestSync::singleShot(0, this, [this] {
      if (size() != m_loggedWindowSize) {
        logLayout();
      }
    });
  }
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

  // 2D unless the kind is changed on the card while it waits for the plane
  // (mitcad#1524).
  CommandDef sketch = action("sketch.create", tr("Create Sketch"), "sketch", CommandDef::Mode::Model, [this] {
    setSketchKind(false);
    startSketch();
  });
  sketch.tooltip = tr("Starts a sketch on a plane or a planar face: pick it, or select it first");
  sketch.tab = QStringLiteral("SOLID");
  sketch.group = QStringLiteral("CREATE");
  sketch.section = QStringLiteral("sketch"); // a section of its own (mitcad#579)
  sketch.pinned = true;
  sketch.keywords = {QStringLiteral("new sketch"), QStringLiteral("draw")};
  // Alt+S, Option+S on macOS (mitcad#267): no menu has S as its mnemonic,
  // and S, Ctrl+S, Ctrl+Shift+S and Ctrl+Alt+S stay as they are.
  sketch.shortcut = QKeySequence(Qt::ALT | Qt::Key_S);

  // The SOLID tab's families (U4), in menu order: Create Sketch goes
  // into CREATE after New Component.
  registerCreateCommands(*m_registry, *this, sketch);
  // Create Sketch with 3D chosen (mitcad#1524): from the command search,
  // also on a selected plane.
  CommandDef sketch3d = action("sketch.create_3d", tr("Create 3D Sketch"), "sketch", CommandDef::Mode::Model, [this] {
    setSketchKind(true);
    startSketch();
  });
  sketch3d.tooltip = tr("Starts a 3D sketch, whose points and curves can leave its plane");
  sketch3d.keywords = {QStringLiteral("3d sketch"), QStringLiteral("spatial sketch"), QStringLiteral("route")};
  m_registry->add(sketch3d);
  // The 3D sketch manipulator's keys (mitcad#1661).
  registerSketch3dKeys();
  // Gear (mitcad#472) ends the CREATE group, in its Machine Elements
  // submenu (mitcad#579).
  registerGearCommands(*m_registry, *this, *this);
  // Pulley (mitcad#562) after it.
  registerPulleyCommands(*m_registry, *this);
  // Bearing (mitcad#577) after Pulley.
  registerBearingCommands(*m_registry, *this);
  registerModifyCommands(*m_registry, *this);
  // SHEET METAL (mitcad#210): its features, then the main window's actions.
  registerSheetMetalCommands(*m_registry, *this);
  registerSheetActions();
  // SURFACE (mitcad#455).
  registerSurfaceCommands(*m_registry, *this);
  registerAssembleCommands(*m_registry, *this);
  registerConstructCommands(*m_registry, *this);
  registerInspectCommands(*m_registry, *this, this, [this] { return m_selection; });
  // Center of Mass, after Physical Properties (mitcad#1094).
  registerCenterOfMassCommand();
  // Add Comment after it, and the comments' View menu commands (mitcad#1119).
  registerCommentCommands();
  registerSketchCommands(*m_registry, *m_sketch);
  // Redefine Sketch Plane (mitcad#787).
  registerSketchPlaneCommands(*m_registry, *m_sketch);
  // The BUILD tab (mitcad#375).
  registerBuildCommands(*m_registry, *this, *this, this, [this] { return documentName(); });

  CommandDef removeDef =
      action("inspect.remove_section", tr("Remove Section Analysis"), "section-analysis",
             CommandDef::Mode::Model, [this] { removeSection(); });
  removeDef.tooltip = tr("Hides the section analysis shown: the bodies whole again");
  removeDef.tab = QStringLiteral("SOLID");
  removeDef.group = QStringLiteral("INSPECT");
  removeDef.keywords = {QStringLiteral("section"), QStringLiteral("clip")};
  removeDef.enabled = [this] { return sectionShown(); };
  m_registry->add(removeDef);
  CommandDef flip = action("inspect.flip_section", tr("Flip Section Analysis"), "flip",
                           CommandDef::Mode::Model, [this] { flipSection(); });
  flip.tooltip = tr("Shows the other side of the section");
  flip.tab = QStringLiteral("SOLID");
  flip.group = QStringLiteral("INSPECT");
  flip.keywords = {QStringLiteral("section"), QStringLiteral("clip"), QStringLiteral("other side")};
  flip.enabled = [this] { return sectionShown(); };
  m_registry->add(flip);
  // ASSEMBLE (mitcad#55): after Drive Joint.
  CommandDef animate = action("assemble.animate_joint", tr("Animate Joint"), "drive-joint",
                              CommandDef::Mode::Model, [this] { animateSelectedJoint(); });
  animate.tooltip = tr("Plays a joint's free motion through its range");
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
  // Motion studies (mitcad#454): the panel of the first study, else of a
  // new one (the browser's Play opens another).
  CommandDef study = action("assemble.motion_study", tr("Motion Study…"), "motion-study", CommandDef::Mode::Model,
                            [this] { openMotionStudy(QString()); });
  study.tooltip = tr("Keys joints' values along steps and plays them back");
  study.tab = QStringLiteral("SOLID");
  study.group = QStringLiteral("ASSEMBLE");
  study.keywords = {QStringLiteral("motion"), QStringLiteral("play"), QStringLiteral("simulate"),
                    QStringLiteral("animation"), QStringLiteral("keys")};
  study.enabled = [this] {
    const QJsonArray joints = m_snapshot.joints.value(QStringLiteral("joints")).toArray();
    return !m_snapshot.motionStudies.isEmpty() ||
           std::any_of(joints.begin(), joints.end(), [](const QJsonValue& joint) {
             return !joint.toObject().value(QStringLiteral("motions")).toArray().isEmpty();
           });
  };
  m_registry->add(study);

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
#ifdef MITCAD_MAC_UI
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

  CommandDef fit = action("view.fit", tr("Fit"), "fit", CommandDef::Mode::Any, [this] { fitView(); });
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
  // The Make tab's Annotate group (mitcad#1049, MainWindowMake.cpp), before
  // the Annotations window that the View commands put last in it.
  registerAnnotateCommands();
  // Standard views, visual styles, the camera, the grid, the environment,
  // named views, Preferences and the shortcut overview (U5).
  m_viewController->registerCommands();
  // View › Show: origins, construction and sketches of the whole design
  // (mitcad#1287, MainWindowVisibility.cpp).
  registerViewShowCommands();
  // Import, export and the INSERT group (U6).
  registerFileCommands();
  // New Project, Save with Description, Start Version History (P12d).
  registerVersionCommands();
  // Component libraries (mitcad#64, mitcad#63).
  registerLibraryCommands();

  CommandDef search = action("tools.search", tr("Command Search"), "search", CommandDef::Mode::Any,
                             [this] { openCommandSearch(); });
  search.shortcut = QKeySequence(Qt::Key_S);
  search.tooltip = tr("Find a command by name");
  // While a panel is open too: what runs beside it (mitcad#564: Change
  // Parameters), the rest shown disabled.
  search.duringCommands = true;
  m_registry->add(search);
  // The timeline's search (mitcad#179); Ctrl+F (Cmd+F) where the timeline
  // or the browser has the keys, so no shortcut of its own.
  CommandDef timelineSearch = action("view.search_timeline", tr("Search Timeline"), "search", CommandDef::Mode::Any,
                                     [this] {
#ifdef MITCAD_MACOS_NATIVE_UI
                                       if (m_nativeWindow) {
                                         m_nativeWindow->showTimelineSearch();
                                         return;
                                       }
#endif
                                       m_controller->timeline()->openSearch();
                                     });
  timelineSearch.tooltip = tr("Finds features by name, type, component and status (%1)")
                               .arg(QKeySequence(QKeySequence::Find).toString(QKeySequence::NativeText));
  timelineSearch.keywords = {QStringLiteral("find"), QStringLiteral("feature"), QStringLiteral("history"),
                             QStringLiteral("failed"), QStringLiteral("warning")};
  timelineSearch.duringCommands = true;
  timelineSearch.enabled = [this] { return m_controller != nullptr; };
  m_registry->add(timelineSearch);
  // Find in Design (mitcad#350): components and bodies by name, and what
  // to do with all of them; also the magnifier on the browser's title row.
  CommandDef findInDesign = action("view.find_in_design", tr("Find in Design…"), "search", CommandDef::Mode::Any,
                                   [this] { m_controller->openFind(); });
  findInDesign.tooltip = tr("Finds components and bodies by name and acts on all of them at once");
  findInDesign.keywords = {QStringLiteral("find"), QStringLiteral("body"), QStringLiteral("component"),
                           QStringLiteral("name"), QStringLiteral("hide"), QStringLiteral("show"),
                           QStringLiteral("wildcard")};
  findInDesign.duringCommands = true;
  findInDesign.enabled = [this] { return m_controller != nullptr; };
  m_registry->add(findInDesign);
  // Property Sets (mitcad#496): the typed properties of the selected body
  // or component with its occurrence's and its definition's; the design's
  // (the root's) when nothing is selected. Also in the browser's and the
  // view's context menus.
  CommandDef propertySets =
      action("view.property_sets", tr("Property Sets…"), "parameters", CommandDef::Mode::Model, [this] {
        PropertyTarget target{QString(), QString(), QStringLiteral("C0")};
        if (!m_selection.isEmpty()) {
          const SelectionItem& first = m_selection.first();
          if (first.kind == SelectKind::Component) {
            target = {QString(), first.occurrence, first.occurrence.isEmpty() ? first.owner : QString()};
          } else if (first.kind == SelectKind::Body || first.kind == SelectKind::Face ||
                     first.kind == SelectKind::Edge || first.kind == SelectKind::Vertex) {
            target = {first.owner, first.occurrence, QString()};
          }
        }
        m_controller->openPropertySets(target);
      });
  propertySets.tooltip = tr("Typed properties of the selected body or component and its definition");
  propertySets.keywords = {QStringLiteral("properties"), QStringLiteral("property set"), QStringLiteral("metadata"),
                           QStringLiteral("attributes"), QStringLiteral("part number"), QStringLiteral("bim")};
  propertySets.enabled = [this] { return m_controller != nullptr; };
  m_registry->add(propertySets);

  CommandDef shortcuts = action("tools.shortcuts", tr("Keyboard Shortcuts…"), "keyboard",
                                CommandDef::Mode::Any, [this] { editShortcuts(); });
  shortcuts.keywords = {QStringLiteral("keys"), QStringLiteral("hotkeys")};
  m_registry->add(shortcuts);

  // The browser and the timeline (U3).
  CommandDef parameters = action("solid.parameters", tr("Edit Parameters…"), "parameters",
                                 CommandDef::Mode::Model, [this] { openParameterTable(); });
  parameters.tooltip = tr("User and model parameters: names, expressions, units and comments");
  parameters.tab = QStringLiteral("SOLID");
  parameters.group = QStringLiteral("MODIFY");
  parameters.keywords = {QStringLiteral("parameters"), QStringLiteral("user parameter"),
                         QStringLiteral("expression"), QStringLiteral("fx"), QStringLiteral("change parameters")};
  // Also while a panel is open, which stays open (mitcad#564); not in a
  // read-only window (duringCommands alone would let it in).
  parameters.duringCommands = true;
  parameters.enabled = [this] { return !windowReadOnly(); };
  m_registry->add(parameters);
  // Configurations (mitcad#807): the configuration table's own window
  // (Options, Rows, Rules), beside Change Parameters; its options are also
  // chosen in the browser's Configuration bar.
  CommandDef configurations = action("solid.configurations", tr("Configurations…"), "configurations",
                                     CommandDef::Mode::Model, [this] { m_controller->openConfigurations(); });
  configurations.tooltip = tr("Options, rows and rules: the design as a family of products");
  configurations.tab = QStringLiteral("SOLID");
  configurations.group = QStringLiteral("MODIFY");
  configurations.keywords = {QStringLiteral("configuration"), QStringLiteral("configuration table"),
                             QStringLiteral("option"),        QStringLiteral("variant"),
                             QStringLiteral("rows"),          QStringLiteral("rules"),
                             QStringLiteral("family")};
  configurations.enabled = [this] { return !windowReadOnly(); };
  m_registry->add(configurations);
  // SKETCH > MODIFY > Change Parameters (mitcad#564; MainWindowParameterEdits.cpp).
  registerParameterEditCommands();
  // The active component's design policy (mitcad#220); the browser's
  // Design Policy... opens it for any component.
  CommandDef policy = action("assembly.policy", tr("Design Policy…"), "component", CommandDef::Mode::Model,
                             [this] { m_controller->openPolicy(m_snapshot.activeComponent); });
  policy.tooltip = tr("Sets what the active component may hold and link to");
  policy.keywords = {QStringLiteral("policy"), QStringLiteral("permissions"), QStringLiteral("assembly"),
                     QStringLiteral("part"), QStringLiteral("attach"), QStringLiteral("hybrid")};
  m_registry->add(policy);
  // Reorient Design (mitcad#502): also in the root's context menu.
  CommandDef reorient = action("assembly.reorient_design", tr("Reorient Design…"), "orientation-cube",
                               CommandDef::Mode::Model, [this] { m_controller->openReorient(); });
  reorient.tooltip = tr("Turns the whole design by 90, 180 or 270 degrees about an origin axis");
  reorient.keywords = {QStringLiteral("rotate"), QStringLiteral("turn"), QStringLiteral("orientation"),
                       QStringLiteral("origin plane"), QStringLiteral("up axis"), QStringLiteral("wrong plane")};
  m_registry->add(reorient);
  // Appearances with their physically based parameters (mitcad#46): the
  // library window, which assigns nothing (the Appearance tool does,
  // mitcad#1059).
  CommandDef appearances = action("solid.appearances", tr("Edit Appearances…"), "appearance",
                                  CommandDef::Mode::Model, [this] { m_controller->openAppearances({}); });
  appearances.tooltip = tr("Creates, edits and deletes appearances");
  appearances.tab = QStringLiteral("SOLID");
  appearances.group = QStringLiteral("MODIFY");
  appearances.keywords = {QStringLiteral("material"), QStringLiteral("colour"), QStringLiteral("color"),
                          QStringLiteral("render"), QStringLiteral("glass"), QStringLiteral("metal")};
  m_registry->add(appearances);
  // Materials (mitcad#655, print/MaterialsDialog.cpp): the Make tab's
  // Materials group (mitcad#1049, the id kept), the library window; the
  // Material tool assigns (solid.material, mitcad#1059).
  CommandDef materials = action("solid.materials", tr("Edit Materials…"), "material", CommandDef::Mode::Model,
                                [this] { m_controller->openMaterial({}); });
  materials.tooltip = tr("Creates, edits, imports and deletes materials");
  materials.tab = QStringLiteral("MAKE");
  materials.group = QStringLiteral("MATERIALS");
  materials.pinned = true;
  materials.keywords = {QStringLiteral("material"), QStringLiteral("density"), QStringLiteral("filament"),
                        QStringLiteral("print"),    QStringLiteral("plastic"), QStringLiteral("metal"),
                        QStringLiteral("library"),  QStringLiteral("physical material"), QStringLiteral("mass"),
                        QStringLiteral("my library")};
  m_registry->add(materials);
  // The rest of the Make tab (mitcad#1049, MainWindowMake.cpp): the
  // Material tool's entry in the Materials group, the Annotate group,
  // Export Face as DXF.
  registerMakeCommands();
  // Tools › My Library… went with mitcad#1135: the Materials, Printers and
  // Processes managers (the Make tab) show My library beside the design,
  // and work on it alone when no design is open.

  // The caches of computed results (P7d).
  CommandDef diagnostics = action("help.diagnostics", tr("Diagnostics…"), "measure", CommandDef::Mode::Any,
                                  [this] { showDiagnostics(); });
  diagnostics.tooltip = tr("What the caches of computed results hold and do, and the program's memory");
  diagnostics.keywords = {QStringLiteral("cache"), QStringLiteral("memory"), QStringLiteral("disk"),
                          QStringLiteral("performance")};
  diagnostics.duringCommands = true;
  m_registry->add(diagnostics);
  m_viewController->showDiagnostics = [this] { showDiagnostics(); };
  // Preferences' Layout (mitcad#419).
  m_viewController->restartRequested = [this] { restartApplication(); };
  CommandDef assistant = action("help.connect_assistant", tr("Connect AI Assistant…"), "settings", CommandDef::Mode::Any,
                                [this] { showConnectAssistant(); });
  assistant.tooltip = tr("Connects a local AI assistant to the open design");
  assistant.keywords = {QStringLiteral("claude"), QStringLiteral("codex"), QStringLiteral("chatgpt"),
                        QStringLiteral("mcp"), QStringLiteral("agent"), QStringLiteral("ai")};
  assistant.duringCommands = true;
  m_registry->add(assistant);
  // Help › User Guide (mitcad#1032): docs/user-guide.md on GitHub, at the
  // release's tag (main in a development build; app/CMakeLists.txt).
  CommandDef guide = action("help.user_guide", tr("User Guide"), "web", CommandDef::Mode::Any,
                            [] { openExternalUrl(QUrl(QStringLiteral(MITCAD_GUIDE_URL))); });
  guide.tooltip = tr("Opens the user guide of this version in the web browser");
  guide.keywords = {QStringLiteral("help"), QStringLiteral("manual"), QStringLiteral("documentation"),
                    QStringLiteral("guide"), QStringLiteral("docs"), QStringLiteral("contents")};
  guide.duringCommands = true;
  m_registry->add(guide);
  // Help > Check for Updates (mitcad#9).
  m_updates->registerCommands(*m_registry);
  // Help > Send Feedback (mitcad#61).
  m_reports->registerCommands(*m_registry);
  // Help > Open Log Folder (mitcad#269).
  registerDebugLogCommands();
  // The wait cursor's test hooks (mitcad#268, MITCAD_TEST_WAIT_CURSOR).
  registerWaitCursorTestCommands();
  // Insert: Image Reference and Calibrate Image Reference (mitcad#782).
  registerImageReferenceCommands();
  // View › Chat (mitcad#947).
  registerChatCommands();
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
  host.folder = [this] { return fileDialogDirectory(); };
  CacheDiagnosticsDialog dialog(std::move(host), QApplication::activeModalWidget() != nullptr
                                                      ? QApplication::activeModalWidget()
                                                      : this);
  prepareModal(&dialog);
  execDialog(&dialog);
}

void MainWindow::restartApplication() {
  // Once no model job runs: a close while computing would only cancel it.
  whenIdle(this, [this] {
    qInfo() << "Restart: closing the window";
    if (!close()) {
      qInfo() << "Restart cancelled: MitCad stayed open";
      return;
    }
    // The design again in the new window, as it was saved (or left).
    QStringList extra;
    if (!m_filePath.isEmpty() && QFileInfo::exists(m_filePath)) {
      extra << QStringLiteral("--open") << m_filePath;
    }
    qInfo().noquote() << "Restart:" << scheduleRestart(extra);
    QCoreApplication::quit();
  });
}

void MainWindow::createMenus() {
  createFileMenu();

  QMenu* edit = menuBar()->addMenu(tr("&Edit"));
  edit->setObjectName(QStringLiteral("editMenu"));
  edit->addAction(m_registry->action(QStringLiteral("edit.undo")));
  edit->addAction(m_registry->action(QStringLiteral("edit.redo")));
  edit->addSeparator();
  edit->addAction(m_registry->action(QStringLiteral("solid.parameters")));
  edit->addAction(m_registry->action(QStringLiteral("solid.configurations")));
  edit->addAction(m_registry->action(QStringLiteral("view.search_timeline")));
  edit->addAction(m_registry->action(QStringLiteral("view.find_in_design")));
  QMenu* view = menuBar()->addMenu(tr("&View"));
  view->setObjectName(QStringLiteral("viewMenu"));
  view->addAction(m_registry->action(QStringLiteral("view.fit")));
  QMenu* tools = menuBar()->addMenu(tr("&Tools"));
  tools->setObjectName(QStringLiteral("toolsMenu"));
  tools->addAction(m_registry->action(QStringLiteral("tools.search")));
  tools->addAction(m_registry->action(QStringLiteral("tools.shortcuts")));
  // Component libraries (mitcad#64, mitcad#63).
  createLibraryMenu(tools);
  // For the UI tests: the entries as the menu opens (mitcad#1135: no My
  // Library… since the print managers show My library).
  connect(tools, &QMenu::aboutToShow, this, [tools] {
    QStringList entries;
    for (const QAction* entry : tools->actions()) {
      entries << (entry->isSeparator() ? QStringLiteral("-") : entry->text().remove(QLatin1Char('&')));
    }
    qDebug().noquote() << QStringLiteral("Tools menu: %1").arg(entries.join(QStringLiteral(" | ")));
  });
#ifdef MITCAD_MAC_UI
  createWindowMenu();
#endif
  QMenu* help = menuBar()->addMenu(tr("&Help"));
  // The user guide first (mitcad#1032), then the Keyboard and Mouse
  // Overview (ViewController::createMenus).
  help->addAction(m_registry->action(QStringLiteral("help.user_guide")));
  m_viewController->createMenus(view, tools, help);
  syncViewShowChecks(); // View › Show's check marks (mitcad#1287)
  help->addAction(m_registry->action(QStringLiteral("help.connect_assistant")));
  help->addAction(m_registry->action(QStringLiteral("help.diagnostics")));
  help->addAction(m_registry->action(QStringLiteral("help.open_log_folder")));
  help->addAction(m_registry->action(QStringLiteral("help.check_updates")));
  help->addAction(m_registry->action(QStringLiteral("help.send_feedback")));
  // About and About Qt: on macOS Qt moves them to the application menu
  // (AboutRole, AboutQtRole); elsewhere they end the Help menu.
#ifndef Q_OS_MACOS
  help->addSeparator();
#endif
  QAction* about = help->addAction(tr("&About MitCad"));
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
  // Hidden while Preferences › Display › Show the menu bar is cleared
  // (mitcad#1620, Windows and Linux): its menus stay reachable by Alt and
  // the ☰ button (createRibbon).
  if (!m_nativeUi) {
    MenuBarReveal::instance().watch(menuBar());
  }
}

#ifdef MITCAD_MAC_UI
// The standard Window menu of macOS applications.
void MainWindow::createWindowMenu() {
  QMenu* window = menuBar()->addMenu(tr("&Window"));
  window->setObjectName(QStringLiteral("windowMenu"));
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
  connect(clearAction, &QAction::triggered, this, [this] { clearSelection(); });
  m_ribbon->addGroupExtras(QStringLiteral("SOLID"), QStringLiteral("SELECT"),
                           {windowAction, filterAction, clearAction}, {filterAction});
  // Sketch mode has its own three-state filter (mitcad#217).
  QAction* sketchFilter = createSketchFilterAction();
  m_ribbon->addGroupExtras(QStringLiteral("SKETCH"), QStringLiteral("SELECT"),
                           {windowAction, sketchFilter, clearAction}, {sketchFilter});
  // A cut sheet's tab (mitcad#1008).
  addDrawingRibbonExtras();
  m_ribbon->build();
  // A group menu row's Change Keyboard Shortcut… (mitcad#1001).
  connect(m_ribbon, &Ribbon::shortcutRequested, this, [this](const QString& id) { editShortcuts(id); });
  // The Make tab's body card and Browser markers (mitcad#1049) come and go
  // with the tab.
  connect(m_ribbon, &Ribbon::tabChanged, this, [this](const QString& tab) {
    qDebug().noquote() << QStringLiteral("Ribbon tab shown: %1").arg(tab);
    // The Start page over the view with the Start tab (mitcad#1258).
    updateStartPage(tab);
    updateSelectionCreate(); // none on the Make tab (mitcad#1178)
    updateMakeCard();
    updateMakeMarkers();
  });
  if (m_nativeUi) {
    // AppKit presents the commands; retain only the shared filter state.
    m_ribbon->hide();
    return;
  }

  // The quick-access group (mitcad#1090): the File, Edit, View and Tools
  // menus' commands the user chose, after the tabs in the Floating title
  // row and in the Docked toolbar's tab row.
  QuickAccessGroup::Hooks quickHooks;
  quickHooks.menus = [this] {
    QList<QMenu*> menus;
    for (const char* name : {"fileMenu", "editMenu", "viewMenu", "toolsMenu"}) {
      if (auto* menu = menuBar()->findChild<QMenu*>(QLatin1String(name), Qt::FindDirectChildrenOnly)) {
        menus.append(menu);
      }
    }
    return menus;
  };
  quickHooks.disabledReason = [this](const QString& id) {
    const CommandDef* def = m_registry->find(id);
    return def != nullptr ? unavailableReason(*def) : QString();
  };
  auto* quickAccess = new QuickAccessGroup(*m_registry, std::move(quickHooks));

  // Undo and Redo get the system's symbol on macOS, the drawing elsewhere.
  for (const char* id : {"edit.undo", "edit.redo"}) {
    QAction* action = m_registry->action(QString::fromLatin1(id));
    action->setIcon(mac::symbolIcon(
        QLatin1String(id) == QLatin1String("edit.undo") ? QStringLiteral("arrow.uturn.backward")
                                                          : QStringLiteral("arrow.uturn.forward"),
        action->icon()));
  }

  if (floating) {
#if defined(MITCAD_MAC_UI) && QT_VERSION >= QT_VERSION_CHECK(6, 9, 0)
    // The content goes under the native title bar, which the title bar row
    // replaces (the traffic lights stay). Before the window is shown.
    setWindowFlag(Qt::ExpandedClientAreaHint, true);
    setWindowFlag(Qt::NoTitleBarBackgroundHint, true);
    // Qt would keep the content below the title bar by a content margin.
    setAttribute(Qt::WA_ContentsMarginsRespectsSafeArea, false);
#endif
    auto* titleBar = new TitleBar(m_ribbon, m_registry->action(QStringLiteral("edit.undo")),
                                  m_registry->action(QStringLiteral("edit.redo")),
                                  m_registry->action(QStringLiteral("tools.search")));
    titleBar->setQuickAccess(quickAccess);
    // The ☰ of the hidden menu bar (mitcad#1620), before Undo and Redo,
    // while Preferences › Display › Show the menu bar is cleared.
    titleBar->setMenuButton(new MenuBarButton(menuBar(), uiIconSize(18)));
    titleBar->setMenuButtonVisible(MenuBarButton::wanted());
    connect(&MenuBarNotifier::instance(), &MenuBarNotifier::shownChanged, titleBar,
            [titleBar](bool) { titleBar->setMenuButtonVisible(MenuBarButton::wanted()); });
    if (floatingTopCards()) {
      // Windows and Linux (mitcad#425): only the menu bar is a bar; the title
      // row floats over the view without a card, its controls and the
      // document's name on boxes of their own (mitcad#429), and each of the
      // ribbon's groups is a card (createFloatingPanels places them).
      titleBar->setFloatingRow(true);
      m_titleRow = titleBar;
      m_ribbon->setGroupCards(true);
      return;
    }
    m_header = new QWidget;
    m_header->setObjectName(QStringLiteral("ribbonHeader"));
    auto* column = new QVBoxLayout(m_header);
    column->setContentsMargins(0, 0, 0, 0);
    column->setSpacing(0);
    column->addWidget(titleBar);
    column->addWidget(m_ribbon);
    return;
  }

  QToolBar* bar = addToolBar(tr("Toolbar"));
  bar->setObjectName(QStringLiteral("ribbonToolbar"));
  bar->setMovable(false);
  bar->setFloatable(false);
  // Never offered in the window's menu of its panels (createPopupMenu,
  // which a right-click on the ribbon's tab row opens): its Toolbar entry
  // hid the design's whole ribbon, which nothing brought back but a new
  // design, and the hidden ribbon came back with the design whenever it
  // was in front again (mitcad#1451).
  bar->toggleViewAction()->setVisible(false);
  // The ☰ of the hidden menu bar (mitcad#1620) first, before Undo and Redo,
  // while Preferences › Display › Show the menu bar is cleared.
  QAction* menuButton = bar->addWidget(new MenuBarButton(menuBar(), bar->iconSize().height()));
  menuButton->setVisible(MenuBarButton::wanted());
  connect(&MenuBarNotifier::instance(), &MenuBarNotifier::shownChanged, menuButton,
          [menuButton](bool) { menuButton->setVisible(MenuBarButton::wanted()); });
  // Undo and Redo at hand in the application bar; named for the tests of
  // the bar's colours (mitcad#1245).
  for (const char* id : {"edit.undo", "edit.redo"}) {
    QAction* action = m_registry->action(QString::fromLatin1(id));
    bar->addAction(action);
    if (auto* button = qobject_cast<QToolButton*>(bar->widgetForAction(action))) {
      button->setFocusPolicy(Qt::NoFocus);
      button->setObjectName(QLatin1String(id) == QLatin1String("edit.undo") ? QStringLiteral("ribbonToolbarUndo")
                                                                             : QStringLiteral("ribbonToolbarRedo"));
    }
  }
  if (QWidget* separator = bar->widgetForAction(bar->addSeparator())) {
    separator->setObjectName(QStringLiteral("ribbonToolbarSeparator"));
  }
  bar->addWidget(m_ribbon);
  // The quick-access group in the tabs' row, right of Undo and Redo, as
  // high as the tabs: the groups below keep the toolbar's whole width
  // (mitcad#1090: in the bar itself they collapsed at 1280 px).
  quickAccess->setFocusPolicy(Qt::NoFocus);
  quickAccess->setCompact(m_ribbon->tabBar()->sizeHint().height());
  m_ribbon->addTabRowWidget(quickAccess);
  // Unless the user hid it (mitcad#1189): then the tabs' row closes up.
  quickAccess->applyVisible();
}

void MainWindow::createPanels() {
  if (m_nativeUi) {
    m_controller = new BrowserController(*this, this);
    m_controller->browser()->hide();
    m_controller->timeline()->hide();
    m_commandPages = new QStackedWidget(this);
    m_idleInfo = new QLabel;
    m_idleInfo->setWordWrap(true);
    createIdlePage();
    m_palette = new sketch::SketchPalette(*m_sketch, *m_registry, sketchConstraintCommands());
    m_palettePage = m_palette;
    m_commandPages->addWidget(m_palette);
#ifdef MITCAD_MACOS_NATIVE_UI
    // Match the shared palette: Finish stays below the scrolling inputs.
    if (QWidget* finish = m_palette->takeFinish()) {
      auto* footer = new QWidget(m_commandPages);
      footer->setObjectName(QStringLiteral("nativeSketchFooter"));
      footer->setProperty("nativeLayoutContract", true);
      auto* row = new QHBoxLayout(footer);
      const QMargins inset = m_palette->layout()->contentsMargins();
      row->setContentsMargins(inset.left(), 4, inset.right(), inset.bottom());
      row->addStretch(1);
      row->addWidget(finish);
      finish->show();
      footer->show();
      m_nativeSketchFooter = footer;
    }
#endif
    m_commandPages->hide();
    setCentralWidget(m_viewer);
    return;
  }
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
  // The Projects panel above the browser (mitcad#145), then the
  // Configuration panel between them (mitcad#1020): split from the
  // browser's dock once Projects is above it (a split moves the second dock
  // after the first, so Projects split from the Configuration dock would
  // have gone below the browser). The Configuration dock shows while the
  // design has options or rows.
  dockProjectsPanel(browserDock);
  QDockWidget* configurationDock = dockConfigurationPanel(browserDock);
  if (configurationDock != browserDock) {
    configurationDock->setVisible(m_controller->configurationBar()->isPresent());
  }

  m_commandPages = new QStackedWidget;
  // A fixed width, so that the view keeps its size when a panel opens.
  m_commandPages->setFixedWidth(300);
  m_idleInfo = new QLabel;
  m_idleInfo->setWordWrap(true);
  m_idleInfo->setAlignment(Qt::AlignTop | Qt::AlignLeft);
  m_idleInfo->setMargin(6);
  createIdlePage();
  m_palette = new sketch::SketchPalette(*m_sketch, *m_registry, sketchConstraintCommands());
  // The palette scrolls when it is longer than the dock, as a command's
  // panel does: the stacked widget reserves every page's minimum height,
  // also the hidden palette's, so each row added to the palette made the
  // window taller (mitcad#412).
  m_palettePage = wrapPalette(m_palette, false);
  m_commandPages->addWidget(m_palettePage);
  auto* commandDock = new QDockWidget(tr("Command"), this);
  commandDock->setObjectName(QStringLiteral("commandDock"));
  commandDock->setWidget(m_commandPages);
  addDockWidget(Qt::RightDockWidgetArea, commandDock);
  // View › Command shows it again once its close button or the window's
  // menu of its panels hid it, as View › Browser and View › Timeline do
  // theirs (mitcad#1455: nothing brought it back). After View › Projects,
  // in the order of that menu.
  if (QMenu* view = findChild<QMenu*>(QStringLiteral("viewMenu"))) {
    view->addAction(commandDock->toggleViewAction());
  }
}

void MainWindow::createFloatingPanels() {
  // Cards over the view instead of docks; the same panels inside.
  m_controller = new BrowserController(*this, this);

  auto* browserCard = new GlassCard;
  browserCard->setObjectName(QStringLiteral("browserDock"));
  browserCard->setTitle(tr("Browser"));
  browserCard->setCollapsible(true);
  // The magnifier first (mitcad#350): Find in Design.
  QToolButton* find = m_controller->findButton();
  find->setIconSize(uiIconSize(14, 14));
  browserCard->addHeaderWidget(find);
  find->show();
  // The failed features' badge, in the header: a red capsule (its own
  // style, BrowserController), small.
  QToolButton* failures = m_controller->failures();
  failures->setIconSize(uiIconSize(12, 12));
  browserCard->addHeaderWidget(failures);
  // The warnings' badge beside it, yellow (mitcad#170): its triangle and
  // count only, so that both fit the header (mitcad#961).
  m_controller->setCompactBadges(true);
  QToolButton* warnings = m_controller->warnings();
  warnings->setIconSize(uiIconSize(12, 12));
  browserCard->addHeaderWidget(warnings);
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
  createIdlePage();
  m_palette = new sketch::SketchPalette(*m_sketch, *m_registry, sketchConstraintCommands());
  // The card is as high as its page asks for within the view's height
  // (commandContentHeight); the palette scrolls in it when the card is
  // shorter than its rows (mitcad#470: without, the stacked widget squeezed
  // the rows below their size hint and they overlapped).
  m_palettePage = wrapPalette(m_palette, true);
  m_commandPages->addWidget(m_palettePage);
  m_commandCard = new GlassCard;
  m_commandCard->setObjectName(QStringLiteral("commandDock"));
  // A stacked widget normally reserves the minimum size of every page,
  // including the hidden sketch palette. Size this card for its active page.
  m_commandPages->setSizePolicy(QSizePolicy::Ignored, QSizePolicy::Ignored);
  m_commandCard->setContent(m_commandPages, QMargins(8, 8, 8, 8));
  m_commandCard->hide(); // nothing to say while idle (updateCommandCard)

  m_floating = new FloatingLayout(m_viewer, m_header, this);
  if (m_titleRow != nullptr) {
    // Windows and Linux (mitcad#425): the title row, below it a row of the
    // ribbon's group cards, at the top of the view; the cards below them.
    // Both take the mouse only on their controls and cards.
    m_floating->addTopRow(m_titleRow, [row = m_titleRow] { return row->rowParts(); });
    connect(m_titleRow, &TitleBar::rowPartsChanged, m_floating, &FloatingLayout::requestLayout);
    m_floating->addTopRow(m_ribbon, [ribbon = m_ribbon] { return ribbon->groupCards(); });
    connect(m_ribbon, &Ribbon::groupCardsChanged, m_floating, &FloatingLayout::requestLayout);
  }
  // The Projects card above the browser's (mitcad#145), the Configuration
  // card between them (mitcad#1020).
  addProjectsCard();
  addConfigurationCard();
  // As high as its rows, down to the card below it (the timeline) before
  // it scrolls (mitcad#425), sharing the left column with the cards above
  // it (mitcad#1020). On every platform: macOS's cap of 55 % of the view,
  // kept by the owner on 18.10.2026, was lifted by the owner for
  // mitcad#1020 on 31.10.2026.
  m_floating->addCard(browserCard, FloatingLayout::Anchor::TopLeft,
                      {260, [browser = m_controller->browser()] { return browser->contentHeight(); }, 1.0});
  // The user's split of the left column, as last dragged (mitcad#1020).
  m_floating->setSplitKey(QStringLiteral("floating/leftSplit"));
  m_floating->addCard(timelineCard, FloatingLayout::Anchor::Bottom);
  m_floating->addCard(m_commandCard, FloatingLayout::Anchor::TopRight,
                      {300, [this] { return commandContentHeight(); }, 1.0, false});
  // The message band above the timeline (mitcad#1042); its stub opens the
  // history above it.
  m_messageBand = new MessageBand(*m_messages);
  m_floating->setMessageBand(m_messageBand);
  connect(m_messageBand, &MessageBand::historyRequested, this,
          [this] { toggleMessageHistory(m_messageBand->stubRect(), m_messageBand); });
  // The pointer over its message paints the places of the feature it is
  // about (mitcad#1327).
  connect(m_messageBand, &MessageBand::noticeHovered, this,
          [this](const QString& feature) { previewNoticePlaces(QStringLiteral("message band"), feature); });
  connect(m_controller->browser(), &BrowserPanel::contentHeightChanged, m_floating,
          &FloatingLayout::requestLayout);
  // Its card, or a box behind each row (Preferences, mitcad#419).
  followBrowserBackground(browserCard, m_controller->browser());
  // A design that opens is folded when its rows would not fit the card at
  // its largest (mitcad#426).
  m_controller->browser()->setAvailableHeight(
      [this, browser = m_controller->browser()] { return m_floating->maximumContentHeight(browser); });
  connect(m_commandPages, &QStackedWidget::currentChanged, this, [this] { updateCommandCard(); });
  setCentralWidget(m_floating);
  m_floating->watchContent(m_idlePage);
  if (auto* scroll = qobject_cast<QScrollArea*>(m_idlePage)) {
    m_floating->watchContent(scroll->widget());
  }
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

void MainWindow::createIdlePage() {
  m_idleInfo->setObjectName(QStringLiteral("selectionInfo"));
  m_idleInfo->setTextInteractionFlags(Qt::TextSelectableByMouse);
  m_idlePage = new QWidget;
  auto* layout = new QVBoxLayout(m_idlePage);
  layout->setContentsMargins(0, 0, 0, 0);
  layout->addWidget(m_idleInfo);
  // The selected body's material and print settings on the Make tab
  // (mitcad#1049): under the selection's text.
  m_makeCard = createMakeCard();
  layout->addWidget(m_makeCard);
  // The CREATE commands that fit the selection (mitcad#619): under the
  // selection's text, above Autoselect.
  m_selectionCreate = new SelectionCreatePanel;
  layout->addWidget(m_selectionCreate);
  connect(m_selectionCreate, &SelectionCreatePanel::commandRequested, this, [this](const QString& id) {
    if (m_mode != Mode::Idle) {
      return;
    }
    const CommandDef* def = m_registry->find(id);
    qDebug().noquote() << QStringLiteral("Selection card create chose %1").arg(def != nullptr ? def->name : id);
    // As the ribbon starts it: the selection goes to its inputs.
    trigger(id);
  });
  m_autoselect = new AutoselectPanel;
  layout->addWidget(m_autoselect);
  layout->addStretch();
  connect(m_autoselect, &AutoselectPanel::toggleRequested, this, [this](const Selection& candidates) {
    if (m_mode == Mode::Idle && !modelBusy()) {
      setSelection(toggleSelectionGroup(m_selection, candidates));
    }
  });
  connect(m_autoselect, &AutoselectPanel::clearRequested, this, [this] {
    if (m_mode == Mode::Idle && !modelBusy()) {
      clearSelection();
    }
  });
  connect(m_autoselect, &AutoselectPanel::previewChanged, this, [this](const Selection& candidates) {
    const Selection preview = m_mode == Mode::Idle && !modelBusy() ? candidates : Selection();
    if (preview != m_autoselectPreview) {
      m_autoselectPreview = preview;
      qDebug().noquote() << "Autoselect preview:" << summarize(preview);
    }
    if (!modelBusy()) {
      updateIdleView();
    }
  });
  // The page scrolls when the selection's lines are longer than its place,
  // in the floating command card and in the docked command panel: in a
  // dock, the page's minimum height was the dock's, and a selection of
  // many bodies (8 measured lines) made the window taller than the screen
  // (mitcad#560), as the palette's rows did (mitcad#412). Transparent: the
  // card's glass or the dock's surface shows through, as before.
  if (!m_nativeUi) {
    // As wide as the scroll area shows, never wider (mitcad#1178): a
    // section wider than its place (a drop-down of long names) had its
    // right edge, the group box's frame with it, cut by the card. The
    // sections are squeezed to the card's width instead, every frame whole
    // with the card's margin on both sides.
    m_idlePage->setSizePolicy(QSizePolicy::Ignored, m_idlePage->sizePolicy().verticalPolicy());
    auto* scroll = new QScrollArea;
    scroll->setWidgetResizable(true);
    scroll->setFrameShape(QFrame::NoFrame);
    scroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    scroll->setWidget(m_idlePage);
    scroll->setAutoFillBackground(false);
    scroll->viewport()->setAutoFillBackground(false);
    m_idlePage->setAutoFillBackground(false);
    m_idlePage = scroll;
  }
  m_commandPages->addWidget(m_idlePage);
}

int MainWindow::commandContentHeight() const {
  QWidget* page = m_commandPages->currentWidget();
  auto* scroll = qobject_cast<QScrollArea*>(page);
  int below = 0;
  if (scroll == nullptr && page != nullptr && page != m_idlePage) {
    // A command's page (mitcad#655): its inputs scroll, OK and Cancel stay
    // under them; they add their height.
    scroll = page->findChild<QScrollArea*>(QString(), Qt::FindDirectChildrenOnly);
    if (scroll != nullptr) {
      below = std::max(0, page->sizeHint().height() - scroll->sizeHint().height());
    }
  }
  if (scroll != nullptr) {
    QWidget* content = scroll->widget();
    if (content == nullptr) {
      return below;
    }
    // The width the inputs get: the card's less its margins, the page's
    // side margins (mitcad#655: else a wrapped row is counted too low and
    // the last row hides under OK and Cancel) and a scroll bar's.
    int inset = 0;
    if (page != nullptr && page != scroll && page->layout() != nullptr) {
      const QMargins margins = page->layout()->contentsMargins();
      inset = margins.left() + margins.right();
    }
    const int width = m_commandCard->width() - 16 - inset - scroll->verticalScrollBar()->sizeHint().width();
    const int wrapped = content->hasHeightForWidth() ? content->heightForWidth(width) : -1;
    return (wrapped >= 0 ? wrapped : content->sizeHint().height()) + 2 + below;
  }
  if (page == m_idlePage) {
    const int height = m_idlePage->layout()->totalHeightForWidth(m_commandCard->width() - 16);
    return height >= 0 ? height : m_idlePage->sizeHint().height();
  }
  return page != nullptr ? page->sizeHint().height() : 0;
}

void MainWindow::updateCommandCard() {
  if (m_commandCard == nullptr) {
    return;
  }
  const bool idle = m_commandPages->currentWidget() == m_idlePage;
  m_commandCard->setVisible(!(idle && m_idleInfo->text().isEmpty() && !m_autoselect->available() &&
                              m_selectionCreate->entries().isEmpty() &&
                              (m_sketchKindRow == nullptr || m_sketchKindRow->isHidden())));
  m_floating->requestLayout();
}

void MainWindow::toggleMessageHistory(const QRect& anchor, QWidget* opener) {
  if (m_messageHistory->isVisible()) {
    m_messageHistory->dismiss();
    return;
  }
  m_messageHistory->open(anchor, opener);
}

bool MainWindow::isAvailable(const CommandDef& def) const {
  // A read-only window (mitcad#89): only what changes no design.
  if (windowReadOnly() && !readOnlyCommand(def)) {
    return false;
  }
  if (m_mode == Mode::Command) {
    return def.duringCommands && (!def.enabled || def.enabled());
  }
  // A 3D sketch takes no 2D sketch tools and has no profiles (mitcad#1524).
  if (m_mode == Mode::Sketch && !sketch3dRefusal(def).isEmpty()) {
    return false;
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

QString MainWindow::unavailableReason(const CommandDef& def) const {
  // As isAvailable decides, in its order.
  if (windowReadOnly() && !readOnlyCommand(def)) {
    return tr("the design is open read-only");
  }
  if (m_mode == Mode::Command && !def.duringCommands) {
    return tr("finish or cancel the open command first");
  }
  if (m_mode == Mode::Sketch) {
    if (const QString refused = sketch3dRefusal(def); !refused.isEmpty()) {
      return refused;
    }
  }
  if (m_mode != Mode::Command) {
    if (def.mode == CommandDef::Mode::Model && m_mode == Mode::Sketch) {
      return tr("finish the sketch first");
    }
    if (def.mode == CommandDef::Mode::Model && m_mode != Mode::Idle) {
      return tr("finish what is open first");
    }
    if (def.mode == CommandDef::Mode::Sketch && m_mode != Mode::Sketch) {
      return tr("only in a sketch");
    }
  }
  if (def.enabled && !def.enabled()) {
    const QString reason = def.disabledReason ? def.disabledReason() : QString();
    return reason.isEmpty() ? tr("not available now") : reason;
  }
  return QString();
}

void MainWindow::updateActions() {
  m_autoselect->setActive(m_mode == Mode::Idle);
  updateModeBanner();
  // The commands' availability checks share their queries' answers.
  QHash<QString, QJsonValue> memo;
  m_queryMemo = &memo;
  struct Reset {
    MainWindow* window;
    ~Reset() { window->m_queryMemo = nullptr; }
  } reset{this};
  // A command of several contexts (Command Search, Undo) carries its key of
  // the mode's context (mitcad#720), as isAvailable() enables the others.
  m_registry->setShortcutContext(m_mode == Mode::Command  ? unsigned(CommandShortcutContext)
                                 : m_mode == Mode::Sketch ? unsigned(SketchShortcutContext)
                                                          : unsigned(ModelShortcutContext));
  // A 3D sketch's drawing tool: its placement keys win over the same keys
  // of the view (Up is Turn View Up's elsewhere; mitcad#1661).
  const bool drawing3d =
      m_mode == Mode::Sketch && m_sketch3d != nullptr && m_sketch3d->isActive() && m_sketch3d->takesPlacementKeys();
  m_registry->setShortcutOverlay(drawing3d ? unsigned(Sketch3dToolShortcutContext) : 0u);
  m_registry->updateEnabled([this](const CommandDef& def) { return isAvailable(def); });
  // What File › Print prints now, for the UI tests (mitcad#1089).
  logPrintTarget();
  // The card's Create row offers enabled commands only (mitcad#619).
  updateSelectionCreate();
  updateConfirmEnabled();
  updateWindowTitle();
#ifdef MITCAD_MACOS_NATIVE_UI
  if (m_nativeWindow) {
    refreshNativeWindow();
  }
#endif
  m_documentSession->notifyChanged();
}

void MainWindow::trigger(const QString& id) {
  if (modelBusy() && id != QLatin1String("file.exit")) {
    // From the command search (queued) while a job computes: after it.
    whenIdle(this, [this, id] { trigger(id); });
    return;
  }
  // Exit must reach closeEvent while a preview owns the document: that
  // handler cancels the job before asking about unsaved changes. Queuing
  // Exit until idle would instead wait for the preview it needs to cancel.
  const CommandDef* def = m_registry->find(id);
  if (def != nullptr && windowReadOnly() && !readOnlyCommand(*def)) {
    refuseReadOnly(id);
    return;
  }
  if (def == nullptr || !isAvailable(*def)) {
    // A 3D sketch says why a 2D tool does nothing (mitcad#1524), rather
    // than leaving the key or the click unanswered.
    if (const QString refused = (def != nullptr && m_mode == Mode::Sketch) ? sketch3dRefusal(*def) : QString();
        !refused.isEmpty()) {
      showError(tr("%1: %2").arg(def->name, refused));
      qInfo().noquote() << QStringLiteral("3D sketch: %1 refused: %2").arg(def->id, refused);
    }
    return;
  }
  if (m_reports != nullptr) {
    m_reports->noteAction(QStringLiteral("command ") + id);
  }
  // Delete in a 3D sketch removes its own selection (mitcad#1524).
  if (id == QStringLiteral("sketch.delete") && m_mode == Mode::Sketch && m_sketch3d != nullptr &&
      m_sketch3d->isActive()) {
    deleteSketch3dSelection();
    return;
  }
  // So do Copy, Cut and Paste (mitcad#1661).
  if (m_mode == Mode::Sketch && m_sketch3d != nullptr && m_sketch3d->isActive() && runSketch3dClipboard(id)) {
    return;
  }
  // Point, Line, the arcs and the splines draw in space in a 3D sketch
  // (mitcad#1525).
  if (const sketch::Sketch3dTool tool = sketch::Sketch3dController::toolForCommand(id);
      tool != sketch::Sketch3dTool::None && m_mode == Mode::Sketch && m_sketch3d != nullptr &&
      m_sketch3d->isActive()) {
    m_messages->commandStarted();
    setSelection({});
    m_sketch3d->startTool(tool);
    return;
  }
  // Coincident, Tangent, Equal, Fix, Midpoint and Sketch Dimension apply
  // to a 3D sketch's selection (mitcad#1526).
  if (runSketch3dEdit(id)) {
    return;
  }
  // A command with a panel or a tool is the next command: an error the
  // message band shows gives way to it (mitcad#1042); an action (Undo, a
  // view) leaves it.
  if (def->kind != CommandDef::Kind::Action) {
    m_messages->commandStarted();
  }
  m_locks->activity();
  // A command of the 3D view leads back to the model from a drawing
  // (mitcad#707); the menus' commands (Undo, Save, Export) and the
  // drawing's own tabs' (mitcad#1008) and Edit Parameters (mitcad#1582) do
  // not.
  if (leavesDrawing(*def)) {
    leaveDrawing();
  }
  // So does a command of the view from the Start tab (mitcad#1258): a
  // feature, a tool, a ribbon tab's command; the File commands and the
  // menus' others (Open, Import, Undo, Preferences) leave it only when a
  // design opens or something comes in.
  if (def->kind != CommandDef::Kind::Action ||
      (!def->tab.isEmpty() && !def->id.startsWith(QLatin1String("file.")))) {
    leaveStartPage();
  }
  // Each start of a sketch tool counts for the Sketch Palette's Create
  // section (mitcad#1027), kept in the local settings.
  if (m_mode == Mode::Sketch && sketch::SketchPalette::isCreateTool(*def)) {
    sketch::countPaletteTool(def->id);
  }
  if (def->kind == CommandDef::Kind::Feature) {
    startFeatureCommand(*def);
  } else if (def->run) {
    // The sketch tool it starts shows checked (markSketchTool).
    m_startingSketchTool = m_mode == Mode::Sketch && def->kind == CommandDef::Kind::Tool ? id : QString();
    def->run();
    m_startingSketchTool.clear();
  }
}

void MainWindow::markSketchTool(const QString& toolName) {
  // The command whose run started the tool; none for a tool started
  // otherwise (an edit's double-click, Paste).
  const QString active = toolName.isEmpty() ? QString() : m_startingSketchTool;
  if (active == m_checkedSketchTool) {
    return;
  }
  // Checkable only while checked, so that the menus show no check boxes
  // beside the tools that are off.
  if (QAction* before = m_registry->action(m_checkedSketchTool)) {
    before->setChecked(false);
    before->setCheckable(false);
  }
  m_checkedSketchTool = active;
  if (QAction* now = m_registry->action(active)) {
    now->setCheckable(true);
    now->setChecked(true);
  }
}

void MainWindow::startFeatureCommand(const CommandDef& def, const QString& editUid,
                                     const QJsonObject& analysis) {
  if (!readOnlyCommand(def) && refuseReadOnly(editUid.isEmpty() ? def.id : QStringLiteral("editing %1").arg(editUid))) {
    return;
  }
  m_messages->commandStarted(); // an edit too (mitcad#1042)
  // A panel works in the 3D view: back from a drawing (mitcad#707).
  leaveDrawing();
  // The sketch a feature command leaves (mitcad#1227), shown once its card
  // is placed.
  QString sketchLeft;
  if (m_mode == Mode::Sketch && def.mode == CommandDef::Mode::Model) {
    // A feature from sketch mode (Extrude): the sketch is finished first,
    // its selected profiles go to the feature.
    sketchLeft = m_sketch->uid();
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
  // Project's own selection filter context while it runs (mitcad#217).
  beginCommandScope(def);
  const Selection preselection = editUid.isEmpty() && analysis.isEmpty() ? m_selection : Selection();
  setSelection({});
  // A feature that changes the bodies it picks, on bodies of another
  // component selected before it (mitcad#1634): that component is
  // activated first, as its own undo step; the panel says so.
  const QString preselectionNotice = activateForPreselection(def, preselection);
  m_placementNote.clear(); // what this command places, it says (mitcad#292)
  endAnnotationPick(); // an annotation's pick waited for ends with it
  endImageCalibration(true); // and a calibration of an image reference (mitcad#782)
  m_mode = Mode::Command;
  m_documentSession->cancelBackgroundEvaluation(QStringLiteral("a command")); // mitcad#1267
  if (!m_commandInSketch && !editUid.isEmpty()) {
    // The sketches the edited feature uses are shown for its time
    // (mitcad#546): before the session starts, as its roll-back refreshes
    // the scene.
    beginCommandSketches(editUid, Selection(), def.name);
  }
  auto* session = new CommandSession(def, *this, editUid, m_documentSession);
  if (!analysis.isEmpty()) {
    session->editAnalysis(analysis);
  }
#ifdef MITCAD_MACOS_NATIVE_UI
  if (m_nativeWindow) {
    m_session = session;
    m_lastCommand = def.id;
    connect(session, &CommandSession::finished, this, &MainWindow::onCommandFinished);
    session->startAsync(preselection, [this, guarded = QPointer<CommandSession>(session), sketchLeft,
                                       name = def.name](bool started, const QString& error) {
      if (!guarded || m_session != guarded) {
        return;
      }
      if (!started) {
        m_session = nullptr;
        guarded->deleteLater();
        m_sketch->setPaused(false);
        m_mode = m_commandInSketch ? Mode::Sketch : Mode::Idle;
        m_commandInSketch = false;
        endCommandScope();
        showError(error);
        CommandSketches ended = std::exchange(m_commandSketches, CommandSketches());
        refreshScene();
        endCommandSketches(std::move(ended));
        return;
      }
      showStartingSketches(*guarded);
      // The value fields' fx tools use this window's parameters (mitcad#815).
      m_nativeForm = createNativeCommandForm(*guarded, this);
      addParametersButton(*m_nativeForm, guarded->def()); // mitcad#564
      addMeasureHistory(*m_nativeForm, guarded->def());   // mitcad#648
      m_nativeWindow->showInspector(true);
      m_nativeWindow->setInspectorView(m_nativeForm->nativeView(), m_nativeForm->nativeFooterView());
      if (!sketchLeft.isEmpty()) {
        showSketchLeft(sketchLeft, name); // mitcad#1227
      }
      if (!m_nativeFocusInput.isEmpty()) {
        guarded->focusValue(std::exchange(m_nativeFocusInput, QString()));
      } else if (!guarded->startsWithValues() || !guarded->focusFirstValueOnOpen()) {
        m_nativeWindow->focusViewport();
      }
      updateActions();
    });
    return;
  }
#endif
  QString error;
  if (!session->start(preselection, error)) {
    delete session;
    m_sketch->setPaused(false);
    m_mode = m_commandInSketch ? Mode::Sketch : Mode::Idle;
    endCommandScope();
    showError(error);
    CommandSketches ended = std::exchange(m_commandSketches, CommandSketches());
    refreshScene();
    endCommandSketches(std::move(ended));
    return;
  }
  showStartingSketches(*session);
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
  addParametersButton(*panel, def); // mitcad#564
  addMeasureHistory(*panel, def);   // mitcad#648
  if (!preselectionNotice.isEmpty()) {
    session->setActivationNotice(preselectionNotice); // mitcad#1634
  }
  if (m_floating != nullptr) {
    // The card's material shows through; a changed panel resizes the card.
    scroll->viewport()->setAutoFillBackground(false);
    panel->setAutoFillBackground(false);
    m_floating->watchContent(panel);
  }
  // Its places are logged again when it scrolls, for UI tests (a low
  // window, such as the 1024x768 desktop of a Windows test).
  connect(scroll->verticalScrollBar(), &QScrollBar::valueChanged, panel, [panel] {
    TestSync::singleShot(0, panel, [panel] {
      if (panel->isVisible()) {
        panel->logLayout();
      }
    });
  });
  // OK (Send) and Cancel stay in view under the inputs, which scroll above
  // them (mitcad#655: a long 3D Print panel in the Floating card).
  auto* page = new QWidget;
  auto* pageBox = new QVBoxLayout(page);
  const QMargins margins = panel->layout() != nullptr ? panel->layout()->contentsMargins() : QMargins();
  pageBox->setContentsMargins(margins.left(), 0, margins.right(), margins.bottom());
  pageBox->setSpacing(0);
  pageBox->addWidget(scroll, 1);
  if (QWidget* buttons = panel->takeButtons()) {
    pageBox->addWidget(buttons);
  }
  if (m_floating != nullptr) {
    page->setAutoFillBackground(false);
  }
  m_commandPages->addWidget(page);
  m_commandPages->setCurrentWidget(page);
  m_panelScroll = page;
  if (!sketchLeft.isEmpty()) {
    showSketchLeft(sketchLeft, def.name); // mitcad#1227
  }
  if (!session->startsWithValues() || !session->focusFirstValueOnOpen()) {
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
  leaveDrawing(); // a feature is edited in the 3D view (mitcad#707)
  // A feature of a linked design changes only with its file: where to
  // change it, with Open Linked Design (mitcad#1416).
  if (const DocumentSnapshot::Feature* entry = m_snapshot.feature(uid)) {
    const DocumentSnapshot::Component* owner = m_snapshot.component(entry->component);
    if (owner != nullptr && owner->linked && !owner->library) {
      showLinkedNotice(owner->name, owner->link);
      return;
    }
  }
  try {
    const QJsonObject feature =
        queryObject({{QStringLiteral("query"), QStringLiteral("feature")}, {QStringLiteral("uid"), uid}});
    // A planar or a 3D sketch (mitcad#1524) opens in sketch mode.
    const QString type = feature.value(QStringLiteral("type")).toString();
    if (type == QStringLiteral("sketch") || type == QStringLiteral("spatial_sketch")) {
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
    showError(tr("%1 cannot be edited in MitCad yet.").arg(name));
    return;
  }
  startFeatureCommand(*def, uid);
}

void MainWindow::editAnalysis(const QString& name) {
  if (m_mode != Mode::Idle) {
    return;
  }
  const QJsonObject analysis = m_snapshot.analysis(name);
  // Each type in its command's panel: Section Analysis, Print Analysis
  // (mitcad#620).
  const QString type = analysis.value(QStringLiteral("type")).toString();
  const QString command = type == QStringLiteral("section") ? QStringLiteral("inspect.section")
                          : type == QStringLiteral("print") ? QStringLiteral("inspect.print_analysis")
                                                            : QString();
  const CommandDef* def = command.isEmpty() ? nullptr : m_registry->find(command);
  if (analysis.isEmpty() || def == nullptr) {
    return;
  }
  startFeatureCommand(*def, QString(), analysis);
}

void MainWindow::editInstead(const QString& uid, const QString& input) {
  // Not inside the running command's own call: it goes first.
  whenIdle(this, [this, uid, input] {
#ifdef MITCAD_MACOS_NATIVE_UI
    if (m_nativeWindow && m_session) {
      connect(m_session, &CommandSession::finished, this, [this, uid, input] {
        QTimer::singleShot(0, this, [this, uid, input] { editInstead(uid, input); });
      });
      m_session->cancel();
      return;
    }
    if (m_nativeWindow) {
      m_nativeFocusInput = input;
    }
#endif
    if (m_session) {
      m_session->cancel();
    }
    editFeature(uid);
#ifdef MITCAD_MACOS_NATIVE_UI
    if (m_nativeWindow && !m_session) {
      m_nativeFocusInput.clear();
    }
#endif
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

bool MainWindow::offersCommand(const QString& id, const Selection& items) const {
  const CommandDef* def = m_registry->find(id);
  return def != nullptr && def->kind == CommandDef::Kind::Feature && def->offeredAtTop && m_mode == Mode::Idle &&
         isAvailable(*def) && def->offeredAtTop(items);
}

bool MainWindow::chooseInCommand(const QString& command, const QString& input, const QString& value) {
  if (m_mode != Mode::Command || !m_session || m_session->def().id != command) {
    return false;
  }
  return m_session->choose(input, value);
}

void MainWindow::onCommandFinished(bool committed) {
  clearNativeCommandContinuation();
#ifdef MITCAD_MACOS_NATIVE_UI
  if (m_nativeWindow) {
    m_nativeWindow->setInspectorView(nullptr);
    // A control may still be dispatching the action that finished this
    // command. Release its presenter on the following event-loop turn.
    const std::shared_ptr<NativeCommandForm> form(m_nativeForm.release());
    QTimer::singleShot(0, this, [form] {});
  }
#endif
  CommandSession* session = m_session;
  m_session = nullptr;
  // OK or Cancel: the panel's errors go with it (mitcad#1621), before its
  // "Done." or "Command canceled." takes the band.
  closeMessageSource(QStringLiteral("command"));
  // An Annotate panel's preview goes with it (mitcad#1097).
  if (!m_annotationPreview.isEmpty()) {
    previewAnnotation(QJsonObject());
  }
  m_mode = m_commandInSketch && m_sketch->isActive() ? Mode::Sketch : Mode::Idle;
  m_commandInSketch = false;
  m_sketch->setPaused(false);
  // The sketch's selection filter as it was before (mitcad#217).
  endCommandScope();
  m_commandPages->setCurrentWidget(m_mode == Mode::Sketch ? m_palettePage : m_idlePage);
  if (session != nullptr) {
    session->deleteLater();
  }
  if (m_panelScroll) {
    m_panelScroll->deleteLater();
  }
  // Its sketches go back to their light bulbs (mitcad#546).
  CommandSketches sketches = std::exchange(m_commandSketches, CommandSketches());
  refreshScene();
  endCommandSketches(std::move(sketches));
  m_viewer->setFocus();
  // A new component placed beside a part says so rather than "Done."
  // (mitcad#292).
  showHint(!committed ? tr("Command canceled.")
                      : m_placementNote.isEmpty() ? tr("Done.") : m_placementNote);
  m_placementNote.clear();
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
    if (m_sketch3d != nullptr && m_sketch3d->isActive()) {
      m_sketch3d->confirm(); // a 3D sketch tool's typed values (mitcad#1525)
      return;
    }
    if (m_sketch->confirm()) {
      return;
    }
  }
  // Outside commands and sketches nothing waits for Enter: it repeats the
  // last command (mitcad#1620), as the view's context menu's Repeat does,
  // unless the focus takes Enter itself (a field, a rename, a list) or the
  // command is not available now.
  if (repeatableCommand() && !focusTakesEnter(QApplication::focusWidget())) {
    const QString id = m_lastCommand;
    const CommandDef* last = m_registry->find(id);
    qDebug().noquote() << QStringLiteral("Repeat %1 (Enter)").arg(last != nullptr ? last->name : id);
    if (m_reports != nullptr) {
      m_reports->noteAction(QStringLiteral("repeat ") + id);
    }
    trigger(id);
  }
}

bool MainWindow::repeatableCommand() const {
  // Not in a sketch: there Enter ends a tool's step and, pressed once more,
  // must not start a command (a model command such as Extrude would even
  // finish the sketch first).
  if (m_mode != Mode::Idle || m_lastCommand.isEmpty()) {
    return false;
  }
  // Its action is enabled as the command is available (updateActions).
  const QAction* action = m_registry->action(m_lastCommand);
  return action != nullptr && action->isEnabled();
}

void MainWindow::updateConfirmEnabled() {
  // Enter confirms the open command or the sketch's tool; outside them it
  // repeats the last command, only while the focus does not take Enter
  // itself, which then gets it (mitcad#1620).
  const bool repeat = repeatableCommand() && !focusTakesEnter(QApplication::focusWidget());
  m_confirmAction->setEnabled(m_mode == Mode::Command || m_mode == Mode::Sketch || repeat);
}

void MainWindow::cancel() {
  if (m_controller->cancelEditing()) {
    return; // a rename in the browser or the timeline
  }
  if (m_projectsPanel != nullptr && m_projectsPanel->cancelEditing()) {
    return; // a name typed in the Projects panel (mitcad#145)
  }
  if (m_session) {
    // The command's own Esc first (mitcad#857: 3D Print's bed face
    // choice: Esc ends the question, nothing sent).
    if (!m_session->escape()) {
      m_session->cancel();
    }
    return;
  }
  if (cancelAnnotationPick()) {
    return; // an annotation's pick no longer waited for
  }
  if (m_mode == Mode::PickPlane) {
    m_mode = Mode::Idle;
    updateModeBanner(); // the planes' pick order first (mitcad#844)
    inputActivated(SelectFilter());
    updateIdleView();
    updateActions();
    showHint(tr("Create Sketch canceled."));
    qDebug() << "Create Sketch cancelled";
    return;
  }
  if (m_mode == Mode::Sketch) {
    // An editor, a drag, the tool and the selection first (a 3D sketch's
    // tool steps back one layer, mitcad#1525); then, with the preference
    // on (mitcad#1623, off by default), the sketch itself.
    const bool stepped = m_sketch3d != nullptr && m_sketch3d->isActive() ? m_sketch3d->cancel() : m_sketch->cancel();
    if (stepped) {
      return;
    }
    if (!m_selection.isEmpty() || !sketch::escLeavesUnchangedSketch()) {
      setSelection({});
      return;
    }
    escapeSketch();
    return;
  }
  if (escapeDrawing()) {
    return; // an open drawing's Esc (mitcad#707)
  }
  clearSelection();
}

void MainWindow::clearSelection() {
  setSelection({});
  m_controller->timeline()->setSelectedFeatures({});
}

void MainWindow::openCommandSearch() {
  // Also while a panel is open (mitcad#564): its commands that run beside
  // the panel are enabled (CommandDef::duringCommands).
#ifdef MITCAD_MACOS_NATIVE_UI
  if (m_nativeWindow) {
    m_nativeWindow->showCommandSearch();
    return;
  }
#endif
  m_search->popup(m_viewer->mapToGlobal(m_viewer->rect().center()));
}

void MainWindow::editShortcuts(const QString& id) {
  ShortcutDialog dialog(*m_registry, this);
  if (!id.isEmpty()) {
    dialog.showCommand(id);
  }
  prepareModal(&dialog);
  if (execDialog(&dialog) == QDialog::Accepted) {
    updateActions(); // refresh the native menu and its effective shortcut list
  }
}

// ---------------------------------------------------------------------------
// Model access

const Document& MainWindow::idleDocument() const {
  // A job in the background gives way (Send to Slicer's bed plan beside a
  // print window, mitcad#1121).
  if (modelBusy() && !m_documentSession->yieldBackgroundJob()) {
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
  // A job in the background gives way (mitcad#1121); any other is waited
  // for by whoever calls.
  if (modelBusy() && !m_documentSession->yieldBackgroundJob()) {
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
    showHint(tr("Computation canceled."));
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
  // The drawing views' projections it asked for, before the scene and the
  // sheet are drawn again (mitcad#1033).
  for (const QString& line :
       drawings::projectionLines(answer.value(QStringLiteral("projections")).toArray())) {
    qDebug().noquote() << line;
  }
  notePlacement(answer);
  noteKeptParameters(answer); // own parameters kept as user parameters (mitcad#1536)
  if (result != nullptr) {
    *result = answer;
  }
  m_locks->activity(); // an edit keeps the edit lock active (mitcad#89)
  return true;
}

void MainWindow::notePlacement(const QJsonObject& result) {
  const QString warning = result.value(QStringLiteral("placement_warning")).toString();
  if (warning.isEmpty()) {
    return;
  }
  // Shown now, and again when the command's panel closes (onCommandFinished).
  m_placementNote = warning;
  showHint(warning);
  qInfo().noquote() << QStringLiteral("Placement warning: %1").arg(warning);
}

void MainWindow::noteKeptParameters(const QJsonObject& result) {
  // A feature's (`slot`); a sketch dimension's (`dimension`) the sketch
  // says itself (SketchController::noteKeptParameters).
  QStringList moved;
  for (const QJsonValue& value : result.value(QStringLiteral("user_parameters")).toArray()) {
    const QJsonObject kept = value.toObject();
    if (!kept.contains(QStringLiteral("slot"))) {
      continue;
    }
    // "Slot extent.distance: depth kept as a user parameter, now d3", for
    // UI tests.
    qDebug().noquote() << QStringLiteral("Slot %1: %2 kept as a user parameter, now %3")
                              .arg(kept.value(QStringLiteral("slot")).toString(),
                                   kept.value(QStringLiteral("name")).toString(),
                                   kept.value(QStringLiteral("parameter")).toString());
    moved << kept.value(QStringLiteral("name")).toString();
  }
  if (moved.isEmpty()) {
    return;
  }
  // In plain words (mitcad#1612).
  const QStringList notes = shownTexts(result, QStringLiteral("notes"), QStringLiteral("note_messages"));
  // The model's words, shown now and again when the command's panel closes
  // (onCommandFinished), after a placement warning.
  QString note = notes.join(QStringLiteral("; "));
  if (!note.isEmpty() && !note.endsWith(QLatin1Char('.'))) {
    note += QLatin1Char('.');
  }
  m_placementNote = m_placementNote.isEmpty() ? note : QStringLiteral("%1 %2").arg(m_placementNote, note);
  showHint(m_placementNote);
  qInfo().noquote() << QStringLiteral("Feature notice: %1").arg(note);
}

bool MainWindow::runCommands(const QJsonArray& commands, const QString& label) {
  return runModelCommands(commands, label);
}

void MainWindow::showSection(const QJsonValue& plane) {
  if (setSection(plane)) {
    // The bodies clipped at the new plane off the UI thread, shown when
    // they are ready (mitcad#438); at once when nothing needs a clip (the
    // bodies whole, or the clips kept), and what still works for an older
    // plane stops.
    if (!startSectionRound()) {
      if (m_sectionWorker != nullptr) {
        const SectionWorker::Stopped stopped = m_sectionWorker->cancel();
        if (stopped.measure && m_sectionState.measurement) {
          m_sectionState.measurement->posted = false;
        }
      }
      showBodies();
    }
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
  // A shown Print Analysis' overhangs and thin walls (mitcad#620).
  followPrintAnalysis();
}

bool MainWindow::sectionShown() const {
  return m_snapshot.shownAnalysis().value(QStringLiteral("type")).toString() == QStringLiteral("section");
}

bool MainWindow::sketchShown() const {
  // A sketch command's panel puts the window in Mode::Command; the sketch
  // stays on screen (paused), and so do its display options.
  return m_sketch->isActive() && (m_mode == Mode::Sketch || (m_mode == Mode::Command && m_commandInSketch));
}

QJsonValue MainWindow::cutPlane() const {
  if (!sketchShown() || !m_sketch->hideAbove()) {
    return m_section;
  }
  const geometry::Frame& frame = m_sketch->model().frame;
  const gp_Dir normal = frame.normal();
  return QJsonObject{{QStringLiteral("origin"), QJsonArray{frame.origin.X(), frame.origin.Y(), frame.origin.Z()}},
                     {QStringLiteral("normal"), QJsonArray{normal.X(), normal.Y(), normal.Z()}}};
}

void MainWindow::showSectionCapsOf(const std::vector<BodyDisplay>& bodies, bool preview) {
  showSectionCaps(bodies, preview);
}

QJsonValue MainWindow::sectionCutPlane() const { return cutPlane(); }

void MainWindow::showSectionCaps(const std::vector<BodyDisplay>& bodies, bool preview) {
  const QJsonValue cut = cutPlane();
  if (cut.isNull()) {
    m_viewer->setSectionCaps({}, gp_Ax3());
    // For UI tests: the caps gone (the bodies whole again).
    if (!m_loggedCaps.isEmpty()) {
      qDebug().noquote() << QStringLiteral("Section caps: none");
    }
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
    m_viewer->setSectionCaps({}, gp_Ax3());
    return;
  }
  const gp_Dir n(normal);
  gp_Ax3 axes(origin, n);
  if (cut != m_section) {
    const geometry::Frame& frame = m_sketch->model().frame;
    axes = gp_Ax3(frame.origin, frame.normal(), frame.x_axis);
  }
  // The caps: the clipped bodies' faces in the plane, but not those the
  // shape the clip cut has there too (a sketch on a body's face), from what
  // each body drawn says of its clip (BodyDisplay::sectioned and uncut), a
  // preview's alike (mitcad#943), without reading the document. One
  // compound per component, in the component's timeline colour
  // (mitcad#382); the root's keep the default colour.
  std::map<int, TopoDS_Compound> groups;
  BRep_Builder builder;
  int count = 0;
  double area = 0.0;
  for (const BodyDisplay& body : bodies) {
    if (!body.sectioned || !body.shape) {
      continue; // not cut
    }
    const std::vector<TopoDS_Face> faces = sectionCapFaces(
        body.shape->occt(), body.uncut ? body.uncut->occt() : TopoDS_Shape(), body.placement, origin, n);
    if (faces.empty()) {
      continue;
    }
    const int colour = componentColorIndex(m_snapshot.components, QString::fromStdString(body.component));
    auto group = groups.find(colour);
    if (group == groups.end()) {
      group = groups.emplace(colour, TopoDS_Compound()).first;
      builder.MakeCompound(group->second);
    }
    for (const TopoDS_Face& face : faces) {
      builder.Add(group->second, face.Moved(body.placement));
      ++count;
    }
    area += facesArea(faces);
  }
  std::vector<OcctViewer::SectionCap> caps;
  for (const auto& [colour, faces] : groups) {
    caps.push_back({faces, colour >= 0 ? componentColor(colour) : QColor()});
  }
  m_viewer->setSectionCaps(caps, axes);
  // With their area (mm²), and "(preview)" for a feature preview's bodies
  // with its clips (mitcad#943).
  const QString logged = QStringLiteral("Section caps: %1 face(s) at %2, area %3%4")
                             .arg(count)
                             .arg(QString::fromUtf8(compactJson(plane)), QString::number(area, 'f', 1),
                                  preview ? QStringLiteral(" (preview)") : QString());
  if (logged != m_loggedCaps) {
    m_loggedCaps = logged;
    qDebug().noquote() << logged;
  }
}

void MainWindow::logSketchReach(const std::vector<BodyDisplay>& bodies) {
  if (!sketchShown()) {
    m_loggedReach.clear();
    m_reachBoxes.clear();
    return;
  }
  const geometry::Frame& frame = m_sketch->model().frame;
  const gp_Vec normal(frame.normal());
  QStringList reach;
  // Each placed shape's box once (mitcad#1646): a refresh in sketch mode,
  // as after each dimension added, measured every body of an assembly of
  // hundreds again (0.15 s). Keyed by the shape's data and where it is
  // placed; the boxes of the shapes shown now are kept for the next time.
  QHash<QByteArray, Bnd_Box> boxes;
  for (const BodyDisplay& body : bodies) {
    const TopoDS_Shape placed = body.shape->occt().Moved(body.placement);
    const gp_Trsf trsf = placed.Location().Transformation();
    QByteArray key;
    const void* shapeData = placed.TShape().get();
    key.append(reinterpret_cast<const char*>(&shapeData), sizeof(shapeData));
    for (int row = 1; row <= 3; ++row) {
      for (int column = 1; column <= 4; ++column) {
        const double value = trsf.Value(row, column);
        key.append(reinterpret_cast<const char*>(&value), sizeof(value));
      }
    }
    Bnd_Box box;
    if (const auto known = m_reachBoxes.constFind(key); known != m_reachBoxes.cend()) {
      box = known.value();
    } else {
      BRepBndLib::Add(placed, box, false);
    }
    boxes.insert(key, box);
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
  m_reachBoxes = boxes;
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
      // Only the cosmetic ones, which the view draws: measuring modelled
      // threads cost an open 0.3-0.6 s (mitcad#878).
      threads = query({{QStringLiteral("query"), QStringLiteral("threads")}, {QStringLiteral("cosmetic"), true}})
                    .toArray();
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
  if (!sectionShown() || !canChangeModel()) {
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
  if (name.isEmpty() || !sectionShown() || !canChangeModel()) {
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

double MainWindow::pixelSize(const gp_Pnt& point) const {
  // One step up the view from the point, measured on the screen (as the
  // manipulators measure the zoom).
  const QPointF a = m_viewer->toWidgetF(point);
  const QPointF b = m_viewer->toWidgetF(point.Translated(gp_Vec(m_viewer->viewUp())));
  const double pixels = std::hypot(b.x() - a.x(), b.y() - a.y());
  return pixels > 1e-9 ? 1.0 / pixels : std::numeric_limits<double>::quiet_NaN();
}

std::optional<gp_Dir> MainWindow::towardViewer(const gp_Pnt& point) const {
  // Towards the eye (a perspective view), else against the view direction.
  const auto& camera = m_viewer->viewCamera();
  if (camera.IsNull()) {
    return std::nullopt;
  }
  const gp_Dir back = camera->Direction().Reversed();
  if (camera->IsOrthographic()) {
    return back;
  }
  const gp_Vec toEye(point, camera->Eye());
  return toEye.Magnitude() > 1e-9 ? gp_Dir(toEye) : back;
}

bool MainWindow::takesFeatures() const {
  return m_session && m_session->activeTakes(SelectKind::Feature);
}

bool MainWindow::takesFeature(const SelectionItem& item) const {
  return m_session && m_session->activeAccepts(item);
}

QJsonValue MainWindow::query(const QJsonObject& request) const {
  // A sketch read once per refresh (m_sketchQueries, mitcad#679); each
  // read of the model logged with MITCAD_LOG_TIMING ("sketch query").
  const bool sketch =
      request.size() == 2 && request.value(QStringLiteral("query")).toString() == QStringLiteral("sketch");
  const QString uid = sketch ? request.value(QStringLiteral("uid")).toString() : QString();
  if (sketch && m_sketchQueries) {
    const auto known = m_sketchQueries->constFind(uid);
    if (known != m_sketchQueries->cend()) {
      return known.value();
    }
  }
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
  std::optional<ScopedTiming> timing;
  if (sketch) {
    timing.emplace("sketch query");
    timing->setDetail(uid);
  }
  const QJsonDocument answer = parseJson(idleDocument().query(rustStr(compactJson(request))));
  const QJsonValue value = answer.isArray() ? QJsonValue(answer.array()) : QJsonValue(answer.object());
  timing.reset();
  if (m_queryMemo) {
    m_queryMemo->insert(key, value);
  }
  if (sketch && m_sketchQueries) {
    m_sketchQueries->insert(uid, value);
  }
  return value;
}

// While a job computes the model, the unit the window last read with the
// document's structure (its snapshot): the document is the worker's then
// (docs/architecture.md, "Background computation"). A value box closed by
// Enter while a preview computes gives its value the unit (mitcad#888);
// the read refused would have given millimetres in any design.
QString MainWindow::lengthUnit() const {
  if (modelBusy()) {
    return m_snapshot.lengthUnit;
  }
  return CommandContext::lengthUnit();
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

// As the view draws them: a stand-in of the interactive tier while there
// is one (framework/DisplayMeshes, mitcad#635).
std::shared_ptr<geometry::Shape> MainWindow::previewBodyShape(const std::string& uid) const {
  return displayShape(idleDocument().preview_body_shape(uid));
}

std::shared_ptr<geometry::Shape> MainWindow::previewToolShape() const {
  return displayShape(idleDocument().preview_tool_shape());
}

bool MainWindow::refinePreviewDisplay(const std::vector<std::shared_ptr<geometry::Shape>>& bodies, QObject* context,
                                      std::function<void(bool)> done) {
  if (bodies.empty() || modelBusy()) {
    return false;
  }
  // Off the UI thread, as previews are (mitcad#634).
  if (asynchronousPreviews()) {
    return m_documentSession->refineDisplayAsync(bodies, context,
        [done = std::move(done)](const DocumentSession::AsyncResult& result) {
          if (done) { done(result.succeeded()); }
        });
  }
  bool refined = false;
  try {
    refined = m_documentSession->refineDisplay(bodies);
  } catch (const std::exception& error) {
    qWarning().noquote() << QStringLiteral("Preview refinement failed: %1").arg(errorText(error));
  }
  if (done) {
    done(refined);
  }
  return true;
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
    // In plain words, its places painted while the pointer is over it
    // (mitcad#1612: the model's `error_notice`; its `error` keeps the
    // names and coordinates for agents and the log).
    const QJsonObject notice = result.value(QStringLiteral("error_notice")).toObject();
    const QString message = notice.value(QStringLiteral("message")).toString();
    const QString feature = notice.value(QStringLiteral("target")).toString();
    qDebug().noquote() << QStringLiteral("Recompute failed: %1").arg(error);
    showFeatureStatus(tr("Recompute failed: %1").arg(message.isEmpty() ? userText(error) : message),
                      NoticeLevel::Error, feature);
    return false;
  }
  const int recomputed = result.value(QStringLiteral("recomputed")).toInt();
  setErrorStyleSheet(m_statusLabel, QString());
  m_statusIsHint = true;
  // Information, without an icon of its own (mitcad#1040).
  m_statusLevel = NoticeLevel::Information;
  // About no feature: a feature's places go with its message (mitcad#1609).
  m_statusFeature.clear();
  updateStatusNoticeHover();
  if (m_statusIcon != nullptr) {
    m_statusIcon->clear();
    m_statusIcon->hide();
  }
  m_statusLabel->setText(tr("Recomputed %n feature(s) in %1 ms", nullptr, recomputed)
                             .arg(milliseconds, 0, 'f', 1));
  qDebug().noquote() << m_statusLabel->text();
  // The drawings of an older design file whose views now keep the bodies
  // they showed (mitcad#1293): one information each, in the history too.
  for (const QJsonValue& value : result.value(QStringLiteral("notices")).toArray()) {
    const QJsonObject notice = value.toObject();
    if (notice.value(QStringLiteral("level")).toString() == QStringLiteral("info")) {
      const QString message = notice.value(QStringLiteral("message")).toString();
      qInfo().noquote() << QStringLiteral("Drawing pinned: %1").arg(message);
      showStatus(message, NoticeLevel::Information);
    }
  }
  return true;
}

// The sketches' entity shapes read once for a loop over many items
// (m_sketchShapes); the outermost batch clears them at its end.
struct MainWindow::SketchShapeBatch {
  explicit SketchShapeBatch(const MainWindow& window) : m_window(window), m_outer(!window.m_sketchShapes) {
    if (m_outer) {
      m_window.m_sketchShapes.emplace();
    }
  }
  ~SketchShapeBatch() {
    if (m_outer) {
      m_window.m_sketchShapes.reset();
    }
  }
  SketchShapeBatch(const SketchShapeBatch&) = delete;
  SketchShapeBatch& operator=(const SketchShapeBatch&) = delete;

private:
  const MainWindow& m_window;
  bool m_outer;
};

std::shared_ptr<void> MainWindow::itemShapeBatch() const { return std::make_shared<SketchShapeBatch>(*this); }

// Each sketch's `sketch` query answered once while it lives
// (m_sketchQueries, mitcad#679); the outermost one clears them at its end.
struct MainWindow::SketchQueryMemo {
  explicit SketchQueryMemo(const MainWindow& window) : m_window(window), m_outer(!window.m_sketchQueries) {
    if (m_outer) {
      m_window.m_sketchQueries.emplace();
    }
  }
  ~SketchQueryMemo() {
    if (m_outer) {
      m_window.m_sketchQueries.reset();
    }
  }
  SketchQueryMemo(const SketchQueryMemo&) = delete;
  SketchQueryMemo& operator=(const SketchQueryMemo&) = delete;

private:
  const MainWindow& m_window;
  bool m_outer;
};

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
      TopoDS_Compound compound;
      BRep_Builder builder;
      builder.MakeCompound(compound);
      for (const SketchEntityDisplay& entity : sketchDisplay(item.owner)) { // either kind (mitcad#1524)
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
      const auto* feature = m_snapshot.feature(item.owner);
      if (!feature || feature->index >= m_snapshot.marker || !feature->succeeded() || feature->suppressed) {
        return TopoDS_Shape();
      }
      // A Move Face's moved faces, which keep their names (mitcad#531).
      if (const TopoDS_Shape moved = movedFacesShape(item.owner); !moved.IsNull()) {
        return moved;
      }
      // An Intersection Curve's curve (mitcad#1405), as the view draws it.
      if (feature->type == QStringLiteral("intersection_curve")) {
        const QJsonArray curves = query({{QStringLiteral("query"), QStringLiteral("feature_curves")}}).toArray();
        for (const QJsonValue& value : curves) {
          const QJsonObject curve = value.toObject();
          if (curve.value(QStringLiteral("uid")).toString() == item.owner) {
            return polylineShape(curve.value(QStringLiteral("points")).toArray());
          }
        }
        return TopoDS_Shape();
      }
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
      if (m_sketchShapes) {
        auto known = m_sketchShapes->find(item.owner);
        if (known == m_sketchShapes->end()) {
          QHash<QString, TopoDS_Shape> shapes;
          for (const SketchEntityDisplay& entity : sketchDisplay(item.owner)) {
            if (!shapes.contains(entity.id)) { // the first, as below
              shapes.insert(entity.id, entity.shape);
            }
          }
          known = m_sketchShapes->insert(item.owner, shapes);
        }
        return known->value(item.name);
      }
      for (const SketchEntityDisplay& entity : sketchDisplay(item.owner)) {
        if (entity.id == item.name) {
          return entity.shape;
        }
      }
      return TopoDS_Shape();
    }
    case SelectKind::Plane:
      if (isFixedPlane(item)) {
        // A definition's fixed plane, drawn where it lies (mitcad#1137).
        return datumShape(fixedPlaneDatum(item.reference().toObject()), kDatumSize);
      }
      [[fallthrough]];
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
  // A recompute, edit, undo or newly opened document can change the
  // geometry behind the same selection reference.
  m_idleMeasurements.clear();
  // And a feature's inputs, kept by its last build (mitcad#957).
  m_featureInputLines.clear();
  ScopedTiming total("refresh");
  ScopedTiming snapshot("refresh snapshot");
  try {
    m_snapshot = DocumentSnapshot::read(*this);
  } catch (const std::exception& e) {
    qWarning().noquote() << "Document structure not read:" << errorText(e);
  }
  snapshot.finish();
  // The linked designs' files watched (mitcad#1416).
  refreshLinkWatch();
  // The sketch being edited went with another change, such as its
  // component deleted (mitcad#99): sketch mode ends instead of editing
  // what is no longer there.
  if (m_mode == Mode::Sketch && !editingSketchUid().isEmpty() && m_snapshot.feature(editingSketchUid()) == nullptr) {
    qDebug().noquote() << QStringLiteral("Sketch %1 is gone; sketch mode ends").arg(editingSketchUid());
    if (m_session) {
      m_session->discard();
    }
    leaveSketchMode();
  }
  // The section of the analysis shown (mitcad#41).
  followAnalyses();
  // The measurement objects shown, as dimensions over the view (mitcad#649).
  followMeasurements();
  // The Make tab's Browser markers follow the design (mitcad#1049).
  updateMakeMarkers();
  // The drawing open over the view follows the model (mitcad#707).
  followDrawing();
  // The image references shown, on their planes (mitcad#782).
  followImageReferences();
  // The Center of Mass marker where the design's center is (mitcad#1094).
  followCenterOfMass();
  // The comments' pins and the Comments panel (mitcad#1119).
  followComments();
  // The Origin folder and Isolate are the document's (P9).
  m_originShown = m_snapshot.originShown;
  m_isolation = m_snapshot.isolation;
  // View › Show's check marks (mitcad#1287), the design's too.
  syncViewShowChecks();
  // The document's home view (U5), if it has one.
  std::optional<CameraState> home;
  for (const QJsonValue& view : std::as_const(m_snapshot.namedViews)) {
    if (view.toObject().value(QStringLiteral("name")).toString() == QStringLiteral("Home")) {
      home = ViewController::cameraOf(view.toObject());
    }
  }
  m_viewer->setHome(home);
  // The layout grid in the design's length unit (mitcad#1210).
  m_viewController->setLengthUnit(m_snapshot.lengthUnit);
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
  // Everything outside the active component as ghosts (mitcad#420), set
  // before the items are shown, which then take their look at once.
  m_viewer->setActiveScope(activeScopeOf(m_snapshot, editingSketchUid()));

  ScopedTiming bodies("refresh bodies");
  showBodies();
  bodies.finish();
  ScopedTiming sketches("refresh sketches");
  // From here on the refresh changes no model: each sketch it shows, sketch
  // mode's reload and the selection kept read it once (mitcad#679).
  std::optional<SketchQueryMemo> sketchQueries(std::in_place, *this);
  ScopedTiming profiles("refresh profiles");
  showProfiles();
  refreshAutoselect();
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
  if (m_sketch3d != nullptr && m_sketch3d->isActive()) {
    m_sketch3d->reload(); // mitcad#1524
    logSketch3dEntities(); // what a tool drew, where a click picks it (mitcad#1525)
  }
  sketches.finish();

  ScopedTiming browser("refresh browser");
  m_controller->refresh(m_snapshot);
  browser.finish();
  ScopedTiming actions("refresh actions");
  if (m_mode != Mode::Command) {
    // What was selected may be gone (each sketch read once, mitcad#670).
    const SketchShapeBatch batch(*this);
    Selection kept;
    for (const SelectionItem& item : retainCurrentProfiles(m_selection, m_currentProfiles)) {
      if (keepSelected(item)) {
        kept.append(item);
      }
    }
    m_selection = kept;
    m_controller->showSelection(m_selection);
    updateIdleView();
  }
  sketchQueries.reset();
  // The part picker's In this design follows the design (mitcad#418).
  if (auto* picker = findChild<PartPicker*>(QString(), Qt::FindDirectChildrenOnly)) {
    picker->designChanged();
  }
  updateActions();
}

bool MainWindow::keepSelected(const SelectionItem& item) const {
  if (item.kind == SelectKind::Profile && m_sketch->draggingGeometry() && item.owner == m_sketch->uid()) {
    return true; // validate against the final regions when the drag ends
  }
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
    // A joint, as-built joint or constraint set while the model has it: its
    // highlight shows its sides (mitcad#523).
    if (!jointEntry(item.owner).isEmpty()) {
      return true;
    }
    // A feature selected on the timeline (mitcad#28) while it has faces on
    // the bodies at the marker.
    return TopExp_Explorer(itemShape(item), TopAbs_FACE).More();
  }
  return !itemShape(item).IsNull();
}

std::vector<BodyDisplay> MainWindow::modelBodies() const { return displayBodies(nullptr); }

std::vector<BodyDisplay> MainWindow::displayBodies(SectionTargets* unclipped) const {
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
      // The base colour's image and every map's (mitcad#314).
      resolveTextureMaps(appearance.texture, folder, [this, id](const QString& map) {
        try {
          return QByteArray::fromBase64(queryObject({{QStringLiteral("query"), QStringLiteral("appearance_image")},
                                                     {QStringLiteral("id"), id},
                                                     {QStringLiteral("map"), map}})
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
  // Sheets shown flat (mitcad#210): their valid Flat Pattern's developed
  // solid in place of the folded body, in the body's coordinates.
  QSet<QString> flat;
  try {
    for (const QJsonValue& value : queryArray(QStringLiteral("sheet_bodies"))) {
      const QJsonObject pattern = value.toObject().value(QStringLiteral("flat_pattern")).toObject();
      if (pattern.value(QStringLiteral("shown")).toBool() &&
          pattern.value(QStringLiteral("status")).toString() == QStringLiteral("valid")) {
        flat.insert(value.toObject().value(QStringLiteral("body")).toString());
      }
    }
  } catch (const std::exception&) {
  }
  std::vector<BodyDisplay> bodies;
  // The bodies the plane cuts, by index into `bodies`, with their targets.
  SectionTargets cutting;
  cutting.plane = cut;
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
    if (flat.contains(uid)) {
      try {
        if (auto developed = idleDocument().analysis_shape(rustStr(
                compactJson({{QStringLiteral("shape"), QStringLiteral("flat_pattern")}, {QStringLiteral("body"), uid}})))) {
          shape = developed;
        }
      } catch (const std::exception&) {
        // No valid pattern now: the folded body.
      }
    }
    const QJsonArray rows = instance.value(QStringLiteral("transform")).toArray();
    if (!cut.isNull() && !flat.contains(uid)) {
      // The cut is in design coordinates. Clip in this occurrence's local
      // coordinates, then retain its normal display placement.
      cutting.which.push_back(bodies.size());
      cutting.targets.append(QJsonObject{{QStringLiteral("body"), uid}, {QStringLiteral("occurrence"), occurrence}});
      cutting.keys << sectionClipKey(uid, occurrence, rows);
    }
    BodyDisplay display;
    display.uid = utf8(uid);
    display.component = utf8(m_snapshot.bodies.value(uid).component);
    display.componentColor = componentColorIndex(m_snapshot.components, m_snapshot.bodies.value(uid).component);
    display.shape = shape;
    if (const auto look = looks.constFind(uid); look != looks.cend()) {
      display.color = look->displayColor;
      display.appearance = look.value();
    }
    if (!isIdentity(rows)) {
      display.placement = TopLoc_Location(trsfOf(rows));
    }
    display.occurrence = utf8(occurrence);
    // The body's or an occurrence's on its path (mitcad#1093).
    display.transparent = instance.value(QStringLiteral("transparent")).toBool();
    bodies.push_back(display);
  }
  if (unclipped != nullptr) {
    *unclipped = std::move(cutting);
    return bodies;
  }
  if (!cutting.which.empty()) {
    useClips(cutting, bodies);
  } else {
    // No body to cut, or no section (mitcad#1230): nothing failed.
    m_sectionState.outcome = SectionOutcome{};
    m_sectionState.outcome.plane = cut;
    if (cut.isNull() && !m_sectionState.clips.isEmpty()) {
      // Whole again: what was kept goes.
      m_sectionState.clips.clear();
      m_sectionState.plane = QJsonValue();
    }
  }
  for (BodyDisplay& body : bodies) {
    const QString uid = QString::fromStdString(body.uid);
    if (faceAppearances.contains(uid)) {
      body.faceLooks = faceLooksOf(uid, *body.shape);
    }
  }
  return bodies;
}

void MainWindow::useClips(const SectionTargets& cut, std::vector<BodyDisplay>& bodies) const {
  SectionState& kept = m_sectionState;
  if (kept.plane != cut.plane) {
    kept.clips.clear();
    kept.plane = cut.plane;
  }
  // Those clipped at this plane before (a round's, an earlier call's),
  // while their shapes and placements are the same.
  std::vector<std::size_t> missing; // into cut's lists
  std::vector<std::size_t> which;   // into bodies
  QJsonArray targets;
  for (std::size_t i = 0; i < cut.which.size(); ++i) {
    BodyDisplay& body = bodies[cut.which[i]];
    const auto found = kept.clips.constFind(cut.keys.value(static_cast<qsizetype>(i)));
    if (found != kept.clips.cend() && found->source == body.shape) {
      if (found->shape) {
        body.sectioned = !found->shape->occt().IsSame(body.shape->occt());
        body.uncut = body.sectioned ? body.shape : nullptr; // for its caps (mitcad#943)
        body.shape = found->shape;
      }
      continue;
    }
    missing.push_back(i);
    which.push_back(cut.which[i]);
    targets.append(cut.targets.at(static_cast<qsizetype>(i)));
  }
  if (!missing.empty()) {
    // Clipped here, and their new shapes meshed, beside a round that may
    // run: the section worker reads only what was detached from the bodies
    // (mitcad#438).
    QStringList errors;
    const std::vector<std::shared_ptr<geometry::Shape>> left =
        clipBodies(idleDocument(), cut.plane, targets, which, bodies, errors);
    for (std::size_t k = 0; k < missing.size(); ++k) {
      BodyDisplay& body = bodies[which[k]];
      const std::shared_ptr<geometry::Shape> shape = k < left.size() ? left[k] : nullptr;
      kept.clips.insert(cut.keys.value(static_cast<qsizetype>(missing[k])),
                        SectionClip{body.shape, shape, errors.value(static_cast<qsizetype>(k))});
      if (shape) {
        // Cut, not left whole: drawn behind its caps (mitcad#784).
        body.sectioned = !shape->occt().IsSame(body.shape->occt());
        body.uncut = body.sectioned ? body.shape : nullptr; // for its caps (mitcad#943)
        body.shape = shape;
      }
    }
  }
  // What the plane did to each body (mitcad#1230), for the warning of the
  // failed clips and the analysis' row: a failed clip shows the body
  // whole; a clip that is the body itself keeps it whole, one without
  // faces removes it, any other cuts it (a sliver too).
  SectionOutcome outcome;
  outcome.plane = cut.plane;
  for (std::size_t i = 0; i < cut.which.size(); ++i) {
    const BodyDisplay& body = bodies[cut.which[i]];
    const auto found = kept.clips.constFind(cut.keys.value(static_cast<qsizetype>(i)));
    if (found == kept.clips.cend()) {
      continue;
    }
    if (!found->shape) {
      if (!outcome.failed.contains(QString::fromStdString(body.uid))) {
        outcome.failed << QString::fromStdString(body.uid);
        outcome.errors << found->error;
      }
    } else if (found->source && found->shape->occt().IsSame(found->source->occt())) {
      ++outcome.kept;
    } else if (!TopExp_Explorer(found->shape->occt(), TopAbs_FACE).More()) {
      ++outcome.removed;
    } else {
      ++outcome.cut;
    }
  }
  kept.outcome = outcome;
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
  // Bodies the section shows whole because their clip failed, or a plane
  // that cuts none (mitcad#1230).
  reportSectionClips();
  // The panel's measurement once the view has the section's bodies
  // (mitcad#438).
  measureIfWaiting();
  ScopedTiming reach("bodies sketch reach");
  logSketchReach(bodies);
  reach.finish();
  ScopedTiming threads("bodies threads");
  showThreads(bodies);
  threads.finish();
  // The design printer's build volume, when shown (mitcad#640).
  if (m_viewController != nullptr && m_viewController->buildVolumeShown()) {
    m_viewController->refreshBuildVolume();
  }
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
    // Keyed by the shape's data without its location: a moved body shares
    // it with the body before the move, as a location (mitcad#838).
    const void* key = shape.TShape().get();
    if (!m_localCenters.contains(key)) {
      Bnd_Box box;
      BRepBndLib::Add(shape.Located(TopLoc_Location()), box);
      if (box.IsVoid()) {
        continue;
      }
      m_localCenters.insert(key, gp_Pnt((box.CornerMin().XYZ() + box.CornerMax().XYZ()) / 2.0));
    }
    const gp_Pnt center = m_localCenters.value(key);
    centers.insert(key, center);
    m_bodyCenters.append(
        {label, center.Transformed(body.placement.Transformation() * shape.Location().Transformation())});
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
    const QPoint at = m_viewer->mapTo(window(), m_viewer->toWidget(center));
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
// and so do one the user showed and one a command shows for its time
// (mitcad#546: an edited feature's). Hidden sketches show none.
void MainWindow::showProfiles() {
  // Region hashes and faces change at every drag step. Keep the curves
  // live, and read/build the final profiles once the drag ends.
  if (m_sketch->draggingGeometry()) {
    return;
  }
  const QString active = m_sketch->isActive() ? m_sketch->uid() : QString();
  std::vector<std::pair<ProfileKey, TopoDS_Shape>> profiles;
  std::map<std::string, QString> occurrences; // of sketches of placed components
  QHash<QString, ProfileFace> faces;
  const QJsonArray all =
      query({{QStringLiteral("query"), QStringLiteral("profiles")}, {QStringLiteral("hashes"), true}}).toArray();
  m_currentProfiles = all;
  for (const QJsonValue& value : all) {
    const QJsonObject profile = value.toObject();
    const QString sketch = profile.value(QStringLiteral("sketch")).toString();
    if (sketch == active) {
      if (!m_sketch->showProfiles()) {
        continue;
      }
    } else {
      const DocumentSnapshot::Feature* feature = m_snapshot.feature(sketch);
      const bool shownByUser = (feature != nullptr && feature->visibleSet && feature->visible) ||
                               m_commandSketches.used().contains(sketch);
      if (!sketchShown(sketch) || (!shownByUser && profile.value(QStringLiteral("consumed")).toBool())) {
        continue;
      }
    }
    const std::string region = utf8(profile.value(QStringLiteral("region")).toString());
    const QString occurrence = placingOccurrence(sketch);
    if (!occurrence.isEmpty()) {
      if (sketch != active && !m_snapshot.isOccurrenceShown(occurrence)) {
        continue; // the component's occurrence is hidden (not the edited sketch's, mitcad#1194)
      }
      occurrences[utf8(sketch)] = occurrence;
    }
    // A face built before is used again while the region's geometry and
    // place (the model's hash) and its occurrence's placement stay, so the
    // view keeps its display of it. The faces are the view's own: built
    // here from the region's numbers and handed to nothing but the view,
    // which meshes them on this thread (mitcad#1586, OcctViewer::setProfiles);
    // never pass one to the model or a job.
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

void MainWindow::refreshAutoselect() {
  if (m_sketch->draggingGeometry()) {
    return;
  }
  const QJsonObject answer = queryObject({{QStringLiteral("query"), QStringLiteral("autoselect_profiles")}});
  const QString sketch = answer.value(QStringLiteral("sketch")).toString();
  m_autoselect->setProfiles(AutoselectProfiles::read(answer, placingOccurrence(sketch)));
}

QString MainWindow::placingOccurrence(const QString& feature) const {
  if (!feature.isEmpty() && feature == editingSketchUid()) {
    return m_sketchOccurrence; // where sketch mode shows it (mitcad#505), either kind
  }
  const DocumentSnapshot::Feature* entry = m_snapshot.feature(feature);
  return entry != nullptr ? m_snapshot.firstOccurrence(entry->component) : QString();
}

QString MainWindow::sketchEditOccurrence(const QString& uid, const QString& picked) const {
  // A new sketch is not in the snapshot yet: it went into the active
  // component.
  const DocumentSnapshot::Feature* entry = m_snapshot.feature(uid);
  const QString component = entry != nullptr ? entry->component : m_snapshot.activeComponent;
  const QString first = m_snapshot.firstOccurrence(component);
  if (first.isEmpty()) {
    return QString(); // the root's sketch
  }
  try {
    const QJsonObject sketch = queryObject({{QStringLiteral("query"), QStringLiteral("sketch")},
                                            {QStringLiteral("uid"), uid}});
    const QString target =
        sketch.value(QStringLiteral("plane_link")).toObject().value(QStringLiteral("target")).toString();
    if (!target.isEmpty() && m_snapshot.occurrenceComponent(target) == component) {
      return target;
    }
  } catch (const std::exception&) {
    // A sketch just taken back: its component's first occurrence.
  }
  if (!picked.isEmpty() && m_snapshot.occurrenceComponent(picked) == component) {
    return picked;
  }
  return first;
}

TopoDS_Shape MainWindow::sketchFace(const QString& uid) const {
  try {
    const QJsonObject sketch = queryObject({{QStringLiteral("query"), QStringLiteral("sketch")},
                                            {QStringLiteral("uid"), uid}});
    const QJsonObject plane = sketch.value(QStringLiteral("plane")).toObject();
    SelectionItem face{SelectKind::Face, plane.value(QStringLiteral("body")).toString(),
                       plane.value(QStringLiteral("face")).toString(), QString()};
    if (face.owner.isEmpty() || face.name.isEmpty()) {
      return TopoDS_Shape(); // a plane, or a face of any body
    }
    const QJsonValue link = sketch.value(QStringLiteral("plane_link"));
    face.occurrence =
        link.isObject() ? link.toObject().value(QStringLiteral("source")).toString() : m_sketchOccurrence;
    return itemShape(face);
  } catch (const std::exception&) {
    return TopoDS_Shape();
  }
}

gp_Trsf MainWindow::sketchPlacement(const QString& uid) const {
  return placementOf(placingOccurrence(uid)).Transformation();
}

QString MainWindow::sketchOccurrence(const QString& uid) const { return placingOccurrence(uid); }

bool MainWindow::modelReadableNow() { return !modelBusy() || m_documentSession->yieldBackgroundJob(); }

bool MainWindow::queryInBackground(const QJsonObject& query, const BackgroundRead& read, QObject* context,
                                   const BackgroundDone& done) {
  if (modelBusy()) {
    return false;
  }
  return m_documentSession->queryAsync(
      QStringLiteral("body geometry"), query, read, context,
      [done](const DocumentSession::AsyncResult& outcome) { done(outcome.succeeded(), outcome.cancelled ? QString() : outcome.error); });
}

QString MainWindow::bodyOccurrence(const QString& body) const {
  const auto entry = m_snapshot.bodies.constFind(body);
  return entry != m_snapshot.bodies.cend() ? m_snapshot.firstOccurrence(entry->component) : QString();
}

// As showDatums places it.
QString MainWindow::datumOccurrence(const QString& datum) const {
  return kOriginDatums.contains(datum) ? QString() : placingOccurrence(datum);
}

// A Construct command's preview where showDatums places the finished datum
// (mitcad#1029): a new datum goes into the active component.
gp_Trsf MainWindow::datumPlacement(const QString& datum) const {
  if (!datum.isEmpty() && m_snapshot.feature(datum) != nullptr) {
    return placementOf(placingOccurrence(datum)).Transformation();
  }
  return placementOf(m_snapshot.firstOccurrence(m_snapshot.activeComponent)).Transformation();
}

// The shown sketches (the browser's light bulbs, else the default rule:
// DocumentSnapshot::featureShown), the one being edited, and those a
// command shows for its time (mitcad#546), drawn lighter while their light
// bulbs are off.
void MainWindow::showSketches() {
  // Planar and 3D sketches alike (mitcad#1524).
  QStringList sketches;
  const QString editedUid = editingSketchUid();
  if (!editedUid.isEmpty()) {
    sketches << editedUid;
  }
  for (const DocumentSnapshot::Feature& feature : std::as_const(m_snapshot.features)) {
    if (feature.isAnySketch() && feature.succeeded() &&
        sketchShown(feature.uid) && !sketches.contains(feature.uid)) {
      sketches << feature.uid;
    }
  }
  std::vector<SketchEntityDisplay> entities;
  QStringList shown;
  for (const QString& sketch : sketches) {
    try {
      std::vector<SketchEntityDisplay> some = sketchDisplay(sketch);
      // Every shown sketch tells how constrained it is, edited or not
      // (mitcad#286): entering or leaving its edit keeps its colours.
      // A component's sketch where its occurrence places it.
      const QString occurrence = placingOccurrence(sketch);
      QString placed;
      if (!occurrence.isEmpty()) {
        // The component's occurrence hidden: its sketches are not drawn,
        // but the edited one is, where its dimensions and constraints are
        // (mitcad#1194).
        const bool edited = sketch == editedUid;
        if (!edited && !m_snapshot.isOccurrenceShown(occurrence)) {
          continue;
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
      // Shown only for a command (mitcad#546): lighter, as it hides again.
      const bool temporary = sketch != editedUid && m_commandSketches.temporary(sketch, m_snapshot.isDrawn(sketch));
      // The signature also has how the entity is drawn and placed.
      for (SketchEntityDisplay& entity : some) {
        entity.temporary = temporary;
        entity.signature += QStringLiteral("|%1|%2|%3")
                                .arg(entity.constrained ? 1 : 0)
                                .arg(placed)
                                .arg(temporary ? 1 : 0);
      }
      entities.insert(entities.end(), some.begin(), some.end());
      shown << sketch;
    } catch (const std::exception&) {
      // A sketch that was just taken back.
    }
  }
  m_viewer->setSketchEntities(entities);
  m_sketchDisplay = std::move(entities);
  logSketch3dEntities(); // where an edited 3D sketch's are (mitcad#1524)
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
  // The origin while Create Sketch asks for a plane or its light bulb is
  // on; construction features unless hidden. View › Show leaves out the
  // origins or the construction geometry of the whole design whatever the
  // light bulbs say (mitcad#1287); Create Sketch's plane pick still shows
  // the origin planes.
  const bool origin = m_originVisible || (m_originShown && m_snapshot.showOrigins);
  const bool picks = qEnvironmentVariableIsSet("MITCAD_LOG_PICKS");
  QVector<QJsonObject> logged;
  QStringList construction; // for UI tests: the construction datums drawn
  for (const QJsonValue& value : all) {
    const QJsonObject datum = value.toObject();
    const QString uid = datum.value(QStringLiteral("uid")).toString();
    if (kOriginDatums.contains(uid) ? !origin : !m_snapshot.featureShown.value(uid, true)) {
      continue;
    }
    if (m_constructionHidden) {
      continue; // an input that takes none hides them (mitcad#394)
    }
    const DocumentSnapshot::Feature* feature = kOriginDatums.contains(uid) ? nullptr : m_snapshot.feature(uid);
    if (feature != nullptr && m_snapshot.kindHidden(*feature)) {
      continue; // View › Show (mitcad#1287)
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
    if (kOriginDatums.contains(uid) ? origin : picks) {
      logged.append(datum);
    }
    if (feature != nullptr && feature->isConstruction()) {
      construction << (display.occurrence.isEmpty() ? feature->name
                                                    : QStringLiteral("%1 in %2").arg(feature->name, display.occurrence));
    }
  }
  // Intersection Curves (mitcad#1405): their curves drawn as lines, with
  // the construction geometry's light bulbs; a click picks the feature, which
  // a path input takes.
  const QJsonArray curves = query({{QStringLiteral("query"), QStringLiteral("feature_curves")}}).toArray();
  for (const QJsonValue& value : curves) {
    const QJsonObject curve = value.toObject();
    const QString uid = curve.value(QStringLiteral("uid")).toString();
    if (m_constructionHidden || !m_snapshot.featureShown.value(uid, true)) {
      continue;
    }
    const DocumentSnapshot::Feature* feature = m_snapshot.feature(uid);
    if (feature != nullptr && m_snapshot.kindHidden(*feature)) {
      continue;
    }
    DatumDisplay display{uid, SelectKind::Feature, polylineShape(curve.value(QStringLiteral("points")).toArray())};
    display.occurrence = placingOccurrence(uid);
    // Picked as the curve (mitcad#1417): the sketch's Project and Intersect,
    // Rib and Web take it, not other features.
    display.geometry = featureCurveGeometry();
    if (!display.occurrence.isEmpty()) {
      if (!m_snapshot.isOccurrenceShown(display.occurrence)) {
        continue;
      }
      if (!display.shape.IsNull()) {
        display.shape = display.shape.Moved(placementOf(display.occurrence));
      }
    }
    datums.push_back(display);
    const QString name = feature != nullptr ? feature->name : uid;
    construction << (display.occurrence.isEmpty() ? name : QStringLiteral("%1 in %2").arg(name, display.occurrence));
  }
  m_viewer->setDatums(datums);
  // For UI tests (mitcad#1287): the construction planes, axes and points
  // the view draws, when they change.
  const QString constructionShown = construction.isEmpty() ? QStringLiteral("none") : construction.join(QStringLiteral(", "));
  if (constructionShown != m_loggedConstruction) {
    m_loggedConstruction = constructionShown;
    qDebug().noquote() << QStringLiteral("Construction shown: %1").arg(constructionShown);
  }
  // For UI tests (mitcad#739): whether the view shows the design's origin,
  // and the Origin folder's light bulb, when either changes. A sketch draws
  // its own origin and axes whatever these say.
  const QString originState = QStringLiteral("Design origin %1 in the view; the Origin folder's light bulb %2")
                                  .arg(origin && !m_constructionHidden ? QStringLiteral("shown") : QStringLiteral("hidden"),
                                       m_originShown ? QStringLiteral("on") : QStringLiteral("off"));
  if (originState != m_loggedOriginState) {
    m_loggedOriginState = originState;
    qDebug().noquote() << originState;
  }
  m_loggedDatums = logged;
  m_loggedDatumPlaces.clear();
  logDatumPlaces();
}

void MainWindow::logDatumPlaces() {
  // Where a click picks each datum (the origin's while they are shown;
  // with MITCAD_LOG_PICKS also the construction features, at the first
  // place tried where a click would pick the datum now; a component's
  // "Datum F4 in O1 at x,y", where its occurrence places it), for UI tests.
  // Logged again when the camera comes to rest (the places move with it;
  // a place logged before the view was laid out is 0,0, mitcad#318).
  QStringList lines;
  const bool picks = qEnvironmentVariableIsSet("MITCAD_LOG_PICKS");
  for (const QJsonObject& datum : std::as_const(m_loggedDatums)) {
    const QString uid = datum.value(QStringLiteral("uid")).toString();
    const SelectKind kind = datumKind(datum.value(QStringLiteral("type")).toString());
    const QString occurrence = datumOccurrence(uid);
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
    if (!occurrence.isEmpty()) {
      // In the component's coordinates: where the occurrence places them.
      const gp_Trsf placement = placementOf(occurrence).Transformation();
      for (gp_Pnt& candidate : candidates) {
        candidate.Transform(placement);
      }
    }
    // The first place that picks the datum where no other datum and no
    // edge lies (another may be selected and highlighted over it later,
    // and an edge a pixel away may win the click), else the first that
    // picks it. Tried at the whole pixel a click there hits.
    gp_Pnt at = candidates.front();
    bool found = false;
    // Not under the orientation cube, which takes a click there before the
    // view's picking does (mitcad#1030: the XY plane's place seen beside
    // the blocks lay under it, and the click turned the view).
    const QRect cube = m_viewer->overlayCubeArea();
    const auto tryPlaces = [&](const std::vector<gp_Pnt>& places) {
      for (const gp_Pnt& candidate : places) {
        if (!picks) {
          break;
        }
        const QPoint widget = m_viewer->toWidget(candidate);
        // Nor under a card over the view, which takes the click too
        // (mitcad#1524: Create Sketch's kind row on the command card).
        if (cube.contains(widget) || m_viewer->isCovered(QPointF(widget))) {
          continue;
        }
        const std::vector<SelectionItem> items = m_viewer->itemsAt(QPointF(widget));
        if (items.empty() || items.front().owner != uid || items.front().kind != kind ||
            items.front().occurrence != occurrence) {
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
    };
    tryPlaces(candidates);
    if (picks && !found && kind == SelectKind::Plane && kOriginDatums.contains(uid)) {
      // None of them picks the plane: with a placement input's planes
      // behind the axes (mitcad#656), the axis seen along the diagonal
      // towards the eye covers them in an isometric view.
      std::vector<gp_Pnt> sides = m_viewer->planePickCandidates(
          pointOf(datum.value(QStringLiteral("origin"))), gp_Dir(vectorOf(datum.value(QStringLiteral("normal")))),
          kDatumSize, true);
      if (!occurrence.isEmpty()) {
        const gp_Trsf placement = placementOf(occurrence).Transformation();
        for (gp_Pnt& side : sides) {
          side.Transform(placement);
        }
      }
      tryPlaces(sides);
    }
    const QPoint point = m_viewer->mapTo(window(), m_viewer->toWidget(at));
    const QString in = occurrence.isEmpty() ? QString() : QStringLiteral(" in ") + occurrence;
    lines << QStringLiteral("Datum %1%2 at %3,%4").arg(uid, in).arg(point.x()).arg(point.y());
  }
  const QString text = lines.join(QLatin1Char('\n'));
  if (text == m_loggedDatumPlaces) {
    return;
  }
  m_loggedDatumPlaces = text;
  for (const QString& line : std::as_const(lines)) {
    qDebug().noquote() << line;
  }
}

void MainWindow::setConstructionHidden(bool hidden) {
  if (hidden == m_constructionHidden) {
    return;
  }
  m_constructionHidden = hidden;
  qDebug().noquote() << (hidden ? QStringLiteral("Construction geometry hidden for the command")
                                : QStringLiteral("Construction geometry shown again"));
  showDatums();
}

void MainWindow::logPickPlaces(SelectFilter filter, bool again) {
  if (!(filter & (SelectKind::Face | SelectKind::Edge | SelectKind::Vertex | SelectKind::Profile)) ||
      !qEnvironmentVariableIsSet("MITCAD_LOG_PICKS")) {
    m_pickPlacesDeferred = false;
    return;
  }
  if (!m_viewer->pickPointsReady()) {
    m_pickPlacesDeferred = true;
    return;
  }
  m_pickPlacesDeferred = false;
  // Where a click picks each face, edge, vertex and profile the input
  // takes, for UI tests (only on request: every item is tried in the view).
  // The points are clear of the cards over the view as they are now.
  const std::vector<std::pair<SelectionItem, QPoint>> places = m_viewer->pickPoints(filter);
  if (again && places.empty()) {
    return; // picking suspended (a job): the places logged before stay
  }
  qDebug().noquote() << "Pick places:";
  for (const auto& [item, at] : places) {
    const QPoint point = m_viewer->mapTo(window(), at);
    qDebug().noquote() << QStringLiteral("Pick %1 %2/%3 at %4,%5")
                              .arg(SelectionItem::kindName(item.kind), item.owner, item.name)
                              .arg(point.x())
                              .arg(point.y());
    // Preserve the original diagnostic for existing scripts; repeated
    // component placements need an unambiguous point of their own.
    if (!item.occurrence.isEmpty()) {
      qDebug().noquote() << QStringLiteral("Pick occurrence %1 %2/%3 in %4 at %5,%6")
                                .arg(SelectionItem::kindName(item.kind), item.owner, item.name, item.occurrence)
                                .arg(point.x())
                                .arg(point.y());
    }
  }
}

void MainWindow::inputActivated(SelectFilter filter) {
  // Only Create Sketch's plane pick shows the origin whatever the Origin
  // folder's light bulb says (mitcad#844). A command's input that takes
  // construction geometry follows the light bulb (mitcad#1030): a hidden
  // origin stays hidden, so it covers no face to pick, and its planes,
  // axes and point are picked from the browser's Origin folder.
  const bool origin = m_mode == Mode::PickPlane && bool(filter & kConstructionGeometry);
  if (origin != m_originVisible) {
    m_originVisible = origin;
    showDatums();
  } else {
    // A placement input puts the planes behind the model and the axes
    // (mitcad#656): where a click picks a datum changes with the input, so
    // the places are tried again (logged only when they moved).
    logDatumPlaces();
  }
  m_pickPlacesFilter = filter;
  logPickPlaces(filter);
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
      const QPoint point = m_viewer->mapTo(window(), m_viewer->toWidget(at));
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
  if (modelBusy() && !m_documentSession->yieldBackgroundJob()) {
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
  return runModelCommandSteps(commands, label, false);
}

bool MainWindow::runNamedModelCommands(const QJsonArray& commands, const QString& label) {
  return runModelCommandSteps(commands, label, true);
}

bool MainWindow::runModelCommandSteps(const QJsonArray& commands, const QString& label, bool nameSingle) {
  if (modelBusy() && !m_documentSession->yieldBackgroundJob()) {
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
        showHint(tr("Computation canceled."));
      } else {
        showError(error);
      }
      return false;
    }
  }
  // Several steps become one named `label`; one step is named so too when
  // asked (merge_undo of a single step renames it).
  if (undoDepth() > depth + 1 || (nameSingle && !label.isEmpty() && undoDepth() == depth + 1)) {
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

QString MainWindow::changeBlockedReason() const {
  if (windowReadOnly() && !readOnlyReason().isEmpty()) {
    return readOnlyReason();
  }
  if (m_mode == Mode::PickPlane) {
    return tr("Create Sketch is waiting for a plane: pick one or press Esc.");
  }
  return tr("Finish the command or the sketch first.");
}

void MainWindow::showStatus(const QString& message, NoticeLevel level) {
  showFeatureStatus(message, level, QString());
}

void MainWindow::showFeatureStatus(const QString& given, NoticeLevel level, const QString& feature) {
  // What raised it, when it lasts only while that is open (mitcad#1621):
  // showSourceError's, or the open command panel's for an error of no
  // feature (a refused OK); it goes when that closes (closeMessageSource).
  QString source = std::exchange(m_nextStatusSource, QString());
  if (source.isEmpty() && level == NoticeLevel::Error && feature.isEmpty() && m_session != nullptr) {
    source = QStringLiteral("command");
  }
  if (given.isEmpty() || level != NoticeLevel::Error) {
    source.clear();
  }
  // An edit refused on a linked component says where to change it, with
  // Open Linked Design (mitcad#1416, files/MainWindowLinks.cpp).
  if (level == NoticeLevel::Error && m_statusActionLabel.isEmpty() && showLinkedRefusal(given)) {
    return;
  }
  // The model's messages in the interface language (mitcad#1284): the
  // status line, the message band, its history and the log line of the UI
  // tests show the same text.
  const QString message = modelMessage(given);
  // A message whose button waits for an answer (mitcad#1416) stays in the
  // Docked status line, as in the band: a hint or a cleared status goes to
  // the history only.
  if (m_statusActionLabel.isEmpty() && m_statusButtonRun &&
      (level == NoticeLevel::Hint || level == NoticeLevel::Progress)) {
    if (m_messages != nullptr) {
      m_messages->post(message, level, feature);
    }
    return;
  }
  // One presentation per level (mitcad#1040, docs/ui-conventions.md
  // "Messages and severity"): the level's colour role and icon beside the
  // text, and the same level in the message band (Floating and AppKit,
  // mitcad#1042, mitcad#1098) and its history.
  m_statusIsHint = level == NoticeLevel::Hint || level == NoticeLevel::Progress;
  m_statusLevel = level;
  m_statusSource = source;   // mitcad#1621
  m_statusFeature = feature; // its notice places on hover (mitcad#1327)
  updateStatusNoticeHover(); // shown under the pointer: its own (mitcad#1609)
  if (level == NoticeLevel::Error) {
    setErrorStyleSheet(m_statusLabel);
  } else if (level == NoticeLevel::Warning) {
    setWarningStyleSheet(m_statusLabel);
  } else {
    setErrorStyleSheet(m_statusLabel, QString());
  }
  m_statusLabel->setText(message);
  const QIcon icon = noticeIcon(level);
  if (m_statusIcon == nullptr) {
    // (Not made yet: a message while the window is built.)
  } else if (icon.isNull() || message.isEmpty()) {
    m_statusIcon->clear();
    m_statusIcon->hide();
  } else {
    const int side = m_statusLabel->fontMetrics().height();
    m_statusIcon->setPixmap(icon.pixmap(QSize(side, side), devicePixelRatioF()));
    m_statusIcon->setToolTip(noticeLevelWord(level));
    m_statusIcon->setVisible(m_statusLabel->isVisibleTo(this));
  }
  // A message with a button (showActionStatus, mitcad#1416): in the
  // Floating band, and in the Docked status bar beside the message; any
  // other message takes the status bar's button away.
  const QString actionLabel = std::exchange(m_statusActionLabel, QString());
  std::function<void()> actionRun = std::exchange(m_statusActionRun, nullptr);
  if (m_statusActionButton != nullptr) {
    const bool shown = !actionLabel.isEmpty() && !message.isEmpty() && actionRun;
    m_statusButtonRun = shown ? actionRun : std::function<void()>();
    m_statusActionButton->setText(shown ? actionLabel : QString());
    m_statusActionButton->setAccessibleName(shown ? actionLabel : QString());
    m_statusActionButton->setVisible(shown);
    // Its place once the status bar is laid out (also when only its label
    // changed).
    TestSync::singleShot(0, this, [this] { logStatusActionPlace(); });
  }
  if (m_messages != nullptr) {
    if (!actionLabel.isEmpty()) {
      m_messages->postAction(message, level, actionLabel, std::move(actionRun));
    } else {
      m_messages->post(message, level, feature, source);
    }
  }
  if (!source.isEmpty()) {
    // Raised now: its source closing in the same turn (a tool that ends on
    // its own failure) leaves it, as it says why (closeMessageSource).
    m_freshSourceError = source;
    QTimer::singleShot(0, this, [this] { m_freshSourceError.clear(); });
    qDebug().noquote() << QStringLiteral("Status error source: %1").arg(source);
  }
  if (!message.isEmpty() && !m_statusIsHint) {
    // The level as the UI tests read it (mitcad#1040); hints are many.
    qDebug().noquote() << QStringLiteral("Status %1: %2").arg(noticeLevelName(level), message);
  }
  if (isProblem(level)) {
    qWarning().noquote() << message;
  }
  // A crash in the geometry kernel is an internal error (mitcad#62).
  if (level == NoticeLevel::Error && m_reports != nullptr) {
    m_reports->errorShown(message);
  }
}

void MainWindow::clearDesignMessages() {
  // Another design in the window (New, Open, Open Recent, a file from the
  // system, recovery, an import into a new design; mitcad#1174): the
  // previous design's message goes, an error too, from the Docked status
  // line and from the message band (its stub as well); the history keeps
  // it.
  m_statusIsHint = true;
  m_statusLevel = NoticeLevel::Hint;
  m_statusSource.clear(); // mitcad#1621
  // Its feature's places with it (mitcad#1609).
  m_statusFeature.clear();
  updateStatusNoticeHover();
  if (m_statusLabel != nullptr) {
    setErrorStyleSheet(m_statusLabel, QString());
    m_statusLabel->clear();
  }
  if (m_statusIcon != nullptr) {
    m_statusIcon->clear();
    m_statusIcon->hide();
  }
  // The old design's button with it (mitcad#1416).
  m_statusButtonRun = nullptr;
  if (m_statusActionButton != nullptr) {
    m_statusActionButton->hide();
  }
  qDebug().noquote() << QStringLiteral("Status line cleared (another design)");
  if (m_messages != nullptr) {
    m_messages->clear("another design");
  }
  // The section warning went with it (mitcad#1230): the new design's
  // sections report their own.
  m_sectionReport = SectionReport{};
}

void MainWindow::createSketchOn(const SelectionItem& plane) {
  if (m_mode == Mode::Idle && isSketchPlane(plane)) {
    leaveDrawing(); // mitcad#707
    setSketchKind(false); // the browser's Create Sketch makes a 2D one (mitcad#1524)
    // The design policy (mitcad#220): of the active component, and of the
    // plane's owner.
    QString refused = refreshSketchSupports();
    if (refused.isEmpty()) {
      refused = sketchSupportRefusal(plane);
    }
    if (!refused.isEmpty()) {
      showError(tr("Create Sketch: %1").arg(refused));
      qInfo().noquote() << QStringLiteral("Create Sketch refused: %1").arg(refused);
      return;
    }
    createSketchOnPick(plane);
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
        // A browser row of geometry the design policy leaves out.
        if (const QString refused = sketchSupportRefusal(item); !refused.isEmpty()) {
          showError(tr("Create Sketch: %1").arg(refused));
          qInfo().noquote() << QStringLiteral("Create Sketch refused: %1").arg(refused);
          return;
        }
        createSketchOnPick(item);
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
      // A flat spline face too (a sheet body's wall, mitcad#1554); the
      // normal outward.
      const std::optional<gp_Pln> planar = geometry::face_plane(face);
      if (!planar) {
        return;
      }
      const gp_Pln plane = *planar;
      normal = plane.Axis().Direction();
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

void MainWindow::fitView() {
  // The selection, outside commands and sketches (whose picks are inputs).
  if (m_mode == Mode::Idle && !m_selection.isEmpty()) {
    std::vector<gp_Pnt> corners;
    mcpItemCorners(m_selection, corners, nullptr);
    if (!corners.empty()) {
      Bnd_Box box;
      for (const gp_Pnt& corner : corners) {
        box.Add(corner);
      }
      m_viewer->frameBox(box);
      qDebug().noquote() << QStringLiteral("Fit: the selection");
      return;
    }
  }
  // Isolated bodies: those, not the origin and what else is shown.
  if (!m_isolation.isEmpty()) {
    const Bnd_Box box = m_viewer->bodiesBox();
    if (!box.IsVoid()) {
      m_viewer->frameBox(box);
      qDebug().noquote() << QStringLiteral("Fit: the isolated bodies");
      return;
    }
  }
  m_viewer->fitAll();
  qDebug().noquote() << QStringLiteral("Fit: everything shown");
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

void MainWindow::showHint(const QString& message) { showStatus(message, NoticeLevel::Hint); }

void MainWindow::showWarning(const QString& message) {
  // A warning, not an error (mitcad#655: 3D Print's export warnings), and
  // a warning on every surface (mitcad#1040).
  showStatus(message, NoticeLevel::Warning);
}

void MainWindow::showError(const QString& message) { showStatus(message, NoticeLevel::Error); }

void MainWindow::showSourceError(const QString& message, const QString& source) {
  m_nextStatusSource = source; // showFeatureStatus takes it
  showStatus(message, NoticeLevel::Error);
}

void MainWindow::closeMessageSource(const QString& source) {
  // An error of a tool, a value editor, a sketch or a command panel goes
  // when that closes (mitcad#1621): from the Docked status line and from
  // the message band (its stub, `cleared`); the history keeps it. Not one
  // raised in this same turn: the source closed on its own failure, and the
  // error says why.
  if (MessageLog::sourceCovers(source, m_freshSourceError)) {
    qDebug().noquote() << QStringLiteral("Message source closed: %1 (its error raised just now stays)").arg(source);
    return;
  }
  if (m_statusLabel != nullptr && !m_statusLabel->text().isEmpty() &&
      MessageLog::sourceCovers(source, m_statusSource)) {
    m_statusSource.clear();
    m_statusIsHint = true;
    m_statusLevel = NoticeLevel::Hint;
    m_statusFeature.clear();
    updateStatusNoticeHover();
    setErrorStyleSheet(m_statusLabel, QString());
    m_statusLabel->clear();
    if (m_statusIcon != nullptr) {
      m_statusIcon->clear();
      m_statusIcon->hide();
    }
    qDebug().noquote() << QStringLiteral("Status line cleared (%1 closed)").arg(source);
  }
  if (m_messages != nullptr) {
    m_messages->closeSource(source);
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

// The view's picks reach onPicked by a queued connection (above in the
// constructor): the ones emitted already are delivered now.
void MainWindow::deliverPendingPicks() {
  QCoreApplication::sendPostedEvents(this, QEvent::MetaCall);
}

void MainWindow::onPicked(const Selection& items, Qt::KeyboardModifiers modifiers, bool window) {
  if (m_nativeUi && m_session) {
    // The session invalidates an in-flight preview before queuing picks.
    m_session->picked(items, modifiers, window);
    return;
  }
  if (modelBusy()) {
    // A pick queued before a job began: it takes effect after the job.
    whenIdle(this, [this, items, modifiers, window] { onPicked(items, modifiers, window); });
    return;
  }
  if (m_session) {
    m_session->picked(items, modifiers, window);
    return;
  }
  // An annotation's pick or plane picked again (its editor's Pick Again…).
  if (m_annotationPick && m_mode == Mode::Idle) {
    if (window) {
      m_viewer->windowSelectionApplied();
    }
    annotationPicked(window ? Selection() : items);
    return;
  }
  if (m_mode == Mode::PickPlane) {
    qDebug().noquote() << QStringLiteral("Create Sketch picked %1").arg(summarize(items));
    for (const SelectionItem& item : items) {
      // The view's filter already leaves out what the design policy
      // refuses; a pick queued before it changed is checked again.
      if (isSketchPlane(item) && sketchSupportRefusal(item).isEmpty()) {
        // The planes lie behind the model (mitcad#844): one is not picked
        // through a face in front of it that the pick refuses (the filter
        // leaves that face out); the face says why instead.
        if (item.kind == SelectKind::Plane && !window) {
          if (const std::optional<SelectionItem> front = m_viewer->untestedClickItem();
              front && front->kind == SelectKind::Face) {
            refusePlanePick(*front);
            return;
          }
        }
        createSketchOnPick(item);
        return;
      }
    }
    // Nothing that takes the sketch: what was clicked says why
    // (mitcad#474), a face that is not planar or one the design policy
    // leaves out, which the view's filter does not pick.
    if (!items.isEmpty()) {
      refusePlanePick(items.first());
    } else if (const std::optional<SelectionItem> refused = window ? std::nullopt : m_viewer->takeRefusedClick()) {
      refusePlanePick(*refused);
    }
    return;
  }
  if (m_mode == Mode::Sketch && m_sketch->tool() != nullptr) {
    return;
  }
  // A click selects, Ctrl or Shift adds or removes; a window
  // selects what it holds of one kind. In sketch mode (mitcad#217) Shift
  // widens a click to the filter's empty rows instead: only Ctrl adds or
  // removes there, and a window (Shift adds it as before) keeps every kind
  // the filter let it take.
  const bool sketching = m_mode == Mode::Sketch;
  // Click adds to the selection in sketches (mitcad#1403): a click with no
  // tool running adds or removes as Ctrl+click does, and one on empty space
  // clears (Ctrl+click on empty space keeps the selection, as before).
  const bool clickAdds = sketching && !window && sketch::clickAddsToSelection();
  const bool add = clickAdds                ? (!items.isEmpty() || modifiers.testFlag(Qt::ControlModifier))
                   : (sketching && !window) ? modifiers.testFlag(Qt::ControlModifier)
                                            : bool(modifiers & (Qt::ControlModifier | Qt::ShiftModifier));
  Selection next = add ? m_selection : Selection();
  if (window) {
    // The main window's part of a window selection (mitcad#670): the
    // sketch's annotations in it, and the selection set and highlighted.
    ScopedTiming timing("window selection");
    const Selection taken = sketching ? sketchWindowItems(items) : items;
    const SelectKind kind = sketching ? SelectKind::None : preferredKind(items);
    // By a set: a window over a large design takes thousands (mitcad#1631).
    QSet<SelectionItem> kept(next.cbegin(), next.cend());
    for (const SelectionItem& item : taken) {
      if ((sketching || item.kind == kind) && !kept.contains(item)) {
        kept.insert(item);
        next.append(item);
      }
    }
    timing.setDetail(QStringLiteral("%1 item(s)").arg(next.size()));
    setSelection(next);
    timing.finish();
    m_viewer->windowSelectionApplied();
    return;
  }
  if (!items.isEmpty()) {
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
  // BrowserController queues widget signals, then calls pickItems synchronously.
  // Read the feature context in that host call, rather than racing the queued
  // selection with a separate direct timeline signal connection.
  m_timelineSelection = m_controller ? m_controller->pickingFeatures() : QStringList();
  const bool changed = items != m_selection;
  m_selection = items;
  if (changed) {
    // A selection of the view's own takes an annotation's highlight
    // away (the Annotations window highlights through
    // highlightAnnotations, not here).
    dropAnnotationHighlight();
    m_idleMeasurements.clear();
    m_locks->activity(); // a selection keeps the edit lock active (mitcad#89)
    // The first items by name: a whole large design selected (mitcad#1631)
    // would write tens of thousands.
    constexpr int kLoggedNames = 100;
    QStringList names;
    for (const SelectionItem& item : items.mid(0, kLoggedNames)) {
      names << item.describe();
    }
    if (items.size() > kLoggedNames) {
      names << QStringLiteral("and %1 more").arg(items.size() - kLoggedNames);
    }
    qDebug().noquote() << QStringLiteral("Selected: %1%2").arg(
        summarize(items),
        names.isEmpty() ? QString() : QStringLiteral(" [%1]").arg(names.join(QStringLiteral("; "))));
  }
  // Each part's time, so that a slow selection names its slow part
  // (mitcad#981): the view's highlights and the card, the sketch, the
  // browser and timeline.
  QElapsedTimer selectionClock;
  selectionClock.start();
  m_idleHighlightMs = 0;
  m_idleCardMs = 0;
  m_idleMeasureMs = 0;
  updateIdleView();
  const qint64 viewMs = selectionClock.restart();
  m_sketch->refreshView();
  if (m_sketch3d != nullptr && m_sketch3d->isActive()) {
    m_palette3d->refresh(); // the selection described (mitcad#1524)
  }
  const qint64 sketchMs = selectionClock.restart();
  m_controller->showSelection(m_selection);
#ifdef MITCAD_MACOS_NATIVE_UI
  if (m_nativeWindow) {
    // A collection pick need not change the document or refresh the whole
    // native window. Its peer collections still need the current selection.
    m_nativeWindow->setSelection(m_controller->browser()->selectedPresentationKeys());
    m_nativeWindow->setHistorySelection(m_controller->timeline()->selectedFeatures());
  }
#endif
  const qint64 browserMs = selectionClock.elapsed();
  if (changed) {
    qDebug().noquote() << QStringLiteral("Selection timing: view %1 ms (highlights %2, card %3, measure %4), "
                                         "sketch %5 ms, browser %6 ms")
                              .arg(viewMs)
                              .arg(m_idleHighlightMs)
                              .arg(m_idleCardMs)
                              .arg(m_idleMeasureMs)
                              .arg(sketchMs)
                              .arg(browserMs);
  }
}

void MainWindow::updateIdleView() {
  // An annotation's pick waited for ends when a command or a sketch starts.
  if (m_mode != Mode::Idle) {
    m_annotationPick.reset();
  }
  // The command panel's notice places go with the command (mitcad#1327).
  if (m_mode != Mode::Command && m_noticeHoverSource == QStringLiteral("panel")) {
    m_noticeHoverSource.clear();
    m_noticeHover = QJsonArray();
    m_noticeHoverBodies.clear();
  }
  m_autoselect->setActive(m_mode == Mode::Idle);
  updateSelectionCreate();
  updateMakeCard(); // the Make tab's body card (mitcad#1049)
  updateModeBanner();
  if (m_mode == Mode::Command) {
    return;
  }
  m_viewer->setHoverColor(kHoverColor);
  if (m_mode == Mode::PickPlane) {
    // Faces and planes the design policy leaves out (mitcad#220) are not
    // picked or highlighted, whatever else the pick would allow.
    m_viewer->setPickFilter(SelectKind::Plane | SelectKind::Face, [this](const SelectionItem& item) {
      return isSketchPlane(item) && sketchSupportRefusal(item).isEmpty();
    });
    m_viewer->setHighlights({});
    return;
  }
  // A sketch tool takes the clicks; nothing is picked or highlighted. In
  // sketch mode, the sketch's selection filter decides (mitcad#217): only
  // the edited sketch's geometry, never another sketch's.
  const bool drawing = m_mode == Mode::Sketch && m_sketch->tool() != nullptr;
  if (m_mode == Mode::Sketch && m_sketch3d != nullptr && m_sketch3d->isActive() && m_sketch3d->pickingNormal()) {
    // A Center Point Arc's Pick Normal (mitcad#1525): lines and edges,
    // datum axes and planar faces.
    m_viewer->setPickFilter(SelectKind::SketchCurve | SelectKind::Edge | SelectKind::Axis | SelectKind::Face,
                            [](const SelectionItem& item) {
                              return item.kind != SelectKind::Face || item.geometry == QStringLiteral("plane");
                            });
  } else if (m_mode == Mode::Sketch && m_sketch3d != nullptr && m_sketch3d->isActive() &&
             m_sketch3d->sourcePick() == sketch::Sketch3dController::SourcePick::Plane) {
    // The plane of a Symmetric or On Plane relation (mitcad#1661): planar
    // faces, construction planes and other sketches (their planes).
    m_viewer->setPickFilter(SelectKind::Face | SelectKind::Plane | SelectKind::SketchPoint | SelectKind::SketchCurve,
                            [this](const SelectionItem& item) {
                              if (item.kind == SelectKind::Face) {
                                return item.geometry == QStringLiteral("plane");
                              }
                              return item.kind == SelectKind::Plane || item.owner != m_sketch3d->uid();
                            });
  } else if (m_mode == Mode::Sketch && m_sketch3d != nullptr && m_sketch3d->isActive() &&
             m_sketch3d->picksSources()) {
    // A 3D sketch's drawing tool, or its Attach and Replace Reference
    // (mitcad#1526), snaps to what the view shows (mitcad#1525):
    // points and curves of sketches, body vertices, edges and faces
    // (mitcad#1549), Intersection Curves; the clicks are the tool's.
    m_viewer->setPickFilter(SelectKind::SketchPoint | SelectKind::SketchCurve | SelectKind::Vertex |
                                SelectKind::Edge | SelectKind::Face | SelectKind::Feature,
                            [this](const SelectionItem& item) {
                              if (item.kind != SelectKind::Feature) {
                                return true;
                              }
                              const DocumentSnapshot::Feature* feature = m_snapshot.feature(item.owner);
                              return feature != nullptr && feature->type == QStringLiteral("intersection_curve");
                            });
    // Where a click meets each face and each body vertex, for UI tests
    // (MITCAD_LOG_PICKS): a point snapped onto a face (mitcad#1549) or
    // onto a vertex of another component (mitcad#1550, the route of
    // GitHub mitcad/Mitcad#202); once as the pick starts.
    if (m_pickPlacesFilter != kSketch3dSourcePlaces) {
      m_pickPlacesFilter = kSketch3dSourcePlaces;
      logPickPlaces(m_pickPlacesFilter);
    }
  } else if (m_mode == Mode::Sketch && m_sketch3d != nullptr && m_sketch3d->isActive()) {
    // A 3D sketch's own points and curves (mitcad#1524).
    if (m_pickPlacesFilter == kSketch3dSourcePlaces) {
      m_pickPlacesFilter = SelectFilter();
    }
    const QString uid = m_sketch3d->uid();
    m_viewer->setPickFilter(SelectKind::SketchPoint | SelectKind::SketchCurve,
                            [uid](const SelectionItem& item) { return item.owner == uid; });
  } else if (m_mode == Mode::Sketch) {
    applySketchScope(drawing);
  } else {
    m_viewer->setPickFilter(idleFilter());
    applyAnnotationPickFilter(); // an annotation's pick waited for
  }
  QElapsedTimer idleClock; // the Selection timing line's parts (mitcad#981)
  idleClock.start();
  m_idleMeasureMs = 0;
  updateIdleHighlights();
  m_idleHighlightMs = idleClock.restart();
  updateIdleCard();
  m_idleCardMs = idleClock.elapsed();
  updateCommandCard();
#ifdef MITCAD_MACOS_NATIVE_UI
  refreshNativeInspector();
#endif
}

// The view's highlights of the selection, the hovered and autoselected
// items, the print bed face shown, joints and annotations (updateIdleView;
// alone where only they change, mitcad#981).
void MainWindow::updateIdleHighlights() {
  std::vector<HighlightDisplay> highlights;
  QStringList featureFaces; // "F4 1": a selected feature's faces (mitcad#28)
  QStringList featureInputs; // "Fillet1 edges 4": its picked edges (mitcad#957)
  // A feature's picked edges where they were (mitcad#957): lines in the
  // selection colour, dashed where the solids hide them, nothing picked on
  // them; a null shape when the feature kept none. `count` gets how many.
  const auto inputLines = [this](const SelectionItem& feature, int& count) {
    HighlightDisplay drawn{feature, featureInputLines(feature, &count), kSelectionColor};
    drawn.inputLines = true;
    drawn.pickable = false;
    drawn.throughSolid = true;
    return drawn;
  };
  std::optional<SketchShapeBatch> batch(std::in_place, *this);
  for (const SelectionItem& item : m_selection) {
    if (item.kind == SelectKind::Profile && m_sketch->draggingGeometry() && item.owner == m_sketch->uid()) {
      continue; // retain the selection without rebuilding its face during a drag
    }
    if (item.kind == SelectKind::Feature && !jointEntry(item.owner).isEmpty()) {
      continue; // a joint makes no faces: its sides are shown (addJointHighlights)
    }
    if (item.kind != SelectKind::SketchConstraint && item.kind != SelectKind::SketchDimension) {
      const TopoDS_Shape shape = itemShape(item);
      if (item.kind == SelectKind::Feature) {
        NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> faces;
        TopExp::MapShapes(shape, TopAbs_FACE, faces);
        featureFaces << QStringLiteral("%1 %2").arg(item.owner).arg(faces.Extent());
      }
      HighlightDisplay shown{item, shape, kSelectionColor};
      shown.throughSolid = item.kind == SelectKind::Feature ||
          (item.kind == SelectKind::Body && m_timelineSelection.contains(item.owner.section(QLatin1Char('.'), 0, 0)));
      if (item.kind == SelectKind::Face || item.kind == SelectKind::Edge) {
        shown.body = bodyShape(item.owner);
      }
      // No click outside a command takes a component (idleFilter has no
      // such kind): its highlight takes none, so that the view draws it
      // from its bodies' own presentations, not as a new object of all
      // their faces (mitcad#1631: the root component of a large design).
      if (item.kind == SelectKind::Component) {
        shown.pickable = false;
      }
      // A Fillet's or Chamfer's picked edges where they were, its faces
      // in a colour apart from them (mitcad#957).
      int inputCount = 0;
      HighlightDisplay lines = item.kind == SelectKind::Feature ? inputLines(item, inputCount) : HighlightDisplay{};
      if (!lines.shape.IsNull()) {
        shown.color = kFeatureFacesColor;
        const DocumentSnapshot::Feature* feature = m_snapshot.feature(item.owner);
        featureInputs << QStringLiteral("%1 edges %2").arg(feature ? feature->name : item.owner).arg(inputCount);
      }
      highlights.push_back(std::move(shown));
      if (!lines.shape.IsNull()) {
        highlights.push_back(std::move(lines));
      }
    }
  }
  for (const SelectionItem& item : m_autoselectPreview) {
    if (!m_selection.contains(item)) {
      highlights.push_back({item, itemShape(item), kHoverColor});
    }
  }
  // A body's print bed face while a face's context menu is open
  // (mitcad#731); it takes no clicks.
  if (m_printBedShown.isValid()) {
    HighlightDisplay shown{m_printBedShown, itemShape(m_printBedShown), kPrintBedColor};
    shown.pickable = false;
    shown.body = bodyShape(m_printBedShown.owner);
    highlights.push_back(std::move(shown));
  }
  QStringList hoverFaces;
  if (m_mode == Mode::Idle) for (const QString& uid : m_timelineHover) {
    const auto* feature = m_snapshot.feature(uid);
    if (!feature || feature->index >= m_snapshot.marker || !feature->succeeded() || feature->suppressed) continue;
    SelectionItem item{SelectKind::Feature, uid, QString(), QString(), m_snapshot.firstOccurrence(feature->component)};
    const TopoDS_Shape shape = itemShape(item);
    NCollection_IndexedMap<TopoDS_Shape, TopTools_ShapeMapHasher> faces;
    TopExp::MapShapes(shape, TopAbs_FACE, faces);
    hoverFaces << QStringLiteral("%1 %2").arg(uid).arg(faces.Extent());
    const bool alreadySelected = m_selection.contains(item) || m_timelineSelection.contains(uid) ||
        std::any_of(m_selection.cbegin(), m_selection.cend(), [&uid](const SelectionItem& selected) {
          return selected.kind == SelectKind::Body && selected.owner.section(QLatin1Char('.'), 0, 0) == uid;
        });
    if (alreadySelected) continue;
    // The hover's colour and look, as a hovered Browser row's bodies
    // (mitcad#1041, setBrowserHover): in sight as the view's hover, fainter
    // through what hides them, with their outlines; still the feature's own
    // faces, not whole bodies.
    HighlightDisplay shown{item, shape, kHoverColor};
    shown.pickable = false;
    shown.hoverLook = true;
    // The hovered Fillet's or Chamfer's picked edges too (mitcad#957).
    int inputCount = 0;
    HighlightDisplay lines = inputLines(item, inputCount);
    if (!lines.shape.IsNull()) {
      shown.color = kFeatureFacesHoverColor;
      // How many picked edges are drawn, in the log too (mitcad#1076).
      hoverFaces.last() += QStringLiteral(" (edges %1)").arg(inputCount);
    }
    highlights.push_back(std::move(shown));
    if (!lines.shape.IsNull()) {
      highlights.push_back(std::move(lines));
    }
  }
  const QString hovered = hoverFaces.isEmpty() ? QStringLiteral("none") : hoverFaces.join(QStringLiteral(", "));
  if (hovered != m_loggedTimelineHover) {
    m_loggedTimelineHover = hovered;
    qDebug().noquote() << QStringLiteral("Timeline hover highlighted: %1").arg(hovered);
  }
  // The places of a notice under the pointer (mitcad#1327).
  for (HighlightDisplay& place : noticeHighlights()) {
    highlights.push_back(std::move(place));
  }
  // The selected joints' sides and frames, and the hovered joint row's
  // (mitcad#523).
  addJointHighlights(highlights);
  // The picks of the annotations highlighted (mitcad#702).
  addAnnotationHighlights(highlights);
  batch.reset();
  m_viewer->setHighlights(highlights);
  const QString logged = featureFaces.isEmpty() ? QStringLiteral("none") : featureFaces.join(QStringLiteral(", "));
  if (logged != m_loggedFeatureFaces) {
    m_loggedFeatureFaces = logged;
    qDebug().noquote() << QStringLiteral("Feature faces highlighted: %1").arg(logged);
  }
  // The selected features' picked edges drawn where they were (mitcad#957).
  const QString loggedInputs =
      featureInputs.isEmpty() ? QStringLiteral("none") : featureInputs.join(QStringLiteral(", "));
  if (loggedInputs != m_loggedFeatureInputs) {
    m_loggedFeatureInputs = loggedInputs;
    qDebug().noquote() << QStringLiteral("Feature inputs highlighted: %1").arg(loggedInputs);
  }
}

// The idle card's text: the selection, its measurements (measured once per
// selection, m_idleMeasurements) and the selected features' and joints'
// cards (updateIdleView).
void MainWindow::updateIdleCard() {
  QString text = tr("<b>No command</b><br>Press S to search for a command, or right-click the "
                    "selection for the commands that fit it.");
  QStringList jointCards; // the selected joints' lines of the card (mitcad#523)
  // A failed feature selected on the timeline has no faces to keep in the
  // selection: the timeline's selection alone has a card, with why it
  // failed (mitcad#961).
  const bool timelineOnly =
      m_selection.isEmpty() && std::any_of(m_timelineSelection.cbegin(), m_timelineSelection.cend(),
                                           [this](const QString& uid) { return m_snapshot.feature(uid) != nullptr; });
  const bool anySelected = !m_selection.isEmpty() || timelineOnly;
  if (anySelected) {
    QStringList featureNames;
    QStringList lines;
    QHash<QString, QString> featureCards;
    // A selected feature's card: a joint's sides (mitcad#523) and a Move
    // Face's motion (mitcad#531) have cards of their own, any other feature
    // its kind, component and sizes. A failed feature's card ends with a
    // red line of why, one with a warning a yellow one (mitcad#961).
    const auto featureCard = [&](const DocumentSnapshot::Feature& feature, QString* heading) {
      const QString status = featureStatusLine(feature.status, feature.error, errorColor(m_idleInfo->palette()),
                                               warningColor(m_idleInfo->palette()));
      // The card's level, for UI tests (mitcad#1040).
      if (!status.isEmpty()) {
        const QString line = QStringLiteral("%1: %2").arg(
            feature.name, noticeLevelName(featureStateLevel(featureState(feature.status))));
        if (line != m_loggedCardLevel) {
          m_loggedCardLevel = line;
          qDebug().noquote() << QStringLiteral("Selection card level %1").arg(line);
        }
      }
      const auto withStatus = [&status](const QString& card) {
        return status.isEmpty() ? card : QStringLiteral("%1<br>%2").arg(card, status);
      };
      const auto selected = std::find_if(m_selection.cbegin(), m_selection.cend(), [&feature](const SelectionItem& item) {
        return item.kind == SelectKind::Feature && item.owner == feature.uid;
      });
      if (selected != m_selection.cend()) {
        if (const QString joint = jointCardBlock(*selected); !joint.isEmpty()) {
          jointCards << joint;
          return withStatus(joint);
        }
        if (const QString moved = moveFaceCard(*selected); !moved.isEmpty()) {
          return withStatus(moved);
        }
      }
      return withStatus(featurePresentation(feature, m_snapshot, *this, *m_registry, heading));
    };
    for (const QString& uid : m_timelineSelection.mid(0, 8)) {
      if (const auto* feature = m_snapshot.feature(uid)) {
        featureNames << feature->name;
        lines << featureCard(*feature, nullptr);
      }
    }
    if (m_timelineSelection.size() > 8) featureNames << tr("%n more feature(s)", nullptr, m_timelineSelection.size() - 8);
    if (m_timelineSelection.isEmpty()) {
      // Browser and MCP feature selections use the same named heading.
      // Keep the ordinary counts of faces, bodies and other mixed items.
      Selection others;
      int featureCount = 0;
      for (const SelectionItem& selected : m_selection) {
        if (selected.kind == SelectKind::Feature) ++featureCount;
        else others.append(selected);
      }
      int named = 0;
      for (const SelectionItem& selected : m_selection.mid(0, 8)) {
        if (selected.kind != SelectKind::Feature) continue;
        if (const auto* feature = m_snapshot.feature(selected.owner)) {
          QString heading;
          featureCards.insert(selected.owner, featureCard(*feature, &heading));
          featureNames << (heading.isEmpty() ? feature->name : heading);
          ++named;
        }
      }
      if (featureCount > named) featureNames << tr("%n more feature(s)", nullptr, featureCount - named);
      if (!featureNames.isEmpty() && !others.isEmpty()) featureNames << summarizeText(others);
    }
    text += tr("<br><br>Selected: %1").arg(featureNames.isEmpty() ? summarizeText(m_selection)
                                                              : featureNames.join(QStringLiteral(", ")).toHtmlEscaped());
    QStringList details;
    for (const SelectionItem& item : m_selection.mid(0, 8)) {
      if (m_timelineSelection.contains(item.owner) ||
          (item.kind == SelectKind::Body && m_timelineSelection.contains(item.owner.section(QLatin1Char('.'), 0, 0)))) continue;
      if (item.kind == SelectKind::Feature) {
        if (const auto* feature = m_snapshot.feature(item.owner)) {
          const QString presentation =
              featureCards.contains(item.owner) ? featureCards.value(item.owner) : featureCard(*feature, nullptr);
          lines << presentation;
          details << feature->name;
          continue;
        }
      }
      details << item.describe();
      // A joint: its kind and what each side is on (mitcad#523).
      if (const QString joint = jointCardBlock(item); !joint.isEmpty()) {
        lines << joint;
        jointCards << joint;
        continue;
      }
      QString label = SelectionItem::kindText(item.kind);
      if (!label.isEmpty()) {
        label[0] = label[0].toUpper();
      }
      QString measurements;
      if (item.kind == SelectKind::Face || item.kind == SelectKind::Edge || item.kind == SelectKind::Body) {
        // With its occurrence (mitcad#646): a component placed more than
        // once is measured in the occurrence picked.
        QJsonObject reference = item.reference().toObject();
        if (!item.occurrence.isEmpty()) {
          reference.insert(QStringLiteral("occurrence"), item.occurrence);
        }
        const QString key = QString::fromUtf8(compactJson(reference));
        if (!m_idleMeasurements.contains(key)) {
          QElapsedTimer measureClock;
          measureClock.start();
          try {
            const QJsonObject answer = queryObject({{QStringLiteral("query"), QStringLiteral("measure")},
                                                    {QStringLiteral("a"), reference}});
            m_idleMeasurements.insert(key, selectionMeasurementText(answer.value(QStringLiteral("a")).toObject()));
          } catch (const std::exception&) {
            m_idleMeasurements.insert(key, tr("Measurements unavailable"));
          }
          m_idleMeasureMs += measureClock.elapsed();
        }
        measurements = m_idleMeasurements.value(key);
      }
      const QString owners = selectionOwnerText(item, m_snapshot);
      if (!owners.isEmpty()) {
        QString block = QStringLiteral("<b>%1</b><br>%2").arg(label.toHtmlEscaped(), owners);
        if (!measurements.isEmpty()) {
          block += QStringLiteral("<br>%1").arg(measurements);
        }
        lines << block;
      } else if (const QString moved = moveFaceCard(item); !moved.isEmpty()) {
        lines << moved; // a Move Face's timeline cell (mitcad#531)
      } else if (measurements.isEmpty()) {
        lines << item.describe().toHtmlEscaped();
      } else {
        lines << QStringLiteral("<b>%1</b><br>%2").arg(label.toHtmlEscaped(), measurements);
      }
    }
    if (m_selection.size() == 2 && m_selection[0].kind == SelectKind::Face &&
        m_selection[1].kind == SelectKind::Face) {
      details << tr("Maximum distance: largest nearest-face gap in either direction");
      QJsonObject picks[2];
      for (int i = 0; i < 2; ++i) {
        picks[i] = m_selection[i].reference().toObject();
        if (!m_selection[i].occurrence.isEmpty()) {
          picks[i].insert(QStringLiteral("occurrence"), m_selection[i].occurrence);
        }
      }
      const QJsonObject query{{QStringLiteral("query"), QStringLiteral("measure")},
                              {QStringLiteral("a"), picks[0]}, {QStringLiteral("b"), picks[1]},
                              {QStringLiteral("face_range"), true}};
      const QString key = QStringLiteral("pair:") + QString::fromUtf8(compactJson(query));
      if (!m_idleMeasurements.contains(key)) {
        QString pair = tr("<b>Between faces</b><br>Measurements unavailable");
        try {
          const QJsonObject between = queryObject(query).value(QStringLiteral("between")).toObject();
          const auto format = [this](double distance) {
            return measurementValueText(QStringLiteral("distance"), distance, m_snapshot.lengthUnit,
                                        lengthUnitMillimetres(m_snapshot.lengthUnit)).toHtmlEscaped();
          };
          const QJsonValue minimum = between.value(QStringLiteral("distance"));
          const QJsonValue offset = between.value(QStringLiteral("offset"));
          if (minimum.isDouble() && std::isfinite(minimum.toDouble())) {
            if (offset.isDouble() && std::isfinite(offset.toDouble())) {
              pair = tr("<b>Between faces</b><br>Distance: <b>%1</b>").arg(format(offset.toDouble()));
              // Parallel planes can overlap outside their bounded faces.
              // Preserve the actual minimum too when it is longer.
              if (std::abs(minimum.toDouble() - offset.toDouble()) > 1e-6) {
                pair += tr("<br>Minimum distance: <b>%1</b>").arg(format(minimum.toDouble()));
              }
            } else {
              pair = tr("<b>Between faces</b><br>Minimum distance: <b>%1</b>").arg(format(minimum.toDouble()));
              const QJsonValue maximum = between.value(QStringLiteral("maximum_distance"));
              pair += maximum.isDouble() && std::isfinite(maximum.toDouble())
                          ? tr("<br>Maximum distance: <b>%1</b>").arg(format(maximum.toDouble()))
                          : tr("<br>Maximum distance: unavailable (requires convex planar polygons)");
            }
          }
        } catch (const std::exception&) {
          // No previous pair value survives a failed query or a refresh.
        }
        m_idleMeasurements.insert(key, pair);
      }
      lines << m_idleMeasurements.value(key);
    }
    text += QStringLiteral("<br><br>%1").arg(lines.join(QStringLiteral("<br><br>")));
    if (m_selection.size() > 8) {
      text += tr("<br><br>Showing the first 8 items.");
    }
    m_idleInfo->setToolTip(details.join(QLatin1Char('\n')));
  } else {
    m_idleInfo->setToolTip(QString());
  }
  if (m_nativeUi || chromeStyle() == ChromeStyle::Floating) {
    // The card is for what there is to say: the selection, not the hint.
    text = anySelected ? text.mid(text.indexOf(QStringLiteral("<br><br>")) + 8) : QString();
  }
  m_idleInfo->setText(text);
  m_idleInfo->setVisible(!text.isEmpty());
  // What the card says of the selected joints, for UI tests.
  const QString cards = jointCards.join(QStringLiteral("; "))
                            .remove(QStringLiteral("<b>"))
                            .remove(QStringLiteral("</b>"));
  if (cards != m_loggedJointCard) {
    m_loggedJointCard = cards;
    if (!cards.isEmpty()) {
      qDebug().noquote() << QStringLiteral("Joint card: %1").arg(cards);
    }
  }
}

void MainWindow::showContextMenu(const QPoint& globalPosition, const SelectionItem& item) {
  if (modelBusy()) {
    qDebug() << "Context menu dropped: the model is computing";
    return; // asked for before a job began; a menu after it would surprise
  }
  if (!m_session && m_mode == Mode::Sketch && m_sketch->tool() != nullptr) {
    // A tool's own menu (mitcad#1540): Sketch Dimension's Center or
    // Tangent for a circle or arc and Driving or Driven.
    const std::vector<sketch::ToolMenuEntry> own = m_sketch->toolMenu(m_viewer->mapFromGlobal(globalPosition));
    if (!own.empty()) {
      QMenu toolMenu(this);
      QStringList shown;
      for (const sketch::ToolMenuEntry& entry : own) {
        if (entry.separatorBefore) {
          toolMenu.addSeparator();
        }
        QAction* action = toolMenu.addAction(entry.text);
        action->setCheckable(entry.checked);
        action->setChecked(entry.checked);
        connect(action, &QAction::triggered, &toolMenu, entry.run);
        shown << (entry.checked ? entry.text + QStringLiteral(" (current)") : entry.text);
      }
      qDebug().noquote() << QStringLiteral("Context menu: %1").arg(shown.join(QStringLiteral(" | ")));
      execMenu(&toolMenu, globalPosition);
      return;
    }
    // A right click while a sketch tool runs is the tool's Esc, without a
    // menu (mitcad#1085): it ends the chain or the shape in progress (the
    // Line's chain), or the tool when nothing is in progress. A right drag
    // moves the view and never comes here (OcctViewer::mouseReleaseEvent).
    qDebug() << "Sketch right-click: Esc of the tool";
    m_sketch->cancel();
    return;
  }
  QElapsedTimer menuClock;
  menuClock.start();
  qint64 selectMs = 0;
  QMenu menu(this);
  QStringList entries;
  const auto add = [&menu, &entries](QAction* action) {
    if (menu.actions().contains(action)) {
      return; // offered already (an edit panel also fitting the selection)
    }
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
                      [input](const SelectionItem& s) { return input->takes(s); })) {
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
    // A dimension or a constraint's symbol right-clicked was selected
    // by the sketch (SketchController::viewerEvent): not the geometry
    // under it.
    if (item.isValid() && !m_selection.contains(item) &&
        !m_sketch->annotationAt(m_viewer->mapFromGlobal(globalPosition))) {
      // With Click adds to the selection in sketches (mitcad#1403) the
      // item joins the selection, so that the menu offers its commands for
      // all of them; a right click never clears it.
      Selection next = sketch::clickAddsToSelection() ? m_selection : Selection();
      next.append(item);
      setSelection(next);
    }
    const bool sketchItems = std::any_of(m_selection.begin(), m_selection.end(),
                                         [](const SelectionItem& s) { return isSketchKind(s.kind); });
    if (sketchItems) {
      add(m_registry->action(QStringLiteral("sketch.delete")));
      add(m_registry->action(QStringLiteral("sketch.construction")));
      add(m_registry->action(QStringLiteral("sketch.constraint.fix")));
      // Projected geometry into ordinary sketch geometry (mitcad#257).
      if (m_sketch->selectionHasProjected()) {
        add(m_registry->action(QStringLiteral("sketch.convert_projected")));
      }
      // Inactive constraints and dimensions (mitcad#721): Activate applies
      // them where the sketch then solves; active ones can be set aside.
      const QStringList inactive = m_sketch->selectedItems(false);
      const QStringList active = m_sketch->selectedItems(true);
      if (!inactive.isEmpty()) {
        addCall(QStringLiteral("constraint"), tr("Activate"),
                [this, inactive] { m_sketch->setItemsActive(inactive, true); });
      } else if (!active.isEmpty()) {
        addCall(QStringLiteral("constraint"), tr("Deactivate"),
                [this, active] { m_sketch->setItemsActive(active, false); });
      }
      if (m_selection.size() == 1 && m_selection.first().kind == SelectKind::SketchDimension) {
        const QString id = m_selection.first().name;
        addCall(QStringLiteral("dimension"), tr("Edit Dimension"), [this, id] { m_sketch->editDimension(id); });
        // A horizontal or vertical distance switches between signed and
        // unsigned, its point kept where it is (mitcad#409).
        const sketch::DimensionData* d = m_sketch->model().dimension(id);
        if (d != nullptr && d->isAxisDistance()) {
          const bool on = !d->signedOffset;
          addCall(QStringLiteral("dimension"), on ? tr("Make Distance Signed") : tr("Make Distance Unsigned"),
                  [this, id, on] { m_sketch->setDimensionSigned(id, on); });
        }
        // Its own parameter kept as a user parameter, which the dimension
        // then uses at the same value (mitcad#1535).
        if (const QString own = m_sketch->ownParameter(id); !own.isEmpty()) {
          addCall(QStringLiteral("parameters"), tr("Move %1 to User Parameters").arg(own),
                  [this, id] { m_sketch->keepAsUserParameter(id); });
        }
        // A name given to its parameter taken off: the automatic d<n> back,
        // and the uses follow (mitcad#1577).
        if (const QString named = m_sketch->namedParameter(id); !named.isEmpty()) {
          addCall(QStringLiteral("parameters"), tr("Remove Name %1").arg(named),
                  [this, id] { m_sketch->removeDimensionName(id); });
        }
      }
      // A spline right-clicked gets a point there (Insert Point,
      // mitcad#836): a fit point, or a control point that keeps its shape.
      if (m_selection.size() == 1 && m_selection.first().kind == SelectKind::SketchCurve) {
        const QString spline = m_selection.first().name;
        const QPointF clicked = m_viewer->mapFromGlobal(globalPosition);
        if (m_sketch->splineAt(clicked) == spline) {
          addCall(QStringLiteral("point"), tr("Insert Point"),
                  [this, spline, clicked] { m_sketch->insertSplinePoint(spline, clicked); });
        }
      }
      // Texts, patterns, offsets and equal spacings have edit panels (P3,
      // P4, mitcad#221).
      if (m_selection.size() == 1) {
        const QString id = m_selection.first().name;
        const sketch::SketchModel& model = m_sketch->model();
        if (const sketch::ConstraintData* k =
                m_selection.first().kind == SelectKind::SketchConstraint ? model.constraint(id) : nullptr;
            k != nullptr && k->type == QStringLiteral("equal_spacing")) {
          add(m_registry->action(QStringLiteral("sketch.edit_equal_spacing")));
        }
        if (const sketch::TextData* text = model.text(id); text != nullptr) {
          // A QR code has a panel of its own (mitcad#473).
          add(m_registry->action(text->code ? QStringLiteral("sketch.edit_code") : QStringLiteral("sketch.edit_text")));
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
    } else if (const sketch::TextData* text = m_selection.size() == 1 && m_selection.first().kind == SelectKind::Profile
                                                   ? m_sketch->model().textOfPick(m_selection.first())
                                                   : nullptr;
               text != nullptr) {
      // A region a text's contours bound, inside a letter or its counter
      // (the hole of an 'o'), stands for the text (mitcad#1369): zoomed in,
      // a click there picks the region, not the outline. Edit Text's input
      // takes the region as the text.
      add(m_registry->action(text->code ? QStringLiteral("sketch.edit_code") : QStringLiteral("sketch.edit_text")));
      menu.addSeparator();
    } else if (!m_selection.isEmpty()) {
      // The model's faces, edges and vertices go into the sketch: Project,
      // and Intersect (mitcad#736) for faces, edges and bodies.
      bool offered = false;
      for (const char* id : {"sketch.project", "sketch.intersect"}) {
        const CommandDef* reference = m_registry->find(QString::fromLatin1(id));
        const InputDef* input = reference != nullptr ? reference->primarySelection() : nullptr;
        if (input != nullptr && isAvailable(*reference) &&
            std::all_of(m_selection.begin(), m_selection.end(),
                        [input](const SelectionItem& s) { return input->accepts(s); })) {
          add(m_registry->action(reference->id));
          offered = true;
        }
      }
      if (offered) {
        menu.addSeparator();
      }
    }
    // Copy, Cut and Paste of sketch geometry (mitcad#788); not of the
    // sketch's axes and origin (mitcad#899).
    if (std::any_of(m_selection.begin(), m_selection.end(), [](const SelectionItem& s) {
          return (s.kind == SelectKind::SketchCurve || s.kind == SelectKind::SketchPoint) &&
                 !sketch::isAxisSelection(s);
        })) {
      add(m_registry->action(QStringLiteral("sketch.cut")));
      add(m_registry->action(QStringLiteral("sketch.copy")));
    }
    if (m_sketch->canPaste()) {
      add(m_registry->action(QStringLiteral("sketch.paste")));
    }
    for (const char* id : {"sketch.line", "sketch.rectangle", "sketch.circle", "sketch.dimension", "sketch.trim"}) {
      add(m_registry->action(QString::fromLatin1(id)));
    }
    // The sketch on another plane or face (mitcad#787).
    if (QAction* redefine = m_registry->action(QStringLiteral("sketch.redefine_plane"))) {
      menu.addSeparator();
      add(redefine);
    }
    // Linked projections that no longer follow their sources (mitcad#1303):
    // Project Again for those whose sources are there, Clean Up Stale
    // Projections for all of them.
    if (m_sketch->model().projectAgain) {
      add(m_registry->action(QStringLiteral("sketch.project_again")));
    }
    if (m_sketch->model().staleProjections) {
      add(m_registry->action(QStringLiteral("sketch.clean_up_stale_projections")));
    }
    menu.addSeparator();
    add(m_registry->action(QStringLiteral("sketch.finish")));
    add(m_registry->action(QStringLiteral("edit.undo")));
  } else {
    // A face, an edge or a vertex of a body or an occurrence selected whole
    // keeps that selection (mitcad#735): the menu is the body's or the
    // component's, with Move/Copy at its top.
    const auto inSelected = [this](const SelectionItem& picked) {
      if (picked.kind != SelectKind::Face && picked.kind != SelectKind::Edge && picked.kind != SelectKind::Vertex) {
        return false;
      }
      return std::any_of(m_selection.begin(), m_selection.end(), [&picked](const SelectionItem& whole) {
        if (whole.kind == SelectKind::Body) {
          return whole.owner == picked.owner && whole.occurrence == picked.occurrence;
        }
        return whole.kind == SelectKind::Component && !whole.occurrence.isEmpty() &&
               (picked.occurrence == whole.occurrence ||
                picked.occurrence.startsWith(whole.occurrence + QLatin1Char('/')));
      });
    };
    if (item.isValid() && !m_selection.contains(item) && !inSelected(item)) {
      // A right click on something not selected selects it.
      setSelection({item});
      selectMs = menuClock.restart();
    }
    fillModelContextMenu(menu, entries, item);
  }
  menu.addSeparator();
  add(m_registry->action(QStringLiteral("view.fit")));
  add(m_registry->action(QStringLiteral("tools.search")));
  // Until the menu shows (mitcad#981): the selection it made, the menu.
  const qint64 menuMs = menuClock.elapsed();
  qDebug().noquote() << QStringLiteral("Context menu timing: select %1 ms, menu %2 ms, total %3 ms")
                            .arg(selectMs)
                            .arg(menuMs)
                            .arg(selectMs + menuMs);
  qDebug().noquote() << QStringLiteral("Context menu: %1").arg(entries.join(QStringLiteral(" | ")));
  execMenu(&menu, globalPosition);
  // The print bed face shown while the menu was open (mitcad#731).
  if (m_printBedShown.isValid()) {
    m_printBedShown = SelectionItem();
    // Only the highlights change (mitcad#981); a command the menu started
    // has the view as its own.
    if (m_mode == Mode::Idle) {
      updateIdleHighlights();
    } else {
      updateIdleView();
    }
  }
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
  leaveDrawing(); // a sketch is drawn in the 3D view (mitcad#707)
  // The active component's design policy (mitcad#220): one that takes no
  // sketches says so; nothing is redirected into another component.
  if (const QString refused = refreshSketchSupports(); !refused.isEmpty()) {
    showError(tr("Create Sketch: %1").arg(refused));
    qInfo().noquote() << QStringLiteral("Create Sketch refused: %1").arg(refused);
    return;
  }
  // On the selected plane or planar face; else the user is asked for one.
  if (m_selection.size() == 1 && isSketchPlane(m_selection.first())) {
    if (const QString refused = sketchSupportRefusal(m_selection.first()); !refused.isEmpty()) {
      showError(tr("Create Sketch: %1").arg(refused));
      qInfo().noquote() << QStringLiteral("Create Sketch refused: %1").arg(refused);
      return;
    }
    createSketchOnPick(m_selection.first());
    return;
  }
  endAnnotationPick();
  m_mode = Mode::PickPlane;
  m_documentSession->cancelBackgroundEvaluation(QStringLiteral("Create Sketch")); // mitcad#1267
  setSelection({});
  // The origin planes in every design, whatever the Origin folder's light
  // bulb says, for the pick only (mitcad#844): behind the model's faces, so
  // that a face in front of a plane, or one a plane runs through, is the
  // pick, also another component's while one is active; a plane where no
  // face is still is. Put back when the pick ends (updateModeBanner).
  m_viewer->setPlanesBehind(true);
  m_viewer->setPlanesByDepth(true);
  inputActivated(kConstructionGeometry);
  updateIdleView();
  updateActions();
  // Faces and planes the policy leaves out are neither picked nor
  // highlighted: say whose they are.
  const QStringList& denied = m_sketchDeniedOwners;
  QString hint = tr("Create Sketch: select a plane or a planar face (Esc cancels).");
  if (!denied.isEmpty()) {
    hint += tr(" The design policy leaves out the faces and planes of %1.").arg(denied.join(QStringLiteral(", ")));
  }
  showHint(hint);
  qDebug() << "Create Sketch: select a plane";
  qDebug().noquote() << QStringLiteral("Create Sketch: origin planes shown");
  if (!denied.isEmpty()) {
    qDebug().noquote() << QStringLiteral("Create Sketch: policy leaves out %1").arg(denied.join(QStringLiteral(", ")));
  }
  logSketchPicks();
}

QString MainWindow::refreshSketchSupports() {
  m_sketchDenied.clear();
  m_sketchDeniedOwners.clear();
  const QJsonObject supports = queryObject({{QStringLiteral("query"), QStringLiteral("sketch_supports")}});
  const QJsonObject paths = supports.value(QStringLiteral("denied_paths")).toObject();
  for (auto it = paths.begin(); it != paths.end(); ++it) {
    m_sketchDenied.insert(it.key(), it.value().toString());
  }
  // The owners left out, for the hint: the root only when it has bodies of
  // its own (an assembly's root usually has none).
  for (const QJsonValue& value : supports.value(QStringLiteral("denied")).toArray()) {
    const QJsonObject owner = value.toObject();
    const DocumentSnapshot::Component* component =
        m_snapshot.component(owner.value(QStringLiteral("component")).toString());
    if (component != nullptr && (component->uid != QStringLiteral("C0") || !component->bodies.isEmpty())) {
      m_sketchDeniedOwners << owner.value(QStringLiteral("name")).toString();
    }
  }
  return supports.value(QStringLiteral("sketches")).toString();
}

QString MainWindow::sketchSupportRefusal(const SelectionItem& item) const {
  // The origin planes are the active component's own.
  if (item.kind == SelectKind::Plane && kOriginDatums.contains(item.owner)) {
    return QString();
  }
  const auto denied = m_sketchDenied.constFind(item.occurrence);
  return denied == m_sketchDenied.cend() ? QString() : denied.value();
}

void MainWindow::createSketch(const QJsonObject& fields) {
  if (m_sketchSpatial) {
    createSpatialSketch(fields); // the kind chosen: 3D (mitcad#1524)
    return;
  }
  QJsonObject request = fields;
  request.insert(QStringLiteral("cmd"), QStringLiteral("sketch.create"));
  // A new sketch opens with the design's +Z up (+Y on a horizontal plane):
  // the model gives it the own frame whose y is that direction, so the
  // screen's horizontal stays the sketch's x (mitcad#692, commands.md "An
  // upright sketch frame").
  request.insert(QStringLiteral("upright"), true);
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
  enterSketch(uid, false, fields.value(QStringLiteral("occurrence")).toString());
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
  // The component it went into: the active one (mitcad#220).
  const QJsonObject components = queryObject({{QStringLiteral("query"), QStringLiteral("components")}});
  const QString active = components.value(QStringLiteral("active")).toString();
  for (const QJsonValue& value : components.value(QStringLiteral("components")).toArray()) {
    if (value.toObject().value(QStringLiteral("uid")).toString() == active) {
      qDebug().noquote() << QStringLiteral("Sketch %1 is in %2")
                                .arg(m_sketch->model().name, value.toObject().value(QStringLiteral("name")).toString());
    }
  }
}

void MainWindow::createSketchOnPick(SelectionItem item) {
  // Another component's face or plane: activate that component first?
  // (mitcad#960, MainWindowActivate.cpp)
  if (askToActivatePicked(item, sketchSupportRefusal(item), QStringLiteral("sketch"))) {
    // What the newly active component's sketches may lie on.
    refreshSketchSupports();
  }
  createSketch(sketchPlaneFields(item));
}

QString MainWindow::featureError(const QString& uid) const {
  const QJsonObject timeline = query({{QStringLiteral("query"), QStringLiteral("timeline")}}).toObject();
  for (const QJsonValue& value : timeline.value(QStringLiteral("features")).toArray()) {
    const QJsonObject feature = value.toObject();
    if (feature.value(QStringLiteral("uid")).toString() == uid &&
        feature.value(QStringLiteral("status")).toString() == QStringLiteral("error")) {
      // In plain words, as its error notice gives it (mitcad#1612); the
      // model's `error` keeps the names for agents.
      for (const QJsonValue& notice : feature.value(QStringLiteral("notices")).toArray()) {
        if (notice.toObject().value(QStringLiteral("level")).toString() == QStringLiteral("error")) {
          return notice.toObject().value(QStringLiteral("message")).toString();
        }
      }
      return userText(feature.value(QStringLiteral("error")).toString());
    }
  }
  return QString();
}

void MainWindow::enterSketch(const QString& uid, bool existing, const QString& picked) {
  m_sketchExisting = existing;
  m_mcpSketchEdited = false;
  endAnnotationPick();
  m_mode = Mode::Sketch;
  m_documentSession->cancelBackgroundEvaluation(QStringLiteral("a sketch")); // mitcad#1267
  updateModeBanner(); // no plane is waited for (mitcad#474)
  m_selection.clear();
  inputActivated(SelectFilter()); // hides the origin planes
  // A component's sketch where its occurrence shows it (mitcad#505): the
  // sketch's frame, curves and profiles are placed by it once.
  m_sketchOccurrence = sketchEditOccurrence(uid, picked);
  m_sketch->enter(uid);
  // Every sketch edit starts with the filter's defaults (mitcad#217).
  m_sketchScope.reset();
  selectionScopeChanged(QStringLiteral("defaults"));
  m_sketchEnterDepth = undoDepth();
  m_sketchEnterRevision = documentRevision();
  m_sketchEscHinted = false;
  m_viewer->setGridPlane(toAx3(m_sketch->model().frame)); // the grid on the sketch plane
  m_ribbon->setSketchMode(true);
  // The palette's Create section chosen again as it opens (mitcad#1027):
  // the Sketch tab's pins first, then the tools started most.
  m_palette->chooseCreateTools(m_ribbon->pins().pins(QStringLiteral("SKETCH")));
  m_commandPages->setCurrentWidget(m_palettePage);
  // The view turns once the cards over it are where sketch mode puts them
  // (the Floating layout's command card shows the palette): the sketch's
  // origin is centred in the free area they leave (mitcad#738), and the
  // view does not move again under the user afterwards. A sketch on a face
  // keeps the face in view (mitcad#772).
  if (m_floating != nullptr) {
    m_floating->layoutNow();
  }
  m_viewer->lookAtSketchPlane(true, sketchFace(uid));
  {
    // For UI tests: the sketch's frame in the design and the direction the
    // view looks along (against the frame's normal).
    const geometry::Frame& frame = m_sketch->model().frame;
    const gp_Dir normal = frame.normal();
    const gp_Dir along = m_viewer->viewDirection();
    const auto xyz = [](double x, double y, double z) {
      const auto number = [](double value) {
        return QString::number(std::abs(value) < 5e-7 ? 0.0 : value, 'f', 6);
      };
      return QStringLiteral("(%1, %2, %3)").arg(number(x), number(y), number(z));
    };
    qDebug().noquote() << QStringLiteral("Sketch frame in the design%1: origin %2, normal %3, x axis %4")
                              .arg(m_sketchOccurrence.isEmpty() ? QString() : QStringLiteral(" (at ") + m_sketchOccurrence + QLatin1Char(')'),
                                   xyz(frame.origin.X(), frame.origin.Y(), frame.origin.Z()),
                                   xyz(normal.X(), normal.Y(), normal.Z()),
                                   xyz(frame.x_axis.X(), frame.x_axis.Y(), frame.x_axis.Z()));
    qDebug().noquote() << QStringLiteral("Sketch view along %1").arg(xyz(along.X(), along.Y(), along.Z()));
  }
  refreshScene();
  showHint(tr("Sketch: %1, %2, %3, %4; %5.")
               .arg(m_registry->shortcutHint(QStringLiteral("sketch.line")),
                    m_registry->shortcutHint(QStringLiteral("sketch.rectangle")),
                    m_registry->shortcutHint(QStringLiteral("sketch.circle")),
                    m_registry->shortcutHint(QStringLiteral("sketch.dimension")),
                    m_registry->shortcutHint(QStringLiteral("sketch.finish"))));
  TestSync::singleShot(300, this, [this] {
    m_ribbon->logLayout();
    m_palette->logLayout();
  });
}

void MainWindow::keyPressEvent(QKeyEvent* event) {
  // Even a read-only editor can ignore Delete and propagate it here. Its
  // focus still means text editing, never permission to delete the model.
  bool editing = false;
  for (QWidget* focus = QApplication::focusWidget(); focus; focus = focus->parentWidget()) {
    if (qobject_cast<QLineEdit*>(focus) || qobject_cast<QTextEdit*>(focus) ||
        qobject_cast<QPlainTextEdit*>(focus) || qobject_cast<QAbstractSpinBox*>(focus)) {
      editing = true;
      break;
    }
  }
  bool remove = event->key() == Qt::Key_Delete;
#ifdef Q_OS_MACOS
  remove = remove || event->key() == Qt::Key_Backspace;
#endif
  if (remove && !editing && event->modifiers() == Qt::NoModifier && startPageShown()) {
    // The Start page over the view (mitcad#1313): the design behind it is
    // out of sight, so the key deletes nothing.
    event->accept();
    return;
  }
  if (remove && !editing && event->modifiers() == Qt::NoModifier && m_mode == Mode::Idle && !modelBusy()) {
    m_controller->deleteSelection(m_selection);
    event->accept();
    return;
  }
  QMainWindow::keyPressEvent(event);
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
  if (features[position].toObject().value(QStringLiteral("type")).toString() == QStringLiteral("spatial_sketch")) {
    enterSketch3d(uid, true); // mitcad#1524
    qDebug().noquote() << QStringLiteral("Editing 3D sketch %1 (%2)").arg(uid, m_sketch3d->model().name);
    return;
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
  // A 3D sketch is seen from every side while it is edited: it is left with
  // the view the user had in it, also one that was a 2D sketch when it was
  // opened (mitcad#1592: that one's top view came back). A 2D sketch puts
  // back the view it was opened from.
  const bool spatial = m_sketch3d != nullptr && m_sketch3d->isActive();
  m_sketch->leave();
  if (m_sketch3d != nullptr) {
    m_sketch3d->leave(); // a 3D sketch's context (mitcad#1524)
  }
  m_sketchOccurrence.clear();
  m_mode = Mode::Idle;
  if (spatial) {
    m_viewer->keepCamera();
  } else {
    m_viewer->restoreCamera();
  }
  {
    // For UI tests: the view the sketch was left with.
    const gp_Dir along = m_viewer->viewDirection();
    const auto number = [](double value) { return QString::number(std::abs(value) < 5e-4 ? 0.0 : value, 'f', 3); };
    qDebug().noquote() << QStringLiteral("Sketch left with %1: the view looks along (%2, %3, %4)")
                              .arg(spatial ? QStringLiteral("the 3D sketch's view") : QStringLiteral("the view before it"),
                                   number(along.X()), number(along.Y()), number(along.Z()));
  }
  m_viewer->setGridPlane(gp_Ax3()); // back on XY
  m_ribbon->setSketchMode(false);
  m_commandPages->setCurrentWidget(m_idlePage);
  m_sketchExisting = false;
  m_sketchRestoreMarker = -1;
}

void MainWindow::showSketchLeft(const QString& uid, const QString& command) {
  // The view the sketch was opened from is back (leaveSketchMode). It can
  // show them elsewhere than in the free area that the command's card
  // leaves: zoomed in, or fitted to the larger free area before the card
  // took its place, under the ribbon, the card or the cube. They are fitted
  // into it then, as Fit would.
  Bnd_Box box;
  const auto add = [&box](const TopoDS_Shape& shape) {
    if (!shape.IsNull()) {
      BRepBndLib::Add(shape, box, false);
    }
  };
  const QString occurrence = placingOccurrence(uid);
  add(itemShape({SelectKind::Sketch, uid, QString(), QString(), occurrence}));
  try {
    const QJsonObject sketch =
        queryObject({{QStringLiteral("query"), QStringLiteral("sketch")}, {QStringLiteral("uid"), uid}});
    const QString body = sketch.value(QStringLiteral("plane")).toObject().value(QStringLiteral("body")).toString();
    if (!body.isEmpty()) {
      // As sketchFace places the face it lies on.
      const QJsonValue link = sketch.value(QStringLiteral("plane_link"));
      add(itemShape({SelectKind::Body, body, QString(), QString(),
                     link.isObject() ? link.toObject().value(QStringLiteral("source")).toString() : occurrence}));
    }
  } catch (const std::exception&) {
    // The sketch alone then.
  }
  if (box.IsVoid()) {
    return;
  }
  if (m_floating != nullptr) {
    m_floating->layoutNow(); // the command's card where it goes
  }
  m_viewer->showInFreeArea(box, QStringLiteral("%1 from the sketch").arg(command));
}

bool MainWindow::finishSketch() {
  if (m_mode != Mode::Sketch) {
    return true;
  }
  // Either kind of sketch (a 3D one: mitcad#1524) finishes alike.
  const QString uid = editingSketchUid();
  const bool spatial = m_sketch3d != nullptr && m_sketch3d->isActive();
  m_sketch->stopTool();
  if (spatial) {
    m_sketch3d->stopTool(); // what it has pending is not committed (mitcad#1525)
  }
  const QString name = editingSketchName();
  const bool empty = editingSketchEmpty();
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
                  // An agent's changes in it say so (mitcad#265).
                  {QStringLiteral("label"), m_mcpSketchEdited ? tr("Edit %1 (agent)").arg(name) : tr("Edit %1").arg(name)}});
    }
    if (!rolled && m_lastCancelled) {
      // The features after the sketch were not computed: the sketch stays
      // open, as it was.
      refreshScene();
      showHint(tr("Computation canceled; Finish Sketch computes the features after %1.").arg(name));
      qInfo().noquote() << QStringLiteral("Finish Sketch cancelled: %1 stays open").arg(name);
      return false;
    }
  }
  const bool edited = m_sketchExisting;
  leaveSketchMode();
  recompute();
  showHint(spatial ? tr("3D sketch finished; create another sketch, or edit it again from the timeline.")
                   : tr("Extrude (E) a profile, or create another sketch."));
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

qint64 MainWindow::documentRevision() const {
  return query({{QStringLiteral("query"), QStringLiteral("document")}})
      .toObject()
      .value(QStringLiteral("revision"))
      .toInteger(-1);
}

void MainWindow::escapeSketch() {
  if (m_mode != Mode::Sketch) {
    return;
  }
  // Either kind of sketch (a 3D one: mitcad#1524).
  const QString uid = editingSketchUid();
  const QString name = editingSketchName();
  // Undo back to where the sketch was entered gives that revision back, so
  // a change taken back counts as none.
  if (m_sketchEnterRevision >= 0 && documentRevision() == m_sketchEnterRevision) {
    const bool created = !m_sketchExisting;
    qDebug().noquote() << QStringLiteral("Esc: leaving %1, nothing changed in it").arg(name);
    // As Finish Sketch: a new sketch still empty is taken back with it
    // (its creation undone, no undo step left).
    if (finishSketch() && created && !sketchExists(uid)) {
      qDebug().noquote() << QStringLiteral("Esc: %1 taken back, it was new and empty").arg(name);
    }
    return;
  }
  if (m_sketchEscHinted) {
    qDebug().noquote() << QStringLiteral("Esc: %1 changed, it stays open").arg(name);
    return;
  }
  m_sketchEscHinted = true;
  const QList<QKeySequence> undoKeys = m_registry->effectiveShortcuts(QStringLiteral("edit.undo"));
  const QString undoKey =
      undoKeys.isEmpty() ? tr("Undo") : undoKeys.front().toString(QKeySequence::NativeText);
  const QString hint = tr("Finish Sketch (or %1 to undo) to leave the sketch.").arg(undoKey);
  showHint(hint);
  qDebug().noquote() << QStringLiteral("Esc: %1 changed, it stays open; hint: %2").arg(name, hint);
}

// ---------------------------------------------------------------------------
// Undo

void MainWindow::undo() {
  if (m_mode == Mode::Command || m_mode == Mode::PickPlane) {
    return;
  }
  endAnnotationPick(); // the annotation it waits for may change or go
  if (m_mode == Mode::Sketch) {
    // What a tool has half drawn goes first (a 3D sketch's too: no anchor
    // stays on a point the undo takes away, mitcad#1525).
    m_sketch->resetTool();
    if (m_sketch3d != nullptr) {
      m_sketch3d->clearPending();
    }
    if (m_sketchRestoreMarker >= 0 && undoDepth() <= m_sketchEnterDepth) {
      // Nothing done in the edited sketch: leave it as it was.
      finishSketch();
      return;
    }
  }
  // What Undo changes is logged like a command's change (MITCAD_LOG_VOLUMES).
  const QJsonObject before = bodyVolumes(*this);
  QJsonObject result;
  if (!runCommand({{QStringLiteral("cmd"), QStringLiteral("undo")}}, &result)) {
    return;
  }
  qDebug().noquote() << "Undo:" << result.value(QStringLiteral("label")).toString();
  logVolumeChanges(*this, before);
  if (m_mode == Mode::Sketch && !sketchExists(editingSketchUid())) {
    // The active sketch itself was taken back.
    leaveSketchMode();
  }
  // An undone conversion to a 3D sketch: the 2D editor again (mitcad#1544).
  followSketchKind();
  refreshScene();
}

void MainWindow::redo() {
  if (m_mode == Mode::Command || m_mode == Mode::PickPlane) {
    return;
  }
  endAnnotationPick();
  const QJsonObject before = bodyVolumes(*this);
  QJsonObject result;
  if (!runCommand({{QStringLiteral("cmd"), QStringLiteral("redo")}}, &result)) {
    return;
  }
  qDebug().noquote() << "Redo:" << result.value(QStringLiteral("label")).toString();
  logVolumeChanges(*this, before);
  // A redone conversion to a 3D sketch: the 3D editor again (mitcad#1544).
  followSketchKind();
  refreshScene();
}

// ---------------------------------------------------------------------------
// Design files

void MainWindow::closeEvent(QCloseEvent* event) {
  if (modelBusy()) {
    // The job stops first; the window closes when it is back (runJob).
    event->ignore();
    m_closePending = true;
    m_documentSession->cancelJob();
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
    if (m_windowManager != nullptr) m_windowManager->windowClosed(this);
  } else {
    if (m_windowManager != nullptr) m_windowManager->cancelQuit(this);
    event->ignore();
  }
}

bool MainWindow::deferForNativeCommand(std::function<void()> action) {
  // Qt feature previews also retain the document after returning to the
  // event loop. New/Open/Close must finish cancelling before maybeSave or
  // document replacement reads that owner.
  if (!m_session || (!m_nativeUi && !modelBusy())) {
    return false;
  }
  if (m_nativeCommandContinuation) {
    return true;
  }
  m_nativeCommandContinuation = connect(m_session, &CommandSession::cancelFinished, this,
      [this, action = std::move(action)](bool succeeded) {
        clearNativeCommandContinuation();
        if (succeeded) { QTimer::singleShot(0, this, action); }
      });
  m_session->cancel();
  return true;
}

void MainWindow::clearNativeCommandContinuation() {
  QObject::disconnect(m_nativeCommandContinuation);
  m_nativeCommandContinuation = {};
#ifdef MITCAD_MACOS_NATIVE_UI
  m_nativeClosingCommand = false;
#endif
}

void MainWindow::newDocument() {
  if (m_windowManager != nullptr) { m_windowManager->newDocument(this); return; }
  if (deferForNativeCommand([this] { newDocument(); })) {
    return;
  }
  if (!maybeSave()) {
    return;
  }
  if (!installEmptyDocument(QString())) {
    return;
  }
  qInfo() << "New document";
  showHint(tr("Create a sketch to start modeling. S searches for commands."));
}

void MainWindow::openDocument() {
  if (deferForNativeCommand([this] { openDocument(); })) {
    return;
  }
  // Replacing this design (no window manager, or Open in the current
  // window, mitcad#1172) asks about its changes before the file is chosen,
  // as Open always did; otherwise the design stays open and nothing is asked.
  const bool replacing = m_windowManager == nullptr || GeneralSettings::load().openInCurrentWindow;
  if (replacing && !maybeSave()) {
    return;
  }
  const QString path = dialogOpenFileName(this, tr("Open"), dialogFolder(DialogGroup::Open), openFilter());
  if (path.isEmpty()) {
    return;
  }
  rememberDialogFolder(DialogGroup::Open, path);
  QString error;
  // The import of the file chosen asks its choices (mitcad#507).
  m_askImportChoices = true;
  const bool opened = m_windowManager != nullptr ? m_windowManager->openFile(this, path, error, replacing) != nullptr
                                                 : openFile(path, error);
  m_askImportChoices = false;
  if (!opened && !error.isEmpty()) {
    const QString message = tr("Could not open %1:\n%2").arg(QDir::toNativeSeparators(path), error);
    qWarning().noquote() << message;
    sheetWarning(this, tr("MitCad"), message);
  }
}

bool MainWindow::loadDesign(const QString& path, QString& error) {
  const bool mcpOpen = m_mcpRequestActive;
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
  constexpr qint64 maxMcpDocumentBytes = 64 * 1024 * 1024;
  const QByteArray json = mcpOpen ? file.read(maxMcpDocumentBytes + 1) : file.readAll();
  if (file.error() != QFileDevice::NoError) {
    error = file.errorString();
    return false;
  }
  if (mcpOpen && json.size() > maxMcpDocumentBytes) {
    error = tr("The design is too large for MCP (maximum 64 MiB).");
    return false;
  }
  if (!json.isValidUtf8()) {
    error = tr("The file is not UTF-8 text.");
    return false;
  }
  // Linked components follow their files (relative to the design's).
  const QByteArray links = compactJson({{QStringLiteral("cmd"), QStringLiteral("update_links")},
                                        {QStringLiteral("base"), QFileInfo(path).absolutePath()}});
  QJsonArray messages;
  int upgraded = 0; // BUILD parts placed again (mitcad#1115)
  QString digest; // of the file as read, for autosave (P8)
  // A version 3 file's B-rep data comes from its project's store (P12a).
  const QByteArray location = QFileInfo(path).absoluteFilePath().toUtf8();
  // On the worker: parse the file, and for ordinary opens follow its links,
  // before installDocument computes it.
  const DocumentMaker read = [&, mcpOpen](ModelJob& job) {
    job.setStage(tr("Reading %1").arg(name));
    digest = fileDigest(json);
    ScopedTiming loading("open load");
    rust::Box<Document> document = load_document_at(rustStr(json), rustStr(location));
    if (mcpOpen) {
      // Audit the actual parsed bytes before installDocument can evaluate
      // them. MCP opens retain the embedded saved component bodies: a
      // linked child must not run unaudited font-file definitions either.
      document->validate_mcp_features();
    }
    loading.setDetail(QStringLiteral("%1 bytes").arg(json.size()));
    loading.finish();
    job.setStage(tr("Opening %1").arg(name));
    if (!mcpOpen) {
      const AttachedJob attached(*document, job);
      messages = parseObject(document->command(rustStr(links))).value(QStringLiteral("messages")).toArray();
    }
    // Slot connectors and their nuts and screws placed by the first BUILD
    // rule are placed by the current one (mitcad#1115): an undo step, none
    // when there are none. A failure leaves them as they are.
    try {
      const AttachedJob attached(*document, job);
      const QByteArray upgrade = compactJson({{QStringLiteral("cmd"), QStringLiteral("build_upgrade")}});
      upgraded = parseObject(document->command(rustStr(upgrade))).value(QStringLiteral("count")).toInt();
    } catch (const std::exception& e) {
      qWarning().noquote() << QStringLiteral("BUILD connectors not placed again: %1").arg(errorText(e));
    }
    // As opened (with refreshed links for ordinary opens): no change to
    // save; a recompute changes no revision.
    markSaved(*document);
    return document;
  };
  if (isVisible()) {
    showHint(tr("Opening %1…").arg(name));
  }
  if (!installDocument(name, QFileInfo(path).absoluteFilePath(), read, error)) {
    if (error.isEmpty()) {
      qInfo().noquote() << QStringLiteral("Opening %1 cancelled").arg(name);
      showHint(tr("Opening %1 canceled.").arg(name));
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
  // What the linked files read again did, in the message band
  // (mitcad#1416); an MCP open follows no link, so a changed linked file is
  // offered with Update instead.
  if (mcpOpen) {
    checkLinkedFiles();
  } else {
    showLinkMessages(messages);
  }
  if (upgraded > 0) {
    qInfo().noquote() << QStringLiteral("BUILD connectors placed again by the current rule: %1 part(s)").arg(upgraded);
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
  // Fields a newer MitCad wrote, not read (mitcad#1132): said at once.
  warnUnknownFields();
  return true;
}

bool MainWindow::save() {
  if (m_session && (m_nativeUi || modelBusy())) {
    showHint(tr("Finish the current command before saving."));
    return false;
  }
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
  if (m_session && (m_nativeUi || modelBusy())) {
    showHint(tr("Finish the current command before saving."));
    return false;
  }
  // In a project with versions: the project, folder and name chosen in the
  // project picker (mitcad#176); Another Folder... and a design in no
  // project: the file dialog.
  const std::pair<QString, QString> place = saveAsProjectPlace();
  if (!place.first.isEmpty()) {
    ProjectPickerOptions options;
    options.title = tr("Save in a Project");
    options.text = tr("The project and the folder for %1, and its name there:").arg(documentName());
    options.acceptText = tr("&Save");
    options.otherText = tr("&Another Folder…");
    options.root = place.first;
    options.folder = place.second;
    options.name = m_filePath.isEmpty() ? documentName() : QFileInfo(m_filePath).completeBaseName();
    // Its own file is no clash, unless the window is read-only (a copy).
    options.ownFile = windowReadOnly() ? QString() : m_filePath;
    offerNewFolder(options);
    const std::optional<ProjectPickerResult> picked = pickInProjects(this, options);
    if (!picked) {
      return false;
    }
    if (!picked->other) {
      if (!writeFile(picked->path)) {
        return false;
      }
      showDesignInPanel(picked->path);
      return true;
    }
  }
  QFileDialog dialog(this, tr("Save As"), dialogFolder(DialogGroup::Open),
                     tr("MitCad designs (*.%1)").arg(kFileSuffix));
  prepareFileDialog(&dialog);
  dialog.setAcceptMode(QFileDialog::AcceptSave);
  dialog.setDefaultSuffix(kFileSuffix);
  dialog.selectFile(m_filePath.isEmpty() ? documentName() + QLatin1Char('.') + kFileSuffix
                                         : documentName());
  if (execDialog(&dialog) != QDialog::Accepted || dialog.selectedFiles().isEmpty()) {
    return false;
  }
  rememberDialogFolder(DialogGroup::Open, dialog.selectedFiles().constFirst());
  return writeFile(dialog.selectedFiles().constFirst());
}

bool MainWindow::writeFile(const QString& path, const QString& description, VersionRecord* required) {
  if (m_nativeUi && m_session) {
    showHint(tr("Finish the current command before saving."));
    if (required != nullptr) {
      required->failure = tr("Finish the current command before saving.");
    }
    return false;
  }
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
  // A design a newer MitCad wrote (mitcad#1132): saving over the file it
  // was read from drops the fields this MitCad did not read, so it asks
  // first; Save As writes another file and leaves that one as it is.
  const std::optional<bool> dropFields = askDropUnknownFields(absolute);
  if (!dropFields) {
    return false; // canceled: no failure for a caller requiring a version
  }
  if (!*dropFields) {
    return saveAs();
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
      showHint(tr("Save canceled."));
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
        showHint(tr("Save canceled."));
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
  // store, else one file. A save that changes the file stamps the design's
  // statistics: Last Saved, by the user, with this MitCad (mitcad#1088).
  std::optional<rust::String> json;
  QString error;
  try {
    json = idleDocument().save_design_stamped(rustStr(path.toUtf8()), "auto");
  } catch (const std::exception& e) {
    error = mitcad::modelMessage(e.what());
  }
  if (!json) {
    const QString failure = tr("Could not save %1:\n%2").arg(QDir::toNativeSeparators(path), error);
    qWarning().noquote() << failure;
    sheetWarning(this, tr("MitCad"), failure);
    if (required != nullptr) {
      required->failure = failure;
    }
    return false;
  }
  VersionRecord status;
  if (project) {
    // The project's web viewer (mitcad#423): the design's page and the
    // index go into the same version.
    paths << webViewerOnSave(**project, absolute, author, paths.size() > 1);
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
  // The other tabs that link this design offer to take it (mitcad#1416).
  if (m_windowManager != nullptr) {
    m_windowManager->designSaved(this, absolute);
  }
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
  // Its preview for the Start tab and File › Open Recent (mitcad#1258).
  saveDesignPreview(m_filePath);
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
  // The UI tests count the questions (mitcad#1317: New Design asks once).
  qInfo().noquote() << QStringLiteral("Save question: %1").arg(documentName());
  QMessageBox box(QMessageBox::Warning, tr("MitCad"), tr("Save changes to %1?").arg(documentName()),
                  QMessageBox::Save | QMessageBox::Discard | QMessageBox::Cancel, this);
  box.setInformativeText(tr("Your changes will be lost if you don't save them."));
  // The same words on every platform, with keys for the keyboard.
  box.button(QMessageBox::Save)->setText(tr("&Save"));
  box.button(QMessageBox::Discard)->setText(tr("&Don't Save"));
  box.setDefaultButton(QMessageBox::Save);
  prepareModal(&box);
  switch (execDialog(&box)) {
  case QMessageBox::Save:
    return save();
  case QMessageBox::Discard:
    return true;
  default:
    return false;
  }
}

bool MainWindow::isModified() const {
  // While a job computes the model: as the title last showed it, without
  // reading the document (mitcad#888).
  if (modelBusy()) {
    return isWindowModified();
  }
  // The model compares its revision with the one marked saved (P8), so no
  // design file is made for it.
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
  // The Projects panel's selected folder of the current project (mitcad#145).
  if (const QString folder = projectsPanelFolder(); !folder.isEmpty()) {
    return folder;
  }
  if (m_project.isProject()) {
    return m_project.root;
  }
  return QStandardPaths::writableLocation(QStandardPaths::DocumentsLocation);
}

QString MainWindow::dialogFolder(DialogGroup group) const {
  return dialogStartFolder(group, fileDialogDirectory());
}

// Where an open's time went (mitcad#878), on one line: "Open part.mitcad:
// 2221 ms: read 48 ms, recompute 1653 ms (44 restored, 106 evaluated),
// results stored 0 ms, display meshes 148 ms (4 bodies from the result
// store, 0 meshed, 0 to store after the open), view 362 ms". The total
// runs from the open's start to the window refreshed; what the parts leave
// of it is the job's start, the root's activation and the previous
// design's edit lock. The display meshes' time holds their store write
// only when it is made in the job (MITCAD_DISPLAY_MESH_WRITER=0); else the
// writer's own line says how long storing them took after the open
// (mitcad#880). Bodies meshed while the recompute went on (mitcad#881)
// are counted apart, "..., 0 meshed, 20 meshed during the recompute in
// 9000 ms, 300 ms of it after the recompute, ...": the meshing's time
// beside the recompute is the first less the second; the second ran beside
// the open's own meshing, within the display meshes' time.
static void logOpenTimes(const QString& label, qint64 totalMs, qint64 readMs, double recomputeMs,
                         const QJsonObject& recomputed, const QJsonObject& persisted, const DisplayMeshing& meshing,
                         qint64 viewMs) {
  qInfo().noquote() << QStringLiteral("Open %1: %2 ms: read %3 ms, recompute %4 ms (%5 restored, %6 evaluated), "
                                      "results stored %7 ms, display meshes %8 ms (%9 bodies from the result "
                                      "store, %10 meshed, %13 meshed during the recompute in %14 ms, %15 ms of it "
                                      "after the recompute, %12 to store after the open), view %11 ms")
                           .arg(label)
                           .arg(totalMs)
                           .arg(readMs)
                           .arg(recomputeMs, 0, 'f', 0)
                           .arg(std::max(0, recomputed.value(QStringLiteral("restored")).toInt()))
                           .arg(recomputed.value(QStringLiteral("recomputed")).toInt())
                           .arg(persisted.value(QStringLiteral("ms")).toDouble(), 0, 'f', 0)
                           .arg(meshing.milliseconds)
                           .arg(static_cast<qulonglong>(meshing.restored))
                           .arg(static_cast<qulonglong>(meshing.bodies))
                           .arg(viewMs)
                           .arg(static_cast<qulonglong>(meshing.deferred))
                           .arg(static_cast<qulonglong>(meshing.during))
                           .arg(meshing.duringMilliseconds)
                           .arg(meshing.duringAfterMilliseconds);
}

bool MainWindow::installDocument(const QString& label, const QString& path, const DocumentMaker& make,
                                 QString& error, const QString& handedOver) {
  std::optional<rust::Box<Document>> made;
  QJsonObject recomputed;
  QJsonObject persisted;
  DisplayMeshing meshing;
  double milliseconds = 0.0;
  QString previousActive; // the component the file had active (mitcad#426)
  QString activateError;
  // A new design keeps the component it has active: New Design in a
  // project (mitcad#844) and an empty design (mitcad#898).
  const bool newDesign = m_openingNewDesign;
  // The result store (P7d): costly features come from it, and what was
  // costly to compute goes to it.
  const QByteArray store = resultStoreDirectory().toUtf8();
  const QByteArray buildId = resultStoreBuildId().toUtf8();
  const QByteArray storeLabel = label.toUtf8();
  const QByteArray handOff = handedOver.toUtf8();
  const double minMs = resultStoreMinMs();
  const auto memoryMb = static_cast<std::uint64_t>(m_documentSession->memoryCacheMegabytes());
  // Display meshes stored after the open (mitcad#880) count toward the
  // store's size too: the writer's thread starts the clean-up.
  DisplayMeshWrites* const writes = displayMeshWrites();
  if (writes != nullptr) {
    writes->setReporter([](const DisplayMeshWriteReport& report) {
      if (report.stored > 0) {
        collectResultStoreGarbage();
      }
    });
  }
  // Where an open's time goes (mitcad#878): the summary line below, and
  // the first frame after it.
  QElapsedTimer opening;
  opening.start();
  qint64 readMs = 0;
  try {
    const bool done = runJob(label, tr("Opening %1").arg(label), [&](ModelJob& job) {
      QElapsedTimer reading;
      reading.start();
      rust::Box<Document> document = make(job);
      readMs = reading.elapsed();
      // Every design opens with its root active (mitcad#426), whatever the
      // file stores: no undo step, no new revision, so not modified. A new
      // design keeps its Component1 active, as the untitled design at the
      // start does: one of a project (mitcad#844) and an empty one
      // (mitcad#898).
      try {
        if (!newDesign) {
          previousActive = parseObject(document->command(rustStr(compactJson(
                                           {{QStringLiteral("cmd"), QStringLiteral("activate_component")},
                                            {QStringLiteral("component"), QStringLiteral("C0")},
                                            {QStringLiteral("opening"), true}}))))
                               .value(QStringLiteral("previous"))
                               .toString();
        }
      } catch (const std::exception& e) {
        activateError = errorText(e);
      }
      // An import's results come from the store its process handed them
      // over in (mitcad#413); the user's store is the document's after.
      document->set_result_store(rustStr(handOff.isEmpty() ? store : handOff), rustStr(buildId),
                                 rustStr(storeLabel));
      document->set_memory_cache_budget(memoryMb);
      // The bodies no later feature changes are meshed while the
      // recompute goes on (mitcad#881); stopped, should the open fail.
      const std::unique_ptr<RecomputeMeshing> during = RecomputeMeshing::start(*document, job);
      {
        const AttachedJob attached(*document, job);
        QElapsedTimer timer;
        timer.start();
        recomputed = parseObject(document->command(rustStr(compactJson(
            {{QStringLiteral("cmd"), QStringLiteral("recompute")}}))));
        milliseconds = static_cast<double>(timer.nsecsElapsed()) / 1.0e6;
      }
      if (!handOff.isEmpty()) {
        document->set_result_store(rustStr(store), rustStr(buildId), rustStr(storeLabel));
      }
      if (!store.isEmpty() && recomputed.value(QStringLiteral("recomputed")).toInt() > 0) {
        job.setStage(tr("Saving results of %1").arg(label));
        persisted = parseObject(document->persist_results(minMs));
      }
      // The view's meshes, here rather than on the UI thread (mitcad#398):
      // those the result store keeps for the bodies' shapes taken from it,
      // the others made and kept there (mitcad#858) after the open, by
      // the writer's thread (mitcad#880); those meshed during the
      // recompute first take theirs (mitcad#881).
      meshing = meshOpenedForDisplay(*document, job, !store.isEmpty(), writes, label, during.get());
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
  logDisplayMeshing(meshing, label);
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
  clearDesignMessages(); // the old design's (mitcad#1174)
  m_measureHistory.clear(); // the old design's measurements (mitcad#648)
  m_measurementKept = false; // and a Keep of its (mitcad#649)
  m_mcpDocumentId = QUuid::createUuid().toString(QUuid::WithoutBraces);
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
  if (!activateError.isEmpty()) {
    qWarning().noquote() << QStringLiteral("The root could not be activated on opening: %1").arg(activateError);
  } else if (!previousActive.isEmpty() && previousActive != QStringLiteral("C0")) {
    qInfo().noquote() << QStringLiteral("Opened with the root active; %1 was active in the file (%2)")
                             .arg(previousActive, isModified() ? QStringLiteral("modified")
                                                               : QStringLiteral("unmodified"));
  }
  reportRecompute(recomputed, milliseconds);
  if (!handOff.isEmpty()) {
    qInfo().noquote() << QStringLiteral("Results handed over for %1: restored %2, evaluated %3")
                             .arg(label)
                             .arg(recomputed.value(QStringLiteral("restored")).toInt())
                             .arg(recomputed.value(QStringLiteral("recomputed")).toInt());
  }
  if (!store.isEmpty()) {
    reportStored(recomputed, persisted);
    // Display meshes stored count toward the store's size too (mitcad#858).
    if (meshing.stored > 0 && persisted.value(QStringLiteral("results")).toInt() == 0) {
      collectResultStoreGarbage();
    }
  }
  // Its browser rows as a design opens: folded when they would not fit
  // (mitcad#426).
  if (m_controller != nullptr) {
    m_controller->designOpened();
  }
  // Its first bodies are fitted into the view once (mitcad#467): a new
  // design's first body, not bodies shown again after all were hidden.
  m_viewer->expectFirstBodies();
  QElapsedTimer showing;
  showing.start();
  refreshScene();
  const qint64 viewMs = showing.elapsed();
  logOpenTimes(label, opening.elapsed(), readMs, milliseconds, recomputed, persisted, meshing, viewMs);
  m_viewer->logFirstFrame(QStringLiteral("Open %1").arg(label), opening);
  if (newDesign) {
    logNewDesignActive();
  }
  if (!m_keepView) {
    // A new design is empty: its first view frames the origin the same way
    // whatever the window's size and the design before (mitcad#1000).
    if (newDesign) {
      m_viewer->frameNewDesign();
    } else {
      m_viewer->fitAll();
    }
  }
  // Its edit lock (mitcad#89): taken, or the same design's kept.
  m_locks->designOpened(path, readOnlyOpen);
  // A design opened, recovered or made new shows in the 3D view, not
  // under the Start page (mitcad#1258), unless only its units changed.
  if (!m_keepStartPage) {
    leaveStartPage();
  }
  updateActions();
  return true;
}

void MainWindow::reportStored(const QJsonObject& recomputed, const QJsonObject& persisted) {
  const int results = persisted.value(QStringLiteral("results")).toInt();
  // Restored results whose bodies' shapes were never unpacked: states a
  // later result replaced (mitcad#858).
  const int restored = recomputed.value(QStringLiteral("restored")).toInt();
  const int packed = recomputed.value(QStringLiteral("restored_packed")).toInt();
  qInfo().noquote() << QStringLiteral("Result store: restored %1, evaluated %2; stored %3 (%4 bytes) in %5 ms; "
                                      "unpacked %6, skipped %7")
                           .arg(restored)
                           .arg(recomputed.value(QStringLiteral("recomputed")).toInt())
                           .arg(results)
                           .arg(persisted.value(QStringLiteral("bytes")).toInteger())
                           .arg(persisted.value(QStringLiteral("ms")).toDouble(), 0, 'f', 1)
                           .arg(restored - packed)
                           .arg(packed);
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
  DocumentSession::refreshMemoryCacheBudgets();
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

bool MainWindow::installEmptyDocument(const QString& name, QString* error) {
  QString reason;
  // A new, empty design (New, Close, an empty project, the open design
  // deleted) starts with its Component1 active, as the untitled design at
  // the start does (mitcad#898), not with the root as a design read from a
  // file (mitcad#426).
  const bool wasOpeningNewDesign = m_openingNewDesign;
  m_openingNewDesign = true;
  // In the default units for new designs (Preferences > General,
  // mitcad#1036), read now: a change of the preference changes no open
  // design, only the next new one.
  const QString units = GeneralSettings::load().newDesignUnits;
  const bool installed = installDocument(
      name.isEmpty() ? tr("Untitled") : name, QString(),
      [units](ModelJob&) { return new_design_document_in(rustStr(units.toUtf8())); }, reason);
  m_openingNewDesign = wasOpeningNewDesign;
  if (!installed) {
    if (error != nullptr) {
      *error = reason;
    } else if (!reason.isEmpty()) {
      sheetWarning(this, tr("MitCad"), tr("Could not create a new design:\n%1").arg(reason));
    }
    return false;
  }
  m_documentName = name;
  updateWindowTitle();
  qInfo().noquote() << QStringLiteral("New design in %1").arg(units);
  return true;
}

void MainWindow::logNewDesignActive() {
  const DocumentSnapshot::Component* active = m_snapshot.component(m_snapshot.activeComponent);
  qInfo().noquote() << QStringLiteral("Opened the new design with %1 active")
                           .arg(active != nullptr ? active->name : m_snapshot.activeComponent);
}

// Leaves any command or sketch, so the next document starts idle.
void MainWindow::resetCommandState() {
  m_commandFinished = nullptr; // the document it would follow up on goes
  if (m_session) {
    m_session->discard(); // nothing to restore in a document that goes
  }
  m_commandSketches = CommandSketches(); // the old document's (mitcad#546)
  if (m_mode == Mode::Sketch) {
    leaveSketchMode();
  }
  if (m_mode == Mode::PickPlane) {
    m_mode = Mode::Idle;
    updateModeBanner(); // the planes' pick order first (mitcad#844)
    inputActivated(SelectFilter());
  }
  // An annotation's pick and highlight are the old document's.
  endAnnotationPick();
  dropAnnotationHighlight();
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
  // With the version (mitcad#1209), so the taskbar and the window switchers
  // show it.
  setWindowTitle(QStringLiteral("%1[*] - MitCad %2").arg(documentName(), QApplication::applicationVersion()));
  const bool modified = isModified();
  setWindowModified(modified);
  // Save follows the mark (its enabled() reads it), also after a save that
  // no other update of the actions follows (mitcad#1090).
  if (const CommandDef* saveDef = m_registry->find(QStringLiteral("file.save"))) {
    if (QAction* saveAction = m_registry->action(saveDef->id)) {
      saveAction->setEnabled(isAvailable(*saveDef));
    }
  }
  // The first change since opening: a newer version on the remote is
  // looked for (P12 remote).
  if (modified && !m_changeNoted && !m_filePath.isEmpty() && m_remote != nullptr) {
    m_changeNoted = true;
    m_remote->firstChange();
  }
  // The chat's conversation follows the design (mitcad#947).
  noteChatDocument();
}

} // namespace mitcad
