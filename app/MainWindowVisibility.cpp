// SPDX-License-Identifier: MIT
// Light bulbs in every mode (mitcad#295; kept analyses and image references
// too since mitcad#1263) and Create Sketch's plane pick
// (mitcad#474: its banner and the picks it refuses; mitcad#844: the origin
// planes behind the model) in the main window, and View › Show's kinds of
// the whole design (mitcad#1287).
#include "MainWindow.hpp"

#include <utility>

#include <QAction>
#include <QMenu>
#include <QtLogging>

#include "OcctViewer.hpp"
#include "browser/BrowserController.hpp"
#include "framework/CommandRegistry.hpp"
#include "sketch/Sketch3dController.hpp"
#include "sketch/SketchSurface.hpp"
#include "view/ModeBanner.hpp"

namespace mitcad {
namespace {

// Whether the commands show or hide a kept analysis (mitcad#1263).
bool showsOrHidesAnalysis(const QJsonArray& commands) {
  for (const QJsonValue& command : commands) {
    if (command.toObject().value(QStringLiteral("cmd")).toString() == QStringLiteral("set_analysis_visible")) {
      return true;
    }
  }
  return false;
}

} // namespace

// The browser's light bulbs while Create Sketch asks for a plane, while
// sketching and during commands: a command's session runs them outside its
// undo range; the plane pick goes on over the bodies shown now.
bool MainWindow::runVisibilityCommands(const QJsonArray& commands, const QString& label) {
  if (commands.isEmpty()) {
    return true;
  }
  if (m_mode == Mode::Command && m_session) {
    // An inspection's panel shows its own section or print analysis until
    // it closes (followAnalyses): a kept analysis's light bulb waits for it,
    // as before mitcad#1263.
    if (m_session->def().inspect && showsOrHidesAnalysis(commands)) {
      const QString reason = changeBlockedReason();
      showStatus(reason, NoticeLevel::Error);
      qDebug().noquote() << QStringLiteral("Visibility during %1 refused: %2").arg(m_session->def().name, reason);
      return false;
    }
    return m_session->runVisibility(commands, label);
  }
  const bool done =
      commands.size() == 1 ? runModelCommand(commands.first().toObject()) : runModelCommands(commands, label);
  if (m_mode == Mode::PickPlane) {
    // refreshScene kept the pick's filter (updateIdleView) and its hint.
    logSketchPicks();
  }
  return done;
}

QString MainWindow::activeCommandName() const {
  switch (m_mode) {
  case Mode::Command:
    return m_session ? m_session->def().name : QString();
  case Mode::PickPlane:
    return QStringLiteral("Create Sketch");
  case Mode::Sketch:
    return editingSketchName(); // either kind (mitcad#1524)
  default:
    return QString();
  }
}

void MainWindow::logSketchPicks() {
  if (!qEnvironmentVariableIsSet("MITCAD_LOG_PICKS")) {
    return;
  }
  // Not while the view cannot try the faces (a navigation not ended, such
  // as a fit made at once in a window that is not exposed, Windows session
  // 0): an empty list then hid the faces for good (app.ui-windows-cell-grid).
  // The retry after the navigation's end or the next frame logs them.
  m_sketchPicksDeferred = !m_viewer->pickPointsReady();
  if (m_sketchPicksDeferred) {
    return;
  }
  // Where a click picks each face the policy lets the sketch lie on, and
  // where it hits each face that cannot take the sketch (mitcad#474: not
  // planar, or left out by the policy), as "Pick refused face ...".
  qDebug().noquote() << "Pick places:";
  const auto logPlace = [this](const SelectionItem& item, const QPoint& at, const QString& prefix) {
    const QPoint point = m_viewer->mapTo(window(), at);
    qDebug().noquote() << QStringLiteral("Pick %1%2 %3/%4%5 at %6,%7")
                              .arg(prefix, SelectionItem::kindName(item.kind), item.owner, item.name,
                                   item.occurrence.isEmpty() ? QString() : QStringLiteral(" in ") + item.occurrence)
                              .arg(point.x())
                              .arg(point.y());
  };
  for (const auto& [item, at] : m_viewer->pickPoints(SelectKind::Face)) {
    logPlace(item, at, QString());
  }
  for (const auto& [item, at] : m_viewer->refusedPickPoints(SelectKind::Face)) {
    logPlace(item, at, QStringLiteral("refused "));
  }
}

QString MainWindow::pickPlaneText() const {
  // The kind chosen names itself (mitcad#1524).
  // A 2D sketch also on a cylindrical or conical face (mitcad#1768).
  return m_sketchSpatial
             ? tr("Create 3D Sketch: pick a plane or a planar face. Esc cancels.")
             : tr("Create Sketch: pick a plane, a planar face or a cylindrical or conical face. Esc cancels.");
}

void MainWindow::updateModeBanner() {
  if (m_modeBanner != nullptr) {
    // Create Sketch's plane, or an annotation's pick (mitcad#702); while a
    // 3D sketch is edited, that it is one (mitcad#1524).
    const bool spatial = m_mode == Mode::Sketch && m_sketch3d != nullptr && m_sketch3d->isActive();
    m_modeBanner->setText(m_mode == Mode::PickPlane ? pickPlaneText()
                          : spatial ? tr("3D Sketch %1: Finish Sketch returns to the model.").arg(editingSketchName())
                                    : annotationPickText());
  }
  // Create Sketch's kind, on the command card while it waits for the plane
  // (mitcad#1524).
  showSketchKindRow(m_mode == Mode::PickPlane);
  // The planes startSketch put behind the model for the pick (mitcad#844)
  // go back with it: the sketch started, Esc, another document. A command's
  // session sets them for its own inputs (CommandSession::activate).
  if (m_mode != Mode::PickPlane && m_mode != Mode::Command && !m_session) {
    m_viewer->setPlanesBehind(false);
    m_viewer->setPlanesByDepth(false);
  }
  // Files that waited for the command or the sketch, and a Move that
  // waited for an inspection, go on once it closed (mitcad#2281).
  followArrivals();
}

void MainWindow::refusePlanePick(const SelectionItem& item) {
  QString reason;
  if (item.kind == SelectKind::Plane ||
      (item.kind == SelectKind::Face && item.geometry == QStringLiteral("plane")) ||
      (!m_sketchSpatial && sketch::isDevelopedFace(item))) {
    reason = sketchSupportRefusal(item); // the design policy's (mitcad#220)
  } else if (item.kind == SelectKind::Face) {
    // What the face is instead of a plane, from its surface's type.
    const QString& surface = item.geometry;
    const QString kind = surface == QStringLiteral("cylinder") ? tr("a cylindrical surface")
                         : surface == QStringLiteral("cone")   ? tr("a conical surface")
                         : surface == QStringLiteral("sphere") ? tr("a spherical surface")
                         : surface == QStringLiteral("torus")  ? tr("a toroidal surface")
                         : surface == QStringLiteral("mesh")   ? tr("a mesh's triangles")
                                                               : tr("a free-form surface, such as a thread's");
    // In words (mitcad#1741): the face's name is for the log below. A 2D
    // sketch lies on a cylindrical or conical face too (mitcad#1768); a 3D
    // sketch on planes only.
    const bool developed = surface == QStringLiteral("cylinder") || surface == QStringLiteral("cone");
    if (m_sketchSpatial || developed) {
      reason = tr("The face is not planar (%1); pick a planar face or a plane, or make one with Construct.")
                   .arg(kind);
    } else {
      reason = tr("The face is neither planar nor cylindrical or conical (%1); pick a plane or a planar, "
                  "cylindrical or conical face, or make a plane with Construct.")
                   .arg(kind);
    }
  }
  if (reason.isEmpty()) {
    return;
  }
  showError(tr("Create Sketch: %1").arg(reason));
  qInfo().noquote() << QStringLiteral("Create Sketch: refused pick %1: %2").arg(item.logDescription(), reason);
}

// The sketches a command shows for its time (mitcad#546,
// view/CommandSketches.hpp): an edited feature's (the `timeline` query's
// `sketches`, the same before and after its roll-back), else those of the
// profiles, curves and points a new command starts with (`items`). A view
// state: showSketches, showProfiles and the parameter labels show them as
// if their light bulbs were on; nothing goes to the model.
void MainWindow::beginCommandSketches(const QString& editUid, const Selection& items,
                                      const QString& command) {
  QStringList edited;
  QString label = command;
  if (!editUid.isEmpty()) {
    const DocumentSnapshot::Feature* feature = m_snapshot.feature(editUid);
    if (feature != nullptr) {
      edited = feature->sketches;
    }
    label = QStringLiteral("the edit of %1").arg(feature != nullptr ? feature->name : editUid);
  }
  const QStringList used = CommandSketches::usedBy(edited, editUid.isEmpty() ? items : Selection());
  const QStringList shown = m_commandSketches.begin(
      used, [this](const QString& sketch) { return m_snapshot.isDrawn(sketch); }, label);
  for (const QString& sketch : shown) {
    const DocumentSnapshot::Feature* feature = m_snapshot.feature(sketch);
    qDebug().noquote() << QStringLiteral("Sketch %1 shown for %2")
                              .arg(feature != nullptr ? feature->name : sketch, label);
  }
}

void MainWindow::endCommandSketches(CommandSketches ended) {
  const QString label = ended.label();
  for (const CommandSketches::Ended& sketch :
       ended.end([this](const QString& uid) { return m_snapshot.isDrawn(uid); })) {
    const DocumentSnapshot::Feature* feature = m_snapshot.feature(sketch.sketch);
    if (feature == nullptr) {
      continue; // no longer in the design (an undo during the command)
    }
    qDebug().noquote() << (sketch.staysShown
                               ? QStringLiteral("Sketch %1 stays shown after %2 (its light bulb)")
                               : QStringLiteral("Sketch %1 hidden again after %2"))
                              .arg(feature->name, label);
  }
}

void MainWindow::showStartingSketches(const CommandSession& session) {
  if (!m_commandInSketch && !session.isEditing()) {
    // A new command (or an analysis' edit): the sketches of the profiles,
    // curves and points it starts with, a hidden sketch's free region taken
    // as the newest profile among them. Picks in the view come from shown
    // sketches only; what a pick adds later shows nothing more.
    Selection items;
    for (const InputDef& input : session.def().inputs) {
      if (input.type == InputDef::Type::Selection) {
        items += session.state().items(input.id);
      }
    }
    beginCommandSketches(QString(), items, session.def().name);
  }
  if (!m_commandSketches.used().isEmpty()) {
    // Also when no roll-back refreshed the scene (a new command, a feature
    // after the marker): before the panel's first input logs its picks.
    showProfiles();
    showSketches();
  }
}

// View › Show (mitcad#1287): the origins, the construction geometry and the
// sketches of the whole design, every component's (linked ones too), turned
// off and on as display state the design keeps (the model's set_view_show).
// The light bulbs stay as they are: showDatums and sketchShown leave the
// hidden kinds out, and the browser greys their rows. Like a light bulb it
// works in every mode, outside a command's undo range.
void MainWindow::registerViewShowCommands() {
  const auto add = [this](const char* id, const char* kind, const QString& name, const char* icon,
                          const QString& tooltip, const QStringList& keywords) {
    CommandDef def;
    def.id = QString::fromLatin1(id);
    def.name = name;
    def.icon = QString::fromLatin1(icon);
    def.tooltip = tooltip;
    def.kind = CommandDef::Kind::Action;
    def.mode = CommandDef::Mode::Any;
    def.duringCommands = true;
    def.keywords = keywords;
    def.keywords << QStringLiteral("view") << QStringLiteral("show") << QStringLiteral("hide")
                 << QStringLiteral("display");
    const QString key = QString::fromLatin1(kind);
    def.run = [this, key] { toggleViewShow(key); };
    // Its action, made later by createActions, is made checkable where the
    // View menu takes it (ViewController::createMenus).
    m_registry->add(def);
  };
  add("view.show_origins", "origins", tr("Origins"), "origin",
      tr("Shows or hides the design's origin and the joint origins in every component"),
      {QStringLiteral("origin"), QStringLiteral("joint origin"), QStringLiteral("axes")});
  add("view.show_construction", "construction", tr("Construction"), "construction",
      tr("Shows or hides the construction planes, axes and points of every component"),
      {QStringLiteral("construction"), QStringLiteral("plane"), QStringLiteral("axis"), QStringLiteral("point"),
       QStringLiteral("datum")});
  add("view.show_sketches", "sketches", tr("Sketches"), "sketch",
      tr("Shows or hides the sketches of every component"), {QStringLiteral("sketch"), QStringLiteral("sketches")});
  // Every sketch's light bulb in the design at once (mitcad#1931, GitHub
  // mitcad/Mitcad#355), one undo step; also in the Browser's menus of the
  // root and of components (BrowserController::setAllSketches).
  const auto all = [this](const char* id, bool show, const QString& name, const QString& tooltip) {
    CommandDef def;
    def.id = QString::fromLatin1(id);
    def.name = name;
    def.icon = show ? QStringLiteral("eye") : QStringLiteral("eye-off");
    def.tooltip = tooltip;
    def.kind = CommandDef::Kind::Action;
    def.mode = CommandDef::Mode::Any;
    def.duringCommands = true;
    def.keywords = {QStringLiteral("sketch"), QStringLiteral("sketches"), QStringLiteral("light bulb"),
                    QStringLiteral("eye"), QStringLiteral("visibility"), QStringLiteral("show"),
                    QStringLiteral("hide"), QStringLiteral("all")};
    def.run = [this, show] {
      if (m_controller != nullptr) {
        m_controller->setAllSketches(QString(), show);
      }
    };
    m_registry->add(def);
  };
  all("view.show_all_sketches", true, tr("Show All Sketches"),
      tr("Turns on the light bulb of every sketch in the design"));
  all("view.hide_all_sketches", false, tr("Hide All Sketches"),
      tr("Turns off the light bulb of every sketch in the design"));
  // Hide/Show of the selection by a key (mitcad#2356, GitHub
  // mitcad/Mitcad#612): the light bulbs of the bodies, components, sketches
  // and construction geometry selected in the view or the Browser, one undo
  // step (BrowserController::toggleItemsVisibility); V in the 3D view, as
  // Keyboard Shortcuts sets it. The Browser row's Hide/Show and the view's
  // Hide submenu run the same.
  CommandDef toggle;
  toggle.id = QStringLiteral("view.toggle_visibility");
  toggle.name = tr("Hide/Show");
  toggle.icon = QStringLiteral("eye-off");
  toggle.tooltip = tr("Hides the selected items, or shows them when the first is hidden");
  toggle.kind = CommandDef::Kind::Action;
  toggle.mode = CommandDef::Mode::Model;
  toggle.shortcut = QKeySequence(Qt::Key_V);
  toggle.keywords = {QStringLiteral("hide"),       QStringLiteral("show"),   QStringLiteral("visibility"),
                     QStringLiteral("light bulb"), QStringLiteral("eye"),    QStringLiteral("toggle"),
                     QStringLiteral("body"),       QStringLiteral("component")};
  toggle.run = [this] {
    if (m_controller == nullptr) {
      return;
    }
    if (!m_controller->toggleItemsVisibility(m_selection)) {
      const QString message = tr("Select a body, component, sketch or construction geometry to hide or show it");
      showStatus(message, NoticeLevel::Information);
      qDebug().noquote() << QStringLiteral("Hide/Show: nothing to hide or show in %1 item(s)").arg(m_selection.size());
    }
  };
  m_registry->add(toggle);
  // Isolate and Unisolate by keys of the user's own (mitcad#2482, GitHub
  // mitcad/Mitcad#613): the Browser row menu's entries as commands, with no
  // default key; Keyboard Shortcuts binds them (BrowserController::
  // isolateItems, unisolate). Isolate acts on the bodies and components
  // selected in the view or the Browser.
  CommandDef isolate;
  isolate.id = QStringLiteral("view.isolate");
  isolate.name = tr("Isolate");
  isolate.icon = QStringLiteral("isolate");
  isolate.tooltip = tr("Shows only the selected bodies and components until Unisolate");
  isolate.kind = CommandDef::Kind::Action;
  isolate.mode = CommandDef::Mode::Model;
  isolate.keywords = {QStringLiteral("isolate"), QStringLiteral("only"), QStringLiteral("focus"),
                      QStringLiteral("hide"),    QStringLiteral("others"), QStringLiteral("body"),
                      QStringLiteral("component"), QStringLiteral("assembly")};
  isolate.run = [this] {
    if (m_controller == nullptr) {
      return;
    }
    if (!m_controller->isolateItems(m_selection)) {
      showStatus(tr("Select a body or a component to isolate it."), NoticeLevel::Information);
      qDebug().noquote() << QStringLiteral("Isolate: nothing to isolate in %1 item(s)").arg(m_selection.size());
    }
  };
  m_registry->add(isolate);
  CommandDef unisolate;
  unisolate.id = QStringLiteral("view.unisolate");
  unisolate.name = tr("Unisolate");
  unisolate.icon = QStringLiteral("eye");
  unisolate.tooltip = tr("Shows everything again as its own visibility says");
  unisolate.kind = CommandDef::Kind::Action;
  unisolate.mode = CommandDef::Mode::Model;
  unisolate.keywords = {QStringLiteral("unisolate"), QStringLiteral("isolate"), QStringLiteral("show"),
                        QStringLiteral("all"), QStringLiteral("restore")};
  unisolate.run = [this] {
    if (m_controller == nullptr) {
      return;
    }
    if (!m_controller->unisolate()) {
      showStatus(tr("Nothing is isolated."), NoticeLevel::Information);
      qDebug().noquote() << "Unisolate: nothing is isolated";
    }
  };
  m_registry->add(unisolate);
}

void MainWindow::toggleViewShow(const QString& kind) {
  bool shown = m_snapshot.showSketches;
  const bool origins = kind == QStringLiteral("origins");
  if (origins) {
    // Checked while the design's origin is drawn: also its Origin folder's
    // light bulb on (mitcad#1642). Turned on, it shows the origin, the
    // light bulb with it in one undo step; checked with the light bulb off
    // it drew nothing, and the first toggle hid what was not seen.
    shown = originsShown();
  } else if (kind == QStringLiteral("construction")) {
    shown = m_snapshot.showConstruction;
  }
  QJsonArray commands;
  if (!origins || shown || !m_snapshot.showOrigins) {
    commands.append(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("set_view_show")}, {kind, !shown}});
  }
  if (origins && !shown && !m_snapshot.originShown) {
    commands.append(QJsonObject{{QStringLiteral("cmd"), QStringLiteral("set_origin_visible")},
                                {QStringLiteral("visible"), true}});
  }
  if (runVisibilityCommands(commands, tr("Show All Origins"))) {
    qDebug().noquote() << QStringLiteral("View show %1: %2")
                              .arg(kind, shown ? QStringLiteral("hidden") : QStringLiteral("shown"));
  }
  // The check marks as the design has them, also after a refusal.
  syncViewShowChecks();
}

void MainWindow::syncViewShowChecks() {
  const std::pair<const char*, bool> checks[] = {{"view.show_origins", originsShown()},
                                                 {"view.show_construction", m_snapshot.showConstruction},
                                                 {"view.show_sketches", m_snapshot.showSketches}};
  for (const auto& [id, on] : checks) {
    if (QAction* action = m_registry->action(QString::fromLatin1(id))) {
      action->setCheckable(true);
      action->setChecked(on);
    }
  }
  // For UI tests: View › Show's entries with their check marks, as the
  // menu has them, when they change.
  if (const auto* menu = findChild<QMenu*>(QStringLiteral("viewShowMenu"))) {
    QStringList entries;
    for (const QAction* entry : menu->actions()) {
      entries << QStringLiteral("%1 %2").arg(entry->text().remove(QLatin1Char('&')),
                                             entry->isChecked() ? QStringLiteral("on") : QStringLiteral("off"));
    }
    const QString line = QStringLiteral("View Show menu: %1").arg(entries.join(QStringLiteral(" | ")));
    if (line != m_loggedViewShow) {
      m_loggedViewShow = line;
      qDebug().noquote() << line;
    }
  }
}

bool MainWindow::sketchShown(const QString& sketch) const {
  return m_commandSketches.shows(sketch, m_snapshot.isDrawn(sketch)); // View › Show too (mitcad#1287)
}

} // namespace mitcad
