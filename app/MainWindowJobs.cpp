// SPDX-License-Identifier: MIT
// Background computation (P7, docs/architecture.md): the main window
// presents progress while DocumentSession runs jobs on its model worker.
// The legacy Qt callers stay synchronous; the native frontend can also
// use the session's asynchronous command and preview entry points.
#include "MainWindow.hpp"

#include <algorithm>
#include <chrono>
#include <exception>
#include <memory>
#include <thread>

#include <QApplication>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QPointer>
#include <QTimer>
#include <QtLogging>

#include "framework/Command.hpp"
#include "framework/CommandRegistry.hpp"
#include "framework/ComputeProgress.hpp"
#include "framework/Dialogs.hpp"
#include "framework/Json.hpp"
#include "framework/TestSync.hpp"
#include "framework/WaitCursor.hpp"
#include "files/LockController.hpp"
#ifdef MITCAD_MACOS_NATIVE_UI
#include "platform/NativeDialogs.hpp"
#endif

namespace mitcad {
namespace {

using std::chrono::milliseconds;

// Until then it draws while input waits in Qt's queue; a job that takes
// longer gets the progress dialog, and input goes to that.
constexpr qint64 kDialogAfterMs = 400;
// How often the progress is read meanwhile, and logged at most.
constexpr int kTickMs = 100;
constexpr qint64 kLogEveryMs = 200;

QString featureOf(const JobProgress& progress) {
  return QString::fromUtf8(progress.feature.data(), static_cast<qsizetype>(progress.feature.size()));
}

// "57 of 196, Extrude6": the feature being computed.
QString positionOf(const JobProgress& progress) {
  const auto shown = static_cast<qulonglong>(std::min(progress.position + 1, progress.total));
  return QStringLiteral("%1 of %2, %3")
      .arg(shown)
      .arg(static_cast<qulonglong>(progress.total))
      .arg(featureOf(progress));
}

} // namespace

bool MainWindow::runJob(const QString& label, const QString& stage, const std::function<void(ModelJob&)>& job,
                        JobLog log) {
  // View picks are produced inside a frame; the presentation may not
  // begin a model computation until that frame has finished.
  Q_ASSERT(!m_viewer->isPainting());
  return m_documentSession->runJob(label, stage, job, log);
}

void MainWindow::waitForJob(const QString& label, ModelJob& job, const QElapsedTimer& clock, bool& logged) {
  if (!logged) {
    logged = true;
    qDebug().noquote() << QStringLiteral("Computing %1 started").arg(label);
  }
  // Scoped (mitcad#268): restored once, by the progress dialog or at the
  // end, also when the wait ends early or waits nest; logged.
  WaitCursor cursor(label);
  std::unique_ptr<ComputeProgress> dialog;
  QPointer<QWidget> before; // the window that had the input when it showed
  QEventLoop loop;
  // A wait ends when the worker is done, and when the dialog is due.
  const QMetaObject::Connection done = connect(m_worker, &ModelWorker::jobDone, &loop, &QEventLoop::quit);
  QString shown;
  qint64 shownAt = -kLogEveryMs;
  const auto tick = [&] {
    const JobProgress progress = job.control().progress();
    const qint64 elapsed = clock.elapsed();
    if (progress.total > 0) {
      const QString at = positionOf(progress);
      if (at != shown && elapsed - shownAt >= kLogEveryMs) {
        qDebug().noquote() << QStringLiteral("Computing %1: %2").arg(label, at);
        shown = at;
        shownAt = elapsed;
      }
    }
    if (dialog) {
      dialog->showProgress(job.stage(), progress, elapsed);
    } else if (elapsed >= kDialogAfterMs) {
      loop.quit();
    }
  };
  QTimer ticker;
  ticker.setInterval(kTickMs);
  connect(&ticker, &QTimer::timeout, &loop, tick);
  ticker.start();
  while (!m_worker->waitFor(milliseconds(0))) {
    if (!dialog && clock.elapsed() >= kDialogAfterMs) {
      // Modal to what has the input now: the window, or a dialog of its
      // own (Change Parameters), whose input it then blocks.
      QWidget* blocked = QApplication::activeModalWidget();
      before = QApplication::activeWindow();
      dialog = std::make_unique<ComputeProgress>(blocked != nullptr ? blocked : this);
      connect(dialog.get(), &ComputeProgress::cancelRequested, this, [&job, &label, &clock] {
        qDebug().noquote() << QStringLiteral("Computing %1: cancel requested after %2 ms").arg(label).arg(clock.elapsed());
        job.control().cancel();
      });
      dialog->showProgress(job.stage(), job.control().progress(), clock.elapsed());
      showDialog(dialog.get());
      // The dialog takes clicks; the cursor says so.
      cursor.release();
      qDebug().noquote() << QStringLiteral("Progress dialog shown: %1").arg(label);
    }
#ifdef MITCAD_MACOS_NATIVE_UI
    if (m_nativeUi) {
      waitNativeUntil([&] {
        return m_worker->waitFor(milliseconds(0)) || (!dialog && clock.elapsed() >= kDialogAfterMs);
      });
    } else
#endif
    {
      loop.exec(dialog ? QEventLoop::AllEvents : QEventLoop::ExcludeUserInputEvents);
    }
    // A loop returns at once while the application quits: wait a little.
    m_worker->waitFor(milliseconds(dialog ? 10 : 0));
  }
  disconnect(done);
  cursor.release();
  if (dialog) {
    // The input goes back to the window that had it (or the one the dialog
    // blocked) while the application is in front: without a window
    // manager, as on a test's X server, nothing else gives it back.
    const bool front = QApplication::activeWindow() == dialog.get();
    QPointer<QWidget> back = before ? before.data() : dialog->parentWidget()->window();
    dialog.reset();
    if (!m_nativeUi && front && back && back->isVisible()) {
      back->activateWindow();
    }
  }
}

void MainWindow::whenIdle(QObject* context, std::function<void()> call) {
  m_documentSession->whenIdle(context, std::move(call));
}

// ---------------------------------------------------------------------------
// Model commands and previews as jobs

QJsonObject MainWindow::command(const QJsonObject& command) {
  Q_ASSERT(!m_viewer->isPainting());
  const QString name = command.value(QStringLiteral("cmd")).toString();
  // Every change of the document passes here: a read-only window
  // (mitcad#89) refuses what would change the design.
  if (windowReadOnly() && !m_writeAnyway && !readOnlyModelCommand(name)) {
    throw ReadOnlyError(readOnlyReason().toStdString());
  }
  QJsonObject answer = m_documentSession->command(command);
  // Edit in Place (mitcad#2321): what the command picked on the assembly,
  // frozen, in the message band.
  if (m_inPlaceHolder && answer.contains(QStringLiteral("in_place_notices"))) {
    showInPlaceNotices(answer);
  }
  return answer;
}

QJsonObject MainWindow::modelPreview(const QByteArray& command, const QString& name) {
  Q_ASSERT(!m_viewer->isPainting());
  return m_documentSession->preview(command, name);
}

bool MainWindow::modelPreviewAsync(const QByteArray& request, const QString& name, QObject* context,
                                  AsyncCompletion completion) {
  Q_ASSERT(!m_viewer->isPainting());
  return m_documentSession->previewAsync(request, name, context, [completion = std::move(completion)](
                                                                       const DocumentSession::AsyncResult& result) {
    completion({result.answer, result.error, result.cancelled});
  });
}

bool MainWindow::modelRuntimeCommandAsync(const QJsonObject& request, QObject* context, AsyncCompletion completion) {
  Q_ASSERT(!m_viewer->isPainting());
  const QString name = request.value(QStringLiteral("cmd")).toString();
  // A read-only window: refused as command() refuses it, by the caller's
  // command.
  if (windowReadOnly() && !m_writeAnyway && !readOnlyModelCommand(name)) {
    return false;
  }
  return m_documentSession->runtimeCommandAsync(request, context, [completion = std::move(completion)](
                                                                       const DocumentSession::AsyncResult& result) {
    completion({result.answer, result.error, result.cancelled});
  });
}

// ---------------------------------------------------------------------------
// The wait cursor's idle recovery (mitcad#268)

void MainWindow::createWaitCursorRecovery() {
  // Right after each job, and every second for a cursor left on otherwise.
  connect(m_documentSession, &DocumentSession::busyChanged, this, [this](bool busy) {
    if (!busy) {
      TestSync::singleShot(0, this, [this] {
        if (!modelBusy()) {
          WaitCursor::recoverIfIdle();
        }
      });
    }
  });
  auto* timer = new QTimer(this);
  timer->setInterval(1000);
  connect(timer, &QTimer::timeout, this, [this] {
    if (!modelBusy()) {
      WaitCursor::recoverIfIdle();
    }
  });
  timer->start();
}

void MainWindow::registerWaitCursorTestCommands() {
  if (!WaitCursor::testHooks()) {
    return;
  }
  CommandDef leave;
  leave.id = QStringLiteral("test.leave_wait_cursor");
  leave.name = QStringLiteral("Leave Wait Cursor On (Test)");
  leave.icon = QStringLiteral("warning");
  leave.kind = CommandDef::Kind::Action;
  leave.mode = CommandDef::Mode::Any;
  leave.tooltip = QStringLiteral("For tests: an override cursor set and left on, which the idle recovery clears");
  leave.run = [] { WaitCursor::leaveOnForTest(QStringLiteral("test hook")); };
  m_registry->add(leave);
  CommandDef nested = leave;
  nested.id = QStringLiteral("test.nested_wait");
  nested.name = QStringLiteral("Nested Wait (Test)");
  nested.mode = CommandDef::Mode::Model;
  nested.tooltip = QStringLiteral("For tests: a job of two seconds whose wait is waited for again inside it");
  nested.run = [this] { nestedWaitForTest(); };
  m_registry->add(nested);
}

void MainWindow::nestedWaitForTest() {
  const QString label = QStringLiteral("nested wait test");
  // While the job runs and its wait holds the cursor, a second wait for
  // the same job starts inside the first one's event loop: it shows the
  // progress dialog (its cursor goes then), and the first wait ends with
  // the job once the second has returned.
  TestSync::singleShot(200, this, [this, label] {
    if (ModelJob* job = m_documentSession->activeJob()) {
      QElapsedTimer clock;
      clock.start();
      bool logged = true;
      waitForJob(label + QStringLiteral(" (nested)"), *job, clock, logged);
    }
  });
  try {
    runJob(label, tr("Waiting for a test"),
           [](ModelJob&) { std::this_thread::sleep_for(milliseconds(2000)); }, JobLog::Always);
  } catch (const std::exception& error) {
    showError(errorText(error));
  }
}

} // namespace mitcad
