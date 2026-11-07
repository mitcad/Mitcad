// SPDX-License-Identifier: MIT
#include "DrawingDialog.hpp"

#include <algorithm>
#include <iterator>
#include <utility>

#include <QCheckBox>
#include <QComboBox>
#include <QCoreApplication>
#include <QDialog>
#include <QDialogButtonBox>
#include <QEvent>
#include <QFileInfo>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QSettings>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Dialogs.hpp"
#include "../framework/TestSync.hpp"
#include "../framework/Theme.hpp"

#include "../platform/MacDialogs.hpp"

namespace mitcad {
namespace {

struct Format {
  const char* value;
  const char* label;
};

const Format kFormats[] = {{"pdf", QT_TRANSLATE_NOOP("Drawing", "PDF (*.pdf)")},
                           {"svg", QT_TRANSLATE_NOOP("Drawing", "SVG (*.svg)")},
                           {"dxf", QT_TRANSLATE_NOOP("Drawing", "DXF (*.dxf)")},
                           // R2000, the DXF file's content (mitcad#728).
                           {"dwg", QT_TRANSLATE_NOOP("Drawing", "DWG (*.dwg)")}};

int formatOfSuffix(const QString& suffix) {
  for (int i = 0; i < static_cast<int>(std::size(kFormats)); ++i) {
    if (suffix.compare(QLatin1String(kFormats[i].value), Qt::CaseInsensitive) == 0) {
      return i;
    }
  }
  return -1;
}

QCheckBox* check(const QString& text, bool on, const QString& tip = {}) {
  auto* box = new QCheckBox(text);
  box->setChecked(on);
  box->setToolTip(tip);
  return box;
}

} // namespace

std::optional<DrawingChoice> askDrawing(QWidget* parent, const QString& path, int selectedBodies,
                                        const QStringList& analyses, const QList<DrawingTarget>& drawings,
                                        const QString& last, const QString& designUnit) {
  QSettings store;
  DrawingSettings s = DrawingSettings::load(store);
  QDialog dialog(parent);
  dialog.setWindowTitle(QObject::tr("Export Drawing"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* form = new QFormLayout;
  layout->addLayout(form);

  QString start = path;
  if (formatOfSuffix(QFileInfo(path).suffix()) < 0) {
    start = QFileInfo(path).path() + QLatin1Char('/') + QFileInfo(path).completeBaseName() + QLatin1Char('.') +
            s.format;
  }
  auto* file = new QLineEdit(start);
  auto* browse = new QPushButton(QObject::tr("Browse…"));
  mac::auxiliaryButton(browse);
  auto* fileRow = new QHBoxLayout;
  fileRow->addWidget(file, 1);
  fileRow->addWidget(browse);
  form->addRow(QObject::tr("&File:"), fileRow);
  auto* format = new QComboBox;
  for (const Format& f : kFormats) {
    format->addItem(QCoreApplication::translate("Drawing", f.label), QLatin1String(f.value));
  }
  format->setCurrentIndex(std::max(0, formatOfSuffix(QFileInfo(start).suffix())));
  form->addRow(QObject::tr("F&ormat:"), format);
  // A DXF file's length unit (mitcad#1408): the design's first; the sheet
  // is written in that unit of paper.
  auto* dxfUnit = new QComboBox;
  dxfUnit->addItems(dxfUnits(designUnit));
  dxfUnit->setCurrentIndex(0);
  dxfUnit->setToolTip(QObject::tr("The length unit of the DXF file: the sheet in that unit of paper"));
  form->addRow(QObject::tr("DXF unit:"), dxfUnit);
  const auto showDxfUnit = [form, format, dxfUnit] {
    form->setRowVisible(dxfUnit, format->currentData().toString() == QStringLiteral("dxf"));
  };
  showDxfUnit();
  // Which drawing of the design it writes (mitcad#725): the one it laid out
  // last first; another of the root's drawings, written as it is (it
  // follows the model; what was made on it by hand stays); or a new one,
  // laid out from the settings below.
  auto* target = new QComboBox;
  for (const DrawingTarget& d : drawings) {
    target->addItem(d.uid == last ? QObject::tr("%1 (laid out last)").arg(d.name) : d.name, d.uid);
  }
  target->addItem(QObject::tr("New Drawing"), QString());
  target->setCurrentIndex(last.isEmpty() || target->findData(last) < 0 ? target->count() - 1 : target->findData(last));
  target->setToolTip(QObject::tr("The drawing of the design written, or a new one laid out with these settings"));
  form->addRow(QObject::tr("Dra&wing:"), target);
  auto* asItIs = new QLabel(QObject::tr("Exported as it is; change it in its own workspace."));
  asItIs->setWordWrap(true);
  form->addRow(QString(), asItIs);
  auto* bodies = new QComboBox;
  bodies->addItem(QObject::tr("All shown bodies"));
  if (selectedBodies > 0) {
    bodies->addItem(QObject::tr("Selected bodies (%1)").arg(selectedBodies));
    bodies->setCurrentIndex(1);
  }
  form->addRow(QObject::tr("&Bodies:"), bodies);

  auto* sheet = new QComboBox;
  for (const auto& [value, label] : DrawingSettings::sheets()) {
    sheet->addItem(label, value);
    if (value == s.sheet) {
      sheet->setCurrentIndex(sheet->count() - 1);
    }
  }
  form->addRow(QObject::tr("&Sheet:"), sheet);
  auto* orientation = new QComboBox;
  orientation->addItems({QObject::tr("Landscape"), QObject::tr("Portrait")});
  orientation->setCurrentIndex(s.portrait ? 1 : 0);
  form->addRow(QObject::tr("O&rientation:"), orientation);
  auto* scale = new QComboBox;
  scale->setEditable(true);
  scale->addItem(QObject::tr("Fit the Sheet"), QStringLiteral("fit"));
  for (const char* ratio : {"1:1", "1:2", "1:5", "1:10", "1:20", "1:50", "2:1", "5:1", "10:1"}) {
    scale->addItem(QLatin1String(ratio), QLatin1String(ratio));
  }
  const int known = scale->findData(s.scale);
  if (known >= 0) {
    scale->setCurrentIndex(known);
  } else {
    scale->setEditText(s.scale);
  }
  scale->setToolTip(QObject::tr("The largest standard scale whose views fit the sheet, or a scale such as 1:2"));
  form->addRow(QObject::tr("S&cale:"), scale);
  auto* projection = new QComboBox;
  projection->addItems({QObject::tr("Third Angle"), QObject::tr("First Angle")});
  projection->setCurrentIndex(s.firstAngle ? 1 : 0);
  projection->setToolTip(QObject::tr("Where the top and right views go: third or first angle projection"));
  form->addRow(QObject::tr("&Projection:"), projection);

  auto* front = check(QObject::tr("Front"), s.front);
  auto* top = check(QObject::tr("Top"), s.top);
  auto* right = check(QObject::tr("Right"), s.right);
  auto* iso = check(QObject::tr("Isometric"), s.iso);
  auto* views = new QHBoxLayout;
  for (QCheckBox* box : {front, top, right, iso}) {
    views->addWidget(box);
  }
  views->addStretch(1);
  form->addRow(QObject::tr("Views:"), views);
  auto* section = new QComboBox;
  section->addItem(QObject::tr("None"), QString());
  for (const QString& name : analyses) {
    section->addItem(name, name);
  }
  section->setToolTip(QObject::tr("A section view cut where a Section Analysis cuts the bodies"));
  form->addRow(QObject::tr("Sec&tion view:"), section);

  auto* hidden = check(QObject::tr("Hidden Edges"), s.hiddenLines);
  auto* tangent = check(QObject::tr("Tangent Edges"), s.tangentEdges,
                        QObject::tr("Thin lines where faces meet smoothly"));
  auto* lines = new QHBoxLayout;
  lines->addWidget(hidden);
  lines->addWidget(tangent);
  lines->addStretch(1);
  form->addRow(QObject::tr("Lines:"), lines);
  auto* dimensions = check(QObject::tr("Sizes and hole diameters"), s.dimensions);
  auto* holeTable = check(QObject::tr("Hole table"), s.holeTable);
  auto* sketchDimensions = check(QObject::tr("Sketch dimensions"), s.sketchDimensions,
                                 QObject::tr("The dimensions of sketches seen square on in a view"));
  auto* notes = new QHBoxLayout;
  notes->addWidget(dimensions);
  notes->addWidget(holeTable);
  notes->addWidget(sketchDimensions);
  notes->addStretch(1);
  form->addRow(QObject::tr("Annotations:"), notes);
  auto* perBody = check(QObject::tr("One sheet per body"), s.perBody,
                        QObject::tr("A PDF gets a page per body; SVG, DXF and DWG a file per body"));
  form->addRow(QString(), perBody);
  // The layout's settings are a new drawing's only (mitcad#725): with a
  // drawing of the design chosen they are off, and the hint says why.
  const QList<QWidget*> layoutInputs = {bodies, sheet, orientation, scale, projection, front, top, right, iso,
                                        section, hidden, tangent, dimensions, holeTable, sketchDimensions, perBody};
  const auto followTarget = [target, asItIs, layoutInputs] {
    const bool fresh = target->currentData().toString().isEmpty();
    for (QWidget* input : layoutInputs) {
      input->setEnabled(fresh);
    }
    asItIs->setVisible(!fresh);
  };
  followTarget();
  QObject::connect(target, &QComboBox::currentIndexChanged, &dialog, [target, followTarget] {
    followTarget();
    const QString uid = target->currentData().toString();
    qDebug().noquote() << QStringLiteral("Drawing dialog: drawing = %1").arg(uid.isEmpty() ? QStringLiteral("new") : uid);
  });

  auto* message = new QLabel;
  setErrorStyleSheet(message);
  layout->addWidget(message);
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  layout->addWidget(buttons);

  // The file's extension follows the format and the format the extension.
  QObject::connect(format, &QComboBox::currentIndexChanged, &dialog, [file, format, showDxfUnit](int index) {
    showDxfUnit();
    const QFileInfo info(file->text());
    if (formatOfSuffix(info.suffix()) != index && !file->text().isEmpty()) {
      const QString base = info.suffix().isEmpty() ? file->text() : file->text().chopped(info.suffix().size() + 1);
      const QSignalBlocker blocker(file);
      file->setText(base + QLatin1Char('.') + format->currentData().toString());
    }
    qDebug().noquote() << QStringLiteral("Drawing dialog: format = %1").arg(format->currentData().toString());
  });
  QObject::connect(file, &QLineEdit::textChanged, &dialog, [format](const QString& text) {
    const int index = formatOfSuffix(QFileInfo(text).suffix());
    if (index >= 0 && index != format->currentIndex()) {
      format->setCurrentIndex(index);
    }
  });
  QObject::connect(sheet, &QComboBox::currentIndexChanged, &dialog, [sheet] {
    qDebug().noquote() << QStringLiteral("Drawing dialog: sheet = %1").arg(sheet->currentData().toString());
  });
  QObject::connect(browse, &QPushButton::clicked, &dialog, [&dialog, file] {
    const QString chosen = dialogSaveFileName(&dialog, QObject::tr("Export Drawing"), file->text(),
                                              QObject::tr("Drawings (*.pdf *.svg *.dxf *.dwg)"));
    if (!chosen.isEmpty()) {
      file->setText(chosen);
    }
  });
  file->setFocus();
  file->selectAll();
  dialog.resize(520, dialog.sizeHint().height());
  qDebug().noquote() << QStringLiteral("Drawing dialog opened: format %1, sheet %2, scale %3, drawing %4")
                            .arg(format->currentData().toString(), sheet->currentData().toString(), s.scale,
                                 target->currentData().toString().isEmpty() ? QStringLiteral("new")
                                                                            : target->currentData().toString());
  // Where the inputs are, in the dialog's own coordinates (UI tests post
  // their clicks and wheel steps to the dialog's window).
  TestSync::singleShot(200, &dialog, [&dialog, file, format, sheet, scale, target, buttons] {
    QCoreApplication::sendPostedEvents(nullptr, QEvent::LayoutRequest);
    const auto at = [&dialog](const QWidget* widget) {
      const QPoint p = widget->mapTo(&dialog, widget->rect().center());
      return QStringLiteral("%1,%2").arg(p.x()).arg(p.y());
    };
    const std::pair<const char*, QWidget*> places[] = {
        {"file", file}, {"format", format}, {"sheet", sheet}, {"scale", scale}, {"drawing", target}};
    for (const auto& [name, widget] : places) {
      qDebug().noquote() << QStringLiteral("Drawing dialog %1 at %2").arg(QLatin1String(name), at(widget));
    }
    if (QPushButton* ok = buttons->button(QDialogButtonBox::Ok)) {
      qDebug().noquote() << QStringLiteral("Drawing dialog OK at %1").arg(at(ok));
    }
  });

  prepareModal(&dialog);
  while (execDialog(&dialog) == QDialog::Accepted) {
    DrawingChoice choice;
    choice.path = file->text().trimmed();
    DrawingSettings& c = choice.settings;
    c.format = format->currentData().toString();
    c.sheet = sheet->currentData().toString();
    c.portrait = orientation->currentIndex() == 1;
    const QString typed = scale->currentIndex() >= 0 && scale->currentText() == scale->itemText(scale->currentIndex())
                              ? scale->currentData().toString()
                              : scale->currentText();
    c.scale = normalizedScale(typed);
    c.firstAngle = projection->currentIndex() == 1;
    c.front = front->isChecked();
    c.top = top->isChecked();
    c.right = right->isChecked();
    c.iso = iso->isChecked();
    c.hiddenLines = hidden->isChecked();
    c.tangentEdges = tangent->isChecked();
    c.dimensions = dimensions->isChecked();
    c.holeTable = holeTable->isChecked();
    c.sketchDimensions = sketchDimensions->isChecked();
    c.perBody = perBody->isChecked();
    choice.selectedOnly = bodies->currentIndex() == 1;
    choice.section = section->currentData().toString();
    choice.drawing = target->currentData().toString();
    choice.dxfUnit = dxfUnit->currentText();
    if (choice.path.isEmpty()) {
      message->setText(QObject::tr("Give the file to write."));
      continue;
    }
    if (QFileInfo(choice.path).suffix().isEmpty()) {
      choice.path += QLatin1Char('.') + c.format;
    }
    // A drawing of the design is written as it is: no layout to check.
    if (choice.drawing.isEmpty() && c.scale.isEmpty()) {
      message->setText(QObject::tr("A scale is \"Fit the Sheet\" or a ratio such as 1:2."));
      continue;
    }
    if (choice.drawing.isEmpty() && c.views().isEmpty() && choice.section.isEmpty()) {
      message->setText(QObject::tr("Choose at least one view."));
      continue;
    }
    if (QFileInfo::exists(choice.path) &&
        sheetQuestion(&dialog, QObject::tr("Export Drawing"),
                      QObject::tr("%1 exists. Replace it?").arg(QFileInfo(choice.path).fileName())) !=
            QMessageBox::Yes) {
      continue;
    }
    c.save(store);
    qDebug().noquote() << QStringLiteral("Drawing dialog: %1 %2 to %3").arg(c.format, c.sheet, choice.path);
    return choice;
  }
  return std::nullopt;
}

} // namespace mitcad
