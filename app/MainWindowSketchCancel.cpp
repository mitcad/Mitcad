// SPDX-License-Identifier: MIT
// Cancel Sketch, and every sketch edit as one undo step (mitcad#2225;
// app/COMMANDS.md, "Finish Sketch"). Finish Sketch merges the steps made
// in the sketch since it was entered into one: a new sketch's into its
// creation, an edited one's as "Edit <name>" (a rolled-back timeline's
// roll-forward merges them in MainWindow::finishSketch). Cancel Sketch
// merges them the same way, the rolled-back timeline marker's move among
// them and its roll-forward after them (mitcad#2263), and undoes that one
// step: the design is as it was before the sketch was opened, and Redo
// brings the edit back as Finish Sketch leaves it, the marker where it was
// before Edit Sketch. It does not go through
// MainWindow::undo(), which finishes an edited sketch once its own steps
// are undone.
#include "MainWindow.hpp"

#include <algorithm>

#include <QDebug>
#include <QJsonObject>
#include <QString>
#include <QtLogging>

#include "sketch/Sketch3dController.hpp"
#include "sketch/SketchController.hpp"

namespace mitcad {

QString MainWindow::sketchCreationStep(const QString& uid) const {
  const QString label = query({{QStringLiteral("query"), QStringLiteral("document")}})
                            .toObject()
                            .value(QStringLiteral("undo"))
                            .toString();
  return label == QStringLiteral("Add ") + featureName(uid) ? label : QString();
}

void MainWindow::mergeSketchSteps(const QString& name) {
  const int depth = undoDepth();
  if (depth <= m_sketchEnterDepth) {
    return; // nothing made in the sketch
  }
  if (!m_sketchCreation.isEmpty() && m_sketchEnterDepth > 0) {
    // A new sketch: one step with its creation, under the creation's label
    // (an agent's changes in it say so, mitcad#265).
    runCommand({{QStringLiteral("cmd"), QStringLiteral("merge_undo")},
                {QStringLiteral("depth"), m_sketchEnterDepth - 1},
                {QStringLiteral("label"), m_mcpSketchEdited ? tr("%1 (agent)").arg(m_sketchCreation) : m_sketchCreation}});
    return;
  }
  runCommand({{QStringLiteral("cmd"), QStringLiteral("merge_undo")},
              {QStringLiteral("depth"), m_sketchEnterDepth},
              {QStringLiteral("label"), m_mcpSketchEdited ? tr("Edit %1 (agent)").arg(name) : tr("Edit %1").arg(name)}});
}

bool MainWindow::cancelSketch() {
  if (m_mode != Mode::Sketch) {
    return true;
  }
  const bool spatial = m_sketch3d != nullptr && m_sketch3d->isActive();
  if (!spatial && m_sketch->openBlockIsTemporary()) {
    // A block edited in a sketch of its own (mitcad#2110): Cancel Block,
    // which leaves the block as it was and the sketch goes.
    qInfo().noquote() << QStringLiteral("Cancel Sketch: the block's own sketch %1 ends as Cancel Block")
                             .arg(editingSketchName());
    m_sketch->finishBlock(true);
    return m_mode != Mode::Sketch;
  }
  const QString uid = editingSketchUid();
  const QString name = editingSketchName();
  const bool created = !m_sketchExisting;
  // What a tool has pending is not committed: a half-drawn chain, a value
  // typed (a 3D sketch's too, mitcad#1525).
  m_sketch->resetTool();
  m_sketch->stopTool();
  if (m_sketch3d != nullptr) {
    m_sketch3d->clearPending();
    if (spatial) {
      m_sketch3d->stopTool();
    }
  }
  const int depth = undoDepth();
  const int made = std::max(0, depth - m_sketchEnterDepth);
  if (made == 0 && !created) {
    // Nothing changed: left as Finish Sketch leaves it (the timeline
    // marker rolled forward again).
    if (!finishSketch()) {
      return false;
    }
    showHint(tr("Sketch %1 left; nothing was changed in it.").arg(name));
    qInfo().noquote() << QStringLiteral("Sketch cancelled: %1, 0 step(s) taken back").arg(name);
    return true;
  }
  // The steps to take back as one: the sketch's own, and before them the
  // rolled-back timeline marker's move or a new sketch's creation.
  int from = m_sketchEnterDepth;
  QString label = m_mcpSketchEdited ? tr("Edit %1 (agent)").arg(name) : tr("Edit %1").arg(name);
  if (m_sketchRestoreMarker >= 0 && m_sketchEnterDepth > 0) {
    from = m_sketchEnterDepth - 1;
    // The marker goes where it was first, as Finish Sketch puts it
    // (mitcad#2263): the step Redo brings back is the one Finish Sketch
    // would have made, the later features shown again.
    if (!runCommand({{QStringLiteral("cmd"), QStringLiteral("set_marker")},
                     {QStringLiteral("position"), m_sketchRestoreMarker}}) &&
        m_lastCancelled) {
      // The features after the sketch were not computed: the sketch stays
      // open, as it was (as Finish Sketch leaves it then).
      refreshScene();
      showHint(tr("Computation canceled; Cancel Sketch computes the features after %1 first.").arg(name));
      qInfo().noquote() << QStringLiteral("Cancel Sketch: %1 stays open, the timeline did not roll forward").arg(name);
      return false;
    }
  } else if (created && !m_sketchCreation.isEmpty() && m_sketchEnterDepth > 0) {
    from = m_sketchEnterDepth - 1;
    label = m_mcpSketchEdited ? tr("%1 (agent)").arg(m_sketchCreation) : m_sketchCreation;
  }
  if (depth > from) {
    // One step under the edit's name (also a single step, renamed), which
    // Redo brings back.
    if (!runCommand({{QStringLiteral("cmd"), QStringLiteral("merge_undo")},
                     {QStringLiteral("depth"), from},
                     {QStringLiteral("label"), label}})) {
      qInfo().noquote() << QStringLiteral("Cancel Sketch: %1 stays open, its steps were not merged").arg(name);
      return false;
    }
    QJsonObject result;
    if (!runCommand({{QStringLiteral("cmd"), QStringLiteral("undo")}}, &result)) {
      // The reason is shown (a computation cancelled among them): the
      // sketch stays open, its edit one undo step now.
      refreshScene();
      qInfo().noquote() << QStringLiteral("Cancel Sketch: %1 stays open, the undo was refused").arg(name);
      return false;
    }
    qDebug().noquote() << "Undo:" << result.value(QStringLiteral("label")).toString();
  }
  bool deleted = false;
  if (created && sketchExists(uid)) {
    // A new sketch whose creation is no longer the last step goes as
    // Finish Sketch takes an empty one: deleted.
    deleted = runCommand({{QStringLiteral("cmd"), QStringLiteral("delete_feature")}, {QStringLiteral("uid"), uid}});
  }
  leaveSketchMode();
  refreshScene();
  if (created && !sketchExists(uid)) {
    showHint(deleted ? tr("Sketch canceled: %1 deleted.").arg(name)
                     : tr("Sketch canceled: %1 taken back; Redo brings it back.").arg(name));
    qInfo().noquote() << QStringLiteral("Sketch cancelled: %1, %2")
                             .arg(name, deleted ? QStringLiteral("deleted, it was new")
                                                : QStringLiteral("taken back, it was new"));
  } else {
    showHint(tr("Sketch canceled: the changes to %1 are taken back; Redo brings them back.").arg(name));
    qInfo().noquote() << QStringLiteral("Sketch cancelled: %1, %2 step(s) taken back").arg(name).arg(made);
  }
  return true;
}

} // namespace mitcad
