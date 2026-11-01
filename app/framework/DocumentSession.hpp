// SPDX-License-Identifier: MIT
#pragma once

#include <atomic>
#include <deque>
#include <functional>
#include <memory>
#include <optional>
#include <string>

#include <QElapsedTimer>
#include <QJsonObject>
#include <QObject>
#include <QPointer>
#include <QString>
#include <QTimer>

#include "DisplayMeshes.hpp"
#include "ModelWorker.hpp"
#include "Command.hpp"
#include "Selection.hpp"
#include "mitcad/geometry/shape.hpp"

namespace mitcad {

class CommandSession;

// A new, empty design (the bridge's new_design_document_in) in the default
// units for new designs (Preferences > General, GeneralSettings, mitcad#1036):
// the untitled design at the start, and New and Close
// (MainWindow::installEmptyDocument). Its unit is its initial state: no undo
// step, not modified.
rust::Box<Document> newDesignDocument();

// One document and its computation, independent of the window toolkit.
// Both the Qt presentation and the AppKit presentation use this owner;
// neither keeps a second document or model worker behind its controls.
class DocumentSession : public QObject {
  Q_OBJECT

public:
  enum class Mode { Idle, PickPlane, Sketch, Command };
  enum class JobLog { Always, WhenSlow };

  // Transitional storage access for the existing Qt host. References to
  // these fields keep its workflows intact while ownership lives here.
  struct State {
    explicit State(bool demo);
    rust::Box<Document> document;
    Selection selection;
    Selection isolation;
    Mode mode = Mode::Idle;
    bool originShown = false;
    QJsonValue section;
    QString filePath;
    QString documentName;
    QString projectDir;
    std::optional<QString> recoveredDigest;
    struct VersionBase {
      QString path;
      QString file;
      QString head;
    };
    std::optional<VersionBase> versionBase;
    QString renamedFrom;
    bool changeNoted = false;
    QString lastError;
    bool lastCancelled = false;
    QPointer<CommandSession> activeCommand;
    bool commandInSketch = false;
    QString lastCommand;
    std::function<void(bool committed)> commandFinished;
    bool sketchExisting = false;
    int sketchEnterDepth = 0;
    int sketchRestoreMarker = -1;
    QString sketchOccurrence; // where sketch mode shows the sketch's component (mitcad#505)
  };

  struct Snapshot {
    QJsonObject document;
    Selection selection;
    Mode mode = Mode::Idle;
    QString filePath;
    QString name;
    bool busy = false;
    bool modified = false;
    bool canUndo = false;
    bool canRedo = false;
    QString lastError;
    bool lastCancelled = false;
  };

  struct AsyncResult {
    QJsonObject answer;
    QString error;
    bool cancelled = false;
    bool succeeded() const { return error.isEmpty() && !cancelled; }
  };

  using Job = std::function<void(ModelJob&)>;
  // Legacy callers remain synchronous, but the presentation decides how
  // it waits and shows progress. The native frontend may instead use the
  // asynchronous command/preview entry points without a nested loop.
  using WaitHandler = std::function<void(const QString&, ModelJob&, const QElapsedTimer&, bool& logged)>;
  using Completion = std::function<void(const AsyncResult&)>;
  using InspectionTask = std::function<Inspection(const Document&)>;
  using InspectionCompletion = std::function<void(const Inspection&, const AsyncResult&)>;

  explicit DocumentSession(bool demo = false, QObject* parent = nullptr);
  ~DocumentSession() override;

  State& state() { return m_state; }
  const State& state() const { return m_state; }
  Snapshot snapshot() const;
  bool busy() const { return m_job != nullptr; }
  bool asynchronousJob() const { return m_async != nullptr; }
  ModelWorker& worker() const { return *m_worker; }
  ModelJob* activeJob() const { return m_job; }
  const Document& idleDocument() const;
  QJsonValue query(const QJsonObject& request) const;
  std::shared_ptr<geometry::Shape> bodyShape(const QString& uid) const;
  std::shared_ptr<geometry::Shape> previewBodyShape(const std::string& uid) const;
  std::shared_ptr<geometry::Shape> previewToolShape() const;
  int undoDepth() const;
  bool modified() const;

  void setWaitHandler(WaitHandler handler) { m_waitHandler = std::move(handler); }
  // Called on the session's thread before every job starts, while the
  // document is still the caller's: what reads the bodies on another
  // thread stops there (Section Analysis' rounds, mitcad#438), as a job
  // may write into their shapes (the meshes of new bodies that share their
  // faces).
  void setBeforeJob(std::function<void()> hook) { m_beforeJob = std::move(hook); }
  bool runJob(const QString& label, const QString& stage, const Job& job, JobLog log = JobLog::Always);
  QJsonObject command(const QJsonObject& request);
  QJsonObject preview(const QByteArray& request, const QString& name);
  // Returns false if a job is already running. Completion always runs on
  // the session's thread and is dropped if its context has been destroyed.
  bool commandAsync(const QJsonObject& request, QObject* context, Completion completion);
  bool previewAsync(const QByteArray& request, const QString& name, QObject* context, Completion completion);
  bool inspectionAsync(const QString& name, QObject* context, InspectionTask task,
                       InspectionCompletion completion);
  // Meshes `bodies` (refinableShapes of a preview's stand-ins) in the
  // view's final tier in a job of their own, which computes nothing in the
  // model and changes no revision and no undo step (DisplayMeshes.hpp,
  // mitcad#635): their stand-ins are dropped once they are meshed to the
  // end, so the view draws them. True when all were.
  bool refineDisplay(const DisplayShapes& bodies);
  bool refineDisplayAsync(const DisplayShapes& bodies, QObject* context, Completion completion);
  void clearPreview();
  void markSaved();
  void cancelJob();
  void whenIdle(QObject* context, std::function<void()> call);
  void setSelection(const Selection& selection);
  void setMode(Mode mode);
  // Publish mutations made by the compatibility host after a workflow.
  void notifyChanged();

signals:
  void changed();
  void busyChanged(bool busy);
  void jobStarted(const QString& label, const QString& stage);
  void jobProgress(const QString& stage, quint64 position, quint64 total, qint64 elapsedMilliseconds);
  void jobFinished(const QString& label, bool cancelled, const QString& error);

private:
  friend class TestDriver;
  // Armed only by the in-process UI driver, consumed on the worker once.
  QString m_testFailCommand;
  // The UI driver can hold an asynchronous preview before model work starts,
  // so input races depend on the worker's state rather than the host's speed.
  std::shared_ptr<std::atomic<bool>> m_testHoldPreview = std::make_shared<std::atomic<bool>>(false);
  struct IdleCall {
    QPointer<QObject> context;
    std::function<void()> call;
  };
  struct AsyncJob;
  bool startAsync(const QString& label, const QString& stage, Job job,
                  std::shared_ptr<AsyncResult> result, QObject* context, Completion completion);
  void completeAsync();
  void runIdleCalls();
  void scheduleIdleCalls();
  void finishJob(const QString& label, bool cancelled, const QString& error);
  void waitWithoutPresentation(ModelJob& job);

  State m_state;
  std::unique_ptr<ModelWorker> m_worker;
  ModelJob* m_job = nullptr;
  std::unique_ptr<AsyncJob> m_async;
  // The last preview's stand-ins (DisplayMeshes.hpp, mitcad#635): its
  // bodies hold them weakly, the view takes them from here.
  DisplayShapes m_standIns;
  WaitHandler m_waitHandler;
  std::function<void()> m_beforeJob;
  std::deque<IdleCall> m_idleCalls;
  bool m_idleCallsPosted = false;
  QTimer m_progressTimer;
  QJsonObject m_documentInfo;
};

} // namespace mitcad
