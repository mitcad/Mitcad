// SPDX-License-Identifier: MIT
// View > Export Snapshot (mitcad#649, mitcad#702; app/COMMANDS.md
// "Annotations"): a picture of the view, larger than the view, with the
// shown annotations (or one group's) drawn over it.
#include <map>
#include <optional>

#include <QDir>
#include <QImage>
#include <QtLogging>

#include "../browser/DocumentHost.hpp"
#include "../framework/DialogFolders.hpp"
#include "SnapshotDialog.hpp"
#include "ViewController.hpp"

namespace mitcad {

void ViewController::exportSnapshot() {
  const QString name = (m_documentName.isEmpty() ? tr("Snapshot") : m_documentName) + QStringLiteral(".png");
  const QString folder = dialogStartFolder(DialogGroup::Export, m_host.documentFolder());
  const QString start = folder.isEmpty() ? name : QDir(folder).filePath(name);
  const int shown = static_cast<int>(m_viewer.measurements().size());
  // The groups of the shown annotations, each with how many it shows.
  std::map<QString, int> counts;
  for (const MeasurementDisplay& m : m_viewer.measurements()) {
    if (!m.group.isEmpty()) {
      ++counts[m.group];
    }
  }
  QList<QPair<QString, int>> groups;
  for (const auto& [group, count] : counts) {
    groups.append(qMakePair(group, count));
  }
  const std::optional<SnapshotChoice> choice = askSnapshot(m_window, start, m_viewer.size(), shown, groups);
  if (!choice) {
    return;
  }
  rememberDialogFolder(DialogGroup::Export, choice->path);
  const SnapshotChoice chosen = *choice;
  // The view's shapes are read only while no model job writes into them.
  m_host.whenIdle(this, [this, chosen] {
    const QImage image = m_viewer.snapshot(chosen.size, chosen.transparent, chosen.measurements, chosen.group);
    const QString shownPath = QDir::toNativeSeparators(chosen.path);
    if (image.isNull() || !image.save(chosen.path)) {
      m_host.showStatus(tr("The snapshot cannot be saved as %1.").arg(shownPath), NoticeLevel::Error);
      qWarning().noquote() << QStringLiteral("Snapshot not saved: %1 (%2)")
                                  .arg(chosen.path, image.isNull() ? QStringLiteral("the view rendered no picture")
                                                                   : QStringLiteral("the file cannot be written"));
      return;
    }
    m_host.showStatus(tr("Snapshot saved as %1.").arg(shownPath), NoticeLevel::Information);
    int drawn = 0;
    if (chosen.measurements) {
      for (const MeasurementDisplay& m : m_viewer.measurements()) {
        drawn += chosen.group.isEmpty() || m.group == chosen.group ? 1 : 0;
      }
    }
    qInfo().noquote() << QStringLiteral("Snapshot saved %1: %2 x %3, %4, %5 annotation(s)%6")
                             .arg(chosen.path)
                             .arg(image.width())
                             .arg(image.height())
                             .arg(image.hasAlphaChannel() ? QStringLiteral("transparent") : QStringLiteral("opaque"))
                             .arg(drawn)
                             .arg(chosen.group.isEmpty() ? QString() : QStringLiteral(" of group ") + chosen.group);
  });
}

} // namespace mitcad
