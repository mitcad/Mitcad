// SPDX-License-Identifier: MIT
#include "CacheDiagnostics.hpp"

#include <utility>

#include <QDialogButtonBox>
#include <QFileDialog>
#include <QFileInfo>
#include <QHBoxLayout>
#include <QHeaderView>
#include <QJsonArray>
#include <QJsonDocument>
#include <QLabel>
#include <QMessageBox>
#include <QPushButton>
#include <QSaveFile>
#include <QStringList>
#include <QTabWidget>
#include <QTreeWidget>
#include <QVBoxLayout>
#include <QtLogging>

#include "Json.hpp"
#include "ResultCache.hpp"
#include "mitcad_bridge/lib.h"

namespace mitcad {
namespace {

QString megabytes(double bytes) {
  return QStringLiteral("%1 MB").arg(bytes / 1048576.0, 0, 'f', 1);
}

QString count(const QJsonValue& value) { return QString::number(value.toInteger()); }

// "a of b (c %)", or "a" without a limit.
QString ofLimit(double bytes, const QJsonValue& limit) {
  if (!limit.isDouble() || limit.toDouble() <= 0.0) {
    return megabytes(bytes);
  }
  return QStringLiteral("%1 of %2 (%3 %)")
      .arg(megabytes(bytes), megabytes(limit.toDouble()))
      .arg(100.0 * bytes / limit.toDouble(), 0, 'f', 1);
}

QTreeWidget* table(const QStringList& columns) {
  auto* tree = new QTreeWidget;
  tree->setColumnCount(static_cast<int>(columns.size()));
  tree->setHeaderLabels(columns);
  tree->setRootIsDecorated(false);
  tree->setUniformRowHeights(true);
  tree->header()->setStretchLastSection(false);
  tree->header()->setSectionResizeMode(0, QHeaderView::Stretch);
  return tree;
}

// Rows of `rows` with the fields `keys`; bytes shown in MB.
void fill(QTreeWidget* tree, const QJsonArray& rows, const QStringList& keys) {
  tree->clear();
  for (const QJsonValue& value : rows) {
    const QJsonObject row = value.toObject();
    QStringList cells;
    for (const QString& key : keys) {
      const QJsonValue cell = row.value(key);
      if (key == QLatin1String("bytes")) {
        cells << megabytes(cell.toDouble());
      } else if (key == QLatin1String("ms")) {
        cells << QString::number(cell.toDouble(), 'f', 1);
      } else if (cell.isDouble()) {
        cells << count(cell);
      } else {
        cells << cell.toString();
      }
    }
    auto* item = new QTreeWidgetItem(tree, cells);
    for (int i = 1; i < static_cast<int>(cells.size()); ++i) {
      if (keys[i] != QLatin1String("type") && keys[i] != QLatin1String("source")) {
        item->setTextAlignment(i, Qt::AlignRight | Qt::AlignVCenter);
      }
    }
  }
  for (int i = 1; i < tree->columnCount(); ++i) {
    tree->resizeColumnToContents(i);
  }
}

// A feature's label: its name and uid.
QJsonArray named(const QJsonArray& rows) {
  QJsonArray out;
  for (const QJsonValue& value : rows) {
    QJsonObject row = value.toObject();
    const QString name = row.value(QStringLiteral("name")).toString();
    const QString uid = row.value(QStringLiteral("uid")).toString();
    row.insert(QStringLiteral("feature"), name.isEmpty() ? uid : QStringLiteral("%1 (%2)").arg(name, uid));
    out.append(row);
  }
  return out;
}

} // namespace

CacheDiagnosticsDialog::CacheDiagnosticsDialog(Host host, QWidget* parent)
    : QDialog(parent), m_host(std::move(host)) {
  setWindowTitle(tr("Diagnostics"));
  resize(760, 620);
  auto* layout = new QVBoxLayout(this);
  m_summary = new QLabel;
  m_summary->setTextInteractionFlags(Qt::TextSelectableByMouse);
  m_summary->setWordWrap(true);
  layout->addWidget(m_summary);

  auto* tabs = new QTabWidget;
  auto* memory = new QWidget;
  auto* memoryLayout = new QVBoxLayout(memory);
  memoryLayout->addWidget(new QLabel(tr("By feature, the largest first:")));
  m_memoryFeatures = table({tr("Feature"), tr("Type"), tr("Results"), tr("Size")});
  memoryLayout->addWidget(m_memoryFeatures, 2);
  memoryLayout->addWidget(new QLabel(tr("By feature type:")));
  m_memoryTypes = table({tr("Type"), tr("Features"), tr("Results"), tr("Size")});
  memoryLayout->addWidget(m_memoryTypes, 1);
  tabs->addTab(memory, tr("Memory"));

  auto* disk = new QWidget;
  auto* diskLayout = new QVBoxLayout(disk);
  diskLayout->addWidget(new QLabel(tr("By document:")));
  m_diskDocuments = table({tr("Document"), tr("Features"), tr("Results"), tr("Size")});
  diskLayout->addWidget(m_diskDocuments, 1);
  diskLayout->addWidget(new QLabel(tr("By feature, the largest first:")));
  m_diskFeatures = table({tr("Feature"), tr("Document"), tr("Type"), tr("Results"), tr("Size")});
  diskLayout->addWidget(m_diskFeatures, 2);
  diskLayout->addWidget(new QLabel(tr("By feature type:")));
  m_diskTypes = table({tr("Type"), tr("Features"), tr("Results"), tr("Size")});
  diskLayout->addWidget(m_diskTypes, 1);
  tabs->addTab(disk, tr("Disk"));

  auto* last = new QWidget;
  auto* lastLayout = new QVBoxLayout(last);
  lastLayout->addWidget(new QLabel(tr("Each feature of the last recompute, in timeline order: computed, "
                                      "from the memory cache or from the disk, and the time it took:")));
  m_lastRecompute = table({tr("Feature"), tr("Type"), tr("Source"), tr("Time (ms)"), tr("Computed in (ms)")});
  lastLayout->addWidget(m_lastRecompute);
  tabs->addTab(last, tr("Last Recompute"));
  layout->addWidget(tabs, 1);

  auto* buttons = new QDialogButtonBox(QDialogButtonBox::Close);
  auto* refreshButton = buttons->addButton(tr("&Refresh"), QDialogButtonBox::ActionRole);
  auto* clearMemoryButton = buttons->addButton(tr("Clear &Memory"), QDialogButtonBox::ActionRole);
  auto* clearDiskButton = buttons->addButton(tr("Clear &Disk"), QDialogButtonBox::ActionRole);
  auto* exportButton = buttons->addButton(tr("&Export Report..."), QDialogButtonBox::ActionRole);
  clearMemoryButton->setToolTip(tr("Drops the computed results the document does not show now"));
  clearDiskButton->setToolTip(tr("Removes every computed result kept on disk"));
  for (QPushButton* button : {refreshButton, clearMemoryButton, clearDiskButton, exportButton}) {
    button->setAutoDefault(false);
  }
  connect(refreshButton, &QPushButton::clicked, this, &CacheDiagnosticsDialog::refresh);
  connect(clearMemoryButton, &QPushButton::clicked, this, &CacheDiagnosticsDialog::clearMemory);
  connect(clearDiskButton, &QPushButton::clicked, this, &CacheDiagnosticsDialog::clearDisk);
  connect(exportButton, &QPushButton::clicked, this, &CacheDiagnosticsDialog::exportReport);
  connect(buttons, &QDialogButtonBox::rejected, this, &QDialog::reject);
  layout->addWidget(buttons);
  refresh();
}

void CacheDiagnosticsDialog::refresh() {
  m_report = m_host.report();
  // The store's limit is the application's setting.
  if (m_report.value(QStringLiteral("store")).isObject()) {
    QJsonObject store = m_report.value(QStringLiteral("store")).toObject();
    store.insert(QStringLiteral("budget"), double(CacheSettings::load().diskMegabytes) * 1048576.0);
    m_report.insert(QStringLiteral("store"), store);
  }
  const QJsonObject memory = m_report.value(QStringLiteral("memory")).toObject();
  const QJsonObject store = m_report.value(QStringLiteral("store")).toObject();
  const QJsonObject diskUsage = store.value(QStringLiteral("disk")).toObject();
  const QJsonObject last = m_report.value(QStringLiteral("last_recompute")).toObject();
  const QJsonObject process = m_report.value(QStringLiteral("process")).toObject();

  QString html = QStringLiteral("<table cellspacing='4'>");
  const auto row = [&html](const QString& what, const QString& value) {
    html += QStringLiteral("<tr><td><b>%1</b></td><td>%2</td></tr>").arg(what.toHtmlEscaped(), value.toHtmlEscaped());
  };
  row(tr("Memory cache"),
      tr("%1, %2 results of %3 features; %4 lookups, %5 hits, %6 misses, %7 from the disk; %8 results dropped "
         "(%9)")
          .arg(ofLimit(memory.value(QStringLiteral("bytes")).toDouble(), memory.value(QStringLiteral("budget"))),
               count(memory.value(QStringLiteral("results"))), count(memory.value(QStringLiteral("features"))),
               count(memory.value(QStringLiteral("lookups"))), count(memory.value(QStringLiteral("hits"))),
               count(memory.value(QStringLiteral("misses"))), count(memory.value(QStringLiteral("restored"))),
               count(memory.value(QStringLiteral("evicted"))),
               megabytes(memory.value(QStringLiteral("evicted_bytes")).toDouble())));
  if (store.isEmpty()) {
    row(tr("Disk cache"), tr("off"));
  } else {
    row(tr("Disk cache"),
        tr("%1 in %2 files (%3 of other versions of Mitcad); %4 lookups, %5 hits, %6 misses, %7 damaged")
            .arg(ofLimit(diskUsage.value(QStringLiteral("bytes")).toDouble(), store.value(QStringLiteral("budget"))),
                 count(diskUsage.value(QStringLiteral("files"))),
                 count(diskUsage.value(QStringLiteral("other_builds")).toObject().value(QStringLiteral("files"))),
                 count(store.value(QStringLiteral("lookups"))), count(store.value(QStringLiteral("hits"))),
                 count(store.value(QStringLiteral("misses"))), count(store.value(QStringLiteral("damaged")))));
    row(tr("Disk reads, writes"),
        tr("%1 read in %2 ms; %3 results written, %4 in %5 ms, %6 failed")
            .arg(megabytes(store.value(QStringLiteral("read_bytes")).toDouble()))
            .arg(store.value(QStringLiteral("read_ms")).toDouble(), 0, 'f', 1)
            .arg(count(store.value(QStringLiteral("writes"))),
                 megabytes(store.value(QStringLiteral("write_bytes")).toDouble()))
            .arg(store.value(QStringLiteral("write_ms")).toDouble(), 0, 'f', 1)
            .arg(count(store.value(QStringLiteral("write_errors")))));
    row(tr("Folder"), store.value(QStringLiteral("dir")).toString());
  }
  row(tr("Last recompute"),
      tr("%1 computed in %2 ms, %3 from the disk in %4 ms, %5 from the memory cache")
          .arg(count(last.value(QStringLiteral("evaluated"))))
          .arg(last.value(QStringLiteral("evaluate_ms")).toDouble(), 0, 'f', 1)
          .arg(count(last.value(QStringLiteral("restored"))))
          .arg(last.value(QStringLiteral("restore_ms")).toDouble(), 0, 'f', 1)
          .arg(count(last.value(QStringLiteral("cached")))));
  row(tr("Process memory"), tr("%1 resident (at most %2), %3 allocated")
                                .arg(megabytes(process.value(QStringLiteral("resident")).toDouble()),
                                     megabytes(process.value(QStringLiteral("peak_resident")).toDouble()),
                                     megabytes(process.value(QStringLiteral("heap")).toDouble())));
  html += QStringLiteral("</table>");
  m_summary->setText(html);

  fill(m_memoryFeatures, named(memory.value(QStringLiteral("by_feature")).toArray()),
       {QStringLiteral("feature"), QStringLiteral("type"), QStringLiteral("results"), QStringLiteral("bytes")});
  fill(m_memoryTypes, memory.value(QStringLiteral("by_type")).toArray(),
       {QStringLiteral("type"), QStringLiteral("features"), QStringLiteral("results"), QStringLiteral("bytes")});
  fill(m_diskDocuments, diskUsage.value(QStringLiteral("by_document")).toArray(),
       {QStringLiteral("document"), QStringLiteral("features"), QStringLiteral("results"), QStringLiteral("bytes")});
  fill(m_diskFeatures, named(diskUsage.value(QStringLiteral("by_feature")).toArray()),
       {QStringLiteral("feature"), QStringLiteral("document"), QStringLiteral("type"), QStringLiteral("results"),
        QStringLiteral("bytes")});
  fill(m_diskTypes, diskUsage.value(QStringLiteral("by_type")).toArray(),
       {QStringLiteral("type"), QStringLiteral("features"), QStringLiteral("results"), QStringLiteral("bytes")});
  fill(m_lastRecompute, named(last.value(QStringLiteral("features")).toArray()),
       {QStringLiteral("feature"), QStringLiteral("type"), QStringLiteral("source"), QStringLiteral("ms"),
        QStringLiteral("evaluated_ms")});

  qDebug().noquote() << QStringLiteral("Diagnostics: memory %1 of %2 bytes (%3 results), disk %4 bytes (%5 files), "
                                       "process %6 bytes; last recompute %7 evaluated, %8 from the store, %9 cached")
                            .arg(memory.value(QStringLiteral("bytes")).toInteger())
                            .arg(memory.value(QStringLiteral("budget")).toInteger())
                            .arg(memory.value(QStringLiteral("results")).toInteger())
                            .arg(diskUsage.value(QStringLiteral("bytes")).toInteger())
                            .arg(diskUsage.value(QStringLiteral("files")).toInteger())
                            .arg(process.value(QStringLiteral("resident")).toInteger())
                            .arg(last.value(QStringLiteral("evaluated")).toInteger())
                            .arg(last.value(QStringLiteral("restored")).toInteger())
                            .arg(last.value(QStringLiteral("cached")).toInteger());
}

void CacheDiagnosticsDialog::clearMemory() {
  const QJsonObject answer = m_host.clearMemory();
  qInfo().noquote() << QStringLiteral("Diagnostics: cleared memory: %1 results, %2 bytes")
                           .arg(answer.value(QStringLiteral("results")).toInteger())
                           .arg(answer.value(QStringLiteral("bytes")).toInteger());
  refresh();
}

void CacheDiagnosticsDialog::clearDisk() {
  const QString folder = resultStoreLocation();
  if (folder.isEmpty()) {
    return;
  }
  const QJsonObject report = parseObject(result_store_clear(rustStr(folder.toUtf8())));
  qInfo().noquote() << QStringLiteral("Diagnostics: cleared disk: %1 files, %2 bytes")
                           .arg(report.value(QStringLiteral("removed")).toInteger())
                           .arg(report.value(QStringLiteral("removed_bytes")).toInteger());
  refresh();
}

QString CacheDiagnosticsDialog::reportText(const QJsonObject& report) {
  QString text = QStringLiteral("Mitcad cache diagnostics\n\n");
  const QJsonObject memory = report.value(QStringLiteral("memory")).toObject();
  text += QStringLiteral("Memory cache: %1 bytes of %2, %3 results of %4 features; %5 lookups, %6 hits, %7 "
                         "from the store; %8 dropped (%9 bytes)\n")
              .arg(memory.value(QStringLiteral("bytes")).toInteger())
              .arg(memory.value(QStringLiteral("budget")).isNull()
                       ? QStringLiteral("no limit")
                       : QString::number(memory.value(QStringLiteral("budget")).toInteger()))
              .arg(memory.value(QStringLiteral("results")).toInteger())
              .arg(memory.value(QStringLiteral("features")).toInteger())
              .arg(memory.value(QStringLiteral("lookups")).toInteger())
              .arg(memory.value(QStringLiteral("hits")).toInteger())
              .arg(memory.value(QStringLiteral("restored")).toInteger())
              .arg(memory.value(QStringLiteral("evicted")).toInteger())
              .arg(memory.value(QStringLiteral("evicted_bytes")).toInteger());
  const auto rows = [&text](const QString& title, const QJsonArray& array, const QStringList& keys) {
    text += QStringLiteral("\n%1\n").arg(title);
    for (const QJsonValue& value : array) {
      QStringList cells;
      for (const QString& key : keys) {
        const QJsonValue cell = value.toObject().value(key);
        cells << (cell.isDouble() ? QString::number(cell.toDouble(), 'g', 12) : cell.toString());
      }
      text += QStringLiteral("  ") + cells.join(QStringLiteral("\t")) + QLatin1Char('\n');
    }
  };
  rows(QStringLiteral("Memory by feature (uid, name, type, results, bytes):"),
       memory.value(QStringLiteral("by_feature")).toArray(),
       {QStringLiteral("uid"), QStringLiteral("name"), QStringLiteral("type"), QStringLiteral("results"),
        QStringLiteral("bytes")});
  rows(QStringLiteral("Memory by type (type, features, results, bytes):"), memory.value(QStringLiteral("by_type")).toArray(),
       {QStringLiteral("type"), QStringLiteral("features"), QStringLiteral("results"), QStringLiteral("bytes")});
  const QJsonObject store = report.value(QStringLiteral("store")).toObject();
  if (!store.isEmpty()) {
    const QJsonObject disk = store.value(QStringLiteral("disk")).toObject();
    text += QStringLiteral("\nDisk cache %1: %2 bytes in %3 files; %4 lookups, %5 hits, %6 misses, %7 damaged; "
                           "%8 bytes read in %9 ms; %10 written (%11 bytes) in %12 ms\n")
                .arg(store.value(QStringLiteral("dir")).toString())
                .arg(disk.value(QStringLiteral("bytes")).toInteger())
                .arg(disk.value(QStringLiteral("files")).toInteger())
                .arg(store.value(QStringLiteral("lookups")).toInteger())
                .arg(store.value(QStringLiteral("hits")).toInteger())
                .arg(store.value(QStringLiteral("misses")).toInteger())
                .arg(store.value(QStringLiteral("damaged")).toInteger())
                .arg(store.value(QStringLiteral("read_bytes")).toInteger())
                .arg(store.value(QStringLiteral("read_ms")).toDouble())
                .arg(store.value(QStringLiteral("writes")).toInteger())
                .arg(store.value(QStringLiteral("write_bytes")).toInteger())
                .arg(store.value(QStringLiteral("write_ms")).toDouble());
    rows(QStringLiteral("Disk by document (document, features, results, bytes):"),
         disk.value(QStringLiteral("by_document")).toArray(),
         {QStringLiteral("document"), QStringLiteral("features"), QStringLiteral("results"), QStringLiteral("bytes")});
    rows(QStringLiteral("Disk by feature (document, uid, name, type, results, bytes):"),
         disk.value(QStringLiteral("by_feature")).toArray(),
         {QStringLiteral("document"), QStringLiteral("uid"), QStringLiteral("name"), QStringLiteral("type"),
          QStringLiteral("results"), QStringLiteral("bytes")});
    rows(QStringLiteral("Disk by type (type, features, results, bytes):"), disk.value(QStringLiteral("by_type")).toArray(),
         {QStringLiteral("type"), QStringLiteral("features"), QStringLiteral("results"), QStringLiteral("bytes")});
  }
  const QJsonObject last = report.value(QStringLiteral("last_recompute")).toObject();
  rows(QStringLiteral("Last recompute (uid, name, type, source, ms):"), last.value(QStringLiteral("features")).toArray(),
       {QStringLiteral("uid"), QStringLiteral("name"), QStringLiteral("type"), QStringLiteral("source"),
        QStringLiteral("ms")});
  const QJsonObject process = report.value(QStringLiteral("process")).toObject();
  text += QStringLiteral("\nProcess memory: %1 bytes resident, at most %2, %3 allocated\n")
              .arg(process.value(QStringLiteral("resident")).toInteger())
              .arg(process.value(QStringLiteral("peak_resident")).toInteger())
              .arg(process.value(QStringLiteral("heap")).toInteger());
  return text;
}

void CacheDiagnosticsDialog::exportReport() {
  QString filter;
  const QString path = QFileDialog::getSaveFileName(this, tr("Export Report"), QString(),
                                                    tr("JSON (*.json);;Text (*.txt)"), &filter);
  if (path.isEmpty()) {
    return;
  }
  const bool json = QFileInfo(path).suffix().compare(QLatin1String("txt"), Qt::CaseInsensitive) != 0;
  const QByteArray content =
      json ? QJsonDocument(m_report).toJson(QJsonDocument::Indented) : reportText(m_report).toUtf8();
  QSaveFile file(path);
  if (!file.open(QIODevice::WriteOnly) || file.write(content) != content.size() || !file.commit()) {
    QMessageBox::warning(this, tr("Diagnostics"), tr("Could not write %1:\n%2").arg(path, file.errorString()));
    return;
  }
  qInfo().noquote() << "Diagnostics report written to" << path;
}

} // namespace mitcad
