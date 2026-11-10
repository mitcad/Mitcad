// SPDX-License-Identifier: MIT
#include "ImportDeviations.hpp"
#include "framework/ModelMessages.hpp"

#include <QAbstractItemView>
#include <QDialog>
#include <QDialogButtonBox>
#include <QHeaderView>
#include <QJsonValue>
#include <QKeyEvent>
#include <QLabel>
#include <QPushButton>
#include <QTableWidget>
#include <QVBoxLayout>
#include <QtLogging>

#include "../framework/Dialogs.hpp"

#include "../platform/MacDialogs.hpp"

namespace mitcad {
namespace {

std::optional<double> number(const QJsonObject& object, const char* key) {
  const QJsonValue value = object.value(QLatin1String(key));
  return value.isDouble() ? std::optional<double>(value.toDouble()) : std::nullopt;
}

QString mm3(double value) {
  return QStringLiteral("%1").arg(value, 0, 'f', 3);
}

QString percent(std::optional<double> share) {
  return share ? QStringLiteral("%1 %").arg(100.0 * *share, 0, 'f', 1) : QStringLiteral("-");
}

QString signedPercent(std::optional<double> share) {
  return share ? QStringLiteral("%1%2 %").arg(*share >= 0.0 ? QStringLiteral("+") : QString()).arg(100.0 * *share, 0, 'f', 1)
               : QStringLiteral("-");
}

// One line for logs and tests.
QString describe(const DeviationRow& row) {
  return QStringLiteral("%1 %2 (%3%4): volume change %5 mm3 against the file's %6 (%7); faces up to %8 mm off, %9 "
                        "of their area beyond %10 mm; body %11 mm3 against the stored %12")
      .arg(row.index)
      .arg(row.name, row.type, row.component.isEmpty() ? QString() : QStringLiteral(" in %1").arg(row.component),
           mm3(row.volumeChange), mm3(row.fileVolumeChange), signedPercent(row.relative),
           row.largest ? QString::number(*row.largest, 'f', 3) : QStringLiteral("-"), percent(row.beyond))
      .arg(row.tolerance)
      .arg(mm3(row.bodyVolume), mm3(row.storedBodyVolume)) +
         (row.missingEdges.isEmpty()
              ? QString()
              : QStringLiteral("; without edges %1").arg(row.missingEdges.join(QStringLiteral(", "))));
}

// The table: Space toggles the current row's choice.
class DeviationTable : public QTableWidget {
public:
  using QTableWidget::QTableWidget;

protected:
  void keyPressEvent(QKeyEvent* event) override {
    if (event->key() == Qt::Key_Space && currentRow() >= 0) {
      QTableWidgetItem* keep = item(currentRow(), 0);
      if (keep != nullptr) {
        keep->setCheckState(keep->checkState() == Qt::Checked ? Qt::Unchecked : Qt::Checked);
      }
      return;
    }
    QTableWidget::keyPressEvent(event);
  }
};

} // namespace

std::vector<DeviationRow> deviationRows(const QJsonObject& result) {
  std::vector<DeviationRow> rows;
  const QJsonArray designs = result.value(QStringLiteral("report")).toObject().value(QStringLiteral("designs")).toArray();
  if (designs.isEmpty()) {
    return rows;
  }
  for (const QJsonValue& value : designs.first().toObject().value(QStringLiteral("items")).toArray()) {
    const QJsonObject item = value.toObject();
    const QJsonObject deviation = item.value(QStringLiteral("deviation")).toObject();
    if (deviation.isEmpty()) {
      continue;
    }
    DeviationRow row;
    row.index = static_cast<qint64>(item.value(QStringLiteral("index")).toDouble());
    row.name = item.value(QStringLiteral("name")).toString();
    row.type = item.value(QStringLiteral("type")).toString();
    row.component = item.value(QStringLiteral("component")).toString();
    row.volumeChange = deviation.value(QStringLiteral("volume_change")).toDouble();
    row.fileVolumeChange = deviation.value(QStringLiteral("file_volume_change")).toDouble();
    row.difference = deviation.value(QStringLiteral("volume_difference")).toDouble();
    row.relative = number(deviation, "relative_difference");
    row.largest = number(deviation, "largest_distance");
    row.beyond = number(deviation, "area_beyond");
    row.fileLargest = number(deviation, "file_largest_distance");
    row.bodyVolume = deviation.value(QStringLiteral("body_volume")).toDouble();
    row.storedBodyVolume = deviation.value(QStringLiteral("stored_body_volume")).toDouble();
    row.tolerance = deviation.value(QStringLiteral("tolerance")).toDouble(0.01);
    row.reason = mitcad::modelMessage(deviation.value(QStringLiteral("reason")).toString());
    row.faces = deviation.value(QStringLiteral("faces")).toArray();
    for (const QJsonValue& edge : deviation.value(QStringLiteral("missing_edges")).toArray()) {
      row.missingEdges << edge.toString();
    }
    rows.push_back(row);
  }
  return rows;
}

QSet<qint64> askDeviations(QWidget* parent, const QString& file, const std::vector<DeviationRow>& rows,
                           const std::function<void(const DeviationRow*)>& highlight) {
  QSet<qint64> keep;
  if (rows.empty()) {
    return keep;
  }
  qInfo().noquote() << QStringLiteral("Import deviations: %1: %2 items").arg(file).arg(rows.size());
  for (const DeviationRow& row : rows) {
    qInfo().noquote() << QStringLiteral("Import deviation: %1: %2").arg(file, describe(row));
  }
  QDialog dialog(parent);
  dialog.setObjectName(QStringLiteral("importDeviations"));
  dialog.setWindowTitle(QObject::tr("Keep Deviating Results?"));
  auto* layout = new QVBoxLayout(&dialog);
  auto* intro = new QLabel(
      QObject::tr("These timeline items built a result that deviates from the bodies stored in %1, and came in as "
                  "the file's stored geometry. An item kept parametric keeps its own result, with the deviation shown "
                  "below; the items after it are replayed on that result, and the import runs again to do so.")
          .arg(file.toHtmlEscaped()));
  intro->setWordWrap(true);
  layout->addWidget(intro);
  const QStringList headers{QObject::tr("Keep Parametric"),
                            QObject::tr("Component"),
                            QObject::tr("Item"),
                            QObject::tr("Type"),
                            QObject::tr("Volume Change (mm³)"),
                            QObject::tr("File's Change (mm³)"),
                            QObject::tr("Difference"),
                            QObject::tr("Faces Off (mm)"),
                            QObject::tr("Area Beyond"),
                            QObject::tr("Body / Stored (mm³)")};
  auto* table = new DeviationTable(static_cast<int>(rows.size()), static_cast<int>(headers.size()), &dialog);
  table->setObjectName(QStringLiteral("importDeviationsTable"));
  table->setHorizontalHeaderLabels(headers);
  table->setSelectionBehavior(QAbstractItemView::SelectRows);
  table->setSelectionMode(QAbstractItemView::SingleSelection);
  table->setEditTriggers(QAbstractItemView::NoEditTriggers);
  table->verticalHeader()->setVisible(false);
  mac::styleListTable(table);
  for (int r = 0; r < static_cast<int>(rows.size()); ++r) {
    const DeviationRow& row = rows[static_cast<std::size_t>(r)];
    auto* choice = new QTableWidgetItem(QObject::tr("Use stored"));
    choice->setFlags(Qt::ItemIsUserCheckable | Qt::ItemIsEnabled | Qt::ItemIsSelectable);
    choice->setCheckState(Qt::Unchecked);
    choice->setToolTip(QObject::tr("Checked: the item stays parametric; unchecked: the file's stored geometry"));
    table->setItem(r, 0, choice);
    const auto text = [&](int column, const QString& value, const QString& tip = QString()) {
      auto* cell = new QTableWidgetItem(value);
      cell->setToolTip(tip.isEmpty() ? row.reason : tip);
      table->setItem(r, column, cell);
    };
    text(1, row.component.isEmpty() ? QObject::tr("(root)") : row.component);
    if (row.missingEdges.isEmpty()) {
      text(2, row.name);
    } else {
      text(2, QObject::tr("%1 (without %n edge(s))", nullptr, static_cast<int>(row.missingEdges.size())).arg(row.name),
           QObject::tr("The geometry kernel does not build the rounding with all its edges; built without %1")
               .arg(row.missingEdges.join(QStringLiteral(", "))));
    }
    text(3, row.type);
    text(4, mm3(row.volumeChange));
    text(5, mm3(row.fileVolumeChange));
    text(6, QStringLiteral("%1 (%2)").arg(mm3(row.difference), signedPercent(row.relative)));
    text(7, row.largest ? QString::number(*row.largest, 'f', 3) : QStringLiteral("-"),
         row.fileLargest ? QObject::tr("Its new faces up to %1 mm from the file's; the file's new faces up to %2 mm "
                                       "from its result")
                               .arg(row.largest.value_or(0.0), 0, 'f', 3)
                               .arg(*row.fileLargest, 0, 'f', 3)
                         : QString());
    text(8, percent(row.beyond),
         QObject::tr("The share of its new faces' area farther than %1 mm from the file's").arg(row.tolerance));
    text(9, QStringLiteral("%1 / %2").arg(mm3(row.bodyVolume), mm3(row.storedBodyVolume)));
  }
  table->resizeColumnsToContents();
  layout->addWidget(table);
  auto* faces = new QLabel(&dialog);
  faces->setWordWrap(true);
  layout->addWidget(faces);
  QObject::connect(table, &QTableWidget::itemSelectionChanged, &dialog, [&] {
    const int r = table->currentRow();
    const DeviationRow* row = r >= 0 && table->selectionModel()->hasSelection() ? &rows[static_cast<std::size_t>(r)]
                                                                                  : nullptr;
    if (row != nullptr) {
      faces->setText(row->faces.isEmpty()
                         ? QObject::tr("%1: no face of the design is left where it deviates (later items changed it).")
                               .arg(row->name)
                         : QObject::tr("%1: %n face(s) where it deviates are selected in the view.", nullptr,
                                       static_cast<int>(row->faces.size()))
                               .arg(row->name));
      qInfo().noquote()
          << QStringLiteral("Import deviation shown: %1: %2, %3 faces").arg(file, row->name).arg(row->faces.size());
    } else {
      faces->clear();
    }
    if (highlight) {
      highlight(row);
    }
  });
  QObject::connect(table, &QTableWidget::itemChanged, &dialog, [&](QTableWidgetItem* item) {
    if (item->column() != 0) {
      return;
    }
    const bool kept = item->checkState() == Qt::Checked;
    item->setText(kept ? QObject::tr("Keep parametric") : QObject::tr("Use stored"));
    qInfo().noquote() << QStringLiteral("Import deviation choice: %1: %2 %3")
                             .arg(file, rows[static_cast<std::size_t>(item->row())].name,
                                  kept ? QStringLiteral("keep") : QStringLiteral("stored"));
  });
  auto* buttons = new QDialogButtonBox(&dialog);
  QPushButton* all = buttons->addButton(QObject::tr("&Keep All Parametric"), QDialogButtonBox::ActionRole);
  all->setToolTip(QObject::tr("Every item listed stays parametric with its own result"));
  QPushButton* none = buttons->addButton(QObject::tr("&Use Stored for All"), QDialogButtonBox::ActionRole);
  none->setToolTip(QObject::tr("The file's stored geometry stands in for every item listed (as imported)"));
  QPushButton* apply = buttons->addButton(QObject::tr("&Apply"), QDialogButtonBox::AcceptRole);
  apply->setToolTip(QObject::tr("Runs the import again with the checked items kept parametric"));
  QPushButton* cancel = buttons->addButton(QDialogButtonBox::Cancel);
  cancel->setToolTip(QObject::tr("Leave the design as imported: the file's stored geometry for every item listed"));
  for (QPushButton* button : {all, none, cancel}) {
    button->setAutoDefault(false);
  }
  apply->setDefault(true);
  const auto checkAll = [table](Qt::CheckState state) {
    for (int r = 0; r < table->rowCount(); ++r) {
      table->item(r, 0)->setCheckState(state);
    }
  };
  QObject::connect(all, &QPushButton::clicked, &dialog, [&] {
    checkAll(Qt::Checked);
    dialog.accept();
  });
  QObject::connect(none, &QPushButton::clicked, &dialog, [&] {
    checkAll(Qt::Unchecked);
    dialog.accept();
  });
  mac::styleDialogButtons(buttons, apply, true);
  mac::styleDialog(&dialog);
  QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
  QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
  layout->addWidget(buttons);
  dialog.resize(980, 360);
  table->setFocus();
  table->selectRow(0);
  prepareModal(&dialog);
  const bool accepted = execDialog(&dialog) == QDialog::Accepted;
  if (highlight) {
    highlight(nullptr);
  }
  if (accepted) {
    for (int r = 0; r < table->rowCount(); ++r) {
      if (table->item(r, 0)->checkState() == Qt::Checked) {
        keep.insert(rows[static_cast<std::size_t>(r)].index);
      }
    }
  }
  QStringList kept;
  for (const DeviationRow& row : rows) {
    if (keep.contains(row.index)) {
      kept << QStringLiteral("%1 (%2)").arg(row.name).arg(row.index);
    }
  }
  qInfo().noquote() << QStringLiteral("Import deviations answer: %1: %2")
                           .arg(file, kept.isEmpty() ? QStringLiteral("stored for all")
                                                     : QStringLiteral("keep %1").arg(kept.join(QStringLiteral(", "))));
  return keep;
}

} // namespace mitcad
