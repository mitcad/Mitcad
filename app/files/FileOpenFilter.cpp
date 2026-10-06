// SPDX-License-Identifier: MIT
#include "files/FileOpenFilter.hpp"

#include <QApplication>
#include <QEvent>
#include <QFileOpenEvent>
#include <QTimer>
#include <QUrl>
#include <QtLogging>

#include <utility>

namespace mitcad {

FileOpenFilter::FileOpenFilter(QObject* parent) : QObject(parent) {}

void FileOpenFilter::setHandler(std::function<void(const QString&)> handler) {
  m_handler = std::move(handler);
  schedule();
}

bool FileOpenFilter::eventFilter(QObject* watched, QEvent* event) {
  if (event->type() != QEvent::FileOpen) {
    return QObject::eventFilter(watched, event);
  }
  const auto* open = static_cast<QFileOpenEvent*>(event);
  // A URL of another kind (a custom scheme) is no file and of no use here.
  QString path = open->file();
  if (path.isEmpty() && open->url().isLocalFile()) {
    path = open->url().toLocalFile();
  }
  if (path.isEmpty()) {
    qDebug().noquote() << QStringLiteral("File open event ignored: %1").arg(open->url().toString());
    return true;
  }
  qDebug().noquote() << QStringLiteral("File open event: %1").arg(path);
  m_pending << path;
  schedule();
  return true;
}

void FileOpenFilter::schedule() {
  if (!m_handler || m_pending.isEmpty() || m_scheduled) {
    return;
  }
  m_scheduled = true;
  QTimer::singleShot(0, this, [this] { deliverNext(); });
}

void FileOpenFilter::deliverNext() {
  m_scheduled = false;
  if (!m_handler || m_pending.isEmpty()) {
    return;
  }
  if (QApplication::activeModalWidget() != nullptr) {
    // A dialog waits for an answer: try again shortly.
    m_scheduled = true;
    QTimer::singleShot(200, this, [this] { deliverNext(); });
    return;
  }
  const QString path = m_pending.takeFirst();
  // The handler can run a dialog's event loop, in which more files arrive:
  // the next round is scheduled before it starts.
  schedule();
  m_handler(path);
}

} // namespace mitcad
