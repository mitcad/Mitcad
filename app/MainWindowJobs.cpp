// SPDX-License-Identifier: MIT
// Background computation (P7, docs/architecture.md): the main window runs
// jobs on the model's worker thread and waits for them on the UI thread
// without freezing. Opening a design is a job, and so is every model
// command and preview; their callers stay synchronous.
#include "MainWindow.hpp"

#include <algorithm>
#include <chrono>
#include <cstdint>
#include <memory>

#include <QApplication>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QPointer>
#include <QTimer>
#include <QtLogging>

#include "framework/ComputeProgress.hpp"
#include "framework/Diagnostics.hpp"
#include "framework/Json.hpp"
#include "files/LockController.hpp"

namespace mitcad {
namespace {

using std::chrono::milliseconds;

// The UI thread blocks this long for a job before it draws again, so that
// quick jobs change nothing in how the window behaves.
constexpr milliseconds kBlockFor(50);
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
  if (modelBusy()) {
    // Started while waiting for another job (from a timer or a queued
    // signal): the model is that job's.
    qWarning().noquote() << QStringLiteral("Computing %1 refused: the model is busy").arg(label);
    throw ModelBusy();
  }
  // Nothing computes while the view draws: its picks come out of a frame.
  Q_ASSERT(!m_viewer->isPainting());
  ModelJob state;
  state.setStage(stage);
  if (const int delay = testRecomputeDelay(label); delay > 0) {
    state.control().set_test_delay(static_cast<std::uint32_t>(delay));
  }
  bool logged = log == JobLog::Always;
  if (logged) {
    qDebug().noquote() << QStringLiteral("Computing %1 started").arg(label);
  }
  QElapsedTimer clock;
  clock.start();
  m_job = &state;
  std::exception_ptr error;
  if (computeInline()) {
    try {
      job(state);
    } catch (...) {
      error = std::current_exception();
    }
  } else {
    m_worker->post([&job, &state] { job(state); });
    try {
      if (!m_worker->waitFor(kBlockFor)) {
        waitForJob(label, state, clock, logged);
      }
    } catch (...) {
      // The job uses what is here: it ends before this does.
      state.control().cancel();
      m_worker->take();
      m_job = nullptr;
      throw;
    }
    error = m_worker->take();
  }
  m_job = nullptr;
  if (m_closePending) {
    // Closing was asked for meanwhile (closeEvent): now it can.
    m_closePending = false;
    QTimer::singleShot(0, this, [this] { close(); });
  }
  if (!m_idleCalls.empty() && !m_idleCallsPosted) {
    // What waited for the model, after this job's caller is done with it.
    m_idleCallsPosted = true;
    QTimer::singleShot(0, this, &MainWindow::runIdleCalls);
  }
  const qint64 elapsed = clock.elapsed();
  if (error) {
    // Whatever stopped a job that was asked to stop, it changed nothing:
    // a cancel.
    if (state.control().is_cancelled()) {
      qDebug().noquote() << QStringLiteral("Computing %1 cancelled after %2 ms").arg(label).arg(elapsed);
      return false;
    }
    std::rethrow_exception(error);
  }
  // A job that ended although a cancel came late did all it was to do.
  if (logged) {
    qDebug().noquote() << QStringLiteral("Computing %1 done in %2 ms (%3 evaluated)")
                              .arg(label)
                              .arg(elapsed)
                              .arg(static_cast<qulonglong>(state.control().progress().evaluated));
  }
  return true;
}

void MainWindow::waitForJob(const QString& label, ModelJob& job, const QElapsedTimer& clock, bool& logged) {
  if (!logged) {
    logged = true;
    qDebug().noquote() << QStringLiteral("Computing %1 started").arg(label);
  }
  QApplication::setOverrideCursor(Qt::WaitCursor);
  bool cursorSet = true;
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
      dialog->show();
      // The dialog takes clicks; the cursor says so.
      QApplication::restoreOverrideCursor();
      cursorSet = false;
      qDebug().noquote() << QStringLiteral("Progress dialog shown: %1").arg(label);
    }
    loop.exec(dialog ? QEventLoop::AllEvents : QEventLoop::ExcludeUserInputEvents);
    // A loop returns at once while the application quits: wait a little.
    m_worker->waitFor(milliseconds(dialog ? 10 : 0));
  }
  disconnect(done);
  if (cursorSet) {
    QApplication::restoreOverrideCursor();
  }
  if (dialog) {
    // The input goes back to the window that had it (or the one the dialog
    // blocked) while the application is in front: without a window
    // manager, as on a test's X server, nothing else gives it back.
    const bool front = QApplication::activeWindow() == dialog.get();
    QPointer<QWidget> back = before ? before.data() : dialog->parentWidget()->window();
    dialog.reset();
    if (front && back && back->isVisible()) {
      back->activateWindow();
    }
  }
}

void MainWindow::whenIdle(QObject* context, std::function<void()> call) {
  m_idleCalls.push_back({QPointer<QObject>(context), std::move(call)});
  if (!modelBusy() && !m_idleCallsPosted) {
    m_idleCallsPosted = true;
    QTimer::singleShot(0, this, &MainWindow::runIdleCalls);
  }
}

void MainWindow::runIdleCalls() {
  m_idleCallsPosted = false;
  // A call may compute (a job), and what its wait lets through may queue
  // more: those run after it, in order. While a job runs, the rest wait for
  // its end (runJob posts this again).
  while (!m_idleCalls.empty() && !modelBusy()) {
    IdleCall next = std::move(m_idleCalls.front());
    m_idleCalls.pop_front();
    if (next.context) {
      next.call();
    }
  }
}

// ---------------------------------------------------------------------------
// Model commands and previews as jobs

QJsonObject MainWindow::command(const QJsonObject& command) {
  const QString name = command.value(QStringLiteral("cmd")).toString();
  // Every change of the document passes here: a read-only window
  // (mitcad#89) refuses what would change the design.
  if (windowReadOnly() && !m_writeAnyway && !readOnlyModelCommand(name)) {
    throw ReadOnlyError(readOnlyReason().toStdString());
  }
  // What the progress dialog says: "Undo", "Set parameter".
  QString stage = name;
  stage.replace(QLatin1Char('_'), QLatin1Char(' ')).replace(QLatin1Char('.'), QLatin1Char(' '));
  if (!stage.isEmpty()) {
    stage[0] = stage[0].toUpper();
  }
  const QByteArray json = compactJson(command);
  ScopedTiming timing("command");
  timing.setDetail(name);
  Document& document = *m_document;
  rust::String answer;
  const bool done = runJob(
      name, stage,
      [&document, &json, &answer](ModelJob& job) {
        const AttachedJob attached(document, job);
        answer = document.command(rustStr(json));
      },
      JobLog::WhenSlow);
  if (!done) {
    throw ComputationCancelled();
  }
  timing.finish();
  return parseObject(answer);
}

QJsonObject MainWindow::modelPreview(const QByteArray& command, const QString& name) {
  ScopedTiming timing("preview");
  timing.setDetail(name);
  Document& document = *m_document;
  rust::String answer;
  const bool done = runJob(
      QStringLiteral("preview"), tr("Preview of %1").arg(name),
      [&document, &command, &answer](ModelJob& job) {
        const AttachedJob attached(document, job);
        answer = document.preview(rustStr(command));
      },
      JobLog::WhenSlow);
  if (!done) {
    throw ComputationCancelled();
  }
  timing.finish();
  return parseObject(answer);
}

} // namespace mitcad
