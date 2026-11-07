// SPDX-License-Identifier: MIT
#include "DisplayMeshes.hpp"

#include <algorithm>
#include <cstdint>
#include <set>
#include <string>
#include <unordered_map>
#include <vector>

#include <QCoreApplication>
#include <QElapsedTimer>
#include <QJsonArray>
#include <QJsonValue>
#include <QtLogging>

#include <BRepTools.hxx>
#include <Message_ProgressIndicator.hxx>
#include <Message_ProgressScope.hxx>
#include <Standard_Failure.hxx>

#include "Json.hpp"
#include "bridge/shape.hpp"
#include "mitcad/io/mesh.hpp"
#include "view/ViewTessellation.hpp"

namespace mitcad {
namespace {

// The job's cancel as OCCT's user break (asked from BRepMesh's threads).
class JobBreak : public Message_ProgressIndicator {
public:
  explicit JobBreak(const ModelJob& job) : m_job(job) {}
  bool UserBreak() override { return m_job.control().is_cancelled(); }
  void Show(const Message_ProgressScope&, const bool) override {}

private:
  const ModelJob& m_job;
};

// The job's cancel or the meshing's own stop as OCCT's user break
// (RecomputeMeshing, mitcad#881).
class StopBreak : public Message_ProgressIndicator {
public:
  StopBreak(const ModelJob& job, const std::atomic<bool>& stop) : m_job(job), m_stop(stop) {}
  bool UserBreak() override { return m_stop.load() || m_job.control().is_cancelled(); }
  void Show(const Message_ProgressScope&, const bool) override {}

private:
  const ModelJob& m_job;
  const std::atomic<bool>& m_stop;
};

// MITCAD_MESH_DURING_RECOMPUTE=0 (or off): opens mesh after the recompute
// only (mitcad#881).
bool meshDuringRecompute() {
  const QByteArray given = qgetenv("MITCAD_MESH_DURING_RECOMPUTE");
  return given != "0" && given != "off";
}

bool addShape(DisplayShapes& shapes, std::shared_ptr<geometry::Shape> shape) {
  if (shape && !shape->occt().IsNull()) {
    shapes.push_back(std::move(shape));
    return true;
  }
  return false;
}

// What meshForDisplay passes by: a shape with a triangulation on every
// face, a mesh body, or one meshed for display to the end.
bool readyForDisplay(const geometry::Shape& shape) {
  const TopoDS_Shape& occt = shape.occt();
  return io::has_triangulation(occt) || io::is_mesh(occt) || shape.display_meshed();
}

// MITCAD_DISPLAY_MESH_STORE=0 (or off) keeps display meshes out of the
// result store (mitcad#858).
bool displayMeshStore() {
  const QByteArray given = qgetenv("MITCAD_DISPLAY_MESH_STORE");
  return given != "0" && given != "off";
}

// A display mesh request of the document: the bodies and the options.
QByteArray meshRequest(const std::vector<std::string>& uids, const QByteArray& options) {
  QJsonArray bodies;
  for (const std::string& uid : uids) {
    bodies.append(QString::fromStdString(uid));
  }
  return compactJson(
      {{QStringLiteral("bodies"), bodies}, {QStringLiteral("options"), QString::fromUtf8(options)}});
}

// The shown bodies and their triangles after an open's display meshing
// (mitcad#858: the store's meshes and fresh ones compared by this).
void countShown(DisplayMeshing& meshing, const DisplayShapes& shapes) {
  meshing.shown = shapes.size();
  meshing.triangles = 0;
  for (const std::shared_ptr<geometry::Shape>& shape : shapes) {
    meshing.triangles += io::mesh_stats(shape->occt()).triangles;
  }
}

// What an open took of the meshing beside its recompute (mitcad#881).
struct DuringRecompute {
  qint64 wait = 0; // for its last batch, after the recompute
  std::size_t taken = 0; // shapes given its meshes
  std::size_t stale = 0; // meshed, but the body's shape is another now
  std::size_t incomplete = 0; // of those taken, with faces OCCT's mesher failed on
};

// Gives each shown body that `during` meshed, and whose shape is still the
// one it meshed and has no triangulations yet, the meshes of its copy; it
// is then meshed to the end, as one meshed here would be.
void takeMeshedDuring(DuringRecompute& out, const std::vector<RecomputeMeshing::Body>& bodies,
                      const DisplayShapes& shapes, const std::vector<std::string>& uids) {
  std::unordered_map<std::string, const RecomputeMeshing::Body*> meshed;
  for (const RecomputeMeshing::Body& body : bodies) {
    meshed.emplace(body.uid, &body);
  }
  std::vector<bool> had(shapes.size());
  for (std::size_t i = 0; i < shapes.size(); ++i) {
    had[i] = io::has_triangulation(shapes[i]->occt());
  }
  for (std::size_t i = 0; i < shapes.size() && i < uids.size(); ++i) {
    const auto found = meshed.find(uids[i]);
    if (found == meshed.end()) {
      continue;
    }
    const RecomputeMeshing::Body& body = *found->second;
    if (body.shape != shapes[i]) {
      ++out.stale;
      continue;
    }
    if (readyForDisplay(*shapes[i])) {
      continue;
    }
    try {
      io::restore_triangulations(shapes[i]->occt(), *body.meshes);
    } catch (const Standard_Failure&) {
      ++out.stale;
      continue;
    } catch (const std::exception&) {
      ++out.stale;
      continue;
    }
    shapes[i]->set_display_meshed();
    ++out.taken;
    if (!body.complete) {
      ++out.incomplete;
    }
  }
  // A body whose faces are all another's it shares them with (a pattern's
  // copies) is meshed to the end too, as it would be meshed together with
  // that one here.
  for (std::size_t i = 0; i < shapes.size(); ++i) {
    if (!had[i] && !shapes[i]->display_meshed() && io::has_triangulation(shapes[i]->occt())) {
      shapes[i]->set_display_meshed();
      ++out.taken;
    }
  }
}

void countDuring(DisplayMeshing& meshing, const DuringRecompute& taken, const RecomputeMeshing* during) {
  if (during == nullptr) {
    return;
  }
  meshing.during = taken.taken;
  meshing.duringStale = taken.stale;
  meshing.incomplete += taken.incomplete;
  meshing.duringGiven = during->given();
  meshing.duringLeftToStore = during->leftToStore();
  meshing.duringLeft = during->left();
  meshing.duringMilliseconds = during->milliseconds();
  meshing.duringAfterMilliseconds = during->afterMilliseconds();
  meshing.duringWaitMilliseconds = taken.wait;
  meshing.duringTrace = during->trace();
}

// The shapes meshed for display after an open's recompute, with what
// `during` meshed beside it (mitcad#881): the bodies it meshed take their
// meshes; the others but those of its batch under way are meshed while
// that batch goes on; then its bodies take theirs, and any it did not end
// with (cut short) are meshed. Without `during`, meshForDisplay.
DisplayMeshing meshBeside(const DisplayShapes& shapes, const std::vector<std::string>& uids, ModelJob& job,
                          RecomputeMeshing* during) {
  if (during == nullptr) {
    return meshForDisplay(shapes, job);
  }
  DuringRecompute taken;
  takeMeshedDuring(taken, during->takeMeshed(), shapes, uids);
  const std::vector<std::string> underWay = during->underWay();
  const auto isUnderWay = [&underWay](const std::string& uid) {
    return std::find(underWay.begin(), underWay.end(), uid) != underWay.end();
  };
  DisplayShapes rest;
  DisplayShapes waiting;
  for (std::size_t i = 0; i < shapes.size() && i < uids.size(); ++i) {
    (isUnderWay(uids[i]) ? waiting : rest).push_back(shapes[i]);
  }
  DisplayMeshing meshing = meshForDisplay(rest, job);
  taken.wait = during->finish();
  takeMeshedDuring(taken, during->takeMeshed(), shapes, uids);
  DisplayShapes again;
  for (const std::shared_ptr<geometry::Shape>& shape : waiting) {
    if (!readyForDisplay(*shape)) {
      again.push_back(shape);
    }
  }
  if (!again.empty()) {
    const DisplayMeshing more = meshForDisplay(again, job);
    meshing.bodies += more.bodies;
    meshing.incomplete += more.incomplete;
    meshing.ready += more.ready;
    meshing.report.meshed += more.report.meshed;
    meshing.report.abandoned += more.report.abandoned;
  }
  countDuring(meshing, taken, during);
  return meshing;
}

} // namespace

DisplayShapes shownBodyShapes(const Document& document, std::vector<std::string>* uids) {
  DisplayShapes shapes;
  const QJsonDocument instances =
      parseJson(document.query(rustStr(compactJson({{QStringLiteral("query"), QStringLiteral("instances")}}))));
  std::set<std::string> seen;
  for (const QJsonValue& value : instances.array()) {
    const QJsonObject instance = value.toObject();
    if (!instance.value(QStringLiteral("visible")).toBool(true)) {
      continue;
    }
    const std::string uid = instance.value(QStringLiteral("body")).toString().toStdString();
    if (!uid.empty() && seen.insert(uid).second && addShape(shapes, document.body_shape(uid)) &&
        uids != nullptr) {
      uids->push_back(uid);
    }
  }
  return shapes;
}

DisplayShapes previewShapes(const Document& document, const QJsonObject& report) {
  DisplayShapes shapes;
  if (report.value(QStringLiteral("status")).toString() != QStringLiteral("ok")) {
    return shapes;
  }
  std::set<std::string> seen;
  const auto add = [&](const QString& uid) {
    if (!uid.isEmpty() && seen.insert(uid.toStdString()).second) {
      addShape(shapes, document.preview_body_shape(uid.toStdString()));
    }
  };
  for (const QJsonValue& value : report.value(QStringLiteral("bodies")).toArray()) {
    const QJsonObject body = value.toObject();
    if (body.value(QStringLiteral("changed")).toBool()) {
      add(body.value(QStringLiteral("uid")).toString());
    }
  }
  for (const QJsonValue& value : report.value(QStringLiteral("instances")).toArray()) {
    add(value.toObject().value(QStringLiteral("body")).toString());
  }
  addShape(shapes, document.preview_tool_shape());
  return shapes;
}

DisplayShapes previewDonors(const Document& document, const QJsonObject& report) {
  DisplayShapes shapes;
  if (report.value(QStringLiteral("status")).toString() != QStringLiteral("ok")) {
    return shapes;
  }
  for (const QJsonValue& value : report.value(QStringLiteral("bodies")).toArray()) {
    const QJsonObject body = value.toObject();
    if (body.value(QStringLiteral("changed")).toBool()) {
      addShape(shapes, document.body_shape(body.value(QStringLiteral("uid")).toString().toStdString()));
    }
  }
  return shapes;
}

DisplayMeshing meshForDisplay(const DisplayShapes& shapes, ModelJob& job, DisplayTier tier,
                              const DisplayShapes& donors) {
  DisplayMeshing meshing;
  meshing.tier = tier;
  QElapsedTimer clock;
  clock.start();
  // The shapes the view would mesh, meshed together so that OCCT spreads
  // the faces of all of them over the cores (mitcad#438); one at a time,
  // a design of many small bodies used a core or two. In the interactive
  // tier their stand-ins instead (mitcad#635).
  std::vector<TopoDS_Shape> pending;
  std::vector<std::shared_ptr<geometry::Shape>> meshed; // whose faces are meshed
  std::vector<std::shared_ptr<geometry::Shape>> bodies; // the body each is drawn for
  for (const std::shared_ptr<geometry::Shape>& shape : shapes) {
    const TopoDS_Shape& occt = shape->occt();
    if (io::has_triangulation(occt) || io::is_mesh(occt) || shape->display_meshed()) {
      // A body whose every face has the final tier's triangulation already
      // (a pattern's copy or a moved body sharing the faces of one shown,
      // mitcad#838, mitcad#1275) is meshed to the end as one meshed here:
      // the view draws it as it is (drawerMeshes, mitcad#878), its drawers
      // never run the mesher over it again on the UI thread.
      if (tier == DisplayTier::Final && !shape->display_meshed() && !shape->display_interactive() &&
          !io::is_mesh(occt) && io::has_triangulation(occt)) {
        shape->set_display_meshed();
      }
      ++meshing.ready;
      continue;
    }
    if (tier == DisplayTier::Final) {
      pending.push_back(occt);
      meshed.push_back(shape);
      bodies.push_back(shape);
      continue;
    }
    if (const auto standIn = shape->display_stand_in(); standIn && standIn->display_meshed()) {
      // Drawn again (a result the model had cached): the caller keeps it
      // as it keeps a new one, or it would go with the previous preview's
      // and the view would mesh the body itself (mitcad#636).
      ++meshing.ready;
      meshing.standIns.push_back(standIn);
      continue;
    }
    try {
      const std::shared_ptr<geometry::Shape> copy = geometry::independent_copy(*shape);
      pending.push_back(copy->occt());
      meshed.push_back(copy);
      bodies.push_back(shape);
    } catch (const Standard_Failure&) {
      // Left to the view, as a shape the meshing did not finish.
    } catch (const std::exception&) {
    }
  }
  if (!pending.empty() && !job.control().is_cancelled()) {
    job.setStage(QCoreApplication::translate("DisplayMeshes", "Meshing the bodies for display"));
    const occ::handle<JobBreak> progress = new JobBreak(job);
    try {
      // The faces the command left as they were, as the view shows them:
      // only bodies a job meshed to the end in the final tier, which
      // nothing writes into any more.
      std::vector<TopoDS_Shape> given;
      for (const std::shared_ptr<geometry::Shape>& donor : donors) {
        if (donor && donor->display_meshed() && !donor->display_interactive() &&
            io::has_triangulation(donor->occt())) {
          given.push_back(donor->occt());
        }
      }
      if (!given.empty()) {
        for (const TopoDS_Shape& shape : pending) {
          meshing.reused += io::reuse_triangulations(shape, given);
        }
      }
      const io::MeshOptions options = tier == DisplayTier::Final ? viewMeshOptions() : previewMeshOptions();
      meshing.bodies = io::mesh_all_in_place(pending, options, progress->Start(), &meshing.report);
      // Meshed to the end: a face still without a triangulation is one
      // OCCT's mesher fails on. The view draws the others and does not
      // mesh the body again on the UI thread, where meshing such a face
      // again could hold the window without end (mitcad#589). So too when
      // a cancel abandoned a face that did not end (mitcad#846): the view
      // draws the faces meshed by then, and never meshes that face itself.
      const bool abandoned = meshing.report.abandoned > 0;
      if ((!job.control().is_cancelled() && meshing.bodies == pending.size()) || abandoned) {
        for (std::size_t i = 0; i < pending.size(); ++i) {
          meshed[i]->set_display_meshed();
          if (!io::has_triangulation(pending[i])) {
            ++meshing.incomplete;
          }
          if (tier == DisplayTier::Interactive) {
            meshed[i]->set_stand_in_for(bodies[i]);
            bodies[i]->set_display_stand_in(meshed[i]);
            meshing.standIns.push_back(meshed[i]);
          } else {
            bodies[i]->set_display_stand_in(nullptr);
          }
        }
      }
    } catch (const Standard_Failure&) {
      // The view meshes them when it draws them, as before.
    } catch (const std::exception&) {
    }
  }
  meshing.milliseconds = clock.elapsed();
  return meshing;
}

QByteArray displayMeshStoreOptions() {
  const io::MeshOptions options = viewMeshOptions();
  return QStringLiteral("final deviation=%1 angle=%2 relative=%3 face_triangles=%4 fast_planes=%5 check_planes=%6")
             .arg(options.linear_deflection, 0, 'g', 17)
             .arg(options.angular_deflection, 0, 'g', 17)
             .arg(options.relative ? 1 : 0)
             .arg(static_cast<qulonglong>(options.face_triangles))
             .arg(options.fast_planes ? 1 : 0)
             .arg(options.check_planes ? 1 : 0)
             .toUtf8() +
         // The deviation by size, the faces' angle and the hold (mitcad#1275).
         QStringLiteral(" size_deviation=%1 size_deviation_max=%2 interior_angle=%3 hold=%4")
             .arg(options.size_deviation, 0, 'g', 17)
             .arg(options.size_deviation_max, 0, 'g', 17)
             .arg(options.interior_angle, 0, 'g', 17)
             .arg(options.hold_face_triangles ? 1 : 0)
             .toUtf8() +
         // Small circles by their size or not (mitcad#879): a design meshed
         // with the other choice is meshed again, not shown with its meshes.
         QStringLiteral(" small_circles=%1").arg(options.small_circle_segments).toUtf8();
}

DisplayRemeshing remeshForDisplay(const DisplayShapes& shapes, ModelJob& job) {
  DisplayRemeshing remeshing;
  QElapsedTimer clock;
  clock.start();
  std::vector<TopoDS_Shape> copies;
  std::set<const TopoDS_TShape*> seen;
  for (const std::shared_ptr<geometry::Shape>& shape : shapes) {
    const TopoDS_Shape& occt = shape->occt();
    if (!shape->display_meshed() || shape->display_interactive() || shape->display_stand_in() ||
        io::is_mesh(occt) || !io::has_triangulation(occt) || !seen.insert(occt.TShape().get()).second) {
      continue; // not drawn with the final tier's meshes, or met already
    }
    try {
      const std::shared_ptr<geometry::Shape> copy = geometry::independent_copy(*shape);
      copies.push_back(copy->occt());
      remeshing.bodies.push_back(shape);
    } catch (const Standard_Failure&) {
      // Keeps the meshes it has.
    } catch (const std::exception&) {
    }
  }
  if (copies.empty() || job.control().is_cancelled()) {
    remeshing.complete = !job.control().is_cancelled();
    remeshing.bodies.clear();
    remeshing.milliseconds = clock.elapsed();
    return remeshing;
  }
  job.setStage(QCoreApplication::translate("DisplayMeshes", "Meshing the bodies for display"));
  const occ::handle<JobBreak> progress = new JobBreak(job);
  try {
    const std::size_t meshed =
        io::mesh_all_in_place(copies, viewMeshOptions(), progress->Start(), &remeshing.report);
    if (meshed == copies.size() && !job.control().is_cancelled() && remeshing.report.abandoned == 0) {
      for (const TopoDS_Shape& copy : copies) {
        remeshing.meshes.push_back(io::capture_triangulations(copy));
        remeshing.triangles += io::mesh_stats(copy).triangles;
      }
      remeshing.complete = true;
    }
  } catch (const Standard_Failure&) {
  } catch (const std::exception&) {
  }
  if (!remeshing.complete) {
    remeshing.bodies.clear();
    remeshing.meshes.clear();
    remeshing.triangles = 0;
  }
  remeshing.milliseconds = clock.elapsed();
  return remeshing;
}

std::size_t giveRemeshed(const DisplayRemeshing& remeshing) {
  if (!remeshing.complete || remeshing.meshes.size() != remeshing.bodies.size()) {
    return 0;
  }
  for (const std::shared_ptr<geometry::Shape>& body : remeshing.bodies) {
    BRepTools::Clean(body->occt());
  }
  std::size_t given = 0;
  for (std::size_t i = 0; i < remeshing.bodies.size(); ++i) {
    const TopoDS_Shape& occt = remeshing.bodies[i]->occt();
    try {
      io::restore_triangulations(occt, *remeshing.meshes[i]);
      ++given;
      continue;
    } catch (const Standard_Failure&) {
    } catch (const std::exception&) {
    }
    // Not its copy's after all: meshed here, so that the view draws it.
    try {
      io::mesh_in_place(occt, viewMeshOptions());
    } catch (const Standard_Failure&) {
    } catch (const std::exception&) {
    }
  }
  return given;
}

void logRemeshing(const DisplayRemeshing& remeshing, std::size_t given, const QString& label) {
  const QString choice = viewSmallCircles() == SmallCircles::Adaptive ? QStringLiteral("adaptive")
                                                                      : QStringLiteral("full detail");
  if (!remeshing.complete) {
    qDebug().noquote() << QStringLiteral("Display meshes of %1: meshing again with small circles %2 cancelled")
                              .arg(label, choice);
    return;
  }
  qDebug().noquote() << QStringLiteral("Display meshes of %1: %2 bodies meshed again with small circles %3 in %4 ms "
                                       "(%5 triangles)")
                            .arg(label)
                            .arg(static_cast<qulonglong>(given))
                            .arg(choice)
                            .arg(remeshing.milliseconds)
                            .arg(static_cast<qulonglong>(remeshing.triangles));
  if (io::mesh_tracing()) {
    qDebug().noquote() << QStringLiteral("Mesh trace %1 again: %2")
                              .arg(label, QString::fromStdString(remeshing.report.line()));
  }
}

DisplayMeshing meshOpenedForDisplay(Document& document, ModelJob& job, bool store, DisplayMeshWrites* writes,
                                    const QString& label, RecomputeMeshing* during) {
  QElapsedTimer clock;
  clock.start();
  // The recompute is over: no batch begins beside it any more (mitcad#881).
  if (during != nullptr) {
    during->finishTaking();
  }
  std::vector<std::string> uids;
  const DisplayShapes shapes = shownBodyShapes(document, &uids);
  if (!store || !displayMeshStore()) {
    DisplayMeshing meshing = meshBeside(shapes, uids, job, during);
    countShown(meshing, shapes);
    meshing.milliseconds = clock.elapsed();
    return meshing;
  }
  // The design opened again at once: what the writer still has of its
  // earlier open is stored first, so that this open finds it (mitcad#880).
  qint64 waited = 0;
  if (writes != nullptr && writes->pending() > 0) {
    job.setStage(QCoreApplication::translate("DisplayMeshes", "Storing the bodies' display meshes"));
    waited = writes->wait(label);
  }
  const QByteArray options = displayMeshStoreOptions();
  // The bodies the view would mesh take what the store keeps for them.
  std::vector<std::string> wanted;
  std::vector<bool> readyBefore(shapes.size());
  for (std::size_t i = 0; i < shapes.size(); ++i) {
    readyBefore[i] = readyForDisplay(*shapes[i]);
    if (!readyBefore[i]) {
      wanted.push_back(uids[i]);
    }
  }
  QJsonObject restored;
  if (!wanted.empty() && !job.control().is_cancelled()) {
    job.setStage(QCoreApplication::translate("DisplayMeshes", "Reading the bodies' display meshes"));
    restored = parseObject(document.restore_display_meshes(rustStr(meshRequest(wanted, options))));
  }
  const qint64 restoreMilliseconds = clock.elapsed();
  // The rest are meshed; what is meshed to the end goes to the store.
  std::vector<bool> ready(shapes.size());
  for (std::size_t i = 0; i < shapes.size(); ++i) {
    // A body given the final tier's triangulation of every face is meshed
    // to the end as one meshed here would be: the view draws it as it is
    // (drawerMeshes, mitcad#878).
    if (!readyBefore[i] && io::has_triangulation(shapes[i]->occt())) {
      shapes[i]->set_display_meshed();
    }
    ready[i] = readyForDisplay(*shapes[i]);
  }
  // Then those meshed beside the recompute (mitcad#881): not `ready`
  // before, so stored below as the bodies meshed here are.
  DisplayMeshing meshing = meshBeside(shapes, uids, job, during);
  meshing.restored = static_cast<std::size_t>(restored.value(QStringLiteral("restored")).toArray().size());
  meshing.restoredFaces = static_cast<std::size_t>(restored.value(QStringLiteral("faces")).toInteger());
  meshing.restoreMilliseconds = restoreMilliseconds;
  meshing.waitMilliseconds = waited;
  if (!job.control().is_cancelled() && meshing.report.abandoned == 0) {
    std::vector<std::string> meshed;
    std::vector<std::size_t> indices;
    for (std::size_t i = 0; i < shapes.size(); ++i) {
      if (!ready[i] && shapes[i]->display_meshed() && !shapes[i]->display_interactive() &&
          !io::is_mesh(shapes[i]->occt())) {
        meshed.push_back(uids[i]);
        indices.push_back(i);
      }
    }
    if (!meshed.empty() && writes != nullptr) {
      // Stored after the open (mitcad#880): only the triangulations are
      // taken here, with the bodies' versions as they are now.
      QElapsedTimer capturing;
      capturing.start();
      std::vector<DisplayMeshWrites::Body> bodies;
      bodies.reserve(indices.size());
      for (const std::size_t i : indices) {
        try {
          bodies.push_back({uids[i], io::capture_triangulations(shapes[i]->occt())});
        } catch (const Standard_Failure& failure) {
          meshing.storeErrors.append(QStringLiteral("%1: %2").arg(QString::fromStdString(uids[i]),
                                                                  QString::fromUtf8(failure.what())));
        } catch (const std::exception& error) {
          meshing.storeErrors.append(QStringLiteral("%1: %2").arg(QString::fromStdString(uids[i]),
                                                                  QString::fromUtf8(error.what())));
        }
      }
      meshing.deferred = writes->queue(document.display_mesh_write(rustStr(meshRequest(meshed, options))),
                                       std::move(bodies), label);
      meshing.captureMilliseconds = capturing.elapsed();
    } else if (!meshed.empty()) {
      QElapsedTimer storing;
      storing.start();
      const QJsonObject stored =
          parseObject(document.persist_display_meshes(rustStr(meshRequest(meshed, options))));
      meshing.stored = static_cast<std::size_t>(stored.value(QStringLiteral("stored")).toInteger());
      meshing.storedBytes = stored.value(QStringLiteral("bytes")).toInteger();
      meshing.storeMilliseconds = storing.elapsed();
      for (const QJsonValue& error : stored.value(QStringLiteral("errors")).toArray()) {
        meshing.storeErrors.append(error.toString());
      }
    }
  }
  countShown(meshing, shapes);
  meshing.milliseconds = clock.elapsed();
  return meshing;
}

// One open's bodies queued to be written.
struct DisplayMeshWrites::Batch {
  explicit Batch(rust::Box<DisplayMeshWrite> made) : write(std::move(made)) {}
  rust::Box<DisplayMeshWrite> write;
  std::vector<Body> bodies;
  std::size_t next = 0; // the first body not taken; all taken or dropped at bodies.size()
  DisplayMeshWriteReport report;
  QElapsedTimer clock; // from the first body taken
};

DisplayMeshWrites::DisplayMeshWrites(bool started) : m_started(started) {}

DisplayMeshWrites::~DisplayMeshWrites() {
  drop();
  {
    const std::lock_guard<std::mutex> lock(m_mutex);
    m_stopping = true;
  }
  m_changed.notify_all();
  if (m_thread.joinable()) {
    m_thread.join();
  }
}

std::size_t DisplayMeshWrites::queue(rust::Box<DisplayMeshWrite> write, std::vector<Body> bodies,
                                     const QString& label) {
  auto batch = std::make_shared<Batch>(std::move(write));
  batch->report.label = label;
  for (Body& body : bodies) {
    if (body.triangulations && batch->write->has_body(rust::Str(body.uid))) {
      batch->bodies.push_back(std::move(body));
    }
  }
  const std::size_t count = batch->bodies.size();
  if (count == 0) {
    return 0;
  }
  {
    const std::lock_guard<std::mutex> lock(m_mutex);
    m_batches.push_back(std::move(batch));
    if (!m_thread.joinable()) {
      m_thread = std::thread([this] { run(); });
    }
  }
  m_changed.notify_all();
  return count;
}

qint64 DisplayMeshWrites::wait(const QString& label) {
  QElapsedTimer clock;
  clock.start();
  std::unique_lock<std::mutex> lock(m_mutex);
  if (!m_started) {
    m_started = true;
    m_changed.notify_all();
  }
  // A batch stays queued until the writer has finished it (logged it).
  m_changed.wait(lock, [this, &label] {
    return std::none_of(m_batches.begin(), m_batches.end(), [&label](const std::shared_ptr<Batch>& batch) {
      return label.isEmpty() || batch->report.label == label;
    });
  });
  return clock.elapsed();
}

std::size_t DisplayMeshWrites::drop() {
  std::vector<std::shared_ptr<Batch>> finished;
  std::size_t dropped = 0;
  std::unique_lock<std::mutex> lock(m_mutex);
  for (auto it = m_batches.begin(); it != m_batches.end();) {
    const std::shared_ptr<Batch>& batch = *it;
    const std::size_t left = batch->bodies.size() - batch->next;
    dropped += left;
    batch->report.dropped += left;
    for (std::size_t i = batch->next; i < batch->bodies.size(); ++i) {
      batch->bodies[i].triangulations.reset();
    }
    batch->next = batch->bodies.size();
    // The writer's own batch it finishes itself, after its body.
    if (batch == m_writing) {
      ++it;
    } else {
      finished.push_back(batch);
      it = m_batches.erase(it);
    }
  }
  lock.unlock();
  for (const std::shared_ptr<Batch>& batch : finished) {
    finish(*batch);
  }
  lock.lock();
  m_changed.notify_all();
  m_changed.wait(lock, [this] { return m_writing == nullptr && m_batches.empty(); });
  return dropped;
}

void DisplayMeshWrites::start() {
  {
    const std::lock_guard<std::mutex> lock(m_mutex);
    m_started = true;
  }
  m_changed.notify_all();
}

std::size_t DisplayMeshWrites::pending() const {
  const std::lock_guard<std::mutex> lock(m_mutex);
  std::size_t count = 0;
  for (const std::shared_ptr<Batch>& batch : m_batches) {
    count += batch->bodies.size() - batch->next;
  }
  return count + (m_writingBody ? 1 : 0);
}

void DisplayMeshWrites::setReporter(std::function<void(const DisplayMeshWriteReport&)> reporter) {
  const std::lock_guard<std::mutex> lock(m_mutex);
  m_reporter = std::move(reporter);
}

void DisplayMeshWrites::run() {
  std::unique_lock<std::mutex> lock(m_mutex);
  for (;;) {
    m_changed.wait(lock, [this] { return m_stopping || (m_started && !m_batches.empty()); });
    if (m_stopping) {
      return;
    }
    const std::shared_ptr<Batch> batch = m_batches.front();
    m_writing = batch;
    if (batch->next >= batch->bodies.size()) {
      // Every body written or dropped: logged, then no longer queued.
      lock.unlock();
      finish(*batch);
      lock.lock();
      m_batches.pop_front();
      m_writing.reset();
      m_changed.notify_all();
      continue;
    }
    if (batch->next == 0) {
      batch->clock.start();
    }
    Body body = std::move(batch->bodies[batch->next++]);
    m_writingBody = true;
    lock.unlock();
    qint64 bytes = 0;
    QString error;
    try {
      std::string data = io::triangulation_bytes(*body.triangulations);
      body.triangulations.reset();
      bytes = static_cast<qint64>(batch->write->write_body(
          rust::Str(body.uid),
          rust::Slice<const std::uint8_t>(reinterpret_cast<const std::uint8_t*>(data.data()), data.size())));
    } catch (const Standard_Failure& failure) {
      error = QStringLiteral("%1: %2").arg(QString::fromStdString(body.uid), QString::fromUtf8(failure.what()));
    } catch (const std::exception& failure) {
      // (write_body's errors name the body.)
      error = QString::fromUtf8(failure.what());
    }
    lock.lock();
    m_writingBody = false;
    if (error.isEmpty()) {
      ++batch->report.stored;
      batch->report.bytes += bytes;
    } else {
      batch->report.errors.append(error);
    }
    batch->report.milliseconds = batch->clock.isValid() ? batch->clock.elapsed() : 0;
    m_changed.notify_all();
  }
}

void DisplayMeshWrites::finish(Batch& batch) {
  const DisplayMeshWriteReport& report = batch.report;
  if (report.stored > 0) {
    qInfo().noquote() << QStringLiteral("Display meshes of %1: stored %2 bodies (%3 bytes) in %4 ms after the open")
                             .arg(report.label)
                             .arg(static_cast<qulonglong>(report.stored))
                             .arg(report.bytes)
                             .arg(report.milliseconds);
  }
  if (report.dropped > 0) {
    qInfo().noquote() << QStringLiteral("Display meshes of %1: %2 bodies not stored (dropped)")
                             .arg(report.label)
                             .arg(static_cast<qulonglong>(report.dropped));
  }
  for (const QString& error : report.errors) {
    qWarning().noquote() << QStringLiteral("Display meshes of %1: not stored: %2").arg(report.label, error);
  }
  std::function<void(const DisplayMeshWriteReport&)> reporter;
  {
    const std::lock_guard<std::mutex> lock(m_mutex);
    reporter = m_reporter;
  }
  if (reporter) {
    reporter(report);
  }
}

namespace {

std::mutex g_writesMutex;
std::unique_ptr<DisplayMeshWrites> g_writes;
bool g_writesFinished = false;

} // namespace

DisplayMeshWrites* displayMeshWrites() {
  const QByteArray given = qgetenv("MITCAD_DISPLAY_MESH_WRITER");
  if (given == "0" || given == "off") {
    return nullptr;
  }
  const std::lock_guard<std::mutex> lock(g_writesMutex);
  if (g_writesFinished) {
    return nullptr;
  }
  if (!g_writes) {
    g_writes = std::make_unique<DisplayMeshWrites>();
  }
  return g_writes.get();
}

void finishDisplayMeshWrites() {
  std::unique_ptr<DisplayMeshWrites> writes;
  {
    const std::lock_guard<std::mutex> lock(g_writesMutex);
    g_writesFinished = true;
    writes = std::move(g_writes);
  }
  writes.reset();
}

std::unique_ptr<RecomputeMeshing> RecomputeMeshing::start(Document& document, const ModelJob& job) {
  if (!meshDuringRecompute()) {
    return nullptr;
  }
  // (The options here: the thread reads no settings.)
  return std::make_unique<RecomputeMeshing>(document.display_feed(rustStr(displayMeshStoreOptions())), job,
                                            viewMeshOptions());
}

RecomputeMeshing::RecomputeMeshing(rust::Box<DisplayFeed> feed, const ModelJob& job, const io::MeshOptions& options)
    : m_feed(std::move(feed)), m_job(job), m_options(options) {
  m_clock.start();
  m_thread = std::thread([this] { run(); });
}

RecomputeMeshing::~RecomputeMeshing() {
  m_stopping = true;
  finishTaking();
  if (m_thread.joinable()) {
    m_thread.join();
  }
}

void RecomputeMeshing::finishTaking() {
  const std::lock_guard<std::mutex> lock(m_mutex);
  if (!m_finishing) {
    m_finishing = true;
    m_finishedAt = m_clock.elapsed();
  }
}

std::vector<std::string> RecomputeMeshing::underWay() const {
  const std::lock_guard<std::mutex> lock(m_mutex);
  return m_underWay;
}

std::vector<RecomputeMeshing::Body> RecomputeMeshing::takeMeshed() {
  const std::lock_guard<std::mutex> lock(m_mutex);
  return std::exchange(m_meshed, {});
}

qint64 RecomputeMeshing::finish() {
  QElapsedTimer clock;
  clock.start();
  finishTaking();
  if (m_thread.joinable()) {
    m_thread.join();
  }
  // What the recompute put in and no batch took: meshed by the open.
  std::unique_ptr<bridge::ShapeList> shapes = bridge::new_shape_list();
  m_left += m_feed->take(0, *shapes).size();
  return clock.elapsed();
}

std::size_t RecomputeMeshing::given() const { return m_feed->given(); }
std::size_t RecomputeMeshing::leftToStore() const { return m_feed->left_to_store(); }
std::size_t RecomputeMeshing::changedLater() const { return m_feed->changed_later(); }

std::size_t RecomputeMeshing::doneSoFar() const {
  const std::lock_guard<std::mutex> lock(m_mutex);
  return m_doneSoFar;
}

void RecomputeMeshing::run() {
  // How long a take waits for bodies before it looks for a stop again.
  constexpr std::uint32_t kWaitMs = 50;
  for (;;) {
    {
      const std::lock_guard<std::mutex> lock(m_mutex);
      if (m_finishing || m_stopping) {
        break;
      }
    }
    std::unique_ptr<bridge::ShapeList> shapes = bridge::new_shape_list();
    const rust::Vec<rust::String> uids = m_feed->take(kWaitMs, *shapes);
    if (uids.empty()) {
      if (m_feed->is_done()) {
        break;
      }
      continue;
    }
    const qint64 began = m_clock.elapsed();
    std::vector<TopoDS_Shape> pending;
    std::vector<Body> batch;
    std::vector<std::shared_ptr<geometry::Shape>> copies;
    {
      // The copies are made before the batch is under way, so that once
      // the open sees it under way, nothing here reads the bodies any
      // more; put in after the recompute ended, the bodies are the open's.
      const std::lock_guard<std::mutex> lock(m_mutex);
      if (m_finishing || m_stopping) {
        m_left += uids.size();
        m_doneSoFar += uids.size();
        break;
      }
      for (std::size_t i = 0; i < uids.size() && i < shapes->size(); ++i) {
        std::shared_ptr<geometry::Shape> shape = shapes->at(i);
        if (!shape || shape->occt().IsNull() || io::is_mesh(shape->occt()) || shape->display_meshed()) {
          continue;
        }
        // A body whose shape is one taken before in another place (a
        // pattern's copy, the body moved as a location, mitcad#1275): its
        // faces are that one's, and get their meshes with it; the open
        // then finds it meshed (or meshes the faces once for both).
        if (!m_taken.insert(shape->occt().TShape().get()).second) {
          continue;
        }
        m_takenShapes.push_back(shape->occt());
        // A copy that shares no sub-shape with the body: meshing it writes
        // nothing the recompute may read (the class comment).
        try {
          std::shared_ptr<geometry::Shape> copy = geometry::independent_copy(*shape);
          pending.push_back(copy->occt());
          copies.push_back(std::move(copy));
          batch.push_back({std::string(uids[i]), std::move(shape), nullptr, true});
          m_underWay.push_back(batch.back().uid);
        } catch (const Standard_Failure&) {
          ++m_left;
        } catch (const std::exception&) {
          ++m_left;
        }
      }
      // (Skipped, or not copied: meshed by the open.)
      m_doneSoFar += uids.size() - batch.size();
    }
    if (pending.empty()) {
      continue;
    }
    const occ::handle<StopBreak> progress = new StopBreak(m_job, m_stopping);
    io::MeshReport report;
    std::vector<Body> meshed;
    std::size_t failed = 0;
    try {
      const std::size_t count = io::mesh_all_in_place(pending, m_options, progress->Start(), &report);
      if (count == pending.size() && report.abandoned == 0 && !m_stopping && !m_job.control().is_cancelled()) {
        for (std::size_t i = 0; i < batch.size(); ++i) {
          try {
            batch[i].meshes = io::capture_triangulations(copies[i]->occt());
            batch[i].complete = io::has_triangulation(copies[i]->occt());
            meshed.push_back(std::move(batch[i]));
          } catch (const Standard_Failure&) {
            ++failed;
          } catch (const std::exception&) {
            ++failed;
          }
        }
      } else {
        failed = batch.size();
      }
    } catch (const Standard_Failure&) {
      failed = batch.size();
    } catch (const std::exception&) {
      failed = batch.size();
    }
    const qint64 ended = m_clock.elapsed();
    {
      const std::lock_guard<std::mutex> lock(m_mutex);
      m_underWay.clear();
      m_doneSoFar += batch.size();
      for (Body& body : meshed) {
        m_meshed.push_back(std::move(body));
      }
      m_left += failed;
      if (m_finishedAt >= 0) {
        m_afterMilliseconds += std::max<qint64>(0, ended - std::max(began, m_finishedAt));
      }
    }
    m_milliseconds += ended - began;
    m_trace.append(QStringLiteral("batch of %1 bodies at %2 ms for %3 ms: %4")
                       .arg(static_cast<qulonglong>(batch.size()))
                       .arg(began)
                       .arg(ended - began)
                       .arg(QString::fromStdString(report.line())));
  }
}

std::shared_ptr<geometry::Shape> displayShape(const std::shared_ptr<geometry::Shape>& shape) {
  if (shape) {
    if (std::shared_ptr<geometry::Shape> standIn = shape->display_stand_in()) {
      return standIn;
    }
  }
  return shape;
}

DisplayShapes refinableShapes(const DisplayShapes& shown) {
  DisplayShapes bodies;
  for (const std::shared_ptr<geometry::Shape>& shape : shown) {
    if (!shape || !shape->display_interactive()) {
      continue;
    }
    std::shared_ptr<geometry::Shape> body = shape->stand_in_for();
    if (body && !body->display_meshed() && !io::has_triangulation(body->occt()) &&
        std::find(bodies.begin(), bodies.end(), body) == bodies.end()) {
      bodies.push_back(std::move(body));
    }
  }
  return bodies;
}

void logDisplayMeshing(const DisplayMeshing& meshing, const QString& label) {
  if (io::mesh_tracing()) {
    qDebug().noquote() << QStringLiteral(
                              "Mesh trace %1 %2: %3 bodies ready, %4 meshed, %5 faces reused, %6 faces from the "
                              "result store; %7")
                              .arg(label, meshing.tier == DisplayTier::Final ? QStringLiteral("final")
                                                                             : QStringLiteral("interactive"))
                              .arg(static_cast<qulonglong>(meshing.ready))
                              .arg(static_cast<qulonglong>(meshing.bodies))
                              .arg(static_cast<qulonglong>(meshing.reused))
                              .arg(static_cast<qulonglong>(meshing.restoredFaces))
                              .arg(QString::fromStdString(meshing.report.line()));
  }
  // The result store's display meshes (mitcad#858): the faces taken from
  // it against those meshed.
  if (meshing.restored > 0) {
    qDebug().noquote() << QStringLiteral("Display meshes of %1: %2 bodies (%3 faces) from the result store in %4 ms; "
                                         "%5 bodies (%6 faces) meshed")
                              .arg(label)
                              .arg(static_cast<qulonglong>(meshing.restored))
                              .arg(static_cast<qulonglong>(meshing.restoredFaces))
                              .arg(meshing.restoreMilliseconds)
                              .arg(static_cast<qulonglong>(meshing.bodies))
                              .arg(static_cast<qulonglong>(meshing.report.meshed));
  }
  if (meshing.shown > 0) {
    qDebug().noquote() << QStringLiteral("Display meshes of %1: %2 bodies shown, %3 triangles")
                              .arg(label)
                              .arg(static_cast<qulonglong>(meshing.shown))
                              .arg(static_cast<qulonglong>(meshing.triangles));
  }
  if (meshing.stored > 0) {
    qDebug().noquote() << QStringLiteral("Display meshes of %1: stored %2 bodies (%3 bytes) in %4 ms")
                              .arg(label)
                              .arg(static_cast<qulonglong>(meshing.stored))
                              .arg(meshing.storedBytes)
                              .arg(meshing.storeMilliseconds);
  }
  if (meshing.deferred > 0) {
    qDebug().noquote() << QStringLiteral("Display meshes of %1: %2 bodies to store after the open (their "
                                         "triangulations taken in %3 ms)")
                              .arg(label)
                              .arg(static_cast<qulonglong>(meshing.deferred))
                              .arg(meshing.captureMilliseconds);
  }
  if (io::mesh_tracing()) {
    for (const QString& line : meshing.duringTrace) {
      qDebug().noquote() << QStringLiteral("Mesh trace %1 during the recompute: %2").arg(label, line);
    }
  }
  if (meshing.duringGiven > 0) {
    qDebug().noquote() << QStringLiteral("Display meshes of %1: %2 bodies meshed during the recompute in %3 ms, %4 ms "
                                         "of it after the recompute, beside the open's own meshing, which then waited "
                                         "%5 ms for its last batch; %6 bodies final before its end, %7 left to the "
                                         "result store, %8 meshed after it, %9 changed after they were meshed")
                              .arg(label)
                              .arg(static_cast<qulonglong>(meshing.during))
                              .arg(meshing.duringMilliseconds)
                              .arg(meshing.duringAfterMilliseconds)
                              .arg(meshing.duringWaitMilliseconds)
                              .arg(static_cast<qulonglong>(meshing.duringGiven))
                              .arg(static_cast<qulonglong>(meshing.duringLeftToStore))
                              .arg(static_cast<qulonglong>(meshing.duringLeft))
                              .arg(static_cast<qulonglong>(meshing.duringStale));
  }
  if (meshing.waitMilliseconds > 0) {
    qInfo().noquote() << QStringLiteral("Display meshes of %1: waited %2 ms for those of its previous open to be "
                                        "stored")
                             .arg(label)
                             .arg(meshing.waitMilliseconds);
  }
  for (const QString& error : meshing.storeErrors) {
    qWarning().noquote() << QStringLiteral("Display meshes of %1: not stored: %2").arg(label, error);
  }
  if (meshing.bodies > 0) {
    qDebug().noquote() << QStringLiteral("Display meshes of %1: %2 bodies in %3 ms on the model's thread%4")
                              .arg(label)
                              .arg(static_cast<qulonglong>(meshing.bodies))
                              .arg(meshing.milliseconds)
                              .arg(meshing.tier == DisplayTier::Interactive ? QStringLiteral(" (interactive)")
                                                                            : QString());
  }
  if (meshing.report.abandoned > 0) {
    qWarning().noquote() << QStringLiteral("Display meshes of %1: cancelled, %2 faces that did not end abandoned; "
                                           "%3 bodies drawn without the faces not meshed by then")
                                .arg(label)
                                .arg(static_cast<qulonglong>(meshing.report.abandoned))
                                .arg(static_cast<qulonglong>(meshing.incomplete));
  } else if (meshing.incomplete > 0) {
    qWarning().noquote() << QStringLiteral("Display meshes of %1: %2 bodies with faces that could not be meshed "
                                           "(not drawn)")
                                .arg(label)
                                .arg(static_cast<qulonglong>(meshing.incomplete));
  }
}

} // namespace mitcad
