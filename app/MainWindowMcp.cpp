// SPDX-License-Identifier: MIT
// Live MCP operates on the window's document, through its ordinary command
// workflow. It never owns another model or accesses the worker's document.
// The application's own state and actions (ui_state, ui_select, ui_command,
// ...) are in MainWindowMcpUi.cpp (mitcad#265). document_new is the one
// tool that makes another window's document (mitcad#1266).
#include "MainWindow.hpp"
#include "WindowManager.hpp"
#include "framework/CommandRegistry.hpp"

#include <stdexcept>
#include <utility>

#include <QApplication>
#include <QDebug>
#include <QDir>
#include <QElapsedTimer>
#include <QEventLoop>
#include <QFileInfo>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonValue>
#include <QLabel>
#include <QList>
#include <QPointer>
#include <QScopedValueRollback>
#include <QString>
#include <QStringList>
#include <QThread>
#include <QTimer>

#include "agent/ChatController.hpp"
#include "browser/BrowserController.hpp"
#include "files/LockController.hpp"
#include "files/Projects.hpp"
#include "framework/AppSettings.hpp"
#include "framework/Json.hpp"
#include "mcp/AgentFolders.hpp"
#include "mcp/AgentIndicator.hpp"
#include "mcp/AssistantSetup.hpp"
#include "mcp/EndpointLimits.hpp"
#include "mcp/LiveErrors.hpp"
#include "mcp/ModalGuard.hpp"
#include "sketch/Sketch3dController.hpp"
#ifdef MITCAD_MACOS_NATIVE_UI
#include "platform/NativeDialogs.hpp"
#include "platform/NativeMacWindow.hpp"
#endif

namespace mitcad {
namespace {

// What an agent's change says in the undo text and the history (mitcad#265).
const QString kAgentSuffix = QStringLiteral(" (agent)");

[[noreturn]] void refuse(const QString& reason) {
  // A refusal can repeat a client's text: its log line keeps the start
  // (mitcad#935).
  qInfo().noquote() << QStringLiteral("MCP refused: %1").arg(mcp::echoed(reason, mcp::kRefusalLogChars));
  throw std::runtime_error(reason.toStdString());
}

// A refusal with fields of its own beside the error (mitcad#689: which
// entry of a command array).
[[noreturn]] void refuseWith(const QString& reason, const QJsonObject& fields) {
  qInfo().noquote() << QStringLiteral("MCP refused: %1").arg(mcp::echoed(reason, mcp::kRefusalLogChars));
  throw mcp::Refusal(reason, fields);
}

} // namespace

void MainWindow::mcpRefuseBusy() const {
  // Only computation refuses everything, queries too (mitcad#265): the
  // agent is told when to retry.
  int retry = 0;
  QString reason;
  if (m_mcpRequestActive) {
    reason = QStringLiteral("Another external request is running");
    retry = 500;
  } else if (modelBusy()) {
    reason = QStringLiteral("The design is computing");
    retry = 1000;
  } else if (m_viewer->isPainting()) {
    reason = QStringLiteral("The view is drawing");
    retry = 100;
  } else if (!m_import.isNull()) {
    reason = QStringLiteral("An import is running");
    retry = 2000;
  }
  if (retry > 0) {
    const QString message = QStringLiteral("busy: %1; retry in %2 ms").arg(reason).arg(retry);
    qInfo().noquote() << QStringLiteral("MCP refused: %1").arg(message);
    throw mcp::Refusal(message, {{QStringLiteral("busy"), true}, {QStringLiteral("retry_after_ms"), retry}});
  }
}

QString MainWindow::mcpBlockedReason(bool editing) const {
  if (m_mcpRequestActive) {
    return QStringLiteral("Another external design request is running.");
  }
  if (modelBusy() || m_viewer->isPainting()) {
    return QStringLiteral("The design is busy; retry when computation or drawing has finished.");
  }
  // A sketch being edited takes its own commands (mcpSketchCommand); a
  // command panel or a plane pick takes none (mitcad#265).
  if (m_mode == Mode::Command || m_session || m_panelScroll) {
    const QString name = m_session ? m_session->def().name : QStringLiteral("a command");
    return QStringLiteral("The command panel of %1 is open: finish it (ui_command with finish ok or cancel) before changing the "
              "design; queries and the ui tools work meanwhile.")
        .arg(name);
  }
  if (m_mode == Mode::PickPlane) {
    return QStringLiteral("Create Sketch waits for a plane to be picked: pick one or cancel it first.");
  }
  if (m_drag || (m_controller && m_controller->markerDragActive())) {
    return QStringLiteral("Finish dragging the design before accessing it externally.");
  }
  if (m_animation) {
    // It plays until stopped (mitcad#1790).
    return QStringLiteral("A joint's animation plays until it is stopped: ui_command assemble.animate_joint again "
                          "stops it (Esc in the application).");
  }
  if (!m_import.isNull()) {
    return QStringLiteral("An import is running; retry when it has finished.");
  }
  const QString dialog = mcpDialogReason();
  if (!dialog.isEmpty()) {
    return dialog;
  }
#ifdef MITCAD_MACOS_NATIVE_UI
  if (m_nativeForm || m_nativeClosingCommand || nativeDialogModalActive() ||
      (m_nativeWindow && m_nativeWindow->modalBlocked())) {
    return QStringLiteral("The design is waiting for a native command or dialog.");
  }
#endif
  if (editing && windowReadOnly()) {
    return readOnlyReason();
  }
  return QString();
}

QString MainWindow::mcpDialogReason() const {
  if (m_closePending) {
    return QStringLiteral("The design is closing.");
  }
  // An agent cannot answer a dialog that is open (mitcad#1725): the
  // refusal names it, and the recovery question at start-up says how to
  // start without it.
  return mcp::dialogRefusal();
}

QJsonObject MainWindow::mcpDocumentInfo() const {
  Q_ASSERT(QThread::currentThread() == thread());
  const DocumentSession::Snapshot snapshot = m_documentSession->snapshot();
  const QString blocked = mcpBlockedReason(true);
  QJsonObject info{{QStringLiteral("document"), m_mcpDocumentId},
                   {QStringLiteral("path"), snapshot.filePath.isEmpty() ? QJsonValue(QJsonValue::Null)
                                                                       : QJsonValue(snapshot.filePath)},
                   {QStringLiteral("name"), snapshot.name},
                   {QStringLiteral("state"), snapshot.document},
                   {QStringLiteral("busy"), snapshot.busy},
                   {QStringLiteral("editable"), blocked.isEmpty()}};
  if (!blocked.isEmpty()) {
    info.insert(QStringLiteral("blocked_reason"), blocked);
#ifdef MITCAD_MACOS_NATIVE_UI
    QJsonArray native;
    if (m_nativeForm) native.append(QStringLiteral("command_form"));
    if (m_nativeClosingCommand) native.append(QStringLiteral("closing_command"));
    if (nativeDialogModalActive()) native.append(QStringLiteral("modal_dialog"));
    if (m_nativeWindow && m_nativeWindow->modalBlocked()) native.append(QStringLiteral("window_modal"));
    if (!native.isEmpty()) {
      info.insert(QStringLiteral("native_blockers"), native);
      info.insert(QStringLiteral("native_modal_state"),
                  nativeDialogModalState(m_nativeWindow ? m_nativeWindow->nativeWindow() : nullptr));
    }
#endif
  }
  return info;
}

QJsonObject MainWindow::mcpQuery(const QJsonObject& request) const {
  Q_ASSERT(QThread::currentThread() == thread());
  // Read-only: also with a sketch or command open, during drags and
  // dialogs; refused only while the model computes (mitcad#265).
  mcpRefuseBusy();
  // The shared MCP validator excludes disk reads and external workflows.
  validate_mcp_query(rustStr(compactJson(request)));
  return {{QStringLiteral("document"), m_mcpDocumentId}, {QStringLiteral("result"), query(request)}};
}

QJsonObject MainWindow::mcpCommand(const QJsonObject& arguments) {
  Q_ASSERT(QThread::currentThread() == thread());
  mcpRefuseBusy();
  const QString refusal = mcpBlockedReason(true);
  if (!refusal.isEmpty()) {
    refuse(refusal);
  }
  // One command, or an array run in order as one undo step (mitcad#689).
  const QJsonValue given = arguments.value(QStringLiteral("command"));
  const bool array = given.isArray();
  QList<QJsonObject> entries;
  if (array) {
    for (const QJsonValue& entry : given.toArray()) {
      entries.append(entry.toObject());
    }
  } else {
    entries.append(given.toObject());
  }
  if (entries.isEmpty()) {
    refuse(QStringLiteral("The command array is empty."));
  }
  const auto entryName = [array](qsizetype i) {
    return array ? QStringLiteral("commands[%1]: ").arg(i) : QString();
  };
  const auto isStep = [](const QJsonObject& entry) {
    const QString kind = entry.value(QStringLiteral("cmd")).toString();
    return kind == QStringLiteral("undo") || kind == QStringLiteral("redo");
  };
  for (qsizetype i = 0; i < entries.size(); ++i) {
    if (entries.size() > 1 && isStep(entries[i])) {
      refuse(QStringLiteral("%1undo and redo run alone, not in a command array.").arg(entryName(i)));
    }
    // By class: file IO, remote and internal checkpoint commands stay
    // unavailable (mitcad#265, core/model-api/src/classes.rs).
    try {
      idleDocument().validate_mcp_document_command(rustStr(compactJson(entries[i])));
    } catch (const std::exception& error) {
      if (!array) {
        refuse(agentErrorText(error));
      }
      // Nothing of the array has run.
      refuseWith(entryName(i) + agentErrorText(error),
                 {{QStringLiteral("entry"), static_cast<qint64>(i)},
                  {QStringLiteral("applied"), false},
                  {QStringLiteral("undone"), 0}});
    }
  }
  if (arguments.contains(QStringLiteral("hints")) && !arguments.value(QStringLiteral("hints")).isBool()) {
    refuse(QStringLiteral("hints must be a boolean"));
  }
  const bool hints = arguments.value(QStringLiteral("hints")).toBool(true);
  const QString mode = arguments.value(QStringLiteral("timeline")).toString(QStringLiteral("changed"));
  const QScopedValueRollback<bool> active(m_mcpRequestActive, true);
  // What the result compares (mitcad#689): the document before, when only
  // the changes are answered.
  const QByteArray before = mode == QStringLiteral("changed") ? compactJson(mcpSnapshot())
      : hints ? compactJson(QJsonObject{{QStringLiteral("timeline"), query({{QStringLiteral("query"), QStringLiteral("timeline")}})}})
              : QByteArray();
  const bool inSketch = m_mode == Mode::Sketch;
  const QString sketchUid = inSketch ? editingSketchUid() : QString();
  const int depth = undoDepth();
  QJsonArray results;
  QJsonArray times;
  qsizetype stopped = -1;
  for (qsizetype i = 0; i < entries.size(); ++i) {
    QJsonObject answer;
    QString reason;
    const bool ran = inSketch ? mcpSketchStep(entries[i], &answer, reason) : mcpModelStep(entries[i], &answer, reason);
    if (!ran) {
      if (!array) {
        refuse(reason);
      }
      // Nothing of the array stays: the entries before it are undone.
      const int undone = mcpUndoTo(depth);
      refuseWith(QStringLiteral("%1%2; the %3 step(s) of the entries before it were undone").arg(entryName(i), reason).arg(undone),
                 {{QStringLiteral("entry"), static_cast<qint64>(i)},
                  {QStringLiteral("applied"), false},
                  {QStringLiteral("undone"), undone}});
    }
    const QByteArray answerText = compactJson(answer);
    if (mcp_recomputed(rustStr(answerText))) {
      times.append(query({{QStringLiteral("query"), QStringLiteral("recompute_times")}}));
    }
    results.append(answer);
    // An entry whose added or edited feature fails stops the array.
    if (array && i + 1 < entries.size() && mcp_entry_failed(rustStr(answerText))) {
      stopped = i;
      break;
    }
  }
  if (!(entries.size() == 1 && isStep(entries.first()))) {
    if (inSketch) {
      m_mcpSketchEdited = true;
    }
    // The agent's steps as one, labelled (agent).
    mcpLabelAgentChange(depth);
  }
  const QByteArray timesText = QJsonDocument(times).toJson(QJsonDocument::Compact);
  const QByteArray after = compactJson(mcpSnapshot());
  QJsonObject result = parseObject(mcp_command_document(rustStr(mode.toUtf8()), rustStr(before),
                                                        rustStr(after), rustStr(timesText)));
  result.insert(QStringLiteral("document"), m_mcpDocumentId);
  result.insert(QStringLiteral("applied"), true);
  result.insert(QStringLiteral("result"), results.last());
  if (hints) {
    result.insert(QStringLiteral("hints"), QJsonArray());
    QJsonArray commands;
    for (const QJsonObject& entry : std::as_const(entries)) {
      commands.append(entry);
    }
    try {
      const QByteArray commandText = QJsonDocument(commands).toJson(QJsonDocument::Compact);
      const QByteArray resultText = QJsonDocument(results).toJson(QJsonDocument::Compact);
      const rust::String request = mcp_command_hint_query(rustStr(commandText), rustStr(resultText),
                                                         rustStr(before), rustStr(after));
      if (!request.empty()) {
        const QByteArray review = compactJson(query(parseObject(request)).toObject());
        result.insert(QStringLiteral("hints"), parseJson(mcp_command_hints(rustStr(review))).array());
      }
    } catch (const std::exception& failure) {
      // Optional guidance does not change the outcome of the applied edits.
      qWarning().noquote() << QStringLiteral("MCP modeling hints unavailable: %1").arg(agentErrorText(failure));
    }
  }
  if (inSketch) {
    result.insert(QStringLiteral("sketch"), sketchUid);
  }
  if (array) {
    result.insert(QStringLiteral("ran"), static_cast<qint64>(results.size()));
    result.insert(QStringLiteral("results"), results);
    if (stopped >= 0) {
      result.insert(QStringLiteral("stopped_at"), static_cast<qint64>(stopped));
    }
  }
  // Which edges of a failed fillet or chamfer fail alone, tried in
  // previews (mitcad#1587); the answer as it was when that cannot be said.
  if (!inSketch) {
    try {
      result = parseObject(m_document->mcp_diagnose_dressups(rustStr(compactJson(result))));
    } catch (const std::exception& error) {
      qWarning().noquote() << QStringLiteral("MCP: the failed fillet's edges were not tried one at a time: %1")
                                  .arg(agentErrorText(error));
    }
  }
  QStringList kinds;
  for (const QJsonObject& entry : std::as_const(entries)) {
    kinds << entry.value(QStringLiteral("cmd")).toString();
  }
  qInfo().noquote() << (inSketch ? QStringLiteral("MCP: an agent ran %1 in sketch %2").arg(kinds.join(QStringLiteral(", ")), sketchUid)
                                 : QStringLiteral("MCP: an agent ran %1").arg(kinds.join(QStringLiteral(", "))));
  return result;
}

bool MainWindow::mcpModelStep(const QJsonObject& request, QJsonObject* answer, QString& reason) {
  // An agent's selections by geometry (a profile by a point, a fillet's
  // edges by a filter) become names on the document as the entries before
  // left it (mitcad#1587); the stored definition keeps the names.
  QJsonObject command;
  try {
    command = parseObject(idleDocument().mcp_resolve_command(rustStr(compactJson(request))));
  } catch (const std::exception& error) {
    reason = agentErrorText(error);
    return false;
  }
  if (!runModelCommand(command, answer)) {
    reason = lastError().isEmpty() ? QStringLiteral("The model command was refused.") : lastError();
    return false;
  }
  return true;
}

bool MainWindow::mcpSketchStep(const QJsonObject& request, QJsonObject* answer, QString& reason) {
  // The sketch being edited: its own commands go through the sketch
  // session, as the user's would (mitcad#265): one undo step, the view,
  // the palette and the sketch's undo follow.
  const QString kind = request.value(QStringLiteral("cmd")).toString();
  const QString uid = m_sketch->uid();
  const QString sketchName = m_sketch->model().name;
  if (kind == QStringLiteral("undo") || kind == QStringLiteral("redo")) {
    const QString label =
        queryObject({{QStringLiteral("query"), QStringLiteral("document")}}).value(kind).toString();
    if (label.isEmpty()) {
      reason = QStringLiteral("Nothing to %1.").arg(kind);
      return false;
    }
    if (kind == QStringLiteral("undo")) {
      undo();
    } else {
      redo();
    }
    *answer = {{QStringLiteral("label"), label}};
    return true;
  }
  if (m_sketch3d != nullptr && m_sketch3d->isActive()) {
    return mcpSketch3dStep(request, answer, reason); // a 3D sketch's own commands (mitcad#1524)
  }
  // The open sketch turned into a 3D sketch (mitcad#1544), as its palette's
  // 3D Sketch box does: the 3D editor takes it.
  if (kind == QStringLiteral("sketch3d.convert") && request.value(QStringLiteral("sketch")).toString() == uid) {
    const int depth = undoDepth();
    convertOpenSketchTo3d();
    if (undoDepth() == depth) {
      reason = lastError().isEmpty() ? QStringLiteral("The conversion was refused.") : lastError();
      return false;
    }
    m_mcpSketchEdited = true;
    *answer = queryObject({{QStringLiteral("query"), QStringLiteral("sketch3d")}, {QStringLiteral("uid"), uid}});
    return true;
  }
  const bool own =
      (kind.startsWith(QStringLiteral("sketch.")) && request.value(QStringLiteral("sketch")).toString() == uid) ||
      (kind == QStringLiteral("edit_feature") && request.value(QStringLiteral("uid")).toString() == uid);
  if (!own) {
    reason = QStringLiteral("%1 (%2) is being edited: only its own sketch commands (with \"sketch\": \"%2\"), edit_feature of "
                "it, undo and redo run now. Finish it (ui_command with id sketch.finish) for other commands.")
                 .arg(sketchName, uid);
    return false;
  }
  sketch::SketchOp op(*m_sketch);
  if (!op.run(request, answer)) {
    const QString error = op.error();
    op.rollback();
    reason = error.isEmpty() ? QStringLiteral("The sketch command was refused.") : error;
    return false;
  }
  op.commit();
  return true;
}

int MainWindow::mcpUndoTo(int depth) {
  // An array's entries taken back after a later one was refused
  // (mitcad#689), as Undo would, in a sketch too.
  int undone = 0;
  for (int now = undoDepth(); now > depth; now = undoDepth()) {
    undo();
    if (undoDepth() >= now) {
      break; // refused
    }
    ++undone;
  }
  return undone;
}

QJsonObject MainWindow::mcpSnapshot() const {
  // The queries a command's result compares (mitcad#689, mcp_snapshot_queries).
  QJsonObject snapshot;
  const QJsonObject queries = parseObject(mcp_snapshot_queries());
  for (auto it = queries.constBegin(); it != queries.constEnd(); ++it) {
    snapshot.insert(it.key(), query(it.value().toObject()));
  }
  return snapshot;
}

QJsonObject MainWindow::mcpExport(const QJsonObject& arguments) {
  mcpRefuseBusy();
  // Into a folder for designs, a new file, as document_save's
  // path (mitcad#689); the document does not change.
  if (m_session || m_mode == Mode::Command) {
    refuse(QStringLiteral("Finish the current command before exporting."));
  }
  if (autosaveMarker() >= 0) {
    // Editing a sketch in the middle of the timeline rolls the later
    // features back until it ends (mitcad#750): the bodies at the marker
    // are not the design's.
    refuse(QStringLiteral("Finish the sketch before exporting: editing it rolls the later features back."));
  }
  if (const QString waiting = mcpDialogReason(); !waiting.isEmpty()) {
    refuse(waiting);
  }
  // An existing file only with overwrite (mitcad#1569), as document_save's.
  const bool overwrite = arguments.value(QStringLiteral("overwrite")).toBool(false);
  QString reason;
  const QString path = mcp::agentOutputPath(arguments.value(QStringLiteral("path")).toString(), overwrite, reason);
  if (path.isEmpty()) {
    refuse(reason);
  }
  const QString title = m_filePath.isEmpty() ? QStringLiteral("Untitled") : QFileInfo(m_filePath).completeBaseName();
  const QString folder = m_filePath.isEmpty() ? QString() : QFileInfo(m_filePath).absolutePath();
  // The command and the checks are the headless server's (core/ffi/src/mcp.rs).
  QJsonObject plan;
  try {
    plan = parseObject(idleDocument().mcp_export_plan(rustStr(compactJson(arguments)), rustStr(path.toUtf8()),
                                                      rustStr(title.toUtf8()), rustStr(folder.toUtf8())));
  } catch (const std::exception& error) {
    refuse(agentErrorText(error));
  }
  // The other files it writes (an OBJ's .mtl) by the same rule.
  for (const QJsonValue& output : plan.value(QStringLiteral("outputs")).toArray()) {
    if (mcp::agentOutputPath(output.toString(), overwrite, reason).isEmpty()) {
      refuse(reason);
    }
  }
  const QJsonObject command = plan.value(QStringLiteral("command")).toObject();
  const QScopedValueRollback<bool> active(m_mcpRequestActive, true);
  QJsonObject result;
  if (plan.value(QStringLiteral("drawing")).toBool()) {
    // A drawing checks each sheet's file before it writes any.
    try {
      result = parseObject(idleDocument().mcp_export_drawing(rustStr(compactJson(command)), overwrite));
    } catch (const std::exception& error) {
      refuse(agentErrorText(error));
    }
  } else if (!runCommand(command, &result)) {
    refuse(lastError().isEmpty() ? QStringLiteral("The export failed.") : lastError());
  }
  qInfo().noquote() << QStringLiteral("MCP: an agent exported %1 (%2)")
                           .arg(path, arguments.value(QStringLiteral("format")).toString());
  return {{QStringLiteral("document"), m_mcpDocumentId},
          {QStringLiteral("path"), path},
          {QStringLiteral("exported"), true},
          {QStringLiteral("result"), result}};
}

QJsonObject MainWindow::mcpRender(const QJsonObject& arguments) const {
  Q_ASSERT(QThread::currentThread() == thread());
  // A read of the design at the marker (mitcad#1487), as model_query: also
  // with a sketch or command open; the window's view plays no part.
  mcpRefuseBusy();
  QString path;
  const QString given = arguments.value(QStringLiteral("path")).toString();
  if (!given.isEmpty()) {
    QString reason;
    path = mcp::agentOutputPath(given, arguments.value(QStringLiteral("overwrite")).toBool(false), reason,
                                {QStringLiteral("png")});
    if (path.isEmpty()) {
      refuse(reason);
    }
  }
  // The headless server's picture (core/ffi/src/mcp/render.rs).
  QJsonObject result;
  try {
    result = parseObject(idleDocument().mcp_render(rustStr(compactJson(arguments)), rustStr(m_mcpDocumentId.toUtf8()),
                                                   rustStr(path.toUtf8())));
  } catch (const std::exception& error) {
    refuse(agentErrorText(error));
  }
  if (result.contains(QStringLiteral("error"))) {
    const QString error = result.take(QStringLiteral("error")).toString();
    refuseWith(error, result);
  }
  if (!path.isEmpty()) {
    qInfo().noquote() << QStringLiteral("MCP: an agent wrote a picture of the design to %1").arg(path);
  }
  return result;
}

void MainWindow::mcpLabelAgentChange(int depthBefore) {
  // The steps an agent added become one, labelled "... (agent)" in the
  // undo text and the version history (mitcad#265); they undo as one.
  try {
    const QJsonObject document = queryObject({{QStringLiteral("query"), QStringLiteral("document")}});
    const int depth = document.value(QStringLiteral("undo_depth")).toInt();
    const QString label = document.value(QStringLiteral("undo")).toString();
    if (depth <= depthBefore || label.isEmpty() || label.endsWith(kAgentSuffix)) {
      return;
    }
    if (runCommand({{QStringLiteral("cmd"), QStringLiteral("merge_undo")},
                    {QStringLiteral("depth"), depthBefore},
                    {QStringLiteral("label"), label + kAgentSuffix}})) {
      qInfo().noquote() << QStringLiteral("Agent change: %1").arg(label + kAgentSuffix);
      updateActions();
    }
  } catch (const std::exception&) {
    // The label stays as the command left it.
  }
}

QJsonObject MainWindow::mcpSave(const QJsonObject& arguments) {
  mcpRefuseBusy();
  // As File > Save does (mitcad#265): to the document's own path, with the
  // project's version and commit message; with a path as Save As does, into
  // a folder for designs (mitcad#402).
  if (m_session || m_mode == Mode::Command) {
    refuse(QStringLiteral("Finish the current command before saving."));
  }
  if (const QString waiting = mcpDialogReason(); !waiting.isEmpty()) {
    refuse(waiting);
  }
  if (windowReadOnly()) {
    refuse(readOnlyReason());
  }
  QString target;
  if (arguments.contains(QStringLiteral("path"))) {
    QString reason;
    target = mcp::agentDesignPath(arguments.value(QStringLiteral("path")).toString(), true,
                                  arguments.value(QStringLiteral("overwrite")).toBool(false), reason);
    if (target.isEmpty()) {
      refuse(reason);
    }
    // A file another window has open is not written over (mitcad#1323):
    // that window's save would overwrite this one's, or this one's its.
    MainWindow* other = m_windowManager != nullptr ? m_windowManager->findByPath(target) : nullptr;
    if (other != nullptr && other != this) {
      refuseWith(QStringLiteral("%1 is open in another window as the document %2: save to another path, or work on that "
                    "document.")
                     .arg(QDir::toNativeSeparators(target), other->mcpDocumentId()),
                 {{QStringLiteral("open_document"), other->mcpDocumentId()}});
    }
  } else if (m_filePath.isEmpty()) {
    refuse(QStringLiteral("%1 has never been saved: it has no path to save to. Give a path in a folder for designs "
              "(document_list lists them), or save it in the application (File › Save As).")
               .arg(documentName()));
  }
  const QScopedValueRollback<bool> active(m_mcpRequestActive, true);
  bool saved = false;
  QString dialog;
  {
    mcp::ModalGuard guard;
    saved = target.isEmpty() ? save() : writeFile(target);
    dialog = guard.opened();
  }
  if (!dialog.isEmpty()) {
    refuse(QStringLiteral("Saving asked a question (%1), which an agent cannot answer: save in the application.").arg(dialog));
  }
  if (!saved || isModified()) {
    refuse(m_statusLabel->text().isEmpty() ? QStringLiteral("The design was not saved.") : m_statusLabel->text());
  }
  qInfo().noquote() << QStringLiteral("MCP: an agent saved %1").arg(m_filePath);
  return {{QStringLiteral("document"), m_mcpDocumentId},
          {QStringLiteral("path"), m_filePath},
          {QStringLiteral("saved"), true},
          {QStringLiteral("status"), m_statusLabel->text()},
          {QStringLiteral("state"), query({{QStringLiteral("query"), QStringLiteral("document")}})}};
}

QJsonObject MainWindow::mcpOpen(const QJsonObject& arguments) {
  mcpRefuseBusy();
  // As File > Open does, without its dialogs (mitcad#402): a design from a
  // folder for designs; the window's document is replaced and
  // with it the handle.
  QString reason;
  const QString path = mcp::agentDesignPath(arguments.value(QStringLiteral("path")).toString(), false, false, reason);
  if (path.isEmpty()) {
    refuse(reason);
  }
  // A design another window has open is worked on there, as File › Open
  // presents that window (mitcad#1323): a second copy's save would
  // overwrite the other's.
  MainWindow* other = m_windowManager != nullptr ? m_windowManager->findByPath(path) : nullptr;
  if (other != nullptr && other != this) {
    refuseWith(QStringLiteral("%1 is open in another window as the document %2: work on that document.")
                   .arg(QDir::toNativeSeparators(path), other->mcpDocumentId()),
               {{QStringLiteral("open_document"), other->mcpDocumentId()}});
  }
  if (!m_session && m_mode == Mode::PickPlane) {
    refuse(QStringLiteral("Create Sketch waits for a plane to be picked: pick one or cancel it first."));
  }
  if (m_session || m_mode != Mode::Idle) {
    refuse(QStringLiteral("Finish the sketch or command first (ui_command with finish ok or cancel)."));
  }
  // What else keeps a model command from running: drags, imports, dialogs,
  // a native command form.
  const QString blocked = mcpBlockedReason(false);
  if (!blocked.isEmpty()) {
    refuse(blocked);
  }
  const bool discard = arguments.value(QStringLiteral("discard")).toBool(false);
  if (!discard && (windowReadOnly() ? m_locks->keptChanges() : isModified())) {
    refuse(QStringLiteral("%1 has unsaved changes: save them first (document_save), or pass discard: true to lose them.")
               .arg(documentName()));
  }
  const QString before = m_mcpDocumentId;
  const QScopedValueRollback<bool> active(m_mcpRequestActive, true);
  QString error;
  bool opened = false;
  QString dialog;
  {
    mcp::ModalGuard guard;
    opened = openFile(path, error);
    dialog = guard.opened();
  }
  if (!dialog.isEmpty()) {
    refuse(QStringLiteral("Opening asked a question (%1), which an agent cannot answer: open it in the application.").arg(dialog));
  }
  if (!opened || m_mcpDocumentId == before) {
    refuse(error.isEmpty() ? QStringLiteral("%1 was not opened.").arg(QDir::toNativeSeparators(path))
                           : QStringLiteral("Could not open %1: %2").arg(QDir::toNativeSeparators(path), error));
  }
  qInfo().noquote() << QStringLiteral("MCP: an agent opened %1").arg(m_filePath);
  QJsonObject result = mcpDocumentInfo();
  result.insert(QStringLiteral("opened"), true);
  result.insert(QStringLiteral("replaced"), before);
  return result;
}

QJsonObject MainWindow::mcpNew(const QJsonObject& arguments) {
  mcpRefuseBusy();
  // As File > New Design makes it (mitcad#1266), without its naming row or
  // dialog: in a window of its own beside this one, never in place of a
  // design, so every open design and its handle stay as they are.
  if (m_windowManager == nullptr) {
    refuse(QStringLiteral("This window cannot open another design beside it."));
  }
  if (const QString waiting = mcpDialogReason(); !waiting.isEmpty()) {
    refuse(waiting);
  }
#ifdef MITCAD_MACOS_NATIVE_UI
  if (nativeDialogModalActive()) {
    refuse(QStringLiteral("The design is waiting for a native dialog."));
  }
#endif
  // Where: the folder given; else the current project's top folder when the
  // user allows agents to use it, as New Design's; else, for a name, the
  // first folder for designs; else an untitled design.
  const QString name = arguments.value(QStringLiteral("name")).toString().trimmed();
  QString reason;
  QString folder;
  QString note;
  if (arguments.contains(QStringLiteral("folder"))) {
    folder = mcp::agentDesignFolder(arguments.value(QStringLiteral("folder")).toString(), reason);
    if (folder.isEmpty()) {
      refuse(reason);
    }
  } else {
    if (m_project.hasHistory()) {
      folder = mcp::agentDesignFolder(m_project.root, reason);
      if (folder.isEmpty()) {
        note = QStringLiteral("The current project's folder is not one an agent may use (%1)").arg(reason);
      }
    }
    if (folder.isEmpty() && !name.isEmpty()) {
      folder = mcp::agentDesignFolder(mcp::agentFolders().value(0), reason);
      if (folder.isEmpty()) {
        refuse(reason);
      }
    }
  }
  QString file;
  if (!folder.isEmpty()) {
    file = mcp::agentNewDesignPath(folder, name, reason);
    if (file.isEmpty()) {
      refuse(reason);
    }
  } else if (!note.isEmpty()) {
    note += QStringLiteral(": the new design is untitled; save it with document_save and a path.");
  }
  // In a project with versions it is made and recorded as New Design does
  // (the project's new_design); elsewhere the empty design is saved there.
  const ProjectState project = file.isEmpty() ? ProjectState() : projectOfFile(file);
  QString root = project.hasHistory() ? QFileInfo(project.root).canonicalFilePath() : QString();
  if (root.isEmpty()) {
    root = project.root;
  }
  const QString relative = project.hasHistory() ? QDir(root).relativeFilePath(file) : QString();
  // Outside it: a parent reference (Qt's separator is "/"), not a name that
  // starts with two dots, such as "..draft.mitcad" (mitcad#1317).
  const bool outside = relative == QStringLiteral("..") || relative.startsWith(QStringLiteral("../")) ||
                       QDir::isAbsolutePath(relative);
  if (project.hasHistory() && (relative.isEmpty() || outside)) {
    refuse(QStringLiteral("%1 is not inside its project %2.").arg(QDir::toNativeSeparators(file), QDir::toNativeSeparators(root)));
  }
  const QString before = m_mcpDocumentId;
  const QScopedValueRollback<bool> active(m_mcpRequestActive, true);
  QPointer<MainWindow> target;
  QString error;
  QString dialog;
  bool made = true;
  {
    mcp::ModalGuard guard;
    if (project.hasHistory()) {
      QJsonObject refused;
      const std::optional<QJsonObject> answer =
          projectWrite(root,
                       {{QStringLiteral("cmd"), QStringLiteral("new_design")},
                        {QStringLiteral("path"), relative},
                        {QStringLiteral("units"), GeneralSettings::load().newDesignUnits}},
                       tr("New Design"), &refused);
      refreshProjectsPanel();
      if (!answer) {
        made = false;
        error = errorMessageOf(refused);
      }
    }
    if (made && guard.opened().isEmpty()) {
      target = m_windowManager->createWindow(false, this, false);
      if (!file.isEmpty()) {
        if (project.hasHistory()) {
          // With its Component1 active, as the file stores it (mitcad#844).
          target->m_openingNewDesign = true;
          made = target->openFile(file, error);
          if (target) {
            target->m_openingNewDesign = false;
          }
        } else {
          // The window's empty design, in the default units for new designs
          // (mitcad#1036), saved as the file.
          made = target->writeFile(file);
        }
        made = made && !target.isNull();
      }
    }
    dialog = guard.opened();
  }
  if (!made || !dialog.isEmpty()) {
    if (target) {
      m_windowManager->windowClosed(target.data());
    }
    if (!dialog.isEmpty()) {
      refuse(QStringLiteral("Making the design asked a question (%1), which an agent cannot answer: make it in the application "
                "(File › New Design); ui_log has the details.")
                 .arg(dialog));
    }
    refuse(error.isEmpty() ? QStringLiteral("%1 was not made.").arg(QDir::toNativeSeparators(file))
                           : QStringLiteral("Could not make %1: %2").arg(QDir::toNativeSeparators(file), error));
  }
  m_windowManager->present(target.data());
  // Ready for the agent's first command: the open's computation ends here
  // (at most a minute; document_list's editable tells the rest).
  QElapsedTimer clock;
  clock.start();
  while (target && target->modelBusy() && clock.elapsed() < 60000) {
    QEventLoop loop;
    QTimer::singleShot(10, &loop, &QEventLoop::quit);
    loop.exec();
  }
  if (!target) {
    refuse(QStringLiteral("The new design's window closed."));
  }
  if (project.hasHistory()) {
    target->showHint(tr("%1 is a new design of %2: every save records a version.")
                         .arg(QFileInfo(file).fileName(), QFileInfo(root).fileName()));
  }
  qInfo().noquote() << QStringLiteral("MCP: an agent made a new design %1 in %2 (beside %3)")
                           .arg(file.isEmpty() ? QStringLiteral("(untitled)") : file,
                                m_windowManager->handle(target.data()), m_windowManager->handle(this));
  QJsonObject result = target->mcpDocumentInfo();
  result.insert(QStringLiteral("created"), true);
  // The design the call came from: still open, its handle still valid.
  result.insert(QStringLiteral("previous"), before);
  result.insert(QStringLiteral("recorded"), project.hasHistory());
  if (!note.isEmpty()) {
    result.insert(QStringLiteral("note"), note);
  }
  return result;
}

void MainWindow::setMcpAgents(const QList<mcp::ConnectedAgent>& agents,
                              std::function<void(const QStringList& ids)> disconnect) {
  m_mcpDisconnect = std::move(disconnect);
  if (agents == m_agentIndicator->agents()) {
    return;
  }
  const bool before = m_mcpAgents > 0;
  // ui_state's agent_clients keeps the clients' own labels; people see the
  // assistants' names.
  QStringList names;
  QStringList shown;
  for (const mcp::ConnectedAgent& agent : agents) {
    names << agent.name;
    if (const QString name = mcp::assistantDisplayName(agent.name); !shown.contains(name)) {
      shown << name;
    }
  }
  m_mcpAgentNames = names;
  m_mcpAgents = int(agents.size());
  m_agentIndicator->setAgents(agents);
  if (m_chat != nullptr) {
    // The chat panel points to Claude Desktop while it is connected
    // (mitcad#1297).
    m_chat->assistantsChanged();
  }
  if (m_mcpAgents > 0 && !before) {
    qInfo().noquote() << QStringLiteral("Agent connected");
    showHint(tr("%1 connected.").arg(shown.join(QStringLiteral(", "))));
  } else if (m_mcpAgents == 0 && before) {
    qInfo().noquote() << QStringLiteral("Agent disconnected");
    showHint(tr("The AI assistant disconnected."));
  }
}

void MainWindow::setMcpListener(const QString& instance, int port) {
  m_agentIndicator->setListener(instance, port);
}

void MainWindow::setMcpUnavailable(const QString& reason) {
  m_agentIndicator->setUnavailable(reason);
}

QJsonObject MainWindow::mcpTool(const QString& name, const QJsonObject& arguments) {
  Q_ASSERT(QThread::currentThread() == thread());
  if (name == QStringLiteral("document_list")) {
    // With the folders document_save's path and document_open may use
    // (mitcad#402).
    QJsonArray folders;
    for (const QString& folder : mcp::agentFolders()) {
      folders.append(QDir::toNativeSeparators(folder));
    }
    QJsonArray documents;
    const auto windows = windowManager() ? windowManager()->windows() : QList<MainWindow*>{this};
    for (MainWindow* window : windows) {
      auto info = window->mcpDocumentInfo();
      info.insert(QStringLiteral("active"), windowManager() ? window == windowManager()->activeWindow() : true);
      documents.append(info);
    }
    return {{QStringLiteral("documents"), documents}, {QStringLiteral("folders"), folders}};
  }
  const QString document = arguments.value(QStringLiteral("document")).toString();
  if (!document.isEmpty() && document != m_mcpDocumentId) {
    throw std::runtime_error("No open document with this handle; call document_list after New or Open");
  }
  // Replacement is explicit for MCP; UI Open may create a new window.
  if (name == QStringLiteral("document_open")) {
    return mcpOpen(arguments);
  }
  // What the window shows: no document handle.
  if (name == QStringLiteral("ui_state")) {
    return mcpUiState();
  }
  if (name == QStringLiteral("ui_log")) {
    return mcpLog(arguments);
  }
  if (name == QStringLiteral("ui_screenshot")) {
    return mcpScreenshot(arguments);
  }
  if (name == QStringLiteral("ui_camera")) {
    return mcpCamera(arguments);
  }
  if (name == QStringLiteral("model_query")) {
    return mcpQuery(arguments.value(QStringLiteral("query")).toObject());
  }
  if (name == QStringLiteral("model_command")) {
    return mcpCommand(arguments);
  }
  if (name == QStringLiteral("model_export")) {
    return mcpExport(arguments);
  }
  // A picture of the design, not of the view (mitcad#1487).
  if (name == QStringLiteral("model_render")) {
    return mcpRender(arguments);
  }
  if (name == QStringLiteral("document_save")) {
    return mcpSave(arguments);
  }
  if (name == QStringLiteral("ui_select")) {
    return mcpSelect(arguments);
  }
  if (name == QStringLiteral("ui_command")) {
    return mcpUiCommand(arguments);
  }
  // A new design beside this one (mitcad#1266).
  if (name == QStringLiteral("document_new")) {
    return mcpNew(arguments);
  }
  // Clicks, keys and dialogs for viewing (mitcad#1735), only in a run
  // started with --mcp-folder.
  if (name == QStringLiteral("ui_click")) {
    return mcpClick(arguments);
  }
  if (name == QStringLiteral("ui_type")) {
    return mcpType(arguments);
  }
  if (name == QStringLiteral("ui_dialog")) {
    return mcpDialog(arguments);
  }
  throw std::runtime_error("Unknown live document tool");
}

} // namespace mitcad
