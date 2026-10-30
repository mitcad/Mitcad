// SPDX-License-Identifier: MIT
// Section Analysis off the UI thread (mitcad#438): the clips of the bodies
// shown, their meshes for the view and the panel's area and length worked
// out by the section worker (framework/SectionWorker.hpp) from what the
// model prepared while the document was idle; the view shows them when they
// are ready, and the panel's numbers come after the view's update. A
// feature preview's bodies are clipped the same way (mitcad#943,
// framework/SectionPreview.hpp).
#include "MainWindow.hpp"

#include <chrono>
#include <optional>
#include <thread>
#include <utility>
#include <vector>

#include <QCoreApplication>
#include <QDebug>
#include <QElapsedTimer>
#include <QJsonArray>
#include <QPointer>
#include <QThread>
#include <QtLogging>

#include <Message_ProgressIndicator.hxx>
#include <Message_ProgressScope.hxx>
#include <Standard_Failure.hxx>
#include <TopoDS_Shape.hxx>

#include "framework/Diagnostics.hpp"
#include "framework/Json.hpp"
#include "framework/SectionPreview.hpp"
#include "framework/SectionWorker.hpp"
#include "mitcad/geometry/cancel.hpp"
#include "mitcad/io/mesh.hpp"
#include "view/ViewTessellation.hpp"

namespace mitcad {
namespace {

// MITCAD_TEST_SECTION_DELAY_MS: how long each view round waits before it
// clips (in steps that watch its stop), so that UI tests can change the
// plane or cancel while one runs.
int testSectionDelay() {
  static const int delay = qEnvironmentVariableIntValue("MITCAD_TEST_SECTION_DELAY_MS");
  return delay;
}

// A round's stop as OCCT's user break (asked from BRepMesh's threads).
class StopBreak : public Message_ProgressIndicator {
public:
  explicit StopBreak(const geometry::CancelSource& stop) : m_stop(stop) {}
  bool UserBreak() override { return m_stop.cancel_requested(); }
  void Show(const Message_ProgressScope&, const bool) override {}

private:
  const geometry::CancelSource& m_stop;
};

// Where the work ran, for the log: the round's own thread, not the UI's.
QString threadOfWork() {
  const bool ui = QThread::currentThread() == QCoreApplication::instance()->thread();
  return ui ? QStringLiteral("on the UI thread")
            : QStringLiteral("off the UI thread (%1)").arg(QThread::currentThread()->objectName());
}

QString textOf(const rust::String& text) {
  return QString::fromUtf8(text.data(), static_cast<qsizetype>(text.size()));
}

// What a measurement round computes.
struct MeasureRound {
  std::optional<rust::Box<PreparedQuery>> prepared;
  QByteArray answer;
  QString error;
};

} // namespace

// What a view round computes: for each body it clips, its key and shape as
// the view had them, and what the round left of it (or its error).
struct MainWindow::SectionRound {
  QStringList keys;
  QStringList uids;
  std::vector<std::shared_ptr<geometry::Shape>> sources;
  std::optional<rust::Box<PreparedClips>> prepared;
  std::vector<std::shared_ptr<geometry::Shape>> clipped;
  QStringList errors;
};

namespace {

// The view round's work, on the worker's thread: the prepared clips (each
// body the plane cuts on a copy of its own), then the new shapes meshed as
// the view meshes them, all together, so that the view finds them meshed.
// What it makes shares no sub-shape with the bodies the view draws. Logged
// as "<what> at <plane> off the UI thread ...": "Section clips" for the
// bodies at the marker, "Section preview clips" for a feature preview's
// (mitcad#943).
template <class Round>
void clipForView(Round& round, const QJsonValue& plane, const geometry::CancelSource& stop, const QString& what) {
  for (int waited = 0; waited < testSectionDelay() && !stop.cancel_requested(); waited += 10) {
    std::this_thread::sleep_for(std::chrono::milliseconds(10));
  }
  if (stop.cancel_requested() || !round.prepared) {
    return;
  }
  QElapsedTimer clock;
  clock.start();
  ScopedTiming timing("section clips");
  const rust::Box<ClippedBodies> results = (*round.prepared)->run();
  round.prepared.reset();
  std::vector<TopoDS_Shape> fresh;
  for (std::size_t i = 0; i < round.sources.size(); ++i) {
    const std::shared_ptr<geometry::Shape> shape = i < results->count() ? results->shape(i) : nullptr;
    round.clipped.push_back(shape);
    round.errors << (shape ? QString() : textOf(results->error(i)));
    if (shape && round.sources[i] && !shape->occt().IsSame(round.sources[i]->occt())) {
      fresh.push_back(shape->occt());
    }
  }
  timing.setDetail(QStringLiteral("%1 bodies, %2 new").arg(round.sources.size()).arg(fresh.size()));
  timing.finish();
  if (stop.cancel_requested()) {
    return;
  }
  std::size_t meshed = 0;
  const occ::handle<StopBreak> progress = new StopBreak(stop);
  try {
    meshed = io::mesh_all_in_place(fresh, viewMeshOptions(), progress->Start());
  } catch (const Standard_Failure&) {
    // The view meshes them when it draws them.
  } catch (const std::exception&) {
  }
  qDebug().noquote() << QStringLiteral("%1 at %2 %3: %4 bodies, %5 new, %6 meshed in %7 ms")
                            .arg(what, QString::fromUtf8(compactJson(QJsonObject{{QStringLiteral("plane"), plane}})),
                                 threadOfWork())
                            .arg(round.sources.size())
                            .arg(fresh.size())
                            .arg(static_cast<qulonglong>(meshed))
                            .arg(clock.elapsed());
}

// A preview round's clips into its cut, on the UI thread once it is done
// (mitcad#943): a body whose clip failed, or every body when the round
// failed, is drawn whole with its warning (mitcad#392).
template <class Round>
void fillPreviewSection(const Round& round, const QJsonValue& plane, const QString& error, PreviewSection& cut) {
  const bool clipped = error.isEmpty() && round.clipped.size() == round.sources.size();
  for (std::size_t i = 0; i < round.sources.size(); ++i) {
    const qsizetype at = static_cast<qsizetype>(i);
    const std::shared_ptr<geometry::Shape> shape = clipped ? round.clipped[i] : nullptr;
    cut.left.insert(round.keys.value(at), shape);
    if (!shape) {
      const QString why = clipped ? round.errors.value(at) : error.isEmpty() ? QStringLiteral("no clips") : error;
      qWarning().noquote() << QStringLiteral("Section: %1 shown whole: %2").arg(round.uids.value(at), why);
    }
  }
  cut.ready = true;
  qDebug().noquote() << QStringLiteral("Section preview ready at %1: %2 bodies from the section worker")
                            .arg(QString::fromUtf8(compactJson(QJsonObject{{QStringLiteral("plane"), plane}})))
                            .arg(round.sources.size());
}

} // namespace

std::shared_ptr<PreviewSection> MainWindow::sectionPreview(const std::vector<PreviewClipTarget>& targets,
                                                           QObject* context, std::function<void()> ready) {
  const QJsonValue plane = cutPlane();
  if (plane.isNull() || targets.empty() || m_sectionWorker == nullptr) {
    return nullptr; // whole
  }
  if (modelBusy()) {
    qWarning().noquote() << QStringLiteral("Section: the preview not clipped: the model computes");
    return nullptr;
  }
  auto cut = std::make_shared<PreviewSection>();
  cut->plane = plane;
  auto round = std::make_shared<SectionRound>();
  QJsonArray bodies;
  for (const PreviewClipTarget& target : targets) {
    // The shape the clip cuts, kept for the caps: the model's preview of
    // the body, or the body at the marker that the preview places anew.
    const std::shared_ptr<geometry::Shape> uncut =
        target.preview ? idleDocument().preview_body_shape(target.uid) : idleDocument().body_shape(target.uid);
    if (!uncut) {
      continue;
    }
    const QString uid = QString::fromStdString(target.uid);
    cut->keys.insert(target.key);
    cut->uncut.insert(target.key, uncut);
    round->keys << target.key;
    round->uids << uid;
    round->sources.push_back(uncut);
    bodies.append(previewClipTarget(uid, target.preview, target.placement));
  }
  if (round->sources.empty()) {
    cut->ready = true;
    return cut;
  }
  try {
    round->prepared.emplace(idleDocument().prepare_clips(
        rustStr(compactJson({{QStringLiteral("plane"), plane}, {QStringLiteral("bodies"), bodies}}))));
  } catch (const std::exception& e) {
    // Drawn whole, as a clip that failed.
    fillPreviewSection(*round, plane, errorText(e), *cut);
    return cut;
  }
  const QPointer<QObject> owner(context);
  m_sectionWorker->start(
      SectionWorker::Kind::Preview, QStringLiteral("preview"),
      [round, plane](const geometry::CancelSource& stop) {
        clipForView(*round, plane, stop, QStringLiteral("Section preview clips"));
      },
      [round, plane, cut, owner, ready = std::move(ready)](const QString& error) {
        fillPreviewSection(*round, plane, error, *cut);
        if (owner && ready) {
          ready();
        }
      });
  return cut;
}

bool MainWindow::startSectionRound() {
  if (m_sectionWorker == nullptr || m_section.isNull() || modelBusy() || cutPlane() != m_section) {
    return false; // whole, or cut at the sketch's plane (Hide Above Sketch): as before
  }
  SectionTargets cut;
  std::vector<BodyDisplay> bodies;
  try {
    bodies = displayBodies(&cut);
  } catch (const std::exception&) {
    return false;
  }
  // The bodies the clips kept for this plane do not have.
  const bool samePlane = m_sectionState.plane == cut.plane;
  auto round = std::make_shared<SectionRound>();
  QJsonArray targets;
  for (std::size_t i = 0; i < cut.which.size(); ++i) {
    const BodyDisplay& body = bodies[cut.which[i]];
    const QString key = cut.keys.value(static_cast<qsizetype>(i));
    if (samePlane) {
      const auto found = m_sectionState.clips.constFind(key);
      if (found != m_sectionState.clips.cend() && found->source == body.shape) {
        continue;
      }
    }
    round->keys << key;
    round->uids << QString::fromStdString(body.uid);
    round->sources.push_back(body.shape);
    targets.append(cut.targets.at(static_cast<qsizetype>(i)));
  }
  if (round->sources.empty()) {
    return false; // the kept clips show it at once
  }
  try {
    round->prepared.emplace(idleDocument().prepare_clips(
        rustStr(compactJson({{QStringLiteral("plane"), cut.plane}, {QStringLiteral("bodies"), targets}}))));
  } catch (const std::exception& e) {
    qWarning().noquote() << QStringLiteral("Section: not clipped off the UI thread: %1").arg(errorText(e));
    return false;
  }
  const QJsonValue plane = cut.plane;
  m_sectionWorker->start(
      SectionWorker::Kind::View, QStringLiteral("view"),
      [round, plane](const geometry::CancelSource& stop) {
        clipForView(*round, plane, stop, QStringLiteral("Section clips"));
      },
      [this, round, plane](const QString& error) { showSectionRound(*round, plane, error); });
  return true;
}

void MainWindow::showSectionRound(const SectionRound& round, const QJsonValue& plane, const QString& error) {
  if (plane != cutPlane()) {
    return; // shown another way since (whole, or the sketch's plane)
  }
  if (!error.isEmpty() || round.clipped.size() != round.sources.size()) {
    qWarning().noquote() << QStringLiteral("Section: not clipped off the UI thread: %1")
                                .arg(error.isEmpty() ? QStringLiteral("no clips") : error);
    showBodies(); // clipped here
    return;
  }
  SectionState& kept = m_sectionState;
  if (kept.plane != plane) {
    kept.clips.clear();
    kept.plane = plane;
  }
  for (std::size_t i = 0; i < round.sources.size(); ++i) {
    const qsizetype at = static_cast<qsizetype>(i);
    kept.clips.insert(round.keys.value(at), SectionClip{round.sources[i], round.clipped[i]});
    if (!round.clipped[i]) {
      // Shown whole; a crash inside OCCT is its error (mitcad#392).
      qWarning().noquote() << QStringLiteral("Section: %1 shown whole: %2")
                                  .arg(round.uids.value(at), round.errors.value(at));
    }
  }
  qDebug().noquote() << QStringLiteral("Section view ready at %1: %2 bodies from the section worker")
                            .arg(QString::fromUtf8(compactJson(QJsonObject{{QStringLiteral("plane"), plane}})))
                            .arg(round.sources.size());
  showBodies(); // from the kept clips, then the measurement
}

void MainWindow::measureLater(const QJsonObject& query, QObject* context, Measured done) {
  SectionState& state = m_sectionState;
  SectionMeasurement measurement;
  measurement.id = ++state.measurements;
  measurement.query = query;
  measurement.context = context;
  measurement.done = std::move(done);
  // One posted for an older request is stopped by this one's round.
  state.measurement = std::move(measurement);
  measureIfWaiting();
}

void MainWindow::measureIfWaiting() {
  std::optional<SectionMeasurement>& waiting = m_sectionState.measurement;
  if (!waiting || waiting->posted || m_sectionWorker == nullptr) {
    return;
  }
  if (!waiting->context || m_section.isNull()) {
    waiting.reset(); // its panel is gone, or the section it measures
    return;
  }
  if (m_sectionWorker->busyWith(SectionWorker::Kind::View)) {
    return; // after the view's update (showBodies asks again)
  }
  if (modelBusy()) {
    m_documentSession->whenIdle(this, [this] { measureIfWaiting(); });
    return;
  }
  auto round = std::make_shared<MeasureRound>();
  try {
    round->prepared.emplace(idleDocument().prepare_query(rustStr(compactJson(waiting->query))));
  } catch (const std::exception& e) {
    const SectionMeasurement failed = std::move(*waiting);
    waiting.reset();
    if (failed.context) {
      failed.done(QJsonObject(), errorText(e));
    }
    return;
  }
  waiting->posted = true;
  const quint64 id = waiting->id;
  m_sectionWorker->start(
      SectionWorker::Kind::Measure, QStringLiteral("measure"),
      [round](const geometry::CancelSource&) {
        try {
          const rust::String answer = (*round->prepared)->run();
          round->answer = QByteArray(answer.data(), static_cast<qsizetype>(answer.size()));
        } catch (const std::exception& e) {
          round->error = errorText(e);
        }
        round->prepared.reset();
        qDebug().noquote() << QStringLiteral("Section measured %1").arg(threadOfWork());
      },
      [this, round, id](const QString& error) {
        std::optional<SectionMeasurement>& current = m_sectionState.measurement;
        if (!current || current->id != id) {
          return; // a newer request
        }
        const SectionMeasurement measured = std::move(*current);
        current.reset();
        if (!measured.context) {
          return;
        }
        const QString failure = error.isEmpty() ? round->error : error;
        measured.done(failure.isEmpty() ? QJsonDocument::fromJson(round->answer).object() : QJsonObject(), failure);
      });
}

void MainWindow::redoSectionWork() {
  if (m_sectionWorker == nullptr) {
    return;
  }
  const SectionWorker::Stopped stopped = m_sectionWorker->cancel();
  if (!stopped.view && !stopped.measure) {
    return;
  }
  if (stopped.measure && m_sectionState.measurement) {
    m_sectionState.measurement->posted = false;
  }
  // Done again once the window is idle: the view at the section's plane
  // (a new round, or at once from the kept clips), which then measures.
  const bool view = stopped.view;
  m_documentSession->whenIdle(this, [this, view] {
    if (!view) {
      measureIfWaiting();
    } else if (!startSectionRound()) {
      showBodies();
    }
  });
}

} // namespace mitcad
