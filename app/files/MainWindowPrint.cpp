// SPDX-License-Identifier: MIT
// 3D Print (mitcad#13, Print3d.hpp): the selected bodies, or every visible
// one, written for a slicer into a folder of their own (as one 3MF file or
// an STL file per body, through the model's `export`) and given to it in
// one start; without a slicer, or when it does not start, the folder
// opens.
#include "../MainWindow.hpp"

#include <utility>

#include <QColor>
#include <QDesktopServices>
#include <QDir>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonObject>
#include <QMessageBox>
#include <QProcess>
#include <QUrl>
#include <QtLogging>
#include <QtMath>

#include "../framework/Appearances.hpp"
#include "Print3d.hpp"
#include "mitcad/geometry/import.hpp"

namespace mitcad {
namespace {

QJsonObject exportCommand(const QString& path, const QString& format, const QStringList& bodies,
                          const QJsonValue& refinement) {
  return {{QStringLiteral("cmd"), QStringLiteral("export")},
          {QStringLiteral("path"), path},
          {QStringLiteral("format"), format},
          {QStringLiteral("bodies"), QJsonArray::fromStringList(bodies)},
          {QStringLiteral("refinement"), refinement}};
}

} // namespace

QJsonObject MainWindow::appearanceColors(const QStringList& bodies) const {
  QJsonObject colors;
  for (const QJsonValue& value : queryArray(QStringLiteral("bodies"))) {
    const QJsonObject body = value.toObject();
    const QString uid = body.value(QStringLiteral("uid")).toString();
    const QColor color = appearanceColor(body.value(QStringLiteral("appearance")).toString());
    if ((bodies.isEmpty() || bodies.contains(uid)) && color.isValid()) {
      colors.insert(uid, QJsonArray{color.redF(), color.greenF(), color.blueF()});
    }
  }
  return colors;
}

void MainWindow::print3d() {
  // What the selection stands for (faces, edges and vertices their
  // bodies, as Export takes it; an occurrence the bodies shown in it),
  // else every visible body.
  const QJsonArray instances = queryArray(QStringLiteral("instances"));
  QStringList uids;
  const auto add = [&uids](const QString& body) {
    if (!uids.contains(body)) {
      uids << body;
    }
  };
  for (const SelectionItem& item : std::as_const(m_selection)) {
    if (item.kind == SelectKind::Body || item.kind == SelectKind::Face || item.kind == SelectKind::Edge ||
        item.kind == SelectKind::Vertex) {
      add(item.owner);
    } else if (item.kind == SelectKind::Component) {
      for (const QJsonValue& value : instances) {
        QStringList path;
        for (const QJsonValue& occurrence : value.toObject().value(QStringLiteral("path")).toArray()) {
          path << occurrence.toString();
        }
        const QString occurrence = path.join(QLatin1Char('/'));
        if (occurrence == item.occurrence || occurrence.startsWith(item.occurrence + QLatin1Char('/'))) {
          add(value.toObject().value(QStringLiteral("body")).toString());
        }
      }
    }
  }
  const bool selected = !uids.isEmpty();
  if (!selected) {
    for (const QJsonValue& value : instances) {
      const QJsonObject instance = value.toObject();
      if (instance.value(QStringLiteral("visible")).toBool()) {
        add(instance.value(QStringLiteral("body")).toString());
      }
    }
  }
  // Solids and mesh bodies go; sheet and empty bodies cannot be printed.
  QStringList sent;
  QStringList names;
  QStringList leftOut;
  for (const QString& uid : std::as_const(uids)) {
    const auto shape = bodyShape(uid);
    const geometry::BodyKind kind = shape ? geometry::body_kind(shape->occt()) : geometry::BodyKind::Empty;
    if (kind == geometry::BodyKind::Sheet || kind == geometry::BodyKind::Empty) {
      leftOut << (kind == geometry::BodyKind::Sheet ? tr("%1 (a surface body)") : tr("%1 (empty)")).arg(bodyName(uid));
      continue;
    }
    sent << uid;
    names << bodyName(uid);
  }
  if (!leftOut.isEmpty()) {
    qWarning().noquote() << QStringLiteral("3D Print: left out %1").arg(leftOut.join(QStringLiteral(", ")));
  }
  if (sent.isEmpty()) {
    qInfo().noquote() << QStringLiteral("3D Print: no solid or mesh bodies");
    showError(selected ? tr("3D Print: the selection has no solid or mesh bodies.")
                       : tr("3D Print: there are no visible solid or mesh bodies."));
    return;
  }
  const QString what = selected ? tr("%n selected: %1", nullptr, static_cast<int>(sent.size()))
                                      .arg(names.join(QStringLiteral(", ")))
                                : tr("%n visible: %1", nullptr, static_cast<int>(sent.size()))
                                      .arg(names.join(QStringLiteral(", ")));
  const std::optional<PrintChoice> choice = askPrint(this, what, leftOut.join(QStringLiteral(", ")), findSlicers());
  if (!choice) {
    qInfo().noquote() << QStringLiteral("3D Print cancelled");
    return;
  }

  // A folder of the design's own in the temporary folder, emptied first.
  const QString design = m_filePath.isEmpty() ? documentName() : QFileInfo(m_filePath).completeBaseName();
  QString error;
  const QString folder = preparePrintFolder(QDir::tempPath(), design, &error);
  if (folder.isEmpty()) {
    QMessageBox::warning(this, tr("3D Print"), error);
    return;
  }
  const QJsonValue refinement =
      choice->refinement == QStringLiteral("custom")
          ? QJsonValue(QJsonObject{{QStringLiteral("deviation"), choice->deviation},
                                   {QStringLiteral("angle"), qDegreesToRadians(choice->angle)}})
          : QJsonValue(choice->refinement);
  QStringList files;
  QStringList written;
  if (choice->format == QStringLiteral("3mf")) {
    // One object with a part per body, in the colours shown.
    const QString path = QDir(folder).filePath(safeFileName(design) + QStringLiteral(".3mf"));
    QJsonObject command = exportCommand(path, QStringLiteral("3mf"), sent, refinement);
    command.insert(QStringLiteral("colors"), appearanceColors(sent));
    QJsonObject result;
    if (!runCommand(command, &result)) {
      QMessageBox::warning(this, tr("3D Print"), tr("Could not write the 3MF file:\n%1").arg(m_lastError));
      return;
    }
    files << path;
    for (const QJsonValue& value : result.value(QStringLiteral("meshes")).toArray()) {
      const QJsonObject mesh = value.toObject();
      written << QStringLiteral("%1 %2 triangles, %3 mm3")
                     .arg(mesh.value(QStringLiteral("name")).toString())
                     .arg(mesh.value(QStringLiteral("triangles")).toInt())
                     .arg(mesh.value(QStringLiteral("volume")).toDouble(), 0, 'f', 3);
    }
  } else {
    // A file per body, named after it, in the design's coordinates; a body
    // shown more than once a file per occurrence (mitcad#17).
    const QVector<PrintPiece> pieces = printPieces(
        sent, names, query({{QStringLiteral("query"), QStringLiteral("instances")}, {QStringLiteral("hidden"), true}})
                         .toArray());
    QStringList pieceNames;
    for (const PrintPiece& piece : pieces) {
      pieceNames << piece.name;
    }
    const QStringList fileNames = printFileNames(pieceNames, QStringLiteral("stl"));
    for (int i = 0; i < pieces.size(); ++i) {
      const QString path = QDir(folder).filePath(fileNames[i]);
      QJsonObject command = exportCommand(path, QStringLiteral("stl"), {pieces[i].body}, refinement);
      if (!pieces[i].occurrence.isEmpty()) {
        command.insert(QStringLiteral("occurrence"), pieces[i].occurrence);
      }
      if (!runCommand(command)) {
        QMessageBox::warning(this, tr("3D Print"),
                             tr("Could not write %1:\n%2").arg(QDir::toNativeSeparators(path), m_lastError));
        return;
      }
      files << path;
      written << fileNames[i];
    }
  }
  qInfo().noquote() << QStringLiteral("3D Print: %1 bodies as %2 (%3) to %4: %5")
                           .arg(sent.size())
                           .arg(choice->format, choice->refinement, folder, written.join(QStringLiteral(", ")));

  // One start of the slicer with every file.
  const auto openFolder = [folder] {
    QDesktopServices::openUrl(QUrl::fromLocalFile(folder));
    qInfo().noquote() << QStringLiteral("3D Print: opened %1").arg(folder);
  };
  if (!choice->slicer.isValid()) {
    openFolder();
    showHint(tr("The files are in %1.").arg(QDir::toNativeSeparators(folder)));
    return;
  }
  QStringList arguments = choice->slicer.arguments;
  for (const QString& file : std::as_const(files)) {
    arguments << QDir::toNativeSeparators(file);
  }
  arguments << choice->slicer.argumentsAfter;
  if (!QProcess::startDetached(choice->slicer.program, arguments, folder)) {
    qWarning().noquote() << QStringLiteral("3D Print: %1 did not start").arg(choice->slicer.describe());
    showError(tr("%1 did not start; the files are in %2.")
                  .arg(choice->slicer.name.isEmpty() ? choice->slicer.program : choice->slicer.name,
                       QDir::toNativeSeparators(folder)));
    openFolder();
    return;
  }
  qInfo().noquote() << QStringLiteral("3D Print: started %1 %2")
                           .arg(QDir::toNativeSeparators(choice->slicer.program), arguments.join(QLatin1Char(' ')));
  showHint(tr("Sent %n body(ies) to %1.", nullptr, static_cast<int>(sent.size()))
               .arg(choice->slicer.name.isEmpty() ? QFileInfo(choice->slicer.program).fileName()
                                                  : choice->slicer.name));
}

} // namespace mitcad
