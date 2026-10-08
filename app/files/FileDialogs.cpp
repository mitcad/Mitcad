// SPDX-License-Identifier: MIT
#include "FileDialogs.hpp"

#include <cmath>
#include <initializer_list>
#include <utility>
#include <vector>

#include <QComboBox>
#include <QCoreApplication>
#include <QDialog>
#include <QButtonGroup>
#include <QDialogButtonBox>
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
#include <QTextDocumentFragment>
#include <QTreeWidget>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "../framework/Numbers.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"

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
};

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

} // namespace

std::optional<ExportChoice> askExport(QWidget* parent, const QString& path, int selectedBodies,
                                      const QVector<QPair<QString, QString>>& sketches,
                                      const QString& sketch, bool occurrences) {
  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Export"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* form = new QFormLayout;
  layout->addLayout(form);

  auto* file = new QLineEdit(path);
  auto* browse = new QPushButton(QObject::tr("Browse..."));
  auto* fileRow = new QHBoxLayout;
  fileRow->addWidget(file, 1);
  fileRow->addWidget(browse);
  form->addRow(QObject::tr("File:"), fileRow);

  auto* type = new QComboBox;
  for (const ExportType& entry : kTypes) {
    type->addItem(QCoreApplication::translate("Export", entry.label));
  }
  form->addRow(QObject::tr("Type:"), type);
  auto* bodies = new QComboBox;
  bodies->addItem(QObject::tr("All bodies"));
  if (selectedBodies > 0) {
    bodies->addItem(QObject::tr("Selected bodies (%1)").arg(selectedBodies));
    bodies->setCurrentIndex(1);
  }
  form->addRow(QObject::tr("Bodies:"), bodies);
  // Where the bodies go (mitcad#19): as the design shows them, or each in
  // its component's coordinates.
  auto* coordinates = new QComboBox;
  coordinates->addItem(QObject::tr("Where the design shows them"), QStringLiteral("design"));
  coordinates->addItem(QObject::tr("Each in its component's coordinates"), QStringLiteral("component"));
  coordinates->setToolTip(QObject::tr("A component placed twice is written twice where the design shows it, a "
                                      "STEP file as an assembly; or each body once as its component has it"));
  form->addRow(QObject::tr("Coor&dinates:"), coordinates);
  auto* unit = unitBox();
  unit->setToolTip(QObject::tr("The length unit of the file; the bodies keep their size"));
  form->addRow(QObject::tr("Unit:"), unit);
  auto* refinement = new QComboBox;
  refinement->addItems({QObject::tr("Low"), QObject::tr("Medium"), QObject::tr("High")});
  refinement->setCurrentIndex(1);
  form->addRow(QObject::tr("Refinement:"), refinement);
  auto* stlFormat = new QComboBox;
  stlFormat->addItems({QObject::tr("Binary"), QObject::tr("Text (ASCII)")});
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
  auto* message = new QLabel;
  setErrorStyleSheet(message);
  layout->addWidget(message);
  layout->addWidget(okCancel(dialog));

  // The rows a type uses; the file's extension follows the type and the
  // type the extension typed.
  const auto showRows = [=] {
    const QString format = QLatin1String(kTypes[type->currentIndex()].format);
    const bool mesh =
        format == QStringLiteral("stl") || format == QStringLiteral("obj") || format == QStringLiteral("3mf");
    form->setRowVisible(bodies, format != QStringLiteral("dxf"));
    form->setRowVisible(coordinates, occurrences && format != QStringLiteral("dxf"));
    form->setRowVisible(unit, format == QStringLiteral("step") || format == QStringLiteral("iges"));
    form->setRowVisible(refinement, mesh);
    form->setRowVisible(stlFormat, format == QStringLiteral("stl"));
    form->setRowVisible(sketchBox, format == QStringLiteral("dxf"));
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
  QObject::connect(browse, &QPushButton::clicked, &dialog, [&dialog, file] {
    const QString chosen = QFileDialog::getSaveFileName(&dialog, QObject::tr("Export"), file->text());
    if (!chosen.isEmpty()) {
      file->setText(chosen);
    }
  });
  const int start = typeOfSuffix(QFileInfo(path).suffix(), 0);
  type->setCurrentIndex(start >= 0 ? start : 0);
  showRows();
  file->setFocus();
  file->selectAll();
  dialog.resize(480, dialog.sizeHint().height());
  qDebug().noquote() << QStringLiteral("Export dialog opened: unit %1%2")
                            .arg(unitOffer(unit), occurrences ? QStringLiteral(", coordinates design (design | component)")
                                                              : QString());

  prepareModal(&dialog);
  while (dialog.exec() == QDialog::Accepted) {
    const ExportType& chosen = kTypes[type->currentIndex()];
    ExportChoice choice;
    choice.path = file->text().trimmed();
    choice.format = QLatin1String(chosen.format);
    choice.schema = QLatin1String(chosen.schema);
    choice.unit = unit->currentText();
    choice.refinement = QStringList{QStringLiteral("low"), QStringLiteral("medium"),
                                    QStringLiteral("high")}[refinement->currentIndex()];
    choice.ascii = stlFormat->currentIndex() == 1;
    choice.selectedOnly = bodies->currentIndex() == 1;
    choice.sketch = sketchBox->currentData().toString();
    if (occurrences) {
      choice.coordinates = coordinates->currentData().toString();
    }
    if (choice.path.isEmpty()) {
      message->setText(QObject::tr("Give the file to write."));
      continue;
    }
    if (QFileInfo(choice.path).suffix().isEmpty()) {
      choice.path += QLatin1Char('.') + QLatin1String(chosen.suffix);
    }
    if (choice.format == QStringLiteral("dxf") && choice.sketch.isEmpty()) {
      message->setText(QObject::tr("The document has no sketch to export."));
      continue;
    }
    if (QFileInfo::exists(choice.path) &&
        sheetQuestion(&dialog, QObject::tr("Export"),
                              QObject::tr("%1 exists. Replace it?").arg(QFileInfo(choice.path).fileName())) !=
            QMessageBox::Yes) {
      continue;
    }
    qDebug().noquote() << QStringLiteral("Export dialog: %1 to %2")
                              .arg(QCoreApplication::translate("Export", chosen.label), choice.path);
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
                                                     {"center", QObject::tr("Centre")}};
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
    size->setText(b ? QObject::tr("%1 x %2 mm").arg(b->width(), 0, 'g', 6).arg(b->height(), 0, 'g', 6)
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
  while (dialog.exec() == QDialog::Accepted) {
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

std::optional<double> askMeshUnit(QWidget* parent, const QString& file) {
  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Insert Mesh"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* form = new QFormLayout;
  layout->addLayout(form);
  auto* unit = unitBox();
  form->addRow(QObject::tr("Unit of %1:").arg(QFileInfo(file).fileName()), unit);
  layout->addWidget(okCancel(dialog));
  unit->setFocus();
  qDebug().noquote() << QStringLiteral("Insert Mesh dialog opened: unit %1").arg(unitOffer(unit));
  prepareModal(&dialog);
  if (dialog.exec() != QDialog::Accepted) {
    return std::nullopt;
  }
  return kUnits[unit->currentIndex()].second;
}

std::optional<bool> askLinked(QWidget* parent, const QString& file) {
  QMessageBox box(QMessageBox::Question, QObject::tr("Insert Component"),
                  QObject::tr("Insert %1 linked, so that it follows the file, or as a copy that can be edited "
                              "here?")
                      .arg(QFileInfo(file).fileName()),
                  QMessageBox::Cancel, parent);
  QPushButton* link = box.addButton(QObject::tr("&Linked"), QMessageBox::AcceptRole);
  QPushButton* copy = box.addButton(QObject::tr("&Copy"), QMessageBox::AcceptRole);
  box.setDefaultButton(link);
  prepareModal(&box);
  box.exec();
  if (box.clickedButton() == link) {
    return true;
  }
  if (box.clickedButton() == copy) {
    return false;
  }
  return std::nullopt;
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
  if (isFreeCadImport(result) || isIptImport(result)) {
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
  return QObject::tr("%1 bodies; of the history's %2 features, %3 were replayed as Mitcad features (checked "
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
                          p.value(QStringLiteral("note")).toString());
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
                          e.value(QStringLiteral("note")).toString());
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
          ? QObject::tr("%1 (Mitcad's %2)").arg(field("material", QString()), field("mitcad_material", QString()))
          : QObject::tr("%1 (not in Mitcad's library: the bodies keep the default)")
                .arg(field("material", QObject::tr("none")));
  QString summary =
      iptHistory
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
  auto* label = new QLabel(summary);
  label->setWordWrap(true);
  layout->addWidget(label);
  auto* items = new QTreeWidget;
  items->setColumnCount(5);
  items->setHeaderLabels({QObject::tr("#"), QObject::tr("Item"), QObject::tr("Type"), QObject::tr("Came in as"),
                          QObject::tr("Note")});
  items->setRootIsDecorated(false);
  items->header()->setSectionResizeMode(QHeaderView::ResizeToContents);
  // An .ipt part's bodies: one base feature each, or the replay's.
  int bodyIndex = 0;
  for (const QJsonValue& value : design.value(QStringLiteral("imported")).toArray()) {
    const QJsonObject body = value.toObject();
    const QJsonArray madeBodies = body.value(QStringLiteral("bodies")).toArray();
    const QJsonObject made = madeBodies.isEmpty() ? QJsonObject() : madeBodies.first().toObject();
    const QString note = QObject::tr("%1, %2 faces, from %3")
                             .arg(body.value(QStringLiteral("valid")).toBool() ? QObject::tr("valid")
                                                                                : QObject::tr("not valid"))
                             .arg(body.value(QStringLiteral("faces")).toInt())
                             .arg(body.value(QStringLiteral("source")).toString());
    auto* item = new QTreeWidgetItem(
        items, {QString::number(++bodyIndex), made.value(QStringLiteral("name")).toString(),
                body.value(QStringLiteral("solid")).toBool() ? QObject::tr("solid") : QObject::tr("sheet"),
                (iptHistory ? QObject::tr("body of %1") : QObject::tr("base feature %1"))
                    .arg(body.value(QStringLiteral("feature")).toString()),
                note});
    item->setToolTip(4, note);
  }
  for (const QJsonValue& value : (iptHistory ? replay : design).value(QStringLiteral("items")).toArray()) {
    const QJsonObject item = value.toObject();
    auto* row = new QTreeWidgetItem(
        items, {QString::number(item.value(QStringLiteral("index")).toInt()), item.value(QStringLiteral("name")).toString(),
                item.value(QStringLiteral("type")).toString(), item.value(QStringLiteral("outcome")).toString(),
                item.value(QStringLiteral("note")).toString()});
    row->setToolTip(4, item.value(QStringLiteral("note")).toString());
  }
  layout->addWidget(items, 1);
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
  QStringList warnings;
  for (const QJsonValue& warning : design.value(QStringLiteral("warnings")).toArray()) {
    warnings << warning.toString().toHtmlEscaped();
  }
  for (const QJsonValue& warning : replay.value(QStringLiteral("warnings")).toArray()) {
    warnings << warning.toString().toHtmlEscaped();
  }
  if (!warnings.isEmpty()) {
    auto* text = new QLabel(QObject::tr("<b>Warnings</b><br>%1").arg(warnings.join(QStringLiteral("<br>"))));
    text->setWordWrap(true);
    layout->addWidget(text);
  }
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Close);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  buttons->button(QDialogButtonBox::Close)->setDefault(true);
  layout->addWidget(buttons);
  dialog.resize(720, 520);
  if (isIptImport(result)) {
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
  dialog.exec();
}

} // namespace mitcad
