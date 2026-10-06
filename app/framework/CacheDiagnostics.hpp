// SPDX-License-Identifier: MIT
#pragma once

// Help > Diagnostics (P7d): what the caches of computed results hold and
// do. The memory cache and the result store on disk: their sizes against
// their limits, hits and misses, results dropped, the store's reads and
// writes with their times; what takes the space by feature, by feature
// type and (on disk) by document, the largest first; where each result of
// the last recompute came from and how long it took; and the process's
// memory. Clear Memory, Clear Disk and Export Report. The numbers are the
// model's `cache` query (core/model/src/api/commands.md).

#include <functional>

#include <QDialog>
#include <QJsonObject>
#include <QString>

class QLabel;
class QTreeWidget;

namespace mitcad {

class CacheDiagnosticsDialog : public QDialog {
  Q_OBJECT

public:
  // What the dialog asks of the window: the `cache` query (with the
  // store's files and the process's memory), and the memory cache
  // cleared (the clear_cache command's answer).
  struct Host {
    std::function<QJsonObject()> report;
    std::function<QJsonObject()> clearMemory;
  };

  CacheDiagnosticsDialog(Host host, QWidget* parent);

  // The report as text, for Export Report and the log.
  static QString reportText(const QJsonObject& report);

private:
  void refresh();
  void clearMemory();
  void clearDisk();
  void exportReport();

  Host m_host;
  QJsonObject m_report;
  QLabel* m_summary = nullptr;
  QTreeWidget* m_memoryFeatures = nullptr;
  QTreeWidget* m_memoryTypes = nullptr;
  QTreeWidget* m_diskDocuments = nullptr;
  QTreeWidget* m_diskFeatures = nullptr;
  QTreeWidget* m_diskTypes = nullptr;
  QTreeWidget* m_lastRecompute = nullptr;
};

} // namespace mitcad
