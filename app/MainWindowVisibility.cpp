// SPDX-License-Identifier: MIT
// Light bulbs in every mode (mitcad#295; kept analyses and image references
// too since mitcad#1263) and Create Sketch's plane pick
// (mitcad#474: its banner and the picks it refuses; mitcad#844: the origin
// planes behind the model) in the main window.
#include "MainWindow.hpp"

#include <QtLogging>

#include "OcctViewer.hpp"
#include "browser/BrowserController.hpp"
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
    return m_sketch->isActive() ? m_sketch->model().name : QString();
  default:
    return QString();
  }
}

void MainWindow::logSketchPicks() {
  if (!qEnvironmentVariableIsSet("MITCAD_LOG_PICKS")) {
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
  return tr("Create Sketch: pick a plane or a planar face. Esc cancels.");
}

void MainWindow::updateModeBanner() {
  if (m_modeBanner != nullptr) {
    // Create Sketch's plane, or an annotation's pick (mitcad#702).
    m_modeBanner->setText(m_mode == Mode::PickPlane ? pickPlaneText() : annotationPickText());
  }
  // The planes startSketch put behind the model for the pick (mitcad#844)
  // go back with it: the sketch started, Esc, another document. A command's
  // session sets them for its own inputs (CommandSession::activate).
  if (m_mode != Mode::PickPlane && m_mode != Mode::Command && !m_session) {
    m_viewer->setPlanesBehind(false);
    m_viewer->setPlanesByDepth(false);
  }
}

void MainWindow::refusePlanePick(const SelectionItem& item) {
  QString reason;
  if (item.kind == SelectKind::Plane ||
      (item.kind == SelectKind::Face && item.geometry == QStringLiteral("plane"))) {
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
    reason = tr("%1 is not planar (%2); pick a planar face or a plane, or make one with Construct.")
                 .arg(item.describe(), kind);
  }
  if (reason.isEmpty()) {
    return;
  }
  showError(tr("Create Sketch: %1").arg(reason));
  qInfo().noquote() << QStringLiteral("Create Sketch: refused pick %1: %2").arg(item.describe(), reason);
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
      used, [this](const QString& sketch) { return m_snapshot.isShown(sketch); }, label);
  for (const QString& sketch : shown) {
    const DocumentSnapshot::Feature* feature = m_snapshot.feature(sketch);
    qDebug().noquote() << QStringLiteral("Sketch %1 shown for %2")
                              .arg(feature != nullptr ? feature->name : sketch, label);
  }
}

void MainWindow::endCommandSketches(CommandSketches ended) {
  const QString label = ended.label();
  for (const CommandSketches::Ended& sketch :
       ended.end([this](const QString& uid) { return m_snapshot.isShown(uid); })) {
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

bool MainWindow::sketchShown(const QString& sketch) const {
  return m_commandSketches.shows(sketch, m_snapshot.isShown(sketch));
}

} // namespace mitcad
