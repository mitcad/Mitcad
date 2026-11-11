// SPDX-License-Identifier: MIT
#include "DocumentSession.hpp"

#include <algorithm>
#include <chrono>
#include <cstdint>
#include <exception>
#include <stdexcept>
#include <thread>
#include <utility>

#include <QEventLoop>
#include <QFileInfo>
#include <QJsonDocument>
#include <QList>
#include <QtLogging>

#include "AppSettings.hpp"
#include "CommandSession.hpp"
#include "Diagnostics.hpp"
#include "DisplayMeshes.hpp"
#include "Json.hpp"
#include "ResultCache.hpp"
#include "platform/MacChrome.hpp"

namespace mitcad {
namespace {

constexpr std::chrono::milliseconds kBlockFor(50);

// The background evaluation's label for the tests' delays
// (MITCAD_TEST_RECOMPUTE_DELAY_MS="background evaluation=…").
const QString& backgroundLabel() {
  static const QString label = QStringLiteral("background evaluation");
  return label;
}

// MITCAD_BACKGROUND_EVALUATION=0 turns the background evaluation off
// (mitcad#1267); computing inline (MITCAD_COMPUTE_INLINE) does too.
bool backgroundEvaluationEnabled() {
  static const bool enabled = qEnvironmentVariable("MITCAD_BACKGROUND_EVALUATION") != QLatin1String("0");
  return enabled && !computeInline();
}

// Sessions belong to the application thread; models in flight stay on
// their worker until whenIdle delivers the current share.
QList<DocumentSession*>& documentSessions() {
  static QList<DocumentSession*> sessions;
  return sessions;
}

QString commandStage(QString name) {
  name.replace(QLatin1Char('_'), QLatin1Char(' ')).replace(QLatin1Char('.'), QLatin1Char(' '));
  if (!name.isEmpty()) {
    name[0] = name[0].toUpper();
  }
  return name;
}

} // namespace

struct DocumentSession::AsyncJob {
  ModelJob job;
  QString label;
  QElapsedTimer clock;
  std::shared_ptr<AsyncResult> result;
  QPointer<QObject> context;
  Completion completion;
  // A job in the background that gives way to the UI thread's own work
  // (commandTaskAsync, yieldBackgroundJob; mitcad#1121).
  bool yields = false;
};

// A run of the rolled-back features (mitcad#1267): its control (the run's
// own monitor, cancelled from the session's thread), the run, made from the
// idle document on the session's thread, and what the log line says.
struct DocumentSession::BackgroundEvaluation {
  explicit BackgroundEvaluation(const Document& document) : run(document.background_run(job.control())) {}
  ModelJob job;
  rust::Box<BackgroundRun> run;
  QElapsedTimer clock;
  QString cancelledBy;
};

rust::Box<Document> newDesignDocument() {
  const QString units = GeneralSettings::load().newDesignUnits;
  return new_design_document_in(rustStr(units.toUtf8()));
}

DocumentSession::State::State(bool demo) : document(demo ? new_demo_document() : newDesignDocument()) {}

DocumentSession::DocumentSession(bool demo, QObject* parent)
    : QObject(parent), m_state(demo), m_worker(std::make_unique<ModelWorker>()) {
  markSaved();
  m_state.document->set_result_store(rustStr(resultStoreDirectory().toUtf8()),
                                    rustStr(resultStoreBuildId().toUtf8()), rustStr(tr("Untitled").toUtf8()));
  documentSessions().append(this);
  refreshMemoryCacheBudgets();
  collectResultStoreGarbage();
  connect(m_worker.get(), &ModelWorker::jobDone, this, &DocumentSession::completeAsync, Qt::QueuedConnection);
  m_progressTimer.setInterval(100);
  connect(&m_progressTimer, &QTimer::timeout, this, [this] {
    if (!m_async || !m_job) {
      return;
    }
    const JobProgress progress = m_job->control().progress();
    emit jobProgress(m_job->stage(), static_cast<quint64>(progress.position),
                     static_cast<quint64>(progress.total), m_async->clock.elapsed());
  });
  QString startupError;
  if (!m_worker->ensureStarted(startupError)) {
    m_state.lastError = startupError;
    qWarning().noquote() << startupError;
  }
  // The background evaluation's thread (mitcad#1267), started with its
  // first run at the lowest priority, so that it takes only what the
  // processors have left.
  m_backgroundWorker = std::make_unique<ModelWorker>(nullptr, [](QThread& thread) {
    thread.start(QThread::IdlePriority);
  });
  m_backgroundWorker->setObjectName(QStringLiteral("background evaluation"));
  connect(m_backgroundWorker.get(), &ModelWorker::jobDone, this, [this] {
    // (A job that started meanwhile ended the run already.)
    if (m_background && m_backgroundWorker->waitFor(std::chrono::milliseconds(0))) {
      finishBackgroundEvaluation(true);
    }
  }, Qt::QueuedConnection);
  notifyChanged();
}

DocumentSession::~DocumentSession() {
  m_progressTimer.stop();
  // The run holds a fork of the document: it stops before the document goes.
  stopBackgroundEvaluation(QStringLiteral("closing"), false);
  cancelJob();
  // The worker must release the document before the owned job and model
  // are destroyed. Queued completions cannot run during this destruction.
  m_worker.reset();
  m_job = nullptr;
  m_async.reset();
  delete m_state.activeCommand;
  documentSessions().removeAll(this);
  refreshMemoryCacheBudgets();
}

qint64 DocumentSession::memoryCacheMegabytes() const {
  return std::max<qint64>(1, CacheSettings::load().memoryMegabytes / std::max<qsizetype>(1, documentSessions().size()));
}

void DocumentSession::refreshMemoryCacheBudgets() {
  for (DocumentSession* session : documentSessions()) {
    if (!session->busy()) {
      session->m_state.document->set_memory_cache_budget(static_cast<std::uint64_t>(session->memoryCacheMegabytes()));
    } else if (!session->m_cacheBudgetPending) {
      session->m_cacheBudgetPending = true;
      session->whenIdle(session, [session] {
        session->m_cacheBudgetPending = false;
        session->m_state.document->set_memory_cache_budget(static_cast<std::uint64_t>(session->memoryCacheMegabytes()));
      });
    }
  }
}

const Document& DocumentSession::idleDocument() const {
  if (busy()) {
    throw ModelBusy();
  }
  return *m_state.document;
}

QJsonValue DocumentSession::query(const QJsonObject& request) const {
  const QJsonDocument answer = parseJson(idleDocument().query(rustStr(compactJson(request))));
  return answer.isArray() ? QJsonValue(answer.array()) : QJsonValue(answer.object());
}

std::shared_ptr<geometry::Shape> DocumentSession::bodyShape(const QString& uid) const {
  return idleDocument().body_shape(uid.toStdString());
}

// A preview's body or tool as the view draws it: its stand-in of the
// interactive tier while it has one (DisplayMeshes.hpp, mitcad#635).
std::shared_ptr<geometry::Shape> DocumentSession::previewBodyShape(const std::string& uid) const {
  return displayShape(idleDocument().preview_body_shape(uid));
}

std::shared_ptr<geometry::Shape> DocumentSession::previewToolShape() const {
  return displayShape(idleDocument().preview_tool_shape());
}

int DocumentSession::undoDepth() const {
  return query({{QStringLiteral("query"), QStringLiteral("document")}}).toObject()
      .value(QStringLiteral("undo_depth")).toInt();
}

bool DocumentSession::modified() const {
  return query({{QStringLiteral("query"), QStringLiteral("document")}}).toObject()
      .value(QStringLiteral("modified")).toBool();
}

DocumentSession::Snapshot DocumentSession::snapshot() const {
  Snapshot result;
  // During computation the cached info is the last committed state; no
  // document is read concurrently with the model worker.
  result.document = busy() ? m_documentInfo
                          : query({{QStringLiteral("query"), QStringLiteral("document")}}).toObject();
  result.selection = m_state.selection;
  result.mode = m_state.mode;
  result.filePath = m_state.filePath;
  result.name = m_state.filePath.isEmpty()
                    ? (m_state.documentName.isEmpty() ? tr("Untitled") : m_state.documentName)
                    : QFileInfo(m_state.filePath).fileName();
  result.busy = busy();
  result.modified = result.document.value(QStringLiteral("modified")).toBool();
  result.canUndo = !result.document.value(QStringLiteral("undo")).toString().isEmpty();
  result.canRedo = !result.document.value(QStringLiteral("redo")).toString().isEmpty();
  result.lastError = m_state.lastError;
  result.lastCancelled = m_state.lastCancelled;
  return result;
}

bool DocumentSession::runJob(const QString& label, const QString& stage, const Job& job, JobLog log) {
  // A job in the background gives way (mitcad#1121).
  if (busy() && !yieldBackgroundJob()) {
    throw ModelBusy();
  }
  QString startupError;
  if (!computeInline() && !m_worker->ensureStarted(startupError)) {
    m_state.lastError = startupError;
    notifyChanged();
    throw std::runtime_error(startupError.toStdString());
  }
  // The background evaluation reads the bodies the job may write into (the
  // meshes of new bodies that share their faces): it stops first, and the
  // document takes what it evaluated by now (mitcad#1267).
  stopBackgroundEvaluation(QStringLiteral("job %1").arg(label), true);
  if (m_beforeJob) {
    m_beforeJob();
  }
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
  emit busyChanged(true);
  emit jobStarted(label, stage);
  std::exception_ptr error;
  if (computeInline()) {
    try {
      job(state);
    } catch (...) {
      error = std::current_exception();
    }
  } else {
    // For tests, a job made longer after its work (MITCAD_TEST_JOB_SLEEP_MS).
    const int extraMs = testJobSleep(label);
    const auto hold = label == QStringLiteral("edit_feature") ? m_testHoldEdit : nullptr;
    m_worker->post([&job, &state, hold, extraMs] {
      while (hold && hold->load() && !state.control().is_cancelled()) {
        std::this_thread::sleep_for(std::chrono::milliseconds(1));
      }
      job(state);
      if (extraMs > 0) {
        std::this_thread::sleep_for(std::chrono::milliseconds(extraMs));
      }
    });
    try {
      if (!m_worker->waitFor(kBlockFor)) {
        if (m_waitHandler) {
          m_waitHandler(label, state, clock, logged);
        } else {
          waitWithoutPresentation(state);
        }
      }
    } catch (...) {
      state.control().cancel();
      m_worker->take();
      m_lastJobCancelRequested = true;
      finishJob(label, true, QString());
      throw;
    }
    error = m_worker->take();
  }
  // A job the memory guard stopped (mitcad#1480) failed with its command's
  // error, which says why; it was not the user's cancel.
  const bool userCancel = state.control().is_cancelled() && state.control().cancel_reason().empty();
  // Also a cancel that stopped only the job's meshing (mitcad#1267).
  m_lastJobCancelRequested = userCancel;
  const bool cancelled = error && userCancel;
  QString failure;
  if (error && !cancelled) {
    try {
      std::rethrow_exception(error);
    } catch (const std::exception& exception) {
      failure = errorText(exception);
    } catch (...) {
      failure = tr("An unknown computation error occurred.");
    }
  }
  finishJob(label, cancelled, failure);
  if (cancelled) {
    qDebug().noquote() << QStringLiteral("Computing %1 cancelled after %2 ms").arg(label).arg(clock.elapsed());
    return false;
  }
  if (logged) {
    qDebug().noquote() << QStringLiteral("Computing %1 done in %2 ms (%3 evaluated)")
                             .arg(label).arg(clock.elapsed())
                             .arg(static_cast<qulonglong>(state.control().progress().evaluated));
  }
  if (error) {
    std::rethrow_exception(error);
  }
  return true;
}

void DocumentSession::waitWithoutPresentation(ModelJob& job) {
  QEventLoop loop;
  const auto done = connect(m_worker.get(), &ModelWorker::jobDone, &loop, &QEventLoop::quit);
  QTimer ticker;
  ticker.setInterval(100);
  QElapsedTimer clock;
  clock.start();
  connect(&ticker, &QTimer::timeout, &loop, [this, &job, &clock] {
    const JobProgress progress = job.control().progress();
    emit jobProgress(job.stage(), static_cast<quint64>(progress.position),
                     static_cast<quint64>(progress.total), clock.elapsed());
  });
  ticker.start();
  while (!m_worker->waitFor(std::chrono::milliseconds(0))) {
    loop.exec(QEventLoop::ExcludeUserInputEvents);
    m_worker->waitFor(std::chrono::milliseconds(10));
  }
  disconnect(done);
}

QJsonObject DocumentSession::command(const QJsonObject& request) {
  const QString name = request.value(QStringLiteral("cmd")).toString();
  const QByteArray json = compactJson(request);
  rust::String answer;
  Document& document = *m_state.document;
  ScopedTiming timing("command");
  timing.setDetail(name);
  // The bodies the view will draw are meshed in the job (DisplayMeshes.hpp,
  // mitcad#398); a sketch's own commands do not change them.
  const bool display = !name.startsWith(QStringLiteral("sketch."));
  // An export's plan (`plan`: what it would write, mitcad#1109: Send to
  // Slicer's bed face as its panel opens) is a job of its own name, apart
  // from an export that writes, so that logs and the tests' job delays
  // (MITCAD_TEST_JOB_SLEEP_MS=export=…, mitcad#888) tell them apart.
  const QString label = name == QStringLiteral("export") && request.value(QStringLiteral("plan")).toBool()
                            ? QStringLiteral("export plan")
                            : name;
  DisplayMeshing meshing;
  const bool done = runJob(label, commandStage(name), [&document, &json, &answer, display, &meshing](ModelJob& job) {
    // The bodies as the view shows them before the command: the meshes of
    // the faces it leaves as they were (mitcad#635).
    const DisplayShapes before = display ? shownBodyShapes(document) : DisplayShapes();
    {
      const AttachedJob attached(document, job);
      answer = document.command(rustStr(json));
    }
    if (display) {
      meshing = meshForDisplay(shownBodyShapes(document), job, DisplayTier::Final, before);
    }
  }, JobLog::WhenSlow);
  if (!done) {
    throw ComputationCancelled();
  }
  timing.finish();
  logDisplayMeshing(meshing, name);
  return parseObject(answer);
}

QJsonObject DocumentSession::preview(const QByteArray& request, const QString& name) {
  rust::String answer;
  DisplayMeshing meshing;
  Document& document = *m_state.document;
  ScopedTiming timing("preview");
  timing.setDetail(name);
  const bool done = runJob(QStringLiteral("preview"), tr("Preview of %1").arg(name),
                          [&document, &request, &answer, &meshing](ModelJob& job) {
    {
      const AttachedJob attached(document, job);
      answer = document.preview(rustStr(request));
    }
    // The interactive tier: stand-ins (DisplayMeshes.hpp, mitcad#635).
    const QJsonObject report = parseObject(answer);
    meshing = meshForDisplay(previewShapes(document, report), job, DisplayTier::Interactive,
                             previewDonors(document, report));
  }, JobLog::WhenSlow);
  m_standIns = meshing.standIns;
  if (!done) {
    throw ComputationCancelled();
  }
  timing.finish();
  logDisplayMeshing(meshing, QStringLiteral("preview"));
  return parseObject(answer);
}

bool DocumentSession::commandAsync(const QJsonObject& request, QObject* context, Completion completion) {
  const QString name = request.value(QStringLiteral("cmd")).toString();
  const QByteArray json = compactJson(request);
  auto result = std::make_shared<AsyncResult>();
  Document* document = &*m_state.document;
  const bool display = !name.startsWith(QStringLiteral("sketch."));
  auto meshing = std::make_shared<DisplayMeshing>();
  const bool failForTest = !busy() && name == m_testFailCommand;
  if (failForTest) { m_testFailCommand.clear(); }
  return startAsync(name, commandStage(name), [document, json, result, display, meshing, failForTest](ModelJob& job) {
    if (failForTest) { throw std::runtime_error("Injected recovery command failure."); }
    const DisplayShapes before = display ? shownBodyShapes(*document) : DisplayShapes();
    {
      const AttachedJob attached(*document, job);
      result->answer = parseObject(document->command(rustStr(json)));
    }
    if (display) {
      *meshing = meshForDisplay(shownBodyShapes(*document), job, DisplayTier::Final, before);
    }
  }, result, context, [meshing, name, completion = std::move(completion)](const AsyncResult& outcome) {
    logDisplayMeshing(*meshing, name);
    if (completion) { completion(outcome); }
  });
}

bool DocumentSession::previewAsync(const QByteArray& request, const QString& name, QObject* context,
                                   Completion completion) {
  auto result = std::make_shared<AsyncResult>();
  Document* document = &*m_state.document;
  auto meshing = std::make_shared<DisplayMeshing>();
  return startAsync(QStringLiteral("preview"), tr("Preview of %1").arg(name),
                    [document, request, result, meshing](ModelJob& job) {
    {
      const AttachedJob attached(*document, job);
      result->answer = parseObject(document->preview(rustStr(request)));
    }
    *meshing = meshForDisplay(previewShapes(*document, result->answer), job, DisplayTier::Interactive,
                              previewDonors(*document, result->answer));
  }, result, context, [this, meshing, completion = std::move(completion)](const AsyncResult& outcome) {
    m_standIns = meshing->standIns;
    logDisplayMeshing(*meshing, QStringLiteral("preview"));
    if (completion) { completion(outcome); }
  });
}

bool DocumentSession::refineDisplay(const DisplayShapes& bodies) {
  DisplayMeshing meshing;
  Document& document = *m_state.document;
  const bool done = runJob(QStringLiteral("display"), tr("Meshing the bodies for display"),
                           [&document, &bodies, &meshing](ModelJob& job) {
    // The bodies shown give the faces the previewed command left as they
    // were (mitcad#635).
    meshing = meshForDisplay(bodies, job, DisplayTier::Final, shownBodyShapes(document));
  }, JobLog::WhenSlow);
  logDisplayMeshing(meshing, QStringLiteral("preview refinement"));
  return done && meshing.bodies + meshing.ready == bodies.size();
}

bool DocumentSession::refineDisplayAsync(const DisplayShapes& bodies, QObject* context, Completion completion) {
  auto result = std::make_shared<AsyncResult>();
  auto meshing = std::make_shared<DisplayMeshing>();
  Document* document = &*m_state.document;
  return startAsync(QStringLiteral("display"), tr("Meshing the bodies for display"),
                    [document, bodies, meshing](ModelJob& job) {
    *meshing = meshForDisplay(bodies, job, DisplayTier::Final, shownBodyShapes(*document));
    if (job.control().is_cancelled()) {
      throw ComputationCancelled();
    }
  }, result, context, [meshing, completion = std::move(completion)](const AsyncResult& outcome) {
    logDisplayMeshing(*meshing, QStringLiteral("preview refinement"));
    if (completion) { completion(outcome); }
  });
}

bool DocumentSession::remeshDisplayAsync(QObject* context, RemeshCompletion completion) {
  auto result = std::make_shared<AsyncResult>();
  auto remeshing = std::make_shared<DisplayRemeshing>();
  Document* document = &*m_state.document;
  const QString label = m_state.documentName.isEmpty() ? QStringLiteral("untitled") : m_state.documentName;
  return startAsync(QStringLiteral("display"), tr("Meshing the bodies for display"),
                    [document, remeshing](ModelJob& job) {
    *remeshing = remeshForDisplay(shownBodyShapes(*document), job);
    if (job.control().is_cancelled()) {
      throw ComputationCancelled();
    }
  }, result, context, [remeshing, label, completion = std::move(completion)](const AsyncResult& outcome) {
    // No job runs now: the bodies are this thread's to write.
    const std::size_t given = outcome.succeeded() ? giveRemeshed(*remeshing) : 0;
    if (!outcome.succeeded()) {
      remeshing->complete = false;
    }
    logRemeshing(*remeshing, given, label);
    if (completion) { completion(given, outcome); }
  });
}

bool DocumentSession::inspectionAsync(const QString& name, QObject* context, InspectionTask task,
                                      InspectionCompletion completion) {
  auto result = std::make_shared<AsyncResult>();
  auto inspection = std::make_shared<Inspection>();
  Document* document = &*m_state.document;
  return startAsync(QStringLiteral("inspection"), tr("Inspection of %1").arg(name),
                    [document, task = std::move(task), inspection](ModelJob& job) {
    const AttachedJob attached(*document, job);
    if (job.control().is_cancelled()) { throw ComputationCancelled(); }
    *inspection = task(*document);
    // Some direct OCCT measurements cannot interrupt midway. Discard
    // their result if the user cancelled or changed the inspection.
    if (job.control().is_cancelled()) { throw ComputationCancelled(); }
  }, result, context, [inspection, completion = std::move(completion)](const AsyncResult& outcome) {
    if (completion) { completion(*inspection, outcome); }
  });
}

bool DocumentSession::commandTaskAsync(const QString& label, const QString& stage, CommandTask task,
                                       QObject* context, Completion completion) {
  auto result = std::make_shared<AsyncResult>();
  Document* document = &*m_state.document;
  const bool started = startAsync(label, stage, [document, task = std::move(task), result](ModelJob& job) {
    {
      const AttachedJob attached(*document, job);
      result->answer = task([document](const QJsonObject& request) {
        const QByteArray json = compactJson(request);
        return parseObject(document->command(rustStr(json)));
      });
    }
    // A task that went on past a command the cancel stopped: cancelled all
    // the same (yieldBackgroundJob).
    if (job.control().is_cancelled()) {
      throw ComputationCancelled();
    }
  }, result, context, std::move(completion));
  if (started && m_async) {
    m_async->yields = true;
  }
  return started;
}

bool DocumentSession::queryAsync(const QString& label, const QJsonObject& request, QueryRead read,
                                 QObject* context, Completion completion) {
  auto result = std::make_shared<AsyncResult>();
  Document* document = &*m_state.document;
  const QByteArray json = compactJson(request);
  const bool started = startAsync(label, label, [document, json, read = std::move(read)](ModelJob& job) {
    {
      const AttachedJob attached(*document, job);
      const QJsonDocument answer = parseJson(document->query(rustStr(json)));
      if (job.control().is_cancelled()) {
        throw ComputationCancelled();
      }
      read(answer.isArray() ? QJsonValue(answer.array()) : QJsonValue(answer.object()));
    }
    // Read past a cancel: given way all the same (yieldBackgroundJob).
    if (job.control().is_cancelled()) {
      throw ComputationCancelled();
    }
  }, result, context, std::move(completion));
  if (started && m_async) {
    m_async->yields = true;
  }
  return started;
}

bool DocumentSession::yieldBackgroundJob() {
  if (!m_async || !m_async->yields || m_job != &m_async->job) {
    return !busy();
  }
  qDebug().noquote() << QStringLiteral("Computing %1: gives way after %2 ms").arg(m_async->label)
                           .arg(m_async->clock.elapsed());
  m_async->job.control().cancel();
  // The worker stops at the cancel (the test's job sleep too); the UI
  // thread waits for it without an event loop, as runJob's first 50 ms.
  while (!m_worker->waitFor(std::chrono::milliseconds(20))) {
  }
  finishAsync(true);
  return !busy();
}

bool DocumentSession::startAsync(const QString& label, const QString& stage, Job job,
                                 std::shared_ptr<AsyncResult> result, QObject* context, Completion completion) {
  if (busy()) {
    return false;
  }
  QString startupError;
  if (!m_worker->ensureStarted(startupError)) {
    result->error = startupError;
    m_state.lastError = startupError;
    notifyChanged();
    // Complete the refused request normally, without entering the busy
    // state or leaving the frontend waiting for a nonexistent worker.
    QMetaObject::invokeMethod(this,
        [context = QPointer<QObject>(context), completion = std::move(completion), result] {
          if (context && completion) { completion(*result); }
        }, Qt::QueuedConnection);
    return true;
  }
  // As runJob (mitcad#1267).
  stopBackgroundEvaluation(QStringLiteral("job %1").arg(label), true);
  if (m_beforeJob) {
    m_beforeJob();
  }
  m_async = std::make_unique<AsyncJob>();
  m_async->label = label;
  m_async->result = std::move(result);
  m_async->context = context;
  m_async->completion = std::move(completion);
  m_async->job.setStage(stage);
  if (const int delay = testRecomputeDelay(label); delay > 0) {
    m_async->job.control().set_test_delay(static_cast<std::uint32_t>(delay));
  }
  m_async->clock.start();
  m_job = &m_async->job;
  ModelJob* state = m_job;
  const auto hold = label == QStringLiteral("preview") ? m_testHoldPreview : nullptr;
  // For tests, a job made longer after its work, as runJob's
  // (MITCAD_TEST_JOB_SLEEP_MS, mitcad#1121: "bed plan=…").
  const int extraMs = testJobSleep(label);
  m_worker->post([job = std::move(job), state, hold, extraMs] {
    while (hold && hold->load() && !state->control().is_cancelled()) {
      std::this_thread::sleep_for(std::chrono::milliseconds(1));
    }
    job(*state);
    // Cut short by a cancel (a job that gives way, yieldBackgroundJob).
    for (int slept = 0; slept < extraMs && !state->control().is_cancelled(); slept += 10) {
      std::this_thread::sleep_for(std::chrono::milliseconds(10));
    }
  });
  m_progressTimer.start();
  qDebug().noquote() << QStringLiteral("Computing %1 started").arg(label);
  emit busyChanged(true);
  emit jobStarted(label, stage);
  return true;
}

void DocumentSession::completeAsync() { finishAsync(false); }

void DocumentSession::finishAsync(bool queueCompletion) {
  if (!m_async || !m_worker->waitFor(std::chrono::milliseconds(0))) {
    return;
  }
  const std::exception_ptr error = m_worker->take();
  std::unique_ptr<AsyncJob> finished = std::move(m_async);
  m_progressTimer.stop();
  // As runJob: the memory guard's stop is an error (mitcad#1480).
  const bool userCancel =
      finished->job.control().is_cancelled() && finished->job.control().cancel_reason().empty();
  m_lastJobCancelRequested = userCancel;
  finished->result->cancelled = error && userCancel;
  if (error && !finished->result->cancelled) {
    try {
      std::rethrow_exception(error);
    } catch (const std::exception& exception) {
      finished->result->error = errorText(exception);
    } catch (...) {
      finished->result->error = tr("An unknown computation error occurred.");
    }
  }
  m_state.lastCancelled = finished->result->cancelled;
  m_state.lastError = finished->result->error;
  if (finished->result->cancelled) {
    qDebug().noquote() << QStringLiteral("Computing %1 cancelled after %2 ms")
                             .arg(finished->label).arg(finished->clock.elapsed());
  } else {
    // The UI test driver also balances failed jobs with this completion
    // line; failure details remain in jobFinished and the result.
    qDebug().noquote() << QStringLiteral("Computing %1 done in %2 ms (%3 evaluated)")
                             .arg(finished->label).arg(finished->clock.elapsed())
                             .arg(static_cast<qulonglong>(finished->job.control().progress().evaluated));
  }
  finishJob(finished->label, finished->result->cancelled, finished->result->error);
  if (queueCompletion) {
    // Given way to work the UI thread is doing now: the completion after it.
    QMetaObject::invokeMethod(
        this,
        [context = finished->context, completion = std::move(finished->completion), result = finished->result] {
          if (context && completion) {
            completion(*result);
          }
        },
        Qt::QueuedConnection);
    return;
  }
  if (finished->context && finished->completion) {
    finished->completion(*finished->result);
  }
}

void DocumentSession::finishJob(const QString& label, bool cancelled, const QString& error) {
  m_job = nullptr;
  emit busyChanged(false);
  notifyChanged();
  emit jobFinished(label, cancelled, error);
  scheduleBackgroundEvaluation();
  scheduleIdleCalls();
}

// The background evaluation (mitcad#1267).

void DocumentSession::scheduleBackgroundEvaluation() {
  if (m_backgroundPending || !backgroundEvaluationEnabled()) {
    return;
  }
  m_backgroundPending = true;
  // After what else waited for the idle point: the session's idle point.
  whenIdle(this, [this] {
    m_backgroundPending = false;
    startBackgroundEvaluation();
  });
}

void DocumentSession::startBackgroundEvaluation() {
  if (busy() || m_background || m_lastJobCancelRequested || m_state.mode != Mode::Idle || m_state.activeCommand) {
    return;
  }
  // The document query of the state now (notifyChanged after the job): the
  // marker before the end, and a state no run went through to the end.
  const qint64 features = m_documentInfo.value(QStringLiteral("features")).toInteger();
  const qint64 marker = m_documentInfo.value(QStringLiteral("marker")).toInteger();
  const auto revision = static_cast<std::uint64_t>(m_documentInfo.value(QStringLiteral("revision")).toInteger());
  const Document* document = &*m_state.document;
  if (marker >= features || (document == m_backgroundDoneDocument && revision == m_backgroundDoneRevision)) {
    return;
  }
  auto background = std::make_unique<BackgroundEvaluation>(*m_state.document);
  if (background->run->is_empty()) {
    return; // only suppressed features after the marker
  }
  QString startupError;
  if (!m_backgroundWorker->ensureStarted(startupError)) {
    qWarning().noquote() << QStringLiteral("Background evaluation: %1").arg(startupError);
    return;
  }
  if (const int delay = testRecomputeDelay(backgroundLabel()); delay > 0) {
    background->job.control().set_test_delay(static_cast<std::uint32_t>(delay));
  }
  const auto rolledBack = static_cast<qulonglong>(background->run->rolled_back());
  BackgroundRun* run = &*background->run;
  background->clock.start();
  m_background = std::move(background);
  m_backgroundWorker->post([run] {
    // Below the user's work on macOS too (the worker's own quality is the
    // model worker's).
    const mac::ThreadQualityScope quality(mac::ThreadQuality::Utility);
    run->run();
  });
  qDebug().noquote() << QStringLiteral("Background evaluation started: %1 features after the marker").arg(rolledBack);
}

void DocumentSession::cancelBackgroundEvaluation(const QString& reason) {
  if (!m_background || m_background->job.control().is_cancelled()) {
    return;
  }
  m_background->cancelledBy = reason;
  m_background->job.control().cancel();
}

void DocumentSession::stopBackgroundEvaluation(const QString& reason, bool adopt) {
  if (!m_background) {
    return;
  }
  cancelBackgroundEvaluation(reason);
  // The run stops at the cancel, inside the kernel's long operations too
  // (P7e); finishBackgroundEvaluation waits for it.
  finishBackgroundEvaluation(adopt);
}

void DocumentSession::finishBackgroundEvaluation(bool adopt) {
  if (!m_background) {
    return;
  }
  // Waits for the run to end (it has, unless a job or the session's end
  // stopped it).
  const std::exception_ptr error = m_backgroundWorker->take();
  const std::unique_ptr<BackgroundEvaluation> finished = std::move(m_background);
  const bool cancelled = finished->job.control().is_cancelled();
  const auto evaluated = static_cast<qulonglong>(finished->run->evaluated());
  const qint64 milliseconds = finished->clock.elapsed();
  if (!cancelled && !error) {
    m_backgroundDoneDocument = &*m_state.document;
    m_backgroundDoneRevision = finished->run->base_revision();
  }
  std::size_t adopted = 0;
  // Only while no job has the document (a job stops the run before it
  // starts); the document adopts nothing of another state.
  if (adopt && !error && !busy()) {
    adopted = m_state.document->adopt_background_run(std::move(finished->run));
  }
  if (error) {
    try {
      std::rethrow_exception(error);
    } catch (const std::exception& exception) {
      qWarning().noquote() << QStringLiteral("Background evaluation failed: %1").arg(errorText(exception));
    } catch (...) {
      qWarning().noquote() << QStringLiteral("Background evaluation failed");
    }
  } else if (cancelled) {
    qDebug().noquote() << QStringLiteral("Background evaluation cancelled by %1 after %2 ms: %3 features, adopted %4")
                              .arg(finished->cancelledBy.isEmpty() ? QStringLiteral("a stop") : finished->cancelledBy)
                              .arg(milliseconds)
                              .arg(evaluated)
                              .arg(static_cast<qulonglong>(adopted));
  } else {
    qDebug().noquote() << QStringLiteral("Background evaluation: %1 features in %2 ms, adopted %3")
                              .arg(evaluated)
                              .arg(milliseconds)
                              .arg(static_cast<qulonglong>(adopted));
  }
}

void DocumentSession::clearPreview() {
  if (busy()) {
    throw ModelBusy();
  }
  m_state.document->clear_preview();
  m_standIns.clear();
}

void DocumentSession::markSaved() {
  if (busy()) {
    throw ModelBusy();
  }
  m_state.document->command(rustStr(compactJson({{QStringLiteral("cmd"), QStringLiteral("mark_saved")}})));
}

void DocumentSession::cancelJob() {
  if (m_job) {
    // A job without a progress dialog (a feature preview, mitcad#634):
    // when it was asked to stop, as the dialog's Cancel logs it, so that
    // the time a cancel takes inside the kernel can be read (P7e).
    if (m_async && m_job == &m_async->job && !m_job->control().is_cancelled()) {
      qDebug().noquote() << QStringLiteral("Computing %1: cancel requested after %2 ms")
                               .arg(m_async->label).arg(m_async->clock.elapsed());
    }
    m_job->control().cancel();
  }
}

void DocumentSession::whenIdle(QObject* context, std::function<void()> call) {
  m_idleCalls.push_back({QPointer<QObject>(context), std::move(call)});
  scheduleIdleCalls();
}

void DocumentSession::scheduleIdleCalls() {
  if (!busy() && !m_idleCalls.empty() && !m_idleCallsPosted) {
    m_idleCallsPosted = true;
    QTimer::singleShot(0, this, &DocumentSession::runIdleCalls);
  }
}

void DocumentSession::runIdleCalls() {
  m_idleCallsPosted = false;
  while (!m_idleCalls.empty() && !busy()) {
    IdleCall next = std::move(m_idleCalls.front());
    m_idleCalls.pop_front();
    if (next.context) {
      next.call();
    }
  }
}

void DocumentSession::setSelection(const Selection& selection) {
  m_state.selection = selection;
  emit changed();
}

void DocumentSession::setMode(Mode mode) {
  if (m_state.mode != mode) {
    if (mode != Mode::Idle) {
      cancelBackgroundEvaluation(mode == Mode::Command ? QStringLiteral("a command") : QStringLiteral("a sketch"));
    }
    m_state.mode = mode;
    emit changed();
  }
}

void DocumentSession::notifyChanged() {
  if (!busy()) {
    m_documentInfo = query({{QStringLiteral("query"), QStringLiteral("document")}}).toObject();
  }
  emit changed();
}

} // namespace mitcad
