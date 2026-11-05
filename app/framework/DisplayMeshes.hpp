// SPDX-License-Identifier: MIT
#pragma once

// The 3D view's meshes, made on the model's worker thread (mitcad#398).
// OCCT meshes a shape for display when it is first drawn, on the UI thread;
// a design of large curved faces then held the window (and every MCP call
// with it) for as long as that took. The jobs that change what the view
// shows (opening, model commands, undo and redo, previews) therefore end
// with the meshes of the bodies the view will draw, made as the view makes
// them (viewMeshOptions, view/ViewTessellation.hpp), while the job still
// owns the document: the window waits as for any job (progress dialog after
// 400 ms, MCP queries answered busy with a time to retry), and the view
// finds the triangulations and keeps them. Cancel stops the meshing only
// (what the job computed stays); the view meshes what is left. A face whose
// triangulation does not end lets the cancel return all the same, within
// about io::kMeshAbandonSeconds (mitcad#846): the meshing is abandoned in
// the background, and the view draws the bodies with the faces meshed by
// then and never meshes the others itself.
//
// Two tiers (mitcad#635). The final tier is the view's meshes as above,
// written into the bodies' own faces. A preview's bodies are meshed in the
// interactive tier: each is drawn through a stand-in, a copy of it that
// shares no face, edge or vertex (geometry::independent_copy) meshed with
// previewMeshOptions, and the body's own faces stay without triangulations
// until a job meshes it in the final tier (the command that commits the
// preview, or refineDisplay once a drag ends). So the two tiers never meet
// in one face, a body the view draws is never meshed again on another
// thread, and a committed body never keeps a preview's meshes. Both tiers
// first take the triangulations of the faces a preview or command left as
// they were from the bodies as the view shows them (io::reuse_
// triangulations): only the faces it changed are meshed.

#include <atomic>
#include <condition_variable>
#include <cstddef>
#include <deque>
#include <functional>
#include <memory>
#include <mutex>
#include <set>
#include <string>
#include <thread>
#include <vector>

#include <TopoDS_Shape.hxx>
#include <TopoDS_TShape.hxx>

#include <QByteArray>
#include <QElapsedTimer>
#include <QJsonObject>
#include <QString>
#include <QStringList>
#include <QtGlobal>

#include "ModelWorker.hpp"
#include "mitcad/geometry/shape.hpp"
#include "mitcad/io/mesh.hpp"
#include "mitcad/io/mesh_store.hpp"

namespace mitcad {

using DisplayShapes = std::vector<std::shared_ptr<geometry::Shape>>;

enum class DisplayTier {
  Final,       // viewMeshOptions, in the bodies' own faces
  Interactive, // previewMeshOptions, in stand-ins
};

// The shown bodies' shapes (the `instances` query's visible bodies, each
// once), and with `uids` their bodies' uids in the same order. Called on
// the thread that owns the document.
DisplayShapes shownBodyShapes(const Document& document, std::vector<std::string>* uids = nullptr);

// A preview's changed and made bodies (its report's `bodies` with
// `changed`, and `instances`) and its tool body.
DisplayShapes previewShapes(const Document& document, const QJsonObject& report);

// The bodies a preview changes as the document has them before it (its
// report's `bodies` with `changed`): the donors of the faces it left as
// they were (meshForDisplay).
DisplayShapes previewDonors(const Document& document, const QJsonObject& report);

// What meshForDisplay did.
struct DisplayMeshing {
  std::size_t bodies = 0; // shapes meshed
  std::size_t incomplete = 0; // of them, shapes with faces OCCT's mesher failed on
  std::size_t ready = 0; // shapes that had a triangulation on every face, or a stand-in (not meshed)
  std::size_t reused = 0; // faces that took a donor's triangulation
  DisplayTier tier = DisplayTier::Final;
  qint64 milliseconds = 0;
  // The stand-ins made or drawn again (the interactive tier): the caller
  // keeps them for the view, whose bodies hold them only weakly.
  DisplayShapes standIns;
  io::MeshReport report; // the meshing's faces and phases (MITCAD_MESH_TRACE, mitcad#635)
  // The result store's display meshes (meshOpenedForDisplay, mitcad#858):
  // shapes and faces given their stored triangulations (counted in
  // `ready`, not meshed) and how long that took; shapes whose meshes were
  // stored after the meshing, their bytes, the time and what failed.
  std::size_t restored = 0;
  std::size_t restoredFaces = 0;
  qint64 restoreMilliseconds = 0;
  std::size_t stored = 0;
  qint64 storedBytes = 0;
  qint64 storeMilliseconds = 0;
  QStringList storeErrors;
  // With a writer (DisplayMeshWrites, mitcad#880) the meshes are stored
  // after the open instead: the shapes whose triangulations were taken for
  // it (`stored` stays 0) and how long taking them took; and how long the
  // open waited for the writer to store an earlier open's of the design.
  std::size_t deferred = 0;
  qint64 captureMilliseconds = 0;
  qint64 waitMilliseconds = 0;
  // Meshed during the recompute (RecomputeMeshing, mitcad#881): shapes
  // given the meshes made beside the recompute (counted in `ready`, not in
  // `bodies`, and stored as meshed ones), those meshed then whose body
  // changed after all (meshed again here); the bodies the recompute put
  // in, those left to the result store's meshes and those not meshed in
  // time (meshed here); how long its batches meshed, how much of that came
  // after the recompute (beside the open's own meshing), and how long the
  // open then waited for its last batch.
  std::size_t during = 0;
  std::size_t duringStale = 0;
  std::size_t duringGiven = 0;
  std::size_t duringLeftToStore = 0;
  std::size_t duringLeft = 0;
  qint64 duringMilliseconds = 0;
  qint64 duringAfterMilliseconds = 0;
  qint64 duringWaitMilliseconds = 0;
  QStringList duringTrace; // RecomputeMeshing::trace
  // An open's shown bodies and their triangles once meshed
  // (meshOpenedForDisplay, with or without the store).
  std::size_t shown = 0;
  std::size_t triangles = 0;
};

// Meshes the shapes that have faces without a triangulation as the view
// does, in the job (its stage says so; its cancel stops the meshing), all
// of them together, so that their faces are meshed in parallel on every
// core (io::mesh_all_in_place, mitcad#438). A shape meshed to the end is
// marked so (geometry::Shape::display_meshed): a face OCCT's mesher failed
// on is not drawn, and neither this nor the view meshes it again
// (mitcad#589: meshing such a face again took without end, on the UI
// thread). A shape left when the meshing stopped or threw is left to the
// view, as before; but when a cancel abandoned a face that did not end
// (io::MeshReport::abandoned, mitcad#846), every shape is marked as meshed
// to the end, its faces without a triangulation not drawn.
//
// In the interactive tier each such shape gets a stand-in meshed instead
// (see above), unless it has one; a shape that has a triangulation on
// every face (a body shown already) is drawn as it is. In the final tier a
// shape meshed to the end drops its stand-in. `donors` are the bodies as
// the view shows them, meshed in the final tier, whose untouched faces the
// shapes may take (only bodies meshed to the end by a job are read).
DisplayMeshing meshForDisplay(const DisplayShapes& shapes, ModelJob& job, DisplayTier tier = DisplayTier::Final,
                              const DisplayShapes& donors = {});

// The shown bodies of a design being opened meshed for display with the
// result store (mitcad#858): a body whose shape (by its version) has
// display triangulations stored with the final tier's options
// (displayMeshStoreOptions) takes them back and is not meshed; the others
// go through meshForDisplay, and once meshed to the end (no cancel, no face
// abandoned) their triangulations go to the store. A body changed since
// (another version), other options or another build of the program find
// none and are meshed as before. Without `store` (the document has none),
// or with MITCAD_DISPLAY_MESH_STORE=0, just meshForDisplay of the shown
// bodies.
//
// With `writes` (the window's, mitcad#880) the open does not wait for the
// store: the job takes the meshed bodies' triangulations
// (io::capture_triangulations, quick) and the writer stores them on its
// own thread after the open; the meshing's `deferred` counts them. Before
// it reads the store, an open waits for the writer to finish what an
// earlier open of the same `label` (the design's name) queued, so that a
// design opened again at once finds its meshes. Without a writer, they are
// stored in the job as before.
//
// With `during` (mitcad#881), the bodies it meshed beside the recompute
// take its meshes (each body whose shape is still the one meshed), after
// those the store gives back, and count as meshed here; the others are
// meshed here while its last batch goes on, and that batch's bodies take
// its meshes once it ends.
class DisplayMeshWrites;
class RecomputeMeshing;
DisplayMeshing meshOpenedForDisplay(Document& document, ModelJob& job, bool store,
                                    DisplayMeshWrites* writes = nullptr, const QString& label = QString(),
                                    RecomputeMeshing* during = nullptr);

// Display meshes made while an open's recompute goes on (mitcad#881): a
// design whose bodies are final long before the timeline's end spent a
// third of its open meshing them after the recompute, on every core while
// the recompute had used one. The recompute puts each shown body into a
// feed once no later feature can change it (Document::display_feed,
// mitcad_model::FinalBodies, which says which features change which
// bodies), and this meshes them on a thread of its own meanwhile, in
// batches of what came, all of a batch at once (io::mesh_all_in_place).
//
// Ownership: the body shapes are the recompute's; the mesher only reads
// them, as the recompute's own operations read their inputs (they never
// write into them, mitcad#95), to make a copy of each that shares no face,
// edge or vertex (geometry::independent_copy) and meshes the copy, never
// the body: a body may still be read by later features, and a triangulated
// one would change what they compute (bounding boxes from triangulations).
// The copies' triangulations are taken by their handles
// (io::capture_triangulations) on the mesher's thread and handed over
// under a lock; the model's thread alone gives each body whose shape is
// still the one meshed (the same object) those triangulations
// (io::restore_triangulations of a capture, every face checked first),
// once the recompute is over and the body no longer read by anyone.
//
// Once the recompute is over (finishTaking) no batch begins; the bodies of
// the batch under way (underWay) are left out while the open meshes the
// others, so that both meshings share the cores (one after the other, a
// batch whose slowest faces took long held the rest back), and they take
// their meshes after it (finish waits for the batch). A body changed
// after all (a feature the classification missed), a batch cut short and
// the bodies not yet taken are meshed after the recompute as before. The
// job's cancel stops the meshing (a face that does not end is abandoned,
// mitcad#846); its batch then gives nothing. MITCAD_MESH_DURING_RECOMPUTE=0
// (or off): none.
class RecomputeMeshing {
public:
  struct Body {
    std::string uid;
    std::shared_ptr<geometry::Shape> shape; // the body's shape as meshed (read only)
    std::shared_ptr<const io::TriangulationCapture> meshes; // its copy's
    bool complete = true; // every face of the copy triangulated
  };

  // The document's next recompute feeds it (call right before the open's
  // recompute, with the document's result store set). Null with
  // MITCAD_MESH_DURING_RECOMPUTE=0.
  static std::unique_ptr<RecomputeMeshing> start(Document& document, const ModelJob& job);
  RecomputeMeshing(rust::Box<DisplayFeed> feed, const ModelJob& job, const io::MeshOptions& options);
  // Stops the meshing (a batch under way is abandoned) and its thread.
  ~RecomputeMeshing();
  RecomputeMeshing(const RecomputeMeshing&) = delete;
  RecomputeMeshing& operator=(const RecomputeMeshing&) = delete;

  // On the model's thread once the recompute is over: no batch begins any
  // more; what the recompute put in and no batch took is left (`left`).
  void finishTaking();
  // The uids of the batch under way (after finishTaking: the last).
  std::vector<std::string> underWay() const;
  // The bodies meshed since the last call, at most one per uid over all.
  std::vector<Body> takeMeshed();
  // Bodies taken from the feed and done with, meshed or not (a test waits
  // for them to reach `given`).
  std::size_t doneSoFar() const;
  // Waits for the batch under way and the thread; how long it waited (ms).
  qint64 finish();

  std::size_t given() const; // bodies the recompute put in
  std::size_t leftToStore() const; // of them, left to the store's meshes
  std::size_t changedLater() const; // of them, changed by a later feature all the same
  // After finish: the bodies left to the open (not taken, or a batch cut
  // short), the batches' meshing time summed and the part of it after the
  // recompute, and a line per batch: its bodies, when it began (ms from
  // the start), how long it took and its io::MeshReport line
  // (MITCAD_MESH_TRACE).
  std::size_t left() const { return m_left; }
  qint64 milliseconds() const { return m_milliseconds; }
  qint64 afterMilliseconds() const { return m_afterMilliseconds; }
  const QStringList& trace() const { return m_trace; }

private:
  void run();

  rust::Box<DisplayFeed> m_feed;
  const ModelJob& m_job;
  io::MeshOptions m_options;
  std::atomic<bool> m_stopping{false};
  QElapsedTimer m_clock; // from the start
  mutable std::mutex m_mutex;
  // Under the mutex: whether batches may begin, when the recompute ended
  // (ms), the batch under way and the bodies meshed not yet taken.
  bool m_finishing = false;
  qint64 m_finishedAt = -1;
  std::vector<std::string> m_underWay;
  std::vector<Body> m_meshed;
  std::size_t m_doneSoFar = 0;
  // The thread's until it is joined.
  std::size_t m_left = 0;
  qint64 m_milliseconds = 0;
  qint64 m_afterMilliseconds = 0;
  QStringList m_trace;
  // The thread's: the shapes (their TShapes, held) of the bodies taken, so
  // that another place of one (a pattern's copy, mitcad#1275) is not
  // meshed again.
  std::set<const TopoDS_TShape*> m_taken;
  std::vector<TopoDS_Shape> m_takenShapes;
  std::thread m_thread;
};

// What the writer did with one open's display meshes (DisplayMeshWrites).
struct DisplayMeshWriteReport {
  QString label;
  std::size_t stored = 0;
  std::size_t dropped = 0; // not written: dropped (a store cleared, the program quitting)
  qint64 bytes = 0;
  qint64 milliseconds = 0; // from the first body taken to the last written
  QStringList errors;
};

// The display meshes of opens, stored after the open on a thread of their
// own (mitcad#880): the 600 MB of a large design's meshes took 11 s of a
// 47 s first open, all of it in the open's job. Each open queues its
// bodies with the triangulations its job took (io::TriangulationCapture,
// which the bodies' later changes, other jobs' meshing and the document's
// close leave as they were) and their versions and store
// (DisplayMeshWrite); one thread writes them a body at a time, each file
// whole or not at all (the store's temporary file renamed), in the order
// queued.
//
// So a body's file is either the whole one or none, whatever happens
// meanwhile: a design changed or closed during the writing changes
// nothing (the entries are of the versions opened, which a later open of
// the same file looks for); drop() (the store cleared, the program
// quitting) drops what is queued and finishes only the body being written,
// and an open that then finds no file meshes the body and stores it again.
// Logs "Display meshes of <label>: stored N bodies (B bytes) in T ms after
// the open" (and what was dropped or failed) on the writer's thread.
class DisplayMeshWrites {
public:
  struct Body {
    std::string uid;
    std::shared_ptr<const io::TriangulationCapture> triangulations;
  };

  // A writer made stopped (`started` false, tests) queues until start().
  explicit DisplayMeshWrites(bool started = true);
  // Drops what is queued, finishes the body being written.
  ~DisplayMeshWrites();
  DisplayMeshWrites(const DisplayMeshWrites&) = delete;
  DisplayMeshWrites& operator=(const DisplayMeshWrites&) = delete;

  // Queues an open's bodies (those `write` has a version of; the others
  // are left out) under its label; returns how many.
  std::size_t queue(rust::Box<DisplayMeshWrite> write, std::vector<Body> bodies, const QString& label);
  // Waits until the bodies queued under `label` (any label when empty)
  // are written or dropped; returns how long it waited (ms). A stopped
  // writer is started.
  qint64 wait(const QString& label = QString());
  // Drops every body queued; the one being written is finished first.
  // Returns how many were dropped.
  std::size_t drop();
  void start();
  // Bodies queued or being written.
  std::size_t pending() const;
  // Called on the writer's thread after each open's bodies are done.
  void setReporter(std::function<void(const DisplayMeshWriteReport&)> reporter);

private:
  struct Batch;
  void run();
  void finish(Batch& batch); // logs, reports (without the lock)

  mutable std::mutex m_mutex;
  std::condition_variable m_changed;
  std::deque<std::shared_ptr<Batch>> m_batches;
  std::shared_ptr<Batch> m_writing; // the batch the writer works on (until finished)
  bool m_writingBody = false; // a body taken and not yet written
  bool m_started;
  bool m_stopping = false;
  std::thread m_thread;
  std::function<void(const DisplayMeshWriteReport&)> m_reporter;
};

// The application's writer (the window's opens); made at the first use.
// Null with MITCAD_DISPLAY_MESH_WRITER=0 (or off): opens store their
// display meshes in their job, as before mitcad#880.
DisplayMeshWrites* displayMeshWrites();
// Stops the application's writer when the program quits: what is queued is
// dropped, the body being written is finished. Nothing when none was made.
void finishDisplayMeshWrites();

// The final tier's meshing options as the result store keys display
// meshes by them: other options find none.
QByteArray displayMeshStoreOptions();

// What the view draws for a shape: its stand-in while it has one (the
// interactive tier), else the shape. Null stays null.
std::shared_ptr<geometry::Shape> displayShape(const std::shared_ptr<geometry::Shape>& shape);

// The bodies behind the stand-ins among `shown` that are still without the
// final tier's meshes: what refining a preview's display meshes (a job's
// meshForDisplay in the final tier) works on. Their faces are not drawn
// (the stand-ins are), so a job may mesh them while the view draws.
DisplayShapes refinableShapes(const DisplayShapes& shown);

// On the UI thread after the job: "Display meshes of <label>: 2 bodies in
// 85 ms on the model's thread", when it meshed any, and "Display meshes of
// <label>: 1 bodies with faces that could not be meshed (not drawn)" when
// some faces failed ("Display meshes of <label>: cancelled, 1 faces that did
// not end abandoned; ..." after an abandon, mitcad#846). With MITCAD_MESH_TRACE (io::mesh_tracing, mitcad#635)
// also "Mesh trace <label> <tier>: <n> bodies ready, <m> meshed, <r> faces
// reused, <s> faces from the result store; <the report's line>": which
// faces kept the triangulation they had, which were meshed, the phases'
// times and the slowest faces. With the result store (mitcad#858):
// "Display meshes of <label>: 90 bodies (7162 faces) from the result store
// in 240 ms; 0 bodies meshed" and "Display meshes of <label>: stored 3
// bodies (1234567 bytes) in 40 ms"; with a writer (mitcad#880) "Display
// meshes of <label>: 3 bodies to store after the open (their
// triangulations taken in 5 ms)", and the writer's own line once stored.
// With meshing during the recompute (mitcad#881): "Display meshes of
// <label>: 12 bodies meshed during the recompute in 20135 ms, 17223 ms of
// it after the recompute, ...; 33 bodies final before its end, 0 left to
// the result store, 21 meshed after it, 0 changed after they were meshed",
// and with MITCAD_MESH_TRACE a line per batch.
void logDisplayMeshing(const DisplayMeshing& meshing, const QString& label);

} // namespace mitcad
