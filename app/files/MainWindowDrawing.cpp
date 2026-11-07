// SPDX-License-Identifier: MIT
// File > Export Drawing (mitcad#471, DrawingDialog.hpp): the selected
// bodies, or every shown one, as a drawing with views, dimensions and a
// hole table, laid out and kept in the design (add_drawing with the
// dialog's choices as its layout, mitcad#707) and written by the model's
// export_drawing as PDF, SVG, DXF or DWG; or a drawing of the design chosen in
// the dialog, written as it is (mitcad#725).
#include "../MainWindow.hpp"

#include <QDir>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonObject>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "../framework/WaitCursor.hpp"
#include "DrawingDialog.hpp"

namespace mitcad {
namespace {

// The bodies a selection stands for: bodies, and the bodies of faces,
// edges and vertices (as Export takes them).
QStringList bodiesOf(const Selection& selection) {
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

void MainWindow::exportDrawing() {
  const QStringList selected = bodiesOf(m_selection);
  QStringList analyses;
  for (const QJsonValue value : std::as_const(m_snapshot.analyses)) {
    const QJsonObject analysis = value.toObject();
    if (analysis.value(QStringLiteral("type")).toString() == QStringLiteral("section")) {
      analyses << analysis.value(QStringLiteral("name")).toString();
    }
  }
  const QString base = m_filePath.isEmpty() ? documentName() : QFileInfo(m_filePath).completeBaseName();
  const QString suggested = QDir(dialogFolder(DialogGroup::Export)).filePath(base);
  // The root's drawings, which the dialog can lay out again, and the one
  // it laid out last (the `drawings` query's highest `exported`,
  // mitcad#725).
  QList<DrawingTarget> targets;
  QString last;
  int lastNumber = 0;
  for (const QJsonValue value : std::as_const(m_snapshot.drawings)) {
    const QJsonObject entry = value.toObject();
    const QString component = entry.value(QStringLiteral("component")).toString();
    if (!component.isEmpty() && component != QStringLiteral("C0")) {
      continue;
    }
    const QString uid = entry.value(QStringLiteral("drawing")).toString();
    targets.append({uid, entry.value(QStringLiteral("name")).toString()});
    const int number = entry.value(QStringLiteral("exported")).toInt();
    if (number > lastNumber) {
      lastNumber = number;
      last = uid;
    }
  }
  const std::optional<DrawingChoice> choice =
      askDrawing(this, suggested, static_cast<int>(selected.size()), analyses, targets, last, lengthUnit());
  if (!choice) {
    return;
  }
  rememberDialogFolder(DialogGroup::Export, choice->path);
  const QJsonObject options =
      choice->settings.command(choice->path, choice->selectedOnly ? selected : QStringList(), base, choice->section);
  // The drawing is kept in the design (mitcad#707): the dialog's choices
  // lay it out in the root's Drawings folder, and that drawing is written,
  // so that it can be opened, changed and written again. A design that
  // cannot change now (read-only, a panel open) gets the file only, laid
  // out for it as the kept one would be.
  QString drawing;
  // A checkpoint before the drawing is kept: a file that is not written
  // takes it back without a redo step. Taking and releasing it changes
  // nothing the scene shows (runCommand, no refresh). Without one (the
  // model refused it), Undo takes the drawing back, leaving a redo step.
  QString token;
  int depthBefore = -1;
  const auto discard = [this, &token] {
    if (!token.isEmpty()) {
      runCommand({{QStringLiteral("cmd"), QStringLiteral("discard_command")}, {QStringLiteral("token"), token}});
    }
  };
  // A drawing of the design chosen in the dialog (mitcad#725) is written
  // as it is: it follows the model already, and what was made on it by
  // hand stays. Only a new one is laid out from the dialog's settings.
  bool added = false;
  if (!choice->drawing.isEmpty()) {
    drawing = choice->drawing;
    qDebug().noquote() << QStringLiteral("Drawing exported as it is: %1").arg(drawing);
  } else if (canChangeModel()) {
    QJsonObject begun;
    if (runCommand({{QStringLiteral("cmd"), QStringLiteral("begin_command")}}, &begun)) {
      token = begun.value(QStringLiteral("token")).toString();
    }
    depthBefore = undoDepth();
    QJsonObject layout = options;
    for (const char* key : {"cmd", "path", "format"}) {
      layout.remove(QLatin1String(key));
    }
    QJsonObject made;
    if (!runModelCommand({{QStringLiteral("cmd"), QStringLiteral("add_drawing")}, {QStringLiteral("layout"), layout}},
                         &made)) {
      const bool cancelled = m_lastCancelled;
      const QString error = m_lastError;
      discard();
      if (!cancelled) {
        sheetWarning(this, tr("Export Drawing"), tr("Could not lay out the drawing:\n%1").arg(error));
      }
      return;
    }
    drawing = made.value(QStringLiteral("drawing")).toString();
    added = true;
    qDebug().noquote() << QStringLiteral("Drawing kept: %1 %2")
                              .arg(drawing, made.value(QStringLiteral("name")).toString());
    // What the layout could not do (views that show nothing, a scale that
    // does not fit), as the export says it.
    for (const QJsonValue warning : made.value(QStringLiteral("warnings")).toArray()) {
      qWarning().noquote() << QStringLiteral("Export drawing: %1").arg(warning.toString());
    }
  } else {
    qDebug().noquote() << QStringLiteral("Drawing not kept: %1").arg(changeBlockedReason());
  }
  QJsonObject command = options;
  if (!drawing.isEmpty()) {
    command = {{QStringLiteral("cmd"), QStringLiteral("export_drawing")},
               {QStringLiteral("path"), choice->path},
               {QStringLiteral("format"), options.value(QStringLiteral("format"))},
               {QStringLiteral("drawing"), drawing},
               {QStringLiteral("title"), base}};
  }
  // A DXF file in another unit than the design's (mitcad#1408); left out,
  // each sheet's unit (or the design's).
  const bool dxf = options.value(QStringLiteral("format")).toString() == QStringLiteral("dxf");
  if (dxf && !choice->dxfUnit.isEmpty() && choice->dxfUnit != lengthUnit()) {
    command.insert(QStringLiteral("units"), choice->dxfUnit);
  }
  QJsonObject result;
  bool written = false;
  {
    const WaitCursor wait(QStringLiteral("Export drawing"));
    written = runCommand(command, &result);
  }
  if (!written) {
    const QString error = m_lastError;
    const bool cancelled = m_lastCancelled;
    qWarning().noquote() << QStringLiteral("Export of %1 failed: %2").arg(choice->path, error);
    // No file, no new drawing: the one kept for it goes again, to the
    // checkpoint (no redo step left), else by Undo.
    if (added) {
      bool restored = false;
      if (!token.isEmpty()) {
        restored = runModelCommand({{QStringLiteral("cmd"), QStringLiteral("restore_command")},
                                    {QStringLiteral("token"), token}});
      } else if (depthBefore >= 0 && undoDepth() > depthBefore) {
        restored = runModelCommand({{QStringLiteral("cmd"), QStringLiteral("undo")}});
      }
      if (restored) {
        qDebug().noquote() << QStringLiteral("Drawing not kept: %1 was not written").arg(choice->path);
      } else {
        qWarning().noquote() << QStringLiteral("Drawing %1 kept although %2 was not written: %3")
                                    .arg(drawing, choice->path, m_lastError);
      }
    }
    discard();
    if (!cancelled) {
      sheetWarning(this, tr("Export Drawing"), tr("Could not export:\n%1").arg(error));
    }
    return;
  }
  discard();
  for (const QJsonValue warning : result.value(QStringLiteral("warnings")).toArray()) {
    qWarning().noquote() << QStringLiteral("Export drawing: %1").arg(warning.toString());
  }
  const int views = result.value(QStringLiteral("views")).toInt();
  const auto bodies = result.value(QStringLiteral("bodies")).toArray().size();
  const QString sheet = result.value(QStringLiteral("sheet")).toString();
  const QString scale = result.value(QStringLiteral("scale")).toString();
  // A DXF file's length unit (mitcad#1408).
  const QString fileUnits = result.value(QStringLiteral("file_units")).toString();
  qInfo().noquote() << QStringLiteral("Exported %1: drawing, %2 views, %3 bodies, %4, %5%6")
                           .arg(choice->path)
                           .arg(views)
                           .arg(bodies)
                           .arg(sheet, scale, fileUnits.isEmpty() ? QString() : QStringLiteral(", in %1").arg(fileUnits));
  const int sheets = result.value(QStringLiteral("sheets")).toInt();
  showHint(tr("Exported %1 (%2 view(s) of %3 body(ies), %4 at %5%6).")
               .arg(QDir::toNativeSeparators(choice->path))
               .arg(views)
               .arg(bodies)
               .arg(QString(sheet).replace(QLatin1Char('_'), QLatin1Char(' ')), scale,
                    sheets > 1 ? tr(", %1 sheets").arg(sheets) : QString()));
}

} // namespace mitcad
