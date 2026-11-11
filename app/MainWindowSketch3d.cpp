// SPDX-License-Identifier: MIT
// 3D sketches in the main window (mitcad#1524): Create Sketch's choice of
// the kind before the plane (2D, the default, or 3D), the 3D sketch's
// creation on an origin plane, construction plane or planar face of the
// active component, its editing context in sketch mode (sketch/
// Sketch3dController.hpp, the 3D Sketch Palette), the planar tools and the
// profile commands disabled with the reason, Delete, and an agent's
// commands in it. Finish Sketch, undo and a rolled-back timeline are
// sketch mode's own (MainWindow.cpp), for either kind.

#include "MainWindow.hpp"

#include <exception>
#include <memory>
#include <utility>

#include <QAbstractSpinBox>
#include <QAction>
#include <QApplication>
#include <QButtonGroup>
#include <QComboBox>
#include <QHBoxLayout>
#include <QJsonArray>
#include <QJsonDocument>
#include <QKeyEvent>
#include <QKeySequence>
#include <QLabel>
#include <QLineEdit>
#include <QPlainTextEdit>
#include <QScrollArea>
#include <QStackedWidget>
#include <QTextEdit>
#include <QToolButton>
#include <QVBoxLayout>
#include <QtLogging>

#include <BRepAdaptor_Curve.hxx>
#include <BRep_Tool.hxx>
#include <Graphic3d_Camera.hxx>
#include <Standard_Failure.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <gp_Ax3.hxx>

#include "framework/CommandRegistry.hpp"
#include "framework/FloatingLayout.hpp"
#include "framework/GlassCard.hpp"
#include "framework/MessageBand.hpp"
#include "framework/ModelShapes.hpp"
#include "framework/Ribbon.hpp"
#include "framework/TestSync.hpp"
#include "sketch/Sketch3dController.hpp"
#include "sketch/Sketch3dModel.hpp"
#include "sketch/Sketch3dPalette.hpp"
#include "sketch/SketchController.hpp"
#include "sketch/SketchModel.hpp"
#include "sketch/SketchPalette.hpp"
#include "sketch/SketchTool.hpp"

namespace mitcad {
namespace {

const QString kRootComponent = QStringLiteral("C0");

// The commands a 3D sketch keeps of the SKETCH tab's: Finish Sketch,
// Delete of its own selection (deleteSketch3dSelection), the drawing
// tools that draw in space (mitcad#1525: Point, Line, 3-Point, Center
// Point and Tangent Arc, Fit Point and Control Point Spline) and Sketch
// Fillet (mitcad#1548), which run the 3D sketch's own tools, and the
// constraints and Sketch Dimension that
// apply to its selection (mitcad#1526: sketch3dEditCommand).
// The SKETCH tab's commands that edit a 3D sketch's selection
// (mitcad#1526: MainWindow::runSketch3dEdit).
bool editCommand(const QString& id) {
  return id == QStringLiteral("sketch.constraint.coincident") || id == QStringLiteral("sketch.constraint.tangent") ||
         id == QStringLiteral("sketch.constraint.equal") || id == QStringLiteral("sketch.constraint.fix") ||
         id == QStringLiteral("sketch.constraint.midpoint") || id == QStringLiteral("sketch.dimension");
}

// Copy, Cut and Paste of a 3D sketch's geometry (mitcad#1661: the
// controller's), and the manipulator's keys.
bool clipboardCommand(const QString& id) {
  return id == QStringLiteral("sketch.copy") || id == QStringLiteral("sketch.cut") ||
         id == QStringLiteral("sketch.paste");
}

bool placementKeyCommand(const QString& id) {
  using sketch::Sketch3dController;
  return id == QLatin1String(Sketch3dController::kNextPlaneCommand) ||
         id == QLatin1String(Sketch3dController::kPreviousPlaneCommand) ||
         id == QLatin1String(Sketch3dController::kNormalCommand);
}

bool keptIn3dSketch(const QString& id) {
  return id == QStringLiteral("sketch.finish") || id == QStringLiteral("sketch.delete") ||
         sketch::Sketch3dController::toolForCommand(id) != sketch::Sketch3dTool::None || editCommand(id) ||
         clipboardCommand(id) || placementKeyCommand(id);
}

// A widget that takes typing: a field keeps Tab, Shift+Tab and the arrows
// (mitcad#1661).
bool takesTyping(const QWidget* widget) {
  if (widget == nullptr) {
    return false;
  }
  if (const auto* combo = qobject_cast<const QComboBox*>(widget)) {
    return combo->isEditable();
  }
  return qobject_cast<const QLineEdit*>(widget) != nullptr ||
         qobject_cast<const QAbstractSpinBox*>(widget) != nullptr ||
         qobject_cast<const QTextEdit*>(widget) != nullptr || qobject_cast<const QPlainTextEdit*>(widget) != nullptr;
}

} // namespace

void MainWindow::setUpSketch3d() {
  m_sketch3d = new sketch::Sketch3dController(*this, *m_viewer, this);
  m_palette3d = new sketch::Sketch3dPalette(*m_sketch3d, *m_registry);
  if (m_nativeUi) {
    m_palette3dPage = m_palette3d;
  } else {
    // It scrolls when it is longer than its place, as the Sketch Palette
    // does (mitcad#412); transparent in a floating card.
    auto* scroll = new QScrollArea;
    scroll->setObjectName(QStringLiteral("sketch3dPaletteScroll"));
    scroll->setWidgetResizable(true);
    scroll->setFrameShape(QFrame::NoFrame);
    scroll->setHorizontalScrollBarPolicy(Qt::ScrollBarAlwaysOff);
    scroll->setWidget(m_palette3d);
    if (m_floating != nullptr) {
      scroll->setAutoFillBackground(false);
      scroll->viewport()->setAutoFillBackground(false);
      m_palette3d->setAutoFillBackground(false);
    }
    m_palette3dPage = scroll;
  }
  m_commandPages->addWidget(m_palette3dPage);
  // The 2D Sketch Palette's 3D Sketch box (mitcad#1544).
  connect(m_palette, &sketch::SketchPalette::convertTo3dRequested, this, [this] { convertOpenSketchTo3d(); });
  // The 3D Sketch Palette's box unchecked (mitcad#1550).
  connect(m_palette3d, &sketch::Sketch3dPalette::convertTo2dRequested, this, [this] { convertOpenSketchTo2d(); });
  // What the drawing tools' snaps need of the design (mitcad#1525); the
  // names the palette's words give sources, never an id, and the places the
  // row under the pointer paints, as a notice's (mitcad#1652).
  const auto componentOf = [this](const QString& occurrence) {
    return m_snapshot.componentName(occurrence.isEmpty() ? kRootComponent
                                                         : m_snapshot.occurrenceComponent(occurrence));
  };
  sketch::SourceNames names;
  names.feature = [this](const QString& uid) {
    const DocumentSnapshot::Feature* feature = m_snapshot.feature(uid);
    return feature != nullptr ? feature->name : QString();
  };
  names.component = [this](const QString& occurrence) {
    const QString uid = m_snapshot.occurrenceComponent(occurrence);
    const DocumentSnapshot::Component* component = uid.isEmpty() ? nullptr : m_snapshot.component(uid);
    return component != nullptr ? component->name : QString();
  };
  m_sketch3d->setHooks({[this](const QString& occurrence) { return placementOf(occurrence).Transformation(); },
                        [this](const QString& uid) { return featureName(uid); }, componentOf, names,
                        [this](const QString& source, const QJsonArray& places) {
                          hoverNoticePlaces(source, places, {});
                        }});
  // The manipulator's keys as Keyboard Shortcuts has them now, for the hint
  // line and the palette's tooltips (mitcad#1661).
  m_sketch3d->setKeyText([this](const QString& id) {
    const QList<QKeySequence> keys = m_registry->effectiveShortcuts(id);
    return keys.isEmpty() ? QString() : keys.front().toString(QKeySequence::NativeText);
  });
  // A tool takes the view's clicks and snaps to more than the sketch's own
  // geometry: the pick filter and the commands follow.
  connect(m_sketch3d, &sketch::Sketch3dController::toolChanged, this, [this] {
    if (m_mode == Mode::Sketch) {
      updateIdleView();
      updateActions();
      if (m_floating != nullptr) {
        m_floating->layoutNow(); // the card with the tool's options
      }
    }
  });
  connect(m_sketch3d, &sketch::Sketch3dController::toolStateChanged, this, [this, shown = QString()]() mutable {
    // The tool's next step on the hint line, when it changes.
    const QString hint = m_sketch3d->toolActive() ? m_sketch3d->toolHint() : QString();
    if (hint != shown) {
      shown = hint;
      if (!hint.isEmpty()) {
        showHint(hint);
      }
    }
  });
  // What a click would pick: one of the sketch's points or curves is
  // described (a reference curve with its source).
  connect(
      m_viewer, &OcctViewer::hovered, this,
      [this](const SelectionItem& item) {
        if (m_mode == Mode::Sketch && m_sketch3d->isActive()) {
          m_sketch3d->setHover(item);
        }
      },
      Qt::QueuedConnection);
  connect(m_viewer, &OcctViewer::cameraRested, this, [this] {
    if (m_sketch3d->isActive()) {
      logSketch3dEntities();
    }
  });

  // Create Sketch's kind, on the command card while it waits for the
  // plane: 2D or 3D.
  QWidget* page = m_idlePage;
  if (auto* scroll = qobject_cast<QScrollArea*>(page)) {
    page = scroll->widget();
  }
  auto* column = page != nullptr ? qobject_cast<QVBoxLayout*>(page->layout()) : nullptr;
  if (column == nullptr) {
    return;
  }
  m_sketchKindRow = new QWidget;
  m_sketchKindRow->setObjectName(QStringLiteral("sketchKindRow"));
  auto* row = new QHBoxLayout(m_sketchKindRow);
  row->setContentsMargins(0, 0, 0, 0);
  auto* caption = new QLabel(tr("Sketch type"));
  row->addWidget(caption);
  const auto choice = [row](const QString& text, const QString& name, const QString& tip) {
    auto* button = new QToolButton;
    button->setText(text);
    button->setObjectName(name);
    button->setToolTip(tip);
    button->setCheckable(true);
    button->setFocusPolicy(Qt::NoFocus);
    button->setToolButtonStyle(Qt::ToolButtonTextOnly);
    row->addWidget(button);
    return button;
  };
  m_sketchKind2d =
      choice(tr("2D"), QStringLiteral("sketchKind2d"), tr("A sketch in its plane, whose loops are profiles"));
  m_sketchKind3d =
      choice(tr("3D"), QStringLiteral("sketchKind3d"), tr("A sketch whose points and curves can leave its plane"));
  row->addStretch(1);
  auto* group = new QButtonGroup(m_sketchKindRow);
  group->setExclusive(true);
  group->addButton(m_sketchKind2d);
  group->addButton(m_sketchKind3d);
  m_sketchKind2d->setChecked(true);
  connect(m_sketchKind2d, &QToolButton::clicked, this, [this] { setSketchKind(false); });
  connect(m_sketchKind3d, &QToolButton::clicked, this, [this] { setSketchKind(true); });
  m_sketchKindRow->hide();
  column->insertWidget(0, m_sketchKindRow);
}

void MainWindow::setSketchKind(bool spatial) {
  m_sketchSpatial = spatial;
  if (m_sketchKind2d != nullptr) {
    m_sketchKind2d->setChecked(!spatial);
    m_sketchKind3d->setChecked(spatial);
  }
  qDebug().noquote() << QStringLiteral("Create Sketch kind: %1").arg(spatial ? QStringLiteral("3D") : QStringLiteral("2D"));
  if (m_mode == Mode::PickPlane) {
    updateModeBanner(); // the banner names the kind
    showHint(pickPlaneText());
  }
}

void MainWindow::showSketchKindRow(bool shown) {
  if (m_sketchKindRow == nullptr || m_sketchKindRow->isHidden() != shown) {
    return;
  }
  m_sketchKindRow->setVisible(shown);
  updateCommandCard();
  if (!shown) {
    return;
  }
  // Placed at once: the places offered for the plane pick, logged right
  // after, keep clear of the card (OcctViewer::isCovered).
  if (m_floating != nullptr) {
    m_floating->layoutNow();
  }
  // Where the choices are, for UI tests, once the card is laid out.
  TestSync::singleShot(200, this, [this] {
    if (m_sketchKindRow == nullptr || !m_sketchKindRow->isVisible()) {
      return;
    }
    for (const QToolButton* button : {m_sketchKind2d, m_sketchKind3d}) {
      const QPoint at = GlassCard::mapToHost(button, button->rect().center());
      qDebug().noquote() << QStringLiteral("Create Sketch kind %1 at %2,%3").arg(button->text()).arg(at.x()).arg(at.y());
    }
  });
}

void MainWindow::createSpatialSketch(const QJsonObject& fields) {
  // A 3D sketch's frame follows a plane or a planar face of the active
  // component: another component's is not linked into it.
  if (fields.contains(QStringLiteral("occurrence"))) {
    const QString occurrence = fields.value(QStringLiteral("occurrence")).toString();
    const QString component = occurrence.isEmpty() ? kRootComponent : m_snapshot.occurrenceComponent(occurrence);
    const QString active = m_snapshot.activeComponent.isEmpty() ? kRootComponent : m_snapshot.activeComponent;
    if (component != active) {
      const QString refused = tr("a 3D sketch lies on a plane or a planar face of the active component; "
                                 "activate %1 to sketch on it")
                                  .arg(m_snapshot.componentName(component));
      showError(tr("Create Sketch: %1").arg(refused));
      qInfo().noquote() << QStringLiteral("Create Sketch refused: %1").arg(refused);
      return;
    }
  }
  const QJsonObject request{{QStringLiteral("cmd"), QStringLiteral("sketch3d.create")},
                            {QStringLiteral("support"), fields.value(QStringLiteral("plane"))}};
  const int depth = undoDepth();
  QJsonObject result;
  if (!runCommand(request, &result)) {
    return; // the reason is shown; a plane can still be picked
  }
  const QString uid = result.value(QStringLiteral("uid")).toString();
  // A sketch whose support cannot be found where it is added is taken
  // back with the reason, as a planar one (mitcad#99).
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
  enterSketch3d(uid, false, fields.value(QStringLiteral("occurrence")).toString());
  const QJsonValue support = fields.value(QStringLiteral("plane"));
  const QString on = support.isString()
                         ? support.toString()
                         : QString::fromUtf8(QJsonDocument(support.toObject()).toJson(QJsonDocument::Compact));
  const gp_Pnt origin = m_sketch3d->designOrigin();
  qDebug().noquote() << QStringLiteral("3D sketch started on %1, origin (%2, %3, %4)")
                            .arg(on)
                            .arg(origin.X())
                            .arg(origin.Y())
                            .arg(origin.Z());
}

void MainWindow::enterSketch3d(const QString& uid, bool existing, const QString& picked) {
  m_sketchExisting = existing;
  m_mcpSketchEdited = false;
  endAnnotationPick();
  m_mode = Mode::Sketch;
  m_documentSession->cancelBackgroundEvaluation(QStringLiteral("a sketch")); // mitcad#1267
  m_selection.clear();
  inputActivated(SelectFilter()); // hides the origin planes
  // A component's sketch where its occurrence shows it (mitcad#505).
  m_sketchOccurrence = sketchEditOccurrence(uid, picked);
  m_loggedSketch3dEntities.clear();
  m_sketch3d->enter(uid);
  m_sketchEnterDepth = undoDepth();
  m_sketchEnterRevision = documentRevision(); // Esc leaving it unchanged (mitcad#1623)
  m_sketchEscHinted = false;
  // The grid on the sketch's own XY plane; the camera stays where it is:
  // the sketch is in space, orbit, pan and zoom see it from every side.
  if (m_sketch3d->model().hasFrame) {
    try {
      const gp_Pnt origin = m_sketch3d->designOrigin();
      const gp_Pnt x = m_sketch3d->designAlong({1.0, 0.0, 0.0}, 1.0);
      const gp_Pnt z = m_sketch3d->designAlong({0.0, 0.0, 1.0}, 1.0);
      m_viewer->setGridPlane(gp_Ax3(origin, gp_Dir(z.XYZ() - origin.XYZ()), gp_Dir(x.XYZ() - origin.XYZ())));
    } catch (const Standard_Failure&) {
      m_viewer->setGridPlane(gp_Ax3());
    }
  }
  m_ribbon->setSketchMode(true);
  m_commandPages->setCurrentWidget(m_palette3dPage);
  m_palette3d->opened();
  updateModeBanner(); // the 3D Sketch banner
  if (m_floating != nullptr) {
    m_floating->layoutNow();
  }
  refreshScene();
  showHint(tr("3D Sketch: its points and curves are in space; %1 returns to the model.")
               .arg(m_registry->shortcutHint(QStringLiteral("sketch.finish"))));
  // Of the 2D sketch tools, what is disabled, for UI tests.
  int disabled = 0;
  for (const std::unique_ptr<CommandDef>& def : m_registry->commands()) {
    if (def->mode == CommandDef::Mode::Sketch && !sketch3dRefusal(*def).isEmpty()) {
      ++disabled;
    }
  }
  QStringList named;
  for (const char* id : {"sketch.slot", "solid.extrude"}) {
    if (const CommandDef* def = m_registry->find(QString::fromLatin1(id))) {
      named << QStringLiteral("%1: %2").arg(def->id, sketch3dRefusal(*def));
    }
  }
  qDebug().noquote() << QStringLiteral("3D sketch: %1 2D sketch commands disabled; %2")
                            .arg(disabled)
                            .arg(named.join(QStringLiteral("; ")));
  TestSync::singleShot(300, this, [this] {
    if (m_sketch3d->isActive()) {
      m_palette3d->logLayout();
      logSketch3dEntities();
    }
  });
}

bool MainWindow::isSpatialSketch(const QString& uid) const {
  if (const DocumentSnapshot::Feature* feature = m_snapshot.feature(uid)) {
    return feature->isSpatialSketch();
  }
  // Just created: not in the snapshot yet.
  return m_sketch3d != nullptr && m_sketch3d->isActive() && m_sketch3d->uid() == uid;
}

QString MainWindow::editingSketchUid() const {
  if (m_sketch3d != nullptr && m_sketch3d->isActive()) {
    return m_sketch3d->uid();
  }
  return m_sketch != nullptr && m_sketch->isActive() ? m_sketch->uid() : QString();
}

QString MainWindow::editingSketchName() const {
  if (m_sketch3d != nullptr && m_sketch3d->isActive()) {
    return m_sketch3d->model().name;
  }
  return m_sketch != nullptr ? m_sketch->model().name : QString();
}

bool MainWindow::editingSketchEmpty() const {
  if (m_sketch3d != nullptr && m_sketch3d->isActive()) {
    return m_sketch3d->model().empty();
  }
  const sketch::SketchModel& model = m_sketch->model();
  return model.points.empty() && model.curves.empty() && model.texts.empty();
}

std::vector<SketchEntityDisplay> MainWindow::sketchDisplay(const QString& uid) const {
  if (isSpatialSketch(uid)) {
    return spatialSketchEntities(
        queryObject({{QStringLiteral("query"), QStringLiteral("sketch3d")}, {QStringLiteral("uid"), uid}}));
  }
  return sketchEntities(queryObject({{QStringLiteral("query"), QStringLiteral("sketch")}, {QStringLiteral("uid"), uid}}));
}

QString MainWindow::sketch3dRefusal(const CommandDef& def) const {
  if (m_sketch3d == nullptr || !m_sketch3d->isActive()) {
    return QString();
  }
  if (def.mode == CommandDef::Mode::Sketch) {
    if (keptIn3dSketch(def.id)) {
      return QString();
    }
    return tr("a 2D sketch tool does not apply to a 3D sketch");
  }
  // Extrude and Revolve from a 3D sketch take the profiles its closed loops
  // in the sketch plane bound (mitcad#1550), as from a 2D sketch.
  return QString();
}

bool MainWindow::mcpSketch3dStep(const QJsonObject& request, QJsonObject* answer, QString& reason) {
  const QString kind = request.value(QStringLiteral("cmd")).toString();
  const QString uid = m_sketch3d->uid();
  const bool own =
      (kind.startsWith(QStringLiteral("sketch3d.")) && request.value(QStringLiteral("sketch")).toString() == uid) ||
      (kind == QStringLiteral("edit_feature") && request.value(QStringLiteral("uid")).toString() == uid);
  if (!own) {
    reason = QStringLiteral("%1 (%2) is being edited: only its own 3D sketch commands (with \"sketch\": \"%2\"), "
                            "edit_feature of it, undo and redo run now. Finish it (ui_command with id "
                            "sketch.finish) for other commands.")
                 .arg(m_sketch3d->model().name, uid);
    return false;
  }
  // Turned back into 2D (mitcad#1550), as its palette's 3D Sketch box
  // does: the 2D editor takes it.
  if (kind == QStringLiteral("sketch3d.convert_to_2d")) {
    const int depth = undoDepth();
    convertOpenSketchTo2d();
    if (undoDepth() == depth) {
      reason = lastError().isEmpty() ? QStringLiteral("The conversion was refused.") : lastError();
      return false;
    }
    m_mcpSketchEdited = true;
    *answer = queryObject({{QStringLiteral("query"), QStringLiteral("sketch")}, {QStringLiteral("uid"), uid}});
    return true;
  }
  if (!runModelCommand(request, answer)) {
    reason = lastError().isEmpty() ? QStringLiteral("The 3D sketch command was refused.") : lastError();
    return false;
  }
  return true;
}

void MainWindow::deleteSketch3dSelection() {
  // A point that curves still use is not deleted at once: the palette
  // names what would go with it and offers Delete with Dependents
  // (mitcad#1526); its relations and dimensions go with it.
  if (m_sketch3d->deleteSelection()) {
    setSelection({});
  }
}

bool MainWindow::runSketch3dEdit(const QString& id) {
  if (!editCommand(id) || m_mode != Mode::Sketch || m_sketch3d == nullptr || !m_sketch3d->isActive()) {
    return false;
  }
  // A queued pick reaches the selection first, as for the 2D Dimension
  // (mitcad#660).
  deliverPendingPicks();
  m_messages->commandStarted();
  if (id == QStringLiteral("sketch.dimension")) {
    m_sketch3d->applyAutoDimension();
  } else if (id == QStringLiteral("sketch.constraint.fix")) {
    m_sketch3d->toggleFix();
  } else if (id == QStringLiteral("sketch.constraint.coincident")) {
    m_sketch3d->applyRelation(sketch::Relation3::Coincident);
  } else if (id == QStringLiteral("sketch.constraint.tangent")) {
    m_sketch3d->applyRelation(sketch::Relation3::Tangent);
  } else if (id == QStringLiteral("sketch.constraint.equal")) {
    m_sketch3d->applyRelation(sketch::Relation3::Equal);
  } else if (id == QStringLiteral("sketch.constraint.midpoint")) {
    m_sketch3d->applyRelation(sketch::Relation3::Midpoint);
  }
  return true;
}

bool MainWindow::runSketch3dClipboard(const QString& id) {
  if (!clipboardCommand(id) || m_mode != Mode::Sketch || m_sketch3d == nullptr || !m_sketch3d->isActive()) {
    return false;
  }
  // A queued pick reaches the selection first, as for Delete.
  deliverPendingPicks();
  if (id == QStringLiteral("sketch.copy")) {
    m_sketch3d->copySelection();
  } else if (id == QStringLiteral("sketch.cut")) {
    m_sketch3d->cutSelection();
  } else {
    m_messages->commandStarted();
    m_sketch3d->paste();
  }
  return true;
}

void MainWindow::registerSketch3dKeys() {
  using sketch::Sketch3dController;
  const auto add = [this](const char* id, const QString& name, const char* icon, const QKeySequence& key,
                          const QList<QKeySequence>& alternates, const QString& tooltip) {
    CommandDef def;
    def.id = QString::fromLatin1(id);
    def.name = name;
    def.icon = QString::fromLatin1(icon);
    def.kind = CommandDef::Kind::Action;
    def.mode = CommandDef::Mode::Sketch;
    def.tab.clear(); // no tab: its key, Keyboard Shortcuts and the command search
    def.shortcut = key;
    // Its keys live in the 3D drawing tools' context, which wins over the
    // others' while a tool is active (Up is Turn View Up's elsewhere).
    def.shortcutContexts = Sketch3dToolShortcutContext;
    def.alternates = alternates;
    def.tooltip = tooltip;
    def.keywords = {QStringLiteral("3d sketch"), QStringLiteral("placement"), QStringLiteral("manipulator")};
    def.enabled = [this] {
      return m_sketch3d != nullptr && m_sketch3d->isActive() && m_sketch3d->takesPlacementKeys();
    };
    def.disabledReason = [] { return tr("only while a 3D sketch's drawing tool is active"); };
    def.run = [this, command = def.id] { runSketch3dKey(command); };
    m_registry->add(def);
  };
  add(Sketch3dController::kNextPlaneCommand, tr("Next Placement Plane"), "offset-plane", QKeySequence(Qt::Key_Tab),
      {}, tr("Next points on the next placement plane: XY, YZ, ZX"));
  // Shift+Tab arrives as Backtab on some systems: both are its keys.
  add(Sketch3dController::kPreviousPlaneCommand, tr("Previous Placement Plane"), "offset-plane",
      QKeySequence(Qt::SHIFT | Qt::Key_Tab), {QKeySequence(Qt::SHIFT | Qt::Key_Backtab)},
      tr("Next points on the previous placement plane: ZX, YZ, XY"));
  // Up is Turn View Up's elsewhere: this one's context wins over it while a
  // drawing tool is active (MainWindow::updateActions).
  add(Sketch3dController::kNormalCommand, tr("Placement Along Normal"), "construction-axis",
      QKeySequence(Qt::Key_Up), {}, tr("On Plane or Normal: the next point moves along the plane's normal"));
}

void MainWindow::runSketch3dKey(const QString& id) {
  using sketch::Sketch3dController;
  if (m_sketch3d == nullptr || !m_sketch3d->isActive() || !m_sketch3d->takesPlacementKeys()) {
    return;
  }
  const QList<QKeySequence> keys = m_registry->effectiveShortcuts(id);
  const QString key = keys.isEmpty() ? id : keys.front().toString(QKeySequence::PortableText);
  // A field with the keyboard keeps the key: Tab and Shift+Tab move to the
  // next or the previous field, as they would without the command.
  if (QWidget* focus = QApplication::focusWidget(); takesTyping(focus) && !keys.isEmpty()) {
    const QKeyCombination combination = keys.front()[0];
    QKeyEvent press(QEvent::KeyPress, combination.key(), combination.keyboardModifiers());
    QCoreApplication::sendEvent(focus, &press);
    qDebug().noquote() << QStringLiteral("3D placement key %1 left to the field %2").arg(key, focus->objectName());
    return;
  }
  if (id == QLatin1String(Sketch3dController::kNormalCommand)) {
    m_sketch3d->toggleNormal(key);
  } else {
    m_sketch3d->cyclePlane(id == QLatin1String(Sketch3dController::kPreviousPlaneCommand), key);
  }
}

void MainWindow::logSketch3dEntities() {
  if (m_sketch3d == nullptr || !m_sketch3d->isActive()) {
    return;
  }
  const QString uid = m_sketch3d->uid();
  QStringList lines;
  for (const SketchEntityDisplay& entity : m_sketchDisplay) {
    if (entity.sketch != uid || entity.shape.IsNull()) {
      continue;
    }
    gp_Pnt at;
    try {
      if (entity.shape.ShapeType() == TopAbs_VERTEX) {
        at = BRep_Tool::Pnt(TopoDS::Vertex(entity.shape));
      } else {
        const TopExp_Explorer edges(entity.shape, TopAbs_EDGE);
        if (!edges.More()) {
          continue;
        }
        const BRepAdaptor_Curve curve(TopoDS::Edge(edges.Current()));
        at = curve.Value((curve.FirstParameter() + curve.LastParameter()) / 2.0);
      }
    } catch (const Standard_Failure&) {
      continue;
    }
    const QPoint point = m_viewer->mapTo(window(), m_viewer->toWidget(at));
    lines << QStringLiteral("3D sketch entity %1/%2 at %3,%4").arg(entity.sketch, entity.id).arg(point.x()).arg(point.y());
  }
  const QString logged = lines.join(QLatin1Char('\n'));
  if (logged == m_loggedSketch3dEntities) {
    return;
  }
  m_loggedSketch3dEntities = logged;
  for (const QString& line : std::as_const(lines)) {
    qDebug().noquote() << line;
  }
}

void MainWindow::convertSketchTo3d(const QString& uid) {
  // The Browser's Convert to 3D Sketch opens the sketch first (mitcad#1544).
  if (m_mode == Mode::Sketch && m_sketch->isActive() && m_sketch->uid() == uid) {
    convertOpenSketchTo3d();
    return;
  }
  if (m_mode != Mode::Idle) {
    showError(tr("Convert to 3D Sketch: finish what is open first"));
    return;
  }
  editSketch(uid);
  if (m_mode == Mode::Sketch && m_sketch->isActive() && m_sketch->uid() == uid) {
    convertOpenSketchTo3d();
  }
}

void MainWindow::convertOpenSketchTo3d() {
  if (m_mode != Mode::Sketch || !m_sketch->isActive()) {
    return;
  }
  const QString uid = m_sketch->uid();
  const QString name = m_sketch->model().name;
  if (m_sketch->tool() != nullptr || m_sketch->valueEditorOpen()) {
    m_lastError = tr("Convert to 3D Sketch: finish or Escape the active tool first");
    showError(m_lastError);
    qInfo().noquote() << QStringLiteral("Convert to 3D Sketch refused: a tool has pending input");
    return;
  }
  QJsonObject result;
  if (!runCommand({{QStringLiteral("cmd"), QStringLiteral("sketch3d.convert")}, {QStringLiteral("sketch"), uid}},
                  &result)) {
    // The model's refusal is shown; it names the features in the way.
    qInfo().noquote() << QStringLiteral("Convert to 3D Sketch refused: %1").arg(lastError());
    refreshScene();
    return;
  }
  switchSketchEditor(true);
  const auto listed = [&result](const char* key) {
    QStringList items;
    for (const QJsonValue& value : result.value(QString::fromLatin1(key)).toArray()) {
      items << value.toString();
    }
    return items.isEmpty() ? QStringLiteral("none") : items.join(QStringLiteral(", "));
  };
  const QJsonObject status = result.value(QStringLiteral("status")).toObject();
  const QJsonObject plane = result.value(QStringLiteral("on_sketch_plane")).toObject();
  QStringList kept;
  for (const QJsonValue& value : plane.value(QStringLiteral("points")).toArray()) {
    kept << value.toString();
  }
  QStringList rewritten;
  for (const QJsonValue& value : result.value(QStringLiteral("rewritten")).toArray()) {
    rewritten << value.toObject().value(QStringLiteral("name")).toString();
  }
  qDebug().noquote() << QStringLiteral("Converted %1 (%2) to a 3D sketch: dof %3, in plane %4, on sketch plane %5, "
                                       "attachments %6, rewritten %7")
                            .arg(name, uid)
                            .arg(status.value(QStringLiteral("dof")).toInt())
                            .arg(listed("in_plane"))
                            .arg(kept.isEmpty() ? QStringLiteral("none") : kept.join(QStringLiteral(", ")))
                            .arg(listed("attachments"))
                            .arg(rewritten.isEmpty() ? QStringLiteral("none") : rewritten.join(QStringLiteral(", ")));
  showHint(tr("%1 is a 3D sketch now: its points and curves can leave its plane; Undo turns it back into 2D.")
               .arg(name));
}

void MainWindow::convertSketchTo2d(const QString& uid) {
  // The Browser's Convert to 2D Sketch opens the 3D sketch first
  // (mitcad#1550).
  if (m_mode == Mode::Sketch && m_sketch3d->isActive() && m_sketch3d->uid() == uid) {
    convertOpenSketchTo2d();
    return;
  }
  if (m_mode != Mode::Idle) {
    showError(tr("Convert to 2D Sketch: finish what is open first"));
    return;
  }
  editSketch(uid);
  if (m_mode == Mode::Sketch && m_sketch3d->isActive() && m_sketch3d->uid() == uid) {
    convertOpenSketchTo2d();
  }
}

void MainWindow::convertOpenSketchTo2d() {
  if (m_mode != Mode::Sketch || !m_sketch3d->isActive()) {
    return;
  }
  const QString uid = m_sketch3d->uid();
  const QString name = m_sketch3d->model().name;
  if (m_sketch3d->toolActive()) {
    m_lastError = tr("Convert to 2D Sketch: finish or Escape the active tool first");
    showError(m_lastError);
    qInfo().noquote() << QStringLiteral("Convert to 2D Sketch refused: a tool is active");
    return;
  }
  QJsonObject result;
  if (!runCommand({{QStringLiteral("cmd"), QStringLiteral("sketch3d.convert_to_2d")}, {QStringLiteral("sketch"), uid}},
                  &result)) {
    // The model's refusal is shown; it names what lies out of the plane or
    // the features in the way.
    qInfo().noquote() << QStringLiteral("Convert to 2D Sketch refused: %1").arg(lastError());
    refreshScene();
    return;
  }
  switchSketchEditor(false);
  const auto listed = [&result](const char* key) {
    QStringList items;
    for (const QJsonValue& value : result.value(QString::fromLatin1(key)).toArray()) {
      items << value.toString();
    }
    return items.isEmpty() ? QStringLiteral("none") : items.join(QStringLiteral(", "));
  };
  QStringList rewritten;
  for (const QJsonValue& value : result.value(QStringLiteral("rewritten")).toArray()) {
    rewritten << value.toObject().value(QStringLiteral("name")).toString();
  }
  qDebug().noquote() << QStringLiteral("Converted %1 (%2) to a 2D sketch: dof %3, dropped %4, projected %5, "
                                       "rewritten %6")
                            .arg(name, uid)
                            .arg(result.value(QStringLiteral("dof")).toInt())
                            .arg(listed("dropped"))
                            .arg(listed("projected"))
                            .arg(rewritten.isEmpty() ? QStringLiteral("none") : rewritten.join(QStringLiteral(", ")));
  for (const QJsonValue& note : result.value(QStringLiteral("notes")).toArray()) {
    qInfo().noquote() << QStringLiteral("Convert to 2D Sketch: %1").arg(note.toString());
  }
  showHint(tr("%1 is a 2D sketch again; Undo makes it 3D.").arg(name));
}

void MainWindow::switchSketchEditor(bool spatial) {
  const QString uid = editingSketchUid();
  if (uid.isEmpty()) {
    return;
  }
  // The sketch edit goes on: Finish Sketch still groups it and rolls the
  // timeline forward, and Undo at its start still leaves it.
  const bool existing = m_sketchExisting;
  const int depth = m_sketchEnterDepth;
  const qint64 revision = m_sketchEnterRevision; // the conversion is a change (mitcad#1623)
  const int marker = m_sketchRestoreMarker;
  const bool agent = m_mcpSketchEdited;
  const QString occurrence = m_sketchOccurrence;
  // The view the sketch was opened from stays the one a 2D sketch's Finish
  // puts back: the 2D editor taking the sketch again saved the view it had
  // then, the 2D editor's own top view after a conversion's round trip
  // (mitcad#1592).
  const occ::handle<Graphic3d_Camera> opened = m_viewer->savedCamera();
  if (spatial) {
    m_sketch->stopTool();
    m_sketch->leave();
    enterSketch3d(uid, existing, occurrence);
  } else {
    m_sketch3d->stopTool();
    m_sketch3d->leave();
    enterSketch(uid, existing, occurrence);
  }
  if (!opened.IsNull()) {
    m_viewer->setSavedCamera(opened);
  }
  m_sketchEnterDepth = depth;
  m_sketchEnterRevision = revision;
  m_sketchRestoreMarker = marker;
  m_mcpSketchEdited = agent;
  updateActions();
}

void MainWindow::followSketchKind() {
  if (m_mode != Mode::Sketch) {
    return;
  }
  const QString uid = editingSketchUid();
  if (uid.isEmpty()) {
    return;
  }
  QString type;
  const QJsonObject timeline = queryObject({{QStringLiteral("query"), QStringLiteral("timeline")}});
  for (const QJsonValue& value : timeline.value(QStringLiteral("features")).toArray()) {
    const QJsonObject feature = value.toObject();
    if (feature.value(QStringLiteral("uid")).toString() == uid) {
      type = feature.value(QStringLiteral("type")).toString();
    }
  }
  if (type.isEmpty()) {
    return; // gone: undo leaves sketch mode
  }
  const bool spatial = type == QStringLiteral("spatial_sketch");
  const bool editing3d = m_sketch3d != nullptr && m_sketch3d->isActive();
  if (spatial == editing3d) {
    return;
  }
  switchSketchEditor(spatial);
  qDebug().noquote() << QStringLiteral("Sketch editor follows the history: %1 is a %2 sketch")
                            .arg(editingSketchName(), spatial ? QStringLiteral("3D") : QStringLiteral("2D"));
}

} // namespace mitcad
