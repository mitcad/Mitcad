// SPDX-License-Identifier: MIT
#include "CachePreferences.hpp"

#include <algorithm>
#include <utility>

#include <QCheckBox>
#include <QDesktopServices>
#include <QDir>
#include <QFormLayout>
#include <QHBoxLayout>
#include <QJsonObject>
#include <QLabel>
#include <QLineEdit>
#include <QPushButton>
#include <QSpinBox>
#include <QUrl>
#include <QtLogging>

#include "Json.hpp"
#include "mitcad_bridge/lib.h"

namespace mitcad {

CachePreferencesBox::CachePreferencesBox(std::function<void()> diagnostics, QWidget* parent)
#ifdef Q_OS_MACOS
    : QWidget(parent),
#else
    : QGroupBox(tr("Cache"), parent),
#endif
      m_settings(CacheSettings::load()) {
  auto* form = new QFormLayout(this);
  m_memorySize = new QSpinBox;
  m_memorySize->setRange(CacheSettings::kMinMemoryMegabytes,
                         static_cast<int>(std::max<qint64>(CacheSettings::kMinMemoryMegabytes,
                                                           CacheSettings::physicalMegabytes())));
  m_memorySize->setSingleStep(256);
  m_memorySize->setSuffix(tr(" MB"));
  m_memorySize->setValue(static_cast<int>(m_settings.memoryMegabytes));
  m_memorySize->setToolTip(tr("Computed results used longest ago leave the memory beyond this (a quarter of the "
                              "machine's memory by default); those on disk come back from there"));
  auto* showDiagnostics = new QPushButton(tr("D&iagnostics..."));
  connect(showDiagnostics, &QPushButton::clicked, this, [diagnostics] {
    if (diagnostics) {
      diagnostics();
    }
  });
  auto* memoryRow = new QHBoxLayout;
  memoryRow->addWidget(m_memorySize);
  memoryRow->addStretch(1);
  memoryRow->addWidget(showDiagnostics);
  form->addRow(tr("In memory, at most:"), memoryRow);
  m_disk = new QCheckBox(tr("&Keep computed results on disk, at most"));
  m_disk->setChecked(m_settings.disk);
  m_disk->setToolTip(tr("Opening a design again takes its costly features from there instead of "
                        "computing them again"));
  m_diskSize = new QSpinBox;
  m_diskSize->setRange(CacheSettings::kMinDiskGigabytes, CacheSettings::kMaxDiskGigabytes);
  m_diskSize->setSuffix(tr(" GB"));
  m_diskSize->setValue(static_cast<int>((m_settings.diskMegabytes + 512) / 1024));
  m_diskSize->setEnabled(m_settings.disk);
  connect(m_disk, &QCheckBox::toggled, m_diskSize, &QWidget::setEnabled);
  auto* diskRow = new QHBoxLayout;
  diskRow->addWidget(m_disk);
  diskRow->addWidget(m_diskSize);
  diskRow->addStretch(1);
#ifdef Q_OS_MACOS
  form->addRow(QString(), diskRow); // under the other controls, as macOS aligns them
#else
  form->addRow(diskRow);
#endif

  const QString folder = resultStoreLocation();
  auto* folderPath = new QLineEdit(QDir::toNativeSeparators(folder));
  folderPath->setReadOnly(true);
  folderPath->setToolTip(tr("The results used longest ago go when there is more than the size above"));
  auto* openFolder = new QPushButton(tr("Open Folder"));
  openFolder->setEnabled(!folder.isEmpty());
  connect(openFolder, &QPushButton::clicked, this, [folder] {
    QDir().mkpath(folder);
    QDesktopServices::openUrl(QUrl::fromLocalFile(folder));
  });
  auto* clear = new QPushButton(tr("C&lear"));
  clear->setEnabled(!folder.isEmpty());
  clear->setToolTip(tr("Removes every computed result kept on disk"));
  connect(clear, &QPushButton::clicked, this, &CachePreferencesBox::clearDisk);
  auto* folderRow = new QHBoxLayout;
  folderRow->addWidget(folderPath, 1);
  folderRow->addWidget(openFolder);
  folderRow->addWidget(clear);
  form->addRow(tr("Results folder:"), folderRow);
  connect(m_disk, &QCheckBox::toggled, this, &CachePreferencesBox::changed);
  connect(m_diskSize, &QSpinBox::valueChanged, this, &CachePreferencesBox::changed);
  connect(m_memorySize, &QSpinBox::valueChanged, this, &CachePreferencesBox::changed);
  m_cleared = new QLabel;
  m_cleared->hide();
#ifdef Q_OS_MACOS
  form->addRow(QString(), m_cleared);
#else
  form->addRow(m_cleared);
#endif
}

CacheSettings CachePreferencesBox::chosen() const {
  CacheSettings chosen = m_settings;
  chosen.disk = m_disk->isChecked();
  chosen.diskMegabytes = qint64(m_diskSize->value()) * 1024;
  chosen.memoryMegabytes = m_memorySize->value();
  return chosen;
}

void CachePreferencesBox::clearDisk() {
  const QString folder = resultStoreLocation();
  if (folder.isEmpty()) {
    return;
  }
  const QJsonObject report = parseObject(result_store_clear(rustStr(folder.toUtf8())));
  const qint64 removed = report.value(QStringLiteral("removed")).toInteger();
  const double megabytes = report.value(QStringLiteral("removed_bytes")).toDouble() / 1048576.0;
  m_cleared->setText(tr("Removed %n result file(s), %1 MB.", nullptr, static_cast<int>(removed))
                         .arg(megabytes, 0, 'f', 1));
  m_cleared->show();
  qInfo().noquote() << QStringLiteral("Result store cleared: %1 files, %2 MB").arg(removed).arg(megabytes, 0, 'f', 1);
}

} // namespace mitcad
