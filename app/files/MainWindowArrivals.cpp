// SPDX-License-Identifier: MIT
// Work that arrives while a command is open (mitcad#2281, GitHub
// mitcad/Mitcad#574; docs/ui-conventions.md "Work that arrives while a
// command is open"): a Component from Web download, or files dropped on the
// window. From Idle they come in at once, as before. Beside a command that
// changes nothing in the design (CommandDef::changesNoDesign: Measure,
// Interference, Section Analysis, Print Analysis) they come in at once too,
// one undo step each, and the inspection stays open and checks its picks
// again (CommandSession::designChangedBeside); Move on a download starts
// when it closes. While a command that changes the design, a sketch or
// Create Sketch's plane pick is open they wait, said in words (the
// browser's note under its page, the message band for a drop), and come in
// when it closes, in the order they arrived, with one Move on the last
// download. Nothing asks to close the command: waiting loses nothing,
// closing would lose its inputs. A download that waits is a copy of its
// own, kept until it comes in; a design closed first drops them, said. A
// design dragged from the Projects panel onto the view (mitcad#2428) is an
// arrival too: inserted through Insert Component, which asks how, so it
// waits for an inspection as well.
#include "../MainWindow.hpp"

#include <algorithm>
#include <memory>
#include <utility>

#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QMetaObject>
#include <QScopeGuard>
#include <QTimer>
#include <QtLogging>

#include "../framework/CommandRegistry.hpp"
#include "../framework/CommandSession.hpp"
#include "../webbrowser/ComponentBrowserHost.hpp"
#include "../webbrowser/ComponentWebFiles.hpp"
#include "FileFormats.hpp"

namespace mitcad {
namespace {

// The files a drop brings in beside an inspection: those that go into this
// design at once, as one import each (STEP, IGES, BRep, glTF, meshes).
// Designs of other formats (an import of their own across the event loop,
// or a new document) and anything else wait until it closes.
bool insertsBeside(const QStringList& paths) {
  for (const QString& path : paths) {
    const FileKind kind = fileKind(path);
    if (kind != FileKind::Cad && kind != FileKind::Gltf && kind != FileKind::Mesh) {
      return false;
    }
  }
  return !paths.isEmpty();
}

QStringList fileNames(const QStringList& paths) {
  QStringList names;
  for (const QString& path : paths) {
    names << QFileInfo(path).fileName();
  }
  return names;
}

} // namespace

MainWindow::ArrivalGate MainWindow::arrivalGate() const {
  if (m_mode == Mode::Idle) {
    return ArrivalGate::Now;
  }
  // An inspection started from a sketch goes back to the sketch: that one
  // is waited for.
  if (m_mode == Mode::Command && m_session && !m_commandInSketch && m_session->def().changesNoDesign) {
    return ArrivalGate::Beside;
  }
  return ArrivalGate::Wait;
}

QString MainWindow::arrivalWaitsFor() const {
  if (m_mode == Mode::PickPlane) {
    const CommandDef* create = m_registry->find(QStringLiteral("sketch.create"));
    return create != nullptr ? create->name : tr("Create Sketch");
  }
  if (m_mode == Mode::Sketch || m_commandInSketch) {
    return tr("the sketch");
  }
  if (m_session) {
    return m_session->def().name;
  }
  return tr("the open command");
}

// ---------------------------------------------------------------------------
// Component from Web's downloads

WebImportResult MainWindow::importWebDownload(const QString& path, WebFileKind kind, QString* note) {
  const QString file = QFileInfo(path).fileName();
  if (kind == WebFileKind::Document) {
    qInfo().noquote() << QStringLiteral("Component from Web: %1 opens as a design of its own").arg(file);
    openPath(path);
    return WebImportResult::Imported;
  }
  if (windowReadOnly()) {
    *note = changeBlockedReason();
    return WebImportResult::Failed;
  }
  const ArrivalGate gate = arrivalGate();
  // Behind those that wait, in the order they came; while the model
  // computes, after it.
  if (gate == ArrivalGate::Wait || !m_arrivals.isEmpty() || modelBusy()) {
    return keepWebArrival(path, gate, note);
  }
  QString error;
  bool inserted = false;
  if (gate == ArrivalGate::Beside) {
    bringInBeside(file, [this, &path, &error, &inserted] { inserted = insertWebDownload(path, &error); });
  } else {
    inserted = insertWebDownload(path, &error);
  }
  if (!inserted) {
    *note = error;
    return WebImportResult::Failed;
  }
  return WebImportResult::Imported;
}

WebImportResult MainWindow::keepWebArrival(const QString& path, ArrivalGate gate, QString* note) {
  const QString file = QFileInfo(path).fileName();
  // A copy of its own: the browser removes the download's folder.
  if (!m_arrivalFolders) {
    m_arrivalFolders = std::make_shared<WebDownloadFolders>();
  }
  const QString folder = m_arrivalFolders->newDownloadFolder();
  const QString kept = folder.isEmpty() ? QString() : QDir(folder).filePath(file);
  if (kept.isEmpty() || !QFile::copy(path, kept)) {
    qWarning().noquote() << QStringLiteral("Component from Web: %1 could not be kept to come in later").arg(file);
    if (!folder.isEmpty()) {
      m_arrivalFolders->remove(folder);
    }
    *note = tr("MitCad could not keep %1 to bring it in later.").arg(file);
    return WebImportResult::Failed;
  }
  Arrival arrival;
  arrival.paths = {kept};
  arrival.folder = folder;
  arrival.web = true;
  arrival.waitedFor = gate == ArrivalGate::Wait ? arrivalWaitsFor() : QString();
  m_arrivals.append(arrival);
  *note = waitingWebNote();
  if (gate == ArrivalGate::Wait) {
    qInfo().noquote() << QStringLiteral("Component from Web: %1 waits until %2 is finished or canceled")
                             .arg(file, arrival.waitedFor);
    return WebImportResult::Waiting;
  }
  qInfo().noquote() << QStringLiteral("Component from Web: %1 comes in after the model and the files before it").arg(file);
  if (modelBusy()) {
    whenIdle(this, [this] { scheduleArrivals(); });
  } else {
    scheduleArrivals();
  }
  return WebImportResult::Waiting;
}

QString MainWindow::waitingWebNote() const {
  QStringList names;
  QString waitsFor;
  for (const Arrival& arrival : m_arrivals) {
    if (arrival.web) {
      names << fileNames(arrival.paths);
      if (!arrival.waitedFor.isEmpty()) {
        waitsFor = arrival.waitedFor;
      }
    }
  }
  if (names.isEmpty()) {
    return QString();
  }
  const QString list = names.join(QStringLiteral(", "));
  if (waitsFor.isEmpty()) {
    return names.size() == 1 ? tr("%1 comes in in a moment.").arg(list) : tr("%1 come in in a moment.").arg(list);
  }
  return names.size() == 1 ? tr("%1 comes in when %2 is finished or canceled.").arg(list, waitsFor)
                           : tr("%1 come in when %2 is finished or canceled.").arg(list, waitsFor);
}

void MainWindow::showWebNote(const QString& note, bool warning) {
  if (m_componentBrowser.isNull()) {
    return;
  }
  // The window's note (ComponentBrowserHost.hpp, interface 4).
  if (!QMetaObject::invokeMethod(m_componentBrowser.data(), "showHostNote", Qt::DirectConnection,
                                 Q_ARG(QString, note), Q_ARG(bool, warning))) {
    qWarning().noquote() << QStringLiteral("Component from Web: the browser's note was not set: %1").arg(note);
  }
}

// ---------------------------------------------------------------------------
// Files dropped on the window

void MainWindow::arriveDropped(const QStringList& paths, std::optional<QPointF> onView) {
  const ArrivalGate gate = arrivalGate();
  // From Idle with nothing waiting, and in a read-only window (whose
  // import says why), as before.
  if (windowReadOnly() || (gate == ArrivalGate::Now && m_arrivals.isEmpty())) {
    openDroppedNow(paths, onView);
    return;
  }
  // A MitCad design opens, and a drawing goes into the sketch, whatever is
  // open (as before); the rest by the gate.
  QStringList now;
  QStringList beside;
  QStringList later;
  for (const QString& path : paths) {
    const FileKind kind = fileKind(path);
    if (kind == FileKind::Design || kind == FileKind::Drawing) {
      now << path;
    } else if (gate == ArrivalGate::Beside && m_arrivals.isEmpty() && insertsBeside({path})) {
      beside << path;
    } else {
      later << path;
    }
  }
  if (!now.isEmpty()) {
    openDroppedNow(now, onView);
  }
  if (!beside.isEmpty()) {
    Arrival arrival;
    arrival.paths = beside;
    arrival.onView = onView;
    bringIn(arrival, ArrivalGate::Beside);
  }
  if (later.isEmpty()) {
    return;
  }
  Arrival arrival;
  arrival.paths = later;
  arrival.onView = onView;
  // What it waits for, as the hint names it: the open command (an
  // inspection too, for what does not come in beside it).
  arrival.waitedFor = gate != ArrivalGate::Now ? arrivalWaitsFor() : QString();
  m_arrivals.append(arrival);
  if (arrival.waitedFor.isEmpty()) {
    scheduleArrivals();
    return;
  }
  const QStringList names = fileNames(later);
  const QString list = names.join(QStringLiteral(", "));
  const QString hint = names.size() == 1
                           ? tr("%1 comes in when %2 is finished or canceled.").arg(list, arrival.waitedFor)
                           : tr("%1 come in when %2 is finished or canceled.").arg(list, arrival.waitedFor);
  qInfo().noquote() << QStringLiteral("Dropped %1 waits until %2 is finished or canceled").arg(list, arrival.waitedFor);
  showHint(hint);
}

// ---------------------------------------------------------------------------
// A design dragged from the Projects panel onto the view (mitcad#2428)

void MainWindow::arriveProjectDesign(const Arrival& given) {
  const ArrivalGate gate = arrivalGate();
  // From Idle with nothing waiting, and in a read-only window (whose
  // Insert Component says why): at once.
  if (windowReadOnly() || (gate == ArrivalGate::Now && m_arrivals.isEmpty() && !modelBusy())) {
    insertDroppedDesign(given.paths.first(), given.onView);
    return;
  }
  // Insert Component asks how to insert it: it waits for an inspection
  // too, as designs of other formats do.
  Arrival arrival = given;
  arrival.waitedFor = gate != ArrivalGate::Now ? arrivalWaitsFor() : QString();
  m_arrivals.append(arrival);
  const QString file = QFileInfo(arrival.paths.first()).fileName();
  if (arrival.waitedFor.isEmpty()) {
    if (modelBusy()) {
      whenIdle(this, [this] { scheduleArrivals(); });
    } else {
      scheduleArrivals();
    }
    return;
  }
  qInfo().noquote() << QStringLiteral("Dropped %1 waits until %2 is finished or canceled").arg(file, arrival.waitedFor);
  showHint(tr("%1 comes in when %2 is finished or canceled.").arg(file, arrival.waitedFor));
}

// ---------------------------------------------------------------------------
// Bringing them in

void MainWindow::bringInBeside(const QString& what, const std::function<void()>& insert) {
  const QString inspection = m_session ? m_session->def().name : QString();
  qInfo().noquote() << QStringLiteral("%1 comes in beside %2, which stays open").arg(what, inspection);
  m_insertingBeside = true;
  {
    const auto done = qScopeGuard([this] { m_insertingBeside = false; });
    insert();
  }
  // Its picks checked again, its result computed again with the new
  // bodies.
  if (m_session) {
    m_session->designChangedBeside(QStringLiteral("the insert of %1").arg(what));
  }
}

void MainWindow::bringIn(const Arrival& arrival, ArrivalGate gate) {
  const QString list = fileNames(arrival.paths).join(QStringLiteral(", "));
  if (!arrival.waitedFor.isEmpty() && gate == ArrivalGate::Now) {
    qInfo().noquote() << QStringLiteral("%1 comes in now that %2 closed").arg(list, arrival.waitedFor);
  }
  if (arrival.web) {
    const QString path = arrival.paths.first();
    QString error;
    bool inserted = false;
    if (gate == ArrivalGate::Beside) {
      bringInBeside(list, [this, &path, &error, &inserted] { inserted = insertWebDownload(path, &error); });
    } else {
      inserted = insertWebDownload(path, &error);
    }
    // The copy goes once it came in (the component keeps its own) or did
    // not.
    if (m_arrivalFolders) {
      m_arrivalFolders->remove(arrival.folder);
    }
    if (!inserted) {
      qInfo().noquote() << QStringLiteral("Component from Web: could not import %1: %2").arg(list, error);
    }
    showWebNote(inserted ? tr("Imported %1.").arg(list) : tr("Could not import %1: %2").arg(list, error), !inserted);
    return;
  }
  if (arrival.project) {
    // Never beside an inspection (insertsBeside: a design waits).
    insertDroppedDesign(arrival.paths.first(), arrival.onView);
    return;
  }
  if (gate == ArrivalGate::Beside) {
    bringInBeside(list, [this, &arrival] {
      leaveStartPage();
      for (const QString& path : arrival.paths) {
        importPath(path, true);
      }
    });
    return;
  }
  openDroppedNow(arrival.paths, arrival.onView);
}

void MainWindow::scheduleArrivals() {
  if (m_arrivalsScheduled || m_arrivals.isEmpty()) {
    return;
  }
  m_arrivalsScheduled = true;
  QTimer::singleShot(0, this, [this] {
    m_arrivalsScheduled = false;
    takeArrivals();
  });
}

void MainWindow::takeArrivals() {
  if (m_arrivalsRunning) {
    return;
  }
  m_arrivalsRunning = true;
  const auto done = qScopeGuard([this] {
    m_arrivalsRunning = false;
    // The kept copies' folder goes when no download waits any more.
    const bool webWaits =
        std::any_of(m_arrivals.cbegin(), m_arrivals.cend(), [](const Arrival& arrival) { return arrival.web; });
    if (!webWaits) {
      m_arrivalFolders.reset();
    }
  });
  while (!m_arrivals.isEmpty()) {
    const ArrivalGate gate = arrivalGate();
    if (gate == ArrivalGate::Wait) {
      return; // the command's end asks again (followArrivals)
    }
    // A drop of files that do not come in beside an inspection waits for
    // it to close.
    if (gate == ArrivalGate::Beside && !m_arrivals.first().web && !insertsBeside(m_arrivals.first().paths)) {
      return;
    }
    if (modelBusy()) {
      whenIdle(this, [this] { scheduleArrivals(); });
      return;
    }
    const Arrival arrival = m_arrivals.takeFirst();
    bringIn(arrival, gate);
  }
}

void MainWindow::followArrivals() {
  const ArrivalGate gate = arrivalGate();
  if (gate == ArrivalGate::Wait) {
    return;
  }
  if (!m_arrivals.isEmpty() && !m_arrivalsRunning &&
      (gate == ArrivalGate::Now || m_arrivals.first().web || insertsBeside(m_arrivals.first().paths))) {
    scheduleArrivals();
  }
  // Move on the last download brought in beside an inspection, now that it
  // closed (after any files still to come in: their imports start it on
  // the last of them).
  if (gate == ArrivalGate::Now && m_webMoveWaits && m_webMovePending) {
    m_webMoveWaits = false;
    QTimer::singleShot(0, this, [this] { startWebDownloadMove(); });
  }
}

void MainWindow::dropArrivals(bool closing) {
  if (m_arrivals.isEmpty()) {
    return;
  }
  const QList<Arrival> dropped = std::exchange(m_arrivals, QList<Arrival>());
  QStringList names;
  bool web = false;
  for (const Arrival& arrival : dropped) {
    names << fileNames(arrival.paths);
    web = web || arrival.web;
  }
  // The kept copies go with them.
  m_arrivalFolders.reset();
  const QString list = names.join(QStringLiteral(", "));
  qInfo().noquote() << QStringLiteral("Waiting files dropped: %1: the design they waited for was %2")
                           .arg(list, closing ? QStringLiteral("closed with its window")
                                              : QStringLiteral("replaced in its window"));
  const QString note = names.size() == 1
                           ? tr("%1 did not come in: the design it waited for was closed.").arg(list)
                           : tr("%1 did not come in: the design they waited for was closed.").arg(list);
  if (web) {
    showWebNote(note, true);
  }
  if (!closing) {
    showStatus(note, NoticeLevel::Warning);
  }
}

} // namespace mitcad
