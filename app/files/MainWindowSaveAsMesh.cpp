// SPDX-License-Identifier: MIT
// Save As Mesh (mitcad#734, SaveAsMesh.hpp): the bodies of what a body's or
// a component's context menu was opened on, in the browser or in the view,
// to mesh files. A short dialog asks the format (3MF, STL binary or text,
// OBJ), the unit, one file or a file per body, the refinement (a preset or
// the surface deviation, the normal deviation and the longest edge) and
// whether the bodies lie on their print bed face (mitcad#731); OK asks for
// the file (or the folder) with the name of what is saved proposed, and
// the model's `export` writes it. The design does not change.
#include "../MainWindow.hpp"
#include "framework/ModelMessages.hpp"

#include <algorithm>
#include <memory>
#include <optional>
#include <utility>

#include <QCheckBox>
#include <QComboBox>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDir>
#include <QDoubleSpinBox>
#include <QFileInfo>
#include <QFormLayout>
#include <QJsonArray>
#include <QJsonObject>
#include <QMessageBox>
#include <QSettings>
#include <QVBoxLayout>
#include <QtLogging>

#include "../browser/DocumentSnapshot.hpp"
#include "../framework/DialogFolders.hpp"
#include "../framework/Dialogs.hpp"
#include "../framework/WaitCursor.hpp"
#include "Print3d.hpp"
#include "SaveAsMesh.hpp"
#include "mitcad/geometry/import.hpp"
#include "mitcad/geometry/shape.hpp"

namespace mitcad {
namespace {

constexpr char kGroup[] = "saveAsMesh";

// The choices of the last save (format, unit, refinement and its numbers);
// a file per body and the print bed are chosen each time.
SaveMeshOptions loadOptions() {
  QSettings settings;
  settings.beginGroup(QLatin1String(kGroup));
  SaveMeshOptions options;
  const QString format = settings.value(QStringLiteral("format"), options.format).toString();
  if (QStringList{QStringLiteral("3mf"), QStringLiteral("stl"), QStringLiteral("obj")}.contains(format)) {
    options.format = format;
  }
  options.ascii = settings.value(QStringLiteral("ascii"), false).toBool();
  const QString unit = settings.value(QStringLiteral("unit"), options.unit).toString();
  if (QStringList{QStringLiteral("mm"), QStringLiteral("cm"), QStringLiteral("m"), QStringLiteral("in"),
                  QStringLiteral("ft")}
          .contains(unit)) {
    options.unit = unit;
  }
  const QString refinement = settings.value(QStringLiteral("refinement"), options.refinement).toString();
  if (QStringList{QStringLiteral("low"), QStringLiteral("medium"), QStringLiteral("high"), QStringLiteral("custom")}
          .contains(refinement)) {
    options.refinement = refinement;
  }
  options.deviation = std::clamp(settings.value(QStringLiteral("deviation"), options.deviation).toDouble(), 0.0001, 100.0);
  options.angle = std::clamp(settings.value(QStringLiteral("angle"), options.angle).toDouble(), 0.5, 90.0);
  options.maxEdge = std::clamp(settings.value(QStringLiteral("maxEdge"), options.maxEdge).toDouble(), 0.0, 100000.0);
  return options;
}

void saveOptions(const SaveMeshOptions& options) {
  QSettings settings;
  settings.beginGroup(QLatin1String(kGroup));
  settings.setValue(QStringLiteral("format"), options.format);
  settings.setValue(QStringLiteral("ascii"), options.ascii);
  settings.setValue(QStringLiteral("unit"), options.unit);
  settings.setValue(QStringLiteral("refinement"), options.refinement);
  settings.setValue(QStringLiteral("deviation"), options.deviation);
  settings.setValue(QStringLiteral("angle"), options.angle);
  settings.setValue(QStringLiteral("maxEdge"), options.maxEdge);
}

// The dialog, short and without explanations; nothing when cancelled.
std::optional<SaveMeshOptions> askOptions(QWidget* parent, const MeshPlan& plan) {
  SaveMeshOptions options = loadOptions();
  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Save as Mesh"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* form = new QFormLayout;
  layout->addLayout(form);

  auto* format = new QComboBox;
  format->addItem(QObject::tr("3MF"), QStringLiteral("3mf"));
  format->addItem(QObject::tr("STL (binary)"), QStringLiteral("stl"));
  format->addItem(QObject::tr("STL (ASCII)"), QStringLiteral("stl-ascii"));
  format->addItem(QObject::tr("OBJ"), QStringLiteral("obj"));
  const QString formatKey =
      options.format == QStringLiteral("stl") && options.ascii ? QStringLiteral("stl-ascii") : options.format;
  format->setCurrentIndex(std::max(0, format->findData(formatKey)));
  form->addRow(QObject::tr("&Format:"), format);

  auto* unit = new QComboBox;
  for (const char* name : {"mm", "cm", "m", "in", "ft"}) {
    unit->addItem(QString::fromLatin1(name));
  }
  unit->setCurrentIndex(std::max(0, unit->findText(options.unit)));
  form->addRow(QObject::tr("&Unit:"), unit);

  // A file per body only when there are several.
  auto* structure = new QComboBox;
  structure->addItem(QObject::tr("One file"));
  structure->addItem(QObject::tr("One file per body"));
  form->addRow(QObject::tr("&Structure:"), structure);
  form->setRowVisible(structure, plan.bodies.size() > 1);

  auto* refinement = new QComboBox;
  refinement->addItem(QObject::tr("Low"), QStringLiteral("low"));
  refinement->addItem(QObject::tr("Medium"), QStringLiteral("medium"));
  refinement->addItem(QObject::tr("High"), QStringLiteral("high"));
  refinement->addItem(QObject::tr("Custom"), QStringLiteral("custom"));
  refinement->setCurrentIndex(std::max(0, refinement->findData(options.refinement)));
  form->addRow(QObject::tr("&Refinement:"), refinement);
  auto* deviation = new QDoubleSpinBox;
  deviation->setDecimals(4);
  deviation->setRange(0.0001, 100.0);
  deviation->setSuffix(QStringLiteral(" mm"));
  deviation->setValue(options.deviation);
  form->addRow(QObject::tr("Surface &deviation:"), deviation);
  auto* angle = new QDoubleSpinBox;
  angle->setDecimals(1);
  angle->setRange(0.5, 90.0);
  angle->setSuffix(QStringLiteral(" °"));
  angle->setValue(options.angle);
  form->addRow(QObject::tr("&Normal deviation:"), angle);
  // 0: no limit.
  auto* maxEdge = new QDoubleSpinBox;
  maxEdge->setDecimals(3);
  maxEdge->setRange(0.0, 100000.0);
  maxEdge->setSuffix(QStringLiteral(" mm"));
  maxEdge->setSpecialValueText(QObject::tr("None"));
  maxEdge->setValue(options.maxEdge);
  form->addRow(QObject::tr("Maximum &edge length:"), maxEdge);

  auto* onBed = new QCheckBox(QObject::tr("On its print &bed face"));
  if (!plan.bedBlocked.isEmpty()) {
    onBed->setEnabled(false);
    onBed->setToolTip(plan.bedBlocked);
  }
  form->addRow(QString(), onBed);

  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  layout->addWidget(buttons);

  const auto showRows = [form, refinement, deviation, angle, maxEdge] {
    const bool custom = refinement->currentData().toString() == QStringLiteral("custom");
    form->setRowVisible(deviation, custom);
    form->setRowVisible(angle, custom);
    form->setRowVisible(maxEdge, custom);
  };
  QObject::connect(refinement, &QComboBox::currentIndexChanged, &dialog, [&dialog, showRows](int) {
    showRows();
    dialog.adjustSize();
  });
  showRows();
  format->setFocus();
  qDebug().noquote() << QStringLiteral("Save As Mesh dialog opened: bodies %1%2; print bed face %3")
                            .arg(plan.names.join(QStringLiteral(", ")),
                                 plan.skipped.isEmpty()
                                     ? QString()
                                     : QStringLiteral("; left out %1").arg(plan.skipped.join(QStringLiteral(", "))),
                                 plan.bedBlocked.isEmpty() ? QStringLiteral("available")
                                                           : QStringLiteral("unavailable (%1)").arg(plan.bedBlocked));

  prepareModal(&dialog);
  if (execDialog(&dialog) != QDialog::Accepted) {
    qDebug().noquote() << "Save As Mesh dialog cancelled";
    return std::nullopt;
  }
  const QString chosen = format->currentData().toString();
  options.format = chosen == QStringLiteral("stl-ascii") ? QStringLiteral("stl") : chosen;
  options.ascii = chosen == QStringLiteral("stl-ascii");
  options.unit = unit->currentText();
  options.perBody = plan.bodies.size() > 1 && structure->currentIndex() == 1;
  options.refinement = refinement->currentData().toString();
  options.deviation = deviation->value();
  options.angle = angle->value();
  options.maxEdge = maxEdge->value();
  options.onBed = onBed->isEnabled() && onBed->isChecked();
  saveOptions(options);
  qDebug().noquote() << QStringLiteral("Save As Mesh dialog: %1").arg(options.describe());
  return options;
}

// What the selected items stand for: components where their occurrences
// place them, and bodies (of faces, edges and vertices too).
QVector<MeshItem> meshItems(const Selection& items) {
  QVector<MeshItem> chosen;
  for (const SelectionItem& item : items) {
    if (item.kind == SelectKind::Component) {
      chosen.append(MeshItem{MeshItem::Kind::Component, item.owner, item.occurrence});
    } else if (item.kind == SelectKind::Face || item.kind == SelectKind::Edge || item.kind == SelectKind::Vertex ||
               item.kind == SelectKind::Body) {
      chosen.append(MeshItem{MeshItem::Kind::Body, item.owner, item.occurrence});
    }
  }
  return chosen;
}

} // namespace

QString MainWindow::exportName(const Selection& items) const {
  QHash<QString, QString> bodyNames;
  for (const DocumentSnapshot::Body& body : std::as_const(m_snapshot.bodies)) {
    bodyNames.insert(body.uid, body.name);
  }
  QHash<QString, QString> components;
  for (const DocumentSnapshot::Component& component : std::as_const(m_snapshot.components)) {
    components.insert(component.uid, component.name);
  }
  const QString design = m_filePath.isEmpty() ? documentName() : QFileInfo(m_filePath).completeBaseName();
  return meshItemsName(meshItems(items), bodyNames, components, design);
}

MeshPlan MainWindow::saveAsMeshPlan(const Selection& items) const {
  const QVector<MeshItem> chosen = meshItems(items);
  QHash<QString, MeshBody> bodies;
  for (const DocumentSnapshot::Body& body : std::as_const(m_snapshot.bodies)) {
    const std::shared_ptr<geometry::Shape> shape = bodyShape(body.uid);
    const geometry::BodyKind kind = shape ? geometry::body_kind(shape->occt()) : geometry::BodyKind::Empty;
    MeshBody mesh;
    mesh.name = body.name;
    mesh.printable = kind == geometry::BodyKind::Solid || kind == geometry::BodyKind::Mesh;
    mesh.bedFace = !body.printBedFace.isEmpty() && body.printBedError.isEmpty();
    mesh.bedError = body.printBedFace.isEmpty() ? QString() : body.printBedError;
    bodies.insert(body.uid, mesh);
  }
  QHash<QString, QString> components;
  for (const DocumentSnapshot::Component& component : std::as_const(m_snapshot.components)) {
    components.insert(component.uid, component.name);
  }
  QJsonArray instances;
  if (std::any_of(chosen.begin(), chosen.end(),
                  [](const MeshItem& item) { return item.kind == MeshItem::Kind::Component; })) {
    instances = query({{QStringLiteral("query"), QStringLiteral("instances")}, {QStringLiteral("hidden"), true}})
                    .toArray();
  }
  const QString design = m_filePath.isEmpty() ? documentName() : QFileInfo(m_filePath).completeBaseName();
  return planSaveAsMesh(chosen, instances, bodies, components, design);
}

QString MainWindow::saveAsMeshBlocked(const Selection& items) const { return saveAsMeshPlan(items).blocked; }

void MainWindow::saveAsMesh(const Selection& items) {
  const MeshPlan plan = saveAsMeshPlan(items);
  if (!plan.blocked.isEmpty()) {
    showStatus(plan.blocked, NoticeLevel::Error);
    return;
  }
  const std::optional<SaveMeshOptions> options = askOptions(this, plan);
  if (!options) {
    return;
  }
  const QString extension = options->extension();
  const QStringList names = meshFileNames(plan, options->perBody, extension);
  QStringList paths;
  if (options->perBody) {
    const QString folder = dialogDirectory(this, tr("Save Meshes in Folder"), dialogFolder(DialogGroup::Export));
    if (folder.isEmpty()) {
      return;
    }
    QStringList existing;
    for (const QString& name : names) {
      paths << QDir(folder).filePath(name);
      if (QFileInfo::exists(paths.last())) {
        existing << name;
      }
    }
    if (!existing.isEmpty() &&
        sheetQuestion(this, tr("Save as Mesh"),
                      tr("%1 in %2 exist. Replace them?")
                          .arg(existing.join(QStringLiteral(", ")), QDir::toNativeSeparators(folder))) !=
            QMessageBox::Yes) {
      return;
    }
    rememberDialogFolder(DialogGroup::Export, paths.first());
  } else {
    const QString filter = extension == QStringLiteral("3mf")   ? tr("3MF files (*.3mf)")
                           : extension == QStringLiteral("stl") ? tr("STL files (*.stl)")
                                                                : tr("OBJ files (*.obj)");
    QString path = dialogSaveFileName(this, tr("Save Mesh"), QDir(dialogFolder(DialogGroup::Export)).filePath(names.first()),
                                      filter);
    if (path.isEmpty()) {
      return;
    }
    if (QFileInfo(path).suffix().isEmpty()) {
      path += QLatin1Char('.') + extension;
    }
    rememberDialogFolder(DialogGroup::Export, path);
    paths << path;
  }

  // The colours the view shows (3MF, OBJ).
  const QJsonObject colors = options->format == QStringLiteral("stl") ? QJsonObject() : appearanceColors(plan.bodies);
  QStringList warnings;
  {
    const WaitCursor wait(QStringLiteral("Save As Mesh"));
    for (int i = 0; i < paths.size(); ++i) {
      const QStringList bodies = options->perBody ? QStringList{plan.bodies[i]} : plan.bodies;
      const QString occurrence = options->perBody ? plan.occurrences[i] : plan.occurrence;
      QJsonObject result;
      if (!runCommand(meshExportCommand(paths[i], bodies, occurrence, *options, colors), &result)) {
        sheetWarning(this, tr("Save as Mesh"),
                     tr("Could not write %1:\n%2").arg(QDir::toNativeSeparators(paths[i]), m_lastError));
        return;
      }
      qInfo().noquote() << QStringLiteral("Saved as mesh %1: %2, %3 body(ies)")
                               .arg(paths[i], options->describe())
                               .arg(result.value(QStringLiteral("bodies")).toArray().size());
      if (options->onBed) {
        // How the bodies lie on the print bed (mitcad#731).
        qInfo().noquote() << QStringLiteral("Save As Mesh bed %1: %2")
                                 .arg(QFileInfo(paths[i]).fileName(),
                                      printBedLog(result.value(QStringLiteral("bed")).toObject()));
      }
      for (const QJsonValue& warning : result.value(QStringLiteral("warnings")).toArray()) {
        warnings << mitcad::modelMessage(warning.toString());
      }
    }
  }
  const QString done = paths.size() == 1
                           ? tr("Saved %1.").arg(QDir::toNativeSeparators(paths.first()))
                           : tr("Saved %1 files in %2.")
                                 .arg(paths.size())
                                 .arg(QDir::toNativeSeparators(QFileInfo(paths.first()).absolutePath()));
  if (warnings.isEmpty()) {
    showHint(done);
  } else {
    showWarning(done + QLatin1Char('\n') + warnings.join(QLatin1Char('\n')));
  }
}

} // namespace mitcad
