// SPDX-License-Identifier: MIT
#include "LinkWatcher.hpp"

#include <utility>

#include <QFileInfo>
#include <QFileSystemWatcher>
#include <QtLogging>

namespace mitcad {
namespace {

// How long a file must be quiet before it counts as changed: a save's
// temporary file and its rename come within it.
constexpr int kSettleMs = 400;

} // namespace

LinkWatcher::LinkWatcher(QObject* parent) : QObject(parent) {
  m_watcher = new QFileSystemWatcher(this);
  m_settle.setSingleShot(true);
  m_settle.setInterval(kSettleMs);
  connect(m_watcher, &QFileSystemWatcher::fileChanged, this, &LinkWatcher::touched);
  // The folder sees a file renamed over the watched one (the watch on the
  // old file is gone then).
  connect(m_watcher, &QFileSystemWatcher::directoryChanged, this, [this](const QString& folder) {
    for (const QString& file : std::as_const(m_files)) {
      if (QFileInfo(file).absolutePath() == folder && !m_watcher->files().contains(file) && QFileInfo::exists(file)) {
        touched(file);
      }
    }
  });
  connect(&m_settle, &QTimer::timeout, this, &LinkWatcher::settle);
}

void LinkWatcher::setFiles(const QStringList& files) {
  QStringList wanted = files;
  wanted.removeDuplicates();
  wanted.sort();
  if (wanted == m_files) {
    return;
  }
  m_files = wanted;
  if (!m_watcher->files().isEmpty()) {
    m_watcher->removePaths(m_watcher->files());
  }
  if (!m_watcher->directories().isEmpty()) {
    m_watcher->removePaths(m_watcher->directories());
  }
  QStringList folders;
  for (const QString& file : std::as_const(m_files)) {
    if (QFileInfo::exists(file)) {
      m_watcher->addPath(file);
    }
    folders << QFileInfo(file).absolutePath();
  }
  folders.removeDuplicates();
  for (const QString& folder : std::as_const(folders)) {
    if (QFileInfo(folder).isDir()) {
      m_watcher->addPath(folder);
    }
  }
  m_pending.clear();
  qDebug().noquote() << QStringLiteral("Linked files watched: %1").arg(m_files.size());
}

void LinkWatcher::touched(const QString& file) {
  if (!m_files.contains(file)) {
    return;
  }
  m_pending.insert(file);
  m_settle.start();
}

void LinkWatcher::settle() {
  const QSet<QString> pending = std::exchange(m_pending, {});
  for (const QString& file : pending) {
    // Renamed over: watched again now that it is back.
    if (QFileInfo::exists(file) && !m_watcher->files().contains(file)) {
      m_watcher->addPath(file);
    }
    if (!QFileInfo::exists(file)) {
      continue; // removed (for now): nothing to read
    }
    qDebug().noquote() << QStringLiteral("Linked file changed on disk: %1").arg(file);
    emit changed(file);
  }
}

} // namespace mitcad
