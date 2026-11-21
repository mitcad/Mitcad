// SPDX-License-Identifier: MIT
// Sketch blocks (mitcad#2110; app/COMMANDS.md, "Sketch blocks"): Edit Block
// from the Browser's Sketch Blocks opens the block in a sketch of its own
// (the model's `edit_sketch_block` without an instance makes it), its
// geometry in the block's frame; Finish Sketch or Finish Block there takes
// the edit and the sketch goes (MainWindow::finishSketch).
#include "MainWindow.hpp"

#include <QJsonObject>
#include <QString>

#include <QDebug>
#include <QtLogging>

namespace mitcad {

void MainWindow::editSketchBlock(const QString& name) {
  if (m_mode != Mode::Idle) {
    showHint(tr("Finish the sketch first, then edit block %1.").arg(name));
    return;
  }
  QJsonObject result;
  if (!runCommand({{QStringLiteral("cmd"), QStringLiteral("edit_sketch_block")}, {QStringLiteral("block"), name}},
                  &result)) {
    return;
  }
  const QString sketch = result.value(QStringLiteral("sketch")).toString();
  qDebug().noquote() << QStringLiteral("Editing block %1 in its own sketch %2").arg(name, sketch);
  editSketch(sketch);
  showHint(tr("Editing block %1: its base point is at the origin. Finish Block (or Finish Sketch) takes the "
              "edit; Cancel Block leaves it as it was.")
               .arg(name));
}

} // namespace mitcad
