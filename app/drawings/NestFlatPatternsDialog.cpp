// SPDX-License-Identifier: MIT
#include "NestFlatPatternsDialog.hpp"

#include <QCheckBox>
#include <QComboBox>
#include <QCoreApplication>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDoubleSpinBox>
#include <QFormLayout>
#include <QHeaderView>
#include <QJsonObject>
#include <QLabel>
#include <QPushButton>
#include <QSettings>
#include <QSpinBox>
#include <QTableWidget>
#include <QVBoxLayout>
#include <QtLogging>

#include <algorithm>
#include <vector>

#include "../framework/Dialogs.hpp"
#include "../framework/ModelMessages.hpp"
#include "../framework/UserText.hpp"

namespace mitcad::drawings {
namespace {

// The file's texts in the context NestFlatPatterns, where lupdate records
// them too.
struct Text {
  Q_DECLARE_TR_FUNCTIONS(NestFlatPatterns)
};

// The material, spacing and kerf a new cut sheet starts with: the last
// ones chosen, else a common plasma table's sheet.
const char* const kWidthKey = "drawings/nestWidth";
const char* const kHeightKey = "drawings/nestHeight";
const char* const kGapKey = "drawings/cutSheetNestGap";
const char* const kKerfKey = "drawings/nestKerf";

QDoubleSpinBox* lengthBox(const char* name, double low, double high, int decimals, double value,
                          const QString& tooltip) {
  auto* box = new QDoubleSpinBox;
  box->setObjectName(QString::fromLatin1(name));
  box->setRange(low, high);
  box->setDecimals(decimals);
  box->setSuffix(QStringLiteral(" mm"));
  box->setValue(value);
  box->setToolTip(tooltip);
  return box;
}

} // namespace

std::optional<NestChoice> askNestFlatPatterns(QWidget* parent, const QJsonArray& rows,
                                              const QList<NestCutSheet>& cutSheets, const QStringList& selected) {
  QDialog dialog(parent);
  dialog.setWindowTitle(Text::tr("Nest Flat Patterns"));
  auto* column = new QVBoxLayout(&dialog);
  auto* about = new QLabel(Text::tr("The sheet-metal components' Flat Patterns are laid out on a cut sheet, as many "
                                    "copies as chosen, where they can be moved and turned and their plasma cutting "
                                    "program written."));
  about->setWordWrap(true);
  column->addWidget(about);
  // The parts: a check, the component and body, the blank's size and the
  // copies.
  auto* table = new QTableWidget(static_cast<int>(rows.size()), 4);
  table->setObjectName(QStringLiteral("nestFlatPatternsParts"));
  table->setHorizontalHeaderLabels(
      {Text::tr("Component"), Text::tr("Thickness"), Text::tr("Flat Size"), Text::tr("Copies")});
  table->verticalHeader()->hide();
  table->horizontalHeader()->setSectionResizeMode(0, QHeaderView::Stretch);
  table->setSelectionMode(QAbstractItemView::NoSelection);
  table->setEditTriggers(QAbstractItemView::NoEditTriggers);
  bool anySelected = false;
  for (const QJsonValue& value : rows) {
    if (selected.contains(value.toObject().value(QStringLiteral("body")).toString())) {
      anySelected = true;
    }
  }
  struct Row {
    QString body;
    QCheckBox* check = nullptr;
    QSpinBox* copies = nullptr;
  };
  std::vector<Row> parts;
  int offered = 0;
  for (int i = 0; i < rows.size(); ++i) {
    const QJsonObject row = rows.at(i).toObject();
    const QString body = row.value(QStringLiteral("body")).toString();
    const QString problem = row.value(QStringLiteral("problem")).toString();
    const QString name = QStringLiteral("%1 / %2").arg(row.value(QStringLiteral("component_name")).toString(),
                                                      row.value(QStringLiteral("name")).toString());
    auto* check = new QCheckBox(name);
    check->setObjectName(QStringLiteral("nestPart%1").arg(i + 1));
    auto* copies = new QSpinBox;
    copies->setObjectName(QStringLiteral("nestCopies%1").arg(i + 1));
    copies->setRange(1, 1000);
    copies->setValue(std::max(1, row.value(QStringLiteral("quantity")).toInt()));
    copies->setToolTip(Text::tr("How many of this component the cut sheet holds"));
    const bool developed = problem.isEmpty();
    check->setEnabled(developed);
    copies->setEnabled(developed);
    check->setChecked(developed && (!anySelected || selected.contains(body)));
    if (developed) {
      ++offered;
      table->setItem(i, 1,
                     new QTableWidgetItem(Text::tr("%1 mm").arg(row.value(QStringLiteral("thickness")).toDouble())));
      table->setItem(i, 2,
                     new QTableWidgetItem(QStringLiteral("%1 × %2 mm")
                                              .arg(row.value(QStringLiteral("width")).toDouble(), 0, 'f', 1)
                                              .arg(row.value(QStringLiteral("height")).toDouble(), 0, 'f', 1)));
    } else {
      // Why its Flat Pattern cannot be developed.
      const QString why = mitcad::modelMessage(userText(problem));
      check->setToolTip(why);
      auto* item = new QTableWidgetItem(why);
      item->setToolTip(why);
      table->setItem(i, 2, item);
    }
    table->setCellWidget(i, 0, check);
    table->setCellWidget(i, 3, copies);
    parts.push_back({body, check, copies});
  }
  table->resizeColumnsToContents();
  table->horizontalHeader()->setSectionResizeMode(0, QHeaderView::Stretch);
  column->addWidget(table, 1);
  // The cut sheet and its material.
  auto* form = new QFormLayout;
  column->addLayout(form);
  QSettings settings;
  auto* sheet = new QComboBox;
  sheet->setObjectName(QStringLiteral("nestCutSheet"));
  sheet->setToolTip(Text::tr("The cut sheet the Flat Patterns go onto: a new one or one of the design's"));
  sheet->addItem(Text::tr("New Cut Sheet"), QString());
  for (const NestCutSheet& cut : cutSheets) {
    sheet->addItem(cut.name, cut.uid);
  }
  QDoubleSpinBox* width = lengthBox("nestWidth", 1.0, 100000.0, 1,
                                    settings.value(QString::fromLatin1(kWidthKey), 2500.0).toDouble(),
                                    Text::tr("The material's width, along X"));
  QDoubleSpinBox* height = lengthBox("nestHeight", 1.0, 100000.0, 1,
                                     settings.value(QString::fromLatin1(kHeightKey), 1250.0).toDouble(),
                                     Text::tr("The material's height, along Y"));
  QDoubleSpinBox* gap = lengthBox("nestGap", 0.0, 1000.0, 1,
                                  settings.value(QString::fromLatin1(kGapKey), 5.0).toDouble(),
                                  Text::tr("The room between the components and from the sheet's edges"));
  QDoubleSpinBox* kerf = lengthBox("nestKerf", 0.0, 9.99, 2,
                                   settings.value(QString::fromLatin1(kKerfKey), 1.5).toDouble(),
                                   Text::tr("The width the cut takes away, moved half off each component"));
  form->addRow(Text::tr("Cut &sheet"), sheet);
  form->addRow(Text::tr("&Width"), width);
  form->addRow(Text::tr("&Height"), height);
  form->addRow(Text::tr("S&pacing"), gap);
  form->addRow(Text::tr("&Kerf"), kerf);
  // A cut sheet of the design: its own material and kerf.
  QObject::connect(sheet, &QComboBox::currentIndexChanged, &dialog, [sheet, cutSheets, width, height, kerf] {
    const QString uid = sheet->currentData().toString();
    for (const NestCutSheet& cut : cutSheets) {
      if (cut.uid == uid) {
        width->setValue(cut.width);
        height->setValue(cut.height);
        kerf->setValue(cut.kerf);
      }
    }
  });
  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Ok | QDialogButtonBox::Cancel);
  QPushButton* ok = buttons->button(QDialogButtonBox::Ok);
  ok->setText(Text::tr("Nest"));
  ok->setObjectName(QStringLiteral("nestFlatPatternsOk"));
  const auto update = [&parts, ok] {
    bool any = false;
    for (const Row& row : parts) {
      any = any || (row.check->isEnabled() && row.check->isChecked());
    }
    ok->setEnabled(any);
  };
  for (const Row& row : parts) {
    QObject::connect(row.check, &QCheckBox::toggled, &dialog, update);
  }
  update();
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  column->addWidget(buttons);
  dialog.resize(560, 460);
  qDebug().noquote() << QStringLiteral("Nest Flat Patterns dialog opened: %1 part(s)").arg(offered);
  if (execDialog(&dialog) != QDialog::Accepted) {
    return std::nullopt;
  }
  NestChoice choice;
  choice.cutSheet = sheet->currentData().toString();
  choice.width = width->value();
  choice.height = height->value();
  choice.gap = gap->value();
  choice.kerf = kerf->value();
  for (const Row& row : parts) {
    if (row.check->isEnabled() && row.check->isChecked()) {
      choice.bodies.append(QJsonObject{{QStringLiteral("body"), row.body}, {QStringLiteral("count"), row.copies->value()}});
      choice.count += row.copies->value();
    }
  }
  if (choice.cutSheet.isEmpty()) {
    settings.setValue(QString::fromLatin1(kWidthKey), choice.width);
    settings.setValue(QString::fromLatin1(kHeightKey), choice.height);
    settings.setValue(QString::fromLatin1(kKerfKey), choice.kerf);
  }
  settings.setValue(QString::fromLatin1(kGapKey), choice.gap);
  return choice;
}

} // namespace mitcad::drawings
