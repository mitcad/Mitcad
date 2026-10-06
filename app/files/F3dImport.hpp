// SPDX-License-Identifier: MIT
#pragma once

#include <QElapsedTimer>
#include <QJsonObject>
#include <QObject>
#include <QPointer>
#include <QString>
#include <QTemporaryDir>

class QProcess;
class QTimer;
class QWidget;

namespace mitcad {

class ImportProgress;

// An .f3d design (.f3d, .f3z) imported with its history (U6, the model's
// import_f3d), or a FreeCAD document (.FCStd) with its stored bodies
// (import_fcstd), in a process of its own: the application runs itself
// with --import-worker, and a crash of the geometry kernel cannot take the
// window with it. The import runs to its end; a progress dialog shows the
// timeline item being replayed and offers two ways to end it early (T1e):
// Stop and Keep What Is Imported asks the worker to stop (the remaining
// items take the file's bodies, and the report says where it stopped; not
// for a FreeCAD document, which has no timeline to replay yet), and
// Cancel Import stops the process, leaving the open document as it is.
class F3dImport : public QObject {
  Q_OBJECT

public:
  F3dImport(const QString& path, QWidget* window);
  ~F3dImport() override;

  void start();

signals:
  // ok: `project` is the imported document's project file (JSON) and
  // `result` the import_f3d command's result with its report. Not ok:
  // `error` says why (cancelled: empty).
  void finished(bool ok, const QByteArray& project, const QJsonObject& result, const QString& error);

private:
  void readProgress();
  void processFinished(int exitCode);
  void stop();
  void cancel();
  void tick();
  void done(bool ok, const QString& error);

  QString m_path;
  QPointer<QWidget> m_window;
  QTemporaryDir m_dir;
  QProcess* m_process = nullptr;
  ImportProgress* m_dialog = nullptr;
  QTimer* m_ticker = nullptr;
  QElapsedTimer m_clock;
  QByteArray m_errors; // the worker's standard error, line by line
  int m_items = 0;     // timeline items, when the worker said
  int m_done = 0;      // items replayed
  int m_position = 0;  // the item being replayed (from 1)
  QString m_current;   // its name
  QString m_lastLine;  // the worker's last message
  bool m_stopping = false;
  bool m_finished = false;
};

// The worker process: imports the file given after --import-worker and
// writes the project (--output) and the command's result (--result). A
// line "stop" on its standard input stops the import early. Returns the
// process's exit code.
int runImportWorker(int argc, char* argv[]);

// Whether the command line asks for the worker.
bool isImportWorker(int argc, char* argv[]);

} // namespace mitcad
