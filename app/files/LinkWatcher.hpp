// SPDX-License-Identifier: MIT
#pragma once

#include <QObject>
#include <QSet>
#include <QString>
#include <QStringList>
#include <QTimer>

class QFileSystemWatcher;

namespace mitcad {

// Watches the files of a design's linked designs (mitcad#1416): changed()
// once a file stopped changing for a moment. A save writes a temporary file
// and renames it over the old one, which a watcher sees as the file
// removed: the file is watched again as soon as it is back. Nothing is
// read or updated here; the window asks the model what changed.
//
// Logged: `Linked files watched: <n>` when the files change, `Linked file
// changed on disk: <file>`.
class LinkWatcher : public QObject {
  Q_OBJECT

public:
  explicit LinkWatcher(QObject* parent = nullptr);

  // The files to watch (absolute paths); the others are no longer watched.
  void setFiles(const QStringList& files);
  const QStringList& files() const { return m_files; }

signals:
  void changed(const QString& file);

private:
  void touched(const QString& file);
  void settle();

  QFileSystemWatcher* m_watcher = nullptr;
  QStringList m_files;
  QSet<QString> m_pending;
  QTimer m_settle;
};

} // namespace mitcad
