// SPDX-License-Identifier: MIT
#pragma once

#include <QObject>
#include <QString>
#include <QStringList>

#include <functional>

namespace mitcad {

// Takes the files the system asks the application to open: on macOS, a
// double-click or Open With in Finder, a file dropped on the Dock icon and
// `open -a Mitcad file` arrive as a QFileOpenEvent at the QApplication,
// possibly before the main window exists (elsewhere none ever arrives).
// Installed on the application; the paths wait until setHandler says the
// window is ready, and then come one at a time, never while a modal dialog
// (a question at start-up, a save prompt) runs.
class FileOpenFilter : public QObject {
public:
  explicit FileOpenFilter(QObject* parent = nullptr);

  // The window is ready: opens what has been queued and what comes later.
  void setHandler(std::function<void(const QString&)> handler);

protected:
  bool eventFilter(QObject* watched, QEvent* event) override;

private:
  void schedule();
  void deliverNext();

  std::function<void(const QString&)> m_handler;
  QStringList m_pending;
  bool m_scheduled = false;
};

} // namespace mitcad
