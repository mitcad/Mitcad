// SPDX-License-Identifier: MIT
#include "FileDialogs.hpp"
#include "FileFormats.hpp"
#include "framework/ModelMessages.hpp"

#include <algorithm>
#include <cmath>
#include <initializer_list>
#include <utility>
#include <vector>

#include <QComboBox>
#include <QCoreApplication>
#include <QDialog>
#include <QButtonGroup>
#include <QCheckBox>
#include <QDialogButtonBox>
#include <QDir>
#include <QDoubleSpinBox>
#include <QFileDialog>
#include <QFileInfo>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QJsonArray>
#include <QLabel>
#include <QLineEdit>
#include <QListWidget>
#include <QMessageBox>
#include <QPushButton>
#include <QRadioButton>
#include <QRectF>
#include <QSettings>
#include <QTextDocumentFragment>
#include <QTreeWidget>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "../framework/NoticeLevel.hpp"
#include "../framework/Numbers.hpp"
#include "../framework/PanelTree.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"
#include "../print/PrintKeys.hpp"
#include "IamImport.hpp"

#include "../platform/MacDialogs.hpp"

namespace mitcad {
namespace {

struct ExportType {
  const char* format;
  const char* schema;
  const char* suffix;
  const char* label;
};

const ExportType kTypes[] = {
    {"step", "ap214", "step", QT_TRANSLATE_NOOP("Export", "STEP AP214 (*.step)")},
    {"step", "ap242", "step", QT_TRANSLATE_NOOP("Export", "STEP AP242 (*.step)")},
    {"iges", "", "igs", QT_TRANSLATE_NOOP("Export", "IGES (*.igs)")},
    {"stl", "", "stl", QT_TRANSLATE_NOOP("Export", "STL mesh (*.stl)")},
    {"obj", "", "obj", QT_TRANSLATE_NOOP("Export", "OBJ mesh (*.obj)")},
    {"brep", "", "brep", QT_TRANSLATE_NOOP("Export", "OCCT BRep (*.brep)")},
    {"dxf", "", "dxf", QT_TRANSLATE_NOOP("Export", "Sketch to DXF (*.dxf)")},
    // For slicers: the bodies as the parts of one object (mitcad#13).
    {"3mf", "", "3mf", QT_TRANSLATE_NOOP("Export", "3MF for 3D printing (*.3mf)")},
    // For AR and 3D viewers: shaded meshes with appearances (mitcad#123).
    {"usdz", "", "usdz", QT_TRANSLATE_NOOP("Export", "USDZ for AR and 3D viewers (*.usdz)")},
    // One self-contained page with the web viewer and the design (mitcad#423).
    {"html", "", "html", QT_TRANSLATE_NOOP("Export", "Web page (*.html)")},
    // glTF 2.0 for the web, catalogues and sharing (mitcad#1826): the web
    // page's file of its own, binary and self-contained or as JSON with
    // its buffer and images beside it.
    {"glb", "", "glb", QT_TRANSLATE_NOOP("Export", "glTF binary (*.glb)")},
    {"gltf", "", "gltf", QT_TRANSLATE_NOOP("Export", "glTF (*.gltf)")},
};

constexpr int kTypeCount = static_cast<int>(sizeof(kTypes) / sizeof(kTypes[0]));
constexpr char kExportGroup[] = "export";
const QStringList kRefinements{QStringLiteral("low"), QStringLiteral("medium"), QStringLiteral("high")};

bool isMeshType(const QString& format) {
  return format == QStringLiteral("stl") || format == QStringLiteral("obj") || format == QStringLiteral("3mf") ||
         format == QStringLiteral("usdz") || format == QStringLiteral("html") || format == QStringLiteral("glb") ||
         format == QStringLiteral("gltf");
}

// The type a file's extension asks for (the first of a format), or -1.
int typeOfSuffix(const QString& suffix, int current) {
  const QString s = suffix.toLower();
  const auto is = [&](std::initializer_list<const char*> suffixes) {
    for (const char* candidate : suffixes) {
      if (s == QLatin1String(candidate)) {
        return true;
      }
    }
    return false;
  };
  if (is({"step", "stp"})) {
    return current == 1 ? 1 : 0;
  }
  if (is({"iges", "igs"})) {
    return 2;
  }
  if (is({"stl"})) {
    return 3;
  }
  if (is({"obj"})) {
    return 4;
  }
  if (is({"brep", "brp"})) {
    return 5;
  }
  if (is({"dxf"})) {
    return 6;
  }
  if (is({"3mf"})) {
    return 7;
  }
  if (is({"usdz"})) {
    return 8;
  }
  if (is({"html", "htm"})) {
    return 9;
  }
  if (is({"glb"})) {
    return 10;
  }
  if (is({"gltf"})) {
    return 11;
  }
  return -1;
}

// The units a dialog offers, metric first; the first (millimetres) is the
// default everywhere (P11: inches only by choice).
const QVector<QPair<QString, double>> kUnits = {
    {QStringLiteral("mm"), 1.0}, {QStringLiteral("cm"), 10.0}, {QStringLiteral("m"), 1000.0},
    {QStringLiteral("in"), 25.4}, {QStringLiteral("ft"), 304.8}};

QComboBox* unitBox() {
  auto* box = new QComboBox;
  for (const auto& [name, mm] : kUnits) {
    box->addItem(name);
  }
  box->setCurrentIndex(0);
  return box;
}

// "mm (mm | cm | m | in | ft)": a unit box's choice and offer, for the log.
QString unitOffer(const QComboBox* box) {
  QStringList names;
  for (int i = 0; i < box->count(); ++i) {
    names << box->itemText(i);
  }
  return QStringLiteral("%1 (%2)").arg(box->currentText(), names.join(QStringLiteral(" | ")));
}

QDialogButtonBox* okCancel(QDialog& dialog) {
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  return buttons;
}

// The files of a 3MF file per plate that exist already (mitcad#1679): each
// plate's `<name> - <plate name>.3mf` beside `path`, its characters that a
// file name cannot have on every system as `_`, as the model's export
// (`exchange/plates.rs`) names them.
QStringList existingPlateFiles(const QString& path, const QVector<QPair<QString, QString>>& plates) {
  const QFileInfo info(path);
  QStringList existing;
  for (const auto& entry : plates) {
    QString part;
    for (const QChar c : entry.second.trimmed()) {
      const bool kept = c.isLetterOrNumber() || c == QLatin1Char(' ') || c == QLatin1Char('-') ||
                        c == QLatin1Char('_') || c == QLatin1Char('(') || c == QLatin1Char(')') ||
                        c == QLatin1Char('.');
      part += kept ? c : QChar(u'_');
    }
    while (part.startsWith(QLatin1Char('.'))) {
      part.remove(0, 1);
    }
    while (part.endsWith(QLatin1Char('.'))) {
      part.chop(1);
    }
    part = part.trimmed();
    if (part.isEmpty()) {
      part = QStringLiteral("Plate");
    }
    const QString file = info.dir().filePath(info.completeBaseName() + QStringLiteral(" - ") + part +
                                             QStringLiteral(".3mf"));
    if (QFileInfo::exists(file)) {
      existing << QFileInfo(file).fileName();
    }
  }
  return existing;
}

} // namespace

QString exportTypeKey(int index) {
  if (index < 0 || index >= kTypeCount) {
    return QString();
  }
  const ExportType& entry = kTypes[index];
  QString key = QLatin1String(entry.format);
  if (entry.schema[0] != '\0') {
    key += QLatin1Char('-') + QLatin1String(entry.schema);
  }
  return key;
}

QStringList exportDialects() {
  return {QString(),
          QStringLiteral("bambu_studio"),
          QStringLiteral("orcaslicer"),
          QStringLiteral("qidi_studio"),
          QStringLiteral("prusaslicer"),
          QStringLiteral("cura")};
}

QString exportStartPlates(bool hasPlates, const QString& dialect) {
  if (!hasPlates) {
    return QString();
  }
  // Cura has one build plate: the model refuses every plate in one file.
  return dialect == QStringLiteral("cura") ? QStringLiteral("file_per_plate") : QStringLiteral("one_file");
}

int exportStartType(const QString& remembered, bool sketch, bool sketches) {
  int dxf = 0;
  int start = 0;
  for (int i = 0; i < kTypeCount; ++i) {
    if (QLatin1String(kTypes[i].format) == QLatin1String("dxf")) {
      dxf = i;
    }
    if (exportTypeKey(i) == remembered) {
      start = i;
    }
  }
  if (sketch) {
    return dxf;
  }
  return start == dxf && !sketches ? 0 : start;
}

std::optional<ExportChoice> askExport(QWidget* parent, const QString& stem, int selectedBodies,
                                      const QVector<QPair<QString, QString>>& sketches,
                                      const QString& sketch, bool occurrences, const QString& designUnit,
                                      const ExportPlates& plates, const QVector<QPair<QString, QString>>& frames) {
  // The last export's choices (mitcad#1164).
  QSettings settings;
  settings.beginGroup(QLatin1String(kExportGroup));
  const int start = exportStartType(settings.value(QStringLiteral("type")).toString(), !sketch.isEmpty(),
                                    !sketches.isEmpty());
  const int refined = std::max(0, static_cast<int>(kRefinements.indexOf(
                                      settings.value(QStringLiteral("refinement"), QStringLiteral("medium")).toString())));
  const bool stlText = settings.value(QStringLiteral("stlText"), false).toBool();
  const bool dxfConstruction = settings.value(QStringLiteral("dxfConstruction"), false).toBool();
  const bool dxfPoints = settings.value(QStringLiteral("dxfPoints"), true).toBool();
  settings.endGroup();
  const QString path = stem + QLatin1Char('.') + QLatin1String(kTypes[start].suffix);

  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Export"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* form = new QFormLayout;
  layout->addLayout(form);

  auto* file = new QLineEdit(path);
  auto* browse = new QPushButton(QObject::tr("Browse…"));
  mac::auxiliaryButton(browse);
  auto* fileRow = new QHBoxLayout;
  fileRow->addWidget(file, 1);
  fileRow->addWidget(browse);
  form->addRow(QObject::tr("File:"), fileRow);

  auto* type = new QComboBox;
  for (const ExportType& entry : kTypes) {
    type->addItem(QCoreApplication::translate("Export", entry.label));
  }
  form->addRow(QObject::tr("&Type:"), type);
  auto* bodies = new QComboBox;
  bodies->addItem(QObject::tr("All bodies"));
  if (selectedBodies > 0) {
    bodies->addItem(QObject::tr("Selected bodies (%1)").arg(selectedBodies));
    bodies->setCurrentIndex(1);
  }
  form->addRow(QObject::tr("Bodies:"), bodies);
  // Where the bodies go (mitcad#19): as the design shows them, or each in
  // its component's coordinates; or in a coordinate system's frame
  // (mitcad#1775).
  auto* coordinates = new QComboBox;
  coordinates->setObjectName(QStringLiteral("exportCoordinates"));
  coordinates->addItem(QObject::tr("Where the design shows them"), QStringLiteral("design"));
  if (occurrences) {
    coordinates->addItem(QObject::tr("Each in its component's coordinates"), QStringLiteral("component"));
  }
  for (const auto& [frameUid, frameName] : frames) {
    coordinates->addItem(QObject::tr("In %1").arg(frameName), QStringLiteral("frame:") + frameUid);
  }
  coordinates->setToolTip(QObject::tr(
      "Where the design shows bodies, each in its own component's coordinates, or in a coordinate system"));
  form->addRow(QObject::tr("Coor&dinates:"), coordinates);
  const bool chooseCoordinates = occurrences || !frames.isEmpty();
  auto* unit = unitBox();
  unit->setToolTip(QObject::tr("The length unit of the file; the bodies keep their size"));
  form->addRow(QObject::tr("Unit:"), unit);
  auto* refinement = new QComboBox;
  refinement->addItems({QObject::tr("Low"), QObject::tr("Medium"), QObject::tr("High")});
  refinement->setCurrentIndex(refined);
  form->addRow(QObject::tr("&Refinement:"), refinement);
  auto* stlFormat = new QComboBox;
  stlFormat->addItems({QObject::tr("Binary"), QObject::tr("Text (ASCII)")});
  stlFormat->setCurrentIndex(stlText ? 1 : 0);
  form->addRow(QObject::tr("STL format:"), stlFormat);
  auto* sketchBox = new QComboBox;
  int current = 0;
  for (const auto& [uid, name] : sketches) {
    if (uid == sketch) {
      current = sketchBox->count();
    }
    sketchBox->addItem(name, uid);
  }
  sketchBox->setCurrentIndex(current);
  form->addRow(QObject::tr("Sketch:"), sketchBox);
  // A DXF file's length unit (mitcad#1408): the design's first.
  auto* dxfUnit = new QComboBox;
  dxfUnit->addItems(dxfUnits(designUnit));
  dxfUnit->setCurrentIndex(0);
  dxfUnit->setToolTip(QObject::tr("The length unit of the DXF file; the sketch keeps its size"));
  form->addRow(QObject::tr("DXF unit:"), dxfUnit);
  // Construction geometry (mitcad#1445): left out unless chosen, so that a
  // CAM program cuts only the sketch's real curves.
  auto* construction = new QCheckBox(QObject::tr("Include construction geometry"));
  construction->setChecked(dxfConstruction);
  construction->setToolTip(
      QObject::tr("Also write the sketch's construction curves and center lines, on the layer construction"));
  form->addRow(QString(), construction);
  // The sketch's free-standing points (mitcad#1963): marks a cutting table
  // or a drill takes as hole positions; on unless unchecked.
  auto* pointsBox = new QCheckBox(QObject::tr("Include points"));
  pointsBox->setObjectName(QStringLiteral("exportDxfPoints"));
  pointsBox->setChecked(dxfPoints);
  pointsBox->setToolTip(QObject::tr("Also write the sketch's points that stand alone, such as hole marks, "
                                    "on the layer POINTS"));
  form->addRow(QString(), pointsBox);
  // A 3MF file of the design's print plates (mitcad#1679), as Send to
  // Slicer writes them (mitcad#1050): every plate in one file, a file per
  // plate, one plate, or the bodies without plates.
  const bool hasPlates = !plates.plates.isEmpty();
  const QString startPlates = exportStartPlates(hasPlates, plates.dialect);
  auto* plateBox = new QComboBox;
  plateBox->setObjectName(QStringLiteral("exportPlates"));
  plateBox->addItem(QObject::tr("The bodies, without plates"), QString());
  plateBox->addItem(QObject::tr("All plates in one file"), QStringLiteral("one_file"));
  plateBox->addItem(QObject::tr("A file per plate"), QStringLiteral("file_per_plate"));
  for (const auto& [plateId, plateName] : plates.plates) {
    plateBox->addItem(QObject::tr("Only %1").arg(plateName), QStringLiteral("plate:") + plateId);
  }
  plateBox->setCurrentIndex(std::max(0, plateBox->findData(startPlates)));
  plateBox->setToolTip(QObject::tr("The design's print plates, laid out on the bed as Send to Slicer writes them"));
  form->addRow(QObject::tr("&Plates:"), plateBox);
  // The slicer whose print settings go with the bodies (the model's
  // `dialect`): the design's when it has plates, else none.
  auto* slicerBox = new QComboBox;
  slicerBox->setObjectName(QStringLiteral("exportSlicer"));
  for (const QString& dialect : exportDialects()) {
    slicerBox->addItem(dialect.isEmpty() ? QObject::tr("None") : slicerTitle(dialect), dialect);
  }
  slicerBox->setCurrentIndex(std::max(0, slicerBox->findData(plates.dialect)));
  slicerBox->setToolTip(QObject::tr("The slicer whose print settings go with the bodies"));
  form->addRow(QObject::tr("S&licer:"), slicerBox);
  // On the print bed (the model's `on_bed`); the plates always lie on it.
  auto* onBed = new QCheckBox(QObject::tr("On the print bed"));
  onBed->setObjectName(QStringLiteral("exportOnBed"));
  onBed->setToolTip(QObject::tr("Each piece on its print bed face, else on the face it lies best on"));
  form->addRow(QString(), onBed);
  auto* centerBox = new QCheckBox(QObject::tr("Center on Plate"));
  centerBox->setObjectName(QStringLiteral("exportCenter"));
  centerBox->setChecked(true);
  centerBox->setToolTip(QObject::tr("The middle of each piece over the middle of the plate"));
  form->addRow(QString(), centerBox);
  auto* dropBox = new QCheckBox(QObject::tr("Drop to Bed"));
  dropBox->setObjectName(QStringLiteral("exportDrop"));
  dropBox->setChecked(true);
  dropBox->setToolTip(QObject::tr("The lowest point of each piece onto the bed"));
  form->addRow(QString(), dropBox);
  auto* message = new QLabel;
  setErrorStyleSheet(message);
  layout->addWidget(message);
  layout->addWidget(okCancel(dialog));

  // The rows a type uses; the file's extension follows the type and the
  // type the extension typed.
  const auto showRows = [=] {
    const QString format = QLatin1String(kTypes[type->currentIndex()].format);
    const bool mesh = isMeshType(format);
    // A web page shows the design as the view does: every visible body
    // where the design places it.
    const bool page = format == QStringLiteral("html");
    // A glTF file keeps the component tree with its placements: the design's
    // coordinates (mitcad#1826).
    const bool gltf = format == QStringLiteral("glb") || format == QStringLiteral("gltf");
    // A 3MF file of the plates prints the whole design on the bed
    // (mitcad#1679); on the bed the bodies are where the design shows them.
    const bool threeMf = format == QStringLiteral("3mf");
    const bool platesChosen = threeMf && hasPlates && !plateBox->currentData().toString().isEmpty();
    const bool bed = platesChosen || (threeMf && onBed->isChecked());
    form->setRowVisible(bodies, format != QStringLiteral("dxf") && !page && !platesChosen);
    form->setRowVisible(coordinates, chooseCoordinates && format != QStringLiteral("dxf") && !page && !gltf && !bed);
    form->setRowVisible(plateBox, threeMf && hasPlates);
    form->setRowVisible(slicerBox, threeMf);
    form->setRowVisible(onBed, threeMf && !platesChosen);
    form->setRowVisible(centerBox, threeMf);
    form->setRowVisible(dropBox, threeMf);
    // Centred with a slicer too (mitcad#1596); dropped on the bed only.
    centerBox->setEnabled(bed || !slicerBox->currentData().toString().isEmpty());
    dropBox->setEnabled(bed);
    form->setRowVisible(unit, format == QStringLiteral("step") || format == QStringLiteral("iges"));
    form->setRowVisible(refinement, mesh);
    form->setRowVisible(stlFormat, format == QStringLiteral("stl"));
    form->setRowVisible(sketchBox, format == QStringLiteral("dxf"));
    form->setRowVisible(dxfUnit, format == QStringLiteral("dxf"));
    form->setRowVisible(construction, format == QStringLiteral("dxf"));
    form->setRowVisible(pointsBox, format == QStringLiteral("dxf"));
  };
  QObject::connect(type, &QComboBox::currentIndexChanged, &dialog, [=](int index) {
    QFileInfo info(file->text());
    if (typeOfSuffix(info.suffix(), index) != index && !file->text().isEmpty()) {
      const QString base = info.suffix().isEmpty() ? file->text() : file->text().chopped(info.suffix().size() + 1);
      const QSignalBlocker blocker(file);
      file->setText(base + QLatin1Char('.') + QLatin1String(kTypes[index].suffix));
    }
    showRows();
  });
  QObject::connect(file, &QLineEdit::textChanged, &dialog, [=](const QString& text) {
    const int index = typeOfSuffix(QFileInfo(text).suffix(), type->currentIndex());
    if (index >= 0 && index != type->currentIndex()) {
      const QSignalBlocker blocker(type);
      type->setCurrentIndex(index);
      showRows();
    }
  });
  QObject::connect(plateBox, &QComboBox::currentIndexChanged, &dialog, [=](int) { showRows(); });
  QObject::connect(onBed, &QCheckBox::toggled, &dialog, [=](bool) { showRows(); });
  QObject::connect(slicerBox, &QComboBox::currentIndexChanged, &dialog, [=](int) {
    // Cura has one build plate: a file per plate instead of one file.
    if (slicerBox->currentData().toString() == QStringLiteral("cura") &&
        plateBox->currentData().toString() == QStringLiteral("one_file")) {
      plateBox->setCurrentIndex(plateBox->findData(QStringLiteral("file_per_plate")));
    }
    showRows();
  });
  QObject::connect(browse, &QPushButton::clicked, &dialog, [&dialog, file] {
    const QString chosen = dialogSaveFileName(&dialog, QObject::tr("Export"), file->text());
    if (!chosen.isEmpty()) {
      file->setText(chosen);
    }
  });
  type->setCurrentIndex(start);
  showRows();
  file->setFocus();
  file->selectAll();
  dialog.resize(480, dialog.sizeHint().height());
  // What Coordinates offers, for UI tests: "design | component" and the
  // coordinate systems by name (mitcad#1775).
  QStringList offered{QStringLiteral("design")};
  if (occurrences) {
    offered << QStringLiteral("component");
  }
  for (const auto& frameEntry : frames) {
    offered << frameEntry.second;
  }
  const QString coordinatesOffer =
      chooseCoordinates ? QStringLiteral(", coordinates design (%1)").arg(offered.join(QStringLiteral(" | ")))
                        : QString();
  qDebug().noquote() << QStringLiteral(
                            "Export dialog opened: unit %1%2, type %3, refinement %4, file %5, dxf unit %6, "
                            "dxf construction %7, plates %8, slicer %9")
                            .arg(unitOffer(unit),
                                 coordinatesOffer,
                                 exportTypeKey(start), kRefinements[refined], path, unitOffer(dxfUnit),
                                 QStringLiteral("%1, dxf points %2")
                                     .arg(dxfConstruction ? QStringLiteral("on") : QStringLiteral("off"),
                                          dxfPoints ? QStringLiteral("on") : QStringLiteral("off")),
                                 hasPlates ? QStringLiteral("%1 (%2)").arg(startPlates).arg(plates.plates.size())
                                           : QStringLiteral("none"),
                                 plates.dialect.isEmpty() ? QStringLiteral("none") : plates.dialect);

  prepareModal(&dialog);
  while (execDialog(&dialog) == QDialog::Accepted) {
    const ExportType& chosen = kTypes[type->currentIndex()];
    ExportChoice choice;
    choice.path = file->text().trimmed();
    choice.format = QLatin1String(chosen.format);
    choice.schema = QLatin1String(chosen.schema);
    choice.unit = unit->currentText();
    choice.refinement = kRefinements[refinement->currentIndex()];
    choice.ascii = stlFormat->currentIndex() == 1;
    choice.selectedOnly = bodies->currentIndex() == 1;
    choice.sketch = sketchBox->currentData().toString();
    choice.dxfUnit = dxfUnit->currentText();
    choice.dxfConstruction = construction->isChecked();
    choice.dxfPoints = pointsBox->isChecked();
    if (chooseCoordinates) {
      choice.coordinates = coordinates->currentData().toString();
      // In a coordinate system's frame (mitcad#1775).
      if (choice.coordinates.startsWith(QStringLiteral("frame:"))) {
        choice.frame = choice.coordinates.mid(6);
        choice.coordinates = QStringLiteral("frame");
      } else if (!occurrences && choice.coordinates == QStringLiteral("design")) {
        choice.coordinates.clear(); // as before: nothing to choose by
      }
    }
    if (choice.format == QStringLiteral("3mf")) {
      // The design's print plates and the bed (mitcad#1679).
      const QString mode = hasPlates ? plateBox->currentData().toString() : QString();
      if (mode.startsWith(QStringLiteral("plate:"))) {
        choice.plates = QStringLiteral("plate");
        choice.plate = mode.mid(6);
      } else {
        choice.plates = mode;
      }
      choice.dialect = slicerBox->currentData().toString();
      choice.onBed = !choice.plates.isEmpty() || onBed->isChecked();
      choice.center = centerBox->isChecked();
      choice.drop = dropBox->isChecked();
      if (!choice.plates.isEmpty()) {
        choice.selectedOnly = false;
      }
      if (choice.onBed) {
        // On the bed the bodies are where the design shows them.
        choice.coordinates.clear();
      }
      if (choice.plates == QStringLiteral("one_file") && choice.dialect == QStringLiteral("cura")) {
        message->setText(QObject::tr("UltiMaker Cura has one build plate: choose a file per plate."));
        continue;
      }
    }
    if (choice.coordinates == QStringLiteral("frame") && choice.format == QStringLiteral("usdz")) {
      message->setText(
          QObject::tr("A USDZ file keeps the design's coordinates: choose them, or another type of file."));
      continue;
    }
    if (choice.path.isEmpty()) {
      message->setText(QObject::tr("Give the file to write."));
      continue;
    }
    if (QFileInfo(choice.path).suffix().isEmpty()) {
      choice.path += QLatin1Char('.') + QLatin1String(chosen.suffix);
    }
    if (choice.format == QStringLiteral("dxf") && choice.sketch.isEmpty()) {
      message->setText(QObject::tr("The design has no sketch to export."));
      continue;
    }
    if (choice.plates == QStringLiteral("file_per_plate")) {
      // The plates' files beside the path (mitcad#1679), named as the
      // export names them.
      const QStringList existing = existingPlateFiles(choice.path, plates.plates);
      if (!existing.isEmpty() &&
          sheetQuestion(&dialog, QObject::tr("Export"),
                        QObject::tr("%1 exist. Replace them?").arg(existing.join(QStringLiteral(", ")))) !=
              QMessageBox::Yes) {
        continue;
      }
    } else if (QFileInfo::exists(choice.path) &&
               sheetQuestion(&dialog, QObject::tr("Export"),
                             QObject::tr("%1 exists. Replace it?").arg(QFileInfo(choice.path).fileName())) !=
                   QMessageBox::Yes) {
      continue;
    }
    qDebug().noquote() << QStringLiteral("Export dialog: %1 to %2%3")
                              .arg(QCoreApplication::translate("Export", chosen.label), choice.path,
                                   (isMeshType(choice.format)
                                        ? QStringLiteral(", refinement %1").arg(choice.refinement)
                                        : QString()) +
                                       (choice.format == QStringLiteral("3mf")
                                            ? QStringLiteral(", plates %1, slicer %2%3")
                                                  .arg(choice.plates.isEmpty() ? QStringLiteral("none")
                                                       : choice.plates == QStringLiteral("plate")
                                                           ? QStringLiteral("plate ") + choice.plate
                                                           : choice.plates,
                                                       choice.dialect.isEmpty() ? QStringLiteral("none")
                                                                                : choice.dialect,
                                                       choice.onBed ? QStringLiteral(", on the bed") : QString())
                                            : QString()));
    // Remembered for the next export (mitcad#1164); a sketch's DXF keeps
    // the type the bodies were exported as.
    settings.beginGroup(QLatin1String(kExportGroup));
    if (choice.format != QStringLiteral("dxf")) {
      settings.setValue(QStringLiteral("type"), exportTypeKey(type->currentIndex()));
    }
    settings.setValue(QStringLiteral("refinement"), choice.refinement);
    settings.setValue(QStringLiteral("stlText"), choice.ascii);
    settings.setValue(QStringLiteral("dxfConstruction"), choice.dxfConstruction);
    settings.setValue(QStringLiteral("dxfPoints"), choice.dxfPoints);
    settings.endGroup();
    settings.sync();
    return choice;
  }
  return std::nullopt;
}

std::optional<DxfChoice> askDxfInsert(QWidget* parent, const QString& file,
                                      const QVector<QPair<QString, QString>>& planes, const QJsonObject& info) {
  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Insert DXF"));
  auto* layout = new QVBoxLayout(&dialog);
  const QString name = QFileInfo(file).fileName();
  layout->addWidget(new QLabel(planes.isEmpty() ? QObject::tr("%1 goes into the edited sketch.").arg(name)
                                                : QObject::tr("%1 goes into a new sketch.").arg(name)));
  auto* form = new QFormLayout;
  layout->addLayout(form);
  auto* plane = new QComboBox;
  for (const auto& [value, label] : planes) {
    plane->addItem(label, value);
  }
  form->addRow(QObject::tr("Plane:"), plane);
  form->setRowVisible(plane, !planes.isEmpty());
  // The drawing's own unit, else the one chosen (millimetres first, P11).
  const QJsonValue ownUnit = info.value(QStringLiteral("unit"));
  auto* unit = unitBox();
  form->addRow(QObject::tr("Unit of a drawing without one:"), unit);
  if (ownUnit.isString()) {
    unit->setEnabled(false);
    unit->setToolTip(QObject::tr("The drawing gives its unit: %1").arg(ownUnit.toString()));
    form->addRow(QObject::tr("Drawing unit:"), new QLabel(ownUnit.toString()));
  }
  const auto millimetres = [&info, unit] {
    const QJsonValue own = info.value(QStringLiteral("unit_mm"));
    return own.isDouble() ? own.toDouble() : kUnits[unit->currentIndex()].second;
  };

  // P9: where the drawing goes, by its origin, its lower left corner or
  // its centre, and which of its layers.
  auto* anchors = new QWidget;
  auto* anchorRow = new QHBoxLayout(anchors);
  anchorRow->setContentsMargins(0, 0, 0, 0);
  auto* group = new QButtonGroup(anchors);
  const std::pair<const char*, QString> kAnchors[] = {{"origin", QObject::tr("Drawing origin")},
                                                     {"lower_left", QObject::tr("Lower left corner")},
                                                     {"center", QObject::tr("Center")}};
  std::vector<QRadioButton*> anchorButtons;
  for (const auto& [id, label] : kAnchors) {
    auto* button = new QRadioButton(label);
    button->setObjectName(QString::fromLatin1(id));
    group->addButton(button);
    anchorRow->addWidget(button);
    anchorButtons.push_back(button);
  }
  anchorButtons.front()->setChecked(true);
  anchors->setToolTip(QObject::tr("The point of the drawing that goes to X, Y in the sketch"));
  form->addRow(QObject::tr("Place:"), anchors);
  const auto coordinate = [] {
    auto* box = new DecimalSpinBox;
    box->setRange(-1e6, 1e6);
    box->setDecimals(3);
    box->setSuffix(QStringLiteral(" mm"));
    return box;
  };
  auto* x = coordinate();
  auto* y = coordinate();
  form->addRow(QObject::tr("At X:"), x);
  form->addRow(QObject::tr("At Y:"), y);
  auto* layers = new QListWidget;
  layers->setToolTip(QObject::tr("Entities on layers that are not checked are left out"));
  QStringList offered;
  for (const QJsonValue& value : info.value(QStringLiteral("layers")).toArray()) {
    const QJsonObject layer = value.toObject();
    const bool shown = layer.value(QStringLiteral("visible")).toBool(true);
    auto* item = new QListWidgetItem(QStringLiteral("%1 (%2)")
                                         .arg(layer.value(QStringLiteral("name")).toString())
                                         .arg(layer.value(QStringLiteral("entities")).toInt()));
    item->setData(Qt::UserRole, layer.value(QStringLiteral("name")).toString());
    item->setData(Qt::UserRole + 1, layer.value(QStringLiteral("bounds")).toObject());
    item->setFlags(item->flags() | Qt::ItemIsUserCheckable);
    item->setCheckState(shown ? Qt::Checked : Qt::Unchecked);
    layers->addItem(item);
    offered << item->text() + (shown ? QString() : QStringLiteral(" off"));
  }
  layers->setMaximumHeight(120);
  form->addRow(QObject::tr("Layers:"), layers);
  auto* size = new QLabel;
  form->addRow(QObject::tr("Size:"), size);
  auto* message = new QLabel;
  setErrorStyleSheet(message);
  layout->addWidget(message);
  auto* buttons = okCancel(dialog);
  layout->addWidget(buttons);

  // The checked layers' box in millimetres.
  const auto box = [layers, millimetres]() -> std::optional<QRectF> {
    std::optional<QRectF> all;
    for (int i = 0; i < layers->count(); ++i) {
      const QListWidgetItem* item = layers->item(i);
      const QJsonObject bounds = item->data(Qt::UserRole + 1).toJsonObject();
      if (item->checkState() != Qt::Checked || bounds.isEmpty()) {
        continue;
      }
      const QJsonArray lo = bounds.value(QStringLiteral("min")).toArray();
      const QJsonArray hi = bounds.value(QStringLiteral("max")).toArray();
      const QRectF r(QPointF(lo.at(0).toDouble(), lo.at(1).toDouble()),
                     QPointF(hi.at(0).toDouble(), hi.at(1).toDouble()));
      all = all ? all->united(r) : r;
    }
    if (all) {
      const double k = millimetres();
      all = QRectF(all->topLeft() * k, all->bottomRight() * k);
    }
    return all;
  };
  const auto updateSize = [box, size] {
    const auto b = box();
    size->setText(b ? QObject::tr("%1 × %2 mm").arg(b->width(), 0, 'g', 6).arg(b->height(), 0, 'g', 6)
                    : QObject::tr("nothing"));
  };
  QObject::connect(layers, &QListWidget::itemChanged, &dialog, [updateSize] { updateSize(); });
  QObject::connect(unit, &QComboBox::currentIndexChanged, &dialog, [updateSize] { updateSize(); });
  updateSize();
  plane->setFocus();
  qDebug().noquote() << QStringLiteral("Insert DXF dialog opened: unit %1").arg(unitOffer(unit));
  qDebug().noquote() << QStringLiteral("Insert DXF drawing: unit %1, layers %2")
                            .arg(ownUnit.isString() ? ownUnit.toString() : QStringLiteral("none"),
                                 offered.join(QStringLiteral(" | ")));
  // Where the inputs are, in the main window's coordinates (UI tests).
  TestSync::singleShot(200, &dialog, [anchorButtons, x, y, layers, buttons, parent] {
    QCoreApplication::sendPostedEvents(nullptr, QEvent::LayoutRequest);
    const QPoint origin = parent != nullptr ? parent->window()->mapToGlobal(QPoint(0, 0)) : QPoint();
    const auto at = [&origin](const QWidget* widget, const QPoint& point) {
      const QPoint global = widget->mapToGlobal(point) - origin;
      return QStringLiteral("%1,%2").arg(global.x()).arg(global.y());
    };
    for (const QRadioButton* button : anchorButtons) {
      qDebug().noquote() << QStringLiteral("Insert DXF place %1 at %2")
                                .arg(button->objectName(), at(button, button->rect().center()));
    }
    for (const auto& [label, widget] : {std::pair<const char*, QWidget*>{"x", x},
                                        std::pair<const char*, QWidget*>{"y", y}}) {
      qDebug().noquote() << QStringLiteral("Insert DXF %1 at %2").arg(QLatin1String(label), at(widget, widget->rect().center()));
    }
    for (int i = 0; i < layers->count(); ++i) {
      const QRect rect = layers->visualItemRect(layers->item(i));
      qDebug().noquote() << QStringLiteral("Insert DXF layer %1 at %2")
                                .arg(layers->item(i)->data(Qt::UserRole).toString(),
                                     at(layers->viewport(), rect.center()));
    }
    if (QPushButton* ok = buttons->button(QDialogButtonBox::Ok)) {
      qDebug().noquote() << QStringLiteral("Insert DXF OK at %1").arg(at(ok, ok->rect().center()));
    }
  });

  prepareModal(&dialog);
  while (execDialog(&dialog) == QDialog::Accepted) {
    DxfChoice choice;
    choice.plane = plane->currentData().toString();
    choice.unit = unit->currentText();
    bool all = true;
    for (int i = 0; i < layers->count(); ++i) {
      if (layers->item(i)->checkState() == Qt::Checked) {
        choice.layers << layers->item(i)->data(Qt::UserRole).toString();
      } else {
        all = false;
      }
    }
    if (choice.layers.isEmpty() && layers->count() > 0) {
      message->setText(QObject::tr("Check a layer to insert."));
      continue;
    }
    if (all) {
      choice.layers.clear();
    }
    // The chosen point of the drawing goes to (X, Y).
    QPointF from(0.0, 0.0);
    const auto b = box();
    const QAbstractButton* anchor = group->checkedButton();
    if (b && anchor->objectName() == QStringLiteral("lower_left")) {
      from = b->topLeft(); // the smaller corner: y grows upwards here
    } else if (b && anchor->objectName() == QStringLiteral("center")) {
      from = b->center();
    }
    choice.x = x->value() - from.x();
    choice.y = y->value() - from.y();
    qDebug().noquote() << QStringLiteral("Insert DXF: %1 at (%2, %3), layers %4")
                              .arg(anchor->text().toLower())
                              .arg(x->value())
                              .arg(y->value())
                              .arg(all ? QStringLiteral("all") : choice.layers.join(QStringLiteral(", ")));
    return choice;
  }
  return std::nullopt;
}

std::optional<DxfChoice> askSvgInsert(QWidget* parent, const QString& file,
                                      const QVector<QPair<QString, QString>>& planes, const QJsonObject& info) {
  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Insert SVG"));
  auto* layout = new QVBoxLayout(&dialog);
  const QString name = QFileInfo(file).fileName();
  layout->addWidget(new QLabel(planes.isEmpty() ? QObject::tr("%1 goes into the edited sketch.").arg(name)
                                                : QObject::tr("%1 goes into a new sketch.").arg(name)));
  auto* form = new QFormLayout;
  layout->addLayout(form);
  auto* plane = new QComboBox;
  for (const auto& [value, label] : planes) {
    plane->addItem(label, value);
  }
  form->addRow(QObject::tr("Plane:"), plane);
  form->setRowVisible(plane, !planes.isEmpty());
  auto* scale = new DecimalSpinBox;
  scale->setRange(1e-6, 1e6);
  scale->setDecimals(4);
  scale->setValue(1.0);
  scale->setToolTip(QObject::tr("Millimeters in the sketch per millimeter of the drawing"));
  form->addRow(QObject::tr("Scale:"), scale);
  const auto coordinate = [] {
    auto* box = new DecimalSpinBox;
    box->setRange(-1e6, 1e6);
    box->setDecimals(3);
    box->setSuffix(QStringLiteral(" mm"));
    return box;
  };
  auto* x = coordinate();
  auto* y = coordinate();
  x->setToolTip(QObject::tr("Where the drawing's lower left corner goes in the sketch"));
  form->addRow(QObject::tr("Lower left at X:"), x);
  form->addRow(QObject::tr("Lower left at Y:"), y);
  auto* bands = new QCheckBox(QObject::tr("Stroked paths as bands of their stroke width"));
  bands->setToolTip(QObject::tr("A stroked path becomes the outline of its stroke, a region to extrude; "
                                "otherwise its middle line"));
  bands->setEnabled(info.value(QStringLiteral("stroked")).toInt() > 0);
  form->addRow(QString(), bands);
  const QJsonArray drawn = info.value(QStringLiteral("size")).toArray();
  auto* size = new QLabel;
  form->addRow(QObject::tr("Size:"), size);
  const auto updateSize = [size, scale, drawn] {
    size->setText(QObject::tr("%1 × %2 mm")
                      .arg(drawn.at(0).toDouble() * scale->value(), 0, 'g', 6)
                      .arg(drawn.at(1).toDouble() * scale->value(), 0, 'g', 6));
  };
  QObject::connect(scale, &QDoubleSpinBox::valueChanged, &dialog, [updateSize] { updateSize(); });
  updateSize();
  form->addRow(QObject::tr("Contents:"),
               new QLabel(QObject::tr("%1 filled shapes, %2 stroked, %3 texts")
                              .arg(info.value(QStringLiteral("filled")).toInt())
                              .arg(info.value(QStringLiteral("stroked")).toInt())
                              .arg(info.value(QStringLiteral("texts")).toInt())));
  auto* buttons = okCancel(dialog);
  layout->addWidget(buttons);
  scale->setFocus();
  scale->selectAll();
  // The log keeps its own form of the size, whatever the label says.
  qDebug().noquote() << QStringLiteral("Insert SVG dialog opened: %1 x %2 mm, %3 filled, %4 stroked, %5 texts")
                            .arg(drawn.at(0).toDouble() * scale->value(), 0, 'g', 6)
                            .arg(drawn.at(1).toDouble() * scale->value(), 0, 'g', 6)
                            .arg(info.value(QStringLiteral("filled")).toInt())
                            .arg(info.value(QStringLiteral("stroked")).toInt())
                            .arg(info.value(QStringLiteral("texts")).toInt());
  // Where the inputs are, in the main window's coordinates (UI tests).
  TestSync::singleShot(200, &dialog, [scale, x, y, bands, buttons, parent] {
    QCoreApplication::sendPostedEvents(nullptr, QEvent::LayoutRequest);
    const QPoint origin = parent != nullptr ? parent->window()->mapToGlobal(QPoint(0, 0)) : QPoint();
    const auto at = [&origin](const QWidget* widget) {
      const QPoint global = widget->mapToGlobal(widget->rect().center()) - origin;
      return QStringLiteral("%1,%2").arg(global.x()).arg(global.y());
    };
    for (const auto& [label, widget] :
         {std::pair<const char*, QWidget*>{"scale", scale}, std::pair<const char*, QWidget*>{"x", x},
          std::pair<const char*, QWidget*>{"y", y}, std::pair<const char*, QWidget*>{"bands", bands}}) {
      qDebug().noquote() << QStringLiteral("Insert SVG %1 at %2").arg(QLatin1String(label), at(widget));
    }
    if (QPushButton* ok = buttons->button(QDialogButtonBox::Ok)) {
      qDebug().noquote() << QStringLiteral("Insert SVG OK at %1").arg(at(ok));
    }
  });

  prepareModal(&dialog);
  if (execDialog(&dialog) != QDialog::Accepted) {
    return std::nullopt;
  }
  DxfChoice choice;
  choice.plane = plane->currentData().toString();
  choice.scale = scale->value();
  choice.x = x->value();
  choice.y = y->value();
  choice.strokeOutline = bands->isChecked();
  qDebug().noquote() << QStringLiteral("Insert SVG: scale %1, lower left at (%2, %3), bands %4")
                            .arg(choice.scale)
                            .arg(choice.x)
                            .arg(choice.y)
                            .arg(choice.strokeOutline ? QStringLiteral("on") : QStringLiteral("off"));
  return choice;
}

std::optional<PointsChoice> askPointsInsert(QWidget* parent, const QString& file,
                                            const QVector<QPair<QString, QString>>& planes,
                                            const QJsonObject& info, const QString& unit,
                                            double unitMillimetres) {
  const double unitMm = unitMillimetres > 0.0 ? unitMillimetres : 1.0;
  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Insert Points"));
  auto* layout = new QVBoxLayout(&dialog);
  const QString name = QFileInfo(file).fileName();
  layout->addWidget(new QLabel(planes.isEmpty() ? QObject::tr("%1 goes into the edited sketch.").arg(name)
                                                : QObject::tr("%1 goes into a new sketch.").arg(name)));
  auto* form = new QFormLayout;
  layout->addLayout(form);
  auto* plane = new QComboBox;
  for (const auto& [value, label] : planes) {
    plane->addItem(label, value);
  }
  form->addRow(QObject::tr("Plane:"), plane);
  form->setRowVisible(plane, !planes.isEmpty());
  // What was read: the points, and an airfoil's name.
  const int count = info.value(QStringLiteral("points")).toInt();
  const bool detected = info.value(QStringLiteral("airfoil")).toBool();
  const QString airfoilName = info.value(QStringLiteral("name")).toString();
  QString read = QObject::tr("%n point(s)", nullptr, count);
  if (detected) {
    read = airfoilName.isEmpty() ? QObject::tr("%1, an airfoil").arg(read)
                                 : QObject::tr("%1, the airfoil %2").arg(read, airfoilName);
  }
  form->addRow(QObject::tr("Read:"), new QLabel(read));
  // A spline or points.
  auto* spline = new QRadioButton(QObject::tr("Spline"));
  spline->setToolTip(QObject::tr("One smooth spline through or near the points"));
  auto* points = new QRadioButton(QObject::tr("Points"));
  points->setToolTip(QObject::tr("One sketch point per point of the file"));
  spline->setChecked(true);
  auto* kind = new QButtonGroup(&dialog);
  kind->addButton(spline);
  kind->addButton(points);
  auto* kindRow = new QHBoxLayout;
  kindRow->addWidget(spline);
  kindRow->addWidget(points);
  kindRow->addStretch(1);
  form->addRow(QObject::tr("Insert as:"), kindRow);
  auto* airfoil = new QCheckBox(QObject::tr("Airfoil"));
  airfoil->setToolTip(QObject::tr("Read the file as an airfoil: its chord along X from the leading edge"));
  airfoil->setChecked(detected);
  airfoil->setEnabled(detected);
  form->addRow(QString(), airfoil);
  auto* scale = new DecimalSpinBox;
  scale->setRange(1e-6, 1e6);
  scale->setDecimals(4);
  scale->setValue(1.0);
  scale->setToolTip(QObject::tr("The file's numbers are in %1, times this").arg(unit));
  form->addRow(QObject::tr("Scale:"), scale);
  const QString suffix = QLatin1Char(' ') + unit;
  auto* chord = new DecimalSpinBox;
  chord->setRange(1e-6 / unitMm, 1e6 / unitMm);
  chord->setDecimals(4);
  chord->setSuffix(suffix);
  chord->setValue(100.0 / unitMm);
  chord->setToolTip(QObject::tr("The length from the leading edge to the trailing edge"));
  form->addRow(QObject::tr("Chord length:"), chord);
  auto* angle = new DecimalSpinBox;
  angle->setRange(-90.0, 90.0);
  angle->setDecimals(3);
  angle->setSuffix(QStringLiteral("°"));
  angle->setValue(0.0);
  angle->setToolTip(QObject::tr("Turned about the leading edge; a positive angle raises it"));
  form->addRow(QObject::tr("Angle of attack:"), angle);
  auto* close = new QCheckBox(QObject::tr("Close trailing edge"));
  close->setToolTip(QObject::tr("A line between the two ends of a blunt trailing edge, so the profile is closed"));
  close->setChecked(true);
  form->addRow(QString(), close);
  const auto coordinate = [unitMm, &suffix] {
    auto* box = new DecimalSpinBox;
    box->setRange(-1e6 / unitMm, 1e6 / unitMm);
    box->setDecimals(4);
    box->setSuffix(suffix);
    return box;
  };
  auto* x = coordinate();
  auto* y = coordinate();
  auto* xLabel = new QLabel;
  auto* yLabel = new QLabel;
  form->addRow(xLabel, x);
  form->addRow(yLabel, y);
  auto* buttons = okCancel(dialog);
  layout->addWidget(buttons);
  // The airfoil's rows in place of Scale, and where its leading edge goes.
  const auto update = [form, airfoil, spline, scale, chord, angle, close, x, xLabel, yLabel] {
    const bool foil = airfoil->isChecked();
    form->setRowVisible(scale, !foil);
    form->setRowVisible(chord, foil);
    form->setRowVisible(angle, foil);
    form->setRowVisible(close, foil);
    close->setEnabled(spline->isChecked());
    xLabel->setText(foil ? QObject::tr("Leading edge at X:") : QObject::tr("Origin at X:"));
    yLabel->setText(foil ? QObject::tr("Leading edge at Y:") : QObject::tr("Origin at Y:"));
    x->setToolTip(foil ? QObject::tr("Where the airfoil's leading edge goes in the sketch")
                       : QObject::tr("Where the file's origin goes in the sketch"));
  };
  QObject::connect(airfoil, &QCheckBox::toggled, &dialog, [update] { update(); });
  QObject::connect(spline, &QRadioButton::toggled, &dialog, [update] { update(); });
  update();
  (detected ? static_cast<QWidget*>(chord) : static_cast<QWidget*>(scale))->setFocus();
  qDebug().noquote() << QStringLiteral("Insert Points dialog opened: %1 points, %2")
                            .arg(count)
                            .arg(detected ? QStringLiteral("airfoil %1 (%2)")
                                                .arg(airfoilName,
                                                     info.value(QStringLiteral("layout")).toString())
                                          : QStringLiteral("not an airfoil"));
  // Where the inputs are, in the main window's coordinates (UI tests).
  TestSync::singleShot(200, &dialog, [spline, points, airfoil, scale, chord, angle, close, x, y, buttons, parent] {
    QCoreApplication::sendPostedEvents(nullptr, QEvent::LayoutRequest);
    const QPoint origin = parent != nullptr ? parent->window()->mapToGlobal(QPoint(0, 0)) : QPoint();
    const auto at = [&origin](const QWidget* widget) {
      const QPoint global = widget->mapToGlobal(widget->rect().center()) - origin;
      return QStringLiteral("%1,%2").arg(global.x()).arg(global.y());
    };
    for (const auto& [label, widget] :
         {std::pair<const char*, QWidget*>{"spline", spline}, std::pair<const char*, QWidget*>{"points", points},
          std::pair<const char*, QWidget*>{"airfoil", airfoil}, std::pair<const char*, QWidget*>{"scale", scale},
          std::pair<const char*, QWidget*>{"chord", chord}, std::pair<const char*, QWidget*>{"angle", angle},
          std::pair<const char*, QWidget*>{"close", close}, std::pair<const char*, QWidget*>{"x", x},
          std::pair<const char*, QWidget*>{"y", y}}) {
      if (widget->isVisible()) {
        qDebug().noquote() << QStringLiteral("Insert Points %1 at %2").arg(QLatin1String(label), at(widget));
      }
    }
    if (QPushButton* ok = buttons->button(QDialogButtonBox::Ok)) {
      qDebug().noquote() << QStringLiteral("Insert Points OK at %1").arg(at(ok));
    }
  });

  prepareModal(&dialog);
  if (execDialog(&dialog) != QDialog::Accepted) {
    return std::nullopt;
  }
  PointsChoice choice;
  choice.plane = plane->currentData().toString();
  choice.asPoints = points->isChecked();
  choice.airfoil = airfoil->isChecked();
  choice.scale = scale->value();
  choice.x = x->value() * unitMm;
  choice.y = y->value() * unitMm;
  choice.chord = chord->value() * unitMm;
  choice.angle = angle->value();
  choice.closeTrailingEdge = close->isChecked();
  const QString where = QStringLiteral("(%1, %2)").arg(x->value()).arg(y->value());
  qDebug().noquote() << (choice.airfoil
                             ? QStringLiteral("Insert Points: %1, airfoil on, chord %2%3, angle %4°, leading edge at %5, "
                                              "trailing edge %6")
                                   .arg(choice.asPoints ? QStringLiteral("points") : QStringLiteral("spline"))
                                   .arg(chord->value())
                                   .arg(suffix)
                                   .arg(choice.angle)
                                   .arg(where, choice.closeTrailingEdge ? QStringLiteral("closed")
                                                                        : QStringLiteral("open"))
                             : QStringLiteral("Insert Points: %1, airfoil off, scale %2, origin at %3")
                                   .arg(choice.asPoints ? QStringLiteral("points") : QStringLiteral("spline"))
                                   .arg(choice.scale)
                                   .arg(where));
  return choice;
}

std::optional<MeshInsertChoice> askMeshInsert(QWidget* parent, const QString& file) {
  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Insert Mesh"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* form = new QFormLayout;
  layout->addLayout(form);
  // A 3MF file says its unit itself (mitcad#2156): nothing to ask.
  QComboBox* unit = meshFileHasUnit(file) ? nullptr : unitBox();
  if (unit != nullptr) {
    form->addRow(QObject::tr("Unit of %1:").arg(QFileInfo(file).fileName()), unit);
  }
  // Convert Mesh's Faceted Solid on the inserted bodies (mitcad#1082), with
  // its panel's tolerances; the choice is remembered.
  QSettings settings;
  auto* convert = new QCheckBox(QObject::tr("&Convert to solid"));
  convert->setObjectName(QStringLiteral("insertMeshConvert"));
  convert->setToolTip(QObject::tr("A new solid of flat faces by Convert Mesh; the mesh body is kept, hidden"));
  convert->setChecked(settings.value(QStringLiteral("insertMesh/convertToSolid"), false).toBool());
  form->addRow(QString(), convert);
  auto* tolerance = new QDoubleSpinBox;
  tolerance->setDecimals(6);
  tolerance->setRange(0.000001, 1.0);
  tolerance->setSingleStep(0.01);
  tolerance->setValue(0.01);
  tolerance->setSuffix(QStringLiteral(" mm"));
  form->addRow(QObject::tr("Weld tolerance:"), tolerance);
  auto* angle = new QDoubleSpinBox;
  angle->setDecimals(3);
  angle->setRange(0.001, 30.0);
  angle->setSingleStep(0.1);
  angle->setValue(0.5);
  angle->setSuffix(QStringLiteral(" deg"));
  form->addRow(QObject::tr("Merge angle:"), angle);
  const auto follow = [tolerance, angle](bool on) {
    tolerance->setEnabled(on);
    angle->setEnabled(on);
  };
  follow(convert->isChecked());
  QObject::connect(convert, &QCheckBox::toggled, &dialog, [follow](bool on) {
    follow(on);
    qDebug().noquote() << QStringLiteral("Insert Mesh convert to solid %1")
                              .arg(on ? QStringLiteral("on") : QStringLiteral("off"));
  });
  QDialogButtonBox* buttons = okCancel(dialog);
  layout->addWidget(buttons);
  if (unit != nullptr) {
    unit->setFocus();
  }
  qDebug().noquote() << QStringLiteral("Insert Mesh dialog opened: unit %1")
                            .arg(unit != nullptr ? unitOffer(unit) : QStringLiteral("of the file"));
  qDebug().noquote() << QStringLiteral("Insert Mesh convert to solid %1")
                            .arg(convert->isChecked() ? QStringLiteral("on") : QStringLiteral("off"));
  // Where the check box and OK are, in the main window's coordinates (UI
  // tests).
  TestSync::singleShot(200, &dialog, [convert, buttons, parent] {
    QCoreApplication::sendPostedEvents(nullptr, QEvent::LayoutRequest);
    const QPoint origin = parent != nullptr ? parent->window()->mapToGlobal(QPoint(0, 0)) : QPoint();
    const auto at = [&origin](const QWidget* widget) {
      const QPoint global = widget->mapToGlobal(widget->rect().center()) - origin;
      return QStringLiteral("%1,%2").arg(global.x()).arg(global.y());
    };
    qDebug().noquote() << QStringLiteral("Insert Mesh convert at %1").arg(at(convert));
    if (QPushButton* ok = buttons->button(QDialogButtonBox::Ok)) {
      qDebug().noquote() << QStringLiteral("Insert Mesh OK at %1").arg(at(ok));
    }
  });
  prepareModal(&dialog);
  if (execDialog(&dialog) != QDialog::Accepted) {
    return std::nullopt;
  }
  MeshInsertChoice choice;
  choice.unitMillimetres = unit != nullptr ? kUnits[unit->currentIndex()].second : 1.0;
  choice.convert = convert->isChecked();
  choice.tolerance = tolerance->value();
  choice.angleDegrees = angle->value();
  settings.setValue(QStringLiteral("insertMesh/convertToSolid"), choice.convert);
  qDebug().noquote() << QStringLiteral("Insert Mesh: unit %1, convert to solid %2")
                            .arg(unit != nullptr ? unit->currentText() : QStringLiteral("of the file"),
                                 choice.convert
                                                          ? QStringLiteral("on, weld tolerance %1 mm, merge angle %2 deg")
                                                                .arg(choice.tolerance)
                                                                .arg(choice.angleDegrees)
                                                          : QStringLiteral("off"));
  return choice;
}

std::optional<InsertLink> askLinked(QWidget* parent, const QString& file) {
  // Linked (the whole design, as before, or chosen parts of it,
  // mitcad#2130) or a copy; Enter takes the whole design linked.
  QDialog dialog(parent);
  dialog.setObjectName(QStringLiteral("insertLinkDialog"));
  dialog.setWindowTitle(QObject::tr("Insert Component"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* text = new QLabel(QObject::tr("Insert %1 linked, so that it follows the file, or as a copy that can be "
                                      "edited here?")
                              .arg(QFileInfo(file).fileName()),
                          &dialog);
  text->setWordWrap(true);
  layout->addWidget(text);
  auto* linked = new QRadioButton(QObject::tr("&Linked"), &dialog);
  auto* whole = new QRadioButton(QObject::tr("&Whole design"), &dialog);
  auto* parts = new QRadioButton(QObject::tr("Choose &parts…"), &dialog);
  auto* copy = new QRadioButton(QObject::tr("&Copy"), &dialog);
  whole->setToolTip(QObject::tr("Every visible body of the design, as it places them"));
  parts->setToolTip(QObject::tr("Only the occurrences and bodies you tick"));
  copy->setToolTip(QObject::tr("The design and its history become a component of this design"));
  auto* how = new QButtonGroup(&dialog);
  how->addButton(linked);
  how->addButton(copy);
  auto* which = new QButtonGroup(&dialog);
  which->addButton(whole);
  which->addButton(parts);
  linked->setChecked(true);
  whole->setChecked(true);
  layout->addWidget(linked);
  auto* choices = new QVBoxLayout;
  choices->setContentsMargins(panel::kTreeIndentation + 8, 0, 0, 0);
  choices->addWidget(whole);
  choices->addWidget(parts);
  layout->addLayout(choices);
  layout->addWidget(copy);
  QObject::connect(linked, &QRadioButton::toggled, &dialog, [whole, parts](bool on) {
    whole->setEnabled(on);
    parts->setEnabled(on);
  });
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, &dialog);
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  layout->addWidget(buttons);
  if (QPushButton* ok = buttons->button(QDialogButtonBox::Ok)) {
    ok->setDefault(true);
  }
  qDebug().noquote() << QStringLiteral("Insert Component asked: %1").arg(QFileInfo(file).fileName());
  prepareModal(&dialog);
  if (execDialog(&dialog) != QDialog::Accepted) {
    qDebug().noquote() << QStringLiteral("Insert Component: canceled");
    return std::nullopt;
  }
  const InsertLink chosen = copy->isChecked()    ? InsertLink::Copy
                            : parts->isChecked() ? InsertLink::ChooseParts
                                                 : InsertLink::Whole;
  qDebug().noquote() << QStringLiteral("Insert Component: %1")
                            .arg(chosen == InsertLink::Copy          ? QStringLiteral("copy")
                                 : chosen == InsertLink::ChooseParts ? QStringLiteral("linked, chosen parts")
                                                                     : QStringLiteral("linked, whole design"));
  return chosen;
}

namespace {

// The rows of Choose Parts… (mitcad#2130): an occurrence's path or a
// body's uid, and which it is.
constexpr int kPartRole = Qt::UserRole;
constexpr int kPartKindRole = Qt::UserRole + 1;
constexpr int kOccurrencePart = 1;
constexpr int kBodyPart = 2;

void addOccurrenceRows(QTreeWidgetItem* parent, QTreeWidget* view, const QJsonArray& occurrences) {
  for (const QJsonValue& value : occurrences) {
    const QJsonObject occurrence = value.toObject();
    auto* item = parent != nullptr ? new QTreeWidgetItem(parent) : new QTreeWidgetItem(view);
    item->setText(0, occurrence.value(QStringLiteral("name")).toString());
    item->setData(0, kPartRole, occurrence.value(QStringLiteral("path")).toString());
    item->setData(0, kPartKindRole, kOccurrencePart);
    item->setFlags(item->flags() | Qt::ItemIsUserCheckable);
    item->setCheckState(0, Qt::Unchecked);
    if (!occurrence.value(QStringLiteral("visible")).toBool(true)) {
      item->setToolTip(0, QObject::tr("Hidden in the design: it brings no bodies while it is hidden"));
    }
    addOccurrenceRows(item, view, occurrence.value(QStringLiteral("children")).toArray());
  }
}

// Every row under `item` takes its state.
void setSubtree(QTreeWidgetItem* item, Qt::CheckState state) {
  for (int i = 0; i < item->childCount(); ++i) {
    item->child(i)->setCheckState(0, state);
    setSubtree(item->child(i), state);
  }
}

bool anyTicked(const QTreeWidgetItem* item) {
  for (int i = 0; i < item->childCount(); ++i) {
    if (item->child(i)->checkState(0) != Qt::Unchecked || anyTicked(item->child(i))) {
      return true;
    }
  }
  return false;
}

// The rows above `item`: one ticked whole while something in it is not
// is partly ticked, and one with something ticked in it at least partly.
void settleAbove(QTreeWidgetItem* item) {
  for (QTreeWidgetItem* above = item->parent(); above != nullptr; above = above->parent()) {
    bool all = true;
    for (int i = 0; i < above->childCount(); ++i) {
      all = all && above->child(i)->checkState(0) == Qt::Checked;
    }
    const Qt::CheckState now = above->checkState(0);
    if (now == Qt::Checked && !all) {
      above->setCheckState(0, Qt::PartiallyChecked);
    } else if (now != Qt::Checked) {
      above->setCheckState(0, anyTicked(above) ? Qt::PartiallyChecked : Qt::Unchecked);
    }
  }
}

// What is ticked: an occurrence ticked whole with what is in it, else the
// ones ticked in it; a ticked body of the root.
void collectParts(const QTreeWidgetItem* item, QJsonArray& occurrences, QJsonArray& bodies) {
  const int kind = item->data(0, kPartKindRole).toInt();
  if (item->checkState(0) == Qt::Checked && kind == kOccurrencePart) {
    occurrences.append(item->data(0, kPartRole).toString());
    return;
  }
  if (item->checkState(0) == Qt::Checked && kind == kBodyPart) {
    bodies.append(item->data(0, kPartRole).toString());
    return;
  }
  for (int i = 0; i < item->childCount(); ++i) {
    collectParts(item->child(i), occurrences, bodies);
  }
}

} // namespace

std::optional<LinkPartsChoice> chooseLinkParts(QWidget* parent, const QString& title, const QJsonObject& tree,
                                               bool offerWhole) {
  QDialog dialog(parent);
  dialog.setObjectName(QStringLiteral("linkPartsDialog"));
  dialog.setWindowTitle(title);
  auto* layout = new QVBoxLayout(&dialog);
  const QString file = tree.value(QStringLiteral("file")).toString();
  auto* text = new QLabel(QObject::tr("The occurrences and bodies of %1 to link. An occurrence brings the components "
                                      "in it; changes to them come in with Update Link.")
                              .arg(file),
                          &dialog);
  text->setWordWrap(true);
  layout->addWidget(text);
  auto* view = new QTreeWidget(&dialog);
  view->setObjectName(QStringLiteral("linkPartsTree"));
  view->setHeaderHidden(true);
  view->setColumnCount(1);
  panel::setCompactIndentation(view, panel::Indent::Tree);
  addOccurrenceRows(nullptr, view, tree.value(QStringLiteral("occurrences")).toArray());
  const QJsonArray bodies = tree.value(QStringLiteral("bodies")).toArray();
  if (!bodies.isEmpty()) {
    // The root's bodies in their folder, as the browser lists them.
    auto* folder = new QTreeWidgetItem(view);
    folder->setText(0, QObject::tr("Bodies"));
    folder->setFlags(folder->flags() | Qt::ItemIsUserCheckable);
    folder->setCheckState(0, Qt::Unchecked);
    for (const QJsonValue& value : bodies) {
      const QJsonObject body = value.toObject();
      auto* item = new QTreeWidgetItem(folder);
      item->setText(0, body.value(QStringLiteral("name")).toString());
      item->setData(0, kPartRole, body.value(QStringLiteral("uid")).toString());
      item->setData(0, kPartKindRole, kBodyPart);
      item->setFlags(item->flags() | Qt::ItemIsUserCheckable);
      item->setCheckState(0, Qt::Unchecked);
      if (!body.value(QStringLiteral("visible")).toBool(true)) {
        item->setToolTip(0, QObject::tr("Hidden in the design: it brings nothing while it is hidden"));
      }
    }
  }
  view->expandAll();
  // What the component links now.
  bool settling = true;
  const QJsonObject chosen = tree.value(QStringLiteral("chosen")).toObject();
  const QJsonArray chosenOccurrences = chosen.value(QStringLiteral("occurrences")).toArray();
  const QJsonArray chosenBodies = chosen.value(QStringLiteral("bodies")).toArray();
  for (QTreeWidgetItemIterator it(view); *it != nullptr; ++it) {
    QTreeWidgetItem* item = *it;
    const QString part = item->data(0, kPartRole).toString();
    const int kind = item->data(0, kPartKindRole).toInt();
    if ((kind == kOccurrencePart && chosenOccurrences.contains(QJsonValue(part))) ||
        (kind == kBodyPart && chosenBodies.contains(QJsonValue(part)))) {
      item->setCheckState(0, Qt::Checked);
      setSubtree(item, Qt::Checked);
    }
  }
  for (QTreeWidgetItemIterator it(view); *it != nullptr; ++it) {
    if ((*it)->checkState(0) != Qt::Unchecked) {
      settleAbove(*it);
    }
  }
  settling = false;
  layout->addWidget(view, 1);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, &dialog);
  QPushButton* whole = nullptr;
  if (offerWhole) {
    whole = buttons->addButton(QObject::tr("Whole Design"), QDialogButtonBox::ResetRole);
    whole->setToolTip(QObject::tr("Links every visible body of the design again"));
  }
  bool wholeChosen = false;
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  if (whole != nullptr) {
    QObject::connect(whole, &QPushButton::clicked, &dialog, [&dialog, &wholeChosen] {
      wholeChosen = true;
      dialog.accept();
    });
  }
  layout->addWidget(buttons);
  QPushButton* ok = buttons->button(QDialogButtonBox::Ok);
  const auto anything = [view] {
    QJsonArray occurrences;
    QJsonArray ticked;
    for (int i = 0; i < view->topLevelItemCount(); ++i) {
      collectParts(view->topLevelItem(i), occurrences, ticked);
    }
    return !occurrences.isEmpty() || !ticked.isEmpty();
  };
  if (ok != nullptr) {
    ok->setEnabled(anything());
  }
  QObject::connect(view, &QTreeWidget::itemChanged, &dialog, [&settling, view, ok, anything](QTreeWidgetItem* item) {
    if (settling) {
      return;
    }
    settling = true;
    const Qt::CheckState state = item->checkState(0);
    // A partly ticked row clicked is ticked whole.
    const Qt::CheckState whole = state == Qt::Unchecked ? Qt::Unchecked : Qt::Checked;
    item->setCheckState(0, whole);
    setSubtree(item, whole);
    settleAbove(item);
    settling = false;
    qDebug().noquote() << QStringLiteral("Choose Parts: %1 %2")
                              .arg(whole == Qt::Checked ? QStringLiteral("ticked") : QStringLiteral("unticked"),
                                   item->text(0));
    if (ok != nullptr) {
      ok->setEnabled(anything());
    }
    view->viewport()->update();
  });
  if (view->topLevelItemCount() > 0) {
    view->setCurrentItem(view->topLevelItem(0));
  }
  view->setFocus();
  dialog.resize(dialog.sizeHint().expandedTo(QSize(360, 320)));
  qDebug().noquote() << QStringLiteral("Choose Parts opened: %1, %2 rows").arg(file).arg([view] {
    int rows = 0;
    for (QTreeWidgetItemIterator it(view); *it != nullptr; ++it) {
      ++rows;
    }
    return rows;
  }());
  prepareModal(&dialog);
  if (execDialog(&dialog) != QDialog::Accepted) {
    qDebug().noquote() << QStringLiteral("Choose Parts: canceled");
    return std::nullopt;
  }
  LinkPartsChoice choice;
  if (wholeChosen) {
    choice.whole = true;
    qDebug().noquote() << QStringLiteral("Choose Parts: the whole design");
    return choice;
  }
  QJsonArray occurrences;
  QJsonArray ticked;
  for (int i = 0; i < view->topLevelItemCount(); ++i) {
    collectParts(view->topLevelItem(i), occurrences, ticked);
  }
  choice.parts = QJsonObject{{QStringLiteral("occurrences"), occurrences}, {QStringLiteral("bodies"), ticked}};
  QStringList listed;
  for (const QJsonValue& value : occurrences) {
    listed << value.toString();
  }
  for (const QJsonValue& value : ticked) {
    listed << value.toString();
  }
  qDebug().noquote() << QStringLiteral("Choose Parts: %1").arg(listed.join(QStringLiteral(", ")));
  return choice;
}

std::optional<OutsideLinkChoice> askOutsideLink(QWidget* parent, const QString& file, bool inProject) {
  const QString name = QFileInfo(file).fileName();
  const QString text =
      inProject ? QObject::tr("%1 is outside this design's project, so a link to it is not followed: changes to "
                              "the file will not come in.")
                      .arg(name)
                : QObject::tr("%1 is outside this design's folder, so a link to it is not followed: changes to "
                              "the file will not come in.")
                      .arg(name);
  const QString more =
      inProject ? QObject::tr("Copy it into the project's parts folder and link the copy, or insert it as a copy "
                              "that can be edited here.")
                : QObject::tr("Copy it into the parts folder beside this design and link the copy, or insert it "
                              "as a copy that can be edited here.");
  QMessageBox box(QMessageBox::Warning, QObject::tr("Insert Component"), text, QMessageBox::Cancel, parent);
  box.setObjectName(QStringLiteral("insertOutsideLink"));
  box.setInformativeText(more);
  QPushButton* copyIn = box.addButton(QObject::tr("Copy into the Project and &Link"), QMessageBox::AcceptRole);
  copyIn->setToolTip(QObject::tr("The copy in the project is linked and followed when the design opens"));
  QPushButton* copy = box.addButton(QObject::tr("Insert as a &Copy"), QMessageBox::AcceptRole);
  copy->setToolTip(QObject::tr("The design and its history become a component of this design"));
  box.setDefaultButton(copyIn);
  // The copy linked whole (unticked, the default) or only chosen parts of
  // it (mitcad#2137), as Linked > Whole design or Choose parts… does for a
  // file in the project (`askLinked`, mitcad#2130). Insert as a Copy takes
  // the whole design either way.
  auto* parts = new QCheckBox(QObject::tr("Choose &parts…"), &box);
  parts->setObjectName(QStringLiteral("insertOutsideLinkParts"));
  parts->setToolTip(QObject::tr("Link only the occurrences and bodies you tick of the copy"));
  parts->setChecked(false);
  box.setCheckBox(parts);
  QObject::connect(parts, &QCheckBox::toggled, &box, [](bool on) {
    qDebug().noquote() << QStringLiteral("Insert Component outside the project: choose parts %1")
                              .arg(on ? QStringLiteral("on") : QStringLiteral("off"));
  });
  QString copyInLabel = copyIn->text();
  QString copyLabel = copy->text();
  copyInLabel.remove(QLatin1Char('&'));
  copyLabel.remove(QLatin1Char('&'));
  qDebug().noquote() << QStringLiteral("Insert Component outside the project: %1 (%2); choices %3 | %4 | Cancel")
                            .arg(name, inProject ? QStringLiteral("project") : QStringLiteral("folder"), copyInLabel,
                                 copyLabel);
  qDebug().noquote() << QStringLiteral("Insert Component outside the project text: %1 %2").arg(text, more);
  prepareModal(&box);
  execDialog(&box);
  std::optional<OutsideLinkChoice> chosen;
  QString how = QStringLiteral("cancelled");
  if (box.clickedButton() == copyIn) {
    if (parts->isChecked()) {
      chosen = OutsideLinkChoice::CopyIntoProjectChooseParts;
      how = QStringLiteral("copy into the project and link, chosen parts");
    } else {
      chosen = OutsideLinkChoice::CopyIntoProject;
      how = QStringLiteral("copy into the project and link");
    }
  } else if (box.clickedButton() == copy) {
    chosen = OutsideLinkChoice::Copy;
    how = QStringLiteral("insert as a copy");
  }
  qDebug().noquote() << QStringLiteral("Insert Component outside the project: %1").arg(how);
  return chosen;
}

std::optional<QString> askStepStructure(QWidget* parent, const QString& file, bool* repeatedParts) {
  QMessageBox box(QMessageBox::Question, QObject::tr("Import STEP Assembly"),
                  QObject::tr("%1 is an assembly. Import each part once as a component with an occurrence per "
                              "instance, or every placed part as a body of one base feature?")
                      .arg(QFileInfo(file).fileName()),
                  QMessageBox::Cancel, parent);
  box.setObjectName(QStringLiteral("importStepStructure"));
  QPushButton* components = box.addButton(QObject::tr("&Components (keep the assembly)"), QMessageBox::AcceptRole);
  components->setToolTip(QObject::tr("Repeated parts share one body: lighter in memory, in the file and in the view"));
  QPushButton* base = box.addButton(QObject::tr("&One base feature"), QMessageBox::AcceptRole);
  base->setToolTip(QObject::tr("Every placed part a body of its own, where the file places it"));
  box.setDefaultButton(components);
  // Find Repeated Parts after the import (mitcad#1277), off by default.
  auto* repeated = new QCheckBox(QObject::tr("Find &repeated parts"), &box);
  repeated->setObjectName(QStringLiteral("importRepeatedParts"));
  repeated->setToolTip(QObject::tr("Bodies that are copies of one another, moved or turned, become occurrences of "
                                   "one component"));
  repeated->setChecked(false);
  box.setCheckBox(repeated);
  qDebug().noquote() << QStringLiteral("Import STEP Assembly dialog opened: %1").arg(QFileInfo(file).fileName());
  prepareModal(&box);
  execDialog(&box);
  std::optional<QString> chosen;
  if (box.clickedButton() == components) {
    chosen = QStringLiteral("components");
  } else if (box.clickedButton() == base) {
    chosen = QStringLiteral("base");
  }
  const bool find = chosen && repeated->isChecked();
  if (repeatedParts != nullptr) {
    *repeatedParts = find;
  }
  qDebug().noquote() << QStringLiteral("Import STEP Assembly: %1%2")
                            .arg(chosen ? *chosen : QStringLiteral("cancelled"),
                                 find ? QStringLiteral(", repeated parts") : QString());
  return chosen;
}

std::optional<bool> askImportPlace(QWidget* parent, const QString& file) {
  const QFileInfo info(file);
  QMessageBox box(QMessageBox::Question, QObject::tr("Where to Import"),
                  QObject::tr("Import %1 into a new component named %2, or into the active component?")
                      .arg(info.fileName(), info.completeBaseName()),
                  QMessageBox::Cancel, parent);
  box.setObjectName(QStringLiteral("importPlace"));
  QPushButton* component = box.addButton(QObject::tr("&New Component"), QMessageBox::AcceptRole);
  component->setToolTip(QObject::tr("One component holds what the file brings, to move or hide at once"));
  QPushButton* active = box.addButton(QObject::tr("Into the &Active Component"), QMessageBox::AcceptRole);
  active->setToolTip(QObject::tr("The file's bodies or components go straight into the active component"));
  box.setDefaultButton(component);
  qDebug().noquote() << QStringLiteral("Import place dialog opened: %1").arg(info.fileName());
  prepareModal(&box);
  execDialog(&box);
  std::optional<bool> chosen;
  if (box.clickedButton() == component) {
    chosen = true;
  } else if (box.clickedButton() == active) {
    chosen = false;
  }
  qDebug().noquote() << QStringLiteral("Import place: %1")
                            .arg(!chosen  ? QStringLiteral("cancelled")
                                 : *chosen ? QStringLiteral("new component")
                                           : QStringLiteral("active component"));
  return chosen;
}

std::optional<DesignImportPlace> askDesignImportPlace(QWidget* parent, const QStringList& files, bool inProject) {
  QStringList names;
  for (const QString& file : files) {
    names << QFileInfo(file).fileName();
  }
  const QString text =
      files.size() == 1
          ? QObject::tr("Import %1 into this design as a component, or as a new design?").arg(names.first())
          : QObject::tr("Import %n files into this design as components, or each as a new design?", nullptr,
                        static_cast<int>(files.size()));
  QMessageBox box(QMessageBox::Question, QObject::tr("Where to Import"), text, QMessageBox::Cancel, parent);
  box.setObjectName(QStringLiteral("importDesignPlace"));
  QPushButton* into = box.addButton(QObject::tr("Into &This Design"), QMessageBox::AcceptRole);
  into->setToolTip(QObject::tr("A component of the active component, with the file's history and material"));
  QPushButton* separate = box.addButton(QObject::tr("A &New Design"), QMessageBox::AcceptRole);
  separate->setToolTip(QObject::tr("Opens in a tab of its own; this design stays as it is"));
  box.setDefaultButton(into);
  // In a project with versions: saved in the project and linked, so that
  // several designs share it (remembered); else a copy.
  QCheckBox* linked = nullptr;
  QSettings settings;
  const QString linkedKey = QStringLiteral("import/designsLinked");
  if (inProject) {
    linked = new QCheckBox(QObject::tr("&Linked: saved in the project as a design of its own"), &box);
    linked->setObjectName(QStringLiteral("importDesignLinked"));
    linked->setToolTip(QObject::tr("Designs that link it share it; off: a copy that is edited here"));
    linked->setChecked(settings.value(linkedKey, true).toBool());
    box.setCheckBox(linked);
  }
  qDebug().noquote() << QStringLiteral("Where to Import dialog opened: %1%2")
                            .arg(names.join(QStringLiteral(", ")),
                                 linked == nullptr ? QString()
                                 : linked->isChecked() ? QStringLiteral("; linked on")
                                                       : QStringLiteral("; linked off"));
  prepareModal(&box);
  execDialog(&box);
  std::optional<DesignImportPlace> chosen;
  if (box.clickedButton() == into) {
    chosen = DesignImportPlace{true, linked != nullptr && linked->isChecked()};
    if (linked != nullptr) {
      settings.setValue(linkedKey, linked->isChecked());
    }
  } else if (box.clickedButton() == separate) {
    chosen = DesignImportPlace{false, false};
  }
  qDebug().noquote() << QStringLiteral("Where to Import: %1")
                            .arg(!chosen                ? QStringLiteral("cancelled")
                                 : !chosen->intoDesign  ? QStringLiteral("new design")
                                 : chosen->linked       ? QStringLiteral("this design (linked)")
                                                        : QStringLiteral("this design (copy)"));
  return chosen;
}

std::optional<bool> askIamImport(QWidget* parent, const QString& file) {
  QDialog dialog(parent);
  dialog.setObjectName(QStringLiteral("importAssembly"));
  dialog.setWindowTitle(QObject::tr("Import Assembly"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* text = new QLabel(QObject::tr("Import the components of %1.").arg(QFileInfo(file).fileName()));
  text->setWordWrap(true);
  layout->addWidget(text);
  auto* history = new QCheckBox(QObject::tr("Components with their design &history"));
  history->setObjectName(QStringLiteral("iamPartsHistory"));
  history->setToolTip(QObject::tr("Each component's parameters and features replayed; off: its stored bodies only"));
  QSettings settings;
  history->setChecked(iamPartsHistory(settings));
  layout->addWidget(history);
  // (In the dialog before Import is made its default: a button not yet in
  // a dialog keeps the flag without telling the dialog, which then makes
  // its first button, Cancel, the default too, and Enter cancelled.)
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel, &dialog);
  QPushButton* importButton = buttons->button(QDialogButtonBox::Ok);
  importButton->setText(QObject::tr("&Import"));
  importButton->setToolTip(QObject::tr("Import the assembly with this choice, which is remembered"));
  buttons->button(QDialogButtonBox::Cancel)->setAutoDefault(false);
  importButton->setDefault(true);
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  QObject::connect(history, &QCheckBox::toggled, &dialog, [](bool on) {
    qDebug().noquote() << QStringLiteral("Import Assembly: parts with their design history %1")
                              .arg(on ? QStringLiteral("on") : QStringLiteral("off"));
  });
  layout->addWidget(buttons);
  history->setFocus();
  qDebug().noquote() << QStringLiteral("Import Assembly dialog opened: %1, parts with their design history %2")
                            .arg(QFileInfo(file).fileName(),
                                 history->isChecked() ? QStringLiteral("on") : QStringLiteral("off"));
  prepareModal(&dialog);
  if (execDialog(&dialog) != QDialog::Accepted) {
    qDebug().noquote() << QStringLiteral("Import Assembly cancelled");
    return std::nullopt;
  }
  const bool chosen = history->isChecked();
  rememberIamPartsHistory(settings, chosen);
  qDebug().noquote() << QStringLiteral("Import Assembly: %1, parts with their design history %2")
                            .arg(QFileInfo(file).fileName(), chosen ? QStringLiteral("on") : QStringLiteral("off"));
  return chosen;
}

namespace {

// An import_fcstd result (a FreeCAD document) rather than import_f3d's.
bool isFreeCadImport(const QJsonObject& result) {
  return result.value(QStringLiteral("report")).toObject().contains(QStringLiteral("program_version"));
}

// An import_ipt result (an .ipt part's stored bodies, mitcad#60).
bool isIptImport(const QJsonObject& result) {
  return result.value(QStringLiteral("report")).toObject().value(QStringLiteral("format")).toString() ==
         QLatin1String("ipt");
}

// An .iam assembly's import result (import_ipt or import_iam, mitcad#60
// stage 4).
bool isIamImport(const QJsonObject& result) {
  return result.value(QStringLiteral("report")).toObject().value(QStringLiteral("format")).toString() ==
         QLatin1String("iam");
}

// An IFC file's import result (import_ifc, mitcad#273).
bool isIfcImport(const QJsonObject& result) {
  return result.value(QStringLiteral("report")).toObject().value(QStringLiteral("format")).toString() ==
         QLatin1String("ifc");
}

// An IFC file's products by how they came in.
QString ifcCounts(const QJsonObject& report) {
  const QJsonObject outcomes = report.value(QStringLiteral("outcomes")).toObject();
  QStringList parts;
  for (auto it = outcomes.begin(); it != outcomes.end(); ++it) {
    parts << QStringLiteral("%1 %2").arg(it.value().toInt()).arg(it.key());
  }
  return parts.join(QStringLiteral(", "));
}

// A drawing's import result (import_dwg, mitcad#453).
bool isDwgImport(const QJsonObject& result) {
  return result.value(QStringLiteral("report")).toObject().value(QStringLiteral("format")).toString() ==
         QLatin1String("dwg");
}

// A .3dm model's import result (import_3dm).
bool isThreeDmImport(const QJsonObject& result) {
  return result.value(QStringLiteral("report")).toObject().value(QStringLiteral("format")).toString() ==
         QLatin1String("3dm");
}

// A drawing's entities taken, by type (a .3dm model's objects likewise).
QString dwgCounts(const QJsonObject& report) {
  const QJsonObject taken = report.value(QStringLiteral("taken")).toObject();
  QStringList parts;
  for (auto it = taken.begin(); it != taken.end(); ++it) {
    parts << QStringLiteral("%1 %2").arg(it.value().toInt()).arg(it.key());
  }
  return parts.join(QStringLiteral(", "));
}

// An import_rvt result: an .rvt project's elements or an .rfa family's
// metadata (mitcad#274).
bool isRvtImport(const QJsonObject& result) {
  const QString format = result.value(QStringLiteral("report")).toObject().value(QStringLiteral("format")).toString();
  return format == QLatin1String("rvt") || format == QLatin1String("rfa");
}

// An import_solidcraft result: a SolidCraft design with its history
// (mitcad#450).
bool isSolidcraftImport(const QJsonObject& result) {
  return result.value(QStringLiteral("format")).toString() == QLatin1String("solidcraft");
}

// A SolidCraft design's features by how they came in.
QString solidcraftCounts(const QJsonObject& result) {
  const QJsonObject counts = result.value(QStringLiteral("counts")).toObject();
  QStringList parts;
  for (const char* outcome : {"parametric", "partial", "unmapped", "failed", "skipped"}) {
    parts << QStringLiteral("%1 %2").arg(counts.value(QLatin1String(outcome)).toInt()).arg(QLatin1String(outcome));
  }
  return parts.join(QStringLiteral(", "));
}

// The bodies an import_rvt result made: saved meshes and boxes, on how many
// levels; the elements built as features (mitcad#275) and the links found.
QString rvtCounts(const QJsonObject& report) {
  const QJsonObject import = report.value(QStringLiteral("import")).toObject();
  int levels = 0;
  for (const QJsonValue& level : import.value(QStringLiteral("levels")).toArray()) {
    levels += level.toObject().value(QStringLiteral("bodies")).toInt() > 0 ? 1 : 0;
  }
  int found = 0;
  const QJsonArray links = import.value(QStringLiteral("links")).toArray();
  for (const QJsonValue& link : links) {
    found += link.toObject().value(QStringLiteral("found")).isString() ? 1 : 0;
  }
  return QStringLiteral("%1 saved meshes, %2 boxes at a family type's transform, %3 boxes on %4 levels; "
                        "%5 meshes of the geometry the file keeps, %6 reference bodies, %7 boxes hidden; "
                        "%8 elements as features; %9 of %10 links found")
      .arg(import.value(QStringLiteral("meshes")).toInt())
      .arg(import.value(QStringLiteral("oriented_boxes")).toInt())
      .arg(import.value(QStringLiteral("boxes")).toInt())
      .arg(levels)
      .arg(import.value(QStringLiteral("geometry_meshes")).toInt())
      .arg(import.value(QStringLiteral("reference_bodies")).toInt())
      .arg(import.value(QStringLiteral("hidden_boxes")).toInt())
      .arg(import.value(QStringLiteral("features")).toInt())
      .arg(found)
      .arg(links.size());
}

// An assembly's occurrences and files: placed, suppressed, parts imported
// and missing.
QString iamCounts(const QJsonObject& report) {
  const QJsonObject occurrences = report.value(QStringLiteral("occurrences")).toObject();
  const QJsonObject parts = report.value(QStringLiteral("parts")).toObject();
  return QStringLiteral("%1 occurrences placed, %2 suppressed, %3 part files imported, %4 missing, %5 failed")
      .arg(occurrences.value(QStringLiteral("placed")).toInt())
      .arg(occurrences.value(QStringLiteral("suppressed")).toInt())
      .arg(parts.value(QStringLiteral("imported")).toInt())
      .arg(parts.value(QStringLiteral("missing")).toInt())
      .arg(parts.value(QStringLiteral("failed")).toInt());
}

// The bodies an import_ipt result reports: solids, valid ones, sheets.
QString iptCounts(const QJsonObject& report) {
  int solids = 0;
  int valid = 0;
  const QJsonArray imported = report.value(QStringLiteral("imported")).toArray();
  for (const QJsonValue& value : imported) {
    const QJsonObject body = value.toObject();
    solids += body.value(QStringLiteral("solid")).toBool() ? 1 : 0;
    valid += body.value(QStringLiteral("valid")).toBool() ? 1 : 0;
  }
  return QStringLiteral("%1 solids, %2 valid, %3 sheets, %4 not built")
      .arg(solids)
      .arg(valid)
      .arg(imported.size() - solids)
      .arg(report.value(QStringLiteral("skipped")).toArray().size());
}

// The outcomes of an .ipt part's features and sketches replayed with the
// history (`counts` of the import_ipt report).
QString iptFeatureCounts(const QJsonObject& report) {
  const QJsonObject counts = report.value(QStringLiteral("counts")).toObject();
  QStringList parts;
  for (const char* outcome : {"parametric", "partial", "fallback", "skipped"}) {
    parts << QStringLiteral("%1 %2").arg(counts.value(QLatin1String(outcome)).toInt()).arg(QLatin1String(outcome));
  }
  return parts.join(QStringLiteral(", "));
}

} // namespace

QString importCounts(const QJsonObject& result) {
  if (isSolidcraftImport(result)) {
    return solidcraftCounts(result);
  }
  if (isIfcImport(result)) {
    return ifcCounts(result.value(QStringLiteral("report")).toObject());
  }
  if (isRvtImport(result)) {
    return rvtCounts(result.value(QStringLiteral("report")).toObject());
  }
  if (isDwgImport(result) || isThreeDmImport(result)) {
    return dwgCounts(result.value(QStringLiteral("report")).toObject());
  }
  if (isIamImport(result)) {
    return iamCounts(result.value(QStringLiteral("report")).toObject());
  }
  if (isIptImport(result)) {
    return iptCounts(result.value(QStringLiteral("report")).toObject());
  }
  const QJsonObject counts = result.value(QStringLiteral("counts")).toObject();
  QStringList parts;
  const std::initializer_list<const char*> f3d = {"parametric", "partial", "fallback", "skipped"};
  const std::initializer_list<const char*> fcstd = {"body",    "component", "occurrence", "parametric",
                                                    "partial", "fallback",  "included",   "skipped"};
  for (const char* outcome : isFreeCadImport(result) ? fcstd : f3d) {
    parts << QStringLiteral("%1 %2").arg(counts.value(QLatin1String(outcome)).toInt()).arg(QLatin1String(outcome));
  }
  return parts.join(QStringLiteral(", "));
}

namespace {

// The design the import result reports on: import_f3d's first design, or
// import_fcstd's report itself (its items and warnings have the same form).
QJsonObject importedDesign(const QJsonObject& result) {
  const QJsonObject report = result.value(QStringLiteral("report")).toObject();
  if (isFreeCadImport(result) || isIptImport(result) || isIamImport(result) || isIfcImport(result) ||
      isRvtImport(result) || isSolidcraftImport(result) || isDwgImport(result) || isThreeDmImport(result)) {
    return report;
  }
  const QJsonArray designs = report.value(QStringLiteral("designs")).toArray();
  return designs.isEmpty() ? QJsonObject() : designs.first().toObject();
}

// What a FreeCAD document's bodies are: the history's features replayed
// (each checked against the shape FreeCAD stored for it), fallen back to
// their stored shapes or skipped; or FreeCAD's stored shapes alone.
QString freeCadBodies(const QJsonObject& report, int bodies) {
  if (report.value(QStringLiteral("bodies_only")).toBool()) {
    return QObject::tr("%1 bodies, the shapes FreeCAD stored (the history was not imported).").arg(bodies);
  }
  const QJsonArray features = report.value(QStringLiteral("features")).toArray();
  if (features.isEmpty()) {
    return QObject::tr("%1 bodies, the shapes FreeCAD stored: the document has no feature history to replay.")
        .arg(bodies);
  }
  int replayed = 0;
  int fallback = 0;
  for (const QJsonValue& value : features) {
    const QString outcome = value.toObject().value(QStringLiteral("outcome")).toString();
    if (outcome == QLatin1String("parametric") || outcome == QLatin1String("partial")) {
      ++replayed;
    } else if (outcome == QLatin1String("fallback")) {
      ++fallback;
    }
  }
  return QObject::tr("%1 bodies; of the history's %2 features, %3 were replayed as MitCad features (checked "
                     "against the shapes FreeCAD stored), %4 came in as their stored shapes (fallback) and %5 "
                     "were skipped.")
      .arg(bodies)
      .arg(features.size())
      .arg(replayed)
      .arg(fallback)
      .arg(features.size() - replayed - fallback);
}

// What became of a FreeCAD document's parameters and expressions: the
// parameters made (of FreeCAD's expressions, of its values), and the
// document's expressions translated, kept as FreeCAD's values or bound to
// what the import does not carry over; empty without any. `kept` gets each
// expression kept as FreeCAD's value with the reason.
QString freeCadParameters(const QJsonObject& report, QStringList* kept) {
  const QJsonArray parameters = report.value(QStringLiteral("parameters")).toArray();
  const QJsonArray expressions = report.value(QStringLiteral("expressions")).toArray();
  if (parameters.isEmpty() && expressions.isEmpty()) {
    return QString();
  }
  const auto outcome = [](const QJsonValue& value) {
    return value.toObject().value(QStringLiteral("outcome")).toString();
  };
  const auto keptValue = [](const QString& o) {
    return o == QLatin1String("value") || o == QLatin1String("mismatched");
  };
  int translated = 0;
  int values = 0;
  int leftOut = 0;
  for (const QJsonValue& value : parameters) {
    const QString o = outcome(value);
    const QJsonObject p = value.toObject();
    if (o == QLatin1String("parameter")) {
      ++translated;
    } else if (keptValue(o)) {
      ++values;
      // A cell's formula (bound expressions are listed below).
      const QString formula = p.value(QStringLiteral("freecad")).toString();
      if (kept && !formula.isEmpty() && p.contains(QStringLiteral("cell"))) {
        *kept << QObject::tr("%1 = %2: %3")
                     .arg(p.value(QStringLiteral("source")).toString(), formula,
                          mitcad::modelMessage(p.value(QStringLiteral("note")).toString()));
      }
    } else {
      ++leftOut;
    }
  }
  int expressionsTranslated = 0;
  int expressionsKept = 0;
  for (const QJsonValue& value : expressions) {
    const QString o = outcome(value);
    if (o == QLatin1String("parameter") || o == QLatin1String("expression")) {
      ++expressionsTranslated;
    } else if (keptValue(o)) {
      ++expressionsKept;
      const QJsonObject e = value.toObject();
      QString path = e.value(QStringLiteral("path")).toString();
      if (path.startsWith(QLatin1Char('.'))) {
        path.remove(0, 1);
      }
      if (kept) {
        *kept << QObject::tr("%1.%2 = %3: %4")
                     .arg(e.value(QStringLiteral("object")).toString(), path,
                          e.value(QStringLiteral("expression")).toString(),
                          mitcad::modelMessage(e.value(QStringLiteral("note")).toString()));
      }
    }
  }
  QString text = QObject::tr("%1 parameters (%2 of FreeCAD's expressions, %3 of its values)")
                     .arg(translated + values)
                     .arg(translated)
                     .arg(values);
  if (leftOut > 0) {
    text += QObject::tr(", %1 left out").arg(leftOut);
  }
  return text + QObject::tr("; of the document's %1 expressions, %2 were translated, %3 kept FreeCAD's value and "
                            "%4 drive nothing the import carries over.")
                    .arg(expressions.size())
                    .arg(expressionsTranslated)
                    .arg(expressionsKept)
                    .arg(expressions.size() - expressionsTranslated - expressionsKept);
}

} // namespace

QString importStop(const QJsonObject& result) {
  const QJsonValue stopped = importedDesign(result).value(QStringLiteral("stopped"));
  if (!stopped.isObject()) {
    return QString();
  }
  const QJsonObject at = stopped.toObject();
  if (!at.contains(QStringLiteral("item"))) {
    return QStringLiteral("after the last item");
  }
  return QStringLiteral("at %1 (item %2)")
      .arg(at.value(QStringLiteral("name")).toString())
      .arg(at.value(QStringLiteral("item")).toInteger());
}

namespace {

// The columns of the Import Report's table: the row's status first
// (mitcad#1893), then the row's number, name, type, how it came in and its
// note.
enum ReportColumn { kStatus = 0, kIndex, kItem, kType, kCameIn, kNote, kReportColumns };

// A row's rank by its status (errors first) and its place in the report's
// own order, kept for sorting and filtering.
constexpr int kRankRole = Qt::UserRole + 1;
constexpr int kOrderRole = Qt::UserRole + 2;

// The level a report row's status is drawn with (the model's `row_status`,
// mitcad#1893): OK the success check, a warning and an error their
// badges; nothing for a row without a status.
NoticeLevel reportRowLevel(const QString& status) {
  if (status == QLatin1String("ok")) {
    return NoticeLevel::Success;
  }
  if (status == QLatin1String("warning")) {
    return NoticeLevel::Warning;
  }
  if (status == QLatin1String("error")) {
    return NoticeLevel::Error;
  }
  return NoticeLevel::Hint;
}

// Errors 0, warnings 1, OK 2, no status 3.
int reportRowRank(const QString& status) {
  if (status == QLatin1String("error")) {
    return 0;
  }
  if (status == QLatin1String("warning")) {
    return 1;
  }
  return status == QLatin1String("ok") ? 2 : 3;
}

// A row of the Import Report: sorted by its status (errors first, then the
// report's order) in the status column, in the report's order in the #
// column (the lists of a report each count from 1), by its text elsewhere.
class ReportRow : public QTreeWidgetItem {
public:
  ReportRow(QTreeWidget* tree, const QStringList& texts) : QTreeWidgetItem(tree, texts) {}

  bool operator<(const QTreeWidgetItem& other) const override {
    const int column = treeWidget() != nullptr ? treeWidget()->sortColumn() : int(kIndex);
    const int order = data(kIndex, kOrderRole).toInt();
    const int otherOrder = other.data(kIndex, kOrderRole).toInt();
    if (column == kStatus) {
      const int rank = data(kStatus, kRankRole).toInt();
      const int otherRank = other.data(kStatus, kRankRole).toInt();
      return rank != otherRank ? rank < otherRank : order < otherOrder;
    }
    if (column == kIndex) {
      return order < otherOrder;
    }
    return QTreeWidgetItem::operator<(other);
  }
};

} // namespace

void showImportReport(QWidget* parent, const QString& file, const QJsonObject& result, int bodies) {
  const QJsonObject design = importedDesign(result);
  const QJsonArray fileBodies = design.value(QStringLiteral("bodies")).toArray();
  int matched = 0;
  for (const QJsonValue& value : fileBodies) {
    const QJsonObject body = value.toObject();
    const double difference = std::abs(body.value(QStringLiteral("volume_difference")).toDouble(1.0));
    // A FreeCAD document's bodies are its stored shapes themselves.
    if ((body.contains(QStringLiteral("mitcad")) && difference < 1e-3) || isFreeCadImport(result)) {
      ++matched;
    }
  }

  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Import Report"));
  auto* layout = new QVBoxLayout(&dialog);
  const auto field = [&design](const char* key, const QString& otherwise) {
    const QString value = design.value(QLatin1String(key)).toString();
    return (value.isEmpty() ? otherwise : value).toHtmlEscaped();
  };
  // An .ipt part replayed with its features (mitcad#60, stages 2 and 3):
  // the replay's report is nested in the import's.
  const bool iptHistory = isIptImport(result) && design.value(QStringLiteral("history")).toBool();
  const QJsonObject replay = design.value(QStringLiteral("design")).toObject();
  const QString iptMaterial =
      design.value(QStringLiteral("mitcad_material")).isString()
          ? QObject::tr("%1 (MitCad's %2)").arg(field("material", QString()), field("mitcad_material", QString()))
          : QObject::tr("%1 (not in MitCad's library: the bodies keep the default)")
                .arg(field("material", QObject::tr("none")));
  const QJsonObject boxes = design.value(QStringLiteral("boxes")).toObject();
  const QJsonObject display = design.value(QStringLiteral("display")).toObject();
  QStringList found;
  const QJsonObject resolution = design.value(QStringLiteral("resolution")).toObject();
  for (auto it = resolution.begin(); it != resolution.end(); ++it) {
    found << QStringLiteral("%1 %2").arg(it.value().toInt()).arg(it.key());
  }
  const QJsonObject quantities = design.value(QStringLiteral("quantities")).toObject();
  // A building project's elements or a family's metadata (mitcad#274).
  QStringList leftOut;
  const QJsonObject rvtImport = design.value(QStringLiteral("import")).toObject();
  const QJsonObject left = rvtImport.value(QStringLiteral("left_out")).toObject();
  for (auto it = left.begin(); it != left.end(); ++it) {
    leftOut << QStringLiteral("%1 %2").arg(it.value().toInt()).arg(it.key());
  }
  const QJsonObject rvtElements = design.value(QStringLiteral("elements")).toObject();
  // A SolidCraft design's kinds without a MitCad counterpart (mitcad#450).
  QStringList unmapped;
  for (const QJsonValue& value : design.value(QStringLiteral("unmapped")).toArray()) {
    const QJsonArray kind = value.toArray();
    unmapped << QStringLiteral("%1 (%2)").arg(kind.at(0).toString()).arg(kind.at(1).toInt());
  }
  // A drawing's entity types left out (mitcad#453).
  QStringList dwgLeftOut;
  for (const QJsonValue& value : design.value(QStringLiteral("left_out")).toArray()) {
    const QJsonObject l = value.toObject();
    dwgLeftOut << QStringLiteral("%1 %2 (%3)")
                      .arg(l.value(QStringLiteral("count")).toInt())
                      .arg(l.value(QStringLiteral("type")).toString(), l.value(QStringLiteral("why")).toString());
  }
  QString summary =
      isSolidcraftImport(result)
          ? QObject::tr("<b>%1</b> (SolidCraft design %2, file format %3)<br>%4 timeline features: %5.<br>%6 bodies, "
                        "%7 sketches, %8 parameters, %9 joints.<br>Without a MitCad counterpart: %10.")
                .arg(file.toHtmlEscaped(), field("design", QString()))
                .arg(design.value(QStringLiteral("format")).toInt())
                .arg(design.value(QStringLiteral("features")).toArray().size())
                .arg(importCounts(result).toHtmlEscaped())
                .arg(bodies)
                .arg(design.value(QStringLiteral("sketches")).toArray().size())
                .arg(design.value(QStringLiteral("parameters")).toArray().size())
                .arg(design.value(QStringLiteral("joints")).toArray().size())
                .arg(unmapped.isEmpty() ? QObject::tr("none") : unmapped.join(QStringLiteral(", ")).toHtmlEscaped())
      : isThreeDmImport(result)
          ? QObject::tr("<b>%1</b> (.3dm model, file version %2)<br>%3 bodies (%4 solids, %5 meshes), %6 "
                        "components.<br>Objects taken: %7.<br>Left out: %8.<br>Units: %9.")
                .arg(file.toHtmlEscaped(), field("version", QString()))
                .arg(design.value(QStringLiteral("bodies")).toInt())
                .arg(design.value(QStringLiteral("solids")).toInt())
                .arg(design.value(QStringLiteral("meshes")).toInt())
                .arg(design.value(QStringLiteral("components")).toArray().size())
                .arg(importCounts(result).toHtmlEscaped(),
                     dwgLeftOut.isEmpty() ? QObject::tr("nothing") : dwgLeftOut.join(QStringLiteral(", ")).toHtmlEscaped(),
                     field("file_units", QStringLiteral("-")))
      : isDwgImport(result)
          ? QObject::tr("<b>%1</b> (drawing %2)<br>%3 bodies (%4 solids, %5 meshes), %6 components.<br>Entities "
                        "taken: %7.<br>Left out: %8.<br>Units: %9.")
                .arg(file.toHtmlEscaped(), field("version", QString()))
                .arg(design.value(QStringLiteral("bodies")).toInt())
                .arg(design.value(QStringLiteral("solids")).toInt())
                .arg(design.value(QStringLiteral("meshes")).toInt())
                .arg(design.value(QStringLiteral("components")).toArray().size())
                .arg(importCounts(result).toHtmlEscaped(),
                     dwgLeftOut.isEmpty() ? QObject::tr("nothing") : dwgLeftOut.join(QStringLiteral(", ")).toHtmlEscaped(),
                     field("drawing_units", QStringLiteral("-")))
      : isIfcImport(result)
          ? QObject::tr("<b>%1</b> (%2, view %3, from %4)<br>%5 products: %6.<br>%7 bodies, %8 features, %9 type "
                        "shapes as components; %10 storey planes, %11 grid axes.<br>Checked against the file: %12 of "
                        "%13 element volumes agree with its quantities.")
                .arg(file.toHtmlEscaped(), field("schema_name", QString()), field("view_definition", QStringLiteral("-")),
                     field("originating_system", QStringLiteral("-")))
                .arg(design.value(QStringLiteral("products")).toArray().size())
                .arg(importCounts(result).toHtmlEscaped())
                .arg(design.value(QStringLiteral("bodies")).toInt())
                .arg(design.value(QStringLiteral("features")).toInt())
                .arg(design.value(QStringLiteral("maps")).toArray().size())
                .arg(design.value(QStringLiteral("storey_planes")).toInt())
                .arg(design.value(QStringLiteral("grid_axes")).toInt())
                .arg(quantities.value(QStringLiteral("agree")).toInt())
                .arg(quantities.value(QStringLiteral("compared")).toInt())
      : isRvtImport(result)
          ? (design.value(QStringLiteral("format")).toString() == QLatin1String("rfa")
                 ? QObject::tr("<b>%1</b> (family %2)<br>Saved by release %3 (build %4). %5 elements read; the "
                               "family's forms are not imported yet.")
                       .arg(file.toHtmlEscaped(), field("family", QObject::tr("unnamed")),
                            design.value(QStringLiteral("release")).toVariant().toString(),
                            field("build", QObject::tr("unknown")))
                       .arg(rvtElements.value(QStringLiteral("declared")).toInt())
                 : QObject::tr("<b>%1</b> (project)<br>Saved by release %2 (build %3).<br>%4 bodies, one component "
                               "per level: %5.<br>%6 elements read (%7 in part); %8 placed on a level by their "
                               "elevation.<br>Left out: %9.")
                       .arg(file.toHtmlEscaped(), design.value(QStringLiteral("release")).toVariant().toString(),
                            field("build", QObject::tr("unknown")))
                       .arg(bodies)
                       .arg(importCounts(result).toHtmlEscaped())
                       .arg(rvtElements.value(QStringLiteral("declared")).toInt())
                       .arg(rvtElements.value(QStringLiteral("read_in_part")).toInt())
                       .arg(rvtImport.value(QStringLiteral("by_elevation")).toInt())
                       .arg(leftOut.join(QStringLiteral(", ")).toHtmlEscaped()))
      : isIamImport(result)
          ? QObject::tr("<b>%1</b> (assembly)<br>%2; %3 sub-assemblies.<br>Files found: %4.<br>Checked against "
                        "the file: %5 of %6 placements agree with the displayed ones, %7 of %8 parts' bodies lie "
                        "within their range boxes.<br>Units: %9. Saved by release %10.")
                .arg(file.toHtmlEscaped(), importCounts(result).toHtmlEscaped())
                .arg(design.value(QStringLiteral("assemblies")).toInt())
                .arg(found.join(QStringLiteral(", ")).toHtmlEscaped())
                .arg(display.value(QStringLiteral("agree")).toInt())
                .arg(display.value(QStringLiteral("compared")).toInt())
                .arg(boxes.value(QStringLiteral("agree")).toInt())
                .arg(boxes.value(QStringLiteral("compared")).toInt())
                .arg(field("units", QStringLiteral("mm")), field("release", QObject::tr("unknown")))
      : iptHistory
          ? QObject::tr("<b>%1</b> (part number %2)<br>%3 features and sketches: %4.<br>%5 parameters, %6 of %7 "
                        "expressions as the file evaluates them.<br>%8 bodies: %9.<br>Material: %10. Units: %11. "
                        "Saved by release %12.")
                .arg(file.toHtmlEscaped(), field("part_number", QObject::tr("none")))
                .arg(replay.value(QStringLiteral("items")).toArray().size())
                .arg(iptFeatureCounts(design))
                .arg(replay.value(QStringLiteral("parameters")).toObject().value(QStringLiteral("imported")).toInt())
                .arg(design.value(QStringLiteral("expressions")).toObject().value(QStringLiteral("agree")).toInt())
                .arg(design.value(QStringLiteral("expressions")).toObject().value(QStringLiteral("translated")).toInt())
                .arg(bodies)
                .arg(importCounts(result), iptMaterial, field("units", QStringLiteral("mm")),
                     field("release", QObject::tr("unknown")))
      : isIptImport(result)
          ? QObject::tr("<b>%1</b> (part number %2)<br>%3 bodies stored in the file, each a base feature: %4.<br>"
                        "Material: %5. Units: %6. Saved by release %7.")
                .arg(file.toHtmlEscaped(), field("part_number", QObject::tr("none")))
                .arg(bodies)
                .arg(importCounts(result), iptMaterial, field("units", QStringLiteral("mm")),
                     field("release", QObject::tr("unknown")))
      : isFreeCadImport(result)
          ? QObject::tr("<b>%1</b> (FreeCAD %2)<br>%3 objects: %4.<br>%5")
                .arg(file.toHtmlEscaped(),
                     design.value(QStringLiteral("program_version")).toString().toHtmlEscaped())
                .arg(result.value(QStringLiteral("items")).toInt())
                .arg(importCounts(result), freeCadBodies(design, bodies))
          : QObject::tr("<b>%1</b> (%2)<br>%3 timeline items: %4.<br>%5 bodies; %6 of the file's %7 solids match "
                        "by volume.")
                .arg(file.toHtmlEscaped(), result.value(QStringLiteral("design")).toString().toHtmlEscaped())
                .arg(result.value(QStringLiteral("items")).toInt())
                .arg(importCounts(result))
                .arg(bodies)
                .arg(matched)
                .arg(fileBodies.size());
  // A FreeCAD document's parameters and expressions (stage 4).
  QStringList kept;
  const QString parameters = isFreeCadImport(result) ? freeCadParameters(design, &kept) : QString();
  if (!parameters.isEmpty()) {
    summary += QStringLiteral("<br>") + parameters.toHtmlEscaped();
  }
  // An assembly's parts with their design history (mitcad#507).
  if (isIamImport(result)) {
    const QJsonObject parts = design.value(QStringLiteral("parts")).toObject();
    summary += design.value(QStringLiteral("history")).toBool()
                   ? QObject::tr("<br>Components with their design history: %1 replayed, %2 as their stored bodies.")
                         .arg(parts.value(QStringLiteral("replayed")).toInt())
                         .arg(parts.value(QStringLiteral("as_bodies")).toInt())
                   : QObject::tr("<br>The components' stored bodies only (their design history not replayed).");
  }
  // Stopped by the user (T1e).
  const QString stop = importStop(result);
  const QJsonObject stopped = design.value(QStringLiteral("stopped")).toObject();
  if (stopped.contains(QStringLiteral("item"))) {
    summary += QObject::tr("<br><b>The import was stopped</b> at %1 (timeline item %2): it and the features "
                           "after it came in as the file's bodies.")
                   .arg(stopped.value(QStringLiteral("name")).toString().toHtmlEscaped())
                   .arg(stopped.value(QStringLiteral("item")).toInteger());
  } else if (!stop.isEmpty()) {
    summary += QObject::tr("<br><b>The import was stopped</b> after the last item: the bodies were compared by "
                           "volume only.");
  }
  // The verdict in one line with its symbol (mitcad#1893): the model's,
  // of the design the report shows.
  const QJsonObject verdict = design.contains(QStringLiteral("verdict"))
                                  ? design.value(QStringLiteral("verdict")).toObject()
                                  : result.value(QStringLiteral("verdict")).toObject();
  const QString verdictStatus = verdict.value(QStringLiteral("status")).toString();
  const QString verdictMessage = verdict.value(QStringLiteral("message")).toString();
  if (!verdictMessage.isEmpty()) {
    auto* verdictLabel = new QLabel(QStringLiteral("%1&nbsp;<b>%2</b>")
                                        .arg(noticeIconHtml(reportRowLevel(verdictStatus), 16),
                                             mitcad::modelMessage(verdictMessage).toHtmlEscaped()));
    verdictLabel->setObjectName(QStringLiteral("importReportVerdict"));
    verdictLabel->setWordWrap(true);
    layout->addWidget(verdictLabel);
    qDebug().noquote() << QStringLiteral("Import report verdict: %1: %2: %3").arg(file, verdictStatus, verdictMessage);
  }
  auto* label = new QLabel(summary);
  label->setWordWrap(true);
  layout->addWidget(label);
  // The rows shown by their status (mitcad#1893).
  auto* filterRow = new QHBoxLayout;
  auto* filterLabel = new QLabel(QObject::tr("&Show:"));
  auto* filter = new QComboBox;
  filter->setObjectName(QStringLiteral("importReportFilter"));
  filter->addItems({QObject::tr("All rows"), QObject::tr("Warnings and errors"), QObject::tr("Errors")});
  filterLabel->setBuddy(filter);
  filterRow->addWidget(filterLabel);
  filterRow->addWidget(filter);
  filterRow->addStretch(1);
  layout->addLayout(filterRow);
  auto* items = new QTreeWidget;
  items->setObjectName(QStringLiteral("importReportRows"));
  panel::setCompactIndentation(items, panel::Indent::Flat);
  items->setColumnCount(kReportColumns);
  items->setHeaderLabels({QObject::tr("Status"), QObject::tr("#"), QObject::tr("Item"), QObject::tr("Type"),
                          QObject::tr("Came in as"), QObject::tr("Note")});
  items->setRootIsDecorated(false);
  items->header()->setSectionResizeMode(QHeaderView::ResizeToContents);
  // A row with its status's symbol, the status in words as its tooltip
  // (the model's `row_status` and `row_status_message`, mitcad#1893), in
  // the report's order; logged for the tests.
  int rowOrder = 0;
  const auto addRow = [&](const QJsonObject& source, const QStringList& texts) {
    const QString rowStatus = source.value(QStringLiteral("row_status")).toString();
    const QString rowWords = mitcad::modelMessage(source.value(QStringLiteral("row_status_message")).toString());
    auto* added = new ReportRow(items, QStringList{QString()} + texts);
    added->setIcon(kStatus, noticeIcon(reportRowLevel(rowStatus)));
    added->setToolTip(kStatus, rowWords);
    added->setData(kStatus, Qt::AccessibleTextRole, rowWords);
    added->setData(kStatus, kRankRole, reportRowRank(rowStatus));
    added->setData(kIndex, kOrderRole, rowOrder++);
    qDebug().noquote() << QStringLiteral("Import report row: %1: %2 %3: %4")
                              .arg(file, texts.value(0), texts.value(1),
                                   rowStatus.isEmpty() ? QStringLiteral("none") : rowStatus);
    return added;
  };
  // An .ipt part's bodies: one base feature each, or the replay's.
  int bodyIndex = 0;
  for (const QJsonValue& value : design.value(QStringLiteral("imported")).toArray()) {
    const QJsonObject body = value.toObject();
    const QJsonArray madeBodies = body.value(QStringLiteral("bodies")).toArray();
    const QJsonObject made = madeBodies.isEmpty() ? QJsonObject() : madeBodies.first().toObject();
    // Valid or not, and how it compares with the file's body (mitcad#1893:
    // "matches the file's body (volume +0.0002 %)").
    const QString state = body.contains(QStringLiteral("row_status_message"))
                              ? mitcad::modelMessage(body.value(QStringLiteral("row_status_message")).toString())
                          : body.value(QStringLiteral("valid")).toBool() ? QObject::tr("valid")
                                                                         : QObject::tr("not valid");
    const QString note = QObject::tr("%1, %2 faces, from %3")
                             .arg(state)
                             .arg(body.value(QStringLiteral("faces")).toInt())
                             .arg(body.value(QStringLiteral("source")).toString());
    // The feature by its name, never its uid (docs/ui-conventions.md).
    const QString featureName = body.value(QStringLiteral("feature_name")).toString();
    QString cameIn;
    if (featureName.isEmpty()) {
      cameIn = iptHistory ? QObject::tr("a body") : QObject::tr("a base feature");
    } else {
      cameIn = (iptHistory ? QObject::tr("body of %1") : QObject::tr("base feature %1")).arg(featureName);
    }
    QTreeWidgetItem* bodyRow = addRow(
        body, {QString::number(++bodyIndex), made.value(QStringLiteral("name")).toString(),
               body.value(QStringLiteral("solid")).toBool() ? QObject::tr("solid") : QObject::tr("surface"), cameIn,
               note});
    bodyRow->setToolTip(kNote, note);
  }
  // An .ipt part's bodies that could not be built (mitcad#1893: errors).
  if (isIptImport(result)) {
    for (const QJsonValue& value : design.value(QStringLiteral("skipped")).toArray()) {
      const QJsonObject body = value.toObject();
      const QString note = mitcad::modelMessage(body.value(QStringLiteral("error")).toString());
      QTreeWidgetItem* bodyRow = addRow(body, {QString::number(++bodyIndex), QObject::tr("a body of the file"),
                                               QObject::tr("body"), QObject::tr("not built"), note});
      bodyRow->setToolTip(kNote, note);
    }
  }
  // The file's bodies against the import's (an .f3d design's), and the
  // import's bodies the file does not have (mitcad#1893): a small
  // difference matches, with the difference in its note.
  if (!isIptImport(result)) {
    for (const QJsonValue& value : design.value(QStringLiteral("bodies")).toArray()) {
      const QJsonObject body = value.toObject();
      if (!body.contains(QStringLiteral("row_status"))) {
        continue;
      }
      const QString note = mitcad::modelMessage(body.value(QStringLiteral("row_status_message")).toString());
      QTreeWidgetItem* bodyRow =
          addRow(body, {QString::number(++bodyIndex), body.value(QStringLiteral("name")).toString(), QObject::tr("body"),
                        body.contains(QStringLiteral("mitcad")) ? QObject::tr("compared with the file's body")
                                                                : QObject::tr("missing"),
                        note});
      bodyRow->setToolTip(kNote, note);
    }
    for (const QJsonValue& value : design.value(QStringLiteral("extra_body_rows")).toArray()) {
      const QJsonObject body = value.toObject();
      const QString note = mitcad::modelMessage(body.value(QStringLiteral("row_status_message")).toString());
      QTreeWidgetItem* bodyRow = addRow(body, {QString::number(++bodyIndex), body.value(QStringLiteral("name")).toString(),
                                               QObject::tr("body"), QObject::tr("not in the file"), note});
      bodyRow->setToolTip(kNote, note);
    }
  }
  // A building project's levels: their components and bodies.
  int levelIndex = 0;
  for (const QJsonValue& value : rvtImport.value(QStringLiteral("levels")).toArray()) {
    const QJsonObject level = value.toObject();
    const QString note = QObject::tr("%1 meshes, %2 boxes%3")
                             .arg(level.value(QStringLiteral("meshes")).toInt())
                             .arg(level.value(QStringLiteral("boxes")).toInt())
                             .arg(level.value(QStringLiteral("elevation")).isDouble()
                                      ? QObject::tr(", elevation %1 mm")
                                            .arg(level.value(QStringLiteral("elevation")).toDouble(), 0, 'f', 1)
                                      : QString());
    QTreeWidgetItem* row = addRow(
        level, {QString::number(++levelIndex), level.value(QStringLiteral("name")).toString(), QObject::tr("level"),
                QObject::tr("%1 bodies in component %2")
                    .arg(level.value(QStringLiteral("bodies")).toInt())
                    .arg(level.value(QStringLiteral("component")).toString()),
                note});
    row->setToolTip(kNote, note);
  }
  // A drawing's (or a .3dm model's) blocks as components: their bodies
  // and occurrences.
  if (isDwgImport(result) || isThreeDmImport(result)) {
    int componentIndex = 0;
    for (const QJsonValue& value : design.value(QStringLiteral("components")).toArray()) {
      const QJsonObject c = value.toObject();
      const QString note = QObject::tr("%1 occurrences, %2 placements in all")
                               .arg(c.value(QStringLiteral("occurrences")).toInt())
                               .arg(c.value(QStringLiteral("placed")).toInt());
      QTreeWidgetItem* row = addRow(
          c, {QString::number(++componentIndex), c.value(QStringLiteral("name")).toString(), QObject::tr("block"),
              QObject::tr("%1 bodies in component %2")
                  .arg(c.value(QStringLiteral("bodies")).toInt())
                  .arg(c.value(QStringLiteral("component")).toString()),
              note});
      row->setToolTip(kNote, note);
    }
  }
  // An assembly's files: how each was found and what came in.
  int fileIndex = 0;
  for (const QJsonValue& value : design.value(QStringLiteral("files")).toArray()) {
    const QJsonObject f = value.toObject();
    const QString path = f.value(QStringLiteral("path")).isString() ? f.value(QStringLiteral("path")).toString()
                                                                    : f.value(QStringLiteral("saved")).toString();
    // A part's design replayed or its stored bodies, and why (mitcad#507).
    const QString outcome = iamPartOutcome(f.value(QStringLiteral("part")).toObject());
    const QString note = QObject::tr("%1 occurrences, found %2: %3")
                             .arg(f.value(QStringLiteral("occurrences")).toInt())
                             .arg(f.value(QStringLiteral("found")).toString(), path);
    const QString status = f.value(QStringLiteral("status")).toString();
    QTreeWidgetItem* row = addRow(
        f, {QString::number(++fileIndex), QFileInfo(path).fileName(), f.value(QStringLiteral("kind")).toString(),
            outcome.isEmpty() ? status : QStringLiteral("%1, %2").arg(status, outcome), note});
    row->setToolTip(kCameIn, outcome.isEmpty() ? status : outcome);
    row->setToolTip(kNote, note);
    if (!outcome.isEmpty()) {
      qDebug().noquote() << QStringLiteral("Import report part: %1: %2").arg(QFileInfo(path).fileName(), outcome);
    }
  }
  for (const QJsonValue& value : (iptHistory ? replay : design).value(QStringLiteral("items")).toArray()) {
    const QJsonObject item = value.toObject();
    // The note in plain words where the report gives it (mitcad#1741); the
    // `note` itself is the agents', with the sketch's ids.
    const QString note = mitcad::modelMessage(
        item.value(item.contains(QStringLiteral("note_message")) ? QStringLiteral("note_message")
                                                                 : QStringLiteral("note"))
            .toString());
    QTreeWidgetItem* row = addRow(
        item, {QString::number(item.value(QStringLiteral("index")).toInt()), item.value(QStringLiteral("name")).toString(),
               item.value(QStringLiteral("type")).toString(), item.value(QStringLiteral("outcome")).toString(), note});
    row->setToolTip(kNote, note);
  }
  // A SolidCraft design's features and joints: how each came in, or why
  // it did not (mitcad#450).
  if (isSolidcraftImport(result)) {
    int index = 0;
    for (const char* list : {"features", "joints"}) {
      for (const QJsonValue& value : design.value(QLatin1String(list)).toArray()) {
        const QJsonObject f = value.toObject();
        QStringList notes;
        for (const QJsonValue& note : f.value(QStringLiteral("notes")).toArray()) {
          notes << mitcad::modelMessage(note.toString());
        }
        const QString note = notes.join(QStringLiteral("; "));
        QTreeWidgetItem* row = addRow(f, {QString::number(++index), f.value(QStringLiteral("name")).toString(),
                                          f.value(QStringLiteral("kind")).toString(),
                                          f.value(QStringLiteral("outcome")).toString(), note});
        row->setToolTip(kNote, note);
      }
    }
  }
  // An IFC file's products: how each came in, or why it did not.
  if (isIfcImport(result)) {
    for (const QJsonValue& value : design.value(QStringLiteral("products")).toArray()) {
      const QJsonObject p = value.toObject();
      QStringList notes;
      for (const QJsonValue& note : p.value(QStringLiteral("notes")).toArray()) {
        notes << mitcad::modelMessage(note.toString());
      }
      const QString note = notes.join(QStringLiteral("; "));
      QTreeWidgetItem* row = addRow(
          p, {QString::number(p.value(QStringLiteral("id")).toInteger()),
              QStringLiteral("%1 %2").arg(p.value(QStringLiteral("name")).toString(),
                                          p.value(QStringLiteral("global_id")).toString()),
              p.value(QStringLiteral("kind")).toString(), p.value(QStringLiteral("outcome")).toString(), note});
      row->setToolTip(kNote, note);
    }
  }
  // Sorted by a click on a column's header: by the status, errors first;
  // by # in the report's own order, which it starts in.
  items->setSortingEnabled(true);
  items->sortByColumn(kIndex, Qt::AscendingOrder);
  // The filter shows all rows, the warnings and errors, or the errors.
  QObject::connect(filter, &QComboBox::currentIndexChanged, items, [items, file](int shownLevels) {
    int shown = 0;
    for (int i = 0; i < items->topLevelItemCount(); ++i) {
      QTreeWidgetItem* entry = items->topLevelItem(i);
      const int rank = entry->data(kStatus, kRankRole).toInt();
      const bool visible = shownLevels == 0 || (shownLevels == 1 && rank <= 1) || (shownLevels == 2 && rank == 0);
      entry->setHidden(!visible);
      shown += visible ? 1 : 0;
    }
    qDebug().noquote() << QStringLiteral("Import report filter: %1: %2 of %3 rows shown")
                              .arg(file)
                              .arg(shown)
                              .arg(items->topLevelItemCount());
  });
  layout->addWidget(items, 1);
  // The table has the keyboard first, as before the filter came (Enter
  // still closes the report).
  QWidget::setTabOrder(items, filter);
  if (!kept.isEmpty()) {
    QStringList lines;
    for (const QString& line : kept) {
      lines << line.toHtmlEscaped();
    }
    auto* text = new QLabel(
        QObject::tr("<b>Expressions kept as FreeCAD's values</b><br>%1").arg(lines.join(QStringLiteral("<br>"))));
    text->setWordWrap(true);
    layout->addWidget(text);
  }
  // The documents an .f3d design refers to (mitcad#196): where each came
  // from, or why its components stay empty.
  QStringList references;
  for (const QJsonValue& value : design.value(QStringLiteral("references")).toArray()) {
    const QJsonObject r = value.toObject();
    const QString name = r.value(QStringLiteral("name")).toString();
    const int version = r.value(QStringLiteral("version")).toInt();
    const QString title = (name.isEmpty() ? r.value(QStringLiteral("document")).toString() : name) +
                          (version > 0 ? QObject::tr(" (version %1)").arg(version) : QString());
    const QString source = r.value(QStringLiteral("file")).toString();
    const QString note = mitcad::modelMessage(r.value(QStringLiteral("note")).toString());
    const QString from =
        source.isEmpty() ? QString() : QObject::tr(", from %1").arg(QFileInfo(source).fileName().toHtmlEscaped());
    references << QObject::tr("%1: %2%3%4")
                      .arg(title.toHtmlEscaped(), r.value(QStringLiteral("status")).toString(), from,
                           note.isEmpty() ? QString() : QStringLiteral(" (%1)").arg(note.toHtmlEscaped()));
  }
  if (!references.isEmpty()) {
    auto* text = new QLabel(
        QObject::tr("<b>Documents referred to</b><br>%1").arg(references.join(QStringLiteral("<br>"))));
    text->setWordWrap(true);
    layout->addWidget(text);
  }
  // The real warnings, each with the warning's symbol; "No warnings." when
  // there are none (mitcad#1893).
  QStringList warnings;
  const QString warningIcon = noticeIconHtml(NoticeLevel::Warning);
  for (const QJsonValue& warning : design.value(QStringLiteral("warnings")).toArray()) {
    warnings << QStringLiteral("%1&nbsp;%2").arg(warningIcon, mitcad::modelMessage(warning.toString()).toHtmlEscaped());
  }
  for (const QJsonValue& warning : replay.value(QStringLiteral("warnings")).toArray()) {
    warnings << QStringLiteral("%1&nbsp;%2").arg(warningIcon, mitcad::modelMessage(warning.toString()).toHtmlEscaped());
  }
  {
    auto* text = new QLabel(QObject::tr("<b>Warnings</b><br>%1")
                                .arg(warnings.isEmpty() ? QObject::tr("No warnings.").toHtmlEscaped()
                                                        : warnings.join(QStringLiteral("<br>"))));
    text->setObjectName(QStringLiteral("importReportWarnings"));
    text->setWordWrap(true);
    layout->addWidget(text);
    qDebug().noquote() << QStringLiteral("Import report warnings: %1: %2").arg(file).arg(warnings.size());
  }
  // Items tried again on the file's stored bodies before them (mitcad#283):
  // the imported bodies each retry kept and replaced, and the user's answer.
  QStringList retries;
  const auto names = [](const QJsonValue& list) {
    QStringList out;
    for (const QJsonValue& name : list.toArray()) {
      out << name.toString();
    }
    return out.isEmpty() ? QObject::tr("none") : out.join(QStringLiteral(", "));
  };
  for (const QJsonValue& value : design.value(QStringLiteral("items")).toArray()) {
    const QJsonObject retry = value.toObject().value(QStringLiteral("exact_retry")).toObject();
    if (retry.isEmpty()) {
      continue;
    }
    retries << QObject::tr("%1: kept %2; replaced %3")
                   .arg(value.toObject().value(QStringLiteral("name")).toString(),
                        names(retry.value(QStringLiteral("kept"))), names(retry.value(QStringLiteral("replaced"))));
  }
  const QJsonObject question = design.value(QStringLiteral("retry_question")).toObject();
  if (!question.isEmpty()) {
    const bool replaced = question.value(QStringLiteral("answer")).toString() == QStringLiteral("replace");
    retries.prepend(replaced ? QObject::tr("Asked at %1: the stored bodies replace the imported bodies that match "
                                           "them")
                                   .arg(question.value(QStringLiteral("item")).toString())
                             : QObject::tr("Asked at %1: the imported bodies that match the stored bodies are kept")
                                   .arg(question.value(QStringLiteral("item")).toString()));
  }
  if (!retries.isEmpty()) {
    QStringList escaped;
    for (const QString& line : retries) {
      escaped << line.toHtmlEscaped();
      qDebug().noquote() << QStringLiteral("Import report retry: %1: %2").arg(file, line);
    }
    auto* text = new QLabel(QObject::tr("<b>Retries on the file's stored bodies</b><br>%1")
                                .arg(escaped.join(QStringLiteral("<br>"))));
    text->setWordWrap(true);
    layout->addWidget(text);
  }
  // Items whose results deviate from the file's (mitcad#379): those kept
  // parametric with their deviation, and those the file's stored geometry
  // stands in for.
  QStringList deviations;
  for (const QJsonValue& value : design.value(QStringLiteral("items")).toArray()) {
    const QJsonObject item = value.toObject();
    const QJsonObject accepted = item.value(QStringLiteral("accepted_deviation")).toObject();
    const QJsonObject stored = item.value(QStringLiteral("deviation")).toObject();
    const QJsonObject deviation = accepted.isEmpty() ? stored : accepted;
    if (deviation.isEmpty()) {
      continue;
    }
    const double change = deviation.value(QStringLiteral("volume_change")).toDouble();
    const double fileChange = deviation.value(QStringLiteral("file_volume_change")).toDouble();
    const QJsonValue largest = deviation.value(QStringLiteral("largest_distance"));
    deviations << QObject::tr("%1: %2; volume change %3 mm³ against the file's %4%5")
                      .arg(item.value(QStringLiteral("name")).toString(),
                           accepted.isEmpty() ? QObject::tr("the file's stored geometry")
                                          : QObject::tr("kept parametric"))
                      .arg(change, 0, 'f', 3)
                      .arg(fileChange, 0, 'f', 3)
                      .arg(largest.isDouble()
                               ? QObject::tr(", its new faces up to %1 mm off").arg(largest.toDouble(), 0, 'f', 3)
                               : QString());
  }
  if (!deviations.isEmpty()) {
    QStringList escaped;
    for (const QString& line : deviations) {
      escaped << line.toHtmlEscaped();
      qDebug().noquote() << QStringLiteral("Import report deviation: %1: %2").arg(file, line);
    }
    auto* text = new QLabel(QObject::tr("<b>Results that deviate from the file's</b><br>%1")
                                .arg(escaped.join(QStringLiteral("<br>"))));
    text->setWordWrap(true);
    layout->addWidget(text);
  }
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Close);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  buttons->button(QDialogButtonBox::Close)->setDefault(true);
  layout->addWidget(buttons);
  dialog.resize(720, 520);
  if (isIfcImport(result)) {
    qDebug().noquote() << QStringLiteral("Import report: %1: %2 products (%3), %4 bodies")
                              .arg(file)
                              .arg(design.value(QStringLiteral("products")).toArray().size())
                              .arg(importCounts(result))
                              .arg(design.value(QStringLiteral("bodies")).toInt());
  } else if (isIamImport(result) || isRvtImport(result) || isSolidcraftImport(result) || isDwgImport(result) ||
             isThreeDmImport(result)) {
    qDebug().noquote() << QStringLiteral("Import report: %1: %2").arg(file, importCounts(result));
  } else if (isIptImport(result)) {
    qDebug().noquote()
        << QStringLiteral("Import report: %1: %2 bodies (%3)").arg(file).arg(bodies).arg(importCounts(result));
    if (iptHistory) {
      qDebug().noquote() << QStringLiteral("Import report features: %1: %2").arg(file, iptFeatureCounts(design));
    }
  } else {
    qDebug().noquote() << QStringLiteral("Import report: %1: %2 items (%3), %4 bodies, %5 of the file's %6 solids match%7")
                              .arg(file)
                              .arg(result.value(QStringLiteral("items")).toInt())
                              .arg(importCounts(result))
                              .arg(bodies)
                              .arg(matched)
                              .arg(fileBodies.size())
                              .arg(stop.isEmpty() ? QString() : QStringLiteral(", stopped %1").arg(stop));
  }
  qDebug().noquote() << QStringLiteral("Import report summary: %1")
                            .arg(QTextDocumentFragment::fromHtml(summary).toPlainText().simplified());
  for (const QString& line : kept) {
    qDebug().noquote() << QStringLiteral("Import report kept: %1").arg(line);
  }
  prepareModal(&dialog);
  execDialog(&dialog);
}

} // namespace mitcad
